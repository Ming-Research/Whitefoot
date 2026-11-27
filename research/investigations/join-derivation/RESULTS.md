# What the one derivation keeps at a join: results, version 2

Status: version 2, after the first review. `RESULTS-v1.md` is version 1 as
it was reviewed (tag `join-derivation-study-v1`); its numbers stand except
where "Answers to the review" below corrects them. This file gives what
version 2 measured, the choices that are the owner's, and an answer to each
finding of the review. It decides none of the owner's choices.

## Runs and data

| Run | What | Commit |
| --- | --- | --- |
| [45997947973](https://github.com/Ming-Research/Whitefoot/actions/runs/45997947973) | model: `check`, `measure`, `recount`, `false`, `dense`, one job each; **every number below is from this run unless it names another** | `2fb50ce87` (the model of `20ad8d594`) |
| [45957193473](https://github.com/Ming-Research/Whitefoot/actions/runs/45957193473), job `big` | the largest sizes, k = 64 to 1,024 | `20ad8d594` |
| [45906624486](https://github.com/Ming-Research/Whitefoot/actions/runs/45906624486), job `probe` | the compiler probe of version 1, unchanged | `c3a6fa94` |

Data in `data/`: `model-measure.csv` (34 families, 38 candidates),
`model-check.csv`, `model-false.csv`, `model-dense.csv`, `model-big.csv`,
`compiler-probe.tsv`; version 1's files are in `data/v1/`. The candidates
and families new in version 2 are defined in `DESIGN.md`, "Version 2".

Two runs before these were lost or superseded and are not used: one ended
at the job's time limit before it saved its results, one was cancelled when
the search for failing runs was replaced.

## Checks of the instrument

- **Exhaustive evaluation.** 91 small programs, 38 candidates, 4,978
  verdicts: no candidate proved a false obligation, and each SEL variant
  proved exactly the true ones.
- **False variants (V5: holds, with a gap).** Every family at every size
  with its last obligation tightened by one: 477 programs, 16,985 verdicts.
  **No candidate proved any of them**, with or without a known failing run.
  The search found a failing run for 446 of the 477. The 31 without one are
  BINSEARCH and ARMFACT and MULTIEXIT from k = 2. They are false too (set
  every `len` one above its `i`; in BINSEARCH take the arm with
  `mid + 1 == high`), but the search changes one value at a time and every
  such step breaks an assumed fact, so it does not get there. For these 31
  the evidence is the argument, not a run. V5's second sentence, that the
  search finds a run for every family, was wrong.
- **Counts are deterministic.** The second pass repeated 33,153 rows; all
  are identical in every count except 110 rows that the 20-second guard
  stopped.
- **The two theory decisions agree** wherever both finish (no differing
  verdict between a candidate and its `-lp` twin on any family and size).

## The cost of a whole function (finding 4, V4, V13)

Version 1 said the work grows with the square of the joins. That was wrong.
`work` counts every literal read or copied and every tableau entry touched.
Largest sizes, `bnd-rel` (bounds only, elimination):

| k | 64 | 128 | 256 | 512 | 1,024 |
| --- | --- | --- | --- | --- | --- |
| CHAIN, `work` | 1.26e6 | 9.23e6 | 7.05e7 | 5.50e8 | 4.35e9 |
| ratio to the size before | | 7.3 | 7.6 | 7.8 | 7.9 |
| CHAIN, time | 0.19 s | 1.4 s | 11 s | 90 s | 755 s |
| time over `work`, ns | 154 | 150 | 156 | 163 | 174 |

- **The cost is cubic**, for every candidate with join bounds, both theory
  decisions, on CHAIN, CHAINOBL, GUARDED and OP5: the ratio tends to 8.
  With the simplex decision CHAIN takes 663 s at k = 1,024 and OP5 578 s at
  k = 512.
- **`work` predicts the time**: time over `work` stays within 150 to 176 ns
  for elimination and 44 to 63 ns for simplex from k = 64 to 1,024 (V4's
  unit holds).
- The cause is in the rule, not in the copying version 1 blamed. A join's
  bound is computed over every literal its values reach; a join bound names
  the value before it, so one projection reads the whole chain, and each
  theory call on a chain of k literals costs k times k in this model.
- An obligation after every join (CHAINOBL) adds 16% to CHAIN: the bounds,
  not the obligations, are the cost.

**The candidate `win16-root-shallow`** (a join bound brought into a problem
does not bring in what its other values reach; V13):

| k | 64 | 128 | 256 | 512 | 1,024 |
| --- | --- | --- | --- | --- | --- |
| CHAIN, `work` | 4.34e4 | 1.52e5 | 5.67e5 | 2.18e6 | 8.56e6 |
| ratio | | 3.5 | 3.7 | 3.9 | 3.9 |
| CHAIN, time | 7 ms | 26 ms | 94 ms | 0.36 s | 1.5 s |
| OP5, `work` | 1.01e5 | 2.06e5 | 4.16e5 | 8.38e5 | 1.68e6 |
| OP5, time | 17 ms | 46 ms | 0.14 s | 0.46 s | 1.7 s |

- CHAIN is quadratic and 500 times cheaper at k = 1,024; GUARDED and OP5
  are linear in `work`. On GUARDED and OP5 the time grows faster than
  `work` (3.6 times per doubling at the end): the model's walk copies the
  literal list at every join, which `work` does not count. For this
  candidate the unit does not predict the time, and the time is the
  honest number: about quadratic.
- **What it gives up (as predicted):** ADDENDS from k = 6 and MINSEL from
  k = 10, whose proofs chain the transition bounds of every join. It keeps
  every other verdict of `win16-root`, CHAINSYM included, and proves
  ADDENDSINV: with the invariant written, the chain is not needed.

Whether to take this rule is the owner's choice 4 below.

## What each remedy does (findings 1, 6, 7, 8, 9)

No remedy loses a verdict that `win16-root` has, on any family and size
where both finish. What each gains:

| Finding | Family | `win16-root` | Remedy | Result |
| --- | --- | --- | --- | --- |
| 1 | ARMFACT, MULTIEXIT | proves none | `-af`: record the facts every arm states | proved at every size, 2 projections per join |
| 1 | ARMCORR | proves none | `-nf`: near by a stated fact's value | proved to k = 4 (16 leaves), not at 6 |
| 1 | cost on OP5 | 642, 2,850, 5,794 pairs at k = 16, 64, 128 | `-afnf` | the same counts |
| 6 | CHAINSYMINV, bounds at the last join | 11, 19, 35, 51 at k = 8, 16, 32, 48; guard at 64 | `-pres` | 4 at every size; 4.7 s at k = 128 |
| 6 | ADDENDSINV, the same (simplex) | 12, 20, 36, 52; guard at 64 | all remedies, simplex | 6 at every size; 3.8 s at k = 128 |
| 7 | MATCHSHAPES(m), BIGTAG(m) | proved at every m to 256 | | see below |
| 7 | MATCHSHAPES, `x <= 100` with no split | needs the probe | `-int` | proved with no probe |
| 8 | SAMECOND | k <= 2 | `-rootfix` | k <= 2 |
| 8 | ORDERXY / ORDERYX | k <= 4 / every size | `-rootfix` | every size both ways, to the guard at k = 96 |
| 8 | FLAGCHAIN, OP5CASC | k <= 4 | `-rootfix` | every size |
| 9 | TIE | (state bound first, for every candidate) | | proved by BND-R alone, no split |

- **V1 holds.** The two remedies for finding 1 are complementary: recorded
  arm facts cover what every arm states at any length; nearness covers
  facts that differ by arm, within P.
- **V6 holds.** The reviewer's prediction is confirmed: under the version 1
  rule the bounds per join grow with the chain once invariants of new
  shapes are written. Not carrying a weakened state bound removes it and
  costs no verdict here.
- **V7 was wrong, and so was the review's expectation.** `win16-root`
  proves BIGTAG and MATCHSHAPES at every m. The root probe tries every arm
  of a near join and proves the obligation when all are contradictory; that
  is a split of one join whatever its arm count, and it costs no product.
  So counting live arms (`-live`) and a free first split (`-free`) change no
  verdict on these families. `-free` does prove one join more elsewhere
  (FLAGCHAIN and OP5CASC to 6, PMONO decided at 5). The cost of a wide
  join is real: MATCHSHAPES needs 2m^2 projections (131,072 at m = 256,
  7.6 s), because each arm's value is a candidate and is tried in each arm.
  Two wide joins that must be told apart together were not measured.
- **V8 holds for the verdicts and was wrong for one cost.** The one-pass
  probe depends on statement order (ORDERXY against ORDERYX). Repeating the
  probe at the root removes that and proves flags derived from flags. Its
  cost: ORDERXY needs 334 probes at k = 16 and 4,414 at k = 64 (8.5 s) and
  reaches the guard at 96; the last OP5CASC question costs 34, 130, 258
  pairs at k = 16, 64, 128, which is linear (the prediction said more than
  8 times). On OP5 the repeated probe costs what one pass costs, because
  nothing there is decided at the root. Forced cases everywhere (`win16`)
  also prove SAMECOND at every size and keep the square of version 1's H10
  on OP5.
- One combination is slow for a reason not found: `win16-v2-lp` on TIE
  takes 3.7 s at k = 64 where `win16-root-lp` takes 0.11 s, and reaches
  the guard at 128.

## The combinations that ran (finding 3, V3)

- `win16-root-lp` and `sat-lp` have the verdicts of `win16-root` and `sat`
  wherever both finish, and finish ADDENDS at every size (2,561 pivots and
  0.25 s at k = 128). **V3's last clause failed**: they reach the guard on
  ADDENDSINV at 64 and CHAINSYMINV at 48 (finding 6's growth), and `sat-lp`
  on OP5 at 128.
- `win16-v2-fix-lp` (every remedy, repeated root probe, simplex) is the one
  candidate that proves every family the others prove, SWAPS, ARMCORR and
  SAMECOND within P excepted. Rows it does not finish: ORDERXY, TIE and
  OP5 at k = 128, all the 20-second guard. Its cost is cubic.

## The choices that are the owner's

### Choice 1. Does the rule fix which joins are split? (finding 2a)

Two decisions of the design tree stand against the recommended kind of
rule. `range-problem-size-ceilings` rejects ceilings on branches opened;
`split-on-any-open-item` rejects a split order stated in the specification,
because it would fix the checker's search. Version 1 used the first against
other candidates and did not say that a product P with "latest near join
first" goes against both.

| Option | What it is | Measured |
| --- | --- | --- |
| A. Keep both decisions, no bound | every open item is split, any order | 2^k on CHAIN in the model and in today's compiler (141 s at k = 16); breaks `checking-work-never-grows-exponentially` |
| B. Keep both, add a ceiling N on open joins as a hard error | the settled design's N | see choice 3: usable only with the cut reading of an invariant; within the ceiling the work still depends on the order (MIX(14): 2 leaves or 4,944) and reaches 65,536 leaves, 13 s, on SWAPS(16) |
| C. Change both: the rule says which joins a derivation looks at | near joins, a probe, a product P | at most P leaves per obligation on every family and size |
| D. Change neither, give up the split | bounds and the root probe only (`sat` with a one-pass or repeated root probe; P = 1) | no split order exists to fix; proves everything in the tables except SWAPS, ARMCORR and SAMECOND beyond one join |

Recommendation: C, with D as its first step. Under D nothing in the rule is
an order of search: each near join is probed alone. The owner would then
decide whether a product P above 1 is worth changing the two decisions;
the families that need it are those where two joins must be told apart
together and no condition helps (SWAPS, ARMCORR, SAMECOND).

### Choice 2. N as a hard error, or "out of reach is unproved"? (finding 2b)

The settled design (section 4) makes N a hard error that names the ceiling.
Version 1 replaced it without saying so.

| Option | Consequence |
| --- | --- |
| Hard error at a ceiling | The writer is told the size is the reason. A program is rejected although one split would prove it (MIX and REL1 from k = 16, FLAGS from m = 17 under `cut16`). The work per obligation is bounded by the ceiling only as 2^N. |
| Out of reach is unproved | No program is rejected for size alone. The message must name the join that was not looked at. The settled sentence "the ceiling bounds the work of each obligation by a constant" no longer holds: the work of one obligation grows with what its values reach (linearly under choice 4's second option, with the square of the chain otherwise). |

Recommendation: out of reach is unproved.

### Choice 3. Is a local invariant a fact or a cut? (finding 2c)

The settled design says a writer "cuts" with a local invariant. Version 1's
F3 assumed an invariant only adds a fact, and its case against a ceiling N
(H5) rested on F3. Under the cut reading F3 is false: the invariant closes
the joins before it, and the count starts again. `cut16` measures that
reading with N = 16.

| | Fact reading (`win16-root`) | Cut reading (`cut16`) |
| --- | --- | --- |
| SWAPSINV, MIXINV, REL1INV (the repair written) | proved at every size | proved at every size |
| MIX, REL1 (no invariant) | proved at every size | `ceiling` from k = 16 |
| FLAGS (no invariant) | proved at every size | `ceiling` from m = 24 (the next size after 16) |
| SWAPS (no invariant) | proved to k = 4 | proved to k = 16 with 65,536 leaves, `ceiling` from 24 |
| FLAGSCUT (an invariant about something else between the flags and their tests) | proved at every size | **unproved at every size** |
| work within the ceiling | at most 16 leaves | depends on the split order |

So H5's conclusion is corrected: with the cut reading a ceiling N does have
a repair by invariants, and version 1 was wrong to say it has none. What
the cut reading costs is the FLAGSCUT row: an invariant written for one
purpose hides what earlier arms established for every later obligation, so
adding an invariant can make a proved obligation unproved.

Recommendation: the fact reading. This is a choice of language meaning and
is the owner's.

### Choice 4. How far a join bound reaches (finding 4)

| Option | Cost at k = 1,024 joins on one chain | Verdicts |
| --- | --- | --- |
| Full reach (version 1's rule) | cubic: 755 s for CHAIN | all |
| Shallow reach | quadratic at most: 1.5 s for CHAIN | loses ADDENDS from k = 6 and MINSEL from k = 10; the repair is the invariant (ADDENDSINV is proved) |

Recommendation: shallow reach. The aim is compile time within an order of
magnitude of Rust and C++, and a cubic term in the joins of one function
cannot meet it. Not measured: shallow reach together with the remedy for
finding 6. Under both, CHAINSYM is predicted to need its invariants too,
because the weakened bound is no longer carried and the chain is no longer
read.

### Choice 5. Where the forced cases run (finding 8)

| Option | Proves | Costs |
| --- | --- | --- |
| One pass at the root | flags at any distance; verdict depends on statement order (ORDERXY to 4, ORDERYX at every size) | 2 probes per OP5 question |
| Repeated at the root | also ORDERXY, FLAGCHAIN, OP5CASC at every size | probes grow with the square where a cascade is long: 4,414 at ORDERXY(64) |
| Everywhere | also SAMECOND at every size | an unproved OP5 question costs the square of the chain |

Recommendation: repeated at the root. It removes the dependence on
statement order, which a writer cannot see, and costs more only where a
cascade is used. Version 1 said flags derived from flags were the only
ability given up; SAMECOND, ARMCORR beyond P and (under choice 4) ADDENDS
and MINSEL are given up too.

### Choice 6. P cannot be raised later, unless the redundancy rule asks less (finding 11)

[OP-5] rejects an `if` whose condition the derivation decides. A larger P
decides more: the PMONO question is decided for k <= 1 by the probe alone,
k <= 4 under the one-pass rule with P = 16, k <= 5 with P = 16 and
unbounded forced cases, k <= 7 with P = 64. A program with that `if` at
k = 6 is accepted at P = 16 and rejected at P = 64. A written invariant can
do the same to a later `if`.

| Option | Consequence |
| --- | --- |
| [OP-5] asks the same derivation | P is fixed for good once programs exist; every invariant may turn a later test redundant |
| [OP-5] asks only the bounds and the root probe | P can be raised later without rejecting an accepted program; a test that only a split shows redundant is kept |

Recommendation: the second. It is a change to a rule the owner kept
("redundant is rejected"), so it is the owner's. Not measured: how many
tests a written invariant turns redundant; OP5 with repairs was not built.

## Answers to the review

1. **Holds.** ARMFACT and MULTIEXIT are unproved by every version 1
   candidate at every size. Fixed by two remedies, measured: recorded arm
   facts and nearness by a stated fact. Both are in `win16-v2`.
2. **Holds, all three.** They are choices 1, 2 and 3. F3 is true only under
   the fact reading; H5's "no repair" is withdrawn for the cut reading. The
   review is also right that "the work depends on the split order" was no
   argument against a ceiling; the measured argument is FLAGSCUT.
3. **Holds.** `win16-root-lp` and `sat-lp` ran on every family. Version 1's
   "yes with simplex" cells for ADDENDS are now measured and true.
4. **Holds.** The cost is cubic; version 1's "square" and its explanation
   are withdrawn. `work` predicts the time for the full-reach candidates.
   The settled sentence about a constant per obligation is named in
   choice 2. Choice 4 gives a measured way to quadratic.
5. **Holds.** 16,985 verdicts on tightened obligations, none proved. The
   exhaustive check now has 342 false obligations among 4,978.
6. **Holds.** Measured as predicted; `-pres` fixes it without a lost
   verdict under full reach.
7. **Does not hold as stated; one part does.** BIGTAG is proved at every m:
   the root probe is a free split of one join. The cost part holds:
   2m^2 projections for a `match` whose arms give values of different
   shapes. A relation between a tag and a payload across two wide joins was
   not built.
8. **Holds.** SAMECOND and the order pair are measured; it is choice 5.
   Version 1's "the only ability given up" is withdrawn.
9. **Holds.** The rule now says a form that is both is a state bound; TIE is
   proved with no split.
10. **Both hold.** The compiler's weakest-bound configuration is cheap on
    CHAIN because its last requirement is `undecided` from k = 3: it did not
    prove it, and it is no usable comparison. That the earlier study's
    doubled time came from recording every bound at every join is an
    inference from this model's H3; the earlier variant was not run here.
11. **Holds.** It is choice 6.
12. **Both hold.** The certificate of the theory step is: the integer
    solution of the equalities as main's rule gives it (decision
    `integer-equality-solving`, not the model's unit-coefficient
    substitution), each tightening step with its divisor, and then a
    nonnegative combination of the tightened inequalities that sums to a
    false constant. The count of theory calls of one derivation is the
    probes and the leaves, plus the projections of every join the
    obligation reaches: joins times candidates times arms, which P does not
    bound.

## What stands from version 1

- The selector alone doubles per join, in the model and in today's
  compiler; backjumping does not help (H1).
- No convex bound replaces the selector (FLAGS, MIX); intervals fail every
  symbolic limit.
- Recording every bound every arm satisfies is far more costly than the
  restricted rule (H3), and version 2 shows the restricted rule needed one
  more restriction (finding 6).
- The theory step's text should name the verdict, not elimination (H9,
  H11): elimination reaches its capacity on ADDENDS from k = 24 and on
  dense systems of 7 variables.

## Still open

1. Shallow reach with the other remedies was not run as one candidate, so
   the combination this file leans toward has not been measured whole.
   Predicted loss: CHAINSYM without invariants.
2. The model has linear literals and joins only. Reads and writes of
   arrays, quantified facts, disequalities and nonlinear rows are open
   items too and are not in it.
3. Absolute compile time needs the rule in the compiler; the model gives
   growth.
4. Two wide `match`es that must be told apart together; a relation between
   a tag and a payload that needs a disjunction to state.
5. How many later tests a repair invariant turns redundant under [OP-5].
6. Why `win16-v2-lp` is slow on TIE.
7. The simplex method's own worst case is not bounded by measurement here.
8. No real program was measured, on purpose.
