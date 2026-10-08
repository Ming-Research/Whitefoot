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

## Found along the way

- `publish_reference_owner_suffix` in the same Snowghost module is a
  recursion that does offer its own calls, but `--par-ledger` excludes it
  from the budget family because it "has no sequential clone", so its offers
  nest without a budget. The pair edit does not reach it; an edit that
  publishes a reference suffix would. Recorded in `docs/todo.md`.
