# Proposal B — contracts as named, justified obligations

Design study of the `requires`/`ensures` contract terminal. Research only: this
document changes no specification byte, no compiler behavior, and no test.
Written against `spec/kernel-spec.md` v0.32 at `d32d7dd0` (branch `batch-0072`).

Stance assigned: the contract terminal becomes a *named proposition with a
mandatory justification*, sharing `claim`'s identity discipline — unique name,
retained justification, refutation as a hard error, accountability projection —
while differing in execution.

## 0. Verdict first

Two owner rulings arrived while this was being written, and together they remove
this stance's stated ground:

1. A contract must generate no runtime code. Requirement, not preference.
2. The program entry is out of scope; a declaration carrying a `requires` will
   be required to be an internal function.

The brief's ground for aligning contracts with `claim` was that a `requires` at
a real program entry *is* a writer-stated trap, and therefore deserves the
batch-0071 accountability package. After ruling 2 there is no executed contract
anywhere in the language. The ground is gone, not weakened.

I therefore report the honest result rather than defend the stance. Of the five
elements of `claim`'s accountability package, **two survive on compile-time
grounds and three do not**:

| element | survives? | why |
|---|---|---|
| name | **no** (conditionally yes later) | identity is already complete without it; at most one proposition per block makes the uniqueness rule vacuous |
| mandatory justification | **no** | a justification is the receipt for an *unpaid proof debt*; a contract is proved at every call, so no debt exists |
| refutability | **yes**, declaration-site form only | a compile-time judgment about the proposition itself; independent of execution |
| redundancy advisory | **yes**, declaration-local form | same; catches a signature that promises to constrain and does not |
| accountability projection | **no** | already served better by the retained derivation DAG; merging would degrade it |

The one-sentence finding: **the accountability that survives execution's
departure is compile-time lifecycle judgment — refutation and redundancy — not
identity (a name) and not narrative (a justification).**

Section 7 leaves the strongest defensible remnant on the table as a concrete
rival for the adversarial round. Section 8 attacks it. Section 9 records two
corrections the study needs regardless of which proposal wins.

## 1. What the spec actually says (verified, not recalled)

Every claim below was read at `spec/kernel-spec.md` v0.32.

**The terminal.** `check_stmt := "check" expr "else" "trap" STRING ";"` (GRAM-4,
line 220). Batch-0071 removed it from the `stmt` alternation; its only two
remaining referents are `requires_entry` and `ensures_entry` (GRAM-2, lines
166/169). It is a contract-only production that still carries statement syntax.

**In `ensures` it is wholly inert.** FN-9: "The final `check` is proposition
syntax only: its message, clause-local spelling, and sharing have no identity,
it contributes no `traps`, it never executes, and it emits no [DIAG-3] record."

**In `requires` at an ordinary call it is a caller obligation.** FN-8: the
instantiated goal takes "`discharged`, `refuted`, or `unproved`"; "`refuted` or
`unproved` is the [DIAG-1] FN-8 call-site rejection"; "There is no executable
ordinary-callee prologue." The callee body then receives the goal as ENT-3
source S4.

**In `requires` at a program entry it executes.** PROG-3: the compiler-owned
wrapper evaluates the goal once; "A false result emits the final `check_stmt`'s
exact [OP-5, DIAG-3] trap record, invokes the body zero times". DIAG-3: "For an
[FN-8] program-start goal, `rule_id` is `OP-5` and `message` is the final
`check_stmt`'s STRING value decoded by [FORM-5]." This is the correction the
brief supplied, and it is exact.

**A claim's justification is already not runtime data.** CLM-1: the `because`
STRING is "mandatory compile-time review data retained by the checked program
[DIAG-2], absent from runtime behavior". DIAG-3 is explicit: "For a [CLM-1]
claim, `rule_id` is `CLM-1` and `message` is the claim's exact IDENT spelling;
the justification STRING is compile-time data and does not appear in the
record." So the operator sees the *name*; the reviewer reads the *justification*.
This matters in §6.2 and corrects a natural but wrong intuition about what a
justification is for.

**Claim-name uniqueness has a job because claims are plural.** CLM-1: "Within
one `fn_decl` every claim name is unique". A function may hold many claims and
DIAG-3 must say which one fired. GRAM-2 admits `requires_block?` and
`ensures_block?` — at most one each — and FN-8/FN-9 each mandate "exactly one
final `check_stmt`". Contracts are singletons.

**Contract identity is already complete without a name.** DIAG-2 fixes the
requirement occurrence as `(concrete callee instance, final-check NodePath,
conjunct ordinal 0)`; FN-9 fixes the postcondition occurrence identity as
`(concrete function instance, ensures_block NodePath, 0)`. Both are NodePath
tuples. DIAG-1's entire identity apparatus is coordinate- and NodePath-keyed by
law.

**`deny_claims` already sees requirements, correctly.** CLM-3: a demanded
component succeeds "exactly when its `MayClaims` set is empty, every protected
obligation ... discharges in its owning function's existing unasserted U state
..., **every ordinary user-call requirement owned by the component discharges at
that call in caller U** [FN-8], and every strictly outgoing demanded callee
component has a successful strict summary."

**The R3-PROVISIONAL register flags exactly this spelling** (line 8): "the
`requires { requires_entry* }` surface spelling with its FN-8-checked
ordinary-let/final-check subset (FN-8 — semantics selected, spelling not yet
compared)."

**The batch-0071 decision this stance must compose with.**
[mcts_mem/whitefoot/checks-and-proofs/obligation-discharge/writer-trap-surface.md](https://github.com/mbbill/Whitefoot/blob/0f22b026b062c36ef2f8ce9c725ee4307e1fda64/mcts_mem/whitefoot/checks-and-proofs/obligation-discharge/writer-trap-surface.md):
"One writer-stated trap construct exists: the named, justification-bearing
claim." Its second bullet is load-bearing here: "The retained condition-judgment
and program-start trap semantics stay owned by the rule the retired body
statement carried, **and the contract final keeps that spelling**." The frozen
alternative `dual-check-and-claim.alt` was replaced with the reason: "an
anonymous body check is a claim minus its name, justification, accountability
entry, redundancy advice, and refutation, so keeping both left a strictly weaker
duplicate through which a writer could assert without accountability."

That reason is the exact hinge of this study, and §6 shows it does not transfer.

## 2. Corpus measurement

Whole tree at `d32d7dd0`, excluding `archive/` (no active source may depend on
it) and `.git/`.

| quantity | count |
|---|---|
| `.wf` sources | 637 |
| `requires` blocks | 87, in 82 files |
| `requires` blocks with a final `check` | 85 |
| `requires` blocks *without* one | 2 (`fn8-neg-requires-no-check`, `fn8-neg-doc-only-clause` — negative cases testing the missing-final rejection) |
| `ensures` blocks | 19, in 14 files |
| contract finals, total | 104 |
| files carrying at least one contract | 90 |
| `claim` statements (the construct this stance would align with) | 635 |
| stale body `check` statements retired by v0.32 and still present in research files | 13 |

**Message bytes.** 104 message strings, 1901 bytes total, mean 18.3 bytes.

**The `requires` population is far smaller than 85 distinct decisions.** 44
distinct final lines and 39 distinct messages; a single line —
`check ile<u64>(src_len, covered) else trap "output capacity";` — accounts for
34 of the 85, because `tests/codegen/cases/bounds/output-capacity-lockstep` holds
44 near-identical base64 fixtures. Real design pressure comes from roughly 40
sites, not 85.

**The `ensures` messages are already dead ceremony, measurably.** 15 of 19
contain the literal word `postcondition`; twelve are exactly `postcondition`,
`bounded postcondition`, `relay postcondition`, `selected postcondition`,
`control postcondition`, `first postcondition`, `second postcondition`,
`identity postcondition`, or `choose postcondition`. Writers handed an inert
required string filled it with the name of the slot. Only four say anything
(`read bits result exceeds mask`, `append result exceeds destination` ×2,
`relay result`). This is the study's single most useful empirical datum and
§8 turns it against this proposal.

**Entries carrying a requirement: 3, all conformance cases.**

- `tests/conformance/cases/fn8-trap-requires-false.wf:1` — `fn main() -> own unit pure requires {`
- `tests/conformance/cases/clm3-pos-transitive-value-branch.wf:36` — `deny_claims command fn main() -> own ExitStatus pure requires {`
- `tests/conformance/cases/clm3-neg-generated-wrapper-check.wf:1` — same shape

The tree holds **529** entry declarations, broken down exactly:

| form | count |
|---|---|
| `fn main` | 451 |
| `deny_claims fn main` | 2 |
| `command fn main` | 74 |
| `deny_claims command fn main` | 2 |

All three `requires`-bearing entries are protected conformance evidence, and one
of them (`fn8-trap-requires-false`, expectation `{"kind":"trap"}`) exists solely
to prove the entry wrapper traps. §9.2 records what that costs, and §9.3 records
why the count kept coming out wrong.

**Conformance surface.** 490 manifest cases; 22 name FN-8, FN-9, or OP-5 in
their `rules`. `tests/conformance/runner.py` verifies a trap expectation as
`{"kind":"trap"}` only — it asserts no record bytes — so changing the DIAG-3
message would not by itself flip a conformance verdict, though the compiler's
focused trap tests assert bytes and would.

**Compiler surface.** 13 Rust files reference `CheckStmt`/`check_stmt`, with the
weight in `syntax/grammar/generated.rs` (generated tables), `syntax/parser`,
`semantic/check/requires.rs`, `semantic/check/control.rs`, and
`resolution/engine/admission.rs`.

## 3. The design, as briefed

Presented complete so the study has a real artifact to compare, then tested in
§6 and attacked in §8.

### 3.1 Grammar

`GRAM-4` loses `check_stmt` entirely — after v0.32 it has no other referent, so
this deletes the production rather than orphaning it. `GRAM-2` gains
`contract_final` immediately after `ensures_selector`:

```wf-ebnf GRAM-2
requires_block:= "requires" "{" requires_entry* "}"
requires_entry:= doc | stmt | contract_final
ensures_block := "ensures" ensures_selector "{" ensures_entry* "}"
ensures_selector:= IDENT | TYPEID "(" fieldbind_list? ")"
ensures_entry := doc | stmt | contract_final
contract_final:= "holds" IDENT ":" expr "because" STRING ";"
```

Written form:

```
fn append(dst: &uniq 'd Buffer, src: &'s slice<'s, u8>) -> own u64 pure requires {
  let covered = imul(4_u64, quarter);
  holds output_capacity: ile<u64>(src_len, covered)
    because "the caller sizes the destination from the lockstep 3:4 relation";
} ensures result {
  holds result_within_capacity: ile(result, capacity)
    because "the append never writes past the space the caller proved";
} {
```

(Rendered on one line under FORM-2; broken here only for the page.)

**Keyword defense.** The owner's objection to reusing `claim` is accepted, not
defeated. `claim` is spelled as an *act performed at a program point*: it has a
control-flow NodePath, it establishes a dominated fact on its normal
continuation (CLM-1), and it fires. A contract proposition is not at a point; it
is a property of the declaration's signature. FORM-1 admits exactly one spelling
per semantic construct, so one word cannot mean both "assert here and pay at
runtime" and "this is the signature's precondition" — especially when, as §5
shows, the two must be treated *oppositely* by `deny_claims`.

`holds` is the copula of a proposition rather than a verb of action: it names
what the block *is*, not what the writer *does*. It reads in both blocks. It is
one new FORM-3 fixed lowercase atom, and it costs nothing: all 26 occurrences of
`holds` and all 45 of `must` in the corpus are inside STRING literals, none is
an identifier.

**LL(2).** `holds` is a fixed terminal ineligible for IDENT under FORM-3, so it
cannot collide with `expr_stmt`'s IDENT-headed `call` start, and every `stmt`
arm begins with a distinct fixed terminal. The `requires_entry` decision stays
one-token, exactly as `check` was. Arm order `doc | stmt | contract_final` is
preserved so DIAG-1's arm ranking is unchanged; the production's *definition*
rank moves from GRAM-4 to GRAM-2 and must be re-derived by the native grammar
verifier before any such change is proposed.

### 3.2 FORM-2, FORM-3, FORM-5

- FORM-2's line-bearing simple productions list replaces `check_stmt` with
  `contract_final`. Rendering is byte-shaped exactly like `claim_stmt`, whose
  attachment behavior for `:` and `"` is already fixed and tested. No new
  formatting machinery.
- FORM-3: `holds` becomes an exact fixed lowercase grammar atom. `check` and
  bare `trap` cease to be grammar atoms. `trap` stays reserved through FORM-3's
  OPNAME mode-word set `(wrap|trap|checked|sat|strict)`, so only `check` is
  freed as an IDENT spelling — a widening, monotone-safe. **No reservation
  ordinal shifts**: DIAG-1's reservation inventory is OP-1 operation names plus
  the five mode words, and `check` was never a member.
- FORM-5's STRING home list becomes "`doc` entries, `claim` justifications, and
  `contract_final` justifications".

### 3.3 OP-5

OP-5 stops being a statement rule and becomes what all five of its clients
already use it as — the Bool condition judgment. Its condition text and its
citation identity are preserved verbatim so the 22 contract-touching conformance
verdicts (notably `fn8-neg-requires-non-bool-check`, expecting
`{"kind":"reject","rule":"OP-5"}`) do not move:

> [OP-5] The Bool condition judgment; this rule owns no syntax of its own.
> A construct judged under this rule requires its selected `expr` to have exact
> value mode and type `own Bool`, where `Bool` is the PRE-1 nominal type.
> *(no integer / other enum / borrowed Bool / truthiness paragraph unchanged)*
> *(TYPE-7 exclusivity paragraph unchanged)*
> Every other exact-mode or exact-type failure is a hard error citing OP-5 at
> the selected `expr` node, with `SourceCoordinate` equal to that expression
> node's complete checked half-open source extent.
> The `contract_final` of a `requires` block uses this condition judgment;
> [FN-8] owns its identity and its dynamic-boundary behavior.
> The `contract_final` of an `ensures` block uses the same condition judgment;
> [FN-9] owns it as a proof obligation, and it never executes.

The two sentences deleted are the ones naming `check_stmt` and its "decoded
message".

### 3.4 FN-8

Structural pass, verbatim shape preserved with the production renamed:

> Before recursively checking any entry, an early FN-8 structural pass requires
> those selected children to form zero or more `let_stmt` nodes whose selected
> right-hand side is `ordinary_let_rhs`, followed by exactly one final
> `contract_final`, and nothing else. ... an empty block or an all-let sequence
> instead reports the `requires_block` node for its missing final proposition.
> Thus a nonfinal or repeated `contract_final`, a `doc`, a `propagate_let_rhs`,
> a `value_match`, a `value_if`, or any other direct statement shape is a hard
> error citing FN-8 before any child semantic error can win.

Four added clauses:

**(a) Name.** The proposition name is one IDENT and is not a declaration: it
enters no TYPE-6 domain, no OP-1 reservation inventory, and no lexical lookup,
and no source construct references it; its DIAG-1 carrier classification is the
contract-name carrier. Within one `fn_decl`, the `requires` proposition name,
the `ensures` proposition name, and every claim name are drawn from **one shared
uniqueness domain**; a repeated spelling is a hard error citing the later
carrier's owning rule.

The shared domain is required, not stylistic: otherwise DIAG-3's `message` field
becomes ambiguous between a fired claim and a fired entry proposition, and a
reviewer reading an accountability list sees two `bound` entries meaning
different things. (Note for §6.1: this is the *only* work the uniqueness rule
does, and it exists only because clause (d) puts the name into DIAG-3.)

**(b) Justification.** The `because` STRING is the proposition's justification:
mandatory compile-time review data retained by the checked program [DIAG-2],
absent from runtime behavior, and never semantics-selecting.

**(c) Identity is unchanged.** FN-8's expansion clause gains: "Clause-local
spellings, clause-local NodePaths, whether identical subexpressions were shared
through one let, **the proposition name, and the justification** are absent after
expansion." This is mandatory. If a name entered goal equality, renaming a
proposition could flip a call from `discharged` to `unproved` — a
semantics-selecting name, which FORM-1 and CLM-1's own "never semantics-selecting"
clause both forbid. The requirement occurrence stays
`(concrete callee instance, contract_final NodePath, conjunct ordinal 0)`; the
name is **not** a component.

**(d) Boundary behavior.** "A false result has the `contract_final`'s
[OP-5, DIAG-3] failure behavior and invokes the body zero times."

### 3.5 FN-9

Same structural-pass rename. The inertness sentence becomes:

> The `contract_final` names one proposition. Its name and its justification are
> compile-time review data with no template identity, it contributes no `traps`,
> it never executes, and it emits no [DIAG-3] record.

The RelationTemplate exclusion list gains name and justification beside "binder
spelling, let spelling or sharing, message bytes".

Observe what just happened, and §8 will not soften it: the inertness clause did
not shrink. `else trap "msg"` left; a name and a justification arrived; both are
equally inert here. **The design replaces one dead limb with two.**

### 3.6 CLM-1, CLM-2, CLM-3

- **CLM-1** gains the shared-uniqueness-domain sentence. Nothing else changes.
- **CLM-2** gains a contract-proposition lifecycle paragraph — the declaration-
  site refutation and redundancy judgments specified in §7.2. These are the two
  elements that survive §6.
- **CLM-3** is unchanged, and §5 is the argument for why.

### 3.7 DIAG-1, DIAG-2, DIAG-3

**DIAG-1** gains one carrier, mirroring the claim-name carrier verbatim:

> The contract-name carrier is exactly the IDENT of a `contract_final`
> [FN-8, FN-9]. It produces one record for the shared per-function
> proposition-name uniqueness judgment; it produces no declaration, lexical-use,
> dependent-declaration, deferred-use, or table-checked record, enters and
> queries no lexical name domain, and does not participate in FORM-3's
> reservation inventory.

DIAG-1's row-4 attribution list already names `requires_entry*`,
`requires_entry`, `ensures_entry*`, `ensures_entry`; unchanged. The one other
textual site is line 1928, `... uses the final "check_stmt" condition's "expr"`
→ `contract_final`.

**DIAG-2** replaces "The final check inside a `requires` block is not an
ordinary-callee check" with the `contract_final` spelling, and adds: the checked
program retains each contract proposition's name and justification beside the
GoalTemplate, exactly as it retains a claim's name, predicate, and justification.

**DIAG-3** — the entry record (temporary exception under ruling 1; **removed
entirely under ruling 2**): `rule_id` stays `OP-5`; `message` becomes the
proposition's exact IDENT spelling, precisely parallel to CLM-1; the
justification is compile-time data and does not appear.

Consequence, stated plainly: `fn8-trap-requires-false.wf` would emit
`"message":"positive_input"` where it now emits `"message":"x must be positive"`.
An operator reading a trap gets an identifier instead of a sentence. That is a
**regression in operator legibility** bought with a gain in reviewer
traceability — and under ruling 2 the operator side does not exist at all, so
this design trades away the half that is real for the half that never will be.

### 3.8 Effect row

**Nothing changes, and that is the point.** EFF-2 already reads: "An optional
`requires` block is a checked callable-boundary obligation [FN-8], and an
optional `ensures` block is a verified normal-return relation [FN-9]; neither is
an executed body occurrence, and neither contributes a read, write, allocation,
external, blocking, or trapping category."

Ruling 1 ("a contract must generate no runtime code") is, for `ensures` and for
ordinary `requires`, already law in this sentence. The only thing either ruling
adds is the disposal of the entry exception.

One hazard this design creates and cannot fully close: `claim_stmt`
"syntactically exhibits `traps`" (CLM-1), and `holds name: e because "text";`
reads exactly like a claim. A future editor generalizing "every named justified
proposition exhibits `traps`" would silently give every contract-bearing function
a trapping effect row. That hazard is created purely by the cosmetic alignment
this stance was asked to build, and it is one more reason §6 concludes against it.

## 4. Behavior in all three positions

| position | executes? | judgment | diagnostic on failure | name used for | justification used for |
|---|---|---|---|---|---|
| ordinary call | no | caller must prove the instantiated goal in the complete state, pre-transfer (FN-8) | FN-8 rejection at the `call` node | nothing (NodePath already identifies) | nothing |
| program entry | yes, *until ruling 2* | compiler-owned wrapper evaluates the goal once after setup, before owner transfer (PROG-3) | DIAG-3 record, `rule_id` OP-5 | DIAG-3 `message` | nothing |
| contract member (`ensures`) | never | proved at every selected return in complete/U/B (FN-9) | FN-9 rejection at the selected return | nothing | nothing |

Read the last two columns. Under ruling 2 the entry row is deleted, and the
"name used for" column is empty in every surviving row; the "justification used
for" column is empty in every row today. **The design's two additions have no
consumer in any position that will exist.** This table is the proposal's own
refutation and it is derived from its own specification, not from taste.

## 5. The `deny_claims` answer

> **Does a `deny_claims` root reject a callee's `requires`?**
>
> **No — and it must not.** A `contract_final` proposition, in either block and
> in every position, contributes no member to `DirectClaims` and therefore none
> to `MayClaims`. CLM-3's existing clause — "every ordinary user-call
> requirement owned by the component discharges at that call in caller U" — is
> the complete and correct treatment, and this proposal changes not one byte of
> it.

Three grounds, each from the spec:

1. **`MayClaims` enumerates assertions that will execute.** CLM-3's identity is
   `(concrete function instance, claim_stmt NodePath, claim name)`, and CLM-1
   makes a claim "a runtime check in all build modes ... never elided". Under
   ruling 1 a contract executes nowhere but the entry; under ruling 2, nowhere
   at all. Demanding an empty set of things that never run is vacuous.

2. **At an ordinary call a `requires` is not an assertion — it is the opposite.**
   FN-8 makes the *caller* prove it or be rejected. If contracts entered
   `MayClaims`, `deny_claims` would forbid calling any function with a
   precondition. That inverts the marker: a claim-free root wants *more*
   preconditions, because a precondition is exactly the mechanism that converts
   a would-be claim into a discharged obligation transported across a call
   boundary. `deny_claims` and `requires` are allies, not rivals.

3. **The one apparent hole is already closed by construction.** A
   `deny_claims command fn main` with a requirement retains one runtime wrapper
   check the marker does not forbid — but CLM-3 and PROG-3 already require that
   requirement to discharge in U *before* the wrapper check executes, so a
   marked entry's wrapper check is provably never taken. The conformance case
   `clm3-neg-generated-wrapper-check.wf` exists to prove exactly this: an opaque
   `band` conjunction that would pass dynamically is rejected statically,
   because "U does not compose that atomic goal". Ruling 2 deletes the hole
   outright.

**This answer defeats the proposal's premise.** If contracts must be treated
*oppositely* to claims in the one accountability mechanism where the difference
is observable, then "aligned with `claim`" was never a semantic alignment. It was
a shared syntax shape over two constructs with opposite obligations — which is
precisely the mechanism duplication this project's review axis is meant to catch.

**Accountability projection: also no.** DIAG-2's claim-accountability projection
exists to enumerate what a program asserts *without proof*. Contracts are proved.
The checked program already retains "every [FN-8] GoalTemplate, its requirement
occurrence ..., every concrete call substitution and discharged-goal derivation,
the S4 body-entry axiom", plus FN-9's `PostconditionExit` and
`PostconditionAggregate` roots. That record is *stronger* than a name list: it
retains the proof, not the promise. Merging contracts into the claim projection
would put proved and unproved obligations in one list and destroy the list's
meaning.

## 6. Composing with the FFI destination

The brief's ground was that an entry `requires` is a writer-stated trap. Owner
ruling 1 made that ground temporary: the destination is an FFI specification in
which a foreign boundary declares its own validation contract, the program entry
is one instance of a foreign boundary, and the entry check moves there. Owner
ruling 2 then superseded it with something stronger — the entry is out of scope
now, and a `requires`-bearing declaration will be required to be internal.

Under either ruling the conclusion is the same and ruling 2 makes it immediate:
**there is no executed contract**. So each element must earn its place on a
construct that provably never runs. Taken separately, as instructed.

### 6.1 The name — does not survive (conditionally survives later)

Candidate jobs, each tested:

- **DIAG-3 message.** Gone. No execution, no record.
- **Deterministic diagnostic identity.** Already complete. DIAG-2 fixes the
  requirement occurrence as `(concrete callee instance, final-check NodePath,
  conjunct ordinal 0)` and FN-9 the postcondition occurrence as
  `(concrete function instance, ensures_block NodePath, 0)`. DIAG-1's whole
  identity system is coordinate/NodePath-keyed by law. A name is *strictly
  redundant* identity, and §3.4(c) had to explicitly exclude it from goal
  equality to keep it from becoming semantics-selecting. An addition whose
  correctness requires an exclusion clause is not carrying identity.
- **Human reference in a diagnostic.** Real but weak: "requirement
  `capacity_bound` of `append` is unproved here" reads better than "the
  requirement of `append` is unproved here". But GRAM-2 admits at most one
  `requires` and one `ensures` per declaration, so *the requirement of `append`*
  is already unambiguous. The name is a nickname for a singleton, not a
  disambiguator.
- **Uniqueness discipline.** A uniqueness rule over a population of size one is
  vacuous by construction. Compare: 635 claims live in this tree, many per
  function, and CLM-1's uniqueness rule earns its keep every time DIAG-3 must
  name which one fired.

**Verdict: does not survive.** But the analysis yields something the study
should keep: FN-8 already fixes "conjunct ordinal 0", which is a deliberate hook
for a future block admitting several propositions. The moment a block admits
more than one, a name earns its place immediately — a diagnostic must say which
conjunct failed, and an ordinal is a worse handle than a name.

> **The name question is the multi-conjunct question in disguise.** Names are
> not wrong; they are premature. Adopt them when, and only when, a block admits
> more than one proposition.

### 6.2 The mandatory justification — does not survive

Start from what a claim justification is actually for, which is *not* what
intuition suggests. DIAG-3 is explicit that the justification never reaches the
trap record; the operator sees the name. So a justification does not explain a
runtime failure to whoever hits it. It answers a compile-time question:

> *Why does the writer believe this, when the checker could not prove it?*

It is the receipt for an **unpaid proof debt**. CLM-1 exists because a claim
asserts something the fragment did not derive, and the writer owes an
explanation.

A contract proposition carries no such debt:

- At an ordinary call, FN-8 rejects `refuted` and `unproved`. The caller must
  *prove* it.
- Inside the body it becomes S4, and ENT-1 fixes that every fact source is "an
  executed control condition, an executed runtime check, **a requirement proved
  at an ordinary call** or checked at a dynamic boundary before S4 admits it to
  a body". With the boundary out of scope, S4 is earned by proof at every call,
  full stop.

Nothing is asserted; everything is proved. The question the justification
answers never arises, so the answer is required prose with no reader.

**The corpus has already run this experiment.** 15 of 19 `ensures` messages
contain the word `postcondition`; twelve are nothing but the word for the slot.
Writers handed an inert mandatory string filled it with its own name. A stronger
mandate on a longer string will produce the same noise at greater length across
104 sites.

There is one genuine residual: a contract states an *interface decision* —
"this function is not defined outside this domain" — and a reader may want to
know why the domain is what it is. But that is documentation, and the language
already has `doc`. FN-8/FN-9 forbid `doc` inside the block, and correctly; the
declaration's own `doc` is the right home.

**Verdict: does not survive, and would be actively harmful.**

### 6.3 Refutability — survives, declaration-site form only

Two distinct judgments hide under one word.

- **Call-site refutation already exists.** FN-8's `refuted` disposition rejects.
  Nothing to add.
- **Declaration-site refutation does not exist.** A `requires` whose proposition
  contradicts the parameters' own implicit ENT-2 facts — `ilt(x, x)`, or
  `igt(x, 255_u8)` for `x: own u8` where ENT-2's implicit facts already carry
  `x - Z <= 255` — is accepted today. It merely makes the function uncallable,
  silently. That is a real defect with a cheap deterministic detector, exactly
  analogous to CLM-2's refutation.

Crucially, refutation is a compile-time judgment *about the proposition itself*.
It has no dependence whatever on execution, so ruling 2 does not touch it.

`ensures` gets this free: FN-9 already mandates complete discharge at every
selected return, so a self-contradictory postcondition is already rejected as
unproved. **Only `requires` has the gap.**

**Verdict: survives.** And it needs no name, no justification, and no keyword.

### 6.4 Redundancy advisory — survives, declaration-local form

A `requires` proposition derivable from the parameters' implicit facts alone
imposes no obligation on any caller: `ile(x, 255_u8)` for `x: own u8` is a
signature that promises to constrain and does not. CLM-2's advisory precedent
applies verbatim — non-rejecting, so ENT-1's version-monotonicity law is
untouched, and "a later specification version that proves more predicates
therefore rejects no previously accepted program on that ground".

Restrict it to the **declaration-local** form. The stronger unit-wide form ("no
call in this unit had to work for it") is unit-dependent: adding one caller
changes the advice, which makes it noise.

**Verdict: survives.** Again: no name, no justification, no keyword.

### 6.5 Accountability projection — does not survive

Answered in full at the end of §5. Already served better by the retained
derivation DAG, and merging would degrade the existing artifact by mixing proved
with unproved.

## 7. Proposal B′ — the strongest defensible remnant

Left concrete so the adversarial round has something real to attack. B′ keeps
what §6 proved survives and discards what it did not.

### 7.1 Terminal

```wf-ebnf GRAM-2
requires_entry:= doc | stmt | contract_final
ensures_entry := doc | stmt | contract_final
contract_final:= "holds" expr ";"
```

`check_stmt` is deleted from GRAM-4. `holds` becomes a FORM-3 fixed atom
(corpus cost zero); `check` is freed as an IDENT spelling with no reservation
ordinal shift; `trap` stays reserved through the OPNAME mode-word set. FORM-2's
line-bearing list swaps `check_stmt` for `contract_final`. FORM-5's STRING home
list loses "contract final `check` messages" and keeps `doc` and `claim`
justifications only.

Every §3 clause about names, justifications, shared uniqueness domains, the
contract-name carrier, DIAG-2 retention of name and justification, and the
DIAG-3 message is **dropped**.

**Why keep a keyword at all?** Because the honest alternative — a bare
`expr ";"` proposition — forces `requires_entry` to narrow from
`doc | stmt | contract_final` to `doc | let_stmt | proposition`, since
`expr_stmt := call ";"` and a bare-expression proposition would both accept an
IDENT-headed `(` start, and GRAM-1 makes two matching arms a specification
defect rather than a precedence rule. That narrowing is probably *right* — it
would delete FN-8's and FN-9's structural passes, which exist only because the
grammar deliberately over-admits — but it is a larger change than this remnant
claims, and it belongs to whichever proposal wants to argue for it. B′ is the
conservative remnant: one fixed terminal, one-token decision, DIAG-1 frontier
attribution and arm ranking unchanged.

### 7.2 The surviving accountability: two CLM-2-shaped judgments in FN-8

> A `requires` proposition additionally takes one declaration-site lifecycle
> disposition, judged in the block's own scope — the function parameters, named
> consts, and type and const parameters — under exactly [ENT-4]'s closure over
> [ENT-2]'s implicit facts, with no caller state and no body fact.
>
> When that state is non-contradictory and derives the proposition's exact
> negation, the declaration is rejected with a hard error citing FN-8 at the
> `contract_final` node, carrying the proposition and the derived negation: a
> requirement no call could ever discharge is a defect found at compile time.
>
> When that state derives the proposition itself, the requirement constrains no
> caller. The declaration remains accepted and a conforming implementation
> reports one non-rejecting redundancy advisory naming the declaration. An
> advisory is not a [DIAG-1] rejection, so a later specification version that
> derives more propositions rejects no previously accepted program on this
> ground.

`ensures` needs neither: FN-9's mandatory complete discharge at every selected
return already rejects the contradictory case, and a redundant postcondition is
a weak-summary question, not a lifecycle one.

### 7.3 Everything else

- **`deny_claims`**: unchanged, per §5. Contracts never enter `DirectClaims` or
  `MayClaims`.
- **Accountability projection**: unchanged, per §5.
- **Effect row**: unchanged, per §3.8. EFF-2 already states ruling 1.
- **Entry**: FN-8 gains one sentence — a `fn_decl` that FN-7 selects as the
  unit entry may carry no `requires_block`; a violation is a hard error citing
  FN-8 at the `requires_block` node. See §9.1 for why this must key on FN-7's
  entry and not on `program_kind`. That deletes FN-8's program-start paragraph,
  PROG-3's two requirement paragraphs, DIAG-2's "one retained program-start goal
  evaluation", DIAG-3's OP-5 row, and CLM-3's marked-entry clause — a
  substantial simplification, and it removes OP-5's last runtime consumer on the
  contract side.

### 7.4 Recorded successor, not adopted

When a contract block admits more than one proposition, revisit the name. That
is the condition under which §6.1's verdict flips, and it is the only one.

## 8. What this costs

Stated without softening, as required.

**The churn is real and it is not mechanical.** 104 contract finals across 90
files. The condition transfers verbatim; the surrounding tokens do not. For the
briefed design (§3) every site needs an *invented name* and an *authored
justification* — 104 pieces of new prose, and the existing message cannot be
reused, because a message says what went wrong and a justification says why the
writer believes it holds. Byte delta per site is `name + justification − message`;
against a measured mean message of 18.3 bytes, a plausible name of ~14 and
justification of ~50 gives roughly +45 bytes per site, ~+4.7 KB total. FORM-1
forbids auto-formatting and FORM-2 fixes canonical bytes exactly, so all 90
files are hand-rewritten and re-verified. B′ (§7) reduces this to a mechanical
token substitution — `check ` → `holds `, delete ` else trap "…"` — which is a
regex, not authorship, and a net *saving* of about 1.9 KB.

**Mandatory justifications on every contract are ceremony, and the corpus proves
it before the change is made.** 15 of 19 existing `ensures` messages contain the
word `postcondition`; twelve say nothing else. Writers handed an inert mandatory
string filled it with the name of the slot. There is no reason to expect a
mandatory `because` to fare better, and one reason to expect worse: a `because`
carries a stronger implication of insight, so the noise will be longer and read
as content. The briefed design turns 19 measured instances of ceremony into 104,
and §3.5 already conceded that it replaces one dead limb with two.

**The `deny_claims` interaction is where the proposal breaks itself.** §5 had to
argue that a `deny_claims` root must treat a contract proposition *oppositely*
to a claim — never rejecting it, only demanding it discharge in U. If the two
constructs must be treated oppositely in the one mechanism where the difference
is observable, the alignment was cosmetic: a shared syntax shape over two
constructs with opposite obligations. That is textbook mechanism duplication.
Worse, §3.8 identified the live hazard it creates: `holds name: e because "…";`
reads exactly like a claim, and a future editor generalizing "every named
justified proposition exhibits `traps`" would silently give every
contract-bearing function a trapping effect row. The proposal manufactures a
resemblance the semantics must then work to deny.

**The DIAG-3 trade was backwards even before ruling 2.** §3.7 replaces
`"x must be positive"` with `"positive_input"` in the one record an operator
ever sees. It buys reviewer traceability that §6.1 then shows was already
supplied by NodePath, and it sells operator legibility that was real. Under
ruling 2 it sells the only half that existed to buy a half that never will.

**The strongest rival argument against me** is not any of the above. It is this:

> The entire proposal is an answer to a question the language stopped asking.
> Batch-0071 established that every *writer-stated trap* carries a name, a
> justification, an accountability entry, and refutability — and that law is
> correct, for traps. The proposal reads "writer-stated" as the load-bearing
> half of that phrase and generalizes to every writer-stated *proposition*. But
> the load-bearing half is "trap". The apparatus exists because a claim
> **executes and can fire**: the name is what the operator reads in the record,
> the justification is the receipt for the proof debt that only an unproved
> assertion incurs, the accountability entry is a list of exactly those debts,
> and refutation is the compile-time discovery that a debt can never be repaid.
> Strip execution and the first three lose their consumer in the same stroke.
> A contract is not a weak claim; it is the opposite construct — the mechanism
> by which a writer *avoids* a claim, converting an assertion into an obligation
> the caller must prove. Dressing the opposite construct in the trap
> construct's clothes is not alignment. It is a category error with good
> intentions, and it would leave the language with two things that look the same
> and mean the reverse of each other.

I do not have a rebuttal. That argument is correct, it is derivable from the
spec text quoted in §1, and it is why §0 reports a verdict against the assigned
stance rather than a defense of it. What survives — §7's `holds e;` plus
declaration-site refutation and a redundancy advisory — survives precisely
because those two judgments were never about the trap.

## 9. Three corrections the study needs regardless of which proposal wins

### 9.1 "No `program_kind`" does not close the entry hole

Owner ruling 2 was relayed as: a declaration carrying a `requires` will have no
`program_kind`. That does not achieve what it intends.

FN-7 fixes the unit entry as "exactly one top-level `fn_decl` named `main`", and
its *unlabelled* form "carries no `program_kind` child". PROG-3 then says: "This
rule governs both the unlabelled no-input entry and the `command` entry." So an
unlabelled `fn main` is a program entry, executes the wrapper check, and carries
no `program_kind`.

The corpus confirms this is not hypothetical. Of the three entry declarations
carrying a `requires`, the one that actually exercises the trap —
`tests/conformance/cases/fn8-trap-requires-false.wf`, expectation
`{"kind":"trap"}` — is `fn main() -> own unit pure requires {`, with no
`program_kind`. A `program_kind`-keyed restriction leaves exactly that site
alive and keeps one executed contract in the language.

**The restriction must key on FN-7's selected entry declaration — the top-level
`fn` named `main`, labelled or not — not on `program_kind`.** Note also that
the unlabelled entry "retains its ordinary callee status", so it is genuinely a
dual-position declaration and the rule must name the position, not the callee.

### 9.2 Three protected conformance cases are affected, not zero and not one

The restriction was reported as costing zero migration, then as costing one case.
It costs three: all three `requires`-bearing entry declarations are in
`tests/conformance/cases/`.

- `fn8-trap-requires-false.wf` — rules `["FN-8","SCOPE-4"]`, expectation
  `{"kind":"trap"}`. The only runnable case proving the entry wrapper traps. The
  restriction makes its subject unrepresentable; the case must be deleted, not
  edited.
- `clm3-neg-generated-wrapper-check.wf` — rules
  `["FN-8","PROG-3","CLM-3","ENT-4"]`, expectation
  `{"kind":"reject","rule":"FN-8"}`. Exists to prove a marked entry's U judgment
  precedes and cannot be laundered by the wrapper check. Same fate.
- `clm3-pos-transitive-value-branch.wf` — a positive CLM-3 case whose entry
  carries a requirement; recoverable by moving the requirement to an internal
  callee, but the diff is not trivial.

Per `CLAUDE.md`, "Any addition, modification, deletion, or rename involving
protected conformance or equivalent compliance evidence requires an exact
before/after audit, owner explanation and approval, and an approval-ledger
entry." Whichever proposal wins must carry these three in its approval packet.
`tests/conformance/runner.py` verifies a trap expectation as `{"kind":"trap"}`
and asserts no record bytes, so no *verdict* moves on the DIAG-3 message alone —
but the compiler's focused trap tests do assert bytes.

### 9.3 The entry count has now been measured wrong twice, in the same place

Worth recording because the same blind spot will recur in the migration diff.

- First measurement: "77 entry declarations, not one carries a `requires`." The
  pattern required a leading `program_kind` word and missed the unlabelled form
  entirely.
- Second measurement: "525 entry declarations — 451 unlabelled plus 74
  `command` — exactly one carries a `requires`." Closer, but the pattern anchors
  on `fn` or `command` and drops the optional `deny_claims` prefix that GRAM-2
  admits *before* `program_kind`:
  `fn_decl := "deny_claims"? program_kind? "fn" IDENT ...`.

Both figures are the same four declarations short: 2 `deny_claims fn main` and
2 `deny_claims command fn main`. 451 + 2 + 74 + 2 = **529**. And the omission is
not random with respect to this study — two of the four dropped declarations are
`deny_claims command fn main ... requires {`, which is precisely why the
`requires`-bearing entry count kept reading as one instead of three.

Any grep over declarations in this migration must admit the full optional prefix
chain, and any count that comes out clean should be checked against
`deny_claims`-marked declarations before it is believed.

## 10. Provenance

- Spec read: `spec/kernel-spec.md` v0.32 — GRAM-2, GRAM-4, GRAM-6, FORM-1/2/3/5,
  OP-5, FN-7, FN-8, FN-9, PROG-3, EFF-2, DIAG-1/2/3, CLM-1/2/3, ENT-1/2/3,
  R3-PROVISIONAL register (line 8).
- Design memory read: `checks-and-proofs.md`,
  `checks-and-proofs/requires-entry-contract.md` and its
  `.alt/recognizer-driven-elision.md`,
  `requires-entry-contract/requirement-enforcement.md` and its
  `.alt/callee-entry-prologue.md`,
  `checks-and-proofs/obligation-discharge.md`,
  `obligation-discharge/writer-trap-surface.md` and its frozen
  `.alt/dual-check-and-claim.md`.
- Corpus measured over 637 `.wf` sources at `d32d7dd0`, excluding `archive/`.
- No repository code, specification byte, or test was changed by this study.
