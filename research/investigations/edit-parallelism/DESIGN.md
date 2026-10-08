# Why a small edit gains nothing from `--par`

## Question

After the [recursive-offer grain fix](../recursive-offer-grain/DESIGN.md),
Snowghost-wf's incremental edit pair (insert, then delete, a sentence in one
text node of the ECMAScript page) runs at four workers in 1.08 to 1.10 times
its sequential time on the 14900K, about 250 microseconds per edit. The
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
  stay near the six D3 measured; the tasks are then too small, a grain
  question.

## Results

None yet.
