# Range helper frames implementation

## Delivery and selection ground

The amendment on `claude/range-helper-frames`, initially based on
`cef63d637d3f07377163c9e4c70cb70c3e229cea`, implements the owner-selected
[entry-version and one-sided unchanged frames](DESIGN.md). Owner-supplied CI
results for `ee730e186835f7870109d55433438b341f4b0df7` exposed the fixture
failures recorded below. The current fixture corrections are uncommitted and
await CI; specified expectations are distinguished from observed results.

On 2026-10-11 the owner selected decision B: the implicit entry read follows
RANGE-1's existing defined-read premise. An exit tuple absent at entry has a
vacuous frame instance and gives the caller no definition. The note's meaning
and acceptance list were corrected accordingly. No separate definedness
obligation was introduced. The note's claim that cancellation held the sole
identifier collision was also corrected after the active corpus scan.

No commits, pushes, GitHub polling, status-board changes or approval-log edits
were performed. `design/skill` was not changed. Work stops for the requested CI
handoff; no semantic decision or known implementation contract gap remains open.

## Specification changes, rule by rule

| Rule | Before | After and selection ground |
| --- | --- | --- |
| GRAM-5 | Range conclusions were two affine operands around a comparison; `unchanged` was an identifier. | Adds the factored `unchanged` suffix. RANGE-1 admits only a single element place in a range postcondition. Guard and ordinary-clause syntax also recognize the suffix so their semantic refusals cite RANGE-1. FORM-3's existing derived reservation makes the word reserved. Selected by the original helper-frame note. |
| GRAM-6 | The explanatory clause shape mentioned only a bare affine expression or two around an operator. | Includes the parsed suffix and points to RANGE-1 for admission. A grammar-correspondence wording change, with no additional admitted context. |
| MSR-3 | Entry formers denoted ordinary projected entry measures or fragment-integer places. | Points to RANGE-1 for range reads. Existing former admission and refusals remain: only a reference parameter whose row writes it, in ensures; own and nonwriting parameters still reject MSR-3. No new decision beyond the selected range use. |
| RANGE-1 | Range terms read no entry image; no unchanged conclusion existed. | Admits entry-rooted postcondition places and `P unchanged` for an element projection of a written reference parameter, without a final measure or equality capability. Generic instances retain this conclusion for any T. Requirements, guards, loop/type invariants, ordinary relations, whole windows, measures and nonwriting roots reject unchanged with RANGE-1 and DIAG-1 repair. Plain consumed own storage retains RANGE-1. The implicit entry read participates in ordinary read-definedness, as owner decision B selected. |
| RANGE-2 | Versions described ordinary projection values; calls imported exit facts after forgetting writes. | Adds abstract whole-element reads whose identities are unknown after overlapping writes. Calls record entry storage views before effects. Imported unchanged instances define requested exit projection reads by recorded entry reads under the fact's bounds, guards and definedness. Copy wording now says value projections: copying retains values, while whole-container replacement cannot recover old identity. The latter follows the note's explicit refusal of written/restored storage and fixes review R1. |
| RANGE-3 | Ordinary range comparisons were owed and proved through the fixed derivation. | An unchanged obligation is equality of exit and entry whole-element reads; imported frame definitions join step 2. Automatic instantiation remains exactly two rounds, with the existing shared 256-instance and 4096-atom ceilings. Selected by the note, without a new theory or search. |

The active file is v0.129. Its v0.128 archive is byte-identical to the base's
active specification: both SHA-256 values are
`b492cec7a43b20da4e45791e218bb129bf2bcc4dc68a41ebe464a67a222596ca`.
No other archive was changed. Specification/design approval logs were left for
the owner, as requested.

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
is now admitted. All expectation changes follow v0.129; identifier-only edits
retain their prior verdicts.

Baseline command: the supplied released compiler
`../rel4173/whitefootc --check FILE`, from main
`4173ec89e9fe187108fc40679427cd39bb533293`.
The released before-behavior below is from the initial handoff. The owner
subsequently supplied amended-compiler CI results at ee730e186; see the
fixture-correction section for that evidence and its remaining failure.

| Case | Expected v0.129 verdict | Released before behavior |
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

The supplied regression witnesses were checked with the released compiler:

| Witness | Released before behavior | Amended/base expectation |
| --- | --- | --- |
| `../hf-wit/publish-consume.wf` | RANGE-3 at return, missing `root_bounds`, line 52 | Accept through the inherited operation/relocation/take_back work; unverified here. |
| `../hf-wit/live-inverse-bare.wf` | GRAM-5 at the bare Bool guard, line 25 | Accept through the inherited Bool-guard work; unverified here. |

The released main predates the supplied merged base, so those baseline
rejections do not contradict the base expectation. CI must establish their
post-amendment regression results.

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

The added tests were unrun at the initial handoff. Owner CI later exposed two
fixture failures at ee730e186; their corrections remain unrun. The paired
expected outcomes below describe the faults each test distinguishes; no local
red/green result is claimed.

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

These are proposed live-tree Decisions as requested; no design approval log
was written. The proof theory, instantiation rounds, structural ceilings,
equality capability and refusal of two-sided element relocation still stand.

## Grammar pins and computation

A temporary data calculation parsed all seven normative EBNF fences and
computed the generator's provenance-bearing FIRST(2), FOLLOW(2), repetition
continuations and two-token SELECT rows. It reproduced every base inventory
before deriving the amended counts:

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

| Checklist | Disposition |
| --- | --- |
| A4, D2, G1, G2, G3, DC1, DC2 | Pass by source inspection after fixes. |
| D1, T8, DC3 | Not applicable: constitution/stages unchanged; walk methods extracted rather than a design approach retired. |
| C4 | No remaining concrete finding; general semantic soundness unverified. |
| T1, T7, V3 | Unverified overall: native generation, gates, actual amended verdicts, final PR/approval delivery unavailable or outside this handoff. |
| T4, T5, T6 | Source ownership, shared construction and research boundary pass by inspection; execution unverified. |
| DC4 | Implementation paths present by inspection; execution unverified. |

## CI fixture corrections after ee730e186

The owner ran CI on `ee730e186835f7870109d55433438b341f4b0df7`, containing the
initial amendment plus the owner's clippy fix in facts/unchanged.rs. The
reported conformance adapter result was Pass=2411, Fail=1. The failed case and
one failed unit share the same source fixture; a second unit has a separate
call-effect error. These are source-fixture mistakes, not failures of the
selected range-frame semantics.

| Reported failure | Why it happened | Correction |
| --- | --- | --- |
| `imported_entry_places_precede_call_effects` in `walk/calls.rs`, EFF-5 | PRE-1 grow declares `writes(cell)`. Supplying xs as cell substitutes `writes(xs^)`; supplying `xs^.inner.cap` by value contributes a read under xs^.inner, compared with that write by EFF-5 clause 2. The statement's arguments overlap. The fixture's existing `writes(xs)` already covers the earlier measure read and the grow call under EFF-1/EFF-2; no additional effect entry is missing. | Read capacity into a local binding before grow, then pass the local value. All entry/exit version and descriptor-generation assertions remain. |
| `generic_unchanged_forms_entry_identity_without_projection_expansion`, OWN-1 | Its include_bytes source is the copy-element append fixture. Block has only a u32 field, so OWN-1 requires bare use; the concrete wrapper wrote `move value`. | Correct that shared source and inspect both the copy and affine fixtures. Retain every prior identity/root/index assertion, require both symbolic and concrete scopes in each fixture, and assert concrete copy capability true for the copy Block and false for the owning Block. |
| `range2-pos-helper-append-frame`, expected accept but rejected OWN-1 | The same concrete wrapper passed a copy Block with `move`. | Pass bare `value` in append_target. The generic T:drop body's move remains valid under OWN-1's generic spelling rule; the affine fixture's Box-owning Block retains its explicit move. |

Files changed in this follow-up: compiler/src/semantic/range_judgment/walk/calls.rs,
compiler/src/semantic/tests/range_facts.rs and
tests/conformance/cases/range2-pos-helper-append-frame.wf. No verdict, contract,
frame assertion, grammar pin, language decision or compiler proof logic was
weakened. Existing formal cases still cover both copy and affine elements, and
the representation unit now explicitly checks both ownership classes. No new
unit or case was added.

The spec-archives static failure is expected for this stacked branch, as the
owner reported. The v0.128 archive is left for the owner's correction after
[Whitefoot#366 — spec v0.128 base](https://github.com/Ming-Research/Whitefoot/pull/366)
merges. The owner's clippy fix is retained. No builds, cargo,
make, clippy or tests ran locally for this follow-up; direct rustfmt was the
only authoring command. The corrected fixtures await the owner's next CI run.

A fresh read-only completion review, requested as gpt-6.1-sol at high effort,
inspected ee730e186 through the complete four-file fixture/report diff, using
the reported CI failures as validation evidence. It found no concrete findings
within scope. A4/D2/C4/T1/T4/T5/T6/V3 and applicable G/DC checks pass by source
inspection; D1/G1/DC3 and new stage/budget checks are not applicable. Corrected
execution and new-revision gate success remain unverified. The reviewer ran no
tests, lint, builds or check suites.

## Remaining uncertainty and CI handoff

There is no known unresolved contract gap in the selected note. Owner CI
compiled the amended compiler and ran the conformance adapter, reporting
2,411 passes and one failure at ee730e186. Two unit fixtures also failed.
The corrections above need a new CI run, including the affected units and
conformance case. Full gate success, the supplied regression witnesses and
merge readiness are not established by the provided results. The review did not
rerun a suite or establish proof-system soundness beyond the inspected paths. Design-lint metrics were
not generated because checks are reserved for CI. Approval records and merge
readiness remain the owner's work, as requested.

Only authorized released-compiler before checks, read-only source/data/git
inspection, grammar-data calculation and direct rustfmt authoring were performed
on the Mac. No builds, cargo, make, clippy, test suites or performance
measurements ran locally. Amended-compiler results in this report are the
owner-supplied CI observations, not local execution.
