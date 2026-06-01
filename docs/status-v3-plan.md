# DeterSim V3 status

V3 starts from `v2.0.0-alpha.1`. V2 delivered replay, label-aware shrink,
suite reports, debug artifacts, and an alpha CLI. V3 turns those pieces into a
beta toolchain for repeated external use.

## V3 additions

- `detersim-search`: seed/tape search with random, coverage-guided, and
  failure-directed strategies.
- `detersim-check`: checker artifacts plus an Elle-lite transaction checker for
  small serializability histories.
- `detersim-net`: deterministic stream helpers for socket-shaped protocols
  without real sockets.
- `detersim-viz`: schema version 3 debug artifacts with search, coverage,
  causal graph, checker, and shrink sections.
- `detersim-cli`: `doctor`, `init-sut`, `search`, `explain`, and v3 example
  rendering.

## Non-claims

- Mini-Raft is still a reference benchmark, not production Raft.
- DeterSim still promises reproducibility only for the same binary on the same
  platform.
- Transparent tokio interception, real sockets, production Raft, and complete
  Elle remain post-V3 work.

## Gates

Run the normal workspace gates, then the V3 fast gates:

```powershell
cargo test -p detersim-search --test coverage_guided_search
cargo test -p detersim-check --test checker_v3_models
cargo test -p detersim-net --test stream_api
cargo test -p detersim-viz --test debug_artifact_v3
cargo test -p detersim-cli --test cli_smoke
cargo run -p detersim-cli -- doctor
```
