## Summary

## Determinism checklist

- [ ] No real time, real threads, real sockets, real files, or system RNG in deterministic crates.
- [ ] New control-plane randomness goes through the entropy tape with a stable label.
- [ ] New failures use `FailureSignature` or structured checker results where possible.
- [ ] Replay and shrink preserve the same normalized failure signature.

## Checks

- [ ] `cargo fmt --all --check`
- [ ] `cargo build --workspace`
- [ ] `cargo test --workspace`
- [ ] `cargo clippy --workspace -- -D warnings`
- [ ] `bash scripts/lint_determinism.sh`
- [ ] `$env:DST_SEED_COUNT='10000'; cargo test --release --test determinism_meta`
