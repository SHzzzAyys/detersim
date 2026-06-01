# Debug artifact schema v3

Schema v3 keeps the v2 run report intact and wraps it with richer debugging
sections:

- `run`: deterministic `RunReport`.
- `experiment`: suite or case report JSON.
- `search`: coverage/failure-directed search report JSON.
- `checker`: checker artifact JSON.
- `shrink`: signature-preserving shrink report JSON.
- `failure_signature`: normalized failure identity.
- `coverage`: semantic coverage signals.
- `causal_graph`: small static event graph for explanation.
- `environment`: non-deterministic build/platform metadata recorded outside the
  deterministic core.

`detersim-viz` still renders v2 artifacts. V3 HTML remains self-contained and
does not load remote scripts, fonts, or services.
