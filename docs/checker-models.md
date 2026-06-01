# Checker models

V3 checker surface:

- Linearizability: register, single-key KV, multi-key KV, append-only log.
- Serializability: Elle-lite register transactions for small histories.
- Artifacts: stable JSON containing outcome, witness order, conflict ops,
  minimal subhistory, explored states, and budget exhaustion.

`Inconclusive` is not a failure. Test harnesses must keep it separate from
`NotLinearizable` and `NotSerializable`.

The transaction checker is intentionally small. It is meant to catch write
skew, lost update, stale read, and read-your-writes violations in compact
histories, not to replace full Elle.
