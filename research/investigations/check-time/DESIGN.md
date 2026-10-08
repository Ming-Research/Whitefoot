# Wide-match checking cost

## Question and prior criterion

Q140 authorizes investigating the cost of checking a wide `match`; Q155
will select the implementation direction. The requested measurement base is
`13bb0d572522c4c52342e87d90e50db7ffe90a79`, including C1, `continue`, C2's
per-edge join induction, its completion repairs, and the temporary
`WHITEFOOT_CHECK_WORK` counters. This uncommitted worktree is still based on
its parent `8e33ca2987af9e62fa5eb3efa2959e157d91fdc7`. Before CI, the requester
must place the C3 diff on the requested base and record the resulting exact
prototype revision; comparing this older parent directly would mix C2 repairs
and specification bytes into C3. The artifact workflow rejects that pairing.
The constitutional aim is practical verification at real-project scale
without changing acceptance or weakening required proofs.

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
unset; run paired instrumented checks to attribute work and observation
overhead. Include the owner's 318-arm stage-3 interpreter (`nat.wf`) and
the optional loop-wrapped series. Record machine, compiler and settings,
all exit statuses, run order, spread, and limits of attribution.

This session runs no local build, test, compiler invocation or measurement.
Only source inspection, editing and the expressly permitted `rustfmt` are
used. Earlier local executions supply no validation for this prototype.
CI builds and validates; the requester downloads the two macOS arm64 binaries
and alone runs timing on the M5 under the host-wide lock. No prototype speedup
has been measured. No commit, push or CI dispatch is performed in this task.

## Scope and alternatives

[ENT-4] and [ENT-5] in the active specification define derivability and
join results, not matrix construction. [DIAG-2] requires retained valid
derivations. [MSR-4] fixes the query route and finite candidate order.
None of the proposed directions changes those rules.

1. A1, reduce join materialization: compute per-term information first and
   retain only rows or pairs whose joined result cannot be rederived.
   Keep forward flow, ordinary closure, affine queries, kills and snapshots.
2. A2, reuse the complete affine index for unchanged closed facts and ordered
   candidate images, including ordinary queries as well as certificates.
   Target-directed construction and event-incremental maintenance are
   alternatives within this direction, considered below.
3. B, demand-driven proof states: retain point/edge predecessors and answer
   obligations backward, memoizing bound queries; give affine queries and
   every MSR-4 step the same demand-driven treatment.

The prototype implements A2. The new real-program profile supersedes the
join-focused recommendation: A1's implicit-singleton reduction is removed
from production code and its three tests are removed with it. Its algebra
remains an option, but real arms do not assign the series' constants and its
join-local change cannot reach the measured index hotspot. Keeping both
changes would also prevent attributing the paired timing to A2. Q155 remains
open for owner selection; the requested prototype is authorized, and neither
owner approval of the final design nor a specification amendment is implied.

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
seconds. The timing and counter panels below compare total checking cost and
existing work counts, not phase durations or affine-cache hit rates. A split
of these costs would need a separately authorized profile or new counters;
this protocol includes neither.

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

## Direction A2: reuse the complete affine index

**Recommendation: one function-local memo of the most recent full index.**
The existing `compiler/proof-query-context` decision already calls for reuse;
the implementation had confined it to a certificate's explicit `ProofClosure`.
Ordinary `ProofContext::new` queries already share an immutable `Rc<ClosedState>`
through `FactState::close`, but did not share their affine index.

`Reasoning::affine_query_view` forms the complete ordered candidate vector
first, obtains the current closure, and reuses the index exactly when:

- the retained `Rc<ClosedState>` is the same allocation; and
- the ordered `(TermId, AffineForm)` vector is exactly equal, including every
  coefficient, constant and atom identity.

The memo lives in the function's `Vocabulary`, next to its one derivation
ledger. It holds the Rc itself, not a raw address that could be recycled.
`FactState::close` invalidates its remembered closure when facts, selected
proofs, signed goals or contradiction change, and keys the view by term,
goal and ledger identity and their relevant revisions. Thus equal numeric
matrices from distinct proof points are not a hit. Clones of unchanged facts
may safely share a hit. A kill, snapshot, join, changed standing measure or
new goal projection reuses only if the existing closure contract permits it.

Value images require the second part of the key: a measure can receive a
fresh current atom or a binding can change image while L0 and both inventory
revisions stay the same. Candidate formation can itself mint an image or
intern a binding term, so comparing before formation would be too early.
Exact vector equality avoids both collision assumptions and a second,
distributed revision protocol for mutable value maps. Target, affine premises
and DIRECT's interval memo are not cached: they do not enter index construction
and are read again for each query. One entry caps retained storage rather than
keeping a quadratic index at every program point; alternating states can miss.

Every miss calls the unchanged `affine_l0_index`. Its full pair enumeration,
strictly strongest replacement, first equal witness and per-image checked
i128 skip are untouched. Every hit returns that exact ordered index, with
L0 endpoint/bound records rather than cached proof IDs. `bound_proof` still
gets selected parents from the current closed view and the same ledger.
DIRECT, all AUTO families and tightenings, the step-6 bridge and PRF-1's final
residual retain their existing traversal. No successful query publishes a
fact. The old certificate-only index cell is replaced by this shared memo;
its closure view remains. This changes evaluation cost, not the specification.

For N candidates of total image size S and K successive queries against one
unchanged key, the quadratic pair build occurs once instead of K times.
Each query still forms candidates (including sorted bindings), compares O(S)
image data and obtains a closure; AUTO traversal costs are unchanged. This is
not a linear-time whole-check claim. The measured 64% is inclusive sampled
time for one supplied program/revision, not a prediction of savings or a hit
rate. Rebuilds after each changed point, comparison costs and one retained
closure can limit speed and memory improvements. M5 timing decides the effect.

Alternatives examined:

- **Construct only target/residual vectors.** DIRECT could avoid many pairs,
  but AUTO's final family still contains every strongest L0 image in order.
  Merely requiring an image to mention a target atom is insufficient: with
  current images `b` and `a-b`, the L0 images `b<=-1` and `a-b<=1` can jointly
  prove `a<=0`; the first final-family candidate mentions no target atom.
  Lazy exact lookup would need a complete ordered enumeration for that final
  family and both tightenings. Defer until reuse measurements show substantial
  misses; an overlap filter alone is not a semantics-preserving design.
- **Incremental maintenance across events.** A bound insertion can change
  many closed pairs; a kill or ordinary-fallback selection can weaken a
  strongest image, expose an older equal witness or change its proof support.
  Changed images can regroup all pairs incident to a binding. This requires
  dependency tracking for every ordered candidate pair and canonical winner,
  including deletions and deterministic ties. It may help when keys change on
  almost every query, but the present profile identifies repeated preparation,
  not that miss pattern. Defer; use the same differential before adopting it.

These are representation choices, not acceptance fallbacks. Any mismatch
rejects the prototype. No new budget, traversal truncation or candidate cap
selects acceptance. `AffineCheckState::charge` counts work and imposes no
aggregate acceptance budget, so avoiding repeated construction charges changes
no formation limit.

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

---

**Q155 — Select A1 (join reduction), A2 (affine-index reuse), or B (backward evaluation) for C3?**

- **Background.** The constant-assignment series has reported 6.46× and 9.74×
  doubling ratios, and its join has cubic predecessor/parent work. But the
  supplied stage-3 profile attributes 3,098/4,842 checking-thread samples (64%)
  to `affine_l0_index`, rebuilt at each ordinary `affine_target_proof`.
  Join construction is not a leader; real arms do not assign the series'
  constants. The existing reuse decision is implemented only for explicit
  certificate views. ENT-6/MSR-4 require every strongest canonical image and
  the exact DIRECT/AUTO/bridge families; the specification stays unchanged.
- **Options.** **A1:** restore and measure the implicit-singleton join
  reduction. It removes a cubic construction on the synthetic series while
  retaining a quadratic store, but misses the real program's measured
  hotspot; retain it as a deferred option, not in this comparison.
  **A2 (recommended):** reuse the full ordered index when the retained
  closure and complete ordered candidate images match. It targets the
  measured rebuild through one shared query preparation path; every miss
  preserves the existing algorithm. Costs: O(image-size) comparison, one
  retained closure/index, and no savings across changed keys. Target-lazy
  and event-incremental variants await evidence that misses matter.
  **B:** replace forward states with persistent proof points and backward
  L0/affine queries. It may avoid more unused work, but must still implement
  complete contradiction, survivor snapshots, ordinary fallbacks and the
  exact MSR-4 families. Its complete preservation argument and measured
  advantage remain open.
- **Confidence 4/5.** The profile identifies the hotspot and the unchanged
  full rebuild supplies a direct oracle. Cache hit rate and wall-time savings
  are unmeasured. A mismatch rejects A2; little reuse or no improvement beyond
  base/twin variation reopens its representation. Super-2.5× series scaling
  rejects the existing scaling claim even if `nat.wf` improves, and may
  justify A1 or B as further work. Owner selection remains open.

---

## Prototype and required validation

The production change is confined to the shared affine query preparation in
`flow.rs`, `flow/prover.rs` and `flow/certificates.rs`. The full index
builder and all proof-family loops are unchanged. The join prototype and its
two state tests plus one source test were removed because they no longer
implement the selected experiment, not because of a failed verdict. Existing
join differentials and source conformance remain wired. The three existing
`proof_closure_tests` retain their closure assertions; artificial insertion of
an empty certificate-only index is replaced by real index differentials.

New tests in `compiler/src/semantic/entailment/flow/prover/tests.rs`:

| Test (prefix `affine_index_cache_`) | Observation and failure on a wrong index |
| --- | --- |
| `matches_full_rebuild_for_ordered_images` | Measures, source-ordered integer bindings, aliases, constants, negative/multiple coefficients and overflowing images. Compares every ordered entry, lookup map, selected endpoint/bound and exact-vector query at the bound and ±1. Pins strongest replacement and first equal witness. A missing image, wrong bound/order or skipped later representable image fails. |
| `rebuilds_after_facts_kills_and_joins` | Reuses an unchanged clone, then strengthens a bound, snapshots, kills, revisits a sibling and joins. Full rebuild and boundary queries follow each transition. A stale index either misses a new proof or retains a killed one; Rc assertions also detect unintended reuse or no reuse. |
| `keys_complete_current_images_even_with_a_closed_context` | Leaves the closed Rc unchanged while a binding's constant, coefficient or atom changes, a measure image is killed/reminted, an identical image moves to a different registered term, and a binding disappears. Both ordinary and explicit closed contexts are exercised. A key using only closure identity, revisions, counts or coefficient vectors fails. |
| `rebuilds_on_inventory_revisions_and_contradiction` | Adds a measure, changes a standing bound without changing term count, gives an existing positive goal a new projection without changing goal count, then establishes its opposite. Detects omitted new images, stale inventory matches and stale combined-state parents. |
| `preserves_direct_auto_families_and_selected_parents` | Primes the ordinary memo, compares proof answers and source/L0 parents with a forced full rebuild for DIRECT, single, unordered-pair, integer-tightening and final-L0-image routes, and pins accepted/boundary-rejected answers. In particular two multi-atom L0 images must compose although intervals alone cannot prove the target. No target answer is memoized. |

The oracle calls the original `affine_l0_index` without the cache, not a
second cache lookup. Ordered entry/map equality catches a dropped image even
if another AUTO route can recover the same final answer. Boundary checks
prevent an always-successful comparator from passing. The full builder itself
is unchanged; these tests establish memo equivalence, not an independent
proof of that preexisting builder. The independent specification/conformance
expectations and retained-derivation validators remain required.

CI must first construct the unit executable separately, run the smallest new
case, inspect its duration, then run the new group and the ordinary gate on
hosted Linux and macOS. These commands are **CI only**, with the gate's normal
toolchain, dependencies, submodule, `main` and review-base setup:

```sh
make -C compiler test-build-unit
perl .github/run-check.pl compiler/test-unit cargo test --manifest-path compiler/Cargo.toml --profile gate --locked --offline --lib affine_index_cache_matches_full_rebuild_for_ordered_images
perl .github/run-check.pl compiler/test-unit cargo test --manifest-path compiler/Cargo.toml --profile gate --locked --offline --lib affine_index_cache_
make check
```

Require the full gate for the exact prototype revision and the requested base,
including the existing eager transition comparisons, retained-derivation
validation, certificate failures, corpus and native conformance adapter.
No expected verdict, fixture or invocation is narrowed. The base and prototype
must have identical specification, conformance, library, compiler dependency
and build-profile inputs.

CI must also demonstrate that the new artifact guard refuses a missing or
wrong base and a disallowed compiler input change, then verify that the normal
package contains both executable arm64 binaries and matching recorded
revisions/checksums. None of those new workflow paths has been executed here.

Also demonstrate test sensitivity on disposable CI copies, restoring the
source before the green gate: (1) remove a nonzero canonical image from the
memo's newly built result while leaving the full builder alone; the ordered
image differential must fail; (2) omit the candidate-vector match; the
current-images case must fail; (3) omit the closed-Rc match; the facts/kills
and inventory cases must fail. Require the named semantic assertion failure,
not a build/formation error. These deliberate faults never ship. This is a
requirement for new check evidence, not a claim that these runs occurred.

## CI artifact construction

The temporary workflow
`.github/workflows/check-time-artifacts.yml` builds the requested base and
the prototype sequentially on one native macOS arm64 runner, with the same
Rust toolchain, `gate` profile, `CARGO_INCREMENTAL=0` and
`CARGO_PROFILE_GATE_DEBUG=1`. It downloads locked dependencies before offline
construction and uses the existing host-lock wrapper. It uploads both binaries
in one tar archive (preserving executable bits), full revisions/tree IDs,
toolchain and host data, the compiler diff and checksums. It runs no research
workload and is not wired into the gate. It runs only when the requester pushes
its file to `claude/check-time`, so no default-branch workflow installation is
needed for this experiment.

The workflow rejects a prototype that does not descend from
`13bb0d572522c4c52342e87d90e50db7ffe90a79` or changes other compiler,
specification, library or test inputs. The requester must first place this
uncommitted patch on that base, commit and push the reviewable work branch;
this session does none of those actions. That push starts artifact construction;
inspect its run and use its successful ID below:

```sh
gh run list --workflow check-time-artifacts.yml --branch claude/check-time
# Set C3_RUN_ID to the successful run whose recorded prototype is the reviewed commit.
gh run view "$C3_RUN_ID" --exit-status
# To repeat construction of this exact revision after investigating a failure:
# gh run rerun "$C3_RUN_ID"
```

The temporary workflow builds outside the gate's time budgets and adds no
budget row. Remove the workflow before marking the branch ready; preserve
the resulting experiment artifacts. Any later tree change still needs the
exact-revision gate.

## M5 timing commands for the requester

Only timing of the CI-produced binaries runs on the M5. No local Cargo,
Make, rustc, clang, test executable or locally built compiler is involved.
Prerequisites: an idle M5, GitHub CLI authenticated to this repository,
Python 3 for the supplied source generator, Perl, and macOS BSD
`/usr/bin/time`. Run from the repository root. Set `C3_RUN_ID` to the
successful artifact run, `C3_INPUTS` to a directory containing the supplied
`series-gen.py` and `nat.wf`, and `C3_PROTOTYPE_REV` to the full reviewed
commit the artifact should name. They are required inputs, not inferred refs.

Download, verify and prepare once; no command below builds a compiler:

```sh
set -eu
: "${C3_RUN_ID:?set the successful artifact run ID}"
: "${C3_INPUTS:?set the supplied input directory}"
: "${C3_PROTOTYPE_REV:?set the full reviewed prototype commit}"
export C3_ROOT="$(git rev-parse --show-toplevel)"
export C3_BASE_REV=13bb0d572522c4c52342e87d90e50db7ffe90a79
export C3_WORK="$(mktemp -d /tmp/whitefoot-c3.XXXXXX)"
gh run view "$C3_RUN_ID" --exit-status
gh run download "$C3_RUN_ID" --name c3-macos-arm64 --dir "$C3_WORK/download"
tar -xzf "$C3_WORK/download/c3-macos-arm64.tar.gz" -C "$C3_WORK"
(cd "$C3_WORK" && shasum -a 256 -c SHA256SUMS)
grep -Fx "base=$C3_BASE_REV" "$C3_WORK/identity.txt"
grep -Fx "prototype=$C3_PROTOTYPE_REV" "$C3_WORK/identity.txt"
test "$(uname -m)" = arm64
cp "$C3_WORK/bin/base" "$C3_WORK/bin/twin"
cmp "$C3_WORK/bin/base" "$C3_WORK/bin/twin"
mkdir "$C3_WORK/inputs" "$C3_WORK/results"
printf '%s  %s\n' \
  d77434bb9fd46c6e1698fbd5d006033568cd437ea52ccae67567f6f913491e57 "$C3_INPUTS/series-gen.py" \
  dc8c0e703bcf9df033d992a27c755f70407c46adbe23e2ab8fe97a91f4d39f71 "$C3_INPUTS/nat.wf" \
  > "$C3_WORK/supplied-sha256.txt"
shasum -a 256 -c "$C3_WORK/supplied-sha256.txt"
for n in 40 80 160 320 640; do
  python3 "$C3_INPUTS/series-gen.py" "$n" > "$C3_WORK/inputs/plain-$n.wf"
  python3 "$C3_INPUTS/series-gen.py" "$n" loop > "$C3_WORK/inputs/loop-$n.wf"
done
cp "$C3_INPUTS/nat.wf" "$C3_WORK/inputs/nat.wf"
git show "$C3_BASE_REV":.github/run-check.pl > "$C3_WORK/run-check.pl"
{
  sw_vers
  uname -a
  sysctl -n machdep.cpu.brand_string
  sysctl hw.memsize hw.logicalcpu
  pmset -g
  shasum -a 256 "$C3_WORK/bin/"* "$C3_WORK/inputs/"* "$C3_WORK/run-check.pl"
  printf 'cwd=%s\nno --cache; serial checks; counters absent in primary panel\n' "$C3_ROOT"
} > "$C3_WORK/results/m5-identity.txt"
```

The following scratch driver is used by this experiment and removed with its
scratch directory after retaining results. BSD time's `-l -p` writes real,
user and system seconds plus maximum RSS (bytes on macOS) to stderr; keep that
raw file separate from the compiler's stderr. Child status is captured outside
any pipeline. Three rotating orders give every variant each position once;
the twin is the exact same binary bytes, not a second build. There is no
compiler disk-cache flag and every executable sees the same source path/cwd.

```sh
cat > "$C3_WORK/time-budgets.txt" <<'BUDGETS'
label linux macos windows
c3-sample - 120 -
c3-panel - 1800 -
BUDGETS
cat > "$C3_WORK/measure.sh" <<'MEASURE'
#!/usr/bin/env bash
set -eu
cd "$C3_ROOT"
out="$C3_WORK/results/$C3_PHASE"
mkdir "$out"
printf 'input\tround\tposition\tvariant\tstatus\n' > "$out/status.tsv"
for input in "$@"; do
  for round in 0 1 2; do
    case "$round" in
      0) order='base twin prototype' ;;
      1) order='twin prototype base' ;;
      2) order='prototype base twin' ;;
    esac
    position=0
    for variant in $order; do
      stem="$out/$input-$round-$variant"
      status=0
      /usr/bin/time -l -p sh -c '
        if [ "$C3_COUNTERS" = 1 ]; then
          export WHITEFOOT_CHECK_WORK="$3.work.tsv"
        else
          unset WHITEFOOT_CHECK_WORK
        fi
        unset WHITEFOOT_TEST_TIMINGS
        exec "$1" --check "$2" > "$3.stdout" 2> "$3.stderr"
      ' c3-time "$C3_WORK/bin/$variant" "$C3_WORK/inputs/$input.wf" "$stem" \
        2> "$stem.time" || status=$?
      printf '%s\t%s\t%s\t%s\t%s\n' "$input" "$round" "$position" "$variant" "$status" >> "$out/status.tsv"
      [ "$status" -eq 0 ] || exit "$status"
      position=$((position + 1))
    done
  done
done
MEASURE
export WHITEFOOT_TIME_BUDGET_FILE="$C3_WORK/time-budgets.txt"
# Use the default shared /tmp/whitefoot-check-<uid>.lock. A busy lock exits 75;
# inspect its owner and wait. Never remove another run's lock.
WHITEFOOT_CHECK_TIMEOUT=120 C3_PHASE=sample C3_COUNTERS=0 \
  perl "$C3_WORK/run-check.pl" c3-sample bash "$C3_WORK/measure.sh" plain-40
cat "$C3_WORK/results/sample/status.tsv" "$C3_WORK/results/sample/"*.time
```

Inspect all nine smallest-input times and the base/twin spread before scaling.
If resolution or spread prevents comparison, repeat only that panel under a
new phase name or lengthen that panel; do not launch the larger panel first.
After the sample supports scaling, run one width at a time and inspect it
before continuing. These are the exact remaining timing invocations:

```sh
export WHITEFOOT_CHECK_TIMEOUT=1800
C3_PHASE=plain80 C3_COUNTERS=0 perl "$C3_WORK/run-check.pl" c3-panel bash "$C3_WORK/measure.sh" plain-80
C3_PHASE=plain160 C3_COUNTERS=0 perl "$C3_WORK/run-check.pl" c3-panel bash "$C3_WORK/measure.sh" plain-160
C3_PHASE=plain320 C3_COUNTERS=0 perl "$C3_WORK/run-check.pl" c3-panel bash "$C3_WORK/measure.sh" plain-320
C3_PHASE=plain640 C3_COUNTERS=0 perl "$C3_WORK/run-check.pl" c3-panel bash "$C3_WORK/measure.sh" plain-640
C3_PHASE=nat C3_COUNTERS=0 perl "$C3_WORK/run-check.pl" c3-panel bash "$C3_WORK/measure.sh" nat
```

The loop series is a separate panel, beginning with its own small sample;
its results are not pooled with the plain series:

```sh
WHITEFOOT_CHECK_TIMEOUT=120 C3_PHASE=loop40 C3_COUNTERS=0 perl "$C3_WORK/run-check.pl" c3-sample bash "$C3_WORK/measure.sh" loop-40
C3_PHASE=loop80 C3_COUNTERS=0 perl "$C3_WORK/run-check.pl" c3-panel bash "$C3_WORK/measure.sh" loop-80
C3_PHASE=loop160 C3_COUNTERS=0 perl "$C3_WORK/run-check.pl" c3-panel bash "$C3_WORK/measure.sh" loop-160
C3_PHASE=loop320 C3_COUNTERS=0 perl "$C3_WORK/run-check.pl" c3-panel bash "$C3_WORK/measure.sh" loop-320
C3_PHASE=loop640 C3_COUNTERS=0 perl "$C3_WORK/run-check.pl" c3-panel bash "$C3_WORK/measure.sh" loop-640
```

Optional counter attribution is timed separately under the same lock after
the primary panel; no local sampling/profile command is authorized here.
Start its own small sample and inspect it before the remaining instrumented
runs. The existing counters do not count affine cache hits or pair builds,
so they cannot establish this cache's hit rate:

```sh
WHITEFOOT_CHECK_TIMEOUT=120 C3_PHASE=work40 C3_COUNTERS=1 perl "$C3_WORK/run-check.pl" c3-sample bash "$C3_WORK/measure.sh" plain-40
C3_PHASE=work C3_COUNTERS=1 perl "$C3_WORK/run-check.pl" c3-panel bash "$C3_WORK/measure.sh" plain-80 plain-160 plain-320 plain-640 nat
```

Report all statuses (including crashes, timeouts or missing runs), per-variant
median/minimum/maximum wall and CPU time, peak RSS, prototype/base and twin/base
ratios in run order, and every adjacent-width prototype median ratio. No
startup subtraction or discarded slow observations. A timeout is missing
performance evidence, never source rejection. If the twin spread could
explain an improvement or crossing 2.5×, repeat or lengthen only that panel
and report the uncertainty. Compare counter-on against counter-off times to
quantify instrumentation overhead; never pool them. The natural interpreter
has its own speed ratio, not an arm-count scaling point. Retain the tar archive,
identity/checksum records, generated inputs, scripts, raw times and statuses
before deleting the scratch directory.

## Read-only review and disposition

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
- Q155: open; A2 recommended and prototyped under the requester's direction.
  A1 is removed from this diff and deferred; B remains a design alternative.
- Found along the way: the previous advice attributed the real program too
  strongly to join costs; the new profile corrects that advice. Dense
  single-input snapshots, candidate formation and per-event rebuilds remain
  deferred in `docs/todo.md` until their elapsed cost is material.
- Baseline mismatch: requester must place the patch on 13bb0d572 before CI;
  the new workflow refuses the old baseline and unrelated compiler changes.
- Oversized state/test modules remain recorded; no unrelated file split.
- Specification delta: none. ENT-4/ENT-5/ENT-6, MSR-4, PRF-1 and DIAG-2 have
  identical required behavior; only reuse of query preparation changes.
- No owner approval/log entry, ready transition, commit, push or merge.
