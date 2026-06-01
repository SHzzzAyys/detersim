# Artifact causal graph

Schema-v3 artifacts can include a deterministic causal graph:

```text
CausalGraph { nodes, edges }
CausalNode { id, kind, label }
CausalEdge { from, to, kind }
```

IDs are generated from stable indexes such as `trace-0`, `history-0`, and
`nemesis-0`. They must never use memory addresses, wall-clock time, task
allocation order outside the run report, or trace line text as identity.

## Sources

- `RunReport.trace`
- `RunReport.nemesis_trace`
- `RunReport.history`
- checker artifact JSON
- shrink report JSON

The current graph builder derives trace, nemesis, and history nodes directly
from `RunReport`. Checker and shrink panels remain separate JSON sections but
can be linked by future graph edges without changing artifact schema version.

## Command

```powershell
cargo test -p detersim-viz --test causal_artifact_v3
cargo run -p detersim-cli -- render --examples target/detersim-artifacts/v3
```
