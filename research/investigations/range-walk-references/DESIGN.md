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

### The shapes that share the root

The same-root cases were then judged by the compiler of main 1caef2cd5 with `tests/conformance` as it stands at commit
e24e372ed, so with every case of this item and none of the fix. The conformance adapter in CI run
[45761264248](https://github.com/Ming-Research/Whitefoot/actions/runs/45761264248) reported:

| Case | Expected after the fix | Reached before it |
| --- | --- | --- |
| `range2-neg-replaced-binding-written-through-reference` | reject OP-2 | accept |
| `range2-neg-joined-reference-written-struct` | reject OP-2 | accept |
| `range2-neg-replaced-array-written-through-reference` | reject OP-2 | accept |
| `range2-neg-replaced-binding-written-through-reference-index` | reject OP-4 | accept |
| `range2-neg-replaced-enum-written-through-reference` | reject OP-2 | accept |
| `range2-neg-replaced-in-one-arm-written-through-reference` | reject OP-2 | accept |
| `range2-neg-replaced-binding-written-through-copied-reference` | reject OP-2 | accept |
| `range2-neg-rebound-reference-copy-written-struct` | reject OP-2 | accept |
| `range2-neg-loop-rebound-reference-copy-written-struct` | reject OP-2 | accept |
| `range2-neg-joined-element-reference-written-field` | reject RANGE-3 | accept |
| `range2-neg-updated-binding-written-through-reference` | reject OP-2 | accept |
| `range2-neg-updated-affine-binding-written-through-reference` | reject OP-2 | accept |
| `range2-neg-joined-reference-call-writes-struct` | reject OP-2 | reject OP-2 |
| `range2-neg-revived-binding-keeps-moved-value` | reject OP-2 | reject OP-2 |
| `range2-pos-replaced-binding-written-through-reference` | accept | reject OP-2 |
| `range2-pos-joined-reference-write-keeps-unrelated-facts` | accept | accept |
| `range2-pos-joined-reference-call-keeps-unrelated-facts` | accept | reject RANGE-3 |

Every rejection case but two was accepted, so each witnesses the defect. The other two guard the fix:

- `range2-neg-joined-reference-call-writes-struct`: a call taking a joined reference was already sound, since the walk
  forgot every location when a written reference argument held no reference. It guards that the per-member substitution
  still forgets what the callee writes.
- `range2-neg-revived-binding-keeps-moved-value`: it guards that a binding given a value after a move takes new storage;
  writing the moved storage in place would make it accepted.

The two `updated` cases answer whether the checker keeps a reference valid across an atomic update `set s = f(s)`: it
accepts both spellings, the copy and the `move`, without a REF-2 rejection, and REF-2 agrees, since the update writes
the storage at the reference's own path. So the walk's write-in-place covers an atomic read-out as well as a live value.

Of the acceptance cases, the replaced-binding one was rejected because the walk read the stale 9, and the call one
because the call forgot every fact, `marks`'s included. The write one was accepted before the fix only because the write
landed in storage nothing read; after the fix it shows that a write through a joined reference keeps the facts of
storage it cannot reach.

### A subscript through a joined reference

Three cases subscript `r^[3_u64]` where `r` names `a^` or `b^`, two `Slots<u64, 4>` parameters whose lengths the
contract fixes. They were judged by the compiler of main c807b5941 with every case of this item and none of the fix
(commit fdf8ecab4); the conformance adapter in CI run
[45771134799](https://github.com/Ming-Research/Whitefoot/actions/runs/45771134799) reported:

| Case | Lengths of `a^`, `b^` | Expected after the fix | Reached before it |
| --- | --- | --- | --- |
| `range2-neg-joined-reference-index-past-one-member` (a read) | 4, 2 | reject OP-4 | reject OP-4 |
| `range2-neg-joined-reference-index-write-past-one-member` (a write) | 4, 2 | reject OP-4 | reject OP-4 |
| `range2-pos-joined-reference-index-inside-every-member` (a read) | 4, 4 | accept | reject OP-4 |

The same run gave every case of the table above the verdict listed there. Before the fix the walk bound `r` to storage
of its own whose length nothing fixed, so every subscript through it was unproved. The two rejection cases therefore
guard the fix rather than witness the defect: they reject only if the bound is proved at every place `r` names, so a
check at the first member alone, or acceptance once any member is proved, makes them accepted. The acceptance case
shows that the fix proves the bound at each member rather than giving up on a joined reference.

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

- A whole-binding `set` whose checked commit displaces a live value, or writes back the storage an atomic update read
  out, stores into the binding's existing location. A revived or declared binding still takes new storage, since storage
  it held before was handed over by a move.
- References that differ between arms join as a view naming each of them. A write through it leaves each member unknown
  where the write overlaps it. A call taking it substitutes the callee's row at each member. A subscript through it is
  proved in bounds at each member. A read through it is unknown. A reference binder of a value initializer, and a reference a loop body rebinds, hold an unknown reference
  rather than storage of their own.
