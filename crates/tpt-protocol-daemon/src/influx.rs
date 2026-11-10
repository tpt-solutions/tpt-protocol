//! InfluxDB sink: writes each TUP point as one line of InfluxDB line protocol and
//! POSTs the lines in batches to a `/write` endpoint (InfluxDB 1.x style, or the
//! v2 compatibility endpoint with `db=` or `bucket=` in the query).
//!
//! - Measurement: the source protocol. Tags: `device`, `point`, `quality`, and
//!   `unit` when the point has one. Field: `value`. Timestamp: the point's
//!   milliseconds, sent with `precision=ms`; a point with timestamp 0 (not yet
//!   stamped) is written without one, so the server assigns its own.
//! - Plain HTTP only (see `http_out`).
//! - Batches are flushed every second or at 500 lines. A batch the server
//!   refuses, or cannot be delivered within five seconds, is logged and dropped;
//!   there is no retry queue, so a slow database cannot grow the daemon's memory.
//! - Points whose value line protocol cannot carry (non-finite floats, bytes)
//!   are skipped.

use std::time::Duration;

use anyhow::Result;
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;
use tpt_protocol_tup::{DataPoint, TupValue};

use crate::http_out::{self, Target};

const MAX_BATCH: usize = 500;
const FLUSH_EVERY: Duration = Duration::from_secs(1);
const CONTENT_TYPE: &str = "text/plain; charset=utf-8";

/// Parses `http://HOST[:PORT]/PATH?QUERY` for `--influx`. `precision=ms` is added
/// to the query unless the URL already sets a precision.
pub fn parse_url(url: &str) -> Result<Target> {
    let mut target = http_out::parse_http_url(url, "--influx")?;
    if !target.path.contains("precision=") {
        target
            .path
            .push(if target.path.contains('?') { '&' } else { '?' });
        target.path.push_str("precision=ms");
    }
    Ok(target)
}

/// Escapes a measurement name or a tag key or value: commas, equals signs and
/// spaces are backslash-escaped, and control characters become `_`.
fn escape_key(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            ',' | '=' | ' ' => {
                out.push('\\');
                out.push(c);
            }
            c if c.is_control() => out.push('_'),
            c => out.push(c),
        }
    }
    if out.is_empty() {
        out.push('_');
    }
    out
}

/// Escapes a string field value (between double quotes).
fn escape_string_field(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '"' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            '\n' | '\r' => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

/// The line protocol line for a point, or `None` when the value cannot be
/// written (a non-finite float, or raw bytes).
pub fn line(dp: &DataPoint) -> Option<String> {
    let field = match &dp.value {
        TupValue::Int(v) => format!("{v}i"),
        TupValue::Float(v) if v.is_finite() => format!("{v}"),
        TupValue::Float(_) => return None,
        TupValue::Bool(v) => v.to_string(),
        TupValue::Text(v) => format!("\"{}\"", escape_string_field(v)),
        TupValue::Bytes(_) => return None,
    };

    let mut out = escape_key(dp.source_protocol.as_str());
    out.push_str(",device=");
    out.push_str(&escape_key(&dp.device_id));
    out.push_str(",point=");
    out.push_str(&escape_key(&dp.point_id));
    out.push_str(",quality=");
    out.push_str(dp.quality.as_str());
    if let Some(unit) = &dp.unit {
        out.push_str(",unit=");
        out.push_str(&escape_key(unit));
    }
    out.push_str(" value=");
    out.push_str(&field);
    if dp.timestamp != 0 {
        out.push(' ');
        out.push_str(&dp.timestamp.to_string());
    }
    Some(out)
}

async fn flush(target: &Target, batch: &mut Vec<String>) {
    if batch.is_empty() {
        return;
    }
    let count = batch.len();
    let body = batch.join("\n");
    batch.clear();
    if let Err(e) = http_out::post(target, CONTENT_TYPE, &body).await {
        tracing::warn!("influx: {count} points not written: {e}");
    }
}

/// Writes every point from `rx` until the channel closes, flushing what is left.
pub async fn run(target: Target, mut rx: broadcast::Receiver<DataPoint>) {
    tracing::info!(
        "influx: writing to {}:{}{}",
        target.host,
        target.port,
        target.path
    );
    let mut batch: Vec<String> = Vec::new();
    let mut tick = tokio::time::interval(FLUSH_EVERY);
    loop {
        tokio::select! {
            received = rx.recv() => match received {
                Ok(dp) => {
                    if let Some(text) = line(&dp) {
                        batch.push(text);
                    }
                    if batch.len() >= MAX_BATCH {
                        flush(&target, &mut batch).await;
                    }
                }
                Err(RecvError::Lagged(n)) => {
                    tracing::warn!("influx: fell behind and skipped {n} points");
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
    fn urls_are_parsed_and_precision_is_added() {
        let t = parse_url("http://127.0.0.1:8086/write?db=tpt").unwrap();
        assert_eq!(t.path, "/write?db=tpt&precision=ms");

        let t = parse_url("http://influx.local/api/v2/write?bucket=b&precision=s").unwrap();
        assert_eq!(t.port, 80);
        assert_eq!(t.path, "/api/v2/write?bucket=b&precision=s");

        let t = parse_url("http://[::1]:8086/write").unwrap();
        assert_eq!(t.host, "::1");
        assert_eq!(t.path, "/write?precision=ms");

        assert!(parse_url("https://example.com/write").is_err());
    }

    #[test]
    fn values_are_written_with_their_line_protocol_types() {
        assert_eq!(
            line(&point(TupValue::Int(-7))).unwrap(),
            "modbus,device=plc-1,point=holding_register[1],quality=good value=-7i 1753900000000"
        );
        assert_eq!(
            line(&point(TupValue::Float(1.5))).unwrap(),
            "modbus,device=plc-1,point=holding_register[1],quality=good value=1.5 1753900000000"
        );
        assert_eq!(
            line(&point(TupValue::Bool(true))).unwrap(),
            "modbus,device=plc-1,point=holding_register[1],quality=good value=true 1753900000000"
        );
        assert_eq!(
            line(&point(TupValue::Text("say \"hi\"\\".into()))).unwrap(),
            "modbus,device=plc-1,point=holding_register[1],quality=good value=\"say \\\"hi\\\"\\\\\" 1753900000000"
        );
    }

    #[test]
    fn values_line_protocol_cannot_carry_are_skipped() {
        assert!(line(&point(TupValue::Float(f64::NAN))).is_none());
        assert!(line(&point(TupValue::Float(f64::INFINITY))).is_none());
        assert!(line(&point(TupValue::Bytes(vec![1, 2]))).is_none());
    }

    #[test]
    fn unstamped_points_have_no_timestamp_and_tags_cannot_split_the_line() {
        let mut dp = DataPoint::new(
            SourceProtocol::Modbus,
            "plc, one=two",
            "tank level",
            0,
            TupValue::Int(1),
            Quality::Stale,
        );
        dp.unit = Some("deg C".into());
        let text = line(&dp).unwrap();
        assert_eq!(
            text,
            "modbus,device=plc\\,\\ one\\=two,point=tank\\ level,quality=stale,unit=deg\\ C value=1i"
        );
        assert!(!text.contains('\n'));
    }

    #[tokio::test]
    async fn run_flushes_what_is_left_when_the_channel_closes() {
        let (target, server) = test_server::serve_once("204 No Content", "/write").await;
        let (tx, rx) = broadcast::channel::<DataPoint>(16);
        let task = tokio::spawn(run(target, rx));
        tx.send(point(TupValue::Int(1))).unwrap();
        tx.send(point(TupValue::Int(2))).unwrap();
        drop(tx);
        task.await.unwrap();
        let request = server.await.unwrap();
        assert!(request.contains("value=1i"), "{request}");
        assert!(request.contains("value=2i"), "{request}");
    }
}
