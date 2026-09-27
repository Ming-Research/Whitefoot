# Range-reference alias facts

## Question and scope

compiler/backend-facts already supplies the reference facts `noalias`,
`nonnull`, no-capture and `dereferenceable` to every `&T` parameter, from the
call-site disjointness [EFF-5] proves. A `&[T]` range reference [REF-4] is a
reference kind too, but it was passed as one `{ ptr, i64 }` aggregate, and an
LLVM aggregate parameter cannot carry a pointer attribute. The emitter
therefore stated nothing for it. In a loop that writes one range and reads
another, the host vectorizer then inserted a runtime overlap check and a
scalar fallback, the code unannotated C gets, although no admitted call can
pass overlapping arguments there.

This investigation establishes which fact the checked program proves for a
range-reference parameter, what LLVM's `noalias` requires, and which lowering
states that fact. The baseline is main at `0f22b026`.

## The proved fact

At a call, [EFF-5] substitutes the actual paths into the callee's row and
rejects any pair of overlapping substituted effects [OWN-7] of which one is a
write, unless the paths are proved disjoint; `swap` [OP-11] is the only
operation whose two arguments may name one place. Range steps are disjoint
only when one captured range ends at or before the other starts, or one is
empty [OWN-7]. [EFF-2] checks each row against the complete body in both
directions, so the callee reaches caller storage only through its reference
parameters, under entries of its row, and a callee of the callee only through
paths its own row covers. [OWN-9] states the consequence: a place the call
writes is unaliased by any other access path for the call's duration, and a
place it only reads is read-only for that duration. Parallel lanes run
concurrently only under [PAR-1, PAR-2] independence, which excludes
concurrent conflicting accesses in the same way.

LLVM's `noalias` on a pointer argument says that memory accessed through
pointers based on the argument is not also accessed, during the call, through
pointers not based on it; the guarantee concerns only memory modified during
the call. For a range-reference parameter:

- A written range is disjoint from every other substituted path, so its
  modified elements are reached only through its own pointer.
- A read-only range may overlap another read-only range or reference. No
  admitted access modifies that memory during the call, so `noalias` makes no
  claim about it.
- A parameter absent from the row is never accessed by the body [EFF-2].
  Its overlap with a written argument is admitted [EFF-5], and `noalias`
  holds because nothing is accessed through it.
- A range the callee forms from its own parameter, `&deref(p)[a..b]`, and a
  range it passes to a further callee are derived from that parameter's
  pointer, so their accesses are based on it.
- A self-tail transfer rebinds the parameters inside one activation, and
  every reference argument of the transfer is rooted at a reference parameter
  of the current function [FN-10]. The row covers the transfer's substituted
  effects [EFF-2], so the entering call's EFF-5 check already separates every
  path the whole activation writes, and each modified location is reached
  only through pointers derived from one incoming parameter.
- `swap` [OP-11] takes `&T` references, never ranges; the existing exception
  keeps `noalias` off its two parameters.

The element pointer addresses storage that exists while the reference is
valid [REF-2], including an empty range's position inside or one past its
origin, so it is `nonnull`. [REF-3] forbids returning or storing a reference,
so no copy of the pointer outlives the call. The range's extent is its runtime
`len`, which may be zero, so no `dereferenceable` extent is guaranteed.
DIAG-2 permits supplying these facts as target attributes, and
compiler/backend-facts already selects attribute supply for references.

## Selection criterion

The criterion was fixed from the task before implementation, while the
alternatives were prototyped:

1. Soundness for every call EFF-5 admits, including two disjoint ranges of
   one array and overlapping read-only ranges.
2. The runtime overlap check disappears from the optimized kernel below.
3. No machine calling-convention change for linked definitions and hosts,
   so the one callable ABI of the compiler root decision is kept.
4. Of the options satisfying 1 to 3, the one with the least emitter
   machinery.

## Alternatives

The kernel is the `add_into` function of the maintained test
`range_reference_parameters_state_the_call_site_disjointness_fact` in
`compiler/src/backend/tests/ranges.rs`:

```wf
fn add_into(dst: &[u32], src: &[u32]) -> result: unit reads(src), writes(dst) contract {
  requires deref(dst).len <= deref(src).len;
} {
  let n = deref(dst).len;
  for (i in 0_u64..n) {
    let a = deref(dst)[i];
    let b = deref(src)[i];
    set deref(dst)[i] = a +wrap b;
  }
  return unit;
}
```

Each prototype edited the baseline's emitted LLVM for this function and was
lowered with `clang -O2 -S`, the shipped host optimization, on x86-64 Linux
with clang 18.1.3. Line counts exclude directives.

| Lowering | `add_into` asm lines | Runtime overlap check | Sound for every admitted call |
|---|---:|---|---|
| Baseline: one `{ ptr, i64 }` aggregate, no fact | 64 | yes | yes |
| Split pointer and count, facts on the pointer | 35 | no | yes |
| Aggregate plus scoped `!alias.scope`/`!noalias` metadata | 35 | no | only with per-activation scopes |
| Aggregate plus `llvm.assume` `"separate_storage"` | 35 | no | no |

- **Split parameters.** The pointer carries `noalias nonnull` and the
  build's no-capture spelling; the count carries nothing. The body
  reassembles its ordinary pair at entry, so the range representation of
  compiler/storage-representation is unchanged inside bodies.
- **Scoped metadata.** Every load and store through a range parameter would
  need that parameter's scope, which requires the emitter to track the
  provenance of every access, a metadata table the emitter does not have,
  and `llvm.experimental.noalias.scope.decl` at entry so inlining and
  unrolling cannot merge the scopes of two activations. The parameter
  attribute states the same fact once, and LLVM's inliner converts it to
  scoped metadata itself.
- **`separate_storage`.** The operand bundle asserts that the two pointers
  have different underlying objects. Two disjoint ranges of one array
  [OWN-7] share one object, so the assumption would be false for an admitted
  call. It is also pairwise, where the attribute is per parameter.
- **Wrapper.** An internal split-parameter body behind an aggregate-taking
  wrapper doubles every range-taking symbol and depends on inlining for the
  fact, while the split itself changes no machine ABI.

## Machine calling convention

LLVM lowers a first-class aggregate argument by passing each of its elements
as an independent argument. A caller and a callee passing five `i64`
arguments, two ranges, an `i64` and a third range were compiled with
`llc -O2` 18.1.3 for each target `TargetLayout::for_triple` admits, once with
aggregate ranges and once with split ranges. The machine code was identical
for `x86_64-unknown-linux-gnu`, `x86_64-pc-windows-msvc`,
`x86_64-apple-darwin`, `aarch64-unknown-linux-gnu` and
`aarch64-apple-darwin`, including the System V case where one range is split
between the last register and the stack. A separately compiled object that
still passes the aggregate therefore links compatibly, but LLVM text in the
same module as WF code must spell the split form: the linked definitions in
`compiler/src/backend/ordinary_values.ll`, the formal compute host adapters
and the raw-deflate test adapter were updated.

## Result

With the split lowering, `add_into` has 35 instruction lines and no overlap
check. Its optimized LLVM keeps the `vector.body` loop and loses the
`vector.memcheck` block the baseline had. The emitted head is

```llvm
define i8 @wf_add_into(ptr noalias nonnull nocapture %wf.arg.v0.data, i64 %wf.arg.v0.len, ptr noalias nonnull nocapture %wf.arg.v1.data, i64 %wf.arg.v1.len)
```

The same source with overlapping ranges `&x[0..8]` and `&x[4..12]` is
rejected with EFF-5; with disjoint ranges `&x[0..8]` and `&x[8..16]` of one
array it is accepted and computes the expected values.

## Limits

- Synthesized loop-split chunks and splitters have no source signature, so
  their range captures receive no facts. In the sequential world the chunk is
  inlined into its source function and inherits that function's facts; a
  chunk run as a worker lane does not. Supplying facts there needs its own
  derivation from PAR-2 independence and the enclosing call's EFF-5 result.
- The experiment-private adapters under `research/experiments/compute-bench/`
  (`array_reference_host.ll`, `first_index_host.ll`, `dag_fanin_host.ll`) and
  the first-index observation rewrite still spell the aggregate form. They
  are dated research inputs and were not changed; they need the split form
  before those experiments run with a compiler that includes this change.
