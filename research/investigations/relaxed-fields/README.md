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
a proposal checked against Whitefoot `fd9c080b98f2e4b4a73e9a6780a8ed31867a2e92`, branch
`claude/relaxed-fields`, active specification v0.108. It changes no language
rule or implementation. No new compilation, concurrency test or performance
measurement has been run for it.

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
The proposed depth-16 GET criterion on the i9-14900K requires loss within
no-stamp twin noise, with a fixed 1% noise ceiling; a 3% residual loss is not
acceptable. Excess noise means inconclusive, never a larger allowance.
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
| IRIW: `x=y=0`; writers store `x=1` and `y=1`; reader C loads x then y, reader D loads y then x | C sees `(1,0)` and D sees `(1,0)`. Each cell is coherent, but the readers disagree on cross-cell order; there is no single order of whole blocks producing both observations. This is permission, not a promise that each backend exhibits it. |

**No thin-air values: retain the promise, add its missing condition.** Merely
requiring a read to have a store source does not prevent cyclic justification:
with `x=y=0`, A does `r=load(x); store(y,r)`, B does
`s=load(y); store(x,s)`. Assigning 42 to both loads lets each store justify the
other load without initialization or an independent computation supplying 42.
Recommend an RC11-style requirement that the union of program-order and
reads-from edges is acyclic ([RC11 §3.2, Definition 1][rc11]). This excludes
that execution while permitting the three litmus outcomes above. Neither the
words “C11 relaxed” nor the LLVM ordering name alone are the Whitefoot proof.

The cost is real: preserving load-to-later-store order on weak targets can
require dependencies or barriers beyond plain scalar instructions. The RC11
paper proves stronger mappings for Power/ARMv7 (§§5–6); it is not an AArch64
or general embedded-target qualification. Inspect each selected mapping,
including orders already supplied by holds, before promising one instruction
per operation. An alternative is
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
| Xtensa LX6 in the original ESP32 | 8, 16, 32 in suitable data RAM | Optional Xtensa `S32C1I` provides 32-bit compare/conditional-store; ESP32 declares it present. Fetch-add can use a CAS loop. | No generic 64-bit guarantee. Qualify the exact core configuration, memory region and atomic-control settings; do not generalize across all ESP32-branded chips. Ordinary memory ordering is weaker than x86; `S32C1I` itself imposes stronger order than monotonic needs. [Cadence ISA §§3.4, 3.8.1–3, 4.3.13][xtensa-isa], [ESP32 configuration][esp32-isa]. |
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
handoff rules. Those may require compiler constraints, dependencies or
barriers on a weak target and remain part of qualification. A single-copy
atomic access is not by itself a proof of the complete event model.

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

**Recommendation, awaiting the owner:** permit this as a declared
single-core RMW implementation, subject to all of these conditions:

* The target/runtime contract restricts all possible cell accessors to one
  executing core/hart. It states privilege, mask coverage, preemption and
  interrupt-entry rules. NMI, faults, higher-priority or secure-world handlers
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

Whitefoot's embedded runtime model is undefined today. This decision needs
the above target/runtime premises and their enforcement, not a choice of
embedded scheduler, interrupt API or complete port. A cooperative runtime
with no interrupt access might need less exclusion; that cannot be assumed
from the ISA or from running a desktop workload on one CPU. Source contexts
still have their in-order meaning, and cell observations still lack proof
authority. Do not infer permission for an ISR to enter today's waiting
`atomic` statement or reuse the desktop map/hold runtime on an MCU. If the
owner admits interrupt exclusion, each port must establish its context,
interrupt, lifetime and hold-handoff contract before declaring the capability;
until then its RMW entry is unavailable.

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
`store atomic` and, if offered, `atomicrmw`, all with `monotonic` ordering and
qualified alignment. LLVM requires explicit `align` on atomic
[loads][llvm-load] and [stores][llvm-store]; the no-thin-air obligation may require
additional ordering beyond those monotonic operations.

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
| ESP32 Xtensa LX6 with qualified RAM and S32C1I | 8,16,32 | 32 / E | 32 / I |
| AVR native minimum, without an approved exclusion provider | 8 | empty | empty |

For example, a future M0+ target would declare `thumbv6m-none-eabi`,
`cortex-m0plus`, 8/16/32-bit load/store with 1/2/4-byte alignment in specified
RAM, and empty RMW sets. An explicitly selected single-core runtime profile
could instead declare C for those RMW widths after the owner accepts that
implementation class and the port establishes its premises. An RV32IMC
profile similarly fixes `rv32imc` and its ABI, with no A extension; a separate
RV32+A profile declares the additional operations. The ISA does not establish
the number of cores. Wider software-emulated loads/stores are outside this
initial native-width rule even when interrupt exclusion could implement them;
they would need a separate declared capability and decision, not a fallback.

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
and no ordinary `set` into its representation. Recommend refusing direct
whole-cell replacement after publication: `writes(cell)` at `&Relaxed<T>`
then permits the atomic operations, not a hidden plain replacement. The
alternative is to define such replacement as exactly one atomic store of T;
leaving it an ordinary write while allowing a read hold is unsound. Replacing
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

**Effects are the hard part.** S1-R is rejected, leaving two candidates:

S1-R would label stores as `reads(cell)`. EFF-1 defines observation and
mutation separately ([spec/kernel-spec.md:1562–1564][spec]); EFF-2 checks the
body's accesses ([spec/kernel-spec.md:1566–1568][spec]); OWN-9 says read-only
call storage stays read-only ([spec/kernel-spec.md:734][spec]). A direct
counterexample is two nonwaiting calls inside one block:

```wf
relaxed_store::<u32>(cell: &c, value: 1_u32);
relaxed_store::<u32>(cell: &c, value: 2_u32);
```

If both report reads, PAR-1's read/read permission can overlap them and leave
1, whereas source order leaves 2 (PAR-1,
[spec/kernel-spec.md:2146–2157][spec]). PAR-2 forms the same footprints across
iterations ([spec/kernel-spec.md:2168–2171][spec]), so relabeling stores also
misclassifies a loop writing that cell. Replacing the meaning of reads and
the interference rules would be a different proposal, not a viable S1-R.

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
explanation of the old loss. All shapes admitting mutation under read holds
(S1-W, S1-A and S3) need the same alias and stability audit:

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
  referent can contain a relaxed leaf, including aggregates and range
  elements; carry this structural property through generic/exported
  interfaces. A reference to a separately proved ordinary subfield may keep
  its justified attributes. Audit inlining metadata and all derived aliases.
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

1. **Before a language change**, pin Whitefoot, Firn-wf, Halo-wf, Redis benchmark
   version, Clang/linker versions, flags and one identical LTO mode for all
   arms. Use one firn source with only the named experimental factors varied;
   keep entry layout fixed. Also compare actual firn main without stamping
   to expose total layout cost. Run R0/L0, the current locked-stamp control,
   and the second-map and separate write-on-change controls first; only after
   semantic/target qualification add R1/L1 and complete the panel. Report the
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
   uniform and hot-key workloads at depth 16; retain depth 1 and SET controls.
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
   Begin with two one-second no-stamp/twin pairs as a small timing/noise pilot.
   Use that spread to choose and record the final run duration before timing
   the candidate. Exclude the pilot from the decision batch. Fix **ten paired
   blocks per arm and workload cell**, each containing all arms and twins
   with balanced ordering; do not stop early or add pairs after seeing a
   favorable candidate result.
4. Fix the estimator and limits now. In each cell, epsilon is the median of
   `abs(R0_twin/R0 - 1)` over the ten blocks; loss is the larger of
   `1 - median(R1/R0)` and `1 - median(R1_twin/R0_twin)`, paired by block.
   A pass requires loss <= epsilon and epsilon <= **0.01** in every measured
   depth-16 uniform/hot-key CPU cell. As a repeatability screen, every arm's
   median absolute twin difference must be <= 0.01, and the first-five versus
   last-five estimates of epsilon and of each candidate loss must differ by
   <= 0.01. Failure of any noise/repeatability screen is **inconclusive**;
   with those screens satisfied, loss > epsilon rejects the motivating
   performance claim. Report all raw ratios and spread, not only medians.
   The old no-stamp ratios differ from 1 by 3.1% and 2.7%; they do not supply
   this paired estimator, and noise that large could not pass this protocol.
   **A 3% residual loss is unacceptable under this proposed criterion.** If
   inconclusive, diagnose the noise and prerecord a new duration/batch before
   rerunning the complete panel; retain the first outcome and never widen
   the 1% ceiling. These thresholds are prospective research recommendations,
   not an owner-approved performance requirement or a retrospective verdict.
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

Use a pinned Clang/LLVM with ARM and RISC-V backends. Compile isolated aligned
8/16/32-bit relaxed load/store functions, 32-bit wrapping fetch-add and strong
compare-exchange, and 64-bit load/store boundary cases. Use C atomic builtins
with relaxed ordering and separately explicit LLVM `monotonic` IR, since a
C frontend's combined atomic policy may introduce a library call before
target lowering sees the load/store. Keep source, emitted IR, assembly,
diagnostics, exit codes and helper dependencies. The initial target commands
for the probe source `atomic-probe.c` are prospective, not runs performed here:

```sh
clang --target=thumbv6m-none-eabi -mcpu=cortex-m0plus -mthumb -O2 -ffreestanding -S atomic-probe.c -o thumbv6m.s
clang --target=riscv32imc-unknown-none-elf -march=rv32imc -mabi=ilp32 -O2 -ffreestanding -S atomic-probe.c -o rv32imc.s
```

Record the driver's normalized triple; if that Clang requires the canonical
RISC-V spelling `riscv32-unknown-none-elf`, use it with explicit `-march=rv32imc`
and record that mapping to `riscv32imc-unknown-none-elf`, never silently enable
A. Start with one `u32` load/store pair, record compilation duration, then
expand the width/operation panel. Add M23 and RV32+A as positive RMW controls
with their explicit CPU/ISA settings. In the no-RMW profiles, expect either
an unsupported-operation diagnostic or a library call for RMW, not an
invented hardware instruction. Resolve every called provider before claiming
usable atomics: merely emitting `__atomic_*` is not a refusal and not an
implementation qualification. If forced-atomics/provider support is explored,
record its flags separately and inspect its entire implementation; do not
use ordinary/volatile accesses to conceal an LLVM atomic-lowering gap.

For an eventual WF target, the same panel must additionally show successful
composition for native-width load/store and source-positioned refusal of
64-bit accesses or unsupported RMW. Enabling an approved single-core C profile
must select the bounded mask/restore sequence; selecting a multicore profile
must refuse that implementation. Check nested/already-masked entry, permitted
ISR access, privilege/security domains and preservation of the mask, with
unmasked accessors excluded by the target/runtime contract. Prove the complete
event-model mapping, including hold handoffs and no-thin-air, and inspect the
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
relaxed-containing reference parameters carry no `noalias`; wrapping boundary
and no-lost-update RMW; aliasing targets, multi-target ordinary invariants,
and aggregate movement. Include missing-key, nested-map and forced whole-hold
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
3. **Should atomic scalar identity live in a type or a field modifier?**
   Recommend S1 over S3 because a typed parameter can carry the discipline
   through ordinary helpers and arrays. S3 remains viable with a convincing
   modular projection rule. S2 can be studied as a separate optimization;
   S4 does not meet the chosen field capability. Confidence 4/5 on the
   interface distinction, not on implementation performance.
4. **How should a callable declare relaxed observations and mutation?**
   Recommend S1-W's truthful rows and declared-type/path hold selection, with
   conservative PAR conflicts including loads, the OWN-9 qualification and
   suppression of `noalias` for relaxed-containing referents. S1-A needs a
   demonstrated advantage to reopen CAP-1, EFF-1 and the effects decision.
   S1-R is rejected by the store/store counterexample. Confidence 3/5:
   signatures carry the relevant type, but generic/exported interfaces and
   the complete coverage/alias algebra still need validation. Embedded
   instruction or interrupt lowering does not change the recommendation;
   operation requirements must compose separately from effect rows.
5. **Which target-capability model should govern scalar operations, and may
   a single-core target declare interrupt-masked RMW?** The owner's embedded
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
   desktop throughput alone cannot settle it. The no-thin-air ordering cost
   remains subject to the separate causal-model decision below.
6. **Which nontransactional outcomes may firn accept for the stamp?**
   Recommend load/store LRU and an explicit evaluation of LFU lost updates,
   stale/regressing clocks, script rollback and introspection; choose CAS
   only if the desired accuracy/behavior requires it. Alternatively retain
   exact transaction semantics for those consumers. Confidence 2/5: Redis's
   algorithm and current firn paths are known, but concurrent behavior and
   eviction quality under the new semantics have not been measured.
7. **May relaxed cells be used as wait conditions or publication flags?**
   Recommend no in the first design: use existing guarded Shared state for
   synchronization, and specify snapshots only for scalar hints. A broader
   choice must add a visibility/progress/publication model and proofs, plus
   notifications on read-hold completion for any admitted wait conditions.
   Confidence 3/5: the safety boundary is clear; useful reliable polling may
   justify more later.
8. **Must relaxed values exclude cyclic thin-air justification?** Recommend
   retaining the no-invented-value promise with the proposed acyclic
   program-order/reads-from condition and qualified lowering. Alternatively
   explicitly permit causally unsupported but type-valid scalars, with no
   proof/publication authority; that weakens the promised behavior.
   Confidence 3/5: the cycle counterexample is clear and RC11 provides a
   reference condition, but Whitefoot's mixed model and weak-target cost remain
   unverified. Resolve this before fixing a one-instruction promise.
9. **What does whole-cell replacement mean after publication?** Recommend
   refusing direct replacement of the published cell and requiring its
   explicit atomic store; enclosing-owner replacement remains exclusive and
   quiescent. Alternatively specify whole-cell replacement as one atomic
   store of T. Neither permits a plain write through a read-held leaf.
   Confidence 3/5: the ambiguity is concrete; the chosen type/placement rules
   must make the published boundary statically checkable.

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
Architecture and runtime sources establish why embedded native load/store,
hardware RMW and single-core interrupt exclusion must be separated. The
owner's requirement preserves weak embedded CPUs in the intended target scope;
the proposed capability declarations and conditional C provider are not yet
approved rules or implemented target support.

Unverified: the owner's intended Store abstraction; the complete relaxed
execution model and progress guarantee; the choice and soundness of effect,
proof and modular projection rules; actual code generation and layout for
each declared operation/target/feature/runtime profile; bounded exclusion's
admissibility and an embedded port's context/interrupt contract; S2's mixed-use
implementation; any general safe upgrade/replay protocol; firn's LFU quality,
rollback and observable compatibility under relaxed updates; and the depth-16
performance criterion and embedded path/interrupt cost. No acceptance,
performance success or implementation completion is claimed for any candidate.
Next run the existing-language same-source controls and embedded CI code-shape
probe, and evaluate the proposed event model and target qualification plan for
the owner's semantic choices, before changing language rules. No build, test
suite or benchmark was run for this documentation revision. The local Clang
`-###` query only inspected driver defaults; it compiled and linked nothing.
Effective LTO CPU/features and Linux outlined-atomic dependencies
remain part of target qualification, not conclusions of that query. The
embedded probe above is a protocol only; it has not been compiled or run for
this documentation revision.

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
[14900k]: https://www.intel.com/content/www/us/en/products/sku/236773/intel-core-i9-processor-14900k-36m-cache-up-to-6-00-ghz/specifications.html
