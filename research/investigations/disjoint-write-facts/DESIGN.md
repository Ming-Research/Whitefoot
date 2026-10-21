# Disjoint-write preservation: check cost

## Question and rejection criterion

Can ENT-5 preserve facts across OWN-7-proved disjoint writes without adding
more than 3% median compiler CPU on Halo's `pkg::vm` check? Compare the same
source with the base compiler, a byte-identical base twin, and the candidate
compiler in five interleaved rounds. Compiler CPU is user plus system time;
build time and counter collection are outside the timing panel. A median
candidate/base ratio above 1.03 with the twin median within 1% of one rejects
the negligible-cost claim. Any source verdict changed from the unoptimized
candidate rejects an optimization claimed to preserve acceptance,
independently of its timing.

## Supplied CI observation

The owner supplied the round-2 CI report for compiler revision `62bdc50a7`
against base `5850a7e0e`. The temporary branch workflow selects Halo source
`80f0ee4`, the gate compiler profile, `ubuntu-24.04` hosted runners, and
`--graph lib/halo/modules.wfg --check-module pkg::vm`. It warms each compiler,
rotates the interleaved execution order, and collects work counters in a
separate invocation. The supplied summary does not contain the runner CPU
model, Rust version, raw per-function counters, or absolute CPU samples;
the following attribution is limited to that summary and code inspection.
No local measurements were made.

| Compiler CPU ratio | Median | Minimum | Maximum |
| --- | ---: | ---: | ---: |
| Candidate / base | 1.0341 | 1.0116 | 1.0372 |
| Base twin / base | 1.0043 | 0.9926 | 1.0159 |

This observation rejects the negligible-cost claim for this workload. It
does not establish a cost for other modules or programs.

| Aggregated work counter | Base | Candidate | Difference |
| --- | ---: | ---: | ---: |
| `closure:count` | 36,274 | 36,828 | +554 |
| `closure:cells_sum` | 12,288,996 | 12,646,119 | +357,123 |
| `closure:rows_sum` | 523,385 | 536,466 | +13,081 |
| `snapshot:cells_sum` | 6,005,441 | 6,202,017 | +196,576 |
| `snapshot:rows_sum` | 247,059 | 253,827 | +6,768 |
| `function:intern_calls` | 8,089,556 | 8,128,401 | +38,845 |
| `function:affine_final_images_visited` | 3,040,363 | 3,043,091 | +2,728 |
| `function:affine_left_candidates_scanned` | 83,178 | 83,937 | +759 |

The reported join pair, retained-pair, parent-reference, union-row and
interning totals are unchanged. The reported probe totals are unchanged.
The counters are work observations, not timers: the CPU increase cannot be
partitioned into percentages from these aggregates.

The supplied case comparison shows that the ordinary and nested symbolic
index positives reject with OP-4 on base and accept on the candidate. The
bound and inline range-call controls, now reading `rows[0]` directly, accept
on both. The rebound-index and one-arm-only controls reject with OP-4 on
both. Those range controls therefore do not witness a new capability.

## Code-path attribution

The initial implementation collects the endpoints of every live numeric
relation, sorts their duplicates, collects same-base position pairs, and
submits each unique pair to the ordinary prover before applying the existing
support kills. The pair cache already covers ordinary, saved conditional,
and pending-result support within one write. Sorting and allocation of the
repeated endpoint list are additional work not counted by closure metrics.

Optional questions themselves can increase both closure and snapshot work.
`captured_index_term` can intern a literal or const, and index-separation
fallback can intern source-binding terms. A failed ordinary proof with goal
origins also constructs an origin query view, which interns goals and numeric
projections and closes that view. The closed-view cache includes vocabulary
revisions; the pre-kill materialization shortcut checks its closed term count.
Consequently, an optional question can cause another closure or snapshot
without preserving any additional source fact. A literal alone is normally
passive in the closure universe, so vocabulary growth does not by itself
establish that active rows increased.

Genuinely retained facts can also enlarge later state materializations.
However, `cells_sum` sums work across calls, rather than counting distinct
surviving facts. Its 2.91% closure and 3.27% snapshot increases do not measure
the cost of retention alone. The unchanged join work and 0.09% increase in
final affine candidates visited support targeting query preparation and
materialization first; they do not exclude extra facts within straight-line
regions. Per-function or per-query evidence would be needed to allocate the
observed CPU increase more precisely.

## Selected optimization

Reuse the bound store's existing live-term enumeration instead of allocating
and sorting two term IDs per occupied relation cell. It returns exactly the
same endpoint set. It still scans the matrix; the saving is duplicate-list
allocation and sorting, not a claim of eliminating the matrix traversal.

Remove exact duplicate support places before resolving their paths and
collecting candidate pairs. Equality includes complete captured identities;
do not merge merely equivalent spellings. Preserve the first occurrence and
the final deterministic pair sort and deduplication. Identical places
contribute identical questions, so the submitted question set, proof order,
proof context, answers, and roots are unchanged. All ordinary, conditional,
pending, affine, and entry-image support sources remain in the collection.

The complete pre-write state still enters the prover and the required
pre-kill closure. A relation through a doomed intermediate term can still
produce a surviving relation before that intermediate dies.

## Rejected query pruning

Skipping support that the old classifier already preserves is not generally
acceptance-equivalent. The answers are numeric relations shared across all
supports of the write, while candidate admission requires matching containing
places. Consider the following path-level witness (not a complete WF program):

- Write `rows[a][k].right`.
- An already-safe support A is `rows[a][l].left`.
- A threatened support B is `rows[b][l].right`.
- The current context proves `k < l`, but cannot separate `a` and `b`.

A and the write have the same `rows[a]` containing place, so A submits the
`k`/`l` question. The resulting numeric separation also preserves B: the
ordinary overlap walk continues below unresolved outer index pairs. B cannot
submit that inner question itself because `rows[b]` and `rows[a]` are different
containing places. Pruning A, either by its survival or by complete-path
separation of `.left` and `.right`, removes an answer B uses. The candidate
pruning was withdrawn after read-only review; no source-level execution of
this witness is claimed.

Raw place overlap is also not a replacement for typed support classification:
for example, a current page's length reads its owner's descriptor, which the
measure classifier reaches by removing the terminal page step. A path filter
must account for this transformation. This is an additional equivalence gap
in that rejected approach, not a demonstrated whole-program regression.

Do not restrict questions to two nonliteral positions: a current fact such
as `i != 0_u64` can separate a symbolic write from literal-index support.
Pair-answer reuse across all support is already implemented. Skipping support
killed by an earlier step has the same cross-support concern, and deleting
its facts before closure would also lose surviving consequences. Sharing
origin query views may be another opportunity, but is deferred because their
construction can change vocabulary and recursively invoke proof; their
invalidation needs a separate argument if collection deduplication still
fails the cost criterion.

## Validation

The Linux gate groups (static, unit, corpus, runtime) pass on `307c23418`
([run 38420968087](https://github.com/Ming-Research/Whitefoot/actions/runs/38420968087)),
including the mixed literal/symbolic preservation case and its proof replay.

A temporary same-source workflow
([run 38420967928](https://github.com/Ming-Research/Whitefoot/actions/runs/38420967928))
recorded the base (`5850a7e0e`) verdicts: the symbolic, nested and
literal/symbolic positives reject with OP-4 on base and accept on the branch;
the two range-call positives accept on both (existing call-argument
separation); the rebound-index and one-arm-join negatives reject with OP-4 on
both. The workflow's file selection missed
`ent5-neg-overlapping-symbolic-index-write`, so its base verdict is not
recorded; the branch verdict comes from the full corpus run.

Halo `--check-module pkg::vm`, gate profile, five interleaved rounds,
compiler CPU on a hosted runner: branch/base 1.0213 (min 1.0147, max 1.0277),
byte-identical base twin 0.9980. The unoptimized candidate `62bdc50a7`
measured 1.0341 (min 1.0116, max 1.0372; twin 1.0043) in an earlier run.
Work counters are identical between the two candidates, as predicted. The
two candidates were not timed in one panel, so the reduction is attributed to
the collection changes only by this cross-run comparison. Firn was not
measured: its current main source names a local `held`, a keyword since
v0.147, which the base compiler rejects.

Whole conformance corpus: a temporary workflow ran `whitefootc --check` over
the 3,044 single-file cases (module-form cases excluded) with the merge-base
compiler (`23aefb63e`) and the candidate (`644892cd3`, then `7425067b2`), gate
profile, `ubuntu-24.04` hosted runner, compiler CPU as user plus system time.
Without a twin
([run 39523567779](https://github.com/Ming-Research/Whitefoot/actions/runs/39523567779),
three alternating rounds) candidate/base was 1.0206 median (min 1.0095, max
1.0247). With a byte-identical base twin and a rotation that puts every
compiler in every position
([run 39525086802](https://github.com/Ming-Research/Whitefoot/actions/runs/39525086802))
candidate/base was 1.0066 median (min 1.0053, max 1.0164) and twin/base 1.0037
(min 0.9952, max 1.0194). The twin's spread exceeds the candidate's median
difference, so this corpus resolves neither a regression nor an improvement
from the base; the two panels bound the cost below the 1.03 line and agree
with Halo's 1.0213. The panels sampled the corpus only; they do not bound the
cost of other programs. Exactly three case verdicts differ between the base
and candidate compilers, all reject to accept: the symbolic, nested and
literal/symbolic positives; the base twin differs from base in none.
