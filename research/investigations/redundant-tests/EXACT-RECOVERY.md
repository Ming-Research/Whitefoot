# Exact recovery of redundant-condition query cost

This is the proposed implementation argument for the OP-5 recovery on the
redundant-condition work branch. It changes no specification rule. The
implementation must preserve dispositions, retained derivations and rendered
diagnostics; no work budget selects an answer.

## Question and rejection criterion

Can the checker remove repeated query preparation without changing the
ENT-4.OT collection or its effects? Reject any reuse that changes a disposition,
selected proof parent or diagnostic. Compare the protected unit cases and the
existing gate in CI before measuring. Subsequent performance attribution must
use the same source, interleaved base/change runs and a base twin; the numbers
below are the owner's supplied attribution, not a new measurement.

## Starting evidence

The supplied hosted-CI Firn check was 8.72 s wall on the branch versus 6.11 s
on main. All 4,380 source conditions were undecided. Inclusive phase sums were:

| Phase | Branch sum | Recorded work |
| --- | ---: | --- |
| closure | 6.93 s | 60,347 fresh closures; main had 43,328 |
| condition | 4.25 s | 4,561 judgments |
| origin_view_build | 1.32 s | 6,847 views |
| origin_view_fixed_point | 1.81 s | 6,847 views |
| ordinary_prove | 0.83 s | ordinary attempts |
| affine_search | 0.76 s | affine traversal |
| model_view_build | 0.44 s | model premise gathering |

Nested analyses double-count in these sums; they cannot be added to predict
wall time. A view averaged 32 expressions, 17 Boolean members and 1.39 passes.
The approximately 130-test dispatch chain in Firn's `execute` accounted for
35% of origin-view cost. Interning goals in the True query invalidated the
entering closure for False, adding about one closure per condition.

## R1: a new negation class only

True still tries its ordinary proof first. If it needs origin transport, form
the collection for `bnot X`. Share it with False only if its root class was
absent before submission and its final members are exactly the submitted root
and, when distinct, its own term view, and its completed view is
noncontradictory. Otherwise use the two original views.
False still tries its own ordinary proof first. Both queries retain the
ordinary result when the view does not change its disposition.

Compared with collecting X, collecting `bnot X` adds only that Boolean root
and its term view. Its integer subtrees, representatives, equality relations
and entering projections are unchanged. No entering expression contains a
member of the new class. Derived signs do not decompose. Consequently an
inferred positive negation cannot supply a signed fact to an old member;
the old classes keep their order and fixed-point proofs. A further guard
rejects a contradictory enlarged view: even a new singleton negation can
supply a different contradiction parent if opposite affine proofs are found
in one synchronous pass. Returning to the original view preserves its exact
retained proof, including when True was already provable there. The memo
lives only during one condition judgment, whose entering facts and affine
images are unchanged. Comparison-origin queries keep their original path.

Unrestricted sharing fails on entering `-band(bnot Z,w)` and `+w`, with Z
origin-equivalent to X but only X admitting the needed negative affine proof.
For a concrete internal fixture, X compares an integer operand with zero; Z
compares an ordinary-let alias with zero. The alias is the first numeric
representative and has no current affine image, while X's operand has the
constant affine image 1. `view(X)` supplies no independently derived negative
X. `view(bnot X)` proves the positive negation affinely and transports it to
`bnot Z`, reconstructing the positive conjunction and contradicting its
entering negative fact. Using that view for True would decide True incorrectly.
The regression also checks an origin set containing X and w: True fails on X,
False fails on w, so the complete condition remains admitted.

A separate regression protects the new-class contradiction guard. Give integer
x affine image 1, let z originate from x, and enter L0 z <= 0. For
X = (2*x <= 0), the ordinary query is unproved. Origin equality supplies L0
x <= 0; its affine image is the false constant inequality 1 <= 0, whose two
uses prove X through AUTO and DIRECT. The X view retains that positive proof
without contradiction, since this signed affine consequence adds no L0 fact.
The negated view can also prove `+bnot X` from the constant image. The new
class then contradicts its negative sign reconstructed from +X. Sharing would
replace an origin-transport proof by a contradiction proof. The completed-view
guard preserves the original route and parent instead.

## R2: remember collection and complete metadata

Keep the expansion DAG while the exact ordered live-definition map is
unchanged, including each introduction's proof identity. Cache the initial
collection only while the exact ordered entering signed sources also match,
including their numeric-admission flag and source proof. Add the submitted
expression afresh. Cache keys therefore preserve the collection's preorder,
admission views and transport parents; a definition replacement, kill or source
change misses.

Persistent DAG identities alone are insufficient: sorting classes by historical
node allocation would reorder proof selection. Each cached expansion retains
its node-allocation order; replaying first occurrences into fresh per-query
ranks reproduces the order of a new DAG, including intermediate expanded nodes.
Class traversal uses those ranks, never historical node numbers.

An exact goal lookup skips metadata construction only after its record has
been fully populated by recursive goal interning. This includes Boolean
children, struct fields, support, projection and normalization. A missing
projection can legitimately remain absent; completeness is an explicit flag,
not a test that optional metadata is nonempty. Different admission views and
incomplete low-level records still take the full path. GoalTable's existing
determinism assertions remain in the full interning path.

## R3: append-only goals do not erase a closure

The cache retains the number of checked goal records. Equality of
`revision - record_count` distinguishes pure append from metadata edits.
With unchanged facts, term table and ledger, a recursively complete old goal
cannot acquire a missing child or a new proof route. L0 is unchanged. Check
both signs only for appended records; if any is contradictory, use ordinary
closure to select its contradiction and retained parents. Advance the checked
prefix on a hit.

An integer-match exclusion is part of the facts but decides a goal by a
query-time rule, not by L0: a closure answers an appended comparison goal the
exclusion decides, yet only a fresh closure materializes that sign and its
proof as an opaque fact, which later snapshots and origin views read. So an
appended record that an exclusion decides in either sign is treated like a
contradictory one and recomputed; the test
`an_appended_goal_an_exclusion_decides_recomputes_the_remembered_view` fails
without this condition (temporary run 39510883437).

An already contradictory cached state still recomputes after append, since a
new goal could change the retained contradiction. Incomplete old trees also
recompute: adding a previously absent child can contradict an old parent even
when the child itself has only one sign. Metadata edits retain ordinary cache
invalidation. The regressions distinguish these cases from a harmless append.

## R5: retain L0 inputs across signed-only views

The affine index key retains the immutable bound, disequality and disequality
proof stores, the integer-match exclusions, the term revision, ledger identity
and contradiction state, plus the exact ordered candidate terms and value
images. An origin view that only adds signed facts keeps those L0 stores and
exclusions. Their retained references force copy-on-write on numeric or
source-proof edits, so these changes miss. This conservatively declines to
merge independently allocated equal-content stores.

The exclusions belong in the key because an exclusion tightens closed bounds
past an excluded endpoint (excluding 5 closes `x <= 5` to `x <= 4`) without
touching any of those stores: two states differing only in an exclusion would
otherwise share an index that memoized the looser bound, and the model view
keyed on that index would inherit it. The test
`affine_index_cache_misses_when_only_an_exclusion_differs` fails without them
(temporary run 39510883437).

The index contains pair identities and numeric bounds, not retained proof IDs.
Every selected proof parent is obtained from the current closed state. A model
view keys on that index residency and the complete ordered affine inequalities;
it contains no derivations and can only skip a traversal it refutes. The three
affine-atom rollback sites still clear the models, so a reused atom number
cannot carry an old intrinsic range. Regressions cover strengthened L0,
changed affine premises, and equal bounds with different source proofs.

## Expected phases and validation boundary

Expect fewer origin builds and fixed-point traversals from R1, less collection
and metadata work inside origin_view_build from R2, fewer fresh closures from
R3, and fewer affine-index/model builds from R5. Ordinary query counts and
condition query counts are unchanged. Existing work counters still count the
work actually performed; a rejected sharing attempt spends build time but
never increments fixed-point inventory/pass counters.

For scale only, if every measured origin view belonged to an equally costly
shareable pair, R1 alone would take the 1.32 s build and 1.81 s fixed-point sums
toward 0.66 s and 0.91 s respectively. That is a conditional model, not a
prediction: the sharing fraction, extra negation work, guard misses, R2 memo
cost and R3/R5 interactions are unmeasured. R3 targets roughly the observed
one-extra-closure-per-condition cost, but the affected closures need not have
the average closure cost. No defensible numeric replacement for the 6.93 s
closure, 4.25 s condition, 0.83 s ordinary, 0.76 s affine or 0.44 s model sums
is available before CI. Their inclusive nature also precludes subtraction
from wall time.

No compilation, test or timing run is permitted on this workstation. The
new cases are implementation regressions, not changed conformance verdicts.
CI must first verify compilation, the new cases, existing proof-parent cases
and the project gate; a timing claim requires the subsequent paired run.
