# Proposal D — the contract is one expression

Status: research proposal. Nothing here is approved, and this document changes
no specification byte, test, or compiler file. It is one of four independent
proposals in the contract-surface design study; it argues one stance and
attacks it.

Baseline: `spec/kernel-spec.md` v0.32 at `d32d7dd0`.

---

## 1. Core

A contract clause today is a *block of statements that is not a function body*
— `requires { let a = …; let b = …; check e else trap "…"; }` — and that
category is the defect. Because the clause is a statement context, it had to
borrow the statement vocabulary (`let_stmt`, `check_stmt`) whole and then
neuter the half it could not use, which is where the inert `else trap "msg"`
comes from; because it is not a body, [FN-8] must police the borrowed
vocabulary back down with a structural pass ("zero or more ordinary lets
followed by exactly one final `check_stmt`"), the locals need their own
copy-only judgment, and both [FN-8] and [FN-9] need an alpha-expansion
algorithm to erase the bindings again before anything semantic happens.
Proposal D deletes the category: a contract clause becomes **one proposition
expression** terminated by a semicolon — `requires ile(filled,
len(deref(destination)));` and `ensures result: ile(result,
len(deref(destination)));` — written in a nested applicative `prop` grammar
that is reachable only from those two clause positions. There is no statement,
so there is no `check` and no `trap` to leave inert; there is no binding, so
there is no copy judgment and no expansion step, and the [FN-8] GoalTemplate
and the [FN-9] RelationTemplate become *the written tree itself* after [FN-2]
substitution rather than the output of an erasure pass. The corpus says this
is nearly free: across the 64 contract clauses in the live tree the mean
clause-local count is 0.9, every `ensures` clause in the tree has at most one
local and that local exists only to route around [GRAM-9], and the longest
predicate anywhere in the live tree is 62 characters on one line.

---

## 2. The measurement

This is the decisive evidence for the stance, so it is reported before the
design.

### 2.1 Method

Every `.wf` file in the tree (637 files) was scanned for `requires` and
`ensures` clauses; each clause body was split into top-level statements; each
clause's locals were alpha-expanded into its final check condition by the same
substitute-until-no-local rule [FN-8] states, and the result measured for
length, operation-node count, and call-nesting depth. Scripts are throwaway and
live outside the repository.

Population: **108 clause occurrences**, of which 104 have the well-formed
`let* check` shape (the other 4 are negative conformance cases whose subject is
a malformed clause). 89 `requires`, 19 `ensures`.

The population splits sharply and the split matters:

| set | clauses | note |
|---|---|---|
| **live** — `tests/programs`, `tests/conformance/cases` | 64 | current v0.32 grammar |
| **dormant** — `tests/codegen/cases`, `research/experiments` | 40 | pre-v0.32 spellings |

The dormant set uses `imul.wrap(…)`, `iadd.wrap(…)` as *callees*. Those names
are not in the v0.32 operation table (`spec/kernel-spec.md` §7) and not in
`compiler/src/resolution/catalog.rs`; v0.32 respelled integer arithmetic as
operators only. So these files do not parse under the current grammar and no
live gate drives them (`tests/codegen/cases` has no runner in `compiler/` or
`tools/`; the old runner is under `archive/tests/codegen/`). 37 of the 40 are
mutant copies of one contract.

### 2.2 Clause-locals per contract

Live set, 64 clauses:

| locals | 0 | 1 | 2 | 3 | 4 | ≥5 |
|---|---|---|---|---|---|---|
| all clauses | 24 | 21 | 15 | 3 | 1 | **0** |
| `requires` (45) | 7 | 19 | 15 | 3 | 1 | 0 |
| `ensures` (19) | **17** | **2** | 0 | 0 | 0 | 0 |

Mean 0.9 locals per clause. Maximum 4, in exactly one clause. **No live
contract has five locals.**

Whole tree including dormant files, 104 clauses: `0→24, 1→22, 2→15, 3→3,
4→38, 16→1, 27→1`. The 38-clause spike at four locals is 37 copies of the
base64 capacity contract plus its original.

### 2.3 Predicate length and shape

Live set, after alpha expansion, rendered as one line in the proposed form:

| | min | median | max |
|---|---|---|---|
| characters | 1 | **18** | **62** |
| operation nodes | 0 | 1 | 5 |
| call-nesting depth | 0 | 1 | **4** |

Call-nesting depth distribution: `1→43, 2→11, 3→8, 4→1`.

**Zero live contracts require an infix operand that is itself infix.** Exactly
one live contract uses an infix operator at all.

### 2.4 The `ensures` result

17 of 19 `ensures` clauses in the whole tree already have zero locals. The two
that have one are identical:

```wf
} ensures result {
  let capacity = len(deref(destination));
  check ile(result, capacity) else trap "append result exceeds destination";
}
```

([`tests/programs/wfgrep.wf:125`](../../../tests/programs/wfgrep.wf) and
`tests/programs/raw_deflate_boundary.wf:21`.)

That local carries no meaning. [FN-9] already admits `len(P)` directly as a
relation operand — "Both operands must be the symbolic result datum, a
parameter datum with only field and `deref` projections, a named const, a
typed integer literal, or `len(P)` for an admitted formal place P". The local
exists *solely* because [GRAM-9] forbids a `call` in an atom position, so
`ile(result, len(deref(destination)))` cannot be spelled. Under Proposal D it
can. **Every `ensures` clause in the tree becomes one line with no bindings.**

### 2.5 The largest live contract, verbatim

`tests/conformance/cases/x-base64-rfc-vectors-run.wf:3` — 4 locals, the
maximum anywhere in the live tree:

```wf
fn encode['r](out: &uniq 'r buffer<u8>, input: own buffer<u8>) -> own u64 reads('r), writes('r), traps requires {
  let required_out_length = len(deref(out));
  let required_input_length = len(input);
  let required_out_groups = ishr.wrap(required_out_length, 2_u32);
  let required_covered_input = required_out_groups *wrap 3_u64;
  check ile(required_input_length, required_covered_input) else trap "base64 output capacity";
} {
```

In Proposal D:

```wf
fn encode['r](out: &uniq 'r buffer<u8>, input: own buffer<u8>) -> own u64 reads('r), writes('r), traps
requires ile(len(input), ishr.wrap(len(deref(out)), 2_u32) *wrap 3_u64);
{
```

62 characters, depth 4, one line. Six source lines become one. This is the
"base64 capacity contract" the brief asked about, and it needs four locals
today, not five, and it survives the collapse comfortably.

### 2.6 The worst case anywhere, verbatim

`research/experiments/zlib-core-kernels/match_copy.wf:1` — 27 locals, the
largest contract that has ever been written in this project. Verbatim source:

```wf
fn inflate_match_copy ['o] (out: &uniq 'o buffer<u8>, seed_len: own u64, distance: own u64, match_len: own u64, repeats: own u64) -> own u64 reads('o), writes('o), traps requires {
  let distance_positive: own Bool = igt<u64>(distance, 0_u64);
  let distance_in_history: own Bool = ile<u64>(distance, seed_len);
  let distance_valid: own Bool = ile<u64>(distance, 32768_u64);
  let length_minimum: own Bool = ige<u64>(match_len, 3_u64);
  let length_maximum: own Bool = ile<u64>(match_len, 258_u64);
  let distance_shape: own Bool = band<Bool>(distance_positive, distance_valid);
  let distance_history: own Bool = band<Bool>(distance_shape, distance_in_history);
  let match_valid: own Bool = band<Bool>(length_minimum, length_maximum);
  let repeats_low: own u64 = iand<u64>(repeats, 4294967295_u64);
  let repeats_high: own u64 = ishr.wrap<u64>(repeats, 32_u32);
  let product_low: own u64 = imul.wrap<u64>(match_len, repeats_low);
  let product_high: own u64 = imul.wrap<u64>(match_len, repeats_high);
  let product_high_low: own u64 = iand<u64>(product_high, 4294967295_u64);
  let product_high_high: own u64 = ishr.wrap<u64>(product_high, 32_u32);
  let product_shifted: own u64 = ishl.wrap<u64>(product_high_low, 32_u32);
  let product: own u64 = iadd.wrap<u64>(product_low, product_shifted);
  let high_zero: own Bool = ieq<u64>(product_high_high, 0_u64);
  let product_no_carry: own Bool = ige<u64>(product, product_low);
  let product_fits: own Bool = band<Bool>(high_zero, product_no_carry);
  let total: own u64 = iadd.wrap<u64>(seed_len, product);
  let total_fits: own Bool = ige<u64>(total, seed_len);
  let output_len: own u64 = len<u8>(deref(out));
  let output_fits: own Bool = ile<u64>(total, output_len);
  let arithmetic_valid: own Bool = band<Bool>(product_fits, total_fits);
  let capacity_valid: own Bool = band<Bool>(arithmetic_valid, output_fits);
  let shape_valid: own Bool = band<Bool>(distance_history, match_valid);
  let all_valid: own Bool = band<Bool>(shape_valid, capacity_valid);
  check all_valid else trap "invalid match-copy arguments";
} {
```

Alpha-expanded, this is **1087 characters, 47 operation nodes, call-nesting
depth 10**. Rendered as one `prop` it is unreadable. That is stated without
softening and taken up again in §12. Three facts qualify it, none of which
dissolves it:

1. The file does not parse under v0.32 (`imul.wrap`, `iadd.wrap` are not
   operation names) and no gate drives it.
2. Twelve of the 27 locals (`repeats_low` … `product_fits`) are a hand-rolled
   64×64→128 multiply-overflow test. `imulhi` is in the v0.32 operation table
   — `| imulhi | all int T | (T, T) -> own T | pure |`, "the high half of the
   full double-width product" [OP-8] — so the entire twelve-local block is
   exactly `ieq(imulhi(match_len, repeats), 0_u64)`, one node. The full
   contract re-expressed with `imulhi` is 8 operation nodes, not 47.
3. It is nonetheless a real contract a real writer wrote for real DEFLATE
   work, and it is the honest worst case for this proposal.

Second largest: `huffman_literals.wf:68`, 16 locals, 446 characters, depth 7,
same dormant directory, same pre-v0.32 spellings.

### 2.7 Program entries

Measured independently (the number in my brief was wrong twice and is
corrected here):

- **532** `fn_decl`s named `main` across the tree: 451 unlabelled `fn main`,
  74 `command fn main`, 2 `deny_claims fn main`, 2 `deny_claims command fn
  main`, 3 with a rejected kind word (`service`/`embedded`/`daemon`, negative
  [FN-7] cases).
- **Three** of them carry a `requires`, not one:
  - `tests/conformance/cases/fn8-trap-requires-false.wf:1` —
    `fn main() -> own unit pure requires {` (verdict `{"kind":"trap"}`)
  - `tests/conformance/cases/clm3-neg-generated-wrapper-check.wf:1` —
    `deny_claims command fn main() -> own ExitStatus pure requires {`
    (verdict `{"kind":"reject","rule":"FN-8"}`)
  - `tests/conformance/cases/clm3-pos-transitive-value-branch.wf:36` —
    `deny_claims command fn main() -> own ExitStatus pure requires {`
    (verdict `{"kind":"run","exit":0}`)

Migration cost for the entry restriction is therefore **three protected
conformance cases**, not zero. §11 prices them.

### 2.8 What the measurement already licenses

`spec/kernel-spec.md` line 8, the R3-PROVISIONAL REGISTER, names this exact
item:

> the `requires { requires_entry* }` surface spelling with its FN-8-checked
> ordinary-let/final-check subset (FN-8 — semantics selected, spelling not yet
> compared)

and [mcts_mem/whitefoot/checks-and-proofs/requires-entry-contract.md](https://github.com/mbbill/Whitefoot/blob/0f22b026b062c36ef2f8ce9c725ee4307e1fda64/mcts_mem/whitefoot/checks-and-proofs/requires-entry-contract.md) records:

> 2026-07-11 statement: the semantics (existence, callee-entry execution,
> always-retained check, concrete-only scope) are evidence-selected; the
> `requires { let* check }` block spelling is minimality-selected and
> R3-provisional pending a writer-tier comparison against a credible
> single-predicate alternative. (sourced)

Proposal D is that credible single-predicate alternative, and §2 is the
writer-tier comparison the register asked for.

---

## 3. The form

### 3.1 What a contract looks like

```wf
fn append_slice['d, 'm](destination: &uniq 'd buffer<u8>, filled: own u64, text: own slice<'m, u8>) -> own u64 reads('d 'm), writes('d)
requires ile(filled, len(deref(destination)));
ensures result: ile(result, len(deref(destination)));
{
  doc "Appends as much of one static message as the destination still holds.";
  …
}
```

Compare the current spelling of the same declaration
(`tests/programs/wfgrep.wf:121`), which spends seven lines and two inert
`else trap` phrases on the same two propositions.

### 3.2 Exact grammar

[GRAM-2], changed lines only:

```wf-ebnf GRAM-2
fn_decl        := "deny_claims"? program_kind? "fn" IDENT generics? region_params? "(" param_list? ")"
                  "->" rtype effects requires_clause? ensures_clause? "{" doc? stmt* "}"
requires_clause:= "requires" prop ";"
ensures_clause := "ensures" ensures_selector ":" prop ";"
ensures_selector:= IDENT | TYPEID "(" fieldbind_list? ")"
```

Deleted from [GRAM-2]: `requires_block`, `requires_entry`, `ensures_block`,
`ensures_entry`. `ensures_selector` and `fieldbind_list` are unchanged, so
[GRAM-10]'s selector paragraph and [FN-9]'s selector admission survive
verbatim.

Deleted from [GRAM-4]: `check_stmt`. v0.32's check dissolution already removed
`check_stmt` from the `stmt` alternation; its only two remaining users are
`requires_entry` and `ensures_entry`, both of which this proposal deletes, so
the production becomes unreachable and goes with them. The fixed atoms `check`
and `trap` leave the grammar entirely (`else` survives in `if_stmt`; the
`traps` effect word and the `.trap` OPNAME suffix are different tokens and are
untouched).

New [GRAM-12], the proposition grammar:

```wf-ebnf GRAM-12
prop        := pprimary pinfix_tail?
pinfix_tail := infix_op pprimary
pprimary    := literal | place | pcall
pcall       := callee targs? "(" ( prop ("," prop)* )? ")"
```

`literal`, `place`, `callee`, `targs`, and `infix_op` are the existing
[GRAM-5] productions, reused unchanged. `prop` is reachable only from
`requires_clause` and `ensures_clause`; `expr`, `atom`, `stmt`, and
`construct` are unreachable from `prop`, and `prop` is unreachable from them.

Net production count: **six added** (`requires_clause`, `ensures_clause`,
`prop`, `pinfix_tail`, `pprimary`, `pcall`), **five deleted**
(`requires_block`, `requires_entry`, `ensures_block`, `ensures_entry`,
`check_stmt`). Net +1 production, −2 fixed atoms, −1 STRING position. §12
owns that accounting.

### 3.3 Determinism ([GRAM-1] strong-LL(2))

Every new decision is 2-token decidable, by the same argument the existing
grammar already uses:

- `fn_decl` after `effects`: `requires` selects `requires_clause`, `ensures`
  selects `ensures_clause`, `{` selects the body. Pairwise disjoint on one
  token.
- `ensures_clause` selector: IDENT selects the plain form, TYPEID selects the
  variant form. One token. Unchanged from today.
- `prop → pprimary pinfix_tail?`: the `pinfix_tail` `SELECT_2` set is the
  `infix_op` token set; the follow set of `prop` is `{ "," ")" ";" }`.
  Disjoint.
- `pprimary → literal | place | pcall`: on a literal or OPNAME token the arm is
  fixed by one token. On IDENT, the second token decides: `(` or `<` selects
  `pcall`, anything else selects `place`. This is *exactly* the existing
  [GRAM-5] discrimination between `atom → place` and `call`, so it introduces
  no new class of decision. `deref` is a fixed atom and therefore ineligible
  for IDENT [FORM-3], so `deref (` never competes with `pcall`.
- `pcall` argument list: `)` closes an empty list, anything else starts a
  `prop`. One token.

No decision uses predicate priority, and no new terminal is introduced.

### 3.4 No parenthesization surface

[GRAM-6]'s guarantee — "no precedence, associativity, or parenthesization
surface exists" — is preserved verbatim for `prop`. `pinfix_tail`'s operand is
a `pprimary`, never a `prop`, so an infix operand is never itself infix and no
precedence or associativity question can arise. Parentheses are therefore
never needed and are not admitted.

The cost is real and is stated: **an arithmetic tree of depth greater than one
in operator position cannot be spelled in a `prop`.** `seed_len +wrap (a
+wrap b)` has no spelling. Measured: **zero of the 64 live contracts need
it**, and exactly one live contract uses an infix operator at all. The one
contract in the tree that would need it (`match_copy.wf`, §2.6) does not parse
under v0.32 and collapses under `imulhi`.

The monotone extension is one production and one `SELECT_2` row —
`pprimary := literal | place | pcall | "(" prop ")"`, with parentheses
*mandatory* around an infix operand that is itself infix, so exactly one
spelling per tree and still no precedence table. Deferring it costs nothing
that a later version cannot recover, because it only enlarges the accepted set.

**Trigger for adopting it:** the first contract in `tests/programs` or
`tests/conformance/cases` whose proposition genuinely needs an infix operand
that is itself infix, and which no operation-table row collapses.

---

## 4. The ANF answer

This is the constraint that made statement blocks attractive and it is
answered head on, in three parts.

### 4.1 [GRAM-9] governs computation; a proposition is not a computation

[GRAM-9]'s scope is exact:

> Every call argument, construct field value, infix operand, subscript offset,
> and lower or upper endpoint of a `for_stmt` is an `atom` [GRAM-5]; a `call`
> or `construct` in an atom position does not derive under the grammar and is
> a hard error citing GRAM-9. A computed value is forwarded to another
> operation only by binding it with a preceding `let` (whose mode and type are
> derived [TYPE-5]) and referencing the binding.

Every listed position is a position in `expr`, `stmt`, or `for_stmt` — the
executable surface — and the stated mechanism is *forwarding a computed value*.
Forwarding a value is an execution notion: it names an evaluation order, an
intermediate storage, and a point at which [TYPE-5] derives a mode and type for
that storage. A `prop` has none of these. Under the entry restriction (§6) it
is never evaluated, never lowered, and never produces an intermediate value;
[DIAG-2] retains it as a GoalTemplate or RelationTemplate, not as an operation
sequence.

So a `prop` is not exempted from [GRAM-9] — **[GRAM-9] never reaches it**,
because the `prop` productions contain no `atom` position and derive no `call`
in one. The rule's text needs one clarifying clause naming its scope, given in
§5.4, and nothing else.

### 4.2 [FORM-1] is preserved because the two grammars are positionally disjoint

[FORM-1] requires "exactly one spelling per semantic construct". The concern is
that a nested `prop` and a flat `let`-chain would be two spellings of one
computation. They are not, because the reachability is disjoint (§3.2): a
body computation has exactly one spelling (flat, [GRAM-9]); a proposition has
exactly one spelling (nested, [GRAM-12]); and no byte sequence can be in both
positions. [GRAM-9]'s own sentence "Nesting and let-splitting are not two
spellings of one computation" survives verbatim and becomes *more* true, since
in `prop` position let-splitting does not exist at all.

The weaker existing situation is worth naming: today a writer *can* spell one
proposition many ways — with or without intermediate lets, with shared or
duplicated subexpressions — and [FN-8] then declares them all identical by
erasing the differences during expansion ("Clause-local spellings, clause-local
NodePaths, and whether identical subexpressions were shared through one let are
absent after expansion"). That is a [FORM-1] many-to-one that the current rule
tolerates by erasure. Proposal D makes it one-to-one at the source: **the
writer can read goal equality off the page**, because the goal *is* the tree.

### 4.3 The readability premise does not bite here

The reason to prefer ANF over nesting is that deep nesting is unreadable. §2.3
measures the actual depths: median 1, maximum 4, and the maximum is a
62-character line. The premise that justifies ANF for bodies is simply not
present for propositions at the sizes this project writes.

---

## 5. Exact rule text

Only changed or new text is given. Everything not shown is unchanged.

### 5.1 [OP-5] — becomes the condition judgment, with no trap vocabulary

`check_stmt` is deleted, so [OP-5]'s subject vanishes; but its judgment is
still cited by [GRAM-6] for `if` conditions and is now cited by [FN-8]/[FN-9]
for the proposition root. [OP-5] is therefore retained, reduced to the
judgment, and loses every dynamic sentence. Rule IDs are stable, so it keeps
the number.

> **[OP-5]** A Bool-position expression must have exact value mode and type
> `own Bool`, where `Bool` is the PRE-1 nominal type. The Bool positions are
> exactly: an `if_stmt` or `value_if` condition [GRAM-6]; and the complete
> `prop` of a `requires_clause` or an `ensures_clause` [FN-8, FN-9].
> No integer, other enum, borrowed `Bool`, or implicit truthiness conversion is
> admitted [TYPE-4].
> The implicit-read case already owned by [TYPE-7] is exclusive: when the
> expression uses a borrow-mode or box/arena binding where its referent `Bool`
> value would be required, that use is rejected citing TYPE-7 and OP-5 forms no
> candidate.
> Every other exact-mode or exact-type failure is a hard error citing OP-5 at
> the selected node, with `SourceCoordinate` equal to that node's complete
> checked half-open source extent.
> The fuller stated-and-checked vocabulary (loop invariants, ranges) is
> DEFERRED with its delta.

Deleted from [OP-5]: the opening `check e else trap "msg";` sentence, the
`requires`-final sentence ("uses this exact condition judgment, decoded
message, and dynamic-boundary failure behavior…"), and the `ensures`-final
sentence. Three sentences out, one scope sentence in.

### 5.2 [FN-8] — requires

> **[FN-8]** Any source `fn_decl` other than the unit entry [FN-7], generic or
> nongeneric, may carry one `requires_clause` after its effect row; the fixed
> grammar terminal `requires` is ineligible for IDENT under [FORM-3].
> A `fn_decl` named `main` that carries a `requires_clause` is a hard error
> citing FN-7 at that `requires_clause` node [FN-7].
>
> The clause is one `prop` [GRAM-12] and carries no statement, binding,
> message, or failure action.
> Its scope contains only the function parameters, named consts, and the
> function's type and const parameters.
>
> Every `pcall` callee in the `prop` must resolve to an operation-table row
> [OP-1] that is total, non-trapping, and has effect `pure`; every
> `pinfix_tail` must spell such a row.
> Every `pprimary` place is a non-consuming read formed from field-selection
> and `deref` projections only.
> A user-function call, system operation [SYS-1], construction, borrow,
> `move`, subscript `psuffix`, or any trapping, partial, or allocating row is
> a hard error citing FN-8 at that node.
> Every operation result in a `prop` is consequently a copy value: each is
> either an operand of an admitted row, whose operand types are integer,
> float, or `Bool`, or the `prop` root, which is `own Bool`. No separate
> copy judgment is stated or implemented.
> The complete `prop` must satisfy [OP-5]'s condition judgment.
> Normal typing, ownership, [FORM-3], and no-shadowing rules still apply.
> `requires` remains absent from `fn_sig` and cannot discharge a law under
> [FN-4]; contract/refinement support is DEFERRED with a recorded delta.
>
> The checked `prop` tree, after concrete [FN-2] substitution, is the
> function's one GoalTemplate. No expansion, erasure, or normalization step
> exists between the written tree and the template.
> A template datum naming a parameter is identified by that parameter's
> zero-based declaration ordinal followed by its written field and `deref`
> projections before call substitution.
> A named-const datum retains its declaration identity and projections; a
> literal retains its exact type and mathematical or nominal value.
> Every operation node retains the selected operation-table row, written type
> and const arguments actually present at that node after [FN-2] substitution,
> result type, and written operand order.
> The callee-instance identity and `requires_clause` NodePath identify the
> requirement occurrence for diagnostics and checked metadata but are not part
> of predicate equality.
>
> Two instantiated goals are identical exactly when these finite typed
> expression trees are identical.

Everything from "No equality step commutes operands…" through the end of the
ordinary-call paragraph, the S4 paragraph, and the [CLM-3] paragraph is
**unchanged**, except:

- delete the eleven program-start sentences ("Program start is the one
  implemented dynamic boundary…" through "…contributes no source effect
  [EFF-2].") and replace the last of them with one sentence: *"A requirement
  is a checked signature obligation with no executed occurrence anywhere and
  contributes no source effect [EFF-2]."*
- delete the two [CLM-3] entry sentences ("For a marked program entry with a
  requirement…" and "Success never removes or replaces the one runtime wrapper
  evaluation fixed below.").
- replace each "failure cites FN-8 at the requirement final `check_stmt`" with
  "at the `requires_clause` node".
- delete the [PRV-3] entry-leaf paragraph's dependence on the wrapper (§5.9).

Deleted outright from [FN-8]: the six structural-pass sentences; the
clause-local own-copy sentence; the two alpha-expansion sentences; the
"clause-local spellings … absent after expansion" sentence; the eleven
program-start sentences; the two [CLM-3] entry sentences. **Twenty-two
sentences deleted, four added.**

### 5.3 [FN-9] — ensures

> **[FN-9]** Any source `fn_decl` other than the unit entry [FN-7], generic or
> nongeneric, may carry one `ensures_clause` after its optional [FN-8]
> `requires_clause` and before its body.
> The fixed grammar terminal `ensures` is ineligible for IDENT under [FORM-3].
> The clause declares one verified normal-return relation.
> It is neither an executable epilogue nor a trusted assertion, and it is
> absent from `fn_sig`, contract members, system-operation declarations, and
> every dynamic-boundary surface.
>
> The clause is one `prop` [GRAM-12] and carries no statement, binding,
> message, or failure action, so it can express no execution and emits no
> [DIAG-3] record.
> Every `pcall` callee and `pinfix_tail` row must satisfy exactly the [FN-8]
> admitted-row judgment, and every `pprimary` place exactly the [FN-8]
> non-consuming projection judgment.
> The `prop` must satisfy [OP-5]'s condition judgment.
> Normal typing, [FORM-3], and declaration-before-use rules apply.
> Neither the `prop` nor the symbolic result datum is visible in the function
> body.

The selector paragraph is **unchanged** ("A plain `ensures_selector` IDENT is
admitted only when…" through "FN-9 owns the selector field list and candidate
binder as [GRAM-10, TYPE-6] fix."), except that the sentence

> After selector-shape and ordinary freshness succeed but before any
> `ensures_entry` is resolved, scan the structurally admitted direct
> ensures-local `let_stmt` binders in source order; the first binder whose
> spelling equals the symbolic result datum is a hard FN-9 rejection, so an
> accepted ensures local can never shadow that datum.

is **deleted**: there are no ensures locals, so nothing can shadow the result
datum. The selector's freshness against parameters, named consts, and the
paired `value` field is unchanged.

The RelationTemplate paragraph becomes:

> The `prop`'s root must be exactly one `pcall` of `ieq`, `ine`, `ilt`, `ile`,
> `igt`, or `ige`.
> Both operands must be the symbolic result datum, a parameter datum with only
> field and `deref` projections, a named const, a typed integer literal, or
> `len(P)` for an admitted formal place P.
> At least one operand must contain the symbolic result datum.
> No other operation result, arithmetic expression, subscript, ephemeral actual
> datum, Boolean connective, nested result projection, or body local becomes a
> relation term.
> The comparison normalizes to exactly one finite L0 RelationTemplate under
> [ENT-2]; equality is its ordinary two-bound L0 relation but remains one
> semantic relation occurrence.
> The template retains parameter ordinals and projections, selector and field
> declaration identity, named-const identity, typed literals, concrete type and
> const substitutions, selected comparison row, operand order, and normalized
> relation.
> It excludes binder spelling and callee-instance identity.
> Its occurrence identity is `(concrete function instance, ensures_clause
> NodePath, 0)`.

Note what disappears from the exclusion list: "let spelling or sharing,
message bytes, clause-local NodePaths" — none of those objects exists.

Everything from "A plain selector selects every explicit `return`." to the end
of [FN-9] is **unchanged**. That is the whole verification apparatus —
selected exits, entry-image stability, the three views, the SCC schedule, the
establishment formula, the admitted result routes — and Proposal D does not
touch a byte of it. The proposal is a surface change, not a semantic one.

Deleted outright from [FN-9]: the four structural-pass sentences; the ensures-
local visibility sentence; the "final `check` is proposition syntax only"
sentence (now vacuous); the result-datum shadow scan; the alpha-expansion
sentence. **Nine sentences deleted, five added.**

### 5.4 [GRAM-9] — one scope clause

Append one sentence:

> GRAM-9 governs the executable surface only. A `prop` [GRAM-12] contains no
> `atom` position, derives no `call` in one, and forwards no computed value,
> so it neither satisfies nor violates this rule.

### 5.5 [GRAM-6] — one deletion

Delete `check_stmt` wherever [GRAM-6] enumerates statement forms. The "no
precedence, associativity, or parenthesization surface" sentence is unchanged
and now additionally covers `prop` by §3.4.

### 5.6 [ENT-2] — one clause deleted

In the term definition, clause (a) currently reads:

> a `place` [GRAM-5] whose root `pbase` IDENT resolves to any `let_stmt`
> binding, a `for_stmt` binder, a `param`, **a requires-clause local**, any
> match binder regardless of its [OWN-13]-derived mode, or a named const
> [CONST-2]

Delete "a requires-clause local". There are no clause locals. (Verified: no
compiler code binds this case — `compiler/src/semantic/entailment/term.rs` has
no clause-local root, because the clause locals were always expanded away
before any fact was formed.)

The concrete-goal paragraph's sentence

> A concrete goal is one finite typed expression tree with exact result `own
> Bool` formed under [FN-8]'s structural identity, either by concrete
> substitution of a GoalTemplate or by [ENT-3]'s goal-origin judgment in the
> current function.

is unchanged in force and gains precision: "[FN-8]'s structural identity" now
denotes the written `prop` tree rather than an expansion product.

### 5.7 [ENT-3.S4] — the justification collapses to the static half

Current:

> S4 is the admitted-body axiom justified by every ordinary caller's static
> discharge or the successful dynamic boundary check [PROG-3, GATE-1]; no
> callee-entry prologue executes.

Replacement:

> S4 is the admitted-body axiom justified by the static discharge of every
> ordinary call edge in the closed compilation unit [PROG-1, PROG-2]; a
> `requires`-bearing declaration is never the unit entry [FN-7], so it has no
> caller that is not an ordinary source call, and no dynamic boundary
> participates. No callee-entry prologue executes.

This is a *stronger* justification than today's, because the disjunct that
disappears is the one that could be satisfied by a runtime check instead of a
proof. §9 develops the point.

### 5.8 [GIVE-1] — one phrase deleted

In the fact-carrier exclusion list, delete "requires local" from

> A literal, named const, const-generic constant, Z, counted capture, requires
> local, projected place, consuming atom, or any other atom may still be
> delivered as a value but carries no relation through the initializer.

### 5.9 [PROG-3], [DIAG-2], [DIAG-3], [EFF-2], [OWN-1], [FORM-2]

**[PROG-3].** Delete the entry-wrapper machinery: the paragraph beginning "If
the entry has no [FN-8] requirement…" through "…no fabricated call, adapter,
helper, or second body participates." Replace with one sentence:

> The implementation then invokes the entry body once; the entry declares no
> requirement [FN-7, FN-8], so no compiler-owned wrapper, wrapper evaluation,
> owner-retention window, or start-time goal exists.

Everything about standard-input setup, start failure, `ExitStatus` mapping,
and trap termination is unchanged.

**[DIAG-2].** In the checked-program contents paragraph, replace

> its requirement occurrence `(concrete callee instance, final-check NodePath,
> conjunct ordinal 0)`

with `(concrete callee instance, requires_clause NodePath, conjunct ordinal
0)`, and delete "and the one retained program-start goal evaluation when the
entry has a requirement". Replace the two clause sentences

> The final check inside a `requires` block is not an ordinary-callee check:
> its condition is represented by the GoalTemplate, and an executable retained
> check exists only for program start [PROG-3] and a later implemented gated
> boundary [GATE-1]. The final check inside an `ensures` block is represented
> only by its verified RelationTemplate, selected-exit judgments, and
> derivations; it is never an executable retained check.

with one:

> A `requires_clause` is represented only by its GoalTemplate and discharged-
> goal derivations, and an `ensures_clause` only by its verified
> RelationTemplate, selected-exit judgments, and derivations; neither is ever
> an executable retained check, at any boundary.

**[DIAG-3].** Delete the sentence

> For an [FN-8] program-start goal, `rule_id` is `OP-5` and `message` is the
> final `check_stmt`'s STRING value decoded by [FORM-5].

and, in the `node_path` sentence, delete the clause "the final `check_stmt`
whose complete goal fails at program start [OP-5; FN-8, PROG-3];". After this,
`rule_id: OP-5` can never appear in a trap record, and the remaining trap
rule_ids are `CLM-1`, `OP-2`, and the table-operation and [SYS-8] contract
checks. The record grammar, field order, encoding, and byte-identity guarantee
are unchanged.

**[EFF-2].** Replace

> An optional `requires` block is a checked callable-boundary obligation
> [FN-8], and an optional `ensures` block is a verified normal-return relation
> [FN-9]; neither is an executed body occurrence, and neither contributes a
> read, write, allocation, external, blocking, or trapping category.

with

> A `requires_clause` is a checked call-boundary obligation [FN-8] and an
> `ensures_clause` a verified normal-return relation [FN-9]. Neither is a
> `stmt`, neither contains a `.trap` OPNAME, a bare trapping operator, a
> `claim`, or a call to any row whose effect includes `traps`, and every row
> either admits is `pure`; so neither contributes a read, write, allocation,
> external, blocking, or trapping category.

and delete

> The retained program-start check [PROG-3] and any future gated adapter check
> [GATE-1] belong to those dynamic boundaries, not to an ordinary source call
> or the callee's exhibited row.

The point of the rewrite is that today the exclusion is a *stipulation* the
rule has to make against a clause that visibly contains the word `trap`;
under Proposal D it is a *derivation* from the clause's grammar. §8 develops
this.

**[OWN-1].** The position-conditional bare-affine repair survives with
`requires` block reworded to `requires_clause` or `ensures_clause`
proposition. A non-copy own parameter used bare as an operand is still an
[OWN-1] `BareAffineUse`, and the ordinary "write `move p`" fix is still wrong
inside a proposition. This is *not* deleted; §12 counts it against the
proposal's "everything gets simpler" claim.

**[FORM-2].** `requires_block` and `ensures_block` leave the block-bearing
list; `check_stmt` leaves the line-bearing list; `requires_clause` and
`ensures_clause` join the line-bearing list. Replace the three clause-join
rendering sentences with two:

> A function with neither clause puts its complete header through the body `{`
> on one line. A function with either clause ends its header line after
> `effects`, renders each present clause completely on its own line at the
> function's depth including its final semicolon, and renders the body `{` on
> its own line at that depth.

### 5.10 [FORM-3]

`check` and `trap` cease to be exact fixed grammar atoms and therefore become
eligible IDENTs. This is a monotone widening of the accepted set and breaks no
existing source.

---

## 6. The entry restriction

Stated as a rule, with exact wording and home. It is phrased over `main`, not
over `program_kind`, because [FN-7] admits an *unlabelled* entry that carries
no `program_kind` child and 451 of the tree's 532 `fn main` declarations take
that form — a `program_kind`-phrased restriction would miss all of them, and
would miss `fn8-trap-requires-false.wf`, the one entry that actually exercises
the machinery.

Home: **[FN-7]**, appended to the paragraph that fixes the entry's shape,
because [FN-7] is already the rule that says what an entry is and is not.
[FN-8] and [FN-9] cite it.

> The unit entry declares no requirement and no postcondition: a `fn_decl`
> named `main` that carries a `requires_clause` or an `ensures_clause` is a
> hard error citing FN-7 at that clause node, with `SourceCoordinate` equal to
> the clause's complete checked half-open source extent and the restructuring
> `move the condition into the entry body as a real branch, or into a called
> function's contract`. This holds for both entry forms. An entry has no
> ordinary caller that could discharge an obligation, so a contract on it
> could only be enforced by an executed boundary check; the language defines
> no such check.

The last sentence is the whole argument, and it is the reason this restriction
belongs in the language rather than in a style guide: **a contract is a
caller-side obligation, and the entry is the one declaration with no caller.**
Barring the combination removes the only case in which "the contract must
execute" could ever be true.

Measured cost: three conformance cases (§2.7, priced in §11).

---

## 7. The three positions

The brief asks for "the three positions". Two readings are live; both are
answered.

### Reading A — the three positions of `check e else trap "msg";`

v0.32's check dissolution already removed `check_stmt` from [GRAM-4]'s `stmt`
alternation, so today the spelling occurs in exactly two grammar positions and
carries three semantics:

| position | today | Proposal D |
|---|---|---|
| final of a `requires` block, ordinary call | compile-time caller obligation; never executes | `requires_clause`; unchanged semantics, no `check`, no `trap`, no STRING |
| final of a `requires` block, program entry | **executes**; traps with `rule_id` `OP-5` and the writer's STRING | **deleted** by §6 |
| final of an `ensures` block | never executes; the `else trap` half is wholly inert | `ensures_clause`; unchanged semantics, no `check`, no `trap`, no STRING |

After the change, `check_stmt` has zero positions and is deleted from the
grammar. The one spelling that meant three things is replaced by two
keywords, each meaning exactly one thing, neither of which mentions a failure
action.

### Reading B — the three positions a proposition could occupy

| position | disposition |
|---|---|
| `fn_decl` `requires_clause` | admitted, on any non-entry declaration |
| `fn_decl` `ensures_clause` | admitted, on any non-entry declaration |
| `fn_sig` inside a `contract_decl` | **still absent**, unchanged. [FN-8]'s "requires remains absent from `fn_sig` and cannot discharge a law under [FN-4]" is carried over verbatim; contract/refinement support stays DEFERRED with its recorded delta. `conform`-bound functions likewise still carry neither clause [FN-3]. |

A fourth candidate — a future foreign-boundary declaration — is §9.

---

## 8. The effect-row story

Today [EFF-2] must *stipulate* that a contract contributes no effect, and the
stipulation is load-bearing against a clause whose final statement literally
contains the token `trap`. The rule's own body-syntactic clause says a
function "exhibits `traps` iff the body contains … a `.trap` OPNAME, `claim`,
or a call to any operation or function whose effect row includes `traps`", and
the only thing keeping the `requires` clause out of that test is that the
clause is not "the body". That is an exclusion by scope, and it is exactly the
kind of exclusion a reader has to be told rather than able to see.

Under Proposal D the exclusion is derivable from three structural facts:

1. A `prop` is not a `stmt` and is not in the body, so the body-syntactic
   contribution does not reach it — unchanged.
2. A `prop` contains no `trap` token of any kind: the fixed atom is deleted
   from the grammar, and the `.trap` OPNAME suffix is excluded by [FN-8]'s
   admitted-row judgment (a `.trap` row is not total and not `pure`).
3. Every row a `prop` may name is `pure`, total, non-trapping, and
   copy-result. No `claim`, call, construction, allocation, or borrow can
   appear.

The release contribution is untouched, since a `prop` names no owner and
creates no binding that could require release. [EFF-2]'s existing consequence
"A function whose body and release contribution are empty may therefore
declare `pure` while carrying a requirement" stays true and becomes obvious
rather than surprising.

One deletion: [EFF-2]'s sentence about the retained program-start check and
the future gated adapter check goes with the boundary itself.

---

## 9. Composing with the FFI destination

The owner's first ruling made the entry check a tolerated temporary exception
destined for an FFI boundary specification; the second ruling superseded it by
removing the exception now. Both are addressed, because the FFI question
survives the second ruling.

### 9.1 The structural claim, checked against my exact form

The claim: an expression with no statement context and no trap vocabulary
cannot express "executes here, not there", so it enforces the ruling
structurally rather than complying with it.

This is true of the exact form in §3, and the mechanism is worth naming
precisely. An executed boundary check needs three things: a **predicate**, a
**failure action**, and a **report**. Today the block form supplies all three
in one statement — `check e else trap "msg";` is predicate, action, and report
message glued together — and that gluing is why the entry semantics leaked
into the contract surface in the first place. A `prop` supplies only the
predicate. It has no statement position in which an action could be written,
no STRING in which a report could be carried, and no admitted row whose effect
is anything but `pure`. A writer cannot say "trap here" in a `prop` because
there is nowhere in a `prop` for the word to go.

That is stronger than compliance, but it is not a proof of anything about the
*compiler*: a compiler could still be written to evaluate a `prop` at a
boundary and synthesize a failure action. What the form guarantees is that the
**writer** cannot request it and the **source** cannot record it, so no
declaration in the language can mean "executes here, not there". That is the
exact claim, and it is the one worth making.

### 9.2 What happens to the entry check in the meantime

Nothing: there is no meantime. Under §6 the entry cannot carry a contract at
all, so no entry runtime check exists to relocate. This is the answer to
"where does the entry's runtime check come from if the contract is a pure
expression?" — it comes from nowhere, because the case that needed it is
removed rather than re-hosted.

A writer who genuinely needs entry-time validation writes it in the entry body
as a real branch, which is what [ENT-3] source S1 already is and what
[PRV-3]'s existing advice already says ("A real source branch in the body
remains S1 in U and B and may discharge it"). That path is strictly better
than the wrapper: it is visible, it is ordinary source, it can produce a
sensible `ExitStatus` instead of a trap, and it is already fully specified.

### 9.3 Composing with a future FFI boundary contract

Proposal D composes with it and needs no reconciliation, because it separates
the two objects that the block form fused.

A foreign boundary declaration must eventually say something like: *at this
boundary, validate P; on failure, do R*. P is a proposition. R is a failure
action with a report. Under Proposal D, P is exactly a `prop`, and the FFI
specification can reuse the `prop` production verbatim as its predicate
grammar; R lives in the boundary declaration, where a failure action belongs,
alongside the other things only a boundary has — the calling convention, the
foreign type mapping, the report channel. Nothing has to be un-glued first.

Under the block form, the FFI specification would face a choice between two
bad options: reuse `requires`'s block and inherit its `else trap "msg"` (thus
permanently entangling the trap vocabulary with the ordinary caller-side
contract), or introduce a second, parallel proposition surface for boundaries.
Proposal D makes the first option unavailable and the second unnecessary.

### 9.4 What justifies S4

Today [ENT-3.S4] is justified by a disjunction: "every ordinary caller's
static discharge **or** the successful dynamic boundary check". The second
disjunct exists for exactly one reason — a program entry has no ordinary
caller, so something else had to establish its requirement, and the runtime
wrapper was that something.

Under §6 the disjunction collapses to its first branch (§5.7). Every
`requires`-bearing declaration is an ordinary function; the compilation unit is
closed [PROG-1, PROG-2]; every written call edge into it must discharge its own
instantiated goal ([FN-8], unchanged); therefore S4 is justified by
quantification over a finite, statically known set of call edges, with no
runtime evidence anywhere in the chain. This is the stronger justification,
because the disjunct that disappears is the one where a *check* rather than a
*proof* was the warrant.

**The one honest gap, unchanged by this proposal.** A `requires`-bearing
function with zero callers gets S4 from an empty quantification: `requires
ilt(1_u64, 0_u64);` is vacuously justified and hands its body an inconsistent
fact state, from which any relation follows and any bounds check can be
elided. This is sound for execution — the function is never called — but the
elided checks are still lowered into a body that a later edit could make
reachable, at which point the new caller must discharge the goal and the
contradiction becomes unreachable again. So the hole is closed by [FN-8]'s
call-edge rule at the moment it would matter.

That hole exists today, identically, for any uncalled non-entry function with
a requirement. What §6 changes is that it removes the one case that was
*caller-less and executed*: an entry with a requirement. That case is exactly
what the dynamic wrapper existed to close. Removing the case removes the need
for the wrapper — which is a much better outcome than keeping both.

---

## 10. What is deleted

### 10.1 Specification

| item | disposition |
|---|---|
| [GRAM-2] `requires_block`, `requires_entry`, `ensures_block`, `ensures_entry` | **delete** |
| [GRAM-4] `check_stmt` | **delete** (unreachable) |
| fixed atoms `check`, `trap` | **delete** from the grammar; become legal IDENTs [FORM-3] |
| [FN-8] structural pass (6 sentences) | **delete** — the grammar does it |
| [FN-8] clause-local own-copy sentence + copy judgment | **delete** — subsumed by the operand typing of the admitted rows plus the `own Bool` root; nothing replaces it |
| [FN-8] alpha expansion (2 sentences) + "spellings absent after expansion" | **delete** — no expansion exists |
| [FN-8] program-start block (11 sentences) | **delete** |
| [FN-8] [CLM-3] entry sentences (2) | **delete** |
| [FN-9] structural pass (4 sentences) | **delete** |
| [FN-9] ensures-local visibility, result-datum shadow scan, alpha expansion | **delete** |
| [FN-9] "final check is proposition syntax only" | **delete** — vacuous |
| [OP-5] `check_stmt` subject, program-start semantics, `ensures` sentence | **delete**; [OP-5] retained as the condition judgment |
| [DIAG-3] `rule_id: OP-5` row and its `node_path` clause | **delete** |
| [DIAG-2] retained program-start goal evaluation; two clause sentences | **delete**, one replacement sentence |
| [PROG-3] entry-wrapper paragraphs | **delete**, one replacement sentence |
| [EFF-2] program-start / gated-adapter sentence | **delete** |
| [ENT-2] "a requires-clause local" | **delete** |
| [GIVE-1] "requires local" | **delete** |
| [ENT-3.S4] dynamic-boundary disjunct | **delete**, replaced by §5.7 |
| [FORM-2] three clause-join rendering sentences | **replace** with two |
| [GATE-1] | **reserve.** [GATE-1] itself survives unchanged as the gated-editing rule. Four of its seven citations die (FN-8's "later gated foreign callable boundary", EFF-2's "future gated adapter check", DIAG-2's "later implemented gated boundary", ENT-3.S4's justification). The three that survive (its own text, SYS-1's non-membership note, §14's family stub) are about *gated editing*, not about a dynamic contract boundary. Recommendation: do **not** hang the future FFI boundary on GATE-1; give it its own rule when it lands. GATE-1 is a toolchain-authority rule that acquired an unrelated second job by proximity. |

Approximately **35 sentences and 5 productions out; 9 sentences and 6
productions in.**

### 10.2 Compiler

| file | disposition |
|---|---|
| `compiler/src/resolution/engine/admission.rs` (188 lines) | **delete entirely.** This file *is* the [FN-8]/[FN-9] structural pass: `check_clause_blocks`, `clause_entry_kind`, `ShapeIssue::{InvalidEntry, MissingFinalCheck}`, and the `RequiresShapeIssue`/`EnsuresShapeIssue` diagnostic kinds. The grammar replaces all of it. |
| `compiler/src/semantic/check/requires.rs` (972 lines) | **large deletion.** Gone: `clause_entry_statement`, `clause_statement_expression`, `validate_clause_statement`, `validate_clause_let`, `validate_clause_copy_local`, the `expanded_bindings` map, `ExpandedClauseExpression`, `ExpandedClauseDatum`, `build_clause_expression`'s substitution path, and the per-entry `check_statement` call with its `can_continue` handling. Retained and simplified: the admitted-row filter (`validate_clause_operation`, `validate_clause_infix`, `validate_clause_atom`, `validate_clause_place`) and goal-tree construction, which now walk a `prop` directly instead of reconstructing a tree from checked statements. `clause_conditional_repair` (the [OWN-1] bare-affine repair) is **retained** — see §12. |
| `compiler/src/semantic/check/ensures.rs` (1460 lines) | **partial deletion**: the ensures-local path and the result-datum shadow scan. The RelationTemplate, selected-exit, three-view, SCC-schedule, and establishment machinery — the great majority of the file — is untouched. |
| `compiler/src/lowering/builder/entry_goal.rs` (600 lines) | **delete entirely.** No entry goal is lowered. |
| `compiler/src/semantic/check/entry_form.rs` (565 lines) | **retained**, minus its requirement interaction; gains the §6 rejection. |
| `compiler/src/resolution/scopes.rs`, `engine.rs` | `ScopeKind::RequiresBlock` disappears (no clause scope to push, since the `prop` scope is just the parameter scope). `ScopeKind::EnsuresBlock` is retained for the selector's result-datum candidate. |
| `compiler/src/syntax/grammar/generated.rs`, `parser/finalize/canonical/format.rs` | regenerated for the new productions and the new [FORM-2] rendering |
| `compiler/src/semantic/check/contracts.rs`, `nominal_instances.rs` | production-name updates only |

Order-of-magnitude: **roughly 800–1000 lines of checker and lowering code
deleted**, against a new `prop` parse path of perhaps 60 lines in the
generated parser and a small rewrite of goal construction. The parser grows;
the checker shrinks by much more.

---

## 11. Migration

### 11.1 Recipe

Mechanical, per clause, and fully determined:

1. Topologically expand the clause locals into the final check condition —
   the same substitution [FN-8] already performs, so the compiler already has
   the code to emit the answer.
2. Render the expanded tree in `prop` syntax: a `pcall` for each operation
   call, an infix for each operator spelling, and no parentheses.
3. Emit `requires <prop>;` or `ensures <selector>: <prop>;` on its own line
   after the header line.
4. Drop the `else trap "STRING"`. §11.3 answers where the string goes.
5. Re-render the declaration under the new [FORM-2] rule.

Step 1 is exactly `build_clause_expression` plus a pretty-printer, so a
one-shot migration tool is a few dozen lines against the existing checker and
is deleted after use.

**Blocking case:** a clause whose expansion needs an infix operand that is
itself infix (§3.4). Measured occurrences in the live tree: **zero**.

### 11.2 Measured cost

Counted at file granularity, because a protected conformance case is approved
as a file:

| set | files | clauses | cost |
|---|---|---|---|
| `tests/conformance/cases` | **45** | 60 | every one changes bytes: at minimum the `check e else trap "…";` line becomes a `prop`, and the [FORM-2] rendering changes. 30 of the 45 additionally have at least one clause-local to expand. **All 45 are protected evidence.** |
| `tests/programs` | 6 | 8 | ordinary mechanical rewrite; longest result 62 chars on one line |
| dormant fixtures, `tests/codegen/cases` + `research/experiments` | 5 | 40 | do not parse under v0.32 today and are driven by no gate; migrate if and when they are revived, and only `match_copy.wf` is hard |

**45 protected conformance files is the honest headline number, and it is not
small.** Two things qualify it, and only the second is specific to this
proposal:

- It is the study-wide floor. *Any* proposal that changes how a contract is
  spelled rewrites the same 45 files, because 45 files is simply how many
  conformance cases contain a contract. Proposal D is not more expensive than
  its rivals on this axis.
- Proposal D's **marginal** cost over a rival is the subset whose *subject*
  disappears rather than being re-spelled — ten cases, each of which needs a
  decision rather than a rewrite:

| case | today's subject | disposition |
|---|---|---|
| `fn8-neg-doc-only-clause` | clause is `doc`-only, no final check | **retires**; becomes a grammar non-derivation |
| `fn8-neg-requires-no-check` | lets with no final check | **retires**; grammar non-derivation |
| `fn8-neg-requires-set` | `set` inside a clause | **retires**; grammar non-derivation |
| `fn8-neg-requires-control` | `return` inside a clause | **retires**; grammar non-derivation |
| `fn8-neg-requires-local-in-body` | a clause local is not visible in the body | **retires**; there are no clause locals |
| `fn8-pos-requires-name-reuse` | a clause local's name is fresh again in the body | **retires**; there are no clause locals |
| `fn8-neg-requires-noncopy-local` | `let xs = array_new<u64,4>(…)` is non-copy | **retires**; the property becomes structurally unreachable (§5.2) rather than checked |
| `fn8-neg-requires-noncopy-cvt-local` | `cvt<u8,i8>` derives `Result`, inadmissible | **retires**; same |
| `fn8-trap-requires-false` | the program-start requirement trap | **retires with the machinery**; see below |
| `clm3-neg-generated-wrapper-check` | the generated entry wrapper | **retires**; see below |

Retiring a positive case is a real loss of coverage even when the property it
tested becomes unrepresentable, and each retirement is a protected-evidence
deletion needing its own audit and approval. Five of the ten (the structural-
pass negatives) are replaced one-for-one by grammar-derivation negatives whose
verdict rule changes from `FN-8` to the grammar rule; the other five have no
replacement because their subject ceases to exist.

`form3-neg-requires-binding` is **unchanged**: `requires` remains a fixed
grammar atom, so rebinding it is still a [FORM-3] rejection with the same
verdict.

The three entry cases, priced individually:

- **`fn8-trap-requires-false`** (`{"kind":"trap"}`, rules `[FN-8, SCOPE-4]`).
  Its subject is precisely the program-start requirement trap. It **retires
  with the machinery**; the honest replacement is a new negative case whose
  verdict is `{"kind":"reject","rule":"FN-7"}` and whose subject is the §6
  restriction. This is the one case whose *verdict class* changes, and it is
  the sole witness for `rule_id: OP-5` in a [DIAG-3] trap record.
- **`clm3-neg-generated-wrapper-check`** (`{"kind":"reject","rule":"FN-8"}`).
  Its subject is the generated entry wrapper. **Retires**; its actual semantic
  content — "U does not compose an atomic Boolean conjunction from its two
  true comparisons" — is worth preserving as an ordinary non-entry function
  with the same contract, which keeps the `{"kind":"reject","rule":"FN-8"}`
  verdict.
- **`clm3-pos-transitive-value-branch`** (`{"kind":"run","exit":0}`). The
  entry's requirement is `ieq(0_u64, 0_u64)` — trivially true and incidental
  to the case, whose real subject is a relay, a required callee, and a
  seedless mutual SCC. **Drop the entry clause**; the verdict is unchanged and
  the rule list loses `PROG-3`.

Every one of these is protected conformance evidence. So are the other 42
files. All 45 need an exact before/after audit, an owner explanation and
approval, and an approval-ledger entry; ten of them additionally need a
decision about a subject that no longer exists. **Not zero, and not small.**

### 11.3 Where the STRING goes: deleted

The `else trap "msg"` STRING is **deleted, with no replacement label.** Three
reasons, in order of force:

1. **Its only consumer is gone.** The STRING's one normative use is [DIAG-3]'s
   `message` field for the program-start trap record. §6 removes the only site
   that emits one. In every other position the STRING is already inert — [FN-9]
   says so outright ("its message … ha[s] no identity").
2. **A rejection does not need it.** [DIAG-1] already identifies the exact
   offending node by `SourceNode(NodePath, SourceCoordinate)`, and [FN-8]'s
   call-site rejection already carries the exact residual, the concrete callee
   instance, and the failed premise. A writer-supplied string adds nothing a
   diagnostic can act on, and it can *contradict* the predicate, which is
   worse than absent.
3. **Prose about a declaration already has a home.** `doc STRING ";"` is the
   declaration documentation form [GRAM-2, FORM-4]. A writer who wants to
   explain a contract writes it in the function's `doc`, where the rest of the
   declaration's prose already lives. Adding an optional label to the clause
   would create a second home for the same thing and a second spelling of the
   same construct — a direct [FORM-1] violation.

A `claim` keeps its `because STRING` and should: a claim *asserts* something
the compiler cannot prove, and the justification is the writer's argument for
being believed. A contract asserts nothing; it is checked. That asymmetry is
the whole reason the owner rejected reusing `claim`, and it is the same reason
the contract needs no string.

---

## 12. What this costs

Stated without softening.

### 12.1 A five-local contract in this form

There is no five-local contract in the live tree; the maximum is four, and it
becomes a comfortable 62-character line (§2.5). So the honest test is the real
worst case, `match_copy.wf` at 27 locals. In this form, with v0.32 operator
spellings and with no parenthesized-infix extension, it is **not spellable at
all** — it needs `seed_len +wrap (product_low +wrap product_shifted)`, an
infix operand that is itself infix (§3.4). With the deferred parenthesis
extension it becomes a **1087-character, depth-10 single expression**. Rendered
on one line, it is unreadable, and no amount of arguing about medians changes
that. A reader cannot see that `distance` is bounded three ways, and a
reviewer cannot check the multiply-overflow argument at all.

Two things are true about it and neither rescues the form:

- The contract as written is a *design smell independent of the surface*: 12 of
  its 27 locals are a hand-rolled 128-bit multiply-overflow test that the
  v0.32 `imulhi` row collapses to one node, taking the whole contract from 47
  operation nodes to 8, at which point it is roughly 200 characters and depth
  4 — large but readable. The block form let that smell hide comfortably for a
  year; the expression form would have made it obvious on day one. That is an
  argument *for* the form, but it is an argument about incentives, not about
  the worst case.
- The file does not parse under v0.32 and no gate drives it. That is an
  argument about *urgency*, not about correctness.

**Verdict on my own worst case: a genuinely large contract does not survive
this form well.** The form bets that large contracts are rare and usually a
sign of something else being wrong. The corpus supports the bet — 62 of 64
live contracts are at most two locals — but a bet is what it is.

### 12.2 Have I just moved the statement machinery into an expression?

No, and the discriminating test is what code disappears rather than what
vocabulary is used. The block form's machinery is:

1. a structural pass policing a statement subset (188 lines,
   `admission.rs`) — **gone**, because the grammar admits nothing to police;
2. a clause-local copy judgment (`validate_clause_copy_local`) — **gone**,
   and not replaced: with no binding, every operation result has a consumer
   whose operand type already constrains it, so non-copy results are
   unreachable by construction (§5.2). The judgment existed because a `let`
   binding is an unconstrained sink for any value;
3. an alpha-expansion algorithm (`ExpandedClauseExpression`,
   `expanded_bindings`, `build_clause_expression`'s substitution path) —
   **gone**, because the written tree is the template;
4. a clause scope (`ScopeKind::RequiresBlock`) — **gone**;
5. a result-datum shadow scan in [FN-9] — **gone**, because nothing can
   shadow.

If I had proposed a `where`-style binding list on the expression, items 2, 3,
4, and 5 would all have come back under new names and only item 1 would have
gone; I would have renamed the machinery, not deleted it. That is exactly why
Proposal D has no binding facility at all, and it is why the corpus
measurement had to come first: without §2 showing a mean of 0.9 locals, the
no-binding choice would be indefensible and I would have had to propose the
`where` list and accept that criticism.

What I *have* added is a second expression grammar (six productions, §3.2),
which is a real cost: net +1 production, a second `SELECT_2` region to keep
disjoint, a second set of node kinds, and a second place in the parser where
"is this a call or a place?" is decided. If the metric is grammar size,
Proposal D loses. If the metric is rule text and checker code, it wins by
roughly 35 spec sentences and 800–1000 lines.

And two pieces of machinery genuinely survive, which I will not pretend
otherwise about: the [OWN-1] position-conditional bare-affine repair (§5.9)
still needs its clause-specific wording, because a non-copy own parameter used
bare in a `prop` is still a consuming use with a wrong default fix; and
[FN-8]'s admitted-row filter is unchanged in size — every sentence about which
operation rows a contract may name is still needed, because that judgment was
never about statements.

### 12.3 The strongest rival arguments against me

It is not the worst case, which is a dormant file. There are two, and the
second is the one I find hardest to answer.

#### Rival 1 — names are the readable part

> **The block form's locals are names, and names are the readable part.**
>
> `let distance_in_history = ile(distance, seed_len);` tells a reviewer what
> the subterm *means*. `ile(distance, seed_len)` inlined into a `band` tree
> tells them only what it *computes*. Proposal D's own §2 data can be read
> against it: 40 of 64 live clauses have at least one local, and a writer who
> introduces a local for a two-node subterm is not routing around [GRAM-9] —
> they are naming a concept. Proposal D deletes the only naming facility the
> contract surface has, and the measured "median 18 characters" is partly an
> artifact of a corpus written by people who knew the surface was awkward and
> kept contracts small to avoid it. As contracts get more ambitious — and the
> whole point of the proof fragment is that they should — the missing names
> will be felt exactly when the contracts finally matter. A `where` list
> costs one production and keeps them.

I think this argument is wrong on the specific data and right in principle,
and the split is worth being precise about.

Wrong on the data: of the 40 live clauses with a local, the great majority
have exactly one, and the expanded predicate is 18 characters at the median.
A name for an 18-character predicate is not a concept, it is a line break. The
two `ensures` locals in the tree are provably not names — they are `let
capacity = len(deref(destination));` written *only* because [GRAM-9] forbids
`len(…)` in an atom position, and [FN-9] already accepts `len(P)` as an
operand directly.

Right in principle: at 8+ nodes a name is doing real work, and Proposal D has
no answer except "don't write those". If the project's contracts grow, the
right response is **not** a `where` list — that reintroduces items 2–5 of
§12.2 — but a *named proposition declaration*: a `prop`-valued item, reusable
across contracts, checked once, with the admitted-row judgment applied at its
declaration. That is a genuinely different object from a clause-local: it has
a declaration site, an identity, and reuse, and it does not put a statement
context back inside a contract.

I am deliberately **not** proposing it now, because nothing in the live corpus
needs it and the project's goal discipline forbids building the machinery for a
hypothetical path. **Trigger:** the first live contract whose proposition
exceeds eight operation nodes after every operation-table collapse has been
applied. Nothing in the tree is close: the live maximum is five.

#### Rival 2 — you are deleting ten protected tests, and green will still be green

> Proposal D retires ten conformance cases (§11.2), five of which have no
> replacement because the property they test becomes unrepresentable. A gate
> that no longer contains `fn8-neg-requires-noncopy-local` cannot tell you
> that non-copy results are still inadmissible; it can only tell you that the
> source no longer parses. "Structurally impossible" is a claim about the
> grammar, and the grammar is exactly the artifact this proposal is
> rewriting — so the argument that retires the test is the argument the test
> would have checked. The same holds for the entry pair: `rule_id: OP-5`
> disappears from [DIAG-3] and its only witness is deleted in the same change,
> so nothing observes that the program-start trap really is gone rather than
> merely untested. A surface change that deletes its own falsifiers is exactly
> the shape of change that looks clean and is not.

This is the strongest argument against the proposal and I cannot fully defuse
it. Three partial answers, offered as partial:

1. The five structural-pass negatives *do* get one-for-one replacements — the
   same source, a grammar-derivation verdict instead of an [FN-8] verdict — so
   the coverage moves rather than vanishing. That is five of ten.
2. The copy-judgment pair is genuinely unreplaceable, and the honest mitigation
   is a *derivation obligation*, not a test: §5.2's subsumption argument
   (every operation result in a `prop` is either an admitted row's operand or
   the `own Bool` root) must be checked exhaustively against the operation
   table before this proposal is adopted, row by row, and recorded. If a
   single row admits a non-copy value in an operand position, the argument
   fails and the copy judgment must come back as an explicit premise.
3. The entry pair is the one place where I would insist on a replacement even
   though the machinery is gone: a new negative case whose verdict is
   `{"kind":"reject","rule":"FN-7"}` on an entry carrying a `requires_clause`
   is the falsifier for §6, and it is strictly cheaper to check than the
   wrapper it replaces.

What none of this recovers is the general point, which stands: **this proposal
shrinks the gate's surface in the same change that shrinks the language's, and
a green gate afterwards says less than a green gate before.**

---

## 13. Summary of the bet

Proposal D bets that a contract proposition is small, and pays for that bet
with a second expression grammar. The corpus says the bet is currently safe by
a wide margin — mean 0.9 locals, median 18 characters, maximum 62 characters
across 64 live clauses, and every `ensures` in the tree collapsing to one
binding-free line. What it buys is that the contract surface stops being a
statement context that isn't a body: the structural pass, the copy judgment,
the expansion algorithm, the clause scope, the shadow scan, and the entire
`check`/`else`/`trap` vocabulary all go, and the trap vocabulary has nowhere
left to hide because there is no statement to carry it.

What it costs, in one line each:

- **+1 grammar production** and a second `SELECT_2` region to keep disjoint.
- **45 protected conformance files** change bytes — the study-wide floor, not
  specific to this proposal.
- **10 of those retire or change verdict class**, five with no replacement.
  That is Proposal D's marginal cost and it is the part worth arguing about.
- **A genuinely large contract does not survive the form well.** The bet is
  that large contracts are rare and usually a symptom; the corpus supports it
  today and does not prove it.

The cheapest falsifier: take the ten retiring cases in §11.2, and for each ask
whether the property it tests would still be *observable* after the change. If
five of them come back as grammar negatives and the copy-judgment subsumption
argument survives a row-by-row check of the operation table, the proposal is
as clean as it claims. If either fails, it is not.
