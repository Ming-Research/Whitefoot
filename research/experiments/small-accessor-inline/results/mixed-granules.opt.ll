; ModuleID = 'mixed-granules.ll'
source_filename = "mixed-granules.ll"
target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-unknown-linux-gnu"

define void @accessor(ptr writeonly captures(none) initializes((0, 48)) %result, ptr %base) local_unnamed_addr {
entry:
  %resolved = tail call ptr @resolve_source(ptr %base)
  %source = getelementptr inbounds nuw i8, ptr %resolved, i64 16
  %copy.v0 = load i32, ptr %source, align 1
  %copy.p1 = getelementptr inbounds nuw i8, ptr %resolved, i64 20
  %copy.v1 = load float, ptr %copy.p1, align 1
  %copy.p2 = getelementptr inbounds nuw i8, ptr %resolved, i64 24
  %copy.v2 = load i32, ptr %copy.p2, align 1
  %copy.p3 = getelementptr inbounds nuw i8, ptr %resolved, i64 28
  %copy.v3 = load float, ptr %copy.p3, align 1
  %copy.p4 = getelementptr inbounds nuw i8, ptr %resolved, i64 32
  %copy.v4 = load i32, ptr %copy.p4, align 1
  %copy.p5 = getelementptr inbounds nuw i8, ptr %resolved, i64 36
  %copy.v5 = load float, ptr %copy.p5, align 1
  %copy.p6 = getelementptr inbounds nuw i8, ptr %resolved, i64 40
  %copy.v6 = load i32, ptr %copy.p6, align 1
  %copy.p7 = getelementptr inbounds nuw i8, ptr %resolved, i64 44
  %copy.v7 = load float, ptr %copy.p7, align 1
  %copy.p8 = getelementptr inbounds nuw i8, ptr %resolved, i64 48
  %copy.v8 = load i32, ptr %copy.p8, align 1
  %copy.p9 = getelementptr inbounds nuw i8, ptr %resolved, i64 52
  %copy.v9 = load float, ptr %copy.p9, align 1
  %copy.p10 = getelementptr inbounds nuw i8, ptr %resolved, i64 56
  %copy.v10 = load i32, ptr %copy.p10, align 1
  %copy.p11 = getelementptr inbounds nuw i8, ptr %resolved, i64 60
  %copy.v11 = load float, ptr %copy.p11, align 1
  store i32 %copy.v0, ptr %result, align 1
  %copy.d1 = getelementptr inbounds nuw i8, ptr %result, i64 4
  store float %copy.v1, ptr %copy.d1, align 1
  %copy.d2 = getelementptr inbounds nuw i8, ptr %result, i64 8
  store i32 %copy.v2, ptr %copy.d2, align 1
  %copy.d3 = getelementptr inbounds nuw i8, ptr %result, i64 12
  store float %copy.v3, ptr %copy.d3, align 1
  %copy.d4 = getelementptr inbounds nuw i8, ptr %result, i64 16
  store i32 %copy.v4, ptr %copy.d4, align 1
  %copy.d5 = getelementptr inbounds nuw i8, ptr %result, i64 20
  store float %copy.v5, ptr %copy.d5, align 1
  %copy.d6 = getelementptr inbounds nuw i8, ptr %result, i64 24
  store i32 %copy.v6, ptr %copy.d6, align 1
  %copy.d7 = getelementptr inbounds nuw i8, ptr %result, i64 28
  store float %copy.v7, ptr %copy.d7, align 1
  %copy.d8 = getelementptr inbounds nuw i8, ptr %result, i64 32
  store i32 %copy.v8, ptr %copy.d8, align 1
  %copy.d9 = getelementptr inbounds nuw i8, ptr %result, i64 36
  store float %copy.v9, ptr %copy.d9, align 1
  %copy.d10 = getelementptr inbounds nuw i8, ptr %result, i64 40
  store i32 %copy.v10, ptr %copy.d10, align 1
  %copy.d11 = getelementptr inbounds nuw i8, ptr %result, i64 44
  store float %copy.v11, ptr %copy.d11, align 1
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
  %result = alloca [12 x i32], align 4
  br i1 %take, label %rare, label %common.ret, !prof !0

common.ret:                                       ; preds = %entry, %rare
  ret void

rare:                                             ; preds = %entry
  call void @accessor(ptr nonnull %result, ptr %base)
  %consume.v0 = load i32, ptr %result, align 4
  %consume.p1 = getelementptr inbounds nuw i8, ptr %result, i64 4
  %consume.v1 = load float, ptr %consume.p1, align 4
  %consume.p2 = getelementptr inbounds nuw i8, ptr %result, i64 8
  %consume.v2 = load i32, ptr %consume.p2, align 4
  %consume.p3 = getelementptr inbounds nuw i8, ptr %result, i64 12
  %consume.v3 = load float, ptr %consume.p3, align 4
  %consume.p4 = getelementptr inbounds nuw i8, ptr %result, i64 16
  %consume.v4 = load i32, ptr %consume.p4, align 4
  %consume.p5 = getelementptr inbounds nuw i8, ptr %result, i64 20
  %consume.v5 = load float, ptr %consume.p5, align 4
  %consume.p6 = getelementptr inbounds nuw i8, ptr %result, i64 24
  %consume.v6 = load i32, ptr %consume.p6, align 4
  %consume.p7 = getelementptr inbounds nuw i8, ptr %result, i64 28
  %consume.v7 = load float, ptr %consume.p7, align 4
  %consume.p8 = getelementptr inbounds nuw i8, ptr %result, i64 32
  %consume.v8 = load i32, ptr %consume.p8, align 4
  %consume.p9 = getelementptr inbounds nuw i8, ptr %result, i64 36
  %consume.v9 = load float, ptr %consume.p9, align 4
  %consume.p10 = getelementptr inbounds nuw i8, ptr %result, i64 40
  %consume.v10 = load i32, ptr %consume.p10, align 4
  %consume.p11 = getelementptr inbounds nuw i8, ptr %result, i64 44
  %consume.v11 = load float, ptr %consume.p11, align 4
  store i32 %consume.v0, ptr %out, align 1
  %consume.d1 = getelementptr inbounds nuw i8, ptr %out, i64 4
  store float %consume.v1, ptr %consume.d1, align 1
  %consume.d2 = getelementptr inbounds nuw i8, ptr %out, i64 8
  store i32 %consume.v2, ptr %consume.d2, align 1
  %consume.d3 = getelementptr inbounds nuw i8, ptr %out, i64 12
  store float %consume.v3, ptr %consume.d3, align 1
  %consume.d4 = getelementptr inbounds nuw i8, ptr %out, i64 16
  store i32 %consume.v4, ptr %consume.d4, align 1
  %consume.d5 = getelementptr inbounds nuw i8, ptr %out, i64 20
  store float %consume.v5, ptr %consume.d5, align 1
  %consume.d6 = getelementptr inbounds nuw i8, ptr %out, i64 24
  store i32 %consume.v6, ptr %consume.d6, align 1
  %consume.d7 = getelementptr inbounds nuw i8, ptr %out, i64 28
  store float %consume.v7, ptr %consume.d7, align 1
  %consume.d8 = getelementptr inbounds nuw i8, ptr %out, i64 32
  store i32 %consume.v8, ptr %consume.d8, align 1
  %consume.d9 = getelementptr inbounds nuw i8, ptr %out, i64 36
  store float %consume.v9, ptr %consume.d9, align 1
  %consume.d10 = getelementptr inbounds nuw i8, ptr %out, i64 40
  store i32 %consume.v10, ptr %consume.d10, align 1
  %consume.d11 = getelementptr inbounds nuw i8, ptr %out, i64 44
  store float %consume.v11, ptr %consume.d11, align 1
  br label %common.ret
}

declare ptr @resolve_source(ptr) local_unnamed_addr

!0 = !{!"branch_weights", i32 1, i32 10000}
