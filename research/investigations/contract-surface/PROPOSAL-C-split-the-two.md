# Proposal C — split the two, and what survives when the split is taken away

Research document. Proposal only: no specification byte, compiler line, test, or
conformance case is changed by this file. Written against `spec/kernel-spec.md`
v0.32 at branch `batch-0072` tip (`d32d7dd0`).

Study slot P3 of a four-way design study of the `requires`/`ensures` contract
surface. The assigned stance was **stop making them share a form**: design
`requires` and `ensures` separately, and lift the executable program-start check
out of `requires` into an explicit construct at the entry.

Two owner rulings arrived mid-study and removed the load-bearing half of that
stance. This document therefore does two things: it reports the re-aimed
proposal, and it states plainly where the original stance died and why.

---

## 0. The two rulings, and a measurement correction

**Ruling 1.** A contract must generate no runtime code. Requirement, not
preference. Exactly one exception is tolerated, temporarily: the executed
`requires` final at a program entry.

**Ruling 2 (supersedes the FFI plan in ruling 1 for now).** "The entry is not
under consideration for now" (translated from Chinese) — the program entry is
out of scope, and the owner is willing to require that any declaration carrying
a `requires` is an internal function.

Ruling 2 removes the entire subject of my assigned stance. There is no entry
check left to house in an explicit construct, because no entry may state a
requirement at all. A restriction is cheaper than a construct and reaches the
same end state. I accept that and drop the construct; §9 states what I think
that costs me.

**Measurement correction — load-bearing.** Two numbers reached me with the
rulings and both are wrong. I re-measured the live tree (637 `.wf` files,
`archive/` excluded) directly:

| claim as received | measured |
|---|---|
| "77 entry declarations" | **533** `fn main` declaration headers in 531 files — 457 unlabelled, 76 `command fn main`, plus 3 negative cases naming a reserved or unadmitted kind (`service`, `embedded`, `daemon`) |
| "NOT ONE carries a `requires`" | **three** entries carry one |
| corrected to "exactly ONE carries a `requires`" | still wrong: **three** |

The three, in full:

```
tests/conformance/cases/fn8-trap-requires-false.wf:1
  fn main() -> own unit pure requires {
tests/conformance/cases/clm3-neg-generated-wrapper-check.wf:1
  deny_claims command fn main() -> own ExitStatus pure requires {
tests/conformance/cases/clm3-pos-transitive-value-branch.wf:36
  deny_claims command fn main() -> own ExitStatus pure requires {
```

Two consequences.

1. **The restriction must be phrased over the [FN-7] entry, not over
   `program_kind`.** [PROG-3] governs *both* entry forms — "This rule governs
   both the unlabelled no-input entry and the `command` entry." A restriction
   spelled "no `program_kind` declaration may carry a `requires`" leaves the
   unlabelled `fn main()` form fully executing the compiler-owned wrapper, and
   that is exactly the form `fn8-trap-requires-false.wf` uses; its manifest
   verdict is `{"kind": "trap"}`. Under that phrasing the invariant "a contract
   generates no runtime code" would be *false while believed true*, which is
   worse than today's documented exception. Since [FN-7] mandates exactly one
   top-level `fn_decl` named `main` per unit, the correct phrasing is over that
   declaration.

2. **The protected cost is three conformance cases, not zero and not one**, and
   two of the three have the deleted mechanism as their *subject*, so they
   cannot be migrated by rewriting bytes — they lose their reason to exist. §9
   states the disposition.

---

## 1. What I verified in the spec

Anchors, all read in full at v0.32 before designing.

- **[GRAM-2]** contract productions: `requires_block`, `requires_entry`,
  `ensures_block`, `ensures_selector`, `ensures_entry`.
- **[GRAM-4]** `check_stmt := "check" expr "else" "trap" STRING ";"` — and
  critically, `check_stmt` is **not** an alternative of `stmt`. Its only
  producers are `requires_entry` and `ensures_entry`. The v0.32 trap-endpoint
  work (`f8c81dfc`, mcts node `writer-trap-surface`) retired the anonymous body
  check in favour of `claim`, and left `check_stmt` alive solely as the contract
  final. It is already a dedicated one-position production.
- **[OP-5]** owns the exact `own Bool` condition judgment, and carries two
  further sentences: the requires final "uses this exact condition judgment,
  decoded message, and dynamic-boundary failure behavior"; the ensures final
  "uses the exact condition judgment but [FN-9] owns it as a proof obligation;
  it never executes and has no dynamic-boundary failure behavior."
- **[FN-8]** in full: the structural pass, alpha expansion, goal identity, the
  ordinary-call pre-transfer judgment, the S4 body axiom, the program-start
  block (13 sentences), and the CLM-3 marked-entry clauses.
- **[FN-9]** in full: structural pass, selector admission, the single-comparison
  RelationTemplate, entry-image stability, the three views, the SCC schedule,
  the four closed S12 result routes, and the failure-atomic batch.
- **[PROG-3]**, **[GATE-1]**, **[DIAG-2]**, **[DIAG-3]**, **[EFF-2]**,
  **[ENT-3.S4]**, **[ENT-1]**, **[CLM-3]**, **[PRV-3]**, **[FORM-2/3/5]**.
- **[FN-4]**: "The bound function is nongeneric and has neither `requires` nor
  `ensures` block." One clause excludes both — the spec already treats them as
  one category where it does not care about direction.
- **[CONST-2]** `cvalue := literal | IDENT | "[" ... "]" | TYPEID targs? "(" ... ")"`.
  No call form. Const evaluation creates no call edge into a user function.
- mcts: `checks-and-proofs/requires-entry-contract` and its
  `requirement-enforcement` child, plus both `.alt` nodes
  (`recognizer-driven-elision`, `callee-entry-prologue`).

The mcts record matters for one reason. The 2026-07-11 rationale on
`requires-entry-contract` reads: "callee-boundary coverage was selected over
reliance on known callers because the direct-C entry path showed entry
enforcement is necessary — a caller-proof scheme leaves foreign entries
unprotected." That is the decision the current shared form descends from. Ruling
2 does not refute it; it *scopes it out*. The end state below is a caller-proof
scheme that is sound precisely because the foreign entry it feared does not
exist in this version, and the restriction makes the absence checkable rather
than assumed.

---

## 2. Corpus measurement

Live tree, `archive/` excluded, 637 `.wf` files. Counted with a
character-accurate brace-depth scanner rather than line greps (a line-based
count over-attributes by ~3x because `} {` and `} ensures result {` join two
blocks on one line).

| quantity | count |
|---|---|
| `requires` blocks | **87** |
| `ensures` blocks | **19** (15 plain selector, 4 `Ok(value: r)`) |
| final `check … else trap` in `requires` | 85 (2 negative cases deliberately omit it) |
| final `check … else trap` in `ensures` | 19 |
| `let` entries in `requires` blocks | 255 |
| `let` entries in `ensures` blocks | 2 |
| `doc` entries inside contract blocks | 1 (the negative case `fn8-neg-doc-only-clause`) |
| `check … else trap` anywhere else in a live `.wf` | 0 |
| `check … else trap` in stale `research/` programs (pre-v0.32 body checks) | 11 |
| `.wf` files carrying at least one contract block | 90 (44 under `tests/conformance/cases`, 46 elsewhere) |
| entries (`fn main`) carrying a `requires` | **3**, all under `tests/conformance/cases` |
| inline `check … else trap` fixtures in Rust files under `compiler/` | 248, in 17 files |

Distribution of `requires`: 37 conformance cases, 36 files under
`tests/codegen/cases/bounds/output-capacity-lockstep`, 5 `tests/programs`, 2
`research/experiments/zlib-core-kernels`, 1 `tests/codegen/cases`, 1
`research/experiments/port-study/base64`. Distribution of `ensures`: 11
conformance cases, 3 `tests/programs`.

**The shape of the defect, measured.** 104 contract finals exist. Under v0.32,
three of them can ever emit a [DIAG-3] record — the three entries above, all
test cases, none in `tests/programs` and none in a research program. After ruling
2 the number is **zero**. So the `else trap "msg"` half of the spelling is inert
at 101/104 sites today and at 104/104 sites after the restriction, and its
STRING has no compile-time identity anywhere: [FN-8] "does not select a predicate
by clause-local spelling", [FN-9] "its message … ha[s] no identity", [DIAG-2]
retains it for neither template.

---

## 3. The question that survives: are they two kinds of thing?

With execution gone from both, the assigned framing ("they are not the same kind
of thing, and the shared form is the root defect") has to be re-argued on
structure alone. I argued it both ways against the spec text. The honest answer
is neither "one thing" nor "two things".

### 3.1 What they genuinely share

Both blocks are, verbatim in behaviour:

- `let*` then exactly one final Boolean statement, enforced by an early
  structural pass that runs *before* any child is recursively checked, reports
  the first offending entry left to right, and reports the block node itself for
  a missing final;
- an admitted computation subset restricted to ANF [GRAM-9] calls to, or infix
  spellings of, pure, total, non-trapping operation-table rows;
- clause-local `own` copies, visible only to later entries of the same block and
  never to the body;
- recursive alpha expansion of clause locals into one finite typed expression
  tree;
- an identity rule that retains parameter ordinals and projections, named-const
  declaration identity, typed literals, concrete type and const substitutions,
  selected operation rows, and operand order — and discards clause-local
  spelling, sharing, and NodePaths;
- no effect contribution [EFF-2], no runtime operation, no lowering path.

That is not a family resemblance. It is the same mechanism, specified twice.

**And the two statements have already drifted.** Three concrete pieces of
evidence from the v0.32 bytes:

1. FN-9's inadmissible list names `claim` explicitly. FN-8's does not — it
   reaches `claim` only through "any other direct statement shape", and the fact
   that a claim cannot appear in a `requires` block is stated *in a third rule*,
   [CLM-3]: "FN-8's structural pass admits only ordinary lets and one final
   check, so a claim cannot appear in a `requires` block." One rule's structural
   pass is documented inside another rule.
2. FN-8 says the admitted rows are "non-trapping, total … with effect `pure`".
   FN-9 says "pure, total, non-trapping". Same content, two orderings, two
   sentences to keep in sync.
3. The identity-exclusion lists differ by exactly one item: FN-9 excludes
   "message bytes", FN-8 does not — because FN-8's message *is* load-bearing at
   program start. **The entry restriction is precisely what makes those two
   lists mergeable.** That is the cleanest single argument that ruling 2 and the
   surface change belong in the same change.

### 3.2 What they genuinely do not share

Five axes, none of them cosmetic.

**(a) Who owes the proof, and where.** A `requires` goal is owed by the *caller*
and discharged once per call site, in the caller's complete fact state at the
pre-transfer point, before any consume, borrow commit, or callee-effect kill. An
`ensures` relation is owed by the *callee* and discharged at every selected
return, immediately before return transfer and edge-carried cleanup. One is a
per-site query; the other is a universally quantified per-exit obligation with a
non-vacuity rule ("the selected-return set must be nonempty … this explicit
non-vacuity surface rule is not implied by FN-8").

**(b) Different admitted predicate languages.** FN-8 admits an arbitrary
Boolean tree — `band`, `bor`, `bxor`, `bnot` over comparisons — as **one atomic
goal** that "no evidence for its children ever composes". FN-9 admits **exactly
one comparison root** (`ieq`/`ine`/`ilt`/`ile`/`igt`/`ige`) whose operands are
drawn from a closed set: the symbolic result datum, a parameter datum with only
field and `deref` projections, a named const, a typed integer literal, or
`len(P)`. No connectives, no arithmetic, no computed operand. This is not an
oversight in either direction: a requirement goal is *consumed atomically*, while
a postcondition must *normalize into an L0 RelationTemplate* to be publishable
into a caller's relation state at all.

**(c) Different value universes.** An `ensures` may — and must — name the
symbolic result datum, which does not exist at entry; a `requires` cannot. An
`ensures` parameter datum denotes an *entry image* carrying a view-independent
stability judgment that can render it permanently unavailable on the first
[ENT-5] kill overlapping the datum, a holder, or its ordinary support; a
`requires` datum has no such lifetime layer because it is judged at one instant.

**(d) Different fact channels.** [ENT-3.S4] injects the requirement goal *into
the callee body* as one signed opaque goal plus, conditionally, its exact L0
projection. FN-9/S12 injects the relation *into the caller* through four closed
result routes (direct ordinary-let call; the `Ok(value: payload)` arm binder;
the narrow `set x = user_call(...)` receiver; the narrow first-statement payload
receiver), each with its own liveness and [OWN-7] disjointness side conditions,
in three views with a Bq-first evidence-identity rule, under an SCC schedule
that withholds same-component summaries. Nothing on the `requires` side
resembles the route machinery, and nothing on the `ensures` side resembles S4.

**(e) Different failure modes.** A failed `requires` is a call-site rejection at
the caller's `call` node. A failed `ensures` is a declaration-site rejection at
the first source-ordered selected return, with U/B failure *not* rejecting at
all but instead becoming checked metadata.

### 3.3 Verdict

**They are one kind of *thing* and two kinds of *obligation*.**

The thing — a declared proposition over a function's boundary values, written as
`let*` plus one final Boolean, alpha-expanded to one template with one identity
discipline, proved by the compiler, generating no code — is one mechanism, and
the specification currently states it twice and has already drifted three times.

The obligation — who owes it, at which program point, over which value universe,
into which fact channel — is two, and no amount of surface unification should
paper over that.

So the shared spelling is **not** coincidence, and my assigned premise ("that
shared form is arguably the root defect") is wrong as stated. The defect is
narrower and more embarrassing: the shared part is spelled as **a statement of an
executable action** — `check E else trap "M";` — inherited from a body construct
that v0.32 already deleted from the statement grammar. The blocks share the
right thing in the wrong words.

That makes the proposal **one contract form, two directions**.

---

## 4. Proposal C1 — one contract form, two directions

### 4.1 Grammar

[GRAM-2], replacing four productions with four:

```text
requires_block  := "requires" "{" clause_entry* "}"
ensures_block   := "ensures" ensures_selector "{" clause_entry* "}"
ensures_selector:= IDENT | TYPEID "(" fieldbind_list? ")"
clause_entry    := doc | stmt | proposition
proposition     := "holds" expr ";"
```

[GRAM-4], deleting one production outright:

```
check_stmt  := "check" expr "else" "trap" STRING ";"      -- DELETED
```

`check_stmt` has no other producer (`stmt` does not list it), so deletion is
total. `requires_entry` and `ensures_entry` collapse into one `clause_entry`,
reducing the core-tree node-kind count by one under [GRAM-1]'s 1:1 mapping. The
`fn_decl` production is otherwise untouched: `effects requires_block?
ensures_block? "{" doc? stmt* "}"`.

`clause_entry` keeps `stmt` in the grammar deliberately, preserving [GRAM-2]'s
existing property that "syntax formation does not encode the block's semantic
subset" — the structural pass, not the parser, decides admission. `SELECT_2`
disjointness is trivial: `doc`, `let`, and `holds` are three distinct fixed
lower-word atoms and `stmt`'s remaining first-sets are disjoint from all three.

**Terminal choice.** `holds` — one fixed lower-word atom, IDENT-ineligible under
[FORM-3] once fixed. Verified against the corpus: `holds` occurs in the live tree
only inside `doc` and `claim` STRING interiors (10 occurrences), never as an
identifier, so it costs no rename. It reads as a proposition rather than an
action, which is the whole point; `assert` was rejected because [SCOPE-2]
reserves the trusted-assertion vocabulary for the §14 ledger, and `check` was
rejected because it is the word we are retiring.

### 4.2 Lexical and formatting consequences

- **[FORM-2] line-bearing list**: replace `check_stmt` with `proposition`. The
  block-bearing list and the function-clause rendering sentences ("A function
  whose first clause is `requires_block` puts its header through `requires {` on
  one line", the `} ensures ` join line, the `} {` join line) are unchanged.
- **[FORM-5]**: "STRING appears only in `doc` entries, contract final `check`
  messages, and `claim` justifications" becomes "only in `doc` entries and
  `claim` justifications".
- **[FORM-3]**: unchanged. `check` and `trap` leave the fixed-atom set and
  therefore become IDENT-eligible. This needs no new reservation: `wrap`,
  `checked`, `sat`, and `strict` are already in exactly that position — they are
  OPNAME *suffixes*, not atoms, so they are already legal IDENTs today and are
  reserved only from field binding. `trap` joins them with no new mechanism and
  no maximal-munch hazard, because no struct field may be named `trap` and so the
  place `x.trap` is unspellable regardless.

### 4.3 Rule text

**[OP-5]** loses its two dynamic clauses and becomes a pure type judgment with
three named users:

> [OP-5] A condition judgment requires its expression to have exact value mode
> and type `own Bool`, where `Bool` is the PRE-1 nominal type. No integer, other
> enum, borrowed `Bool`, or implicit truthiness conversion is admitted [TYPE-4].
> The implicit-read case already owned by [TYPE-7] is exclusive: when the
> expression uses a borrow-mode or box/arena binding where its referent `Bool`
> value would be required, that use is rejected citing TYPE-7 and OP-5 forms no
> candidate. Every other exact-mode or exact-type failure is a hard error citing
> OP-5 at the selected `expr` node, with `SourceCoordinate` equal to that
> expression node's complete checked half-open source extent.
> The judgment has exactly three users: an `if_stmt` or `value_if` condition
> [GRAM-6], a `requires` block's final `proposition` [FN-8], and an `ensures`
> block's final `proposition` [FN-9]. It states a type judgment only: it
> introduces no runtime evaluation, no trap, and no [DIAG-3] record.
> The fuller stated-and-checked vocabulary (loop invariants, ranges) is DEFERRED
> with its delta.

**[FN-8]** states the shared clause-block judgment once, in the paragraph it
already occupies, and FN-9 cites it instead of restating it. The shared
statement, replacing FN-8's current structural-pass paragraph:

> A **contract clause block** is judged by an early structural pass, before any
> entry is recursively checked. The pass examines direct entries left to right
> and requires them to form an optional leading `doc`, then zero or more
> `let_stmt` nodes whose selected right-hand side is `ordinary_let_rhs`, then
> exactly one final `proposition`, and nothing else. The first entry that
> violates that shape is reported; an empty block, or one with no final
> `proposition`, instead reports the block node for its missing final
> proposition. A nonleading or repeated `doc`, a nonfinal or repeated
> `proposition`, a `propagate_let_rhs`, a `replace_let_rhs`, a `value_match`, a
> `value_if`, a `claim_stmt`, and every other direct statement shape are each a
> hard error citing the owning rule before any child semantic error can win.
> After the pass, the block's scope initially contains only the function
> parameters, named consts, and the function's type and const parameters; each
> let introduces a fresh clause-local `own` copy value visible only to later
> entries of the same block and never to the function body; every computation
> must be one ANF [GRAM-9] call to, or infix spelling of, a pure, total,
> non-trapping operation-table row; and the final `proposition` condition is
> either a Bool clause atom or one such operation returning Bool under [OP-5].
> User-function and system calls, construction, `move`, borrowing, subscripting,
> mutation, control flow, allocation, and every trapping or partial operation
> are rejected by the owning rule; a place is legal only as a non-consuming
> operand of an admitted table operation (for example `len(deref(out))`). Normal
> typing, ownership, the clause-local copy restriction, no-shadowing, FORM-3,
> and declaration-before-use rules apply after the pass succeeds. Clause locals
> are then recursively replaced by their unique defining right-hand sides until
> none remains, yielding one finite typed expression whose result is exact
> `own Bool`. The resulting template retains parameter ordinals and their
> written field and `deref` projections, named-const declaration identity and
> projections, typed literals with exact type and value, the selected
> operation-table row and written type and const arguments present at each node
> after [FN-2] substitution, result type, and written operand order; it excludes
> clause-local spellings, clause-local NodePaths, `doc` bytes, whether identical
> subexpressions were shared through one let, and callee-instance identity.
> [FN-9] uses this same judgment for an `ensures_block` and adds only its
> selector, relation-shape, and result-datum rules.

FN-8 then keeps only its direction-specific content: goal identity and the
no-normalization rule, the ordinary-call pre-transfer substitution and
disposition, the [PRV-2] ordering, S4, and the CLM-3 strict-view clauses. FN-9
keeps only its own: selector admission and freshness, the single-comparison
RelationTemplate, entry-image stability, selected exits and views, the SCC
schedule, the four S12 routes, and the failure-atomic batch.

**The new entry restriction**, stated in FN-8 where the deleted program-start
block sat:

> A `fn_decl` carrying a `requires_block` must not be the [FN-7] entry — the one
> top-level `fn_decl` named `main`, in either entry form. A `requires_block` on
> that declaration is a hard error citing FN-8 at the `requires_block` node,
> with the restructuring `state the requirement on the function the entry calls,
> and establish it in the entry body by a real branch`. This version therefore
> admits no dynamic requirement boundary of any kind, and every requirement is
> discharged statically by an ordinary source caller.

An `ensures_block` on the entry stays admitted and needs no rule: it never
executes, and its S12 publication simply has no caller for a kind-declaring
entry (uncallable under FN-7) and an ordinary one for the unlabelled form.

**[EFF-2]** keeps its sentence but loses its exception status: "An optional
`requires` block is a checked callable-boundary obligation [FN-8], and an
optional `ensures` block is a verified normal-return relation [FN-9]; neither is
an executed body occurrence, and neither contributes a read, write, allocation,
external, blocking, or trapping category." That was previously true-with-an-
exception; it becomes a theorem, because the grammar no longer contains a
construct inside a contract block that could execute.

**[EFF-4]**'s sentence "The retained program-start check [PROG-3] and any future
gated adapter check [GATE-1] belong to those dynamic boundaries, not to an
ordinary source call or the callee's exhibited row" is deleted (§9).

### 4.4 The three positions

The brief asked for the three positions. Under ruling 2 they resolve as:

| position | v0.32 | Proposal C1 |
|---|---|---|
| **ordinary source call** | static discharge of the instantiated goal in the caller's complete state at the pre-transfer point; no prologue, no fallback check | unchanged in every particular; only the final's spelling changes |
| **program entry** | compiler-owned wrapper evaluates the goal once after setup, before owner transfer; false → [OP-5]/[DIAG-3] trap, body invoked zero times; marked entries additionally discharge in U first | **inadmissible**: the entry may not carry a requirement. No wrapper, no goal evaluation, no entry trap record, no S4 at the entry body |
| **gated foreign boundary** | none exists; [GATE-1] states a forward constraint that a future adapter must validate the same FN-8 goal | none exists; [GATE-1] states a forward constraint that a future boundary must specify **its own** validated precondition (§9) |

### 4.5 The effect-row story

Unchanged in mechanism and simplified in justification. Neither block
contributes any category, and after the restriction that is structural rather
than stipulated. The one v0.32 oddity the restriction removes without comment is
worth naming: `fn8-trap-requires-false.wf` declares `fn main() -> own unit
pure` and traps at start. Under C1 that program does not compile, and no `pure`
declaration can trap through a contract. [EFF-2]'s remaining sentence "A
function whose body and release contribution are empty may therefore declare
`pure` while carrying a requirement" stays true and is now unambiguous.

---

## 5. [ENT-3.S4] in the end state

This is the load-bearing question and it has a better answer than v0.32's.

**Today.** S4 reads: "S4 is the admitted-body axiom justified by every ordinary
caller's static discharge **or** the successful dynamic boundary check [PROG-3,
GATE-1]; no callee-entry prologue executes." That is a disjunction whose second
arm is open-ended — it names a mechanism ([PROG-3]) plus a placeholder for a
mechanism that does not exist ([GATE-1]). The soundness of every requirement-
bearing body rests on a reader believing the disjunction is exhaustive.

**Under C1.** The disjunction is replaced by a *closed enumeration of invocation
routes*, each of which is either discharged or impossible by a cited rule. New
S4 justification sentence:

> S4 is the admitted-body axiom. Every invocation of a concrete function f
> carrying requirement goal G in an accepted program is one ordinary source
> `call` node of the same closed compilation unit [PROG-1, PROG-2], which FN-8
> admits only when f's instantiated G takes disposition `discharged` in the
> caller's complete fact state at the pre-transfer point. There is no second
> route: the [FN-7] entry may carry no requirement and is the sole program-start
> invocation [PROG-3]; [FN-4] law discharge binds only a function with neither
> `requires` nor `ensures` block; [CONST-2]'s `cvalue` admits no call form; and
> §14 admits no foreign import, export, inbound callback, or generated adapter
> in this version. Therefore G holds at every body entry of f. No callee-entry
> prologue executes and no dynamic boundary exists.

Each clause is a verified spec citation, not an assumption:

- ordinary calls — FN-8's own rule;
- program start — the new restriction plus FN-7's uniqueness of `main`;
- law discharge — FN-4 verbatim: "The bound function is nongeneric and has
  neither `requires` nor `ensures` block";
- const evaluation — CONST-2's `cvalue` production contains no `call`;
- foreign entry — GATE-1: "This version defines no callable FFI import, export,
  inbound callback, foreign-thread entry, or generated foreign adapter."

**This is strictly stronger than v0.32's ground**, for two reasons. It is a
closed enumeration rather than an open disjunction, so a future amendment that
adds an invocation route must *notice* that it has to extend the enumeration.
And every premise is machine-checkable in the compiler as it stands: route (i)
is already the FN-8 call judgment, and route (ii) becomes a one-predicate
declaration check.

**What moves with it.** The B (S4-blinded) view is unchanged in definition — B
is still U with the positive S4 goal and its L0 projection omitted at body entry
— but its *purpose* narrows honestly. Today B exists partly to stop a
`command` entry's runtime wrapper from laundering an unconditionally external
input into a protected leaf ([PRV-3]: "neither that runtime check nor the S4
axiom can launder an external protected leaf"). With no entry requirement, an
entry has no S4 at all, so [PRV-3]'s entire `command`-entry paragraph collapses
to the general rule: an entry-local protected leaf whose subject carries the
unconditional-external bit must discharge in B, and since the entry has no S4,
B equals U there — exactly what FN-8 already says elsewhere ("A function with no
S4 requirement cannot distinguish U from B"). **The restriction deletes a
special case rather than adding one.** That is the strongest technical argument
in favour of ruling 2 and I want it on the record independently of my proposal.

---

## 6. The now-unreachable program-start machinery

Delete, reserve, or leave dormant. My answer: **delete the mechanism; keep one
forward-constraint sentence in [GATE-1].**

### 6.1 What becomes unreachable

Enumerated against v0.32 bytes:

| site | content | sentences |
|---|---|---|
| [FN-8] | the program-start block (`Program start is the one implemented dynamic boundary…` through `…contributes no source effect [EFF-2]`) plus the two CLM-3 marked-entry sentences | **13** |
| [PROG-3] | the wrapper paragraph and the marked-entry static-query paragraph | ~13 of 14 (the sentence "A source call to the unlabelled entry is not program start and instead follows [FN-8]'s ordinary static discharge" survives and becomes an anchor of the §5 enumeration) |
| [OP-5] | the requires-final dynamic clause and the ensures-final clause | 2 (rewritten, §4.3) |
| [DIAG-3] | the `node_path` clause naming "the final `check_stmt` whose complete goal fails at program start"; the whole sentence "For an [FN-8] program-start goal, `rule_id` is `OP-5` and `message` is the final `check_stmt`'s STRING value decoded by [FORM-5]" | 2 |
| [DIAG-2] | "the one retained program-start goal evaluation when the entry has a requirement"; "an executable retained check exists only for program start [PROG-3] and a later implemented gated boundary [GATE-1]" | 2 |
| [CLM-3] | "A marked program entry follows [PROG-3] and must discharge its requirement in the post-setup, pre-wrapper-check U state…" | 1 |
| [PRV-3] | the `command`-entry paragraph | 6 |
| [EFF-4] | "The retained program-start check [PROG-3] and any future gated adapter check [GATE-1] belong to those dynamic boundaries…" | 1 |
| [GATE-1] | the four-sentence foreign-adapter clause requiring a future adapter to validate "the same concrete complete goal" | 4 |
| [ENT-1] | the clause "…or checked at a dynamic boundary before S4 admits it to a body…" in the [SCOPE-2] fact-source enumeration | 1 (clause edit) |
| [ENT-3.S4] | the justification sentence | 1 (rewritten, §5) |

Roughly **46 sentences across 11 rules**, of which 13 are FN-8's. Compiler side:
`IrEntryGoal` and `IrEntryGoalDefinition` in `compiler/src/lowering.rs`
(~lines 966–1023) plus their construction and emission, and the assertion at
`compiler/src/lowering/tests.rs:748` ("a required entry must retain one wrapper
goal").

`GATE-1` is cited from four rules outside its own section: FN-8 (line 1272),
EFF-4 (1482), DIAG-2 (1963), ENT-3.S4 (2830). Three of those four citations
disappear with the deletions above; the fourth is rewritten in §5.

### 6.2 The argument

**Against "leave dormant."** Unreachable normative text is the worst option
available. No test can reach it, no reviewer can falsify it, and it reads as law.
The repository's standing rule is supersede in place; dormant rules are the spec
equivalent of an abandoned experiment left beside its replacement.

**Against "reserve for the FFI work."** Tempting, because [PROG-3]'s wrapper
paragraph is a genuinely careful piece of design — sole ownership during
evaluation, non-consuming reads only, no helper that accepts a source owner,
exactly-once evaluation, zero body invocations on failure. It would be a shame
to retype it. But reserving it means moving ~13 sentences of *mechanism* into
§14, where nothing can exercise them, in service of a boundary this version does
not have. That is precisely the "exhaustive protocol machinery … unless a current
experiment directly needs them" the project rules forbid, and §14 is already the
place the project keeps a *stub* rather than a mechanism ([CAP-1] reserves two
predicate names and nothing else; [GATE-1] reserves a family and nothing else).
There is also a design reason not to preserve it: the wrapper rule is shaped for
*one* instance, the process entry with [FN-7]'s closed standard-input table. A
general foreign boundary has an argument-validation step, an ABI, an ownership
transfer protocol, and an error channel the entry does not have. A rule shaped
for the entry would need rewriting to generalize; preserving it buys the
appearance of readiness, not readiness.

**For "delete the mechanism, keep the obligation."** One sentence added to
[GATE-1], replacing its four-sentence adapter clause:

> This version admits no untrusted callable boundary into source code. A later
> amendment that admits one must specify that boundary's own validated
> precondition, the exact point at which it is evaluated relative to argument
> validation and owner transfer, and its trap record; no clause of §8 supplies
> them, and a source `requires` block is a static caller obligation that never
> stands in for such a validation.

That is not machinery. It is a closed-world statement plus a forward constraint —
exactly what §14 already is — and it preserves the one thing worth preserving
from the 2026-07-11 rationale in the mcts tree: that a caller-proof scheme leaves
foreign entries unprotected, so a foreign entry must bring its own check. The
mechanism is re-derived when there is a real boundary to derive it against,
against that boundary's real requirements. Deleting ~46 sentences and adding one
is a net legibility win of the kind the repository asks for.

The mcts consequence, for whoever lands this: `requirement-enforcement`'s third
and fourth bullets ("Each real process entry evaluates the complete requirement
exactly once in its compiler-owned wrapper…") describe a mechanism this change
removes, so the change is a paired move under the `mcts-mem-use` rules — a new
node replacing `requirement-enforcement`, with the current node moved to
`.alt/`, not an in-place edit.

---

## 7. Migration recipe, with measured cost

Ordered so the gate is green at each step.

**Step 1 — the entry restriction alone (no grammar change).**
Add the FN-8 restriction over `main`; delete the machinery in §6.1; rewrite S4
per §5; delete `IrEntryGoal` and the wrapper lowering. Corpus cost: the three
entry cases in §0. Verification: `make -C compiler check`, then `make check`.

**Step 2 — the terminal.** Parser and AST: `check_stmt` → `proposition`;
regenerate the syntax data; run the native grammar verifier that reuses the
compiler's own lexer and parser, as `docs/WORKFLOW.md` requires before any
grammar proposal. Corpus cost: **104 finals** across **90 `.wf` files**, plus
**248 inline fixtures** across **17** Rust files under `compiler/`. The rewrite is
mechanical:

```
  check E else trap "M";      →      holds E;
```

and, where the message is worth keeping as prose, the leading `doc` slot now
admits it:

```
  check admitted else trap "append filled exceeds destination";
→ doc "append filled exceeds destination";
  holds admitted;
```

I recommend the plain form by default and the `doc` form only where a writer
judges the prose useful, because the 104 existing strings are written in
trap-message register ("output too short"), not documentation register, and
mass-promoting them to `doc` would produce 104 poor docs. Dropping them loses
nothing machine-checked: the STRING has no compile-time identity in either rule
and, after step 1, no runtime consumer.

**Step 3 — state the shared pass once.** FN-8 gains the shared clause-block
judgment; FN-9 cites it and keeps only its direction-specific rules. No corpus
byte changes. This step is separable from steps 1 and 2 and is, in my view, the
highest-value part of the whole proposal (§9.2).

**Cost by approval class.**

| class | count | note |
|---|---|---|
| protected conformance evidence | **44 case files** rewritten (spelling only) + **3 cases** whose subject is deleted + their `manifest.jsonl` rows | every one needs an exact before/after audit, owner explanation and approval, and an approval-ledger entry |
| specification bytes | ~46 sentences deleted, ~6 rewritten, 1 added, 2 grammar fences | one exact-byte owner approval packet with complete SHA-256, diff, impact inventory, and verifier results |
| ordinary tests and code | 46 non-conformance `.wf` files, 17 Rust files, parser/AST/generated data, lowering | batch autonomy |

The three subject-losing cases, with dispositions:

- `fn8-trap-requires-false.wf` — verdict `{"kind": "trap"}`, doc "A false
  requirement on the real process entry traps in the entry wrapper". Its subject
  ceases to exist. Convert to a rejection case for the new restriction
  (`expect: reject, rule: FN-8`) preserving the same source shape, so the case
  keeps testing the entry/requirement interaction in its new form rather than
  being deleted outright.
- `clm3-neg-generated-wrapper-check.wf` — its subject is the generated wrapper
  check and whether it can authorize itself. With no wrapper, retire it and
  record the retirement in the audit; there is no successor behaviour to test.
- `clm3-pos-transitive-value-branch.wf` — the entry requirement is incidental
  (`check valid_entry else trap "entry relation";` over `ieq(0_u64, 0_u64)`);
  the case is about transitive value-branch discharge. Delete the requirement
  block; the case's subject is untouched.

Deleting or repurposing protected conformance evidence is the highest-risk part
of this proposal and must be presented as such: two of these three cases exist
*because* an adversarial review once found a hole in the requirement path
(mcts, 2026-07-11: "a doc-only `requires` clause vanished before validation").
Retiring such a case is retiring a regression barrier, and the packet should say
so in those words.

---

## 8. Options compared

| | scope | corpus cost | protected cost | satisfies ruling 1 |
|---|---|---|---|---|
| **Option 0** — delete the entry path only; grammar untouched | §6.1 + S4 | 3 files | 3 cases | **yes, fully** |
| **Option C1** — Option 0 + `holds` + shared pass + leading `doc` | + GRAM-2/4, FORM-2/5, OP-5 | 90 `.wf` + 17 `.rs` | 44 + 3 cases | yes |
| **Option C2** — the assigned stance: a separate entry-boundary construct | + a new block production admitted only at the entry | 3 files | 3 cases | yes, by exception |

**Option 0 is the honest minimum and it fully satisfies the owner's stated
requirement.** After it, no contract generates runtime code, in any position,
with no exception. What it does *not* do is address what the owner actually
complained about — that the surface says `else trap "msg"` when nothing traps.
Option 0 makes that spelling inert at 104/104 sites instead of 101/104, i.e. it
makes the complaint worse in kind while satisfying the requirement in letter.

**Option C1 buys the surface honesty at 87 additional files and 41 additional
protected-conformance approvals.** Whether that is worth it is an owner call,
not mine. My recommendation is C1 with step 3 taken first if the budget is
tight, because step 3 costs no corpus byte at all.

**Option C2 is dead** under ruling 2, and §9.1 says what I think of having
proposed it.

---

## 9. What this costs

Unsoftened, as required.

### 9.1 My assigned stance was wrong, and the evidence — not the ruling — is what killed it

I was assigned "stop making them share a form," and after reading FN-8 and FN-9
in full I do not believe it. They share a form because they *are* the same form:
one structural pass, one admitted computation subset, one alpha expansion, one
identity discipline. The parts that differ (who owes it, where, over what, into
which channel) are already in separate rules and already spelled differently in
the source — `requires` versus `ensures ensures_selector`. There was no
positional ambiguity to remove except the entry one, and the entry one is now
out of scope. Had ruling 2 not arrived, I would still have had to abandon the
headline move, because my own §3.1 evidence points the other way.

The construct I was going to propose would have had **three instances in a
637-file corpus**, all of them test cases, and would have existed to serve a
foreign-call path this version does not have. I would have shipped a keyword for
three conformance cases. The project rules name that failure exactly: "Is it
exercising a real compiler path or inventing machinery for a hypothetical one?"

### 9.2 Stripped of the entry move, two-thirds of my proposal is editorial

What remains is: rename one terminal, delete dead rule text, state a shared pass
once. Two of those three change no program behaviour whatsoever, and the third
changes 104 lines of source that mean exactly what they meant before. A reviewer
is entitled to ask whether rewriting **44 protected conformance case files** —
the highest approval class this project has, each needing an exact before/after
audit and a ledger entry — is a proportionate price for deleting eight inert
bytes per site.

I do not have a fully satisfying answer. My best one is that the inert bytes are
not the real purchase: the real purchase is that FN-8 and FN-9 stop stating the
same structural pass twice, and the three drifts in §3.1 stop accumulating. But
that purchase is available in **step 3 alone, at zero corpus cost**, and I should
say plainly that a reviewer who takes step 3 and rejects step 2 has taken the
better-value half of my proposal and left the expensive half.

### 9.3 Writers will not forget anything, because there is nothing to remember — and that is the one place the restriction can bite

The brief asked whether writers would forget the entry check and lose protection.
Under C1 they cannot forget it: the restriction is a hard error at the
`requires_block` node, not an omission. But the protection *is* genuinely lost,
and I should not hide behind the error message. Today a writer can state a
precondition on `main` over `command.args` and have it checked before the body
runs. After C1 they cannot; they must call a helper that carries the requirement
and establish it in the entry body by a real branch — which means writing the
branch by hand, which means the check is now *their* code and not the
compiler's. For the current corpus that costs nothing (the only `command` entry
requirements are `ieq(0_u64, 0_u64)` and a constant conjunction, both in test
cases). For a future program that wants to validate its arguments at startup, it
costs a hand-written branch. That is a real capability regression, it is
temporary only if the FFI work actually happens, and the owner should hear it as
a regression rather than as a simplification.

### 9.4 "One kind, two directions" may be a distinction I invented to keep my paper

I claimed the shared part is "the entire writer-facing surface." Counting
honestly, the shared part is: a brace block, `let*`, one final Boolean, and the
rule that clause locals expand away. That is small. Everything a writer actually
reasons about — may I name the result, may I use `bor`, will this be checked at
my call site or at my returns, will a kill make my parameter unavailable — is on
the *unshared* side. A reviewer could fairly say I have promoted a lexical
similarity into a thesis in order to have something to propose after my assigned
move was cut. I think that reviewer is half right. My defence is that the
duplication is real and has already produced three documented drifts; my
concession is that "one kind of thing" is a stronger claim than my evidence
supports, and "one shared structural pass, two unrelated obligations" is what I
can actually defend.

### 9.5 The strongest rival argument against me

It is Proposal D's, and I cannot refute it.

**`check E else trap "M";` is one spelling with one meaning: this proposition
must hold, and here is what to say if it does not.** Whether anything currently
says it is a fact about this version's implemented boundaries, not a fact about
the construct. Read that way, `else trap "M"` is not *inert* — it is
*unexercised*, and this specification deliberately keeps unexercised surfaces
all over: `service` and `embedded` are reserved spellings in the [FN-7] kind
table with no form defined; [CAP-1] reserves `Shareable` and `Sendable` for a
concurrency layer that does not exist; [GATE-1] reserves an entire boundary
family with no member. The project has a settled, deliberate habit of reserving
vocabulary for a boundary it has not built yet.

My proposal breaks that habit for the *one* construct whose boundary is already
specified ([PROG-3]) and already implemented (`IrEntryGoal` exists and lowers) —
and pays 44 protected-conformance approvals to do it. If the FFI specification
the owner named in ruling 1 lands in six months, the boundary comes back, it
needs a message and a trap record, and every one of those 104 lines is a
candidate for rewriting again. Under that reading, ruling 2 is a *scheduling*
decision (entry is out of scope for now) and I have converted it into a
*language* decision (the vocabulary is deleted), which is a category error.

My counter is thin and I will state it as thin: a future foreign boundary should
declare its *own* validated precondition with its own message, because its
untrusted caller, its ABI, and its error channel are its own — so the return is
not symmetric and those 104 lines would not come back. I believe that, but I
cannot prove it from anything in v0.32, and the mcts record contains a decision
(2026-07-11, callee-boundary coverage over caller-proof, "the direct-C entry
path showed entry enforcement is necessary") that leans the other way. A
reviewer who weights that record heavily should prefer Option 0, or prefer doing
nothing at all until the FFI work forces the question.

---

## 10. Summary

- `requires` and `ensures` share one structural pass, one admitted computation
  subset, one alpha expansion, and one identity discipline — stated twice in the
  spec, with three documented drifts. They do not share who owes the proof, the
  discharge point, the admitted predicate language, the value universe, or the
  fact channel. **One shared form, two directions.**
- The defect is not the shared form. It is that the shared form is spelled as an
  executable action inherited from a body construct v0.32 already deleted.
- **Proposal C1**: `proposition := "holds" expr ";"` replaces `check_stmt` in
  both blocks; `check_stmt` is deleted from the grammar; one leading `doc` is
  admitted; the shared clause-block judgment is stated once in FN-8 and cited by
  FN-9; a `requires_block` is inadmissible on the [FN-7] entry; ~46 sentences of
  program-start machinery are deleted and replaced by one forward-constraint
  sentence in [GATE-1].
- **[ENT-3.S4]** stops being an open disjunction and becomes a closed
  enumeration of invocation routes — ordinary call (discharged), program start
  (restricted), FN-4 law discharge (excluded by FN-4), const evaluation (no call
  form), foreign entry (none in §14) — which is strictly stronger than what it
  replaces and machine-checkable as it stands.
- **Measured cost**: 104 finals across 90 `.wf` files and 248 fixtures in 17
  Rust files; 44 protected conformance cases rewritten and 3 whose subject is
  deleted; ~46 spec sentences.
- **The cheapest path to the owner's actual requirement is Option 0** — delete
  the entry path and change no grammar — at 3 files. It satisfies ruling 1 in
  full and addresses the owner's complaint not at all.
