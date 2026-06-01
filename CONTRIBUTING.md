# Contributing to DeterSim

DeterSim is a deterministic simulation testing framework. Contributions are
welcome, but every change has to preserve the project’s core contract:

> Same binary + same platform + same seed/tape => byte-identical trace.

This document describes how to make changes without weakening that contract.

## Project Priorities

Order matters:

1. Determinism.
2. Replay soundness.
3. Signature-preserving shrink.
4. Structured oracles and explainable artifacts.
5. New functionality.

If a feature is useful but makes same-seed execution unstable, it is not ready
for this repository.

## What Belongs Where

- `detersim-core`: traits and deterministic capability types only.
- `detersim-sim`: the single-threaded runtime, event queue, simulated
  network/storage, tape, replay, and `World`.
- `detersim-nemesis`: pure fault plan data and deterministic plan logic.
- `detersim-check`: structured histories, invariant helpers, checker models,
  checker artifacts.
- `detersim-protocols`: reusable reference SUTs written against `Env`.
- `detersim-shrink`: tape minimization that preserves the same failure.
- `detersim-testkit`: public harness helpers, experiment reports, recall APIs.
- `detersim-search`: seed/tape search over public experiment APIs.
- `detersim-net`: deterministic stream helpers, not real sockets.
- `detersim-viz`: static JSON/HTML artifact generation.
- `detersim-cli`: local user workflow glue. This crate may use local file I/O.

Do not introduce dependency cycles. Do not make low-level deterministic crates
depend on `testkit`, `search`, `viz`, or `cli`.

## Determinism Rules

Do not use these in deterministic crates, examples, or tests:

- real time: `Instant::now`, `SystemTime::now`, `.elapsed()`
- real scheduling: `std::thread`, `tokio::spawn`, `spawn_blocking`, rayon
- real network: `std::net`, `tokio::net`
- real files: `std::fs`, `tokio::fs`
- system entropy: `thread_rng`, `rand::random`
- order-sensitive `HashMap` / `HashSet` iteration
- pointer identity as behavior
- mutable global state that affects execution

Use these instead:

- `Clock`
- `Rng`
- `Network`
- `Storage`
- `Spawn`
- `EntropyTape::draw(label)`
- `BTreeMap`, `BTreeSet`, and deterministic vectors

If a new control-plane decision is random, it must draw from the entropy tape
with a stable label. Labels should describe the decision, not an incidental file
or line number.

## Adding a New Experiment

A good experiment has both a positive and negative control:

- correct implementation does not fail under the configured seed budget
- plant-a-bug implementation is recalled within budget
- generate -> replay -> shrink preserves the same `FailureSignature`
- oracle uses structured checker output when possible
- artifact includes trace, history, nemesis trace, replay diagnostics, shrink
  stats, and checker/search data if relevant

Prefer `ExperimentCase` and `ExperimentSuite` over ad hoc loops. Avoid tests that
only search for incidental trace substrings when a structured history or
invariant label would be stable.

## Adding a New Checker

Checker changes must include:

- a correct history that passes
- a known bad history that fails
- a legal concurrent reorder that passes when applicable
- a low-budget case that returns `Inconclusive`, not failure
- stable artifact fields for witnesses, conflicts, explored states, and budget
  exhaustion

Search order must remain deterministic. Use deterministic containers and stable
operation IDs.

## Adding a New Fault

Fault changes must include:

- a deterministic representation in nemesis or sim fault config
- entropy tape labels for probabilistic decisions
- trace/history visibility sufficient for artifact explanation
- at least one positive-control test
- at least one plant-a-bug or observability test
- replay and shrink coverage when the fault can cause a real failure

Do not implement a fault by calling real time, threads, sockets, files, or system
RNG.

## Adding or Changing Public APIs

Public APIs should be small and explicit. Add rustdoc for new public types and
functions. Prefer stable data structures over stringly behavior when a result is
meant to be consumed by testkit, search, viz, or CLI.

Compatibility rules:

- Keep V2 artifact rendering working.
- Use `schema_version` for artifact shape changes.
- Do not remove existing public APIs without a replacement path.
- Keep `Inconclusive` distinct from failure.

## Local Validation

Run the full gate before sending changes:

```powershell
cargo fmt --all --check
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
bash scripts/lint_determinism.sh
$env:DST_SEED_COUNT='10000'; cargo test --release --test determinism_meta
```

For V3-facing changes, also run:

```powershell
cargo test -p detersim-search --test coverage_guided_search
cargo test -p detersim-check --test checker_v3_models
cargo test -p detersim-net --test stream_api
cargo test -p detersim-viz --test debug_artifact_v3
cargo test -p detersim-cli --test cli_smoke
cargo run -p detersim-cli -- doctor
```

For release candidates or risky runtime changes, run the manual full-soak
workflow or the equivalent local commands from `docs/release-checklist.md`.

## Pull Request Expectations

Every PR should state:

- what changed
- which deterministic boundary it touches
- whether replay/shrink/artifacts are affected
- which gates were run
- known limitations or non-claims

If a gate fails, do not mark it ignored, reduce its coverage, or retry it away.
Either fix the determinism leak or explain why the PR should not merge yet.

## Documentation Expectations

Update docs when changing:

- public APIs
- CLI commands
- artifact schema
- experiment semantics
- fault behavior
- checker result interpretation
- release or validation commands

README should remain conservative. Do not claim production Raft, cross-platform
byte identity, transparent Tokio interception, or complete Elle support unless
those are actually implemented and tested.

## Reporting Problems

Use the issue templates:

- Bug report: normal runtime/checker/shrink/artifact bug.
- Determinism leak: same seed diverges.
- Experiment proposal: new protocol or plant-a-bug benchmark.
- Documentation issue: docs are stale, confusing, or overclaiming.

For determinism leaks, attach the seed, command, trace/history/tape divergence,
and any minimized artifact if available.
