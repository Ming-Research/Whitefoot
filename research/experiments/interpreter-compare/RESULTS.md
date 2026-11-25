# Results

Nothing is recorded yet. This file holds, with the run id of each entry:

- the confirmation of what put the earlier `compiler-compare` wall times on a
  0.1 s grid: the 14900K's `timeout` and `taskset` (versions and paths) and the
  walls of the sleeping and counting stand-ins under each launch chain
  (`compiler-compare.yml`, group `wallclock`);
- the residual limit, chosen from the counting stand-in's wall minus CPU time
  in that run, and the later run that was judged against it;
- the noise floors measured by null runs: `compiler-compare.yml` (fib and
  CoreMark) and `interpreter-compare.yml` (CoreMark), each with the settings it
  holds for.

Evidence from before the fix: the first `compiler-compare` rounds on the 14900K
read all 30 fib launches at 0.302 to 0.303 s and all 30 CoreMark launches at
0.402 to 0.403 s, while fib's user plus sys time was about 0.21 s. Their
spreads, floors and "no change" verdicts are not evidence.
