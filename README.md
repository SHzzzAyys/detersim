# detersim

[![ci](https://github.com/SHzzzAyys/detersim/actions/workflows/ci.yml/badge.svg)](https://github.com/SHzzzAyys/detersim/actions/workflows/ci.yml)
[![nightly-soak](https://github.com/SHzzzAyys/detersim/actions/workflows/nightly-soak.yml/badge.svg)](https://github.com/SHzzzAyys/detersim/actions/workflows/nightly-soak.yml)

A from-scratch **Deterministic Simulation Testing (DST)** framework: run a
distributed/concurrent system single-threaded, with logical time and injectable
faults, such that **the same seed reproduces a byte-identical execution**.

This repository started as the **Phase 0 scaffold** and now includes the first
usable slices of Phases 1-6. The implementation still keeps the Phase 0
discipline: determinism is the first gate for every feature.
See `PRD.md` for the full design, `AGENTS.md` for the rules every change must
respect, and `ROADMAP.md` for the phased task list.

## What works today

- `detersim-core`: the capability traits a SUT is written against
  (`Env` / `Clock` / `Rng` / `Network` / `Storage`) plus `SimTime` and a
  deterministic `SplitMix64` RNG. `ClockExt::timeout` is provided without a
  `Send` bound. **Zero dependencies.**
- `detersim-sim`: a single-threaded, event-queue cooperative executor; a
  simulated unreliable/unordered network with tape-driven delays; an in-memory
  `PageStore` with `flush` durability; task `spawn`/join; cancellable sleep,
  recv, and join waits; an `EntropyTape` (generate + replay); and a `World`
  that runs to quiescence. **Zero third-party dependencies.**
- `detersim-nemesis`: deterministic fault actions, connectivity matrices, and
  plan primitives (`ScriptedPlan`, `RandomLinkFault`, `RandomPartition`,
  `Composite`) for partitions, crash/restart, clock skew, bit rot, torn writes,
  and lost-on-crash storage.
- `detersim-protocols`: reusable reference SUTs written only against `Env`.
  It currently contains primary-backup KV and a minimal Mini-Raft reference
  object used by recall experiments. It does not depend on `detersim-sim`.
- `detersim-check`: structured histories, deterministic step-budgeted
  linearizability checking, coarse minimal counterexamples, and built-in
  register/KV/append-log models.
- `detersim-shrink` and `detersim-viz`: conservative tape shrinking plus local
  label-aware tape shrinking plus local JSON/HTML debug artifact helpers.
- `detersim-testkit`: reusable assertions for same-seed determinism, seed ranges,
  generate-vs-replay equality, structured failure signatures, experiment recall
  reports, plant-a-bug recall, and minimized failure artifacts.
- `detersim-cli`: local artifact and suite commands layered above public APIs;
  it is intentionally outside the deterministic core.
- The **determinism meta-test** (the master oracle): same seed ⇒ byte-identical
  event trace, across pingpong, spawn/join, multi-node gossip, timeout
  cancellation, network partition, WAL crash recovery, bit rot, and a toy
  Raft-shaped replication smoke test.

## Run it

```bash
# Build everything
cargo build --workspace

# Unit tests + the determinism meta-test
cargo test --workspace

# See a deterministic, seed-varying trace
cargo run -p detersim-sim --example pingpong
DST_SEED=7 cargo run -p detersim-sim --example pingpong

# Run the Raft-shaped reference smoke test
cargo run -p detersim-sim --example toy_raft

# Run failure-focused examples
cargo run -p detersim-sim --example wal
cargo run -p detersim-sim --example partition_dual_leader

# Find, replay, shrink, and export a failure artifact
cargo run -p detersim-testkit --example debug_failure

# Real fault -> real oracle -> replay/shrink loop
cargo test -p detersim-testkit --test partitioned_register
cargo test -p detersim-testkit --test replicated_kv

# Experiment harness and fault matrices
cargo test -p detersim-testkit --test experiment_matrix
cargo test -p detersim-testkit --test experiment_suite
cargo test -p detersim-sim --test nemesis_faults
cargo test -p detersim-sim --test storage_faults
cargo test -p detersim-sim --test mini_raft_recall
cargo test -p detersim-shrink --test label_aware_shrink
cargo test -p detersim-viz --test debug_artifact

# CLI smoke paths
cargo run -p detersim-cli -- run-suite
cargo run -p detersim-cli -- shrink
cargo run -p detersim-cli -- render 0 target/detersim-artifacts/missing-message.html
cargo run -p detersim-testkit --example v2_artifacts

# Determinism soak over many seeds (release)
DST_SEED_COUNT=10000 cargo test --release --test determinism_meta

# Cheap forbidden-API gate (the fast counterpart to the meta-test)
bash scripts/lint_determinism.sh
```

See also:

- `docs/status-v1-progress.md` for current v1.0 status and non-claims.
- `docs/tutorial-first-sut.md` for the first user-owned `Env` SUT.
- `docs/tutorial-debug-failure.md` for seed → replay → shrink → artifact.
- `docs/design-determinism.md` for the core determinism model.
- `docs/protocol-benchmarks.md` for KV and Mini-Raft benchmark design.
- `docs/experiments/replicated-kv.md` for the formal KV recall benchmark.
- `docs/experiments/mini-raft.md` for the minimal Mini-Raft reference
  experiment.
- `docs/api-guide.md` for crate boundaries and public API usage.

## The one rule

Determinism is sacred. If a change can make two runs with the same seed diverge,
it is wrong — no matter how clean. Every random/time/IO decision goes through an
`Env` capability; every control-plane draw goes through the `EntropyTape`. The
meta-test exists to catch any leak. See `AGENTS.md`.

## Status

Phase 0 gates are green, and the repo now contains executable core slices of
Phases 1-6. v0.2 hardened replay diagnostics, restart outcomes, and the reusable
test harness. The v1.0 line adds a protocol crate, formal Replicated KV recall
benchmark, checker stats artifacts, signature-preserving shrink wrapper, JSON
schema versioning, and CI/nightly gate definitions.

The V2 line adds labeled tape diagnostics, label-aware shrinking, suite-level
experiment reports, schema-v2 debug artifacts, checker-backed Mini-Raft stale
read recall, CLI smoke commands, and GitHub contribution templates.

The Replicated KV suite has positive and negative controls for
ack-before-replication, stale follower reads, lost updates, duplicate request
replay, uncommitted follower reads, and quorum off-by-one. Mini-Raft is still a
minimal reference protocol, not production Raft; it exists to prove deterministic
recall over Raft-shaped failure signatures before any larger Raft/VSR work.
See `docs/status-v1-progress.md` and `ROADMAP.md`.

License: MIT OR Apache-2.0; see `LICENSE-MIT` and `LICENSE-APACHE`.
