# Length-expression arena invariant witnesses

These temporary programs isolate the proof needed by an immutable, locally
indexed length-expression arena for CSS `min()`, `max()` and `clamp()`. A
node's operands precede it, and its root is valid. Recursive evaluation
reads independent operands and needs no runtime index checks or sequential
evaluation pass.

`generic.wf` states those invariants on `LengthMath<L>`, as the proposed
representation naturally requires. `concrete.wf` changes only the wrapper
to a concrete `ComputedMath` holding `MathNode<f32>`. It is a diagnostic
control, not a selected production alternative. The two files differ only
in the wrapper's name and generic parameter; every proof step is identical.

## Observed results

The pinned Whitefoot release is the one `whitefoot.pin` names, and its
source revision is the language read here, as
[downstream.md](../../../../whitefoot-kit/downstream.md#reading-the-language)
describes. The first CI run of the witnesses, before the written proof
below existed, gave:

- `generic.wf`: rejected with TYPE-11 (`InvalidTypeInvariant`) at
  `invariant valid_root(v): v.root < v.nodes.inner.len;`, because its struct
  declares generics. TYPE-11 admits a type invariant only when its struct
  declares no generics, and its range-invariant paragraph keeps that
  condition. This is the gap the request below asks Whitefoot to close.
- `concrete.wf`: rejected with FN-8 (`UndischargedCallRequirement`) at the
  first recursive call, goal `value^.nodes.inner[at].Min.left <
  value^.nodes.inner.len`, unproved. The function's requirement gives
  `at < len`, and the type invariant over the parameter gives
  `nodes[k].Min.left < k` for every `k` below `len`; the range judgment did
  not combine them by itself.

## The written proof

Each recursive call now receives a copied operand index whose bound a local
invariant proves from one written instance of the type invariant:

```
Min(left: l, right: r) => {
  let first_at = l^;
  invariant first_in: first_at < value^.nodes.inner.len {
    use left_before(at);
  }
  let first = eval(value: value, at: first_at);
  ...
}
```

The second operand repeats this with `second_at`, `second_in` and
`right_before`. The type invariants' range facts are named `left_before`
and `right_before`, so the name a `use` step cites is not also a payload
field. The form rests on these rules of the pinned specification:

- TYPE-11: a range type invariant names itself by its clause's name, and
  for a reference parameter of the struct's type it is a range requirement
  of the function.
- RANGE-4: a `use NAME(atom, ...)` step is admitted in the proof of an
  `invariant_stmt` where NAME is a range requirement of the function, and
  the instance joins the problem in which the range judgment proves the
  invariant's target.
- RANGE-2: a local invariant that ordinary entailment leaves undischarged,
  whose sides are range terms, is owed to the range judgment; `first_at` is
  a live integer binding and `value^.nodes.inner.len` a measure, both range
  terms. Once proved, the target is a path condition, so the call's
  requirement `first_at < len` follows.
- RANGE-1: the payload read `nodes[at].Min.left` is defined where the tag
  of `nodes[at]` is `Min`, which the `Min` arm of the `match` establishes.

The conformance case `range4-pos-use-in-local-invariant.wf` is this form
over a written range requirement: `let t = targets^[i];` then
`invariant bounded: t < 4_u64 { use valid(i); }`.

### Open risk: whether the type invariant's name is in scope

No pinned conformance case cites a range type invariant by name in a `use`
step. RANGE-1 places a requirement's or postcondition's fact name in the
function's signature scope and a header invariant's in its loop's header
scope, and says nothing for a type invariant's name. The pinned compiler
declares that name only in the type invariant's own scope, so
`use left_before(at)` inside `eval` may fail as the ordinary INV-1
unresolved-name error. If CI shows that, it is a second gap: the
specification makes the parameter's type invariant a range requirement
RANGE-4 may cite, yet gives the writer no name in the function's scope to
cite it by, and a function with two parameters of the struct's type would
need a name per parameter. Restating the type invariant as a written
requirement of `eval` would hide that gap and is not done here.

A second risk is PRF-1's redundancy rule: if the range judgment proves the
local invariant's target without the written instance, the certificate is
rejected as redundant (`prf1-neg-redundant-range-use.wf`). The first CI run
suggests it does not, because the same goal was unproved at the call.

## Draft Whitefoot request: type invariants on generic structs

**Natural form.** One arena type serves both of Snowghost's length stages:
a specified length expression, whose leaves are values with units and
percentages, and a computed one, whose leaves are resolved `f32` pixel
values. The arena is generic over the leaf, and its invariants say every
operand index precedes its node and the root is in range:

```
enum MathNode<L: copy> {
  Leaf(value: L);
  Min(left: u64, right: u64);
}

struct LengthMath<L: copy> {
  nodes: Box<Array<MathNode<L>>>;
  root: u64;
  invariant valid_root(v): v.root < v.nodes.inner.len;
  invariant(v): forall left_before(k in 0_u64..v^.nodes.inner.len): v^.nodes.inner[k].Min.left < k;
  invariant(v): forall right_before(k in 0_u64..v^.nodes.inner.len): v^.nodes.inner[k].Min.right < k;
}
```

None of the invariants names a value of type `L`: each is about indices and
lengths, which mean the same at every instance.

**Refusal.** The pinned compiler rejects the struct with TYPE-11
(`InvalidTypeInvariant`) at `valid_root`, because TYPE-11 admits a type
invariant only when its struct declares no generics.

**Control.** `concrete.wf` is the same program with a non-generic
`ComputedMath` holding `MathNode<f32>`. TYPE-11 admits it; its remaining
question is the proof of the operand bounds described above.

**Why the generic form is the natural one.** The arena's structure and its
evaluation proofs do not depend on the leaf type. Without generic
invariants Snowghost must either write one copy of the arena type, its
invariants, its constructors and its evaluator per leaf type, or drop the
invariants and check every operand index at run time. The first duplicates
proved code for each stage that holds lengths; the second adds a check the
structure already rules out.

**Acceptance test.** `generic.wf`, unchanged, compiles with the Whitefoot
compiler and its binary exits with status 0.

## CI commands

Run only in CI, after downloading the compiler selected by `whitefoot.pin`
and installing the toolchain required by the ordinary project gate. From
the repository root, with `WHITEFOOTC` set to that compiler's absolute path:

```sh
mkdir -p build/research/layout/length-math-witnesses
"$WHITEFOOTC" research/investigations/layout/length-math-witnesses/generic.wf -o build/research/layout/length-math-witnesses/generic
```

Record that command's own exit status and diagnostics. It should fail for
the TYPE-11 generic-invariant restriction, not an unrelated syntax error.
Then independently run the control:

```sh
"$WHITEFOOTC" research/investigations/layout/length-math-witnesses/concrete.wf -o build/research/layout/length-math-witnesses/concrete
build/research/layout/length-math-witnesses/concrete
```

The standalone source and `-o` CLI form is documented by the pinned
Whitefoot README's compiler examples.

Remove this directory once the gap's disposition is settled and a production
length-math case exercises the resulting proof.
