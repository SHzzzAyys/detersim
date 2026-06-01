# DeterSim V3.1 status and completion plan

V3.1 starts from the `v3.0.0-beta.1` baseline. The beta platform already has
runtime replay, experiment reports, search smoke tests, static artifacts, and a
local CLI. V3.1 hardens the proof layer: Mini-Raft bugs must be checker-backed
where they affect client history, search must produce comparison evidence, and
artifacts must carry a stable causal graph.

## Completed in this line

- Mini-Raft exposes `RaftObservation`, `RaftInvariantEvent`, and
  `RaftClientHistory` as public protocol observations.
- Client-visible Mini-Raft variants now have checker-readable histories.
- `detersim-search` can compare strategies with `SearchComparisonReport`.
- `detersim-viz` can build deterministic schema-v3 causal graphs from a run.
- `detersim-cli` supports real suite names: `smoke`, `replicated-kv`,
  `mini-raft-smoke`, and `storage-faults`.

## Release gates

```powershell
cargo fmt --all --check
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
bash scripts/lint_determinism.sh
$env:DST_SEED_COUNT='10000'; cargo test --release --test determinism_meta
```

## V3.1 focused gates

```powershell
cargo test -p detersim-sim --test mini_raft_recall
cargo test -p detersim-search --test search_comparison
cargo test -p detersim-shrink --test signature_preserving_shrink
cargo test -p detersim-viz --test causal_artifact_v3
cargo test -p detersim-cli --test cli_benchmark_flow
cargo run -p detersim-cli -- run-suite --suite replicated-kv
cargo run -p detersim-cli -- search --suite replicated-kv --budget 500 --strategy coverage-guided
```

## Non-goals

V3.1 still does not claim production Raft, transparent Tokio interception, real
socket adapters, full Elle, cross-platform byte identity, or stable crates.io
APIs.
