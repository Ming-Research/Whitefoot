# Cost of carrying loop relations through joins

The owner approved carrying the relations a loop owes through the joins in
its body, and its header invariants through the join of its break edges, on
the condition that the cost is measured (Q30 in the
[design record](../DESIGN.md)). This run measures that cost.

## Criterion

Recorded before the measurement, and taken from the
[branch-join investigation](../../branch-join-relations/DESIGN.md#current-replay-protocol):
the carry is acceptable when, for each workload group, the summed wall time
and the largest per-process resident set with the carry exceed those without
it by at most 10 percent. Only inputs that receive the same verdict in both
configurations are timed; inputs whose verdict changes are counted
separately, since a rejection ends checking early and its time measures
nothing about the carry.

## Method

- Compiler: this branch with two switches added for the run and removed
  after it. `WF_NO_JOIN_CARRY` returns the plain join from `join_carrying`
  (`compiler/src/semantic/entailment/flow/walk.rs`), which turns off both
  carries; `WF_TERM1_CENSUS` lets a loop with no form, or a rank that does
  not fall, pass, as in the [checker run](checker.md#method), so a
  Snowghost module that the loop rule would reject is still checked to the
  end. One release build serves both configurations.
- Workloads:
  - Snowghost `renderer/` at `09d33ba`, copied; the 17 library modules,
    which write no waiting kind and which this copy checks to the end under
    `WF_TERM1_CENSUS`, each with `--graph modules.wfg --check-module
    <module>`. The oracle, tool and prototype modules stop at the waiting
    keyword this branch renames and are not timed;
  - every single-file program in `tests/programs/` with `--check`;
  - every conformance case source with `--check`.
- One warmup and five measured rounds, alternating which configuration runs
  first. Each row sums wall time over the group's inputs and takes the
  largest resident set of any one process, from `wait4`.

## Results

Pending.
