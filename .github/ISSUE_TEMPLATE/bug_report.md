---
name: Bug report
about: Report a deterministic runtime, checker, shrink, or artifact bug
title: "[bug] "
labels: bug
---

## What failed

## Reproduction

```powershell
DST_SEED=<seed> cargo test <test-name> -- --nocapture
```

## Expected behavior

## Actual behavior

## Artifact

Attach minimized JSON/HTML when available. If replay does not reproduce the same
failure signature, call that out explicitly.

## Checks run

- [ ] `cargo test --workspace`
- [ ] `cargo clippy --workspace -- -D warnings`
- [ ] `bash scripts/lint_determinism.sh`
