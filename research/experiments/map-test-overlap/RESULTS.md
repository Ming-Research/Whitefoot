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

## Results

Not yet measured.
