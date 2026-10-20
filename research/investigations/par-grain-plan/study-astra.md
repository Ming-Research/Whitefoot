<!-- Independent study for the plan in DESIGN.md, 2026-10-09, by GPT-6 Astra (Codex, reasoning effort xhigh), read-only. It was given only
the brief (the question, the owner's words on the criteria, the rules), the
criteria section as of commit 2d9fe0652, an evidence-cited history of the
mechanism and the classified backlog, and a checkout of main at 81010e578 that
does not contain DESIGN.md or this author's recommendation. It did not build,
run or time anything. Kept verbatim as evidence; the comparison is in
DESIGN.md. -->

**I recommend studying demand-driven lazy parallelization, with an explicit budget for scheduling overhead. Static estimates and profiles should guide which work to expose, while completed work pays for exposing it.**

This is a research direction, not a claim that it already meets the criteria. **No design can honestly satisfy H1 and H2 exactly as written.** H1 requires a universal wall-time guarantee under conditions a runtime cannot control. H2 sometimes demands speedup that even an ideal scheduler cannot achieve. Those problems need correcting before selecting a production design.

I worked alone, read only, and used no network, builds, tests, or measurements. Repository citations below refer to checkout `81010e578da238a231cc790ee57c2883620545ff`. The brief describes a different revision, `5907200fa701317ba60abba8e4f6cb090012c0a0`; its October 9 small-loop candidate is absent here. This checkout still calls the runtime query and overlapping splitter at loop entry (`compiler/src/backend/emitter/parallel.rs:814`). I distinguish repository evidence from proposals and deductions below.

**Why I would change the organizing principle**

Whitefoot already establishes the difficult semantic prerequisite: permission to overlap work without changing its result. PAR-1 covers complete statement footprints, including argument evaluation; PAR-2 covers iterations and specified reductions. Neither permission requires actual parallel execution (`spec/kernel-spec.md:2147`, `spec/kernel-spec.md:2164`, `spec/kernel-spec.md:2171`, `spec/kernel-spec.md:2215`).

That permits the compiler to prepare several execution choices without introducing speculative execution, rollback, dynamic dependence checking, or runtime proof validation. Scheduling tests remain implementation decisions between equivalent executions. They do not become conditions of source acceptance.

The missing distinction is between:

- **An opportunity:** work that could safely run elsewhere.
- **A prediction:** how valuable exposing that work might be.
- **A spending rule:** how much overhead the implementation may incur while discovering and exploiting opportunities.

Today those are partly combined in separate mechanisms. Calls face a static threshold or recursive-component exemption; loops receive an estimated-work-based split depth; recursion receives a worker-count-based depth limit. The compiler’s static price counts IR instructions, substitutes callees three times, and uses a loop multiplier of 16 (`compiler/src/lowering/builder/call_grain.rs:24`, `compiler/src/lowering/builder/split.rs:1560`, `compiler/src/lowering/builder/split.rs:1613`).

The evidence argues against making any one of those predictions the permanent admission rule:

- A permitted phased loop priced at 199 stays unsplit through 32 iterations whether its leaves perform 1 or 65,536 recurrence steps (`research/investigations/compute-model/DESIGN.md:387`).
- Removing the recursive frontier restores deep-spine overlap, but the stable-scatter trial becomes approximately 30–37% slower at parallel widths (`research/investigations/compute-model/DESIGN.md:350`, `research/investigations/compute-model/DESIGN.md:2901`).
- On one hosted SMT machine, an edit took 504 μs sequentially, 506 μs with two workers, and 728 μs with four. The source explicitly leaves spinning versus wakeup attribution unresolved (`research/investigations/recursive-offer-grain/DESIGN.md:333`).

**Inference:** better task prices alone cannot resolve all three. The policy must also control exposure, preserve access to deep work, and manage the resources consumed by idle and active helpers.

**The proposed mechanism**

I would build one parallel actualization policy around the following behavior.

1. **Execute locally until there is reason and budget to expose work.** A permitted call group or range is initially a latent opportunity, rather than an eagerly constructed task. Do not acquire a slot, fill a capture frame, publish, and join merely because permission exists.

2. **Poll at compiler-selected coarse checkpoints, not at every permitted call.** Preserve ordinary optimized loops between checkpoints. Batch across repeated short calls where their enclosing control flow permits it. A three-iteration loop invoked repeatedly should not independently call the scheduler on every invocation.

   This requires more than moving the existing query behind another branch. The compiler must account for the instructions, spills, lost inlining, and retained state introduced by checkpoints.

3. **Use worker demand to select exposure.** A worker without work records a request and parks after a bounded search. At a checkpoint, a working lane can answer demand by exposing an unstarted sibling or part of a remaining range. Keep only a small amount of queued work beyond current demand.

4. **Prefer an older, larger opportunity over the next tiny leaf.** For ranges, split the unexecuted remainder, ordinarily in halves. For recursive groups, expose an older pending sibling or continuation when possible. Continue local execution of the other part.

   This is essential: a heartbeat that merely permits every thousandth tiny call is not sufficient. It can expose the wrong granularity while missing the substantial work represented by an enclosing continuation.

5. **Make exposure replenish through work, not recursion depth.** Deep work remains eligible after arbitrarily many ancestors. A stolen task does not receive an unlimited fresh allowance merely because it changed threads. Scheduling credits are conserved when work is divided.

6. **Treat cost information as advice.** Static summaries, captured extents, sampled execution history, and optional PGO can rank opportunities and choose initial chunk sizes. A low estimate must not permanently hide an instrumentable subtree. A high estimate must not manufacture scheduling credits.

The two worlds remain useful. One-worker execution should retain the ordinary sequential world. The parallel world should execute coarse sequential regions efficiently, with cold paths for promotion. Entering an entirely uninstrumented sequential clone is appropriate only where doing so does not hide an unbounded amount of potentially useful parallel work.

This changes a current architectural decision, not merely its constants: the existing two-world design normally selects once at bootstrap, and the optional refusal path enters a sequential subtree (`design/compiler/parallel-lowering/two-worlds.md:1`, `design/compiler/parallel-lowering/two-worlds.md:5`).

There is substantial compiler work here. Retaining a latent continuation may lengthen lifetimes, require stores, prevent tail-call elimination, or require searching ancestor frames. Those costs must be explicit. Current emission already documents corresponding losses from parallel frames and calls (`compiler/src/backend/emitter/parallel.rs:19`). “Lazy” does not mean free.

**The alternatives I considered most seriously**

The strongest competing direction is **a substantially better static cost model, strengthened by PGO**. It could include symbolic extents, branch probabilities, vectorization, memory traffic, reduction preparation, and recursion summaries. For regular kernels it could expose useful work immediately, with little runtime machinery.

I would keep those capabilities as advisory inputs, but reject them as the governing policy. Static bounds on *possible* work are not reliable prices of *executed* work. Profiles add distribution assumptions rather than removing them. Payload-dependent exits, phase changes, cache residency, allocation contention, and an unfamiliar input can invalidate an otherwise excellent estimate. The existing investigation explicitly demonstrates different search costs at the same lengths and descriptors (`docs/todo.md:1801`). Arbitrary prediction error would still either flood the runtime or permanently serialize expensive work.

The second serious alternative is **a conventional work-first work-stealing runtime with a steal-sensitive partitioner**, close in spirit to Cilk, TBB, Rayon, or Java fork/join. It is simpler and proven useful in practice. I would retain its deque and execution mechanisms.

I would reject queue occupancy or steal-sensitive depth as the complete policy. An empty queue establishes demand, not that the next task is worth transferring. Conversely, a cutoff into an opaque sequential subtree can conceal later parallelism. Charging every potential fork is particularly dangerous when the compiler exposes far more fine-grained opportunities than a human normally writes.

The broader design space informs the recommendation as follows. These literature references are from memory, without a version-specific implementation audit.

| Direction | What I would take from it | What it does not establish |
|---|---|---|
| Simple static weights | Cheap ranking and obvious compile-time exclusions | A portable conversion from IR weight to elapsed time |
| Symbolic, architecture-aware static models | Extent-sensitive chunk sizing; estimated memory and reduction costs | Accurate data-dependent work or interference costs for every input |
| Lazy task creation — Mohr, Kranz, and Halstead, 1991 | Delay materializing tasks; expose older pending work | That retaining potential tasks has negligible cost |
| Cilk’s work-first approach — Frigo, Leiserson, and Randall, 1998 | Keep frequent local execution cheap; move expense toward steals | A 2% bound relative to separately optimized sequential code |
| Work-stealing analysis — Blumofe and Leiserson, 1999 | Work/span reasoning under explicit scheduler assumptions | A universal wall-time guarantee on oversubscribed, heterogeneous hosts |
| Lazy binary splitting — Tzannes, Caragea, Barua, and Vishkin, 2010 | Divide remaining ranges as demand develops | A solution for arbitrary recursive continuations or indivisible expensive calls |
| Oracle scheduling — Acar, Charguéraud, and Rainey, 2011 | Learn grain from observed execution cost | Protection against arbitrary oracle error without an independent spending rule |
| TBB auto-partitioning, Rayon splitting, Java fork/join | Steal-sensitive replenishment, queue-surplus tests, efficient local execution | Universal profitability; exact policies are version-dependent |
| Software heartbeat scheduling | Make promotion frequency proportional to executed work | Low enough metering and continuation-retention cost without compiler support |
| Timer/interrupt-driven compiled heartbeat variants | Potentially remove polling from the ordinary instruction path | Portability, bounded interruption cost, or compatibility with the weak-target requirement |

For the library lineage, Reinders’ *Intel Threading Building Blocks* is from 2007 and Lea’s fork/join framework paper from 2000. I associate Rayon’s early development with Matsakis and contributors around 2015, but am unsure of the exact date. I also recall Acar and collaborators’ later work on practical granularity control and heartbeat techniques, but would verify its precise bibliography before relying on a particular theorem or implementation detail.

The current tree rejects a heartbeat gate because it does not fix coarse overhead or skewed execution and because promoting old forks requires retaining them (`design/compiler/parallel-lowering/parallel-runtime.md:22`). That objection remains valid. My proposal differs by combining rate control with latent continuation exposure and existing join helping. **It must demonstrate that combination’s benefit; renaming the previously rejected gate would accomplish nothing.**

**What H1 can and cannot mean**

There are two separate questions:

- Can scheduling work be bounded relative to payload work?
- Can wall time be bounded relative to a different, sequential executable?

The first admits a useful conditional argument. The second does not follow from it.

For the proposed mechanism, define:

- \(U\): executed payload work in an explicit compiler cost model, excluding scheduling work.
- \(R\): additional work from metering, retained state, and altered local execution outside checkpoint slow paths.
- \(p\): maximum modeled cost of a checkpoint that publishes nothing.
- \(h\): maximum modeled lifecycle cost of an ordinary bounded-size promotion.
- \(q\): completed payload work required between checkpoints.
- \(g\): completed payload work charged per promotion.
- \(D\): fixed initialization costs for the configured execution lanes.

The intended accounting invariants are:

\[
N_{\text{checkpoints}}\le U/q+O(P), \qquad
N_{\text{promotions}}\le U/g.
\]

Credits start empty, persist across short calls, and are neither duplicated at forks nor reset on each recursive entry. Initial per-lane checkpoint costs belong in \(D\), not in an allowance renewed for each phase.

If compiler analysis establishes \(R\le \rho U\), then modeled added work satisfies:

\[
A \le
\left(\rho+\frac{p}{q}+\frac{h}{g}\right)U+D.
\]

A concrete initial allocation of the proposed 2% allowance is:

\[
\rho\le0.005,\qquad q\ge200p,\qquad g\ge100h.
\]

This reserves 0.5% for local code changes, 0.5% for unsuccessful checkpoints, and 1% for promotions.

These are proposed engineering constraints, not measured constants.

The three requested costs then have precise interpretations:

- **At an ordinary site handing nothing out:** no scheduler call, frame construction, publication, or join. Any local marker or retained-state cost is charged to \(R\); it cannot be described as zero without inspecting optimized code.
- **At a checkpoint handing nothing out:** at most \(p\) modeled work, including demand inspection and bounded opportunity search.
- **At a hand-out:** at most \(h\) modeled work for capture transport, publication, bounded search/wakeup activity, completion, and join bookkeeping. Failed attempts also spend budget.

Large captures, ancestor searches, and indexed-reduction preparation are not automatically constant-cost operations. They require separate charges. In the general case, replace \(N_{\text{promotions}}h\) by a sum of charged costs and require that sum to remain within its budget. Unbounded retries and uncharged spinning would invalidate the argument.

The most demanding requirement is the definition of \(U\). **It cannot be today’s guessed IR price.** Credits must follow work actually executed, using conservative accounting that survives optimization. Unexecuted branches earn nothing; eliminated loops earn nothing. If the compiler cannot instrument a region within the local overhead allowance, it must leave that region ordinary—and record the resulting parallelism limitation. Such refusal may fail H2; it must not be relabeled a permission gap.

An illustrative conversion shows the tradeoff. If a checkpoint cost 5 ns and a promotion lifecycle cost 1 μs, the allocation above would require roughly 1 μs between checkpoints and 100 μs of completed work per promotion. These numbers are hypothetical, not measurements. A policy with that promotion interval could miss short parallel bursts despite excellent long-run efficiency.

**When a task-price estimate is wrong by an arbitrary factor**, the accounting bound survives because predictions do not generate credits:

- Overestimation can waste a bounded number of promotions.
- Underestimation can make a poor initial split, but checkpoints retain opportunities to expose later work.
- Wrong profiles can degrade placement and balance.
- An opaque call without checkpoints can still hide parallel work until it returns.

The last limitation is fundamental to this design. Two expensive opaque independent calls might profit from immediate parallel execution, while two cheap calls with indistinguishable scheduling information would not. Past execution alone cannot reliably distinguish their first occurrence.

None of this proves H1’s literal wall-time inequality. Additional threads can change cache behavior, allocator contention, memory bandwidth, clock frequency, and OS scheduling. Moving a task to a preempted helper can delay a join far beyond its bookkeeping cost. Even a nonexecuted parallel path can alter code placement: Whitefoot’s general zero-budget-dispatch candidate regressed sequential execution that never took the new branch (`research/investigations/compute-model/DESIGN.md:5703`).

Therefore my recommendation **does not pass literal H1**, and I would not claim otherwise. It offers a model-level overhead contract plus a stringent empirical wall-time requirement under stated environmental assumptions.

**Why it could use the cores—and where it would fail**

Under sustained demand, each active worker can expose another substantial piece after accumulating sufficient work. With plentiful divisible work, startup resembles successive doublings rather than a single producer creating every leaf.

If a promotion interval takes \(G\) seconds of useful execution, idealized ramp-up to \(P\) workers costs approximately:

\[
G\lceil\log_2 P\rceil
\]

plus checkpoint, publication, and wakeup latency. For 32 workers that is five exposure rounds. This is a prediction to test, not a theorem for the proposed compiler.

Once active, ordinary work stealing should distribute remaining work. The relevant classical result is expected time of the form \(T_1/P+O(T_\infty)\) for suitable structured computations and scheduler assumptions—not a promise of 80% of every work/span upper bound.

| Workload class | Expected behavior | Principal failure condition |
|---|---|---|
| Large regular counted loops | Expose contiguous remaining ranges; run optimized loops between checkpoints | Chunking damages vectorization, or memory bandwidth saturates early |
| Balanced recursion | Expose large pending subtrees; stop spending on tiny leaves | Preserving latent forks adds excessive local cost |
| Unbalanced recursion | Replenish through actual work rather than ancestral depth | Heavy work enters an uninstrumentable subtree |
| Deep spine with side leaves | While executing a substantial leaf, expose an eligible pending suffix or sibling | No surviving opportunity is accessible cheaply; live joins exhaust storage |
| Heterogeneous task costs | Revisit splitting while expensive work runs; redistribute remaining work | Expensive indivisible leaves dominate the tail |
| Repeated tiny loops and helpers | Amortize checks across enclosing computation; avoid per-call scheduler entry | The compiler cannot batch across the relevant boundaries |
| Bounded DAGs | Schedule the readiness exposed by the lowered program | Source representation or lowering introduces phase barriers |
| Allocation-heavy work | Restrict concurrent allocation pressure; improve allocator locality where justified | Allocation contention defeats any useful concurrency |
| Mostly sequential interpreters | Leave helpers parked and keep local execution close to sequential | Frequent checkpoint or metadata work survives in the interpreter’s hot loop |
| Compute beside I/O | Share a process-wide resource budget; preserve the compute/waiting boundary | Independent pools oversubscribe or starve driver progress |

This is not a claim that granularity resolves the general DAG backlog. The current compute model can introduce routing work and barriers before scheduling even sees ready work. Nor does permission imply that an allocator scales: the recorded allocation-only example is slower at both two and four workers (`research/investigations/segmented-storage/DESIGN.md:60`).

Near-reference performance is plausible where sufficient work remains after ramp-up, opportunities are accessible, worker code is comparable to sequential code, and hardware interference is manageable. It is not assured for every class above.

**Runtime and topology are part of the direction**

I would interpret the requested worker count as a **maximum resource budget**, not an instruction to keep that many threads runnable.

The default resource policy should:

- Prefer separate physical cores before SMT siblings.
- Treat mixed core classes as unequal execution resources.
- Keep idle SMT siblings parked.
- Wake helpers on actual exposure rather than speculation about future work.
- Bound unsuccessful search and spinning, charging them to scheduling activity.
- Allow fewer active workers when bandwidth or allocation throughput has saturated.
- Coordinate compute capacity with I/O drivers and other process-owned execution pools.
- Account for affinity, CPU quotas, and oversubscription; logical CPU count alone is insufficient.

The existing runtime uses affinity-visible CPU count and reported performance levels to select a 1 ms idle window. Unknown performance-level information is treated as uniform (`compiler/src/backend/sched/core.c:1200`). I would replace that startup-only inference with conservative idle behavior and demand-based activation.

Automatic adaptation may use coarse observations at completed chunks or epochs. It should not time every call, and it cannot detect an adversarial phase change before paying for some exploration.

On weak targets, the portable path should require only ordinary local counters and native-word synchronization. No signal delivery, cycle-counter access, or timer read should be necessary at a payload checkpoint. A one-core target selects the sequential world. A two-core weak target needs a small implementation, not a desktop scheduler with its worker count reduced.

NUMA also needs inclusion: steal locally first, preserve contiguous ranges, and charge remote placement costs. SMT, asymmetric cores, and NUMA affect the profitability of *where* work runs, not just *how much* is transferred.

**The criteria need several corrections**

The most important correction to H2 is mathematical.

Consider eight processors and a computation consisting of:

- a sequential prefix taking \(S\);
- a very wide parallel phase with total work \(8S\), composed of leaves much shorter than \(S\).

Its total work is approximately \(9S\), and its span approximately \(S\). H2’s formula requests at least \(0.8\times8=6.4\) speedup.

But every scheduler must execute the prefix first, then spend at least another \(S\) on the parallel phase. Its best possible time is approximately \(2S\), giving speedup only \(4.5\).

The leaves can individually outweigh a hand-out by an arbitrarily large factor. This is not a granularity failure. **The smaller of processor count and work/span is a speedup upper bound, not a generally attainable target.**

I would revise the criteria as follows:

| Criterion | Proposed correction |
|---|---|
| Universal H1 wall-time bound | Separate a proved accounting bound from an empirical 2% wall-time requirement under a declared host/environment model |
| H2 work/span target | Compare against a realizable reference schedule, or an optimal schedule for small generated DAGs; retain work/span as explanatory bounds |
| Monotonic improvement with worker count | Permit active width below requested width; require performance close to the best usable width rather than forcing all cores active |
| “Tasks outweigh a hand-out” | Include exposure latency, wakeup cost, imbalance, and total phase duration; merely being slightly larger is insufficient |
| Fixed extra CPU for no useful parallelism | Distinguish statically no opportunities from repeatedly inspected but unprofitable opportunities; recurring inspection needs a proportional allowance |
| One set of constants | Require one policy and fixed error budgets; explicitly decide whether automatic observations and mechanically derived target costs are allowed |
| Weak-target coverage | Add actual weak hardware or a representative target; affinity restriction does not reproduce weak-core instruction, cache, memory, or atomic costs |
| Statistical verdict | Predefine uncertainty and multiple-comparison handling; excessive twin spread makes a result inconclusive, not a pass |
| Held-out set | Hold out shapes and distributions, not merely neighboring parameter values; already studied programs cannot retrospectively become unseen evidence |

The reference comparisons should report both absolute execution time and speedup. A slower sequential baseline can make parallel speedup appear better. Where possible, also compare schedulers executing identical payload code.

Additional coverage should include NUMA, CPU quotas, abrupt workload phase changes, large reduction state, nested regions, live-frame pressure, false sharing, and latency of I/O drivers sharing the process.

**The main risks and the first refuting experiment**

The largest risk is that affordable checkpoints and latent opportunities cannot coexist with the sequential optimizer. Counter updates may be cheap in isolation while retained continuations introduce spills, inhibit tail calls, or enlarge hot code enough to violate the entire allowance.

Other substantial risks are:

- Exposure arrives too late for short bursts.
- Conservative accounting prevents competitive parallelism.
- Coarse chunks conceal heterogeneous expensive tails.
- Continuation discovery costs grow with recursion depth.
- Queued-task capacity remains available while live joins retain all frame slots.
- Reduction preparation or allocation dominates the scheduling budget.
- Width adaptation mistakes input changes for hardware saturation.

**The first experiment should attack the no-hand-out path before implementing a complete new scheduler.**

Build a minimal compiler prototype containing the proposed checkpoints and latent-opportunity representation, but capable of running with helpers parked and no promotions. Compare it with the ordinary sequential build on:

- dynamically short counted loops invoked repeatedly;
- tiny balanced recursion;
- a deep spine with tiny side leaves;
- a hot loop containing data-dependent calls;
- a vectorizable range loop;
- a substantial helper whose internal loop must remain instrumentable.

Use observable outputs so the work survives optimization. Include cases where optimization legitimately removes work, to catch false credit accounting.

Freeze the accounting rules, checkpoint placement rules, and constants before performance runs.

Proposed pass/fail rules:

- Optimized-code inspection must substantiate the accounting argument; pre-optimization instruction counts alone are insufficient.
- Any reproducible no-hand-out wall-time overhead above 2%, beyond qualified twin uncertainty, rejects this implementation.
- If meeting that limit requires disabling checkpoints in the deliberately expensive, instrumentable controls, the proposal fails its exposure requirement; those cells cannot be omitted.
- An instrument unable to resolve the 2% threshold produces no verdict.
- One preregistered rerun distinguishes a persistent failure from an isolated host disturbance.

Run the smallest useful sizing sample first in CI, then choose repetitions sufficient for the fixed uncertainty target. Precise timing belongs on the 14900K runner. Include a second architecture before accepting the portable mechanism.

This experiment can reject the direction’s hardest assumption without first building the full suite or rewriting all parallel lowering.

**A staged plan**

1. **Resolve the contracts and preregister the study.** Correct H1/H2, define accounting units and all charged costs, freeze the first experiment, and record held-out families before implementation.

2. **Run the no-hand-out experiment.** Establish whether checkpoints and latent state can preserve cheap local execution. Stop this direction if they cannot.

3. **Implement one loop and one recursive vertical slice.** Support demand requests, budget conservation, one promotion path, completion, and continued eligibility below deep ancestors. Test deliberately wrong estimates, including factors of \(10^{-6}\) and \(10^6\), phase reversals, cheap prefixes, and expensive tails. The exponents are proposed stress settings, not predictions of realistic errors.

4. **Unify actualization after the mechanism earns its place.** Lower ordinary IR first, retain checked opportunity metadata, then perform one actualization transformation. Keep semantic permission, target frame feasibility, and profitability as distinct inputs to that transformation. Do not begin with a broad refactor whose performance purpose is still unqualified.

5. **Integrate resource management and expensive task forms.** Add topology-aware activation, bounded idle behavior, nested work, I/O coexistence, indexed reductions, and live-frame accounting. Diagnose allocator problems separately.

6. **Run the full visible suite, then freeze the candidate.** Compare same-source base/candidate/twin images and tuned references. Repair only from the visible set. Run the held-out verdict once the implementation and constants are frozen.

7. **Adopt only with explicit scope.** Record which classes pass, which remain unsupported, and which assumptions the accounting bound needs. Retire superseded mechanisms only after their protection cases pass.

**What I would keep, change, or remove**

| Current mechanism | Disposition |
|---|---|
| PAR-1/PAR-2 proof-derived permission and proof erasure | Keep |
| Separate sequential world | Keep and validate independently |
| Ordinary optimized loops at range leaves | Keep |
| Chase–Lev deques and structured joins | Keep initially; they remain useful execution machinery |
| Acquire storage before constructing a published frame | Keep the principle |
| Static call threshold and recursive reachability exemption | Replace as final admission rules; retain summaries as hints |
| Runtime extent expressions | Keep as optional sizing information |
| Loop entry query plus fixed power-of-two decomposition | Replace with demand-driven splitting of remaining work |
| Fixed recursive depth budget | Remove after replenishable exposure passes its protection cases |
| Refusal into an opaque sequential subtree | Restrict; it must not permanently hide substantial instrumentable work |
| Fixed 16-chunks-per-lane profitability rule | Replace with bounded queued surplus and charged exposure |
| 64-slot capacity as an accidental grain policy | Keep safe refusal initially; separate queue capacity from live-join storage and qualify deep cases |
| Startup-only idle-window selection | Replace with conservative parking and controlled activation |
| Per-call online suppression | Do not revive; observation belongs at coarse checkpoints and task boundaries |
| Parallel ledger | Extend with separate permission, feasibility, exposure, budget, and observed-execution reasons |

The runtime currently holds 64 slots per lane and uses a 256-byte frame bound (`compiler/src/backend/sched/core.h:5`). Those are useful implementation limits, but they do not establish profitable granularity or a general space bound.

The actualization refactor is already recorded as an architectural opportunity: policy is spread across lowering, call-grain filtering, emitter fitting, and launcher decisions (`docs/todo.md:2074`). This study supplies a concrete reason to reopen it, after the first experiment.

**Backlog disposition**

The following are proposed dispositions, not claims of completed fixes. IDs use the brief’s names.

| Items | Disposition under this direction |
|---|---|
| `coord-wfbl-03-27` — general granularity; `03-59` — provisional call grain | Central research deliverables |
| `03-28` — unavailable helper extents | No longer a prerequisite for discovering expensive instrumentable work; useful optional prediction improvement remains |
| `03-29` — loaded per-task costs | Directly addressed through execution-driven exposure; opaque leaves and tail imbalance remain |
| `03-58` — fixed recursion frontier | Directly addressed by work-based replenishment and retained opportunities |
| `03-61` — excluded recursive components | Depth-budget exclusion becomes obsolete; clone reachability and compute entry from waiting contexts still need correctness work |
| `03-54` — tiny-loop query costs | Directly addressed; an early protection case |
| `03-39` — scattered actualization | Addressed by the eventual common transformation |
| `03-60` — idle workers on SMT | Directly addressed by activation and idle policy; existing attribution still needs resolution |
| `03-23` — capture pruning; `03-34` — missing worker alias facts | Remain compiler-quality work; granularity does not recover lost optimization |
| `03-22` — initialization span; `03-25` — DAG scheduling; `03-31` — scatter construction/packing | Remain algorithm/lowering work, with scheduling comparisons separated from it |
| `03-55` — small allocations | Remains allocator work; reduced concurrency is only a mitigation |
| `03-30`, `03-32`, `03-48` — placement and attribution | Remain measurement prerequisites; alignment alone does not close them |
| `03-35` — stale benchmark adapters | Repair before using those experiments |
| Permission items `03-00`, `03-33`, `03-36`, `03-37`, `03-38`, `03-57`, `03-64`, `03-65`, `03-66`, `09-19`, and `sg-bl-01-02` | Remain separate semantic, lowering, or validation work |
| `03-56` — inline-range facts, reported resolved by the brief | Not reopened by this policy |

Two boundaries deserve emphasis. The missing Box-owner read is a checker correctness issue, currently masked for the described non-call pair by lowering restrictions; broader continuation exposure must not bypass that restriction before the defect is fixed (`docs/todo.md:2047`). The waiting-recursion issue does not authorize executing waiting operations as compute tasks (`spec/kernel-spec.md:2156`, `docs/todo.md:2395`).

The decision I would put to the owner is to fund the **small checkpoint-and-continuation experiment**, under corrected criteria. It tests the central claim this direction depends on: that Whitefoot can preserve cheap sequential execution while retaining enough opportunities to expose substantial work when other cores need it.
