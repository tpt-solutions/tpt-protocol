# tpt-protocol-cli

TPT Protocol terminal UI — ratatui dashboard for live protocol data.

## Usage

```bash
# Connect to daemon on default port
cargo run --bin tpt-protocol-cli

# Connect to daemon on custom port
cargo run --bin tpt-protocol-cli -- --url ws://127.0.0.1:9090

# Connect to remote daemon
cargo run --bin tpt-protocol-cli -- --daemon 192.168.1.100:9080
```

## Controls

- `q`, `Esc`, `Ctrl+C` — quit
- `↑` / `↓` — scroll data table

## Display

- Title bar with connection status (green/red)
- Stats bar (message count, uptime)
- Scrollable data table with Time, Protocol, Device, Point, Value, Quality columns
- Color-coded quality: green=good, red=bad, yellow=uncertain, gray=stale
