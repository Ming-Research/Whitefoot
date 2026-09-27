# Kernel Specification v0.74

Rule IDs are stable; diagnostics cite rule IDs.

## 1. Scope and conformance

[SCOPE-1] This document defines the complete writer-facing kernel. Values and functions have only the ordinary declaration, type, ownership, effect, and contract judgments defined here. Implementation language and linkage add no source-language category or permission.

[SCOPE-2] A program is checker-accepted iff it parses under the canonical grammar and satisfies every machine judgment in this document.
Every proof-required partial operation is statically discharged by the deterministic checker before lowering; a writer may expose a missing fact with executed control flow, provide a machine-proved loop-header or local invariant [INV-1], direct a larger local linear derivation with [PRF-1], or publish it across a function boundary through verified contracts [FN-8, FN-9].
Failure to discharge any such obligation rejects compilation; no operation receives an implicit runtime fallback and no writer statement can request one.
There is no writer-emittable unchecked state and nothing writer-stated is trusted without machine derivation.
Runtime-origin values — parameters, function results, loaded storage, and values derived from them — enter the proof context only as typed symbolic terms; their origin neither grants nor removes proof authority.
A proposition enters the context only from a selected ordinary control-flow edge, a declaration or type fact fixed by this specification, a verified callee postcondition, or the target of a machine-proved header or local invariant whose optional [PRF-1] certificate the checker has independently discharged.
No written conclusion, origin annotation, trusted-value mark, runtime observation outside executed source control, compiler-generated record, or optimizer result is a fact source.

[SCOPE-3] Accepted programs have no undefined behavior, conditional on the declared trusted computing base: compiler, checker, linked function definitions, runtime, allocator, and OS.
Every linked definition must implement its ordinary declaration with the same value, ownership, effect, contract, and call-lifetime meaning as a Whitefoot body. Its implementation language and build binding do not alter source acceptance or permissions.
The source outcome model leaves resource availability outside it: heap exhaustion, stack exhaustion, operating-system quotas, and runtime-start resources may stop execution without a Whitefoot value, status, or cleanup guarantee.
That placement does not defer static layout, stride, allocation-ceiling, address, target-domain, or parallel-independence proof; each obligation still succeeds before the governed operation is emitted.
Resource failure establishes no source fact, grants no fallback path, and cannot turn an unproved operation into an accepted one.

## 2. Canonical form

[FORM-1] There is exactly one spelling per semantic construct and one legal byte-level formatting.
Non-canonical input is a hard error; the toolchain never auto-formats.
Unknown constructs are hard errors (conservative extension).

[FORM-2] Each source file is UTF-8.
Once every source has passed raw lexical formation and the complete compilation unit has one derivation, each source owns one ordered derivation forest: exactly the top-level `item` subtrees under the single compilation-unit `program` root whose terminals belong to that source, in source-local item order.
A source forest is not a second `program` node, and a source with no items owns an empty forest.
That source's canonical bytes are exactly the result of rendering its forest by the following rules.
The input bytes must equal that rendering byte for byte; the toolchain does not normalize or rewrite input.
A source that has no complete `item*` derivation is rejected by its owning lexical or grammar rule before this forest-format comparison, and no tree or forest is fabricated [DIAG-1].
A module graph record [MOD-1] is rendered by the same rules over its forest of `module_row` and `entry_decl` subtrees under its `graph_file` root.

Outside terminal interiors, lines end only with LF and formatting bytes are only ASCII space and LF.
There is no CR, tab, trailing horizontal whitespace, leading blank line, or blank line inside a top-level item.
A nonempty source has exactly one empty line between consecutive top-level `item` nodes and no trailing blank line; its final nonempty line ends with exactly one LF.
The one exception is a run of consecutive `alias_decl` items, and a run of consecutive `module_row`s of a graph record, which stand on consecutive lines with no empty line between them [MOD-1, MOD-4].
A source containing zero items is exactly one LF.
Terminal interiors retain their exact bytes and are checked by their owning FORM rule.

The left-attachment set contains `(`, `[`, `<`, `&`, `.`, `..`, and `::`.
The right-attachment set contains `)`, `]`, `>`, `,`, `;`, `.`, `:`, `(`, `<`, `[`, `..`, and `::`.
Between two consecutive terminals on the same line, emit zero bytes when the left terminal is in the left-attachment set or the right terminal is in the right-attachment set; otherwise emit exactly one ASCII space.
A `<` or `>` terminal selected by `compare_op` [GRAM-5] is rendered as a member of neither set, so a comparison is `a < b` while a type-argument list is `f::<T>(x)`; this stated spacing overrides the generic attachment of those two bytes exactly as the `for` header's stated space does below.
Thus function headers are `fn f()` and `fn f<T>()`; subscripts are `p[i]`; a counted range is `lower..upper`; generic and square-bracket interiors are compact; `](`, `>(`, and `::<` are attached; and commas and colons attach to their left operand and have one space before the grammar-required following element.
The colon separating a `binding_decl` name from its interface application is rendered as a member of neither attachment set: `binding SeedKey : Key<u64, Seed>`. This declaration separator is distinct from parameter, field, and bound colons, which retain the rule above.
Examples include `Result<i32, Overflow>`, `f(x: a, y: b)`, `cvt::<u8, u32>(w)`, `a <= b`, `binding SeedKey : Key<u64, Seed>`, and `[10_u8, 20_u8]`.
The range step renders compactly: `&r[lo..hi]`, `part[k]`, because `[`, `..`, and `]` are all attachment members.
A payload step renders compactly: `n.left.Some.value`.
A measure or window part renders as an ordinary field suffix: `r.len`, `r.next`, and `deref(p).len`.
`&` attaches left, so a reference expression is `&p`, `&deref(p).f`, `&r[i]`, or `&r[lo..hi]`, and a reference parameter mode is `&u8` or `&Slots<Int, N>`.
The range-reference parameter kind renders compactly: `part: &[Int]`.
`move deref(b)` renders with exactly one space after `move`, as `move place` already does.

Every nonempty physical line begins with exactly two ASCII spaces for each enclosing brace block.
A closing brace is rendered after reducing the depth for the block it closes.
A match-arm header is therefore one level inside its match, and statements in the arm body are two levels inside it.

The line-bearing simple productions are `field`, `variant`, `fn_bind`, `const_decl`, `heap_decl`, `alias_decl`, `module_row`, `doc`, `contract_define`, `requires_clause`, `ensures_clause`, `set_stmt`, `expr_stmt`, `return_stmt`, `proof_use`, `break_stmt`, and `give_stmt`, plus a `let_stmt` whose selected right-hand side is `ordinary_let_rhs` or `propagate_let_rhs` and a `let_stmt` whose selected binder is a parenthesized binder list or a destructuring consume [GRAM-4].
Each renders completely on one line, including its final semicolon.
A `fn_sig` renders its signature inline, with a result-list space after `->` just as a `fn_decl` does. Its optional `contract_block` uses the ordinary block layout. In an interface body each member starts a new line and the following semicolon attaches to the signature or its contract's closing brace. In a `gparam` the signature stays in the surrounding generic header; no member semicolon is inserted.

The generically block-bearing productions are `struct_decl`, `enum_decl`, `interface_decl`, `binding_decl`, the body of `fn_decl`, `contract_block`, `match_stmt`, `value_match`, `if_stmt`, `value_if`, and `arm`.
Their introducer through `{` is one line; their children render on following lines at depth plus one; and `}` renders on its own line at the original depth.
Empty blocks still use an opening line followed by a closing-brace line.
An `invariant_stmt` ending in `;` renders completely on one line, and so does an `entry_decl` ending in `;`.
An `entry_decl` carrying a requirement block renders its introducer through `{` on one line, `no_heap;` on a following line at depth plus one, and `}` on its own line at the original depth.
A `module_row` renders exactly one space between its `:` and `[`, overriding the generic right attachment of `[` as the `for` header's stated space overrides that of `(`, so the canonical spelling is `pkg::runtime: [pkg::data, pkg::queue];`.
An `invariant_stmt` carrying a proof block renders its introducer through `{` on one line, each `proof_use` on a following line at depth plus one, and `}` on its own line at the original depth.

A `for_stmt` renders `for`, its optional label, exactly one space, and `(`; this stated space overrides the generic right attachment of `(`.
A `proof_use` whose `use_premise` is a delimited relation renders exactly one space before that premise's `(`, `use (a <= b);` and `use 3 times (a <= b);`; this stated space likewise overrides the generic right attachment of `(`, exactly as the `for_stmt` space above does, while the relation's own affine parentheses keep the generic attachment.
A `fn_decl` result list renders exactly one space between `->` and its `(`, and a destructuring `let_stmt` exactly one space between `let` and its `(`; each of these two stated spaces overrides the generic right attachment of `(` exactly as the `for` header's does, so the canonical spellings are `-> (kept: u64, spare: u64)` and `let (kept, spare) = split(taken: move run);` [GRAM-2, GRAM-4].
A destructuring consume's rest marker renders exactly one space between its preceding `,` and `..`, overriding the generic right attachment of `..` exactly as the `for` header's stated space overrides that of `(`, so the canonical spellings are `let Conn(f: fh, ..) = move c;` and, with no bound field, `let Conn(..) = move c;` [GRAM-4].
A `for_stmt` with no `header_invariant` renders its whole header, from `for` through `) {`, on one line; a counted loop with no invariant therefore has the one-line header `for (i in 0_u64..count) {`.
A `for_stmt` with at least one `header_invariant` breaks after `(` instead: its `for_binding` and every `header_invariant` each render on a separate following line at depth plus one, with a comma after every item except the last; and `) {` renders on one line at the original depth.
An ordinary `loop_stmt` without a parenthesized invariant header keeps the one-line introducer `loop` plus optional label through `{`.
With a header it instead renders `loop`, its optional label, exactly one space, and `(` on one line, again overriding generic right attachment; every `header_invariant` renders on a separate following line at depth plus one, with a comma after every item except the last; and `) {` renders on one line at the original depth.
In either loop form, body children and the final closing brace retain the ordinary block-bearing rendering.
An `if_stmt` or `value_if` is rendered solely by this sentence, the generic block-bearing rendering notwithstanding: its introducer through the then-block `{` is one line; then-children render at depth plus one; an `else` renders as the join line `} else {` at the original depth, and a chained `else if` as the join line `} else if` through that `if`'s `{` at the original depth, never as a nested introducer line; else-children render at depth plus one; and the final `}` renders on its own line at the original depth.
No one-line `if` form exists.
A value-match or value-if let places its complete let prefix and the `match` or `if` introducer through `{` on one line.

A function without a `contract_block` puts its complete header through the body `{` on one line.
An interface `fn_decl` without a body [MOD-7] renders its complete header and its final `;`, or its `doc` entry, on one line; with a `contract_block`, that `;` or `doc` entry follows the block's closing `}` on its line, as `} doc "Appends one job.";`.
A function with a `contract_block` puts its header through `contract {` on one line.
After that block, render its close and the body open as the single line `} {`.
Then render the body children and closing brace.
Every production not listed as line-bearing or block-bearing introduces no formatting boundary of its own.
Its terminals stay on the current line unless a descendant line-bearing or block-bearing production introduces one of the boundaries prescribed above.
No other LF or blank line is emitted.

[FORM-3] Lexical classes: IDENT `[a-z][a-z0-9_]*` excluding every lowercase token spelling produced by exact fixed grammar atoms in the complete grammar; TYPEID `[A-Z][A-Za-z0-9]*`; LABEL `@[a-z][a-z0-9_]*`; OPNAME `[a-z][a-z0-9_]*\.(wrap|defined|checked|sat|strict)` (single token; the base has the raw lowercase-word shape used by IDENT and the mode suffix is a closed word set, so an OPNAME can never maximal-munch a valid field-access place `p.field`: all five suffix words are reserved from field binding [OP-1, GRAM-5]; e.g. `ineg.checked`).
`program` and `no_heap` are exact fixed grammar atoms of `heap_decl` [GRAM-2], so both spellings leave IDENT by the exclusion above.

[FORM-4] There are no comments.
Documentation is the `doc` field of declarations [GRAM-2].
Source coordinates and diagnostic derivations live in toolchain records.

[FORM-5] Literals, exhaustively: integers `-?[0-9]+_TYPE` (decimal only, mandatory suffix; a leading `-` is legal for signed TYPE, and the signed value must lie in TYPE's range [FORM-7]; e.g. `42_i32`, `-2147483648_i32`); finite floats use the grammar `-?(0|[1-9][0-9]*)\.[0-9]+(e-?(0|[1-9][0-9]*))?_TYPE`, where TYPE is `f32` (IEEE 754 binary32) or `f64` (IEEE 754 binary64), positive exponents carry no sign, negative exponents carry one `-`, and only the integer and exponent components have the stated no-leading-zero form.
Let C be the nonnegative integer formed by concatenating the integer and fraction digits, let F be the number of fraction digits, and let E be the signed integer formed by the exponent digits and their optional `-`; when the exponent is absent E is zero, and `e-0` also gives E zero.
A matching decimal whose C is zero denotes signed decimal zero: a leading literal `-` selects negative zero and its absence selects positive zero, independently of E.
Every other matching decimal denotes the exact nonzero rational whose magnitude is C × 10^(E − F), with the leading literal sign applied.
For one finite bit pattern of TYPE, consider every matching decimal that rounds from that signed zero or exact nonzero rational to the bit pattern under IEEE 754 round-to-nearest, ties-to-even.
Its canonical spelling is the candidate with the fewest ASCII bytes before `_TYPE`; a tie is resolved by lexicographically least unsigned ASCII bytes.
This selection is total, host-independent, and unique; in particular `0.0` and `-0.0` remain distinct.
Other examples are `1.5_f64` and `6.022e23_f64`.
`unit`; STRING `"..."` whose interior is a sequence of items, each one raw ASCII-printable byte in U+0020..U+007E other than `"` and `\`, or one of exactly three escapes `\\ \" \n`; no other byte is legal, and each character has exactly one spelling (the escape where one is defined, the raw byte otherwise).
STRING appears only in `doc` entries.
There are no boolean literals: `Bool` is a prelude enum (§14).
Generic-numeric literals `0_T` and `1_T` are legal where `T` is a gparam bound by a numeric contract (`Int` or `Float`, §14), denoting T's additive and multiplicative identity; a concrete type uses `0_i32` and the like, so there is no dual spelling.
NaN and the infinities are not literals; they are the nullary ops `fnan` and `finf` [OP-1].

[FORM-6] The token `unit` names the unit type in type position and the unit value in expression position; the grammar positions are disjoint productions, so resolution is production-local, not contextual.
The lowercase spelling follows the primitive-type convention (TYPE-1: primitives are lowercase keywords, not TYPEIDs); the single-token value spelling follows the one-spelling convention [FORM-1] for the type's sole inhabitant.

[FORM-7] Numeric-literal well-formedness.
An integer literal `-?d_T` is legal where its signed value lies in the closed range of T (signed `[-2^(K-1), 2^(K-1)-1]`, unsigned `[0, 2^K-1]`) and it has no leading zeros: the single digit `0` is its own form, a leading `-` is legal for signed T, and `-0` is written `0`.
A float literal is legal only when it has the unique canonical spelling selected by [FORM-5] and denotes a finite value of its stated TYPE.
An out-of-range integer, a leading-zero integer, a noncanonical float spelling, or a float decimal that rounds to a non-finite value is a hard error at check time [SCOPE-2]; a literal never denotes a wrapped, truncated, saturated, or undefined value.

## 3. Grammar

[GRAM-1] The grammar is deterministic and unambiguous.
Raw lexical formation scans each source independently from byte offset zero and partitions it into tokens and trivia without normalization, decoding a value, or consulting grammar position, name lookup, the operation table, or another source.
At each cursor it takes exactly the following maximal form; no token or trivia crosses a source boundary.

- One or more ASCII space bytes form one trivia item.
One LF byte forms one trivia item.
- A lower word starts with `[a-z]` and continues through the maximal `[a-z0-9_]*` suffix.
If that complete base is followed immediately by `.` and exactly one of `wrap`, `defined`, `checked`, `sat`, or `strict`, and the suffix is not followed by an ASCII letter, ASCII digit, or `_`, the base, dot, and suffix instead form one operation-name token.
Otherwise the lower word ends before the dot.
- An upper word starts with `[A-Z]` and continues through the maximal `[A-Za-z0-9]*` suffix.
- A label form starts with `@`; the sigil must be followed by `[a-z]`, after which the token continues through the maximal `[a-z0-9_]*` suffix.
- A numeric form starts with a decimal digit, or with `-` immediately followed by a decimal digit.
It then consumes the maximal sequence of ASCII letters, ASCII digits, `_`, and `.`, plus a `+` or `-` only when that sign byte immediately follows `e` or `E`, except that when the next two bytes are `..` the numeric form ends immediately before the first dot.
A single dot and every other numeric candidate retain the preceding maximal rule unchanged.
Raw formation deliberately retains broad candidates such as `1e+`, `1.00_f64`, and `1.0E2_f64`; [FORM-5] and [FORM-7] decide membership and canonicality without rescanning or splitting them.
- An operator form starts with `+`, `*`, `/`, or `%`, or with a `-` that is immediately followed by neither a decimal digit (numeric form, unchanged) nor `>` (the `->` compound, unchanged), and continues through the maximal `[a-z]*` suffix; the suffix must be empty or one of `wrap`, `defined`, `checked`, `sat` per the closed `infix_op` list, and any other suffix is a terminal-membership rejection.
- A STRING form starts with `"` and ends at the first unescaped `"`.
Its interior consists only of raw bytes `0x20` through `0x7e` other than `"` and `\`, or the two-byte escapes `\\`, `\"`, and `\n`.
An escape consumes its backslash and follower together.
- `->`, `=>`, `..`, `==`, `!=`, `<=`, `>=`, and `::` are the eight compound punctuation tokens; each is formed exactly when its two bytes are adjacent, by the same maximal rule that forms `=>` from `=` and `>`.
The byte `!` occurs in no other token: a `!` not immediately followed by `=` is a raw lexical defect.
Otherwise each byte in `(`, `)`, `{`, `}`, `[`, `]`, `<`, `>`, `,`, `:`, `;`, `.`, `=`, and `&` is one exact punctuation token.

In source EBNF, each quoted fixed atom denotes the unique sequence of raw formed tokens whose concatenated bytes equal that atom.
In particular, `"->"`, `"=>"`, `".."`, `"=="`, `"!="`, `"<="`, `">="`, and `"::"` each denote one compound punctuation token, and `"&"` denotes the one exact punctuation token `&`, which carries no mode, permission, or other follower word.
The quoted `"[0-9]+"` occurrences in the `const` production and the optional multiplicity position of `proof_use` share the grammar's sole pattern predicate: each denotes one numeric-form token whose complete bytes match `[0-9]+`, and neither is a fixed atom.
`SELECT_2` and the two-token parser bound count the expanded raw formed tokens, not quoted-atom occurrences.
An external terminal denotes one predicate over one formed token.

Anything that cannot take one of those forms is a raw lexical defect with the attribution and exact span in [DIAG-1].
Raw formation gives every token exactly one context-free shape kind: lower word, upper word, label form, operation-name form, operator form, numeric form, STRING form, or one exact punctuation form.
Terminal membership then visits every formed token in source-ordinal and token order.
For each token independently, and without consulting grammar position, name lookup, the operation table, or another token, it evaluates the complete set of exact fixed-terminal predicates and external-terminal predicates in this specification and retains every matching predicate.
It rejects the token exactly when that retained set is empty; it never selects one preferred predicate and never tests only the predicates expected at a parser position.
Grammar derivation later tests the retained predicate sets against its `SELECT_2` rows.

A grammar terminal is therefore a predicate over a token's shape kind and exact bytes, not a priority-selected replacement token kind.
Exact-spelling and union predicates may overlap only when they do not compete at one grammar decision; every choice, optional, and repetition decision has pairwise-disjoint strong-LL(2) `SELECT_2` languages, so a parser selects exactly one arm with at most two tokens.
In particular, a noncompeting overlap such as fixed `unit` with the `literal` union does not create an ambiguous parse, but no decision may use predicate priority to hide an overlap.
A `psuffix` decision reads at most two tokens: a `.` whose next token is an IDENT begins a field step, a `.` whose next token is a TYPEID begins an enum payload step, and a `[` begins an index or range step [GRAM-5].
Every production maps 1:1 to one source-tree node kind.
The only abbreviation expansion is FN-3's hygienic expansion of interface and binding groups before semantic instantiation and IR; every expanded declaration and use retains its written source node and member position.
`infix_tail` maps to the `infix` node kind: a selected tail forms one `infix` node spanning the complete `expr` — the atom and the tail — so the 1:1 production-to-node mapping is preserved by the factored recognition; its operator child is one `infix_op` or one `compare_op` node.

[GRAM-2] Items:

```wf-ebnf GRAM-2
program      := item*
item         := alias_decl
              | "public" ( fn_decl | struct_decl | enum_decl | interface_decl | binding_decl
              | const_decl )
              | fn_decl | struct_decl | enum_decl | interface_decl | binding_decl | const_decl
              | heap_decl
alias_decl   := "alias" (IDENT | TYPEID) "=" ("pkg" | "std") ("::" (IDENT | TYPEID))* ";"
heap_decl    := "program" "no_heap" ";"
struct_decl  := "opaque"? ("nocopy" | "nodrop")? "struct" TYPEID generics? "{" doc? field* "}"
field        := "public"? "readonly"? IDENT ":" type ";"
enum_decl    := ("nocopy" | "nodrop")? "enum" TYPEID generics? "{" doc? variant* "}"
variant      := TYPEID "(" vfield_list? ")" ";"
vfield_list  := vfield ("," vfield)*
vfield       := "public"? IDENT ":" type
fn_decl      := "fn" IDENT generics? "(" param_list? ")"
                "->" ( result_binding | "(" result_binding ("," result_binding)+ ")" )
                effects contract_block? ( ";" | doc | "{" doc? stmt* "}" )
result_binding:= IDENT ":" rtype
contract_block:= "contract" "{" contract_define* requires_clause* ensures_clause* "}"
contract_define:= "define" IDENT "=" expr ";"
requires_clause:= "requires" clause_expr ";"
ensures_clause:= "ensures" ("when" result_route ":")? clause_expr ";"
result_route:= (IDENT "is")? TYPEID "(" fieldbind ")"
interface_decl  := "interface" TYPEID generics? "{" doc? (fn_sig ";")* "}"
binding_decl  := "binding" TYPEID ":" (pack_use | type_path targs?) "{" doc? fn_bind* "}"
fn_sig       := "fn" IDENT "(" param_list? ")"
                "->" (result_binding | "(" result_binding ("," result_binding)+ ")")
                effects contract_block?
pack_use     := TYPEID targs?
function_arg := "fn" callee ("::" targs)?
const_decl   := "const" IDENT ":" type "=" cvalue ";"
fn_bind      := IDENT "=" callee ("::" targs)? ";"
doc          := "doc" STRING ";"
generics     := "<" gparam ("," gparam)* ">"
gparam       := TYPEID (":" (TYPEID | capability_bound))?
              | "const" IDENT ":" type | fn_sig | "interface" (pack_use | type_path targs?)
capability_bound:= "copy" | "drop"
param_list   := param ("," param)*
param        := IDENT ":" (type | "&" (type | "[" type "]"))
graph_file   := module_row+ entry_decl*
module_row   := module_path ":" "[" (module_path ("," module_path)*)? "]" ";"
module_path  := ("pkg" | "std") ("::" IDENT)*
entry_decl   := "entry" IDENT "=" module_path (";" | "{" "no_heap" ";" "}")
```

A `heap_decl` is admitted at most once in a source bundle and only as the first `item` of its first source record [PROG-2]; a second `heap_decl`, or one at any later item position, is a hard error citing GRAM-2 at that `heap_decl` node, and a module program admits none [MOD-9].
Its two fixed atoms compete with no other `item` arm, so the decision is strong-LL(2) on the first token alone [GRAM-1].
What the declaration means is [STOR-8]'s.

[GRAM-3] Types and modes:

```wf-ebnf GRAM-3
type   := "i8"|"i16"|"i32"|"i64"|"u8"|"u16"|"u32"|"u64"|"f32"|"f64"|"unit"
        | TYPEID targs? | type_path targs?
type_path := ("pkg" | "std" | IDENT) "::" (IDENT "::")* TYPEID
rtype  := type
targs  := "<" targ ("," targ)* ">"
targ   := type | const | function_arg
```

A parameter written `name: T` has value mode; `name: &T` and `name: &[T]` have the reference and range-reference kinds respectively [REF-1, REF-4].
Results follow [FN-1]'s value-only rule. In these rules, `own T` denotes a semantic mode/type pair, not a source annotation; `own` is an ordinary IDENT under [FORM-3].

[GRAM-4] Statements:

```wf-ebnf GRAM-4
stmt        := let_stmt | set_stmt | expr_stmt | return_stmt | loop_stmt
             | for_stmt | invariant_stmt | break_stmt
             | if_stmt | match_stmt | give_stmt
let_stmt    := "let" ( IDENT "="
               ( ordinary_let_rhs | propagate_let_rhs
               | value_match | value_if )
               | "(" IDENT ("," IDENT)+ ")" "=" call ";"
               | (TYPEID | type_path) "(" ( fieldbind_list ("," "..")? | ".." )? ")" "=" "move" place ";" )
if_stmt     := "if" expr "{" stmt* "}" ("else" (if_stmt | "{" stmt* "}"))?
value_if    := "if" expr "{" stmt* "}" "else" (value_if | "{" stmt* "}")
ordinary_let_rhs:= expr ";"
propagate_let_rhs := "propagate" expr ";"
set_stmt    := "set" place "=" expr ";"
expr_stmt   := call ";"
return_stmt := "return" expr ("," expr)* ";"
loop_stmt   := "loop" LABEL? ("(" header_invariant ("," header_invariant)* ")")?
               "{" stmt* "}"
for_stmt    := "for" LABEL? "(" for_binding ("," header_invariant)* ")"
               "{" stmt* "}"
for_binding := IDENT "in" atom ".." atom
header_invariant := "invariant" IDENT ":" affine_expr compare_op affine_expr
invariant_stmt := "invariant" IDENT ":" affine_expr compare_op affine_expr
                  (";" | "{" proof_use+ "}")
proof_use   := "use" (("[0-9]+" | IDENT) "times")? use_premise ";"
use_premise := IDENT | "(" affine_expr compare_op affine_expr ")"
affine_expr := affine_term (affine_add_op affine_term)*
affine_term := affine_factor ("*" affine_factor)?
affine_factor := atom | call | "(" affine_expr ")"
affine_add_op := "+" | "-"
break_stmt  := "break" LABEL? ";"
give_stmt   := "give" expr ";"
match_stmt  := "match" expr "{" arm+ "}"
value_match := "match" expr "{" arm+ "}"
arm            := TYPEID "(" ( fieldbind_list ("," "..")? | ".." )? ")" "=>" "{" stmt* "}"
fieldbind_list := fieldbind ("," fieldbind)*
fieldbind      := IDENT ":" IDENT
```

[GRAM-5] Expressions and places:

```wf-ebnf GRAM-5
expr           := atom infix_tail? | call
infix_tail     := (infix_op | compare_op) atom
infix_op       := "+" | "+wrap" | "+defined" | "+checked" | "+sat"
                | "-" | "-wrap" | "-defined" | "-checked" | "-sat"
                | "*" | "*wrap" | "*defined" | "*checked" | "*sat"
                | "/" | "/defined" | "/checked"
                | "%" | "%defined" | "%checked"
compare_op     := "==" | "!=" | "<" | "<=" | ">" | ">="
atom           := literal | "move" place | place | borrow_expr
call           := "musttail"? callee ("::" targs)? "(" ( atom_list | fieldinit_list )? ")"
callee         := OPNAME | IDENT ("::" callee_path)? | ("pkg" | "std") "::" callee_path
                | pack_use ("::" (IDENT | TYPEID))?
callee_path    := IDENT ("::" callee_path)? | pack_use ("::" (IDENT | TYPEID))?
fieldinit_list := fieldinit ("," fieldinit)*
fieldinit      := IDENT ":" atom
borrow_expr    := "&" place
atom_list      := atom ("," atom)*
clause_expr    := affine_expr (clause_op affine_expr)?
clause_op      := compare_op | "+defined" | "-defined" | "*defined"
                | "/defined" | "%defined"
place          := pbase psuffix*
pbase          := IDENT | "deref" "(" place ")" | "entry" "(" IDENT ")"
psuffix        := "." IDENT | "." TYPEID "." IDENT | "[" atom range_tail? "]"
range_tail     := ".." atom
```

The enum payload step is `"." TYPEID "." IDENT` — the variant name and then that variant's declared field name, `n.left.Some.value` — and it is the one spelling for reaching a payload, the kernel having no positional fields [GRAM-8].
A field step and a payload step are told apart by the shape kind of the token after the `.`, never by grammar position or context, so both spellings are META-2-clean [GRAM-1].
The range step is factored into `"[" atom range_tail? "]"` with `range_tail` a production of its own, because `"[" atom "]"` and `"[" atom ".." atom "]"` as two alternatives would share a `SELECT_2` language [GRAM-1]; `range_tail` maps 1:1 to its own node kind, so the mapping of [GRAM-1] holds for the factored form.
The `deref` alternative of `pbase` takes any `place` whose selected kind is a reference — `&T` or `&[T]` — and spells its referent [TYPE-7]; a `Box`'s content is its field `inner`, reached by the ordinary field step [TYPE-9].
`borrow_expr` is `"&" place`: there is no permission marker and no other qualifier on a reference [REF-1].

[GRAM-6] There is no general operator syntax and no precedence: an `infix` expression is exactly one operation over two atoms [GRAM-5, GRAM-9], composition is by `let`, and no precedence, associativity, or parenthesization surface exists.
The `compare_op` alternatives are the six integer comparisons of [OP-1] and form `infix` expressions exactly as the `infix_op` arithmetic does; a `call` writes its type arguments after the `::` delimiter, `cvt::<u8, u32>(w)`, so that `IDENT "<"` begins a comparison and never a type-argument list, while a constructor `call` and a `type` write theirs bare.
There is no `while`.
Conditional control is type-driven with one form per class: a Bool condition takes `if`/`else`, an enum scrutinee takes `match`, and each is the sole legal form for its class — a `match` whose scrutinee has type `Bool` is a hard error citing GRAM-6 at the scrutinee `expr` node, with a repair [DIAG-1].
An `if` condition must have exact value mode and type `own Bool` under exactly the [OP-5] condition judgment, TYPE-7 exclusivity included; every other condition failure cites GRAM-6 at the condition `expr` node.
An `if_stmt` `else` whose block is empty is a hard error citing GRAM-6 at that `if_stmt` node, with a repair [DIAG-1]; a `value_if`'s undelivering else is [GIVE-1]'s rejection, not this one.
An `else` whose block contains exactly one `if_stmt` and nothing else is a hard error citing GRAM-6 at that nested `if_stmt` node, with a repair [DIAG-1]; in a `value_if` whose else block is exactly one else-free `if_stmt`, the branch cannot deliver, [GIVE-1] owns the rejection, and GRAM-6 forms no candidate there, so no repair demands a chain form that could not be spelled.
A conditional value is a `let`-initializer `match` or `if` [GRAM-7, GIVE-1].
The only iteration forms are the ordinary `loop` plus `break`, and the counted ascending half-open `for` form whose complete semantics are [TYPE-5, TYPE-6, OWN-11, FN-1, ENT-2, ENT-3, ENT-5]; there is no step, reverse, iterator, or `continue` form.
The subscript suffix, the range step, and the enum payload step are place forms (their sole home); bounds semantics are [OP-4].
A `clause_expr` is the contract-clause shape and its sole home is a `requires_clause` and an `ensures_clause` [GRAM-2]: one `affine_expr`, or two around one `clause_op`.
Its side is [GRAM-4]'s own `affine_expr` [INV-1] and its factor is an `atom`, a `call`, or a constructor `call`, where the `call` factor stands for a constructor `call` and the domain-query rows while a measure reaches a clause as an `atom` [MSR-5].
`+`, `-`, and `*` are consumed inside `affine_expr` and are never a `clause_op`, so the operator position carries exactly the Bool-valued rows of [OP-1] — the six comparisons and the five infix `defined` domain queries — and no alternative of the union derives two ways [GRAM-1].
Every other position keeps [GRAM-9]'s one-operation-over-two-atoms shape and a nested call is still bound by its own `let`.

[GRAM-7] `match` and `if` each have one source body shape and two distinct core-tree node kinds: `match_stmt`/`if_stmt` for statements, `value_match`/`value_if` for a `let` initializer.
The pairs never compete at one grammar decision: the statement forms begin at the statement boundary, the value forms only after the complete `let IDENT =` prefix, so the parser decides from source position alone, without type, name-resolution, or checker context.
A value form is value-producing, and every arm or branch must satisfy the complete [GIVE-1] delivery judgment for its binding; a `value_if`'s `else` is grammatically mandatory [GRAM-4] because a missing branch could not deliver.
Statement forms produce no value; their bodies act by effect.
`return`-position conditionals deliver by returning from branches; there is no helper-function conditional-initialization idiom, and value-production is confined to the `let` initializer, so neither construct ever occupies an arbitrary expression position.

[GIVE-1] `give e;` delivers `e` as the value of the nearest enclosing value initializer — a `value_match` or `value_if`.
An else-position `value_if` of a chain is part of the chain, not a nested initializer: its `give`s deliver to the chain's binding.
A value initializer bound by its own inner `let` delivers only to that inner binding and never makes an outer arm or branch deliver.
`give` is legal only inside a value initializer's arm or branch — a checker-scoped restriction exactly as `break`'s enclosing-loop rule [TYPE-6]: the grammar admits `give_stmt` and the checker restricts it, which is META-2-clean by the `break` precedent.
The binding's mode and type are derived from the delivery set [TYPE-5]: every delivering `give` of one value initializer must have one identical exact mode and type, and that is the binding's derived mode and type; a delivering `give` whose exact mode or type differs from an earlier delivering `give` of the same initializer is a hard error citing GIVE-1 at the later `give_stmt` node — derivation is agreement over the closed delivery set, never a join, widening, or common-supertype rule.
When every delivering `give` of one initializer delivers a reference, that agreement is agreement of reference kind — `&T` with the same `&T`, `&[T]` with the same `&[T]` — and the binder's path set is [REF-1]'s union over the delivery set.
A value initializer whose delivery set is empty — every arm or branch leaves by `return` or by `break` to an enclosing loop — is a hard error citing GIVE-1 at the `let_stmt` node, with a repair [DIAG-1].
On every control path an arm or branch terminates in exactly one `give e;` or cannot reach the initializer's continuation; a give-free continuing path, a statement following a `give` in the same block, and a second `give` on one path are each a hard error citing GIVE-1 — the value analog of match exhaustiveness [ERR-2].
Give-completeness is a structural last-statement recursion: an arm or branch delivers when its final statement is a `give_stmt`, a `return_stmt`, a `break_stmt` whose resolved target loop lexically encloses the same value initializer, a `match_stmt` every arm of which delivers, or an `if_stmt` with `else` both branches of which deliver, relative to that same value initializer; an else-free `if_stmt` has a continuing false edge and never delivers.
A final nested value initializer bound by its own `let` delivers only to its own inner let and therefore does not make the outer arm or branch deliver.
A call with a normal result edge does not itself count as delivery or must-divergence.
No `loop_stmt` or `for_stmt` is assumed to diverge.
This recursion is strictly simpler than the ownership checker.
`give e;` moves or copies `e` per [OWN-1].
When an initializer's derived delivery mode is `own` and its type is one [ENT-2] fragment integer, a direct non-consuming bare-atom `give` additionally participates in [ENT-5]'s bounded relation delivery.
A local Result value follows ENT-5's conditional value transport through either initializer.
This scalar delivery adds no typing premise and never makes a move, borrow, call, construction, subscript, projection, or computed expression into a scalar fact carrier.
GIVE-1 still owns delivery completeness and exact mode/type agreement; only after those judgments succeed may ENT-5 substitute the atom's already evaluated value into the receiving binding.

For that additional fact-carrier judgment, the direct atom must be one bare tracked own-value binding of the exact receiving type: its root resolves to a body `let_stmt` binding, `for_stmt` binder, parameter, or match binder, and it carries no suffix.
A literal, named const, const-generic constant, Z, counted capture, contract definition, symbolic result datum, projected place, consuming atom, or any other atom may still be admitted in its own grammar role but carries no relation through a value initializer.
Replace every occurrence of the delivered binding d with the receiver x (`d ↦ x`); no receiver fact is read and no inverse substitution is formed.

[GRAM-8] Named construction.
A constructor `call` of struct or enum-variant type K writes every declared field of K exactly once as `IDENT ":" atom`, the IDENTs equal to K's declared field names in declared order.
A missing, extra, repeated, misspelled, or out-of-order field name is a hard error citing GRAM-8 and K's declared field list.
There is no positional construction form; a nullary K is written `K()`.
Field names are redundant-explicit facts (the TYPE-5 class): checked, never chosen, never a reordering option (declared order is the one legal byte sequence).
The name-only-when-two-same-typed-fields alternative is a context-dependent spelling and is rejected [META-2].

[GRAM-9] Flat (three-address) computation.
Every call argument, construct field value, infix operand, subscript offset, and lower or upper endpoint of a `for_stmt` is an `atom` [GRAM-5]; a `call` or constructor `call` in an atom position does not derive under the grammar and is a hard error citing GRAM-9.
A computed value is forwarded to another operation only by binding it with a preceding `let` (whose mode and type are derived [TYPE-5]) and referencing the binding.
Nesting and let-splitting are not two spellings of one computation; there is no expression-nesting alternative [FORM-1].
`borrow_expr` is an `atom`, so references passed as arguments need no binding and [REF-1] is untouched.

[GRAM-10] Named match binders.
An `arm` for variant K writes declared fields of K, each at most once as `IDENT ":" IDENT` (the declared field name, then a fresh binder), in declared order, and a final `..` covers every declared field the arm does not write; a missing field name with no `..`, an extra, a repeated, a misspelled, or an out-of-order field name is a hard error citing GRAM-10 and K's declared field list.
In an own-place match [OWN-13] each covered field takes its compiler-derived release on entry to the arm [STOR-3], and a covered field of linear type is a hard error citing WIN-3 at the `arm` [PROV-6]; a reference-mode match covers a field without releasing it.
The binder is a fresh IDENT chosen by the writer and distinct from the field name, so TYPE-6 no-shadowing is never engaged by two arms binding fields of the same name.
Binder modes remain derived by [OWN-13] (not written), a reference-mode binder naming the scrutinee path extended by its payload step [REF-1].
A nullary variant is written `K()`.

The `result_route` owns exactly one `fieldbind`, so zero-field and multi-field route shapes do not derive.
FN-9, not GRAM-10, owns that route after its leading TYPEID resolves: it admits exactly `Ok(value: IDENT)` for a concrete `Result<T, E>` whose T is one entailment-fragment integer type.
A misspelled field is therefore an FN-9 rejection at the `fieldbind`, as [DIAG-1] fixes; no match arm or runtime binder is formed.
Every other successfully resolved variant, payload type, nested projection, or route is outside the postcondition boundary and is rejected by FN-9 rather than generalized through this rule.

[GRAM-11] Named call arguments.
A `call` whose callee resolves to an ordinary function, a source function and a [PRE-1] prelude function alike, writes its arguments as `fieldinit_list` [GRAM-5] — each `IDENT ":" atom` equal to the callee's declared parameter names in declared order [FN-1], the GRAM-8 discipline applied to calls.
A missing, extra, repeated, misspelled, or out-of-order parameter name is a hard error citing GRAM-11 and the callee's parameter list.
A `call` whose callee resolves to a table operation [OP-1] writes positional `atom_list` operands (operands are order-intrinsic and unnamed).
Argument reordering is not a spelling option: declared order is the one legal byte sequence [FORM-1], so parameter names are redundant checked facts, never a reordering license.
Callee kind is resolved by name lookup [OP-1], the same partition that already selects the callee.

## 4. Types

[TYPE-1] Primitive types: `i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 unit`.
(`Bool` is a prelude enum, §14, not a primitive.)

[TYPE-2] Composite types: `struct` and `enum`; the three storage shapes `Array`, `Slots`, and `Ring` and the cell `Box` are the prelude's opaque structs [TYPE-9, PRE-1].
The four are ordinary nominals of the nominal-type TYPEID domain [TYPE-6], written as a TYPEID with `targs` [GRAM-3]; what a declaration cannot state — element storage, the omitted-capacity form, placement, and element domains — is [TYPE-9]'s.
In this specification's prose `N` stands for a written const argument; source writes a `const` IDENT, lowercase under [FORM-3], as the [PRE-1] rows do.
`Slots`, `Ring`, and `Box` are declared `nocopy`, so their values are affine unless an element or content type makes them linear, and an `Array` has exactly the capabilities of its element type [OWN-1, PROV-6].
A `struct` or `enum` declaration may carry one capability modifier [GRAM-2]: `nodrop`, which states a logical must-consume obligation on values of that nominal in every scope, or `nocopy`, which makes its values non-duplicable although every part could be copied; neither changes a component, layout, or construction route [OWN-1, PROV-6].
A `struct` declaration may carry the `opaque` modifier [GRAM-2], written before a capability modifier when both are present: an opaque struct has fields and no usable constructor. Its constructor entry [TYPE-6] exists to be refused: a constructor `call` whose leading TYPEID names an opaque struct is a hard error citing TYPE-2 at the complete `call`, and a destructuring `let_stmt` whose TYPEID names one is a hard error citing TYPE-2 at the complete `let_stmt`, each with a repair [DIAG-1]. Its fields obey the ordinary field, ownership, and release rules [OWN-1, PROV-6, STOR-3], and a `move` out of one of its fields is the ordinary [WIN-3] consume. A value of an opaque struct is formed only by a definition the build supplies: a construction row [OP-13] forms the three storage shapes and `Box<T>`, which the prelude declares [PRE-1], and a host function forms the host handles the host modules declare [PRE-2], so no other opaque struct ever has a value.
A `field` may carry the `readonly` modifier [GRAM-2]; a source field carries it only together with `public` [MOD-6]. Inside the module that declares a source struct its readonly field is an ordinary field. Outside that module — and everywhere, for a PRE-1 struct's field — a path that ends at or passes through a readonly field is never a write target: a `set` whose target is such a path [SET-1], and an argument naming such a path at a reference parameter whose callee row writes that parameter [EFF-5], are each a hard error citing TYPE-2 at the complete target `place` or argument `atom`, with a repair [DIAG-1]. Construction gives a readonly field its value like any other field [GRAM-8], and a construction outside the declaring module supplies none [MOD-5]; a whole-value assignment replaces it together with its owner. Its value otherwise changes only through a compiler-owned [PRE-1] operation whose row declares `writes` of it [OP-10]; a declared row may name a readonly field in `writes`, because a row reports every change its callees make [EFF-2]. `readonly` states that the field is not assignable, not that its value is constant.

[TYPE-3] Nameability: every constructible type, parameter kind and effect has a canonical, finite source spelling requiring no compiler execution [GRAM-3, EFF-1].
A capability modifier and a generic parameter's capability bound are properties of a declaration and not components of a type name: two instances of one nominal have one name whether or not its declaration is marked, and no name spells a capability [PROV-6].

[TYPE-4] There are no implicit conversions.
Numeric value conversion uses the explicit `cvt`, `cvt.checked`, and `cvt.defined` interfaces of [OP-6].

[TYPE-5] Statement-local typing; boundary-explicit facts.
The factored `call` grammar denotes a construction exactly when its callee is an unqualified TYPEID application. A constructor writes any nominal arguments directly after that TYPEID, never with the function-call `::` introducer; writing the latter is a TYPE-5 error at the complete call. Its operands are named fields under GRAM-8, so a positional operand list is a GRAM-8 error there. Construction is an ordinary expression, not the callable occurrence required by an expression statement or a destructuring result-list let; either statement position rejects it under TYPE-5. These judgments preserve the constructor forms while sharing the strong-LL(2) prefix with qualified member calls.
A `let` binder's mode and type are derived, never written: exactly the mode and type its selected right-hand side produces — an `ordinary_let_rhs` from its expression, which is always self-typed (operands are typed atoms, calls are typed by their [FN-1]/[OP-1] signatures, literals carry mandatory suffixes [FORM-5], constructions name their nominal and, when that nominal is generic, write its arguments); a `propagate_let_rhs` from the propagated Ok payload [ERR-3]; a `value_match` or `value_if` from the derived common delivery type [GIVE-1], whose delivering `give`s are inside the same `let_stmt`, so the derivation stays statement-local; and a parenthesized binder list from its `call`'s declared result ordinals, binder i at the mode and type determined by result ordinal i's declared `rtype` [GRAM-4, FN-1, CALL-4].
A binder whose selected right-hand side is a reference instead takes that reference kind, by the same derivation and on the same statement-local ground [REF-1].
This is unique reconstruction, not inference: no binder's type depends on a later statement, an expected type, or any use site, and no two derivations can disagree [FORM-1].
Call sites state explicitly exactly what their callee class requires: type, const, and function arguments for user generics [FN-2], including group abbreviations; and, for exactly the retained-argument table operations — `cvt`, `cvt.checked`, `cvt.defined`, and `reinterpret` (type pairs [OP-6, OP-8]) and `finf`/`fnan` (result type) — the written arguments their rows fix, because no operand can supply them.
A constructor `call` of a generic nominal states that nominal's type, const, and function arguments on the same ground and in every position, mandatorily: the source nominals under [FN-2], and the prelude generic nominals `Option<T>` and `Result<T, E>` through their variant constructors `None`, `Some`, `Ok`, and `Err`.
A nullary `None()` has no operand to supply anything, and construction never consults an expected nominal type [TYPE-6], so the written arguments are the only supply there is; their absence, or a count other than the named nominal's parameter list, is a hard error citing TYPE-5 at the complete constructor `call`.
A non-generic prelude nominal [PRE-1] has no parameters and writes no type arguments.
Every other table operation carries no written argument and derives its selected type from its operands [OP-2]; a written argument there is a hard error citing OP-1.
Argument types match declared parameter types exactly.
After [SET-1] derives a writable value target place of type T, the right-hand side of `set p = e;` must produce exactly `own T`; there is no mode coercion, type conversion, or target-selected operation overload.
After the TYPE-7 implicit-read exclusivity below, a different right-hand-side mode or type is a hard error citing TYPE-5 at the complete `expr` child of the `set_stmt`, carrying expected `own T` and the actual mode and type.
A binder list whose written count differs from its `call`'s declared result count, or whose callee declares a single result, is a hard error citing TYPE-5 at the complete `call` child of the statement, carrying the written count and the actual result.
Explicit boundary information remains mandatory — signatures with parameter kinds determined by [GRAM-3], full types and effect rows [FN-1], construction field names [GRAM-8], match binders [GRAM-10], call argument names [GRAM-11] — while body binder annotations are absent under the reconstruction rule above.

Every result ordinal of a `fn_decl` or `fn_sig` has one mandatory `result_binding` whose written `rtype` fixes that callable result's mode and type.
The result name is a proof-only boundary spelling: it denotes no runtime slot, does not enter callable signature equality, and is unavailable in a function body.
An unrouted [FN-9] postcondition may admit it as that clause's symbolic whole-result datum; a routed postcondition instead derives its payload binder's type from the admitted `Result.Ok` payload and makes the whole-result name unavailable in that clause.

A `contract_define` derives exactly the own copy mode and type of its right-hand-side expression.
It is an erased, declaration-before-use abbreviation rather than a statement, evaluation, snapshot, or storage allocation.
Its initializer must satisfy [FN-8]'s pure, total, non-consuming contract-expression judgment; every clause use is recursively alpha-expanded before a proof template is formed.

Each lower and upper endpoint atom of a `for_stmt` must produce exactly `own u64`; after [TYPE-7]'s implicit-read exclusivity, every other mode or type is a hard error citing TYPE-5 at that endpoint's `atom` node, with `SourceCoordinate` equal to its complete checked half-open source extent.
The counted binder has the fixed compiler-derived mode and type `own u64`; it carries no source annotation and does not infer from either endpoint.

[TYPE-6] Name resolution uses the following closed declaration domains.
The grammar role, never an inferred type or expected result, selects the domain and admissible declaration class.

| domain | declarations | admitted uses |
|---|---|---|
| lexical IDENT | top-level `fn_decl`; raw function-kind `gparam`; top-level `const_decl`; const `gparam`; `param`; `let_stmt`; `for_stmt` binder; arm `fieldbind` binders; `contract_define`; FN-9-owned result and route candidates; PRE-1 functions | a `callee` IDENT admits a top-level function, in-scope function parameter, or PRE-1 function; an unqualified `function_arg` or `fn_bind` right side admits an ordinary function or function parameter; `const` IDENT admits an in-scope const generic or a named const; `cvalue` IDENT admits a named const; `pbase` admits an in-scope runtime value binding, contract definition, admitted symbolic result datum, named const, or in-scope const generic [MSR-6] |
| nominal-type TYPEID | source `struct_decl` and `enum_decl` names; source interface and binding groups; PRE-1 nominal types; lexical type `gparam`s overlay this domain while live | a runtime `type` or generic-numeric suffix admits only its ordinary type class; an explicit `targ` additionally admits a interface or binding abbreviation; a `pack_use` admits a interface or binding group, with FN-3/FN-5 checking its position and member selection |
| constructor TYPEID | each source struct constructor under its struct TYPEID; PRE-1 variants, classified as struct-constructor or enum-variant; an opaque struct's constructor, existing only to be refused [TYPE-2] | the constructor TYPEID of a `call`, `cvalue` or destructuring `let_stmt` admits either class |
| numeric-bound TYPEID | the two built-in bounds `Int` and `Float` [PRE-1] | the bound TYPEID of a type `gparam`; a capability bound instead uses its fixed grammar spelling [GRAM-2, PROV-6] |
| LABEL | an optional LABEL written by `loop_stmt` or `for_stmt` | an optional LABEL written by `break_stmt` |
| invariant IDENT | names written by `header_invariant` and `invariant_stmt` | the IDENT premise alternative of `use_premise` |

A source struct contributes one declaration event that adds one nominal-type entry and one constructor entry with the same spelling.
Those entries do not collide because the grammar distinguishes a `type` role from a constructor `call` role.
An enum declaration adds only its nominal type. Each source variant belongs to its enum and enters no constructor domain: a construction writes it after its enum, `E::V(...)` or `E<args>::V(...)`, directly or behind a module prefix [MOD-5], or through an alias of it [MOD-4]; an enum declares each variant spelling once, and variants of different enums never collide.
The label of an `arm` resolves against its scrutinee's already known enum type, and the label of a `result_route` against its result ordinal's declared type: it names the variant of that enum with its spelling, and an arm label naming no variant of the scrutinee's enum is a hard error citing TYPE-6 at the `arm`. A PRE-1 variant is written unqualified.
Entries must be unique within, but not across, the nominal-type, constructor, and numeric-bound domains. Interface and binding group names share the nominal-type collision domain, but neither is a runtime type or a constructor.
Construction and matching never consult an expected nominal type: a constructor names its struct, its PRE-1 variant or its variant's enum, and an arm or route label reads the type its scrutinee or result already has.

PRE-1 contributes its declaration records in the preorder stated there.
The prelude's nominals, constructors, functions and numeric bounds, including the construction functions [OP-13] and the window operations [OP-10], enter the ordinary lookup inventory and are visible in every module. A declaration's type parameters, value parameters and fields are owner-local and enter only that declaration's ordinary owner tables.
PRE-1 records have no source event or source node.
Every top-level declaration of a module — function, struct with its constructor, enum, interface group, binding group and named const — is visible, independently of record and item order, in the records of its module that [MOD-3] selects, after inventory formation and before any use is resolved [FN-1].
An alias is visible throughout its own record [MOD-4]. Another module's declarations are reached only through a module prefix or an alias [MOD-5].

A generic TYPEID parameter becomes visible after its declaring terminal through the remainder of its declaration's generic, header, and body scope.
It may not redeclare another parameter in the same generic list or shadow a live nominal type or enclosing generic type.
Constructor and numeric-bound spellings are separate grammar-selected domains and do not participate in that comparison. Interface and binding names do participate.
A const generic becomes visible after its complete `gparam`. A raw function-kind parameter becomes visible after its complete `fn_sig` through the receiving declaration's remaining header and body; its own value parameters and proof candidates remain local to its signature. A interface member has a declaration identity but no unqualified lexical entry; FN-5 selects it through the written group application.
A `fn_decl` parameter becomes visible after its complete `param` through the function's optional `contract_block` and body.
A `fn_sig` parameter becomes visible after its complete `param` through that signature's effects and optional contract block; duplicate parameters in that signature are same-scope redeclarations. It is not visible in a sibling member or the receiving function's body.
A `let_stmt` binder becomes visible only after its complete initializer statement through the end of its lexical block.
A `contract_define` binder becomes visible after its complete initializer through later definitions and every following requires or ensures clause of that one block, and nowhere in the body.
Its initializer may therefore use parameters, named consts, live type or const parameters, and earlier definitions, never itself or a later definition.
The IDENT of every `result_binding` is an FN-9-owned proof candidate rather than a runtime TYPE-6 value declaration.
It participates in FORM-3 reservation and must differ from every parameter and definition in its function or function-formal signature.
In a `fn_decl` or `fn_sig`, FN-9 admits each eligible ordinal as a symbolic datum visible only in that signature's ensures clauses under the same result-route rules.
The second IDENT of a `result_route` fieldbind is an FN-9-owned payload candidate.
After the route's leading constructor and field are admitted, that binder is visible only in the same `ensures_clause` expression and the header result binder is not visible there.
It must differ from its paired field, every parameter, the result binder, and every live definition.
Different ensures clauses have disjoint result-datum scopes and may reuse one route-binder spelling.
Neither kind of result datum has runtime storage or ownership state, and neither is visible in the function body, so a body `let` may reuse a result binder's spelling without a redeclaration event.
A match binder becomes visible in its arm body only after the complete fieldbind list and only after GRAM-10 has established that it differs from its paired field label, every earlier binder in that arm list, and every lexical-IDENT declaration live on arm entry.
A `for_binding` binder becomes visible after its complete `for_binding`, including both endpoint atoms, through the remaining `header_invariant` clauses and the counted body; it is not visible in either endpoint.
An ordinary or counted loop label, when written, is visible only in its loop body; a counted label is not visible in the binding or invariant header.
A loop label is an optional lexical name, never the identity of the loop: every `loop_stmt` and `for_stmt` has one distinct compiler-owned structural loop identity whether or not it writes a LABEL.
An unlabeled `break;` must be lexically inside at least one ordinary or counted loop and resolves to the nearest such enclosing loop.
A labeled `break @name;` performs the ordinary LABEL-domain lookup below and may therefore resolve past one or more inner loops to an enclosing loop carrying that spelling.
The resolved loop's structural identity, not a LABEL declaration, is the target retained by the semantic checker.
A `header_invariant` name is a proof-only declaration in a separate invariant-name domain.
All names in one header must be distinct; none is visible in the header itself or before the loop, and after the complete header all become visible simultaneously throughout that loop body only.
An `invariant_stmt` name becomes visible only after its complete statement through the remainder of its lexical block and nested blocks.
An invariant name never denotes a runtime value, place, ownership object, label, or callable, and it is referenced only by the IDENT premise alternative of `use_premise` under [PRF-1].
Within the invariant-name domain a new live declaration may not shadow another live declaration, while disjoint expired scopes may reuse a spelling.
Adding, removing, or changing a loop label cannot change any invariant binding.
A named const is visible in its whole module like every top-level declaration; [CONST-2] judges the dependencies among constants.

Within one domain, two declarations in one module's inventory, in one record's alias header, or in the same lexical scope are a redeclaration attributed to the later declaration event.
Declarations in unrelated function or declaration owners are not duplicates merely because their spellings match.
A nested lexical declaration may not shadow an entry live at that declaration.
GRAM-10 exclusively owns arm match-binder distinctness and freshness: a second IDENT of an arm `fieldbind` equal to its paired field label, an earlier binder in the same arm list, or any lexical-IDENT declaration live on arm entry is rejected citing GRAM-10 at that later/offending binder before it becomes a declaration, rather than also being reported as TYPE-6 shadowing.
FN-9 exclusively owns the analogous result-datum checks described above; failure creates no TYPE-6 declaration or duplicate event.
Because every top-level declaration and every alias is live throughout its module or record, any other parameter, local, or generic in a nested scope may not use such a spelling in its domain even when that declaration's source item occurs later; the nested declaration is the offending shadow event.
Disjoint expired lexical scopes may reuse an ordinary value or label spelling.
Within one module, logical paths and record boundaries never create a namespace or lookup key; a module and a record's alias header are the only scopes above a declaration [MOD-3, MOD-4].

The owner-dependent declaration and use roles are exactly the carriers classified by [DIAG-1].
They do not enter or query a lexical name domain.
DIAG-1 retains each owner-dependent carrier for later typed owner/member checking.
Deferral is neither acceptance nor rejection of its later owner/member relation.

[MSR-6] A const generic is a value wherever a named const is.
The `pbase` admission of [TYPE-6] carries an in-scope const generic, and with it the `for_stmt` endpoint admission of [ENT-2], the clause operand of [MSR-5], and the affine atom of [INV-1] carry one too.
The first three are the positions in which a named const is already a value; the fourth is one it is not, and a const generic is admitted there on its own ground: [ENT-2] clause (c) makes it a constant rather than a tracked place, so it needs no liveness, no entry state and no support, while a named const is a term of clause (a) whose exclusion from the affine atom this version keeps.
A read of an in-scope const generic is a `pbase` with no `psuffix` and no `deref` wrapping; its exact type is the `gparam`'s written integer type and its value mode is exact `own`, so the ordinary [TYPE-5] check applies at each use with no widening of any other judgment.
Reading a const generic performs no operation, allocates nothing, and has the empty effect row.
A const generic is fixed at [FN-2] instantiation, so a concrete instance reads a mathematical constant and the one source-canonical symbolic instance reads the symbolic constant term [ENT-2] clause (c) already fixes; this rule adds a spelling and no fact source.
It introduces no shadowing hazard, because a const generic is already a lexical-IDENT declaration and a colliding later binding is a TYPE-6 redeclaration before this admission is consulted.
Absence of an in-scope const generic under this spelling remains the ordinary [TYPE-5] unresolved-use rejection.

[TYPE-8] A reference kind is not a value type.
`&T` and `&[T]` are reference kinds and not types [GRAM-3].
No struct field, enum variant payload, `Array`, `Slots`, or `Ring` element, `Box` content, or written generic type argument may be one, recursively and closed under wrapping, so `Option<&T>` does not form whatever T is.
The grammar admits no reference kind in a stored or type-argument position at all [GRAM-3, STOR-5], so the semantic check closes exactly one case: a substituted generic instance.
A violation is a hard error citing TYPE-8 at the complete offending `type`, with a repair [DIAG-1].
Every aggregate therefore holds only owned values, which is what [STOR-7] rests on.

[TYPE-9] Three storage shapes, two placements each, and one cell.
`Array`, `Slots`, and `Ring` are the prelude's opaque structs [TYPE-2, PRE-1], each declared with a const capacity parameter N and its measures as readonly fields [MSR-1]; their element storage is compiler-owned and reached only by a subscript [OP-4] and the window operations [OP-10].
`Array<T, N>`, `Slots<T, N>`, and `Ring<T, N>` are the constant-capacity forms, whose capacity is the type constant N [CONST-1] and whose storage is inline in the owner or the stack frame [STOR-1].
`Array<T>`, `Slots<T>`, and `Ring<T>` are the runtime-capacity forms, written by omitting the const argument N, whose capacity is fixed at construction and read as the readonly field `cap`, or as `len` for an `Array<T>` [MSR-1]; a runtime-capacity form may appear only as the content of a `Box` — the type of its `inner` field — and never inline in another value and never as a local binding; every other position is a hard error citing TYPE-9 at the complete `type`, with a repair [DIAG-1].
`Box<T>` is the prelude's opaque struct `opaque nocopy struct Box<T> { inner: T; }` [TYPE-2, PRE-1]: its one field `inner` is its content, stored in exactly one heap object the `Box` value owns [STOR-1]; T is any nameable type [TYPE-3], including a runtime-capacity form; there is one heap [STOR-8], a `Box` carries no brand, and it may be moved, stored in an aggregate, and returned freely.
The content is reached by the ordinary field step, `b.inner`, and through a reference to the cell as `deref(cell).inner`, where `deref` steps through the reference and `inner` through the cell; `deref` never reaches the content itself [TYPE-7, REF-1]. `let n = move b.inner;` consumes the `Box`, yields its content, and frees the cell [WIN-3].
A `move` of a runtime-capacity content is a hard error citing TYPE-9 at the complete `place`, with a repair [DIAG-1].
The element type of any shape is any nameable type, copy, affine, or linear [OWN-1, PROV-6].
A constructor `call` and a destructuring `let_stmt` naming any of the four is refused by [TYPE-2] like every opaque struct's, with a repair [DIAG-1].

[TYPE-10] Window parts are names, not declarations.
`len`, `cap`, and `head` are the readonly fields the prelude declares on the storage shapes [PRE-1, MSR-1]; a program reads them as fields [OP-15] and can never assign one [TYPE-2], and only the operations of [OP-10] and [OP-13] change them.
`next`, `last`, `filled`, and `free` are the four window parts [WIN-2]; they are vocabulary for effect rows and the overlap judgment only, selected by the window type of the place they follow, and they occupy no declaration domain and reserve nothing: a binding, a field of another type, or a label may carry the same spelling.
A read of a window part, a `borrow_expr` over one, and a write of one are each a hard error citing TYPE-10 at the complete `place`, with a repair [DIAG-1].

[TYPE-7] Reading through a reference is explicit.
`deref(place)` where `place` has type `&T` or `&[T]` denotes a place of referent type T [GRAM-5] — for `&[T]` the run of T elements that range names [REF-4] — and a use of that place copies it when T is copy and requires `move` when T is affine [OWN-1].
A reference binding used where a value of its referent type is expected is a hard error citing TYPE-7, with a repair [DIAG-1].
There is no implicit read through a reference [TYPE-4, META-2].
`deref(place)` where `place` is not a reference, a `Box` included, is a hard error citing TYPE-7 at the complete `place`, with a repair [DIAG-1].

[SET-1] Place assignment.
For `set p = e;`, target evaluation first resolves and evaluates the complete `p` without reading or consuming the value stored there.
A `set` target must resolve to a binding already in scope: a `set` whose target name resolves to nothing declares nothing and is a hard error citing SET-1 at that `place`, with a repair [DIAG-1].
A nested place is evaluated from its base outward; at each subscript, the base place is evaluated before its offset atom, and the subscript's [OP-4] discharge obligation is judged at that target place exactly as in read position, so accepted target evaluation executes no runtime check and cannot trap.
Field suffixes introduce no runtime evaluation.

This rule judges a value target; a `set` whose target is a reference variable and whose right-hand side is a `borrow_expr` rebinds that name and is judged by [REF-1] instead.
A `set` whose target is a reference variable and whose right-hand side is a value is not a rebinding: it is a hard error citing TYPE-7 at the target `place`, with a repair [DIAG-1].
The value target's final selected type is T.
The target is writable exactly when it is rooted in a live own-mode value binding, is `deref(p)` or a path below it where `p` is a reference parameter whose declared row carries `writes` of that path [EFF-1, EFF-5] or a local reference variable whose named path is itself writable.
Fields and indices inherit the writability of their selected base.
A named const is never writable [CONST-2], and a target path that ends at or passes through a readonly field is refused by [TYPE-2].
A `for_stmt` binder is compiler-updated state and is never source-writable; a target rooted there is a SET-1 rejection at the complete target `place`.
A place projected, dereferenced, or subscripted from a dead root is never writable; a dead binding is writable only as the complete binding, and that commit reinitializes it [OWN-1, OWN-11].
These specific rules own their stated violations; every other failure of this closed writability relation cites SET-1 at the complete target `place` child of the `set_stmt`, carrying the resolved root class and the required writable classes.

T is copy or affine under [OWN-1].
The right-hand side is then checked under [TYPE-5] and evaluated under its ordinary expression, ownership, effect, and partial-operation domain rules.
The checker analyzes the normal continuation of `e` and re-establishes there that the resolved target remains writable and that the target root is live.
If the right-hand side moved a strict prefix of the target place, the commit is a later write of a dead root under OWN-1.
If it invalidated a reference the target path is reached through, [REF-2] rejects the commit.
This is a static acceptance check: at runtime every target component is evaluated exactly once before `e`, and lowering carries the resulting target address and offset values across `e` rather than evaluating source again.
No root-liveness or writability fact from before the right-hand side bypasses the post-state check.

On successful revalidation, assignment performs exactly one write of the resulting value into `p`.
The old value's disposition at the commit is [WIN-3]'s.
A commit derives no drop, release, finalizer, or cleanup edge beyond that disposition.
The new value occupies the same place and the target root is live after the commit.
The store occurs only after right-hand-side evaluation completes; until that commit point the target retains its previous value.
The checked program retains the exact target path, each required target check, the right-hand-side value, the post-right-hand-side liveness and writability judgments, and the single store before lowering [DIAG-2].

[CONST-1] The grammar production `const` of the fence below is usable at `Array<T, N>` sizes and `const` targs, and, being the `const` alternative of `targ` [GRAM-3], at every const argument of a compiler-owned storage nominal [TYPE-9].

```wf-ebnf CONST-1
const := ("[0-9]+" | IDENT) (infix_op ("[0-9]+" | IDENT))?
```

A decimal integer literal is bare and u64 by position; an IDENT names an in-scope integer-typed const-generic parameter [GRAM-2] or a top-level integer-typed named-const item [CONST-2].
A const-expression is at most one operation over two terms, exactly the shape [GRAM-6] fixes for expressions: composition is by a named const or a forwarded const parameter, and no precedence, associativity, or parenthesization surface exists.
The tail reuses `infix_op`, and its spelling must be one of the five bare operators `+`, `-`, `*`, `/`, `%`; a mode-suffixed spelling is a hard error citing CONST-1 at the `infix_op` node, because const evaluation has no runtime overflow mode — the grammar admits and the checker restricts, META-2-clean by the `break` precedent [GIVE-1].
Constant-expressions are evaluated at monomorphization [FN-2].
An IDENT resolving to a non-integer or `Array`-typed const is a compile-time rejection [DIAG-1].
Const evaluation is exact in the unsigned 64-bit domain under the const-eval overflow policy named `const-reject`: an operation whose mathematical result lies outside that domain, or whose divisor is zero, is a compile-time rejection citing CONST-1 at the complete `const` node.
`const-reject` is disjoint from runtime proof-required exact arithmetic: it never creates an [ENT-6] operation obligation or admits a `.defined` spelling, an accepted const-expression executes no runtime check, and a const-expression contributes no runtime effect.
Inside a generic template an unevaluated const-expression is symbolic; two symbolic const-expressions are identical exactly when their operation and ordered terms are identical, with no commutation, constant folding, or reassociation, exactly as [FN-8] fixes goal identity.
This keeps the const-generic forwarding path closed under the one operation: `const N` is usable as an `Array<T, N>` size, and a derived expression such as `N * 2` is usable there and forwardable as a `const` targ, with each concrete instantiation evaluating it to one u64 value.

[CONST-2] A `const IDENT: type = cvalue;` item declares an immutable, program-lifetime, read-only static value, with the `cvalue` production of the fence below.

```wf-ebnf CONST-2
cvalue := literal | IDENT | "[" cvalue ("," cvalue)* "]"
        | (TYPEID | type_path) targs? ("::" TYPEID)? "(" (IDENT ":" cvalue ("," IDENT ":" cvalue)*)? ")"
```

`type` must be const-eligible: a primitive [TYPE-1], `Array<T, N>` of const-eligible T, or a source non-opaque `struct` whose every field type is const-eligible; `enum`, `Box`, `Slots`, and `Ring` are not const-eligible (a const is pure static rodata: no allocation, no drop).
The `cvalue` totally defines the value: a primitive-typed const takes a FORM-5 numeric or unit literal or an IDENT naming a const of that exact type; an `Array<T, N>`-typed const takes `[cvalue, ..., cvalue]` with exactly N entries, each of type T, and a struct-typed const takes the construction form `TYPEID(field: cvalue, ...)` naming its exact struct and writing every declared field in declared order [GRAM-8], each field value a cvalue of the declared field type.
The const-dependency graph is acyclic: consts are visible throughout their module [MOD-3], and a const whose value depends on itself through any chain of consts is a hard error citing CONST-2 at the first const in item order on that cycle. Evaluation follows the dependencies and is substitution and layout only.
A const item is never `move`d or `set`, and no declared row may write a path rooted at one [EFF-1, EFF-5].
It is read via a subscript, a measure member [OP-15], a field suffix, or a `&` reference [REF-1], so a const table may be passed to a consumer.
A struct-typed const is additionally read via its field suffixes exactly as subscript reads: a copy-scalar selection copies out, and a composite selection keeps the whole-composite read rules.
A struct-typed const is laid out as one read-only static aggregate in the nominal's ordinary representation.

## 5. Ownership and references

[OWN-1] Every value has exactly one owner.
A type has two capabilities, copy and drop, and its class is read from them: a type with both is *copy*, a type with drop alone is *affine*, and a type with neither is *linear* [PROV-6]; no type has copy without drop.
Primitives (TYPE-1) have both. Every other type has a capability exactly when every part it owns has it [PROV-6] — its fields, its variant payload fields, its `Box` content, and the elements of a storage shape — and its declaration does not remove it [GRAM-2]: `nocopy` removes copy, and `nodrop` removes drop and copy with it. A tag-only enum and a struct of copy fields are therefore copy, `Bool` being the canonical case; the prelude declares `Slots`, `Ring` and `Box` `nocopy` [PRE-1] and the host modules declare every host handle `nocopy` or `nodrop` [PRE-2], so a type owning one of them is not copy, and an `Array` has the capabilities of its element type [TYPE-9].
A type parameter has the capabilities its bound grants [PROV-6], so a generic nominal's class is decided at each instance from its arguments.
An affine or linear place rooted in a live own-mode binding is consumed exactly once by an explicit `move p`, by use as an own-place match scrutinee under [OWN-13], by use as the direct bare affine `Result<T, E>` place operand of `propagate` under [ERR-3], or by the `move place` of a destructuring consume [PROV-6].
Every other bare `place` expression of affine type is a hard error, and `move p` on a copy value is a hard error (copy values are used bare — one spelling per meaning, FORM-1), each with a repair [DIAG-1].
That spelling judgment is made once per written body: at a concrete instance of a generic template it is not re-made, and a `move` of a value whose type parameter was bounded `drop` or left unbounded denotes a copy there [FN-2, PROV-6].
Resolving and evaluating the target of SET-1 does not by itself read, copy, or move the selected value or its affine owner.
After any consuming use, the whole binding rooting `p` is dead (partial moves kill the whole binding) [WIN-3]; any later use of a dead binding, and any write or `set` of a place projected, dereferenced, or subscripted from a dead root, is an error at the later use or target place.
`swap` is not a consuming use: it swaps the stored values of its two places, leaves both roots live, and leaves each root the sole owner of the value the other held [OP-11].
SET-1 rechecks its premises after its right-hand side under [LIV-1]; a dead binding is revived only by a [SET-1] commit whose target is that complete binding, which reinitializes it, and by nothing else.

[REF-1] A reference is a local name for a path.
A path starts at a local variable, a parameter, or a named const [CONST-2] and continues through field selections, `deref` (a reference's referent [TYPE-7]), an index step, a range step [REF-4], or an enum payload step [GRAM-5].
A payload step is available only under the refinement fact that the enum currently holds that variant, which a `match` arm establishes [ENT-3.S15] and which any write to the enum invalidates [REF-2].
A reference variable denotes the reference, and the storage it names is reached only through `deref` [TYPE-7]: every place expression, subscript, field selection, payload step, and measure read that goes through a reference variable `p` is written under that step — `deref(p)`, `deref(p).field`, `deref(part)[i]`, `deref(part).len`, and `deref(p).Some.value`.
`let q = p;` where `p` is a reference variable makes `q` a reference to the same path — an alias, not a copy of the referent — and passing a bare reference variable where a `&T` or `&[T]` parameter is expected passes that reference.
A reference variable names a path and is not storage of its own, so `&p` where `p` is a reference variable is a hard error citing REF-1 at that `borrow_expr`, with a repair [DIAG-1].
Resolving a place whose `deref` step names a reference variable replaces that step with the path that reference names, recursively, and every [OWN-7] judgment reads resolved places.
An index expression inside a path is evaluated when the reference is formed; the path records that value, and later assignments to the variables the expression used do not change it.
A `let` binder whose initializer is a `borrow_expr`, and a `let` binder whose initializer is a `value_if` or a `value_match` every arm of which delivers a reference, takes that reference kind — `&T` or `&[T]` — rather than a type [TYPE-5, GIVE-1], and is itself a reference variable.
A `set` whose target is a reference variable and whose right-hand side is a `borrow_expr` rebinds that name and writes no storage, so [SET-1]'s value-target judgment does not apply to it.
At a control-flow join, the join of a value initializer's delivering arms included, a reference variable's target is the union of the path sets its incoming edges may name, and every check on it must hold for every member of that set.
A loop-carried rebinding may extend its path through itself; its finite checking description is the loop-header summary below, not an enumeration of runtime path depths.
There is no permission marker on a reference; whether a callee may write through a reference parameter is stated by its effect row [EFF-1].

For each loop header and reference holder that a structurally continuing body path may rebind, retain one possible-location description per ultimate root. Seed it with the entering paths. A description is either the finite set of entering static path shapes with opaque captured offsets, or a known prefix R followed by an unknown descendant cover, written `R.**` in this specification only. The cover includes R and all its finite owned descendants; it is not writer syntax. Distinct roots are never dropped.
Join a contribution with the description for its root as follows. An incoming static shape already present in the entering set keeps that set; each potentially rebound index or range endpoint has a finite opaque identity distinguished by loop, holder, path and endpoint. Otherwise take the longest common definitely identical prefix of the existing alternatives and incoming cover and append `**`. Unequal or nonidentical captured selectors end that common prefix. A summary contains no nested `**`; further descent is absorbed by the cover. Once introduced, a cover's prefix may only shorten. A reset can add a root or shorten an anchor, and does not narrow the settled header.
Propagate entry and continuing rebinding contributions in source order, through aliases, joins and nested loops, until no header description changes. All final judgments use those settled descriptions. There are finitely many roots and headers, the static alternatives of one root can become a cover once, and a cover's finite prefix can only shorten; the procedure runs to completion without a pass count, timeout or acceptance budget. Validity and writability meet over all entering and executable backedges, with owner-distinct header dependencies solved simultaneously; an invalid alternative is never discarded because its path is covered. This is an ownership analysis, not iteration of [ENT-5]'s numeric facts or an inferred loop invariant.
A summarized description separately retains the referent type, every readonly restriction crossed at formation, and the identity of its current selected target. Each rebound header holder has a distinct target identity for the arbitrary iteration. Straight-line aliases snapshot that target and checked projections retain their finite relative suffix. Equal covers alone establish neither equal nor disjoint targets. A formation must still check every newly written payload selection, index, range and other partial operation; a cover provides no refinement or numeric fact. Loop exits retain their path-specific post-event state rather than restoring the entering target.

[REF-2] Reference validity is a fact.
"p is valid" is an ownership fact established where `p` is formed. It is invalidated when a proper prefix of its selected path is written, or when that path or a prefix is moved out of or released, by a statement, call [EFF-5] or compiler-derived release [STOR-3]; when its ultimate root leaves scope; or by the window-selection invalidations of [OP-10].
Formation checks the existence of the selected place. An already selected payload remains that place when its selecting arm ends; the disappearance of the lexical refinement fact alone does not invalidate its reference. Selecting that payload again still requires a current [ENT-3.S15] fact. Replacing its enum or any containing owner invalidates the existing reference normally. No selected-place witness publishes a variant or arithmetic fact.
Writing the storage at `p`'s path or below it is a content write and does not invalidate `p`.
For a summarized path, a potentially overlapping structural write invalidates its witness unless the written place is proved at or below its captured target, or disjoint from its cover. A write through one cursor therefore preserves that cursor and proved aliases, but invalidates unrelated possibly overlapping descendant cursors. An ordinary store to an existing primitive leaf [TYPE-1] cannot remove a selected path and preserves its witness; this exception excludes measures, window boundary operations, moves and releases. It does not preserve facts about the old contents: [ENT-5] still kills every overlapping support. Ancestor events above the cover's anchor count, and compound actions apply every member's invalidation; preservation by one write does not exempt a reference from another.
Using an invalid reference is a hard error citing REF-2 at the offending `place`, carrying the invalidating event and a repair [DIAG-1].
Prefix and overlap are judged by [OWN-7], conservatively: two indexed positions on one storage are taken to overlap unless proved distinct.
Validity is re-established only by forming the reference again, and a move never re-roots an existing reference: after `let w = move v;`, references formed from `v` are invalid and are not reinterpreted as references into `w`.
A reference-validity fact is an ownership judgment and is not a member of the entailment fragment [ENT-1].

[REF-3] References never escape.
A reference may be bound to a local and passed as a call argument, and may be used within the function that formed it or received it.
It may not be assigned into any aggregate [TYPE-8], may not be returned, and may not be captured by a function value that is stored or returned [FN-5].
A function that finds something returns an index or other owned data, with its bounds relation in `ensures` [FN-9], and the caller forms the reference.
A violation is a hard error citing REF-3 at the offending `expr`, with a repair [DIAG-1]; a `return_stmt` whose selected expression is a reference is that violation, and [FN-1] forms no candidate there.

[REF-4] Range references.
`&x[lo..hi]` forms a range reference over an indexable place or over another range reference, under the obligation `lo <= hi` and `hi <= x.len` submitted to [MSR-4].
Its parameter kind is `&[T]` [GRAM-2], which is a reference kind and not a type [TYPE-8].
Its one measure is `len`, equal to `hi - lo` [MSR-1].
It is never a stored value, never a result, and never a generic type argument.
Re-slicing is admitted: `&deref(part)[a..b]` under `a <= b <= deref(part).len`.
A range reference over a `Ring` is a hard error citing REF-4 at the complete `psuffix`, carrying a repair [DIAG-1], because a wrapped window is two extents and `&[T]` has one `len`.
A range reference dies with the bound that formed it exactly as any other reference does [REF-2].

[OWN-7] Overlap: resolved `p` overlaps resolved `q` iff one is a prefix of the other.
A resolved place is its root and the ordered steps below it — field selections, payload steps, index steps, and range steps [REF-1].
Two places fail to overlap exactly when some step of their common prefix provably selects two different storages: two field selections of different fields, two payload steps of one variant selecting different fields of that payload, two index steps whose offsets are proved distinct by the fixed [ENT-6] families under [MSR-4]'s disposition, two range steps proved disjoint by the four non-strict orderings stated below, or an index step and a range step proved apart by the three orderings stated below.
Two payload steps naming different variants of one enum select the same storage and therefore overlap.
Range steps [REF-4] extend this relation. Two ranges under containing paths that are identical step for step or differ only in index steps are disjoint when the current ProofContext proves either captured end no greater than the other's captured start, or either range empty. The fixed family submits all four non-strict orderings under [ENT-6]: `left.end <= right.start`, `right.end <= left.start`, `left.end <= left.start`, and `right.end <= right.start`; either empty-range ordering suffices because formation already proved start no greater than end.
An index step and a range step under containing paths that are identical step for step or differ only in index steps are disjoint when the current ProofContext proves the captured index before the range's captured start or at or after its captured end, or the range empty. The fixed family submits three orderings under [ENT-6]: `index < range.start`, `range.end <= index`, and `range.end <= range.start`.
Every subsequent step is relative to its containing range, so unequal subscripts below two different range steps, or below a range step and an index step, never establish separation by themselves. A proved separation of two containing ranges, or of an index from a containing range, separates all their descendants. Otherwise the complete range paths overlap conservatively.
The captured endpoints are immutable mathematical values, and assigning to a binding that supplied an endpoint cannot change an existing range or retarget a proof about it.
The relation is therefore over the complete path and not over one offset: `grid[k]` and `grid[i][j]` are decided at `k` against `i`, and two places that agree there overlap however their later steps read.
A window part is judged by [WIN-2]'s fixed answers, and a pair whose separation no admitted family discharges is overlapping.
A call's substituted effect paths are compared with one another in the pairs [EFF-5] selects, and against every live reference's path, by this relation.
For [REF-1]'s unknown-depth descriptions, a cover `R.**` overlaps R, every ancestor and every descendant. Two covers or a cover and an ordinary path are disjoint only when their known prefixes prove separation before the unknown tail. When both accesses share a captured current-target identity, compare their finite relative suffixes by the ordinary rules above. Different suffix fields below unrelated targets in one cover establish no separation. Equal-place, possible overlap and possible proper ancestry remain distinct judgments; an unknown tail cannot prove absence of a destructive prefix.

[OWN-8] Reject-when-unsure: the checker rejects any program it cannot prove conformant.
Rejection of a sound-but-unprovable program is not a defect; the diagnostic names the rule and carries a repair [DIAG-1].

[OWN-9] Non-normative consequence for the optimizer: for the duration of a live call, a place its substituted row declares `writes` of is reached only through the reference parameter whose entry names it, because [EFF-5] has proved it disjoint from every substituted effect another argument supplies, the effects one argument supplies are reached through that one parameter, and a statement that overlaps the call touches only storage [PAR-1] proved disjoint from the call's writes; a place that call only reads is read-only for that duration; an owned value is unaliased except by the references formed from it [REF-1].

[OWN-11] Loops: the body of an ordinary `loop_stmt` or a counted `for_stmt` is an ordinary block whose own bindings begin and end with one iteration.
A binding declared outside that body may be moved inside it, and the per-iteration judgment is [LIV-1]'s liveness agreement read at this loop's head: a binding declared outside the body whose live-or-dead status on the backedge differs from its status on the entering edge is a hard error citing OWN-11 at the loop, naming that binding, because one iteration would then start in a state the previous one did not leave.
A body that moves such a binding and reinitializes it before the backedge [SET-1] agrees and is admitted; a body that leaves it dead does not.
The backedge read here is the structural one [FN-1]: it carries the state the body reached whether or not the body's own fallthrough is executable, so a body that consumes such a binding and then leaves by `break` or `return` is judged on that state exactly as a body that falls through is.
A counted binder may be copied and may have a reference formed to it [REF-1], but it may not be moved and may not be passed to a callee whose row declares a write of it [EFF-1]; source writes are independently forbidden by [SET-1].
These restrictions are checked for each enclosing loop, so nesting never grants an outer binding to an inner body.

[OWN-13] Match ownership: a non-place expression scrutinee is an owned temporary (moved into the match).
Matching a place of own mode moves it (the binding dies; binders receive `own` payloads); matching through a reference leaves the scrutinee live and binds each payload as a reference naming the scrutinee path extended by that payload step [REF-1]. Its selected place follows [REF-2], while newly written payload selections require [ENT-3.S15]'s current refinement.
Binder modes are derived by this rule, stated once; they are not written.
Sibling binders are judged by [OWN-7].
A value initializer — a `let`-initializer `match` or `if` — binds its value from its arm or branch `give`s [GIVE-1]; scrutinee treatment and binder-mode derivation are unchanged, and each delivering arm or branch delivers a value of the binding's derived mode and type [GIVE-1, TYPE-5], so on the taken arm or branch an `own` result is moved exactly once (no double-move).

[LIV-1] Liveness is join-checked, and that is what makes every scope-exit release unconditional.
A binding's live-or-dead status is a property of a program point, not of a path: at every join of the conservative structural normal-control graph [FN-1] and at every loop head, every predecessor agrees on the live-or-dead status of every binding in scope.
A disagreement is a hard error naming the binding and the two disagreeing predecessors.
The loop-head instance is [OWN-11]'s per-iteration judgment, which owns its stated violation, reads the structural backedge stated there, and cites OWN-11 at the loop; every other join cites LIV-1 at the join and takes the predecessors that reach it.
This agreement is judged before any capability limit of a conforming checker reports an unsupported join, so a disagreeing predecessor pair is a source rejection and never a stop.
Because the status agrees at every join, whether a compiler-derived release runs on an edge leaving a scope is not runtime state: on every edge leaving a scope — a `break`, a `give`, a `propagate` error edge, the function-return edge, and a block exit — every binding of that scope that is live on that edge takes its compiler-derived release there, unconditionally, and a binding that is dead takes none [STOR-3].
Which release runs inside a live value may still be selected by that value's own discriminant, exactly as an enum's derived drop selects on its variant today.
A binding whose value is linear [PROV-6] takes no compiler-derived release on such an edge and is refused there instead, because the compiler never releases a linear value.
This rule states the liveness premise [SET-1] rechecks after a right-hand side, and the premise [OWN-11] reads at a backedge; it adds no scope-exit action and removes none.

[PROV-6] Linearity is a property of the type, closed under ownership.
A type is linear, lacking the drop capability [OWN-1], exactly when its declaration carries the `nodrop` modifier [GRAM-2] or it owns, at any depth, a linear type; a struct, an enum, an `Array`, a `Slots`, a `Ring`, or a `Box` [TYPE-9] owning a linear part is linear, and every other type is copy or affine by [OWN-1].
Linearity is a property of the type and not of a scope.
This rule refines [OWN-1]'s classification and replaces none of it: a copy value is never linear, and a value this rule does not make linear keeps exactly the disposition [OWN-1] and [STOR-3] give it.
A type owns its fields, its enum variant payloads, its `Box` content [TYPE-9], and the elements of an `Array`, a `Slots`, or a `Ring` it is.
A full `Array` has the same element-type ownership closure as a window [WIN-1]: if T is linear then `Array<T, N>` is linear, including when N is zero; a zero extent changes the executed element count, not this type-level judgment.
A written type argument is owned through the field, payload, or element position it lands in and never by the type that writes it.

The `nodrop` modifier is one optional atom on `struct_decl` and `enum_decl`, written in the place `nocopy` may be written instead [GRAM-2], and states a logical obligation, holding in every scope.
Both capability modifiers are admitted on every source struct or enum, including a fieldless struct or a tag-only enum: they remove the declared capabilities regardless of the capabilities its parts would otherwise grant [OWN-1].

A linear value leaves a scope by exactly two routes: moved out whole, or destructured whole [WIN-3].
An affine value has those two, plus the one compiler-derived release [STOR-3] carries on every leaving edge [LIV-1]; an affine value is released early by moving it into a function that consumes it and ends, and there is no release operation.
A window whose element type is linear is emptied element by element and its storage is then consumed by `free_empty` [OP-14].
A binding whose value is linear and which is live on an edge leaving its scope is a hard error citing PROV-6 at that edge's statement, naming the binding and the `nodrop` declaration or the written bound that made its value linear, and offering exactly the routes that remain.

`let N(f1: b1, ..., fk: bk) = move v;` [GRAM-4] is the destructuring consume: it consumes a value of nominal struct type `N` and binds declared fields of `N` in declaration order to fresh IDENTs.
`N` is a source `struct`.
Its field names are judged exactly as [GRAM-10] judges an `arm`'s: every field name it writes is written exactly once as `IDENT ":" IDENT` in declared order, a final `..` covers every declared field the form does not write, and a missing field name with no `..`, an extra, a repeated, a misspelled, or an out-of-order field name is a hard error citing GRAM-10 and `N`'s declared field list.
A field a final `..` covers is judged by [WIN-3].
Its binders are ordinary `let` binders of the enclosing block, fresh under [TYPE-6] exactly as [CALL-4]'s binder list's are.
Each binder receives its field's declared type and `own` mode [TYPE-5], the statement is one consuming use of `v` [OWN-1], and no residual of `v` survives it, so the statement derives no release of the consumed value's own storage [STOR-3].
An own-place `match` [OWN-13] is the enum form of the same destructuring.

The release graph of a type `T` has as its nodes the types reachable from `T` through fields, enum variant payloads, `Box` content, and `Array`, `Slots`, or `Ring` elements.
A runtime-capacity `Array<T>`, `Slots<T>`, or `Ring<T>` is reached in that graph only as the content of its `Box` [TYPE-9], so the `Box` is the leaf every owner's edge lands on and no runtime-capacity shape is ever a node of an owner other than its own cell.
A type's release action is non-empty by the least fixed point of two clauses: a `Box` [TYPE-9] is non-empty, and any type owning a non-empty type is non-empty [STOR-3].
The graph has an edge from a node to a sub-node exactly when that sub-node's release action is non-empty.
One walk performs the compiler-derived release, and it visits exactly the nodes of that graph in [STOR-3]'s order — every field of a struct in declaration order, an enum's active variant's payload selected by the discriminant, a cell's content before the cell itself, every element of an `Array` and every slot of a `Slots` or a `Ring` window [WIN-1] in ascending logical index order — freeing each `Box` cell after its content [STOR-8] and running each other non-empty node's release action.
A field, payload, or element whose release action is empty is never visited, and a container's elements are visited before its backing is released, so a release of a full container needs no emptiness premise.
A type whose release graph has a cycle makes that walk's depth a runtime quantity rather than a compile-time constant, and is admitted: the derived release of such a type is one release action per node type, entering itself where the graph closes, and the walk's depth is the value's own.
Every judgment of this rule that reads the graph reads each node once, which terminates on a cyclic graph and is exactly the node set this version's release actions need.

A consume of a proper sub-place of a value one of whose remaining parts is linear is a partial consume [WIN-3].
A partial consume is a hard error citing PROV-6 at the complete consumed `place`, naming the residual linear part, with a repair [DIAG-1].
The refusal is stated over the consume, so it reaches every consuming use of that sub-place [OWN-1].

A type parameter's bound is a capability filter on its argument [GRAM-2]: `T: copy` requires an argument that can be copied, `T: drop` requires one that can be dropped, and a parameter written with no bound requires nothing; every judgment of this rule inside that declaration's body reads the bound.
What the bound requires of the argument is what it grants the body: under `T: copy` the body may duplicate the value, use it bare, and drop it; under `T: drop` it may `move` it at most once and may drop it; with no bound it must consume it exactly once and may never drop it. These are the classes copy, affine, and linear of [OWN-1] read at the parameter, and they form the strict chain `copy < affine < linear`.
`copy` names [OWN-1]'s copy capability and `drop` the capability by which the compiler-derived release reclaims a value; a parameter with no bound also admits a type whose declaration carries `nodrop` or that owns a part that does.
A type parameter's bound is never inferred: an absent bound means no capability, and a type parameter carries at most one bound [FN-2, GRAM-2].
Satisfaction is the filter itself: `T: copy` accepts copy arguments only, `T: drop` accepts copy and affine arguments, and a parameter with no bound accepts every class.
An instantiation whose argument's class does not satisfy the written bound is a hard error citing PROV-6 at that instantiation's `call`, naming the parameter, the bound, and the argument.
A type parameter with no bound is linear at the one symbolic instance its body is checked at [FN-2], so a body that lets such a parameter's value reach a scope exit receives this rule's own not-consumed rejection there, naming the absent bound in place of a `nodrop` declaration.
The bound is a capability filter: it supplies no function-kind argument, selects no behavior, and creates no bound-satisfaction judgment other than this one [FN-2, FN-3].

The checked program retains, before lowering [DIAG-2], each type's linearity class, each release edge's release graph, each destructuring consume's binder list, and each declaration bound that was checked.

## 6. Storage

[STOR-1] Storage class is a function of type, stated once: `Box<T>` is heap-owned, one compiler-derived allocation released by one compiler-derived free at owner scope exit [STOR-3]; a constant-capacity `Array<T, N>`, `Slots<T, N>`, or `Ring<T, N>` is frame-resident, its slots inline in its owner or the stack frame; a runtime-capacity `Array<T>`, `Slots<T>`, or `Ring<T>` exists only as `Box` content [TYPE-9] and is heap-owned with that `Box`; a `const` item [CONST-2] is immutable static storage; every other owned value is frame-resident, inline in its owner or the stack frame.
There is no per-binding storage annotation and no default clause.
An `Array<T, N>` holds exactly N stride-spaced element representations in index order and stores no length, capacity, head, occupancy, or discriminant; a `Slots` or `Ring` stores its `len`, and a `Ring` its `head`, with the block [WIN-1].
A shape's concrete size, stride, padding, and zero-extent representation obey [STOR-6]; placing a shape inside another owner changes no element order or ownership.
Char and Unicode text are out-of-v0, recorded.

[WIN-1] A `Slots` or `Ring` is a run of `cap` slots whose initialized storage is a window: exactly the `len` slots beginning at `head` modulo `cap`, every other slot holding nothing.
`head` exists on `Ring` alone; on `Slots` the window begins at slot zero.
An `Array` has no window: every slot always holds a value, and its one measure `a.len` is its slot count [MSR-1].
No slot carries a tag, no occupancy bitmap, and no runtime discriminant; the window is the complete typestate, and no program point can observe a slot inside the window as empty.
`len` is a runtime number stored with the block [STOR-1] and is changed only by the operations of [OP-10] and [OP-13].
A subscript `r[i]` selects the element at logical offset `i` and carries [OP-4]'s obligation `i < r.len`, stated against `len` and never against `cap` or `head`; the storage it selects is slot `(r.head + i) mod r.cap`, which is the coordinate system [MSR-1] fixes.

[WIN-2] Besides its slots, a window has four named parts that paths and effect rows may name, all interpreted at call entry: `r.next` is the append slot at index `r.len`, `r.last` is the last filled slot at index `r.len - 1`, `r.filled` is every slot below `r.len`, and `r.free` is every slot from `r.len` up.
Overlap is fixed data for [OWN-7], read in the state where the two places are compared: an `r[i]` live there, `i < r.len` being proved there, never overlaps `r.next` or `r.free`; any other `r[i]` overlaps both; a range `r[lo..hi]` does not overlap `r.next` or `r.free` when `hi <= r.len` or `hi <= lo` is proved there, and overlaps both otherwise; every `r[i]` and every range overlap `r.last` and `r.filled`, because a row's `r.last` stands for every slot its callee's successive `take_back` calls reach, one `writes(r.last)` covering them all [EFF-2]; `r.filled` and `r.free` do not overlap; `r.next` is contained in `r.free`; `r.last` is contained in `r.filled`; the measure `r.len` is itself a write target and overlaps no slot.
A subscript formed in that state is live there by its discharged [OP-4] obligation, a range formed there ends at or below `r.len` there by its discharged [REF-4] obligation, and a reference into a window stays valid only while that bound holds [OP-10]; the place of a fact an [ENT-5] event may kill, and an index or range position an effect row takes from other arguments [EFF-5], are live, or end at or below `r.len`, exactly where [ENT-6] derives the bound in the entry state of that event or call.
Parts are vocabulary for effect rows and the overlap judgment only: no program forms a reference to a part, reads one, or writes one [TYPE-10], and the append slot is written only by `place_back` and `insert_at` [OP-10].
This vocabulary is ordinary: a user function may declare the same rows a built-in operation declares.

[WIN-3] There is no take operation and no hole.
A move out of a field or out of `Box` content consumes the whole owner: the owner ceases to exist, its other affine parts take their compiler-derived release [STOR-3], and a remaining linear part is a hard error citing WIN-3 at the complete consumed `place`, with a repair [DIAG-1].
A destructuring consume binds the fields it names and covers the rest with `..` [GRAM-4], and an own-place `arm` does the same [GRAM-10].
A move out of a window slot or an array element is a hard error citing WIN-3 at that `place`, with a repair [DIAG-1].
Assigning over any owned place releases the old value when it is affine and is a hard error citing WIN-3 at the target `place` when it is linear.
At scope exit the compiler releases the slots inside the window recursively and frees the block; an `Array` releases every slot.
No operation releases a linear element: a storage whose element type is linear is itself linear [PROV-6] and the program must take every element out and consume it, and then, with the storage proved empty, call `free_empty` [OP-14].
That one route also consumes a `Box` whose content is such a window, freeing the cell with it [OP-14].

[STOR-7] Any value may be relocated by copying its bytes, because no value contains a reference [TYPE-8, STOR-5].
No judgment of this specification depends on a value's address being stable, and no accepted program can observe one.

[STOR-8] There is one heap, provided by the trusted base and internally synchronized.
Allocation is total in the source: it never returns a failure, never traps, and no allocating operation carries a `Result`.
Exhaustion of the heap terminates the program from the trusted base, outside the language [SCOPE-3], so no payload is ever handed back and no program point holds a value whose owner has vanished.
The arithmetic that computes an allocation size carries the static overflow obligation [OP-9].
Addresses are not observable, so allocator concurrency does not affect program determinism.
Allocation and release carry no effect entry [EFF-1] and never prevent two statements from overlapping [PAR-1].
A source bundle that carries the no-heap declaration [GRAM-2, PROG-3] cannot name `Box` or the runtime-capacity shapes [TYPE-9] and cannot call an allocating prelude row — `box_new`, `box_array_filled`, `box_slots_new`, `box_ring_new`, and `grow` [OP-13, OP-10]; naming such a type is a hard error citing STOR-8 at the complete `type`, and calling such a row is a hard error citing STOR-8 at the complete `call`, each with a repair [DIAG-1].
A module program entry whose `no_heap` states the requirement [MOD-9] withdraws the heap from that entry's execution closure: the entry function and every function its checked body reaches through its calls, in every branch, each as the concrete instance the call selects [FN-6]; an instance's body calls the function-kind actuals it names, and an erased proof annotation calls nothing.
A function of that closure uses the heap on its own when its body calls an allocating prelude row, or when the concrete type of one of its parameters, its results, or a value its body evaluates, binds or releases holds a `Box` or a runtime-capacity shape [TYPE-9] as itself, a field, a payload field or an element, private fields included, since releasing such a value frees heap storage [STOR-3]; it requires the heap when it uses it on its own or calls a function that requires it.
A component of the closure's call graph introduces the requirement when its functions require the heap and no function of another component that they call does; the first such component a breadth-first walk from the entry reaches is a hard error citing STOR-8 at the declaration of its first function in that walk that uses the heap on its own, reporting the call path from the entry. Definitions the closure does not reach impose nothing on the entry.

[STOR-3] Deallocation and resource release are compiler-derived and explicit in the checked program [DIAG-2]: every release is represented before lowering.
Release actions run on every source control-flow edge that leaves their owner scope, in reverse declaration order; [FN-10] places a guaranteed self-tail transfer's releases before that transfer.
Host termination caused solely by unavailable external resources under [SCOPE-3] is not a Whitefoot control-flow edge, and this specification makes no source-level cleanup promise for that case.
No reference counting.

Every edge that leaves one entered `for_stmt` body normally — its fallthrough, a `break` resolved to that counted loop or an enclosing loop, a `return`, or a `propagate` error edge — carries exactly once every compiler-derived release for the body scopes that edge leaves, innermost scope first and in reverse declaration order within each scope.
On body fallthrough those actions complete before the hidden counted update [FN-1].
The header's false edge never enters the body and therefore carries no body-scope cleanup.
An external-resource termination under [SCOPE-3] likewise creates no source edge on which these actions could run.
No exit duplicates an action already carried by an inner scope edge.

The release action of a type is compiler-owned semantic data selected by that type, not a fixed enumeration of memory-reclamation actions.
Which components of a value that action visits, and in which order, is [PROV-6]'s release graph and its one walk; the per-type actions below are the leaves that walk runs.
A `Box<T>` release is its content's compiler-derived release followed by one compiler-derived heap free.
An `Array` release is each element's compiler-derived release in ascending index order and no storage reclamation of its own.
A `Slots` or `Ring` release is each element's compiler-derived release over its window in ascending logical index order and no storage reclamation of its own.
A `const` item [CONST-2] is never released.
Every other frame-resident owned value [STOR-1] has no release action.

A host handle [PRE-2] has no fields, so its release is empty; every other opaque struct [TYPE-2] takes the release its fields give it under this rule, `Box` the cell case above.
An opaque struct's `nodrop` modifier, and only the ordinary ownership closure of [PROV-6], requires explicit consumption.
No source declaration, annotation, attribute, contract, or binding attaches a finalizer or any other user-defined action to a value's release.

A successful [SET-1] assignment derives no finalizer or cleanup edge and no release beyond the old value's own: a copy target's previous value needs none, and an affine target's previous value takes the release [WIN-3] states.

[STOR-5] Storage is reference-free.
No struct field, enum variant payload, element of any shape, `Box` content, or written generic type argument is a reference kind [TYPE-8].
The `field`, `vfield`, and `targ` grammar admits only `type`, and `type` has no reference production [GRAM-3], so no source writes a reference kind in a stored or type-argument position at all; the semantic check is recursive after substitution and is therefore the closing clause for a substituted generic instance alone, such as `Box<Option<&T>>` or a generic field instantiated with a reference kind.
A violation is a hard error citing STOR-5 at the complete contained `type`, with a repair [DIAG-1].

Consequently reference provenance cannot hide in a stored or generic payload.
No reference leaves a callee at all [REF-3].

[STOR-6] Concrete target layout is a target-stage obligation after complete source-semantic acceptance and monomorphization.
It neither supplies nor replaces a source type-formation, ownership, recursion, or monomorphization judgment; an unavailable earlier semantic judgment remains an unsupported compiler capability rather than becoming a target-layout failure.
For this rule, the selected target is the exact backend target and ABI fixed by the compiler executable together with its invocation options; it is not a source declaration, inferred source fact, or component of the [PROG-2] compilation-unit identity.
A target-independent checked program and target-independent IR may precede this obligation.
Before emitting any form whose object layout, allocator ABI, or address arithmetic depends on a target, the compiler must have selected that exact target and ABI.

For every concrete representation and compiler-generated target object in the ordinary facts-off target-stage materialization set after monomorphization and before optional optimization, the compiler computes the representation's size, alignment, field or payload offsets, element stride, and padding under that target's ABI using checked mathematical arithmetic.
The target-object calculation includes the complete size, alignment, and offsets of statics, complete stack frames and their slots or temporaries, and call/return ABI objects.
A semantics-preserving omission made by ordinary facts-off lowering creates no target materialization to check; optional optimizer facts and facts-on dead-code elimination may not shrink the established set or change target-layout success.
Each result is checked against the actual allocation, ABI, and address-index domain in which that object or value will be used.
If a required result is not representable, target compilation stops before emitting target-dependent output that contains the materialization.
It must not wrap, truncate, underallocate, reduce alignment, change [STOR-1] storage class, emit an unrepresentable materialization on the assumption that an optimizer will erase it, or continue into target address formation.
This stop is a target-layout failure under [DIAG-1], not a source-language rejection, and cites no language rule.

For a runtime-sized allocation, the concrete descriptor and element layout are checked statically as above.
For every runtime-capacity shape materialized by a construction function [OP-13] or resized by `grow` [OP-10], target qualification additionally verifies the actual size, alignment, and element stride against [OP-9]'s language ceilings before lowering the operation.
The accepted [OP-9] judgment retains a numeric upper bound for the source length at that allocation site; target qualification computes the complete allocation size, including the shape's descriptor, its padding before the elements, and that bound multiplied by the actual target stride, using checked mathematical arithmetic, and requires the result to fit both the allocator-parameter and address-index domains before lowering the operation.
At this target stage, when the actual element stride is positive, the exact SSA result of a runtime-capacity shape's `len` measure additionally carries the selected target's runtime-allocation byte maximum minus that shape's padded descriptor size, divided by its actual element stride and rounded down, because every materialized shape already satisfies the successful-allocation representation invariant.
When that stride is zero, the invariant contributes no additional count bound beyond the source length type; the complete padded descriptor must still satisfy target qualification, and every actually emitted address operand still obeys the exact-representation requirement below.
Qualification may intersect this target bound with the retained source bound only for that exact SSA result; it does not publish a Whitefoot comparison fact or transfer the bound through a block parameter, storage load, conversion, user call, or another value merely because its source spelling or type is similar.
The source allocation proof and this target qualification jointly establish that every reachable runtime byte count has one exact value-preserving target representation; neither alone authorizes emission, and the allocator receives exactly that value.
Every emitted target address computation must likewise be proved valid for every runtime value that reaches it: the compiler establishes before emission that each runtime index and each mathematically scaled byte offset actually used by the computation has an exact value-preserving representation in the applicable target address-index domain, and that scaling and offset addition do not wrap.
An [OP-4] bounds judgment together with an established complete-object-layout or successful-allocation invariant may discharge these obligations; a backend's implicit narrowing does not.
If target qualification cannot establish one of these facts, target compilation stops before emitting the governed allocation or address operation.
This stop is a target-layout failure, not a source rejection, [OP-4] bounds failure, runtime proof outcome, or resource-availability failure; no target-domain runtime guard is emitted.

Complete generated frames remain subject to the mandatory checked-representability judgment above.
That judgment does not predict available stack capacity: available capacity depends on dynamic call depth, recursion, the caller, and the execution environment.
The language therefore defines no numeric per-array, per-object, or per-function frame ceiling.
A tool or selected target may stop compilation for its own conservative frame-capacity or resource limit as a non-language target/resource failure [DIAG-1], but that optional limit does not replace the mandatory representability judgment.
Exhaustion during execution is inside the compiler/runtime/OS TCB boundary [SCOPE-3]: it adds no source effect or proof fact and authorizes no hidden heap promotion.

## 7. Operations

[OP-1] Every computation is a call naming one operation from the operation table; one operation per (semantic operation × mode); nothing is overloaded.
The table below is the normative inventory (columns: op, type domain, signature, effects).

```wf-ops
| op | domain | signature | effects |
|---|---|---|---|
| `+wrap` `-wrap` `*wrap` | all int T | `(T, T) -> own T` | pure |
| `+` `-` `*` | all int T | `(T, T) -> own T` | pure |
| `+defined` `-defined` `*defined` | all int T | `(T, T) -> own Bool` | pure |
| `+checked` `-checked` `*checked` | all int T | `(T, T) -> own Result<T, Overflow>` | pure |
| `/` `%` | all int T | `(T, T) -> own T` | pure |
| `/defined` `%defined` | all int T | `(T, T) -> own Bool` | pure |
| `/checked` `%checked` | all int T | `(T, T) -> own Result<T, DivError>` | pure |
| `ineg.wrap` | signed int T | `(T) -> own T` | pure |
| `ineg` | signed int T | `(T) -> own T` | pure |
| `ineg.defined` | signed int T | `(T) -> own Bool` | pure |
| `ineg.checked` | signed int T | `(T) -> own Result<T, Overflow>` | pure |
| `==` `!=` `<` `<=` `>` `>=` | all int T | `(T, T) -> own Bool` | pure |
| `eeq` `ene` | one exact nominal tag-only enum T (every variant nullary), including `Bool` | `(T, T) -> own Bool` | pure |
| `fadd.strict` `fsub.strict` `fmul.strict` `fdiv.strict` | f32 f64 | `(T, T) -> own T` | pure |
| `feq` `flt` `fle` `fgt` `fge` `fne` | f32 f64 | `(T, T) -> own Bool` | pure |
| `band` `bor` `bxor` | Bool | `(Bool, Bool) -> own Bool` | pure |
| `bnot` | Bool | `(Bool) -> own Bool` | pure |
| `cvt` | all numeric pairs [OP-6] | `(Src) -> own Dst` | pure |
| `cvt.checked` | all numeric pairs [OP-6] | `(Src) -> own Result<Dst, NarrowError>` | pure |
| `cvt.defined` | all numeric pairs [OP-6] | `(Src) -> own Bool` | pure |
| `iand` `ior` `ixor` | all int T | `(T, T) -> own T` | pure |
| `inot` | all int T | `(T) -> own T` | pure |
| `ishl.wrap` `ishr.wrap` | all int T | `(T, u32) -> own T` | pure |
| `ishl` `ishr` | all int T | `(T, u32) -> own T` | pure |
| `ishl.defined` `ishr.defined` | all int T | `(T, u32) -> own Bool` | pure |
| `irotl` `irotr` | all int T | `(T, u32) -> own T` | pure |
| `ipopcount` `iclz` `ictz` | all int T | `(T) -> own u32` | pure |
| `ibswap` | int T, width>=16 | `(T) -> own T` | pure |
| `imulhi` | all int T | `(T, T) -> own T` | pure |
| `+sat` `-sat` `*sat` | all int T | `(T, T) -> own T` | pure |
| `imin` `imax` | all int T | `(T, T) -> own T` | pure |
| `iabs.wrap` | signed int T | `(T) -> own T` | pure |
| `iabs` | signed int T | `(T) -> own T` | pure |
| `iabs.defined` | signed int T | `(T) -> own Bool` | pure |
| `iabs.checked` | signed int T | `(T) -> own Result<T, Overflow>` | pure |
| `reinterpret` | equal-width primitive pairs: i8<->u8, i16<->u16, i32<->u32, i64<->u64, {i32,u32}<->f32, {i64,u64}<->f64 | `(Src) -> own Dst` | pure |
| `fneg` `fabs` | f32 f64 | `(T) -> own T` | pure |
| `fcopysign` | f32 f64 | `(T, T) -> own T` | pure |
| `fmin` `fmax` | f32 f64 | `(T, T) -> own T` | pure |
| `ffloor` `fceil` `ftrunc` `froundeven` | f32 f64 | `(T) -> own T` | pure |
| `frem` | f32 f64 | `(T, T) -> own T` | pure |
| `fsqrt.strict` | f32 f64 | `(T) -> own T` | pure |
| `ffma.strict` | f32 f64 | `(T, T, T) -> own T` | pure |
| `finf` `fnan` | f32 f64 | `() -> own T` | pure |
```

Let `DotlessOperationNames` be exactly the set of distinct individual operation spellings enumerated in this rule's normative `op` column whose complete spelling satisfies IDENT and contains no dot.
Let `ModeWords` be exactly the suffix alternatives in FORM-3's active OPNAME formation rule together with the operator-form suffixes of [GRAM-1]; in this version the two carriers share one closed set, `{wrap, defined, checked, sat, strict}`.
`ReservedLowerNames` is exactly `DotlessOperationNames` union `ModeWords`.

Each distinct complete spelling in the operation table declares one operation-family identity.
An OPNAME callee resolves to its exactly spelled operation family.
An `infix_op` or `compare_op` token resolves to its exactly spelled operation by the operator table row; infix resolution consults no name domain, and an operator token is never a declaration, callee IDENT, or OPNAME.
An IDENT callee whose spelling belongs to `DotlessOperationNames` resolves to that operation family; every other IDENT callee admits an in-scope raw function-kind parameter [FN-5], a top-level source `fn_decl`, or a PRE-1 function.
Absence from the selected operation-family, function inventory is a hard error citing OP-1.
Later typed operation checking uses the operand domains and, for the retained-argument operations [TYPE-5], the written arguments, to select the applicable row within the resolved family.
Operand types never select between an operation family and a function.
A bare `place` operand that a table-operation row reads without consuming — the base place of a subscript — is a non-consuming read: it neither moves nor partially consumes an affine root [OWN-1], exactly the reading [FN-8] already states for a place used as a non-consuming operand of an admitted table operation.

No source declaration or FN-9 result-datum candidate in this closed list may use a member of `ReservedLowerNames`: the IDENT of `fn_decl`; the IDENT of `const_decl`; every `param` and `result_binding` IDENT; every `let_stmt` IDENT, including ordinary, propagate, value-match, and value-if lets; every `for_binding` IDENT; every `contract_define` IDENT; the second IDENT of any `fieldbind`, including a `result_route` payload binder; and every `field` and `vfield` IDENT.
Such a reserved spelling is rejected citing exactly FORM-3 before freshness ownership is considered.
Dependent field declarations participate in this pre-resolution reservation inventory even though their owner/member duplicates remain deferred.
No other declaration role is covered: type-generic TYPEIDs, const-generic IDENTs, LABELs, interface-member `fn_sig` IDENTs, and the IDENTs of `header_invariant` and `invariant_stmt` remain outside this prohibition.
Dotted OPNAMEs cannot be declarations under the grammar.
This reservation keeps operation-versus-function resolution context-free [META-2] and keeps a field-access place from maximal-munching as OPNAME [FORM-3].

[OP-2] Integer value semantics are defined over mathematical integers and fixed-width bit strings, never host-language overflow or undefined behavior.
The closed integer-type set is `i8 i16 i32 i64 u8 u16 u32 u64`.
For `iK`, where K is 8, 16, 32, or 64, the value set is `[-2^(K-1), 2^(K-1)-1]`; for `uK` it is `[0, 2^K-1]`.
Let `M = 2^K`.
For any mathematical integer z, let u be the unique integer satisfying `0 <= u < M` and `u ≡ z (mod M)`.
Define `wrap_uK(z) = u`; define `wrap_iK(z) = u` when `u < 2^(K-1)`, and `wrap_iK(z) = u - M` otherwise.

The bare infix spellings `+ - * / %` and dotless spellings `ineg iabs ishl ishr` are the sole proof-required exact integer operations.
Every occurrence carries one canonical [ENT-6] integer-domain obligation equal to the corresponding total domain-query expression over the same selected type and exact operand-expression identities.
The checker accepts the occurrence only when the complete state discharges that goal; a refuted or unproved goal is a compile-time OP-2 rejection at the `infix` or `call` node.
A contradictory state discharges it under [ENT-4].
No runtime test, fallback check, trap site, or checked-result conversion is synthesized; the discharged obligation may be stated to the backend as a proved fact under [DIAG-2].
After discharge, the exact operation executes without a guard and returns the result fixed below.

For a common selected type T, the domain queries have these exact total Bool values:

```text
a +defined b  iff mathematical(a + b) belongs to T
a -defined b  iff mathematical(a - b) belongs to T
a *defined b  iff mathematical(a * b) belongs to T
n /defined d  iff d != 0 and (T is unsigned or n != MIN(T) or d != -1)
n %defined d  iff d != 0 and (T is unsigned or n != MIN(T) or d != -1)
ineg.defined(x) iff x != MIN(T)
iabs.defined(x) iff x != MIN(T)
ishl.defined(x, k) iff k < K
ishr.defined(x, k) iff k < K
```

Each domain query is pure, total, and returns `own Bool`; it does not execute the corresponding exact operation.
An executed branch condition, proved requirement, proved invariant, or verified postcondition may establish its canonical goal through [ENT-3].
Merely computing the Bool value without an admitted fact source establishes nothing.
The `.trap` spellings and hidden named aliases such as `iadd.trap` do not derive and are not compatibility names.

After discharge, exact add, subtract, and multiply return their mathematical result, which the obligation proves belongs to T.
Exact division is truncating toward zero and exact remainder satisfies `n = (n / d) * d + (n % d)` with the remainder having the dividend's sign or being zero; their obligation excludes both zero divisor and the signed `MIN(T), -1` pair for division and remainder alike.
Exact `ineg` returns mathematical `-x`, and exact `iabs` returns the nonnegative mathematical absolute value; their obligation excludes the signed minimum.
Exact `ishl` is the fixed-width left shift and may discard high bits, exact signed `ishr` is arithmetic, and exact unsigned `ishr` is logical; their sole domain condition is `k < K`.

The wrap forms return `wrap_T(z)` for add, subtract, multiply, and negation; `iabs.wrap(MIN(T))` returns `MIN(T)`.
`ishl.wrap` and `ishr.wrap` mask the amount to `k & (K-1)`.
The checked add, subtract, multiply, negation, and absolute forms return `Ok(value: z)` when their exact mathematical result belongs to T and otherwise `Err(error: Overflow())`.
`/checked` and `%checked` return `Err(error: DivideByZero())` for zero divisor, `Err(error: DivOverflow())` for the signed minimum/-1 pair, and `Ok(value: result)` otherwise.
The existing saturating forms clamp exactly as [OP-8] fixes.
There is no wrap division or remainder because divisor zero has no modular quotient or remainder.

For `==`, `!=`, `<`, `<=`, `>`, and `>=`, both operands denote their mathematical values in T and the result is respectively `True()` exactly when `a=b`, `a!=b`, `a<b`, `a<=b`, `a>b`, or `a>=b`.
Ordering on signed T is signed mathematical ordering and on unsigned T is unsigned ordering.
All six comparisons are pure, total, Bool-valued operations.

Every operation above derives its selected type from its operands and carries no written type argument.
Operands that are specified as common-T must have one identical exact closed integer type or one live `Int`-bound generic type later concretized by [FN-2]; agreement never widens, converts, or consults an expected result.
The unary exact, defined, wrap, and checked negation and absolute families require the signed subset.
Shift values have selected type T and their amounts have exact type `own u32`.
A wrong argument kind or count, written argument, invalid concrete or generic domain, or unsigned negation/absolute cites OP-1; after TYPE-7 exclusivity, an exact-type mismatch cites TYPE-5 at the offending operand.
The table result type is exact, and the containing construct owns any later mode or result mismatch.

Mode membership is table data: add/subtract/multiply have exact, defined, wrap, checked, and sat; divide/remainder have exact, defined, and checked; negate/absolute have exact, defined, wrap, and checked; shifts have exact, defined, and wrap.
All these rows are pure.

[OP-3] Float ops that ROUND carry `.strict` (IEEE 754, no reassociation, no contraction): `fadd.strict` `fsub.strict` `fmul.strict` `fdiv.strict` `fsqrt.strict` `ffma.strict`.
Float ops that are EXACT or exact-selection are dotless: `fneg` `fabs` `fcopysign` `fmin` `fmax` `ffloor` `fceil` `ftrunc` `froundeven` `frem` and the six comparisons.

[OP-4] A subscript `p[i]` selects one element place of an indexable base: the base place `p`'s final selected type must be `Array<T, N>`, `Array<T>`, `Slots<T, N>`, `Slots<T>`, `Ring<T, N>`, `Ring<T>`, or the run of T elements a range reference `&[T]` names [TYPE-9, REF-4], a runtime-capacity form and a range reference alike being reached through `deref` [TYPE-7], and the subscripted place's selected type is exactly that element type T — derived from the base place's already-fixed type [TYPE-5] — written where the binding carries an annotation, derived at a body `let` — by the same declared-type selection that types a field suffix, never from expected type or cross-statement inference; a subscript whose base's final selected type is not one of those indexable types is a hard error citing OP-4 at that subscript's `psuffix` node.
A `const` item whose type is `Array<T, N>` is indexable on the same terms [CONST-2].
The subscript carries the bounds obligation `i < p.len` [ENT-6], and `i` is a logical offset whose storage slot [WIN-1] fixes, so the obligation is against `p.len` for every indexable base and never against `p.cap`.
The injectivity sentence of [MSR-1] is what carries that logical conclusion to a storage conclusion, and its premise `p.len <= p.cap` is one of [MSR-2]'s standing facts, so no subscript occurrence submits a separate obligation for it.
The obligation is submitted to the one numeric goal disposition [MSR-4]; that rule fixes the complete ordered derivation and this rule grants no route of its own.
A discharged subscript reads or writes with no runtime bounds check in every build mode, and its checked-program disposition records the discharging derivation [DIAG-2].
A subscript whose current ProofContext does not discharge the obligation is a compile-time rejection citing OP-4 at that subscript's `psuffix` node, carrying the residual obligation rendered exactly per [ENT-6], and publishes no checked program.
Discharge is a deterministic checker derivation [ENT-1]; a solver result never participates.
A runtime-capacity shape's obligation is over the runtime length term.
The offset atom has exact value mode and type `own u64`; after the [TYPE-7] implicit-read exclusivity, any other offset mode or type is a hard error citing OP-4 at the offset `atom` node, with `SourceCoordinate` equal to that atom's complete checked half-open source extent.
A subscript in a [SET-1] target forms the selected place without reading its stored value; its base and offset are evaluated during target evaluation, and its discharge judgment is identical in target position.
A successful bounds judgment neither narrows nor authorizes narrowing the offset or its scaled byte offset; target address formation additionally obeys [STOR-6].

[OP-5] Every source condition and contract predicate requires its selected expression to have exact value mode and type `own Bool`, where `Bool` is the PRE-1 nominal type.
No integer, other enum, or implicit truthiness conversion is admitted [TYPE-4].
The implicit-read case already owned by [TYPE-7] is exclusive: when `e` uses a reference binding where its referent `Bool` value would be required, that use is rejected citing TYPE-7 and OP-5 forms no candidate.
Every other exact-mode or exact-type failure is a hard error citing OP-5 at the selected `expr` node, with `SourceCoordinate` equal to that expression node's complete checked half-open source extent.
An `if` condition is executed control flow [GRAM-6], while a contract predicate, invariant relation, and `proof_use` are erased proof syntax [FN-8, FN-9, INV-1, PRF-1].
This judgment alone creates no runtime check or effect.

[OP-6] Exact numeric conversion has one partial value function `C(Src,Dst,x)` and one total Boolean domain predicate `D(Src,Dst,x)` over all 100 ordered pairs of the ten numeric primitives.
Both endpoints are written type arguments [TYPE-5]; an endpoint may be a numeric primitive or a symbolic type parameter whose [FN-2] bound is `Int` or `Float`.
The following disjoint rows define the domain and the selected value completely.

| Pair and input | D is true exactly when | C on the domain |
|---|---|---|
| Same numeric type | Always | The complete input representation, including a float's NaN payload and sign and its zero sign |
| Distinct integer types | The mathematical integer is in the destination's closed range | That integer in the destination type |
| Integer to float | The mathematical integer is exactly representable in the destination format | That value; integer zero selects positive zero |
| Float to integer | The input is finite, integral, and in the destination's closed integer range | That mathematical integer; either zero sign selects integer zero |
| Distinct float formats, finite input | The value is exactly representable in the destination format | That value with the input's zero sign preserved |
| Distinct float formats, infinity | Always | Infinity with the input's sign |
| Distinct float formats, NaN | Always | The destination's canonical quiet NaN [OP-8] |

The interface spelling selects the behavior independently of the type pair:

| Interface | Obligation and result |
|---|---|
| `cvt::<Src, Dst>(x)` | Carries the canonical ConversionDomain goal `cvt.defined::<Src, Dst>(x)`; after [ENT-6] discharges it, returns C |
| `cvt.checked::<Src, Dst>(x)` | Returns `Ok(value: C)` when D is true and `Err(error: NarrowError())` when D is false |
| `cvt.defined::<Src, Dst>(x)` | Returns D |

A whole-type total pair has D true for every source value.
The 39 such pairs are exactly all ten identity pairs; `iN->iM` and `uN->uM` for N<M; `uN->iM` for N<M; `{i8,i16,u8,u16}->f32`; `{i8,i16,i32,u8,u16,u32}->f64`; and `f32->f64`.
The other 61 pairs have value-dependent domains.
For symbolic endpoints, whole-type totality requires every pair admitted by their finite bound domains to be whole-type total: each occurrence of the same type parameter receives the same primitive, and distinct type parameters vary independently.
This judgment inspects at most the 100 ordered primitive pairs and invents no numeric capability for an unbounded parameter.
A refuted or unproved bare conversion is rejected at its `call` node citing OP-6 and rendering its canonical domain goal [MSR-4].

[OP-7] Operation-name convention.
An arithmetic, logic, bit, or compare op carries a domain prefix — `i` (integer), `f` (float), `b` (Bool logic), or `e` (tag-only enum comparison, including `Bool`) — whether or not a cross-domain twin exists; the structural ops (`cvt`, `reinterpret`) carry no prefix.
The integer arithmetic and integer comparison symbols of [GRAM-5] are the one prefix-free operation class: each is an integer-only table row, so `+` and `<` never denote a float or enum operation, and `fadd.strict`, `feq`, and `eeq` keep their prefixed names.
`Bool` participates in the `b` family for boolean logic and the `e` family for tag-only equality; the operation name, not operand inference, selects the family.
A respelled operation's token is its one constant spelling under the same one-spelling-per-operation discipline.
Bare infix and dotless named integer spellings, and bare `cvt`, are proof-required exact operations; `.defined` is the distinct total Bool-valued domain query, not a result mode and not an execution of the partial primitive.
The total value-result policies remain `.wrap`, `.checked`, and `.sat` where [OP-1] lists them, and float `.strict` is unchanged.
Signedness-parametric lowering keyed on the operand-derived selected type [OP-2] (`ishr` is `ashr` for signed T and `lshr` for unsigned T; `imin` is `smin` or `umin`) is the same discipline as the `<` = `slt`/`ult` row, not overloading.
Nominal enum identity is likewise checked from the operand-derived selected type before `eeq`/`ene` lowering; equal representation width never makes distinct enum types interchangeable.

[OP-8] Edge semantics and confirmed lowerings for the operations added in this revision; every totality edge is closed here as table data, so no added row is writer-reachable poison.
`iand`/`ior`/`ixor` lower to `and`/`or`/`xor` and `inot` to `xor x, -1` (total).
A shift or rotate amount is `u32`; `ishl.wrap`/`ishr.wrap` mask the amount to `amt & (width-1)` and are total, exact `ishl`/`ishr` execute an ordinary shift only after [OP-2] proves the amount smaller than the width, `ishr` is `ashr` for signed T and `lshr` for unsigned T, and `irotl`/`irotr` lower to `llvm.fshl`/`llvm.fshr` whose amount is taken modulo width, so rotates are total.
`ipopcount` is `llvm.ctpop`; `iclz`/`ictz` are `llvm.ctlz`/`llvm.cttz` with is-zero-poison false, so a zero input returns the bit width (the zero-input fix); counts return `u32`.
`ibswap` is `llvm.bswap` (width a multiple of 16).
`imulhi` is the high half of the full double-width product.
`+sat`/`-sat` are `llvm.sadd.sat`/`uadd.sat` or `ssub.sat`/`usub.sat` clamping to T's range; `*sat` widens, multiplies, and clamps, which avoids the signed-saturation miscompile in `llvm.smul.fix.sat`.
`imin`/`imax` are `llvm.smin`/`umin` or `smax`/`umax`.
`iabs.wrap`, exact `iabs`, and `iabs.checked` use `llvm.abs` with is-int-min-poison false; `.wrap` returns `iK::MIN` on that edge, exact `iabs` is emitted only after its domain proof excludes the edge, and `.checked` returns `Err(Overflow())` there.
Every arithmetic `.defined` query computes only its total comparison or overflow predicate and never executes the corresponding exact primitive.
An admitted bare `cvt` lowers without a validity guard; the result transformations required by [OP-6] remain part of its value semantics. A checked conversion and a conversion-domain query may evaluate total conversion primitives while deciding their answer, and may evaluate a partial primitive only on a path where its domain holds.
`reinterpret` is the LLVM bitcast instruction for cross-domain pairs (int<->float; bit-preserving, all NaN payloads and sign bits preserved) and an identity bit-relabel for same-width int<->int resign (i8<->u8, i16<->u16, i32<->u32, i64<->u64); it is the bit-preserving counterpart of value-preserving `cvt`, giving bit-level resign a home distinct from cvt's value-preserving resign.
`fneg` is the LLVM fneg instruction (a sign-bit flip, not `fsub(0.0, x)`); `fabs` is `llvm.fabs`; `fcopysign` is `llvm.copysign`.
`fmin`/`fmax` are `llvm.minimum`/`llvm.maximum` (IEEE-2019, NaN-propagating, negative zero ordered below positive zero, deterministic); `llvm.minnum`/`maxnum` are not used, because their signed-zero tie result is unspecified and breaks the reproducibility FORM-1 requires.
`ffloor`/`fceil`/`ftrunc` are `llvm.floor`/`ceil`/`trunc` (roundToIntegral, staying in the float type); `froundeven` is `llvm.roundeven` (ties-to-even, matching `fadd.strict`).
`frem` is the LLVM frem instruction (the C `fmod`: remainder with the dividend's sign, truncated quotient, exact), a distinct operation from IEEE `remainder`.
`fsqrt.strict` is `llvm.sqrt` and `ffma.strict` is `llvm.fma` (single-rounding fused, distinct from the contraction [OP-3] forbids; a correctly-rounded libcall on hardware without an FMA unit).
The comparisons `feq`/`flt`/`fle`/`fgt`/`fge` are ordered (`fcmp o*`, false when either operand is NaN) and `fne` is unordered (`fcmp une`), so `fne` equals `bnot(feq)` on every input and `fne(x, x)` is true exactly when x is NaN.
`finf` is the positive-infinity value (negative infinity is `fneg(finf::<T>())`) and `fnan` is the canonical quiet NaN; other NaN payloads are reachable through `reinterpret`.
For a tag-only enum T — the operand-derived selected type [OP-2] — `eeq(a, b)` is `True()` exactly when `a` and `b` denote the same declared variant of that nominal T, and `ene(a, b)` is its exact boolean complement.
Both operands must have that exact T, derived by [OP-2]'s agreement rule; representation equality never permits cross-enum comparison.
`Bool` is admitted by the same tag-only rule.
Both operations lower directly to equality or inequality of the validated discriminants in T's already-selected representation.
They are pure and total: after normal operand evaluation, the primitive does not inspect a payload, access memory, trap, convert a value, or introduce a new optimizer fact channel; an operand read still exhibits its ordinary effect before the primitive executes.
Payload-carrying enums, enum ordering, and enum/integer conversion remain outside the operation table.

[OP-9] The static allocation-size obligation over a stored type T and a runtime count `n` is the pure, total, target-independent predicate
`n <= floor((2^64 - 1) / stride_ceiling(T))`, where `stride_ceiling(T) >= 1` is the language layout ceiling fixed below.
It is exactly the condition that the allocation's own size arithmetic `stride_ceiling(T) * n` does not leave the u64 domain [STOR-8]; it has no writer-callable spelling, exposes no target ABI value, and has the same result for one source type and n on every qualified target.
Each runtime-capacity construction [OP-13] and `grow` [OP-10] carries it over that operation's own stored type and count.

The obligation is accepted only when [ENT-6] discharges that exact goal; its sole normalized component is the defining comparison above, which may supply an alternate L0 derivation of the same root.
The root does not project a new general L0 fact in the other direction.
A refuted or unproved goal is a static OP-9 rejection; a contradictory state discharges it under [ENT-4].
When n comes from runtime input, it remains an ordinary symbolic term; only an enumerated fact constructor such as a selected real branch, a proved invariant target (including one checked by [PRF-1]), or a verified postcondition may discharge this goal [SCOPE-2, ENT-3, ENT-6].
No written conclusion alone, runtime multiplication guard, or fallback is retained.

All layout-ceiling arithmetic is over unbounded mathematical integers.
Let `round_up(x,a) = ceil(x/a) * a`.
For a sequence of `(size, alignment)` pairs, start at offset zero, round each current offset up to the next field's alignment, add that field's size, take aggregate alignment as the maximum of one and the field alignments, and round the final offset to that aggregate alignment.
The primitive `(size_ceiling, align_ceiling)` pairs are: `unit`, `Bool`, `i8`, and `u8` `(1,1)`; `i16` and `u16` `(2,2)`; `i32`, `u32`, and `f32` `(4,4)`; `i64`, `u64`, and `f64` `(8,8)`; `Box<T>` `(8,8)`, one pointer, its `inner` field living in the heap object and entering no sequence; a runtime-capacity `Array<T>` `(16,8)`, a pointer and a length; a runtime-capacity `Slots<T>` `(24,8)`, a pointer, a capacity, and a length; a runtime-capacity `Ring<T>` `(32,8)`, those three and a window origin; and every fieldless opaque struct `(32,16)`, the host handles' host-supplied representation [PRE-1].
Every other struct applies the sequence rule to fields in declaration order.
A constant-capacity `Array<T, N>` repeats T's pair N times.
A constant-capacity `Slots<T, N>` repeats T's pair N times and then applies the sequence rule to that block followed by one `(8,8)` word, its length.
A constant-capacity `Ring<T, N>` repeats T's pair N times and then applies the sequence rule to that block followed by two `(8,8)` words, its length and its window origin.
A tag-only enum with at most two variants has `(1,1)`, and every other tag-only enum `(4,4)`.
A payload enum, including `Option` and `Result`, sequences a `(4,4)` tag followed conservatively by every variant payload field in variant and field declaration order.
The existing recursive-type rejection remains, and the `Box` and runtime-capacity shape ceilings do not recursively expand their content.
`stride_ceiling(T)` is `max(1, size_ceiling(T))` after the aggregate rule.

Before emitting a stored type S, target qualification verifies that its actual size, alignment, and stride do not exceed the three language ceilings.
Only with both that qualification and the source obligation disposition may lowering emit `n * actual_stride(S)` as non-overflowing arithmetic.
Qualification failure is a target failure and may not become a runtime guard.
The [STOR-6] rule separately governs allocator and address-index representability; heap exhaustion remains a trusted-base resource failure [SCOPE-3, STOR-8], never a language trap.
`Array<T, N>` performs no runtime size computation: N is fixed at monomorphization and concrete target representability is checked under [STOR-6].
The language defines no numeric frame limit, and target-layout and resource failure are not program execution.

[OP-10] The window operations.
The nine operations are `place_back`, `take_back`, `insert_at`, `remove_at`, `append`, `split_off`, `grow`, `place_front`, and `take_front`; their signatures, rows and contracts are the [PRE-1] records and are declared there alone, and this rule states which part of the window each one moves and what that costs.
A compiler-owned window type parameter, written W in this rule and W or X in a [PRE-1] record, has exactly the admitted arguments `Slots<T, n>`, `Slots<T>`, `Ring<T, n>`, and `Ring<T>`; its element type is that shape's own element type; it is supplied by the operand and never written, and no source declaration can write such a parameter.
A window operation, `swap` [OP-11], and `free_empty` [OP-14] therefore write no type arguments at a call: every type parameter of those rows is supplied by an operand.
The construction functions [OP-13] write theirs explicitly at every call, as [FN-2] requires of every other generic callee.
An operand whose shape is outside the operation's admitted set — `place_front` or `take_front` on a `Slots`, or any other operand outside the admitted arguments stated here — is a hard error citing OP-10 at the complete `call`, with a repair [DIAG-1].
`place_back` fills the append slot `r.next` and moves the boundary `r.len` up by one; `take_back` empties the last filled slot `r.last` and moves that boundary down by one.
`insert_at` and `remove_at` each shift `r.filled` by one memmove and move the boundary by one, `insert_at` filling the append slot on its way.
Each of those two is a content write of `r.filled`, so a surviving slot reference names its slot, whose occupant may have changed, exactly as a stale index does [OP-13].
`append` and `split_off` each move a run of elements between two windows by one copy and move both boundaries.
`grow` is defined on `Box<Slots<T>>` alone, remakes the cell's content `deref(cell).inner` whole, may reallocate in place, and carries [OP-9]'s obligation.
`place_front` and `take_front` admit `Ring<T, n>` and `Ring<T>` alone and shift every logical index of `r`, so every reference into `r` becomes invalid [REF-2].
Writing one element is not a window operation: it is the ordinary assignment `set r[k] = x;` [SET-1], whose old value takes [WIN-3]'s disposition.
A reference into a window is formed under a bound and stays valid while that bound holds: `&r[i]` under `i < r.len`, and `&r[lo..hi]` under `hi <= r.len` [REF-4].
`place_back`'s `ensures` carries the bound across the call, while `take_back`'s and `remove_at`'s do not, so such references die there.
A ring position is a logical index and the wrap is the storage's business [WIN-1].

[OP-11] `swap`.
`swap(first: p, second: q)` exchanges the values at two owned places of one type; its signature and row are the [PRE-1] record, and it is built in because no source body can write it without a hole [WIN-3].
It is the one operation whose two reference arguments may name the same place, in which case nothing happens, so `swap(first: &r[i], second: &r[j])` needs no `i != j` branch [EFF-5].
Every possible pair of exchange targets must be equal or disjoint, with neither possibly a proper ancestor of the other. Equal captured targets and equal-depth slots under one identical array or window satisfy the equality allowance. Equal descendant covers alone do not. An unproved proper-ancestry exclusion is a hard error citing OP-11 at the call, with a repair [DIAG-1].
That admission concerns its own two arguments and nothing else: against any other statement a `swap` is judged by the ordinary pairwise rule on its two write paths [PAR-1, EFF-5], so an adjacent statement touching either path denies overlap permission.
At every program point each place holds exactly one valid owner; no temporary uninitialized hole, vacancy state, or second owner exists.
Neither root is consumed: both bindings remain live.
A `swap` over a copy place is a hard error citing OP-11 at the first `borrow_expr`, with a repair [DIAG-1]; that refusal is judged once at the written bound, exactly as [OWN-1]'s spelling judgment is, and is not re-made at a concrete instance [FN-2].

[OP-12] The atomic in-place update.
`set p = f(move p, args...);` for an affine place and `set p = f(p, args...);` for a copy one, where the first argument of the call is the target place itself, is the atomic in-place update: the old value enters `f` by value, `f`'s result is committed, and no program point lies between.
The target argument carries [OWN-1]'s spelling of its own class and no other.
Its target `p` is any owned place named by a path [REF-1] and writable under [SET-1], a place reached through `deref` of a reference parameter whose declared row carries `writes` of that path included.
Its effect is `writes(p)`.
`f` must return the place's type and must have no failure exit — every declared result ordinal is the place's type and no `ensures when` route removes a normal return.
`f`'s declared row must not write, move out of, or free any prefix of `p`, while reading anything and writing disjoint storage is admitted [EFF-5]; a row that does is a hard error citing OP-12 at the complete `call`, carrying that substituted path.
Nothing is said about termination.
A failure is an enum in the place, not a second result.
A call that fails the result condition is not an atomic update and is judged as an ordinary `set` [SET-1], in which the consumed argument would kill the target root [OWN-1]; the resulting rejection cites OWN-1 at the argument `atom`.
An atomic update is not a [PAR-2] accumulator form: that rule's accumulator combines by one operation fixed for it from a closed associative and commutative set, and `f` is not a member of that set.

[OP-13] Construction.
The construction functions are the [PRE-1] records `box_new`, `slots_new`, `ring_new`, `array_filled`, `box_array_filled`, `box_slots_new`, `box_ring_new`, `slots_from_array`, and `slots_into_array`; there is no `Type::name` spelling and no element-list literal in expression position [FORM-5].
Each runtime-capacity construction and `grow` [OP-10] carries [OP-9]'s static allocation-size obligation on its own count.
Allocation is total [STOR-8], so no construction returns a `Result` and none has a failure arm.
A window built by `slots_new`, `ring_new`, `box_slots_new`, or `box_ring_new` starts empty.
An `Array` built by `array_filled` or `box_array_filled` has every slot holding the supplied value and requires a copy element type [OWN-1].
`slots_from_array` consumes a full array into a full window, and `slots_into_array` consumes a window whose `len` equals its `cap`.
A pool is a `Slots` plus indices used as handles, and a bump allocator is the same storage used with `place_back` [OP-10] as allocation and a library reset.
A stale index that is still in bounds names the current occupant of that slot, which is a logic error and not a memory error, and a program that must detect it keeps a generation number as data.

[OP-14] `free_empty`.
`free_empty(window: move r)` consumes any window proved empty, an affine element type and a linear one alike, with the contract `requires window.len == 0_u64` submitted to [MSR-4] at the call.
Its compiler-owned shape parameter W has exactly the admitted arguments `Slots<T, n>`, `Slots<T>`, `Ring<T, n>`, `Ring<T>`, `Box<Slots<T>>`, and `Box<Ring<T>>`, so `free_empty(window: move b)` consumes a boxed runtime-capacity window and frees its cell with it; at a boxed argument the row's measure place instantiates as `window.inner` and the clause reads `window.inner.len == 0_u64`, exactly as any `Box` content is reached [TYPE-9, OP-4].
An operand whose shape is outside that admitted set is a hard error citing OP-14 at the complete `call`, with a repair [DIAG-1], exactly as [OP-10] refuses its own operands.
A linear element type stays linear [PROV-6]; the proof is about the runtime length and never about the class.
An undischarged obligation is a hard error citing OP-14 at the complete `call`, rendering the residual and its disposition [ENT-4], with a repair [DIAG-1].

[OP-15] A measure read is a field read.
`r.len`, `r.cap`, and `r.head` are ordinary `psuffix` field selections of the readonly fields the prelude declares [GRAM-5, MSR-1, PRE-1], and `deref(part).len` of a range reference is the one measure no declaration states [REF-4]; none is a call.
Reading one performs no operation, allocates nothing, and reads only the descriptor storage [MSR-2]; its exact type is `own u64` and its value mode is exact `own`, so the ordinary [TYPE-5] check applies at each use.
Its effect is the ordinary attribution [EFF-2]: a measure read through a reference parameter `p`, written `deref(p).len` [REF-1], exhibits `reads(p.len)`, the measure being a path below that parameter [EFF-1]; a measure read rooted in a local exhibits nothing.
A measure is never a write target [TYPE-2].
One quantity, one spelling: there is no reader operation beside the place form [FORM-1].

## 8. Functions, generics, contracts

[FN-1] A concrete function's callable boundary states everything ordinary callers need: parameter modes and types, the ordered result list's modes and types, one formal-path state-effect row, the ordered [FN-8] requirement GoalTemplates, the ordered verified [FN-9] normal-result RelationTemplates.
A function returns owned values only [GRAM-3, REF-3]; a position found by a search is returned as an index with its bounds relation in `ensures` [FN-9].
Every result binder's spelling is mandatory but ignored by callable-signature equality and denotes no runtime storage.

A `fn_decl` or `fn_sig` writes one result or a parenthesized list of two or more [GRAM-2].
Each `result_binding` is one **result ordinal**, numbered from zero in written order; the list's binder spellings are distinct under [TYPE-6], and each ordinal receives every result judgment of this rule independently.
A declaration that writes a list hands its ordinals back together, and a caller names them again only through a destructuring `let` binder list [GRAM-4, TYPE-5, CALL-4]; no expression position produces a result list, so a list-returning callee is bindable only by that form.
This rule's remaining sentences are stated over a written result and read per ordinal where a declaration writes a list.
The written templates are ordinary interface propositions. A Whitefoot definition proves them under FN-9; a PRE-1 or PRE-2 definition is supplied under SCOPE-3. A caller consults only the declared finite summary and never the definition.
The written effect paths state which reference-parameter-supplied state the function observes or changes. The checker derives the exact same set from body accesses and calls and checks it in both directions under [EFF-2].
Strengthening a requirement GoalTemplate or RelationTemplate is a caller-visible interface change.
A generic function carries the same boundary with its written type, const, and function parameters, and each concrete [FN-2] instance substitutes them before its calls and body are re-checked.
A `fn_sig` may carry the same requirement and postcondition templates. FN-4 checks their formation and refinement at binding; its selected ordinary definition supplies their proof under FN-9, PRE-1 or PRE-2.
Function-signature visibility is the [TYPE-6] table.
Every explicit `return e1, ..., en;` writes exactly as many expressions as the enclosing declaration writes results, and expression i must produce exactly result ordinal i's `rtype`; there is no result-mode or result-type conversion [TYPE-4].
A written count other than the declared result count is a hard error citing FN-1 at the `return_stmt` node.
The implicit-read case already owned by [TYPE-7] is exclusive: when `e` uses a reference binding where its referent value would be required by the written `rtype`, that use is rejected citing TYPE-7 and FN-1 forms no candidate.
A returned reference is owned by [REF-3] in the same exclusive way.
Every other return mode or type mismatch is a hard error citing FN-1 at the `return_stmt` node, with `SourceCoordinate` equal to the complete checked half-open source extent of its selected `expr` child.
FN-9 adds a stricter result and return-expression shape only for a function that declares an `ensures_clause`; a function with none retains every return form admitted here.

On entry to a `for_stmt`, the lower endpoint atom is evaluated exactly once and then the upper endpoint atom is evaluated exactly once, each under its ordinary atom, ownership, and source-check judgments; the compiler copies their mathematical u64 values into distinct immutable hidden lower and upper captures in that left-to-right order.
No header test or body operation occurs before both captures exist.
The compiler then initializes the fixed `own u64` binder to the lower capture.
At each header it performs one pure mathematical comparison of the binder with the upper capture.
A false result reaches the counted continuation without entering the body; a true result enters the body.
Thus lower greater than or equal to upper executes zero iterations after still evaluating both endpoints.
On normal body fallthrough, body-scope cleanup completes first [STOR-3], then the compiler updates the binder exactly once to its mathematical value plus one and returns to the header.
The true guard proves the old binder is less than the u64 upper capture, so that increment is representable; it is a pure compiler operation with no hidden trap, wrap, saturation, operation-table call, or effect, including when the upper capture is max(u64).
An edge leaving the counted body by `break` to that loop or an enclosing loop, `return`, or `propagate`'s `Err` path performs its ordinary cleanup exactly once and performs no hidden update.
The counted header's carried-identity set is exactly the bindings carried into the construct plus both captures and the binder; the continuation interface and a break resolved to that counted loop carry only the incoming identities after their path-specific ownership, cleanup, and effect judgments, with any counted label, the binder, and captures all out of scope.
Ordinary `loop_stmt` execution is unchanged.

Function completion and statement reachability use one conservative structural normal-control graph over the resolved function body.
For any statement s, `normal_successor(s)` is the entry of s's next sibling statement in the same block when one exists, and otherwise that containing block's normal exit.
A block entry reaches its first statement, or its normal block exit when it contains no statement.
An ordinary `let`, `set`, an expression statement, and an `invariant_stmt` have a normal edge to `normal_successor(s)`; an `invariant_stmt` is then erased before lowering.
A call with a normal result edge never proves divergence merely because external resource availability is outside this cycle's guarantee [SCOPE-3].
A `return_stmt` has an edge only to the function-return sink.
A `match_stmt` enters every arm body, using an arm's normal exit when that body contains no statement, and each arm's normal exit reaches `normal_successor(match_stmt)`.
A `let_stmt` selecting `value_match` enters every arm body the same way and follows [GIVE-1]: each `give` edge reaches `normal_successor` of that enclosing `let_stmt`, each return edge reaches the function-return sink, and each resolved break edge reaches `normal_successor` of its target loop.
An `if_stmt` enters its then-block, and its else-block when it has one, using a block's normal exit when that block contains no statement; each block's normal exit reaches `normal_successor(if_stmt)`, and an else-free `if_stmt` also has its false edge directly to `normal_successor(if_stmt)`.
A `let_stmt` selecting `value_if` enters both branch blocks the same way and follows [GIVE-1] exactly as the `value_match` sentence above does; an else-position `value_if` of a chain contributes its own branch edges to the same enclosing `let_stmt` [GIVE-1], not to a nested one.
A `let_stmt` selecting `propagate_let_rhs` has an `Ok` edge to `normal_successor` of that enclosing `let_stmt` and an `Err` edge to the function-return sink [ERR-3].
A `break_stmt` reaches `normal_successor` of its resolved target loop, ordinary or counted.
A `loop_stmt` reaches its body entry, or its body's normal exit when the body contains no statement; the loop-body normal exit reaches the body entry again, or itself when the body contains no statement.
For this conservative judgment every `loop_stmt` also has an edge to `normal_successor(loop_stmt)`; no ordinary loop is assumed to diverge [GIVE-1].
A `for_stmt` reaches its compiler-owned preheader, the preheader reaches its header after both endpoint evaluations and binder initialization, and the header has both a true edge to its body entry (or the body's normal exit when empty) and a false edge to `normal_successor(for_stmt)`.
Its body normal exit reaches the compiler-owned update, and that update reaches the header.
A `break` resolved to the counted loop reaches `normal_successor(for_stmt)` without the update.
Every counted header retains both structural edges even when its captured endpoints are constant, so no counted loop is assumed to execute or to diverge [GIVE-1].
The function-body normal exit has no successor.
These edges are structural and are not removed by constant evaluation, a proof, or backend reachability.

Each statement not reachable from function-body entry establishes an FN-1 rejection premise using `SourceNode` at the selected concrete statement production beneath its `stmt` wrapper and a `SourceCoordinate` equal to that production's complete checked half-open source extent.
When more than one statement establishes that premise, the reported one follows DIAG-1's implementation-defined deterministic traversal. [GIVE-1] remains the more specific owner of a statement following `give` in the same block, so that statement establishes no additional FN-1 reachability rejection.
The function body's normal exit must be unreachable.
If it is reachable, the function falls through and is rejected citing FN-1 at the `fn_decl` node, with `SourceCoordinate` equal to the complete source interval of the body-closing `}` token.
This requirement applies to `own unit` as well as every other result: successful completion is written `return unit;`; there is no implicit return.
A call with no termination proof or a loop does not satisfy the return requirement.
This complete structural graph, its statement reachability, and every source call and invariant-declaration identity are retained for source audit even when [FN-8] later proves one concrete instance uninhabited.
That proof changes only its checked body disposition and lowering authority; it never erases a source node or narrows the written effect row.

[FN-2] Function and nominal generics are monomorphization-only; type, const, and function instantiation arguments are always explicit. FN-3's named groups abbreviate those explicit parameter and argument vectors; expansion is compiler-side, pre-IR, and instantiations are re-checked as concrete code.
Every contract definition and requirement or postcondition template is substituted separately for each concrete function instance.
The [FN-8] uninhabited judgment is likewise instance-local and never propagates from one concrete substitution to its generic template or another instance.
Every explicit type argument supplied to a function, source nominal, or [PRE-1] nominal generic parameter must be a value type, so a reference kind is a hard error citing FN-2 at that complete `targ` [TYPE-8], with a repair [DIAG-1].
Arguments this rule admits remain governed by the ordinary bound and substitution rules.
A generic type parameter's numeric bound is admitted only when it resolves to the built-in `Int` or `Float` bound [PRE-1]. A interface or binding group is not a numeric bound; it occupies its own explicit argument-list position [FN-3].
A written `copy` or `drop` bound on a type parameter is [PROV-6]'s capability filter, never inferred, read once at the declaration and checked at every instantiation by that rule. It selects no behavior; function-kind parameters supply behavior separately.
Every type parameter of a function or of a nominal carries at most one bound, never inferred: one built-in numeric bound, `Int` or `Float` [PRE-1], or one capability bound, `copy` or `drop` [GRAM-2, PROV-6]; an absent bound grants the body no capability; a numeric bound selects the numeric rows [OP-1] and implies copy.
The template is the spelling authority: a generic body is checked once at the symbolic instance of its own parameters, under each parameter's written bound, and the concrete-instance recheck this rule performs does not re-judge the spellings [FORM-1] keys on a value's copy/affine class — `move p` against a bare `p` [OWN-1].
At a concrete instance a `move` of a value whose parameter was bounded `drop` or left unbounded denotes a copy where the argument is copy; every other judgment of that rule is made at the instance, because each is a property of the instance and not of the written spelling.

[FN-3] A function-kind generic parameter is one `fn_sig`: an ordered ordinary callable signature with its own effect row, requirements, and ensures. It is a compile-time parameter, never a value, field, receiver, or implicit argument.
An `interface` declaration names one ordered parameter group. Its header declares type and const parameters with their ordinary bounds; its body declares the function-kind parameters in source order, with distinct member names. An interface header cannot contain another group or a function-kind parameter: groups are flat abbreviations, not functions that construct interfaces.
An interface member's parameter, result, effect, requirement, and ensures formation follows the same rules as an ordinary callable declaration; it has no body or trusted proof. Its contracts constrain admissible actuals [FN-4] and may be used only after those bindings have been checked.

An `"interface" pack_use` in a function or nominal generic parameter list names an interface declaration and writes one fresh type or const binder for each header parameter, in order. For example, `fn find<interface Key<K, E>>` declares the two written binders with Key's respective bounds, followed by one distinct function-kind parameter for each Key member. The same interface may be used more than once with distinct written applications.
The `interface` marker is mandatory at every group introduction, including an interface with no header parameters: `fn run<interface Source>()` imports Source, while a bare `Source` in that position declares an ordinary type parameter under [TYPE-6] and never falls back to a group by name lookup. The marker is not written on group forwarding, concrete arguments, or qualified member calls.
A forwarding `pack_use` names those already declared binders and their function-kind parameters; it declares nothing. `Key<K, E>` in a call's or nominal's argument list forwards the complete expanded vector. No expected type or argument omission supplies a behavior.
Generated member identities are indexed by the receiving declaration, its written pack application, and the member ordinal. They are hygienic: they neither introduce an unqualified lexical name nor capture a local spelling such as `hash`.
The abbreviation names do not survive in type identity: two nominal applications with the same expanded type, const, and function vectors are the same instance, even when their binding group names differ.

A `binding` declaration names one ordered argument group for its explicitly written interface application. It declares no type, value, conformance, or implementation attached to a type.
For each interface member in source order it writes exactly one `fn_bind` in that order, with the matching left name and an explicitly selected right function. Missing, extra, repeated, unknown, or out-of-order members reject under FN-3 at the offending binding or the complete binding declaration for a missing member. No function is selected by name similarity, signature search, expected result, or a type-owned implementation.
All bindings are checked together at the declaration by FN-4; no partial group is published. A zero-member interface and matching empty binding are legal abbreviations.
`find::<SeedKey>` and `Map<SeedKey>` expand the named binding's complete argument vector in their written position. A raw function-kind argument is `fn seed_hash`, or `fn helper::<T, n>` with every type, const, and function argument supplied. It must denote a fully instantiated ordinary function or an in-scope function-kind parameter, never a table operation, constructor, runtime expression, or partially applied generic function.
A binding group's member may be forwarded by its explicitly qualified name. Its dependency graph must be acyclic; a cycle is an FN-3 error naming that cycle, rather than an attempt to evaluate a group.

[FN-4] Every function-kind binding is checked against the instantiated formal signature before use. Named groups check all members at declaration; raw arguments receive the same check at their written argument.
A supplied function may refine the formal signature rather than match it.
Parameter and result counts, modes, and exact types must agree in order; parameter and result binder spellings are not signature identity.
The actual's declared row must be a subset of the formal's after parameter-ordinal and path normalization; a row states exactly what the body does [EFF-2], so a read-only function cannot declare a write.
The actual's own declaration must independently satisfy EFF-1 and exhibit exactly its own row under EFF-2.
The actual's `requires` must be weaker than the formal's and its `ensures` stronger, and each actual's requirements and ensures have the ordinary FN-8/FN-9 formation and verification boundary, including PRE-1 and PRE-2 declarations.
Weaker and stronger are decided by a fixed finite check inside the existing affine entailment fragment [ENT-1, MSR-4] and by no solver: for each actual `requires` goal, the formal's `requires` set must discharge it under [MSR-4]'s disposition with the formal's own set as the only premises; for each formal `ensures` relation, the actual's `ensures` set must discharge it with the actual's own set as the only premises.
The check is deterministic and terminating because both sets are finite and each query exhausts [ENT-6]'s fixed families.

A mismatch names the member or raw argument and the differing signature, row, clause, or result ordinal, and carries a repair [DIAG-1].
No reflexivity, transitivity, ordering consistency, hash/equality compatibility, or other algebraic law follows from a binding. Ownership, range proofs, initialization and cleanup must hold even for inconsistent supplied behavior.

[FN-5] There are no function values, methods, receivers, implicit Self, dictionaries, or dynamic dispatch. Closed-set runtime dispatch is `match`.
An unqualified IDENT call may name an in-scope raw function-kind parameter. `Key::hash(...)` names the corresponding member of the unique in-scope application of interface Key; when two applications of Key are present it is rejected at the callee and the source must write the full application, for example `Key<K1, E1>::hash(...)`. Selection uses distinct written applications before substitution, even if their eventual concrete types coincide. Two identically written applications require separately named raw function-kind parameters rather than a pack alias.
At a member call, named value arguments use the formal signature's parameter names in declared order, not the actual implementation's parameter names. The selected function's type, const, and function arguments were supplied explicitly at binding or group instantiation under FN-2; the already bound member accepts no further specialization. Requirements, ensures, and effect attribution use the instantiated formal interface. A bare measure rooted at a reference parameter whose row declares a write of it denotes the exit state, and `entry(parameter)` the entry state [MSR-3]; CALL-6 kills overlapping formal-row support before publishing the verified relations.
Every concrete function binding resolves to one ordinary function instance before IR. Lowering emits a direct call to that instance with the ordinary ABI, preserving its ordinary signature and ownership; group expansion emits no adapter call, storage, dispatch, dependency, or runtime proof check.
Bodies retain FN-2's symbolic spelling check and FN-2/FN-9's concrete-instance rechecks. A concrete instance uses formal contracts and the authoritative formal row at each bound call while independently checking the selected actual under the same signature, row, and contracts.

[FN-6] Recursion is permitted. Instantiation is finite by a structural rule over the finite written dependency graph, not by executing compile-time code or by a work, depth, or time budget.
The graph contains source function and source nominal templates; its edges include calls, instantiated nominal uses in signatures and fields, function arguments, and calls through function-kind parameters after their finite explicit bindings are resolved. Group abbreviations are expanded before the graph is checked. All edges and their complete type, const, and function argument vectors are retained.
Within every recursive component, each edge must forward the caller's complete parameter vector unchanged in position and kind. Constructing, specializing, dropping, adding, or permuting an argument on a cycle rejects under FN-6 at that dependency. This includes a growing nominal such as `Grow<T>` containing `Box<Grow<Box<T>>>`, and a call that wraps a function argument in a new specialized function on each traversal.
The diagnostic names the function/nominal cycle and the changed argument, with a repair [DIAG-1].
This criterion deliberately rejects some finite permutation cycles. Acyclic expansion is finite and a cycle creates no new instance key; deterministic checking visits each admitted instance. It assumes no behavior laws and uses no fuel.

[FN-7] Program start selects an ordinary function and supplies ordinary arguments [PROG-3]. Its name, signature, result types, written contracts, and source callers obey FN-1 through FN-10 without an entry-specific restriction.
A module program names its entries in its graph [MOD-9]; a source bundle's build selects its function `main`, and the language reserves no entry name. Selection, argument construction and binding, and interpretation of a normal result belong to the build invocation and do not select source acceptance.

[FN-8] Every source `fn_decl`, generic or nongeneric, and every `fn_sig` may carry one optional `contract_block`. A function formal's block has the same formation rules and constrains bindings under FN-4. Every supplied definition must satisfy the ordinary declared contract; a Whitefoot body is checked under FN-9 and a PRE-1 or PRE-2 declaration is supplied under SCOPE-3.
A present block must contain at least one `requires_clause` or `ensures_clause`; an empty or define-only block is an FN-8 rejection at `contract_block`.
Grammar fixes all definitions before all requirements and all requirements before all postconditions.

The definition scope initially contains the function parameters, named consts, and live type and const parameters, then each earlier definition after its complete initializer.
Every definition and clause expression must consist only of non-consuming datums, measure place forms [OP-15], [ENT-2] clause (b) places, and operation-table forms that are pure and total for every value in their selected operand domain.
Bare `cvt` is admitted exactly for [OP-6]'s whole-type total pairs, including its universally total symbolic pairs; every `cvt.checked` and `cvt.defined` pair is admitted by its total row.
Exact addition, subtraction, and multiplication are admitted and read as operations over the mathematical integers rather than as evaluations, exactly as an `affine_expr` is [INV-1]. A clause is erased before lowering and evaluates nothing, so a row whose meaning is total over the mathematical integers states a relation where it would otherwise request an operation, and no domain obligation arises to discharge.
Function calls, construction, move, borrow, a subscript outside a clause (b) place, mutation, control flow, allocation, and every other partial operation are inadmissible even when another clause states their domain. Exact division, remainder, negation, absolute value, and the shifts remain partial under this judgment.
Their corresponding `.defined` queries are total and admissible.
Each definition produces an own copy value, follows ordinary typing and no-shadowing, and is erased by recursive alpha-expansion into every later clause; no definition is evaluated, snapshotted, lowered, or visible in the body.

Each requires expression is one `clause_expr` [GRAM-5, MSR-5], has exact mode and type `own Bool` under [OP-5], and independently forms one finite typed GoalTemplate after definition expansion.
A clause side's `+`, `-`, and `*` form that template's own operation nodes over the mathematical integers [MSR-5] and add no domain obligation, so a requirement side carries the whole affine expression and is not narrowed to the difference-bound fragment a published relation is [FN-9].
A formal datum keeps its zero-based parameter ordinal and its field, `deref`, subscript, measure, and payload projections; named consts, literals, selected operation rows, written arguments after substitution, result types, and operand order retain their existing identities.
Definition spelling, sharing, and NodePaths are absent after expansion.
The requirement occurrence is `(concrete function instance, requires_clause NodePath)` and is outside predicate equality.
Two predicates are equal only by exact typed-tree equality: there is no commutation, folding, reassociation, inversion, or De Morgan rewrite.
Signed decomposition, exact comparison-root L0 projection, and the fixed query-time Boolean introduction over independently proved children remain exactly [ENT-3, ENT-4, ENT-6].

At an ordinary source call, resolution, concrete instantiation, named arguments, exact types, borrow feasibility, and all actual-expression obligations complete first.
For every GoalTemplate in requires-clause source order, substitute each formal with that actual's Goal value identity in the same pre-transfer fact state: a borrow formal uses its resolved referent and an own actual its value before transfer.
A literal, named const, or place with field and `deref` projections remains an ordinary datum.
After every actual-expression obligation succeeds, an own actual whose complete
checked value belongs to [ENT-2]'s admitted exact-operation or index tree uses
that same structural Goal identity. If the complete value is outside that
admitted tree, it uses [ENT-2]'s occurrence-local call-argument
evaluated-value identity instead. No
exact operation or index identity is admitted before all of its nested domain
obligations succeed.
Every instantiated goal is judged independently in that unchanged state; a discharged clause adds no fact for a later clause.
The first refuted or unproved clause is the FN-8 call-site rejection and forms no checked program.
Only total success reaches ordinary transfer, effects, and normal return; no call receives a runtime fallback, alternate entry, or body clone.

At concrete body entry, every requirement goal is established independently as an [ENT-3] S4 source, in source order, with its own signed decomposition and exact L0 projection.
Its established ordering leaves also receive exactly S4's fixed affine images below.
The clauses are never banded together.
Requirement establishment adds no executable callee prologue or alternate lowering; the proved facts may be supplied to the backend under [DIAG-2], and later kills apply normally.
Direct and mutual recursion, forward calls, and every concrete generic instance use the same finite rule.

After all S4 sources and implicit parameter/type facts are closed under [ENT-4], a contradictory entry state makes that concrete instance legally uninhabited.
The checked body disposition is `Uninhabited { contradiction: DerivationId }`; it is success metadata, not a source rejection, and the derivation survives final identity remapping.
Syntax, resolution, type, ownership, effect, return-shape, statement reachability, call, and proof-form checks still inspect the complete source body, while proof obligations discharge under the contradictory state.
An uninhabited instance publishes no postcondition summary.
Lowering must preserve its ordinary ABI and symbol but emit exactly one empty entry block terminated by `unreachable`, without traversing or lowering any source statement.
A source call must still prove every contradictory requirement, which no reachable non-contradictory caller state can do.

[FN-9] Each `ensures_clause` in a FN-8 `contract_block` declares one independent normal-return relation.
A source declaration is not a trusted assertion: its body proves the relation by the selected-return judgment below. A PRE-1 or PRE-2 signature supplies its declared relation under SCOPE-3 and has no source returns to check. Formation and caller instantiation are the same ordinary judgments in both cases.
No contract definition or clause contributes an effect, executable epilogue, runtime operation, storage slot, or runtime report.

Every declared result ordinal is a datum of every clause, written as that ordinal's `result_binding` spelling [CALL-4].
An unrouted clause is admitted only when every result ordinal it names is `own T` with T one [ENT-2] fragment integer after concrete [FN-2] substitution, or is `own T` with T a measured type [MSR-1] named as a measure member and nowhere else [CALL-4].
Its symbolic result datums are those ordinals' `result_binding`s.
A routed clause is admitted only as exact `when Ok(value: r):` or `when b is Ok(value: r):` for a result ordinal whose mode and type are `own Result<T,E>` with T a fragment integer, where `b` names that ordinal, r is that clause's fresh symbolic payload datum, and `Ok` and `value` retain their PRE-1 identities.
The ordinal binder may be omitted exactly when one declared ordinal has that enum type; two or more leave the route ambiguous and are refused at the declaration [CALL-4].
Route owner, ordinal, variant, field, and freshness admission precedes resolution of that clause expression [GRAM-10, TYPE-6].
The routed ordinal's whole-Result binder is unavailable in that clause; every other ordinal's binder remains a datum of it.
Unit, float, aggregate, nested-payload, whole-Result, non-Ok, and every other shape remains a legal ordinary result but cannot supply a relation datum in this version.
Omitting Err routes means Err exits are unselected, not unreachable.

After recursively alpha-expanding every shared `contract_define`, the clause expression must have exact type `own Bool` and its root must be exactly one `compare_op` — `==`, `!=`, `<`, `<=`, `>`, or `>=` [GRAM-5].
Each operand is one **relation term**: one datum displaced by a written constant, which is the shape [ENT-4]'s closure represents and the shape every declared relation of the kernel declaration domain writes [PRE-1, PRE-2].
Its datum must be one of the clause's symbolic result datums, a parameter datum with field and `deref` projections, a named const, a typed integer literal, a measure member of an admitted formal place P [OP-15, MSR-5], or a measure member of a declared result ordinal of measured type [CALL-4]; at least one operand contains a result datum (a measure member over one included) or the exit-state measure of a reference parameter whose row declares a write of that path, and the two may name two different result ordinals. A clause naming only that exit state is admitted regardless of the result type, including unit.
Its displacement is the mathematical value of the rest of that `affine_expr` side, which must reduce to one integer constant: the side is admitted exactly when it carries one such datum with coefficient one, or none and a constant, and a side carrying two datums or a datum with any other coefficient is outside the difference-bound fragment [ENT-4] and is an FN-9 rejection at that clause naming the fragment.
A measure member rooted at a reference parameter whose row declares a write of that path denotes the selected return's exit state; `entry(parameter)` denotes that parameter at function entry [MSR-3].
No proof-required exact operation, computed arithmetic result, subscript, occurrence-local evaluated-value datum, Boolean connective, nested result projection, or body local becomes a relation datum; a clause side's own `+`, `-`, and `*` are the mathematical integer expression [MSR-5] fixes and are the displacement rather than an operation.
The comparison normalizes to one finite L0 RelationTemplate whose two terms carry their displacements as one folded constant; equality's two bounds remain one relation occurrence.
Parameter datums denote function-entry images, except that a bare measure member rooted at a reference parameter whose row declares a write of that path denotes exit state [MSR-3].
The template retains parameter ordinals and projections, result ordinals, route declarations, named-const identity, literals, substitutions, comparison row, operand order, and normalized relation, while excluding result/route/definition spellings, definition sharing, and callee identity.
Its occurrence is `(concrete function instance, ensures_clause NodePath)`.

An unrouted clause selects every explicit return.
A routed Ok clause selects an explicit return of its routed ordinal under the assumption that the returned value is Ok. A direct canonical `Ok<T,E>(value: atom)` uses its payload atom as that ordinal's result datum. A forwarded Result uses the private payload parameter and conditional numeric context of [ENT-5]; the return proves the clause in that context combined with the ordinary current context.
A direct Err, an outcome whose transported constructor-tag information is definitely Err [ENT-5], and a propagated error exit are unselected. No other Result expression is rejected solely for its return shape; absent transported evidence supplies only the payload's standing type facts.
At a selected return, each result datum the clause names evaluates to one [ENT-2] term or constant, read from its own ordinal's returned expression; an ordinal the clause does not name imposes nothing.
For an ordinary inhabited instance, each clause's selected-return set is independently nonempty; an empty set rejects at that `ensures_clause`.
An [FN-8] uninhabited instance still checks route, type, expression, and return-shape source judgments, but is exempt from nonempty and proof requirements and publishes no relation.

A referenced `own` parameter's measure, a measure member of a reference parameter whose row declares no write of that path, or a measure explicitly rooted at `entry(parameter)`, is that parameter's entry datum [MSR-3], which is minted at body entry, contains no place, and is therefore killed by nothing. A bare measure member of a reference parameter whose row declares a write of that path instead evaluates over that parameter's resolved referent immediately before each selected return, after the return's ordinary effects and kills; a write of that referent changes this exit term and never retargets the entry datum. Non-measure parameter datums retain the entry-image stability rule below.
Every other referenced parameter entry image creates no snapshot term.
Its stability begins live at body entry and becomes permanently unavailable on the first structural edge whose [ENT-5] kill overlaps the datum, a holder used by it, or its support; join is intersection and contradiction never restores it.
An element write does not invalidate such an image, while a write to the place's own descriptor storage or to any prefix of it, or killing its root or holder, does [MSR-2]; at a call, which of the two a projected callee write is, is [CALL-1] through [CALL-3]'s classification and never the argument's shape [CALL-5].

For each clause in source order and then each selected return in NodePath order, the checker first completes ordinary return typing, obligations, calls, effects, and pre-return kills.
If an entry image is unavailable, the relation is unproved; otherwise substitute the result datum and query immediately before return transfer and edge cleanup.
The relation is queried once in the current ProofContext at that return.
Every query must discharge; the first clause/return failure rejects with no runtime fallback.

Postcondition verification has no summary fixed point.
Form the concrete ordinary-call graph, its SCCs, and the callee-before-caller condensation. PRE-1 and PRE-2 supplied declarations and pending interface declarations [MOD-8] are leaves whose declared relations are already available; source definitions undergo the following body verification.
The graph treats a call from one module to an instance of another module's generic callable as reaching every function-kind actual that instance was supplied, whether or not its body calls them [MOD-8]; components formed this way only grow, and an edit to that body cannot change which summaries the calling module's proofs may use. Within one module the graph is the calls alone.
While verifying a component, all same-component S12 summaries are unavailable; previously completed callee components remain available.
Only after every relation of every inhabited instance in the component succeeds are all its relation summaries published atomically; any failure publishes none. Publication is per module: the members one module's check verifies publish their summaries to that module's other proofs once all of them verify, another module's instance never delays it, and composition still requires every member to verify.
Uninhabited instances contribute no summary.
Declaration or worklist order and iteration cannot change the result.

For one ordinary call c, `A0(c)` means resolution, concrete instantiation, named arguments, exact types, borrow feasibility, every actual-expression obligation, exact formal substitution, and success of every FN-8 requirement have all occurred in that order at the same pre-transfer point.
Failure forms no postcondition candidate.
For one relation q, `M(c,q)` holds only when q's route matches that exact establishment event, result and referenced formals substitute independently to live [ENT-2] terms or constants after ordinary kills, and no referenced actual is represented only by an occurrence-local evaluated-value datum.
A discarded or aggregate-stored result with no admitted destination, an unsupported route, killed support, or a nonterm actual makes only the relations that reference that unavailable datum false under M. An integer-payload Result's conditional destination and success selections are [ENT-5]'s. A relation naming no result needs no result destination and establishes on an ordinary successful call continuation, including a unit-returning or discarded-result call.

Subject to A0 and M, failure-atomic scratch establishes q after transfer, consumes, borrow commits, callee-effect kills, and target kills.
Every establishment retains its ordinary declared-relation parent, including the selected-return proofs for a source definition, plus all actual-obligation and requirement parents from A0.

All matching verified relations are established together on the admitted result route.
An unrouted fragment result establishes onto the fresh binding of a direct ordinary-let call and onto the target place of a direct ordinary `set` whose right-hand side is that call: one destination rule at two placements, the `set` placement established after that statement's own commit and target kills [CALL-6], where a substitution whose support those kills remove makes only that relation unavailable.
An unrouted relation over a declaration's result ordinals establishes onto the binders of a destructuring `let`, ordinal i onto binder i [GRAM-4, CALL-4]; an ordinal whose destination is no [ENT-2] place makes only the relations naming it unavailable.
A routed Ok relation establishes in the returned value's conditional context with the private payload parameter substituted for the routed result. ENT-5 owns its transport, support lifetime and selection; naming the outcome does not re-instantiate the call.

The existing narrow receiver routes remain per relation.
For `set x = user_call(...)`, x is a live bare own fragment of the exact result type and exactly one argument is direct non-consuming x; after transfer, effects, commit, and kill, a relation may substitute result with post-write x only when it omits the formal supplied by x and all other supports remain live and disjoint [OWN-7].
That direct-set receiver route establishes no equality; a projected, consuming, repeated, aliased, wrong-type or unsupported receiver establishes nothing by that route. A selected integer payload subsequently follows the ordinary S5 value-image and SET-1 commit rules at every assignment, with no first-arm-statement condition.

All candidate S12 and delivery facts remain in one failure-atomic scratch batch until the current statement's ordinary transfer, effects, ownership commits, and kills succeed; then the whole batch commits once.
No candidate is individually committed or retracted and no second flow walk or negative fixed point exists.

Every successful selected-return proof and caller establishment extends [DIAG-2]'s one derivation DAG.
Postconditions add no runtime operation, hidden check, alternate lowering path, or ABI field; their proved facts may be supplied to the backend under [DIAG-2].

[MSR-5] A contract clause is the relation an invariant already is, over a wider operand set.
A `clause_expr` side is an `affine_expr` whose `affine_factor` is an `atom`, a `call`, or a constructor `call` [GRAM-4].
A clause side is that same `affine_expr`, so `ensures rest.len <= vector.len + 1_u64;` states in one clause the relation a `header_invariant` states in one invariant, and the two placements of one relation share one production rather than one spelling each.
The `+`, `-`, and `*` of a clause side denote the mathematical integer expression [INV-1] fixes and perform no [OP-1] operation, so a clause side creates no [OP-2] domain obligation and admits no runtime value it could overflow; a `clause_op` is exactly the Bool-valued rows of [OP-1], the six comparisons and the five infix `defined` queries, and the arithmetic operators are consumed inside the side.
A clause is judged by exactly the [OP-5] condition [FN-8] and [FN-9] already apply: the root has exact value mode and type `own Bool`, and every operand is a non-consuming datum or an operation-table form pure and total over its selected operand domain.
A measure is a place [OP-15], so it reaches a clause and an invariant through the `atom` admission and needs no former; the `call` alternative of `affine_factor` admits a constructor `call` and the domain-query rows.
These spellings confer no route, fact source, or proof authority beyond the admissions defined here.

[CALL-4] Contract vocabulary, the result ordinal, the routes, and where the relations land.
The clause operands of [FN-9] are terms [MSR-5], so a measure member of an admitted formal place is an operand with no per-family admission, and so is one of an admitted result place.
A `fn_decl` declares one result or an ordered result list of two or more [GRAM-2, FN-1], and each `result_binding` is one **result ordinal**, numbered from zero in written order.
Every ordinal is a datum of every clause, written as that ordinal's binder spelling, and a single-result declaration is the one-ordinal case of this sentence rather than a second rule.
A result ordinal's declared type is a fragment integer after concrete [FN-2] substitution [FN-9] or a measured type [MSR-1], and which of the two decides what that ordinal supplies: a fragment ordinal is a datum of the clause as its own value, and a measured ordinal is a datum only as a measure member of that ordinal's place.
A measure member of a result place is instantiated at that ordinal's own destination [ENT-3.S12] — the place the destination names — exactly as a measure member of a formal place is instantiated at the formal's, and is queried at a selected return over the place that return hands back.
An `ensures_clause` admits a measure member of a result place exactly when that place is the bare result place or is reached from it through an owned descendant projection [MSR-3] made only of struct-field selections and `Box` `inner` steps — `result.len`, `result.inner.len`, `made.storage.len`; the construction rows publish `result.inner.len` for a boxed runtime-capacity shape [OP-13, PRE-1, TYPE-9]. An enum-payload step admits no such member, because no route selects a variant of an unrouted result.

A routed clause is written `when V(f: r):` or `when b is V(f: r):`, where `b` names the result ordinal the route applies to.
The ordinal binder may be omitted exactly when one declared ordinal has that route's enum type; when two or more do, the route is ambiguous and the declaration is a hard error citing CALL-4 at the `ensures_clause`, `AmbiguousResultRoute`, carrying a repair [DIAG-1].
The judgment is at the declaration because the ordinal set is fixed there, exactly as [CALL-6]'s consistency judgment is: a route no reader can attribute to one ordinal publishes a fact about a value the writer did not name.

The destinations are exactly [ENT-3.S12]'s closed list, and a relation reaches a caller only there; [CALL-6] fixes the point at which each is instantiated and the point at which each is established.
For a multi-result contract, **each binder of a destructuring `let`** is the S12 destination for every published relation naming the value that lands there, ordinal i landing at binder i [GRAM-4].
An own-place match of an integer-payload Result selects its transported conditional evidence under [ENT-5]. Measured payloads and destructuring-consume binders receive their measure relations through [MSR-3]'s placement table.
A published measure of the transferred value or one of its exact owned measured descendants reaches both through [MSR-3]'s payload and destructuring placements; a measure datum carries nothing else.

[FN-10] Guaranteed self-tail calls.
The optional `musttail` atom on a `call` [GRAM-5] requires that call to transfer to the enclosing function without retaining the current activation or growing the stack for that transfer.
The marked call is the sole expression of a `return_stmt`, and its callee resolves directly to the enclosing source function at that function's own generic arguments [FN-6].
Every ordinary call and return judgment still applies, including contracts, effects, ownership, and reference validity; the marker supplies no proof and changes no result shape [FN-1, FN-8, FN-9].
After evaluating the actual arguments in their declared order, every path in every reference argument's [REF-1] path set is rooted at a reference parameter of the current function, never at storage owned by the current activation.
Every still-live owned binding, parameters included, must admit its ordinary scope release [PROV-6]. A binding with a nonempty release may be released before the transfer exactly when no live valid reference binding has a path rooted at that owner; a release is nonempty when its type's release graph [STOR-3, PROV-6] can perform an action, with a symbolic type parameter read at its written capability bound [FN-2].
The checker represents all remaining releases explicitly [DIAG-2]; after capturing all actual values, those releases run in their ordinary order, then all parameters receive the captured arguments together and execution restarts at function entry. No caller continuation, result copy, or release remains after the transfer.
A failed condition is a hard error citing FN-10 at the marked `call`, naming the failed condition and the offending argument or owner when applicable; a call to another function, including a mutual-recursion edge or a function-kind parameter, fails the direct-self condition.
An unmarked call carries no tail-transfer guarantee. The guarantee bounds only stack retained by the marked transfer, not the stack or heap used by argument evaluation, release, or the rest of the program, and does not prove termination.

## 9. Effects

[EFF-1] Row grammar: the `effects`, `effect`, `effect_path`, `epbase`, `epsuffix`, and `erange` productions of the fence below.

```wf-ebnf EFF-1
effects := "pure" | effect ("," effect)*
effect := "reads" "(" effect_path ")"
        | "writes" "(" effect_path ")"
effect_path := epbase epsuffix*
epbase := IDENT
epsuffix := "." IDENT | "." TYPEID "." IDENT | "[" IDENT erange? "]"
erange := ".." IDENT
```

An effect row lists `reads(path)` and `writes(path)`, each entry naming exactly one path, which is the one spelling of an entry [FORM-1].
A category may appear more than once in one row, and the canonical order is every `reads` entry before every `writes` entry, each in written argument order.
A row lists each path at most once per category, and a repeated entry is an EFF-1 rejection at that `effect`.
`pure` is the unique spelling of the empty row.
The root IDENT names the storage its reference parameter refers to, so a row never writes `deref`: `writes(cell)`, `reads(cell.inner.len)` [FORM-1].
Frame residency [STOR-1] is not an allocation by definition, and allocation and release carry no effect entry [STOR-8].
The spellings `external`, `blocks`, `memory`, `world`, and `capability` are not grammar atoms, effects, or reserved words. They satisfy IDENT wherever any other lowercase identifier does.

Every `effect_path` is rooted at one reference parameter of the same callable and continues through field selections, enum payload steps, measure and window-part names [TYPE-10, WIN-2], and whole-index or range positions supplied as arguments. A by-value parameter has no effect entry at all: the call site records the consumption of a `move` argument or the read of a copy argument [EFF-5]. A signature never contains an index expression; an index enters an effect only through an IDENT that resolves to a value parameter of the same callable, evaluated once at the call. A root resolving to a local, a result binder, a by-value parameter, or a non-parameter declaration is an EFF-1 rejection. A bare parameter names the complete state that parameter supplies; a field path names only that structural substate.

The row describes observations and changes of ordinary Whitefoot state. It does not distinguish memory from outside state and does not describe a host scheduling mechanism. Opaque nominals and aggregates all use the same path, exactness, call-substitution, and ownership rules. No type or path carries a writer-visible capability category.
`reads(path)` means the operation observes that state, so a `reads` entry states every observation at that path and below it. `writes(path)` means the operation writes, replaces, moves out of, or frees the storage at that path and everything below it, so a `writes` entry states every access at that path and below it.
One row states each access once. An entry is covered by another entry of the row when it is a `reads` entry and the other is a `writes` entry whose `effect_path` followed by zero or more `epsuffix` is its `effect_path`, or when both are entries of one category and the other's `effect_path` followed by one or more `epsuffix` is its `effect_path`: `reads(p.x)` beside `reads(p)`, and `reads(p)`, `reads(p.x)` or `writes(p.x)` beside `writes(p)`, are each covered. A covered entry is an EFF-1 rejection at that entry's `effect`, carrying the first entry in written order that covers it.

[EFF-2] A concrete function declaration exhibits the union of its resolved body accesses and calls.
The body contribution is syntactic over the complete function body. Erased definitions and contracts [FN-8, FN-9], proofs, and the compiler-owned captures, comparison and update of a counted loop contribute nothing.
Every source occurrence contributes even in an FN-8 uninhabited instance. No path condition, constant evaluation, proof, optimizer result, or backend reachability narrows the conservative structural normal-control graph [FN-1].
An allocation or a release contributes no path [STOR-8].

At a function-kind parameter or named member call [FN-5], the instantiated formal's written row is the authoritative callable boundary. Its paths undergo the same resolved-place projection and ENT-5 support kill as ordinary callee paths; the selected actual's narrower covered row never substitutes for that boundary. Generic spelling checking retains formal-rooted contributions under the written bounds, including an owned parameter that later has a copy instance. Concrete rechecking verifies the fixed containing declaration's row under the same formal interface and independently checks each actual against FN-4 and its own row. No adapter, dummy read, widened runtime access, or law supplies an exhibited path.

Every read or write is attributed after ordinary place resolution [REF-1]. A reference parameter's effect path names the path the reference names rather than the reference variable. An access rooted in a formal contributes the most precise static path EFF-1 admits for that resolved place; a dynamic element or range maps to its nearest statically nameable enclosing path.
An unknown descendant cover maps to its nearest statically nameable containing prefix by the same rule; no wildcard is written in an effect row. Substitution retains the actual's captured target and the declared finite suffix for [OWN-7], rather than treating the actual as equal to its anchor.
Moving, returning, or structurally repacking an owner contributes no read or write merely by transferring it. After transfer into a local binding or aggregate field, subsequent accesses are attributed to that destination's resolved storage. A whole-place replacement changes the value at that place without changing the place's effect root. No effect root follows an owned value through moves, results, or replacements.
A const root and `immutable-const` contribute no read effect. An access rooted only in local storage contributes no enclosing formal-rooted effect, including through a local reference; the checked access and its ordinary footprint remain.
A write is admitted only where ordinary ownership already admits it [SET-1]. An effect path grants no permission.

At a call, each declared effect path selects its root formal's actual argument and appends its suffix to the actual's path [EFF-5]. The resulting ordinary read or write footprint is retained at the call. A projection rooted in a current formal contributes that formal's corresponding path; a projection rooted only in local storage contributes no enclosing path.
The actuals' call data are captured after argument evaluation and before the call. Each projected write kills overlapping support under ENT-5, then CALL-6 instantiates entry terms from that call datum and exit terms from the actual's resolved return place. This substitution does not inspect the callee body and retains no owned-result or exclusive-referent ancestry.
Framing an action out of an enclosing row removes no checked action or ordinary effect footprint. Optimization is governed by EFF-3 and the source's value, ownership and control semantics.

A SET-1 commit contributes a write. SET-1's reinitialization of a complete binding already dead at statement entry keeps its no-previous-owner exception. Target and right-hand-side evaluation contribute ordinarily.
A declared entry is exhibited when the body accesses storage at or below its path. Rows are checked both ways against this complete exhibited set — every declared entry is exhibited in that sense, and every exhibited access lies under some declared entry — so undeclared-but-exhibited and declared-but-unexhibited are both EFF-2 errors. A declaration with no exhibited contribution writes `pure`, whether or not it carries erased contracts.
A PRE-1 or PRE-2 function signature is the ordinary declared boundary; its supplied definition must satisfy the same boundary [SCOPE-3, PRE-1, PRE-2]. No source body is fabricated for it and no alternate effect rule applies to its calls.

[EFF-3] A call whose row is `pure` and which allocates nothing licenses deduplication and reordering with equal arguments.
The ground is that the heap a call takes from is finite and a duplicated take is a different program [STOR-8].
Elimination of an unused licensed call additionally requires a termination proof; v0 provides no termination checker, so unused calls are not eliminated.
The source spelling `pure` excludes state reads and state writes; it does not promise termination.
A call that exhibits `writes(path)` may remain observable even when its result is unused. A call on fresh local state retains that instantiated effect even though it frames out of the enclosing signature. No optimization may erase, duplicate, speculate, or reorder either call unless ordinary effect-path overlap, closed-state, escape, ownership, control, result, release, and surviving-observer proofs establish the exact transformation.

[EFF-4] Accepted source has no writer-reachable abort effect, exception, unwinding edge, or hidden runtime proof fallback.
Every proof failure rejects the source before lowering.
Unavailable resources and a trusted-computing-base failure remain outside the source effect system under [SCOPE-3]; none creates a writer-visible effect spelling or an alternate successful source judgment.

[EFF-5] At a call, the actual argument paths are substituted into the callee's row: each `effect_path` rooted at reference parameter i takes actual argument i's path, and each IDENT index or range endpoint takes the value its own argument supplies, evaluated once at the call.
The substituted effects are then compared in pairs: every two effects that different arguments supply, and every two effects that one reference argument supplies whose declared paths do not overlap at every position.
Two declared paths rooted at one parameter overlap at every position when one is the other followed by zero or more `epsuffix`, or when the first pair of steps at which they differ is one that [OWN-7] and [WIN-2] fix as overlapping whatever values the positions take: two payload steps naming different variants, an index or range position and `.last` or `.filled`, `.next` and `.free`, or `.last` and `.filled`. Steps compare as written, so two index or range positions are the same step exactly when they name the same value parameters. `reads(v[i]), writes(v[j])` and `reads(v[lo..hi]), writes(v[j])` are therefore compared, while `reads(v), writes(v[j])`, `reads(r[i]), writes(r.last)`, `reads(r[lo..hi]), writes(r.filled)` and `reads(p.A.x), writes(p.B.y)` are not.
Two effects one argument supplies whose declared paths overlap at every position reach that storage through that one parameter, and the callee's body is checked against both [EFF-2]; their writes still kill every caller fact whose support they overlap [CALL-5] and invalidate references under clause 3.

1. Two compared effects on overlapping paths [OWN-7] where at least one is a write must be proved disjoint, by different roots, by index and range positions proved apart, or by an index or range position proved apart from a window's `.next` or `.free` [WIN-2], and are otherwise a hard error citing EFF-5 at the complete `call`, carrying both substituted paths and a repair [DIAG-1]. Read/read overlap is admitted. The built-in `swap` [OP-11] is the one operation whose two arguments may name the same place.
2. A by-value argument contributes a consumption (`move`) or a read (copy) of its place to this same comparison.
3. Every live reference, including an actual argument, receives the invalidations of every substituted write under [REF-2]. A content write at or below its captured target preserves it; argument membership does not protect it from another actual's destructive ancestor write.

A function body is checked against its own row: every statement's effect and every callee's substituted row must be exactly covered by the declared row [EFF-2].
Function-typed parameters are generic parameters [FN-2]: a call through one uses the formal's row, each instance is a direct call, and a supplied function may refine the formal signature under [FN-4].
Recursion is checked through contracts, never by unfolding bodies.

## 10. Errors

[ERR-1] Recoverable errors are values: prelude `Result<T, E>` and `Option<T>` (§14), dispatched by `match`.
No exceptions, no unwinding, no panic values.

[ERR-2] Every `match` is exhaustive over declared variants; there are no wildcard arms.
Bool exhaustiveness is carried by `if`: an else-free `if` is the empty-alternative form, an `if` with `else` covers both, and a Bool-scrutinee `match` is rejected at GRAM-6.
The asymmetry is deliberate and content-driven: the empty then-block is admitted while the empty else is not, because the else-free form is the one spelling of the empty alternative.
Variant addition surfaces site-enumerated edit lists (toolchain contract).

[ERR-3] Propagation: `let x = propagate e;` requires `e : own Result<T, E>` and the enclosing function's return type `own Result<U, E>` (same E — no conversions, TYPE-4); x's derived mode and type are `own T` [TYPE-5].
The propagation operand is a consuming context.
A non-place Result expression is its owned temporary.
When `e` is a direct bare place of affine `Result<T, E>` type rooted in a live own-mode binding, propagation consumes that place exactly once under [OWN-1] without requiring a written `move`; a partial place consumes its whole root and retains the ordinary residual cleanup.
An explicitly written `move p` retains its ordinary OWN-1 meaning.
A place reached through `deref` of a reference, a reference binding used without `deref`, a dead root, and an outer affine root consumed inside a loop retain their REF-2, TYPE-7, OWN-1, and OWN-11 judgments; ERR-3 grants no read-through, move-through-reference, revival, copy, or loop escape.
The operand is consumed before the result tag is dispatched.
On `Ok(v)` propagation binds v; on `Err(err)` the function returns `Err(err)`, and the checked program attaches an auto-derived context record `(function, node_path)` to the propagation edge — zero hand-written tokens per site.
For an enclosing FN-9 `Ok` route, that automatic error return is unselected and publishes no normal-result relation.
This is Result propagation, not an exception construct or a scope in which an exception may be thrown.

[ERR-4] Classification: expected environment and input failures represented by an operation contract are values (`Result`); unproved function, operation-domain, allocation-size, bounds, layout, address, and target-domain obligations attached to source execution are source rejections.
A use of an invalid reference and an unproved overlap at a call or in a reference-validity judgment are source rejections of the same class [REF-2, EFF-5].
Unavailable external resources and trusted-computing-base failures remain outside the source outcome model under [SCOPE-3].
An operation's classification is fixed by its table row and attached static obligations, never by call-site preference.
The overlap permissions of [PAR-1, PAR-2] are implementation permissions over an already accepted sequential program, not source obligations in this version: absence of a complete permission derivation retains sequential lowering and never rejects the source.
If an implementation does select an overlapping lowering, every premise of that permission must be discharged before emission; a failed premise cannot be repaired by a runtime check or partially parallel fallback.

## 11. Programs and modules

[PROG-1] A program is a module program, whose graph [MOD-1] registers its modules, or a source bundle [PROG-2], which forms one module. Every language name is declared by one of the program's modules, by a standard library module it selects [MOD-10] or by the prelude [PRE-1].
The standard library is the one package a program reads besides its own: there is no source include, other external package, glob import, source-path search, dynamic loading or reflection, and a module program reads exactly the records its graph registers [MOD-2] and those of the standard library modules it selects [MOD-10].
Build and link supply definitions for ordinary declarations and select the invocation [PROG-3]; implementation language and linkage are not source semantic inputs.

[PROG-2] A source bundle is one ordered nonempty sequence of logical source records.
Each record contains one logical path and one exact source-byte sequence.
A logical path is an ASCII relative path made from one or more nonempty components separated by exactly one `/` byte, with no leading, trailing, or repeated `/`; each component contains only ASCII letters, ASCII digits, `.`, `_`, or `-`, and no component is `.` or `..`.
Path spelling is preserved exactly and compared case-sensitively.
An empty record sequence, an invalid logical path, or two records with the same logical path is an input-envelope failure, not a source-language rejection.
Record order is exactly the order in the bound invocation; no path sort, host enumeration order, or other reordering is applied.
Within that bound unit, a source record is identified by its zero-based ordinal, exact logical path, and exact source bytes.
A source bundle forms one module, the root module `pkg`, whose every record is an implementation record [MOD-2]: it has no interface record, so no declaration of it is public [MOD-6], it may name every standard library module as a module whose graph row lists them all [MOD-5, MOD-10], and every other module rule applies to it unchanged.
Its first record may begin with `program no_heap;` [GRAM-2, STOR-8].

[PROG-3] Execution starts by an ordinary call to the build-selected function with arguments matching its ordinary signature: a source bundle's function `main`, or a module program's entry [MOD-9]. The implementation must establish the arguments' declared types, ownership, and requirements before making that call, exactly as any caller must [FN-1, FN-8].
A program may take the standard library's `Inputs` struct [PRE-2] or use any other admitted signature; the heap is ambient and has no source spelling [STOR-8].
A source bundle declares that it uses no heap by writing `program no_heap;` as the first `item` of its first source record [GRAM-2, PROG-2]; a module program states that requirement on an entry [MOD-9]. What either withdraws is [STOR-8]'s.
The ordinary call ABI, result transfer, and scope-exit rules apply to both Whitefoot and linked definitions. Implementation engines may wait or schedule internally only while preserving this same boundary. The build interprets returned values and performs any invocation teardown outside the source call; neither operation adds a source effect or changes acceptance.
Resource unavailability before that call and trusted-computing-base termination remain outside the source outcome guarantee [SCOPE-3].

[MOD-1] A module program is selected by one graph record, `modules.wfg`, whose directory is the package root; no other location, working directory or search selects it.
The graph record derives `graph_file` [GRAM-2] and passes the lexical, grammar and canonical [FORM-2] stages as every source record does [DIAG-1].
Each `module_row` registers one module: `pkg` alone registers the root module, and `pkg::a::b` the directory `a/b` below the package root; registering a directory registers none of its ancestors or descendants.
The row's bracketed list is the module's exact set of direct dependencies: every `pkg` dependency is an earlier row, which is what makes the graph acyclic, and every `std` dependency a standard library module [MOD-10].
A row that registers an already registered module, a dependency naming the row's own module, a dependency listed twice in one row, and a `pkg` dependency that no earlier row registers are each a hard error citing MOD-1 at that `module_path`.
Each `entry_decl` takes a fresh entry name and names one function by a `module_path` whose last component is the function's name and whose other components name its module; a repeated entry name, or an entry whose module is not registered, is a hard error citing MOD-1.
Written row and dependency order is preserved and never inferred; the meaning of a graph does not depend on it.

[MOD-2] A registered module's records are the file `module.wfm` in its directory, its interface record, and the directory's direct regular files whose names end in `.wf`, its implementation records; a child directory's files belong to no ancestor module.
The bound unit orders modules by row and each module's interface record before its implementation records, which follow in byte order of their names; a record's logical path is its path below the package root [PROG-2].
An absent registered directory, a missing `module.wfm`, a symbolic link at the package root or on the path of a module directory or record, a record name that is not a logical path component, and two entries of one directory that differ only in letter case are input-envelope failures, not source-language rejections.

[MOD-3] Each module has one declaration inventory.
Every top-level declaration of the module's interface and implementation records other than an alias enters it. A declaration of the interface record is visible in every record of the module, and a declaration of an implementation record in every implementation record, independently of record and item order, so the interface closes over its own declarations, the public declarations of the modules it may name and PRE-1 [MOD-8]; local declarations keep their lexical scopes [TYPE-6].
The inventory declares each spelling once in each domain [TYPE-6], and an interface declaration of a function together with its one definition is one declaration of that function [MOD-7].
A lowercase top-level declaration whose spelling extends the declaring module's path to the path of a registered module is a hard error citing MOD-3 at the declaration: the two would occupy one qualified name.
Visibility grants no value, proof or layout: constant dependencies, group expansion, finite instantiation and layout are judged by their own rules [CONST-2, FN-3, FN-6, STOR-6].

[MOD-4] A record's items begin with its `heap_decl` when it has one [GRAM-2], then its alias header, which holds every `alias_decl` of the record; an `alias_decl` that follows an item that is neither an `alias_decl` nor the `heap_decl` is a hard error citing MOD-4 at the alias.
An alias binds its IDENT or TYPEID, in its own record alone, to the identity its complete `pkg` or `std` path names in the alias's module: a lowercase alias binds a registered module, or a function or named const of one; an uppercase alias binds a struct, enum, interface or binding of one, or a variant of a nongeneric enum of one, written as that enum's path followed by the variant TYPEID.
A path whose registered module the alias's module may not name is a hard error citing [MOD-5] at the alias. Otherwise a path naming nothing an alias of that case binds where the alias is written, or a variant of a generic enum, is a hard error citing MOD-4 at the alias: of another module's declarations an alias binds only public ones, and an alias of an interface record sees what that record sees [MOD-3].
An alias takes its target's lookup classes and collision domains, and a module alias the lexical-IDENT domain, and it collides as a declaration of its record's scope does [TYPE-6]: with another alias, with a declaration of its module's inventory, with a PRE-1 declaration, and with a local declaration that would shadow it.
Every use of an alias resolves to its target's own identity. An alias is never public, never another alias's target, and grants no graph edge; an unused alias receives the same checks.

[MOD-5] A qualified `type`, `callee`, construction or destructuring target begins with a module prefix: `pkg`, `std`, or a module alias of the record, followed by lowercase path components [GRAM-3, GRAM-5].
The prefix names the registered module with the resulting path, a `std` prefix the standard library module with it [MOD-10]; a prefix naming no registered module, or rooted at an IDENT that is not a module alias of the record, is a hard error citing MOD-5 at the prefix.
A record may name its own module and the modules its module's graph row lists; naming any other module, by a prefix or an alias, is a hard error citing MOD-5 whatever transitive or ancestral relation connects them.
The final name resolves in the named module's inventory in the grammar-selected domain [TYPE-6]; a name that inventory does not declare is a hard error citing MOD-5, and so is a declaration of another module that is not public [MOD-6].
One accessibility rule serves executable code and annotations: every name and field selection in a body, a contract clause or `define`, an invariant, a `use` premise, an effect row and a function-kind formal must be accessible where it is written.
A private declaration or field is accessible only in its declaring module; a public one also in each module whose graph row lists its declaring module. PRE-1 declarations and members keep their ordinary availability.
A construction names every field [GRAM-8], so a construction outside the declaring module requires every field to be public and supplies no readonly field [TYPE-2]; a destructuring consume or an `arm` outside it binds only public fields, covering the rest with `..`. Each inaccessible selection, binding or construction is a hard error citing MOD-5 at its `psuffix`, `effect_path`, `fieldbind`, `arm`, `call` or `cvalue`.

[MOD-6] `public` is written only in an interface record, on a top-level declaration, a struct `field` or an enum `vfield`; a declaration or field without it is private to its module, and a public enum's variants are public.
`public` in an implementation record or a source bundle, and `public` on a field or payload field of a private type, are each a hard error citing MOD-6 at that `item`, `field` or `vfield`; so is `readonly` on a field that is not `public` [TYPE-2].
A public function's parameter and result types, bounds, effect row, contract clauses and function-kind formals, a public field's type and a public const's type name only declarations and fields its module's clients can access: public declarations of the module, public declarations of its dependencies, PRE-1 declarations, and public fields along every written path [MOD-5]. A name or field selection there that its clients cannot access is a hard error citing MOD-6 at that use, in the declaring module as elsewhere; a private function's contract and row may name its module's private declarations and fields.

[MOD-7] A function item of an interface record is a declaration without a body that ends in its `doc` entry, and a function item of an implementation record or a source bundle writes its body; a function item of any other form is a hard error citing MOD-7 at the `fn_decl`.
An interface function declaration has at most one definition among its module's implementation records, and the definition repeats the declaration's complete signature and contract without `public`: equal parameter order, names, kinds and types; equal result order, names and types; equal generic order, kinds and bounds; an equal effect row; and equal requires and ensures clauses in written order, compared after resolving every alias to its identity and renaming generic and contract-local binders consistently.
`doc` entries are not compared, and a `define` whose expression is one atom is erased sharing: a use of it corresponds to that atom written in place, while a `define` of a compound expression corresponds only to a `define` of the same expression at the same position. A definition that differs is a hard error citing MOD-7 at its `fn_decl`.
A struct, enum, interface, binding or const that an interface record declares has that one definition, used by every record of the module; a declaration of the same name in an implementation record is a redeclaration [TYPE-6].

[MOD-8] A module's source verdict covers every judgment on its records and depends only on them, the graph, the prelude and the interfaces of the modules it may name; it never depends on another module's implementation records.
An interface function declaration without a definition is pending: callers use its written boundary as they use every callee's [FN-8, FN-9], and it blocks only composition, lowering and publication.
A program composes when every selected module's verdict holds, every declared function has its definition, which is a body in its module's implementation records or the definition the build supplies for a host function [PRE-2], every required concrete instance checks [FN-2, FN-6] and every target requirement of its entry holds [MOD-9]. Composition judges the selected modules' verdicts in row order before its own conditions and reports the first rejection. A rejection raised while checking a required concrete instance is reported at its template's source, in the module that declares the template, and names a call that requested the instance.

[MOD-9] A named entry selects one public, ordinary, nongeneric function by its full path, and a build may select any ordinary nongeneric function of a registered module as an unnamed entry; the build calls the selected function under [PROG-3], and an entry naming no such function is a hard error citing MOD-9, at a named entry's `entry_decl`.
An entry's `no_heap` states the no-heap requirement over that entry's execution closure [STOR-8]; entries of one graph share its modules and proofs and impose no requirement on one another.
A module program's record never writes `program no_heap`: a `heap_decl` in one is a hard error citing MOD-9 at the `heap_decl`.

[MOD-10] The standard library is one package the toolchain supplies: a graph record and the records of the modules it registers, judged by MOD-1 to MOD-9 as every package is, in whose records `pkg` names the standard library itself.
Every other package names it `std`: `std::a::b` is the module the standard library's graph registers as `pkg::a::b`, and a standard library record's logical path is its path below the standard library's root after the component `std`.
A `std` prefix in a standard library record, a `module_row` or `entry_decl` path that begins with `std`, and a `std` dependency that names no standard library module are each a hard error citing MOD-10 at that path.
A module program selects the standard library modules its rows list, and a source bundle those its records name by a `std` path; each selected module selects its dependencies in the standard library's graph. The bound unit orders the program's modules first [MOD-2], then the selected standard library modules in the standard library's row order, so a program record's ordinal never depends on which standard library modules it selects.
A standard library module's verdict depends only on the standard library's records and graph and the prelude [MOD-8], never on the program that selects it, and a check or composition reads a standard library module's records only when it selects that module.
The host modules are the standard library modules PRE-2 fixes; every other standard library module is written in Whitefoot and checked as every module is.

## 12. Diagnostics and checked compilation (toolchain floor)

[DIAG-1] Every source-language rejection cites exactly one numbered language rule and exactly one location from this closed sum:

1.
`SourceBytes(SourceCoordinate)` when no offending canonical-tree node exists or the defect belongs only to a source boundary;
2.
`SourceNode(NodePath, SourceCoordinate)` when one source-backed canonical-tree node is the offending node; or
3.
`BundleRoot(NodePath, BundleRootExtent)` for a whole-unit defect with no offending source declaration.
This form requires the empty root `NodePath` and carries no source-local byte interval.

`SourceCoordinate` is `(source_ordinal, byte_start, byte_end)` in the bound [PROG-2] unit.
Its byte interval is checked, half-open, and contained in that exact source.
End of source is the zero-width interval whose two offsets equal the source byte length.
`NodePath` is the sequence of zero-based child ordinals from the finalized compilation-unit root; the root path is the empty sequence.
Every source-backed node has one checked source-local extent.
In `SourceNode`, the rule-selected coordinate lies within that extent (`node_start <= byte_start <= byte_end <= node_end`) but need not equal the complete extent; the path identifies the existing offending or owning node while the coordinate identifies its exact offending subinterval or boundary.
`BundleRootExtent` is the exact ordered byte-extent sequence defined by [PROG-2], not a cross-source byte span.
A diagnostic never fabricates a node or node path.
A nested-place rejection additionally renders the offending access-path segment.

The frontend selects defects stage by stage.
Each stage scans every source in source-ordinal order and byte order, stops at its first defect, and the next stage begins only if the preceding stage succeeds for every source.
The stage order is: raw lexical formation; terminal membership; grammar derivation; then canonical [FORM-2] rendering.
Within one grammar decision, production definitions rank by their first appearance in this specification, and alternatives rank left to right as written.
Numbered rules rank by their first appearance in this specification.

Raw lexical scanning is quote-aware and reports the first defect at its cursor.
If the actual byte sequence beginning at the cursor does not begin one complete well-formed UTF-8 encoding of a Unicode scalar value, the first byte always cites [FORM-2] and spans that one byte, including when the cursor is inside a STRING candidate.
Outside a STRING candidate, a byte in `0x00..0x1f` other than LF, or byte `0x7f`, cites [FORM-2] and spans that byte.
An exact `//` or `/*` prefix outside a STRING candidate cites [FORM-4] and spans those two bytes.
A `@` not followed by `[a-z]` cites [FORM-3] and spans only the sigil.
Any other ASCII byte that cannot begin a specified token cites [FORM-1] and spans that byte.
Any valid non-ASCII scalar outside a STRING candidate cites [FORM-1] and spans its complete UTF-8 encoding.

After an opening `"`, `//` and `/*` are ordinary raw STRING bytes and never comment prefixes.
A final backslash cites [FORM-5] and spans only that backslash.
A backslash followed by an ASCII byte other than `\`, `"`, or `n` cites [FORM-5] and spans both bytes.
If the actual byte sequence beginning at a backslash's follower does not begin one complete well-formed UTF-8 encoding of a Unicode scalar value, that follower instead cites [FORM-2] and spans only its first byte; if the follower begins a valid non-ASCII scalar, [FORM-5] spans the backslash and that scalar's complete UTF-8 encoding.
A raw ASCII byte outside the permitted STRING interior set cites [FORM-5] and spans that byte.
At any other STRING cursor, if the actual byte sequence beginning there does not begin one complete well-formed UTF-8 encoding of a Unicode scalar value, [FORM-2] spans its first byte; a valid non-ASCII scalar instead cites [FORM-5] and spans its complete UTF-8 encoding.
If no unescaped closing quote occurs and no earlier defect applies, the unterminated STRING cites [FORM-5] and spans from its opening quote through end of source.
Terminal membership uses the complete context-free predicate set required by [GRAM-1]; a token with no matching predicate cites [FORM-3] or [FORM-5], whichever rule owns the rejected spelling.
Every lexical, terminal-membership, or grammar rejection uses `SourceBytes`; its coordinate is the exact interval above, the exact offending token interval, or the zero-width end-of-source interval defined above.

Every grammar production and external terminal predicate is owned by the numbered rule containing its unique definition.
A source-EBNF decision is a `|`, `?`, `*`, or the continuation decision of `+`.
Its stable identity is the zero-based ordinal of its production by first definition in this specification followed by the zero-based EBNF child-index path from that production's root.
Its arms retain source order; a consuming arm precedes an exit arm.
The strong-LL(2) analysis required by [GRAM-1] supplies every arm's `SELECT_2` rows.
Every predicate in a row retains its source-EBNF provenance and whether it came from inside that arm or from the arm's caller continuation.
Lookahead is padded to two positions with `SOURCE_END`.

Recognition selects an arm only when that arm has a full two-position row match.
Two matching arms are a [GRAM-1] specification defect, not a precedence rule.
Whenever no row matches, the diagnostic machine computes each arm's score: the greatest proper-prefix length, zero or one, by which any of that arm's rows accepts the actual two-position lookahead.
Let `m` be the greatest score at that frontier.
The failure boundary is the actual lookahead token at position `m`, or the zero-width end-of-source coordinate when that position is `SOURCE_END`.
The maximal-prefix rows are every row with score `m`.
The expected-terminal set is the distinct predicates at position `m` in those rows, ordered by their first terminal occurrence in the grammar; written terminals precede `SOURCE_END`.
A direct terminal mismatch is the same calculation with one row and has a singleton expected set.

At every no-row frontier, the following closed attribution rows are tested in order before diagnostic traversal descends.
The first matching row stops traversal.
A row retains the frontier expected-terminal set and coordinate unless that row names a replacement.

1.
If the boundary token is one member of four consecutive actual tokens `IDENT "." IDENT ("("|"::")`, that dotted call-or-targs spelling cites [FORM-3].
Its coordinate is the complete interval from the first IDENT through the second IDENT.
An allowed suffix would already be one maximal OPNAME token, while a field place cannot be called or given targs.
This bounded diagnostic window may include already recognized tokens, performs no operation-table or name lookup, consumes nothing, and does not enlarge recognition's two-token lookahead.
2.
If source-EBNF provenance reaches or would next enter an `atom` occurrence in `atom_list`, `fieldinit`, an `infix` operand, the subscript offset, or either endpoint of a `for_stmt`, and the two actual tokens at the start of that occurrence are `(IDENT, "(")`, `(IDENT, "::")`, `("pkg", "::")`, `(OPNAME, "(")`, `(OPNAME, "::")`, `(TYPEID, "(")`, `(TYPEID, "<")`, or `(TYPEID, "::")`, the rejection cites [GRAM-9]; in an infix-operand occurrence, a two-token start whose second token is an `infix_op` or `compare_op` token — the forbidden nested-infix start — likewise cites [GRAM-9].
These are exactly the `call` and constructor `call` starts forbidden in an atom-only position; no name lookup participates.
Its coordinate is the complete interval from the first through the second token of that forbidden call or construct start.
3.
If the boundary token has the raw shape admitted by an expected external predicate before that predicate's explicit spelling restrictions, and fails only those restrictions, the rejection cites that predicate's owner.
This includes an exact fixed lowercase grammar word in an IDENT slot and a numeric-form token missing FORM-5 membership.
For the rest of this row, a boundary-name candidate is one of `IDENT`, `TYPEID`, `LABEL`, or `OPNAME` when the boundary token satisfies a different predicate in that four-member set.
A transparent mandatory-name path begins at a position-m predicate occurrence in one of the current frontier's maximal-prefix `SELECT_2` rows, using that occurrence's source-EBNF provenance; it never restarts at the failed decision's head.
The path ends at a boundary-name candidate which is its first nonnullable unconsumed terminal.
It may traverse a group; a sequence whose preceding children are completely matched; or a production reference whose expansion before that terminal contains no source `|` decision.
At a `?`, `*`, or `+` continuation it examines both the consuming direction and the exit/caller-continuation direction recursively.
The nullable decision is transparent only when no direction's first nonnullable unconsumed predicate accepts the boundary token.
Every direction that recursively reaches a boundary-name candidate contributes a path; a direction that instead reaches a different nonmatching predicate contributes none.
A path stops at any source `|` or at any nonnullable terminal before its candidate.
A name-slot mismatch exists only when at least one transparent path exists and every transparent path ends in the same name predicate.
It cites [FORM-3].
Thus traversal reaches a direct name, a name inside a consuming list arm, or a name after one or more skipped nullable prefixes such as `doc?`, but cannot tunnel through structural choices such as `item`, `stmt`, `expr`, `atom`, `callee`, `pbase`, `targ`, `contract_define`, `requires_clause`, `ensures_clause`, `result_route`, or `atom_list | fieldinit_list`.
If several external predicates qualify under the first sentence, their owners rank by first rule occurrence in this specification.
4.
At the `program` `item*` or `item` entry, any `stmt*` or `stmt` entry, or any of the three repeated-entry frontiers inside `contract_block`, an IDENT-headed lookahead accepted by no complete construct row cites [FORM-1] as an unknown construct.
Its coordinate is the exact interval of that first IDENT token.
A lookahead that selects a defined construct is not covered by this clause.
5.
At `program`'s `item*`, after any complete item prefix, if the first actual token predicate matches no consuming `item` row, the token is an unexpected leftover, the expected-terminal set is replaced by only `SOURCE_END`, and the rejection cites the owner of `program`.

If no attribution row applies and exactly one arm has a score strictly greater than every other arm, diagnostic traversal descends into that arm only when every next expected predicate in that arm's maximal-prefix rows came from inside the arm rather than from its caller continuation.
Otherwise the current frontier is the stopping point.
A tie is never guessed.
Traversal through the selected arm follows the same source EBNF and repeats this procedure.
It cannot cross from a completed arm into its continuation, make a failed row valid, insert, delete, recover, or skip a token, or create a derivation or node.
It is used only after recognition has failed; reaching a successful end instead of a stopping point is a compiler-invariant failure.

At a stopping decision the total fallback cites the owner of the production containing that source-EBNF decision; a direct terminal mismatch cites the owner of its containing production.
A recursive-descent or table-driven implementation must report the result of this same source-EBNF diagnostic machine.

A source-local trivia gap is the complete interval between two adjacent terminal leaves after excluding those terminal bytes, between source start and the first terminal, or between the last terminal and source end.
It contains every intervening trivia item and may be zero-width; for a source with no terminal leaves, the whole source is its single gap.
The forest renderer defines the required bytes for each corresponding boundary.
A [FORM-2] mismatch is selected by the first byte offset at which the source and its complete forest rendering differ, treating the end of either byte sequence as a boundary.
Because terminal bytes have already passed lexical formation, terminal membership, and grammar derivation, that offset selects exactly one such actual-or-required gap.
Its coordinate is the complete actual gap interval, or the zero-width terminal boundary when required trivia is missing.
For a gap between two adjacent terminal leaves in the same top-level item, the location is `SourceNode` for their deepest common production-node ancestor in the finalized compilation-unit tree.
A source-leading, source-final, inter-item, or zero-item-source gap uses `SourceBytes`.
No renderer-authored owner, parser stack position, or implementation emission order participates in this choice.

An input-envelope failure, resource failure, target-layout failure [STOR-6], compiler-invariant failure, unsupported compiler capability, backend failure, or external-tool failure is not a source-language rejection, cites no language rule, and carries no expected-terminal set.

After canonical FORM-2 succeeds for every source, semantic diagnostic selection first runs [FN-8]'s contract-presence judgment over every `contract_block`.
An empty or define-only block uses `SourceNode` at that complete `contract_block`; no declaration, route reservation, or use role inside such a rejected block is classified or counted.
Grammar already fixes each admitted block's definitions-before-requirements-before-postconditions structure and excludes every statement form, so no second structural-entry filter exists.
Only complete unit-wide FN-8 admission permits the remaining resolution stages, each complete over the unit before the next begins: the [GRAM-2] heap-declaration position of a source bundle; the module forms, one scan in source order judging [MOD-4]'s alias placement, [MOD-9]'s heap declaration of a module program, [MOD-6]'s `public` and `readonly` placement and [MOD-7]'s function item forms; ordinary role classification and declaration inventory, whose event order also reports each refused alias target [MOD-4, MOD-5]; lexical resolution, including qualified references [MOD-5]; [MOD-6]'s closure of public signatures, in source order of the offending use; and [MOD-7]'s correspondence of each definition with its interface declaration, in definition source order.
A module program's graph record is formed first [MOD-1]; no module record is read before its graph forms.
Within an admitted routed `ensures_clause`, the route's leading lookup and [FN-9] route-admission subjudgment occur before lexical resolution of that clause expression; every unrelated block and event retains the ordinary global ordering.
Poison declarations and partial resolution are forbidden.
An early FN-8 rejection outranks every inventory or resolution rejection; inventory still outranks resolution even when the later-stage event has an earlier source coordinate.

A semantic role is owned by the lowest production node whose selected right-hand side directly contains the terminal that carries the role; a role reached only through a referenced child production is owned by that child.
A referenced child production means a child production node, not an external terminal predicate such as `literal`.
A semantic role may occupy a complete name terminal, a complete literal terminal, or the exact TYPEID suffix of a FORM-5 generic numeric literal `0_T` or `1_T`.
The suffix role's spelling excludes `_`, and its coordinate is exactly the suffix byte interval.
A generic numeric literal carries its lexical generic-type use on the suffix; its numeric value remains the literal judgment, not a second deferred role.
A struct TYPEID remains one declaration event producing two domain entries, not two events.

Within one owner node, distinct direct grammar-role carriers are ordered left to right by their complete carrier coordinates; distinct carriers with identical complete coordinates use the closed class order declaration, result-reservation, lexical-use, deferred-use.
The zero-based carrier index is `role_ordinal`.
`subtoken_ordinal` is zero for a role covering its complete carrier; embedded semantic name roles are numbered from one in byte order.
No current production gives one carrier both a complete-token role and an embedded role; the generic-numeric suffix retains subtoken ordinal one.
Every role has exactly one owner, class, role ordinal, and subtoken ordinal.
Every declaration event, FN-9 result-reservation event, lexical-use event, and deferred-use event has canonical key `(source_ordinal, byte_start, byte_end, NodePath, role_ordinal, subtoken_ordinal)`.
Numeric fields compare ascending.
NodePath compares lexicographically by production-child ordinal, with a proper prefix first.
Role and subtoken ordinals are consulted only after the complete path is equal.
For a complete IDENT, TYPEID, OPNAME, LABEL, or literal role, the coordinate is the complete token interval, including a sigil; only the generic-numeric suffix uses a subtoken coordinate.
The event's `SourceNode` names its owner production.
Traversal order, allocation identity, map order, logical path, and inferred type never participate.

Declaration inventory and FN-9 result reservation create candidates under this closed rank:

1. a FORM-3 reserved-name violation defined by OP-1's derived set;
2. a GRAM-10 match-binder freshness violation;
3. a declaration collision with PRE-1;
4. a compilation-root duplicate or same-lexical-scope redeclaration; and
5. a nested declaration shadowing a live declaration.

Each declaration or result-reservation event forms an inventory candidate only for an applicable rank above; an event for which no rank applies forms no candidate.
The stage selects the minimum canonical event key among events with at least one candidate and then the first applicable rank at that event.
A FORM-3 reservation payload is `(spelling, carrier_role, reserved_class, inventory_ordinal)`.
Its `spelling` is the complete declaration or result-candidate spelling.
Its closed carrier roles are function, named-const, parameter, contract-definition, let, for-binder, match-binder, result-binding, route-result, field, and variant-field.
`reserved_class` is dotless-operation or mode-word.
A dotless-operation ordinal is the zero-based first occurrence among distinct operation-family spellings, scanning OP-1 rows top to bottom and each `op` cell left to right and skipping every later occurrence of the same spelling.
A mode-word ordinal is the zero-based FORM-3 alternative order `wrap`, `defined`, `checked`, `sat`, `strict`.
Those two reserved sets are disjoint in this version.
For the GRAM-10 violation defined by TYPE-6, the payload is `(binder_spelling, paired_field_spelling, optional_earlier_binder_origin, ordered_arm_entry_live_lexical_ident_origins)`.
Earlier binders and arm-entry origins are ordered by declaration-event key.
That binder does not also create a TYPE-6 duplicate or shadow candidate.

A declaration collision payload is `(spelling, ordered_nonempty_conflicts)`; it cites INV-1 when the later declaration is an invariant name and TYPE-6 for every other declaration domain.
Conflict domains use the fixed order lexical-IDENT, nominal-type, constructor, numeric-bound, LABEL, invariant.
Each conflict contains its domain, declaration class, and `conflicting_origin`; conflicts within one domain use PRE-1 declaration ordinal first, then source declaration-event key.
A source origin is `(NodePath, SourceCoordinate, role_ordinal, subtoken_ordinal)`; a PRE-1 origin is `(PRE-1, declaration_ordinal)`, with the zero-based preorder fixed by PRE-1. Prelude declarations are admitted to every module [PRE-1].
A struct event may report both nominal-type and constructor conflicts in that order.
Rank 3 reports only PRE-1 conflicts when the same event also conflicts with source.
A PRE-1 collision points to the source declaration.
Rank 4 points to the later source declaration event.
Rank 5 points to the nested declaration, including one shadowing a source-later but whole-unit-visible function.
Every declaration-inventory rejection uses `SourceNode` at the declaration role and has no expected-terminal set.
An FN-9 result-datum reservation instead uses `SourceNode` at the owning `result_binding` or `fieldbind`, a coordinate equal to the candidate IDENT token, and the FORM-3 payload above; it creates no TYPE-6 runtime declaration or duplicate event.

If inventory succeeds, every lexical use admitted by TYPE-6, OP-1, INV-1, or PRF-1 creates one lexical-use event.
The generic-numeric suffix admits a live generic TYPEID parameter; FN-3 and FORM-5, not lexical resolution, later require its numeric bound.
Lexical resolution fixes only the declaration or operation-family target.

The closed declaration-class order is function, function-parameter, named-const, const-generic, value, generic-type, nominal-type, struct-constructor, enum-variant, numeric-bound, interface, binding, label, invariant, operation-family.
TYPE-6, OP-1, INV-1, and PRF-1 fix each lexical role's ordered admissible subset.
A use's exact-spelling candidate universe contains all compilation-root entries in its grammar-selected domain and, for non-root declarations, only entries belonging to its declaration-owner chain.
All sibling or expired lexical scopes within the same `fn_decl` owner participate so that an out-of-scope same-function declaration can be distinguished from absence.
A function-formal signature admits declarations of that signature and its enclosing declaration ancestry but not declarations owned only by a sibling member signature.
A struct, enum, interface, or function generic belongs only to that declaration and its descendants.
No local, generic, parameter, or label owned solely by an unrelated top-level declaration or function participates.
PRE-1 owner-local type parameters and fields never participate in source lookup.
LABEL uses instead follow the separate current-function rule below.

For one lexical-use event the closed lookup rank is:

1. the candidate universe has at least one declaration in an admissible class but its admissible visible subset is empty; cite the role-attribution table below and carry every invisible admissible origin in declaration-event order;
2. for LABEL only, the current function has at least one exact-spelling label but none declares a loop lexically enclosing the `break`; cite TYPE-6 and carry every such current-function label origin in declaration-event order; and
3. the visible admissible subset is empty and neither rank 1 nor rank 2 applies; cite the role-attribution table below.

| lexical-use role | rule cited by rank 1 or rank 3 |
|---|---|
| `type` TYPEID | TYPE-5 |
| numeric-bound TYPEID or the group TYPEID of `pack_use` | FN-3 |
| constructor `call` constructor TYPEID, enum-variant-only `arm` TYPEID, or `result_route` TYPEID | TYPE-6 |
| LABEL use | TYPE-6 |
| `const` IDENT | CONST-1 |
| `cvalue` IDENT | CONST-2 |
| `pbase` IDENT | TYPE-5 |
| IDENT or OPNAME `callee` | OP-1 |
| unqualified `fn_bind` or `function_arg` callee IDENT | FN-3 |
| FORM-5 generic-numeric TYPEID suffix | FORM-5 |
| affine IDENT in a `header_invariant` or `invariant_stmt` target | INV-1 |
| affine IDENT in a relation-form `use_premise` | PRF-1 |
| the named multiplicity IDENT of `proof_use` | PRF-1 |
| IDENT premise of `use_premise`, an invariant name | INV-1 |

A successful non-LABEL lookup has exactly one visible admissible target; a successful LABEL lookup has exactly one enclosing target.
A rank-1 payload is `(spelling, lexical_use_role, ordered_admissible_classes, ordered_nonempty_invisible_origins)`.
A rank-2 payload is `(spelling, lexical_use_role, ordered_nonempty_label_origins)`.
A rank-3 payload is `(spelling, lexical_use_role, ordered_admissible_classes, ordered_available_classes)`, where available classes are visible exact-spelling entries in that use's candidate universe, listed once in the closed class order and possibly empty.
Complete IDENT, TYPEID, OPNAME, and LABEL use spellings include any sigil; only the generic-numeric suffix spelling is bare `T`.
This is declaration-kind resolution, not type checking.
Across use events the minimum event key wins.
Every resolution rejection uses `SourceNode` at the use role and has no expected-terminal set.

The dependent-declaration carriers are exactly the `field` and `vfield` declarations. A `fn_sig` instead contributes a function-parameter declaration with the scope and qualified-member disposition stated by TYPE-6 and FN-3.
Each is a declaration-class carrier that produces one dependent-declaration record and one declaration event for later typed owner/member checking, but none enters a resolver lookup inventory.
The two field carriers participate in FORM-3's reservation inventory; a function-parameter name has the ordinary function-name reservation judgment.
The deferred-use carriers are the left IDENT of `fn_bind`, the member IDENT of a qualified `callee`, the first IDENT of an arm `fieldbind`, each `fieldinit` IDENT, each `psuffix` IDENT, the TYPEID and IDENT of a payload `psuffix`, and each field selected below an effect-path root.
Each produces one deferred-use record for later typed owner/member checking.
The `result_binding` IDENT and the second IDENT of a `result_route` fieldbind are FN-9-owned result-datum carriers.
The header candidate produces one reservation event with `role_ordinal` zero in its `result_binding`.
The route candidate produces one reservation event with `role_ordinal` one in its `fieldbind`, after the first IDENT's FN-9 field-owner carrier.
A result-datum reservation uses the canonical key above but is not a runtime declaration, lexical use, dependent declaration, or deferred use.
Candidates participate in FORM-3 reservation checking but provide no pbase target before FN-9 admission; they enter no owner/member lookup and no TYPE-6 duplicate or shadow inventory.
The header candidate is available only to an admitted unrouted ensures clause; after the leading route TYPEID's ordinary constructor lookup, FN-9 admits the route payload candidate only within that one routed clause.
No result datum is visible in a contract definition, requirement, function body, or different ensures clause.
The name of every `header_invariant` and `invariant_stmt` produces one proof-only invariant declaration record that uses TYPE-6's inventory and scope machinery; [INV-1] owns collision and lookup failure in this domain.
An IDENT premise of `use_premise` produces one lexical-use record querying only that domain; it can never resolve to a value declaration that happens to have the same spelling. A `proof_use`'s own IDENT is the named multiplicity and queries the value domain instead, so the two positions never compete for one spelling.
These records have no runtime declaration or value identity and participate in deterministic lexical resolution exactly at their stated scopes; OP-1 owns declaration-name reservation.
A function-formal expansion retains its written declaration and application identities rather than fabricating a second lexical spelling.
In an `arm` or `result_route`, the leading TYPEID first resolves globally to an enum variant.
Later typed checking compares that variant's owner with the scrutinee enum for an arm; a foreign arm variant cites TYPE-6.
FN-9 separately requires the route's successfully resolved variant and owner to be exactly PRE-1 `Result.Ok`.
The resolver does not otherwise accept or reject a dependent role's owner/member relation.

A missing whole-unit requirement is not fabricated as an inventory or lookup event.
Missing or duplicate interface members, field labels, and group bindings remain typed-dependent rejections under FN-3 and the ordinary field-owner rules.

Apart from FN-9's explicitly interleaved selector-admission subjudgment above, after complete lexical resolution succeeds, source semantic checking covers the complete closed unit and precedes every target-dependent check or lowering action.
A source-semantic rejection cites one numbered rule whose rejection premise the checker has established.
A required child or referenced-declaration premise that has not obtained its judgment is never replaced by a guessed parent rejection.
An unsupported compiler capability, unavailable semantic judgment, semantic-checker invariant failure, or resource failure establishes no source violation and remains a non-language failure under this rule.

A semantic rejection uses the exact location stated by its cited numbered rule or by a more specific row below.
When neither states one, it uses `SourceNode` at one existing canonical node that directly supplied an immediate source premise of the failed judgment, with a `SourceCoordinate` equal to that node's complete checked half-open source extent.
Which such participating node is selected is implementation-defined.
A whole-unit requirement with no offending source declaration uses `BundleRoot` exactly as already defined.
Post-resolution semantic rejection never uses `SourceBytes`, fabricates a node, or carries an expected-terminal set.

Two or more simultaneously established post-resolution semantic rejections whose immediate offending source premise is the same use of the same canonical node are one rejection event, and that event cites the established rule whose definition appears first in this specification; a rule whose own text states that it forms no candidate in that situation [FN-1] is not among the established rules, and the event's location follows the cited rule.
The order among rejection events at distinct nodes is implementation-defined.
One compiler executable invoked on the identical bound unit with identical options and sufficient resources must use one stable deterministic traversal and produce the same first rejection event.
Selection may not depend on allocation identity, unordered-container iteration, worker scheduling, or backend traversal.
Different conforming implementations may report different first established rejection events at distinct nodes; the same-node citation above is fixed for every conforming implementation, and neither freedom changes the accepted-program set or checked-program authority.

Semantic success is failure-atomic at its publication boundary.
Private scratch judgments may be constructed in any deterministic order, but no checked program, lowering input, optimization fact, partially accepted declaration, or other semantic authority is published unless every applicable source-side numbered-rule judgment through complete-unit semantic checking succeeds.
Target-layout checking under [STOR-6] occurs only after that publication and produces no source rejection, rule citation, or semantic authority.
Target-stage failures are outside source-language rejection ordering.
Backend, linker, runtime-environment, and external-tool failures remain non-language failures [DIAG-1].

After complete lexical resolution succeeds, FN-3 validates the complete source-ordered interface and binding tables before FN-4 publishes any binding.
A repeated interface member rejects at the later `fn_sig` and its complete extent.
A malformed interface application, wrong-kind argument, or binding expansion cycle rejects under FN-3 at the application or binding that supplies the failed premise; the cycle diagnostic names its declarations.
An unknown, repeated, extra, or out-of-order binding rejects at the offending `fn_bind` and its complete extent.
A missing binding rejects at the complete `binding_decl`.
A signature, effect-coverage, or structural-contract mismatch rejects under FN-4 at the offending `fn_bind`, or at the raw function argument when no named group is involved.
Unresolved names retain their earlier resolver-owned rejections; no binding check guesses their meaning.
The post-resolution order among independent candidates remains the implementation-defined deterministic order above.
An invalid group publishes neither a partial interface nor a partial binding vector.

For a call to a callee class that carries written arguments — a user-generic `fn` [FN-2], or a retained-argument table operation [TYPE-5] — a missing, wrong-kind, wrong-count, or wrong-domain argument, or a missing operand, uses `SourceNode` at the `call` node and that node's complete source extent.
For an operation spelled infix, a wrong operand domain or a missing operand uses `SourceNode` at the `infix` node and its complete extent.
An extra operand or every wrong exact operand type other than the TYPE-7 implicit-read case uses `SourceNode` at the first offending `atom` node in source order and that atom's complete extent — for [OP-2]'s operand-agreement error, the second operand atom.
The cited rule is the rule selected by the callee's class: [FN-2] for a user-generic call, and, for a table operation, the rule [OP-2] selects — OP-1 or TYPE-5.
The TYPE-7 case follows that rule and the general participating-node location above.
A table-operation call written with a `fieldinit_list` instead of positional operands cites GRAM-11 using `SourceNode` at the `call` node and its complete extent.
A result mismatch is located and attributed only by the consuming construct as stated in OP-2.

An [FN-8] ordinary-call requirement judgment begins only after every earlier callee, concrete-instantiation, argument, type, borrow-feasibility, and actual-expression-obligation judgment named by FN-8 succeeds.
An unproved or refuted instantiated goal is one hard rejection citing FN-8 with `SourceNode` at that existing `call` node and `SourceCoordinate` equal to the call node's complete checked half-open source extent.
Its deterministic payload contains the concrete callee instance, the failing `requires_clause` NodePath, the complete instantiated typed goal, and exactly one disposition, `unproved` or `refuted`.
When the payload contains an occurrence-local call-argument evaluated-value datum, it additionally renders that datum as `argument #N pre-transfer value`, with N the zero-based argument ordinal.
A concrete generic instance that changes a substituted type, const, or datum changes the payload goal and is judged independently.
This rejection is never replaced with a runtime fallback or reported at the callee declaration.

An [FN-9] result-datum admission subjudgment begins only after [FN-8] contract admission, FORM-3 result reservation, the route's ordinary leading-variant lookup when present, and concrete [FN-2] signature substitution.
Admission through freshness precedes lexical resolution or semantic checking of the owning `ensures_clause` expression; the remaining clause, selected-return, and proof judgments begin only after that expression resolves and the surrounding function's ordinary semantic judgments required by the failed premise succeed.
For an unrouted clause, test in this fixed order: result mode/type determined by its declared `rtype` and fragment class; header result-candidate freshness against every declaration live in the clause.
For a routed clause, test in this fixed order: whole-result mode/type determined by its declared `rtype` and `Result` class; resolved variant owner and exact `Ok` identity; the written field against the variant's sole declaration-order field; route-candidate freshness against that field, the header result candidate, and every declaration live in the clause.
A result, class, owner, variant, or missing-field failure uses `SourceNode` at the complete `ensures_clause` or its `result_route` when present.
An extra, misspelled, or out-of-order field uses `SourceNode` at the complete `fieldbind`.
A candidate equal to its paired field or another live candidate or declaration uses `SourceNode` at its owning `result_binding` or `fieldbind`, with coordinate equal to the candidate IDENT token.
Those are FN-9 events, not GRAM-10 or TYPE-6 duplicates.
An unresolved leading route TYPEID remains the earlier TYPE-6 lexical-use rejection and forms no FN-9 candidate.

After result-datum admission, an inadmissible clause computation uses its offending expression node; a condition that is not one exact output-bearing L0 relation uses that `ensures_clause`'s `expr`.
A concrete instance with no selected normal exit uses the `ensures_clause` and residual exactly `no selected normal exit`.
An unsupported selected return expression, unavailable entry image, or complete relation failure uses `SourceNode` at that existing `return_stmt` and its complete checked extent.
The deterministic relation payload is `(concrete function instance, postcondition occurrence, route identity or unrouted, instantiated normalized relation, disposition)`, with disposition exactly `unproved` or `refuted`; entry-image unavailability fixes `unproved`.
Instances use DIAG-1's stable concrete-instance order, selected returns use NodePath order, and the first relation failure wins.
No FN-9 failure fabricates an executable epilogue, runtime fallback, optimizer assumption, conditional outcome evidence, or caller-side rejection.
An excluded caller route is not itself a rejection: it establishes no S12 fact, and a later query needing that absent relation is diagnosed at that later node by its ordinary owning rule. An admitted named Result retains the conditional metadata of ENT-5.

Invariant and certificate diagnostics use their ordinary semantic schedule.
FN-1 first rejects every structurally unreachable statement; only a reachable loop header or `invariant_stmt` enters the schedule below.
After GRAM-4 and INV-1 have admitted the invariant names and their uniqueness, INV-1 checks for one ordinary or counted loop header its affine formation, the simultaneous base batch, every reachable arbitrary-backedge batch, and then any counted exact-exhaustion export, in that order.
For one local `invariant_stmt`, INV-1 first admits its name and then checks target formation.
For an optional block, ordinary parsing and lexical resolution precede semantic checking; PRF-1 then selects a rejection in this precedence: for each `proof_use` in source order, factor canonicality and then relation-source formation; whole-block redundancy; the 4096-entry capacity, duplicate normalized sources, and checked scaled-sum formation; every written `proof_use` independently against the one entering context in source order; then the one final DIRECT residual.
The first failed premise or target owns the rejection at the smallest source node fixed by INV-1 or PRF-1.
No written invariant conclusion enters the context before its complete owning judgment succeeds, and no later invariant may supply evidence to an earlier one.
Complete OP-2, OP-4, OP-9, OP-12, FN-8, FN-9, layout, address, and target-domain judgments select their own ordinary source errors; an [OP-12] atomic update whose callee row writes, moves out of, or frees a prefix of the target rejects at that complete `call`, while an update demoted to an ordinary `set` rejects at the consumed argument `atom` under OWN-1.
PAR-1 and PAR-2 permission failures select the sequential checked lowering or an explicit unsupported target lowering, never a source rejection.
An unavailable semantic judgment or inconsistent internal derivation is a compiler failure or explicit unsupported capability, not a guessed source rejection.

A rejection carries a repair where its owning rule states that it does, and any other rejection may carry one.
A repair names one or more alternative source changes, each saying what to add, remove, or replace and where, selected by the rejected construct, its position, and, for a goal, its disposition [ENT-4].
Carried out as it directs, each alternative lets the rejected judgment succeed at that construct in a state that is not contradictory [ENT-4]; an alternative that does so only when the program meets a condition the checker has neither established nor refuted states that condition.
No alternative writes text that a rule rejects where it is written, or at any use of it, independently of the rest of the program.
A refuted goal's repair therefore changes what reaches the construct, such as its operands, arguments, or returned value or the statements and requirements that fix them, or changes the operation or clause that poses the goal, because establishing a refuted goal where the facts that refute it remain makes that point contradictory.
The words of a repair are not specified.
Every published static diagnostic is deterministic for one compiler executable under the conditions above.
Cross-implementation byte identity is required only where this specification explicitly fixes both selection and encoding.

[DIAG-2] Successful semantic checking produces one private checked-program value bound to the exact canonical compilation unit.
It is the only input that may grant lowering authority.

The checked program explicitly represents every source operation and every compiler-derived operation required for execution, including drops, monomorphized instances, propagation edges, every reference's recorded path and target set [REF-1], every reference-validity fact and its invalidating event [REF-2], every atomic in-place update and its committed call [OP-12], every call-site pairwise disjointness derivation [EFF-5], and one abstract target-domain representability obligation at every runtime-sized allocation and element-address operation governed by [STOR-6].
It retains every [FN-8] GoalTemplate, its requirement occurrence `(concrete callee instance, requires_clause NodePath)`, every concrete call substitution and discharged-goal derivation, every proved body-entry requirement, and each inhabited or contradiction-proved body disposition.
It retains every proof-required integer-domain, allocation-size, subscript-bounds, layout, address, and target-domain obligation occurrence together with the exact derivation authorizing its accepted source node.
It separately retains each successful PAR-1 and PAR-2 permission derivation that authorizes an optional nonsequential lowering; absence retains no permission and changes no source verdict.
It also retains every proved loop-invariant base and arbitrary-backedge judgment, each permitted exhaustion export, and every PRF-1 premise-admission, factor, scaled-sum, and final-DIRECT-residual judgment.
Target lowering must discharge each target-domain obligation from the selected target plus already-checked layout, allocation, and bounds facts before emitting the governed allocation or address operation; it may not replace a missing proof with a runtime guard.
No accepted proof-required operation carries an implicit runtime check or elimination disposition: a subscript, exact integer operation, allocation, or function range requirement is `discharged` at its owning source node, and the checked program retains its exact [ENT-4] or [ENT-6] derivation there.
A concrete terminal-root identity uses the owning function instance plus the operation NodePath/family/conjunct, the call NodePath/callee/requirement NodePath, or the complete-postcondition block/relation ordinal; display symbols are never identity.
A `requires_clause` is represented only by its GoalTemplate, call-site derivations, and proved body-entry fact; an `ensures_clause` only by its verified RelationTemplate, selected-exit judgments, and derivations.
Neither contract clause has executable checked-program form.
Facts-off compilation preserves every source-acceptance and call-goal judgment and erases the same proof-only syntax before lowering.
Neither a discharged call goal nor a proved body-entry fact authorizes a second lowering path or an alternate acceptance; what a proved fact may tell the backend is stated below.
STOR-6 target-domain obligations instead follow the target-stage discharge judgment above identically in facts-on and facts-off compilation; an optional optimizer fact supplies no target-layout discharge.
Correctness comes first and performance is pursued on top of it, so every fact the checker has proved may be supplied to the backend, as target attributes, instruction flags, metadata, or assumptions: the places one call's substituted row proves disjoint [EFF-5], the places a permitted adjacency or loop proves independent [PAR-1, PAR-2], a reference's validity and extent [REF-2, REF-4], a discharged subscript bound [OP-4], and a discharged integer-domain obligation [OP-2], the last being what licenses a no-wrap flag on an exact operation.
A supplied fact adds no runtime branch, no lock, no dependency, and no scheduling edge to the emitted program, and its absence changes no source verdict and no emitted behavior; only a fact the checker has actually discharged may be supplied, never one a writer states.
That supply authorizes no second lowering path and no alternate acceptance.

The one current ProofContext is failure-atomic.
No fact, postcondition summary, invariant target, partial-operation discharge, checked function, or lowering input leaves semantic scratch until every premise of its originating judgment has succeeded.
Every `requires` fact enters a callee only after the caller has discharged that concrete call's complete instantiated requirements; runtime argument values establish nothing by themselves.
Every `ensures` summary of one call-graph strongly connected component is withheld until every selected return of every concrete member has been proved from its own entry facts and body flow, after which the component publishes its complete summaries atomically.
No member of that component may use a summary withheld by this rule, so recursive postconditions cannot bootstrap one another without an independently established source fact.
A header invariant enters the current loop-body ProofContext as an induction hypothesis only after its complete simultaneous base batch succeeds; it grants no checked-program or continuation authority unless every applicable arbitrary-backedge batch also succeeds. A local invariant enters its dominance region only after its one AUTO or complete PRF-1 judgment succeeds. A PRF-1 conclusion enters only after every source, factor, scaled sum, and final DIRECT residual succeeds.
Any failure discards the complete prospective checked program and every unpublished derivation root.

The current ProofContext and its diagnostic derivations are produced by the same source-semantic walk.
Every accepted fact has one specification-enumerated constructor and the direct parents used by that constructor; no runtime-value origin, compiler-generated record, optimizer result, or written conclusion supplies an alternate route into the context.
Every parent precedes its child, every retained node is reachable from a required accepted root, and finalization performs one reachability traversal and one identity remap.
An implementation may choose its private Rust layout, but it may not create another acceptance-bearing fact route, omit a source fact and reconstruct it after publication, re-run the function under a mask, or consult a second checker.
A callee summary is referenced by checked-program-private `(concrete callee instance, postcondition occurrence)` identity; a caller never imports a callee's local node identity.

Each [ENT-3.S7] result bound, order relation, and offset relation is one `OperationFact` node carrying the selected row, the value it was established on, the relation, and the source event. Its parents are the closed operand bounds the row read, or, for a `*` row over the interval-product rule's intervals, that multiplication's discharged IntegerDomain derivation; a literal or named-const operand and an operand read at its type's interval contribute no parent. Like every other source fact, an `OperationFact` is retained exactly when a required root reaches it.
The literal scaled division image of [ENT-3.S7] cites the `OperationFact` bounding the quotient by its dividend — the order relation `q <= a`, or the quotient's upper result bound when a is a literal or named-const value — together with the exact q and a value images rather than creating an independent source fact. `UnsignedDivisionProduct` roots that source's product consequence and carries the exact multiplication source with that `OperationFact` and that multiplication's discharged IntegerDomain root as parents.
Each `RequirementAffineImage` roots one S4 affine ordering image and retains its exact goal, truth sign, established-goal parent, requirement ordinal, and root-or-decomposition-member ordinal. It is affine-premise evidence, not an L0 relation or an independent assumption.
These are ordinary nodes in the same DAG, not trusted optimizer facts.

For every concrete FN-9 declaration, `PostconditionExit` roots each discharged selected-return relation on the exact local [ENT-4] derivation after result substitution and before return transfer or cleanup.
It also retains the entry-image-stability disposition and ordered invalidating event when unavailable; successful absence of an invalidating event is diagnostic metadata, not a fabricated positive parent.
`PostconditionAggregate` has the nonempty selected-exit roots as parents in return-NodePath order.
A non-discharged declaration retains its ordered dispositions and residual but no success aggregate root.
Component summaries become referenceable together only after the SCC schedule validates that every summary reference points strictly from a caller component to an earlier callee component.

Every S12 fact actually established in accepted semantic flow is a required caller-local root even when no later query consumes it.
`PostconditionCall` carries q, the checked aggregate summary reference, exact per-formal pre-transfer substitution, A0's complete actual-obligation and FN-8 goal roots, and the ordered transfer/consume/borrow/effect/kill event prefix.
`PostconditionDirectResult` adds the fresh ordinary-let binding substitution.
`PostconditionConditional` roots the call relation inside its returned value's conditional Ok context. `ResultTransport` records each retained forward substitution between an evaluated integer and that context's private payload parameter, or between the parameter and a selected receiving binding, with its source occurrence and relation parent. `ResultErr` identifies the constructor whose success context is contradictory. Conditional roots are never unconditional caller premises.
`PostconditionDirectReceiver` adds the direct-set target kill and result-only post-write substitution.
False `M(c,q)`, a rejected call, killed support or an excluded receiver creates no source fact root. Result copies and joins retain the corresponding conditional parents without re-instantiating a callee contract.

For bounded value-initializer delivery, `PostconditionGive` records one eligible reaching edge, the already evaluated source value and relation root, then the forward `d ↦ x` substitution, then that edge's ordinary scope and event kills applied to every other support in that order.
`PostconditionDeliveryJoin` orders all non-contradictory reaching delivery images by edge NodePath and applies exactly the ordinary [ENT-5] L0 delivery join.
Its parents therefore need not state byte-identical relations; an `x < 8` image and an `x < 128` image may parent the joined `x < 128` root.
Contradictory inputs use the existing contradiction root and are neutral when a non-contradictory input reaches.
Missing edge evidence or no common joined relation creates no delivery root.
Kill events never become invented positive evidence.

Candidate S12 and delivery nodes live only in failure-atomic semantic scratch until the current source judgment and its ordinary ownership, effect, and kill events all succeed.
Any failure discards the whole candidate root set with the unpublished checked program; success commits that set once, without a provenance batch, masked rerun, strict gate, or reconstructed root.
This preserves A0 and S12 atomicity and makes the derivation DAG an explanation of the accepted semantic flow rather than a second acceptance path.

For every `for_stmt`, the checked program additionally represents its optional source label and mandatory binder, the two source endpoint atoms in evaluation order, the two immutable compiler-owned captures with their identities, binder initialization, the pure header comparison, both header edges, the exact normal-body cleanup and update order, every no-update exit edge, the distinct header and continuation carried-binding sets, every hidden scope kill, and the complete [ENT-4] derivation of each S11 fact.
These are checked semantic operations and facts, not a source desugaring or an optimizer reconstruction.

Function-kind instantiation retains the source identity of each written parameter, group application and member, its complete concrete substitution, the selected ordinary function instance, and its FN-4 signature, row and contract checks. Every bound call has one direct target and its instantiated formal interface; lowering emits no group object or adapter.
A symbolic template may use a `FunctionFormalContract` hypothesis while checking its written spelling. That source-local hypothesis records the formal contract and relation ordinal, is never a proof of an actual body, and is never published in a concrete checked program or its derivation DAG. Concrete calls use only their selected actual's independently verified FN-9 summaries. No algebraic-law metadata or law-derived optimization authority exists.

The checked-program representation is private compiler state.
Its Rust layout, allocation strategy, dense identities, instruction grouping, and internal ordering where this specification defines no semantic order are implementation-defined.
Any diagnostic or debug projection is explanatory only: it establishes no source fact and grants no lowering authority.

The language defines no writer-reachable runtime proof-failure report, because every writer-visible proof obligation is discharged before lowering or rejects the source.
Static source diagnostics follow [DIAG-1]. Diagnostic derivations follow [DIAG-2] and explain the source semantic decision; they are never reloaded or checked as a second acceptance step.
An implementation may report unavailable resources, trusted-computing-base failures, or compiler failures on implementation-defined channels, but none is a source-language outcome and none may be mistaken for a successful source judgment.

## 13. Execution overlap

[CAP-1] The kernel defines no writer-visible capability category and no additional concurrency permission. `own`, `&`, path overlap [OWN-7], and the ordinary effect row [EFF-1] are the complete authority and interference vocabulary available to [PAR-1] and [PAR-2].
The kernel defines no thread construct. Its data-race guarantee is subject to [SCOPE-3]; it does not exclude general race conditions.

[PAR-1] An implementation may execute two adjacent statements of one block with overlapping execution exactly when the first's write paths are disjoint from the second's read and write paths and the second's write paths are disjoint from the first's, using the same path-overlap and index/range-disjointness judgment as [EFF-5] and [OWN-7].
Read/read overlap is admitted.
A by-value consumption counts as a write of the argument's place.
Evaluating a statement's own argument expressions is part of that statement, so each statement's write paths must also be disjoint from the places the other statement's argument expressions read; a `let`'s defined binding is a write path.
An `if` or `match` statement's footprint includes its condition or scrutinee evaluation and the union of the footprints of every arm that may execute; independence of the scrutinee call alone does not establish independence of that statement. An implementation that cannot resolve an arm's footprint has no permission for that statement.
Allocation and release contribute no path [STOR-8].
The paths of both statements are interpreted in the state before the first statement; the first statement's `ensures` maps the second's indices into that state, so an index that is live only after an append is not distinct from the append slot [WIN-2].
Permission composes: any run of adjacent statements that pairwise may overlap may all overlap, and "pairwise" means every ordered pair in the run.
A footprint element whose caller place the implementation does not resolve overlaps every place and denies permission.

Under a permitted overlap, bindings and every Whitefoot state place equal the source-order result.
That identity is conditional on contract compliance, exactly as [SCOPE-3]'s freedom from undefined behavior is conditional on its trusted computing base.
It holds in every source execution, not in a typical execution or in some execution: accepted source contains no writer-reachable proof-failure branch, and every partial operation in the window has already been discharged by its owning static Goal.
No overlapped pair reaches one state place except as the permission conditions above admit.
Target-resource exhaustion and trusted-computing-base termination remain outside the source execution model under [SCOPE-3] and grant no overlap permission.
No permission or execution path reads a proof-failure latch or pays any other cost for a writer-reachable runtime proof fallback.
The number of workers, the identity of the host thread that executes a statement, the schedule, and whether an overlap was performed at all are not observable, and no rule of this specification is stated in terms of them.
An implementation that overlaps nothing therefore conforms: this permission is never an obligation, and no program depends on it being taken.
Exhaustion of the execution resources an implementation spends on overlapping is a resource condition under [SCOPE-3] and is not an observable of this rule.
Every construct of this specification defines one total sequential order over its operand evaluations, and this rule is a consumer of that order rather than a relaxation of it.
This rule uses [CAP-1]'s ordinary ownership boundary directly; it introduces no additional sharing classification.
The counted permission [PAR-2] forms every statement's read and write paths exactly as this rule does.

[PAR-2] An implementation may execute two iterations of one `for_stmt` body with overlapping execution, and may recombine that loop's accumulator across them, only when the permission this rule defines holds for that counted loop.
Permission holds for a `for_stmt` L exactly when all of the following hold, writing B for L's body and forming every written, read, and operand-read footprint of a statement of B exactly as [PAR-1] forms one.
Among whole-place writes of B, at most one place is rooted in a binding declared outside L; that binding is L's accumulator, and every occurrence of it in B is one operand of one `set` statement whose target is that whole binding and whose right-hand side is one operation applied to that operand and to a second operand reaching the accumulator nowhere.
That operation is one operation fixed for the accumulator across the whole of B, and is exactly one of `+wrap`, `*wrap`, `iand`, `ior`, `ixor`, `imin`, `imax`, `band`, `bor`, and `bxor` [OP-1].
Every place a footprint of B writes is iteration-own storage, the accumulator's whole place, one proved single-binder affine element write, or one proved range reference.
A proved single-binder affine element write is exactly a `set_stmt` whose target is one direct `Array` or `Slots` subscript rooted in an own binding declared outside L or reached through `deref` of a reference parameter whose row declares the write [EFF-5], whose exact [OP-4] bounds obligation at that subscript is discharged in the current ProofContext and retains the offset's canonical exact value `a*i + b`: i is L's compiler-owned binder, a and b are mathematical integer constants, a is nonzero, and no other symbolic term occurs.
The retained [OP-4] result and affine value are consumed from the same source semantic check. The value may have been carried through copies and checked affine operations; PAR-2 neither repeats the bounds proof, reconstructs the value from parser shape, nor trusts a runtime check, optimizer fact, or backend result.
For permission only, this fixed form refines the ordinary whole-collection write footprint to the single-element range `[a*i + b, a*i + b + 1)`.
The counted recurrence of [FN-1] gives distinct binder values to distinct iterations, and multiplication by the same nonzero integer a preserves distinctness, so their refined ranges do not overlap; statement order within one iteration is unchanged.
This refinement proves only the source element-range and cross-iteration disjointness. The selected-target [STOR-6] check must still prove the concrete element stride, layout, and address domain before emission; that later target check consumes the already-permitted source access and never grants PAR-2 permission retroactively.
Every write by B to one mapped root must be another proved single-binder affine element write carrying exactly the same a and b; different resolved roots may carry different maps. Every operand read through that same root binding must be a direct `Array` or `Slots` subscript whose own discharged [OP-4] result retains exactly the same a and b. For permission only, that read footprint is refined to the same single-element range, so it overlaps writes of its own iteration in source order and no access of another iteration. A whole-root read, a subscript carrying a different or unavailable map, any other access overlapping the resolved root, or an unresolved place denies.
The element family admits one affine map per root, including same-index read-modify-write and writes reached through `deref` of a reference parameter whose row declares the write. A constant element image, two different element maps of one root, and every other element injectivity argument deny permission rather than starting proof search. A `Ring` in an element-map position denies permission, because a `Ring` subscript selects the slot `(r.head + i) mod r.cap` [WIN-1], a wrapping map onto storage rather than a linear offset.

A proved range reference is a range reference `&r[s*i+b..s*i+b+s]` [REF-4] passed as an ordinary argument, whose discharged endpoint domain retains the exact mathematical images `[s*i+b, s*i+b+s)`, where i is L's binder, and s and b are fixed throughout L with proved `0 <= s` and `0 <= b`. The indexable place or range reference it is formed from is declared outside B and retains its resolved origin; a range reference formed inside B instead inherits an existing proved range reference only when its complete origin path is a descendant of that range reference. Each further formation's own [REF-4] endpoint obligation establishes containment. No child call, read, or write gains a wider extent than its actual origin path.
The automatic image family is finite and fixed. At L's preheader after continuing kills, the immutable numeric value atoms still available to surviving scalar bindings and measures are fixed. Canonical checked affine sums and scalar multiples preserve exact value images. A recorded admitted exact multiplication may be expanded through its two operand value images, including the checked transparent images behind copied-value handles. A product of two fixed operands is fixed. Otherwise exactly one operand may depend on i, and multiplying its coefficient and constant part by the fixed operand must leave both parts affine: each such multiplication has a mathematical constant on at least one side. This rule recursively traverses the finite checked value graph, rejects a cyclic or unknown image, and introduces no arbitrary-degree polynomial or injectivity search. Normalized constants and coefficients use [ENT-6]'s checked mathematical integer domain. Each active counted binder is considered once, endpoint coefficients must agree, and the ending constant part must equal the starting constant part plus s. Both sign goals are submitted to the existing ProofContext and their successful derivations are retained with the formation's bounds result. Permission consumes those checked images and proofs; it neither reinterprets source spelling nor reruns arithmetic proof.
For distinct counted indices i < j, integer discreteness gives i+1 <= j; nonnegative s gives `s*i+b+s <= s*j+b`. Their half-open ranges therefore do not overlap under [OWN-7], including s=0's empty ranges. This argument is independent of runtime stride, iteration count, and worker count. Checked endpoint domains and selected-target layout qualification remain required separately.
Among proved range references through which B writes, all whose resolved origins overlap must name the same origin place and carry identical s and b images. Every element access overlapping such a written origin must descend from a proved range reference with that same origin and those identical images; the [EFF-2] projection of a helper's declared row counts as an access on its actual range. A whole-origin access, a differently mapped access, an unresolved origin, or mixing an element map with an overlapping written range origin denies. Read-only accesses whose origins overlap no written origin need no iteration partition and may overlap one another across iterations. Same-iteration sibling separation alone cannot establish cross-iteration independence between two different range references when either writes. Forming a range reference reads its endpoints, reads no element content, and authorizes no change to the origin's storage.
A footprint element whose caller place the implementation does not resolve overlaps every place, so an unresolved element denies permission rather than granting it.
Effects and path overlap decide interference between iterations exactly as they do between [PAR-1] statements. An implementation retains each iteration's live storage for that complete extent.
Every normal continuation of every statement of B reaches L's compiler-owned binder update, so no statement of B is a `return_stmt`, a `give_stmt`, a `break_stmt` resolved to L or a loop enclosing L, or a `let_stmt` selecting `propagate_let_rhs` [FN-1, GIVE-1, ERR-3].

Under a permitted overlap every state-place observable is the one produced by executing L's iterations in index order.
Write a0 for the accumulator's value on the true header edge entering the first executed iteration, and t0 through tm for the values the second operand of its writes evaluates to, in the order those writes execute across L's iterations taken in index order.
Source order computes the accumulator's value at L's continuation as the left-nested application of that operation to a0 then t0 through tm where its writes place the accumulator in the first operand position, and as the right-nested application to t0 through tm then a0 where they place it in the second.
An implementation may instead apply that operation over any binary tree whose leaves are a0 and t0 through tm, each occurring exactly once and in any order, together with any number of leaves holding that operation's identity element.
Every admitted operation is a total function on the complete value set of its type, carries no domain obligation, and is associative and commutative on that set with a two-sided identity element — `+wrap` and `*wrap` are the ring operations of the integers modulo two to the width, with identities zero and one; `iand`, `ior`, and `ixor` are the meet, join, and group operations of the bit vector, with identities the all-ones vector, zero, and zero; `imin` and `imax` are the meet and join of that type's total order, with identities the type's greatest and least values; and `band`, `bor`, and `bxor` are the two-element cases of the same three, with identities `true`, `false`, and `false` — so every such tree denotes one value of that type and the accumulator's value at L's continuation is that one value in every execution.
No further operation is admitted: `+`, `+defined`, and `+checked` each attach a domain obligation or a `Result` route to every application, `+sat` is not associative, and no float operation of [OP-1] is associative, so recombining a `fadd.strict` or `fmul.strict` fold could change published bytes.
This rule uses associativity, commutativity, and the identity together: commutativity is what admits any leaf order and the fold of the second operand position, and the identity is what lets an implementation seed a subrange of iterations before knowing whether that subrange writes, so a range of iterations that writes the accumulator not at all contributes either nothing or identity leaves that change nothing.
That identity is conditional on contract compliance exactly as [PAR-1]'s is; every partial operation in the admitted loop has already been discharged before lowering.
Both endpoint atoms are still evaluated exactly once each in [FN-1]'s order before any iteration begins, and the binder still takes each value of the half-open range exactly once; this rule relaxes only the order in which iterations execute and the shape of the accumulator's combination, never the set of iterations, the values the binder takes, or either endpoint evaluation.
The number of workers, the identity of the host thread that executes an iteration, the schedule, how the index range is divided, and whether any overlap or recombination was performed at all are not observable, and no rule of this specification is stated in terms of them.
An implementation that overlaps nothing therefore conforms: this permission is never an obligation, and no program depends on it being taken.
When an execution of one iteration does not reach its continuation, the overlapped execution produces exactly the observables the index-order execution produces before that point and produces none after it.
Exhaustion of the execution resources an implementation spends on overlapping is a resource condition under [SCOPE-3] and is not an observable of this rule.
Permission over the iterations of a `for_stmt` written inside B is exactly this rule applied to that loop; no rule of this specification joins two index ranges into one iteration space.
This rule uses [CAP-1]'s ordinary ownership boundary directly; it introduces no additional sharing classification for the accumulator or any other place.

## 14. Prelude and host modules (normative, counted)

[PRE-1] The prelude contributes ordinary nominal, constructor, numeric-bound and function declarations to every module. Their source visibility, collisions, typing, ownership and calls are the ordinary rules; an entry's prelude origin supplies only its deterministic diagnostic ordinal [TYPE-6, DIAG-1].

The prelude's opaque structs [TYPE-2] are the three storage shapes and the cell `Box` [TYPE-9], built by the construction rows [OP-13]. An opaque struct is not const-eligible [CONST-2]; its capability modifier and the ordinary ownership closure are exactly [OWN-1, PROV-6]. Their declarations are:

```
opaque struct Array<T, const n: u64> {
  readonly len: u64;
}

opaque nocopy struct Slots<T, const n: u64> {
  readonly len: u64;
  readonly cap: u64;
}

opaque nocopy struct Ring<T, const n: u64> {
  readonly len: u64;
  readonly cap: u64;
  readonly head: u64;
}

opaque nocopy struct Box<T> {
  inner: T;
}
```

The complete enum declarations are:

```
enum Bool {
  True();
  False();
}

enum Option<T> {
  None();
  Some(value: T);
}

enum Result<T, E> {
  Ok(value: T);
  Err(error: E);
}

enum Overflow {
  Overflow();
}

enum DivError {
  DivideByZero();
  DivOverflow();
}

enum NarrowError {
  NarrowError();
}
```

The two built-in numeric bounds `Int` and `Float` admit exactly OP-1's integer and floating-point domains and imply `copy` under PROV-6. They are not source declarations, interface groups, implicit behaviors or logical-law bundles; a source actual cannot bind or extend either bound.

The complete function declarations are the following records, each written as the head of a GRAM-2 `fn_decl` — `"fn" IDENT generics? "(" param_list? ")"` and the rest of `fn_sig` from `->` on — so a record carries `fn_decl`'s `generics?` where a function-kind parameter's `fn_sig` [FN-3] carries none. A record's final semicolon is table punctuation, not a new top-level source production. Each signature uses ordinary parameter paths under EFF-1 and the same requirement and postcondition templates as any FN-8/FN-9 contract. The type parameters `W` and `X` of the window operations are the compiler-owned window type parameter OP-10 fixes, and the `W` of `free_empty` is the wider shape parameter OP-14 fixes. No proposition is available merely from a function's name, implementation, result constructor, or prelude origin.

```
fn box_new<T>(value: T) -> result: Box<T> pure;
fn array_filled<T: copy, const n: u64>(value: T) -> result: Array<T, n> pure contract {
  ensures result.len == n;
};
fn slots_new<T, const n: u64>() -> result: Slots<T, n> pure contract {
  ensures result.len == 0_u64;
  ensures result.cap == n;
};
fn ring_new<T, const n: u64>() -> result: Ring<T, n> pure contract {
  ensures result.len == 0_u64;
  ensures result.cap == n;
  ensures result.head == 0_u64;
};
fn box_array_filled<T: copy>(count: u64, value: T) -> result: Box<Array<T>> pure contract {
  ensures result.inner.len == count;
};
fn box_slots_new<T>(capacity: u64) -> result: Box<Slots<T>> pure contract {
  ensures result.inner.len == 0_u64;
  ensures result.inner.cap == capacity;
};
fn box_ring_new<T>(capacity: u64) -> result: Box<Ring<T>> pure contract {
  ensures result.inner.len == 0_u64;
  ensures result.inner.cap == capacity;
  ensures result.inner.head == 0_u64;
};
fn slots_from_array<T, const n: u64>(values: Array<T, n>) -> result: Slots<T, n> pure contract {
  ensures result.len == n;
  ensures result.cap == n;
};
fn slots_into_array<T, const n: u64>(values: Slots<T, n>) -> result: Array<T, n> pure contract {
  requires values.len == n;
  ensures result.len == n;
};
fn place_back<W, T>(window: &W, value: T) -> result: unit writes(window.next), writes(window.len) contract {
  requires deref(window).len < deref(window).cap;
  ensures deref(window).len == deref(entry(window)).len + 1_u64;
};
fn take_back<W, T>(window: &W) -> value: T writes(window.last), writes(window.len) contract {
  requires deref(window).len > 0_u64;
  ensures deref(window).len + 1_u64 == deref(entry(window)).len;
};
fn insert_at<W, T>(window: &W, index: u64, value: T) -> result: unit writes(window.filled), writes(window.next), writes(window.len) contract {
  requires index <= deref(window).len;
  requires deref(window).len < deref(window).cap;
  ensures deref(window).len == deref(entry(window)).len + 1_u64;
};
fn remove_at<W, T>(window: &W, index: u64) -> value: T writes(window.filled), writes(window.len) contract {
  requires index < deref(window).len;
  ensures deref(window).len + 1_u64 == deref(entry(window)).len;
};
fn append<W, X>(destination: &W, source: &X) -> result: unit writes(destination.free), writes(destination.len), writes(source.filled), writes(source.len) contract {
  requires deref(source).len <= deref(destination).cap - deref(destination).len;
  ensures deref(destination).len >= deref(entry(destination)).len;
  ensures deref(destination).len >= deref(entry(source)).len;
  ensures deref(source).len == 0_u64;
};
fn split_off<W, X>(source: &W, index: u64, destination: &X) -> result: unit writes(source.filled), writes(source.len), writes(destination.free), writes(destination.len) contract {
  requires index <= deref(source).len;
  requires deref(source).len - index <= deref(destination).cap - deref(destination).len;
  ensures deref(source).len == index;
  ensures deref(destination).len >= deref(entry(destination)).len;
};
fn grow<T>(cell: &Box<Slots<T>>, capacity: u64) -> result: unit writes(cell) contract {
  requires capacity >= deref(cell).inner.cap;
  ensures deref(cell).inner.cap == capacity;
  ensures deref(cell).inner.len == deref(entry(cell)).inner.len;
};
fn place_front<W, T>(window: &W, value: T) -> result: unit writes(window) contract {
  requires deref(window).len < deref(window).cap;
  ensures deref(window).len == deref(entry(window)).len + 1_u64;
  ensures deref(window).cap == deref(entry(window)).cap;
  ensures deref(window).head >= 0_u64;
  ensures deref(window).head <= deref(window).cap;
};
fn take_front<W, T>(window: &W) -> value: T writes(window) contract {
  requires deref(window).len > 0_u64;
  ensures deref(window).len + 1_u64 == deref(entry(window)).len;
  ensures deref(window).cap == deref(entry(window)).cap;
  ensures deref(window).head >= 0_u64;
  ensures deref(window).head <= deref(window).cap;
};
fn swap<T>(first: &T, second: &T) -> result: unit writes(first), writes(second);
fn free_empty<W>(window: W) -> result: unit pure contract {
  requires window.len == 0_u64;
};
```

Each record is an ordinary callable boundary usable by a direct call or a function-kind binding under FN-2 through FN-5. Its definition is supplied by the build and must satisfy the declared boundary [SCOPE-3]; calls neither inspect nor classify that definition. There is one ordinary callable ABI for definitions written in Whitefoot and definitions supplied by linking. A reference passed to either lasts through that call's return and is not retained beyond it [REF-3]. A missing definition or incompatible physical representation is a build/link failure, not a source-language rejection.
PRE-1 requirement templates are discharged by FN-8 and declared postconditions are instantiated only by CALL-6 and FN-9's ordinary selected-result rules. The supplied definition is responsible for those propositions under SCOPE-3; its declaration has no Whitefoot body for FN-9 to verify. No compiler-owned operation fact or alternative acceptance judgment exists.
The declaration preorder is each opaque struct above in written order with its refused constructor and its fields in declaration order, then each enum above in written order with its variants and their fields in declaration order, then `Int`, `Float`, then each construction function above in written order, then each window operation above in written order, then `swap` and `free_empty`, each with its type, const and value parameters in declared order. Owner-local fields and parameters do not enter compilation-root name lookup. This preorder fixes each PRE-1 diagnostic ordinal [DIAG-1].

[PRE-2] The host modules are the five standard library modules [MOD-10] `std::io`, `std::text`, `std::fs`, `std::net` and `std::process`, registered by these rows of the standard library's graph:

```
pkg::io: [];
pkg::text: [];
pkg::fs: [pkg::io, pkg::text];
pkg::net: [pkg::io];
pkg::process: [pkg::io, pkg::text, pkg::fs];
```

A host module has no implementation record, and its interface record is exactly the text below. Each function it declares is an ordinary callable boundary whose definition the build supplies and must satisfy the declared boundary [SCOPE-3], exactly as a PRE-1 function record's is; calls neither inspect nor classify that definition, and its requirement templates and postconditions are discharged and instantiated as PRE-1's are.
A host handle is an opaque struct [TYPE-2] a host module declares with no fields: it has a host-supplied representation, its release is empty [STOR-3], and only a host function returns one.
`TcpConnection`, `AcceptedConnection` and `Inputs` have ordinary public constructors, fields, partial-move and destructuring rules. Their linearity follows their fields. No relation between two fields is implied by constructing a struct.

`std::io`, the record `io/module.wfm`:

```
public opaque nocopy struct HandleFactory {
}

public opaque nocopy struct OutputStream {
}

public opaque nocopy struct InputStream {
}

public enum IoError {
  NotFound(public code: u32, public origin: u8);
  PermissionDenied(public code: u32, public origin: u8);
  AlreadyExists(public code: u32, public origin: u8);
  NotDirectory(public code: u32, public origin: u8);
  IsDirectory(public code: u32, public origin: u8);
  DirectoryNotEmpty(public code: u32, public origin: u8);
  ReadOnly(public code: u32, public origin: u8);
  ResourceBusy(public code: u32, public origin: u8);
  InvalidInput(public code: u32, public origin: u8);
  InvalidPath(public code: u32, public origin: u8);
  Unsupported(public code: u32, public origin: u8);
  TimedOut(public code: u32, public origin: u8);
  BrokenPipe(public code: u32, public origin: u8);
  WriteZero(public code: u32, public origin: u8);
  UnexpectedEnd(public code: u32, public origin: u8);
  ConnectionRefused(public code: u32, public origin: u8);
  ConnectionReset(public code: u32, public origin: u8);
  ConnectionAborted(public code: u32, public origin: u8);
  NotConnected(public code: u32, public origin: u8);
  AddressInUse(public code: u32, public origin: u8);
  AddressUnavailable(public code: u32, public origin: u8);
  ResourceExhausted(public code: u32, public origin: u8);
  FileTooLarge(public code: u32, public origin: u8);
  NoSpace(public code: u32, public origin: u8);
  QuotaExceeded(public code: u32, public origin: u8);
  CrossDevice(public code: u32, public origin: u8);
  DeviceFailure(public code: u32, public origin: u8);
  Other(public code: u32, public origin: u8);
}

public enum ReadStop {
  ReadEnd();
  ReadFailed(public error: IoError);
}

public fn write_once(factory: &HandleFactory, output: &OutputStream, source: &[u8], start: u64, end: u64) -> result: Result<u64, IoError> reads(source), writes(factory), writes(output) contract {
  requires start <= end;
  requires end <= deref(source).len;
  ensures when Ok(value: next): start <= next;
  ensures when Ok(value: next): next <= end;
} doc "Writes bytes of source from start toward end to output with one host write; Ok carries the index after the last byte written.";

public fn read_next(factory: &HandleFactory, input: &InputStream, destination: &[u8], start: u64, end: u64) -> result: Result<u64, ReadStop> writes(factory), writes(input), writes(destination) contract {
  requires start <= end;
  requires end <= deref(destination).len;
  ensures when Ok(value: next): start <= next;
  ensures when Ok(value: next): next <= end;
} doc "Reads bytes of input into destination from start toward end with one host read; Ok carries the index after the last byte read, and ReadEnd reports the end of the input.";
```

`std::text`, the record `text/module.wfm`:

```
public opaque nocopy struct Args {
}

public opaque nocopy struct HostString {
}

public enum ArgError {
  InvalidIndex();
}

public enum CopyError {
  CopyTooSmall(public required: u64);
}

public enum Utf8Error {
  Utf8Invalid();
}

public enum Utf8CopyError {
  Utf8CopyTooSmall(public required: u64);
  Utf8CopyInvalid();
}

public fn args_count(args: &Args) -> result: u64 reads(args) doc "Returns the number of invocation arguments.";

public fn arg_get(args: &Args, position: u64) -> result: Result<HostString, ArgError> reads(args) doc "Returns the invocation argument at position, or InvalidIndex when position is not below the argument count.";

public fn host_bytes_len(value: &HostString) -> result: u64 reads(value) doc "Returns the length of value in bytes.";

public fn host_copy_bytes(value: &HostString, destination: &[u8], start: u64, end: u64) -> result: Result<u64, CopyError> reads(value), writes(destination) contract {
  requires start <= end;
  requires end <= deref(destination).len;
  ensures when Ok(value: next): start <= next;
  ensures when Ok(value: next): next <= end;
} doc "Copies every byte of value into destination from start; Ok carries the index after the last byte copied, and CopyTooSmall carries the length the window needs when it is shorter.";

public fn host_utf8_len(value: &HostString) -> result: Result<u64, Utf8Error> reads(value) doc "Returns the length in bytes of the UTF-8 text value denotes, or Utf8Invalid when it denotes none.";

public fn host_copy_utf8(value: &HostString, destination: &[u8], start: u64, end: u64) -> result: Result<u64, Utf8CopyError> reads(value), writes(destination) contract {
  requires start <= end;
  requires end <= deref(destination).len;
  ensures when Ok(value: next): start <= next;
  ensures when Ok(value: next): next <= end;
} doc "Copies the UTF-8 text value denotes into destination from start; Ok carries the index after the last byte copied, Utf8CopyTooSmall carries the length the window needs when it is shorter, and Utf8CopyInvalid reports a value that denotes no UTF-8 text.";
```

`std::fs`, the record `fs/module.wfm`:

```
alias HandleFactory = pkg::io::HandleFactory;
alias IoError = pkg::io::IoError;
alias ReadStop = pkg::io::ReadStop;
alias HostString = pkg::text::HostString;

public opaque nocopy struct RelativePath {
}

public opaque nodrop struct DirectoryRead {
}

public opaque nodrop struct ReadFile {
}

public opaque nodrop struct DirectorySource {
}

public enum PathError {
  PathInvalid();
}

public enum ListStop {
  ListEnd();
  ListFailed(public error: IoError);
}

public fn relative_path(value: HostString) -> result: Result<RelativePath, PathError> pure doc "Returns value as a path relative to a directory, or PathInvalid when it cannot name one.";

public fn open_read(factory: &HandleFactory, root: &DirectoryRead, path: &RelativePath) -> result: Result<ReadFile, IoError> reads(root), reads(path), writes(factory) doc "Opens the file at path below root for reading.";

public fn read_at(factory: &HandleFactory, file: &ReadFile, destination: &[u8], file_offset: u64, start: u64, end: u64) -> result: Result<u64, ReadStop> writes(factory), writes(file), writes(destination) contract {
  requires start <= end;
  requires end <= deref(destination).len;
  ensures when Ok(value: next): start <= next;
  ensures when Ok(value: next): next <= end;
} doc "Reads bytes of file at file_offset into destination from start toward end with one host read; Ok carries the index after the last byte read, and ReadEnd reports the end of the file.";

public fn open_directory(factory: &HandleFactory, root: &DirectoryRead, name: &[u8], start: u64, end: u64) -> result: Result<DirectoryRead, IoError> reads(root), reads(name), writes(factory) contract {
  requires start <= end;
  requires end <= deref(name).len;
} doc "Opens the directory that the bytes of name from start to end name below root.";

public fn open_directory_source(factory: &HandleFactory, directory: &DirectoryRead) -> result: Result<DirectorySource, IoError> reads(directory), writes(factory) doc "Opens the listing of the entries of directory.";

public fn directory_next(source: &DirectorySource, destination: &[u8], start: u64, end: u64) -> (result: Result<unit, ListStop>, next: u64, entries: u64) writes(source), writes(destination) contract {
  requires start <= end;
  requires end <= deref(destination).len;
  ensures start <= next;
  ensures next <= end;
} doc "Writes the names of the next entries of source into destination from start toward end; next is the index after the bytes written, entries counts the names, and ListEnd reports that no entry remains.";

public fn open_file(factory: &HandleFactory, root: &DirectoryRead, name: &[u8], start: u64, end: u64) -> result: Result<ReadFile, IoError> reads(root), reads(name), writes(factory) contract {
  requires start <= end;
  requires end <= deref(name).len;
} doc "Opens the file that the bytes of name from start to end name below root for reading.";

public fn close_read(factory: &HandleFactory, file: ReadFile) -> result: Result<unit, IoError> writes(factory) doc "Closes file.";

public fn close_directory(factory: &HandleFactory, directory: DirectoryRead) -> result: Result<unit, IoError> writes(factory) doc "Closes directory.";

public fn close_directory_source(factory: &HandleFactory, source: DirectorySource) -> result: Result<unit, IoError> writes(factory) doc "Closes the directory listing source.";
```

`std::net`, the record `net/module.wfm`:

```
alias HandleFactory = pkg::io::HandleFactory;
alias IoError = pkg::io::IoError;
alias ReadStop = pkg::io::ReadStop;

public opaque nocopy struct SocketAddress {
}

public opaque nodrop struct TcpListener {
}

public opaque nodrop struct TcpReceive {
}

public opaque nodrop struct TcpSend {
}

public struct TcpConnection {
  public receive: TcpReceive;
  public send: TcpSend;
}

public struct AcceptedConnection {
  public connection: TcpConnection;
  public peer: SocketAddress;
}

public fn socket_address_v4(a: u8, b: u8, c: u8, d: u8, port: u16) -> result: SocketAddress pure doc "Returns the IPv4 socket address a.b.c.d with port.";

public fn socket_address_v6(a: u16, b: u16, c: u16, d: u16, e: u16, f: u16, g: u16, h: u16, port: u16) -> result: SocketAddress pure doc "Returns the IPv6 socket address whose eight groups are a to h, with port.";

public fn tcp_listen(factory: &HandleFactory, address: &SocketAddress) -> result: Result<TcpListener, IoError> reads(address), writes(factory) doc "Opens a TCP listener bound to address.";

public fn tcp_accept(factory: &HandleFactory, listener: &TcpListener) -> result: Result<AcceptedConnection, IoError> writes(factory), writes(listener) doc "Accepts the next connection on listener and returns it with the address of its peer.";

public fn tcp_connect(factory: &HandleFactory, address: &SocketAddress) -> result: Result<TcpConnection, IoError> reads(address), writes(factory) doc "Opens a TCP connection to address.";

public fn receive_next(receive: &TcpReceive, destination: &[u8], start: u64, end: u64) -> result: Result<u64, ReadStop> writes(receive), writes(destination) contract {
  requires start <= end;
  requires end <= deref(destination).len;
  ensures when Ok(value: next): start <= next;
  ensures when Ok(value: next): next <= end;
} doc "Receives bytes into destination from start toward end with one host receive; Ok carries the index after the last byte received, and ReadEnd reports that the peer finished sending.";

public fn send_once(send: &TcpSend, source: &[u8], start: u64, end: u64) -> result: Result<u64, IoError> reads(source), writes(send) contract {
  requires start <= end;
  requires end <= deref(source).len;
  ensures when Ok(value: next): start <= next;
  ensures when Ok(value: next): next <= end;
} doc "Sends bytes of source from start toward end with one host send; Ok carries the index after the last byte sent.";

public fn close_listener(factory: &HandleFactory, listener: TcpListener) -> result: Result<unit, IoError> writes(factory) doc "Closes listener.";

public fn close_receive(factory: &HandleFactory, receive: TcpReceive) -> result: Result<unit, IoError> writes(factory) doc "Closes the receiving half of a connection.";

public fn close_send(factory: &HandleFactory, send: TcpSend) -> result: Result<unit, IoError> writes(factory) doc "Closes the sending half of a connection.";
```

`std::process`, the record `process/module.wfm`:

```
alias HandleFactory = pkg::io::HandleFactory;
alias InputStream = pkg::io::InputStream;
alias OutputStream = pkg::io::OutputStream;
alias Args = pkg::text::Args;
alias DirectoryRead = pkg::fs::DirectoryRead;

public opaque nocopy struct ExitStatus {
}

public struct Inputs {
  public args: Args;
  public cwd: DirectoryRead;
  public stdout: OutputStream;
  public stderr: OutputStream;
  public handles: HandleFactory;
  public stdin: InputStream;
}

public fn exit_status(code: u8) -> result: ExitStatus pure doc "Returns the status that reports code when the entry returns it.";
```

## 15. Obligation discharge: deterministic facts, invariants, and local certificates (normative)

[ENT-1] The entailment fragment is a closed, deterministic, terminating derivation system fixed completely by this specification.
Its state is the L0 relation state, [ENT-2]'s finite signed opaque goals, [ENT-6]'s exact current-value images and specification-fixed automatic affine images, and the finite affine theorems admitted by [INV-1] and [PRF-1].
Complete-state obligation discharge [ENT-6], ordinary-call requirement discharge [FN-8], verified normal-return proof [FN-9], loop induction and program-point invariant checking [INV-1], and local certificate checking [PRF-1] are post-resolution source-acceptance judgments under [DIAG-1].
They are identical in facts-on and facts-off compilation and are not an optimizer-fact family.

The fact sources are exactly the executed control-flow edges, independently proved function requirements at callee entry, declaration and type properties fixed by this specification, constants, compiler-owned structural consequences enumerated by [ENT-3], verified earlier-SCC normal-result publications [FN-9], and machine-proved header or local invariant targets.
A runtime-origin value is an ordinary typed term in those judgments; its origin is neither a fact source nor a reason to discard an otherwise derived fact [SCOPE-2].
Only the fact sources enumerated above establish propositions; a written conclusion, unselected condition, diagnostic record, or optimizer result does not.

No source postcondition is trusted: FN-9 proves every selected exit, requires a nonempty selected-exit set, and withholds same-SCC summaries before atomic publication.
The fragment is the deterministic checker derivation of [OP-2], [OP-4], [OP-6], [OP-9], [FN-8], [FN-9], [INV-1], [PRF-1], [STOR-6], and [DIAG-2] for the judgments this version attaches.
A solver result never participates, and no implementation may strengthen, weaken, time-bound, randomize, or truncate an unsuccessful query within the derivable set.
Every semantic candidate family and iteration count is fixed below from the complete source text; an unproved result requires exhausting its complete family regardless of elapsed time, machine speed, thread schedule, hash iteration order, or memory pressure short of [SCOPE-3]'s external resource boundary.
A successful query may retain the first witness in the specification-fixed order and omit later witnesses, because no later candidate can revoke that success; this changes diagnostic parent choice only, never the derivable set or acceptance.
An implementation may organize or cache the same derivation differently, but exceeding a wall-clock or cumulative-work budget is never a source-language verdict.
Parser, finalizer, or canonical-source invocation ceilings may stop one compiler invocation only as [DIAG-1]'s non-language `Resource` failure; they return neither source acceptance nor source rejection and may never turn an unfinished ENT-1 candidate family into `unproved`.
Two conforming implementations derive the same fact state at every applicable program point; the same FN-9 selected exits, concrete-SCC order, and established result relations; the same certificate premise, combination, and target dispositions; and the same disposition for every operation obligation, call goal, postcondition relation, and invariant.

Every nongeneric source body receives this judgment whether or not the selected invocation reaches it.
Every generic source body additionally receives one source-schema judgment under the one source-canonical symbolic substitution formed during generic-body validation, even when it has no concrete instantiation.
That schema checks every OP-2/OP-4/OP-9, FN-8, and expressible FN-9 judgment its symbolic vocabulary can represent; an unproved operation is not accepted merely because no concrete instance is reachable.
Generic integer and float type parameters are copy datums only for exact opaque goals in this schema and are not [ENT-2] L0 fragment types, while an integer-typed const generic remains the symbolic constant term [ENT-2] fixes.
An FN-9 schema goal exists only when its result datum, selected return, and normalized relation are expressible in the finite schema vocabulary; otherwise it is rechecked in every inhabited concrete instance and is never approximated.
The schema publishes no executable function, callable summary, or lowering authority.
Every inhabited concrete [FN-2] instance is rechecked independently after substitution; a contradictory path discharges only by [ENT-4] and never bypasses an ordinary formation check.
If concrete instances disagree, the first invalid concrete instance in stable instance order rejects the shared source occurrence.

The fragment joins the trusted computing base exactly as the type and ownership checkers do [SCOPE-3]; a wrong derivation is a compiler defect owned by implementation repair and tests, not a second runtime validation layer.
No implementation may add a fact source, relation family, closure rule, proof rule, protected operation family, or callable publication surface beyond those defined here.
[ENT-2] The fragment constructs one ProofContext for one concrete function body at a time.
No caller fact is copied into a callee: an ordinary call judges its instantiated [FN-8] goal in the caller's entering state, the callee body begins with its own proved requirement as [ENT-3] source S4, and only a separately FN-9-verified earlier-SCC summary may establish its instantiated normal-result relation back in the caller.
A fragment type is one member of the closed integer set [OP-2]; relations are over mathematical values, so relations between terms of different fragment types are well-formed and are created only by the sources and flow transports [ENT-3, ENT-5] admit.

A term is exactly one of: (a) a tracked place — a `place` [GRAM-5] whose root `pbase` IDENT resolves to any `let_stmt` binding, a `for_stmt` binder, a `param`, any match binder regardless of its [OWN-13]-derived mode, or a named const [CONST-2], formed with any number of field-selection and enum-payload `psuffix`es and `deref` wrappings and no subscript suffix, whose final selected type is one fragment type; (b) a subscripted readonly field — a `place` [GRAM-5] whose root resolves as in (a), formed with any number of field-selection and enum-payload `psuffix`es and `deref` wrappings and at least one subscript, whose final step selects a readonly field [TYPE-2] of one fragment type, `table[i].len` [MSR-1] and a writer's `nodes[i].count` alike; (c) a constant — the mathematical value of an integer literal or of an integer-typed named const, or symbolically an in-scope integer-typed const-generic parameter; (d) one of the two compiler-owned u64 capture terms belonging to an admitted `for_stmt`, identified exactly by `(that for_stmt's NodePath, lower)` or `(that for_stmt's NodePath, upper)`; (e) the one compiler-owned symbolic result datum of an admitted FN-9 clause while its RelationTemplate is formed, identified by that `ensures_clause`, its route or unrouted class, and fragment type; (f) the one compiler-owned commit value of an admitted [SET-1] `set` whose right-hand side has one fragment type, identified exactly by `(that statement's NodePath, that fragment type)`; (g) the distinguished zero term Z, used only to carry constant bounds and [ENT-6]'s normalized integer-domain components; or (h) one compiler-owned measure datum [MSR-3], which is a call datum [ENT-3.S13], identified exactly by `(that call's NodePath, the formal ordinal, that operand's ordered projections, whether it denotes the operand's value or one measure of it)`; an entry datum, identified exactly by `(the formal ordinal, that operand's ordered projections, which measure it denotes)`; or a placement datum, identified exactly by `(that statement's NodePath, which placement of [MSR-3]'s placement table it stands at, the ordinal within that statement, the ordered owned descendant projection, which measure it denotes)`. The final alternative (i) is the private integer success-payload parameter of an ENT-5 conditional Result context, typed by the Ok payload and scoped to that context; the same formal name in two contexts does not identify their values.
The FN-9 result datum occurs only in its template: every selected-return or caller query substitutes it with an ordinary term, constant or the private payload parameter of ENT-5's conditional Result context. That typed parameter denotes only the success payload of the value associated with its context; parameters of distinct contexts have no shared value identity. It is compiler-owned, unwritable, carries its fragment type's standing bounds, and is substituted away at an ordinary success delivery. Neither symbolic datum creates runtime storage.
Two places are the same term exactly when their roots resolve to the same declaration event [TYPE-6, DIAG-1] and their canonical source spellings [FORM-2] are byte-identical; a fresh binding legally reusing an expired spelling is a distinct term, and distinct spellings are distinct terms even when they resolve to overlapping storage.
Term identity thus under-approximates aliasing, while kills [ENT-5] use [OWN-7]'s resolved-place overlap relation and over-approximate it.
A readonly field below a subscript is a term because every event that changes one writes a place containing its storage: outside its declaring module it is never a write target [TYPE-2], so the event is a replaced value holding it [SET-1], an element exchanged or updated with it [OP-11, OP-12], a window part an [OP-10] operation moves, or the field itself under a row that writes it [EFF-2], and inside that module an assignment to it writes its own place; the ordinary [ENT-5] kill reaches every term over it in either case.
Each subscript in a clause (b) place is an [OP-4] occurrence like every other and owes that rule's own obligation against the base it indexes, submitted to [MSR-4] where the place is formed; a place whose subscripts are not all discharged is no term, exactly as an undischarged subscript in read position is no value.
A requirement forms its places at body entry, in the state holding the requirements written before its clause [FN-8], and a definition's places are formed in the first requirement whose expansion reaches them, so `requires k < deref(rows)[i].len;` needs `i < deref(rows).len` from an earlier requirement.
Each offset occurring inside a clause (b) place is itself a clause (a) or clause (c) term, because the place's identity is decided over its offsets [OWN-7] and [ENT-5] kills a term only through its support, which takes in each offset's own: an offset of any other form, such as an element read or a computed value, could come to select another element with no event killing the term.
A place carrying any other offset is no term, and reading it remains an ordinary read.
An implementation that cannot represent an admitted offset reports the place as an unsupported compiler capability, never as a source rejection [DIAG-1].

After TYPE-5 succeeds, each `for_stmt` endpoint atom is admitted only when its evaluated value is itself one preceding term or constant.
An in-scope const generic is such a constant and is therefore an admitted endpoint [MSR-6].
Any other atom is a hard error citing ENT-2 at that endpoint's `atom` node, with `SourceCoordinate` equal to its complete checked half-open source extent and a repair [DIAG-1].
In particular endpoint position makes no place a term: a subscripted place is an admitted endpoint exactly when it is a clause (b) term.
The two capture terms are finite, immutable, compiler-owned, and not source bindings or source places: source cannot name, write, borrow, move, or shadow them.
Their scope begins after their respective once-only endpoint captures and ends on every edge leaving the counted construct.
The counted binder's compiler fact scope begins at its initialization and ends on every edge leaving the counted construct, even though [TYPE-6] makes its source name visible only in the body.
A commit value is compiler-owned and unwritable in the same sense, and denotes the one value its `set` occurrence's right-hand side evaluated to: it exists from that evaluation, no [ENT-5] event kills it, and no later write can retarget it.
A call datum is compiler-owned and unwritable in the same sense, and denotes the value one `own` operand of a declared relation had at its call's pre-transfer point [ENT-5]: it exists from that point, contains no place, and no [ENT-5] event kills it.
One static term per statement is enough because [ENT-3]'s forward flow visits every statement of one function body exactly once: a loop body is walked once from the head state [ENT-5] forms before that walk, each `match` arm walks its own statements, and no statement is visited twice in one analysis.
A commit value therefore denotes that statement's value in the one abstract evaluation the walk performs, exactly as a counted header image denotes the binder's value in an arbitrary iteration, and every fact derived about it holds of each dynamic evaluation of that statement separately.

An FN-9 parameter datum denotes its function-entry image in the RelationTemplate but creates no snapshot term.
Local proof may reuse the ordinary parameter term only while FN-9's entry-image stability remains live; caller publication substitutes the corresponding pre-transfer actual image independently for each referenced formal.

A concrete goal is one finite typed expression tree with exact result `own Bool` formed under [FN-8]'s structural identity, either by concrete substitution of a GoalTemplate, by [ENT-3]'s goal-origin judgment in the current function, or as the canonical total predicate of an [ENT-6] operation obligation.
A concrete place datum retains the resolved root declaration event and its ordered field, enum-payload, and `deref` projections, and the subscripts of a clause (b) place; an actual substituted for a reference formal uses the resolved referent datum, while an own actual uses its pre-transfer datum.
Named consts and typed literals retain the identities FN-8 fixes.

A direct value expression is the finite typed tree formed from those datums and the pure total operation rows admitted by [FN-8].
An admitted value expression is a finite tree recursively formed from direct-value rows and selected exact integer-operation, exact conversion, or index rows.
Each selected partial row may enter that tree only after its own occurrence and every nested child obligation have succeeded in source evaluation order.
An index row retains its indexable family and exact selected element, length, and range arguments as applicable.
This admitted structure records the mathematical identity of the value already proved safe at that occurrence; it neither makes a subscript an L0 term nor authorizes evaluation before its owning nested obligation has succeeded.
Two occurrences of the same admitted typed tree therefore have the same value identity, but each occurrence separately discharges its nested operations and an earlier signed fact remains available only while [ENT-5] retains its support.

An evaluated-value datum is the finite occurrence-local identity for a value that has already been evaluated but has no admitted value expression.
FN-8's call-argument form is identified by `(concrete caller instance, call NodePath, argument ordinal, exact captured type, ordered projections, final result type)` and may occur only in the instantiated goal of that one ordinary call.
An [ENT-6] obligation-operand form is identified by `(concrete function instance, owning obligation NodePath, operand ordinal, exact captured type, ordered projections, final result type)` and may occur only in the canonical Goal queried for that one obligation.
Both forms are neither places nor L0 terms, have no direct or complete ordinary source goal origin, add no flow fact or place support, and cannot be established by naming or reevaluating their source expression.
Goal equality is exact typed tree equality, including every selected row and datum field, and therefore may hold across two source occurrences or concrete callee instances only when their complete typed trees are identical.
The finite goal universe of one concrete function is exactly the goals formed from its admitted Bool origins, requirement S4 sources, instantiated ordinary-call requirements, and the canonical OP-2, OP-6, and OP-9 operation obligations, together with the finite parent and child trees their fixed decomposition and reconstruction rules visit.
Invariant targets and `proof_use` sources are affine inequalities rather than opaque Goals [INV-1, PRF-1]; an OP-4 bounds obligation remains an L0/affine relation and has no opaque Goal of its own.
Goal construction may intern only written subexpressions and the exact normalized components fixed by their owning rules; it synthesizes no arbitrary formula or unbounded algebraic search.

A signed opaque fact is exactly `+G` or `-G` for one concrete goal G, meaning that exact whole expression evaluated respectively true or false.
It carries no child facts merely by existing; [ENT-3] fact sources establish their selected signed contribution and [ENT-4] alone performs the finite parent reconstruction below.
If G's complete root is exactly one comparison origin relation R under [ENT-3], `+G` has the exact L0 projection R and `-G` has R's exact negation; a non-comparison root has no L0 projection.
The signed fact and its projection are distinct manifestations in one combined state and have the supports [ENT-5] fixes.

An atomic fact is one difference bound `t1 - t2 <= c` (t1, t2 terms, c a mathematical integer) or one disequality `t1 != t2`.
Difference-bound identity preserves the ordered term pair; disequality identity is the unordered endpoint pair, although the first source-normalization encounter preserves its written orientation for rendering and component order.
Source relations normalize exactly: `a <= b` is `a - b <= 0`; `a < b` is `a - b <= -1`; `a = b` is the bound pair `a - b <= 0` and `b - a <= 0`; `a >= b` and `a > b` swap operands; `a != b` is one disequality.
A constant operand folds through Z: `a <= 7` is `a - Z <= 7`.
Implicit facts hold at every program point: every term t carries the reflexive bound `t - t <= 0`; every term t of fragment type T carries `t - Z <= max(T)` and `Z - t <= -min(T)`; every measure term carries [MSR-2]'s standing facts; and every `P.len` term over a place of type `Array<T, N>` carries the equality to N (both bounds), with concrete N a constant and const-generic N a symbolic constant term.

[MSR-1] Measure terms are the readonly fields of the storage shapes, over one place, for every measured value [OP-15].
`P.len`, `P.cap`, and `P.head` — the readonly fields the prelude declares on `Array`, `Slots`, and `Ring` [PRE-1], and the `len` of a range reference, which no declaration states [REF-4] — are [ENT-2] terms of fragment type u64 over an admitted measure place P: a clause (a) tracked place when P carries no subscript and a clause (b) place when it carries one.
An admitted measure place is a `place` rooted and formed as in [ENT-2] clause (a) or clause (b), whose final selected type is a measured type.
The subscript admission is what makes `table[i].len` a term, so a storage whose elements are themselves storages has provable operations; it is also why [MSR-2]'s granularity is stated over storage rather than over the word *element*.
Its subscripts and their offsets are those [ENT-2] clause (b) admits.

Which measures a type has is its prelude declaration's readonly fields; what each denotes, and whether it is *exact* or *bounded*, is table data.
The rule is that the table exists, gives every measured type a row, and gives every cell exactly one of *exact*, *bounded*, or *absent*.
An **exact** measure is one every writing operation publishes a value for; a **bounded** measure is one some writing operation can publish only a two-sided range for.
A measured type is exactly a type the table gives a row to; every other type has no measure term, and a measure member read on a place whose final selected type is any other is the ordinary [TYPE-5] operand rejection at that place, carrying the measured types the table has a row for.
The table in this version is:

```wf-measures
| measured type   | len                      | cap                | head                    |
|-----------------|--------------------------|--------------------|-------------------------|
| Array<T, N>     | N, exact                 | absent             | absent                  |
| Array<T>        | allocated slots, exact   | absent             | absent                  |
| Slots<T, N>     | initialized slots, exact | N, exact           | absent                  |
| Slots<T>        | initialized slots, exact | slots taken, exact | absent                  |
| Ring<T, N>      | initialized slots, exact | N, exact           | window origin, bounded  |
| Ring<T>         | initialized slots, exact | slots taken, exact | window origin, bounded  |
| &[T]            | range elements, exact    | absent             | absent                  |
```

Exactly one cell class is *bounded* anywhere — a `Ring`'s `head` — and it is the one cell the two `Ring` rows share: the two front-moving operations `place_front` and `take_front` [OP-10] publish it two-sidedly and no operation re-establishes it exactly, so no derivation may treat a `Ring`'s window origin as a known constant after a front operation.

A measure is a logical quantity, and a measured value's window origin is `P.head` where the table gives that cell and slot zero where it does not.
A measured value's initialized set is the `P.len` slots beginning at that origin taken modulo `P.cap`, and a **logical offset** `i` names the slot at physical offset `(origin + i) mod P.cap`.
Every measure term and [OP-4] obligation is stated in logical coordinates, and one sentence carries a logical conclusion to a storage conclusion:

> `i |-> (origin + i) mod P.cap` is injective on `[Z, P.len)` because `P.len <= P.cap`, so two disjoint logical ranges of one measured value describe disjoint storage.

That sentence is a definition proved from `P.len <= P.cap`, which [MSR-2] publishes as a standing fact; it is never a separate obligation an occurrence submits.
Where a row's `head` cell is absent the map is the identity on `[Z, P.len)` and the sentence's conclusion is immediate; where a `Ring`'s `head` is bounded and may be nonzero, the sentence is what carries two disjoint logical ranges of a wrapped window to two disjoint storage ranges, and it is the whole reason a logical obligation is a storage guarantee.

[MSR-2] Support is descriptor storage, a kill is an ordinary [ENT-5] event, and a standing fact has empty support.
A measured value's storage is two disjoint parts: its **descriptor storage**, the measure words its value carries, and its **element storage**.
The support of a measure term over P is the one descriptor word that term names — `len`, `cap`, and `head` are three disjoint words of P's descriptor storage — every reference variable and every `Box` binding any prefix of P reads through, and the support of every offset occurring anywhere in P.
P's descriptor storage is exactly the [REF-1] resolved place of P itself, not the resolved place of P's root: a measure of `frame.tail` is supported by `frame.tail` and not by `frame`.

The kill is [ENT-5]'s own rule with no new overlap notion: a measure term dies exactly on an [ENT-5] event whose written place overlaps its support under [OWN-7], where an event is any [SET-1] commit, [OP-11] `swap`, [OP-12] update, consume, scope exit, or any action carrying a `writes` occurrence that projects onto that storage under [EFF-2].
Stating the kill over the effect row keeps it closed when a later family derives a new action.
A row entry that names one measure word, `writes(window.len)`, overlaps that word alone, so it kills the `len` facts of the actual and no `cap` or `head` fact; an entry naming the whole window overlaps all three.
The granularity is stated once, over storage, and nothing is derived from the word *element*:

> A write at an element position of P overlaps the descriptor storage of `P[i]` and none of P's own descriptor storage.
> It therefore kills every measure of `P[i]` and no measure of P, whether the write is a [SET-1] commit, an [OP-11] `swap`, or an element write of a scalar — for which the set of killed measures is empty because a scalar has none.

Two consequences follow as derivations rather than clauses.
A write to a sibling field does not kill, because the descriptor storage of `deref(r).flags` and that of `deref(r).tail` do not overlap.
A write to an offset occurring in P kills at every level, because that offset's support is part of every enclosing measure term's support.
The element-position carve-out of [ENT-5] is removed rather than narrowed: a measured element type is admitted [TYPE-9], so a write at an element position kills that element's measures by the ordinary storage rule and nothing is derived from the word *element*.

At every point at which P is live these hold implicitly, as [ENT-2] implicit facts that no event kills, each for a place whose row has the cells it names:

```text
Z <= P.len     Z <= P.head     P.len <= P.cap     P.head <= P.cap
```

A contract clause both of whose sides follow from these standing facts alone discharges no obligation.
A measure whose value the table fixes as a compile-time constant or a runtime-profile symbol is a standing fact with empty support: for `Array<T, N>` `P.len = N`, for `Array<T>` and `&[T]` no constant beyond `Z <= P.len`, and for a constant-capacity `Slots<T, N>` or `Ring<T, N>` `P.cap = N`.
A row whose cell is *bounded* fixes no such constant: a `Ring`'s `head` is a standing fact only through `Z <= P.head` and `P.head <= P.cap` above, and a window's `len` is an ordinary killable term.
A standing fact holds at every program point of P's scope and no event kills it, exactly as an [ENT-2] implicit fact does.

[MSR-3] One denotation per operand position, keyed on the parameter's mode, on what its declared row writes, and on the explicit entry former.
The complete measure table is:

```text
| measure operand position                                  | inside the callee     | at the caller        |
|-----------------------------------------------------------|-----------------------|----------------------|
| requires, any parameter                                   | entry image           | pre-transfer term    |
| ensures, own parameter                                    | immutable entry datum | immutable call datum |
| ensures, deref(reference parameter the row only reads)    | immutable entry datum | live term            |
| ensures, deref(reference parameter the row writes)        | exit-state term       | resolved exit place  |
| ensures, deref(entry(reference parameter the row writes)) | immutable entry datum | immutable call datum |
| ensures, result binder                                    | selected result       | result destination   |
```

The proof-only former `entry(parameter)` is admitted only in an `ensures_clause` and only when its direct IDENT resolves to a reference parameter of that function whose declared row carries a `writes` of that path; every other occurrence is a hard error citing MSR-3 at the former, with a repair [DIAG-1].
It has that parameter's ordinary reference kind for projection checking. Ordinary explicit dereference and field projections follow it, as in `deref(entry(buf)).len` and `deref(entry(frame)).tail.len`; it is no runtime value, allocation, or snapshot copy.
The former selects the entry denotation of the projected measure. A bare `deref(buf).len` in ensures instead selects exit state. A nested `entry`, an expression argument, entry of a local, entry of an own parameter, and entry of a reference parameter the row does not write are not admitted.
Non-measure parameter datums retain [FN-9]'s entry-image stability judgment; this former adds no scalar snapshot family.
An `own` operand denotes the call datum because its caller cannot name the consumed value's post-state. The referent of a reference parameter the row writes is still the caller's resolved place after the call: its exit measures can therefore be checked at returns and instantiated there without transferring its owner.
Entry and exit measures are distinct terms even when both project from the same formal and actual. The exact projected effects kill the caller's supported facts before the verified exit relations establish [CALL-6]; no syntactic property of an actual may retain or kill a fact in place of that effect judgment.
A [PRE-1] or [PRE-2] declaration uses this same spelling, explicit dereference, and denotation, with no separate snapshot notation.

A **call datum** is a compiler-owned immutable [ENT-2] term with empty support: no place occurs in it, no [ENT-5] event kills it, and no later write retargets it.
There is one former, keyed on what a datum denotes: a datum is identified by `(that call's NodePath, the formal ordinal, that operand's ordered projections, whether it denotes the operand's value or one measure of it)`, is compiler-owned and immutable, and is established equal to that operand's pre-transfer term at the call's pre-transfer point [ENT-3.S13].
A datum is formed, never proved.
When the operand's pre-transfer term is itself immutable with empty support — a constant, a symbolic const-generic parameter, a counted capture, a commit value, or another call datum — nothing can retarget what that term denotes, so the datum is that term and no second one is formed; every other operand mints its own.
Its placement is the call, which is one of the events at which the language undertakes to carry a value's measures.

An **entry datum** is the same former at the second placement, body entry.
For each parameter of measured type and each [MSR-1] measure of it that a declared relation of that function names, one compiler-owned immutable term is identified by `(the formal ordinal, that operand's ordered projections, which measure it denotes)` and established equal to that measure at body entry.
It is the same kind of term as a call datum and carries the same closure: no place occurs in it, no [ENT-5] event kills it, and no later write retargets it.
That is what the immutable entry-datum cells of the table above denote.
A body that overwrites an `own` parameter's local binding with newly constructed storage — `set vector = move fresh;` — therefore leaves every clause naming that parameter's measure meaning exactly what it read as at entry, and the caller reading the same clause after substitution reads that call's call datum, which the same statement's commit cannot kill either.
For a reference parameter whose declared row writes it the immutable entry datum is named explicitly through `deref(entry(parameter))`; the same measure read through `deref(parameter)` without that former instead denotes the selected return's resolved referent.
An entry datum is formed, never proved, and it is not a second fact source: its standing orderings [MSR-2] reach it through the equality it is established with, exactly as they reach any other term.
A parameter operand that is not a measure keeps the entry-image judgment [FN-9] states over the live place, since a value of fragment type is not a measured value and has no measure datum.
A **placement datum** is the same former at every remaining placement, each of which is one naming event inside a body in the table below.
Each event carries the measures of the transferred value and its exact owned measured descendants from its **source** place to its **destination** place.
An **owned descendant projection** is an ordered sequence of struct-field selections, enum-payload selections, and selections of a `Box`'s `inner` content, each selecting a value owned by the preceding value, ending at the first measured type [MSR-1]; the sequence is empty when the transferred value itself has that type.
The source and destination need not themselves have a measured type: the same projection from each selects the measured value whose measures the event carries.
The projection contains no subscript, range, or dereference of a borrowed referent, so a measured storage's elements are outside it.
Its source identity is exact [ENT-2]; an alias or a set of possible descendants used by [OWN-7]'s overlap judgment does not identify that source.
For each carried projection, one compiler-owned immutable term per [MSR-1] measure of its endpoint is identified by `(that statement's NodePath, which placement of the table below it stands at, the ordinal within that statement, the ordered owned descendant projection, which measure it denotes)`, is established equal to that measure of the projected source immediately before the statement's own kills, and is established equal to that measure of the corresponding projected destination at the statement's normal continuation after those kills.
It is the same kind of term as a call datum and carries the same closure: no place occurs in it, no [ENT-5] event kills it, and no later write retargets it.
That the datum is minted before the statement's kills and read after them is the whole content of every placement: the event consumes or overwrites the source, so without a term with empty support standing between the two places a measured value would arrive at its new place with no measures at all, and `let built = move spare;` would lose what the caller proved about `spare`.
A placement datum is formed, never proved, and it is not a second fact source: its standing orderings [MSR-2] reach it through the equality it is established with, exactly as they reach any other term.
The finite measure vocabulary already formed at the event suffices for these equalities to carry the available measure relations; a descendant outside that vocabulary has only [MSR-2]'s standing facts, which hold at its destination without transport.
This applies equally to recursive types: each admitted projection is finite, its depth has no fixed bound, and type recursion creates no requirement to enumerate further descendant terms.
A path's presence in that vocabulary establishes neither a relation nor a variant refinement; the source equality reads the current pre-kill state after all earlier events and their kills, and a discarded fact is not restored by finding its former path.
The complete placement table is:

```text
| the naming event                                                      | source place                | destination place            |
|-----------------------------------------------------------------------|-----------------------------|------------------------------|
| the REBIND: one `let` binder, or one [SET-1] `set` target that is a   | that place                  | the binder, or the place the |
|   place, whose right-hand side is a bare use of one place             |                             |   commit writes              |
| the ELEMENT: the same, where the [SET-1] target is an element         | that place                  | `P[i]`, the element position |
|   position of a window                                                |                             |   the commit writes          |
| the CONSTRUCT: one field operand of a constructor `call` that is a bare use  | that place                  | that field of the            |
|   of one place                                                        |                             |   constructed value          |
| the DESTRUCTURING: one binder of a destructuring consume [GRAM-4]     | that field of the operand   | the binder                   |
|   whose operand is a bare use of one nominal place                    |                             |                              |
| the PAYLOAD: one arm binder of a `match` whose scrutinee is a bare    | that field of the           | the arm binder               |
|   use of one enum place                                               |   scrutinee's payload       |                              |
```

A `move p` is such a use: [OWN-1] requires the `move` spelling of an affine place, so it participates in the placements of the table exactly as a bare use of a copy place does.
A right-hand side, operand, or scrutinee that is anything but a bare or `move`d use of a place mints none, and the ordinary sources establish whatever that expression publishes.
The source or destination place itself may contain a written subscript admitted by [MSR-1]; the carried projection begins at that selected value and retains the boundary above.
The element placement names an element position, and an element position is a place exactly where its offset is one a place relation can name [MSR-1]: a written literal, a live `own` fragment-integer binding, or an in-scope const generic [MSR-6].
Two element places are decided by their offsets [OWN-7], so an offset provably distinct from nothing — itself included — would relate two elements of one window as one term; a commit at such an offset carries no measure and, being an element write of unknown position, kills every measure of every element of that window [MSR-2].
The element placement reaches only a written element position, so a window operation [OP-10] carries no measure through the slot it writes: `place_back` stores its value at the entry length of its referent and `take_back` takes one from the exit length of its referent, and a measure term is not an offset this version admits.
A window put into a slot by a window operation and taken back out by one therefore arrives with no measures of its own, and a caller that needs one reads it and branches [MSR-4].
The payload placement names the field of the arm's selected variant, reached by the payload step `.Variant.field` [GRAM-5] and available under that arm's refinement fact [ENT-3.S15].
A tracked place's path admits that step [ENT-2], so an enum more than one of whose variants carries fields needs no separate treatment: each arm's path names its own variant's storage.

*Judgment:* the denotation at every operand position, and the restriction of `entry(parameter)` to a reference parameter the row writes, in `ensures`.
*Publishes:* the call datum at the call placement, the entry datum at the entry placement, the placement datum at every placement of the table above, and the denotation table.

[CALL-1] Through a reference the callee only reads, every fact survives.
For an argument whose declared parameter is a reference whose row carries no `writes` of any path rooted at it, of any type, the call is a kill event for no fact supported by that actual's path.
The whole ground is [EFF-2]'s both-ways check: a body exhibits no write its declared row does not carry and this row carries none, so no `writes` occurrence projects onto that place and no [MSR-2] kill fires.
That ground is exactly as strong as the set of actions this document classifies as writes, so a later family that makes a new action a write of a referenced path reaches this rule through [EFF-2] alone and needs no clause here.

*Judgment:* none; the absence of a kill, which is [MSR-2]'s kill not firing.
*Publishes:* the survival of every fact supported by the path of an actual at a reference parameter the row only reads.

[CALL-2] Through a value passed and returned, only the contract's facts exist on the result.
An `own` argument whose type is affine or linear [OWN-1, PROV-6] is a consuming use, so every fact whose support contains that binding's root dies at the call [ENT-5](c), that place's measures included.
An `own` argument whose type is copy is a duplicate and not a consuming use, so the caller's place and every fact supported by it survive the call and are available at the next one.
The result is a fresh binding carrying exactly the callee's declared relations, established as [CALL-6] states, and no fact of any argument reaches it: a caller that wants a relation between what it handed over and what it got back reads it from the contract or does not have it.
A declared relation may name a consumed operand's measure, which at the caller denotes that call's call datum [MSR-3]; a datum has empty support, so the consume the same statement performs cannot kill it and the relation means what it reads as.

*Judgment:* the ordinary [ENT-3.S12] establishment, subject to the denotation [MSR-3] fixes.
*Publishes:* the callee's declared relations on the result, and nothing else on it.

[CALL-3] A write through a range reference reaches the range's storage and no measure of the origin place itself.
For a range reference parameter `&[T]` [REF-4], a projected callee `writes` occurrence kills every fact whose support overlaps the **range's storage**, which for an element type having descriptor storage of its own includes that element's measures, and kills **no measure term over the origin place itself and none over the range reference**.
For every other parameter the projected write kills measures as an ordinary descriptor-storage-overlapping [ENT-5] event [MSR-2].
The classification is stated over storage and nothing is derived from the word *element*, exactly as [MSR-2]'s granularity is: when the ranged element type is itself measured, the range's storage **is** the descriptor storage of the origin's elements, so a measure of a ranged element dies and a measure of the origin survives.
The descriptor/element split is a property of the element type, not of the word *element*.
This judgment applies to every admitted element type [TYPE-9, REF-4], including one whose element storage contains a descriptor.

*Judgment:* the kill classification per declared parameter, which is [MSR-2]'s judgment parameterized by what the parameter reaches.
*Publishes:* the surviving measures of the origin place and of the range reference.

[CALL-5] No transport reads the actual's spelling.
The transport a call selects for one argument is fixed by the callee's declared parameter mode and type and by its declared contract, and by nothing else: not the argument expression's shape, not the callee's body, not its name, and not any per-parameter summary derived from a body.
An ordinary function's declaration is its complete call boundary whether its definition is a Whitefoot body or supplied by linking.
The exact declared effect row is projected onto each actual's resolved places. A caller fact dies exactly when a projected write overlaps its ordinary support [ENT-5, MSR-2]; the actual's syntactic shape creates no write and removes none. A parameter absent from the declared writes has no write kill.
For a projected write through a parameter that is not a range reference, the affected extent is the ordinary descriptor storage [CALL-3]. Thus a whole-window write through a reference formal kills the old window facts, including when the actual is reached through `deref` or a nested field. A body that changes only elements can still have the same declared `writes(r)` row as a whole replacement; callers frame neither body beyond what that exact declaration states.
Window mutation uses the operations of [OP-10]; a source helper over a written reference parameter may publish the verified exit measures it promises [FN-9, MSR-3]. With no ensures the caller obtains no replacement fact from the mere presence of a written reference parameter. A genuine empty/nonempty branch after rereading length may supply a new fact; no runtime check substitutes for a required static proof.

*Judgment:* the conservative default for every parameter that is not a range reference.
*Publishes:* the absence of any call-site-derived or body-derived classification.

[ENT-3] The fact state is defined constructively over the conservative structural normal-control graph [FN-1]: each source below establishes its L0 and signed-goal facts at its stated point; facts flow forward along normal edges; kill events apply on the edges where [ENT-5] places them, with scope-exit kills applied before any join; merge points take the [ENT-5] join and loop heads the [ENT-5] loop rule; and the state queried at any point is the [ENT-4] closure of that flow.
Dominated straight-line establishment is a consequence of this construction, not a second definition.
Nothing else is a fact: a writer's `ensures_clause` is only an FN-9 proof obligation, never a trusted source; a written header or local invariant conclusion has no authority until INV-1 and any applicable PRF-1 certificate prove it; no struct invariant, compiler-invented loop proposition, inferred summary, or unverified user-function result exists.
S11 is only the compiler-owned consequence of the counted operations [FN-1] actually executes, and S12 exists only from the declaration relations available under FN-9: a separately verified earlier-SCC summary or a PRE-1 or PRE-2 supplied declaration, under the publication formula below.
Each accepted fact retains the constructor identity and direct parents that already produced it; this diagnostic information establishes and kills no additional relation or signed goal, and no [ENT-4] answer depends on a second provenance state.

A comparison origin is defined first.
An expression has comparison origin R when (a) it is an `infix` expression whose operator is a `compare_op` — `==`, `!=`, `<`, `<=`, `>`, `>=` [OP-2] — and whose two operands are each a term or constant, R the corresponding relation over them; or (b) it is a bare IDENT naming a `let` binding of type `own Bool` whose initializer right-hand side satisfies (a) with relation R, no [ENT-5] kill event (a)–(d) applies to a fact supported by an operand term of R on any path from that initializer to the use, and the binding is the target of no `set` on any such path.
No other shape has one: `band`, `bor`, `bxor`, `bnot`, `eeq`, `ene`, user-function results, and deeper indirection chains contribute no L0 comparison origin in this version; an established Boolean goal contributes relations only through the members of its signed decomposition set.

An expression has operation-domain-predicate origin G when (a) it is one total `+defined`, `-defined`, `*defined`, `/defined`, `%defined`, `ineg.defined`, `iabs.defined`, `ishl.defined`, `ishr.defined`, or `cvt.defined` operation with its selected types and complete ordered admitted value-expression identities, after every nested obligation in those operands has succeeded, G that exact typed GoalExpression; or (b) it is a bare IDENT naming an own-Bool ordinary-let binding whose initializer satisfies (a), no [ENT-5] kill event applies to G's support on any path from that initializer to the use, and the binding is the target of no `set` on any such path.
This origin is one ordinary exact goal, not a second fact channel.
Its support, expansion, kills, scope exit, joins, and signed establishment are the ordinary goal rules below.

A Bool expression has an ordinary goal origin G when, after its ordinary expression judgment and every nested operation obligation have succeeded, its completely typed expression is one admitted value expression and its root is a pure total operation-table row, with exact tree identity as [FN-8] fixes.
Construction, an ordinary function call, a move or borrow, an undischarged partial operation, an expression requiring occurrence-local evaluated-value identity, and every other expression shape has no goal origin.
A checked exact integer operation or subscript may therefore occur only below that total root and only through the admitted structure above; it never establishes its own safety merely by occurring in G.
The unexpanded tree G is the direct goal.
Starting from that direct goal, its complete origin expansion recursively replaces an ordinary-let datum by that binding's unique defining right-hand side exactly when the right-hand side itself has an admitted value expression formed after its own nested obligations succeeded, the binding is no `set` target on any path from that initializer to this use, and no [ENT-5] kill event applies to the replacement's support on any such path.
Expansion continues to a fixed point and is all-or-nothing for every eligible leaf; it never performs an algebraic rewrite.
The goal-origin set is the direct goal plus that one complete valid expansion when it differs.
Thus a condition binding's own Bool value and its still-valid computation origin are both retained: a later write to an origin place kills the expanded goal but not the already-computed binding goal, while a write to the binding kills the latter normally.
Definition expansion in FN-8 is unconditional because every `contract_define` is erased pure proof syntax and the admitted block contains no mutation.

Signed Boolean decomposition applies at every ordinary establishment of a signed goal fact by the sources below.
The decomposition set of `+G` whose complete root is `band(A, B)` is `+A` and `+B` together with each member's own decomposition set; the decomposition set of `-G` whose complete root is `bor(A, B)` is `-A` and `-B` together with each member's own decomposition set; the decomposition set of either sign of `bnot(A)` is the opposite sign of A and its recursive decomposition; `-band` and `+bor` remain exact disjunctive roots and have no child member.
Every admitted member whose root has an exact comparison projection also establishes that signed projection.
Each member has its own [ENT-5] support, kills, joins, and loop treatment.
This is a finite structural walk with no algebraic rewrite.

The sources are:

[ENT-3.S1]
- S1 (branch facts).
At an `if_stmt` or `value_if`, each goal G in the condition's goal-origin set is established as `+G` at the then-block's entry and `-G` at the else-block's entry; for an else-free `if_stmt`, `-G` is established on the false edge, which joins the then exit at the continuation [ENT-5].
Independently, when the condition has comparison origin R, R is established at the then entry and R's exact negation at the else entry or false edge.
L0 negation is exact over mathematical integers: the negation of `a - b <= c` is `b - a <= -c - 1`; the negation of `a = b` is `a != b` and conversely.
[ENT-3.S4]
- S4 (requires facts).
At a concrete function-body entry, its complete instantiated [FN-8] goal G is established as `+G`.
When and only when G's complete root is one comparison admitted by comparison-origin shape (a), whose operands after template and call substitution are each an admitted term, constant, or measure term, that exact relation R is also established.
Beyond that projection, only the members of G's signed decomposition set and their projections are established; no other child of any goal is established.
For G and each member of that same signed decomposition, in the existing member order, an integer ordering leaf with no L0 projection also establishes its [ENT-6] affine ordering normalization when that normalization is admitted. Its written `<`, `<=`, `>`, or `>=` and established truth sign determine the one inequality; negation reverses the order with the ordinary integer strictness adjustment. Equality, disequality, nonlinear products, and undecomposed Boolean children supply no additional image. The normalization uses the body-entry immutable scalar and measure images and is appended as one ordinary automatic affine premise, with no loop assumption. A later replacement or measure kill cannot retarget those captured images. An ordering leaf that already has an L0 projection adds no affine premise by this family, so this rule does not duplicate ordinary L0 bounds to enlarge AUTO's premise combinations.
S4 is the admitted-body axiom justified by every ordinary caller's static discharge; no callee-entry prologue or boundary check executes.
[ENT-3.S5]
- S5 (copy and conversion equalities).
An `ordinary_let_rhs` establishes at its binding: for `let x = lit;`, x = value(lit); for `let x = p;` with p a term of type T, x = p; for `let y = cvt::<Src, Dst>(p);` with integer Src and Dst and p a term or constant, y = p after its [OP-6] obligation succeeds — `cvt` keeps its written type pair [TYPE-5].
A successful [SET-1] commit to a direct fragment-typed place first evaluates its right-hand side to that occurrence's commit value v, establishing at v exactly the [ENT-3] image the same right-hand side establishes at an `ordinary_let_rhs` binding: this clause's three rows and every S6, S7, and S9 row whose conclusion is a relation over the bound value itself.
A row concluding instead over a measure term of the destination place has no commit form, a commit value being no place.
Every fact supported by the old target value then dies under [ENT-5], and only then is the post-write equality x = v established.
Evaluating v before that kill is what lets [ENT-5]'s pre-kill closure carry the value's surviving consequences across the write, and the equality still carries no old target fact, since v is a term distinct from x and every fact naming x has died.
An index target and a non-fragment target receive no commit value, and a right-hand side whose form matches no image row forms none either: with no commit value to name, S5 establishes no post-write equality and adds nothing to the state [ENT-5]'s kill leaves, and no S5 commit image beyond that exists in this version.
[ENT-3.S6]
- S6 (length facts).
S6 carries no construction row: a construction function's length, capacity and origin facts are the `ensures` of its [PRE-1] record and reach the caller through [ENT-3.S12] like any other declared relation.
A `let` binding a measure term is [ENT-3.S5]'s ordinary copy equality, a measure term being a term [ENT-2]; this row adds none of its own.
`let part = &P[lo..hi];` for a tracked P establishes `deref(part).len = hi - lo` after [REF-4]'s domain goals discharge, over the exact current-value images captured where the endpoints are evaluated.
This is a mathematical difference of captured values, not a new executed subtraction or a relation that is retargeted when an endpoint binding is later assigned.
[ENT-3.S7]
- S7 (operation facts).
S7 is the source of facts about the value an integer operation row produces; an integer `cvt` keeps [ENT-3.S5]'s equality instead.
At an `ordinary_let_rhs` binding and at a [SET-1] commit value whose right-hand side is one row of the table below with integer operands, after that row's own obligations have succeeded, S7 establishes on the bound value r the row's result bounds and, for each operand that is an admitted [ENT-2] term x other than a literal or named-const value, the row's relations to x.
Each operand has one interval, read in the closed state where the right-hand side is evaluated, which for a commit precedes the target kill: a typed literal or integer-typed named const has its value; any other admitted term has the least and greatest values its strongest closed L0 bounds through Z permit; every other operand has its type's interval.
At a contradictory point S7 establishes nothing, every relation being derivable there already.
Below, T is the operation's selected type with width K and interval [m, M]; a in [a0, a1] and b in [b0, b1] are the first and second operands' intervals; for a shift, b is the amount and s is [b0, b1] when b1 <= K - 1 and [0, K - 1] otherwise; and bitlen(x) is the number of binary digits of x >= 0, with bitlen(0) = 0.
The corner hull of an expression is the least and greatest of its values at every combination of operand-interval endpoints; where a row names a split, the operand interval is first divided into its negative part and its nonnegative part, a divisor's nonnegative part starting at one, and the hull is taken over the nonempty parts.
Every table value is computed over mathematical integers with checked i128 arithmetic.
The result bounds are the row's exact value when every operand interval is one value, and otherwise the table's result bounds when the row's condition holds and each of its computations is representable; the relations are established when the row's condition holds and each of its computations is representable.
Result bounds are established as `lo <= r` and `r <= hi` through Z, each limited to the interval of the row's result type; an order relation is the stated difference bound; an offset relation is the pair of difference bounds placing `r - x` in the stated interval.
These facts have the ordinary [ENT-5] support of their terms.

| Row | Condition | Result bounds | Relations |
|---|---|---|---|
| `+` | always | corner hull of a + b | offset: r - a in [b0, b1], r - b in [a0, a1] |
| `+wrap` | the corner hull of a + b lies in [m, M] | that hull | as `+` |
| `-` | always | corner hull of a - b | offset: r - a in [-b1, -b0] |
| `-wrap` | the corner hull of a - b lies in [m, M] | that hull | as `-` |
| `*` | always | corner hull of a * b; when [ENT-6]'s interval-product rule discharged the domain, over the operand intervals that rule selected | none |
| `*wrap` | the corner hull of a * b lies in [m, M] | that hull | none |
| `+sat`, `-sat`, `*sat` | always | the corner hull of the unsaturated result, each end clamped to [m, M] | when a0 >= 0 and b0 >= 0: for `+sat` a <= r and b <= r; for `-sat` r <= a |
| `/` | always | corner hull of the truncating quotient, b split | when a0 >= 0 and b0 >= 0: r <= a |
| `%` | always | [min(0, max(a0, 1 - D)), max(0, min(a1, D - 1))] with D = max(abs(b0), abs(b1)) | when a0 >= 0 and b0 >= 0: r <= a and r < b |
| `ineg` | always | corner hull of -a | none |
| `ineg.wrap` | a0 > m | corner hull of -a | none |
| `iabs` | always | corner hull of abs(a), a split | a <= r |
| `iabs.wrap` | a0 > m | corner hull of abs(a), a split | a <= r |
| `inot` | always | corner hull of M + m - a | none |
| `iand` | a0 >= 0 or b0 >= 0 | 0 <= r, and r <= x1 for each operand interval [x0, x1] with x0 >= 0 | r <= x for each such operand x |
| `ior` | a0 >= 0 and b0 >= 0 | [max(a0, b0), min(a1 + b1, 2^bitlen(max(a1, b1)) - 1)] | a <= r and b <= r |
| `ixor` | a0 >= 0 and b0 >= 0 | [0, min(a1 + b1, 2^bitlen(max(a1, b1)) - 1)] | none |
| `ishl`, `ishl.wrap` | a0 >= 0 and the corner hull of a * 2^s lies in [m, M] | that hull | none |
| `ishr`, `ishr.wrap` | always | corner hull of floor(a / 2^s) | when a0 >= 0: r <= a |
| `imin` | always | [min(a0, b0), min(a1, b1)] | r <= a and r <= b |
| `imax` | always | [max(a0, b0), max(a1, b1)] | a <= r and b <= r |
| `imulhi` | always | corner hull of floor(a * b / 2^K) | when a0 >= 0 and b0 >= 0: r <= a and r <= b |
| `ipopcount` | a0 >= 0 | [1 when a0 >= 1 and 0 otherwise, bitlen(a1)] | none |
| `ipopcount` | a0 < 0 | [0, K] | none |
| `iclz` | a0 >= 0 | [K - bitlen(a1), K - bitlen(a0)] | none |
| `iclz` | a0 < 0 | [0, K] | none |
| `ictz` | a0 >= 1 | [0, bitlen(a1) - 1] | none |
| `ictz` | a0 <= 0 | [0, K] | none |
| `irotl`, `irotr`, `ibswap` | never | none | none |
| `reinterpret` between integer types | [a0, a1] lies in the destination's interval | [a0, a1] | offset: r - a in [0, 0] |
| `reinterpret` between integer types | a1 < 0 and the destination is unsigned | [a0 + 2^K, a1 + 2^K] | none |
| `reinterpret` between integer types | the destination is signed and a0 is greater than its maximum | [a0 - 2^K, a1 - 2^K] | none |

Each corner hull above is exact for its expression, which is monotone in each operand while the others are fixed, so its extremes over an interval box lie at corners; each bit row's bounds hold for nonnegative operands, whose two's-complement bits are their binary digits.
For an unsigned exact division `q = a / d` established as above, when a and d are admitted [ENT-2] terms or constants, S7 also captures the exact immutable value images of q, a, and d. When d is a positive written integer literal k, it additionally retains the separate affine value image `k*q <= a`.
At a later direct ordinary exact-multiplication binding, after its own IntegerDomain obligation succeeds over affine operand images, compare those two operand images with the captured q and d images in operand order and then reversed order. Exact identity with one such division establishes `product <= a` over the bound product image and the captured dividend image. Among matching divisions use source-establishment order; one matching division suffices. An unavailable operand image establishes nothing. This finite value-image comparison neither expands a nonlinear proposition nor proves the multiplication's own domain.
The quotient's S7 facts are ordinary L0 facts; the literal scaled relation and matched product relation are specification-fixed members of [ENT-6]'s automatic affine-premise list and are not copied into L0.
Replacing q, a, or d creates a new value image and cannot retarget a captured relation to the replacement; a still-live alias of an old value may continue to use the old relation under [ENT-5].
A signed division, a nonterm operand, a conditional success payload, and every other division form capture no image.
[ENT-3.S9]
- S9 (const-array element ranges).
For `let x = c[i];` where c is the bare IDENT of a named const of type `Array<T, N>` [CONST-2] and T a fragment type, with vlo and vhi the minimum and maximum of its N declared element values, vlo <= x and x <= vhi are established at the binding.
The index's own bounds obligation [ENT-6] is judged separately and is unaffected.
Deeper const shapes establish nothing in this version.
[ENT-3.S11]
- S11 (counted-range structural facts).
In a `for_stmt` preheader, immediately after the lower and upper endpoint values have been captured once in [FN-1]'s order, establish `lower_capture = lower_endpoint` and `upper_capture = upper_endpoint`, reading each admitted endpoint as its exact term or constant, and establish `binder = lower_capture` at the compiler-owned initialization.
Close that post-capture state under [ENT-4] before [ENT-5] forms the counted head state.
On every true header edge that actually enters the body, establish `lower_capture <= binder` and `binder < upper_capture`; the first follows from initialization plus the exact representable compiler updates, and the second from the header comparison just executed.
The capture-to-endpoint equalities are established once in the preheader only: no later header or body entry rereads an endpoint or reasserts a capture equal to the current value of a mutable endpoint source.
The false header edge, every `break` edge, and the counted continuation establish no S11 fact and in particular no raw `binder = upper_capture` postcondition.
Before the binder and captures leave scope, [INV-1]'s separately proved exact-exhaustion rule may use the false guard to form a binder-free affine conclusion; that conclusion is an INV-1 fact, not S11 and not a retained binder equality.
[ENT-3.S12]
- S12 (ordinary declared normal results).
S12 has one owning `CallResultPublication(c,q)` judgment in the ordinary semantic flow.
That judgment succeeds only when q belongs to a declaration relation available under FN-9, either supplied by PRE-1 or PRE-2 or atomically published by a strictly earlier call-graph component; every actual-expression obligation and instantiated FN-8 requirement of c is discharged in the caller before transfer; every referenced formal has its exact pre-transfer substitution; ordinary consumes, borrow commits, projected effects, writes, target commits, and kills have run in the fixed order below; and every support of the substituted result relation remains live.
The candidate relation and all of those parent derivations remain private until every source-semantic judgment in the compilation unit succeeds, then enter the checked program in the same failure-atomic publication as their call and function.
Failure of any premise or any later source-semantic judgment discards the candidate and the complete prospective checked program.
This is the original construction of S12, not a second provenance pass or a check of compiler-generated data.
For one call c and verified relation q, use exactly FN-9's `A0(c)` and per-relation `M(c,q)`.
Candidate scratch establishes q once in the current ProofContext exactly as [FN-9] fixes, after ordinary transfer and every applicable consume, borrow, callee-effect, and target kill.
Each substituted formal is independent: a referenced actual that has no ENT-2 image makes only that q unavailable, while an unreferenced non-ENT-2 actual has no effect on q.
Occurrence-local call-argument evaluated-value datums never enter q.
For relations that name a result, the result destinations are the fresh direct ordinary-let binding, the direct-set target place — including FN-9's narrow direct-set receiver — each binder of a destructuring `let`, and the private success-payload parameter of an admitted single-Result call's conditional context [FN-9, CALL-4, ENT-5].
The destructuring destination takes result ordinal i at binder i and exists only for a declaration that writes an ordered result list [GRAM-2, GRAM-4]. An unrouted result-free relation over the exit state of a written reference parameter needs no result destination: it establishes once on the call's normal continuation at its resolved exit places, after the same kills, including for an expression statement and a unit call.
A false matching predicate, killed support or rejected call establishes nothing. Conditional evidence moves and is selected only under ENT-5; excluded aggregate and indexed storage adds no transport.
The complete candidate set stays unchanged in failure-atomic scratch until the owning source judgment succeeds; any failure publishes none, and success commits all of them atomically.

[ENT-3.S13]
- S13 (call datums).
At an ordinary source call whose callee has an atomically published summary, each `own` operand and each explicitly entry-qualified measure of a written reference parameter of each declared relation of the resolved callee mints one call datum [MSR-3] and establishes it equal to that operand's exact pre-transfer term, at the pre-transfer point of [ENT-5]'s call-boundary order and before that boundary's consumes, borrow commits, callee-effect kills, and target kills.
The population of this source is every callee whose declared relation list is published data: an ordinary function with its FN-9-verified, PRE-1-supplied or PRE-2-supplied contract. A PRE-1 or PRE-2 signature is declaration data and requires no source-body earlier-component verification premise; it gains no additional result-fact source.
The same datum formation applies to source summaries and [PRE-1] and [PRE-2] declarations. The exit measure of a written reference parameter is never a call datum: it is the ordinary live term after the call's exact projected effects and the statement's own kills.
The operand's pre-transfer term is the one [FN-9]'s `A0(c)` substitution already fixes; the datum adds no term the substitution could not name and no relation the callee did not declare.
A datum has empty support, so [ENT-5]'s pre-kill closure carries its consequences across the same statement's kills while every fact whose support those kills remove dies normally.
An operand the substitution leaves without an [ENT-2] term mints no datum, exactly as it makes only that relation unavailable under `M(c,q)`.
S13 is the one source by which a declared relation's substitution is computed at a point other than the point at which the relation is established, and [CALL-6] states both points.

[ENT-3.S15]
- S15 (variant refinement facts).
At entry to an `arm` of a `match_stmt` or `value_match` whose scrutinee is a tracked place, the fact that that place holds the arm's declared variant is established.
It is the fact a payload path step depends on [REF-1] and the fact [MSR-3]'s PAYLOAD placement stands on.
Its support is that scrutinee place's own storage, so it dies on any [ENT-5] event whose written place overlaps it and on the arm's exit edges by the ordinary join.
This loss forbids a new payload selection; it does not itself destroy a payload place already captured by a reference [REF-2].
It establishes no L0 relation and no signed goal; it is an ownership-side refinement consumed by [REF-1], [REF-2] and [OWN-7].

[CALL-6] Publication: how a declared relation becomes a fact, where it is computed, where it is established, and that the set it belongs to is consistent.
Every published relation in this document is published by exactly one route — [ENT-3.S12]'s, with [ENT-3.S13]'s substitution — and nothing else publishes anything.
This rule states that route's four points once, so no rule computes a fact at one program point and uses it at another without naming both.

A declared relation is **instantiated at the call**, by substituting each operand at the denotation [MSR-3]'s table gives it: an `own` measure or an explicitly entry-qualified measure of a written reference parameter by that call's pre-transfer datum [ENT-3.S13], a measure at a reference parameter the row only reads by its live resolved referent, a bare measure at a written reference parameter by the actual's resolved exit place, and a referenced result binder by its destination below. Entry and exit terms are distinct even when they name one formal.
Its **support** is the ordinary L0 support of the substituted terms. The immutable call datums have empty support; an exit term at a written reference parameter has the support of the resolved place after the call's projected write kills. Those writes kill pre-call facts, not the exit relation that the verified callee establishes afterwards. A later target commit or other write to that place kills the exit relation normally.
It is **established** on the call's normal continuation, after the call's ordinary transfer, consumes, borrow commits, target commit and kills, exactly in [ENT-5]'s call-boundary order.
A relation routed to Ok is instantiated at the call in the same order and is **restricted** to that value's conditional success context [ENT-5]. A later success selection activates surviving evidence; it never performs the call substitution again. An intervening event therefore kills the conclusions whose support it removes before they can be selected.
A relation whose support is dead is not available at all; a relation over a call datum has empty support and no event kills it.
A relation naming results uses exactly [ENT-3.S12]'s closed result-destination list [CALL-4]. An unrouted relation naming only the exit state of a written reference parameter is established on the ordinary normal continuation even when no result is bound; its destination is that resolved state, and a unit return adds no result datum.

Every published relation set is checked for consistency at the declaration.
A `contract_block` whose instantiated relations are contradictory at their establishment point is a hard error citing CALL-6 at the `fn_decl`, `ContradictoryPublishedRelations`, naming the clauses and carrying a repair [DIAG-1].
The set is partitioned by route first, because a routed clause is available only on its own arm and two clauses on two arms are never in one caller state together; an unrouted clause selects every explicit return [FN-9] and is therefore a member of every route's set.
Contradiction is the ordinary [ENT-4] question over the declared templates: each distinct operand datum is one term, a literal folds through Z with its value, and the set is contradictory exactly when its transitive closure derives a negative self-bound or forces two terms one declared disequality separates to be equal.
A template whose operand shape that closure cannot represent contributes no premise, so a reported contradiction is always a real one.
The judgment is at the declaration because the set is fixed there: at a contradictory point every L0 relation and both signs of every goal are derivable [ENT-4], so an inconsistent contract is not one wrong fact at a caller but every fact at every caller, and no caller state repairs it.
A contradictory `requires` set is a different thing and stays admissible: it makes the instance legally uninhabited [FN-8], publishes no relation, and no reachable non-contradictory caller can call it.

*Judgment:* the S13 instantiation at the call, the establishment and restriction, the kill from the call, and the consistency check at the declaration.
*Publishes:* the source, the substitution, the instantiation point, the establishment point, the destination list, and the support of every declared relation in the language.

[ENT-4] The L0 component of the closed fact state is the least set containing its established and implicit facts and closed under exactly: (1) from `t1 - t2 <= c1` and `t2 - t3 <= c2`, derive `t1 - t3 <= c1 + c2`; (2) from `t1 - t2 <= 0` and a disequality between t1 and t2 in either orientation, derive `t1 - t2 <= -1`; (3) of two bounds on one ordered pair, the smaller constant subsumes.
L0 derivability is exact: `a - b <= c` is derivable when the closed state contains `a - b <= c'` with c' <= c; `a = b` when both `a - b <= 0` and `b - a <= 0` are derivable; `a != b` when a disequality is present or `a - b <= -1` or `b - a <= -1` is derivable.

The opaque component retains established signed facts and the following finite truth-functional parent reconstruction over exact parent goals already interned in [ENT-2]'s universe.
`+band(A,B)` derives from both `+A` and `+B`; `-band(A,B)` derives from either `-A` or `-B`; `+bor(A,B)` derives from either `+A` or `+B`; `-bor(A,B)` derives from both `-A` and `-B`; and either sign of `bnot(A)` derives from the opposite sign of A.
Literal `True()` has an implicit positive proof and literal `False()` an implicit negative proof.
No `bxor` or Boolean-equivalence introduction is admitted in this version.
The closure considers only already-interned exact parent trees, uses the written rule order and minimum non-cyclic derivation depth, and creates no new formula.
Exact signed-goal identity includes every selected operation-table row, concrete selected operand type, and complete ordered operand GoalExpression.
`+G` is derivable when that exact positive fact is present, when G has an exact comparison projection R and L0 derives R, when G is an operation-domain predicate whose fixed [ENT-6] normalization proves true, or when G's comparison root has an affine normalization and `AUTO` proves it. The affine route is the goal's own comparison normalized, so proving it proves the goal and an L0 projection is what the retained evidence names rather than what the route requires: a goal carrying a coefficient has no two-term projection to name and its retained derivation is the affine consequence alone.
`-G` is derivable when that exact negative fact is present, when G has a comparison projection and L0 derives R's exact negation, or when G is an operation-domain predicate whose fixed normalization proves false.
Integer-domain component relations are only an alternate derivation route into that same exact signed goal; they establish no second source goal and receive no source-obligation identity of their own.
Derivability never decomposes a merely derived parent: [ENT-3] decomposes only the specification-enumerated source establishments.
One retained proof never uses a parent-to-child source derivation and then that child solely to reconstruct the same parent; deterministic minimum-depth selection therefore contains no parent-child-parent cycle.

The combined state is contradictory when L0 derives `t - t <= -1` for any t or when both signs of one exact goal are derivable.
At a contradictory point every L0 relation and both signs of every goal in the finite universe are derivable and every ordinary obligation, call goal, and FN-9 selected-return relation is discharged.
At a non-contradictory query point, an instantiated goal G is `discharged` when `+G` is derivable, `refuted` when `+G` is absent and `-G` is derivable, and `unproved` otherwise.
An instantiated L0 relation R is `discharged` when every normalized conjunct of R is derivable, `refuted` when R is not discharged and R's exact negation is derivable, and `unproved` otherwise.
A one-bound negation is S1's reversed strict bound, an equality relation's negation is its disequality, and a disequality's negation is the equality's two-bound relation.
These three dispositions are complete and exclusive [FN-8, FN-9].
The least closure is unique and finite up to L0 subsumption because only the finite terms and goals [ENT-2] participate and the rules are monotone.
Implementations may compute lazily or incrementally, but every derivability and disposition answer must equal this least-closure answer.

[ENT-5] The support of an L0 fact is every tracked place occurring in its terms; every compiler-owned counted capture term occurring in its terms; for each [ENT-2] clause (b) term, the storage of the readonly field its final step selects — for a measure term over P, P's descriptor storage but not P's element storage [MSR-2] — and the support of every offset occurring in its place; and every reference variable [REF-1] and every `Box` binding [TYPE-7] any of its places reads through, a bound call-result binding included — its resolved place is the candidate actual's complete resolved place, so a `set` commit or projected callee write through the chain kills exactly the facts supported by that storage.
Z, literals, named const values, and every measure datum of [MSR-3] — a call datum, an entry datum, and a placement datum alike — have empty support and never die.
A counted capture is immutable and can die only on an edge leaving its compiler-owned construct scope.

The support of either sign of an opaque goal is the union of the resolved places whose values its complete typed expression reads.
A direct binding goal therefore depends on that binding, while its separately established complete origin expansion depends on the places read by the expansion.
For a measure node over P, support includes P's descriptor storage, every reference variable and every `Box` binding used to reach it, and the support of every offset occurring in P, but not P's element storage, under the same descriptor-storage boundary as an L0 measure term [MSR-2].
A clause (b) place datum's support also includes the support of every offset occurring in its place [ENT-2].
For an index node, support includes its indexable's resolved element storage and the complete support of its offset; it is not a measure node, so any potentially overlapping element write kills the goal.
Literals and named const values add no support.
An evaluated-value datum adds no support: it denotes an already evaluated captured value, is queried only at its one immediate call or operation judgment, never enters an ordinary goal-origin map, join, or loop-carried source fact, and never causes the original expression to be reevaluated.
Every reference variable and every `Box` binding used by a goal's resolved place is also a support member.
The two signs of one goal have identical support.

A requirement or verified postcondition fact has exactly the ordinary L0 or opaque-goal support of its normalized relation after the rule's stated substitutions.
An affine invariant conclusion is different: it is a theorem over the immutable mathematical value-image atoms captured when that invariant occurrence was proved, not a proposition that rereads the mutable source bindings whose spellings formed it.
A write, consume, or scope exit changes or removes the current binding-to-image map but does not make an already proved theorem about the old image false; a live alias may therefore continue to use it, and a named `proof_use` source denotes exactly that immutable theorem while its invariant declaration remains in lexical scope [INV-1, PRF-1].
Without a current value image or another retained theorem connecting an old atom to a submitted target, an unreachable old atom cannot help prove that target.
Header assumptions are removed on every edge leaving their loop, while local invariant conclusions follow [ENT-5]'s canonical control-flow intersection independently of their proof-only names.
The compiler neither removes one constructor and reruns the body nor computes a masked fact state to decide whether any fact was necessary.

An S12 relation, a narrow-receiver relation, and a relation transported through a value initializer have exactly the ordinary L0 support of their terms after the route's stated substitutions.
The callee summary reference, call or delivery edge, pre-transfer substitution record, and a result or payload binder already replaced by its receiver are checked metadata, not additional support.
A route whose substitution leaves a non-[ENT-2] operand never creates an L0 fact.

Independently of relation flow, FN-9 entry-image stability begins live for each referenced parameter datum at function-body entry.
The same overlap, reference, consume, effect, scope-exit, and counted-continuing-kill classifications below permanently invalidate it; for a measure datum the descriptor-storage boundary is the same as for ordinary measure support [MSR-2].
A structural merge retains stability only when every reaching input retains it, and a loop head removes stability for every datum a continuing kill may invalidate.
Neither contradiction, re-establishment of a fact, assignment of an equal value, nor a later iteration restores stability.
This metadata creates no snapshot, term, relation, signed goal, or runtime action.

An L0 fact or opaque signed goal dies at the earliest of: (a) a [SET-1] `set` commit, an [OP-11] `swap` — one read and one write of each of its two targets — or an [OP-12] update — one write of its target — whose resolved target [SET-1] overlaps, under [OWN-7]'s overlap relation, the resolved place of any support member, or the compiler-owned update of a `for_stmt` binder when that binder is a support member — a measure term's support is its place's descriptor storage [MSR-2], so a write to that place or to any prefix of it kills that place's measures, a write at an element position of it kills the measures of the written element and none of its own, and a write to a sibling field kills neither; a [SET-1] `set` commit whose right-hand side has one fragment type evaluates that right-hand side to its commit value before this kill, and after the kill exactly [ENT-3.S5]'s applicable post-write image is established; (b) a call — ordinary function or table operation — one of whose [EFF-2] boundary-projected `writes` occurrences projects onto a caller place or origin set containing a place that overlaps [OWN-7] the resolved place of any support member; the projection is exactly [EFF-2]'s, so a callee writing only through one reference actual kills exactly the facts whose support overlaps that actual's resolved place, and a call whose row carries no `writes` kills nothing; how far such a projected write *reaches* — the actual's descriptor storage, or only a range reference's element storage — is classified by [CALL-1] through [CALL-3] from the callee's declaration and by nothing else [CALL-5], and neither the actual's spelling nor the callee's body is consulted; (c) a consuming use [OWN-1] of any support member's root; (d) an edge leaving the lexical scope of any support binding, or leaving the owning counted construct of any capture term in its support.
Immediately before every specification-ordered batch of kills (a)–(d), materialize the complete [ENT-4] least closure while every pre-kill term and goal remains available; then remove exactly the conclusions whose own support the batch kills. This includes consequences obtained through transitive bounds, implicit type or constant bounds, disequality strengthening, and opaque-goal closure; a partial projection over explicit bound edges is not equivalent. A post-write image or other post-event establishment occurs only after that event's kill as its source rule states.
Scope exits are edge events. After every earlier event and its stated post-event image on that edge, materialize the complete reaching closure, apply scope kills (c) and (d), and close the surviving state before any query or join at the target.
A materialized conclusion survives exactly when its own support survives. Thus an arm-local term may be an intermediate vertex proving a relation among outer values, but no fact or goal whose conclusion still names that local, its holder, or its storage survives the scope into a join.

An ordinary user-call boundary has one order in the current ProofContext.
First, at the pre-transfer point, complete the A0 judgments, retain each referenced formal's exact pre-transfer substitution, and judge the actual obligations and FN-8 goal.
Second, apply argument consumes and borrow commits, the callee's projected effect and write kills, and any route-specific target commit and kill.
Third, and only when `M(c,q)` still holds after those events, establish an eligible S12 relation with its result destination substituted.
A fresh direct ordinary-let result is introduced only after the call kills. A whole Result retains conditional evidence at that point; selecting its Ok payload later delivers it under the conditional transport judgment below.
For the narrow direct-set route, the target kill precedes the result-to-post-write receiver substitution.
No pre-transfer substitution carries an old fact through a kill, no later substitution reverses a kill, and every non-result support must still be live at establishment.

Conditional Result transport: a local own `Result<T,E>` with T one fragment integer has an independent conditional numeric context meaning "if this value is Ok, these L0 relations hold of its payload". It carries one private typed payload parameter and the existing finite L0 vocabulary, with no new source spelling or runtime value. No conditional relation is an ordinary fact before success selection, and contexts of different outcomes are never conjoined. An unknown outcome has an empty conditional context and no known constructor tag.

A successful ordinary single-Result call establishes its admitted routed relations there by FN-9 and CALL-6. Constructing Ok substitutes the private parameter for its evaluated payload term in the closed ordinary L0 facts and establishes their equality; a payload outside the existing term vocabulary contributes no numeric image. Constructing Err gives a contradictory success context and definitely-Err tag information. A direct non-consuming or consuming use of a bare own Result binding copies that value's context and tag information before transfer. A fresh binding, a whole-binding set commit and a give edge install the evaluated value's context at their destination after the operation's ordinary kills. The source association then follows the ordinary copy, consume and replacement rules. Aggregate fields, indexed storage, borrowed Result selections and multi-result calls add no conditional transport in this version; their ordinary value and existing measure-placement semantics remain unchanged.

Evaluating `cvt.checked::<Src, Dst>(x)` with integer Src and Dst creates a conditional context whose private success parameter denotes x's evaluated mathematical integer, captured before any later event. At that private parameter it establishes exactly the L0 bound-value image that [ENT-3.S5], [ENT-3.S6], [ENT-3.S7], and [ENT-3.S9] would establish for an ordinary let of x, together with x's source-type bounds and the private parameter's destination-type bounds. An admitted operand term therefore contributes its equality and closed ordinary L0 relations. An operand outside those rows contributes only those type bounds; an indirect storage read creates no new relation to mutable element storage. An affine current-value image adds no premise beyond that L0 context. The result then follows exactly the conditional transport above, including pre-kill closure, replacement, joins, continuing-backedge kills, success selection and FN-9 forwarded-return checking. A conversion with a float endpoint creates no conditional numeric relation or opaque domain fact; the Result's Ok tag alone therefore establishes no `cvt.defined` goal about the original input.

Evaluating an integer `+checked`, `-checked`, `*checked`, `/checked`, `%checked`, `ineg.checked`, or `iabs.checked` likewise creates a conditional context whose private success parameter denotes the corresponding exact row's mathematical result. Success implies that exact row's domain, so at that private parameter it establishes exactly the [ENT-3.S7] facts the exact row would establish on an ordinary let of its result, reading the operand intervals in the ordinary closed state where the operation is evaluated. Its `Err` outcome carries no relation. The result then follows exactly the conditional transport above.

Before a conditional context crosses an ordinary event or scope exit, include the current ordinary closed L0 facts, close under ENT-4 and apply ENT-5's existing support kills to its conclusions. The private parameter itself has no external support. A write that may overlap the owning Result, its consume or its scope exit removes that holder's association; a previously evaluated copy has its own association. No association is reconstructed from an old call expression. At a loop head, remove associations and external supports changed by any continuing-backedge kill under the existing loop rule. Calls and constructions in the abstract body create evidence for that iteration, without identifying values of separate iterations or unrolling them.

At an ordinary control-flow or value-delivery join, align each reaching value's private payload parameter and join its conditional contexts by ENT-5's weakest-bound and common-disequality judgment. A missing association supplies an empty context; a definitely-Err alternative supplies a contradictory success context. Definitely-Err tag information survives exactly when every contributing incoming value has it. Conditional contradiction remains local and never makes the ordinary continuation contradictory. No conjunction of guards, path-history enumeration or iterative summary inference is performed.

An own match's Ok arm and propagate's successful continuation select the evaluated outcome's context: combine it with the current ordinary L0 facts, close it, substitute the receiving integer binding for the private parameter, and establish the surviving relations as ordinary facts. The Err edge establishes no success relation. FN-9 uses the same context when judging a forwarded return under its success route. Relations retain their verified call, value substitution and join parents in DIAG-2's derivation DAG; none of these events adds a runtime branch, slot, allocation, dependency or scheduling edge.

Bounded relation delivery is an additional edge transfer for the integer carrier admitted by [GIVE-1], in either value initializer.
On one reaching eligible `give d;` edge, evaluate the bare atom's value first.
From the closed state at that point, take exactly each L0 bound or disequality whose normalized terms contain d; facts that do not contain d and opaque signed goals are not delivery candidates.
Replace every occurrence of d with the receiving binding x before applying the give edge's ordinary scope-exit and other event kills to every remaining support.
Thus d's own branch-scope exit cannot delete the already delivered relation, while the death of any other support deletes that relation normally.
Close the surviving substituted relations under [ENT-4] to form that edge's delivery image.
A non-bare, projected, consuming, computed, constructed, call, subscripted, literal, named-const, const-generic, capture, Z, contract-symbolic, wrong-mode, or wrong-type delivery forms no image; the value still follows ordinary GIVE-1 semantics.

At the receiving `let` continuation, ordinary fact flow and its ordinary branch join remain unchanged.
Separately join one delivery image from every reaching `give` edge of the initializer, in edge NodePath order, after the substitutions and kills above.
When at least one image is non-contradictory, contradictory images are neutral and the non-contradictory images retain for each ordered term pair the weakest (largest-constant) bound held by all and each disequality held by all; a relation missing from one such image is not delivered.
Hence images containing `x < 8` and `x < 128` establish `x < 128`, not nothing and not `x < 8`.
An all-contradictory image set is contradictory; an absent eligible relation on a non-contradictory edge contributes an empty image and prevents delivery of that relation.
Add exactly the joined L0 relations to the receiver's ordinary continuation state and close once.
This transport reads no pre-existing fact on x, forms no inverse `x ↦ d`, copies no unrelated relation, and creates no runtime operation.

Joins: at the continuation of a `match_stmt` or `value_match`, the fact state is the join of the states on every arm exit edge reaching that continuation on the conservative structural graph [FN-1], each taken after that edge's pre-exit closure, scope-exit kills, and surviving-state closure above; an arm every path of which leaves by `return`, `break` to an enclosing loop, or `propagate`'s error edge contributes nothing there.
In any nonempty join with at least one non-contradictory input, a contradictory all-derivable input imposes no constraint.
Over the non-contradictory inputs, the L0 join keeps for each ordered term pair the weakest (largest-constant) bound held by all and each disequality held by all; the opaque join keeps one signed fact exactly when that identical goal and sign are held by all.
The join of closed states is closed.
A nonempty join whose every input is contradictory, and an empty join with no reaching edge, are each the contradictory all-derivable state.
At the continuation of an `if_stmt` or `value_if`, this same join is taken over every branch exit edge reaching that continuation — for an else-free `if_stmt`, the false edge is such an edge — each after its pre-exit closure, scope-exit kills, and surviving-state closure; a branch every path of which leaves by `return`, `break` to an enclosing loop, or `propagate`'s error edge contributes nothing there.
The continuation of a `loop_stmt` uses the same join over its `break` edges.
A `loop_stmt` with no `break` resolved to it has an empty join and therefore the contradictory state, consistent with that continuation being unreachable in truth while the conservative graph keeps it reachable.
A `propagate` right-hand side's `Err` edge leaves the function; its normal continuation keeps the preceding state subject to the initializer call's own kill events (b) and (c), and its binder selects the evaluated outcome's conditional success evidence as specified above.
For every join above, contributing arm, branch, `give`, and `break` edges use their source `NodePath` order.

The continuation of a `for_stmt` is the join of its structural false-header edge and every `break` edge resolved to that counted loop, each taken after the applicable pre-exit closure, all binder, capture, and body-scope exit kills, and surviving-state closure.
The false edge always exists in the conservative graph [FN-1], so this join is never empty.
A counted continuation orders that false-header edge first and its contributing `break` edges in source `NodePath` order after it.
A `break` resolved to an enclosing loop, a `return`, or a `propagate` error edge contributes nothing there.
Because the counted binder and both captures are out of scope before the join, no S11 body fact, capture fact, or raw `binder = upper_capture` fact reaches the continuation.
An [INV-1] exact-exhaustion conclusion reaches the continuation only when the identical outer-value conclusion is present on every reaching input of this join; in particular, a `break` edge receives no conclusion from the false header and therefore removes a conclusion not independently true on that edge.

For an ordinary loop L, the conservative head state is the state before L minus every fact having a support member that a continuing kill event of L may kill.
A kill event (a)–(d) placed inside L's body, at any nesting depth, is continuing for L exactly when some path of the conservative structural normal-control graph [FN-1] leads from the edge carrying that event to L's body entry without leaving L's body — that is, exactly when an execution taking that edge can reach a later iteration head of the same loop.
Every other kill event inside the body is not continuing and is not scanned: an event on or reachable only through a `break` edge resolved to L or any enclosing loop, a `return` edge, or a `propagate` error edge leaves L for the loop's continuation or the function-return sink [FN-1, ERR-3], and no iteration head of L is reached from it without first re-entering L from outside, where the enclosing flow supplies the state.
A kill inside a nested ordinary or counted loop whose continuation lies inside L's body is continuing for L, including the kills carried on that nested loop's own `break` edges, because L's body entry is reached from that nested loop's continuation without leaving L.
Without a parenthesized invariant header, exactly those surviving facts hold at every iteration head; establishment and kills then proceed ordinarily within the iteration, and no fact established inside an iteration survives to the next iteration's head.
With a header, [INV-1] first proves every header invariant simultaneously in the complete state before L without assuming any invariant from that header.
After that base batch succeeds, the complete header batch is added to the conservative head state as the assumptions for an arbitrary iteration.
At every reachable normal body fallthrough, after ordinary statement effects, closures, and body-scope cleanup, [INV-1] proves the complete header batch again over the current value images while assuming the current-iteration header batch.
Only the proved header batch, not an arbitrary body-established fact, is reintroduced at the next ordinary-loop head.
If no normal fallthrough reaches the backedge, the preservation batch is vacuous.
A fact a non-continuing edge kills is still removed on that edge: the continuation join above takes each `break` edge after that edge's scope-exit kills, and an edge to the function-return sink reaches no queried program point, so narrowing this scan opens no path on which a dead fact is read.

A counted `for_stmt` uses one compiler-owned structural binder recurrence.
An [INV-1] header invariant changes no runtime edge or recurrence: the writer supplies the proposition, while the checker proves its base and arbitrary-backedge obligations against this fixed recurrence rather than inventing an induction hypothesis.
First its preheader establishes the S11 capture equalities and binder initialization and closes that complete post-capture state under [ENT-4].
Second, [INV-1] proves the complete header batch simultaneously in that closed post-capture preheader state, without assuming any member of the batch.
An event in the body, including the hidden normal-fallthrough binder update and body-scope cleanup, is continuing exactly when some path of the conservative structural normal-control graph [FN-1] leads from its edge through the counted header to a later entry of that same body without leaving the counted body; an event on or reachable only through a `break` resolved to that counted loop or an enclosing loop, a `return`, or a `propagate` error edge is not continuing.
Kills inside a nested ordinary or counted loop are classified by that same positive reachability predicate.
Third, its conservative head state is the closed post-capture state minus every fact having a support member that a continuing kill event may kill, and the complete proved header batch is then activated there.
On each true header edge, S11 adds the two structural body-entry bounds to that state.
The hidden binder update kills every fact supported by the binder before a later header, while S11 re-establishes only its two stated bounds after the next true guard.
At every reachable normal body fallthrough, [INV-1] proves the complete next-header batch after ordinary body effects and cleanup and after substituting the compiler-owned `binder + 1` image for the binder; the complete current header batch is available as an assumption, and no target may assume its own next-header conclusion.
This order is fixed: preheader establishment and closure, simultaneous base proof, continuing-kill subtraction, header-batch activation, S11 body-entry establishment, body flow including body-scope cleanup, formation of the compiler-owned `binder + 1` image and proof of the hidden update's representability, then simultaneous next-header proof over that image.
Neither endpoint is evaluated again and neither capture-to-endpoint equality is re-established after the preheader.
Therefore a continuing write to a mutable endpoint source kills the direct capture-to-source equality, while a consequence already closed in the preheader whose support contains only immutable captures and other still-live terms may soundly survive.
No other fact established inside one counted iteration survives to a later counted head; a body `invariant_stmt` may nevertheless serve as an ordinary proved premise for the next-header batch at the backedge where it is live.

[ENT-6] Every proof-required partial operation and callable contract creates one typed Goal at the source node that would otherwise perform it.
A Goal is normalized by its owning rule from that node's exact operands, types, layout, target, ownership state, and effects.
The checker submits the Goal with the current [ENT-3] ProofContext to one fixed deterministic domain checker; a consumer never selects a checker, retries another route, or treats a diagnostic derivation as a second acceptance pass.
A Goal that no fixed domain discharges is rejected by its owning source rule before lowering.
No runtime guard, trap, fallback operation, optimizer assumption, timeout result, or writer-stated conclusion can replace discharge.

The affine part of one ProofContext contains a current-value image for every live own-mode integer binding that this rule can represent.
An image is one exact mathematical form `c + Σ ai*xi`, where c and every ai are checked i128 integers and each xi is one compiler-owned immutable value atom carrying its exact source integer type interval.
This map is not a second source fact database: it records what value a binding currently denotes so that every consumer normalizes its one submitted proposition over the same atoms.

Image formation is exactly the following structural transfer.
An own integer parameter and any integer result whose listed form below is unavailable receive one fresh atom with that type's complete interval.
A typed integer literal or named integer const has its mathematical constant image; reading or ordinarily copying a live own integer binding reads its current image; and an integer-to-integer `cvt` keeps the operand image after its ConversionDomain obligation succeeds.
A successful measure observation [MSR-1] reads its current measure image, including a standing constant or the captured range image where those rules fix it. Binding or copying that integer preserves the observed value's image; a later kill of the measured place does not retarget the copied value.
After its ordinary IntegerDomain obligation has succeeded, an exact integer addition or subtraction has the sum or difference of its operand images, and an exact integer multiplication has the scaled image when either complete operand image is a mathematical constant; every other integer-producing operation receives a fresh atom.
An expression that may write or consume a place before producing its result receives a fresh result atom rather than an image reconstructed across that effect.
A range reference's `len` image from its formation [REF-4, ENT-3.S6] is the difference of the captured endpoint images [MSR-1].

An ordinary `let` installs the initializer image at its new binding after the initializer's effects.
A whole-binding `set` first forms the right-hand-side image from the entering values, performs the ordinary target kill, then makes the target denote that image; a projected or indexed set does not replace the root binding's scalar image.
A consume or scope exit removes the affected binding-to-image entry but does not alter an immutable theorem over the former atoms.

At a control-flow join, a binding keeps an identical image held on every non-contradictory input.
Otherwise every input image is first normalized: each delta atom an earlier join minted is folded back into the constant interval it stands for — that atom's coefficient times its interval, added to the input's constant — leaving one non-delta nonconstant form and one closed constant interval.
If every normalized input then has one identical non-delta nonconstant form, the joined image is that common form plus one fresh delta atom whose interval is exactly the minimum through maximum of the inputs' constant intervals; otherwise the binding receives one fresh full-type atom.
An input carrying no delta atom normalizes to its own nonconstant form and the closed interval of its own constant.
A delta atom is an ordinary shared atom everywhere except a join, so a relation formed over it after one join still holds at the next; folding at the join is what makes the joined image the same whether the writer spells one branch set as nested conditionals or as one flat `match`, so acceptance never depends on the shape of the join.
The join never equates distinct atoms merely because two source expressions have the same spelling.
A loop's continuing-kill construction similarly replaces every loop-carried mutable binding by a fresh header atom; proved header invariants are the only source-written relations reintroduced over those header images.
The counted binder uses the captured lower image for its base, one fresh header image for an arbitrary iteration, and the exact `header_image + 1` form for a reachable next-header obligation.

All of these transfers are source-structural, checked to the same affine formation ceilings, and independent of proof success order.
They create no independently selectable premise except the invariant conclusions and specification-fixed automatic images expressly listed below.

Every live measure term carries one compiler-owned immutable affine atom as its image, over the complete `u64` interval, minted once per measure term and never retargeted [MSR-4].
That atom is not a source binding and is not a written `affine_factor` [INV-1]; it exists so that the automatic derivation below can range over measure terms, and it dies exactly when its measure term's support dies [MSR-2].
The closed L0-to-affine index is formed on demand from exactly Z, each live measure term, and each live own integer binding having both its ordinary [ENT-2] place term and a current affine value image.
For every ordered pair of those candidates whose closed L0 state has a tightest bound `left_term - right_term <= c`, substitute the candidates' current affine images to form `left_image - right_image <= c`.
For one canonical affine coefficient vector retain only the smallest upper bound; a single image whose coefficients or bound are unrepresentable in i128 is skipped and cannot suppress another image.
These retained inequalities are the `strongest canonical L0 images` below.
They are an ephemeral goal-query index over already-closed L0, not copies published into the automatic affine-premise list.

For a normalized affine inequality A, `DIRECT(A)` is exactly the following nonrecursive check, in this order: a contradictory current combined state under [ENT-4]; the strongest canonical L0 image having exactly A's canonical coefficient vector and an upper bound no greater than A's; or fixed interval substitution of every remaining atom, using its lower endpoint for a negative coefficient and upper endpoint for a positive coefficient, where each endpoint is the strongest closed L0/type bound for that atom.
`DIRECT` never selects or subtracts a published affine premise.
Every invariant conclusion and specification-fixed automatic image is appended when established to one automatic affine-premise sequence; its source category is diagnostic evidence and never partitions proof authority.
At a join, an inequality survives exactly when the canonically identical inequality is present on every non-contradictory input under [ENT-5]'s all-predecessor rule; contradictory inputs are neutral, and if every input is contradictory the affine sequence is empty because L0 already proves every target.
The surviving sequence is ordered by the first occurrence of each canonical inequality in the first non-contradictory structural predecessor under the edge orders fixed above.
For each surviving inequality and each non-contradictory predecessor, the retained representative is that predecessor's occurrence with the fewest active-loop dependencies, ties retaining insertion order; the joined dependency set is the sorted union of those representatives' dependency sets.
This preference prevents an earlier loop-local duplicate from hiding a later loop-independent proof of the same theorem; source and derivation evidence otherwise selects diagnostic parents only.
At every query, canonically identical inequalities are represented once at their first occurrence in this sequence.
Ordinary L0 relations are not copied into that list.

`AUTO(T)` for one affine target T exhausts exactly these finite families: `DIRECT(T)`; for every listed premise P, form `S = P` and check `DIRECT(T - S)`; for every unordered listed pair P,Q including P equal to Q, form `S = P + Q` in pair order and check `DIRECT(T - S)`; and for every strongest canonical L0 image R, form `S = R` and check `DIRECT(T - S)`.
Every premise has coefficient one; forming S and then the one residual uses checked `i128` arithmetic in the stated order.
An unrepresentable candidate is skipped, not accepted and not allowed to suppress a later candidate.
Every accumulated S carrying at least one term is also offered in its integer tightening: because every atom denotes a mathematical integer, S divided by a positive integer factor k dividing each of its coefficients proves that divided left-hand side against the mathematical floor of S's bound divided by k, taken toward negative infinity, and `DIRECT(T - S/k)` is checked immediately after `DIRECT(T - S)`.
Exactly two factors are formed, in this order: the k for which S's coefficient vector is exactly k times T's, and the greatest common divisor of S's coefficient magnitudes; each is read from those two vectors in checked `i128` arithmetic, and a factor of one, a factor not dividing S exactly, and an unrepresentable division each add no candidate.
These complete zero-, one-, unordered-two-, and final L0-image families, each with its two integer tightenings, define AUTO's semantic candidate set.
Within one family, premises use the traversal above, unordered pairs use lexicographic `(first, second)` order with `first <= second`, and strongest canonical L0 images use Z followed by live own integer bindings in their compiler-owned source-allocation order for each ordered `(left, right)` pair, retaining the first occurrence of a coefficient vector unless a later image has a strictly smaller upper bound.
An unproved result exhausts the whole set; a proved result may stop at the first witness in this fixed traversal because later candidates cannot revoke it, so traversal order selects only retained diagnostic parents and cannot change acceptance.
`AUTO` does not recurse, saturate newly derived residuals, search for a multiplier outside the two integer-tightening factors fixed above, choose a subset larger than two published affine premises, or publish an intermediate result.
Consequently an author can determine from this rule alone whether a target is automatic: a derivation outside these exact shapes requires the explicit [PRF-1] `proof_use` list rather than compiler probing.

An [FN-8] Signed Goal query first applies the ordinary positive and negative [ENT-4] disposition to its complete root.
When neither sign is ordinarily derivable, its one remaining positive-proof route recursively follows exactly [ENT-4]'s fixed Boolean introduction table over the already-written goal tree: positive `band` and negative `bor` require every child in source order; negative `band` and positive `bor` visit every child in source order and retain the first successful witness; and `bnot` checks its sole child under the opposite sign.
`bxor` has no introduction route.
At each visited child, when the child root is `<=`, `<`, `>=`, or `>` over values having current affine images or measure terms, the checker normalizes that exact truth sign to one affine inequality; the child is then submitted to [MSR-4]'s disposition in the same ProofContext, which takes the ordinary [ENT-4] proof first and the normalized inequality at its affine steps.
Successful children are joined only by the stated Boolean introduction node; they publish no child, parent, L0, or affine fact, and an unsuccessful candidate changes no later candidate or acceptance result.
This structural traversal invents no proposition, connective, rewrite, premise, or coefficient and is part of the single Signed Goal query rather than an [FN-8] retry or fallback.

[MSR-4] One numeric goal disposition, shared by every consumer.
This rule states once the complete ordered derivation of a numeric goal, and every consumer submits a goal and receives that disposition:

```text
1  contradiction in the current combined state                                  [ENT-4]
2  the exact signed fact, when the goal has one                                 [ENT-4]
3  the closed L0 state                                                          [ENT-4]
4  DIRECT over the affine domain                                                [ENT-6]
5  AUTO over the affine domain, exactly the zero-, one-, unordered-pair
     and final-L0-image families with their two integer tightenings             [ENT-6]
6  the affine-left / L0-right bridge, for every live measure term, every
     measure datum, and every live own integer binding having a current image   [ENT-6]
```

Step 2 applies exactly when the submitted goal has an exact signed identity in [ENT-2]'s finite universe; a normalized affine relation with no opaque goal skips it.
Step 6 visits its candidates in compiler-owned source-allocation order, measure terms before own integer bindings; when closed L0 has the tightest bound `m - r <= c` relating a candidate m to the goal's right-hand term r, it submits the one exact residual target to `AUTO` and composes a success transitively with that L0 bridge.
An unavailable image or an unrepresentable candidate is skipped without suppressing a later candidate; a goal no step discharges is unproved and is rejected by its owning rule.

The consumers are exactly [OP-4] subscript bounds, [OP-2] integer domain, [OP-6] conversion domain, [OP-9] allocation size, [FN-8] requirements, [FN-9] normal-result relations, and [INV-1] invariant targets.
Each keeps its own normalization — which proposition it forms from its source node — and none keeps a route grant of its own: an operation adds a goal, never a route.
A rejection for a goal that no step discharges names its disposition, `refuted` or `unproved` [ENT-4], and carries a repair [DIAG-1]; an [INV-1] target that no step discharges is refuted when this disposition derives the negation of one of its bounds, and unproved otherwise.
Each family paragraph below states its normalization and then submits.

A derivation outside these exact automatic families requires the explicit [PRF-1] `proof_use` list; this rule admits no additional automatic candidates.
Step 1 is the disposition's own hazard and is stated first because it is real: in this language an inconsistent published relation is not a wrong fact, it is every fact, which is why [CALL-6] carries a consistency check at the declaration that publishes one.

The numeric relation domain attaches exactly the following normalized families in this version.
For every source subscript `P[i]` — read, write, and [SET-1] target position alike — SubscriptBounds is `i < P.len`, normalized `i - P.len <= -1`, at that subscript's `psuffix` node.
There is one obligation per subscript in a chain.
The offset has exact type `own u64` [OP-4], so the relation is over the two u64 mathematical values, and it is a logical offset [MSR-1].
A subscript has no separate opaque signed-goal identity for its own bounds obligation; after that obligation succeeds, its selected structural index row may occur as a value child of another exact Goal as [ENT-2] fixes.
For an `Array<T, N>` whose selected N is a concrete value in this instance, the normalization also offers `i <= N - 1` composed with the implicit L0 equality `N = P.len`, which is a second proposition for the same obligation and not a second route.
The normalized target is then submitted to [MSR-4]'s disposition.
A refuted or unproved occurrence is an OP-4 rejection and publishes no checked program.

IntegerDomain attaches one obligation to every proof-required exact integer occurrence [OP-2] at its `infix` or `call` node.
Its canonical goal is always the corresponding total `.defined` operation with the same selected concrete type and complete ordered operand identities.
Each operand uses its stable value expression when available after all nested obligations have succeeded, otherwise that obligation's occurrence-local evaluated-value datum [ENT-2].
Its normalizations are the finite L0 normalization, the affine normalization, and the nonconstant-product rule below; each yields one proposition, and each is submitted to [MSR-4]'s disposition in that written order, the goal itself supplying step 2's exact signed identity.
One derivation root aggregates a successful route's parents in the fixed component order below; components are internal derivation nodes, not separate obligations.

The finite L0 normalization for exact add, subtract, and multiply applies when at least one operand is a constant and is the following two-bound proof over mathematical integers, upper component before lower component.
For `t + c` and `c + t`: `t - Z <= max(T) - c`, then `Z - t <= c - min(T)`.
For `t - c`: `t - Z <= max(T) + c`, then `Z - t <= -min(T) - c`.
For `c - t`: `t - Z <= c - min(T)`, then `Z - t <= max(T) - c`.
For `t * c` with c > 0: `t - Z <= floor(max(T)/c)`, then `Z - t <= -ceil(min(T)/c)`; with c = 0 both components are `Z - Z <= 0`; with c < 0: `t - Z <= floor(min(T)/c)`, then `Z - t <= -ceil(max(T)/c)`.
For two constants, normalization is ground true exactly when the mathematical result belongs to T and ground false otherwise.

When already evaluated operands have current affine images, the affine normalization forms the exact mathematical result image for add or subtract, for negate, and for multiply when either complete operand image is a constant.
It submits, in order, `result <= max(T)` and `min(T) <= result` to `AUTO`; both must succeed.
Thus two nonconstant affine add or subtract operands have this automatic route even though they have no finite L0 normalization.

When neither multiplication operand image is constant but both are affine, the only nonlinear automatic rule is the fixed interval product.
For each operand independently, start with its direct closed L0/type interval, then visit each canonical premise once in the AUTO traversal and retain a strictly tighter endpoint when subtracting that one premise followed by fixed interval substitution proves it; the selected lower and upper endpoints are each re-proved by `AUTO`.
Form the four products of the two inclusive endpoint pairs with checked `i128` arithmetic.
The multiplication domain succeeds exactly when all four are in `min(T)..=max(T)`.
The rule publishes no product inequality over the operands and no intermediate premise; the one thing it publishes is the constant interval its own four products bound, established by [ENT-3.S7]'s `*` row on the value the admitted multiplication binds.

The finite L0 normalization for exact division and remainder is `d != Z` together with a second component: ground true for unsigned T, and `(n != min(T)) or (d != -1)` for signed T, testing the dividend witness before the divisor witness.
It is refuted when `d = Z`, or when T is signed and both `n = min(T)` and `d = -1` are derived.
If that finite route is unknown and the operands have affine images, unsigned division or remainder submits `1 <= d` to `AUTO`.
For signed T, take in order each nonzero target `d <= -1`, `1 <= d`, crossed in that order with each overflow-safe target `min(T) + 1 <= n`, `d <= -2`, `0 <= d`; the first pair whose two members both succeed under `AUTO` proves the domain.
For exact negate and absolute, the finite L0 normalization is `x != min(T)`; the affine route uses the two result-range targets above for negate and `min(T) + 1 <= x` for absolute.
For exact shift, the finite L0 normalization is `k < K`, equivalently `k - Z <= K - 1`, where K is the selected value type's bit width; the affine route submits `k <= K - 1` to `AUTO`.
A refuted or unproved IntegerDomain Goal is an OP-2 rejection carrying its canonical `.defined` spelling.
The `.defined` Goal itself is not an invariant target: when an affine route needs writer guidance, a preceding proved invariant establishes the required operand or interval relation, optionally using [PRF-1], and the operation's fixed checker consumes that published relation.

ConversionDomain attaches one obligation to every bare `cvt` occurrence [OP-6] at its `call` node. Its canonical goal is `cvt.defined` with the exact source and destination types and the operand's admitted value-expression identity, or its occurrence-local evaluated-value datum when no such expression exists. The identical normalization applies to a `cvt.defined` root queried by FN-8, including a root visited during its fixed Boolean introduction. In the following order it applies the ordinary contradictory-state and exact signed-goal judgments, then the following finite normalizations:

1. [OP-6]'s whole-type totality judgment proves the domain.
2. A typed literal or scalar named const operand in the goal, after any valid [ENT-3] origin expansion, is decided from its decoded exact value. Integer constants use mathematical integers. A finite float is decoded from its sign, significand and exponent bits as `(-1)^s * m * 2^e`; remove factors of two from nonzero m. It is integral exactly when m is zero or e is nonnegative, and integer range tests compare its exact integer magnitude. Integer-to-float exactness holds exactly when the integer's magnitude after removal of trailing zero bits has at most p significant bits, p=24 for f32 and p=53 for f64. A nonzero binary64 value fits binary32 exactly when its normalized significand has at most 24 bits, its least significant exponent is at least -149, and its highest is at most 127. Zero, infinity, NaN and identity use [OP-6]'s rows. For symbolic endpoints, apply this decision over their finite bound domains with the same parameter correlation as whole-type totality: all true answers prove the domain, all false answers refute it, and mixed answers are unknown. The typed identities `0_T` and `1_T` denote zero and one in each selected numeric type. This is exact decoded-value evaluation, not a host rounded cast or new const-expression form.
3. For integer-to-integer pairs, the operand's current mathematical value is submitted as `x <= max(Dst)` then `min(Dst) <= x` to [MSR-4], using its available L0 term and immutable affine image. Both components must succeed.
4. For integer-to-float pairs, the sufficient interval `x <= 2^p` then `-2^p <= x`, with p as above, is submitted by the same bound judgment. Both components must succeed.

The bound normalizations prove only the positive domain: a failed component yields unknown, establishes no negation and refutes no requirement. An exact decoded constant answer can prove either sign; an established identical negative goal refutes the domain in a consistent state. An established domain goal is not projected back into numeric inequalities. All other conversion goals are unproved: nonconstant float operands gain no automatic integrality, round-trip, divisibility or float-arithmetic rule. Merely computing a domain Bool establishes neither sign; [ENT-3] supplies its ordinary origins and establishments, and [ENT-5] supplies its support and kills. Each successful normalization retains one derivation root with its parents in the stated order and publishes no new premise. A refuted or unproved bare occurrence is the OP-6 rejection; a refuted or unproved ordinary-call requirement is the FN-8 rejection.

The allocation-size family attaches one canonical Goal to each runtime-capacity construction [OP-13] and to `grow` [OP-10], at that `call` node [OP-9].
Its count child uses the same stable-or-occurrence-local identity rule as IntegerDomain, so every allocation-size occurrence has one canonical Goal.
Its normalization is `n <= floor((2^64 - 1) / stride_ceiling(S))` for the selected stored type S, and that proposition, with the Goal itself supplying step 2's exact signed identity, is submitted to [MSR-4]'s disposition; a derived false comparison refutes.
A refuted or unproved occurrence is an OP-9 rejection and creates no allocation or runtime operation.

Initialization, ownership and references, state effects, layout and address formation, selected-target integer domains and parallel permissions keep their own finite proposition and checker domains under their owning numbered rules.
They use the same fail-closed Goal/checker principle; a later domain needing a source numeric conclusion consumes the checked conclusion retained from ProofContext rather than repeating its derivation. They are not encoded as numeric L0 relations merely to make one universal solver, and they do not become a second authority for accepting source propositions.
Each checker has a specification-fixed finite algorithm whose complete work is a deterministic function of its source-derived input, a unique closure or result, a deterministic diagnostic order, and no timeout-selected acceptance.


Each concrete obligation identity is `(concrete function instance, exact source NodePath, family ordinal)`.
SubscriptBounds, IntegerDomain, ConversionDomain, and the allocation-size family use ordinal zero.
A requirement occurrence is `(concrete function instance, requires_clause NodePath)` [DIAG-2].
These identities do not participate in Goal equality [FN-8].
The checked program retains the accepted Goal, its deterministic derivation root, and its erased disposition for diagnostics and proof consumers.
Internal derivation metadata is only the diagnostic explanation of that acceptance decision and establishes nothing independently.
[INV-1] A `header_invariant` and an `invariant_stmt` are two placements of the same proof-only declaration: the writer states that one ordered affine relation holds at that exact source point, and the checker must prove it before the relation gains authority.
A loop-header placement additionally creates induction obligations because control may enter that point from the preheader and from a backedge; a body placement creates only the one ordinary program-point obligation in its entering ProofContext.
The spelling `invariant` therefore describes the writer-visible meaning in both positions, while the control-flow owner determines how many incoming-edge obligations exist.

The `compare_op` of a `header_invariant` or an `invariant_stmt` must be exactly `==`, `<=`, `<`, `>=`, or `>`, and that of a relation-form `use_premise` exactly `<=`, `<`, `>=`, or `>`; it selects a proof-domain relation over its two affine expressions and performs no [OP-1] operation.
`==` in a `use_premise` and `!=` in either position are a hard error at the `compare_op` node, citing INV-1 in a `header_invariant` or an `invariant_stmt` target and PRF-1 in a `use_premise`, because a relation source is owned diagnostically by PRF-1 as that rule states; the restriction itself is one rule stated once, and only its citation follows the owning position.
The checker normalizes `a <= b` to `a-b <= 0`, `a < b` to `a-b <= -1`, `a >= b` to `b-a <= 0`, `a > b` to `b-a <= -1`, and `a == b` to the bound pair `a-b <= 0` and `b-a <= 0`, each proved as one batch member, exactly as [ENT-4] normalizes a source equality.
Disequality and every other Bool root are outside this version's invariant surface.
A `use_premise` admits one inequality, because a certificate adds one normalized premise into one sum [PRF-1].

An `affine_expr` denotes a mathematical integer expression and never a runtime evaluation.
At a counted-loop header an IDENT may resolve to that header's `for_binding` binder or to a live own-mode integer value in the preheader.
At an ordinary-loop header it may resolve only to a live own-mode integer value in the preheader.
At an `invariant_stmt` it may resolve only to a live own-mode integer value in the statement's entering lexical context.
A `call` in `affine_factor` position is a hard error citing INV-1 at the `call` node, carrying the constructor `call` and the domain-query rows as what that position admits in a contract clause [MSR-5].
An `affine_factor` `atom` is admitted exactly when it is one `place` formed from an admitted measure place [MSR-1] by one measure-member `psuffix` [OP-15], one bare `place` whose `pbase` is an IDENT and which carries no `psuffix`, or one integer literal; [GRAM-4]'s production is shared with a contract clause [MSR-5] and carries the wider factor set that clause needs.
A bare `pbase` resolves to a live own-mode integer value as this rule states above, or to an in-scope integer-typed const generic [MSR-6], whose image is the constant [ENT-2] clause (c) already fixes: a concrete instance reads its mathematical value and the one source-canonical symbolic instance reads the symbolic constant term, which no [ENT-5] event kills and whose support is empty.
A measure place's root resolves in exactly that same context, except that it names a live own-mode value of measured type, or a live reference whose referent is reached through `deref` [REF-1, TYPE-7] as section 16's example writes `deref(p).len`, rather than a live own-mode integer, and it is never a counted header's `for_binding`.
Such an atom denotes the [ENT-2] measure term over that place, of fragment type u64, lifted to its mathematical integer value like every other atom, and its support is [MSR-2]'s: an event killing that term retargets the atom's image exactly as a write to a named local retargets that local's, so no conclusion resting on it survives the write.
A subscript inside a measure place is an ordinary [OP-4] occurrence: its offset resolves in that same context and is one of the offsets [ENT-2] clause (b) admits, and it owes `i < base.len` against the prefix reaching its base, judged where the relation is written — at the loop header in its entering ProofContext, at an `invariant_stmt` in that statement's entering one — exactly as one written at a measure read the program executes is judged at the read [MSR-4].
A measure over a place whose subscripts are not all discharged is no term here either, so the relation names a slot the window has or it names nothing.
A const generic is not an integer literal, so it never supplies the one direct literal operand a non-unit `*` requires.
An integer-typed named const is admitted and denotes the one closed value it declares, folded to that value at formation; it is already an [ENT-2] constant term, so it means in a relation exactly what it means everywhere else. An integer-typed const generic is admitted as the paragraph above states [MSR-6], symbolic in the source-canonical instance and its value in a concrete one, and is not the closed-value admission. Construction, allocation, a field selection not ending in a measure member, a subscript outside a measure place, a reference expression, a moved value, and every other runtime expression form are not admitted as affine atoms in this version, and each is a hard error citing INV-1 at that `affine_factor`.
Every literal and local value has its exact closed source integer type and is lifted to its mathematical integer value; mixed widths and signedness create no runtime conversion.
`+` and `-` associate from left to right in source order.
`*` is admitted only when at least one direct operand is an integer literal; two local or parenthesized nonliteral operands are non-affine.
Parentheses alter grouping but never turn a composite expression into a direct literal factor.
Formation and normalization use checked `i128` arithmetic and the fixed structural ceilings of 4096 scheduled expression nodes, 4096 input terms, and 4096 normalized result terms.
Overflow or a structural-ceiling excess rejects at the owning invariant; there is no cumulative work or elapsed-time ceiling.

A `for_stmt` header is the complete parenthesized list fixed by [GRAM-4]: its first and only `for_binding` is followed by zero or more comma-separated `header_invariant` clauses, with no trailing comma.
A `loop_stmt` either has no header parentheses or has one nonempty parenthesized comma-separated list containing only `header_invariant` clauses, with no trailing comma.
A header invariant has no proof block and no `proof_use`; a complex base is stated by a preceding `invariant_stmt`, and a complex backedge is stated by an `invariant_stmt` on the reaching body path where its local premises are live.
All invariant names in one header are distinct and enter scope simultaneously only after the complete header [INV-1].
Their conclusions form one simultaneous batch.

For the base batch, the checker submits every header target to [MSR-4]'s disposition in the complete preheader state and assumes no conclusion from that same header.
If any base target fails, no header conclusion is published.
After all bases succeed, the complete batch is available as the current-iteration assumption throughout the body.
For every reachable normal backedge, the checker proves every next-header target in one batch from the complete state on that edge while the current-iteration header batch is available; a target never assumes its own unproved next-header result.
For an ordinary loop the next-header target is the same written relation over the current backedge value images.
For a counted loop each binder occurrence in the source relation is rendered and proved as the current binder's exact mathematical `+ 1` image, and every other mutable atom uses its current backedge image.
This is the induction step for an arbitrary iteration, not a check of a particular second iteration.
A backedge batch is vacuous when no normal body fallthrough reaches it.
A `break`, `return`, or `propagate` error edge creates no backedge obligation.
Failure reports the invariant name, whether the failed incoming edge is base or backedge, and the complete required source-level relation after the counted next-state substitution; an internal affine term or value-image identifier is never the writer-facing residual.

A reachable `invariant_stmt` is checked exactly once in its entering ProofContext.
Without a proof block its target must succeed under [MSR-4]'s disposition.
With a proof block it is checked by [PRF-1].
It cannot assume its own target.
On success its normalized target and immutable value images become one published affine fact after the statement; the fact may serve every later goal in the declaration's dominance region and may itself be named by a later `proof_use`.
Only that target is published: formation state, certificate premises, scaled premises, accumulator values, and residuals are never added to ProofContext.
At a control-flow join, facts are compared by canonical inequality and immutable value images rather than invariant spelling or proof-source ordinal; identical conclusions reaching every non-contradictory input survive under [ENT-5].
The invariant name keeps only its lexical scope and never changes canonical fact identity.

For a counted loop whose complete header batch succeeds, the fixed exact-exhaustion rule is available only when the captured lower endpoint is proved no greater than the captured upper endpoint without using that header batch and, when a backedge is reachable, the hidden `binder + 1` update is proved representable in u64.
Initialization, every true guard, the exact hidden update, and the false guard then establish `binder = upper_capture` on the structural false-header edge.
The checker substitutes the captured upper value for the binder in each proved header invariant exactly once and exports a result only when every remaining atom is an outer value live at the continuation.
No `break` edge receives this conclusion.
The ordinary continuation join therefore retains it only when the same canonical outer-value fact reaches every input; an ordinary loop has no corresponding exhaustion substitution.
The header invariant name itself does not escape the loop body.

Every invariant form and its certificate are erased before runtime lowering.
They evaluate no expression, read or write no storage, form or consume no reference, move no value, contribute no effect, branch, allocation, call, trap, fallback, or instruction.
A malformed relation or an unproved target cites INV-1 at the smallest specified source node; a certificate-specific failure cites PRF-1 as fixed below.

[PRF-1] A proof block is an optional local linear certificate attached only to an `invariant_stmt`.
It supplies an ordered list of `proof_use` steps that tells the checker which already-provable inequalities to add; it is not allowed on a `header_invariant` clause.

A next-state relation using one current header theorem and then a value-range discharge by `DIRECT` remains inside `AUTO` and therefore has no block:

```wf
invariant next_per_byte: sum <= 255_u32 * (i + 1_u64);
```

The first example below names three listed affine premises, beyond `AUTO`'s complete two-listed-premise family.
The second uses one explicit non-unit factor, which `AUTO` never guesses:

```wf
invariant component_sum: first + second + third <= first_limit + second_limit + third_limit {
  use (first <= first_limit);
  use (second <= second_limit);
  use (third <= third_limit);
}

invariant pair_bound: first + second <= first_limit + second_limit;
invariant scaled_bound: 3_u64 * first + 3_u64 * second <= 3_u64 * first_limit + 3_u64 * second_limit {
  use 3 times pair_bound;
}
```

An IDENT premise of `use_premise` resolves in the invariant-name domain to the exact immutable normalized target published by that dominating invariant declaration; it is not reparsed using the current value bound to each source spelling.
The checked reference stores `(concrete function instance, invariant declaration identity)` obtained from that lexical resolution, never the IDENT spelling or a later spelling lookup.
Inside a loop body a header declaration identity denotes the currently activated arbitrary-iteration header theorem, not its base value images and not the still-unproved next-header target.
A relation-form source in `proof_use` uses INV-1's exact affine formation and normalization rules, including substitution of every referenced local's current value image before canonical normalization. It is owned diagnostically by PRF-1 and must itself be proved by `AUTO`. The `compare_op` restriction is INV-1's one rule, and the position decides which relations it admits: an invariant target admits `==` and a `use_premise` does not.
A named source must be in lexical scope and its published fact must be available in the entering ProofContext.
Every use, named or written, is checked against the same snapshot immediately before the owning `invariant_stmt`.
No use publishes a fact, and no earlier use can help prove a later use.

A `proof_use` cites exactly one premise, its `use_premise`, and states how many times that premise is added into the certificate sum.
A relation premise is always delimited, `use (a <= b);` and `use 3 times (a <= b);`, and a named premise never is [GRAM-4]; the delimiting parentheses are the grammar's own and are not an affine grouping.
The optional `N times` prefix is that multiplicity, and it is written either as a bare decimal or as a name.
A bare-decimal multiplicity is a proof-domain positive integer factor: its canonical spelling is one decimal with no leading zero, its value must be in `2..=i128::MAX`, and omission alone means one.
It is neither a source integer literal nor a runtime type.
Zero, an explicit one, a leading zero, an out-of-range factor, a negative or typed literal, and arithmetic overflow reject.
A named multiplicity denotes the value image its declaration holds in the same entering ProofContext every premise is checked against.
It must be a live own-mode integer value binding — a `let_stmt` local, a `param`, a `for_stmt` binder, or a match binder — or an integer `const`, and its type must be **unsigned**; a signed type, a reference, and a non-integer type each reject. Every admitted type is copy, so a moved binding is [OWN-1]'s hard error before this rule reads it.
That restriction is what makes the scaling step sound without a further obligation: multiplying a normalized `p <= 0` by a value known nonnegative from its written type yields `m*p <= 0`, while a negative multiplier would reverse the premise.
A runtime multiplicity of zero drops its premise and is not a rejection, because no written text asserts that it is nonzero.
No two `proof_use` entries may resolve to the same normalized premise, regardless of multiplicity; their total scaling must be expressed by one multiplicity on one use.
No global or subset-minimality judgment is performed on the remaining list.

The checker forms S independently of premise admission by multiplying each normalized premise by its written multiplicity and adding the results in source order with checked `i128` arithmetic; acceptance additionally requires every source to be independently admitted.
A bare-decimal multiplicity keeps that accumulation affine.
A named multiplicity does not: scaling a normalized premise by a value introduces products of two value images, so the accumulation is a polynomial of degree at most two whose nonlinear monomials exist only while S is being formed.
Before S is used, every such monomial is folded to the one value that already equals it: the value image bound by an admitted exact multiplication [ENT-6] whose two operands are the same two values, where that multiplication's own [OP-2] domain discharged over affine operand images — by the fixed interval-product rule or by an affine clause, either of which fixes the images the judgment read. A domain discharged by the finite L0 route alone records nothing, because that route reads no affine image.
A multiplication's operand and a written multiplicity name the same value when they name the same declaration, not when their images coincide. A local's image is transparent: `let stride = width + padding;` gives `stride` the image `width + padding`, so a product over `stride` and a certificate scaling by `stride` would otherwise reach the fold as different arithmetic. Each such binding therefore contributes one opaque handle, minted at its type range on first demand and killed with the binding's image, and the product record and the multiplicity both name that handle. This is the rule [PRF-1] already applies to a named premise, which resolves to its declaration identity and is never reparsed from the current value of its spelling.
A handle exists only between the fold and the residual. Once every nonlinear monomial has folded, each handle is replaced by the image it stands for, so what reaches the residual is written in exactly the terms every other premise and the target are written in, and no certificate can be discharged by a handle's own defining equation.
When several bindings hold that product, the one the target itself names is chosen, and otherwise the least; they are equal values, so either choice is sound and this one is a canonical form rather than a search.
S is that folded accumulation, and it must be an affine inequality: a certificate that leaves any nonlinear monomial unfolded rejects and proves nothing.
Nothing else in this specification carries a nonlinear term — no fact, no published conclusion, no invariant target, and no `affine_expr` [INV-1] — so the accumulation above is the complete extent of degree two in the language.
Let S be that one accumulated inequality and T the owning invariant target.
The certificate succeeds exactly when `DIRECT(T - S)` succeeds in the same entering ProofContext, or when `DIRECT(T - S/k)` succeeds there for one of the two integer-tightening factors k that [ENT-6] fixes for S and T.
In particular, when S's coefficient vector is exactly k times T's, that tightened residual is constant and the target is admitted whenever the mathematical floor of S's bound divided by k is no greater than T's bound.
This admits a target that is a fixed L0 or interval weakening of the written sum; it does not require byte-for-byte equality and it does not call `AUTO` on the residual.
The checker never guesses a source, multiplier, ordering, subset, case split, or intermediate lemma.

After every use has parsed, resolved, and formed canonically, but before premise admission and combination, the checker applies `AUTO(T)` to the target in the same entering context.
If it succeeds, the proof block is a redundant source form and compilation rejects at the owning `invariant_stmt`, with a repair [DIAG-1].
This is a source-language judgment, not an implementation-dependent warning: [ENT-1] fixes `AUTO` exactly.
The checker does not search whether an individual nonduplicate use could be removed.
At most 4096 `proof_use` entries are admitted by one block; this is a source structural ceiling, not a work or time budget.

Only the owning invariant target is published after a successful certificate.
The `proof_use` list and all of its intermediate arithmetic are erased with the invariant and have no runtime semantics.
An unresolved invariant name is the ordinary INV-1 lexical-scope failure and forms no certificate source.
A resolved but unavailable named source, undischarged or malformed relation source, invalid multiplicity, duplicate source, arithmetic or structural overflow, unfolded nonlinear monomial in S, failed final `DIRECT` residual, or redundant block cites PRF-1 at the smallest owning source node and publishes no target.

## 16. Worked example (normative bytes)

[EX-1] The following complete program is byte-exact canonical form:

```
alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

enum Sign {
  Neg();
  Zero();
  Pos();
}

fn sign_of(x: i32) -> result: Sign pure {
  doc "Conditional value produced by returning from branches (canonical for return position).";
  if x < 0_i32 {
    return Sign::Neg();
  } else if x == 0_i32 {
    return Sign::Zero();
  } else {
    return Sign::Pos();
  }
}

fn fill_eight() -> total: u64 pure {
  doc "A Slots window appended by place_back under a counted header invariant, then measured.";
  let r = slots_new::<u64, 8>();
  for (
    i in 0_u64..8_u64,
    invariant h: r.len == i
  ) {
    place_back(window: &r, value: i);
  }
  let n = r.len;
  return n;
}

fn main() -> status: ExitStatus pure {
  doc "let-initializer match with give and a reference read through deref.";
  let a = 40_i32;
  let p = &a;
  let v = match deref(p) +checked 2_i32 {
    Ok(value: w) => {
      give w;
    }
    Err(error: e) => {
      let failed = exit_status(code: 1_u8);
      return move failed;
    }
  }
  let expected = v == 42_i32;
  if expected {
  } else {
    let failed = exit_status(code: 1_u8);
    return move failed;
  }
  let success = exit_status(code: 0_u8);
  return move success;
}
```

## 17. Language regularity

[META-2] No context-dependent spellings or rule variants: no rule's meaning depends on surrounding context; defaulting rules do not exist.
