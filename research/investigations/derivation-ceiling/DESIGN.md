# One derivation for every obligation: its ceiling, its joins and its build time

Status: design and criteria, written before any measurement. Results go to
`RESULTS.md` in later commits; nothing below changes once a number
is measured, except to correct a fact about the instrument, which RESULTS.md
then states.

Compiler baseline: main at `0b8b4ad348b9e75aafdf3f3dbaa034c7aba78635`
(specification as of that commit). The experiment compiler is a branch of that
revision that is never merged; RESULTS.md names its commit and the retention
tag that keeps it.

## Question

The redesign of the specification (item #352, its conclusion's section 4 and
the experiment E1 of its section 9) proposes that one derivation proves every
obligation: today's [RANGE-3] four steps (instances, definitions, theory,
decision), renamed, with a structural ceiling N per obligation whose excess is
a hard error. The difference-bound closure (ENT-4), the AUTO families and the
MSR-4 steps become shortcuts that never change a verdict. The owner chose this
direction on 2026-11-26 and asked for this experiment to settle four things
the proposal leaves open:

1. Which of today's provable obligations the derivation, as today's text
   states it, cannot prove, and whether each falls into the eight items the
   conclusion lists (its section 4 table).
2. Which structural quantity N counts and how large it must be.
3. Whether a join keeps only "the value of the arm taken" (a selector the
   derivation splits on) or also records the weakest bound every arm proves.
4. Whether a whole build stays within ten times a C build of similar scale
   and function.

A fifth part runs probe programs, such as `x != 2` proving `2 * x != 4`.

## What is measured

### The shadow derivation

The experiment compiler walks every function with the range walk, whether or
not it takes part today [RANGE-2], and poses every obligation record at its
site to the RANGE-3 derivation. Ordinary entailment still decides every
verdict. The shadow's answers, and all its counters, go only to a
tab-separated file. A conformance run of the experiment compiler that
reaches every declared verdict therefore shows that the shadow changed none.

From one run, two verdicts per program follow:

| Verdict | How it is read from the file |
| --- | --- |
| Handoff | Every record is proved by ordinary entailment, or else by the shadow. |
| Derivation alone | Every record is proved by the shadow. |

Obligations posed:

- Every record the walk already knows how to pose, in every conjunct, whether
  ordinary entailment discharged it or not:
  - [OP-4] bounds;
  - [OP-2] integer domains;
  - [OP-6] conversion domains;
  - [INV-1] loop invariants;
  - local invariants and certificates [INV-1, PRF-1];
  - call requirements [FN-8, OP-14].
- [OP-5] conditions, asked both ways: the condition true, and the condition
  false.
- Every range-clause obligation the walk owes today [RANGE-3].

Every other record (postconditions [FN-9], range formations [REF-4], the
separation families [EFF-5, OP-11, SHARE-2, REF-2] and [TYPE-11] requirements)
is either posed, if the experiment compiler learns to pose it, or written as
`not-posed:<kind>`. It is never dropped. Parallel permission's separation
queries [PAR-1, PAR-2] are not obligation records; their count is reported
and they are not posed.

### Three configurations, judged in the same compilation

| Configuration | Operand intervals of an operation row | Result range of a nonlinear row | Join |
| --- | --- | --- | --- |
| baseline | as ordinary S7 recorded them (spec RANGE-2, the operation sentence) | none (today's text) | selector only |
| remedy | the value interval defined by the derivation (below) | the row's result range, computed by the same fixed operation table from those operand intervals, added as literals on the result | selector only |
| remedy+join | as remedy | as remedy | selector, plus the weakest common bound (below) as an unconditional literal |

- **Value interval by derivation.** It is the tightest constant lower and
  upper bound that the theory step gives for a value over the walk's
  unconditional literals at that point, with no split: equalities solved over
  the integers, inequalities tightened, and Fourier-Motzkin projection onto
  the value. This is the definition the conclusion's section 4 proposes for
  the table's first row. The remedy configuration is the remedy under test for
  rows 1 and 2.
- **Weakest common bound at a join.** For a joined scalar value v and each w
  in {0} and the atoms paired with v's arm value in some arm's difference
  literal, take each non-contradictory arm's bound on `v_arm - w` by the same
  projection, and keep the weakest. This is the conclusion's first way out
  (ENT-5's weakest-bound join, moved onto values). It carries difference
  bounds only.

### Counters, per posed obligation

Each obligation gets one row per configuration:

- record identity: function, rule, kind, conjunct;
- ordinary answer (proved, refuted, unproved) and the ordinary route (below);
- shadow answer (proved, unproved, ceiling, unsupported, unreached,
  not-posed);
- atoms after step 2;
- instances;
- the three candidate N quantities;
- the largest case count of an open item;
- the product of the open items' case counts;
- branches of the full case analysis (no backjumping, which is the
  specification's analysis), and branches with the compiler's backjumping;
- maximum depth;
- time, split into forming the problem (steps 1 and 2) and judging it
  (steps 3 and 4).

An obligation posed on several incoming edges sums its times and branches and
keeps the maxima of the other counts.

The **ordinary route** is the step of ordinary entailment that proved the
record. Its values are:

- closure only;
- a fact resting on an S7 result range (`s7-result-bound`);
- contradiction;
- an AUTO family;
- MSR-4 step 6;
- MSR-4 step 7 (quotient);
- the power atom;
- the interval product;
- the origin view;
- a certificate fold;
- float decoding.

### Candidates for N

All three are counted on the problem's root, after steps 1 and 2 have added
every instance and definition and after the theory step has saturated the
root (forced choices decided, entailed rules fired). A root the theory
refutes counts **0**, in both join variants, and the ceiling is not consulted
for it.

| Candidate | What it counts |
| --- | --- |
| **N1** | The open items of the root: undecided definitions and choices, exclusions whose expression the solutions do not fix, disequalities whose difference the solutions do not fix, and pairs of reads of one version and projection that are neither identified nor held apart. |
| **N2** | The same item kinds counted syntactically, before the theory step: every choice, every exclusion and inclusion, every disequality, and every pair of reads of one version and projection with differently written index tuples. |
| **N3** | The open items of N1 that are connected to the goal's atoms. An item and an atom are connected when they share an atom; a read is connected to the atoms of its index tuple. |

Each depends only on the root's literal set, so no split order changes it.
Because the decision step only adds literals, an item fixed at the root is
never open below it, so N1 bounds every branch's depth. The full case
analysis is at most the product of the open items' case counts, so the
largest case count is recorded too: an exclusion with many members splits
into many gaps even when N is small.

## Workloads

- Every case of `tests/conformance/manifest.jsonl`, through the conformance
  adapter's ordinary compiler path.
- The programs of `tests/programs`.
- `apps/firn`, checked as `--graph apps/firn/modules.wfg --entry firn`.

Firn-wf, Halo-wf and Snowghost-wf are not measured in this record. The owner
chose (question `downstream-numbers`) to merge this record first and to
measure them in a later item that depends on #453, #454 and #455.

## Criteria, fixed before measuring

Throughout, "today-proved" means a record that ordinary entailment discharges,
or a range-clause obligation the walk proves at baseline.

### (1) The eight items are the whole difference

Every record that ordinary entailment proves and the baseline shadow does not
prove is classified, in this order:

1. **Item 1 or 2, recovered by the remedy:** proved under remedy.
2. **Item 7, recovered by the join:** unproved under remedy, proved under
   remedy+join.
3. **Item 1 or 2, not recovered by the remedy:** route `s7-result-bound` or
   interval product.
4. **Items 3, 4, 5, 6 and 8 by route:**

   | Route | Item |
   | --- | --- |
   | power atom | 3 |
   | MSR-4 step 7 | 4 |
   | origin view, or an identity of written expressions | 5 |
   | float decoding | 6 |
   | certificate fold | 8 |

5. **Walk implementation gap:** the shadow answered unsupported, or the walk
   forgot past its depth ceilings.
6. **Not posed,** by kind.
7. **Ninth kind:** anything else, each examined individually.

Records proved at baseline and unproved under remedy are listed apart as
"lost under the remedy".

**Rejects the claim that the eight items are the whole difference:** one or
more records in the ninth kind that individual examination does not move into
classes 1 to 6.

### (2) A structural count can serve as N

For each candidate q, its covering value is the largest q over today-proved
records. RESULTS.md gives:

- q's distribution;
- the records above a set of smaller values;
- at the covering value, the worst record's derivation time and full-analysis
  branch count.

**Rejects a candidate:** at its covering value, a today-proved record's full
case analysis reaches the instrument's cap of 2^20 branches, or its
derivation takes more than 1 s on the runner. Such a count does not keep the
work of every accepted obligation within a practical constant.

If every candidate is rejected, the result is "no usable quantity" for the
three candidates tested here, and it goes to the owner with the data.

### (3) The selector join alone suffices

Measured at the recommended candidate and value, for remedy and for
remedy+join:

- the records with q > 0;
- the records above the value;
- the total derivation time.

**Rejects "selector alone suffices":** the selector variant has today-proved
records above the value that the weakest-bound variant does not, or its total
derivation time over all workloads is more than twice the weakest-bound
variant's.

### (4) A whole build stays within ten times a C build

All on the 14900K runner, on one fixed core (`taskset -c 2`, as
`firn-check-time.yml` uses), with the compiler's processor budget set to one.

- **Whitefoot side:** a cold build of `apps/firn`: check, lowering, LLVM and
  link, with no `--cache`, to an executable.
  - It is timed with the experiment compiler in its timing mode: no counter
    file, one solve per obligation, no full-analysis re-solve, derivation
    intervals, and each join variant in turn.
  - The compiler's own timers give the ordinary-entailment time of the same
    run.
- **C side:** Redis 7.0.15 from the archive and SHA-256 that
  `research/experiments/redis-compat/run.sh` pins, built with
  `make distclean && make -j1` at its default `-O2`, dependencies included,
  on the same core.
- **Rounds:** one warmup and five rounds each. Medians and ranges are
  reported.
- **Verdict quantity:**
  `(Firn wall time - Firn ordinary-entailment time) / Firn KLOC`, divided by
  `Redis wall time / Redis KLOC`. It is judged per join variant.
- **KLOC:** non-blank lines of every source each build compiles:
  - Firn: the `.wf` sources of `apps/firn`, the `std::` module sources the
    graph compiles, and the runtime C units the build compiles.
  - Redis: the `.c` files on `make`'s compile lines, and the `.h` files under
    `src/` and `deps/`.
- **Reference only:**
  - main's compiler;
  - the experiment compiler's whole time;
  - the ratio without the per-KLOC division;
  - a multi-core pair (`make -j<n>` against the compiler's default budget).
- **Profile.** Each time is split into algorithmic work (branches,
  eliminations) and implementation overhead, with `perf` where the runner
  allows it and the compiler's phase timers otherwise. Where overhead
  dominates, RESULTS.md also estimates the time with it removed, marked as an
  estimate.

**Rejects "within ten times" for a join variant:** its verdict quantity is
above 10. The owner is then asked, with the data. If the 14900K cannot fetch
the Redis archive, both sides run on one hosted `ubuntu-24.04` runner
instead, and RESULTS.md says so.

### (5) Probes

Each probe in `probes/` is checked under every configuration, and RESULTS.md
gives the ordinary verdict and each shadow verdict.

- **Rejects the claim that the theory step needs no extra rule for greatest
  common divisors:** the probe `x != 2` proving `2 * x != 4` is unproved at
  baseline.
- **Rejects the conclusion's claim for one eight-item row:** that row's probe
  is proved at baseline.

## Who chooses

N (which quantity, and its value) and the join approach are the owner's
choices. RESULTS.md gives the data and a recommendation, and this work asks
the owner before it merges. No design-tree decision is written here, since no
code on main implements N or the join rule. The work that implements them
writes it from the owner's answers.

## Limitations

- The derivation measured runs on today's range walk: its storage versions and
  joins, not the single "value form" the redesign proposes. Its implementation
  gaps are counted apart (class 5) and are not language results.
- The weakest-bound join carries difference bounds only, as ENT-5's join does.
  It is an approximation of what a specified rule would record.
- The value interval projects the unconditional literals only. Instances of
  range facts and the definitions of joined values are not projected, since
  using them would need a split.
- The derivation-alone build time is the experiment compiler's time minus its
  measured ordinary-entailment time on one core, an estimate. The shadow keeps
  ordinary entailment, which the walk still reads at baseline.
- Per-obligation times come from CI runners that other jobs share. They rank
  obligations; they are not a timing benchmark. Criterion (4) uses the
  isolated timing run instead.
- Conformance cases and this repository's programs were written to exercise
  the compiler. Their distribution of N is not evidence of what real programs
  need (decision `language-choices-are-made-on-merits`). `apps/firn` is the one
  real program here; the downstream programs come later.
