# Why a small edit gains nothing from `--par`

## Question

After the [recursive-offer grain fix](../recursive-offer-grain/DESIGN.md),
Snowghost-wf's incremental edit pair (insert, then delete, a sentence in one
text node of the ECMAScript page) takes at four workers 1.08 to 1.10 times
its sequential time on the 14900K, about 250 microseconds per edit: slightly
slower, not faster. The
owner asked why it is not faster: whether the edit has no work that can run
concurrently, or whether such work exists and is lost to scheduling cost or
never offered (refused by the call grain, or denied permission by the
checker). A full build of the same page is faster under `--par` (layout
about 2.05 times, style 3.5 to 3.8 times at four workers on the 14900K), so
the question is about the edit's own work.

## Hypotheses

- **C, chain.** Most of the edit's time is a true dependency chain (the
  ancestor chain each level of which feeds the next), so no schedule can
  overlap it.
- **G, gap.** A material share of the edit's time is work that is
  independent in principle but denied permission because the program cannot
  prove it independent in today's language, such as the suffix translation's
  writes, whose target slots the checker cannot certify as distinct.
- **O, overhead.** A material share is permitted and offered, and its tasks
  are too small to pay for scheduling.

They are not exclusive; the measurement assigns the edit's time to them.

## Measurement

Snowghost-wf `3ec4bb491`'s `layout_oracle`, built with `wf-exp-95be0340e904`
(the same call grain as main at `db72af434`), on a hosted runner:

1. A sequential profile of the 2,000-edit pair and of its first edit pair
   alone, with absolute sample counts per function. The difference, over
   1,998 edits, is each function's per-edit time; setup cancels.
2. `--par-ledger` of the `--par` build: for each function carrying
   per-edit time, whether its statement pairs and loops are permitted,
   offered or omitted, and each denial's stated reason.
3. `perf stat` task-clock and steal counts of the `--par` build at one and
   four workers, for CPU utilization and steals per edit.

Each per-edit function's time is assigned to C (its denials are data
dependencies of the algorithm, by its source), G (its denials name a proof
or effect the program cannot state although the work is independent), or O
(it lies in a permitted, offered region). The Amdahl bound is then the
four-worker speedup if all of G and O ran perfectly in parallel and C ran
as it does.

Readings, fixed now:

- C holds if C carries at least 70 percent of the per-edit time; the edit
  then cannot gain much from any schedule, and the answer is "no
  concurrency to find".
- G is material if it carries at least 20 percent; that is a language or
  checker gap to be stated as its minimal witness and brought to the owner
  as a decision.
- O is material if it carries at least 20 percent while steals per edit
  stay near the six the recursive-offer investigation's third diagnostic
  measured; the tasks are then too small, a grain question.

## Results

Snowghost-wf run
[37840469770](https://github.com/Ming-Research/Snowghost-wf/actions/runs/37840469770)
(branch `research/gran-profile`, hosted `ubuntu-24.04`): `perf record -F 4999
-g` of the sequential image over the 2,000-edit pair and over its first pair
alone. The full run counts 21,500 listed samples and the setup run 16,436,
so 5,064 samples (about 1.0 CPU second, roughly 510 microseconds per edit on
this host under the profiler) belong to the edits. Per-edit self samples:

| Function | Samples | Share |
|---|---:|---:|
| `translate_reference_payload` | 1,638 | 32.3% |
| `reference_owner_cursor` | 1,297 | 25.6% |
| `translate_reference_owner_suffix` | 811 | 16.0% |
| `slot_read` (the lookup both of the above call) | 310 | 6.1% |
| `__libc_calloc` | 207 | 4.1% |
| everything else, each at most 2.3% | 801 | 15.8% |

The caller-inclusive report lacks frame pointers and is not used; the four
translation functions' self time alone is 80 percent of the edit.

The suffix translation walks the owner AVL tree past the edited entry and
moves every later block, paragraph and child down by the edit's height
change. Its work per subtree is independent in principle: each entry names a
distinct block, paragraph or child, so the left subtree, the node's own
payload and the right subtree write disjoint targets. `--par-ledger` of the
`--par` build shows none of it is permitted:

- The two recursive calls each write `context` as a whole
  (`writes(context)`), so PAR-1's path test finds them overlapping. Proving
  them disjoint needs the fact that each block's owner sequence holds it
  once, an invariant over stored data that the writers establish and that no
  source form lets a caller assume; Snowghost-wf's write-up is
  `research/investigations/m2-edit-cost/inverse-proof/call-site.md` on its
  branch `research/m2-frag-a-proof` (`ad6108a`), and the Whitefoot session
  that owns proof work carries it as its stored-invariant card.
- Independently of that proof, the compiler forms no run through the left
  call: it sits in `if skip < before { ... }`, and the permission planner
  (`compiler/src/semantic/permission.rs`, `classify`) gives a footprint to a
  `match` only when its scrutinee is a call and refuses every other
  `if`/`match` statement as "a match statement". PAR-1 permits such a
  statement with the footprint of its condition and every arm that may
  execute, so the implementation admits less than the specification.
- The per-atomic loop of `translate_reference_payload` (line 388) is denied
  for the same reason as the calls: its body writes `context` without an
  admitted element family.
- The one permitted group, the two cursor reads, is a few loads; the call
  grain now omits its offer.

The same work written as the dense variant's counted loop
(`translate_dense_reference_suffix`, line 512) is permitted, because a counted
loop over slots needs no proof that the owner sequence names each target once.

Classification of the per-edit time:

- **G, about 80 percent**: the suffix translation, independent in principle,
  denied for want of the stored-data invariant, and additionally unreachable
  for statement groups because the planner refuses `if` statements.
- **C, at most about 20 percent**: reshaping the edited paragraph, finishing
  its lines and the ancestor chain, whose levels feed one another, plus
  allocation.
- **O, about 0 percent** after the grain fix: the `--par` build at four
  workers steals about seven tasks per edit; its whole run uses 11.2 CPU
  seconds over 3.8 seconds against 4.6 over 4.6 at one worker, the extra
  CPU being workers searching for work that is not offered.

By the readings fixed above, C does not hold and G is material. The Amdahl
bound with G perfectly parallel at four workers and C sequential is 1 /
(0.2 + 0.8 / 4) = 2.5 times; real tasks would be coarser than the 13,600
entries, so the attainable gain is below that.

The four-worker edit's remaining 8 to 10 percent is not offered work: the
recursive-offer investigation measured about 15 microseconds per edit of
fixed `--par` cost on main's edit path at two and four workers on the
14900K with either compiler, which this edit pays as well.

Answer to the question: the edit is not faster because about 80 percent of
it, the suffix translation, is independent work that the program cannot
prove independent in today's language, and the compiler would not form the
group even with the proof. It is neither a dependency chain nor lost to
scheduling overhead.

## Follow-up

The owner chose to let a conditional call join a statement group
(`design/compiler/parallel-lowering.md`): an `if` whose one non-empty arm is
a single call with total arguments is now a PAR-1 member and lowers, inside
a group, to a call of a synthesized guard function.

That does not by itself let the suffix translation's two recursive calls
form a group, even once the stored-data invariant makes them disjoint. The
function body at Snowghost-wf `3ec4bb491` (`renderer/layout/reference.wf`,
lines 428 to 438) is, in order:

```text
let after = before +sat held.own_events;
if skip < before { translate_reference_owner_suffix(...left...); }
if skip < after { let own_skip = skip -sat before; let item = reference_owner_payload(...); translate_reference_payload(...); }
let right_skip = skip -sat after;
translate_reference_owner_suffix(...right...);
```

The left call's statement is now a member. The payload statement's arm has
three statements, so it is still refused and ends every run, and the
`let right_skip` before the right call separates it from any group. A group
of the left call, the payload and the right call would need either the
source to compute `own_skip`, `item` and `right_skip` before the
conditionals, so that each arm is a single call with total arguments, or a
wider member form. Which one, and whether the benefit pays, can be measured
only once the stored-data invariant exists.
