# AGENTS.md — Working Guide for Coding Agents

This file governs how automated coding agents (e.g. Codex) work in this repository.
Read it before every task. If a change would violate any rule here, stop and reconsider.

`detersim` is a **Deterministic Simulation Testing (DST) framework**. The whole point of
the project is that *the same seed produces a byte-for-byte identical execution*. Every
decision below exists to protect that property. See `PRD.md` for the full design and
`ROADMAP.md` for the phased task list.

---

## 0. The Prime Directive: determinism is sacred

A change is **wrong** — no matter how clean — if it can make two runs with the same seed
diverge. Before opening a PR, the determinism meta-test (below) MUST be green.

Non-determinism leaks in through these doors. Treat this as a checklist:

| Source | Rule |
|---|---|
| Wall/monotonic clock | NEVER call `std::time::Instant::now`, `SystemTime::now`, or `Instant::elapsed` in `core`, `sim`, `nemesis`, `check`, or any example SUT. Use the `Clock` trait. |
| Threads & scheduling | NEVER use `std::thread`, `tokio::spawn`, `tokio::task::spawn_blocking`, or rayon in those crates. All concurrency goes through `Spawn` + the single-threaded scheduler. |
| Randomness | NEVER use `rand::thread_rng`, `rand::random`, or any OS-entropy RNG. Use the `Rng` trait / the seeded tape. |
| Network/disk | NEVER touch `std::net`, `std::fs`, or `tokio::net`/`tokio::fs` in sim paths. Use `Network` / `Storage`. |
| Hash iteration | NEVER iterate a `HashMap`/`HashSet` whose order can affect behavior. Use `BTreeMap`/`BTreeSet`/`IndexMap`. If you must use a `HashMap`, never iterate it for control flow. |
| Pointer/address | NEVER hash, order, or branch on a pointer address or `Box`/`Rc` identity. |
| Global mutable state | NEVER introduce `static mut`, `lazy_static!`/`OnceCell` mutable singletons, or thread-locals that affect behavior. State is passed explicitly. |
| Time-of-day formatting | NEVER let logging/UUIDs pull from the system clock or system RNG. |

CI enforces a subset of these with a grep/clippy gate (`just lint-determinism`). If you add
a new crate or example, add it to that gate.

---

## 2. Architecture map

```
crates/
  detersim-core/      Env, Clock, Rng, Network, Storage, Spawn traits + SimTime. No I/O, no deps that spawn threads.
  detersim-sim/       SimEnv: event-queue scheduler, cooperative executor, EntropyTape, sim network/storage, World.
  detersim-nemesis/   Fault actions, connectivity matrices, and plan interfaces. Pure data; simulator supplies entropy.
  detersim-check/     Structured histories, invariant hooks, small-history linearizability checker, and reference models.
  detersim-protocols/ Reference SUTs written only against Env: primary-backup KV and Mini-Raft.
  detersim-shrink/    Budgeted chunk + single-draw entropy-tape minimization. Candidate tapes are accepted only if they still reproduce.
  detersim-viz/       Local JSON + self-contained HTML trace export.
  detersim-testkit/   User-facing assertions, experiment reports, failure signatures, plant-a-bug recall, and shrink/debug flows.
  detersim-search/    Coverage/signal guided seed search over public experiment cases.
  detersim-net/       Pure deterministic stream helpers for socket-shaped SUTs.
  detersim-cli/       Optional local CLI for running suites and writing artifacts. This crate may use local file I/O.
  detersim-real/      Planned: tokio + std production impls of the same traits.
examples/             pingpong, toy_raft, WAL and plant-a-bug scenarios used by tests as SUTs.
tests/                cross-crate integration, the determinism meta-test, the plant-a-bug suite.
```

Dependency direction: `core` depends on nothing internal. `nemesis` depends on `core`.
`protocols` depends only on `core` + `check`. `sim` depends on `core` + `nemesis`.
`viz` depends on `sim`. `testkit` may depend on
`sim`, `check`, `nemesis`, `protocols`, `shrink`, and `viz` because it is the user harness layer.
`search` sits above `sim` and `testkit`; it must not feed dependencies back
into deterministic runtime crates. `net` depends only on `core`.
`cli` sits above the harness and may write local artifacts; it must not feed
dependencies back into deterministic crates.
`check` and `shrink` stay independent of `sim`. Planned `real` must not feed back into `core` or `sim`.
**Do not create cycles.** Do not let `core` depend on `tokio`.

---

## 3. Build, test, and reproduce

```bash
# Build everything
cargo build --workspace

# Fast unit + integration tests
cargo test --workspace

# The master oracle: same seed => byte-identical event log (run this before every PR)
cargo test -p detersim-sim determinism_meta -- --nocapture

# Seed soak: run the meta-test (and plant-a-bug suite) over many seeds
DST_SEED_COUNT=10000 cargo test --release seed_soak -- --nocapture

# Experiment-driven recall and fault matrices
cargo test -p detersim-testkit --test experiment_matrix
cargo test -p detersim-testkit --test partitioned_register
cargo test -p detersim-testkit --test replicated_kv
cargo test -p detersim-sim --test nemesis_faults
cargo test -p detersim-sim --test storage_faults
cargo test -p detersim-sim --test mini_raft_recall

# Reproduce a specific failure printed by a test
DST_SEED=<seed> cargo test <failing_test> -- --nocapture

# Determinism lint gate (grep/clippy for forbidden APIs)
just lint-determinism      # or the equivalent script under scripts/
```

Every test that uses randomness MUST print its seed on failure, e.g.
`panic message ends with "(reproduce with DST_SEED={seed})"`.

---

## 4. Definition of Done (every task)

A task/PR is done only when ALL of the following hold:

1. Code compiles with `cargo build --workspace` and `cargo clippy --workspace -- -D warnings`.
2. New behavior has unit tests; the relevant `examples/` SUT (if any) runs.
3. **The determinism meta-test is green.**
4. A seed soak over at least `DST_SEED_COUNT=10000` passes (in `--release`).
5. For Phases ≥ 2, the relevant plant-a-bug case (if the task touches that subsystem) is
   reliably reproduced within the seed budget.
6. Public items have rustdoc; any new forbidden-API surface is added to the lint gate.
7. No new internal dependency cycle; `core` gained no thread-spawning/system-entropy deps.

If you cannot satisfy (3)–(5), do not paper over it — open an issue describing the leak and
stop. A failing determinism meta-test is a real bug, never a flaky test to retry.

---

## 5. Coding conventions

- **No `unwrap()`/`expect()` in library code** except on documented, locally-proven
  invariants (add a `// INVARIANT:` comment). Tests may unwrap freely.
- **Errors**: use `thiserror`-derived enums per crate; no `Box<dyn Error>` in public APIs.
- **Futures are `!Send` by design** in the sim. Do not add `Send`/`Sync` bounds to make
  things "work with tokio multi-thread" — that is the wrong layer; `RealEnv` handles prod.
- **Collections in control flow**: `BTreeMap`/`BTreeSet`/`IndexMap` only. `Vec` is fine.
- **All control-plane randomness goes through `EntropyTape::draw(label)`** with a
  descriptive `label`, so traces and shrinking can locate each decision.
- **The event queue is the single source of concurrency.** Do not add side channels that
  let tasks make progress outside the scheduler.
- Keep modules small and matched to `PRD.md` §5.1. Prefer clarity over cleverness — this is
  also a teaching codebase.

---

## 6. Extension points (how to add things correctly)

- **New fault type**: implement `NemesisPlan::next_action`, drawing all randomness from the
  tape; add a plant-a-bug SUT that the new fault should surface; wire it into the soak.
- **New consistency model**: implement a checker over `Vec<OpRecord>` + a user `Model`;
  add both a passing and a deliberately-violating example.
- **New experiment**: expose a normalized `FailureSignature`, run generate/replay/shrink
  through `ExperimentCase`, and include a negative control so recall is not just a string match.
- **New `Env` capability**: add the trait to `core`, implement in BOTH `sim` and `real`,
  extend the determinism meta-test to exercise it, and add forbidden-API lints for the
  underlying std/tokio surface it replaces.

---

## 7. What NOT to do

- Do not reach for real time, real sockets, real files, real threads, or system RNG in sim
  paths — ever, even "just for now".
- Do not make the scheduler's ordering depend on pointer addresses, allocation order, or
  `HashMap` iteration.
- Do not weaken or `#[ignore]` the determinism meta-test to get a PR green.
- Do not implement low-level cryptography by hand; reuse audited crates if Phase 6 needs TLS.
- Do not expand scope into transparent runtime interception, socket-style APIs, or
  transactional checkers before Phase 6 — they are stretch goals (see `ROADMAP.md`).
- Do not add dependencies that spawn background threads or read OS entropy into `core`/`sim`.

---

## 8. Reproduction protocol (for humans and agents triaging failures)

1. Read the seed from the failing test output (`DST_SEED=<n>`).
2. Re-run with that seed to confirm the exact same failure (it must reproduce — if it does
   not, you have found a determinism leak, which is the highest-priority bug).
3. From Phase 5 on, run the shrinker to get a minimal failing tape, then attach the
   minimized trace (JSON) and, if available, the timeline visualization to the issue.
