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

### Original emission rule approved with #345

The original rule below records the approved experiment; the proposed
[producer-sensitive selection](#producer-sensitive-transfer-selection) narrows
when its unchanged layout plan is used.

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
Frame copy still has wide loads over its two widest field pairs. For Frame,
the machine-width criterion stated above therefore fails: the frontend-only
remedy does not keep its boundaries through LLVM. The bounded emission is kept
because the criterion that decides cost holds: six kernels are faster than the
control beyond the twin's spread in all three pair counts (string-key, whose
twin varied from 0.947 to 1.001, is faster in every count but within that
noise), and the Value transfers, which the
integer-table and loop stalls involve, do keep their boundaries. Keeping the
Frame boundaries through LLVM would need the late target-aware lowering named
above; it is deferred to the status board item for this work, not part of this
change.
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
backend evidence at that point; the gate later passed on both code commits
(runs 38092297049 at `cd79ad705` and 38092299946 at `5fe3dd3e4`), and the
optimized/LTO widths and timing are in [Halo timing of both
steps](#halo-timing-of-both-steps). Baseline failures of the new cases were not
executed, and other-target code generation remains unverified. No local build,
Cargo, Make, native test or performance run was made.

## Transfer inline-cost regression

Question: does recognizing ordinary looping immutable consumers remove the
unnecessary capture that PR #345 introduced into Halo's `table_get`, restore
its full-LTO inlining, and account for part of the reported concat slowdown? Compare
the unchanged Halo source with the pre-#345 compiler, current compiler and
candidate through CI, inspecting emitted IR, actual full-LTO remarks and
final disassembly before paired 14900K timing. A remaining private key
capture rejects the placement claim; unchanged full-LTO call sites reject
the inlining claim. Restored inlining without restored timing rejects an
explanation of the entire concat regression. Snowghost
requires its own same-source compiler comparison and hot-function evidence.

### Supplied evidence and inspection

The supplied artifacts compare `wf-9497f0a54fec` and
`wf-exp-b8cf83108946`, whose merged change is PR #345 at `13243c0af`.
The owner reports 74 non-inlined `table_get` calls after that change versus
one before, and a 3.2% concat slowdown on the 14900K in a later compiler
comparison. These are supplied observations, not measurements made here.
The following quotations were inspected in the supplied artifacts on
2026-10-11; line numbers identify those files, not newly generated output.

Before, `ir-table_get-wf-9497f0a54fec.ll:7–8` captures both inputs at entry:

```llvm
call void @llvm.memmove.p0.p0.i64(ptr %wf.slot.0, ptr %wf.arg.v1, i64 ptrtoint (ptr getelementptr (%wf.t.90128e7f982cc38a, ptr null, i32 1) to i64), i1 false)
call void @llvm.memmove.p0.p0.i64(ptr %wf.slot.1, ptr %wf.arg.v2, i64 ptrtoint (ptr getelementptr (%wf.t.90128e7f982cc38a, ptr null, i32 1) to i64), i1 false)
```

After, `ir-table_get-wf-exp-b8cf83108946.ll:44` reads the key directly,
but lines 70–82 materialize three granules before the looping consumer:

```llvm
%v16 = call i64 @wf_package.halo.heap.integer_key(ptr %wf.arg.v2)
; layout-bounded transfer from %wf.arg.v2 to %wf.slot.1
%t13 = load i32, ptr %wf.arg.v2, align 1
%t14 = getelementptr inbounds i8, ptr %wf.arg.v2, i64 4
%t15 = load i32, ptr %t14, align 1
%t16 = getelementptr inbounds i8, ptr %wf.arg.v2, i64 8
%t17 = load i64, ptr %t16, align 1
store i32 %t13, ptr %wf.slot.1, align 1
%t18 = getelementptr inbounds i8, ptr %wf.slot.1, i64 4
store i32 %t15, ptr %t18, align 1
%t19 = getelementptr inbounds i8, ptr %wf.slot.1, i64 8
store i64 %t17, ptr %t19, align 1
; end layout-bounded transfer from %wf.arg.v2 to %wf.slot.1
%v53 = call %wf.t.612fba5367897926 @wf_package.halo.heap.node_find(ptr %v19, ptr %v52, ptr %wf.slot.1)
```

`wf-exp-b8cf83108946.ll:27038,27068` shows that `node_find.body`
already passes its incoming key directly to `main_position` and `equal`.
Its backedge at line 27056 returns to an `i1` bounds test, not a nominal
enum dispatch header. The consumer classifier nevertheless rejects every
reentered CFG block to ensure the body cannot split. The call's checked
read-only reference facts and inline own-value representation admit source
stability; no earlier result write requires this capture on this path.

The supplied O3 remarks show `heap.table_get` cost 230 becoming 250 at
`heap.table_border`, whose threshold is 250. At `embed.install`, the before
cost 220 names `embed.table_get`, whereas the after cost 240 names
`heap.table_get` after the wrapper separately inlined at cost 40; those are
different callee symbols and stages, not a same-callee cost comparison.
Another direct pair has `heap.table_get` into `embed.table_get` cost 220
before (inlined) and 270 after (not inlined), at threshold 250. These direct
pairs establish a cost change for the combined transformation, not
an isolated cost attribution to one capture or every result transfer.
The redundant cold copy is one concrete source of additional instructions;
other layout-granule result transfers also differ. Caller address passing
predates #345: the before and after function headers use the same indirect
input ABI.

The actual driver hands the complete WF module and runtime units to clang
with `-flto=full` and the shared `-O2` optimization arguments
(`compiler/src/bin/whitefootc.rs`, `compile_executable` and `link`;
`compiler/src/driver.rs`, `HOST_OPTIMIZATION_ARGUMENTS`). LLVM therefore
performs compile-time and full-link-time optimization with the native
consumer's pipeline; `opt -passes='default<O3>'` is supporting evidence,
not its inlining oracle. No full-LTO cost or candidate speedup is established
by the saved O3 remarks.

### Proposed remedy and alternatives

Replace acyclicity with the existing conservative dispatch storage
partition: a callee qualifies only when every block remains in the enclosing
partition. Complete unexposed single-origin own storage, nonreturned slots,
synchronous execution and all existing source-stability tests remain
required. A possible split remains excluded even if a particular target
would ultimately emit it whole. No recursive storage-plan selection is
introduced. This removes a redundant transfer rather than changing the
representation of required transfers.

Blanket `noalias` on indirect inputs is incompatible with the ABI's permitted
result/input overlap. `readonly` alone does not forbid writes through an
aliasing result pointer. Neither is a substitute for the missing consumer
classification. Reverting all granule transfers would also affect #345's
independent producer-store forwarding benefit; it would hide an avoidable
copy and needs separate cost and forwarding evidence for required copies.
Function-name-specific inline attributes and source workarounds are excluded.

No source-language, specification, conformance or callable ABI change is
proposed. Baseline failures, candidate emission, native behavior, full-LTO
inlining and Halo/Snowghost timing remain for CI and the owner's experiments;
no local build, check, test or optimization run is authorized for this task.

The backend case `destination_parameters_read_through_ordinary_loop_consumers`
uses a union-laid-out Value and explicitly checks the caller's public
destination ABI and the reader's real CFG cycle. It requires the early scalar
reader and later counted-loop reader to receive the same incoming pointer
without private capture. The old cyclic-body guard rejects that consumer,
requesting a capture and failing both observations. In the same native image,
an exposed mutating consumer and a possible split-dispatch consumer must still
receive captures; hot, cold and zero-iteration results and an unchanged source
are observed with retained calls. Existing destination-alias and layout-width
cases remain unchanged. Fixture admission and executed baseline failure are
unverified until CI.

### Concat loop: established limits

Direct inspection finds the entire emitted `concat_batch_copy` function
identical before and after (`wf-9497f0a54fec.ll:56178–56459` and
`wf-exp-b8cf83108946.ll:60564–60845`). Its byte `place_back` helper is also
identical (lines 177414–177429 and 194443–194458 respectively). The loop
already has the same reference alias facts on both sides:

```llvm
define i8 @wf_package.halo.vm.concat_batch_copy(ptr noalias nonnull captures(none) dereferenceable(728) %v0, ptr noalias nonnull captures(none) dereferenceable(8) %v1, ptr noalias nonnull captures(none) dereferenceable(8) %v2, i64 %v3, i64 %v4, i64 %v5) #0 align 64 {
```

`concat_step`'s parameter signature also remains identical. Its local change
is two stack Value relocations changing from memmove to granules after the
main concat call; `concat_batch.body` changes its final Value transfer too.
The immediately-before-#345 disassembly already has the same scalar byte loop
and reloads, at `56ab8–56adb`, as the after image at `58068–5808b`:

```text
before 56ab8 / after 58068: mov r8,QWORD PTR [rdx+0x28]
before 56abc / after 5806c: cmp rdi,QWORD PTR [r8]
before 56ac1 / after 58071: mov r9,QWORD PTR [rbx]
before 56ac4 / after 58074: cmp r9,QWORD PTR [rbx+0x8]
before 56aca / after 5807a: movzx r8d,BYTE PTR [r8+rdi*1+0x10]
before 56ad0 / after 58080: mov BYTE PTR [rbx+r9*1+0x10],r8b
before 56ad8 / after 58088: mov QWORD PTR [rbx],r9
```

Thus this pair disproves a hoisting/unrolling loss at the #345 boundary.
The loop loss reported against the older `wf-78223721f77d` comparison is
attributed to a separate boundary below, in the PR #344 investigation.
No new in-place byte-loop input, destination parameter or removed alias
attribute appears in the raw loop. Changing alias promises to repair this
purported #345 loop regression has no supporting evidence.

The saved O3 remarks reject `concat_batch_copy` into `concat_batch.body`
with cost 150 and threshold 45 on both sides, and reject `concat_batch`
into `concat_step` with costs 1015 before and 1010 after at threshold 250.
The full-LTO disassembly nevertheless contains the byte loop inside
`concat_step`, demonstrating why these O3 remarks cannot identify that
build's decisive inlining event. Candidate full-LTO inspection must settle
whether removing the redundant consumer captures restores `table_get`
inlining; the older loop regression needs its own before/after boundary.

### Snowghost: bounded source inspection

The inspected Snowghost source at
`903dafc2f20452b2e6ea0362b4c259c112051c54` has an aggregate append path:
[`push_component`](https://github.com/Ming-Research/Snowghost-wf/blob/903dafc2f20452b2e6ea0362b4c259c112051c54/renderer/css/syntax/buffers.wf#L47)
passes Component to `push_within`, which grows if needed and calls
`place_back`. The
[Component payload](https://github.com/Ming-Research/Snowghost-wf/blob/903dafc2f20452b2e6ea0362b4c259c112051c54/renderer/css/syntax/module.wfm#L32)
includes Numeric and Span records, so this path exercises aggregate transfer
machinery. Neither append helper loops; this is not a witness of the
ordinary-loop consumer exclusion fixed here. Required granule transfer cost
remains a plausible investigation target. No Snowghost benchmark workload,
measured source revision, emitted IR or before/after optimization evidence
was supplied, so this inspection establishes no slowdown attribution and
does not claim that this patch resolves it.

### Review and validation state

A fresh read-only `gpt-6.1-sol` review at high effort inspected the complete
four-file working-tree diff against
`0f779c7f4bf924bfc315400d8773fd04ebd881cd`, changed contexts and direct
storage/emission consumers, the applicable repository, documentation, safety,
case and validation checklist items, and design checks G1–G3/DC1–DC4.
It found one cost-comparison wording defect: the initial `embed.install`
comparison named different callees. That was corrected and reread against
the remarks. No remaining finding was reported within scope. The primary
agent verified the added direct same-callee cost pair and Snowghost source
excerpts. No specification or interface change or additional contract gap
was identified; target-dependent unsplit dispatch candidates remain
conservatively excluded.

Review is inspection only. Fixture admission, baseline failure, candidate
IR, native outcomes, existing store-forwarding checks, full-LTO recovery and
performance remain unverified. No local build, test, formatter, lint, gate,
optimizer or measurement ran. The owner committed this four-file fix as
`7a76c286c` and opened draft Whitefoot PR #357. No approval record or status
board was written.

## Runtime storage alias facts after PR #344

### Question and falsifier

The owner's ten-release disassembly comparison places the concat byte-loop
change between `wf-bae97fdcf890` and `wf-179dc9cf1345`, the latter built from
PR #344, which makes an empty contiguous runtime window allocate nothing.
The question is whether exposing disjoint storage roles restores hoisting
without changing that behavior. Compare the same Halo source and full-LTO
settings before #344, after #344 and with these facts; inspect optimized IR
and the loop before timing. Failure to remove the repeated source-owner,
source-length and destination-capacity loads rejects the proposed repair
for this loop. Interleaved timings with a base twin remain necessary for
any performance attribution. No such candidate observation has run yet.

### Boundary and emitted shape

The supplied full-LTO `concat_step` disassembly before #344 loads the source
pointer and length at `5a074` and `5a078`, outside a loop with two byte-copy
instructions at `5a0ae` and `5a0c4`. After #344 its loop instead contains:

```text
56ab8: mov r8,QWORD PTR [rdx+0x28]
56abc: cmp rdi,QWORD PTR [r8]
56ac1: mov r9,QWORD PTR [rbx]
56ac4: cmp r9,QWORD PTR [rbx+0x8]
```

The source diff `bae97fdcf..179dc9cf1 -- compiler/` changes construction
from an allocator-derived result to a join of fresh storage and a global:

```llvm
%v1 = phi ptr [ @wf__empty_window, %window.block.allocate.v1.empty ], [ %t1, %window.block.allocate.v1.init ]
```

This is the saved Halo byte-Slots constructor (`wf-9497f0a54fec.ll:129999`),
emitted by `emitter/runs.rs::emit_window_block_new`; the Array constructor
makes the same choice. The runtime adds:

```c
_Alignas(64) const unsigned char wf__empty_window[64] = {0};
```

The saved later pre-#345 Halo IR confirms the external immutable header
(`wf-9497f0a54fec.ll:179596`), and the concat path creates its destination
through the changed constructor (`56776–56780`):

```llvm
%v267 = call ptr @wf_box_slots_new$instance$d0714e30cb3ae434(i64 %v264)
store ptr %v267, ptr %v268
%v271 = call i8 @wf_package.halo.vm.concat_batch_copy(ptr %v249, ptr %v250, ptr %v268, i64 %v251, i64 %v265, i64 %v270)
```

The byte placement helper (`177414–177429`) expresses element and header
accesses without alias metadata, including:

```llvm
%t0 = load i64, ptr %t1
store i8 %v1, ptr %t2
%t3 = load i64, ptr %t4
store i64 %t5, ptr %t6
```

The first and third loads read the Slots length; the byte store addresses
its element storage. Their disjointness is absent from these instructions.
Header-first layout alone is insufficient when the optimizer lacks
provenance for the constructor's mixed result. Reference `noalias` applies
to the referenced owner word, and does not grant freshness to the backing
pointer loaded from that word.

The precise regression mechanism supported by the diff is loss of an
always-allocator-derived constructor result, with no replacement access
facts. `wf__heap_take` still returns malloc storage; `wf__heap_retake` adds
a fresh-allocation branch when given the shared header, and otherwise still
uses fresh allocation or realloc. No allocator return changes to a global.
The evidence does not include optimized LLVM at the exact #344 boundary,
so which inferred LLVM attribute or alias query first changes is unverified.
The empty-result branch is the causal hypothesis tied to the observed
boundary; the proposed facts directly address the missing distinctions.

### Proposed mapping and soundness

Use classic scalar-access TBAA with a common ancestor and sibling roles:
explicit numeric source writes, typed Box-owner accesses, runtime Array,
Slots, Ring and Paged lengths, runtime capacity, Ring head, and Paged
directory entries and directory capacity. Same-role accesses may alias;
there is no per-owner promise. Numeric source reads remain untagged:
`read_u64(&window.len)` is admitted and can read a generated measure.
Only runtime-capacity window headers receive these roles. Fixed-capacity
windows can be inline elements or enum payloads and are deliberately
untagged. Source constructor, whole-value, aggregate and transfer accesses
remain untagged, including payload storage reused by a variant change;
generated initialization of fresh runtime headers may use their known roles.

The specification grounds are TYPE-2 and TYPE-10 (opaque readonly measures
and inaccessible directory storage), TYPE-9 (runtime storage only as Box
contents), STOR-1 (backing and the immutable shared empty header), TYPE-5
and OP-4 (declared selected types), REF-1 and REF-2 (typed live selections,
readonly restrictions and invalidation on replacement), and OP-6's
primitive-only `reinterpret` row with OP-8's lowering. TYPE-8's prohibition
on stored references helps lifetime reasoning but alone proves no type
punning rule. Physical overlap on enum replacement remains possible,
which is why construction and whole-value writes carry no sibling tag.
Different shapes may read the same shared empty bytes: those bytes are
immutable and never written through a role. Positive growth obtains
writable backing before updating measures.

The [LLVM TBAA contract](https://llvm.org/docs/LangRef.html#tbaa-metadata)
allows sibling scalar access tags to communicate disjointness; an incorrect
overlap promise makes optimization unsound. Therefore classification is
by the checked source access or compiler-owned storage role, never merely
by LLVM integer/pointer width. Unknown accesses retain ordinary aliasing.

Alternatives: constructor return `noalias` is invalid for shared accessible
empty backing; allocator-only return facts cannot describe the mixed
constructor or separate Paged directory and element accesses; the header
is already emitted `constant`, which by itself did not preserve this loop;
per-access scoped noalias requires activation/provenance tracking across
inlining, whereas the selected roles need no owner exclusivity. No
allocation, representation, source spelling or acceptance change is needed.

### Paged scope and evidence limits

The same directory-versus-source-write distinction targets Snowghost's
reported `sg-paged-elem-dir-alias` symptom: scalar element field stores
cannot change a directory entry. The Paged representation places directory
entries in the cell and elements in separately allocated pages. Typed
Box-valued elements use the Box-owner role, never the directory role.
This gives one general lowering scheme for both storage families. No
Snowghost optimized IR or timing accompanies this report; removal of its
inter-field page-pointer reload and any speedup remain unverified.

An independent `gpt-6-astra` high-effort architectural audit found the
narrowed mapping sound under these emission restrictions and identified
no specification gap. Its audit does not validate the implementation or
establish candidate optimization. Backend cases in
`compiler/src/backend/tests/storage_alias.rs` inspect the runtime byte-copy
loop's Box/Array/Slots roles and explicit capacity and Ring measures,
conservative numeric readers and nested inline header reads and mutations,
Paged's two-field element writes versus directory reads, conservative enum construction and
whole-value transfers, and metadata preservation through encoded modules
and both fragment granularities. The byte-copy and Paged cases also include
native result checks for CI. Baseline IR has no TBAA, so their positive
metadata assertions detect the original missing information; the conservative
and roundtrip assertions protect boundaries of the new implementation.
Fixture admission, executed baseline failure and candidate results await
the owner's CI. No local build, test, formatter, lint or optimizer ran.

A fresh read-only `gpt-6.1-sol` high-effort completion review inspected
`7a76c286c..working tree`, including both new alias source/test files,
affected emitters, fragment/cache transport, specification grounds and
design ancestors. It reported no remaining finding after corrections to
an unused format argument, the missing typed Box projection tag, the
constructor test's ABI-body selection and the design node's source
construction qualifier. Applicable repository, documentation, case and
design correspondence checks passed inspection; complete optimizer
soundness, fixture admission, LLVM verification, native behavior and
performance remain unverified. No metadata-withholding switch is required
for acceptance: these annotations change no operation, and LLVM may ignore
them; empirical facts-off equivalence has not been demonstrated.

### Executable alias-boundary witnesses

The enum transition case in `backend/tests/storage_alias.rs` now reuses one
union enum's storage for 64 rounds: a Number payload is written and read,
replaced with a Box-owner payload, read and written through that Box, then
replaced with Number and written and read again. It checks all six observed
values per round, confirms common payload placement, retains untagged
constructor/transfer assertions and requires tagged scalar writes and
Box-owner loads. Stale owner bytes can cause a fault or fail the Box checks
(exit 3/4); stale pointer bytes exposed as Number fail exits 5/6. These
observations exercise the role transition; they do not establish that every
possible incorrect annotation necessarily changes optimized output.

The Paged relocation case grows a Pair window from capacity zero to 1025
inside one loop. Each round appends an element, writes and checks both new
fields, and writes and checks page-zero fields through the current
directory; a final pass checks every retained element after page zero.
The 16-byte Pair crosses five 256-element pages and outgrows directory
reservations of one, two and four entries. Stale directory/owner reads may
fault, leave new fields zero instead of `i+17`/`i+29` (exits 1/2), or access
the wrong retained page (exits 3–6).

The new case in `backend/tests/read_through.rs` passes a consumed four-word
Row to a looping destination-result callee and asserts equal result/input
pointers at the actual call. One loop reads the original input in place;
construction then writes the aliased result, and a second loop must read
the persistent captured input. The case checks capture placement in IR,
all four returned words and observations for 32 input seeds. Missing or
late capture yields `3*7 + 3*99 = 318` instead of `6*7 = 42` on the first
seed (exit 1). Both ordinary optimized emission and retained call boundaries
run through the existing native helper.

These cases use `compile_and_run`, whose ordinary host flags include `-O2`.
Their source admission, IR observations and native results await CI; no
local build, test, formatter, check, optimizer or wrong-tag mutation ran.

CI on `74def15e1`, before these additional native cases, failed nine backend
unit tests on Linux and macOS. The supplied failure extracts identify five
fixture problems: reserved `copy`, prelude-colliding `append`, a Pair
constructor used directly as a placement argument, and arithmetic used as
a Number constructor argument in three read-through functions. The corrected
fixtures use unique names and bind values before atom-only call arguments;
the shared copy fixture also repairs the cached-fragment case. The newer
native cases now bind nested calls and comparison arithmetic, remove a WF
comment and use canonical arm layout, without changing their observations.
Field/argument order and loop exits were inspected against the grammar.

The other four CI failures were IR-text matches that treated a TBAA suffix
as part of an address or instruction. The array target-preparation check,
the shared split-loop Box reload detector and the projected-window target
check now strip only `, !tbaa !` before exact operand matching. Their
target identities, reload requirements, counts and ordering checks remain
intact. Corrected fixture admission and all native and IR outcomes still
await a new CI run; no local checks were run to substitute for it.

### Found along the way: readonly measure reference formation

A separate implementation gap was found while preparing the conservative
numeric-reader case. The specified witness is a nonwriting function
`read_u64(value: &u64) -> u64` called with `&source^.inner.len`, where
`source` is a runtime Array Box. TYPE-2 permits a readonly path at a
nonwriting reference parameter, TYPE-10 defines the measure as a readonly
field and REF-1 admits field-reference formation. Static inspection instead
finds reference formation calling `resolve_storage_path`
(`semantic/check/references.rs:1349`); non-index suffixes go to
`elaborate_place_member` (`expressions/flat_storage.rs:1616`), whose
`PlaceMember` supports only ordinary fields and Box contents
(`expressions/places.rs:35`), and whose nominal-only selection rejects a
runtime descriptor field (`places.rs:894`). An executed rejection is not
available under the no-local-test rule.

Disposition: deferred from this alias-emission change. Reopen with that
minimal witness, implement ordinary measure reference formation and cover
its readonly restriction and live-reference invalidation through CI; no
workaround or acceptance change is added here. Generic numeric reference
loads remain conservative now so this future source support requires no
change to the alias contract. The backend case observes an ordinary numeric
reference reader instead of claiming that this measure witness already
reaches the backend. This gap is recorded here because the task forbids
status-board writes.


## Unavoidable union result transfers

Question: after removing the key materialization, does retaining a whole-value
union transfer at the public result boundary restore Halo's `table_get`
inlining in the actual `clang -O2 -flto=full` build? Compare the unchanged
Halo source with `wf-exp-4753d28fb4ae` and the candidate, inspecting emitted
IR, full-LTO call sites and remarks before paired timing. An interval-expanded
public union transfer rejects the emission claim; unchanged full-LTO call
sites reject the inlining claim. No new timing or optimizer run was performed
in this round.

The owner supplied experiment-release results for `4753d28fb` (Whitefoot
runs 38153853900 and 38155407651): 74 out-of-line `table_get` call sites,
versus one in `wf-78223721f77d`; the saved O3 remarks show cost 230 at
`embed.install` with threshold 250, and cost 260 at `embed.table_get` and
`vm.metamethod`. These were read in the supplied artifacts, not regenerated.
The compiler driver uses `HOST_OPTIMIZATION_ARGUMENTS` (`-O2`) and
`compile_executable` adds `-flto=full`. LLVM's
[inline-cost source](https://llvm.org/doxygen/InlineCost_8cpp_source.html)
sets the default threshold to 225 and chooses the aggressive threshold only
above optimization level two. This explains why an O3 inlining observation
at cost 230 is insufficient evidence for the real build. Per-call bonuses,
simplification and full-LTO sequencing still require actual build remarks;
230 is an O3 observation, not a newly measured O2 cost.

`tg.ll` shows that the candidate has removed both entry captures and the
key transfer. Its only `node_find` call returns a register-sized index option:

```llvm
%v53 = call %wf.t.612fba5367897926 @wf_package.halo.heap.node_find(ptr %v19, ptr %v52, ptr %wf.arg.v2)
```

The two Value result paths are snapshots of a heap array element and a
node's Value field, respectively, already copied directly into `%wf.result`:

```llvm
%t29 = getelementptr inbounds { i64, i64, [0 x %wf.t.90128e7f982cc38a] }, ptr %v45, i64 0, i32 2, i64 %v25
%v46 = getelementptr i8, ptr %t29, i64 0
; layout-bounded transfer from %v46 to %wf.result
%t30 = load i32, ptr %v46, align 1
%t31 = getelementptr inbounds i8, ptr %v46, i64 4
%t32 = load i32, ptr %t31, align 1
%t33 = getelementptr inbounds i8, ptr %v46, i64 8
%t34 = load i64, ptr %t33, align 1
store i32 %t30, ptr %wf.result, align 1
%t35 = getelementptr inbounds i8, ptr %wf.result, i64 4
store i32 %t32, ptr %t35, align 1
%t36 = getelementptr inbounds i8, ptr %wf.result, i64 8
store i64 %t34, ptr %t36, align 1
```

The other path has `%t53 = getelementptr ... ptr %t50, i32 0, i32 2`,
`%v81 = getelementptr i8, ptr %t53, i64 0`, followed by the same three
loads and three stores into `%wf.result`. Before PR #345, each path was
one intrinsic, for example:

```llvm
call void @llvm.memmove.p0.p0.i64(ptr %wf.result, ptr %v46, i64 ptrtoint (ptr getelementptr (%wf.t.90128e7f982cc38a, ptr null, i32 1) to i64), i1 false)
```

Thus destination forwarding cannot remove these copies: there is no callee
Value result or local intermediate to redirect, and the heap value must
remain independently observable after the call. Existing return-group
coalescing and `value_place(result)` already pass unchanged callee results
into the caller's public destination. A relay witness now asserts that
behavior without changing its implementation.

The first #357 policy kept one whole-value memory intrinsic for a top-level
union enum transferred into the public result destination; that exception is
now superseded by [producer-sensitive transfer selection](#producer-sensitive-transfer-selection). LLVM can lower
that intrinsic after its inlining decision in caller context. Under that initial policy, internal
union transfers and every stored struct transfer, including structs with
union fields, retained the typed granules that motivated PR #345. Its grounds
were provisional: the supplied cost difference motivates the boundary, but
the candidate's cost and inlining have not been measured. It changes no
layout, snapshot, effect or result/input alias contract; `memmove` preserves
partial overlap, and captures still precede potentially invalidating result
writes.

Alternatives: general call-result forwarding already applies and cannot
remove a heap snapshot; reverting all small transfers would discard the
independent struct store-forwarding experiment; reverting all union
transfers would unnecessarily change internal transfer behavior without
corresponding cost evidence; inline hints or thresholds would tune a
consumer instead of addressing the emitted transfer.

Under that first #357 policy, `backend/tests/layout_transfers.rs` required one direct intrinsic and no
interval expansion for public union snapshots, while checking the exact
original intervals for internal unions, public structs and structs containing
unions. Those transfer-form expectations are superseded by the matrix below.
Its native heap-array/relay case checks an old snapshot of 17,
a replacement observed as 30, and an untouched element of 17, with call
boundaries retained. A stale snapshot would observe the replacement; a
missing transfer would leave the result incorrect. The old compiler fails
the positive intrinsic assertions. The handler-word tests in
`backend/tests/match_dispatch.rs` move their unchanged typed-interval
oracles to an executed internal source-to-target copy, and separately
require the public union intrinsic. Omitting that internal copy leaves
`Halt(0)` and returns 0 instead of 18. No native or IR test was run locally;
fixture admission and emitted results await CI.


The added consumed-union witness makes aliasing concrete rather than merely
permitting it in the ABI: both the unchanged-result call and the
write-then-observe call must pass identical input and result pointers. It
checks that the unchanged public transfer is an untagged `memmove`, and
that a capture precedes construction in the aliased public destination.
Across 32 Number seeds, an omitted or late capture observes the replacement
Function(99) as 100 instead of the original first seed 7 (exit 2); exits 1
and 3 check the unchanged result and replacement respectively. Both normal
optimized and retained-call native forms are wired through the shared
helper. This adds executed overlap coverage without changing the existing
result/input alias contract; admission and execution await CI.

## Readonly-owner access scopes

Question: after storage-role TBAA hoists Halo's source pointer, do access
scopes separating readonly reference roots from another reference root's
runtime-header writes let LLVM promote source length and destination
length/capacity across the byte loop? Compare the same Halo source with
`wf-exp-4753d28fb4ae` and the candidate through CI, inspecting full-LTO
IR/disassembly before paired timing. Metadata present without header
promotion rejects the optimization prediction; promotion without a timing
improvement rejects a claim that this accounts for the remaining slowdown.
Snowghost requires its own hot-loop and timing observations.

### Remaining alias evidence

The supplied `concat_step-wf-exp-4753d28fb4ae.txt` still contains:

```asm
cmp    rdi,QWORD PTR [rdx]
mov    r8,QWORD PTR [rbx]
cmp    r8,QWORD PTR [rbx+0x8]
movzx  r9d,BYTE PTR [rdx+rdi*1+0x10]
mov    BYTE PTR [rbx+r8*1+0x10],r9b
inc    r8
mov    QWORD PTR [rbx],r8
```

The saved O3 module retains `concat_batch_copy` separately (so it is
supporting evidence, not the final full-LTO inlined `concat_step`). Its
inner loop has source and destination lengths in the same TBAA class:

```llvm
%t34 = load i64, ptr %t24, align 8, !tbaa !4
%t36 = load ptr, ptr %v2, align 8, !tbaa !0
%t37 = load i64, ptr %t36, align 8, !tbaa !4
%t40 = load i64, ptr %t41, align 8, !tbaa !6
store i8 %v167, ptr %t2.i, align 1, !tbaa !11, !alias.scope !6899
store i64 %t5.i, ptr %t36, align 8, !tbaa !4, !alias.scope !6899
```

`!4` names the optimized module's runtime Slots length role. The scope on
the stores comes from the inlined `place_back` parameter, while the source
loads have no matching `!noalias` fact. Even the destination's owner-cell
parameter `%v2` is inferred `readonly`: loading its pointer does not modify
the cell, while the pointed-to header is modified. Ordinary reference
parameters already have `noalias`; adding that attribute again cannot
separate these loaded pointers. LLVM's
[pointer based-on rules](https://llvm.org/docs/LangRef.html#pointer-aliasing-rules)
do not make a loaded pointer based on the address of its owner slot.

GitHub API access to the requested pinned Halo source failed again during
the per-word repair (`error connecting to api.github.com`).
A saved read-only source copy has the matching `concat_batch_copy` loop:
`reads(vm), reads(stack), writes(bytes)`; it reads
`vm^.heap.strings.inner[si].payload.bytes.inner` and appends into
`bytes^.inner`. The exact saved copy's correspondence to the pinned branch
was not independently established; its pattern agrees with the supplied
LLVM. Source row coverage is EFF-2, inter-argument write separation is
EFF-5 (with OWN-7's path comparison), and REF-1/REF-2 govern the reached
paths and their validity. OWN-1 and STOR-1 give live nonempty Box contents
one allocation owner. STOR-1 explicitly permits distinct empty owners to
share an immutable header; allocation freshness is therefore unavailable.

LLVM's [scoped metadata contract](https://llvm.org/docs/LangRef.html#noalias-and-alias-scope-metadata)
has no read/read exception: accesses in the two separated collections must
not alias. Blanket per-owner disjoint scopes or `!noalias` on a generic
call would be too strong, because the call may read the same shared empty
header as a readonly source. The parameter `noalias` contract has a
modified-memory qualification that scoped access metadata lacks. A
readonly attribute alone also describes owner bytes, not every reachable
Box allocation. `invariant.load` would promise a longer immutability than
a checked call duration; balanced invariant regions would require an
additional lifetime mapping and matching exits.


### Proposed access mapping and implementation

The former mapping incorrectly treated the absence of declared writes through
one formal as coverage of every physical load through it, and treated every
boundary store through another formal as a declared write. A fresh review of
`4260c8688` found two admitted same-owner counterexamples. With
`reads(source.inner.head), writes(target.inner.next), writes(target.inner.len)`,
EFF-5 permits the same Ring Box for both arguments: head and next/len are
disjoint [WIN-2]. The back operation nevertheless emitted:

```rust
"  store i64 {new_head}, ptr {head_address}{}{}",
storage_alias::runtime_word(run_type, field),
owner_alias
```

That unchanged head rewrite was excluded from the reader's scope even though
both accesses touched the same word. The repair removes head stores from
back placement and removal, rather than annotating an undeclared write.
Likewise `reads(source.inner[lo..hi])` beside writes of `target.inner.next`
and `target.inner.len` permits sharing when the range is already filled
[WIN-2, EFF-5]. `SliceFromRun` loads the whole descriptor length before
forming its range count; the old `run_word` attached `owner_alias.read(run)`
to this implicit load although the declared range read excludes the length
word. The conformance case `eff5-pos-two-arguments-range-beside-append`
establishes the permitted sharing independently of the backend.

The revised mapping requires complete-state declared coverage. Lowering
retains separate whole-read and whole-write formal sets only for ordinary
reference parameters with a declared effect path rooted at that parameter
and with no suffix. A scoped reader must also remain readonly (no declared
write rooted there). EFF-1 states that a bare parameter names the complete
state it supplies: reads(root) covers every observed descendant, and
writes(root) covers every accessed descendant. Thus each exact physical
word in a tagged typed projection is covered, including a supporting
header load. An excluded runtime boundary store must have one whole-write
formal origin. EFF-5 compares these effects from different arguments and
rejects overlapping read/write state; OWN-7 supplies the path comparison.
Partial rows receive no scopes or exclusions, even when an individual
access could be proved separate with a more detailed path mapping. This is
an equivalent sufficient per-word criterion, not a claim that every read
through a readonly reference is exhibited in its row.

Eligible functions remain ordinary, synchronous and unsplit, with at least
one qualifying whole-state readonly reference and one whole-state writing
reference. Ranges, runs, waiting and overlapping functions, alias-permitting
rows and missing source signatures receive no scopes. A module-wide plan
assigns collision-free numbered metadata nodes after the fixed TBAA nodes;
each activation declares its readonly scopes with
`llvm.experimental.noalias.scope.decl`, so LLVM has the scope lifetime when
inlining or duplicating a loop.

A finite provenance propagation carries formal roots through typed
`ProjectAddress` operations and incoming block arguments. A use gets a fact
only when its origins are exactly one formal; unknown definitions explicitly
contribute an unknown origin, and mixed joins keep their uncertainty.
Reads never receive write-side exclusions. STOR-1's shared empty header is
immutable, so two read collections can still share it while no admitted
boundary store writes it. Source typed paths and reference validity
[REF-1, REF-2] remain premises. Scalar read scopes are needed as well as
header scopes: otherwise a source byte load could still alias a delayed
destination-length store.

A full per-word effect-path mapper could retain more facts for narrow rows,
but would need to distinguish explicit measure reads, implicit range
support, and every boundary's physical writes. It is unnecessary for the
observed concat row, whose source reads and destination writes are complete
roots. Withholding partial-row facts closes the correctness defect without
changing source spelling, effect rules, allocation semantics or runtime
checks. Blanket owner scopes and owner-pointer attributes still assert a
stronger contract than the language supplies.

Only the selected runtime header stores carry exclusions. Whole-value
construction/replacement/transfers, public result writes, owned by-value
arguments, unknown provenance, inline window descriptor writes and generic
calls remain unannotated. In particular this grants no `noalias` to a
read-through own input and makes no change to the result/input overlap and
capture contract introduced with destination-result placement. Withholding
these optional facts retains the ordinary operations and acceptance; no
source proof or runtime alias check is introduced.

The byte copy writes its destination header through `place_back`. Tagging
the call would also tag its header reads after inlining, which is too strong
for shared empty headers. Instead, a call whose complete lowered body is
one placing `RunBoundary` definition and a return of that unit definition,
with no extra blocks, block parameters or cleanup, expands by substituting actual
operands into the existing primitive emitter. Matching checks the complete
body, operand types and returned value; it uses no source function name,
marker or inline attribute. Every other callee keeps the ordinary ABI path.
Caller operand preparation and the same primitive implementation preserve
snapshot semantics and store ordering. This gives the access mapper actual
caller roots at the header stores without a general IR inliner.

This complements storage-role TBAA rather than replacing it. The existing
Paged directory/element-role separation concerns different physical roles
under one owner; readonly-root scopes concern reads versus another
reference root's boundary-header writes. The latter does not add scopes to
Paged directory entries or general element writes, so no additional claim
about Snowghost's same-owner Paged field-store reload is established here.

### Physical-access audit and concat preservation

All owner-scope attachment sites use the revised complete-state gates:

| Attachment site | Exact physical access and coverage |
| --- | --- |
| `operations::emit_load` | Scalar or Box-owner word reached by a typed projection of one whole-read readonly formal; EFF-1 covers that exact word and every reached descendant. No source store gets an owner exclusion here. |
| `runs::emit_run_measure` Array length branch | The Array header length word below the whole-read formal; partial length-only rows receive no scope. |
| `runs::run_word` | Runtime Array/Slots/Ring/Paged length, capacity, Ring head or Paged directory-capacity word. Explicit measures and implicit slice/boundary/projection support loads use the same gate: only complete-state read coverage licenses a scope. Fixed-capacity inline descriptors remain unscoped. |
| `runs::move_run_boundary` length store | Slots/Ring/Paged length word below a whole-write formal; the excluded scope belongs to another whole-read formal, so EFF-5 would reject sharing of that word. |
| `runs::move_run_boundary` Ring head store | Only front operations now store the changed head, under the same whole-write gate. Back operations emit no head store. |

Paged directory entries, generic element stores, whole-value transfers,
result construction and opaque calls gain no owner exclusions. Their
existing storage-role TBAA policy remains separate from these scoped
read/write facts; no new claim about Paged directory relocation follows
from this repair. Module scope declarations and definitions contain no
memory access of their own. Unknown/mixed origins and owned input/result
storage remain outside the mapping.

In the saved Halo loop, the source length and byte addresses descend from
`vm` and the destination length store descends from `bytes`.
`reads(vm)` covers the source header and payload word accesses;
`writes(bytes)` covers the destination length word. These are the complete
bare roots that the new gates retain. Therefore the source length/byte
loads still carry the vm scope and the destination length store still
excludes it after the single-primitive call expansion. Destination reads
receive no write-side exclusion; same-owner destination length recurrence
remains intact, while capacity/length roles are separated by the existing
TBAA. The stricter mapping removes neither half of the intended source-read
versus destination-length-store fact. This is IR-emission reasoning, not
new emitted Halo IR or evidence that LLVM hoists/unrolls the loop. The
pinned remote source remains unavailable in this session.

The scope definitions currently travel through the shared module header,
which the existing fragment splitter and cache serialize. The scope plan
is computed once per module rather than rescanning all functions for each
emitter. A limitation is that adding an eligible function/formal changes
shared metadata text and can invalidate otherwise unchanged fragment cache
entries. Selective metadata ownership is deferred from this performance
fix; reopen with an incremental-build comparison if that invalidation is
material. It is a cache reuse cost, not an alias or linking contract gap.


### Backend witnesses and remaining evidence

The same-role backend witness copies from a nested struct's `Box<Slots<u8>>`
into another `Box<Slots<u8>>`. It requires a source-length load and source-byte
load in the same scope, a destination-length store excluding that exact
scope-node collection, the matching activation declaration and no write-side
metadata on loads or calls. Its positive metadata assertions fail on the
base compiler. An existing Array-to-Slots assertion now matches the exact
TBAA attachment independently of following scope attachments; it still
requires the same load kind and the same Array length tag.

The executed witness checks destination length 16 and every byte equal to
7, in ordinary optimized and retained-call forms, plus a zero-trip copy
between distinct capacity-zero owners and two readonly empty-owner reads.
A lost header update leaves length below 16 (exit 1), and a missing or
incorrect byte transfer fails exit 2. This provides a result oracle for
primitive expansion and the annotations; it does not prove that every
incorrect metadata assignment necessarily changes this program's output.
Replacing the empty destination argument with the source owner's own Box
must be rejected as EFF-5, independently of the zero-trip runtime behavior.
The existing cached-module and function/module-fragment roundtrip witness
also checks the scope definitions and intrinsic declaration. None of these
fixtures or checks was run locally.

The new same-Box Ring witness appends and takes from the front for 16
iterations, checking each observed/current head, length and both byte values;
its final back take must preserve head 16. IR assertions require the explicit
head read to remain unscoped and both back primitives to update length but
emit no head store. The Slots range witness appends its current filled-prefix
range count for 16 iterations and checks counts 2 through 17, unchanged
prefix values 11/22 and final length 18. Its implicit header length loads must
be unscoped and its partial-row function must carry no exclusions. Each
executes normal optimized and retained-call native forms. The metadata/head
store assertions deterministically fail on the old emission; these native
programs are semantic oracles, not evidence that the old metadata must
produce a particular wrong output (the unchanged store and overwritten
preliminary range length may optimize away). Separate whole-read/partial-write
and partial-read/whole-write variants of the positive copy fixture require no
scopes, so omitting either new coverage gate fails independently.

Remaining uncertainties are fixture admission, LLVM verification on both
native toolchains, final full-LTO cost and inlining, header promotion and
unrolling, and same-source Halo/Snowghost timing. No inference here treats
O3 IR as the actual full-LTO optimization result, or attributes Snowghost's
slowdown without its own evidence. No language/specification contract was
changed. The scoped mapping deliberately covers only known whole-state
readonly roots and whole-state-covered runtime boundary stores; provenance
across opaque calls or general writes remains a compiler information gap
rather than a source restriction.


The previous round's read-only completion review inspected the changes
from `4753d28fb` through `4260c8688`, including emitters and their retained
checker facts, primitive expansion, fixtures, cache/fragment transport,
specification premises, design ancestors and the research additions. It
found no remaining issue after a local indentation repair; the subsequent
Astra review of `4260c8688` supplied the two physical-access counterexamples
above, superseding that round's alias-soundness conclusion. Applicable
repository, citation, case and design-correspondence checks passed inspection;
fixture admission/native outputs, target LLVM verification and activation
handling, the complete alias argument beyond local inspection, and actual
optimization/performance remain unverified. No build, test, check, formatter
or optimizer was run locally, and no board or approval log was changed.


A fresh independent read-only completion review of the per-word repair,
configured as GPT-6 Astra with high effort, inspected all eleven changed
files against `4260c8688`, the attachment sites, provenance, constructors,
fixtures and design/specification correspondence. It found no issue within
that inspection scope. Fixture admission, negative-control failures, native
execution, LLVM scope handling, CI gates and performance remain unverified;
no local build, test, formatter, optimizer or check was run.


## Producer-sensitive transfer selection

Question: does retaining one intrinsic for transfers without function-local
field-producer provenance restore Halo and Snowghost inlining without losing
more of #345's forwarding benefit than it gains? Compare the same sources
with `d9abe90b3`, the candidate and a byte-identical control twin through CI;
inspect full-LTO inline remarks and disassembly before paired timing on the
14900K. Rejection criteria are failure to recover the affected inline sites,
or a regression in previously improved kernels beyond the control spread.
No new optimizer run or timing is reported here.

### Evidence and instruction costs

The supplied Snowghost layout-oracle IR compares `wf-9497f0a54fec` with
`wf-exp-b8cf83108946`. `set_links.body` formerly copied its untouched incoming
40-byte Links value into a heap field with one memmove. After #345:

```llvm
; layout-bounded transfer from %wf.arg.v2 to %v10
%t7 = load i32, ptr %wf.arg.v2, align 1
%t8 = getelementptr inbounds i8, ptr %wf.arg.v2, i64 4
%t9 = load i32, ptr %t8, align 1
```

The remaining fields repeat this pattern through offset 36, then ten stores
write offsets 0 through 36. There are ten loads, ten stores and eighteen
address GEPs instead of one intrinsic. The supplied O2 remarks give the
body's inline cost as -15015 before and -14920 after, an increase of 95;
`set_links` into `unlink.body` changes from cost 0 to 45 at threshold 45.
Other supplied call sites change from 5 to 50 at that threshold. These are
saved observations, not candidate measurements.

LLVM 22's [InlineCost.cpp](https://raw.githubusercontent.com/llvm/llvm-project/release/22.x/llvm/lib/Analysis/InlineCost.cpp)
sets the ordinary threshold to 225, the cold threshold to 45 and the default
instruction cost to 5. Uneliminated loads/stores pay that instruction cost;
a memory intrinsic is assessed as one instruction, not its eventual machine
copy sequence. Acceptance requires cost strictly below threshold. Thus the
95-point increase is consistent with twenty memory instructions replacing
one (19 times 5); it is not a new independently measured cost breakdown.
Caller-specific SROA savings, simplification and other instructions can
change the actual inline cost. LLVM's
[SROA implementation](https://raw.githubusercontent.com/llvm/llvm-project/release/22.x/llvm/lib/Transforms/Scalar/SROA.cpp)
can split alloca-backed memcpy accesses when its analysis permits, while
preserving external transfer regions when scalarization is unhelpful. An
intrinsic does not guarantee a wide load, nor do frontend granules guarantee
that LLVM will retain their widths.

Halo `table_get` has two unavoidable Value snapshots already addressed
directly into `%wf.result`, as documented above: `%v46` is a heap-array value
and `%v81` a node value field. Each post-#345 result transfer is three loads
and three stores; neither source has a local field producer. The proposed
policy selects one memmove on each path: zero explicit transfer loads and
stores, two intrinsic sites total. Snowghost `set_links.body` likewise
selects one memmove instead of twenty explicit memory instructions. These
counts follow the saved source identities and the candidate selection rule;
rebuilt downstream LLVM and its optimized costs remain unverified.

### Chosen rule and alternatives

Select existing layout granules only for a physical source with retained
function-local MAY field-producer provenance. Constructors and field
insertions seed it; local field stores and initialized local backing can
supply it; typed inline projections, snapshots and CFG value transfers
propagate it. Formal inputs, opaque calls and Box/heap/container projections
supply no producer evidence. Actual read-through/incoming storage is checked
before using a value marker, so a coalesced slot cannot confer local provenance
on an unrelated external source. Mixed value flows may retain a possible
local producer. The marker is flow-insensitive: a later or branch-local
field write can influence selection, and calls do not clear it. This is a
profitability heuristic, not proof of a reaching store, exclusivity, capture,
initialization or lifetime. No new target
metadata or language fact follows from it.

The existing target plan decides whether scalarization is representable,
with the same types, offsets, pointer/Bool byte intervals, overlap-safe
capture ordering, 128-byte and 16-granule limits. Every untracked source
retains the intrinsic. The criterion is independent of function names,
destination ABI and struct versus union layout; the public-union exception
is removed. Waiting/split code gains no new storage reuse permission.

This is a provisional cost tradeoff, not a claim that local provenance solves
the original cross-call Frame witness. The historical Halo IR constructs
Frame with eleven stores in `enter_lua`, forwards it through `push_frame`,
and copies it in `place_back` from `%wf.arg.v1`. A function-local classifier
intentionally leaves that boundary copy as an intrinsic. #345's final Frame
machine-width criterion already failed because LLVM remerged field pairs,
while its Value scalarization and runtime improvements survived. This policy
may relinquish some of those observed external-Value benefits; only the
comparison above can establish the net result. The new local-producer tests
preserve the narrower emitted-boundary obligation, not the retired promise
that every incoming Frame copy expands.

- Merging adjacent plain integer leaves is representation-safe only under
  the existing representation rules; layout adjacency alone cannot establish
  the absence of a forwarding hazard from separate narrow stores.
- Lowering the cap cannot retain eleven-leaf Frame while excluding ten-leaf
  Links, and a cap small enough to exclude three-interval Value would discard
  the earlier Value experiment. The existing limits remain size safeguards,
  not a demonstrated profitability model.
- A caller-propagated field-written bit does not discriminate: the inspected
  Snowghost DOM source also updates Links fields immediately before
  `set_links`. Its exact identity with the saved experiment was not asserted.
- A complete reaching-byte-write analysis or deferred target-aware lowering
  could make a more informed decision, but neither is supplied by the current
  backend. Cross-call store widths and LLVM recombination remain an explicit
  optimization gap; the function-local policy does not pretend to solve it.
- Whole intrinsics everywhere would also discard locally visible narrow
  producers; union/result-only selection misses Snowghost's internal struct
  copy. Inline hints and threshold changes select consumers instead of fixing
  the general transfer cost.

### Approved sentence changed

The owner-approved #345 sentence in `storage-placement.md` began:
“Transfer stored structs of at most 128 bytes and at most 16 granules by
recursively loading each scalar leaf with its own LLVM type and width at
the selected-target field offset”. Its unconditional eligibility is changed
to require retained function-local producer provenance; the original layout
plan, bounds and capture order remain. `payload-enum-layout.md`'s corresponding
unconditional internal union expansion and the later proposed public-result
exception are replaced by the same source-sensitive rule. These decision
changes are proposed for owner approval; this record grants no approval.


### Backend evidence and retired expectations

`layout_transfers` retains the exact typed-load/store, target-offset,
SSA-correspondence and all-source-before-destination oracle. It now applies
to actual local field producers rather than incoming copies. This is an
intentional narrowing of the tested policy, not evidence that the earlier
cross-call Frame forwarding requirement still holds.

| Existing #345 guard | Proposed-policy observation |
| --- | --- |
| `struct_and_union_transfers_respect_leaf_boundaries` | Local Frame keeps eight mixed-width leaves, local Value keeps 4+4+8-byte intervals, and local Boundary keeps sixteen 8-byte leaves. Incoming Frame/Value/Boundary/Overlay copies now require intrinsics. Local 17-leaf, 136-byte and array-containing producers still check the original fallback conditions independently of source eligibility. As before, the 136-byte scalar fixture exceeds both bounds rather than isolating the byte bound. |
| Former `whole_union_results_keep_one_intrinsic_without_changing_internal_transfers` | Replaced by `nonproducer_transfers_keep_one_intrinsic_for_structs_unions_and_results`: external public struct/union/wrapped-union and internal ten-u32 copies require one intrinsic and no granule region; the same ten-field layout with a local constructor requires ten exact i32 load/store pairs. The incoming internal-union expansion expectation is retired. |
| `union_pointer_and_one_bit_intervals_use_byte_capture` | Local Overlay Bits construction, observed before consumption, retains the exact byte-interval captures, scratch object and all-captures-before-restores oracle; an incoming Overlay copy is now an intrinsic control. Representation checks still cover pointer/Bool overlays. |
| `bounded_transfer_preserves_equal_and_overlapping_storage_natively` | Extracts the actual emitted local Frame transfer region instead of the incoming copy. The same C byte-snapshot oracle still tests equal pointers and overlap in both directions; the copy emitter's overlap obligation is unchanged. |
| `handler_words_preserve_copies_replacements_tags_and_four_byte_alignment` and `each_dispatch_family_gets_a_word_only_when_all_families_fit` | Exact pointer-word intervals at offsets 12 and 20 move to a locally constructed Cell copy. Incoming internal and public Cell copies require intrinsics. The local copy is executed and its tag/payload observed before the original handler-copy/replacement sequence. |
| `payload_enums`, `read_through`, arrays, windows, owning swaps and container checks | Their existing complete-copy counts, source identities, snapshot placement, no-copy assertions, native values and unsupported-shape/large-copy fallback requirements already accept either transfer form and remain unchanged. |

The external ten-u32 intrinsic assertion fails the pre-policy compiler's
twenty-memory-instruction transfer. Public struct and internal union controls
also distinguish the policy from the former union/result exception. The
new native ten-field case checks all ten values through local construction,
public return and internal copy in both normal optimized and retained-call
forms. The handler fixture checks the locally copied Add(7,2) and observer
result 9 before its earlier sequence. These are execution paths wired into
the maintained backend group, not locally obtained results.

A positive public union-result control requires the local 4+4+8-byte
transfer into `%wf.result`, so reinstating the former public-result exception
would fail it. An untouched local clone of the ten-field input remains an
intrinsic; changing one local field selects ten exact granules. Native
observations check the public union payload and both ten-field copies. These
controls distinguish local producer provenance from merely allocating local
backing, and destination-independent selection from the superseded exception.

Remaining evidence: Rust compilation, WF fixture admission, the new IR
assertions, native execution/overlap, source-classifier analysis cost and
supported LLVM verification await CI. Downstream instruction counts above
are predictions from saved IR identities; actual inline costs, recovered
sites, retained machine widths and net Halo/Snowghost timing are unverified.
No specification, target layout, ABI or alias contract changes. The explicit
gap is target-aware profitability across calls; neither a local MAY bit nor
the unchanged layout bounds establish it.

A fresh independent read-only completion review, requested as `gpt-6-astra`
at high effort, inspected `d9abe90b3..working tree`, all twelve changed files
including the new transfer helper, affected callers, specification grounds,
design ancestors and saved inline remarks. It found no remaining issue within
source-inspection scope after adding the positive public local union-result
control and repairing atom-only WF operands. Exact emitted-boundary,
capture-order, handler-word and native-overlap obligations remain represented;
unconditional incoming expansion is intentionally retired. Rust compilation,
fixture admission, negative-control failures, emitted LLVM, native execution,
CI gates, analysis cost and downstream profitability remain unverified.
No local build, test, formatter, optimizer or check ran, and no board or
approval log was changed. The final prose clarification marks the earlier
union-only test policy as historical; it changes no additional decision.
