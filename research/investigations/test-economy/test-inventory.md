# Test system inventory

[The redesign and delivered test map](redesign.md#delivered-test-map) describe
the replacement. The counts, paths and times below are the dated pre-redesign
baseline, not current commands or target inventory.

This is the source, artifact, resource and execution inventory for redesigning
the test system measured in PR #66. It describes implementation `f3858780`;
it does not claim that the existing division into targets is necessary or that
every existing assertion is valuable. Paths are relative to the repository
root. Retain this dated inventory with its measurements for before/after attribution;
current responsibilities and commands belong to the delivered map.

[Build and test cost](build-and-test.md) retains the dated measurements,
attribution methods, implementation changes and limitations.
[test-times.tsv](test-times.tsv) contains individual compiler/corpus test
attempts. No build, test selection, specification or verdict changes accompany
this classification, and no long measurement protocol was rerun for it.

## Contents

- [What is being counted and timed](#what-is-being-counted-and-timed)
- [Compiler and test artifacts](#compiler-and-test-artifacts)
- [1. Compiler frontend and proof implementation](#1-compiler-frontend-and-proof-implementation)
- [2. Backend emission and generated-program checks](#2-backend-emission-and-generated-program-checks)
- [3. Scheduling, loop splitting and exhaustion samples](#3-scheduling-loop-splitting-and-exhaustion-samples)
- [4. Command-line tools and their own unit tests](#4-command-line-tools-and-their-own-unit-tests)
- [5. Source corpora: canonical form, conformance and snapshots](#5-source-corpora-canonical-form-conformance-and-snapshots)
- [6. Real-program integration tests](#6-real-program-integration-tests)
- [7. Direct C runtime checks](#7-direct-c-runtime-checks)
- [8. Research models and compiler witnesses](#8-research-models-and-compiler-witnesses)
- [9. Benchmark-program construction and small correctness checks](#9-benchmark-program-construction-and-small-correctness-checks)
- [10. Repository, build-tool and test-runner checks](#10-repository-build-tool-and-test-runner-checks)
- [11. Performance protocols](#11-performance-protocols)
- [12. Historical and other explicit experiment runners](#12-historical-and-other-explicit-experiment-runners)
- [Local entry points](#local-entry-points)
- [CI entry points](#ci-entry-points)
- [Repetition and redesign questions](#repetition-and-redesign-questions)
- [Unmeasured boundaries](#unmeasured-boundaries)

## What is being counted and timed

Four different objects have previously been described as a "test program":

| Object | Concrete example | What the count means |
|---|---|---|
| Rust test case | A `#[test]` function in `compiler/tests/programs/wfgrep.rs` | One assertion routine; it can compile several programs or run one program many times |
| Rust test executable | `compiler/target/gate/deps/programs-<hash>` | One process hosting 110 Rust test cases, not 110 Rust executables |
| WF input | `tests/programs/wfgrep.wf` | Source fed to the compiler; the same input appears in several checks |
| Native program under test | A scratch `program` linked from emitted LLVM and C runtime objects | A child process whose output/status/behavior is observed; not another Rust test harness |

The measured tree contains **39 WF files in `tests/programs/`**, including
multi-file program components, and **110 Rust cases** in the `programs` test
executable. There is no one-to-one mapping between these counts. The other two
large source collections contain 805 conformance files and 484 snapshot files.

The boundaries of a typical native test are:

```text
Rust construction, only when its Cargo target is missing or stale
  compiler/src/*.rs + #[test] functions -> rustc -> Rust test executable

Rust test execution
  one selected #[test] function starts
    WF bytes -> whitefoot compiler functions -> checked program / LLVM text
    LLVM + runtime C/LLVM + observer -> Clang and linker -> native executable
    native executable + input/files/environment -> child process -> observations
    Rust assertions compare the observations with the expected result
```

The outer test time includes the WF compilation, native construction and child
execution inside it. The compiler is normally called as a Rust library in
these tests; it is not necessarily a `whitefootc` subprocess. Research shell
recipes and platform workflows often use the actual `whitefootc` executable.

**Timing conventions used throughout this inventory:**

- All values are elapsed seconds. They are not CPU seconds or test counts.
- Cold Rust construction comes from `277a1844`, with a fresh Cargo target,
  two build jobs, an available registry and no downloads. These are the
  compiler-then-tests measurements, not a new cold run of the report revision.
- Local suite execution comes from the successful `f3858780` gate, with Rust
  targets already built and two test threads. That gate took **786.97 s**;
  IO image reconstruction was still required. Separate cold/warm gate results
  are 1353.25/648.54 s under the artifact states in the measurement report.
- Individual-case and native-command attribution are separate runs at the
  same implementation. They are identified where used. A partial or failed
  attempt is not substituted for a passing whole-suite time.
- A parent's time includes its children. Concurrent Cargo units and concurrent
  tests overlap. A shared build duration printed in two categories is one
  shared cost, not a cost to add twice.
- **Not separately measured** means exactly that; it does not mean zero.
  Detailed source inspection does not manufacture a phase measurement.

## Compiler and test artifacts

Owner: [compiler/Cargo.toml](../../../compiler/Cargo.toml),
[compiler/Makefile](../../../compiler/Makefile).

There is one Cargo package, `whitefoot`, with no external crate dependencies.
Its normal tools and test executables are different Rust targets. The
following inventory excludes transient native images produced while tests run.

```text
compiler/build.rs
  -> one build-script executable, then execution to derive build metadata

compiler/src/lib.rs and its modules
  -> ordinary compiler library: target/gate/deps/libwhitefoot-<hash>.rlib
  -> test-enabled library: target/gate/deps/whitefoot-<hash>
       one executable, 1662 #[test] cases

compiler/src/bin/whitefootc.rs
  -> tool: target/gate/whitefootc
  -> test executable: target/gate/deps/whitefootc-<hash>, 14 cases

compiler/src/bin/spec.rs
  -> tool: target/gate/whitefoot-spec
  -> test executable: target/gate/deps/whitefoot_spec-<hash>, 9 cases

compiler/src/bin/grammar_tables/main.rs and its sibling modules
  -> tool: target/gate/whitefoot-grammar-tables
  -> test executable: target/gate/deps/whitefoot_grammar_tables-<hash>, 1 case

compiler/tests/programs.rs and programs/*.rs
  -> target/gate/deps/programs-<hash>, 110 cases
compiler/tests/canonical_corpus.rs
  -> target/gate/deps/canonical_corpus-<hash>, 3 cases
compiler/tests/conformance.rs and conformance/*.rs
  -> target/gate/deps/conformance-<hash>, 1 corpus-driving case
compiler/tests/snapshot.rs
  -> target/gate/deps/snapshot-<hash>, 1 corpus-driving case
```

Paths in that tree start at `compiler/`; Windows tools additionally have an
`.exe` suffix. In total this is **eight Rust test executables**, **three normal
tools**, the ordinary compiler library and Cargo's build-script artifact. It
is not eight independently built copies of the ordinary compiler library.

The two isolated construction commands, from `compiler/`, are:

```sh
cargo build --profile gate --bin whitefootc --locked --offline
cargo test --profile gate --all-targets --no-run --locked --offline
```

The guarded equivalents are `make -C compiler build` and
`make -C compiler test-build`. Recipes in the categories below describe the
underlying construction; the [verification entry points](../../../docs/workflow.md#checks)
own bounded local invocation.

| Operation | Gate construction | Dev/default-test construction |
|---|---:|---:|
| Construct `whitefootc`, including its ordinary library | 43.34 | 15.43 |
| Subsequently construct all test targets and remaining normal tools | 76.79 | 26.73 |
| Sequential total, with no test execution | 120.13 | 42.16 |

The ordinary library's Cargo unit takes 41.93 s in gate / 14.19 s in dev. The
CLI unit then takes 0.72/0.45 s. Compiling/executing `build.rs` takes
0.22/0.32 s in gate and 0.29/0.46 s in dev; command overhead accounts for the
remaining difference. The test-enabled library is a separate unit taking
**69.36/23.38 s**, already included in the second command. Other target unit
durations appear with their categories below and overlap at two Cargo jobs.

`gate` inherits optimized Rust release code generation and retains debug
assertions and overflow checks. `dev` is the unoptimized Rust compiler build.
These are not optimization settings on WF source. The same wfgrep LLVM
emission took 39.49 s through prebuilt gate `whitefootc` and 535.38 s through
prebuilt dev `whitefootc`; neither operation executed wfgrep. The outputs were
byte-identical. Which individual rustc optimization passes dominate the cold
Rust build remains unmeasured.

## 1. Compiler frontend and proof implementation

**Purpose.** Test the compiler's own representations, judgments and
diagnostics before native execution: tokens, parse trees, canonical rendering,
resolution, typing, ownership, proof derivation and lowering.

**Sources and cases.** These are **1318 `#[test]` cases** in the library,
distributed as follows. Source fixtures are byte/string literals in Rust and
`include_bytes!`/`include_str!` references to WF files. Referenced corpus and
research files are test resources, not additional Rust targets.

| Module and source location | Cases | What is checked |
|---|---:|---|
| `compiler/src/source/tests.rs` | 12 | Source bundles, paths and source limits |
| `compiler/src/lexer/tests/` | 33 | Tokens, malformed bytes, literal and location handling |
| `compiler/src/syntax/tests.rs`, `syntax/grammar/tests.rs`, `syntax/parser/tests.rs`, `syntax/parser/finalize/tests/` | 88 | Parsing, grammar classification, tree finalization and canonical rendering |
| `compiler/src/resolution/tests.rs` | 71 | Name lookup, scopes, imports and duplicate/unknown names |
| `compiler/src/semantic/tests.rs`, `semantic/tests/` | 1042 | Types, contracts, ownership, borrows, partial-operation domains, proof facts and ledgers |
| `compiler/src/lowering/tests.rs` | 28 | Checked-program to executable IR, ownership cleanup and loop forms |
| `compiler/src/driver.rs` and `driver/` | 38 | Pipeline, compiler options, diagnostic/report and rejection boundaries |
| `compiler/src/prelude.rs` | 1 | Embedded prelude consistency |
| `compiler/src/spec.rs` and `spec/` | 5 | Embedded specification identity and digest implementation |

**Build.** `cargo test --profile gate --lib --no-run --locked --offline`
builds the single `whitefoot-<hash>` test executable. It also contains
categories 2 and 3. Its 69.36 s cold construction cost is shared by all three
categories; a filtered run does not produce a smaller test executable.

**Run and resources.** The cases call compiler functions inside that process
and assert on results. Their referenced files include `tests/conformance/cases/`,
`tests/programs/`, `research/experiments/compute-bench/programs/` and specific
investigation WF fixtures. This category does not need Clang or a running
WF program to obtain its compiler judgments. The whole library executable also
contains category 2/3 cases that do need native resources.

**Cost and callers.** `make -C compiler test-unit` executes categories 1 and
2 together: **1588 cases, 196.09 s test execution**, 196.13 s outer command.
There is no separately measured exclusive elapsed total for the 1318 cases.
The local compiler/root gate includes this command; CI's `unit` job runs it.

**Individual expensive work.** In `semantic/tests/entailment.rs`,
`frozen_real_sources_retain_complete_proof_roots_without_counted_false_positives`
takes 58.881 s in the case-attribution run: it analyzes UTF-8, the four-file
DEFLATE bundle and wfgrep, then examines proof roots. It executes none of those
WF programs. `generic_counted_roots_are_deterministic_across_twenty_analyses`
takes 9.371 s and repeats analysis twenty times to compare normalized ledgers.
The data does not establish twenty as the minimum useful repeat count.

## 2. Backend emission and generated-program checks

**Purpose.** Check emitted LLVM structure, ABI, resource handling and the
behavior of machine code generated from WF. Some tests only inspect IR;
others compile and run native children. The present `test-unit` name hides
this distinction.

**Sources and cases.** **270 `#[test]` cases** in
[compiler/src/backend/tests.rs](../../../compiler/src/backend/tests.rs) and
`compiler/src/backend/tests/`, excluding `exhaustion.rs`, `loop_split.rs` and
`parallel.rs` (category 3). Examples include `cost_shape.rs`, `slices.rs`,
`base64.rs`, `sched.rs`, `completion.rs` and `system_io.rs`.
Inputs include inline WF, retained WF corpus files, LLVM text and C observers
or deliberately modified runtime builds.

**Build.** Same `whitefoot-<hash>` library-test executable as category 1;
the 69.36 s cold library-test construction is not paid a second time locally.

**Run.** A case may call the in-process compiler, inspect emitted IR, ask
`/usr/bin/clang` to optimize/link it, run the native child and compare its
bytes/status/counters with an independent oracle. Other cases build a C
runtime probe directly without WF input. Helpers live in `backend/tests.rs`
and [compiler/tests/support/mod.rs](../../../compiler/tests/support/mod.rs),
which is also included by the library under test configuration.

**Resources and artifacts.** Executable scratch storage; Clang, host linker,
`nm` where symbols are checked; pthreads; platform C/LLVM runtime sources from
`compiler/src/backend/`, `backend/sched/` and `backend/completion/`. IO cases
also use real files, pipes and host IO facilities. Each native invocation
stages a module/observer and links a native executable in a temporary directory.
The compiler CLI does not need to be launched for the usual library path.

**Reuse.** The shared helper compiles twelve immutable runtime translation
units once per process/dialect and keeps their object bytes in `OnceLock`.
Default-C and explicit-C11 variants are separate. Modules, observers and
executable images remain per case; macro-interposed runtime tests keep their
own builds. Reuse ends when that Rust test process exits. It is not a
persistent cross-suite or cross-CI cache.

**Cost and callers.** Category 1 plus this category executes in 196.09 s under
`test-unit`; this category's exclusive suite wall is not measured. The two
wfgrep `cost_shape` cases take 41.738/39.449 s individually, but share the same
expensive emission/optimization through `OnceLock`; both clocks may include
waiting for it. Adding those case times would double-count shared waiting.
The native scatter oracle case takes 6.342 s, nominal-data execution 4.913 s,
and the Base64 RFC-vector case 4.534 s in that attribution run.

## 3. Scheduling, loop splitting and exhaustion samples

**Purpose.** Exercise runtime policies and interleavings that a single native
execution cannot cover: worker counts, grant/refusal, recursive returns,
cleanup, stack/resource limits and split-loop result equivalence.

**Sources and cases.** Three modules in `compiler/src/backend/tests/`:

| Source | `#[test]` cases | Main observations |
|---|---:|---|
| `exhaustion.rs` | 23 | Exhaustion classification, stack floor, shared fault record and cleanup |
| `loop_split.rs` | 17 | Splitting/grain choices, associative combines, worker counts and published bytes |
| `parallel.rs` | 34 | Grant/join/refusal, recursion controls, ownership and scalar/aggregate ABI |

**Build.** These **74 cases are in the same 1662-case library executable**.
`make -C compiler test-sampling` selects these module names; it does not
construct three Rust test binaries. On a cold separate CI VM it still needs
the entire library-test build.

**Run and resources.** Inline/generated WF and C observers -> compiler library
-> LLVM -> Clang/link -> native children under chosen `WF_WORKERS` and policy
settings. Some tests inspect symbols or run a shell with changed resource
limits. They require scratch storage, native threads, Clang/linker, host
process/resource APIs and the shared runtime sources. An intentionally stopped
native child is classified by its owning oracle; it is not a WF source reject.

**Cost and callers.** **29.23 s execution**, 29.27 s outer command, in local
`compiler check` / root `make check`. CI has a separate `sampling` job. A
separate individual-case run takes 30.562 s; these are different observations.

**Why a small case count can still do substantial work:**

- `recursive_controls_preserve_scalar_and_destination_results`: 12.453 s;
  2 recursion forms x 2 return forms x 9 policies = **36 native images**,
  each run at 2 budgets x 2 grant outcomes = **144 executions**.
- `an_overlapped_program_reports_one_byte_sequence_at_every_worker_count`:
  four worker settings x five executions, plus a sequential reference.
- The split-loop byte-equivalence test checks nine worker settings and the
  unset default. Some split fixtures contain 400,000 loop iterations.
- Grant-observation loops may try up to 32 times and stop after observing a
  grant. A runtime refusing every grant must not pass merely because its
  sequential result is correct.

These are concrete reasons for repeats, not proof that every current matrix
cell or repeat count is minimal. Those choices remain redesign questions.

## 4. Command-line tools and their own unit tests

**Purpose and owner.** The three `[[bin]]` entries in
`compiler/Cargo.toml` provide real tools. Their source-local `#[test]`
functions are built into three additional test executables. A bin-test run
invokes the test harness, not the tool's normal `main`.

### 4.1 Compiler command-line interface

- **Source:** `compiler/src/bin/whitefootc.rs`.
- **Normal artifact:** `compiler/target/gate/whitefootc`, built by
  `cargo build --profile gate --bin whitefootc --locked --offline`.
  It reads WF input and invokes the compiler library; native-output mode also
  launches Clang/linking. `--emit-llvm` stops before native construction.
- **Test artifact:** `target/gate/deps/whitefootc-<hash>`, from
  `cargo test --profile gate --bin whitefootc --no-run --locked --offline`.
  **14 `#[test]` cases** check option combinations, lowering selections,
  source display names, usage text, runtime-unit selection and include closure.
- **Resources:** The ordinary compiler `.rlib`, embedded runtime/header text
  and in-memory argument lists. These unit cases do not run 14 WF programs.
  Actual CLI/native behavior is exercised elsewhere, including categories
  8/9 and Windows platform checks.
- **Cost:** Normal CLI unit 0.72 s after the ordinary library; bin-test unit
  1.06 s. Test execution displays 0.00 s at gate precision, not literally no
  work. The complete cold compiler command remains 43.34 s, not 0.72 s.
- **Caller:** `test-corpus` runs the tests. Research and platform recipes use
  the normal tool. Both artifacts reuse the ordinary compiler library.

### 4.2 Specification scanner and identity checker

- **Source:** [compiler/src/bin/spec.rs](../../../compiler/src/bin/spec.rs).
- **Normal artifact:** `compiler/target/gate/whitefoot-spec`.
  `cargo run --profile gate --bin whitefoot-spec --locked --offline` constructs
  it if necessary and runs it. No arguments checks the active specification's
  derived identity, version declarations, numbered rules and references;
  `--index` and `--counts` expose queries.
- **Test artifact:** `target/gate/deps/whitefoot_spec-<hash>`, built with
  `cargo test --profile gate --bin whitefoot-spec --no-run --locked --offline`.
  **9 `#[test]` cases** exercise identity mismatches, missing status, rule-ID
  shape, invalid references and consistency of index/count output.
- **Resources:** `spec/kernel-spec.md` embedded through the compiler library,
  derived build metadata and deliberately malformed specification strings.
  No WF compilation, Clang or native WF execution is needed.
- **Cost:** Normal bin unit 0.96 s; bin-test unit 1.63 s, both after a usable
  library exists. Test execution displays 0.00 s. The warm `spec` command
  that runs the real checker takes 0.34 s. An isolated CI `static` job needs
  its own ordinary library first: that build takes 51.13/55.58 s on Linux/macOS.
- **Callers:** `test-corpus` runs the nine tests; `compiler check` and
  `compiler static` also run the real checker against the current spec.
  Parts of the current-spec checks overlap with positive test assertions;
  the separate bin count alone does not establish useful additional coverage.

### 4.3 Grammar-table generator

- **Sources:** `compiler/src/bin/grammar_tables/{main,ebnf,model}.rs`;
  checked-in output is `compiler/src/syntax/grammar/generated.rs`.
- **Normal artifact:** `compiler/target/gate/whitefoot-grammar-tables`.
  `cargo build --profile gate --bin whitefoot-grammar-tables --locked --offline`
  constructs a maintenance tool. Its `main` reads specification grammar and
  prints generated Rust tables, writes them with `--output`, or compares
  them with `--check`.
- **Test artifact:** `target/gate/deps/whitefoot_grammar_tables-<hash>`, from
  `cargo test --profile gate --bin whitefoot-grammar-tables --no-run --locked --offline`.
  Its **one `#[test]` case** calls the generator in memory and compares all
  output bytes with the checked-in tables.
- **Resources:** The active spec text, checked-in generated tables, generator
  modules and ordinary compiler library. No Clang or WF executable.
- **Cost:** Normal tool unit 3.31 s; bin-test unit 3.29 s; test execution
  0.32 s. These are distinct artifacts, not two executions of one test.
- **Callers:** `test-corpus` runs the test version. The all-targets construction
  experiment also constructs the normal tool, but the gate does not invoke
  that tool's `main`. Generation is an explicit maintenance operation.

These tools' unit tests could live in shared modules or another test target.
Their useful test identities do not require three physical test executables.
Consolidation would change build dependencies and link/code-generation work;
the actual saving has not been measured. Conversely, the normal grammar tool
being built by Cargo is not by itself a reason to require that extra artifact
in every verification path.

## 5. Source corpora: canonical form, conformance and snapshots

These three integration targets ask different questions about WF input.
Their `.rs` top-level files are separate Cargo test targets today; this is a
packaging choice, not a requirement for three separate operating-system
processes. All link the ordinary compiler library.

### 5.1 Canonical-source and normative-example checks

- **Runner:** [compiler/tests/canonical_corpus.rs](../../../compiler/tests/canonical_corpus.rs),
  with manifest/JSON reader modules under `compiler/tests/conformance/`.
- **Cases/resources:** **3 `#[test]` functions** scan the `.wf` files in
  `tests/conformance/cases/` and `tests/programs/`, consult
  `tests/conformance/manifest.jsonl`, and read the active spec's worked example.
- **Build:** `cargo test --profile gate --test canonical_corpus --no-run --locked --offline`
  -> `compiler/target/gate/deps/canonical_corpus-<hash>`; cold target unit
  **1.93 s**, after the ordinary compiler library.
- **Run:** Lex/parse/finalize/render each source, compare canonical bytes and
  render idempotence; check only permitted manifest exclusions and exact
  normative example bytes. Deliberately malformed corpus inputs follow those
  exclusion checks. No semantic proof, Clang or native program execution.
- **Cost/caller:** **0.72 s** execution in `test-corpus`, included in local
  compiler/root gate and CI `corpus`. This is one executable, not one executable
  per source file.
- **Distinct purpose:** A correctly accepted program may still have a broken
  renderer or stale example copy. This checks those properties; it does not
  repeat the entire conformance semantic verdict.

### 5.2 Specification conformance through the compiler and host toolchain

- **Runner:** `compiler/tests/conformance.rs`, with
  `compiler/tests/conformance/{adapter,corpus,json}.rs` and shared native
  support in `compiler/tests/support/mod.rs`.
- **Cases/resources:** `tests/conformance/manifest.jsonl` and
  `tests/conformance/cases/*.wf`. The manifest has **821 records**: 16 rule
  annotations and 805 cases. Expected acceptance/rejection/run outcomes come
  from the conformance corpus; the Python structure checker is category 10.
- **Build:** `cargo test --profile gate --test conformance --no-run --locked --offline`
  -> `target/gate/deps/conformance-<hash>`; cold target unit **2.06 s**, after
  the ordinary compiler library. It contains **one `#[test]` adapter** that
  loops over the manifest, not 805 Rust test functions.
- **Run:** Root `make conformance-run` invokes that adapter with `--ignored`.
  Ordinary `test-corpus` constructs/selects the target but leaves that one
  expensive test ignored. The explicit root command is what executes it.
  For 804 non-pending cases it calls `whitefoot::compile`; for the 330 `run`
  cases it also links and executes a native program under manifest-provided
  files, arguments, stdin and redirection.
- **Resources:** Compiler library, manifest and WF sources; Clang/linker,
  native runtime objects, executable scratch directories, real host process
  and filesystem behavior. Runtime objects are reused within this adapter
  process; verdicts are always obtained from current compiler calls.
- **Cost/result:** **118.75 s test execution**, 118.79 s outer command in the
  complete gate. Results: 803 pass, one tracked Xfail expecting `Reject(OP-4)`
  but reaching `Unsupported`, and one pending skip. Unsupported is not a
  normative source rejection.
- **Separate phase measurement:** A temporary timer-only adapter takes
  120.54 s: 804 WF checks/emissions **25.514 s**, 330 native builds **23.001 s**,
  330 native launches/waits **70.567 s**, remainder about 1.46 s. These phases
  are serial here and can be added. The native elapsed total includes host
  launch/wait overhead, not just algorithm CPU work.
- **Callers:** Root `make check` and CI `conformance`; not included by merely
  running the non-ignored Cargo suite.

### 5.3 Recorded compiler-verdict snapshots

- **Runner:** [compiler/tests/snapshot.rs](../../../compiler/tests/snapshot.rs).
- **Cases/resources:** `tests/snapshot/index.tsv`,
  `tests/snapshot/cases/<family>/*.wf` and the compiler library. **484 rows and
  WF files**; one `#[test]` function walks the collection.
- **Build:** `cargo test --profile gate --test snapshot --no-run --locked --offline`
  -> `target/gate/deps/snapshot-<hash>`; cold target unit **0.42 s**, after
  the ordinary library.
- **Run:** Call the ordinary compiler on every source and compare semantic
  `accept`/`reject` with the recorded row. Exact diagnostic rule numbers are
  not the comparison. Earlier syntax/source stops and compiler/target failures
  are not silently relabeled as semantic rejection. No Clang/link/native run.
- **Cost/result:** **19.17 s execution**, 19.21 s outer root command; 484 pass,
  zero flips. Separate timed adapter: 19.06 s, including 19.007 s in individual
  read/compile calls. Its slowest case is a binary-search proof at 1.245 s;
  it does not execute a binary search.
- **Callers:** Root `make snapshot-run` passes `--ignored`; root `make check`
  and CI `conformance` call it. Ordinary `test-corpus` leaves it ignored.
- **Meaning:** A changed historical verdict is a signal to investigate, not
  proof of a correctness regression. The active spec decides whether the old
  or new behavior is correct. Unique defect coverage versus category 1 and
  conformance has not been established for every snapshot row.

## 6. Real-program integration tests

**Runner and input locations.** [compiler/tests/programs.rs](../../../compiler/tests/programs.rs)
includes `compiler/tests/programs/*.rs`; its helpers are
`compiler/tests/programs/support.rs` and `compiler/tests/support/mod.rs`.
WF source is primarily the **39 files in `tests/programs/`**, including
multi-file components; some tests construct additional source strings or
modified negative controls. Input files/directories/streams are created by
the test cases.

**Build.** `cargo test --profile gate --test programs --no-run --locked --offline`
links the ordinary compiler library into **one** `programs-<hash>` executable.
Cold target construction is **4.33 s** after the library. It contains
**110 `#[test]` cases**, grouped as follows:

| Module under `compiler/tests/programs/` | Cases | Main subject |
|---|---:|---|
| `parallel.rs` | 43 | Parallel forms, sequential controls, actual grants and output equivalence |
| `wfgrep.rs` | 12 | Search output, line/read boundaries and directory/file behaviors |
| `network.rs` | 12 | TCP clients/servers, refusals, route and concurrency behaviors |
| `runs.rs` | 6 | Fixed-run library construction, transposition, boundaries and draining |
| `traversal.rs` | 6 | Directory enumeration, nested trees, links and denied paths |
| `generics.rs` | 5 | Generic programs and ordinary/parallel forms |
| `heap.rs` | 5 | Owned heap programs and cleanup behaviors |
| `numerics.rs` | 4 | Numeric program results |
| `raw_deflate.rs` | 4 | Independent byte/status checks for DEFLATE outcomes |
| `stream.rs` | 4 | Files/pipes, read and output behavior |
| `text.rs` | 3 | Text and byte processing |
| `binary.rs`, `hashing.rs`, `image.rs`, `signal.rs`, `support.rs`, `wide_scan.rs` | 1 each | Respective program behavior, plus the helper's exact-symbol selector test |

**Run.** Cases call the compiler library to emit LLVM, use Clang plus native
runtime/observer objects to produce one or more executables, run them under
inputs and worker/IO policies, and check bytes, exit codes, filesystem state
or observed grants. Some cases only inspect compilation/IR or a rejection.
Tests may share a compiled program through `OnceLock`, or run the same image
many times; neither all 110 cases nor all 39 source files imply 110 fresh builds.

**Resources.** Clang/linker and current compiler library; ordinary values,
floor, scheduler and completion runtime sources; executable temporary storage;
pipes, symlinks, file modes and byte-oriented POSIX names for relevant cases;
loopback socket permission for networking; sufficient open-file limit for
deep traversal. Denied-path cases require a user actually denied by mode bits,
not a privileged process bypassing them. `WF_WORKERS` and
`WF_IO_NO_NATIVE_RING` select the scenarios being asserted.

**Cost and callers.** The successful complete gate executes all 110 cases in
**241.74 s**. Root/compiler gate and CI `corpus` reach them through
`test-corpus`; that command also executes the 24 bin tests and three canonical
tests, for 137 non-ignored Rust cases in total.

The individual-attribution attempt is not the passing suite above: it takes
247.607 s with 101 pass/9 environment failures (eight denied loopback binds,
one descriptor limit). Those nine pass in a focused 46.535 s recheck with
loopback allowed and descriptor limit 4096. Do not add the attempt and retry
as a normal 110-case cost.

| Successful case in the separate attribution run | WF compile | Native build | Native execution | Whole case |
|---|---:|---:|---:|---:|
| `runs::the_fixed_run_library_proves_and_runs` | 90.222 | 0.106 | 0.262 | 90.593 |
| `parallel::corpus_par_fixed_run_library` | 85.890 | 0.114 | Not observed by helper | 86.038 |
| `parallel::corpus_par_wfgrep` | 85.225 | 0.544 | 0.434 | 86.208 |
| `wfgrep::a_match_across_a_read_boundary_keeps_its_line_number` | 46.178 | 0.236 | 0.224 | 46.643 |
| `parallel::the_default_compilation_of_the_demo_names_no_runtime` | 0.262 | 0.106 | 9.839 | 10.208 |

Fixed-run's 375-line source has four-element runtime capacity: its cost is
proof compilation, not a giant native workload. The last row uses
`tests/programs/par_layout.wf`, with 800 tree traversals per fold, two folds per
batch and several invocations. Its substantial native runtime has a different
cause. Per-case intervals run concurrently and must not be summed into suite
elapsed time.

## 7. Direct C runtime checks

**Purpose.** Test the native scheduler, completion and ordinary-value runtime
directly. A C `main`/assertion harness can force runtime states and link
boundaries without first compiling a WF program. These are not Rust `#[test]`
cases and do not construct a Rust compiler.

**Owner and construction.** [compiler/Makefile](../../../compiler/Makefile),
target `completion-test` and its prerequisites. Source paths in the following
table start at `compiler/src/backend/`. Recipes use `cc -std=c11 -O2`, strict
warnings, pthreads and the probe-specific macro definitions from that Makefile.
They link scheduler/completion C units as appropriate and produce native
executables under `$(COMPLETION_TMP)`, normally
`$(WHITEFOOT_SCRATCH_ROOT)/whitefoot-completion-test`.

**Resources.** A host C compiler/linker, `nm`, native threads, executable
scratch directories, real files and loopback networking for IO probes. The
Linux-native adapter additionally needs Linux headers and usable `io_uring`.
macOS cannot provide Linux kernel execution evidence. C fixtures and scripted
callbacks live in the probe sources; there is no separate WF corpus here.

| Probe source -> executable | What executes and what it checks |
|---|---|
| `completion/core_read_probe.c` -> `core-read-probe` | Isolated completion core/read protocol; real positioned-read boundaries and forced reverse completion, with a forwarding hook that counts host reads; `nm` also checks that it did not acquire a bridge dependency |
| `completion/bridge_default_probe.c` -> `bridge-default-probe` | File/TCP behavior under the shipped helper policy; Linux additionally forces the non-ring route |
| `ordinary_values_probe.c` plus `ordinary_values.c` -> `ordinary-values-probe` | Text/ranges/files/directories/TCP, close/reuse and credit transfer; runs with helpers 0 and 2 |
| `sched/smoke.c` -> `sched-smoke` | Seven startup/failure/lifetime modes at four workers, then one-worker execution |
| `sched/deque_probe.c` -> `sched-deque-probe` and `sched-deque-probe-no-stats` | Two macro builds, each exercising 200,000 tasks and concurrent deque reuse; statistics enabled/disabled |
| `completion/harness.c` -> `harness` | Completion/native-contract assertions with helpers 0, 1 and 4, plus a no-cache-policy run; checks symbols for removed runtime interfaces |
| `completion/pure_compute_probe.c` -> `pure-compute` | Construct executable, inspect symbols with `nm` for forbidden completion dependencies; **do not execute it** |
| `completion/native_adapter_probe.c` plus Linux runtime units -> `linux-native-probe` | On Linux, execute the native ring probe and an additional required-ring harness when available; on macOS, only syntax-check `linux_io_uring.c` |

**Cost.** The ordinary local `completion-test` command takes **5.75 s total**.
A separate shell/CC observer produced the following child-command readings;
its own startup makes the instrumented whole command 9.08 s. Do not add its
rows to the ordinary 5.75 s result or call that difference a runtime regression.

| Artifact | C construction | Execution or inspection |
|---|---:|---|
| `core-read-probe` | 0.468 | 0.247 execution |
| `bridge-default-probe` | 0.496 | 0.315 execution |
| `ordinary-values-probe` | 0.590 | 0.234 / 0.009 for helpers 0 / 2 |
| `sched-smoke` | 0.214 | Seven modes 0.274; one-worker run 0.007 |
| Two deque probes | 0.207 / 0.207 | Combined construction/run shell loop 1.157; executions not separately isolated |
| Completion `harness` | 0.683 | Helpers 0 / 1 / 4: 0.346 / 0.100 / 0.096; no-cache 0.097 |
| `pure-compute` | 0.053 | Symbol check 0.018; no native execution |
| Linux ring syntax check on macOS | 0.024 | No executable produced |

**Callers and repeated construction.** Both `compiler check` and
`compiler static` include `completion-test`; root `make check` reaches it once
through `compiler check`. CI `static` runs it on Linux/macOS, and the Linux
IO-host job runs it again on its own VM. These recipes compile their C probes
on each invocation, rather than using per-artifact freshness prerequisites.
Backend Rust cases in category 2 also exercise native runtime behavior; the
exact assertion overlap is not established merely by shared source files.

### 7.1 Linux sanitizer and required-platform checks

- **Owner:** `.github/workflows/io-hosts.yml`, job `completion-linux`, and the
  `completion-*sanitize`, `completion-*tsan`, `sched-deque-tsan` targets in
  `compiler/Makefile`.
- **Build/run:** Rebuild the relevant C probe/harness with
  `-fsanitize=address,undefined` or `-fsanitize=thread`; ASan/UBSan completion
  recipes use `-O1 -g`. Execute instrumented processes with sanitizer failure
  options. No Rust tests or WF compiler are involved.
- **Resources:** Real Linux runner, Clang and installed sanitizer runtimes,
  threads/files/sockets, usable native `io_uring`. This job treats ring
  unavailability as failure; the general local probe may report unavailable.
- **Cost:** Whole Linux job **43 s**, including setup. Native ring probe 1 s;
  ordinary completion target 8 s; required-ring executions below 1 s;
  ASan/UBSan construction+runs 7 s; core/default/deque TSan step 5 s;
  whole bridge/ring TSan step 3 s. Inner build/run splits are not separately
  logged for these sanitizer steps.
- **Coverage:** Sanitizers test native-memory/thread defects using different
  instrumentation; they are not equivalent to repeating an unsanitized run.
  The job nevertheless rebuilds/repeats some ordinary probes also present in
  the main Linux gate. Their marginal coverage needs a per-assertion review.

### 7.2 Actual Windows platform checks

**Owner.** `.github/workflows/io-hosts.yml`, job `completion-windows`.
The workflow itself is part of the test source: Bash/PowerShell arrange data,
compile C/WF, run children and assert results. It is not a Windows invocation
of the large Rust library-test executable.

**Resources and construction.** Real Windows, Clang, Rust/Cargo, PowerShell
and Bash, writable temporary storage, IOCP, Winsock/loopback and native threads.
The Windows runtime link set uses `sched/prim_windows.c`,
`completion/wait_windows.c`, `completion/file_windows.c`,
`completion/windows_iocp.c`, `windows_runtime.c` and `wf_floor_windows.c`
beside the shared units. Host link libraries include `ws2_32` and `shell32`.
One gate-profile `whitefootc.exe` is constructed and reused by subsequent
WF tests; native outputs have `.exe` names under `RUNNER_TEMP`.

| Step, source and produced artifact | Build/run observations and whole step cost |
|---|---|
| `sched/deque_probe.c` -> stats-on/off probes | C builds and native runs, **9 s** |
| `sched/cpu_levels_probe.c`; `sched/smoke.c` -> CPU/startup probes | C builds and real Windows topology/startup assertions, **1 / 1 s** |
| `completion/bridge_default_probe.c` -> default-route probe | **3 s build + 1 s run**, IOCP and refused-ring routes |
| `completion/native_adapter_probe.c` -> adapter probe | **2 s build + below 1 s run** |
| `windows_namespace_probe.c` -> namespace probe | **1 s build + below 1 s run**, native name/code-unit behavior |
| `ordinary_values_probe.c` and runtime units | **4 s build/run**; separate strict C warning/syntax check of all runtime units **2 s** |
| `completion/windows_bridge_init_fail_stop_probe.c` and a macro-interposed bridge object | **3 s build + below 1 s run**; initialization refusal must fail stop |
| `tests/programs/completion_read_boundary.wf` -> IOCP program | **94 s**, including **88 s Rust compiler construction** and about 6 s WF/native build/run |
| `tests/programs/{tcp_echo,tcp_refused}.wf` -> TCP programs, ordinary/parallel forms | **24 s**, actual loopback peers and both runtime engines |
| `tests/programs/host_string_bytes.wf` -> HostString program | **4 s**, native code-unit/byte boundary |
| `research/experiments/io-completion-bench/programs/windows_component_open.wf` -> direct/completion programs | **8 s**, real wide-character names plus ASCII decoys; expected file bytes distinguish the route |
| `tests/programs/par_layout.wf` plus C grant observers -> worker/control programs | **14 s**, actual non-owner workers, grants/steals, invalid configuration and output equivalence |
| `runaway.wf` generated inline by the workflow -> floor test | **13 s**, ordinary-thread overflow classification using the Windows floor/runtime |

The whole job is **207 s**, including checkout/toolchain/other setup. Later
WF step totals do not separately log WF proof, Clang/link and execution.
Cross-compiling or running under Wine would not replace these host observations.

## 8. Research models and compiler witnesses

**Entry point.** Root `make research-tests`, implemented in the
[root Makefile](../../../Makefile). The measured warm command is **15.94 s**.
This label currently combines standalone Rust models, direct C programs,
real WF compilations and Python/shell oracle self-tests. It is not another
large Cargo unit-test crate.

### 8.1 Explicit-proof checking cost witnesses

- **Directory/owner:** `research/experiments/proof-use-cost/`, `Makefile`,
  `runner.rs`.
- **Cases:** The Rust runner generates WF fixtures, including seven accepted
  checks (one at the 4096-use ceiling) and two rejection controls. They are
  runner-defined checks, not seven `#[test]` functions.
- **Build:** `rustfmt` checks `runner.rs`; `rustc --edition=2024
  --forbid unsafe_code --deny warnings -C opt-level=2 runner.rs` creates
  `$(WORK_ROOT)/runner`. Cargo refreshes gate `whitefootc`.
- **Run/resources:** The runner launches that compiler on generated WF files
  in scratch directories and checks verdicts/diagnostics. It needs Rust,
  executable scratch storage and the compiler CLI, not a native WF workload
  or an external solver.
- **Cost/callers:** **1.57 s warm** in root/CI `research`. A separately
  observed invocation attributes 0.410 s to Rust runner construction and
  0.960 s to fixture checks. `bench`/`compare` are separate timing protocols.
  The runner is rebuilt on each `check` through a phony build recipe.

### 8.2 Container representation experiments

**Shared owner/resources.** `research/experiments/container-representation/Makefile`
refreshes gate `whitefootc` and calls six subdirectory `check` targets below.
Standalone models use `rustc` directly, not Cargo. C programs use `cc`/Clang;
WF programs use the compiler CLI and its native link path. `native.mk` shares
the runtime source list for dense/families and IO experiments, but stores
objects under each caller's own build directory. It is not a global binary
cache. Warm parent cost is **13.36 s** in the final gate; the cold measured
container stage was **142.19 s** under the earlier cold-gate artifact state.

#### Authority and membership

- **Location:** `container-representation/authority/`, specifically
  `model.rs`, `membership.rs`, `Makefile`.
- **Build:** Each `.rs` is compiled twice with `rustc -C opt-level=2`: once
  as a normal executable and once with `--test`. Outputs are
  `.build/{model,tests,membership,membership-tests}`.
- **Run:** **7 + 2 `#[test]` cases**, plus both models' `main` functions;
  enumerate 510 bounded live sets and 31 identity/generation/lifecycle traces.
  `rustfmt` also checks membership source. No WF compiler or C compiler is
  required by this subdirectory's own checks.
- **Cost:** Warm sub-make **0.428 s** in the separate instrumented observation;
  build/run phase totals not isolated. The model/test artifacts are cached
  by their source dependencies. Each Rust test suite displays 0.00 s locally.

#### Foundation and construction layout

- **Location:** `container-representation/foundation/`; `model.rs`,
  `construction.rs`, `rust-baseline.rs`, `layout.c`, `large-result.wf`.
- **Build:** Three Rust sources each produce a normal `.build/` executable
  and a `--test` executable at `-C opt-level=2`; `layout.c` produces
  `.build/layout` at C `-O2`; the WF CLI produces both `large-result.ll` and
  a `large-result` native executable through separate recipes.
- **Run:** **5 + 1 + 2 Rust test cases**, the three Rust models, the C layout
  program and the WF large-result program. These check construction/layout
  behavior; the separate `measure` target also creates retained/optimized IR.
- **Resources:** Rust, Clang/cc, current `whitefootc`, model constants and
  the local WF source; no external dataset or network service.
- **Cost:** Instrumented warm sub-make **1.041 s**, phase split unavailable.
  The `layout` executable itself takes **0.323 s**, including **81 timed
  copy samples totaling 0.224 s** (nine samples across its cohorts, with
  1200 iterations). That timing has no regression decision and currently
  leaks exploratory measurement into `check`. The samples are not 81
  additional correctness test cases.
- **Repeated work to examine:** A missing `.ll` and native program can cause
  two compiler calls on the same WF source. `check` constructs that `.ll`
  without running the retained/optimized IR inspection available to `measure`.
  Normal-model versus `--test`
  assertions also need comparison before consolidating their artifacts.

#### Lifecycle, ownership and negative controls

- **Location:** `container-representation/lifecycle/`; accepted sources
  `pool_known_capacity`, `pool_conservation`, `pool_static_capacity`,
  `pool_boxed_helper`, `pool_boxed_capacity`, `linear_failure_cleanup`,
  `ring_indexed` (all `.wf`), with `pool_helpers.wf`, `linear_helpers.wf`,
  `ring_helpers.wf` where required.
- **Build/run:** The shell `check` recipe unconditionally invokes
  `whitefootc --no-overlap ... -o build/<name>` and executes each of those
  **seven** programs. It then compiles **eight negative fixtures** in
  `--emit-llvm` mode, checks exit status, exact rule/detail and absence of
  output, and writes `build/outcomes.tsv`. One negative source is produced
  by modifying `pool_static_capacity.wf` in the build directory.
- **Resources:** WF files, gate compiler CLI, Clang/linker, ordinary native
  runtime and writable executable storage. No Rust `#[test]` executable.
- **Cost:** Instrumented warm sub-make **12.723 s**; its inner compile/run/
  negative-check loop **12.503 s**. WF build versus native run is not fully
  separated. The seven accepted images are rebuilt even when unchanged;
  this remains a measured residual, not a fix claimed by PR #66.

#### Dense containers and native layout controls

- **Location:** `container-representation/dense/`; `driver.rs`, `dense.wf.in`,
  `inline-view.wf`, `reference.c`, `harness.c`, `Makefile`.
- **Build:** `rustc -C opt-level=2` creates `build/driver`; it generates WF
  for sizes 16/256/4096 and a 16-by-4-lane form, and adapts emitted LLVM.
  `whitefootc` emits LLVM and separately builds smoke programs. Clang produces
  optimized IR, assembly, WF/native objects and linked C comparison programs,
  plus a retained-boundary control. Objects come from `../native.mk`.
- **Run:** Rust formatting check; WF inline-view and smoke executables;
  native comparison harnesses in `verify` mode using a fixed seed. The
  `shape` prerequisite additionally creates `optimized*.ll`, `native*.opt.ll`,
  `native*.s` and `boundary256.ll` for inspection; the shown `check` recipes
  do not automatically compare those files. The WF `dense*.s` assembly is
  also generated, but feeds `dense*.o` and the comparison executables that
  `check` actually runs.
  There is no `rustc --test` executable in this subdirectory.
- **Resources:** Rust, compiler CLI, Clang, C runtime objects, generated WF
  and reference source; no large external corpus.
- **Cost:** Instrumented warm sub-make **0.675 s**; cold Rust/WF/C construction
  not separately measured. Make prerequisites reuse built products; explicit
  `measure` mode adds repeated timed runs, outside root `make check`.

#### Map layout and sparse ownership costs, in correctness mode

- **Location:** `container-representation/costs/`; `map-layout.c`,
  `sparse-owned.c`, `rust-map.rs`.
- **Build:** C `-O2` produces `.build/map-layout` and `.build/sparse-owned`;
  `rustc -C opt-level=2` produces `.build/rust-map`. There are no `#[test]`
  functions selected by this Makefile and no WF compilation.
- **Run:** `rustfmt`, then each executable with `check`, asserting membership,
  payload, allocation and ownership observations. Timed `measure` commands
  are separate recipes.
- **Resources/cost:** Host Rust/C toolchains and process memory;
  instrumented warm sub-make **0.390 s**, construction/run split unmeasured.

#### Container families, ABI adapters and allocation observers

- **Location:** `container-representation/families/`, its `Makefile`, local
  `.wf`, `.rs` and `.c` files; shared `../linkage.rs` and `../native.mk`.
- **WF inputs:** `hashmap`, `owning-map`, `owning-growth`, `owning-behavior`,
  `ordered`, `priority`, `priority-borrowed`, `priority-behavior`,
  `priority-behavior-direct`, `packed-page`, `growth`, `boxed-migration`,
  `ordered-runtime-gap`, `boxed-helper-gap`, `shared-option-view`.
  Fifteen sources x `default` / `--par` / `--no-overlap` = **45 images**.
- **Build:** Current `whitefootc` compiles the images into `.build/`.
  Rust `abi.rs`, `growth-abi.rs`, `priority-interface.rs` and
  `owning-growth-abi.rs` build adapter/inspection tools; they expose or
  transform LLVM for C harnesses. Clang links priority, owning-growth,
  owning-behavior, owning-map and growth observers/cost controls with the real
  runtime. Retained/inlined variants and optimized IR are additional products.
- **Rust tests:** `.build/priority-interface-tests` contains **1 case**;
  `.build/owning-growth-abi-tests` contains **3 cases**, including the test
  brought in through `linkage.rs`. These two `rustc --test` recipes do not
  specify `-C opt-level`; they are not Cargo gate-profile tests. Normal Rust
  adapter tools use `-C opt-level=2`.
- **Run:** All 45 images, the interface/ABI tests and inspections, allocation/
  growth/behavior observers, retained/inlined `check` controls, and the
  `rejected-wrapper.wf` negative compiler case. The words `costs`/`bench` in
  a binary name do not mean its `check` invocation is a timing protocol.
- **Resources:** Rust, current WF CLI, Clang/linker, C observers, native
  runtime objects and executable build storage. No external scheduler
  libraries such as oneTBB are required by these checks.
- **Cost:** A matched warm check changed **81.86 -> 1.89 s** in PR #66 by
  retaining built WF images while still running all 45. In the separate
  observer run, the sub-make is 3.697 s and its cached 45-image execution loop
  is **0.229 s**; observer startup inflates the outer command. Source/compiler/
  Makefile dependencies now control image reconstruction.

Across the six subdirectories, the standalone Rust test count is **21 cases
in seven test executables**: authority 2 executables, foundation 3, families
2. Their normal model/adapter executables are additional products. Native
WF/C images and model enumerations are additional work, not extra Rust cases.
The six instrumented warm sub-make times above cannot be summed into the
ordinary 13.36 s parent measurement.

### 8.3 Independent oracle and regression-rule self-tests

| Owner and cases | Construction, execution and resources | Warm local cost |
|---|---|---:|
| `research/experiments/ripgrep/test_runner.py`, with `runner.py` | `make ... test` runs Python `unittest`: **22 cases** for the external runner/oracle, fixtures and result handling. No Rust compiler build in this target; temporary fixture storage and Python are required. | 0.16 |
| `research/experiments/raw-deflate-default-shape/test_oracle.py`, with its oracle modules | Run Python directly: **19 cases** checking independent DEFLATE decoding/oracle behavior. No Cargo or WF construction in this invocation. | 0.27 |
| `research/experiments/compute-bench/verdict-test.sh`, `verdict.awk` | **14 crafted-table cases**, shell/AWK assertions on the rule that classifies performance regression. No compiler, kernel image or timing measurement. | Not isolated in the final gate; 0.118 in a separate observation |

All three are called by root `research-tests` and CI `research`. Testing a
benchmark's verdict rule is correctness testing of that rule; it is distinct
from the benchmark's timed kernel executions in category 11.

## 9. Benchmark-program construction and small correctness checks

**Entry point.** Root `make bench-programs` calls IO then compute
`programs-check` from their experiment directories. Combined local cost is
**133.85 s** at `f3858780`, with Rust already present and IO images invalidated.
The two children and their WF/native phase totals were not separately timed in
that run. CI has a separate `bench-programs` job on both platforms.

### 9.1 IO source construction and reference checks

- **Owner:** `research/experiments/io-completion-bench/Makefile`.
- **WF resources:** All **12** `programs/*.wf`: `many_files_loop`,
  `many_files_narrow`, `many_files_wide`, `many_files_wide8`,
  `read_heavy_narrow`, `read_heavy_narrow_4k`, `read_heavy_wide8`,
  `read_heavy_wide8_4k`, `pipe_relay`, `tcp_echo_server`,
  `windows_component_open`, `windows_runtime_mixed`.
- **Build:** Cargo refreshes `compiler/target/gate/whitefootc`, then
  `whitefootc -o $(BUILD)/checked_<name> programs/<name>.wf` performs proof,
  LLVM emission and native linking. These are **twelve WF native products**,
  not twelve Rust tests. Files depend on source, compiler and Makefile.
- **Run:** The twelve `checked_*` executables are **constructed, not executed
  by this target**. Their construction detects source/emission/link failures;
  host-specific runtime behavior is checked by other callers such as Windows
  IO-host tests. This target is not the full storage/network protocol.
- **Small native correctness path:** `ordinary-check` builds `gen.c`,
  `ordinary-caller.c`, `ordinary-caller.ll` and the real runtime via
  `../container-representation/native.mk`. It produces a generator and
  `ordinary-public`/`ordinary-body`, creates a four-file tree with a 16-KiB
  per-file bound, and executes both callers in `check` mode. Its
  `ordinary-build` prerequisite also builds `runner.c` into `runner`, although
  `ordinary-check` does not execute that timing runner.
- **Linux reference path:** `uring-check` builds `uring_echo_check.c`, which
  includes the reference `uring_echo.c`, twice with
  `WF_BENCH_URING_INLINE_SEND=0/1`. The two native executables execute **seven
  deterministic traces each**, asserting submission/completion/refusal
  behavior. They need Linux headers, but no real ring or network service.
  Two builds plus fourteen traces take approximately **0.582 s** in the
  Linux CI log. macOS does not run these Linux-only traces.
- **Resources/cost:** Rust/gate CLI, Clang, runtime sources, scratch storage,
  tiny generated file fixtures. No 512-MiB dataset or external scheduler
  dependencies. Separate standalone proof/emission of `read_heavy_wide8.wf`
  costs 58.29 s and peaks at 2.88 GB RSS; it is one indication of the source
  compilation cost, not a measurement of this whole target or a native read.

### 9.2 Compute lowering, publication and link witnesses

- **Owner:** `research/experiments/compute-bench/Makefile`,
  `programs-check`, `module-symbols-test.sh`, `baseline-entry-test.sh`.
- **Sources:** `programs/{mandelbrot,quadrature,records,fir,stencil,prefix,
  histogram,merge_sort,bfs,radix_scatter,range_split}.wf`: **11 sources**.
- **Build:** Refresh gate `whitefootc`. For ten kernel sources, emit
  `--par` and `--no-overlap` modules, construct their objects and actual native
  runtime objects. Compile a tiny C link witness with both modules and check
  strong kernel/runtime symbols. The resulting link executable has an empty
  `main`; **it is not a run of the kernel's workload**. `range_split` additionally
  gets two ordinary executable forms through `whitefootc`.
- **Run/check:** Inspect publication calls in parallel/sequential IR, symbols
  and actual-runtime linking; execute both `range_split` forms. The two shell
  self-tests check symbol validation and baseline-entry wiring. No five-kernel
  scoreboard or paired performance verdict runs here.
- **Resources:** Gate compiler CLI, Clang/cc/linker, `nm`/shell/text tools,
  scheduler/ordinary runtime sources and local WF files. The main
  `programs-check` path does not require building oneTBB/Rayon/Parlay.
- **Freshness detail:** `checked_*` are stamps depending on source, Makefile
  and compiler. When a stamp is current, the recipe's inspection/link checks
  are skipped; the `range_split` executions inside its recipe are skipped
  too. This is different from container families, which rerun cached images.
  The two explicit shell self-test targets still run. This existing behavior
  is described here, not selected as the policy for a replacement gate.
- **Cost:** Included in the 133.85 s combined local parent; no separate
  cold/warm phase total from that run. CI totals are in the entry-point map.

## 10. Repository, build-tool and test-runner checks

These check the validity of the project, its tools or its test collection.
They are listed individually because the old label `static` does not describe
what runs. In particular some spawn real subprocesses or execute Python tests.

| Category and source owner | Construct/run, inputs and output | Local gate elapsed |
|---|---|---:|
| Rust formatting: `compiler/Makefile` `format` | `cargo fmt --all -- --check`; reads Rust source and returns formatting status. No type check, test executable or WF program. Requires rustfmt. | 1.10 |
| Rust type/lint checking: `compiler/Makefile` `lint` | `cargo clippy --all-targets --locked --offline -- -D warnings`; checks library, bins and tests. Needs compiler/spec/generated inputs and Cargo artifacts; runs no Rust test cases. Cached in this measurement. | 0.13 |
| Rust API documentation: `compiler/Makefile` `docs` | `RUSTDOCFLAGS=-D warnings cargo doc --no-deps --locked --offline`; rustdoc generates `compiler/target/doc/` and checks doc warnings. Not Markdown lint; `doctest=false`, so no doctest executable. | 2.55 |
| Verification-process guard: `.github/test-run-check.sh` testing `.github/run-check.pl` | Shell/Perl subprocess fixtures check exit status, nesting, competing owners, concurrency defaults, timeout, signals and orphan cleanup. Requires process control and scratch storage; no Rust/WF compiler. Included in `repository-invariants`. | Part of 11.18 |
| Repository invariants: root `Makefile` | In addition to guard tests, compare `AGENTS.md`/`CLAUDE.md`, scan tracked files/paths for machine-local names. Uses Git/shell/cmp/search. Output is pass/fail, not a native binary. | 11.18 total |
| Immutable spec archives: root `Makefile` `spec-append-only` | Git diff against local `main` for modified/deleted released spec archives; no compilation. Needs a valid `main` ref. | 0.05 |
| Specification prose integrity: root `Makefile` `spec-prose-integrity` | Scan README/agent/docs prose for copied spec digests and stale active-version declarations; no compilation. | 0.08 |
| Design-tree linter: `design/skill/test_lint.py`, `design/skill/lint.py` | **17 Python unittest cases**, then inspect the design tree/amendments and change boundary against the review base. Requires Python, Git and tree files. No Rust or WF build. | 6.38; Python tests inside it 5.128 |
| Conformance structure/runner: `tests/conformance/test_runner.py`, `runner.py` | **25 Python tests**, then validate manifest/source correspondence, schema and rule coverage: 129/129 rules. Inputs are manifest, WF files and specification. No actual WF compiler verdict or native execution. | 0.21; Python tests inside it 0.087 |
| Library/integration collection: `compiler/Makefile` `test-partition` | Six `cargo test --profile gate --lib ... -- --list` selections, compare union/counts/nonempty sampling modules and the declared integration target list. Cold invocation constructs the **whole library-test executable**, then only lists tests. | 0.32 with Rust artifacts already built |

Root `make static` contains only repository-invariants, spec-append-only,
spec-prose-integrity and design-lint. `make -C compiler static` contains
format, lint, docs, the real spec checker (category 4.2) and direct C
completion checks (category 7). Neither alias means "nothing executes".
None of these counts is part of the 1662 library Rust tests.

## 11. Performance protocols

These commands can be long despite running no Rust unit-test suite. They
construct dependencies/programs/data, verify outputs, then repeatedly execute
native workloads and produce timing tables. Only a protocol with an explicit
regression decision supplies the automatic performance verdict discussed here.

### 11.1 Compute correctness across implementations and widths

- **Owner:** `research/experiments/compute-bench/{Makefile,harness.c}`,
  `backend_*.c`/C++ sources, `rayon/`, `programs/`, and dependency-build scripts
  in that directory. This uses the same WF kernel inputs as category 9.2,
  with real benchmark entry points and native reference implementations.
- **Build:** `make deps` obtains/builds pinned oneTBB, Rayon and available
  Parlay support; `make build` refreshes the WF compiler, emits WF forms,
  compiles C/C++/Rust backends and runtime objects, and links kernel images.
  These dependency builds are separate from the dependency-free compiler crate.
- **Run/resources:** `make verify` executes forms and widths against kernel
  oracles. Requires C/C++/Rust toolchains, CMake, pinned source/crate
  availability, native worker threads, scratch storage and sufficient CPU
  time for the workloads. A missing source checkout can cause network waiting
  before any compiler runs.
- **Cost, local separate protocol:** With compiler and pinned source checkouts
  already available but fresh dependency outputs: **13.78 s deps**, **9.45 s
  image build**, **70.07 s correctness execution command**. These are
  sequential command times under that specific artifact state.
- **Callers:** Automatic `compute-bench.yml` push jobs on Linux/macOS build
  and verify; root `make check` runs category 9's smaller witnesses instead.
  This broader correctness run executes actual kernels, unlike most of
  `programs-check`'s link-only witnesses.

### 11.2 Paired compute performance regression

- **Owner:** `.github/workflows/compute-regression.yml`, plus compute-bench
  `compare`, `verdict` and `verdict.awk`. The workflow checks PR eligibility,
  exports the merge base, builds both compiler/program arms and verifies both
  before measuring. It is separate from root `make check`.
- **Inputs/artifacts:** Merge-base and candidate compiler trees/executables,
  common pinned dependencies, matched WF/native kernel images, paired timing
  tables and a verdict. This is not `cargo test` and has no `#[test]` workload.
- **Run/resources:** Five paired measurement passes on the same Linux VM,
  worker widths 1/2/4, matched CPU placement/environment. The verdict's own
  crafted-table tests are category 8.3, not these workload executions.
- **Cost/result:** Whole CI job **350 s**. Merge-base compiler build 49 s;
  dependencies 27 s; both image sets 61 s; correctness verification 28 s;
  paired timing 152 s; remaining setup/other work is included in the total.
  Fifteen kernel/width blocks pass, with zero suspects and zero CPU reports.
- **Decision:** A kernel fails on greater than 3% paired median slowdown and
  at least four of five adverse pairs at two widths. A single-width signal is
  reported as a suspect; the CPU signal is report-only. Those are the existing
  rule's conditions, not a new policy chosen by this inventory.

### 11.3 Full compute comparison scoreboard

- **Owner/command:** Same compute-bench sources, `make compare` and table
  publication in `compute-bench.yml`, enabled there only by manual dispatch.
- **Run:** Five kernels, 139 implementation/width cells, five passes:
  **695 process invocations and 4170 recorded first/warm calls** in the local
  protocol. Results are a comparison table, not automatically the paired
  regression verdict.
- **Build/resources:** Same image/dependency prerequisites as category 11.1;
  repeated process/worker execution and stable measurement conditions.
- **Cost:** **229.62 s** local `compare` command after the above build/verify.
  The source-fetch attempt stopped after 459.41 s with only 0.19 user and
  0.08 system CPU seconds was network waiting before construction, not a
  slow kernel or slow Rust compiler.

### 11.4 IO many-files, read-heavy, pipe and network protocols

- **Owner:** `research/experiments/io-completion-bench/Makefile`,
  `linux-bench.sh`, the shared Linux/macOS `read-bench.sh`,
  `linux-net-bench.sh`, `windows-bench.ps1`, with `gen.c`, `runner.c`,
  `baseline.c`, `read_baseline.c`, `pipe_producer.c`, `pipe_harness.c`,
  `uring_echo.c`, `epoll_echo.c`, `netload.c` and `programs/*.wf`.
  Platform workflow callers are in `.github/workflows/io-bench.yml`.
- **Build:** Refresh gate `whitefootc`; C `-O2` constructs data generators,
  timing runners and native references; the compiler creates WF/native forms.
  On a cached invocation these images may be reused. This is not compilation
  of Rust unit tests.
- **Run/resources:** Generate real files, verify expected outputs, execute
  process timing rounds and inspect cache-policy probes. The many-files
  default uses 8192 files with a 16-KiB maximum; the measured read-heavy dataset
  is 512 MiB. The read protocol performs 32,768 positioned reads per invocation.
  Pipe tests require real pipes and a delayed producer; network tests require
  Linux event/ring APIs, loopback peers and load generation. Full uncached
  storage measurements need a host whose cache probes validate that label.
- **Many-files cost:** With the dependency fix, required image reconstruction
  128.19 s; warm `build` **0.22 s**, warm `verify` **6.46 s**; unchanged
  seven-measurement/two-warmup full command **70.62 s**, formerly 200.86 s.
  This saving removes construction, not native IO instructions.
- **Read-heavy cost:** Local full command **1288.52 s (21m28.52s)** with Rust
  already built; approximately 4 s native-reference/data preparation and
  349 s for eight WF/native images, remainder verification and repeated reads.
  Seventeen macOS or twenty Linux cells x four tables x seven measured plus
  two warmup rounds = **612/720 invocations** in the measured scripts.
  These protocol settings should not be confused with other Make target
  defaults in the same experiment.
- **Other boundaries:** Local pipe/network build-versus-run totals are not
  separately measured in this audit. The manual Linux network step below is
  whole-step CI evidence, not a native-only runtime.
- **Callers:** Full `io-bench.yml` is manual. Root gate only calls category
  9.1; automatic platform correctness is category 7. No full IO timing verdict
  is claimed as an automatic storage-performance regression check.

The completed manual IO workflow at **`35227ab3`**, an earlier revision, has:

| Manual job | Job elapsed | Main protocol work |
|---|---:|---|
| Linux many-files/network | 355 | File protocol 293; network 30; remaining setup/other work |
| Linux read-heavy | 3995 (66m35s) | Rust 36.70; image/data/verification about 570; uncached tables 1068/2239; warm tables 46/5 |
| macOS read and many-files | 1146 | Read protocol 929; many-files 199 |
| Windows measurement | 463 | Native measurement protocol including construction 418 |

The Linux 64-KiB table's post-probe refused the uncached label: a green workflow
does not make that table valid uncached evidence. These jobs were not rerun
for this classification or for a new one-hour data point.

## 12. Historical and other explicit experiment runners

### 12.1 Retained instrument self-tests

**Caller.** Root `make historical-tool-tests` is explicit reproduction, outside
root `make check` and automatic CI. Its retained purpose is to reproduce
completed research instruments, not to check the current compiler again.

| Directory and source form | Construction and execution | Resources |
|---|---|---|
| `research/experiments/frequency-study/`: `tests/`, `bounds-ir/`, `alias-versioning/`, `effect-attrs/tests/` | Python unittest discovery for miner/oracle/classifier behavior via `make check` | Python and synthetic fixtures |
| `research/experiments/frequency-study/reassociation/` | Its own Cargo crate: default-profile `cargo test`, Clippy and rustfmt | Rust, its locked crate inputs and separate Cargo target directory |
| `research/experiments/default-floor/tests/` | Python unittest discovery for generation/model-study instruments | Python and fixture data |
| `research/experiments/default-floor/utf8parse/{rust-baseline,harness}/` | Two separate Cargo manifests; default-profile `cargo test` builds/runs their own tests | Rust, locked dependencies and per-crate target storage |
| `research/experiments/default-floor/percent-decode/{rust-baseline,harness}/` | Two more Cargo manifests, also default-profile `cargo test` | Same kind of resources, separate target storage |

The retained reproduction command passed in **8.16 s with Rust artifacts
already present**. Cold per-crate compilation, per-suite execution and complete
individual case counts were not measured in that run. This default test
profile belongs to those external baseline/instrument crates; it is not a
second dev build of the production compiler. Current UTF-8/DEFLATE/wfgrep
compiler witnesses remain in the active categories above.

### 12.2 Other explicit experiment entry points found in the tree

The following are not reachable from the root/CI commands mapped here. They
are listed because an agent can still invoke them directly. Costs are
**unmeasured**, and presence of a Make target is not a claim that its historical
inputs still work with the current compiler. A budget listed in a recipe is a
ceiling, not an observed duration.

| Directory/entry | What builds and runs; input/resource boundary |
|---|---|
| `research/experiments/differential-fuzz/`: `smoke`, `campaign`, `probes` | Cargo release `wf-difffuzz` generator/oracle plus gate `whitefootc`; generate WF and compile/run lowering x worker x helper combinations, using scratch files and real IO. Smoke requests 20 accepted programs with a 600 s campaign budget; default full campaign requests 2000 with a 5400 s budget and four jobs. Neither budget includes proof that those are suitable local defaults. |
| `research/experiments/buffer-initialization-cost/`: `check`, `bench` | Direct `rustc` builds `runner.rs`; Clang builds `control.c`; **unprofiled `cargo run --bin whitefootc` builds/uses a dev compiler in separate target storage** for `drain.wf`; inspect optimized LLVM and execute native controls. |
| `research/experiments/wfgrep-baseline/`: `verify`/`check`, `bench`, `profile` | Direct Rust runner; generated filesystem corpus; ordinary/native output, raw/optimized LLVM and assembly. **Unprofiled Cargo compiler invocation uses dev**, potentially repeating expensive proof execution. |
| `research/experiments/wide-scan-lowering/`: `verify`/`check` and measurement targets | Rust runner, local WF forms, C/native comparisons, generated input data and LLVM inspection. **Unprofiled Cargo compiler invocation uses dev** and is separate from the guarded root gate. |
| `research/experiments/wfgrep-double-walk/`: `verify`/`check` | Gate `whitefootc`, direct Rust runner, multiple WF/native binaries and a generated corpus stamp; execute output verification. No measured complete duration here. |
| `research/experiments/park-on-miss-switch-cost/`: `run` | `cc -O2 -pthread switch.c` produces a native timing executable; run its historical switch-cost experiment. No Rust/WF test harness. |
| `research/experiments/port-study/wc-chunk-summary/`: `check`, `bench` | Historical Python prototype compiler invocation, WF/Clang products, Rust/C controls and Python algebra/output checks. The recipe refers to an old `prototype/democ` path; current compatibility was not validated. |
| `research/experiments/zlib-core-kernels/test_guarded_bit_window.py` | Python unittest fixture transformations and LLVM-string assertions via an old prototype compiler import, not current `whitefoot`; no current-toolchain execution evidence is claimed. |
| `research/experiments/zlib-core-kernels/compiler-prototypes/periodic/test_periodic_copy_experiment.py` | Python tests importing the adjacent prototype `democ.py` and a historical WF fixture path; experiment-specific lowering checks, not the Rust acceptance path. Current compatibility unverified. |

Additional Cargo manifests under `research/experiments/literal-line-floor/ceiling/`
and `research/investigations/proof-derived-parallelism/bench/rust/` describe
standalone performance/reference artifacts. Neither is selected by the current
root/CI test commands. Their mere presence is not an additional canonical
Rust test suite or a measured cost in the totals above. This inventory covers
the maintained root/CI collection and the explicit experiment entry points
identified here, not every executable snippet in historical research prose.

### 12.3 Explicit native development targets

`compiler/Makefile` also exposes `completion-core-read-stress` (build the core
probe, then repeat it 200 times), `completion-windows-cross` (construct and
inspect Windows PE/runtime objects with a cross toolchain), and
`completion-windows-wine` (run those products with Wine). They are outside root
`make check`; they need the corresponding compiler/emulator and scratch/host
resources. Complete costs were not measured. Cross-link/symbol success is
not a substitute for the real Windows tests in category 7.2. The sanitizer
targets have actual CI callers and measurements in category 7.1.

## Local entry points

This is the complete measured root gate order, with its contained commands
identified. Construction happens on demand inside the owning Cargo/Make
command; `build` and `test-build` are not extra unconditional steps before it.

```text
make check                                         786.97 s, measured f3858780
  1. repository-invariants                           11.18 s
  2. spec-append-only                                 0.05 s
  3. spec-prose-integrity                             0.08 s
  4. design-lint                                      6.38 s
  5. conformance                                     0.21 s (Python structure)
  6. make -C compiler check                         479.93 s, includes:
       format                                        1.10 s
       lint                                          0.13 s (cached Rust check)
       test-partition                                0.32 s (list only)
       test-unit                                   196.13 s (1588 tests)
       test-sampling                                29.27 s (74 tests)
       test-corpus                                 242.93 s (137 non-ignored tests)
       docs                                          2.55 s
       spec                                          0.34 s (real scanner tool)
       completion-test                               5.75 s (C builds/runs)
  7. research-tests                                  15.94 s
  8. bench-programs                                 133.85 s
  9. conformance-run                                118.79 s (native adapter)
 10. snapshot-run                                    19.21 s
```

These are outer command readings; wrapper/stage-transition overhead explains
the small difference from their sum. The indented compiler rows replace,
rather than add to, 479.93 s. The compiler test cases' own execution clocks
(196.09, 29.23, 241.74, 118.75, 19.17 s) exclude their outer command overhead.

| Other local entry | Exactly what it selects |
|---|---|
| Root `make static` | Root stages 1-4 only; includes shell/Perl/Python execution |
| `make -C compiler static` | Format, Clippy, rustdoc, normal spec checker, direct C completion construction/execution |
| `make -C compiler test` | Partition, ordinary library tests, sampling and bin/integration `test-corpus`; not the two ignored adapter executions |
| `make -C compiler build` | Construct gate compiler; do not execute its Rust tests or any WF program |
| `make -C compiler test-build` | Construct all gate test targets plus Cargo-selected bin artifacts; execute no Rust test cases |
| Root `make historical-tool-tests` | Only category 12.1's retained instrument tests |
| Compute/IO `compare`, `bench`, platform read/network scripts | The explicitly named category 11 protocols; not root gate aliases |

## CI entry points

Owner workflow files are under [.github/workflows/](../../../.github/workflows/).
Timings below are successful **`f3858780` jobs**. Each row is a separate VM/job;
job totals include setup and exclude queueing. Parallel jobs cannot be summed
as the workflow's user-visible latency. The local measurement host and the CI
hosts also have different execution/launch costs.

### Gate workflow

`gate.yml` has seven jobs on each of Linux/macOS. Each installs or records its
toolchain and constructs its own necessary Rust/native artifacts. There is no
cross-job sharing of the compiler built by `research`, `unit` or another job.
Cargo jobs are two; CI's test-thread count follows its dedicated host.

| Job and command | Linux total / main step / setup-other | macOS total / main step / setup-other |
|---|---|---|
| `static`: root `make static`, then `make -C compiler static` | 119 / 98 / 21 | 132 / 117 / 15 |
| `unit`: `make -C compiler test-partition test-unit` | 318 / 298 / 20 | 327 / 311 / 16 |
| `sampling`: `make -C compiler test-sampling` | 194 / 155 / 39 | 152 / 140 / 12 |
| `corpus`: `make -C compiler test-corpus` | 239 / 220 / 19 | 309 / 296 / 13 |
| `conformance`: root `make conformance`, `make conformance-run`, `make snapshot-run` | 134 / 110 / 24 | 174 / 158 / 16 |
| `research`: root `make research-tests` | 289 / 268 / 21 | 254 / 237 / 17 |
| `bench-programs`: root `make bench-programs` | 215 / 191 / 24 | 361 / 341 / 20 |

Construction versus execution inside those steps (Linux / macOS):

- **Static:** Clippy 16.30 / 24.65 s; rustdoc 8.23 / 11.38 s;
  gate spec-tool/library construction 51.13 / 55.58 s. Root checks and direct
  C probe construction/runs account for other work. Clippy/rustdoc work is
  not construction of another dev compiler used on WF programs.
- **Unit:** Library-test construction **99 / 137 s**; 1588 test execution
  **197.60 / 172.54 s**. Listing the tests requires that executable first.
- **Sampling:** The same library-test target is constructed on another VM:
  **130 / 123 s**, then 74 cases execute in **24.99 / 15.38 s**.
- **Corpus:** Rust bin/integration construction **51.10 / 59.35 s**;
  110 program tests execute in **167.97 / 234.38 s**, other 27 cases
  0.79 / 1.15 s. These case intervals include WF/native child work.
- **Conformance:** Ordinary library/adapter construction **46.77 / 75 s**;
  adapter execution **43.78 / 54.87 s**; snapshot target construction
  0.29 / 0.63 s and execution **19.20 / 25.18 s**, plus Python structure checks.
- **Research:** Compiler construction **53.42 / 63 s** occurs inside the
  proof-use target's 63.69 / 67.78 s. Container construction/verification is
  **203.01 / 166.58 s** on these cold VMs; the remaining small oracles also run.
- **Bench programs:** Compiler construction **41.19 / 81 s**, then IO/compute
  WF/native construction and the small checks described in category 9.

Checkout takes about 5-8 s per job, Linux toolchain setup 10-26 s and macOS
toolchain setup 1-3 s in this run. Setup-other is not all network waiting.
The log's "ten largest gaps" output is not per-test duration accounting.

### Other automatic workflows

| Workflow | Actual stages and resources | Observed total |
|---|---|---|
| `compute-bench.yml` on qualifying pushes | Pinned dependencies -> kernel images -> actual form/width correctness. Linux/macOS, C/C++/Rust/CMake and native threads. Timing/table steps require dispatch. | Linux 151 s; macOS 104 s |
| `compute-regression.yml` on eligible PRs or dispatch | Baseline/candidate compiler builds -> both image sets -> verify -> paired timing -> regression verdict; same Linux host | 350 s |
| `io-hosts.yml` push/dispatch, Linux | Required native ring + ordinary C harness + sanitizer variants | 43 s |
| `io-hosts.yml` push/dispatch, Windows | Native platform C probes + one gate compiler + actual WF platform programs | 207 s |

In compute correctness, dependency construction is 41/17 s, image construction
54/59 s and verification 24/13 s (Linux/macOS). Category 11.2 and category 7
give the paired-regression and platform step details. Full `io-bench.yml` is
manual; its earlier measured jobs are explicitly dated in category 11.4.

Evidence: [gate](https://github.com/mbbill/Whitefoot/actions/runs/34924294748),
[compute correctness](https://github.com/mbbill/Whitefoot/actions/runs/34924294745),
[paired regression](https://github.com/mbbill/Whitefoot/actions/runs/34924297881),
[IO hosts](https://github.com/mbbill/Whitefoot/actions/runs/34924294754),
[earlier manual IO](https://github.com/mbbill/Whitefoot/actions/runs/34912406944).

## Repetition and redesign questions

This classification separates **observed repeated work** from **equivalent
assertions**. The former is visible in sources/logs; the latter requires
matching the property, inputs, fault model and oracle. Two tests using the
same WF file can protect different compiler stages. Conversely, different
test names or executables do not establish different coverage.

| Observed overlap or extra work | Evidence and boundary | Question for the replacement system |
|---|---|---|
| One large library-test compilation for both cheap compiler assertions and expensive native tests | Categories 1-3 share the 1662-case Rust executable; 69.36 s cold unit construction in the sequential experiment | Which tests need compiler-private access, and which can use a prebuilt public compiler/runtime artifact without enlarging that unit? |
| Same library-test target built by separate CI unit/sampling jobs | Category 3 and gate CI: 99-137 s for each job's own build | Would one construction with reusable artifacts save more than the separate jobs gain in latency? Requires a measured runner/artifact comparison. |
| Eight Rust test executables plus normal tools | Artifact map and categories 4-6; normal and test versions share the ordinary library | Which physical test boundaries are useful? Fewer executables may reduce link/support work but do not remove WF proof calls automatically. |
| Normal grammar tool constructed while the gate uses its test version | Category 4.3, Cargo all-targets build | Does another normal executable link add coverage beyond the checked test implementation and type checks? Measure a leaner build selection. |
| Spec scanner checks active identity/integrity both in tests and as a normal tool | Category 4.2, `active_spec_has_complete_internal_integrity` and `run_gate` | Preserve negative-input detection and actual current-spec checking with an explicit account of their overlap. |
| Runtime object reuse stops at process exit | `compiler/tests/support/mod.rs` `OnceLock`; unit, sampling, programs and conformance are separate invocations | The same library executable is reused between local unit/sampling, but its in-memory cache is not. Is a dependency-correct native artifact across processes warranted? |
| Real WF sources analyzed in multiple categories | `semantic/tests/entailment.rs`, `backend/tests/cost_shape.rs`, `tests/programs/wfgrep.rs` use wfgrep for proof roots, IR cost shape and runtime behavior | Can one current compilation expose the required observations without caching or substituting a semantic verdict? Exact behavior/option differences must be accounted for. |
| Fixed-run repeated under ordinary/parallel corpus checks | Category 6: 90.222/85.890 s WF compilation in two distinct cases | Which proof/frontend work is shared and which lowering/result property differs? Separate compilation reuse from deleting a mode. |
| Snapshot judgments overlap broad semantic/conformance subjects | Category 5.3: 484 historical accept/reject comparisons, 19.17 s execution | Which rows catch a distinct current-spec defect? A historical verdict change alone is not necessarily a regression. No per-row redundancy proof exists yet. |
| Twenty-analysis and large native sampling matrices | Categories 1/3: 20 proof repeats, 36-image/144-run recursion case, worker/repeat loops | What failure does each repetition or matrix axis distinguish, and what cheaper falsifier preserves that evidence? Minimum counts unestablished. |
| C completion probes rebuilt on every invocation and repeated on Linux IO-host CI | Category 7 and `compiler/Makefile`; sanitizer/required-ring variants have additional conditions | Separate fresh construction from execution and distinguish duplicate ordinary assertions from sanitizer/platform-only evidence. |
| Research normal models plus `--test` products | Authority/foundation/families in category 8.2 | Which normal `main` enumerations add cases absent from `#[test]`, and which only repeat the same assertion? Counts alone cannot answer. |
| Lifecycle rebuilds seven accepted native images unconditionally | Category 8.2: about 12.5 s inner warm loop | Can source/compiler/invocation dependencies reuse images while still executing required cases and checking fresh negative verdicts? |
| Foundation layout emits timing during correctness; ordinary IO build includes a timing runner unused by its check | Categories 8.2/9.1: 0.224 s copy samples; `ordinary-check` never calls `runner` | Remove exploratory measurement from the correctness invocation and construct only resources its assertions consume. No change is implemented by this inventory. |
| Research checks construct inspection artifacts beyond their executed assertions | Foundation `large-result.ll`; dense `optimized*.ll`, `native*.opt.ll`, `native*.s`, `boundary256.ll`, category 8.2. WF `dense*.s` also feeds executed comparison images and is not in this extra-output subset. | Which products feed an automatic correctness/performance decision, and which can be constructed only for the inspection or measurement that uses them? |
| Compute checked stamps suppress recipe assertions and range-split runs when current | Category 9.2 `checked_*` recipes | Which checks are construction validation, and which runtime assertions should execute each verification? State the intended freshness rule explicitly. |
| IO gate builds twelve images without running their workloads; full compute correctness overlaps smaller construction witnesses | Categories 9/11 and the workflow map | Assign each required source/host behavior and performance verdict a named caller, and avoid both unnecessary reconstruction and untested runtime claims. |
| Explicit experiment paths still invoke an unoptimized compiler | Category 12.2 buffer-init/wfgrep-baseline/wide-scan Makefiles | Any retained current experiment needs an explicit compiler profile/resource contract; historical recipes should not silently become local verification defaults. |

These questions are the evidence gaps for a redesign, not an instruction to
delete checks or an assertion that the present architecture must survive.
The owner's stated selection criterion is correctness or performance-regression
detection. Each proposed retained/replaced/retired check can be evaluated
against the concrete input, artifact, oracle, resource and cost described here.
This document does not choose a new target layout or revise the live design tree.

## Unmeasured boundaries

- Exclusive elapsed time for frontend/proof versus ordinary backend cases
  within the two-thread 1588-case suite. Per-case sums overlap and shared
  initialization may be counted by both waiting tests.
- Complete per-target cold build/run splits for the six research subdirectories,
  individual timings for their 21 standalone Rust cases, and full native
  compile/run splits inside Windows/sanitizer jobs.
- The total number of all transient native images and child invocations across
  the entire gate. Selected helpers and detailed cases have counts; custom
  build/run paths are not all observed. A `#[test]` or WF-file count is not a
  substitute for this missing number.
- Coverage equivalence across library, conformance, snapshot, real-program,
  C runtime and research assertions. This inventory identifies intersections;
  it has not established a minimal replacement suite.
- Which rustc/LLVM pass makes the large optimized Rust units expensive;
  attribution of the unusually slow cold corpus context; the host service
  behind first-launch delays. No new causal claim follows from naming a stage.
- Full-duration measurements and current compatibility for explicit historical
  experiment targets outside the maintained root/CI collection.

The existing raw timing companion uses `NA` for unobserved phases and retains
failed attempts separately. The original report records measurement revisions,
host configuration, successful full gates and the pending verification-cost
amendment. A redesign should replace that uncertainty with specific evidence,
not relabel an unknown cost or an old verdict as a verified fact.
