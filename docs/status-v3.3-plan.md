# DeterSim V3.3 Adoption Quality Plan

V3.3 starts after `v3.2.0-beta.1`. The goal is adoption quality, not new
runtime surface area.

## Scope

V3.3 hardens:

- CLI end-to-end workflows.
- Generated SUT templates.
- Suite manifest evidence metadata.
- Sparse search evidence.
- Artifact causal explanation.
- Release and packaging dry-run readiness.

It does not add production Raft, real sockets, transparent Tokio interception,
full Elle, cross-platform byte identity, or stable crates.io API guarantees.

## Implemented Direction

- `detersim-cli doctor --deep` exercises template generation, template tests,
  and artifact rendering.
- `init-sut` supports `message`, `stream`, and `protocol` templates.
- `run-suite` emits suite manifest and summary JSON for real built-in suites.
- `search --compare` emits dense/sparse classification and first-failure
  distribution fields.
- `sparse-discovery` provides sparse smoke cases for search evidence.
- `render` and `explain` support artifact path workflows.
- `detersim-testkit` suite manifests include case family, bug variant, control
  kind, expected recall, and evidence class.

## Evidence Principles

- Dense every-seed-fails cases are evidence for reporting, replay, shrink, and
  artifact stability.
- Sparse cases are required before claiming search acceleration.
- Negative controls must not be counted as successful recalls.
- `Inconclusive` checker results must not be promoted to failures.
- Shrink success requires the same normalized `FailureSignature`.

## V3.3 Focused Gates

```powershell
cargo test -p detersim-testkit --test experiment_suite_manifest
cargo test -p detersim-search --test suite_search_comparison
cargo test -p detersim-viz --test artifact_schema_compat
cargo test -p detersim-cli --test cli_e2e
cargo test -p detersim-cli --test cli_benchmark_flow
cargo test -p detersim-cli --test cli_artifact_workflow
cargo run -p detersim-cli -- doctor --deep
cargo run -p detersim-cli -- search --suite sparse-discovery --compare --budget 32
```

## Release Stop Rules

- Stop if generated templates fail `cargo test`.
- Stop if artifact schema compatibility breaks v2 or v3 readers.
- Stop if `sparse-discovery` no longer marks sparse cases as sparse.
- Stop if CLI reports unsupported suites as successful runs.
- Stop if deterministic crates require real time, threads, network, files, or
  system RNG.
