# Unwinding through a waiting function's resume code

This protocol is written before collecting data. No compiler acceptance,
elapsed time, unwind-table result or profiling result has been observed for
this witness. The temporary workflow must supply that evidence in CI.

Firn reported that `perf record -g --call-graph dwarf,16384` on its LTO-built
server, compiled with release `wf-af7665be672d`, stops at a Whitefoot waiting
function's resume frame. An ordinary callee's parent appears as `[other]`.
Missing or incorrect unwind information, continuation storage outside the
native stack, and an insufficient 16 KiB stack copy are candidate explanations,
not established causes. This experiment asks which of those explanations a
small program supports. It does not establish behavior for every Firn call
site or reproduce Firn's exact compiler revision and server binary.

## Witness and source observations

[`witness.wf`](witness.wf) has a waiting `main` that calls the waiting
`awaitable_loop`. The latter requests a 20 ms sleep before running its loop,
so hot work follows a waiting host operation. Scheduling can delay submission
until the deadline has passed; source alone therefore does not establish that
every execution actually parked. Each batch calls the ordinary `ordinary`
function with recursion depth one. Its base case calls `hot_leaf`; after the
recursive result returns, the outer call invokes `hot_leaf` again and combines
the results. Both returns consume the leaf result after its call. The leaf
performs dependent wrapping integer mixing with three stages across four
words, for 1,000,000 iterations. The final checksum is written as four
little-endian bytes to standard output; normal completion returns status zero,
and an output error returns status one. The arithmetic is intentional modular
mixing, rather than an unproved overflowing operation.

No extra invocation arguments selects 100 batches, totaling 200,000,000 hot
iterations. Supplying N extra arguments selects N batches, totaling 2,000,000
times N hot iterations; their contents are ignored. One extra argument is the
small timing sample. Argument count also seeds the checksum. CI times the
sample and selects a batch count estimated to take 2–5 seconds, recording the
selected count and realized duration. This duration is a target, not a
measurement, and the calibrated workload may differ from the default 200
million iterations. Every profiling mode uses the same executable and argument
vector.

The source form is `waits` after the effect row, defined by [WAIT-1] in the
[active specification](../../../spec/kernel-spec.md). [WAIT-2] runs an
unspawned waiting call in its caller's context; [WAIT-3] describes spawning,
which this witness does not use. The sleep follows the maintained examples in
[`deadlines.wf`](../../../tests/programs/deadlines.wf), including explicitly
closing its cancellation watch. The integer work uses the wrapping arithmetic
and rotation forms exercised by
[`sha256_abc.wf`](../../../tests/programs/sha256_abc.wf).

The compiler emits waiting functions as LLVM switched-resume coroutines. The
ramp allocates a continuation frame from the context's arena, copies arguments,
and initially suspends
([frames.rs:4](../../../compiler/src/backend/emitter/frames.rs#L4),
[frames.rs:132](../../../compiler/src/backend/emitter/frames.rs#L132)). A
waiting call resumes its callee immediately before suspending itself
([frames.rs:317](../../../compiler/src/backend/emitter/frames.rs#L317)); the
completion path similarly resumes the stored parent
([frames.rs:188](../../../compiler/src/backend/emitter/frames.rs#L188)). The
emitter documents that LLVM coroutine splitting turns those transfers into
tail calls, avoiding growth of the native stack
([frames.rs:11](../../../compiler/src/backend/emitter/frames.rs#L11)).
Continuation state and a parent link in an arena are not native return-address
frames. An ordinary call made while resume code executes still needs its
physical resume caller to be unwindable. Recovering that caller and recovering
every logical waiting caller are different observations.

The waiting signature adds `presplitcoroutine`
([frames.rs:483](../../../compiler/src/backend/emitter/frames.rs#L483)). Every
emitted definition uses attribute group zero
([emission.rs:413](../../../compiler/src/backend/emission.rs#L413)), which the
emitter defines with `probe-stack`
([emitter.rs:694](../../../compiler/src/backend/emitter.rs#L694)). These paths
do not explicitly request `uwtable` or a frame pointer. That absence motivates
inspection of the generated resume function's actual FDE and prologue; it does
not prove LLVM emits no unwind information. `nounwind` describes exception
behavior and, by itself, would not establish the absence of unwind tables.
The experiment uses compiler-produced full LTO without patching IR or adding
unwind, frame-pointer or inlining overrides.

The runtime starts the program body on a pthread
([wf_floor.c:308](../../../compiler/src/backend/wf_floor.c#L308),
[wf_floor.c:347](../../../compiler/src/backend/wf_floor.c#L347)). The exact
native `main` function on the original thread is therefore not an expected
ancestor of a hot sample. Report the Whitefoot `main` resume frame and native
`wf__main_body` separately from exact native `main`; LTO may also erase a
wrapper. Reaching a physical scheduler or thread root with correct CFI, without
the logical Whitefoot caller, is consistent with continuation tail transfer.

## Comparison and evidence

The temporary
[`unwind-resume.yml`](../../../.github/workflows/unwind-resume.yml) runs only
on the work branch. It builds the compiler and compiles this witness through
the downstream native build path with full LTO. Record the compiler revision,
command, LLVM version, kernel, perf version and executable symbol inventory.
All three modes sample the software event `cpu-clock:u` at 499 Hz, so the
hosted runner does not need hardware performance counters.
Disassembly must show retained resume, ordinary and leaf functions and actual
non-tail resume-to-ordinary and ordinary-to-leaf call edges. If optimization
erases a target or its call edge, this binary cannot answer the proposed
question; preserve the artifacts and report the run as inconclusive rather
than altering compiler attributes to produce an artificial baseline.

For each retained target, map its native address range to frame-description
entries (FDEs) in `readelf --debug-dump=frames`, `readelf -wF` and
`objdump --dwarf=frames`. Keep the complete dumps and summarize whether an FDE
covers the target, its canonical frame address (CFA) changes, and the return
address and saved frame-pointer rules. Keep the resume disassembly, especially
the prologue and epilogue, to compare stack adjustments and saved registers
with those rules. A present FDE alone does not establish correct CFI.

Profile the same binary and selected workload with:

- `--call-graph dwarf,16384`;
- `--call-graph dwarf,65528`;
- `--call-graph fp`.

For samples whose innermost symbol is `hot_leaf`, count the denominator and
the number and fraction whose chain contains `awaitable_loop.resume`. Also
count the number and fraction reaching Whitefoot's `main` resume symbol,
`wf__main_body`, and exact native `main`. Preserve raw `perf.data` files,
`perf script` output, the table, commands and diagnostics. A mode with no hot
leaf samples has no fraction and supplies no positive unwind evidence. Small
denominators and lost samples constrain any conclusion. Frame-pointer mode is
a diagnostic comparison: without a retained frame-pointer chain, its failure
is expected and says little about DWARF correctness.

## Interpretation and falsifiers

| Candidate | Observation supporting it | Observation limiting or opposing it |
| --- | --- | --- |
| Missing or incorrect resume CFI | The resume lacks an FDE, or its CFA/return-address rules disagree with disassembly; both DWARF sizes stop at it while ordinary/leaf frame rules are usable. A retained FP chain succeeding would strengthen the diagnosis. | Correct rules covering sampled PCs, with both DWARF modes consistently recovering the physical resume caller and scheduler ancestry, oppose an unwind-table explanation for this witness. |
| 16 KiB copy is too small | The larger DWARF copy consistently recovers ancestors absent with 16 KiB, with the same binary, argument vector and retained call edges. | Both sizes recovering the same chains, or both failing at the same frame with a shallow native stack, gives no support for copy size as this witness's cause. Failure to reproduce a large server stack does not exclude it in Firn. |
| Logical callers are stored outside the native stack | Both DWARF sizes unwind ordinary calls through the active resume frame to the scheduler/thread root, while the logical waiting parent is absent; disassembly confirms tail transfers between waiting activations. | Failure before or at the active resume frame cannot be explained solely by storage of older logical callers. Finding the waiting parent physically stacked at the sampled path would oppose this account for that path. |

These explanations can coexist. No positive result for this small program
proves Firn's release has correct CFI, and no absent logical caller establishes
that the active resume frame itself lacks CFI. This experiment proposes no
compiler or language change. The temporary workflow is removed before the
branch is ready, together with its three temporary rows in
`.github/time-budgets.txt`. Their 600/120/180-second limits are unsampled
initial operational ceilings, not budgets derived from twenty CPU-class
runs. The witness and protocol remain the reproducible inputs to the retained
CI evidence.

[`control.c`](control.c) is a C control compiled with `clang -O2 -g0` and sampled in the same job with the same three modes and event: if it shows a similar missing fraction under `cpu-clock:u`, the residual hot-leaf samples without ancestors are a sampling artifact of this host, not a Whitefoot unwind defect; if it reaches about 100%, the residual is Whitefoot-specific.
