---
name: Bug report
about: Report a runtime, checker, shrink, CLI, or artifact bug
title: "[bug] "
labels: bug
---

## What Failed

Describe the bug and the subsystem involved.

## Reproduction

```powershell
<exact command>
```

Seed or tape if relevant:

```text
seed:
tape:
```

## Expected Behavior

What should have happened?

## Actual Behavior

What happened instead?

## Determinism Status

- [ ] Same seed reproduces the same failure.
- [ ] Replay reproduces the same failure.
- [ ] Shrink preserves the same `FailureSignature`.
- [ ] The failure is nondeterministic or not yet minimized.
- [ ] Not applicable.

## Artifact

Attach minimized JSON/HTML if available. If replay does not reproduce the same
failure signature, call that out explicitly.

## Environment

- OS:
- Rust version:
- DeterSim branch/commit:
- Command shell:

## Checks Run

- [ ] `cargo test --workspace`
- [ ] `cargo clippy --workspace -- -D warnings`
- [ ] `bash scripts/lint_determinism.sh`
- [ ] `$env:DST_SEED_COUNT='10000'; cargo test --release --test determinism_meta`
