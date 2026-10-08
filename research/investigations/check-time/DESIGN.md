# Wide-match checking cost

## Question and prior criterion

Q140 authorizes investigating the cost of checking a wide `match`. The owner
has selected Q155 direction A2: retain unchanged-state reuse and make index
construction demand-driven. The previous full-index prototype's M5 results
below motivate this change. The next CI comparison uses the merge base of the
measured head with current main, a byte-identical base twin, and head. Builds
are separate from checks, and all performance measurement runs on the
self-hosted 14900K through CI. No machine settings are changed.

Prior criterion for the next timing experiment, retained from the resumed draft
and recorded before running that experiment: for the same source
generator with 40, 80, 160, 320 and 640 arms, checking time must grow by no
more than 2.5 times at each doubling. Every corpus and conformance verdict
must remain unchanged. The existing eager closure and eager join are the
differential oracle for unchanged state transitions; the full uncached affine
index rebuild is the prototype's direct oracle. Retained derivations must satisfy the validators in
`compiler/src/semantic/tests/entailment.rs`. A faster incorrect or incomplete
answer rejects the proposal. An unchanged cubic work count or a doubling
above 2.5 rejects the scaling claim even if the largest input gets faster.

Compare unchanged base, a byte-identical base twin as a noise control, and
the prototype on identical inputs, interleaving their order and repeating
short runs. Start with the smallest sample and inspect its duration before
scaling. Measure builds separately. Time ordinary `--check` with counters
unset. Include the maintained generator's natural stage-3 interpreter and the
plain arm series. Historical M5 `nat.wf` and optional loop-wrapped inputs are
separate panels, not implicitly pooled with these current inputs. Record
machine, compiler and settings, all exit statuses, run order, spread, and
limits of attribution.

This task permits source reads and edits only: no local builds, checks,
formatting, tests or timing, no git staging or commits, and no approval-log or
specification edits. All executable validation listed below remains for CI.

## Scope and alternatives

[ENT-4] and [ENT-5] in the active specification define derivability and
join results, not matrix construction. [DIAG-2] requires retained valid
derivations. [MSR-4] fixes the query route and finite candidate order.
None of the proposed directions changes those rules.

1. A1, reduce join materialization: compute per-term information first and
   retain only rows or pairs whose joined result cannot be rederived.
   Keep forward flow, ordinary closure, affine queries, kills and snapshots.
2. A2, reuse demanded affine index entries for unchanged closed facts and
   ordered candidate images, including ordinary queries and certificates.
   Exact-vector lookup and lazy final-family traversal implement this direction.
3. B, demand-driven proof states: retain point/edge predecessors and answer
   obligations backward, memoizing bound queries; give affine queries and
   every MSR-4 step the same demand-driven treatment.

The prototype implements A2. A1's implicit-singleton join reduction remains
removed: real arms do not assign the series' constants and a join-local change
cannot reach the measured index hotspot. B remains a broader alternative.
Neither the selected implementation direction nor timing authorizes a change
to acceptance or selected derivation parents.

## Supplied evidence

The supplied artifacts are outside the repository. Their SHA-256 identities
allow CI to use the same bytes; they do not identify the revisions of the
old compiler binaries, which the TSV schema does not record.

| Artifact | SHA-256 |
| --- | --- |
| `nat-work-base.tsv`, before C2 | `202d5f61dff3b66d46e73abf06fedae558b043bc51bb41821cd773ed8953c0c2` |
| `nat-work-c2.tsv`, with C2 | `84b479c2adbaf67256337cae76f5954dd23544bc952f7d33bee1f66693ea3a6e` |
| `series-gen.py` | `d77434bb9fd46c6e1698fbd5d006033568cd437ea52ccae67567f6f913491e57` |
| `nat.wf` | `dc8c0e703bcf9df033d992a27c755f70407c46adbe23e2ab8fe97a91f4d39f71` |

The requester reports 0.24, 1.55 and 15.1 seconds on the Mac at 160, 320
and 640 arms: **6.46× and 9.74× per doubling**, local log2 slopes 2.69 and
3.28. This supports investigating a cubic term, but three times without
repeats do not establish an exponent or separate its causes. The reported
318-arm `loop { match }` interpreter checks in 7.6 seconds. Its different
arm bodies and joins make it another workload, not another series point.

The [counter schema](../../../compiler/src/semantic/entailment/work.rs)
identifies each analysis by process and run ID, then function, kind,
ordinal, metric and unsigned value. These files each contain one `run`
and one `numeric_op` analysis. A pass's interning subtotal includes input
closures and proof preparation; it is already included in the function
total. `parent_references` includes all join node families and interning
attempts. Occupied closure cells are not relaxation counts or elapsed time:
a cheap reconstruction of a recorded-closed view also increments them.

| Function / metric | Before C2 | With C2 |
| --- | ---: | ---: |
| `run`: joins / passes | 482 / 709 | 378 / 552 |
| `run`: evaluated = retained pairs | 247,524 | 155,976 |
| `run`: largest union rows / inputs | 54 / 2 | 38 / 5 |
| `run`: join parent references | 182,345 | 143,992 |
| `run`: interning within passes | 177,654 | 135,248 |
| `run`: total interning | 1,710,477 | 1,771,353 |
| `run`: closures / occupied cells summed | 5,167 / 1,942,393 | 5,918 / 2,183,788 |
| `run`: snapshots / occupied cells summed | 1,088 / 555,278 | 1,101 / 565,804 |
| `numeric_op`: joins / passes | 123 / 123 | 123 / 123 |
| `numeric_op`: evaluated = retained pairs | 658,870 | 658,870 |
| `numeric_op`: largest union rows / inputs | 125 / 1 | 125 / 1 |
| `numeric_op`: join parent references | 0 | 0 |
| `numeric_op`: pass / total interning | 10 / 25,155 | 10 / 25,155 |
| `numeric_op`: closures / occupied cells summed | 124 / 658,871 | 247 / 1,317,741 |

C2's `run` has 342 one-input passes, 200 two-input passes, nine passes with
three through five inputs and one empty pass. There is no 318-input join
in these counters. Its pair count falls 37.0%, yet total interning rises
3.56% and occupied closure cells summed rise 12.43%; only 7.64% of its
interning attempts occur inside join passes. Across the C2 file, joins
evaluate 1,180,099 pairs; `numeric_op` accounts for 55.83%. Total interning
is 2,703,433, with 301,889 (11.17%) inside passes. A reduction in join
parents alone cannot be asserted to solve the interpreter's elapsed cost.

The exact sum of `inputs * pairs_evaluated` is 244,043 for C2's `run` and
658,870 for `numeric_op`. These upper-bound the old pair loop's numeric
reads, since contradictions and absent bounds can skip reads; they exclude
proof lookups. Zero parent references rule out parent-vector construction
as `numeric_op`'s leading join cost. Its unchanged interning alongside
doubled closure counts also rules out treating every new closure count as
a newly executed cubic fixed point.

## Profile of the stage-3 interpreter's check

Observed on 2026-10-08 on the Apple M5 under the host lock: macOS `sample`
at 1 ms over the whole `whitefootc --check` of the stage-3 interpreter
written as `loop { match }` (`gen-out/nat.wf`, 318 arms), with the gate
compiler built from 05d8a4e08 (PR #270, C2 included). The checking thread
has 4,842 samples. Inclusive, 3,098 of them (64%) are in
`affine_l0_index`, every one called from `Reasoning::affine_target_proof`.
By top of stack, the leading entries are allocation and freeing (about
1,600 samples together), `affine_l0_index` itself (637), rehashing and
inserting into the `HashMap<Box<[AffineCoefficient]>, usize>` the index
builds (about 900), `affine::merge_scaled` (330) and `ClosedState::value`
(138). Join construction does not appear among the leaders.

`affine_target_proof` reuses a built index only when its `ProofContext`
carries a closed view that matches, which certificate checking supplies;
every other affine query rebuilds the index from all pairs of
`affine_l0_candidates` (every measure term with an image and every live
integer binding), hashing a boxed coefficient vector per pair. In a large
function that is a quadratic rebuild per query at points whose state has
not changed since the previous query.

So for this program the cost that matters is the affine query's index, not
the join: a fix confined to `join_at_once` cannot reach the 64%.

## Source attribution

Let A be input edges, U the union of active closure rows, r_i each input's
rows, and B the stored pairs needing new join proofs. The supplied series
has U proportional to A because assignments introduce distinct literals.

| Work at the base in `state.rs` | Scaling and evidence | Still unresolved |
| --- | --- | --- |
| Pair enumeration in `join_at_once` | Allocates and visits U² pairs, including diagonals: quadratic time and temporary space. | Wall-time share. |
| Per-input `value` and selected-proof lookups | Up to A reads per pair: O(A U²), cubic on the series. Passive literal bounds are present, so absence does not terminate those scans. | Actual read count and wall time. |
| Parent construction | Up to A `bound_proof` calls and A `JoinParent` entries for each of B non-reused pairs: O(A B). Union-only literals often have no common stored proof. | B is not separately reported; parent counts include other join families. |
| Interning | Attempts precede deduplication. Hashing, comparing and traversing a join node visits its A parents; passive bound proofs also intern implicit/transitive nodes. O(A B) parent work can coexist with only O(U²) unique join nodes. | Attempts are not unique nodes or CPU time; most `run` attempts are outside joins. |
| Input closures | Cache hits reuse an Rc; recorded closures reconstruct views; unseeded closure has cubic row products per round, while insertion and repair differ. Small r_i in the series can coexist with quadratic total scans/allocations over a growing global term inventory. | TSV does not split closure routes, rounds, products or input closure time. |
| Post-join closure | The complete store is marked closed; `close_with_row_pruning` can copy it and re-emit implicit bounds without a transitive fixed point. Universe selection still scans U² stored cells; later kills, facts and term growth can require closure. | No evidence establishes a cubic post-join fixed point on the series. |

Thus predecessor scans and parent work have cubic constructions; pair
enumeration alone does not. No supplied measurement ranks those phases in
seconds. The current timing panel compares total checking cost; the historical work
counts above do not establish phase durations or affine-cache hit rates. A
split of these costs would need a separately authorized profile or new
counters; the current protocol includes neither.

## Direction A1: reduce or demand the join locally

Keep the current source walk, C2 frontiers and per-edge induction, affine
images and MSR-4 order. Change construction behind `join_at_once` in two
possible stages:

1. Aggregate joined zero rows once. If implicit facts fix `t=c`, every
   non-contradictory input has `d_i(x,t)=d_i(x,Z)-c`; taking the maximum
   commutes with that shift. Compose the joined zero bound and implicit
   bound in one transitive proof; scan inputs for unresolved pairs.
2. Avoid storing rederivable cells: keep joined zero rows, residual pairs
   and common disequalities. Alternatively, retain immutable predecessor
   closures and memoize `max_i d_i(x,y)` per demanded pair and layer.
   Unknown, absent and derived cells must be distinct states of the view.

A sparse variant can omit implicit singleton rows algebraically. For
other pairs, compute per-term zero maxima and the first attaining input.
If both maxima have the same attaining input and its pair equals their
sum, it witnesses that the joined zero path is exact. This is sufficient,
not necessary; undecided pairs are computed exactly and retained when
tighter. Zero maxima alone lose correlation: inputs `x=y=0` and `x=y=10`
join to `x-y<=0`, whereas their independent intervals permit `x-y<=10`.

Common disequalities are separate: `x=-1` and `x=1` both imply `x!=0`
while their joined interval does not. For passive constants, a per-term
sorted union of predecessor intervals can decide which points every input
excludes; genuine retained exclusions still require all input proofs.
General explicit disequalities and implicit components keep exact treatment.

Sparse representation must update `ClosedState::cell`, the closed-store
marker, materialization, ordinary fallbacks and delivery enumeration
together. Deleting cells while retaining `ClosureRecord::Closed` is
incorrect. Before a kill, conclusions whose endpoints survive must remain
available even if their proof used a middle that dies. A lazy snapshot may
retain a frozen pre-kill view and answer surviving pairs there; deleting
the middle and only then searching loses those consequences.

A call-dependent zero bound cannot replace an ordinary pair unless its
ordinary alternative also survives. Full and ordinary aggregates and memo
keys must be separate, including the case of an input contradictory only
in the full layer and the resulting expanded-row pass. Proofs retain join
events, predecessor order, inventory revisions and support generations;
sharing numeric values alone is insufficient. A demanded pair's proof
must acquire its original join boundary before publication.

Affine canonical intersections and current-image rules remain unchanged.
MSR-4 step 6 still visits every eligible measure, datum and current-image
binding in source-allocation order: laziness changes the bound query,
not the candidate inventory. Deterministic finite traversal has no budget.

For K demanded pairs and retained-output cost R, joining can cost
O(A K + R) beyond input closure. With implicit singleton rows omitted and
a fixed number of obligations, the series' join can approach linear work;
whole checks may remain quadratic in inventory scans or snapshots.
Arbitrary relational inputs can require U² residual pairs with A parents
each. Stage 1 is local; stage 2 needs coordinated store/view and consumer
changes plus transition differentials. The existing design's rejection of
omitting arbitrary implied cells still applies until those consumers have
complete preservation arguments.

## Direction A2: demand the affine index

The owner selected unchanged-state reuse plus lazy construction after the M5
results showed that most states change between queries. `affine_query_view`
still forms the complete ordered candidate vector before closing, then keys
one function-local memo by retained closed-Rc identity and exact equality of
candidate terms and images, including constants, coefficients and atom IDs.
Candidate formation may intern terms or mint images; it must precede matching.
No new mutation/revision protocol is introduced. Facts, signed goals, standing
measure metadata, snapshots, kills and joins retain the closure's existing
invalidation; image changes are covered by the complete vector comparison.

On a miss, `LazyAffineL0Index` groups candidates by coefficient vector in
candidate order. It builds no pair entries. DIRECT and every AUTO or bridge
residual request only their exact coefficient vector. For each left candidate
L and requested vector T, the lookup computes the possible right vector L-T
and probes the grouping. It visits only matching right candidates, in their
original order. Constants do not enter that grouping: each matching pair still
passes `tight_bound` and the original `from_bounded_forms` constructor, so
constant shifts, intermediate overflow and formation ceilings remain exact.
The smallest upper bound wins; only a strict improvement replaces the prior
witness. Both present and absent answers are memoized, without proof IDs.

The inverse coefficient subtraction is lookup arithmetic, not a new affine
rule. It uses checked subtraction directly: MIN-MIN=0 is representable even
though negating MIN first is not. If an inverse coefficient is out of i128,
no candidate can have it. Every successful inverse match is rechecked by the
original affine constructor, which may still reject a negation, constant or
bound intermediate. Thus this lookup neither adds nor loses representable
full-builder entries.

The final AUTO family can use every strongest canonical image, including an
image with no target atom. It has an independent row-major candidate-pair
cursor and seen-vector set. On demand for the next ordinal it scans until the
next first representable occurrence, then resolves that vector's strongest
witness across *all* matching pairs before returning it. Earlier DIRECT
lookups never change final-family order. The cursor pauses on success and
resumes on later queries of the same state. No target-overlap filter, interval
heuristic, recursive derivation or search budget omits a family member. The
two tightenings and MSR-4 bridge retain their existing loops. Contradiction
still precedes any exact lookup, and proof parents still come from the same
closed view and vocabulary ledger.

Equivalence follows separately for lookup and traversal: equality of L-R and
T selects exactly the full builder's pairs for T; the unchanged constructor,
matching-pair order and strict replacement select its identical endpoints and
bound. The cursor orders vectors by the same first representable occurrence
as the full builder, independently of where their final winners occur. Hence
every proof-family iteration receives the same inequalities in the same order
and calls `bound_proof` on the same endpoints and bound. No index operation
interns a derivation. The original full builder remains unchanged as a
test-only oracle, injected into the same proof traversal for parent comparison.

For N candidates and total image size S, initial preparation is O(S). A new
exact query scans N left images and only matching pairs; aliases can still
make one lookup quadratic. Across K distinct requests the scan cost is O(KN)
plus coefficient arithmetic and matching pairs. An exhausted final family
also scans N² pairs and may request N² vectors, so worst-case lookup work can
be cubic. This is a material performance uncertainty: the synthetic series,
natural interpreter and rejected-boundary cases must expose regressions. No
whole-check scaling improvement is claimed. Incremental per-event maintenance
remains deferred because it would require winner dependencies and deletion/tie
repair across closure and image changes; laziness preserves those interfaces.

## Direction B: backward demand throughout the proof state

Replace forward closed states by immutable program-point records for
entry, establishment, snapshot/kill, join and loop-header environments.
Each records its source-ordered predecessors, constructors and frozen
term/goal/image inventory. The ordinary semantic walk remains the only
authority, discharging required judgments at their existing source points;
there is no masked replay or second acceptance path.

The internal API would supply `contradiction(point)`,
`tight_bound(point,left,right,layer)`, `distinct(point,left,right,layer)`,
`signed_goal(point,goal,sign)` and ordered affine premise/image views.
Memo keys include point, exact question, full/ordinary layer and inventory
revision. Answers retain derivations, not merely booleans. At a join, take
the maximum tight answer across all non-contradictory predecessors; a
missing bound on one contributing input means absence. Retain contradictory
input proofs as neutral parents; all contradictory inputs give contradiction.
Disequality intersection asks derivability, not explicit flag membership.

A shortest path answers a bound only in the **saturated** difference-bound
graph. Raw source edges omit ENT-4 disequality strengthening. An unrelated
negative cycle or opposite signed goal makes every obligation derivable,
so MSR-4 step 1 needs complete combined-state consistency even when the
requested pair is elsewhere. Use finite ordered relaxation and signed-goal
worklists, or complete closure of a demanded component; Dijkstra on negative
edges, bounded search and heuristic slices cannot supply exact answers.

Kills need frozen survivor projections, not vertex deletion. From `x<=m`
and `m<=y`, a kill of m preserves `x<=y`. A later request for surviving
endpoints can query the pre-kill point and wrap its proof at that event;
a killed endpoint cannot. Eagerly enumerating all survivor pairs recovers
the old cost. Lazy projections defer it but still need ordinary fallback
and delivery-substitution semantics. This changes `state.rs`, flow state,
`events.rs`, `domain.rs` and `frontier.rs`, not just a prover cache.

The affine half also needs demand-driven views. Persist immutable value
images and source-established canonical inequalities; expose them in the
same ordered inventory, with join membership determined by canonical
intersection. Replacing intersection by "AUTO proves it on each path"
would add a rule. DIRECT requests intervals only for residual atoms. AUTO
lazily visits exactly the zero, one, unordered-pair and final-L0-image
families, with the same two tightenings, checked arithmetic and order.
Explicit PRF-1 lists stay checked as written. C2 transport uses each frozen
input's images and proves its components before simultaneous publication.

All six MSR-4 stages remain: contradiction, exact signed fact, L0, DIRECT,
AUTO, then the bridge. Step 6 needs its entire finite candidate inventory,
including terms the goal did not mention. Each candidate m requests its
tight `m-r` bound and then the one exact AUTO residual. A reachability
prefilter requires a proof that no successful candidate can be omitted.
Opaque minimum-depth parent reconstruction and cycle prevention remain;
equal L0 witnesses may vary under the existing policy.

Termination follows from finite source-derived point/question spaces and
the specified closures, not recursion depth or timeouts. Runtime loops use
existing abstract headers and per-edge induction boundaries, not unrolled
predecessor graphs. Recursive truth dependencies need a finite fixed-point
worklist. Source/term/goal order selects traversal, never hash iteration.
DIAG-2 still requires parents before children, failure-atomic publication
and final reachability/remapping; speculative memo entries cannot escape
a failed judgment.

With Q demanded bounds, a wide join can cost O(A Q) after predecessor
preparation. Q also includes consistency, DIRECT intervals, AUTO families,
bridge candidates, snapshots and delivery. Worst-case memo storage is
O(P U²) for P points, and demanding all pairs recovers eager cost. This
experiment would replace several proof-state interfaces, affine query
views and provenance consumers, requiring a transition oracle for every
point kind before corpus comparison. Its broader savings are plausible,
but neither total cost nor a complete lazy consistency/kill mechanism has
been established.

## Q155: the direction decision

**Ruled by the owner in this task:** keep the prototype's reuse across queries
on unchanged state and make the index demand-driven or incremental, rather
than rebuilding every candidate pair on each miss. The implementation uses
exact-vector lookup and lazy ordered final-family traversal as described above.
The M5 results reopen representation, not acceptance, the full-rebuild oracle,
the 2.5× series criterion or the obligation to retain identical proof parents.
A1 and B remain deferred. No design/log.md or spec/log.md entry is written in
this task, as the owner explicitly prohibited both.

## Prototype and required validation

The implementation changes `flow.rs`, `flow/prover.rs` and the certificate
call site. The full builder is retained unchanged under `cfg(test)`. No
specification, conformance verdict, join algorithm or proof-family rule changes.

The reported failing test `affine_index_cache_rebuilds_on_inventory_revisions_and_contradiction`
had a wrong expectation, not an omitted cache invalidation. `GoalTable::intern`
increments its revision when adding a projection; `close` includes that
revision, and `establish_goal` clears its remembered view. A positive opaque
goal does not publish its projection back into L0: source comparison
establishment publishes that relation separately. Attaching `fresh <= 2` to
an already-held goal therefore leaves fresh's upper bound at u64::MAX, not 2.
The test now asserts that bound and absence of the supposed new L0 fact, keeps
the positive-goal assertion, checks new memo identity, and still establishes
the opposite sign and compares contradiction parents. The expectation is
supported by ENT-3 source establishment and ENT-4's L0-to-goal projection rule;
no verdict was relaxed to accommodate a compiler limitation.

The existing five cache tests remain. Their index helper now demands every
full-builder vector in reverse order, compares selected endpoints/bounds,
then exhausts the lazy cursor and compares every ordered entry and lookup map.
It also compares bound and ±1 exact-vector answers/parents. This discriminates
query-order pollution, omitted aliases, stale keys, overflow skipping and tie
changes across facts, images, inventory revisions, snapshots, kills and joins.
The proof-family test starts with an empty lazy cache for each case and injects
a separately computed full oracle for the comparison, rather than clearing the
memo and accidentally comparing two lazy executions.

Additional tests:

- `affine_index_cache_demands_only_requested_vectors_and_memoizes_absence`:
  one DIRECT target amid unrelated measures creates one entry and leaves the
  final cursor untouched; an absent vector is memoized and reused.
- `affine_index_cache_lazy_final_family_keeps_disjoint_images_and_late_winners`:
  b<=-1 and a-b<=1 prove a<=0 but not a<=-1. The winning b image is a later
  shifted alias. Cold/warm lazy proofs must match full-builder parents; target
  overlap pruning or using the first bound before finding its winner fails.

CI must construct the unit executable separately, run the smallest cache case,
inspect its duration, then run `affine_index_cache_`, retained derivation and
certificate coverage, and the exact-revision `make check` gate on Linux/macOS.
Formatting, clippy, static/design lint and their actual results also remain
unverified here. No command, including a whitespace check, ran locally.

CI must demonstrate sensitivity on disposable copies before the green gate:
remove a demanded nonzero image; omit candidate-vector matching; omit the
closed-Rc match; use first encountered rather than strongest witness; and
advance final-family order from DIRECT demand order. Require semantic assertion
failures in the affected cases, not a compilation error. Restore the source
before the final gate. Also exercise workflow guard refusal for disallowed
compiler/spec/library inputs and malformed/missing timing rows. These are
required future checks, not reported observations.

## CI artifact construction and 14900K timing

The former `.github/workflows/check-time-artifacts.yml` is removed. It was
not on the default branch, so changing it to dispatch-only would leave no
runnable timing entry point. Temporary `check-time` jobs and inputs instead
live in the existing manual `.github/workflows/compute-bench.yml`; select
`experiment=check-time` on the work branch. Its hosted Linux job resolves the merge base with main and refuses differing
compiler inputs outside the four C3 source/test files, including differing
specification, dependencies, profiles, library or conformance inputs. It builds
base/head sequentially with the same toolchain, gate profile, debug=1 and
incremental=0, and packages binaries, revisions/trees, source checksums and the
compiler diff. Twin is a byte copy of base. Build time is separate from checks.

The timing job selects `[self-hosted, 14900k]` through `check_time_runner`. Reserve
that runner with the coordinator before dispatch; the job refuses an occupied
shared checker lock and runs the whole panel under `.github/run-check.pl`.
It changes no machine settings and installs no tools. The default `check_time_max_arms=40`,
`check_time_natural=false`, `check_time_rounds=3` is the sample. Inspect wall/CPU spread and timer
resolution before selecting a larger width or more rounds (multiples of 3).
Use `check_time_max_arms=640` and `check_time_natural=true` for the full requested panel after that
inspection. For example, after pushing this patch, dispatch
`gh workflow run compute-bench.yml --ref claude/check-time -f experiment=check-time`
for the sample. This task performs no push or dispatch; existing scoreboard
and placement jobs are excluded from the C3 selection.

`measure.py` generates the same input paths for all three binaries. The copied
`series-gen.py` is the supplied constant-assignment generator; the natural
interpreter is the current `research/experiments/match-dispatch/wasm/gen.py`
output without instrumentation or inline flags. Generator sources and generated
inputs are retained with SHA-256 identities. Natural output is a new workload
revision, not assumed byte-identical to historical M5 `nat.wf`.

Each input rotates base/twin/head, twin/head/base, head/base/twin. GNU time
records wall/user/system/RSS; raw rows record order and child exit status, with
stdout/stderr/time files retained. No cold observation is discarded. Counters
and compiler disk caching are absent. A failed or timed-out check completes
its paired round then stops scaling and fails the job; a timeout is missing
performance evidence, never source rejection. Summary reports spread, ratios
and adjacent-width head ratios only for complete successful panels; failed
checks never contribute a speed ratio. All raw rows remain available to compare order and
noise. A 120-second per-check timeout and 30-minute panel limit bound the
experiment, not acceptance. Ratios do not select workflow success.

The prior criterion remains: a mismatch rejects the implementation, improvement
within twin variation does not support a speedup, and a doubling above 2.5
rejects the series scaling claim. Base/twin/head distinguish aggregate C3 cost
from runner noise; an additional previous-prototype comparison would be needed
to isolate lazy construction from unchanged-state reuse alone. Remove the
temporary C3 jobs/inputs and timing driver before readiness after retaining results;
retain the input generator while the investigation still uses the series.

## Historical full-index prototype review

The replacement separate read-only Codex reviewer (exact model identifier
unavailable) inspected `8e33ca2987af9e62fa5eb3efa2959e157d91fdc7..working tree`:
all eight tracked changes and both untracked files, including the final
push-trigger workflow and the current-image test's term-identity case. It read
the task, ownership rules, complete diff, direct consumers, closure
invalidation, ledger lifetime, relevant specification and design nodes
`compiler`, `proof-query-context`, `incremental-closure`, `closure-evaluation`,
`engine-components`, `fact-map-hashing` and relevant `verification` decisions.

F1 found a stale promise of call-graph commands in Source attribution, although
the protocol permits timing only and contains no such command. The local prose
repair now states the timing/counter limits; the reviewer inspected the fix.
No open defect remained within scope. A4, D2 after F1, T4-T8, G1-G2 and DC1-DC3
passed inspection; D1, T1 and V3 were not applicable. C4, G3 and DC4 remain
unverified for execution, broader soundness and performance. No checklist
group was omitted wholesale; design-lint node/depth counts are also unverified.

The reviewer ran no builds, tests, lint, scripts or measurements. The
implementer ran only the permitted Rust formatting and read-only patch checks,
and verified the supplied input hashes. The compiler tests, deliberate fault
runs, native gate and artifact workflow have not run. CI and requester timing
remain required; the review is neither owner approval nor a measured result.

- Q140: investigation authorized; keep the existing counters for the paired
  comparison, then remove them when their owning investigation finishes.
- Q155 at that review: open; subsequently ruled above.
  A1 is removed from this diff and deferred; B remains a design alternative.
- Found along the way: the previous advice attributed the real program too
  strongly to join costs; the new profile corrects that advice. Dense
  single-input snapshots, candidate formation and per-event rebuilds remain
  deferred in `docs/todo.md` until their elapsed cost is material.
- Baseline mismatch at that review: requester had to place the patch on 13bb0d572 before CI;
  the new workflow refuses the old baseline and unrelated compiler changes.
- Oversized state/test modules remain recorded; no unrelated file split.
- Specification delta: none. ENT-4/ENT-5/ENT-6, MSR-4, PRF-1 and DIAG-2 have
  identical required behavior; only reuse of query preparation changes.
- No owner approval/log entry, ready transition, commit, push or merge.

## M5 timing results

Observed on 2026-10-08 on the Apple M5 under the host lock, with the
binaries of CI run 37713758139 (base 13bb0d572, prototype 305e474e8, both
the gate profile with line information, built on GitHub's macos-15 arm64
runner; checksums verified) and the twin a byte copy of the base. Each
panel ran three rounds, rotating base, twin and prototype through every
position. Wall seconds, median (minimum to maximum), and peak RSS:

| input | base | twin | prototype | RSS |
|---|---|---|---|---|
| plain-40 | 0.01 | 0.01 | 0.01 | — |
| plain-160 | 0.20 (0.20–0.20) | 0.19 (0.19–0.21) | 0.19 (0.19–0.20) | 90 MB |
| plain-320 | 1.79 (1.64–1.80) | 1.68 (1.65–1.80) | 1.66 (1.66–1.69) | 433 MB |
| plain-640 | 17.70 (15.57–17.96) | 16.70 (15.40–18.01) | 16.85 (16.02–18.22) | 2.7 GB |
| nat.wf | 9.73 (9.72–9.96) | 9.76 (9.69–9.77) | 8.75 (8.74–8.86) | 0.57 GB |

The first three runs of the 40-arm sample took 0.15 to 0.33 s (a cold start)
and are not comparable. On nat.wf the prototype is 10% faster than the base,
with the twin within 0.3%. The series is unchanged within its spread: its
cost is the join, which this prototype does not touch, and it still grows
about 9 to 10 times per doubling, so the 2.5x criterion is not met.

The index is 64% of the profile, yet reusing it at an unchanged state
recovers 10%: nearly every statement changes the state, so most queries
still rebuild the index. The remaining cost is the rebuild itself, all
candidate pairs per query, and its next candidates are building only the
part a target can use or updating the index across events.


## Lazy-index read-only review and remaining evidence

A separate read-only GPT-6-based Codex agent reviewed
`70f8b19d5f283e6384713af7a330bb722083d984..working tree`, including the complete
tracked diff, both new scripts, direct consumers, affine arithmetic, closure
invalidation, ENT-3/ENT-4/ENT-6/MSR-4 and the timing workflow. It read design
nodes `compiler`, `proof-query-context`, `incremental-closure`,
`closure-evaluation`, `engine-components`, `fact-map-hashing` and `verification`.
Its limited rereview confirmed these fixes:

- F1: failed checks were eligible for timing ratios. Raw failures remain, but
  only complete successful panels produce comparative ratios.
- F2: a standalone dispatch-only workflow absent from main was not a runnable
  work-branch entry point. C3 now uses the existing manual compute-bench
  workflow; the old temporary workflow is deleted.
- F3: the design node contained progress/approval prose. It now states the
  enduring choice, grounds, alternatives and reopening condition only.

No findings remain within the inspected scope. A4, D2, T4, T6, T7, G1, G2 and
DC1–DC3 passed source inspection; T5 passed after the workflow fixes but its
execution remains unverified. C4 found no unsafe, weakened proof, heuristic
cutoff or alternate acceptance path, while universal preservation and DC4
remain unverified pending CI. G3's design examination and worst-case final
family cost are recorded above; performance remains unverified. D1, T1, V3 and
new verification-stage T8 are not applicable. Design-lint counts and readiness
are unverified. The reviewer used source/Git reads and official GitHub
reference lookup, with no execution of builds, tests, checks or measurements.

The implementer likewise performed reads and edits only, without local Cargo,
Make, rustfmt, checks, timing, staging or commits. The changed tests have not
run, the workflow has not been dispatched, and there is no measured lazy-index
speedup. CI must supply the validation listed above on the eventual exact
revision. Specification delta: none; no approval logs were written. Work stops
at the requested read/edit boundary, with CI and owner-controlled branch
publication remaining.
