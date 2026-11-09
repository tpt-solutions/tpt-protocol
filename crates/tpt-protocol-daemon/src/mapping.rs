//! Declarative mapping rules, read from the `[[map]]` tables of the config file.
//!
//! A rule matches data points by protocol, device and point id (each optional,
//! all must match when given). A matching rule can rename the point, set its
//! unit, scale and offset its numeric value, and check a range. A value outside
//! the range is kept but marked `Uncertain`, so the TUI and clients can see it.
//!
//! The first rule that matches a point is the one applied.
//!
//! ```toml
//! [[map]]
//! protocol = "modbus"
//! device = "plc-1"
//! point = "modbus-point-1"
//! name = "tank_level"
//! unit = "m"
//! scale = 0.001        # value = raw * scale + offset
//! min = 0.0            # outside [min, max] the quality becomes "uncertain"
//! max = 5.0
//! ```

use serde::Deserialize;
use tpt_protocol_tup::{DataPoint, Quality, TupValue};

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MapRule {
    /// Source protocol name, as `--list-protocols` prints it.
    pub protocol: Option<String>,
    /// Exact device id to match.
    pub device: Option<String>,
    /// Exact point id to match.
    pub point: Option<String>,
    /// Replacement point id.
    pub name: Option<String>,
    /// Unit to set on the point.
    pub unit: Option<String>,
    /// Multiplier for numeric values. Defaults to 1.
    #[serde(default = "one")]
    pub scale: f64,
    /// Added after scaling. Defaults to 0.
    #[serde(default)]
    pub offset: f64,
    /// Lowest acceptable scaled value.
    pub min: Option<f64>,
    /// Highest acceptable scaled value.
    pub max: Option<f64>,
}

fn one() -> f64 {
    1.0
}

impl MapRule {
    /// Checks the rule's own fields for mistakes that would otherwise fail silently.
    pub fn validate(&self) -> Result<(), String> {
        if !self.scale.is_finite() || !self.offset.is_finite() {
            return Err("scale and offset must be finite numbers".into());
        }
        if let (Some(min), Some(max)) = (self.min, self.max) {
            if min > max {
                return Err(format!("min ({min}) is greater than max ({max})"));
            }
        }
        if let Some(protocol) = &self.protocol {
            if tpt_protocol_tup::SourceProtocol::from_str_opt(protocol).is_none() {
                return Err(format!("unknown protocol '{protocol}' in [[map]]"));
            }
        }
        Ok(())
    }

    fn matches(&self, dp: &DataPoint) -> bool {
        self.protocol
            .as_deref()
            .is_none_or(|p| p == dp.source_protocol.as_str())
            && self.device.as_deref().is_none_or(|d| d == dp.device_id)
            && self.point.as_deref().is_none_or(|p| p == dp.point_id)
    }

    fn apply(&self, mut dp: DataPoint) -> DataPoint {
        let numeric = match dp.value {
            TupValue::Int(n) => Some(n as f64),
            TupValue::Float(x) => Some(x),
            _ => None,
        };
        if let Some(x) = numeric {
            let scaled = x * self.scale + self.offset;
            let identity = self.scale == 1.0 && self.offset == 0.0;
            if !identity {
                dp.value = TupValue::Float(scaled);
            }
            let below = self.min.is_some_and(|min| scaled < min);
            let above = self.max.is_some_and(|max| scaled > max);
            if (below || above) && dp.quality == Quality::Good {
                dp.quality = Quality::Uncertain;
            }
        }
        if let Some(name) = &self.name {
            dp.point_id = name.clone();
        }
        if let Some(unit) = &self.unit {
            dp.unit = Some(unit.clone());
        }
        dp
    }
}

/// Applies the first matching rule to `dp`. Points that match no rule pass through unchanged.
pub fn apply(rules: &[MapRule], dp: DataPoint) -> DataPoint {
    match rules.iter().find(|rule| rule.matches(&dp)) {
        Some(rule) => rule.apply(dp),
        None => dp,
    }
}

/// The mapping stage between the devices and the clients: every point from
/// `rx` is mapped, checked against the anomaly rules, and sent on `tx`, until
/// either side closes. The anomaly check runs after mapping, so it sees the
/// point ids the clients see.
pub async fn run(
    rules: Vec<MapRule>,
    anomaly_rules: Vec<crate::anomaly::AnomalyRule>,
    mut rx: tokio::sync::broadcast::Receiver<DataPoint>,
    tx: tokio::sync::broadcast::Sender<DataPoint>,
) {
    use tokio::sync::broadcast::error::RecvError;
    let mut detector = crate::anomaly::Detector::new(anomaly_rules);
    loop {
        match rx.recv().await {
            Ok(dp) => {
                let mut dp = apply(&rules, dp);
                if let Some(reason) = detector.check(&mut dp) {
                    tracing::debug!("{} {}: {reason}", dp.device_id, dp.point_id);
                }
                let _ = tx.send(dp);
            }
            Err(RecvError::Lagged(n)) => {
                tracing::warn!("mapping stage fell behind and dropped {n} points");
            }
            Err(RecvError::Closed) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_protocol_tup::SourceProtocol;

    fn point(value: TupValue) -> DataPoint {
        DataPoint::new(
            SourceProtocol::Modbus,
            "plc-1",
            "modbus-point-1",
            1,
            value,
            Quality::Good,
        )
    }

    fn rule(toml_text: &str) -> MapRule {
        toml::from_str(toml_text).unwrap()
    }

    #[test]
    fn rule_renames_scales_and_sets_unit() {
        let r = rule(
            r#"
            protocol = "modbus"
            device = "plc-1"
            point = "modbus-point-1"
            name = "tank_level"
            unit = "m"
            scale = 0.001
            "#,
        );
        let out = apply(&[r], point(TupValue::Int(2500)));
        assert_eq!(out.point_id, "tank_level");
        assert_eq!(out.unit.as_deref(), Some("m"));
        assert_eq!(out.value, TupValue::Float(2.5));
        assert_eq!(out.quality, Quality::Good);
    }

    #[test]
    fn out_of_range_becomes_uncertain_and_keeps_the_value() {
        let r = rule("min = 0.0\nmax = 5.0");
        let high = apply(std::slice::from_ref(&r), point(TupValue::Float(7.0)));
        assert_eq!(high.quality, Quality::Uncertain);
        assert_eq!(high.value, TupValue::Float(7.0));
        let inside = apply(std::slice::from_ref(&r), point(TupValue::Float(4.0)));
        assert_eq!(inside.quality, Quality::Good);
    }

    #[test]
    fn non_numeric_values_pass_through_untouched() {
        let r = rule("scale = 2.0");
        let text = apply(&[r], point(TupValue::Text("ok".into())));
        assert_eq!(text.value, TupValue::Text("ok".into()));
    }

    #[test]
    fn only_matching_points_are_changed() {
        let r = rule("device = \"other\"\nname = \"renamed\"");
        let out = apply(&[r], point(TupValue::Int(1)));
        assert_eq!(out.point_id, "modbus-point-1");
    }

    #[test]
    fn first_matching_rule_wins() {
        let rules = [rule("name = \"first\""), rule("name = \"second\"")];
        assert_eq!(apply(&rules, point(TupValue::Int(1))).point_id, "first");
    }

    #[test]
    fn validation_catches_mistakes() {
        assert!(rule("min = 5.0\nmax = 1.0").validate().is_err());
        assert!(rule("protocol = \"nope\"").validate().is_err());
        assert!(rule("name = \"ok\"").validate().is_ok());
    }

    #[test]
    fn unknown_keys_are_rejected() {
        assert!(toml::from_str::<MapRule>("nmae = \"typo\"").is_err());
    }
}
