# Arithmetic-mode dissolution — exact v0.31 candidate rule edits

> Superseded 2026-08-18: applied at v0.31 activation (eb8e8634). Historical design evidence; the active spec owns the normative text.

Status: DELTA TEXT FOR LEAD INTEGRATION (batch 0070, task #13). This file
is delta input to the single v0.31 candidate; the lead integrates it and it
is superseded by the activated specification. Nothing here changes
`spec/kernel-spec.md`; every byte lands only through the owner's exact-byte
approval.

## Basis

Recorded measurements, [mcts_mem/whitefoot/checks-and-proofs/obligation-discharge.md](https://github.com/mbbill/Whitefoot/blob/0f22b026b062c36ef2f8ce9c725ee4307e1fda64/mcts_mem/whitefoot/checks-and-proofs/obligation-discharge.md) Facts, 2026-08-08 entries:

- An overflow goal is expressible in the existing L0 fragment whenever one
  operand is a literal: it folds to a difference bound on the other operand
  against a checker-computed constant — the shape the fragment's constant
  normalization already fixes. Two thirds of live trapping sites are that
  shape, and the loop-counter case discharges by the same transitive
  closure the index obligation uses.
- Two-variable accumulator sites are unprovable by any closure rule:
  nothing in the program bounds their operands.
- Operand signedness is not the discriminator (45 signed against 44
  unsigned live trapping sites; neither group's goal is expressible
  without a constant operand).
- Real programs already chose wrapping 228:30, so the dissolution's value
  is the trap-surface reduction and the thesis — proof reaches exactly the
  expressible class — not an operational win.

Design consequence: extend the obligation-discharge model from subscripts
to the constant-operand arithmetic class. A bare `+`, `-`, or `*` with at
least one constant operand carries a compile-time overflow obligation that
must discharge (or be established by a dominating claim or branch); a
discharged site loses its runtime check in every build mode. The bare
forms with two non-constant operands — the inexpressible class — keep
their trapping semantics unchanged. `.wrap`, `.checked`, `.sat`, division,
remainder, `ineg.trap`, `iabs.trap`, and the shift `.trap` rows are
untouched.

## The class

A bare-operator `+`, `-`, or `*` call is in the **constant-operand class**
exactly when at least one of its two operand atoms reads as an [ENT-2]
constant — an integer literal or an integer-typed named const — judged per
concrete [FN-2] instance, so a substituted generic literal or const is a
constant in each instance. Membership does not require the other operand
to be a term: a non-term other operand (a subscripted place) leaves the
obligation underivable with the same one-`let` rebinding fallback [ENT-6]
already states for a subscripted offset atom. This keeps the trap surface
independent of operand spelling: `let t = a[i]; let x = t + 1_u8;` and
`let x = a[i] + 1_u8;` reach the same judgment rather than one proving and
one silently trapping.

Attaching a new obligation family is an amendment-level accepted-set
change explicitly enumerated by [ENT-1]; this delta is exactly such an
amendment.

No grammar production, token, or spelling changes; the native grammar
verifier is not implicated. All edits are semantic-rule prose.

## Rule edits

Each edit gives the current sentence(s) and the exact replacement, one
sentence per line, v0.30 profile style.

### 1. [OP-1] operation table — the bare add/sub/mul row's effects cell

Current row:

```
| `+` `-` `*` | all int T | `(T, T) -> own T` | traps |
```

Replacement row:

```
| `+` `-` `*` | all int T | `(T, T) -> own T` | traps (outside OP-2's constant-operand class) |
```

### 2. [OP-2] — the bare-operator paragraph

Current (six sentences, from "For `a + b`, …" through "…runtime overflow
test."):

```
For `a + b`, `a - b`, and `a * b` over a common selected type T, let z be the same mathematical result.
If z belongs to T's value set, the operation returns that exact value.
Otherwise it traps for integer overflow before producing a result.
Integer overflow in one of these bare-operator trapping operations is a contract violation [ERR-4, SCOPE-4], not a recoverable `Overflow` value, source rejection, wrapped result, saturation, truncation, or undefined behavior.
A call whose constant operands make overflow inevitable remains a well-typed accepted call and traps when executed; constant folding may replace it only with the same attributed trap.
Each such call syntactically exhibits `traps` under [EFF-2], even when a proof eliminates its runtime overflow test.
```

Replacement:

```
For `a + b`, `a - b`, and `a * b` over a common selected type T, let z be the same mathematical result.
A bare-operator call at least one of whose two operand atoms reads as an [ENT-2] constant — an integer literal or an integer-typed named const, judged per concrete [FN-2] instance — is in the constant-operand class.
A constant-operand-class call carries the overflow obligation that z belongs to T's value set [ENT-6], judged by the same complete-state base discharge as a subscript bounds obligation.
A discharged class call returns the exact value z with no runtime overflow check in any build mode, never traps, exhibits no `traps` under [EFF-2], and its checked-program disposition records the discharging derivation [DIAG-2].
A class call whose obligation the complete fact state does not discharge is a compile-time rejection citing OP-2 at that call's `infix` node, carrying the residual obligation rendered exactly per [ENT-6]; it publishes no checked program.
Its mechanical fix is a dominating `claim` of the residual [CLM-1], a dominating branch establishing it [ENT-3], or the explicit `wrap`, `checked`, or `sat` respelling.
A class call whose two constant operands make overflow inevitable instantiates a ground false conjunct [ENT-6] and is therefore rejected at every non-contradictory point; there is no accepted always-trapping bare spelling.
For a class call in a [CLM-3] demanded strict component, the same normalized obligation must additionally discharge in that function's already-computed unasserted U state [ENT-6]; a refuted or unproved strict judgment is a hard rejection citing OP-2 at the same `infix` node, carrying the same exact residual plus the strict root, concrete function instance, and `unasserted` view, and its mechanical repair is [OP-4]'s strict repair.
A bare-operator call both of whose operand atoms are non-constant retains the trapping judgment: if z belongs to T's value set the operation returns that exact value, and otherwise it traps for integer overflow before producing a result.
Integer overflow in one of these retained bare-operator trapping operations is a contract violation [ERR-4, SCOPE-4], not a recoverable `Overflow` value, source rejection, wrapped result, saturation, truncation, or undefined behavior.
Each retained trapping call syntactically exhibits `traps` under [EFF-2], even when a proof eliminates its runtime overflow test.
```

### 3. [ENT-6] — the obligation-family inventory

Current second sentence:

```
This version attaches exactly one obligation family: for every source subscript `P[i]` — read, write, and [SET-1] target position alike — the bounds obligation `i < len(P)`, normalized `i - len(P) <= -1`, at that subscript's `psuffix` node, one obligation per subscript in a chain, where `i` is the offset atom whose exact type [OP-4] fixes as `own u64`, so both sides are u64-typed and the relation is over their mathematical values.
```

Replacement (the subscript sentence is retained byte-identically after the
new opening sentence; the overflow family follows it):

```
This version attaches exactly two obligation families.
The first family: for every source subscript `P[i]` — read, write, and [SET-1] target position alike — the bounds obligation `i < len(P)`, normalized `i - len(P) <= -1`, at that subscript's `psuffix` node, one obligation per subscript in a chain, where `i` is the offset atom whose exact type [OP-4] fixes as `own u64`, so both sides are u64-typed and the relation is over their mathematical values.
The second family: for every bare-operator `+`, `-`, or `*` call in [OP-2]'s constant-operand class, the overflow obligation that the call's exact mathematical result belongs to the selected type T's value set, at that call's `infix` node.
The overflow obligation normalizes to exactly two conjuncts — ordinal zero the upper bound and ordinal one the lower bound — each one difference bound between the non-constant operand read as a term and Z with one checker-computed constant, folded exactly as follows over mathematical integers, with floor and ceiling the exact-quotient roundings toward negative and positive infinity.
For `t + c` and `c + t` with constant c: `t - Z <= max(T) - c` and `Z - t <= c - min(T)`.
For `t - c`: `t - Z <= max(T) + c` and `Z - t <= -min(T) - c`.
For `c - t`: `t - Z <= c - min(T)` and `Z - t <= max(T) - c`.
For `t * c` with c > 0: `t - Z <= floor(max(T)/c)` and `Z - t <= -ceil(min(T)/c)`.
For `t * c` with c = 0: both conjuncts are `Z - Z <= 0`.
For `t * c` with c < 0: `t - Z <= floor(min(T)/c)` and `Z - t <= -ceil(max(T)/c)`.
For two constant operands with exact mathematical result z: both conjuncts are `Z - Z <= 0` when z belongs to T's value set and `Z - Z <= -1` otherwise.
The complete-state base judgment discharges a conjunct exactly when the closed complete fact state at that node derives it [ENT-4, ENT-5], and discharges the obligation exactly when both conjuncts discharge.
Failure of that base judgment is the [OP-2] rejection; its diagnostic renders the residual of the least undischarged conjunct as exactly: the non-constant operand's canonical source bytes, then ` <= `, then the conjunct constant in decimal, for ordinal zero; the negated conjunct constant in decimal, then ` <= `, then the operand's canonical source bytes, for ordinal one; and, for a ground conjunct, the exact decimal mathematical result, then ` outside `, then the selected type's spelling.
The overflow family attaches base discharge only: it creates no [PRV-2] or [PRV-3] protected demand, no provenance event, and no runtime operation in this version.
An operand that is not a term or constant leaves each non-ground conjunct underivable, and the one-rebinding fallback stated below for a subscripted offset atom applies identically to a subscripted class operand.
```

Current metadata sentence:

```
The sole current bounds relation has one conjunct at ordinal zero.
```

Replacement:

```
The bounds relation has one conjunct at ordinal zero; the overflow relation has its upper conjunct at ordinal zero and its lower conjunct at ordinal one.
```

The occurrence-identity form `(concrete function instance, exact
obligation-occurrence NodePath, normalized conjunct ordinal)` already
covers the new family unchanged; an overflow occurrence's NodePath is its
`infix` node.

### 4. [EFF-2] — the body-syntactic traps clause

Current fragment (inside the first sentence of the body-syntactic
paragraph):

```
it exhibits `traps` iff the body contains any trapping-mode operation — a bare infix arithmetic operator (`+`, `-`, `*`, `/`, `%`) or a `.trap` OPNAME — `check`, `claim`, or a call to any operation or function whose effect row includes `traps` (even if later proven away);
```

Replacement fragment:

```
it exhibits `traps` iff the body contains any trapping-mode operation — a bare `/` or `%`, a bare `+`, `-`, or `*` outside [OP-2]'s constant-operand class, or a `.trap` OPNAME — `check`, `claim`, or a call to any operation or function whose effect row includes `traps` (even if later proven away);
```

### 5. [ENT-3] S7 — the bare constant-offset justification

Current sentence:

```
For `p + k` and `p - k` with constant k, s = p ± k is established on the normal continuation unconditionally: the executed contract check is the proof [OP-2].
```

Replacement:

```
For `p + k` and `p - k` with constant k, s = p ± k is established on the normal continuation unconditionally: the site is a constant-operand-class call whose discharged overflow obligation is the proof [OP-2, ENT-6].
```

(No S7 row is added or removed; `k - p` and `p * k` remain without an
equality source.)

### 6. [CLM-3] — the strict U clause's citation

Current fragment:

```
every protected obligation owned by the component discharges in its owning function's existing unasserted U state [OP-4, ENT-6],
```

Replacement fragment:

```
every protected obligation owned by the component discharges in its owning function's existing unasserted U state [OP-4, OP-2, ENT-6],
```

### 7. [DIAG-3] — the executed-overflow record

Current sentence:

```
For an executed bare `+`, `-`, or `*` overflow, `rule_id` is `OP-2`, `message` is `integer overflow`, and `node_path` is the trapping `infix` node; a bare `/` or `%` contract violation is a table-operation contract check at its `infix` node.
```

Replacement:

```
For an executed bare `+`, `-`, or `*` overflow, `rule_id` is `OP-2`, `message` is `integer overflow`, and `node_path` is the trapping `infix` node; such a record arises only outside [OP-2]'s constant-operand class, because a class call discharges at compile time and executes no overflow test; a bare `/` or `%` contract violation is a table-operation contract check at its `infix` node.
```

### 8. [ENT-1] — the version-addition inventory

Current sentence:

```
Beyond those classes, this specification adds only FN-9/S12, the two stated unsigned S7 relations, and [ENT-5]'s value-if-only delivery, and retains the provenance gate.
```

Replacement:

```
Beyond those classes, this specification adds only FN-9/S12, the two stated unsigned S7 relations, [ENT-6]'s constant-operand overflow obligation family, and [ENT-5]'s value-if-only delivery, and retains the provenance gate.
```

## Soundness notes

- Both folded conjuncts are equivalences, not approximations: for a
  monotone `t op c` over mathematical integers, z in [min(T), max(T)]
  holds iff both conjuncts hold, including the sign-reversal cases for
  negative multiplication constants and the exact floor/ceil quotients.
- The trivial conjunct discharges automatically: [ENT-2]'s implicit type
  bounds (`t - Z <= max(T)`, `Z - t <= -min(T)`) subsume whichever side
  the constant relaxes, so `x + 1_u64` leaves exactly the binding
  `x <= max - 1` residual.
- In an accepted program every class site is discharged (a claim route
  discharges the site via the claim's established fact, and the claim
  itself carries the runtime check and the `traps` effect), so dropping
  the site's own check removes no reachable trap: the discharged relation
  proves the overflow branch dead.
- Contradictory (dead) code discharges everything, including ground false
  conjuncts, exactly as [ENT-4] already fixes for bounds obligations.
- Facts-off acceptance is unchanged by construction: the judgment is a
  source-acceptance judgment under [ENT-1], identical in facts-on and
  facts-off compilation, like [OP-4].
- [ENT-1] monotonicity is respected: the new family lands as the
  enumerated amendment class ("attaching a new protected family"), not as
  implementation strengthening.

## Acceptance-set analysis

Programs that change acceptance under this delta, exactly:

1. REJECTED THAT WERE ACCEPTED: a program containing a bare `+`, `-`, or
   `*` with a constant operand whose overflow obligation the complete
   fact state does not discharge. This includes every constant-constant
   site whose overflow is inevitable outside dead code. The repairs are
   the stated mechanical fixes.
2. REJECTED THAT WERE ACCEPTED (effect rows): a function whose written
   effect row lists `traps` where the only body trap contributor was a
   constant-operand-class bare site now has a written/exhibited [EFF-2]
   disagreement and rejects at its `effects` node until `traps` is
   removed from the row.
3. NEWLY ACCEPTED: none. The delta accepts no program v0.30 rejects
   (attaching obligations only rejects more; no fact source or closure
   rule is added, so no other judgment changes).
4. UNCHANGED: every `.wrap`, `.checked`, `.sat`, `/`, `%`, `ineg.*`,
   `iabs.*`, shift, and comparison spelling; every bare site with two
   non-constant operands; every accepted class site's runtime *behavior*
   (its trap was statically proven unreachable — the observable value
   set is identical).

Live-tree inventory at this revision (counted with grep over `*.wf`,
excluding suffixed operators): tests/programs has 4 bare trapping sites —
2 in the class (`offset + 1_u64` in prefix_expression.wf, discharged by
its dominating bounds branch; `children + 1_u64` in recursive_tree.wf,
undischarged — needs one claim, branch, or `wrap` respelling at v0.31
migration) and 2 two-variable sites that keep trap semantics
(`left_value + right_value`, `left_count + right_count`).
tests/conformance has 74 bare-trapping-site lines, roughly 34 with a
literal operand; the conformance family's migration is protected evidence
and belongs to the activation packet, not this task.

## Non-goals

- `ineg.trap` and `iabs.trap` (their goals are expressible without a
  constant operand, but they are outside the measured literal-operand
  class; a later amendment may dissolve them the same way).
- Shift `.trap` amount obligations and the `/`/`%` zero-divisor
  obligation (expressible; same later-amendment path).
- Any S7 extension (`k - p`, `p * k` equalities), loop induction, or new
  closure rule.
- Any provenance gating of the overflow family.

## Implementation and integration switch

The compiler implements the complete judgment behind one integration
switch, default off, so the tree stays green under active v0.30:

- Switch: `ARITHMETIC_OVERFLOW_OBLIGATIONS` in
  `compiler/src/semantic/check.rs`, `false` under v0.30; the v0.31
  activation change flips it to `true` and deletes nothing else.
- With the switch off, class sites keep their `TrapSite`, their `traps`
  effect contribution, their overflow branch, and no obligation is
  attached — byte-identical v0.30 behavior.
- With the switch on: the checker classifies class sites once (shared
  predicate), drops their `TrapSite` and `traps` contribution, the
  entailment flow attaches and judges the two-conjunct obligation in the
  complete, unasserted, and S4-blinded views, undischarged sites reject
  citing OP-2, strict components re-judge in U citing OP-2, and the
  backend emits the plain exact operation for a trap-free class site.
