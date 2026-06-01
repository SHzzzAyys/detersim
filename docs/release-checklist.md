# Release checklist

## Pre-release truth check

- README distinguishes completed benchmarks from scaffolds.
- `docs/status-v1-progress.md` matches the current crate layout and tests.
- `CHANGELOG.md` has an entry for the release.
- `SECURITY.md`, `CODE_OF_CONDUCT.md`, `docs/versioning.md`, and
  `docs/crates-publishing.md` are linked from README.
- `docs/benchmark-evidence-v3.md` reflects the current test targets.
- `docs/status-v3.2-plan.md` reflects the current beta adoption/evidence scope.
- Mini-Raft claims remain "minimal reference protocol", not production Raft.
- Every public API added for the release has rustdoc.
- Every deterministic SUT path is included in `scripts/lint_determinism.sh`.

## Required local gates

```powershell
cargo fmt --all --check
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
bash scripts/lint_determinism.sh
$env:DST_SEED_COUNT='10000'; cargo test --release --test determinism_meta
```

## Experiment gates

```powershell
cargo test -p detersim-testkit --test partitioned_register
cargo test -p detersim-testkit --test replicated_kv
cargo test -p detersim-testkit --test experiment_matrix
cargo test -p detersim-testkit --test experiment_suite
cargo test -p detersim-testkit --test experiment_suite_manifest
cargo test -p detersim-sim --test nemesis_faults
cargo test -p detersim-sim --test storage_faults
cargo test -p detersim-sim --test mini_raft_recall
cargo test -p detersim-shrink --test label_aware_shrink
cargo test -p detersim-viz --test debug_artifact
cargo test -p detersim-search --test coverage_guided_search
cargo test -p detersim-search --test search_comparison
cargo test -p detersim-search --test suite_search_comparison
cargo test -p detersim-check --test checker_v3_models
cargo test -p detersim-net --test stream_api
cargo test -p detersim-viz --test debug_artifact_v3
cargo test -p detersim-viz --test causal_artifact_v3
cargo test -p detersim-viz --test artifact_schema_compat
cargo test -p detersim-cli --test cli_smoke
cargo test -p detersim-cli --test cli_e2e
cargo test -p detersim-cli --test cli_benchmark_flow
cargo test -p detersim-cli --test cli_artifact_workflow
cargo run -p detersim-cli -- doctor
cargo run -p detersim-testkit --example v3_artifacts
cargo run -p detersim-cli -- run-suite --suite replicated-kv --out target/detersim-artifacts/replicated-kv-suite.json
cargo run -p detersim-cli -- search --suite replicated-kv --budget 500 --strategy coverage-guided
cargo run -p detersim-cli -- search --suite mini-raft-smoke --compare --budget 1000
cargo run -p detersim-cli -- shrink --case missing-message --seed 0 --out target/detersim-artifacts/missing-message-shrink.json
cargo run -p detersim-cli -- render --artifact target/detersim-artifacts/missing-message-shrink.json --out target/detersim-artifacts/missing-message-shrink.html
cargo doc --workspace --no-deps
```

## Artifact check

- At least one Replicated KV failure has JSON and HTML artifacts.
- At least one Mini-Raft failure has checker-backed history or a normalized
  invariant signature.
- At least one schema-v3 artifact includes a deterministic causal graph.
- Shrink report records original length, minimized length, attempts, accepted
  removals, and signature preservation.
- V2 alpha tag command:

```powershell
git tag -a v2.0.0-alpha.1 -m "DeterSim v2.0.0-alpha.1"
git push origin v2.0.0-alpha.1
```

- V3 beta tag command:

```powershell
git tag -a v3.0.0-beta.1 -m "DeterSim v3.0.0-beta.1"
git push origin v3.0.0-beta.1
```

- V3.1 beta tag command:

```powershell
git tag -a v3.1.0-beta.1 -m "DeterSim v3.1.0-beta.1"
git push origin v3.1.0-beta.1
```

- Crates.io dry run before any publish:

```powershell
cargo package --workspace --allow-dirty
cargo publish --dry-run -p detersim-core
```

## Stop rules

- Stop the release if same-seed trace equality fails.
- Stop the release if a correct negative control produces `NotLinearizable`.
- Stop the release if a plant-a-bug variant is not recalled within its budget.
- Stop the release if determinism lint flags a real forbidden API.
