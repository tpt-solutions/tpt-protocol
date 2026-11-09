//! Anomaly rules on the TUP stream: stateful checks that mark a point `Uncertain`
//! when it looks replayed, arrives too fast, or jumps by more than a set amount.
//!
//! Rules are read from the `[[anomaly]]` tables of the config file. Each rule
//! matches points by protocol, device and point id, as `[[map]]` rules do. The
//! checks keep the last accepted timestamp and value of each series (one series
//! per protocol, device and point). A flagged point keeps its value; only its
//! quality changes, so clients can still see what arrived.
//!
//! ```toml
//! [[anomaly]]
//! device = "plc-1"
//! order = true          # a timestamp that does not advance is a replay or a clock step
//! min_interval_ms = 50  # two points of one series closer than this are a rate spike
//! max_step = 100.0      # a value that moves more than this from the last one is a step spike
//! ```
//!
//! Not covered: function-code rules. A TUP point does not carry the Modbus or other
//! function code it came from, so those rules need frame metadata the plugins
//! would have to pass on.

use std::collections::HashMap;

use serde::Deserialize;
use tpt_protocol_tup::{DataPoint, Quality, TupValue};

/// Most series tracked at once. A new series past this limit is not checked, so a
/// flood of distinct point ids cannot grow the table without bound.
const MAX_SERIES: usize = 10_000;

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AnomalyRule {
    /// Source protocol name, as `--list-protocols` prints it.
    pub protocol: Option<String>,
    /// Exact device id to match.
    pub device: Option<String>,
    /// Exact point id to match.
    pub point: Option<String>,
    /// Flag a timestamp that does not advance past the last accepted one.
    #[serde(default)]
    pub order: bool,
    /// Flag a point that arrives less than this many milliseconds after the last.
    pub min_interval_ms: Option<u64>,
    /// Flag a numeric value that differs from the last accepted one by more than this.
    pub max_step: Option<f64>,
}

impl AnomalyRule {
    /// Checks that the rule has at least one check and sensible numbers.
    pub fn validate(&self) -> Result<(), String> {
        if !self.order && self.min_interval_ms.is_none() && self.max_step.is_none() {
            return Err(
                "an [[anomaly]] rule needs at least one check: order, min_interval_ms or max_step"
                    .into(),
            );
        }
        if let Some(step) = self.max_step {
            if !(step.is_finite() && step > 0.0) {
                return Err(format!("max_step must be a positive number, got {step}"));
            }
        }
        if let Some(protocol) = &self.protocol {
            if tpt_protocol_tup::SourceProtocol::from_str_opt(protocol).is_none() {
                return Err(format!("unknown protocol '{protocol}' in [[anomaly]]"));
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
}

/// The last accepted point of one series.
#[derive(Debug, Clone, Copy)]
struct Last {
    timestamp: u64,
    value: Option<f64>,
}

/// Checks points against the anomaly rules, remembering each series' last point.
#[derive(Debug, Default)]
pub struct Detector {
    rules: Vec<AnomalyRule>,
    last: HashMap<(String, String, String), Last>,
}

impl Detector {
    pub fn new(rules: Vec<AnomalyRule>) -> Self {
        Self {
            rules,
            last: HashMap::new(),
        }
    }

    /// Checks one point. A flagged point's quality becomes `Uncertain`; a point
    /// already marked `Bad` or `Stale` keeps its quality. Returns the reason, if any.
    pub fn check(&mut self, dp: &mut DataPoint) -> Option<&'static str> {
        let rule = self.rules.iter().find(|rule| rule.matches(dp))?;
        let key = (
            dp.source_protocol.as_str().to_string(),
            dp.device_id.clone(),
            dp.point_id.clone(),
        );
        let value = match dp.value {
            TupValue::Int(n) => Some(n as f64),
            TupValue::Float(x) => Some(x),
            _ => None,
        };

        let reason = self.last.get(&key).and_then(|prev| {
            if rule.order && dp.timestamp <= prev.timestamp {
                return Some("timestamp does not advance (replay or clock step)");
            }
            if rule
                .min_interval_ms
                .is_some_and(|min| dp.timestamp.saturating_sub(prev.timestamp) < min)
            {
                return Some("points arrive faster than min_interval_ms (rate spike)");
            }
            match (rule.max_step, value, prev.value) {
                (Some(max), Some(now), Some(before)) if (now - before).abs() > max => {
                    Some("value moved by more than max_step (step spike)")
                }
                _ => None,
            }
        });

        // Only a point that moves the series forward becomes the new reference, so a
        // replayed point cannot make later, correct points look like they went back.
        let advances = self
            .last
            .get(&key)
            .is_none_or(|prev| dp.timestamp > prev.timestamp);
        let tracked = self.last.len() < MAX_SERIES || self.last.contains_key(&key);
        if reason.is_none() && advances && tracked {
            self.last.insert(
                key,
                Last {
                    timestamp: dp.timestamp,
                    value,
                },
            );
        }

        if reason.is_some() && dp.quality == Quality::Good {
            dp.quality = Quality::Uncertain;
        }
        reason
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_protocol_tup::SourceProtocol;

    fn point(device: &str, ts: u64, value: TupValue) -> DataPoint {
        DataPoint::new(
            SourceProtocol::Modbus,
            device,
            "reg",
            ts,
            value,
            Quality::Good,
        )
    }

    fn rule(text: &str) -> AnomalyRule {
        toml::from_str(text).unwrap()
    }

    #[test]
    fn a_timestamp_that_does_not_advance_is_flagged() {
        let mut d = Detector::new(vec![rule("order = true")]);
        assert!(d.check(&mut point("plc", 100, TupValue::Int(1))).is_none());
        let mut replayed = point("plc", 100, TupValue::Int(1));
        assert!(d.check(&mut replayed).is_some());
        assert_eq!(replayed.quality, Quality::Uncertain);
        assert_eq!(replayed.value, TupValue::Int(1), "the value is kept");
        // A later, correct point is not blamed for the replay.
        let mut next = point("plc", 200, TupValue::Int(1));
        assert!(d.check(&mut next).is_none());
        assert_eq!(next.quality, Quality::Good);
    }

    #[test]
    fn points_faster_than_the_minimum_interval_are_a_rate_spike() {
        let mut d = Detector::new(vec![rule("min_interval_ms = 50")]);
        d.check(&mut point("plc", 1_000, TupValue::Int(1)));
        let mut fast = point("plc", 1_020, TupValue::Int(1));
        assert!(d.check(&mut fast).is_some());
        assert_eq!(fast.quality, Quality::Uncertain);
        assert!(d
            .check(&mut point("plc", 1_200, TupValue::Int(1)))
            .is_none());
    }

    #[test]
    fn a_large_step_is_flagged_and_a_small_one_is_not() {
        let mut d = Detector::new(vec![rule("max_step = 10.0")]);
        d.check(&mut point("plc", 1, TupValue::Float(20.0)));
        assert!(d
            .check(&mut point("plc", 2, TupValue::Float(25.0)))
            .is_none());
        let mut jump = point("plc", 3, TupValue::Float(90.0));
        assert!(d.check(&mut jump).is_some());
        assert_eq!(jump.quality, Quality::Uncertain);
    }

    #[test]
    fn only_matching_series_are_checked() {
        let mut d = Detector::new(vec![rule("device = \"plc-1\"\norder = true")]);
        d.check(&mut point("plc-2", 100, TupValue::Int(1)));
        assert!(d.check(&mut point("plc-2", 50, TupValue::Int(1))).is_none());
    }

    #[test]
    fn a_bad_point_keeps_its_quality() {
        let mut d = Detector::new(vec![rule("order = true")]);
        d.check(&mut point("plc", 100, TupValue::Int(1)));
        let mut bad = DataPoint::new(
            SourceProtocol::Modbus,
            "plc",
            "reg",
            50,
            TupValue::Int(1),
            Quality::Bad,
        );
        d.check(&mut bad);
        assert_eq!(bad.quality, Quality::Bad);
    }

    #[test]
    fn a_rule_needs_a_check_and_a_positive_step() {
        assert!(rule("device = \"plc\"").validate().is_err());
        assert!(rule("max_step = 0.0\norder = true").validate().is_err());
        assert!(rule("order = true").validate().is_ok());
        assert!(rule("protocol = \"nope\"\norder = true")
            .validate()
            .is_err());
    }

    #[test]
    fn series_table_is_bounded() {
        let mut d = Detector::new(vec![rule("order = true")]);
        for i in 0..(MAX_SERIES + 50) {
            d.check(&mut point(&format!("dev-{i}"), 1, TupValue::Int(0)));
        }
        assert_eq!(d.last.len(), MAX_SERIES);
    }
}
