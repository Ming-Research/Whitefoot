# Whitefoot

Whitefoot is a research systems programming language built around three
properties:

- **Safe.** A program the compiler accepts has no undefined behavior, given
  a correct trusted base; it cannot panic, and no bounds, overflow or
  conversion check runs in it. There is no `unsafe` to opt out with.
- **Fast.** The safety comes from proofs checked at compile time, not from
  checks at run time, and the same proofs let the compiler drop bounds and
  overflow checks, tell LLVM which references do not alias, and run
  independent code in parallel.
- **Small.** Functions, structs, enums and explicit generics, close to C. No
  lifetimes, no methods, no traits, no exceptions.

The compiler finds most proofs itself, with a fixed procedure and no SMT
solver. You write the rest as loop invariants and, now and then, a short
proof step, and the compiler checks them.

## Fast: the proofs pay for the speed

A proof that an operation is in range also makes its runtime check
unnecessary, and a proof that two pieces of code touch different memory lets
them run at the same time. The two examples below show one use each, and the
list after them names others.

### A bounds check proved away

This loop keeps the non-space bytes of a buffer, in place. The line
`invariant behind: kept <= i` states why the store `buf[kept]` is in range.
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
sequential program computes. A run that sorts 2 million numbers takes 0.18 s
sequentially and 0.07 s on 4 workers
([measurement](research/experiments/par-quicksort/README.md));
`--par-ledger` prints every decision with its reason. [Write sequential code,
get parallel results](docs/articles/sequential-code-parallel-results.md)
follows the compiler from the checked rows to the parallel code.

### Other uses of the same proofs

- A proved `+` compiles to a plain add carrying LLVM's no-wrap flag (`nuw`
  unsigned, `nsw` signed), which the optimizer can use.
- Each reference parameter reaches LLVM as `noalias`, C's `restrict`,
  because every call has proved that what one argument writes, no other
  argument reaches. The exception is `swap`, whose two arguments may be the
  same place.
- A loop whose iterations write their own elements, or combine one value with
  one of a fixed set of associative and commutative operations such as
  `+wrap`, can be split across workers.

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
  definitions, libc and the operating system ([SCOPE-3](spec/kernel-spec.md)).

As far as we know, no other general-purpose systems language gives this
guarantee for every program it accepts. SPARK proves the same absence of
runtime errors, with SMT solvers, as an analysis separate from compilation:
the Ada compiler builds a program whether or not it has been proved. Wuffs
checks similar proofs without a solver, but it is a language for libraries
that parse, decode and encode file formats, and its code cannot make system
calls or allocate memory.

## A small language

Whitefoot is close to a safe C with simple generics. Its syntax borrows from
Rust, but a program has C's shape: functions, structs, enums, arrays and heap
cells; a generic function takes its type arguments explicitly, as in
`array_filled::<u8, 4>(value: 0_u8)`, and is compiled once per instance.
Here is a cursor over a byte buffer:

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

A C programmer writes the same shape: a struct that holds a position, and a
function that takes the buffer. A Rust cursor that borrows its buffer
carries a lifetime, `struct Cursor<'a> { input: &'a [u8], position: usize }`,
and a struct that stores one usually needs a lifetime parameter too. Whitefoot
has no lifetimes at all. A reference can be bound to a local and passed to a
call, but never stored in a struct or returned
([REF-3](spec/kernel-spec.md)), so it cannot outlive what it points to; a
function that finds something returns its index. Rust written in the C shape
needs no lifetime annotations either; Whitefoot makes that shape the only
one.

The language also leaves out:

- methods, traits and dynamic dispatch. A call names one function; generic
  code receives the functions it uses as explicit compile-time arguments, an
  `interface` names such a group, and nothing is looked up from a type;
- operator overloading and implicit conversions. `+` on two `u64` values has
  one meaning, and a conversion is written `cvt`;
- exceptions, unwinding and null. An error is a `Result` value and absence is
  an `Option`;
- closures and function values.

The cost is spelling: an expression does one operation, literals carry their
type (`1_u64`), arguments are named, and a reference is read through `deref`.
Code is longer than C, and each construct has one spelling.

## Articles

Short pieces, each on one idea, with programs that compile. The first three
start from the examples above:

1. Prove it or write a branch — bounds and overflow checks that disappear
   because they are proved.
2. [Write sequential code, get parallel
   results](docs/articles/sequential-code-parallel-results.md) — how the
   compiler finds independence in plain code, recursion included, and hands
   it to the workers.
3. [Proofs without a solver, by hand](docs/articles/proofs-by-hand.md) —
   difference bounds, closure and loop invariants, worked on paper.
4. What a rejection tells you — diagnostics written for the agent that fixes
   the code.
5. Integers — every operation states its meaning.
6. One build — no panic, no debug/release split, a fixed record on resource
   exhaustion.
7. What a reviewer reads — contracts and effect rows as the review surface.
8. The trusted base — what is trusted, and the plan to shrink it.
9. Where the speed comes from — every way the proofs are used.
10. A layout engine — the first large program.
11. How this project is built with agents.

The other articles are being written; each title becomes a link when its
article is published.

## What you write

Contracts on functions (`requires`, `ensures`), `reads`/`writes` rows on
signatures, loop invariants, and occasionally an explicit proof step. The test
programs and container library contain about 200 contract blocks, 290
invariants and 41 explicit proof steps across about 800 functions. The grep,
about 1,700 lines, needs 2 invariants and no explicit proof step.

The proof procedure is fixed ([ENT-1](spec/kernel-spec.md)): it has no timeout
and no work budget, so every machine gives the same verdict. Proof steps for
an invariant the compiler proves on its own are themselves an error, so proofs
do not accumulate as noise.

## Status

Whitefoot started in July 2026 and is a research compiler, not a product. One
person makes the design rulings; most of the code is written by AI agents and
checked against the specification, the conformance suite and review. Do not
use it for anything that matters.

Not yet available:

- calls to C from source; C enters only as trusted linked definitions;
- high-concurrency I/O for servers, which is being designed;
- explicit threads, async or SIMD. Parallelism comes from `--par` as
  described above.

## Try it

Requires Rust stable (see [Running the compiler](#running-the-compiler)) and
clang.

```sh
git clone https://github.com/mbbill/Whitefoot.git && cd Whitefoot
cargo build --release --manifest-path compiler/Cargo.toml
compiler/target/release/whitefootc tests/programs/wfgrep.wf -o wfgrep
./wfgrep invariant tests/programs
compiler/target/release/whitefootc tests/conformance/cases/op4-neg-index-undischarged.wf
```

Building the compiler takes about a minute; compiling the grep takes about
four seconds. The last command shows a rejection; `--diagnostic-format json`
prints it as JSON.

## Evidence

- [Specification](spec/kernel-spec.md): 132 numbered rules. Every rejection
  cites one rule and one location.
- [Conformance suite](tests/conformance/): about 1,300 cases, more than 600
  of which must be rejected under a named rule (70 distinct rules).
- [Programs](tests/programs/) built and run by the test gate.
- [Known defects and follow-up work](docs/todo.md), including compiler bugs.
- [Experiments](research/experiments/README.md), negative results included.

## Related work

| | Borrowed | Different |
|---|---|---|
| Rust | ownership, `Result`, no null | no `unsafe` and no lifetimes in source; bounds and overflow are proved, not checked at run time |
| C | functions and structs as the main building blocks | no undefined behavior; every partial operation is proved; enums carry payloads |
| SPARK | proving the absence of runtime errors | no SMT solver, and acceptance is the proof; what the fixed procedure cannot prove is written as explicit steps |
| Wuffs | a proof checker instead of a solver | a general-purpose language with heap data and effects |
| Dafny, Verus | contracts and invariants | the goal is runtime safety, not full functional correctness |

## Working on the project

The [constitution](docs/constitution.md) owns the objectives and tradeoffs;
[AGENTS.md](AGENTS.md) owns priorities and workflow, including how agents work
under the owner's rulings. Read the material that owns your question:

| Question | Source |
|---|---|
| What does the language admit? | [Active kernel specification](spec/kernel-spec.md) |
| What does this compiler implement, and how do I run it? | [Running the compiler](#running-the-compiler) below; the conformance report states the implemented surface |
| What happens when in development, where, and who decides? | [Workflow map](docs/workflow.md) |
| How do I work on a branch and prepare a merge? | [AGENTS.md](AGENTS.md) |
| How do I amend the specification, finish a task, or hand work back? | [Agent skills](docs/skills/) |
| Which writer forms should I try? | [Patterns](docs/patterns.md) |
| How should I investigate, verify, and maintain documentation? | [AGENTS.md](AGENTS.md#how-work-proceeds), the [investigation skill](docs/skills/investigation/SKILL.md) and the [document roles](docs/workflow.md#document-roles) |
| Why was a design chosen? | [Design trees](design/), with reasons and refused alternatives |
| Which research questions and experiments could be useful? | [Ideas](docs/ideas.md) |
| What defects and follow-up work remain? | [Todo](docs/todo.md) |

Research and dated essays provide evidence and ideas; they do not add approval
requirements. The reading and authority rules are in
[AGENTS.md](AGENTS.md#authority-and-reading).

Repository layout:

- [compiler/](compiler/): the Rust compiler, LLVM emission, and native
  runtime support.
- [lib/std/](lib/std/README.md): the standard library package the compiler
  carries from its own build: the host modules `std::io`, `std::text`,
  `std::fs`, `std::net` and `std::process`, and the container modules under
  `std::collections`, exercised by callers in the ordinary program corpus.
- [spec/](spec/): the active language and its immutable version archives.
- [tests/](tests/): normative conformance evidence, executable programs,
  code-generation evidence, and the separate performance regression suite.
- [docs/](docs/): principles, writer guidance, the workflow map, agent
  skills, and reference material.
- [research/](research/README.md): investigations and experiments with their
  designs, measurements, and rejected alternatives.
- [design/](design/): live design decisions with their reasons, and the
  procedure that maintains them.
- [governance/](governance/): archive-protection hooks and specification-change
  design evidence.
- [.github/](.github/): CI, repository checks and the pull-request template.
- [.agents/skills/](.agents/skills/) and [.claude/skills/](.claude/skills/):
  links through which Codex and Claude Code discover the skills kept in
  `docs/skills/` and `design/skill/`.
- [archive/](archive/): frozen historical material. Active source, builds,
  tests, and tools do not depend on it.

## Running the compiler

Prerequisites: a Rust stable toolchain at least the version in
[compiler/Cargo.toml](compiler/Cargo.toml)'s `rust-version` (`rustup update
stable` on an older installed stable — rustup does not update it on its own),
and clang available at `/usr/bin/clang` on Linux/macOS or as `clang` on PATH
on Windows. A cached build that links ThinLTO fragments (`--cache DIR
--fragments module|function`) also needs LLD on Linux and Windows; the macOS
toolchain's linker does link-time optimization itself. `--full-lto` is
research-only: it builds the comparator that the
[build-cost experiment](research/experiments/modular-build-cost/RESULTS.md)
measures fragment builds against, the program and its runtime optimized as one
region, with the same linker requirement, and is not a build mode for
programs.

From `compiler/`:

```sh
cargo run --bin whitefootc -- source.wf -o program
cargo run --bin whitefootc -- --emit-llvm source.wf
cargo run --bin whitefootc -- --par source.wf -o program
```

`whitefootc` accepts an ordered bundle of source files. `--no-overlap` selects
the exact sequential reference lowering and cannot be combined with `--par`.
`--par-ledger` and `--stack-ledger` print their reports; name the LLVM output
with `-o` when a report and emitted LLVM would otherwise share stdout.

`--par` takes three grain controls. `--par-scalar-leaf-limit N|off` moves or
removes the default threshold that keeps scalar leaves of at most 16
nonconstant operations out of compute offers; `--par-sequential-refusal` runs
a refused offer's callee in its sequential clone; `--par-recursive-frontier
auto|N|off` sets the starting budget of a recursive component's clone family,
where `auto`, the default, asks the runtime, `N` from 1 to 32 pins it at
compile time, and `off` emits no family so every node offers. `whitefootc
--help` prints the full usage. At run time `WF_WORKERS` selects compute
participation; `WF_STACKS` is inert.

A rejection prints the location, the cited rule and the kind, the marked
source line, and every payload field under a stable label:

```text
bounds.wf:11:21: error[OP-4]: UndischargedBoundsObligation
  source:       set deref(out)[kept] = byte;
  marker:                     ^^^^^^
  residual: kept < deref(out).len
  disposition: Unproved
  mechanical_fix: `kept < deref(out).len` is not proved here: when facts that reach the access imply it, ...
```

`--diagnostic-format json` prints the complete record, category, stage and
byte interval included, as one JSON object per line.
The [readable-diagnostics investigation](research/investigations/readable-diagnostics/DESIGN.md)
describes the record.

## Verification

`make check` also needs `python3` (design lint, repository invariants and the
conformance runner), LLD on Linux (`ld.lld`, Debian/Ubuntu package `lld`) for
the fragment-build test, and the guarded wrapper `.github/run-check.pl`, used
below and throughout this section, needs `/usr/bin/time` (Debian/Ubuntu
package `time`).

From the repository root:

```sh
make check
make install-hooks   # optional: catch immutable-spec edits earlier
```

`make check` is the canonical complete gate and prints group and phase timings.
The root [Makefile](Makefile) owns its group inventory and recipes; ordinary CI
reads that same inventory with `make check-groups` and invokes the same
`make check-group GROUP=<name>` entry. For a shorter development feedback loop:

```sh
make static
make -C compiler format lint
make -C compiler build        # optimized compiler only
make -C compiler test-build   # construct test executables without running cases
perl .github/run-check.pl source-proofs cargo test --manifest-path compiler/Cargo.toml --profile gate --locked --offline --lib semantic::tests::source_proofs
```

Use a test filter matching the responsibility changed; `source_proofs` above
is one example. The `gate` profile builds the Rust compiler implementation and
test harnesses with optimization, debug assertions and overflow checks. It is
not an optimization switch for WF source. Local builds of it are incremental,
so an edit rebuilds in seconds rather than minutes; CI sets
`CARGO_INCREMENTAL=0`. Use a dev build when debugging the
Rust implementation, rather than constructing it for ordinary verification.
Formatting and API documentation have explicit `format` and `docs` commands;
they are not extra correctness-test stages. The complete gate is still
required on the exact revision merged into main.

The root gate, research/benchmark checks and compiler verification targets use
one host-wide owner across worktrees, with two Cargo jobs and two test threads
by default. Wrap other heavy commands as in the filtered example above. The
wrapper prints wall/user/system time and a heartbeat every 30 seconds; a
competing invocation reports the owner and exits. Its 30-minute command limit
terminates the owned process group, including nested commands. Set
`WHITEFOOT_CHECK_TIMEOUT` in seconds for an intentionally longer protocol.
Explicit job/thread settings remain available. After an uncatchable stop,
inspect the recorded PID and command before removing a stale lock.

For a slow compiler test, set `WHITEFOOT_TEST_TIMINGS` to a scratch TSV path.
The shared semantic/backend/program helpers record test name and phase:
Whitefoot compilation, native construction, native execution and semantic
assertions. This is diagnostic coverage of those helpers, not every custom
subprocess. Nested or parallel rows are not additive suite wall time. See the
[measured build/test investigation](research/investigations/test-economy/build-and-test.md).

The [gate workflow](.github/workflows/gate.yml) runs those groups on Linux and
macOS, and the
[design-readiness workflow](.github/workflows/design-readiness.yml) rejects
pending design amendments on a pull request that is ready for review.
`make review-scope` lists what a completion review covers. Additional
[I/O host checks](.github/workflows/io-hosts.yml) and
[benchmarks](.github/workflows/io-bench.yml) own their platform-specific
evidence. Automatic CI checks correctness and performance regressions under
the [test rules](AGENTS.md#specification-and-test-integrity): useful
research cases and their dependencies belong in formal tests, while research
runs on explicit request. Full IO matrices and compute scoreboards are
experiments; the separate
[compute regression check](.github/workflows/compute-regression.yml) supplies
a paired performance verdict using the [formal runner](tests/performance/README.md).
Routine correctness CI and local `make check`
do not build a baseline compiler or run that comparison. A green run describes its tested revision and
coverage; it is not a proof of completeness or the absence of known defects.
Conformance reports distinguish passing cases, expected compiler failures,
and pending support.

Specification identity is derived from the active file's bytes by
[compiler/build.rs](compiler/build.rs). The work-branch and specification
amendment rules are stated once in [AGENTS.md](AGENTS.md#branch-and-main-boundary).

## License

Whitefoot is available under the [MIT License](LICENSE).
