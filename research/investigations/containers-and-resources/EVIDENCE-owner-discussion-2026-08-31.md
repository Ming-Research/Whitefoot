# Evidence — owner design discussion, 2026-08-31/09-01

Condensation of the owner's design session on containers, backing, and resource
failure. The session ran in a fork of an earlier session; the owner's messages
are rulings, the assistant's and the audit agents' messages are proposals and
audits that only become evidence when the owner accepted them.

- Source: session transcript, 9224 lines, `USER` / `ASSISTANT` / `AGENT_MESSAGE`
  turns. All `L` references below are line numbers in that transcript.
- The owner writes in Chinese. Section (a) gives each ruling in English
  translation. The Chinese originals were removed on 2026-09-27, because the
  repository's artifacts are in English, and remain in its history, for example
  at commit `ad51e05df`. Every other section is English.
- This file records only what the discussion contains. It adds no design of its
  own; each statement carries the line range it comes from.

---

## (a) The owner's rulings, in order

### R1 — `L207` — open the deferred resource questions

*Translation:* This session is a fork; the original session continues its own
work. Here I want to discuss the things that were deferred, OOM among them.
First lay the problems out briefly, then let us look at what solutions exist and
what their advantages and drawbacks are.

### R2 — `L872` — performance first, then safety; do not half-solve OOM; containers must not be forced onto the heap

*Translation:* The language's goal is maximum performance, then safety. I think
a system-dependent problem like OOM, if it cannot be solved, should not be
forcibly half-solved by introducing schemes that make performance or code worse.
The same holds for recursion. Also, we should let the user avoid heap and
recursion altogether and so reach maximum safety. The language should permit
that use; it must not force some containers to be bound to the heap.

### R3 — `L989` — tail recursion is the recursion answer; heap is not, so containers must separate from the heap

*Translation:* Recursion has one special case: tail recursion is fine. So
recursion is not hard — loops and tail recursion are the two fallbacks. The heap
is not so simple, so containers need to be separated from the heap. Let us now
start designing the container/backing separation.

### R4 — `L2224` — best design, not a release plan; the hard part is the container/backing coupling; discuss only the decisive details

*Translation:* Do not talk about a first version and a second version. I am not
discussing a roadmap. I want the best design; only once we have it does it get
broken into a roadmap. Get the priorities straight. A few compiler-known storage
kinds should suffice — there are only a handful: heap, stack, global. The
troublesome part is the coupling between container and backing: vector growth
depends on the heap, so it is hard to pull vector and heap apart. Do not rush to
enumerate every detail in one essay; we should discuss the decisive details and
skip the rest.

### R5 — `L2332` — a non-growing push makes loop appends unusable

*Translation:* push does not grow implicitly? Then when I push elements in a
loop, how am I supposed to know when growth happens?

### R6 — `L2396` — hold correct positions; do not agree reflexively; use adversarial agents when unsure

*Translation:* You got something this simple wrong? What are you thinking? Must
you say "you are right" to everything? Think more: hold to what is correct, and
do not throw out something wrong without thinking. When you are unsure, start
several agents and have them pick holes in it.

### R7 — `L2408` — do not lose the original question inside the branch

*Translation:* Also, I think you have gone off topic. From the original question
to now we have burrowed into one branch; the branch matters, of course, but let
us not forget the topic we were discussing before.

### R8 — `L3254` — name it `heap vector`; "fixed" is the side with variety

*Translation:* I suggest just renaming it to heap vector — what else could it
possibly be but heap? That is more direct. It is "fixed" that has several
choices.

### R9 — `L3280` — accepted, but check the variant explosion in the type system

*Translation:* Fine, that works. But will it cause a type-system problem — for
instance, does a function taking a vector then need several variants?

### R10 — `L4489` — keep researching

*Translation:* Research it further.

### R11 — `L5530` — ignore what I/O uses today; the container conclusion is taking shape; move to the open directions

*Translation:* Never mind what I/O uses today; we are designing the best
container scheme, and once it exists everything will be updated globally anyway.
I think the conclusion above has taken initial shape. Let us discuss the
remaining uncertain directions.

### R12 — `L5657-5666` — a pool with a fallback is worthless; the two-tier split is right and is named `res-closed`; heap fragmentation defeats a byte cap; no depth certificates; the runtime must meet `res-closed` or explain why not

*Translation:* Quoting the proposal "use pre-granted bounded resources: acquire
a truly guaranteed memory pool before the program starts, after which
`PoolVector` operates only within that capacity" — what happens when it is
exceeded? With no Result the only options are falling back to something larger
or dying, and both are no different from not having the pre-grant at all. So
what is the point of it? On the proposed two tiers — every accepted program: no
UB, no out-of-bounds, no overflow, no illegal address, no data race; may
terminate when environment resources run short. A resource-closed program: uses
no unguaranteed heap; has no unbounded recursion; stack, pool, and par runtime
capacities all have static upper bounds; once the required resources are granted
and `main` is entered, it will not terminate again from resource exhaustion — I
think that is a very good split. The second tier corresponds to places like
high-assurance embedded applications. By analogy with `no-std`, we can call it
`res-closed` or some better name. In fact, even giving the heap a cap does not
guarantee space is available: the heap also has internal fragmentation. On
"allow the author to supply a recursion-depth certificate: more expressive, but
the proof system gains a significant new domain" — do not do this. On "when the
resources for the runtime and `par` are acquired" — the runtime must meet every
requirement of `res-closed`, and if it cannot, you must tell me why.

### R13 — `L7036` — `resource-closed` is the program's promise, and the causality runs program first, environment second

*Translation:* `resource-closed` obviously cannot guarantee success in the
physical world; what it guarantees is the program's own promise. Do not get the
direction backwards. A `resource-closed` program cannot request more resource
than the compiler statically computed. That is the program's promise to the
outside world. If the outside world satisfies that promise, the program will not
fail. Very reliable. A non-`resource-closed` program makes no promise outward,
and the world makes none to it; both are best-effort. So we must first have the
outward `resource-closed` promise, and only then go and satisfy the
environment's promise to it — confirming page commitment and so on.

### R14 — `L7232` — the definition holds; keep going and keep it defensible

*Translation:* The definition of `resource-closed` looks good. The analysis
after it is good too, all on the right track. Keep researching. Make sure the
research can stand up.

### Not rulings

`L3` and `L4277-4487` are tool-injected instruction and environment text, not
owner design statements. `L9205` asks where the session log lives on disk and
carries no design content.

---

## (b) Conclusions the owner accepted

### B1 — Container and backing separate into owners and views

The owner directed the container/backing separation at `L989`, and the
discussion settled that container and backing cannot be independent of each
other, only separated in responsibility: a vector is always bound to one exact
backing, and that binding is carried by the static type and by ownership
(`L2226-2253`). The three-layer statement is that the backing owner knows
capacity, location, lifetime and release; the container knows `len`, the
initialized state and its own invariants; and the view knows only pointer, range
and access mode (`L1985-2013`, `L4563-4628`).

### B2 — `push` never grows

`push` is a fixed-capacity operation whose obligation is `len < capacity`; once
proved, its lowering is one store plus one integer increment, with no branch and
no allocation, and it is identical for inline and heap backing (`L2254-2276`,
`L2054-2098`). The owner's counter-example at `L2332` killed an earlier form of
this rule that had no growth path at all; the surviving rule is that `push`
itself never grows while growth remains available as a separate operation
(`L2753-2822`, `L3155-3193`).

### B3 — Growth is owner-level, with an explicit effect and a typed failure

Real growth is storage-specific: only a heap-backed owner supports it, it
carries `allocates(heap)` explicitly, and an inline vector may never silently
spill to the heap (`L2277-2297`). The growth transaction order is fixed —
compute the new capacity and check the arithmetic and target domain, acquire the
new backing, leave the old vector and the input value completely unchanged on
failure, then move elements, commit the descriptor once, and release the old
backing — and the failure is a typed outcome carrying the value back, never a
hidden termination path (`L2874-2916`, `L4629-4684`).

### B4 — `HeapVector` and `FixedVector<N>`, named for the resource they depend on

The owner rejected the vague `GrowableVector` name at `L3254`, and the accepted
naming states the resource dependency in the type: `HeapVector<T>` is
heap-backed, `FixedVector<T, N>` has fixed capacity with several possible
placements, and any future arena or pool vector gets its own name rather than
hiding under a shared "growable" label (`L3256-3277`). Seeing the type is
therefore enough to know that a value cannot participate in a heap-free
guarantee (`L3269-3277`, `L3209-3247`).

### B5 — One unified view boundary, so ordinary algorithms are written once

Backing may be erased at zero cost, but only as far as a borrow: read, mutation
of existing elements, and append within current capacity unify, while automatic
growth and owner consumption do not (`L3628-3662`, `L5032-5060`). The accepted
shape is `HeapVector` / `FixedVector` / future arena and pool owners all
projecting in O(1) to `Span`, `MutSpan` and the append capability, with each
function declaring the least capability it actually needs (`L4161-4222`,
`L5384-5406`).

### B6 — `AppendView` and the `len` write-back

Beyond `Span` and `MutSpan` there is a third capability that may only use
existing capacity but may advance the initialized prefix; it binds the data
origin, the owner's opaque `len` state, and the capacity, and it never grows and
never allocates (`L5429-5450`, `L5077-5100`, `L3733-3766`). The implementation
detail the discussion insisted on is that this view cannot be a by-value
`{ptr, len, cap}`, because the `len` the callee changes would not reach the
owner; the compiler must fix a write-back protocol, such as a hidden returned
new `len` or a stable `len` slot, and this needs no tag, vtable, or allocator
pointer (`L5476-5484`).

### B7 — System I/O takes views, not an owning heap buffer

The audit found that the system operations require a caller-owned `buffer<u8>`
while `buffer` is necessarily heap-owned, so every file, directory and host-copy
operation forces a program through the heap (`L4720-4740`). The accepted
boundary is that a system operation borrows a read view or a write view with a
range and lets the caller choose the backing, keeping the existing good
properties — no allocation, no growth of caller storage, statically proved
range, a loan-released milestone, and `TooSmall` or partial progress as typed
outcomes (`L4740-4764`, `L2173-2181`, `L5516-5522`).

### B8 — The `resource-closed` definition and its causality

`resource-closed` is the program's unilateral, statically checkable promise: for
every legal execution and every finite prefix of an infinite one, the component's
requests for covered resources never exceed one finite envelope `E`, with no
assumption that any environment has delivered anything (`L7060-7085`,
`L8884-8912`). The owner fixed the causality at `L7036`: the compiler proves the
bound, the program promises never to exceed it, the environment then decides
whether it can meet it, and only the conjunction of the program's promise and the
environment's admission yields freedom from covered-resource exhaustion
(`L7042-7059`, `L8731-8789`).

### B9 — A general heap, and a bounded general pool, cannot be part of `resource-closed`

The owner's own reason is fragmentation: even a capped heap does not guarantee
that space is available, because the heap also has internal fragmentation
(`L5657-5666`). The discussion made that concrete — a 16-byte region holding four
4-byte objects, with the first and third freed, has 8 free bytes yet cannot serve
an 8-byte contiguous request — so proving `max live bytes` is not enough, and a
general heap without a full allocator-and-backing guarantee simply cannot be the
basis of `resource-closed` (`L8913-8966`, `L8311-8343`, `L6500-6540`).

### B10 — No recursion-depth certificates

The owner struck the depth-certificate option at `L5657-5666` when it was offered
as the more expressive alternative that would add a significant new proof domain.
The surviving recursion rule is therefore only two-valued: tail recursion is
structurally lowered to a loop by the compiler before lowering and consumes no
recursion stack, and an ordinary recursive SCC admits no finite stack envelope and
cannot be `resource-closed` (`L1967-1984`, `L7160-7172`).

### B11 — `E` is a list of tangible resources, not a byte count

The envelope must state what the environment actually has to deliver, with shape:
a contiguous region with size and alignment for the main stack, a contiguous
scratch region with its allocator discipline, a count of same-shaped completion
slots, and one worker stack per lane that meets its own bound (`L8913-8966`,
`L7100-7140`). The general form is an initial grant of shaped items —
`memory_region(domain, size, alignment, committed)`, `handle_token(kind, rights,
count)`, `queue_pool(kind, count)`, `worker_stack(layout, count)` — over which the
target's fixed resource state machine is the thing the proof runs on
(`L8277-8310`, `L8444-8465`).

### B12 — `par` resources are acquired before `main`, and the runtime is inside the promise

The owner required at `L5657-5666` that the runtime meet every `res-closed`
requirement or explain why it cannot, and the accepted shape puts a `SourceStart`
barrier before source execution at which all covered resources are committed, all
workers are created and parked, and all lazy runtime state is initialized; the
guarantee then covers `SourceStart` through `ProgramFinished`, not just the body of
`main` (`L6315-6349`, `L5626-5654`). `E` therefore includes the runtime's own
demand — worker count, task, deque and join slots, completion records,
continuation frames, runtime metadata, cleanup scratch — and the environment
selects one finite profile row before entry, after which `W` is fixed and invisible
to source, with queue-full handled by inline execution, helping, waiting or
backpressure rather than by acquiring more (`L7141-7159`, `L9095-9138`). The
refinement the owner's correction forced is that the requirement is not
"instantiate everything before `main`" but "never demand more than `E`": building
objects during the run out of an already granted region is deferred use of a
granted resource, not an enlarged demand (`L7086-7099`).

---

## (c) Alternatives the discussion rejected

### X1 — One `Vector<T, B>` with a single `push`

Merging "use existing capacity" and "acquire new resource" into one `push` gives
the same call a different return type and a different effect per backing: the
inline instance can be total under a capacity proof while the heap instance may
allocate, may OOM, and may hit a target capacity limit (`L2753-2792`). Widening it
into one large `PushError` forces every generic caller to handle errors that
cannot occur for its backing, and letting the heap version return `unit` and abort
on OOM reintroduces the hidden trap, so the operation had to be split into
`push` / `try_push` / `reserve` (`L2793-2822`, `L3155-3193`).

### X2 — A span plus a loose external length state

If the append interface were two free parameters — a mutable span of slots and a
`&uniq` vector state — an author could pair storage of capacity 4 with a state
holding `len = 100`, or cross storage and state taken from two different vectors
(`L3780-3798`). The accepted repair is that the append capability must bind the
two through the backing origin as one opaque, branded borrow: the runtime value may
still be just a pointer, a `len`-state pointer and a capacity, but the source may
not take it apart and recombine it (`L3790-3798`, `L5077-5100`).

### X3 — A sum-type owner

An `AnyVector<T>` enum over heap, fixed and arena cases cannot even express the
parameters it needs — an arbitrary `const N`, an arbitrary arena lifetime, an
arbitrary pool identity — and closing the set does not fix it (`L4064-4083`). Even
closed, the owner grows to the largest variant, every move, drop and grow checks a
tag, the fixed variant's inline storage makes the enum enormous, boxing it to
shrink reintroduces a hidden heap, and returning it permanently loses the concrete
storage guarantee; borrowing needs no sum at all, since the owner projects directly
to a view (`L4084-4097`).

### X4 — Grow callbacks or grow capability on a view

A view holds the current backing's pointer, length and capacity, and a heap grow
replaces the backing and invalidates that pointer, so a view-neutral function can
only require enough spare, return `Full` / `NeedCapacity`, or hand the owner back
for the caller to grow (`L2956-2986`, `L3799-3843`). Giving a view a
grow callback or an allocator would make it carry a function pointer or provider
after all, which is exactly the indirect call, larger descriptor,
statically-undeterminable effect and runtime-selected release the design set out
to avoid (`L2987-2999`, `L4005-4032`).

### X5 — Generic traits for read algorithms

Writing `checksum<V: ReadableVector<u8>>(v: &V)` monomorphizes the same loop
separately for heap, fixed and arena owners, adding code size and compile cost and
buying no additional semantic capability (`L3690-3704`). A `Span` of pointer and
length is the correct boundary: it needs no vtable, no backing tag and no drop
responsibility, and the fixed vector's `N` is a constant at the projection point
that the compiler can still propagate (`L3663-3689`).

### X6 — `SmallVector` as a state change of `Inline<N>`

A value of static type `Vector<T, Inline<N>>` cannot change its own `B` to `Heap`
at runtime; it is simply a fixed-capacity inline vector, and describing it as one
that "becomes heap when full" is wrong (`L3000-3013`). A real small vector is a
separate backing policy whose `B` is itself a runtime sum, and it pays real costs —
the owner always contains `N` inline slots, a move may relocate `O(N)` elements,
drop must distinguish the two states, a spill may OOM, and a live view blocks the
spill — so it must be its own type, and the no-spill fact must come from its state,
not from its type name (`L3014-3056`).

### X7 — Shared `push` inside `par`

Even with a proved `len(out) + n <= capacity(out)`, every iteration writes the same
`len`, choosing a slot needs an atomic fetch-add or a lock, and the output order
depends on the schedule; proving total spare only proves that no growth occurs, not
that each iteration gets a disjoint slot (`L3057-3080`). The accepted form is
reserve, then a position map — each iteration initializes its own slot in the
reserved range, the checker proves uniqueness and full coverage, and the new length
is published once after the join — with all reserve or OOM failures occurring before
any parallel effect begins and no growth inside a parallel view (`L3081-3124`,
`L2182-2205`, `L5514`).

---

## (d) The eleven questions from the spec audit (`L2436-2743`)

The audit raised these as preconditions for the boundary being self-consistent.
Each entry records whether the later discussion answered it and how.

### Q1 — Is `Vector` an ordinary source struct, or a builtin/opaque type? (`L2444-2478`)

**Answered, in the direction of a compiler-known type.** The container cannot be
an ordinary library struct under the current rules: the compiler must understand
its initialized prefix and its drop, though it stays a small storage primitive
rather than pulling a whole container library into the language core
(`L2115-2121`). The later responsibility split makes this normative per owner
type — the heap owner is responsible for capacity acquisition, growth, the OOM
outcome, element ownership and release, and the fixed owner for its `N` inline
slots, fixed capacity and `Full` outcome (`L4519-4562`). The audit's option C,
re-proving arbitrary field states at every view, was not taken; no field-privacy
mechanism was selected either.

### Q2 — Must `Inline<N>` support affine `T`? (`L2479-2510`)

**Answered in mechanism, not settled as a rule.** The audit's premise was that an
inline backing would need `array<Option<T>, N>` to hold affine elements and that no
such thing exists; the accepted design removes the premise, because the
initialized prefix is a checker-maintained fact with one runtime `len` and no
per-slot tag, so `Option<T>` is not the representation in either backing
(`L2054-2098`). The discussion never issued an explicit ruling on whether
`FixedVector` admits affine `T`, and the drop rule it did fix — drop `[0, len)`
in fixed order — is exactly the rule such elements would need (`L2100-2121`).

### Q3 — What exactly is the `view` given to algorithms? (`L2511-2532`)

**Answered.** Read-only is not sufficient: the accepted boundary is three
borrowed capabilities — `Span` for reading, `MutSpan` for reading and mutating
initialized elements without changing `len`, and the append capability for
push/pop within existing capacity (`L3643-3662`, `L5032-5060`). The audit's
observation that today's slice is shared-only, so a `&uniq` slice descriptor still
cannot write the backing, was accepted as a real gap that a genuine exclusive
write view must close (`L4627`, `L2206-2221`).

### Q4 — How does `view<T>` hide vacant slots? (`L2533-2574`)

**Answered: it does not need to.** A view covers only `[0, len)`, and the
invariant `capacity = capacity(backing)`, `0 <= len <= capacity`, `[0, len)`
initialized, `[len, capacity)` uninitialized and unreadable is maintained by the
checker with one runtime `len` (`L2054-2077`). Of the audit's three candidates —
container typestate, compiler-known invariant maintained by fixed operations, or
an author certificate — the second was taken: the author cannot reach raw
`storage[i]` at all and only the fixed operations `at`, `push`, `pop` and `span`
give access (`L2078-2098`).

### Q5 — Does the view borrow the owning vector, the backing identity, or a storage range? (`L2575-2590`)

**Answered as a borrow of the owner, uniformly.** The unique borrow guarantees the
owner cannot grow, move or drop for the duration of the call, and that single rule
is applied to every owner rather than being made policy-dependent (`L3705-3732`,
`L5429-5450`). Because a grow replaces the backing and invalidates the view's
pointer, a live view blocks growth — stated for the heap owner and again for the
small-vector spill (`L2956-2967`, `L3014-3056`); the audit's alternative of
letting a heap view tolerate a moved header was not adopted.

### Q6 — How do `push`'s name, return type and effect stay canonical across policies? (`L2591-2623`)

**Answered by removing the cross-policy operation.** There is no single `push`
that sometimes grows: `push` requires `len < capacity`, never allocates and has
no capacity branch; `try_push` is fixed-capacity and returns the value on `Full`;
and `reserve`/`grow` is owner-level with an explicit provider and effect and a
typed `OOM`/`CapacityLimit` failure that leaves the vector unchanged
(`L3155-3193`). This dissolves the audit's own objection that option C would
collide with the exact effect system, since the heap body exhibits
`allocates(heap)` and the inline body does not and there is no effect polymorphism
to bridge them (`L2609-2623`, `L2965-2999`).

### Q7 — Is `Heap` a static policy name or a real allocator/budget owner? (`L2624-2648`)

**Answered at the two ends, with no allocator-parameter design chosen.**
`HeapVector` is a concrete owner type that is normatively responsible for heap
capacity acquisition, growth, the OOM outcome and heap release, and OOM is confined
to its construction and growth boundary (`L4519-4535`, `L5526`). For the
proof-bearing side, the resource work concluded that an ambient general heap cannot
support any guarantee and that a pool only counts when the environment delivers a
specific pool and the compiler proves the program never exceeds its real capability
(`L7100-7140`, `L8967-9005`). The audit's question about proving resource
independence between two heap-backed vectors was not separately resolved.

### Q8 — On heap `push` failure, what state are `T` and the vector in? (`L2649-2675`)

**Answered.** The failure is a typed outcome that carries the value back — the
example form is `heap_vector_push(...) -> Ok(...) | OutOfMemory(value: T)` — and
every heap-capacity operation must be either a fallible acquire that returns typed
`OutOfMemory` with all passed-in owners returned or unchanged, or a reserved
acquire against a granted ticket that has no OOM outcome at all; a third kind whose
signature looks total but takes a hidden termination path on allocator failure is
forbidden (`L4629-4684`). Failure atomicity is the fixed six-step transaction of
B3, so nothing is dropped, duplicated or lost (`L2890-2916`).

### Q9 — Is growth policy part of the language semantics? (`L2676-2696`)

**Answered as a set of constraints, not as a chosen multiplier.** The fixed points
are that logical capacity is a language value in the descriptor and is not the
allocator's usable size, that `reserve(required)` guarantees `capacity' >=
required`, that allocation arithmetic and target-domain obligations are proved
before the allocator is called, and that `CapacityLimit` and `OOM` are distinct
causes (`L3140-3154`). The checker may not assume any growth factor unless `B`'s
policy states it as a formal contract rather than as a library implementation
detail, which is the audit's own condition for making exact doubling provable
(`L3125-3139`, `L3149-3154`).

### Q10 — Is drop by logical length or by full backing state? (`L2697-2712`)

**Answered: by logical length.** The compiler-owned drop of the container is to
drop `[0, len)` in a fixed order and then execute the backing's release, and the
release step differs per backing — inline does nothing, arena lets the region
reclaim the bytes, pool returns the block, heap frees it — with no runtime tag or
dynamic dispatch because `B` remains in the static type (`L2100-2121`,
`L4765-4790`). The audit's `O(capacity)` alternative was therefore not taken;
what carries it is the initialized-prefix invariant of Q4, and when `T` is a copy
type with no release the element loop disappears entirely (`L2054-2077`,
`L2115-2117`).

### Q11 — Does "algorithms use views" include parallel mutable ranges? (`L2713-2743`)

**Answered: yes, but not by sharing one append view.** The accepted parallel form
is reserve, then mapped initialization into disjoint slots, then one published
length after the join, with the checker proving that each `i` maps to a unique slot,
that all slots lie in the reserved range and that the range is fully covered
(`L3081-3110`, `L2182-2205`). Unknown output lengths use a per-iteration count, a
prefix sum, one reserve and disjoint ranges, or per-worker local vectors merged at
the end; a per-worker fixed capacity may not be justified by
`total_items <= workers * N`, because the schedule may hand one worker `N+1` items
(`L3097-3124`, `L5514`).
