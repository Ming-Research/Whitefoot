# Heap diagnostic after the frozen dataset witness failed

## Recorded result and scope

[CI run 38036398318](https://github.com/Ming-Research/Whitefoot/actions/runs/38036398318),
at `3fa88873489f54c69936f46598608ebaea99be35`, compiled the original witness
successfully and printed `witness run exit status: 9`. Its job log was inspected
on 2026-10-10. The pre-registered expectation **failed** and remains unchanged
in [README.md](README.md). Reaching 9 means every check in `update_reclaim`
passed, including the empty-tree check (61–65); the concurrent phase was
never reached.

This is source inspection and an unexecuted diagnostic, not a measured
attribution. No local compilation, execution, test, commit or push was
performed. The new numerical readings must come from the next branch push's
CI run. This report records predictions separately from that original result.

## Ownership and accounting traced

Line references below name the unchanged compiler/specification at the
recorded revision and the diagnostic worktree's witness.

- `witness.wf:54` freezes a moved value into one Shared object, retains one
  SharedRead and lets the writable handle leave scope. `witness.wf:89`
  allocates one boxed byte array per Pair. Nodes own retained Pair handles
  and child Tree handles; `witness.wf:75` retains these without copying their
  payloads. Inline structs, `Costs`, `Check`, fixed oracle arrays and static
  key constants add no independent heap allocation (STOR-1).
- `witness.wf:293` creates the serial phase's only publication cell.
  Capture retains its tree, not that writable cell. Publication at
  `witness.wf:194` overwrites Current, releasing its old owned tree.
  `witness.wf:368` observes the empty-cell level after all dataset roots
  have been removed; returning still has to release the cell itself.
- `witness.wf:462` creates main's Progress **before** its baseline. It
  remains owned by main across both readings. Serial verification always
  passes `False` for interleaving (`witness.wf:310`); its branch at
  `witness.wf:237` therefore never calls `advance`. The Progress writes
  at `witness.wf:202` and `witness.wf:392` belong to the unreached
  concurrent handshake. They assign inline u64 fields, not owners.
- `compiler/src/lowering/builder/prelude.rs:162` moves the existing value
  into Shared state; it does not copy a Box allocation.
  `compiler/src/backend/emitter/shared.rs:860` retains the same object
  pointer through `wf__shared_share` for both kinds of shared handle.
  `compiler/src/backend/completion/bridge.c:2804` increments the common
  count; line 2809 decrements it. The generated last-handle cleanup at
  `compiler/src/backend/emitter/cleanup.rs:129` recursively drops state,
  then calls `wf__shared_free`.
- `compiler/src/backend/completion/bridge.c:2814` returns that object's
  pool grant. `wf_pool_give` subtracts it at line 1152 **before** retaining
  the freed block on a free list. Box release independently subtracts its
  requested bytes in `compiler/src/backend/heap.c:42`. The meter sums
  live pool grants and emitted-storage counters at
  `compiler/src/backend/completion/bridge.c:1375`.
  [PRE-2](../../../spec/kernel-spec.md), line 2560, counts live pool grants
  but excludes unused reserves, allocator-retained released blocks and
  stacks; RSS retention alone cannot explain this result.

## Ranked hypotheses

1. **A waiting-call frame-arena chunk remains granted to the root context.**
   Strongest source lead, not yet confirmed for the failed binary.
   `compiler/src/backend/completion/bridge.c:2133` obtains a counted
   chunk; `wf_context_release` at line 2156 resets its used length and at
   line 2162 retains an empty extra chunk as `context->spare`, without
   returning that chunk to the pool. Only a displaced spare or arena
   teardown (line 2166) returns it. This can change main's baseline when
   entering `update_reclaim`, yet keep all readings inside that activation
   consistent. The spare can be reused on the next call.
   The pool starts at 512-byte grants (line 1001), and frame growth includes
   4096 bytes of slack (line 1183); the residual need not equal an object
   payload's size. Steps 019–026 compare cold/warm calls and a frame-only
   control. Whether counting an empty retained frame chunk fits PRE-2's
   exclusions needs resolution if this attribution is confirmed; this task
   changes neither that interpretation nor the accounting.

2. **Scope exit misses the publication-cell or a SharedRead release.**
   Possible lowering defect, but the ordinary cleanup helper implements
   both releases and the witness already passed calibration, last-reader
   reclamation and final empty-dataset accounting. The outstanding cell
   at return remains a narrower candidate than a general node leak.
   See `witness.wf:293`, `witness.wf:377`,
   `compiler/src/backend/emitter/cleanup.rs:129` and
   `compiler/src/backend/completion/bridge.c:2809`.
   Steps 002, 007, 015 exercise explicit consuming releases; 020 and 022
   exercise implicit scope exit, including a last SharedRead. Repeated
   growth there with flat frame-only controls would strengthen this lead.

3. **An owned Box/array survives being moved into Shared or overwritten.**
   Less likely because the move stores one owner and the last-handle path
   recursively cleans it up; Pair calibration and tree reclamation passed.
   See `compiler/src/lowering/builder/prelude.rs:162`,
   `compiler/src/backend/emitter/cleanup.rs:147`,
   `compiler/src/backend/heap.c:42` and `witness.wf:89`.
   Steps 003–007 separate the Box charge from its Shared grant;
   009–015 test boxed arrays behind an Option of a SharedRead across
   two overwrites and final cell destruction.

4. **The after-reading executes before cleanup.**
   Low likelihood on this invocation. STOR-8 (`spec/kernel-spec.md:844`)
   grants no allocation/release effect ordering, but the actual default
   lowering chooses `OverlapLowering::Off` at
   `compiler/src/bin/whitefootc.rs:1777`; `run.sh` passes no `--par`.
   Waiting-call transfer destroys the completed callee frame before
   continuing (`compiler/src/backend/emitter/frames.rs:316`), which calls
   frame release at line 211. Heap reads and release calls are ordinary
   external calls with observable runtime state, not emitted `readnone`
   operations; the host reading also transitions the meter
   (`compiler/src/backend/ordinary_values.c:1249`).
   This is source-level lowering evidence, not inspection of the final
   optimized binary. The call-boundary and explicit-release samples localize
   a discrepancy; a persistent unexplained discrepancy still needs emitted
   IR/optimized-code inspection, not an assumption of a source-level fence.

5. **A freed Shared pool block remains counted merely because the allocator
   retains it.** The direct runtime path contradicts this explanation:
   `compiler/src/backend/completion/bridge.c:1152` uncharges every grant
   given back, even though its memory remains mapped. Steps 001–002
   provide the smallest observation. This differs from hypothesis 1:
   the frame spare has not been given back to the pool at all.

## What diag.wf prints and what each line distinguishes

[diag.wf](diag.wf) buffers all samples in an inline array and prints only
after every measurement, through [heap_report.wf](heap_report.wf).
Thus its own output calls, cancellation-watch handles and reporting frames
cannot change any sampled interval. Every successful row has this format,
with decimal bytes padded to 20 digits and a signed mathematical difference:

```text
step=NNN before=BBBBBBBBBBBBBBBBBBBB after=AAAAAAAAAAAAAAAAAAAA difference=+DDDDDDDDDDDDDDDDDDDD
```

A decrease prints `-`; zero prints `+`. These are format placeholders,
**not observed output**. All 26 numerical rows remain unexecuted. The following
are discriminating predictions from ownership/accounting, not replacement
expectations for the witness. Let S be the Shared grant measured by 001,
B the Box charge in 003, P the publication-cell charge in 008, and N the
node-plus-boxed-array charge in 009.

| Step | Interval and distinguishing observation |
| --- | --- |
| 001 | Create Shared<u64>: positive S; source predicts a 512-byte grant for this small object. |
| 002 | Drop its only handle: -S; zero or a residual points to missing release/accounting, not payload recursion. |
| 003 | Allocate Box<u64> alone: B (source predicts 8 requested bytes). |
| 004 | Move that Box into Shared: a Shared grant alone; an additional B suggests duplicated payload storage. |
| 005 | Retain SharedRead<Box<u64>>: zero; a handle is counted, not separately allocated. |
| 006 | Drop writer while reader lives: zero; negative storage change would be premature release. |
| 007 | Drop last reader: negate 003 + 004; failure to recover B isolates payload cleanup, failure to recover the grant implicates the object/count. |
| 008 | Create empty Option publication cell: positive P, independently of nodes. |
| 009 | Create first immutable node: N; its freeze helper already leaves the writable handle's scope. |
| 010 | Move that node into None: zero; publication moves the retained handle. |
| 011 | Create replacement node: same N as 009. |
| 012 | Replace the first Some: -N; lack of decrease isolates overwrite cleanup. |
| 013 | Create second replacement: same N again. |
| 014 | Replace the second Some: -N again; detects accumulating old versions. |
| 015 | Drop cell holding final Some: -(P + N); separates cell destruction and recursive final-node cleanup. |
| 016 | Create Progress: one Shared grant, no owned payload allocation. |
| 017 | Write requested and published in separate atomics: zero; growth would implicate atomic/runtime machinery, not replacement of heap owners. |
| 018 | Drop Progress: negate 016. |
| 019 | First scope_phase: caller baseline to callee's pre-cleanup live reading; includes its remaining scalar, reader, cell/node, Progress and any additional frame grants. |
| 020 | Same call: caller baseline to after return; all callee source owners have left scope. Nonzero storage is implicit-cleanup or runtime-frame evidence. |
| 021 | Repeat scope_phase: pre-cleanup live reading relative to the new caller baseline; compare its increment with 019. |
| 022 | Repeat return: repeated growth favors unreleased owners; a first-call rise followed by zero favors retained/reused frame storage. |
| 023 | First frame_only: baseline to largest reading in depth-256 non-tail waiting recursion, with no Box or Shared operations. Growth isolates coroutine storage. |
| 024 | Same recursion after return: positive retained bytes demonstrate a residual possible without shared objects or Box payloads. |
| 025 | Repeat identical frame-only recursion: compare peak increment with 023 to see reuse of existing arena storage. |
| 026 | Repeated frame-only return: zero after a first rise supports bounded spare retention; repeated rises need further investigation. |

The callee's live and returned rows share the same caller baseline: subtract
their `after` values to observe the cleanup decrement. Compare 002's after
with 001's before, 007's after with 003's before, and 015's after with 008's
before for full-phase restoration. A zero result in this smaller diagnostic
does not prove the original witness's deeper call tree allocates no frame
chunk. A frame-only retained amount similar to exit 9 supports attribution
but cannot establish identical chunk histories from totals alone.

On the witness's existing exit-9 branch, the same format is printed to
**stderr** with `step=009` using its already-saved before/after readings;
even an output error leaves the source return code 9. Output is performed
after the comparison, although adding a waiting reporting path may change
compiled frame layout and therefore a new run's absolute readings or whether
the original mismatch reproduces. The original failed run remains evidence.

## Invocation and remaining validation

[run.sh](run.sh) compiles each entry with the shared reporting source, runs
the witness first, then compiles/runs the diagnostic even after a witness
failure. The witness's `witness run exit status: N` line is unchanged.
The script preserves the witness compile/run failure as its final status;
only a successful witness permits a diagnostic failure to select that status.
Diagnostic compile/run statuses also go to `results.tsv`; compiler output
and program output go to `diag-compile.log` and `diag-run.log`.
The existing temporary workflow already uploads the whole output directory.

No specification, design decision, conformance verdict, README expectation
or runtime implementation was changed. These diagnostic sources and their
shared printer belong to this temporary research experiment and can be
removed when attribution is recorded; the original witness remains intact
apart from its requested exit-9 reporting call and stderr binding.

Pending CI: source acceptance, all numerical output, exact attribution, and
the original witness's still-unreached concurrent phase. The runner's
failure-precedence paths have been inspected, not executed. No new result
is claimed from that inspection.

## Read-only completion review

A separate Codex reviewer (inherited configuration, exact model ID unavailable)
reviewed `3fa88873489f54c69936f46598608ebaea99be35..worktree`: both tracked
diffs and all three new files, including the depth-256 controls. It read
the governing nodes, specification, I/O precedents and affected runtime
paths, and independently confirmed the original CI revision and statuses.
No findings remained within scope. Repository/citation checks A4/D2,
construction and research-boundary checks T5/T6, and design checks G2/G3 and
DC1/DC2 passed by inspection. DC4 and executable validation remain unverified;
unchanged specification, conformance, compiler-safety and gate-budget groups
were not applicable. The reviewer ran no compiler, program, test or lint.

No unrelated defect was established. The retained-frame explanation and
possible cleanup/accounting defects above remain hypotheses awaiting the
diagnostic CI output, not fixes or approved specification interpretations.
