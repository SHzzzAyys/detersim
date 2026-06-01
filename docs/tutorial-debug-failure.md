# Tutorial: debug a failure

The expected loop is:

1. Generate a failing run from a seed.
2. Replay the same seed with the recorded tape.
3. Check the structured oracle.
4. Shrink while preserving the same `FailureSignature`.
5. Export JSON/HTML for inspection.

## Built-in smoke path

```powershell
cargo run -p detersim-cli -- run-suite
cargo run -p detersim-cli -- shrink --case missing-message --seed 0 --out target/detersim-artifacts/missing-message-shrink.json
cargo run -p detersim-cli -- render --artifact target/detersim-artifacts/missing-message-shrink.json --out target/detersim-artifacts/missing-message-shrink.html
cargo run -p detersim-cli -- render --examples target/detersim-artifacts/v3
cargo run -p detersim-testkit --example v3_artifacts
```

The CLI is intentionally outside the deterministic core. It may read and write
local artifact files; `core`, `sim`, `nemesis`, `check`, `shrink`, and
`protocols` remain pure deterministic crates.

## Interpreting artifact fields

- `trace`: scheduler-visible events.
- `history`: user-visible client operations and invariant labels.
- `nemesis_trace`: injected fault actions.
- `tape_events`: control-plane draws with stable labels.
- `experiment`: recall and replay/shrink diagnostics.
- `checker`: linearizability or invariant evidence.
- `shrink`: original/minimized tape information.

The `v3_artifacts` example builds Replicated KV, Mini-Raft, storage, stream, and
message-drop artifacts in memory and prints JSON/HTML sizes.

## V3.2 path workflow

Use this when you want files that can be handed to another person or attached to
a bug report:

```powershell
cargo run -p detersim-cli -- run-suite --suite replicated-kv --out target/detersim-artifacts/suite.json
cargo run -p detersim-cli -- search --suite replicated-kv --compare --budget 500 --out target/detersim-artifacts/search.json
cargo run -p detersim-cli -- shrink --case replicated-kv-read-from-stale-follower --seed 0 --out target/detersim-artifacts/shrink.json
cargo run -p detersim-cli -- render --artifact target/detersim-artifacts/shrink.json --out target/detersim-artifacts/shrink.html
```

The shrink command uses the case's normalized `FailureSignature` as the
predicate. If the original seed does not fail for that case, the command returns
a schema-v3 JSON diagnostic instead of pretending a shrink succeeded.

## Generated SUT template

```powershell
cargo run -p detersim-cli -- init-sut --name demo --template message target/demo-sut
cd target/demo-sut
cargo test
```

The generated message template includes a negative control, a plant-a-bug
`ExperimentCase`, and a small artifact rendering test. The stream template
shows deterministic stream transcripts and does not use real sockets.
