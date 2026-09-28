# Declaration and call boundaries after the reference redesign

This study began with the v0.67 callable surface after x1 removed returned
references and reference-valued types. The selected change removes written
`own` from value parameters and results. Result names remain mandatory in
signatures, beside their types; named call and construction operands retain
their existing rules. Contract-local result aliases were not selected.
Ownership transfer, reference access, expression nesting and the canonical
source policy are outside this change.

The follow-up [ownership-surface comparison](OWNERSHIP.md) examines transfer
and reference access using the current container and owned-link programs.

## Requirements and comparison criterion

The comparison criterion was recorded before examining the alternatives:

1. Parameter kinds, result types and ordinals, effects and contracts must be
   readable from the declaration alone. No body, actual argument, later use
   or inferred concrete type selects the written form.
2. Copy, drop and must-consume behavior, explicit consuming uses, reference
   invalidation and the prohibition on returned references remain unchanged.
3. Proof-only result identities remain distinct from runtime locals,
   parameter entry images, reference exit state and routed Result payloads.
   Generic instantiation and function-formal refinement preserve these roles.
4. The grammar remains deterministic, with one spelling selected by grammar
   class. Unit and generic results require no type- or use-dependent omission.
5. Operand labels must be assessed against the errors they actually detect:
   wrong, missing, repeated and reordered labels, with same-typed values
   exchanged under otherwise correct labels retained as a counterexample.

Complete scalar, generic, multi-result, routed-contract, reference-state and
resource examples discriminate the forms. Their frequency in this corpus is
not evidence of real-world frequency or writer productivity. A shorter
signature alone does not establish a better language or a compilation speedup.

## The v0.67 boundary and recovered history

The outgoing [v0.67 grammar and typing rules](../../../spec/kernel-spec-v0.67.md)
make the questions independent:

| Element | Role in v0.67 | Rules |
|---|---|---|
| `own` in `x: own T` | Selects value mode; the other parameter forms are `&T` and `&[T]`. A reference kind cannot be substituted for `T`. | GRAM-2/3, TYPE-8, FN-1 |
| `own` in `r: own T` | The only result mode; no reference result is legal. | GRAM-3, REF-3, FN-1 |
| A result name | Proof-only name for an ordinal; neither a body local nor part of callable equality. | TYPE-5/6, FN-1/4/9, CALL-4 |
| Argument or field label | Checks declaration names and order; does not permit reordering or select an overload. | GRAM-8/10/11 |

The [v0.32 grammar](../../../spec/kernel-spec-v0.32.md) had owned values,
shared/exclusive borrows and returned borrows. Its result type was unnamed;
FN-9 bound a result inside the postcondition form. The
[v0.33 change](https://github.com/mbbill/Whitefoot/commit/55a754340aa2be7f3374429534f16d197054aa36)
introduced mandatory proof-only result bindings while consolidating contracts.
The later result-list extension made those bindings identify result ordinals.

The [x1 spelling discussion](../access-effects/SPEC-AMENDMENT-MAP.md#surface-and-spelling)
recommended retaining result `own` because removal would rewrite signatures
without a semantic gain. This historical recommendation is not an independent
ground for retaining the spelling: current
[decision practice](https://github.com/mbbill/Whitefoot/blob/efe40194a05b339c46b931e87bd05a40036f23cf/docs/practice.md#evidence-guidance) excludes
internal migration cost as a language-selection ground before real adoption.
The resulting v0.60 grammar removed borrow results; v0.67 still retained
`rtype := "own" type`.

The previous [surface decision](../../../design/language/surface-form.md)
kept boundary modes written because readers could not reconstruct them there.
The selected revision distinguishes each element: `&` already separates
reference parameters from value parameters, and results have no other mode.
Types, result names, effects and contracts remain written. Named operands
retain their separate label/order-checking reason.

## Selected parameter and result forms

```text
Before:   fn mix(x: own u32, y: own u32) -> result: own u32 pure
Selected: fn mix(x: u32, y: u32) -> result: u32 pure

Before:   fn inspect<T>(value: &T, range: &[T]) -> result: own u64 reads(value), reads(range)
Selected: fn inspect<T>(value: &T, range: &[T]) -> result: u64 reads(value), reads(range)
```

This is a mandatory replacement for the entire grammar class, not an optional
abbreviation. A written `T` determines value mode without reading a body or
substituting a generic argument. It does not grant copy or drop capabilities
and does not change any `move` requirement.

The affected productions are:

```text
param := IDENT ":" (type | "&" (type | "[" type "]"))
rtype := type
```

Retire the `mode` production and fixed terminal `own`. Keep
`result_binding := IDENT ":" rtype`, result-list forms and contract grammar
unchanged. `rtype` retains its result-type role; there is no result-mode
choice. Under FORM-3, `own` becomes an ordinary IDENT, rather than remaining
a historical reserved word. The old `name: own T` annotation is rejected.
Ordinary functions, raw function formals, interface members and PRE-1 records
all use this boundary, including a named unit result.

## Result naming alternatives and selected disposition

The retained [contract decision](../../../design/language/checks-and-proofs/requires-entry-contract.md)
keeps result names in signatures. The earlier proposal to move them into a
contract-local `results(...)` declaration was not selected.

| Alternative | Benefit | Cost or reason not selected |
|---|---|---|
| Keep `name: T` in every result position | Name, type and ordinal are one declaration; no additional binder form. | Some names have no proof consumer, including unit result names. This is the selected tradeoff. |
| Optional or selectively omitted header names | Concise simple headers. | Optional naming gives two annotation forms; omission based on concrete type or use requires a conditional spelling rule. |
| Implicit result names or indices | No name declaration. | Removes descriptive names or introduces a second mechanism; indices expose ordinal bookkeeping in proof expressions. |
| Bind names per ensures clause | Clause-local names. | Repeats ordinal mappings across independent clauses; cross-result clauses still need both identities. |
| One contract-local `results(...)` list | Places proof names with their consumers. | Splits types and names into two ordered declarations whose correspondence must be maintained; adds a keyword and declaration. |

```text
Rejected:  -> (u64, u64) ... contract { results(written, remaining); ... }
Selected:  -> (written: u64, remaining: u64) ...
```

A count check cannot establish that a permutation of names for same-typed
results matches the writer's intended correspondence. Keeping names beside
types fixes that mapping in one place. Header names also describe result
roles without a postcondition. Those benefits outweigh introducing a separate
alias declaration solely to remove unused names. This choice is independent
of removing the ownership qualifier; neither choice was selected by corpus
counts or implementation effort.

Other languages demonstrate the alternatives without deciding Whitefoot's.
[Rust's function grammar](https://doc.rust-lang.org/reference/items/functions.html)
has an unnamed output type; its omitted unit output is not selected here.
[WhyML](https://why3.org/doc/syntaxref.html) offers named results, implicit
`result` and postcondition patterns, rather than one required binding form.
[Dafny method outputs](https://dafny.org/dafny/QuickReference) are named
parameters that the body assigns, unlike Whitefoot's proof-only names.

## Complete examples

The [SHA-256 scalar helper](../../../tests/programs/sha256_abc.wf) becomes:

```text
fn xor_three(x: u32, y: u32, z: u32) -> result: u32 pure {
  let first = ixor(x, y);
  return ixor(first, z);
}
```

The [generic measured-result helper](../../../tests/conformance/cases/call4-pos-route-names-a-result-ordinal.wf)
retains its named relation and explicit transfer:

```text
fn filled<T: copy, const n: u64>(value: T) -> result: Slots<T, n> pure contract {
  ensures result.len == n;
} {
  doc "Builds a run of n slots, every one holding a copy of value; the copy bound is what admits the bare repeated use.";
  let block = array_filled::<T, n>(value: value);
  let built = slots_from_array::<T, n>(values: block);
  return move built;
}
```

The same source's two-result helper keeps names beside their types and the
routed payload distinct from the whole-result name:

```text
fn probe(taken: Slots<u8, 8>) -> (outcome: Result<u64, Overflow>, spare: u64) pure contract {
  ensures when outcome is Ok(value: reported): reported == taken.len;
  ensures spare == taken.len;
} {
  doc "One route names the enum ordinal it applies to; the unrouted clause names the other.";
  let measured = taken.len;
  return Ok<u64, Overflow>(value: measured), measured;
}
```

The caller still writes `let (outcome, spare) = probe(taken: move run);`.
Caller locals and callee proof names remain separate bindings connected by
ordinal. Result names remain unavailable in the body, requires and contract
definitions, and excluded from callable-signature equality.

The [owning-map helper](../../../tests/programs/containers/owning-behavior.wf)
still writes `-> result: unit` for its exit-state-only contract. Its `&Box`
parameter, write row, body and invariants do not change. The
[formal-result case](../../../tests/conformance/cases/fn4-pos-formal-owned-result-transfer-2.wf)
still declares `fn make(value: T) -> result: T pure;` under the same FN-4
rules. The [deque resource helper](../../../tests/programs/containers/deque-program.wf)
still consumes its `DequeTicket` and transfers its payload with explicit
`move`; the bare type grants no additional capability.

## Named operands remain independent

Retain GRAM-8/10/11, including declared order and single-field labels.
Function-formal calls still use the formal's labels, not the selected actual's.

For `fn difference(left: u32, right: u32) -> result: u32 pure`, whose body
returns `left -wrap right`, `difference(right: 2_u32, left: 7_u32)` is rejected
for label order. `difference(left: 2_u32, right: 7_u32)` remains well-formed:
labels cannot prove which values the writer intended. Construction fields of
the same type have the same limitation.

The `language/surface-form/construction-form` decision states that exact
label/order protection and its limit, instead of claiming that labels detect
same-typed value swaps. No positional form, reordered arguments, shorthand
or field rule is introduced.

## Implementation structure and validation boundary

Continue through the generated grammar and ordinary parameter formation path.
Read value/reference/range kind from the parameter's own written prefix before
substitution, returning the existing internal `CheckedMode`. Removing a syntax
node does not remove semantic ownership modes. PRE-1 records use the same
declaration grammar. Results continue through `rtype` and the existing
result-binding record. No alias table, inferred mode or second signature
decoder is introduced.

Update the active grammar, fixed-terminal inventory, generated syntax
consumers, signature examples, source/prelude records, conformance sources
and compiler tests together. FORM-2's named result-list example loses only
`own`; no new line-bearing production or spacing rule is needed. The earlier
DCR formatting finding concerned only the unselected `result_names` production.

Validation must cover primitive and generic value parameters, both reference
kinds, named single/multiple/unit results, function formals and PRE-1 records;
old annotation rejection and ordinary-IDENT use of `own`; unchanged ownership
and capability failures, result scope, routes and formal refinement. Migrate
source spellings without weakening existing verdicts. Generated node paths
and byte locations may change, but each diagnostic test must still identify
its original offending construct. The canonical `make check` remains the
complete correctness check.

The selected opportunity removes a redundant qualifier while retaining one
combined result declaration. Types, ownership, effects, the proof domain, ABI
and Result-fact transport remain unchanged. No runtime or compile-time gain
is claimed. Ownership transfer and expression/formatting questions remain
separate TODO groups with their own validation requirements.

## Earlier comparison evidence

The [initial study revision](https://github.com/mbbill/Whitefoot/blob/7c5eff38fbcf9aef2790ab3237353c6644f4a14f/research/investigations/contract-surface/CALL-BOUNDARY.md#validation-of-this-study)
records the exact 2026-09-23 scratch probes and reproduction steps against
v0.67 at `9bed1c33`. The native harness kept the compiler's FIRST, FOLLOW and
SELECT algorithms, adapting only the fixed-terminal mapping and expected
production inventory for the compared grammars.

| Compared grammar | Productions | Observation |
|---|---:|---|
| v0.67 baseline | 86 | Strong-LL(2) tables generated. |
| Selected removal of `own`, retaining header names | 85 | Strong-LL(2) tables generated. |
| Unselected contract-local result-name candidate | 84 | Strong-LL(2) tables generated. |
| Deliberately duplicated parameter type arm | 84 | GRAM-1 rejected shared two-token lookahead. |

Seventeen v0.67 source probes matched the stated label boundary: six maintained
units and four correctly labeled/value-swapped controls were accepted, six
call-label/arity/positional controls were rejected by GRAM-11, and reversed
constructor labels were rejected by GRAM-8. These were checking/lowering
probes, not execution or candidate semantic validation. They discriminate the
alternatives; maintained compiler and conformance tests own implementation
correctness.
