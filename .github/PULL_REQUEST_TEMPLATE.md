## Summary

<!-- What changes, and why. -->

## Checklist

- [ ] `cargo fmt --all --check` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] `cargo test --workspace` passes
- [ ] A regression test is included for any bug fix (it fails before the fix)
- [ ] Protocol/core/tup changes still build for `thumbv6m-none-eabi` (no_std)
