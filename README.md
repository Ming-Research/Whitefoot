# Whitefoot

Whitefoot is a research systems programming language built around three
properties:

- **Safe.** A program the compiler accepts has no undefined behavior, as long
  as the software it relies on is correct: the compiler, LLVM, the runtime
  and the operating system, among others listed below. It cannot panic, and
  no bounds, overflow or conversion check runs in it. There is no `unsafe` to
  opt out with, and a program can declare that it uses no heap at all.
- **Fast.** The safety comes from proofs checked at compile time, not from
  checks at run time, and the same proofs let the compiler drop bounds and
  overflow checks, tell LLVM which references do not alias, and run
  independent code in parallel.
- **Small.** Functions, structs, enums and explicit generics, close to C. No
  lifetimes, no methods, no traits, no exceptions.

The compiler finds most proofs itself, with a fixed procedure rather than an
SMT solver, the automatic theorem prover behind tools such as SPARK, Dafny and
Verus. No timeout or work budget takes part in the decision, so two machines
never disagree about whether a program is accepted
([ENT-1](spec/kernel-spec.md#15-obligation-discharge-deterministic-facts-invariants-and-local-certificates-normative)).
You write the rest as loop invariants and, now and then, a short proof step,
and the compiler checks them.

## Safe: no undefined behavior, no panics, no failing checks

Every operation that could go wrong at run time, such as an index, an integer
operation, a narrowing conversion, a division or an allocation size, must be
proved in range before the program is accepted. The language has no
`unsafe`, no panic, no exceptions and no unwinding; an expected failure is a
value (`Result`, `Option`) the caller handles. When the trusted base is
correct, an accepted program cannot:

- read or write out of bounds, use freed memory, or read uninitialized memory;
- overflow an integer silently. Each operation states its meaning (`+wrap`,
  `+checked`, `+sat`), and a bare `+` must be proved not to overflow;
- lose a value in a narrowing conversion, or divide by zero;
- race: parallel execution comes only from proved independence, and its
  result equals the sequential one;
- panic, abort, throw or unwind. The language has no such construct;
- behave differently between a debug and a release build. There is one build.

It still can:

- run out of stack. It then stops with the fixed record
  `{"resource":"stack"}`, the same way on every run, and `--stack-ledger`
  reports each function's frame and how many levels each recursive cycle fits;
- run out of heap. The allocator stops the program; on Linux with overcommit,
  the kernel's OOM killer may act first;
- loop forever, or compute the wrong answer. Contracts describe what was
  written down, not what was meant;
- be miscompiled. The trusted base is the Whitefoot compiler and its checker,
  LLVM and clang, the runtime and allocator, C functions linked in as trusted
  definitions, libc and the operating system
  ([SCOPE-3](spec/kernel-spec.md#1-scope-and-conformance)).

Other systems prove the same absence of runtime errors in other ways. SPARK,
a subset of Ada for high-integrity software, uses SMT solvers in an analysis
separate from compilation: the Ada compiler builds a program whether or not it
has been proved. Wuffs checks similar proofs without a solver, but it is a
language for libraries that parse, decode and encode file formats, and its
code cannot make system calls or allocate memory.

### Beyond memory: resources

Memory safety is where the proofs start, not where they stop. The aim is to
make Whitefoot as safe and robust as proofs can make a systems language,
enough to carry the most critical infrastructure, and the next step is the
program's resources: the memory it uses, its stack, its time and the devices
it drives.

Today:

- **The heap is optional.** A program that begins with `program no_heap;`,
  or an entry that a module program declares with `no_heap`, cannot allocate:
  the compiler rejects every heap type and every allocating call in the code
  that program or entry runs ([STOR-8](spec/kernel-spec.md#6-storage)).
- **Resources that must be released are linear.** A linear value cannot be
  copied, and the compiler never discards it on its own: the program has to
  pass it on or hand it to a function that consumes it. The standard
  library's files, directories, listeners and connection halves are linear,
  so each is closed exactly once, by an explicit call such as `close_read`,
  and code that could lose one is rejected.
- **The stack is reported, and tail calls do not grow it.** Running out of
  stack stops the program with the fixed record above, `--stack-ledger`
  reports each function's frame, and a self call marked `musttail` transfers
  without growing the stack.

Planned: a maximum-safety mode, for systems where a failure is not
acceptable. A program compiled in that mode would have:

- no heap and no other dynamic resource;
- a peak stack proved to fit a capacity given in bytes;
- every loop and every recursion proved to finish;
- hardware peripherals mapped as linear values, so that a device is owned,
  used and released under the same proofs as a file;
- no parallelism scheduled at run time;
- proved bounds on how long each peripheral takes to respond and how long the
  program takes to start.

None of this mode is implemented yet. The [fixed-resource
investigation](research/investigations/fixed-resource-execution/README.md)
records the design so far for the heap, the stack and termination, and what
each still needs; the peripheral, parallelism and timing parts are not
designed yet.

## Fast: the proofs pay for the speed

A proof that an operation is in range also makes its runtime check
unnecessary, and a proof that two pieces of code touch different memory lets
them run at the same time.

### A bounds check proved away

This loop keeps the non-space bytes of a buffer, in place. The line
`invariant behind: kept <= i` declares an invariant named `behind` that states
why the store `buf[kept]` is in range.
The compiler proves it before the first iteration and after every iteration,
and with `i < buf.len` concludes `kept < buf.len`, so the store compiles to a
plain store:

```
fn squeeze(buf: &[u8]) -> kept: u64 writes(buf) {
  let kept = 0_u64;
  for (
    i in 0_u64..deref(buf).len,
    invariant behind: kept <= i
  ) {
    let byte = deref(buf)[i];
    if byte != 32_u8 {
      set deref(buf)[kept] = byte;
      set kept = kept + 1_u64;
    }
  }
  return kept;
}
```

In each of the seven safe Rust spellings of this loop we measured (rustc
1.98.1, x86-64), the compiled loop keeps a runtime check of `kept`; the
measured spellings without one use `unsafe`, `retain` on an owned `Vec`, or a
second buffer
([measurements](research/experiments/bounds-check-spellings/README.md#results)).
Without the invariant, Whitefoot rejects the function and names the missing
fact, `kept < deref(buf).len`. [Proofs without a solver, by
hand](docs/articles/proofs-by-hand.md) follows the compiler through this
proof step by step.

### Sequential code, parallel results

Every function states what it reads and writes, `reads(...)` and
`writes(...)`, and the compiler checks the statement against the body. So it
knows what memory each call touches, and when it proves that two calls touch
different memory, it may run them at the same time. Nothing in this source
asks for parallelism:

```
fn quicksort(v: &[u64]) -> result: unit writes(v) {
  let n = deref(v).len;
  if n <= 1_u64 {
    return unit;
  }
  let p = partition(v: v);
  let after = p + 1_u64;
  let smaller = &deref(v)[0_u64..p];
  let larger = &deref(v)[after..n];
  quicksort(v: smaller);
  quicksort(v: larger);
  return unit;
}
```

Compiled with `--par`, the two recursive calls run in parallel down to a
depth derived from the number of workers, because the compiler proves that
`[0, p)` and `[p + 1, n)` do not overlap, and the result is the one the
sequential program computes. Sorting 2 million numbers took 0.18 s
sequentially and 0.07 s on 4 workers, the best of seven runs on a shared
machine ([measurement](research/experiments/par-quicksort/README.md));
`--par-ledger` prints every decision with its reason. In Rust, `rayon::join`
would run the two calls in parallel after a small change to the source, and
its types rule out data races. Here no line asks for parallelism, and the
compiler runs calls in parallel only when it has proved that the result equals
the sequential one.
[Write sequential code, get parallel
results](docs/articles/sequential-code-parallel-results.md) follows the
compiler from the checked rows to the parallel code.

### Other uses of the same proofs

- A proved `+` compiles to a plain add carrying LLVM's no-wrap flag (`nuw`
  unsigned, `nsw` signed), which the optimizer can use.
- Each reference parameter reaches LLVM as `noalias`, C's `restrict`, because
  the compiler accepts a call only when it has proved that what one argument
  writes, no other argument reaches. The exception is `swap`, whose two
  arguments may be the same place.
- A loop whose iterations write their own elements, or combine one value with
  one of a fixed set of associative and commutative operations such as
  `+wrap`, can be split across workers.

## Small: C's simple structure, some of Rust's syntax

Whitefoot keeps C's simple structure and borrows some of Rust's syntax. A
program is made of functions, structs, enums and arrays. This function returns
the next byte of a buffer and advances a cursor:

```
struct Cursor {
  position: u64;
}

fn next_byte(input: &[u8], cursor: &Cursor) -> result: Option<u8> reads(input), writes(cursor) {
  let at = deref(cursor).position;
  if at < deref(input).len {
    let byte = deref(input)[at];
    set deref(cursor).position = at + 1_u64;
    return Some<u8>(value: byte);
  }
  return None<u8>();
}
```

A C programmer can read most of this at once. The differences are things
Whitefoot asks you to write out:

- `reads(input), writes(cursor)`: what the function may read and write, its
  effects, stated in its signature. There is no `&mut`: a function writes
  through a reference only when its effects say so;
- `deref(cursor)` and `set`: every read through a reference, and every
  assignment;
- `1_u64` and `value: byte`: the type of every number, and the name of each
  argument to a function or a constructor;
- one operation per expression, with a `let` for each step of a longer
  computation, so there is no operator precedence
  ([GRAM-6](spec/kernel-spec.md#3-grammar)).

Code comes out longer than the same C, and each construct has one spelling.

There are no lifetimes. A reference can be bound to a local or passed to a
call, but it is never stored in a struct or returned
([REF-3](spec/kernel-spec.md#5-ownership-and-references)), so it cannot
outlive what it points to. That is why `Cursor` holds a position rather than
the buffer, and why a function that finds something returns an index, not a
reference. Rust code written this way needs no lifetime annotations either.
Rust also allows a cursor that holds its buffer,
`struct Cursor<'a> { input: &'a [u8], position: usize }`, and then every
struct that contains such a cursor needs a lifetime annotation too. Whitefoot
has only the first way, so there are no lifetimes to learn. The cost is that a
function cannot hand back a reference into its input: a tokenizer returns the
positions of its tokens, not borrowed slices, and the caller forms the
references.

Generics are explicit: a generic function takes its type arguments at every
call, as in `array_filled::<u8, 4>(value: 0_u8)`, and is compiled once for
each set of arguments
([FN-2](spec/kernel-spec.md#8-functions-generics-contracts)).

The language leaves out:

- methods, traits and dynamic dispatch. A call names one function; generic
  code receives the functions it uses as explicit compile-time arguments, an
  `interface` names such a group, and nothing is looked up from a type;
- operator overloading and implicit conversions. `+` on two `u64` values has
  one meaning, and a conversion is written `cvt`;
- exceptions, unwinding and null. An error is a `Result` value and absence is
  an `Option`;
- closures and function values. A choice made at run time is a `match` over
  an enum.

## Highlights

Safe, fast and small are the core. These are the other things worth knowing.

### Available now

- **Easy to write, easy to review.** A small language with no lifetimes and
  one spelling for each construct is quick to learn, and a piece of code
  reads one way. Every signature states what the function reads and writes,
  and a contract states what it requires and ensures, so a reviewer reading
  a call knows what it may touch without opening the function. Every
  rejection names one rule and one location, many also suggest a fix, and
  all of it is available as JSON (`--diagnostic-format json`); tests pin the
  most common fixes to a repaired program that compiles. What makes the
  language easy for people to write and review makes it easy for AI agents
  too.
- **Parallelism sized at run time.** A program never says how many tasks run
  at once. Under `--par` the compiler turns independent calls and loop ranges
  into work that idle workers may take, and the runtime decides how far a
  recursion fans out from the number of workers (`WF_WORKERS`); a call that no
  worker picks up runs on the calling thread. Whatever the runtime decides,
  the result equals the sequential one, and `--par-ledger` explains each
  decision the compiler made.
- **Incremental builds.** A program is checked and compiled module by module.
  With `--cache DIR`, a module's verdict and each function's proof are reused
  while their inputs are unchanged, and compiled code is cached as well, so
  an edit re-proves and recompiles little more than what it changed; each
  build still type-checks the whole program. Build speed has not been
  measured systematically yet.

### In progress

- **Concurrent I/O without async.** The language has no `async`, `await`,
  futures, callbacks or tasks: files and sockets are ordinary values, and an
  I/O operation is an ordinary call. The compiled program submits I/O through
  a completion runtime (io_uring on Linux, I/O completion ports on Windows),
  and independent calls in plain sequential code are issued together, so
  code is never split into synchronous and asynchronous kinds. The `reads`
  and `writes` rows that let computation run in parallel decide which I/O
  calls may overlap. Serving many connections at once is being designed.

### Planned

- **The maximum-safety mode** described under [Beyond
  memory](#beyond-memory-resources): no dynamic resources, a proved stack
  bound, proved termination, peripherals as linear values, no parallelism
  scheduled at run time, and proved response and startup times.
- **Bare-metal targets.** Today the compiler builds programs that run on
  Linux, macOS and Windows. Programs that run without an operating system,
  such as firmware, are planned.

### Research directions

Not started. Each builds on what the proofs already establish.

- **Safe GPU kernels.** A kernel is correct only if no two threads write the
  same element. Whitefoot already proves that ranges such as `[0, p)` and
  `[p + 1, n)` of one array do not overlap; that is how `--par` splits the
  quicksort above. The same proofs could show that each GPU thread writes
  only its own part of an array, including parts computed from the thread's
  index. Rust's borrow checker cannot see that two computed ranges are
  disjoint, so a kernel in Rust usually splits its data by a fixed pattern,
  such as equal chunks, or uses `unsafe`.
- **Parallelism tuned by profiles.** Because the program never fixes how many
  tasks run, the degree of parallelism can be tuned to a workload from a
  profile, or adjusted while the program runs, without editing the source.
- **Sandbox policies from effects.** A program's checked effects could become
  its seccomp filter, WASI capability set, or file and network allowlist, so
  that a deployed program can do only what its signatures say.
- **Constant-time code.** A discipline that keeps a secret from choosing a
  branch, an address or a variable-latency instruction, for cryptographic
  code.
- **Safe libraries for C.** A Whitefoot module shipped as a C header with
  opaque, validated handles, so that a C program can replace its riskiest
  code, such as a parser, with proved code.

## Articles

Short pieces, each on one idea, with programs that compile. The first three
start from the examples above:

1. [Prove it or write a branch](docs/articles/prove-it-or-write-a-branch.md) —
   bounds and overflow checks that disappear because they are proved.
2. [Write sequential code, get parallel
   results](docs/articles/sequential-code-parallel-results.md) — how the
   compiler finds independence in plain code, recursion included, and hands
   it to the workers.
3. [Proofs without a solver, by hand](docs/articles/proofs-by-hand.md) —
   difference bounds, closure and loop invariants, worked on paper.
4. [What a rejection tells you](docs/articles/what-a-rejection-tells-you.md) —
   diagnostics written for the agent that fixes the code.
5. [Integers](docs/articles/integers.md) — every operation states its meaning.
6. [One build](docs/articles/one-build.md) — no panic, no debug/release
   split, a fixed record on resource exhaustion.
7. [Beyond memory](docs/articles/beyond-memory.md) — no heap, linear
   resources, and the plan for a maximum-safety mode.
8. [What a reviewer reads](docs/articles/what-a-reviewer-reads.md) —
   contracts and effect rows as the review surface.
9. The trusted base — what is trusted, and the plan to shrink it.
10. Where the speed comes from — every way the proofs are used.
11. I/O without async — ordinary calls that the compiler overlaps.
12. A layout engine — the first large program.
13. How this project is built with agents.

The other articles are being written; each title becomes a link when its
article is published.

## Try it

You need Rust stable, at least the `rust-version` in
[compiler/Cargo.toml](compiler/Cargo.toml), and clang: `/usr/bin/clang` on
Linux and macOS, or `clang` on `PATH` on Windows.

```sh
git clone https://github.com/mbbill/Whitefoot.git && cd Whitefoot
cargo build --release --manifest-path compiler/Cargo.toml
compiler/target/release/whitefootc tests/programs/wfgrep.wf -o wfgrep
./wfgrep invariant tests/programs
compiler/target/release/whitefootc tests/conformance/cases/op4-neg-index-undischarged.wf
```

Building the compiler takes about a minute; compiling the grep takes about
four seconds. The last command shows a rejection: the location, the cited
rule and its kind, the marked source line, and every payload field under a
stable label.

```text
tests/conformance/cases/op4-neg-index-undischarged.wf:6:18: error[OP-4]: UndischargedBoundsObligation
  source:   return deref(b)[i];
  marker:                  ^^^
  residual: i < deref(b).len
  disposition: Unproved
  mechanical_fix: add `requires i < deref(b).len;` to the `contract` of `get`, which each caller then establishes; or guard the access with `if i < deref(b).len` where skipping it is the intended behavior, adding to the effect row any read that condition makes which the row does not yet declare
```

Other options:

- `--par` builds the parallel version, and `--par-ledger` prints every
  parallelism decision with its reason. At run time, `WF_WORKERS` sets how
  many workers it uses;
- `--stack-ledger` reports each function's frame and how many levels each
  recursive cycle fits;
- `--emit-llvm` prints the LLVM IR;
- `--diagnostic-format json` prints each rejection as one JSON object per
  line;
- `whitefootc --help` lists the rest.

To work on the language or the compiler, start from [AGENTS.md](AGENTS.md)
and the [workflow map](docs/workflow.md).

## Related work

| | Borrowed | Different |
|---|---|---|
| Rust | ownership, `Result`, no null | no `unsafe` and no lifetimes in source; bounds and overflow are proved, not checked at run time |
| C | functions and structs as the main building blocks | no undefined behavior; every partial operation is proved; enums carry payloads |
| SPARK | proving the absence of runtime errors | no SMT solver, and acceptance is the proof; what the fixed procedure cannot prove is written as explicit steps |
| Wuffs | a proof checker instead of a solver | a general-purpose language with heap data and effects |
| Astrée, Frama-C (Eva) | a fixed, terminating analysis that proves the absence of runtime errors without a solver | they analyze C programs beside the compiler, which builds them either way; in Whitefoot the proof is a condition of compiling |
| Dafny, Verus | contracts and invariants | the goal is runtime safety, not full functional correctness |

## Disclaimer

Whitefoot is a research language and compiler, not a product. Do not use it
for anything that matters.

## License

Whitefoot is available under the [MIT License](LICENSE).
