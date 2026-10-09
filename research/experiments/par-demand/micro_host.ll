; Bind the selected world once per measured batch, outside the hot WF loop.
define i64 @wf_bench_micro(i64 %repetitions, i64 %extent, i64 %seed) {
  %r = call i64 @wf_workload(i64 %repetitions, i64 %extent, i64 %seed)
  ret i64 %r
}
