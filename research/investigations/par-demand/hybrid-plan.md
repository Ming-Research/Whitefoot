# A hybrid plan after experiment 2

Written by Codex (gpt-6-astra, read-only, extra-high reasoning) on
2026-10-10 from the whole study ([DESIGN.md](DESIGN.md)) and the code at
revision 9324e6551, after experiment 2's decisive run, as the owner directed
for a failed fix round of demand-driven hand-out. It is a proposal awaiting
the owner's direction decision, not an accepted design; file:line
references are to that revision.

I would **replace demand-only hand-out with a hybrid: expose a bounded frontier of work eagerly, use demand to replenish it, and select an unmodified sequential version around cheap regions.** Idle workers should wait for published work, with a short, bounded spin period tied to an actual producer—not a standing one-millisecond window or a single designated spinner.

This is a proposed experiment, not a claim that the owner’s bar has been met. A literal guarantee for every future input is stronger than an online scheduler can provide: discovering whether unfamiliar work benefits from parallelism itself costs time, and previous samples cannot guarantee the cost of the next input. The implementation should eliminate overhead where the compiler can establish that scheduling is unnecessary, and expose the remaining uncertainty through strict measurements rather than a per-decision allowance.

I inspected the clean `claude/par-demand` worktree at `9324e6551`. No files were changed and no builds, tests, or experiments were run.

**1. Diagnose the failures before changing the policy.**

The decisive evidence is [DESIGN.md:1466](DESIGN.md#experiment-2s-decisive-run-thirty-rounds). Two qualifications matter:

- `mandelbrot` W8 is a confirmed demand E2-keep failure. Demand’s `recursion` W8 exceedance did not become a confirmed failure after rerunning; `stencil` W8 is void. Those are diagnostic leads, not additional established failures.
- E2-H3 establishes excessive process CPU under its formula. It does **not** by itself establish that the CPU was spent waiting. The earlier CPU breakdown specifically found that removing the long idle window did not explain stencil’s extra CPU.

The following references use repository-relative `file:line` locations.

| Failure | Evidence and most likely cause | Measurement and falsifier |
|---|---|---|
| **Mandelbrot W8: demand/par 1.215 [1.198, 1.243]** | `builder/demand.rs:83–148` polls only at slice boundaries, then bisects the remainder. `sched/core.c:1010–1014` asks one randomly selected victim after a failed scan; `:1571–1577` clears that victim’s request on publication. Multiple idle lanes therefore do not constitute a counted request for a ready frontier. **Inference:** delayed exposure of expensive remaining work causes startup or tail underutilization. There is also a scheduling-order difference: demand publishes the far half; the legacy splitter publishes the near half (`builder/split.rs:1267–1305`). | Record request→publication→execution latency, ready-task count, actual executing lanes, and remaining index ranges. Compare demand with **eager seeding only**, retaining its chunks, grain, and idle policy. Separately reverse publication order without changing anything else. Reject latency as the main cause if execution is already well balanced and eager seeding removes neither idle gaps nor the wall loss. Reject an order explanation if the order-only arm is neutral. |
| **Recursion W8: demand/par 1.080 [1.052, 1.099] initially** | `emitter/parallel.rs:770–793` gates each eligible offer on demand. But `emitter.rs:1824–1829` passes the decremented recursion budget to group members whether an offer was published or refused. At the cut, `:1863–1911` enters a sequential clone; stolen callbacks receive the carried budget (`parallel.rs:728–737, 813–817`). **Inference:** demand can spend its opportunity to expose work before enough lanes are occupied, then strand work below the cut. Repeated requests and their lifetime-protection atomics are another possible contributor. | Trace frontier depth, remaining budget at successful steals, budget spent without publication, and idle time while sequential leaves execute. Compare **eager offers above the existing cut only** with demand. Keep the budget and sequential leaves identical. The explanation fails if both expose comparable frontiers before the cut, or eager offers do not recover wall time. Do not simply increase the budget. |
| **Stencil: W8 demand/par 1.119, void; W4 H3 margin +0.257 [0.245, 0.290]** | Demand bypasses the runtime-extent estimate: `emitter/parallel.rs:928–929` returns into demand emission **before** `:941–953` evaluates `split.work`. Demand instead passes the static weight at `:1155`. `builder/work.rs:435–475, 547–630` contains the available nested-extent machinery. The earlier inspected image used an outer threshold of roughly 505 rows and bypassed the inner row driver at width 1024 (`inspection-38034318254.json:47–50`). **Inference:** coarse row batches and repeated phase startup hurt balance. Memory traffic and scheduling work may explain much of H3: the earlier W4 zero-window comparison changed CPU only from 1.548 to 1.528 times sequential (`DESIGN.md:1063`). | Separate initialization and each timestep; record exclusive useful execution, scheduler execution, spinning, sleeping, and chunk dimensions. Compare **runtime-extent pricing only**, **eager seeding only**, and the two together. Separately compare zero window with zero spin floor. Reject the coarse-grain explanation if smaller properly priced batches do not improve occupancy or wall time. Reject waiting as the H3 explanation if CPU remains excessive during useful execution after waiting CPU is removed. |
| **`small_split`: demand/seq 1.113 at W4, 1.104 at W8** | The timed extent-three path **does not execute a TLS poll**. `emit_demand_split`, `parallel.rs:1133–1145`, computes a saturated span and branches before entering the driver; the saved inspection confirms bypass (`inspection-38034318254.json:7–10`). Earlier assembly attributes cost to the guard, escaped captures, and spills (`DESIGN.md:587–606`). **Inference:** the remaining loss is repeated dispatch and its effect on optimization, not idle requests. | Inspect the decisive image, then compare separately: capture transport improvement; region-level guard hoisting/versioning; both. Require the selected cheap loop’s instructions, register traffic, vectorization, and unrolling to match sequential. The diagnosis is falsified if removing the repeated decision and associated spills leaves the loss; then investigate code placement and caller optimization rather than widening slices. |
| **Histogram W8: H3 +0.152 [0.085, 0.230]** | The inspected image parallelizes block counting but bypasses both the bucket driver and nested merge driver: 256 buckets are below the static threshold, and 16,385 counter rows are below the nested threshold (`inspection-38034318254.json:57–60`). The source’s distinct merge phase is `tests/programs/compute/histogram.wf:75–77`. **Inference:** workers spin through a substantial sequential merge, while demand’s omission of runtime extents may unnecessarily leave that merge sequential. The historical zero-window arm reduced CPU substantially (`DESIGN.md:1067`). | Attribute CPU separately to allocation/zeroing, block counting, tail, and merging. Compare producer-aware parking and runtime-extent pricing independently. Reject the waiting explanation if idle CPU is small. Reject missing merge parallelism as useful remediation if pricing exposes it but strided-memory contention prevents a wall gain or violates H3. |
| **`idle1` loses speedup** | It still gives **every lane 1,024 initial rounds**; only the extended window is single-spinner (`core.c:1180–1196`). It also changes join parking, so the experiment did not isolate the spinner rule (`DESIGN.md:1306–1313`). **Inference:** it retains a costly initial scan/request phase while making later work wait for sleeping lanes to wake. | Measure ready-work→execution latency and first-round scan CPU. Separate the join-wakeup change from the spin policy. Reject this reading if neither additional wake latency nor scan CPU accounts for the tradeoff. Do not retain `idle1` merely because histogram’s CPU improves. |

There is a useful correction to the workload description: **this Mandelbrot fixture is a flat point map, not a row loop.** `tests/programs/compute/mandelbrot.wf:38–41` maps points; `mandelbrot_oracle.c:144` selects 98,304 points with the “trailing” distribution. Lines 78–81 place the expensive points in the final quarter. That distribution makes publication order and delayed subdivision particularly important, without requiring any workload-specific scheduler rule.

Two additional investigations belong in this diagnostic batch:

- **Request contention after the exit fix.** `core.c:409–413` performs atomic increment/decrement around every attempted request, even when the word is already set. Repeated failed scans can repeatedly contend on the same victim’s `request_users`. The crashing pre-fix arm did not settle its cost (`DESIGN.md:1388–1403`). Measure attempted requests versus newly posted requests and CPU in this path. Compare safe request deduplication against the current safe protocol; never use the unsafe predecessor as a timing control.
- **Protect the gains.** Demand’s W8 demand/par ratios are **0.277 for `large_helper`, 0.502 for FIR, 1.014 for records, and 1.023 for prefix**. The legacy deque-nonempty veto in `core.c:1683–1688` plausibly explains `large_helper`’s old ceiling, but must be traced. FIR’s large gain needs a chunk-code and scheduling comparison before replacing its driver. Returning wholesale to the old eager implementation could lose these improvements.

Instrumentation should use per-lane buffers and counters, with clocks at region, leaf-chunk, publication, wake, and completion boundaries. Existing `WF_PAR_TRACE` needs extension: its call-head hook is reached from `split_budget`, not demand entry, and its callback durations include nested work and waits. Summing those durations would double-count CPU. Its calibration probes also perturb execution (`core.c:518–529`); diagnostic traces must remain separate from uninstrumented timing verdicts.

**2. Implement the hybrid in four separable changes.**

**First, make cheap regions genuinely sequential.**

Keep the call-grain fix that removed `hot_helper`’s offers. Extend scheduling eligibility from isolated sites to the enclosing region:

- Construct scheduling predicates from available extents and cost summaries.
- Move an invariant predicate to the enclosing loop preheader or function entry.
- Generate two versions there: the ordinary sequential region and the scheduling-capable region.
- Bind calls in the sequential version directly to sequential bodies before optimization. Do not merely put a cold driver branch beside every tiny call.

For `small_split`, `extent` is invariant across the walker. On an iteration where the existing non-wrapping guard succeeds, `hi − lo` equals that extent. A single outer decision can select the entire sequential walker; all original bounds and wrap guards remain. There should be **no scheduling comparison, TLS access, clock, frame preparation, or counter update per `mark` call**.

Implement predicate transport and simplification alongside `builder/work.rs`; consume it in lowering before `emit_demand_split` and clone selection. Keep two region versions, rather than generating a combinatorial family for every combination of sites.

Also examine `split.rs::split_counted_range` capture construction. It already snapshots promoted local Box owners (`:334–367, 570–575`); that does not automatically solve a captured `&Box` formal. Extend loaded-pointer transport only where the retained write footprint establishes that the owner slot cannot change. Preserve cleanup ownership and structured joins.

This differs from the previously rejected all-site zero-budget branch: it removes repeated branches from an enclosing hot region. Nevertheless, the recorded records regression from changed caller optimization remains a mandatory counterexample to test.

**Second, give substantial counted loops work before waiting for requests.**

Change `build_demand_driver` and `emit_demand_split` to accept a region plan containing:

- the runtime-extent estimate already produced by `work.rs`;
- a minimum profitable chunk size;
- a bounded split budget;
- the capacity assigned to this region.

At entry, create a **bounded eager frontier**. Retain the existing 16-chunks-per-lane ceiling as the initial experimental maximum; do not introduce a newly tuned cap. Split descriptors can remain recursive and lazy, but a stolen descriptor must subdivide its inherited budget before entering a long leaf, without waiting for another request. A single eager bisection is insufficient for eight lanes.

After the initial frontier exists, use demand to expose additional remaining or nested work at chunk boundaries. Keep the current outlined chunk body initially; the failed caller-local/forced-inlining rounds are evidence against combining another loop-shape rewrite with this scheduling experiment.

Remove the rule that any queued sibling prohibits nested splitting. Replace it with **capacity accounting shared by the enclosing parallel region**: a queued task occupies an opportunity, not the entire machine. This is necessary to retain `large_helper`’s gain.

Use one scheduling estimate for eager splitting, demand refinement, and nested-loop decisions. Static weight remains a fallback for unavailable information, not a substitute for an available row width or helper extent. Scheduling arithmetic remains total and separate from acceptance.

Fast-path costs should be:

| Path | Intended scheduling cost |
|---|---|
| Statically pruned region | None |
| Invariant cheap region | One outer selection; none inside its hot loop |
| Substantial region entry | Estimate, capacity selection, bounded initial publication |
| Executing leaf | Ordinary optimized loop; no per-iteration scheduler operation |
| Chunk boundary needing refinement | Availability read and local arithmetic; publication only for worthwhile work |
| Sequential recursion below cut | None |

Initially retain the measured work-unit floor. Then qualify coarse timing samples to calibrate actual chunk duration and publication cost. Sampling belongs at region/chunk boundaries, never at every call or iteration. A first sample or phase change can still be wrong; calibration is not a proof of profitability.

**Third, make recursive offers depend on useful work on both sides.**

`call_grain.rs:136–146` prices the published member while preserving the source-last join member. Thus a large recursive child can remain eligible even when the only local companion is a tiny leaf—as in `spine`.

Change group selection to examine the work that can actually overlap:

- Keep a group when at least two independent portions provide enough concurrent work to justify publication.
- Suppress a group whose only substantial member would be handed away while the owner performs negligible work and immediately waits.
- Recompute recursive-offer reachability after pruning, as the existing fixed-point pass already does.

For `spine`, this should remove the unproductive group and consequently the budgeted parallel recursion, allowing the ordinary accumulating-recursion optimization. This must be demonstrated in the emitted code, not inferred from pruning alone.

For balanced recursion, eagerly expose the bounded frontier above the existing cut. Keep the sequential clone below it. Demand should replenish useful frontier work, not gate every budgeted node.

Do **not** unconditionally reset a stolen task to a fresh full recursion budget: that can recursively multiply fine-grained offers. Any refresh needs a conserved region allowance and evidence of substantial remaining work. Changes belong in `call_grain.rs`, `emitter/frontier.rs`, `emitter.rs::callee_target`, and `parallel.rs::emit_handed_out_call`.

**Fourth, make idle capacity persistent and wake it from publication.**

I would replace repeated random writes into owners’ TLS with a scheduler-owned availability protocol:

- A lane announces availability when it becomes idle; the announcement survives parking.
- A producer consumes available capacity only when it has actual worthwhile work to publish.
- Publishing several ready tasks wakes the corresponding number of lanes, rather than relying on one spinner to discover and distribute them.
- A worker performs a bounded final scan and parks when no ready work or immediate publication is known.
- Joiners remain available for other work while awaiting their target.

The availability record lives in permanent scheduler storage. Updates happen on idle/active transitions, not every failed scan or every source call. This removes the demand-word lifetime problem without weakening the exit fix; retain the existing safe implementation until the replacement passes its concurrency tests.

Use the existing announce/final-scan/posted-lock protocol as the correctness foundation. `core.c:1278–1308` already contains the joiner-availability variant, independently useful even though the combined `idle1` policy failed. Preserve deque ordering, completion ordering, and frame lifetime.

Keep only a short spin allowance measured against park/wake latency, and only while work is plausibly imminent. No long window during a known sequential phase. Start the pool lazily after a substantial region passes admission; demand currently starts it during world selection (`core.c:1655–1667`), so cold cheap programs can pay startup even when their steady-state cell looks good.

If stencil’s excess CPU remains in **useful memory-bound execution**, parking will not solve H3. Add region-level width selection only after the phase measurements establish that case. Its objective is the smallest team giving indistinguishable completion time, using coarse comparable work samples and hysteresis. Sampling cannot replay side effects, and irregular samples cannot be assumed comparable. If fewer lanes materially lose wall time while more lanes necessarily fail H3, this scheduler design fails the combined requirement; do not hide that conflict by choosing whichever metric looks better.

The expected treatment of every manifest workload is:

| Workload | Intended behavior |
|---|---|
| `small_constant` | Prune and preserve whole-loop folding; no pool startup for eliminated work. |
| `small_split` | Select the sequential walker once; remove repeated scheduling decisions. |
| `hot_helper` | Preserve static group pruning and its measured sequential performance. |
| `spine` | Remove one-sided, unproductive recursive offers; preserve sequential loop optimization. |
| `recursion` | Eager bounded recursive frontier; optimized sequential leaves. |
| `large_helper` | Split both useful helpers within shared capacity; no deque-nonempty veto. |
| `mandelbrot` | Ready frontier before expensive chunks begin; demand refinement for skew. |
| `records` | Same frontier policy; preserve caller/capture quality and test irregular record lengths. |
| `fir` | Preserve strict arithmetic and current good leaf code; scheduling changes must retain demand’s gain. |
| `stencil` | Price actual row width, expose each timestep promptly, retain joins between steps, avoid useless nested teams. |
| `prefix` | Parallel block phases; parked excess lanes during the dependent middle scan. |
| `histogram` | Price both block and merge phases correctly; avoid nested teams and spinning through sequential work. |

No workload name, manifest entry, or benchmark-specific annotation enters these decisions.

**3. Pre-register the experiment before implementing the combined candidate.**

Use the existing CI route: `compute-bench.yml`, `experiment=par-demand`, with the 14900K reached only through its CI runner. Hosted CI builds, verifies, and sizes the images first. Coordinate the runner slot before measurement.

Extend the harness explicitly; its current arm and width lists are fixed in `summarize.py:20–22`, and `measure.py` hard-codes their settings. Do not silently repurpose `idle1` to mean the new policy.

The sequence should be:

1. **Diagnostic batch:** the isolated changes described above, with the current demand implementation and its identical-image twin as controls. Start with the smallest useful sample; use separate instrumented runs for attribution.
2. **Six-round sizing batch:** all twelve workloads, widths 1, 4, and 8. It selects duration and checks measurement quality; it judges nothing.
3. **Frozen decisive batch:** **30 interleaved rounds**, then exactly one further 30-round attempt for every exceeding cell. Do not pool sizing rounds or change thresholds after seeing results.

The decisive arms should be:

- sequential;
- shipped eager `--par`;
- frozen current demand;
- hybrid with the existing idle policy;
- complete hybrid with the new idle policy;
- byte-identical twins of current demand and both hybrid arms.

Use the same source, input, repetitions, compiler/toolchain settings, and affinity set for each cell. Keep one logical CPU per performance core and archive the topology and image hashes. Preserve first and second calls separately; the second remains the existing steady-state statistic.

Use **the existing paired calculation unchanged**: per-round ratios, median, 10,000 bootstrap medians, 95% interval, fixed seed `20261010`; a twin/candidate interval excluding 1 voids its comparison. A first exceedance requires the specified rerun; disagreement is inconclusive. Missing inspection cannot pass.

Pre-register these criteria:

- **Sequential performance:** remove the one-nanosecond-per-decision allowance from the new candidate’s acceptance criteria. Report the old E2-H1 verdict separately for historical continuity. For the owner’s literal “no slower” claim, require the upper interval of candidate/seq to be **≤1.00**. An interval straddling 1 is inconclusive, not proof of equality. A separate ≤1.02 engineering-equivalence result may be reported, but must not be called satisfaction of the literal bar.
- **Keep eager speedups:** retain E2-keep, upper candidate/par **≤1.05** wherever par/seq is confidently below 1.
- **Keep demand’s gains:** upper candidate/current-demand **≤1.05** wherever current demand has a qualified speedup over sequential. This prevents “fixing” Mandelbrot by sacrificing FIR or `large_helper`.
- **CPU:** retain E2-H3 exactly: the upper interval of its CPU margin must be **≤0** in every applicable cell.
- **Idle-policy qualification:** complete-hybrid/hybrid-with-old-idle upper wall ratio **≤1.02**, with CPU reported alongside it. No CPU saving purchases an otherwise rejected wall regression.
- **Full scope:** `spine` participates in the new candidate’s verdict. `small_constant` remains a code-elimination and cold-start control when its hot work folds away; its microsecond timing cannot qualify surviving-loop overhead.
- **Cold behavior:** report first-call wall and total CPU against sequential directly. CPU-above-wall alone does not establish startup overhead. A cheap program must not pass solely because sample zero absorbed pool creation.
- **Correctness:** every arm must retain the complete independent-oracle result. CI must exercise empty/tiny ranges, boundary arithmetic, nested splits, reductions, frame refusal, partial startup, join wake races, and owner-thread exit.

After the primary panel, run a pre-registered generality panel through the same instrument: width 2 for saturation diagnosis; extents around grain boundaries; leading/interleaved/trailing irregular work; cheap→expensive and expensive→cheap phases; repeated short invocations. Freeze these cases before tuning. The main manifest alone cannot establish a claim about all future programs.

Also correct the harness README in the implementation change: it currently describes the rejected caller-local slicing and superseded spread-based noise rule. The current code and `summarize.py` are the relevant experimental machinery.

**4. Risks and abandonment conditions.**

- **Compiler optimization remains the largest cheap-path risk.** Region versioning or capture changes may alter inlining, register allocation, or placement even in the sequential arm. Reject a version that cannot preserve the cheap path; do not compensate by increasing its allowance.
- **An eager frontier can overproduce work.** Retain grain and capacity bounds, then reject the hybrid if it recovers Mandelbrot by recreating excessive offers elsewhere.
- **Runtime extents are estimates, not time.** Early exits, vectorization, cache behavior, and input skew can invalidate them. Calibration must be measured with its own cost included.
- **Parking may cost more than the work it saves.** If producer-driven wakeups still lose the required speedups, the new idle policy fails. The rejected single-spinner design is not the fallback.
- **Memory-system cost may make H3 and E2-keep incompatible for a workload.** Establish that with phase and width measurements. If so, stop claiming this is merely an idle-policy defect.
- **Unknown cheap work remains a fundamental boundary.** If the compiler cannot move or eliminate its decisions, and sampling still causes a measurable regression, this design does not meet the universal bar. Asynchronous promotion would then be a separate architectural investigation, not an unmeasured escape hatch.
- **Portability is unqualified.** ELF’s cheap TLS behavior does not establish other targets, and the present demand prototype explicitly rejects Windows at `core.c:379–380`. Qualification must eventually include supported targets and fragmented compilation.

I would abandon the combined candidate after its prescribed rerun if any valid cell loses sequential performance, required speedup, or H3; if its gains depend on workload-specific thresholds; or if the causal ablations fail to support the proposed mechanisms. The next implementation round should therefore begin with attribution and cheap-region elimination—not another adjustment to the global polling interval.