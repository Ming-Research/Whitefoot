# Data placement and the regression comparison

A change that leaves a compute kernel's generated code unchanged can still
move where the kernel's stack and heap blocks are mapped, and on some hosted
processors that alone changes the kernel's time. This investigation asks
which move of data alone, applied to one image, shows that a host times a
kernel by where its data lies, so that `compute-regression` can run such a
move as a control before it gives a verdict.

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

The traces' other mappings suggest how, as a reading of six traces that the
experiment below tests. The entry runs on a thread with a 1 GiB stack, and its
first allocation makes glibc reserve a heap for the thread: 128 MiB mapped,
then trimmed to the 64 MiB that starts on a 64 MiB boundary. A grid then goes
to the highest hole that holds it.

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
predicts a line's way from a hash of its address, and the hash recovered for
Zen, Zen+ and Zen 2 pairs bit 12 with bit 27 (bits 12 to 27, folded as 12 with
27, 13 with 26, 14 with 25, 15 with 20, 16 with 21, 17 with 22, 18 with 23 and
19 with 24: Lipp et al., "Take A Way: Exploring the Security Implications of
AMD's Cache Way Predictors", ASIA CCS 2020, section 3). If Zen 3 and Zen 4
keep that hash, the two lines look alike to the predictor and each access
evicts the other's prediction, on every cell. This is inferred from one trace
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
