# By-value parameters read in place

## Question and scope

A stored aggregate parameter crosses the call boundary as a pointer to the
caller's storage of the value it consumes (`ptr %wf.arg.v<N>`,
`compiler/src/backend/abi.rs`), and every definition copies it into a slot
of its own at entry with one `llvm.memmove` (the entry copy in
`FunctionEmitter::emit`, `compiler/src/backend/emitter.rs`). The
hypothesis this investigation tested: when the host optimizer inlines such a
definition, scalar replacement splits that copy along the aggregate's
representation, into single bytes for a union-laid-out enum
(compiler/payload-enum-layout).

PR #245 attributed a firn cost to that copy. The Results below find
otherwise: the byte stores it saw remain once the copy is gone, and their
cause is unattributed. In firn, the Redis-compatible server,
`set_key` stored a by-value `Bytes`, an enum of an inline text of up to 24
bytes and a boxed one, into the entry under its key's lock. After inlining,
that store grew from 3 SSE moves to about 25 instructions of shifts and
single-byte stores, and SET's throughput fell about 1.2% against main. A
probe that restored only `set_key` to main's code measured the same as main.
The same shape is now firn's `put_text(slot: &Option<Entry>, value: Bytes,
expires: u64)`, which SET, MSET, APPEND, SETRANGE and INCR's text path reach.

This investigation selects when a definition may read a by-value parameter
through the incoming pointer for its whole body instead of copying it.
Callers, the call ABI, parameter attributes and language acceptance are out
of scope, and no specification rule changes: STOR-7 already makes a value's
address unobservable.

## Candidates

- **Copy every by-value parameter**: the baseline.
- **Read in place under a conservative rule** (below).
- **Restructure firn**: the function only decides and its caller stores the
  value. Every command family would repeat this, and every other program
  with the same shape keeps the cost.
- **Accept the cost**.

## What the entry copy protects

Read from main at `ff1894f7b`. A caller passes the existing place of the
value it consumes (`value_place` in `emit_call`,
`compiler/src/backend/emitter/operations.rs`). That place may be one of:

- a slot of its own;
- a binding's destination;
- a field of a larger allocation;
- the backing that compiler/storage-placement lets a call's result reuse.

So the copy guards against these, each kept by a condition of the rule:

- **A result destination aliasing an input.** A definition whose result
  crosses the boundary through the caller's destination pointer may receive
  that pointer in a consumed input's backing. Its entry copies capture every
  input before the one that initializes the result writes it.
- **A frame that outlives the call.** A waiting definition's ramp captures
  its arguments into its frame before its first suspension, and the incoming
  pointer is valid only for that ramp call
  (compiler/waiting-contexts).
- **Storage that reaches split parts through the frame.** A split dispatch
  loop's parts (compiler/match-dispatch-lowering) do not receive the
  enclosing function's incoming pointers.
- **A slot shared or written.** The storage planner may coalesce a dead
  input's slot with an update result or a block parameter, place a value in
  a field of the parameter's allocation, make the slot a binding's
  destination, or expose its address. Writes through any of these would
  reach the caller's storage.

## The rule

A definition reads a by-value parameter through its incoming pointer, with
no entry copy, when all of these hold:

- its public result returns as a value or in registers, not through a
  destination;
- it does not wait;
- it has no overlap group;
- it is not split into dispatch parts;
- the parameter's slot holds no other value, no field placement and no
  binding destination, no child is placed in it, and its address is not
  exposed (`FunctionStoragePlan::holds_only`);
- the slot is not the returned value's slot.

The value is then written by nothing in the definition, so its bytes stay
as the caller handed them over until the call returns. A register-returned
definition's public entry constructs the result in a slot of its own and
returns it, and its caller stores the value only after the call
(compiler/result-registers), so no destination aliases an input.

Some conditions are wider than safety needs. These are not established
counterexamples, only conditions kept to scope the first change:

- the overlap-group exclusion: deferred hand-outs copy their payloads
  separately;
- the exposed-address exclusion: `AddressOf` copies into a binding place of
  its own;
- the returned-slot exclusion.

A self-tail transfer needs no exclusion. Its repeated body receives new
block-parameter values, and a slot coalesced with them fails the
single-value test.

## Prediction and criterion

Written before measuring.

- **Workload:** firn's `redis-bench.yml` compare on the 14900K runner,
  1 server CPU, `set` and `mset` at pipeline depths 16 and 1.
- **Revisions:** firn's main with main's compiler release as the base; the
  same source with the experiment release of this change as the head; and a
  twin of the head as the noise control.
- **Prediction:** SET at depth 16 rises 1 to 2%, recovering at least the
  #245 loss, and MSET by a similar amount.
- **Criterion:** adopt when SET's median gain at depth 16 is at least 1% and
  larger than the head and twin's median difference, and no measured test
  falls more than 1%. Otherwise keep the copy and record the result here.

## Validation

The implementation and its validation stay on the branch
`research/in-place-parameters` (`d7b9b2466`). Its gate passed at
`e35cc5f95` (run 37473286649).

- `a_by_value_parameter_nothing_writes_is_read_in_place`
  (`compiler/src/backend/tests/payload_enums.rs` on that branch):
  - an eligible reader of a union-laid-out enum has no copy;
  - firn's store shape runs with inline and boxed texts, and every owner is
    released once, with and without retained call boundaries.
- `destination_results_keep_snapshots_of_inputs_their_caller_aliases`
  (`compiler/src/backend/tests/owned_places.rs`, also on main): pins the
  destination condition. Its caller passes one pointer as both the result destination
  and the second input, and reading that input in place would return the
  wrong row.
- The existing union-enum, waiting, tail-call, parallel and dispatch suites
  run unchanged.

## Results

Releases:
- **Base:** `wf-ff1894f7b53c`, main `ff1894f7b`, this change's merge base.
- **Head:** `wf-exp-e35cc5f95c4d`, this change at `e35cc5f95`; its gate passed in run 37473286649.

Both sides build firn from the same source, Firn-wf `6239de8c8`.

**Timing, 14900K, 1 server CPU, 3 interleaved passes of 5 s**
([Firn-wf run 37475611172](https://github.com/Ming-Research/Firn-wf/actions/runs/37475611172)).
The head and its twin are byte-identical images.

| test | depth | head vs base | head vs twin |
|---|---|---|---|
| SET | 16 | -3.70% | -0.65% |
| SET | 1 | +0.42% | -0.19% |
| MSET | 16 | -2.05% | -1.88% |
| MSET | 1 | -1.54% | -6.0% |

Identical code differed by up to 6%, and single passes of one image spanned
about 3% either way. So this run cannot resolve the 1% the criterion needs.

**Instructions per SET under callgrind**
([Firn-wf run 37476541818](https://github.com/Ming-Research/Firn-wf/actions/runs/37476541818)).
Each image took 20,000 SETs to warm up, then 100,000 SETs at depth 16 with
3-byte values, 4 clients and 1,000 keys:

| image | instructions | per SET |
|---|---|---|
| base | 161,506,641 | 1,615 |
| head | 161,681,212 | 1,616 |
| head twin | 161,554,826 | 1,615 |

**The disassembly.** The Firn-wf session compared the same run's images
(artifact `q41-images`) and reported the following. This record has not
re-read the artifact.
- `put_text` and `set_body` are inlined into `wf_commands.set_key.resume`.
- That function has the same 1,028 instructions in both images, identical
  once addresses are masked.
- The store of the entry's `Value::Text(Short)` inside the key's lock is
  still the byte-by-byte pattern #245 saw. Its bytes are assembled from
  shifted registers, not copied from a parameter.
- Nine other functions differ, among them `run_expire.resume` (787 to 740
  instructions), `run_persist.resume` (178 to 140), `bytes_boxed` (102 to
  82) and `log_key_word` (130 to 147).

**Verdict against the criterion: not met.** In this firn the change does not
reach SET's path. After inlining, `set_key` has no parameter entry copy left
for it to remove. The byte stores #245 attributed to that copy come from
another construction, recorded in `docs/todo.md`, "firn's SET stores its
inline text byte by byte inside the lock". So the change is not adopted on
the ground it was proposed for.

**Longer timing, 14900K, 1 server CPU, 7 interleaved passes of 10 s**
([Firn-wf run 37476401996](https://github.com/Ming-Research/Firn-wf/actions/runs/37476401996)).
Medians:

| test | depth | head vs base | head vs twin | twin vs base |
|---|---|---|---|---|
| SET | 16 | -1.80% | -4.79% | +3.14% |
| SET | 1 | -1.75% | -1.00% | -0.76% |
| MSET | 16 | -0.35% | +0.07% | -0.42% |
| MSET | 1 | +2.51% | +1.34% | +1.16% |

The head and its twin differ by 4.8% at SET depth 16, so this host's spread
still exceeds 1% at this length. Nothing here contradicts the verdict.

The base image hashed the same in both timing runs. The head image did not
(`9289439815a0...` and then `30fc8d9137d3...`), from the same firn commit and
the same release, which was not republished between the runs. Whether the
firn build's state on the runner or the compiler's output differs between
builds is not yet established.

A direct probe found the compiler deterministic
([run 37479044280](https://github.com/Ming-Research/Whitefoot/actions/runs/37479044280),
hosted `ubuntu-24.04`). Each compiler built firn from Whitefoot main's
`apps/firn`, with the same sources for both:

| compiler | `--emit-llvm`, 4 runs | `--full-lto` image, 2 runs |
|---|---|---|
| base, main `ff1894f7b` | `aa371955c215...` all 4 | `a5fb4b4c1b2b...` both |
| this change | `3ec07b5cd164...` all 4 | `ae77e1d6c8a5...` both |

On the 14900K's 32 processors, though, each compiler builds firn two ways
([Firn-wf run 37480216530](https://github.com/Ming-Research/Firn-wf/actions/runs/37480216530)).
Four builds at one tree path gave two images from main's compiler and two
from this change's. The two head hashes are exactly those of the timing
runs above. Each pair differs only in the build ID and in 21 bytes of one
spawned context's argument copy. So the changed head hash is that host's
build variation, found in both compilers, and not this change. It is
recorded in `docs/todo.md`, "A full-LTO build of firn is not
byte-reproducible on a 32-processor host".

## Disposition

Not adopted (owner, 2026-10-06): the entry copy stays.
compiler/storage-placement records the refusal and its reopening
condition, and the branch above keeps the implementation.

The change's independent parts are on main:
- the destination-alias test;
- the corrected one-word test comment;
- the `place_back` test's locator fix;
- this record and its two todo entries.

## Reopened: functions with branches, measured on Halo

The rule above applied only to functions without a branch: lowering an `if`
or `match` carries every binding into the continuation block, and the
storage plan coalesces that block parameter into the parameter's slot, so
the slot held two values and the copy stayed. Halo's `push_frame(vm, frame:
Frame)` branches and copied its 80-byte `Frame` on every Lua call. The
widened rule qualifies a slot when every value it holds originates from the
parameter through block transfers alone (branch `claude/in-place-branches`,
gate run 37761168412; the witness tests failed under the old rule in run
37761192907).

Measured by Halo-wf on the 14900K with a criterion recorded before
measuring ([run 37771086458](https://github.com/Ming-Research/Halo-wf/actions/runs/37771086458),
Halo-wf `research/experiments/halo-bench/RESULTS.md`, "Reading by-value
parameters in place, measured"): Halo main built by `wf-c18e6708b6cc` and
by `wf-exp-8eaba144a41f`, six interleaved pairs. `push_frame` lost its entry
copy (91 instructions to 85); fib's median fell 3.8% (0.1055 s to 0.1014 s),
beyond both ranges (2.57%, 0.77%) and the twin's 0.9%; the six other kernels
stayed within their ranges. The gain is the widened rule's across every
function that receives an aggregate by value, not `push_frame`'s copy alone.
The owner reopened the rejection and adopted the rule, with firn measured
for no slowdown before merging.

## By-value let bindings read through a reference

### Question and scope

Does a by-value aggregate let retain its whole snapshot under the host's
`-O2` pipeline when a call can write its source, even if no path from that
write reaches a use of the binding? Phase 1 minimizes and inspects this
shape. It changes neither the compiler nor the specification and selects no
implementation. The existing parameter rule above does not select local
let results.

The motivating downstream report is Halo-wf's Lua interpreter,
`lib/halo/vm/library-sort.wf`, `sort_compare`:
`let local_call_5 = vm^.library_contexts.inner[context];` snapshots a
152-byte `LibraryContext`; its fast path reads only
`local_call_5.frame.func` and `local_call_5.comparator`. Halo reports that a
`slow::<Host<E>>(... vm: vm ...)` call which writes `vm` on another path
leaves a 152-byte memcpy and a `0x1f8` frame. Moving that call to another
function reportedly removes the memcpy, leaves a `0x60` frame, and improves
the sort kernel by 25% on the 14900K. These are the supplied downstream
observations, not measurements independently reproduced here; the optimizer
explanation remains a hypothesis.

### What lowering and emission establish

Source inspection at `0bea2b796` establishes the following path:

- `compiler/src/lowering/builder.rs:1095`: an ordinary
  `CheckedStatement::Let` evaluates its initializer once and binds its
  resulting IR value. It does not make this binding an alias of the source.
- `compiler/src/lowering/builder.rs:1957`: `ReadStorage` computes the
  selected place's address, including its field and array-element steps
  (`compiler/src/lowering/builder/storage.rs:499`), then calls
  `load_storage_value` (`storage.rs:705`) to define an `IrOperation::Load`.
  The addressed object here is the selected row, not the whole enclosing
  array or reference target.
- `compiler/src/backend/storage.rs:24` classifies structs and arrays as
  stored aggregates. `FunctionStoragePlan::build_in_world` assigns backing
  to their definitions (`storage.rs:122`); loads and ordinary projections
  remain snapshots, as the module contract at `storage.rs:8` states.
  `select_incoming_places` (`compiler/src/backend/emitter.rs:1984`) only
  selects eligible function parameters, not this load result.
- `compiler/src/backend/emitter/places.rs:132` routes a stored load to
  `load_place_result` (`places.rs:764`). It copies from the selected address
  into the result's planned slot. `copy_storage` (`places.rs:663`) emits
  one `llvm.memmove.p0.p0.i64` of the complete allocated type size, including
  padding. The `llvm.memcpy` alternative there is reserved for the admitted
  equal-or-disjoint operation row; an ordinary witness reader uses memmove.
  A downstream machine-code memcpy can therefore originate in a memmove
  intrinsic.
- Reading `r.f` or `r.g` subsequently projects from that value's slot
  (`ProjectStruct`, `places.rs:188`) and loads the scalar field. An aggregate
  field projection, as in `let-field-of-param.wf`, uses the same
  `load_place_result` copy path. The existing parameter optimization can
  remove the parameter entry copy without removing this new field snapshot.

Thus the initial LLVM contains one whole copy at the let into `r`'s slot;
subsequent CFG transfers can also copy storage if their slots were not
coalesced. It is not an LLVM aggregate SSA value that merely names the
source's fields. For the reference witnesses the let's copy has this
schematic form (the witnesses' row has eight `u64` fields, 64 bytes):

```llvm
call void @llvm.memmove.p0.p0.i64(ptr %r_slot, ptr %selected_row, i64 64, i1 false)
```

That copy captures the old value required by REF-1 and OWN-1. Simply
forwarding a later load from `%r_slot` to `%selected_row`, or sinking the
snapshot past a call that may write the selected row, is invalid if the
later use must see the old value. Ordinary dead-store elimination cannot
discard bytes that a subsequent read of the slot observes. The proposed
explanation for Halo is that the copy is not sunk onto the read-only arm,
while a may-write call blocks source forwarding and the still-read slot
survives scalar replacement of aggregates (SROA).

This last explanation is **not established by the emitter**. A write on a
path with no subsequent use is not a semantic obstacle to forwarding on the
other path, and even a required old value can be captured in scalar registers.
SROA may replace a read slot; “the slot is read later” alone is not evidence
that SROA cannot run. The minimal witnesses deliberately allow LLVM to
inline the small sink, split the copy, hoist scalar reads, or sink a copy.
If those transformations remove it, phase 1 has narrowed or rejected the
explanation, rather than confirmed a compiler change is needed.

### Witnesses and prior expectation

Each file under [witnesses](witnesses/) is a separate complete program, with
its own `main`. `inplace_let_probe` is the function to inspect in every file;
the emitted symbol is `wf_inplace_let_probe`. Package declarations use the
`InplaceLet` or `inplace_let_` prefix to avoid local/declaration collisions.
The reference cases use a struct holding a two-element array, select an
index constrained by `requires i < 2_u64`, and read two fields of a 64-byte
row. Addition uses `+wrap` so arbitrary `u64` inputs introduce no unrelated
overflow proof or runtime branch. The supplied values sum to 42 without
wrapping. The sink changes both observed fields in both elements to 99 and
7, so a post-write direct read would sum to 106.

| Witness | Observation and prior expectation |
|---|---|
| [let-no-write.wf](witnesses/let-no-write.wf) | Binding read through a reference, no later source write. Expect the whole snapshot to disappear. |
| [let-write-other-path.wf](witnesses/let-write-other-path.wf) | `let r = big^.inner[i]; if c { return r.f +wrap r.g; } inplace_let_sink(big: big); return 0_u64;`. The write arm never reads `r`. Expect a retained whole copy if the reported path-insensitive obstacle reproduces. `main` checks both arms and the write. |
| [let-write-after-last-use.wf](witnesses/let-write-after-last-use.wf) | Computes the sum from `r`, calls the sink, returns the saved sum. Expect a retained whole copy under the hypothesis, though scalar loads before the call would be legal. |
| [let-write-before-use.wf](witnesses/let-write-before-use.wf) | Calls the sink between the let and the reads of `r`. The old value is required: `main` expects 42 and separately checks the source is now 99 and 7. This falsifies an in-place rule that reads the modified source. A whole physical copy is not required if scalar snapshots preserve the old value. |
| [let-field-of-param.wf](witnesses/let-field-of-param.wf) | `let r = big.inner;` selects an aggregate field of a by-value parameter. Separates the local projection copy from the parameter entry-copy optimization; expect the redundant local copy to disappear in this no-write control. |

**Rejection criterion, recorded before CI:** if the other-path or
last-use witness has no whole snapshot after optimization, that witness
rejects the respective prediction that the later write alone retains it.
If the no-write control retains it, a may-write call alone does not explain
the contrast. If both write cases simplify, the five-case minimization has
not reproduced Halo: inspect the downstream shape before drawing a wider
conclusion. Inlining away the sink is one possible reason and must be
reported from the artifacts. The before-use case must preserve the old
observed fields, regardless of whether a memory intrinsic survives. A wrong
106 is a correctness failure, never evidence for an optimization.

A zero memcpy/memmove count does not by itself establish removal: a transfer
may become scalar or vector loads and stores. Inspect the optimized reader
and assembly for a complete 64-byte snapshot, scalar captures, load/store
ordering, and the sink call's survival. Frame size corroborates that
inspection; it does not identify the copy's cause. No runtime speed claim
will be made from hosted-runner code inspection.

### CI experiment and current evidence

The temporary `inplace-lets` job in
[compute-bench.yml](../../../.github/workflows/compute-bench.yml) is selected
only by the new manual experiment option. The scoreboard now selects only
`scoreboard`, so this dispatch runs neither scoreboard nor placement jobs.
It uses hosted `ubuntu-24.04`, installs the exact LLVM major in
`.github/llvm-major` by the gate's setup, builds the compiler with the `gate`
profile under `run-check.pl compiler/build`, and processes the no-write
control first, then the other four witnesses. Each is emitted with
`whitefootc --emit-llvm -o <name>.ll`; the same `/usr/bin/clang` emits
`<name>.opt.ll` and `<name>.s` from that untouched input at `-O2` for
`x86_64-unknown-linux-gnu`. There are no forced call boundaries or IR edits.

The summary counts actual memcpy and memmove calls separately in the raw
and optimized **reader function**, excluding declarations and other
functions. Its stack column is Clang's static stack-usage report from
`-fstack-usage` (`<name>.su`), not just an assembly `subq` immediate; the
assembly permits inspection of saves and red-zone use. A missing reader or
stack record is an instrument error, not a zero. Small counter and
missing-reader controls execute in CI before reporting. The artifact
`inplace-lets-x86-64` contains raw/optimized LLVM, assembly, stack reports,
`summary.tsv`, and a manifest of the source revision and toolchain. Copy
counts and frame sizes are observations, not pass/fail thresholds. The job
does not link or execute these programs, so emitted artifacts alone will
not establish their exit status.

Dispatch: `gh workflow run compute-bench.yml --repo Ming-Research/Whitefoot --ref claude/inplace-lets -f experiment=inplace-lets`.

**First result**
([run 37858726701](https://github.com/Ming-Research/Whitefoot/actions/runs/37858726701),
hosted ubuntu-24.04, `/usr/bin/clang -O2`): every one of the five witnesses
has one raw `llvm.memmove` in the reader and none after optimization, with a
static stack of 0 bytes. Even `let-write-before-use` keeps only the two
scalar field loads the old value needs. The prediction that a write to the
source on another path keeps the copy is rejected for these shapes: a may-write
call alone does not keep it.

**Halo's shape, attributed.** A read-only reading of Halo's unsplit
`sort_compare` disassembly against its source (Halo-wf run 37857130998,
artifact `halo-slowsplit-disasm`) finds the whole-element `memcpy` is the
`let local_call_5 = vm^.library_contexts.inner[context];` snapshot, but no
later instruction reads its destination: the number path reloads the
fields it needs from the source element. The copy is dead yet kept. Halo
differs from the first witnesses in three ways: the element holds a
union-laid-out payload enum (`Value`), which makes it memory-only; fields
and operands of that enum type are passed by value, as pointers to stored
slots, to calls that survive inlining (`slow`, `callback_values`); and
those slots may share one `%wf.frame` allocation with the snapshot when the
function's allocation roots do not all qualify for independent allocas
(compiler/storage-placement), so an escaping sibling pointer can keep the
optimizer from proving the snapshot's bytes dead. Two further witnesses
test that mechanism and differ only in a marker struct's alignment:
`let-union-mixed-frame` (a `u32` marker, predicted to share one frame and
keep the copy) and `let-union-uniform-frame` (a `u64` marker, predicted to
get independent allocas and lose it). If the mixed case loses the copy while
its shared frame and aggregate calls survive, the frame-provenance
hypothesis is rejected for this shape. Both contain a falsifier that must
return the old snapshot's value after the source is written.

**Second result**
([run 37861211781](https://github.com/Ming-Research/Whitefoot/actions/runs/37861211781),
same toolchain): the five first witnesses are unchanged; the two Halo-shaped
ones separate.

| witness | raw memmove | optimized memcpy | static stack |
|---|---:|---:|---:|
| `let-union-mixed-frame` (`u32` marker) | 3 | 3 | 296 |
| `let-union-uniform-frame` (`u64` marker) | 3 | 1 | 56 |

In the mixed case the reader keeps one `%wf.frame` struct allocation and the
whole 152-byte `memcpy` into the snapshot's slot stays, beside the 16-byte
argument copies; in the uniform case every slot is its own `alloca`, SROA
splits the snapshot, and only the 12 bytes of the comparator the call needs
are copied. The frame-provenance hypothesis survives its falsifier: the
copy follows the frame representation, selected by compiler/storage-placement's
uniform-alignment condition for independent allocations, not the later
write. Whether Halo's `sort_compare` frame is mixed for this reason is
inferred from its shape, not yet observed.

The job does not link or execute these programs; checker acceptance is
observed through `--emit-llvm`, runtime results remain unverified. Remove
the temporary job and option once its conclusions are recorded here; keep
the witnesses as evidence.

### Selected follow-up: independent mixed-alignment roots

The owner selected separate allocations for ordinary positive-sized roots
with natural requested alignment. The question for the next `inplace-lets`
CI run is whether removing the uniform-alignment restriction makes the mixed
witness lose its dead 152-byte snapshot copy, as the uniform witness did.
Compare both witnesses under the same toolchain and inspect their raw frame
allocations, optimized copies and the before-use snapshot falsifier. Retention
of the dead copy despite separate roots and surviving aggregate calls would
reject this explanation for the mixed witness. The job does not execute the
falsifier, so native snapshot correctness still needs the maintained tests.

The independent extent is the checked sum of `size + emitted_alignment - 1`
over roots, plus `maximum_alignment - 1`. Each preceding gap is at most its
root's alignment minus one; final padding is at most the maximum minus one,
so the bound covers any ordering. For `i8, i64, i8, i64`, the earlier proposed
sum of sizes plus maximum alignment gives 26 bytes, but aligned starts
`0, 8, 16, 24` need 32 bytes. The selected bound is 39 bytes. Both the sum and
final padding addition must reject arithmetic overflow and target-domain
overflow. This qualifies only planned roots, not machine spills or allocations
outside the plan; exact stored-type qualification remains separate.

Split dispatch frames, parallel lane frames and context argument frames keep
their shared-object interfaces; zero-sized and over-aligned ordinary roots
keep the struct fallback.

**Result after the change**
([run 37871073792](https://github.com/Ming-Research/Whitefoot/actions/runs/37871073792),
head `0a0776ce9`, same toolchain): `let-union-mixed-frame` now matches
`let-union-uniform-frame`, one optimized `memcpy` (the 12 bytes of the
comparator the call needs) and a 56-byte static stack, against three copies
including the dead 152-byte snapshot and 296 bytes before. The five first
witnesses are unchanged. The explanation survives its falsifier for this
shape. The job did not execute the programs; native correctness rests on the
gate. The temporary job is removed; its definition is in this branch's
history at `0a0776ce9`.

### Candidate rule, not selected

The results above show the snapshot copy follows the frame representation,
not a later write, so this rule was not needed for the observed cost. A by-value let could read through its source place when, on every path from
the let to each use of the binding, nothing may write that source: no `set`
to it or overlapping storage, no call whose writes reach it, and no write
through a reference reaching it. The binding must never be written or have
its address exposed, and the source place's storage must outlive all uses.
The before-use witness is excluded. This is a candidate condition for later
work only; phase 1 proposes no lowering, analysis or storage-plan change.
