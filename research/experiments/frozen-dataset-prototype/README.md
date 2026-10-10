# Persistent dataset prototype: reserve ownership boundary

## Question and pre-registration

Can an opt-in persistent dataset library meet contracts B1–B5 and B8 of the
[snapshot investigation](../../investigations/consistent-snapshots/README.md#contracts-required-before-implementation)
under current ownership rules, including exact snapshot-only retention
accounting, automatic last-handle reclamation, and service-first reserve abort?

Written before any run, against main
`fc98b8f1aee54a2d57c55dd25b099f2819320d3f`, active specification v0.121.
Nothing here has been compiled or executed. This is a stopped expressibility
probe, **not an implemented HAMT or a passing library prototype**. The first
unresolved step is releasing a stalled reader's retained root on reserve
abort, together with observing ordinary capture drop in the dataset ledger.
The task explicitly requires stopping at an ownership obstruction. The
programs below isolate that step before constructing the map around it.

The owner's service-first reserve and opt-in library rulings remain in force.
A new storage domain is an option for the owner to consider, not a conclusion
that these probes select. In particular, rejection of a destructive write
through SharedRead proves that operation unavailable; it does not prove that
every possible managed snapshot API is inexpressible.

The existing [expressibility witness](https://github.com/Ming-Research/Whitefoot/blob/2bb8a342539930f04ed661bcf1c677ed5bb7eb07/research/experiments/frozen-dataset-witness/README.md#results)
reports update, capture, enumeration and reclamation working in its CI runs.
Its 8,192-byte whole-process residue was isolated to a retained runtime frame
chunk, not retained dataset nodes. Those are prior results, not runs of this
prototype. Its binary tree and scalar values do not establish the requested
HAMT, byte-array values or reserve protocol.

The intended full-program observations remain fixed:

| Program, not yet implemented | Expected outcome | Contracts |
| --- | --- | --- |
| Sequential | Print a deterministic PRNG seed; replay set, delete and multi-key batch histories into an independent sorted association list; every captured enumeration equals the list at its generation. Include empty keys/values, replacements, absent deletes, hash collisions and multiple captures. Exit 0. | B1, B2, B3, B5 |
| Concurrent | Writer publishes whole batches while the reader captures at racing points and waits between enumeration steps; replay the same history independently through the returned generation. No partial batch or mixed generation; writer completes. Exit 0. | B3, B4, B5 |
| Reclamation | Multiple captures share immutable nodes; after their last handles and the dataset drop, quiescent heap readings in one activation return to baseline without deepening the waiting frame stack between readings. Exit 0. | B1, B8 |
| Reserve | Stall a small-reserve capture while the writer replaces every key. Before excess retention is admitted the capture aborts, publication does not depend on reader progress, and actual retained bytes reach zero. Exit 0. | B1, B4, B8 |

An inexpressible required operation, any oracle mismatch, an abort that waits
for the stalled reader, or retention surviving the promised cleanup boundary
rejects the proposed library contract. A compiler implementation failure,
timeout or unrelated diagnostic establishes no language rejection. No expected
result may be weakened to obtain a green run. The full harnesses have no exit
codes assigned yet because they have not been written; the executable probes'
distinct codes are below.

Performance, fork, scan-plus-log, file export and durability are out of scope.
The timing comparison belongs later on the idle i9-14900K through CI, under
the investigation's pre-registered comparison and consumer-set targets.

## Intended representation and accounting

The requested map would use a fixed-fan-out-16 hash array mapped trie with
path copying, immutable shared child edges and collision storage for distinct
keys with equal complete hashes. Each leaf owns its key and value as
`Box<Array<u8>>`. A node is constructed uniquely, moved into `Shared`, retained
as `SharedRead`, then loses its only writable handle. Unchanged descendants
are retained, never copied as unique owners. Nested mutable values, outside
writers and host resources are outside this dataset; capture copies nothing
mutable. A single publication cell holds the complete root and generation;
batch construction is private and only its final root is published. Neither
hash choice nor collision layout nor generation-exhaustion behavior has been
implemented or selected by this stopped probe.

The exact quantity needed is

`V = sum(bytes(a), a in (union of capture-reachable allocations) minus live-root-reachable allocations)`.

This is a set of allocation identities, not a sum of generation sizes. Nodes
shared by two captures count once, and a node also reachable from the live
root counts zero. Keys, values, node backing and shared-state allocation
overhead belong in those allocation sizes; capture metadata and traversal
scratch belong in the separate buffer/metadata budget. Logical payload bytes
alone are insufficient. Process-wide PRE-2 heap samples cannot compute this
set in concurrent service. This probe chooses no unverified physical-byte
formula and implements no counter pretending to be V.

At publication, the proposed ledger would determine which live allocations
become capture-only and admit the resulting V only within the reserve. A
last-capture drop must remove its otherwise unreachable allocations from V.
An abort must both communicate `aborted` and retire the capture's retention;
setting a counter to zero while another handle keeps the nodes alive is not
accounting or cleanup. The following ownership alternatives prevent finishing
that protocol as requested.

## Minimal boundary and alternatives

1. **Return an independently retained root.** SHARE-1 releases only the
   particular handle dropped. Dropping the writer's registry reference cannot
   release the reader's handle or its nodes. A stalled reader can hold an
   arbitrary old subtree. Setting an abort flag does not consume that handle.
   With reserve zero, one replaced value is already a counterexample: the old
   allocation becomes capture-only even after the writer drops all its handles.
2. **Keep the root exclusively in a shared capture controller.** A writer can
   clear that controller, provided no node/root handle escaped. But if the
   writer retains the controller in its registry, dropping the reader's last
   controller handle leaves the registry handle and root live. Automatic
   capture drop does not unregister it. STOR-3 provides no source finalizer;
   the PRE-1 handle interface provides neither weak handles nor a strong-count
   observation. Holding a `Shared` ledger inside the capture only releases
   that ledger handle on drop; it does not write its counter.
3. **Managed cursor and explicit close.** This is a plausible different API:
   keep all retained owners under writer-controlled state, return detached
   bytes between waits, and require explicit close/unregister. It needs its
   own analysis of traversal pins, multi-reader accounting and bounded scratch.
   It changes automatic last-handle cleanup and the independently retained
   root interface; it has not been silently substituted. A linear handle can
   enforce close, but cannot attach work to ordinary drop.
4. **Cooperative cancellation.** The reader can notice an abort and drop its
   own handles. That does not bound cleanup while the reader waits indefinitely
   on unrelated work; blocking publication until it acknowledges instead
   changes the service-first policy. PRE-2 explicitly distinguishes cancellation
   request from completed cleanup and gives file operations no cancel bound.

The minimal forbidden mutation is in [revoke-rejected.wf](revoke-rejected.wf):

```wf
fn revoke(root: &SharedRead<Box<Array<u8>>>) -> result: unit reads(root) waits {
  let empty = box_array_filled::<u8>(count: 0_u64, value: 0_u8);
  atomic bytes = &root^ {
    set bytes^ = move empty;
  }
  return unit;
}
```

SHARE-2 rejects whole-state replacement through a read-only target. Changing
that target to writable would mutate the captured generation and violate B2/B3;
it would still not remotely consume the reader's handle. There is no finalizer
syntax to offer as a second validly formed program: STOR-3 expressly excludes
attaching any user-defined action to release. [control.wf](control.wf) instead
exhibits the permitted drop behavior, including a retained ledger unchanged
by dropping its ticket.

**Status:** normative rejection expected, diagnostic not yet observed. CI must
establish canonical acceptance of the control and SHARE-2 attribution for the
negative at `set bytes^ = move empty;`. Earlier FORM/GRAM/type rejection,
internal errors and resource stops are not the intended evidence. These
sources are research evidence, not new conformance verdicts.

## Executable probes and failure codes

`control.wf` reduces the storage to one immutable one-byte value with literal
oracle 7. It checks three independent consequences of the current rules:

- Dropping the writable handle leaves the captured value and heap allocation
  intact; dropping the last read handle releases it.
- Dropping the reader's controller handle leaves the registry-owned root
  intact; clearing the registry root releases its allocation.
- Dropping a ticket releases its root but leaves the separately retained
  ledger's outstanding count at 1, demonstrating that there is no drop action.

Each heap comparison brackets allocations and releases inside one function,
without recursion, spawn or deeper waiting calls between its readings. It
does not subtract a magic frame-chunk constant or reinterpret a discrepancy
as a pass. PRE-2's quiescent heap count is the oracle for release, not RSS.

| Program exit | Meaning |
| --- | --- |
| control: 0 | All three specified ownership consequences observed; this is **not** success of the requested reserve contract. |
| control: 1 | Dropping the writer unexpectedly changed heap while a read handle remained. |
| control: 2 | Retained root had the wrong length. |
| control: 3 | Retained byte differed from the independent literal 7. |
| control: 4 | Last root drop did not return to the same-function baseline. |
| control: 5 | Dropping the reader controller changed the registry-held heap. |
| control: 6 | Registry lost its root when only the reader handle dropped. |
| control: 7 | Clearing the registry did not release any storage. |
| control: 8 | Dropping the registry did not return to baseline. |
| control: 9 | Ticket drop changed the separately held ledger's count. |
| control: 10 | Ticket drop did not return to the ledger-only heap level. |
| control: 11 | Dropping the ledger did not return to baseline. |

The negative is compiled only, never executed. Compiler exit statuses are
reported separately from program statuses. [run.sh](run.sh) records all raw
statuses and diagnostics, and deliberately exits **80** after a successful
control because the requested library route remains unresolved; a failing
control instead returns its actual failure. It does not classify an arbitrary
compiler failure as a successful rejection. A human must inspect the negative
diagnostic at the specified operation. Unexpected negative acceptance also
leaves the workflow failed. GNU timeout status 124 and signal termination are
infrastructure failures, never source verdicts.

The [temporary workflow](../../../.github/workflows/frozen-dataset-prototype.yml)
runs only on pushes to `claude/snap-lib-proto`, using ubuntu-24.04 and
`make -C compiler build`; it records revision, specification digest and host,
then compiles/runs these smallest probes. Remove it before any pull request.
No canonical gate consumes this research. No local build, compilation, test,
script run or commit is authorized or performed by this change.

After CI builds `compiler/target/gate/whitefootc`, the explicit invocation is:

```sh
sh research/experiments/frozen-dataset-prototype/run.sh
```

It needs GNU `timeout`. Logs and `results.tsv` go under `$OUT`, defaulting to
`$RUNNER_TEMP/frozen-dataset-prototype` or `/tmp/frozen-dataset-prototype`.

## Contract disposition and handoff

| Contract | Disposition at the stop |
| --- | --- |
| B1: frozen ownership/reclamation | SHARE-1 construction and last-handle reclamation have prior witness evidence. New minimal control pending CI. Exact automatic capture-ledger retirement is unresolved. No HAMT implementation. |
| B2: transitive closure | Intended payload closure is owned byte arrays plus immutable shared edges, with no outside writer. Minimal control follows it. Nested mutable values are excluded explicitly. |
| B3: complete generation/cut | Intended one-cell root/generation publication and private batch construction; not implemented here. Destructive read-handle revocation would violate the captured generation. |
| B4: physical sharing | Shared ownership and atomic state transitions use SHARE-1/3. No concurrent accounting or HAMT publication evidence yet. |
| B5: proof facts | Every probe array access proves its bound in the same atomic read. No pre-capture length fact is imported. Full capture contracts remain unwritten. |
| B6: host-resource disposition | No export job; captures contain no host resources. The control consumes both Inputs directory handles normally. |
| B7: fork frontier | Out of scope; no fork. |
| B8: cancellation/cleanup | Blocking requirement: an independently retained root survives writer abort; a registry retains a dropped holder's root. No bounded cleanup guarantee or wall-clock bound is claimed. |
| B9: expressibility boundary | Two complete minimal programs isolate the boundary; negative attribution and positive behavior await CI. This is narrower than rejecting all persistent libraries. |

No specification, conformance case, compiler, standard library, design decision
or `docs/todo.md` is changed. No API workaround or new storage domain has been
adopted. The HAMT module, module graph and four full correctness programs are
intentionally absent at this explicit stop, rather than placeholders that
could be mistaken for a completed prototype.

Found along the way: the requested automatic-drop ledger and forced-abort
requirements meet a handle-lifetime boundary not tested by the earlier
witness; isolated here. The frame-chunk residue remains with its existing
runtime owner, without a duplicate runtime change.

## Read-only review

A separate GPT-6 agent reviewed the complete five-file working change against
the recorded main revision, the requested outcome and constraints, relevant
specification and library interfaces, the earlier witness, and the governing
design nodes and ancestors. It also inspected the negative's final `freeze`
helper, which drops the writable handle before the attempted revocation.
No actionable findings remained within that scope. The review ran no builds,
compilations, tests or scripts.

Repository/citation checks and construction/research-boundary checks passed
by inspection, as did design consistency and correspondence for the probes.
Specification delivery, changed compiler acceptance, canonical-gate selection,
new timing labels and deleted implementation checks were not applicable.
Actual diagnostic attribution, control behavior, heap equalities and DC4's
full implementation evidence remain unverified. The review supports the
handle-local lifetime argument, not impossibility of every managed API; that
broader design claim remains unverified. This is a blocked edit-only handoff,
not completion or approval of the requested HAMT prototype.
