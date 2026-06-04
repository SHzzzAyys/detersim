# CLI Workflows

DeterSim's CLI is the adoption layer above the deterministic crates. It may
read and write local files, but it must only compose public APIs from
`detersim-sim`, `detersim-testkit`, `detersim-search`, `detersim-shrink`, and
`detersim-viz`.

The CLI is beta. Treat JSON field names as intentionally stable for the V3 line,
but do not treat command names or flags as a final non-beta contract.

## Doctor

Fast sanity:

```powershell
cargo run -p detersim-cli -- doctor
```

Deep adoption sanity:

```powershell
cargo run -p detersim-cli -- doctor --deep
```

`doctor --deep` generates message, stream, and protocol templates in a temporary
directory, runs their tests, and renders a sample artifact. It intentionally does
not run the release soak.

Expected schema-v3 fields include:

- `ok`
- `deep`
- `workspace`
- `rustc`
- `sample_suite_ok`
- `artifact_render_ok`
- `template_smoke_ok`
- `deep_template_ok`
- `determinism_lint_hint`

## New SUT Template

Message template:

```powershell
cargo run -p detersim-cli -- init-sut --name demo-message --template message target/demo-message
cargo test --manifest-path target/demo-message/Cargo.toml
```

Stream template:

```powershell
cargo run -p detersim-cli -- init-sut --name demo-stream --template stream target/demo-stream
cargo test --manifest-path target/demo-stream/Cargo.toml
```

Protocol template:

```powershell
cargo run -p detersim-cli -- init-sut --name demo-protocol --template protocol target/demo-protocol
cargo test --manifest-path target/demo-protocol/Cargo.toml
```

The protocol template demonstrates a small primary-backup KV SUT using
`Env`, structured history, a negative control, and a plant-a-bug case. It is an
adoption scaffold, not a production protocol.

## Suite Execution

Run a built-in suite and persist the suite manifest plus summary:

```powershell
cargo run -p detersim-cli -- run-suite --suite replicated-kv --out target/detersim-artifacts/replicated-kv-suite.json
```

Supported suite names:

- `smoke`
- `replicated-kv`
- `mini-raft-smoke`
- `storage-faults`
- `sparse-discovery`

Unsupported suite names return explicit `unsupported_suite` JSON. The CLI must
not pretend an unimplemented benchmark ran successfully.

## Search

Single strategy:

```powershell
cargo run -p detersim-cli -- search --suite replicated-kv --budget 500 --strategy coverage-guided
```

Strategy comparison:

```powershell
cargo run -p detersim-cli -- search --suite sparse-discovery --compare --budget 32 --out target/detersim-artifacts/sparse-search.json
```

Dense recall cases are reporting/replay/shrink evidence. Sparse cases are the
only cases that support claims about search prioritization.

## Shrink, Render, Explain

```powershell
cargo run -p detersim-cli -- shrink --case missing-message --seed 0 --out target/detersim-artifacts/missing-message-shrink.json
cargo run -p detersim-cli -- render --artifact target/detersim-artifacts/missing-message-shrink.json --out target/detersim-artifacts/missing-message-shrink.html
cargo run -p detersim-cli -- explain --artifact target/detersim-artifacts/missing-message-shrink.json --out target/detersim-artifacts/missing-message-explain.json
```

The shrink command uses normalized `FailureSignature` preservation. It must not
decide success by substring matching trace lines.

## Artifact Examples

```powershell
cargo run -p detersim-cli -- render --examples target/detersim-artifacts/v3
```

This writes schema-v3 example JSON/HTML files plus an index JSON under
`target/`. Generated artifacts are build outputs and should not be committed.
