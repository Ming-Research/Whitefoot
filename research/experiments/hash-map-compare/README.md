# Ordinary HashMap before/after comparison

This manual experiment answers one question for a change to the ordinary
`HashMap` (`lib/std/collections/hash_map/`): does any operation get slower,
or does the map hold more memory, after the change than before it? It serves
the home-bucket chain representation's performance acceptance (status board
item coord-wfbl-02-01) and any later change to the same library, such as
shrinking (wf-need-firn-hash-shrink). `.github/workflows/hash-map-bench.yml`
runs it on request, on a hosted runner or on the owner's i9-14900K; it is
never a gate. Keep these files while a hash-map change still needs this
comparison; remove them with the workflow when none does.

## What is compared

The standard library is compiled into the compiler, so the two arms are two
compilers built from one compiler source, the candidate revision's, that
differ only in `lib/std/collections/hash_map/`: the candidate's own, and the
base revision's checked out over it. Both compile the same programs in this
directory. The twin is a byte copy of the base image, so its difference from
the base measures nothing but this run's noise. `compare.py build` refuses a
candidate image byte-identical to the base image: the change would not reach
the program.

`bench.wf` uses only operations both representations provide. Keys and
values are `u64`; the key of index `i` is `i` times an odd constant and its
hash is the splitmix64 finalizer of the key, so hashes are uniform. Lookups
and removals visit indexes in a fixed permutation unrelated to insertion
order, since a representation that stores pairs in insertion order would
otherwise read them sequentially. Each cell is one operation at one size,
1,000, 10,000 or 1,000,000 pairs:

| Operation | Timed interval | Untimed |
| --- | --- | --- |
| insert-presized | `hash_map_new` at capacity N, then N puts | release |
| insert-growing | `hash_map_new` at capacity zero, then N puts | release |
| lookup-hit | N lookups of present keys | building the map once |
| lookup-miss | N lookups of absent keys | building the map once |
| remove | N removals until empty | building each map, release |
| remove-insert | N steps, each removing the oldest pair and putting a new one | building the map once |
| iterate | one `hash_map_each` over N pairs | building the map once |

A sample repeats its batch until it makes at least 262,144 operations
(iterate: eight times as many batches, at least eight passes, since a visit
costs well under a nanosecond). A process prints one warmup sample and
five measured ones; its value is the median of the measured samples in
nanoseconds per operation. Every row carries the operation count and a check
the driver compares with the value the workload's definition gives (every
lookup of a present key finds it and sees its index, every removal finds its
pair, a built map holds N pairs); a mismatch is invalid data, not a
measurement.

The memory cell builds a map of N pairs from capacity zero. Its observed
image renames the program's heap calls to `peak.c`, which reads after each
the heap counter that `std::process::heap_in_use` reports and keeps its
maximum; for a block below the runtime's copy threshold, which grows by
taking the new block before giving the old one back, it also counts the
moment both are held. The cell reports the bytes the built map retains and the peak
above the reading before building, and the maximum resident set as a
descriptive figure. Both are deterministic; each arm runs twice and the two
runs must agree.

`hold.wf` is built by the candidate compiler alone, since the base
representation has no resumable scan. It times `hash_map_scan` calls with
limit 64 one by one, with a callback that only counts: over 1,000,000,
4,000,000 and 32,000,000 uniform pairs, each call continuing the last after
one untimed pass, and over 64, 1,024 and 65,536 pairs sharing one hash, every
call starting at the call that holds their chain. By their layout (24 bytes a
slot, 8 for its chain link, 8 for a chain head; capacity 2^20, 2^22 and 2^25)
the three uniform tables take about 42 MB, 168 MB and 1.34 GB, computed, not
measured. The 4,000,000- and 32,000,000-pair tables are far larger than the
14900K's 36 MB third-level cache, so the comparison between them is meant to
be of per-call work, not of cache hits against misses, and the 14900K run
tests that premise; the 1,000,000-pair table is only about a sixth over the
cache, so probably mostly cached there, and is read for H2 and H3 only.

## Criterion, set before measurement

The decision rule is the engineering standard's paired rule, with the
resolutions and hold criterion the hash-scan investigation recorded before
measurement:

- A round runs every unresolved cell's three images once, in a rotating arm
  order and a shuffled cell order, each process pinned to one CPU with
  `taskset` and one runtime driver. A cell's per-round ratios are candidate
  over base and twin over base.
- The interval is the distribution-free 95% interval of the median of the
  per-round ratios (the order statistics a binomial bound selects), so one
  disturbed round cannot move it far; it needs at least six rounds.
- A cell is judged once the twin's interval contains 1 and its half-width is
  below the cell's resolution, 3% at 1,000 pairs and 2% at 10,000 and
  1,000,000. Then a candidate interval wholly above 1 is a regression of any
  size, wholly below 1 an improvement, and across 1 no detected change
  only when it also lies within 1 plus or minus the resolution; a wider
  interval across 1 has not resolved a change of that size and takes more
  rounds (a hosted slow control once judged a 13% slowdown, ratio 1.1343
  [0.9929, 1.1667], no change before this condition). A
  cell still not judged at the round limit is reported unresolved. The
  resolution is the smallest difference the cell must resolve, not an
  acceptance threshold.
- The floor: a cell's spread is the median over rounds of |twin/base - 1|,
  the twin's per-round noise, which unlike the twin interval does not shrink
  as rounds are added. A run given no floor only measures it, and its
  summary prints, per cell, the upper end of the spread's 95% interval as a
  `floor` line. A later run on the same machine is given that line
  (`reduce --floor`, the workflow's `floor` input); a cell whose spread is
  above its floor is not counted, whatever its verdict, and is rerun. The
  floor is kept per cell, since the cells' noise differs several times over
  (on one hosted run, twin noise ranged from 0.33% to 2.98% across the 21
  cells). On a machine, the first run measures the floor, which is
  recorded on the board with its run, and the comparison that decides is a
  second run given it; the floor is measured again when the machine, its
  settings or the toolchain change.
- Memory: any difference in retained or peak bytes is a change.
- Hold: H1, the p99 of a uniform call at 32,000,000 pairs is at most twice
  its p99 at 4,000,000 (eightfold the pairs, so work that grows with the
  table gives a ratio near 8); H2, the p99 at 1,000,000 pairs is at most 20
  microseconds; H3, the median call over a chain of m pairs takes at most 2m
  times the median per-pair time of the uniform calls at 1,000,000 pairs.

Accepting a detected regression is not this instrument's decision.

## Controls

The twin is the null control in every run. The slow control, the workflow's
`control: slow`, replaces the candidate arm with the base image given an
argument that adds work inside the interval at every fourth step of every
timed batch: one lookup in the decoy, a second map of the cell's size that
every run builds before timing and no timed operation touches, and for
iterate one more pass. The extra work is selected by a bit mask passed as
data, all ones in every other run, so every run executes the same machine
code and the extra work is the only difference between the arms. `compare.py
reduce --expect-regression` fails unless every timed cell is judged a
regression. Extra lookups in the measured map itself were not detected for
lookups at 10,000 pairs on a hosted AMD EPYC 7763, for a reason not
established: 25% more lookups of the key just used measured 16% faster, and
of a key half the permutation away measured no slower (ratio 0.97, interval
[0.94, 1.01]), while the same extra work slowed the other 20 cells. Extra
lookups in the measured map are therefore not a reliable slowdown; lookups in
a separate map share none of its cache lines. The extra lookups' results go to a separate field, so the checks stay
those of the workload. `compare.py self-test` exercises the interval, the
decision rule and the data checks on synthetic rows.

## Running and outputs

```sh
python3 -B compare.py self-test
python3 -B compare.py build --base-wfc BASE_WFC --candidate-wfc CANDIDATE_WFC \
  --runtime-root CANDIDATE_TREE --out OUT
python3 -B compare.py measure --out OUT --cpu 2
python3 -B compare.py reduce --out OUT [--floor "lookup-hit:1000=0.0123,..."]
```

`OUT/manifest.txt` records the revisions, compiler and image digests, CPU,
sizes and rounds; `raw.tsv`, `memory.tsv` and `hold.tsv` hold every row;
`summary.txt` is the reduction. The workflow uploads them with the images.
On the 14900K the default CPU is 2, one hyperthread of a P core other than
CPU 0. A hosted runner shares its host, so its runs show the instrument
working and decide nothing.

## Limits

One key and value type, one hash distribution and one CPU are measured;
other pair sizes, adversarial hashes outside the hold cells, and parallel
use are not. The memory cell counts bytes the program asks for, not what the
allocator rounds them to.
