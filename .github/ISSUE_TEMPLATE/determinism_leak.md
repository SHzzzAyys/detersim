---
name: Determinism leak
about: Same seed produced divergent trace, history, tape, or artifact output
title: "[determinism] "
labels: determinism, bug
---

## Seed and command

```powershell
DST_SEED=<seed> <command>
```

## Divergence observed

- [ ] trace
- [ ] history
- [ ] nemesis trace
- [ ] tape events
- [ ] replay diagnostics

## First bad change if known

## Notes

Do not retry this away. A same-seed divergence is a correctness bug.
