# Typed restricted snapshot export

## Scope and status

This is the implementation study and phase-2 plan for the snapshot
amendment in [the active specification](../../../spec/kernel-spec.md).
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

## Existing rules and the missing relation

The constitution says: “Their required relations belong in ordinary callable
contracts. An external role grants no source-language exception. No declaration,
type, ownership, effect, proof, diagnostic or semantic rule distinguishes a value
or function by whether its implementation crosses the host boundary.”
[Source](../../../docs/constitution.md#external-interaction-through-ordinary-objects).
The former SNAP-1 direct-call admission violates this condition; an owner choice
to retain it would require changing the constitution, not just approving a host
API. It is rejected in the revised draft.

Ordinary references already almost supply the required mechanism. REF-3 says
“References never escape” and permits a reference to be “bound to a local and
passed as a call argument.” EFF-2 says “The resulting ordinary read or write
footprint is retained at the call” and “An effect path grants no permission.”
SHARE-2 already admits a whole-map reference “passed through aliases or calls”;
it also says “The guard and the block contain no call to a waiting function.”
SHARE-3 gives an atomic statement one effect point: “every read and write they
make of those states takes effect at that point.” These rules establish safe
borrowing, ordinary forwarding and non-suspension. FN-8 expression contracts
carry integer/predicate requirements, not reference-origin relations.
[Rule definitions](../../../spec/kernel-spec.md).

The missing relation is a callable boundary requiring several reference
arguments to originate in the same caller-held statement, with their named
regions unchanged throughout that statement. A minimal semantic witness,
independent of export, is this signature fragment:

```whitefoot
fn read_pair(left: &u64, right: &u64) -> (first: u64, second: u64) reads(left), reads(right) contract {
  requires held(left, right);
} {
  return left^, right^;
}
```

A caller can pass projections of two states held in one atomic block, and a
wrapper declaring the same requirement can forward them. A plain `reads`
version also safely accepts a reference to a private local copy, or surrounding
writes to either input. Neither is a coherent unchanged caller-held pair under
this stronger API. That is an interface gap, not an existing memory-safety bug.
Consistent multi-object readers, audit/checksum helpers over held tables,
validation of coupled indexes, and observation wrappers share this need.

## Held-observation contract card

**How should an ordinary callable require a coherent caller-held observation?**

**Background.** The constitution quote and independent read-pair witness above
rule out privileging one linked exporter. References, effects and atomic
statements already provide most machinery; the missing part is their declarable
origin and whole-statement stability relation. The export now takes `cut: &u64`,
for example a reference to a held journal field. No scalar-history inference is
required, and the language gives the number no journal-specific meaning.

**Options.**

- **A — General `requires held(...)` (recommended).** One nonempty group of
  distinct reference parameters on a nonwaiting callable with a pure/read-only
  reference effect row. A body receives a symbolic common caller statement;
  aliases, projections and joins retain finite origins. Every actual alternative
  must have that origin. The complete caller block must leave the named owned
  regions unchanged; proved-disjoint sibling regions may change. Whitefoot and
  linked functions use identical checking, and wrappers and function formals
  forward it. Cost: grammar, ordinary boundary metadata and finite structural
  checking. Formation/refinement are linear in parameter names; checking uses
  retained path alternatives and protected-region/write-footprint pairs with
  existing OWN-7 overlap queries, with no new solver or body unfolding. This is
  not a claim of linear whole-program complexity. The initial row excludes
  borrowed output writes even when disjoint; private owned scratch and owned
  results remain possible. This deliberate scope limit can be reopened.
- **B — Retain operation-specific direct capture admission.** A smaller local
  checker change, but wrappers and function-kind substitution cannot express the
  relation. **Inadmissible under the current constitution**; it would require an
  explicit constitutional amendment and would encourage further privileged
  operations. The former scalar-cut provenance rule also duplicates reference
  reasoning. Rejected.
- **C — Ordinary read references only.** Take the cut by reference and rely on
  existing effects/borrows, with no new contract syntax. Constitution-consistent
  and cheapest; safe callers already inside an atomic block can use it. Its
  declaration does not require a shared held origin or whole-block unchanged
  regions, so private references and surrounding writes remain admissible. Choose
  it only if the API's stronger observation promise is withdrawn.
- **D — General held contracts with borrowed output writes.** Permit writes
  proved disjoint from protected state. More expressive, but requires specifying
  alias/disjointness facts for other borrowed parameters and their substituted
  effects; defer until a consumer needs that richer boundary. A does not claim
  this is intrinsically unsafe.

**Confidence 4/5.** Existing finite reference/footprint machinery supports A's
narrow relation without new runtime permissions. Implementation and checker
cost remain unmeasured. Evidence that only C's weaker promise is needed, or a
real need for borrowed outputs that A refuses, would reopen this recommendation.

## Rule-by-rule amendment and checking cost

| Rule | Prior draft | Revised draft |
| --- | --- | --- |
| GRAM-2 | Expression/range requirements | Adds the structural `held_clause`; FORM-3 consequently reserves `held`, requiring active source-identifier migration. |
| REF-1 | Finite reference path covers | Retains common held origin and protected descendant cover. |
| FN-1 | Effects, waits, expression contracts | Ordinary boundary also carries the optional held group. |
| FN-4 | Effect/template refinement | Actual held set must be a subset of formal held set; formal remains authoritative. |
| FN-8 | Requirement expressions become GoalTemplates | Dispatches structural held requirements to HOLD-1 before transfer. |
| HOLD-1 (new) | No declarable held relation | Formation, symbolic entry, actual origins, full-block protection and diagnostic locations; source and linked definitions use one judgment. |
| MOD-10 | Snapshot-specific supplied judgment reference | Points to encoder graph/lifetime judgments without granting held-call privilege. |
| DIAG-1 / DIAG-2 | No held clause carrier | Owner-dependent parameter names and erased structural evidence; existing ordinal scheme retained. |
| SHARE-2 / SHARE-3 | Direct capture restriction mentioned | General held requirement inherits atomic observation and waiting rules. |
| PRE-2 | Scalar cut and special admission | Reference cut, ordinary reads and `requires held(map, cut)`. |
| SNAP-1 | Encoder graph plus direct-call/provenance restrictions | Only encoder graph qualification, including concrete function selection. |
| SNAP-3 | Descriptor order unsettled; physical promise weakly qualified | Exclusive waitpid, preprepared descriptors, explicitly research-only memory promise. |

No predicate family, runtime hold token, acceptance budget or recursive callable
inspection is added. The checker retains ordinary finite provenance alternatives
and compares protected regions against the existing complete atomic footprint.
Generic bodies use symbolic groups; concrete callers discharge the resulting
boundary. A joining local-reference alternative cannot inherit a held origin.

## Descriptor order and native integration

All descriptors and bookkeeping are prepared before the hold. The source job
exclusively owns its child identity and uses `waitpid`; no pidfd is opened after
creation. With no competing reaper, the child's identity cannot be reused before
its owner reaps it. Prepared data/status pipes supply collection progress and
EOF; a notification may be activated after release without creating a descriptor.
This loses no source functionality: the job exposes neither process identity nor
independent process monitoring. Host wait failure never discharges ownership.

The native capsule witness still has a separately scoped pidfd path and its
convenience finish waits before reading status. Source integration must drain or
discard a large data pipe while awaiting exit, then reconcile framing/status and
confirm reaping. Otherwise a full pipe deadlocks. It also needs confirmed abort.
Signal arm, fork and restoration must stay on one native thread, while linear
source jobs may move between contexts. Phase 2 must qualify that integration;
the existing native header is not already its implementation.

## Memory promise card

**What memory promise should the qualified fork research route make?**

**Background.** The owner chose service first: reserve exhaustion aborts the
snapshot, preserving the old base and log with bounded cleanup; pause and latency
targets belong to the user. Fork is one research backend candidate, not a source
abstraction. The constitution requires ordinary contracts (quoted above); merely
calling a value `Reserve` cannot promise resources it does not account for. The
minimal memory witness is a retained child image while the parent overwrites
all captured writable pages: originals remain live and parent copies grow,
although encoder scratch and output stay small. The current heap meter omits
kernel COW, other mappings and child retention. WAIT-2 bounds no host latency.

**Options.**

- **A — Research-only bounded workspace/output (recommended).** Promise the
  explicit scratch, control, pipe-payload and emitted-byte bounds of SNAP-3,
  encoder abort on exhaustion and confirmed caller abort/reap. Measure COW,
  aggregate RSS, pause and cleanup time, without a hard bound. Defer production
  eligibility under the owner's total-memory service-first policy. Honest and
  immediately specifiable; it does **not** promise automatic COW-budget abort,
  physical reserve, or elapsed cleanup/pause bounds. The application publishes
  nothing on export failure, so its previous durable base/log remain its own
  responsibility. This draft claims no production qualification.
- **B — Conservative worst-case image reservation.** Require an enforced
  serving-parent budget P and a trusted page-rounded upper bound M on every
  inherited private mapping, including the writable subset (dataset, allocator reserves/metadata,
  stacks, runtime and linked native mappings), plus a kernel/page-table allowance
  K and explicit prepared resources. Admit capture only with separate headroom
  for the retained image and possible duplication of every writable page, conservatively up to M
  additional bytes while parent growth is bounded by P. Such a promise needs a
  mapping/admission meter and a host physical-headroom enforcement contract;
  heap-in-use and sampled resident bytes cannot supply it. Linux virtual commit
  accounting alone is not a physical RAM reservation. Cost approaches a second
  complete process image, with conservative admission refusal and new accounting
  obligations; the exact K and enforcement domain remain to be qualified. A
  reservation still supplies no wall-clock cleanup bound without host guarantees.
  This option is a proposal with prerequisites, not an available runtime feature.
- **C — Observe growth and abort at a threshold.** Sample private/retained pages
  and kill the child before a threshold. A hard promise would require a maximum
  growth rate R, maximum observation-plus-cleanup delay L and granularity margin
  Q, reserving at least R*L+Q beyond the threshold. No such R/L bound exists here;
  measurements can miss an overshoot. Child-only limits do not automatically
  protect the parent that faults copied pages. Useful research policy, not the
  owner's hard service-first reserve without additional host qualification.

**Confidence 5/5 for A's stated limits; 3/5 for eventual production policy.**
A qualified mapping/physical admission mechanism, enforceable parent budget and
host delay guarantees could make B or C viable. Measurements alone cannot
establish their worst-case promises. Fixed finite cleanup work differs from
bounded elapsed termination/reaping under arbitrary host delay.

## Draft judgments for owner review

These are proposed choices, not additional owner rulings. Exact constants,
tags and declarations are owned by the specification draft.

| Draft choice | Alternative | Ground and limit |
| --- | --- | --- |
| `std::snapshot` module and one value-type parameter | Put functions in `std::process`; add key or encoder parameters | Dataset capture is the abstraction, while the existing concurrent map fixes binary keys and the child encoder is supplied. |
| Ordinary held contract and reference cut | Operation-specific direct call or scalar history | General source helpers and linked exporters share the same boundary and forwarding; protected regions stay unchanged. |
| Transitive owned type-graph constraint, including private fields and every variant | Check only the current live variant or public fields | The type contract must remain valid after mutation and through module opacity; an unselected resource-bearing variant remains inadmissible. |
| Logical pairs for shipped HashMap and OrderedMap, logical sequence order for Ring and Slots | Encode vacant/deleted buckets, capacities and tree nodes | Output consumers need values independent of container storage history; traversal can inspect captured structure without user callbacks. |
| Typed structure for other source structs and enums | Invoke user serialization callbacks | All owned fields can be encoded without admitting user code into the child closure. |
| Exact concrete type graph as an external schema; include private owned fields | Carry a self-describing schema; export only public fields | No whole-schema buffer is needed and all owned state is covered, but the consumer must know the exact declaration graph and field privacy is not an export confidentiality boundary. |
| Refuse fieldless opaque types uniformly, and all Shared, SharedRead and Frozen handles | Whitelist host origins or follow immutable shared referents | A finite owned graph supplies the traversal boundary; frozen retention and fieldless opaque representations need a separate semantics before admission. |
| Permit several captures in one atomic statement protecting their named regions | Limit a statement to one capture | Each independent reserve has its own attempt and job; unchanged protected state gives every successful image the same cut without an arbitrary count restriction. |
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

`compiler/build.rs` already generates grammar tables from the active specification;
no generated table is checked in. Phase 2 must teach the ordinary syntax and
semantic consumers the new production, under `compiler/src/syntax/` and
`compiler/src/semantic/`. Phase 1 migrates active source fixture identifiers;
it intentionally implements no acceptance path, so these cases are specification
expectations rather than a claim that the current compiler accepts them.

| Work | Files and evidence |
| --- | --- |
| Register and carry host declarations | New `lib/std/snapshot/module.wfm`, `lib/std/modules.wfg`, `compiler/src/library.rs`; resolution/module tests in `compiler/src/resolution/tests.rs`. Preserve published declaration ordinals. |
| Parse and check general held contracts and resource-free closure | `compiler/src/syntax/`, `compiler/src/semantic/check/behavior/contracts.rs`, `compiler/src/semantic/check/references.rs`, `compiler/src/semantic/check/control/atomic.rs`, `compiler/src/semantic/check/expressions/calls/user.rs` and type checking; semantic tests plus phase-1 conformance cases. Update ordinary contract representation and generated grammar consumers; check symbolic origins, function refinement and full-block protected footprints. Follow aliases, private fields, every enum alternative and finite recursive type graphs, including function-actual selection. |
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
