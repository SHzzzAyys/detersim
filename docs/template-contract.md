# Template Contract

`detersim-cli init-sut` generates small, local-first Rust projects that exercise
the public DeterSim API. Templates are adoption aids, not hidden privileged test
harnesses.

## Shared Rules

All templates must:

- Use path dependencies pointing at the current DeterSim checkout.
- Compile with `cargo test`.
- Include at least one deterministic negative control.
- Avoid real time, real threads, real network, real files, and system RNG inside
  the SUT logic.
- Use `Env`, `Clock`, `Network`, `Storage`, and deterministic history records
  instead of OS facilities.
- Keep local file I/O limited to the generated test harness and Cargo itself.

## Message Template

The message template contains:

- a two-node world
- a deterministic missing-message plant-a-bug case
- `assert_deterministic`
- `run_experiment_case`
- a static artifact render test

Its purpose is to teach the minimum generate -> oracle -> artifact loop.

## Stream Template

The stream template contains:

- a deterministic stream transcript
- duplicate-frame evidence
- no real sockets

Its purpose is to show the socket-shaped model without implying transparent
interception of `std::net` or Tokio networking.

## Protocol Template

The protocol template contains:

- a primary-backup KV reference SUT
- deterministic `World` setup
- structured history recording
- `SingleKeyKv` linearizability oracle
- a correct negative control
- a stale-follower-read plant-a-bug case

Its purpose is to show how a real protocol-shaped SUT can be wrapped in
`ExperimentCase` without bypassing `Env` or `RunReport`.

## Acceptance

The template contract is enforced by:

```powershell
cargo test -p detersim-cli --test cli_e2e
cargo run -p detersim-cli -- doctor --deep
```

`doctor --deep` is a fast adoption gate. It does not replace the release gates
or nightly soak.
