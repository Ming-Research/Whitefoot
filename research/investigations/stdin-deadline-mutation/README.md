# Do the stdin deadline cases still catch an ignored deadline?

## The question

Two program cases read standard input with a 50 ms deadline while the test
harness writes one byte 400 ms later; the program must end the read at its
deadline with nothing read and leave the late byte for the next read [PRE-2]:

- `programs::stream::a_deadline_ends_a_read_of_a_silent_writer_on_both_routes`
  (the native ring route and the helper route),
- `programs::stream::a_deadline_ends_a_read_under_a_pool_pinned_at_zero`
  (the helper route with `WF_IO_HELPERS=0`).

Both run `tests/programs/stdin_deadline.wf`. On 2026-10-05 they failed on a
macOS host with exit status 10 (the bounded read received the late byte):
the harness counted the 400 ms from spawning the program, and a slow start
let the byte arrive before the read was queued. That was a harness timing
defect, not a runtime defect. Commit `bbc78ba09` changed the harness to count
from the program's own first output, a `.` written just before the read.

The synchronization changed, so the open question is whether the cases still
fail when the runtime really ignores the deadline, or whether the change
loosened them.

## The comparison and the criterion

Fixed before the runs:

- Control: the two cases on the work branch head `66041ea9e` must pass.
- Mutant: a commit whose only difference from that head is one added line,
  the first statement of `wf_bridge_cancel` in
  `compiler/src/backend/completion/bridge.c`:

  ```c
  if (record != NULL) return 0; /* mutant: ignore the passed deadline */
  ```

  `wf_bridge_cancel` is the one entry through which the driver asks the route
  holding an operation (the io_uring ring, the file adapter's queue or its
  helper thread, the stop route, the Windows completion port) to give it up
  once its deadline or cancellation watch has fired
  (`wf_driver_expire_wait`). Returning without acting leaves every route's
  read in place until its own outcome arrives.
- Both cases must fail on the mutant with the program's exit status 10.
  A case that passes on the mutant, or fails with another status, is too
  weak or fails for another reason, and is to be fixed.

No assertion of the cases was changed: the late byte reaching the second read
(exit statuses 13, 14 and 15, and the check of the byte `z`) is as before.

## Result (Linux, the owner's self-hosted runner, `fast.yml`)

The mutant commit was built off the work branch with git plumbing from the
head and the one-line change; it is on no branch. Its commit id was
`330793d8dc4d87dfd66d3f0c3c63abd6f0327e8d`; it can be rebuilt by adding the
line above after the opening brace of `wf_bridge_cancel` at the head.

| Commit | Run | Result |
| --- | --- | --- |
| `66041ea9e` (control) | fast run 45607279548 | both cases passed |
| `330793d8d` (mutant) | fast run 45607280963 | both cases failed |

The failures, as the runs reported them:

- `a_deadline_ends_a_read_of_a_silent_writer_on_both_routes` failed on its
  first route, `native ring: true`, with `stdout: "."`, empty stderr and
  `left: Some(10)`, `right: Some(0)`.
- `a_deadline_ends_a_read_under_a_pool_pinned_at_zero` failed with
  `stdout: "."`, empty stderr and `left: Some(10)`, `right: Some(0)`.

So the ring route (the first case's first route) and the helper route (the
second case) each end with status 10 when the deadline is ignored, and the
`.` in standard output shows the program reached the bounded read before the
byte was sent.

## Limits

- One mutation of the shared cancel entry, run on Linux. The first case stops
  at its first failing route, so its helper route is checked by the second
  case. macOS has no ring and takes the helper route, which is the code the
  second case ran; the mutant itself was not run on macOS.
- It shows the cases catch a deadline that is never acted on after the read is
  queued. It does not show they catch every deadline defect, for example one
  that cancels too early.
- The record is one run each, not a spread; the outcomes are exit statuses,
  not timings.
