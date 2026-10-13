# Range helper frames implementation

## Delivery and selection ground

The amendment on `claude/range-helper-frames` implements the owner-selected
[entry-version and one-sided unchanged frames](DESIGN.md) as specification
v0.136 over the v0.135 archive, on top of a merge of main at v0.135. The
sections below record the specification changes, the compiler mechanisms, the
conformance cases and the validation state.

On 2026-10-11 the owner selected decision B: the implicit entry read follows
RANGE-1's existing defined-read premise. An exit tuple absent at entry has a
vacuous frame instance and gives the caller no definition. No separate
definedness obligation exists. The identifier collisions with the newly
reserved word are all those the active corpus scan found, not only the one in
the cancellation program.

The status board, the approval logs (`spec/log.md`, `design/log.md`) and
`design/skill` are untouched by this work. Specification and design approval
remain the owner's.

## Specification changes, rule by rule

| Rule | Before | After and selection ground |
| --- | --- | --- |
| GRAM-5 | Range conclusions were two affine operands around a comparison; `unchanged` was an identifier. | Adds the factored `unchanged` suffix. RANGE-1 admits only a single element place in a range postcondition. Guard and ordinary-clause syntax also recognize the suffix so their semantic refusals cite RANGE-1. FORM-3's existing derived reservation makes the word reserved. Selected by the original helper-frame note. |
| GRAM-6 | The explanatory clause shape mentioned only a bare affine expression or two around an operator. | Includes the parsed suffix, which `clause_expr` accepts only so that RANGE-1 refuses it with a repair, and names the `range_relation` conclusion of a range postcondition as the admitted spelling. A grammar-correspondence wording change, with no additional admitted context. |
| MSR-3 | Entry formers denoted ordinary projected entry measures or fragment-integer places. | Points to RANGE-1 for range reads. Existing former admission and refusals remain: only a reference parameter whose row writes it, in ensures; own and nonwriting parameters still reject MSR-3. No new decision beyond the selected range use. |
| RANGE-1 | Range terms read no entry image; no unchanged conclusion existed. | Admits entry-rooted postcondition places and `P unchanged` for an element projection of a written reference parameter, without a final measure or equality capability. Generic instances retain this conclusion for any T. Requirements, guards, loop/type invariants, ordinary relations, whole windows, measures and nonwriting roots reject unchanged with RANGE-1 and DIAG-1 repair. Plain consumed own storage retains RANGE-1. The implicit entry read participates in ordinary read-definedness, as owner decision B selected. |
| RANGE-2 | Versions described ordinary projection values; calls imported exit facts after forgetting writes. | Adds abstract whole-element reads whose identities are unknown after overlapping writes. Calls record entry storage views before effects. Imported unchanged instances define requested exit projection reads by recorded entry reads under the fact's bounds, guards and definedness. Copy wording now says value projections: copying retains values, while whole-container replacement cannot recover old identity. The latter follows the note's explicit refusal of written/restored storage and fixes review R1. |
| RANGE-3 | Ordinary range comparisons were owed and proved through the fixed derivation. | An unchanged obligation is equality of exit and entry whole-element reads; imported frame definitions join step 2. Automatic instantiation remains exactly two rounds, with the existing shared 256-instance and 4096-atom ceilings. Selected by the note, without a new theory or search. |

The active file is v0.136. Its v0.135 archive is byte-identical to the
merge base's active specification: both SHA-256 values are
`e8a54d37ac561bed4bb770b353c622ad8c3358b8e9953bac3541615d2d208e33`.
No other archive was changed. The specification approval log is the owner's
to write.

## Compiler mechanisms and owning functions

| File | Functions or representation | Mechanism |
| --- | --- | --- |
| `compiler/src/semantic/range_facts.rs` | `CheckedRangeRoot::Entry`, `CheckedRangeProjection::WholeElement` | Entry and exit places remain distinct; one unchanged conclusion stays one opaque equality rather than type-field expansion. |
| `compiler/src/semantic/check/range_clauses.rs` | `range_place`, `range_relation`, new `range_unchanged`, `range_guard` | Forms entry places; admits a structurally single element place only in an active postcondition; checks written-reference root and no final measure; emits paired reads preserving written indices; refuses guard misuse before Bool or operand checking. |
| `compiler/src/semantic/check/range_clauses/aggregate.rs` | `RangeOperand::into_term` | Retains arbitrary selected types for identity framing without equality expansion or omission. |
| `compiler/src/semantic/check/ensures.rs` | `check_entry_formers` documentation | Retains the existing MSR-3 reference/write-row former admission. An initially added own-entry RANGE-1 special case was removed in review R2. |
| `compiler/src/semantic/check/requires.rs` | `validate_clause_condition` | Refuses ordinary-relation unchanged with RANGE-1 and repair before evaluating its operand. |
| `compiler/src/semantic/range_judgment/walk.rs` | `Walker::run`, `Walker::copied`, `entry_places` | Materializes and freezes entry views before the body; copies now receive distinct versions while preserving value projections. |
| `compiler/src/semantic/range_judgment/walk/calls.rs` (new) | `snapshot_entry_places`, `call`, `assume_postconditions`, `result_values`, `owe_postconditions` | Owns extracted call/exit logic. Snapshots only entry-rooted places before call effects, including descriptors and resolved aliases, then overlays immutable views when importing or owing postconditions. Explicit returns and propagated error exits retain ordinary selection. |
| `compiler/src/semantic/range_judgment/world.rs` | `World::read`, `projections_overlap`, `VersionDef::Copied` documentation | Whole identities are untyped mathematical reads. Every owned descendant write overlaps the identity. Copied versions retain values, not source storage identity. |
| `compiler/src/semantic/range_judgment/facts.rs` | `collect_triggers`, `Query::add_instance`, `expand` | Whole selections trigger at existing descendant reads; imported instances register conditional definitions; write hits and copied versions never restore whole identity. Existing two-round tuple generation/accounting remains. |
| `compiler/src/semantic/range_judgment/facts/unchanged.rs` (new) | `Definitions::record`, `expand`, `definitions`, `read` | Demand-driven projection definitions close under step 2, including reads introduced in round two. Bound/guard/definedness and conditional premises are retained, and new atoms share the ordinary ceiling. |
| `compiler/src/semantic/range_judgment/constants.rs` | `judge` query construction | Initializes the additional frame-definition state without changing constant judgment behavior. |
| `compiler/src/syntax/terminal.rs`, `grammar/generator/model.rs` | Terminal enum/inventories/spelling mapping | Registers the reserved word in specification first-occurrence order. |
| `compiler/src/semantic/check/repairs.rs` | Existing killed-call measure repair | Explicitly recommends equality of entry and exit measure values; corresponding driver sentence pins follow it. |

Generic clauses use the existing symbolic and concrete re-formation path.
Imported definitions use projections of actual reads at the concrete caller,
so a `Block.entry_slot` read survives a generic drop-capable helper without
assuming T has equality or eagerly listing all of T's fields. The existing
`walk/windows.rs` grow/place_back/take_back transitions and
`walk/operations.rs` S7 relations supply the inherited base behavior; this
amendment consumes those general transitions.

Call/exit extraction keeps the growing walk below the required limit:
`walk.rs` has 3,631 lines. Every touched or added `compiler/src` file is below
4,000 lines by source inspection.

## Conformance cases and before behavior

24 added/replaced case entries cover the selected acceptance list. The old
`range1-neg-entry-image` becomes `range1-pos-entry-image` because the endpoint
is now admitted. All expectation changes follow the active specification;
identifier-only edits retain their prior verdicts. A further table below lists
the cases that pin the refusal and soundness boundaries of the admitted forms.

Baseline command: the supplied released compiler
`../rel4173/whitefootc --check FILE`, from main
`4173ec89e9fe187108fc40679427cd39bb533293`. The released before-behavior in
the first table was measured on that compiler before the amendment; the
amended compiler's results come from CI, as the validation section records.

| Case | Expected verdict | Released before behavior |
| --- | --- | --- |
| [range2-pos-helper-append-frame](../../../tests/conformance/cases/range2-pos-helper-append-frame.wf) | accept | GRAM-5 / UnexpectedToken (exit 1) |
| [range2-pos-helper-append-affine-frame](../../../tests/conformance/cases/range2-pos-helper-append-affine-frame.wf) | accept | GRAM-5 / UnexpectedToken (exit 1) |
| [range2-pos-helper-append-boundary](../../../tests/conformance/cases/range2-pos-helper-append-boundary.wf) | accept | GRAM-5 / UnexpectedToken (exit 1) |
| [range2-pos-helper-reserve-entry-frame](../../../tests/conformance/cases/range2-pos-helper-reserve-entry-frame.wf) | accept | RANGE-1 / InvalidRangeClause (exit 1) |
| [range2-pos-helper-reserve-boundary](../../../tests/conformance/cases/range2-pos-helper-reserve-boundary.wf) | accept | RANGE-1 / InvalidRangeClause (exit 1) |
| [range3-neg-helper-length-only](../../../tests/conformance/cases/range3-neg-helper-length-only.wf) | RANGE-3 | RANGE-3 / UndischargedRangeFact (exit 1) |
| [range3-neg-unchanged-written-element](../../../tests/conformance/cases/range3-neg-unchanged-written-element.wf) | RANGE-3 | GRAM-5 / UnexpectedToken (exit 1) |
| [range3-neg-unchanged-replaced-window](../../../tests/conformance/cases/range3-neg-unchanged-replaced-window.wf) | RANGE-3 | GRAM-5 / UnexpectedToken (exit 1) |
| [range3-pos-unchanged-beyond-entry-domain](../../../tests/conformance/cases/range3-pos-unchanged-beyond-entry-domain.wf) | accept | GRAM-5 / UnexpectedToken (exit 1) |
| [range2-neg-helper-frame-new-element](../../../tests/conformance/cases/range2-neg-helper-frame-new-element.wf) | FN-8 | GRAM-5 / UnexpectedToken (exit 1) |
| [range1-neg-unchanged-type-invariant](../../../tests/conformance/cases/range1-neg-unchanged-type-invariant.wf) | RANGE-1 | GRAM-5 / UnexpectedToken (exit 1) |
| [range1-neg-unchanged-readonly-parameter](../../../tests/conformance/cases/range1-neg-unchanged-readonly-parameter.wf) | RANGE-1 | GRAM-5 / UnexpectedToken (exit 1) |
| [range1-neg-entry-readonly-parameter](../../../tests/conformance/cases/range1-neg-entry-readonly-parameter.wf) | MSR-3 | MSR-3 / InvalidEntryFormer (exit 1) |
| [range1-pos-entry-image](../../../tests/conformance/cases/range1-pos-entry-image.wf) | accept | RANGE-1 / InvalidRangeClause (exit 1) |
| [range1-neg-unchanged-requirement](../../../tests/conformance/cases/range1-neg-unchanged-requirement.wf) | RANGE-1 | GRAM-5 / UnexpectedToken (exit 1) |
| [range1-neg-unchanged-guard](../../../tests/conformance/cases/range1-neg-unchanged-guard.wf) | RANGE-1 | GRAM-5 / UnexpectedToken (exit 1) |
| [range1-neg-unchanged-loop-invariant](../../../tests/conformance/cases/range1-neg-unchanged-loop-invariant.wf) | RANGE-1 | GRAM-5 / UnexpectedToken (exit 1) |
| [range1-neg-unchanged-measure](../../../tests/conformance/cases/range1-neg-unchanged-measure.wf) | RANGE-1 | GRAM-5 / UnexpectedToken (exit 1) |
| [range1-neg-unchanged-whole-window](../../../tests/conformance/cases/range1-neg-unchanged-whole-window.wf) | RANGE-1 | GRAM-5 / UnexpectedToken (exit 1) |
| [range1-neg-unchanged-ordinary-relation](../../../tests/conformance/cases/range1-neg-unchanged-ordinary-relation.wf) | RANGE-1 | GRAM-5 / UnexpectedToken (exit 1) |
| [range2-neg-helper-frame-uncovered-element](../../../tests/conformance/cases/range2-neg-helper-frame-uncovered-element.wf) | FN-8 | GRAM-5 / UnexpectedToken (exit 1) |
| [range3-neg-unchanged-copied-window](../../../tests/conformance/cases/range3-neg-unchanged-copied-window.wf) | RANGE-3 | GRAM-5 / UnexpectedToken (exit 1) |
| [range1-neg-entry-own-postcondition](../../../tests/conformance/cases/range1-neg-entry-own-postcondition.wf) | MSR-3 | MSR-3 / InvalidEntryFormer (exit 1) |
| [range1-neg-entry-own-requirement](../../../tests/conformance/cases/range1-neg-entry-own-requirement.wf) | MSR-3 | MSR-3 / InvalidEntryFormer (exit 1) |

The append-frame case includes generic `push_item<T: drop>` and its concrete
Block instance. The affine-frame case gives Block an owned Box, making it a
non-equality type. The append-boundary case imports frames through
`push_item` and `push_or_fail`, preserving the inverse invariant at explicit
return and propagated error exit. Reserve cases retain an entry-version value
equality through grow_paged. Caller negatives owe FN-8 at the element outside
the declared frame, or at a new element with no entry read. The length-only
case retains its RANGE-3 failure at the propagated boundary.

Fourteen scratch before-behavior controls isolate the new refusal/proof sites:
eleven original controls and the copy-replacement control accept; the two
caller controls retain their intended FN-8 failures at `need_zero`. The
controls remove the unavailable frame spelling or use a harmless existing
relation, so they establish ordinary source validity, not amendment behavior.
They exposed fixture formatting and unused effect-row issues, which were
fixed before handoff. Readonly refusal cases return u64 so result-selector
admission does not hide the intended diagnostic.

Boundary cases pin where the admitted forms stop and which writes defeat an
`unchanged` conclusion. Their expectations are specified by the rules named;
the site column says which compiler path produces the verdict. None has a
released-compiler before behavior, because the released compiler lacks the
`unchanged` spelling.

| Case | Expected verdict | Site that produces it |
| --- | --- | --- |
| [range1-neg-entry-requirement-written-parameter](../../../tests/conformance/cases/range1-neg-entry-requirement-written-parameter.wf) | MSR-3 | `check_entry_formers` refuses an entry former outside an `ensures_clause`; the parameter is written, so only the position is at fault. |
| [range1-neg-entry-loop-invariant](../../../tests/conformance/cases/range1-neg-entry-loop-invariant.wf) | MSR-3 | The same check covers the function body, so a loop invariant's entry former is refused before its range clause is formed. |
| [range1-neg-unchanged-entry-rooted](../../../tests/conformance/cases/range1-neg-unchanged-entry-rooted.wf) | RANGE-1 | `range_unchanged` admits only a place whose root is a written reference parameter binding; an entry root is refused. |
| [range1-neg-unchanged-result-rooted](../../../tests/conformance/cases/range1-neg-unchanged-result-rooted.wf) | RANGE-1 | The same root test refuses a result-ordinal root. |
| [range3-neg-unchanged-field-write](../../../tests/conformance/cases/range3-neg-unchanged-field-write.wf) | RANGE-3 | A field write overlaps the element's whole-element identity (`projections_overlap`), leaving its exit read unknown. |
| [range3-neg-unchanged-box-content-write](../../../tests/conformance/cases/range3-neg-unchanged-box-content-write.wf) | RANGE-3 | The same overlap through a `Box` content step. |
| [range3-neg-unchanged-alias-write](../../../tests/conformance/cases/range3-neg-unchanged-alias-write.wf) | RANGE-3 | A local reference to the element is an element view; a write through it is a placed element write. |
| [range3-neg-unchanged-helper-write](../../../tests/conformance/cases/range3-neg-unchanged-helper-write.wf) | RANGE-3 | A call forgets the storage its row writes through the argument. |
| [range3-pos-unchanged-routed-postcondition](../../../tests/conformance/cases/range3-pos-unchanged-routed-postcondition.wf) | accept | A routed postcondition is owed at the Some return only; the None return writes the framed element. |
| [range3-neg-unchanged-routed-written-element](../../../tests/conformance/cases/range3-neg-unchanged-routed-written-element.wf) | RANGE-3 | The Some return writes the framed element. |
| [range3-neg-unchanged-paged-page-call](../../../tests/conformance/cases/range3-neg-unchanged-paged-page-call.wf) | RANGE-3 | A page argument has no logical element offset, so the writing call forgets all storage. |
| [range3-neg-unchanged-paged-page-element-write](../../../tests/conformance/cases/range3-neg-unchanged-paged-page-element-write.wf) | RANGE-3 | A direct write through a page is not placed at an element tuple and forgets all storage. |

The supplied regression witnesses were checked with the released compiler:

| Witness | Released before behavior | Expectation on the merge base |
| --- | --- | --- |
| `../hf-wit/publish-consume.wf` | RANGE-3 at return, missing `root_bounds`, line 52 | Accept through the base's operation/relocation/take_back work. |
| `../hf-wit/live-inverse-bare.wf` | GRAM-5 at the bare Bool guard, line 25 | Accept through the base's Bool-guard work. |

The released main predates the merge base, so those baseline rejections do
not contradict the base expectation. The two witnesses are not repository
cases, so their behavior on the amended compiler is unverified.

## Identifier-only conformance and program migration

The following existing conformance files only rename the newly reserved binding;
their expected verdicts remain intact. Baselines use --check, so run cases'
runtime exit codes were not executed.

| Existing case | Maintained expectation | Released before behavior |
| --- | --- | --- |
| [inv1-neg-key-set-insert-stale-length](../../../tests/conformance/cases/inv1-neg-key-set-insert-stale-length.wf) | INV-1 | INV-1 |
| [ent5-pos-grow-possibly-aliased-element-length](../../../tests/conformance/cases/ent5-pos-grow-possibly-aliased-element-length.wf) | accept | INV-1 |
| [ent5-neg-grow-nested-old-capacity-range](../../../tests/conformance/cases/ent5-neg-grow-nested-old-capacity-range.wf) | RANGE-3 | RANGE-3 |
| [x-base64-rfc-vectors-run](../../../tests/conformance/cases/x-base64-rfc-vectors-run.wf) | run (exit 0) | accept under --check |
| [ent5-pos-grow-filled-conditional-evidence](../../../tests/conformance/cases/ent5-pos-grow-filled-conditional-evidence.wf) | accept | INV-1 |
| [op6-neg-nearest-round-trip-is-not-domain](../../../tests/conformance/cases/op6-neg-nearest-round-trip-is-not-domain.wf) | OP-6 | OP-6 |
| [ent5-neg-grow-possibly-aliased-element-capacity](../../../tests/conformance/cases/ent5-neg-grow-possibly-aliased-element-capacity.wf) | INV-1 | INV-1 |
| [ent5-pos-indexed-measure-stable-controls](../../../tests/conformance/cases/ent5-pos-indexed-measure-stable-controls.wf) | accept | accept under --check |
| [inv1-neg-key-set-scan-stale-length](../../../tests/conformance/cases/inv1-neg-key-set-scan-stale-length.wf) | INV-1 | INV-1 |
| [ent5-neg-grow-capacity-dependent-range](../../../tests/conformance/cases/ent5-neg-grow-capacity-dependent-range.wf) | RANGE-3 | RANGE-3 |

Program bindings were also renamed in cancellation, memory_statistics,
raw_deflate_dynamic_decode and the containers hash-map-program, owning-behavior
and owning-growth files. Firn's commands/sorted source uses retained_member.
Embedded source fixtures in semantic/tests/exclusive_contracts.rs and
semantic/tests/source_proofs.rs use replacement binding names, with their
expected diagnostic spelling updated. No active examples directory exists on
this base; the scan included docs and the active app sources.

## Unit tests added and what each guards

The added tests run in the gate's Rust test group, which passed on
8e2ba7eb7. The observation column below names the fault each test
distinguishes; no run of a test against the unamended compiler is recorded.

| Test | Observation guarded |
| --- | --- |
| `generic_unchanged_forms_entry_identity_without_projection_expansion` (`semantic/tests/range_facts.rs`) | Symbolic and concrete Block instances in both copy and affine fixtures retain one equality, distinct entry/exit roots and identical written indices; also asserts the concrete copy capability is true or false respectively, detecting erased noninteger frames, equality-field expansion or loss of either ownership class. |
| `unchanged_refusals_name_the_written_relation` (same file) | RANGE-1 points to the affine operand, ordinary relation or guard before unrelated operand checking. |
| `unchanged_suffixes_parse_for_semantic_admission` (`syntax/parser/tests.rs`) | Admitted and diagnostic-only suffix forms reach semantics instead of an earlier grammar error. |
| `whole_identity_is_untyped_and_descendant_writes_cannot_restore_it` (`facts/unchanged.rs`) | An overlapping/restored write leaves identity open; sibling fields and other tuples still prove identity; tokens carry no source integer type. |
| `copied_collections_preserve_values_without_preserving_identity` (same file) | Direct arity-zero and lazy nested copies preserve scalar values while whole identity stays open; catches R1's copy propagation. |
| `imported_frame_defines_requested_projections_under_its_premises` (same file) | Concrete nested field/Box/index projections transfer inside the range and guard; guarded-out and out-of-range reads remain open. |
| `entry_definedness_limits_imported_frames_without_a_new_obligation` (same file) | Existing entry elements transfer; new exit elements with undefined entry reads receive no equality. |
| `frame_definitions_keep_exactly_two_instantiation_rounds` (same file) | A two-round chain proves while one needing a third remains open. |
| `second_round_projection_reads_receive_existing_frame_definitions` (same file) | Step-2 definitions reach a projection introduced by the second round, without another tuple-instantiation round. |
| `lazy_projections_share_instances_but_do_not_bypass_the_tuple_ceiling` (same file) | Many projections of one tuple share one instance; 257 selected tuples report the existing instance ceiling. |
| `lazy_projection_definitions_count_toward_the_atom_ceiling` (same file) | Lazy entry projection atoms count toward the same 4096-atom ceiling. |
| `imported_entry_places_precede_call_effects` (`walk/calls.rs`) | Imported entry/exit versions and descriptor generations differ after ordinary call forgetting; entry stays Initial and exit becomes Fresh. |
| `replacing_a_registered_container_with_its_copy_changes_identity_only` (`walk/tests.rs`) | Actual copy/set alias resolution yields distinct versions, proves scalar equality and refuses whole identity. |

Existing grammar/terminal inventory tests were updated for the computed counts
and shifted positions. The existing entry-relation regression's name/comment
now describe distinct versions. Existing pinned repair expectations were
updated for clarified measure equality wording. No check or expected outcome
was weakened.

## Proposed design-tree changes

- `design/language/checks-and-proofs/range-facts.md`: four new Decisions
  for entry-version reference storage, one-sided any-type unchanged with
  ordinary definedness, opaque identity proof, and lazy caller projection
  definitions. The generic decision now limits omission to other comparisons,
  and the copy decision explicitly describes value projections.
- `design/compiler/range-judgment.md`: four new Decisions for checked entry
  roots/identity markers, immutable entry place views, demand-driven step-2
  definitions and distinct copied versions preserving values only.
- `design/language/checks-and-proofs.md`: the ancestor's range-fact scope
  now includes unchanged postconditions, with detailed choices in its child.

These Decisions are proposed until the owner approves them; the design
approval log has no entry for them. The proof theory, instantiation rounds, structural ceilings,
equality capability and refusal of two-sided element relocation still stand.

## Grammar pins and computation

A temporary data calculation parsed all seven normative EBNF fences and
computed the generator's provenance-bearing FIRST(2), FOLLOW(2), repetition
continuations and two-token SELECT rows. It reproduced every base inventory
before deriving the amended counts on the base before the v0.135 merge:

| Inventory | v0.128 | v0.129 |
| --- | ---: | ---: |
| Productions | 101 | 101 |
| Decisions | 179 | 182 |
| SELECT rows | 8036 | 8450 |
| Diagnostic terminal predicates | 115 | 116 |
| Fixed terminals | 108 | 109 |

The three new decisions are suffix choices in clause_expr, range_guard and
range_relation. Unchanged's first fixed-terminal occurrence is slot 102;
Dot/Caret/Forall and following slots shift accordingly. All duplicate decision,
row and terminal pins were updated. No production was added, so the extractor's
101 count, parser production pin and ENUM_ORDER remain valid. The extractor
on this base is `syntax/grammar/generator/ebnf.rs`; the former
`syntax/ebnf.rs` path does not exist. Native syntax tables are generated into
OUT_DIR; this repository keeps no committed table copy.

The temporary calculation is grammar-data computation, not a build or test
suite. Native generator execution and its tests still require CI.

## Found along the way and review

- **Fixed R1:** copies shared the source container version, allowing whole
  replacement with a copy to prove unchanged. Distinct copied versions and
  value-only copy definitions fix it; a formal negative and two internal
  regressions cover registered and lazy copy paths.
- **Fixed R2:** an initially added own-entry special case reported RANGE-1.
  Removing it preserves MSR-3's existing former refusal. Two formal cases
  cover requirements and postconditions.
- **Fixed diagnostic wording:** a measure repair's prose resembled the new
  forbidden measure-unchanged syntax. It now requests equality of entry and
  exit values; five pinned sentence expectations were updated.
- **Fixed token collisions:** cancellation and other active test/program/Firn
  bindings were renamed; Rust-embedded WF fixtures and diagnostic names follow
  them. Remaining prose and host-language uses of the word are legal.

A separate fresh read-only reviewer inspected the complete base-to-worktree
change and untracked files, excluding the mechanical archive, and re-reviewed
R1's logic and R2's local repair. The reviewer was requested as
`gpt-6.1-sol` at high effort. Scope included A/D/C/T/V and G1/G2/G3/DC1–DC4,
relevant ancestors, the value-equality decision, directly affected copy/store
consumers, normative cases, inventory data and released baseline controls.
No outstanding concrete finding remains within that scope.

| Checklist | Disposition as the reviewer reported it |
| --- | --- |
| A4, D2, G1, G2, G3, DC1, DC2 | Pass by source inspection after fixes. |
| D1, T8, DC3 | Not applicable: constitution/stages unchanged; walk methods extracted rather than a design approach retired. |
| C4 | No remaining concrete finding; general semantic soundness unverified. |
| T1, T7, V3 | Unverified overall: native generation, gates, actual amended verdicts, final PR/approval delivery unavailable or outside this handoff. |
| T4, T5, T6 | Source ownership, shared construction and research boundary pass by inspection; execution unverified. |
| DC4 | Implementation paths present by inspection; execution unverified. |

## Fixture corrections found by CI

CI on `ee730e186835f7870109d55433438b341f4b0df7`, the initial amendment plus a
clippy fix in facts/unchanged.rs, reported conformance adapter Pass=2411,
Fail=1. The failed case and one failed unit share the same source fixture; a
second unit has a separate call-effect error. These were source-fixture
mistakes, not failures of the selected range-frame semantics, and commit
`caa780036` corrected them.

| Reported failure | Why it happened | Correction |
| --- | --- | --- |
| `imported_entry_places_precede_call_effects` in `walk/calls.rs`, EFF-5 | PRE-1 grow declares `writes(cell)`. Supplying xs as cell substitutes `writes(xs^)`; supplying `xs^.inner.cap` by value contributes a read under xs^.inner, compared with that write by EFF-5 clause 2. The statement's arguments overlap. The fixture's existing `writes(xs)` already covers the earlier measure read and the grow call under EFF-1/EFF-2; no additional effect entry is missing. | Read capacity into a local binding before grow, then pass the local value. All entry/exit version and descriptor-generation assertions remain. |
| `generic_unchanged_forms_entry_identity_without_projection_expansion`, OWN-1 | Its include_bytes source is the copy-element append fixture. Block has only a u32 field, so OWN-1 requires bare use; the concrete wrapper wrote `move value`. | Correct that shared source and inspect both the copy and affine fixtures. Retain every prior identity/root/index assertion, require both symbolic and concrete scopes in each fixture, and assert concrete copy capability true for the copy Block and false for the owning Block. |
| `range2-pos-helper-append-frame`, expected accept but rejected OWN-1 | The same concrete wrapper passed a copy Block with `move`. | Pass bare `value` in append_target. The generic T:drop body's move remains valid under OWN-1's generic spelling rule; the affine fixture's Box-owning Block retains its explicit move. |

The correction changed compiler/src/semantic/range_judgment/walk/calls.rs,
compiler/src/semantic/tests/range_facts.rs and
tests/conformance/cases/range2-pos-helper-append-frame.wf. No verdict,
contract, frame assertion, grammar pin, language decision or compiler proof
logic was weakened. Formal cases still cover both copy and affine elements, and
the representation unit checks both ownership classes.

A fresh read-only completion review of `ee730e186`, requested as
`gpt-6.1-sol` at high effort, inspected the four-file fixture and report
diff against the reported CI failures. It found no concrete findings within
scope. A4/D2/C4/T1/T4/T5/T6/V3 and applicable G/DC checks pass by source
inspection; D1/G1/DC3 and new stage/budget checks are not applicable. The
reviewer ran no tests, lint, builds or check suites.

## Validation state

- The gate (`gate.yml`: static, compiler build and tests, conformance adapter,
  runtime, on Linux and macOS) passed on `8e2ba7eb7`, the revision with the
  merge of main at v0.135 and the EFF-5 alias cases. Its conformance adapter
  and Rust tests include every case and unit named above except the
  boundary-case table, which was added afterwards, and the `clause_expr`
  wording of GRAM-6, edited afterwards.
- Amended-compiler results in this report are CI observations; nothing
  compiled or ran on the authoring machine.
- Not established: proof-system soundness beyond the inspected paths and
  the boundary cases' amended verdicts.
- Approval records and merge readiness are the owner's work.

## Merge with main at v0.135

The branch was merged with main at `f0df1d2587533618d7ad8acaac4b0ea1ad9830e8`,
whose active specification is v0.135. The rule changes above were reapplied to
v0.135's text without change of meaning, v0.135 was archived byte for byte and
the active file is v0.136. MSR-3's admission sentence is main's projected
entry former, extended so that in a range postcondition its projection may
select a place RANGE-1 admits, and RANGE-1 lets a range place begin with
`entry(p)` or `entry(p projection)`, the inner steps first, because the merged
compiler admits both spellings there; the branch's edit to MSR-3's denotation
sentence is unchanged.

Main's EFF-5 contract-read footprint already includes range conclusions and
entry-state reads. Its collector now treats an entry-rooted range place as a
read of that parameter and an `unchanged` whole-selection identity as a read of
the complete selected element. The generated inventory at v0.136, read from the
debug build's grammar tables, is 183 decisions, 8548 SELECT rows, 116
diagnostic terminal predicates and 109 fixed terminals; the three rows beyond
the two sides' sum are the `unchanged` suffix following the entry former's
`psuffix*` repeat.
