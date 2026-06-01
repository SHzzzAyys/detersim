---
name: Determinism leak
about: Same seed produced divergent trace, history, tape, or artifact output
title: "[determinism] "
labels: determinism, bug
---

## Command

```powershell
<exact command>
```

## Seed / Tape

```text
seed:
tape:
```

## Divergence Observed

- [ ] trace
- [ ] history
- [ ] nemesis trace
- [ ] tape log
- [ ] tape events
- [ ] replay diagnostics
- [ ] artifact JSON/HTML

Paste the smallest useful diff or summary:

```text

```

## Expected Result

Same seed and tape should produce byte-identical trace/history/artifact output.

## First Bad Change If Known

Commit, PR, or subsystem:

## Suspected Source

- [ ] real time
- [ ] real threads or async task spawning
- [ ] real file/network I/O
- [ ] system RNG
- [ ] nondeterministic map/set iteration
- [ ] unstable artifact serialization
- [ ] unknown

## Artifact

Attach generated and replayed JSON/HTML if available.

Do not retry this away. A same-seed divergence is a correctness bug.
