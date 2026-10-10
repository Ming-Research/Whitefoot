# Frozen dataset library: expressibility witness

## Pre-registration, before execution

Written against `a69d5afc7d37f65dd34e58366ba9ad5c01b15a2c`, on
`claude/snap-lib-witness`. No build, compilation or execution has been performed
for this witness. This record fixes the expected results before its first CI
run; it is not a result or a claim of implemented snapshot support.

**Question:** can an opt-in persistent string-keyed map express replacement
updates with shared unchanged nodes, one-point capture, enumeration beside
continued publication, and last-reader reclamation under existing rules?

The selected direction is decision 2 of the
[snapshot investigation](../../investigations/consistent-snapshots/README.md#2-how-should-a-frozen-dataset-be-represented-and-exposed),
with its [implementation contracts](../../investigations/consistent-snapshots/README.md#contracts-required-before-implementation)
and [open questions](../../investigations/consistent-snapshots/README.md#questions-the-design-must-answer),
recorded in the [frozen dataset decision](../../../design/language/data-model/frozen-datasets.md).
This experiment supplies the four-operation prerequisite, not the later
resource-reserve, cancellation, file-publication or performance contract.

## Program and expected outcome

One complete program, [witness.wf](witness.wf), shares the implementation and
oracle between its serial ownership checks and its two-context check. Expected:
**compilation succeeds and execution exits 0**. Every nonzero result remains a
failure to investigate; none is pre-registered as an acceptable rejection.

The representation is an unbalanced binary search tree. A `Node` holds two
owned `Tree` edges and a `SharedRead<Pair>`; `Pair` owns its byte-string key in
`Box<Array<u8>>` and an inline `u64` value. A nonempty edge is a
`SharedRead<Node>`. `freeze` moves its input into `Shared<T>`, returns a read
handle and releases the only writable handle. No writable node or payload
handle escapes. The graph is acyclic by construction; shared edges always
point to previously constructed immutable values. There are no host resources,
external writers, mutable nested shared objects, saved references or map-entry
borrows in the captured graph.

The BST is the simplest comparison tree for this witness: no balancing, hash
function, capacity policy or collision machinery. `edit` and `compare` accept
arbitrary byte strings, including empty strings and proper prefixes. The
bounded execution uses the one-byte strings `a`, `b`, `c`; it does not qualify
all key lengths, tree shapes or deletion histories.

| Operation | Expression and independent expected observation |
| --- | --- |
| Update | `edit(tree, key, Some(value))` inserts/replaces; `None` deletes. `node_view` retains a node's handles without reading or copying its descendants. Recursion constructs new nodes on the selected path, retaining the other child and unchanged payload. Deleting a two-child node joins its left and right subtrees by copying the left subtree's rightmost path. Insert `b=20,a=10,c=30`; enumerate as `a=10,b=20,c=30`. Replace `a` with 11, delete `b`, delete missing `b`, then delete `a` and `c`; compare each resulting state to a separately constructed sorted list, including the empty list. |
| Capture | One `atomic` on `Shared<Current>` retains the root and reads its generation together. The writer privately changes both `a` and `c`, then publishes their complete root and generation in one atomic replacement. The reader races the first publication: its generation must be 0 or 1, and its entire map must match that generation's independently specified pairs. No intermediate root is published. |
| Enumerate | In-order traversal retains child handles during each short node hold, leaves that atomic block, and then descends. Payload reads likewise use separate atomic holds. During traversal the writer must publish generation 2 after the first visited pair and generation 3 after the second. The reader must still see precisely its captured sorted list, once per key, with no extra or missing entries. After joining the writer, the live root must be generation 3 with `a=13,b=20,c=33`. |
| Reclaim | In the quiescent ownership phase, capture two readers of generation 0, then publish `a=11`. Retaining roots must allocate no dataset storage. With both versions retained, growth must equal **two Node objects plus one Pair and its key allocation**: the untouched `c` subtree and `b` payload remain shared. Dropping the first reader must free nothing; dropping the last must return exactly to the original three-key live heap level. Removing all live keys must return exactly to the empty publication-cell level. Returning from the phase must restore its caller's baseline. |

Expected pairs are ordinary inline `Array<Expected, N>` values built from
literal operation results, never enumerated from an earlier implementation
output. Enumeration checks count, byte length, key order and values. The
capture generation selects between two complete predefined expectations; it
does not supply observed values to the oracle.

Allocation sizes are independently calibrated by constructing one `Pair` and
one `Node`, rather than by running `edit`. All measured keys have the same
one-byte length, and each type has the same allocation size regardless of its
child handles. Calibration must release completely and report nonzero costs.
The expected replacement growth follows the known three-node shape: new `b`
and `a` nodes and one new `a` payload. Cloning the untouched `c` node adds a
third Node charge and fails this equality; deep copying adds payload charges
too. Root retention, first-reader drop and last-reader drop each have distinct
checks. Counter differences and sums use explicit modular arithmetic; the
experiment's tiny allocations are far below `u64`'s range.

Heap readings occur before any spawn or after joining the writer, with no
other allocating context and without `--par`, following the quiescent comparisons in
[memory_statistics.wf](../../../tests/programs/memory_statistics.wf).
Reclamation is isolated from driver/pool allocation, not inferred from RSS or
allocator reserves. The serial phase's two retained roots represent two
outstanding readers. The concurrent phase also measures releasing its actual
raced capture: after the writer joins, its generation-0-or-1 tree and the live
generation-3 tree share only the `b` payload. Dropping that capture must free
exactly three Nodes and two Pairs with their keys. Both readings follow the
join, so context teardown is outside the measured difference. Neither check
claims bounded reclamation latency.

The entry consumes `Inputs`, destructures `Directory`, and closes both its
read and write handles. The root context is the reader; `spawn writer` is the
second context. Guarded atomics on a separate `Progress` object coordinate
publication between visits. There is no sleep, polling loop or elapsed-time
assertion. The reader always requests the final publication and joins before
returning an oracle failure, so an incorrect short enumeration cannot strand
the writer. Waiting operations never occur inside another atomic block.

## Failure codes and interpretation fixed in advance

| Exit | Mismatch |
| --- | --- |
| 1 | Calibration did not release to baseline. |
| 2, 3 | Pair or Node allocation cost was zero. |
| 4 | Capturing retained roots changed the heap count. |
| 5 | Replacement did not allocate exactly the expected shared path. |
| 6 | Dropping the first reader changed retained heap. |
| 7 | Dropping the last reader failed to restore the live three-key level. |
| 8 | Deleting all keys failed to restore the empty publication-cell level. |
| 9 | Leaving the ownership phase failed to restore the caller's baseline. |
| 11–15 | Initial insertion state is wrong. |
| 21–25 | A retained old root changed after replacement. |
| 31–35 | Replacement state is wrong. |
| 41–45 | Two-child deletion state is wrong. |
| 51–55 | Deleting an absent key changed the expected contents. |
| 61–65 | Removing all keys left an entry. |
| 70 | Capture returned a generation outside its permitted race. |
| 71–75 | Concurrent enumeration differs from the captured state. |
| 80 | The writer did not finish at generation 3. |
| 81–85 | Final live contents are wrong. |
| 86 | Releasing the raced capture did not free exactly three Nodes and two Pairs/keys after the writer joined. |

Within each five-code range, offsets 1–5 mean an extra pair, wrong key length,
wrong key/order, wrong value, or a missing pair, respectively. The first
observed content failure is retained while traversal finishes its handshake.
The shell prints compile and run statuses separately; a compiler exit code
is never interpreted as a program code. GNU `timeout` status 124, a signal,
missing tool or compiler internal error is an unsuccessful observation, not a
source-language rejection or proof of inexpressibility.

**Falsifier for expressibility:** isolate the first operation that cannot be
accepted under the current specification while preserving this ownership,
sharing and publication contract. Stop that operation and retain a minimal
complete rejecting program, its diagnostic and exact normative rule. Explain
the missing capability before proposing a rule change. A valid program the
compiler cannot compile is a compiler implementation gap; it is not evidence
that a new language storage domain is necessary. A runtime mismatch rejects
this implementation or its runtime support, not by itself the language's
expressiveness. No expectation, ownership rule or representation is weakened
to turn either result green; no whole-dataset copy, retained writable node,
runtime safety substitute or alternative spelling is a fallback.

## Rules relied on

These are line locations in [the active specification](../../../spec/kernel-spec.md)
at the recorded base, not changes to it.

| Rule and base line | Use in this witness |
| --- | --- |
| TYPE-8, 548; STOR-5, 882 | Edges and captured roots are owned handles, never stored or returned references. |
| TYPE-9, 556 | Runtime-sized key arrays live only in `Box<Array<u8>>`; handles and payload records may be stored and returned. `ConcurrentHashMap` can only be a shared state, so it is not embedded as a mutable captured payload. |
| OWN-1, 652; PROV-6, 761, 773, 799 | Affine owners move; retaining uses `shared_read_share`, not copying a Box. Generic freeze/discard require `T: drop`. Moving into `discard` gives an explicit early release boundary. |
| STOR-1, 811; STOR-7, 841 | Box owns heap storage, inline arrays hold the oracle; relocation never duplicates an ownership obligation. |
| STOR-3, 855–880 | Exactly-once compiler-derived scope release, including releases of fields and overwritten affine values. |
| SHARE-1, 2267–2272 | Creation, read-handle retention, no read-to-write conversion, and state release after the last handle and executing atomic statement are gone. |
| SHARE-2, 2285–2309 | Read-only node/payload targets; atomic references do not escape; separate short holds make waiting traversal legal without nested atomics. |
| SHARE-3, 2311–2317 | Root and generation publication/capture each have one atomic observation point. |
| WAIT-1, 1533; WAIT-2, 2246; WAIT-3, 2256 | `waits` functions, guarded progress, value-only spawn arguments and the structured join. No wall-clock progress claim. |
| PRE-2, 2560 | Quiescent `heap_in_use` readings count live heap holdings, not RSS, stacks or allocator reserves. |

`Shared`, `SharedRead`, `Box`, `ConcurrentHashMap`, `Slots` and `Array` are
prelude declarations (PRE-1), not source implementations in `lib/std`.
[lib/std](../../../lib/std/README.md) supplies host module interfaces and source
collections; [process](../../../lib/std/process/module.wfm) supplies the meter
and entry types. The source follows the retained-handle and atomic-exit forms
in [shared-read lifetime cases](../../../tests/conformance/cases/shared-read-run-lifetime-exits.wf)
and the Inputs/resource convention in
[memory_contexts.wf](../../../tests/programs/memory_contexts.wf).

## CI invocation and unresolved evidence

[run.sh](run.sh) compiles the one program with
`compiler/target/gate/whitefootc`, then executes it once and prints both exit
statuses. It records compiler diagnostics, program output and `results.tsv`
under `$OUT` (default `$RUNNER_TEMP/frozen-dataset-witness`, or
`/tmp/frozen-dataset-witness`). It requires GNU `timeout` on the Linux runner.
The 300-second compile and 60-second run limits bound infrastructure waiting;
they select no language verdict and constitute no performance experiment.

The temporary [workflow](../../../.github/workflows/frozen-dataset-witness.yml)
runs only on pushes to `claude/snap-lib-witness`, on `ubuntu-24.04`. It installs
the native toolchain, fetches locked Rust dependencies, builds with
`make -C compiler build`, runs the script, and uploads the revision, host,
specification digest and logs. It follows the context-starvation script and
workflow read from `origin/claude/ctx-handoff`, retaining only the construction
and reporting needed here. Remove the workflow before opening any pull
request. No canonical gate, specification, library or compiler is changed.

Still unverified: source acceptance and canonical spelling; recursive
SharedRead node construction/lowering/release; proof/effect composition of
path replacement; exact heap accounting for shared node/payload release;
and the spawn/atomic handshake's runtime behavior. CI must settle these.
The race may observe either generation and one run need not observe both;
the forced publications during traversal must all complete. This is one
small expressibility witness, not a schedule exploration, balance/performance
comparison, proof of arbitrary histories, memory-reserve guarantee or platform
qualification beyond the named CI host. No specification or design decision
is amended, and no language obstruction has yet been established.

## Read-only completion review

A separate Codex/GPT-6 reviewer inspected all four new files against
`a69d5afc7d37f65dd34e58366ba9ad5c01b15a2c`, including the post-join
reclamation check, the specification, governing design nodes and ancestors,
existing examples and workflow precedent. Applicable repository, citation,
research-boundary, construction, design-consistency and correspondence checks
passed by inspection; executable evidence and DC4 remain unverified pending
CI. There were no outstanding findings within that scope.

Found along the way and fixed: three missing `doc` semicolons (GRAM-2), three
invalid local array-literal declarations (GRAM-4/OP-13), and two effect rows
with noncanonical read/write order (EFF-1). The reviewer reinspected these
local repairs. The author also corrected FORM-2 spacing between constants;
the reviewer confirmed it. These repairs leave the trace and expectations
unchanged. No unrelated defect or new language decision was identified.
No local build, compilation, test, script execution or commit was performed.

## Results

**Run 1** ([CI 38036398318](https://github.com/Ming-Research/Whitefoot/actions/runs/38036398318),
revision 3fa888734, GitHub-hosted ubuntu-24.04, WF_DRIVERS=2, WF_WORKERS=1,
no `--par`): the witness compiled and exited **9**. No other failure code
occurred, so every check inside the ownership phase passed: insertion,
retained old roots unchanged after replacement, replacement and two-child
deletion contents, absent-key deletion, capture without heap change, exact
shared-path allocation on replacement, and release to the live and empty
levels when readers drop. Under the pre-registered criterion the witness
**fails**: after the ownership phase returned, `heap_in_use` exceeded the
caller's baseline.

**Run 2** ([CI 38040834762](https://github.com/Ming-Research/Whitefoot/actions/runs/38040834762),
revision 9968bd763, same host type and settings) printed the readings and
ran `diag.wf` (step meanings in [DIAGNOSTICS.md](DIAGNOSTICS.md)):

- The witness's residue is **8,192 bytes** (1,536 before, 9,728 after).
- A function performing every ownership operation of the witness (Shared
  cells and nodes, SharedRead handles, Box payloads, the Progress object)
  returns with **0** bytes above its caller's baseline, on the first and on
  a repeated call (steps 020 and 022).
- A depth-256 waiting recursion with **no** Box or Shared operation leaves
  **8,192** bytes after its first return and **0** more after a repeated
  identical call (steps 024 and 026).

**Interpretation.** The residue is runtime frame storage retained for reuse
(a frame-arena chunk kept as the context's spare), not storage of the
persistent library: the ownership operations alone return to baseline, a
frame-only control reproduces exactly the witness's 8,192 bytes, and the
retention does not grow on repetition. Update, capture, enumeration and
reclamation of the library are therefore **expressible under current rules
and behaved as expected** in these runs; the witness's whole-process
baseline check fails because `heap_in_use` counts a retained, empty frame
chunk. Whether PRE-2 should count such a chunk is a runtime accounting
question outside this witness; it is recorded for the runtime's owner. The
pre-registered failure stands as recorded.
