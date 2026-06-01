# Release checklist

## Pre-release truth check

- README distinguishes completed benchmarks from scaffolds.
- `docs/status-v1-progress.md` matches the current crate layout and tests.
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
cargo test -p detersim-sim --test nemesis_faults
cargo test -p detersim-sim --test storage_faults
cargo test -p detersim-sim --test mini_raft_recall
cargo test -p detersim-shrink --test label_aware_shrink
cargo test -p detersim-viz --test debug_artifact
```

## Artifact check

- At least one Replicated KV failure has JSON and HTML artifacts.
- At least one Mini-Raft failure has a reproducible trace and protocol label.
- Shrink report records original length, minimized length, attempts, accepted
  removals, and signature preservation.
- V2 alpha tag command:

```powershell
git tag -a v2.0.0-alpha.1 -m "DeterSim v2.0.0-alpha.1"
git push origin v2.0.0-alpha.1
```

## Stop rules

- Stop the release if same-seed trace equality fails.
- Stop the release if a correct negative control produces `NotLinearizable`.
- Stop the release if a plant-a-bug variant is not recalled within its budget.
- Stop the release if determinism lint flags a real forbidden API.
