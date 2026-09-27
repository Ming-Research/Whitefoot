# Where the speed comes from

The proofs that make a Whitefoot program safe are also information about
it: which operations cannot fail, which memory each call touches, and which
calls are independent. The compiler passes that information on to the
machine code in a few specific ways, and each of them is listed here, with
the LLVM IR it produces. The last section lists what is not used yet.

The first use is the language's own rule: an operation that has been proved
carries no check at run time. The facts after it are optional. Leaving one
out would change neither which programs are accepted nor what they compute,
only how fast the code may run, and none of them adds a check at run time.

## 1. Checks that are not emitted

An operation whose goal is proved compiles to the bare instruction. Each row
is the body of a function whose requirement proves the goal:

| Source | Proved | LLVM IR |
|---|---|---|
| `deref(b)[i]` | `i < deref(b).len` | `getelementptr inbounds` and `load` |
| `total / count` | `count != 0_u64` | `udiv i64 %v0, %v1` |
| `cvt::<u64, u8>(x)` | `x < 256_u64` | `trunc i64 %v0 to i8` |
| `ishl(x, k)` on `u64` | `k < 64_u32` | `shl i64 %v0, %t0` |

In LLVM, `udiv` by zero is undefined behavior and `shl` by the width or more
gives poison, so these instructions are only correct where the excluded
inputs cannot reach them. C emits the same instructions and leaves that to
the programmer; a language that checks at run time emits a comparison and a
branch in front of them. Here the proof is the reason no branch is needed.
[Prove it or write a branch](prove-it-or-write-a-branch.md) shows how such
goals are proved.

## 2. Facts attached to the IR

**No-wrap flags.** A proved `+`, `-` or `*` carries LLVM's `nuw` for an
unsigned type or `nsw` for a signed one: `add nuw i64 %v0, %v1`. The flag
promises that the operation does not overflow, which the optimizer may use to
simplify the code around it. `+wrap` and the other forms carry no flag,
because overflow is part of their meaning ([Integers](integers.md)).

**Reference parameters.** This function writes one field through a
reference:

```
fn deposit(to: &Account, amount: u64) -> result: unit writes(to.balance) {
  let t = deref(to).balance;
  set deref(to).balance = t +wrap amount;
  return unit;
}
```

Its parameter reaches LLVM with four attributes:

```text
define i8 @wf_deposit(ptr noalias nonnull nocapture dereferenceable(16) %v0, i64 %v1) #0 {
```

- `noalias`, C's `restrict`: during the call, the memory the function writes
  through this pointer is reached through no other pointer. The compiler
  accepts a call only when it has proved that what one argument writes, no
  other argument reaches ([What a reviewer reads](what-a-reviewer-reads.md)).
  The exception is `swap`, whose two arguments may be the same place, and
  its parameters do not get the attribute.
- `nonnull`: a reference always names storage that exists.
- `nocapture`: a reference cannot be stored or returned, so no copy of the
  pointer outlives the call.
- `dereferenceable(16)`: the referent is an `Account`, 16 bytes on this
  target, so its bytes may be loaded early.

**Ring indices.** A subscript of a `Ring` selects slot `(head + i) mod cap`.
After computing that slot, the compiler states with `llvm.assume` that it is
nonnegative as a signed number, which the checked layout guarantees. A
`Slots` subscript goes through the same projection. This fact is kept
provisionally: in the double-ended queue measurement behind it, it helped on
one path, and a cost on another path is not yet explained.

## 3. A loop the vectorizer widens without a runtime check

This loop adds one array into another:

```
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

A range reference such as `&[u32]` is a pointer and a length. The compiler
passes the two as separate parameters, so that the pointer can carry the
same attributes as any reference, except `dereferenceable`, since the length
may be zero:

```text
define i8 @wf_add_into(ptr noalias nonnull nocapture %wf.arg.v0.data, i64 %wf.arg.v0.len, ptr noalias nonnull nocapture %wf.arg.v1.data, i64 %wf.arg.v1.len) #0 {
```

After `clang -O2`, the loop is vectorized, and there is no runtime check
that `dst` and `src` overlap. The same loop written in C gets that check,
unless the programmer writes `restrict`:

| Version | Overlap check before the vector loop | x86-64 instructions |
|---|---|---|
| Whitefoot | no | 29 |
| C | yes, `vector.memcheck` | 53 |
| C with `restrict` on both pointers | no | 28 |

The Whitefoot function is the C `restrict` function instruction for
instruction, plus one that sets the `unit` result. The difference is what
happens when the promise is broken. In C, passing overlapping arrays to the `restrict` version is
undefined behavior. In Whitefoot, the call is rejected:

```text
overlap.wf:20:3: error[EFF-5]: UndischargedCallSeparation
  source:   add_into(dst: dst, src: src);
  marker:   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  residual: x[4_u64..12_u64] and x[0_u64..8_u64] select different storage (one ends before the other starts, or one is empty)
  mechanical_fix: when the two ranges can lie apart here, prove before this call that one ends at or before the other starts, or that one is empty; otherwise pass ranges this call proves apart
```

Here `dst` is `&x[0_u64..8_u64]` and `src` is `&x[4_u64..12_u64]`. With
`src` as `&x[8_u64..16_u64]`, two halves of one array, the call is accepted
and computes the expected sums.

## 4. Independent calls run at the same time

The effect rows that give `noalias` also say which calls are independent.
Compiled with `--par`, calls that the compiler has proved independent, and
loops whose iterations are, may run on several workers, and the result is
the one the sequential program computes. The README's quicksort sorts 2
million numbers in 0.18 s sequentially and 0.07 s on four workers, the best
of seven runs each
([measurement](../../research/experiments/par-quicksort/README.md)).
[Write sequential code, get parallel
results](sequential-code-parallel-results.md) follows the compiler from the
rows to the parallel code.

## 5. Independent I/O is issued together

The same rows decide which I/O calls may overlap. A program's I/O calls are
ordinary calls in sequential code, and the compiled program issues
independent ones together through a completion runtime (io_uring on Linux,
I/O completion ports on Windows). `--no-overlap` turns this off, to measure
it. This part is still in progress; the
[README](../../README.md#in-progress) describes where it stands.

## 6. What is not used yet

- **Memory effects.** A row with only `reads` entries does not become
  LLVM's `memory(argmem: read)`, because the row does not cover everything
  that attribute promises: reads of constant storage and allocation are not
  in the row.
- **Requirements.** A `requires` is proved at every call, but the callee's
  code does not tell LLVM about it. `llvm.assume` carries only the Ring and
  Slots fact above, because each fact needs its own complete mapping to
  LLVM's meaning before it is emitted.
- **Termination.** `pure` does not become `willreturn`, because termination
  is not proved.
- **Split loops.** When `--par` splits a loop into chunks, the chunks the
  compiler synthesizes have no source signature. A chunk inlined into its
  function keeps that function's attributes; a chunk that runs on another
  worker gets none for its ranges.

Ideas for further facts, and how to test that one pays for itself, are in
[docs/ideas.md](../ideas.md#proof-derived-optimizer-facts).

## The rules behind each step

| Step | Rule in the [specification](../../spec/kernel-spec.md) or design |
|---|---|
| A proved operation lowers to the bare instruction | OP-2, OP-4, OP-6, OP-8 |
| Proved facts may reach the backend; none adds a check | DIAG-2 |
| Arguments' writes proved apart at each call | EFF-5 |
| References are neither stored nor returned | REF-3 |
| Which facts are emitted, and why | [`design/compiler/backend-facts.md`](../../design/compiler/backend-facts.md) |
| Range references as pointer and count | [range-reference investigation](../../research/investigations/range-reference-facts/DESIGN.md) |
| Parallel calls and loops | PAR-1, PAR-2 |

The IR in sections 1 to 3 is `whitefootc --emit-llvm` output, before
optimization, from the compiler at commit `e041772d8`. The fragments are
compiled in one file with `Account` declared as in
[What a reviewer reads](what-a-reviewer-reads.md), the `ExitStatus` and
`exit_status` aliases, and a `main`. The Ring fact is in the IR of
[`tests/programs/runtime_ring_wrap.wf`](../../tests/programs/runtime_ring_wrap.wf).
The table in section 3 counts the instructions of each function, from its
label to its end label, after `clang -O2 -S` with clang 18.1.3 on x86-64
Linux, without labels, directives or comments; the C versions are the loop
above over `uint32_t *` pointers and a `size_t` length. The rejected call
is compiled after the aliases and `add_into`, in a `main` whose first lines
are `let x = array_filled::<u32, 16>(value: 1_u32);`,
`let dst = &x[0_u64..8_u64];` and `let src = &x[4_u64..12_u64];`. The
accepted one, with `src` as `&x[8_u64..16_u64]`, was also run, and its sums
were checked.
