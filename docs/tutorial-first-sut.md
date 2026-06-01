# Tutorial: first deterministic SUT

This tutorial shows the intended user path: write a tiny system against `Env`,
run it in `World`, and assert same-seed determinism.

## 1. Write against capabilities

A SUT receives `E: Env` and uses only capabilities:

```rust
use detersim_core::{Env, Network};

async fn echo<E: Env>(env: E) {
    let net = env.net();
    let (from, msg) = net.recv().await;
    net.send(from, msg).await;
}
```

Do not call real time, threads, sockets, files, or system RNG in this path.

## 2. Run in a world

```rust
use detersim_sim::{SimEnv, World};

let mut world = World::new(0);
world.add_node(0, |env: SimEnv| echo(env));
let report = world.run();
assert!(!report.deadlocked);
```

## 3. Promote to an experiment

Once a behavior matters, wrap it as an `ExperimentCase` with:

- a seed budget,
- generate and replay functions,
- an oracle returning `Option<FailureSignature>`.

Use `run_experiment_suite` when the project has multiple positive and negative
controls.
