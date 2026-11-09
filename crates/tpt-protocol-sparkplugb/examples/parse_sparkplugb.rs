//! Decode a Sparkplug B DDATA message (protobuf payload) on its topic and
//! normalize each metric into a TUP `DataPoint`.
//!
//! Run with: `cargo run -p tpt-protocol-sparkplugb --example parse_sparkplugb`

use tpt_protocol_sparkplugb::{decode, SparkplugNormalizer};

fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let byte = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

fn put_key(out: &mut Vec<u8>, field: u64, wire: u64) {
    put_varint(out, (field << 3) | wire);
}

fn put_varint_field(out: &mut Vec<u8>, field: u64, v: u64) {
    put_key(out, field, 0);
    put_varint(out, v);
}

fn put_bytes_field(out: &mut Vec<u8>, field: u64, data: &[u8]) {
    put_key(out, field, 2);
    put_varint(out, data.len() as u64);
    out.extend_from_slice(data);
}

/// A Metric: name (field 1), datatype (field 4), and a double value (field 13, fixed64).
fn double_metric(name: &str, value: f64) -> Vec<u8> {
    let mut m = Vec::new();
    put_bytes_field(&mut m, 1, name.as_bytes());
    put_varint_field(&mut m, 4, 10); // Double
    put_key(&mut m, 13, 1);
    m.extend_from_slice(&value.to_bits().to_le_bytes());
    m
}

/// A Payload: timestamp (field 1), metrics (field 2), sequence (field 3).
fn payload(metrics: &[Vec<u8>], seq: u64) -> Vec<u8> {
    let mut p = Vec::new();
    put_varint_field(&mut p, 1, 1_700_000_000_000);
    for m in metrics {
        put_bytes_field(&mut p, 2, m);
    }
    put_varint_field(&mut p, 3, seq);
    p
}

fn main() {
    let bytes = payload(
        &[
            double_metric("Temperature", 21.5),
            double_metric("Pressure", 101.3),
        ],
        7,
    );
    let topic = "spBv1.0/Plant1/DDATA/Edge7/Sensor3";
    println!("topic: {topic}");
    println!(
        "payload (hex): {}",
        bytes
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(" ")
    );

    let frame = decode(topic, &bytes).expect("a well-formed Sparkplug payload decodes");
    println!(
        "decoded: {} metric(s), seq {:?}",
        frame.metrics.len(),
        frame.seq
    );

    for dp in SparkplugNormalizer.points(&frame) {
        println!(
            "normalized: {} {} = {:?} ({})",
            dp.source_protocol, dp.point_id, dp.value, dp.quality
        );
    }
}
