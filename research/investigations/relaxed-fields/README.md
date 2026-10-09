# Relaxed scalar fields under shared read holds

## Question and comparison, before implementation or measurement

Can Whitefoot let a program update a small scalar hint while it holds the
surrounding shared state for reading, retaining memory safety and ordinary
state's transactional meaning, without paying an entry lock? Which scalar
types and operations can have that contract under explicitly declared target
capabilities, including weak embedded CPUs, and what distinguishes the result
from a shared object holding one scalar?

The selected direction is the owner's A: a basic-type field updated atomically
under a read-only hold. This selects the problem to solve, not a spelling,
effect rule, memory model or instruction-width policy. This investigation is
a proposal checked against Whitefoot `9f078d16f65987841fc812e90dbf9046e7090152`, branch
`claude/relaxed-fields`, active specification v0.108. Only this README has
changed since that check revision, including through review head `a476697d8`.
It changes no language rule or implementation. No new compilation,
concurrency test or performance measurement has been run for it.

The owner's additional requirement is that Whitefoot's targets remain open
to security applications on embedded devices with weak CPUs. The five ABIs
currently implemented are an inventory, not a language boundary. This fits
the constitution's [small-embedded-systems objective][constitution]. The
proposal must separate native scalar load/store from optional RMW support;
requiring 64-bit atomics everywhere would exclude targets the owner intends.

Compare four shapes: an explicit scalar cell type (S1), optimization of
existing scalar shared-object statements (S2), a scalar field modifier (S3),
and read-then-upgrade or optimistic statements (S4). First compare their
observable executions and proof/effect boundaries against the current
specification. Before changing the language, compare the same firn source
with stamping disabled, stamping under today's entry lock, stamps in a second
map, and a separate write-on-change statement. These are attribution controls,
not replacements for the owner's direction. Then compare a surviving relaxed
candidate against those controls. Separate hold route from stamp writes in
a complete 2×2 comparison, and give every arm a same-source rebuild twin.
The proposed depth-16 uniform GET criterion on the i9-14900K requires loss
within no-stamp twin noise, with a fixed 1% noise ceiling; a 3% residual loss
there is not acceptable. Hot-key cells instead test recovery of the reader
path against locking, with stamp-versus-no-stamp loss reported descriptively:
store coherence traffic may remain after the path is recovered. Excess noise
means inconclusive, with at most two corrective reruns, never a larger allowance.
Faster code with a weaker safety proof does not qualify. The detailed
protocol and shape-specific rejection conditions below precede any proposed
implementation or new measurement.

The grounds are the constitution's performance and safety requirements,
including revising a restriction when it excludes a safe, better-performing
implementation ([docs/constitution.md:26–66][constitution]), rather than an
assumption that approximate data permits a data race. The comparison method
follows [research/README.md:3–16][research-method].

## Answering the owner's “Store” question first

The owner asked: “This is somewhat like Store, but for basic types; which types
may depend on the platform and must be considered carefully. And if this is
done, Store<u32> becomes meaningless, so how the two are defined together must
be considered. Think it through.”

There is no `Store<T>` declaration on this main. The closest current construct
is **`Shared<T>` plus an `atomic` statement**: a handle to an object holding one
T, with lifetime, guards and whole-statement atomicity. This is explicit in
SHARE-1 and SHARE-3 ([spec/kernel-spec.md:2248–2252,2288–2294][spec]). Searches
for `Store<` in the active specification, `design/` and
`research/investigations/` found no generic proposal with that spelling. The
earlier Redis sketch actually declares a non-generic `struct Store` and puts
it in `Shared<Store>` ([io-model/SHARED.md:90–120][shared-research]). The old
access-effects research also uses “store” for allocation origin/brands,
which the current data model removed, not for a scalar atomic wrapper
([design/language/data-model.md:9,28][data-model];
[access-effects/RESEARCH.md:3,32][access-research]).

**Owner's answer:** “Store” was a typo for `Shared<T>` (board note on
2026-10-09). The owner's question is therefore how a relaxed scalar field and
`Shared<u32>` are defined together, given that the field looks like a
`Shared<T>` for basic types.

A relaxed scalar does **not** make `Shared<u32>`
pointless. The distinction is granularity of atomicity and ownership:

| Construct | What it owns/protects | What a client can rely on |
|---|---|---|
| Existing `Shared<u32>` | A separately lived shared object; the handle can be shared across contexts | A whole block, possibly guarded or combined with other targets, takes effect once in the common atomic-statement order. Several reads and a conditional change form one transaction. |
| Proposed inline `Relaxed<u32>` | One scalar cell belonging to its enclosing owner; no separate allocation, handle count or permission to escape | Each explicitly atomic access is indivisible. Several accesses are not a transaction; another reader may change the cell between them. The cell publishes no other storage. |
| Proposed `Shared<Relaxed<u32>>` | The existing handle/lifetime around the weaker cell | Provides a way to share the cell between contexts, but does not by itself remove the existing whole-object lock or upgrade the cell's meaning to transactional. A separately justified lowering optimization would be needed to avoid that lock. |

For example, `atomic n = &credits when n^ > 0_u32 { set n^ = n^ - 1_u32; }`
is an existing guarded transaction: the guard proves the subtraction's domain
at the update. A relaxed load followed by a decrement cannot supply that
proof about the then-current cell. A second example holds `Shared<u32>` and a
map entry together so the count and entry change at one point. Neither is
equivalent to load/store or fetch-add on a relaxed cell. Even if a program
uses `Shared<u32>` only as a counter, overlapping use cases are a reason to
optimize an equivalent operation, not to erase the stronger contract.

“Relaxed” is a provisional name for an invariant—atomic scalar access without
cross-cell publication—not a decision to import all of C/C++'s memory-order
API. The type/effect discussion below must establish that distinction.

## Existing evidence and the precise gap

The Firn-wf record at `bbd53a2e3007eac0dafd03c6dd150ff301accaf1`, branch
`claude/maxmemory`, was read locally. Its
[memory-limit/README.md:126–162][firn-evidence] records i9-14900K CI runs with
LTO and two interleaved five-second passes. For GET at depth 16:

| Server CPUs | No-stamp twin/base | Stamped head/base | Stamped twin/base |
|---|---:|---:|---:|
| 1 | 1.031 | 0.860 | 0.905 |
| 2 | 0.973 | 0.898 | 0.909 |

Those are the recorded results of [the stamp comparison][firn-run], not new
measurements. The [profiled rerun][firn-profile] records head and head-twin at
0.912, base-twin 1.004, and attributes added samples to entry acquisition and
release while `run_get` remains at 0.7%. The record explicitly leaves cache
effects unseparated. The raw run artifacts, machine settings and original
profile were not independently inspected here; the record and this checkout's
control path substantiate the investigation, not a new causal measurement.

The reported 9–14% loss opens this investigation; it does not establish a
same-source stamp cost or justify reversing a semantic refusal. The compared
builds also change GET's journal/helper boundary and entry layout (Firn-wf
`6585531` versus `66ff776`: [firn/commands/strings.wf:204–230][firn-head-get]
against [its base:204–229][firn-base-get], and
[firn/store/module.wfm:33–38][firn-head-entry] against
[its base:33–37][firn-base-entry]). At one CPU the
head/base ratios 0.860 and 0.905 differ by 0.045, or 4.5 percentage points of
base throughput. The record includes depth-1 GET and SET controls as well as
depth 16; the loss claim concerns depth-16 GET on only one and two CPUs, not
all depths or CPU counts. No same-source falsifier is reported there.
[research/README.md:7–9][research-method] requires both same-source attribution
and a falsifier before assigning the cost to the stamp/lock mechanism.

The source makes the mechanism concrete:

* The semantic checker marks a target read-only only when the checked block
  and guard have no writes through any alias of its binding
  ([compiler/src/semantic/check/control/atomic.rs:275–287][atomic-check]). A
  conditional store is still a write in that union.
* Lowering retains that classification in `readonly_atomic_roots` and in the
  map take's `read` flag. A guard mentioning the target further restricts the
  read route ([compiler/src/lowering/builder/atomic.rs:154–183][atomic-lower]).
  Merely changing “refresh every time” to “refresh when the clock changes”
  inside that same statement therefore leaves the lock selected. Moving the
  conditional refresh to a separate statement is a different control below.
* Entry nodes do not move: `move_block` transfers the cell's 64-bit value
  word containing the node address, after waiting for its reader count to
  reach zero. A reader that discovers a move after counting itself leaves
  and retries; no payload is relocated
  ([compiler/src/backend/concurrent_map.c:492–516,1206–1219,1233–1252][cmap]).
  This is the address-stability premise recorded in the concurrent-map
  decision ([design/compiler/waiting-contexts/concurrent-map.md:5][cmap-node]).
* The reader count prevents reclamation/reuse while a reader reaches the
  node. Entry removal frees under the cell lock; both `try_entry` and
  `acquire_whole` wait for readers before accessing an existing node
  ([compiler/src/backend/concurrent_map.c:820–840,1139–1154,1928–1962,2183–2203][cmap]).
  Shared-map clear/swap requires whole-table exclusion, which waits for all
  active keyed users ([concurrent_map.c:1293–1315,2400–2409,2590–2598][cmap]).
  Deletion and eviction therefore need no new reclamation protocol for an
  aligned scalar store/RMW under a reader pin. The release/acquire handoff
  below orders that store before a later exclusive access. Ordinary accesses
  under genuine exclusion need not be atomic merely to avoid a race;
  concurrent accesses to the leaf under read holds do.
* **A read hold yields the node's storage, not a copy.** The runtime returns
  `slot_of`, and the emitter keeps that pointer
  ([concurrent_map.c:1256–1276][cmap];
  [keyed_table.c:159–184][keyed-runtime];
  [emitter/shared.rs:435–468][shared-emitter]). The header's “copies the value
  out” describes the word map's `wf_cmap_get`, not entry holds
  ([concurrent_map.h:1–4,29–30][cmap-header]). Missing keys yield the map's
  shared zero slot, or `.wf_table_none` on the held-entry route
  ([concurrent_map.c:1062–1080,1276][cmap];
  [emitter/shared.rs:387–403][shared-emitter]). `None` exposes no payload
  fields ([spec/kernel-spec.md:2354–2357][spec]): it cannot supply a relaxed
  leaf to update. A future copying
  optimization must preserve shared cell identity rather than stamp a copy.
* “Lock-free read path” is the repository's name for the normal route, not a
  proof that an entire lookup is lock-free: `wf_cmap_read_entry` may wait on a
  writer, help a move and, on `IMPATIENT`, take `wf_cmap_hold`. This existing
  progress mechanism must be distinguished from the forbidden new fallback
  that implements an unsupported scalar atomic with a hidden lock. The
  fallback actually excludes other users, so the scalar access needs no
  special runtime path there; grouped reads have the same fallback
  ([concurrent_map.c:1266–1271,2083–2107][cmap]).
* Nested-map reads through `wf_cmap_held_entry` take no inner reader count:
  enclosing ownership or a shared outer hold stabilizes the index, while
  ordinary inner writes require an exclusive whole hold
  ([concurrent_map.c:2210–2232][cmap]). Relaxed leaves may not weaken that
  exclusion for inner index changes or node lifetime.
* Entry storage already supports the required scalar alignment: slot offsets
  round up to slot alignment, nodes use 16-byte grains, and alignments above
  16 are refused ([concurrent_map.c:145–150,678–688,711–734,1068–1074][cmap];
  [compiler/src/target.rs:1243–1274,1734–1759][targets]). This preserves a
  field's required alignment up to 16 bytes; field size alone does not prove
  atomic alignment. It is not a general 16-byte atomicity guarantee.

These runtime premises support adding aligned integer atomics without changing
the map's lifetime/hold protocol. They do not discharge the compiler's
`noalias`, aggregate-access or event-model obligations below.

The language gap, independent of that compiler, is an entry with a stable
payload and an independently changing numeric hint. Two contexts should be
able to read the payload while each atomically replaces the hint, without
requiring exclusive access to the payload. Today SHARE-3 puts **all** those
accesses in one transaction. Hardware atomicity alone cannot relax that
language guarantee.

Redis is an oracle for the algorithm, not for Whitefoot's concurrency safety.
Redis 7.0.15 stores its LRU/LFU data in a 24-bit `lru` bitfield
([src/server.h:847–859][redis-object]); firn uses a 32-bit field. Redis's
`updateLFU` reads/decays/increments and assigns the packed minute/counter;
`lookupKey` refreshes LRU or LFU subject to its flags and child-process
condition ([src/db.c:50–57,87–120][redis-db]). These are plain accesses, with
no per-access C11 ordering. That source alone does not establish that Redis
permits racing worker threads to access this field. In firn, concurrent LFU
load/compute/store may lose increments, and concurrent LRU stores can install
an older clock after a newer one. Both are defined possibilities to evaluate,
not consequences made acceptable merely by calling the policy approximate.

## Current rules and earlier positions

### The current memory model

SHARE-3 says an atomic statement “takes effect at one point after it begins
and before it completes” and that its statements “take effect in one order”
([spec/kernel-spec.md:2288–2294][spec]). This is whole-statement
linearizability, with same-context source order, not a choice of C11
`memory_order_*`. The permission to share a read hold is conditional on
preserving outcomes of exclusive blocks.

The surrounding obligations matter as much as SHARE-3:

| Rules and source | Current guarantee affected by this investigation |
|---|---|
| SHARE-1/2, [spec/kernel-spec.md:2248–2286][spec] | State is reached only through targets; object lifetime lasts through the statement; references end with its block. Held-state paths are removed from the enclosing footprint. |
| WAIT-1/2/3, [spec/kernel-spec.md:1529–1534,2227–2246][spec] | `atomic` waits; its block cannot wait or nest another atomic statement. Contexts execute in order; atomic-statement order is an execution input. Weak progress has stated conditions, not a hardware timing bound. |
| EFF-1/2/3/5, [spec/kernel-spec.md:1538–1606][spec] | Exactly checked reads/writes on reference paths; writes kill supported facts and constrain interference. `pure` has a specific deduplication/reordering permission. A signature alone must suffice at calls. |
| CAP-1, PAR-1/2, [spec/kernel-spec.md:2142–2173,2212–2220][spec] | Ownership and ordinary paths supply interference; read/read overlap preserves source-order results. Waiting statements get no implicit overlap. A new atomic scalar is not permission for nondeterministic implicit parallelism. |
| HOST-1, [spec/kernel-spec.md:2222–2225][spec] | Host order arises from overlapping state footprints with a write. A relaxed flag cannot quietly acquire a publication or host-order meaning. |
| OWN-9, TYPE-11, ENT-2/3/5, [spec/kernel-spec.md:734,420–428,2963,3283,3453–3486][spec] | Read-only call storage and exclusive write reachability support optimizer assumptions; shared struct invariants hold at statement boundaries; facts persist until a local kill. A concurrent store has no kill point in this context, so the mutable cell itself must never support a fact. |

### Proposed event model for S1/S3

This is a proposed contract to check, not a proved model or an amendment.
Ordinary state retains SHARE-3's single point and global statement order.
The ordinary projection of a block, given its scalar observations, must be
consistent with that order. Relaxed loads, stores and RMWs are separate events
inside the block's lifetime; they have no joint commit point with it. Source
evaluations execute once. A scalar store neither publishes ordinary storage
nor supplies an additional permission to reach it.

For each cell lifetime, initialization precedes publication; every load reads
one initialization/store/RMW value from that lifetime. Writes have a per-cell
total modification order. Happens-before includes context program order and
synchronizing hold handoffs, publication and joins; scalar reads-from alone
adds no synchronizing edge. For happens-before-related accesses to one cell,
coherence requires writes to follow that order; a later load cannot go
backwards from an earlier load's source; a load before a store cannot read
from that store or a later modification; and a load after a store cannot
read an earlier modification. An RMW reads its immediate predecessor
in modification order. These are the relevant [LLVM monotonic
constraints][llvm-order], with the additional no-thin-air condition below.
Ordinary statement order across unrelated targets does not impose a global
order on independent relaxed cells.

The hold boundary must explicitly order relaxed events too: events before
releasing a reader pin happen before the accesses of a subsequent exclusive
holder that waits for that pin. In this checkout, successful entry reads keep
the count; `wf_cmap_unread_entry` decrements it with release, and the writer's
`try_entry` waits using a seq_cst load
([compiler/src/backend/concurrent_map.c:1206–1219,1279–1281,838,705–708][cmap]). The intervening
count RMWs carry the release sequence to the acquiring zero observation.
Thus an exclusive load cannot return a modification older than a store whose
reader has handed off to it; it sees that store or a later modification, not
every earlier store's value. The proposed specification must require this
edge, plus the corresponding writer-to-reader and writer-to-writer handoffs,
for every hold implementation. The map path is evidence for this mechanism,
not qualification of every target/runtime path.

WAIT-2 currently lists atomic-statement order among execution inputs
([spec/kernel-spec.md:2227–2233][spec]). Add cell modification orders,
reads-from choices and synchronizing handoffs constrained by the model, so
identical inputs still determine each context. The following abstract litmus
fragments use `load`, `store` and `fetch_add` as proposed scalar events, not
current WF syntax. The contexts overlap their read holds and share initialized
cells; no unmentioned exclusive hold or join orders their accesses.

| Litmus | Proposed permitted outcome and what the old single-point model excludes |
|---|---|
| Lost update: `c=5`; A and B each do `r=load(c); store(c,r+1)` | Both read 5 and the final value is 6. Serial execution of these two increments gives 7. This isolates the LFU lost-update mechanism; it is not a claim that every probabilistic LFU access increments. |
| Multi-target: a relaxed counter starts at 0; A1 does `a=fetch_add(counter,1); store(stamp,1)`, A2 does `b=fetch_add(counter,1); store(stamp,2)` | `a=0, b=1`, but stamp modification order is 2 then 1. No serial order of the two complete blocks matches both. The counter is itself relaxed: an ordinary counter exclusively held by both complete blocks would serialize them and forbid this outcome under the handoff rule. |
| IRIW: `x=y=0`; writers store `x=1` and `y=1`; reader C loads x then y, reader D loads y then x | C sees `x=1, y=0`; D sees `y=1, x=0`. Each cell is coherent, but the readers disagree on cross-cell order; there is no single order of whole blocks producing both observations. This is permission, not a promise that each backend exhibits it. |

**No thin-air values: retain the promise, add its missing condition.** Merely
requiring a read to have a store source does not prevent cyclic justification:
with `x=y=0`, A does `r=load(x); store(y,r)`, B does
`s=load(y); store(x,s)`. Assigning 42 to both loads lets each store justify the
other load without initialization or an independent computation supplying 42.
Recommend an RC11-style requirement that the union of program-order and
reads-from edges is acyclic ([RC11 §3.2, Definition 1][rc11]). This excludes
that execution while permitting the three litmus outcomes above. Neither the
words “C11 relaxed” nor the LLVM ordering name alone are the Whitefoot proof.

**The LLVM lowering cost bears directly on a one-instruction promise.**
LLVM `monotonic` does not order a load before a later store to another cell
([ordering constraints][llvm-order]). Nor does ordinary IR preserve a false
dependency: LLVM 21.1.0's [branch folding][llvm-fold-branch] removes a
conditional branch whose successors coincide. The RC11 paper's fake
control-dependency mapping (§§5–6) therefore cannot simply be written in IR
and assumed to survive optimization. A realistic lowering through existing
LLVM mechanisms uses an acquire load or a suitable fence to retain the
load-to-later-store edge ([LLVM acquire code generation][llvm-atomic]); a
cheaper dependency-based route needs a new IR mechanism and its optimization
contract. This is a mapping obligation, not a new source publication right.

For native-width accesses in qualified ordinary RAM, the cost of that edge is:

| Target | Ordering cost beyond a plain relaxed access |
|---|---|
| x86 TSO | No extra machine fence; TSO already preserves load-to-store order. The compiler must still preserve the required edge ([RC11 §4][rc11]). “Free” here means no added hardware ordering instruction, not unrestricted LLVM motion. |
| AArch64 | An acquire load such as `LDAPR` with the required RCpc feature, or `LDAR` otherwise, instead of a plain `LDR` ([Arm's compiler mapping][arm-rcpc]). It can remain one memory instruction while imposing real ordering constraints. |
| ARMv7 | An acquire mapping such as `ldr; dmb` adds a hardware barrier ([LLVM acquire mapping][llvm-atomic]; [RC11 §6][rc11]). |
| RISC-V RVWMO | `fence r,w` between the load and later store orders that pair; a general acquire mapping may use a stronger fence such as `fence r,rw` ([RV32I memory-ordering instructions][rv32]). The precise LLVM sequence still needs qualification. |
| Qualified single-core MCU | For this edge alone, a compiler barrier suffices when all accessors execute on that core and no DMA/other bus master shares the cell. This follows from the single-core accessor premise; it does not remove separate device, hold-handoff or interrupt-exclusion requirements. Whether a selected LLVM lowering avoids a hardware fence remains unverified. |

Weakly ordered multicore therefore has a real ordering cost, even when the
scalar width is native; a universal promise of one plain instruction is not
justified. Inspect each selected mapping, including orders already supplied
by holds. These are source-grounded mappings and conditional deductions,
not emitted-code or timing results for Whitefoot. The RC11 proof does not
qualify AArch64, RISC-V or the full mixed Whitefoot model. An alternative is
to permit causally unsupported scalar values explicitly, subject to their
type bounds and no authority over other storage. That would weaken this
draft's promise and needs the owner's choice; it is not recommended merely
because the scalar contains no pointer. The full mixed ordinary/relaxed model,
compiler transformations and progress remain to be validated independently.

Finally, the model requires a proof boundary: cells never support facts,
guards cannot read them, and type invariants cannot mention their contents.
Facts about an already loaded ordinary local remain valid. The precise
ENT-2/3/5 and TYPE-11 obligations are given under S1 and S3 below. Treating
these events as an optimization under SHARE-3's existing final sentence
would be wrong.

### Positions to reopen, preserve or distinguish

These are quotations of design reasons and historical research, not alternate
language authority. Historical forms such as nested map statements are not
the current syntax.

| Position and quotation | Consequence for this proposal |
|---|---|
| [design/language/waiting/shared-objects.md:30][shared-node]: “Atomic fields and lock-free cells: rejected because they expose interleavings of single reads and writes inside what the context meaning makes one atomic step.” | S1/S3 reopen exactly this refusal. The changed ground is a performance signal and the owner's willingness, in selecting direction A, to investigate weaker hint semantics. The semantic objection still stands. Reversal requires an owner-accepted change of tradeoff after same-source controls, not a claim that firn refutes it. |
| [io-model/SHARED.md:28–31][shared-research]: “A shared object is interior mutability under a checked, lexically scoped lock”. | Interior mutability was not forbidden categorically. What was selected is scoped transactional mutation. Scalar interior mutation needs a separately stated invariant. |
| [design/language/waiting/shared-objects.md:31][shared-node]: “Transactions that read optimistically and retry: rejected because a block runs once, moves values and writes its context's locals, so it cannot be repeated.” | S4 must answer ownership, local writes and effects before timing; a retry loop is not a local lock optimization. |
| [design/language/waiting/shared-objects/keyed-tables.md:11][keyed-node]: read sharing is selected because reads “take effect at one point each in one order”; rejected are “reads that check a version afterwards, which a block that runs once cannot redo after a torn read.” | Keep the reader pin and exclusive ordinary writers. Post-validating a racing plain payload read does not establish safety. |
| [design/language/waiting/shared-objects/keyed-tables.md:35–36][keyed-node]: exclusive holds for every reader were rejected because readers “then take turns”; a writer-marked read form was rejected because “the outcome is the same either way”. | The performance objection remains. An S3 modifier would instead change observable semantics, so this is not that previously refused redundant read marker. |
| [design/language/effects.md:3,28][effects-node]: rows carry only `reads` and `writes`; “Separate external, blocks, and traps effect categories” were rejected as mechanisms rather than state. | A third scalar-access category must distinguish an actual interference contract, not label C intrinsics or a lowering route. |
| [access-effects/OPTIONS.md:168,377,489][access-options] records gated interior cells, rejection of runtime borrow flags, and a deferred “atomic-cell (cross-thread shared-mutable) analog”. | This is an earlier survey, not current approval. It already identifies the missing memory-model decision. No general unsynchronized ordinary access is justified by it. |
| [concurrent-map/DESIGN.md:38–45][map-research]: the lock-free index belongs in the trusted runtime; writing it in WF would require the refused atomic cells. | A bounded integer hint must not quietly become a general raw-pointer, reclamation or synchronization API. The runtime's existing C11 atomics are not source-language precedent. |
| [design/language/data-model.md:7,11][data-model]: signatures, not representations, give access permissions; fixed-width primitives were selected instead of address-width and 128-bit integers. | A machine's instructions do not automatically add types or change reference authority. |
| [design/language/parallelism.md:3][parallel-node]: concurrency is visible through `spawn`; a poll loop may hang if overlapped unasked. | Preserve deterministic implicit computation overlap. Do not infer concurrency from a relaxed type. |

## Effect rows, Shared, and an owned-handle relaxed cell

This answers the owner's effect-row/owned-object comment against this
checkout. Current rules, historical reasons, and proposed extensions are
separated below. The conclusion is to keep the present row boundary explicit
and retain S1-W as the recommendation for an inline stamp. A separately lived
relaxed handle is a conditional alternative, not an implementation of the
same boundary for free. Its complete semantics and performance are unverified.

### 1. What rows guarantee today

**A row is a checked summary of accesses through a callable's reference
parameters, not a ledger of every memory location the execution may change.**
Its entries cover the resolved paths they name; they confer no permission to
write. "Exact" means every declared entry is exhibited and every exhibited
formal-rooted access is covered, not that a row must name the narrowest field.
Locals, transferred owners, allocation/release and held shared state have
specified treatment rather than an implicit all-memory effect. These are
EFF-1/2, [spec/kernel-spec.md:1554–1585][spec], and, for held shared state,
SHARE-2 ([spec/kernel-spec.md:2286][spec]); the recorded reason is to tell
the caller which storage it still owns the callee reaches, independently of
owned-value history ([design/language/effects.md:1–5,25,29][effects-node]).

| Rule | Boundary and consequence |
|---|---|
| EFF-1, [spec/kernel-spec.md:1538–1564][spec] | Only `reads(path)` and `writes(path)`, rooted at reference parameters. A read observes; a write covers observation, mutation, replacement, moving out and freeing at/below the path. A by-value parameter has no row entry. Opaque types do not create an alternative row algebra. |
| EFF-2, [spec/kernel-spec.md:1566–1585][spec] | Syntactic body/callee union, with both coverage directions checked even for unexecuted source branches. Local-only paths frame out of the enclosing signature but retain their checked ordinary footprints. Projecting a call's row never requires its body. SHARE-2 defines the special footprint of the atomic statement itself. |
| EFF-3, [spec/kernel-spec.md:1587–1591][spec] | `pure` is the empty row, not a promise of termination. The intended reading combines its allocation and transformation conditions with EFF-2's retained actions and ordinary control semantics (line 1581): an empty enclosing row does not make a waiting Shared action unobservable or permit ignoring SHARE-3 order. This is not a settled guarantee: EFF-3's licence conflicts with SHARE-3/WAIT-2, with no stated precedence; see the defect below. |
| EFF-5, [spec/kernel-spec.md:1597–1608][spec] | Substitute actual reference paths and argument indices; compare the specified pairs and reject overlapping demands with a write unless separated. A value argument contributes its move/consumption or copy/read at the call site. This is call-argument compatibility, not a test that different Shared handles name different objects. |
| CAP-1, PAR-1/2, [spec/kernel-spec.md:2142–2171,2203–2220][spec] | Ownership, paths and effects supply the interference vocabulary; Shared adds no implicit overlap permission. Adjacent statements and counted iterations may overlap only under the rules, preserving source-order state observables (and the specified accumulator equivalences). Waiting calls deny that permission. |
| HOST-1, [spec/kernel-spec.md:2222–2225][spec] | Host operations in one context have the prescribed order through overlapping state footprints with a write. Disjoint footprints do not order the host. Rows do not describe host scheduling or globally identify every external object two handles might affect. |
| SHARE-1/2/3, [spec/kernel-spec.md:2248–2252,2264–2294][spec] | Shared state belongs to no context or binding. Only atomic targets form its paths; within a block those are checked reference paths. The outer atomic footprint removes every path starting at target state, retains handle/index reads and all other guard/block accesses, and the statement waits. Whole statements take effect at one point in one order, in each context's source order. |

In particular, SHARE-2's last sentence is explicit:

> The statement's footprint is its handle places and index atoms, read,
> together with the footprint of its guard and block from which every path
> that starts at a target's state is removed.

([spec/kernel-spec.md:2286][spec].) Removal is at the atomic statement's
boundary. A helper called **inside** the block with `s: &T` must still declare
its observations/mutations of `s` by EFF-2/5. The design node's "state is a
path of no effect row" describes the enclosing handle boundary, not permission
for that helper to omit writes ([design/language/waiting/shared-objects.md:9][shared-node];
EFF-2, [spec/kernel-spec.md:1573–1585][spec]).

For a function taking `h: &Shared<T>`, `reads(h)` can cover looking up the
object through the handle without changing the handle slot, even when its
atomic block changes T. `writes(h)` instead covers mutation/replacement/move
out of the handle place; it does not mean "writes T". Implicit allocation
and release have no row entry. PRE-1's `shared_share(shared: &Shared<T>)`
declares `reads(shared)` although the runtime increments the object's count;
that is not a write to the caller's handle slot (EFF-1/2, SHARE-1,
[spec/kernel-spec.md:1554–1569,2249–2252,2484–2486][spec]; implementation:
[compiler/src/backend/completion/bridge.c:2684–2691][shared-runtime]). If the
handle is passed by value, even that handle read is local and frames out of
the enclosing row. The historical `serve(..., keyspace: Shared<Store>) ...
pure waits` example makes this visible
([io-model/SHARED.md:95–108,192–198][shared-research]).

Two spawned contexts may therefore own distinct handle slots naming **one**
object. Their rows need not conflict. WAIT-3 transfers only values into each
context; SHARE-2 expressly says two handles do not prove two objects, and
compares possibly identical target states inside a statement accordingly
([spec/kernel-spec.md:2237–2246,2276][spec]). Between contexts, exclusion and
SHARE-3's atomic order govern those mutations. Within one context, every
atomic statement counts as waiting, so PAR cannot overlap it even when its
outer footprint only reads a handle (SHARE-2, PAR-1, PAR-2,
[spec/kernel-spec.md:2282–2286,2155–2157,2203–2205][spec]).

Thus the owner's observation is correct; treating it as a hidden violation
would assume a stronger row promise than the specification makes. More
precisely than "rows describe context-local state": rows describe
**reference-rooted state supplied at that callable boundary**, which includes
held-state references passed to helpers inside an atomic block. The composed
invariant is that checked reference accesses and permitted implicit overlap
respect the ordinary ownership/conflict rules and preserve source-order
state results, while explicitly shared state is reached through the separate
waiting, scoped, atomic-order discipline. It is neither absence of
cross-context communication nor serial execution of all contexts. CAP-1
expressly excludes general race conditions from its data-race guarantee
([spec/kernel-spec.md:2142–2144][spec]; historical rationale:
[design/language/parallelism.md:1–3,11][parallel-node]).

**Clarification recommended, not a new all-memory meaning:** explain EFF-1's
boundary together with EFF-2's framing, SHARE-2's removal and WAIT-1's separate
callable kind. Clarify the enclosing scope of the shared-objects node's
wording and EFF-3's empty-row wording so neither is read as referential
transparency for a `pure waits` function. The explicit SHARE rules explain
the owner's row-boundary example; they do not establish a general theorem
that arbitrary hidden, nonwaiting interior mutation is safe. A rule conflict
also remains: EFF-3 ([spec/kernel-spec.md:1587][spec]) licenses deduplicating
and reordering a `pure` call that allocates nothing with equal arguments,
without qualifying that licence for waiting calls, while SHARE-3 and WAIT-2
require one context's statements and unspawned calls to take effect in source
order ([spec/kernel-spec.md:2227–2230,2288–2290][spec]). The specification does
not say which prevails; the defect is conflicting rules, not absent ordering
rules.

`fn bump(counter: Shared<u8>) -> result: unit pure waits` changes shared state
in [conformance `share-pos-counter`](../../../tests/conformance/cases/share-pos-counter.wf).
Its `nocopy` argument is consumed: two calls need two distinct handles, and
that case's three spawned calls use three handles made by `shared_share`.
EFF-3 does not define whether distinct `nocopy` handles to one object count
as "equal arguments", so it does not establish literal source-level merging
of two bumps. The concrete risk is in lowering: `emit_shared_retain` returns
the same pointer for the new handle
([compiler/src/backend/emitter/shared.rs:855–875][shared-emitter]). If a future
pass derives `memory(none)`/`readnone` from `pure` for such calls, LLVM could
merge calls with equal pointer arguments or reorder shared observations and
updates. This is a potential miscompile, not an observed one: the current
emitter supplies no such row-derived function attributes. The
[effect-attribute test](../../../compiler/src/backend/tests/effect_attributes.rs)
asserts only absence of `willreturn`, has no waiting-function sample, and
would not catch this attribute error. Board item `proof-bl-eff3-pure-waits`
tracks the repair: reconcile EFF-3 and the effects node's `pure` with the
waiting/shared-state boundary and explicitly limit transformations that
change those observations. It is independent of this proposal but constrains
any relaxed design that would keep operations out of rows.

### 2. What was rejected when handles were chosen

The closest match to the owner's recollection is the later shared-state
investigation, not a blanket rejection of reference-based mutation:

> Spawns taking references to a state the starter owns, with no handle or
> count: rejected because the state's content is still reached only under
> an exclusion established at run time, an effect row cannot state it as
> written without forbidding a second spawn, since a row entry excludes
> others for the whole call, and the same state reached through two names
> has its identity checked at run time either way, which handles serve as well

([design/language/waiting/shared-objects.md:32][shared-node].)
[shared-state/DESIGN.md:60–74][shared-state-research] records the owner's
objection: eliminating the count eliminates neither runtime exclusion nor
the runtime identity problem; a spawned call lasts the context's life, so
`writes(store)` prevents the second overlapping use. Each reason has a
different consequence for the proposed inline scalar:

1. **Runtime exclusion.** The relaxed proposal removes exclusive access for
   exactly the designated scalar, replacing it with indivisible scalar
   operations; ordinary entry state retains its existing hold discipline.
2. **A row excludes other accesses for the whole call.** S1-W's
   `writes(cell)` appears on helpers inside one context's atomic block, not
   on a spawned call borrowing the starter's state for its whole lifetime.
   Rows are not compared across contexts; WAIT-3 moves or copies the spawn's
   arguments into its context ([spec/kernel-spec.md:2239–2242][spec]).
3. **Runtime identity checks.** A keyed lookup already fixes the entry's
   identity and returns its slot, so accessing that slot's inline scalar
   needs no second shared-object identity check
   ([compiler/src/backend/concurrent_map.c:1256–1276][cmap]).

These are reasons about the proposed *spawn/reference boundary*, not a
refusal of temporary references to inline scalars. Today WAIT-3 requires
value parameters, and REF-3 forbids stored/returned references
([spec/kernel-spec.md:2239–2241,693–697][spec]).

Three other refusals distinguish the questions:

* "An implicit scope that acquires the object for the life of a reference
  formed through a handle: rejected because whether two adjacent statements
  form one transaction could not be read from the source"
  ([design/language/waiting/shared-objects.md:27][shared-node]; original
  `keyspace^.x` alternative at [io-model/SHARED.md:214–220][shared-research]).
  This rejects an invisible **transaction boundary**, not the machine pointer.
* "Atomic fields and lock-free cells: rejected because they expose
  interleavings of single reads and writes inside what the context meaning
  makes one atomic step"
  ([design/language/waiting/shared-objects.md:30][shared-node]). The original
  refusal is [io-model/SHARED.md:233–235][shared-research]; the explicit-context
  restatement is [io-model/CONCURRENCY-MODEL.md:498–504][concurrency-research].
  Both inline cells and independently owned relaxed handles must reopen this
  semantic choice; an allocation does not make per-access events transactional.
* "Reference or slice values held in aggregates: rejected because a value
  that contains an interior pointer can no longer be relocated by copying
  its bytes" ([design/language/data-model.md:30][data-model]). An owned
  opaque handle is not such a stored source reference. The same node separates
  ownership promises from representation and allows inline owned storage
  ([design/language/data-model.md:5–7,13–15][data-model]). Ownership does not
  mean every value is a pointer.

The affirmative grounds for Shared's handles are independent lifetime and
the explicit block: contexts finish in an unknown order, so no one binding
owns the state; `nocopy` handles are explicitly shared and the last release
releases the state ([design/language/waiting/shared-objects.md:1,7–11][shared-node];
SHARE-1, [spec/kernel-spec.md:2249–2252][spec]). The first Shared record
explicitly calls this "interior mutability under a checked, lexically scoped
lock" ([io-model/SHARED.md:28–31][shared-research]). `&h` reaches the handle
and a target binds `&T` even in this design: "owned versus reference" is not
an exhaustive distinction between APIs.

The access-effects records supply earlier background, with narrower status:

| Record | What it supports, and what it does not |
|---|---|
| [access-effects/OPTIONS.md:168][access-options] | Quotes historical refusals of interior mutability as a fourth mode, writer-emittable gated `cell<T>`, and runtime borrow flags; its quoted objection is "one shared-mutable hole anywhere in the type system makes every aliasing fact conditional". This is a survey of older mechanisms, not the reason Shared later chose its particular handle lifetime. Its combined `RefCell`/`Cell` label is not evidence that every cell needs runtime borrow flags. |
| [access-effects/OPTIONS.md:377,489][access-options] | Explicitly defers "an atomic-cell (cross-thread shared-mutable) analog" as a distinct gated primitive and identifies the missing memory-model decision. It does not select the present relaxed proposal. |
| [access-effects/VERDICT-CORE.md:1–13,743][access-verdict] | The synthesis is "not yet ruled" and says it changes no live tree. Its D10 sketch admits a lock as a scoped projection and forecloses long-lived cross-thread holders without a pool and a lock; it is not an owner-approved proof that handles outperform references. |
| [access-effects/DESIGN.md:272–318][access-design] | Records the direction of temporary, nonstored references, one reference form and effect-derived call compatibility; conflicting complete call effects deny overlap. Its then-proposed rows over whole-value parameters are superseded by current EFF-1 and the reference-only effects decision. Do not carry those historical rules into this answer. |
| [access-effects/MECHANISM-MAP.md:361–389,1116–1126,1221–1231][access-mechanisms] | Separates sequential aliasing from concurrent ordering/stability. Two lock-bearing `writes(lock)` calls cannot overlap under PAR-1; admitting overlapping synchronized mutation needs an explicitly different interference model. Protocol-governed shared regions would require a stability judgment and memory model. These are research alternatives, not current source permissions or performance results for firn. |

The recovered reasons therefore preserve the present handle design for
transactional Shared state, while leaving the new scalar capability to be
judged on its own ordering, aliasing and placement costs.

### 3. Two meanings of an owned relaxed cell

These are proposed representations and interfaces, not accepted WF syntax.
Both must keep per-cell coherence, snapshot-only proofs and the no-publication
contract in [the proposed event model](#proposed-event-model-for-s1s3).

A uniquely owned heap cell such as `Box<Relaxed<u32>>` is included as a
storage variant of S1-W, because its `.inner` remains visible in rows: it adds
allocation and indirection, but no reference count or hidden state
(EFF-1, STOR-1, OP-9, [spec/kernel-spec.md:1554–1558,810,1170][spec]).

**(a) S1-H: a separately allocated, reference-counted handle.** An entry stores
`access: RelaxedCell<u32>`. A new constructor allocates the scalar state; an
explicit share makes another owned handle; last release destroys the state.
An operation borrowing `h: &RelaxedCell<u32>` reads the handle slot and
accesses separate state, which a *new* rule excludes from its row. A consuming
operation taking the handle by value has no formal-rooted row entry, just
the call-site transfer effects; it must return a handle if its caller needs
continued use (EFF-1/5, [spec/kernel-spec.md:1558,1603][spec]). Moving a handle
is not itself a retain, and borrowing it need not retain for each access when
its owner/entry hold already keeps it live. Current Shared's analogous
count-elision reason is [design/compiler/waiting-contexts.md:19][waiting-backend].
This is one reading of the owner's "an owned object is also a pointer": the current
Shared emitter and layout do represent a handle by a pointer, as OP-9
specifies: `(8,8)`, one pointer ([spec/kernel-spec.md:1170][spec];
[compiler/src/backend/emitter/shared.rs:811–875][shared-emitter];
[compiler/src/target.rs:206,1631–1635][targets]).

The scalar access after obtaining that pointer could use the same qualified
atomic instruction as an inline cell. The complete path is different:
`entry -> handle pointer -> allocation's scalar`, instead of
`entry + scalar offset`. Construction, destruction and explicit sharing add
allocation/free and count updates. The current Shared runtime uses an atomic
64-bit count and fetch-add/fetch-sub
([compiler/src/backend/completion/bridge.c:2596–2613,2666–2696][shared-runtime]);
a relaxed handle need not retain its locks, queues or watches, but would need
its own lifetime implementation. On weak targets its **count operations**
need qualification too; qualifying only a u32 load/store is insufficient for
literal reuse of that runtime. These are structural costs, not measurements
or proof that a particular optimized hot path must issue retain/release.

**(b) S1-I: an inline owned cell with hidden mutation through `&Cell`.** The
entry owns the scalar bytes directly, and operations borrow that inline cell.
Calling the cell owned does not make it a separately lived object or remove
the reference: S1 already owns its inline `Relaxed<u32>`. If S1-I records
`writes(cell)` for stores, it is S1-W. If it records only `reads(cell)` of an
opaque identity and hides the changing bytes, it proposes a **new separation
between cell identity and content** in EFF-1/2, unlike the ordinary inline
substate rule ([spec/kernel-spec.md:1558,1562–1564][spec]). SHARE-1/2 cannot be
invoked unchanged: that state currently belongs to no binding and only
atomic targets form paths into it. S1-I's state is inline in an owner and
accessed by a nonwaiting operation. The representation can match S1-W's
inline code; the hidden-state semantics do not come with it for free.

**Memory and locality, for 1,000,000 firn keys.** The arithmetic below isolates
the stamp component on a 64-bit layout with a 4-byte scalar. It is not total
firn memory, measured RSS, or a measured difference between final entry
layouts. The hosted pointer size is 8 bytes ([compiler/src/target.rs:206][targets]);
entry nodes use 16-byte allocation grains
([compiler/src/backend/concurrent_map.c:145–150,711–734][cmap]), so padding and
size-class crossings must be measured on the actual Entry and key lengths.

| Shape and explicit assumptions | Bytes/key charged here | At one million keys (decimal MB) |
|---|---:|---:|
| S1-W or S1-I, inline u32 with no extra cell metadata; entry padding excluded | 4 | 4 MB scalar storage; no separate cell allocation |
| S1-H, hypothetical compact object: 8-byte handle plus an assumed 16-byte allocation containing an 8-byte count, 4-byte scalar and 4-byte padding; allocator overhead excluded | 24 | 24 MB, 20 MB above the inline component |
| Literal reuse of current Shared object layout and pool for u32: 8-byte handle plus a 512-byte granted block; each allocation/free also takes the pool spin lock | 520 | 520 MB, 516 MB above the inline component |

The compact 16-byte object is an **assumption, unverified**, not an existing
allocator class or an intrinsic minimum for reference counting. Literal
reuse is source-derived: Shared puts state at offset 64 and requests header
plus state bytes ([compiler/src/backend/completion/bridge.h:331–337][shared-bridge];
[compiler/src/backend/completion/bridge.c:2666–2681][shared-runtime]); its pool
rounds to powers of two starting at 512
([compiler/src/backend/completion/bridge.c:992–1004,1057–1062,1098–1123][shared-runtime]).
Thus a requested 68-byte u32 object occupies a 512-byte granted block here.
Each `shared_new`/final free also passes through the pool's spin lock
([compiler/src/backend/completion/bridge.c:997–999,1088–1096,1145–1164,2666–2696][shared-runtime]);
the table's byte totals do not quantify this synchronization cost.
This illustrates reuse cost, not a lower bound on a new relaxed-cell runtime.
Replacing a preexisting stamp changes entry size by its actual aligned layout
delta, not necessarily four bytes; pool reserves and map/node overheads are
outside all three component totals.

The inline store dirties a node line that may also hold the key or payload
header; separating the scalar can reduce that sharing, but adds a pointer
dependency and another allocation to touch. A compact pool can put several
cells on one line and cause sharing between different keys; the separate
count may share the scalar's line too. Which wins is **unverified**, including
cache misses, line ownership traffic and any allocation-elision optimization.
The current map already returns the entry's actual slot after looking up its
key ([compiler/src/backend/concurrent_map.c:795–797,1256–1276][cmap]); an inline
cell does not need the new pointer load. An owner pointer and a reference
pointer can have the same machine argument representation without these two
storage layouts generating identical code.

**The missing PAR barrier is the decisive semantic cost.** For either shape,
consider the schematic operations `store(c, 1); store(c, 2);`. Reading only
one handle/identity in both footprints satisfies the present read/read test.
Overlapping the stores could leave 1 instead of source order's 2, contradicting
PAR-1's required result, not demonstrating that current PAR authorizes a wrong
result ([spec/kernel-spec.md:2146–2157][spec]). The same footprint formation is
used by PAR-2 (lines 2168–2171). `writes(h)` is no general repair: two different
handle slots may name the same cell (SHARE-2,
[spec/kernel-spec.md:2276][spec]). Padding a merely read slot with `writes(h)`
also fails the compiler's category-sensitive exactness check
([compiler/src/semantic/tests/contracts.rs:443–447](../../../compiler/src/semantic/tests/contracts.rs)).
EFF-2's wording is broader: "exhibited" requires only an access at or below
the path, without distinguishing read and write categories, literally
allowing that padding ([spec/kernel-spec.md:1584][spec]). This is the separate
wording defect recorded in proof's backlog; the no-padding argument here
uses the compiler's intended exactness, not that literal definition.
Taking a handle by value can order uses of that one consumed place, but
cannot make two separately owned handles prove distinct state.

Shared avoids this conflict through `waits` and atomic-statement order.
Giving new cell operations `waits` would prohibit their use inside the
motivating atomic block (SHARE-2,
[spec/kernel-spec.md:2282–2283][spec]); merely being inside a waiting atomic
statement does not disable PAR among its nonwaiting inner statements.
Hardware atomic stores prevent tearing/data races; they do not select the
required source order. S1-W keeps store conflicts in the existing row, but
also needs the proposed load/load protection: two sequential observations of
a cell cannot go backwards in its modification order, whereas their implicit
overlap could reverse them.

For S1-H to be a viable candidate, propose a **transitively declared,
nonwaiting interference summary** that denies implicit PAR-1/2 overlap for
every call/statement/iteration that performs relaxed accesses, including
loads. It must survive helpers, generics, private fields and framing of
owned/local handles, and bar EFF-3 deduplication/speculation that would change
observations. This is a proposed callable contract, not a chosen spelling or
an implemented checker. Looking only for visible `&RelaxedCell` parameters
does not cover a helper using an owned or hidden handle. A new state-identity
effect algebra is another possible solution, but would need sound alias
comparison across handles. Either route reopens CAP-1 and the effects and
parallelism decisions ([design/language/effects.md:3,29][effects-node];
[design/language/parallelism.md:1–3,11][parallel-node]); it is not "Shared with
its wait removed". S1-I with hidden content needs the same protection, or a
proved type/path scheme covering every such access. **Unverified:** complete
summary composition, acceptance rules and sequential-equivalence proof.

**OWN-9 and backend facts distinguish (a) from (b).** Current OWN-9 derives
write exclusivity and read stability for the *places in a substituted row*
([spec/kernel-spec.md:734][spec]). In S1-H that can still describe the borrowed
handle slot; it must not be extended to its independently shared pointee.
The existing S1 alias analysis distinguishes a pointer derived into an inline
aggregate from a pointer loaded out of a handle: the latter does not inherit
exclusivity of the slot merely by being loaded from it. This fits the existing
separate-directory/page-pointer distinction and complete-target-contract
requirement ([design/compiler/backend-facts.md:1–7,17][backend-facts]). A
reference to the stable handle slot may retain justified attributes; actual
pointee/derived metadata and linked primitive mappings remain **unverified**.

In S1-I the scalar bytes remain inside the referenced cell/Entry, so hiding
them from a row does not make whole-referent `noalias` or read stability valid.
It needs S1-W's qualifications for every aggregate, range and generic referent
containing a relaxed leaf, plus protection against ordinary aggregate copying
over a concurrently changed leaf. The current nonwaiting reference-attribute
path supplies `noalias` without such a cell distinction
([compiler/src/backend/emitter.rs:1702–1734][emitter]). Both representations
need fresh scalar snapshots and forbid facts over mutable cell contents;
neither ownership nor row framing supplies a stability proof (ENT-2/3/5,
TYPE-11, [spec/kernel-spec.md:2960–2963,3283,3453–3486,420–426][spec]).

**Rule changes if selected** (all proposals; this edit changes none):

| Boundary | S1-H, separate hidden state | S1-I, hidden inline state | S1-W, inline writes recorded |
|---|---|---|---|
| Types and lifetime | PRE-1/TYPE-2/TYPE-9, OWN-1/STOR-3: new handle, construction, explicit share and last-release semantics analogous to SHARE-1; qualify allocation and count support, including its STOR-8/no-heap treatment | New opaque inline type/operations and move/replacement/aggregate rules; no independent handle lifetime | Same inline lifetime work, explicit scalar operations and reference replacement restriction |
| Rows and calls | EFF-1/2/5: specify handle paths versus removed cell-state accesses; EFF-3: preserve nonwaiting shared observations despite framing; no padded handle writes | EFF-1/2/5: introduce a hidden content boundary *inside* reference-reachable storage; EFF-3 transformation limits | EFF-1/2/5 keep load/read and store/write meaning; type/path-based hold classification, exactness and generic composition need verification; EFF-3 must respect the new observations |
| Ordering | CAP-1/PAR-1/2, WAIT-2: nonwaiting summary, per-cell events and execution inputs; HOST-1 must not infer publication/order from independent handles | Same nonwaiting ordering problem; no existing waiting exemption | CAP-1/PAR-1/2: type-sensitive conflicts, including loads; WAIT-2 adds scalar inputs; preserve HOST-1 ordinary-state order |
| Transactions | SHARE-1/2 state-access boundary needs a separately defined relaxed-object category; SHARE-3 still protects held ordinary state, but a call to a relaxed handle is outside that commit even inside its block | SHARE-2/3 must permit selected inline content to change under read holds and outside the joint commit | SHARE-2/3 split ordinary transaction state from Relaxed-typed leaf events |
| Facts and lowering | Scope OWN-9 to handle storage; ENT-2/3/5 and TYPE-11 exclude mutable contents/guards; separately qualify pointee facts | Qualify OWN-9 read and write consequences and suppress unsound inline alias/copy facts; same proof exclusions | Same inline alias/proof obligations, with writes still visible at the named leaf |

Rule anchors for the additional type/lifetime rows are TYPE-2/TYPE-9,
PRE-1, OWN-1, STOR-3/8 and SHARE-1
([spec/kernel-spec.md:410–426,560–574,2298–2300,2331–2332,651–661,843–855,2248–2252][spec]);
effect, ordering and proof anchors are given above. S1-H's independently lived
state additionally needs its own initialization/publication and last-use
handoff rules: a copied handle may outlive the map entry, unlike an inline
cell whose lifetime follows the entry and whose access the hold scopes.
Borrowing an existing `Shared<u32>` cannot supply the proposal unchanged:
opening it waits and cannot nest, and a target cannot use a binder created by the same header
(SHARE-2, [spec/kernel-spec.md:2274,2282–2283][spec]).

### 4. Conclusion and decision consequences

For the requested **inline per-key hint under a map read hold**, recommend
S1-W. S1-H is structurally more expensive in storage/lifetime machinery and
requires an additional interference boundary, while its benefit is an
independently shareable lifetime that this capability does not require.
Separate storage could improve cache isolation or preserve more ordinary
aggregate alias facts; whether those gains outweigh its costs is unverified.
S1-I can equal S1-W's representation but loses the ordinary visible write
without avoiding its alias, proof or PAR obligations. Neither is established
as a performance winner or a complete sound language design. These are
discriminating design grounds under the constitution's performance and safety
requirements ([docs/constitution.md:28–66][constitution]), not an argument
from the number of compiler edits.

Keep the existing effect-row meaning and explain its boundaries more clearly.
Shared already makes an explicit, justified separation between handle state
and shared-object state. Extending that separation to *nonwaiting* operations
is a material language decision, not a correction forced by the owner's
example. Decision 3 below now separates placement/lifetime from type/modifier
spelling and includes S1-H. Decision 4 includes the hidden-state alternative
only with its required new interference contract; S1-W remains recommended
and plain S1-R remains refused. Decision 9's replacement question is scoped
to inline cells; replacing a separate handle is an ordinary handle-slot write,
not a replacement of the shared scalar's bytes.

## Uses and the guarantees they actually need

| Plausible use | Operations | Ordering, accuracy and boundary |
|---|---|---|
| LRU clock on a cache entry | Store; load for inspection/eviction | One intact clock is enough for approximate ranking. No payload publication. Concurrent clock regression and wrap must be acceptable; exact “latest” requires more. |
| Packed LFU minute and logarithmic hit count | Load, local calculation, store; CAS loop if updates must be preserved | One packed word prevents mixed minute/counter halves. Load/store may lose updates. Fetch-add is not the decay/random/saturating algorithm. CAS retries must not repeat a random draw or other client effect. |
| Statistics and hit totals | Fetch-add plus load | Relaxed is enough for a running approximate snapshot with no missing increments from races. A load followed by a store loses increments. Exact totals after a join depend on join visibility and overflow policy. |
| Sampled telemetry gauge | Load/store | Last observed sample, possibly stale; no consistent cross-gauge snapshot or freshness deadline. |
| Sequence hint/cache epoch | Load/store; sometimes fetch-add or CAS | Suitable only as a hint checked by an ordinary safe operation. Wrap/ABA means equality alone cannot authorize lifetime, index validity or stale-reference reuse. |
| Progress/cancellation hint | Load/store, possibly a Bool encoding | A displayed progress hint may be stale. Reliable waiting needs visibility and scheduling rules; a “ready” flag for other data needs publication/ordering and is outside a relaxed-only cell. |
| Unique ticket or quota | Fetch-add for modular tickets; conditional CAS or existing guarded `Shared<u32>` for a bounded quota | No silent wrap for unique identities or resources. A preceding relaxed bound check does not prove a later RMW's current value in range. |

The first experiment should cover integer load/store and, on targets declaring
it, explicitly wrapping fetch-add as separate cost cases. On targets lacking
RMW, test the composition refusal as well as load/store lowering. CAS is a
candidate for lossless LFU or conditional counters, not presumed necessary
for the LRU witness. Exact addition cannot silently inherit hardware wrap:
OP-1/2 distinguishes exact,
wrapping, checked and saturating arithmetic
([spec/kernel-spec.md:920–965,996 onward][spec]). An operation provisionally
named `relaxed_fetch_add_wrap` returns the old T and updates modulo its width.
A checked or saturating RMW would need its own total result/operation rule;
proving a bound on a previously loaded local is insufficient.

Provisional scalar semantics: no tearing or uninitialized value, and no
causally invented value under the proposed no-thin-air condition. A load
observes initialization or a value written to that cell, subject to the
event model's coherence and happens-before constraints.
“Some previously stored value” is shorthand for that constrained set, not
permission to choose any historical bit pattern or a value from an erased
entry's earlier lifetime. There is no cross-cell order, transaction snapshot,
publication edge or bounded freshness from the scalar operation itself.
Existing holds, initialization and joins may still impose order. A safe local
snapshot is ordinary T; it does not assert that the cell still equals it.
The event model separates the coherent scalar contract from its additional
causality requirement. Its complete Whitefoot execution and progress
definition remains unverified, especially for repeated polling.

## Platforms: width is not the whole question

### Current supported targets versus exercised targets

The closed ABI list is [compiler/src/target.rs:50–104][targets], not the set
of architectures LLVM can generally compile. Production emission selects the
host triple ([target.rs:107–134][targets]; [driver.rs:2884–2885][driver];
[compiler/src/backend/emitter.rs:399][emitter]); the ABI inventory is not
a cross-compilation interface.

| Admitted triple | Architecture | Evidence of routine coverage in this checkout |
|---|---|---|
| `x86_64-unknown-linux-gnu` | x86-64 | `ubuntu-24.04` gate, Linux I/O job and compiler release |
| `x86_64-apple-darwin` | x86-64 | ABI record; no corresponding native job found in these matrices |
| `x86_64-pc-windows-msvc` | x86-64 | Windows I/O/runtime/compiler-program job; not the full root gate matrix |
| `aarch64-apple-darwin` | AArch64 | `macos-15` gate and `macos-arm64` release |
| `aarch64-unknown-linux-gnu` | AArch64 | ABI record; no corresponding native job found in these matrices |

Sources: [gate.yml:49–58][gate], [io-hosts.yml:27–30,82–106,246 onward][io-ci],
[compiler-release.yml:190–204][release-ci]. There is no admitted 32-bit x86,
ARM32, RISC-V, WebAssembly, Windows GNU or Windows AArch64 record. LLVM support
for those targets does not establish Whitefoot support. The current toolchain
chooses host Clang and probes IR features, not an atomic-width policy
([compiler/src/toolchain.rs:14–61][toolchain]); no per-operation scalar atomic
qualification was found. No embedded target or embedded execution/runtime
model is defined today. Triple admission is not proof of a CPU feature floor,
and this implementation inventory must not bound the proposed language rule.

**No CPU feature floor is fixed by Whitefoot today.** The production driver
and runtime recipes pass neither `-march` nor `-mcpu`, and the emitter supplies
neither `target-cpu` nor `target-features` attributes. Their shared optimization
flags are `-O2 -falign-functions=64`
([compiler/src/driver.rs:41–56][driver]; [compiler/runtime.mk:20–49][runtime-make];
[compiler/src/bin/whitefootc.rs:990–1006,1095–1105,1118–1133,1211–1218,1259–1267][native-driver]).
The absence of those settings was also checked by searching `compiler/src`
and `compiler/runtime.mk`. Thus Clang defaults are inputs, not a stable
Whitefoot target contract.

On this checkout's MacBook, a driver-only query of `/usr/bin/clang` (Apple
Clang 21.0.0, `clang-2100.3.34.2`) reported the following defaults. For each
listed triple, the command was `clang -### -target <triple> -O2
-falign-functions=64 -x c -c /dev/null -o /dev/null`; `-###` printed the
invocation without compiling or linking. These are this toolchain's C-driver
defaults, not native qualification of the other hosts or final WF binaries.

| Triple | Reported CPU / relevant features |
|---|---|
| `aarch64-apple-darwin` | `apple-m1`, including `+v8.5a` and `+lse`; LLVM's Apple-M1 model also includes LSE2 through Armv8.4-A. |
| `aarch64-unknown-linux-gnu` | `generic`, `+v8a`, no LSE guarantee. |
| `x86_64-apple-darwin` | `penryn`, whose LLVM model includes `CMPXCHG16B`. |
| `x86_64-unknown-linux-gnu`, `x86_64-pc-windows-msvc` | `x86-64`, whose LLVM baseline does not include `CMPXCHG16B`. |

Feature interpretation: [LLVM 21.1 Apple-M1 alias][llvm-arm-alias],
[Apple CPU features][llvm-arm-cpus], [Armv8.4 inheritance][llvm-arm-features],
[Penryn features][llvm-x86-cpus] and [x86-64 baseline][llvm-x86-baseline].
Defaults can change with Clang releases;
Apple AArch64 must not be described as the Linux generic Armv8-A floor.
Linux may use outlined `__aarch64_ldadd*` helpers that select LSE or LL/SC,
depending on compiler settings and the available libgcc/compiler-rt; that
link/runtime combination remains unverified here.

**LTO adds a second code-generation boundary.** The driver requests full LTO
or ThinLTO, with the platform linker on macOS and LLD elsewhere
([whitefootc.rs:197–214,978–979,1034–1039,1124–1133][native-driver]). LTO
generates native code during linking ([LLVM LTO design][llvm-lto]). Since WF
functions lack CPU attributes, the LTO backend's defaults may determine
atomic lowering; the effective CPU/features, including the macOS linker's
choice, are unverified. The motivating firn runs use LTO, so checking a C
driver default or an unlinked `.ll` alone cannot establish their code shape.
Record linker version/options and inspect the linked binary for each LTO mode
being qualified. The Windows runtime likewise uses Clang and C11
`stdatomic.h`, not an assumed MSVC C-frontend atomic policy
([runtime.mk:9–13,43–49][runtime-make]; [concurrent_map.c:55–59][cmap];
[compiler/src/toolchain.rs:14–21][toolchain]).

### Instruction and library guarantees

“Lock-free”, “one atomic memory instruction”, and “one instruction for the
entire function” are different requirements. An LL/SC retry sequence may be
lock-free without being single-instruction or wait-free. An x86 `LOCK` prefix
does not mean a software mutex fallback; it still incurs coherence traffic.
Use natural alignment, ordinary RAM with coherence for all participating
observers (cacheable or uncached as the target requires), and one fixed width
per cell. Do not extend the tables to MMIO, packed unaligned fields or mixed-size
overlapping accesses. An instruction's width is distinct from the atomicity
of the bus/memory region that serves it.

| Target family / feature floor | 8/16/32/64-bit load and store | Same-width fetch-add and compare-exchange | 128-bit caveat |
|---|---|---|---|
| All three admitted x86-64 ABIs | Aligned accesses can use single `MOV` memory instructions. | Fetch-add can use `LOCK XADD` (or `LOCK ADD` without the old result); CAS uses `LOCK CMPXCHG`. These are atomic memory instructions; moving arguments and producing a Bool/result can add instructions. | `CMPXCHG16B` needs CPU support and 16-byte alignment. Other operations may require a CAS loop. Modern Intel also documents some aligned 16-byte moves as atomic when AVX is enumerated; that is not a guarantee for every x86-64 CPU or Clang target. |
| AArch64 without LSE, as in the observed Linux generic Armv8-A default | Naturally aligned 8/16/32/64-bit loads/stores have single-access forms (`LDRB/H`, `LDR`, `STRB/H`, `STR`). | Without LSE, use exclusive load/store sequences and retry where required, possibly in an outlined helper. Lock-free implementations are possible; a single-instruction promise is false. | Pair-exclusive sequences exist, but a load pair is not generically an atomic plain 128-bit load. Qualification depends on the operation and architecture features. |
| AArch64 with FEAT_LSE explicitly guaranteed | Same as baseline. | `LDADD` and `CAS`, with byte/halfword variants, provide single atomic memory instructions for these widths. | LSE `CASP` supplies pair CAS; it does not imply one-instruction fetch-add or a universally atomic ordinary load/store pair. LSE2 and later features must be considered separately. |

Hardware sources: [Intel SDM volume 3A §10.1.1–10.1.2][intel-atomic],
[Arm synchronization guide §§2,3,5][arm-atomic],
[Arm's LSE versus baseline example][arm-lse], and
[Arm's discussion of LSE2][arm-order]. Compiler mappings and the possibility
of library expansion: [LLVM Atomics, “Atomics and Codegen”][llvm-atomic].
These establish the architecture-level distinction; **actual output for all
five Whitefoot triples, their CPU flags, alignment and linked runtime is
unverified**. Apple's observed C default has LSE/LSE2; that does not qualify
the feature settings used when linking attribute-free WF IR.

### Embedded classes the rule must accommodate

These are architecture capabilities for a future target declaration, not
claims that Whitefoot can already compile for these machines. Widths are in
bits and load/store entries require natural alignment in the qualified RAM
region. RMW means an indivisible update, not an ordinary load/add/store.

| Target class | Native single-copy-atomic load/store | Hardware basis for fetch-add / compare-exchange | Limits and ordering |
|---|---|---|---|
| ARMv6-M: Cortex-M0/M0+ | 8, 16, 32 | None: no `LDREX`/`STREX` and no atomic RMW instruction. | 64-bit access is multiple transactions. Normal memory is architecturally weakly ordered. [ARMv6-M ARM §§A3.4, A3.5.1–2][arm-v6m]. |
| ARMv7-M/ARMv7E-M: Cortex-M3/M4/M7; ARMv8-M Mainline: Cortex-M33 | 8, 16, 32 | `LDREXB/H`, `LDREX` and `STREXB/H`, `STREX` support 8/16/32-bit exclusive loops. | No 64-bit exclusive pair; `LDRD`/`STRD` are separate word accesses, not single-copy-atomic 64-bit operations. Normal memory is weakly ordered; qualify the memory system's exclusive monitor and progress conditions. [ARMv7-M ARM §§A3.4–5][arm-v7m], [ARMv8-M ARM §§B5.5, B5.14, C2.4 (LDREX/STREX)][arm-v8m]. |
| ARMv8-M Baseline: Cortex-M23 | 8, 16, 32 | Unlike M0/M0+, M23 has 8/16/32-bit `LDREX`/`STREX` variants; it also adds acquire/release forms. | No 64-bit exclusive or native 64-bit atomic load/store. Baseline is not synonymous with “no RMW”; stronger ordering instructions need not be used for a monotonic operation. [M23 guide §3.5.7–10][arm-m23], [ARMv8-M ARM §B5.5][arm-v8m]. |
| RV32I, including RV32IMC, without A or another atomic extension | 8, 16, 32 | No dedicated atomic instructions: neither LR/SC nor AMO. Native aligned ordinary loads/stores are nevertheless indivisible. | No native 64-bit atomic access. The ISA uses a weak memory model, RVWMO; an in-order implementation is not a stronger language contract. [RV32I §Load and Store Instructions][rv32], [RISC-V A §Specifying Ordering][riscv-a]. |
| RV32 with A (Zaamo + Zalrsc) | 8, 16, 32 | 32-bit `AMOADD.W` for fetch-add; `LR.W`/`SC.W` for CAS. | A alone has no byte/halfword AMO and no RV32 doubleword LR/SC/AMO. Subword masked-word synthesis needs its own layout/interference qualification; do not infer it from 8/16-bit load/store. Monotonic RMW needs no `aq`/`rl` ordering bits. [RISC-V A §§Zalrsc, Zaamo][riscv-a]. |
| Xtensa LX6 in the original ESP32 | 8, 16, 32 in suitable data RAM | Optional Xtensa `S32C1I` provides 32-bit compare/conditional-store; ESP32 declares it present. Fetch-add can use a CAS loop. | No generic 64-bit guarantee. Qualify the exact core configuration, memory region and atomic-control settings; do not generalize across all ESP32-branded chips. Ordinary memory ordering is weaker than x86; `S32C1I` itself imposes stronger order than monotonic needs. An atomics-capable Xtensa LLVM backend by default is not established here: qualification needs a fixed toolchain, features and linked provider, as do the ARM/RISC-V probes. [Cadence ISA §§3.4, 3.8.1–3, 4.3.13][xtensa-isa], [ESP32 configuration][esp32-isa]. |
| AVR and other 8/16-bit MCUs | AVR: 8; other cores only their documented native widths | No general integer RMW promise; target-specific instructions or interrupt exclusion must be declared separately. | On an 8-bit AVR even a 16-bit access can tear across an interrupt; `volatile` does not fix it. A 16-bit core likewise does not imply atomic 32-bit access. [AVR-LibC atomic-access example][avr-atomic]. |

Thus ordinary 64-bit loads/stores on the listed 32-bit cores are **not** a
portable atomic operation: neither alignment nor a doubleword/pair mnemonic
makes two word transfers indivisible. A future architecture extension can be
qualified explicitly; “32-bit” alone cannot establish a wider guarantee.
Conversely, lack of an RMW instruction does not remove 32-bit load/store
atomicity on M0/M0+ or RV32IMC. Firn's `u32` stamp needs only 32-bit load/store,
so its scalar requirement fits every 32/64-bit class above, including those
two. That does not port firn's runtime, and it does not extend this claim to
8/16-bit cores, whose native-width limit is lower.

Weak ordering is compatible with the proposed per-cell relaxed (`monotonic`)
guarantee: it needs indivisibility and per-cell coherence, not a strong global
order or publication of unrelated data. This does not remove the proposed
model's **additional** no-thin-air obligation or the hold/publication/join
handoff rules. The LLVM mapping above needs an acquire/fence or a new IR
mechanism to retain the required load-to-store edges; these costs remain
part of qualification. A single-copy atomic access is not by itself a proof
of the complete event model.

### Single-core interrupt exclusion versus a hidden lock

An MCU can implement one RMW by saving its interrupt mask, masking every
interrupt that could access the cell or preempt into another accessor,
performing a fixed load/compute/store sequence, and restoring the saved mask.
This is established practice: [Rust portable-atomic's single-core option][portable-atomic]
uses interrupt exclusion where native CAS is absent; [critical-section][critical-section]
distinguishes single-core masking from multicore implementations that also
need a lock. [FreeRTOS's generic atomic functions][freertos-atomic] use critical
sections with port-specific ISR restrictions. [Zephyr's C atomic API][zephyr-atomic-c]
uses interrupt locking; its [uniprocessor spinlock implementation][zephyr-spinlocks]
does not spin. These are implementation precedents, not adoption of their
APIs, stronger ordering or unsafe caller assumptions into Whitefoot.

LLVM describes `__atomic_*` library expansion and separately the lock-free
`__sync_*` route for [targets with externally supplied support][llvm-atomic].
Its [RISC-V forced-atomics implementation discussion][llvm-forced-atomics]
explicitly gives privileged single-core interrupt masking as one possible
provider. Neither generic LLVM lowering nor a successful link automatically
supplies or verifies that provider. A library named “critical section” may
use a global spinlock on a multicore target; names are not evidence.

Privilege is a functional premise, not just a deployment detail. On
M-profile with unprivileged execution, unprivileged `CPSID` is ignored without
a fault ([ARMv6-M ARM §B4.2.1][arm-v6m]; [ARMv7-M ARM §B5.2.1][arm-v7m]).
A probe that returns successfully therefore proves no exclusion. The port
must show that the entire mask/operation/restore sequence runs privileged,
either directly or inside a privileged SVC handler; SVC entry alone does not
establish mask coverage ([ARMv6-M ARM §A2.1.2][arm-v6m]). On RISC-V, U-mode
access to the machine-level `mstatus` CSR raises an illegal-instruction
exception ([privileged CSR rules and register table][riscv-csrs]). S-mode
clearing SIE cannot mask M-mode interrupts: higher-privilege interrupts remain
globally enabled while executing below that privilege
([machine ISA §2.1.6.1][riscv-privilege]). Such handlers must be excluded as
accessors or covered by a qualified higher-privilege provider.

**Recommendation, awaiting the owner:** permit this as a declared
single-core RMW implementation, subject to all of these conditions:

* The target/runtime contract restricts all possible cell accessors to one
  executing core/hart. It states privilege, mask coverage, preemption and
  interrupt-entry rules. Mask coverage explicitly includes BASEPRI-style
  partial masking on cores that implement it: a priority threshold leaves
  higher-priority handlers able to run ([ARMv7-M ARM §B1.5.4][arm-v7m]).
  NMI, faults, higher-priority or secure-world handlers
  left unmasked must not access the cell; masking interrupts does not exclude
  DMA or another bus master. Their access must be absent by construction or
  covered by a separately justified memory/ownership protocol.
* The masked sequence implements only the scalar operation, with no wait,
  yield, callback, allocation, lock acquisition or retry loop. CAS has fixed
  success/failure paths. Save/restore handles nesting and entry with interrupts
  already disabled; never unconditionally re-enable interrupts. Scheduler
  preemption must be covered, including any RTOS or security-domain boundary.
* Compiler and machine ordering keep the complete operation inside the
  exclusion interval. Every access path, including native load/store and
  native code sharing the cell, obeys the same representation and event
  contract. Plain or volatile C accesses alone do not prove that contract.
* Record the bounded instruction sequence and its interrupt-latency cost for
  the chosen CPU, memory and runtime. It is plausibly cheap for a native-width
  scalar but not free; a worst-case cycle claim also needs memory wait-state
  and exception assumptions. It provides no whole-block atomicity or new
  publication guarantee to source code.

Under these conditions there is no lock owner to wait for, so no lock-holder
preemption deadlock and no contention retry. This is bounded exclusion with
interrupt latency, not a promise of hardware lock-freedom. A hidden mutex or
spinlock fallback remains forbidden: it can wait for another context and,
in an ISR, deadlock on the preempted holder. Multicore weak targets without
RMW cannot obtain an atomic RMW merely by masking local interrupts. They
retain native load/store capabilities and explicitly refuse unsupported RMW.

This also constrains the **hold runtime**, not just relaxed fields. The
current map read pin increments and decrements a shared reader count with
RMWs ([concurrent_map.c:1206–1219,1279–1281][cmap]); a native `u32` stamp alone
does not make that read-hold path available. The dual-core Cortex-M0+ RP2040
has no CPU RMW instruction but does have SIO hardware spinlocks
([RP2040 datasheet §§2.3.1.3,2.4.3.3][rp2040-datasheet]). Those could support a
separately qualified hold implementation; they do not make a hidden lock
acceptable for the scalar operation. Whether `atomic` statements can be
offered on such a multicore port at all depends on its hold/lifetime and
handoff implementation, which remains unverified, even if no relaxed RMW
is exposed to the writer.

Whitefoot's embedded runtime model is undefined today. This decision needs
the above target/runtime premises and their enforcement, not a choice of
embedded scheduler, interrupt API or complete port. A cooperative runtime
with no interrupt access might need less exclusion; that cannot be assumed
from the ISA or from running a desktop workload on one CPU. Source contexts
still have their in-order meaning, and cell observations still lack proof
authority. Do not infer permission for an ISR to enter today's waiting
`atomic` statement or reuse the desktop map/hold runtime on an MCU. If the
owner admits interrupt exclusion, each port must establish its context,
privileged execution, interrupt coverage, lifetime and hold-handoff contract
before declaring the capability; until then its RMW entry is unavailable.

### Representation and compiler qualification

All admitted ABIs here use 64-bit ordinary pointers, but Whitefoot has no
writer-visible pointer-width integer; TYPE-1 lists fixed widths
([spec/kernel-spec.md:407][spec]). A lock-free pointer-sized machine load
does not license `Relaxed<Box<T>>`, `Relaxed<Shared<T>>` or references: those
values carry ownership, retain/release or validity obligations. The current
allocator-alignment floor is eight bytes in these target records, so a
128-bit proposal also needs a layout/allocation argument. LLVM's `i128` in
data-layout strings is not a source type or an atomicity promise. Use the
existing signed/unsigned integer family, with each operation/width subject to
the target capability table below; 64-bit source integers do not require
64-bit atomic cells on every target. Investigate Bool and f32/f64
load/store separately. Bool is an enum; floating RMW semantics are a separate
numeric question, even where exchanging the representation is possible.

C11 provides `_Atomic(T)`, explicit relaxed load/store/fetch-add/CAS and
`atomic_is_lock_free`; its `ATOMIC_*_LOCK_FREE` macros distinguish never,
sometimes and always lock-free. It does not promise that an arbitrary integer
atomic is lock-free or one instruction, nor that `_Atomic(T)` has T's ordinary
size/alignment ([N1570 §7.17.5–7.17.8][c11]). LLVM supplies atomic load/store,
`atomicrmw` and `cmpxchg`; `monotonic` is the relevant relaxed ordering.
Unsupported widths may expand to library calls. Plain or `volatile` access
does not substitute for an atomic operation ([LLVM Atomics][llvm-atomic]).

The emitter's existing LLVM atomic instruction is the abort latch's seq_cst
`cmpxchg`, in its ordinary and Windows diagnostic writers
([emitter.rs:3723,3759][emitter]). Ordinary IR Load/Store use non-atomic
loads/stores without explicit alignment, including aggregate copies
([emitter/operations.rs:33–68][operations-emitter];
[emitter/places.rs:788–843][places-emitter]); “unordered” would instead name
an LLVM atomic ordering. A relaxed field would add the first source scalar
atomic operations. It needs explicit IR operations emitting `load atomic`,
`store atomic` and, if offered, `atomicrmw`, with qualified alignment and at
least the per-cell `monotonic` guarantees. LLVM requires explicit `align` on
atomic [loads][llvm-load] and [stores][llvm-store]. The no-thin-air choice in
decision 5 additionally requires the acquire/fence or new-IR mapping above;
an all-`monotonic` lowering alone does not establish it.

Qualification should fix the target features, runtime and field layout, and
inspect emitted code and linked symbols for each declared operation. GCC's
`__atomic_always_lock_free` is a compile-time query, unlike a runtime query
that may vary by object; Clang exposes the compatible builtins and its C11
macros ([GCC atomic builtins][gcc-atomic], [Clang stdatomic.h][clang-atomic]).
A “true” answer alone does not prove the instruction count or complete model;
a “false” answer may reflect a compiler's combined load/store/RMW policy even
when native load/store exists. Such queries are qualification evidence, not
the availability rule. A declared interrupt-exclusion implementation needs
its own evidence rather than a misleading always-lock-free claim.
No runtime branch choosing a locked scalar implementation is acceptable.
An outlined AArch64 helper that chooses LSE or LL/SC is not automatically a
lock fallback, but it still fails a strict inline/single-instruction cost
contract and requires inspection.

LLVM's [code-generation guidance][llvm-atomic] explains why some no-RMW
targets send even native-width atomic loads/stores to a library: native
accesses must interoperate with any library RMW on the same object, and a
mutex-based provider requires them to take that mutex too. P1's separate
operation sets therefore need a qualified backend/provider contract that
excludes such mixed access. Hardware availability alone does not supply that
mapping. An LLVM limitation exposed here is a compiler gap to resolve and
record, not permission to emit plain IR or silently borrow a locked library.

### P1: target-capability model, recommended

Replace the intersection of today's five ABIs with explicit per-target data.
The portable minimum is aligned integer load/store at the widths no larger
than that target's native single-copy-atomic width: 8/16/32 on the listed
32-bit cores, and 64 only where single-copy atomicity is guaranteed. On an
8-bit target the minimum stops at 8. Fetch-add and compare-exchange are
separate declared capabilities, not consequences of load/store or integer
type availability. No future target must acquire wider atomics just to join
Whitefoot's target set.

The following is **proposed declaration data**, not an implemented compiler
table. Width sets enumerate operations; an empty set means unavailable.
Each row must also specify alignment, RAM region/coherence, execution scope,
CPU/features, lowering/provider and progress premises. `I` means a native
memory instruction, `E` an exclusive/CAS sequence with qualified progress,
and `C` the bounded single-core interrupt-exclusion provider above. These are
implementation classes, not source effect categories.

| Target profile | Load/store widths (I) | Fetch-add widths / provider | Compare-exchange widths / provider |
|---|---|---|---|
| x86-64 qualified baseline | 8,16,32,64 | 8,16,32,64 / I | 8,16,32,64 / I |
| AArch64 qualified baseline without LSE | 8,16,32,64 | 8,16,32,64 / E | 8,16,32,64 / E |
| AArch64 with declared LSE | 8,16,32,64 | 8,16,32,64 / I | 8,16,32,64 / I |
| ARMv6-M without an approved single-core provider | 8,16,32 | empty | empty |
| ARMv6-M with an approved single-core provider | 8,16,32 | 8,16,32 / C | 8,16,32 / C |
| ARMv7-M/ARMv7E-M, ARMv8-M Mainline or Baseline with qualified exclusives | 8,16,32 | 8,16,32 / E | 8,16,32 / E |
| RV32IMC without an approved single-core provider | 8,16,32 | empty | empty |
| RV32IMC with an approved single-core provider | 8,16,32 | 8,16,32 / C | 8,16,32 / C |
| RV32 with A, without additional subword qualification | 8,16,32 | 32 / I | 32 / E |
| ESP32 Xtensa LX6 with qualified RAM and S32C1I; fixed LLVM toolchain/provider required, default atomic lowering not established | 8,16,32 | 32 / E | 32 / I |
| AVR native minimum, without an approved exclusion provider | 8 | empty | empty |

For example, a future M0+ target would declare `thumbv6m-none-eabi`,
`cortex-m0plus`, 8/16/32-bit load/store with 1/2/4-byte alignment in specified
RAM, and empty RMW sets. An explicitly selected single-core runtime profile
could instead declare C for those RMW widths after the owner accepts that
implementation class and the port establishes its premises. An RV32IMC
profile similarly fixes `rv32imc` and its ABI, with no A extension; a separate
RV32+A profile declares the additional operations. The ISA does not establish
the number of cores: [RP2040 has two Cortex-M0+ cores][rp2040], so M0+ alone
cannot justify single-core interrupt exclusion. Wider software-emulated
loads/stores are outside this initial native-width rule even when interrupt
exclusion could implement them; they would need a separate declared capability
and decision, not a fallback.

Module checking retains ordinary types/effects and records atomic requirements
in composable interface/instance summaries. Selected-target composition
checks the concrete required operation, width, alignment and execution scope
against this deterministic table and reports any missing capability at its
source use. For example, `Relaxed<u64>` access on M0+ and `u32` CAS on RV32IMC
without C fail composition explicitly; a helper must not hide the requirement.
No optimizer success, timing probe, build-host CPU, runtime feature test or
library fallback selects acceptance. If a declared capability lacks a correct
backend implementation, report that toolchain gap explicitly; do not pretend
the source is invalid or silently use a lock. Fix CPU features through explicit
`-mcpu`/`-march` settings or LLVM function attributes and preserve them through
native and LTO generation. Today's drifting defaults satisfy none of this.
This extends the existing module/composition boundary
([design/language.md:15][language-node]); it is a proposed new rule.

The recommendation remains to start with native-width integer load/store;
qualify RMW independently where needed. A single-instruction cost objective
for these loads/stores is subject to the complete event-model qualification,
not a requirement to exclude weak targets. RMW can have a declared sequence
cost; a universal one-instruction RMW rule would unnecessarily exclude both
MCUs and baseline AArch64. No rule adds 128-bit source integers, permits a
hidden lock, or replaces required proof with a runtime safety check.

## Candidate shapes

The following are **fragments**, not complete programs or claimed compiler
acceptances. They use Whitefoot's current named arguments, reference paths,
borrowed matches and `atomic` header, with proposed additions identified.
`Entry.payload` stands for firn's ordinary response data. `encode_get` reads
that payload and writes only a caller-owned reply. `clock` is already computed
outside the hold; no waiting clock call is hidden in a block. The real LFU
calculation uses a scalar snapshot and client-local random state, as in
[Firn-wf firn/commands/access.wf:69–110][firn-access].

### S1 — an explicit scalar cell type

Proposed new opaque, noncopyable, inline `Relaxed<T>`, with explicit
construction, load, store and an optional wrapping fetch-add operation:

```wf
struct Entry {
  access: Relaxed<u32>;
  payload: Bytes;
}

atomic slot = &keyspace[key] {
  match slot^ {
    Some(value: entry) => {
      relaxed_store::<u32>(cell: &entry^.access, value: clock);
      encode_get(value: &entry^.payload, reply: &reply);
    }
    None() => {
      encode_missing(reply: &reply);
    }
  }
}
```

Proposed operation-call fragments for other uses:

```wf
let previous = relaxed_load::<u32>(cell: &entry^.access);
let old_hits = relaxed_fetch_add_wrap::<u64>(cell: &entry^.hits, value: 1_u64);
```

The second fragment requires a declared 64-bit fetch-add capability; it is
not portable to the embedded native-width profiles above. The `u32` stamp
fragment needs only 32-bit load/store and no RMW provider.

There is no implicit conversion to T, no reference to the raw inner integer,
and no ordinary `set` into its representation. Recommend a static rule:
direct whole-cell replacement is forbidden through any reference and allowed
only on an owned binding, subject to ordinary ownership/exclusion rules.
The check uses the source access root's kind, not a publication state a callee
cannot know; a reference alias retains the reference restriction.
`writes(cell)` at `&Relaxed<T>` permits the explicit atomic operations,
never direct replacement, even for an unpublished or exclusively held cell.
The alternative is to define replacement through a reference as exactly one
atomic store of T; leaving it an ordinary write while allowing a read hold
is unsound. Replacing
or moving the enclosing owner remains an exclusive, quiescent operation;
its layout/lifetime treatment must preserve the atomic representation.
Initialization before publication is exclusive. Recommend uniform atomic
leaf access after publication, including in exclusive blocks, as a simpler
initial compiler discipline. This is stricter than the runtime requires:
proved exclusion and the hold handoff permit ordinary machine accesses with
no concurrent atomic access. Such a lowering optimization must preserve the
cell's coherence and ordering contract; it grants no ordinary source-level
reference to its representation. A move cannot relocate a cell an active
reader still reaches. No references escape SHARE-2/REF-3.
The type should be usable as an owned local,
an aggregate field or an array element with the same scalar meaning; it does
not itself grant sharing outside the existing handle/hold boundary.

**Effects are the hard part.** S1-R is rejected under the current path
meaning. S1-W and S1-A keep the inline cell's accesses visible; the
[owned-handle comparison](#3-two-meanings-of-an-owned-relaxed-cell) adds S1-H
(separate hidden state) and S1-I (hidden inline content) as conditional
alternatives that require an additional interference contract.

S1-R would label stores as `reads(cell)`. EFF-1 defines observation and
mutation separately ([spec/kernel-spec.md:1562–1564][spec]); EFF-2 checks the
body's accesses ([spec/kernel-spec.md:1566–1568][spec]); OWN-9 says read-only
call storage stays read-only ([spec/kernel-spec.md:734][spec]). A direct
counterexample is two nonwaiting calls inside one block:

```wf
relaxed_store::<u32>(cell: &c, value: 1_u32);
relaxed_store::<u32>(cell: &c, value: 2_u32);
```

If both report reads, PAR-1's footprint test admits their overlap, which could
leave 1, whereas source order leaves 2. That contradicts PAR-1's required
source-order result; it is not a result current PAR-1 authorizes (PAR-1,
[spec/kernel-spec.md:2146–2157][spec]). PAR-2 forms the same footprints across
iterations ([spec/kernel-spec.md:2168–2171][spec]), so relabeling stores also
misclassifies a loop writing that cell. Shared's removed held-state paths do
not refute this example: its atomic statements wait and cannot overlap by
PAR. Replacing the content boundary and interference rules would instead be
S1-H/S1-I, not a viable unchanged S1-R.

| Boundary | Consequence |
|---|---|
| S1-W, recommended: `reads` for load and `writes` for store/RMW, with type-directed hold selection | A helper `e: &Entry` with `writes(e.access)` exposes the declared `Relaxed<u32>` type at that row path. A caller can classify it without the body. Every written path must resolve to an atomic leaf to retain a read hold; a broad `writes(e)` over ordinary payload remains exclusive. Reads of relaxed leaves also require new PAR conflicts. |
| S1-A: a new path category, provisionally `atomic_access(cell)`, for both observation and mutation | Gives an explicit third callable boundary, but changes CAP-1, EFF-1 and the effects design decision as well as exactness, ancestor coverage, call substitution, alias compatibility, invalidation and PAR/HOST rules. The additional vocabulary has no demonstrated modular advantage over S1-W's declared type and path. |

S1-W's modularity comes from the signature's declared referent type at the
resolved row path, including generic substitution, not body inspection or a
new capability. A hidden field can force a broader, exclusive row; that is
an interface cost to test. It is the smaller-vocabulary recommendation, not
a soundness result or an owner selection.

The embedded analysis preserves this recommendation. A store/RMW mutates the
same cell whether implemented by a native instruction, exclusive sequence or
qualified interrupt exclusion; it still declares `writes(cell)`. The target
requirement summary records the required operation/width separately from the
effect row. A C provider does not make the helper a waiting operation, grant
whole-statement exclusion, or change source interference. If its implementation
can wait or invoke user code, it fails the proposed C contract. This judgment
does not define Whitefoot's future interrupt/context API or discharge the
existing S1-W proof, alias and PAR obligations.

Truthful writes alone do **not** make S1-W conservative under PAR-1/2. With
cell modification order `0,1`, two same-context loads in source order cannot
return `(1,0)` by read/read coherence. Overlapping them as ordinary reads can
put the second before the first and produce that pair. Initially deny implicit
overlap for any statement/iteration whose footprint can reach a relaxed leaf,
including reads covered by an ancestor path; narrower permission would need
its own argument. At minimum same-cell load/load must conflict. Explicit
spawned contexts may share holds; this supplies no reduction permission.

S1-A additionally contradicts CAP-1's complete authority/interference
vocabulary ([spec/kernel-spec.md:2142–2144][spec]), EFF-1's two-category grammar
and ban on a writer-visible capability category
([spec/kernel-spec.md:1540–1544,1562][spec]), and the reads/writes-only decision
([design/language/effects.md:3][effects-node]). Those must be reopened, not
counted as mere implementation work. S1-W still needs CAP-1/PAR-1/2 wording
consistent with type-sensitive conflicts. For either boundary, HOST-1's
ordinary-state order remains; relaxed operations create no new host or
publication order ([spec/kernel-spec.md:2222–2225][spec]).

**Proof boundary.** ENT-5 kills a fact at this context's writes, calls,
consumes or scope exits ([spec/kernel-spec.md:3453–3486][spec]); another
context's store has no such point here. The cell must never support a fact,
even until the next local write. Each load produces a fresh ordinary T;
facts may mention that snapshot, never an equality to the still-mutable cell.
S1's opaque non-integer type with no inner projection excludes the cell from
the integer terms in ENT-2 ([spec/kernel-spec.md:2960–2963][spec]). Contracts,
including returned/entry data, must not smuggle a reread into a stable term.
Explicitly refuse relaxed reads in `when` guards (direct or via helpers),
whose conditions otherwise become ENT-3.S1 facts, and relaxed contents in
TYPE-11 binder data ([spec/kernel-spec.md:3283,420–426][spec]). Invariants
solely over ordinary fields remain available. Whole-owner replacement still
invalidates ordinary paths, and snapshots cannot establish authority to
access otherwise unprotected storage.

The guard exclusion also matches the current wake protocol: ending a read
returns before `table_written`, and grouped read release reports no write
([keyed_table.c:188–195,232–239][keyed-runtime];
[concurrent_map.c:2293–2306][cmap]). A relaxed store under that hold wakes no
watch. Allowing such a leaf in a wait condition later would require read-hold
completion to notify its watchers as well as a new proof/progress rule;
hardware atomicity alone would leave sleepers unwoken.

**Reader contract and composition.** Ordinary payload remains stable for the
hold; each cell access has the proposed scalar semantics above. A relaxed
leaf stays relaxed even inside a block that also names `Shared<u32>` or
several entries: ordinary targets commit together, relaxed events are not
rolled back or part of their joint commit. Source evaluations still execute
once. This must be explicit wherever a client expects transactional logging,
snapshots or scripts. `Shared<u32>` retains its stronger role described above.

**Cost and risks.** For the current hosted map, an inline aligned word plus
the current reader pin and qualified scalar operations, including any ordering
needed by the event model; no per-entry cell allocation. On a single-core MCU,
there is no simultaneous inter-core contention: the relevant comparison is
read/hold-path instruction count, ordinary lock/protocol overhead, native
load/store and (if used) the RMW exclusion sequence and interrupt latency.
The embedded runtime may use a different hold implementation; its cost is
unmeasured and cannot be inferred from this desktop map. In the hosted map,
cache-line ownership and writes on hot keys remain.
The reader count is in the table cell; the stamp is in the separately stored
node, so they do not share a cache line in this layout. The candidate's extra
store instead dirties a node line that can also contain the Option tag, key
bytes or payload header. Every successful lookup reads the key in `same_key`;
nodes are carved from per-user chunks in 16-byte grains, so small nodes can
also share lines with neighbors
([concurrent_map.c:113–128,145–150,678–688,711–734,795–797,1217][cmap]).
Which bytes share a line depends on key length and entry layout; neither
reader-count/stamp false sharing nor node-line contention is a measured
explanation of the old loss. All inline shapes admitting mutation under read
holds (S1-W, S1-A, S1-I and S3) need the same alias and stability audit:

* `reference_parameter_facts` emits `noalias` for source-signature reference
  parameters of nonwaiting functions other than Run references and the
  alias-permitting `swap` family; synthesized functions without that evidence
  also skip it ([compiler/src/backend/emitter.rs:880–883,1702–1735][emitter]).
  Helpers called inside atomic blocks are nonwaiting by SHARE-2
  ([spec/kernel-spec.md:2282–2283][spec]); their signatures receive no exemption
  merely for having a caller that holds a reader pin.
* [LLVM's parameter `noalias` contract][llvm-noalias] excludes accesses via
  unrelated pointers to memory modified by any means during the call.
  Another context's atomic store violates that promise when the helper accesses the cell,
  despite being data-race-free. Suppress `noalias` on references whose
  referent can contain a relaxed leaf **inline**: struct fields, enum
  payloads, array/range elements, or a generic T that may instantiate to such
  storage. Carry this property through generic/exported interfaces.
  LLVM's [“based on” relation][llvm-based] does not extend from the address
  of a Box or handle slot to the pointer loaded out of it merely because of
  that load. Suppression for separately allocated leaves reached only through
  such a pointer is conservative, not required by this argument; inspect
  the actual derived pointer and any separate attributes on it. A reference
  to a separately proved ordinary subfield may keep its justified attributes.
  Audit inlining metadata and all derived aliases.
* Qualify **both** OWN-9 consequences: neither exclusive reachability of a
  written relaxed place nor immutability of a read relaxed place follows
  during a call. Ordinary payload remains protected. The backend-facts
  decision requires the complete target contract before any attribute is
  emitted ([design/compiler/backend-facts.md:1–7][backend-facts]).
* `emit_load` emits a whole-type ordinary `load`, including aggregates
  ([emitter/operations.rs:33–51][operations-emitter]); `copy_storage` emits
  ordinary `llvm.memcpy`/`llvm.memmove`
  ([emitter/places.rs:788–843][places-emitter]). A load overlapping a concurrent
  atomic store can yield `undef` for the racing bytes under
  [LLVM's bytewise memory model][llvm-memory]; it is not a valid scalar
  snapshot. This does not imply every unrelated field becomes undefined.
  Forbid plain aggregate loads/copies that touch a live relaxed leaf under
  shared access. Project ordinary fields separately; any permitted snapshot
  must atomically load each relaxed leaf and claim no cross-field instant.
  Keep S1 cells noncopyable; specify S3's aggregate copy meaning before
  admitting it. Audit matching, argument/result materialization, memcpy,
  memmove and widened/vectorized accesses, not just explicit field reads.
  Quiescent owner moves remain subject to the exclusion rule above.

Read-hold mutation must not inherit whole-referent `readonly`, `memory(read)`,
immutable-load or equivalent assumptions:
`borrow_may_write` currently consults `readonly_atomic_roots`
([compiler/src/lowering/builder/storage.rs:437–442][storage-lower]). Audit all
consumers, including helper signatures, aggregate copies, layout, vectorized
access and alias metadata. S2/S4, if semantics-preserving and with no relaxed
leaves, keep their ordinary exclusion contract; machine atomics alone do not
require this language change. These are implementation obligations for the
proposal, not claimed defects in current main.

**Specification work if selected:** PRE-1 and the type/placement rules for the
cell; operation and numeric rules; EFF-1/2/3/5 and OWN-9 as required by the
chosen effect boundary; ENT-2/3/5, TYPE-11 and contracts/invariants;
SHARE-2/3 and WAIT-2; CAP-1/PAR-1/2/HOST-1; and target-composition diagnostics.
Ordinary field and shared-object semantics must remain intact outside the
stated new category.

### S2 — optimize existing `Shared<integer>` statements

No new syntax or type is necessary for these current-source fragments:

```wf
atomic stamp = &stamp_handle {
  set stamp^ = clock;
}
atomic count = &hit_handle {
  set count^ = count^ +wrap 1_u64;
}
atomic stamp = &stamp_handle {
  set observed = stamp^;
}
```

Recognize a single scalar load, store or RMW and lower it to qualified target
operations **only if all existing SHARE-3 outcomes and progress are
preserved**. Rows and `waits` remain unchanged. A relaxed machine store is not
automatically an implementation of the global source order; SC operations
or a proved equivalent protocol may be needed and can cost more. The reader
gets the current transactional guarantee, not a stale-value hint contract.
On a single-core target a bounded exclusion sequence might be an equivalent
implementation, but P1's monotonic capability alone does not prove SHARE-3's
stronger order or runtime interoperability. When this optional optimization
is unavailable, S2 retains the existing declared Shared protocol; that is
distinct from silently locking an unsupported relaxed-cell operation.

There is an essential interoperability obligation. Suppose one handle uses a
single atomic increment while another uses a locked block that reads twice,
checks a guard and writes. Bypassing that lock allows an increment between
the block's reads or allows its final store to erase the increment. Using
atomic machine loads inside the locked block removes a machine data race but
does not restore block linearizability. A general lowering must coordinate
both routes, or establish for the entire object's reachable uses that there
are no compound/guarded/multi-target accesses. A source-shape recognizer alone
is insufficient. Closed-program qualification is a possible optimization
proof, not a change in what a module accepts or a special path for a benchmark.

Firn's GET still writes a **field of an entry**, not the whole scalar state
of a separate `Shared<integer>`. Its ordinary fragment remains:

```wf
atomic slot = &keyspace[key] {
  match slot^ {
    Some(value: entry) => {
      set entry^.access = clock;
      encode_get(value: &entry^.payload, reply: &reply);
    }
    None() => {
      encode_missing(reply: &reply);
    }
  }
}
```

Putting a `Shared<u32>` inside each entry adds a separately allocated object,
handle traffic and another target. One cannot name `entry^.stamp_handle` in
the same header before `entry` exists, and nested atomic statements are
forbidden (SHARE-2). A second later statement changes lifetime/transaction
boundaries; it is not this optimization and not direction A.

**Cost, risks and specification:** scalar operations could become cheaper
without making `Shared<u32>` redundant—it would be the optimized construct.
Mixed accesses may force protocol costs that erase the gain. No normative
change is needed for a proved equivalent lowering; new runtime interop and
lowering tests would be needed. A weakened order, source eligibility
restriction or lock-free guarantee would instead require a specification
decision. S2 is not a solution to the motivating field gap.

### S3 — designate fields whose accesses stay atomic under read holds

Proposed modifier `relaxed`, absent from GRAM-2 today:

```wf
struct Entry {
  relaxed access: u32;
  payload: Bytes;
}

atomic slot = &keyspace[key] {
  match slot^ {
    Some(value: entry) => {
      set entry^.access = clock;
      encode_get(value: &entry^.payload, reply: &reply);
    }
    None() => {
      encode_missing(reply: &reply);
    }
  }
}
```

Every load and store of the designated leaf would have atomic semantics,
including accesses outside map entries under ordinary ownership and through
helpers. S1's uniform atomic lowering recommendation and optional proved
exclusive-access lowering apply here too. Ordinary owner replacement remains
exclusive. `readonly` cannot serve as this marker: it already controls assignment across a module
boundary, not concurrent access ([spec/kernel-spec.md:416][spec];
[design/language/data-model/readonly-field.md:1][readonly-node]).

An expression `set entry^.hits = entry^.hits +wrap 1_u64` must mean separate
load/compute/store, hence potentially lost updates. Silently making that
expression a fetch-add changes its possible outcomes depending on spelling.
An explicit RMW operation is still needed if the API offers fetch-add/CAS.
What type could its argument have? Passing `&entry^.access` as an ordinary
`&u32` loses the access discipline in a separately checked callee. Either
reject such escaping projections, carry a field qualification in reference
types/interfaces, or introduce a scalar reference/cell type. The last two
choices converge toward S1; the first limits composition and needs a real
writer example to justify it. General helpers cannot be silently inlined to
repair a missing contract.

**Effects and reader contract:** S3 needs the same S1-W/S1-A choice and
the same scalar memory, proof and guard rules. Writes remain truthful
mutations even when hold selection permits sharing. `Shared<u32>` remains
transactional because its unmarked scalar state is not a designated field.
S3 must also define standalone/array use: a one-field wrapper could supply
it, but the resulting distinction between “field” and “scalar cell” needs a
reason beyond firn's current layout.

Unlike S1, S3 leaves `entry^.access: u32` as an ordinary integer place unless
the rules explicitly exclude it. In this checkout that direct place is
ENT-2 **clause (a)**, not (b); (b) admits a subscripted readonly integer field
([spec/kernel-spec.md:2963][spec]). Exclude relaxed leaves from both forms
where applicable, from TYPE-11 binder data, and from direct/derived guard
facts under ENT-3.S1 ([spec/kernel-spec.md:420,3283][spec]). The exclusion also
has to cover opaque goals and entry/result data, not just numeric L0 terms:
ENT-5's support for opaque goals reads the complete expression
([spec/kernel-spec.md:3457–3467][spec]). Ordinary local snapshots still support
facts. A local kill on `set` cannot repair asynchronous mutation.

**Cost and risks:** potentially the same machine layout and runtime cost as
S1, with the same target-qualified widths and RMW providers. A modifier
cannot turn a non-atomic 64-bit MCU field into a native atomic. Less explicit
access syntax carries more implicit behavior; aggregate
copying, matching, generic projections and exported field signatures all
need atomic-aware treatment. Merely exempting a flagged path from
`readonly_atomic_roots` does not address these interfaces or S1's `noalias`
and OWN-9 obligations, which apply equally here.

**Specification work if selected:** GRAM-2's field production, TYPE-2 and
reference formation/substitution, rules for reads/SET-1 and explicit RMWs,
plus the S1 effect, proof, execution and target changes. S3 is viable only
with an explicit modular answer to the `&u32` problem.

### S4 — read first; upgrade or restart at a write

One possibility is a lowering optimization for the current fragment; an
explicit alternative might spell a new `optimistic atomic` statement. This
fragment uses only the existing syntax and asks the runtime to upgrade at
the `set`:

```wf
atomic slot = &keyspace[key] {
  match slot^ {
    Some(value: entry) => {
      let old = entry^.access;
      if old != clock {
        set entry^.access = clock;
      }
      encode_get(value: &entry^.payload, reply: &reply);
    }
    None() => {
      encode_missing(reply: &reply);
    }
  }
}
```

An unconditional stamp reaches the write on every hit, so merely delaying
the lock cannot meet direction A. A conditional refresh sometimes avoids it
but retains an entry lock on every refresh and changes the comparison being
made. It is useful to understand this alternative, not to substitute it for
the selected atomic-under-read-hold capability.

**Retain the read pin during upgrade:** two readers trying to become writers
can each wait for the other to leave. Reserving one upgrader requires a
protocol for the losing reader, and multiple targets reintroduce lock-order
questions. The current writer waits for all readers, including the upgrader
unless the protocol is changed.

**Release the pin, acquire a write hold and continue:** an intervening writer
may replace/delete the entry or change a guard's premise. Existing references
and proofs are no longer valid. Continuing from the first write is unsound.

**Release, acquire and replay:** rollback must include caller-owned reply
buffers, moves, releases, allocations, client random state and nonwaiting
host effects. A version check cannot retroactively legalize non-atomic reads
that raced a writer or a dereference of reclaimed memory. Keeping the read
pin during the speculative portion prevents those races but still needs a
replay-safe block boundary and cannot generally undo effects. This is the
previously rejected transaction design, not a lock insertion tweak.

**Effects, reader contract and composition:** a semantics-preserving version
retains ordinary reads/writes, guards, `Shared<T>` and SHARE-3's whole-block
guarantee, which is stronger than the relaxed-cell guarantee. If restart is
writer-visible or a new restricted statement is introduced, its body effects,
ownership and retry outcomes must be specified and composed across helpers.
It does not make `Shared<u32>` redundant and does not supply single-access
relaxed semantics. Cost includes read acquisition plus promotion/revalidation
and possible repeated work; retry counts and progress under contention are
unverified. A single-core MCU has no inter-core contention gain to justify
that machinery; preemption can still invalidate an unprotected read, and
masking interrupts over an entire arbitrary block is not the bounded scalar
C provider. General replay changes SHARE-2/3, WAIT-2, effects, ownership and
host semantics. Only a genuinely equivalent, statically justified restricted
optimization could leave the specification unchanged.

## Existing-language controls and a policy alternative

These controls test the cost's cause before a language amendment. They do not
replace the owner's selected direction A, and are source-level proposals,
not compiled acceptance or performance results.

**Second map, same key.** Keep payload in `keyspace` and ordinary scalar
stamps in a separate `stamps` map, with a header fragment
`atomic slot = &keyspace[key], st = &stamps[key] { ... }`. Read only `slot`
and update only `st`. SHARE-2 admits both independently named targets
([spec/kernel-spec.md:2264–2276][spec]); with distinct value types such as
`Entry` and `u32`, they have distinct state roots. The per-target effect
classification can keep the payload entry read-held while write-holding the
stamp ([compiler/src/semantic/check/control/atomic.rs:275–282][atomic-check];
[compiler/src/lowering/builder/atomic.rs:154–183][atomic-lower]). Aliasing or a
broader helper row can defeat that classification and must be checked in IR.
Measure the second lookup, allocation/layout and memory overhead, and the
remaining per-key stamp lock. A gain here would distinguish payload-entry
exclusion from all per-key locking. Creation, deletion and replacement must
keep the two maps' key lifetimes consistent in the same multi-target
statements; measure that maintenance cost too.

**Separate write-on-change statement.** Finish GET and compute whether a
refresh is needed under a purely read-held statement. Carry only owned
snapshots/local state out; if stale, use a second write-held statement to
relookup the key and recheck its current state before changing it. This is
legal statement composition today (SHARE-2/3,
[spec/kernel-spec.md:2280–2294][spec]), not an upgrade or replay of GET.
References into held state and facts supported by it do not survive the first
hold (SHARE-2, ENT-5, [spec/kernel-spec.md:2281,3453–3458,3486–3489][spec]);
facts solely about surviving owned snapshots/locals can remain. A deletion or
replacement between statements needs an explicit identity/revalidation
policy; do not blindly stamp a different entry. The stamp is no longer
transactional with GET, so policy quality and script/introspection semantics
must be evaluated alongside throughput.

Redis's clock resolution is one second and its LFU counter increment is
probabilistic ([Redis 7.0.15 src/server.h:847–849][redis-object];
[src/evict.c:69–85,280–306][redis-evict]). These permit unchanged stamps, not
a general “most accesses change nothing” conclusion: at a low LFU count the
increment probability can be one, and the packed minute may change even when
the counter does not ([src/db.c:53–57][redis-db]). Uniform random accesses
with revisit intervals long relative to the clock can make nearly every
stamp stale; uniformity alone does not establish that interval. Measure the
unchanged/stale fraction, full packed-word change rate, recheck cancellation
rate and second-statement frequency for the actual key population/rate, plus
hot-key and Zipf workloads. Preserve each LFU random draw's intended count;
revalidation must not quietly draw again or overwrite a newer counter.

**Per-context lossy access buffer, design alternative.** Append owned keys and
scalar access samples to a bounded context-owned buffer and drain batches at
eviction, accepting a stated loss policy. Existing owners and shared
statements suffice; another context cannot read a local buffer unsynchronized,
so any cross-context drain needs an explicit handoff through ordinary shared
state. Bound memory, revalidate replaced/deleted keys, and measure drop rate,
drain latency, publication cost and eviction quality. [Caffeine's design][caffeine]
supports batching and lossy access records, but uses striped buffers and
maintenance drains; it does not establish a per-context, eviction-only design
for Whitefoot. This is a policy alternative with no language change, not
evidence that direction A is unnecessary.

## Rejection conditions fixed before implementation

Every shape is refused as a solution if it permits a source-level or C/LLVM
data race, torn/uninitialized reads, stale-reference use, silent overflow or
proofs based on another context's mutable value; requires a hidden locked
implementation of an admitted scalar atomic; silently accepts an operation
or width missing from the selected target declaration; or changes ordinary
SHARE-3 behavior. A declared single-core C provider is admissible only if the
owner selects it and its bounded exclusion premises are established. Reject
mask-only RMW for multicore access, unaccounted interrupt/DMA access, or an
implementation that can wait while masked. Native/loop/exclusion cost claims
must match the declaration; LL/SC does not meet a single-instruction RMW claim.
Evidence of small cost on one CPU never overrides these conditions.

| Shape | Additional rejection conditions |
|---|---|
| S1 | Reject if a helper signature conceals mutation, `noalias` or stability attributes cover concurrently mutable leaves, load/load implicit overlap violates coherence, whole-cell replacement bypasses atomic access, cell moves/copies race readers, or the split between ordinary transaction events and relaxed events has no coherent execution/proof model. Reject as firn's performance solution if the depth-16 criterion below fails. |
| S2 | Reject an optimization that can race or interleave with an ordinary locked/guarded/multi-target use of the same object, or loses current source ordering. Already ruled out as the sole solution to the map-entry-field requirement by its scope, regardless of standalone-counter speed. |
| S3 | Reject if passing a designated field through `&u32` loses its discipline, a generic helper needs body inspection to be safe, or surface-equivalent load/add/store unexpectedly becomes an indivisible RMW. Apply S1's model and performance rejection conditions too. |
| S4 | Reject if two upgrading readers can deadlock, any replay duplicates an observable effect or consumes an owner twice, validation follows an unsafe racing read, or continue-after-upgrade uses stale references/facts. It also fails direction A if refresh still requires the entry lock, even if conditional refresh improves one benchmark. |

The hosted firn performance comparison is **prospective** and must not inherit
unrelated differences between the old Firn-wf base/head builds. Its direct
attribution panel has two independent factors, with one fixed entry layout:

| Hold route | No stamp store | Stamp store |
|---|---|---|
| Reader pin | R0: no-stamp baseline | R1: relaxed candidate |
| Entry lock | L0: forced lock, no stamp | L1: identical atomic stamp under the lock |

R0/L0 isolate route cost without a store; R1/L1 isolate it with a store.
R0/R1 and L0/L1 isolate store cost under each route. Compare those paired
contrasts to expose interaction instead of attributing the entire R0/L1
loss to locking. Keep stamp calculation and all other work equal within
this panel. Retain today's ordinary locked-stamp implementation as a separate
control if its generated access differs from L1. Every arm, including L0,
L1 and that existing implementation, gets a twin.

For hot keys, **choose path recovery, not equality with R0**, before seeing
results. `read_in` calls `same_key`, which reads the node's length/key bytes;
the value slot follows that header and key at its required alignment
([concurrent_map.c:678–688,795–797,1217–1220][cmap]). Thus a stamp sharing a
cache line with bytes readers inspect can make R1 pay for dirty-node
coherence even with the reader path fully restored. This is a source-grounded
hypothesis, particularly relevant with four or more readers of hot keys, not
a measured loss or proof that every layout shares that line. Record offsets
and test it with the attribution runs below. A <=1% R1/R0 requirement would
conflate that store cost with failure to recover the hold path, and tightening
noise would tighten the wrong comparison. The hot-key claim is therefore
limited to resolved route savings; it makes no near-zero total-stamp-cost
claim. Uniform cells retain the stricter total-loss criterion.

1. **Before a language change**, pin Whitefoot, Firn-wf, Halo-wf, Redis benchmark
   version, Clang/linker versions, flags and one identical LTO mode for all
   arms. Use one firn source with only the named experimental factors varied;
   keep entry layout fixed. Also compare actual firn main without stamping
   to expose total layout cost. Run R0/L0, the current locked-stamp control,
   and the second-map and separate write-on-change controls first, in the
   staged order below; only after semantic/target qualification add R1/L1 and
   complete the panel. Report the
   second-map and separate-statement representation/semantic differences
   separately; evaluate buffering as a policy alternative. A forced-route
   prototype must change only the final hold selection and its corresponding
   release, not the block's effects, references, arithmetic or representation.
   Changing `readonly_atomic_roots` alone is not that experiment:
   `borrow_may_write` also consumes it ([storage.rs:437–442][storage-lower]).
   Inspect IR and linked code to show that the paired route arms differ only
   in the hold calls/flags and their consequences; otherwise reject the
   attribution as confounded.
2. Use the CI `14900k` runner, confirm it is idle and coordinate a long run.
   Record OS, CPU/microcode, frequency policy, actual driver count, key/value
   sizes, distribution, dataset occupancy, client count and warmup. Pin server
   and client to disjoint physical P-cores, with no E-cores or shared SMT
   siblings; use one logical CPU per physical core and identical placement
   across arms. Recover the motivating settings from artifacts before
   replicating them. Measure 1, 2, 4 and 8 server P-cores separately, with
   uniform, hot-key and Zipf workloads at depth 16; retain GET depth 1 and
   SET at depths 1 and 16 as controls.
   The [14900K has eight P-cores][14900k], so an eight-core server leaves none
   for a disjoint local client: that cell needs a separate CI-controlled
   P-core client host. Until available it remains unmeasured, not replaced
   with E-cores or an SMT-sharing client. Keep client placement/topology fixed
   across the scaling panel or report separate panels. Verify one driver for
   the one-core case: without overlapping entry holders it measures path
   length rather than inter-driver contention. Two-core uniform access may
   also reveal little contention; it cannot establish hot-key scaling.
3. Define a **twin** as an independent same-source, same-settings rebuild in
   separate clean build and ThinLTO-cache directories, then interleave runs
   of both images. Record SHA-256 binary hashes, cache state and build commands.
   Bit-identical twins measure run noise only; differing images also expose
   build/layout variation, which must be reported rather than conflated.
   The old firn record says it reran each image; its numbers are not evidence
   of independent rebuild variation ([memory-limit/README.md:128–135][firn-evidence]).
   Begin with two one-second no-stamp/twin pairs per pilot cell as a small
   timing/noise pilot.
   Use that spread to choose and record the final run duration before timing
   the candidate. Exclude the pilot from the decision batch. Fix **ten paired
   blocks per workload cell**, each containing every arm and twin assigned
   to that stage, with balanced ordering; do not stop a batch early or add
   pairs after seeing a favorable candidate result.
4. Fix the estimator and limits now. For each arm A in a cell, let
   `e_A = median(abs(A_twin/A - 1))` over the ten paired blocks. Every arm
   in a decision batch must have `e_A <= 0.01`; otherwise that batch is
   **inconclusive**, not a candidate failure. For depth-16 uniform GET,
   epsilon is `e_R0`; loss is the larger of `1 - median(R1/R0)` and
   `1 - median(R1_twin/R0_twin)`. A pass requires loss <= epsilon in every
   measured uniform CPU/policy cell; with the noise screen satisfied,
   loss > epsilon rejects this total-cost claim. **A 3% uniform residual
   loss remains unacceptable.**

   For depth-16 hot-key GET, let `e_route = max(e_R0, e_L0, e_R1, e_L1)`.
   The no-store route penalty is the smaller of `1 - median(L0/R0)` and
   its twin estimate; the stamped route saving is the smaller of
   `1 - median(L1/R1)` and its twin estimate. Call the **path recovered**
   only when both exceed `e_route` and the lowering inspection in step 5
   confirms the reader route. These compare L0 against R0 and R1 against
   L1 without requiring equal savings in the two store conditions. If the
   no-store penalty is unresolved, report route attribution inconclusive;
   if it is resolved but stamped savings do not exceed noise, the hot-key
   path-recovery criterion fails. R1/R0 remains descriptive even when its
   loss exceeds 1% or 3%; expose that cost and the interaction, never call
   it a near-zero stamp cost. Zipf, depth-1 and SET results are controls
   reported separately, not substitutes for a failed primary cell.

   Report raw block ratios, spread and first-five/last-five medians. The
   split-half difference is a drift diagnostic, **not another <=0.01
   pass/fail screen**: two five-block estimates add sampling variability
   and do not establish 1% precision. Balanced ordering and the full
   ten-block twin screen supply the preregistered operational noise rule;
   it is not a confidence interval or a guarantee of the true effect.
   Record any placement, thermal or frequency departure alongside the data.
   The old no-stamp ratios differ from 1 by 3.1% and 2.7%; they are not
   this paired estimator, and no evidence yet establishes that the 14900K
   can satisfy the new screen.

   Allow **one initial batch and at most two corrective reruns per stage**.
   Before each retry, record the diagnosed cause, duration and scheduling
   change, and recompute the queue estimate; rerun all arms/twins/cells of
   that stage, not just favorable or noisy cells. Keep all outcomes separate;
   never pool attempts or select the best ratio. Reruns address inconclusive
   measurement, not a noise-qualified performance failure. If the third
   attempt still fails the twin screen, stop this campaign and report
   **“this machine cannot measure at 1%” for these workloads and settings**;
   this is a bounded experimental conclusion, not a permanent hardware limit.
   An unresolved route contrast after the same limit instead ends with
   “no resolved route cost/recovery”, not a claim of machine noise. Do not
   queue later stages while their prerequisite is inconclusive, or widen
   the ceiling. A new campaign requires an explicit revised protocol, not
   resetting this retry counter. These thresholds remain prospective research
   recommendations, not owner-approved requirements or retrospective verdicts.
5. Inspect GET lowering and profile: actual stamp stores must execute; R1
   must retain reader acquisition/release without selecting the entry write
   route for that field. The complete panel supplies both same-source
   falsifiers: force the lock with and without the store, and remove only
   the store under each route. If removing the lock does not recover cost,
   or removing the store does, separate route cost from arithmetic/coherence
   cost and their interaction. Collect `perf c2c`/HITM samples in separate
   attribution runs, with node/field offsets, key lengths and cache-line
   addresses, to distinguish table-cell traffic from dirty node lines and
   neighboring nodes. Check PMU/event availability first; missing counters
   leave coherence attribution unverified, not zero. Profiles do not replace
   the unprofiled paired throughput panel.
6. Exercise LRU and LFU separately under uniform, hot-key and Zipf access;
   record the control-specific stale/change/drop/drain observations above.
   Predict the separating observation before each control's run: second-map
   writes test the payload-lock claim, write-on-change tests whether change
   frequency repays a relookup, and buffering tests whether tolerated policy
   loss repays batching. Record a falsifier for each, including negligible
   recovery outside twin spread or a policy-quality failure. A target probe
   must establish scalar code shape, alignment and the chosen no-thin-air
   ordering in the **linked binary**, including LTO-generated instructions
   and helper symbols, alongside `.ll` inspection. Results apply only to
   measured operations, workloads, CPU counts and client topology; missing
   four/eight-core cells preclude a claim covering those counts.

### Hosted queue estimate and staged order

Estimate before reserving the 14900K. The seven requested arms are R0, R1,
L0, L1, current locked stamp, second map and write-on-change. Budget the
current-lock control even if later inspection proves it identical to L1.
The separate actual-main/no-stamp layout control in step 1 is an eighth arm,
also with a twin. Buffering is a separate policy experiment, not silently
included in this throughput budget.

For the full set, the proposed inventory is four server counts (1/2/4/8),
three distributions (uniform/hot/Zipf), two policies (LRU/LFU), and four
operation/depth cells (GET/SET, each at 1/16). Each cell runs ten blocks,
two images per arm, serially on the same server. Thus seven arms require
`7 × 2 × 10 × 4 × 3 × 2 × 4 = 13,440` timed intervals. At an **assumed**
five seconds per interval, that is **18 h 40 min** of measurement alone;
including the eighth arm makes 15,360 intervals and **21 h 20 min**.
Five seconds is a planning assumption, matching the old record's interval,
not evidence that five seconds attains 1% precision here.

Let `t` be the pilot-selected duration, `h` the measured per-interval warmup,
reset and launch overhead, and `B` the total build/setup time, all in seconds.
Full-set runner occupancy is `15,360 × (t + h) + B`, before separate profiles,
eviction-quality runs or corrective retries. For illustration only, `t=5`
and **unverified assumed** `h=5` gives **42 h 40 min + B**; build/setup time
and actual overhead remain unmeasured. Two full-set retries at those same
durations would raise it to **128 h + cumulative build/setup time**. The
one/two/four-core subset is three quarters of the full set (16 h measurement
alone with eight arms at five seconds); the eight-core panel also needs the
separate CI client host and its occupancy. No full set is queued on these
assumptions: first measure `t`, `h` and build/setup samples, then publish
the revised total including the maximum retry cost and coordinate its slots.

Use this order, with each decision batch completed before judging it:

1. **Can the noise rule be met?** Start with LRU GET depth 16 on one-core
   uniform and four-core hot-key cells: two one-second R0/twin pilot pairs
   per cell (eight seconds timed in total), then ten R0/twin pairs at the
   selected duration. At five seconds this calibration is 3 min 20 s,
   excluding overhead. Stop under the finite retry rule if it cannot pass.
2. **Does the existing path explain a recoverable cost?** On those same two
   cells, run R0/L0, current locked stamp, second map, write-on-change and
   actual-main/no-stamp, all with twins and ten blocks. At five seconds this
   is 20 min timed. Inspect route differences and the controls' predicted
   separating observations before proposing the relaxed implementation.
3. **Does the qualified candidate recover it?** After semantic/target
   qualification, run the complete four-arm R0/R1/L0/L1 panel on those two
   cells, with twins and ten blocks: 13 min 20 s timed at five seconds.
   Uniform loss and hot-key route recovery can reject the claim here before
   expanding it. Success covers only these two cells, not LFU or scaling.
4. **Complete the inventory.** Only after those prerequisites, schedule the
   eight-arm matrix in preregistered batches: remaining depth-16 uniform/hot
   cells and LFU first, then Zipf, GET depth 1 and SET controls. Defer
   eight-core cells until their client topology exists. Rerun the early
   cells with all eight arms in this final matrix; do not pool the earlier
   subset batches into it. At the illustrative five-second duration, the
   pilots, calibration, two subset stages and full matrix total
   **21 h 56 min 48 s of timed intervals**, before overhead, builds, profiles,
   policy-quality measurements or retries. Record incomplete coverage on
   stopping; a small decisive subset permits rejection, not a full pass.

### Embedded qualification and cost experiment

The hosted throughput criterion does not establish suitability for a weak
embedded CPU. Before selecting a platform policy, add an embedded-class
**code-shape probe in hosted CI**; physical hardware is not needed to inspect
lowering. The question is whether native-width load/store remains a small
indivisible access when the ISA lacks RMW, and whether RMW is either declared
with its actual implementation or refused explicitly. Reject a claimed native
load/store mapping that tears or calls a hidden lock; a libcall in a default
Clang probe instead identifies compiler/provider work still needed, never a
reason to exclude that CPU from Whitefoot's intended targets.

The monotonic probes below isolate width/provider behavior and preserve
that comparison; they do not establish the no-thin-air contract. Before
qualifying a WF profile under decision 5's recommended rule, also inspect
load-then-store fragments using its selected acquire/fence mapping, retain
the optimized IR, and count the resulting ordering instructions. A native
monotonic load/store result alone cannot validate the one-instruction claim.

Use a pinned Clang/LLVM with ARM and RISC-V backends. The predictions below
are grounded in **LLVM 21.1.0 source inspection**, not a local probe result;
if the experiment pins another version, inspect its corresponding paths
and prerecord any changed prediction before running it. Compile isolated,
aligned 8/16/32-bit relaxed load/store functions, 32-bit wrapping fetch-add and strong
compare-exchange, and 64-bit load/store boundary cases. Use C atomic builtins
with relaxed ordering and separately explicit LLVM `monotonic` IR, since a
C frontend's combined atomic policy may introduce a library call before
target lowering sees the load/store. Keep source, emitted IR, assembly,
diagnostics, exit codes and helper dependencies. Make the following two
feature settings explicit arms for each target, keeping source and all other
settings fixed. The supported width here is LLVM's backend atomic width,
not the hardware's native load/store width.

| Probe arm | thumbv6m / Cortex-M0+ | RV32IMC without A | Prediction for explicit aligned LLVM atomic IR |
|---|---|---|---|
| Default, no provider feature | No `+atomics-32`; supported width 0 | No `+forced-atomics`; supported width 0 | Even `u32` monotonic load/store expand to `__atomic_load_4` / `__atomic_store_4`; RMW/CAS also need `__atomic_*` calls. This is library dependence, not a source rejection. |
| Provider feature selected | `+atomics-32`; supported width 32 | `+forced-atomics`; supported width 32 | Native 8/16/32-bit monotonic loads/stores; 32-bit fetch-add/CAS use `__sync_*` calls whose implementations the platform must supply. These flags assert provider availability, not new hardware instructions. 64-bit atomic load/store remain library boundary cases. |

The width and helper selection are visible in
[ARMISelLowering.cpp][llvm-arm-atomic-lowering],
[ARM's `atomics-32` declaration][llvm-arm-atomics32], and
[RISCVISelLowering.cpp][llvm-rv-atomic-lowering]. Thumb load/store patterns
map supported accesses to native instructions
([ARMInstrThumb.td][llvm-thumb-atomic-patterns]); the RISC-V
[forced-atomics regression fixture][llvm-rv-forced-tests] explicitly expects
the libcall/native split, including monotonic `i32` loads/stores and RMW
helpers. These sources substantiate gran's recalled backend behavior for
this revision. They do not establish the output of the future CI toolchain
or the correctness/availability of any linked provider.

Keep the C-frontend arm separate: in the inspected Clang 21.1.0 sources,
ARM's inline-width test depends on architecture/Thumb version and RV32's on
the A extension ([ARM.cpp][clang-arm-atomic-width];
[RISCV.h][clang-rv-atomic-width]). Merely adding the backend provider feature
therefore does not establish that a C builtin reaches LLVM as an atomic
instruction; predict that the C arm may retain `__atomic_*` calls in its
emitted IR even when the explicit-IR provider arm uses native load/store.
Inspect that IR first. A frontend libcall is a separately located toolchain
gap, not a falsification of the explicit-IR backend prediction.

The initial C commands remain prospective, not runs performed here:

```sh
clang --target=thumbv6m-none-eabi -mcpu=cortex-m0plus -mthumb -O2 -ffreestanding -S atomic-probe.c -o thumbv6m.s
clang --target=riscv32imc-unknown-none-elf -march=rv32imc -mabi=ilp32 -O2 -ffreestanding -S atomic-probe.c -o rv32imc.s
```

For each C target, retain a default run and a provider-feature run, requesting
`-Xclang -target-feature -Xclang +atomics-32` or
`-Xclang -target-feature -Xclang +forced-atomics`, respectively, and save
`-S -emit-llvm` output too. Driver acceptance and feature propagation are
unverified until the pinned CI run; a rejected/ignored flag is reported, not
silently dropped. The independently authored `atomic-probe.ll` contains
actual atomic IR, not C-generated libcalls, with no conflicting per-function
CPU/features. Its two backend arms are specified directly:

```sh
llc -O2 -mtriple=thumbv6m-none-eabi -mcpu=cortex-m0plus -mattr=-atomics-32 atomic-probe.ll -o thumbv6m-default.s
llc -O2 -mtriple=thumbv6m-none-eabi -mcpu=cortex-m0plus -mattr=+atomics-32 atomic-probe.ll -o thumbv6m-provider.s
llc -O2 -mtriple=riscv32-unknown-none-elf -target-abi=ilp32 -mattr=+m,+c,-a,-forced-atomics atomic-probe.ll -o rv32imc-default.s
llc -O2 -mtriple=riscv32-unknown-none-elf -target-abi=ilp32 -mattr=+m,+c,-a,+forced-atomics atomic-probe.ll -o rv32imc-provider.s
```

These command specifications have not been executed. Native accesses in the
default explicit-IR arm, or a load/store libcall in the provider explicit-IR
arm at a supported width, falsify the predicted mapping and require diagnosis
against the recorded version/features. Do not relabel that result a pass.
For both arms, inspect every called implementation before qualification;
provider-feature code must not mix with mutex-based atomic access to the same
cell ([LLVM's `__sync_*` interoperability requirements][llvm-atomic]).

Record the driver's normalized triple; if that Clang requires the canonical
RISC-V spelling `riscv32-unknown-none-elf`, use it with explicit `-march=rv32imc`
and record that mapping to `riscv32imc-unknown-none-elf`, never silently enable
A. Start with one `u32` load/store pair in both feature arms, record compilation
duration, then expand the width/operation panel. Add M23 and RV32+A as positive RMW controls
with their explicit CPU/ISA settings. In these no-RMW LLVM profiles, predict
the library calls above, not an invented hardware instruction or a successful
source-level refusal. Resolve every called provider before claiming
usable atomics: merely emitting `__atomic_*` is not a refusal and not an
implementation qualification. The provider-feature arm requires inspection
of its entire implementation; do not use ordinary/volatile accesses to conceal
an LLVM atomic-lowering gap.

For an eventual WF target, the same panel must additionally show successful
composition for native-width load/store and source-positioned refusal of
64-bit accesses or unsupported RMW. Enabling an approved single-core C profile
must select the bounded mask/restore sequence; selecting a multicore profile
must refuse that implementation. Check nested/already-masked entry, permitted
ISR access, privilege/security domains and preservation of the mask, with
unmasked accessors excluded by the target/runtime contract. Show the actual
privilege of the mask/operation/restore sequence, including any SVC handler;
a nonfaulting M-profile `CPSID` probe is insufficient. Include an unprivileged
negative control where supported, verify exclusion with a competing permitted
interrupt, and document BASEPRI threshold coverage or RISC-V privilege-level
coverage. U-mode `mstatus` traps and unmasked M-mode handlers cannot be treated
as successful exclusion. Prove the complete event-model mapping, including
hold handoffs and no-thin-air, and inspect the
linked image with its actual providers and LTO settings before qualification.
These are prospective target tests, not assertions about current WF support.

The single-core cost comparison is **instruction count and critical-section
length**, not desktop contention or cache-line scaling. Hold the payload work,
cell layout, target and runtime fixed; compare the read/hold path without a
stamp, that path plus a native relaxed store, the ordinary locked-stamp path,
and a declared atomic RMW separately. Count address setup, helper calls,
barriers, interrupt-mask save/restore and the worst masked path, not just the
memory mnemonic. If the ordinary runtime lock itself reduces to interrupt
exclusion, that is the actual comparator; no spin contention may be invented.
Interrupt interleavings and scheduling can still matter on one core.
The separating prediction is less executed path work for the native store
than for the ordinary locked update; equal or greater work rejects that cost
argument for the chosen runtime. RMW has its own cost and is unnecessary for
firn's stamp. Assembly instruction counts are not measured cycles, energy or
interrupt latency. Later timing requires a CI-controlled embedded board with
CPU, clock, memory wait states and interrupt load fixed, a small timed pilot,
same-source interleaved comparisons and a rebuild twin. No MCU speedup or
real-time bound is claimed until then; the i9-14900K threshold is not an MCU
acceptance condition.

### Semantic and client validation

Semantic validation precedes candidate throughput: the specified lost-update,
multi-target and IRIW outcomes; forbidden coherence regressions, thin-air
cycles and stale loads after reader-to-writer handoff; rejection of racing
raw/atomic access, unqualified aggregate snapshots and invalid guards/proofs;
ordinary-payload stability under replacement/deletion/resize;
cross-module helpers and inspection that their
reference parameters with inline relaxed leaves carry no `noalias`; wrapping
boundary and no-lost-update RMW; aliasing targets, multi-target ordinary invariants,
and aggregate movement. For decision 9's recommended rule, check refusal of
direct cell replacement through a reference even when unpublished or held
exclusively, and permitted replacement on an owned binding under its ordinary
ownership obligations. Include missing-key, nested-map and forced whole-hold
fallback cases, and inspect matching/copies for plain loads or memory
intrinsics overlapping a relaxed leaf. An independent event model should be
the oracle, with runtime sanitizers/stress as additional implementation
evidence, not a proof of the memory model.
Negative cases must fail for the intended rule. Native CI must qualify the
declared target/feature/runtime combinations before coverage of those profiles
is claimed; the five hosted ABI records are not the universe of future targets.
Unsupported operation/width cases are composition diagnostics, not weakened
source semantics or backend fallback tests that merely happen to link.
Formal cases would live under conformance/program/runtime ownership when the
owner selects a design; this research document is not a gate dependency.

For firn, separately validate policy behavior and the effect of lost LFU
updates: the existing [memory-limit investigation's eviction-quality
criterion][firn-evidence] uses a Zipf workload and rejects a hit-rate deficit
over two percentage points against Redis 7.0.15. Also inspect restoration,
policy changes, introspection and script rollback obligations. Its
[step-1 record][firn-step1] and [access implementation][firn-access] are
consumers to adapt, not evidence that relaxed interleavings already meet them.

## Decisions still needed from the owner

The discussion above answers the relationship/platform question before asking
for a shape. These are open research decisions, not approvals or spec edits.

1. **Settled: “Store<u32>” meant `Shared<u32>`.** The owner confirmed it on
   the board (2026-10-09). What remains open from that comment is decision 3
   below together with this section's comparison: the field gives single
   indivisible accesses inside its owner without a lock, while `Shared<u32>`
   keeps whole-block transactions, guards and joint commits with other
   targets; the investigation recommends keeping both, with S2 as an optional
   optimization of single-operation `Shared<integer>` statements.
2. **May explicitly designated scalar accesses interleave inside otherwise
   transactional blocks?** Recommend evaluating the proposed split model,
   including per-cell coherence, hold handoffs and the permitted litmus
   outcomes, while retaining one point for ordinary state. Reversal of the
   refusal needs an explicit owner-accepted tradeoff after the same-source
   controls; the 9–14% signal alone does not supply it. Alternatively retain
   one point for every field. Confidence 3/5: the semantic change is explicit,
   but the mixed event/proof model is not yet proved. Direction A authorizes
   investigation, not its detailed rules.
3. **Should the scalar be an inline cell type, a field modifier, or a
   separately lived owned handle?**

   **Background.** Type/modifier spelling and allocation/lifetime are distinct
   choices. An inline `Relaxed<u32>` is already an owned value; a
   `RelaxedCell<u32>` handle like Shared would point to separate shared state.
   The [owned-handle comparison](#3-two-meanings-of-an-owned-relaxed-cell)
   derives their layout, lifetime and ordering costs from SHARE-1/2 and the
   current runtime. Equal pointer arguments do not establish equal whole-path
   code or memory use.

   **Options.**

   **A (recommended): S1, an explicit inline scalar cell type, paired with
   S1-W in decision 4.** Its declared type carries the discipline through
   helpers and arrays, and its lifetime follows the entry; the hold scopes
   access, without a second allocation/count. It still needs the mixed-event,
   alias, proof and PAR work; no implementation or performance result is claimed.

   **B: S3, a field modifier.** Keeps the scalar inline and could have the same
   storage cost, but needs a modular projection rule that retains the field's
   discipline at helper boundaries. Viable in principle; that rule remains
   unverified, so it is not preferred over A.

   **C: S1-H, a separately allocated, reference-counted relaxed handle.**
   Viable as a research candidate conditional on decision 4's new nonwaiting
   interference contract. It supplies independent lifetime and isolates
   shared scalar storage from ordinary inline alias facts, but adds an
   allocation, indirection, explicit-sharing/release traffic and a larger
   per-key representation. Its compact layout and cache benefit are
   unverified; literal reuse of Shared's allocator is particularly costly.
   Recommend reconsidering C if independent lifetime is needed or a matched
   experiment establishes a locality/alias benefit worth those costs.

   S1-I (inline cell with hidden content) is A's representation combined with
   decision 4's alternative C, not evidence that A must allocate. S2 remains
   a separate optimization of transactional Shared; S4 still does not meet
   the selected field capability.

   **Confidence 3/5.** Source inspection establishes the representation and
   boundary distinctions; the relative performance and complete semantics of
   the new handle are unverified. A need for independent lifetime, a matched
   experiment showing a handle's locality/alias benefit outweighs its costs,
   or a demonstrated modular advantage for a field modifier would overturn
   the recommendation.

4. **How should a callable expose relaxed access?**

   **Background.** EFF-1/2 describe reference-rooted ordinary state at a
   callable boundary; SHARE-2 removes target-state paths while retaining
   handle/index reads and other effects. Waiting and SHARE-3 order supply the
   intended complementary boundary, but EFF-3's transformation licence
   conflicts with SHARE-3/WAIT-2 without stated precedence; board item
   `proof-bl-eff3-pure-waits` tracks that defect. The
   [effect-row answer](#1-what-rows-guarantee-today) recommends clarifying it,
   not redefining reads as arbitrary mutation or claiming rows describe all
   memory. Inside atomic blocks relaxed operations
   cannot wait; the store/store and load/load counterexamples therefore still
   require an explicit nonwaiting interference rule.

   **Options.**

   **A (recommended): S1-W.** Preserve `reads` for load and `writes` for
   store/RMW on a Relaxed-typed path; select holds by declared type/path.
   Retain conservative PAR conflicts including loads, qualify both OWN-9
   consequences, suppress unsound inline `noalias`/aggregate-copy facts, and
   keep mutable contents out of stable proof terms. The row continues to show
   the changed leaf even though that type permits concurrent scalar access.
   Signature/generic coverage remains to be proved.

   **B: S1-A, a third explicit path category.** Can state a distinct atomic
   interference contract, but needs complete exactness, substitution, alias,
   proof-kill and ordering rules. It reopens CAP-1, EFF-1 and the reads/writes
   design decision; no modular advantage over A has yet been demonstrated.

   **C: hidden cell state with a new declared nonwaiting interference
   contract.** S1-H records handle-slot access, excluding separately shared
   state; S1-I additionally separates inline identity from mutable content.
   A transitive summary must prevent PAR overlap and invalid EFF-3
   transformations through every helper, including those whose handle
   accesses frame out. Handle-path writes alone cannot detect two handles
   naming one object and cannot be padded into a merely read handle's row
   under the compiler's intended exactness (the EFF-2 wording defect is noted
   in the [owned-handle comparison](#3-two-meanings-of-an-owned-relaxed-cell)).
   C is a conditional alternative, not an existing Shared permission;
   its full checking/alias algebra is unverified. Not recommended for the
   inline hint because it hides the useful write without eliminating the
   ordering obligation; S1-I retains all inline alias costs too.

   **Refused: unchanged S1-R**, stores recorded as ordinary reads with no new
   content or interference boundary. Shared is not its precedent because
   Shared's waiting statements cannot implicitly overlap. A broader
   definition of reads would require C's explicit redesign rather than
   making the counterexample disappear.

   **Confidence 3/5.** The current row/Shared distinction and counterexamples
   have rule-level support; none of the extensions has a complete verified
   model. Embedded instruction or interrupt lowering does not select among
   these contracts; operation requirements must compose separately from rows.
   A sound modular model showing S1-W cannot preserve the required ordering
   or generic composition, or that B or C does so with a demonstrated
   interface or optimization advantage, would overturn the recommendation.
5. **Must relaxed values exclude cyclic thin-air justification?**

   **Background.** With `x=y=0`, A does `r=load(x); store(y,r)` and B does
   `s=load(y); store(x,s)`. Each load of 42 can be justified only by the other
   context's store of that loaded value. The proposed acyclic
   program-order/reads-from rule excludes this cycle ([RC11 §3.2][rc11]).
   LLVM `monotonic` supplies neither the needed load-to-later-store order
   across cells nor preservation of false dependencies. The lowering cost
   described above must therefore be settled before decision 6's target/RMW
   policy or any one-instruction promise.

   **Options.**

   **A (recommended): retain the no-invented-value promise** with acyclic
   program-order/reads-from and qualified lowering. Existing LLVM mechanisms
   require an acquire load or suitable fence: AArch64 `LDAPR`/`LDAR`, ARMv7
   `ldr; dmb`, or RISC-V `fence r,w` or stronger. A cheaper preserved-dependency
   route requires a new IR mechanism. x86 TSO needs no extra hardware fence;
   under the stated single-core accessor premises only a compiler barrier
   is needed for this edge. Weakly ordered multicore pays real ordering
   cost, and AArch64's one-instruction acquire still orders more than a plain
   load. This option preserves the promised causality boundary but requires
   the mixed-model proof and per-target qualification.

   **B (not recommended): explicitly permit causally unsupported but
   type-valid scalar values**, with no proof or publication authority. This
   avoids the extra ordering imposed solely by A's acyclicity rule; coherence
   and hold-handoff obligations remain. Its risk is weakening the stated
   scalar behavior even when no initialization or independent computation
   supplied a value. Keeping pointers out does not settle that tradeoff.

   **Confidence 3/5.** The cycle and LLVM ordering/optimization gap are
   supported by the cited model and source inspection. Whitefoot's full mixed
   model, selected emitted sequences and measured weak-target cost remain
   unverified; a validated cheaper mapping could change the cost assessment.

6. **Which target-capability model should govern scalar operations, and may
   a single-core target declare interrupt-masked RMW?** Subject to decision
   5's causal-model choice and ordering cost, the owner's embedded
   target requirement rules out an intersection bounded by today's five ABIs.
   M0/M0+ and RV32IMC provide native 32-bit load/store without hardware RMW;
   `u64` access on those cores can tear. Recommend P1: deterministic target
   declarations, native-width load/store as the minimum, and independent
   fetch-add/CAS capabilities checked at composition. Missing widths or
   operations are refused explicitly, with no hidden lock. Within P1:
   **A (recommended)** admits qualified exclusive/CAS sequences and bounded
   interrupt exclusion declared by a single-core runtime; this preserves RMW
   availability on weak CPUs but requires mask, privilege, accessor and
   latency qualification. **B** permits only hardware-backed RMW, including
   qualified loops; this avoids interrupt-masking costs but leaves RMW
   unavailable on M0/M0+ and RV32IMC without excluding their load/store use.
   **C** defers all RMW and starts with native-width load/store; that suffices
   for firn's stamp but postpones counters needing indivisible updates.
   Recommend no universal single-instruction RMW requirement; keep wider
   emulation, pointer-bearing cells, 128-bit types and floating RMW outside
   this first decision. Confidence 4/5 on the architecture distinction,
   2/5 on WF target/runtime qualification: there is no embedded port or fixed
   CPU-feature contract today. The embedded CI code-shape probe and a port's
   context/interrupt contract could change the implementation recommendation;
   desktop throughput alone cannot settle it. Native width alone cannot
   establish a promise of one plain instruction per load/store under decision 5.
7. **Which nontransactional outcomes may firn accept for the stamp?**
   Recommend load/store LRU and an explicit evaluation of LFU lost updates,
   stale/regressing clocks, script rollback and introspection; choose CAS
   only if the desired accuracy/behavior requires it. Alternatively retain
   exact transaction semantics for those consumers. Confidence 2/5: Redis's
   algorithm and current firn paths are known, but concurrent behavior and
   eviction quality under the new semantics have not been measured.
8. **May relaxed cells be used as wait conditions or publication flags?**
   Recommend no in the first design: use existing guarded Shared state for
   synchronization, and specify snapshots only for scalar hints. A broader
   choice must add a visibility/progress/publication model and proofs, plus
   notifications on read-hold completion for any admitted wait conditions.
   Confidence 3/5: the safety boundary is clear; useful reliable polling may
   justify more later.
9. **Where is direct whole-cell replacement of an inline cell allowed?**

   This card concerns S1's inline storage (including S1-I if considered).
   S1-H instead replaces a handle slot under ordinary writes/release rules;
   it changes the shared scalar only through its proposed scalar operations,
   as distinguished in the owned-handle comparison above.

   **Background.** A callee with `cell: &Relaxed<T>` cannot determine from
   that signature whether the cell is published. A rule forbidding replacement
   only after publication therefore supplies no modular refusal for
   `set cell^ = ...` (a schematic proposed replacement, not current syntax
   for an implemented type). `writes(cell)` must not authorize a plain write
   racing another reader. The rule needs a statically visible boundary.

   **Options.**

   **A (recommended): forbid direct whole-cell replacement through any
   reference; allow it only on an owned binding**, subject to ordinary
   ownership/exclusion rules. A callee decides from the source access root alone;
   even an unpublished or exclusively held reference uses explicit atomic
   operations. This costs some replacement convenience but gives the signature
   a uniform contract without publication tracking. Enclosing-owner
   replacement remains exclusive and quiescent, preserving cell lifetime
   and representation; it cannot bypass exclusion under a read hold.

   **B (not recommended): define whole-cell replacement through a reference
   as one atomic store of T.** This makes replacement notation usable for
   shared cells without a publication test, but requires a precise payload
   extraction/consumption rule for the opaque noncopyable replacement value
   and its operation capability/effect checks. It duplicates the explicit
   store and must never lower to a plain cell/aggregate copy. No such rule
   or lowering has been validated here.

   **Confidence 3/5.** A's root-kind test resolves the specific modularity
   ambiguity without knowing publication state; the complete type/placement,
   ownership and aggregate-replacement rules remain unverified. A demonstrated
   need for B together with a sound replacement rule could reopen the choice.

## Owner rulings (2026-10-09)

On the shared status board on 2026-10-09 the owner chose option A on seven of
the decision cards above; decision 4 (how a callable exposes relaxed access)
remains open, with the owner asking why loads of one cell conflict (answered
on the card: a cell another context can change keeps per-cell coherence only
if two loads keep source order).

- **Decision 2 (`firn-rf-split`), A:** run the same-source controls first --
  stamps in a second map keyed the same way, and a separate write only when
  the stamp changes, against the locked stamp and no stamp -- and accept the
  semantic change only if they do not recover the cost, together with the
  mixed event model's proof.
- **Decision 3 (`firn-rf-shape`), A:** an inline `Relaxed<T>` type (S1),
  paired with S1-W, not a field modifier or a separately allocated handle.
- **Decision 5 (`firn-rf-thinair`), A:** values are never invented; the
  acyclic program-order/reads-from rule holds, with its ordering cost on weakly
  ordered multicore.
- **Decision 6 (`firn-rf-targets`), A:** explicit per-target capabilities,
  native-width aligned load/store as the minimum, read-modify-write by
  capability and refused at composition when absent, never a hidden lock;
  bounded interrupt masking admissible on a single-core runtime that proves
  privilege, mask coverage and latency. The owner noted this needs care when
  Whitefoot lowers to MCUs; the board item `firn-wf-mcu-atomics` records it.
- **Decision 7 (`firn-rf-firn`), A:** load/store LRU and LFU first, LFU's lost
  increments judged by the predeclared Zipf hit-rate criterion.
- **Decision 8 (`firn-rf-wait`), A:** no wait conditions or publication flags
  on relaxed cells in the first design. The owner's ground for refusing B:
  Whitefoot promises freedom from deadlock, and the atomic statement's lock
  ordering exists so that two contexts never wait in opposite orders; allowing
  waits on relaxed cells would reopen exactly that, since two waits in
  different orders could deadlock.
- **Decision 9 (`firn-rf-replace`), A:** no whole replacement, swap included,
  through any reference; only an owned binding may replace the cell. The
  owner asked whether this is another special case. It is a new rule that the
  current rules do not imply (OP-11 lets any noncopy value be exchanged, and
  opaque, private fields and nodrop do not prevent it); it is needed because
  S1-W's rows cannot distinguish an atomic store from an ordinary whole
  write. The specification should state it as a general type property, values
  of the type change through references only by the type's own operations,
  with `Relaxed<T>` its first user.

## What is established and what remains unverified

Established by source inspection: the current rule conflict; the absence of
a generic `Store`; the existing read/write route and its lifetime protocol;
five admitted target ABIs versus narrower routine CI coverage; the earlier
atomic-field and retry refusals; and the difference between scalar atomicity
and a Shared transaction. The Firn-wf numbers above are verified as a faithful
transcription of its pinned research record, not independently reproduced.
The source review also establishes the alias-attribute and aggregate-access
obligations, stable node addresses and direct slot access, the read release's
lack of guard notification, the absence of fixed CPU-feature settings,
the need for relaxed load/load conflicts, and the ENT-2/TYPE-11/guard exclusion
points. The event model and control protocol are proposals, not results.
The thin-air cost analysis now distinguishes LLVM's missing ordering edge
and false-dependency preservation from target hardware cost. Its acquire/fence
examples are source-grounded mapping candidates, not measured Whitefoot code.
The static owned-root replacement rule is a proposal, and the alias audit
distinguishes inline relaxed storage from pointers loaded out of a handle.
The owner's effect-row question now has a checkout-grounded answer: SHARE-2
explicitly removes held-state paths from the outer footprint, and waiting
plus SHARE-3 order supplies the intended complementary interference boundary,
subject to the unresolved conflict with EFF-3's transformation licence and
its lack of stated precedence over SHARE-3/WAIT-2
(`proof-bl-eff3-pure-waits`). The historical refusal of spawns borrowing their
starter's state is recovered separately from the refusal of implicit
transaction scopes and atomic fields.
The new handle comparison establishes current Shared allocation/count code
and gives conditional per-million-key component arithmetic; it does not
measure a compact relaxed handle, actual firn layout/RSS or cache behavior.
Architecture and runtime sources establish why embedded native load/store,
hardware RMW and single-core interrupt exclusion must be separated. The
owner's requirement preserves weak embedded CPUs in the intended target scope;
the proposed capability declarations and conditional C provider are not yet
approved rules or implemented target support.

Unverified: the complete relaxed
execution model and progress guarantee; the choice and soundness of effect,
proof and modular projection rules; actual code generation and layout for
each declared operation/target/feature/runtime profile; bounded exclusion's
admissibility and an embedded port's context/interrupt contract; S2's mixed-use
implementation; any general safe upgrade/replay protocol; firn's LFU quality,
rollback and observable compatibility under relaxed updates; and the depth-16
performance criterion and embedded path/interrupt cost. No acceptance,
performance success or implementation completion is claimed for any candidate.
S1-H/S1-I's hidden-state summaries, EFF-3 transformation treatment, complete
alias mappings and initialization/lifetime handoffs also remain unverified.
Next calibrate the hosted noise/duration and queue estimate, then run the
staged existing-language same-source controls and two-arm embedded CI code-shape
probe, and evaluate the proposed event model and target qualification plan for
the owner's semantic choices, before changing language rules. No build, test
suite or benchmark was run for this documentation revision. The local Clang
`-###` query only inspected driver defaults; it compiled and linked nothing.
Effective LTO CPU/features and Linux outlined-atomic dependencies
remain part of target qualification, not conclusions of that query. The
embedded probe above is a protocol only; it has not been compiled or run for
this documentation revision. Its LLVM 21.1.0 backend predictions and privilege
constraints are sourced; actual feature propagation, linked providers,
Xtensa default atomic lowering and all embedded code/latency results remain
unverified. The hosted time totals are arithmetic on explicit planning
assumptions, not measured runner occupancy or evidence of attainable noise.

## Sources

Whitefoot file:line citations refer to the revision at the top; relative links
are for navigation and may move on later main revisions. Firn-wf links are
pinned to the read revision. External architecture/compiler sources were
consulted for this investigation; their general capabilities are not native
Whitefoot qualification results.

[spec]: ../../../spec/kernel-spec.md
[constitution]: ../../../docs/constitution.md
[research-method]: ../../README.md
[language-node]: ../../../design/language.md
[data-model]: ../../../design/language/data-model.md
[effects-node]: ../../../design/language/effects.md
[parallel-node]: ../../../design/language/parallelism.md
[shared-node]: ../../../design/language/waiting/shared-objects.md
[keyed-node]: ../../../design/language/waiting/shared-objects/keyed-tables.md
[readonly-node]: ../../../design/language/data-model/readonly-field.md
[shared-research]: ../io-model/SHARED.md
[map-research]: ../concurrent-map/DESIGN.md
[access-options]: ../access-effects/OPTIONS.md
[access-research]: ../access-effects/RESEARCH.md
[access-verdict]: ../access-effects/VERDICT-CORE.md
[access-design]: ../access-effects/DESIGN.md
[access-mechanisms]: ../access-effects/MECHANISM-MAP.md
[shared-state-research]: ../shared-state/DESIGN.md
[concurrency-research]: ../io-model/CONCURRENCY-MODEL.md
[waiting-backend]: ../../../design/compiler/waiting-contexts.md
[shared-runtime]: ../../../compiler/src/backend/completion/bridge.c
[shared-bridge]: ../../../compiler/src/backend/completion/bridge.h
[atomic-check]: ../../../compiler/src/semantic/check/control/atomic.rs
[atomic-lower]: ../../../compiler/src/lowering/builder/atomic.rs
[storage-lower]: ../../../compiler/src/lowering/builder/storage.rs
[cmap]: ../../../compiler/src/backend/concurrent_map.c
[cmap-node]: ../../../design/compiler/waiting-contexts/concurrent-map.md
[cmap-header]: ../../../compiler/src/backend/concurrent_map.h
[keyed-runtime]: ../../../compiler/src/backend/keyed_table.c
[shared-emitter]: ../../../compiler/src/backend/emitter/shared.rs
[operations-emitter]: ../../../compiler/src/backend/emitter/operations.rs
[places-emitter]: ../../../compiler/src/backend/emitter/places.rs
[targets]: ../../../compiler/src/target.rs
[emitter]: ../../../compiler/src/backend/emitter.rs
[backend-facts]: ../../../design/compiler/backend-facts.md
[toolchain]: ../../../compiler/src/toolchain.rs
[driver]: ../../../compiler/src/driver.rs
[runtime-make]: ../../../compiler/runtime.mk
[native-driver]: ../../../compiler/src/bin/whitefootc.rs
[gate]: ../../../.github/workflows/gate.yml
[io-ci]: ../../../.github/workflows/io-hosts.yml
[release-ci]: ../../../.github/workflows/compiler-release.yml
[firn-evidence]: https://github.com/Ming-Research/Firn-wf/blob/bbd53a2e3007eac0dafd03c6dd150ff301accaf1/research/investigations/memory-limit/README.md
[firn-access]: https://github.com/Ming-Research/Firn-wf/blob/bbd53a2e3007eac0dafd03c6dd150ff301accaf1/firn/commands/access.wf
[firn-step1]: https://github.com/Ming-Research/Firn-wf/blob/bbd53a2e3007eac0dafd03c6dd150ff301accaf1/research/investigations/memory-limit/step-1.md
[firn-head-get]: https://github.com/Ming-Research/Firn-wf/blob/66ff776e7cb8ce7b3f1035626579f26fe4580377/firn/commands/strings.wf#L204-L230
[firn-base-get]: https://github.com/Ming-Research/Firn-wf/blob/6585531b7f884ef3089f6909d3998e0e69facc1b/firn/commands/strings.wf#L204-L229
[firn-head-entry]: https://github.com/Ming-Research/Firn-wf/blob/66ff776e7cb8ce7b3f1035626579f26fe4580377/firn/store/module.wfm#L33-L38
[firn-base-entry]: https://github.com/Ming-Research/Firn-wf/blob/6585531b7f884ef3089f6909d3998e0e69facc1b/firn/store/module.wfm#L33-L37
[firn-run]: https://github.com/Ming-Research/Firn-wf/actions/runs/37888136395
[firn-profile]: https://github.com/Ming-Research/Firn-wf/actions/runs/37889202069
[redis-db]: https://github.com/redis/redis/blob/7.0.15/src/db.c#L50-L120
[redis-object]: https://github.com/redis/redis/blob/7.0.15/src/server.h#L847-L859
[redis-evict]: https://github.com/redis/redis/blob/7.0.15/src/evict.c#L280-L306
[caffeine]: https://github.com/ben-manes/caffeine/wiki/Design#read-buffer
[c11]: https://www.open-std.org/jtc1/sc22/wg14/www/docs/n1570.pdf
[llvm-order]: https://llvm.org/docs/LangRef.html#atomic-memory-ordering-constraints
[llvm-noalias]: https://llvm.org/docs/LangRef.html#noalias
[llvm-based]: https://llvm.org/docs/LangRef.html#pointer-aliasing-rules
[llvm-fold-branch]: https://github.com/llvm/llvm-project/blob/llvmorg-21.1.0/llvm/lib/Transforms/Utils/Local.cpp#L134-L167
[llvm-memory]: https://llvm.org/docs/LangRef.html#memory-model-for-concurrent-operations
[llvm-load]: https://llvm.org/docs/LangRef.html#load-instruction
[llvm-store]: https://llvm.org/docs/LangRef.html#store-instruction
[llvm-lto]: https://llvm.org/docs/LinkTimeOptimization.html
[llvm-arm-alias]: https://github.com/llvm/llvm-project/blob/llvmorg-21.1.0/llvm/lib/Target/AArch64/AArch64Processors.td#L1211-L1213
[llvm-arm-cpus]: https://github.com/llvm/llvm-project/blob/llvmorg-21.1.0/llvm/lib/Target/AArch64/AArch64Processors.td#L872-L881
[llvm-arm-features]: https://github.com/llvm/llvm-project/blob/llvmorg-21.1.0/llvm/lib/Target/AArch64/AArch64Features.td#L783-L791
[llvm-x86-cpus]: https://github.com/llvm/llvm-project/blob/llvmorg-21.1.0/llvm/lib/Target/X86/X86.td#L1628-L1645
[llvm-x86-baseline]: https://github.com/llvm/llvm-project/blob/llvmorg-21.1.0/llvm/lib/Target/X86/X86.td#L753-L758
[rc11]: https://people.mpi-sws.org/~dreyer/papers/scfix/paper.pdf
[llvm-atomic]: https://llvm.org/docs/Atomics.html#atomics-and-codegen
[intel-atomic]: https://cdrdv2-public.intel.com/835754/253668-sdm-vol-3a.pdf
[arm-atomic]: https://documentation-service.arm.com/static/68c223238a337a2bc6645c0a
[arm-lse]: https://developer.arm.com/community/arm-community-blogs/b/tools-software-ides-blog/posts/making-the-most-of-the-arm-architecture-in-gcc-10
[arm-order]: https://developer.arm.com/community/arm-community-blogs/b/tools-software-ides-blog/posts/armv8-sequential-consistency
[arm-rcpc]: https://developer.arm.com/community/arm-community-blogs/b/tools-software-ides-blog/posts/enabling-rcpc-in-gcc-and-llvm
[gcc-atomic]: https://gcc.gnu.org/onlinedocs/gcc/_005f_005fatomic-Builtins.html
[clang-atomic]: https://clang.llvm.org/doxygen/stdatomic_8h_source.html
[arm-v6m]: https://documentation-service.arm.com/static/5f8ff05ef86e16515cdbf826
[arm-v7m]: https://documentation-service.arm.com/static/5f8fedcbf86e16515cdbf30f
[arm-v8m]: https://documentation-service.arm.com/static/5f8efff7f86e16515cdbe5f9
[arm-m23]: https://documentation-service.arm.com/static/5eff4394cafe527e86f5b3c4?token=
[rv32]: https://docs.riscv.org/reference/isa/v20240411/unpriv/rv32.html
[riscv-a]: https://docs.riscv.org/reference/isa/v20240411/unpriv/a-st-ext.html
[xtensa-isa]: https://www.cadence.com/content/dam/cadence-www/global/en_US/documents/tools/silicon-solutions/compute-ip/isa-summary.pdf
[esp32-isa]: https://github.com/espressif/esp-idf/blob/master/components/xtensa/esp32/include/xtensa/config/core-isa.h
[avr-atomic]: https://avrdudes.github.io/avr-libc/avr-libc-user-manual/group__util__atomic.html
[portable-atomic]: https://github.com/taiki-e/portable-atomic#optional-features
[critical-section]: https://github.com/rust-embedded/critical-section
[freertos-atomic]: https://github.com/FreeRTOS/FreeRTOS-Kernel/blob/main/include/atomic.h
[zephyr-atomic-c]: https://docs.zephyrproject.org/latest/doxygen/html/atomic_8h_source.html
[zephyr-spinlocks]: https://docs.zephyrproject.org/latest/kernel/services/synchronization/spinlocks.html
[llvm-forced-atomics]: https://reviews.llvm.org/D130621
[llvm-arm-atomic-lowering]: https://github.com/llvm/llvm-project/blob/llvmorg-21.1.0/llvm/lib/Target/ARM/ARMISelLowering.cpp#L1287-L1366
[llvm-arm-atomics32]: https://github.com/llvm/llvm-project/blob/llvmorg-21.1.0/llvm/lib/Target/ARM/ARMFeatures.td#L570-L577
[llvm-thumb-atomic-patterns]: https://github.com/llvm/llvm-project/blob/llvmorg-21.1.0/llvm/lib/Target/ARM/ARMInstrThumb.td#L1700-L1723
[llvm-rv-atomic-lowering]: https://github.com/llvm/llvm-project/blob/llvmorg-21.1.0/llvm/lib/Target/RISCV/RISCVISelLowering.cpp#L691-L701
[llvm-rv-forced-tests]: https://github.com/llvm/llvm-project/blob/llvmorg-21.1.0/llvm/test/CodeGen/RISCV/forced-atomics.ll
[clang-arm-atomic-width]: https://github.com/llvm/llvm-project/blob/llvmorg-21.1.0/clang/lib/Basic/Targets/ARM.cpp#L137-L154
[clang-rv-atomic-width]: https://github.com/llvm/llvm-project/blob/llvmorg-21.1.0/clang/lib/Basic/Targets/RISCV.h#L195-L200
[riscv-csrs]: https://docs.riscv.org/reference/isa/v20240411/priv/priv-csrs.html
[riscv-privilege]: https://docs.riscv.org/reference/isa/v20240411/priv/machine.html
[rp2040]: https://www.raspberrypi.com/documentation/microcontrollers/pico-series.html
[rp2040-datasheet]: https://datasheets.raspberrypi.com/rp2040/rp2040-datasheet.pdf
[14900k]: https://www.intel.com/content/www/us/en/products/sku/236773/intel-core-i9-processor-14900k-36m-cache-up-to-6-00-ghz/specifications.html
