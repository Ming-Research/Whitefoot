# Distinct affine bridge residuals after integer-datum inventory growth

## Question and evidence boundary

This is a read-only investigation dated 2026-10-13. Why does the enlarged
integer-datum inventory spend approximately 20.64 billion extra instructions
inside bridge-residual AUTO, and can the compiler compute the identical
specified answer more cheaply? These are candidate reductions, not selected
designs or implementation requirements. A changed AUTO answer or successful
first-witness parent sequence rejects every candidate below. No compiler,
specification, workflow or approval log was changed, and no local build, test
or measurement ran.

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
