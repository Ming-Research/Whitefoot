# Declaration-site provenance — exact v0.32-candidate rule edits (DRAFT, not a spec change)

Status: DRAFT delta text for batch 0071 (W2 item #50, "declaration-site
rejection of ambiguous-provenance borrow returns"), prepared for the lead's
single-writer integration into the one v0.32 candidate. Nothing here edits
`spec/kernel-spec.md`; every byte below awaits the owner's exact-byte
approval. Line references are to the active v0.31 file
(SHA-256 `ea4b8ad4a56fbf43f3c98b91fc667da0b693c75b81807250a36454e03a197f1c`).

Owner-settled law (2026-08-18): *at most one parameter may share the result
borrow's region and kind — a declaration whose result no caller can use is
itself the error.*

Implementation status: implemented in the compiler behind the one-line
integration switch `DECLARATION_PROVENANCE` in
`compiler/src/semantic/check.rs` (default `false` = v0.31 semantics
byte-identical; activation flips it to `true`). The test-only entry
`check_semantics_declaration_provenance` exercises the candidate; paired
default-checker tests pin the v0.31 dispositions of the same sources.

Builds on `research/investigations/reborrow-extension/SPEC-DELTA.md` (the
v0.31 singleton-provenance machinery, superseded in place by activation) and
on the recorded v0.17 rejection of a possible-provenance *set* over borrows
([mcts_mem/whitefoot/ownership/no-reborrow.md](https://github.com/mbbill/Whitefoot/blob/0f22b026b062c36ef2f8ce9c725ee4307e1fda64/mcts_mem/whitefoot/ownership/no-reborrow.md)). This delta does not
re-derive either; it is the declaration-site surface of the same singleton
invariant.

## 1. What changes (summary)

v0.31 already judges a borrow-mode result's provenance from the callable
boundary alone. It puts the *rejection* at the caller's binding: a `let`
that binds a borrow result whose callee signature determines no candidate
is a hard error citing OWN-6.

v0.32 moves that rejection to the declaration:

- **The judgment is unchanged.** For a signature whose written result mode
  is `&K 'b`, a parameter is a provenance candidate iff it is written as a
  borrow of the same kind K in the same formal region `'b`.
- **Exactly one candidate** — the v0.31 debtor. Unchanged in every respect:
  binding, resolved place, suspension, chains.
- **Zero candidates** — legal. OWN-10 forbids rooting a `'b` borrow in
  callee-local storage and distinct formal regions are incomparable
  [OWN-3], so the only remaining source is named-`const` storage, which is
  permanently read-only for program lifetime [CONST-2]. Provenance is
  unique by elimination and needs no claim.
- **Two or more candidates** — the *declaration* is a hard error at its
  complete `rtype`, whether or not it is ever called.

The ground is GRAM-9. Every call argument is an `atom` and a computed value
is forwarded only by binding it with a preceding `let`; a `call` in an atom
position does not derive. So a borrow-mode call result has exactly one
legal destination — a `let` — and a result no caller can bind is unusable
by construction. Under v0.31 such a declaration is accepted, its calls are
writable as discarded statements, and its result is inert: the boundary
promises a value the language provides no way to consume. The law closes
that: **bindable iff usable**, and a declaration whose result no caller can
use is itself the error.

## 2. Two consequences worth stating before the edits

**The accepted set does not move; the diagnostic does.** Every program the
declaration rule rejects was already unable to use that result under v0.31.
What changes is *where* the writer learns it and *when*: at the boundary
they wrote, before any caller exists, instead of at a caller's binding much
later. In the other direction, the zero-candidate boundary — which v0.31
also accepted as a declaration and rejected only at a caller's binding —
becomes legal at both sites; no program newly compiles, because the
checker has no const-rooted holder, so what a binding meets there is an
explicit capability stop instead of an invalid-source verdict. That is the
right classification either way: the language admits the form, the compiler
has not built it.

**The judgment covers every undetermined shape, not only the two-same-kind
pair.** Kind filtering is asymmetric: a `uniq` result can derive only from
`uniq` sources, but a `shared` result CAN derive from a `uniq` parameter (a
shared child of a uniq parent laundered through a nested borrow-returning
call). So a same-region parameter of the *other* kind leaves two possible
roots exactly as a second same-kind parameter does, and a parameter whose
written type names the result region (`slice<'b, _>`) does the same through
its storage. All three are the same defect — the boundary does not
determine the source — and all three carry the same restructuring. Naming
only the same-kind pair would leave the other two shapes as accepted
declarations whose results still no caller can bind, which is the state
this rule exists to remove. **This is the one place where the delta is
wider than the owner's literal sentence; the lead should confirm it before
the candidate goes to the owner.**

## 3. Exact rule edits

### FN-1 — new declaration-site judgment (insert after line 954)

Insert a new paragraph immediately after the existing borrowed-slice-result
paragraph (lines 952–954, ending `This rejection does not change any other
returned-borrow judgment.`) and before the `fn_sig` sentence at line 956:

> A function whose written result mode is `&'b` or `&uniq 'b` determines the
> result's provenance from its written parameters alone: a parameter is a
> provenance candidate iff its written mode is a borrow of the result's kind
> in the result's formal region `'b` [OWN-6].
> Exactly one candidate is the result's debtor, and zero candidates is
> legal — OWN-10 admits no `'b`-region borrow rooted in callee-local
> storage, so the only remaining source is named `const` storage, whose
> immutable program-lifetime extent needs no claim [CONST-2].
> Two or more candidates, a same-region parameter of the other borrow kind,
> or any parameter whose written type names `'b` leaves the source
> undetermined and is a hard error citing FN-1 at the complete `rtype`, with
> `SourceCoordinate` equal to that production's complete checked half-open
> source extent and the restructuring `give the source parameter its own
> region so exactly one parameter shares the result's region and kind, or
> return the decision as a value and let the caller borrow from the source
> it names`.
> The declaration is the error and no call is required to reach it: [GRAM-9]
> admits a computed value only through a preceding `let`, so a result no
> caller can bind is unusable by construction.

### FN-1 — extend the `fn_sig` sentence (line 956)

Replace `The signature-formation parts of these two slice-result judgments
apply equally to a top-level `fn_decl` and a contract-member `fn_sig`` with:

> The signature-formation parts of these two slice-result judgments and of
> the borrow-result provenance judgment apply equally to a top-level
> `fn_decl` and a contract-member `fn_sig`

and append to the same sentence, after `a borrow-mode direct-slice member is
rejected at that member's complete `rtype``:

> , and a borrow-result member whose source its own parameters leave
> undetermined is rejected there too

### OWN-6 — delete the caller-side rejection (line 570)

Delete line 570 in full:

> Binding a borrow-mode user-call result whose callee signature does not
> determine a candidate is a hard error citing OWN-6 with the restructuring
> `give the callee exactly one parameter written as a borrow of the result's
> mode and region and no other parameter naming that region, or bind the
> borrow from a direct borrow expression`.

It is unreachable once FN-1 rejects the boundary: no accepted declaration
reaches a call site without a determined source.

### OWN-6 — restate the holder condition (lines 568–569)

Replace line 568:

> A `let` whose ordinary right-hand side is a user call with borrow-mode
> result is a borrow holder exactly when the callee signature determines one
> provenance-candidate parameter: the one parameter written as a borrow of
> the result's kind in the result's formal region, with no other parameter
> naming that formal region in its mode or written type and with a
> region-free result type.

with:

> A `let` whose ordinary right-hand side is a user call with borrow-mode
> result is a borrow holder rooted at the callee's provenance candidate
> [FN-1], and every accepted callee has one or has none.

Line 569 (`resolved(result holder) = …`) is unchanged. Line 571 (`Nothing
here narrows FN-1: the caller still judges the call by the signature
alone.`) is unchanged and now reads as the pointer it always was.

### OWN-6 — the zero-candidate holder (append after line 571)

> A borrow-mode call result with no candidate is rooted in named `const`
> storage [FN-1, CONST-2], which no accepted write or unique borrow reaches
> [OWN-5, OWN-7]; its holder claims no caller place and conflicts with
> nothing.

### META-5 accounting

Rules edited: FN-1, OWN-6. Tokens ±0, spellings ±0. Rules ±0. Exceptions:
+1 (FN-1's boundary provenance rejection), −1 (OWN-6's binding-side
ambiguity rejection, now unreachable). SELECTION GROUND:
minimality-selected — the rejected set is unchanged in substance and the
move is a diagnostic-site and boundary-honesty choice, grounded in GRAM-9's
flat form rather than in new measurement.

## 4. Implementation

One new default-off switch, `DECLARATION_PROVENANCE` in
`compiler/src/semantic/check.rs`, threaded to a `Checker` field exactly as
`REBORROW_EXTENSION_ACTIVE` is. Activation flips it in the same change as
the candidate bytes.

- `borrow_result_provenance()` in `compiler/src/semantic/check.rs` is the
  single judgment, returning `Candidate(index)`, `ConstStorage`,
  `Ambiguous`, or `Unjudgeable` (a slice or unsubstituted-generic result,
  which FN-1's slice paragraph already rejects at the boundary). It is the
  v0.31 predicate refactored to name its dispositions; the call-site
  `result_borrow_candidate()` now reads it and is behaviorally identical
  with the switch off.
- `Checker::reject_ambiguous_result_provenance()` issues the FN-1 rejection
  at the complete `rtype`, and is called from both signature-formation
  sites: `build_function_signature()` in
  `compiler/src/semantic/check/generics.rs` (top-level `fn_decl`, after the
  existing STOR-4 arena, FN-1 slice, and borrowable-type judgments) and
  `contract_member()` in `compiler/src/semantic/check/contracts.rs`
  (`fn_sig`). Every non-generic declaration is signature-formed whether or
  not it is called, so the rejection lands on an uncalled declaration.
- The inert path is removed under the switch: the OWN-6
  `AmbiguousResultBorrow` binding rejection in
  `compiler/src/semantic/check/control.rs` is structurally unreachable when
  the switch is on. What remains reachable there is the const-storage
  result, whose claim would need a const-rooted holder the checker does not
  represent; it becomes an explicit `RegionsAndBorrows` capability stop,
  never an invalid-source verdict.
- New diagnostic kind `SemanticIssueKind::AmbiguousResultProvenance` in
  `compiler/src/semantic/mod.rs`, carrying the exact restructuring.

Recorded compiler capability gap (not a rule change): the zero-candidate
boundary is legal but has no writable body today — the checker has no
const-rooted borrow representation, so `return &'b K;` is an explicit
`RegionsAndBorrows` stop. The rule is stated for the language; the
capability is not needed by any current experiment and is not built here.

Also recorded: an *uncalled generic* function is never signature-formed
today (only templates with no generic parameters are instantiated eagerly),
so its boundary — like its body — is unjudged until a call instantiates it.
This delta does not change that traversal; it inherits it.

## 5. Evidence

`compiler/src/semantic/tests/borrows.rs`:

- `declaration_provenance_rejects_two_same_region_sources_at_the_declaration`
  — `pick['r](a: &uniq 'r, b: &uniq 'r) -> &uniq 'r`, never called, rejects
  at FN-1 with the exact restructuring.
- `declaration_provenance_rejects_every_undetermined_source_shape` — the
  other-kind and region-naming-type shapes, same rule and text.
- `declaration_provenance_admits_distinct_region_sources_and_keeps_them_usable`
  — the named fix: `pick['r, 's]` accepted, its result bound, written
  through, and read through.
- `declaration_provenance_admits_the_zero_candidate_boundary` — no
  candidate is legal; the body meets the capability stop, not a rejection.
- `declaration_provenance_makes_the_binding_side_ambiguity_unreachable` —
  the ambiguous *call* program now rejects at FN-1, paired with the pinned
  v0.31 OWN-6 disposition of the identical source.
- `declaration_provenance_keeps_the_established_boundary_judgment_order` —
  the borrowed-slice result still cites FN-1's slice rejection, and a
  callee-local-rooted zero-candidate body still cites OWN-10.
- `the_shipped_checker_keeps_the_v031_declaration_dispositions` — with the
  switch off, every source above keeps its exact v0.31 disposition.

## 6. PROPOSED conformance cases (no `tests/conformance/` edit here)

Protected evidence; nothing below is written by this batch's executors. The
lead carries these into the candidate's marked protected commit with the
exact before/after audit and the owner's approval.

**Modified — 1 case.** `own6-neg-callresult-no-provenance-candidate`
(`tests/conformance/cases/own6-neg-callresult-no-provenance-candidate.wf`,
`manifest.jsonl` line 493). Its program is exactly the two-same-kind pair
plus a caller binding. Under the candidate its verdict moves from
`{"rules": ["OWN-6"], "expect": {"kind": "reject", "rule": "OWN-6"}}` to
`{"rules": ["FN-1"], "expect": {"kind": "reject", "rule": "FN-1"}}`, and
its `doc` prose moves from the binding to the boundary. Renaming the case
to `fn1-neg-callresult-undetermined-provenance` is the honest identity but
costs a protected rename; the lead decides.

**Proposed new cases — 4.**

1. `fn1-neg-result-provenance-two-same-kind` — the two-same-kind pair with
   **no caller at all**. This is the case the existing one cannot carry: it
   proves the declaration is the error. Expect reject / FN-1.
2. `fn1-neg-result-provenance-other-kind` — `fn either['r](a: &uniq 'r i32,
   b: &'r i32) -> &'r i32`, no caller. Expect reject / FN-1.
3. `fn1-pos-result-provenance-distinct-regions` — `pick['r, 's]` accepted,
   result bound and written through, running to the expected output.
   Expect accept / run.
4. `fn1-pos-result-provenance-zero-candidate` — a borrow result with no
   borrow parameter, as a `fn_sig` contract member so no body is required.
   Expect accept (signature formation only). If the contract surface makes
   this awkward, drop it: the rule's zero-candidate arm is already carried
   by the unit test and has no writable body today.

No existing accepted case is affected: the repository's only other
borrow-returning declarations (`own4-pos-return-caller-borrow`,
`own6-pos-callresult-borrow-chain`, `x-borrow-return-uniq-local-region`,
`type7-neg-match-reference-call`, `own10-neg-dangle-caller`,
`reject-own10-dangle`, `fn1-neg-borrowed-slice-result`) each have zero or
one candidate, and the two OWN-10 dangle cases are zero-candidate
boundaries whose OWN-10 body rejection is unchanged (pinned by
`declaration_provenance_keeps_the_established_boundary_judgment_order`).

Migration inventory, complete and reproducible with
`grep -rn "^fn .*-> &" --include='*.wf' .` and the same pattern with a
leading-whitespace anchor for `fn_sig` members. Thirteen top-level
borrow-returning declarations exist in the repository (one under
`archive/`, which no active source, build, test, or tool reads) and one
contract member, `fn1-neg-contract-borrowed-slice-result`, whose slice
result is rejected by FN-1's existing member judgment before the new one
runs. Exactly one declaration in the repository is ambiguous under this
rule: `pick` in `own6-neg-callresult-no-provenance-candidate`. No wfgrep,
raw-DEFLATE, codegen, or `research/` program is affected.

## Appendix A — mcts_mem entry draft (apply at activation, not before)

[mcts_mem/whitefoot/ownership/no-reborrow.md](https://github.com/mbbill/Whitefoot/blob/0f22b026b062c36ef2f8ce9c725ee4307e1fda64/mcts_mem/whitefoot/ownership/no-reborrow.md). No node moves to `.alt/`:
v0.31's singleton provenance is kept and relocated, not replaced.

Edit the v0.31 decision item (Item 4) to end with the declaration-site
form, and add one item after it:

> - v0.32 moves the provenance rejection to the declaration. Two laws:
>   **bindable iff usable** — a call result the language admits binding for
>   is a usable holder, and there is no inert bindable-but-unusable state;
>   and **a declaration whose result no caller can use is itself the
>   error** — GRAM-9 admits a computed value only through a preceding
>   `let`, so a borrow result with no signature-determined source has no
>   legal destination and the boundary, not the caller's binding, is
>   rejected [FN-1]. At most one parameter may share the result borrow's
>   region and kind; zero is legal and means named-const storage by
>   elimination.

Append one dated fact:

> - 2026-08-18 rationale: the rejected set does not grow — every boundary
>   the declaration rule rejects already had an unusable result under v0.31
>   — so the change buys diagnostic honesty and boundary legibility rather
>   than safety. It also removes a state the checker had to represent: the
>   v0.31 caller-side rejection existed only to catch a declaration the
>   language should never have accepted. The judgment is deliberately wider
>   than "two same-kind parameters": a same-region parameter of the other
>   kind and a parameter whose type names the result region leave the
>   source equally undetermined, because a `shared` result can derive from
>   a `uniq` source through a nested borrow-returning call. (code)

## Appendix B — `docs/patterns.md` writer idiom draft (apply at activation)

New entry after P12, in that file's established four-part form. The file
carries no code fences, so the worked example is written as prose over
named signatures, exactly as P10 does.

> ## P13. Return the decision, not the access
>
> Problem: a helper must choose between two borrowed sources and hand the
> chosen one back, but the callable boundary cannot say which one it
> chose. `fn pick['r](a: &uniq 'r Node, b: &uniq 'r Node) -> &uniq 'r Node`
> is rejected at its own `rtype` [FN-1]: two parameters share the result's
> region and kind, so no caller can root the returned claim, and a result
> no caller can bind is the declaration's error, not the caller's.
>
> Pattern: decide which fix applies by asking *why* there are two sources.
> If the sources are structurally distinct — a node and its scratch buffer,
> a subject and its dictionary — give the non-source its own formal region:
> `fn pick['r, 's](a: &uniq 'r Node, b: &uniq 's Node) -> &uniq 'r Node` is
> accepted, and its result is an ordinary holder over `a`'s storage that
> the caller binds, writes through, and reborrows from. If instead the
> choice is data-dependent, no signature can name the source, and the
> access belongs to the caller: return the *decision* as an owned value —
> a two-variant enum, or an index into a pool [P2] — and let the caller
> re-borrow from the place the decision names.
>
> The worked shape for the data-dependent case is three parts. The callee
> `fn heavier(a: &'r Node, b: &'r Node) -> own Side reads('r)` reads both
> weights through its shared borrows and returns `Left()` or `Right()`; it
> takes shared borrows, so both sources may name one region and nothing is
> ambiguous — a returned owned value has no provenance. The caller binds
> `let side = heavier(a: &'a left, b: &'a right);`, and then `match side`
> takes the exclusive borrow it actually wants inside the taken arm, from
> `left` or from `right` by name. The result is a longer function than the
> rejected one-liner and that is the whole trade: the borrow is created
> where its source is a written place, so the checker sees one root per
> holder, and the caller keeps both sources usable until it commits.
>
> Fast because: the decision is a scalar. The read pass takes shared
> borrows that constrain nothing, and the write pass takes exactly one
> exclusive borrow at the point of use, so the one-usable-mutable-path
> fact [OWN-9] is preserved with no widened claim — where the rejected
> form, had it been admitted, would have had to claim the whole of both
> sources conservatively.
>
> Replaces: returning a reference selected by a body-internal branch
> (Rust's `if c { a } else { b }` returning `&mut T`), which forces the
> caller's claim to cover both sources or forces the compiler to derive a
> body-dependent provenance summary the boundary does not state.
>
> Also answers the sibling-chain limit: when two chains would reach
> disjoint fields of one root, return the field selector, not the field
> borrow.

The named signatures above are illustrative writer prose, not a checked
fixture. `heavier` and the `match`-arm re-borrow shape should be compiled
through the candidate before the entry lands, and the spelling adjusted to
whatever the candidate admits.
