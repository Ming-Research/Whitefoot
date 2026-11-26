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
- C7, controls of the tests added after the first hand-in: registered at the
  test review's request, before it was measured. The tests are
  `swap_hook_runs_once_under_the_locks`, `reserves_refused_inside_a_self_swap`
  and `owed_is_bounded`, each with its own focused path in the test
  (`swap-hook`, `release-inside-self-swap`, `owed-bound`). Each path must pass
  5 of 5 runs on the sources as they are. Then, on one copy of the sources for
  each wrong behaviour, 5 runs must fail 5 of 5, with the test's own message:
  the hook after the exchange began; the hook before the locks are taken; the
  hook run twice; the hook never run (all four on `swap-hook`); the swap's locks
  deleted (on `release-inside-self-swap`, and on `swap-hook`); `owed`'s 10 s
  bound made 100 s; the early return of `owed` for an act already made
  deleted (both on `owed-bound`). A run that fails for another reason, by
  timeout or at compile time, does not count.

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
| macos-15, second pass (run 45748160597) | 0 failed | 5 failed | the same message, five times |

On macOS the third assertion failed twice in 200 loaded runs, and five times
in 200 in a second pass of the same step, so its diagnosis stands there. The first two assertions did not fail on either host, and the
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

### C7, controls of the tests added after the first hand-in

Run 45748160597 on commit `d8879bb2c`, on ubuntu-24.04 and macos-15 (the
workflow's other steps ran on this head as well, with the same results as
above on both hosts: C2 0 of 50 and 0 of 200, C3 5 of 5 and 0 of 5, C4, C6; C1
as in its table). The table is the same on both hosts: every count and every
message below was the same on ubuntu-24.04 and on macos-15.

| path | sources as they are | wrong behaviour | runs failed | message |
| --- | --- | --- | --- | --- |
| `swap-hook` | 0 of 5 failed | hook after the exchange began | 5 of 5 | `a swap's hook ran after the exchange began` |
| `swap-hook` | | hook before the locks | 5 of 5 | `a swap's hook ran without a map's lock` |
| `swap-hook` | | hook run twice | 5 of 5 | `a swap did not run its hook once (pair, calls) (0, 2)` |
| `swap-hook` | | hook never run | 5 of 5 | `a swap did not run its hook once (pair, calls) (0, 0)` |
| `swap-hook` | | swap's locks deleted | 5 of 5 | `a swap's hook ran without a map's lock` |
| `release-inside-self-swap` | 0 of 5 failed | swap's locks deleted | 5 of 5 | `a release inside a swap of a map with itself freed a reserve (bytes) (512, 0)` |
| `owed-bound` | 0 of 5 failed | bound 10 s made 100 s | 5 of 5 | `a wait 11 s old for an act did not end the test with exit status 1 (signal or status + 1000) (1007, 0)` |
| `owed-bound` | | early return deleted | 5 of 5 | `a wait for an act already made asked again or started its clock (answer, calls) (1, 0)` |

C7 holds on both hosts: each wrong behaviour is caught by its own test with its own
message. Two behaviours are not covered: a swap that takes the lock of a map
swapped with itself twice would wait for ever and end at the test's 120 s alarm
instead of a message, and a bound made 11 s instead of 10 s is not told apart
by the child that is 10 s old, whose integer-second difference may or may not
reach 11. The whole binary with the three tests in it ran, on ubuntu-24.04, in
2 s (plain), 7 s (ASan+UBSan) and 23 s (TSan), and on macos-15 in 3 to 4 s,
14 s and 38 s.

## Limits

- What was measured is a hosted runner's load by four copies of the test, not
  the load a failed pull request saw; the first two assertions were never seen
  failing without the injected delay.
- The `before` sources are from `04ca5968e`, which lack the runtime and test
  changes of later main merges; those do not touch the three assertions.
- The 10 s limit was never reached by a passing run, and no run measured how
  long the changed threads continue past their quota; the late-start control
  shows they continue at least 1.5 s.
