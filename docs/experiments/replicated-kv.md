# Replicated KV experiment

## Hypothesis

DeterSim should distinguish a correct primary-backup KV from common replicated
storage bugs under a fixed seed budget. Correct code must not trigger a
linearizability failure; each plant-a-bug variant must recall a normalized
`FailureSignature`.

## System under test

- Crate: `detersim-protocols`
- Entrypoints:
  - `run_primary_backup_kv`
  - `run_primary_backup_kv_client`
- Topology: node `0` is primary, nodes `1` and `2` are followers, node `3` is
  the deterministic client.
- Oracle models:
  - `SingleKeyKv<i32>` for key/value cases,
  - `AppendOnlyLog<String>` for duplicate request replay.

## Variables

- Network fault: asymmetric partition `0 -> 1` for ack-before-replicate.
- Protocol bug variants:
  - `AckBeforeReplicate`
  - `ReadFromStaleFollower`
  - `LostUpdate`
  - `DuplicateRequestReapplied`
  - `FollowerAppliesUncommitted`
  - `QuorumCountOffByOne`

## Oracle

The experiment converts `RunReport.history` into structured `OpRecord`s through
`detersim-protocols::history`. The checker result is normalized through
`linearizability_signature`; `Inconclusive` is not treated as a failure.

## Budget and acceptance

- Correct variant: 500 seeds, zero `NotLinearizable` signatures.
- Bug variants: 500 seeds, 100% recall.
- First failing seed must replay byte-identically.
- Shrunk tape must preserve the same `FailureSignature`.

Command:

```powershell
cargo test -p detersim-testkit --test replicated_kv
```
