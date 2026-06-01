## Summary

Describe the change in one or two paragraphs.

## Change Type

- [ ] Runtime / scheduler / replay
- [ ] Nemesis / fault model
- [ ] Storage model
- [ ] Checker / oracle
- [ ] Shrink / search
- [ ] Protocol benchmark
- [ ] Viz / artifact schema
- [ ] CLI / docs / release engineering

## Determinism Impact

- [ ] No deterministic crate or SUT path uses real time, real threads, real sockets, real files, or system RNG.
- [ ] New control-plane randomness goes through `EntropyTape::draw(label)`.
- [ ] New ordered data structures use deterministic iteration.
- [ ] Same-seed behavior remains byte-identical.

Notes:

## Replay / Shrink / Artifact Impact

- [ ] Replay still consumes the generated tape as expected.
- [ ] Shrink preserves the same normalized `FailureSignature`.
- [ ] Artifact schema changes use `schema_version`.
- [ ] V2 artifact rendering remains supported when relevant.
- [ ] Not applicable.

Notes:

## Experiments and Oracles

- [ ] New failures use `FailureSignature` or structured checker results.
- [ ] Correct negative controls do not fail.
- [ ] Plant-a-bug variants are recalled within budget.
- [ ] `Inconclusive` is not treated as failure.
- [ ] Not applicable.

## Checks Run

- [ ] `cargo fmt --all --check`
- [ ] `cargo build --workspace`
- [ ] `cargo test --workspace`
- [ ] `cargo clippy --workspace -- -D warnings`
- [ ] `bash scripts/lint_determinism.sh`
- [ ] `$env:DST_SEED_COUNT='10000'; cargo test --release --test determinism_meta`

V3 fast gates if relevant:

- [ ] `cargo test -p detersim-search --test coverage_guided_search`
- [ ] `cargo test -p detersim-check --test checker_v3_models`
- [ ] `cargo test -p detersim-net --test stream_api`
- [ ] `cargo test -p detersim-viz --test debug_artifact_v3`
- [ ] `cargo test -p detersim-cli --test cli_smoke`
- [ ] `cargo run -p detersim-cli -- doctor`

## Artifacts

Attach or link minimized JSON/HTML artifacts when this PR changes a failing
experiment, checker witness, shrinker behavior, or artifact schema.

## Known Limits

List non-claims, skipped gates, open risks, or intentionally deferred work.
