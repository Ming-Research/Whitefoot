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

Run on 2026-10-01 at `96a6aacf` with the two switches, one release build
(`q30-time` stage, 1,456 s). Wall time is the median of the five rounds'
sums; resident set is the largest of any process in any round.

| Group | Inputs | Timed | Verdict changes | Wall, carry on | Wall, carry off | Ratio | Largest RSS ratio |
|---|---|---|---|---|---|---|---|
| Snowghost library modules | 17 | 17 | 0 | 67.15 s | 65.96 s | 1.018 | 1.001 |
| `tests/programs` single files | 61 | 60 | 1 | 15.51 s | 7.49 s | 2.071 | 1.021 |
| Conformance case sources | 1,537 | 1,533 | 4 | 25.91 s | 25.18 s | 1.029 | 1.001 |

The programs group fails the criterion. One run of each program in each
configuration attributes the difference: `wfgrep.wf` takes 10.06 s with the
carry and 2.61 s without it, `dir_walk.wf` 0.98 s and 0.48 s, and
`redis_subset.wf`, untimed because its verdict changes, 1.51 s and 0.46 s;
every other program differs by at most 0.05 s. All three are I/O programs
whose waiting loops hold `match` joins over many owed relations, so each
join re-proves every candidate against a clone of every input state.

The verdict changes are the inputs the carry exists to accept:
`redis_subset.wf` (INV-1 without the carry),
`ent6-pos-break-join-keeps-header-invariant` (FN-9),
`inv1-pos-guarded-cursor-patterns` (TERM-1),
`inv1-pos-sequential-guarded-steps` (INV-1) and `term1-pos-bisection-join`
(INV-1). Snowghost's library modules and the conformance sources stay within
the criterion.

The carry as implemented therefore does not meet the condition of Q30. The
repair `docs/todo.md` names, cloning each input once and proving every
relation against it and skipping relations whose operands no input wrote,
targets exactly the per-relation clone that these programs multiply; it is
measured again with this method before the rule leaves draft.
