# Search benchmark results

V3.1 adds deterministic strategy comparison through
`SearchComparisonReport`. V3.2 adds suite-level aggregation through
`SuiteSearchComparisonReport`. V3.3 adds dense/sparse classification and
first-failure distribution fields for suite evidence.

The report compares:

- first failing seed
- first failing rank
- failures observed
- unique semantic coverage
- retained candidate count
- strategy winner by case
- suite-level winner counts
- median first failing rank
- p90 first failing rank
- dense/sparse classification
- no-failure strategy count

Coverage signals are normalized to semantic classes. History timestamps are not
treated as coverage because they create noisy, seed-specific evidence.

## Current smoke comparison

| Case | Budget | Random first failure rank | Coverage-guided first failure rank | Result |
| --- | ---: | ---: | ---: | --- |
| odd-seed-missing-message | 8 | 1 | 0 | coverage-guided faster |
| replicated-kv-read-from-stale-follower | 500 | 0 or 1 depending strategy order | 0 | not worse |

The replicated KV case fails for every seed by construction, so it is useful as
a stability and reporting gate, not as a strong prioritization benchmark.

## V3.3 sparse comparison

Sparse cases are the only built-in cases currently used for search acceleration
evidence. Dense cases remain valuable release gates, but they should not be used
to claim that a strategy finds failures faster.

| Suite | Case | Budget | Expected classification | Current smoke result |
| --- | --- | ---: | --- | --- |
| `sparse-discovery` | `sparse-delayed-replication-stale-read` | 32 | `sparse_case:true`, `dense_case:false` | `FailureDirected` wins in the local smoke run |
| `sparse-discovery` | `sparse-crash-after-ack-before-flush` | 32 | `sparse_case:true`, `dense_case:false` | `CoverageGuided` wins in the local smoke run |
| `sparse-discovery` | `sparse-partition-heal-race` | 32 | `sparse_case:true`, `dense_case:false` | `CoverageGuided` wins in the local smoke run |

The local smoke command is:

```powershell
cargo run -p detersim-cli -- search --suite sparse-discovery --compare --budget 32
```

This is evidence that the report plumbing, classification, and deterministic
ranking are working. It is not a broad claim that coverage-guided search is
better for all protocols.

## Commands

```powershell
cargo test -p detersim-search --test search_comparison
cargo test -p detersim-search --test suite_search_comparison
cargo run -p detersim-cli -- search --suite replicated-kv --budget 500 --strategy coverage-guided
cargo run -p detersim-cli -- search --suite replicated-kv --budget 500 --compare
cargo run -p detersim-cli -- search --suite mini-raft-smoke --budget 1000 --compare --out target/detersim-artifacts/mini-raft-search.json
cargo run -p detersim-cli -- search --suite sparse-discovery --budget 32 --compare --out target/detersim-artifacts/sparse-search.json
```
