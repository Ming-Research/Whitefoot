# Run-time tests the fixed derivation already decides

Design of the [OP-5] source redundancy judgment, proposed on a work branch
against Kernel Specification v0.134; the rule is a proposal until the owner
approves the specification change. It ports an October 2026 draft written
against v0.10x, before origin transport ([ENT-4.OT]) landed.

## Question and prior criterion

The owner selected rejecting a run-time Boolean test whose outcome the fixed
automatic derivation already decides, with a repair that deletes the test and
its dead branch, as the language already rejects a proof block that AUTO
makes redundant ([PRF-1]). The owner also selected the strict criterion below
and directed that a test program's check of its own final result move to the
harness that checks the program's output, not to a language test facility.

The question is whether the judgment can reuse the existing condition origins
and proof dispatcher, remove exactly the unnecessary control flow, and leave
generic bodies writable. Before measurement, the acceptance criterion is: the
decided truth is found by the ordinary dispositions in the complete entering
state; a noncontradictory proof of every origin under one truth rejects; an
undecided condition is admitted. The design is rejected if substitution alone
rejects a correct generic body, if a query publishes a premise, or if deleting
a reported test and its dead branch makes a later goal unprovable. Check time
is measured on the natural interpreter and one large maintained program with
interleaved base, base-copy and changed runs; timing selects no verdict, and a
doubling of check time warrants attribution before adoption.

## The rule

The scope is the condition `expr` of every `if_stmt` and `value_if`, each
`else if` link included. [OP-5] owns the judgment because it already owns
condition judgments; [GRAM-6] keeps formation failures and points to [OP-5]
for redundancy. No new syntax or rule ID is needed.

The query point is after the condition's ordinary evaluation, obligations,
effects and publications, before either branch's [ENT-3.S1] facts. A truth
decides the test only when every fact the branch it selects would establish at
entry is already derivable: each member of the condition's goal-origin set,
direct before expanded, through the Signed Goal query with [MSR-4]'s numeric
steps in the [ENT-4.OT] view, and the comparison origin, when present, in the
closed L0 state. True is tried first. The queries add no premise and use no
written `use`; already published invariants, including certified ones, remain
ordinary facts.

The strict criterion is what makes deletion safe. The draft first decided by
any one origin; its witness was a guard `if parity == 0_u64` around a call
requiring `iand(bytes^.len, 1_u64) == 0_u64`, where S7 proved the binding goal
but not its expansion, so deleting the decided guard made the call
unprovable. Under the strict criterion that guard was admitted. Origin
transport has since closed the gap itself: the binding goal's proof now
transports to its live expansion, the unguarded call is accepted
(`ent4-pos-origin-parity`), and the guard is decided and rejected
(`op5-neg-redundant-origin-transport`).

The comparison origin is judged in the closed L0 state rather than through
MSR-4's affine steps, because S1 establishes it as an L0 fact and later L0
consumers read it there; `op5-pos-redundant-comparison-not-l0` admits a test
whose goal an affine certificate proves but whose comparison is not an L0
fact, so its query reads the ordinary closed state only and never falls back
to the origin-transport view that the goal origins use
(`op5-pos-redundant-comparison-transport`). Whether the comparison
origin should instead count as derivable when MSR-4 discharges it is an open
choice for the owner: it would reject more tests, and its soundness depends on
every reader of the lost L0 fact re-deriving it through MSR-4.

A condition with neither a goal origin nor a comparison origin, such as a
user function's result, has no derivable truth: a vacuous "every fact is
derivable" would decide it True and delete a test that decides nothing, so the
judgment admits it (`op5-pos-redundant-no-origin`).

In a contradictory state both truths follow by explosion and no reachable
branch is selected, so the judgment admits the condition. [FN-1] still
rejects structurally unreachable statements, and [FN-8] uninhabited bodies are
admitted (`op5-pos-redundant-uninhabited`).

A nongeneric body is judged at its ordinary instance. A generic body is
judged once at the symbolic instance of its own parameters, under its written
bounds ([FN-2]); a concrete recheck still enforces every ordinary safety
obligation but does not repeat this source judgment. Otherwise `if x < n` in a
const-generic body could be required for most `n` and forbidden for one, with
no body valid for both (`op5-pos-redundant-generic-instance`).

The diagnostic cites [OP-5] at the complete condition, names
`RedundantCondition`, the condition, the decided truth and the facts its
proofs used, and directs deleting the test and the dead branch: the else
branch for True, the then branch (or the whole else-free statement) for
False, keeping the selected branch's execution and delivery.

## Scope inventory and alternatives

| Source test | Same rule? | Discriminating reason |
| --- | --- | --- |
| `if_stmt`, `value_if`, each `else if` | Yes | Explicit Boolean choice with one removable branch. |
| `atomic` guard | No | A waiting predicate re-evaluated under changing shared state, not an ordinary branch. |
| Enum `match`, including Result and Option | No | Elimination binds payloads and discharges ownership; removing an arm conflicts with its coverage judgment. |
| `propagate` | No | Result elimination and error delivery, not a source Boolean choice. |
| Counted-loop header | No | A compiler-owned iteration test; constant ranges keep [FN-1]'s structural edges. |
| Checked and saturating operations and conversions | No | Total value functions with selected result types, not removable branches. |
| A comparison or `.defined` bound or returned as a value | No | Computing a Boolean can implement an interface; no dead branch exists there. |
| `requires`, `ensures`, invariants, proof blocks | No | Erased proof syntax; [PRF-1] owns redundant certificates. |

Accepting a decided test with a warning is rejected: the boundary must be a
specification-fixed source judgment, as [PRF-1]'s is. Rechecking at every
substitution is rejected because it forbids a branch other instances need.
Skipping generic bodies is rejected because redundancy under symbolic bounds
is knowable. Treating a contradictory state as True is rejected because
explosion proves False equally and supplies no reachable branch to keep.
Deciding by any one origin is rejected by the deletion witness above.

## Architecture and cost

The checker records the condition's node on the checked two-armed Bool match
and forms one acceptance record per condition; the entailment walk answers it
where it establishes S1 facts, through the ordinary acceptance-record
interface, so statement, value and chained conditions take one path. The
generic schedule is a flag of the analysis context. Each condition costs at
most two signed queries per origin plus one L0 query per truth; the counters
`conditions`, `condition_queries` and `redundant_conditions` of the checker
work instrument count them.

## Migration

Existing tests that a decided condition now rejects are listed by the
temporary `WHITEFOOT_REDUNDANT_CONDITIONS` instrument (see
[`compiler/src/semantic/check/redundancy_inventory.rs`](https://github.com/Ming-Research/Whitefoot/blob/2ae94da7a5b580971e1fcef32c54ff644752ac0f/compiler/src/semantic/check/redundancy_inventory.rs)) over one gate run. A
genuinely redundant test is deleted; where an observation needs a branch, the
input is made actually unknown while the oracle is kept; a program's check of
its own final result moves to the harness that checks its output. No expected
rejection is rewritten to OP-5 to pass.
