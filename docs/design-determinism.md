# Determinism design

DeterSim treats determinism as a structural property. A system under test does
not reach out to the machine directly; it receives an `Env` with explicit
capabilities for time, randomness, network, storage, and spawning.

## Why not real time?

Real clocks make two same-seed runs diverge. DeterSim uses logical `SimTime` and
node-local skew controlled by the simulator.

## Why not real threads?

Thread scheduling is outside the seed. DeterSim uses a single event queue keyed
by `(SimTime, seq)` so scheduling has a deterministic total order.

## Why not system RNG?

Control-plane randomness must be recorded and replayed. DeterSim routes it
through the entropy tape and records tape labels for shrinking and artifacts.

## Where can real I/O happen?

Only outside deterministic crates, for example in `detersim-cli` when writing
local JSON/HTML artifacts.
