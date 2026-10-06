# Incremental L0 closure

## Question

After the [flow-analysis follow-up](CHECKING-COST.md#flow-analysis-closure-follow-up),
`tests/programs/wfgrep.wf` and `tests/programs/fixed_run_library.wf` each still
take about 24 s to check. A whole-run sample of wfgrep puts about 66% of
semantic samples in the complete closure and 11% in its proof-free
contradiction twin. The remainder is mostly rebuilding relation maps around
those closures.

Every kill, predecessor join and query that follows a relation change
recomputes the complete cubic fixed point. It does so even when nearly the
whole state is already a closed difference-bound matrix from the previous
materialization or join. This investigation asks how much of that work
disappears when the fixed point starts from the part of the state that is
already closed.

## Rule boundary

The [ENT-4 rule](../../../spec/kernel-spec.md) fixes the least closure and
allows incremental computation, provided every derivability and disposition
answer equals the least-closure answer. For L0 bounds it does not fix which of
several equal-bound derivations is retained. The minimum-depth sentence
governs the opaque-goal reconstruction.

So this work may change:

- which equal-bound L0 derivation a cell retains;
- the ledger node inventory and its identities;
- diagnostic parent choice.

It must not change:

- any L0 bound value, disequality or contradiction;
- any opaque-goal answer or obligation disposition;
- acceptance, diagnostics' rules and locations, or emitted LLVM.

Every retained derivation must remain valid. Production ledger validation
checks parent ordering and ancestry-table integrity, not every semantic
inference. The compiler tests' `validate_derivations` additionally reconstructs
the relations and checks rule-specific parents for the summaries they inspect;
it is not a universal independent checker of all accepted programs.

The live [closure-row-dominance](../../../design/compiler/closure-row-dominance.md)
decision now preserves selected derivations for the unseeded complete fixed
point. Its owner-approved revision lets the seeded fixed point follow
[incremental-closure](../../../design/compiler/incremental-closure.md)'s
retained-witness policy: valid alternative L0 derivations are permitted,
with independently live join reuse, preserved ordinary fallbacks and neutral
contradictory predecessors. The language judgments above remain unchanged.

## Mechanism

This is the mechanism as first proposed; [Implementation](#implementation)
records what replaced parts of it, including weakened cells and edge insertion.

A fact state records how much of its bound matrix is known closed:

- **unknown**: no claim;
- **closed** over a term count;
- **closed core**: closed over a base term count, except for listed fresh
  cells and fresh terms.

Operations update the record:

- A materialization or join produces a closed state.
- Adding a bound makes its cell fresh.
- Adding a disequality makes both orientations fresh.
- An endpoint kill removes the killed terms' cells and marks those terms
  fresh, because their implicit facts hold again immediately.
- Terms registered after the base count are fresh.
- Removing an S12 proof candidate can weaken a selected bound without a
  matching projection, so it resets the record to unknown.
- Removing signed goals leaves the L0 matrix unchanged.

Removing whole rows and columns from a closed matrix leaves the survivors
closed among themselves. The removed terms' implicit edges composed with
closed survivor paths cannot improve a survivor-to-survivor bound. Each such
path enters and leaves a removed term only through edges the closed matrix
already accounted for.

The closure engine is unchanged except for its seed:

- Stale cells start not fresh, so round one visits only triples touching a
  fresh cell.
- In seeded mode a candidate replaces a cell only with a strictly smaller
  bound. The disequality rule adds only absent pairs. A strictly smaller
  bound can reach a stale cell only through a fresh one, which the semi-naive
  rounds follow.
- Equal-bound, shallower candidates are no longer accepted. Accepting them
  would rewrite stale proofs across the whole matrix and restore cubic work.

The unseeded closure keeps its current acceptance, so states with an unknown
record retain today's derivations. The proof-free contradiction probe uses
the same seed.

## Alternatives

- A separate incremental difference-bound implementation with explicit
  per-edge O(n²) updates duplicates the closure rules. It also needs its own
  kill, strengthening and contradiction logic. Reusing the engine with a seed
  keeps one rule implementation.
- A cache keyed by complete state content only helps exact repeats. After
  generic validation scope, repeats are about 1–5% of closure time.
- Persisting a dense matrix inside the fact state, replacing the relation
  maps, would also remove map rebuilds. Measure first whether the seed alone
  leaves map work as the dominant cost.
- Changing ENT-5 so a kill does not materialize would change language rules
  and is outside this investigation.

## Selection criteria

These are recorded before implementation timing.

**Correctness.**
- A test-only verification switch recomputes every seeded closure unseeded on
  a cloned ledger. It must then find identical bound values, disequality
  sets, contradiction flags and goal-contradiction results. The switch runs
  over the real-program bundles `utf8parse.wf`, the raw DEFLATE chain,
  `wfgrep.wf` and `fixed_run_library.wf`.
- The ordinary compiler, program and conformance suites pass. Any changed
  test expectation must be a ledger-shape or derivation-choice expectation,
  listed with its reason, never a verdict, rule, location or runtime result.
- Base and candidate emit byte-identical LLVM for the paired sources.

**Performance.** Use five warmed alternating pairs against the PR #68 head
compiler, on the paired-source set of the flow follow-up. Select only if
all of these hold:
- wfgrep and fixed-run each improve by at least 3x in median time;
- no protected fixture or real program regresses by both more than 10% and
  more than 1 ms.

Sub-second checking is the owner's target and is reported whether or not it
is reached.

If the seed misses 3x, attribute the remainder before choosing a persistent
dense representation.

## Implementation

The implemented closure record is richer than the proposed seed.

**The closure record.** Besides fresh cells and fresh terms, it keeps weakened cells: cells whose selected candidate was removed while their terms survive. It is kept for both the full and the ordinary proof layers.

**Closing a core.** A closure of a core does not use the seeded fixed point.
- Each fresh edge enters with one column pass, then one row pass over the rows tight through the edge and the columns the edge can improve.
- The implicit bounds of fresh terms enter as such edges.
- Up to one weakened cell per term is rederived in place, within one repair pass per weakened cell. A negative cycle behind those cells can otherwise lower them forever.
- A larger weakened set keeps the seeded fixed point over the weakened endpoints.
- The proof-free contradiction probe uses the same insertion.

**Representation.** Closed states are dense matrices, and fact-state bounds live in a dense, reference-counted store. A single proof candidate is held inline. The closed view of an unchanged state is remembered.

This reverses the first alternative refused above. The seeded fixed point
alone reached 9.65 s on wfgrep and 7.62 s on fixed-run: 2.6x and 3.3x
against PR #68, which misses the 3x criterion for wfgrep. So, as pre-registered,
the remainder was attributed first. Samples of the seeded build put the
remaining closure time in fixed-point rounds after small changes: up to 14
rounds, each visiting every middle term.

Per-edge insertion replaces those rounds. It carries its own settling of the
strict-bound disequality and zero-bound strengthening rules, and its own
contradiction scan. It does not carry kill rules: term kills still only
remove cells and mark terms fresh, and a candidate removal only marks the
cells it weakens for the rederivation above. Because transitivity,
strict-bound/disequality strengthening and contradiction detection now also
have incremental implementations, the verification switch compares every insertion
with the complete closure, and the proof-free probe is checked the same way.

**Attribution of the representation changes.** After edge insertion
(wfgrep 6.84 s), a native sample of about 1,100 semantic samples put:

- 136 in rebuilding the dense matrix from hashed relation maps at each
  closure;
- about 220 in hash inserts while building materialized and joined maps;
- about 250 in sorting term-pair keys and in allocating and freeing
  per-cell candidate lists.

That measured result is what the pre-registered "persistent dense
representation" step asked for before choosing it. The dense closed matrix,
inline candidates, reference-counted maps, the dense bound store and the
in-place candidate updates each removed their attributed samples; single
exploratory runs moved wfgrep through 4.87, 4.34, 3.42, 3.29 and 1.68 s.

The remembered view came from samples showing the same unchanged state closed
by contradiction promotion and then by materialization or a join.

### Earlier reuse alternatives

The [flow comparison](CHECKING-COST.md#flow-selection-2026-09-16) rejected
sharing only the contradiction-promotion closure with its following
materialization: the measured gain was 1.04x and did not justify a special
path for that check. Remembering every unchanged state's closed view later
provided that reuse without the special path. Sharing each predecessor's
closure with its join had also conflicted with retaining the eager traversal's
ledger order. The selected witness policy instead permits different valid
equal-bound witnesses under the retained-proof conditions below.

The closure record tracks term kills, removed candidates and joins, replacing
the reason earlier query and row-dominance designs refused cross-flow reuse.
Query preparation still preserves candidate and diagnostic-parent traversal
within a prepared view. Unseeded row pruning preserves the unpruned traversal's
selected witnesses; seeded closure need only preserve derivability under the
selected witness policy. These are distinct guarantees.

### Retained derivations

**Consumers of retained derivations.** Three consumers read a retained
proof's ancestry rather than its value.

- *Postcondition-call ancestry.* It selects which candidates an ordinary view
  or holder kill removes. A selection that does not depend on a call is still
  derivable from surviving ordinary facts. A contradictory predecessor is
  absorbing and neutral at a join under ENT-5: its proof remains a parent,
  but its former call-dependent value support must not make another path's
  ordinary conclusion removable. Candidate dependency therefore stops at a
  contradiction parent; it is not a literal traversal of every retained
  provenance edge.
- *Delivery.* `depends_on_explicit_relation` delivers a relation only when its
  proof uses an explicit fact. A cell whose closed value is implied by
  implicit bounds alone holds the same value at a same-typed receiver through
  that receiver's own implicit bounds, so which of two equal-bound proofs is
  retained does not change a delivered bound.
- *Retained derivations.* The checked program retains them, including the
  PAR permission derivations of [DIAG-2]. Their shape may change, but an
  independently live conclusion needs its source, snapshot or join boundary.
  Sharing a temporary closure proof across all predecessors does not give it
  that boundary. Focused compiler tests check these retained records.

These are arguments, not proofs. The byte-identical LLVM for the measured
sources and the unchanged verdicts of the compiler, program and conformance
suites are the observed evidence.

**Memory.** Peak resident memory for one standalone compile, measured with
`/usr/bin/time -l`, fell from 1.32 GB (1,317,896,192 bytes, main) to 281 MB on
wfgrep, and from 1.71 GB (1,714,683,904 bytes) to 375 MB on fixed-run. The bound store keeps a stride 1.5 times the term count,
and live states can pin a remembered closed matrix. Both are included in
these peaks.

**Joins, fallback views and kills.**
- A join reuses an already independently live or implicit proof every predecessor selected; temporary consequences still receive a join node.
- Ordinary fallback candidates are merged where a selection depends on a postcondition call. Delivery preserves those candidates through both substitution and the separate delivery join, and shared Give parents receive exactly one retained root.
- Kill predicates are evaluated once per term.

An independent adversarial review found the unbounded repair loop, which is now bounded and has a regression test that hangs without the bound. Two independent reviews also reported value-level models of edge insertion agreeing with a complete closure on randomized small states. Those models are review aids outside the repository, not retained evidence.

The verification switch now runs on the test thread only and asserts that it compared closures. Two committed tests use it:
- One compiles utf8parse, in about 2 s under the gate profile.
- One runs 400 generated flows of 24 steps over places, a constant, a fixed-length measure and a signed opaque goal. The steps mix source and postcondition bounds, disequalities, materialized kills, candidate kills, ordinary views, joins, goal establishment/removal and new terms. Each flow also advances a separate eager reference state that takes unseeded closures, always records join boundaries, and retains every ordinary candidate. Both full and ordinary layers are compared after transitions. This adds transition preservation to the same-state closure check; it still shares term, candidate-storage and kill primitives and is not an independent language implementation.

The separate-state comparison exposed a dependency leak through a neutral
contradictory predecessor. With two different valid contradiction witnesses,
one joined ordinary view retained `t <= 0` while the other retained only its
type bound. A focused case now checks both ordinary and S12-derived
contradictions, their retained parents, and the surviving ordinary bound.

The expanded generator counts and requires every named route: remembered and
already-closed views, unseeded and seeded fixed points, insertion with and
without proofs, weakened-cell repair, and its size/pass-limit fallbacks.
These counters exist only in test builds. Separate mutants of the repair,
column pass, row pass and zero-bound settle rule failed the original generator;
those mutation runs remain historical evidence.

The raw DEFLATE chain (about 9 s), fixed_run_library (about 27 s) and wfgrep (about 30 s) were verified by temporary inclusion at every change, not in the committed tests.

Six existing compiler tests pin derivation shape; no verdict, rule or location changes:
- One permits either constant-four or zero as the transitive endpoint through the killed middle. Both must retain the middle's actual transitive derivation.
- Five count join grounds. The follow-up restores one-edge boundaries for strict-derived disequalities; an explicit source fact already has its own boundary and needs no wrapper. The bound/disequality join regression checks that a consequence survives a middle-only kill with that boundary, but dies with its own endpoint.

## Selection

### Retained-proof follow-up criteria

Before timing the follow-up to `3f205ff6`, preserve the following boundaries:
join reuse must retain an independently live conclusion, and delivery must
preserve ordinary fallback candidates through both substitution and the
delivery join. Compare separate optimized and eager-reference flow states
after transitions, including after removing call-dependent candidates;
reclosing the same optimized state alone cannot detect a dropped candidate.
Keep these checks in compiler tests because they inspect internal proof
state, without a native build or a research dependency in the gate.

Compare the saved `3f205ff6` compiler with the follow-up on the same 15-source
inventory as the recorded pairs below, using five alternating warmed pairs.
The runner must compare both LLVM outputs and record their SHA-256 digests
outside the measured interval. Report any median increase exceeding both
10% and 1 ms; correctness repairs are not abandoned to satisfy a speed target.
The historical 3x comparison with PR #68 remains a separate measurement.

The [raw pairs](../../experiments/proof-use-cost/incremental-pairs-2026-09-17.tsv) compare the PR #68 head `902ae791` (gate binary SHA-256 `476a03de5d2f7bf356c75ec2043522b476f3fad683f67e4ad4244cbf511f85ba`) with `9f5e2616` (`7f32f6128ddda36c4522d30ebf1752ce2bdf4838f5842ebc18739e2a9d0e71c2`). They were run on the flow-baseline M1 Pro host with no other compiler or test job running: five alternating warmed pairs.

| Source | PR #68 median | Candidate median | Speedup |
|---|---:|---:|---:|
| fixed-run | 24.966 s | 1.162 s | **21.49x** |
| wfgrep | 24.959 s | 0.878 s | **28.41x** |
| prefix | 121.7 ms | 25.3 ms | 4.81x |
| histogram | 123.4 ms | 28.2 ms | 4.37x |
| radix scatter | 639.8 ms | 72.7 ms | 8.80x |
| growing-256 | 3.234 s | 3.065 s | 1.05x |
| control-256 | 482.8 ms | 291.3 ms | 1.66x |

Fixed-context cells and 16-size cells change by at most 2.3% (growing-16, +0.37 ms; fixed-256, +0.54 ms), far inside the 10% and 1 ms clause. The retained TSV contains 15 sources and 150 timing rows. It records no LLVM digests, so it does not substantiate the earlier claim of byte-identical LLVM for 22 measured sources. The follow-up runner checks byte equality and records per-arm digests for its explicit inventory.

The candidate meets both 3x criteria and regresses no protected source. wfgrep checks in under one second. Fixed-run, at 1.16 s, does not reach the owner's sub-second target.

Against main `ab93c8e9`, the two programs went from 80.5 s and 40.5 s to 1.16 s and 0.88 s. These are checking-cost results for these sources, not runtime speedups or a universal bound.

In a full `make check` of `3edaf0ba`, the `WHITEFOOT_TEST_TIMINGS` phase log sums 9.0 s of WF compilation across 195 test compiles. The #66 gate log summed 259.6 s. These are CPU-summed phase times, not suite wall time.

**What remains** is distributed rather than cubic:
- the ordinary-fallback view that materialization builds by removing postcondition candidates from a clone;
- the row and column passes of edges after kills;
- the interning of derivations for recreated cells;
- per-event kill scans.

Fixed-run spends most of that in fallback views; wfgrep in joins and edge insertion.

### Retained-proof follow-up results

The [follow-up pairs](../../experiments/proof-use-cost/incremental-refinement-pairs-2026-09-17.tsv)
compare `3f205ff6` with `e598a887`, the production proof-preservation repairs.
Both are Cargo `gate` compilers built with rustc 1.98.1 on the same macOS
26.6.2 host. The host guard admitted one experiment, with no other Whitefoot
build or test running. Five alternating warmed pairs cover the same 15-source
inventory as the historical TSV; this is compiler execution through LLVM
emission, with no clang invocation or native program execution.

| Source | Before repairs | After repairs | Median change |
|---|---:|---:|---:|
| fixed-run | 1.181531 s | 1.207622 s | +2.21% |
| wfgrep | 0.886768 s | 0.943300 s | +6.38% |
| prefix | 24.107 ms | 23.966 ms | -0.58% |
| histogram | 28.736 ms | 28.659 ms | -0.27% |
| radix scatter | 70.939 ms | 71.189 ms | +0.35% |
| growing-256 | 3.062684 s | 3.057336 s | -0.17% |
| control-256 | 296.309 ms | 298.692 ms | +0.80% |

No source exceeds the follow-up's 10% and 1 ms reporting threshold. The other
eight fixtures range from -2.14% to +0.78%. All 150 rows accepted, and all
15 LLVM byte sequences and per-arm SHA-256 digests agree across both compilers
and all five pairs. The complete experiment, including building the small
Rust runner and hashing outside the timed intervals, took 64.21 s. This does
not repeat the historical PR #68 comparison or isolate each repair's cost.

Compiler SHA-256:

```text
3f205ff6 bb758e599adb4b166db4e9010cba199148f0873474129746b9af696f003dea5a
e598a887 7af36d04920150f72e173fd3444123b383bd4bba38b726358c8d265507ce196f
```

The three new proof-record tests and expanded generated-flow comparison all
fail on the `3f205ff6` production implementation when given the new tests.
They fail respectively at the missing join boundary, removable dependency
through a neutral contradiction, missing ordinary delivery bound, and the
ordinary-layer mismatch in generated case 262. A deliberately modified LLVM
output also makes the comparison runner fail at pair zero rather than publish
a timing-only success.

One pre-existing defect remains separate: S12 holder kills lose field
precision. The [normative acceptance case](../../../tests/conformance/cases/ent5-pos-postcondition-sibling-field-write.wf)
keeps `observed == deref(pair).left` across a write to `pair.right`.
The postcondition route fails at FN-8 in both compilers above, while its
ordinary-read twin accepts. It is tracked in `docs/todo.md` and the conformance
manifest as `xfail`, not recast as a language rejection. The follow-up does
not claim universal acceptance equivalence in the presence of this older
support-classification defect, nor a formal proof of the incremental engine.

To reproduce, build the two pinned revisions with `--profile gate`, then run
the existing `compare` target with `SIZES=16,64,256,4096` and `REAL_SOURCES`
listing `tests/programs/compute/{prefix,histogram,radix_scatter}.wf`,
`tests/programs/wfgrep.wf`, and `tests/programs/fixed_run_library.wf` as absolute
paths. Set `BASELINE`, `CANDIDATE` and `WORK_ROOT` explicitly and run under
`.github/run-check.pl`. The runner requires `shasum` for retained SHA-256
evidence. Later test-only route counters and conformance evidence do not alter
the measured production mechanisms.
