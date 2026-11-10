# crates.io Publish Tracker

Tracks the release of the **21 published crates** to crates.io, in dependency order.
Source of truth for scope: `TODO.md` (Phase 4). Update the checkboxes as each step completes.

**Status:** Published 2026-11-11 (all 21 crates live at 0.1.0). `cargo publish` is irreversible. TODO.md requires an explicit go-ahead before the first real publish.

---

## Publish order (3 tiers)

A crate can only be published once every internal crate it depends on is already live on crates.io. Within a tier, order does not matter.

| Tier | Crates | Why this tier |
|------|--------|---------------|
| 0 | `tpt-protocol-tup` | Zero internal deps |
| 1 | `tpt-protocol-core` | Depends on `tup` |
| 2 | 19 protocol crates (any order, see below) | Each depends only on `core` + `tup` |

Not published (`publish = false`): `tpt-protocol-daemon`, `tpt-protocol-cli`, `tpt-protocol-bindings`, `tpt-protocol-telos`, `tpt-protocol-ui-embedded`, `tpt-protocol-ui-web`, `xtask`.

---

## Pre-flight (do once, before Tier 0)

- [x] Working tree committed. Uncommitted README/CHANGELOG edits exist as of the 2026-11-11 snapshot. Commit them first, so `cargo publish` does not need `--allow-dirty`.
- [x] `cargo fmt --check` passes
- [x] `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes
- [x] `cargo test --workspace` passes
- [x] `cargo build --workspace --target thumbv6m-none-eabi` passes (no_std gate)
- [x] `cargo package --list -p <crate>` runs cleanly for all 21 crates (needs no registry)
- [x] Every crate's `CHANGELOG.md` has a `0.1.0` entry dated on the release day
- [x] Logged in: `cargo login` with a crates.io token that has publish scope
- [x] Confirm all 21 names are still unclaimed on crates.io (`cargo search tpt-protocol-` or check each page)
- [x] Explicit go-ahead from the maintainer recorded here: given in chat 2026-11-11 ("proceed to publish to crates.io")

---

## Tier 0: foundation

- [x] `cargo publish --dry-run -p tpt-protocol-tup`
- [x] `cargo publish -p tpt-protocol-tup`
- [x] Confirm live at https://crates.io/crates/tpt-protocol-tup (wait for it to show before continuing)

## Tier 1: core

- [x] `cargo publish --dry-run -p tpt-protocol-core` (needs `tup` live, so run after Tier 0)
- [x] `cargo publish -p tpt-protocol-core`
- [x] Confirm live at https://crates.io/crates/tpt-protocol-core

## Tier 2: protocol crates

Dry-run each crate only after its dependencies are live. Publish in any order. The registry index can lag a minute or two, so if a dry-run or publish reports the dependency is missing, wait and retry.

| # | Crate | Internal deps | Dry-run | Publish | Live on crates.io |
|---|-------|---------------|---------|---------|-------------------|
| 1 | `tpt-protocol-modbus` | core, tup | [x] | [x] | [x] |
| 2 | `tpt-protocol-j1939` | core, tup | [x] | [x] | [x] |
| 3 | `tpt-protocol-isobus` | core, tup | [x] | [x] | [x] |
| 4 | `tpt-protocol-uds` | core, tup | [x] | [x] | [x] |
| 5 | `tpt-protocol-canopen` | core, tup | [x] | [x] | [x] |
| 6 | `tpt-protocol-doip-someip` | core, tup | [x] | [x] | [x] |
| 7 | `tpt-protocol-nmea` | core, tup | [x] | [x] | [x] |
| 8 | `tpt-protocol-mavlink` | core, tup | [x] | [x] | [x] |
| 9 | `tpt-protocol-bacnet` | core, tup | [x] | [x] | [x] |
| 10 | `tpt-protocol-opcua` | core, tup | [x] | [x] | [x] |
| 11 | `tpt-protocol-dnp3` | core, tup | [x] | [x] | [x] |
| 12 | `tpt-protocol-ethercat` | core, tup | [x] | [x] | [x] |
| 13 | `tpt-protocol-sparkplugb` | core, tup | [x] | [x] | [x] |
| 14 | `tpt-protocol-ocpp` | core, tup | [x] | [x] | [x] |
| 15 | `tpt-protocol-ieee11073-sdc` | core, tup | [x] | [x] | [x] |
| 16 | `tpt-protocol-hl7v2` | core, tup | [x] | [x] | [x] |
| 17 | `tpt-protocol-knx` | core, tup | [x] | [x] | [x] |
| 18 | `tpt-protocol-matter-thread` | core, tup | [x] | [x] | [x] |
| 19 | `tpt-protocol-coap` | core, tup | [x] | [x] | [x] |

Commands per row:

```bash
cargo publish --dry-run -p <crate>
cargo publish -p <crate>
```

---

## Post-publish

- [ ] Verify docs.rs built each crate (https://docs.rs/<crate>). Any failure is usually a `cfg`/feature docs issue. Fix it in a patch release.
- [ ] Add crates.io and docs.rs badges to the root README (TODO.md Phase 13 is waiting on this)
- [ ] Set the cargo-semver-checks baseline to the published 0.1.0 (TODO.md Phase 13 is waiting on this)
- [ ] Add a `cargo publish --dry-run` CI job now that the registry is live (TODO.md Phase 13)
- [ ] Tag the release `v0.1.0`. This triggers `release.yml`, which builds the daemon and CLI binaries. Decide whether to tag before or after the crates are published.
- [ ] Update TODO.md Phase 4 checkboxes and the README "published crates" count if it changed
- [ ] Record the publish date here: 2026-11-11

---

## Rollback notes

- A published version cannot be deleted. Use `cargo yank --version 0.1.0 -p <crate>` to stop new dependents from resolving to a bad release.
- If a Tier 2 crate fails after Tier 0 and 1 are live, the other crates can still publish. Fix the failing one and publish it as `0.1.1` (or `0.1.0` if nothing shipped yet).
- Never publish Tier 2 before Tier 1 is live. The dry-run will fail, and a real publish would leave the crate unresolvable.
