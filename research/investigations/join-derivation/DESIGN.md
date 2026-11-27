# What the one derivation keeps at a join, and how its case analysis is bounded

Status: questions, candidates, hypotheses and refutation criteria. This file
was committed before any measurement was taken. `RESULTS.md` holds the
numbers and the verdicts. A later change to this file is listed, with its
reason, under "Changes after the first measurement" at the end.

Base: main at `ad1fcb11c`. The branch `join-derivation-study` is never merged.

## Question

The redesigned specification has one derivation for every obligation: the
four steps of today's [RANGE-3] (instances, definitions, theory, decision).
At a join of an `if` or a `match`, a place whose arms hold different values
gets a join value "defined by the arm taken". This study answers three
things:

1. What the derivation keeps at a join.
2. How the case analysis is bounded by a fixed rule, and what a writer is told
   when a proof is out of its reach.
3. What each candidate decides, what it costs in the worst case, what it
   costs on inputs built to be hard as they grow, and how a Lean model would
   state it.

An earlier study (`research/investigations/derivation-ceiling` on the branch
`e1-derivation-ceiling`) answered from the conformance corpus and
`apps/firn`. Those programs were written to pass today's checker, so their
distribution shows nothing about what the rule must handle. This study uses
no corpus. It argues from the algorithm, and measures on inputs that are
constructed and grown.

## Rules of method

1. No claim rests on how often a shape occurs in existing Whitefoot code.
   A candidate is judged by its worst case and by constructed inputs.
2. Each hypothesis below states what would refute it. It was written before
   its measurement.
3. The primary measure is a count of operations that the instrument makes
   deterministically: theory calls, Fourier-Motzkin pair combinations, case
   branches, arm probes, bound projections. A count has no noise and does not
   depend on the machine. Wall time is secondary: it is the minimum of
   repeated rounds on a GitHub-hosted runner, and it is used only to show
   that the counts predict it.
4. Each input family is measured at growing sizes, and the growth is
   reported as a curve.
5. Each measurement says what it can and cannot show (section "What the
   instruments can and cannot show").
6. The instrument is checked: on small instances an exhaustive evaluation of
   the program decides each obligation's truth, and every candidate's "proved"
   is compared with it.

## The problem in the abstract

In the value form, an obligation at a site asks whether a relation `G` over
values follows from:

- the literals that hold on every way to the site: conditions of enclosing
  arms, facts (requirements, invariants, postconditions, type ranges) and
  definitions of values; call this set `Γ`;
- for each join `J` completed before the site: one of its arms was taken,
  and the literals of that arm hold. The literals of arm `i` are its
  condition, the facts and definitions made inside it, and `v = a_i` for each
  join value `v` and its arm value `a_i`.

A loop head and a call are cuts, and they are the same for every candidate:
the head replaces each place the body may write with a fresh value and
assumes the written invariants; a call replaces what it writes and assumes
its postconditions. So the join values an obligation can reach are those of
the joins between the nearest cut and the site. A subscript bound, a call
requirement and an invariant on a backedge are all obligations of this one
form; "an array indexed by a join value" and "a requirement that names a
join value" differ only in where `G` comes from.

Three facts follow from this form alone. They need no measurement.

- **F1. Deciding the complete relation is coNP-hard.** Take Boolean
  variables `b_1..b_n`, each a join value of its own `if` with arm values 1
  and 0. Write each clause of a 3-CNF formula as a nested condition such as
  `b_1 + (1 - b_2) + b_3 >= 1`. Inside all the conditions, ask whether the
  site is unreachable. The site is unreachable exactly when the formula has
  no satisfying assignment. So any derivation that proves every true
  obligation of this form decides 3-UNSAT, and no known algorithm does that
  in less than exponential time. A complete rule therefore needs a cap, and
  an uncapped rule must be incomplete.
- **F2. A cap on work done is not a fixed rule.** The number of branches a
  search opens depends on the order of splits and on what the search learns.
  Decision `range-problem-size-ceilings` already rejects such a cap. A cap
  must count something the problem has before any search.
- **F3. A structural count of open joins is not reduced by an invariant.**
  A local `invariant` adds a literal. It removes no join from the problem. So
  under a rule "reject when more than N joins are open", a writer who is
  rejected cannot reach an accepted form by stating invariants, unless the
  count is taken only after the stated facts already prove the goal with no
  split at all.

## Candidates

All candidates use the same theory step T: [RANGE-3] step 3 (solve the
equalities, tighten each inequality over the integers once, then decide
rational infeasibility). They differ in what a join contributes and in the
decision step.

| Name | What a join contributes | Decision step |
| --- | --- | --- |
| SEL | the selector: each join value is the arm value of the arm taken | split every open join until each branch is contradictory (today's text) |
| BND-I | for each join value, the weakest constant interval over the arms; no selector | none |
| BND-R | relational join bounds (below); no selector | none |
| BOTH | selector and BND-R bounds | split every open join |
| SAT | selector and BND-R bounds | forced cases only (below), no split |
| WIN(P) | selector and BND-R bounds | forced cases, then split near joins within a product P (below) |

The owner's two-step idea (try the bounds first, split only where that
fails) is BOTH with the root checked first: the bounds are consequences of
the selector definitions, so both steps together accept exactly what BOTH
accepts. They differ only where a cap applies, which is the subject of H5.

**Relational join bounds (BND-R).** For a join and each place whose arms
disagree, with join value `v`:

- *Transition bounds.* For each arm whose value of the place is a linear
  expression `E` of values that exist at the join's entry, the candidates
  are `v - E` and `E - v`. An arm that leaves the place alone gives `E` = the
  entry value.
- *State bounds.* For each inequality that held at the join's entry and names
  the entry value of a place whose arms disagree, and that is a path
  condition, a fact from a source, or a state bound of an earlier join, the
  candidate is that inequality's linear form with the entry value of every
  such place replaced by that place's join value.
- For each candidate form, each arm's supremum is the tightest constant
  upper bound T gives for the form at the arm's end, with the arm's value in
  place of `v`, without any split. An arm whose literals are contradictory is
  left out. If every remaining arm has a finite supremum, the join records
  "form <= the largest of them" as an unconditional fact.

A transition bound is not itself a candidate for a later join's state
bounds. The variant BND-R-all drops this restriction; H3 measures why it is
there. BND-I is the same rule with the only candidates `v` and `-v`.

The bounds of a join are a function of the program alone. A checker may
compute them when the join is formed (eager) or when a problem first names
the join value (lazy) with the same verdict.

**Forced cases.** A join is *near* when one of its join values, or a value
named in one of its arms' literals, is reached from the goal's values, from
the values of the conditions on the way to the site, or from the values of a
case already decided, through literals of the problem that are not join
bounds. A near join with exactly one arm whose literals are not
contradictory with the current literals is decided: that arm's literals are
added. This repeats until nothing changes. A near join with no such arm
makes the branch contradictory.

**Split within a product (WIN(P)).** After the forced cases, the latest near
join whose number of arms is at most the branch's remaining product is
split, one branch per arm, and each branch continues with the product
divided by that number. P is the starting product. A branch that is not
contradictory and has no join to split is open, and the obligation is
unproved. No obligation is rejected for its size: a join out of reach keeps
its bounds only.

Variants measured to show what each part does: WIN without forced cases,
WIN without bounds, SEL and BOTH with the opposite split order, SEL with the
backjumping today's compiler uses (a branch is not split on a join its
refutation does not use).

## Other approaches considered, and how they map onto the candidates

- **Intervals** are BND-I. **Zones and octagons** keep bounds on `±v ± w` for
  every pair of live values: a state of size quadratic in the live values and
  a closure cubic in them at every statement. BND-R keeps a bound only for a
  form that a written fact or an arm's assignment names, which is the part
  of a zone or octagon the program asked for. **Convex polyhedra** keep the
  exact convex hull at a join; the hull of two polyhedra can have
  exponentially many faces, so its cost has no fixed polynomial bound. No
  convex domain proves a fact that depends on which arm was taken (families
  FLAGS, SWAPS below), so none replaces the selector.
- **Bounded disjunctions** (keep up to d states apart after a join) must
  choose which states to merge when there are more than d. Merging the
  oldest first is WIN seen from the front: the nearest joins stay apart.
- **Lazy case splitting as in DPLL(T) or CDCL** decides the same relation as
  SEL. Its cost depends on the search, so by F2 it can serve as an
  implementation of a bounded rule but not as the bound.
- **Interpolation** finds a fact at a cut that proves the goal. The fact it
  finds depends on the proof search, so it cannot decide a verdict. It can
  suggest the invariant to state, which is a matter for diagnostics.

## Input families

`k` is the size. Every family is a program in the model's small language
(integer values, linear definitions, facts, `if` and `match` with join
values, obligations). "Bump" means `if c_i { x = x + 1 }` on an unknown
condition `c_i`.

| Family | Program | Obligation |
| --- | --- | --- |
| CHAIN(k) | `x` in 0..100, then k bumps of `x` | `x <= 100 + k` |
| CHAINSYM(k) | `x <= n` for unknown `n`, then k bumps | `x <= n + k` |
| ADDENDS(k) | k times `if c_i { x = x + y_i }`, each `y_i >= 0` | `x <= x_0 + y_1 + .. + y_k` |
| GUARDED(k) | `i < n`, then k times `if i + 1 < n { i = i + 1 }` | `i < n` (a subscript bound, or a loop invariant on the backedge) |
| MINSEL(k) | k times `m = if a_i < m { a_i } else { m }` | `m <= a_j` for one j |
| BINSEARCH | one loop body of a binary search: `low <= high <= len`, `mid` between, `if c { low = mid + 1 } else { high = mid }` | `high <= len`, `low <= high` |
| MATCH(k, m) | k times a `match` of m arms, arm j adds j to `x` | `x <= 100 + k(m - 1)` |
| NEST(d) | bumps nested d deep, each inside the taken arm of the one before | `x <= 100 + d` |
| WIDEREL(n, k) | k joins, each bumps all of n places | bound on one place |
| WIDEIRR(n, k) | n places, each with its own k bumps | bound on one place |
| FLAGS(m) | m times `ok_j = if y_j >= 5 { 1 } else { 0 }`, then the obligation inside `if ok_1 >= 1 { .. if ok_m >= 1 {` | `y_1 + .. + y_m >= 5m` |
| FLAGCHAIN(k) | `ok_1` as in FLAGS, then `ok_j = if ok_(j-1) >= 1 { 1 } else { 0 }`, then inside `if ok_k >= 1 {` | `y >= 5` |
| SWAPS(k) | k times `if c_i { y_i = 0; z_i = 1 } else { y_i = 1; z_i = 0 }` | sum of all `y_i + z_i` `<= k` |
| SWAPSINV(k) | SWAPS with `invariant y_i + z_i <= 1` after each join | the invariants, and the same sum |
| MIX(k) | one flag as in FLAGS, then CHAIN(k), then inside `if ok >= 1 {` | `x - y <= 95 + k` |
| REL1(k) | one swap as in SWAPS, then CHAIN(k) | `y + z + x <= 101 + k` |
| OP5(k) | GUARDED(k) where before each `if` both "is the condition already decided true" and "already decided false" are asked | each such question (the answer is no), and the final bound |
| DENSE(n) | theory step only: n variables, 2n random inequalities with coefficients in -3..3 | Fourier-Motzkin work to decide feasibility |

FLAGS is the shape of a tag or a status flag set in one place and tested in
another. SWAPS is the hard case for every complete candidate: each join must
be split, and no partial choice is contradictory. OP5 exists because the
redundancy rule [OP-5] asks the derivation about every condition and expects
"not decided" in an accepted program, so the cost of an unproved question is
paid at every `if`.

## What the instruments can and cannot show

**The model** (`model/`, a standalone Rust program) implements T with plain
Fourier-Motzkin elimination, the candidates above, the families, and an
exhaustive evaluator.

- It can show: which constructed obligations each candidate proves; how the
  operation counts of each candidate grow with the size of each family; that
  a count is or is not independent of the split order.
- It cannot show: the cost of the production compiler, whose data structures
  differ; anything about nonlinear operations, element reads, writes,
  quantified facts or disequalities, which it does not model; how often any
  family occurs in real programs, which this study does not use.
- Its theory step substitutes equalities that have a unit coefficient and
  turns any other equality into two inequalities. Every family's equalities
  have a unit coefficient, so this does not differ from [RANGE-3] on them.

**The compiler probe** runs the earlier study's experiment compiler (tag
`derivation-ceiling-prototype`) on generated Whitefoot programs for CHAIN,
NEST and GUARDED, with its counter file on.

- It can show: that today's range derivation, with its backjumping, has the
  growth the model predicts for SEL on the same family, and what its
  weakest-bound variant does there.
- It cannot show anything about BND-R, SAT or WIN, which that compiler does
  not implement; its "weakest bound" is the earlier study's own rule
  (difference bounds against atoms an arm pairs the value with).
- Whether the counter file changes a verdict is checked: each program is
  checked with the counters on and off and the exit status compared.

**Wall times** come from a GitHub-hosted runner shared with other tenants.
They are the minimum of the repeated rounds. They are reported next to the
counts and decide no hypothesis.

## Hypotheses and what refutes each

**H1. The selector alone grows as 2^k on a chain of joins, and backjumping
does not help.**
Prediction: on CHAIN(k), SEL's leaves are exactly 2^k in both split orders
and with backjumping, in the model; in the compiler probe the baseline
configuration's `backjump_nodes` at the final requirement at least doubles
with each added join.
Refuted if, in either instrument, the count grows by a factor below 1.5 per
added join on average over k = 8..16.
What it decides: whether SEL can have a cap that admits a chain of twenty
bumps and still bounds the work.

**H2. With relational bounds the same chain needs no split, and the work is
polynomial.**
Prediction: on CHAIN, CHAINSYM, ADDENDS, GUARDED, MINSEL, MATCH, NEST and
WIDEREL, BND-R proves the obligation with zero branches, and the
Fourier-Motzkin pair count of the whole program, bounds included, grows no
faster than k^3.
Refuted if any of these is unproved, or if the pair count at k = 64 is more
than 64 times the count at k = 16 (which would be faster than k^3).

**H3. Recording every relation makes the facts per join grow with the
length of the chain; the restricted rule keeps them constant.**
Prediction: on CHAIN(k), BND-R-all records a number of bounds at the last
join that grows linearly in k, and BND-R records the same number at every
join.
Refuted if BND-R-all's count at the last join is the same at k = 8 and
k = 32, or BND-R's differs.
What it decides: whether "keep every bound every arm satisfies" is the cause
of the earlier study's doubled time, and whether the restriction removes it.
Also measured: on WIDEIRR(n, k), eager computation projects bounds for all
n·k joins and lazy computation only for the k joins the obligation reaches.
Refuted if lazy projections exceed half of eager ones at n = 16.

**H4. The decision table.** Predicted verdicts, "yes" for proved:

| Family | SEL | BND-I | BND-R | SAT | WIN(P) |
| --- | --- | --- | --- | --- | --- |
| CHAIN | yes | yes | yes | yes | yes |
| CHAINSYM, ADDENDS, GUARDED, MINSEL, BINSEARCH | yes | no | yes | yes | yes |
| MATCH, NEST, WIDEREL, WIDEIRR | yes | yes | yes | yes | yes |
| FLAGS(m), FLAGCHAIN(k), MIX(k) | yes | no | no | yes | yes |
| SWAPS(k) | yes | no | no | no | yes exactly when 2^k <= P |
| SWAPSINV(k) | yes | no | no | no | yes for P >= 2 |
| REL1(k) | yes | no | no | no | yes for P >= 4 |

WIN without forced cases is predicted to prove FLAGS(m) and FLAGCHAIN(k)
only when 2^m or 2^k is at most P. WIN without bounds is predicted to fail
CHAIN(k) when 2^k exceeds P.
Refuted by any cell that differs. A differing cell is reported as a finding
with its cause.

**H5. No structural count of open joins both admits and bounds.**
Prediction: on MIX(k) and REL1(k), BOTH proves the obligation in a number of
branches that does not grow with k in one split order and grows as 2^k in
the other; the number of open joins at the root is k + 1 in both.
Refuted if the branch count is the same in both orders, or if the open-join
count does not grow with k.
What it decides: whether "BOTH with a ceiling N on open joins" (the earlier
study's recommendation, with bounds added) can be the rule. If H5 holds, any
N below k + 1 rejects a program that needs one split, and by F3 the writer
cannot repair it with invariants.

**H6. WIN(P) is bounded by P and polynomial in the program.**
Prediction: for every family and size, a WIN(P) derivation has at most P
leaves; on SWAPS(k) with 2^k > P it stops at P leaves or fewer, unproved; on
CHAIN, GUARDED, FLAGCHAIN and OP5 the pair count per obligation grows no
faster than k^3 at fixed P.
Refuted if any derivation has more than P leaves, or a pair count at k = 64
exceeds 64 times the count at k = 16.

**H7. An unproved obligation has a repair by invariants.**
Prediction: SWAPSINV(k) is proved by WIN(2) for every k, including each
stated invariant, where SWAPS(k) is unproved for 2^k > P.
Refuted if any obligation of SWAPSINV(k) is unproved under WIN(2).
What it cannot show: that every true obligation has such a repair. That is
argued in RESULTS.md from the rule itself.

**H8. The instrument is sound, and SEL is exact on these families.**
Prediction: on every family at sizes small enough to enumerate every value
of every unknown, no candidate proves an obligation the enumeration finds
false, and SEL proves exactly the obligations the enumeration finds true.
Refuted by one disagreement.

**H9. The theory step as written has no polynomial bound, although its
verdict has one.**
The verdict of step 3 is rational infeasibility of the tightened
inequalities, which linear programming decides in polynomial time. The text
names Fourier-Motzkin elimination.
Prediction: on DENSE(n) the pair count of plain elimination grows faster
than any fixed power of n over n = 4..12: the ratio of successive counts
itself grows.
Refuted if the counts fit n^4 or lower (the count at n = 12 is at most 81
times the count at n = 4).
What it decides: whether the ceiling of 4,096 atoms bounds the work of one
theory call in practice. It does not test a replacement; that is left to a
later study if H9 holds.

**H10. The cost of an unproved question does not grow with the chain before
it.**
Prediction: on OP5(k), under WIN(P) the pair count of one "is it decided"
question at the last `if` grows no faster than linearly in k (the literals
its values reach), and under SEL without bounds it is found open after at
most k theory calls.
Refuted if the WIN count at k = 64 exceeds 8 times the count at k = 16.

## What the choice between candidates will rest on

- A candidate that fails H1's question (it has no cap that both admits a
  chain and bounds the work) is rejected as the rule.
- A candidate that cannot prove FLAGS is rejected as the rule, because a
  value's tag set in one place and tested in another is how `match` on a
  returned `Result` or an `Option` works, and a rule that loses it at every
  join forces a restatement at every join.
- Between the candidates that remain, the measured curves and the decision
  table go to the owner with a recommendation. P, and the exact definition
  of "near", are the owner's choices; RESULTS.md gives the cost of each P.

## Changes before the first measurement

- State bounds replace the entry value of every place whose arms disagree,
  not of one place at a time. Writing the BINSEARCH family showed the need:
  `low <= high` after a join that changes both must be asked of both join
  values together. No measurement had been taken.

## Changes after the first measurement

The first measurement is run 45907910884 (model) and the probe job of run
45906624486. Two of its results led to additions. The hypotheses below were
written after those results and before the measurement of what they add
(model commit `8cf761bf3`).

- H2 failed on ADDENDS: from k = 24 the model's elimination reaches its
  capacity. H10 failed: under WIN, one unproved question of OP5 costs work
  that grows with the square of the chain, because forced cases cascade down
  the whole chain before the question is found open.

**H11. A simplex decision of the theory step gives the same verdicts and
has no such growth.** The model gains a second decision of step 3's last
condition, an exact simplex method with Bland's rule (`model/src/lp.rs`),
used by the candidates `bnd-rel-lp` and `win16-lp`.
Prediction: wherever both finish, each `-lp` candidate has the verdict of its
elimination twin on every family and size, and on the small instances agrees
with the exhaustive evaluation; ADDENDS is proved at every size; its pivot
count on ADDENDS at k = 64 is at most 64 times the count at k = 16; on
DENSE(n) it decides every instance up to n = 14.
Refuted by one differing verdict, by an ADDENDS size it does not finish, or
by faster growth.
What it cannot show: a polynomial worst-case bound for the simplex method
itself, which Bland's rule does not give. The bound rests on the verdict
being a linear-programming feasibility question.

**H12. Bounding the forced cases removes the quadratic cost of an unproved
question, at a stated loss.** Two forms are added, both with P = 16:
`win16-root` runs one pass of forced cases over the joins near at the root
and none below; `win16-depth8` lets at most 8 joins be decided by forced
cases on one branch.
Prediction: under both, the pair count of the last OP5 question at k = 64
is at most 8 times the count at k = 16. `win16-root` proves FLAGS(m) and
MIX(k) at every size and FLAGCHAIN(k) only up to k = 5. `win16-depth8`
proves FLAGS(m) and FLAGCHAIN(k) up to 12 and not from 14.
Refuted by a pair count that grows faster, or by a verdict that differs.

**A narrower "near", after run 45911803573.** H12's cost prediction failed
for both bounded forms: the last OP5 question still cost work that grew with
the square of the chain. The cause is the definition of "near" above. Every
arm's condition names the limit `n`, the obligation reaches `n`, so every
join of the chain was near and was probed. From model commit after
`8cf761bf3`, a join is near only when one of its join values is reached; a
value that an arm's literals merely name no longer makes the join near.
Prediction, written before the new measurement: the verdicts of H4 and H12
stay as predicted there; under `win16-root` and `win16-depth8` the pair
count of the last OP5 question at k = 64 is at most 8 times the count at
k = 16; under `win16`, whose forced cases are not bounded, it is more than
8 times, because the cascade still runs down the chain.
