# Getting Started with DeterSim

DeterSim tests systems that are written against explicit capabilities. A node
does not call wall clocks, sockets, files, threads, or system randomness
directly. It receives an `Env`, then uses `env.clock()`, `env.net()`,
`env.storage()`, `env.rng()`, and `env.spawn()`.

## Minimal Node

```rust
use std::time::Duration;
use detersim_core::{Clock, Env, Network};

async fn run_node<E: Env>(env: E) {
    let clock = env.clock();
    let net = env.net();

    if env.node_id() == 0 {
        net.send(1, b"ping".to_vec()).await;
        let (_from, msg) = net.recv().await;
        assert_eq!(msg, b"pong");
    } else {
        let (from, msg) = net.recv().await;
        assert_eq!(msg, b"ping");
        clock.sleep(Duration::from_millis(5)).await;
        net.send(from, b"pong".to_vec()).await;
    }
}
```

## Run It in a World

```rust
use detersim_sim::{SimEnv, World};

let mut world = World::new(7);
world.add_nodes(2, |env: SimEnv| run_node(env));
let report = world.run();

assert!(!report.deadlocked);
assert!(!report.aborted);
```

## Use the Testkit

```rust
use detersim_testkit::assert_deterministic;

assert_deterministic(7, |seed| {
    let mut world = World::new(seed);
    world.add_nodes(2, |env: SimEnv| run_node(env));
    world.run()
});
```

For replay, return a generated `RunReport.tape_log` to a `World::replay` run
with the same seed and scenario setup. The replay report exposes
`tape_replaying`, `tape_consumed_all`, and `tape_exhausted` so failures are
diagnosable.

For shrinking and trace export, see `docs/debugging-failures.md`.

## Fault Injection

Schedule faults with logical times:

```rust
use detersim_core::SimTime;
use detersim_nemesis::NemesisAction;

world.schedule_nemesis(
    SimTime::from_nanos(1_000_000),
    NemesisAction::Crash { node: 0 },
);
world.schedule_nemesis(
    SimTime::from_nanos(2_000_000),
    NemesisAction::Restart { node: 0 },
);
```

The crash removes the node's volatile tasks, waiters, timers, inbox, and queued
node-local events. Restart re-runs the registered node factory and keeps only
durable storage state.

## Local Examples

```bash
cargo run -p detersim-sim --example pingpong
cargo run -p detersim-sim --example wal
cargo run -p detersim-sim --example partition_dual_leader
cargo run -p detersim-sim --example toy_raft
```
