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

## Version 2: after the first review

A review of version 1 (tag `join-derivation-study-v1`) found twelve points.
This section was committed with the model changes it describes and before
any of their measurements. It adds families, candidate remedies and
hypotheses. `RESULTS.md` answers each finding by its number.

### Rules changed in the model for every candidate

- **A form that is both a state bound and a transition bound is a state
  bound** (finding 9). State candidates are now offered first.
- A local invariant is one statement, `Invariant`, in place of an obligation
  followed by a fact. Candidates read it as one more fact, except the two
  `cut16` candidates below.
- A new count, `work`: every literal handed to a theory call or copied to
  form one, every inequality visited in an elimination round, every pair,
  and every tableau row and entry a simplex step touches.

### New candidates

Each is `win16-root` (bounds, one probe pass at the root, P = 16) with one
change, so that each change can be read alone:

| Name | Change | For finding |
| --- | --- | --- |
| `win16-root-lp`, `sat-lp` | the simplex decision | 3 |
| `win16-root-af` | a join also records, for each fact an arm states over entry values (not the arm's own condition), the weakest constant over the arms | 1 |
| `win16-root-nf` | a join is also near by a value that such a stated fact names | 1 |
| `win16-root-afnf` | both | 1 |
| `win16-root-pres` | a state bound that some arm weakens is recorded but not offered to the next join | 6 |
| `win16-root-int` | a join also records the constant interval of each join value | 7 |
| `win16-root-live` | the product counts the arms the root probe left alive, and only those are split | 7 |
| `win16-root-free` | the first split of a derivation costs no product | 7 |
| `win16-rootfix` | the root probe repeats until nothing is decided; none below the root | 8 |
| `win16-v2`, `win16-v2-lp` | all of `af`, `nf`, `pres`, `int`, `live`, `free` together, one root pass | |
| `win16-v2-fix`, `win16-v2-fix-lp` | the same with the repeated root probe | |
| `cut16`, `cut16-early` | the reading of the settled design: selector and bounds, an invariant closes the joins before it (they keep bounds only), the root is tried first, then more than 16 open joins since the last invariant is the verdict `ceiling`, else every open join is split, latest or earliest first | 2 |

### New families

| Family | Program | Obligation |
| --- | --- | --- |
| ARMFACT(k) | k joins with no join value; both arms state `i_j < len_j` | sum of `i_j - len_j` `<= -k` |
| MULTIEXIT(k) | the same, the arms state `i_j < len_j` and `i_j <= len_j - 3` | the same |
| ARMCORR(k) | k joins with no join value; one arm states `y_j >= 5`, the other `z_j >= 5` | sum of `y_j + z_j` `>= 5k` |
| ADDENDSINV(k) | ADDENDS with `invariant x <= x_0 + y_1 + .. + y_i` after join i | the invariants and the same bound |
| CHAINSYMINV(k) | CHAINSYM with `invariant x <= n + w_j + j`, `w_j` new each time | the invariants and `x <= n + k` |
| CHAINOBL(k) | CHAIN with the bound asked after every join | every one |
| MATCHSHAPES(m) | one `match` of m arms, arm j sets `x = a_j`, each `a_j <= 100` | `x <= 100` |
| BIGTAG(m) | one `match` of m arms, arm j sets `tag = j % 3` and a `len` that is below `n` when the tag is 0; then inside `if tag == 0` | `len < n` |
| SAMECOND(k) | k times `if c { a += 1 }`, `if c { b += 1 }` on one `c` | `a <= b` |
| ORDERXY(k), ORDERYX(k) | k pairs `y_i == w_i`; `okx_i` flags `y_i >= 5`, `oky_i` flags `w_i >= 5`; the `okx` joins before the `oky` joins, or after; inside the conditions on every `okx_i` | every `oky_i` is 1 |
| TIE(k) | `lo <= hi`; `if c { lo = hi }`; k times `if d { lo += 1; hi += 1 }` | `lo <= hi` |
| PMONO(k) | SWAPS(k), then the question "is the sum already decided" | the question |
| OP5CASC(k) | FLAGCHAIN(k), and inside its condition k questions about unknowns | each question, and `y >= 5` |
| MIXINV(k), REL1INV(k) | MIX and REL1 with the invariant that repairs them after the first join (`5 * ok <= y`, `y + z <= 1`) | the invariant and the obligation |
| FLAGSCUT(m) | FLAGS with an invariant about an unrelated value between the flags and their tests | the same as FLAGS |

**The false variant.** For every family and size, the last obligation is
tightened by one. A search over 6,000 runs looks for one run on which the
tightened obligation fails; where it finds one, the obligation is known
false, and a candidate that proves it is unsound. The exhaustive check of
version 1 stays.

**The largest sizes.** CHAIN, CHAINOBL, GUARDED and OP5 are run at k = 64,
128, 256, 512 and 1,024 for `bnd-rel`, `bnd-rel-lp`, `win16-root`,
`win16-root-lp`, `win16-v2` and `win16-v2-lp`, each until one run passes
150 seconds.

### Hypotheses of version 2

**V1 (finding 1).** Prediction: `win16-root` proves ARMFACT, MULTIEXIT and
ARMCORR at no size. `-af` proves ARMFACT and MULTIEXIT at every size and
ARMCORR at none. `-nf` proves all three only up to k = 4 or 5. `-afnf`
proves ARMFACT and MULTIEXIT at every size and ARMCORR up to k = 4 or 5.
The pair count of the last OP5 question under `-afnf` at k = 64 stays within
8 times the count at k = 16.
Refuted by a verdict that differs or a faster growth.

**V2 (finding 2).** Prediction: `cut16` proves SWAPSINV, MIXINV and REL1INV
at every size, and gives `ceiling` on MIX and REL1 from k = 16 and on SWAPS
from k = 17 (so at 24). It does not prove FLAGSCUT at any size, which every
candidate that reads an invariant as a fact proves. On SWAPS(16) its work
differs between the two split orders by less than a factor of 2, because
every leaf is needed.
Refuted by a verdict that differs.
What it decides: nothing. It gives the owner the cut reading measured beside
the fact reading.

**V3 (finding 3).** Prediction: `win16-root-lp` and `sat-lp` have the
verdicts of `win16-root` and `sat` wherever both finish, finish ADDENDS at
every size, and no row of theirs up to k = 128 is stopped by a guard.
Refuted by a differing verdict or a stopped row.

**V4 (finding 4).** Prediction: on CHAIN, CHAINOBL, GUARDED and OP5, for
both theory decisions, time divided by `work` varies by less than a factor
of 2 between k = 64 and the largest size that finishes; and `work` grows
between the square and the cube of k: the ratio of `work` at 2k to `work` at
k is between 4 and 8 at the largest sizes. If the ratio is near 8, the
whole-function cost of every candidate with bounds is cubic in this model,
and version 1's "square" was wrong.
Refuted (for the unit) if time over `work` varies by more than 2.

**V5 (finding 5).** Prediction: no candidate proves a tightened obligation
for which a failing run was found, at any size. Every family but PMONO has
such a run at every size.
Refuted by one `UNSOUND` row. A program without a found run is reported and
counts for nothing.

**V6 (finding 6).** Prediction: on ADDENDSINV and CHAINSYMINV the bounds
recorded at the last join grow linearly with k under `win16-root` and are
constant under `win16-root-pres`; `-pres` proves every family that
`win16-root` proves.
Refuted if the count under `win16-root` is the same at k = 8 and k = 32, if
it grows under `-pres`, or if `-pres` loses a verdict.

**V7 (finding 7).** Prediction: on BIGTAG(m), `win16-root` proves m <= 16
only; `-live` proves up to m = 48 (16 live arms); `-free` proves every m
with m / 3 leaves or m leaves. On MATCHSHAPES(m) `win16-root` proves only
m <= 16, by splitting; `-int` proves every m with no split; the projections
of the one join grow as m squared under every candidate with bounds.
Refuted by a verdict that differs.

**V8 (finding 8).** Prediction: `win16-root` proves SAMECOND up to k = 2
and ORDERXY up to k = 4 and ORDERYX at every size; `win16-rootfix` proves
both orders at every size and SAMECOND up to k = 2; `win16` (forced cases
everywhere) proves all three at every size. On OP5CASC the pair count of
the last question at k = 64 over k = 16 is at most 8 under `win16-root` and
more than 8 under `win16-rootfix` and `win16`.
Refuted by a verdict or a growth that differs.

**V9 (finding 9).** Prediction: with state candidates first, TIE is proved
at every size by BND-R alone.
Refuted if it is not.

**V11 (finding 11).** Prediction: the PMONO question is decided for k <= 4
under `win16` and `win16-root`, for k <= 6 under `win64`, and only for k = 1
under `sat`; so a program with that `if` at k = 5 or 6 is accepted at
P = 16 and rejected at P = 64.
Refuted if the sizes at which the question is decided do not grow with P.

**V13 (finding 4, added after run 45928696201 and before its own
measurement).** That run's `work` on CHAIN grows by 7.3 from k = 64 to
k = 128 under `bnd-rel`: cubic. Each join's bound is computed over every
literal its values reach, and a join bound reaches the join before it, so
one projection reads the whole chain. The candidates `win16-root-shallow`
and `win16-root-shallow-lp` change one thing: a join bound that is brought
into a problem does not bring in what its other values reach.
Prediction: on CHAIN, CHAINOBL, CHAINSYM, GUARDED and OP5 the verdicts stay
and `work` at k = 128 is at most 4.5 times `work` at k = 64. The change
loses ADDENDS and MINSEL, whose proofs chain transition bounds through
every join, and keeps ADDENDSINV.
Refuted by a faster growth or by other verdicts.

**The search for a failing run, changed after run 45928696201.** That run
found a failing run for 269 of the 477 tightened programs. The search drew
values at random and cut every range to 60 above its lower end, so it missed
CHAIN, whose start value must be 100, and every family that needs many
values set exactly. It is replaced by coordinate ascent on the value of the
obligation's form from five starts. V5's prediction is unchanged.

## Version 3: after the second review

The second review (of tag `join-derivation-study-v2`) has fourteen findings.
This section was committed with the model changes it describes and before
their measurement.

### Changes to the instrument, for every candidate

- **A second theory step, `indexed.rs`** (finding 2): the same verdict; a
  substitution or an elimination touches only the rows that hold the
  variable. Candidates named `-ix` and every `v3-` candidate use it.
- **Counts of what the rule asks**, apart from what the theory step does
  with it: `rule_lits` and `rule_terms` are the literals and the terms
  handed to theory calls; `max_atoms` is the most distinct values in one
  call, to be read against the settled ceiling of 4,096 atoms (finding 8).
- **A guard met inside a bound's projection voids the run** (finding 8): the
  verdicts of that candidate on that program are `abort`. Before, the bound
  was silently left out.
- **A join's values include those of the joins nested in its arms**
  (finding 5), so that SEL reaches a join through a fact an inner branch
  states.
- A question that was stopped by a guard is reported as stopped, not as
  decided; the mode `false-inv` tightens the last invariant of a program
  instead of its last obligation (finding 14).

### Candidates

`v3-full` is one candidate that holds every remedy this study leans to:
relational bounds; the facts every arm states, those of nested joins
included (`nested`, finding 5); nearness by a stated fact; a weakened state
bound not carried on; the constant interval of a join value; the product
counted over live arms; the root probe repeated; P = 16; the indexed theory
step. It has no free first split (finding 6). The others change one thing:

| Name | Change from `v3-full` | For finding |
| --- | --- | --- |
| `v3-ps` | a join bound does not bring in what its other values reach while a join's bounds are computed; an obligation's problem reaches everything | 2 |
| `v3-shallow` | the same also in an obligation's problem | 1 |
| `v3-ps-lp` | `v3-ps` with the simplex decision | |
| `v3-ps-q1`, `v3-ps-q0` | `v3-ps` where a question of the redundancy rule gets one probe pass, or the bounds alone | 4, 12 |
| `v3-full-d`, `v3-ps-d`, `v3-shallow-d` | no split at all (option D of choice 1) | 11 |
| `v3-ps-nonest` | without the nested arm facts | 5 |
| `v3-ps-strict` | a transition bound only for an arm value that names the place's entry value | 10 |
| `v3-ps-nopres` | a weakened state bound is carried on | 1 |
| `v3-ps-onepass` | one probe pass at the root | |
| `cut16-cone` | the cut reading where an invariant closes only the joins its own goal reaches | 9 |
| `bnd-rel-ix`, `win16-root-ix`, `win16-rootfix-ix`, `win16-root-shallow-ix` | version 2's candidates on the indexed theory step | 2, 4 |

### Families

| Family | Program | Obligation |
| --- | --- | --- |
| LIMITCHAIN(k) | GUARDED with a limit that grows: `n_j = n_(j-1) + d_j` before each `if i + 1 < n_j { i += 1 }` | `i < n_k` |
| SUMOBL(k) | no join: `s_j = s_(j-1) + a_j`, each `a_j` in 0..255 | after each step, `s_j` within its bound |
| NESTFACT(k) | k pairs; each `i_j < len_j` is stated in every innermost arm of a branch two deep | sum of `i_j - len_j` `<= -k` |
| NESTDEEP(d) | one pair, stated in every innermost arm of a branch d deep | `i < len` |
| SPLITAFTER(k), SPLITBEFORE(k) | k swaps and a `match` of eight arms that sets `t` to the arm's number, after or before the swaps | sum of `y_i + z_i`, plus `t`, `<= k + 7` |
| BIGTAG2(m) | two BIGTAG matches; inside both tests | `len_1 + len_2 <= 2n - 2` |

The largest sizes (k = 64 to 1,024) are run for CHAIN, CHAINOBL, GUARDED,
OP5, CHAINSYM, CHAINSYMINV, ADDENDSINV, LIMITCHAIN, SUMOBL and MINSEL.

### Hypotheses

**W1 (finding 2).** With the indexed theory step, `work` and time of
`bnd-rel-ix` on CHAIN, CHAINOBL and OP5 grow by at most 4.5 per doubling of
k from 256 to 1,024 (quadratic), where `bnd-rel` grows by 8. `rule_terms`
of the full-reach candidates grows by about 4 per doubling. If so, the
third factor of k in version 2 was the model's theory step and choice 4 of
version 2 traded verdicts for nothing.
Refuted if `bnd-rel-ix` grows by more than 6 per doubling at the top.

**W2 (finding 2).** `v3-ps` has every verdict of `v3-full` on every family
and size, and its `rule_terms` for the whole program is at most that of
`v3-full`.
Refuted by one differing verdict.

**W3 (finding 1).** `v3-shallow` loses CHAINSYM from about k = 6 and
`v3-shallow-d` from k = 2; both lose ADDENDS and MINSEL. On CHAINSYMINV and
ADDENDSINV, `v3-shallow` and `v3-ps` keep the bounds at the last join
constant and grow at most cubically in k; `v3-ps-nopres` grows faster and
meets a guard by k = 128.
Refuted by other verdicts or growths.

**W4 (finding 3).** On LIMITCHAIN and SUMOBL, `rule_terms` grows with the
square of k or faster under every candidate, `v3-shallow` included: shallow
reach does not stop a chain of definitions.
Refuted if `v3-shallow`'s `rule_terms` grows by less than 3 per doubling.

**W5 (finding 4).** On OP5 the probes of `v3-ps` grow with the square of k
(the forced cases at the root cascade down the chain), and those of
`v3-ps-q1` and `v3-ps-q0` grow linearly, with the same verdict on every
obligation that is not a question.
Refuted if `v3-ps`'s probes at k = 128 are less than 3 times those at 64.

**W6 (finding 5).** On NESTFACT and NESTDEEP: `sel-late` proves every size
it finishes (with nested values); `v3-ps-nonest` proves none; `v3-ps`
proves every size. The exhaustive check reports no `SEL-INEXACT`.
Refuted by a differing verdict.

**W7 (finding 7).** SPLITBEFORE(k) is proved by `v3-ps` for k <= 4 and
SPLITAFTER(k) only for k <= 1 (the `match` is split first and takes 8 of
the 16): the verdict depends on the order of unrelated statements.
Refuted if the two families have the same verdicts.

**W8 (finding 8).** Under full reach, `max_atoms` of CHAIN, GUARDED and OP5
grows linearly with k and passes 4,096 before k = 1,024; under `v3-ps` the
same holds for the obligation's problem. ADDENDSINV rows that version 2
reported as proved with a `capacity` note are now `abort` for the
candidates on the old theory step.
Refuted if `max_atoms` stays below 4,096 at k = 1,024.

**W9 (finding 9).** `cut16-cone` proves FLAGSCUT at every size up to the
ceiling (m <= 16) and keeps `cut16`'s verdicts on SWAPSINV, MIXINV, REL1INV,
MIX, REL1 and SWAPS.
Refuted by a differing verdict.

**W10 (finding 10).** `v3-ps-strict` needs about 2m projections on
MATCHSHAPES(m) where `v3-ps` needs 2m^2; it loses MINSEL (whose proof needs
the join value against the other arm's value) and no other family.
Refuted by other counts or losses.

**W11 (finding 11).** `v3-ps-d` (no split) proves everything `v3-ps` proves
except SWAPS, ARMCORR, SAMECOND, PMONO's decided sizes and BIGTAG2 beyond
what one probed join gives.
Refuted by another loss.

**W12.** BIGTAG2(m) is proved by `v3-ps` only while the live arms of the
two matches multiply to at most 16 (m <= 12).

**W13 (finding 14).** No candidate proves a tightened invariant for which a
failing run is found, nor any obligation after it.

## Version 4: after the third review

The third review (of tag `join-derivation-study-v3`) has eight findings.
This section was committed with the model changes it describes and before
their measurement.

### Changes to the instrument

- **The indexed theory step solves the equalities over the integers
  completely** (finding 3). An equality with no unit coefficient is reduced
  by steps of Euclid's algorithm, each a one-to-one change of the integer
  solutions, until it has one. The theory step of `theory.rs` is kept as it
  was, to show the difference. Where an equality has several unit
  coefficients, the variable that the fewest rows hold is the one solved
  for; by [RANGE-3]'s own words no choice of parameters changes the verdict,
  and this choice keeps the rows short on a chain of definitions (finding
  7).
- **The simplex decision can follow the indexed equality step**, so that one
  theory step has both.
- The mode `props` checks three properties of each theory step on 40,000
  random systems of 3 or 4 values with coefficients in -4..4: the verdict is
  the same when the literals are reversed or rotated; a set that is not
  contradictory has no contradictory subset of one literal fewer; a
  contradictory set has no integer solution in -4..4.
- Every row carries `prog_terms`, the terms of the program's own literals,
  so that growth can be read against the program's size (finding 8).
- Jobs `shapes` and `survey` (finding 7): the shapes of CHAIN, GUARDED,
  SUMOBL and LIMITCHAIN written as C and as Rust and checked by clang and
  rustc at k = 64 to 16,384; and a count of branch points per function in
  Linux 6.6, SQLite 3.46, CPython 3.12 and Lua 5.4, as a bound on the sizes
  this study should reach.

### Candidates

`v4` is the candidate that the recommendations of version 3's choices
describe, taken together (finding 5): no split (choice 1, D); the root probe
repeated for obligations and the bounds alone for questions of the
redundancy rule (choice 5); bounds computed with shallow reach (choice 4);
the remedies of choice 7 except live arms, which mean nothing without a
split; complete integer equalities and the simplex decision. Its shallow
reach differs from version 3's in one thing (finding 2): the other values
of a join bound still bring in the facts that name them, and what those
facts reach through facts, but no further join bound (`bridge`).

| Name | Change from `v4` | For finding |
| --- | --- | --- |
| `v4-fm` | elimination in place of simplex | 5 |
| `v4-work`, `v4-work-fm` | after the first probe pass, only the joins that share a value with a case just decided, or with a literal next to it, are probed again | 1 |
| `v4-onepass` | one probe pass | 1 |
| `v4-c` | P = 16 with live arms (option C) | 5 |
| `v4-ps` | version 3's shallow reach, without the facts beside a bound | 2 |
| `v4-full` | full reach | 2 |
| `v4-q` | questions asked as obligations | |

### Families

| Family | Program | Obligation |
| --- | --- | --- |
| FLAGNOISE(k), FLAGNOISE2(k) | FLAGCHAIN(k) and k joins `t_i = if c_i { 1 } else { 0 }` that need no decision, after the flags or before them; inside the last flag's test | `y + t_1 + .. + t_k >= 5` |
| PSBRIDGE(k) | k pairs: `z = if c { n - 5 } else { n - 3 }` with `n >= 10`; `x <= z`; `if d { x = 5 }` | sum of every `x - z` `<= 0` |
| STRIDE(k) | `3x == 2y`, `x <= 5`; k times `if c { x = x + 6 }` | `x <= 4 + 6k` |
| SUMOBLCUT(k) | SUMOBL where after every 16 steps the sum is stated within its bound and replaced by a new value that has only that bound | every step's bound |

The largest sizes go to 4,096 and now include ORDERXY, FLAGNOISE,
FLAGNOISE2, PSBRIDGE, SUMOBLCUT and STRIDE; a candidate stops on a family
after a run of more than 60 seconds.

### Hypotheses

**X1 (finding 3).** The unit-only theory step shows order changes and
additions that revive a contradiction among the 40,000 systems; the
complete one shows none of either, with elimination and with simplex; no
theory step calls a set contradictory that has an integer solution in the
box. STRIDE is proved at every size by every `v4` candidate and by no
candidate on `theory.rs`.
Refuted by one order change or revival under the complete step, or by one
unsound verdict.

**X2 (finding 1).** On FLAGNOISE one obligation costs about 2k^2 probes
under `v4` and `rule_terms` grows by about 8 per doubling; on FLAGNOISE2
about 4k probes. Under `v4-work` both cost a number of probes linear in k.
`v4-work` has the verdicts of `v4` on every family, ORDERXY included.
Refuted if `v4-work`'s probes on FLAGNOISE at k = 256 are more than 3 times
those at 128, or by a lost verdict (which is then reported as the cost of
the rule).

**X3 (finding 2).** PSBRIDGE is proved at every size by `v4` and `v4-full`
with no split and by `v4-ps` only at k = 1. On CHAIN and GUARDED the
`rule_terms` of `v4` stay linear in k.
Refuted by a differing verdict, or if `v4`'s `rule_terms` on CHAIN grow by
more than 2.5 per doubling.

**X4 (finding 5).** `v4` finishes every family at every size up to 128 with
no row stopped by a guard, ADDENDSINV included. Against `v3-ps` it loses
exactly the families option D loses (SWAPS, ARMCORR, SAMECOND, SPLITAFTER,
SPLITBEFORE and BIGTAG2 beyond one probed join, and the decided PMONO
question) and gains PSBRIDGE and STRIDE.
Refuted by a stopped row or another difference.

**X5 (finding 7).** With the rows kept short, the time of `v4-fm` on
LIMITCHAIN and SUMOBL grows by at most 5 per doubling of k up to 4,096.
clang and rustc grow by about 2 per doubling on the same shapes.
Refuted if LIMITCHAIN or SUMOBL grows by more than 6 per doubling at the
top.

**X6 (finding 4).** The most values in one theory call (`max_atoms`) under
`v4` passes 4,096 on LIMITCHAIN, SUMOBL, MINSEL and CHAINSYMINV before
k = 4,096, and stays below 100 on SUMOBLCUT at every size.
Refuted if `max_atoms` stays below 4,096 at k = 4,096 on those four.
