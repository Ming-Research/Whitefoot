# Reborrow extension — exact v0.31-candidate rule edits (DRAFT, not a spec change)

> Superseded 2026-08-18: applied at v0.31 activation (eb8e8634). Historical design evidence; the active spec owns the normative text.

Status: DRAFT delta text for batch 0070 (W5 item "grandchild reborrows and
call-result-borrow roots"), prepared for the lead's single-writer integration
into the one v0.31 candidate. Nothing here edits `spec/kernel-spec.md`; every
byte below awaits the owner's exact-byte approval. Builds on
`research/investigations/reborrow-investigation/` (v0.7 statement-scoped
child, its escape analysis, and the recorded deferrals) and on v0.20's
returned reborrow; it does not re-derive them.

Implementation status: implemented in the compiler behind the one-line
integration switch `REBORROW_EXTENSION_ACTIVE` in
`compiler/src/semantic/check.rs` (default `false` = v0.30 semantics
byte-identical; the v0.31 activation change flips it to `true`). Test-only
entries `check_semantics_reborrow_extension` / `emit_reborrow_extension`
exercise the extension; paired default-checker tests pin the v0.30
dispositions of the same sources.

## 1. What is admitted (summary)

Two deferred forms move, coupled through the call boundary:

1. **Call-result-borrow roots.** A `let` whose ordinary right-hand side is a
   user call with a borrow-mode result becomes an ordinary borrow holder —
   readable, writable (`uniq`), and reborrowable — exactly when the callee
   signature determines one **provenance-candidate** parameter (§3). The
   holder's resolved place is the candidate actual's complete resolved place.
2. **Grandchild chains composed through calls.** With (1), a written child
   reborrow whose eligible holder is itself a bound call-result borrow forms
   a lineage of depth ≥ 2. No new rule is needed for the chain itself: the
   bound result is a `let`-bound holder, so the existing OWN-6 argument-atom
   child and OWN-14 returned reborrow apply to it unchanged, and
   resolved-place composition is the existing recursive resolved().

One admission change makes (1) reachable: in the **candidate position only**,
a written child reborrow may feed a **borrow-returning** call, and its region
is any live region the parent's region outlives-or-equals (caller-supplied
included) rather than a statement-scoped one. The price is stated in §4:
creating that child through a `&uniq` parent suspends the parent for the
remainder of its life.

## 2. Why the boundary sat where it sat, and what carries the move

v0.7's `own`/`unit`-result condition was the escape closure: a callee could
otherwise launder the child's loan into a result that outlives the statement,
and the statement-end resumption of the parent would leave two usable `&uniq`
paths to one place (reborrow-investigation MINIMAL-RULE §3, the rev-1 F001
miscompile). This delta admits the laundering deliberately and replaces the
closure with two facts:

- the **claim survives in the result holder**: the bound result carries the
  candidate actual's complete resolved place as an ordinary loan, so every
  conflicting access is judged by the existing OWN-5/OWN-7 machinery; and
- the **parent never resumes**: suspension extends from statement-scoped to
  remainder-of-life (the OWN-13 arm-scoped precedent), so no program point
  observes parent and chain both usable. Relaxing resumption later only grows
  the accepted set (the recorded v0.20 ground).

The singleton holder invariant (T-A) is preserved: a result holder has one
immutable resolved root, never a set. Signature-level unambiguity (§3) is
what buys this; a possible-provenance set over borrows (the slice v0.17
shape) was considered and rejected for this slice because it would be the
first non-singleton borrow root and T-A is the load-bearing
frontend-scale-checker simplification.

## 3. The provenance-candidate judgment (signature-level, formal regions)

For a callee signature with borrow-mode result `&K 'b T` (K ∈ {shared,
uniq}, `'b` a formal region, T region-free):

- a parameter is the **candidate** iff its written mode is a borrow of the
  same kind K in the same formal region `'b`;
- the judgment holds iff there is **exactly one** candidate and **no other
  parameter names `'b` anywhere** — in its mode region or inside its written
  type (`slice<'b, _>`; every other storable type is region-free by STOR-5;
  an unsubstituted generic conservatively counts as naming every region);
- a slice-typed or otherwise region-carrying result forms no candidate,
  whatever region the written slice names (OWN-5 already rejects the
  borrow-mode direct-slice result shape; the candidate rule fails closed on
  it independently).

Soundness ground (the induction the caller relies on): inside the callee,
distinct caller-supplied regions are incomparable (OWN-3 fails closed), and
OWN-10 forbids rooting a `'b`-region borrow in callee-local storage, so every
borrow an accepted callee can deliver in `'b` derives transitively — through
its own returned reborrows, returned holders, and nested borrow-returning
calls — from an actual that names `'b`, or from named-const storage
(OWN-10's const case), which is immutable for program lifetime and needs no
claim. Caller-side region aliasing (passing one actual region for two
formals) cannot widen this: the callee's judgments are over formals.

Kind filtering is asymmetric and the rule stays conservative about it: a
`uniq` result can derive only from `uniq` sources (no upgrade exists), but a
`shared` result CAN derive from a `uniq` parameter (a shared child of a uniq
parent laundered through a nested borrow-returning call), so any same-region
parameter of the other kind makes the signature ambiguous and the judgment
fails — reject-when-unsure (OWN-8).

**The claim is deliberately wide:** resolved(result holder) is the complete
candidate actual place even when the callee returned a narrower suffix
(`return &uniq 'r deref(p).left`). Prefix overlap (OWN-7) makes the wide
claim cover every narrower truth; the cost is that two sibling chains to
disjoint fields of one root are rejected. Recorded as the same conservatism
class as OWN-7 itself.

## 4. Exact rule edits

Line references are to the v0.30 active file at `caps-batch` 029f31e.

### OWN-5 (lines 504–505, 510)

- 504: extend the creation exception list:
  "…except the creation of a statement-scoped child reborrow, an arm-scoped
  child reborrow, a candidate-position child reborrow, or a returned reborrow
  of that holder [OWN-6, OWN-13, OWN-14]."
- 505: extend the suspension sentence:
  "While a holder is suspended (a live statement-scoped child, arm-scoped
  child, candidate-position child, or returned reborrow of it exists), its
  own read/write allowance is withdrawn: no read, write, move, copy, `set`
  commit, or call-transfer through it is admitted until its last child ends;
  a `&uniq` holder suspended by candidate-position child creation does not
  resume — its claim may survive in the bound call result [OWN-6]."
- 510: extend the invariant gloss:
  "…the only overlapping pairs — a suspended parent with its
  statement-scoped child, arm-scoped child, candidate-position child, bound
  call-result holder, or returned reborrow — are never both-usable by
  construction."

### OWN-6 (lines 534–537)

- 534 (child admission), replace the result-mode condition clause:
  "…admitted only when: the receiving call's result mode is `own` or `unit`,
  never a borrow — except in the receiving call's provenance-candidate
  position, where a borrow result is admitted; `'c` is a locally-introduced
  region [OWN-3] whose block does not extend beyond the enclosing statement,
  and a caller-supplied region parameter is not admitted — except in the
  provenance-candidate position, where `'c` is any live region that
  resolved(`h`)'s region outlives-or-equals, caller-supplied included; …"
  (holder eligibility and uniq-needs-uniq unchanged).
- 535 (suspension), append:
  "Creating a candidate-position child through a `&uniq` holder suspends
  that holder for the remainder of its life; there is no statement-end
  resumption, because the child's claim may survive in the bound call
  result. A shared holder needs no suspension: it admits no write through
  itself."
- NEW paragraph after 536 (the call-result borrow holder):
  "A `let` whose ordinary right-hand side is a user call with borrow-mode
  result is a borrow holder exactly when the callee signature determines one
  provenance-candidate parameter: the one parameter written as a borrow of
  the result's kind in the result's formal region, with no other parameter
  naming that formal region in its mode or written type and with a
  region-free result type. resolved(result holder) = the candidate actual's
  complete resolved place, even when the callee delivered a narrower suffix
  of it; the holder's borrow is otherwise ordinary — OWN-4 liveness in the
  substituted result region, OWN-5 exclusivity, OWN-6 child admission,
  OWN-14 returned reborrow. Binding a borrow-mode user-call result whose
  callee signature does not determine a candidate is a hard error citing
  OWN-6 with the restructuring `give the callee exactly one parameter
  written as a borrow of the result's mode and region and no other parameter
  naming that region, or bind the borrow from a direct borrow expression`.
  Nothing here narrows FN-1: the caller still judges the call by the
  signature alone."
- 537 (deferral list): remove "grandchild reborrow chains" (chains composed
  through bound call results are admitted above; the within-function bound
  form remains deferred with bound children); the list becomes:
  "Bound children, result-carrying children (reference-result provenance),
  `uniq`-to-`shared` downgrade, `match`-binder parents, and written
  grandchild chains through a bound direct reborrow are DEFERRED with
  recorded delta; …"

### OWN-9 (line 548, non-normative)

Extend the parenthetical: "…a statement-scoped child, arm-scoped child,
candidate-position child, bound call-result holder, or returned reborrow and
its suspended ancestor, though both live, are never mutually noalias…".
(No alias-scope emitter exists in the active compiler; conditions C/D of the
v0.7 review bind any future emitter to lineage awareness across call-result
holders too.)

### OWN-12 (line 561)

Extend the ancestor exemption: "When an argument is a statement-scoped or
candidate-position child reborrow [OWN-6], its suspended ancestor holder is
excluded from this effect-row overlap check…" (unchanged otherwise; the
existing place-prefix ancestor test composes over chains because every chain
member's resolved place extends its ancestor's).

### OWN-14 (line 581)

Deferral list: remove "reborrows rooted at a call-result borrow" (a bound
call-result borrow is a `let`-bound holder under OWN-6, so reborrows rooted
at it are already the admitted argument-atom and return positions); the rest
of the list is unchanged. Return position's "sole non-argument position"
sentence is unchanged.

### ENT-5 (line 2771 support; 2793 kills)

Append one sentence to the holder clause of the support definition:
"…and every borrow or box/arena holder binding any of its places reads
through by `deref`, a bound call-result holder included — its resolved place
is the candidate actual's complete resolved place [OWN-6], so a `set` commit
or projected callee write through the chain kills exactly the facts
supported by that storage."
Kill events (a)–(d) themselves are unchanged: (a)/(b) reach the chain
through resolved(); (d) already kills on edges leaving the region of any
supporting holder, and a result holder's binding scope is always inside its
region's block.

### META-5 accounting

Rules edited: OWN-5, OWN-6, OWN-9 (non-normative), OWN-12, OWN-14, ENT-5.
Tokens ±0, spellings ±0. Exceptions: +2 (candidate-position result-mode and
region exceptions in OWN-6), −1 deferral item (call-result-borrow roots),
−1 deferral item narrowed (grandchild chains → bound-direct-reborrow form
only). SELECTION GROUND: evidence-selected (the recorded standing revival
candidate: binary-trees recursive-arena shape, wfc through-holder census,
v0.20 validation), with the candidate-position suspension minimality-selected.

## 5. Stated consequences (for the review packet)

- **Bound-child equivalence.** An identity passthru
  (`fn same['r](x: &uniq 'r T) -> &uniq 'r T { return &uniq 'r deref(x); }`)
  gives writers a bound child with strictly stronger obligations than a
  native `let c = &uniq 'r deref(h)` would need (permanent parent
  suspension). The accepted set therefore contains bound-child power; the
  native bound form stays deferred only as surface, not as capability.
- **Recursion composes.** The candidate child admits caller-supplied
  regions, so `fn walk['r](t: &uniq 'r Node)` can bind
  `let c = step<'r>(x: &uniq 'r deref(t));` and recurse on a child of `c` —
  the recursive traversal shape the binary-trees pilot could not express
  (the standing revival candidate). The pilot's re-run trigger applies once
  v0.31 activates.
- **Sibling chains are rejected** (whole-place claim, §3), and a second
  chain from a suspended parent is the OWN-5 rejection; scattered deep
  writes keep the command-buffer alternative.
- **Discarded borrow-returning calls** stay legal; a candidate child passed
  to one still suspends its `&uniq` parent permanently (conservative: the
  claim could not have escaped, but the rule does not special-case discard).
- **Facts-off acceptance is unaffected**: the extension changes acceptance
  only through the ordinary checker rules above, and the entailment change
  only kills more facts, never creates them.

## 6. Compiler capability gaps recorded (not rule changes)

- **Suffixed reborrows** (`&uniq 'c deref(h).field`) remain an explicit
  RegionsAndBorrows capability stop in both admitted positions (checked
  model has no address-of-field reborrow expression). Spec-admitted since
  v0.7/v0.20; unimplemented before and after this delta.
- **Holder scope envelope**: a `let`-bound holder is supported only
  immediately inside its region's block, or at function-body top level for a
  caller-supplied region; inner-block holders remain a capability stop
  (claim tracking is scope-carried).
- **v0.30 caller-side defect fixed alongside** (switch-independent): a
  discarded borrow-returning call statement was accepted and then failed as
  backend InvalidIr because the call site was typed with the referent value
  type; calls are now typed by the callee's declared result and a discarded
  borrow result is never dropped [OWN-2, STOR-3].

## 7. Evidence

- Acceptance + both-direction gating + permanent suspension + ambiguity +
  non-candidate rejection: `compiler/src/semantic/tests/borrows.rs`
  (`extension_*` and `default_checker_keeps_the_v030_dispositions_*`).
- ENT-5 kill with deliberate negative control:
  `extension_writes_through_result_holders_kill_source_facts`.
- End-to-end execution of the chain:
  `compiler/src/backend/tests/reborrows.rs`
  (`extension_chains_execute_and_write_the_owners_storage`), same source as
  `chain-evidence.wf` beside this file.
- v0.30 regression for the defect fix:
  `a_discarded_borrow_returning_call_compiles_and_runs`.

## 8. mcts_mem delta for the lead (apply at v0.31 activation, not before)

[mcts_mem/whitefoot/ownership/no-reborrow.md](https://github.com/mbbill/Whitefoot/blob/0f22b026b062c36ef2f8ce9c725ee4307e1fda64/mcts_mem/whitefoot/ownership/no-reborrow.md): edit Items 3–4 — the admitted
family gains the candidate-position child, the call-result borrow holder
with signature-unambiguous provenance, and the chains they compose;
remainder-of-life suspension of the candidate `&uniq` parent; deferred list
drops call-result roots and narrows grandchild chains to the
bound-direct-reborrow form. Append a dated validation fact citing this
delta, the test evidence above, and the conservative whole-place claim. No
node moves to `.alt/`: the v0.7/v0.20 forms are extended, not replaced.
