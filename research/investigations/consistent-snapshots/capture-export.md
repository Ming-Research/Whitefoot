# Typed restricted snapshot export

## Scope and status

This is the implementation study and phase-2 plan for the proposed v0.126
snapshot amendment in [the active specification](../../../spec/kernel-spec.md).
The specification owns the callable declarations, admission rules, byte
grammar and outcomes. This record owns the implementation questions and
comparisons; it establishes no implemented capability or performance result.
Phase 1 changes the specification, design decisions and conformance cases.
Compiler and runtime implementation follow review of that draft.

The owner's 2026-10-11 choice was a fixed typed export with no user execution
in the child, using one value-type argument and an ordinary callable surface.
It rejected a restricted callback and a dedicated capture statement. The
[snapshot survey](README.md#owner-decisions) retains the earlier comparison
and service-first direction. Neither choice establishes a physical COW bound.

## Questions and falsifiers

Can a prepared export capture a complete held map and its sequence cut,
stream its transitive owned state using only the qualified native capsule,
and leave all collection and reaping obligations with a linear parent owner?
The comparison is a serial operation-history model, independent of the map
implementation and encoder. A mismatched record, missing or duplicate key,
incorrect cut, accepted incomplete stream, child callback, inherited runtime
access, or unreaped child rejects the implementation.

Can bounded encoder scratch and streaming avoid a second whole dataset or
whole-record buffer? Compare long records and deeply recursive values with
the same small preparation. A hidden heap allocation, child stack growth
proportional to value depth outside the reserve, whole-record staging, or
reserve overrun without its specified error rejects that claim. A snapshot
that exhausts its reserve may fail; service mutations continue.

Physical COW growth is a separate experiment. Compare no capture, captured
read-only data, repeated hot-key changes, uniform replacement, deletion,
access-metadata changes and a blocked collector under matched traces. The
survey's overwrite argument rejects an unconditional sub-2x footprint with
unlimited mutation and guaranteed export completion. Child-only RSS limits
and sampling followed by killing do not prove a combined parent/child bound.

## Interface gaps exposed by the draft

### Ordinary declarations do not express a held capture contract

The constitution's [ordinary-object requirement](../../../docs/constitution.md#external-interaction-through-ordinary-objects)
requires host interactions to use the same contracts as other calls.
Existing function declarations can express references, effects, ownership,
waiting and proof conditions. They do not express that a reference is the
whole-map binding of the current atomic statement, or that a scalar is read
from another state that statement holds. A direct-call restriction introduces
an operation-specific admission rule rather than deriving it from an ordinary
reference/effect row. It must be reviewed as such, not inferred from native
implementation origin.

Minimal semantic witness:

```text
forward(reserve, map: &ConcurrentHashMap<V>, cut: u64):
    return capture_export<V>(reserve, map, cut)
atomic whole_map and journal:
    forward(reserve, whole_map, journal.sequence)
```

The caller has the required hold, but the wrapper's declaration does not
carry it. The proposed first surface requires a direct capture call. General
held-state contracts would admit checked forwarding; that alternative adds
an interface and proof mechanism beyond this prototype.

### The native capsule does not collect a large data pipe

[`fork_capsule.h`](../../../compiler/src/backend/completion/fork_capsule.h)
is a native witness interface with an audited encoder pointer, caller-owned
output descriptors and a separate bounded status pipe. Its native
[`finish`](../../../compiler/src/backend/completion/fork_capsule.c) waits
for exit before reading status. Applying that order to a large data pipe
deadlocks when the child fills the pipe before exit. Source finish must drain
or discard unread output while awaiting exit, then reconcile stream framing
with the exit/status report. Cancellation needs a new confirmed kill-and-reap
path; the native interface currently supplies no cancellation operation.

The native parent-start operation restores the calling thread's signal mask,
closes the child status writer and attempts pidfd creation. These steps occur
outside dataset holds. Lowering must arrange them after atomic release,
including every return, give and break edge. Capturing inside the hold cannot
simply call the native convenience start operation.

There is also a descriptor-order mismatch to resolve: the draft prepares all
descriptors before acquiring the hold, while the native parent-start opens
its optional pidfd afterward. The source integration must use an owned
waitpid path with preprepared pipes, or the owner must explicitly qualify
the draft to permit that optional post-hold monitor descriptor. Neither
choice permits descriptor work inside the hold.

The native witness header also requires one calling thread to own
prepare/start/finish and outlive the child. Ordinary linear reserve/job values
can move between contexts and waits may resume on another driver. Phase 2
must qualify ownership and reaping across that movement, while keeping signal
arm, held fork and signal restoration on the same native thread. The witness
header's condition is not already a source-job lifetime implementation.

### Prototype reserve is not the production service-first footprint bound

The draft can bound prepared scratch and total emitted bytes and return an
export error on exhaustion. It cannot account kernel COW through the ordinary
heap meter: parent changes, allocator metadata and unrelated private mappings
also diverge while the child lives. A captured child may retain nearly the
entire old resident image. This draft supplies neither a hard combined
physical-memory reserve nor a maximum wall-clock cleanup or capture pause.

Before production eligibility, resolve the survey's service-first condition
with a conservative worst-case image reservation or another independently
qualified enforcement policy. Measuring `/proc` private pages can characterize
the route; it cannot turn polling into a hard bound. Confirmed termination
precedes reclamation. A fixed cleanup operation count is different from a
wall-clock bound on host signal delivery and reaping.

The proposed collection read takes a deadline and cancellation watch. A
cancelled or expired read returns control with the job still owned; it does
not certify that the child has stopped. The caller can then invoke consuming
abort to confirm termination and reaping. A read borrows the job, so another
context cannot consume that same job during an outstanding read. Cancellation
delivery and confirmed abort still supply no wall-clock cleanup bound under
arbitrary host delay.

The proposed job also exposes no process identity or child footprint meter.
The parent heap meter and resident observation do not include the child's
private retained pages. A native CI observer can measure them for the route
comparison; adding a source monitoring API would be a separate ordinary
interface choice and would still supply no hard physical bound.

## Draft judgments for owner review

These are proposed choices, not additional owner rulings. Exact constants,
tags and declarations are owned by the specification draft.

| Draft choice | Alternative | Ground and limit |
| --- | --- | --- |
| `std::snapshot` module and one value-type parameter | Put functions in `std::process`; add key or encoder parameters | Dataset capture is the abstraction, while the existing concurrent map fixes binary keys and the child encoder is supplied. |
| Direct call in a whole-map-and-cut atomic statement that does not write held state | Permit forwarding wrappers or a capture amidst writes | Direct checking establishes a single stable cut without a general held-state contract; the ordinary-host conflict above still needs an owner ruling. |
| Transitive owned type-graph constraint, including private fields and every variant | Check only the current live variant or public fields | The type contract must remain valid after mutation and through module opacity; an unselected resource-bearing variant remains inadmissible. |
| Logical pairs for shipped HashMap and OrderedMap, logical sequence order for Ring and Slots | Encode vacant/deleted buckets, capacities and tree nodes | Output consumers need values independent of container storage history; traversal can inspect captured structure without user callbacks. |
| Typed structure for other source structs and enums | Invoke user serialization callbacks | All owned fields can be encoded without admitting user code into the child closure. |
| Exact concrete type graph as an external schema; include private owned fields | Carry a self-describing schema; export only public fields | No whole-schema buffer is needed and all owned state is covered, but the consumer must know the exact declaration graph and field privacy is not an export confidentiality boundary. |
| Refuse fieldless opaque types uniformly, and all Shared, SharedRead and Frozen handles | Whitelist host origins or follow immutable shared referents | A finite owned graph supplies the traversal boundary; frozen retention and fieldless opaque representations need a separate semantics before admission. |
| Permit several captures in one read-only atomic statement | Limit a statement to one capture | Each independent reserve has its own attempt and job; read-only held state gives every successful image the same cut without an arbitrary count restriction. |
| Bounded segmented streaming and explicit terminal outcome | Buffer each record and treat EOF as success | A record can exceed scratch; complete framing and child status distinguish completion, truncation and encoder error. |
| Bit-preserving floating-point encoding | Decimal conversion or normalized NaNs | Integer bit transport preserves infinities, signed zero and NaN representations without child formatting or arithmetic. |
| Linear prepared reserve and job, explicit consuming cleanup | Automatically finalize dropped job handles | Ordinary linearity makes reaping an obligation on every exit; hidden drop cannot encode native cleanup. |
| Separate child outcome, collection outcome and abort-request observation | Collapse simultaneous failures to one precedence-selected error | A signal death can coincide with truncation or a requested abort; preserving these facts makes the completion evidence interpretable. |
| One data pipe and a separately bounded status pipe | Spend output quota on an error trailer | Exhausting the data quota can still report an explicit export failure; the status pipe carries no dataset records. |
| Deadline and cancellation on read, with consuming finish or abort afterward | Make cancellation itself consume the job | Ending a read must retain the owner until termination and reaping are confirmed; cancellation does not establish cleanup. |
| Bounded framing/schema validation in the collector | Recompare output with the captured map or keep all keys for duplicate detection | The runtime encoder owns semantic correctness; the independent test oracle checks it without turning collection into a second dataset. |
| Charge dedicated pipe resources to the prepared reserve | Spend shared HandleFactory credits | The reserve owns exactly the resources that move to the job, while a factory parameter would add a second lifetime/accounting owner; simultaneous reservations can still be refused by the host. |
| Provisionally fixed ancillary caps with scratch and output limits supplied separately | Count only scratch, or choose an unexplained aggregate allowance | Pipe and control costs are visible independently of traversal/output; the selected constants have no measurement support yet. |
| Cap encoder scratch and total output, measure COW separately | Claim reserve covers total physical retention | The caps can be enforced before scratch growth and writes; production service-first COW enforcement remains unresolved. |
| Unsupported platform diagnostic after source acceptance | Source rejection or transparent copy/versioning fallback | Platform qualification belongs to the target path and a substitute would change the measured mechanism. |

## Phase-2 implementation plan

All paths below already exist except the named new snapshot module and future
encoder/witness files, which belong beside their existing callers.

| Work | Files and evidence |
| --- | --- |
| Register and carry host declarations | New `lib/std/snapshot/module.wfm`, `lib/std/modules.wfg`, `compiler/src/library.rs`; resolution/module tests in `compiler/src/resolution/tests.rs`. Preserve published declaration ordinals. |
| Check held capture and resource-free closure | `compiler/src/semantic/check/control/atomic.rs`, `compiler/src/semantic/check/expressions/calls/user.rs` and type checking; semantic tests plus phase-1 conformance cases. Follow aliases, private fields, every enum alternative and finite recursive type graphs. |
| Lower typed traversal and post-hold continuation | `compiler/src/lowering/builder/atomic.rs`, `compiler/src/lowering/specialize.rs`, `compiler/src/backend/emitter.rs`, `compiler/src/backend/emitter/shared.rs`, `compiler/src/backend/emitter/frames.rs`; lowering/backend tests for every atomic exit edge and explicit target diagnostics. Full-map and descendant reads must prevent unsafe lock narrowing. |
| Integrate preparation, collection and reap | `compiler/src/backend/completion/fork_capsule.{c,h}`, `compiler/src/backend/completion/bridge.{c,h}`, `compiler/src/backend/runtime.rs`; new encoder units beside the capsule. Integrate read deadlines/cancellation without losing job ownership. Use bounded iterative traversal and call no inherited allocation, TLS, locks or runtime queues in the child. |
| Expose captured table traversal | `compiler/src/backend/concurrent_map.{c,h}`, `compiler/src/backend/keyed_table.c`; native tests for captured old entries after parent replacement/free. Call no hash/equality/comparison callback. |
| Test lifecycle and faults | `compiler/src/backend/completion/fork_capsule_test.c` and the completion group in `compiler/Makefile`; saturated output, creation and encoder failure, absent/malformed status, signal death, abort, exact reaping and reserve boundaries. Demonstrate each wrong result fails its check. |

The conformance schema has no target selector or host-conditional expectation;
target diagnostics need compiler tests unless that schema is amended. All
builds and test runs belong to owner-run CI in phase 2.
The phase-1 cases cover source admission only: SNAP-2 byte-format behavior
has no executable conformance evidence yet. Before claiming conformance,
settle how `tests/conformance/runner.py` and
`compiler/tests/conformance/adapter.rs` express required-platform execution
and unsupported-target outcomes, then add exact-byte, segmentation and
failure-prefix cases without treating unsupported compilation as source
rejection or silently skipping cases. No runner or verdict schema changed
in this draft.

## Maintained Whitefoot witness

Add a source program under `tests/programs/` and its runner in
`compiler/tests/programs/`, wired through `compiler/tests/corpus.rs` and the
existing program support. Keep its independent serial-history oracle beside
the program, outside research. Use the ordinary callable interface and
compiler-shipped library; a native custom exporter would evade the question.

The program needs all five owned value forms together: binary strings,
wrapped rings of byte strings, sets, hashes with deleted buckets, and a
multi-node ordered score/member index, with fractions, both infinities and
the specified NaN behavior. Include long and short binary bytes, numeric
strings and expiry metadata at zero, past and future values. Publish each
mutation and its sequence in one map-and-log atomic statement, including
multi-key EXEC-like and script-like write groups before and after capture.
Continue a writer after capture; free and replace captured owners in the
parent before the exporter finishes.

Decode the specified stream with an oracle that checks byte framing,
segmentation, counts and complete logical values against independent replay
through the captured cut. An intentionally selected post-cut live image must
fail this same checker for a snapshot mismatch. Truncating a large segmented
record, deleting the terminal frame and injecting an error terminal must
produce distinct failure evidence. Use matched short traces first; timing
and footprint comparisons belong to later authorized CI experiments under
the [survey's measurement protocol](README.md#comparison-fixed-before-future-measurements).

Firn's supplied closure and witness request motivate these forms but do not
select a special implementation path. Its downstream session must verify the
released compiler with its own source and independently measured criteria;
[downstream releases](../../../design/compiler/downstream-releases.md)
defines the adopted artifact path.
