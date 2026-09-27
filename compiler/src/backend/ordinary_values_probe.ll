; Formal native probe boundary: a C descriptor pointer becomes the exact WF
; range argument at this LLVM call, its element pointer and count,
; independently of C's target ABI coercions.
declare void @wf_std.fs.open_file(ptr, ptr, ptr, ptr, i64, i64, i64)
declare void @wf_std.fs.read_at(ptr, ptr, ptr, ptr, i64, i64, i64, i64)

define void @wf_test_public_open(ptr %result, ptr %factory, ptr %root, ptr %name, i64 %start, i64 %end) {
  %view = load { ptr, i64 }, ptr %name
  %data = extractvalue { ptr, i64 } %view, 0
  %len = extractvalue { ptr, i64 } %view, 1
  call void @wf_std.fs.open_file(ptr %result, ptr %factory, ptr %root, ptr %data, i64 %len, i64 %start, i64 %end)
  ret void
}

define void @wf_test_public_read(ptr %result, ptr %factory, ptr %file, ptr %destination, i64 %offset, i64 %start, i64 %end) {
  %view = load { ptr, i64 }, ptr %destination
  %data = extractvalue { ptr, i64 } %view, 0
  %len = extractvalue { ptr, i64 } %view, 1
  call void @wf_std.fs.read_at(ptr %result, ptr %factory, ptr %file, ptr %data, i64 %len, i64 %offset, i64 %start, i64 %end)
  ret void
}
