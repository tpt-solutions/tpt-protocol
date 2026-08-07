# tpt-protocol-ui-web

Web fleet dashboard for the **TPT Protocol** daemon, built with
[Tauri v2](https://v2.tauri.app) + [SvelteKit](https://kit.svelte.dev).

The renderer connects directly to the daemon's local WebSocket
(`ws://127.0.0.1:9080`) and shows a live, scrollable table of TUP data points
across all connected devices/protocols.

## Develop

```sh
# terminal 1 — daemon
cargo run -p tpt-protocol-daemon -- --simulate modbus

# terminal 2 — frontend + Tauri shell
cd crates/tpt-protocol-ui-web
npm install
npm run tauri dev
```

## Build

```sh
cd crates/tpt-protocol-ui-web
npm install
npm run tauri build
```

The `daemon_url` Tauri command reads `TPT_DAEMON_URL` (defaults to
`ws://127.0.0.1:9080`) so the dashboard can target a different loopback port.

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at
your option.
