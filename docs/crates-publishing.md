# Crates Publishing Dry Run

DeterSim is not ready for stable crates.io publishing. Use this checklist for
dry-run validation before any beta crate release.

## Metadata checklist

Each crate should include:

- `description`
- workspace `license`
- workspace `repository`
- `readme = "../../README.md"`
- rustdoc for public APIs
- no accidental dependency on CLI, testkit, or viz from deterministic core
  crates

## Dry-run order

Start with lower-level crates:

```powershell
cargo package -p detersim-core --allow-dirty
cargo publish --dry-run -p detersim-core
cargo publish --dry-run -p detersim-check
cargo publish --dry-run -p detersim-nemesis
cargo publish --dry-run -p detersim-net
cargo publish --dry-run -p detersim-sim
cargo publish --dry-run -p detersim-protocols
cargo publish --dry-run -p detersim-shrink
cargo publish --dry-run -p detersim-viz
cargo publish --dry-run -p detersim-testkit
cargo publish --dry-run -p detersim-search
cargo publish --dry-run -p detersim-cli
```

Do not publish if any deterministic crate introduces real time, threads, network,
files, system RNG, or nondeterministic iteration.

## Current policy

For V3, the expected release target is a GitHub beta tag. Crates.io remains a
dry-run readiness check unless maintainers explicitly decide to publish a `0.3.x`
crate line.
