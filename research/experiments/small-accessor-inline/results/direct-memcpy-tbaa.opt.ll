; ModuleID = 'direct-memcpy-tbaa.ll'
source_filename = "direct-memcpy-tbaa.ll"
target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-unknown-linux-gnu"

; Function Attrs: mustprogress nofree norecurse nosync nounwind willreturn memory(argmem: readwrite)
define void @accessor(ptr writeonly captures(none) initializes((0, 48)) %result, ptr readonly captures(none) %base) local_unnamed_addr #0 {
entry:
  %source = getelementptr inbounds nuw i8, ptr %base, i64 16
  tail call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 1 dereferenceable(48) %result, ptr noundef nonnull align 1 dereferenceable(48) %source, i64 48, i1 false), !tbaa.struct !0
  ret void
}

; Function Attrs: mustprogress nofree norecurse nosync nounwind willreturn memory(argmem: read)
define i32 @caller_partial(ptr readonly captures(none) %base, i1 %take) local_unnamed_addr #1 {
entry:
  br i1 %take, label %rare, label %common.ret, !prof !4

common.ret:                                       ; preds = %entry, %rare
  %common.ret.op = phi i32 [ %result.sroa.0.0.copyload, %rare ], [ 0, %entry ]
  ret i32 %common.ret.op

rare:                                             ; preds = %entry
  %source.i = getelementptr inbounds nuw i8, ptr %base, i64 16
  %result.sroa.0.0.copyload = load i32, ptr %source.i, align 1, !tbaa !1
  br label %common.ret
}

; Function Attrs: mustprogress nofree norecurse nosync nounwind willreturn memory(argmem: readwrite)
define void @caller_full(ptr writeonly captures(none) %out, ptr readonly captures(none) %base, i1 %take) local_unnamed_addr #0 {
entry:
  br i1 %take, label %rare, label %common.ret, !prof !4

common.ret:                                       ; preds = %entry, %rare
  ret void

rare:                                             ; preds = %entry
  %source.i = getelementptr inbounds nuw i8, ptr %base, i64 16
  %result.sroa.5.0.source.i.sroa_idx = getelementptr inbounds nuw i8, ptr %base, i64 32
  %result.sroa.9.0.source.i.sroa_idx = getelementptr inbounds nuw i8, ptr %base, i64 48
  %0 = load <4 x i32>, ptr %source.i, align 1, !tbaa !1
  %consume.d4 = getelementptr inbounds nuw i8, ptr %out, i64 16
  %1 = load <4 x i32>, ptr %result.sroa.5.0.source.i.sroa_idx, align 1, !tbaa !1
  %consume.d8 = getelementptr inbounds nuw i8, ptr %out, i64 32
  %2 = load <4 x i32>, ptr %result.sroa.9.0.source.i.sroa_idx, align 1, !tbaa !1
  store <4 x i32> %0, ptr %out, align 1
  store <4 x i32> %1, ptr %consume.d4, align 1
  store <4 x i32> %2, ptr %consume.d8, align 1
  br label %common.ret
}

; Function Attrs: mustprogress nocallback nofree nounwind willreturn memory(argmem: readwrite)
declare void @llvm.memcpy.p0.p0.i64(ptr noalias writeonly captures(none), ptr noalias readonly captures(none), i64, i1 immarg) #2

attributes #0 = { mustprogress nofree norecurse nosync nounwind willreturn memory(argmem: readwrite) }
attributes #1 = { mustprogress nofree norecurse nosync nounwind willreturn memory(argmem: read) }
attributes #2 = { mustprogress nocallback nofree nounwind willreturn memory(argmem: readwrite) }

!0 = !{i64 0, i64 4, !1, i64 4, i64 4, !1, i64 8, i64 4, !1, i64 12, i64 4, !1, i64 16, i64 4, !1, i64 20, i64 4, !1, i64 24, i64 4, !1, i64 28, i64 4, !1, i64 32, i64 4, !1, i64 36, i64 4, !1, i64 40, i64 4, !1, i64 44, i64 4, !1}
!1 = !{!2, !2, i64 0}
!2 = !{!"i32 leaf", !3, i64 0}
!3 = !{!"Whitefoot synthetic record TBAA"}
!4 = !{!"branch_weights", i32 1, i32 10000}
