# tpt-protocol-telos

**Formal verification of [TPT Protocol](https://github.com/tpt-protocol/tpt-protocol) critical state machines using [`tpt-telos`](https://crates.io/crates/tpt-telos).**

The protocol crates must uphold safety properties even on malformed, truncated,
or adversarial input — most importantly, they must never panic or read out of
bounds. This crate encodes those properties as `.telos` specifications and
discharges them with the `tpt-telos` QF_LRA verifier (Fourier–Motzkin
elimination — **no external Z3 required**).

This is a *specifications + harness* crate, not a wire parser: it holds the
machine-checkable contracts for the arithmetic/bounds reasoning inside the
real parsers (`tpt-protocol-modbus`, and the connection-health logic mirrored
by several other crates).

## Features

| Feature | Default | Description |
| ------- | ------- | ----------- |
| `verify` | no | Enables the verification harness (`verify_dir` / `verify_source` / `verify_bundled`) and pulls in `tpt-telos-parser`, `tpt-telos-ir`, `tpt-telos-verifier`. Off by default to keep the dependency surface minimal. |

Without `verify` the crate compiles to essentially just the `TelosError` type
and `VerificationReport` struct — a tiny, dependency-free unit.

## Architecture

```
telos/*.telos            .telos source specs (DSL)
        │
        ▼  (feature = "verify")
tpt_telos_parser         parse(src)      -> modules
tpt_telos_ir             extract(modules)-> problems (verification conditions)
tpt_telos_verifier       verify(problem) -> checks (QF_LRA)
        │
        ▼
VerificationReport { func_name, all_passed, failures }
```

The harness API (`src/lib.rs`):

- `TelosError` — `Parse(String)`, `Extract(String)`, `CheckFailed(String)`;
  implements `Display` + `std::error::Error`.
- `VerificationReport` — one verified module: its `func_name`, `all_passed`,
  and the list of failed check descriptions.
- `verify_dir(dir)` — discovers every `*.telos` file under `dir`, parses,
  extracts, and verifies each (sorted, deterministic order).
- `verify_source(src)` — parses/extracts/verifies a single `.telos` string and
  returns a `VerificationReport`, or `Err(TelosError::CheckFailed(..))` with the
  counter-example descriptions on the first failure.
- `verify_bundled()` — verifies the specs shipped in `telos/` (relative to the
  crate root) and returns `Err` on any failed check; intended to be called from
  a `#[test]`.

## Bundled specs (the DSL format)

The `.telos` DSL expresses a `module` containing `struct` state, `invariant`
declarations, and `func` declarations with `requires`/`ensures` contract clauses
and a `mutate state { … }` body. The three bundled specs model the
safety-critical contracts behind the parser pipeline:

| Spec | State machine / function | Invariant verified |
| --- | --- | --- |
| `telos/modbus_crc.telos` | `ModbusCrc::parse` — CRC-16 framing window | `crc_start >= 2 && crc_start < len` for `len >= 4`; the CRC window `[0..len-2]` is always in bounds, so there is no out-of-bounds read on garbage/truncated input. |
| `telos/modbus_validator.telos` | `ModbusValidator` connection-health counter | `error_count >= 0` always; `on_ok` resets to `0`, `on_err` increments by exactly `1` — the arithmetic core of the "reject after > 10 errors" threshold. |
| `telos/modbus_normalizer.telos` | `ModbusNormalizer::compute` — `WriteMultipleRegisters` | `quantity >= 0` and `value_count <= quantity`: a declared quantity `N` yields at most `N` register values, so the parser cannot read past the data slice. |

These mirror the contracts enforced at runtime by
[`tpt-protocol-modbus`](../tpt-protocol-modbus) (`UdsValidator`/`ModbusValidator`
threshold logic, CRC window, normalizer bounds).

## Usage

Verification dependencies are gated behind the `verify` feature (off by default
to keep the dependency surface minimal). Run the bundled-spec verification as a
test:

```bash
cargo test -p tpt-protocol-telos --features verify
```

The bundled-spec test (`tests::bundled_specs_verify`) calls `verify_bundled()`
and **panics on any failed check**, so it fails CI loudly. You can also verify
an ad-hoc string programmatically:

```rust
use tpt_protocol_telos::verify_source;

let report = verify_source("module Demo { func f() ensures true { mutate state {} } }")
    .expect("spec must verify");
assert!(report.all_passed);
```

Or load a whole directory of specs:

```rust
use tpt_protocol_telos::verify_dir;

let reports = verify_dir("telos").expect("all .telos files parse & verify");
for r in reports {
    println!("{} -> {}", r.func_name, if r.all_passed { "OK" } else { "FAIL" });
}
```

## Crate layout

```
tpt-protocol-telos/
├── Cargo.toml
├── README.md
├── src/
│   └── lib.rs            # TelosError, VerificationReport, verify_dir/verify_source/verify_bundled
└── telos/
    ├── modbus_crc.telos       # CRC-16 window bounds (no OOB read)
    ├── modbus_validator.telos # error-count threshold arithmetic
    └── modbus_normalizer.telos# WriteMultipleRegisters quantity bounds
```

## Status / Conformance

- Three bundled specs, all discharging on the `verify` feature with the
  `tpt-telos` QF_LRA verifier (Fourier–Motzkin, no external SMT solver).
- Specs target the memory-safety and threshold arithmetic contracts; they are
  checked by `cargo test --features verify` and must pass in CI.
- `TelosError::CheckFailed` carries the failing check descriptions so a
  regression names its own counter-example.

## Related crates

- [`tpt-protocol-core`](../tpt-protocol-core) — the `FrameParser` /
  `StateValidator` / `DataNormalizer` traits the verified logic implements.
- [`tpt-protocol-modbus`](../tpt-protocol-modbus) — the runtime parser whose
  CRC window, validator threshold, and normalizer bounds are modelled here.
- `tpt-telos-parser` / `tpt-telos-ir` / `tpt-telos-verifier` — the upstream
  `tpt-telos` toolchain (pulled in only with the `verify` feature).

## License

Licensed under either of [MIT](../../LICENSE-MIT) or
[Apache-2.0](../../LICENSE-APACHE) at your option.
