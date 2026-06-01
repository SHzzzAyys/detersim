# Contributing to DeterSim

DeterSim is a deterministic simulation testing framework. The main engineering
rule is simple: same seed, same binary, same platform must produce the same
trace.

## Development rules

- Write SUT code against `Env`.
- Use `BTreeMap`, `BTreeSet`, or `Vec` for deterministic control flow.
- Do not use real time, real threads, real sockets, real files, or system RNG
  in deterministic crates, examples, or tests.
- Put reusable protocol SUTs in `detersim-protocols`.
- Put user-facing experiment helpers in `detersim-testkit`.
- Normalize failures as `FailureSignature`; do not assert on incidental trace
  substrings when a structured oracle is available.

## Before sending changes

Run:

```powershell
cargo fmt --all --check
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
bash scripts/lint_determinism.sh
```

For runtime, replay, shrink, checker, nemesis, or protocol changes, also run:

```powershell
$env:DST_SEED_COUNT='10000'; cargo test --release --test determinism_meta
```

If a deterministic gate fails, fix the leak. Do not weaken the test or mark it
ignored.
