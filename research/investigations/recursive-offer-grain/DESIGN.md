# Grain of recursive offers under `--par`

## Question

The [call-offer grain](../call-offer-grain/DESIGN.md#recommendation) offers
a statement-group call only when its callee reaches the 150,000-unit work
unit statically or belongs to or reaches a cyclic call component. Recursion
is exempt because a static summary cannot bound it; the
[recursion budget](../../../design/compiler/parallel-lowering/two-worlds.md)
limits how deeply its offers nest, about eight levels at four workers. The
investigation that chose the rule named its known limit: "a cheap call that
enters a recursive component keeps its offer however often it runs".

Snowghost's incremental layout meets that limit. One edit of the ECMAScript
specification page (insert a sentence into one text node, then delete it)
translates the retained suffix of a few owner trees, walking about 13,600
held entries with the recursion `translate_reference_owner_suffix` in
Snowghost-wf `renderer/layout/reference.wf` at `3ec4bb491`. That recursion
visits an AVL tree's suffix: when the skipped prefix reaches into the left
subtree it recurses there, then translates its own payload, then recurses
into the right subtree; the two recursive calls and the payload write
disjoint coordinates, so PAR-1 permits them to overlap, and every level above
the budget's cut is a fork whose subtree holds a few microseconds of work.

Snowghost-wf's measurement, on a GitHub-hosted 4-vCPU `ubuntu-24.04` runner,
run [37636318467](https://github.com/Ming-Research/Snowghost-wf/actions/runs/37636318467)
(compiler release `wf-0b7f5c5b9854`, 2,000 edits, median microseconds per
edit, identical work counters in every build):

| Driver | Sequential | `--par` W1 | `--par` W4 |
|---|---:|---:|---:|
| `3ec4bb491` ("recent") | 608 | 607 | 5,533 |
| `0430437c8` ("prior") | 689 | 697 | 1,056 |

Its profile of the recent W4 run puts 65 percent of samples in
`wf__par_worker_main`, 8.9 percent in `wf__par_join` (half of it under the
budget variant of `translate_reference_owner_suffix`) and 7.3 and 2.5 percent
in the budget variants of `translate_reference_owner_suffix` and
`translate_reference_payload`; the prior build's profile names no budget
variant of either. The task-clock samples of the W4 run count about 50 CPU
seconds against 4.5 for the sequential run, and the two translation
functions' own samples grow from about 0.5 CPU seconds sequentially to about
5 at four workers.

The question: what should decide whether a recursive offer is made, so that
a recursion whose subtrees hold little work runs as fast with four workers as
with one, while the recursions that gain from offers keep their speedups?

## Hypotheses for the reproducer

- **H1, recursive offers.** The slowdown is the recursive offers of the
  suffix translation: steals of microsecond subtrees, the offering lane's
  wait for them at the join, and the stolen subtrees' writes on other cores.
- **H2, a per-edit runtime cost.** The slowdown is a fixed cost every edit
  pays once workers exist (waking parked workers, for instance), and the
  recursive offers only carry it.

The prior driver's 1.5-times W4 cost, with no recursive variant in its
profile, is a separate cost of the same kind of size; Snowghost-wf's
`docs/todo.md` attributes its par-4 E1 colour-edit loss to "the runtime's
fixed cost". It is outside H1 and is not this investigation's first subject.

## Diagnostic D1: cut the recursive offers

Recorded before the run. The recent driver's source, built with the same
compiler release, as five configurations of one image family:

- `seq`: built without `--par`.
- `auto` and `twin`: built with `--par`, two separate builds of the same
  source and options, an identical-image control.
- `f1`: `--par --par-recursive-frontier 1`, one level of recursive offers
  under each entry into the component.
- `off`: `--par --par-recursive-frontier off`, recursive offers at every
  level.

Each `--par` image runs at `WF_WORKERS=1` and `WF_WORKERS=4` with
`WF_SCHED_REPORT=2`, which prints the process's steal count at exit; `seq`
runs once per round. Three rounds, the order reversed on the second, on one
hosted 4-vCPU runner, the same 2,000-edit pair script. A cell is the median
per-edit time of a run; a configuration's figure is the median over rounds.
`--par-ledger` of `auto` lists the offers of the two translation functions.

Readings, fixed now:

- The run is conclusive only if `auto` and `twin` at W4 lie within 15 percent
  of each other and every `--par` image at W1 lies within 10 percent of
  `seq`.
- H1 holds if `f1` at W4 is at most 2 times `seq`, its steal count falls at
  least tenfold against `auto`, and `off` at W4 is no faster than `auto`.
- H2 holds if `f1` at W4 stays at 5 times `seq` or more.
- Anything else leaves both open and names what the next diagnostic must
  separate.

D1 selects no policy. A budget of one is a [rejected
policy](../call-offer-grain/DESIGN.md#rejected-alternatives): it cut useful
recursive offers with useless ones on the style setup, and it would do so
here as well.

## Prior objections a policy must answer

A policy for recursive offers is chosen only after the diagnostics, by the
owner. These earlier results constrain it:

- Static pricing of a recursive callee without an exemption removes the
  formal quadrature kernel's recursive offer, 2.3 to 2.4 times slower at
  four workers ([call-offer grain](../call-offer-grain/DESIGN.md#rejected-alternatives)).
- Stopping publication from a site whose offers do not pay, judged at run
  time, left setup 1.5 to 2.2 times slower, failed the formal regression rule
  on quadrature and FIR and charged every offer a site lookup (same section).
- A fixed recursion depth cannot follow an unbalanced tree: Snowghost's style
  shape B on apollo11 needs a budget of 24 to gain, while the default is
  about eight ([recursion budget at splits](../call-offer-grain/DESIGN.md#the-recursion-budget-at-splits)).
- A heartbeat gate on promotion was rejected for the runtime because it
  cannot lift the coarse ceiling or free a caller waiting on its handed-out
  half ([parallel runtime](../../../design/compiler/parallel-lowering/parallel-runtime.md)).

## D1 result

Snowghost-wf run
[37766202672](https://github.com/Ming-Research/Snowghost-wf/actions/runs/37766202672)
on branch `research/gran-diag` (`eb4e3ec` plus the cache flag): one hosted
4-vCPU `ubuntu-24.04` runner, compiler `wf-0b7f5c5b9854`, the five images
built from `3ec4bb491`. Median per-edit microseconds, the median of three
rounds' medians, and the steals each process reported at exit:

| Image | W1 | W4 | W4 steals |
|---|---:|---:|---:|
| `seq` | 394 | | |
| `auto` | 397 | 3,514 | 20.0 M |
| `twin` | 399 | 3,498 | 20.0 M |
| `f1` | 401.5 | 3,182 | 20.0 M |
| `off` | 401 | 3,159 | 20.0 M |

The run is conclusive: `auto` and `twin` are byte-identical images 0.5
percent apart at W4, and every `--par` image at W1 lies within 2 percent of
`seq`. H1 as stated fails: `f1` at W4 is 8.1 times `seq` and its steals did
not fall. The H2 reading holds (`f1` stays above 5 times `seq`), but the
ledger shows that neither hypothesis named the mechanism.

## Attribution: offers the budget never sees

`--par-ledger` of `auto` lists what the translation component offers. Its two
recursive calls are not in any group: PAR-1 denies the pair before the left
call and the pair after the right one. What it permits are two groups of
non-recursive calls in every activation: the reads of the left and the right
child's cursor, `reference_owner_cursor` twice, and the payload read. The
cursor read is a few loads and a call of `sequence_node`, which reads one
slot through `slot_read`, a page-directory descent that recurses once per
level. So the cursor read reaches a cyclic call component, and the call grain
keeps its offer however small it is.

The recursion budget does not bound these offers. It spends a level only at a
call into the component that is a member of a group
([two worlds](../../../design/compiler/parallel-lowering/two-worlds.md)), and
here no such call exists, so every activation keeps the budget it entered
with and hands out its cursor read. Each edit walks about 13,600 held
entries; 20.0 million steals over 2,000 edits is about 10,000 per edit, one
stolen cursor read of a few loads for most nodes. That is why `f1`, `auto`
and `off` differ in bytes and not in time: the budget's initial value is
never spent.

The deduction behind the proposed rule. The call grain exempts a callee that
reaches recursion because "a static summary cannot bound recursion, so this
rule treats recursion as unbounded and leaves its depth to the recursion
budget" ([call-offer grain](../call-offer-grain/DESIGN.md#recommendation)).
Its premise holds only for a component one of whose groups calls into it:
for any other component the budget spends nothing at any depth. `slot_read`,
like any recursion that offers none of its own calls, therefore gets the
exemption without the bound that justified it.

## Proposed rule

Keep a statement-group offer when its callee belongs to or reaches a cyclic
component that offers its own calls (a group of one of its members calls into
it), or when its static work reaches the work unit. A callee that reaches only
recursion offering none of its own calls is priced by its static work summary
like any other callee; that summary already substitutes callees three rounds
deep, recursion included. Recursions that offer their own calls, which are
the ones the formal quadrature and merge sort kernels and Snowghost's style
shape B gain from, keep the exemption unchanged.

Its known limit: a callee whose work is large only through a long recursion
that offers nothing, such as a recursive walk of a long list, now weighs about
three bodies and loses its offer, as a non-recursive helper with one loop over
a large argument already does. No measured program is known to have one; the
emission comparison below looks for it.

Validation, recorded before the measurement:

1. `call_grain_prices_callees_reaching_only_unoffered_recursion` fails under
   the former rule (the lookup pair is handed out) and passes under this one,
   and a lookup reaching a recursion that offers its own calls keeps its
   offer.
2. The `--par` emission of every program under `tests/programs` and of the
   formal kernels is byte-identical to the base compiler's, or each
   difference is named with the offer it removes and timed.
3. On the D1 workload, the candidate's `--par` image at W4 is at most 1.1
   times `seq` and W1 within 10 percent of `seq`, with a twin control.
4. Snowghost's full-layout timing of html5 and ecma262 at four workers is no
   slower than the base compiler's (Snowghost-wf runs it on the 14900K).

## Alternatives considered

The owner chose the proposed rule on 2026-10-08 over two alternatives:

- **Keep the exemption and spend a budget level at every activation that
  makes an offer.** The cursor reads would then stop below the budget's cut,
  but every entry into such a component would still hand out an offer at
  each activation above it, up to about 2^8 of them at four workers (the
  default budget of eight levels, with both recursive calls passing the
  budget down), each a few loads, against an edit whose whole sequential
  work is a few hundred microseconds. It would also change when the budget is
  spent in recursions that do offer their own calls, which the
  [recursion budget at splits](../call-offer-grain/DESIGN.md#the-recursion-budget-at-splits)
  set for Snowghost's style shape B.
- **Leave the rule and rewrite the program** so it reads no cursor pair in
  parallel. This routes around a compiler defect in the program that
  exposed it; any program calling a lookup per node of a recursion would
  meet it again.

The rule is applied to the groups that remain after pruning, decided again
after each pass until a pass omits nothing: omitting a small member can
dissolve the only group that made a recursion count as offering its own
calls, and the budget spends nothing in such a recursion
(`call_grain_prices_callees_reaching_only_unoffered_recursion` holds that
case).

## Emission comparison (validation 2)

A temporary workflow on this branch (run
[37781288310](https://github.com/Ming-Research/Whitefoot/actions/runs/37781288310))
built the merge base's and this branch's compilers and emitted every program
under `tests/programs` and `research/experiments/par-quicksort` with `--par
--emit-llvm`, the multi-file programs in the source sets their tests compile
together. 97 single-file programs and all five multi-file sets (the three
raw-deflate drivers, the slab and the indexed-membership programs) are
byte-identical, the five formal kernels among them. One differs:
`tests/programs/containers/ordered-map-program.wf`, a container correctness
program, loses three offers, two calls of `ordered_map_put` (static work
135,892 and 88,532) and one of a test check (332), each of which reaches only
the ordered map's one-way descent. It is not timed: the program inserts a
handful of keys per call to check results, so its offers measure nothing a
performance criterion protects, and the criterion's wording ("timed") is not
met for it.

That run measured the single-pass grain (`95be0340e`). After the fixed point
was added, the comparison ran again on `76649d32a` (run
[37786236991](https://github.com/Ming-Research/Whitefoot/actions/runs/37786236991))
over `tests/programs` only, since the repository forbids a workflow input
from `research/`: 96 single-file programs and the five multi-file sets stay
byte-identical, and `ordered-map-program.wf` loses one more offer, a call of
`ordered_map_free` (static work 816) whose recursion offered its own calls
only through a group the grain dissolves. par-quicksort is not rerun: its
only group pairs its two recursive calls, so no pass can dissolve it.

## Candidate on the D1 workload (validation 3)

The owner chose the proposed rule on 2026-10-08. Experiment release
`wf-exp-95be0340e904` (this branch at `95be0340e`, gate green) against
`wf-c18e6708b6cc` (main at the merge base, the same source without the
change), both building `3ec4bb491`'s `layout_oracle`, Snowghost-wf run
[37777439403](https://github.com/Ming-Research/Snowghost-wf/actions/runs/37777439403),
one hosted 4-vCPU runner, three interleaved rounds, median per-edit
microseconds and the process's steals:

| Compiler | Sequential | `--par` W1 | `--par` W4 | W4 steals |
|---|---:|---:|---:|---:|
| base | 263 | 266 | 2,692 (twin 2,729) | 19.0 M |
| candidate | 261 | 262 | 355 | 74 k |

Work counters are identical in every cell. The candidate removes the cursor
offers (`--par-ledger` names 22 omitted Snowghost offers that reach only
recursion offering none of its own calls, `reference_owner_cursor` among
them) and takes the four-worker edit from 10.2 to 1.36 times sequential.
Criterion 3 (at most 1.1 times) is not met: about 94 microseconds per edit
remain at four workers.

## Full builds on the 14900K (validation 4)

Snowghost-wf ran its full-layout and style timing on the 14900K (run
37783423140): Snowghost main `8fbc160` built with `wf-c18e6708b6cc`, a twin
of that build, and with `wf-exp-95be0340e904`; three interleaved rounds,
best round, layout on ecma262 and html5 and style on ecma262, html5 and
apollo11, sequential and four workers. Every candidate-to-base ratio lies
within the twin's own spread, which reaches 3.3 percent: the whole layout
stage reads 1.004 and 0.992 (ecma262, sequential and W4) and 1.014 and 1.014
(html5), against twins of 0.996, 1.000, 1.010 and 1.021; style reads 0.989
to 1.008. Omitting the 22 Snowghost offers, `style.applied_value` and
`inherited_float_reach` among them, costs nothing measurable on full builds.

The same session timed the edit pair on main's source (run 37788438619),
which lacks the suffix recursion: best of three rounds, base 77 sequential,
93 at W4; candidate 78 and 96. Main's edit path pays about 15 microseconds
per edit at two and at four workers with either compiler, a fixed cost the
grain does not touch.

## The edit pair on the 14900K

Snowghost-wf timed the pair on the 14900K (run 37794254936): source
`3ec4bb491`, built with `wf-c18e6708b6cc`, a twin of that build, and
`wf-exp-95be0340e904`; three interleaved rounds, median microseconds per
edit per round, best round shown, steals from a separate untimed pass:

| Compiler | Sequential | W1 | W2 | W4 | W4 steals |
|---|---:|---:|---:|---:|---:|
| base | 242 | 237 | 2,320 | 3,128 | 19.2 M |
| twin | 234 | 240 | 2,286 | 3,105 | 19.2 M |
| candidate | 248 | 235 | 267 | 274 | 78 k |

The candidate's four-worker edit is 1.10 times sequential on the best rounds
and 1.08 on the median of rounds, two workers 1.08, against 12.9 and 9.6
times for the base; rounds spread by 5 to 10 percent at these sizes, so the
result is at the criterion rather than clearly under it. The hosted runner's
four-worker residual does not appear on this host, which agrees with, but
does not establish, the spinning-worker attribution below.

## D3: the residual four-worker cost

Recorded before the run. On the candidate's images, three rounds of: the
sequential image at W1; the `--par` image at W4 and W2; at W4 with
`WF_SPLIT_WORK=1000000000`, under which no range split affords a second
chunk; and the same two W4 settings on a two-edit script, whose steal count
is the setup's, so that the 2,000-edit run's steals less the two-edit run's,
over 1,998, are the steals per edit. A profile of the W4 run follows.

- If the W4 time without splits is at most 1.1 times sequential, the
  residual is the edit path's range splits.
- If the edit path steals less than one task per edit and W4 stays above
  1.1 times sequential, the residual is a cost of having workers at all
  (waking or spinning workers, for instance), not of any offer.
- Otherwise the profile names the next candidate.

## D3 result

Snowghost-wf run
[37781362015](https://github.com/Ming-Research/Snowghost-wf/actions/runs/37781362015),
the candidate's images, one hosted runner reporting an AMD EPYC 7763 with 4
CPUs as 2 cores of 2 threads each. This runner was slower than the previous
one; every cell below comes from it. Median per-edit microseconds, median
of three rounds, and the process's steals:

| Setting | Per edit | Steals, 2,000 edits | Steals, 2 edits |
|---|---:|---:|---:|
| sequential image, W1 | 504 | | |
| `--par`, W2 | 506 | 41 k | |
| `--par`, W4 | 728 | 72 k | 59 k |
| `--par`, W4, no range splits | 739 | 118 k | 110 k |

- Range splits are not the residual: without them W4 is no faster.
- The edit path steals about six tasks per edit at W4 ((72,132 - 59,219) /
  1,998), so the first reading's condition of less than one does not hold;
  the profile decides.
- Two workers cost nothing (506 against 504) and four cost 224 microseconds
  per edit (1.44 times). The W4 profile puts 39 percent of samples in
  `wf__par_worker_main` and 1.3 percent in `wf__par_join`, and the edit's
  own functions keep their sequential shares.

Provisional attribution, not yet separated by an experiment: with four
lanes on two two-thread cores, three workers spin in their idle window and
one of them shares the main thread's core, slowing it; with two lanes the one
spinning worker can sit on the other core. The idle window is used exactly
when the lane count fits the CPUs the process may use
([parallel runtime](../../../design/compiler/parallel-lowering/parallel-runtime.md)),
and four lanes fit four CPUs that are only two cores. The runtime decision
records an earlier "sparse-cadence loss on hosted SMT runners" whose
comparisons did not reproduce across apparently identical machines. This is
a property of the runtime's waiting policy on an SMT host, not of the call
grain, and it is recorded as status board item `gran-par-s5`. The pair workload on the
14900K, where four threads need not share a core, would separate it; it is
requested with Snowghost-wf's full-layout check.

The candidate therefore meets criterion 3 at two workers on this host and
not at four; whether the four-worker residual is the host's cost of
spinning workers is open.

## Found along the way

- `publish_reference_owner_suffix` in the same Snowghost module is a
  recursion that does offer its own calls, but `--par-ledger` excludes it
  from the budget family because it "has no sequential clone", so its offers
  nest without a budget. The pair edit does not reach it; an edit that
  publishes a reference suffix would. Recorded as status board item
  `coord-wfbl-03-61`.
