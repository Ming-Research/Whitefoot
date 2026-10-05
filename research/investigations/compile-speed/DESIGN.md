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

Rejected alternatives:

- Per-function concurrency first: the critical path is one function, not
  many functions.
- Caching the ordinary snapshot that consecutive Result refreshes take of an
  unchanged state: at most a third of the refresh snapshots of
  `pkg::html::tree_builder` repeat the previous state, by a count of
  consecutive snapshots with the same store and term count, and the shortcut
  would reuse one snapshot event for several flow points.
- Storing only bounds stronger than their terms' implied bounds: the
  existing [incremental-closure decision](../../../design/compiler/incremental-closure.md)
  refuses it for the separate arguments kills, joins and deliveries would
  each need; candidate 4 removes most of the same cost at snapshots without
  them.

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
compiler (the thread count then forced to one) took 66.5 s for
`style_oracle` and 196.2 s for `layout_oracle`: the algorithmic candidates
give 1.44x and 1.40x, and concurrency on four processors the rest (1.74x and
2.13x).

With candidate 5, Halo's `pkg::vm` module check takes 57.9 s and 2.2 GB
instead of exhausting 12 GB, and the `test` entry's fresh-cache check is
accepted in 122.6 s at 3.3 GB peak RSS. The Snowghost LLVM of all five
entries stays byte-identical.

## Remaining costs

- **Edge insertion of constant terms.** In `pkg::style`, now the slowest
  module, 46% of samples are edge insertion during pre-kill materialization.
  A term with an exact value (a constant, or a measure with a standing
  constant value) is zero shifted by that value, so inserting its two
  implicit edges finds every row tight and recomposes all n columns of each,
  n² products that improve no cell. Its closed row and column are zero's
  shifted; filling them directly, with the transitive proof through zero,
  would remove that work. It needs its own argument that the insertion order
  still closes the matrix.
- **The composition's own analyses.** After its module verdicts, the
  `style_oracle` composition analyzes on one thread the functions no receipt
  covers, about 14 s here. Functions of one postcondition component read
  only earlier components' summaries, so a component level could run
  concurrently once the receipt key is shown to depend only on a function's
  callees.
- **No-cache builds analyze every body twice.** An in-memory receipt store
  would let a cacheless entry check reuse its module verdicts' analyses.
