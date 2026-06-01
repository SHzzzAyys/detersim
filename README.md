# DeterSim

[![ci](https://github.com/SHzzzAyys/detersim/actions/workflows/ci.yml/badge.svg)](https://github.com/SHzzzAyys/detersim/actions/workflows/ci.yml)
[![nightly-soak](https://github.com/SHzzzAyys/detersim/actions/workflows/nightly-soak.yml/badge.svg)](https://github.com/SHzzzAyys/detersim/actions/workflows/nightly-soak.yml)

DeterSim is a from-scratch **Deterministic Simulation Testing (DST)** framework
for distributed and concurrent Rust systems. It runs a system under test inside a
single-threaded logical world with simulated time, simulated network/storage, an
entropy tape, replay, shrinking, fault injection, consistency checkers, and
static debug artifacts.

The core promise is deliberately narrow and testable:

> Same binary + same platform + same seed/tape => byte-identical execution trace.

DeterSim is currently a beta-stage research and debugging toolchain. It is useful
for building deterministic protocol experiments, reproducing known distributed
systems bugs, and producing explainable failure artifacts. It is not a production
Raft implementation, not a transparent Tokio interceptor, and not a real network
stack.

## Why This Exists

Distributed bugs are hard because the interesting failures hide in timing,
message order, crash points, persistence boundaries, and retry behavior. Normal
tests either avoid that state space or explore it with real threads, real clocks,
and real I/O, which makes the failure hard to reproduce.

DeterSim makes that state space explicit:

- All time comes from a logical `Clock`.
- All randomness comes from deterministic RNGs or the `EntropyTape`.
- All message delivery is scheduled by one event queue.
- All storage durability is modeled by simulated storage.
- All faults are data: partition, drop, duplicate, delay, crash, restart, skew,
  torn write, bit rot, lost-on-crash, and pre-fsync reorder.
- Every failing run can be replayed from the same seed and tape.
- Failing tapes can be minimized while preserving the same normalized
  `FailureSignature`.
- Debug artifacts are static JSON/HTML files that can be attached to issues or
  shared without a service.

The design goal is not to prove that a protocol is correct. The goal is to make
bug discovery, replay, shrinking, and explanation deterministic enough that a
human can trust and debug the result.

## Current Status

The repository contains executable slices from the original Phase 0-6 roadmap
plus the V2/V3 hardening work:

- Phase 0 baseline: deterministic traits, runtime scaffold, tests, and
  determinism lint.
- V1.0 line: fault injection, simulated storage, checker/testkit harness,
  Replicated KV benchmark, Mini-Raft recall scaffold, JSON/HTML artifacts.
- V2 alpha: tape labels, label-aware shrink, suite reports, checker-backed
  Mini-Raft stale-read history, CLI smoke paths, GitHub issue/PR templates.
- V3 beta branch: coverage-guided search, checker artifacts, Elle-lite
  transaction checking, deterministic stream helpers, schema-v3 debug artifacts,
  richer CLI, docs, and full-soak workflow.

Important non-claims:

- Mini-Raft is a **minimal reference benchmark**, not production Raft.
- Determinism is guaranteed only for the same binary on the same platform.
- DeterSim does not transparently intercept Tokio, OS sockets, real files, or
  production storage.
- The Elle-lite checker is intentionally small and is not a replacement for full
  Elle.
- Crates.io publishing is intentionally deferred until the beta API surface is
  stabilized.

## Repository Layout

```text
crates/
  detersim-core/       Env, Clock, Rng, Network, Storage, Spawn traits, SimTime
  detersim-sim/        single-threaded deterministic runtime and World
  detersim-nemesis/    fault actions, connectivity matrices, nemesis plans
  detersim-check/      invariants, linearizability, Elle-lite transaction checks
  detersim-protocols/  reference SUTs: primary-backup KV and Mini-Raft
  detersim-shrink/     signature-preserving entropy tape minimization
  detersim-testkit/    user-facing assertions, suites, recall reports
  detersim-search/     random, coverage-guided, and failure-directed seed search
  detersim-net/        deterministic stream/socket-shaped helper model
  detersim-viz/        JSON and self-contained HTML debug artifacts
  detersim-cli/        local CLI for suites, search, replay, shrink, render
docs/                  tutorials, experiment design notes, release checklist
scripts/               determinism lint and project gates
.github/               CI, nightly soak, full soak, issue and PR templates
```

Dependency direction matters:

- `detersim-core` has no internal dependencies.
- `detersim-protocols` depends only on `core` and `check`.
- `detersim-sim` depends on `core` and `nemesis`.
- `testkit`, `search`, `viz`, and `cli` sit above the deterministic runtime.
- `detersim-cli` may do local file I/O; deterministic crates must not.

## Quick Start

Prerequisites:

- Rust stable toolchain.
- Git.
- Bash for `scripts/lint_determinism.sh` on Windows, for example Git Bash or
  WSL. PowerShell is fine for all Cargo commands.

Clone and run:

```powershell
git clone https://github.com/SHzzzAyys/detersim.git
cd detersim
cargo build --workspace
cargo test --workspace
```

Run a deterministic trace example:

```powershell
cargo run -p detersim-sim --example pingpong
$env:DST_SEED='7'; cargo run -p detersim-sim --example pingpong
```

Run the main local gates:

```powershell
cargo fmt --all --check
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
bash scripts/lint_determinism.sh
$env:DST_SEED_COUNT='10000'; cargo test --release --test determinism_meta
```

## CLI

The CLI is intentionally outside the deterministic core. It can read/write local
files and assemble public APIs into user workflows.

```powershell
# Check local toolchain and sample deterministic flow.
cargo run -p detersim-cli -- doctor

# Run the built-in smoke suite and print JSON.
cargo run -p detersim-cli -- run-suite

# Search for high-signal seeds.
cargo run -p detersim-cli -- search --budget 100 --strategy coverage-guided

# Replay a generated tape.
cargo run -p detersim-cli -- replay 0 12,34,56

# Shrink a built-in failing run and print a debug artifact JSON.
cargo run -p detersim-cli -- shrink

# Render a static HTML artifact.
cargo run -p detersim-cli -- render 0 target/detersim-artifacts/missing-message.html

# Produce a schema-v3 explanation artifact.
cargo run -p detersim-cli -- explain target/detersim-artifacts/v3-explain.json

# Create a minimal external SUT template.
cargo run -p detersim-cli -- init-sut target/detersim-sut-template
```

The current CLI is a beta interface. Prefer using it for local workflows and
examples, but do not treat the command surface as stable until the project
publishes a non-beta API stability note.

## Writing a SUT

A SUT should be written against the `Env` trait, not against `std::time`,
`std::net`, `std::fs`, threads, or OS randomness.

Minimal shape:

```rust
use std::time::Duration;

use detersim_core::{ClockExt, Env, Network};

pub async fn node<E: Env>(env: E) {
    let net = env.net();
    let result = env
        .clock()
        .timeout(Duration::from_millis(20), net.recv())
        .await;

    if result.is_err() {
        env.record("timeout");
    }
}
```

Run it inside a `World`:

```rust
use detersim_sim::{SimEnv, World};

let mut world = World::new(0);
world.add_node(0, |env: SimEnv| async move {
    node(env).await;
});
let report = world.run();
assert!(!report.deadlocked);
```

For a complete walkthrough, see
[`docs/tutorial-first-sut.md`](docs/tutorial-first-sut.md).

## Experiments

DeterSim treats “bug finding” as an experiment with a hypothesis, a budget, an
oracle, and an artifact. The testkit layer provides:

- `FailureSignature`: normalized failure identity.
- `ExperimentCase`: generate, replay, oracle, and budget.
- `ExperimentSuite`: multiple positive and negative controls.
- `ExperimentReport`: seed attempts, first failing seed, replay diagnostics,
  shrink ratio, artifact sizes, and signature preservation.
- `RecallPolicy`: whether a case must recall, must not recall, or is
  informational.

Run the main experiment suites:

```powershell
cargo test -p detersim-testkit --test partitioned_register
cargo test -p detersim-testkit --test replicated_kv
cargo test -p detersim-testkit --test experiment_matrix
cargo test -p detersim-testkit --test experiment_suite
cargo test -p detersim-sim --test mini_raft_recall
```

Current protocol benchmarks:

- Partitioned register baseline: small linearizability failure under partition.
- Replicated KV: correct primary-backup plus plant-a-bug variants for
  ack-before-replication, stale follower read, lost update, duplicate request
  replay, uncommitted follower read, and quorum off-by-one.
- Mini-Raft reference: term/vote/log persistence, stale reads, duplicate client
  requests, dual leaders, wrong commit/log matching, apply-before-commit, and
  old-term commit variants.

See [`docs/protocol-benchmarks.md`](docs/protocol-benchmarks.md),
[`docs/experiments/replicated-kv.md`](docs/experiments/replicated-kv.md), and
[`docs/experiments/mini-raft.md`](docs/experiments/mini-raft.md).

## Search

`detersim-search` runs public `ExperimentCase` values repeatedly and ranks seeds
by stable signals from `RunReport`.

Strategies:

- `Random`: baseline seed order.
- `CoverageGuided`: prefers seeds that expose new semantic coverage.
- `FailureDirected`: prefers failure signatures and high-signal candidates.

Coverage signals include tape labels, message edges, trace outcomes, nemesis
events, network drops, timers, task polls, and history classes.

```powershell
cargo test -p detersim-search --test coverage_guided_search
cargo run -p detersim-cli -- search --budget 5000 --strategy coverage-guided
```

Search is only a discovery accelerator. A failure is accepted only after replay
and signature-preserving shrink keep the same `FailureSignature`.

## Replay and Shrinking

Every generated run records:

- `seed`
- `tape_log`
- `tape_events`
- `trace`
- `history`
- `nemesis_trace`
- replay diagnostics
- semantic coverage signals

Replay uses the same seed and generated tape:

```rust
let generated = generate(seed);
let replayed = replay(seed, generated.tape_log.clone());
assert_eq!(generated.trace, replayed.trace);
```

Shrinking removes entropy draws while requiring the same normalized failure
signature to remain true. Label-aware shrinking tries lower-priority draws first
so that the minimized tape tends to keep causal events such as partitions,
crashes, restarts, and storage faults.

```powershell
cargo test -p detersim-shrink --test label_aware_shrink
cargo run -p detersim-cli -- shrink
```

## Checkers

`detersim-check` is deliberately small and deterministic. It uses step budgets,
structured histories, and deterministic enumeration order.

Supported models:

- Register.
- Single-key KV.
- Multi-key KV.
- Append-only log.
- Elle-lite transaction histories for compact serializability checks.

Important result rules:

- `Linearizable` / `Serializable` means a valid sequential order was found.
- `NotLinearizable` / `NotSerializable` is a real failure.
- `Inconclusive` is not a failure; it means the deterministic budget was too
  low.

Checker artifacts include witness order, conflict operation IDs, minimal
subhistory IDs, explored state count, and budget exhaustion.

```powershell
cargo test -p detersim-check --test checker_v3_models
```

See [`docs/checker-models.md`](docs/checker-models.md).

## Fault Injection

Faults are deterministic and reproducible. They are either scripted through
`NemesisAction` / `NemesisPlan` or drawn through the entropy tape.

Current fault families:

- Network: partition, asymmetric connectivity, drop, duplicate, delay.
- Process: crash, restart, invalid restart diagnostics.
- Storage: lost-on-crash, torn write, bit rot, pre-fsync reorder.
- Clock: per-node skew while preserving local monotonicity.

Run the fault tests:

```powershell
cargo test -p detersim-sim --test nemesis_faults
cargo test -p detersim-sim --test storage_faults
```

## Debug Artifacts

`detersim-viz` exports JSON and self-contained HTML. The HTML viewer does not
load remote fonts, scripts, CDNs, or services.

Schema v2 covers:

- run report
- trace
- history
- nemesis trace
- tape and tape labels
- replay diagnostics

Schema v3 adds:

- experiment report
- search report
- checker artifact
- shrink report
- failure signature
- semantic coverage
- causal graph
- environment/build metadata

Render examples:

```powershell
cargo test -p detersim-viz --test debug_artifact
cargo test -p detersim-viz --test debug_artifact_v3
cargo run -p detersim-cli -- render --examples target/detersim-artifacts/v3
cargo run -p detersim-testkit --example v3_artifacts
```

## Deterministic Stream Helpers

`detersim-net` models socket-shaped frame streams without real sockets. It is a
pure deterministic helper layer for protocols that are easier to describe as
ordered frames rather than raw one-shot messages.

It provides:

- `ConnectionId`
- `StreamEndpoint`
- `Frame`
- `StreamFault`
- `StreamTranscript`
- `DeterministicStream`

```powershell
cargo test -p detersim-net --test stream_api
```

See [`docs/tutorial-stream-api.md`](docs/tutorial-stream-api.md).

## Determinism Rules

This is the project’s prime directive:

> A change is wrong if it can make two runs with the same seed diverge.

Do not use these in deterministic crates or SUT paths:

- `std::time::Instant::now`
- `SystemTime::now`
- real threads or Tokio tasks
- OS RNG or `rand::random`
- `std::net` / `tokio::net`
- `std::fs` / `tokio::fs`
- iteration over `HashMap` / `HashSet` when order can affect behavior
- pointer identity for control flow
- global mutable state that affects behavior

Use these instead:

- `Clock`
- `Rng`
- `Network`
- `Storage`
- `Spawn`
- `EntropyTape`
- `BTreeMap` / `BTreeSet` / deterministic vectors

The cheap static gate is:

```powershell
bash scripts/lint_determinism.sh
```

The stronger behavioral gate is:

```powershell
$env:DST_SEED_COUNT='10000'; cargo test --release --test determinism_meta
```

## Development Workflow

Before opening a PR:

```powershell
cargo fmt --all --check
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
bash scripts/lint_determinism.sh
$env:DST_SEED_COUNT='10000'; cargo test --release --test determinism_meta
```

V3 fast gates:

```powershell
cargo test -p detersim-search --test coverage_guided_search
cargo test -p detersim-check --test checker_v3_models
cargo test -p detersim-net --test stream_api
cargo test -p detersim-viz --test debug_artifact_v3
cargo test -p detersim-cli --test cli_smoke
cargo run -p detersim-cli -- doctor
```

Manual full soak is available through `.github/workflows/full-soak.yml`.

## Documentation Map

- [`docs/getting-started.md`](docs/getting-started.md): first orientation.
- [`docs/tutorial-first-sut.md`](docs/tutorial-first-sut.md): write a small SUT.
- [`docs/tutorial-debug-failure.md`](docs/tutorial-debug-failure.md): seed →
  replay → shrink → artifact.
- [`docs/tutorial-coverage-guided-search.md`](docs/tutorial-coverage-guided-search.md):
  coverage-guided search workflow.
- [`docs/tutorial-stream-api.md`](docs/tutorial-stream-api.md): deterministic
  stream helper usage.
- [`docs/design-determinism.md`](docs/design-determinism.md): determinism model.
- [`docs/protocol-benchmarks.md`](docs/protocol-benchmarks.md): KV and
  Mini-Raft benchmark design.
- [`docs/benchmark-evidence-v3.md`](docs/benchmark-evidence-v3.md): V3
  experiment evidence and limitations.
- [`docs/checker-models.md`](docs/checker-models.md): checker models and result
  interpretation.
- [`docs/v3-artifact-schema.md`](docs/v3-artifact-schema.md): schema-v3 debug
  artifacts.
- [`docs/status-v3.1-plan.md`](docs/status-v3.1-plan.md): V3.1 hardening
  status and gates.
- [`docs/mini-raft-checker-backed.md`](docs/mini-raft-checker-backed.md):
  Mini-Raft checker-backed and invariant-backed oracle split.
- [`docs/search-benchmark-results.md`](docs/search-benchmark-results.md):
  search comparison evidence and commands.
- [`docs/artifact-causal-graph.md`](docs/artifact-causal-graph.md): causal
  graph schema used by V3 artifacts.
- [`docs/public-api-stability.md`](docs/public-api-stability.md): beta API
  stability rules.
- [`docs/versioning.md`](docs/versioning.md): release, crate, and artifact
  versioning.
- [`docs/crates-publishing.md`](docs/crates-publishing.md): crates.io dry-run
  checklist.
- [`docs/release-checklist.md`](docs/release-checklist.md): release gates and
  tag commands.
- [`CHANGELOG.md`](CHANGELOG.md): release notes and known limitations.
- [`SECURITY.md`](SECURITY.md): security reporting scope.
- [`AGENTS.md`](AGENTS.md): engineering rules for automated coding agents.
- [`ROADMAP.md`](ROADMAP.md): phased roadmap and stretch goals.
- [`PRD.md`](PRD.md): original product/design requirements.

## Contributing

Contributions should preserve determinism first and feature scope second.

Good contributions:

- Add a fault with a positive and negative control.
- Add a checker model with self-tests and inconclusive handling.
- Improve debug artifacts without external dependencies.
- Add a protocol benchmark with replay, shrink, and a normalized failure
  signature.
- Improve docs with exact commands and current limitations.

Bad contributions:

- Bypassing `Env` with real time, real threads, real files, real sockets, or
  system RNG.
- Adding a failure test that only matches arbitrary strings and cannot replay.
- Expanding Mini-Raft claims beyond the current reference-benchmark scope.
- Making artifacts require an external service to render.

See [`CONTRIBUTING.md`](CONTRIBUTING.md) if present in your checkout, plus the PR
template and issue templates under `.github/`.

## License

Licensed under either of:

- Apache License, Version 2.0 ([`LICENSE-APACHE`](LICENSE-APACHE))
- MIT license ([`LICENSE-MIT`](LICENSE-MIT))

at your option.
