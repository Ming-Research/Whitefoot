; Scalar/pointer host ABI for the formal records fixture.
; The shared adapter binder selects the entry-equivalent execution world.
; The owned runtime-capacity result is a `Box<Array<T>>`, whose content
; [TYPE-9] stores "in exactly one heap object the `Box` value owns" and which
; [STOR-1] releases with "one compiler-derived free at the owner's scope exit
; [STOR-3]". That one object is thin: the pending amendment
; compiler/storage-representation makes `Box<Array<T>>` "one pointer to one
; block laid out `[len | cap | elements]`", with "an `Array`, whose `len` equals
; its `cap` [WIN-1], storing that one runtime number once". So the block behind
; the returned pointer is `[len | elements]`: the adapter reads the length out
; of its first word and takes the element base one 8-byte header word past it,
; every element type here being `u64`, `f64` or `u8`, none of which [OP-9]
; aligns past 8. The same pointer is handed back as the retained handle, which
; is the only value the release row accepts.
; A range argument crosses the call as its element pointer and count.
define void @wf_bench_records(ptr %data, i64 %data_len, ptr %offsets, i64 %offsets_len, i64 %first, i64 %end, ptr %out, ptr %out_len, ptr %out_cell) {
  %r = call ptr @wf_summarize_records(ptr %data, i64 %data_len, ptr %offsets, i64 %offsets_len, i64 %first, i64 %end)
  %n = load i64, ptr %r
  %p = getelementptr inbounds i8, ptr %r, i64 8
  store ptr %p, ptr %out
  store i64 %n, ptr %out_len
  store ptr %r, ptr %out_cell
  ret void
}
define void @wf_bench_records_release(ptr %cell) {
  %x = call i64 @wf_record_result_release(ptr %cell)
  ret void
}
