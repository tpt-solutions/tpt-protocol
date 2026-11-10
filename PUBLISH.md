# crates.io Publish Tracker

Tracks the release of the **21 published crates** to crates.io, in dependency order.
Source of truth for scope: `TODO.md` (Phase 4). Update the checkboxes as each step completes.

**Status:** Not started. `cargo publish` is irreversible. TODO.md requires an explicit go-ahead before the first real publish.

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

- [ ] Working tree committed. Uncommitted README/CHANGELOG edits exist as of the 2026-11-11 snapshot. Commit them first, so `cargo publish` does not need `--allow-dirty`.
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes
- [ ] `cargo test --workspace` passes
- [ ] `cargo build --workspace --target thumbv6m-none-eabi` passes (no_std gate)
- [ ] `cargo package --list -p <crate>` runs cleanly for all 21 crates (needs no registry)
- [ ] Every crate's `CHANGELOG.md` has a `0.1.0` entry dated on the release day
- [ ] Logged in: `cargo login` with a crates.io token that has publish scope
- [ ] Confirm all 21 names are still unclaimed on crates.io (`cargo search tpt-protocol-` or check each page)
- [ ] Explicit go-ahead from the maintainer recorded here: ____________ (date)

---

## Tier 0: foundation

- [ ] `cargo publish --dry-run -p tpt-protocol-tup`
- [ ] `cargo publish -p tpt-protocol-tup`
- [ ] Confirm live at https://crates.io/crates/tpt-protocol-tup (wait for it to show before continuing)

## Tier 1: core

- [ ] `cargo publish --dry-run -p tpt-protocol-core` (needs `tup` live, so run after Tier 0)
- [ ] `cargo publish -p tpt-protocol-core`
- [ ] Confirm live at https://crates.io/crates/tpt-protocol-core

## Tier 2: protocol crates

Dry-run each crate only after its dependencies are live. Publish in any order. The registry index can lag a minute or two, so if a dry-run or publish reports the dependency is missing, wait and retry.

| # | Crate | Internal deps | Dry-run | Publish | Live on crates.io |
|---|-------|---------------|---------|---------|-------------------|
| 1 | `tpt-protocol-modbus` | core, tup | [ ] | [ ] | [ ] |
| 2 | `tpt-protocol-j1939` | core, tup | [ ] | [ ] | [ ] |
| 3 | `tpt-protocol-isobus` | core, tup | [ ] | [ ] | [ ] |
| 4 | `tpt-protocol-uds` | core, tup | [ ] | [ ] | [ ] |
| 5 | `tpt-protocol-canopen` | core, tup | [ ] | [ ] | [ ] |
| 6 | `tpt-protocol-doip-someip` | core, tup | [ ] | [ ] | [ ] |
| 7 | `tpt-protocol-nmea` | core, tup | [ ] | [ ] | [ ] |
| 8 | `tpt-protocol-mavlink` | core, tup | [ ] | [ ] | [ ] |
| 9 | `tpt-protocol-bacnet` | core, tup | [ ] | [ ] | [ ] |
| 10 | `tpt-protocol-opcua` | core, tup | [ ] | [ ] | [ ] |
| 11 | `tpt-protocol-dnp3` | core, tup | [ ] | [ ] | [ ] |
| 12 | `tpt-protocol-ethercat` | core, tup | [ ] | [ ] | [ ] |
| 13 | `tpt-protocol-sparkplugb` | core, tup | [ ] | [ ] | [ ] |
| 14 | `tpt-protocol-ocpp` | core, tup | [ ] | [ ] | [ ] |
| 15 | `tpt-protocol-ieee11073-sdc` | core, tup | [ ] | [ ] | [ ] |
| 16 | `tpt-protocol-hl7v2` | core, tup | [ ] | [ ] | [ ] |
| 17 | `tpt-protocol-knx` | core, tup | [ ] | [ ] | [ ] |
| 18 | `tpt-protocol-matter-thread` | core, tup | [ ] | [ ] | [ ] |
| 19 | `tpt-protocol-coap` | core, tup | [ ] | [ ] | [ ] |

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
- [ ] Record the publish date here: ____________

---

## Rollback notes

- A published version cannot be deleted. Use `cargo yank --version 0.1.0 -p <crate>` to stop new dependents from resolving to a bad release.
- If a Tier 2 crate fails after Tier 0 and 1 are live, the other crates can still publish. Fix the failing one and publish it as `0.1.1` (or `0.1.0` if nothing shipped yet).
- Never publish Tier 2 before Tier 1 is live. The dry-run will fail, and a real publish would leave the crate unresolvable.
