# Call-offer grain under `--par`

## Question

Under `--par` the compiler offers independent calls to idle workers: every
member of a permitted PAR-1 statement group but the last is published to the
calling lane's deque, and the halves of a PAR-2 range split are published the
same way ([parallel lowering](../../../design/compiler/parallel-lowering.md),
[parallel runtime](../../../design/compiler/parallel-lowering/parallel-runtime.md)).
A range split is priced: the splitter asks the runtime how many chunks the
span affords at a 150,000-unit work floor. A call offer is not priced. The
only filter is the scalar-leaf limit, which drops offers of one-block
functions over scalar values with at most 16 operations, and the tree says
that "no universal grain policy is selected, because one exploratory result
on one workload supports the provisional default".

The first large real program built with `--par`, Snowghost's style prototype,
is swamped by call offers. Its page setup parses the 7.6 MB ECMAScript
specification with `pkg::html::tree_builder` in 0.30 s at one worker and
17.5 s at four, about 18 million steals, as its author recorded in the
prototype's `run.sh`.

What grain policy should `--par` use for call offers, so that ordinary real
programs never get slower with more workers, while the maintained parallel
programs keep their measured speedups?

## Selection criterion

Recorded before any candidate policy was built or measured. A candidate is
selectable only when it meets all three conditions on this host, in the same
session as the baseline it is compared with, and produces the same results:
the prototype's `check` mode passes on every page and every maintained program
passes its own result check.

1. **No real page sets up slower with more workers.** For each of the three
   real pages of the prototype's dossier (ecma262, html5, apollo11), the
   candidate's `--par` build has a best-of-seven setup time T(0) at
   `WF_WORKERS=2` and at `WF_WORKERS=4` no greater than its own best-of-seven
   T(0) at `WF_WORKERS=1` plus the noise allowance: 5 percent of the
   one-worker time or 10 ms, whichever is larger. On this shared host the
   sequential and one-worker best-of-seven times of the
   [par-quicksort record](../../experiments/par-quicksort/README.md#results)
   repeat within 2 percent between batches and the four-worker time within
   6 percent; the configurations compared here run interleaved in one block,
   which removes the batch term, and 10 ms covers process start and page
   reading on the fastest page.
2. **The maintained parallel programs keep their times.** A program whose
   candidate `--par` emission (`--emit-llvm`) and linked runtime are
   byte-identical to the baseline's is unchanged and needs no timing. Otherwise:
   - the five formal kernels of [`tests/performance`](../../../tests/performance/README.md)
     (Mandelbrot, records, FIR, quadrature, stencil) pass that runner's own
     rule against the baseline images (no kernel with two adverse widths, a
     width being adverse when its paired wall ratio is below 0.97 with the
     baseline faster in at least four of five pairs), after the same
     session's identical-image control passes;
   - [par-quicksort](../../experiments/par-quicksort/README.md) and every
     other program under `tests/programs` whose emission changes have a
     best-of-seven time at `WF_WORKERS=1`, 2 and 4 within 5 percent or 10 ms,
     whichever is larger, of the baseline's best of seven in the same block.
     Each such block also times a second copy of the baseline image as an
     identical-image control; a block whose control differs from the
     baseline by more than the same allowance is inconclusive and is rerun.
3. **Lowering cost stays within noise.** The front-end time `--report` prints
   for the style prototype's `--par` build, the largest program at hand,
   has a best of seven within 5 percent of the baseline compiler's best of
   seven.

When more than one candidate meets all three, prefer the one that adds no
work to an offer at run time, then the one stated over fewer program
properties, then the one with the lower worst four-worker to one-worker setup
ratio over the three pages. When none meets all three, no policy is
recommended as a default and the result names what each candidate lacks.

Blocks are timed under the shared verification lock. The one-minute load is
recorded with `uptime` before and after every block; the reading before the
block, taken once this investigation's previous block has decayed, is the
background load, and a block that starts above 2.0 is rerun.

## Method

The host has four CPUs (Intel Xeon at 2.10 GHz, one thread per core, one NUMA
node), Linux 6.18, Ubuntu clang 18.1.3 and rustc 1.98.1, and is shared with
other agent sessions. Every build and timing block ran under the shared
verification lock; unlocked processes of other sessions were not excluded.

Main's gate compiler (`9c4579fba`) refuses the prototype: its text literals
stop it at `renderer/css/selectors/anb.wf:8:35: error[CONST-2]:
UnexpectedToken` (`const odd_letters: Array<u8, 3> = "odd";`). The Snowghost
builds therefore use the compiler the prototype's author used, main plus the
text-literal pull request #166 at that branch's first commit `26761daef`. The
branch's later commits make the prototype's `'\u{9}'_u32` noncanonical, so
they do not build it either. The prototype policies live in a scratch copy of
`26761daef` with a patch that changes nothing unless an environment variable
selects a policy. Unselected, the scratch compiler emits LLVM byte-identical
to that compiler's for the prototype, and byte-identical to main's for every
program under `tests/programs` that both accept. They disagree only on
`raw_deflate_boundary.wf`, which uses text literals, and `tcp_gather.wf`,
which uses the marked waiting `let` main gained after `26761daef`; both
refuse par-quicksort and both fail `wfgrep`'s `--par` build (found along the
way, below). Its runtime with no instrument selected compiles to the same
object code as main's.

Snowghost is the snapshot of `research/concurrency` at `80d9d2c97874` in a
separate checkout, built from a copy; the pages are the pinned files of its
`run.sh`, with matching SHA-256 prefixes. Setup is the prototype's T(0) path,
`proto_style C 0 PAGE ua.css SHEETS...`: it reads the page, builds the tree
with `pkg::html::tree_builder`, parses the page's style sheets and builds the
traversal arrays, and runs no style repetition. A time is whole-process wall
time to the millisecond, the configurations of one block run round-robin, and
each cell is the best of seven.

Attribution uses two scratch instruments that change no uninstrumented build.
The runtime instrument counts, per published thunk (one per handed-out call
site) and per acquisition call site, the acquisitions, deque-full refusals,
publications, steals and inline joins, and times with the timestamp counter
the stolen and the inline executions and the offering lane's wait for a stolen
offer. The compiler instrument writes one line per thunk naming its parent and
callee and the callee's static properties: blocks, instructions, whether it
contains or reaches a loop, whether it belongs to or reaches a cyclic call
component, and the three-round static weight `assign_weights` computes for
range pricing. Counts and durations come only from the instrumented build;
wall times come only from uninstrumented builds.

## Reproduction

Setup seconds, best of seven, with the author's compiler. Each block started
at a one-minute load below 2.0 (1.85, 1.96 and 1.99).

| Page (elements) | Sequential build | `--par`, W1 | `--par`, W2 | `--par`, W4 |
|---|---:|---:|---:|---:|
| ecma262 (179,471) | 0.299 | 0.278 | 8.446 | 14.526 |
| html5 (117,179) | 0.214 | 0.229 | 5.319 | 8.449 |
| apollo11 (11,844) | 0.068 | 0.071 | 0.626 | 1.094 |

At one worker the `--par` build runs its sequential world and matches the
sequential build. Two workers make setup 30 times slower on ecma262 and four
workers 52 times; the author's 17.5 s at four workers was taken on the same
machine class at another time.

## Attribution

Offers during setup, from the instrumented build. An offer is an acquisition;
it is published unless the offering lane's 64-slot deque is full.

| Page, workers | Offers | Per element | Stolen | Popped back and run inline | Refused, deque full |
|---|---:|---:|---:|---:|---:|
| ecma262, W4 | 20,598,365 | 114.8 | 10,783,821 | 9,755,191 | 59,353 |
| ecma262, W2 | 20,598,365 | 114.8 | 10,069,816 | 10,470,849 | 57,700 |
| html5, W4 | 11,578,346 | 98.8 | 3,298,140 | 8,237,619 | 42,587 |
| apollo11, W4 | 1,339,070 | 113.1 | 291,183 | 1,042,329 | 5,558 |

The same classes of callee account for the offers on every page. By class, on
ecma262 at four workers (run times are means per execution, including the
thunk's frame traffic):

| What the offered callee contains | Sites | Published | Stolen | Stolen run | Inline run | Owner's wait per steal |
|---|---:|---:|---:|---:|---:|---:|
| Straight-line code, no call | 240 | 18,210,631 | 9,145,539 | 134 ns | 43 ns | 229 ns |
| Calls to small helpers, no loop or recursion | 32 | 1,542,140 | 1,100,430 | 879 ns | 208 ns | 1,038 ns |
| A loop, reached directly or through calls | 64 | 786,234 | 537,850 | 260 ns | 49 ns | 714 ns |
| Recursion (the halves of one range split) | 1 | 7 | 2 | 59 µs | 30 µs | 29 µs |

`pkg::html::tree_builder` publishes 19.4 million of the 20.5 million offers
(230 straight-line and 29 helper sites), `pkg::html::tokenizer` 1.1 million (two
loop sites, two helper sites, four straight-line sites) and `pkg::css::rules`
4,310. The heaviest offering functions:

| Offering function | Sites | Callees | Published | Stolen |
|---|---:|---|---:|---:|
| `tree_builder::is_addr_block_start` | 23 | `a_is`, `bor8` | 4,051,933 | 2,001,235 |
| `tree_builder::is_addr_block_end` | 22 | `a_is`, `bor8` | 3,871,560 | 1,924,253 |
| `tree_builder::in_body_start_tag` | 21 | `a_is`, `bor8` | 2,751,971 | 1,455,895 |
| `tree_builder::is_formatting_html` | 9 | `a_is` | 2,323,332 | 1,092,840 |
| `tree_builder::is_html_or_svg_or_mathml_table_context` | 4 | `a_is` | 1,677,204 | 788,990 |
| `tree_builder::is_h1_h6` | 5 | `a_is` | 1,603,620 | 765,388 |
| `tree_builder::is_special_html` | 73 | `a_is`, `bor8` | 1,319,135 | 768,734 |
| `tokenizer::clear_output` | 1 | `truncate_bytes` | 598,692 | 405,685 |
| `tree_builder::appropriate_place` | 1 | `a_is` | 419,307 | 288,713 |
| `tokenizer::push_attribute_dedup` | 2 | `attribute_index_reserve`, `push_attribute` | 374,956 | 264,833 |
| `tree_builder::is_html_p` | 1 | `ns_is_html` | 358,940 | 242,475 |

`a_is(atom: Atom, id: u32)` is `atom.index == id`: one block, static weight
2. `ns_is_html` is an enum equality of the same size, and `bor8` ors eight
flags through two `bor4` calls that are themselves an offered pair.
`truncate_bytes` is an uncounted loop that removes bytes past a length,
usually zero or one trip (weight 67), and `attribute_index_reserve` a loop of
weight 1,223.

The hottest callees are outside the scalar-leaf limit's domain rather than
above its count: `a_is(atom: Atom, id: u32)` takes a struct and `ns_is_html`
an enum, so no limit value reaches them. 330 of the prototype's 420 thunks
call a one-block function. Weighted by publications, 88.7 percent of setup
offers call a callee of static weight at most 2, 99.0 percent at most 100,
and none above 6,162, where the range work unit is 150,000. No setup call
offer reaches recursion; the one recursive offer site active in setup is a
range splitter, which publishes 7 halves.

`pkg::css::selectors`' `byte_eq` offers (`ascii_lower_byte` pairs) run in the
style stage, not in setup; the shape measurements below include them.

The mechanism is the ordinary cost of an offer against an idle pool. An offer
costs the offering lane an acquisition, frame stores, a push with a
sequentially consistent store and a shared epoch increment, an idle-mask
check, and at its join a sequentially consistent pop. Setup has no other
work, so idle lanes spin through their one-millisecond window scanning the
deques and take about half of all offers within a fraction of a microsecond.
A stolen two-instruction call then costs its offering lane the wait for
another core to load the frame, run the call, write the result back and
publish completion, while the frame's and the document's cache lines move
between cores. Over ecma262's setup the added wall time is 14.25 s for 10.8
million steals at four workers, 1.3 µs per steal, and 8.17 s for 10.1 million
at two, 0.81 µs per steal.

### The existing flags

ecma262 setup seconds, best of seven, one block started at a one-minute load
of 1.49, all builds interleaved. An earlier run of this block was discarded
because another session ran an unlocked four-worker benchmark beside it.

| `--par` build | W2 | W4 |
|---|---:|---:|
| Default (scalar-leaf limit 16, recursion budget from the pool width) | 9.170 | 16.834 |
| `--par-scalar-leaf-limit off` | 8.621 | 18.850 |
| `--par-scalar-leaf-limit 64` | 8.608 | 16.987 |
| `--par-scalar-leaf-limit 4294967295` | 7.355 | 17.629 |
| `--par-sequential-refusal` | 7.911 | 15.643 |
| `--par-recursive-frontier off` | 7.182 | 17.578 |
| `--par-recursive-frontier 4` | 8.349 | 17.560 |
| `--par-recursive-frontier 1` | 1.329 | 1.853 |

The default build's one-worker time in the same block is 0.288 s. No
scalar-leaf limit reaches the offending callees, which are outside the rule's
domain rather than above its count, and sequential refusal only changes what
the 0.3 percent of offers refused for a full deque do. A recursion budget of
one is the only flag that helps, and it helps indirectly. Only 3 of the 20.5
million setup offers are made inside a budget variant; the rest come from
ordinary helpers such as `is_addr_block_start` called from the tree
builder's insertion-mode functions, which reprocess tokens through one
another and so form a cyclic component. When a component's budget is spent,
its calls enter the sequential clones, and the whole call tree beneath makes
no offers. A budget of one spends it after one intra-component call, so most
token processing leaves the parallel world; the budgets of 7 and 8 that the
default derives for two and four workers, and a budget of 4, are rarely spent
by the tree builder's shallow reprocessing. Even at one, setup is 4.6 and 6.4
times its one-worker time, and the flag cuts every offer beneath a spent
budget, useful or not.

## Candidates

All four remove offers after checking and before emission, or refuse them at
run time. A removed or refused offer becomes the ordinary call at its original
position, which the refused edge of every offer already computes, so no
candidate can change a value, and none is read by an acceptance or proof
judgment: the program is accepted, checked and proved before any of them
runs. Each is a scratch prototype selected by an environment variable; the
unselected compiler and runtime are the baseline.

- **(a) Price a call offer as a range is priced.** Each handed-out call is
  priced by its callee's whole-call work summary, the one range pricing
  already builds: IR instructions weighted by 16 per enclosing loop, available
  counted-loop extents in place of that factor, and callees' summaries through
  their call sites, three rounds deep, so recursion is priced by the rounds.
  The summary is bound to the call's own arguments. A constant price below
  the 150,000-unit work floor removes the offer at compile time; a price that
  depends on the arguments is compared with the run-time work unit (the one
  `WF_SPLIT_WORK` sets for ranges) immediately before the lane acquisition,
  and a price below it makes the ordinary call without acquiring a lane. This
  is *(a) priced*. Three variants isolate its parts. *(a) static* compares
  only the constant three-round weight `assign_weights` already computes for
  every function, with the loop factor in place of every extent, so it
  decides every offer at compile time. *(a) static, recursion kept* also
  keeps every offer whose callee belongs to or reaches a cyclic call
  component, treating recursion as unbounded work, since the recursion budget
  already bounds how deep such offers go. *(a) priced, recursion exempt*
  keeps every offer whose callee lies in the caller's own cyclic component,
  the offers the recursion budget governs, and prices the rest.
- **(b) Offer only a callee that contains a loop or recursion.** An offer is
  kept exactly when its callee contains a loop or reaches one, or belongs to
  or reaches a cyclic call component, through any chain of calls.
- **(c) Sequential refusal by default.** `--par-sequential-refusal` as it
  exists: a refused offer enters the callee's sequential clone.
- **(d) Stop publishing from a site that does not pay.** Each lane keeps, per
  offering call site (the acquisition's return address), how many of its
  published offers were stolen and how long the stolen ones ran. After 64
  publications a site is judged, and judged again every 32 thereafter: it
  stops publishing when fewer than one offer in eight was stolen or when its
  stolen offers ran shorter on average than 4,096 timestamp-counter cycles
  (1.95 µs), about one and a half times the 1.3 µs measured wall cost of a
  steal above. A stopped site still publishes one offer in 256 so that its
  record can recover. Only the owning lane writes its table; a thief writes
  the duration into the slot it ran.

Determinism. (a), (b) and (c) are fixed functions of the checked program;
(a)'s run-time comparison is a fixed function of the call's arguments and the
configured work unit. (d) decides from observed steals and durations, so
which offers it publishes varies between runs, as which offers are stolen
already does; the values it computes do not.

Compile-time cost. (a) and (b) need, per function, the static weight that
range pricing already computes, the call graph's strongly connected
components and one closure over them; (a) priced also rebuilds the range
estimate's call summaries once more to bind them to call arguments. (c) is
the existing flag. (d) adds nothing to lowering. The measured front-end times
are under Measurements.

## Measurements

### Snowghost setup (criterion 1)

Setup seconds, best of seven, one block per page, every candidate's build
interleaved round-robin in that block. The blocks started at one-minute loads
of 1.80, 1.97 and 1.86. The sequential build and the baseline `--par` build at
one worker are in the same blocks; the baseline's two- and four-worker times
are the reproduction above.

| Build | ecma262 W1 / W2 / W4 | html5 W1 / W2 / W4 | apollo11 W1 / W2 / W4 |
|---|---|---|---|
| Sequential build | 0.252 | 0.245 | 0.070 |
| Baseline `--par` | 0.286 / 8.446 / 14.526 | 0.250 / 5.319 / 8.449 | 0.061 / 0.626 / 1.094 |
| (a) static | 0.274 / 0.270 / 0.273 | 0.218 / 0.224 / 0.226 | 0.072 / 0.072 / 0.066 |
| (a) static, recursion kept | 0.278 / 0.270 / 0.273 | 0.227 / 0.226 / 0.207 | 0.070 / 0.060 / 0.069 |
| (a) priced | 0.265 / 0.278 / 0.279 | 0.227 / 0.205 / 0.208 | 0.063 / 0.064 / 0.065 |
| (a) priced, recursion exempt | 0.275 / 0.263 / 0.278 | 0.211 / 0.222 / 0.220 | 0.062 / 0.063 / 0.069 |
| (b) loop or recursion | 0.272 / 1.285 / 1.583 | 0.228 / 0.677 / 1.059 | 0.061 / 0.148 / 0.175 |
| (c) sequential refusal | 0.277 / 8.603 / 10.865 | 0.215 / 4.815 / 10.029 | 0.062 / 0.563 / 1.096 |
| (d) adaptive | 0.261 / 0.444 / 0.568 | 0.231 / 0.310 / 0.376 | 0.070 / 0.077 / 0.106 |

Every build printed the same shape-C checksum on apollo11 at four workers
(`1d5b5dbb01aac0a0`), so the policies changed no result.

Against the recorded allowance (the larger of 5 percent of the one-worker
time and 10 ms), the two static variants of (a) meet criterion 1 on all three
pages. The priced variant misses it by one cell, ecma262 at four workers,
0.279 s against 0.265 s plus 13.3 ms, and the priced, recursion-exempt variant
by one cell, html5 at two workers, 0.222 s against 0.211 s plus 10.6 ms. Both
misses are under a millisecond, while the one-worker cells of the eight
builds, which all run the same sequential world, span 0.261 to 0.286 s on
ecma262, 0.211 to 0.250 s on html5 and 0.061 to 0.072 s on apollo11 in the
same blocks. The allowance was smaller than this host's spread between
identical configurations, so criterion 1 does not separate the four (a)
variants from one another; it does separate all of them, with four-worker to
one-worker ratios of 0.91 to 1.11, from the baseline, whose ratios in the
reproduction are 52, 37 and 15.

(b) keeps the 72 offers whose callees reach a loop, among them the tokenizer's
`truncate_bytes`, an uncounted loop that usually runs zero or one trip:
setup at four workers is still 5.8, 4.6 and 2.9 times its one-worker time. (c)
changes only what a deque-full refusal does, which is 59 thousand of 20.6
million ecma262 offers, and setup is as slow as the baseline's. (d) recovers
most of the loss but not all of it: at four workers setup is 2.2, 1.6 and 1.5
times its one-worker time. Every offer still calls the acquisition and looks
up its site, and each site publishes its first 64 offers per lane before it
is judged; how many offers it still publishes was not measured.

### The maintained parallel programs (criterion 2)

The candidates' `--par` emission of every program under `tests/programs` and
of par-quicksort was compared byte for byte with the baseline's. The call
offers of those programs are of three kinds: recursive calls inside their own
cyclic component (quadrature's `adaptive`, merge sort's `sort_values` and
`merge_values`, `par_layout`'s `build`, `layout` and `layout_banded`, the
parallel tests' `fold` and `spine`, `range_split`'s `fill_recursive`,
quicksort); calls of a function that contains a range split (radix scatter's
`copy_run` and `copy_values`, `range_fold`'s `folded`), whose callee reaches
the splitter's own recursion; and calls of small constructors and helpers in
correctness tests. The halves a range splitter publishes are priced by the
split budget. (a), (b) and (c) leave them unchanged (the emission of every
kernel whose only offers are splitter halves is byte-identical under each),
while (d) applies to every offer. Programs whose emission changes:

| Candidate | Programs whose emission changes | Formal kernels among them |
|---|---:|---|
| (a) static | 32 | quadrature |
| (a) static, recursion kept | 26 | none |
| (a) priced | 33 | quadrature |
| (a) priced, recursion exempt | 27 | none |
| (b) loop or recursion | 23 | none |
| (c) sequential refusal | 12 | quadrature |
| (d) adaptive | every program with an offer (the runtime changes) | all five |

The formal kernels whose emission or runtime changed ran through
`tests/performance/compare.sh` against the baseline images, after the
identical-image control (baseline images as both arms), all in one session
started at a one-minute load of 1.19 to 1.79. Paired wall ratios are
baseline over candidate, so a ratio below one is a slower candidate:

| Arm | Verdict | Adverse widths |
|---|---|---|
| Identical-image control | PASS | none; suspects fir W1 0.793, quadrature W1 0.897, stencil W4 0.899 |
| (a) static | FAIL | quadrature W2 0.576 and W4 0.437, five of five pairs slower |
| (a) priced | FAIL | quadrature W2 0.551 and W4 0.416; records W1 0.857 and W2 0.951 |
| (c) sequential refusal | PASS | none; suspects records W4 0.907, stencil W1 0.942 |
| (d) adaptive | FAIL | quadrature W2 0.589 and W4 0.427; fir W2 0.826 and W4 0.918 |

Pricing quadrature's recursive call by the summary rounds puts it far below
the work unit, so both unexempted variants of (a) remove its offer, and the
kernel loses its parallel speedup: 2.3 and 2.4 times slower at four workers.
(d) loses the same speedup, consistent with its per-site rule stopping that
one recursive offer site, whose offers below the first levels are mostly
popped back by their own lane. The priced arm's records regression comes with an
unchanged records module: that arm's runtime objects carry the priced
acquisition function, which records never calls, so the difference is code
placement, not policy. (a) static with recursion kept, (a) priced with
recursion exempt, and (b) leave all five kernels byte-identical to the
baseline.

The other programs whose emission changes ran as whole processes, best of
seven, in one block started at a one-minute load of 1.30, each baseline image
beside an identical copy of itself as a control. The block is **inconclusive
under its recorded rule**: the control differed from the baseline beyond the
allowance in one cell (`sha256_abc` at W2, 0.052 s against 0.071 s), and the
rerun did not take place before the session ended. What it observed, without
qualifying anything: under (a) static with recursion kept, (a) priced, (a)
priced with recursion exempt, (b) and (c), no cell exceeded the baseline by
more than the allowance; under (a) static and (d), `adaptive_quadrature` at
W2 did (0.044 s against 0.055 and 0.059 s), the same recursive offer the
formal quadrature kernel loses. Two maintained programs already slow down
with workers under the baseline, as Snowghost's setup does: `sha256_abc`
takes 0.002, 0.052 and 0.141 s at one, two and four workers and `dir_walk`
0.003, 0.005 and 0.013 s, while every (a) variant runs both in 0.002 to
0.003 s at every width. The merge-sort oracle process, a correctness matrix
of small sorts, read 0.030 to 0.037 s in every arm.

Not measured: par-quicksort, because main's copy of the program no longer
compiles (`quicksort.wf:2:19: error[GRAM-5]`, the retired `deref(v)`
spelling); the radix-scatter oracle, whose link failed and was not
diagnosed; and `wfgrep`, whose `--par` build stopped in the backend on main.
The implementation results below measure par-quicksort and `wfgrep` on a
main that has both fixes.

### Lowering cost (criterion 3)

Not measured to the recorded standard. The seven-run comparison was
scheduled but did not run. Each candidate's prototype build ran once, inside
a locked session, and reported front-end times (checking, lowering and
emission) of 22,171 ms for the baseline, 22,127 ms for (a) static, 21,912 ms
for (a) static with recursion kept, 21,574 ms for (a) priced, 22,550 ms for
(a) priced with recursion exempt, 22,467 ms for (b) and 22,009 ms for (d);
the flag builds ranged from 21,541 to 22,778 ms. These single builds differ
from the baseline's by at most 3 percent in either direction, but they are
not best-of-seven cells.

### Style shapes

Not measured per candidate. A sizing pass with the baseline build only, one
run each on apollo11 with one repetition (so each time includes setup): shape
A 4.057 s at one worker and 2.583 s at four, B 3.752 and 5.415 s, C 4.010 and
2.538 s, the intern pass 3.842 and 2.433 s. A and C split their counted loops
(`style_level`, `match_elements`); the instrumented build registers no call
offer for `style_run` or `style_subtree`, so shape B's halving makes no offer
under this compiler, and its four-worker time is its sequential work plus the
per-element offers of the helpers it calls, `byte_eq`'s among them. Whether
the candidates change the shapes' stage times remains open.

## Status

Measured: the reproduction, the attribution, the existing flags, every
candidate on the setup of the three real pages, the emission of every
maintained program under every compile-time candidate, and the five formal
kernels with the identical-image control. Inconclusive: the other maintained
programs, whose block's control failed. Not measured: par-quicksort, the
radix-scatter oracle, `wfgrep`, the lowering-cost comparison, and the style
shapes per candidate.

The recommended rule's implementation was then measured against every part
of the criterion; see [Implementation results](#implementation-results).

## Recommendation

One candidate meets every part of the criterion that was measured: **(a)
static, recursion kept**. Offer a statement-group call only when its callee
belongs to or reaches a cyclic call component, or when the callee's static
work summary (the three-round weighted instruction count range pricing
already computes) reaches the 150,000-unit runtime work unit; run every other
call as the ordinary call it already is on the refused edge. On the three real
pages its setup at two and four workers stays within the allowance of its
one-worker time, against 15 to 52 times slower under the baseline, and it leaves
the five formal kernels byte-identical to the baseline.

Why this shape. A range split is priced by the split budget, and a recursive
component's offers are bounded by the recursion budget, but nothing prices
the remaining call offers, and those are the ones that swamp the prototype:
99 percent of its setup offers call a callee of static weight at most 100.
A static summary cannot bound recursion, so this rule treats recursion as
unbounded and leaves its depth to the recursion budget, which is what keeps
quadrature's and the other recursive programs' speedups. Everything else must
reach the work unit ranges already use. It is decided at compile time, reads
only the IR after checking, adds nothing to an offer at run time, subsumes the
scalar-leaf limit, and changes no value: a removed offer is the ordinary call
the refused edge already makes. The priced variants were not better on any
measured page, and the unexempted variants remove quadrature's recursive
offer.

The implementation then met every part of the criterion, with the lowering
cost taken over three runs as the owner directed
([Implementation results](#implementation-results)). Its known limit, not exercised by any measured
program: a non-recursive helper whose work is large only through its runtime
extents, such as a single loop over a large argument, weighs 16 times its
body statically and loses its offer, which the priced variant would keep; and
a cheap call that enters a recursive component keeps its offer however often
it runs.

## Rejected alternatives

- (a) static and (a) priced without a recursion exemption: rejected because
  they remove quadrature's recursive offer, so the formal quadrature kernel
  runs 2.3 to 2.4 times slower at four workers and fails the regression rule.
- (b) offer only a callee that reaches a loop or recursion: rejected because
  a loop is not work, and uncounted loops that usually run zero or one trip
  keep 72 offer sites; setup stays 2.9 to 5.8 times slower at four workers.
- (c) sequential refusal by default: rejected because it acts only on the
  0.3 percent of offers refused for a full deque and leaves setup as slow as
  the baseline.
- (d) stop publishing from a site that does not pay: rejected because it
  leaves setup 1.5 to 2.2 times slower at four workers, fails the formal
  regression rule on quadrature and FIR, charges every offer a site lookup,
  and makes which offers are published depend on the run.
- A larger scalar-leaf limit: rejected because the offending callees take
  aggregates or branch, which is outside the rule's domain; no limit value
  changes setup.
- A recursion budget of one: rejected because it helps only by cutting whole
  call trees into the sequential clones, leaves setup 4.6 and 6.4 times
  slower at two and four workers, and cuts useful recursive offers with the
  useless ones.

## Validation criterion for an implementation

- With the new rule, `--par-ledger` names every removed offer with its
  callee's weight or its recursion, in place of the scalar-leaf lines, and an
  override flag restores every permitted offer for tests of the offer path.
- The `--par` emission of the formal kernels and of every recursive program
  (quadrature, merge sort, `par_layout`, the parallel tests, `range_split`,
  par-quicksort) is byte-identical to main's.
- Snowghost's setup on the three real pages meets criterion 1 above.
- `tests/performance/compare.sh` passes against main after a passing
  identical-image control, and the other maintained programs meet
  criterion 2 in a block whose control passes.
- The prototype's `--par` front end is within 5 percent of main's, best of
  seven.
- Existing tests that assert scalar-leaf ledger lines or rely on a small
  offered pair are updated to the new rule or to the override, each with its
  reason; none is deleted to pass.

## Implementation results

The recommended rule is implemented in `compiler/src/lowering/builder/call_grain.rs`
(`b4917910`), in place of the scalar-leaf filter. `--par` keeps a statement-group
call offer only when its callee belongs to or reaches a cyclic call component or
its static weight (the `assign_weights` total) reaches 150,000; `--par-call-grain
off` offers every permitted call, and `--par-ledger` names each omitted offer
with its callee's static work. The baseline is main at `6259db68`. Both
compilers are gate builds on the investigation's host (four CPUs, Intel Xeon at
2.10 GHz), and every block below ran under the verification lock.

**Emission.** Of the 90 `--par` builds compared (every `tests/programs` source
outside `windows/`, and par-quicksort), 55 are byte-identical to main's, 27
differ and 8 fail on both compilers alike (the multi-source container units and
the deflate programs, which do not compile as single files). The five formal
kernels, merge sort, `range_split`, `par_layout`, adaptive quadrature and
par-quicksort are byte-identical. The implementation criterion above predicted
byte identity for the parallel tests too; that does not hold for the whole
modules of `parallel/tree.wf`, `parallel/window.wf` and `recursive_tree.wf`,
whose non-recursive fan-out helpers (`leaf`, `pair`, `quad`, `oct`, `branch`,
`boxed_leaf`, static work 3 to 61) lose their offers while the recursive `fold`
and `spine` offers stay. They are therefore timed under criterion 2 below.

**Criterion 1: met.** Setup T(0), best of seven, in one block started at a
one-minute load of 0.75, the three arms interleaved:

| Page | Arm | W1 | W2 | W4 |
|---|---|---:|---:|---:|
| ecma262 | call grain | 0.280 | 0.284 | 0.280 |
| ecma262 | main | 0.280 | 7.455 | 10.434 |
| ecma262 | main, identical copy | 0.283 | 7.706 | 10.312 |
| html5 | call grain | 0.223 | 0.226 | 0.223 |
| html5 | main | 0.221 | 4.460 | 5.813 |
| html5 | main, identical copy | 0.222 | 4.559 | 5.975 |
| apollo11 | call grain | 0.057 | 0.057 | 0.059 |
| apollo11 | main | 0.058 | 0.577 | 0.725 |
| apollo11 | main, identical copy | 0.058 | 0.576 | 0.733 |

Every call-grain cell at two and four workers is within 5 percent or 10 ms of
its own one-worker time; main is 37 times slower at four workers on ecma262.
The prototype's `check` mode passes on every page at one and four workers, with
the same checksums as main's build.

**Criterion 2: met.** The five formal kernels are byte-identical, so they need
no timing; the hosted `compute regression` job on `b4917910`, which runs
`tests/performance/compare.sh` against the merge base after its identical-image
control, passed. The 25 changed programs that run without a network peer were
timed as whole processes, best of seven, at one, two and four workers, beside an
identical copy of main's image. The first block is void (`wfgrep` was given an
absolute root, which it refuses). The second is inconclusive under the recorded
rule: the control read 0.057 s against main's 0.092 s for `sha256_abc` at four
workers, main's own variance there. The third and fourth blocks, started at
one-minute loads of 0.74 and 1.09, are clean: every control cell within the
allowance of main's, and no call-grain cell slower than main's by more than the
allowance. From the third block:

| Program | main W1 / W2 / W4 | call grain W1 / W2 / W4 |
|---|---|---|
| `sha256_abc` | 0.0014 / 0.0660 / 0.0895 | 0.0014 / 0.0020 / 0.0020 |
| `dir_walk` | 0.0041 / 0.0171 / 0.0236 | 0.0041 / 0.0040 / 0.0039 |
| `wfgrep fn ../compiler/src` | 0.0221 / 0.0270 / 0.0304 | 0.0224 / 0.0222 / 0.0221 |
| `radix_scatter` | 0.0017 / 0.0062 / 0.0086 | 0.0018 / 0.0020 / 0.0029 |
| `recursive_tree` | 0.0015 / 0.0017 / 0.0022 | 0.0015 / 0.0014 / 0.0014 |

`tcp_gather.wf` and `tcp_refused.wf` changed and were not timed: they need a
network peer the block does not provide.

**Criterion 3: met, on three runs.** The front-end time `--report` prints for
the prototype's `--par` build, alternating the two compilers, read 97,288,
95,349 and 97,775 ms for main and 97,301, 95,155 and 97,491 ms for the call
grain; the bests are 95,349 and 95,155 ms, 0.2 percent apart. The owner cut
the recorded seven runs to three on 2026-09-29, since the rule's own work (one
call-graph decomposition and one pass over each function's groups) is small
beside the 95-second check that both compilers share.

## The design-tree node it would change

`design/compiler/parallel-lowering.md`: the decision that excludes scalar
leaves with at most 16 operations from compute offers, which the rule
replaces, and the decision that no universal grain policy is selected, whose
grounds this measurement changes for call offers. Sequential refusal stays
opt-in, and `parallel-lowering/parallel-runtime.md` and
`parallel-lowering/two-worlds.md` are unchanged by the grain. The owner
approved the revision, which now stands in the live node; the recursion
budget's revision of `two-worlds.md` follows in
[the recursion budget at splits](#the-recursion-budget-at-splits).

## The recursion budget at splits

With the grain in place, Snowghost's style shape B (a preorder recursion that
styles an element and then halves the run of its children) reached 1.04
times at four workers on ecma262 and apollo11 and 2.57 on html5, against
3.76 and 3.81 on the first two with `--par-recursive-frontier off`
([Snowghost concurrency preview](https://github.com/mbbill/Snowghost/blob/9a6b78e/research/investigations/concurrency/DESIGN.md#preview-with-the-call-grain)).
The budget of `design/compiler/parallel-lowering/two-worlds.md`, about eight
levels at four workers, was decremented by every call into the recursive
component, so the one-child descents of a deep document spent it before the
wide runs below them. The change decrements it only at calls in an
actualized statement group, the only calls that offer; a call outside every
group passes the caller's levels on. The criterion was recorded in
`docs/todo.md` before the change: shape B on the three pages must improve,
and the formal kernels' recursive offers must keep their times.

Emission. Of 82 programs (the maintained programs under `tests/programs`
and the parallel quicksort experiment) built with `--par` before and after
the change, it alters one module: `tests/programs/compute/radix_scatter.wf`, whose
`pack_chunks` reaches `pack_payload` through a match arm outside every group
and now passes its levels on. Its timing is the compute-regression job's
comparison on the PR. `only_a_group_member_spends_a_recursion_budget_level`
in `compiler/src/backend/tests/parallel.rs` fails under the former rule.

Shape B, with the prototype of Snowghost commit 9a6b78e (parent by
reference), best of three at one and four workers, stage time
(T(2) - T(0)) / 2, under the check lock:

| Page | Shape | W1 (s) | W4 (s) | Speedup | Before |
|---|---|---:|---:|---:|---:|
| ecma262 | A | 2.882 | 0.843 | 3.42 | |
| ecma262 | B | 2.960 | 0.954 | 3.10 | 1.04 |
| ecma262 | C | 2.995 | 0.802 | 3.73 | |
| html5 | A | 3.789 | 1.057 | 3.58 | |
| html5 | B | 4.043 | 1.103 | 3.67 | 2.57 |
| html5 | C | 3.956 | 1.058 | 3.74 | |
| apollo11 | A | 1.722 | 0.489 | 3.52 | |
| apollo11 | B | 1.795 | 1.574 | 1.14 | 1.04 |
| apollo11 | C | 1.732 | 0.453 | 3.82 | |

Shapes A and C have no recursive offers and match the earlier preview within
its spread. Apollo11's B stays sequential for a second reason: its work sits
under a few children of wide runs, so the halvings above it are real splits
that spend the budget. Whole runs (setup included, which is under 0.08 s
here) with four repetitions, best of two: at four workers they took 6.65 s
with the default budget, 3.49 s with `--par-recursive-frontier 16`, 1.88 s
with 24 and 1.93 s with the budget off, against 7.07 to 7.23 s at one
worker, whole-run ratios of about 1.1, 2.1, 3.8 and 3.7. A fixed depth cannot follow an
unbalanced tree; that is a separate budget question, recorded in
`docs/todo.md`.

## Found along the way

- `whitefootc --par tests/programs/wfgrep.wf` stopped on main with a backend
  `InvalidIr` failure; the build without `--par` succeeded. An instrumented
  build registered three offers in `name_before` before the failure. Fixed on
  main by PR #182 before the implementation was measured.
- `research/experiments/par-quicksort/quicksort.wf` no longer compiled on
  main: it still spelled dereference `deref(v)`. Fixed on main by PR #183
  before the implementation was measured.

## Synthesized split reachability: prospective causal probe

Question: does a source-nonrecursive list comparison retain a cheap call offer
only because its permitted loop lowers to a recursive splitter? The experiment
compares the pinned compiler at `3629be153b8fdb792ffe146d21dfc3251e4f887a`
with the same compiler whose call-grain recursion graph follows a `LoopSplit`
only to its chunk. Ordinary calls, including source recursion inside chunks,
remain edges. Generated splitters retain their self-calls and range allowance.
This is an experimental compiler change, not selection of a default policy.
The existing static cost estimator already follows the chunk for loop work;
the IR operation supplies provenance without testing generated symbol names.

The independent work is an outer map with two list comparisons per element.
Each comparison reduces independent equality flags with Boolean conjunction.
Neither comparison depends on the other; only the element's output depends on
both. A last-element mismatch, lengths 0, 1, 8 and 4096, and a source-recursive
callee reached from a loop chunk provide correctness and classification
controls. The research workload `split-reachability.wf` belongs beside this
record, is called explicitly by this probe, and is removed if it ceases to
reproduce this question and its retained evidence no longer needs it. Raw
commands, failed trials, compiler/source hashes and outputs live in ignored
`compiler/target/call-grain-probe/`; retained results will identify their surviving bundle.

Before builds or timings, the discriminating criteria are:

1. Both compilers accept the identical source and produce every independently
   calculated flag. Deliberately wrong expected flags must fail the oracle.
2. The baseline retains comparison offers through generated recursion; the
   candidate omits them below 150000 static units. Source recursion reached
   through a chunk retains exemption, and generated range splitting remains.
3. Permission evidence and non-parallel LLVM emission are identical. A changed
   permission, proof, acceptance or sequential module invalidates this probe.
4. After mechanism confirmation, three interleaved timing blocks compare
   baseline, candidate and a second copy of baseline, sequential and four
   workers. A control difference exceeding 5 percent or 10 ms (whichever is
   larger) makes a block inconclusive. A causal speed claim requires candidate
   improvement exceeding that allowance in each short-list block. Long-list
   results bound the tradeoff; they do not establish a general policy.

The hypothesis is refuted for this workload if the baseline does not retain
these offers or the candidate does not remove them. Unchanged runtime despite
verified offer removal refutes scheduling cost as the measured cause for this
workload. Compilation failure is an untested mechanism, not a runtime result.
No Snowghost source or submodule pin changes, runtime constants, permission
rules, specification edits, or proof changes are part of the A/B comparison.
The test host is Darwin 25.6.0 arm64; further machine, compiler and environment
details are recorded with the run. Every build and run uses the host-wide
check lock, after the separately coordinated renderer measurements finish.

Initial probe qualification passed with identical sequential LLVM and identical
permission/split ledger, but its runtime harness mistakenly labelled an unset
`WF_WORKERS` as sequential. The runtime defaults that setting to online CPUs;
those observations are auto-worker observations, not sequential evidence. The
raw first batch is retained unchanged. Its four-worker short-list differences
were below the prespecified 10 ms allowance. Before a second batch, enlarge
only the number of independent rows from 100000 to 1000000 for lengths 0, 1
and 8, retain 512 rows for length 4096, and explicitly set `WF_WORKERS=1` or
`4`. Keep the same three-block, three-interleaved-samples-per-cell median
summary and the original allowance. This tests scale above the instrument's
allowance rather than relaxing the criterion. Qualification's runtime checks
are repeated with explicit worker settings. Both batches include allocation,
initialization, comparisons and output checks in process wall time; they do
not isolate the comparison's own elapsed time.

### Split-reachability probe result

The mechanism is demonstrated on the standalone workload. The pinned compiler
retains the `equal_list` call offer in the outer map's chunk. With the sole
compiler change in `5f682db71127710bc60bc93f6770ff0113967f27`, the ledger
omits that offer at static work 119, below 150000. Both compilers retain the
outer map, the inner Boolean reduction split, and the recursive-loop split.
The candidate still publishes a `recursive_loop` call whose chunk reaches
`tree`, and `tree` still has its recursion-budget family. LLVM contains the
range splitters' publication sites in both images. Thus this does not suppress
all loop-containing callees or source recursion hidden inside a chunk.

Explicit `WF_WORKERS=1` and `4` qualification passed 22 build/run checks:
ordinary outputs succeeded; both independent wrong-expectation mutants
(empty-list flag and nonempty-list flag) failed with exit 1 on both compilers
and both worker settings. The permission and split ledger lines match exactly.
Non-parallel LLVM matches byte for byte, SHA-256
`fc50d4afd1fd47bf45dd2437a0c59686f3693fedbb17451974d92af6ec19d2ca`.
The earlier 20-check batch used auto workers and four workers and is retained
as such, not relabelled as sequential.

The confirmation used macOS 26.6.2, Darwin 25.6.0 arm64, eight online CPUs,
Rust 1.98.1, gate-profile compilers, and native `--par` executables. It ran
under the common check lock. Each of three blocks ran each image three times,
rotating baseline/candidate/byte-identical-baseline-copy order. The table gives
the range of the three block medians, milliseconds of complete process wall
time. Each short-list case has 1000000 independent rows; length 4096 has 512.

| List length | Workers | Baseline median range | Candidate median range | Criterion |
|---|---|---|---|---|
| 0 | 1 | 4.152–6.011 | 4.189–7.287 | within allowance |
| 0 | 4 | 71.229–73.194 | 6.455–6.533 | improves in every block |
| 1 | 1 | 8.319–11.865 | 8.357–12.563 | within allowance |
| 1 | 4 | 70.651–73.037 | 9.391–14.732 | improves in every block |
| 8 | 1 | 34.061–34.751 | 33.472–37.221 | within allowance |
| 8 | 4 | 89.095–91.810 | 30.724–32.334 | improves in every block |
| 4096 | 1 | 10.132–13.093 | 10.101–14.032 | within allowance |
| 4096 | 4 | 9.877–10.263 | 9.576–10.281 | within allowance |

All 24 confirmation cells satisfy the identical-image control allowance. All
nine short-list four-worker cells improve beyond it. The 216 initial and 216
confirmation runtime observations, including statuses and load, are retained
in [split-reachability-results.tsv](split-reachability-results.tsv). Load was
not independently controlled beyond the shared lock and the interleaved copy
control; the older investigation's Linux background-load admission threshold
is not used for this separately preregistered host comparison. The difference
attributes a substantial short-list cost to the compiler change on this
workload. It does not establish the fraction of Snowghost's measured delta
time caused by these offers, a broad regression-free default policy, or the
long-list performance boundary.

Reproduction uses `split-reachability.wf` for qualification. For each timing
case keep all declarations before `fn main()`, replace the two `100000_u64`
contract bounds by `1000000_u64`, and use a main that calls only
`check_case(width: WIDTH_u64, count: ROWS_u64)` and returns exit 0 on True,
1 on False. Build the same source with each compiler using
`whitefootc --par --par-ledger -o IMAGE SOURCE`; copy the baseline image
byte-for-byte for the control. Set `WF_WORKERS` explicitly to 1 or 4. The
confirmation source SHA-256 values, in increasing width order, are:

- 0: `6149493e1b3f4dc88d50b2cbbfb180e14680e0e62a8b9e7cd8e1b380fbd68db1`
- 1: `ed7f7e8351df1d2608bcb0148cf30a3a877c798709a6eaf592fd23f460e4fb1a`
- 8: `a6c83929494c80fa056534a3008bffe97a3307a3b6d098543162ccf96797febc`
- 4096: `fa2ab728db6f66a6f78b999c60db184070d3267b6f022f0f25f538936c3ac955`

The baseline compiler SHA-256 is
`3b1ff1c4292e95fc38557a94764dd9f13858aaf29ea4a62db9f8ba1557018971`;
the candidate is
`9b8e767a341ad82148b59c2dc93342efb0144f4cadcdbd1b8a4e8f0e38cf182e`.
Local command/status records, full stdout/stderr, emitted LLVM, both source
variants, binaries and failed initial compilations remain under
`compiler/target/call-grain-probe/` and
`compiler/target/call-grain-confirmation/`. Initial failures were canonical
syntax, an unresolved `Unit` spelling and a non-affine invariant. Runtime
guards now check slice endpoints during map/setup; these checks are identical
in A and B. No failed compile is counted as a timing observation.

`make static` passed on `e13ec4336` before the result record was appended,
and the changed Rust file passes `rustfmt --check`. The full repository gate,
maintained performance kernels, cross-target behavior, a default-policy
design change and independent review are not established by this probe. No
source-language, permission, proof, runtime constant or Snowghost pin changed.
The new result TSV serves this causal comparison and is removed only when
its evidence is superseded and no longer cited.
