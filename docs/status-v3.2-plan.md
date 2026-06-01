# DeterSim V3.2 Adoption Evidence Plan

V3.2 starts after the `v3.1.0-beta.1` tag. The goal is not a new runtime layer;
it is to make the beta line easier to adopt and easier to audit.

## Completed Baseline

- V3.1 is merged to `main` and tagged as `v3.1.0-beta.1`.
- Mini-Raft has checker-backed and invariant-backed recall tests.
- Search has single-case strategy comparison.
- Artifact v3 has a stable `CausalGraph { nodes, edges }` shape.
- CLI supports real suite aliases for smoke, Replicated KV, Mini-Raft smoke, and
  storage faults.

## V3.2 Scope

- Add suite manifests in `detersim-testkit` so a suite describes case names,
  oracle kinds, expected signatures, budgets, policies, and artifact behavior.
- Promote CLI `run-suite` from single-case smoke output to multi-case suite JSON
  with policy results and manifest metadata.
- Add suite-level search comparison for `Random`, `CoverageGuided`, and
  `FailureDirected`.
- Strengthen artifact v3 causal graphs while keeping schema version `3`.
- Add explicit CLI path chaining:
  `run-suite --out`, `search --out`, `shrink --case --seed --out`, and
  `render --artifact --out`.
- Keep unsupported suite names explicit; the CLI must not pretend to run a
  benchmark it does not implement.

## Non-Goals

- No production Raft claim.
- No transparent Tokio interception.
- No real socket adapter.
- No full Elle checker.
- No cross-platform byte identity guarantee.
- No stable crates.io API promise in this beta line.

## Acceptance

```powershell
cargo test -p detersim-testkit --test experiment_suite_manifest
cargo test -p detersim-search --test suite_search_comparison
cargo test -p detersim-viz --test artifact_schema_compat
cargo test -p detersim-cli --test cli_artifact_workflow
cargo run -p detersim-cli -- run-suite --suite replicated-kv
cargo run -p detersim-cli -- search --suite mini-raft-smoke --compare --budget 1000
cargo run -p detersim-cli -- render --examples target/detersim-artifacts/v3
```

The global release gates remain unchanged: format, build, workspace tests,
clippy with warnings denied, determinism lint, documentation build, and 10k
release determinism soak.
