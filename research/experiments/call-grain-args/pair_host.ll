; Bind the selected world once per call, outside the timed WF loop. A source
; without `main` still emits wf__floor_run, which calls wf__main_body; runner.c
; supplies that body.
declare i32 @wf__main_body(i32, ptr)

define i64 @wf_bench_pair(i64 %repetitions, i64 %extent, i64 %seed) {
  %r = call i64 @wf_workload(i64 %repetitions, i64 %extent, i64 %seed)
  ret i64 %r
}
