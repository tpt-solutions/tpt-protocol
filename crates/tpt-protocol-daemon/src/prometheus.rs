//! Prometheus remote-write sink: sends numeric TUP points to a Prometheus-compatible
//! receiver (Prometheus with `--web.enable-remote-write-receiver`, Mimir, VictoriaMetrics,
//! and others) using the remote-write 0.1.0 protocol.
//!
//! - One series per point, named `tpt_value`, with the labels `device`, `point`,
//!   `protocol`, `quality` and `unit` (when set), in the sorted order the protocol
//!   requires. The sample is the value as a double, with the point's time in
//!   milliseconds, or the time of the batch when the point has no time (0).
//! - Integers are sent as doubles, so integers above 2^53 lose precision. Text and
//!   bytes are not numbers and are skipped; so are non-finite floats.
//! - The body is a protobuf `WriteRequest`, compressed with snappy. This module writes
//!   snappy in its literal-only block form, which every decoder reads, at a little
//!   more than the uncompressed size.
//! - Plain HTTP only (see `http_out`). The default path is `/api/v1/write`.
//! - Batches are flushed every second or at 500 points. Prometheus rejects a sample
//!   older than one it already holds for the same series, so a batch the receiver
//!   refuses is logged and dropped, with no retry.

use std::time::Duration;

use anyhow::Result;
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;
use tpt_protocol_tup::{DataPoint, TupValue};

use crate::http_out::{self, Target};
use crate::plugins::now_ms;

const MAX_BATCH: usize = 500;
const FLUSH_EVERY: Duration = Duration::from_secs(1);
const DEFAULT_PATH: &str = "/api/v1/write";
const METRIC_NAME: &str = "tpt_value";
/// Snappy literal elements carry at most this many bytes each.
const SNAPPY_LITERAL_MAX: usize = 60;

/// Parses `http://HOST[:PORT][/PATH]` for `--prometheus-remote-write`. The path defaults
/// to `/api/v1/write`.
pub fn parse_url(url: &str) -> Result<Target> {
    let mut target = http_out::parse_http_url(url, "--prometheus-remote-write")?;
    if target.path == "/" {
        target.path = DEFAULT_PATH.to_string();
    }
    Ok(target)
}

fn put_varint(mut value: u64, out: &mut Vec<u8>) {
    loop {
        let byte = (value & 0x7F) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

fn put_key(field: u32, wire: u8, out: &mut Vec<u8>) {
    put_varint((u64::from(field) << 3) | u64::from(wire), out);
}

fn put_bytes(field: u32, bytes: &[u8], out: &mut Vec<u8>) {
    put_key(field, 2, out);
    put_varint(bytes.len() as u64, out);
    out.extend_from_slice(bytes);
}

/// One `Label` message: a name and a value.
fn label(name: &str, value: &str) -> Vec<u8> {
    let mut out = Vec::new();
    put_bytes(1, name.as_bytes(), &mut out);
    put_bytes(2, value.as_bytes(), &mut out);
    out
}

/// One `Sample` message: a double value (fixed64) and a timestamp in milliseconds.
fn sample(value: f64, timestamp_ms: i64) -> Vec<u8> {
    let mut out = Vec::new();
    put_key(1, 1, &mut out);
    out.extend_from_slice(&value.to_le_bytes());
    put_key(2, 0, &mut out);
    put_varint(timestamp_ms as u64, &mut out);
    out
}

/// The numeric value of a point, or `None` for values that are not numbers.
fn numeric(value: &TupValue) -> Option<f64> {
    match value {
        TupValue::Int(v) => Some(*v as f64),
        TupValue::Float(v) if v.is_finite() => Some(*v),
        TupValue::Float(_) => None,
        TupValue::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        TupValue::Text(_) | TupValue::Bytes(_) => None,
    }
}

/// One `TimeSeries` message for a point, or `None` for a value that is not a number.
fn series(dp: &DataPoint, now_ms: i64) -> Option<Vec<u8>> {
    let value = numeric(&dp.value)?;
    let timestamp = if dp.timestamp == 0 {
        now_ms
    } else {
        i64::try_from(dp.timestamp).unwrap_or(i64::MAX)
    };
    // Sorted by name, as the remote-write protocol requires.
    let mut labels: Vec<(&str, &str)> = vec![
        ("__name__", METRIC_NAME),
        ("device", &dp.device_id),
        ("point", &dp.point_id),
        ("protocol", dp.source_protocol.as_str()),
        ("quality", dp.quality.as_str()),
    ];
    if let Some(unit) = &dp.unit {
        labels.push(("unit", unit));
    }
    let mut out = Vec::new();
    for (name, value) in labels {
        put_bytes(1, &label(name, value), &mut out);
    }
    put_bytes(2, &sample(value, timestamp), &mut out);
    Some(out)
}

/// The protobuf `WriteRequest` for a batch (repeated `TimeSeries` in field 1), or
/// `None` when no point in the batch is a number.
pub fn write_request(points: &[DataPoint], now_ms: i64) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    for dp in points {
        if let Some(series) = series(dp, now_ms) {
            put_bytes(1, &series, &mut out);
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Snappy block format with literal elements only: the uncompressed length as a
/// varint, then the data in literal elements of up to 60 bytes, each tagged
/// `(length - 1) << 2`. Every snappy decoder reads this form.
pub fn snappy_literal(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + data.len() / SNAPPY_LITERAL_MAX + 8);
    put_varint(data.len() as u64, &mut out);
    for chunk in data.chunks(SNAPPY_LITERAL_MAX) {
        out.push(((chunk.len() - 1) << 2) as u8);
        out.extend_from_slice(chunk);
    }
    out
}

async fn flush(target: &Target, batch: &mut Vec<DataPoint>) {
    let request = write_request(batch, now_ms() as i64);
    let count = batch.len();
    batch.clear();
    let Some(request) = request else {
        return;
    };
    let body = snappy_literal(&request);
    let headers = [
        ("Content-Encoding", "snappy"),
        ("Content-Type", "application/x-protobuf"),
        ("X-Prometheus-Remote-Write-Version", "0.1.0"),
    ];
    if let Err(e) = http_out::post_bytes(target, &headers, &body).await {
        tracing::warn!("prometheus: {count} points not written: {e}");
    }
}

/// Writes every point from `rx` until the channel closes, flushing what is left.
pub async fn run(target: Target, mut rx: broadcast::Receiver<DataPoint>) {
    tracing::info!(
        "prometheus: remote write to {}:{}{}",
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
                    tracing::warn!("prometheus: fell behind and skipped {n} points");
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

    /// Decodes the literal-only snappy form written above. Fails on any other element.
    fn snappy_decode_literal(data: &[u8]) -> Vec<u8> {
        let mut pos = 0;
        let mut expected = 0usize;
        let mut shift = 0;
        loop {
            let byte = data[pos];
            pos += 1;
            expected |= usize::from(byte & 0x7F) << shift;
            shift += 7;
            if byte & 0x80 == 0 {
                break;
            }
        }
        let mut out = Vec::new();
        while pos < data.len() {
            let tag = data[pos];
            pos += 1;
            assert_eq!(tag & 3, 0, "only literal elements are written");
            let len = usize::from(tag >> 2) + 1;
            assert!(len <= SNAPPY_LITERAL_MAX);
            out.extend_from_slice(&data[pos..pos + len]);
            pos += len;
        }
        assert_eq!(out.len(), expected, "the length preamble matches");
        out
    }

    /// One protobuf field: number, wire type and payload (varints as their value).
    #[derive(Debug, PartialEq)]
    enum Value<'a> {
        Varint(u64),
        Fixed64(u64),
        Bytes(&'a [u8]),
    }

    fn read_varint(data: &[u8], pos: &mut usize) -> u64 {
        let mut value = 0u64;
        let mut shift = 0;
        loop {
            let byte = data[*pos];
            *pos += 1;
            value |= u64::from(byte & 0x7F) << shift;
            shift += 7;
            if byte & 0x80 == 0 {
                return value;
            }
        }
    }

    fn fields(data: &[u8]) -> Vec<(u32, Value<'_>)> {
        let mut pos = 0;
        let mut out = Vec::new();
        while pos < data.len() {
            let key = read_varint(data, &mut pos);
            let number = (key >> 3) as u32;
            let value = match key & 7 {
                0 => Value::Varint(read_varint(data, &mut pos)),
                1 => {
                    let v = u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
                    pos += 8;
                    Value::Fixed64(v)
                }
                2 => {
                    let len = read_varint(data, &mut pos) as usize;
                    let v = &data[pos..pos + len];
                    pos += len;
                    Value::Bytes(v)
                }
                wire => panic!("unexpected wire type {wire}"),
            };
            out.push((number, value));
        }
        out
    }

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
    fn snappy_literal_form_round_trips_at_any_size() {
        for len in [0usize, 1, 59, 60, 61, 1000] {
            let data: Vec<u8> = (0..len).map(|i| (i * 7) as u8).collect();
            assert_eq!(snappy_decode_literal(&snappy_literal(&data)), data);
        }
    }

    #[test]
    fn a_series_has_sorted_labels_and_one_sample() {
        let mut dp = point(TupValue::Float(21.5));
        dp.unit = Some("Cel".into());
        let request = write_request(&[dp], 0).unwrap();

        let timeseries = fields(&request);
        assert_eq!(timeseries.len(), 1);
        let Value::Bytes(series_bytes) = timeseries[0].1 else {
            panic!("a TimeSeries is a message")
        };
        let parts = fields(series_bytes);

        let names: Vec<String> = parts
            .iter()
            .filter(|(n, _)| *n == 1)
            .map(|(_, v)| {
                let Value::Bytes(label_bytes) = v else {
                    panic!()
                };
                let pair = fields(label_bytes);
                let Value::Bytes(name) = pair[0].1 else {
                    panic!()
                };
                String::from_utf8(name.to_vec()).unwrap()
            })
            .collect();
        assert_eq!(
            names,
            ["__name__", "device", "point", "protocol", "quality", "unit"]
        );

        let samples: Vec<&(u32, Value)> = parts.iter().filter(|(n, _)| *n == 2).collect();
        assert_eq!(samples.len(), 1);
        let Value::Bytes(sample_bytes) = samples[0].1 else {
            panic!()
        };
        let sample = fields(sample_bytes);
        assert_eq!(sample[0].1, Value::Fixed64(21.5f64.to_bits()));
        assert_eq!(sample[1].1, Value::Varint(1_753_900_000_000));
    }

    #[test]
    fn unstamped_points_take_the_batch_time_and_non_numbers_are_skipped() {
        let mut unstamped = point(TupValue::Int(3));
        unstamped.timestamp = 0;
        let points = [
            unstamped,
            point(TupValue::Text("skipped".into())),
            point(TupValue::Bytes(vec![1])),
            point(TupValue::Float(f64::NAN)),
        ];
        let request = write_request(&points, 42).unwrap();
        let timeseries = fields(&request);
        assert_eq!(timeseries.len(), 1, "only the integer point is a series");
        let Value::Bytes(series_bytes) = timeseries[0].1 else {
            panic!()
        };
        let samples: Vec<_> = fields(series_bytes)
            .into_iter()
            .filter(|(n, _)| *n == 2)
            .collect();
        let Value::Bytes(sample_bytes) = samples[0].1 else {
            panic!()
        };
        assert_eq!(fields(sample_bytes)[1].1, Value::Varint(42));
    }

    #[test]
    fn a_batch_with_no_numbers_sends_nothing() {
        assert!(write_request(&[point(TupValue::Text("x".into()))], 0).is_none());
    }

    #[tokio::test]
    async fn run_posts_snappy_protobuf_with_the_remote_write_headers() {
        let (target, server) = test_server::serve_once("204 No Content", "/api/v1/write").await;
        let (tx, rx) = broadcast::channel::<DataPoint>(16);
        let task = tokio::spawn(run(target, rx));
        tx.send(point(TupValue::Bool(true))).unwrap();
        drop(tx);
        task.await.unwrap();
        let request_text = server.await.unwrap();
        assert!(
            request_text.starts_with("POST /api/v1/write HTTP/1.1\r\n"),
            "{request_text}"
        );
        assert!(
            request_text.contains("Content-Encoding: snappy\r\n"),
            "{request_text}"
        );
        assert!(
            request_text.contains("X-Prometheus-Remote-Write-Version: 0.1.0\r\n"),
            "{request_text}"
        );
        assert!(
            request_text.contains("Content-Type: application/x-protobuf\r\n"),
            "{request_text}"
        );
    }
}

#[cfg(test)]
mod sample_for_reader {
    use super::*;
    use tpt_protocol_tup::{Quality, SourceProtocol};

    /// Writes one compressed request body for an independent reader (python-snappy and
    /// a protobuf reader). Run with `TPT_PROMETHEUS_SAMPLE=<path> cargo test -p
    /// tpt-protocol-daemon -- --ignored prometheus_sample`.
    #[test]
    #[ignore = "writes a sample file: set TPT_PROMETHEUS_SAMPLE"]
    fn prometheus_sample() {
        let Ok(path) = std::env::var("TPT_PROMETHEUS_SAMPLE") else {
            return;
        };
        let mut celsius = DataPoint::new(
            SourceProtocol::Modbus,
            "plc-1",
            "holding_register[7]",
            1_753_900_000_000,
            TupValue::Float(21.5),
            Quality::Good,
        );
        celsius.unit = Some("Cel".into());
        let counter = DataPoint::new(
            SourceProtocol::Modbus,
            "plc-1",
            "holding_register[8]",
            1_753_900_000_500,
            TupValue::Int(-42),
            Quality::Stale,
        );
        let request = write_request(&[celsius, counter], 0).unwrap();
        std::fs::write(path, snappy_literal(&request)).unwrap();
    }
}
