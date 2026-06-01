# DeterSim v1.0 progress status

This document is the v1.0 baseline ledger. It separates executable capability
from reference scaffolding so release claims stay conservative.

## Current executable slices

- Runtime: deterministic single-threaded event queue, logical time, spawn/join,
  network delivery, replayable entropy tape, crash/restart, and replay
  diagnostics.
- Nemesis: partitions, asymmetric links, drop/duplicate/delay knobs, crash,
  restart, clock skew, bit rot, torn writes, lost-on-crash, and pre-fsync
  reorder.
- Storage: in-memory page store with pending/committed state and explicit
  `flush` durability.
- Checker: structured histories, deterministic linearizability search budget,
  built-in register, single-key KV, multi-key KV, and append-only log models.
- Shrink/viz: conservative tape shrinker, signature-preserving wrapper, JSON
  schema version, and self-contained HTML trace rendering.
- Testkit: same-seed assertions, replay identity, seed ranges, experiment
  reports, matrix summaries, normalized failure signatures, recall assertions,
  and failure artifact generation.
- Protocols: `detersim-protocols` now owns reusable SUT logic for
  primary-backup KV and a minimal three-node Mini-Raft reference object.

## Protocol benchmark status

- Replicated KV is a formal benchmark:
  - correct three-node primary-backup negative control,
  - `AckBeforeReplicate`,
  - `ReadFromStaleFollower`,
  - `LostUpdate`,
  - `DuplicateRequestReapplied`,
  - `FollowerAppliesUncommitted`,
  - `QuorumCountOffByOne`.
- Mini-Raft is a minimal reference protocol, not production Raft:
  - covered concepts: persisted term/vote/log probe, leader-style append, quorum
    ack, follower append, client write/read smoke path, and crash/restart
    recovery probe,
  - covered bug signatures: missing term/vote persistence, wrong commit rule,
    wrong log matching, dual leader, follower stale read, duplicate request,
    apply-before-commit, and old-term commit.

## Explicit non-claims

- Mini-Raft is not a complete Raft implementation. It intentionally excludes
  membership change, snapshots, joint consensus, log compaction, real transport,
  and production backpressure.
- v1.0 determinism means same binary and same platform. Cross-platform
  byte-identical traces are not a v1.0 promise.
- `detersim-protocols` contains reference SUTs for experiments. It must not
  depend on `detersim-sim`, `detersim-testkit`, or `detersim-viz`.

## Required gates

```powershell
cargo fmt --all --check
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
bash scripts/lint_determinism.sh
$env:DST_SEED_COUNT='10000'; cargo test --release --test determinism_meta
```

Protocol gates:

```powershell
cargo test -p detersim-testkit --test partitioned_register
cargo test -p detersim-testkit --test replicated_kv
cargo test -p detersim-sim --test mini_raft_recall
```

## Remaining v1.0 hardening

- Replace label-only Mini-Raft bug recalls with checker-backed histories for
  the variants where a small linearizable state machine can express the bug.
- Add label-aware shrink prioritization using tape labels once labels are
  exposed through the public shrink/testkit boundary.
- Extend HTML artifact views with explicit conflict highlighting from
  `CheckerStats`.
- Keep CI/nightly artifact upload wired to the same gates listed above.
