; ModuleID = 'mixed-direct-granules.ll'
source_filename = "mixed-direct-granules.ll"
target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-unknown-linux-gnu"

; Function Attrs: mustprogress nofree norecurse nosync nounwind willreturn memory(argmem: readwrite)
define void @accessor(ptr writeonly captures(none) initializes((0, 48)) %result, ptr readonly captures(none) %base) local_unnamed_addr #0 {
entry:
  %source = getelementptr inbounds nuw i8, ptr %base, i64 16
  %copy.v0 = load i32, ptr %source, align 1
  %copy.p1 = getelementptr inbounds nuw i8, ptr %base, i64 20
  %copy.v1 = load float, ptr %copy.p1, align 1
  %copy.p2 = getelementptr inbounds nuw i8, ptr %base, i64 24
  %copy.v2 = load i32, ptr %copy.p2, align 1
  %copy.p3 = getelementptr inbounds nuw i8, ptr %base, i64 28
  %copy.v3 = load float, ptr %copy.p3, align 1
  %copy.p4 = getelementptr inbounds nuw i8, ptr %base, i64 32
  %copy.v4 = load i32, ptr %copy.p4, align 1
  %copy.p5 = getelementptr inbounds nuw i8, ptr %base, i64 36
  %copy.v5 = load float, ptr %copy.p5, align 1
  %copy.p6 = getelementptr inbounds nuw i8, ptr %base, i64 40
  %copy.v6 = load i32, ptr %copy.p6, align 1
  %copy.p7 = getelementptr inbounds nuw i8, ptr %base, i64 44
  %copy.v7 = load float, ptr %copy.p7, align 1
  %copy.p8 = getelementptr inbounds nuw i8, ptr %base, i64 48
  %copy.v8 = load i32, ptr %copy.p8, align 1
  %copy.p9 = getelementptr inbounds nuw i8, ptr %base, i64 52
  %copy.v9 = load float, ptr %copy.p9, align 1
  %copy.p10 = getelementptr inbounds nuw i8, ptr %base, i64 56
  %copy.v10 = load i32, ptr %copy.p10, align 1
  %copy.p11 = getelementptr inbounds nuw i8, ptr %base, i64 60
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

; Function Attrs: mustprogress nofree norecurse nosync nounwind willreturn memory(argmem: read)
define i32 @caller_partial(ptr readonly captures(none) %base, i1 %take) local_unnamed_addr #1 {
entry:
  br i1 %take, label %rare, label %common.ret, !prof !0

common.ret:                                       ; preds = %entry, %rare
  %common.ret.op = phi i32 [ %copy.v0.i, %rare ], [ 0, %entry ]
  ret i32 %common.ret.op

rare:                                             ; preds = %entry
  %source.i = getelementptr inbounds nuw i8, ptr %base, i64 16
  %copy.v0.i = load i32, ptr %source.i, align 1
  br label %common.ret
}

; Function Attrs: mustprogress nofree norecurse nosync nounwind willreturn memory(argmem: readwrite)
define void @caller_full(ptr writeonly captures(none) %out, ptr readonly captures(none) %base, i1 %take) local_unnamed_addr #0 {
entry:
  br i1 %take, label %rare, label %common.ret, !prof !0

common.ret:                                       ; preds = %entry, %rare
  ret void

rare:                                             ; preds = %entry
  %source.i = getelementptr inbounds nuw i8, ptr %base, i64 16
  %copy.v0.i = load i32, ptr %source.i, align 1
  %copy.p1.i = getelementptr inbounds nuw i8, ptr %base, i64 20
  %copy.v1.i = load float, ptr %copy.p1.i, align 1
  %copy.p2.i = getelementptr inbounds nuw i8, ptr %base, i64 24
  %copy.v2.i = load i32, ptr %copy.p2.i, align 1
  %copy.p3.i = getelementptr inbounds nuw i8, ptr %base, i64 28
  %copy.v3.i = load float, ptr %copy.p3.i, align 1
  %copy.p4.i = getelementptr inbounds nuw i8, ptr %base, i64 32
  %copy.v4.i = load i32, ptr %copy.p4.i, align 1
  %copy.p5.i = getelementptr inbounds nuw i8, ptr %base, i64 36
  %copy.v5.i = load float, ptr %copy.p5.i, align 1
  %copy.p6.i = getelementptr inbounds nuw i8, ptr %base, i64 40
  %copy.v6.i = load i32, ptr %copy.p6.i, align 1
  %copy.p7.i = getelementptr inbounds nuw i8, ptr %base, i64 44
  %copy.v7.i = load float, ptr %copy.p7.i, align 1
  %copy.p8.i = getelementptr inbounds nuw i8, ptr %base, i64 48
  %copy.v8.i = load i32, ptr %copy.p8.i, align 1
  %copy.p9.i = getelementptr inbounds nuw i8, ptr %base, i64 52
  %copy.v9.i = load float, ptr %copy.p9.i, align 1
  %copy.p10.i = getelementptr inbounds nuw i8, ptr %base, i64 56
  %copy.v10.i = load i32, ptr %copy.p10.i, align 1
  %copy.p11.i = getelementptr inbounds nuw i8, ptr %base, i64 60
  %copy.v11.i = load float, ptr %copy.p11.i, align 1
  store i32 %copy.v0.i, ptr %out, align 1
  %consume.d1 = getelementptr inbounds nuw i8, ptr %out, i64 4
  store float %copy.v1.i, ptr %consume.d1, align 1
  %consume.d2 = getelementptr inbounds nuw i8, ptr %out, i64 8
  store i32 %copy.v2.i, ptr %consume.d2, align 1
  %consume.d3 = getelementptr inbounds nuw i8, ptr %out, i64 12
  store float %copy.v3.i, ptr %consume.d3, align 1
  %consume.d4 = getelementptr inbounds nuw i8, ptr %out, i64 16
  store i32 %copy.v4.i, ptr %consume.d4, align 1
  %consume.d5 = getelementptr inbounds nuw i8, ptr %out, i64 20
  store float %copy.v5.i, ptr %consume.d5, align 1
  %consume.d6 = getelementptr inbounds nuw i8, ptr %out, i64 24
  store i32 %copy.v6.i, ptr %consume.d6, align 1
  %consume.d7 = getelementptr inbounds nuw i8, ptr %out, i64 28
  store float %copy.v7.i, ptr %consume.d7, align 1
  %consume.d8 = getelementptr inbounds nuw i8, ptr %out, i64 32
  store i32 %copy.v8.i, ptr %consume.d8, align 1
  %consume.d9 = getelementptr inbounds nuw i8, ptr %out, i64 36
  store float %copy.v9.i, ptr %consume.d9, align 1
  %consume.d10 = getelementptr inbounds nuw i8, ptr %out, i64 40
  store i32 %copy.v10.i, ptr %consume.d10, align 1
  %consume.d11 = getelementptr inbounds nuw i8, ptr %out, i64 44
  store float %copy.v11.i, ptr %consume.d11, align 1
  br label %common.ret
}

attributes #0 = { mustprogress nofree norecurse nosync nounwind willreturn memory(argmem: readwrite) }
attributes #1 = { mustprogress nofree norecurse nosync nounwind willreturn memory(argmem: read) }

!0 = !{!"branch_weights", i32 1, i32 10000}
