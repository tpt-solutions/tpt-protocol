# tpt-protocol-telos

Formal verification of the **TPT Protocol** critical state machines using
[`tpt-telos`](https://crates.io/crates/tpt-telos).

The protocol crates must uphold safety properties even on malformed, truncated,
or adversarial input. This crate encodes those properties as `.telos`
specifications and discharges them with the `tpt-telos` QF_LRA verifier
(Fourier–Motzkin, no external Z3 required).

## Bundled specs

| Spec | Invariant |
| --- | --- |
| `telos/modbus_crc.telos` | CRC-16 window `[0..len-2]` is always in-bounds for `len >= 4`; no out-of-bounds read on garbage input. |
| `telos/modbus_validator.telos` | Connection-health state machine rejects only when `!connected` or `error_count > 10`. |
| `telos/modbus_normalizer.telos` | `WriteMultipleRegisters` rejects short payloads and never yields more than `N` register values. |

## Running verification

Verification deps are gated behind the `verify` feature (off by default to keep
the dependency surface minimal). Run:

```sh
cargo test -p tpt-protocol-telos --features verify
```

The bundled-spec test panics on any failed check, so it runs cleanly in CI.

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at
your option.
