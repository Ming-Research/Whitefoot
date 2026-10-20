# Origin transport

## Question and prior criterion

Owner decision Q151 B selects treating a goal over an ordinary let's value
and the same goal over its valid defining expression as one proposition at
disposition. The owner-approved integration uses the precise query-local
equivalence rule below, developed in Q157. The constitutional
requirements are static safety, practical finite checking and no runtime
substitute for a proof. This investigation concerns the proof boundary,
not optimization of a redundant branch.

The discriminating witness binds a two-byte range's length, then
`let parity = iand(length, 1_u64);`. S7 proves the evaluated parity is zero.
A helper requires `iand(bytes^.len, 1_u64) == 0_u64` through a contract
definition. Before this amendment a guard on `parity == 0_u64` supplies the
expanded signed goal through S1; the unguarded call lacks that identity.

Before validation, the criterion is: the unguarded call and a Boolean binding
whose defining comparison becomes provable later discharge; invalidating a
required link on one reaching path prevents transport; nested definitions
and contract definitions compose; records expose the definitions and signed
premise used. Acceptance after a killed link, enumeration of substitution
subsets, or a separate FN-8 proof path rejects the proposal. No local build
or experiment was run; conformance and record tests are CI obligations.

## Selected rule

The existing ENT-3 origin map supplies directed definitions, recorded after
the initializer's obligations succeed. Its eligibility, ordinary support
kills, scope kills and intersection at joins remain authoritative. A link
does not assert that a Boolean initializer is true. A killed link is not
restored by an equal assignment or a newly provable fact.

Two admitted value trees are origin-equivalent at a query when their complete
valid expansions have identical typed trees under ENT-2's proof-path identity.
An exact reference alias and its referent therefore name the same place;
origin transport does not introduce a second binding-based place identity.
Operation row, concrete types,
constants, projections and operand order remain part of identity. There is
no algebraic equality, commutation or function-body inspection. Nested links
are followed only while each is live; an intermediate binding can remain an
endpoint after its own origin dies.

The query uses a finite view of the entering state:

1. Collect live definition sides in binding-declaration order, entering
   signed goals in source-allocation order with positive sign first, then
   the submitted signed goal. Visit each tree's value subtrees in preorder,
   keeping an exact tree only at its first occurrence. Partition the result
   by origin equivalence.
2. For each integer class with L0-representable members, use the first member
   in the fixed collection order as its term representative and equate the
   other L0 members to it. These equalities state only valid definitions,
   including the ordinary offset of a literal through Z.
3. Retain each Boolean tree and one term view replacing its proper integer
   subtrees with those representatives. This contraction introduces no new
   operation or formula shape. Both trees belong to the same origin class.
4. Transport entering signed facts across each Boolean class. A transported
   entering source has its ordinary exact comparison projection. Close this
   fixed numeric view with the existing L0 rules.
5. Apply the existing signed derivation rules to the finite Boolean members
   and share a proved sign with every member of its class until no new sign
   is available. Only existing Boolean introduction combines signs; a derived
   parent supplies no child. Derived signs add no L0 or affine premise.
   Contradiction and disposition then have their ordinary meaning.

The positive Boolean-introduction traversal also follows a visited Boolean
datum's live definition under the demanded sign. This preserves the existing
negative affine child route under `bnot`: `bnot(test)` and
`bnot(x + y < 9_u64)` have the same proof while `test` denotes that comparison.
The negative child proof is retained under the introduction, not published as
an independent sign or granted a standalone negative affine root route.
Memoization of each visited goal and sign belongs to that single unchanged
query view, so repeated definition children share work and a failed child
cannot suppress a later signed-saturation pass.

All consumers use this view at the shared disposition entry. It is discarded
after the query: a proved requirement does not become a source for a later
clause or survive a later kill. Ordinary source establishment,
materialization, joins and loop-head filtering continue to operate on the
ordinary flow state. The proposal does not capture a new Boolean truth at a
let merely because its initializer could be proved there.

An origin-place write removes the link reading that place; facts already
established about the computed binding keep the binding's support. A binding
write removes its own link and links reading it. A join keeps a definition
only when every contributing non-contradictory input keeps it; ordinary
contradiction handling is unchanged. A loop removes links affected by a
continuing kill before querying its body.

Contract `define` remains unconditional erased alpha-expansion under FN-8.
Ordinary-let transport follows that expansion, actual substitution and the
actuals' obligations in the one pre-transfer caller state. No definition is
reevaluated; partial operation syntax does not authorize its own domain.

## Alternatives and discriminating reasons

- **Selected: query-time equivalence using live definitions.** It admits
  facts learned after a binding and uses the existing invalidation boundary.
  It needs a temporary proof view and retained definition premises.
- **Expand only the submitted goal.** It proves a Boolean through its
  comparison but loses the evaluated term needed by the parity witness.
- **Publish currently provable expanded goals at the let.** It misses
  `let test = x < 9_u64;` followed by learning `x < 8_u64`.
- **Persist a congruence closure in the flow state.** It changes pre-kill
  materialization and join contents, capturing extra facts after a link dies.
  The requested disposition rule has no consumer for that larger semantics.
- **Enumerate partial expansions or contractions.** Independent aliases
  multiply formula trees; shared identities and one term view avoid that
  exponential family.
- **Keep the guard or teach FN-8 about S7.** A guard hides the language gap;
  a contract-only route gives one proposition different consumer judgments.

## Cost and records

Expansion identity is a hash-consed DAG of typed nodes, with memoized
definition lookup. It does not materialize a recursively duplicated expanded
tree. Collection and contraction visit finite existing trees. Integer
equalities form a star per class; the Boolean inventory has at most two
views per collected Boolean tree. Each successful saturation step installs
one of the finite signed members. There is no path search, substitution-subset
search, fuel, timeout or acceptance budget. Existing L0 and affine families
retain their own finite costs. This is a complexity argument, not a
performance measurement; CI must establish practical cost.

The ordinary derivation ledger retains let-origin introductions,
origin-equality steps, signed transport and projections of transported
entering sources. A transport names exact input/output goals, sign, ordinary
proof parent and definition introductions needed to compare expansions.
Parent traversal, compaction, hashing and test-side validation handle these
alongside existing source, projection and Boolean-introduction nodes. The
records describe the checked derivation and are not independent authority.

## Delivery and validation

This amendment is integrated with constant-offset disequality over main's
v0.101 specification as v0.102. The v0.101 archive preserves main's active
bytes, including proof-path identity and counted next-header target instances.

Conformance owns source verdicts; compiler tests own retained records and
deliberate corruptions of their premises. Negative kill/join controls already
reject before the change: they detect unsound transport, not a new rejection.
Positive cases lack the transported identity before the amendment. Builds,
tests and compiler checks run only in CI for this task.

The conformance cases under `tests/conformance/cases/` separate these
observations; before/after entries are specification and source-inspection
expectations, pending execution on the base and changed compiler in CI.

| Case | Before | Selected result and discriminating observation |
| --- | --- | --- |
| `ent4-pos-origin-parity` | FN-8 unproved | Accept the supplied S7 parity witness without its guard. |
| `ent4-pos-origin-boolean` | FN-8 unproved | Accept a Boolean saved before its stronger bound is learned. |
| `ent4-pos-origin-nested` | FN-8 unproved | Accept nested Boolean links and nested contract definitions together. |
| `ent4-neg-origin-write` | FN-8 unproved | Remain unproved after an origin-place write. |
| `ent4-neg-origin-write-through-reference` | FN-8 unproved | Remain unproved after a write through a local reference kills the binding's link and the parity link reading it. |
| `ent4-neg-origin-write-through-call` | FN-8 unproved | Remain unproved after a callee `writes` row over the borrowed actual kills the same two links. |
| `ent4-pos-origin-unwritten-control` | FN-8 unproved | Accept the two write cases' shape without the write, so their rejection is attributable to the kill. |
| `ent4-neg-origin-join` | FN-8 unproved | Remain unproved when one reaching input lost the link. |
| `ent4-pos-origin-equal-values` | FN-8 unproved | Equate two represented results with one valid defining expression. |
| `ent4-neg-origin-refuted` | FN-8 unproved | Reject as refuted through a transported negative sign. |
| `ent4-pos-origin-named-conversion` | FN-8 unproved | Accept the named `cvt.wrap` input spelling beside its expanded literal origin. |
| `ent4-pos-origin-affine-child` | FN-8 unproved | Accept the saved Boolean under `bnot` using its negative affine child proof. |

The compiler record tests reuse these cases, inspect definition, equality,
transport and source-projection nodes, and deliberately corrupt definition
premises, signs and an equality offset. The negative-sign case's negated
requirement must accept and retain its negative transport parent. The affine
child case's expanded form must also accept; replacing `bnot(value)` with
`value` must remain unproved, preserving the existing standalone affine
refutation boundary. A separate record fixture observes a transported
entering source's comparison projection, which the parity case alone does
not require.

No pre-existing conformance verdict is edited. Existing guarded direct and
expanded-origin successes retain their ordinary evidence when that already
proves the goal. The named-conversion origin mismatch in `docs/todo.md` is
covered by the new case and removed from that queue; it is the same gap, not
a separate conversion rule. The adjacent obsolete comment claiming Boolean
source decomposition left v0.30 acceptance unchanged is corrected in place.
The existing oversized-module TODO retains its planned split and replaces
stale line/test counts with the maintained size threshold.

Read-only review found missing test-only proof-context fields, stale evidence
documentation and the negative affine child gap described above. All three
were repaired before handoff. CI must confirm formatting, patch whitespace,
compilation, the focused record tests (including their corruption controls),
conformance and the full gate; none of those checks ran during this integration.
The owner-approved query-local equivalence rule is integrated here; the
design/specification approval log entries are reserved for the later
completion step.

## v0.102 integration limits

The integration preserves main's proof-path place identity, counted
next-header target instances and frozen header-relation transport. The
ENT-5 conflict keeps main's proof-path-based support definition and inserts
ENT-4.OT before it. The conformance-manifest conflict keeps both the existing
subdirectory cases and the origin-transport cases, with no verdict edits.
Specifically, main's support text says "the proof path [ENT-2] of any of its
places reads through", while the incoming branch says "any of its places
reads through" after its new ENT-4.OT block. The resolution retains that
block and main's support text. In the manifest, the competing appended runs
start with `sysubdir-run-create` and `ent4-pos-origin-parity`; both complete
runs remain.

The missing `NodePath` import belongs to `flow/goals.rs`; the missing
`GoalOrigin` and `DerivationId` imports belong to the isolated
`goal_origin_kill_tests` module in `flow.rs`. These types are imported from
their current homes, and origin transport explicitly imports its derivation
identity from `state`. Main's goal builders already canonicalize exact
reference aliases through proof paths before origin keys are formed.

Seven offset fixtures required source repairs: the three delivery cases
ended `value_if` with an extraneous semicolon; the three ordinary join cases
needed the canonical `else if` form; and the range-length helper declared a
runtime read performed only in its erased requirement. Their verdicts and
intended observations remain unchanged.

The equality-sentinel loop is a separate unresolved language composition
gap, recorded in `docs/todo.md`: its invariant is an affine premise, not the
L0 bound that offset tightening needs. The approved constant-offset rule
alone supplies no affine-to-L0 publication route. Neither a runtime guard
nor an extra implementation-only inference was added.

This integration ran no build, test, formatter or check locally. CI must
confirm compilation, grammar/effect repairs, signed origin evidence,
proof-path alias behavior, offset closure and the full gate; the loop's
accept verdict remains an outstanding requirement pending the composition
decision. The active v0.102 file and manifest have resolved contents, but
staging them was denied because the worktree's Git index is outside the
writable sandbox. No commit or approval log entry was made.

An independent GPT-6.1 read-only review covered `1a6a96c5b..worktree`, the
merged changes, direct consumers and untracked files, excluding the copied
specification archive. It ran no executable checks. A4, D2, T4, T6, G1-G3
and DC1-DC3 passed by inspection; T1/DC4 retain the loop-composition finding,
and unresolved index entries prevent merge completion. Full C4 soundness,
T1 mechanical checks, V3 completion and design-lint counts remain
unverified; D1, T5, T7 and T8 do not apply to the changed scope. The review
found no further visible import or signature mismatch. Its two documentation
concerns were repaired in place: the offset decision's ground no longer
implies that L0 tightening alone solves the loop, and this record makes
the absence of local checks explicit.
