# Versioning

DeterSim currently uses beta versioning.

## GitHub releases

GitHub release tags use project-level names:

- `v2.0.0-alpha.1`
- `v3.0.0-beta.1`

These tags describe repository capability, examples, docs, workflows, and
artifacts as a whole.

## Crate versions

Crates are still on the `0.x.y` line. V3 beta readiness corresponds to a future
`0.3.0` crate line, but crates.io publishing remains dry-run only until the API
surface is audited.

## Artifact schema versions

Artifact schema versions are independent from crate and release versions:

- schema `2`: V2 debug artifact shape
- schema `3`: V3 debug artifact shape with search, coverage, checker, shrink,
  causal graph, and environment sections

Artifact readers must inspect `schema_version` rather than infer shape from the
crate version.

## Compatibility policy

- Keep V2 artifact rendering working while V3 is beta.
- Avoid removing public APIs without a migration path.
- Keep `Inconclusive` separate from failure results.
- Do not expand README claims beyond tested capability.
