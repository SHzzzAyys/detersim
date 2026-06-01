# DeterSim API guide

## SUT boundary

Write systems under test against `detersim_core::Env`:

- time through `Clock`,
- network through `Network`,
- storage through `Storage`,
- randomness through `Rng`,
- concurrency through `Env::spawn`.

Do not call real time, threads, sockets, files, or system RNG in deterministic
SUT paths.

## Protocol crate

`detersim-protocols` contains reusable reference SUTs. It may depend only on
`detersim-core` and `detersim-check`.

Primary-backup KV:

- `KvConfig`
- `KvBugVariant`
- `run_primary_backup_kv`
- `run_primary_backup_kv_client`
- `single_key_kv_history`
- `append_log_history`

Mini-Raft:

- `MiniRaftConfig`
- `RaftBugVariant`
- `run_mini_raft`
- `run_mini_raft_client`
- `collect_protocol_events`

## Experiment harness

Use `detersim-testkit::ExperimentCase` for plant-a-bug and negative-control
experiments. A good experiment has:

- fixed seed budget,
- generate function,
- replay function,
- structured oracle returning `Option<FailureSignature>`,
- replay equality check,
- signature-preserving shrink check.

`Inconclusive` checker results must remain diagnostic, not failures.

## Artifacts

Use `detersim_viz::run_report_to_json` and `timeline_html` for local artifacts.
The JSON includes `schema_version` and replay diagnostics. The HTML is
self-contained and makes no external network request.
