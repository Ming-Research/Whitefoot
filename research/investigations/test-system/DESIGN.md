# Testing Whitefoot implementations and Whitefoot programs

## Status, question and rejection criteria

Research proposal, not an implemented test interface or an approval record.
The inspected worktree is `claude/test-system` at
`13bb0d572522c4c52342e87d90e50db7ffe90a79`. The R1 record is the local
`claude/natural-loop-r1` snapshot at
`44c72d1b53ba568ce06704953650ece90ccb7529`, compared with its supplied base
`026074111746b0cd4f869dfa2b02b1a8749e9be3`. No build, test, compiler run or
performance measurement was performed for this investigation. Counts below
come from reading manifests and git diffs, not from executing cases.

The owner approved Q145 A: R1 rejects an `if` or value `if` whose outcome
the specification's automatic derivation decides. The owner refused Q152's
user-visible opaque identity and directed that compiler testing add no
capability ordinary programs can use. The R1 design is available locally
through `git show 44c72d1b53ba568ce06704953650ece90ccb7529:research/investigations/redundant-tests/DESIGN.md`.
This exact git-object citation does not assume the local WIP was published.
This work takes those owner decisions as constraints. Q158 and Q159 below
are new, pending decisions.

The question is how to observe whether generated code implements Whitefoot
without writing a source branch that R1 forbids, and what separate testing
need remains for Whitefoot applications. The comparison is between external
oracles, private compiler instrumentation, runtime fixtures and new source
facilities. Reject a recommendation if it changes source acceptance according
to test intent, needs an oracle supplied by the implementation being tested,
loses a case's protected observation, or conceals a missing language proof.
The migration criteria below are prospective falsifiers, not completed checks.

The governing distinction comes from the
[constitution's safety boundary](../../../docs/constitution.md#safety) and
[SCOPE-2/SCOPE-3](../../../spec/kernel-spec.md#1-scope-and-conformance): a
source proof relies on the checker and compiler being correct. It does not
establish that those implementations are correct. A runtime result belongs
on the observed side of a compiler test, even when its source value is proved.
An erased invariant cannot replace that observation.

## Needs and current evidence

### Compiler, runtime and toolchain developers

The [existing verification decisions](../../../design/compiler/verification.md)
already assign cases by the contract they check. Keep those owners. This
proposal changes where a runtime oracle is evaluated, not the suite taxonomy
or the gate inventory.

The compute fixtures' C oracles are currently invoked from
[backend ranges tests](../../../compiler/src/backend/tests/ranges.rs) as well
as the separate performance runner; their location under program fixtures
does not imply that every caller lives under `compiler/tests/programs`.

| Family and repository evidence | Observation and independent oracle | R1 interference |
| --- | --- | --- |
| Conformance acceptance: [manifest](../../../tests/conformance/manifest.jsonl), [runner](../../../tests/conformance/runner.py), [native adapter](../../../compiler/tests/conformance/adapter.rs) | The complete ordinary source-semantic path accepts a specified program. The specification and the manifest's rule-grounded `accept` decide the expectation; running native code is unnecessary. | A formerly accepted fixture can contain a newly forbidden constant branch even without a runtime oracle. Preserve the feature it witnesses when migrating it. |
| Conformance rejection: the same corpus and adapter | A source rejection cites the manifest's particular rule. An unsupported capability, internal stop or timeout is not that rejection. The specification supplies the violation independently of the checker. | A decided branch can introduce OP-5 before the intended violation. Keep the original negative witness and its rule; do not relabel the case OP-5 to obtain green results. A case actually about the changed rule needs a separately justified expectation change. |
| Run-mode conformance: [integer example](../../../tests/conformance/cases/type1-pos-i32-unit.wf), [mixed-width fields](../../../tests/conformance/cases/x-struct-mixed-width.wf) | Compile through the ordinary target path, run with the manifest's arrangement, and compare the process exit status. Currently a WF comparison often compresses the computed value into success/failure; its literal expectation is source-authored, but evaluation of that comparison is also under test. | Direct collision: a proved comparison in `if` is rejected. Moving the expected value and comparison to the adapter avoids that dependency. Computation, effects and all safety obligations remain in ordinary WF. |
| Whole programs: [fixtures](../../../tests/programs/), [Rust integration tests](../../../compiler/tests/), [interpreter caller](../../../compiler/tests/programs/binary.rs), [parallel observations](../../../compiler/tests/programs/parallel.rs), [host driver support](../../../compiler/tests/programs/support.rs) | Complete outputs, protocols, filesystem effects and resource behavior. Oracles include specified literal outputs, mathematical results and independently written host computations. The quadrature case checks an analytic integral as well as agreement across execution policies; agreement alone would admit a shared error. Some programs instead decide success internally, such as [continue_interpreter.wf](../../../tests/programs/continue_interpreter.wf). | Internal decided result checks collide; external byte comparisons do not. Any ordinary branch elsewhere in a compiled source can still collide. Backend success does not imply the host adapter established valid arguments or respected ownership: those boundaries need their own review. |
| Native complete-result program oracles: [compute fixtures](../../../tests/programs/compute/), [FIR oracle](../../../tests/programs/compute/fir_oracle.c), [FIR adapter](../../../tests/programs/compute/fir_host.ll), [shared oracle boundary](../../../tests/programs/compute/oracle.h) | C obtains complete results from ordinarily compiled WF kernels. FIR compares every output and history value bitwise against direct and delay-line reference calculations, with independently stated small answers. Adapters handle private ABI, lengths and ownership; they must not compute the expected answer. | Host comparisons are outside R1. The source is still checked in full: `compute/fir.wf` and `compute/mandelbrot.wf` contain `command_smoke` self-checks, so using an external oracle does not by itself make the compilation immune. |
| Backend shape tests: [tests](../../../compiler/src/backend/tests/), [exact arithmetic](../../../compiler/src/backend/tests/arithmetic_obligations.rs), [dispatch](../../../compiler/src/backend/tests/match_dispatch.rs) | Rust asserts an emission obligation such as exact versus wrapping instruction flags, a dispatch shape or an ABI; some tests additionally run the generated program. The oracle is the selected lowering contract, checked in the emitted structure, with an independent runtime answer where needed. A prior compiler's output is not a correctness authority. | Rust assertions are unaffected. Embedded WF fixtures pass the ordinary checker and can fail before emission. Preserve the shape witness separately from any runtime result check; a result-only replacement cannot establish the old shape claim. |
| Performance: [test contract](../../../tests/performance/README.md), [construction](../../../tests/performance/Makefile), [measurement runner](../../../tests/performance/runner.c) | Complete-result C oracles first establish the same workload's answer; paired measurements compare compiler/runtime revisions. Fixed sample selection and the reducer decide a performance regression, with host controls and an inconclusive outcome when controls fail. Time is not a correctness oracle. | The C oracle and timing reducer have no R1 branch. WF kernels, including their smoke entry bodies, may still reject. Preserve workload, input, complete result and timed region; migrating source checks is no license to alter a benchmark. |
| Checker tests: [tests](../../../compiler/src/semantic/tests/), [conditionals](../../../compiler/src/semantic/tests/conditionals.rs), [arithmetic obligations](../../../compiler/src/semantic/tests/arithmetic_obligations.rs) | Rust checks source judgments, rule/location/payload, proof obligations and retained derivations. Expected judgments come from the specification; internal representation assertions come from their documented implementation contracts. No generated-code execution is required for these observations. | Rust assertions are unaffected. The checked WF input may contain a constant `if`, including a fixture intended to inspect a value initializer. Make that subject genuinely undecided under its interface when required; keep decided conditions in the R1 rejection tests. Do not replace a checked invariant with a runtime observation. |
| Stage-3 interpreter experiment: [CoreMark harness](../../experiments/match-dispatch/wasm/coremark.py), [stage-3 design](../match-dispatch/DESIGN.md#stage-3-a-wasm-interpreter-running-coremark), [recorded results](../../experiments/match-dispatch/RESULTS.md#stage-3-a-wasm-interpreter-running-coremark) | The Python host harness reads the guest's printed CRCs and score. It requires list `0xe714`, matrix `0x1fd7` and state `0x8e3a`; it also requires one final CRC across launches and engines. Fixed component CRCs are the reference oracle; cross-engine final-CRC agreement adds differential evidence, not an independent expected final CRC. The guest module is runtime input to the WF interpreter. | The Python comparisons and guest wasm branches are not WF source conditions. They have no direct R1 collision. Generated WF interpreter code still needs a separate audit. This experiment remains research, not a new canonical gate dependency. |

The current conformance manifest contains **311 accept, 915 reject and 581
run cases**, with no `unsupported` expectation. The runner's schema and native
types retain an `unsupported` kind, distinct from source rejection; unexpected
unsupported outcomes fail. Its Python `ADAPTER` is currently unset: Python
owns structure, coverage and schema, while the native Rust test in
[corpus.rs](../../../compiler/tests/corpus.rs) owns execution.

### What is known about the R1 migration size

The R1 design promises a `RESULTS.md`, but the inspected snapshot contains
only its `DESIGN.md` and `origin-publication.wf` in that investigation.
It contains no executed per-case census. The following is a **diff footprint,
not a count of proven R1 failures**:

| Family | Recorded changes from `026074111` to `44c72d1b5` | Actual R1 hit count |
| --- | --- | --- |
| Existing conformance accept / reject / run | 23 / 13 / 122 source files modified; 158 total. No existing `expect` object changed. | Unknown for each mode. Modified files include proof-bearing guards as well as result observations; a file may have several affected conditions. |
| New R1 conformance cases | 8 reject and 6 accept entries added. | New coverage, not existing cases hit. |
| Whole programs under `tests/programs` | 20 source files modified. No changes under `compiler/tests`. | Unknown; file count is not integration-test count. |
| Backend tests | No changes under `compiler/src/backend/tests`. | Unknown, not zero. Embedded/generated source must be checked. |
| Performance | No changes under `tests/performance`. | Unknown for the WF workloads; their external comparison code is unaffected. |
| Checker tests | `compiler/src/semantic/tests/conditionals.rs` changed. | Unknown for existing Rust test cases; one file is not one case. |
| Stage 3 | No changes under `research/experiments/match-dispatch`. | No direct R1 application to `coremark.py`; unknown for the generated WF interpreter. |

These figures were obtained by reading the base and R1 manifests and
classifying `git diff --name-only --diff-filter=M 026074111..claude/natural-loop-r1`
paths by their base manifest modes. Current-corpus totals were counted
separately at `13bb0d572`; they are not the denominator for the older R1 diff.
Neither a text search for `if` nor the changed-file count proves redundancy.

Several snapshot migrations add a WF `observe_condition` identity with no
postcondition, including the integer example and the IPv4 and base64 cases.
That spelling exploits information not crossing a call; it supplies no
independent oracle and is not the selected migration. Do not adopt those edits
merely because the manifest's expectations stayed unchanged.

To obtain real counts on CI after approval:

1. Pin the source population and compiler revisions separately. Compare an
   unchanged pre-migration corpus with ordinary R1 checking, separating known
   unrelated branch/specification changes; do not call every failing case an
   R1 regression. Record the active specification identities.
2. Inventory manifest IDs, program test invocations, backend/checker Rust
   test names and their embedded/generated sources, and performance workload
   sources. Count a shared source once in a source table and every affected
   owning test in a separate test table. Preserve selected configurations.
3. Record OP-5 `RedundantCondition` locations and truth dispositions. Report
   distinct cases and conditions separately. A first rejection masks later
   conditions; an earlier non-OP-5 error makes R1 reachability unknown.
   Preserve each original source while reviewing repairs and later findings;
   never disable a rule to enumerate them. Tally unresolved cases explicitly.
4. Audit the stage-3 generator and its exact external interpreter inputs in
   an explicitly requested research CI run. The generator alone is not the
   full program. Do not count Python's CRC checks as WF conditions or make
   correctness CI import this experiment to obtain a census.

### Writers and owners of Whitefoot applications

Halo-wf, Firn-wf and Snowghost-wf were named as real projects; their repositories
are unavailable here. No claim below counts their tests or identifies a
particular unproved property in them. These needs follow from the language's
guarantees and limits, not their unseen implementations.

| Property or task | What proof already establishes | What testing still serves, or why neither is needed |
| --- | --- | --- |
| Memory access, initialization, ownership, integer domains and race freedom | Required safety for every admitted execution under SCOPE-3, including runtime inputs; overflow policy is explicit. | Application tests need not recheck each proved bound or synthesize impossible-error branches. Compiler/runtime qualification still tests implementation of these guarantees. An application test is no substitute for a missing proof. |
| Written contracts, type and loop invariants | The admitted propositions at their specified boundaries. A proved index relation is not merely a sampled claim. | A second runtime assertion of exactly that proposition adds no application-level guarantee under the same trusted base. Testing the toolchain that implements it is a distinct need. |
| Correct intended answers | Only what the writer actually states in the admitted fact language. [FN-9](../../../spec/kernel-spec.md#8-functions-generics-contracts) has restricted integer relations and routes; range facts are bounded and serve their specified consumers. | Test a decoder's bytes against format vectors, a command's response against its protocol, or a renderer's observable output against an independent reference. An implementation can be safe and consistently satisfy an incomplete or mistaken contract while returning the wrong answer. |
| External input behavior | Safe handling and the written contracts on reads/arguments, not the application's desired interpretation of every byte sequence. | Deterministic valid, invalid, boundary and regression fixtures check parsing, error classification and output. Invalid input is ordinary behavior to specify and test, not a way to request an unsafe operation. |
| I/O, persistence and host effects | Ordinary ownership/effect/contract obligations, including required releases and the specified ordering boundary; linked implementations are trusted to honor them. | Host tests check exact bytes, files after restart, short transfers, errors and protocol interactions. They test the external contract and trusted implementations. Ordering expectations must follow footprints and protocol requirements, not assume an order for independent concurrent effects. |
| Properties outside the proof language | No general theorem that an arbitrary algorithm, visual layout, float error bound or protocol trace matches the intended model. A function's termination is not generally proved. | Compare numerical outputs with an independently chosen error bound, render fixed inputs for regression, and check bounded interaction traces. Cases are evidence for sampled behavior, not a universal proof. A required safety fact outside current proof support remains a language/compiler design issue. |
| Speed, latency and resource cost | Current proofs do not establish general throughput, deadline response or complete memory/termination budgets. The [README](../../../README.md#beyond-memory-resources) distinguishes current facilities from planned fixed-resource guarantees. | Measure selected workloads separately from deterministic functional verdicts. A watchdog reports a stopped test, not proof of nontermination or a changed source verdict. Heap/stack exhaustion cannot become an invented recoverable source outcome. |
| Choices with no observable requirement | A private variable's spelling or an incidental optimizer choice is not an application behavior. | Neither an application proof nor a regression test is needed just to preserve it. A backend shape test is justified only by a concrete compiler contract or performance requirement. |

Rigor removes a large class of application assertions, not the gap between
intended requirements and written contracts. It also does not remove the need
to qualify the trusted base. There is no evidence here for a new writer-visible
test language. Existing ordinary functions, module entries, input objects and
host-observed results are enough to state the concrete testing needs above;
their convenience and completeness for these three projects have not been
experimentally established.

## Requirements

1. **No testing privilege in source.** Add no `std` operation, prelude name,
   annotation, implicit intrinsic, special source filename or shipped compiler
   flag that lets an ordinary program hide a fact or bypass a judgment.
   Reusing ordinary output is not adding a testing capability.
2. **One source judgment.** R1 and every safety, formation, ownership and
   contract rule apply identically to fixtures and applications. A compiler's
   test-build flag cannot turn an invalid fixture into a conformance pass.
3. **Independent expected result.** State expectations from the specification,
   a reference algorithm/format, or an independently derived literal. Do not
   ask the WF program, its proof record or a previous compiler build to supply
   its own expected answer. Differential agreement is supporting evidence.
4. **Preserve observation.** Keep all result components, intermediate
   observations that mattered, executed effects, ownership/release checks and
   intended code-shape coverage. A successful process after deleting its
   comparisons proves none of those values correct.
5. **Deterministic functional cases.** Fix fixtures, seeds, invocation bytes
   and expected observations; isolate host state, use explicit handshakes
   rather than elapsed sleeps as event-order evidence, and fail incomplete or
   malformed observations. Do not retry until a case happens to pass. Timing
   samples themselves vary: performance retains its explicit controls and
   inconclusive outcome, never a timing-dependent language verdict. Existing
   clock/order weaknesses in [the TODO](../../../docs/todo.md#platforms-and-host-interfaces)
   are not claimed solved by moving an oracle.
6. **No concealed gap.** When removing a guard loses a needed fact, minimize
   the semantic witness and close the missing rule or implementation. Do not
   add runtime data, opaque calls, `match`, checked arithmetic or a different
   spelling solely to evade the missing proof or R1.
7. **Ordinary compilation and bounded host execution.** Keep complete source
   checking, target qualification, runtime linking and existing process
   deadlines. Test transport cannot fabricate validity, ownership or a callee
   precondition. Keep formal cases independent of research inputs.

## Alternatives

| Alternative | What it supplies | Cost and limits | Disposition |
| --- | --- | --- | --- |
| **A. External oracle over ordinary observations** | WF computes the subject result; the manifest or host harness compares its returned status, complete output or existing native-adapter result. | Requires preserving every observation and sometimes adding output plumbing. A byte exit status cannot carry a wider result; I/O can perturb a shape or timed region. Does not repair source proof gaps. | **Recommended default.** Separates source proof from evidence about its implementation without changing source rules. |
| B. Compiler-internal facility under a harness-only flag, absent from the specification | Could insert observation probes after ordinary checking or expose internal values to a private test driver. | A hidden CLI flag in a shipped compiler is still usable by ordinary programs. Even a genuinely test-build-only R1 exemption changes the judgment and cannot validate conformance. Post-check probes preserve acceptance but cannot accept the original forbidden `if`, and can perturb optimization. | Do not add now. Existing Rust inspection and private host adapters cover concrete internal observations. Reopen only for an observation neither can express, with no altered acceptance. |
| C. Harness supplies values unknown to the checker at runtime | Fixed argv/stdin/files or native call arguments exercise real input-dependent paths; compile once and run a fixed input matrix. | Need a real input boundary and valid contracts. Making an expected constant obscure changes what the fixture establishes, may lose constant/callee-fact coverage, and still does not independently check the result. | Complement A when runtime variability is the subject; not an R1 repair by itself. No random entropy, clock reads or arbitrary opaque providers. |
| D. Source-level `test` / `assert` / test-only declaration | Could discover tests or request a runtime comparison from WF. | An erased form does not test generated execution. An executable form duplicates ordinary control or needs an exception to R1; special intent or failure semantics would enter the language. | Refuse a privileged form; defer even ordinary discovery sugar until a writer need establishes its benefit. |
| E. User-visible or fixture-local opaque identity | Keeps the current `if` by withholding facts across a call. | Depends on information loss, adds no independent oracle, may still be optimized away, and can sever a proof needed later. A generic or enum disguise has the same defect. | User-visible form refused by Q152; do not adopt the snapshot's local `observe_condition` as the substitute. |
| F. Replace the test with an invariant, or delete its comparison | Proves an admitted source property or removes a redundant source form. | No evidence about emitted execution; an unconditional exit 0 can pass a broken program. | Correct only for a pure source-proof subject, not as migration of a runtime value observation. |
| G. Private typed result capture at an ordinary native boundary | Existing C/LLVM adapters capture full scalars, tags, lengths and aggregates outside WF, without output formatting. [PROG-3](../../../spec/kernel-spec.md#11-programs-and-modules) already puts result interpretation outside the source call. | Private ABI maintenance and host obligations; cannot substitute for invocation/I/O coverage. Building a universal typed observer or stable export ABI would add machinery without a present need. | Use as A's existing transport for compiler implementation and compute tests. Portable conformance defaults to ordinary status/output, not compiler-specific symbol names in its manifest. |

External observation is not a promise that an optimizer executes every source
operation. A legal constant-folded implementation can return the right answer
without running an add or interpreter loop. That limitation already applies
to internal self-checks, which can disappear altogether. Preserve constant
cases for their specified semantics; use real runtime inputs and emitted-code
assertions when the protected property requires a dynamic instruction or
dispatch path. Do not insert an optimization barrier merely to keep an
incidental instruction alive. The
[backend-fact decision](../../../design/compiler/backend-facts.md) still
allows only proved facts with complete target mappings.

## Recommendation and migration contract

Choose A, reusing G where it already fits, and C for cases whose subject is a
runtime-dependent path. No compiler privilege or language amendment is needed
for that choice. The R1 amendment and its upstream proof prerequisites remain
separate. Do not create a common assertion framework spanning all suites;
each existing owner already knows how to interpret its observations.

### Source case and result transport

Keep the calculation and send its value to the observer. These are fragments
illustrating the design, not complete, compiler-checked programs:

```wf
let r = run(code: &code, start: 0_u64, seed: 0_u64, steps: 1000_u64);
if r == 3000_u64 {
  return exit_status(code: 0_u8);
}
return exit_status(code: 1_u8);
```

becomes the same computation followed by ordinary delivery of **all of r**.
For a process-output case, deliver its eight little-endian bytes and complete
normally; the proposed manifest expectation is:

```json
{"kind":"run","exit":0,"stdout":"b80b000000000000","stderr":""}
```

The host expects 3000 because the fixture executes 1000 additions of 3, not
because the checker reports that value. The bytes are a portable observation
encoding, not a native-memory dump. A private native test adapter may instead
capture `u64` directly and compare it with 3000 on the host. It must preserve
the normal source and lowering path, satisfy the called function's entry
requirements, and transfer/release owned results correctly.

Use the existing `expect.exit` with a directly returned computed byte when
the subject is itself a `u8`. For example, `return exit_status(code: byte);`
can be checked by `{"kind":"run","exit":42}`. Do not truncate a wider
integer to use this route, even when its expected value fits: a generated
result off by 256 must remain detectable. In the existing i32 conformance
example preserve the i32 operation and observe four bytes, `2a000000`, not a
new u8 computation. Preserve signed bits, widths, enum tags and relevant
lengths; observe all previously tested fields in a fixed order. A checksum
is not a lossless replacement for full-result comparisons. A preexisting CRC
test keeps its expressly weaker CRC contract.

If the original case intentionally observes only a Boolean result or sign,
its host oracle may check that property of the delivered value; do not silently
promote it to a stronger full-value requirement without a specification-grounded
reason. Conversely, calculating `r == expected` in WF and disguising its
control flow is not this migration: the compiler would still evaluate its own
oracle. Ordinary representations of an actual Boolean subject are distinct
from that disguise.

An output writer is ordinary checked WF using
[Inputs and ExitStatus](../../../lib/std/process/module.wfm) and
[write_once](../../../lib/std/io/module.wfm). It must handle partial writes
and errors according to that interface; `Ok(next)` does not promise the whole
buffer was written. Use an existing adequate writer or ordinary helper source
owned by the tests, with no new `std` API. Every helper goes through the same
checker. A transport proof that cannot be written is a witness to examine,
not permission for a privileged emitter. A native adapter is preferable for
an existing pure shape/compute case whose observation would be distorted by
adding I/O. No new globally shipped C export surface is proposed.

### Runner and adapter

For conformance, propose the smallest necessary schema change: `run` keeps
its required `kind` and `exit` and admits optional `stdout` and `stderr`,
each a lowercase, even-length hex string. An absent field adds no assertion;
`""` requires empty bytes. A migrated result observation must have an explicit
expectation. No implicit golden generation, numeric coercion, newline trimming
or unordered comparison occurs. Existing `arrange` continues to supply argv,
stdin, files and redirections; it supplies inputs, never an expected answer to
be checked inside WF.

[runner.py](../../../tests/conformance/runner.py) must document and validate
the new expectation, make its abstract match rule include the bytes, and keep
`declared_verdicts`/`verdict_diff` reporting every change to the complete
expectation. Its unset Python adapter remains explicitly unavailable; this
work does not create a second compiler implementation or revive that hook.
[corpus.rs](../../../compiler/tests/conformance/corpus.rs) must parse the same
fields and reject malformed/unknown expectation fields instead of silently
dropping an observation. Accept/reject source verdicts keep their existing
meaning and compilation boundary.

The native [adapter](../../../compiler/tests/conformance/adapter.rs) currently
drains outputs, deletes the invocation directory and returns only the status.
It would retain captured bytes, collect redirected sink bytes before cleanup,
and match status and every declared output. For a redirected stream the
expectation names the complete bytes of its destination sink. If stdout and
stderr share a sink, they denote the same combined bytes, not reconstructible
separate transcripts; reject incompatible expectations on that one sink.
An I/O-order case keeps that arrangement and its order-sensitive expectation.
General filesystem-state oracles continue in their existing host program
tests; no generic file-snapshot schema is proposed without a consumer.

Keep one owned child, drain both streams, wait for completion, and report a
deadline, signal or abnormal compiler/link/invocation stop distinctly from a
matching run. Missing, extra or differing bytes fail the observation even if
exit status is 0. Failure reports identify the case and the differing field
and byte offset. No expected output reaches compiler acceptance or lowering;
changing only an expectation must leave the compiled source/artifact unchanged.

The [runner integrity tests](../../../tests/conformance/test_runner.py) and
native schema/matcher tests must cover nonempty and empty output, malformed
hex, unknown fields, missing/truncated/extra/wrong bytes, and shared redirected
sinks. A synthetic producer that prints correct bytes then exits unsuccessfully
must fail. These are future implementation checks, not tests run in this task.

### Coverage-preserving migration and CI evidence

For every migrated case, the implementation PR records one row with its ID,
owning rule or implementation contract, old calculation and observation,
new observation/encoding, oracle derivation, and meaningful wrong result.
This is a migration review record, not a new permanent rule-to-test database.
The reviewer reads both source versions and the manifest/host assertion:

- The subject operation, types, relevant control edges, dataflow and effects
  remain; every old field/intermediate check has a host observation or an
  explicit independently justified replacement. If running onward after an
  old early failure would hit a proved partial operation, prefer a suitably
  placed native observation or a split case preserving the original subject;
  do not add a forged safety proof or silently omit that failure observation.
- An `if` whose branching is itself the subject still exists with genuinely
  undecided operands, and its fixed host input matrix distinguishes its
  alternatives. A decided test required by R1's negative coverage remains a
  rejection fixture. Removing the branch is appropriate only when the branch
  is the obsolete result comparator, not the language feature being tested.
- A guard can both check a value and establish a downstream fact. The R1
  origin-publication witness shows deletion losing an expanded-origin
  premise in base64 and IPv4. Externalizing an oracle does not resolve that
  gap. The owner approved its general repair, origin transport (Q151 B,
  numbered Q149 in the first R1 draft this investigation read), which must
  precede migration of those consumers; keep their contracts and their
  original witnesses.
- Native adapters only marshal actual values and control the existing host
  environment. They do not substitute expected values, repair generated code,
  invent a callee's postcondition, or change the algorithm/timed region.
  Conformance sources remain independent of a particular compiler's ABI.
- An output fixture is not automatically evidence that a particular dynamic
  instruction executed. Retain the necessary shape inspection or runtime-input
  witness for such a claim. An unused `command_smoke` body still owes source
  checking; preserve its small observations in the host oracle before removing
  its internal comparisons.

On CI, start with a small representative migration: a native byte result,
a wider integer, multiple fields, a branch whose behavior is the subject,
a redirected-sink case and a native-adapter result. Measure that sample's
check duration before selecting the wider batch, following the owner rule.
Do not run this sample locally. For new observation machinery show rejection
of the wrong expected value and of missing/malformed output; for each newly
protected condition supply a representative fault. In particular 3000 and
3256 share their low byte but have different eight-byte results, so an oracle
that only checks the low byte fails qualification. Where corruption of the
under-test result is used, inject it in a disposable output/native artifact
on CI, never by adding an acceptance bypass to WF.

Finish the real-hit census, migrate by family, and run the ordinary selected
compiler, program, conformance and integrity groups and then the exact-revision
root gate on CI. Keep performance qualification separate and use its existing
workload/oracle checks; no new timing campaign is part of a correctness run.
Stage 3 remains an explicitly requested research run. The implementation
review records actual tested revisions, changed conformance expectations and
the fixed findings. Green verdicts alone cannot detect an emptied positive
case, as the runner's `verdict_diff` comment already explains.

Conformance source/manifest changes belong to the R1 specification migration
with its exact-revision selection ground. This research task changes no
specification rule, verdict, compiler, runner, fixture or gate. Do not edit
active expectations on this branch merely to make the proposed schema concrete.

## Decision cards

---

**Q158 — Should compiler runtime tests put their oracle in the host harness,
using ordinary result delivery and existing native adapters?**

**Background.** A source `if r == 3000_u64` is invalid under approved R1 when
automatic derivation decides it. The proof relies on compiler correctness;
it cannot check the generated program. Today conformance compares only exit
status, while compute and parallel program tests already compare complete
external results. Minimal migration: the same `run(...)` produces eight
bytes `b80b000000000000`; the manifest, not WF, demands those bytes and a
normal exit. An erased invariant or unconditional exit 0 loses the runtime
observation. The older R1 snapshot's 122 modified run-case files indicate
scope to audit, not a completed failure census.

**Options.**

- **A — Recommended: external oracle.** Use ordinary status/output for
  portable conformance and existing private typed adapters for compiler and
  compute tests; add exact byte expectations to the runner/adapter. Use fixed
  runtime inputs when they are the subject. Cost: per-case observation review,
  output/schema work and adapter maintenance. Risk: lossy transport or legal
  optimization can weaken a claimed observation; the migration checks above
  distinguish those claims. No source rule or public testing capability changes.
- **B — Private compiler test facility.** Add test-build-only observation
  instrumentation, never a shipped hidden flag. Cost: a maintained internal
  path and proof that it leaves source judgments and subject lowering intact.
  Risk: R1 bypass would invalidate conformance evidence, while post-check
  instrumentation still needs valid rewritten fixtures. Not recommended without
  a concrete observation A cannot provide.
- **C — Keep WF self-checks using runtime inputs.** Move data behind a real
  invocation boundary and keep ordinary branches. Cost: input plumbing and
  re-justifying each changed witness. Risk: loses constant/proof coverage and
  leaves the comparison itself compiled by the toolchain under test. Useful
  for genuine input-dependent behavior, insufficient as the common solution.
- **D — Source test/opaque facility.** Adds a name, construct or source-rule
  exception so existing comparisons remain. Cost: permanent language and
  trusted-base surface. Risk: ordinary programs can use compiler-test powers
  or acceptance depends on intent. Conflicts with the owner's constraints and
  Q152; not recommended. Erasing the check instead supplies no runtime evidence.

**Confidence 4/5.** Existing external-result tests and the source/host boundary
support A. No migration was compiled here. A minimal fixture whose required
observation cannot cross that boundary without changing its subject, or an
ordinary transport that exposes a genuine language gap, could require revising
the transport design. It would not justify weakening R1. Origin transport
(Q151) remains an independent upstream prerequisite where its witness
applies.

---

**Q159 — Does Whitefoot need a new user-facing test facility now?**

**Background.** Proof establishes safety and written admitted contracts, not
all intended outputs, host behavior or performance. For example, a safe decoder
may produce the wrong bytes for a fixed packet; a host fixture comparing its
ordinary output with a format vector detects that without any test keyword.
The three downstream projects have not been inspected, so their convenience
needs cannot be inferred from the compiler corpus.

**Options.**

- **A — Recommended: no new language or standard-library facility now.** Use
  ordinary functions/entries, project-owned deterministic fixtures and external
  result comparison. Keep provable safety obligations as proofs. Cost: some
  host orchestration and explicit result delivery. Risk: repetition or awkward
  private-component setup in real projects, not yet measured; revisit on a
  concrete case. This does not defer application testing itself.
- **B — Add ordinary test organization tooling now.** Discovery or a runner
  selects ordinary entries with no semantic privilege. Cost: a convention and
  maintained tool before downstream requirements are known. Risk: selecting
  the wrong packaging/fixture interface; it would not solve R1 or proof gaps.
  If later evidence warrants it, this host-side organization is the smallest
  candidate, ahead of any new source construct.
- **C — Add source `test`/`assert` semantics now.** Cost: grammar, failure and
  execution semantics plus specification and conformance work. An erased
  assertion adds no execution evidence; an ordinary executable one repeats
  control; a privileged one violates the constraints. No demonstrated need
  selects it.

**Confidence 4/5.** The language boundary explains the remaining needs and
ordinary observations already serve concrete repository consumers. Confidence
about downstream ergonomics is limited by the unavailable projects. Reopen
with a minimal ordinary test that cannot express a real observation, or a
documented repeated orchestration burden; a missing required proof first
reopens the relevant language design, not a testing exemption.

---

## Proposed design-tree node

Candidate home: `design/compiler/verification/test-oracles.md`, a child of
the existing verification owner. The following text is proposed under Q158,
not installed as a live implemented rule by this research-only change. Add it
with the approved migration and implementation; the parent's suite ownership,
ordinary acceptance and research boundary remain unchanged. Q159 A introduces
no user-facing mechanism and needs no language node now.

> Decision: Compiler runtime tests compare ordinary program observations with expectations held by the host harness, using lossless delivery of every value their oracle needs and independent specification or reference grounds, because a source proof relies on the compiler whose generated behavior the test must observe, instead of encoding an automatically decided result comparison as Whitefoot control flow or adding a source testing privilege. The [test-system investigation](../../../research/investigations/test-system/DESIGN.md#recommendation-and-migration-contract) distinguishes runtime observations from source-verdict and backend-shape obligations.
>
> Rejected:
> - Compiler-test modes that change a source judgment: rejected because their accepted fixture would not establish conformance of the ordinary compiler path.
> - Opaque identities used to retain result-check branches: rejected because withholding facts supplies no independent oracle and can remove evidence another operation requires.
> - Erasing a runtime value observation in favor of a proof: rejected because the proof does not check the implementation of its trusted compiler and runtime.

The relative link above is written for the candidate node's location, not for
this investigation. No design or specification approval log is written before
the owner rules. This proposal adds no token, spelling, source exception or
normative rule; OP-5's before/after change is owned by the separate R1 work.

## Dispositions and remaining work

- **Q145 A:** approved input direction; unconditional R1 stays.
- **Q152:** refused input direction; no user-visible opaque identity is proposed.
- **Q158 / Q159:** pending owner decisions; recommendations A / A.
- **Q151:** the proof-transport gap of the R1 witness (numbered Q149 in the
  first R1 draft); the owner approved option B, origin transport, which is
  being implemented separately. It stays a prerequisite for the affected
  guards; no workaround is claimed.
- **R1 hit census and migration qualification:** deferred to CI after the
  design decision. The static footprint is recorded above; zero edits in a
  family mean no failure count was recorded, not that it is unaffected.
- **CoreMark process outcome:** source inspection found `launch` discards
  the subprocess return code and reads stdout alone. Correct-looking CRCs
  can therefore accompany a failing process without failing this check.
  Recorded in [the verification TODO](../../../docs/todo.md#verification-tooling)
  with repair and falsifier; no code was changed. This observation qualifies
  the current experiment's oracle, not the correctness of any recorded run.
- **Output-only adapter status, wide results and shared sinks:** addressed
  in the proposed migration contract; implementation remains deliberately
  outside this research task.

The document is the research record for deciding Q158/Q159 and reviewing their
migration; its home is this investigation. Keep it as evidence, with future
implementation results explicitly distinguished from the proposal. No helper
script or unconsumed test infrastructure is added by this work.
