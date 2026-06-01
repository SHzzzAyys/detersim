# Protocol benchmarks

## Replicated KV

The Replicated KV benchmark is the stable mid-level protocol suite. It has a
correct negative control plus bug variants for ack-before-replication, stale
follower reads, lost update, duplicate request replay, uncommitted follower
reads, and quorum off-by-one.

Oracle: `SingleKeyKv` or `AppendOnlyLog` through structured `OpRecord`s.

Command:

```powershell
cargo test -p detersim-testkit --test replicated_kv
```

## Mini-Raft

Mini-Raft is a reference protocol, not production Raft. V2 uses checker-backed
history for client-visible key/value safety failures and keeps invariant labels
for internal protocol violations such as dual leadership and persistence
omissions.

Command:

```powershell
cargo test -p detersim-sim --test mini_raft_recall
```

Acceptance: correct Mini-Raft must not fail under the configured seed budget;
each plant-a-bug variant must be recalled and replay/shrink must preserve the
same signature where the case uses `ExperimentCase`.
