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

The temporary `inplace-lets` job, now removed, was a manual experiment option
of `compute-bench.yml`; its definition is in this branch's history at
[`0a0776ce9`](https://github.com/Ming-Research/Whitefoot/blob/0a0776ce9/.github/workflows/compute-bench.yml).
It used hosted `ubuntu-24.04`, installed the exact LLVM major in
`.github/llvm-major` by the gate's setup, built the compiler with the `gate`
profile under `run-check.pl compiler/build`, and processed the no-write
control first, then the other witnesses. Each was emitted with
`whitefootc --emit-llvm -o <name>.ll`; the same `/usr/bin/clang` emitted
`<name>.opt.ll` and `<name>.s` from that untouched input at `-O2` for
`x86_64-unknown-linux-gnu`, with no forced call boundaries or IR edits.

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

It was dispatched with `-f experiment=inplace-lets` on this branch while the job existed.

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

### Halo with separate roots

Observed on the 14900K by Halo-wf
([run 37878776396](https://github.com/Ming-Research/Halo-wf/actions/runs/37878776396),
artifact `halo-lm-inplace-let`): Halo's sources with `sort_compare`
unsplit, built by experiment release `wf-exp-2ab47723e356` (main
`a4a333fb8`'s tree) and by `wf-exp-b4e79dfdff01` (that base plus this
change), 6 alternating pairs with a twin of the base.

| kernel | head/base median |
|---|---:|
| sort with a comparator (300,000 elements) | 0.941 |
| string-key | 0.748 |
| integer-table | 0.866 |
| concat | 0.866 |
| fib | 0.969 |
| sort (homogeneous fast path) | 0.951 |
| binary-trees | 0.984 |
| loop | 0.996 |

The first five lie outside their spreads and the twin's largest deviation
(integer-table 1.015, concat 0.984); none is slower. `sort_compare` drops
from 297 instructions, one `memcpy` and a 0x1d8-byte frame to 255
instructions, no `memcpy` and 0x60 bytes. `run`'s call arm is identical in
both builds, its `Step` copy retained, as split frames are outside this
change; `prepare` goes from two `memcpy` calls and 0x538 bytes to one and
0x2d8, and whether the remaining one is the `Value` read is not yet
attributed. Halo's check time is unchanged by this change (10.32 and 10.22 s
against 10.25 and 10.40 s).

### Earlier candidate rule

The results above show the snapshot copy follows the frame representation,
not a later write, so this rule was not needed for the observed cost. A by-value let could read through its source place when, on every path from
the let to each use of the binding, nothing may write that source: no `set`
to it or overlapping storage, no call whose writes reach it, and no write
through a reference reaching it. The binding must never be written or have
its address exposed, and the source place's storage must outlive all uses.
The before-use witness is excluded. Phase 1 did not select this candidate
or change snapshot placement. The later [read-through snapshot decision](#read-through-snapshots)
selects it for the indexed Halo witness, whose remaining copy is now attributed.


### Unobserved join forwarding

Source inspection of main `d392af91c` identifies a separate cause for the
provisional-result selection copy: `lower_match_from_value` carries every
binding into a continuing join. In `let step = prepare(...); let final_step =
step; match step { ... } return final_step;`, the unused post-match `step`
parameter remains a distinct destination. Storage interference protects both
join destinations, preventing coalescing; a union-enum edge transfer then
emits a whole-value `memmove`.

The change generalizes the outlined-chunk capture dependency walk to all
finished lowered functions, before work estimation and storage planning.
Instruction operands, terminator observations and every value or place used by
a release are roots. A needed parameter retains every incoming argument;
a forwarding cycle with no route to a root disappears. Ordinary instructions,
cleanup order, source signatures and value IDs remain. Reconstruction and
capture-ABI removal remain confined to the existing chunk caller.

Without the unused duplicate, existing CFG transfer candidates can coalesce
`prepare`, `unwind` and `final_step`. The returned-slot selector maps their
common complete allocation to the result destination, which `emit_call` already
passes. No interference, aliasing, exposed-address, swap snapshot, waiting or
overlap restriction is relaxed; split parts retain their enclosing-frame
lifetime rules. This is a code-inspection conclusion, pending CI evidence.

The maintained 32-byte `Step` fixture asserts direct producer destinations and
no whole-value copies in raw and optimized ordinary/split-arm definitions.
The base retains the duplicate join destination and fails the raw assertions.
Native cases observe the provisional value after recovery, simultaneous
aggregate-carry swaps and exactly-once release of a cleanup-only owner. The
shared walk's unit case distinguishes dead forwarding cycles from cycles
reaching instruction, branch, return or cleanup uses. The earlier record-entry
test now expects only genuinely read continuation carries; its copy and native
assertions remain. Execution, including baseline failure, awaits CI.

Halo timing (with read-through snapshots, PR #310, Halo-wf run 37986461719, 14900K, six interleaved pairs against the #292 compiler): fib 0.779, binary-trees 0.913, loop 1.024 (register allocation in ForLoop, board item lm-bl-forloop-spill), other kernels within the twin's spread. No specification, verdict or diagnostic change.

### Read-through snapshots

The owner selected option A on the status board on 2026-10-09: preserve
by-value snapshot semantics, but read through a stable source address when
the compiler proves that no intervening operation can change or invalidate
it. This supersedes the earlier unselected candidate for this work; the
separate-allocation results above remain evidence for their own change.

The supplied Halo optimized artifact `bench.opt.path.ll`, function
`prepare$instance$2ead40aebe81bc32`, block `bb5`, contains:

```llvm
%t4 = getelementptr inbounds nuw %wf.t.90128e7f982cc38a, ptr %t4.split, i64 %v24639
call void @llvm.memmove.p0.p0.i64(ptr noundef nonnull align 8 dereferenceable(16) %wf.slot.0, ptr noundef nonnull align 1 dereferenceable(16) %t4, i64 16, i1 false)
%t1.i.i = load i32, ptr %wf.slot.0, align 8
```

The source is `let v = stack^.inner[fslot]; let view = func_of(v: v);`
in Halo `lib/halo/vm/calls.wf:296–297`; `state.wf:81` defines `func_of`
with a by-value `Value` parameter. The retained IR settles the earlier
attribution question: this particular 16-byte snapshot survives host
optimization. It supplies no timing result for its removal.

Source inspection on Whitefoot `d903bf3f63e970653504984bd333ee2a34ac7bb7`
confirms `CheckedExpression::ReadStorage` calls `load_storage_value`, which
emits `IrOperation::Load`. Stored-aggregate planning reserves a snapshot
slot, and `load_place_result` copies the complete value to it. The existing
immutable-parameter path removes an eligible callee's entry copy but does
not remove this caller copy.

The implementation question is whether placement can eliminate that copy
while preserving the old value on all mutation paths. Compare the maintained
backend fixtures before and after the placement change, keeping observation
callees out of line. A whole-value copy on an eligible hot path rejects the
optimization claim. Observing a changed value, losing a required capture,
or emitting an address across part boundaries rejects correctness. Baseline
failure, generated LLVM and native execution remain pending CI. Only the
owner-authorized released compiler's `--check` fixture admission runs were
made on the editing machine; they do not validate the changed backend.

#### Placement and materialization

The planner records the Load's captured address for complete, unexposed slots
whose values come only from that Load through unchanged CFG transfers.
Scalar field/tag observations and eligible synchronous by-value calls read
through. Each other use gets a copy immediately before that instruction or
terminator, on that path only. The emitter uses private backing for that
operation and the captured source address for other eligible uses. Copies
never rely on which block was emitted first, nor on a previous operation
having initialized backing. A returned snapshot can materialize directly
into its result destination. Exposed and mixed-origin slots still retain the
Load copy: an escaping mutable address needs persistent snapshot storage.

Every read-through use requires source stability for the complete operation;
every materialization requires stability from the Load to just before that
operation. Thus a cold call that writes the source can receive its old value
from a copy taken immediately before the call. This also covers an immutable
value formal when another argument authorizes the source write during that
call: immutability of the value formal alone cannot make the use read through.
If any intervening write or
invalidation can reach any required use, the entire snapshot falls back to
its original Load copy. The owner requested this lazy placement within
option A on 2026-10-09; the snapshot's logical semantics are unchanged.

Typed projections, loads and unchanged reference carries retain a containing
root. Checked reference formals and known fresh local allocations provide
root identities; unknown producers or rebindings cannot establish a root.
The finite CFG walk explores both clean and invalidated states. Same-root or
unknown-root stores, container changes, replacements, releases and call
writes invalidate the source. Calls use retained read-only reference-formal
facts; unknown contracts, waiting calls and unclassified operations remain
barriers. Effects on a proved separate root do not block placement.

Halo's retained `prepare` snapshot was rejected because `metamethod`'s
`reads(name)` range had no read-only formal fact, and its constant text slice
(`ConstantAddress` → `SliceFromRun` → `SliceRange`) had an unknown root: the
call falsely dirtied the stack source before a later observation of `v`.
Lowering now retains the no-declared-write fact for ordinary references,
ranges and runs, and the call barrier consumes it for both reference source
modes. This follows CALL-1 and REF-4 without assuming range disjointness;
writing ranges still invalidate overlapping sources. The maintained backend
case pairs parameter and constant text ranges with an overlapping range
write. Its source passed the authorized prebuilt compiler's `--check`;
copy placement and native execution remain pending.

An own argument whose type consists entirely of inline scalars, structural
struct/enum fields or fixed arrays/windows cannot reach another allocation.
It therefore contributes no source invalidation, even if an unknown producer
returned it. This matters for Halo's `func_of(v: tm)` between `metamethod`
and a later observation of `v`. The type walk excludes Box, Shared, opaque,
reference and other indirect representations; absence of release work is
not used as proof of an inline representation. A transfer of a potentially
aliasing owner into a constructor, binding, box, store or window remains a
barrier: moving it can hide the source under a different root before release.

A write after the final use is irrelevant. Re-executing the Load starts a
new interval only when no old snapshot carry is live into it. Uses employ
the captured address, never a recomputed index. All uses must stay in the
enclosing part or one possible split arm, and potential dispatch-header
loads stay excluded because header instructions can be hoisted. Waiting and
overlapping execution remain excluded. Overlap restrictions follow the world
being emitted, including callee eligibility and call invalidation: a sequential
clone executes its body and reachable calls without deferred hand-outs, so
retained overlap annotations do not exclude it. Source stability, waiting,
exposure and split-part restrictions still apply. These rules use no source
names and add no proof, diagnostic, specification or verdict change.

#### Destination-result calls

The caller-side restriction was unnecessary and is removed. A read-through
call still requires a checked own parameter, an acyclic synchronous body,
no overlapping execution, and an unexposed complete slot holding only that
parameter and not serving as its result. Acyclicity excludes split callees.
A destination result does not invalidate that proof: the callee must preserve
its original inputs across possible aliased result writes. At the time of the
Load-snapshot change, it did so with entry captures. Passing a stable element
address removes the caller's redundant capture without relaxing that duty.

At the time of the Load-snapshot change, the separate callee-entry
restriction in `select_incoming_places` retained all captures: a destination
is not always distinct from inputs. The proposed extension below preserves
this ABI obligation with proved capture placement. `call_reuse_operand_for_type` may reuse a consumed input as the
result, and `returned_storage_slot` permits a different parameter's slot to
be redirected into that destination. For example, this schematic fragment
uses a `Big` record large enough to require a destination result:

```text
fn choose(a: Big, b: Big) -> result: Big {
  let out = a;
  let old = b.last;
  set out.last = old;
  return out;
}
let result = choose(a: left, b: move right);
```

If the result occupies `right`, the callee's entry transfer from `a` into
`out` overwrites `b`'s incoming storage. Letting `b` read through that pointer
would observe `a.last`, not the original `right.last`. The existing prologue
captures `b` before initializing the result and prevents this error. That
protection remains required; it is not grounds for copying the caller's
separate snapshot too. The proposed extension retains these entry captures
when the prologue initializes the result; other bodies may instead prove
that each input use or private capture precedes a possible aliased write.

#### Halo prepare trace

This is source inspection of the supplied Halo tree, not newly generated IR.
The snapshot at `calls.wf:296` has four observations:

| Use of `v` | Placement under the revised rule |
|---|---|
| `func_of(v: v)`, line 297 | Read through the captured stack element. `state.wf:81` matches the tag and reads the selected scalar function handle or native id; its unchanged by-value parameter is eligible. |
| Native id 111 error, `call_type_error(..., v: v, ...)`, line 322 | Read through. The preceding config store and this call's writes affect `vm`, a separate checked root from `stack`. `state.wf:1344` has an acyclic body and never changes or exposes `v`. |
| Other branch, `metamethod(vm: vm, v: v, ...)`, line 442 | Read through at the caller. The `Value` destination is separate from this captured element, and `slow.wf:134` keeps its input capture required by the general destination-result ABI. Its writes reach `vm`; the name range is read-only. |
| Failed metamethod lookup, `call_type_error(..., v: v, ...)`, line 445 | Read through. `func_of(v: tm)` observes a different inline `Value` and cannot change the stack element; intervening writes still reach only `vm`. |

Lua and ordinary native paths have no further use of `v` before their stack
mutations or returns. In the Other branch, `ensure_stack` at line 458 and
the following shifts happen after the last use; the next loop iteration
loads a fresh `v`. No caller-side copy of this `v` is expected, even on the
error paths. `metamethod` may still copy at its own entry. Lazy per-use
materialization covers consumers outside this eligibility subset without
putting their copies back on the hot path. Exact slot eligibility, dispatch
partitioning and the final copy traffic require rebuilt Halo IR to confirm.

#### Maintained cases and fixture admission

`compiler/src/backend/tests/read_through.rs` covers:

- A 16-byte prepare-like snapshot with a surviving non-inlined classifier and
  direct tag load: raw and optimized eligible bodies must have no whole copy.
- Source writes and writing calls before a later read: the Load copy remains
  and the old value is 7 rather than the replacement 99. Growth and push have
  explicit copy assertions even if allocation or the earlier element stays put.
- Unrelated-root writes and writes after the last use: no snapshot copy.
- Moving a Box owner into a wrapper released before the use: retain the copy,
  so the observation cannot read freed storage. The fixture reads through an
  explicit reference to the Box content to produce a Load; a direct
  `holder.cell.inner` expression lowers as BoxDeref and does not exercise
  Load snapshot placement.
- Mutable by-value consumers: privately materialize at the use. An exposed
  snapshot binding still copies at the Load, and subsequent source reads
  check that local mutation did not change the source.
- Actual split arms: local use reads through, cross-part use retains backing.
- A hot tag-only path and a cold ineligible call: exactly one raw copy in
  the cold successor, immediately before the call. The call replaces the source
  with 99 before reading its argument, then mutates its local parameter to 37;
  it must return 737 from old value 7 and local value 37. In the paired case,
  the source changes before the cold call, so the copy must precede the branch
  at the Load, and the call still returns 737. Both hot paths return 1.
- A destination-result consumer followed by a call using its returned inline
  value and then another observation of the original snapshot: the caller
  passes the element directly and observes 15; the destination-result callee
  retains its input capture. A returned snapshot materializes at its return
  directly into the result destination. An immutable-formal call that changes
  the source through another argument must capture immediately before the
  call, after an earlier read-through observation. Its callee reads the
  parameter in place; both observations must see 7 and sum to 14, not 106.
  The earlier observation distinguishes lazy placement from a Load copy.

The fixture repairs preserve the four committed tests' selection, dispatch,
provisional-value, swap and exactly-once cleanup observations. They expand
all blocks (including empty arms), use distinct match binder names, and bind
`Command` constructors before calls. The new read-through fixtures also
remove the leading blank line, spell the unit value `unit`, and make `touch`
actually read its declared reference root. No expected outcome was weakened.

On 2026-10-09, the authorized released `whitefootc --check FILE` accepts eight
standalone files assembled from the exact fixture bytes: `select_step.wf`
with an inert main; the two payload-enum tests; the split selection test;
the cleanup-only owner test; and the three read-through tests. No module
graph is needed. The released compiler predates this branch, so these runs
establish syntax, canonical form and acceptance only. Rust authoring used
`rustfmt`; no Cargo, Make, build, native test or performance run was made.

The effect whitelist, known-root subset, callee restrictions and exposure or
mixed-origin exclusions can retain safe copies. Analysis cost, baseline
failure, optimized shape, facts-off behavior, native correctness and the
full compiler gate remain unverified. **Halo timing and rebuilt IR pending.**


## Destination-result parameters

Can lazy
incoming capture remove Halo's hot key copy while preserving original-input
semantics? Compare identical Halo source/toolchain inputs before and after;
a surviving eligible hot-path entry copy rejects the placement claim, and
any changed input observation under result/input aliasing rejects correctness.

The supplied `sol-sf.txt` sections A ("The first SetTableRR wide read is a
callee entry snapshot") and C.4 ("selective parameter capture") identify
`table_get(table: Value, key: Value)`'s two callee entry copies at
`bench.ll:26031–26032`, after PR #310 removed its caller snapshots. The key
copy survives as 16 bytes at `bench.opt.ll:17728`; full LTO places its
`movupd` in `arm.13.txt:26–34`, spanning the producer's two 8-byte stores.
The supplied body reads the numeric key at optimized lines 17756–17775 and
passes it to cold `node_find` at 17781, before the result writes on their
paths at 17752, 17800 and 17814. These are supplied artifact observations,
not newly generated IR or timing evidence.

The prototype reuses `storage/snapshots.rs`'s consumer classification,
materialization-site analysis and conservative effect barriers. An incoming
pointer has no disjoint-root proof. Writes to the result, its placed fields,
call destinations, edge transfers and Load materializations invalidate every
indirect input. Before a first barrier with a reachable use, or an ineligible
consumer, the path captures private backing; later uses and unchanged carriers
read that preserved copy. Load snapshots keep their existing per-use behavior;
incoming captures need a separate finite placement walk because recapturing
after a result write would read the wrong value.

Mixed clean/captured joins and reentered capture sites retain entry copies.
A prologue that initializes the result from another parameter retains its
existing two-pass capture order. Waiting definitions, overlap groups, split
parts, incomplete or exposed storage, updates and mixed origins remain
excluded. Calls, pointer writes, releases and unknown effects keep conservative
barriers. ABI, result-slot reuse, layout, acceptance, verdicts and diagnostics
remain unchanged; no runtime flag or pointer phi is introduced.

The backend regression shares one native image: `read_first` rejects main's
entry copy; `write_then_read` rejects omitted/late capture by observing 7
after writing 99; `hot_cold` requires a copy only before the cold result call.
Caller assertions require equal result/input pointers, and calls stay out of
line. Small CFG cases cover mixed joins, all-captured joins and reentry. The
existing Load case's callee-copy expectation follows the new rule; the
prologue-alias regression remains unchanged.

On 2026-10-10 the authorized prebuilt `whitefootc --check` accepted the exact
new fixture and the existing lazy-Load fixture. This establishes admission
only. Changed Rust files received `rustfmt --edition 2024`. No build, Rust
test, native execution, gate or performance run was made locally; Rust checks,
baseline failures, emitted placement, native correctness, analysis cost and
rebuilt Halo IR remain pending CI.


## Layout-bounded transfers

Does emitting
unavoidable aggregate transfers at layout boundaries preserve producer store
widths through optimization and full LTO, and improve Halo beyond the same-source
twin's spread? Compare base, twin and candidate with identical Halo source,
target, LLVM version and settings, using interleaved runs through CI on the
14900K. Inspect optimized IR and final source loads before attributing any gain.
Widened loads crossing the selected boundaries reject the frontend-only remedy;
forwarding improvements without a runtime gain beyond noise reject its cost.

The supplied `sol-sf.txt` sections A and C.1 identify Frame's retained
`place_back` transfer at `bench.ll:178655–178656`: an 80-byte memmove becomes
an 80-byte memcpy at `bench.opt.ll:87373`, then five `movups` loads/stores in
`disasm/push_frame.txt:79–88`. The first 16-byte read at source offset 64 spans
the separately stored 8-byte `varcount` and `frame_top` fields. Frame's other
fields include 4-byte handles and a 1-byte flag with intervening padding.

The same supplied artifacts show Halo Value's overlapping views: a 4-byte tag,
a 4-byte handle at offset 4, or an 8-byte numeric payload at offset 8. ForLoop's
optimized construction stores two 8-byte pieces, while SetTableRR's surviving
key entry capture reads 16 bytes (`bench.opt.ll:17728`,
`disasm/arm.13.txt:26–34`); full/slow paths retain further wide reads at lines
255 and 364–369. These are observations of supplied artifacts, not new timing
or rebuilt candidate output. Destination-result parameter placement can remove
some captures independently; this experiment addresses copies that remain.

### Proposed emission rule

Target layout owns transfer planning alongside variant and aggregate layout.
Structs recursively transfer scalar leaves with their original LLVM types and
widths at target offsets; padding is omitted. Product enums recurse through
the tag and their physical fields. A union enum partitions its complete size
at every variant leaf's start **and end**, including nested aggregate boundaries
and each handler word. Plain-byte intervals use an integer of exactly that
width (not a first-class union carrier); a full pointer word present in every
view uses `ptr`. Any interval touching an overlaid/split pointer, a one-bit leaf,
or a nested byte-only interval instead uses byte memmove through planned scratch
storage. Common handler words remain pointer-typed, including four-aligned words.
All scalar source loads and all byte-interval captures precede every destination
store or restore, preserving partial-overlap and equal-pointer semantics.
Textually identical source/destination places still need no transfer.

Both bounds must hold: **128 allocated bytes and 16 granules**. They include
Halo's 80-byte/11-field Frame and 16-byte/3-interval Value while capping scalar
code growth and live captures. They are provisional experiment limits, not
measured optima or language limits. Arrays/windows, including nested fields,
and other unsupported shapes keep the whole memory intrinsic even below the
bounds: window tails must not become initialized scalar reads, and array
scalarization already has an established compilation-cost objection. An exceeded
bound also keeps the intrinsic. The existing OP-11 equal-or-disjoint memcpy
selection remains in that fallback; all other fallback copies use memmove.
Conservative alignment 1 is emitted for transfers, including pointer accesses,
because these copy entry points do not retain a stronger place-alignment fact.

### Evidence to obtain

The backend gate's `layout_transfers` module checks raw IR for the mixed-width
56-byte Frame fixture: eight loads and stores at offsets 0/8/16/24/32/36/40/48,
with widths 8/8/4/8/4/1/8/8, all loads before stores and no whole-size intrinsic.
A Halo-like Value requires 4+4+8, rejecting a tag-only or wide 16-byte transfer.
A scalar-only 136-byte record, a 17-byte/17-leaf record, and even small
array-containing records must retain memmove; a 128-byte/16-leaf record still
expands. Under current at-most-eight-byte leaves, the large scalar record
exceeds both bounds, so it does not independently test the byte cap. Pointer/Bool
union overlays require captured byte intervals; the target-plan case covers all
five supported triples. A native C observer feeds the actual emitted Frame
transfer equal pointers and partial overlap in both directions, comparing each
field against a pre-transfer byte snapshot. Its C entry calls `wf__floor_run`
and supplies `wf__main_body`, as required by the ordinary native test link.
Interleaving source loads and destination stores corrupts the forward-overlap
case. Existing owning enum, handler, swap and snapshot cases
remain wired; their native outcomes and cleanup checks are unchanged.

Existing IR expectations intentionally updated: payload-enum record parameter
entry/store copy counts and read-through Load/incoming capture presence, count,
path and immediately-before-consumer checks now recognize bounded transfers as
complete copies; the handler-cell copy checks common pointer-word transfer
instead of whole memmove. Copy-elimination assertions also recognize bounded
transfers, so absence of an intrinsic alone no longer passes them. Array-backed
record, large swap and container transfer expectations remain whole intrinsics.

The changed maintained expectations are listed individually below. These
layout-transfer expectation changes preserve the WF fixtures, copy placement
requirements and native expected results.

| Backend case or shared assertion | Intentional IR expectation change |
| --- | --- |
| `payload_enums::a_by_value_parameter_nothing_writes_is_read_in_place` | No-copy checks also exclude bounded transfers. |
| `payload_enums::unchanged_by_value_parameters_are_read_in_place_across_branches` | `rec_set` still has one required store copy, now ten scalar leaves; other bodies still have zero copies. |
| `payload_enums::a_different_value_on_one_branch_keeps_the_parameter_entry_copy` | The required `Rec` entry capture is now ten scalar leaves from the same incoming pointer. |
| `payload_enums::assert_step_destination` (ordinary and split selection callers) | No-copy checks also exclude bounded transfers. |
| `read_through::read_through_snapshot_placement_preserves_old_values_and_call_boundaries` | Required captures and copy-free stable paths recognize either transfer form. |
| `read_through::read_through_distinguishes_readonly_ranges_from_overlapping_range_writes` | Writing-range captures remain required; read-only paths still forbid copies. |
| `read_through::read_through_snapshot_stays_in_one_dispatch_part` | Cross-part captures still count as copies; local paths still forbid them. |
| `read_through::read_through_distinguishes_previous_and_fresh_loop_snapshots` | Old-snapshot capture presence and fresh-snapshot absence apply to either transfer form. |
| `read_through::snapshot_materialization_is_local_to_the_use_unless_the_source_changed` | Capture counts, cold-block placement and immediately-before-call/return checks use the bounded transfer's end; result materialization still targets the result pointer. |
| `read_through::destination_parameters_capture_before_invalidation_only_on_paths_that_need_it` | Incoming capture checks recognize bounded transfers while preserving write/read/path order. |
| `match_dispatch::handler_words_preserve_copies_replacements_tags_and_four_byte_alignment` | The 20-byte Cell copies bounded intervals with a pointer-typed handler word at offset 12, rather than whole memmove. Exact load/store offsets, types, ordering and SSA correspondence cover every interval; layout, stored-word alignment and native results stay unchanged. |
| `match_dispatch::each_dispatch_family_gets_a_word_only_when_all_families_fit` | The same exact copy checks require both family words, at offsets 12 and 20, once each. Family selection and layout expectations stay unchanged. |
| `match_dispatch::assert_handler_load` (handler-word, family-count and cursor callers) | Trace the actual indirect tail-call target through its aligned pointer load and word GEP to the received element. Earlier copy GEPs may share the offset, and copies may load the tag as i32; tag-switch/table dispatch remains forbidden. |
| `windows::a_projected_window_target_is_formed_once_before_rhs` | Count loads from the captured target field address, excluding the two Box-pointer loads that copy Columns. The complete target chain, unique projections and RHS/store order remain required. |

Unchanged intrinsic expectations were inspected in `arrays` (array-backed Record
assignment and place_back), `owned_places` (array-backed result transfer and large
owning swap), and `containers` (array-backed take/swap transfer). Their collection
fields or exceeded bounds intentionally retain the old fallback. Paged directory
growth and dynamic array-range memmove are separate operations, unchanged here.

SLP, MemCpyOpt and code-generation combines can re-merge ordinary nonvolatile
accesses. This IR does not impose a machine access-width promise; no volatile,
atomic, inline assembly, global vectorizer restriction or artificial dependency
is added. If final loads widen, evaluate late target-aware lowering explicitly.
Extra instructions, register pressure/spills, scratch storage and code size may
outweigh forwarding benefits. AArch64 paired/vector transfers may benefit from
wider accesses, so inspect its output rather than extrapolating x86 timing.
Targets outside the current qualified set are untested.

### Halo timing of both steps

The owner chose this direction (status board card on the store-forwarding
fix, option A, 2026-10-10). Same Halo source (Halo-wf main d7ad06d), full LTO,
native 14900K, `taskset -c 2`, six interleaved pairs per comparison against the
control and a byte-identical twin (Whitefoot run 38093924096): control
`wf-fd49ea518e16` (main fd49ea518), experiment 1 `wf-exp-cd79ad705c03`
(destination-result parameter read-through), experiment 2
`wf-exp-5fe3dd3e4e7a` (experiment 1 plus layout-bounded transfers). Each cell
is the median ratio to the control (lower is faster); the 1-, 3- and 6-pair
runs agree within the twin's spread unless noted.

| Kernel | Twin | Experiment 1 | Experiment 2 |
|---|---:|---:|---:|
| fib | 0.999 | 1.016 (1.011-1.023) | 0.955 |
| loop | 1.000 | 0.959 | 0.965 |
| integer-table | 1.005 | 0.861 | 0.858 |
| string-key | 1.001 (0.947-1.001) | 0.933 | 0.956 |
| concat | 1.005 | 0.992 | 0.958 |
| sort | 0.992 | 0.997 | 0.979 |
| binary-trees | 1.003 | 1.026 | 0.974 |

Experiment 1 removes the hot `table_get` key capture (SetTableRR, `arm.13`,
679 to 453 instructions) and speeds integer-table, but alone it slows fib by
about 2%. Experiment 2 speeds every kernel. Its final code shows the predicted
risk only in part: Value transfers became scalar (ForLoop `arm.66`, 36 to 19
128-bit moves; whole program 3680 to 3158), but LLVM re-merged adjacent 8-byte
Frame fields in `push_frame` into three 16-byte moves (12 to 8 moves), so the
Frame copy still has wide loads over its two widest field pairs. Keeping those
boundaries through LLVM would need the late target-aware lowering named above;
it is not part of this change.
No specification, acceptance, verdict, diagnostic or ABI change is proposed.


On 2026-10-10 the permitted prebuilt `whitefootc --check` accepted the exact
new fixture source. Changed Rust files received `rustfmt --edition 2024` from
`compiler/`; incidental formatting in unchanged child modules was removed.
Read-only review against step 1 (`110412ad49ae988558ea7bc50779ee5a494c232c`),
including the untracked test module, checked the repository/documentation,
safety, case ownership/wiring and design correspondence groups. It found a
rejection-list format issue, masked bound coverage and an insufficient byte-copy
ordering oracle; all were repaired and the changed coverage/layout hunks received
limited follow-up review with no new finding. This is inspection, not executed
backend evidence. Rust compilation, baseline failures, emitted IR assertions,
native overlap/cleanup behavior, gates, optimized/LTO widths and other-target
code generation remain pending CI. No local build, Cargo, Make, native test or
performance run was made; no approval or specification log was written.
