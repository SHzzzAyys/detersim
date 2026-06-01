# Mini-Raft checker-backed benchmark

Mini-Raft is a reference benchmark, not production Raft. V3.1 splits failures
by oracle:

- Client-visible safety failures use structured history plus
  `detersim-check`.
- Protocol-internal failures use normalized invariant labels.

## Checker-backed variants

- `WrongCommitRule`
- `WrongLogMatching`
- `FollowerStaleRead`
- `DuplicateClientRequest`
- `ApplyBeforeCommit`
- `OldTermLeaderCommitsEntry`

Each variant has a `RaftClientHistory` description. Single-key bugs use the
`SingleKeyKv` model. Duplicate request replay uses the append-only log model.
The expected signature is `NotLinearizable` with stable model and conflict
fields.

## Invariant-backed variants

- `TermNotPersisted`
- `VoteNotPersisted`
- `DualLeaderUnderPartition`

These emit stable labels such as
`raft-invariant:single-leader-per-term`. Signatures must not include logical
timestamps, task IDs, trace line numbers, or platform-specific details.

## Command

```powershell
cargo test -p detersim-sim --test mini_raft_recall
```
