# Index surface settlement — v0.22 micro-batch candidate

Status: CANDIDATE, DRAFT (2026-08-07; owner ruling "approved" (translated
from Chinese) of 2026-08-07 applied — the adversarial review's v0.22 section at
`research/investigations/obligation-discharge/CANDIDATE-REVIEW.md`
(186bde5) is applied in full: must-fix V1/O3, editorial V4–V6, the O1–O7
recommendations, and the owner-approved residue strike R1).
Non-authoritative. This document is
the complete v0.22 delta against the exact text of the active
`spec/kernel-spec-v0.21.md` (installed 7512f46; roadmap authority 1e23d03).
It authorizes nothing: activation runs the `docs/WORKFLOW.md` loop, and the
grammar delta requires the native grammar path extended first (§3). The
items are owner-ruled 2026-08-07: (1) remove `index_get`, (2) respell
`index` as the subscript place suffix `p[i]`, (3) delete the subscript's
element type argument — items 2 and 3 are the
`research/investigations/spelling-relief/SWEEP.md` verdicts A2 and C2
pulled forward, item 1 is the same ruling's option-a removal — plus (4)
the review's residue strike R1: fact source S8 (the midpoint family)
leaves the fragment, PARKED with a monotone re-add path. The ruled list
and the one discovered knock-on are §7.

Rationale of record for item 1: `index_get` is a residue of the
pre-collapse design. The kernel's total access is the washing branch —
`match ilt<u64>(i, n) { … }` — whose facts discharge the subscript inside
the guarded arm, so the guarded-total road already exists and is the
reference shape [ENT-3, CLM-1]. Rust needs `get` because a branch there
does not license unchecked `[]`; under caller-side discharge a branch here
does exactly that, making a second total access form redundant (one
spelling per meaning, FORM-1). Removal widens acceptance: `index_get`
leaves the derived reserved sets and becomes an ordinary IDENT.

## 1. Proposed version-header paragraph

> Status: REVIEW CANDIDATE v0.22 (2026-08-07; index surface settlement:
> subscript respelling, element-type derivation, index_get removal).
> Respells the sole indexed-place form from the prefix
> `index "<" type ">" "(" place "," atom ")"` to the place suffix
> `"[" atom "]"`, composing after any `pbase` and `psuffix` chain, so
> nested access spells `a[i][j]` and `deref(h)[i]` without wrapper
> nesting; deletes the element type argument — the subscripted place's
> selected type is derived from the base place's already-stated indexable
> type by the same declared-type selection that types a field suffix,
> never from expected type or inference [OP-4, TYPE-5]; and removes the
> `index_get` operation, whose total-access role the washing branch
> already carries under caller-side discharge. The bounds obligation, its
> `own u64` offset typing, discharge, residual rendering, and every
> [ENT] judgment are unchanged in substance; their anchors move to the
> subscript's `psuffix` node — a real node with one extent per subscript,
> well-defined for chains where the flat `place` production gives `a[i][j]`
> one place node. Additionally strikes fact source S8 (the midpoint
> family) from [ENT-3] as an owner-approved residue re-decision: zero
> corpus sites write the shape, the label is retired unreused, and
> [ENT-1]'s removal prohibition is scoped to checker strengthening —
> cross-version compatibility promises are deferred wholesale (owner
> ruling 2026-08-07), and version amendments are governed by the ordinary
> workflow — leaving restoration a later version's free monotone addition
> the day a program writes the shape. Specification delta: numbered rules
> +0/-0; twenty existing rules modified at twenty-five verbatim-anchored
> modification sites (a site is one contiguous verbatim-anchored
> replacement): FORM-2 (two sites: the right-attachment set gains `[`;
> header and example
> bytes follow), GRAM-5 (`pbase` loses the index alternative; `psuffix`
> gains the subscript alternative), GRAM-6 (subscript is the place
> suffix, its sole home), GRAM-9 (subscript offset is an atom), SET-1
> (two sites: spelling-neutral base-before-offset order; carried offset
> values), CONST-2 (read via subscript/`len`), OWN-6 (child-reborrow
> metanotation restated in words — `deref(h)[suffix]` would read as live
> subscript syntax), OWN-7 (subscripted-place
> overlap wording), OWN-14 (returned-reborrow metanotation restated
> likewise), OP-1 (two sites: the `index_get` row deleted, the
> non-consuming place-operand sentence's base list), OP-4 (rewritten to
> the subscript spelling with the element-type derivation rule, both
> errors anchored at the subscript's `psuffix` node, and
> without the `index_get` restatement), FN-8 (subscripting rejected in
> requires), EFF-2 (accepted target subscript), DIAG-1 (subscript offset
> in the attribution row), DIAG-2 (source subscript disposition; the
> retained derivation named per `psuffix` node), ENT-1 (the monotonicity
> law scoped to checker strengthening), ENT-2
> (no subscript segment in tracked places), ENT-3 (two sites: S9
> respelled; S8 struck with its label retired), ENT-6
> (two sites: obligation attachment at the subscript's `psuffix` node;
> the rebinding sentence), SYS-2 (the signature block's region-parameter
> headers respell to the new canonical attachment). Tokens +0/-1
> (`index` leaves the exact fixed
> lowercase grammar atoms and becomes IDENT-eligible; `[` and `]` are
> already punctuation tokens); terminal spellings +0/-1; grammar
> productions +0/-0 (two productions modified); operation table +0/-1
> rows (`index_get`), shrinking the derived `DotlessOperationNames` and
> `ReservedLowerNames` sets by one member each; exception clauses +0/-0;
> source constructs +0/-0 (one construct respelled, one operation
> removed); sections +0. The accepted-program set widens by exactly two
> spelling classes — `index` and `index_get` as ordinary IDENTs — and
> narrows by exactly one measured-empty class: a program whose discharge
> relied on the struck S8 midpoint facts (zero sites in either corpus,
> measured 2026-08-07) — an ordinary version amendment under the
> workflow, outside [ENT-1]'s checker-strengthening law. Every program
> spelling the old
> forms is
> respelled mechanically by canonical reprint [FORM-1], including the
> attachment ripple the FORM-2 modification fixes. Selection ground:
> evidence-selected under the four-test spelling rule —
> `research/investigations/spelling-relief/SWEEP.md` rows A2 (the element
> type is uniquely reconstructible from the place at 100% of sites) and
> C2 (the sole place form respelled; T3 uniqueness trivial), with the
> named delta over Rust: `get` exists there because a branch does not
> license unchecked `[]`, while caller-side discharge makes the washing
> branch the kernel's total access — plus the owner rulings of
> 2026-08-07. These bytes are non-authoritative until the grammar check,
> derived-material review, full-document hash, exact owner approval, and
> active-target installation complete.

Register maintenance in the same header area: the R3-PROVISIONAL register
entry "deref/index prefix places (GRAM-5)" reduces to "deref prefix places
(GRAM-5)" — the index half is settled by this batch's swept, ruled
respelling (open question O2 records the classification choice).

## 2. Grammar delta

[GRAM-5]'s place block becomes (one alternative deleted from `pbase`, one
added to `psuffix`; every other line byte-identical):

```
place          := pbase psuffix*
pbase          := IDENT | "deref" "(" place ")"
psuffix        := "." IDENT | "[" atom "]"
```

`[` and `]` are already exact punctuation tokens [GRAM-1] (region_params,
cvalue arrays), so raw lexical formation is untouched. `index` ceases to be
an exact fixed grammar atom, so [FORM-3]'s derived IDENT exclusion releases
it with no FORM-3 text change. Strong-LL(2) survives: the `psuffix*`
repetition decision selects the field arm on `.`, the subscript arm on `[`,
and exits otherwise; no production in the grammar places a `[` immediately
after a complete `place` (region_params follow a declaring `fn` IDENT or a
generics `>`, never a place; cvalue arrays follow `=`, `,`, or `[`), so
the exit continuations stay
disjoint. Nested offsets remain grammatical exactly as before: the
offset
is an `atom`, an atom admits a place, and a subscripted place is a place,
so `lens[order[j]]` derives where `index<u8>(lens, index<u8>(order, j))`
did (its ENT status is likewise unchanged — no term, rebind to discharge
[ENT-2, ENT-6]). Node identity under the flat production: `a[i][j]` is one
`place` node whose children are the `pbase` and one `psuffix` node per
subscript, each with its own extent — which is why every per-subscript
judgment below anchors at the `psuffix` node (review V1).

## 3. Native grammar-verifier expectations

This delta both shrinks and reshapes the frontend contract, so
`whitefoot-grammar` on any v0.22 candidate fails closed against the v0.21
tables ("a structural change must first extend the native grammar path"),
exactly as batch 1 did; the run is recorded at proposal time and the
grammar-path task extends the lexer/parser tables first. Post-extension
expectations: named productions unchanged at 65 (alternatives moved, none
added or removed), terminal predicates 77 -> 76 (the exact-`index`
fixed-terminal predicate leaves; the bracket predicates already exist),
`SELECT_2` rows re-derived for the two modified decisions. Exact counts
are established by that task, not asserted here.

## 4. Modified rules (complete replacement deltas, verbatim anchors)

**[FORM-2]** (one site; resolves the attachment fork — open question O1
records the alternative). "The right-attachment set contains `)`, `]`,
`>`, `,`, `;`, `.`, `:`, `(`, and `<`." becomes "The right-attachment set
contains `)`, `]`, `>`, `,`, `;`, `.`, `:`, `(`, `<`, and `[`." In the
same paragraph, "Thus function headers are `fn f()`, `fn f<T>()`, and
`fn f ['r]()`; generic and square-bracket interiors are compact" becomes
"Thus function headers are `fn f()`, `fn f<T>()`, and `fn f['r]()`;
subscripts are `p[i]`; generic and square-bracket interiors are compact".
Ripple (mechanical reprint, §6): every region-parameter header loses its
pre-bracket space, and a cvalue array after `=` renders `=[…]`.

**[GRAM-5]** As §2: `pbase` loses
`               | "index" "<" type ">" "(" place "," atom ")"` and
`psuffix        := "." IDENT` becomes
`psuffix        := "." IDENT | "[" atom "]"`.

**[GRAM-6]** "`index` is a place (its sole home); bounds semantics are
[OP-4]." becomes "the subscript suffix is a place form (its sole home);
bounds semantics are [OP-4]."

**[GRAM-9]** "Every call argument, construct field value, and `index`
offset is an `atom` [GRAM-5]" becomes "Every call argument, construct
field value, and subscript offset is an `atom` [GRAM-5]".

**[SET-1]** Two sites. "A nested place is evaluated from its base outward;
at each `index<T>(base, offset)`, `base` is evaluated before `offset`, and
the index's [OP-4] discharge obligation is judged at that target place
exactly as in read position, so accepted target evaluation executes no
runtime check and cannot trap." becomes "A nested place is evaluated from
its base outward; at each subscript, the base place is evaluated before
its offset atom, and the subscript's [OP-4] discharge obligation is judged
at that target place exactly as in read position, so accepted target
evaluation executes no runtime check and cannot trap." Later, "lowering
carries the resulting target address and index values across `e` rather
than evaluating source again" becomes "lowering carries the resulting
target address and offset values across `e` rather than evaluating source
again".

**[CONST-2]** "It is read via `index`/`len` (copy-out for copy elements)"
becomes "It is read via subscript/`len` (copy-out for copy elements)".

**[OWN-6]** (review V6: `[suffix]` was metanotation; under this batch it
reads as live subscript syntax with an offset binding named `suffix`.)
"A statement-scoped child reborrow is the written form
`&uniq 'c deref(h)[suffix]` or `&'c deref(h)[suffix]` occurring as an
argument atom" becomes "A statement-scoped child reborrow is the written
form `&uniq 'c` or `&'c` over `deref(h)` followed by any written suffix
chain, occurring as an argument atom". The resolution sentence ("the
borrowed place plus the appended suffix") is already worded and needs no
edit; OWN-13 carries no bracket metanotation (measured against v0.21).

**[OWN-7]** "Two `index` places with the same resolved base overlap iff
their indices are not both literals with unequal values." becomes "Two
subscripted places with the same resolved base overlap iff their offsets
are not both literals with unequal values."

**[OWN-14]** (same V6 class.) "A returned reborrow is the written form
`&'b deref(h)[suffix]` or `&uniq 'b deref(h)[suffix]` occurring as the
complete `expr` of a `return_stmt` [GRAM-4]" becomes "A returned reborrow
is the written form `&'b` or `&uniq 'b` over `deref(h)` followed by any
written suffix chain, occurring as the complete `expr` of a `return_stmt`
[GRAM-4]".

**[OP-1]** Two sites. The table row
`| `index_get` | `array<T, N>`, `slice<'r, T>`, `buffer<T>`, copy element T | `(place, u64) -> own Option<T>` | pure |`
is deleted; `DotlessOperationNames`, and therefore `ReservedLowerNames`,
shrink by that derived member (no text change to the derivation clauses).
In the non-consuming place-operand sentence, "the `len` operand, the place
viewed by `slice_of` through its explicit borrow, and the base place of
`index` and `index_get`" becomes "the `len` operand, the place viewed by
`slice_of` through its explicit borrow, and the base place of a
subscript".

**[OP-4]** Complete replacement of the rule's single paragraph (old text
begins "[OP-4] `index<T>(p, i)` carries the bounds obligation"):

> [OP-4] A subscript `p[i]` selects one element place of an indexable
> base: the base place `p`'s final selected type must be `array<T, N>`,
> `slice<'r, T>`, or `buffer<T>`, and the subscripted place's selected
> type is exactly that element type T — derived from the base place's
> already-stated type by the same declared-type selection that types a
> field suffix, never from expected type or cross-statement inference
> [TYPE-5]; a subscript whose base's final selected type is not one of
> the three indexable types is a hard error citing OP-4 at that
> subscript's `psuffix` node. The subscript carries the bounds
> obligation `i < len(p)` [ENT-6]. A discharged subscript reads or writes
> with no runtime bounds check in every build mode, and its
> checked-program disposition records the discharging derivation
> [DIAG-2]. A subscript whose obligation is not discharged is a
> compile-time rejection citing OP-4 at that subscript's `psuffix` node,
> carrying the residual obligation rendered exactly per
> [ENT-6]; the mechanical fix is a dominating `claim` of the residual
> [CLM-1] or a dominating branch establishing it [ENT-3]. Discharge is a
> deterministic checker derivation [ENT-1]; a solver result never
> participates. A `buffer<T>` obligation is over the runtime length term.
> The offset atom has exact value mode and type `own u64`; after the
> [TYPE-7] implicit-read exclusivity, any other offset mode or type is a
> hard error citing OP-4 at the offset `atom` node, with
> `SourceCoordinate` equal to that atom's complete checked half-open
> source extent. A subscript in a [SET-1] target forms the selected place
> without reading its stored value; its base and offset are evaluated
> during target evaluation, and its discharge judgment is identical in
> target position. A successful bounds judgment neither narrows nor
> authorizes narrowing the offset or its scaled byte offset; target
> address formation additionally obeys [STOR-6]. The range validation of
> the system transfer operations [SYS-8] is an operation-internal
> contract check with table-fixed trap semantics [ERR-4] whose trap
> record uses the operation `call` node [DIAG-3]; the discharge judgment
> does not apply to it.

The `index_get` restatement sentence is deleted with the operation. The
wrong-base hard error makes explicit an attribution v0.21 left unstated
(ruled kept, O7). Node choice, stated exactly per the lead's delegation:
both OP-4 errors anchor at the offending subscript's `psuffix` node, and
the offset-typing error stays at the offset `atom` node. The reviewer's
base-side suggestion was considered and declined because it is not total:
under the flat `place` production the base of a non-initial subscript
(`a[i]` inside `a[i][j]`) has no node of its own, so a base-place anchor
would be undefined for exactly the chains V1 fixed; the `psuffix` node is
the only per-subscript total anchor, and the two OP-4 errors distinguish
by premise, not location.

**[FN-8]** "User-function calls, construction, `move`, borrowing,
`index`, mutation, control flow, allocation, and any trapping operation
are rejected citing FN-8" becomes "User-function calls, construction,
`move`, borrowing, subscripting, mutation, control flow, allocation, and
any trapping operation are rejected citing FN-8".

**[EFF-2]** "an accepted target `index` is discharged [OP-4] and
contributes no `traps`" becomes "an accepted target subscript is
discharged [OP-4] and contributes no `traps`".

**[DIAG-1]** In attribution row 2, "an `atom` occurrence in `atom_list`,
`fieldinit`, or the `index` offset" becomes "an `atom` occurrence in
`atom_list`, `fieldinit`, or the subscript offset".

**[DIAG-2]** "A source `index` place carries no implicit check and no
such disposition: an accepted index is `discharged`" becomes "A source
subscript carries no implicit check and no such disposition: an accepted
subscript is `discharged` at its `psuffix` node" — naming the node keeps
the per-node derivation retention well-defined for chains, where one
place node carries several subscripts (review V1).

**[ENT-1]** (discovered knock-on of the S8 strike; ruled per §7 N1 — the
minimal scoping only, no procedural clause.) The law
sentence "a later specification version may add fact sources and closure
rules and may remove none, so checker strengthening never converts a
discharged obligation into an undischarged one and never converts a claim
into a redundancy-ground rejection — a claim the stronger fragment proves
becomes a [CLM-2] advisory in every later version, never an error."
becomes "a later specification version may add fact sources and closure
rules, and checker strengthening removes none, so it never converts a
discharged obligation into an undischarged one and never converts a claim
into a redundancy-ground rejection — a claim the stronger fragment proves
becomes a [CLM-2] advisory in every later version, never an error."
Without this scoping the S8 strike would contradict the installed law's
"may remove none" as literally read; with it, the guarantee writers rely
on (checker strengthening never rejects their programs) is unchanged, and
what a later specification version may amend is governed by the ordinary
workflow like every other rule — cross-version compatibility promises are
deferred wholesale (owner ruling 2026-08-07).

**[ENT-2]** "formed with any number of `psuffix` field selections and
`deref` wrappings and no `index` segment" becomes "formed with any number
of field-selection `psuffix`es and `deref` wrappings and no subscript
suffix".

**[ENT-3]** Two sites. In S9, "For `let x: own T = index<T>(c, i);` where
c is the
bare IDENT of a named const of type `array<T, N>` [CONST-2]" becomes "For
`let x: own T = c[i];` where c is the bare IDENT of a named const of type
`array<T, N>` [CONST-2]" (T is the derived element type [OP-4]; the rest
of the source is unchanged). Second, the complete S8 bullet — from
"- S8 (the midpoint family). Where a body contains, in this definitional
shape with T unsigned, lo and hi terms of type T," through its fenced
three-let block and the continuation paragraph ending "This is the whole
family; no other multi-variable arithmetic composes in this version." —
is deleted (owner-approved residue strike, review R1: zero corpus sites
write the shape in `tests/programs/` or `tests/conformance/`, the
midpoint is imported folklore not corpus demand, and it was the sole
source keyed to a multi-statement shape rather than one node plus a path
condition). One sentence is appended after the S10 bullet: "The label S8
is retired, not reused: its midpoint family was struck as an
owner-approved version amendment and may return as a later version's
monotone addition the day a corpus program writes the shape." [ENT-4]'s
reflexive implicit bound `t - t <= 0` is retained on its own ground — it
serves derivability generally (equal-term instantiation, derived
`a = a`), and its definition never referenced S8 even though S8 was the
review finding that forced stating it.

**[ENT-6]** Two sites. "for every source `index<T>(P, i)` place — read,
write, and [SET-1] target position alike — the bounds obligation
`i < len(P)`, normalized `i - len(P) <= -1`, at the `index` node" becomes
"for every source subscript `P[i]` — read, write, and [SET-1]
target position alike — the bounds obligation `i < len(P)`, normalized
`i - len(P) <= -1`, at that subscript's `psuffix` node, one obligation
per subscript in a chain".
"For an offset atom that is itself an index-bearing place — legal under
[GRAM-5]'s place grammar but no term under [ENT-2] —" becomes "For an
offset atom that is itself a subscripted place — legal under [GRAM-5]'s
place grammar but no term under [ENT-2] —". The residual rendering
sentence is unchanged and stays well-defined: the base place is the place
the final subscript suffixes, and the offset atom is that subscript's
atom — for a chain, each subscript's residual names its own base prefix
and offset.

**[SYS-2]** (review V5 class 1; respell chosen over exemption.) The
signature block's region-parameter headers — `fn args_count ['a](…` and
every other ` ['` line in the block — respell to the new canonical
attachment (`fn args_count['a](…`). The block declares itself "normative
record notation and … not writable source", so FORM-2 does not formally
govern it, but it is written "in the [GRAM-2] `fn_sig` shape" and must
keep tracking the canonical rendering of that shape or the spec's own
bytes teach a spelling the toolchain rejects. Exemption was considered
and declined for exactly that reason.

## 5. Acceptance-set delta

Widens by exactly two spelling classes: `index` (no longer a fixed grammar
atom) and `index_get` (no longer a derived reserved name) become ordinary
IDENTs, declarable and callable as user names. Narrows by exactly one
semantic class: a program whose discharge relied on the struck S8 midpoint
facts would newly reject — measured at zero sites in both corpora
(2026-08-07; the only shift-by-one sites are a CRC bit fold and a
pool-tree bound, neither in the three-let shape, and no binary search
exists), so the narrowing is real in kind and empty in fact, performed as
an ordinary version amendment under the workflow; the scoped [ENT-1] law
binds checker strengthening only, and this honest per-batch accounting is
batch convention, not law. Every other
judgment — bounds obligation, offset typing, discharge, kills, residuals,
dispositions — is spelling-transported, not changed.
Every existing program that spells `index<…>(…)`, a region-parameter
header, or a cvalue array changes canonical bytes and is respelled by
mechanical reprint (§6); under FORM-1 the old bytes are rejected, which is
the ordinary consequence of any canonical respelling, not a semantic
narrowing.

## 6. Corpus migration (mechanical, printer-driven)

Per SWEEP.md's A/C batch rule: the canonical printer computes the new
spelling from the old tree; zero semantic judgment. Measured footprint,
corrected per review V4 (2026-08-07; governance measurements count
canonical `.wf` sources only — the first draft's 40 cvalue count included
nine Python list literals in `tests/conformance/runner.py` and
`test_runner.py`, which do not reprint): 266 subscript sites in
`tests/programs/*.wf`, 138 in
`tests/conformance/`, `index_get` used nowhere in either corpus; under the
O1 attachment ruling additionally 84 region-parameter headers
(`fn f ['r](` -> `fn f['r](`) and 31 cvalue arrays (`= [` -> `=[`)
reprint. Two further ripple classes per review V5: the [SYS-2] signature
block's own ` ['` headers respell in the full-document assembly (§4
SYS-2), and the `, [` boundary — a nested cvalue array rendering `],[`
after a comma — is named for printer review although both corpora carry
zero such sites today. One conformance case is added in the same change
(ruled O5): a positive case using `index` as an ordinary value binding,
pinning the released spelling; additive cases need no approval-ledger
entry. Conformance sources and any spelling-bearing manifest expectations
respell in the same change under the standing derived-material rule (the
spec change brings everything derived from it to the newest version in the
same work); no verdict changes meaning, and no protected expectation is
weakened — this is the respelling the rule anticipates, not an
owner-approval verdict edit. The derivation ledger takes its v0.22
amendment in the same change (rules +0/-0; the modified-rule notes, the
register reduction, and the S8 strike recorded as PARKED with its monotone
re-add note), and `docs/patterns.md` writer forms that spell
indexing update likewise.

## 7. Ruled and open list

All items are ruled (owner "approved", translated from Chinese, 2026-08-07,
applying the adversarial review at 186bde5):

- R1 — the three items themselves: index_get removal (option a), subscript
  respelling (SWEEP C2), element-type deletion (SWEEP A2).
- R2 — batch composition: one v0.22 micro-batch ahead of the remaining
  spelling-relief items, now including the S8 residue strike.
- O1 — ruled: `[` joins the right-attachment set; ripple carried with the
  review-corrected counts (84 region headers, 31 cvalue arrays; canonical
  sources only) and the V5 classes enumerated in §6.
- O2 — ruled: settlement reading — SWEEP's four-test verdict is the
  validation the R3-PROVISIONAL register demands; the surviving "deref
  prefix places" half stays provisional.
- O3 — ruled AGAINST the draft, per review must-fix V1: the drafted
  place-node anchor is undefined for chains (one flat place node carries
  every subscript), so every per-subscript judgment — the undischarged
  rejection, the wrong-base error, ENT-6's obligation attachment, and
  DIAG-2's derivation retention — anchors at the subscript's `psuffix`
  node. Applied throughout §4.
- O4 — ruled: no tightening; nested subscripts mirror v0.21.
- O5 — ruled: no soft-reservation; one additive conformance case pins
  `index` as an ordinary binding (§6).
- O6 — ruled: batch-1 verifier shape as drafted.
- O7 — ruled: the wrong-base attribution sentence is kept, re-anchored per
  O3. The node choice within the ruling was delegated and is stated
  exactly in §4's OP-4 entry: the subscript's `psuffix` node, not the
  reviewer's suggested base place node, because the base of a non-initial
  subscript has no node under the flat place production — the base-side
  anchor would be undefined for exactly the chains V1 fixed.
- R1 (residue, owner-approved) — ENT-3 S8 struck and PARKED: zero corpus
  demand, imported shape-keyed folklore, the sole multi-statement
  shape-keyed source; restoration is a later version's free monotone
  addition the day a corpus program writes the shape (§4 ENT-3).

Discovered knock-on, now ruled:

- N1 — ruled (owner, 2026-08-07): version compatibility is deferred
  wholesale — the language makes no cross-version compatibility promises
  at this stage. The minimal resolution stands: [ENT-1]'s removal
  prohibition is scoped to checker strengthening (§4), and the initially
  drafted procedural sentence ("deliberate source removal is an
  owner-approved re-decision recorded with measured acceptance impact")
  is dropped — spec-version amendments are already governed by the
  ordinary workflow and need no special clause; the honest
  acceptance-delta accounting stays in §5 as batch convention, not law.

No other contradiction between the ruled items and v0.21 was found:
every collision (the OP-4 restatement of `index_get`, the OP-1
sixteenth-site base list, ENT-2/ENT-3/ENT-6's spelling anchors, the
FORM-2 attachment gap, the OWN-6/OWN-14 metanotation, and the SYS-2
signature block) is an enumerated modification above.
