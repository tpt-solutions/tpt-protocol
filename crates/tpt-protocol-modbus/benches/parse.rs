//! Parser throughput for the Modbus TCP and RTU request paths.
//!
//! Compiled by the CI `bench` job; timings are not compared in CI because
//! shared runners are too noisy. Run locally with `cargo bench -p tpt-protocol-modbus`.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use tpt_protocol_core::FrameParser;
use tpt_protocol_modbus::ModbusParser;

fn bench_parse(c: &mut Criterion) {
    // MBAP header (tid 1, pid 0, length 6, unit 1) + FC 0x03 read 10 holding registers at 0.
    let tcp: [u8; 12] = [0x00, 0x01, 0x00, 0x00, 0x00, 0x06, 0x01, 0x03, 0x00, 0x00, 0x00, 0x0A];
    let parser = ModbusParser::new();
    c.bench_function("modbus_tcp_read_holding_request", |b| {
        b.iter(|| parser.parse(black_box(&tcp)))
    });
}

criterion_group!(benches, bench_parse);
criterion_main!(benches);
