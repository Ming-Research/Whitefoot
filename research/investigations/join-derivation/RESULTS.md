# What the one derivation keeps at a join: results, version 3

Status: version 3, after the second review. `RESULTS-v2.md` and
`RESULTS-v1.md` are the earlier versions as they were reviewed. This file
gives what version 3 measured, the choices that are the owner's, and an
answer to each finding of the second review. It decides none of the owner's
choices.

## Run and data

Every number below is from run
[46021785248](https://github.com/Ming-Research/Whitefoot/actions/runs/46021785248)
on commit `110c3fd6e`: sixteen jobs on GitHub-hosted `ubuntu-24.04`
runners, one for each part of each measurement, each with its own artifact.
All sixteen finished.

Data in `data/`: `model-measure.csv` (41 families, 29 candidates),
`model-big.csv` (k = 64 to 1,024), `model-check.csv`, `model-false.csv`,
`model-false-inv.csv`, `model-dense.csv`; `data/v2/` and `data/v1/` hold
the earlier versions. Candidates and families are defined in `DESIGN.md`,
"Version 3". The hypotheses W1 to W13 there were committed with the model
changes and before this run.

## Checks of the instrument

- **Exhaustive evaluation.** 4,350 verdicts on the small programs of all 41
  families: no candidate proved a false obligation, each SEL variant proved
  exactly the true ones, NESTFACT and NESTDEEP included.
- **Tightened obligations.** 541 programs, 15,231 verdicts: none proved. A
  failing run was found for 502; the other 39 (ARMFACT, MULTIEXIT, NESTFACT
  from k = 2, and BINSEARCH) are false by argument only, as in version 2.
- **Tightened invariants (W13: holds).** The last invariant of every program
  that has one, tightened by one: 2,662 verdicts, none proved, a failing run
  found for every program.
- **Counts are deterministic.** 30,795 rows counted twice; all identical
  except 100 that a guard stopped.
- **A guard met inside a bound's projection now voids the row.** Version 2
  left that bound out and went on. The rows this changes are named under
  finding 8.

## The cost of a whole function (findings 1, 2, 3; W1 to W4)

Three counts are reported apart. `rule_terms` is what the rule asks the
theory step to read: the terms of every literal handed to a theory call.
`work` is what the model's theory step did with it. Time is time.

**The third factor of k was the model's theory step (W1: holds).** One chain
of k = 1,024 joins, full reach, bounds only:

| | time | `work` per doubling of k at the top | `rule_terms` per doubling |
| --- | --- | --- | --- |
| `bnd-rel`, the theory step of versions 1 and 2 | 653 s | 7.9 | 4.0 |
| `bnd-rel-ix`, the same rule on the indexed theory step | 9.2 s | 4.0 | 4.0 |

The verdicts of the two are the same on every family and size both finish.
Version 2's "cubic, and the cause is in the rule" is withdrawn: under full
reach the rule asks for a quadratic amount of reading, and a theory step
that touches only the rows holding a variable does a quadratic amount of
work. Version 2's choice 4 offered to trade verdicts for a cost the rule
does not have. The review was right.

**The three reaches, as whole candidates, at k = 1,024:**

| Family | `v3-full` | `v3-ps` | `v3-shallow` |
| --- | --- | --- | --- |
| CHAIN | 9.2 s | 1.33 s | 1.33 s |
| CHAINOBL (an obligation after every join) | 10.7 s | 2.83 s | 1.80 s |
| GUARDED | 9.6 s | 0.33 s | 0.33 s |
| CHAINSYM | 8.6 s | 1.34 s | **unproved from k = 6** |
| MINSEL | 9.9 s | 1.14 s | **unproved from k = 10** |
| CHAINSYMINV | 28.2 s | 26.7 s | 26.8 s |
| `rule_terms`, CHAIN | 1.9e7 | 6.8e4 | 6.1e4 |
| `rule_terms` per doubling, CHAIN | 4.0 | 2.0 | 2.0 |

- **W2 holds.** `v3-ps` (a join bound does not bring in what its other
  values reach while a join's bounds are computed; an obligation's own
  problem reaches everything) has the verdict of `v3-full` on every family
  and size. What the rule asks is linear in k on CHAIN and GUARDED, and
  quadratic on CHAINOBL, where each of k obligations reads its chain. The
  model's time on CHAIN is still quadratic because it copies the literal
  list before each call; that copying is counted in `work` and not in
  `rule_terms`.
- **W3 holds.** Shallow reach in the obligation's problem too loses
  CHAINSYM, ADDENDS (from k = 6) and MINSEL; without any split it loses
  them from k = 3 or 4. With invariants written, the three reaches cost the
  same on CHAINSYMINV, about 4.4 times per doubling. Without the rule that
  a weakened state bound is not carried on (`v3-ps-nopres`), CHAINSYMINV
  takes 61 s at k = 128 and meets the 900-second guard at 256.
- **Elimination itself fails on one shape.** ADDENDSINV, and ADDENDS under
  the candidates of versions 1 and 2, meet the elimination's capacity inside
  a projection from k = 6 to 24, on either theory step. `v3-ps-lp` (the
  simplex decision) proves both at every size to 128. The simplex of this
  model is not indexed and is slow: it meets the 20-second guard on OP5 and
  ORDERXY at k = 128.

**Long chains that do not pass through join bounds (W4: holds).**

| | k = 64 | 128 | 256 | 512 | 1,024 |
| --- | --- | --- | --- | --- | --- |
| LIMITCHAIN, `v3-ps`, time | 0.21 s | 1.8 s | 19.8 s | 291 s | not run |
| LIMITCHAIN, `v3-ps`, `rule_terms` | 1.4e5 | 5.3e5 | 2.1e6 | 8.3e6 | |
| LIMITCHAIN, `v3-shallow`, time | 0.17 s | 1.3 s | 14.2 s | 204 s | |
| SUMOBL, every candidate, time | 0.01 s | 0.06 s | 0.35 s | 2.3 s | 17 s |
| SUMOBL, `rule_terms` | 1.1e4 | 4.2e4 | 1.7e5 | 6.6e5 | 2.6e6 |

- What the rule asks grows with the square of k on both, under every reach:
  no reach stops a chain of definitions, as the review said.
- The model's theory step does much more than that: SUMOBL's time grows 7.4
  times per doubling, LIMITCHAIN's 8 to 15 times. Each obligation is solved
  from the start, and after the equalities are substituted a limit that grew
  j times is a row of j terms. An obligation after every statement of a
  function with no join at all is cubic in this model.
- Two more facts from LIMITCHAIN: bounds alone do not prove it (the limit
  the state bound names is no longer the limit asked about); the repeated
  root probe proves it, with 2k probes.

So the rule's own cost is quadratic at worst on every family measured, and
the model's theory step adds a factor on long linear forms that no
candidate here removes. Whether an incremental theory step removes it is
not measured.

## What each remedy does

`v3-full` holds every remedy. Each row is one change, measured alone.

| Remedy (candidate) | Gains | Costs or loses | Finding |
| --- | --- | --- | --- |
| Facts every arm states are recorded (`-af`, version 2) | ARMFACT, MULTIEXIT at every size | 2 projections per join | v1 #1 |
| The same through nested joins (`v3-ps` against `v3-ps-nonest`) | NESTFACT at every size; NESTDEEP at every depth to 8 (without it: none, and depth 1 only) | projections equal to the inner arms, 510 at depth 8 | 5 |
| Nearness by a stated fact's value (`-nf`) | ARMCORR to k = 4 | none measured | v1 #1 |
| A weakened state bound is not carried on (`-pres`) | CHAINSYMINV to k = 1,024 in 27 s; without it the guard at 256 | none under full or projection-shallow reach; under shallow reach everywhere CHAINSYM is lost | v1 #6, 1 |
| The interval of a join value (`-int`) | MATCHSHAPES with no probe | 2 candidates per join value | v1 #7 |
| The product counts live arms (`-live`) | none alone in version 2; `v3-full` proves BIGTAG2 to m = 12 where `win16-root` proves m = 3, and this remedy was not measured apart from the others there | none measured | v1 #7 |
| A transition bound only for an arm value that names the entry value (`v3-ps-strict`) | MATCHSHAPES(256): 512 projections and 29 ms, against 131,584 and 7.6 s; 2 bounds at the join against 514 | **loses MINSEL from k = 8**: `m = min(m, a)` no longer gives `m <= a` | 10 |
| A form that is both is a state bound | TIE with no split | none | v1 #9 |
| A join is near only by a join value (version 1's change) | an OP5 question costs 2 probes, not 2k | ARMCORR is proved only within P | v1 |
| The first split is free (`-free`, version 2) | one join more on FLAGCHAIN, PMONO | up to arms times P leaves, not P | 6 |
| The root probe repeated | ORDERXY, FLAGCHAIN, OP5CASC, LIMITCHAIN at every size (one pass: to k = 4) | see choice 5 | v1 #8, 4 |

**W6, W10, W12 hold.** `v3-full` has no free first split: no obligation of
any `v3-` candidate has more than 16 leaves.

## The choices that are the owner's

Every remedy in the table above was measured and changes verdicts, so each
is the owner's choice under the design tree's rules. They are gathered in
choice 7; the six choices of version 2 follow first, with what version 3
adds.

### Choice 1. Does the rule fix which joins are split?

Unchanged from version 2 in its options (A: no bound; B: a ceiling N; C: a
product P over near joins; D: no split at all). Version 3 adds:

- **D was run as written** (W11: holds). `v3-ps-d` proves everything
  `v3-ps` proves except: SWAPS, ARMCORR and SPLITBEFORE from k = 2 to 4,
  SAMECOND and SPLITAFTER at 2, BIGTAG2 from m = 6 to 12, and the PMONO
  question is decided only at k = 1. Version 2's sentence about D described
  `sat`, which lacked the remedies and proved neither ARMFACT nor MULTIEXIT.
- **C's verdict depends on the order of unrelated statements** (W7: holds).
  With k swaps and one `match` of eight arms that needs no split, `v3-ps`
  proves k <= 4 when the `match` stands before the swaps and k <= 2 when it
  stands after: the latest near join is split first, and the `match` takes
  8 of the 16. This is the fault for which version 2 rejected the one-pass
  probe.
- Against SEL on the sizes SEL finishes, `v3-ps` loses only what needs more
  than P: SWAPS and ARMCORR from 6, SAMECOND from 3, SPLITAFTER from 3,
  BIGTAG2 at 15.

Recommendation: D first, as in version 2. D has no order of search. The
owner then decides whether the families D loses are worth a product P whose
verdict depends on statement order, or whether a selection that does not
depend on position should be designed first; none was measured.

### Choice 2. What happens at a limit

- **N as a hard error, or out of reach is unproved**: as in version 2.
  Recommendation unchanged: unproved.
- **New: the ceiling of 4,096 atoms and a join's projection** (finding 8).
  The settled design keeps "more than 4,096 atoms rejects the site". Under
  full reach a projection's problem grows with the chain. Measured most
  values in one theory call: k + 2 on CHAIN, GUARDED, OP5 and CHAINSYM
  (1,026 at k = 1,024), 2k on MINSEL, SUMOBL and CHAINSYMINV (2,050), 3k on
  LIMITCHAIN (1,539 at k = 512). W8's prediction, that 4,096 is passed
  before k = 1,024, was wrong: by these lines it is passed near k = 4,100,
  2,050 and 1,365. Under `v3-shallow` the count stays at 4 to 16 on CHAIN,
  GUARDED, CHAINSYM and MINSEL; under `v3-ps` a projection's count stays small and an obligation's
  grows as under full reach.

  | Option | Consequence |
  | --- | --- |
  | A projection over the ceiling is a hard error | a function is rejected at a join for the length of what came before it |
  | A projection over the ceiling records no bound | a fixed rule, since the count is of the problem; a bound silently missing can make a later obligation unproved with no message at the join |
  | Projections reach shallowly (choice 4), so they do not meet the ceiling | the ceiling then applies to obligations only |

  Recommendation: the third, with the first for whatever still meets it.

### Choice 3. Is a local invariant a fact or a cut?

Version 3 adds the reading the review asks for: an invariant closes only
the joins its own goal reaches (`cut16-cone`; W9: holds).

| | Fact | Cut, everything before | Cut, what the goal reaches |
| --- | --- | --- | --- |
| SWAPSINV, MIXINV, REL1INV | proved | proved | proved |
| FLAGSCUT (an invariant about something else) | proved | unproved at every size | proved to m = 16, `ceiling` from 24 |
| MIX, REL1, FLAGS, LIMITCHAIN with no invariant | proved | `ceiling` from 16 or 24 | the same |
| SWAPS with no invariant | proved to k = 4 | proved to 16 with 65,536 leaves | the same |

Version 2's measured argument against the cut reading (FLAGSCUT) holds only
against its strictest form. Against the cone form what remains is the
ceiling: a function with more than N joins and no invariant is rejected
although no split is needed beyond the probe, and within the ceiling the
work is 2^N at worst.

Recommendation: the fact reading, for that reason. The owner's choice.

### Choice 4. How far a join bound reaches

| Option | Verdicts | Cost at k = 1,024 |
| --- | --- | --- |
| Full reach | all | CHAIN 9.2 s; what the rule asks is quadratic |
| Shallow while bounds are computed, full in an obligation (`v3-ps`) | the same as full reach on every family and size | CHAIN 1.33 s; what the rule asks is linear on CHAIN and GUARDED |
| Shallow everywhere (`v3-shallow`) | loses CHAINSYM, ADDENDS, MINSEL; one invariant per step repairs them | CHAIN 1.33 s; CHAINSYMINV 26.8 s, the same as the others |

Recommendation: the second. Version 2 recommended the third on a cost that
was the model's; that recommendation is withdrawn.

### Choice 5. Where the forced cases run, and for which questions

Version 2 said the repeated root probe costs on OP5 what one pass costs.
That was wrong (finding 4; W5 holds). On OP5 the forced cases at the root
run down the chain:

| OP5, whole program | k = 64 | 128 | 256 | 512 | 1,024 |
| --- | --- | --- | --- | --- | --- |
| `v3-ps`, probes | 4,158 | 16,510 | 65,790 | 262,654 | not run |
| `v3-ps`, time | 0.53 s | 3.9 s | 29.8 s | 244 s | |
| `v3-ps-q1` (a question gets one pass), probes | 252 | 508 | 1,020 | 2,044 | 4,092 |
| `v3-ps-q1`, time | 0.09 s | 0.35 s | 1.3 s | 5.4 s | 21.8 s |
| `v3-ps-q0` (a question gets the bounds alone), time | 0.02 s | 0.07 s | 0.25 s | 0.97 s | 3.9 s |

`v3-ps-q1` and `v3-ps-q0` have the verdict of `v3-ps` on every obligation
that is not a question. `v3-ps-q0` no longer decides the PMONO question at
any k.

| Option for obligations | Proves | Costs |
| --- | --- | --- |
| One pass at the root | flags at any distance; depends on statement order; loses ORDERXY, FLAGCHAIN, OP5CASC, LIMITCHAIN from k = 6 | 2 probes per near join |
| Repeated at the root | those four at every size | probes grow with the square where a cascade is long: ORDERXY(128) 15.9 s |
| Everywhere | also SAMECOND | the square on every unproved question |

| Option for questions of the redundancy rule | Consequence |
| --- | --- |
| As an obligation | the square of the chain at every `if`: 244 s for 512 of them |
| One probe pass | linear probes; 21.8 s for 1,024 |
| The bounds alone | 3.9 s for 1,024; fewer tests are found redundant (choice 6) |

Recommendation: repeated at the root for obligations; the bounds alone for
questions.

### Choice 6. What the redundancy rule asks, and what may later grow

Version 2 said only P could not be raised. The review is right that it is
wider (finding 12): anything the redundancy rule asks with, once it grows
stronger, rejects accepted programs. A later remedy such as recorded arm
facts decides `if i < len` after an ARMFACT shape, and that `if` becomes an
error.

| Option | Consequence |
| --- | --- |
| The rule asks the same derivation | no part of the derivation can be strengthened once programs exist |
| The specification fixes what the rule asks with, apart from the derivation (for instance the bounds alone, as `v3-ps-q0`) | the derivation can grow; the fixed part cannot |
| "Redundant" becomes a warning with the mechanical removal command | nothing accepted is ever rejected by a stronger derivation; it changes a rule the owner kept |

Recommendation: the second, with the bounds alone, which is also the
cheapest in choice 5. The owner's choice.

### Choice 7. The remedies

Each row of "What each remedy does" is a choice. Recommendations: take
recorded arm facts with nested joins, nearness by a stated fact, the rule
on weakened state bounds, the interval, live arms, and "a form that is
both is a state bound"; do not take the free first split.

One is left open with its cost on both sides, **the wide `match`**
(finding 10):

| Option | MATCHSHAPES(256) | Loses |
| --- | --- | --- |
| A transition bound for every arm's value | 131,584 projections, 7.6 s, 514 bounds carried by every later problem that names the value | nothing |
| Only for an arm value that names the place's entry value | 512 projections, 29 ms, 2 bounds | MINSEL from k = 8 |

No recommendation: `m = min(m, a)` and a dispatch of 256 arms are both
ordinary code, and no rule measured here serves both. A rule that offers
the other arms' values only up to a fixed number of arms was not measured.

## Answers to the second review

1. **Holds.** Shallow reach everywhere is not "at most quadratic" once the
   invariants it asks for are written without `-pres`, and with `-pres` it
   loses CHAINSYM (measured: unproved from k = 6, from k = 3 with no split).
   Choice 4 is rewritten and its recommendation withdrawn.
2. **Holds.** The indexed theory step gives 9.2 s where the old one gives
   653 s, with the same verdicts. "Projection shallow, obligation full" was
   run and has the verdicts of full reach.
3. **Holds.** LIMITCHAIN and SUMOBL are in; what the rule asks is quadratic
   under every reach, and the model's theory step is worse than cubic on
   LIMITCHAIN. This is the main cost left open.
4. **Holds.** The sentence is withdrawn; the square is measured; asking
   questions with one pass or with the bounds alone removes it.
5. **Holds.** Nested arm facts were lost by every candidate of version 2,
   SEL included. Fixed for SEL in the instrument and for the rule by
   `nested`; NESTFACT and NESTDEEP are proved, and the exhaustive check
   covers them.
6. **Holds.** The bound with a free first split is arms times P. Version
   3's candidates do not have it and stay within 16 leaves.
7. **Holds.** Measured as SPLITAFTER against SPLITBEFORE; written into
   choice 1. No position-independent selection was designed.
8. **Holds.** A guard inside a projection now voids the row. This turns
   version 2's "proved, with a capacity note" rows of ADDENDSINV into
   `abort` for every elimination candidate from k = 6 or 8; version 2's
   statement that no remedy loses a verdict counted those rows as finished
   and should not have. Atoms are counted; the rule for a projection at the
   ceiling is choice 2.
9. **Holds.** `cut16-cone` is measured; choice 3 is rewritten.
10. **Holds.** The remedy is measured and costs MINSEL; it is in choice 7
    without a recommendation. Two wide matches told apart together
    (BIGTAG2) are proved while the live arms multiply to at most 16.
11. **Holds.** `v3-full-d`, `v3-ps-d` and `v3-shallow-d` ran on every
    family; choice 1's D is rewritten from them.
12. **Holds.** Choice 6 is rewritten.
13. **Holds.** Every measured remedy is listed with its gain and loss and
    is choice 7.
14. **All three hold.** Invariants are tightened (2,662 verdicts, none
    proved); a stopped question is reported as stopped; a row with a
    capacity note is `abort` and is not counted as agreeing.

## What this study leans to, as one candidate

`v3-ps-q0` ran on every family and at the largest sizes: relational bounds
with the remedies of choice 7; bounds computed with shallow reach,
obligations with full reach; the root probe repeated for obligations; P =
16; questions of the redundancy rule asked with the bounds alone. Its
verdicts equal `v3-ps` except that the PMONO question is not decided. Its
times at k = 1,024: CHAIN 1.33 s, GUARDED 0.33 s, CHAINSYM 1.35 s, MINSEL
1.14 s, CHAINOBL 2.82 s, OP5 3.87 s, SUMOBL 17 s, CHAINSYMINV 26.3 s; at
k = 512, LIMITCHAIN 287 s. It does not finish ADDENDSINV on the elimination
theory step (`abort` from k = 8); `v3-ps-lp` does.

It is a candidate, not a decision: choices 1 to 7 are the owner's.

## Still open

1. **The theory step on long linear forms.** LIMITCHAIN and SUMOBL are
   cubic or worse in the model although the rule asks for a quadratic
   amount of reading. Each obligation is solved from the start. An
   incremental theory step was not built.
2. **Elimination against simplex.** Elimination meets its capacity on
   ADDENDS and ADDENDSINV; the simplex decision proves them but this
   model's simplex is slow. An indexed simplex was not built, so no single
   theory step here both finishes every family and is fast.
3. **A selection of joins to split that does not depend on statement
   order** (choice 1).
4. **The wide `match`** (choice 7): no rule measured serves both shapes.
5. The model has linear literals and joins only: no reads and writes of
   arrays, quantified facts, disequalities or nonlinear rows.
6. Absolute compile time needs the rule in the compiler.
7. 39 tightened programs are false by argument only; the search for a
   failing run does not reach them.
8. How many later tests a repair invariant turns redundant was not
   measured.
9. No real program was measured, on purpose.
