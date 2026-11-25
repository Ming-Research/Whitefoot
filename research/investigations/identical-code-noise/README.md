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
