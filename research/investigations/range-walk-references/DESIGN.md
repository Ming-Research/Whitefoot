# Range-walk references: a replaced binding and a joined reference

Item #211. The [checker-soundness study](../specification-studies/checker-soundness.md#2-two-candidate-holes-confirmed-since)
found two programs whose `251_u8 + v` the range judgment proved from a value a write through a reference had already
changed. This record holds how the compiler judged them before the fix, whether the specification or the checker was at
fault, and the shapes that share their root.

## Verdicts before the fix

Commit 76242b431 is main 116b8cc with the two candidate programs added as conformance cases expecting rejection under
OP-2. The conformance adapter in CI run
[45606105588](https://github.com/Ming-Research/Whitefoot/actions/runs/45606105588) reported:

| Case | Expected | Reached |
| --- | --- | --- |
| `range2-neg-replaced-binding-written-through-reference` (candidate one) | reject OP-2 | accept |
| `range2-neg-joined-reference-written-struct` (candidate two) | reject OP-2 | accept |

Every other case of the corpus reached its declared verdict (3156 passed). Both candidates are therefore confirmed: no
rule of the ordinary check rejects either program first, and the range judgment discharges the deferred OP-2 obligation.

## The specification is not at fault

Candidate one sits between two sentences of [RANGE-2]: "a construction yields new storage of its own" and "A binding is a
location once a reference to it is formed". The first says what the constructed value is. A `set` then writes that value
into the binding, and because the binding is a location, RANGE-2's write rule makes it a new version of that storage. REF-2
says a write at the reference's own path is a content write and leaves the reference valid. So the write through `r` lands
in `s`, and the program must be rejected. The checker did not follow the rule: its walk gave `s` new storage at the whole
`set` and left `r` naming the old one.

Candidate two: [REF-1] makes a joined reference's target the union of the paths its incoming edges name. RANGE-2's join
sentence defines a disagreeing binding by the arm taken, and its call rule forgets what a callee's row writes at the
substituted path. The walk instead joined the two references as unknown and then, binding the result by the referent's
type, gave `r` storage of its own that nothing read.

No rule of the specification changes.

## The fix

- A whole-binding `set` whose checked commit displaces a live value stores into the binding's existing location. A
  revived or declared binding still takes new storage, since storage it held before was handed over by a move.
- References that differ between arms join as a view naming each of them. A write through it leaves each member unknown
  where the write overlaps it. A call taking it substitutes the callee's row at each member. A read through it is
  unknown. A reference binder of a value initializer, and a reference a loop body rebinds, hold an unknown reference
  rather than storage of their own.
