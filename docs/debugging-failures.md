# Debugging Failures

DeterSim's debugging loop is:

1. Find a failing seed.
2. Record the generated `RunReport.tape_log`.
3. Replay the same seed and tape to confirm the failure.
4. Shrink the tape while the failure still reproduces.
5. Attach JSON or HTML trace output to the issue.

The `detersim-testkit` crate packages that loop.

```rust
use detersim_shrink::ShrinkConfig;
use detersim_testkit::shrink_replay_failure;

let artifact = shrink_replay_failure(
    seed,
    run_generate,
    run_replay,
    |report| report.history.contains(&"missing-message".to_string()),
    ShrinkConfig { max_attempts: 200, min_chunk_len: 2 },
);

assert!(artifact.shrink.reproduced);
println!("{}", artifact.minimized_json);
```

`run_generate(seed)` and `run_replay(seed, tape)` must build the same scenario.
The only difference is that replay constructs the world with `World::replay`.

## Example

```bash
cargo run -p detersim-testkit --example debug_failure
```

The example searches for a seed where a message is dropped, shrinks the tape
while `missing-message` still appears in history, and prints the minimized trace
artifact sizes.

## Real Fault Example

The integration test below is the first end-to-end WP-A/WP-C/WP-D slice:

```bash
cargo test -p detersim-testkit --test partitioned_register
cargo test -p detersim-testkit --test replicated_kv
cargo test -p detersim-sim --test mini_raft_recall
```

It builds a deliberately buggy replicated register. A nemesis plan consumes a
tape draw to choose a directed link fault. When the `0 -> 1` replication link is
cut, node 0 acknowledges `write(7)` before replication, and a later read from
node 1 can return `0`. The generated history is fed into
`detersim-check::models::Register`; the checker reports `NotLinearizable`, then
`shrink_replay_failure` verifies replay soundness and emits minimized JSON/HTML
artifacts.

The Replicated KV suite is the next rung: it keeps a correct primary-backup
negative control and recalls ack-before-replication, stale-follower read,
lost-update, and duplicate-request bugs through `ExperimentCase`. The Mini-Raft
suite is intentionally still a small reference object; it verifies persisted
term/vote/log behavior and recalls the planned plant-a-bug variants before the
project grows a fuller Raft/VSR implementation.

## Interpreting Replay Diagnostics

- `tape_replaying`: the run was driven by a supplied tape.
- `tape_input_len`: the original replay tape length.
- `tape_cursor`: how many tape slots the run attempted to consume.
- `tape_consumed_all`: the run consumed at least the full supplied tape.
- `tape_exhausted`: the run attempted to read past the supplied tape.

For full-fidelity replay, `tape_exhausted` should be false. During shrinking it
may become true for candidates; that is acceptable only if the failure predicate
still holds and the resulting artifact is treated as a minimized reproducer, not
as a byte-identical replay of the original run.

## Experiment Reports

For plant-a-bug recall, prefer `ExperimentCase` over one-off assertions. It
records the seed budget, first failing seed, recall rate, replay identity,
original/minimized tape lengths, shrink ratio, artifact sizes, and replay tape
diagnostics.

```rust
use detersim_testkit::{assert_recall, ExperimentBudget, ExperimentCase};

let report = assert_recall(&ExperimentCase {
    name: "partitioned-register-ack-before-replication",
    budget: ExperimentBudget { seed_count: 200, shrink: Default::default() },
    generate: run_generate,
    replay: run_replay,
    oracle: classify_failure,
});

assert!(report.replay_byte_identical);
assert!(report.minimized_matched_signature);
```

Use `experiment_report_to_json` or `experiment_matrix_report_to_json` when an
issue needs machine-readable evidence. The trace HTML separates nemesis,
history, raw trace, and simple node/task lanes, while staying fully static.

The oracle should return a normalized `FailureSignature`. Do not put absolute
times, task ids, or trace line numbers in the signature; those fields make
shrinking preserve incidental behavior instead of the underlying bug.
