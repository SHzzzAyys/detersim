# V3 Benchmark Evidence

This document records the evidence DeterSim uses for the V3 beta line. The
claim is limited: these experiments recall known bug variants under deterministic
budgets and produce replay/shrink/artifact data. They do not prove a protocol is
correct.

The test targets are the source of truth. CLI suite mapping is still beta and
does not yet replace the explicit Rust test targets listed here.

## Partitioned Register Baseline

- Experiment name: partitioned register
- Hypothesis: a partitioned register can acknowledge a write that is not visible
  to a later read, producing a linearizability violation.
- SUT: small register scenario in `detersim-testkit`.
- Fault variables: network partition.
- Oracle: `SingleKeyKv` / register-style linearizability signature.
- Seed budget: 200 in the test target.
- Positive control: non-partitioned register behavior is not accepted as the
  same failure.
- Plant-a-bug result: recalled by `partitioned_register_bug_replays_and_shrinks`.
- Replay/shrink status: testkit verifies replay and minimized signature
  preservation.
- Artifact command: `cargo test -p detersim-testkit --test partitioned_register`.
- Limitations: minimal baseline, not a full protocol.

## Replicated KV

- Experiment name: replicated KV bug zoo
- Hypothesis: primary-backup mistakes around quorum, stale reads, duplicate
  requests, and uncommitted state are observable as structured history failures.
- SUT: `detersim-protocols::primary_backup_kv`.
- Fault variables: asymmetric partition, read placement, duplicate request,
  commit/replication behavior.
- Oracle: `SingleKeyKv`, `MultiKeyKv`, or append-log linearizability depending
  on the variant.
- Seed budget: 500 for the benchmark suite.
- Positive control: `correct_primary_backup_kv_is_negative_control`.
- Plant-a-bug results:
  - `AckBeforeReplicate`
  - `ReadFromStaleFollower`
  - `LostUpdate`
  - `DuplicateRequestReapplied`
  - `FollowerAppliesUncommitted`
  - `QuorumCountOffByOne`
- Replay/shrink status: `ExperimentCase` verifies generate -> replay -> shrink
  against the same normalized failure signature.
- Artifact command: `cargo test -p detersim-testkit --test replicated_kv`.
- Limitations: fixed small cluster and workload; intended as a benchmark rung,
  not a production primary-backup implementation.

## Mini-Raft Reference

- Experiment name: Mini-Raft reference recall
- Hypothesis: Raft-shaped safety bugs can be split into client-visible checker
  failures and protocol-internal invariant failures.
- SUT: `detersim-protocols::mini_raft`.
- Fault variables: stale follower reads, duplicate client requests, persistence
  omissions, wrong commit/log matching, dual leadership.
- Oracle: `SingleKeyKv` linearizability for client-visible stale reads;
  normalized `RaftInvariant` labels for internal invariants.
- Seed budget: fast tests use small budgets; full-soak may use 50k for the
  correct reference.
- Positive control: `mini_raft_reference_is_stable_under_seed_sweep`.
- Plant-a-bug results:
  - checker-backed stale follower read
  - invariant-backed dual leader
  - term/vote/log persistence probe
  - apply-before-commit
  - old-term leader commit
- Replay/shrink status: recall tests preserve normalized signatures.
- Artifact command: `cargo test -p detersim-sim --test mini_raft_recall`.
- Limitations: Mini-Raft is not production Raft. It intentionally excludes
  membership change, snapshots, joint consensus, and real transport.

## Storage / WAL Faults

- Experiment name: storage fault matrix
- Hypothesis: simulated durability boundaries expose failures that normal
  in-memory tests hide.
- SUT: WAL/storage scenarios in `detersim-sim`.
- Fault variables: crash/restart, flush, bit rot, torn write, pre-fsync reorder.
- Oracle: durable recovery history and checksum/history checks.
- Seed budget: deterministic direct tests.
- Positive control: flushed data survives crash/restart.
- Plant-a-bug results:
  - ack-before-flush is lost across crash
  - bit rot corrupts committed storage detectably
  - torn write commits partial data
- Replay/shrink status: storage scenarios are included in deterministic meta and
  targeted tests.
- Artifact command: `cargo test -p detersim-sim --test storage_faults`.
- Limitations: page store is a deterministic model, not a filesystem.

## Search / Viz

- Experiment name: coverage-guided search smoke and artifact v3
- Hypothesis: search can retain high-signal seeds and schema-v3 artifacts can
  explain search/checker/shrink context without external services.
- SUT: CLI smoke case and debug artifact examples.
- Fault variables: message drop and stream transcript faults.
- Oracle: `FailureSignature::InvariantViolated` plus artifact schema checks.
- Seed budget: smoke tests use small deterministic budgets.
- Positive control: same command emits stable schema-v3 JSON.
- Plant-a-bug result: missing-message failure recalled and rendered.
- Replay/shrink status: CLI and testkit use public replay/shrink APIs.
- Artifact commands:
  - `cargo test -p detersim-search --test coverage_guided_search`
  - `cargo test -p detersim-search --test search_comparison`
  - `cargo test -p detersim-viz --test debug_artifact_v3`
  - `cargo test -p detersim-viz --test causal_artifact_v3`
  - `cargo run -p detersim-testkit --example v3_artifacts`
  - `cargo run -p detersim-cli -- render --examples target/detersim-artifacts/v3`
- Limitations: CLI suite aliases now cover smoke, replicated KV, Mini-Raft
  smoke, and storage faults. The explicit test targets remain the source of
  truth for full benchmark gates.

## V3.1 Search Comparison

| Case | Hypothesis | Budget | Oracle | Result |
| --- | --- | ---: | --- | --- |
| odd-seed-missing-message | Coverage-guided order should find the odd-seed failure before monotonic random order. | 8 | `InvariantViolated(odd-seed-failure)` | Coverage-guided first failing rank `0`; random first failing rank `1`. |
| replicated-kv-read-from-stale-follower | Real protocol suite should be searchable and produce stable `NotLinearizable` signatures. | 500 | `SingleKeyKv` | Every seed recalls; coverage-guided is not worse, but this case is not a prioritization benchmark because every seed fails. |

Source-of-truth commands:

```powershell
cargo test -p detersim-search --test search_comparison
cargo run -p detersim-cli -- search --suite replicated-kv --budget 500 --strategy coverage-guided
cargo run -p detersim-cli -- search --suite replicated-kv --budget 500 --compare
```
