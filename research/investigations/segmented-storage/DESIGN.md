# Element subtrees and segmented storage

Status: design selected by the owner on 2026-09-29 (two parts below);
implemented with specification v0.81 on mbbill/Whitefoot#186, whose
design-tree and specification changes await the owner's approval. The
validation below holds.

## Question

Snowghost's renderer now keeps only each algorithm's true data dependencies
and leaves the grain of parallel work to the compiler (Snowghost's
`pipeline` decision). Two shapes of per-item work recur in every renderer
stage, and [PAR-2] denies both today although each iteration touches only
its own item's storage:

1. **Writing inside element i.** An iteration that sets a field of its own
   element, or hands its element to a helper that writes it, or writes into
   a buffer its element owns.
2. **Writing a variable-length output.** Item i produces a number of
   outputs that differs per item (paint commands per box, glyphs per text
   run, lines per paragraph), and all outputs go to one contiguous buffer,
   item i's at an offset the counts before it determine.

Which language forms admit both as ordinary loops, without a special case
in the permission rule?

## Minimal examples

Compiled with the compiler of Whitefoot 13556101 with `--par-ledger`; the
sources are beside this file.

`field.wf`:

```whitefoot
for (i in 0_u64..count) { set a^[i].n = i; }
// PAR loop denied: condition 2: the body writes storage that is neither
// introduced by the iteration nor the accumulator, at set a^[i].n = i;
for (i in 0_u64..count) { set_n(p: &a^[i], v: i); }
// denied, condition 2, at &a^[i]
for (i in 0_u64..count) { let old = a^[i]; let made = P(n: i, m: old.m); set a^[i] = made; }
// permitted: eligible; no accumulator
```

[PAR-2]'s element family is exactly one direct `Array` or `Slots`
subscript. A write below that subscript, or a borrow of the element passed
to a writing helper, is not in the family. A write reached through a `Box`
the element owns meets a second gap: forming
`&a^[i].text.inner[0_u64..n]` is refused by the checker as an unsupported
reference formation, found by Snowghost's layout prototype.

`scatter.wf`: a loop writing `&out^[offsets^[i]..offsets^[i + 1]]` is
denied, condition 2, at that range: [PAR-2]'s proved range family needs
endpoints affine in the binder. The windows are disjoint whenever the
offsets do not decrease, which no fact the checker holds states.
`halving.wf`: the same scatter written as a recursion that splits the
offsets and the output at the middle offset is permitted today
(`PAR permitted pair(scatter_run, scatter_run)`), at the cost of a
recursive helper with runtime guards at every site.

## Measurement: where the outputs go

The alternative to one contiguous buffer is a buffer per item, which the
first part would let a loop write. `scatter/` simulates paint-command
generation: 200,000 items, item i producing 1 to 8 commands of 32 bytes by
a hash of i (about 900,000 commands), the whole pipeline repeated 20 times
with each repetition salted by the previous result so the repetitions run
one after another. All four builds return the same result.

- **A** (`a.wf`): each item allocates its own buffer in a parallel loop and
  fills it; a sequential pass copies the buffers into one array.
- **A2** (`a2.wf`): A's allocation and fill only.
- **B** (`b.wf`): a parallel loop counts each item's commands, a sequential
  pass turns the counts into offsets, and the recursion of `halving.wf`
  fills each item's window of one array in parallel.
- **C** (`c.wf`, added with the implementation): B's count loop, then
  `box_segments_filled` over the counts and a plain counted loop filling
  `&out.inner[i]`.

Best of three, seconds, on the development machine (four Intel Xeon cores
at 2.1 GHz, Linux 6.18) under the check lock:

| Build | Sequential | `--par` 1 worker | 2 workers | 4 workers |
|---|---:|---:|---:|---:|
| A | 0.753 | 0.701 | 1.040 | 1.147 |
| A2 | 0.711 | 0.755 | 1.150 | 0.982 |
| B | 0.150 | 0.177 | 0.125 | 0.103 |

A's cost is its four million small allocations (A2), which grow slower with
more workers; the copy costs little. B is five times faster sequentially and
eleven times faster at four workers. One contiguous buffer written by count,
offsets and fill is the shape to support; a buffer per item is not a
substitute. That per-item allocation slows down with workers is a separate
runtime finding (`docs/todo.md`).

## Selected design

**Part 1: an element's whole subtree is iteration-own storage.** A place
below an affine element subscript, through fields, payloads, `Box`
contents, further subscripts and ranges, is storage owned by that element
alone: aggregates hold only owned values [TYPE-8] and a `Box` has one owner
[TYPE-9], so nothing below element i is reachable from element j. [PAR-2]'s
element family becomes every access that descends from one mapped
subscript: a `set` target, an operand read, and a borrow passed as an
argument, whose callee's row is projected onto the actual path as for a
proved range reference. The checker's reference formation through a
subscript and a `Box` is completed so that such a borrow can be formed.

**Part 2: a fourth storage shape, `Segments<T>`.** A run of segments whose
lengths are fixed at construction, stored as one contiguous run of elements
and the segment boundaries, which the shape owns and no program can change.

- It exists only as the content of a `Box` (a runtime-capacity form); its
  readonly measure is `len`, the number of segments.
- It is built by a prelude function from a range of lengths and a fill
  value, `box_segments_filled`, which computes the boundaries, allocates
  once, and returns `None` when the total passes the element type's limit.
- The subscript `s[i]` selects segment i, a run of `T` like the referent of
  a range reference: `&s[i]` forms a range reference over it, and that
  reference is subscripted, re-ranged and measured as any range reference
  is. The segment subscript is the last suffix of its borrow.
- `s.all` is the run of every element in segment order, over which
  `&s.all` forms a range reference, so the joined output reaches a consumer
  without a copy.
- Two segment subscripts with distinct offsets select distinct storage, as
  two array subscripts do, and [PAR-2] lists `Segments` beside `Array` and
  `Slots` in its element family, so `fill(window: &s.inner[i])` in a counted
  loop is permitted by Part 1's rule with no rule of its own.

The kernel owns the shape because the disjointness of its segments is a
fact about storage only the storage can establish, as a window's length
is [`language/data-model/kernel-minimality`].

## Rejected alternatives

- **An offsets witness type with its own permission rule** (the first
  proposal): an opaque type proving that a range of offsets does not
  decrease, and a [PAR-2] clause admitting windows between consecutive
  offsets. Rejected because it adds a type whose only purpose is to carry a
  proof and a permission clause specific to it, where segmented storage
  gets disjointness from the storage shape the element rule already trusts.
- **A general proved fact that an array does not decrease**: rejected
  because it needs quantified facts in automatic derivation, whose cost and
  termination the specification fixes; the property is needed only for
  boundaries that storage can own.
- **A loop that peels a window from the front of the remaining output each
  iteration**: disjoint by construction, but each window's position is a
  running sum, so the compiler would need a scan-shaped lowering to run the
  iterations apart, and it is no more general than segmented storage.
- **A buffer per item**: measured above; its allocations cost five to
  eleven times the contiguous form and slow down with workers.
- **The halving recursion as the only form**: it runs as fast as the
  contiguous form and needs no language change, but each use writes a
  recursive helper with runtime guards that restate a disjointness the
  program's data already has; Snowghost's rule is to add the missing
  feature instead of bending the renderer around it.
- **A parallel scan in the language** (a loop that updates an accumulator
  and publishes each new value): rejected for now because the running sums
  a renderer needs are a few additions per item after the costly per-item
  work, which the grain policy would not split; reopen when a measured
  program has a scan on its critical path.
- **Part 1 limited to field writes, or to fields and element borrows**:
  rejected because per-item owned buffers (`a[i].text.inner`) are common in
  a renderer, and ownership already guarantees that everything below
  element i is element i's.

## Validation

- The three minimal examples: `field.wf`'s two denied loops and
  `scatter.wf`'s loop, rewritten over `Segments`, are permitted and split;
  the ledger shows it.
- `scatter/`: B rewritten as a plain counted fill loop over `Segments` runs
  no slower than the halving recursion at one and four workers.
- Conformance cases for each new admission and each preserved denial
  (overlapping maps, whole-root access beside mapped access, a segment
  subscript with a non-affine offset).

## Results

Measured with the implementation at mbbill/Whitefoot `f3090178`, on the
same machine under the check lock; each build returns the same result as
before.

- `field.wf`: all three loops are permitted and split, the field write and
  the element borrow included.
- `scatter/c.wf`: the fill loop over `&out.inner[i]` is permitted and split
  (`PAR split once loop ... independent map`).
- Timing, best of seven, three alternating rounds, seconds:

  | Round | B, 1 worker | B, 4 workers | C, 1 worker | C, 4 workers |
  |---|---:|---:|---:|---:|
  | 1 | 0.184 | 0.107 | 0.156 | 0.106 |
  | 2 | 0.179 | 0.108 | 0.158 | 0.105 |
  | 3 | 0.178 | 0.106 | 0.158 | 0.104 |

  C is about 12 percent faster than the halving recursion at one worker and
  within one percent at four, meeting the criterion. A first build of C
  summed the lengths twice, once to judge the size limit and once to
  allocate, and was 6 percent slower than B at four workers in two of
  three rounds (0.114 against 0.107); summing once removed the gap, so the
  sequential passes over the lengths are on the critical path at four
  workers and a construction should make one pass for the total and one
  for the bounds.
- Conformance cases: each new admission (`type9-pos-segments-fill-and-join`,
  `fn9-pos-segments-routed-count`, `eff5-pos-segments-distinct-segments`,
  `op13-pos-segments-size-predicate`, `par2-pos-element-subtree-writes`)
  and each preserved refusal (`eff5-neg-segments-all-overlaps-segment`,
  `op4-neg-segments-element-subscript`, `op4-neg-segments-index-past-count`,
  `type9-neg-segments-parameter`, `type9-neg-segments-move-content`,
  `stor8-neg-no-heap-segments-filled`). Overlapping maps, whole-root access
  beside a mapped access and a non-affine segment offset are denials of
  permission rather than of acceptance, so compiler tests pin them
  (`compiler/src/semantic/tests/loop_permission.rs`).

