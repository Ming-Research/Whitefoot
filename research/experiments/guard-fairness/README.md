# Guard fairness probe

`probe.wf` is firn's guard-fairness probe, copied unchanged from Firn-wf
branch `exp/engine-probe`, `research/experiments/guard-fairness/probe.wf`
(Firn-wf's README there describes the instrument): N spawned contexts each
take one `Shared<State>` K times with `atomic p = &state when p^.out == 0`,
hold it for 4,096 dependent mixing steps, and release it; it prints the
exact p50, p99 and longest acquisition delay over all N × K acquisitions,
the mean hold and the run time.

`run.sh` compiles it with the candidate compiler (`WFC`) and, when
`BASE_WFC` is set, with the base compiler, then runs both interleaved at
N = 2, 8 and 50 under `WF_DRIVERS=2`, pinned to `CPUS` (default 2,4, two
physical performance cores of the native i9-14900K), reversing the order on
even passes. The question and the criteria it answers are in
[the investigation](../../investigations/guard-fairness/DESIGN.md). Timing
runs only on the 14900K, through CI.
