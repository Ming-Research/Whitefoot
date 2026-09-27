# One build

Many languages build two programs from one source. In Rust, integer overflow
panics in a debug build and wraps in a release build. In C, an `assert` runs
until `NDEBUG` removes it. The program that was tested is then not quite the
program that ships.

Whitefoot builds one program. There is no check that a flag adds or removes,
because the program has no checks to toggle: every operation that could fail
is proved before the program is accepted. What remains is the one way a
correct program can still stop early, which is running out of a resource,
and that stop is fixed too. This article covers both.

## 1. Nothing to switch off

The language has no `assert`, no panic, no exceptions and no unwinding. An
index, an integer operation, a division and a narrowing conversion are each
proved before the program is accepted, or written as a branch the program
handles ([Prove it or write a branch](prove-it-or-write-a-branch.md)). A
`+wrap` wraps and a proved `+` adds, whatever the build.

The compiler always asks clang for the same optimization level, `-O2`, and
it has no option that changes what a program computes. Two options change
how it runs:

- `--par` runs calls in parallel where the compiler has proved them
  independent, and the result is the one the sequential program computes.
- `--no-overlap` issues I/O one call at a time. It exists to measure the
  default, which overlaps independent calls.

## 2. The one early stop: resources

The specification leaves resource availability outside the program's
meaning ([SCOPE-3](../../spec/kernel-spec.md#1-scope-and-conformance)):
heap exhaustion, stack exhaustion, operating-system quotas and the resources
a program needs to start may stop it, without a value, a status or cleanup.
Nothing in the source was wrong when that happens, so the compiler reports
it as what it is: a fixed record that names the resource and nothing else.

## 3. The stack

This recursion does a little work after each call returns, so LLVM cannot
turn it into a loop:

```
fn walk(n: u64, x: u64) -> result: u64 pure {
  if n == 0_u64 {
    return x;
  }
  let m = n - 1_u64;
  let y = x *wrap 3_u64;
  let d = walk(n: m, x: y);
  let e = d *wrap x;
  return e +wrap 7_u64;
}
```

`--stack-ledger` reports, at compile time, how deep it can go:

```text
STACK stack     1073741824 B  the entry thread and every worker lane
STACK frame     wf_walk                                          16 B  static
STACK frame     wf_main                                          16 B  static
STACK frame     wf__floor_run                                     8 B  static
STACK frame     wf__main_body                                    48 B  static
STACK frame     main                                              8 B  static
STACK cycle     wf_walk                                          16 B/level  67108864 levels
STACK chain     wf_main                                          16 B  wf_main
STACK chain     main                                             64 B  main -> wf__floor_run -> wf__main_body
```

The program runs on a 1 GiB stack, and each level of `walk` takes 16 bytes,
so about 67 million levels fit. The frame sizes come from compiling the
program, so they are what the machine code actually uses. Called from `main`
with `n` of 60 million, the program finishes normally. With 100 million, it
writes one line to standard error and stops:

```text
{"resource":"stack"}
```

The process ends with an abort, exit status 134 on Linux. The record is the
same on every run that exhausts the stack. It names the resource and nothing
else: no function, depth or address, because it reports a machine limit,
not a mistake in a function.

A recursion that ends in a call to itself can be written so that it does not
grow the stack at all. `musttail` requires the call to reuse the current
frame:

```
fn count(n: u64, acc: u64) -> result: u64 pure {
  if n == 0_u64 {
    return acc;
  }
  let m = n - 1_u64;
  let a = acc *wrap 3_u64;
  let b = a +wrap 7_u64;
  return musttail count(n: m, acc: b);
}
```

This runs to 100 million levels in the same 1 GiB, and its ledger shows no
cycle. The compiler rejects `musttail` on a call that cannot be made this way,
for example one to a different function.

## 4. The heap

Allocation is total in the source: `box_array_filled::<u8>(count: n, value: 0_u8)`
returns a `Box`, not a `Result`
([STOR-8](../../spec/kernel-spec.md#6-storage)). When the host refuses an
allocation, the program writes this line to standard error and aborts:

```text
{"resource":"heap"}
```

On Linux, where memory is usually overcommitted, the kernel's OOM killer may
end the process before any allocation is refused, and then no record is
written. A program that must not depend on the heap can declare
`program no_heap;`, and the compiler then rejects every heap type and every
allocating call in the code it runs.

## 5. What the record is not

- **Not a proof failure.** It carries no rule, function or source location,
  so it cannot be mistaken for one. Every proof failure is reported at
  compile time.
- **Not language-defined bytes.** The specification says only that the
  program may stop. The two records and the abort are this compiler's
  choices, fixed so that a script or an agent can recognize them.
- **Not a refused offer.** When `--par` finds no idle worker for a call, the
  call runs on the calling thread. That is ordinary scheduling, not an
  exhausted resource, and it writes nothing.

## 6. Limits

- The stack is reported, not yet proved. The ledger shows how many levels
  fit, but the compiler does not reject a call that could go deeper.
- Loops and recursion are not proved to terminate. A program can still run
  forever.
- Both are part of the planned maximum-safety mode described in the
  [README](../../README.md#beyond-memory-resources).

## The rules behind each step

| Step | Rule in the [specification](../../spec/kernel-spec.md) or design |
|---|---|
| Resource exhaustion may stop the program, outside its meaning | SCOPE-3 |
| One heap; allocation is total; `no_heap` | STOR-8 |
| `musttail` reuses the frame | FN-10 |
| The records, stack probing and the ledger's numbers | [`design/compiler/resource-exhaustion-floor.md`](../../design/compiler/resource-exhaustion-floor.md) |

Every program in this article is compiled by the compiler at commit
`e041772d8`. The ledger is `whitefootc --stack-ledger` output, and the runs
were made on x86-64 Linux; the fragments are compiled inside a file with the
standard `ExitStatus` aliases and a `main` that calls them.
