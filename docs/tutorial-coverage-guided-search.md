# Tutorial: coverage-guided search

Start with a normal `ExperimentCase`. Then run search instead of a plain seed
range:

```powershell
cargo run -p detersim-cli -- search --budget 1000 --strategy coverage-guided
```

Read these fields first:

- `first_failing_seed`: seed to replay.
- `first_failing_rank`: search order position where it was found.
- `unique_coverage`: semantic signals discovered during search.
- `candidates`: retained high-signal seeds with tape length and signature.

Search is a discovery accelerator. A failure is not accepted until replay and
signature-preserving shrink keep the same `FailureSignature`.
