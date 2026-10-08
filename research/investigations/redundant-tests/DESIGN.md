# Automatically decided Boolean tests

## Question and prior criterion

Q145 option A selects rejection of a run-time test whose outcome the fixed
automatic derivation already decides. Can this judgment use the existing
condition facts and proof dispatcher, with a repair that removes exactly the
unnecessary control flow, without making generic bodies unwritable?

Before implementation or measurement, the acceptance criterion is: both
signs are queried by the ordinary signed-goal disposition in the complete
condition state; a noncontradictory proof of either sign rejects; undecided
conditions remain admitted. Reject the design if substitution alone rejects
a correct generic body, if a query publishes a premise, or if removing a
reported test changes an executed effect. Measure check time on the same
natural interpreter and one large maintained program, with interleaved base,
base-copy noise control and changed executable runs. Timing does not select
acceptance. A doubling of check time warrants attribution before adoption.

## Proposed rule

The owner-approved scope is the Boolean condition of `if_stmt` and
`value_if`, including every `else if` link. OP-5 owns the judgment because it
already owns condition judgments. GRAM-6 continues to own formation failures;
it refers to OP-5 for the new entailment judgment. No new syntax or rule ID
is needed.

At the point after the condition's ordinary evaluation and its obligations,
effects and publications, but before either branch's S1 facts, take the
complete entering ProofContext. Use the condition's S1 goal-origin set,
direct origin before expanded origin, and its independent comparison origin.
A sign decides the test only when every fact its branch would establish at
entry is already derivable: every origin's goal under that sign through the
existing ENT-4/ENT-6 signed-goal disposition and MSR-4, and the comparison
origin under it. True is tried first. Deciding by one origin while another
stays underivable would reject a test whose deletion breaks a later proof
(see Q151 below).
The queries add no premise and use no explicit certificate. Already published
invariants, including those proved by a certificate, remain ordinary facts.
An unavailable origin adds no proposition and cannot decide a test.

In a contradictory state both signs follow by explosion, which selects no
executable branch. This rule admits the condition there. FN-1 still rejects
structurally unreachable statements; a structurally reachable continuation
whose fact state contradicts is permitted, as are FN-8 uninhabited bodies.
Changing that policy would be a separate dead-code judgment, not redundancy
of a test on a reachable value.

A nongeneric body is judged at its ordinary instance. A generic body is
judged once at FN-2's canonical symbolic instance, under the written type,
const and function bounds. Concrete rechecks still enforce every ordinary
safety obligation but do not repeat this source redundancy judgment.
Otherwise `if index < n` in a const-generic body could be required for most
values of n and forbidden for one, without any body valid for both.

The diagnostic cites OP-5 at the complete condition `expr`, identifies
`RedundantCondition`, gives the residual condition and its decided truth,
and names the premises of the retained proof. For True it directs removal
of the test and else branch, preserving the then body; for False, removal
of the test and then branch, preserving the else body (or removing the
entire else-free statement). For a value initializer, retain the selected
delivery with GIVE-1's ordinary let spelling; for a chain, retain the selected
alternative in its enclosing position. Preserve evaluations with effects
when applying a repair: a direct call without a Boolean goal origin cannot
be decided from its implementation. DIAG-1 already specifies repair
requirements without enumerating an inventory.

## Scope inventory and alternatives

| Source test | Same rule? | Discriminating reason |
| --- | --- | --- |
| `if_stmt`, `value_if`, each `else if` | Yes | Explicit Boolean choice with one removable branch. |
| `atomic` guard | No | A waiting predicate re-evaluated under changing shared state; its wait and acquisition protocol is not an ordinary branch. |
| Enum `match`, including Result and Option | No | Exhaustive elimination can bind a payload and discharge ownership; removing an excluded arm conflicts with its coverage judgment. Bool match is already refused by GRAM-6. |
| `propagate` | No | Result elimination and error delivery, with payload extraction and cleanup, not a source Boolean choice. |
| Counted-loop header | No | Compiler-owned iteration test; constant zero-trip ranges retain FN-1's structural edges. |
| `-checked`, `-sat`, checked conversion, saturating conversion where admitted | No | Total value functions with selected result types and overflow policy, not removable Boolean source branches. A proved domain may enable lowering but does not change the selected operation. |
| Comparison, Boolean operation or `.defined` bound or returned as a value | No | Computing a Boolean can implement a callable interface; no dead branch exists at that occurrence. |
| `requires`, `ensures`, invariant, proof block | No | Erased proof syntax; PRF-1 already owns redundant explicit certificates. |

No extension is recommended. Accepting with a warning is rejected by Q145 A:
the boundary must be a specification-fixed source judgment. Rechecking at
every substitution is rejected because it forbids a branch needed by other
instances. Skipping all generic bodies is rejected because unconditional
redundancy can be known under symbolic bounds. Treating contradictory states
as True is rejected because explosion proves False equally and supplies no
reachable-value repair. Extending the automatic proof families as an implicit part of R1 is rejected:
this consumer reads exactly the facts other consumers already have, including
C2 facts when that separate change lands.

## Architecture, cost and evidence plan

Retain the condition's source location alongside the existing checked Bool
match. Form a mandatory acceptance record independently of the flow walker,
answer it where that walk establishes S1 facts, and consume the answer through
the ordinary acceptance-record interface. This keeps statement, value and
chained conditions on one path. Keep the generic scheduling bit in the
analysis context, not in executable semantics. Reuse the existing derivation
ledger for diagnostics and count query work through Q140's instrumentation.

Two signed dispositions per origin add at most a constant factor to querying
one condition; each uses the fixed finite ordinary families. The expanded
and direct origins are the existing bounded S1 set, not a new enumeration of
paths. Unknown results can exhaust AUTO and must be measured on large bodies.

Conformance evidence will cover proved, refuted, chained and value tests;
Boolean origins and affine negation; contradictory and uninhabited states;
symbolically redundant generics and specialization-only decisions; and
controls lacking a certificate, a join fact or a callee guarantee. New rejection
cases must be accepted by the supplied 026074111 base, so the pre-change suite
fails for the missing judgment. Positive controls deliberately pass both;
their purpose is to falsify over-rejection, not to claim a preexisting bug.

Run all conformance and compiler-library tests and the program corpus, check
examples and the interpreter generator's output, and retain a per-file
disposition for each existing rejection. Remove genuinely redundant tests;
when an observation needs a branch, supply an actually unknown input while
preserving its oracle. Do not replace rejection expectations with OP-5 just
to pass. Count source conditions and observed redundant outcomes separately,
including the limitation of earlier errors and repeated analysis runs.

## Ledger

- Q145 A: owner-approved rejection principle and default scope; A2 remains
  outside this work.
- Q147: proposed symbolic-body schedule for generic redundancy. Recommend
  symbolic once; alternatives are per-instance rejection (forbids useful
  generic bodies) or excluding generics (misses provable redundancy).
- Q150: proposed admission in contradictory states. Recommend no OP-5
  rejection there; alternatives are reporting a separate unreachable-state
  error (new policy) or selecting a truth by explosion (invalid repair).

Implementation evidence, corpus dispositions and measured costs will be
recorded in RESULTS.md in this directory. These are proposed details under
Q145 A, not an owner approval record.

## Deletion repair exposes an origin-publication gap (Q151)

The minimized [witness](origin-publication.wf) is accepted by the supplied
026074111 executable. R1 rejects its `parity == 0_u64` condition. Removing
that condition and keeping `need(bytes: view)` makes both executables reject
the call with FN-8: `iand(view^.len, 1_u64) == 0_u64` is Unproved. Replacing
the condition with `invariant even: parity == 0_u64;` also leaves that exact
FN-8 residual. This is separate from partial call-argument origin expansion:
the expression is identical, but only S1 publishes its signed expanded fact.
S7 already bounds the evaluated `parity` value, which decides the direct
comparison. INV-1 publishes an affine relation, not the expanded opaque goal.

The corpus exposed this in the RFC base64 conformance program's capacity
precondition and the IPv4 checksum program's even-length precondition. Their
observations and contracts remain required. An opaque test helper would lose
the needed premise and is not a repair for these guards.

The rule now decides a sign only when every origin is derivable under it, so
this witness is admitted and no rejected test's deletion can break a later
proof. The gap itself remains: a binding's own goal and its defining
expression's goal are distinct identities, so the checker can derive
`parity == 0_u64` and not `iand(view^.len, 1_u64) == 0_u64` although they
state the same proposition while the expansion is valid. Q151 awaits the
owner: recommend identifying a goal over let-bound data with its valid
expansion in the goal disposition, so that both are derivable together,
with replacement, support kills, joins and bounded query work as
falsifiers. Neither option permits changing the two contracts or adding a
runtime guard.

## Observed with the strict criterion

With the compiler of the phase-1 branch (C2's transported relations
included), the rule rejects a decided test in 133 conformance cases and 15
programs under `tests/programs`, and 94 compiler unit tests whose fixtures
contain one. The interrupted first draft, deciding by any one origin and
without C2, had migrated about 180 files.

## Runtime oracles in test programs (Q152)

A run-mode test program checks its computed result at run time, as in
`if r != 3000_u64 { return exit_status(code: 1_u8); }`. When the checker
derives `r`, R1 rejects that test, although it exists to test the generated
code, which the checker's proof of `r` does not establish. The first
draft's migration routed each such comparison through a local function
without a contract. Q152 awaits the owner on how such oracles are written.

