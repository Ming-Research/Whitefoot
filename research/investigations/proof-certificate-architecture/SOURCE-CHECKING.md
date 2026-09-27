# Source proof selection grounds

## Scope and required properties

The [constitution](../../../docs/constitution.md) requires machine-checked
safety and practical compilation. It does not uniquely select a proof language.
This assessment covers the proof-authority and cost arguments attached to
ENT-1, ENT-6, INV-1 and PRF-1 in the [active specification](../../../spec/kernel-spec.md).
It leaves their accepted forms unchanged. The older packet in this directory
records a producer/verifier decision under historical retained-claim semantics;
its lifecycle and approval language does not describe the current system.

A proposition used to admit a partial operation must be established by the
checker. Given the selected model in which a loop invariant supplies such a
proposition, checking its base and preservation on every reachable backedge
is a sound induction obligation. Neither conclusion requires the author to
write every invariant: inference, different certificate calculi and other
checked representations can also satisfy the safety objective.

Proof erasure removes the proof's own runtime instructions. It does not show
that the surrounding algorithm is fast, that the chosen proof is short, or
that checking is cheap. Those are separate obligations under the performance
and development-feasibility objectives.

## Assessed grounds and unresolved choices

The table records the scope of this focused assessment. The later
[historical full-rule assessment](../decision-workflow/RULE-GROUNDS.md#proof-state-and-transport)
examined the remaining facets below and recorded cost and expression questions
as `revisit`. Those labels describe that assessment, not current selections;
it supplies neither new comparative experiments nor a formal soundness proof.

| Rule and assessed aspect | Grounds that hold | What remains open |
|---|---|---|
| ENT-1: authority and deterministic acceptance | Required proof cannot be replaced by a writer assertion or an optimizer result. Given specification-defined derivability, timeouts and machine speed cannot decide whether a source theorem follows. Fixed automatic rules and explicit evidence provide one inspectable implementation strategy. | The constitution does not independently require no SMT or exactly this fact vocabulary. These remain selected mechanisms. Coverage, checking costs and the complete generic/source-schema policy need their own assessment; determinism and termination alone do not establish feasibility. |
| ENT-6: automatic families | Fixed finite families make the admitted proof routes explicit. The current zero-, one-, unordered-pair and L0-image families provide automatic help without unrestricted search. Retaining them is a provisional choice while their limits are investigated. | Neither the exact family boundary nor its practical cost follows from the constitution. A different finite family can remain deterministic and polynomial. The complete closure, image formation, bridges and candidate costs need analysis; counting families alone is insufficient. Integer-domain normalization and product-interval publication are not reassessed here. |
| INV-1: checked invariant authority | Given the chosen invariant model, base and arbitrary-backedge preservation justify induction; local facts must be proved before publication. Erasure serves the runtime-cost objective without trusting the author. | Affine-only syntax, invariant inference versus written annotations, placement restrictions, joins and exact-exhaustion publication are concrete selections, not constitutional necessities. Their full expression and maintenance costs remain unassessed. |
| PRF-1: checked explicit composition | Given already established inequalities, adding them with nonnegative multiplicities preserves their direction; the checked residual must imply the target. Independent premise checks in one entering state avoid circular justification. Explicit choices remove the need to discover those choices. | This does not establish linear total checking cost. Premise verification, mandatory target AUTO, coefficient formation and the final residual still cost work. The 4096-use ceiling, redundancy rejection, exact calculus and all nonlinear-fold cases need separate justification. |

These are conditional arguments about rules, not a verification of the
compiler implementation or a proof that the entire language is sound.
The current index retains `revisit` where the table leaves a specific question.
No historical `derived` label is promoted by replacing its constitutional name.

## Alternatives and evidence boundaries

A larger automatic family, a different explicit calculus, or a checked producer
can satisfy the broad objectives without being selected here. In particular,
adding a fixed finite family need not change polynomial checking into
exponential checking or make the accepted family unspecified. Its actual
algorithm, coefficient representation and source-size costs would decide that
question. The old blanket argument against widening AUTO is not a valid reason
to retain the exact current boundary.

Likewise, rejecting automatically provable `use` blocks is a canonical-source
choice. Accepting an independently checked redundant certificate need not
weaken proof safety. Neither alternative is selected by erasure: both can have
no proof instructions at runtime. The existing rule remains in force; its
value as writer guidance and its maintenance/checking costs remain open.

Existing [source-proof tests](../../../compiler/src/semantic/tests/source_proofs.rs)
include three explicit premises beyond the automatic pair family, a midpoint
certificate, integer tightening, redundant-block rejection and invalid-source
controls. They document executable witnesses of the selected behavior, not
comparative evidence that it is the best authoring contract. The
[certificate-fold decision](https://github.com/mbbill/Whitefoot/blob/0f22b026b062c36ef2f8ce9c725ee4307e1fda64/mcts_mem/whitefoot/checks-and-proofs/certificate-fold.md)
retains dated comparisons of declaration identity against expanded operand
images. That evidence is specific to folding; it does not validate every
aspect of PRF-1.

The historical 2026-09-05 report of 389 ms, 3.0 s and 26.8 s for 64, 128 and
256 entries did not pin a reproducible source/toolchain bundle or isolate the
stages. The [checking-cost experiment](CHECKING-COST.md) supersedes it as the
current cost evidence, separating certificate length from entering-context
size and distinguishing query preparation, closure traversal and long-target
AUTO candidate work. It is not
a reconstruction of those old inputs. Absence of a large certificate in the
existing corpus does not justify its cost, demonstrate general expressiveness,
or make it a constitutional tradeoff already accepted by the owner.

The current implementation makes the distinction visible:
[source proof checking](../../../compiler/src/semantic/entailment/flow.rs)
queries the target for redundancy, checks relation-form premises against the
entering context, checks named-premise availability, accumulates the written
sum and checks its residual. Thus the old argument that explicit steps alone
make total checking proportional to their count is unsupported. Reusing
unchanged query preparation and conservatively skipping dominated closure
products remove measured work without removing any of these checks; they
establish neither linear total cost nor that the
selected proof-authoring contract is optimal.

## Affected set and reopening conditions

The focused reassessment affected the four current index rows, the compiler
guide's cost interpretation and the proof memory's current grounds. MSR-4
shares the AUTO boundary; its complete routing and FN-8/FN-9's contract choices
are now assessed in the linked full-rule assessment. This narrower study does
not certify those mechanisms. Writer patterns continue to teach the specified
forms: none of the open alternatives is a new writing permission.

The checking-cost follow-up changes the implementation cost evidence for
ENT-6/PRF-1, not these conditional safety arguments or the selected language
rules. It updates the compiler's known-cost guidance and implements query-local
reuse under `compiler/proof-query-context`. It supplies no comparison of
automatic-family or redundancy policies; those language choices still stand
on the limited grounds stated above, with their open questions intact.

The v0.52 note in
[host qualification](../../../compiler/src/backend/qualification.rs) describes
preserved host semantics rather than claiming unchanged specification bytes:
the amendment also changes selection-rationale prose. Host mappings and
qualified operations retain their semantics.

Reopen the concrete selections when a target workload exposes unavailable
proofs, a stage-attributed cost study changes the feasibility judgment, or
comparative authoring evidence distinguishes redundancy policies. A change
then follows the ordinary decision procedure and updates its actual material
dependents. Existing examples' migration cost cannot decide language design.
