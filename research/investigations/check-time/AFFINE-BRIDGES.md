# Distinct affine bridge residuals after integer-datum inventory growth

## Question and evidence boundary

The initial read-only investigation, dated 2026-10-13, asked: why does the enlarged
integer-datum inventory spend approximately 20.64 billion extra instructions
inside bridge-residual AUTO, and can the compiler compute the identical
specified answer more cheaply? The ranked reductions below were research
proposals; the follow-up sections record the subsequent instrumentation,
selected pruning, measurements and cleanup. A changed AUTO answer or
successful first-witness parent sequence rejects every candidate. The
specification and approval logs remain unchanged. No local build, test or
measurement ran.

The sources are:

- [The original attribution run](https://github.com/Ming-Research/Whitefoot/actions/runs/38366693138),
  comparing `c9c45a6ecee57c53675cc4c9e5c82cfe96eb5022` with
  `8ffb6d93241f69e736b12d4339eba780e857d8c9`, on hosted Ubuntu 24.04,
  Halo `80f0ee4`, gate profile with line tables and incremental compilation
  disabled. The recovered job log contains the exclusive Callgrind top-40
  differences and counter summaries. The artifact is
  [halo-check-attribution](https://github.com/Ming-Research/Whitefoot/actions/runs/38366693138/artifacts/11798006689),
  ZIP digest `be303ed17e38aec46ec1f864da68dba8dedf410d074592d5822be4f0615633c6`.
  Its raw ZIP was not accessible to local reading in this session; the earlier
  analysis's raw-edge attribution is retained below as supplied evidence.
- [The reuse measurement](https://github.com/Ming-Research/Whitefoot/actions/runs/38372231854),
  comparing `f0903241325885e1704a7212524a8c8f1eeedc3e` with
  `233f568eaa4e5c76cb02955bd3dc802915cd3f45`. Its supplied summary was read
  from scratch input and is transcribed here without making that file a
  maintained dependency. Same Halo pin and build settings; three interleaved
  uninstrumented pairs and one separate counter run per compiler.
- Code inspection at `f09032413`, with comparison against `233f568ea` to avoid
  proposing preparation sharing that the measured head already implements.
  Every code line number below refers to `f09032413` unless explicitly marked
  as the reuse head. Immutable source views:
  [prover](https://github.com/Ming-Research/Whitefoot/blob/f0903241325885e1704a7212524a8c8f1eeedc3e/compiler/src/semantic/entailment/flow/prover.rs),
  [affine arithmetic](https://github.com/Ming-Research/Whitefoot/blob/f0903241325885e1704a7212524a8c8f1eeedc3e/compiler/src/semantic/entailment/affine.rs),
  [query representations](https://github.com/Ming-Research/Whitefoot/blob/f0903241325885e1704a7212524a8c8f1eeedc3e/compiler/src/semantic/entailment/flow.rs),
  [premise canonicalization](https://github.com/Ming-Research/Whitefoot/blob/f0903241325885e1704a7212524a8c8f1eeedc3e/compiler/src/semantic/entailment/flow/domain.rs).
- The [active specification](../../../spec/kernel-spec.md), unchanged between
  those last two revisions, especially ENT-1, ENT-6 and MSR-4. There is no
  ENT-7 definition in this reviewed specification; the bridge rule is MSR-4
  Step 6 under ENT-6.

The original raw-edge analysis reported:

| Work in Halo's `run` | Before inventory growth | After inventory growth |
| --- | ---: | ---: |
| Initial AUTO calls | 869 | 869 |
| Numeric proofs entering Step 6 | 176 | 176 |
| Registered datum/measure candidates visited | 4,782 | 7,246 |
| Ordinary integer-binding candidates visited | 528 | 528 |
| Bridge residual AUTO calls | 5,310 | 7,774 |

The bridge AUTO subtree accounts for 20.64 G of the approximately 21.55 G
whole-check instruction increase. Thus the inventory adds 2,464 calls,
averaging 14 calls per Step-6 invocation. Dividing the subtree delta gives
**8.38 M extra instructions per added call**, or **117.3 M per Step-6
invocation**, as an amortized attribution. This is not an isolated measurement
of one query: inventory growth can also enlarge the final L0 family of the
5,310 pre-existing residual queries. Different targets can have different
premise and index sizes.

The later reuse run accepted `pkg::vm` identically in all eight executions.
CPU head/base was 1.028, 0.944 and 0.985; median 0.985. Reuse hits were 310;
final-family starts changed from 10,465 to 10,155 and exhaustions from 10,450
to 10,140. All other reported work counters were identical. This confirms
avoided traversals but establishes no recovery beyond hosted-runner noise.
The 310 hits are 3.0% of that run's baseline exhaustions. Comparing their
count with the historical 2,464 additions gives 12.6%, not a measured
instruction share or a target-level classification: the runs use different
compiler revisions and no retained residual trace maps those populations.

## Where an exhausted query spends work

Let M be the canonical automatic-premise count, E the strongest canonical L0
image count, N the candidate-image inventory size, and A the atoms actually
visited. A fully exhausted AUTO has the following structural split:

| Part | Work | Exact code sites |
| --- | --- | --- |
| Query-view preparation | Collect/clone ordered candidate images, close/reuse L0, compare complete image sequence; at least a scan of N even on an index memo hit | `flow/prover.rs:3066–3126,3335–3358` |
| Interval preparation | Build atom-to-measure associations once per DIRECT query; for newly demanded atoms scan bindings, choose strongest L0/type endpoints and preserve endpoint witnesses | `flow/prover.rs:240–267,3137–3201` |
| Premise preparation | Canonicalize and clone M premises; numeric entry already canonicalized its assumptions once | `flow/prover.rs:1526,3377,3725–3742`; `flow/domain.rs:1446–1458` |
| DIRECT target | One exact-vector lookup, then fixed interval dot product if needed | `flow/prover.rs:3244–3260`; `affine.rs:675–710` |
| Single-premise family | M candidate residual attempts | `flow/prover.rs:3382–3398` |
| Unordered-pair family | M(M+1)/2 checked pair sums, then candidate residual attempts | `flow/prover.rs:3404–3417,3440–3461`; `affine.rs:235–254` |
| Final L0-image family | E candidate residual attempts in retained first-occurrence order | `flow/prover.rs:3297–3327,3605–3615` |
| Each candidate attempt | Vector-dependent proportional factor, coefficient GCD and up to two divided candidates; checked target-minus-candidate merge/bound; DIRECT lookup and interval evaluation for each representable form | `affine.rs:267–318,540–592`; `flow/prover.rs:3274–3290` |

There are `1 + M + M(M+1)/2 + E` untightened DIRECT opportunities, plus up to
two tightenings for each nonconstant candidate; arithmetic failures remove
individual opportunities. This is a work model, not a measured family
percentage. Recovered counters show an extra 2,464 starts and exhaustions,
and unchanged index misses/promotions. They do not count single-premise,
pair or final-image candidates, nor tell how many instructions each consumes.
`ordered_entry` borrows a `Ref`; it does not clone index entries. Its
`promote` call checks for an existing complete index on every iteration;
unchanged promotion counts do not mean enumeration and lookup are free.

The recovered **exclusive** top-40 difference rows provide this more useful
operation split. Rows were grouped by their complete symbol labels, including
inlined source locations; the categories below are disjoint within that table.

| Selected whole-program exclusive deltas | G instructions | Amortized M / 2,464 added calls |
| --- | ---: | ---: |
| Residual/tightening controller and scratch/drop glue | 4.127 | 1.675 |
| Checked coefficient merging | 2.733 | 1.109 |
| Tightening formation and signed division | 0.445 | 0.180 |
| Interval weighted-bound arithmetic and lookup in `interval_proves` | 4.555 | 1.849 |
| Interval-map membership probes | 1.425 | 0.578 |
| `affine_interval_proof` preparation/control | 0.720 | 0.292 |
| L0 exact lookup/enumeration and promotion control | 1.006 | 0.408 |
| AUTO control | 0.736 | 0.299 |
| DIRECT routing | 0.404 | 0.164 |
| Selected-row subtotal | 16.151 | 6.555 |

Residual arithmetic/tightening/scratch totals 7.305 G; interval
evaluation/map/control totals 6.699 G. Those are respectively 35.4% and
32.5% of the supplied 20.64 G bridge delta **as a scale comparison**. They
are not caller-scoped shares: these rows cover the whole process and only its
top 40 positive differences. Neither the unreported instructions nor the
preparation/search split inside mixed/inlined symbols is known. In
particular, the 0.720 G interval-control category is not a measurement of
endpoint construction alone. There is no defensible numeric split between
the single-premise, pair and final-L0 families without the raw caller edges
or a new CI trace. Summing inclusive function costs would double-count them.

The strongest supported conclusion is that repeated checked residual
formation, endpoint lookup and interval dot products are substantial;
reconstructing the complete index is not the new repeated operation. The
residual-controller symbols with `&AffineInequality` include single-premise
and final-L0 calls; that type label cannot separate those two families.

## Specification constraints

Relevant exact excerpts, with line numbers in the reviewed specification:

> ENT-1, line 3214: “An implementation may organize or cache the same
> derivation differently”

> ENT-1, line 3211: “no implementation may strengthen, weaken, time-bound,
> randomize, or truncate an unsuccessful query within the derivable set.”

> ENT-6, line 4004: “the strongest canonical L0 image having exactly A's
> canonical coefficient vector and an upper bound no greater than A's”

The remainder of line 4004 fixes interval substitution by coefficient sign
using the strongest closed L0/type endpoints. Lines 4015–4019 fix the zero,
single, unordered-pair and final-L0 families and the two integer tightenings:

> Line 4016: “forming S and then the one residual uses checked `i128`
> arithmetic in the stated order.”

> Line 4017: “An unrepresentable candidate is skipped, not accepted and not
> allowed to suppress a later candidate.”

> Line 4022: “An unproved result exhausts the whole set”

> Line 4023: “or publish an intermediate result.”

MSR-4 line 4053 requires each admitted bridge to submit its exact residual to
AUTO and compose a success with that bridge. Therefore an optimization may
reuse or establish the result of all those fixed tests; it may not substitute
an incomplete support heuristic for them, introduce an acceptance solver,
drop integer datums, or reinterpret arithmetic failure as source rejection.
Any proof of inevitable failure below is a computation of the same AUTO
answer, not an additional source-language proof family. Successful targets
must continue through the original first-witness order.

## Ranked candidate reductions

The ranking separates a narrow representation improvement with observed hot
operations from more powerful but unmeasured failure certificates. No large
prunable fraction is established by the available evidence.

### 1. Replace repeated endpoint hashing with atom-ID lookup

**Proposal and sites.** Keep the prepared endpoints and their exact witnesses,
but use atom-ID-indexed optional slots into compact interval records instead
of hashing every coefficient. The repeated membership loop is
`flow/prover.rs:3146–3201`; the dot product hashes the same IDs again at
`3207–3215`, and successful parent construction reads them at `3221–3235`.
Atom IDs are dense function-local identities. A compact slot representation
avoids allocating a full parent-bearing endpoint record for every old atom.

**Soundness.** Line 4004 depends on endpoint values and coefficient order, not
map representation. Preserve the same checked multiplication/addition order,
strictly stronger endpoint selection, and witness tuples. Preserve view
invalidation and atom rollback boundaries. This changes no candidate family.

**Recoverable estimate.** The observed membership-probe rows alone contribute
1.425 G, or 0.578 M amortized instructions per added call: 6.9% of the
20.64 G scale. This is an addressable hotspot, not a predicted gain or an
exhaustive upper/lower bound on total lookup work. Dot-product lookup is mixed
with arithmetic in the 4.555 G category. Some array bounds/slot accesses
replace the old probes; allocation and initialization may offset the saving.

**Regression and rejection.** Compare cold, warm and mixed-atom queries,
large/sparse demanded IDs, changed facts/images, and exact endpoint parents
with the current evaluator. CI must show fewer lookup instructions with
identical answers. Reject the performance proposal if hashing is a small
caller-scoped cost or new storage costs consume its savings.

### 2. Validate one countermodel for an entire failed bridge group

**Proposal and sites.** After the initial complete AUTO failure, try a small
fixed deterministic set of integer assignments, starting with zero clamped
to each selected interval. Validate each assignment against every automatic
premise, every strongest L0 image, all strongest intervals used by DIRECT,
and a noncontradictory closed state. No search for an assignment is needed;
an invalid, missing or overflowing validation result means ordinary AUTO.
Use the prepared view at `flow/prover.rs:3335–3358`, the premises at
`3377`, endpoint selection at `3137–3201`, and final index at `3605–3615`.
Initial complete failure already enters/promotes the final index at `3306`.
Form the right image and bridge inventory first (`1608–1609`), then revalidate
the view so lazy image formation cannot invalidate the certificate.

**Soundness.** A validated assignment satisfies every input inequality.
Consequently it satisfies each checked single/pair sum and every admitted
positive-factor integer tightening. DIRECT's exact-L0 and interval routes
cannot prove an inequality this assignment violates. If it violates target T,
every listed AUTO route fails. This proves complete semantic failure without
constructing every residual; it publishes no theorem or new fact. All target
evaluation and certificate validation use exact checked arithmetic, falling
back on overflow. Certificate existence is not an acceptance criterion.

If the assignment violates initial `T <= t` and satisfies each formed bridge
`R <= r`, then `value(T) > t` and `value(R) <= r` imply
`value(T-R) > t-r`. Thus it rules out the whole Step-6 group, including
distinct coefficient vectors. A conservative prototype can instead test
each formed residual against the validated assignment. Keep Step 7 intact.
Validation must cover all interval-visible atoms and the stabilized complete
inputs, not merely the target's named atoms.

**Recoverable estimate.** Unknown, from zero to the certified groups' entire
search cost minus certificate preparation. It is the only candidate here
that could discard most distinct bridge residuals at once. Its usable share
requires counts of groups with a valid violating assignment and their
caller-scoped saved instructions; absence of premise support gives no such
count. Inconsistent affine premises, negative bounds or unrelated constraints
can make simple fixed assignments unusable. Preparing intervals for every
candidate atom can itself be expensive.

**Regression and rejection.** A valid violating model must suppress the
group's traversals; a later-success witness, a two-premise proof, a tightening
proof, contradiction, changed images and overflowing model arithmetic must
force fallback as appropriate. Compare all answers and successful parents
with ordinary AUTO. Reject if assignments rarely validate/violate the target,
or validation and interval preparation cost as much as the searches avoided.

### 3. Reuse failed bounds only with an overflow-qualified certificate

**Proposal and sites.** For a fixed input view and coefficient vector, retain
the largest failed upper bound `c` for which no otherwise formable candidate
was skipped by upward overflow of `c - S.upper`. A new stricter bound
`c' <= c` can then reuse that failure. Relevant arithmetic is
`affine.rs:277–308`; proportional factors are vector-only at `540–564`,
and DIRECT compares a bound-independent checked maximum at `675–710`.

**Soundness and the required direction.** Smaller upper bounds are stricter.
With identical vectors the candidate sums, factors, coefficient arithmetic
and interval sums are identical. Lowering the target bound cannot make a
previously tested inequality easier. It can introduce downward overflow,
which only removes candidates. However it can *restore* an upward-overflowed
candidate. Hence an unqualified failed-bound dominance cache is unsound.

A normalized checker-input counterexample is:

```text
M = i128::MAX; x has the full signed I64 interval.
Only automatic premise: P = M*x <= -1.

AUTO(M*x <= M)     fails.
AUTO(M*x <= M-1)   succeeds: subtract P to obtain 0 <= M.
```

The weaker query overflows `M - (-1)`; its tightened candidate has the same
bound overflow, its doubled premise overflows coefficients, and DIRECT/type
residual interval products overflow. The stricter query's untightened
residual is representable. This follows line 4017's skip rule, not a theorem
about ordinary mathematical implication. A dominance record must track that
specific upward-overflow hazard during full failure or conservatively prove
its absence for all otherwise formable candidate bounds. Retain exact-failure
reuse when dominance is ineligible.

**Recoverable estimate.** Unknown fraction of same-vector, stricter-bound
queries; zero for entirely new vectors. Record bound-only groups and
overflow-qualified hits before predicting a fraction. Each qualifying hit
can avoid nearly one complete search; certificate bookkeeping costs remain.

**Regression and rejection.** Failed weaker followed by stricter, looser
later success, the MAX witness, lower overflow, and changed facts/premises/
images. Reject if distinct queries mostly have distinct vectors, upward
overflow qualifications are rare, or hits save negligible instructions.

### 4. Amortize candidate-only arithmetic, retaining target subtraction

**Proposal and sites.** Lazily retain exact checked pair-sum results/failures
for ordered premise indices (`flow/prover.rs:3440–3461`,
`affine.rs:235–254`), and candidate GCD/divided forms (`affine.rs:275–295`).
The proportional-to-target factor still depends on the target vector;
the target-minus-candidate merge/bound and DIRECT check still run per target.
Preserve untightened, proportional and GCD ordering, including duplicate
factor suppression. Do not eagerly materialize M squared candidates on
queries whose initial DIRECT or early premise succeeds.

A smaller first experiment is stopping `coefficient_gcd` (`affine.rs:571–592`)
when its running divisor reaches one. `gcd(1,x)=1`, including unsigned
magnitude of MIN; line 4019 says factor one adds no candidate. Work charging
at `778–780` saturates a counter and does not select acceptance. There is no
reason to perform the remaining remainders merely to recover the same one.

**Soundness.** Retain exactly the original checked sum/formation result;
do not reassociate target subtraction or bypass intermediate overflow.
Candidate-only facts are functions of fixed ordered premises/images.
Sharing their computation implements ENT-1 caching while preserving all
ENT-6 opportunities and successful witness order.

**Recoverable estimate.** A component repeated K times can remove at most
`1 - 1/K` of that component's work, less cache overhead. The 7.305 G arithmetic
category is much broader: checked target merges account for 2.733 G selected
rows and remain. There is no isolated GCD/pair-sum share in the recovered
top-40 table; signed division is not a proxy for GCD's unsigned remainder.
Thus no measured percentage is available. The GCD-one exit is narrow and
low-cost; retaining whole pair catalogs has greater memory risk.

**Regression and rejection.** Early coefficient one followed by MIN,
nonunit prefixes followed by one, nonunit GCDs, empty/all-MIN vectors,
overflowing early pairs followed by a later success, and input invalidation.
Reject if eligible GCD suffixes or repeated pairs are few, or cache memory
and validation outweigh recomputation.

### 5. Share the canonical automatic-premise list

`numeric_affine_proof` already obtains canonical assumptions at line 1526,
but each target canonicalizes/clones them again at `3377,3725–3742` through
`canonical_affine_facts` (`flow/domain.rs:1446–1458`). One fixed ordered
automatic list retains first-occurrence source/derivation evidence and the
same families. Savings are only O(M) canonicalization/cloning per target,
not the O(M squared + E) search. No profile share isolates this component.
Test duplicate inequalities with differing evidence, later pair success and
new sessions after premise changes; reject if preparation is negligible.

### 6. Endpoint/measure-map construction once per numeric proof

This is **already present** in measured head `233f568ea`:
`flow.rs:858–869` retains endpoints, the measure map and residual scratch,
and `flow/prover.rs:3398–3431` moves those fields through subsequent targets.
Repeating this proposal has zero additional recoverable work at that head.
The noisy combined measurement cannot isolate its historical saving from
the 310 exact-failure hits and session overhead. Crucially, shared construction
does not remove the per-residual membership probes, exact L0 lookups or
checked interval dot products.

Candidate collection still runs before every reuse check. Removing that
revalidation by trusting index Rc identity alone would miss interior-mutated
measure associations. A cheaper version needs a complete inventory/image
revision contract; it is a separate correctness obligation, not free setup
removal. Keep the existing lazy-image invalidation case.

## Filters that do not establish failure

Every valid affine atom has a finite intrinsic type interval (ENT-6 line
3949). An atom with no named binding or automatic premise can still satisfy
`x <= max(type)` through DIRECT. A failed initial DIRECT also does not rule
out a single, pair or final-L0 residual, including candidates that cancel
terms. Missing support alone is therefore not a sound blanket filter.

The no-bound bridge check already happens at `flow/prover.rs:1616–1618` before
forming the residual or entering AUTO. `None` supplies no usable bound;
a representable large i128 value is a real bound, not an infinity sentinel
that may be discarded. Unrepresentable bridges/residuals are also already
skipped at `1620–1629`. Adding those checks again saves no admitted query.

## Next evidence that would decide the candidates

A future authorized CI investigation should first take one small paired
sample on the pinned Halo check and inspect duration before repeating.
Retain the current reuse code as the comparison base. Instrument counters
separately from uninstrumented timing, and use a byte-identical base twin to
separate noise in any precise performance claim. Hosted evidence remains
exploratory; precise timing uses the 14900K through CI.

The missing observations are M, E and residual atom lengths per fixed view;
candidate attempts/DIRECT calls split into initial, single, pair and final
families; construction versus warm interval probes; skipped arithmetic with
upper/lower overflow classification; same-vector bound groups; countermodel
validation costs/hits; and GCD-one suffix counts. A small deterministic trace
of canonical residuals and originating bridge terms would connect those
numbers to actual Halo source obligations. Record inclusive caller-edge
subtrees once and exclusive operations separately to avoid double-counting.

For every proposed reduction, compare AUTO verdicts and successful parent
sequences against the current complete traversal, retain the independent
conformance adapter in the ordinary gate, and show each new regression's
representative wrong result fails. A reduction with unchanged operation
counts, too few qualifying failures or no recovery beyond base/twin spread
is rejected as a performance remedy even when its equivalence argument holds.
These are proposed measurements only; this research task runs none of them.

## Counter-only follow-up protocol

The counter-only follow-up used the existing `WHITEFOOT_CHECK_WORK` path.
This section records its historical protocol; the temporary shadow instrument
was removed in the review cleanup below. Its metric definitions were then
documented in [work.rs](../../../compiler/src/semantic/entailment/work.rs).
The shadow countermodel probe ran only in an active counter scope, after a
complete initial AUTO failure enters Step 6. It tries three assignments in
fixed order: zero clamped to each strongest interval, all interval minima,
then all maxima. It validates the complete affine input set through detached
read-only data. It neither supplies a proof nor suppresses any ordinary AUTO
call. A group contributes covered bridge calls and exhaustions only when its
model and every observed bridge remain valid under the same input view.

The probe had an initial observation ceiling of 100,000 charged operations per
group and 2,000,000 per function-analysis invocation, including snapshot and
validation work. These are experiment limits, never acceptance limits.
Arithmetic overflow, unavailable or changed inputs, and exhausted observation
budgets produce unknown observations. The operations counter is an abstract
work measure, not an instruction count or elapsed-time estimate.

For the fixed assignment set, few would-certify groups covering little actual
bridge work would reject its usefulness on Halo when probe coverage is
adequate. Many unknowns or budget cutoffs instead make the measured coverage
a lower bound and leave that conclusion open. A would-certify group followed
by a successful bridge under identical inputs would invalidate the probe's
certificate argument. Useful certification coverage still needs subsequent
CI cost attribution: validation work consuming the avoided searches would
reject it as a performance remedy. The following section records this
follow-up's counter evidence and the next selected reduction before measuring
that reduction.

## Final-L0 evidence and selective traversal

[Counter run 38377586720](https://github.com/Ming-Research/Whitefoot/actions/runs/38377586720)
checked Halo `80f0ee4` with head
`b3d7d9e54911be2be45b4873333660e35b665fdd` against `f09032413`. All eight
printed verdicts were `pkg::vm: accepted`; median uninstrumented CPU head/base
was 1.001, with range 0.998–1.009. The completed job log was read to distinguish
the whole-check totals from its exact-function `run` table:

| Head counter | Whole check | Function `run` |
| --- | ---: | ---: |
| AUTO initial submissions | 884 | 466 |
| AUTO Step-6 submissions | 10,268 | 10,268 |
| DIRECT initial | 10,842 | 10,424 |
| DIRECT single-premise | 31,522 | 31,522 |
| DIRECT pair | 96,806 | 96,806 |
| DIRECT final-L0 | 34,248,745 | 34,237,146 |
| Interval construction probes | 10,875 | 10,330 |
| Interval warm probes | 131,704,851 | 131,659,779 |

The final-L0 family supplies 99.6% of `run`'s observed DIRECT calls. Its count
divided by bridge submissions is approximately 3,334 DIRECT calls per bridge
submission. This is a scale comparison: final-family calls also include
initial-origin AUTO and possible integer-tightened variants, so it is not an
exact measurement of E per bridge. It nevertheless identifies repeated
final-family search, rather than repeated endpoint construction, as the next
material work to remove.

All 176 shadow groups were unknown: 20 hit the group ceiling and 156 hit the
function ceiling, totaling 2,000,000 charged units. There were zero certified
and zero rejected groups. This is inconclusive, not evidence against the
countermodel proposal. The observation used unchanged ceilings: the probe copied and rechecked the
full atom/measure snapshots repeatedly; raising the limits was not a cheap
way to settle the final-family question. The review cleanup below removes
this temporary probe without claiming a negative result.

### Why overlap alone is insufficient

The proposed simple rule, "a disjoint, interval-feasible image cannot help",
misses DIRECT's exact-L0 branch. A normalized checker-input witness uses
anonymous full-interval integer atoms a and b, with no binding or datum whose
unit image is b:

```text
datum r has image 2*b, and closed L0 has r <= 0.
datum q has image a-b, and closed L0 has q <= -1.
Target T: a <= -1.
```

The disjoint image `R: 2*b <= 0` is interval-feasible. Its GCD tightening is
`b <= 0`; subtracting it leaves `a-b <= -1`, which DIRECT proves through q's
exact indexed image. Trying the overlapping q instead leaves `b <= 0`;
neither the raw index image `2*b <= 0` nor b's full type interval proves that
residual. Dropping R therefore loses a required AUTO proof. The design tree's
existing refusal of overlap-only pruning remains justified.

An independent issue is checked prefix overflow. In atom order a,b,c,d, take
each selected interval endpoint as 1:

```text
T: MAX*a + c - MAX*d <= 1, where MAX = i128::MAX.
R: b <= 1.
```

T's interval sum overflows at `MAX+1`, although its mathematical maximum is 1.
Subtracting the feasible disjoint R yields the ordered sum
`MAX-1+1-MAX = 0` and bound 0, which DIRECT can prove. A previous failed DIRECT
is therefore insufficient to justify pruning; the target interval maximum
must itself be representable and strictly above its upper bound.

### Selected conservative index

This branch implements an index of **retained image ordinals**, preserving
the complete index's first-occurrence order. Two structural posting maps are
built once for the complete L0 index: atom to image ordinals, and sorted atom
support to image ordinals. Support ignores coefficients. Interval-dependent
image feasibility is cached alongside the prepared endpoints and discarded
on the same input-view invalidation. Neither cache publishes a fact.

For a nonempty target T, pruning requires a checked strongest-interval
maximum `max(T) > T.upper`. Missing endpoints, arithmetic overflow or a
constant target selects the complete original traversal. Otherwise retain:

1. Every image sharing any atom with T.
2. Every image whose checked strongest-interval minimum is unavailable,
   overflows, or exceeds its upper bound.
3. Every possible disjoint exact-L0 composer, conservatively by support:
   for each indexed Q whose coefficients on every atom of T equal T exactly,
   find Q's foreign support B and retain every image R with support B.

For the third category, any target atom can anchor Q discovery through its
posting list; a disjoint R leaves every target coefficient unchanged, so Q
necessarily appears there. This protects raw and GCD-tightened complements
without deriving unchecked proof coefficients or adding a separate GCD
index. Different coefficients sharing support may be retained unnecessarily;
that only reduces the potential saving. Merge the retained ordinals, sort
and deduplicate them, and evaluate the original candidate routine unchanged.

### Deduction that every omitted image fails

Let R be omitted and let k be 1 or an admitted positive GCD divisor. R and T
have disjoint supports, and a checked interval minimum establishes
`min(R) <= R.upper`. Integer interval endpoints and coefficient divisibility
give an integer `min(R/k) = min(R)/k`, hence
`min(R/k) <= floor(R.upper/k)`. A target-proportional tightening cannot arise
for nonzero disjoint supports. Constant images have no tightening.

For every representable residual, fixed interval substitution gives

```text
max(T-R/k) = max(T) - min(R/k)
          > T.upper - floor(R.upper/k).
```

Thus its interval route fails. If its exact-L0 route could succeed through
an indexed Q, Q would agree with T on the complete target support and its
foreign support would equal R's support. The third retained category would
include R, contradicting its omission. Contradiction was already handled by
the ordinary initial DIRECT. These cases exhaust DIRECT.

Unrepresentable residuals already fail by the skip rule. The selector does
not replace their checked formation with rearranged arithmetic or invent a
success. Every retained image still uses the original untightened,
target-proportional and GCD candidate formation, checked residual merge/bound,
and DIRECT order. Every removed image has no successful variant; therefore
the first successful retained image and its original L0/interval parents
are identical. Step 6 continues to append its original bridge parent.

The governing specification text is:

> ENT-1, line 3214: "An implementation may organize or cache the same
> derivation differently"

> ENT-6, line 4004: "the strongest canonical L0 image having exactly A's
> canonical coefficient vector" followed by "fixed interval substitution"

> ENT-6, lines 4016–4017: "checked `i128` arithmetic in the stated order"
> and "An unrepresentable candidate is skipped"

> ENT-6, line 4022: "a proved result may stop at the first witness in this
> fixed traversal"

> MSR-4, line 4053: "it submits the one exact residual target to `AUTO` and
> composes a success transitively with that L0 bridge."

This is a certificate of failure for omitted members of the specified family,
not a shortened semantic candidate family or another acceptance algorithm.
Successful first witnesses are retained in their specified order.

### Regression and measurement conditions

Compare the selective route with the existing exhaustive reference on:
an ordinary feasible disjoint image that is skipped; an interval-infeasible
disjoint image that still proves; the GCD exact-complement witness above;
an earlier sharing witness with unchanged parents; target prefix-overflow
fallback; image-minimum overflow retention; and coefficient/bound overflow
where the untightened candidate must stay skipped while later variants or
images may succeed. Changed facts/images must discard interval classification
and the derived selection along with the prepared view. Keep the owner's
one-element-slice clippy fix.

New counters distinguish complete family size, planned retained images,
actually visited images, actual omitted ordinals in the reached prefix, and
fallbacks. An early success does not count later omitted ordinals as avoided
work. Existing origin/family/interval/overflow counters remain available.
The same pinned Halo counter run must show which final-DIRECT and warm-probe
work is eliminated; image-callback ratios alone are not CPU savings.

Any verdict or first-parent difference rejects the reduction. Target overflow
or an uncertain image minimum becoming a false failure also rejects it. If
the support-complement superset retains nearly every image, or classification
and posting-list work consumes the searches removed, it is not a remedy for
this workload. The following CI result tests this criterion. Builds, tests and measurements
run only in CI.


## Pruning measurement and review cleanup

[Measurement 38381962220](https://github.com/Ming-Research/Whitefoot/actions/runs/38381962220)
compared `668dd2d76745e68d2f005efc58aa604f579d1377` with the same base
`f0903241325885e1704a7212524a8c8f1eeedc3e`, using the same pinned Halo
`80f0ee4` on hosted ubuntu-24.04. All eight verdicts were `pkg::vm: accepted`.
Three interleaved uninstrumented CPU head/base ratios were 0.601, 0.605 and
0.599, median 0.601: 39.9% less CPU than this base on this workload. This is
exploratory hosted evidence, not a precise 14900K result or a claim about other
programs. The comparison includes shared preparation and failed-query reuse;
the cleaned-head confirmation will check that removing failure caching leaves
the recovery intact.

For function `run`, the counter log reports 34,237,146 total image opportunities,
2,878,666 selected and actually visited images, and 31,358,480 skipped images
(91.6% of reached traversal). There were 176 full-traversal fallbacks. Thus the
selector removes the repeated image search identified in the prior counters,
and the hosted CPU reduction meets the proposed performance criterion.
[Gate 38381962183](https://github.com/Ming-Research/Whitefoot/actions/runs/38381962183)
passed the Linux static, unit, corpus and runtime groups on that revision;
the ready/main-only combined two-platform job was skipped. These checks are
evidence for the measured revision, not execution evidence for subsequent
cleanup edits.

The shadow probe again observed 176 groups, all unknown: 20 group cutoffs,
156 function cutoffs, and 2,000,000 charged operations. Zero certified and
zero rejected groups leave candidate 2 inconclusive. The probe, observation
limits and all shadow counters/tests are removed as temporary instrumentation.
Countermodel certification remains deferred; no failure or success is inferred
from the cutoffs.

The review cleanup also removes the completely failed-query set, lookup,
publication, its counter and reuse-only tests. Its earlier 310 exact-repeat
hits did not recover time beyond hosted variation, satisfying that proposal's
falsifier. Pruning does not depend on cached answers. The prepared evaluator
remains solely to share interval endpoints, measure associations, residual
scratch and image interval-feasibility classifications across a numeric proof's
initial target and bridge residuals. Rechecking the existing closed/candidate
view still invalidates all this preparation together after input changes.
Every target runs AUTO afresh with its own arithmetic-check state. The pruning
regressions continue to compare answers and first parents with exhaustive
traversal, including fact/image invalidation.

The retained additions to the existing work instrument are:

| Counter group | Why it remains |
| --- | --- |
| AUTO initial/bridge/other origins | Separates the Step-6 multiplier from initial proof work; `other` keeps the accounting complete. |
| DIRECT initial/single/pair/final-L0/other families | Identifies which fixed family consumes searches and whether work moves between families; `other` includes DIRECT outside ordinary AUTO. |
| Final images total/selected/visited/skipped and filter fallbacks | Measures the selector's coverage and actual work avoided, including early success and conservative overflow fallback. |
| Interval construction/warm probes | Separates shared preparation from repeated endpoint access that remains after pruning. |

Tightening counts and overflow-event classifications are removed with the
attribution-only metrics. Their checked arithmetic and overflow regression
cases remain; removing observations cannot change candidate formation.
Existing join, closure and affine-index counters are unchanged. The documented
TSV schema in `work.rs` remains the owner of counter definitions.

Cleaned-head confirmation (hosted runner, 2026-10-13, head `13d2df4f9`
against the pinned base above, run
https://github.com/Ming-Research/Whitefoot/actions/runs/38384699923): the
check verdict `pkg::vm: accepted` is identical across all eight runs; median
paired user+system CPU ratio head/base is 0.600 (range 0.599-0.607; medians
13.240 s base, 7.940 s head). The recovery survives removing failure reuse,
so the attribution stands. The gate passed on the same revision
(https://github.com/Ming-Research/Whitefoot/actions/runs/38384700155). The
temporary measurement workflow and its budget rows were then removed. A paired
14900K comparison against the release before #361 remains the completion
measurement.
