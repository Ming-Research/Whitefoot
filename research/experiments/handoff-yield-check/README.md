# Handoff and yield-check measurements

Serves `research/investigations/driver-handoff/DESIGN.md`, section "Plan for
the 14900K measurements": the question, the comparisons and the criteria are
written there, before any run. `run.sh` takes two compilers, built from the
base and the candidate revision, and runs items 1 to 3 on one Linux host:

- the ready context of `tests/programs/ready_beside_computations.wf` at one
  and two drivers and the polling shapes of `tests/programs/lone_timer_writer.wf`
  and `tests/programs/poll_contexts.wf`, base and candidate interleaved run by
  run (`latency.py`, which writes `latency.tsv` and prints a summary);
- the fixed cost of an uncontended atomic statement, by
  `research/experiments/shared-take-overhead/run.sh` on one and two P-cores,
  against `c_reference.c`, the same per-operation work in C with a plain
  counter and with a spin lock around it.

Item 4, Firn's throughput, is `redis-bench.sh compare` of
`research/experiments/io-completion-bench/`, which records the scheduler's
counters of each measured pass in `compare-sched.txt`. The record copies
every number it relies on with its run id, since job logs and artifacts
expire.
