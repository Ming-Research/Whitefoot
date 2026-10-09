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

All candidates add a distinct `MeterScope` opened from `MemoryMeter`, an
observing `ScopeView`, and the lifetime rules below. None changes the meaning
of an existing `MemoryMeter` handle. These are signature sketches, not
compiled examples or an amendment to the reviewed specification, v0.117.

| Candidate | Semantics and affected rules | Assessment |
| --- | --- | --- |
| **A. Ordinary generic scoped runner (recommended, subject to its body provider).** | `scope_run` consumes a scope and arguments, calls a statically supplied waiting callee, then returns the scope and result; refusal returns the scope and arguments. PRE-2 specifies the operations and WAIT-2 inherited attribution. | Supports direct calls and `let`-bound spawns through ordinary call/ownership rules. Borrowed-parameter calls need an owned wrapper; the body-provider gap is below. |
| B. Scope operand on `spawn` and optionally calls. | Capture at context creation; amend GRAM-5, FN-1 and WAIT-3. Direct calls additionally need restoration on return. | Spawn-only cannot scope direct calls; supporting both adds syntax where A uses a function boundary. |
| C. A scope-carrying meter parameter selects attribution. | PRE-2, FN-1, EFF and WAIT must define which handle wins when several are passed or stored. | Reject: observation possession selects attribution without identifying entry/exit. |
| D. Setter returning the previous scope. | Change the context until a later restore; PRE-2/WAIT-2/EFF must describe that state. | Reject as the public surface: early exits and nested calls must restore it, and overlapping compute must capture it. |

The owned-argument form has this expressible signature shape (interface
`doc` entries and implementations omitted):

```wf
public enum ScopeRun<A, R> {
  Ran(public scope: MeterScope, public result: R);
  Refused(public scope: MeterScope, public argument: A);
}
public interface ScopeBody<A, R> {
  fn run(argument: A) -> result: R pure waits;
}
public fn scope_run<interface ScopeBody<A, R>>(scope: MeterScope, argument: A) -> outcome: ScopeRun<A, R> pure waits;
```

Function-kind parameters carry their row and `waits`, not runtime function
values (FN-3/FN-5, `spec/kernel-spec.md:1332`, `:1364`). A waiting formal
admits a nonwaiting actual (FN-4 at `:1353`), but calling through that formal
is waiting (WAIT-1 at `:1532`), so the runner is `waits`. By-value arguments
give it `pure`, the empty reference row (EFF-1 at `:1561`), even when the
callee mutates owned data or shared objects. `Result` and `Option` impose no
drop bound (`:2379–2387`); aggregates can carry linear owners (PROV-6 at
`:761`). Returning refused owners has library precedents in
`lib/std/collections/slab/module.wfm:33` (`slab_insert`) and
`lib/std/collections/hash_map/module.wfm:18` (`HashMapFull` returns the pair).
Because every `ScopeRun` owns a `nodrop MeterScope`, spawning it requires
`let r = spawn scope_run::<...>(...);`: an expression-statement spawn requires
a droppable result (WAIT-3 at `spec/kernel-spec.md:2257`, `:2264`).

**The body-provider gap remains a decision.** PRE-2 permits no host-module
implementation records and supplies definitions through the build
(`spec/kernel-spec.md:2552`); existing PRE-1/PRE-2 callables provide no
function-kind callback precedent, although PRE-1 has ordinary generic rows.
FN-5 requires each binding to become one direct ordinary call, without
adaptation or dispatch, and source and linked bodies share an ABI
(`design/compiler.md:13`). Deduction: a single native `scope_run` body cannot
invoke each instantiated waiting coroutine without a forbidden function
pointer or a generated per-instance definition. Ordinary monomorphization
alone does not explain who supplies that body. Options are:

- **(a), recommended:** allow host modules to contain checked Whitefoot
  implementation records, using private build-provided enter/leave primitives.
  MOD-6 (`spec/kernel-spec.md:1692`) hides those primitives from clients;
  PRE-2/MOD composition must change to admit the mixed module. This preserves
  direct calls and a delimited public surface; that preference is a design
  judgment, not an implemented lowering result.
- **(b):** place the runner in a Whitefoot module over public enter/leave
  primitives. It uses ordinary module composition but exposes candidate D's
  unbalanced-entry/restoration obligation to every writer.
- **(c):** generate a build-provided definition per concrete instance. It
  keeps primitives private but needs an account of its special generation
  path against `design/language/system-interface/declaration-home.md:1`,
  which refuses a distinct path by implementation origin.

A borrowed-environment alternative takes `env: &E`, with a callback and runner
row `writes(env)`, like `slab_edit` (`lib/std/collections/slab/module.wfm:45`).
It returns `Result<ScopeRan<R>, MeterScope>`, where `ScopeRan` contains the
scope and result: refusal leaves the borrowed environment in place and
returns only the scope. This avoids an owned argument record for direct
calls; WAIT-3 still requires a writer-written by-value wrapper to spawn it.
It has the same body-provider gap and adds `writes(env)` as a storage-transfer
channel. Prefer the owned form for the spawned rewrite; keep the borrowed
form as an explicit alternative, not an assumed effect-polymorphic wrapper.

### Ownership, closing and observation

Recommend one **nodrop owner `MeterScope` plus droppable `ScopeView`**. A view
contains a slot and generation, can be shared and stored in `Shared`, holds
no slot alive, and reads `Option<u64>` (`None` after close). The owner alone
enters and closes the scope. Deduction from consumption and WAIT-3's
structured joins (`spec/kernel-spec.md:2263–2264`): holding the returned owner
means no invocation through it is active. This avoids a last-handle test;
shared nodrop owner aliases would require runtime activity and alias tracking.

The case for `nodrop` is **conditional quota return**, not “runtime state is
an external resource” or “opaque drop is empty.” The external-resource
wording of `design/language/ownership/linearity.md:1` is a tension to resolve.
The [cancellation-handle proposal, PR #319](https://github.com/Ming-Research/whitefoot/pull/319)
removes `nodrop` for runtime-memory handles so they can enter shared state;
v0.117 still declares them nodrop (`spec/kernel-spec.md:2595–2598`). It also
narrows the opaque-drop prohibition to fieldless handles, so that prohibition
alone cannot justify this new owner. The relevant precedents are explicit
native closers and the refusal of hidden quota return
(`design/language/system-interface.md:5`, `:17`), and `stop_listen` spending
a credit that `close_stop_listener` returns (`spec/kernel-spec.md:2974`,
`:2978`). Deduction: LIV-1/STOR-3 release is unconditional on each exit edge
(`:756`, `:855`), cannot refuse or return the handle, whereas a scope slot
cannot retire with attributed storage remaining. Droppable ownership would
therefore defer retirement in hidden runtime state, affecting future open
refusals, or relocate live bytes. Both change the proposed explicit lifetime;
relocating bytes also violates origin accounting.

The cost is real: a nodrop owner cannot enter `Shared<T: drop>`,
`SharedRead<T: drop>`, `ConcurrentHashMap<V: drop>` (`spec/kernel-spec.md:2353–2362`),
other `T: drop` containers, or expression-statement spawn results. A droppable
view recovers shared observation, not shared entry/close ownership.

Proposed boundaries:

- `scope_open(meter: &MemoryMeter)` writes the meter and returns
  `Result<MeterScope, MeterError>`. Finite slot exhaustion is a typed refusal,
  not heap acquisition, a proof failure or fallback to the default scope.
- `scope_share(scope: &MeterScope) -> ScopeView` reads the owner; duplicating
  a view reads that view. `scope_bytes(view: &ScopeView) -> Option<u64>`
  writes its view, ordering readings through that handle as for `MemoryMeter`.
- The close signature is
  `public fn scope_close(meter: &MemoryMeter, scope: MeterScope) -> result: Result<unit, MeterScope> writes(meter);`.
  `Ok` retires the slot; `Err` returns the busy owner while descendants or
  origin-bearing storage remain. A retry loop can restore its moved binding
  with `set s = move back;` in the error arm (OWN-11/SET-1 at
  `spec/kernel-spec.md:737`). This would be the first **handle-returning close
  among host handles**: `close_read`, `close_send` and `close_stop_listener`
  consume unconditionally, returning `Result<unit, IoError>` (`:2853–2923`,
  `:2978`). Without a waiting primitive, retry is polling. A waiting close
  avoids polling but can deadlock when its caller holds the remaining storage;
  its release/progress protocol would be another obligation.

Closing never frees program storage, clears live charges or moves them to
default. Returned storage may outlive the invocation and remains observable
until freed. Under origin accounting **the account is not monotone after the
last invocation**: another context can resize an S-origin block and charge S
again. Hence even then a reading is concurrently bounded, not exact.
Retirement needs synchronized lifecycle state covering growth, reclamation
and outstanding origin references; a sampled zero cannot authorize it.
That protocol is still unimplemented and must be established before slot
reuse. Views must detect stale generations, including generation exhaustion,
rather than observe a later occupant.

### Nesting, ambient state and execution inputs

Recommend **inclusive fixed ancestry**: a child's bytes count in it and every
ancestor. Innermost-only readings ease disjoint subtraction but omit explicit
subscopes from the parent's reading; do not sum overlapping inclusive accounts.
Ordinary calls and spawns inherit the current scope. The runner restores its
caller's scope after callback cleanup and structured joins. Caller-allocated
arguments and runner context/frame blocks obtained before entry retain their
origins; entry precedes callback execution and subsequent allocation requests.
Existing frame-chunk reuse retains the chunk's origin too. Candidate B could
select at spawn creation instead. Neither changes WAIT-3's argument evaluation
in the starter or its join boundary (`spec/kernel-spec.md:2255–2264`).

Ambient parent selection (“open while S runs means child of S”) and refusing
entry from an unrelated current scope expose state no parameter or row names.
This is in tension with `design/language/system-interface.md:16`, which
rejects ambient host channels absent from effect rows. The argument for a
delimited ambient form is that entry/restoration is bounded and allocation
already has no effect path (STOR-8/PAR-1 at `spec/kernel-spec.md:844–849`,
`:2149–2158`); this is a design judgment, not proof that the new observable
ancestry and refusal satisfy that decision.

Prefer explicit top-level `scope_open(meter)` and child creation
`scope_open_child(meter, parent: &MeterScope)`, writing the meter and reading
the parent. Entry still checks the ambient current scope against the recorded
parent (or the same scope where ownership permits); its compatibility with
the no-hidden-channel decision must be ruled on. The owner-consuming runner
also means the current signature opens children **before** moving the parent
into it; opening a child inside the body would need a specified parent borrow.
This is the cost of explicit ancestry, not a silently available reference.

Extend PRE-2's single sentence “Each memory reading is an input of the
execution” (`spec/kernel-spec.md:2559`) to scope readings, open's capacity
refusal (racing closes), and close's success/busy outcome (racing other
contexts' frees or resizes). Their status follows that explicit contract,
not a `waits` annotation or an already-enumerated WAIT-2 reading; WAIT-2's
map reserve-release bytes are the comparable input (`:2249`). Readings are
not atomic statements in SHARE-3's single order (`:2312`); only HOST-1's
overlapping handle footprints order them (`:2240`). They never select source
acceptance or add allocation effect paths or PAR scheduling edges.

Use PRE-2's quiescent-exact/concurrent-bounded observation contract, bounding
error by bytes moved in the observed subtree during the reading. Inclusive
enumeration additionally needs generation validation or a lifecycle lock:
if a child closes and an unrelated scope reuses its slot mid-read, counting
that occupant is outside the bound, not permissible sampling error. Each
overlapped task must also retain its logical scope through stealing and
helping; a host thread's current context alone is insufficient.

### Ownership transfer is a semantic decision

The accounting choice has three alternatives. Current-scope debit avoids
origin lookup, but is not exact under transfer. Minimal completed witness:

```text
scope A allocates an 8-byte block; ownership moves to B; B frees it.
process live bytes = 0; current-scope ledger: A = +8, B = -8.
```

Clamping B to zero leaves A overstated. Reversing the transfer can understate
the rewrite's scope. Repeated transfers make the error unbounded by current
live bytes; modulo counters and the process meter's concurrency bound do not
repair it. Resize in another scope has the same problem for old/new extents.

| Accounting option | Consequences and recommendation |
| --- | --- |
| **A. Exact origin (recommended).** Moves preserve charges; free/resize debit or extend the recorded origin. | Map tables/chunks and runtime-owned structures carry an origin word per structure, not per entry: a cheap, exact candidate by structural reasoning, with no timing evidence yet. Compiler-emitted heap needs either a **16-byte per-block prefix** preserving 16-byte alignment, or **per-scope arenas with address lookup** replacing malloc. Compare the prefix's memory cost on firn's actual value sizes before selecting it; arenas avoid that prefix but are the larger allocator change. |
| B. Current-scope credits and debits. | This is a net-activity ledger. It is exact only for a closed allocation/free workload; under transfer its live-byte error is unbounded. Reject it for subtraction: over-attribution can exclude unrelated live memory. Its cheap-path estimate assumed the rewrite creates and drops its private keyspaces. |
| C. A sealed region rule. | Forbid storage crossing the scope boundary in **both** directions, making current-scope debit exact by deduction. This needs a new storage classification and closure restriction, and forbids the returned built storage A is intended to support. It is a language alternative, not a runtime workaround. |

For C, references already cannot escape (REF-3/STOR-5,
`spec/kernel-spec.md:694–698`), and spawns join before the runner returns
(`:2263–2264`). The remaining channels are result R, by-value argument A,
moves into or out of atomic target state, map entries, and `writes(env)` in
the borrowed form. Shared state belongs to no binding (`:2270–2272`), so
local ownership alone does not seal it. A sufficient proposed rule makes A
and R contain no runtime-owned storage and forbids the callee's complete
call closure from transferring it across atomic targets, map entries or the
borrowed environment. An explicit boundary transfer for A/R alone leaves
the shared-state and map channels open.

Checking this could reuse STOR-8's finite deterministic call-graph closure
walk (`:852`), not a proof search. It needs a judgment such as “element type
owns or retains runtime storage”: PROV-6's nonempty-release classification
(`:787`) seeds only `Box` and `Paged`, missing `Shared`, `SharedRead`,
`ConcurrentHashMap`, `KeySet` and `Entries` (PRE-1 at `:2322`). Reusing it
would miss transfers; that classification gap must be resolved if C is chosen.

C forbids returning built storage, passing a preallocated buffer or Box,
storing a new Box in the main keyspace, and consuming a queue filled by
another context. Without region-typed handles A cannot carry an outside
`Shared` handle at all. Its suitability for firn is **unverified**: a rewrite
reading only files/host handles and returning storage-free copy data could
qualify, but incoming AOF buffers from another context would not. Creating
and dropping the private maps inside the scope alone does not prove closure.

## Runtime candidate and open questions

Current emitted wrappers pass sizes on allocation, resize and free
(`compiler/src/backend/heap.c:7–29`). After first registration,
`wf__heap_change` performs a thread-local add and relaxed publication to a
cache-line-private slot; readings sum slots and the separately locked pool
counter (`compiler/src/backend/completion/bridge.c:1341–1388`). Pool grants
and returns update under its existing lock (`:1088–1093`, `:1145–1152`).
Direct map accounting calls the same change function; pool-backed map storage
uses runtime take/give (`compiler/src/backend/keyed_table.c:26–34`).

The candidate extends this to one single-writer slot per thread per scope.
TLS selects the current scope for new storage; exact-origin release selects
the stored tag, even on another thread. Drivers install the current ID on
resumption; compute frames carry it through helping and stealing. The earlier
under-1-ns current-scope estimate is unmeasured and does not cover exact-origin
retirement. For emitted heap, a prefix adds an origin store at allocation and
a load from the block's first line at free; **about 1 ns is an unmeasured
estimate**, not a result. The 16-byte prefix doubles a 16-byte allocation's
extent and adds 50% to a 32-byte one, before allocator rounding. It can worsen
cache behavior and RSS while `heap_in_use` still counts requested payload
bytes. Measure firn's real value-size distribution, especially small Boxes;
the expectation that its private keyspaces are mostly map storage is not an
inventory result. Per-scope arenas avoid the prefix but replace malloc and
need address-to-origin lookup, the largest implementation alternative.

Resolve these details before implementing:

- **IDs and lifecycle:** compare 16 and 64 slots including the default.
  Recommend target-declared finite capacity and recoverable open refusal,
  never aliasing or silently using the default. Retire an ID only after its
  owner handles, activity, descendant dependencies and origin-bearing storage
  are gone; non-retaining views instead check generations. Define exact
  retirement separately from sampled byte readings, and never clear slots
  that their writer can still use. Descendant enumeration must not mix slot
  generations.
- **Slots and reads:** keep thread rows on separate cache lines, not every
  scope slot on a separate line. Exclusive origin slots permit one update;
  inclusive reads sum descendants. A leaf reading costs O(T), a subtree or
  process sum O(TS), for T registered threads and S slots. Compare maintaining
  the old process counter (another publication per event) with summing all
  slots, preserving total accuracy. Measure reads at admission frequency,
  sparse/dense scope occupancy, and after thread exit. Never sum clamped
  per-thread balances; use the existing modulo argument on the aggregate.
- **Origin metadata:** a table owns its cell array/reserve and per-user chunks
  hold nodes (`compiler/src/backend/concurrent_map.c:143–196`). Tag each
  structure, retaining the origin with detached spare cells and through
  swap/clear/drain (`:2422`, `:2677`, `:2735`), rather than each entry or just
  the map handle. This resolves the representation principle for exchanged
  storage; metadata, large pool-backed nodes (`:158`) and all resize/release
  paths still need an inventory. Map control state keeps its creating origin;
  separately allocated payloads retain theirs, without recursive relabeling.
  Runtime-owned context/frame storage similarly has a place for an origin
  field (`compiler/src/backend/completion/bridge.c:1168`, `:1263`). Emitted
  Box/byte-string storage is the class using headerless malloc/realloc/free
  (`compiler/src/backend/heap.c:7–29`), requiring the prefix/arena choice.
- **Pool split and shared infrastructure:** add per-scope granted-byte
  counters under the existing pool lock, without also charging the direct
  path for those blocks. Returns debit the owning structure's tag, never the
  freeing context's current scope, including driver cleanup. Keep process-wide
  driver, timer-capacity and descriptor-registry storage in the default account;
  audit this classification against PRE-2 rather than charging whichever
  rewrite first caused shared infrastructure to grow. Accounting machinery
  itself is process/default overhead, not rewrite storage.

## Firn worked example and evidence owed

The admission context opens a rewrite scope, retains a droppable view,
and moves the owner into a `let`-bound spawned scoped runner. The runner calls
the rewrite; its spawned replay worker and compute tasks inherit the scope.
Both private replay keyspaces must be created inside it.
After workers join and replay storage is released, the runner returns its
owner for explicit close; an empty sampled account alone cannot authorize
retirement. Successful close makes later view readings `None`. A returned
live buffer remains charged until actually freed, which is supported by
origin accounting and forbidden by the sealed-region alternative.
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
origin needs at least enough bits to distinguish S slots (4 or 6), but a
16-byte emitted-block prefix, structure fields, generations and arena
metadata are additional RAM. Determine the layout and peak RAM cost on the
actual value-size distribution before claiming suitability for embedded CPUs;
the i9 estimate establishes neither their latency nor their RAM budget.
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
   event cost from open/close and O(TS) reads at admission frequency. Compare
   structure tags plus emitted-block prefixes against per-scope arenas;
   record allocator bytes, cache effects and RSS by firn's real value-size
   distribution, not just the unchanged requested-byte meter. Current-scope
   debit is a cost control on identical traces, not a subtraction candidate:
   move/free one block across scopes to falsify it. Use same-source twins and
   interleaved runs; audit every transfer channel before claiming a region
   restriction would accept firn.
4. Run the firn comparison above, including allocation-heavy script traffic
   for cost: previous SET/MSET evidence is insufficient. Add correctness
   observations for nested scopes, sequential and stolen compute, suspension,
   frame cleanup, capacity refusal, busy close, reuse, and cross-scope map
   swap/clear/drain. Include origin-block growth after the last invocation,
   frees racing close, stale views and a child slot reused outside a subtree
   during its reading. Expected bytes come from the selected contract and an
   independent allocation inventory; retirement needs exact lifecycle evidence
   beyond the sampled counter's bound. Before accepting an embedded claim,
   qualify one named weak target's storage layout and allocation/read costs
   through CI; selecting that target and its concrete RAM/time budget remains
   open. No desktop result substitutes for it.

## Decisions for the owner

These are proposals in dependency order, not additional owner rulings.

### 1. What accounting permits safe exclusion after storage transfers?

**Background.** Firn subtracts private rewrite storage from admission memory.
If A allocates 8 bytes and B frees them, current-scope debit leaves A at +8
and B at -8 after all work completes; repeated transfers defeat a live-byte
error bound. Returned built storage and shared queues exercise this boundary.

**Options.** **A, recommended:** exact origin, with one tag per map table/chunk
or runtime structure; moves preserve tags and free/resize use them. For emitted
heap, choose a 16-byte prefix preserving alignment or per-scope arenas with
address lookup.
Measure prefixes on firn's value-size distribution before choosing: the
structure scheme is cheap by inspection, but neither heap alternative has
measured cost. Keep independent payload origins and put process infrastructure
in default. **B:** current-scope debit; reject for subtraction because transfer
error is unbounded. **C:** seal both boundary directions with a new
storage-owning type classification and a finite closure check; this removes
origin lookup but forbids returned storage, incoming buffers and shared/map
transfers. Firn's compliance is unverified; re-accounting arguments/results
does not seal the other channels.

**Confidence 4/5.** The transfer witness settles B; representation costs and
the region alternative's real-program restrictions still need evidence.

### 2. Who owns retirement, and how is finite scope capacity recovered?

**Background.** An origin account can grow through an external resize even
after its runner returns. Slot retirement must establish no activity,
descendants or origin-bearing storage; neither sampled zero nor unconditional
scope-exit release can perform a conditional, handle-returning close.

**Options.** **A, recommended:** one nodrop owner for entry/close, droppable
non-retaining generation-checked views for shared observation, and a busy
close returning the owner. This makes retry polling and excludes the owner
from shared/maps and `T: drop` containers, but keeps quota return explicit.
**B:** the same split with waiting close; avoids polling but requires a
protocol preventing the caller from waiting on storage it still holds.
**C:** droppable owners with deferred retirement; permits shared ownership
but hides state affecting later open refusals. Shared nodrop owner aliases
are another variant, adding last-handle/activity tracking without restoring
shared-container eligibility. For capacity, prefer documented finite slots
(compare 16 and 64), typed exhaustion and synchronized safe reuse over
unbounded metadata or never-reused slots that exhaust repeated rewrites.
Views must not retain quota or observe reused generations.

**Confidence 3/5.** Conditional release supports the split by deduction;
the external-resource wording of the linearity decision, cancellation-handle
precedent and exact retirement protocol still require resolution.

### 3. Who supplies the generic waiting runner's body?

**Background.** `scope_run` can consume the owner and arguments and return
`ScopeRun<A, R>` with both on refusal. Its all-value row is `pure waits` and
its linear result permits only a `let`-bound spawn. However, PRE-2 host modules
have no implementation records, while FN-5 requires a direct instantiated
callee call; one native body cannot dispatch arbitrary waiting callbacks.

**Options.** **(a), recommended:** admit checked Whitefoot bodies in host
modules with private enter/leave primitives; change PRE-2/module composition
while preserving ordinary direct calls and hiding unbalanced entry.
**(b):** use a Whitefoot module over public primitives; this exposes the
setter/restoration discipline previously rejected as surface D.
**(c):** synthesize a build-provided body per instance; keeps primitives private
but needs justification against the declaration-home rule forbidding distinct
paths by implementation origin. Prefer owned arguments for the spawned
rewrite; the alternative `env: &E`, `writes(env)` form avoids an argument
record for direct calls, returns only the scope on refusal, adds a transfer
channel and requires a by-value wrapper to spawn. A spawn operand would
instead add syntax and still need a separate direct-call scoping form.

**Confidence 3/5.** Signature reasoning follows existing rules; provider
choice (a) is a design judgment and has no lowering experiment yet.

### 4. How are parents selected and nested scopes observed?

**Background.** Inclusive ancestry preserves the descendant-context count,
but implicit parenting and unrelated-scope entry refusal expose an ambient
channel absent from effect rows, contrary to the standing system-interface
decision. Reusing a child slot during a read can also count an unrelated scope.

**Options.** **A, recommended:** explicit top-level open and
`scope_open_child(meter, parent: &MeterScope)`, inclusive fixed ancestry and
an ambient entry check requiring a recorded parent/current-scope match.
Children must be opened before the runner consumes the parent with the present
signature; dynamic child creation needs a specified parent borrow. The
remaining ambient check still needs an explicit ruling. **B:** select both
parent and entry from ambient state; convenient nesting, but delimitation
alone has not established compatibility with the no-hidden-channel rule.
**C:** innermost-only counts; simpler disjoint sums, but omits explicit child
scopes from parent readings and narrows the requested guarantee. For any
inclusive choice, use generation validation or a lifecycle lock while
enumerating descendants. Extend PRE-2's execution-input sentence to readings
and open/close outcomes; handle ordering does not make them atomic snapshots.

**Confidence 3/5.** The generation race is concrete; explicit parenting
reduces ambient dependence, but the entry rule and parent-borrow surface
remain design questions.

### 5. Which evidence is sufficient to accept implementation and cost?

**Background.** No scoped-meter measurement exists. The earlier SET/MSET
comparison barely exercised emitted heap; desktop estimates establish
neither prefix RAM cost nor embedded suitability.

**Options.** **A, recommended:** require independent exact quiescent byte
inventories, generation/retirement and transfer cases, interleaved
same-source comparisons with twin controls, at most 1% workload slowdown
and 1 ns added per accounting event, and overlapping firn rewrites with no
refused writes or key collapse and at most 0.2% sampled adjusted overshoot.
Include prefix/arena memory cost, admission-frequency reads and a named weak
target with owner-selected RAM/time budgets. **B:** qualify desktop firn
first and explicitly defer embedded acceptance; useful staging, but it leaves
weak-target costs unresolved and permits no embedded suitability claim.

**Confidence 3/5.** The experiment comparisons are falsifiable; the numerical
bounds are proposed judgments, and the embedded target/budgets remain open.
