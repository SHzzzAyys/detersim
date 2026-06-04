# V3 Benchmark Evidence

This document records the evidence DeterSim uses for the V3 beta line. The
claim is limited: these experiments recall known bug variants under deterministic
budgets and produce replay/shrink/artifact data. They do not prove a protocol is
correct.

The test targets are the source of truth. V3.3 CLI suite mapping now covers the
same beta benchmark families plus sparse-discovery evidence, but the explicit
Rust targets remain the release gates for full evidence.

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
  smoke, storage faults, and sparse discovery. The explicit test targets remain
  the source of truth for full benchmark gates.

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

## V3.2 Suite-Level Evidence

V3.2 distinguishes reporting-stability cases from search-prioritization cases:

- Every-seed-fails cases prove that suite reporting, replay/shrink metadata, and
  artifact export are stable. They do not prove search is faster, because every
  strategy sees a failure immediately.
- Sparse-failure cases are the right basis for comparing `Random`,
  `CoverageGuided`, and `FailureDirected`.

Current CLI suite coverage:

| Suite | Cases | Oracle families | Output |
| --- | ---: | --- | --- |
| `replicated-kv` | 7 | `SingleKeyKv`, append-log, negative control | suite manifest + summary JSON |
| `mini-raft-smoke` | 9 | checker-backed history, normalized invariants | suite manifest + summary JSON |
| `storage-faults` | 3 | durability/checksum/torn-write signatures | suite manifest + summary JSON |

Current strategy comparison output:

| Suite | Strategies | Evidence recorded |
| --- | --- | --- |
| `replicated-kv` | `Random`, `CoverageGuided`, `FailureDirected` | first failing seed/rank, failures, unique coverage, retained candidates, strategy wins |
| `mini-raft-smoke` | `Random`, `CoverageGuided`, `FailureDirected` | same fields; useful for smoke regression, not a full 50k soak replacement |

V3.2 source-of-truth commands:

```powershell
cargo test -p detersim-testkit --test experiment_suite_manifest
cargo test -p detersim-search --test suite_search_comparison
cargo test -p detersim-cli --test cli_artifact_workflow
cargo run -p detersim-cli -- run-suite --suite replicated-kv --out target/detersim-artifacts/replicated-kv-suite.json
cargo run -p detersim-cli -- search --suite mini-raft-smoke --compare --budget 1000 --out target/detersim-artifacts/mini-raft-search.json
```

Known limitation: most built-in bug variants are intentionally dense recall
cases. They are strong tests for deterministic reporting and shrink preservation,
but only sparse cases should be used to claim search efficiency improvements.

## V3.3 Sparse Discovery Evidence

V3.3 adds `sparse-discovery`, a CLI-facing suite with three deterministic sparse
cases. These cases are synthetic harness cases rather than protocol expansions;
their purpose is to test search evidence reporting and strategy comparison when
not every seed fails.

| Case | Failure condition | Why it is sparse | Expected evidence |
| --- | --- | --- | --- |
| `sparse-delayed-replication-stale-read` | selected seeds model delayed replication before a stale follower read | only part of the seed range reaches the stale-read window | coverage/failure directed strategies should not be worse than monotonic random order |
| `sparse-crash-after-ack-before-flush` | selected seeds model crash timing after ack but before durable flush | failure requires a narrow crash point | comparison report records first failing rank and sparse classification |
| `sparse-partition-heal-race` | selected seeds model partition heal interacting with message ordering | failure requires a specific ordering race | report distinguishes sparse search evidence from dense recall evidence |

The currently observed smoke command:

```powershell
cargo run -p detersim-cli -- search --suite sparse-discovery --compare --budget 32
```

emits `dense_case:false` and `sparse_case:true` for each sparse case. In the
local V3.3 run, `CoverageGuided` or `FailureDirected` won every sparse case at
budget 32. This is a smoke result, not a statistical claim about all workloads.

## V3.3 Adoption Evidence

The adoption pack validates that an external user can start from generated
templates and exercise the deterministic loop without private APIs:

```powershell
cargo run -p detersim-cli -- doctor --deep
cargo run -p detersim-cli -- init-sut --name demo-message --template message target/demo-message
cargo run -p detersim-cli -- init-sut --name demo-stream --template stream target/demo-stream
cargo run -p detersim-cli -- init-sut --name demo-protocol --template protocol target/demo-protocol
```

The `protocol` template is the most important adoption test. It includes a
primary-backup KV SUT, structured history, a `SingleKeyKv` oracle, a correct
negative control, and a stale-follower-read plant-a-bug case.
