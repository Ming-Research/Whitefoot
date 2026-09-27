# Division dissolution — exact v0.32 candidate rule edits

Status: DELTA TEXT FOR LEAD INTEGRATION (batch 0071, task #48). This file is
delta input to the single v0.32 candidate; the lead integrates it and it is
superseded by the activated specification. Nothing here changes
`spec/kernel-spec.md`; every byte lands only through the owner's exact-byte
approval. Basis revision: active v0.31 at `spec/kernel-spec.md` (activation
commit `eb8e8634`).

Template: `research/investigations/arith-dissolution/SPEC-DELTA.md` (the
superseded v0.31 constant-operand overflow delta). This delta follows its
recipe: one new [ENT-6] obligation family, base discharge only, default-off
compiler switch, both-direction tests, migration inventory, proposed
conformance list.

## Basis

Recorded measurement, [mcts_mem/whitefoot/checks-and-proofs/obligation-discharge.md](https://github.com/mbbill/Whitefoot/blob/0f22b026b062c36ef2f8ce9c725ee4307e1fda64/mcts_mem/whitefoot/checks-and-proofs/obligation-discharge.md) Facts, 2026-08-08 (`5188548f`):

> the entailment fragment admits exactly four term forms — a tracked place,
> a length term, a constant, and the distinguished zero term — and no
> arithmetic term, so an addition-overflow goal is not expressible as an
> atomic fact at all, while the bounds goal **and the zero-divisor goal**
> both are.

The zero-divisor goal is stronger than the overflow goal was: `d != 0` is one
[ENT-2] atomic disequality against Z, expressible for **every** divisor the
fragment reads as a term or a constant, with no constant-operand restriction.
[ENT-4]'s derivability rule already closes it: `a != b` is derivable when a
disequality is present or when `a - b <= -1` or `b - a <= -1` is derivable, so
an executed `igt(d, 0_T)` check, an `ine(d, 0_T)` claim, a counted binder, a
nonzero constant, or any transitive bound that puts the divisor strictly on
one side of zero all discharge it.

The signed-overflow goal does not follow. Its safe condition is

```
dividend != iK::MIN  or  divisor != -1
```

a **disjunction**. [ENT-4]'s L0 component is a difference-bound and
disequality fragment closed under transitivity, disequality strengthening,
and subsumption; it has no disjunction, no case split, and no way to state
"one of these two atoms holds". A conjunctive over-approximation would be
unsound as a language rule in the wrong direction: it would reject correct
programs (every `p / q` whose operands nothing bounds), which is exactly the
outcome the dissolution recipe must not produce. Honest answer: **that trap
stays** wherever the disjunction is real.

It is not always real. One disjunct is statically true whenever

- T is unsigned (`iK::MIN / -1` cannot be written at all — [OP-2] already
  records that `DivOverflow` is statically unreachable for unsigned T); or
- the divisor operand reads as an [ENT-2] constant other than `-1`; or
- the dividend operand reads as an [ENT-2] constant other than `min(T)`.

and in the two remaining constant cases the disjunction collapses to one
expressible disequality:

- divisor is the constant `-1` → the goal is `dividend != min(T)`;
- dividend is the constant `min(T)` → the goal is `divisor != -1`.

Both are ordinary [ENT-2] disequalities between one term and one constant
term, and [ENT-4] derives them from the strict bounds the fragment already
carries. So the expressible class is exactly: **unsigned T, or at least one
constant operand.** That is the same shape [OP-2]'s constant-operand overflow
class already has, widened by the unsigned case that the zero-divisor goal
alone can carry.

Design consequence: extend the obligation-discharge model from subscripts and
constant-operand arithmetic to the divisor class. A class site's discharge
removes **both** of its runtime tests, so its checked-program disposition
records real dissolution rather than a partial one; the inexpressible
signed/two-variable case keeps its trapping semantics unchanged. `/checked`,
`%checked`, `.wrap`, `.checked`, `.sat`, `ineg.*`, `iabs.*`, the shift rows,
and every bare `+`/`-`/`*` judgment are untouched.

## The class

A bare-operator `/` or `%` call is in the **divisor class** exactly when its
selected type T is unsigned, or at least one of its two operand atoms reads as
an [ENT-2] constant — an integer literal or an integer-typed named const,
judged per concrete [FN-2] instance, so a substituted generic literal or const
is a constant in each instance.

Membership does not require the divisor to be a term: a non-term divisor (a
subscripted place) leaves conjunct zero underivable with the same one-`let`
rebinding fallback [ENT-6] already states for a subscripted offset atom. This
keeps the trap surface independent of operand spelling, exactly as the
constant-operand class does.

`/` and `%` share one class and one pair of conjuncts: both rows fail on
exactly the same two inputs, so the operation identity adds nothing to the
judgment.

Attaching a new obligation family is an amendment-level accepted-set change
explicitly enumerated by [ENT-1] ("attaching a new protected family"); this
delta is exactly such an amendment.

No grammar production, token, or spelling changes; the native grammar
verifier is not implicated. All edits are semantic-rule prose plus one
operation-table effects cell.

## Rule edits

Each edit gives the current sentence(s) at the cited v0.31 line and the exact
replacement, one sentence per line, v0.31 profile style.

### 1. [OP-1] operation table — the bare div/rem row's effects cell

Current row (`spec/kernel-spec.md:721`):

```
| `/` `%` | all int T | `(T, T) -> own T` | traps |
```

Replacement row:

```
| `/` `%` | all int T | `(T, T) -> own T` | traps (outside OP-2's divisor class) |
```

(This mirrors the bare `+ - *` row already reading
`traps (outside OP-2's constant-operand class)`.)

### 2. [OP-2] — the division/remainder paragraph

Current (`spec/kernel-spec.md:844`, one line):

```
(Negation has a wrap mode: two's-complement wrapping negation is sound modular arithmetic — ledger fix 2026-07-07.) Integer division and remainder have two checkable failures: a zero divisor for all int T, and, for signed T, the single signed-overflow case `iK::MIN / -1` (LLVM sdiv/srem are UB on both); the bare `/` and `%` operators trap on either, and `/checked` and `%checked` return `Err(DivideByZero())` for a zero divisor and `Err(DivOverflow())` for signed minimum divided by negative one, else `Ok`.
```

Replacement (the leading parenthetical and the failure inventory are retained
byte-identically; only the bare-operator clause is replaced, and the class
rules follow):

```
(Negation has a wrap mode: two's-complement wrapping negation is sound modular arithmetic — ledger fix 2026-07-07.) Integer division and remainder have two checkable failures: a zero divisor for all int T, and, for signed T, the single signed-overflow case `iK::MIN / -1` (LLVM sdiv/srem are UB on both); `/checked` and `%checked` return `Err(DivideByZero())` for a zero divisor and `Err(DivOverflow())` for signed minimum divided by negative one, else `Ok`.
A bare-operator `/` or `%` call whose selected type T is unsigned, or at least one of whose two operand atoms reads as an [ENT-2] constant — an integer literal or an integer-typed named const, judged per concrete [FN-2] instance — is in the divisor class.
A divisor-class call carries the division obligation that its divisor is nonzero and that it is not the signed-overflow case [ENT-6], judged by the same complete-state base discharge as a subscript bounds obligation.
A discharged class call returns the exact quotient or remainder with no runtime zero-divisor or signed-overflow check in any build mode, never traps, exhibits no `traps` under [EFF-2], and its checked-program disposition records the discharging derivation [DIAG-2].
A class call whose obligation the complete fact state does not discharge is a compile-time rejection citing OP-2 at that call's `infix` node, carrying the residual obligation rendered exactly per [ENT-6]; it publishes no checked program.
Its mechanical fix is a dominating `claim` of the residual [CLM-1], a dominating branch establishing it [ENT-3], or the explicit `checked` respelling.
A class call whose constant divisor is zero, or whose two constants are exactly `iK::MIN` and `-1`, instantiates a ground false conjunct [ENT-6] and is therefore rejected at every non-contradictory point; there is no accepted always-trapping bare spelling.
For a class call in a [CLM-3] demanded strict component, the same normalized obligation must additionally discharge in that function's already-computed unasserted U state [ENT-6]; a refuted or unproved strict judgment is a hard rejection citing OP-2 at the same `infix` node, carrying the same exact residual plus the strict root, concrete function instance, and `unasserted` view, and its mechanical repair is [OP-4]'s strict repair.
A bare-operator `/` or `%` call over a signed selected type both of whose operand atoms are non-constant is outside the class and retains the trapping judgment: it returns the exact quotient or remainder unless the divisor is zero or its operands are exactly `iK::MIN` and `-1`, and otherwise traps before producing a result.
That retained safe condition is the disjunction `dividend != iK::MIN or divisor != -1`, which the [ENT-4] closure cannot state as an atomic fact or derive as one; no obligation attaches there and no closure rule this specification defines would discharge one.
A zero-divisor or signed-overflow failure in one of these retained bare-operator trapping operations is a contract violation [ERR-4, SCOPE-4], not a recoverable `DivError` value, source rejection, wrapped result, saturation, truncation, or undefined behavior.
Each retained trapping call syntactically exhibits `traps` under [EFF-2], even when a proof eliminates its runtime test.
```

`spec/kernel-spec.md:843` (no wrap modes), `:845` (DivOverflow statically
unreachable for unsigned T), and `:846` (both classifications table-fixed)
are unchanged.

### 3. [ENT-6] — the obligation-family inventory

Current second sentence (`spec/kernel-spec.md:2909`):

```
This version attaches exactly two obligation families.
```

Replacement:

```
This version attaches exactly three obligation families.
```

Then, immediately after the overflow family's last sentence
(`spec/kernel-spec.md:2923`, ending `…applies identically to a subscripted
class operand.`), insert:

```
The third family: for every bare-operator `/` or `%` call in [OP-2]'s divisor class, the division obligation that the operation is defined on its operands, at that call's `infix` node.
The division obligation normalizes to exactly two conjuncts — ordinal zero the zero-divisor conjunct and ordinal one the signed-overflow conjunct — each one atomic relation over terms read from the call's operand atoms.
Conjunct zero is the disequality `d != Z`, with d the divisor operand read as a term or constant.
Conjunct one is the ground true bound `Z - Z <= 0` when the selected type T is unsigned, when the divisor operand reads as a constant other than -1, or when the divisor operand is not a constant and the dividend operand reads as a constant other than min(T).
Conjunct one is the disequality `n != min(T)`, with n the dividend operand, when the divisor operand reads as the constant -1.
Conjunct one is the disequality `d != -1`, with d the divisor operand, when the divisor operand is not a constant and the dividend operand reads as the constant min(T).
The complete-state base judgment discharges a conjunct exactly when the closed complete fact state at that node derives it [ENT-4, ENT-5], and discharges the obligation exactly when both conjuncts discharge.
Failure of that base judgment is the [OP-2] rejection; its diagnostic renders the residual of the least undischarged conjunct as exactly: the conjunct's operand atom's canonical source bytes, then ` != `, then the conjunct's constant in decimal, which is `0` for the zero-divisor conjunct.
The division family attaches base discharge only: it creates no [PRV-2] or [PRV-3] protected demand, no provenance event, and no runtime operation in this version.
An operand that is not a term or constant leaves its conjunct underivable, and the one-rebinding fallback stated below for a subscripted offset atom applies identically to a divisor-class operand.
```

Current metadata sentence (`spec/kernel-spec.md:2931`):

```
The bounds relation has one conjunct at ordinal zero; the overflow relation has its upper conjunct at ordinal zero and its lower conjunct at ordinal one.
```

Replacement:

```
The bounds relation has one conjunct at ordinal zero; the overflow relation has its upper conjunct at ordinal zero and its lower conjunct at ordinal one; the division relation has its zero-divisor conjunct at ordinal zero and its signed-overflow conjunct at ordinal one.
```

The occurrence-identity form `(concrete function instance, exact
obligation-occurrence NodePath, normalized conjunct ordinal)` already covers
the new family unchanged; a division occurrence's NodePath is its `infix`
node.

### 4. [ENT-2] — the distinguished zero term's stated uses

Current fragment (`spec/kernel-spec.md:2682`, clause (f) of the term list):

```
or (f) the distinguished zero term Z, used only to carry constant bounds and S7's exact mathematical-zero disequality.
```

Replacement fragment:

```
or (f) the distinguished zero term Z, used only to carry constant bounds, S7's exact mathematical-zero disequality, and [ENT-6]'s zero-divisor conjunct.
```

(No term form is added; the zero-divisor conjunct relates an existing term to
the existing Z.)

### 5. [EFF-2] — the body-syntactic traps clause

Current fragment (`spec/kernel-spec.md:1416`, inside the first sentence of
the body-syntactic paragraph):

```
it exhibits `traps` iff the body contains any trapping-mode operation — a bare `/` or `%`, a bare `+`, `-`, or `*` outside [OP-2]'s constant-operand class, or a `.trap` OPNAME — `check`, `claim`, or a call to any operation or function whose effect row includes `traps` (even if later proven away);
```

Replacement fragment:

```
it exhibits `traps` iff the body contains any trapping-mode operation — a bare `/` or `%` outside [OP-2]'s divisor class, a bare `+`, `-`, or `*` outside [OP-2]'s constant-operand class, or a `.trap` OPNAME — `check`, `claim`, or a call to any operation or function whose effect row includes `traps` (even if later proven away);
```

### 6. [DIAG-3] — the executed-division record

Current sentence (`spec/kernel-spec.md:2031`):

```
For an executed bare `+`, `-`, or `*` overflow, `rule_id` is `OP-2`, `message` is `integer overflow`, and `node_path` is the trapping `infix` node; such a record arises only outside [OP-2]'s constant-operand class, because a class call discharges at compile time and executes no overflow test; a bare `/` or `%` contract violation is a table-operation contract check at its `infix` node.
```

Replacement:

```
For an executed bare `+`, `-`, or `*` overflow, `rule_id` is `OP-2`, `message` is `integer overflow`, and `node_path` is the trapping `infix` node; such a record arises only outside [OP-2]'s constant-operand class, because a class call discharges at compile time and executes no overflow test; a bare `/` or `%` contract violation is a table-operation contract check at its `infix` node, and such a record likewise arises only outside [OP-2]'s divisor class.
```

### 7. [ENT-1] — the version-addition inventory

Current sentence (`spec/kernel-spec.md:2669`):

```
Beyond those classes, this specification adds only FN-9/S12, the two stated unsigned S7 relations, [ENT-6]'s constant-operand overflow obligation family, and [ENT-5]'s value-if-only delivery, and retains the provenance gate.
```

Replacement:

```
Beyond those classes, this specification adds only FN-9/S12, the two stated unsigned S7 relations, [ENT-6]'s constant-operand overflow obligation family, [ENT-6]'s divisor-class division obligation family, and [ENT-5]'s value-if-only delivery, and retains the provenance gate.
```

### 8. [CLM-3] — no edit required

`spec/kernel-spec.md:2643` already reads `[OP-4, OP-2, ENT-6]` for the strict
U re-judgment of protected obligations, which the division family joins
without a citation change. The v0.31 delta already widened it.

## Soundness notes

- Conjunct zero is an equivalence, not an approximation: the operation is
  defined on its divisor exactly when the divisor is nonzero, and `d != Z` is
  that statement. A constant divisor needs no special case — [ENT-2] interns
  a constant as a term whose implicit bounds `c - Z <= c` and `Z - c <= -c`
  make `c != Z` derivable exactly when c is nonzero, and underivable (hence a
  ground false conjunct) when c is zero.
- Conjunct one is an equivalence in each of its three shapes. For unsigned T
  the trapping pair is unwritable, so the ground true bound is exact. For a
  constant divisor `-1` the pair reduces to `dividend != min(T)`; for a
  constant dividend `min(T)` it reduces to `divisor != -1`; in every other
  constant case one disjunct is statically true.
- The class boundary is drawn by expressibility, not convenience. A signed
  site with two non-constant operands is excluded precisely because the
  fragment cannot state its safe condition, and it is left byte-identical to
  v0.31 rather than approximated in either direction.
- In an accepted program every class site is discharged (a claim route
  discharges the site via the claim's established fact, and the claim itself
  carries the runtime check and the `traps` effect), so dropping the site's
  own tests removes no reachable trap: the discharged relations prove both
  failure branches dead.
- Contradictory (dead) code discharges everything, including ground false
  conjuncts, exactly as [ENT-4] already fixes for the other two families.
- Facts-off acceptance is unchanged by construction: the judgment is a
  source-acceptance judgment under [ENT-1], identical in facts-on and
  facts-off compilation, like [OP-4] and the overflow family.
- [ENT-1] monotonicity is respected: the new family lands as the enumerated
  amendment class ("attaching a new protected family"), not as implementation
  strengthening. No fact source and no closure rule is added, so no other
  judgment moves.

## Acceptance-set analysis

Programs that change acceptance under this delta, exactly:

1. **REJECTED THAT WERE ACCEPTED**: a program containing a bare `/` or `%`
   in the divisor class whose division obligation the complete fact state
   does not discharge. In practice this is (a) every unsigned `n / d` whose
   divisor nothing proves nonzero, (b) every constant-zero divisor, and
   (c) every `n / -1_iK` whose dividend nothing bounds away from `iK::MIN`.
   The repairs are the stated mechanical fixes.
2. **REJECTED THAT WERE ACCEPTED (effect rows)**: a function whose written
   effect row lists `traps` where the only body trap contributor was a
   divisor-class bare site now has a written/exhibited [EFF-2] disagreement
   and rejects at its `effects` node until `traps` is removed from the row.
3. **NEWLY ACCEPTED**: exactly one class, and it is an [EFF-2] consequence
   rather than an entailment one. A program v0.31 rejected *only* because a
   divisor-class bare site forced `traps` into a body whose written row omits
   it is accepted once that site's obligation discharges, since a discharged
   class call exhibits no `traps`. `fn halve(n: own i32) -> own i32 pure {
   let q = n / 2_i32; return q; }` is the minimal witness: v0.31 rejects it
   under EFF-2, v0.32 accepts it. Nothing else becomes accepted — no fact
   source and no closure rule is added, so every entailment judgment on a
   program v0.31 already accepted is unchanged.

   Note for the lead: the superseded v0.31 arith-dissolution delta recorded
   "NEWLY ACCEPTED: none" for the structurally identical constant-operand
   family, which appears to have been inaccurate for the same reason. Worth a
   line in the v0.32 packet so the owner is not told the accepted set only
   ever shrinks.
4. **UNCHANGED**: every `/checked` and `%checked` spelling; every `.wrap`,
   `.checked`, `.sat`, `ineg.*`, `iabs.*`, shift, comparison, and bare
   `+`/`-`/`*` judgment; every signed bare `/` or `%` with two non-constant
   operands, including its trap record, its `traps` contribution, and its
   emitted guard set; and every accepted class site's runtime *behavior*
   (its traps were statically proven unreachable — the observable value set
   is identical).

## Live-tree migration inventory

Exhaustive scan of every `*.wf` outside `archive/` at this revision, for any
`/` or `%` character outside comments and doc strings:

| Site | Spelling | Class | v0.31 verdict | v0.32 verdict |
| --- | --- | --- | --- | --- |
| `tests/conformance/cases/x-arith-idiv-trap-zero-divisor-traps.wf:4` | `let q = x / 0_i32;` | divisor class (constant divisor) | `trap` (runs, aborts) | **`reject` citing OP-2**, residual `0_i32 != 0` |
| `tests/conformance/cases/err3-neg-propagate-different-error-type.wf:4` | `let r = v /checked 2_i64;` | outside (checked row) | `reject` (ERR-3) | unchanged |
| `tests/conformance/cases/op2-pos-div-checked.wf:3` | `match x /checked y {` | outside (checked row) | `accept` | unchanged |

That is the complete inventory: three division sites in the live tree, one of
which is a bare operator. `tests/programs/`, `tests/codegen/cases/`, and
every `research/experiments/**` program contain no `/` or `%` operator at
all, so the corpus, the codegen cases, and the experiment programs need no
migration.

The one affected case is protected conformance evidence. It is **not** edited
by this task; its migration belongs to the activation packet. The natural
migration is to keep the case id and rewrite the verdict to
`{"kind": "reject", "rule": "OP-2"}` with a doc line stating that the bare
zero-divisor spelling is now a compile-time rejection, and to add the
runtime-trap coverage back through the retained class (a signed two-variable
site) as `x-arith-idiv-trap-signed-two-variable-traps` below.

## PROPOSED conformance cases (names + verdicts)

Nothing under `tests/conformance/` is created or edited by this task. The
list below is the proposal for the activation packet, in the manifest's
verdict vocabulary.

Modified case (protected — exact before/after audit required):

| id | rules | before | after |
| --- | --- | --- | --- |
| `x-arith-idiv-trap-zero-divisor-traps` | `["OP-2", "SCOPE-4"]` | `{"kind": "trap"}` | `{"kind": "reject", "rule": "OP-2"}` |

New cases:

| id | rules | expect |
| --- | --- | --- |
| `op2-pos-division-obligation-discharged` | `["OP-2", "ENT-6", "EFF-2"]` | `{"kind": "accept"}` — a dominating `check igt(d, 0_u64)` discharges an unsigned `n / d`; no runtime check in any build mode and no `traps` exhibited, which is why the row is pure |
| `op2-neg-division-obligation-undischarged` | `["OP-2", "ENT-6"]` | `{"kind": "reject", "rule": "OP-2"}` — an unsigned `n / d` with nothing proving `d != 0` rejects at its `infix` node with residual `d != 0` |
| `op2-pos-division-claim-backstop` | `["OP-2", "ENT-6", "CLM-1"]` | `{"kind": "run", "exit": 0}` — a dominating `claim nonzero: ine(d, 0_u64)` discharges the site; the claim carries the retained runtime check and the `traps` row |
| `op2-neg-division-constant-zero-divisor` | `["OP-2", "ENT-6"]` | `{"kind": "reject", "rule": "OP-2"}` — a constant zero divisor is a ground false conjunct, rejected at every non-contradictory point; there is no accepted always-trapping bare spelling |
| `op2-pos-division-constant-divisor-total` | `["OP-2", "ENT-6", "EFF-2"]` | `{"kind": "run", "exit": 0}` — a nonzero constant divisor discharges both conjuncts with no fact source, so a signed `n / 2_i32` is pure and emits one plain instruction |
| `op2-neg-division-minus-one-divisor-unbounded` | `["OP-2", "ENT-6"]` | `{"kind": "reject", "rule": "OP-2"}` — the one expressible signed-overflow shape: `n / -1_i32` demands `n != -2147483648` and rejects with that residual |
| `op2-pos-division-minus-one-divisor-bounded` | `["OP-2", "ENT-6"]` | `{"kind": "accept"}` — the same site with a dominating branch bounding `n` above `iK::MIN` discharges both conjuncts and keeps no runtime test |
| `op2-pos-division-remainder-same-obligation` | `["OP-2", "ENT-6"]` | `{"kind": "accept"}` — `%` carries the identical obligation and discharges from the identical fact |
| `x-arith-idiv-trap-signed-two-variable-traps` | `["OP-2", "SCOPE-4", "ENT-6"]` | `{"kind": "trap"}` — the retained class: a signed `p / q` with two non-constant operands attaches no obligation, keeps its runtime test, and aborts on a zero divisor (replaces the runtime-trap coverage the modified case above gives up) |
| `op2-pos-division-checked-untouched` | `["OP-2", "OP-1"]` | `{"kind": "run", "exit": 0}` — the dissolution attaches obligations to the bare divisor class only; `/checked` is untouched, needs no discharge, and returns `Err(DivideByZero())` for a zero divisor |
| `eff2-neg-division-class-site-pure-row` | `["EFF-2", "OP-2"]` | `{"kind": "reject", "rule": "EFF-2"}` — a body whose only trap contributor is a retained signed two-variable site still exhibits `traps`, so a `pure` row rejects |
| `eff2-pos-discharged-division-site-pure-row` | `["EFF-2", "OP-2", "ENT-6"]` | `{"kind": "run", "exit": 0}` — the newly accepted class: a discharged divisor-class site exhibits no `traps`, so the narrower `pure` row is the correct one |

## Non-goals

- Any provenance gating of the division family (base discharge only, exactly
  as the overflow family landed).
- Any new [ENT-3] fact source, [ENT-4] closure rule, disjunction, case split,
  or loop induction. In particular, no rule is added to make the retained
  signed two-variable condition provable; if a corpus program ever needs it,
  the monotone successor is a fact source, not a weakening of this family.
- `ineg.trap`, `iabs.trap`, and the shift `.trap` amount obligations (their
  goals are expressible, but they are outside this delta's measured class; a
  later amendment may dissolve them the same way).
- Any change to `/checked`, `%checked`, `DivError`, `DivideByZero`, or
  `DivOverflow`.

## Implementation and integration switch

The compiler implements the complete judgment behind one integration switch,
default off, so the tree stays green under active v0.31:

- Switch: `DIVISION_OBLIGATIONS` in `compiler/src/semantic/check.rs`, `false`
  under v0.31; the v0.32 activation change flips it to `true` and deletes
  nothing else.
- With the switch off, every bare `/` and `%` keeps its `TrapSite`, its
  `traps` effect contribution, its complete guard set, and no obligation is
  attached — byte-identical v0.31 behavior. Three default-switch control
  tests pin this, including the one that pins the newly accepted `pure`-row
  class in both directions.
- With the switch on: the checker classifies class sites once through the
  shared `division_obligation_class` predicate (also read by the flow, so the
  two views cannot drift), drops their `TrapSite` and `traps` contribution;
  the entailment flow attaches and judges the two-conjunct obligation in the
  complete, unasserted, and S4-blinded views; undischarged sites reject citing
  OP-2 with `UndischargedDivisionObligation`; strict components re-judge in U
  citing OP-2 with `StrictUndischargedDivision`; and the backend emits the
  plain `udiv`/`sdiv`/`urem`/`srem` for a trap-free class site.
- Fifteen tests cover the family: thirteen in
  `compiler/src/semantic/tests/division_obligations.rs` and two backend
  emission tests in `compiler/src/backend/tests/division_obligations.rs`.
- The one shared structure this delta widened is `ObligationOutcome`'s
  `BoundsRequest`, which gains a `distinct` cell because the division family's
  normalized conjunct is a disequality rather than a difference bound. Both
  existing families set it `false`.
- The claim-accountability projection records a division-supporting claim use
  with `ClaimUseProvenance::NotApplicable`, exactly as it already does for the
  overflow family, because neither family forms a protected leaf.
