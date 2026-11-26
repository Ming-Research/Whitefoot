# Data placement and the regression comparison

A change that leaves a compute kernel's generated code unchanged can still
move where the kernel's stack and heap blocks are mapped, and on some hosted
processors that alone changes the kernel's time. This investigation asks
which move of data alone, applied to one image, shows that a host times a
kernel by where its data lies, so that `compute-regression` can run such a
move as a control before it gives a verdict. The [results](#results) name the
cause on Zen 3 and Zen 4 and the control that follows from it, which
`compute-regression` runs (`tests/performance/README.md`, "Data placement");
its [confirmation](#confirmation) ran the control as implemented.

## The observation

The stop-signals work once started a receiver thread in every program before
the entry. The generated kernels did not change, and on EPYC 7763 (Zen 3) and
EPYC 9V74 (Zen 4) hosts the regression comparison read `stencil` 9 to 30
percent slower at one worker; EPYC 9V45 (Zen 5), Xeon and i9-14900K hosts
showed no loss, and creating the same thread after the entry recovered most of
it on Zen 3
([stop signals](../stop-signals/README.md#startup-cost-on-amd-hosts)). That
record left open which structure the placement defeats. The comparison's
placement control moves code only
([code placement](../code-placement/DESIGN.md)).

## What the earlier probe's data already shows

The second probe (Whitefoot run 37764957433, `claude/stop-probe2` at
`1adb44abe`) uploaded each job's raw samples and one `strace` of `stencil` per
variant: B the merge base `c18e6708b`, S the receiver started before the
entry (`9ec755606`), N the receiver disabled, P the receiver started after the
entry. Read again on 2026-11-25, while the artifacts still existed:

**Times come in levels, per launch.** `stencil` at one worker, the median of
each process's five calls in milliseconds, on the two quiet EPYC 7763 jobs:

| job | arm | process medians |
|---|---|---|
| 1 | B (15 launches) | 41.3 to 42.4 in 13, then 43.0 and 52.6 |
| 1 | S (15 launches) | 44.0 to 48.6 in 7, 59.9 to 61.1 in 8 |
| 2 | B (15 launches) | 41.2 to 42.8 in all 15 |
| 2 | S (15 launches) | 46.7 to 48.6 in 10, 60.0 to 60.6 in 5 |

So S is not uniformly slower: each launch of S lands on one of two levels,
both above B's, and the identical-image control of S against S passed on both
jobs because a median of five pairs does not see two levels in equal shares.
On the Xeon 8370C job the same split is larger: B and N read 88 to 100 ms in
all 20 launches, while S and P read either 92 to 101 ms or 171 to 176 ms in
their 20, and that job's S against S control read 0.58 at one worker. The
stop-signals record called that host too noisy to read; a split that follows
the variant says it also times `stencil` by placement, by another amount.

**The two grids' distance differs by variant.** A timed call of `stencil`
allocates two grids of 32 MiB plus a header, which glibc maps one by one
(33,558,528 bytes each). With A the first and B the second, the traces read:

| job | host | B | S | N | P |
|---|---|---|---|---|---|
| 1 | 7763 | +0x2001000 | +0x47a00000 | +0x2001000 | -0xa001000 |
| 2 | 7763 | +0x63ff000 | +0x7fff000 | +0x61ff000 | -0x2001000 |
| 4 | 7763 | +0x2001000 | +0x7fff000 | +0x2001000 | +0x2001000 |
| 3 | 9V45 | +0x7dff000 | +0x7fff000 | +0x6bff000 | +0x2001000 |
| 5 | 9V45 | +0x71ef000 | +0x47c00000 | +0x2001000 | +0x2001000 |
| 6 | 8370C | +0x77ff000 | +0x46ff0000 | +0x7bff000 | -0xa001000 |

The traces' other mappings say how. The entry runs on a thread with a 1 GiB
stack, and its first allocation makes glibc reserve a heap for the thread:
128 MiB mapped, then trimmed to the 64 MiB that starts on a 64 MiB boundary. A
grid then goes to the highest hole that holds it. [`layout.py`](layout.py)
models these rules for one thread or none before the entry; given each
trace's C library base it reproduces both grid addresses exactly for 16 of the
18 traces of B, S and N, and is 64 KiB off for one grid in the other two (job
5's B and job 6's S), where a 64 KiB mapping the model leaves out took the top
of the hole first.

- In B and N the stack sits under the C library at a random 2 MiB step, so the
  hole between the thread's heap and its stack is anywhere up to 64 MiB. Half
  of the launches have no room for a grid there and put both grids under the
  heap, 0x2001000 apart (adjacent); the other half put A above the heap and B
  below it, about 0x61ff000 to 0x7fff000 apart in 2 MiB steps (job 5's
  0x71ef000 has a small mapping between), the last of them one launch in 32.
- In S the receiver thread and its heap exist first, and the entry's stack is
  mapped directly under the receiver's heap, which starts on a 64 MiB
  boundary. The hole above the entry's heap is then always 64 MiB less one
  page, exactly one grid fits, and the grids are 0x7fff000 apart (128 MiB less
  4 KiB) unless a hole beside the receiver takes A first, which the same
  arithmetic gives 17 launches in 32. The slow level's share of S's launches on
  the two quiet 7763 jobs, 13 of 30, agrees with 15 in 32.

**Hypothesis.** The slow level is the distance 0x7fff000. Both grids start 16
bytes into a page, so a cell of one and the same cell of the other always
share a set of the first-level data cache; at that distance their addresses
differ only in bits 12 and 27 (and above 27). AMD's first-level data cache
predicts a line's way from a hash of its linear address, and for Zen 3 AMD
states that "at a given L1 DC index (11:6), only one cacheline with a given
linear hash is accessible at any time" (Software Optimization Guide for AMD
Family 19h Processors, 56665 rev. 3.00, section 2.6.2). The guide does not
say which bits the hash takes; the hash recovered for Zen, Zen+ and Zen 2
pairs bit 12 with bit 27 (bits 12 to 27, folded as 12 with 27, 13 with 26, 14
with 25, 15 with 20, 16 with 21, 17 with 22, 18 with 23 and 19 with 24: Lipp
et al., "Take A Way: Exploring the Security Implications of AMD's Cache Way
Predictors", ASIA CCS 2020, section 3). If Zen 3 and Zen 4 keep that hash,
the two lines look alike to the predictor and each access makes the other
miss, on every cell. This is inferred from one trace
per variant and the level shares; no trace belongs to a timed launch, and the
second level (S's 44 to 49 ms against B's 41 to 42) is not explained by it.

If the hypothesis holds, a move that shifts every mapping by the same amount
cannot expose the effect: under address randomization it only draws another
of the positions the unmoved image already draws. What changes the time is
the grids' distance, which the mapping order decides.

## Moves compared

Each is applied to one image at run time, so no code address differs between
the arms, except the static-data pad, which needs a second link.

| name | move | from |
|---|---|---|
| `map8` | one idle mapping of 8 MiB plus a page, the size of a thread stack, before the entry | the item's first candidate |
| `map1`, `map34` | the same with 1 MiB plus a page and with 34 MiB | several fixed sizes |
| `touch` | the launching thread's heap used before the entry: one mapped block, one from the program break | allocator use ahead of the entry |
| `datapad` | 32 bytes of `.data` and of `.bss` linked ahead of every other object's | the data counterpart of the code pad |
| `thread` | one idle thread with its own heap, settled before the entry | what the stop receiver did to the mappings |
| `arena1` | `GLIBC_TUNABLES=glibc.malloc.arena_max=1`: every thread allocates from the process's one heap, so no per-thread heap is mapped and large blocks are mapped one under the other below the entry stack | a placement that does not depend on the mapping order |

## Method

A temporary workflow runs [`probe.sh`](probe.sh) on eight hosted
`ubuntu-24.04` jobs, which the pool assigns to processors it chooses. Each job
builds three compilers and their regression images with this branch's
instrument: `b` (`c18e6708b`), `s` (`9ec755606`) and `h` (this branch), each
linked with [`shift.c`](shift.c), whose moves `WF_DATA_SHIFT` selects, and
`hd`, `h` with the static-data pad. It then runs:

- [`alias.c`](alias.c): the stencil's access pattern over two arrays mapped at
  chosen addresses, the distance the only variable, first at the low address
  bits the traced images had, then from an address whose bits 12 to 27 are
  zero with one bit pair of the hash changed at a time;
- `tests/performance/compare.sh` campaigns: `h` against itself, `b` against
  `s`, and every revision against itself under each move (`thread`, `arena1`
  and `map8` for all three revisions, the others for `h`);
- traced launches of `stencil` at one worker, `strace` stopping the process
  only at `mmap` and `munmap` (four stops inside a timed call): 24 unmoved, 16
  under `thread` and 6 under `arena1` per revision, and 32 launches without
  address randomization, each with an idle mapping of k times 2 MiB ahead of
  the entry, which walks the entry stack through every position within the
  heap's 64 MiB alignment.

[`reduce.py`](reduce.py) prints each campaign's ratios with the regression rule
applied in both directions, and each traced series by grid distance.

## Criterion

Written before the run. A job is *conclusive* when its `h` against `h`
campaign passes with no adverse width. A campaign reads a kernel *moved* at a
width when the regression rule holds in either direction: the wall ratio is
below 0.97 with at least four of five pairs lower, or above 1/0.97 with at
most one pair lower. A move *detects* on a job when its campaign reads
`stencil` moved at one width or more. A host is *insensitive* when its job is
conclusive and its `b` against `s` campaign reads `stencil` moved at no width.

1. **The incident reproduces.** On every conclusive EPYC 7763 job, `b` against
   `s` reads `stencil` slower at one worker. A job where it does not counts
   against everything below, since nothing can then be said to expose it.
2. **The distance is the cause.** On every conclusive 7763 job: at least nine
   in ten of the traced launches whose grids are 0x7fff000 apart are more than
   15 percent slower than the median of the adjacent launches (0x2001000), the
   launches without address randomization agree, and `alias` reads 0x7fff000
   more than 15 percent slower than 0x2001000 at the traced low bits. On a
   9V45 or Intel job `alias` reads the two within 3 percent. Any of these
   failing rejects the hypothesis; which other distances are slow is recorded
   without a criterion.
3. **A uniform shift does not detect.** Predicted from the hypothesis: `map8`,
   `map1`, `map34`, `touch` and `datapad` detect on no conclusive 7763 job. A
   detection by one of them on a 7763 job where `thread` also detects rejects
   the prediction and makes that move a candidate.
4. **The receiver's move detects on the usual image.** `thread` qualifies as
   the control's move when it detects for `b` and for `h` on every conclusive
   7763 job and reads no kernel moved at any width on any insensitive host.
5. **A fixed placement detects on the incident's image.** `arena1` is
   predicted to read `stencil` faster at one worker for `s` on every
   conclusive 7763 job, and to read it moved for `b` and `h` on none; whether
   it moves any kernel at two or four workers, where threads then share one
   heap, is recorded for every host.
6. **The proposal as a whole** (that some move of data alone marks a Zen 3
   host while the incident's comparison runs) is rejected if no move detects
   for `s`, and none for `b`, on every conclusive 7763 job.

`thread` against `s`, and the spread of an arm's five process medians as a
second signal, are read without a criterion.

## Results

Whitefoot run 45625645014 at `2e1632cd1`, 2026-11-25, eight hosted jobs: two
on EPYC 9V74 (jobs 1 and 8), three on EPYC 7763 (3, 5, 7) and three on Xeon
6975P-C (2, 4, 6); none reached a 9V45. Every job built all three compilers
and ran every campaign. Each job's reduced record is under
[`results/`](results/), written by `reduce.py` from the job's artifact; the
raw samples and traces stay in the run's artifacts while GitHub keeps them.
The hosts ran Linux 7.0 with glibc 2.39, transparent huge pages on request
only and address randomization on. The stop-signals probe ran on Linux 6.17,
where the same merge-base image read `stencil` at 41 to 42 ms on a 7763; here
it reads 76 to 99 ms outside its slow class, so the two runs' times and levels
are not comparable.
A campaign took 34 to 41 s on the AMD hosts and 25 to 27 s on the Xeon.

### The distance between the grids decides the time on Zen 3 and Zen 4

Every launch made with address randomization (138 a job: three revisions,
unmoved, under `thread` and under `arena1`), by the distance between the two
grids of its timed calls; milliseconds, the median with the range under it:

| job | host | 0x7fff000 | adjacent, 0x2001000 | 0x61ff000 to 0x7dff000 | 0x46000000 to 0x49000000 |
|---|---|---|---|---|---|
| 1 | 9V74 | 122.5 (116.0 to 130.1, 25) | 87.1 (85.1 to 100.6, 56) | 85.6 (84.7 to 87.0, 24) | 85.9 (85.1 to 96.5, 30) |
| 8 | 9V74 | 120.2 (113.8 to 126.3, 20) | 85.9 (83.6 to 96.5, 54) | 84.7 (82.8 to 87.9, 22) | 85.0 (83.6 to 86.5, 40) |
| 3 | 7763 | 112.3 (104.3 to 130.3, 22) | 87.7 (82.4 to 93.7, 59) | 80.1 (77.4 to 88.3, 14) | 82.1 (78.1 to 91.0, 37) |
| 5 | 7763 | 116.6 (107.0 to 125.3, 28) | 91.2 (87.0 to 98.8, 51) | 84.6 (80.8 to 87.8, 24) | 84.6 (80.8 to 91.8, 34) |
| 7 | 7763 | 109.4 (101.4 to 116.7, 30) | 86.7 (83.0 to 92.2, 42) | 78.9 (76.3 to 81.7, 34) | 79.8 (77.3 to 84.1, 28) |
| 2 | 6975P-C | 59.9 (58.7 to 64.2, 25) | 59.8 (58.8 to 62.8, 49) | 60.5 (58.8 to 62.7, 23) | 60.0 (58.5 to 62.9, 32) |
| 4 | 6975P-C | 64.4 (61.0 to 67.6, 19) | 63.7 (61.0 to 68.6, 52) | 63.0 (60.3 to 66.7, 24) | 63.7 (61.2 to 67.5, 38) |
| 6 | 6975P-C | 63.1 (60.6 to 71.3, 25) | 62.9 (59.3 to 77.1, 50) | 66.2 (61.5 to 75.0, 25) | 62.7 (59.7 to 70.0, 30) |

On the five AMD jobs every launch whose grids were 0x7fff000 apart was slower
than every launch at any other distance, by 26 to 43 percent on the
medians; on the 7763 the adjacent grids are also 6 to 10 percent slower than
the far ones. The Xeon shows no class. Which class a
launch gets follows the startup order, as the model says: `layout.py` gives
the traced distance to within 256 KiB in all 976 launches of the orders it
models. Over the eight jobs, an image that maps nothing before the entry (B
and this branch, 384 launches) got adjacent grids in 188, 0x61ff000 to
0x7dff000 in 190 and 0x7fff000 in 6; an image with one thread and its heap
before the entry (S, and B and this branch under `thread`, 448 launches) got
0x46000000 to 0x49000000 in 244, 0x7fff000 in 188 and adjacent grids in 16.

`alias`, the two-array loop with no runtime around it, reads the same; the
fastest of five runs in milliseconds:

| distance | 9V74 | 9V74 | 7763 | 7763 | 7763 | 6975P-C, three jobs |
|---|---|---|---|---|---|---|
| 0x2001000 | 54.8 | 54.6 | 46.6 | 49.5 | 47.8 | 37.0, 37.3, 36.8 |
| 0x61ff000 | 53.4 | 53.4 | 40.0 | 45.6 | 44.7 | 37.5, 38.2, 37.7 |
| 0x7fff000 | 89.6 | 76.8 | 57.6 | 76.2 | 56.3 | 37.2, 37.4, 37.1 |
| 0x8001000 | 88.0 | 103.6 | 93.8 | 55.6 | 102.0 | 37.9, 38.2, 37.3 |
| 0x4000000 | 76.9 | 76.3 | 58.2 | 60.1 | 62.8 | 37.5, 38.1, 37.3 |
| 0x10000000, from bits 12 to 27 zero | 82.9 | 113.5 | 82.0 | 98.2 | 129.3 | 37.2, 38.0, 37.2 |
| with bit 12 | 59.5 | 59.1 | 46.9 | 50.8 | 49.6 | 37.5, 38.6, 37.4 |
| with bit 27 | 60.2 | 61.3 | 46.8 | 49.7 | 48.6 | 37.8, 38.0, 37.5 |
| with bits 12 and 27 | 92.3 | 97.3 | 71.6 | 75.8 | 83.4 | 37.7, 37.8, 37.0 |
| with bits 13 and 26 | 86.6 | 89.9 | 66.1 | 70.2 | 77.4 | 37.1, 38.7, 37.5 |
| with bits 14 and 25 | 75.2 | 85.5 | 66.5 | 68.2 | 70.2 | 37.1, 38.6, 37.8 |
| with bit 20 | 53.2 | 53.3 | 42.4 | 45.2 | 45.4 | 37.1, 38.2, 37.7 |

The first five rows keep the low address bits the traced images had. A
distance that leaves the hash of the 2020 paper unchanged (256 MiB, or 256 MiB
with both bits of one of its pairs) is slow on the 9V74 and the 7763, and with
one bit of a pair alone it is not, so Zen 3 and Zen 4 keep that hash; the
other five pairs read 54 to 77 ms (in each job's record). The cause on these
hosts is therefore the first-level data cache's way predictor: at such a
distance a cell of one grid and the cell the loop reads beside it in the other
predict the same way, and each access makes the other miss.

### What the criterion says

A job is conclusive when its `h` against `h` campaign has no adverse width:
jobs 1 to 5 and 8. Jobs 6 and 7 each read one (`fir` at width 2, `records` at
width 4), so the conclusive 7763 jobs are 3 and 5.

1. **Not met.** `b` against `s` read `stencil` at one worker 0.958 with three
   of five pairs lower on job 3 and 0.777 with three of five on job 5, which
   the rule does not call slower. Over the five AMD jobs the campaign failed
   once (job 1: 0.762 and 0.823 at widths 1 and 2, four of five pairs each),
   read a single-width suspect twice (job 3 at width 2, job 8 at width 4) and
   passed twice with a width-1 ratio of 0.777 and 0.772. Each launch of S
   lands in the slow class or not, and five pairs decide by their median and
   their count, so the campaign reads a loss that is there in the mean about
   as often as not. By the criterion as written, nothing below can be said to
   expose the incident on these two jobs; the per-launch readings of
   criterion 2 do not depend on a campaign.
2. **Met.** On jobs 3, 5 and 7 every traced launch at 0x7fff000 (22, 28 and
   30 of them) is more than 15 percent above the median of the adjacent
   launches (the least are 104.3 against 87.7, 107.0 against 91.2 and 101.4
   against 86.7 ms), the launches without address randomization agree (below),
   and `alias` reads 0x7fff000 above 0x2001000 by 24, 54 and 18 percent. On
   the three Xeon jobs `alias` reads the two within 1 percent.
3. **Not met as written.** `map8` and `map1` each read `stencil` moved at one
   width on job 3 (1.08 and 1.09, the unmoved arm slower) and `datapad` on job
   5 (1.08), three of the ten campaigns of the five uniform moves on the two
   jobs, and `thread` detected on both jobs, so the written rule makes `map8`
   and `map1` candidates. None of the three repeats on another AMD job, and
   in each the unmoved arm drew more launches of the slower classes. The
   traced launches support the prediction itself: under address
   randomization a uniform move draws the same classes as no move.
4. **Not met.** `thread` detects for `h` on jobs 3 and 5 and for `b` on job 5
   only (job 3: 0.97, 0.87 and 0.94, none with four pairs), and it read `fir`
   moved at width 2 on the Xeon job 2.
5. **Not met.** `arena1` read `stencil` for `s` at one worker 0.94 slower on
   job 3 and 1.23 faster on job 5. With every thread on one heap the grids are
   always adjacent (144 of 144 traced launches), but they then sit at any
   2 MiB step instead of one fixed offset under a heap, and the time follows
   that too: 85 to 101 ms on job 1 among launches of adjacent grids.
6. **The proposal is not rejected by its rule** (`thread` detects for `s` on
   jobs 3 and 5, and a move reads a width moved for `b` on both), but no move
   detects reliably. A campaign of five pairs cannot tell a host that is
   sensitive from one that is not when the image's own launches fall into
   classes.

### Readings made after the run

These rules were chosen after seeing the data, so they are exploratory until
a run whose criterion is written first confirms them ([next](#next)).

**Positions without address randomization.** Without address randomization a
launch's mappings are the same every time, and an idle mapping of k times
2 MiB made before the entry puts the launch at one chosen position within the
heap alignment's 64 MiB. Each job launched every revision once at each of the
32 positions. B and this branch map alike, so their two series are two
readings of one profile. Taking a position as *off* when its time is 10
percent or more from the median position:

| job | host | B | this branch | off in both | S |
|---|---|---|---|---|---|
| 1 | 9V74 | 30 (1.41) | 30 (1.40) | 30 | 6 to 20 (1.31 to 1.40) |
| 8 | 9V74 | 30 (1.41) | 30 (1.39) | 30 | 6 to 20 (1.25 to 1.37) |
| 3 | 7763 | 30 (1.24), 3 (0.89) | 30 (1.33) | 30 | 6 to 20 (1.22 to 1.36) |
| 5 | 7763 | 30 (1.33) | 30 (1.26) | 30 | 6 to 20 (1.18 to 1.29), 4 (0.90) |
| 7 | 7763 | 30 (1.30) | 30 (1.25) | 30 | 6 to 20 (1.22 to 1.39) |
| 2 | 6975P-C | none | none | none | none |
| 4 | 6975P-C | none | 31 (1.12) | none | none |
| 6 | 6975P-C | none | none | none | none |

Position 30 is the one where B's grids are 0x7fff000 apart, and positions 6
to 20 are where S's are. A position that is off in two readings separates the
five AMD jobs from the three Xeon jobs for every image. On the Xeon jobs the
one reading past the line was not repeated, and every other position read
0.949 to 1.094 of its median.

**Two arms' positions compared.** The same series read as a comparison of two
arms: each arm's time at a position is divided by the arm's median position,
so a loss an arm has at every position cancels, and a position is *unlike*
when the two quotients differ by 10 percent or more. Positions unlike, of 32,
in one reading:

| job | host | this branch over B, which map alike | S over B |
|---|---|---|---|
| 1 | 9V74 | 0 | 16 |
| 8 | 9V74 | 0 | 16 |
| 3 | 7763 | 0 | 20 |
| 5 | 7763 | 0 | 20 |
| 7 | 7763 | 0 | 18 |
| 2 | 6975P-C | 0 | 0 |
| 4 | 6975P-C | 2 (1.13, 1.16) | 2 (1.10, 0.90) |
| 6 | 6975P-C | 1 (1.10) | 1 (0.90) |

On the AMD jobs two images that map alike are slow at the same position and
agree within 8.3 percent everywhere, while B and S are unlike at 16 positions
or more; on the Xeon jobs the few positions past the line are single readings
on hosts whose ratios spread 0.90 to 1.16. Lines below 10 percent
do not separate: at 5 percent the arms that map alike read 8 positions unlike
on job 3.

**Two arms' positions in order of time.** Position k of one image and
position k of another are the same placement only when the two map alike. A
position is made by an idle mapping of k times 2 MiB before the entry, so an
image that maps r times 2 MiB more before its entry has the same profile moved
by r positions: on every job B's grids are 0x7bff000 apart at position 0 and
2 MiB less at each position to 13, adjacent from 14 to 29 but for position 16
(0x4002000 the other way), 0x7fff000 apart at 30, and position 31 (0x7dff000)
continues into position 0. With address randomization a launch draws its
position, so a campaign can be decided by how many positions an arm has at
each time, not by which positions they are. This branch's series read from
position k + r is a reading of the image B would be with r times 2 MiB more
mapped. Compared with B's one position against one, it is unlike at two
positions or more for every r from 1 to 31 on the five AMD jobs, each arm's
slow position at the least, so that comparison marks a change that address
randomization hides. Compared in order of time (each arm's 32 times over its
median position, sorted, a rank unlike when the two differ by 10 percent or
more), every r reads the same:

| job | host | one against one, this branch moved by 1 to 31 positions over B: positions unlike | in order of time, this branch over B: ranks unlike, with all ratios | S over B | S over this branch |
|---|---|---|---|---|---|
| 1 | 9V74 | 2 at every r | 0 (0.994 to 1.001) | 14 | 14 |
| 8 | 9V74 | 2 at every r | 0 (0.986 to 1.007) | 14 | 14 |
| 3 | 7763 | 2 to 12 | 0 (0.993 to 1.075) | 14 | 14 |
| 5 | 7763 | 2 to 14 | 0 (0.944 to 1.010) | 14 | 14 |
| 7 | 7763 | 2 to 7 | 0 (0.957 to 1.006) | 14 | 15 |
| 2 | 6975P-C | 0 | 0 (0.986 to 1.004) | 0 | 0 |
| 4 | 6975P-C | 0 to 4 | 0 (0.996 to 1.034) | 0 | 0 |
| 6 | 6975P-C | 0 to 1 | 0 (0.980 to 1.040) | 0 | 0 |

In order of time, two readings of one profile agree within 7.5 percent at
every rank on all eight jobs, B and S are unlike from rank 18 on the five AMD
jobs (S has 15 slow positions where B has one), and on the Xeon jobs every
rank of S is within 4.1 percent of B's and of this branch's; the single
readings past the line that the comparison one against one shows there do not
appear.

**Mapping calls.** In the same launches, B and this branch make the same
mapping calls at the same addresses at all 32 positions on all eight jobs
(256 of 256), although their runtimes differ by seven weeks of changes, and B
and S differ at all 256. Whether a change moves where a program's large
blocks are mapped can therefore be read without timing anything.

## Conclusion

- On EPYC 7763 and EPYC 9V74 hosts `stencil` runs 26 to 43 percent slower
  when its two grids lie 128 MiB less 4 KiB apart, because the way predictor
  of the first-level data cache then cannot tell them apart; on the 7763
  adjacent grids also cost 6 to 10 percent against far ones. The Xeon 6975P-C
  has no such dependence. Zen 5 and the other Xeon models were not sampled;
  on the confirmation's two EPYC 9V45 (Zen 5) jobs S's `stencil` runs at two
  levels 6 to 17 percent apart.
- Which distance a launch gets is decided by what the program maps before its
  entry and by where address randomization puts the C library. A thread with
  its own heap before the entry moves the slow distance from 6 launches in
  384 to 188 in 448. That is what the stop-signals receiver did.
- A move that shifts every mapping alike (an idle mapping of any size, heap
  use ahead of the entry, a pad in the static data) changes nothing under
  address randomization, and no move read through a campaign of five pairs
  detects reliably.
- Readings that do not depend on a campaign separate the hosts and the
  changes in this run. Among 32 positions without address randomization, a
  position 10 percent or more off marks a host that times the kernel by
  placement; two arms whose positions, in order of time, are unlike at a rank
  mark a comparison that placement decides on that host; and equal mapping
  calls in such launches mark two images that place their data alike.
  Comparing two arms one position against one also marks an image whose
  profile is only moved, which address randomization hides.

## Next

The control these readings suggest, for `compute-regression`, before the
comparison: launch each arm of a kernel once at one worker without address
randomization and compare the mapping calls the two make; when they differ,
launch both arms at the 32 positions and compare the two in order of time;
when a rank is unlike, launch both arms at the 32 positions again, and set the
kernel's comparison aside when the second round has a rank unlike in the same
direction as the first.

Its confirmation, written before the run, with the control in the instrument
and several hosted jobs:

1. With the stop-signals B and S images as the two arms, on every job whose
   host is an EPYC 7763, three such jobs at least, `stencil`'s arms are
   reported to map differently and its comparison is set aside.
2. With one image as both arms, under two directories whose names differ in
   length, every kernel's arms are reported to map alike on every job, for B,
   for S and for the branch's own image.
3. With B and S as the arms, no kernel is set aside on any Xeon 6975P-C job.
4. With one image as both arms, launching both at the 32 positions sets no
   kernel aside on any job, for B and for S. The comparison does not read the
   position a time came from, so this is also the reading of an image against
   itself moved by any number of positions.

A 7763 job that does not set `stencil` aside, equal arms reported to differ,
a kernel set aside on a Xeon 6975P-C job, or a kernel set aside between one
image's two series rejects the control as designed. Which kernels other than
`stencil` are set aside on the AMD jobs is recorded without a criterion.

## Confirmation

The control as `tests/performance/data-placement.sh` implements it ran on
2026-11-25 in two runs of a temporary workflow, ten hosted jobs each:
Whitefoot runs 45686397302 (head `84d25869c`) and 45686691164 (head
`a3b54bcfa`); the workflow is kept in this branch's history at `74ef113b1`. Each
job built the compilers of B, S and the branch, their images with the branch's
`tests/performance`, and ran the lines `compute-regression.yml` runs with only
the directories changed: the control and then the comparison of B against S,
the control of each of the three images against a copy of itself under a
longer directory name, and the qualification of B and of S. The jobs drew 3
EPYC 7763, 3 EPYC 9V74, 2 EPYC 9V45 and 12 Xeon 6975P-C hosts, on Linux 7.0.

The instrument at the two heads differs from the one that merges only in a
parenthesized `printf` argument of `positions.awk` (`84d25869c`) and in
`layout.awk` dropping a space before a resumed call's closing parenthesis.
None of the 1,600 traces has a resumed call: read again with the merging
`layout.awk` every trace gives its job's record byte for byte, and the
merging `positions.awk` gives every one of the 160 control directories its
`verdict.txt` and `aside.txt` byte for byte.

Each criterion holds:

1. On 3 of 3 EPYC 7763 jobs B and S record all five kernels differently and
   `stencil` is set aside: 15, 15 and 15 ranks unlike upward in the first
   round, 14, 14 and 15 in the second.
2. Each of the 60 controls of one image against its copy (B, S and the branch
   on 20 jobs) records all five kernels alike, and so does each of the 40
   qualifications' records of an image against itself.
3. On 12 of 12 Xeon 6975P-C jobs no kernel is set aside: no rank of any
   kernel is unlike, and the quotients lie between 0.977 and 1.030.
4. None of the 40 qualifications of B and of S sets a kernel aside. Of their
   200 kernel comparisons 7 launched a second round, 1 on a 7763 and 6 on a
   9V45, and none was set aside after it. Every qualification also recorded
   the image at position 1 against itself as mapping differently for all five
   kernels. That record differs by the runner's own idle mapping in any case;
   without that one line each of the 200 records still differs, every later
   mapping lying 2 MiB lower, and the qualification has required this since
   the confirmation.

Without a criterion:

- On 3 of 3 EPYC 9V74 jobs `stencil` is set aside, at 14 and 14, 14 and 14,
  and 18 and 27 unlike ranks; the last has ranks unlike downward too.
- On the EPYC 9V45 (Zen 5) S's `stencil` also has two levels: in each of its
  four rounds 15 of 32 positions lie 6 to 17 percent above the arm's median,
  against at most two of B's. That straddles the line, so one 9V45 job judged
  it (6 ranks unlike, then none) and the other set it aside (12, then 1).
- On the second 9V45 job `fir` was set aside on one slow launch of S in each
  round, at position 10 (1.76 times the arm's median) and at position 30 (1.18
  times). A placement repeats at its position, so this is noise that the rule
  reads as placement. `records` was launched again on three B/S comparisons
  and set aside on none.
- The comparison of B and S judged without the list, as it did before the
  control, fails on 2 of the 20 jobs, both 9V74 (`stencil` adverse at one and
  two workers, and at one and four), and reads `stencil` a suspect at two
  workers on one 7763. With the list each comparison exits 0, prints as not
  judged the rows of exactly the kernels set aside (`stencil` on all 6 Zen 3
  and Zen 4 jobs and on one 9V45, `fir` on that 9V45), and no judged kernel
  fails. Each job that set a kernel aside shows a warning
  `comparison: not judged: KERNEL (data placement)` and its summary holds,
  under `data`, both arms' 32 sorted quotients for each round. The
  comparison that read the list then was `compare.sh`; the instrument has
  since judged each kernel in `judge.sh`, which reads the list instead and
  does not time a kernel set aside. That reading did not run here; it is
  confirmed below.
- `strace` 6.8 is on the runner image: on no job did the workflow install it.

What each unit of work took, in seconds, the longest run of its label on each
model with the number of runs, and the budget the table's rule (1.1 times the
maximum plus its distance above the median of the runs, read from the same job
logs) gives it:

| label | EPYC 7763 | EPYC 9V74 | EPYC 9V45 | Xeon 6975P-C |
|---|---|---|---|---|
| `performance-data-layout` | 3.00 (24), 10 | 3.10 (24), 10 | 2.20 (16), 10 | 2.20 (96) |
| `performance-data-round-mandelbrot` | 13.62 (10), 15 | 12.92 (9), 15 | 10.11 (6), 15 | 9.41 (36) |
| `performance-data-round-records` | 12.41 (10), 15 | 12.22 (10), 15 | 8.61 (9), 10 | 9.41 (36) |
| `performance-data-round-fir` | 12.61 (9), 15 | 11.21 (9), 15 | 6.71 (8), 10 | 7.71 (36) |
| `performance-data-round-quadrature` | 10.41 (9), 15 | 8.61 (9), 10 | 5.71 (8), 10 | 5.71 (36) |
| `performance-data-round-stencil` | 50.96 (12), 60 | 60.08 (12), 80 | 37.15 (9), 45 | 35.75 (36) |

The table has no Xeon 6975P-C column, so that model takes each row's largest
value, which covers its runs; the Xeon 8573C column, which no job reached,
takes it too. The control's step for B against S took 63 to 67 s on the Xeon
jobs and 100 to 173 s on the AMD jobs. A run of a pull request whose arms map
alike executes only the record.

### The judgment's reading of the list

Since the confirmation the instrument judges each kernel in `judge.sh`, which
reads the control's list and does not time a kernel set aside. A second
temporary workflow runs, on twelve hosted jobs, the lines
`compute-regression.yml` runs with only the directories changed: the control
of B against S, the judgment of B against S with the control's results, and
the publication. B's and S's images differ for every kernel, since S's runtime
starts the receiver thread. Its criteria, written before the run:

1. On every job the judgment times exactly the kernels the control did not
   set aside, and its verdict has one line `not judged: KERNEL (data
   placement)` for each kernel set aside and none for another.
2. On every EPYC 7763 or 9V74 job, of which the run has one at least,
   `stencil` is set aside, the job shows the warning
   `judgment: not judged: stencil (data placement)`, and its summary holds,
   under `data`, both arms' 32 sorted quotients for each round.
3. On every Xeon 6975P-C job no kernel is set aside and all five are timed.
4. The judgment's exit status is 0 or 3 unless its verdict fails a timed
   kernel; what it decides of the timed kernels is recorded without a
   criterion.

A kernel set aside and timed, a `not judged` line for a kernel not set aside,
or a kernel not set aside and not timed rejects the reading as implemented.

## Limitations

- One run of eight jobs on one day, three processor models. The pool also
  holds EPYC 9V45 and other Xeon models, which this run did not reach, and
  the earlier probe's levels on the 9V45 and the Xeon 8370C are not explained
  here. The confirmation reached two 9V45 jobs and no other Xeon model.
- Only `stencil` was traced and launched by position in that run; the other
  four kernels were timed in the campaigns alone. The confirmation launched
  all five by position.
- The control's rule reads one slow launch in each round, at any positions,
  as placement: it set `fir` aside so on one 9V45 job of the confirmation.
  A kernel set aside that way is not judged on that run.
- The position rule, the comparisons of two arms' positions, one against one
  and in order of time, and the comparison of mapping calls are post hoc. S
  has one series a job, and the only two series of one profile are B's and
  this branch's.
- A placement that costs less than 10 percent, as adjacent grids do on the
  7763, is below the line every reading here uses.
- The 32 positions walk the 64 MiB that glibc's heap alignment makes matter.
  A layout whose time depends on a longer period, as adjacent grids on one
  heap do, is only partly covered.
- Mapping calls show where blocks of a page or more are mapped, not where a
  small block lies inside a heap.
- `layout.py` does not model two threads before the entry, the order of S
  under `thread`.
- A program on these processors pays the same cost outside the regression
  check: whether two of its large blocks collide is decided at each launch.
  Nothing here addresses that.
