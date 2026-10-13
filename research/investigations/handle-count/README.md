# Does skipping the handle count make a value handle as fast as a reference?

Status: registered before any result. The experiment's files are in
[`research/experiments/handle-count/`](../../experiments/handle-count/); its
workflow `tmp-handle-count.yml` is temporary and is removed after the run.

The decision under test is in
[`design/compiler/waiting-contexts.md`](../../../design/compiler/waiting-contexts.md):
an atomic statement counts a handle of its own only when its guard or block may
write, move or release a place it read the handle from, or when it read the
handle from no place or from one not rooted at a binding. Before the change, a
statement counted a handle in every case except one reached through a
reference parameter whose row writes nothing below it. The count is one update
before the statement and one after it, to the object's header, a line that
every context's statements on that object share. The design records that the
gain for a handle in a local, a value parameter or a field was not yet
measured. This investigation measures it.

## Question

With the pull request's compiler, does an atomic statement through a value
handle (the handle in a local or a value parameter) run at least 0.95 times
the throughput of the same statement through a read-only reference parameter,
which the base compiler already leaves uncounted, on the 14900K's 8 P-cores?

## Workload

One shared concurrent hash map, `Shared<ConcurrentHashMap<u64>>`. Eight
contexts run on eight drivers, one driver per P-core, each for a deadline of
1000 ms after a common gate opens. Context `i` owns the key `i` (eight bytes
distinct per context), seeded with 0 before the contexts start, so no timed
operation inserts. Each operation is one
`atomic slot = &keys[key] { ... }` statement that adds one to the context's
own value. Keys are distinct, so the per-key locks never contend, and the only
line the contexts share is the object's header, where the handle count lives.
Operations run in batches of 1024 with one clock reading per batch. The
throughput of an arm-run is the operations all contexts made divided by the
time from the gate's opening to after the last join.

The run is time-bound, not count-bound: a count that lasts about one second
would have to be fitted to each arm's rate, and a different count per arm
would differ in more than the handle. The deadline is the same 1000 ms for
every arm, so an arm-run lasts about one second on any rate.

After the run the program sums the map's values and compares the sum with the
operations the contexts reported; a difference makes it exit 3. It also prints
`check` equal to that sum, and the harness requires `check = ops` on every row.

## Arms

The two sources differ only where the handle lives. Both give each context a
`Shared<ConcurrentHashMap<u64>>` it owns; both call a walk function that
loops over the batches.

- V (`value.wf`): the walk function owns the handle as a value parameter and
  writes `&keys[key]`.
- R (`reference.wf`): the walk function takes `keys: &Shared<ConcurrentHashMap<u64>>`
  with the row `reads(keys)`, so nothing below the parameter is written, and
  writes `&keys^[key]`.

Runs, each built with `--full-lto`:

| Arm | Source | Compiler | Role |
|---|---|---|---|
| PRV | V | the pull request's head | the arm under test |
| PRR | R | the pull request's head | the reference arm |
| BV | V | the pull request's merge base with main | falsifier: the base counts the handle here |
| PRR2 | R | the pull request's head, built a second time into its own binary | twin of PRR, noise control |

The binary of each arm has its own file. Compiler revisions are recorded in the
run's `host.txt`.

## Comparison

Blocks of the four arms. Within a block each arm runs once, pinned by `taskset`
to the P-core list `0,2,4,6,8,10,12,14` (one hyperthread per core), with
`WF_DRIVERS=8`. The arm order rotates by one position per block and is
reversed in odd blocks, so each pair of arms runs in both orders equally
often. The main run has 12 blocks; the pilot (2 blocks) only checks the
wiring and the arm-run length on the 14900K and decides the block count.

Per block the harness computes V/R = PRV/PRR, the falsifier BV/PRR and the
twin deviation PRR2/PRR - 1. The reading is the median of V/R over the blocks
that stay, with a 95% percentile bootstrap interval (10000 resamples).

## Rejection criterion and void rules

The owner's registered criterion: the proposal is accepted when the median V/R
is at least 0.95. It is rejected when the median V/R is below 0.95: the
skipped count then buys less than its cost in complexity, or a cost other than
the count dominates.

A block is void when the twin deviation |PRR2/PRR - 1| is above 0.03.
The whole measurement is void, and decides nothing in either direction, when

- fewer than 6 blocks stay;
- the falsifier is invisible: the median over the kept blocks of BV/PRR is
  above 0.95, so the base's counted handle is not measurably slower than the
  reference arm and the measurement cannot see the count;
- any arm-run fails, prints a witness name other than its arm's, reports
  contexts or drivers other than 8, or prints `check` different from `ops`;
- the structural check below fails.

The verdict is PASS when the measurement is not void and the median V/R is at
least 0.95; FAIL when it is not void and the median V/R is below 0.95. The
interval is reported, not part of the rule. The result is reported no wider
than what it tested: one map workload with distinct keys, 8 contexts on 8
drivers, one machine and one pair of compiler revisions.

## Structural check

Before any timing, `build.sh` emits the LLVM of each source under each
compiler and counts the calls of the handle-count function
(`call void @wf__shared_share(`; the explicit `shared_share` calls that spawn
the contexts are the same in all). It requires: calls(PR, V) = calls(PR, R);
calls(base, R) = calls(PR, R) (R is unchanged by the pull request); and
calls(base, V) > calls(base, R) (the base counts V's handle). This shows that
the arms are what their names say, independently of the timings.

## Results

(empty until the run)
