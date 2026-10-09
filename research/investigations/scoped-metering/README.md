# Scoped memory metering

## Question and prior criterion

Investigation before implementation, based on Whitefoot `ef86e3ad943af8e780608c86393ca83c48d324ba`.
The owner selected context scopes on status-board card `firn-aofrw-meter-card`,
option A: a context can open a scope, its allocations and those of contexts
it spawns are counted there, and other contexts can observe that count.
The surface, attribution, lifetime and implementation below are proposals,
not additional owner rulings or implemented capabilities.

Can this give firn an accurate exclusion for its private AOF rewrite without
materially taxing allocation or weak embedded CPUs? Compare the current
process-only meter with scoped accounting, both unused and active, and compare
firn admission with and without the rewrite exclusion on the same workload.
The prospective rejection criteria are:

- Any quiescent mismatch against independently counted live storage, including
  transferred owners, or persistent rewrite bytes after its storage is freed.
- A resolved slowdown above **1%** on any selected compute-regression or firn
  cost workload, or more than **1 ns per allocation/free accounting event**
  added in the allocation-heavy experiment. These are proposed acceptance
  bounds, not measurements; noise spanning a bound leaves it unresolved.
- With a rewrite actually overlapping evict-zipf, any refused write,
  rewrite-induced key collapse, or sampled admission-memory overshoot above
  **0.2% of maxmemory**. This proposes a numerical meaning for “near zero”;
  it does not bound process RSS or an unsampled instantaneous peak.

An accounting failure rejects the semantics/implementation, even if fast.
A cost failure reopens the implementation; it does not authorize weakening
the promised count. The constitution requires project-specific performance
criteria and includes small embedded systems
(`docs/constitution.md:16`, `docs/constitution.md:28`).

## Current contract and motivating evidence

PRE-2 counts emitted allocation requests, live runtime-pool grants and the
host descriptor registry. Allocator reserves, rounding of emitted requests,
stacks and executable mappings are excluded. `heap_in_use` is exact at
quiescence; concurrent error is bounded by bytes moved during its reading.
`resident_bytes` reports optional process RSS. Both observations write their
meter and are execution inputs (`spec/kernel-spec.md:2559`, declarations at
`:2956` and `:2980`). **Total stays total**: opening or closing a scope must
not remove anything from `heap_in_use`; RSS has no scoped counterpart here.

The existing [memory-statistics investigation](../memory-statistics/README.md)
records the counter argument at lines 139–161 and the map allocation inventory
at lines 234–264. Its earlier firn cost comparison did not resolve its 1%
bound, and ordinary SET/MSET barely exercised emitted heap allocation
(`research/investigations/memory-statistics/README.md:123`); it is no cost
evidence for this proposal. Relevant settled grounds are
`design/language/system-interface/memory-statistics.md:1`, `:5`, `:7`,
the ordinary host-interface and explicit-close rules in
`design/language/system-interface.md:1`, `:5`, and the context/compute runtime
choices in `design/compiler/waiting-contexts.md:1` and
`design/compiler/parallel-lowering/parallel-runtime.md:1`.

Firn-wf [PR #35, maxmemory](https://github.com/Ming-Research/Firn-wf/pull/35)
needs to exclude private replay keyspaces. Its
[memory-limit results](https://github.com/Ming-Research/Firn-wf/blob/6acdcac6a830497f7c0dc5c6380c3189450722ff/research/investigations/memory-limit/README.md#results-eviction-quality-and-at-limit-behaviour)
(`research/investigations/memory-limit/README.md:336–383` in Firn-wf) record
i9-14900K runs [37996029478, AOF probe](https://github.com/Ming-Research/Firn-wf/actions/runs/37996029478)
and [37998440715, eviction comparison](https://github.com/Ming-Research/Firn-wf/actions/runs/37998440715).
The latter refused no writes but recorded two-CPU AOF overshoot of 8–20% and
one keyspace fall from 423,241 to 120,031 keys during rewrite. The probe
refused 35,244 and 38,549 writes. The record attributes this to firn's
in-process replay storage entering its admission count; Redis 7.0.15 rewrites
in a fork child whose allocations the parent's counter never sees. These
are baseline observations, not results of scoped metering.

## Language surface and attribution

### Surface alternatives

All candidates add a distinct `MeterScope` opened from `MemoryMeter`, sharing
and observation, and the lifetime rules below. None changes the meaning of
an existing `MemoryMeter` handle.

| Candidate | Semantics and affected rules | Assessment |
| --- | --- | --- |
| **A. Ordinary generic scoped runner (recommended).** `scope_open`, `scope_share`, `scope_bytes`, `scope_run`, `scope_close` in `std::process`. | `scope_run` owns a scope handle and an argument record, invokes a statically supplied waiting callee under that scope, then returns the handle and callee result; entry refusal returns the handle and unconsumed arguments. It can be called or spawned. PRE-2 specifies the operations and WAIT-2 the inherited attribution; FN-2/FN-3 supply the static callee, OWN-1 the ownership, EFF-1/EFF-5 the footprints, WAIT-1/WAIT-3 the waiting call and join. | One invocation shape composes with structured spawn and ordinary monomorphization. The value-argument record is a real cost: this form does not wrap arbitrary borrowed-parameter calls. Helpers inside the runner inherit its scope normally. |
| B. A scope operand on `spawn`, optionally also on ordinary calls. | Capture the scope when starting the context; its lifetime extends through completion. Amend GRAM-5, FN-1 and WAIT-3, plus the common rules above; a synchronous variant also needs specified restoration on return. | Not selected: spawn-only cannot scope a direct call, while adding both forms creates another call decoration for what A expresses with an ordinary function boundary. |
| C. A scope-carrying meter parameter automatically selects allocations of a callee/context. | Passing or receiving the handle changes the ambient allocation scope; PRE-2, FN-1, EFF and WAIT rules must define which handle wins when several are passed, stored or forwarded. | Rejected: possession for observation would also select attribution, and the callable signature does not identify the entry/exit boundary. Keeping `heap_in_use` total makes a scope parameter a separate role anyway. |
| D. An explicit setter returning the previous scope. | Changes the running context until a later restore; PRE-2 and WAIT-2 must account for that context state and EFF for its access. | Rejected: every early return and nested call must restore state, and overlapping compute must capture the right value. A delimited call gives a single boundary for that obligation. |

A's names and signature shapes are sketches, not accepted WF examples.
Function-kind arguments already are compile-time signatures, not closures
(`spec/kernel-spec.md:1321`, `:1332`); no new first-class callable is proposed.
The runner's callback takes owned arguments and can mutate them or use shared
objects, with the ordinary empty formal-reference row. The runner likewise
has an empty row and `waits`: its scope and arguments are consumed values,
not reference effect roots (`spec/kernel-spec.md:1561`). Its ordinary result
retains the handle on success; entry refusal returns both the handle and the
unconsumed argument record, which may contain linear owners. This needs a
concrete interface formation check before a specification amendment, not an
assumed variadic or effect-polymorphic wrapper.

Recommended operation boundaries:

- `scope_open(meter: &MemoryMeter)` writes the meter and returns an ordinary
  `Result<MeterScope, MeterError>`; finite accounting-slot exhaustion is a
  typed refusal. Acquiring an accounting slot does not acquire a second heap.
- `scope_share(scope: &MeterScope)` writes the scope's handle state and returns
  another handle to the same scope. `scope_bytes(scope: &MeterScope) -> u64`
  writes its handle, just as `heap_in_use` writes its meter. Sharing permits
  observations in another context; aliases confer no global snapshot order.
- `MeterScope` is opaque `nodrop`. Explicit consuming
  `scope_close(meter: &MemoryMeter, scope: MeterScope)` writes the meter;
  a successful close releases that handle. The last handle closes the scope
  only when no active invocation, descendant scope or attributed live storage
  remains; otherwise the result returns the handle as busy. The source can
  retry after completion/release. All source exits must retain or consume
  their handles. Dropping a handle must not silently return slot capacity:
  current opaque host drop is empty (`spec/kernel-spec.md:875`).

Outstanding bytes therefore keep their attribution, remain readable and
remain in the process total. Closing never frees program storage, zeros a
live account, or transfers its bytes to the default scope. Last-close
eligibility must use synchronized lifecycle state, not a sampled zero.
Reject immediate slot reuse and implicit finalizer close for those reasons;
permanent never-reused scopes would exhaust a small target under repeated
rewrites. Returned storage can outlive `scope_run`; its returned handle lets
the caller observe it and close after it is freed.

Recommend **inclusive nesting**: a scope opened while S runs is a child of S;
its bytes count in it and every ancestor. Opening at the default context
creates a top-level scope. A runner can enter that scope from its recorded
parent or re-enter the same scope; entering from an unrelated scope returns
a typed refusal with the handle and arguments. This keeps fixed ancestry and
preserves the owner's descendant-context guarantee. Innermost-only readings
simplify disjoint subtraction but would omit a child's explicit subscopes from the
parent's reading; that narrowing is not selected. Do not add together
overlapping ancestor/child readings for admission.

Ordinary calls and spawns inherit the current scope. The runner restores its
caller's scope after callback cleanup and its structured joins. Arguments
already allocated by the caller keep their old attribution; future
allocations inside the callback use the selected scope. For A, the runner's
context/frame storage allocated before entry also keeps the caller's origin;
selection precedes callback-frame allocation and execution, so subsequent
child contexts belong to the selected scope. B could instead select attribution
at spawn creation. WAIT-3 evaluates arguments in the starter and fixes the join
boundary (`spec/kernel-spec.md:2255–2264`); neither changes. A does not require
a spawn path that recognizes the runner's name: ordinary calls do not inspect
linked definitions (`spec/kernel-spec.md:2552`, `design/compiler.md:13`).

Scope reads are WAIT-2 execution inputs, like PRE-2's process reading; they
never determine acceptance. Allocate/free still carry no effect path
(STOR-8 at `spec/kernel-spec.md:844–849`, PAR-1 at `:2149–2158`). Scope
selection changes attribution, not PAR permission or scheduling dependencies.
Each overlapped task must retain its logical scope through stealing and
helping; a host thread's current context is insufficient. Ordered reads of a
handle do not make allocation and reading atomic. Specify the same
quiescent-exact/concurrent-bounded observation contract for scoped bytes,
with the bound covering operations in that scope's subtree.

### Ownership transfer is a semantic decision

Gran's proposed free-debits-current rule is cheaper but not exact live-byte
attribution. Minimal witness, with all operations complete:

```text
scope A allocates an 8-byte block; ownership moves to B; B frees it.
process live bytes = 0; current-scope ledger: A = +8, B = -8.
```

Clamping B to zero leaves A overstated. Reversing the transfer can understate
the rewrite's scope. Repeated transfers make the error unbounded by current
live bytes; modulo counters and the process meter's concurrency bound do not
repair it. Resize in another scope has the same problem for old/new extents.

Recommend **exact allocation-origin attribution**: moving an owner changes no
charge; freeing or resizing charges the recorded origin. Explicit nested
scopes preserve the ancestor charge. Compare this against a precisely named
net-activity ledger (credits and debits in the acting scope), which could be
exact for a closed allocation/free workload but would need an explicitly
approximate live-byte contract under transfer. Reject that contract as the
default for safe subtraction: over-attribution excludes unrelated live
memory. The owner's scope ruling alone does not settle this choice. An exact
origin tag or equivalent ownership metadata is new work with unmeasured cost;
gran's sub-nanosecond estimate does not cover it.

## Runtime candidate and open questions

Current emitted wrappers pass sizes on allocation, resize and free
(`compiler/src/backend/heap.c:7–29`). After first registration,
`wf__heap_change` performs a thread-local add and relaxed publication to a
cache-line-private slot; readings sum slots and the separately locked pool
counter (`compiler/src/backend/completion/bridge.c:1341–1388`). Pool grants
and returns update under its existing lock (`:1088–1093`, `:1145–1152`).
Direct map accounting calls the same change function; pool-backed map storage
uses runtime take/give (`compiler/src/backend/keyed_table.c:26–34`).

Gran's source-based, **unmeasured** estimate extends this to one single-writer
slot per thread per scope, selecting it with a TLS current-scope ID. The
estimated extra allocation-path cost is under 1 ns; free uses its supplied
size and the current ID, with no block header. Drivers install the ID whenever
they resume a context; compute task frames carry it, saving/restoring it even
when a joining worker helps an unrelated task. A map retains its creating
scope and charges its storage changes there. This is the candidate to measure,
not evidence that exact attribution meets the estimate.

Resolve these details before implementing:

- **IDs and lifecycle:** compare 16 and 64 slots including the default.
  Recommend target-declared finite capacity and recoverable open refusal,
  never aliasing or silently using the default. Retire an ID only after its
  handles, activity, descendant dependencies and storage are gone. Define
  counter retirement without clearing slots that their writer can still use.
- **Slots and reads:** keep thread rows on separate cache lines, not every
  scope slot on a separate line. Exclusive origin slots permit one update;
  inclusive reads sum descendants. A leaf reading costs O(T), a subtree or
  process sum O(TS), for T registered threads and S slots. Compare maintaining
  the old process counter (another publication per event) with summing all
  slots, preserving total accuracy. Measure reads at admission frequency,
  sparse/dense scope occupancy, and after thread exit. Never sum clamped
  per-thread balances; use the existing modulo argument on the aggregate.
- **Exact origin:** compare a block tag with metadata already carried by
  owning storage/runtime records. Cover resize, owner exchange, returned
  values, delayed cleanup and transfers via shared objects. No program-specific
  fast path or assumption that a thread always frees its own allocations.
- **Map attribution:** map-owned tables, nodes, metadata and retained spare
  storage use their creating scope even when another context mutates or
  reclaims them. Separately allocated value payloads keep their own origins;
  moving a payload into a map does not recursively relabel it. A single map ID
  is insufficient when two maps exchange backing storage: retain that
  storage's origin through swap/clear/drain, or propose an explicit transfer
  rule before implementation. Existing swap exchanges backing storage while
  retaining per-map control state
  (`design/compiler/waiting-contexts/concurrent-map.md:21`).
- **Pool split and shared infrastructure:** add per-scope granted-byte
  counters under the existing pool lock, without also charging the direct
  path for those blocks. Charge scope-owned context storage consistently at
  take/give even when the driver performs cleanup. Keep process-wide driver,
  timer-capacity and descriptor-registry storage in the default account;
  audit this classification against PRE-2 rather than charging whichever
  rewrite first caused shared infrastructure to grow. Accounting machinery
  itself is process/default overhead, not rewrite storage.

## Firn worked example and evidence owed

The admission context opens a rewrite scope, retains its observing handle,
and gives a shared handle to a spawned scoped runner. The runner calls the
rewrite; the rewrite's spawned replay worker inherits the same scope, as do
its compute tasks. Both private replay keyspaces must be created inside it.
After workers join and replay storage is released, the runner returns its
handle for explicit close; admission observes the empty account before the
last close. A returned live buffer remains charged until actually freed.
Origin accounting does not certify privacy: audit that no replay allocation
becomes live client keyspace state while still excluded. Such a transfer
preserves its charge correctly but would invalidate firn's exclusion policy.

Admission uses `max(0, total_heap - rewrite_bytes - other_excluded_bytes)`.
The exclusions must be disjoint: AOF buffers charged to the rewrite cannot
also appear in the existing buffer exclusion. Reads of total and scope are
separate samples; guarded/saturating arithmetic prevents underflow but does
not establish snapshot accuracy. Keep raw total, raw scope, adjusted admission
bytes and RSS separately visible in the experiment. A large process total
during rewrite is expected and still matters for host capacity.

Repeat evict-zipf with AOF, 1,000,000 keys, Zipf 0.99, 64-byte values,
allkeys-lru and the same half-dataset limit, on one and two server CPUs of the
i9-14900K. Require timestamped evidence that rewrite and its worker overlap
the measured traffic, including a deliberately triggered rewrite. Compare
the same scoped build with exclusion enabled/disabled, plus no-AOF and Redis
7.0.15 controls. Record accepted/refused writes, live key count, hit rate,
latency, throughput and 10-ms memory samples. Success needs no refused
writes, no rewrite-correlated collapse beyond matched no-rewrite variation,
and adjusted overshoot within the stated bound. Raw heap may exceed maxmemory
by the excluded rewrite; calling that value adjusted overshoot would test
the wrong quantity. Independently count requested/granted bytes in a small
replay fixture and require exact quiescent deltas and return to baseline;
two counters agreeing with each other is not that oracle.

## Embedded targets and measurements before acceptance

A single-core target with cooperative contexts and no worker threads uses
one slot array, one current ID and ordinary integer loads/stores: no atomics
or cache-line padding are needed when no interrupt accesses the counters.
Context switching still saves/restores attribution. With S slots including
default, 64-bit counters cost **8S bytes: 128 bytes for 16, 512 for 64**.
Pool changes can update the same array there; a separate pool array would
double that counter storage. Scope descriptors, ancestry, handle/activity
state, IDs in task/context/map records, and origin tags are additional costs,
not included in those figures. A 32-bit CPU needs multiword arithmetic, but
no concurrent observer can see a torn read under these conditions. Interrupt
allocation would require a separate synchronization design.

On threaded targets, published counters cost 8TS bytes before row alignment;
a separate TLS mirror adds another 8TS and pool counters another 8S. Exact
origin needs at least enough bits to distinguish S slots (4 or 6), but actual
tag padding or side metadata can cost much more per allocation. Determine
that layout and its peak RAM cost before claiming suitability for embedded
CPUs; the i9 estimate establishes neither their latency nor their RAM budget.
Keep accounting/observation allocator-free when emitted heap is absent,
as required by `design/language/system-interface/memory-statistics.md:7`.

All following execution is future work through CI; none ran for this record.

1. On the idle i9-14900K runner, first time a small useful sample and inspect
   spread. Interleave base, independently built same-source base twin,
   candidate and candidate twin, recording source/runtime revisions, compiler,
   flags, affinity, allocator, scope/thread counts and workload seeds. Failed
   identical-image controls or noise covering a criterion mean inconclusive,
   not a widened threshold. Keep build time separate from execution.
2. Run the maintained compute-regression panel with its null and sensitivity
   controls (`.github/workflows/compute-regression.yml:173–198`), and repeat
   its paired workloads on the i9-14900K for the proposed 1% bound. Compare
   unchanged workload bodies across runtimes with scoped accounting absent,
   present but unused, and active. Matched harnesses select the old entry or
   proposed runner; the old compiler is not assumed to understand the new API.
   Keep entry-wrapper cost separate from steady-state allocation cost.
3. Add an allocation-heavy experiment: boxes, resize, varied sizes, pool and
   map nodes, same-scope and cross-scope frees, one and several threads;
   observe results so allocation cannot be removed. Separate the steady-state
   event cost from open/close and from scope-reading frequency. Compare exact
   origin and current-scope candidates on identical allocation traces; use
   same-source twins and interleaved runs. Deliberately move/free one block in
   another scope as the falsifier for the cheaper attribution claim.
4. Run the firn comparison above, including allocation-heavy script traffic
   for cost: previous SET/MSET evidence is insufficient. Add correctness
   observations for nested scopes, sequential and stolen compute, suspension,
   frame cleanup, capacity refusal, busy close, reuse, and cross-scope map
   swap/drain. Expected bytes come from the selected contract and an
   independent allocation inventory. Before accepting an embedded claim,
   qualify one named weak target's storage layout and allocation/read costs
   through CI; selecting that target and its concrete RAM/time budget remains
   open. No desktop result substitutes for it.

## Decisions for the owner

1. **Which invocation surface?** Recommend A, an ordinary generic waiting
   runner with owned arguments and a returned scope handle; qualify its
   concrete signature before changing PRE-2. Prefer B only if a real caller
   needs arbitrary borrowed-parameter scoped calls.
2. **Exact live bytes or approximate activity under ownership transfer?**
   Recommend exact origin attribution, including cross-context free/resize;
   measure its metadata cost rather than adopting gran's estimate as fact.
3. **How do nesting and closure work?** Recommend inclusive fixed ancestry,
   explicit linear-handle close, and busy refusal while the last account has
   activity, descendants or storage. Do not erase outstanding bytes on close.
4. **What happens at the scope limit?** Recommend a documented target capacity,
   typed refusal and safe reuse; compare 16 and 64 before choosing a default.
   Slot exhaustion must not become a source proof failure or a hidden fallback.
5. **How are shared map storage and runtime overhead attributed?** Recommend
   creating-scope attribution for map storage, origin retained through backing
   exchanges, independent origins for payloads, and default attribution for
   process-wide infrastructure. Resolve the swap representation before coding.
6. **Which evidence accepts the proposal?** Recommend the prospective 1%
   workload / 1-ns event bounds, exact quiescent rewrite accounting and 0.2%
   sampled adjusted overshoot criterion above, plus a named embedded target
   and budget. A noisy or untested condition remains open.
