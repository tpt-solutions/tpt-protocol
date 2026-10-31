# tpt-protocol-cli

**TPT Protocol terminal UI — a [ratatui](https://ratatui.rs) live dashboard for
protocol telemetry in the [TPT Protocol](https://github.com/tpt-protocol/tpt-protocol)
suite.**

A keyboard-driven TUI that connects to the
[`tpt-protocol-daemon`](../tpt-protocol-daemon)'s local WebSocket, receives a
stream of TUP `DataPoint`s, and renders them as a scrollable, color-coded table.
It is the canonical way to watch simulated (or real) protocol traffic without a
browser or a GUI toolkit.

## Features

- Built on `ratatui` 0.29 + `crossterm` 0.28 (cross-platform terminal rendering).
- Connects over WebSocket (`tokio-tungstenite`) and **auto-reconnects** every 3
  seconds on disconnect/error.
- Rolling buffer of the last `MAX_DATA_POINTS = 100` points.
- Live connection-status title bar (CONNECTED / DISCONNECTED), message counter,
  and uptime.
- Color-coded `Quality` column: green = Good, red = Bad, yellow = Uncertain,
  dark gray = Stale.
- UTF-8-safe column truncation (`truncate_to`) so long `device_id` /
  `point_id` strings never panic mid-character.
- Parses both JSON arrays (a batch of points) and single JSON objects.

## Architecture

```
daemon WebSocket (ws://127.0.0.1:9080)
        │  connect_async + auto-reconnect loop
        ▼
ws_task  ──(mpsc)──▶  App { data_points, connected, data_count, error, ... }
        │                       │
        │                 100 ms tick: draw + drain rx
        ▼                       ▼
   App::add_data        ui(frame, app)  → ratatui widgets
```

The binary (`src/main.rs`) is a single-thread-of-control `tokio` runtime driving
a `ratatui` event loop:

- `ws_task` owns the WebSocket connection; it loops, reconnecting on failure, and
  forwards `WsMessage::{Connected, Disconnected, Data, Error}` over an unbounded
  channel.
- `parse_data_points` converts each text frame into `Vec<DataPoint>` using
  `tpt_protocol_tup::json::from_json_str` (array or single object).
- `ui` lays the screen out as title / stats / table / status, with a
  `TableState` scroll offset that clamps to the visible window.
- `main` polls `crossterm` key events and redraws at `TICK_RATE`
  (100 ms), draining the channel on each tick.

## Usage

```bash
# Connect to the daemon on the default loopback port (ws://127.0.0.1:9080)
cargo run --bin tpt-protocol-cli

# Connect to a daemon on a different loopback port
cargo run --bin tpt-protocol-cli -- --url ws://127.0.0.1:9090

# Connect to a daemon hosted elsewhere (the --daemon flag expands to ws://<host>)
cargo run --bin tpt-protocol-cli -- --daemon 192.168.1.100:9080
```

Flags (clap derive):

| Flag | Default | Effect |
| ---- | ------- | ------ |
| `--url <URL>` | `ws://127.0.0.1:9080` | Full WebSocket URL of the daemon. |
| `--daemon <HOST:PORT>` | — | Convenience: connect to `ws://<HOST:PORT>` (overrides `--url`). |

> The daemon (a separate crate) exposes `--list-protocols`, `--replay`, and
> `--bind`; this CLI binary only consumes the daemon's WebSocket stream, so it
> has no such flags.

Start the daemon in another terminal first, e.g.:

```bash
cargo run --bin tpt-protocol-daemon -- --simulate modbus
```

## Controls

| Key | Action |
| --- | ------ |
| `q` / `Q` | Quit |
| `Esc` | Quit |
| `Ctrl+C` | Quit |
| `↑` / `↓` | Scroll the data table |

## Display

- **Title bar** — ` TPT Protocol CLI ` with a green ` CONNECTED ` / red
  ` DISCONNECTED ` status chip.
- **Stats bar** — `Messages: N | Uptime: HH:MM:SS`.
- **Data table** — columns `Time`, `Protocol`, `Device`, `Point`, `Value`,
  `Quality`. `Time` is `HH:MM:SS.mmm` derived from the `DataPoint` timestamp
  (ms since epoch). `Device`/`Point` are truncated to 18 bytes with a `..`
  marker when longer. `Quality` is color-coded per the table above.
- **Status bar** — `[Q]uit [↑↓] Scroll [Esc] Quit` and, if present, a red
  `Error: …` line.

## Crate layout

```
tpt-protocol-cli/
├── Cargo.toml
├── README.md
└── src/
    └── main.rs   # clap Cli, ws_task, App, parse_data_points, ui(), main()
```

## Status / Conformance

- Consumes TUP `DataPoint`s exactly as produced by the protocol crates and the
  daemon's WebSocket.
- `truncate_to` unit tests assert multi-byte (`é`) boundaries are respected and
  long ASCII strings append `..` without panic.
- No wire parsing of its own — it trusts the daemon to emit valid TUP JSON.

## Related crates

- [`tpt-protocol-daemon`](../tpt-protocol-daemon) — the simulation/transport
  service this dashboard visualizes.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — the `DataPoint` envelope decoded
  from each WebSocket frame.
- [`tpt-protocol-ui-embedded`](../tpt-protocol-ui-embedded) — the Slint native
  dashboard for the same WebSocket stream.
- [`tpt-protocol-ui-web`](../tpt-protocol-ui-web) — the Tauri/SvelteKit fleet
  dashboard for the same WebSocket stream.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or
[Apache-2.0](../../LICENSE-APACHE) at your option.
