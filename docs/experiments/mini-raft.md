# Mini-Raft experiment

## Purpose

Mini-Raft is DeterSim's v1.0 reference protocol for demonstrating recall beyond
single-message examples. It is intentionally small: fixed three-node cluster,
single value state machine, deterministic client, and explicit crash/restart
probe.

## Non-goals

Mini-Raft is not production Raft. It does not implement membership change,
snapshots, joint consensus, production log compaction, real sockets, or a Tokio
interception layer.

## Covered mechanics

- `current_term` persistence probe,
- `voted_for` persistence probe,
- log byte persistence probe,
- leader-style append,
- quorum ack before client success,
- follower append ack,
- client write/read smoke path,
- crash/restart recovery through committed simulated storage.

## Plant-a-bug variants

- `TermNotPersisted`
- `VoteNotPersisted`
- `WrongCommitRule`
- `WrongLogMatching`
- `DualLeaderUnderPartition`
- `FollowerStaleRead`
- `DuplicateClientRequest`
- `ApplyBeforeCommit`
- `OldTermLeaderCommitsEntry`

## Current oracle shape

The current Mini-Raft recall test uses normalized protocol labels collected via
an observer/client node. This keeps the protocol SUT below `detersim-sim` while
still proving deterministic recall. The next hardening step is to migrate the
state-machine-visible variants to structured `OpRecord` histories and the
linearizability checker.

Command:

```powershell
cargo test -p detersim-sim --test mini_raft_recall
```
