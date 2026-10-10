# Cold compile speed of a real project

Snowghost, the browser engine written in Whitefoot, took minutes to build
from a clean cache. A change to one renderer module waited on a check of the
whole entry, and several compiler processes at once filled a ten-core
machine. This investigation asks where a cold build of a real project spends
its time and which compiler changes remove that time without changing a
verdict, a diagnostic or the emitted program.

## Goal

The goal is a cold `--check` of Snowghost's largest entries in seconds rather
than minutes on the owner's ten-core machine, with the same verdicts and
byte-identical LLVM. Incremental rebuilds already reuse module verdicts and
proof receipts (Snowghost PR #36 turned the cache on); this investigation is
about the cold path, which every compiler upgrade and every fresh checkout
pays.

## Method

Workload: Snowghost `29f89af` (PR #36 merged), whose `renderer/modules.wfg`
graph has 52 modules and 26 entries. The measured entries are `style_oracle`
and `layout_oracle`, the two largest; `png_oracle`, `css_selectors_oracle`
and `html_tree_oracle` join them for the LLVM comparison.

Host: a 4-processor Intel Xeon at 2.10 GHz with 15 GiB of memory, Linux
6.18, Rust 1.97.0, the `gate` profile. The owner's machine has ten
processors, so concurrency gains here understate gains there.

Commands, from `renderer/`, with a fresh cache directory per run:

```sh
whitefootc --cache "$CACHE" --check --graph modules.wfg --entry style_oracle
whitefootc --cache "$CACHE" --check --graph modules.wfg --check-module pkg::html::tree_builder
whitefootc --cache "$CACHE" --emit-llvm --graph modules.wfg --entry E -o E.ll
```

Wall, user time and peak RSS come from `/usr/bin/time`. Samples come from
`samply record` on a build with frame pointers and line tables, symbolized
with inlined frames. Each number below is one run. Three repeated checks of
`pkg::html::tree_builder` with the final compiler took 12.24, 12.98 and
12.83 s, a spread of 6%; every reduction that selected a candidate is at
least 1.2x.

Baseline compiler: main `c3d26643`, the revision Snowghost pins.

## Baseline

| Entry | No cache: wall | front end | link | peak RSS | Fresh cache `--check`: wall | peak RSS |
|---|---:|---:|---:|---:|---:|---:|
| `style_oracle` | 190.5 s | 172.5 s | 17.9 s | 3.9 GB | 95.5 s | 2.5 GB |
| `layout_oracle` | 481.3 s | 451.7 s | 29.5 s | 9.1 GB | 274.6 s | 2.9 GB |

The fresh-cache column is the build Snowghost now runs. It halves the
uncached time because an entry composition reuses the proof receipts its
module verdicts recorded moments earlier (`style_oracle`: 1957 analyses
reused, 1702 recorded); without a cache every function body is analyzed
twice, once for its module verdict and once for the composition.

## Attribution

A sample of the fresh-cache `style_oracle` check puts 61% of samples in
function proof analysis (`analyze_candidate_inner`), 31% in L0 closure
(`close`), 23% in pre-kill materialization, and about 12% in name
resolution's `build_tables`, which scanned every terminal of the bundle once
per declaration to find `public` and function bodies. Postcondition
preflight scanned every lexical use of the unit once per node to find the
uses inside a subtree (about 8%), and a fresh bound store probed its
extra-candidate map once per stored cell (about 6%).

The timeline of the threads shows a second structure. The entry check walked
its modules' verdicts one after another, so one processor did all the work.

### Concurrent module verdicts

With the scans removed and the verdicts computed concurrently, the fresh-cache
`style_oracle` check took 58.5 s wall and 83.1 s user on four processors.
The thread timeline then showed one worker busy for 47 s while the others had
finished: the verdict of `pkg::html::tree_builder`, a 41.4 s module check on
its own, was the critical path. Module checks of `pkg::style` took 22.6 s;
`pkg::css::values` 3.1 s; the other measured modules under 2 s.

Timing each function analysis of `pkg::html::tree_builder` (a temporary
print, not retained) found 39.2 s spread over 335 functions, of which three
took 38.8 s: `adjust_svg_attr_name` 21.1 s, `public_id_starts_quirks` 14.1 s
and `adjust_svg_tag_name` 3.2 s. In `pkg::style`, `intern_styles` took
14.5 s and `inherited_pass` 2.3 s. Each slow function is straight-line code
over many constant arrays; `public_id_starts_quirks` is 55 calls of the form
`bytes_starts_with_ci(text: text, prefix: &lit_k[0_u64..lit_k.len])`. Every
array literal adds terms whose standing values relate them to every other
term through zero, so the closed matrix of these functions is nearly full,
with up to 480 terms.

Per-function concurrency would not shorten this critical path: one function
is most of the module.

### Unrecorded states

Counting the closure routes of the `pkg::html::tree_builder` check (a
temporary print) gave 4556 closures, of which 1121 ran the complete
unseeded fixed point because the state had no closure record. Weighting each
closure by its asymptotic cost (n³ for the fixed point, n² for edge
insertion) puts an estimated 95% of the closure cost in those 1121; this is
a weighting of route counts, not a measured time. A backtrace of the
large ones (n from 254 to 356) shows `prove_bounded_relation` closing the
function's ordinary flow state, which carried only 76 to 84 bounds: a state
that has never been snapshotted has no record, every statement registers six
or seven new terms, so the remembered closed view misses on its term count,
and the whole closure is recomputed at each proof.

### Implicit-only snapshots

With unrecorded states closed from their remembered view, the same module
check took 23.2 s, and 48% of its samples were materialization: each
snapshot, at a kill or a Result refresh, interns a `MaterializedBound` node
for each of the nearly n² closed cells. Most of those cells are implied by
term types, constant values and standing measure facts alone, the [ENT-2]
implicit bounds, through closure rules. Such a conclusion holds at every
program point, so wrapping it at a snapshot event adds a node and records
nothing a kill or a join could use.

### Renamed symbolic instances

Halo, the Lua engine on branch `claude/halo-slice1`, is a second workload.
Its `research/experiments/halo-e2e` entry `test` exhausted memory on the
baseline-plus-candidates compiler (killed after 151.9 s at 13.9 GB). The
module check of `pkg::vm` alone reached a 12 GB limit after 131 s and 4134
function analyses, during symbolic generic validation. Every interpreter
helper takes `interface Host<E>`, and each generic template's symbolic
validation instantiates the helpers it reaches at its own symbolic
parameter: `slow` was analyzed 40 times (31.5 s), `library_base` 39 times
(18.2 s) and `prepare` 40 times (14.3 s). Those instances differ only by a
renaming of symbolic parameters, and their callers read only their summaries.

### Terms without facts

After candidate 5 the slowest Halo function, `library_builtin` (a generated
table of about a hundred `if index == k { ... return ... }` arms, each
filling a local array), took 12.9 s in its symbolic and again in its concrete
analysis. Every arm registers new terms, and a term stays a row of every
matrix after its scope exits, so the width only grows. A temporary count over
Halo's `pkg::vm` check found, in closures wider than 300 terms, up to 879
terms of which only 263 have any row other than zero's shifted by their
implicit bounds; the squared widths differ by 11.3x. In Snowghost's
`pkg::style` the same count gives 253 terms and 180 such rows at the median
of the widest closures, a 2x difference.

### Dormant terms

After candidate 6, `library_builtin` still took 9.3 s in its concrete and
9.6 s in its symbolic analysis, measured by a temporary per-function timer.
A generated function of the same shape, N arms of
`if index == k { let a = array_filled(...); set a[0..4] = ...; return Some(Row(...)); }`,
reproduces it: 0.8 s, 4.2 s and 28.1 s for 25, 50 and 100 arms, roughly
cubic. A temporary trace of each pre-kill snapshot showed why. A snapshot
stores every closed cell, including those its endpoints' implicit bounds
imply through zero, such as `index - a.len <= -31` from `index <= 1` and
`a.len == 32`. The closure universe admitted every endpoint of a stored
cell, and every endpoint of an implicit edge between two nonzero terms, such
as `a.len <= a.cap` of every array the body ever named. So the next snapshot
stored those terms' cells again: at the sixth arm of a six-arm function, a
snapshot closed 48 of the 79 registered terms, where the index, the arm's
literals and the arm's own locals are all that facts reach.

With the universe narrowed (candidate 7), the arrays laid out over every
registered term dominated instead. At 100 arms a closure's matrix was
allocated over about 1,200 terms for a universe of about 18, and the fact
state's store, laid out the same way, was 30 MB per copy-on-write copy; in
samples, `libc` (allocation and copying) took 48% and the store's cell scan
28%. With both laid out over their own terms, the remaining per-closure work
over every registered term took over: recomputing each term's implicit bounds
(21% in `passive_bounds`, 15% in `closure_middle_terms`).

## Candidates and selection

Each candidate changes work only. The selection criteria are that every
verdict stays the same, the LLVM of the five entries is byte-identical to the
baseline's, the library unit tests and the corpus pass, the closure routes
agree with the complete closure under the existing seeded-closure
verification, and the stage the candidate targets gets faster by at least
1.2x on this host. Candidates 1 and 2 were first timed in exploratory runs,
before these criteria were written; their results below are the same runs.

1. **Indexed lookups.** Resolution gathers the writers of `public` and `{` in
   one pass over the terminals; the resolved unit keeps lexical uses sorted
   by origin path, so the uses inside a subtree are one contiguous range,
   returned in record order; a fresh bound store skips the extra-candidate
   probe. Routine fixes under unchanged design.
2. **Concurrent module verdicts.** An entry composition computes its module
   verdicts on one thread per processor and
   reports the first rejection or failure in module order. Unlike the
   sequential walk, every module is checked even after an earlier one
   rejects, and a panic in any of them propagates. The build cache's
   counters and settled-verdict map become thread-safe. Candidates 1 and 2
   were measured only together.
3. **View seeds.** A state without a closure record keeps its remembered
   closed view while it gains only bound candidates or signed goals; the next
   closure widens the view to the current terms and inserts the bounds that
   became strictly smaller and every later term's implicit bounds, as edge
   insertion closes a recorded core. A kill, a candidate removal, a new
   disequality or a replaced standing measure fact ends the seed.
4. **Implicit-only snapshots.** The derivation ledger records, at interning,
   whether a proof rests on implicit bounds alone through transitive,
   strengthening, subsumption and strict-bound disequality rules. A snapshot
   keeps such a proof unwrapped and a join treats it as independently live.
   S11's preheader snapshot still wraps every relation, because each counted
   root it captures names that snapshot as its proof point; the corpus's
   counted-root checker caught this when the first implementation skipped it
   there too.

5. **Renamed symbolic instances.** A non-canonical symbolic instance whose
   arguments are distinct symbolic parameters takes the body disposition,
   invariant outcomes and postcondition proofs of an instance of the same
   declaration and argument kinds analyzed in an earlier component; its own
   component still decides publication. A temporary patch that analyzed every
   such instance anyway and compared the two found all 7088 reused instances
   of Halo's `pkg::vm` check identical (456 of them with postconditions).

6. **Closure universe.** Rows only for zero, endpoints of live relations
   and of nonzero implicit edges, and terms of live signed goals; any other
   term is read through zero. Joins take every pair of the terms some
   predecessor has a row for, and ordinary fallbacks (in snapshots and joins)
   look values up the same way. The generated-flow test now compares every
   step with a reference that computes every row, and asserts that a closed
   record's cells equal its closure; that assertion found three places that
   treated a missing cell as underivable, and a snapshot shortcut that skipped
   promoting a signed goal's contradiction when the record was closed, which
   this candidate made reachable and which is fixed.

7. **Implied facts and dormant components.** A live relation no tighter than
   the path through zero, from its endpoints' own bounds through zero, or
   whose proof rests on implicit bounds alone, admits no endpoint; an
   implicit edge between two nonzero terms admits its endpoints' whole
   component, and only when it is tighter than that path and a fact reaches
   the component. A dormant component's members read the component's own
   closure, with proofs rebuilt from it. Closures load no stored cell with
   an endpoint outside the universe. The generated-flow test now also
   registers a length, capacity and head of one place that relations reach
   and of another they never do, and its reference uses every term as a
   middle; it failed when the dormant closure skipped its transitive step and
   when a stored cell one unit tighter than that path admitted nothing.
8. **Slot layouts and a cached implicit structure.** A closure's matrix is
   laid out over its universe's terms and a fact state's store over the terms
   that have a cell, both in term order; a store copy drops the slots of terms
   whose cells are gone. The term table keeps each term's bounds through zero,
   the implicit components and their closures up to date from a log of the
   terms whose implicit bounds may have changed, rebuilding when a standing
   measure fact is replaced.

9. **Indexed resolution passes.** The public-signature closure check finds
   `public` writers in one pass over the terminals and published paths by
   prefix lookup; a match binder finds its paired field through an index by
   owner; each ensures clause takes its roles, variant fields, entry uses and
   variant uses from groupings built in one pass. Routine fixes under
   unchanged design; the serial composition phase of the `style_oracle`
   check fell from 10.7 s to about 6 s in samples.

10. **Summary-read symbolic scope.** Symbolic validation analyzes the
   postcondition components of the canonical generic instances and,
   transitively, only the callee components that contain a postcondition,
   the only components whose analysis a caller reads [FN-9]; before, it
   analyzed every component the canonical instances reach. In Halo-wf
   `9915000`'s `pkg::vm` check on main `ce57c9ddd`, the checker's work
   counters (`WHITEFOOT_CHECK_WORK`, one GitHub-hosted run) attribute 344 of
   the 1227 function analyses to work no caller reads: 254 symbolic analyses
   of nongeneric functions without a postcondition, analyzed again in the
   concrete phase, and 90 repeated symbolic instances of generic functions
   without one. Those runs hold about 36% of the evaluated L0 pairs, 40% of
   the interning calls and 43% of the joins. Written before the timing: the
   candidate is kept if Halo's `pkg::vm` check then performs fewer analyses
   with the same verdict, the gate passes, the LLVM of Halo's `test` entry is
   byte-identical, and on the i9-14900K (interleaved runs of the base, a twin
   of the base and the head, ten each after one warm-up, `taskset -c 2-15`)
   the head's median wall time is below both the base's and the twin's
   minimum; it is rejected otherwise, because the removed analyses run
   concurrently with one another beside a serial symbolic type check that
   may hide their cost.

Rejected alternatives:

- Per-function concurrency first: the critical path is one function, not
  many functions.
- Caching the ordinary snapshot that consecutive Result refreshes take of an
  unchanged state: at most a third of the refresh snapshots of
  `pkg::html::tree_builder` repeat the previous state, by a count of
  consecutive snapshots with the same store and term count, and the shortcut
  would reuse one snapshot event for several flow points.
- Storing only bounds stronger than their terms' implied bounds, for terms
  that have a row: the existing [incremental-closure decision](../../../design/compiler/incremental-closure.md)
  refuses it for the separate arguments kills, joins and deliveries would
  each need; candidate 4 removes most of the same cost at snapshots without
  them. Candidate 7 omits such cells only where every fact of a term is
  implied through zero, which the closure-universe argument covers.
- Applying a kill batch that kills no term of the closure universe and no
  live signed goal without materializing first. The closure after the batch
  is the same either way, and with candidates 7 and 8 the rule skipped 3,256
  of the 4,000 batches it decided in Halo's `pkg::vm` check, by a temporary
  count, but disabling it changed that check from 16.4 s to 16.6 s,
  `pkg::style` from 14.1 s to 13.7 s and the 100-arm function from 0.91 s to
  1.02 s: below the 1.2x criterion, so the rule and its argument were
  removed.

## Results

Fresh-cache `--check`, wall and user seconds on four processors:

| Compiler | `style_oracle` | `layout_oracle` | `pkg::html::tree_builder` alone | `pkg::style` alone |
|---|---:|---:|---:|---:|
| baseline `c3d26643` | 95.5 / — | 274.6 / 256.1 | — | — |
| 1 + 2 | 58.5 / 83.1 | — | 41.4 | 22.6 |
| 1 + 2 + 3 | 44.0 / 67.9 | 101.1 / 206.1 | 23.2 | 24.8 |
| 1 + 2 + 3 + 4 | 38.1 / 55.4 | 92.3 / 186.2 | 13.2 | 23.3 |

Against the baseline, the fresh-cache `style_oracle` check is 2.5x faster
and `layout_oracle` 3.0x. Peak RSS went from 2.5 GB to 2.1 GB for
`style_oracle` and from 2.9 GB to 4.1 GB for `layout_oracle`, whose
concurrent module checks are held in memory at once. The `pkg::style` module check
moved from 22.6 s to 24.8 s and back to 23.3 s across candidates 3 and 4,
within the noise of single runs; its cost is the edge insertion described
below. Peak RSS of
the `pkg::html::tree_builder` check fell from 2.3 GB to 1.2 GB with
candidate 4. The LLVM of `png_oracle`, `css_selectors_oracle`,
`html_tree_oracle` and `style_oracle` is byte-identical to the baseline's
after candidates 2, 3 and 4, and that of `layout_oracle` after candidate 4,
the only one at which it was compared.

Separating the two kinds of gain, a single-thread run of the final
compiler (the thread count then forced to one through the `WHITEFOOT_JOBS`
override that compiler had, since removed) took 66.5 s for
`style_oracle` and 196.2 s for `layout_oracle`: the algorithmic candidates
give 1.44x and 1.40x, and concurrency on four processors the rest (1.74x and
2.13x).

With candidate 5, Halo's `pkg::vm` module check takes 57.9 s and 2.2 GB
instead of exhausting 12 GB, and the `test` entry's fresh-cache check is
accepted in 122.6 s at 3.3 GB peak RSS. The Snowghost LLVM of all five
entries stays byte-identical.

With candidate 6, the module checks take 29.2 s for Halo's `pkg::vm` (was
48.9 s with function concurrency, at 1.9 GB), 6.8 s for Snowghost's
`pkg::html::tree_builder` (was 13.2 s) and 18.3 s for `pkg::style` (was
23.3 s); the Snowghost LLVM of all five entries stays byte-identical.

With candidates 7 and 8, against the compiler before them (`c1d7d598`), on
this host in one run each:

| Check | before | after |
|---|---:|---:|
| Halo `pkg::vm` | 37.2 s, 1.9 GB | 16.6 s, 1.2 GB |
| `pkg::html::tree_builder` | 9.0 s, 0.9 GB | 4.4 s, 0.4 GB |
| `pkg::style` | 21.6 s, 0.7 GB | 14.3 s, 0.6 GB |
| `style_oracle` entry, fresh cache | 34.4 s, 1.6 GB | 26.9 s, 1.0 GB |
| `layout_oracle` entry, fresh cache | 72.2 s, 2.6 GB | 46.8 s, 1.7 GB |
| 100-arm generated function | 27.6 s, 0.7 GB | 0.92 s, 0.09 GB |

Concurrency inside the `pkg::vm` check, its dependencies' module verdicts
and the analyses of one postcondition level (up to 148 functions in one level
there, by an exploratory per-function timer patch not kept in the tree),
gives 1.36x: 23.5 s pinned to one of the four processors with
`taskset -c 0`, 17.2 s on all four.

`library_builtin` now takes 0.34 s in each analysis. The generated function
takes 0.19 s, 0.88 s and 5.2 s for 50, 100 and 200 arms, still superlinear.
The LLVM of all five Snowghost entries stays byte-identical.

With candidate 9 as well, each compiler pinned to one processor with
`taskset -c 0` and then on all four, one run each on this host (seconds;
the baseline cannot finish Halo's check, which exhausted 12 GB):

| Check | baseline 1 / 4 | `c1d7d598` 1 / 4 | this branch 1 / 4 |
|---|---:|---:|---:|
| Halo `pkg::vm` | — | 43.7 / 35.9 | 18.2 / 15.8 |
| `pkg::style` | 27.0 / 25.6 | 21.3 / 20.2 | 13.8 / 12.9 |
| `style_oracle` entry | 102.9 / 101.8 | 47.5 / 33.0 | 30.2 / 20.2 |
| `layout_oracle` entry | 261.7 / 262.1 | 141.7 / 66.9 | 76.7 / 34.3 |

The baseline has no concurrency, so its one-processor column against this
branch's gives the algorithmic gain alone: 3.4x for both entries and 2.0x for
`pkg::style`; concurrency then adds 1.5x and 2.2x to the entries and little
to a module check whose critical path is one module.

Candidate 10 was measured against its base, main `fe5589ec5`, on Halo-wf
`9915000`'s `pkg::vm` check. Both compilers accept, and the LLVM of Halo's
`test` entry is byte-identical (GitHub-hosted runner). The work counters fall
from 1227 function analyses to 912, from 3895 joins to 2771, from 2.21 to
1.78 million evaluated L0 pairs and from 8.89 to 7.01 million interning calls,
less than the 36% of pairs predicted, because the estimate took each generic
declaration's first symbolic run as its canonical instance. On the i9-14900K
(native Ubuntu, performance cores at 5.0 GHz, `taskset -c 2-15`, the `gate`
profile, ten interleaved runs each after one warm-up):

| Compiler | median wall | range | peak RSS |
|---|---:|---:|---:|
| base `fe5589ec5` | 8.84 s | 8.78–8.90 s | 1.54–1.56 GB |
| twin of the base | 8.86 s | 8.79–8.91 s | 1.54–1.57 GB |
| candidate 10 | 8.61 s | 8.53–8.75 s | 1.46–1.49 GB |

The head's median lies below both the base's and the twin's minimum, so the
candidate is kept by its prior criterion; the gain, 2.6%, is much smaller
than the share of proof work removed, because the analyses run concurrently
beside the serial symbolic type check that dominates the module-verdict
thread.

## Remaining costs

- **Symbolic validation on Halo's critical path.** In `pkg::vm` the
  module-verdict thread now spends about 44% of its samples in generic
  validation (28% type-checking the symbolic view, 14% discovering and
  instantiating its signatures), against 25% for the concrete type check and
  10% reading declarations; the symbolic view checks every function body,
  nongeneric ones included. A per-thread sample split on a GitHub-hosted runner
  (Halo-wf `9915000`, main `fe5589ec5`) puts 57% of that thread's samples in
  the symbolic view's body checks and 1% in the concrete view's, so the
  nongeneric bodies are a small part; the generic bodies and their
  non-canonical symbolic instances are the rest. About 30% of that thread's samples are in the C
  library's allocator and copying.

- **Edge insertion of constant terms.** In `pkg::style`, now the slowest
  module, 46% of samples are edge insertion during pre-kill materialization.
  A term with an exact value (a constant, or a measure with a standing
  constant value) is zero shifted by that value, so inserting its two
  implicit edges finds every row tight and recomposes all n columns of each,
  n² products that improve no cell. Its closed row and column are zero's
  shifted; filling them directly, with the transitive proof through zero,
  would remove that work. It needs its own argument that the insertion order
  still closes the matrix.
- **No-cache builds analyze every body twice.** An in-memory receipt store
  would let a cacheless entry check reuse its module verdicts' analyses.
