---
name: Experiment proposal
about: Propose a deterministic plant-a-bug or negative-control experiment
title: "[experiment] "
labels: experiment
---

## Hypothesis

What bug or property should this experiment test?

## System Under Test

Protocol, crate, example, or external SUT:

## Fault Variables

- network:
- process:
- storage:
- clock:
- workload:

## Oracle

- [ ] invariant label
- [ ] linearizability checker
- [ ] serializability checker
- [ ] storage corruption detector
- [ ] deadlock / liveness signal
- [ ] other:

Expected `FailureSignature`:

```text

```

## Positive Control

What correct implementation or configuration should not fail?

## Negative Control / Plant-a-Bug

What injected bug should be recalled?

## Seed Budget

Fast gate budget:

Nightly/full-soak budget:

## Replay / Shrink Expectations

- [ ] generated run produces tape
- [ ] replay is byte-identical
- [ ] minimized replay preserves the same signature
- [ ] artifact explains the failure

## Expected Artifact

What should appear in trace, history, checker witness, shrink report, or viz?
