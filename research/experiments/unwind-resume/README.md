# Unwinding through a waiting function's resume code

The initial protocol below was written before collecting data. The retained
source run and the current follow-up are distinguished explicitly: observations
from the source run do not establish the result of the additional diagnostics.

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
emitter defines with `probe-stack` and `uwtable`
([emitter.rs:694](../../../compiler/src/backend/emitter.rs#L694)). The compiler
explicitly requests unwind tables; it does not explicitly request a frame
pointer. Inspect the generated resume function's actual FDE and prologue,
rather than inferring final CFI from raw IR attributes. `nounwind` describes
exception behavior and does not establish the absence of unwind tables. The
experiment uses compiler-produced full LTO without patching IR or overriding
frame-pointer or inlining behavior. Firn's original compiler release predates the
`uwtable` attribute, so these binaries do not establish its unwind behavior.

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
[`unwind-resume.yml`](https://github.com/Ming-Research/Whitefoot/blob/5f99db77282601293acdb6d9a47d4d60b2e43592/.github/workflows/unwind-resume.yml) ran only
on the work branch and was removed before merging. It built the compiler and compiled this witness through
the downstream native build path with full LTO. Record the compiler revision,
command, LLVM version, kernel, perf version and executable symbol inventory.
All three modes sample the software event `cpu-clock:u` at 499 Hz, so the
hosted runner does not need hardware performance counters.
Disassembly must show a retained resume and leaf with a non-tail native call
chain between them. The ordinary function can survive as an intermediate
caller or be inlined into resume, leaving a direct resume-to-leaf call. If
optimization erases that physical call chain, preserve the artifacts and
report the run as inconclusive rather than adding inlining overrides.

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
that the active resume frame itself lacks CFI. The initial protocol selected no
language change. The `uwtable` attribute's effect against its base is the same-source
comparison under [retained source-run observations](#retained-source-run-observations).
The temporary workflow was removed before merging, together with its three
temporary rows in `.github/time-budgets.txt`. Their 600/120/180-second limits are unsampled
initial operational ceilings, not budgets derived from twenty CPU-class
runs. The witness and protocol remain the reproducible inputs to the retained
CI evidence.

## Retained source-run observations

CI run `38422018983` produced the observed artifacts for revision
`0ebd5e22545689163345311c3a73604e2864eaae`. Its `revision.txt`, `table.md`,
`perf.data` files, frame dumps, disassembly and executable are the evidence;
the follow-up diagnostics below were not part of that run.

The `uwtable` attribute was isolated by two earlier runs of the same workflow
on revisions that differ only in that attribute and the test pinning it
(`242c1d371` without it, run `38420627517`; `d40b4967a` with it, run
`38421090878`). Without it, `readelf` found no frame-description entry covering
`wf_hot_leaf` (FDE count 0) and DWARF sampling recovered resume in 1,326 of
1,562 (84.9%) and 1,322 of 1,585 (83.4%) hot-leaf samples at 16,384 and 65,528
requested bytes. With it, the leaf had one FDE, resume was recovered in 1,480
of 1,655 (89.4%) and 1,467 of 1,665 (88.1%), and `.eh_frame` grew from 4,088 to
4,296 bytes (208 bytes). Neither run recovered a caller beyond resume; that
remaining loss is the linker-layout result below.

| Mode | Hot-leaf samples | Resume recovered | Physical caller beyond resume |
| --- | ---: | ---: | ---: |
| DWARF, 16,384 requested bytes | 1,672 | 1,485 (88.8%) | 0 |
| DWARF, 65,528 requested bytes | 1,681 | 1,496 (89.0%) | 0 |
| Frame pointer | 1,673 | 0 | 0 |

The default-linker C control recovered `caller_loop` and `main` in all 1,673
and 1,671 hot-leaf samples in the two DWARF modes, respectively. Its
frame-pointer mode recovered neither. This comparison does not isolate
Whitefoot semantics: the linkers produced different executable segment
layouts.

The earlier interpretation that the same sampled PC sometimes succeeded and
sometimes failed is incorrect. In both DWARF recordings, every sample that
recovered resume had a leaf offset at most `+0xfd`; every sample without resume
had an offset from `+0x104` through `+0x127`. No PC occurs in both groups. The
187 and 185 missing-resume samples therefore form a PC-dependent boundary,
rather than evidence of an intermittent failure at the same instruction.

The leaf has an FDE for ELF virtual addresses `0xd980..0xdab7`, with CFA
`rsp+8`. Resume has an FDE for `0xf2c0..0xf801`, with CFA `rsp+80` at the two
hot calls. The saved executable's `.text` virtual address is `0x4f40` and its
file offset is `0x3f40`, a `0x1000` difference. The recorded executable mapping
has the same load-segment difference; its build ID matches the saved ELF.
The default-linker C control's executable load segment has equal virtual
address and file offset (`0x1000`).

Raw-stack reconstruction of all 3,353 DWARF hot-leaf samples finds both
physical return addresses: `[SP]` is binary base plus `0xf73e` or `0xf74f`,
and `[SP+80]` is base plus `0xe8ae`, a return into `wf__main_body`. Applying
that caller's frame rules also finds its return into `wf__floor_entry` at
`[SP+528]`, base plus `0x5397`. The copies contain 16,384 or 65,280 bytes
(65,280 is the observed larger copy size, despite the 65,528-byte request),
so these physical frames are present in both recordings. Sampled RBP values
are unmapped 32-bit values; disassembly uses EBP for the integer mixing state in resume.
These observations oppose insufficient copied stack and missing logical
continuation frames as causes of the first physical unwind failures.

One hypothesis explains both boundaries: an unwind lookup using a
file-relative PC where an ELF virtual address is needed. Subtracting
`0x1000` from leaf PCs moves successful samples into an unrelated FDE at
`0xc940..0xca82`, whose CFA is also `rsp+8`, and moves every unsuccessful
sample into the FDE gap `0xca82..0xcac0`. Applying the same bias to the resume
return PCs selects `wf__main_body` rules using `rbp+16`, although the active
resume frame needs `rsp+80`. The `.eh_frame_hdr` entries for the actual leaf
and resume are ordered and point to their correct FDEs. This is a hypothesis
about address translation during unwinding, not identification of a proven
bug in a particular perf or unwind-library component.

## Follow-up diagnostics and rejection criterion

The follow-up workflow keeps the baseline modes, event, frequency and Whitefoot
invocation protocol. It additionally retains ELF headers, load segments and
`.eh_frame_hdr` bytes for Whitefoot and both C controls; perf build options
and linked libraries; and, for each DWARF recording, explicit PID, TID, time,
IP, symbol, period and user-register output, perf's raw event dump and verbose
unwind stderr. These diagnostics use the recorded data and add no profiling
mode. The raw dump retains stack bytes without a new binary-data parser.

During each recording, it reads `/proc/<pid>/maps`, process status and task
IDs from the launched perf process's descendants, accepting only a child with
the exact executable's device and inode. Snapshot metadata records the PID,
executable identity, wall time, absolute monotonic timestamps before and
after the snapshot, elapsed interval since recording began and read failures.
Collection waits 0.2 seconds after identifying the exact child to allow worker
startup, and overlaps the unchanged workload. No complete snapshot within
one second is an explicit missing-snapshot diagnostic. A snapshot is not a
guarantee that every sampled stack address was mapped at that instant; align
its timestamps with sample times and the recorded MMAP2 events before using
it as evidence. This process identification changes neither the workload
argument vector nor its behavior.

[`control.c`](control.c) remains compiled with `clang -O2 -g0` and the default
linker. A second build of the same source adds only `-fuse-ld=lld`. Both use
the same three call-graph modes and `cpu-clock:u` at 499 Hz, with each binary's
own retained calibration and selected argument vector. Interpret the LLD
control against its own load-segment bias and FDE coverage; do not predict
Whitefoot's exact missing fraction for a different layout. Reproducing a
bias-dependent lookup failure in this C build would oppose a
Whitefoot-language-specific explanation. A successful LLD control alone
would not reject the hypothesis without checking whether its shifted PCs
still fall in usable FDEs.

The rejection criterion is a trace that shows failed PCs being translated to
their correct ELF virtual addresses and using the correct covering FDE/CFA
rules, or a failure pattern inconsistent with that binary's measured load
bias and shifted FDE coverage. A trace showing a missing or wrong lookup at
the predicted shifted addresses supports the hypothesis and can identify
which component is responsible. If the available perf build exposes no such
trace, report the component attribution as unresolved. Diagnostic commands
retain argv, elapsed time and exit status; an unavailable field, unsupported
option or timeout is recorded explicitly and does not replace a successful
baseline comparison. All commands remain under the temporary workflow's
existing operational ceilings.

## Linker-layout result

CI run `38424605911` confirmed the address-bias hypothesis with the same C
source: the default-linker build recovered `caller_loop` and `main` in every
DWARF hot-leaf sample, while the `-fuse-ld=lld` build, whose executable load
segment has a `0x1000` difference between virtual address and file offset,
recovered neither in any sample. The failure therefore follows LLD's default
segment layout under this perf (6.17) and its DWARF unwinder, not Whitefoot
code generation.

CI run `38426537112` measured revision
`4f03db37578935aa60f5761d96760d36594d6d0b`, which keeps `uwtable` and adds
`-Wl,-z,separate-code` to the Linux LTO link. The witness's executable load
segment now starts at virtual address and file offset `0x4000`.

| Build | Mode | Hot-leaf samples | Resume recovered | `wf__main_body` | `wf__floor_entry` |
| --- | --- | ---: | ---: | ---: | ---: |
| Whitefoot witness | DWARF, 16,384 | 1,661 | 1,661 (100%) | 1,661 (100%) | 1,661 (100%) |
| Whitefoot witness | DWARF, 65,528 | 1,667 | 1,667 (100%) | 1,667 (100%) | 1,667 (100%) |
| Whitefoot witness | Frame pointer | 1,656 | 0 | 0 | 0 |

In the same run the C controls recovered `caller_loop` and `main` in 100% of
DWARF samples with the default linker, 0% with plain LLD and 100% with LLD and
`-z separate-code`. The logical waiting caller `main`'s resume is absent, as
expected for continuation tail transfer; the physical chain reaches the thread
entry. The witness executable is 81,040 bytes, against 80,048 bytes for the
same source built with `uwtable` and without `-z separate-code` in run
`38424605911`: 992 bytes, 1.2% larger. `uwtable` itself added 208 bytes of
`.eh_frame` (runs `38420627517` and `38421090878`, above).

These results cover one small program, x86-64 Linux, perf 6.17 on a hosted
runner and LLVM 22's LLD. They do not establish the behavior of other
profilers, or of frame-pointer unwinding, which Whitefoot does not retain.
