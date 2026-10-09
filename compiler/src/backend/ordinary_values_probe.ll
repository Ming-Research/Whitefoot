; Formal native probe boundary: a C descriptor pointer becomes the exact WF
; range argument at this LLVM call, its element pointer and count,
; independently of C's target ABI coercions.  Each public waiting operation is
; its start and, unless the start answered at once, its finish, which joins
; the record in place; the operation block is the largest platform's
; `WF_CONTEXT_OPERATION_BYTES`, so one probe serves every target.
declare i32 @wf_std.fs.open_file.start(ptr, ptr, ptr, ptr, i64, i64, i64, ptr)
declare void @wf_std.fs.open_file.finish(ptr, ptr, ptr, ptr, i64, i64, i64, ptr)
declare i32 @wf_std.fs.read_at.start(ptr, ptr, ptr, ptr, i64, i64, i64, i64, ptr)
declare void @wf_std.fs.read_at.finish(ptr, ptr, ptr, ptr, i64, i64, i64, i64, ptr)

define void @wf_test_public_open(ptr %result, ptr %factory, ptr %root, ptr %name, i64 %start, i64 %end) {
entry:
  %operation = alloca [1216 x i8], align 16
  %view = load { ptr, i64 }, ptr %name
  %data = extractvalue { ptr, i64 } %view, 0
  %len = extractvalue { ptr, i64 } %view, 1
  %state = call i32 @wf_std.fs.open_file.start(ptr %result, ptr %factory, ptr %root, ptr %data, i64 %len, i64 %start, i64 %end, ptr %operation)
  %answered = icmp eq i32 %state, 0
  br i1 %answered, label %done, label %finish
finish:
  call void @wf_std.fs.open_file.finish(ptr %result, ptr %factory, ptr %root, ptr %data, i64 %len, i64 %start, i64 %end, ptr %operation)
  br label %done
done:
  ret void
}

define void @wf_test_public_read(ptr %result, ptr %factory, ptr %file, ptr %destination, i64 %offset, i64 %start, i64 %end) {
entry:
  %operation = alloca [1216 x i8], align 16
  %view = load { ptr, i64 }, ptr %destination
  %data = extractvalue { ptr, i64 } %view, 0
  %len = extractvalue { ptr, i64 } %view, 1
  %state = call i32 @wf_std.fs.read_at.start(ptr %result, ptr %factory, ptr %file, ptr %data, i64 %len, i64 %offset, i64 %start, i64 %end, ptr %operation)
  %answered = icmp eq i32 %state, 0
  br i1 %answered, label %done, label %finish
finish:
  call void @wf_std.fs.read_at.finish(ptr %result, ptr %factory, ptr %file, ptr %data, i64 %len, i64 %offset, i64 %start, i64 %end, ptr %operation)
  br label %done
done:
  ret void
}

declare i32 @wf_std.fs.open_directory_write.start(ptr, ptr, ptr, ptr, i64, i64, i64, ptr)
declare void @wf_std.fs.open_directory_write.finish(ptr, ptr, ptr, ptr, i64, i64, i64, ptr)

define void @wf_test_public_open_directory_write(ptr %result, ptr %factory, ptr %root, ptr %name, i64 %start, i64 %end) {
entry:
  %operation = alloca [1216 x i8], align 16
  %view = load { ptr, i64 }, ptr %name
  %data = extractvalue { ptr, i64 } %view, 0
  %len = extractvalue { ptr, i64 } %view, 1
  %state = call i32 @wf_std.fs.open_directory_write.start(ptr %result, ptr %factory, ptr %root, ptr %data, i64 %len, i64 %start, i64 %end, ptr %operation)
  %answered = icmp eq i32 %state, 0
  br i1 %answered, label %done, label %finish
finish:
  call void @wf_std.fs.open_directory_write.finish(ptr %result, ptr %factory, ptr %root, ptr %data, i64 %len, i64 %start, i64 %end, ptr %operation)
  br label %done
done:
  ret void
}

declare void @wf_std.process.stop_listen(ptr, ptr, ptr)
declare i32 @wf_std.process.stop_next.start(ptr, ptr, ptr, ptr, ptr, ptr)
declare void @wf_std.process.stop_next.finish(ptr, ptr, ptr, ptr, ptr, ptr)
declare i32 @wf_std.process.close_stop_listener.start(ptr, ptr, ptr, ptr)
declare void @wf_std.process.close_stop_listener.finish(ptr, ptr, ptr, ptr)

define void @wf_test_public_stop_listen(ptr %result, ptr %factory, ptr %stops) {
entry:
  call void @wf_std.process.stop_listen(ptr %result, ptr %factory, ptr %stops)
  ret void
}

define void @wf_test_public_stop_next(ptr %result, ptr %factory, ptr %listener, ptr %deadline, ptr %cancel) {
entry:
  %operation = alloca [1216 x i8], align 16
  %state = call i32 @wf_std.process.stop_next.start(ptr %result, ptr %factory, ptr %listener, ptr %deadline, ptr %cancel, ptr %operation)
  %answered = icmp eq i32 %state, 0
  br i1 %answered, label %done, label %finish
finish:
  call void @wf_std.process.stop_next.finish(ptr %result, ptr %factory, ptr %listener, ptr %deadline, ptr %cancel, ptr %operation)
  br label %done
done:
  ret void
}

define void @wf_test_public_close_stop_listener(ptr %result, ptr %factory, ptr %listener) {
entry:
  %operation = alloca [1216 x i8], align 16
  %state = call i32 @wf_std.process.close_stop_listener.start(ptr %result, ptr %factory, ptr %listener, ptr %operation)
  %answered = icmp eq i32 %state, 0
  br i1 %answered, label %done, label %finish
finish:
  call void @wf_std.process.close_stop_listener.finish(ptr %result, ptr %factory, ptr %listener, ptr %operation)
  br label %done
done:
  ret void
}
