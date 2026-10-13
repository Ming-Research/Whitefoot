; ModuleID = 'granules.ll'
source_filename = "granules.ll"
target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-unknown-linux-gnu"

define void @accessor(ptr writeonly captures(none) initializes((0, 48)) %result, ptr %base) local_unnamed_addr {
entry:
  %resolved = tail call ptr @resolve_source(ptr %base)
  %source = getelementptr inbounds nuw i8, ptr %resolved, i64 16
  %copy.p4 = getelementptr inbounds nuw i8, ptr %resolved, i64 32
  %copy.p8 = getelementptr inbounds nuw i8, ptr %resolved, i64 48
  %0 = load <4 x i32>, ptr %source, align 1
  %copy.d4 = getelementptr inbounds nuw i8, ptr %result, i64 16
  %1 = load <4 x i32>, ptr %copy.p4, align 1
  %copy.d8 = getelementptr inbounds nuw i8, ptr %result, i64 32
  %2 = load <4 x i32>, ptr %copy.p8, align 1
  store <4 x i32> %0, ptr %result, align 1
  store <4 x i32> %1, ptr %copy.d4, align 1
  store <4 x i32> %2, ptr %copy.d8, align 1
  ret void
}

define i32 @caller_partial(ptr %base, i1 %take) local_unnamed_addr {
entry:
  %result = alloca [12 x i32], align 4
  br i1 %take, label %rare, label %common.ret, !prof !0

common.ret:                                       ; preds = %entry, %rare
  %common.ret.op = phi i32 [ %value, %rare ], [ 0, %entry ]
  ret i32 %common.ret.op

rare:                                             ; preds = %entry
  call void @accessor(ptr nonnull %result, ptr %base)
  %value = load i32, ptr %result, align 4
  br label %common.ret
}

define void @caller_full(ptr writeonly captures(none) %out, ptr %base, i1 %take) local_unnamed_addr {
entry:
  %result = alloca [12 x i32], align 16
  br i1 %take, label %rare, label %common.ret, !prof !0

common.ret:                                       ; preds = %entry, %rare
  ret void

rare:                                             ; preds = %entry
  call void @accessor(ptr nonnull %result, ptr %base)
  %consume.p4 = getelementptr inbounds nuw i8, ptr %result, i64 16
  %consume.p8 = getelementptr inbounds nuw i8, ptr %result, i64 32
  %0 = load <4 x i32>, ptr %result, align 16
  store <4 x i32> %0, ptr %out, align 1
  %consume.d4 = getelementptr inbounds nuw i8, ptr %out, i64 16
  %1 = load <4 x i32>, ptr %consume.p4, align 16
  store <4 x i32> %1, ptr %consume.d4, align 1
  %consume.d8 = getelementptr inbounds nuw i8, ptr %out, i64 32
  %2 = load <4 x i32>, ptr %consume.p8, align 16
  store <4 x i32> %2, ptr %consume.d8, align 1
  br label %common.ret
}

declare ptr @resolve_source(ptr) local_unnamed_addr

!0 = !{!"branch_weights", i32 1, i32 10000}
