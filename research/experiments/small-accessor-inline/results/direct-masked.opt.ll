; ModuleID = 'direct-masked.ll'
source_filename = "direct-masked.ll"
target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-unknown-linux-gnu"

; Function Attrs: mustprogress nofree norecurse nosync nounwind willreturn
define void @accessor(ptr %result, ptr %base) local_unnamed_addr #0 {
entry:
  %source = getelementptr inbounds nuw i8, ptr %base, i64 16
  %sources = getelementptr i8, ptr %source, <12 x i64> <i64 0, i64 4, i64 8, i64 12, i64 16, i64 20, i64 24, i64 28, i64 32, i64 36, i64 40, i64 44>
  %values = tail call <12 x i32> @llvm.masked.gather.v12i32.v12p0(<12 x ptr> align 1 %sources, <12 x i1> splat (i1 true), <12 x i32> poison)
  %destinations = getelementptr i8, ptr %result, <12 x i64> <i64 0, i64 4, i64 8, i64 12, i64 16, i64 20, i64 24, i64 28, i64 32, i64 36, i64 40, i64 44>
  tail call void @llvm.masked.scatter.v12i32.v12p0(<12 x i32> %values, <12 x ptr> align 1 %destinations, <12 x i1> splat (i1 true))
  ret void
}

; Function Attrs: mustprogress nofree norecurse nosync nounwind willreturn
define i32 @caller_partial(ptr %base, i1 %take) local_unnamed_addr #0 {
entry:
  %result = alloca [12 x i32], align 4
  br i1 %take, label %rare, label %common.ret, !prof !0

common.ret:                                       ; preds = %entry, %rare
  %common.ret.op = phi i32 [ %value, %rare ], [ 0, %entry ]
  ret i32 %common.ret.op

rare:                                             ; preds = %entry
  %source.i = getelementptr inbounds nuw i8, ptr %base, i64 16
  %sources.i = getelementptr i8, ptr %source.i, <12 x i64> <i64 0, i64 4, i64 8, i64 12, i64 16, i64 20, i64 24, i64 28, i64 32, i64 36, i64 40, i64 44>
  %values.i = tail call <12 x i32> @llvm.masked.gather.v12i32.v12p0(<12 x ptr> align 1 %sources.i, <12 x i1> splat (i1 true), <12 x i32> poison)
  %destinations.i = getelementptr i8, ptr %result, <12 x i64> <i64 0, i64 4, i64 8, i64 12, i64 16, i64 20, i64 24, i64 28, i64 32, i64 36, i64 40, i64 44>
  call void @llvm.masked.scatter.v12i32.v12p0(<12 x i32> %values.i, <12 x ptr> align 1 %destinations.i, <12 x i1> splat (i1 true))
  %value = load i32, ptr %result, align 4
  br label %common.ret
}

; Function Attrs: mustprogress nofree norecurse nosync nounwind willreturn
define void @caller_full(ptr writeonly captures(none) %out, ptr %base, i1 %take) local_unnamed_addr #0 {
entry:
  %result = alloca [12 x i32], align 16
  br i1 %take, label %rare, label %common.ret, !prof !0

common.ret:                                       ; preds = %entry, %rare
  ret void

rare:                                             ; preds = %entry
  %source.i = getelementptr inbounds nuw i8, ptr %base, i64 16
  %sources.i = getelementptr i8, ptr %source.i, <12 x i64> <i64 0, i64 4, i64 8, i64 12, i64 16, i64 20, i64 24, i64 28, i64 32, i64 36, i64 40, i64 44>
  %values.i = tail call <12 x i32> @llvm.masked.gather.v12i32.v12p0(<12 x ptr> align 1 %sources.i, <12 x i1> splat (i1 true), <12 x i32> poison)
  %destinations.i = getelementptr i8, ptr %result, <12 x i64> <i64 0, i64 4, i64 8, i64 12, i64 16, i64 20, i64 24, i64 28, i64 32, i64 36, i64 40, i64 44>
  call void @llvm.masked.scatter.v12i32.v12p0(<12 x i32> %values.i, <12 x ptr> align 1 %destinations.i, <12 x i1> splat (i1 true))
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

; Function Attrs: mustprogress nocallback nofree nosync nounwind willreturn memory(read)
declare <12 x i32> @llvm.masked.gather.v12i32.v12p0(<12 x ptr>, <12 x i1>, <12 x i32>) #1

; Function Attrs: mustprogress nocallback nofree nosync nounwind willreturn memory(write)
declare void @llvm.masked.scatter.v12i32.v12p0(<12 x i32>, <12 x ptr>, <12 x i1>) #2

attributes #0 = { mustprogress nofree norecurse nosync nounwind willreturn }
attributes #1 = { mustprogress nocallback nofree nosync nounwind willreturn memory(read) }
attributes #2 = { mustprogress nocallback nofree nosync nounwind willreturn memory(write) }

!0 = !{!"branch_weights", i32 1, i32 10000}
