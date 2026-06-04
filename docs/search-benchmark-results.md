# Search benchmark results

V3.1 adds deterministic strategy comparison through
`SearchComparisonReport`. V3.2 adds suite-level aggregation through
`SuiteSearchComparisonReport`.

The report compares:

- first failing seed
- first failing rank
- failures observed
- unique semantic coverage
- retained candidate count
- strategy winner by case
- suite-level winner counts

Coverage signals are normalized to semantic classes. History timestamps are not
treated as coverage because they create noisy, seed-specific evidence.

## Current smoke comparison

| Case | Budget | Random first failure rank | Coverage-guided first failure rank | Result |
| --- | ---: | ---: | ---: | --- |
| odd-seed-missing-message | 8 | 1 | 0 | coverage-guided faster |
| replicated-kv-read-from-stale-follower | 500 | 0 or 1 depending strategy order | 0 | not worse |

The replicated KV case fails for every seed by construction, so it is useful as
a stability and reporting gate, not as a strong prioritization benchmark.

## Commands

```powershell
cargo test -p detersim-search --test search_comparison
cargo test -p detersim-search --test suite_search_comparison
cargo run -p detersim-cli -- search --suite replicated-kv --budget 500 --strategy coverage-guided
cargo run -p detersim-cli -- search --suite replicated-kv --budget 500 --compare
cargo run -p detersim-cli -- search --suite mini-raft-smoke --budget 1000 --compare --out target/detersim-artifacts/mini-raft-search.json
```
