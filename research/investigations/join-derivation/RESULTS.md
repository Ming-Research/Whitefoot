# What the one derivation keeps at a join: results

Status: measurements of 2026-11-26 and 2026-11-27 against the hypotheses of
`DESIGN.md`. The owner chooses; this record gives the numbers, a
recommendation, and what is still uncertain.

## Runs and data

| Run | What | Commit |
| --- | --- | --- |
| [45907910884](https://github.com/Ming-Research/Whitefoot/actions/runs/45907910884) | model, first measurement | `a3ece5ecf` |
| [45906624486](https://github.com/Ming-Research/Whitefoot/actions/runs/45906624486), job `probe` | compiler probe on tag `derivation-ceiling-prototype` | `c3a6fa94` |
| [45911803573](https://github.com/Ming-Research/Whitefoot/actions/runs/45911803573) | model with the simplex decision and the bounded forced cases | `8cf761bf3` |
| [45916524726](https://github.com/Ming-Research/Whitefoot/actions/runs/45916524726) | model with the narrower "near"; **every model number below is from this run** | `d23fbfa82` |

All ran on GitHub-hosted `ubuntu-24.04` runners. The data is in `data/`:
`model-measure.csv` (one `total` row per family, size and candidate, and
`oblig` rows for the last obligation and the last OP5 question),
`model-check.csv`, `model-dense.csv`, `compiler-probe.tsv`.

Candidate names in the data: `sel-late`, `sel-early`, `sel-backjump` are
SEL in the two split orders and with backjumping; `bnd-int`, `bnd-rel`,
`bnd-relall` are BND-I, BND-R, BND-R-all; `both-late`, `both-early` are
BOTH; `sat` is SAT; `winP` is WIN(P) with unbounded forced cases;
`win16-nosat` and `win16-nobnd` drop the forced cases or the bounds;
`win16-root` and `win16-depth8` are the two bounded forms of H12; `-lp`
uses the simplex decision; `-eager` computes every join's bounds at once.

## Checks of the instrument

- **Soundness and exactness (H8: holds).** On 43 small programs (every family
  at sizes 1 to 3) the exhaustive evaluator ran every value of every unknown.
  1,120 verdicts of the 20 candidates were compared: no candidate proved a
  false obligation, and each SEL variant proved exactly the true ones
  (`summary,compared,1120,findings,0`).
- **Counts are deterministic.** A second pass counted the 13 candidates
  without exponential growth again. Of 7,402 rows, 7,393 are identical in
  every count. The other 9 are rows that the 20-second guard stopped, where
  the count depends on when the guard fired.
- **The two decisions of the theory step agree (part of H11: holds).** On 972
  pairs of rows where the elimination and the simplex candidate both
  finished, the verdicts are the same.
- **The compiler probe's counter file changes no verdict.** Every generated
  program is accepted with the counters on and off, except `chain-18`, which
  is accepted without them and passes the 1,500-second limit with them.
- **Instrument guards.** A model run stops at 2^17 search nodes or 20
  seconds; elimination stops at 300,000 inequalities. A stopped row is
  marked in the `note` column and its verdict is `abort`. No conclusion
  below reads an aborted row as a verdict.

## H1. The selector alone: refuted as a rule, the hypothesis holds

Leaves of SEL on CHAIN(k), model:

| k | 4 | 8 | 12 | 16 |
| --- | --- | --- | --- | --- |
| `sel-late`, `sel-early`, `sel-backjump` | 16 | 256 | 4,096 | 65,536 |
| time, `sel-late` | 0.31 ms | 12 ms | 349 ms | 8.6 s |

The compiler probe, baseline configuration (selector only, with the
compiler's backjumping), on the same program written in Whitefoot:

| k | 4 | 8 | 12 | 16 | 18 |
| --- | --- | --- | --- | --- | --- |
| largest `backjump_nodes` of one obligation | 15 | 255 | 4,095 | 65,535 | 262,143 |
| derivation time, all obligations of the function | 5.6 ms | 229 ms | 6.2 s | 141 s | 649 s |

Both instruments double with each join; backjumping saves nothing, because
every refutation uses every join. The compiler's own "weakest bound"
configuration needs 2k + 2 nodes on the same programs (32 at k = 16, 0.40 s).
On GUARDED the compiler's baseline needs 2k - 1 nodes, and on NEST no split:
the conditions prune the first, and nested joins have d + 1 paths. The
growth is in joins one after another that change one value.

Consequence: a function with twenty conditional increments before a
subscript costs 2^20 branches under SEL, or is rejected by any ceiling
below 20 open joins. No ceiling both admits it and bounds the work.
SEL also has no polynomial bound on MATCH (4^k: 65,536 leaves at k = 8),
ADDENDS, SWAPS, MIX and WIDEREL.

## H2. Relational bounds: holds, with one failure that H11 removes

BND-R proves CHAIN, CHAINSYM, GUARDED, MINSEL, BINSEARCH, MATCH, NEST,
WIDEREL and WIDEIRR at every size with no branch. Fourier-Motzkin pairs of
the whole program:

| Family | k = 16 | k = 64 | ratio | k = 128 |
| --- | --- | --- | --- | --- |
| CHAIN | 3,105 | 49,281 | 15.9 | 196,865 |
| CHAINSYM | 2,134 | 33,142 | 15.5 | 131,830 |
| GUARDED | 2,392 | 37,240 | 15.6 | 148,216 |
| MINSEL | 114 | 450 | 3.9 | 898 |
| NEST | 109 | 397 | 3.6 | 781 |
| MATCH (4 arms) | 8,129 | 130,817 | 16.1 | |

The ratios are at or below 16: the work grows with the square of the number
of joins, not faster. The square comes from the bounds: each join's bound is
computed over the literals its values reach, and on a chain that is the
whole chain so far.

**The failure.** On ADDENDS the elimination reaches its capacity from
k = 24 (19,596 pairs at k = 16, 158,390 at the stop). H2 is refuted for
ADDENDS as written. The cause is the theory step, not the join rule: see
H9 and H11.

Wall time does not follow the pair count exactly: CHAIN takes 3.9 ms at
k = 16 and 164 ms at k = 64, a ratio of 42. The model copies its literal
list at every theory call, which adds a factor of k of its own. The counts
are the result; the times only show nothing is hidden behind them.

## H3. Which bounds a join records: holds

Bounds recorded at the last join of CHAIN(k):

| k | 8 | 16 | 32 |
| --- | --- | --- | --- |
| BND-R | 4 | 4 | 4 |
| BND-R-all | 18 | 34 | 66 |
| pairs, BND-R | 785 | 3,105 | 12,353 |
| pairs, BND-R-all | 16,157 | 380,281 | 10,442,737 |

Recording every bound every arm satisfies makes the facts per join grow
with the chain (2k + 2), and the work grows about as k^5 here. Keeping a
transition bound out of the next join's candidates keeps 4 facts per join
and proves the same obligations in every family measured.

Lazy against eager, WIDEIRR(16, 32): 256 projections lazily, 4,096 eagerly;
40.8 ms against 669 ms. Eager computation pays for every join of the
function; lazy computation pays for the joins some obligation reaches. Both
give the same verdicts. This, and BND-R-all, are the two ways a "weakest
bound" rule becomes slow, and the earlier study's variant had both.

## H4. What each candidate proves

"yes" is proved at every measured size up to 128; a number is the largest
size proved.

| Family | SEL | BND-I | BND-R | SAT | WIN(16) | `win16-root` |
| --- | --- | --- | --- | --- | --- | --- |
| CHAIN, MATCH, NEST, WIDEREL, WIDEIRR | yes, to the node cap | yes | yes | yes | yes | yes |
| CHAINSYM, GUARDED, MINSEL, BINSEARCH | yes | **no** | yes | yes | yes | yes |
| ADDENDS | yes, to the node cap | no | yes with simplex | yes with simplex | yes with simplex | yes with simplex |
| FLAGS(m) | yes | no | **no** | yes | yes | yes |
| FLAGCHAIN(k) | yes | no | no | yes | yes | 4 (fails at 6) |
| MIX(k) | yes, to the node cap | no | no | yes | yes | yes |
| REL1(k) | yes, to the node cap | no | no | yes | yes | yes |
| SWAPS(k) | yes | no | no | 1 | 4 (fails at 6) | 4 |
| SWAPSINV(k) | yes | no | no | yes | yes | yes |

Where the prediction was wrong:

- **SAT and WIN prove one join more than predicted.** SAT proves SWAPS(1),
  SWAPSINV and REL1, and WIN(P) proves SWAPS up to log2(P) + 1 where sizes
  allow the reading (WIN(2): 2, WIN(4): 3, WIN(64): 6). The forced-case step
  probes every arm of a near join, and when every arm is contradictory the
  branch is proved. That is a split of one join that costs no product. The
  rule is as `DESIGN.md` states it; the prediction overlooked this.
- **`win16-depth8` loses REL1 from k = 12** although REL1 needs one split.
  Its eight forced decisions are spent on the chain's joins, latest first,
  before the swap is reached. A count of forced decisions is consumed by
  joins that did not need deciding. `win16-root` has no such loss here.
- FLAGCHAIN under `win16-root` is proved to 4, not 5 as predicted.

Predictions that held: no convex bound proves FLAGS, FLAGCHAIN, MIX, REL1
or SWAPS; intervals fail every family with a symbolic limit; WIN without
forced cases proves FLAGS and FLAGCHAIN only to 4, and MIX and REL1 only to
k = 3, because the product is spent on the chain; WIN without bounds fails
CHAIN from k = 6.

## H5. A ceiling on open joins: holds for MIX, differs for REL1

BOTH (selector and bounds, split every open join), leaves:

| k | 4 | 8 | 12 | 16 | joins the obligation depends on |
| --- | --- | --- | --- | --- | --- |
| MIX, latest join first | 32 | 382 | 2,380 | 9,402 | k + 1 |
| MIX, earliest first | 2 | 2 | 2 | 2 | k + 1 |
| REL1, latest first | 6 | 10 | 14 | 18 | k + 1 |
| REL1, earliest first | 2 | 2 | 2 | 2 | k + 1 |

One split proves both obligations. The count of open joins is k + 1, so a
ceiling N rejects them for k >= N, and a local invariant does not lower the
count (F3 of `DESIGN.md`). The work depends on the split order, exponentially
on MIX and linearly on REL1 (the prediction said exponentially for both).
So neither the open-join count nor the work done can be the fixed rule. This
is the case against "selector plus bounds, with a ceiling N", which is also
what the two-step idea becomes once the second step needs a bound.

## H6 and H7. The bounded split: holds

- No WIN(P) derivation has more than P leaves on any family and size. On
  SWAPS(k) beyond its reach it stops unproved after one leaf at the root.
- At fixed P the work is polynomial: SWAPS under `win16-root` 2,205 pairs at
  k = 16 and 33,501 at k = 64 (ratio 15.2); FLAGS 663 and 10,335 (15.6).
- SWAPSINV is proved by WIN(2) and by SAT at every size, each stated
  invariant included, with 2 probes per invariant and 1,150 pairs at
  k = 128. An invariant after a join is proved by that join's arms alone,
  and the next obligation uses it as a plain fact.

## H9 and H11. The theory step

DENSE(n), three seeds per n: elimination decides every instance for n <= 6
(at most 214 pairs) and reaches its capacity of 300,000 inequalities on 23
of the 24 instances from n = 7 to 14, the first with 14 inequalities over 7
variables. **H9 holds.** The ceiling of 4,096 atoms does not bound one
theory call if the call is elimination.

Simplex on ADDENDS: proved at every size; 366 pivots at k = 16, 1,470 at
k = 64 (ratio 4.0), 248 ms for the whole program at k = 128, where
elimination stops at k = 24. On CHAIN at k = 128 the simplex candidate
takes 875 ms against 1,237 ms. **H11 holds** for ADDENDS and for the
agreement of verdicts.

What this does not show: DENSE says nothing about the simplex method. Its
instances have the origin as a solution, where this simplex starts, so it
needs no pivot there. And Bland's rule gives termination, not a polynomial
bound. The claim that stands is narrower: the verdict of step 3's last
condition is rational feasibility of a fixed set of inequalities, so any
exact decision of feasibility gives the same verdict, and the text should
not make elimination part of the rule.

## H10 and H12. The cost of a question whose answer is "not decided"

The redundancy rule [OP-5] asks about every condition, and an accepted
program answers "not decided". Pairs spent on the last such question of
OP5(k):

| k | 16 | 32 | 64 | 128 | ratio 64 to 16 |
| --- | --- | --- | --- | --- | --- |
| BND-R (no selector) | 324 | 676 | 1,380 | 2,788 | 4.3 |
| `win16-nosat` | 528 | 1,136 | 2,352 | 4,784 | 4.5 |
| `win16-root` | 642 | 1,378 | 2,850 | 5,794 | 4.4 |
| `win16-depth8` | 1,130 | 2,890 | 6,410 | 13,450 | 5.7 |
| `win16`, forced cases unbounded | 1,191 | 4,439 | 17,079 | guard | 14.3 |

Whole OP5 program, time: `win16` 2.6 s at k = 64 and 11.9 s at k = 96;
`win16-root` 0.49 s and 1.6 s; BND-R 0.21 s and 0.66 s.

- **H10 is refuted for unbounded forced cases.** One arm of the last join
  contradicts an arm of the join before it, and so on down the chain, so an
  open question is found open only after k decisions.
- **H12's first form failed, its second holds.** With "near" as first
  defined, both bounded forms still grew with the square, because every
  arm's condition names the limit `n` and so every join was near. With a
  join near only by a join value, `win16-root` probes 2 arms per question
  at every k, and the cost is linear.

## The recommended design

1. **A join keeps both.** A place whose arms disagree holds a join value
   defined by the arm taken (as settled), and the join records the
   relational bounds of BND-R as unconditional facts: the join value against
   each arm's value where that is an expression of entry values, and each
   bound that held of the place on entry, with the weakest constant over the
   arms. A transition bound is not carried to the next join. The bounds are
   a function of the program, so a checker may compute them when they are
   first needed.
2. **The decision step is bounded by what it looks at, not by a ceiling.**
   A join is near when the obligation's values, the values of the conditions
   on the way to it, or the values of a case already split reach one of its
   join values through the problem's literals other than join bounds. At the
   root, each near join is probed once: all arms contradictory proves the
   obligation, one arm left is taken. Then the latest near join is split
   while the product of arm counts on the branch stays within P. A join out
   of reach keeps its bounds only.
3. **No obligation is rejected for its size.** An obligation out of reach is
   unproved like any other. The message names the nearest join that was not
   split and whose join values the obligation reaches, and says to state the
   needed relation as a local `invariant` after that join. The invariant is
   proved from that join's arms and is then a plain fact (H7).
4. **The theory step's text names the verdict, not elimination.** Any exact
   decision of rational feasibility gives it.

P is the owner's choice. Measured cost: at most P leaves per obligation, and
on SWAPS, the family where every leaf is needed, WIN(16) proves 4 joins and
WIN(64) proves 6. Each doubling of P buys one more join whose arms must be
told apart with no condition to help. P = 16 is recommended as a start: the
forced cases already cover a flag tested by a condition at any distance
(FLAGS at every size), so P is spent only on relations between two values
of one join that no bound expresses.

### Why the others are rejected

| Candidate | Why |
| --- | --- |
| Selector alone | 2^k on a chain, in the model and in today's compiler; no ceiling admits the chain and bounds the work (H1). It breaks decision `checking-work-never-grows-exponentially`. |
| Weakest bounds alone, any convex domain | Cannot prove a fact that depends on which arm was taken (FLAGS, MIX, SWAPS): a tag or flag set in one place and tested in another. Intervals also fail every symbolic limit. |
| Both, with a ceiling N on open joins; the two-step idea with a ceiling on the second step | The count is k + 1 where one split suffices, an invariant does not lower it, and the work depends on the split order (H5). |
| Both, every bound every arm satisfies, computed at every join | Facts per join grow with the chain and the work with them (H3); this is where the earlier study's doubled time came from. |
| Unbounded forced cases | An unproved question costs work that grows with the square of the chain, and [OP-5] asks one at every `if` (H10). |
| A count of forced decisions | Spent on joins that needed no decision; loses REL1 (H4). |
| Zones, octagons, polyhedra as the state | Quadratic or worse in the live values at every statement, or exponential at a join, and still without the selector's facts. Not measured here; rejected on their known complexity and on FLAGS. |
| A cap on branches opened, CDCL, interpolation as the rule | The work or the fact found depends on the search (F2). They remain open to an implementation as shortcuts, and interpolation to the diagnostic as a way to suggest the invariant. |

### Loops, calls, arrays

A loop head and a call cut the chain of joins for every candidate alike:
past them a place holds a fresh value with the invariants or postconditions
as facts. So k in every table above is the number of joins between two cuts
in one function, and a loop body of twenty conditional updates is CHAIN(20)
or GUARDED(20). No candidate needs widening, because the head's facts are
the written invariants. The state after a loop is a join of its exits, and
gets bounds like any join. A subscript by a join value and a requirement
that names one are the obligations these families already ask (GUARDED's is
a subscript bound). Reads of a written array add open items of their own
(which write a read meets); the model does not hold them, and the product P
must count them with the joins. That is not measured.

### What a Lean model would state

The model's language is small enough to state directly: values, linear
literals, a join as a list of arms each with its literals, a problem as
literals plus joins plus a goal, and a valuation that satisfies a problem
when it satisfies the literals and, for each join, the literals of some arm.

- **Soundness of the theory step.** If step 3 calls a set contradictory,
  no integer valuation satisfies it. The proof object is a nonnegative
  combination of the tightened inequalities that sums to a false constant;
  the checker of that combination is a few lines and independent of how it
  was found (elimination or simplex).
- **Bounds are consequences.** If each arm's literals entail `form <= c_i`
  with the arm's value in place of the join value, every valuation that
  satisfies the join satisfies `form <= max c_i`. One lemma, by cases on the
  arm; a contradictory arm is vacuous.
- **Soundness of the decision step.** Deciding a join whose other arms are
  contradictory, and splitting a join into its arms, both preserve the set
  of satisfying valuations; ignoring a join enlarges it. So "every branch
  contradictory" implies the goal. By induction on the search tree.
- **Determinism.** The verdict is a function of the problem: the probe is
  one pass in program order, the split order is fixed, and the theory
  verdict is a property of the literal set. Defined as a total function in
  Lean, this is true by construction; the theorem to state is that the
  theory verdict does not depend on which certificate is found.
- **Cost.** The number of theory calls of one derivation is bounded by the
  arms of the joins near at the root plus a multiple of P, by induction on
  the remaining product; the exact expression is to be read off the Lean
  function. That the
  implementation runs at this bound is a statement about the compiler's
  function, proved by showing it refines the Lean one.

## What is still uncertain

1. **The model is not the compiler.** It has linear literals and joins only:
   no element reads and writes, no quantified facts, no disequalities or
   exclusions, no nonlinear rows. The product P must govern those open
   items too, and how they share it is not designed here.
2. **Absolute compile time.** The model shows growth, quadratic in the joins
   between two cuts for a whole function. Its constants are not the
   compiler's. Whether the design meets the compile-time aim needs the rule
   built in the compiler's derivation and measured on the 14900K.
3. **The square itself.** A function with k joins on one chain costs k^2
   pairs because each join's bound is computed over everything its values
   reach. An incremental theory state could remove a factor of k. Not
   tried.
4. **Flags derived from flags.** `win16-root` proves FLAGCHAIN only to 4.
   The repair is an invariant that relates the flag and the fact, such as
   `5 * ok <= y`, after the join; this study did not run that repair.
   Unbounded forced cases prove it at any length and cost the square of H10.
   This is the one place where the recommended rule gives up something
   measured, and it is the owner's to weigh.
5. **The exact definition of "near" and the order of splits** decide
   verdicts at the edge of P (H12 changed "near" once for cost). They are
   fixed rules either way; the specification item should settle them with
   the diagnostic in mind.
6. **The simplex method's worst case.** Agreement and ADDENDS are measured;
   a polynomial bound is not, and DENSE does not test it.
7. **The bounds' candidates** are the facts that name a place's entry value.
   A relation that held of no entry value and is true after the join
   (SWAPS's `y + z <= 1`) is not found; the writer states it. Whether a
   wider candidate set is worth its cost was measured only at the two ends
   (BND-R and BND-R-all).
8. **No real program was measured, on purpose.** Nothing here says how many
   invariants Firn or Halo would need under P = 16.
