# Proposal A — the proposition terminal

Design study, not a candidate. Nothing here is authorized: landing any of it
would take the specification-change workflow, an exact-byte owner approval for
`spec/kernel-spec.md`, and a separate protected-conformance approval for the
cases named in §5. This document changes no repository code, no spec byte, and
no test.

Baseline: `batch-0072` tip `d32d7dd0`, active spec v0.32
(`spec/kernel-spec.md`, SHA-256 `5ea3927a…4e6bf5` per `docs/current-plan.md`).
Every quotation below was read from that file; every count was measured on that
tree with the command shown.

---

## 0. The core, in one paragraph

Delete the `check_stmt` production and its `else trap STRING` tail from the
grammar, and replace the mandatory final entry of a `requires` or `ensures`
block with `holds expr ";"` — a form that states a proposition and carries
nothing else: no message, no consequence, no execution. Make the contract
surface generate no runtime code in any position by forbidding a `requires`
block on the [FN-7] entry declaration, so that `requires` becomes an
unconditional caller-side compile-time obligation and `ensures` remains what it
already was. The [ENT-3.S4] body axiom then rests on a single closed premise —
every execution of a body carrying a requirement is reached through a source
call edge that statically discharged that exact instantiated goal — with no
dynamic disjunct and no wrapper. The program-start wrapper, its [OP-5] trap
record, and the [DIAG-3] row that stamps a writer STRING into a trap are
deleted outright rather than left dormant, because a retained-but-unreachable
clause is the same defect as the inert `else trap` half that prompted this
study. The result is that no writer STRING is observable at runtime anywhere in
the language, and no contract construct in any position has a runtime meaning
to misread.

---

## 1. Where the surface stands today (verified against v0.32)

### 1.1 The production survives only inside contracts

[GRAM-4]'s `stmt` alternation in v0.32 is:

```
stmt := let_stmt | set_stmt | expr_stmt | return_stmt | loop_stmt
      | for_stmt | break_stmt | region_stmt | claim_stmt
      | if_stmt | match_stmt | give_stmt
```

`check_stmt` is not in it. Batch 0071 (`f8c81dfc`, activating v0.32) retired the
anonymous body check; [mcts_mem/whitefoot/checks-and-proofs/obligation-discharge/writer-trap-surface.md](https://github.com/mbbill/Whitefoot/blob/0f22b026b062c36ef2f8ce9c725ee4307e1fda64/mcts_mem/whitefoot/checks-and-proofs/obligation-discharge/writer-trap-surface.md)
records the move and its reason ("an anonymous body check is a claim minus its
name, justification, accountability entry, redundancy advice, and refutation")
and, in the same node, the deferral this study inherits:

> The retained condition-judgment and program-start trap semantics stay owned by
> the rule the retired body statement carried, and the contract final keeps that
> spelling.

So `check_stmt := "check" expr "else" "trap" STRING ";"` is now referenced from
exactly two places in the whole grammar — `requires_entry` and `ensures_entry`
— and the compiler says so in its own comments
(`compiler/src/semantic/entailment/flow.rs:5696`: "v0.32 has no body
`check_stmt`: the production survives only …";
`compiler/src/resolution/engine/admission.rs:129-131`;
`compiler/src/semantic/check/requires.rs:290-292`). The trap vocabulary in a
contract is a leftover from a construct that no longer exists.

### 1.2 The two blocks differ in kind — and the difference is one declaration

[FN-9] states its final's status flatly:

> The final `check` is proposition syntax only: its message, clause-local
> spelling, and sharing have no identity, it contributes no `traps`, it never
> executes, and it emits no [DIAG-3] record.

[FN-8]'s final is different, but only at one site. At an ordinary call there is
no prologue ("An ordinary caller never receives a fallback runtime check, entry
branch, or second callee body"; "There is no executable ordinary-callee
prologue"). At program start there is:

> After ordinary input setup, the compiler-owned entry wrapper evaluates the
> same complete goal once, before transferring any source owner to the body.
> A false result has the final `check_stmt`'s [OP-5] trap semantics and invokes
> the body zero times…

and [DIAG-3] carries the writer's bytes out to the operator:

> For an [FN-8] program-start goal, `rule_id` is `OP-5` and `message` is the
> final `check_stmt`'s STRING value decoded by [FORM-5].

That is the whole asymmetry: one declaration in the unit — the [FN-7] `main` —
can make the tail live. Everywhere else in the language the tail is dead bytes.

### 1.3 Two things the current surface says that are not true

Both were found while grounding, both are consequences of the same defect, and
both disappear under this proposal.

**(a) A `pure` function can abort the process.**
`tests/conformance/cases/fn8-trap-requires-false.wf` is
`fn main() -> own unit pure requires { … }` with manifest verdict
`{"expect":{"kind":"trap"}}`. [EFF-2] is explicit that the block contributes
nothing ("A function whose body and release contribution are empty may
therefore declare `pure` while carrying a requirement"), and it isolates the
wrapper: "The retained program-start check [PROG-3] and any future gated adapter
check [GATE-1] belong to those dynamic boundaries, not to an ordinary source
call or the callee's exhibited row." The row is therefore honest about the
*body* and silent about the only place the declaration can trap. A reader of the
signature cannot tell.

**(b) The message channel is the block's only prose channel.**
[FORM-4] says there are no comments. [FN-8] and [FN-9] both reject a `doc` entry
inside the block ("a `doc` … is a hard error citing FN-8"). So inside a contract
block the trap STRING is the sole place a writer can put an English sentence —
and writers use it that way. That is load-bearing for §8.

### 1.4 The register already flags this

The R3-PROVISIONAL register line at `spec/kernel-spec.md:8` names the exact item:

> the `requires { requires_entry* }` surface spelling with its FN-8-checked
> ordinary-let/final-check subset (FN-8 — semantics selected, spelling not yet
> compared)

and [mcts_mem/whitefoot/checks-and-proofs/requires-entry-contract.md](https://github.com/mbbill/Whitefoot/blob/0f22b026b062c36ef2f8ce9c725ee4307e1fda64/mcts_mem/whitefoot/checks-and-proofs/requires-entry-contract.md) dates it:

> 2026-07-11 statement: the semantics (existence, callee-entry execution,
> always-retained check, concrete-only scope) are evidence-selected; the
> `requires { let* check }` block spelling is minimality-selected and
> R3-provisional pending a writer-tier comparison against a credible
> single-predicate alternative.

---

## 2. What the corpus actually contains (measured)

All counts taken on the `batch-0072` tip. `research/experiments/**` frozen
`.xl` artifacts are excluded — they are sealed experiment outputs against older
spec versions and are not migrated by any spec change.

### 2.1 Contract blocks

```
grep -rcoE '\brequires[[:space:]]*\{' --include='*.wf' tests research
grep -rcoE '\bensures[[:space:]]+[A-Za-z_]' --include='*.wf' tests research
```

| corpus | `requires` blocks | `ensures` blocks | files |
|---|---|---|---|
| `.wf` under `tests/` + `research/` | **87** | **19** | 82 / 14 |
| embedded sources in `compiler/**/*.rs` | 143 | 120 | 18 / 13 |

### 2.2 Finals, and the two ways the block count does not equal the check count

117 `else trap` lines exist in the `.wf` corpus and 106 contract blocks exist.
The two numbers differ in both directions, and the reconciliation matters
because a migration recipe that assumes one final per block is wrong:

- **2 blocks carry no final.** `fn8-neg-requires-no-check.wf` (a `requires` with
  only a `let`) and `fn8-neg-doc-only-clause.wf` (a `requires` containing only a
  `doc`) are deliberate FN-8 negative cases. 106 blocks − 2 = **104 contract
  finals**.
- **13 stray body-position checks remain**, in 9 files, all under `research/`.
  Eight files carry only stray checks (`checked-law-channel/kernel.wf`,
  `codegen-vs-rust-c/xl/kernelA.wf` and `kernelB.wf`,
  `effect-attrs-channel/kernel.wf`, `port-study/binary-trees/btrees.wf`,
  `arith-dissolution/evidence.wf`, `o11-composition/probe-band-check.wf`,
  `reborrow-extension/chain-evidence.wf`); the ninth,
  `zlib-core-kernels/huffman_literals.wf`, carries a contract final *and* two
  body checks at lines 125 and 133. All 13 are already invalid under v0.32's
  `stmt` alternation — pre-existing rot, unaffected by this proposal either way,
  noted so that 104 + 13 = 117 closes.

| directory | files with a final | contract finals | stray body checks |
|---|---|---|---|
| `tests/conformance/cases/` (protected evidence) | 42 | 56 | 0 |
| `tests/codegen/cases/` | 37 | 37 | 0 |
| `tests/programs/` | 6 | 8 | 0 |
| `research/` | 3 | 3 | 13 (in 9 files) |
| **total** | **88** | **104** | **13** |

### 2.3 How much of the tail can ever execute

```
grep -rnE '^([a-z_][a-z0-9_]*[[:space:]]+)*fn[[:space:]]+main[[:space:]]*\(.*\brequires[[:space:]]*\{' \
     --include='*.wf' tests research
```

Entry declarations in `.wf` under `tests/` + `research/`: **532** — 451
unlabelled `fn main(`, 74 `command`, 2 `deny_claims command`, 2 `deny_claims`,
plus 3 `service`/`embedded`/`daemon` spellings that are FN-7 negative cases, not
live entries.

Entries carrying a `requires`: **3**, all in `tests/conformance/cases/`:

| case | form | manifest verdict |
|---|---|---|
| `fn8-trap-requires-false.wf:1` | `fn main() -> own unit pure requires {` | `{"kind":"trap"}` |
| `clm3-neg-generated-wrapper-check.wf:1` | `deny_claims command fn main() … requires {` | `{"kind":"reject","rule":"FN-8"}` |
| `clm3-pos-transitive-value-branch.wf:36` | `deny_claims command fn main() … requires {` | `{"kind":"run","exit":0}` |

Zero in `tests/programs/`. Zero in `research/`. Twelve more entry-with-`requires`
sources are embedded in compiler Rust tests across six files
(`backend/tests.rs:1095`, `backend/tests/requires.rs:9,31,63,84,136`,
`lowering/tests.rs:738`, `resolution/tests.rs:699,719`,
`semantic/tests/strict.rs:260,401`, `semantic/tests/requires.rs:241`).

So: **84 of 87 `requires` blocks and 19 of 19 `ensures` blocks carry a message
that no execution can ever reach**, and the three that could are all test
fixtures whose subject *is* the wrapper. On real programs — `tests/programs/`,
`research/experiments/` — the count of reachable contract messages is zero.

### 2.4 What the messages say

53 distinct strings across the 106 `else trap` occurrences in contract-bearing
files (the 104 finals plus `huffman_literals.wf`'s two stray body checks).

| message | count |
|---|---|
| `"output capacity"` | 36 |
| `"x must be nonnegative"` | 5 |
| `"relay postcondition"` / `"postcondition"` | 3 + 3 |
| `"selected postcondition"`, `"bounded postcondition"`, `"seedless bound"`, `"index bound"`, `"guard bound"`, `"shift nonzero"`, `"output too short"`, `"base64 output capacity"`, `"append result exceeds destination"`, `"append filled exceeds destination"` | 2 each |
| 40 further strings | 1 each |

The `ensures` messages are the tell: `"postcondition"`, `"relay postcondition"`,
`"bounded postcondition"`, `"second postcondition"`, `"selected postcondition"`.
When a writer has nothing to say to an operator who will never read it, the
writer writes the construct's own name back. That is the channel reporting that
it carries no information.

The `requires` messages are more mixed, and honestly so: `"append result exceeds
destination"` and `"read bits result exceeds mask"` are real English restatements
of the predicate. §8 treats that as the cost it is.

### 2.5 How many blocks actually use their locals

The block-with-locals shape is defended on the ground that real predicates need
decomposition. That is measurable, so it was measured — clause `let` count per
block, over `tests/conformance/cases/` and `tests/programs/` (the discriminating
corpus; the 36-copy `output-capacity-lockstep` codegen family is excluded so one
fixture does not dominate):

| clause locals | blocks | share |
|---|---|---|
| 0 | 24 | 37.5% |
| 1 | 21 | 32.8% |
| 2 | 15 | 23.4% |
| 3 | 3 | 4.7% |
| 4 | 1 | 1.6% |

64 blocks. **62.5% use at least one local and 30% use two or more** — the shape
is not gratuitous. **37.5% use none at all** — for those, the block is three
lines of ceremony around one expression, and a single-predicate form would read
identically. §8.3 and §8.6 do not soften what that second number means.

### 2.6 Byte cost of the tail

```
xargs grep -ohE ' else trap "[^"]*"' < <contract-bearing .wf files>
```

All 117 `.wf` tails: 3,640 bytes, mean 31.1 B. Netting out the 13 stray body
checks (387 bytes): the **104 contract finals carry 3,253 bytes** of
` else trap "…"`, mean 31.3 B.
Compiler-embedded Rust sources: 248 ` else trap "…"` occurrences across 17 files,
**5,391 bytes**.

---

## 3. The proposal

### 3.1 Grammar

[GRAM-2], exact delta:

```wf-ebnf
requires_block:= "requires" "{" requires_entry* "}"
requires_entry:= doc | stmt | holds_stmt          -- was: … | check_stmt
ensures_block:= "ensures" ensures_selector "{" ensures_entry* "}"
ensures_entry:= doc | stmt | holds_stmt           -- was: … | check_stmt
```

[GRAM-4], exact delta — the production is deleted, not moved:

```wf-ebnf
check_stmt  := "check" expr "else" "trap" STRING ";"    -- DELETED
holds_stmt  := "holds" expr ";"                          -- NEW
```

`holds_stmt` belongs in the [GRAM-4] fence beside the statement forms it sits
among, exactly where `check_stmt` sits today; it is not a `stmt` alternative, in
the same way `check_stmt` is not one today.

**Strong-LL(2) is preserved with one token and no new lookahead.** `requires_entry`
and `ensures_entry` each decide among `doc` (fixed atom `doc`), `stmt`, and
`holds_stmt` (fixed atom `holds`). Because `holds` becomes an exact fixed atom it
is [FORM-3]-ineligible for IDENT, so no `stmt` alternative can begin with it —
`stmt` begins with `let`, `set`, `return`, `loop`, `for`, `break`, `region`,
`claim`, `if`, `match`, `give`, or an IDENT/OPNAME `call`. The `SELECT_2` rows are
structurally identical to today's, where `check` plays the same role.

**Terminal-set consequence under [FORM-3].** IDENT is defined by exclusion —
"excluding every lowercase token spelling produced by exact fixed grammar atoms
in the complete grammar" — so the atom set change moves three words:

- `holds` leaves IDENT (narrowing). Measured: `holds` appears 19 times in the
  `.wf` corpus, every one inside a `doc` or `because` STRING; **zero** identifier
  collisions.
- `check` enters IDENT (widening). Measured: zero uses as an identifier today.
- `trap` enters IDENT (widening) — but only as a binding, parameter, or function
  name. It stays barred from field binding by the independent OPNAME
  maximal-munch reservation in [FORM-3] ("all five suffix words are reserved from
  field binding"), so `tests/conformance/cases/form3-neg-reserved-mode-field.wf`
  (`struct Bad { trap: i32; }`, expect reject FORM-3) keeps its verdict unchanged.
  This was checked, not assumed.

The two widenings are an acceptance-set change and are treated as a cost in §8.

### 3.2 Why `holds`, and what was rejected

The terminal must read as a proposition in both blocks, because after this
change both blocks *are* propositions. Candidates considered:

- **Keep `check e;`** — strictly the smaller delta: no new atom, `check` stays
  reserved, only `else trap STRING` is deleted. Rejected as the primary form
  because it fixes two thirds of the defect and leaves the third: `check` is an
  imperative verb naming an action, and in an `ensures` block no action occurs
  and none ever will. The owner's objection to reusing `claim` — that an ordinary
  claim earns its fact by generating a retained runtime check while a contract
  proposition earns nothing at runtime — applies to `check` with equal force and
  for the same reason. This is recorded as the fallback if the adversarial round
  prices the new atom above the misleading verb.
- **No terminal keyword; the final entry is a bare `expr ";"`.** Smallest surface
  of all — deletes a production and mints nothing. Rejected: it creates a second
  expression-statement spelling next to `expr_stmt := call ";"` (a [META-2]
  proliferation), removes the visual anchor that tells a reader which line of a
  five-line block is the proposition, and leaves FN-8's "missing final" rejection
  with nothing to name but the block. §8 records that a rival can attack this
  rejection.
- **`assert`, `prop`, `it`, `so`** — imperative, abbreviated, or contentless.
  Whitefoot's keywords are whole English words (`requires`, `ensures`,
  `propagate`, `replace`, `because`, `allocates`); `prop` is off-style.

`holds` is a whole word, present indicative, declarative, and reads correctly
under both block heads:

```
fn append_slice['d, 'm](destination: &uniq 'd buffer<u8>, filled: own u64,
                        text: own slice<'m, u8>) -> own u64
                        reads('d 'm), writes('d) requires {
  let capacity = len(deref(destination));
  let remaining = isub(capacity, filled);
  holds ile<u64>(len(text), remaining);
} {
```

```
fn read_bits['s, 'i](state: &uniq 's InflateState, input: &'i buffer<u8>,
                     count: own u32, mask: own u64)
                     -> own Result<u64, InflateError>
                     reads('s 'i), writes('s) ensures Ok(value: result) {
  holds ile<u64>(result, mask);
} {
```

### 3.3 The internal-only restriction

**Rule text (new, in [FN-8]):**

> No `fn_decl` may carry a `requires_block` when it is the [FN-7] entry — the
> unit's single top-level declaration named `main`, in either the unlabelled or
> the kind-declaring form. A `requires_block` on that declaration is a hard error
> citing FN-8 at the `requires_block` node. The mechanical repair states the
> condition where it can be established: move the requirement to an ordinary
> internal callee the entry calls, and give the entry's own external
> precondition a real branch that returns the domain's normal error value on the
> false edge [patterns P12]. A contract therefore never executes, in any
> position, and this version defines no validated start-time or foreign callable
> boundary.

**The predicate must be entry identity, not `program_kind` presence.** [FN-7]
fixes both forms — "Exactly one top-level `fn_decl` named `main` must exist in
the compilation unit. That declaration is the unit's entry" — and states that
"The unlabelled entry carries no `program_kind` child". [PROG-3] confirms both
are program start: "This rule governs both the unlabelled no-input entry and the
`command` entry." A restriction phrased over `program_kind` would leave
`fn main() -> own unit pure requires { … }` legal and executing, which is
precisely the form of the one conformance case whose subject is the wrapper trap
(§2.3). Phrasing it over `main` also needs no new concept: FN-7 already forbids
any other declaration from carrying a `program_kind`, so "the entry", "the
declaration named `main`", and "the only declaration that may carry a
`program_kind`" pick out the same node.

**Doctrinal convergence — agreed.** `docs/patterns.md` P12 already tells writers
this, as guidance:

> A `claim`, an ordinary callee requirement/prologue, or a process-entry wrapper
> check is not a repair: each turns expected external failure into a trap.
> … Replaces: … relying on a checked entry wrapper to authorize a body access.

The restriction does not invent a rule; it makes P12's second sentence
machine-enforced for the one case where the language still offered the
forbidden move. That is a genuine argument in its favor and I take it. What it
is *not* is a reason the restriction is free — see §8.4.

### 3.4 Rule-by-rule text

**[OP-5]** — becomes a pure typing rule with no runtime clause. First four
sentences retained verbatim with `check e else trap "msg";` replaced by
`holds e;`. The two execution sentences

> The final `check_stmt` in a `requires` block uses this exact condition
> judgment, decoded message, and dynamic-boundary failure behavior, but [FN-8]
> owns its execution: it is no ordinary-callee runtime check, and only program
> start plus a later implemented gated adapter evaluate it.
> The final `check_stmt` in an `ensures` block uses the exact condition judgment
> but [FN-9] owns it as a proof obligation; it never executes and has no dynamic
> boundary failure behavior.

are replaced by one:

> A `holds_stmt` states a proposition and nothing else: it names no message,
> contributes no effect category, never executes, emits no [DIAG-3] record, and
> has no dynamic-boundary failure behavior in any position. [FN-8] owns the
> proposition in a `requires` block and [FN-9] owns it in an `ensures` block.

The DEFERRED sentence on the fuller stated-and-checked vocabulary is unchanged.

**[GRAM-4] prose, [GRAM-6], [GRAM-7], [GIVE-1]** — unchanged. None references
`check_stmt`.

**[FN-8]** — four edits.

1. Structural pass, three spellings: "followed by exactly one final
   `holds_stmt`"; "an empty block or an all-let sequence instead reports the
   `requires_block` node for its missing final proposition"; "Thus a nonfinal or
   repeated proposition, a `doc`, …". Semantics of the pass are untouched.
2. "the final check condition is either a Bool clause atom or one such operation
   returning Bool" → "the proposition is either a Bool clause atom or one such
   operation returning Bool".
3. "The callee-instance identity and final-check NodePath identify the
   requirement occurrence" → "final-proposition NodePath". The node still exists,
   so every [DIAG-1] anchor that names it survives.
4. Deletions. The program-start paragraph collapses; taking its sentences in
   order as they appear after "There is no executable ordinary-callee prologue":
   - "Program start is the one implemented dynamic boundary and follows
     [PROG-3]." — **deleted**.
   - "[PRV-3] treats every labelled `command` input as unconditionally external
     and judges an entry-local protected leaf before lowering." — **survives**;
     it does not depend on a requirement.
   - "An entry-local leaf whose constrained subject is unconditionally external
     and whose unasserted S3-blinded state fails is rejected directly; when such
     an external leaf is retained behind this entry's own S4 requirement bridge,
     it must additionally discharge in the S4-blinded state…" — **the second
     clause is deleted** (an entry has no requirement, so no entry-own bridge
     exists); the first clause survives.
   - "An inherited bridge reached through an entry-body call is checked at that
     call's selected argument and any rejection is instead owned by PRV-2." —
     **survives**, and becomes the only bridge route at an entry.
   - "Thus neither the compiler-owned wrapper check nor the body's S4 axiom can
     launder an external protected leaf…" → "Thus the body's S4 axiom cannot
     launder an external protected leaf…".
   - "A requirement unrelated to a protected leaf retains the boundary behavior
     below." — **deleted**.
   - The three wrapper sentences ("After ordinary input setup, the compiler-owned
     entry wrapper evaluates…" / "A false result has the final `check_stmt`'s
     [OP-5] trap semantics…" / "The wrapper evaluates only the non-consuming
     reads admitted above…") — **deleted**.
   - "A later gated foreign callable boundary remains governed by [GATE-1]; this
     version implements no such entry, FFI stub, export, or foreign adapter." —
     **retained**, and joined by the new restriction's closing sentence.
   - "The requirement is a checked signature obligation rather than an executed
     declaration occurrence and contributes no source effect [EFF-2]." —
     **survives**, and becomes unconditionally true.
   - In the [CLM-3] paragraph: "For a marked program entry with a requirement,
     the same concrete goal must discharge in U after ordinary standard-input
     setup but before the compiler-owned wrapper check, owner transfer, or S4
     establishment; failure cites FN-8 at the requirement final `check_stmt`." and
     "Success never removes or replaces the one runtime wrapper evaluation fixed
     below." — **both deleted**.
   - "A source call to the unlabelled `main` uses this ordinary judgment; a
     kind-declaring entry remains uncallable under [FN-7]." — **deleted** from
     FN-8; with no entry requirement there is nothing FN-8-specific about a call
     to `main`, and FN-7 already owns callability.

   The new restriction paragraph of §3.3 is added after "There is no executable
   ordinary-callee prologue."

**[FN-9]** — three edits.

1. Structural pass spellings, exactly as FN-8.
2. The disclaimer sentence

   > The final `check` is proposition syntax only: its message, clause-local
   > spelling, and sharing have no identity, it contributes no `traps`, it never
   > executes, and it emits no [DIAG-3] record.

   becomes

   > The final `holds` states the declared proposition; its clause-local spelling
   > and sharing have no identity.

   The deleted clauses do not disappear — they move up into [OP-5], where they
   now hold for every position. **The disclaimer existed only because the syntax
   lied; once the syntax stops lying, FN-9 stops needing it.** That is the single
   clearest measure of what this proposal buys.
3. The RelationTemplate exclusion list "It excludes binder spelling, let spelling
   or sharing, message bytes, clause-local NodePaths, and callee-instance
   identity" drops "message bytes" — there are none. The occurrence identity
   `(concrete function instance, ensures_block NodePath, 0)` is unchanged.
   The sentence "It is neither an executable epilogue nor a trusted assertion,
   and it is absent from `fn_sig`, contract members, system-operation
   declarations, and every dynamic-boundary surface" keeps its first three
   exclusions; "and every dynamic-boundary surface" becomes vacuous and is
   deleted rather than kept as decoration.

**[EFF-2]** — one deletion. "The retained program-start check [PROG-3] and any
future gated adapter check [GATE-1] belong to those dynamic boundaries, not to
an ordinary source call or the callee's exhibited row." → "Any future gated
adapter check [GATE-1] would belong to a dynamic boundary, not to an ordinary
source call or the callee's exhibited row." The contract sentence ("An optional
`requires` block is a checked callable-boundary obligation… neither contributes
a read, write, allocation, external, blocking, or trapping category") is
unchanged in bytes and becomes unconditionally true: after this change no
declaration can trap through a construct its effect row does not name (§1.3a).

**[ENT-3.S4]** — the justification loses its disjunct.

> S4 is the admitted-body axiom justified by every ordinary caller's static
> discharge or the successful dynamic boundary check [PROG-3, GATE-1]; no
> callee-entry prologue executes.

becomes

> S4 is the admitted-body axiom justified by every ordinary caller's static
> discharge alone: [FN-8] admits a `requires_block` only on a non-entry
> declaration, so every execution of a body carrying a requirement is reached
> through a source call edge that discharged that exact instantiated goal in the
> caller's pre-transfer state. No callee-entry prologue, wrapper, or dynamic
> boundary supplies the axiom.

**The soundness argument is a closed case analysis, and it is the strongest
technical content here.** A function body executes only by being reached, and
there are exactly two ways to reach one: (a) an ordinary source `call` edge, or
(b) program start invoking the [FN-7] entry body [PROG-3]. [FN-8] makes (a)
discharge the instantiated goal statically before transfer, with no fallback.
The restriction makes (b) impossible for any body carrying a requirement. There
is no third route: `requires` is absent from `fn_sig` and cannot discharge a law
[FN-8, FN-4]; [DIAG-2] states that lowering "emits no contract or conformance
object and obtains no dispatch target"; there is no function-pointer, closure,
or `unsafe` surface in the language. A requirement-bearing function with no call
edge is never executed at all, and S4 "authorizes source checking only" [FN-8],
so an unexecuted body's axiom has nothing to be wrong about; its [FN-9] summary
is consumable only at a call that does not exist. The axiom is therefore
justified by a finite, complete, statically checked premise set — strictly
stronger than today's disjunctive justification, which leans on a runtime branch
to cover a case the restriction removes.

**[PROG-3]** — the wrapper paragraph collapses to one sentence.

> If the entry has no [FN-8] requirement, the implementation then invokes its
> body once. If it has one, the compiler-owned entry wrapper first evaluates
> that concrete complete goal exactly once … A false result emits the final
> `check_stmt`'s exact [OP-5, DIAG-3] trap record, invokes the body zero times,
> transfers no source owner to it, and follows [EFF-4] without a second cleanup
> path. The implementation evaluates the expression directly in the sole entry
> wrapper … This rule governs both the unlabelled no-input entry and the
> `command` entry. A source call to the unlabelled entry is not program start and
> instead follows [FN-8]'s ordinary static discharge.

becomes

> The implementation then transfers every declared standard-input owner exactly
> once and invokes the body once. [FN-8] admits no requirement on the entry, so
> program start evaluates no goal and creates no wrapper check, helper, duplicate
> body, or second external entry. A source call to the unlabelled entry is not
> program start and follows the ordinary [FN-8] call judgment.

The following marked-entry paragraph ("For a marked entry, the [CLM-3, FN-8]
source-acceptance judgment additionally evaluates the concrete requirement
proposition in the existing U proof state…" through "…no fabricated call,
adapter, helper, or second body participates") is **deleted entirely**. Start
failure, `ExitStatus` mapping, trap termination, and record partitioning are all
unchanged.

**[DIAG-2]** — three edits.

1. "its requirement occurrence `(concrete callee instance, final-check NodePath,
   conjunct ordinal 0)`" → "final-proposition NodePath".
2. "…and the one retained program-start goal evaluation when the entry has a
   requirement" — **deleted** from the retained-representation list.
3. The two contract sentences

   > The final check inside a `requires` block is not an ordinary-callee check:
   > its condition is represented by the GoalTemplate, and an executable retained
   > check exists only for program start [PROG-3] and a later implemented gated
   > boundary [GATE-1].
   > The final check inside an `ensures` block is represented only by its verified
   > RelationTemplate, selected-exit judgments, and derivations; it is never an
   > executable retained check.

   become one:

   > A contract proposition is never an executable operation. A `requires`
   > proposition is represented only by its GoalTemplate and the discharged-goal
   > derivations of its call edges; an `ensures` proposition only by its verified
   > RelationTemplate, selected-exit judgments, and derivations. Neither produces
   > a checked-program operation, a `retained`/`eliminated` disposition, or a trap
   > record.

**[DIAG-3]** — two deletions, no additions.

1. From the `node_path` sentence, drop the first alternative: "the final
   `check_stmt` whose complete goal fails at program start [OP-5; FN-8,
   PROG-3];". The remaining alternatives — `claim_stmt` for [CLM-1], and the
   operation `call`/`infix` node for a table-operation contract check and the
   [SYS-8] range validation — are unchanged.
2. Delete "For an [FN-8] program-start goal, `rule_id` is `OP-5` and `message` is
   the final `check_stmt`'s STRING value decoded by [FORM-5]." The CLM-1 sentence
   and the empty-message default are unchanged.

### 3.5 What the [DIAG-3] record carries at a program-start failure

Under this proposal, **nothing: there is no such failure, because there is no
such check.** That is the honest answer and the point of the exercise.

The brief asked me to work out what the record would carry if the message were
removed but the entry check kept, and the answer matters for §4, so here it is
worked out. It would be:

```text
{"rule_id":"FN-8","message":"","function":"main","node_path":[…]}
```

`rule_id` must change from `OP-5` to `FN-8`, because [DIAG-3] defines it as "the
exact numbered rule whose runtime condition failed" and once the terminal is not
a check statement, OP-5 owns nothing at runtime — it is a typing rule. `message`
is the empty string by [DIAG-3]'s existing default for a compiler-generated check
without a rule-specific message; no new rule is needed. `function` and
`node_path` are unchanged, and together they identify the failing requirement
exactly: one function, one final proposition node.

**Is losing the writer's message a loss or a gain?** Both, unequally, and the
measurement decides which dominates. It is a gain in 101 of the 104 contract finals, because
in those places the STRING is not merely unused but actively misleading: it
looks like something a reader will see, and it cannot be. It is a real loss in
the remaining places — see §8.2, where I do not soften it.

### 3.6 The three positions

| position | today | proposed |
|---|---|---|
| **ordinary call** | caller-side compile-time obligation; substituted goal judged in pre-transfer state; no prologue, no fallback | **unchanged in substance.** Only the terminal's spelling and the occurrence anchor's name change. |
| **program entry** | compiler-owned wrapper evaluates the goal after input setup, before owner transfer; false → [OP-5] trap record with the writer's STRING, body invoked zero times; marked entries additionally discharge in U | **rejected at compile time.** FN-8 hard error at the `requires_block` node. Repair: move the requirement to an internal callee; branch on the entry's own external precondition and return the domain's error value [patterns P12]. |
| **contract member `fn_sig`** | `requires` absent from `fn_sig`, cannot discharge a law [FN-4]; contract/refinement support DEFERRED. `ensures` absent from `fn_sig`, contract members, and system-operation declarations | **unchanged, and the deferred work gets easier.** Today a `fn_sig` requirement would have to answer "does it execute at a conformance boundary?"; after this change there is no such question to answer — a requirement is a caller obligation, full stop, and the deferred delta shrinks to name resolution and law interaction. |

### 3.7 Effect row

No rule text about effect contribution changes, and that is the point:
[EFF-2] already says the blocks contribute nothing. What changes is that the
statement becomes true without a carve-out.

- A declaration's `traps` category is contributed only by body occurrences.
  After this change there is no path by which a declaration whose row omits
  `traps` can abort the process, closing §1.3a.
- `tests/conformance/cases/fn8-neg-requires-missing-traps.wf` is **unaffected**
  despite its name. Its callee `fn bounded(x: own i32) -> own i32 pure requires {…}`
  is already accepted as `pure`; the rejection is of the *caller*, whose body
  contains a `claim` and therefore exhibits `traps` while declaring `pure`. The
  case tests [CLM-1]'s effect contribution, not the requirement's. Verdict and
  cited rule are unchanged; only its final's spelling migrates.
- `holds` contributes nothing to any category in either block, in either
  position — a single sentence in OP-5 now says so once for both.

### 3.8 One invariant this buys

After this change, **no writer-authored STRING value is observable at runtime
anywhere in the language.** Check it exhaustively against the STRING sites:
`doc STRING` is compile-time data; `claim n: e because STRING` — [DIAG-2] states
the justification "is compile-time data and does not appear in the record", and
the CLM-1 record's `message` is the claim's IDENT spelling; the contract final's
STRING is deleted here; `law` arguments are IDENTs or literals. Nothing else in
[GRAM-2] takes a STRING.

That is a checkable, one-sentence property of the surface, and it is the
strongest statement of what the owner's ruling means in practice: a contract
generates no runtime code, and no prose a writer types can reach an operator by
accident. §8.1 states the price this invariant charges elsewhere.

---

## 4. Composing with the FFI destination

The owner's second ruling removes the entry case from scope and asks whether
anything here would have to be undone when a foreign-boundary specification
arrives. Taking the questions in order.

**Nothing in this proposal encodes positional execution.** That was the design
constraint and it is met by construction: `holds e;` has one meaning in one
place — it states a proposition — and no rule anywhere makes its correctness
depend on which position it occupies. Compare what is being removed: today OP-5
needs two sentences to say the same syntax means different things in a
`requires` block and an `ensures` block, and FN-8 needs a further clause to say
it means yet a third thing at program start. Those three readings are exactly
the "executes here, not there" shape, and all three go.

**What survives an FFI boundary contract, unchanged.** The grammar, `holds`, the
block shape, the FN-8 structural pass, the GoalTemplate, alpha-expansion, goal
identity and its no-normalization rule, the pre-transfer call-site discharge,
[ENT-3.S4], the FN-9 RelationTemplate and its view machinery, and the [EFF-2]
non-contribution. That is the entire proposal minus one paragraph.

**What an FFI boundary would add, and it is an addition, not an undo.** A
foreign boundary declares its own validation contract. The natural shape given
this proposal: the boundary declaration — not the callee, not `requires` —
names the predicate it validates and the diagnostic it emits on failure, and the
compiler derives one wrapper from that boundary declaration. Program start
becomes one instance of that form, exactly as the owner described. The restriction
of §3.3 is then relaxed by a later version's rule, not reversed: the entry is
still not a `requires` site; it is a boundary site, and the boundary form is what
carries a message channel. Nothing written above has to be unwritten. The one
sentence in FN-8 that would be replaced is the restriction's closing clause
("this version defines no validated start-time or foreign callable boundary"),
which exists precisely so that the gap is stated rather than silent.

**What justifies [ENT-3.S4] then.** In this proposal S4 rests on the closed case
analysis of §3.4: the only two routes into a body are an ordinary call edge and
program start, and the restriction empties the second for requirement-bearing
bodies. An FFI boundary adds a third route — a foreign caller. S4's justification
must then be extended, and the honest form is the one the mcts_mem node already
insists on: the boundary contract's validated predicate must *imply* the callee's
requirement at the boundary, checked statically at the boundary declaration, so
the boundary's own runtime check discharges the callee's goal before any foreign
value reaches a body. That is a static implication check plus one runtime check
owned by the boundary — not a resurrection of the entry wrapper, and not a
`requires` that executes. Notably it is strictly better than today, where the
entry wrapper evaluates *the callee's own* predicate and the mcts_mem
`callee-entry-prologue` alternative was rejected in August for exactly the reason
that a prologue "let a helper hide a protected leaf behind a runtime trap".

**What happens to the now-unreachable machinery: delete, do not reserve.**
This is a position and I will defend it. [OP-5]'s program-start trap semantics,
the [DIAG-3] `rule_id: OP-5` row, the [FN-8] program-start clauses, and
[PROG-3]'s wrapper paragraph are deleted outright. Three reasons.

1. *Retaining inert operative text is the defect under investigation.* The whole
   study exists because an `else trap "msg"` half sat in the grammar doing
   nothing while looking like it did something. Answering that by leaving a
   dormant wrapper paragraph in FN-8 reproduces the defect at rule scale.
2. *Nothing is lost.* `spec/` retains history by design: at activation the
   outgoing bytes are archived flat as `spec/kernel-spec-v0.32.md`, and that
   archive is absolutely immutable and hook-enforced. The FFI author recovers the
   exact wrapper text by reading v0.32. A dormant clause buys nothing that the
   archive does not already provide, and costs every future reader a paragraph
   they must determine is inert.
3. *The repository already has a precedent for a stated gap.* [FN-7] carries
   `service` and `embedded` as "reserved spelling; no form defined", with a hard
   error on use. The restriction's closing sentence is the same device: it says
   the capability is absent and names where it will live. That is how a gap is
   made visible instead of half-implemented.

The `[GATE-1]` cross-references stay where they are. GATE-1 is a stub rule about
gated toolchain operations, not about contracts, and the two surviving mentions
(in FN-8 and EFF-2) become ordinary forward pointers with no operative clause
hanging off them.

---

## 5. Migration recipe and its measured cost

**Step 1 — respell every contract final.** One substitution, no judgment calls:

```
s/^(\s*)check (.*) else trap "[^"]*";$/\1holds \2;/
```

- `.wf` under `tests/` + `research/`: **104 finals in 88 files.** Removes 3,253
  bytes of tail, adds 104 bytes (`check`→`holds` is +1 B each). **Net −3,149
  bytes.**
- Two blocks have no final (§2.2) and the substitution correctly leaves them
  alone; both are FN-8 negative cases whose verdicts are unchanged, since the
  rule they violate — a missing final — is unchanged in substance.
- The 13 stray body-position checks (§2.2) are *not* migrated by this
  substitution and must not be: they are already-invalid v0.32 source, and their
  correct disposition is deletion or conversion to [CLM-1] `claim`, which is
  batch 0071's unfinished cleanup rather than this proposal's work.
- Compiler-embedded Rust sources: **248 occurrences in 17 files**, 5,391 bytes of
  tail removed.
- Every canonical-form output changes, so `tests/conformance` canonical fixtures
  and `compiler/src/syntax/parser/finalize/tests/corpus_shape.rs` re-baseline.

**Step 2 — the three entry requirements.** Not mechanical; each needs a decision.

| case | disposition |
|---|---|
| `tests/conformance/cases/fn8-trap-requires-false.wf` (`expect: trap`) | **Retires with the machinery.** Its entire subject is the program-start requirement trap; under the restriction the source is a compile-time rejection, not a trap. Either delete the case or convert it to `expect: reject, rule: FN-8` pinning the new restriction — a different case testing a different thing. |
| `tests/conformance/cases/clm3-neg-generated-wrapper-check.wf` (`expect: reject FN-8`) | **Loses its discriminating power.** Its doc reads "The generated command-entry wrapper would dynamically pass an opaque conjunction whose two comparisons are true, but U does not compose that atomic goal." With no wrapper the source is rejected earlier, by the restriction, and the case stops proving that U-non-composition beats the wrapper. Retires. |
| `tests/conformance/cases/clm3-pos-transitive-value-branch.wf` (`expect: run, exit 0`) | **Rewritable.** Its doc lists several subjects; "proves its entry requirement in U" is one leg. Move the requirement to an internal callee and the other legs (value_if candidate/prior, U-verified generic relay, seedless mutual SCC) survive intact. |

All three are protected conformance evidence. Under `CLAUDE.md` this is an
exact-before/after audit, owner explanation and approval, and an approval-ledger
entry — **not** something a batch may do on its own authority, and it is the
single heaviest item in this proposal.

**Step 3 — compiler tests.** Twelve entry-with-`requires` sources across six
files (§2.3) are deleted or moved to an internal callee. The most consequential
is `compiler/src/backend/tests.rs:1084-1107`, whose own doc comment states the
stakes:

> v0.32 retires the body `check` statement, so the entry requirement's final
> `check_stmt` is the sole remaining [OP-5] record carrier whose `message` is a
> writer-chosen STRING — a migrated body check becomes a [CLM-1] record whose
> `message` is an IDENT and can carry neither byte.

That test dies. §8.1 explains why its death is the sharpest cost in the proposal.

**Step 4 — docs.** `docs/why-whitefoot.md` (1 occurrence) and
`docs/patterns.md` (P12's "process-entry wrapper check" clause becomes a
statement of the rule rather than advice against a legal move).
`docs/done/0038-*.md` is a closed record and is not edited.

---

## 6. Compiler change inventory

Named from reading the tree, with the pass each belongs to.

**Grammar generation (a generated-table change, not hand editing).**
- `compiler/src/bin/grammar_tables/model.rs:121-123` — the fixed-atom table.
  Remove `("check","Check")` and `("trap","Trap")`; add `("holds","Holds")`.
  `("else","Else")` stays: `if_stmt` uses it. Note the file's own comment at
  line 142 — "`else` already exists (check_stmt)" — becomes stale and must move
  its citation to `if_stmt`.
- `compiler/src/bin/grammar_tables/main.rs:79` — the production-name list.
  **This is the one place where the change is not routine.** `generated.rs`
  documents its own ordering rule: "The declaration order is the stable dense
  table index and is historical, not derived: a production keeps its slot and a
  new one appends." No production has ever been *removed* — v0.32 removed
  `check_stmt` from the `stmt` alternation but kept the production — so the
  generator's removal path is unexercised. Either the `CheckStmt` slot is retired
  (leaving a hole) or `HoldsStmt` appends and `CheckStmt` is dropped; whichever,
  this needs the generator's behavior verified rather than assumed.
- `compiler/src/syntax/grammar/generated.rs` (313 KB, generated) — regenerated.
- `compiler/src/syntax/terminal.rs` — fixed-terminal predicates follow the atom
  table; the [FORM-3] IDENT exclusion is derived from it, so the two widenings
  and one narrowing of §3.1 fall out without hand-written lists.
- `compiler/src/bin/grammar.rs` — the native grammar verifier that reuses the
  compiler's lexer and parser; the mandatory pre-proposal check per `CLAUDE.md`.

**Parse and canonical form.**
- `compiler/src/syntax/parser/finalize/canonical/format.rs:35` —
  `Production::CheckStmt` in the fixed-shape list becomes `HoldsStmt`. One-line.
- `compiler/src/syntax/parser/finalize/canonical/render.rs` — token-generic; no
  structural change expected, to be confirmed.

**Resolution.**
- `compiler/src/resolution/engine/admission.rs:129-150` — the contract-entry
  admission, including `ClauseEntryKind::Check` (rename) and the v0.32 comment
  block that explains why the production is contract-only. That comment becomes
  the explanation of a deleted production and is rewritten or removed.

**Semantic — the FN-8/FN-9 structural pass.**
- `compiler/src/semantic/check/requires.rs` (972 lines) — `clause_entry_statement`
  (l. 290-302) and `validate_clause_statement` (l. 725-741) match
  `Production::CheckStmt`. Rename only; the whitelist discipline recorded in
  mcts_mem ("the clause statement filter was inverted from blacklist to
  whitelist so unknown statement kinds fail closed") is preserved and is why this
  is safe.
- `compiler/src/semantic/check/control.rs:265-300` — the arm that builds a
  `TrapSite { rule_id: "OP-5", … }` for the contract final. Under this proposal
  it stops constructing a `TrapSite` at all.
- `compiler/src/semantic/goal.rs:15-23` — `CheckedRequirement.trap: TrapSite` is
  **deleted**; the struct keeps its `GoalTemplate` and its occurrence `NodePath`.
  The doc comment "Keeping the complete record lets entry lowering survive
  removal of the legacy executable clause statements without re-reading source"
  describes machinery that no longer has a consumer.
- `compiler/src/semantic/model.rs:664` — `TrapSite` survives for CLM-1, OP-2, and
  OP-4 table checks; it loses its only writer-STRING producer.

**Semantic — provenance.**
- `compiler/src/semantic/provenance.rs:1603, 3865-3877, 4739, 4893` —
  `entry_requirement: Option<RequirementOccurrence>` and the PRV-3 entry-own
  requirement-bridge path become dead and are removed. The inherited-bridge path
  through an entry-body call (PRV-2) is untouched, matching the FN-8 edit of §3.4.

**Lowering and backend — the largest deletion.**
- `compiler/src/lowering/builder/entry_goal.rs` — **600 lines, deleted whole.**
- `compiler/src/lowering.rs` — `IrEntryGoal`, `IrEntryGoalDefinition`, and their
  wiring. 49 non-test references across `lowering.rs`, `lowering/builder.rs`,
  `backend/emitter.rs`, `backend/target.rs`, `backend/emitter/system.rs`.
- The wrapper's inline-goal emission, its intrinsic declaration ordering, and its
  single-body-call invariant all go with it.

**Tests.** `compiler/src/backend/tests/requires.rs` (entry-wrapper behavior,
5 embedded entry sources), `compiler/src/backend/tests.rs:1084-1107` (the OP-5
record shape and FORM-5 decoding, see §8.1), `compiler/src/lowering/tests.rs:738-750`
(asserts `goal.trap().rule_id == "OP-5"`), `compiler/src/resolution/tests.rs:699,719`,
`compiler/src/semantic/tests/strict.rs:260,401`,
`compiler/src/semantic/tests/requires.rs:241`, plus the corpus-shape and
canonical-form baselines.

---

## 7. Design-memory check

Consulted: [mcts_mem/whitefoot/checks-and-proofs/requires-entry-contract.md](https://github.com/mbbill/Whitefoot/blob/0f22b026b062c36ef2f8ce9c725ee4307e1fda64/mcts_mem/whitefoot/checks-and-proofs/requires-entry-contract.md), its
`.alt/recognizer-driven-elision.md`, its child
`requirement-enforcement.md` and that child's
`.alt/callee-entry-prologue.md`; plus
`obligation-discharge/writer-trap-surface.md` and its
`.alt/dual-check-and-claim.md`.

**No rejected alternative is re-proposed here.** The two recorded rejections
under this node are about *enforcement mechanism*, not spelling:

- `recognizer-driven-elision` (replaced 2026-07-11, `6f031496`) — a single
  pattern recognizer as elision authority. This proposal touches no elision
  authority and keeps the obligation-derived model whole.
- `callee-entry-prologue` (replaced 2026-08-10, `441cd5b8`) — an unconditional
  executable prologue on every invocation. This proposal moves *further* from
  it, not toward it: it removes the last executable requirement evaluation in the
  language.

**One overlap, stated plainly.** The live node records a 2026-07-11 rationale
that cuts against §3.3:

> callee-boundary coverage was selected over reliance on known callers because
> the direct-C entry path showed entry enforcement is necessary — a caller-proof
> scheme leaves foreign entries unprotected

The restriction of §3.3 *is* a caller-proof scheme, and it does leave a foreign
entry unprotected. What changed since that rationale is real but partial: the
2026-08-10 move already replaced the prologue with pre-transfer proof, leaving
program start as the single remaining dynamic remnant of a design whose main
mechanism had already been retired; and the owner has now named an FFI boundary
contract as the destination for exactly the case the rationale was defending.
What has *not* changed is that the FFI contract does not exist yet. I record this
as an overlap with a live rejection rationale, not as a refutation of it, and
§8.4 prices it.

**One deferral this study closes.** The node's own 2026-07-11 statement — "the
`requires { let* check }` block spelling is minimality-selected and
R3-provisional pending a writer-tier comparison against a credible
single-predicate alternative" — anticipated precisely this comparison. Note
carefully what it asked for and what this proposal delivers: it asked for a
comparison against a *single-predicate* alternative, and this proposal keeps the
block with locals and changes only the terminal. §8.3 treats that as the gap it
is.

---

## 8. What this costs

### 8.1 It relocates the inert surface rather than removing it

After this change, examine every field [DIAG-3] can emit. `rule_id` is a fixed
rule name. `function` is a source function IDENT, `[a-z][a-z0-9_]*`. `node_path`
is decimal digits. `message` is one of: a rule-fixed constant (`integer
overflow`), a [CLM-1] claim's IDENT spelling, or the empty string. **Not one of
them can contain a byte that needs JSON escaping.**

[DIAG-3] nonetheless keeps a full paragraph specifying canonical encoding —
`"` → `\"`, `\` → `\\`, LF → `\n` — and [FORM-5] keeps a decode path feeding it.
Both become unreachable from any source program. The compiler's own comment
already names the STRING final as "the sole remaining [OP-5] record carrier whose
`message` is a writer-chosen STRING", and when it dies, the end-to-end test at
`compiler/src/backend/tests.rs:1084` dies with it; only a synthetic unit test that
constructs a `TrapSite` by hand can still exercise the encoder, on inputs no
source can produce.

So the honest summary is: **I delete an inert half of a statement and create an
inert paragraph of a diagnostic rule.** A rival is entitled to say I moved the
lie one level down, and that a proposal claiming to make the surface honest
should have said what happens to the escaping rules. My answer — that the
paragraph should then be reduced to the reachable character set, which is a
[DIAG-3] edit this proposal does not make — is a repair I did not do, not a
defense.

### 8.2 It deletes the only prose channel inside a contract block

[FORM-4]: no comments. [FN-8] and [FN-9]: `doc` rejected inside the block. So the
trap STRING is, today, the only English a writer can put next to a contract
predicate — and 53 distinct strings across the 104 finals show writers using it that
way, several of them genuinely explanatory (`"append result exceeds
destination"`, `"read bits result exceeds mask"`, `"the copy stops at the source
length"`-style intent restatements).

**The reader served worse is the human approver**, and that is the worst possible
reader to serve worse in this project. Whitefoot's premise is human approval of
AI-written code. A written contract has two independent expressions of intent
today — the predicate and the sentence — and a reviewer reading a diff can notice
when they disagree. `holds ile<u64>(len(text), capacity);` next to
`"append result exceeds destination"` is a redundancy check on the predicate's
direction and operands. After this change the reviewer sees the predicate alone
and has nothing to compare it against. That is the loss of a redundancy channel
in exactly the review step the language exists to support, and the measured
observation that most messages are labels does not rescue the ones that are not.

The obvious repair — admit `doc STRING ";"` as an entry of the block, which
[GRAM-2] already parses and only FN-8/FN-9's structural pass rejects — is cheap
and directly answers the objection. I have deliberately **not** folded it in,
because it enlarges a proposal whose stated stance is minimal respelling, and
because a `doc` inside a contract raises its own question (is it the block's
documentation, or the proposition's?). A rival will propose it, the rival will be
right that it is needed, and I will have no argument except scope.

### 8.3 It does not fix what the R3 register actually flags

The register line reads: "the `requires { requires_entry* }` **surface spelling**
with its FN-8-checked ordinary-let/final-check subset". The mcts_mem statement
asks for "a writer-tier comparison against a credible **single-predicate**
alternative". This proposal keeps the block, keeps the locals, keeps the
exactly-one-final rule, and changes one keyword and one tail. Measured against
the question that was actually deferred, it is cosmetic.

Specifically unfixed:

- **The block's locals are justified for a bare majority, and for nobody else.**
  §2.5 measured it rather than asserting it, and the number is uncomfortable:
  **37.5% of contract blocks in the discriminating corpus declare no local at
  all.** For 24 of 64 blocks the shape this proposal preserves is three lines of
  ceremony around a single expression, and a single-predicate form would be
  strictly shorter with nothing lost. The premise I was handed — "the locals
  exist because real predicates need decomposition to stay readable" — is true of
  62.5% of blocks and false of the rest, and this proposal charges the whole
  corpus for the majority's need.
- **The two blocks still have different shapes.** `requires {` versus
  `ensures <selector> {`. Two constructs that are now definitionally the same
  kind of thing — a proposition over a boundary — still read differently.
- **Exactly one final forces conjunction into `band`.** A writer stating two
  independent preconditions must write `band(a, b)`, and [FN-8]'s no-composition
  rule then makes the whole tree one indivisible goal. Allowing several `holds`
  entries would be the natural reading of a proposition list and would compose
  with the signed decomposition set; this proposal does not touch it.
- **`fn_sig` still cannot carry a contract**, so contracts and refinement remain
  DEFERRED. §3.6 argues the deferred work gets easier, which is not the same as
  doing it.

### 8.4 The restriction is a capability removal wearing a respelling's clothes

§3.3 removes a working feature. It is the largest semantic change in this
document and it is not a spelling question at all; it arrived from an owner
ruling mid-study and I have folded it in without independently establishing that
it is right.

Three specific costs.

- **It makes a class of program unwritable.** An entry that validates its own
  standard inputs and refuses to start cannot be written. The [patterns] P12
  repair — branch and return the domain's error value — is better engineering,
  but P12 is guidance about *protected* accesses on external input, and the
  restriction generalizes it to every entry precondition, including ones with no
  protected leaf anywhere near them.
- **The corpus argument is weak and I should not lean on it.** "Zero real
  programs use it" is true (§2.3), and it is also what you would expect of a
  capability that has existed since 2026-07-11 in a tree whose entries are almost
  all test harnesses. Absence of use in a five-week-old feature is not evidence of
  absence of need.
- **It contradicts a live, evidence-dated rationale** (§7): "a caller-proof scheme
  leaves foreign entries unprotected". The restriction reinstates exactly that
  exposure and puts nothing in its place until the FFI specification exists. The
  spec sentence I propose ("this version defines no validated start-time or
  foreign callable boundary") makes the hole *visible*, which is the honest
  minimum, but visible is not covered.

**Would I defend the restriction on its own merits?** Split answer.

For `ensures`: yes, unconditionally, and it is not even a restriction there —
an `ensures` never executed in any position, so nothing is removed.

For `requires`: **no. I would defend it only as scaffolding, and only paired with
a commitment.** The program entry is a real trust boundary with the outside
world; that is not a doctrinal claim, it is what "external" means. Removing its
only validation with nothing in its place trades a known, narrow runtime cost
(one evaluation, once, before the body) for a gap. I would land it only with the
FFI boundary contract as a named, sequenced successor in `docs/roadmap.md`, and
with the FN-8 sentence stating the absence in the spec so that no reader mistakes
silence for coverage. If the FFI work slips, the restriction should be revisited
rather than quietly accepted as the permanent shape.

### 8.5 Keyword churn nobody asked for

Three words move across the [FORM-3] boundary (§3.1). `holds` leaves IDENT —
measured zero collisions, fine. `check` and `trap` **enter** IDENT, which widens
the accepted set: `let trap = 1_u64;` and `fn check(…)` become legal programs
that are rejected today. Nothing in the problem statement asked for that, no
experiment needs it, and it is the kind of incidental acceptance change that a
spec review is right to challenge. The alternative — keeping `check` and `trap`
reserved with no grammar use — is worse, because a reserved word with no
production is precisely the inert surface under attack. There is no free option
here, only a smaller and a larger one, and the proposal takes the widening
without having justified it.

### 8.6 The strongest argument a rival stance would make

**"Your terminal keyword is now pure ceremony — delete it instead of renaming
it."**

The rival's case: after this proposal the block head already states the modality
(`requires` / `ensures`), the block's shape already fixes the final position, and
the terminal contributes no information whatsoever. `holds` is a word the writer
types 104 times to say what the enclosing brace already said. A rival proposing
that the final entry is simply an expression — or that the whole block collapses
to `requires expr;` with locals hoisted or forbidden — deletes a production
instead of renaming one, mints no atom, causes no [FORM-3] churn, and answers the
R3 register's actual question about the *block* rather than dodging it.

**And §2.5 hands the rival its evidence.** 37.5% of contract blocks declare no
local, so for more than a third of the corpus the rival's collapsed form is not
merely smaller — it is exactly as expressive, line for line. The rival can go
further: offer `requires expr;` as the no-local form and keep the block only
where a local appears, which costs one extra production, serves both populations
at their measured sizes, and still deletes `check_stmt`. My objection to that —
two spellings for one construct is a [FORM-1]/[META-2] violation — is the correct
objection and is the only thing standing between this proposal and a better one.

My two defenses of the keyword are real and both are weak. "The keyword is a visual anchor
telling the reader which line is the proposition" is a claim about readability I
have not measured, in a document that measured everything else. "FN-8's
missing-final rejection needs a node to name" is false as stated: FN-8 already
reports the `requires_block` node for an empty or all-let block, and could do so
uniformly.

A second rival line, nearly as strong: **an obligation and a guarantee are
different modalities and should not share a terminal.** `requires` states what a
caller must establish; `ensures` states what a callee will deliver. Giving both
the same word `holds` asserts a symmetry that the rest of the specification does
not have — FN-8 and FN-9 differ in their admitted operand sets, their
substitution rules, their view machinery, and their failure owners. A rival who
spells them differently makes the two rules' genuine asymmetry visible at the
call site, and can point out that this proposal's central virtue — deleting FN-9's
"never executes" disclaimer because the syntax stopped lying — is achieved by
making two different things look identical.
