# Contributing to TPT Protocol

Thanks for your interest in contributing! This document covers local setup and the
common workflows.

## Repository layout

This is a Cargo workspace. Crates live under `crates/`:

- `tpt-protocol-tup` — the TPT Unified Protocol (TUP) schema + CBOR/JSON codecs.
- `tpt-protocol-core` — the shared traits: `FrameParser`, `StateValidator`,
  `DataNormalizer`, `ProtocolPlugin`.
- `tpt-protocol-*` — one crate per supported protocol (19 total).
- `tpt-protocol-ethercat` (from-scratch parser) and `tpt-protocol-mavlink` (wraps the `mavlink` crate) — both published.
- `tpt-protocol-daemon` — background service (simulation + local WebSocket API).
- `tpt-protocol-cli` — terminal dashboard.
- `tpt-protocol-ui-embedded`, `tpt-protocol-ui-web` — UI front-ends.
- `tpt-protocol-telos` — formal-verification specs.

## Dev setup

```bash
rustup toolchain install stable
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --check
```

The `no_std` targets are checked in CI:

```bash
rustup target add thumbv6m-none-eabi
cargo build --workspace --target thumbv6m-none-eabi \n  --exclude tpt-protocol-daemon --exclude tpt-protocol-cli \n  --exclude tpt-protocol-ui-web --exclude tpt-protocol-ui-embedded --exclude tpt-protocol-telos
```

License/cve/duplicate-crate hygiene is enforced by `cargo-deny` against
`deny.toml`.

## Common commands

| Task | Command |
|------|---------|
| Build everything | `cargo build --workspace` |
| Test everything | `cargo test --workspace` |
| Lint | `cargo clippy --workspace --all-targets --all-features -- -D warnings` |
| Format | `cargo fmt` |
| List daemon-simulated protocols | `cargo run --bin tpt-protocol-daemon -- --list-protocols` |

## Adding a new protocol crate (#20 and beyond)

Use the scaffolding script:

```bash
./scripts/new-protocol-crate.sh my-protocol
```

This creates `crates/tpt-protocol-my-protocol/` with a `no_std` lib skeleton,
README, and registers it as a workspace member. Then:

1. Implement `FrameParser`, `StateValidator`, and `DataNormalizer` for your
   protocol in `src/lib.rs` (see `tpt-protocol-core/examples/custom_protocol.rs`).
2. Add unit tests; small (8–20 byte) malformed inputs must **not** panic.
3. Add `proptest` fuzzing: feed random bytes into `parse` and assert no panic
   (see any existing protocol crate's `mod fuzz`).
4. Wire a simulated `ProtocolPlugin` into
   `tpt-protocol-daemon/src/plugins/mod.rs`.
5. If the protocol is published, add it to the 19 published crates in the
   Phase 4 publish order.

## Commit / PR conventions

- Keep changes focused; open PRs against `master`.
- Ensure `cargo test`, `clippy -D warnings`, and `cargo fmt --check` pass.
- Update `TODO.md` and `CHANGELOG.md` for user-facing changes.

## License

By contributing you agree your contributions are licensed under either MIT or
Apache-2.0, at the project's option.
