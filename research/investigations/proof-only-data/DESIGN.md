# Data only proofs read, without run-time cost

## Question

A range requirement can name only what its function receives, and a fact a
caller proves about storage holds only of storage the caller computed. So a
program that proves distinctness through a left inverse allocates, fills and
passes an array no executable statement reads
([layout friction, item 4](../layout-friction/DESIGN.md#4-parameters-only-a-contract-reads)).
The owner asked for a way to avoid that run-time cost that fits the language
well. This record compares the ways, measures what the compiler and its
optimizer already remove, and states one rule in enough detail to judge it.
The owner's ruling on the comparison and a measurement of the cost follow
it.

## Witnesses

The minimal semantic witness is the conformance case
[`range5-pos-scatter-through-left-inverse.wf`](../../../tests/conformance/cases/range5-pos-scatter-through-left-inverse.wf):

```wf
fn scatter(order: &[u64], pos: &[u64], out: &[u64]) -> result: unit reads(order), writes(out) contract {
  requires pos^.len == out^.len;
  requires forall inv(k in 0_u64..order^.len) when order^[k] < out^.len: pos^[order^[k]] == k;
} { ... }
```

`scatter`'s body never reads `pos`; `reversed`, its caller, allocates `pos`,
writes one element per iteration of the loop that builds `order`, and passes
it, because `inv` is the only form in which RANGE-1's pointwise clauses state
that `order` names distinct slots.

The program witness is Snowghost's style cascade at `8b4f332`
(`renderer/proto/style/shapes.wf`). `level_index` builds `positions`, one
`u64` per element, inside the loop that fills the levels; it returns it in a
field of `LevelIndex` whose postconditions state the `listed` fact;
`cascade_levels` passes `&levels.positions.inner[0..n]` to `cascade_level` once
per level. `depths` is computed for bucketing anyway but is kept in the same
struct only for the `up` requirement, and `level` is passed only for `listed`.
So data only a proof reads crosses a local, a struct field, a function result
and a parameter.

## What the compiler and the optimizer remove today

The compiler at `77de43b53` passes every parameter and lowers every
allocation and store. Whether LLVM then removes the proof-only array depends
on how the program happens to be shaped (each binary built by `whitefootc`
with its default optimization, then disassembled):

| Program | Proof-only data | Observed in the optimized binary |
|---|---|---|
| `range5-pos-level-cascade.wf` | `positions`, local to `cascade`, passed to `cascade_level` | `cascade_level` is inlined and `positions` is gone: `wf_cascade` makes three `malloc` calls for its four arrays, the missing one sized by the element count |
| `range5-pos-scatter-through-left-inverse.wf` | `pos`, local to `reversed`, passed to `scatter` | `scatter` is inlined, yet the filling loop still stores to both `order` and `pos` |
| Snowghost `proto_style` at `8b4f332` | `positions` in `LevelIndex` | `level_index` makes four `malloc` calls, `positions` among them; `cascade_levels` calls `cascade_level` out of line and frees `positions` after the last level |

Removal is an accident of inlining and of LLVM's allocation analysis, not a
property a writer can rely on, and nothing removes data that leaves its
function in a struct. The cost that survives in Snowghost is one allocation,
a fill and one store per element, held across every level, and five machine
arguments per level for `positions`, `depths` and `level`; it is timed
below, against a variant written without it.

## Criteria

Recorded before comparing the candidates:

1. **No run-time trace, by a rule.** The data and its computation are absent
   from the lowered program whatever the optimizer does.
2. **Sound by a local check.** Removing the data cannot change what the
   remaining program does, and the check that ensures this needs no search.
3. **No new proof machinery.** Facts about the data are the facts the engine
   already forms; no logical types, functions or lemmas.
4. **Signatures and layouts fixed by declarations.** A module's interface,
   ABI and struct layout follow from what is written, not from a body.
5. **Reach the program witness.** Local, field, result and parameter, since
   Snowghost's data crosses all four.

## Candidates

- **A. Proof-only declarations.** A parameter, `let` binding or struct field
  may be declared `proof`. Such a place is read only by proof syntax, by
  writes to proof places and by the actuals of proof parameters and fields;
  lowering removes it, with every statement that only writes it. Details
  below. Meets all five criteria.
- **B. Proof-only parameters alone** (layout friction 4B). Removes the
  passing but not the computation: Snowghost still allocates and fills
  `positions` and keeps it in `LevelIndex`. Fails 1 and 5.
- **C. An order fact in place of the inverse** (4A, with 4D). The levels are
  filled in increasing element order, so each segment is strictly
  increasing, which already implies distinctness. The callee can require it
  with two binders, `when a < b: slots^[a] < slots^[b]`, but the builder's
  invariant over every segment of a `Segments` needs three, `(d, a, b)`, and
  RANGE-1 admits two; raising the limit makes the instances per fact grow
  with the cube of the reads [RANGE-3]. It removes `positions` only where the
  data is ordered: a permutation computed any other way, as the scatter
  witness's reversal is, still needs its inverse. Fails 5 in general and
  costs the engine.
- **D. Leave it to the optimizer**, giving every non-entry function internal
  linkage so that dead-argument elimination can drop unused parameters. The
  table above already shows the allocation surviving inlining, and a struct
  field is never removed. Fails 1 and 5.
- **E. Infer proof-only data**: erase whatever no executable expression
  reads. The ABI of an exported function and the layout of a public struct
  would then change with a body, and an accidental executable read would put
  the cost back without a word. Fails 4 and gives no checked statement of
  intent.
- **F. A logical model** (the ghost-field candidate of
  [unique keys](../unique-keys/DESIGN.md#candidate-source-form-an-erased-field-tied-to-storage-by-an-invariant)):
  ghost types, sequences, logical functions and lemmas. It answers a larger
  question, abstract specifications of data structures, and needs the
  certificate layer that record sketches. Fails 3; nothing in these
  witnesses needs more than ordinary storage.

## Candidate A in detail

### Admission

A place is a *proof place* when its root is a proof parameter or proof
binding, or when it is reached through a proof field. An expression is
*proof-only* when it reads a proof place. A proof-only expression is
admitted only:

- inside proof syntax: contract clauses and `define`s [FN-8, FN-9], loop and
  local invariants with their `use` steps [INV-1, PRF-1], range clauses
  [RANGE-1] and apart certificates;
- as the value, subscript or condition of a *proof statement*: a `let proof`
  or a `set` whose target is a proof place;
- as the actual of a proof parameter or proof field.

Every other occurrence, including a branch condition, a subscript of an
executable place, a returned value, an argument of an ordinary parameter and
an allocation count, is a rejection under the new rule. Executable values may
flow into proof places freely.

A proof statement contains no call other than the compiler-owned
construction rows [OP-13]: it runs no loop of its own and calls no function
that might not return, so removing it cannot remove a non-terminating or
effectful computation. It sits inside executable control flow, as `set
positions.inner[at] = offset;` sits inside `level_index`'s loop. Its own
obligations, subscripts [OP-4], exact arithmetic [OP-2] and the rest, are
discharged as everywhere else, so the proof state is always defined.

### Checking and lowering

Every judgment treats a proof place as the place it is: rows name the proof
paths a body writes or reads through a reference, writes kill facts about
them [EFF-2], EFF-5 separates them at calls, ownership moves and releases
them. Only lowering differs: it drops proof parameters from the ABI, proof
fields from layouts, proof bindings, proof statements, and the arguments and
field values that fill proof places. A value an executable place owned and
a proof place takes over is released where it is taken over, since the
proof place does not exist at run time.

### Why removal is sound

The source program with its proof places is the meaning; the lowered program
is its executable part. No executable statement reads a proof place, so every
executable value is the same in both. A proof statement terminates, calls
nothing that writes, and traps nowhere, so removing it changes neither the
order nor the outcome of executable steps. The facts the checker derived are
facts about the source program and hold of every execution of it, so the
executable part, which behaves identically, keeps every guarantee they
authorized: the apart certificate's separation, the subscripts' bounds.

### Open points inside A

- **PAR-2.** Proof writes inside a loop body, like `level_index`'s, are writes
  of the source program, and the permission judgment would count them. Since
  no proof place exists at run time, the permission may ignore them; that
  needs its own argument and is not needed by either witness.
- **Proof results.** A function whose result is only a proof value is not
  needed here: the program witness returns a struct with a proof field.
- **Spelling.** `proof` names what the checker enforces, that only proofs read
  the place. Dafny and Verus write `ghost`, but their ghost code has logical
  types and functions with no run-time counterpart; here a proof place holds
  an ordinary Whitefoot value under every ordinary rule, so the borrowed name
  would promise a different feature. `proof` becomes a reserved word [FORM-3];
  no maintained program or library uses it as a name.

### The cascade under A

Proposed syntax, not accepted source:

```text
nocopy struct LevelIndex {
  slots: Box<Segments<u64>>;
  proof positions: Box<Array<u64>>;
  proof depths: Box<Array<u64>>;
}

fn cascade_level(slots: &[u64], proof positions: &[u64], proof depths: &[u64], ..., proof level: u64) -> ... {
  requires forall listed(k in 0_u64..slots^.len) when slots^[k] < out^.len: positions^[slots^[k]] == k, depths^[slots^[k]] == level;
  ...
}

// in level_index
      let proof positions = box_array_filled::<u64>(count: n, value: 0_u64);
      ...
            set segment^[offset] = at;
            set positions.inner[at] = offset;
```

`depths` stays an executable local while `level_index` buckets the elements
and becomes proof data when it moves into the struct, so its storage is
released there instead of after the last level. `cascade_levels` and the call
are unchanged in text; the three proof arguments disappear from the call.

## Validation if A is selected

- Conformance: the scatter and cascade witnesses with their proof data
  declared `proof` are accepted and run; each place where a proof-only
  expression meets an executable position is a rejection of the new rule,
  one case per position; a proof statement with a user call is rejected.
- Lowering: the emitted LLVM of both witnesses has no `pos`/`positions`
  allocation, stores or arguments, and the cascade's `cascade_level` takes
  three parameters, five machine arguments, fewer; this does not depend on
  optimization level.
- Program: Snowghost's cascade with `positions`, `depths` and `level` declared
  `proof` is accepted, its outputs are unchanged, and `level_index` makes
  three allocations instead of four.

## Ruling

The owner refused A after reading the witnesses written under it: with a
`proof` marking a writer must keep two kinds of every name apart, and the
flow between them, one way, refused wherever proof data would reach an
executable position, with no user call in a statement that writes proof
data, is hard to hold while writing. The data stays ordinary, and removing
its cost is the compiler's work if a measurement shows the cost worth a pass
(`design/language/checks-and-proofs.md`). The open question of a proof
counter (`docs/todo.md`, "A proof counter has no type without an overflow
obligation") asks for erased state that ordinary data cannot express; this
ruling's reason weighs on that question without deciding it.

## Finding the data in the compiler

Proofs are erased before lowering, so the data is what no executable
statement reads once they are gone; the checker already knows every read.
LLVM removes it only where inlining makes every use visible, as in the
cascade witness of the table above. Where the call stays out of line, as in
the Snowghost builds timed below, it cannot: the emitted functions keep
external linkage (`cascade_level` is a global symbol of both), so
dead-argument elimination does not change their signatures, and LLVM
removes an allocation only when no call uses the pointer. A full-LTO or
ThinLTO link could internalize the functions; that was not tried. A whole-program pass in the
Whitefoot compiler sees every call and needs:

- **A fixed point across calls and fields.** In the program witness a
  parameter `cascade_level` never reads makes its argument dead, the field
  read only for that argument dead, the field's construction and the stores
  to `positions` dead, and then the allocation. It is the least fixed point
  of liveness, so a parameter passed only to its own function's recursive
  call is dead, and it terminates, being monotone over finite sets.
- **No removed non-termination.** Removing a computation is sound only where
  it cannot trap, which the language guarantees, and terminates, which it
  does not: stores, allocations, borrows and pure arithmetic go, a counted
  loop goes only when its body is left empty, and a call or an uncounted
  loop stays, losing only its dead arguments. Removing an allocation removes
  only a possible heap exhaustion, which lies outside the source outcome
  model [SCOPE-3, STOR-8].
- **Fixed boundaries.** The entry function, functions the runtime calls back,
  any layout the host reads, and every function and struct a module
  publishes, which other modules compile against, keep their shape; a caller's compiled form
  depends on whether its callees read their parameters, which incremental
  and parallel lowering must account for.
- **A pinned result.** It is an optimization, not a rule, so compiler tests
  pin both witnesses' emitted code free of the data.

This is candidate E as an optimization rather than a rule. Criterion 4
still holds, since the boundaries include every published signature and
layout, and both witnesses' functions and Snowghost's `LevelIndex` are
private to their modules; criterion 1 weakens from
a rule to the pinned tests, which catch an executable read added later only
in the two witnesses, where elsewhere such a read keeps the data without a
word, a cost and never a change of meaning.

## Measurement

The criterion, recorded before measuring: the pass is worth designing only
if removing the data saves more than the run-to-run spread of both builds
and at least 1% of the style stage's time on at least one of the two
measured pages, ecma262 and html5.

The measured pair is Snowghost's style stage at `8b4f332`, sequential build,
against a variant whose source is what the pass would produce: no
`positions`, no `depths` field and no `level` parameter, and no apart
certificate in `cascade_level`, which only they served
([erased-variant.diff](erased-variant.diff)). The two builds differ in the
removed data and what removing it implies: `depths` is released when
`level_index` returns instead of with the index, and the code's layout
shifts. Both builds, by Snowghost's pinned compiler `5fc912d9`, give the same
checksums on both pages and on shapes C, D and `cascade-d`
([timing-2.txt](timing-2.txt) lists them).

Three shapes of the `proto_style` driver were timed: `D`, the style stage
(matching and the level cascade); `cascade-d`, the level cascade alone,
which holds all of the removed work; and `C`, matching and the flat cascade,
which calls neither changed function. A trial's figure is the least of three
runs at REPS repetitions less the least of three at none, divided by REPS,
the method of Snowghost's concurrency harness, on a 4-processor Linux x86_64
host under the check lock. The pages and sheets are those Snowghost's
concurrency harness fetches (`research/investigations/concurrency/run.sh
fetch`), so the timing reruns from the diff, the pinned compiler and this
method. Seconds per repetition; a paired difference is
the baseline's figure less the variant's in one trial, so a positive one is
a saving.

**First timing** ([timing.txt](timing.txt)): five trials, the baseline first
in each. Median and range:

| Page | Shape | Baseline | Without the data | Paired differences |
|---|---|---|---|---|
| ecma262 | D | 0.6633 (0.6033–0.6767) | 0.6333 (0.6133–0.6700) | +0.0067 −0.0200 −0.0067 +0.0067 +0.0433 |
| ecma262 | cascade-d | 0.0467 (0.0413–0.0505) | 0.0485 (0.0432–0.0515) | +0.0010 −0.0072 −0.0037 +0.0035 −0.0035 |
| html5 | D | 0.5233 (0.5067–0.5233) | 0.5033 (0.4867–0.5100) | +0.0367 +0.0200 +0.0200 +0.0133 +0.0033 |
| html5 | cascade-d | 0.0118 (0.0107–0.0142) | 0.0118 (0.0113–0.0138) | −0.0018 −0.0007 +0.0000 +0.0023 −0.0005 |

Applied as written to the style stage, `D`, the criterion is not met on
either page: the medians differ by less than either build's range. Seen
after the data, this application could not have come out otherwise. The whole
level cascade, which bounds what the data can cost, takes 0.047 s on
ecma262 and 0.012 s on html5, less than `D`'s ranges, so `D` cannot resolve
it; only `cascade-d` can. The criterion also fixed neither the shape nor
the number of trials, and html5's `D` difference, positive in all five
trials, could be an order effect, since the baseline always ran first.

**Second timing** ([timing-2.txt](timing-2.txt)), designed after the first
timing's data were seen, so exploratory evidence beside the criterion, not
a test of it. Its shapes, fifteen trials and alternating order were fixed
before it ran and its 1% threshold is the criterion's; the rule applied to
it was chosen while analysing it: a saving is excluded when the 96%
distribution-free interval of the median paired difference, the fourth to
the twelfth of the fifteen ordered differences, lies below 1% of the stage.
It timed four page and shape pairs, the level
cascade on both pages and the stage with the control `C` on html5;
ecma262's stage and control were not retimed. The baseline ran first in
even trials and the variant in odd ones:

| Page | Shape | Median paired difference | 96% interval |
|---|---|---|---|
| ecma262 | cascade-d | −0.0025 | −0.0060 to +0.0043 |
| html5 | cascade-d | −0.0015 | −0.0023 to −0.0002 |
| html5 | D | +0.0133 | −0.0200 to +0.0233 |
| html5 | C | −0.0033 | −0.0333 to +0.0233 |

On ecma262 the level cascade's median saving is at most 0.00425 s per
repetition at 96% confidence, 0.64% of the stage's 0.6633 s from the first
timing, since the second did not retime ecma262's stage; on html5 the whole
interval is negative, the variant slightly slower. By the rule above a
saving of 1% of the sequential stage is excluded on both pages. The stage
itself, `D`, cannot separate html5's difference from variation, whose width
the control `C` shows; the level cascade, which holds the removed work,
shows none, which bounds what that work contributes unless removing it
changed the cache or allocator behaviour of the matching around it, which
`D` cannot resolve. No pass is designed now.

## Limitations

The admission rule of A was stated, never implemented. The pass is
sketched, not designed; its interaction with incremental and parallel
lowering is the first question a design would answer. Only the sequential
build was timed, on two pages. The `--par` build cannot be timed this way,
since the variant drops the certificate that permits each level's loop to
run in parallel; only the pass itself could be timed there. Snowghost's
concurrency record puts `cascade-d` at 0.0340 of `D`'s 0.2367 s on ecma262
at four workers, against 0.0565 of 0.7033 s sequentially, so in a parallel
run the level cascade's share, and with it the sequential `level_index`
fill, is about twice as large. Those figures come from that record's run 28
at Snowghost `5a01982`, the best of five runs, not from this record's
method; its sequential `cascade-d` is about a quarter above this record's,
so the ratio is indicative only. Reopen with a program whose profile puts
data only proofs read on its critical path.
