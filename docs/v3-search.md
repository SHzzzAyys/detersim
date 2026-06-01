# V3 search

`detersim-search` runs public `ExperimentCase` values repeatedly and ranks
seeds by stable signals from `RunReport`.

Strategies:

- `Random`: baseline seed order.
- `CoverageGuided`: prioritizes seeds that expose new semantic coverage.
- `FailureDirected`: prioritizes failure signatures and new coverage.

Coverage comes from trace edges, nemesis actions, tape labels, history classes,
and run outcomes. Search does not bypass replay or shrink; once a candidate is
found, the normal testkit path still owns signature preservation.

CLI example:

```powershell
cargo run -p detersim-cli -- search --suite replicated-kv --budget 5000 --strategy coverage-guided
```
