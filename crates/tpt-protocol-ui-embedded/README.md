# tpt-protocol-ui-embedded

**Native embedded UI for the [TPT Protocol](https://github.com/tpt-protocol/tpt-protocol)
daemon, built with [Slint](https://slint.dev).**

A small, self-contained desktop/embedded dashboard that connects to the
daemon's local WebSocket (bound to `127.0.0.1` only) and renders a live,
high-contrast table of TUP `DataPoint`s streamed from any simulated or real
protocol. It is the native-UI counterpart to the [`tpt-protocol-cli`](../tpt-protocol-cli)
TUI and the [`tpt-protocol-ui-web`](../tpt-protocol-ui-web) Tauri app.

## Features

- **Slint** declarative UI (`src/ui.slint`) compiled at build time by
  `slint-build` (see `build.rs`).
- Connects to the daemon WebSocket (`tokio-tungstenite`), auto-reconnects every
  3 seconds, and decodes TUP JSON with `tpt_protocol_tup::json::from_json_str`.
- Live `ListView` of the last 200 points (oldest dropped as new arrive).
- High-contrast dark theme (`#101418` background, `#e6edf3` text) suited to
  shop-floor / kiosk displays.
- Single `--url` flag (clap derive) for the daemon WebSocket endpoint.

## Architecture

```
daemon WebSocket (ws://127.0.0.1:9080)
        │  tokio_tungstenite::connect_async + 3s reconnect loop
        ▼
parse_data_points(text)  →  Vec<DataPoint>
        │  (mpsc tx)
        ▼
slint::spawn_local        ──▶  VecModel<DataRow>  ──▶  MainWindow rows
```

- `build.rs` runs `slint_build::compile("src/ui.slint")`, generating the Rust
  `MainWindow` and `DataRow` types pulled in by `slint::include_modules!()`.
- `main.rs` parses `--url`, builds a `MainWindow`, and shares a `VecModel<DataRow>`
  (capped at 200 rows) as the table model.
- A `tokio::spawn`ed WebSocket task reads frames, converts each to `DataPoint`s,
  and forwards them over an unbounded channel; a `slint::spawn_local` task pops
  them and pushes `DataRow`s into the model. `ui.run()` drives the event loop.

### The Slint view (`src/ui.slint`)

`MainWindow` exposes an `in-out property <[DataRow]> rows`. Each `DataRow` has
`time`, `protocol`, `device`, `point`, `value`, `quality` (all `string`). The
window renders a title (`"TPT Protocol — Live Telemetry"`), a fixed header row
with the six column labels, and a `ListView` binding `for row in root.rows`.

## Build

Requires the Slint compiler toolchain (pulled in via `slint-build`). Build and
run the binary directly:

```bash
cargo run -p tpt-protocol-ui-embedded -- --url ws://127.0.0.1:9080
```

The daemon must be running (simulated or real mode) in another terminal:

```bash
cargo run -p tpt-protocol-daemon -- --simulate modbus
```

## Usage

```bash
# Default loopback endpoint
cargo run -p tpt-protocol-ui-embedded

# Custom daemon WebSocket port
cargo run -p tpt-protocol-ui-embedded -- --url ws://127.0.0.1:9090
```

| Flag | Default | Effect |
| ---- | ------- | ------ |
| `--url <URL>` | `ws://127.0.0.1:9080` | Daemon WebSocket URL to stream from. |

## What it shows

A live, scrollable table of the six columns `Time`, `Protocol`, `Device`,
`Point`, `Value`, `Quality` for every TUP `DataPoint` the daemon emits — across
all 19 protocols when the daemon simulates the full fleet. `Time` is
`HH:MM:SS` from the `DataPoint` timestamp.

## Crate layout

```
tpt-protocol-ui-embedded/
├── Cargo.toml
├── README.md
├── build.rs        # slint_build::compile("src/ui.slint")
└── src/
    ├── main.rs     # clap --url, WebSocket task, VecModel<DataRow>, slint::run
    └── ui.slint    # MainWindow + DataRow + ListView table (dark theme)
```

## Status / Conformance

- Consumes TUP `DataPoint`s exactly as produced by the protocol crates / daemon.
- Capped at 200 rows in the `VecModel` to bound memory on long-running displays.
- Native-only (no web target); counterpart web dashboard is
  [`tpt-protocol-ui-web`](../tpt-protocol-ui-web).

## Related crates

- [`tpt-protocol-daemon`](../tpt-protocol-daemon) — simulation/transport source.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — `DataPoint` decoded from each frame.
- [`tpt-protocol-cli`](../tpt-protocol-cli) — ratatui terminal dashboard for the
  same stream.
- [`tpt-protocol-ui-web`](../tpt-protocol-ui-web) — Tauri/SvelteKit fleet
  dashboard for the same stream.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or
[Apache-2.0](../../LICENSE-APACHE) at your option.

## Rust version

This app needs Rust **1.90** or newer (`rust-version = "1.90"` in `Cargo.toml`). It is separate from the workspace library MSRV (1.85), which the `msrv` CI job checks. It is not published (`publish = false`).
