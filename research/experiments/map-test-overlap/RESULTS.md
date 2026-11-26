# The concurrent map test: overlaps that no longer depend on the scheduler

Registered before any measurement. The measured results follow the registration
below, each under the heading of its criterion.

## Question

`compiler/src/backend/concurrent_map_test.c` has three assertions that need a
second thread to have acted at least once, and each used to end the first
thread's work after a fixed count, without waiting for the second:

1. `entries_held`: the holder thread never completed a hold ("the holder never
   held the map") when the counting threads finished their statements first.
2. `entries_shared_reads`: no reader read the key ("no reader read the key")
   when the writers finished their statements first.
3. `reserves_release_beside_swaps`: no release freed a reserve ("no release met
   a reserve beside the swaps") when the swapping thread finished its 2,000
   rounds first.

The change keeps the first thread running until the second has acted, with a
10 s limit that fails with the assertion's own message, and adds a
single-threaded test that makes a release fall inside a swap.

Does each of the three assertions fail on the code before the change under
load on hosted runners, and does it never fail after the change?

## Comparison and criteria

All runs are on GitHub-hosted `ubuntu-24.04` and `macos-15`, clang, the test's
own compile line (`-O2 -g -Wall -Wextra -Werror -Wpedantic -pthread`).

- C1, before: the test built from the merge commit before the change
  (`04ca5968e`), 50 runs in each of four copies running at once, per host. The
  diagnosis of an assertion is rejected for this host if no run of the 200 shows
  its message. In that case the assertion is reported as derived from the code
  alone.
- C2, after: the changed test, 50 consecutive runs and 50 runs in each of four
  copies at once, per host. Any failure rejects the change.
- C3, swap control: the swap's locks deleted from `wf_cmap_swap`. The new
  `release-inside-swap` run must fail in 5 of 5 runs in a plain, an
  AddressSanitizer plus UndefinedBehaviorSanitizer and a ThreadSanitizer build,
  and `release-beside-swaps` run alone in 5 of 5 runs under each sanitizer. The
  unpatched sources must pass every one of these runs. A shortfall of the
  threaded run is a finding, not an adjusted number.
- C4, inert-thread control: the reader threads, the holder thread and the
  releasing thread each made to return at once must make the test exit 1 within
  the 10 s limit with the assertion's message; the same build with none made
  inert passes.
- C5: each sanitizer build's whole run, plus 10 s, stays under the test's 120 s
  alarm.
- C6, late-start control: registered after the first pass of C1 (its counts
  were small) and before C6 was measured, because a C1 that shows no failure
  does not show whether the old assertions depend on the scheduler at all. One
  thread (the reader, the holder or the releasing thread) starts its loop
  1.5 s late, in the sources before the change and in the sources after it, 5
  runs each. The old sources must fail 5 of 5 with the assertion's own message;
  the changed sources must pass 5 of 5.

## Results

The runs are GitHub Actions run 45731107042 on commit `0720ae758`, from the
temporary workflow `tmp-map-overlap.yml` with `.github/tmp-map-overlap.sh`
(both removed before the full check). The sources of the map and its test are
the same on the head: the main merges since touch other files. An earlier
pass of the same workflow (run 45729541249, counts 3 and 2) gave the same
calibration and C3 results and its inert control did not link: the binary was
written to the path of its own source tree. The script was fixed and C4 is
from the second pass.

### C1, before (sources of `04ca5968e`)

| host | 50 consecutive runs | 200 runs, 4 copies at once | messages |
| --- | --- | --- | --- |
| ubuntu-24.04 | 0 failed | 0 failed | none |
| macos-15 | 0 failed | 2 failed | `no release met a reserve beside the swaps (rounds) (2000, 0)`, twice |

On macOS the third assertion failed twice in 200 loaded runs, so its diagnosis
stands there. The first two assertions did not fail on either host, and the
third did not on Linux: by C1's own rule these are derived from the code, and
C6 is what shows that they depend on the scheduler. The one failure of the
item's history known by its message was the reader's, on macOS; the loaded
runs did not reproduce it, so they do not say how often it occurs.

### C2, after

| host | 50 consecutive runs | 200 runs, 4 copies at once |
| --- | --- | --- |
| ubuntu-24.04 | 0 failed | 0 failed |
| macos-15 | 0 failed | 0 failed |

C2 holds. Its power is the C1 rate: on macOS 2 failures of 200 loaded runs
before and 0 after, on Linux 0 and 0.

### C3, swap control (the swap's locks deleted)

Both hosts, 5 runs each. `release-inside-swap`: plain, ASan+UBSan and TSan
builds failed 5 of 5 with `a release inside a swap freed a reserve (bytes)
(1024, 0)`; the unpatched sources failed 0 of 5 in each. `release-beside-swaps`
alone: ASan+UBSan failed 5 of 5 (a double free or a heap use after free) and TSan
failed 5 of 5 (a data race); the unpatched sources failed 0 of 5 in each. C3
holds on both hosts, the threaded run reaching 5 of 5 under each sanitizer.

### C4, inert-thread control

Both hosts. Reader threads returning at once: exit 1 after 11 s (Linux) and
13 s (macOS), `no reader read the key (reader 0) (0, 1)`. The holder returning
at once: exit 1 after 10 s and 11 s, `the holder never held the map (0, 1)`.
The releasing thread returning at once, `release-beside-swaps` alone: exit 1
after 10 s on both, `no release met a reserve beside the swaps (rounds) (0, 1)`.
The same build with none made inert passed in 1 s and 5 s. C4 holds.

### C5, time

One run of each build, whole binary: ubuntu-24.04 plain 2 s, ASan+UBSan 7 s,
TSan 24 s; macos-15 plain 4 to 5 s, ASan+UBSan 14 s, TSan 45 s. The
largest plus 10 s is 55 s, under the 120 s alarm. C5 holds.

### C6, late-start control

Both hosts, 5 runs each. With the reader started 1.5 s late the old sources
failed 5 of 5 with `no reader read the key (0, 0)`; with the holder, 5 of 5
with `the holder never held the map (0, 0)`; with the releasing thread, 5 of 5
with `no release met a reserve beside the swaps (rounds) (2000, 0)`. The
changed sources passed 5 of 5 in each case. C6 holds: each old assertion
fails whenever the second thread arrives after the first has finished, and
the changed test waits for it.

## Limits

- What was measured is a hosted runner's load by four copies of the test, not
  the load a failed pull request saw; the first two assertions were never seen
  failing without the injected delay.
- The `before` sources are from `04ca5968e`, which lack the runtime and test
  changes of later main merges; those do not touch the three assertions.
- The 10 s limit was never reached by a passing run, and no run measured how
  long the changed threads continue past their quota; the late-start control
  shows they continue at least 1.5 s.
