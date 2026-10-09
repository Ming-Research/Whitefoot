# Relaxed scalar fields under shared read holds

## Question and comparison, before implementation or measurement

Can Whitefoot let a program update a small scalar hint while it holds the
surrounding shared state for reading, retaining memory safety and ordinary
state's transactional meaning, without paying an entry lock? Which scalar
types and operations can have that contract on every supported target, and
what distinguishes the result from a shared object holding one scalar?

The selected direction is the owner's A: a basic-type field updated atomically
under a read-only hold. This selects the problem to solve, not a spelling,
effect rule, memory model or instruction-width policy. This investigation is
a proposal checked against Whitefoot `0c71e2c266d59963302006bf2c8b510ec0fec762`, branch
`claude/relaxed-fields`, active specification v0.108. It changes no language
rule or implementation. No new compilation, concurrency test or performance
measurement has been run for it.

Compare four shapes: an explicit scalar cell type (S1), optimization of
existing scalar shared-object statements (S2), a scalar field modifier (S3),
and read-then-upgrade or optimistic statements (S4). First compare their
observable executions and proof/effect boundaries against the current
specification. Before changing the language, compare the same firn source
with stamping disabled, stamping under today's entry lock, stamps in a second
map, and a separate write-on-change statement. These are attribution controls,
not replacements for the owner's direction. Then compare a surviving relaxed
candidate against those controls. Include separately repeated images of each
as twins. The proposal fails its motivating performance criterion if
GET at pipeline depth 16 on the i9-14900K loses more than the no-stamp twins'
spread. Faster code with a weaker safety proof does not qualify. The detailed
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

**Working interpretation, awaiting the owner:** “Store<u32>” means
`Shared<u32>`. If it denotes another intended abstraction, that abstraction's
contract must be supplied before the design is selected.

Under that interpretation, a relaxed scalar does **not** make `Shared<u32>`
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
* `wf_cmap_read_entry` keeps a reader count on the entry; a writer locks the
  key and waits for readers before accessing the payload. The count also
  prevents reclamation/movement from invalidating an active reader's node
  ([compiler/src/backend/concurrent_map.c:702–708,820–840,1176–1218,1256–1285][cmap]).
  It protects ordinary payload lifetime and stability, not concurrent plain
  stores by two readers. The candidate's field accesses must all be atomic.
* “Lock-free read path” is the repository's name for the normal route, not a
  proof that an entire lookup is lock-free: `wf_cmap_read_entry` may wait on a
  writer, help a move and, on `IMPATIENT`, take `wf_cmap_hold`. This existing
  progress mechanism must be distinguished from the forbidden new fallback
  that implements an unsupported scalar atomic with a hidden lock.

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
qualification. Inspect the AArch64 mapping, including orders already supplied
by holds, before promising one instruction per operation. An alternative is
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

The first experiment should cover integer load/store and explicitly wrapping
fetch-add as separate cost cases. CAS is a candidate for lossless LFU or
conditional counters, not presumed necessary for the LRU witness. Exact
addition cannot silently inherit hardware wrap: OP-1/2 distinguishes exact,
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

### Supported targets versus exercised targets

The closed ABI list is [compiler/src/target.rs:50–104][targets], not the set
of architectures LLVM can generally compile. The emitter uses the selected
triple ([compiler/src/backend/emitter.rs:399][emitter]).

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
qualification was found. Triple admission is not proof of a CPU feature floor.

### Instruction and library guarantees

“Lock-free”, “one atomic memory instruction”, and “one instruction for the
entire function” are different requirements. An LL/SC retry sequence may be
lock-free without being single-instruction or wait-free. An x86 `LOCK` prefix
does not mean a software mutex fallback; it still incurs coherence traffic.
Use natural alignment, normal cacheable/shareable memory and one fixed width
per cell. Do not extend the table to MMIO, packed unaligned fields or mixed-size
overlapping accesses.

| Target family / feature floor | 8/16/32/64-bit load and store | Same-width fetch-add and compare-exchange | 128-bit caveat |
|---|---|---|---|
| All three admitted x86-64 ABIs | Aligned accesses can use single `MOV` memory instructions. | Fetch-add can use `LOCK XADD` (or `LOCK ADD` without the old result); CAS uses `LOCK CMPXCHG`. These are atomic memory instructions; moving arguments and producing a Bool/result can add instructions. | `CMPXCHG16B` needs CPU support and 16-byte alignment. Other operations may require a CAS loop. Modern Intel also documents some aligned 16-byte moves as atomic when AVX is enumerated; that is not a guarantee for every x86-64 CPU or Clang target. |
| Both admitted AArch64 ABIs, conservative Armv8-A baseline | Naturally aligned 8/16/32/64-bit loads/stores have single-access forms (`LDRB/H`, `LDR`, `STRB/H`, `STR`). | Without LSE, use exclusive load/store sequences and retry where required. Lock-free implementations are possible; a single-instruction promise is false. | Pair-exclusive sequences exist, but a load pair is not generically an atomic plain 128-bit load. Qualification depends on the operation and architecture features. |
| AArch64 with FEAT_LSE explicitly guaranteed | Same as baseline. | `LDADD` and `CAS`, with byte/halfword variants, provide single atomic memory instructions for these widths. | LSE `CASP` supplies pair CAS; it does not imply one-instruction fetch-add or a universally atomic ordinary load/store pair. LSE2 and later features must be considered separately. |

Hardware sources: [Intel SDM volume 3A §10.1.1–10.1.2][intel-atomic],
[Arm synchronization guide §§2,3,5][arm-atomic],
[Arm's LSE versus baseline example][arm-lse], and
[Arm's discussion of LSE2][arm-order]. Compiler mappings and the possibility
of library expansion: [LLVM Atomics, “Atomics and Codegen”][llvm-atomic].
These establish the architecture-level distinction; **actual output for all
five Whitefoot triples, their CPU flags, alignment and linked runtime is
unverified**. In particular, do not infer that Apple's default target flags
and Linux's baseline select identical RMW instructions.

All admitted ABIs here use 64-bit ordinary pointers, but Whitefoot has no
writer-visible pointer-width integer; TYPE-1 lists fixed widths
([spec/kernel-spec.md:407][spec]). A lock-free pointer-sized machine load
does not license `Relaxed<Box<T>>`, `Relaxed<Shared<T>>` or references: those
values carry ownership, retain/release or validity obligations. The current
allocator-alignment floor is eight bytes in these target records, so a
128-bit proposal also needs a layout/allocation argument. LLVM's `i128` in
data-layout strings is not a source type or an atomicity promise. Start with
signed/unsigned integers of 8, 16, 32 and 64 bits; investigate Bool and f32/f64
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

Qualification should fix the target features and field layout, require the
appropriate always-lock-free result for the selected width, and inspect the
emitted code and linked symbols for each admitted operation. GCC's
`__atomic_always_lock_free` is a compile-time query, unlike a runtime query
that may vary by object; Clang exposes the compatible builtins and its C11
macros ([GCC atomic builtins][gcc-atomic], [Clang stdatomic.h][clang-atomic]).
A “true” answer alone does not prove the single-instruction requirement.
No runtime branch choosing a locked scalar implementation is acceptable.
An outlined AArch64 helper that chooses LSE or LL/SC is not automatically a
lock fallback, but it still fails a strict inline/single-instruction cost
contract and requires inspection.

### Candidate availability rules

* **P1: portable intersection, recommended for the first load/store design.**
  Admit only widths/operations qualified as lock-free on every admitted
  target; refuse the unsupported combination explicitly. The expected
  intersection is 8/16/32/64-bit integer load/store. Those same widths can
  have lock-free RMWs on both architectures if LL/SC loops are allowed, but
  not single-instruction RMWs at the conservative AArch64 floor. Unqualified
  targets do not silently borrow a nearby ABI's answer.
* **P2: target-qualified operations.** Keep source typing independent of a
  machine; selected-target composition must prove a stated width, alignment
  and CPU-feature requirement and otherwise fail diagnostically. This could
  admit single-instruction RMW only on x86-64 and LSE AArch64. It improves
  availability but complicates portable library contracts. Capability data
  must be deterministic target requirements, not a timing probe or the CPU
  that happened to run the compiler. This fits the existing module/composition
  boundary in [design/language.md:15][language-node], but is a new rule.

Neither rule treats 128-bit integers as already present. Neither makes a
language verdict depend on optimizer success or replaces required safety
with a runtime check. The owner must choose whether “one instruction” is a
requirement for load/store only or for every offered operation.

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

There is no implicit conversion to T, no reference to the raw inner integer,
and no ordinary `set` into its representation. Recommend refusing direct
whole-cell replacement after publication: `writes(cell)` at `&Relaxed<T>`
then permits the atomic operations, not a hidden plain replacement. The
alternative is to define such replacement as exactly one atomic store of T;
leaving it an ordinary write while allowing a read hold is unsound. Replacing
or moving the enclosing owner remains an exclusive, quiescent operation;
its layout/lifetime treatment must preserve the atomic representation.
Initialization before publication is exclusive; after publication all leaf
accesses, including in an exclusive block, use that representation. A move
cannot relocate a cell an active reader still reaches. No references escape
SHARE-2/REF-3. The type should be usable as an owned local,
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

**Reader contract and composition.** Ordinary payload remains stable for the
hold; each cell access has the proposed scalar semantics above. A relaxed
leaf stays relaxed even inside a block that also names `Shared<u32>` or
several entries: ordinary targets commit together, relaxed events are not
rolled back or part of their joint commit. Source evaluations still execute
once. This must be explicit wherever a client expects transactional logging,
snapshots or scripts. `Shared<u32>` retains its stronger role described above.

**Cost and risks.** An inline aligned word plus the current reader pin and
atomic instructions, including any ordering needed by the event model; no
per-entry cell allocation. Cache-line ownership, false sharing and writes on
hot keys remain. All shapes admitting mutation under read holds (S1-W, S1-A
and S3) need the same alias and stability audit:

* `reference_parameter_facts` emits `noalias` for source-signature reference
  parameters of nonwaiting functions other than Run references and the
  alias-permitting `swap` family; synthesized functions without that evidence
  also skip it ([compiler/src/backend/emitter.rs:880–883,1702–1735][emitter]).
  Helpers called inside atomic blocks are nonwaiting by SHARE-2
  ([spec/kernel-spec.md:2282–2283][spec]); their signatures receive no exemption
  merely for having a caller that holds a reader pin.
* [LLVM's parameter `noalias` contract][llvm-noalias] excludes accesses via
  unrelated pointers to memory modified during the call. Another context's
  atomic store violates that promise when the helper accesses the cell,
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

Recognize a single scalar load, store or RMW and lower it to hardware atomic
instructions **only if all existing SHARE-3 outcomes and progress are
preserved**. Rows and `waits` remain unchanged. A relaxed machine store is not
automatically an implementation of the global source order; SC operations
or a proved equivalent protocol may be needed and can cost more. The reader
gets the current transactional guarantee, not a stale-value hint contract.

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

Every load and store of the designated leaf would be atomic, including
accesses outside map entries under ordinary ownership and accesses through
helpers. Ordinary owner replacement remains exclusive. `readonly` cannot
serve as this marker: it already controls assignment across a module
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
S1. Less explicit access syntax carries more implicit behavior; aggregate
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
unverified. General replay changes SHARE-2/3, WAIT-2, effects, ownership and
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
implementation of an admitted scalar atomic on any supported target; or
silently changes ordinary SHARE-3 behavior. If the selected policy requires
single-instruction RMW, an LL/SC loop fails that policy even if lock-free.
Evidence of small cost on one CPU never overrides these conditions.

| Shape | Additional rejection conditions |
|---|---|
| S1 | Reject if a helper signature conceals mutation, `noalias` or stability attributes cover concurrently mutable leaves, load/load implicit overlap violates coherence, whole-cell replacement bypasses atomic access, cell moves/copies race readers, or the split between ordinary transaction events and relaxed events has no coherent execution/proof model. Reject as firn's performance solution if the depth-16 criterion below fails. |
| S2 | Reject an optimization that can race or interleave with an ordinary locked/guarded/multi-target use of the same object, or loses current source ordering. Already ruled out as the sole solution to the map-entry-field requirement by its scope, regardless of standalone-counter speed. |
| S3 | Reject if passing a designated field through `&u32` loses its discipline, a generic helper needs body inspection to be safe, or surface-equivalent load/add/store unexpectedly becomes an indivisible RMW. Apply S1's model and performance rejection conditions too. |
| S4 | Reject if two upgrading readers can deadlock, any replay duplicates an observable effect or consumes an owner twice, validation follows an unsafe racing read, or continue-after-upgrade uses stale references/facts. It also fails direction A if refresh still requires the entry lock, even if conditional refresh improves one benchmark. |

The performance comparison is **prospective** and must not inherit unrelated
differences between the old Firn-wf base/head builds:

1. **Before a language change**, pin Whitefoot, Firn-wf, Halo-wf, Redis benchmark
   version, compiler/Clang flags and LTO settings. Use one firn source with
   only the experimental stamp choice varied; keep entry layout the same for the direct cost
   comparison. Also compare to actual firn main without stamping to expose
   total layout cost. Retain an actual-stamping/old-lock control and add the
   second-map and separate write-on-change controls above on that same source.
   Their representation/semantic differences must be reported separately
   from the fixed-layout on/off comparison. Vary only the named control;
   the previous journal/helper/layout differences are not acceptable inputs
   to a new stamp-cost attribution. Evaluate the lossy buffer separately as
   a policy alternative. Run and interpret these controls before amending
   the language, then qualify and add the relaxed candidate.
2. Use the CI `14900k` runner, confirm it is idle and coordinate a long run.
   Record OS, CPU/microcode, affinity, client/server core placement, frequency
   policy, key/value sizes, key distribution, dataset occupancy, client count
   and warmup. Reuse the motivating workload's settings once recovered from
   its artifacts; do not invent the settings missing from this record.
3. Begin with the smallest useful timed sample and inspect its spread before
   choosing a batch. Interleave no-stamp/base, its twin, each stamp control
   and its twin, and later the candidate and candidate-twin with balanced
   ordering. Test one and two server CPUs at depth 16 separately; depth 1
   and SET are controls.
4. Before that batch fix the estimator: for each workload cell, let epsilon
   be the median absolute fractional throughput difference between paired
   no-stamp twins. Let loss be one minus the median paired candidate/base
   throughput ratio. The criterion is loss <= epsilon in **each** depth-16
   CPU cell. Also require repeatable twin estimates; if their spread cannot
   distinguish the hypotheses, lengthen/repeat and report inconclusive rather
   than widen a tolerance after seeing the candidate. Report candidate twins
   as a second noise control, not a license to raise epsilon. This estimator
   is proposed here; it was not retrospectively applied to the older table.
5. Inspect the GET lowering and profile: actual stamp stores must execute;
   the candidate must retain reader acquisition/release without selecting the
   ordinary entry write route for that field. Same-source falsifiers are
   forcing the old lock route with identical stamp arithmetic/layout, and
   removing only the candidate store. If removing the lock does not recover
   cost, or if removing the store does, distinguish lock cost from remaining
   coherence/arithmetic cost. Recompute attribution instead of claiming the
   old profile explains the new result.
6. Exercise LRU and LFU separately under uniform, hot-key and Zipf access;
   record the control-specific stale/change/drop/drain observations above.
   For each control predict the separating observation before its run:
   moving the same writes to a second-map lock tests the payload-lock claim;
   write-on-change tests whether actual change frequency repays a relookup;
   buffering tests whether tolerated policy loss repays batching. Record an
   outcome that would reject each explanation, including negligible recovery
   outside twin spread or a policy-quality failure. A target probe must
   establish both scalar code shape and the chosen no-thin-air ordering
   before a performance claim. The language need not beat every data
   structure; results apply only to measured operations and workloads.

Semantic validation precedes candidate throughput: the specified lost-update,
multi-target and IRIW outcomes; forbidden coherence regressions, thin-air
cycles and stale loads after reader-to-writer handoff; rejection of mixed
raw/atomic access and invalid guards/proofs; ordinary-payload stability under
replacement/deletion/resize; cross-module helpers and inspection that their
relaxed-containing reference parameters carry no `noalias`; wrapping boundary
and no-lost-update RMW; aliasing targets, multi-target ordinary invariants,
and aggregate movement. An independent event model should be the oracle,
with runtime sanitizers/stress
as additional implementation evidence, not a proof of the memory model.
Negative cases must fail for the intended rule. Native CI must qualify the
missing ABI/feature combinations before “all supported targets” is claimed.
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

1. **Does “Store<u32>” mean the current `Shared<u32>`?** Recommend yes as the
   working interpretation, preserving Shared's whole-block/lifetime/guard
   meaning. Otherwise identify the intended Store contract and repeat the
   comparison. Confidence 4/5: current declarations and the historical
   `Shared<Store>` sketch support the interpretation; only the owner can
   settle the name.
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
   the complete coverage/alias algebra still need validation.
5. **What is the operation and platform promise?** Recommend P1 with
   8/16/32/64-bit integer load/store, and distinguish lock-free RMW from
   single-instruction RMW. Choose between permitting qualified LL/SC RMW,
   limiting RMW to P2 feature-qualified targets, or deferring RMW. Keep
   pointer-bearing, 128-bit and floating RMW outside this first decision.
   Confidence 4/5 on architecture distinctions; emitted-code qualification
   remains unverified, including ordering costs from decision 8. There is
   never a silent scalar lock fallback.
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
   choice must add a visibility/progress/publication model and proofs.
   Confidence 3/5: the safety boundary is clear; useful reliable polling may
   justify more later.
8. **Must relaxed values exclude cyclic thin-air justification?** Recommend
   retaining the no-invented-value promise with the proposed acyclic
   program-order/reads-from condition and qualified lowering. Alternatively
   explicitly permit causally unsupported but type-valid scalars, with no
   proof/publication authority; that weakens the promised behavior.
   Confidence 3/5: the cycle counterexample is clear and RC11 provides a
   reference condition, but Whitefoot's mixed model and AArch64 cost remain
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
The source review also establishes the missing alias-attribute obligation,
the need for relaxed load/load conflicts, and the ENT-2/TYPE-11/guard exclusion
points. The event model and control protocol are proposals, not results.

Unverified: the owner's intended Store abstraction; the complete relaxed
execution model and progress guarantee; the choice and soundness of effect,
proof and modular projection rules; actual lock-free code generation and
layout on every admitted target/feature floor; S2's mixed-use implementation;
any general safe upgrade/replay protocol; firn's LFU quality, rollback and
observable compatibility under relaxed updates; and the depth-16 performance
criterion. No acceptance, performance success or implementation completion
is claimed for any candidate. Next run the existing-language same-source
controls, and evaluate the proposed event model and target qualification plan
for the owner's semantic choices, before changing language rules. No build,
test suite or benchmark was run for this documentation revision.

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
[targets]: ../../../compiler/src/target.rs
[emitter]: ../../../compiler/src/backend/emitter.rs
[backend-facts]: ../../../design/compiler/backend-facts.md
[toolchain]: ../../../compiler/src/toolchain.rs
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
[rc11]: https://people.mpi-sws.org/~dreyer/papers/scfix/paper.pdf
[llvm-atomic]: https://llvm.org/docs/Atomics.html#atomics-and-codegen
[intel-atomic]: https://cdrdv2-public.intel.com/835754/253668-sdm-vol-3a.pdf
[arm-atomic]: https://documentation-service.arm.com/static/68c223238a337a2bc6645c0a
[arm-lse]: https://developer.arm.com/community/arm-community-blogs/b/tools-software-ides-blog/posts/making-the-most-of-the-arm-architecture-in-gcc-10
[arm-order]: https://developer.arm.com/community/arm-community-blogs/b/tools-software-ides-blog/posts/armv8-sequential-consistency
[gcc-atomic]: https://gcc.gnu.org/onlinedocs/gcc/_005f_005fatomic-Builtins.html
[clang-atomic]: https://clang.llvm.org/doxygen/stdatomic_8h_source.html
