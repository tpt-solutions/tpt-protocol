# xtask

Workspace tasks for the [TPT Protocol](https://github.com/tpt-solutions/tpt-protocol)
suite. It is a standard-library-only Rust program, so the tasks behave the same on
Windows, Linux and macOS. It is not published (`publish = false`).

Run it through the alias in `.cargo/config.toml`:

```bash
cargo xtask new-protocol <name>
```

## Commands

| Command | Effect |
| --- | --- |
| `cargo xtask new-protocol <name>` | Scaffold a protocol crate and wire it into the workspace |
| `cargo xtask --dry-run new-protocol <name>` | Print what would change, and write nothing |
| `cargo xtask --root <dir> new-protocol <name>` | Operate on another checkout (used by the tests) |

`--help` prints the usage line.

## `new-protocol`

`<name>` is the protocol's wire name, and the crate is `tpt-protocol-<name>`. A name must
start with a lowercase letter and contain only lowercase letters, digits and single hyphens.
For example, `zigbee` or `matter-thread` are valid, and `Zigbee`, `-x` or `a--b` are not.

It creates:

- `crates/tpt-protocol-<name>/Cargo.toml`, `README.md` and `src/lib.rs`, with a parser, a
  validator, a normalizer, unit tests, and a `proptest` check that the parser never panics.
- `crates/tpt-protocol-<name>/examples/parse_<name>.rs`, a runnable example.
- `fuzz/fuzz_targets/<name>.rs`, a fuzz target, and its entry in `fuzz/Cargo.toml`.

It also edits existing files:

- `Cargo.toml`: adds the crate to the workspace members.
- `crates/tpt-protocol-tup/src/source_protocol.rs`: adds a `SourceProtocol` variant and its
  name tables.
- `crates/tpt-protocol-tup/src/cddl_conformance.rs`: adds the name to the conformance list.
- `schema/tup-v1.cddl` and `schema/tup-v1.schema.json`: adds the name to the protocol lists.
- `crates/tpt-protocol-daemon/Cargo.toml` and `src/plugins/mod.rs`: adds the dependency,
  its feature and the daemon plugin.

Every edit is computed in memory first. If any anchor text is missing from an existing file,
nothing is written, so a half-wired protocol is never left behind.

After scaffolding, the tool prints the next step: run `cargo test -p tpt-protocol-<name>`.
The generated crate contains placeholder parsing logic, so it is a starting point to replace,
not a working decoder.

## Dry run

`--dry-run` prints each file and edit it would make, and then `(dry run: nothing written)`.
Example output, for a protocol named `demo`:

```text
crate manifest                   crates\tpt-protocol-demo\Cargo.toml
crate source                     crates\tpt-protocol-demo\src/lib.rs
crate README                     crates\tpt-protocol-demo\README.md
example                          crates\tpt-protocol-demo\examples\parse_demo.rs
fuzz target                      fuzz/fuzz_targets\demo.rs
workspace member                 Cargo.toml
SourceProtocol variant           crates/tpt-protocol-tup/src/source_protocol.rs
conformance list                 crates/tpt-protocol-tup/src/cddl_conformance.rs
CDDL protocol list               schema/tup-v1.cddl
JSON schema protocol list        schema/tup-v1.schema.json
fuzz manifest                    fuzz/Cargo.toml
daemon dependency and feature    crates/tpt-protocol-daemon/Cargo.toml
daemon plugin                    crates/tpt-protocol-daemon/src/plugins/mod.rs
(dry run: nothing written)
```

## Tests

```bash
cargo test -p xtask
```

The tests cover name derivation and rejection of invalid names, the `SourceProtocol` and
conformance edits, the workspace anchor, the error on a missing anchor, and a dry run that
writes nothing.

## Crate layout

- `src/main.rs` — argument parsing, the `new-protocol` task, the file templates, the
  anchored edits, and the tests.

## Related

- [`docs/bring-your-own-protocol.md`](../docs/bring-your-own-protocol.md) — the protocol
  guide, including what the scaffold does not decide for you.
