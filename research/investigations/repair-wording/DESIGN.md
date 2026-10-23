# Repair wording

## Question

A source rejection may carry a repair: the `mechanical_fix` text an agent
reads and applies. The specification prescribes many of these texts word for
word inside individual rules, and [DIAG-1](../../../spec/kernel-spec.md) ends
with "A mechanical fix or restructuring is included exactly where the owning
rule requires one." Two defects are known, recorded as the todo entries
"Proposal: disposition-specific restructurings for proof rejections" and
"Printed restructurings have drifted from the specification's texts" on
`fix/diagnostic-source-spelling` (PR #123):

- the printed texts have drifted from the prescribed ones, some rules print a
  repair no rule prescribes, and at least two printed repairs, applied
  literally, lead to a further rejection;
- a `refuted` goal is false where it stands [ENT-4], yet the FN-8 and OP-6
  prescriptions, and the printed FN-8, OP-2, OP-6 and OP-4 repairs, ask to
  establish it for both dispositions.

The owner delegated the choice between two directions, with one criterion:
whatever lets an agent, the main reader, understand and locate the problem
and repair it fastest.

- **(a)** Keep the words normative: amend each rule's prescribed text to be
  disposition-specific and align the compiler.
- **(b)** The specification states only which rejections carry a repair and a
  requirement every repair meets; the words become a compiler decision beside
  `compiler/diagnostic-rendering`, the node PR #117 added, tested by the
  pinned-sentence corpus, so wording can improve from agent evidence without a
  specification version.

The research round changed neither the specification nor the compiler. It
proposed the two design-tree amendments filed as
[`design/amendments/repair-scope.md`](../../../design/amendments/repair-scope.md)
and [`design/amendments/diagnostic-repairs.md`](../../../design/amendments/diagnostic-repairs.md).
The implementation round that followed, on the same branch, is recorded in
[Implementation](#implementation); where it differs from sections 1 to 5,
that section says so and why.

Revisions examined: `main` at `efe40194a`, PR #117 (`feat/readable-diagnostics`)
at `145f368f7`, and PR #123 (`fix/diagnostic-source-spelling`) at `902594280`.
PR #117 and the workflow change of PR #116 merged into main while this ran;
main at `6facd86b8` has the specification of `efe40194a` and the `compiler/`
tree of `145f368f7`, byte for byte (`git diff --stat 145f368f7 6facd86b8 --
compiler/ spec/kernel-spec.md` is empty), so the #117 compiler below is
today's main compiler and every line number below is main's at `6facd86b8`.
Each compiler was built with the gate profile under the host lock, `efe40194a`
from this worktree and the two PRs from `git archive` exports of their heads:

```sh
perl .github/run-check.pl repair-wording-build cargo build --manifest-path compiler/Cargo.toml --profile gate --bin whitefootc --locked --offline
# efe40194a 69.7 s; #123 69.7 s and #117 70.6 s, the same command against each export's manifest
```

Where the text below says "main" without a revision, the statement holds for
both `efe40194a` and `6facd86b8`.

## Criterion

The owner fixed the criterion before this work. Its operational form below was
written after the inventory and probes of sections 1 and 2 had run, so those
observations are exploratory with respect to it; the writer trial in section 5
is the prospective test and its decision rule is fixed there before it runs.

The measure is compile rounds from a rejection to an accepted program in which
the rejected construct still does what the writer meant. A repair that is
rejected elsewhere, moves the rejection to another construct, or makes the
construct unreachable counts as a failed round. Four properties of a rejection
feed that measure:

- **U, understand.** The record states what is wrong in the writer's terms:
  the rule, the construct, the residual goal, and whether that goal is false
  (`refuted`) or merely not proved (`unproved`).
- **R1, applicable.** Every alternative the repair names can succeed for this
  case: its disposition, its position, and the kind of terms the goal reads.
- **R2, concrete.** The repair names the instance: the argument, operand,
  value, index or row.
- **R3, current.** When a trial shows a better repair, the printed text follows
  the evidence.

Location is fixed by DIAG-1 and is not at stake. Option (a) is favored if the
right repair is fixed by rule and disposition alone, if prescribed words stay
equal to printed ones, and if agents act on the specification's words. Option
(b) is favored if the right repair depends on position or instance data a
per-rule sentence cannot carry, if prescribed words drift without a check, and
if repair content improved from evidence in the compiler before the
specification followed.

A single writer trial cannot separate (a) from (b): both can print the same
words on the day of the trial. The trial in section 5 tests the
disposition-specific content, which both options need; the choice between (a)
and (b) rests on the R1 to R3 evidence below.

## 1. Inventory

Method: every sentence of `spec/kernel-spec.md` containing "restructuring",
"mechanical fix", "mechanical repair" or a parenthetical "(spell ...)", mapped
to its rule by the nearest preceding rule definition; every `mechanical_fix`
value outside the test modules of `compiler/src`, including the parser's
`FORM3_*`, `GRAM2_*` and `GRAM9_*` constants, the resolver's collision
constants, and the reason/repair pairs passed to the INV-1 and PRF-1 formation
helpers. Branch differences were read with `git diff origin/main...<branch> --
compiler/src` and by extracting the same literal set from each export.

### Prescribed repairs and the text main prints

Status: **same**, byte-equal at every printing site; **words**, the same remedy
in other words; **content**, a different set of remedies; **missing**, the rule
prescribes one and none is printed; **unreachable**, no written source reaches
the rejection. Compiler paths are under `compiler/src/`.

| # | Rule, spec line | Rejection (kind on main) | Prescribed | Printed on main | Status |
|---|---|---|---|---|---|
| 1 | GRAM-6, 300 | `match` on `Bool` (InvalidConditionalForm) | "(spell `if`)" | "spell the Bool conditional `if`" (`semantic/check/control/matches.rs:610`) | words |
| 2 | GRAM-6, 302 | empty `else` (InvalidConditionalForm) | "(spell the else-free `if`; ...)" | "delete the empty `else` and spell the else-free `if`" (`matches.rs:545`) | words |
| 3 | GRAM-6, 303 | `else` holding one `if` (InvalidConditionalForm) | "(spell `else if`)" | "flatten the nested `if` to `else if`" (`matches.rs:571`) | words |
| 4 | GIVE-1, 324 | empty delivery set (InvalidGive) | "the statement form (`match_stmt` or `if_stmt`) with the binding dropped" | none; `InvalidGive` has no field (unit test `an_empty_delivery_set_rejects_at_the_let_statement`) | missing |
| 5 | TYPE-2, 382 | opaque constructor or destructuring (ContainerConstruction) | `build it with a construction function [OP-13, PRE-1]` | same (`control/results.rs:188`, `expressions.rs:1899`) | same |
| 6 | TYPE-2, 383 | readonly write target (ReadonlyWriteTarget) | `use the operation that changes it, or replace the whole value` | same (`expressions/places.rs:872`, `:1156`, `:1182`) | same |
| 7 | TYPE-8, 504 | reference kind in a stored or argument type | `return an index or other owned data and let the caller form the reference` | no site; the grammar admits no such type [TYPE-8, GRAM-3] | unreachable |
| 8 | TYPE-9, 510 | inline runtime-capacity form (InlineRuntimeCapacityShape) | `wrap it in a Box, or write the constant-capacity form` | same (`check/types.rs:102`) | same |
| 9 | TYPE-9, 513 | move of runtime-capacity content (InlineRuntimeCapacityShape) | `let the Box release it at scope exit, or empty it and call free_empty(move b) [OP-14]` | same (`places.rs:305`) | same |
| 10 | TYPE-9, 515 | storage-shape constructor (ContainerConstruction) | `build it with a construction function [OP-13]` | same (`expressions.rs:1809`) | same |
| 11 | TYPE-10, 520 | window-part read, borrow or write (ReservedPseudoField) | `use the operation that moves the window boundary [OP-10]` | same (`places.rs:1104`) | same |
| 12 | TYPE-7, 524 | reference used as a value (MissingDereference) | `deref(.)` | "write `deref(.)`", "write `deref(holder)`" or "write `deref(p)`" across thirteen sites | words |
| 13 | TYPE-7, 526 | `deref` of a non-reference (MissingDereference) | `a Box's content is its field inner [TYPE-9]; an owned place is named as itself` | same (`places.rs:712`) | same |
| 14 | SET-1, 530 | `set` target resolving to nothing (resolution UnresolvedUse) | `declare the binding with let first` | none (probe `set1-undeclared`) | missing |
| 15 | TYPE-7, 535 | `set` through a reference variable (MissingDereference) | `deref(.)` | "write `deref(.)`" (`control/commit.rs:303`, `:504`) | words |
| 16 | OWN-1, 598 | bare affine place in a body (BareAffineUse) | write `move p` | "write `move p` for the affine place" (`expressions.rs:1304`, `places.rs:220`), and four other remedies in positions the rule does not distinguish: a const array and a const struct (`expressions.rs:1494`, `:1503`), an element (`flat_storage.rs:966`, `:1138`), a borrow destructured (`results.rs:204`) | words, plus four content variants |
| 17 | OWN-1, 598 | bare affine place in a `contract_block` (BareAffineUse) | restate the definition or clause over copy operands or non-consuming admitted reads | same (`check/requires.rs:309`) | same |
| 18 | REF-1, 609 | `&p` of a reference variable (ReferenceToReferenceVariable) | `name the path the reference names` | same (`check/references.rs:881`) | same |
| 19 | REF-2, 628 | use of an invalid reference (InvalidReferenceUse) | `form the reference again after that event` | same (`references.rs:568` and three more) | same |
| 20 | REF-3, 637 | escaping reference (EscapingReference) | `return an index and let the caller form the reference` | same (`references.rs:781` and three more) | same |
| 21 | REF-4, 645 | range over a `Ring` (RangeOverRing) | `a ring hands out single slots; take the elements one at a time` | same (`references.rs:1165`) | same |
| 22 | PROV-6, 722 | partial consume (LinearValuePartiallyConsumed) | `destructure the whole value with let N(f: a, ...) = move v;` | same (`check/linearity.rs:335`, `places.rs:324`) | same |
| 23 | WIN-3, 757 | remaining linear part (InvalidElementMove) | `take it in the same destructuring: let N(f: a, ..) = move v;` | same (`results.rs:346`) | same |
| 24 | WIN-3, 759 | move out of a slot or element (MoveOutOfSlot, InvalidElementMove) | `use take_back, remove_at, or swap [OP-10, OP-11]` | same (`flat_storage.rs:956`, `:1130`, `places.rs:211`) | same |
| 25 | STOR-8, 774 | heap under the no-heap declaration (HeapTypeUnderNoHeap) | `use a constant-capacity shape, or withdraw the no-heap declaration` | same (`types.rs:412`, `calls/user.rs:852`) | same |
| 26 | STOR-5, 804 | reference inside a stored type | `keep the reference as a direct local or parameter; do not store it inside another value` | no site; unreachable as row 7 | unreachable |
| 27 | OP-4, 984 | undischarged subscript bound (UndischargedBoundsObligation) | a dominating branch establishing the residual, a proved header or local invariant, an invariant carrying sufficient `use` steps, or a verified callee relation | "when the relation must hold, establish the residual with a verified requirement, a source invariant, or explicit finite proof steps; use a dominating branch only when its false edge is intended program behavior; otherwise restructure the access" (`check.rs:3544`) | content |
| 28 | OP-6, 1025 | undischarged conversion (UndischargedConversionDomainObligation) | establish that domain or use `cvt.checked` to handle conversion failure as a value | "establish this cvt.defined domain with a verified requirement, an integer range invariant, or explicit finite proof steps; use a dominating cvt.defined condition when refusal is intended behavior, or use cvt.checked to return the failed conversion" (`check.rs:3578`) | content |
| 29 | OP-10, 1099 | operand shape outside the admitted set (TypeMismatch) | `use a shape this operation admits` | none; the `expected` field names the admitted shapes (`generics/operands.rs`) | missing |
| 30 | OP-11, 1114 | unproved proper-ancestry exclusion (OverlappingCallEffects, UndischargedCallSeparation) | `exchange equal or disjoint places without an ancestor relation` | same on OverlappingCallEffects (`calls/user.rs:742`); "prove the two positions distinct before this call, or pass one of them" on UndischargedCallSeparation (`check.rs:3594`) | content on one path |
| 31 | OP-11, 1118 | `swap` over a copy place (SwapOverCopyPlace) | `read the two values and assign them back` | same (`calls/user.rs:887`) | same |
| 32 | OP-14, 1145 | operand shape (TypeMismatch) | `use a shape this operation admits` | none | missing |
| 33 | OP-14, 1147 | undischarged `free_empty` requirement (UndischargedEmptyRunRelease) | `empty the window and establish its zero length at this point; otherwise take every element out and consume it` | same (`check.rs:3657`) | same |
| 34 | FN-2, 1224 | reference-kind type argument | `make the reference a direct written parameter instead of a generic argument` | no site; unreachable as row 7 | unreachable |
| 35 | FN-4, 1257 | formal interface mismatch (TypeMismatch) | `supply an explicitly matching function or weaken the formal interface` | none; `found` is "a nonmatching behavior argument" (`check/behavior.rs:697`) | missing |
| 36 | FN-6, 1269 | polymorphic recursion (PolymorphicRecursion) | `forward the complete generic argument vector unchanged on the cycle, or move the changing instantiation off the cycle` | "forward the complete type, const and function argument vector unchanged on the cycle, or move the changing instantiation off the cycle" (`generics/finiteness.rs:255`) | words |
| 37 | CALL-4, 1411 | ambiguous result route (AmbiguousResultRoute) | ``name the result ordinal the route applies to: write `when b is V(f: r):` `` | none; unit variant (`check/ensures.rs:2220`) | missing |
| 38 | EFF-5, 1490 | overlapping call effects (OverlappingCallEffects, UndischargedCallSeparation) | `prove the two positions distinct, or pass one of them` | same on OverlappingCallEffects; "prove the two positions distinct before this call, or pass one of them" on UndischargedCallSeparation | words on one path |
| 39 | FN-8 in DIAG-1, 1841 | undischarged call requirement (UndischargedCallRequirement) | `establish the complete callee requirement with one dominating branch or one preceding proved invariant before the call` | "when the call is required to succeed, establish the entire instantiated callee requirement with a verified requirement, a source invariant, or explicit finite proof steps before the call; use a dominating branch only when rejection is intended program behavior; otherwise restructure the call" (`check.rs:3665`) | content |
| 40 | FN-8 in DIAG-1, 1842 | the same, with an occurrence-local argument datum | `bind that argument or referent value with one preceding ordinary let, establish the complete requirement over that binding, and pass the binding, borrowing it when the parameter mode requires a borrow` | the same with "establish the entire instantiated requirement" (`check.rs:3663`); no test pins it, and two probes did not reach it | words |
| 41 | ENT-2, 2421 | counted endpoint that is not a term (InvalidCountedEndpoint) | `bind the computed u64 value with one preceding ordinary let and use that term as the endpoint` | same (`control/loops.rs:881`) | same |
| 42 | MSR-3, 2545 | misplaced `entry` former (InvalidEntryFormer) | `use entry only on a reference parameter the row writes, in ensures` | "use entry only on an exclusive parameter in ensures" (`ensures.rs:834`) | content |
| 43 | CALL-6, 2789 | contradictory contract (ContradictoryPublishedRelations) | `state one consistent relation set: a contract whose clauses cannot hold together publishes every fact at every caller` | same (`ensures.rs:891`) | same |
| 44 | MSR-4, 3093 | any unproved numeric goal; the consumers without their own prescription are OP-2, OP-9, INV-1 targets and FN-9 | a dominating branch whose false edge handles the domain outcome, a preceding proved invariant with an optional `use` block, or a verified callee relation; for a subscripted offset that is not a term, a preceding `let` | OP-2: "when the relation must hold, establish the fixed `.defined` normalization with a verified requirement, a source invariant, or explicit finite proof steps; use a dominating branch only when its false edge is intended program behavior; otherwise use an available total non-exact row or restructure the arithmetic" (`check.rs:3557`); OP-9 and INV-1 print their own; FN-9 prints none | content, FN-9 missing |
| 45 | PRF-1, 3232 | redundant proof block (UndischargedSourceProof) | removing the complete block | "remove the use block; AUTO already proves this invariant target from the same entering context in this specification version" (`check.rs:3491`) | words |

Short names: `check.rs` is `semantic/check.rs`; the rest are under
`semantic/check/`, with `places.rs`, `flat_storage.rs` and `calls/user.rs`
under `expressions/`, `matches.rs`, `commit.rs`, `loops.rs`, `results.rs` and
`proofs.rs` under `control/`, and `operands.rs` and `finiteness.rs` under
`generics/`.

Two further sentences are general: OWN-8 (line 661) says a reject-when-unsure
diagnostic "names the rule and a restructuring", and DIAG-1 (line 1875) is the
"exactly where the owning rule requires one" sentence.

Counts over the 45 prescriptions: 3 unreachable; of the 42 reachable, 20 are
printed byte-equal and 22 differ, 10 in words only, 6 in content and 6 not
printed at all. The content drift runs both ways. At rows 27, 39 and 44 the
specification's remedy lists omit a requirement of the enclosing function, the
[ENT-3] S4 source that repaired every unproved probe below in one round, and
the compiler lists it; at row 42 the compiler kept the retired "exclusive
parameter" term the specification replaced.

### Printed repairs no rule prescribes

Counted by hand from the sites below: 69 distinct texts.

| Rule | Texts | Sites |
|---|---|---|
| FORM-3 | 3, one per name class | `syntax/parser/diagnostic.rs:62-64` |
| GRAM-2 | 1, contract-block order | `diagnostic.rs:71` |
| GRAM-9 | 2, body and contract form | `diagnostic.rs:52-53` |
| TYPE-6 | 3, declaration collisions | `resolution/engine/inventory.rs:439-441` |
| EFF-1 | 5: category order, repeated entry, non-parameter root, undeclared member, inadmissible suffix | `check/types.rs:46-56` |
| EFF-2 | 1: "declare exactly the row the body exhibits: add every missing category and path and remove every extra one; ..." | `check.rs:426` |
| CONST-1 | 1 | `types.rs:1105` |
| INV-1 | 3 formation texts: target arithmetic, target capacity, duplicate name | `check.rs:3399`, `:3403`, `control/loops.rs:795` |
| PRF-1 | 8 certificate texts besides the redundant block | `check.rs:3485-3510` |
| INV-1 and PRF-1 | 25 affine-formation texts | `control/proofs.rs:162-981` |
| REF-4 | 1, range formation: "establish lo <= hi and hi <= x.len with ..." | `check.rs:3619` |
| OP-12 | 1 | `calls/user.rs:681` |
| OWN-1 | 7: use after move (2), `move` of a copy value (4), move through a reference (1) | `expressions.rs`, `places.rs`, `flat_storage.rs`, `commit.rs`, `results.rs` |
| WIN-3 | 1, linear assignment target | `commit.rs:455`, `expressions.rs:379` |
| OWN-11, CONST-2 | 2: loop status (OWN-11), immutable written argument (CONST-2 or OWN-11) | `loops.rs:763`, `places.rs:845` |
| LIV-1 | 1 | `matches.rs:878` |
| FN-1 | 1, `break` outside a loop | `loops.rs:1131` |
| GRAM-6 | 1, condition type | `matches.rs:368` |
| TYPE-10 | 1, member of a measure | `places.rs:534` |
| PROV-6 | 1, value not consumed | `linearity.rs:309` |

Under DIAG-1's "exactly", each of these 69 texts is a defect, yet several came
from writer evidence. On the day of the
[blind-writer report](../../experiments/blind-writer/2026-08-28/REPORT.md#63-diagnostics-scorecard),
2026-08-28, which scored the rejections without a fix as "the ones that cost
time", the compiler gained the GRAM-9 repair (`1feb44b5a`), the TYPE-6 and
FORM-3 repairs (`dfb0a30d8`) and the EFF-1 and EFF-2 repairs (`d896f738c`); no
specification version followed. On 2026-09-01 (`010126feb`) the proof
repairs stopped leading with a dominating branch ("use a dominating branch
only when its false edge is intended program behavior"); the specification's
FN-8 sentence is unchanged since `441cd5b83` (2026-08-10), and rows 27 and 44
still lead with the branch.

### Differences on PR #117 and PR #123

No repair text differs: the literal sets extracted from both exports, and from
main at `6facd86b8`, equal `efe40194a`'s outside test modules.

- **#117**, now on main, prints the same text as a `mechanical_fix: <text>`
  line of the lean record and as `detail.mechanical_fix` in JSON, in place of
  the `Debug` field. Its investigation found the repair sentence the largest
  part of both records it measured, an OP-4 and an FN-8 rejection.
- **#123** adds EFF-1's `SubsumedEffectRead` rejection with no repair, because
  EFF-1 prescribes none, and turns EFF-2's `expected_row` into the merged row
  every call accepts. The EFF-2 repair text is unchanged, so following its
  add-and-remove procedure no longer produces `expected_row` (probe
  `eff2-merge` below). EFF-5 payloads print source spellings instead of
  `<binding:0>`.

## 2. Disposition and literal application

### Which disposition each proof repair addresses

[ENT-4] defines the three dispositions: at a non-contradictory point a goal
is refuted when its negation is derivable and unproved when neither sign is.

| Repair | Stated for | Establish remedy valid for | Disposition in the payload on main |
|---|---|---|---|
| FN-8 (rows 39, 40) | both, one sentence | unproved | yes |
| OP-6 (row 28) | both | unproved; `cvt.checked` for both | yes |
| OP-2 via MSR-4 (row 44) | unproved only in the specification; printed for both | unproved; the total rows for both | yes |
| OP-4 (row 27) | both | unproved | no |
| OP-9 via MSR-4 | unproved in the specification; printed for both | unproved | no |
| REF-4 range formation | not prescribed; printed for both | unproved | no |
| INV-1 targets via MSR-4 | unproved in the specification; "weaken or correct ... or establish" printed | both, as two alternatives | no |
| FN-9 via MSR-4 | unproved in the specification; nothing printed | none printed | yes |
| EFF-5 and OP-11 separation (rows 30, 38) | unproved position pairs | unproved; "pass one of them" also for a pair two arguments supply | not computed |
| OP-14 (row 33) | both | both: "otherwise take every element out" is the refuted remedy | no |

### Probes

The probes are in [`probes/`](probes/): a rejected source per rule and
disposition, and beside it the same source after one repair alternative is
applied as written. They seed step 2 of the validation plan, and the directory
is deleted once the pinned-sentence corpus carries those pairs. Each was
compiled with each compiler:

```sh
for f in probes/*.wf; do whitefootc --emit-llvm -o /dev/null "$f"; echo "exit=$?"; done
```

Exit 1 prints one rejection; exit 0 is acceptance. All 51 probes give the
same exit and rule on `efe40194a`, #117 and #123, except
`eff1-order-reordered` on #123. The records of four of them on `efe40194a`,
in its `Debug` rendering, abridged:

```text
op2-refuted:  [OP-2] UndischargedIntegerDomainObligation { residual: "255_u8 +defined 1_u8", disposition: Refuted, mechanical_fix: "when the relation must hold, establish the fixed `.defined` normalization with ..." }
fn8-refuted:  [FN-8] UndischargedCallRequirement { concrete_callee: "small", instantiated_goal: "20_u64 < 10_u64", disposition: Refuted, mechanical_fix: "when the call is required to succeed, establish the entire instantiated callee requirement with ..." }
fn9-refuted:  [FN-9] UndischargedPostcondition { concrete_function: "f", relation: "20 - 10 <= -1", disposition: Refuted }
op4-refuted:  [OP-4] UndischargedBoundsObligation { residual: "5_u64 < values.len", mechanical_fix: "when the relation must hold, establish the residual with ..." }
```

The same FN-8 rejection as main prints it since #117 merged, which is what an
agent reads today:

```text
probes/fn8-refuted.wf:8:11: error[FN-8]: UndischargedCallRequirement
  source:   let r = small(x: 20_u64);
  marker:           ^^^^^^^^^^^^^^^^
  concrete_callee: small
  requires_clause: probes/fn8-refuted.wf:2:3 "requires x < 10_u64;"
  instantiated_goal: 20_u64 < 10_u64
  disposition: Refuted
  mechanical_fix: when the call is required to succeed, establish the entire instantiated callee requirement with a verified requirement, a source invariant, or explicit finite proof steps before the call; use a dominating branch only when rejection is intended program behavior; otherwise restructure the call
```

Of the alternatives in that last line, the requirement, the invariant and the
branch fail for this record (probes below), `use` steps cannot prove a refuted
target from consistent premises, and "restructure the call" names no change;
the disposition two lines above already says why.

| Probe | Starting point | Repair applied as written | Result on main |
|---|---|---|---|
| `op2-refuted-requires` | `op2-refuted`: `255_u8 + 1_u8` | move it into a function with `requires 255_u8 +defined 1_u8;` | FN-8 refuted at the new call |
| `op2-refuted-local-invariant` | `op2-refuted-local`: `x = 255_u8; x + 1_u8` | `invariant fits: x <= 254_u8;` | INV-1 |
| `op2-refuted-local-branch` | `op2-refuted-local` | `if x +defined 1_u8 { ... }` | accepted; built and run, it exits 0 through the other return, so the addition never runs |
| `op2-refuted-wrap` | `op2-refuted` | `255_u8 +wrap 1_u8` | accepted |
| `op2-unproved-requires` | `op2-unproved`: `x + 1_u8`, `x` a parameter | `requires x +defined 1_u8;` | accepted |
| `op6-refuted-requires` | `op6-refuted`: `cvt::<u32, u8>(256_u32)` | move it into a function with `requires cvt.defined::<u32, u8>(256_u32);` | FN-8 refuted at the new call |
| `op6-refuted-invariant` | `v = 256_u32`, then the conversion | `invariant fits: v <= 255_u32;` | INV-1 |
| `op6-refuted-checked` | `op6-refuted` | `cvt.checked::<u32, u8>(256_u32)` and match | accepted; runs the `Err` arm, exit 1 |
| `op6-unproved-requires` | `op6-unproved`: `x` a parameter | `requires cvt.defined::<u32, u8>(x);` | accepted |
| `fn8-refuted-requires` | `fn8-refuted`: `small(x: 20_u64)`, `requires x < 10_u64` | call from a helper that `requires 20_u64 < 10_u64;` | FN-8 refuted at the helper's call |
| `fn8-refuted-invariant` | `a = 20_u64`, then the call | `invariant fits: a < 10_u64;` | INV-1 |
| `fn8-refuted-branch` | `a = 20_u64`, then the call | `if a < 10_u64 { ... }` | accepted; exits 0, so the guarded call never runs |
| `fn8-refuted-argument` | `fn8-refuted` | `small(x: 5_u64)` | accepted |
| `fn8-unproved-requires` | `fn8-unproved`: `small(x: y)`, `y` a parameter | `requires y < 10_u64;` on the caller | accepted |
| `fn8-unproved-invariant` | `fn8-unproved` | `invariant fits: y < 10_u64;` | INV-1 |
| `fn8-unproved-branch` | `fn8-unproved` | `if y < 10_u64 { ... }` | accepted |
| `fn9-refuted-requires` | `fn9-refuted`: returns `20_u64`, `ensures result < 10_u64` | `requires 20_u64 < 10_u64;` | FN-8 refuted at the caller |
| `fn9-refuted-weaken` | `fn9-refuted` | `ensures result <= 20_u64;` | accepted |
| `fn9-unproved-requires` | `fn9-unproved`: returns a parameter | `requires x < 10_u64;` | accepted |
| `op4-refuted-invariant` | `op4-refuted`: `values[5_u64]` of four | `invariant inside: 5_u64 < values.len;` | INV-1 |
| `op4-refuted-branch` | `op4-refuted` | `if 5_u64 < values.len { ... }` | accepted; the access can never run |
| `op4-refuted-index` | `op4-refuted` | `values[3_u64]` | accepted |
| `op4-unproved-requires` | `op4-unproved`: `deref(b)[i]` | `requires i < deref(b).len;` | accepted |
| `op4-unproved-invariant` | `op4-unproved` | `invariant inside: i < deref(b).len;` | INV-1 |
| `op9-unproved-requires` | `op9-unproved`: capacity a parameter | `requires length <= 1000_u64;` | accepted |
| `ref4-unproved-requires` | `ref4-unproved`: `&deref(values)[0_u64..hi]` | `requires hi <= deref(values).len;` | accepted |
| `own1-element-move` | `own1-element`: bare read of a `nocopy` element, OWN-1 | OWN-1's prescribed `move p` | WIN-3; main prints the element variant of row 16 instead, which avoids this |
| `eff1-order-reordered` | `eff1-order`: `writes(stats.count), reads(stats.count)`, EFF-1 order | "move every `reads` entry ahead of the first `writes` entry" | EFF-5 at the call on main; EFF-1 `SubsumedEffectRead` on #123 |
| `eff2-merge-add-missing-main` | `eff2-merge` on main: missing `["reads(stats)"]` | "add every missing ... and remove every extra one" | EFF-5 at the call |
| `eff2-merge-add-missing-pr123` | `eff2-merge` on #123: missing `["writes(stats)"]` | the same | EFF-5 at the call, on both compilers |
| `eff2-merge-expected-row` | `eff2-merge` on #123 | declare `expected_row`, `writes(stats)` | accepted |

The two refuted probes `op9-refuted` (`capacity:
18446744073709551615_u64`) and `ref4-refuted` (`&values[0_u64..5_u64]` of
four) print no disposition; their residuals are ground false. `set1-undeclared`
records the missing SET-1 repair of row 14. `fn8-unproved-element`, an attempt
to reach the occurrence-local argument path of row 40 through a subscripted
borrow actual, rendered the structural goal `values[0] < 10_u64` instead, as did
an owned subscripted actual, so that repair was not observed.

### What the probes show

Seventeen literal applications failed: eleven establish repairs of refuted
goals (`op2-refuted-requires`, `op2-refuted-local-invariant`,
`op2-refuted-local-branch`, `op6-refuted-requires`, `op6-refuted-invariant`,
`fn8-refuted-requires`, `fn8-refuted-invariant`, `fn8-refuted-branch`,
`fn9-refuted-requires`, `op4-refuted-invariant`, `op4-refuted-branch`), two
invariant alternatives for unproved goals over unconstrained parameters
(`fn8-unproved-invariant`, `op4-unproved-invariant`), and four structural
repairs (`eff1-order-reordered`, both `eff2-merge-add-missing` probes,
`own1-element-move`). Every refuted goal was repaired in one round by changing
what reaches it, the operation, or the clause that poses it
(`op2-refuted-wrap`, `op6-refuted-checked`, `fn8-refuted-argument`,
`fn9-refuted-weaken`, `op4-refuted-index`); every unproved goal whose terms are
parameters was repaired in one round by a requirement.

That the refuted establish repairs cannot succeed follows from the
specification, not from these programs. [ENT-4]'s closure is monotone, so a
fact source added where the refuting facts still reach the construct leaves
the negation derivable; once the goal is also established both signs are, the
state is contradictory, and everything discharges there. A requirement makes
the instance uninhabited, and [FN-8] then rejects every call, "which no
reachable non-contradictory caller state can do"; a branch leaves the
construct only on a contradictory edge, which the runs above confirm never
executes; an invariant cannot be proved, since its target is refuted. Only a
change to what reaches the construct, or to the operation or clause that poses
the goal, removes the refutation.

### Testing the requirement proposed for (b)

The requirement proposed with option (b), that a repair applied literally is
not rejected again by the same rule for the same construct, flags one of the
seventeen failed applications: `eff1-order-reordered`, and only on the #123
compiler. The others were rejected by another rule (FN-8, INV-1, EFF-5,
WIN-3), at another construct (a caller, the invariant, every call), or
accepted with the construct dead.

Two conditions separate all seventeen from the fourteen applications that
worked. First, the repaired construct's judgment succeeds in a state that is
not contradictory: this excludes every refuted establish repair and every
invariant that cannot be proved. Second, no rule rejects the text the repair
writes, where it is written or at a use of it, whatever the rest of the
program is: this excludes the reordered row, which EFF-5 rejects at every call
on main and EFF-1 rejects outright on #123, the added-missing rows, which
EFF-5 rejects at every call, and `move` of an element, which WIN-3 always
rejects. Section 4 states both in DIAG-1.

## 3. Options against the criterion

### What conformance evidence reads

None of it reads repair text. The Rust adapter maps a source rejection to
`Verdict::Reject(failure.rule_id())` and keeps the rendered text only as a
failure note (`compiler/tests/conformance/adapter.rs:83-92`), and
`Expectation::matched_by` compares rule ids
(`compiler/tests/conformance/corpus.rs:56-63`). The Python runner's
expectation schema is `accept`, `reject` with a rule, `run` with an exit, or
`unsupported` with a reason (`tests/conformance/runner.py:21-26`, `:91`). The
`doc` prose of 28 case files mentions a repair; nothing compares it.

The texts are pinned only by compiler tests: pinned-sentence rows for GRAM-9,
FORM-3, GRAM-2, TYPE-6, CALL-6, EFF-1 (repeated entry, suffix, member) and
EFF-2, and unit tests for OP-2 (`OVERFLOW_FIX`, `DIVISION_FIX`), OP-4 and FN-8
(`semantic/tests/entailment.rs`) and several structural repairs, among them
FN-6, CALL-6, OWN-1, TYPE-7, PROV-6, WIN-3 and ENT-2. None pins the EFF-1
order repair or the FN-8 occurrence-local repair. Two tests apply a suggestion
and compile the result: the GRAM-9 contract-block test in `driver.rs`
(`the_contract_block_repair_gram9_names_is_accepted`, "A repair the compiler
refuses is worse than no repair"), and PR #123's EFF-2 tests, which declare
`expected_row` rather than follow the printed words.

### (a) Normative words, amended per disposition

Normative: every repair sentence, a refuted and an unproved variant for each
goal rejection, and, to make "exactly" true, either the 69 unprescribed texts
added to the specification or removed from the compiler. Nothing checks the
words: conformance reads rule ids only, so the drift of section 1 would recur
unless a new compiler test compared each specification sentence with the
printed one. Rows 16, 39 and 44 show the other cost: the right remedy depends
on position (six bare-affine variants) and on the goal's terms (a requirement
for parameters, an invariant or callee relation for computed values), so each
rule's sentence becomes a case analysis or stays wrong for some cases. A fixed
sentence names no instance unless the specification defines templates over
payload fields. Every improvement a trial motivates is a specification
amendment and version.

### (b) Normative carry set and conditions, compiler-owned words

Normative: which rejections carry a repair; the two conditions of section 2
that every alternative meets; the payload, including the disposition of every
goal rejection. The compiler owns the words, pinned with rejected and repaired
source pairs. Costs: a second implementation may word repairs differently,
which DIAG-1 already permits ("Cross-implementation byte identity is required
only where this specification explicitly fixes both selection and encoding");
the specification no longer shows a reader the repair words; the owner reviews
wording in compiler pull requests rather than in the specification; and the
pinned corpus gains a repaired source per repair, one compilation each, which
is an implementation cost of the next round.

A risk specific to (b) is a vague repair ("restructure the access") that
formally meets the conditions. The proposed DIAG-1 text requires each
alternative to name what to add, remove or replace, and where, and the pinned
pair requires the application to compile, so a vague alternative cannot be
pinned.

### (c) Considered: normative content, free words

The specification would state each rule's remedies per disposition and leave
only the words free. It keeps the case analysis of (a): rows 16, 39 and 44
show that the remedy itself, not only its wording, depends on position and on
the goal's terms, so each rule's content statement must enumerate those cases
or stay wrong for some of them, while (b)'s two conditions exclude every
failure the probes found without enumerating any.

### Evaluation

| Property | (a) | (b) |
|---|---|---|
| U: disposition and residual visible | needs the payload change in either option | the same |
| R1: every alternative applicable | only as far as a per-rule sentence can branch on position and terms | required of every alternative by the DIAG-1 conditions, checked per pinned pair |
| R2: names the instance | only through templates the specification would define | the compiler interpolates any payload field |
| R3: follows evidence | each change is a specification version; history shows the compiler moved first and the specification did not follow | a compiler change with its pinned pair |
| Drift | recurs unless a new test compares the two texts | no second copy of the words exists |
| Specification shows the words | yes | no; it states the conditions |

The grounds are the criterion's R1 to R3. They are empirical for R3 and the
drift (sections 1 and 2) and deductive for R1: the contradiction argument of
section 2 shows why a per-disposition split is necessary, and the terms of the
goal select among the unproved routes. The language tree's rule that a
language choice is made on merits and not on the effort of a change is
respected: the argument is the agent's rounds, not the cost of amending the
specification. The choice would change if agents came to read repairs from
the specification rather than from the rejection, or if a second
implementation needed identical repair words.

## 4. Recommendation

Option **(b)**, with the two conditions of section 2 instead of the same-rule
check, and with the disposition in every goal rejection's payload.

### Proposed DIAG-1 text

Replace line 1875, "A mechanical fix or restructuring is included exactly where
the owning rule requires one.", with:

> A rejection carries a repair where its owning rule states that it does, and
> any other rejection may carry one.
> A repair names one or more alternative source changes, each saying what to
> add, remove, or replace and where, selected by the rejected construct, its
> position, and, for a goal, its disposition [ENT-4].
> Applied as written, each alternative lets the rejected judgment succeed at
> that construct in a state that is not contradictory [ENT-4], and writes
> nothing that a rule rejects, where it is written or at a use of it, whatever
> the rest of the program is.
> A refuted goal's repair therefore changes what reaches the construct, such as
> its operands, arguments, or returned value or the statements and
> requirements that fix them, or changes the operation or clause that poses
> the goal, since establishing a refuted goal while the facts that refute it
> remain makes that point contradictory.
> The words of a repair are not specified.

In the FN-8 paragraph, delete line 1841 and shorten line 1842 to:

> When the payload contains an occurrence-local call-argument evaluated-value
> datum, it additionally renders that datum as `argument #N pre-transfer
> value`, with N the zero-based argument ordinal.

### Per-rule specification edits

MSR-4, after line 3027 ("Each keeps its own normalization ..."), add:

> Each consumer's rejection names its goal's disposition, `refuted` or
> `unproved` [ENT-4], which the FN-8 and FN-9 payloads of [DIAG-1] carry in
> their fixed tuples, and carries a repair [DIAG-1].

and delete lines 3093 to 3095 ("The mechanical repairs for an unproved Goal
are ...", the subscripted-offset instruction, and "Writing a proposition
without one of these derivations establishes nothing.", which [SCOPE-2] and
[ENT-3] already state).

Every other prescription becomes a reference to DIAG-1:

| Line | Rule | Current fragment | Proposed fragment |
|---|---|---|---|
| 300 | GRAM-6 | "at the scrutinee `expr` node (spell `if`)." | "at the scrutinee `expr` node, with a repair [DIAG-1]." |
| 302 | GRAM-6 | "at that `if_stmt` node (spell the else-free `if`; a `value_if`'s undelivering else is [GIVE-1]'s rejection, not this one)." | "at that `if_stmt` node, with a repair [DIAG-1]; a `value_if`'s undelivering else is [GIVE-1]'s rejection, not this one." |
| 303 | GRAM-6 | "(spell `else if`); ... so the flattening fix is never demanded where the chain form could not be spelled." | ", with a repair [DIAG-1]; ... so no repair demands a chain form that could not be spelled." |
| 324 | GIVE-1 | "; the mechanical fix is the statement form (`match_stmt` or `if_stmt`) with the binding dropped." | ", with a repair [DIAG-1]." |
| 382 | TYPE-2 | "each with the restructuring `build it with a construction function [OP-13, PRE-1]`" | "each with a repair [DIAG-1]" |
| 383 | TYPE-2 | "with the restructuring `use the operation that changes it, or replace the whole value`" | "with a repair [DIAG-1]" |
| 504 | TYPE-8 | "with the restructuring `return an index or other owned data and let the caller form the reference`" | "with a repair [DIAG-1]" |
| 510 | TYPE-9 | "with the restructuring `wrap it in a Box, or write the constant-capacity form`" | "with a repair [DIAG-1]" |
| 513 | TYPE-9 | "with the restructuring `let the Box release it at scope exit, or empty it and call free_empty(move b) [OP-14]`" | "with a repair [DIAG-1]" |
| 515 | TYPE-9 | "with the restructuring `build it with a construction function [OP-13]`" | "with a repair [DIAG-1]" |
| 520 | TYPE-10 | "with the restructuring `use the operation that moves the window boundary [OP-10]`" | "with a repair [DIAG-1]" |
| 524 | TYPE-7 | "with the mechanical fix `deref(.)`" | "with a repair [DIAG-1]" |
| 526 | TYPE-7 | "with the restructuring `a Box's content is its field inner [TYPE-9]; an owned place is named as itself`" | "with a repair [DIAG-1]" |
| 530 | SET-1 | "with the restructuring `declare the binding with let first`" | "with a repair [DIAG-1]" |
| 535 | TYPE-7 | "with the mechanical fix `deref(.)`" | "with a repair [DIAG-1]" |
| 598 | OWN-1 | "The bare-affine mechanical fix is position-conditional: ... so the repair never instructs a spelling FN-8 forbids." | "Each of these rejections carries a repair [DIAG-1]." |
| 609 | REF-1 | "with the restructuring `name the path the reference names`" | "with a repair [DIAG-1]" |
| 628 | REF-2 | "carrying the invalidating event and the restructuring `form the reference again after that event`" | "carrying the invalidating event and a repair [DIAG-1]" |
| 637 | REF-3 | "with the restructuring `return an index and let the caller form the reference`" | "with a repair [DIAG-1]" |
| 645 | REF-4 | "carrying the restructuring `a ring hands out single slots; take the elements one at a time`, because" | "carrying a repair [DIAG-1], because" |
| 661 | OWN-8 | "the diagnostic names the rule and a restructuring." | "the diagnostic names the rule and carries a repair [DIAG-1]." |
| 722 | PROV-6 | "with the restructuring `destructure the whole value with let N(f: a, ...) = move v;`" | "with a repair [DIAG-1]" |
| 757 | WIN-3 | "with the restructuring `take it in the same destructuring: let N(f: a, ..) = move v;`" | "with a repair [DIAG-1]" |
| 759 | WIN-3 | "with the restructuring `use take_back, remove_at, or swap [OP-10, OP-11]`" | "with a repair [DIAG-1]" |
| 774 | STOR-8 | "each with the restructuring `use a constant-capacity shape, or withdraw the no-heap declaration`" | "each with a repair [DIAG-1]" |
| 804 | STOR-5 | "with the restructuring `keep the reference as a direct local or parameter; do not store it inside another value`" | "with a repair [DIAG-1]" |
| 984 | OP-4 | "Its mechanical fix is a dominating branch establishing the residual [ENT-3], ... or a verified callee relation [FN-9]." | deleted; MSR-4's consumer sentence covers it |
| 1025 | OP-6 | "rendering its canonical domain goal, with the repair to establish that domain or use `cvt.checked` to handle conversion failure as a value." | "rendering its canonical domain goal [MSR-4]." |
| 1099 | OP-10 | "with the restructuring `use a shape this operation admits`" | "with a repair [DIAG-1]" |
| 1114 | OP-11 | "with the restructuring `exchange equal or disjoint places without an ancestor relation`" | "with a repair [DIAG-1]" |
| 1118 | OP-11 | "with the restructuring `read the two values and assign them back`" | "with a repair [DIAG-1]" |
| 1145 | OP-14 | "with the restructuring `use a shape this operation admits`, exactly as" | "with a repair [DIAG-1], exactly as" |
| 1147 | OP-14 | "rendering the residual, with the restructuring `empty the window and establish its zero length at this point; otherwise take every element out and consume it`" | "rendering the residual and its disposition [ENT-4], with a repair [DIAG-1]" |
| 1224 | FN-2 | "with the restructuring `make the reference a direct written parameter instead of a generic argument`" | "with a repair [DIAG-1]" |
| 1257 | FN-4 | "the differing signature, row, clause, or result ordinal, and the restructuring `supply an explicitly matching function or weaken the formal interface`." | "and the differing signature, row, clause, or result ordinal, and carries a repair [DIAG-1]." |
| 1269 | FN-6 | "with the restructuring `forward the complete generic argument vector unchanged on the cycle, or move the changing instantiation off the cycle`" | "with a repair [DIAG-1]" |
| 1411 | CALL-4 | "carrying the restructuring ``name the result ordinal the route applies to: write `when b is V(f: r):` ``" | "carrying a repair [DIAG-1]" |
| 1490 | EFF-5 | "carrying both substituted paths and the restructuring `prove the two positions distinct, or pass one of them`" | "carrying both substituted paths and a repair [DIAG-1]" |
| 2421 | ENT-2 | "and the restructuring `bind the computed u64 value with one preceding ordinary let and use that term as the endpoint`" | "and a repair [DIAG-1]" |
| 2545 | MSR-3 | "with the restructuring `use entry only on a reference parameter the row writes, in ensures`" | "with a repair [DIAG-1]" |
| 2789 | CALL-6 | "naming the clauses and carrying the restructuring `state one consistent relation set: ...`" | "naming the clauses and carrying a repair [DIAG-1]" |
| 3232 | PRF-1 | "; removing the complete block is the mechanical repair." | ", with a repair [DIAG-1]." |

OP-2 and FN-9 need no edit: MSR-4's consumer sentence gives both a repair,
and FN-9 gains one it does not have today. The carry set is therefore every
rejection the table names, every MSR-4 consumer, and, through OWN-8, every
reject-when-unsure rejection. The amendment lands as one specification
version with the outgoing bytes archived, per AGENTS.md.

### Proposed design-tree amendments

Filed for the owner's ruling:

- [`design/amendments/repair-scope.md`](../../../design/amendments/repair-scope.md)
  adds to the `language` root the decision that the specification fixes which
  rejections carry a repair and what an alternative may direct, but not its
  words, with the refused alternatives (a), (c) and the same-rule check. The
  task asked only for the compiler amendment; this one is added because the
  DIAG-1 change is a choice about what the specification states, which the
  language root owns.
- [`design/amendments/diagnostic-repairs.md`](../../../design/amendments/diagnostic-repairs.md)
  adds `compiler/diagnostic-repairs` beside the live
  `compiler/diagnostic-rendering`, which PR #117 added: repair words selected
  by construct, position, disposition and the goal's terms and naming the
  instance; refuted and unproved repair content; repairs pinned with repaired
  sources. Each decision states the kind of its ground, as the design-tree
  skill asks: the refuted/unproved split is a deduction from [ENT-4] with the
  probes as observation, and the instance-naming wording is provisional until
  the writer trial.

PR #123's `compiler/rejection-payloads` amendment already decides, for EFF-2
alone, that "A suggested repair is one the language admits wherever it is
applied". When both are ruled, that sentence becomes an instance of the DIAG-1
condition and of `compiler/diagnostic-repairs`, and should be normalized
upward, keeping in `rejection-payloads` only what `expected_row` merges.

### Proposed compiler wordings

Written for the next round, under (b); braces name payload fields or values
the checker holds. "Terms are parameters" means every datum of the goal is an
entry value of the enclosing function, which a `requires` clause can name.

| Rejection | Refuted | Unproved |
|---|---|---|
| FN-8 | "`{goal}` is false for the values reaching this call: pass arguments that satisfy it, or change the statements or requirements that fix them" | terms are parameters: "add `requires {goal};` to `{caller}`, or guard the call with `if` when skipping it is intended"; otherwise: "establish `{goal}` before the call with a proved `invariant` (adding `use` steps if needed) or an `ensures` relation of the callee that produced the value, or guard the call with `if` when skipping it is intended" |
| FN-8, argument datum | as refuted above | "argument #{N} is computed at the call, so no fact can name it: bind it with a preceding `let`, establish `{goal}` over the binding, and pass the binding, borrowing it when the parameter is a reference" |
| FN-9 | "the value returned here makes `{relation}` false: return a value that satisfies it, or correct the `ensures` clause or the requirements that fix the value" | "`{relation}` is not proved at this return: add `requires` over the parameters it reads, prove it with an `invariant` or a callee `ensures` relation before the return, or weaken the clause to what the body proves" |
| OP-2 | "the operands reaching this `{op}` make `{goal}` false, so the exact operation can never run here: change the operands or their type, or write the {modes} form whose result the program intends" | terms are parameters: "add `requires {goal};`, guard the operation with `if {goal}` when the other outcome is intended, or write the {modes} form"; otherwise the invariant and callee-relation routes as FN-8 |
| OP-6 | "the value reaching this conversion is outside `{Dst}`: convert a value `{Dst}` holds, choose a destination type that holds it, or use `cvt.checked::<{Src}, {Dst}>` and handle `Err`" | "add `requires {goal};` (terms are parameters) or guard with `if {goal}`, or use `cvt.checked` and handle `Err`"; an invariant route only for an integer source |
| OP-4 | "`{index}` is not below `{len}` here: index within the storage, or size the storage to hold this index" | terms are parameters: "add `requires {residual};`, or guard the access with `if {residual}` when the other outcome is intended"; in a loop: "state `{residual}` as a header `invariant`"; offset not a term: "bind the offset with a preceding `let`, and an integer `cvt` if needed, and index with the binding" |
| OP-9 | "a count of {n} makes the allocation larger than u64 bytes: allocate fewer elements" | "add `requires {residual};`, or bound the count with `if {residual}` when refusing is intended" |
| REF-4 formation | "`{residual}` is false: choose endpoints with start <= end <= the source length" | "add `requires {residual};`, or guard the formation with `if {residual}`" |
| INV-1 target | "`{name}` is false where it is stated: correct or weaken it" | "`{name}` is not proved from the facts reaching it: establish them before it, or add `use` steps" |
| OP-14 | "the window still holds elements here: take every element out and consume it before `free_empty`" | "establish `{residual}` here, or take every element out and consume it" |
| EFF-5, OP-11 separation | not computed | "`{first}` and `{second}` are not proved distinct: establish that their positions differ before the call, or pass only one of them" |
| EFF-5 structural overlap | two arguments: "pass disjoint places, or pass one of them"; one argument supplying both paths: a change to the callee's row, pending PR #123's owner question | |
| EFF-1 | order: "write the row as `{canonical row}`", reordered with every subsumed read dropped; subsumed read (#123): "delete `{entry}`; `writes({path})` covers it" | |
| EFF-2 | "declare exactly `{expected_row}`", the row #123 makes callable | |
| Missing today | GIVE-1: "drop `let {binding} =` and write the `{match or if}` statement"; SET-1: "declare `{name}` with `let` before this `set`"; OP-10 and OP-14 shape: "pass one of {admitted shapes}"; FN-4: "supply a function whose {differing part} matches the formal, or weaken the formal"; CALL-4: "name the result ordinal: write `when {binder} is {Variant}(...)`" | |

The two FN-8 probes would then read, in main's record:

```text
  instantiated_goal: 20_u64 < 10_u64
  disposition: Refuted
  mechanical_fix: `20_u64 < 10_u64` is false for the values reaching this call: pass arguments that satisfy it, or change the statements or requirements that fix them

  instantiated_goal: y < 10_u64
  disposition: Unproved
  mechanical_fix: add `requires y < 10_u64;` to `caller`, or guard the call with `if` when skipping it is intended
```

Each is shorter than the 292 characters printed today (148 and 96), and each
alternative in it compiled in one round in the probes (`fn8-refuted-argument`,
`fn8-unproved-requires`, `fn8-unproved-branch`).

The payload changes these need: a disposition on OP-4, OP-9, REF-4 formation
and INV-1 rejections, which the checker already computes for all but INV-1
(`ObligationOutcome::refuted`); the goal's term kind, which the goal datum
records; and the FN-8 caller's name. The FN-8 datum is rendered today as
`<argument #N pre-transfer value>` with angle brackets, a small drift from the
DIAG-1 spelling to correct in the same round. Renaming the payload field
`mechanical_fix` to `repair` would align the label main prints with the
specification's word; it is optional.

## 5. Validation plan

1. **Probe matrix.** For every rule in the wording table, one refuted and one
   unproved rejected source where both exist (FN-8 twice more, for
   parameter-only terms and for the argument datum), each with the source
   after applying every alternative its new repair names. The probes here are
   the seed: `255_u8 + 1_u8`, `cvt::<u32, u8>(256_u32)`, a literal actual
   outside a callee requirement (`small(x: 20_u64)`), and an ensures relation
   false at its return (`return 20_u64` under `ensures result < 10_u64`), with
   their unproved twins over parameters.
2. **Pinned pairs.** Each pinned-sentence row gains its repaired sources. The
   test requires each repaired source to be accepted and, where the construct
   could be left dead, to run with an exit status only the construct produces,
   such as the operation's result. A deliberate mutation must fail the test in
   each direction: pinning the current establish repair to a refuted probe
   (rejected elsewhere or dead), and pinning a repaired source that no longer
   compiles. A green run then shows that each pinned repair, applied as
   written to its probe, compiles with the construct live; it does not show
   that every repair works in every program.
3. **Writer trial**, measured in compile rounds to a fix. Twelve small
   programs, each seeded with one defect whose intended behavior an output
   oracle fixes: one refuted and one unproved defect for each of FN-8, FN-9,
   OP-2, OP-6 and OP-4, plus the EFF-2 read-and-write row and the EFF-1 order.
   Condition A prints main's texts with #123 merged; condition B prints
   the section 4 wordings. The same model, prompt and tools are used in both;
   the writer sees only the compiler record, one compilation is one round, and
   a run ends at eight rounds. Each defect runs three times per condition.
   Measured: rounds until the program compiles and passes its oracle, and dead
   ends, meaning a repair rejected elsewhere or an accepted program failing the
   oracle.
   Decision rule, fixed before the trial: B is kept when, over the refuted
   defects, the median rounds fall by at least one and no refuted defect that
   finishes under A fails to finish under B, and over the unproved defects the
   median does not rise. Otherwise the wording is revisited; the (a) or (b)
   choice does not depend on this trial. Of the investigation skill's four
   observations, the trial makes only the second, whether the tested agent
   writes the program with the supplied repair help; expressibility,
   composition and runtime cost are held fixed by the seeded programs. The
   model and the harness are its conditions, so the result describes them
   only, not a ceiling on the language. It belongs in a new
   `research/experiments/` record when run.

## Implementation

Option (b) landed as specification v0.73 with the minimum repair set, the
MSR-4 goal rules (FN-9 gains a repair) and OWN-8's rejections. It departs
from section 4 where carrying it out showed a gap:

- **DIAG-1's success condition.** Section 4's "Applied as written, each
  alternative lets the rejected judgment succeed" cannot hold for the proof
  routes of a goal over computed values: an invariant's `use` steps or a
  callee's `ensures` succeed only when facts exist that the checker cannot
  guess, so an unconditional requirement would forbid the routes a writer
  needs most. v0.73 reads "Carried out as it directs, each alternative lets
  the rejected judgment succeed at that construct in a state that is not
  contradictory; an alternative that does so only when the program meets a
  condition the checker has neither established nor refuted states that
  condition", and the no-rejected-text condition reads "where it is written,
  or at any use of it, independently of the rest of the program".
- **INV-1 dispositions.** MSR-4 now also says when an INV-1 target is
  refuted: when its disposition derives the negation of one of its bounds.
  The compiler derives it with the same affine disposition, over the exact
  integer complement of the bound (`sum <= u` becomes `-sum <= -u - 1`).
- **What a goal reads**, which selects the unproved routes, is classified in
  four ways rather than section 4's two. A requirement is offered only for
  parameters that no event on a path to the goal writes or consumes. The flow
  state records the bindings each path's kill events write, and a merge takes
  their union: a function-wide set lost the route for
  `free_empty(window: move window)`, whose consume follows its requirement,
  and a set accumulated over the walk lost it for a goal in the arm walked
  after a sibling arm's write, so swapping two arms changed the repair (found
  in the completion review). An admitted element read is part of the goal's
  identity, so a guard naming the same expression establishes it [ENT-3],
  while a value only its occurrence identifies, or a range formed at the
  call, is bound with a `let` first; for a subscript, whose bound names terms
  alone, an offset that is itself an element is bound too, as the retired
  MSR-4 sentence said.
- **A callee's `ensures`** is offered only when a term of the goal, or FN-9's
  returned value, is a value a call to a function outside the prelude
  returned, directly or through local computation; with no such result among
  them the route can never be carried out (also found in the completion
  review), and a prelude function's contract is not the writer's to change.
- **Subscripts a requirement forms.** Main's v0.71 (#118) makes a subscript
  below a readonly field or a measure a term in a clause, owing OP-4 at body
  entry in the state the requirements written before its clause build. Such
  a rejection's repair adds a requirement ahead of the one that forms the
  place and offers no guard, since a clause evaluates nothing.
- **A refuted subscript** offers a longer storage only for a constant offset:
  an offset such as `r.len` is out of range at every length, as main's
  `ent2-neg-requirement-names-append-slot-element` shows.
- **OP-9's ceiling is no bound to write.** OP-9's residual bound is the
  language's target-independent ceiling, `u64::MAX / stride_ceiling(T)`
  (`2305843009213693951` for 8-byte elements), and every supported target's
  allocation domain is smaller: `requires n <= 2305843009213693951_u64;`
  passes OP-9, and the program then stops at [STOR-6] target qualification
  with a failure that names no site or bound (completion review, finding B).
  Every OP-9 route now names the ceiling as the language's limit for the
  element type, says the selected target admits less, and asks for the
  largest count the program needs, the guard's condition included. The
  OP-9 pairs' repaired programs call the allocating function from `main` and
  must build as well as check, and a separate test shows the ceiling itself
  stopping at target layout. Naming the site and the admitted bound in that
  target failure is recorded in `docs/todo.md` ("A target-layout failure
  names no allocation site or admitted bound").
- **Guards through a reference.** A guard is executable code: `if hi <=
  deref(values).len` in a `pure` function is a read EFF-2 rejects. The pinned
  pairs found this; a guard over a goal that reads through a reference
  parameter now also asks for any read the row does not yet declare.
- **OP-14** offers no guard, since a window left alive is released at its
  scope exit, which owes the same empty proof [PROV-6].
- **Validation step 2** checks liveness without running programs: each
  repaired source must be accepted, and no judgment in its functions may
  succeed only in a contradictory state, which is what a guard around a
  refuted goal compiles to (`op2-refuted-local-branch`); a separate test pins
  that the check catches it. The pairs cover every repair branch the
  compiler prints for a goal and every non-goal repair this round changed.
  FN-8's occurrence-local argument, which section 2 never reached, is reached
  by a field of an element read through a range reference, a value the
  entailment does not admit. The EFF-1, EFF-2 and EFF-5 pairs pin today's
  rules and change with PR #126's.
- **Rendering.** FN-8's occurrence-local datum now renders as DIAG-1 spells
  it, `argument #N pre-transfer value`, without angle brackets.

The writer trial of validation step 3 has not run.

## Owner decisions

1. Option (b) as recommended, or (a).
2. The DIAG-1 conditions (a non-contradictory success, and no text a rule
   rejects whatever the rest of the program is) instead of the same-rule,
   same-construct check.
3. The carry set: the rejections that prescribe a repair today, every MSR-4
   consumer (which gives FN-9 a repair) and OWN-8's reject-when-unsure
   rejections, with any other rejection allowed one; or a repair on every
   rejection.
4. The disposition in every MSR-4 consumer's payload, new for OP-4, OP-9 and
   INV-1.
5. The added language-root decision, and its placement in the root rather than
   a new child node.
6. Normalizing PR #123's EFF-2 repair decision upward once both are ruled, and
   the repair for a row whose two entries one argument supplies, which waits on
   PR #123's open question about such rows.

## Found along the way

Outside the requested change, each with its disposition:

- DIAG-1 fixes FN-9's payload as the instantiated normalized relation, so a
  writer reads `20 - 10 <= -1` for `ensures result < 10_u64`
  (`fn9-refuted`); the clause in source spelling, `20_u64 < 10_u64`, would
  read as written. Recorded in `docs/todo.md` ("FN-9 prints its relation in
  normalized form").
- Four compiler comments cite DIAG-3, the v0.39 runtime claim-trap record
  that v0.40 retired with claims. Recorded in `docs/todo.md` ("Compiler
  comments cite the retired DIAG-3"); this round changes no compiler file.
- An FN-8 goal over an element renders `values[0] < 10_u64` without the
  literal's suffix, on main and on #123 (`fn8-unproved-element`). Declined
  here: PR #123's todo item on the second resolved-place renderer records it.
- EFF-5's "prove the two positions distinct, or pass one of them" is printed
  for a pair one argument supplies (`eff2-merge-add-missing-main`), where
  neither alternative applies; no test pins the EFF-1 order repair or the
  FN-8 argument-datum repair; and the compiler's MSR-3 repair keeps the
  retired "exclusive parameter". All three are inside the proposal (the
  wording table, validation step 2, and row 42).
- PR #123's two todo entries that started this work are not on main yet;
  when #123 merges, this branch points them at this investigation.

## Limitations

- The inventory was taken by hand from the sites listed; the count of 69
  unprescribed texts may be off by a few, which changes no conclusion.
- The probes are minimal programs chosen to isolate one judgment each; they
  show that a repair can fail, not how often agents meet each case.
- Option (a) was assessed from its prescribed texts and their history, without
  building a compiler for it; adding a test that compares specification and
  printed texts would remove its drift but not the case analysis or the
  missing instance data.
- The occurrence-local argument repair (row 40) was not reached by a probe in
  the research round; the implementation round reached it (see
  [Implementation](#implementation)).
- Probe outcomes that depend on running a program were observed for
  `op2-refuted-local-branch` (exit 0), `fn8-refuted-branch` (exit 0) and
  `op6-refuted-checked` (exit 1); the other acceptances were compiled only.
