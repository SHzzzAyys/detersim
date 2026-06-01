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
cargo run -p detersim-cli -- shrink
cargo run -p detersim-cli -- render 0 target/detersim-artifacts/missing-message.html
cargo run -p detersim-testkit --example v2_artifacts
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

The `v2_artifacts` example builds one Replicated KV debug artifact and one
Mini-Raft stale-read debug artifact in memory and prints their JSON/HTML sizes.
