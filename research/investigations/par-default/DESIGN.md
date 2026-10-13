# Demand splitting in default --par

## Question and proposal boundary

Can default `--par` expose already-permitted loops and statement groups on
demand, preserve ordinary sequential computation when no splitting is useful,
and cover indexed reductions and handler-bearing values without introducing
a second acceptance path?

This is a stage-3 design proposal against main
`5b7838b328632999712aacf8313717b6dca1ade6`, not an implemented capability or
a measurement result. The compiler and runtime remain unchanged by this
document. The proposed design-tree decisions await owner approval. The
worth-splitting card remains open: proceed on option A, with runtime range
lengths for loop pricing, static statement-group grain, no scheduling clock
in stage 3, and adaptive pricing reopened by real-program mispricing evidence.
The requested registration rule covers every lane, including helpers.

Demand-driven recursive calls, a new recursion grain beyond experiment 6b,
and centralizing all decisions in one pass belong to later stages. Existing
budgeted recursion stays eager inside its budget and enters ordinary
sequential computation at its cut. This proposal changes no specification
rule: PAR-1 and PAR-2 grant optional overlap; DIAG-2 retains its checked
authority; EFF-5, OWN-7 and REF-2 continue to govern interference and lifetime;
STOR-6 and STOR-8 continue to govern target domains and resource exhaustion.

Keep this investigation as the derivation and qualification record for the
production decisions. Update its open comparisons with their eventual
results; retain superseded reasoning only where it supplies useful evidence.

## Evidence and its limits

All prototype citations below are frozen at
`73aaab85a92479ada6bf57fa996e17b163e019ed`, rather than following a branch.

- [Experiment 3](https://github.com/Ming-Research/Whitefoot/blob/73aaab85a92479ada6bf57fa996e17b163e019ed/research/investigations/par-demand/DESIGN.md#experiment-3s-results)
  found runtime-extent pricing reduced W8 wall time relative to static-price
  demand to 0.838 for Mandelbrot and 0.888 for stencil. Eager seeding made
  records and FIR about 3.6--3.7 times slower; request deduplication did not
  explain the excess CPU. These observations support the chosen pricing,
  not those alternative scheduler policies.
- [Cheap-region versioning](https://github.com/Ming-Research/Whitefoot/blob/73aaab85a92479ada6bf57fa996e17b163e019ed/research/investigations/par-demand/DESIGN.md#experiment-5c-cheap-region-versioning-fixed-before-it-measures)
  and [canonical lowering](https://github.com/Ming-Research/Whitefoot/blob/73aaab85a92479ada6bf57fa996e17b163e019ed/research/investigations/par-demand/DESIGN.md#canonical-sequential-lowering-before-loop-synthesis)
  explain why deleting a poll or cloning an already outlined loop does not
  recover ordinary computation. [Option D](https://github.com/Ming-Research/Whitefoot/blob/73aaab85a92479ada6bf57fa996e17b163e019ed/research/investigations/par-demand/DESIGN.md#identity-construction-option-d)
  requires actual normalized-disassembly identity of computation, with
  evidenced cold resource-record and entry-adapter boundaries.
- [The TLS lifetime failure](https://github.com/Ming-Research/Whitefoot/blob/73aaab85a92479ada6bf57fa996e17b163e019ed/research/investigations/par-demand/DESIGN.md#a-crash-at-exit-in-the-demand-prototype)
  makes withdrawal of an exiting owner's request-word address necessary.
  [The direct poll](https://github.com/Ming-Research/Whitefoot/blob/73aaab85a92479ada6bf57fa996e17b163e019ed/research/investigations/par-demand/DESIGN.md#the-poll-was-still-a-call-the-seventh-change-fixed-before-it-measures)
  explains why an accessor or hidden TLS resolver is not equivalent in cost.
- [Startup exposure repair](https://github.com/Ming-Research/Whitefoot/blob/73aaab85a92479ada6bf57fa996e17b163e019ed/research/investigations/par-demand/DESIGN.md#startup-exposure-repair)
  self-requests only on lane 0. Extending registration to every lane is the
  production instruction, not a result established by that experiment.
- [Experiment 6b and its result](https://github.com/Ming-Research/Whitefoot/blob/73aaab85a92479ada6bf57fa996e17b163e019ed/research/investigations/par-demand/DESIGN.md#experiment-6b-both-sides-grain-and-the-fir-single-thread-criterion-fixed-before-it-measures)
  support the static retained-side call grain and the selected construction.
  The record accounts for 38 rows: 29 timing-summary passes, seven
  not-applicable rows and two inconclusive W1 timing diagnostics. Applying
  the owner's computation-identity rule gives 31 applicable-rule passes,
  seven not applicable and zero failures. It does not turn the two timing
  diagnostics into timing passes. Its 25 protected preservation cells,
  single machine and limited W2 coverage do not qualify the whole unified
  matrix, dynamic indexed storage, or handler transfer.

There is no clock-versus-estimate scheduling comparison. The selected
no-clock proposal is provisional; its reopening witness is an actual program
where cost estimation withholds useful parallel work or exposes work whose
scheduling cost violates the registered bound. The existing idle-window
clock has a different purpose and is not removed by this proposal.

## Shared scheduling and sequential computation

Let `U = 150000` estimated work units and `p >= 1` be the existing
runtime-extent per-iteration price, using the established typed projections
and static fallback. A slice executes at most `max(1, floor(U/p))` iterations
before the next eligible request observation. A split needs at least two
remaining iterations and at least `ceil(U/p)` remaining iterations. It
publishes the far half and retains the near half; partial slices already
executed belong only to the retained result. These are work estimates, not
microseconds or a bound on actual elapsed time. An expensive single iteration
is indivisible. The handed-out half need not itself reach U.

No per-loop query asks the runtime for a split budget, and no power-of-two
leaf count or 16-chunks-per-worker cap remains. A constant small site is
ordinary sequential code with no request read, task frame or splitter call.
A constant trip count alone is insufficient: a three-iteration loop calling
an expensive helper is not necessarily small.

### Range-site proof and arithmetic

The range-site proof uses literal endpoints. An empty, inverted or singleton
range cannot satisfy the driver's requirement of two remaining iterations,
so its ordinary CFG can be retained independently of the body's price. For
larger literal ranges, the current finite cheapness proof requires a call-free,
unnested completed chunk and uses the same instruction cost divided by the
ordinary loop factor as the existing static estimator. A saturated product
below 150,000 proves that this scheduling estimate cannot admit a split.
Calls and nested loops retain runtime extent pricing; a short literal count
does not establish their complete work. The completed ordinary CFG is spliced
into the caller before reserving that range's outlined helpers, preserving
captures and cleanup
without a scheduling call or an experiment-only sentinel weight.

Every driver caller supplies a price of at least one, including when a valid
runtime extent estimate evaluates to zero. The driver compares the cursor
with the upper endpoint before subtracting. It computes a segment length as
`min(upper - cursor, max(1, floor(150000 / price)))`, and adds that length to
the cursor. A granted split computes `cursor + floor((upper - cursor) / 2)`.
Both additions are at most the upper endpoint and therefore cannot overflow,
even when that endpoint is the largest u64. Eligibility is computed as
`(150000 - 1) / price + 1`, avoiding overflow-prone multiplication or ceiling
addition. Completed segments contribute only to the retained seed; the far
half begins with the reduction identity and is combined after the near result.

### Entry selection and ordinary computation

For a function whose complete reachable split decisions can be expressed
from entry-available values, one all-cheap predicate selects the complete
ordinary sequential body. This is at most one choice per function entry,
separate from pool selection at the program entry. All-cheap constants are
folded away. Analysis must include loop prices, indexed setup prices and
surviving calls; stable length observations need the existing checked
read-only/lifetime evidence and must be safe to evaluate even on a zero-trip
path. A source guard is not moved or used to speculate an invalid read.
Unknown, varying or recursive summaries retain local demand decisions. The
prototype's repeated-scheduling-only restriction is not the production
contract; pure entry-invariant cases outside that subset require coverage.

Ordinary computation comes from the same checked input lowered without
parallel extraction, with the same target, roots, optimization settings and
emission mode. Retain its reachable computation and address dependencies,
not only functions that can themselves hand out work. Cheap entry choices,
the recursive budget cut and the existing optional sequential-refusal edge
may enter that closure. The closure does not reenter split computation.
Its resource-record writer remains the shared worker-safe latched writer.
Actual linked computation identity, including the two three-element loop
workloads, is checked separately from IR identity and entry-dispatch cost.

### Lane requests

World selection remains based on the requested worker width. Before entering
request-gated computation, an active selection starts the pool and attaches
the selecting thread, registering its request word and posting its first
request. One-worker selection does neither; repeated selection on an attached
lane is idempotent. Startup refusal retains the requested-world answer and
the existing refused-acquisition fallback. This startup edge is present in
[the prototype's fourth change](https://github.com/Ming-Research/Whitefoot/blob/73aaab85a92479ada6bf57fa996e17b163e019ed/research/investigations/par-demand/DESIGN.md#the-fourth-change-fixed-before-it-measures).

A direct poll cannot itself initialize the pool. Without the selection edge,
the zero TLS word prevents the first acquisition, which prevents attachment
and registration, so no thief can request this lane. A native bootstrap probe
must enter through public world selection and assert registration and the
initial request before any acquisition. Calling private attachment first or
supplying a synthetic request tests the later protocol but misses this edge.
The source-result oracles' existing real-worker assertions independently
protect the complete compiler/runtime connection.

Each lane publishes a TLS request-word address and posts one request to
itself at registration. Idle thieves request victims through a protected
address; the owner polls with a relaxed atomic load and clears before
publishing, so a later request can survive the clear. A request is advisory,
not task publication synchronization. Pointer withdrawal and in-flight
request counting retain the stronger ordering that protects TLS lifetime.
Use platform primitives for thread-exit cleanup on POSIX and Windows,
including an arbitrary first-attaching driver thread. Helpers, owner exit,
partial startup, late-spinning and late-parked schedules need native tests.
The helper self-request is a deliberate bootstrap exception to the rule
that only an idle thief asks; it uses the same bounded offer mechanism.

## Indexed reductions: question and alternatives

The prototype retains the old splitter whenever indexed families are
present. Main's `compiler/src/backend/emitter/indexed.rs` allocates dense
slabs using `2^budget` leaves; `indexed_blocks.rs` allocates a directory of
root-shaped blocks for call-form updates. Merely replacing the budget query
cannot make those representations demand-driven.

The question is whether allocating one far contribution state per granted
split preserves PAR-2 and pays its extra allocation/combination cost. The
existing [indexed-reduction derivation](../indexed-reductions/DESIGN.md#lowering-choices)
supports private contributions, while its
[helper-call lowering](../indexed-reductions/DESIGN.md#lowering)
requires root-shaped storage when ordinary callees see a whole root. Neither
establishes the cost of an unpredictable number of demanded splits.

| Alternative | Correctness and cost | Disposition |
| --- | --- | --- |
| Retain fixed leaf budgets only for indexed reductions | Existing storage plan is reusable and can amortize allocation, but every loop retains the removed query/cap and reserves work without demand. | Rejected for this proposal's scope; retain as a measured reference. |
| Run every indexed reduction sequentially | Preserves results and avoids private-state overhead, but removes useful parallelism independently of demand and work size. | Comparison arm, not an unannounced fallback for a missing implementation. |
| Allocate fresh private states for both children | Can preserve results, but must first account for the retained prefix, duplicates initialization/storage and abandons an already usable destination. | Rejected in favor of retaining the near destination. |
| Retain near destination, allocate only the granted far contribution | No private allocation on no-demand/refused paths; storage lifetime follows the structured join. Extra allocations can exceed the old total leaf count. | Selected proposal, subject to the comparison below. |

### Indexed mechanism

A driver activation receives its current reduction destinations, their
source/dense-private/root-private modes, scalar accumulator seed if any,
range, captured immutable descriptors, and the checked family metadata.
The current destination may be original source storage or a parent's private
contribution. It is never rediscovered from an original source binding.

1. Execute slices directly into the current destinations. With no request,
   insufficient work or a refused frame acquisition, continue on those same
   destinations without allocating private storage or evaluating source
   operands again.
2. At an eligible requested split, partition the unexecuted interval into
   disjoint near and far halves. Determine frame fit using the final target
   representation, then reserve a lane frame through the existing acquisition
   boundary. Reservation grants a split opportunity; publication occurs only
   after its complete state is initialized. A failed reservation allocates
   nothing. No new global pool query is introduced.
3. After reservation, create one neutral far bundle. For roots having no
   call-form updates, allocate dense cells per operation family and false
   masks per mark family. If any family of a root has a call-form update,
   allocate one root-shaped block shared by all that root's families. Retain
   the existing header, stride, projection and measure-preservation rules;
   initialize operation cells to their identity and mark cells to the typed
   constant with its lowest bit inverted (opposite Bool). Preserve only
   initialized inline measures and borrow measure-only Box subtrees; never
   duplicate their cleanup authority or copy incoming accumulator values.
4. A typed bundle descriptor belongs to the acquiring activation and remains
   live through its join. The lane frame borrows that descriptor and the far
   storage. There is no heap directory indexed by predicted leaves. Capture
   remapping makes every alias and every helper argument for one root reach
   that same private root, including fixed inline ancestors and borrowed
   ancestor slots. Group by resolved root identity, not argument position.
5. Publish the far task, with identity scalar seed when a scalar reduction
   accompanies the indexed ones; execute the near half using the current
   scalar seed and current destinations. Both can split further under this
   same rule. A child borrows its incoming destination and owns only the
   new far bundles it acquires itself.
6. Join far after near has completed. Combine each far contribution into the
   current destination in fixed family/cell traversal order, then free that
   far bundle once, then release the reserved lane frame. A worker executing
   or reclaiming far never releases its incoming borrowed bundle. Every child
   completes its own joins before returning, so no descendant can retain a
   pointer after the parent's free.

All transported reduction families that the far task can write must be
isolated and combined, including inherited families used by nested loops.
Compute this set from retained checked family/call-summary metadata, not
from a second effects judgment. A shared block cannot remap an inherited
sibling and then omit that sibling's writes from combination. Conversely,
read-only inherited state is not copied as a seeded contribution and merged
again. Nested joins combine into their captured enclosing private state.

For an operation family, combine the destination cell and the far cell with
the family's existing operator. For a dense mark, a true far mask marks the
source cell with k or sets the enclosing private mask true. For a root-shaped
mark, far cell == k means marked and far cell == sentinel means untouched;
propagate the mark using the destination's mode. Zero is a legitimate mark
constant. Combining masks as numeric data or copying the sentinel back to
source is incorrect.

### Indexed result and lifetime argument

For one operation cell c, maintain the invariant

`destination[c] = incoming[c] op contributions(completed retained work, c)`.

A new far state is `identity op contributions(far, c)`. The split partitions
the remaining iteration set, so after both halves finish, combining far
adds each contribution once. Induction over the finite actual split tree
gives the incoming value exactly once plus exactly the source iteration
contributions. PAR-2 supplies totality, associativity, commutativity and a
two-sided identity, and excludes contributions or subscripts depending on
partial accumulator contents. The argument applies independently to every
cell and to the optional scalar accumulator; it does not admit floating
point or other operations absent from PAR-2.

For a mark family the state is the set of touched cells. Private false masks
or non-k sentinels represent the empty set; combination is union. A source
cell is written with k exactly when touched, and otherwise retains its input.
Different field families are disjoint. Call-form summaries contribute the
same per-cell multiset or mark set, with measures preserved. Source operands
and guards still execute once in their original per-iteration order, and
the endpoint captures are not re-evaluated. Near-before-far combination fixes
an implementation order, although PAR-2 permits every combination tree.

The private bundle is unreachable by a worker until publication. After that,
only far and its descendants write it; near writes a disjoint destination.
Join completion precedes combination and freeing. Borrowed measure subtrees
remain live under the original structured source scope. No private root is
run through source-language release/drop; its owned raw allocations alone
are freed, once per unique root or dense family.

### Indexed allocation failure and cost

Use checked allocation sizing with the selected target layout, not saturating
allocation arithmetic. Keep an initialized-prefix list in the unpublished
bundle. A size failure or failed allocation frees the acquired prefix and
any partial current-root allocation, cancels the unpublished reservation,
then enters the shared STOR-8 resource-abort path. No far work was visible.
Failures after publication terminate through that same failure path; do not
free storage still borrowed by running tasks or introduce unwinding.
Cancellation needs an explicit unpublished-state contract, including trace
accounting: the current tracing release path assumes a published task.
Allocation failure is never converted into a successful sequential result.

Keep a reservation until its far storage is combined and freed. Consequently
simultaneously live far bundles are bounded by simultaneously reserved lane
slots, not by an invented one-bundle-per-worker bound. Each activation still
has one destination; cumulative allocations and combinations can grow with
the actual split count. There is no claim that total allocation work is
bounded by the retired 16-chunk cap.

The proposed marginal price retains the existing three-traversal estimate:
`H = saturating(3 * sum(initialized family cell counts))` for the far bundle.
In addition to the ordinary remaining-span and request conditions, split
only when estimated far work is strictly greater than H. Equality does not
split. This replaces multiplying by a predicted leaf count. H is advice,
not a time upper bound: capacity-sized root initialization, wide records,
allocator contention and preserved descendant measures may cost more than
this length-based estimate. Overflow in price arithmetic saturates and
cannot make an expensive setup look cheap. An all-cheap entry predicate must
include this setup comparison where its inputs are entry-invariant.

### Indexed comparison and rejection criterion

The semantic mechanism can be proposed before measuring; production adoption
needs a new measurement because 6b kept these loops on the legacy splitter.
Compare the proposed splitter, same-source fixed-budget indexed splitting,
and ordinary sequential execution, with identical compiler/target settings
and a byte-identical base twin, interleaved on the CI 14900K at 1/2/4/8.
Research-only baseline construction must not remain a production switch.

Use the maintained indexed histogram/extrema/product and helper-call/mark
fixtures plus a visible sweep that varies iteration count, initialized root
length, len/cap ratio, skew, number of families, record width and nested
splits. Include zero contributions, nonzero incoming cells, mark k=0 and
false, inherited mixed families and partial allocation failure for
correctness. A sequential source-order oracle checks every result cell.
Record allocation count, peak live bytes and combination work in separate
diagnostic builds; timed builds carry no counters.

Freeze the pilot-derived scale before a decisive comparison. Wrong cells,
duplicate seed contributions, changed endpoints, use-after-free, double-free
or incomplete initialization reject correctness. The stage-3 performance
bounds in the final section decide economic suitability. In particular a
qualified persistent regression above 5% against current parallel execution
where it already speeds up rejects this proposal as ready for production;
better fixed-budget results would require revisiting the allocation/price
mechanism or an explicit scope ruling, not retaining that path silently.
If no indexed parallel case meets the sequential bound or supplies useful
speedup, the sequential alternative must return to the owner. Capacity or
record-width failures reopen the marginal price before any held-out tuning.

## Handler transfer: question and evidence

The existing board problem `gran-canon-handler-transfer` describes separately
planned ordinary and split worlds with incompatible enum representations.
The [canonical-lowering record](https://github.com/Ming-Research/Whitefoot/blob/73aaab85a92479ada6bf57fa996e17b163e019ed/research/investigations/par-demand/DESIGN.md#canonical-sequential-lowering-before-loop-synthesis)
and [prototype witnesses](https://github.com/Ming-Research/Whitefoot/blob/73aaab85a92479ada6bf57fa996e17b163e019ed/compiler/src/backend/tests/match_dispatch.rs#L1590)
show two distinct failures: `Cell` has zero split-world handler words but one
ordinary word; or both have one word but the stored addresses designate
different code. Both checked programs currently expect backend `InvalidIr`.
These are inspection/test-source witnesses, not newly executed failures.

An enum handler word is a private address of one dispatch arm. A dispatch
family is one loop's dispatcher and arms with a common prototype and its
enclosing activation's frame layout. Equal pointer types, tags or word
counts do not make handlers from two families interchangeable. A thin
`&Cell` can conceal both storage and target mismatch.

The question is how to pass such values, including mutable borrowed storage
and owned nested payloads, while ordinary computation keeps its non-parallel
representation and dispatch code.

### Handler alternatives

| Alternative | Consequence | Disposition |
| --- | --- | --- |
| Rewrite handler words at world entry and restore on exit | Writes through read-only aliases, can race another reader and needs arbitrary graph traversal; mutable/owned payload conversion also needs alias-preserving writeback and cleanup. | Reject in-place rewriting; there is no general checked conversion contract here. |
| Put ordinary and split handlers in separate words | Can preserve aliasing but adds words, offsets and constructor stores to the ordinary layout, potentially changing ABI and exceeding its ceiling. | Reject for the ordinary computation-identity requirement. |
| Use one shared handler body or a world-selecting trampoline | Family prototypes/frames and behavior can differ; always selecting ordinary code can suppress split computation, while a selector changes ordinary hot code. | Reject as the general transfer mechanism. |
| Disable handler words for the whole program | Safe tag-table representation, but changes the ordinary optimization and its machine code even for isolated families. | Reject blanket removal. |
| Preserve the ordinary layout and target identities; split dispatch uses its own tag table | Borrowed values need no conversion; ordinary dispatch stays ordinary, and split handlers receive their own frames. Changes split dispatch cost and frame sizing. | Selected proposal, pending performance and owner acceptance below. |

### Handler mechanism

1. Lower the ordinary program from the same checked input, roots and target.
   Compute its ordinary dispatch plan to the existing stable layout/ABI
   fixed point for the requested emission mode. This plan chooses the
   canonical family ordering, word count, offsets and final arm bindings.
   Freeze it before final split-world ABI and lane-frame qualification.
2. Apply that plan to source nominal representations in both worlds,
   matching checked nominal identities explicitly rather than assuming that
   independently lowered numeric identifiers agree, transitively through
   inline aggregates, containers and owned payloads.
   Ordinary code uses exactly its existing plan. A split-only dispatch
   family adds no stored word. Do not prune canonical words to make a split
   frame fit: requalify that offer against the shared layout and use its
   existing ordinary fallback if the bounded frame cannot carry it.
3. Every producer in either world initializes the same canonical words:
   ordinary constructors, enum-valued primitive results, replacements and
   value materialization. A whole-value copy transports them unchanged.
   Use the canonical importer's explicit symbol mapping, including final
   result-ABI wrapper identities. Destruction, allocation stride, reference
   projections, return slots and callback copying use this same layout.
4. An ordinary dispatch loads its canonical word and transfers only to its
   own ordinary family, with its ordinary prototype and frame. A split-world
   dispatcher chooses an address from its own tag-indexed table and transfers
   with that split family's prototype and frame. It never calls through a
   canonical stored word. Entering the ordinary world through an approved
   ordinary callable boundary supplies an ordinary activation, not a split
   dispatch frame. No per-dispatch world selector is added.
5. Import every canonical handler address materialized by a live constructor
   and its complete recorded definition/type/global/helper closure, even if
   no ordinary function entry was otherwise a clone root. Conservatively
   rooting every selected canonical family is acceptable. Constructor-only
   address reachability is not ordinary call-graph reachability.
6. Preserve the ordinary plan's genuine foreign boundaries: runtime-created
   or runtime-zero-filled payloads, native signatures, shared storage and
   waiting/context transport keep their qualified representation. Replace
   the blanket exclusion for compiler-emitted compute frames only after
   auditing that the runtime treats their payloads as opaque bytes and
   emitted constructors/callbacks implement every typed operation. A newly
   discovered runtime-authored typed boundary needs a real ABI design; it
   cannot silently force a different canonical layout.
7. Fragment builds freeze the matching ordinary fragment plan, currently
   tag-and-table with no stored handler words. This proposal does not
   authorize fragmenting a whole-program threaded image afterwards. Cache
   identity covers the ordinary program, target/probed ABI, emission mode,
   plan and arm-binding inputs; word counts alone are insufficient.

The transport invariant is: every reachable value has the canonical physical
representation and initialized canonical targets; every canonical target is
retained with its exact family contract; split dispatch ignores those
targets. A replacement in either world reestablishes this invariant. Copies
preserve it. Borrowing moves no storage and rewrites no words, so arbitrary
permitted aliases and callbacks can observe the same value safely. A returned
owned value has the same representation, and writes through a borrowed root
are immediately visible without copyback. Stored code addresses are private
metadata, so this alters no source value or permission judgment.

### Handler comparison and owner boundary

First turn both prototype rejection witnesses into successful output checks.
Cover nested owners and containers, borrowed aliases, replacement followed
by return into the other world, primitive enum producers, canonical targets
reachable only from split constructors, equal counts with different family
signatures, fragments, and a compute frame that crosses its size bound after
the canonical plan is applied. Check actual computation closures against
matching-mode non-parallel disassembly; rendered IR equality is insufficient.

The proposal does change split-world representation and dispatch instructions.
The existing `gran-canon-handler-transfer` acceptance asks for unchanged
parallel emitted code, as well as successful transfer. That byte-preservation
requirement must be amended by the owner for this proposal: preserve source
computation and all split opportunities whose final frames fit, require
ordinary computation identity, and qualify the changed split code by the
registered performance bounds. This document does not silently deem the old
acceptance satisfied. The prototype's assumption that the parallel plan
remains untouched is superseded here as a proposal, not as an approved fact.

Correctness selects this viable architecture without an advance timing run;
its performance remains open. On CI compare same-source ordinary/current-par/
candidate images with a base twin at 1/2/4/8, inspecting retained dispatch
families, constructor stores, frames, code size and actual indirect targets.
Use both transfer witnesses and the existing dispatch workloads; downstream
Halo/Firn/Snowghost cases require their owning sessions' pinned inputs.
Do not infer a downstream pass from an interpreter microbenchmark. Failure
of ordinary identity or any wrong target rejects construction; a persistent
qualified regression beyond the bounds below rejects production adoption.
Reopen shared handlers only with proof that a complete family's behavior,
prototype and frame are identical, not merely that it has the same tag.

## Open qualification and acceptance

The stage-3 item requires the unified loop/call matrix at 1/2/4/8 workers:
ordinary and selected cheap computation have identical normalized machine
code; the wall ratio to sequential has 95% interval upper bound at most 1.00
or the entire interval is inside its separately measured layout floor; current `--par` speedups
are preserved within 5%; a nonhoistable decision costs at most max(1 ns, 2%)
per execution; CPU is at most `1.1 * sequential CPU + 0.1 * saved wall * W`,
where saved wall is max(0, sequential wall minus candidate wall), reported
rather than gating where extra workers still demonstrate speedup.
Entry dispatch remains the separately evidenced boundary of option D.
The generic par-suite startup allowance is not a replacement for these
item-specific bounds. Freeze comparison construction and layout controls
before timing, keep first/second-call and whole-process intervals distinct,
and retain visible/held-out discipline.

Production qualification uses current-main `--par` as a separately built
baseline and the candidate's ordinary build, with corresponding runtime
revisions and hashes. No experiment ablation, previous/unversioned build,
ledger, diagnostic counter or identity-inspection hook becomes a production
policy. Native protocol tests keep only test-object scheduling controls.
Old evidence remains at its frozen URLs rather than being copied into the
daily gate. No measurement in this section has run for this proposal.

Open measurements are dynamic indexed allocation/combination cost, split
dispatch/table cost, full unified-matrix results and platform TLS/protocol
qualification. The two designs' correctness arguments require independent
review and their listed executable witnesses before implementation can be
accepted. The owner still rules on the no-clock card, these proposed tree
decisions, and the handler item's changed byte-preservation criterion.
