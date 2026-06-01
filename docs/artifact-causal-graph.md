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

V3.2 keeps the same field names and schema version while adding richer edges:

- message send node -> message event node -> delivery/drop trace node
- nemesis action -> affected trace/history node
- checker artifact -> conflict/minimal-history nodes
- shrink report -> preserved failure node

Checker and shrink JSON remain separate artifact sections. The graph links them
as opaque stable nodes so the viewer can explain the failure without requiring a
new schema version.

## Command

```powershell
cargo test -p detersim-viz --test causal_artifact_v3
cargo test -p detersim-viz --test artifact_schema_compat
cargo run -p detersim-cli -- render --examples target/detersim-artifacts/v3
```
