# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `svelte-check` and `prepare` scripts, with a locked `package-lock.json`. The CI `ui-web` job
  runs them.

### Changed
- A standalone workspace root, outside the main workspace, so its Tauri tree does not affect
  the core build, CI or `cargo-deny`.
- The CSP allows loopback on any port, not a hard-coded 9080.
- The production devtools permission is removed from the capability.
- Requires Rust 1.90 (`rust-version`). The library workspace stays at 1.85.
- Marked `publish = false`.

### Fixed
- The Vite config imports from `@sveltejs/kit/vite`, and `src/app.html` is present, so the web
  build runs. It never built before.
- One reconnect timer, cleared on teardown.
- Lagged notices are shown as a message, not as a table row.

## [0.1.0] - 2026-07-30

### Added
- Initial release: Tauri v2 + SvelteKit fleet dashboard for the TPT Protocol daemon in the TPT Protocol suite.
- `tpt-protocol-ui-web` Tauri app (lib `tpt_protocol_ui_web_lib` + bin) with `tauri-build` 2 and `tauri` 2 dependencies.
- SvelteKit frontend (`src/routes/+page.svelte`) connecting directly to the daemon WebSocket from the renderer, with 3-second auto-reconnect and a 500-point rolling buffer.
- `daemon_url` Tauri command (src-tauri/src/lib.rs) returning `DaemonConfig { url }`, reading `TPT_DAEMON_URL` (default `ws://127.0.0.1:9080`).
- Live 6-column fleet table (Time/Protocol/Device/Point/Value/Quality) with quality color classes (`q-good`/`q-bad`/`q-uncertain`/`q-stale`) and CONNECTED/DISCONNECTED status.
- `tauri.conf.json` (product `TPT Protocol Fleet Dashboard`, 1280×800 window, loopback-restricted CSP) and `package.json` scripts (`dev`/`build`/`preview`) for the SvelteKit + Vite + Tauri workflow.
- Build config: `@sveltejs/adapter-static`, `svelte` 5, `@sveltejs/kit` 2, `vite` 5, `@tauri-apps/api` 2.
