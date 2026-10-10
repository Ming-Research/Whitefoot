# Context starvation witness

## Question and prediction, before running

Can a context's finite pure computation delay another context's 100 ms timer
and output on one CPU? The proposed missing capability is a bound on that
delay, or a way to isolate computation from the driver that serves waiting
contexts. This is **not a specification violation**: WAIT-2 in the active
[specification](../../../spec/kernel-spec.md) promises eventual progress under
its stated conditions, not a latency bound. This experiment selects no
language or runtime change.

The current [driver design](../../../design/compiler/waiting-contexts.md)
runs contexts cooperatively, queues a spawn on its starter's driver, and lets
idle drivers steal ready contexts. The witness retains the draft's serial
multiply/wrap/add/remainder recurrence with an invocation-supplied iteration
count. It has no wait inside that loop and publishes the full checksum so the
computation remains observable. Compilation uses the default native path,
without `--par`.

The prediction is recorded before compilation or measurement:

| Case | CPU affinity | Requested drivers | Computation | Predicted timer output from process start |
| --- | --- | --- | --- | --- |
| (a) | CPU 0 | 1 | approximately 2 s | delayed until computation ends, approximately 2 s |
| (b) | CPU 0 | 2 | the same iteration count | approximately 100 ms |
| (c) | CPU 0 | 1 | zero iterations | approximately 100 ms |

In (a), observing the timer near 100 ms while computation continues for about
2 s would refute the predicted starvation. In (b), timer output delayed until
computation ends would refute the predicted benefit of a second driver for
that run. In (c), a large delay would undermine the control and the attribution
to computation. A compile failure, malformed output, nonzero exit or timeout
is an unsuccessful experiment, not evidence for a language rejection or a
timing prediction.

The (b) prediction is conditional: two drivers on one CPU are OS-time-sliced,
but neither WAIT-3 nor the runtime design guarantees that the worker will be
stolen away from the driver holding the timer. If that driver takes the worker
itself, the other driver cannot steal its running context or service its
parked timer. Record such observations rather than retrying until a preferred
schedule appears. Also, `wf_drivers_begin` in the
[completion runtime](../../../compiler/src/backend/completion/bridge.c)
can retain just one driver when a kernel ring or additional driver cannot be
created. The table records the requested setting, not an observed thread
count; the host record includes `io_uring_disabled`, but does not establish
successful driver creation. A delayed (b) alone cannot distinguish failed
driver creation from placement on the timer's driver. The one-driver
prediction assumes the timer reaches its wait before its deadline.
These are exploratory observations on a shared
GitHub runner, not precise performance measurements or a latency guarantee.

## Witness and observations

`witness.wf` accepts exactly one argument, an unsigned decimal iteration count
of one through twelve digits. Zero performs no recurrence steps. Invalid
arguments exit with status 2; failed directory closure exits with status 3;
failed timer/output operations or short writes exit with status 1.

After closing both linear directory handles through the current standard
library, the entry makes a separate factory handle and moves it and stderr
into the compute context. The entry sleeps to a deadline 100 ms after a clock
read immediately before the spawn, matches `Result<unit, unit>`, then writes
`T` to stdout. The compute context writes `C` followed by eight little-endian
checksum bytes to stderr after the loop. The remaining invocation objects
are droppable under the current standard interfaces; no invented stream or
clock close operation is needed.

The timer and its output are in a helper. The parent first names the spawned
result **after** that helper returns. This matters because WAIT-3 also joins
before a statement containing an edge out of the binding's block: an inline
match with an early return could otherwise join the worker before output.
The two contexts own distinct output streams and factory handles, so output
does not require a loan of storage owned by the other context.

`run.sh` compiles once with the built compiler, then timestamps each pipe's
first byte using Python's monotonic clock and unbuffered multiplexed reads.
The table records timer output, computation-end output and process exit,
all measured from immediately before launching `taskset -c 0`. These are
external receipt times: they include launch, output and observer scheduling
latency, and the compute marker includes checksum encoding and writing.
They are not internal instruction timestamps; nearly coincident markers do
not establish their internal order. The observer is not pinned to CPU 0.
`WF_WORKERS=1` is fixed in every child; only the driver count changes.

Calibration starts with zero work and three 100,000-iteration samples, prints
their spread, and increases the count by four until median compute-marker
latency reaches 200 ms (at most eight groups). It scales that count once to
target about 2 s, then runs (a), (b), (c) in that order for each of three passes,
printing every row, including calibration. The same nonzero count is used for
both driver settings. A missed 1–4 s calibration range is annotated rather
than hidden. Equal-input checksums must agree; zero work must produce 1.
This consistency check keeps the work observable but is not an independent
oracle for every nonzero recurrence. Missing output and a 30 s process limit
fail the protocol; timing differences themselves do not select success.

## Run on CI

The temporary [workflow](../../../.github/workflows/ctx-starvation.yml) runs
only on pushes to `claude/ctx-starvation-witness`, on `ubuntu-24.04`. It installs
Clang and LLD, fetches locked Rust dependencies, and builds the compiler using
`make -C compiler build`, as the existing
[I/O workflow](../../../.github/workflows/io-bench.yml) does. It then invokes:

```sh
OUT="$RUNNER_TEMP/context-starvation" sh research/experiments/context-starvation/run.sh
```

`WFC` can name an already-built compiler (default:
`compiler/target/gate/whitefootc`); `OUT` holds the native witness and
`results.tsv`. CPU 0 must be in the runner's allowed affinity; the script
fails explicitly otherwise. The workflow prints the table and records the
revision, host, tool versions and environment with its artifacts. Compiler
construction is separate from program timing. This is an explicitly requested
research run, separate from the canonical gate. Remove the temporary workflow
before opening any pull request; retain this bundle while the scheduling
question needs a reproducible witness, removing it when superseded.

## Results

### Run 1: revision 193a4a4cc, 2026-10-10

[CI run 38031719816](https://github.com/Ming-Research/Whitefoot/actions/runs/38031719816),
GitHub-hosted `ubuntu-24.04`, AMD EPYC 9V74 guest with 4 vCPUs, Linux
6.17.0-1022-azure, `io_uring_disabled` 0, Clang/LLD 18.1.3, Rust 1.99.0;
`taskset -c 0`, `WF_WORKERS=1`, no `--par`. Calibration selected 595,534,350
iterations. Rows, times in seconds from launch:

| Case | Pass | Drivers | Timer `T` | Compute `C` | Exit |
| --- | --- | --- | --- | --- | --- |
| a | 1 | 1 | 1.999203 | 1.999197 | 1.999655 |
| b | 1 | 2 | 2.016608 | 2.016587 | 2.017142 |
| c | 1 | 1 | 0.102211 | 0.001909 | 0.102689 |
| a | 2 | 1 | 1.975358 | 1.975351 | 1.975827 |
| b | 2 | 2 | 1.970383 | 1.970359 | 1.970889 |
| c | 2 | 1 | 0.102242 | 0.002010 | 0.102685 |
| a | 3 | 1 | 2.000616 | 2.000610 | 2.001088 |
| b | 3 | 2 | 1.984630 | 1.984623 | 1.985137 |
| c | 3 | 1 | 0.102119 | 0.001830 | 0.102593 |

All checksums for equal inputs agreed and zero work produced 1. The
calibration rows show the same pattern at every count whose computation
exceeded 100 ms: at 102,400,000 iterations the timer arrived at 0.343 s,
within 10 microseconds of the compute marker.

**Interpretation.** (a) matches the prediction: the 100 ms timer's output
waited for the whole computation in every pass. (c) matches: with no
computation the timer fires at about 102 ms. (b) refutes the predicted
benefit of a second requested driver: the timer still waited for the
computation. As recorded above, this run cannot tell whether a second
driver existed, or whether it existed but could not service a timer parked
on the driver running the computation. Run 2 records the witness's thread
count to separate the two.

### Run 2: thread count, prediction before running

`run.sh` now also records the number of threads of the witness process 50 ms
after launch (`threads_50ms`, from `/proc/<pid>/task`). If (b) shows one more
thread than (a), a second driver existed and the delay comes from placement
or from completion harvesting bound to the busy driver; if (a) and (b) show
the same count, the second driver was not created on one allowed CPU and (b)
says nothing about two drivers.

### Run 2: revision ca09743ed, 2026-10-10

[CI run 38031953013](https://github.com/Ming-Research/Whitefoot/actions/runs/38031953013),
same host type and settings as run 1; calibration selected 520,900,271
iterations.

| Case | Pass | Drivers | Timer `T` | Compute `C` | Exit | Threads at 50 ms |
| --- | --- | --- | --- | --- | --- | --- |
| a | 1 | 1 | 1.990566 | 1.990558 | 1.991013 | 2 |
| b | 1 | 2 | 1.990233 | 1.990226 | 1.990770 | 3 |
| c | 1 | 1 | 0.102386 | 0.002080 | 0.102847 | 3 |
| a | 2 | 1 | 1.990189 | 1.990163 | 1.990626 | 2 |
| b | 2 | 2 | 1.990627 | 1.990620 | 1.991129 | 3 |
| c | 2 | 1 | 0.102363 | 0.001987 | 0.102812 | 3 |
| a | 3 | 1 | 1.990058 | 1.990052 | 1.990480 | 2 |
| b | 3 | 2 | 1.989885 | 1.989879 | 1.990416 | 3 |
| c | 3 | 1 | 0.102438 | 0.002095 | 0.102938 | 3 |

Run 2 reproduces run 1: in (a) and (b) the timer waited for the computation
in every pass. The thread count does not settle the question it was added
for: (b) shows one thread more than (a), but so does the one-driver control
(c), which is parked on its timer at 50 ms. A third thread therefore appears
in a one-driver process that is waiting, and the count cannot attribute (b)'s
extra thread to a second driver. Which runtime thread that is, and whether
the spawned computation starts on the entry's driver before the entry parks
its timer, are questions for the runtime's owner; the observation that a
finite computation delays another context's timer by its full length, with
one or two requested drivers on one CPU, stands on both runs.

