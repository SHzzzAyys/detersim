# Changelog

All notable changes to DeterSim are tracked here. The project is still in a
beta line; public APIs may change before a stable crates.io release.

## Unreleased

### Added

- `detersim-cli doctor --deep` for template generation, template test, and
  artifact render sanity checks.
- `init-sut --template protocol` for a small primary-backup KV adoption
  scaffold with a negative control and plant-a-bug case.
- `sparse-discovery` CLI suite for sparse search evidence smoke tests.
- Suite manifest metadata for case family, bug variant, control kind, expected
  recall rate, and evidence class.
- Search comparison fields for dense/sparse classification, first-failure
  distribution, no-failure strategy count, and strategy winner.
- Adoption docs for CLI workflows, template contracts, and existing SUT
  migration.

### Changed

- CI, nightly, and full-soak workflows now exercise deep doctor, sparse search
  comparison, artifact path rendering, and explanation JSON.
- Benchmark evidence docs now distinguish dense recall cases from sparse
  discovery cases.

### Known limitations

- Sparse discovery cases are deterministic smoke evidence for search reporting,
  not broad statistical proof that one strategy dominates all workloads.
- DeterSim remains beta; no production Raft, real socket adapter, transparent
  Tokio interception, full Elle, or stable crates.io API is claimed.

## v3.0.0-beta.1

### Added

- `detersim-search` for random, coverage-guided, and failure-directed seed
  search over public experiment cases.
- `detersim-net` for deterministic stream/socket-shaped protocol helpers
  without real sockets.
- Schema version 3 debug artifacts with experiment, search, checker, shrink,
  coverage, causal graph, and environment sections.
- CLI commands for `doctor`, `init-sut`, `search`, `explain`, and V3 example
  artifact rendering.
- Elle-lite transaction checker for small serializability histories.
- Full-soak GitHub Actions workflow for manual release validation.

### Changed

- `RunReport` now exposes semantic coverage signals in addition to trace,
  history, nemesis trace, tape log, and tape events.
- Contribution docs, README, and GitHub templates now describe deterministic
  boundaries and release gates more explicitly.

### Fixed

- Mini-Raft stale-read recall now has checker-backed history coverage instead
  of relying only on labels.
- V2 artifact rendering remains available while V3 artifacts add richer
  debugging context.

### Known limitations

- Mini-Raft is a reference benchmark, not production Raft.
- Determinism is scoped to the same binary on the same platform.
- Transparent Tokio interception, real socket adapters, full Elle, and crates.io
  stable publishing remain post-beta work.

## v2.0.0-alpha.1

### Added

- Tape label diagnostics and label-aware shrink reporting.
- Suite-level experiment summaries and artifact metadata.
- Checker-backed Mini-Raft stale-read smoke coverage.
- Initial CLI smoke commands and GitHub issue/PR templates.

### Known limitations

- CLI behavior was smoke-level rather than a complete adoption workflow.
- Search, stream helpers, and schema-v3 artifacts were not yet present.
