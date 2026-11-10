# tpt-protocol-ui-web

**Web fleet dashboard for the [TPT Protocol](https://github.com/tpt-solutions/tpt-protocol)
daemon, built with [Tauri v2](https://v2.tauri.app) + [SvelteKit](https://kit.svelte.dev).**

A cross-platform desktop app whose SvelteKit renderer connects directly to the
daemon's local WebSocket and shows a live, scrollable table of TUP `DataPoint`s
across all connected devices and protocols — a fleet-wide view of the whole
suite. This is the web/desktop counterpart to the
[`tpt-protocol-cli`](../tpt-protocol-cli) TUI and the
[`tpt-protocol-ui-embedded`](../tpt-protocol-ui-embedded) Slint app.

## Stack

- **Tauri v2** (`tauri` 2, `tauri-build` 2) — the desktop shell / window manager.
- **SvelteKit** (Svelte 5 + `@sveltejs/kit` 2) with **Vite 5** and the
  `@sveltejs/adapter-static` build adapter (frontend in `src/`, config in
  `svelte.config.js`, `vite.config.ts`, `tsconfig.json`).
- **`@tauri-apps/api`** — the renderer calls the `daemon_url` Tauri command to
  resolve the daemon WebSocket endpoint.
- **Rust (tokio + tokio-tungstenite)** — the Tauri side pulls in the same
  WebSocket/async stack, though the live connection is made from the webview
  (browser `WebSocket`) for low latency.

## Architecture

```
┌──────────────────────────── Tauri window ────────────────────────────┐
│  SvelteKit renderer (src/routes/+page.svelte)                         │
│    onMount → resolveDaemonUrl() → invoke('daemon_url')                │
│    connect() → new WebSocket(daemonUrl)  ◀── daemon (ws://127.0.0.1)  │
│    onmessage → push DataPoint (cap 500) → <table> rows                │
└──────────────────────────────────────────────────────────────────────┘
        ▲
        │  Tauri command (src-tauri/src/lib.rs)
        │
   daemon_url() → DaemonConfig { url: $TPT_DAEMON_URL | "ws://127.0.0.1:9080" }
```

- `src-tauri/src/lib.rs` builds the Tauri app and registers the `daemon_url`
  command (reads `TPT_DAEMON_URL`, default `ws://127.0.0.1:9080`).
- `src-tauri/src/main.rs` simply calls `tpt_protocol_ui_web_lib::run()`.
- `src/routes/+page.svelte` opens the WebSocket from the renderer, auto-reconnects
  after 3 s, parses JSON (array or single object) into `DataPoint`s, and renders
  a 6-column table with quality color classes (`q-good` / `q-bad` / `q-uncertain`
  / `q-stale`).
- `tauri.conf.json` sets the product name `TPT Protocol Fleet Dashboard`, window
  `1280×800`, a restrictive CSP allowing `ws://127.0.0.1:9080` and
  `http://localhost:1420`, and bundles icons from `icons/`.

## How it talks to the daemon

The dashboard consumes the **same daemon WebSocket stream** as the CLI and
embedded UIs — TUP `DataPoint`s serialized as JSON. The renderer connects
straight from the browser `WebSocket` API (no Rust relay in the hot path); the
`daemon_url` Tauri command only supplies the endpoint, honoring `TPT_DAEMON_URL`
so the dashboard can target a different loopback port. When run in a plain
browser (no Tauri), it falls back to the default `ws://127.0.0.1:9080`.

## Develop

```bash
# terminal 1 — daemon
cargo run -p tpt-protocol-daemon -- --simulate modbus

# terminal 2 — frontend + Tauri shell
cd crates/tpt-protocol-ui-web
npm install
npm run tauri dev
```

## Build

```bash
cd crates/tpt-protocol-ui-web
npm install
npm run tauri build
```

The `daemon_url` Tauri command reads `TPT_DAEMON_URL` (defaults to
`ws://127.0.0.1:9080`) so the dashboard can target a different loopback port:

```bash
TPT_DAEMON_URL=ws://127.0.0.1:9090 npm run tauri dev
```

## What it shows

A live, scrollable table of the six columns `Time`, `Protocol`, `Device`,
`Point`, `Value` (with optional `unit`), `Quality` for every TUP `DataPoint`
the daemon emits. The newest 500 points are retained (newest first), the header
shows a CONNECTED/DISCONNECTED chip, and `Quality` is color-coded.

## Crate layout

```
tpt-protocol-ui-web/
├── Cargo.toml              # lib (src-tauri/src/lib.rs) + bin (src-tauri/src/main.rs)
├── README.md
├── build.rs                # tauri_build::build()
├── package.json            # SvelteKit + Vite + @tauri-apps/api scripts
├── svelte.config.js
├── vite.config.ts
├── tsconfig.json
├── tauri.conf.json         # Tauri v2 config (window, CSP, bundle)
├── capabilities/default.json
├── icons/                  # icon.ico / icon.png
├── src/
│   ├── app.css
│   ├── app.d.ts
│   └── routes/
│       ├── +layout.svelte
│       └── +page.svelte    # WebSocket client + fleet table
└── src-tauri/
    └── src/
        ├── lib.rs          # run() + daemon_url command
        └── main.rs         # calls run()
```

## Status / Conformance

- Consumes TUP `DataPoint`s exactly as produced by the protocol crates / daemon.
- Renderer-side connection with 3 s auto-reconnect; caps the in-memory list at
  500 points.
- CSP restricts connect-src to loopback on any port (for example `ws://127.0.0.1:9080`,
  `http://localhost:1420`) — local-first by default.
- The production devtools permission is removed from the capability. The web build runs
  under `vite build`, and CI runs `svelte-check`.

## Related crates

- [`tpt-protocol-daemon`](../tpt-protocol-daemon) — simulation/transport source.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — `DataPoint` envelope consumed.
- [`tpt-protocol-cli`](../tpt-protocol-cli) — ratatui terminal dashboard for the
  same stream.
- [`tpt-protocol-ui-embedded`](../tpt-protocol-ui-embedded) — Slint native
  dashboard for the same stream.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or
[Apache-2.0](../../LICENSE-APACHE) at your option.

## Rust version

This app needs Rust **1.90** or newer (`rust-version = "1.90"` in `Cargo.toml`). It is separate from the workspace library MSRV (1.85), which the `msrv` CI job checks. It is not published (`publish = false`).
