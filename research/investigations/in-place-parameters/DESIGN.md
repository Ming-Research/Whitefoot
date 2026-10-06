# By-value parameters read in place

## Question and scope

A stored aggregate parameter crosses the call boundary as a pointer to the
caller's storage of the value it consumes (`ptr %wf.arg.v<N>`,
`compiler/src/backend/abi.rs`), and every definition copies it into a slot
of its own at entry with one `llvm.memmove` (the entry copy in
`FunctionEmitter::emit`, `compiler/src/backend/emitter.rs`). When the host
optimizer inlines such a definition, scalar replacement splits that copy
along the aggregate's representation. For a union-laid-out enum
(compiler/payload-enum-layout) the pieces are single bytes.

PR #245 observed this in firn, the Redis-compatible server. Its
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

- `a_by_value_parameter_nothing_writes_is_read_in_place`
  (`compiler/src/backend/tests/payload_enums.rs`):
  - an eligible reader of a union-laid-out enum has no copy;
  - firn's store shape runs with inline and boxed texts, and every owner is
    released once, with and without retained call boundaries.
- `destination_results_keep_snapshots_of_inputs_their_caller_aliases`
  (`compiler/src/backend/tests/owned_places.rs`): pins the destination
  condition. Its caller passes one pointer as both the result destination
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
