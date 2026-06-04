# Tutorial: Adopt an Existing SUT

This tutorial describes the engineering path for bringing an existing protocol
into DeterSim without weakening determinism.

## 1. Draw the Boundary

Start by identifying every place the SUT touches the outside world:

- time
- random numbers
- network send/receive
- persistent storage
- task spawning
- retry or timeout scheduling

Each boundary must move behind `Env` or a deterministic helper crate. Do not
wrap real clocks, real sockets, real files, or real threads and call the result
deterministic.

## 2. Define the Minimal Workload

Pick the smallest workload that can express one safety property:

- register write/read
- single-key KV put/get
- append-only log append/read
- small transaction history

Avoid importing a full production workload before the oracle is trustworthy.

## 3. Add Structured History

Record client-visible operations as stable history records:

- operation id
- client id
- invocation/completion order
- operation kind
- value
- outcome

Do not put task ids, pointer addresses, wall-clock timestamps, or trace line
numbers into the failure signature.

## 4. Pick the Oracle

Use the narrowest oracle that matches the property:

- `SingleKeyKv` for a single register/KV key.
- `MultiKeyKv` for independent key histories.
- `AppendOnlyLog` for duplicate append and ordering bugs.
- Elle-lite transaction checks only for small transaction histories.
- Normalized invariants for internal protocol safety such as dual leaders.

`Inconclusive` is not a failure. Increase the checker budget or reduce the
history before claiming a bug.

## 5. Create Controls

Every useful suite needs both sides:

- negative control: correct implementation should not recall the bug signature
- plant-a-bug case: injected bug should recall within the seed budget
- sparse case when evaluating search acceleration

Dense every-seed-fails cases are still valuable, but they only prove reporting,
replay, shrink, and artifact stability.

## 6. Run the Loop

```powershell
cargo run -p detersim-cli -- run-suite --suite replicated-kv --out target/detersim-artifacts/replicated-kv-suite.json
cargo run -p detersim-cli -- search --suite sparse-discovery --compare --budget 32 --out target/detersim-artifacts/sparse-search.json
cargo run -p detersim-cli -- shrink --case missing-message --seed 0 --out target/detersim-artifacts/missing-message-shrink.json
cargo run -p detersim-cli -- render --artifact target/detersim-artifacts/missing-message-shrink.json --out target/detersim-artifacts/missing-message-shrink.html
cargo run -p detersim-cli -- explain --artifact target/detersim-artifacts/missing-message-shrink.json --out target/detersim-artifacts/missing-message-explain.json
```

For a starting scaffold, generate the protocol template:

```powershell
cargo run -p detersim-cli -- init-sut --name demo-protocol --template protocol target/demo-protocol
cargo test --manifest-path target/demo-protocol/Cargo.toml
```

## Stop Rules

Stop and fix the harness if:

- the same seed and tape do not replay to the same trace/history/signature
- the negative control fails
- shrink preserves a different signature
- the oracle reports `Inconclusive` and the report treats it as a failure
- a deterministic crate needs real time, threads, network, files, or system RNG
