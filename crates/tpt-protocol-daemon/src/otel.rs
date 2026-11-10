//! OpenTelemetry sink: sends TUP points as OTLP/HTTP JSON metrics to a collector.
//!
//! - One gauge metric per source protocol, named `tpt.<protocol>`. Each point is a
//!   data point of that gauge, with the attributes `device`, `point`, `quality`
//!   and `unit` (when set). Integers use `asInt`, which OTLP/JSON writes as a
//!   string so that 64-bit values survive; floats and booleans (as 1 or 0) use
//!   `asDouble`. Text and bytes are not numbers, so they are skipped.
//! - The timestamp is the point's time in nanoseconds. A point with timestamp 0
//!   (not yet stamped) is sent with the time of the batch.
//! - Plain HTTP only (see `http_out`). The default path is `/v1/metrics`.
//! - Batches are flushed every second or at 500 points. A batch the collector
//!   refuses is logged and dropped, with no retry queue.

use std::collections::BTreeMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde_json::{json, Value};
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;
use tpt_protocol_tup::{DataPoint, TupValue};

use crate::http_out::{self, Target};

const MAX_BATCH: usize = 500;
const FLUSH_EVERY: Duration = Duration::from_secs(1);
const SERVICE_NAME: &str = "tpt-protocol-daemon";
const SCOPE_NAME: &str = "tpt-protocol";
const DEFAULT_PATH: &str = "/v1/metrics";

/// Parses `http://HOST[:PORT][/PATH]` for `--otel`. The path defaults to `/v1/metrics`.
pub fn parse_url(url: &str) -> Result<Target> {
    let mut target = http_out::parse_http_url(url, "--otel")?;
    if target.path == "/" {
        target.path = DEFAULT_PATH.to_string();
    }
    Ok(target)
}

fn attribute(key: &str, value: &str) -> Value {
    json!({"key": key, "value": {"stringValue": value}})
}

fn now_nanos() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

/// One gauge data point for a point, or `None` for values that are not numbers.
fn gauge_point(dp: &DataPoint) -> Option<Value> {
    let number = match &dp.value {
        TupValue::Int(v) => json!({ "asInt": v.to_string() }),
        TupValue::Float(v) if v.is_finite() => json!({ "asDouble": v }),
        TupValue::Float(_) => return None,
        TupValue::Bool(b) => json!({ "asDouble": if *b { 1.0 } else { 0.0 } }),
        TupValue::Text(_) | TupValue::Bytes(_) => return None,
    };
    let mut attributes = vec![
        attribute("device", &dp.device_id),
        attribute("point", &dp.point_id),
        attribute("quality", dp.quality.as_str()),
    ];
    if let Some(unit) = &dp.unit {
        attributes.push(attribute("unit", unit));
    }
    let time = if dp.timestamp == 0 {
        now_nanos()
    } else {
        dp.timestamp.saturating_mul(1_000_000)
    };

    let mut point = json!({
        "timeUnixNano": time.to_string(),
        "attributes": attributes,
    });
    if let (Value::Object(target), Value::Object(value)) = (&mut point, number) {
        target.extend(value);
    }
    Some(point)
}

/// The OTLP/JSON body for a batch of points, or `None` when no point is a number.
pub fn metrics_body(points: &[DataPoint]) -> Option<String> {
    let mut by_protocol: BTreeMap<&str, Vec<Value>> = BTreeMap::new();
    for dp in points {
        if let Some(point) = gauge_point(dp) {
            by_protocol
                .entry(dp.source_protocol.as_str())
                .or_default()
                .push(point);
        }
    }
    if by_protocol.is_empty() {
        return None;
    }
    let metrics: Vec<Value> = by_protocol
        .into_iter()
        .map(|(protocol, data_points)| {
            json!({
                "name": format!("tpt.{protocol}"),
                "gauge": { "dataPoints": data_points },
            })
        })
        .collect();
    Some(
        json!({
            "resourceMetrics": [{
                "resource": { "attributes": [attribute("service.name", SERVICE_NAME)] },
                "scopeMetrics": [{
                    "scope": { "name": SCOPE_NAME },
                    "metrics": metrics,
                }],
            }],
        })
        .to_string(),
    )
}

async fn flush(target: &Target, batch: &mut Vec<DataPoint>) {
    if batch.is_empty() {
        return;
    }
    let count = batch.len();
    let body = metrics_body(batch);
    batch.clear();
    let Some(body) = body else {
        return;
    };
    if let Err(e) = http_out::post(target, "application/json", &body).await {
        tracing::warn!("otel: {count} points not sent: {e}");
    }
}

/// Sends every point from `rx` until the channel closes, flushing what is left.
pub async fn run(target: Target, mut rx: broadcast::Receiver<DataPoint>) {
    tracing::info!(
        "otel: sending metrics to {}:{}{}",
        target.host,
        target.port,
        target.path
    );
    let mut batch: Vec<DataPoint> = Vec::new();
    let mut tick = tokio::time::interval(FLUSH_EVERY);
    loop {
        tokio::select! {
            received = rx.recv() => match received {
                Ok(dp) => {
                    batch.push(dp);
                    if batch.len() >= MAX_BATCH {
                        flush(&target, &mut batch).await;
                    }
                }
                Err(RecvError::Lagged(n)) => {
                    tracing::warn!("otel: fell behind and skipped {n} points");
                }
                Err(RecvError::Closed) => {
                    flush(&target, &mut batch).await;
                    return;
                }
            },
            _ = tick.tick() => flush(&target, &mut batch).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http_out::test_server;
    use tpt_protocol_tup::{Quality, SourceProtocol};

    fn point(value: TupValue) -> DataPoint {
        DataPoint::new(
            SourceProtocol::Modbus,
            "plc-1",
            "holding_register[1]",
            1_753_900_000_000,
            value,
            Quality::Good,
        )
    }

    #[test]
    fn default_path_is_the_otlp_metrics_endpoint() {
        assert_eq!(
            parse_url("http://collector:4318").unwrap().path,
            "/v1/metrics"
        );
        assert_eq!(
            parse_url("http://collector:4318/custom").unwrap().path,
            "/custom"
        );
    }

    #[test]
    fn numbers_become_gauge_points_and_text_is_skipped() {
        let mut celsius = point(TupValue::Float(21.5));
        celsius.unit = Some("Cel".into());
        let points = [
            point(TupValue::Int(5)),
            celsius,
            point(TupValue::Text("ignored".into())),
            point(TupValue::Bool(true)),
        ];
        let body: Value = serde_json::from_str(&metrics_body(&points).unwrap()).unwrap();

        let scope = &body["resourceMetrics"][0]["scopeMetrics"][0];
        assert_eq!(scope["scope"]["name"], SCOPE_NAME);
        let metric = &scope["metrics"][0];
        assert_eq!(metric["name"], "tpt.modbus");
        let data = metric["gauge"]["dataPoints"].as_array().unwrap();
        assert_eq!(data.len(), 3, "the text point is skipped");

        assert_eq!(data[0]["asInt"], "5");
        assert_eq!(data[0]["timeUnixNano"], "1753900000000000000");
        assert_eq!(data[0]["attributes"][0]["key"], "device");
        assert_eq!(data[0]["attributes"][0]["value"]["stringValue"], "plc-1");

        assert_eq!(data[1]["asDouble"], 21.5);
        assert_eq!(data[1]["attributes"][3]["key"], "unit");
        assert_eq!(data[2]["asDouble"], 1.0);

        assert_eq!(
            body["resourceMetrics"][0]["resource"]["attributes"][0]["value"]["stringValue"],
            SERVICE_NAME
        );
    }

    #[test]
    fn a_batch_with_no_numbers_sends_nothing() {
        let points = [
            point(TupValue::Text("a".into())),
            point(TupValue::Float(f64::NAN)),
        ];
        assert!(metrics_body(&points).is_none());
    }

    #[tokio::test]
    async fn run_sends_the_batch_as_json_when_the_channel_closes() {
        let (target, server) = test_server::serve_once("200 OK", "/v1/metrics").await;
        let (tx, rx) = broadcast::channel::<DataPoint>(16);
        let task = tokio::spawn(run(target, rx));
        tx.send(point(TupValue::Int(7))).unwrap();
        drop(tx);
        task.await.unwrap();
        let request = server.await.unwrap();
        assert!(
            request.starts_with("POST /v1/metrics HTTP/1.1\r\n"),
            "{request}"
        );
        assert!(
            request.contains("Content-Type: application/json\r\n"),
            "{request}"
        );
        assert!(request.contains("\"asInt\":\"7\""), "{request}");
    }
}
