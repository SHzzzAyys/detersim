# Public API stability

V3 is a beta line.

- Artifact readers must inspect `schema_version`.
- V2 artifact rendering remains supported.
- New deterministic core APIs must not require real time, real threads, real
  sockets, real files, or system RNG.
- CLI commands may read and write local files because the CLI is outside the
  deterministic core.
- Crate publishing stays dry-run until API docs, metadata, license, and readme
  links are checked for every crate.
