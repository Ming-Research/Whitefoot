# Ownership transfer and reference-access forms

This investigation compares the remaining ownership surface after signature
`own` was removed. The first comparison used specification v0.69 at
`0f22b026b`; its source-probe refresh uses v0.74 at `ad51e05df`. The arrow and caret comparisons
use the v0.75 grammar at `126d201d6`; the earlier source measurements retain
their recorded baseline. The concrete consumers
are the maintained HashMap and Deque libraries and the owned-link cursor
program. The active specification remains the language authority. Historical
candidates below retain their baseline spelling. The owner selected postfix `^`;
see [the settled spelling](#selected-postfix-caret) for the grammar and
implementation boundary.

## Requirements and comparison criterion

Record these criteria before trying the alternatives:

- Preserve copy/drop capabilities, exactly-once consumption of noncopy owners,
  whole-owner partial moves, linear-value obligations and derived cleanup.
- Preserve reference-holder identity, rebinding, captured selectors,
  invalidation, overlap and declared effects. A shorter access must select the
  same path and establish the same partial-operation obligations.
- Determine a written use from its syntax, the already known local kinds and
  generic bounds. Do not select copying, moving or borrowing from later uses,
  from the selected concrete generic instance, or from a preferred overload.
- Compare the same operation and result contract, including error and
  replacement outcomes. Do not label an API obsolete merely because it
  consumes an input and produces an output.
- Distinguish lexical shortening from fewer ownership transfers or runtime
  operations. Source counts in these examples do not establish population
  frequency, writer productivity or performance improvement.

The alternatives are discriminated by paired normal/error examples and by
counterexamples: copy and noncopy generic instances, owner use after transfer,
stored linear fields, reference aliases versus referent reads, holder rebinding
versus referent assignment, ancestor replacement, and effect overlap.
An alternative that only shifts the same obligation into another mandatory
declaration has not removed that obligation.

The v0.74 refresh repeats the same source and grammar observations after the
standard-library move. A changed verdict overturns a recorded limitation only
if the same type range, proof contract and observed behavior are retained;
an earlier module/name error is not the ownership or proof observation. No
new timing comparison or writer trial is part of this refresh.

## Questions

1. Do current container interfaces still require avoidable owner-in/owner-out
   mutation? Compare ordinary writes through a reference with a consuming
   rebuild, release and atomic owned transformation.
2. Should ordinary affine expressions, match scrutinees, propagation and
   destructuring share one consumption spelling? Compare the current rules,
   explicit markers at every consuming boundary, and type-directed consumption
   of bare value places. Keep last-use inference a separate alternative.
3. Can reference projections become shorter without implicit value reads or
   confusing holder assignment with referent assignment? Compare the current
   `deref` step, an explicit projection spelling, and automatic reference
   projection. Whole-referent access and forwarding a reference must remain
   distinct observations.

## Current rule boundary

OWN-1 requires `move` for ordinary affine place expressions and rejects it on
copy values. FN-2 checks that spelling against a generic body's bounds once;
an unbounded generic `move` may denote copying at a copy instance. OWN-13 and
ERR-3 independently supply consuming contexts for an own-place match and a
bare affine Result propagation operand. Thus written `move` is neither every
ownership transfer nor an unconditional promise of noncopy behavior.

REF-1 makes a bare reference binding an alias or a forwarded reference.
TYPE-7 requires `deref` to reach its referent. SET-1 distinguishes rebinding a
reference holder from storing through it. `Box.inner` is a separate ordinary
owned-field step. Removing a repeated `deref` spelling must not erase those
distinctions.

## Consumer comparison

| Consumer | Actual ownership boundary | Conclusion |
|---|---|---|
| [HashMap](../../../lib/std/collections/hash_map/module.wfm) | `try_put`, `put`, `remove`, `reserve` and `rehash` take a reference to the map. Offered keys/values arrive by value; replacement or refusal returns a complete pair. | Ordinary mutation already avoids returning the map. Returning displaced or refused elements preserves ownership, including linear elements; dropping those results would change the contract. |
| [Deque](../../../lib/std/collections/deque/module.wfm) | Push/pop mutate through references. `rebase` consumes the old backing and returns a new one; `free_empty` consumes an empty backing. | Rebase deserves a same-contract reference comparison. Freeing and removing an element are genuine consuming operations. |
| [Owned-link cursor](../../../tests/programs/owned_link_cursors.wf) | `without_first` consumes a link and returns its successor; `remove_even` uses it in `set deref(cursor) = without_first(head: move deref(cursor));`. | The owned transformer supplies OP-12's indivisible replacement. Deleting its ownership boundary would require another complete implementation, not just a shorter signature. |

HashMap's private [rebuild helper](../../../lib/std/collections/hash_map/hash-map.wf)
is also a counterexample to the claim that a new allocation necessarily
requires an owned interface: it swaps backing through a reference. Its current
contract does not publish the extent relations that Deque rebase does,
however. Those are different proof contracts.

### Deque reference-rebuild probes

The direct wrapper below is a checked research fragment, not a proposed
library addition. On the current compiler, select the standard-library
declaration with the alias below and compile the fragment with a main:

```wf
alias deque_rebase = std::collections::deque::deque_rebase;

fn deque_rebase_reference<T, const ceiling: u64>(values: &Box<Ring<T>>, capacity: u64) -> result: unit writes(values) contract {
  requires capacity >= deref(values).inner.len;
  requires capacity <= ceiling;
  ensures deref(values).inner.len == deref(entry(values)).inner.len;
  ensures deref(values).inner.cap == capacity;
  ensures deref(values).inner.head == 0_u64;
} {
  set deref(values) = deque_rebase::<T, ceiling>(values: move deref(values), capacity: capacity);
  return unit;
}
```

| Probe | Observed result at both v0.69 and v0.74 baselines | Meaning |
|---|---|---|
| Existing Deque program and existing owned-link cursor | Both compile and return native exit 0. | Controls exercise the current APIs and cursor replacement. |
| Unbounded wrapper above | WIN-3 at the `set` target: linear assignment target. | OP-12 covers copy/affine targets; WIN-3 rejects a linear target. A `drop` bound would exclude supported `nodrop` elements. |
| Same wrapper with `T: drop` | FN-9 at `return unit`: unproved length postcondition. | Passing the ownership check is not sufficient to publish the original contract. The precise classification of this measured-result establishment limit remains open. |
| `T: drop` wrapper with all three `ensures` clauses removed | Compiles to LLVM. | A mechanism control only: weakening the contract is not a successful equivalent replacement. |
| Transfer elements through the reference, then swap in the completed backing and free the old empty backing | OP-14 at the empty-input branch's `free_empty`: the old backing's zero length is unavailable after swap. | PRE-1 gives swap no postcondition and MSR-3 does not transport measures through swap. This is the already recorded descriptor-fact limitation, now affecting linear cleanup. |

The last probe is reproduced from the existing
[Deque implementation](../../../lib/std/collections/deque/deque.wf)'s
`deque_rebase` body: change
the signature and contract to the wrapper's; replace body `values.inner`
with `deref(values).inner`; remove the explanatory `doc`; replace both
`free_empty(window: move values); return move built;` sequences with
`swap(first: values, second: &built); free_empty(window: move built); return unit;`.
Keep the loop, invariants and allocation unchanged. The compiler rejects the
first cleanup before testing the later postconditions; this does not establish
that every remaining obligation would pass after measure transport was added.

The intended caller comparison uses all seven rebase sites in the maintained
[Deque program](../../../tests/programs/containers/deque-program.wf), covering
scalar, boxed, `nodrop` and unit elements. Replace each
`let next = deque_rebase::<...>(values: move old, capacity: cap);` with a call
to the reference helper on `&old`, followed by `let next = move old;` solely
to preserve the existing oracle's names. The unbounded wrapper and swap
implementation fail before that native comparison can run. No runtime or
transfer-cost equivalence is claimed for them.

Recommendation: retain the existing library interfaces in this investigation.
Reopen replacement and measure transport with this exact contract, rather than
introducing a weaker overload or describing `nodrop` exclusion as cleanup.
Extending OP-12 to linear targets is a separate language opportunity: the
callee could consume the old owner and return a replacement without implicit
drop. It needs its own account of failure exits, joins, aliases and effects;
the current prohibition alone proves neither that extension necessary nor
unsound. The swap route also merits comparison once its evidence can cross
the exchange. These related tasks remain grouped in
[the maintained TODO](../../../docs/todo.md).

## Consumption spelling

The current surface gives equivalent consuming forms to a noncopy own-place
match and affine propagation, while ordinary assignments, operands and returns
require `move`:

```text
Current:  consume(value: move owner);  return move owner;
Current:  match outcome { ... }       match move outcome { ... }
Current:  let value = propagate outcome;
Current:  let value = propagate move outcome;
```

OWN-1 supplies the ordinary requirement; OWN-13 and ERR-3 supply the implicit
contexts. FN-2's symbolic-body judgment adds a different qualification:
`return move value;` in `fn pass<T: drop>(value: T) -> back: T` may copy at
a copy instance. The marker is not an unconditional runtime move instruction.

| Alternative | Benefit | Cost |
|---|---|---|
| Current mixed contexts | Fewer markers in match and propagate. | The writer needs context-specific consumption rules; explicit and implicit forms coexist there. |
| Explicit consuming owned places everywhere | One requirement for existing noncopy owners across calls, construction, binding, return, match and propagate. | Adds markers at currently implicit sites; symbolic generic checking still determines copy capability. |
| Bare places copy or consume according to their known kind and bound | Removes `move` and its exceptions without requiring last-use inference. | An ordinary-looking field projection can consume its entire owner and release other fields, even when no later use exposes the change. |
| Infer borrowing or transfer from later uses | May shorten more source. | Later edits can change an earlier boundary; it does not meet the recorded local-decision criterion. |

Type-directed implicit consumption is technically viable in principle, not a
harder proof search. [Rust's place-to-value rules](https://doc.rust-lang.org/reference/expressions.html#place-expressions-and-value-expressions)
provide a concrete comparator: copyability and the place determine copying or
moving without a last-use rule. Whitefoot's whole-owner consumption is an
additional consideration. In its owned-link helper,

```text
let node = move cell.inner;
return move node.next;
```

the first line consumes the Box and releases its cell; the second consumes
the entire node, not just the selected field. In an aggregate with other
affine fields, those fields are released. Without the marker,
`let selected = owner.field;` can look like ordinary field access while
discarding the rest of `owner`. The liveness checker still prevents later
reuse and linear residuals still reject, so this is a source-intent tradeoff,
not a claim that implicit moves would break memory safety.

The recommended proposal is **explicit consuming uses of existing noncopy
owned places**, including `match move outcome` and `propagate move outcome`.
It preserves the event marker while removing the two implicit-context rules.
Copy places remain bare; non-place temporary results need no marker; a match
through a reference remains non-consuming. Generic spelling remains checked
once against the written bound. Destructuring retains its existing consuming
form and capability restrictions. There is no new last-use inference, clone,
automatic borrow, or change to whole-owner cleanup.

This requires retiring bare affine match and propagation acceptance, not merely
encouraging a style. The affected rules are OWN-1, OWN-13 and ERR-3, with
FN-2's generic qualification retained; conformance and examples would migrate
with the implementation. The owner approved this choice on 2026-09-27. It is recorded in
[language/ownership](../../../design/language/ownership.md) as one added
decision and one refused alternative; the dependent result-propagation node's
implicit-consumption decision is retired under the same ruling. The approved rule replaces the
implicit-context choice in OWN-13 and ERR-3. The v0.76 amendment implements it
with the selected reference-access spelling; the preceding measurements remain
observations of their named baselines.

The v0.74 module and variant changes do not remove these implicit consuming
contexts. GRAM-10 now also permits `..` in an arm: an owned match releases
covered affine fields and refuses covered linear fields; a reference match
releases none. That existing cleanup rule remains in force under the proposal.
Public payload access and constructor qualification are separate boundaries;
neither an explicit `move` nor a reference step grants field visibility.

## Reference-place spelling

The path model and its current syntax have different histories. The early
[x1 spelling map](../access-effects/SPEC-AMENDMENT-MAP.md#surface-and-spelling)
suggested direct reference access and using `deref` for Box content. That was
design pseudocode. The later
[Box field ruling](https://github.com/mbbill/Whitefoot/commit/c302bcc7f3c0ec66c6167b2859c2fc63c42be872)
records the accepted separation: `.inner` reaches owned Box content and
`deref` reaches a reference's referent. The current REF-1 and TYPE-7, not the
early examples, govern source programs.

Three viable alternatives preserve the reference/owner distinction:

| Form | Example | Tradeoff |
|---|---|---|
| Current explicit prefix step | `&deref(node).inner.next` | Explicit for both a whole referent and a projection, but wraps the existing path. |
| Explicit postfix step | `&node.*.inner.next` | Keeps the explicit distinction and extends paths in reading order. Changes a spelling, not an access permission. |
| Automatic projection through a known reference | `&node.inner.next` | Shorter still; only the known local kind is needed. Whole-referent access still needs a separate spelling, and a rule must fix when an explicit referent step is admitted before a projection. |

Automatic projection need not imply general inference or overloaded
dereferencing. A Whitefoot candidate could insert exactly one reference step
before a field, payload, index, range or measure selection, never through a
Box. [Rust's automatic field dereferencing](https://doc.rust-lang.org/reference/expressions/field-expr.html#automatic-dereferencing)
is broader: it follows `Deref`/`DerefMut` repeatedly. That trait mechanism is
not proposed here. Separately, an implicit whole-referent read selected by an
expected type would give bare `p` a referent-read meaning at value arguments
while `let q = p;` still needs an alias default. That is a further contextual
conversion, not an unavoidable ambiguity or a necessary part of shorter paths.

The initial, unapproved proposal was the explicit postfix step `.*`, replacing
`deref(place)` throughout ordinary and proof places. These examples are the
dot-star comparator for the arrow follow-up below:

```text
Current                                  Proposed
let alias = cursor;                      let alias = cursor;
match deref(cursor) { ... }              match cursor.* { ... }
let value = deref(node).inner.value;      let value = node.*.inner.value;
set cursor = &deref(node).inner.next;     set cursor = &node.*.inner.next;
set deref(cursor) = f(head: move deref(cursor));
set cursor.* = f(head: move cursor.*);
deref(entry(values)).inner.len           entry(values).*.inner.len
```

The new grammar candidate replaces only these productions:

```text
pbase   := IDENT | "entry" "(" IDENT ")"
psuffix := "." IDENT | "." TYPEID "." IDENT | "[" atom range_tail? "]" | "." "*"
```

The step is admitted only on a reference kind; `Box.inner` is unchanged.
Bare references still forward or alias, `set cursor = ...` still rebinds,
`set cursor.* = ...` still writes the referent, and `&cursor` remains invalid.
Index capture, payload refinements, range bounds, no moving through references
outside admitted operations, and invalidation follow the same resolved path.
Effect rows keep their existing parameter-rooted grammar (`writes(values.inner)`);
this proposal does not add `.*` to effect selectors. There is no overload or
recursive automatic traversal. Retire the `deref` terminal; its bytes become
an ordinary IDENT under FORM-3. The old prefix access is not retained as an alias.

This is a lexical and path-composition improvement. It does not remove the
semantic distinction the explicit step carries, transfer fewer owners, or
establish a productivity or compile-time gain. The strong-LL(2) experiment
below removes parsing ambiguity as an objection; it does not select the form
by implementation convenience. Its selection ground is one explicit step
usable identically for a projection and the whole referent, without wrapping
the preceding path or making that step optional at selected sites.
The owner later selected postfix `^`, retaining this path-composition model
with a different token. The [selected form](#selected-postfix-caret) supersedes
the dot-star and arrow proposals. The named-constant root correction is also
approved: the live reference-validity decision and its ancestor include named
constants as REF-1 already does, without changing language permissions.

## Arrow comparison

The owner requested `->` as an alternative to `.*` on 2026-09-27. Before
running the grammar comparison, require each candidate to cover the same
resolved paths: whole-referent reads and writes, field and measure selection,
enum payloads, indices and ranges, `entry` projections, reference formation,
rebinding and atomic replacement. Keep Box content at `.inner`, effect
selectors unchanged, and ordinary ownership, visibility and proof rules.

Compare the current prefix form, the proposed dot-star step, an arrow step
with an ordinary following dot, arrow member selection with prefix access for
the whole referent, and a generalized arrow covering both selection and the
whole referent. Every candidate must have one grammar-selected spelling per
path: accepting both `p->field` and `p->.field`, or both `p->field` and
`deref(p).field`, is not an acceptable simplification under the surface-form
decision. Record the complete factored productions before running the native
strong-LL(2) generator; reject a prediction conflict, and include a deliberately
conflicting grammar as a negative control. Grammar success establishes only
parsing feasibility, not writer preference, semantic implementation or speed.

The comparison includes whole-reference match/assignment, a reference to a
Box, a range reference, and a proof path starting at `entry`. Member spelling
alone cannot settle the choice. Reuse of the signature token `->` must be
checked against the full grammar, not treated as an assumed ambiguity.

The [retired native driver](../../../archive/experiments/reference-access-grammar/main.rs)
selected a grammar for reference access. The v0.76 compiler and formal cases
supersede that probe. Reproduce the commands below in a checkout of
`44032cac693afe4246cf9e9b872ddf40c2491945`, where the driver and imported generator
retain their original paths and fixed-terminal inventory. It changes only
`place`, `pbase` and `psuffix` in memory and calls the unchanged compiler
generator on the full specification. The candidates and their complete
production replacements are fixed in that driver before measurement.

```sh
perl .github/run-check.pl reference-access-build rustc --edition=2024 research/experiments/reference-access-grammar/main.rs -o /tmp/whitefoot-reference-access-grammar
perl .github/run-check.pl reference-access-grammar /tmp/whitefoot-reference-access-grammar spec/kernel-spec.md
```

A follow-up candidate pairs arrow selection with prefix `*p` for the whole
referent, instead of assuming a choice between retained `deref` and a trailing
arrow. Before running it, add `arrow-prefix` to the same driver with
`place := pbase psuffix* | "*" place`, the postfix base, and the arrow-selectors
suffix. Require the full grammar, including multiplication and affine proof
terms, to remain strong LL(2). The prefix takes a complete place, so
`*p->field` denotes access through the selected field, not a second spelling
of `p->field`; parentheses around an arbitrary place are not added. Ordinary
reference-kind checks remain required. Compare the conventional whole-access
pairing against the total-arrow candidate's single postfix family.

### Observations and comparison

On v0.75 at `126d201d6`, using the driver fixed in `6afb02738`, the baseline,
dot-star, literal arrow step, arrow members, arrow selectors and total arrow
grammars all generated strong-LL(2) tables. The deliberately conflicting
`pbase` alternatives rejected with GRAM-1 on `Identifier Dot`; the driver
requires GRAM-1 specifically in `pbase`, and the complete command exited 0.
The reported token pair is the observed diagnostic, not another assertion in
the driver. The full grammar includes signature arrows, qualified names and
module forms. These
are grammar-generation observations, not accepted programs on a modified
compiler. No parsing or compiler timing comparison was performed.

The follow-up `arrow-prefix` production was fixed in `150483eee` before
measurement. It also generated a strong-LL(2) table under the full v0.75
token grammar, including arithmetic and affine proof terms; its guarded command
exited 0. Raw token formation does not supply those tokens for compact `*p`,
as the lexical analysis below establishes. This extends the comparison without rerunning or reinterpreting the
earlier source-semantic probes.

The candidate names below are the driver's arguments. In the table, `p` is a
reference to the selected value, `part` a range reference, and `node` a reference
to a Box; all examples are hypothetical candidate syntax.

| Operation | Dot-star | Arrow members | Arrow selectors | Total arrow | Arrow + prefix star |
|---|---|---|---|---|---|
| Whole referent | `p.*` | `deref(p)` | `deref(p)` | `p->` | `*p` |
| Field or measure | `p.*.field` | `p->field` | `p->field` | `p->field` | `p->field` |
| Box payload field | `node.*.inner.value` | `node->inner.value` | `node->inner.value` | `node->inner.value` | `node->inner.value` |
| Variant payload | `p.*.Some.value` | `p->Some.value` | `p->Some.value` | `p->Some.value` | `p->Some.value` |
| Indexed referent | `part.*[i]` | `deref(part)[i]` | `part->[i]` | `part->[i]` | `part->[i]` |
| Re-slice | `&part.*[lo..hi]` | `&deref(part)[lo..hi]` | `&part->[lo..hi]` | `&part->[lo..hi]` | `&part->[lo..hi]` |
| Entry proof path | `entry(node).*.inner.len` | `entry(node)->inner.len` | `entry(node)->inner.len` | `entry(node)->inner.len` | `entry(node)->inner.len` |
| Rebind holder | `set p = &next;` | `set p = &next;` | `set p = &next;` | `set p = &next;` | `set p = &next;` |
| Write referent | `set p.* = value;` | `set deref(p) = value;` | `set deref(p) = value;` | `set p-> = value;` | `set *p = value;` |

The literal `arrow-step` candidate also passes, but writes `p->.field`:
mechanically changing the `.*` step into `->` does not produce `p->field`.
It gives the arrow a standalone dereference meaning while keeping the ordinary
dot, retaining uniform composition at the cost of an unfamiliar `->.` join.
That is not the member-access readability improvement the owner requested.

`arrow-members` keeps the familiar member selector, but whole referents and
index/range access retain prefix wrapping. `arrow-selectors` extends the arrow
to index/range access as well, retaining `deref(p)` only for the whole referent.
Both are complete alternatives, without implicit reads or type-directed
spelling. Their grammar prevents a projection immediately after `deref(p)`
when the arrow form owns that projection, so they do not keep two spellings
for the same path. Neither removes the repeated prefix access from the
owned-link atomic replacement example.

`arrow-total` uses the arrow both with a following selector and alone. It
covers the original postfix proposal's complete scope and eliminates prefix
wrapping, including `set cursor-> = f(head: move cursor->);`. Its decisive
cost is the standalone form: `match cursor-> { ... }`, `consume(value: p->);`
and `return p->;` can look like unfinished member accesses to a reader used to
C-like syntax. That cost is visible in the examples; no writer trial has
measured its frequency or severity. A second arrow does not traverse a Box:
`node->inner.value` selects its owned content, and `node->inner->value` would
still fail the reference-kind check. No candidate relaxes field visibility,
reference validity, write effects, bounds proofs or the permitted move sites.

`arrow-prefix` removes both prefix wrapping and the dangling arrow: whole
access is `*p`, member access `p->field`, and range access `part->[i]`.
The owned-link update becomes `set *cursor = f(head: move *cursor);`.
Its token-grammar cost is two related forms rather than one postfix step;
compact source also has the unresolved lexical conflict below. The prefix
applies to the entire following place: `*p->field` selects through `p`, then
tries to dereference that field. It is not an alias for `p->field`; with
reference-free stored values, that extra reference step is a kind
error. Neither `(*p).field` nor `p->.field` is a place production, so the
candidate keeps one written form per path without checker-selected omissions.

The [C++ draft's built-in member-access rule](https://eel.is/c++draft/expr.ref#2)
relates `p->member` to member selection on the pointed-to object. That is the
useful semantic analogy: an explicit indirection followed by a member
selection. Whitefoot applies it to a checked path reference rather than a C++
pointer, provides no pointer arithmetic or overloaded arrow, and keeps Box
content separate. Standalone `p->`, `p->[i]` and variant/measure selections
are Whitefoot extensions; C++ member syntax does not supply them. In particular,
familiarity of `p->member` is not evidence that the whole proposal is familiar.
Prefix `*p` shares the draft's explicit indirection meaning but, as with the
arrow, applies only to a Whitefoot reference, never directly to a Box. The
`->[i]` and `->[lo..hi]` forms remain Whitefoot extensions; bare `part[i]`
does not silently dereference a range holder.

All arrow variants need FORM-2 to distinguish a compact path arrow from the
spaced signature arrow. Total arrow additionally distinguishes a selector
arrow's compact right join from the standalone step: `p->field`, `p->[i]`,
`set p-> = value;`, `match p-> {`, `p-> > limit`, and `fn f() -> result: T`.
Those roles are grammar-selected, as the current format already distinguishes
comparison `<` from a type-argument `<`; no type inference or later use is
needed. Adding `->` to both global attachment sets would format these cases
incorrectly. `.*` also needs its closing punctuation joins checked, but
already gets its internal dot/star join from the existing dot attachment.
The initial `arrow-prefix` candidate proposed a compact prefix star while
keeping infix multiply spaced. Grammar-selected formatting alone cannot make
that work: raw formation happens first and the compact bytes do not produce
the intended tokens.

### Raw lexical boundary of prefix star

GRAM-1 forms `*` followed by the maximal lowercase suffix as one operator
candidate, then terminal membership admits only the closed operator list.
This is also the native scanner's `operator_form` rule; the maintained syntax
test rejects `*x` as one token. The following are deductions from that lexical
rule, distinct from the generator's measured token-grammar result:

| Source bytes | Raw token spellings | Consequence |
|---|---|---|
| `*p` | `*p` | Invalid operator suffix; it is not prefix star plus identifier. |
| `*cursor` | `*cursor` | The same failure for the owned-link example. |
| `*wrap` | `*wrap` | The existing wrapping-multiply terminal, not access through a binding named `wrap`. |
| `* p` | `*`, `p` | Supplies the tested prefix grammar's tokens, but needs a spaced canonical form instead of the proposed compact one. |
| `*(p)` | `*`, `(`, `p`, `)` | Lexically suitable, but needs a different explicit parenthesized-access production; it is not covered by the prefix candidate's place grammar. |
| `p->field` | `p`, `->`, `field` | The existing compound arrow forms independently of the following identifier. |
| `p.*.field` | `p`, `.`, `*`, `.`, `field` | The dot terminates the operator candidate before the field name. |

Simply splitting an unrecognized star-word would still leave `*wrap` and
other admitted mode words colliding with ordinary identifier access. A lexer
that consults expression position would change GRAM-1's context-independent
formation, while renaming the arithmetic forms would expand the change to
operation spelling. Neither follows from a preference for arrow projection.
A complete compact-prefix proposal would need an explicit lexical rule and
controls for all mode-word identifiers and existing infix uses. The selected
caret form avoids that lexical revision; the compact-prefix proposal is
superseded. A token-grammar pass is not source feasibility.

### Superseded arrow recommendation

The lexical finding led to recommending arrow selectors with retained
`deref(p)` for whole access. It shortened field and index paths without
changing arithmetic tokenization, but retained prefix wrapping for reads,
matches and atomic replacement and required two related access forms.
The owner instead selected `^` for one explicit postfix step in every place.
Total arrow remained viable but made standalone access look unfinished;
dot-star retained uniformity with a longer punctuation sequence. These are
source-form judgments, not measured writer error rates or performance gains.

### Dependent source-spelling decisions

The same caret ruling updates the referent form in
[borrow-lexicon](../../../design/language/surface-form/borrow-lexicon.md),
the reference-to-Box example in [data-model](../../../design/language/data-model.md),
the rendering clause in [checker-facts](../../../design/compiler/checker-facts.md),
and the reference-parameter rendering clause in
[rejection-payloads](../../../design/compiler/rejection-payloads.md), alongside
[reference-validity](../../../design/language/ownership/reference-validity.md).
Their reference/Box distinction, typed root-and-step representation and single
canonical diagnostic renderer remain unchanged. Historical rejected examples
retain the old spelling. The resolved access amendment is removed; v0.76
carries the coordinated source implementation.

## Selected postfix caret

The owner selected `^` after comparing C-like access, uniform prefix access,
and explicit postfix symbols. The intended complete path grammar is:

```text
place   := pbase psuffix*
pbase   := IDENT | "entry" "(" IDENT ")"
psuffix := "." IDENT | "." TYPEID "." IDENT | "[" atom range_tail? "]" | "^"
```

The chosen forms extend a path from left to right:

| Operation | Selected spelling |
|---|---|
| Whole referent | `p^` |
| Field or measure | `p^.field`, `part^.len` |
| Box payload field | `node^.inner.value` |
| Variant payload | `p^.Some.value` |
| Indexed referent | `part^[i]` |
| Re-slice | `&part^[lo..hi]` |
| Entry proof path | `entry(node)^.inner.len` |
| Rebind holder | `set p = &next;` |
| Write referent | `set p^ = value;` |
| Atomic replacement | `set cursor^ = f(head: move cursor^);` |

`^` admits only a reference-kind operand. Bare names still forward or alias;
Box content remains `.inner`, and effect selectors remain parameter-rooted
without a dereference marker. Bounds, captured indices, refinements, validity,
move permissions and exactly-once target evaluation retain their rules.
Ordinary and proof paths share the same suffix. Retire the prefix-access
production and its `deref` terminal; that word becomes an ordinary IDENT under
FORM-3. No arrow, prefix-star, dot-star or implicit-projection alias is selected.

Canonical formatting attaches `^` to the preceding place. Existing suffixes
join as in `p^.field` and `p^[i]`; operators and assignment remain spaced,
including `p^ + q^`, `p^ > limit` and `set p^ = value;`, and delimiters give
`f(value: p^);` and `match p^ { ... }`. This adds a place suffix without
introducing expression precedence or a type-directed omission.

The selection ground is one explicit step for both whole access and selectors,
with one punctuation byte rather than dot-star's two. A C-like family is
feasible if its lexical and automatic-indexing rules are changed, but needs
separate whole, member and indexed forms. A high-binding prefix star would
also reverse [C's precedence](https://www.gnu.org/software/c-intro-and-ref/manual/html_node/Binary-Operator-Grammar.html)
relative to field and index selection; a fresh
prefix dollar avoids the star collision but still needs that binding rule.
Postfix dollar is structurally equivalent to caret; its unfamiliar symbol
offers no compositional advantage over caret. None of these comparisons establishes a productivity or
compilation-speed advantage.

[Pascal's postfix caret](https://www.freepascal.org/docs-html/ref/refse15.html)
also denotes indirection to the referent. Whitefoot borrows that narrow
notation, not Pascal's pointer value model or pointer arithmetic: a WF
reference remains a checked captured path, not stored data, and `^` never
traverses Box content or supplies a missing proof. Familiarity is a provisional
readability ground; a writer trial could overturn it.

Before measuring the caret grammar, require the full v0.75 grammar to retain
strong LL(2), and a deliberately overlapping caret suffix to fail at
`psuffix`. The unchanged generator has no `Caret` predicate. The existing
driver therefore removes the prefix `deref` production and uses its now
unused fixed `Deref` predicate for the new suffix. Renaming that disjoint
predicate to `Caret` preserves prediction sets; this tests the token grammar,
not a lexer accepting `^` or a parser consuming caret source. The negative
control adds both a standalone suffix and a suffix followed by a dot and an
identifier, which compete with ordinary suffix composition. Require GRAM-1
specifically at `psuffix`, rather than accepting any experiment failure.

Run the existing guarded build command above, then:

```sh
perl .github/run-check.pl reference-caret-grammar /tmp/whitefoot-reference-access-grammar spec/kernel-spec.md caret
perl .github/run-check.pl reference-caret-conflict /tmp/whitefoot-reference-access-grammar spec/kernel-spec.md caret-conflict-control
```

Raw-source feasibility has a separate premise: add `^` to GRAM-1's standalone
punctuation. No existing word, numeric, operator or compound-punctuation form
contains that byte, so it needs no context-sensitive splitting or change to
arithmetic mode tokens. The current scanner rejects `^`; token-grammar
success must not be reported as implemented syntax.

### Caret grammar observation

The criteria and driver were fixed at `4a36a39aa6418f60379da8b467c5ffca20bdb7b3`
before measurement. On the unchanged v0.75 specification integrated from
`126d201d6003256d0f40452d082860cdbc449c67`, the guarded Rust build and both
commands above exited 0. The caret candidate generated a nonempty strong-LL(2)
table containing its `Deref` surrogate predicate. The negative control failed
with GRAM-1 at `psuffix`, reporting `Deref Dot`; the driver checks the rule and
production, not that exact diagnostic word. No source lexer, canonical
formatter or semantic implementation was tested by this observation.

## Validation and remaining uncertainty

The original criteria were published in commit `3ece3c54c` before the v0.69
probes. On v0.74, the same ownership and proof source observations were repeated
using the unmodified compiler from `ad51e05df`, built on research revision
`b519746b2` with `make -C compiler build` (gate profile, locked and offline).
Native controls use the default ordinary compiler path, without compute mode.
All source observations in the tables above and below retained their verdicts.
The current [HashMap program](../../../tests/programs/containers/hash-map-program.wf)
also compiled and returned native exit 0 through its standard-library imports.

The module migration changes reproduction: the compiler supplies the Deque
module from its embedded standard library, selected by the caller's aliases;
its implementation is no longer concatenated into the caller. The only
adaptations to the scratch probes were the `deque_rebase` alias and the
`std::process` aliases for the driver below. Existing formal cases and programs
use their current source, including enum-owned constructor qualification.
These adaptations change name resolution, not a probe's proof contract.

For baseline reproduction from the repository root:

```sh
make -C compiler build
probe_dir=$(mktemp -d)
perl .github/run-check.pl deque-control compiler/target/gate/whitefootc tests/programs/containers/deque-program.wf -o "$probe_dir/deque"
"$probe_dir/deque"
perl .github/run-check.pl cursor-control compiler/target/gate/whitefootc tests/programs/owned_link_cursors.wf -o "$probe_dir/cursor"
"$probe_dir/cursor"
perl .github/run-check.pl map-control compiler/target/gate/whitefootc tests/programs/containers/hash-map-program.wf -o "$probe_dir/map"
"$probe_dir/map"
```

For source-only probes, use `whitefootc --emit-llvm SOURCE... -o output.ll`
under the same guard. Supply the wrapper fragment plus this main:

```wf
alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  return exit_status(code: 0_u8);
}
```

The normal/error propagation pair used this helper, once with the bare operand
and once with `propagate move incoming`:

```wf
fn forward(incoming: Result<Box<u64>, unit>) -> result: Result<Box<u64>, unit> pure {
  let value = propagate incoming;
  return Ok<Box<u64>, unit>(value: move value);
}
```

Both versions compiled and their native drivers returned 0: `Ok` preserved
a boxed 7 and `Err(unit)` stayed an error. Inserting `let reused = move incoming;`
after propagation rejected with OWN-1 use-after-move. Additional controls:

| Source or transformation | Observation |
|---|---|
| `tests/conformance/cases/own1-neg-move-of-copy.wf` | OWN-1, move of copy. |
| `tests/conformance/cases/fn2-pos-the-template-is-the-spelling-authority.wf` | Compiles to LLVM; the symbolic body retains `move` at copy instances. |
| `tests/conformance/cases/xfail-own1-bare-affine-use.wf` | OWN-1, bare affine use. The historical filename is not a verdict change. |
| `tests/conformance/cases/type7-neg-propagate-reference-holder.wf` | OWN-1, move through reference at `propagate deref(holder)`. |
| Owned-link source with `match deref(cursor)` replaced by `match cursor` | TYPE-7, missing dereference. |
| Owned-link source with `deref(node).inner` replaced by `node.inner` | TYPE-7, missing dereference. |

For the grammar experiment, a scratch Rust driver imports the unchanged
`compiler/src/syntax/grammar/generator.rs`. Run its public
`generate(path, specification)` on the baseline text, then on a copy replacing
exactly `pbase` and `psuffix` with the candidate productions above. Both
complete without a prediction conflict on v0.69 and again on v0.74, including
the latter's module and qualified-call productions. Compile the driver with
`rustc --edition=2024` under the ordinary command guard. This uses the native
generator, not another parser or a revised language implementation.

The v0.76 amendment implements the approved consumption rule and selected
caret access in the lexer, parser, checker and formatter. Its validation covers old-form refusal and new-form acceptance, copy and generic
controls, whole-owner partial consumption, linear residuals, borrowed matches,
entry projections, effect overlap and reference invalidation. The source
boundary and Deque failures above are concrete evidence; error-rate improvement,
generated-writer performance, equal machine code and compile-time change remain
unmeasured. The grammar comparison itself ran no full repository gate;
implementation validation is reported against the implementation revision in
the PR.
