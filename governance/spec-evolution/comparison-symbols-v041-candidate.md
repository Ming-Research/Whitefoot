# Integer comparison symbols and the call-site `::` delimiter — v0.41 candidate

Status: ACTIVATED as v0.41 on 2026-09-03 on the work branch. The stable file
`spec/kernel-spec.md` is headed v0.41, declares `ACTIVE v0.41`, and hashes to
`899437ecf48691b9bc436c86a56ccc2a47fc4eb9290d546010296db7808c5761`; the
outgoing v0.40 bytes
(`15ec2f6f475a7b70fb2654026ec3b6ef79afca3bd588fb38f22005d6637c0168`) are
archived at `spec/kernel-spec-v0.40.md`, and the merge-time record in
`governance/APPROVALS.md` becomes effective with the owner's merge approval of
the exact revision containing it. Drafted the same day as a candidate declaring
`CANDIDATE v0.41 supersedes v0.40 15ec2f6f475a7b70fb2654026ec3b6ef79afca3bd588fb38f22005d6637c0168`,
which hashed to `55ee571e7f342471b16078da05fa8b3bfdab11fbe6819a925baad83687966c06`;
activation flipped that status line and changed no other byte. Retained as the
record of the rulings, the rejected alternatives, and the delta; it is removed
only if a later specification restates them.

## 1. What the batch does

The second FLOOR-5 spelling batch, carrying the owner rulings of 2026-09-03
(`research/investigations/spelling-relief/SWEEP.md`, "Comparison respelling"):

1. The six integer comparisons `ieq ine ilt ile igt ige` are respelled
   `== != < <= > >=` as one `compare_op` class of `infix_tail`. They are
   integer-only table rows exactly as the arithmetic symbols are; `feq`..`fge`
   and `eeq`/`ene` keep their prefixed names.
2. Call-site type application is delimited by `::` — `cvt::<u8, u32>(w)`,
   `open_file::<'f, 'n>(...)` — so `IDENT "<"` begins only a comparison and
   every grammar decision keeps its two-token bound. Constructors and type
   position are unmarked. `::` is a delimiter, admitted on the ground the
   `for` header's parentheses are: T1 governs elements, not delimiters.
3. `!=` is inequality; `!` enters the token alphabet only inside that compound.
4. In proof position the four ordered symbols replace `ile`/`ilt`/`ige`/`igt`;
   `==`/`!=` stay outside the invariant surface. A multiplied relation-form
   use step is parenthesized, `use 3 * (a <= b);`, and a bare one is not.
5. `infix_op` stays the arithmetic list and `const` keeps reusing it;
   `compare_op` is its own production so `f::<n > 1>` cannot derive.

Rejected alternatives, with the reason each lapsed: strong-LL(4) without a
delimiter (commits a comparison with a nested right operand to the call arm
and turns DIAG-1's two-token GRAM-9 attribution into a four-token case
analysis; parsing measures 0.13–2.5% of compile time, so cost decided
nothing); `<>` and `/=` for inequality (LEX-1); deleting the multiplied
relation-form use (its rewrite through a named invariant changes proof
structure, a semantic change disguised as spelling); symbols for Bool logic,
the bit family, float or enum comparison, or any unary operation (LEX-1
short-circuit divergence, `>>` against nested generics, no prefix-operator
position).

## 2. Delta declaration

Numbered rules +0/-0 (131 remain); grammar productions +1 (`compare_op`, 83);
compound punctuation tokens +5 (`==`, `!=`, `<=`, `>=`, `::`; 8); token bytes
+1 (`!`); writer operation spellings +6/-6; fixed terminals 93 -> 98
(predicates 101 -> 106). 21 rules modified at
38 verbatim-anchored sites:
DIAG-1, EFF-2, ENT-3, ENT-6, EX-1, FN-9, FORM-2, GRAM-1, GRAM-4, GRAM-5, GRAM-6, INV-1, OP-1, OP-2, OP-7, OP-8, OP-9, PRF-1, STOR-2, SYS-2, TYPE-4. The accepted-program set is unchanged up to
respelling; no obligation, origin, or diagnostic identity moves.

## 3. Acceptance-set delta

Every v0.40 program has exactly one v0.41 spelling, computed from its tree:
each comparison call becomes the infix form over the same two atoms; each
call-site `targs` gains `::`; each invariant or use relation becomes the
infix affine relation; a multiplied relation-form use gains its parentheses.
Conversely a v0.40 spelling is now rejected at its own rule: `ile(a, b)`
resolves no callee ([OP-1]), `f<T>(x)` parses `f < T` and fails at the TYPEID
where an atom is expected ([GRAM-5]), and `use 3 * a <= b;` does not derive
([GRAM-4]). No program changes verdict for any other reason.

## 4. Derived material carried in the same work

- Compiler: lexer compounds and the `!` byte; `FixedTerminal` inventory;
  grammar tables regenerated from the candidate EBNF (`compare_op`; the
  `call` and `proof_use` decisions); the DIAG-1 row-1 and row-2 attributions
  (`(IDENT, "::")` replaces `(IDENT, "<")`); the FORM-2 renderer and audit
  (`::` in both attachment sets; `compare_op` `<`/`>` in neither); the
  resolver's invariant and use roles; the checker's infix, contract-clause,
  invariant, and use readers; the operation catalog and its reserved-name
  derivation; goal and invariant renderings.
  The backend's per-activation qualification review pin (`REVIEWED_FOR` in
  `compiler/src/backend/qualification.rs`) is bumped to v0.41 with its dated
  review note: the respelled rows lower to the same `icmp` predicates, and no
  system operation, resource, release row, result shape, entry form, or host
  ABI mapping changes.
- Corpus and evidence (rule 4 content): every `tests/**/*.wf` case,
  `tests/snapshot/index.tsv`, and `tests/conformance/manifest.jsonl`
  respelled mechanically by the one-shot token rewriter (comparison calls to
  infix at the top-level comma; call-site `<` to `::<` when the closing `>`
  is followed by `(` and the name is not a type keyword or a declaration);
  the `doc` prose that named a retired operation respelled by hand. Verdicts,
  rule citations, statuses, and coverage rows are unchanged.
  Five conformance cases are added for the new rules: `gram5-pos-comparison-operators`
  (all six operators and a delimited call run to exit 0),
  `gram5-neg-type-application-without-delimiter` (a bare `cvt<u8, u32>(...)`
  parses as a comparison and fails at `u8`; DIAG-1 row 3 attributes that
  keyword to FORM-3), `gram4-neg-multiplied-use-relation-bare` (a multiplied
  relation-form use without parentheses does not derive),
  `prf1-pos-multiplied-relation-use` (`use 3 * (a + b <= c + d)` closes a
  factor-three target), and `inv1-neg-equality-relation` (`==` as an
  invariant relation is rejected citing INV-1).
- Docs: `README.md`, `compiler/README.md`, `docs/patterns.md`,
  `docs/why-whitefoot.md` (current examples only; the historical kernels in
  Part II keep their historical spelling), `docs/current-plan.md`,
  `docs/roadmap.md` (FLOOR-5), [mcts_mem/whitefoot/surface-form/operation-spelling.md](https://github.com/mbbill/Whitefoot/blob/0f22b026b062c36ef2f8ce9c725ee4307e1fda64/mcts_mem/whitefoot/surface-form/operation-spelling.md).
- Research programs: the programs a maintained runner still compiles
  through the current compiler are respelled by the same rewriter and were
  compiled through the branch compiler afterwards: the ten
  `research/experiments/io-completion-bench/programs/*.wf` benchmarks that
  the `io-bench` and `io-hosts` workflows build, and
  `research/experiments/buffer-initialization-cost/drain.wf`. Programs that
  already failed to parse on `main` for an unrelated reason (the `for @label`
  loop form in `many_files_loop.wf` and the three
  `research/experiments/wfgrep-double-walk/shapes/*.wf`), and the dated
  evidence under `research/investigations/` and
  `research/experiments/blind-writer/`, keep the spelling of the
  specification they were written against; they are records, not derived
  material.
- Conformance cases that reached `main` after this branch was cut are
  brought to the candidate spelling on the merge of `main` into the branch:
  `set2-neg-arena-replace-target` (two delimited calls) and
  `set2-pos-box-descriptor-replace` (two comparisons);
  `stor5-neg-box-new-arena-content` needed no change. Verdicts and rule
  citations are unchanged.

## 5. Modified rules (complete replacement deltas, verbatim anchors)

Each site is one exact old text replaced by one exact new text; the anchors
are the bytes of v0.40 and the candidate respectively.

### header

Old:

```text
# Kernel Specification v0.40

Status: ACTIVE v0.40
```

New:

```text
# Kernel Specification v0.41

Status: CANDIDATE v0.41 supersedes v0.40 15ec2f6f475a7b70fb2654026ec3b6ef79afca3bd588fb38f22005d6637c0168
```

### header

Old:

```text
META-5 delta declaration: numbered rules +2/-9 (131 remain); grammar productions +8/-1 (82 remain); unique fixed lowercase grammar atoms +2/-4 (54 remain); writer operation spellings +0/-0; opaque system nominal spellings +0/-0; runtime-trap families +0/-1 (0 remain); entry forms +0/-1 (1 remains); contract block forms +0/-0; system operations and declaration records +0/-0 (203 remain); exception clauses +0/-0. The added rules are [INV-1] and [PRF-1]; the retired rule ids, which this version no longer defines and never reuses, are CLM-1, CLM-2, CLM-3, DIAG-3, PRV-1, PRV-2, PRV-3, SCOPE-4, and TRAP-1. The added productions are `affine_add_op`, `affine_expr`, `affine_factor`, `affine_term`, `for_binding`, `header_invariant`, `invariant_stmt`, and `proof_use`; the removed production is `claim_stmt`. The added atoms are `invariant` and `use`; the removed atoms are `because`, `claim`, `deny_claims`, and `traps`. This candidate replaces v0.39's runtime claim path with one source-carried proof surface: every writer-reachable partial operation is proved before execution, the claim trap and its exact runtime record are gone, and with them the `deny_claims` entry form. One proof-only `invariant` declaration serves both a loop header and an ordinary program point. A counted `for` header now encloses its binding followed by zero or more induction relations; an ordinary `loop` may enclose only induction relations; neither form admits a trailing comma, and a header relation carries no certificate block. A local invariant may carry ordered `use` steps with explicit proof-domain multipliers and named earlier invariants. Each use is independently proved in the same entering state, adds nothing to that state, and only the checked outer target is published. The automatic affine rule is complete for exactly the direct, every coefficient-one single-premise, every coefficient-one unordered premise-pair including self-pairs, and final closed-L0-image families; larger or specially weighted combinations are writer-directed. Exact operation and subscript value identities now enter later Goals only after their own nested domain obligations succeed, with occurrence-local evaluated-value identities covering values outside that admitted structural fragment. Value images, source facts, branch joins, pre-kill closure, counted exhaustion, function requirements and postconditions, and operation domains consume this one deterministic source ProofContext. Selected-target layout/address and parallel permission keep their own deterministic checker domains and consume the conclusions retained by source proof rather than repeating it or creating a second source-acceptance authority. Every admitted family runs to its specification-fixed completion without solver, heuristic stopping, elapsed-time verdict, cumulative work budget, runtime fallback, or compiler-generated proof replay.
```

New:

```text
META-5 delta declaration: numbered rules +0/-0 (131 remain); grammar productions +1/-0 (83 remain); unique fixed lowercase grammar atoms +0/-0 (54 remain); compound punctuation tokens +5/-0 (8 remain); token bytes +1/-0 (`!`, inside `!=` only); writer operation spellings +6/-6; opaque system nominal spellings +0/-0; runtime-trap families +0/-0 (0 remain); entry forms +0/-0 (1 remains); contract block forms +0/-0; system operations and declaration records +0/-0 (203 remain); exception clauses +0/-0. The added production is `compare_op`; the added compound tokens are `==`, `!=`, `<=`, `>=`, and `::`; the respelled operations are the six integer comparisons, `ieq` `ine` `ilt` `ile` `igt` `ige` becoming `==` `!=` `<` `<=` `>` `>=`, which thereby leave `DotlessOperationNames` and `ReservedLowerNames`. This candidate carries the second FLOOR-5 spelling batch. Integer comparison joins integer arithmetic as an `infix` expression: one `compare_op` over two atoms, integer-only exactly as the arithmetic symbols are, while float and tag-only enum comparison keep their prefixed names. Call-site type application is delimited by `::` — `cvt::<u8, u32>(w)`, `open_file::<'f, 'n>(...)` — so that `IDENT "<"` begins only a comparison and every grammar decision keeps its two-token bound; constructors and type position are unchanged. In proof position the four ordered symbols replace `ile`, `ilt`, `ige`, and `igt` as the relation of a header invariant, a local invariant, and a relation-form use step; a multiplied relation-form use step is parenthesized, `use 3 * (a <= b);`, and a bare one is not. No rule's semantics changes: every comparison origin, contract-clause root, invariant relation, and diagnostic attribution is keyed on the same operation identities under their new spellings, and the accepted-program set is unchanged up to respelling.
```

### header

Old:

```text
Selection ground: every writer-reachable partial operation must be proved before execution,
```

New:

```text
Selection ground: evidence-selected under the FLOOR-5 spelling rule (T1–T4 and its measured tiebreaks): the six integer comparisons are the most frequent operation class in the corpus, the positional comparison call was the last direction-sensitive positional form, v0.40's proof surface had already made the ordered comparisons relations over infix affine operands, and the `<` collision that cancelled the v0.23 comparison row is dissolved by the `::` delimiter without widening any parser decision beyond two tokens; the rulings and the rejected alternatives are recorded in `research/investigations/spelling-relief/SWEEP.md` and `governance/spec-evolution/comparison-symbols-v041-candidate.md`. Prior selection ground for the v0.40 proof surface remains: every writer-reachable partial operation must be proved before execution,
```

### FORM-2

Old:

```text
The left-attachment set contains `(`, `[`, `<`, `&`, `.`, and `..`.
The right-attachment set contains `)`, `]`, `>`, `,`, `;`, `.`, `:`, `(`, `<`, `[`, and `..`.
Between two consecutive terminals on the same line, emit zero bytes when the left terminal is in the left-attachment set or the right terminal is in the right-attachment set; otherwise emit exactly one ASCII space.
```

New:

```text
The left-attachment set contains `(`, `[`, `<`, `&`, `.`, `..`, and `::`.
The right-attachment set contains `)`, `]`, `>`, `,`, `;`, `.`, `:`, `(`, `<`, `[`, `..`, and `::`.
Between two consecutive terminals on the same line, emit zero bytes when the left terminal is in the left-attachment set or the right terminal is in the right-attachment set; otherwise emit exactly one ASCII space.
A `<` or `>` terminal selected by `compare_op` [GRAM-5] is rendered as a member of neither set, so a comparison is `a < b` while a type-argument list is `f::<T>(x)` and `buffer<u8>`; this stated spacing overrides the generic attachment of those two bytes exactly as the `for` header's stated space does below.
```

### FORM-2

Old:

```text
generic and square-bracket interiors are compact; `](` and `>(` are attached;
```

New:

```text
generic and square-bracket interiors are compact; `](`, `>(`, and `::<` are attached;
```

### FORM-2

Old:

```text
Examples include `Result<i32, Overflow>`, `f(x: a, y: b)`, `conform i32: Zeroed`, `['r, 's]`, and `[10_u8, 20_u8]`.
```

New:

```text
Examples include `Result<i32, Overflow>`, `f(x: a, y: b)`, `cvt::<u8, u32>(w)`, `a <= b`, `conform i32: Zeroed`, `['r, 's]`, and `[10_u8, 20_u8]`.
```

### GRAM-1

Old:

```text
- `->`, `=>`, and `..` are the three compound punctuation tokens.
Otherwise each byte in `(`, `)`, `{`, `}`, `[`, `]`, `<`, `>`, `,`, `:`, `;`, `.`, `=`, and `&` is one exact punctuation token.
```

New:

```text
- `->`, `=>`, `..`, `==`, `!=`, `<=`, `>=`, and `::` are the eight compound punctuation tokens; each is formed exactly when its two bytes are adjacent, by the same maximal rule that forms `=>` from `=` and `>`.
The byte `!` occurs in no other token: a `!` not immediately followed by `=` is a raw lexical defect.
Otherwise each byte in `(`, `)`, `{`, `}`, `[`, `]`, `<`, `>`, `,`, `:`, `;`, `.`, `=`, and `&` is one exact punctuation token.
```

### GRAM-1

Old:

```text
while `"->"`, `"=>"`, and `".."` each denote one compound punctuation token.
```

New:

```text
while `"->"`, `"=>"`, `".."`, `"=="`, `"!="`, `"<="`, `">="`, and `"::"` each denote one compound punctuation token.
```

### GRAM-1

Old:

```text
so the 1:1 production-to-node mapping is preserved by the factored recognition.
```

New:

```text
so the 1:1 production-to-node mapping is preserved by the factored recognition; its operator child is one `infix_op` or one `compare_op` node.
```

### GRAM-4

Old:

```text
header_invariant := "invariant" IDENT ":" IDENT "(" affine_expr "," affine_expr ")"
invariant_stmt := "invariant" IDENT ":" IDENT "(" affine_expr "," affine_expr ")"
                  (";" | "{" proof_use+ "}")
proof_use   := "use" ("[0-9]+" "*")?
               (IDENT | IDENT "(" affine_expr "," affine_expr ")") ";"
```

New:

```text
header_invariant := "invariant" IDENT ":" affine_expr compare_op affine_expr
invariant_stmt := "invariant" IDENT ":" affine_expr compare_op affine_expr
                  (";" | "{" proof_use+ "}")
proof_use   := "use" ( "[0-9]+" "*" (IDENT | "(" affine_expr compare_op affine_expr ")")
             | IDENT | affine_expr compare_op affine_expr ) ";"
```

### GRAM-5

Old:

```text
infix_tail     := infix_op atom
```

New:

```text
infix_tail     := (infix_op | compare_op) atom
```

### GRAM-5

Old:

```text
                | "%" | "%defined" | "%checked"
atom           := literal | "move" place | place | borrow_expr
call           := callee targs? "(" ( atom_list | fieldinit_list )? ")"
```

New:

```text
                | "%" | "%defined" | "%checked"
compare_op     := "==" | "!=" | "<" | "<=" | ">" | ">="
atom           := literal | "move" place | place | borrow_expr
call           := callee ("::" targs)? "(" ( atom_list | fieldinit_list )? ")"
```

### GRAM-6

Old:

```text
[GRAM-6] There is no general operator syntax and no precedence: an `infix` expression is exactly one operation over two atoms [GRAM-5, GRAM-9], composition is by `let`, and no precedence, associativity, or parenthesization surface exists.
```

New:

```text
[GRAM-6] There is no general operator syntax and no precedence: an `infix` expression is exactly one operation over two atoms [GRAM-5, GRAM-9], composition is by `let`, and no precedence, associativity, or parenthesization surface exists.
The `compare_op` alternatives are the six integer comparisons of [OP-1] and form `infix` expressions exactly as the `infix_op` arithmetic does; a `call` writes its type and region arguments after the `::` delimiter, `cvt::<u8, u32>(w)`, so that `IDENT "<"` begins a comparison and never a type-argument list, while a `construct` and a `type` write theirs bare.
```

### TYPE-4

Old:

```text
Representation change is the single explicit op `cvt<Src, Dst>(x)`.
```

New:

```text
Representation change is the single explicit op `cvt::<Src, Dst>(x)`.
```

### STOR-2

Old:

```text
`arena_new<'r, T>(v)` returns `own arena<'r, T>`
```

New:

```text
`arena_new::<'r, T>(v)` returns `own arena<'r, T>`
```

### OP-8

Old:

```text
(negative infinity is `fneg(finf<T>())`)
```

New:

```text
(negative infinity is `fneg(finf::<T>())`)
```

### OP-8

Old:

```text
[OP-9] `buffer_fits<T>(n)` is the pure, total, target-independent allocation-domain predicate
```

New:

```text
[OP-9] `buffer_fits::<T>(n)` is the pure, total, target-independent allocation-domain predicate
```

### OP-9

Old:

```text
`buffer_new(n, v)` over fill type T carries the one canonical obligation `buffer_fits<T>(n)`.
`buffer_vacant<T>(n)` carries `buffer_fits<Option<T>>(n)`.
```

New:

```text
`buffer_new(n, v)` over fill type T carries the one canonical obligation `buffer_fits::<T>(n)`.
`buffer_vacant::<T>(n)` carries `buffer_fits::<Option<T>>(n)`.
```

### EFF-2

Old:

```text
For example, `reserve_file<'r>(factory: &uniq 'r factory)` exhibits
```

New:

```text
For example, `reserve_file::<'r>(factory: &uniq 'r factory)` exhibits
```

### SYS-2

Old:

```text
A call whose callee resolves to a system operation writes its region arguments as `targs` in declared region-parameter order
```

New:

```text
A call whose callee resolves to a system operation writes its region arguments as `targs` after the `::` delimiter [GRAM-5] in declared region-parameter order
```

### ENT-3.S5

Old:

```text
for `let y = cvt<Src, Dst>(p);` with (Src, Dst) a total pair [OP-6]
```

New:

```text
for `let y = cvt::<Src, Dst>(p);` with (Src, Dst) a total pair [OP-6]
```

### ENT-3.S6

Old:

```text
`let b = buffer_new(n, v);` and `let b = buffer_vacant<T>(n);` each establish
```

New:

```text
`let b = buffer_new(n, v);` and `let b = buffer_vacant::<T>(n);` each establish
```

### ENT-6

Old:

```text
AllocationFit attaches one canonical `buffer_fits<T>(n)` Goal to every `buffer_new(n, v)`, and `buffer_fits<Option<T>>(n)` to every `buffer_vacant<T>(n)`, at that `call` node [OP-9].
```

New:

```text
AllocationFit attaches one canonical `buffer_fits::<T>(n)` Goal to every `buffer_new(n, v)`, and `buffer_fits::<Option<T>>(n)` to every `buffer_vacant::<T>(n)`, at that `call` node [OP-9].
```

### OP-1

Old:

```text
| `ieq` `ine` `ilt` `ile` `igt` `ige` | all int T | `(T, T) -> own Bool` | pure |
```

New:

```text
| `==` `!=` `<` `<=` `>` `>=` | all int T | `(T, T) -> own Bool` | pure |
```

### OP-1

Old:

```text
An `infix_op` token resolves to its exactly spelled operation by the operator table row; infix resolution consults no name domain, and an operator token is never a declaration, callee IDENT, or OPNAME.
```

New:

```text
An `infix_op` or `compare_op` token resolves to its exactly spelled operation by the operator table row; infix resolution consults no name domain, and an operator token is never a declaration, callee IDENT, or OPNAME.
```

### OP-2

Old:

```text
For `ieq`, `ine`, `ilt`, `ile`, `igt`, and `ige`, both operands denote their mathematical values in T
```

New:

```text
For `==`, `!=`, `<`, `<=`, `>`, and `>=`, both operands denote their mathematical values in T
```

### OP-7

Old:

```text
whether or not a cross-domain twin exists; the structural ops (`cvt`, `reinterpret`, `len`, `slice_of`, `box_new`, `arena_new`) carry no prefix.
```

New:

```text
whether or not a cross-domain twin exists; the structural ops (`cvt`, `reinterpret`, `len`, `slice_of`, `box_new`, `arena_new`) carry no prefix.
The integer arithmetic and integer comparison symbols of [GRAM-5] are the one prefix-free operation class: each is an integer-only table row, so `+` and `<` never denote a float or enum operation, and `fadd.strict`, `feq`, and `eeq` keep their prefixed names.
```

### OP-7

Old:

```text
is the same discipline as the `ilt` = `slt`/`ult` row, not overloading.
```

New:

```text
is the same discipline as the `<` = `slt`/`ult` row, not overloading.
```

### FN-9

Old:

```text
the clause expression must have exact type `own Bool` and its root must be exactly one of `ieq`, `ine`, `ilt`, `ile`, `igt`, or `ige`.
```

New:

```text
the clause expression must have exact type `own Bool` and its root must be exactly one `compare_op` — `==`, `!=`, `<`, `<=`, `>`, or `>=` [GRAM-5].
```

### DIAG-1

Old:

```text
If the boundary token is one member of four consecutive actual tokens `IDENT "." IDENT ("("|"<")`, that dotted call-or-targs spelling cites [FORM-3].
```

New:

```text
If the boundary token is one member of four consecutive actual tokens `IDENT "." IDENT ("("|"::")`, that dotted call-or-targs spelling cites [FORM-3].
```

### DIAG-1

Old:

```text
and the two actual tokens at the start of that occurrence are `(IDENT, "(")`, `(IDENT, "<")`, `(OPNAME, "(")`, `(OPNAME, "<")`, `(TYPEID, "(")`, or `(TYPEID, "<")`, the rejection cites [GRAM-9]; in an infix-operand occurrence, a two-token start whose second token is an operator token — the forbidden nested-infix start — likewise cites [GRAM-9].
```

New:

```text
and the two actual tokens at the start of that occurrence are `(IDENT, "(")`, `(IDENT, "::")`, `(OPNAME, "(")`, `(OPNAME, "::")`, `(TYPEID, "(")`, or `(TYPEID, "<")`, the rejection cites [GRAM-9]; in an infix-operand occurrence, a two-token start whose second token is an `infix_op` or `compare_op` token — the forbidden nested-infix start — likewise cites [GRAM-9].
```

### ENT-3

Old:

```text
An expression has comparison origin R when (a) it is a call to one of `ieq`, `ine`, `ilt`, `ile`, `igt`, `ige` [OP-2] whose two operands are each a term or constant, R the corresponding relation over them;
```

New:

```text
An expression has comparison origin R when (a) it is an `infix` expression whose operator is a `compare_op` — `==`, `!=`, `<`, `<=`, `>`, `>=` [OP-2] — and whose two operands are each a term or constant, R the corresponding relation over them;
```

### ENT-3.S4

Old:

```text
When and only when G's complete root is one comparison call admitted by comparison-origin shape (a),
```

New:

```text
When and only when G's complete root is one comparison admitted by comparison-origin shape (a),
```

### ENT-6

Old:

```text
when the child root is `ile`, `ilt`, `ige`, or `igt` over values having current affine images,
```

New:

```text
when the child root is `<=`, `<`, `>=`, or `>` over values having current affine images,
```

### ENT-6

Old:

```text
ordinal zero is `ile(start, end)`; ordinal one is `ile(end, len(buffer))`.
```

New:

```text
ordinal zero is `start <= end`; ordinal one is `end <= len(buffer)`.
```

### INV-1

Old:

```text
The relation IDENT in a `header_invariant`, `invariant_stmt`, or relation-form `proof_use` must be exactly `ile`, `ilt`, `ige`, or `igt`; it selects a proof-domain relation and performs no [OP-1] call.
The checker normalizes `ile(a,b)` to `a-b <= 0`, `ilt(a,b)` to `a-b <= -1`, `ige(a,b)` to `b-a <= 0`, and `igt(a,b)` to `b-a <= -1`.
```

New:

```text
The `compare_op` of a `header_invariant`, an `invariant_stmt`, or a relation-form `proof_use` must be exactly `<=`, `<`, `>=`, or `>`; it selects a proof-domain relation over its two affine expressions and performs no [OP-1] operation, and `==` or `!=` in that position is a hard error citing INV-1 at the `compare_op` node.
The checker normalizes `a <= b` to `a-b <= 0`, `a < b` to `a-b <= -1`, `a >= b` to `b-a <= 0`, and `a > b` to `b-a <= -1`.
```

### PRF-1

Old:

```text
invariant next_per_byte: ile(sum, 255_u32 * (i + 1_u64));
```

New:

```text
invariant next_per_byte: sum <= 255_u32 * (i + 1_u64);
```

### PRF-1

Old:

```text
invariant component_sum: ile(first + second + third, first_limit + second_limit + third_limit) {
  use ile(first, first_limit);
  use ile(second, second_limit);
  use ile(third, third_limit);
}

invariant pair_bound: ile(first + second, first_limit + second_limit);
invariant scaled_bound: ile(3_u64 * first + 3_u64 * second, 3_u64 * first_limit + 3_u64 * second_limit) {
  use 3 * pair_bound;
}
```

New:

```text
invariant component_sum: first + second + third <= first_limit + second_limit + third_limit {
  use first <= first_limit;
  use second <= second_limit;
  use third <= third_limit;
}

invariant pair_bound: first + second <= first_limit + second_limit;
invariant scaled_bound: 3_u64 * first + 3_u64 * second <= 3_u64 * first_limit + 3_u64 * second_limit {
  use 3 * pair_bound;
}
```

### PRF-1

Old:

```text
The optional bare-decimal multiplier in `proof_use` is a proof-domain positive integer factor.
```

New:

```text
The optional bare-decimal multiplier in `proof_use` is a proof-domain positive integer factor.
A multiplied relation-form source writes its relation in parentheses, `use 3 * (a <= b);`, an unmultiplied relation-form source is bare, `use a <= b;`, and a named source is never parenthesized [GRAM-4]; those parentheses delimit the premise the factor scales and are the grammar's own, not an affine grouping.
```

### EX-1

Old:

```text
  if ilt(x, 0_i32) {
    return Neg();
  } else if ieq(x, 0_i32) {
```

New:

```text
  if x < 0_i32 {
    return Neg();
  } else if x == 0_i32 {
```

### EX-1

Old:

```text
    let expected = ieq(v, 42_i32);
```

New:

```text
    let expected = v == 42_i32;
```
