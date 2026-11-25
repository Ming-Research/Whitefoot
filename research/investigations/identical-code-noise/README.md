# Identical code judged slower: what the retained compute-regression artifacts show

Question: when both arms of the paired comparison have the same code, how often
does the comparison fail, and why? Data: the artifacts of the 187
`compute-regression.yml` runs of 2026-10-13 to 2026-11-25 that still had one
(`historical-runs.txt`, made by `summarize-artifacts.sh`), plus run 38389055091
(Whitefoot#409).

Facts read from the artifacts (2026-11-25):

- In no run were the two arms' linked executables byte-identical (0 of 187),
  though in 172 runs all five emitted module objects (`<kernel>.o`), oracle and
  runner objects were byte-identical. Cause, seen in run 38389055091: the object
  `native/keyed_table.o` embeds the source path through `assert`'s `__FILE__`
  (`/home/runner/work/_temp/baseline/compiler/src/backend/concurrent_map.c`
  against `/home/runner/work/Whitefoot/Whitefoot/compiler/src/backend/concurrent_map.c`);
  119 bytes of it differ, the strings of the merged read-only data after it move by
  5 bytes, and `objdump -d` of the two `stencil` images differs in 188 lines, 185 of them
  rip-relative `leaq` displacements into `.rodata`, the other 3 printing the same
  text; every instruction address is equal.
- All 4 comparison failures (records 2, stencil 2) and both null-control failures
  (records 1, stencil 1) among the 187 runs fell in runs whose five module objects
  were identical. No comparison failed in the 8 runs whose module objects all
  differed.
- In the null control (identical images as both arms), 60 of 183 runs had at
  least one adverse cell (wall < 0.97, 4 of 5 pairs), 2.3% of the 2,745 kernel-width
  cells; 2 of the 4 comparison failures had an adverse cell of the same kernel in
  their own null control.
- Run 38389055091's `stencil` processes sit in separate modes: the baseline's five
  processes' medians were 40.3-46.4 ms, the candidate's 44.9-47.8 ms, each process
  steady inside itself (within 1-2%).

## Replay of the new judgment (2026-11-25)

`replay.sh identity DIR` and `replay.sh all DIR` apply `tests/performance/decide.awk`
to the retained artifacts of 182 runs that kept both arms' images (the 187 above
less the runs whose artifact lacks them, plus run 38389055091).

- `replay-identity.txt`: a kernel whose module, oracle, runner and native objects
  match (`keyed_table.o` allowed 130 differing bytes, the path) is not timed, as the
  built images now are with the path prefix map. 160 of 182 runs have all five
  kernels identical, so nothing is timed and the job passes, among them run
  38389055091, the four other comparison failures and the two null-control
  failures. The other 22 runs
  have all five kernels different (a changed runtime or emitter) and are timed: 22
  runs, 110 kernel judgments, all `pass`.
- `replay-all-different.txt`: the same data with every kernel taken as different
  code, which tests the null-control gate alone. 910 kernel judgments: 5 runs
  decide something other than `pass`. Of the five known false comparisons (run
  38389055091 and the four runs of 10-21), three stay `FAIL` because their own
  null control is clean (38389055091 stencil, 39531047299 records, 39607981611
  stencil), and two become `indeterminate` and would be measured once more
  (39573372405 records, 39610184438 stencil). The two runs whose null control
  failed (39614173708, 39733395799) have a passing comparison and no decision.
  What this gives the choice of remedy: the gate catches 2 of 5 false failures
  of code that really differs; for the three it does not catch, byte-identical
  images are what keeps them from being timed.
