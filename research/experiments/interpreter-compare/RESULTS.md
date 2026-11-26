# Results

Each entry carries the run id it was read from.

## Hosted runner: the instrument runs (run 45677950409)

A run of the temporary workflow `tmp-interpreter-compare` (removed with this
work) on commit 513b2f2ce, pushed on 2026-11-25 at 18:42 UTC to a `wf-try`
branch of this work. Runner: GitHub's ubuntu-24.04, AMD EPYC 7763, Linux
7.0.0-1017-azure, clang 22.1.8. Every step succeeded. This host is not the
14900K and shares its machine, so the run shows that the stages work and
decides no ratio and nothing about the 14900K.

- The `inputs` and `selftest` stages passed, the Linux-only launch checks of
  `interp.py self-test` included. The step that restores the old `timeout`
  launch line into a copy of `interp.py` printed "the self-test fails with the
  old launch, as it must".
- `time`, 20 iterations, 2 rounds, `--trial`, CPU 1: the base's CoreMark took
  0.046 s; the candidate/base ratio was 0.9119 and the twin/base ratio 0.9431;
  no interval and no verdict (unresolved, "no floor given", fewer than six
  rounds). The residual over 6 runs: median 0.0004 s, maximum 0.0005 s, not
  checked against a limit. The manifest holds one digest for `image v2h-base`
  and `image v2h-twin`, 8be9a407fe60a0e6bf3ad69b27bcd20d8df01f8d537cbc388cc6c5566a8b49de;
  the candidate image differs (1bd4b129...).
- `sample`, 20 iterations: 397 samples of the base and 390 of the candidate.
  `[unknown]` held 265 and 255 of them (66.751% and 65.385%), samples outside
  every handler 19 and 28 (`wf_box_array_filled`). Handler rows, base and
  candidate: `I32LoadSD` 10 and 13 (2.519% and 3.333%), `BrTableN` 7 and 4,
  `I32LoadD` 7 and 3, `Call` 5 and 5, `I32AddSD` 5 and 5. Most samples of a run
  this short are not in a handler; the cause of the `[unknown]` share was not
  examined.
- `wallclock`, 3 launches per stand-in and chain, minimum / median / maximum
  wall time in seconds, and the median of wall time minus CPU time:

  | stand-in | chain | wall | residual |
  | --- | --- | --- | --- |
  | sleeper (0.21 s, no CPU) | bare | 0.2120 / 0.2126 / 0.2128 | 0.2106 |
  | sleeper | taskset | 0.2133 / 0.2133 / 0.2134 | 0.2105 |
  | sleeper | taskset+timeout | 0.2142 / 0.2144 / 0.2145 | 0.2105 |
  | counter (CPU bound) | bare | 0.3916 / 0.3919 / 0.3942 | 0.0004 |
  | counter | taskset | 0.4151 / 0.4207 / 0.4437 | 0.0005 |
  | counter | taskset+timeout | 0.3952 / 0.4159 / 0.4173 | 0.0004 |

  On this host the sleeper's wall time is its own time under every chain, with
  no step of 0.1 s, and the counter's residual is half a millisecond or less. It
  says nothing about the 14900K's `timeout`.

## Not recorded yet

These need the 14900K, run from `main` by `compiler-compare.yml` (group
`wallclock`, then `interpreters,perf`) and `interpreter-compare.yml`, and are
left to a later work:

- the confirmation of what put the earlier `compiler-compare` wall times on a
  0.1 s grid: the 14900K's `timeout` and `taskset` (versions and paths) and the
  walls of the sleeping and counting stand-ins under each launch chain; the
  `wallclock` group and its stage are removed once this is recorded;
- the residual limit, chosen from the counting stand-in's wall minus CPU time
  in that run, and a separate later run judged against it;
- the noise floors measured by null runs: `compiler-compare.yml` (fib and
  CoreMark) and `interpreter-compare.yml` (CoreMark), each with the settings it
  holds for.

## Before the fix

The first `compiler-compare` rounds on the 14900K read all 30 fib launches at
0.302 to 0.303 s and all 30 CoreMark launches at 0.402 to 0.403 s, while fib's
user plus sys time was about 0.21 s. Their spreads, floors and "no change"
verdicts are not evidence.
