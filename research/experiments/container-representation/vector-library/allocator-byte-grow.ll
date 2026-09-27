source_filename = "whitefoot"
target datalayout = "e-m:o-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-n32:64-S128-Fn32"
target triple = "aarch64-apple-darwin"

@.wf_resource.heap = private unnamed_addr constant [20 x i8] c"\7B\22\72\65\73\6F\75\72\63\65\22\3A\22\68\65\61\70\22\7D\0A", align 1

declare i64 @write(i32, ptr, i64)
declare ptr @__error()
declare void @abort() noreturn
declare ptr @malloc(i64)
declare void @free(ptr)
declare void @llvm.memmove.p0.p0.i64(ptr, ptr, i64, i1 immarg)
declare ptr @realloc(ptr, i64)

define private i64 @wf_resource_write(ptr %bytes, i64 %length) #0 {
entry:
  br label %write
write:
  %written = call i64 @write(i32 2, ptr %bytes, i64 %length)
  %failed = icmp slt i64 %written, 0
  br i1 %failed, label %error, label %done
error:
  %errno = call ptr @__error()
  %code = load i32, ptr %errno, align 4
  %interrupted = icmp eq i32 %code, 4
  br i1 %interrupted, label %write, label %done
done:
  ret i64 %written
}

define private void @wf_resource_record_abort(ptr %message, i64 %length) noreturn #0 {
entry:
  br label %write.loop
write.loop:
  %cursor = phi ptr [ %message, %entry ], [ %next, %write.more ]
  %remaining = phi i64 [ %length, %entry ], [ %left, %write.more ]
  %written = call i64 @wf_resource_write(ptr %cursor, i64 %remaining)
  %complete = icmp eq i64 %written, %remaining
  br i1 %complete, label %abort, label %write.incomplete
write.incomplete:
  %progress = icmp sgt i64 %written, 0
  br i1 %progress, label %write.more, label %abort
write.more:
  %next = getelementptr i8, ptr %cursor, i64 %written
  %left = sub i64 %remaining, %written
  br label %write.loop
abort:
  call void @abort()
  unreachable
}

define private void @wf_resource_abort() noreturn #0 {
entry:
  call void @wf_resource_record_abort(ptr @.wf_resource.heap, i64 20)
  unreachable
}

define i8 @wf_probe_grow_A(ptr noalias nonnull captures(none) dereferenceable(8) %v0, i64 %v1) #0 {
entry:
  %t0 = load ptr, ptr %v0
  %t1 = getelementptr inbounds { i64, i64, [0 x i64] }, ptr %t0, i32 0, i32 0
  %t2 = load i64, ptr %t1
  %t3 = mul nuw i64 %v1, ptrtoint (ptr getelementptr (i64, ptr null, i64 1) to i64)
  %t4 = add nuw i64 %t3, ptrtoint (ptr getelementptr ({ i64, i64, [0 x i64] }, ptr null, i64 0, i32 2) to i64)
  %t5 = call ptr @malloc(i64 %t4)
  %t6 = icmp ne ptr %t5, null
  br i1 %t6, label %window.block.ready.v2, label %window.block.oom.v2
window.block.oom.v2:
  call void @wf_resource_abort()
  unreachable
window.block.ready.v2:
  %t7 = getelementptr inbounds { i64, i64, [0 x i64] }, ptr %t5, i32 0, i32 0
  store i64 %t2, ptr %t7
  %t8 = getelementptr inbounds { i64, i64, [0 x i64] }, ptr %t5, i32 0, i32 1
  store i64 %v1, ptr %t8
  %t9 = getelementptr inbounds { i64, i64, [0 x i64] }, ptr %t0, i64 0, i32 2, i64 0
  %t10 = getelementptr inbounds { i64, i64, [0 x i64] }, ptr %t5, i64 0, i32 2, i64 0
  %t11 = mul nuw i64 %t2, ptrtoint (ptr getelementptr (i64, ptr null, i64 1) to i64)
  call void @llvm.memmove.p0.p0.i64(ptr %t10, ptr %t9, i64 %t11, i1 false)
  call void @free(ptr %t0)
  store ptr %t5, ptr %v0
  %v2 = select i1 true, i8 0, i8 0
  %v3 = select i1 true, i8 0, i8 0
  ret i8 %v3
}

define i8 @wf_probe_grow_R(ptr noalias nonnull captures(none) dereferenceable(8) %v0, i64 %v1) #0 {
entry:
  %t0 = load ptr, ptr %v0
  %t1 = getelementptr inbounds { i64, i64, [0 x i64] }, ptr %t0, i32 0, i32 0
  %t2 = load i64, ptr %t1
  %t3 = mul nuw i64 %v1, ptrtoint (ptr getelementptr (i64, ptr null, i64 1) to i64)
  %t4 = add nuw i64 %t3, ptrtoint (ptr getelementptr ({ i64, i64, [0 x i64] }, ptr null, i64 0, i32 2) to i64)
  %t5 = call ptr @realloc(ptr %t0, i64 %t4)
  %t6 = icmp ne ptr %t5, null
  br i1 %t6, label %window.block.ready.v2, label %window.block.oom.v2
window.block.oom.v2:
  call void @wf_resource_abort()
  unreachable
window.block.ready.v2:
  %t7 = getelementptr inbounds { i64, i64, [0 x i64] }, ptr %t5, i32 0, i32 0
  store i64 %t2, ptr %t7
  %t8 = getelementptr inbounds { i64, i64, [0 x i64] }, ptr %t5, i32 0, i32 1
  store i64 %v1, ptr %t8
  store ptr %t5, ptr %v0
  %v2 = select i1 true, i8 0, i8 0
  %v3 = select i1 true, i8 0, i8 0
  ret i8 %v3
}

define i8 @wf_probe_grow_P(ptr noalias nonnull captures(none) dereferenceable(8) %v0, i64 %v1) #0 {
entry:
  %t0 = load ptr, ptr %v0
  %t1 = getelementptr inbounds { i64, i64, [0 x i64] }, ptr %t0, i32 0, i32 0
  %t2 = load i64, ptr %t1
  %h2_cap_ptr = getelementptr inbounds { i64, i64, [0 x i64] }, ptr %t0, i32 0, i32 1
  %h2_cap = load i64, ptr %h2_cap_ptr
  %h2_full = icmp eq i64 %t2, %h2_cap
  %h2_nonempty = icmp ne i64 %t2, 0
  %h2_resize = and i1 %h2_full, %h2_nonempty
  %t3 = mul nuw i64 %v1, ptrtoint (ptr getelementptr (i64, ptr null, i64 1) to i64)
  %t4 = add nuw i64 %t3, ptrtoint (ptr getelementptr ({ i64, i64, [0 x i64] }, ptr null, i64 0, i32 2) to i64)
  br i1 %h2_resize, label %h2.resize, label %h2.copy
h2.resize:
  %h2_fresh = call ptr @realloc(ptr %t0, i64 %t4)
  %h2_ok = icmp ne ptr %h2_fresh, null
  br i1 %h2_ok, label %h2.resize.ready, label %window.block.oom.v2
h2.resize.ready:
  %h2_len_ptr = getelementptr inbounds { i64, i64, [0 x i64] }, ptr %h2_fresh, i32 0, i32 0
  store i64 %t2, ptr %h2_len_ptr
  %h2_new_cap_ptr = getelementptr inbounds { i64, i64, [0 x i64] }, ptr %h2_fresh, i32 0, i32 1
  store i64 %v1, ptr %h2_new_cap_ptr
  store ptr %h2_fresh, ptr %v0
  br label %h2.done
h2.copy:
  %t5 = call ptr @malloc(i64 %t4)
  %t6 = icmp ne ptr %t5, null
  br i1 %t6, label %window.block.ready.v2, label %window.block.oom.v2
window.block.oom.v2:
  call void @wf_resource_abort()
  unreachable
window.block.ready.v2:
  %t7 = getelementptr inbounds { i64, i64, [0 x i64] }, ptr %t5, i32 0, i32 0
  store i64 %t2, ptr %t7
  %t8 = getelementptr inbounds { i64, i64, [0 x i64] }, ptr %t5, i32 0, i32 1
  store i64 %v1, ptr %t8
  %t9 = getelementptr inbounds { i64, i64, [0 x i64] }, ptr %t0, i64 0, i32 2, i64 0
  %t10 = getelementptr inbounds { i64, i64, [0 x i64] }, ptr %t5, i64 0, i32 2, i64 0
  %t11 = mul nuw i64 %t2, ptrtoint (ptr getelementptr (i64, ptr null, i64 1) to i64)
  call void @llvm.memmove.p0.p0.i64(ptr %t10, ptr %t9, i64 %t11, i1 false)
  call void @free(ptr %t0)
  store ptr %t5, ptr %v0
  br label %h2.done
h2.done:
  %v2 = select i1 true, i8 0, i8 0
  %v3 = select i1 true, i8 0, i8 0
  ret i8 %v3
}

attributes #0 = { "probe-stack"="__chkstk_darwin" }
