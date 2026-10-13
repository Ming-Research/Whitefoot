; Extracted from Snowghost-wf c8f5e17 layout_oracle IR (release wf-26355924f942,
; run 38198957372); functions: wf_layout.element_position wf_layout.computed_of wf_layout.font_index wf_layout.font_of wf_layout.same_shaping wf_layout.column_gap_of wf_layout.box_of wf_layout.initial_box wf_layout.positioned wf_layout.text_of wf_layout.valign_edge
source_filename = "whitefoot"
target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-unknown-linux-gnu"
!0 = !{!"Whitefoot checked storage roles"}
!1 = !{!"any memory", !0, i64 0}
!2 = !{!"source scalar write", !1, i64 0}
!3 = !{!"Box owner", !1, i64 0}
!4 = !{!"runtime Array length", !1, i64 0}
!5 = !{!"runtime Slots length", !1, i64 0}
!6 = !{!"runtime Ring length", !1, i64 0}
!7 = !{!"runtime window capacity", !1, i64 0}
!8 = !{!"runtime Ring head", !1, i64 0}
!9 = !{!"Paged length", !1, i64 0}
!10 = !{!"Paged directory", !1, i64 0}
!11 = !{!"Paged directory capacity", !1, i64 0}
!12 = !{!2, !2, i64 0}
!13 = !{!3, !3, i64 0}
!14 = !{!4, !4, i64 0}
!15 = !{!5, !5, i64 0}
!16 = !{!6, !6, i64 0}
!17 = !{!7, !7, i64 0}
!18 = !{!8, !8, i64 0}
!19 = !{!9, !9, i64 0}
!20 = !{!10, !10, i64 0}
!21 = !{!11, !11, i64 0}

%wf.t.07a97e4ae2e40a7a = type { %wf.t.518b4b787557360a, i8, %wf.t.e582deac3cb0a0f6, i8, i8, %wf.t.5ae2e280d1eb25be, %wf.t.5ae2e280d1eb25be, %wf.t.5ae2e280d1eb25be, i8, %wf.t.1f2ca470e213f17e, i8, i8 }
%wf.t.17d39b4707a3f3ff = type { ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, i64, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, %wf.t.645867fde9565119 }
%wf.t.1f2ca470e213f17e = type { i32 }
%wf.t.3b403febe3207150 = type { i8, i8, i8, i8, i8, i8, %wf.t.5ae2e280d1eb25be, %wf.t.5ae2e280d1eb25be, i32, i32, i32, i8, i32, i32, i32, %wf.t.5ae2e280d1eb25be, i8 }
%wf.t.4e2e3dc4f558bade = type { i32, i8, %wf.t.d43c5afff96c5f11 }
%wf.t.518b4b787557360a = type { i8, float, float, float, float, i8 }
%wf.t.5ae2e280d1eb25be = type { i8, i8, %wf.t.e582deac3cb0a0f6 }
%wf.t.645867fde9565119 = type { ptr, ptr, ptr, ptr, ptr, ptr, ptr, %wf.t.6694b177613cd46a, %wf.t.6694b177613cd46a, %wf.t.6694b177613cd46a, %wf.t.6694b177613cd46a, %wf.t.6694b177613cd46a, %wf.t.6694b177613cd46a, %wf.t.6694b177613cd46a, %wf.t.6694b177613cd46a, %wf.t.6694b177613cd46a, ptr, ptr }
%wf.t.66700addc1283274 = type { i8, i8, %wf.t.8b1203ba12af44dc, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, %wf.t.e6297f23cc4d955b, i32, i32, i32, i32, i32, i32, i32, i32, i32, i1, i1, i32, i32, i1, i32, i32, i1, %wf.t.e6297f23cc4d955b, i32, i32, i1, i32, i32, i1, i32, i1, i32, i32, i32, i32, i1, i32, i32, i32, ptr, i1, i32, i32, %wf.t.f25dc0fde8d0c64d, %wf.t.f25dc0fde8d0c64d }
%wf.t.6694b177613cd46a = type { ptr, ptr, ptr, i64 }
%wf.t.8b1203ba12af44dc = type { i32, i32, i1 }
%wf.t.9cbecabb2490f024 = type { i8, i8, i8, i8, i8, i8, i8, i8, i1, i32, i8 }
%wf.t.d43c5afff96c5f11 = type { i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32 }
%wf.t.ddf07502b6392f2c = type { i32, i32, float, float, i8, %wf.t.5ae2e280d1eb25be }
%wf.t.e582deac3cb0a0f6 = type { float, float, i1 }
%wf.t.e6297f23cc4d955b = type { i32, i32, i32, i32, i32, i1 }
%wf.t.f25dc0fde8d0c64d = type { ptr, i32, i64 }
%wf.t.fef67eaa91acc080 = type { i32, i32, i32, float, float, i1, i8, i8 }

define i64 @wf_layout.element_position(ptr noalias nonnull captures(none) dereferenceable(584) %v0, i32 %v1) #0 align 64 {
entry:
  %t0 = getelementptr inbounds %wf.t.17d39b4707a3f3ff, ptr %v0, i32 0, i32 0
  %v2 = getelementptr i8, ptr %t0, i64 0
  %t1 = load ptr, ptr %v2, !tbaa !13
  %v3 = getelementptr i8, ptr %t1, i64 0
  %t3 = getelementptr inbounds { i64, i64, [0 x %wf.t.d43c5afff96c5f11] }, ptr %v3, i32 0, i32 0
  %t2 = load i64, ptr %t3, !tbaa !15
  %v4 = add i64 %t2, 0
  %v5 = select i1 true, i32 4294967293, i32 4294967293
  %v6 = icmp eq i32 %v1, %v5
  switch i1 %v6, label %invalid.tag.b0 [
    i1 1, label %bb2
    i1 0, label %bb3
  ]
invalid.tag.b0:
  unreachable
bb1:
  %v7 = phi ptr [ %v0, %bb3 ]
  %v8 = phi i32 [ %v1, %bb3 ]
  %v9 = phi i64 [ %v4, %bb3 ]
  %v11 = zext i32 %v8 to i64
  %t4 = getelementptr inbounds %wf.t.17d39b4707a3f3ff, ptr %v7, i32 0, i32 27
  %v12 = getelementptr i8, ptr %t4, i64 0
  %t5 = load ptr, ptr %v12, !tbaa !13
  %v13 = getelementptr i8, ptr %t5, i64 0
  %t7 = getelementptr inbounds { i64, i64, [0 x i32] }, ptr %v13, i32 0, i32 0
  %t6 = load i64, ptr %t7, !tbaa !15
  %v14 = add i64 %t6, 0
  %v15 = icmp ult i64 %v11, %v14
  switch i1 %v15, label %invalid.tag.b1 [
    i1 1, label %bb5
    i1 0, label %bb6
  ]
invalid.tag.b1:
  unreachable
bb2:
  %v10 = select i1 true, i64 0, i64 0
  ret i64 %v10
bb3:
  br label %bb1
bb4:
  %v18 = phi i64 [ %v9, %bb6 ], [ %v28, %bb7 ]
  ret i64 %v18
bb5:
  %t8 = getelementptr inbounds %wf.t.17d39b4707a3f3ff, ptr %v7, i32 0, i32 27
  %v20 = getelementptr i8, ptr %t8, i64 0
  %t9 = load ptr, ptr %v20, !tbaa !13
  %v21 = getelementptr i8, ptr %t9, i64 0
  %t10.nonnegative = icmp sge i64 %v11, 0
  call void @llvm.assume(i1 %t10.nonnegative)
  %t10 = getelementptr inbounds { i64, i64, [0 x i32] }, ptr %v21, i64 0, i32 2, i64 %v11
  %v22 = getelementptr i8, ptr %t10, i64 0
  %v23 = load i32, ptr %v22
  %v24 = zext i32 %v23 to i64
  %v25 = icmp ult i64 %v24, %v9
  switch i1 %v25, label %invalid.tag.b5 [
    i1 1, label %bb8
    i1 0, label %bb9
  ]
invalid.tag.b5:
  unreachable
bb6:
  br label %bb4
bb7:
  %v28 = phi i64 [ %v9, %bb9 ]
  br label %bb4
bb8:
  ret i64 %v24
bb9:
  br label %bb7
}

define void @wf_layout.computed_of(ptr %wf.result, ptr noalias nonnull captures(none) dereferenceable(584) %v0, ptr %wf.arg.v1) #0 align 64 {
entry:
  %wf.slot.0 = alloca %wf.t.8b1203ba12af44dc, align 4
  %t0 = getelementptr inbounds %wf.t.8b1203ba12af44dc, ptr %wf.arg.v1, i32 0, i32 1
  %v2 = load i32, ptr %t0
  %v3 = select i1 true, i32 4294967295, i32 4294967295
  %v4 = icmp ne i32 %v2, %v3
  switch i1 %v4, label %invalid.tag.b0 [
    i1 1, label %bb2
    i1 0, label %bb3
  ]
invalid.tag.b0:
  unreachable
bb1:
  %v5 = phi ptr [ %v0, %bb3 ], [ %v13, %bb4 ]
  %t1 = getelementptr inbounds %wf.t.8b1203ba12af44dc, ptr %wf.arg.v1, i32 0, i32 0
  %v21 = load i32, ptr %t1
  %v22 = call i64 @wf_layout.element_position(ptr %v5, i32 %v21)
  %t2 = getelementptr inbounds %wf.t.17d39b4707a3f3ff, ptr %v5, i32 0, i32 0
  %v23 = getelementptr i8, ptr %t2, i64 0
  %t3 = load ptr, ptr %v23, !tbaa !13
  %v24 = getelementptr i8, ptr %t3, i64 0
  %t5 = getelementptr inbounds { i64, i64, [0 x %wf.t.d43c5afff96c5f11] }, ptr %v24, i32 0, i32 0
  %t4 = load i64, ptr %t5, !tbaa !15
  %v25 = add i64 %t4, 0
  %v26 = icmp ult i64 %v22, %v25
  switch i1 %v26, label %invalid.tag.b1 [
    i1 1, label %bb8
    i1 0, label %bb9
  ]
invalid.tag.b1:
  unreachable
bb2:
  %t6 = getelementptr inbounds %wf.t.8b1203ba12af44dc, ptr %wf.arg.v1, i32 0, i32 1
  %v7 = load i32, ptr %t6
  %v8 = zext i32 %v7 to i64
  %t7 = getelementptr inbounds %wf.t.17d39b4707a3f3ff, ptr %v0, i32 0, i32 26
  %v9 = getelementptr i8, ptr %t7, i64 0
  %t8 = load ptr, ptr %v9, !tbaa !13
  %v10 = getelementptr i8, ptr %t8, i64 0
  %t10 = getelementptr inbounds { i64, i64, [0 x %wf.t.4e2e3dc4f558bade] }, ptr %v10, i32 0, i32 0
  %t9 = load i64, ptr %t10, !tbaa !15
  %v11 = add i64 %t9, 0
  %v12 = icmp ult i64 %v8, %v11
  switch i1 %v12, label %invalid.tag.b2 [
    i1 1, label %bb5
    i1 0, label %bb6
  ]
invalid.tag.b2:
  unreachable
bb3:
  br label %bb1
bb4:
  %v13 = phi ptr [ %v0, %bb6 ]
  br label %bb1
bb5:
  %t11 = getelementptr inbounds %wf.t.17d39b4707a3f3ff, ptr %v0, i32 0, i32 26
  %v16 = getelementptr i8, ptr %t11, i64 0
  %t12 = load ptr, ptr %v16, !tbaa !13
  %v17 = getelementptr i8, ptr %t12, i64 0
  %t13.nonnegative = icmp sge i64 %v8, 0
  call void @llvm.assume(i1 %t13.nonnegative)
  %t13 = getelementptr inbounds { i64, i64, [0 x %wf.t.4e2e3dc4f558bade] }, ptr %v17, i64 0, i32 2, i64 %v8
  %v18 = getelementptr i8, ptr %t13, i64 0
  %t14 = getelementptr inbounds %wf.t.4e2e3dc4f558bade, ptr %v18, i32 0, i32 2
  %v19 = getelementptr i8, ptr %t14, i64 0
  ; layout-bounded transfer from %v19 to %wf.result
  %t15 = load i32, ptr %v19, align 1
  %t16 = getelementptr inbounds i8, ptr %v19, i64 4
  %t17 = load i32, ptr %t16, align 1
  %t18 = getelementptr inbounds i8, ptr %v19, i64 8
  %t19 = load i32, ptr %t18, align 1
  %t20 = getelementptr inbounds i8, ptr %v19, i64 12
  %t21 = load i32, ptr %t20, align 1
  %t22 = getelementptr inbounds i8, ptr %v19, i64 16
  %t23 = load i32, ptr %t22, align 1
  %t24 = getelementptr inbounds i8, ptr %v19, i64 20
  %t25 = load i32, ptr %t24, align 1
  %t26 = getelementptr inbounds i8, ptr %v19, i64 24
  %t27 = load i32, ptr %t26, align 1
  %t28 = getelementptr inbounds i8, ptr %v19, i64 28
  %t29 = load i32, ptr %t28, align 1
  %t30 = getelementptr inbounds i8, ptr %v19, i64 32
  %t31 = load i32, ptr %t30, align 1
  %t32 = getelementptr inbounds i8, ptr %v19, i64 36
  %t33 = load i32, ptr %t32, align 1
  %t34 = getelementptr inbounds i8, ptr %v19, i64 40
  %t35 = load i32, ptr %t34, align 1
  %t36 = getelementptr inbounds i8, ptr %v19, i64 44
  %t37 = load i32, ptr %t36, align 1
  store i32 %t15, ptr %wf.result, align 1
  %t38 = getelementptr inbounds i8, ptr %wf.result, i64 4
  store i32 %t17, ptr %t38, align 1
  %t39 = getelementptr inbounds i8, ptr %wf.result, i64 8
  store i32 %t19, ptr %t39, align 1
  %t40 = getelementptr inbounds i8, ptr %wf.result, i64 12
  store i32 %t21, ptr %t40, align 1
  %t41 = getelementptr inbounds i8, ptr %wf.result, i64 16
  store i32 %t23, ptr %t41, align 1
  %t42 = getelementptr inbounds i8, ptr %wf.result, i64 20
  store i32 %t25, ptr %t42, align 1
  %t43 = getelementptr inbounds i8, ptr %wf.result, i64 24
  store i32 %t27, ptr %t43, align 1
  %t44 = getelementptr inbounds i8, ptr %wf.result, i64 28
  store i32 %t29, ptr %t44, align 1
  %t45 = getelementptr inbounds i8, ptr %wf.result, i64 32
  store i32 %t31, ptr %t45, align 1
  %t46 = getelementptr inbounds i8, ptr %wf.result, i64 36
  store i32 %t33, ptr %t46, align 1
  %t47 = getelementptr inbounds i8, ptr %wf.result, i64 40
  store i32 %t35, ptr %t47, align 1
  %t48 = getelementptr inbounds i8, ptr %wf.result, i64 44
  store i32 %t37, ptr %t48, align 1
  ; end layout-bounded transfer from %v19 to %wf.result
  ret void
bb6:
  br label %bb4
bb7:
  %v34 = select i1 true, i32 0, i32 0
  %v35 = select i1 true, i32 0, i32 0
  %v36 = select i1 true, i32 0, i32 0
  %v37 = select i1 true, i32 0, i32 0
  %v38 = select i1 true, i32 0, i32 0
  %v39 = select i1 true, i32 0, i32 0
  %v40 = select i1 true, i32 0, i32 0
  %v41 = select i1 true, i32 0, i32 0
  %v42 = select i1 true, i32 0, i32 0
  %v43 = select i1 true, i32 0, i32 0
  %v44 = select i1 true, i32 0, i32 0
  %v45 = select i1 true, i32 0, i32 0
  store %wf.t.d43c5afff96c5f11 zeroinitializer, ptr %wf.result
  %t49 = getelementptr inbounds %wf.t.d43c5afff96c5f11, ptr %wf.result, i32 0, i32 0
  store i32 %v34, ptr %t49
  %t50 = getelementptr inbounds %wf.t.d43c5afff96c5f11, ptr %wf.result, i32 0, i32 1
  store i32 %v35, ptr %t50
  %t51 = getelementptr inbounds %wf.t.d43c5afff96c5f11, ptr %wf.result, i32 0, i32 2
  store i32 %v36, ptr %t51
  %t52 = getelementptr inbounds %wf.t.d43c5afff96c5f11, ptr %wf.result, i32 0, i32 3
  store i32 %v37, ptr %t52
  %t53 = getelementptr inbounds %wf.t.d43c5afff96c5f11, ptr %wf.result, i32 0, i32 4
  store i32 %v38, ptr %t53
  %t54 = getelementptr inbounds %wf.t.d43c5afff96c5f11, ptr %wf.result, i32 0, i32 5
  store i32 %v39, ptr %t54
  %t55 = getelementptr inbounds %wf.t.d43c5afff96c5f11, ptr %wf.result, i32 0, i32 6
  store i32 %v40, ptr %t55
  %t56 = getelementptr inbounds %wf.t.d43c5afff96c5f11, ptr %wf.result, i32 0, i32 7
  store i32 %v41, ptr %t56
  %t57 = getelementptr inbounds %wf.t.d43c5afff96c5f11, ptr %wf.result, i32 0, i32 8
  store i32 %v42, ptr %t57
  %t58 = getelementptr inbounds %wf.t.d43c5afff96c5f11, ptr %wf.result, i32 0, i32 9
  store i32 %v43, ptr %t58
  %t59 = getelementptr inbounds %wf.t.d43c5afff96c5f11, ptr %wf.result, i32 0, i32 10
  store i32 %v44, ptr %t59
  %t60 = getelementptr inbounds %wf.t.d43c5afff96c5f11, ptr %wf.result, i32 0, i32 11
  store i32 %v45, ptr %t60
  ret void
bb8:
  %t61 = getelementptr inbounds %wf.t.17d39b4707a3f3ff, ptr %v5, i32 0, i32 0
  %v30 = getelementptr i8, ptr %t61, i64 0
  %t62 = load ptr, ptr %v30, !tbaa !13
  %v31 = getelementptr i8, ptr %t62, i64 0
  %t63.nonnegative = icmp sge i64 %v22, 0
  call void @llvm.assume(i1 %t63.nonnegative)
  %t63 = getelementptr inbounds { i64, i64, [0 x %wf.t.d43c5afff96c5f11] }, ptr %v31, i64 0, i32 2, i64 %v22
  %v32 = getelementptr i8, ptr %t63, i64 0
  ; layout-bounded transfer from %v32 to %wf.result
  %t64 = load i32, ptr %v32, align 1
  %t65 = getelementptr inbounds i8, ptr %v32, i64 4
  %t66 = load i32, ptr %t65, align 1
  %t67 = getelementptr inbounds i8, ptr %v32, i64 8
  %t68 = load i32, ptr %t67, align 1
  %t69 = getelementptr inbounds i8, ptr %v32, i64 12
  %t70 = load i32, ptr %t69, align 1
  %t71 = getelementptr inbounds i8, ptr %v32, i64 16
  %t72 = load i32, ptr %t71, align 1
  %t73 = getelementptr inbounds i8, ptr %v32, i64 20
  %t74 = load i32, ptr %t73, align 1
  %t75 = getelementptr inbounds i8, ptr %v32, i64 24
  %t76 = load i32, ptr %t75, align 1
  %t77 = getelementptr inbounds i8, ptr %v32, i64 28
  %t78 = load i32, ptr %t77, align 1
  %t79 = getelementptr inbounds i8, ptr %v32, i64 32
  %t80 = load i32, ptr %t79, align 1
  %t81 = getelementptr inbounds i8, ptr %v32, i64 36
  %t82 = load i32, ptr %t81, align 1
  %t83 = getelementptr inbounds i8, ptr %v32, i64 40
  %t84 = load i32, ptr %t83, align 1
  %t85 = getelementptr inbounds i8, ptr %v32, i64 44
  %t86 = load i32, ptr %t85, align 1
  store i32 %t64, ptr %wf.result, align 1
  %t87 = getelementptr inbounds i8, ptr %wf.result, i64 4
  store i32 %t66, ptr %t87, align 1
  %t88 = getelementptr inbounds i8, ptr %wf.result, i64 8
  store i32 %t68, ptr %t88, align 1
  %t89 = getelementptr inbounds i8, ptr %wf.result, i64 12
  store i32 %t70, ptr %t89, align 1
  %t90 = getelementptr inbounds i8, ptr %wf.result, i64 16
  store i32 %t72, ptr %t90, align 1
  %t91 = getelementptr inbounds i8, ptr %wf.result, i64 20
  store i32 %t74, ptr %t91, align 1
  %t92 = getelementptr inbounds i8, ptr %wf.result, i64 24
  store i32 %t76, ptr %t92, align 1
  %t93 = getelementptr inbounds i8, ptr %wf.result, i64 28
  store i32 %t78, ptr %t93, align 1
  %t94 = getelementptr inbounds i8, ptr %wf.result, i64 32
  store i32 %t80, ptr %t94, align 1
  %t95 = getelementptr inbounds i8, ptr %wf.result, i64 36
  store i32 %t82, ptr %t95, align 1
  %t96 = getelementptr inbounds i8, ptr %wf.result, i64 40
  store i32 %t84, ptr %t96, align 1
  %t97 = getelementptr inbounds i8, ptr %wf.result, i64 44
  store i32 %t86, ptr %t97, align 1
  ; end layout-bounded transfer from %v32 to %wf.result
  ret void
bb9:
  br label %bb7
}

define i32 @wf_layout.font_index(ptr noalias nonnull captures(none) dereferenceable(584) %v0, ptr %wf.arg.v1) #0 align 64 {
entry:
  %wf.slot.0 = alloca %wf.t.8b1203ba12af44dc, align 4
  %wf.slot.1 = alloca %wf.t.d43c5afff96c5f11, align 4
  call void @wf_layout.computed_of(ptr %wf.slot.1, ptr %v0, ptr %wf.arg.v1)
  %t0 = getelementptr inbounds %wf.t.d43c5afff96c5f11, ptr %wf.slot.1, i32 0, i32 4
  %v3 = load i32, ptr %t0
  ret i32 %v3
}

define void @wf_layout.font_of(ptr %wf.result, ptr noalias nonnull captures(none) dereferenceable(584) %v0, ptr %wf.arg.v1) #0 align 64 {
entry:
  %wf.slot.0 = alloca %wf.t.8b1203ba12af44dc, align 4
  %wf.slot.1 = alloca %wf.t.d43c5afff96c5f11, align 4
  %wf.slot.2 = alloca %wf.t.e582deac3cb0a0f6, align 4
  %wf.slot.3 = alloca %wf.t.5ae2e280d1eb25be, align 4
  call void @wf_layout.computed_of(ptr %wf.slot.1, ptr %v0, ptr %wf.arg.v1)
  %t0 = getelementptr inbounds %wf.t.d43c5afff96c5f11, ptr %wf.slot.1, i32 0, i32 4
  %v3 = load i32, ptr %t0
  %v4 = zext i32 %v3 to i64
  %t1 = getelementptr inbounds %wf.t.17d39b4707a3f3ff, ptr %v0, i32 0, i32 5
  %v5 = getelementptr i8, ptr %t1, i64 0
  %t2 = load ptr, ptr %v5, !tbaa !13
  %v6 = getelementptr i8, ptr %t2, i64 0
  %t4 = getelementptr inbounds { i64, i64, [0 x %wf.t.ddf07502b6392f2c] }, ptr %v6, i32 0, i32 0
  %t3 = load i64, ptr %t4, !tbaa !15
  %v7 = add i64 %t3, 0
  %v8 = icmp ult i64 %v4, %v7
  switch i1 %v8, label %invalid.tag.b0 [
    i1 1, label %bb2
    i1 0, label %bb3
  ]
invalid.tag.b0:
  unreachable
bb1:
  %v9 = phi ptr [ %v0, %bb3 ]
  %v17 = select i1 true, i64 0, i64 0
  %t5 = getelementptr inbounds %wf.t.17d39b4707a3f3ff, ptr %v9, i32 0, i32 5
  %v18 = getelementptr i8, ptr %t5, i64 0
  %t6 = load ptr, ptr %v18, !tbaa !13
  %v19 = getelementptr i8, ptr %t6, i64 0
  %t8 = getelementptr inbounds { i64, i64, [0 x %wf.t.ddf07502b6392f2c] }, ptr %v19, i32 0, i32 0
  %t7 = load i64, ptr %t8, !tbaa !15
  %v20 = add i64 %t7, 0
  %v21 = icmp ult i64 %v17, %v20
  switch i1 %v21, label %invalid.tag.b1 [
    i1 1, label %bb5
    i1 0, label %bb6
  ]
invalid.tag.b1:
  unreachable
bb2:
  %t9 = getelementptr inbounds %wf.t.17d39b4707a3f3ff, ptr %v0, i32 0, i32 5
  %v13 = getelementptr i8, ptr %t9, i64 0
  %t10 = load ptr, ptr %v13, !tbaa !13
  %v14 = getelementptr i8, ptr %t10, i64 0
  %t11.nonnegative = icmp sge i64 %v4, 0
  call void @llvm.assume(i1 %t11.nonnegative)
  %t11 = getelementptr inbounds { i64, i64, [0 x %wf.t.ddf07502b6392f2c] }, ptr %v14, i64 0, i32 2, i64 %v4
  %v15 = getelementptr i8, ptr %t11, i64 0
  ; layout-bounded transfer from %v15 to %wf.result
  %t12 = load i32, ptr %v15, align 1
  %t13 = getelementptr inbounds i8, ptr %v15, i64 4
  %t14 = load i32, ptr %t13, align 1
  %t15 = getelementptr inbounds i8, ptr %v15, i64 8
  %t16 = load float, ptr %t15, align 1
  %t17 = getelementptr inbounds i8, ptr %v15, i64 12
  %t18 = load float, ptr %t17, align 1
  %t19 = getelementptr inbounds i8, ptr %v15, i64 16
  %t20 = load i8, ptr %t19, align 1
  %t21 = getelementptr inbounds i8, ptr %v15, i64 20
  %t22 = load i8, ptr %t21, align 1
  %t23 = getelementptr inbounds i8, ptr %v15, i64 21
  %t24 = load i8, ptr %t23, align 1
  %t25 = getelementptr inbounds i8, ptr %v15, i64 24
  %t26 = load float, ptr %t25, align 1
  %t27 = getelementptr inbounds i8, ptr %v15, i64 28
  %t28 = load float, ptr %t27, align 1
  %t29 = getelementptr inbounds i8, ptr %v15, i64 32
  %t30 = load i1, ptr %t29, align 1
  store i32 %t12, ptr %wf.result, align 1
  %t31 = getelementptr inbounds i8, ptr %wf.result, i64 4
  store i32 %t14, ptr %t31, align 1
  %t32 = getelementptr inbounds i8, ptr %wf.result, i64 8
  store float %t16, ptr %t32, align 1
  %t33 = getelementptr inbounds i8, ptr %wf.result, i64 12
  store float %t18, ptr %t33, align 1
  %t34 = getelementptr inbounds i8, ptr %wf.result, i64 16
  store i8 %t20, ptr %t34, align 1
  %t35 = getelementptr inbounds i8, ptr %wf.result, i64 20
  store i8 %t22, ptr %t35, align 1
  %t36 = getelementptr inbounds i8, ptr %wf.result, i64 21
  store i8 %t24, ptr %t36, align 1
  %t37 = getelementptr inbounds i8, ptr %wf.result, i64 24
  store float %t26, ptr %t37, align 1
  %t38 = getelementptr inbounds i8, ptr %wf.result, i64 28
  store float %t28, ptr %t38, align 1
  %t39 = getelementptr inbounds i8, ptr %wf.result, i64 32
  store i1 %t30, ptr %t39, align 1
  ; end layout-bounded transfer from %v15 to %wf.result
  ret void
bb3:
  br label %bb1
bb4:
  %v31 = select i1 true, i1 0, i1 0
  %v32 = select i1 true, float 0x0000000000000000, float 0x0000000000000000
  %v33 = select i1 true, float 0x0000000000000000, float 0x0000000000000000
  store %wf.t.e582deac3cb0a0f6 zeroinitializer, ptr %wf.slot.2
  %t40 = getelementptr inbounds %wf.t.e582deac3cb0a0f6, ptr %wf.slot.2, i32 0, i32 0
  store float %v32, ptr %t40
  %t41 = getelementptr inbounds %wf.t.e582deac3cb0a0f6, ptr %wf.slot.2, i32 0, i32 1
  store float %v33, ptr %t41
  %t42 = getelementptr inbounds %wf.t.e582deac3cb0a0f6, ptr %wf.slot.2, i32 0, i32 2
  store i1 %v31, ptr %t42
  %v35 = select i1 true, i8 3, i8 3
  %v36 = select i1 true, i8 0, i8 0
  store %wf.t.5ae2e280d1eb25be zeroinitializer, ptr %wf.slot.3
  %t43 = getelementptr inbounds %wf.t.5ae2e280d1eb25be, ptr %wf.slot.3, i32 0, i32 0
  store i8 %v35, ptr %t43
  %t44 = getelementptr inbounds %wf.t.5ae2e280d1eb25be, ptr %wf.slot.3, i32 0, i32 1
  store i8 %v36, ptr %t44
  %t45 = getelementptr inbounds %wf.t.5ae2e280d1eb25be, ptr %wf.slot.3, i32 0, i32 2
  ; layout-bounded transfer from %wf.slot.2 to %t45
  %t46 = load float, ptr %wf.slot.2, align 1
  %t47 = getelementptr inbounds i8, ptr %wf.slot.2, i64 4
  %t48 = load float, ptr %t47, align 1
  %t49 = getelementptr inbounds i8, ptr %wf.slot.2, i64 8
  %t50 = load i1, ptr %t49, align 1
  store float %t46, ptr %t45, align 1
  %t51 = getelementptr inbounds i8, ptr %t45, i64 4
  store float %t48, ptr %t51, align 1
  %t52 = getelementptr inbounds i8, ptr %t45, i64 8
  store i1 %t50, ptr %t52, align 1
  ; end layout-bounded transfer from %wf.slot.2 to %t45
  %v38 = select i1 true, i32 0, i32 0
  %v39 = select i1 true, i32 0, i32 0
  %v40 = select i1 true, float 0x4030000000000000, float 0x4030000000000000
  %v41 = select i1 true, float 0x4079000000000000, float 0x4079000000000000
  %v42 = select i1 true, i8 0, i8 0
  store %wf.t.ddf07502b6392f2c zeroinitializer, ptr %wf.result
  %t53 = getelementptr inbounds %wf.t.ddf07502b6392f2c, ptr %wf.result, i32 0, i32 0
  store i32 %v38, ptr %t53
  %t54 = getelementptr inbounds %wf.t.ddf07502b6392f2c, ptr %wf.result, i32 0, i32 1
  store i32 %v39, ptr %t54
  %t55 = getelementptr inbounds %wf.t.ddf07502b6392f2c, ptr %wf.result, i32 0, i32 2
  store float %v40, ptr %t55
  %t56 = getelementptr inbounds %wf.t.ddf07502b6392f2c, ptr %wf.result, i32 0, i32 3
  store float %v41, ptr %t56
  %t57 = getelementptr inbounds %wf.t.ddf07502b6392f2c, ptr %wf.result, i32 0, i32 4
  store i8 %v42, ptr %t57
  %t58 = getelementptr inbounds %wf.t.ddf07502b6392f2c, ptr %wf.result, i32 0, i32 5
  ; layout-bounded transfer from %wf.slot.3 to %t58
  %t59 = load i8, ptr %wf.slot.3, align 1
  %t60 = getelementptr inbounds i8, ptr %wf.slot.3, i64 1
  %t61 = load i8, ptr %t60, align 1
  %t62 = getelementptr inbounds i8, ptr %wf.slot.3, i64 4
  %t63 = load float, ptr %t62, align 1
  %t64 = getelementptr inbounds i8, ptr %wf.slot.3, i64 8
  %t65 = load float, ptr %t64, align 1
  %t66 = getelementptr inbounds i8, ptr %wf.slot.3, i64 12
  %t67 = load i1, ptr %t66, align 1
  store i8 %t59, ptr %t58, align 1
  %t68 = getelementptr inbounds i8, ptr %t58, i64 1
  store i8 %t61, ptr %t68, align 1
  %t69 = getelementptr inbounds i8, ptr %t58, i64 4
  store float %t63, ptr %t69, align 1
  %t70 = getelementptr inbounds i8, ptr %t58, i64 8
  store float %t65, ptr %t70, align 1
  %t71 = getelementptr inbounds i8, ptr %t58, i64 12
  store i1 %t67, ptr %t71, align 1
  ; end layout-bounded transfer from %wf.slot.3 to %t58
  ret void
bb5:
  %t72 = getelementptr inbounds %wf.t.17d39b4707a3f3ff, ptr %v9, i32 0, i32 5
  %v26 = getelementptr i8, ptr %t72, i64 0
  %t73 = load ptr, ptr %v26, !tbaa !13
  %v27 = getelementptr i8, ptr %t73, i64 0
  %v28 = select i1 true, i64 0, i64 0
  %t74.nonnegative = icmp sge i64 %v28, 0
  call void @llvm.assume(i1 %t74.nonnegative)
  %t74 = getelementptr inbounds { i64, i64, [0 x %wf.t.ddf07502b6392f2c] }, ptr %v27, i64 0, i32 2, i64 %v28
  %v29 = getelementptr i8, ptr %t74, i64 0
  ; layout-bounded transfer from %v29 to %wf.result
  %t75 = load i32, ptr %v29, align 1
  %t76 = getelementptr inbounds i8, ptr %v29, i64 4
  %t77 = load i32, ptr %t76, align 1
  %t78 = getelementptr inbounds i8, ptr %v29, i64 8
  %t79 = load float, ptr %t78, align 1
  %t80 = getelementptr inbounds i8, ptr %v29, i64 12
  %t81 = load float, ptr %t80, align 1
  %t82 = getelementptr inbounds i8, ptr %v29, i64 16
  %t83 = load i8, ptr %t82, align 1
  %t84 = getelementptr inbounds i8, ptr %v29, i64 20
  %t85 = load i8, ptr %t84, align 1
  %t86 = getelementptr inbounds i8, ptr %v29, i64 21
  %t87 = load i8, ptr %t86, align 1
  %t88 = getelementptr inbounds i8, ptr %v29, i64 24
  %t89 = load float, ptr %t88, align 1
  %t90 = getelementptr inbounds i8, ptr %v29, i64 28
  %t91 = load float, ptr %t90, align 1
  %t92 = getelementptr inbounds i8, ptr %v29, i64 32
  %t93 = load i1, ptr %t92, align 1
  store i32 %t75, ptr %wf.result, align 1
  %t94 = getelementptr inbounds i8, ptr %wf.result, i64 4
  store i32 %t77, ptr %t94, align 1
  %t95 = getelementptr inbounds i8, ptr %wf.result, i64 8
  store float %t79, ptr %t95, align 1
  %t96 = getelementptr inbounds i8, ptr %wf.result, i64 12
  store float %t81, ptr %t96, align 1
  %t97 = getelementptr inbounds i8, ptr %wf.result, i64 16
  store i8 %t83, ptr %t97, align 1
  %t98 = getelementptr inbounds i8, ptr %wf.result, i64 20
  store i8 %t85, ptr %t98, align 1
  %t99 = getelementptr inbounds i8, ptr %wf.result, i64 21
  store i8 %t87, ptr %t99, align 1
  %t100 = getelementptr inbounds i8, ptr %wf.result, i64 24
  store float %t89, ptr %t100, align 1
  %t101 = getelementptr inbounds i8, ptr %wf.result, i64 28
  store float %t91, ptr %t101, align 1
  %t102 = getelementptr inbounds i8, ptr %wf.result, i64 32
  store i1 %t93, ptr %t102, align 1
  ; end layout-bounded transfer from %v29 to %wf.result
  ret void
bb6:
  br label %bb4
}

define i1 @wf_layout.same_shaping(ptr noalias nonnull captures(none) dereferenceable(584) %v0, ptr %wf.arg.v1, ptr %wf.arg.v2, ptr %wf.arg.v3, ptr %wf.arg.v4) #0 align 64 {
entry:
  %wf.slot.0 = alloca %wf.t.fef67eaa91acc080, align 4
  %wf.slot.1 = alloca %wf.t.8b1203ba12af44dc, align 4
  %wf.slot.2 = alloca %wf.t.fef67eaa91acc080, align 4
  %wf.slot.3 = alloca %wf.t.8b1203ba12af44dc, align 4
  %wf.slot.4 = alloca %wf.t.ddf07502b6392f2c, align 4
  %wf.slot.5 = alloca %wf.t.ddf07502b6392f2c, align 4
  %wf.slot.6 = alloca %wf.t.07a97e4ae2e40a7a, align 4
  %wf.slot.7 = alloca %wf.t.07a97e4ae2e40a7a, align 4
  call void @wf_layout.font_of(ptr %wf.slot.4, ptr %v0, ptr %wf.arg.v2)
  call void @wf_layout.font_of(ptr %wf.slot.5, ptr %v0, ptr %wf.arg.v4)
  %t0 = getelementptr inbounds %wf.t.ddf07502b6392f2c, ptr %wf.slot.4, i32 0, i32 0
  %v7 = load i32, ptr %t0
  %t1 = getelementptr inbounds %wf.t.ddf07502b6392f2c, ptr %wf.slot.5, i32 0, i32 0
  %v8 = load i32, ptr %t1
  %v9 = icmp eq i32 %v7, %v8
  %t2 = getelementptr inbounds %wf.t.ddf07502b6392f2c, ptr %wf.slot.4, i32 0, i32 1
  %v10 = load i32, ptr %t2
  %t3 = getelementptr inbounds %wf.t.ddf07502b6392f2c, ptr %wf.slot.5, i32 0, i32 1
  %v11 = load i32, ptr %t3
  %v12 = icmp eq i32 %v10, %v11
  %v13 = and i1 %v9, %v12
  %t4 = getelementptr inbounds %wf.t.ddf07502b6392f2c, ptr %wf.slot.4, i32 0, i32 2
  %v14 = load float, ptr %t4
  %t5 = getelementptr inbounds %wf.t.ddf07502b6392f2c, ptr %wf.slot.5, i32 0, i32 2
  %v15 = load float, ptr %t5
  %v16 = fcmp oeq float %v14, %v15
  %t6 = getelementptr inbounds %wf.t.ddf07502b6392f2c, ptr %wf.slot.4, i32 0, i32 3
  %v17 = load float, ptr %t6
  %t7 = getelementptr inbounds %wf.t.ddf07502b6392f2c, ptr %wf.slot.5, i32 0, i32 3
  %v18 = load float, ptr %t7
  %v19 = fcmp oeq float %v17, %v18
  %t8 = getelementptr inbounds %wf.t.ddf07502b6392f2c, ptr %wf.slot.4, i32 0, i32 4
  %v20 = load i8, ptr %t8
  %t9 = getelementptr inbounds %wf.t.ddf07502b6392f2c, ptr %wf.slot.5, i32 0, i32 4
  %v21 = load i8, ptr %t9
  %v22 = icmp eq i8 %v20, %v21
  %v23 = and i1 %v16, %v19
  %v24 = and i1 %v23, %v22
  %v25 = and i1 %v13, %v24
  %t10 = getelementptr inbounds %wf.t.fef67eaa91acc080, ptr %wf.arg.v1, i32 0, i32 3
  %v26 = load float, ptr %t10
  %t11 = getelementptr inbounds %wf.t.fef67eaa91acc080, ptr %wf.arg.v3, i32 0, i32 3
  %v27 = load float, ptr %t11
  %v28 = fcmp oeq float %v26, %v27
  %t12 = getelementptr inbounds %wf.t.fef67eaa91acc080, ptr %wf.arg.v1, i32 0, i32 4
  %v29 = load float, ptr %t12
  %t13 = getelementptr inbounds %wf.t.fef67eaa91acc080, ptr %wf.arg.v3, i32 0, i32 4
  %v30 = load float, ptr %t13
  %v31 = fcmp oeq float %v29, %v30
  %v32 = and i1 %v28, %v31
  %t14 = getelementptr inbounds %wf.t.fef67eaa91acc080, ptr %wf.arg.v1, i32 0, i32 5
  %v33 = load i1, ptr %t14
  %t15 = getelementptr inbounds %wf.t.fef67eaa91acc080, ptr %wf.arg.v3, i32 0, i32 5
  %v34 = load i1, ptr %t15
  %v35 = icmp eq i1 %v33, %v34
  %t16 = getelementptr inbounds %wf.t.fef67eaa91acc080, ptr %wf.arg.v1, i32 0, i32 6
  %v36 = load i8, ptr %t16
  %t17 = getelementptr inbounds %wf.t.fef67eaa91acc080, ptr %wf.arg.v3, i32 0, i32 6
  %v37 = load i8, ptr %t17
  %v38 = icmp eq i8 %v36, %v37
  %t18 = getelementptr inbounds %wf.t.fef67eaa91acc080, ptr %wf.arg.v1, i32 0, i32 7
  %v39 = load i8, ptr %t18
  %t19 = getelementptr inbounds %wf.t.fef67eaa91acc080, ptr %wf.arg.v3, i32 0, i32 7
  %v40 = load i8, ptr %t19
  %v41 = icmp eq i8 %v39, %v40
  %v42 = and i1 %v38, %v41
  %v43 = and i1 %v35, %v42
  call void @wf_layout.text_of(ptr %wf.slot.6, ptr %v0, ptr %wf.arg.v2)
  call void @wf_layout.text_of(ptr %wf.slot.7, ptr %v0, ptr %wf.arg.v4)
  %t20 = getelementptr inbounds %wf.t.07a97e4ae2e40a7a, ptr %wf.slot.6, i32 0, i32 4
  %v46 = load i8, ptr %t20
  %t21 = getelementptr inbounds %wf.t.07a97e4ae2e40a7a, ptr %wf.slot.7, i32 0, i32 4
  %v47 = load i8, ptr %t21
  %v48 = icmp eq i8 %v46, %v47
  %v49 = and i1 %v32, %v43
  %v50 = and i1 %v25, %v48
  %v51 = and i1 %v49, %v50
  ret i1 %v51
}

define i32 @wf_layout.column_gap_of(ptr noalias nonnull captures(none) dereferenceable(584) %v0, ptr %wf.arg.v1, i32 %v2) #0 align 64 {
entry:
  %wf.slot.0 = alloca %wf.t.8b1203ba12af44dc, align 4
  %wf.slot.1 = alloca %wf.t.3b403febe3207150, align 4
  %wf.slot.2 = alloca %wf.t.5ae2e280d1eb25be, align 4
  %wf.slot.3 = alloca %wf.t.5ae2e280d1eb25be, align 4
  %wf.slot.4 = alloca %wf.t.e582deac3cb0a0f6, align 4
  %wf.slot.5 = alloca %wf.t.ddf07502b6392f2c, align 4
  call void @wf_layout.container_of(ptr %wf.slot.1, ptr %v0, ptr %wf.arg.v1)
  %t0 = getelementptr inbounds %wf.t.3b403febe3207150, ptr %wf.slot.1, i32 0, i32 7
  ; layout-bounded transfer from %t0 to %wf.slot.2
  %t1 = load i8, ptr %t0, align 1
  %t2 = getelementptr inbounds i8, ptr %t0, i64 1
  %t3 = load i8, ptr %t2, align 1
  %t4 = getelementptr inbounds i8, ptr %t0, i64 4
  %t5 = load float, ptr %t4, align 1
  %t6 = getelementptr inbounds i8, ptr %t0, i64 8
  %t7 = load float, ptr %t6, align 1
  %t8 = getelementptr inbounds i8, ptr %t0, i64 12
  %t9 = load i1, ptr %t8, align 1
  store i8 %t1, ptr %wf.slot.2, align 1
  %t10 = getelementptr inbounds i8, ptr %wf.slot.2, i64 1
  store i8 %t3, ptr %t10, align 1
  %t11 = getelementptr inbounds i8, ptr %wf.slot.2, i64 4
  store float %t5, ptr %t11, align 1
  %t12 = getelementptr inbounds i8, ptr %wf.slot.2, i64 8
  store float %t7, ptr %t12, align 1
  %t13 = getelementptr inbounds i8, ptr %wf.slot.2, i64 12
  store i1 %t9, ptr %t13, align 1
  ; end layout-bounded transfer from %t0 to %wf.slot.2
  %t14 = getelementptr inbounds %wf.t.5ae2e280d1eb25be, ptr %wf.slot.2, i32 0, i32 0
  %v5 = load i8, ptr %t14
  %v6 = select i1 true, i8 0, i8 0
  %v7 = icmp eq i8 %v5, %v6
  switch i1 %v7, label %invalid.tag.b0 [
    i1 1, label %bb2
    i1 0, label %bb3
  ]
invalid.tag.b0:
  unreachable
bb1:
  %v8 = phi ptr [ %v0, %bb3 ]
  call void @wf_layout.font_of(ptr %wf.slot.5, ptr %v8, ptr %wf.arg.v1)
  %t15 = getelementptr inbounds %wf.t.ddf07502b6392f2c, ptr %wf.slot.5, i32 0, i32 2
  %v18 = load float, ptr %t15
  %v19 = call i32 @wf_layout.units(float %v18)
  ret i32 %v19
bb2:
  %t16 = getelementptr inbounds %wf.t.3b403febe3207150, ptr %wf.slot.1, i32 0, i32 7
  ; layout-bounded transfer from %t16 to %wf.slot.3
  %t17 = load i8, ptr %t16, align 1
  %t18 = getelementptr inbounds i8, ptr %t16, i64 1
  %t19 = load i8, ptr %t18, align 1
  %t20 = getelementptr inbounds i8, ptr %t16, i64 4
  %t21 = load float, ptr %t20, align 1
  %t22 = getelementptr inbounds i8, ptr %t16, i64 8
  %t23 = load float, ptr %t22, align 1
  %t24 = getelementptr inbounds i8, ptr %t16, i64 12
  %t25 = load i1, ptr %t24, align 1
  store i8 %t17, ptr %wf.slot.3, align 1
  %t26 = getelementptr inbounds i8, ptr %wf.slot.3, i64 1
  store i8 %t19, ptr %t26, align 1
  %t27 = getelementptr inbounds i8, ptr %wf.slot.3, i64 4
  store float %t21, ptr %t27, align 1
  %t28 = getelementptr inbounds i8, ptr %wf.slot.3, i64 8
  store float %t23, ptr %t28, align 1
  %t29 = getelementptr inbounds i8, ptr %wf.slot.3, i64 12
  store i1 %t25, ptr %t29, align 1
  ; end layout-bounded transfer from %t16 to %wf.slot.3
  %t30 = getelementptr inbounds %wf.t.5ae2e280d1eb25be, ptr %wf.slot.3, i32 0, i32 2
  ; layout-bounded transfer from %t30 to %wf.slot.4
  %t31 = load float, ptr %t30, align 1
  %t32 = getelementptr inbounds i8, ptr %t30, i64 4
  %t33 = load float, ptr %t32, align 1
  %t34 = getelementptr inbounds i8, ptr %t30, i64 8
  %t35 = load i1, ptr %t34, align 1
  store float %t31, ptr %wf.slot.4, align 1
  %t36 = getelementptr inbounds i8, ptr %wf.slot.4, i64 4
  store float %t33, ptr %t36, align 1
  %t37 = getelementptr inbounds i8, ptr %wf.slot.4, i64 8
  store i1 %t35, ptr %t37, align 1
  ; end layout-bounded transfer from %t30 to %wf.slot.4
  %v14 = call i32 @wf_layout.resolve_lp(ptr %wf.slot.4, i32 %v2)
  %v15 = select i1 true, i32 0, i32 0
  %v16 = call i32 @llvm.smax.i32(i32 %v14, i32 %v15)
  ret i32 %v16
bb3:
  br label %bb1
}

define void @wf_layout.box_of(ptr %wf.result, ptr noalias nonnull captures(none) dereferenceable(584) %v0, ptr %wf.arg.v1) #0 align 64 {
entry:
  %wf.slot.0 = alloca %wf.t.8b1203ba12af44dc, align 4
  %wf.slot.1 = alloca %wf.t.d43c5afff96c5f11, align 4
  %wf.slot.2 = alloca %wf.t.9cbecabb2490f024, align 4
  call void @wf_layout.computed_of(ptr %wf.slot.1, ptr %v0, ptr %wf.arg.v1)
  %t0 = getelementptr inbounds %wf.t.d43c5afff96c5f11, ptr %wf.slot.1, i32 0, i32 0
  %v3 = load i32, ptr %t0
  %v4 = zext i32 %v3 to i64
  %t1 = getelementptr inbounds %wf.t.17d39b4707a3f3ff, ptr %v0, i32 0, i32 1
  %v5 = getelementptr i8, ptr %t1, i64 0
  %t2 = load ptr, ptr %v5, !tbaa !13
  %v6 = getelementptr i8, ptr %t2, i64 0
  %t4 = getelementptr inbounds { i64, i64, [0 x %wf.t.9cbecabb2490f024] }, ptr %v6, i32 0, i32 0
  %t3 = load i64, ptr %t4, !tbaa !15
  %v7 = add i64 %t3, 0
  %v8 = icmp ult i64 %v4, %v7
  switch i1 %v8, label %invalid.tag.b0 [
    i1 1, label %bb2
    i1 0, label %bb3
  ]
invalid.tag.b0:
  unreachable
bb1:
  %t5 = getelementptr inbounds %wf.t.8b1203ba12af44dc, ptr %wf.slot.0, i32 0, i32 2
  %v19 = load i1, ptr %t5
  switch i1 %v19, label %invalid.tag.b1 [
    i1 1, label %bb5
    i1 0, label %bb6
  ]
invalid.tag.b1:
  unreachable
bb2:
  %t6 = getelementptr inbounds %wf.t.17d39b4707a3f3ff, ptr %v0, i32 0, i32 1
  %v14 = getelementptr i8, ptr %t6, i64 0
  %t7 = load ptr, ptr %v14, !tbaa !13
  %v15 = getelementptr i8, ptr %t7, i64 0
  %t8.nonnegative = icmp sge i64 %v4, 0
  call void @llvm.assume(i1 %t8.nonnegative)
  %t8 = getelementptr inbounds { i64, i64, [0 x %wf.t.9cbecabb2490f024] }, ptr %v15, i64 0, i32 2, i64 %v4
  %v16 = getelementptr i8, ptr %t8, i64 0
  ; layout-bounded transfer from %wf.arg.v1 to %wf.slot.0
  %t9 = load i32, ptr %wf.arg.v1, align 1
  %t10 = getelementptr inbounds i8, ptr %wf.arg.v1, i64 4
  %t11 = load i32, ptr %t10, align 1
  %t12 = getelementptr inbounds i8, ptr %wf.arg.v1, i64 8
  %t13 = load i1, ptr %t12, align 1
  store i32 %t9, ptr %wf.slot.0, align 1
  %t14 = getelementptr inbounds i8, ptr %wf.slot.0, i64 4
  store i32 %t11, ptr %t14, align 1
  %t15 = getelementptr inbounds i8, ptr %wf.slot.0, i64 8
  store i1 %t13, ptr %t15, align 1
  ; end layout-bounded transfer from %wf.arg.v1 to %wf.slot.0
  ; layout-bounded transfer from %v16 to %wf.result
  %t16 = load i8, ptr %v16, align 1
  %t17 = getelementptr inbounds i8, ptr %v16, i64 1
  %t18 = load i8, ptr %t17, align 1
  %t19 = getelementptr inbounds i8, ptr %v16, i64 2
  %t20 = load i8, ptr %t19, align 1
  %t21 = getelementptr inbounds i8, ptr %v16, i64 3
  %t22 = load i8, ptr %t21, align 1
  %t23 = getelementptr inbounds i8, ptr %v16, i64 4
  %t24 = load i8, ptr %t23, align 1
  %t25 = getelementptr inbounds i8, ptr %v16, i64 5
  %t26 = load i8, ptr %t25, align 1
  %t27 = getelementptr inbounds i8, ptr %v16, i64 6
  %t28 = load i8, ptr %t27, align 1
  %t29 = getelementptr inbounds i8, ptr %v16, i64 7
  %t30 = load i8, ptr %t29, align 1
  %t31 = getelementptr inbounds i8, ptr %v16, i64 8
  %t32 = load i1, ptr %t31, align 1
  %t33 = getelementptr inbounds i8, ptr %v16, i64 12
  %t34 = load i32, ptr %t33, align 1
  %t35 = getelementptr inbounds i8, ptr %v16, i64 16
  %t36 = load i8, ptr %t35, align 1
  store i8 %t16, ptr %wf.result, align 1
  %t37 = getelementptr inbounds i8, ptr %wf.result, i64 1
  store i8 %t18, ptr %t37, align 1
  %t38 = getelementptr inbounds i8, ptr %wf.result, i64 2
  store i8 %t20, ptr %t38, align 1
  %t39 = getelementptr inbounds i8, ptr %wf.result, i64 3
  store i8 %t22, ptr %t39, align 1
  %t40 = getelementptr inbounds i8, ptr %wf.result, i64 4
  store i8 %t24, ptr %t40, align 1
  %t41 = getelementptr inbounds i8, ptr %wf.result, i64 5
  store i8 %t26, ptr %t41, align 1
  %t42 = getelementptr inbounds i8, ptr %wf.result, i64 6
  store i8 %t28, ptr %t42, align 1
  %t43 = getelementptr inbounds i8, ptr %wf.result, i64 7
  store i8 %t30, ptr %t43, align 1
  %t44 = getelementptr inbounds i8, ptr %wf.result, i64 8
  store i1 %t32, ptr %t44, align 1
  %t45 = getelementptr inbounds i8, ptr %wf.result, i64 12
  store i32 %t34, ptr %t45, align 1
  %t46 = getelementptr inbounds i8, ptr %wf.result, i64 16
  store i8 %t36, ptr %t46, align 1
  ; end layout-bounded transfer from %v16 to %wf.result
  br label %bb1
bb3:
  ; layout-bounded transfer from %wf.arg.v1 to %wf.slot.0
  %t47 = load i32, ptr %wf.arg.v1, align 1
  %t48 = getelementptr inbounds i8, ptr %wf.arg.v1, i64 4
  %t49 = load i32, ptr %t48, align 1
  %t50 = getelementptr inbounds i8, ptr %wf.arg.v1, i64 8
  %t51 = load i1, ptr %t50, align 1
  store i32 %t47, ptr %wf.slot.0, align 1
  %t52 = getelementptr inbounds i8, ptr %wf.slot.0, i64 4
  store i32 %t49, ptr %t52, align 1
  %t53 = getelementptr inbounds i8, ptr %wf.slot.0, i64 8
  store i1 %t51, ptr %t53, align 1
  ; end layout-bounded transfer from %wf.arg.v1 to %wf.slot.0
  call void @wf_layout.initial_box(ptr %wf.result)
  br label %bb1
bb4:
  ret void
bb5:
  call void @wf_layout.initial_box(ptr %wf.slot.2)
  %t54 = getelementptr inbounds %wf.t.9cbecabb2490f024, ptr %wf.slot.2, i32 0, i32 0
  %v26 = load i8, ptr %t54
  %t55 = getelementptr inbounds %wf.t.9cbecabb2490f024, ptr %wf.slot.2, i32 0, i32 1
  %v27 = load i8, ptr %t55
  %t56 = getelementptr inbounds %wf.t.9cbecabb2490f024, ptr %wf.slot.2, i32 0, i32 2
  %v28 = load i8, ptr %t56
  %t57 = getelementptr inbounds %wf.t.9cbecabb2490f024, ptr %wf.slot.2, i32 0, i32 3
  %v29 = load i8, ptr %t57
  %t58 = getelementptr inbounds %wf.t.9cbecabb2490f024, ptr %wf.slot.2, i32 0, i32 4
  %v30 = load i8, ptr %t58
  %t59 = getelementptr inbounds %wf.t.9cbecabb2490f024, ptr %wf.slot.2, i32 0, i32 5
  %v31 = load i8, ptr %t59
  %t60 = getelementptr inbounds %wf.t.9cbecabb2490f024, ptr %wf.slot.2, i32 0, i32 6
  %v32 = load i8, ptr %t60
  %t61 = getelementptr inbounds %wf.t.9cbecabb2490f024, ptr %wf.result, i32 0, i32 7
  %v33 = load i8, ptr %t61
  %t62 = getelementptr inbounds %wf.t.9cbecabb2490f024, ptr %wf.slot.2, i32 0, i32 8
  %v34 = load i1, ptr %t62
  %t63 = getelementptr inbounds %wf.t.9cbecabb2490f024, ptr %wf.slot.2, i32 0, i32 9
  %v35 = load i32, ptr %t63
  %t64 = getelementptr inbounds %wf.t.9cbecabb2490f024, ptr %wf.slot.2, i32 0, i32 10
  %v36 = load i8, ptr %t64
  store %wf.t.9cbecabb2490f024 zeroinitializer, ptr %wf.result
  %t65 = getelementptr inbounds %wf.t.9cbecabb2490f024, ptr %wf.result, i32 0, i32 0
  store i8 %v26, ptr %t65
  %t66 = getelementptr inbounds %wf.t.9cbecabb2490f024, ptr %wf.result, i32 0, i32 1
  store i8 %v27, ptr %t66
  %t67 = getelementptr inbounds %wf.t.9cbecabb2490f024, ptr %wf.result, i32 0, i32 2
  store i8 %v28, ptr %t67
  %t68 = getelementptr inbounds %wf.t.9cbecabb2490f024, ptr %wf.result, i32 0, i32 3
  store i8 %v29, ptr %t68
  %t69 = getelementptr inbounds %wf.t.9cbecabb2490f024, ptr %wf.result, i32 0, i32 4
  store i8 %v30, ptr %t69
  %t70 = getelementptr inbounds %wf.t.9cbecabb2490f024, ptr %wf.result, i32 0, i32 5
  store i8 %v31, ptr %t70
  %t71 = getelementptr inbounds %wf.t.9cbecabb2490f024, ptr %wf.result, i32 0, i32 6
  store i8 %v32, ptr %t71
  %t72 = getelementptr inbounds %wf.t.9cbecabb2490f024, ptr %wf.result, i32 0, i32 7
  store i8 %v33, ptr %t72
  %t73 = getelementptr inbounds %wf.t.9cbecabb2490f024, ptr %wf.result, i32 0, i32 8
  store i1 %v34, ptr %t73
  %t74 = getelementptr inbounds %wf.t.9cbecabb2490f024, ptr %wf.result, i32 0, i32 9
  store i32 %v35, ptr %t74
  %t75 = getelementptr inbounds %wf.t.9cbecabb2490f024, ptr %wf.result, i32 0, i32 10
  store i8 %v36, ptr %t75
  ret void
bb6:
  br label %bb4
}

define void @wf_layout.initial_box(ptr %wf.result) #0 align 64 {
entry:
  %v0 = select i1 true, i1 0, i1 0
  %v1 = select i1 true, i1 1, i1 1
  %v2 = select i1 true, i8 1, i8 1
  %v3 = select i1 true, i8 0, i8 0
  %v4 = select i1 true, i8 0, i8 0
  %v5 = select i1 true, i8 0, i8 0
  %v6 = select i1 true, i8 0, i8 0
  %v7 = select i1 true, i8 0, i8 0
  %v8 = select i1 true, i8 0, i8 0
  %v9 = select i1 true, i8 0, i8 0
  %v10 = select i1 true, i32 0, i32 0
  %v11 = select i1 true, i8 0, i8 0
  store %wf.t.9cbecabb2490f024 zeroinitializer, ptr %wf.result
  %t0 = getelementptr inbounds %wf.t.9cbecabb2490f024, ptr %wf.result, i32 0, i32 0
  store i8 %v2, ptr %t0
  %t1 = getelementptr inbounds %wf.t.9cbecabb2490f024, ptr %wf.result, i32 0, i32 1
  store i8 %v3, ptr %t1
  %t2 = getelementptr inbounds %wf.t.9cbecabb2490f024, ptr %wf.result, i32 0, i32 2
  store i8 %v4, ptr %t2
  %t3 = getelementptr inbounds %wf.t.9cbecabb2490f024, ptr %wf.result, i32 0, i32 3
  store i8 %v5, ptr %t3
  %t4 = getelementptr inbounds %wf.t.9cbecabb2490f024, ptr %wf.result, i32 0, i32 4
  store i8 %v6, ptr %t4
  %t5 = getelementptr inbounds %wf.t.9cbecabb2490f024, ptr %wf.result, i32 0, i32 5
  store i8 %v7, ptr %t5
  %t6 = getelementptr inbounds %wf.t.9cbecabb2490f024, ptr %wf.result, i32 0, i32 6
  store i8 %v8, ptr %t6
  %t7 = getelementptr inbounds %wf.t.9cbecabb2490f024, ptr %wf.result, i32 0, i32 7
  store i8 %v9, ptr %t7
  %t8 = getelementptr inbounds %wf.t.9cbecabb2490f024, ptr %wf.result, i32 0, i32 8
  store i1 %v1, ptr %t8
  %t9 = getelementptr inbounds %wf.t.9cbecabb2490f024, ptr %wf.result, i32 0, i32 9
  store i32 %v10, ptr %t9
  %t10 = getelementptr inbounds %wf.t.9cbecabb2490f024, ptr %wf.result, i32 0, i32 10
  store i8 %v11, ptr %t10
  ret void
}

define i1 @wf_layout.positioned(ptr noalias nonnull captures(none) dereferenceable(584) %v0, ptr noalias nonnull captures(none) dereferenceable(360) %v1) #0 align 64 {
entry:
  %wf.slot.0 = alloca %wf.t.9cbecabb2490f024, align 4
  %t0 = getelementptr inbounds %wf.t.66700addc1283274, ptr %v1, i32 0, i32 2
  %v2 = getelementptr i8, ptr %t0, i64 0
  call void @wf_layout.box_of(ptr %wf.slot.0, ptr %v0, ptr %v2)
  %t1 = getelementptr inbounds %wf.t.9cbecabb2490f024, ptr %wf.slot.0, i32 0, i32 1
  %v5 = load i8, ptr %t1
  %v6 = select i1 true, i8 2, i8 2
  %v7 = icmp eq i8 %v5, %v6
  %t2 = getelementptr inbounds %wf.t.9cbecabb2490f024, ptr %wf.slot.0, i32 0, i32 1
  %v8 = load i8, ptr %t2
  %v9 = select i1 true, i8 3, i8 3
  %v10 = icmp eq i8 %v8, %v9
  %v11 = or i1 %v7, %v10
  ret i1 %v11
}

define void @wf_layout.text_of(ptr %wf.result, ptr noalias nonnull captures(none) dereferenceable(584) %v0, ptr %wf.arg.v1) #0 align 64 {
entry:
  %wf.slot.0 = alloca %wf.t.8b1203ba12af44dc, align 4
  %wf.slot.1 = alloca %wf.t.d43c5afff96c5f11, align 4
  %wf.slot.2 = alloca %wf.t.e582deac3cb0a0f6, align 4
  %wf.slot.3 = alloca %wf.t.5ae2e280d1eb25be, align 4
  %wf.slot.4 = alloca %wf.t.518b4b787557360a, align 4
  %wf.slot.5 = alloca %wf.t.e582deac3cb0a0f6, align 4
  %wf.slot.6 = alloca %wf.t.5ae2e280d1eb25be, align 4
  %wf.slot.7 = alloca %wf.t.5ae2e280d1eb25be, align 4
  %wf.slot.8 = alloca %wf.t.1f2ca470e213f17e, align 4
  call void @wf_layout.computed_of(ptr %wf.slot.1, ptr %v0, ptr %wf.arg.v1)
  %t0 = getelementptr inbounds %wf.t.d43c5afff96c5f11, ptr %wf.slot.1, i32 0, i32 5
  %v3 = load i32, ptr %t0
  %v4 = zext i32 %v3 to i64
  %t1 = getelementptr inbounds %wf.t.17d39b4707a3f3ff, ptr %v0, i32 0, i32 6
  %v5 = getelementptr i8, ptr %t1, i64 0
  %t2 = load ptr, ptr %v5, !tbaa !13
  %v6 = getelementptr i8, ptr %t2, i64 0
  %t4 = getelementptr inbounds { i64, i64, [0 x %wf.t.07a97e4ae2e40a7a] }, ptr %v6, i32 0, i32 0
  %t3 = load i64, ptr %t4, !tbaa !15
  %v7 = add i64 %t3, 0
  %v8 = icmp ult i64 %v4, %v7
  switch i1 %v8, label %invalid.tag.b0 [
    i1 1, label %bb2
    i1 0, label %bb3
  ]
invalid.tag.b0:
  unreachable
bb1:
  %t5 = getelementptr inbounds %wf.t.8b1203ba12af44dc, ptr %wf.slot.0, i32 0, i32 2
  %v19 = load i1, ptr %t5
  switch i1 %v19, label %invalid.tag.b1 [
    i1 1, label %bb5
    i1 0, label %bb6
  ]
invalid.tag.b1:
  unreachable
bb2:
  %t6 = getelementptr inbounds %wf.t.17d39b4707a3f3ff, ptr %v0, i32 0, i32 6
  %v14 = getelementptr i8, ptr %t6, i64 0
  %t7 = load ptr, ptr %v14, !tbaa !13
  %v15 = getelementptr i8, ptr %t7, i64 0
  %t8.nonnegative = icmp sge i64 %v4, 0
  call void @llvm.assume(i1 %t8.nonnegative)
  %t8 = getelementptr inbounds { i64, i64, [0 x %wf.t.07a97e4ae2e40a7a] }, ptr %v15, i64 0, i32 2, i64 %v4
  %v16 = getelementptr i8, ptr %t8, i64 0
  ; layout-bounded transfer from %wf.arg.v1 to %wf.slot.0
  %t9 = load i32, ptr %wf.arg.v1, align 1
  %t10 = getelementptr inbounds i8, ptr %wf.arg.v1, i64 4
  %t11 = load i32, ptr %t10, align 1
  %t12 = getelementptr inbounds i8, ptr %wf.arg.v1, i64 8
  %t13 = load i1, ptr %t12, align 1
  store i32 %t9, ptr %wf.slot.0, align 1
  %t14 = getelementptr inbounds i8, ptr %wf.slot.0, i64 4
  store i32 %t11, ptr %t14, align 1
  %t15 = getelementptr inbounds i8, ptr %wf.slot.0, i64 8
  store i1 %t13, ptr %t15, align 1
  ; end layout-bounded transfer from %wf.arg.v1 to %wf.slot.0
  call void @llvm.memmove.p0.p0.i64(ptr %wf.result, ptr %v16, i64 ptrtoint (ptr getelementptr (%wf.t.07a97e4ae2e40a7a, ptr null, i32 1) to i64), i1 false)
  br label %bb1
bb3:
  ; layout-bounded transfer from %wf.arg.v1 to %wf.slot.0
  %t16 = load i32, ptr %wf.arg.v1, align 1
  %t17 = getelementptr inbounds i8, ptr %wf.arg.v1, i64 4
  %t18 = load i32, ptr %t17, align 1
  %t19 = getelementptr inbounds i8, ptr %wf.arg.v1, i64 8
  %t20 = load i1, ptr %t19, align 1
  store i32 %t16, ptr %wf.slot.0, align 1
  %t21 = getelementptr inbounds i8, ptr %wf.slot.0, i64 4
  store i32 %t18, ptr %t21, align 1
  %t22 = getelementptr inbounds i8, ptr %wf.slot.0, i64 8
  store i1 %t20, ptr %t22, align 1
  ; end layout-bounded transfer from %wf.arg.v1 to %wf.slot.0
  call void @wf_layout.initial_text(ptr %wf.result)
  br label %bb1
bb4:
  ret void
bb5:
  %v25 = call %wf.t.e582deac3cb0a0f6 @wf_layout.zero_lp()
  store %wf.t.e582deac3cb0a0f6 %v25, ptr %wf.slot.2
  %v26 = select i1 true, i8 4, i8 4
  %v27 = select i1 true, i8 0, i8 0
  store %wf.t.5ae2e280d1eb25be zeroinitializer, ptr %wf.slot.3
  %t23 = getelementptr inbounds %wf.t.5ae2e280d1eb25be, ptr %wf.slot.3, i32 0, i32 0
  store i8 %v26, ptr %t23
  %t24 = getelementptr inbounds %wf.t.5ae2e280d1eb25be, ptr %wf.slot.3, i32 0, i32 1
  store i8 %v27, ptr %t24
  %t25 = getelementptr inbounds %wf.t.5ae2e280d1eb25be, ptr %wf.slot.3, i32 0, i32 2
  ; layout-bounded transfer from %wf.slot.2 to %t25
  %t26 = load float, ptr %wf.slot.2, align 1
  %t27 = getelementptr inbounds i8, ptr %wf.slot.2, i64 4
  %t28 = load float, ptr %t27, align 1
  %t29 = getelementptr inbounds i8, ptr %wf.slot.2, i64 8
  %t30 = load i1, ptr %t29, align 1
  store float %t26, ptr %t25, align 1
  %t31 = getelementptr inbounds i8, ptr %t25, i64 4
  store float %t28, ptr %t31, align 1
  %t32 = getelementptr inbounds i8, ptr %t25, i64 8
  store i1 %t30, ptr %t32, align 1
  ; end layout-bounded transfer from %wf.slot.2 to %t25
  %t33 = getelementptr inbounds %wf.t.07a97e4ae2e40a7a, ptr %wf.result, i32 0, i32 0
  ; layout-bounded transfer from %t33 to %wf.slot.4
  %t34 = load i8, ptr %t33, align 1
  %t35 = getelementptr inbounds i8, ptr %t33, i64 4
  %t36 = load float, ptr %t35, align 1
  %t37 = getelementptr inbounds i8, ptr %t33, i64 8
  %t38 = load float, ptr %t37, align 1
  %t39 = getelementptr inbounds i8, ptr %t33, i64 12
  %t40 = load float, ptr %t39, align 1
  %t41 = getelementptr inbounds i8, ptr %t33, i64 16
  %t42 = load float, ptr %t41, align 1
  %t43 = getelementptr inbounds i8, ptr %t33, i64 20
  %t44 = load i8, ptr %t43, align 1
  store i8 %t34, ptr %wf.slot.4, align 1
  %t45 = getelementptr inbounds i8, ptr %wf.slot.4, i64 4
  store float %t36, ptr %t45, align 1
  %t46 = getelementptr inbounds i8, ptr %wf.slot.4, i64 8
  store float %t38, ptr %t46, align 1
  %t47 = getelementptr inbounds i8, ptr %wf.slot.4, i64 12
  store float %t40, ptr %t47, align 1
  %t48 = getelementptr inbounds i8, ptr %wf.slot.4, i64 16
  store float %t42, ptr %t48, align 1
  %t49 = getelementptr inbounds i8, ptr %wf.slot.4, i64 20
  store i8 %t44, ptr %t49, align 1
  ; end layout-bounded transfer from %t33 to %wf.slot.4
  %t50 = getelementptr inbounds %wf.t.07a97e4ae2e40a7a, ptr %wf.result, i32 0, i32 1
  %v30 = load i8, ptr %t50
  %t51 = getelementptr inbounds %wf.t.07a97e4ae2e40a7a, ptr %wf.result, i32 0, i32 2
  ; layout-bounded transfer from %t51 to %wf.slot.5
  %t52 = load float, ptr %t51, align 1
  %t53 = getelementptr inbounds i8, ptr %t51, i64 4
  %t54 = load float, ptr %t53, align 1
  %t55 = getelementptr inbounds i8, ptr %t51, i64 8
  %t56 = load i1, ptr %t55, align 1
  store float %t52, ptr %wf.slot.5, align 1
  %t57 = getelementptr inbounds i8, ptr %wf.slot.5, i64 4
  store float %t54, ptr %t57, align 1
  %t58 = getelementptr inbounds i8, ptr %wf.slot.5, i64 8
  store i1 %t56, ptr %t58, align 1
  ; end layout-bounded transfer from %t51 to %wf.slot.5
  %t59 = getelementptr inbounds %wf.t.07a97e4ae2e40a7a, ptr %wf.result, i32 0, i32 3
  %v32 = load i8, ptr %t59
  %t60 = getelementptr inbounds %wf.t.07a97e4ae2e40a7a, ptr %wf.result, i32 0, i32 4
  %v33 = load i8, ptr %t60
  %t61 = getelementptr inbounds %wf.t.07a97e4ae2e40a7a, ptr %wf.result, i32 0, i32 5
  ; layout-bounded transfer from %t61 to %wf.slot.6
  %t62 = load i8, ptr %t61, align 1
  %t63 = getelementptr inbounds i8, ptr %t61, i64 1
  %t64 = load i8, ptr %t63, align 1
  %t65 = getelementptr inbounds i8, ptr %t61, i64 4
  %t66 = load float, ptr %t65, align 1
  %t67 = getelementptr inbounds i8, ptr %t61, i64 8
  %t68 = load float, ptr %t67, align 1
  %t69 = getelementptr inbounds i8, ptr %t61, i64 12
  %t70 = load i1, ptr %t69, align 1
  store i8 %t62, ptr %wf.slot.6, align 1
  %t71 = getelementptr inbounds i8, ptr %wf.slot.6, i64 1
  store i8 %t64, ptr %t71, align 1
  %t72 = getelementptr inbounds i8, ptr %wf.slot.6, i64 4
  store float %t66, ptr %t72, align 1
  %t73 = getelementptr inbounds i8, ptr %wf.slot.6, i64 8
  store float %t68, ptr %t73, align 1
  %t74 = getelementptr inbounds i8, ptr %wf.slot.6, i64 12
  store i1 %t70, ptr %t74, align 1
  ; end layout-bounded transfer from %t61 to %wf.slot.6
  %t75 = getelementptr inbounds %wf.t.07a97e4ae2e40a7a, ptr %wf.result, i32 0, i32 6
  ; layout-bounded transfer from %t75 to %wf.slot.7
  %t76 = load i8, ptr %t75, align 1
  %t77 = getelementptr inbounds i8, ptr %t75, i64 1
  %t78 = load i8, ptr %t77, align 1
  %t79 = getelementptr inbounds i8, ptr %t75, i64 4
  %t80 = load float, ptr %t79, align 1
  %t81 = getelementptr inbounds i8, ptr %t75, i64 8
  %t82 = load float, ptr %t81, align 1
  %t83 = getelementptr inbounds i8, ptr %t75, i64 12
  %t84 = load i1, ptr %t83, align 1
  store i8 %t76, ptr %wf.slot.7, align 1
  %t85 = getelementptr inbounds i8, ptr %wf.slot.7, i64 1
  store i8 %t78, ptr %t85, align 1
  %t86 = getelementptr inbounds i8, ptr %wf.slot.7, i64 4
  store float %t80, ptr %t86, align 1
  %t87 = getelementptr inbounds i8, ptr %wf.slot.7, i64 8
  store float %t82, ptr %t87, align 1
  %t88 = getelementptr inbounds i8, ptr %wf.slot.7, i64 12
  store i1 %t84, ptr %t88, align 1
  ; end layout-bounded transfer from %t75 to %wf.slot.7
  %v36 = select i1 true, i8 0, i8 0
  %t89 = getelementptr inbounds %wf.t.07a97e4ae2e40a7a, ptr %wf.result, i32 0, i32 9
  ; layout-bounded transfer from %t89 to %wf.slot.8
  %t90 = load i32, ptr %t89, align 1
  store i32 %t90, ptr %wf.slot.8, align 1
  ; end layout-bounded transfer from %t89 to %wf.slot.8
  %t91 = getelementptr inbounds %wf.t.07a97e4ae2e40a7a, ptr %wf.result, i32 0, i32 10
  %v38 = load i8, ptr %t91
  %t92 = getelementptr inbounds %wf.t.07a97e4ae2e40a7a, ptr %wf.result, i32 0, i32 11
  %v39 = load i8, ptr %t92
  store %wf.t.07a97e4ae2e40a7a zeroinitializer, ptr %wf.result
  %t93 = getelementptr inbounds %wf.t.07a97e4ae2e40a7a, ptr %wf.result, i32 0, i32 0
  ; layout-bounded transfer from %wf.slot.4 to %t93
  %t94 = load i8, ptr %wf.slot.4, align 1
  %t95 = getelementptr inbounds i8, ptr %wf.slot.4, i64 4
  %t96 = load float, ptr %t95, align 1
  %t97 = getelementptr inbounds i8, ptr %wf.slot.4, i64 8
  %t98 = load float, ptr %t97, align 1
  %t99 = getelementptr inbounds i8, ptr %wf.slot.4, i64 12
  %t100 = load float, ptr %t99, align 1
  %t101 = getelementptr inbounds i8, ptr %wf.slot.4, i64 16
  %t102 = load float, ptr %t101, align 1
  %t103 = getelementptr inbounds i8, ptr %wf.slot.4, i64 20
  %t104 = load i8, ptr %t103, align 1
  store i8 %t94, ptr %t93, align 1
  %t105 = getelementptr inbounds i8, ptr %t93, i64 4
  store float %t96, ptr %t105, align 1
  %t106 = getelementptr inbounds i8, ptr %t93, i64 8
  store float %t98, ptr %t106, align 1
  %t107 = getelementptr inbounds i8, ptr %t93, i64 12
  store float %t100, ptr %t107, align 1
  %t108 = getelementptr inbounds i8, ptr %t93, i64 16
  store float %t102, ptr %t108, align 1
  %t109 = getelementptr inbounds i8, ptr %t93, i64 20
  store i8 %t104, ptr %t109, align 1
  ; end layout-bounded transfer from %wf.slot.4 to %t93
  %t110 = getelementptr inbounds %wf.t.07a97e4ae2e40a7a, ptr %wf.result, i32 0, i32 1
  store i8 %v30, ptr %t110
  %t111 = getelementptr inbounds %wf.t.07a97e4ae2e40a7a, ptr %wf.result, i32 0, i32 2
  ; layout-bounded transfer from %wf.slot.5 to %t111
  %t112 = load float, ptr %wf.slot.5, align 1
  %t113 = getelementptr inbounds i8, ptr %wf.slot.5, i64 4
  %t114 = load float, ptr %t113, align 1
  %t115 = getelementptr inbounds i8, ptr %wf.slot.5, i64 8
  %t116 = load i1, ptr %t115, align 1
  store float %t112, ptr %t111, align 1
  %t117 = getelementptr inbounds i8, ptr %t111, i64 4
  store float %t114, ptr %t117, align 1
  %t118 = getelementptr inbounds i8, ptr %t111, i64 8
  store i1 %t116, ptr %t118, align 1
  ; end layout-bounded transfer from %wf.slot.5 to %t111
  %t119 = getelementptr inbounds %wf.t.07a97e4ae2e40a7a, ptr %wf.result, i32 0, i32 3
  store i8 %v32, ptr %t119
  %t120 = getelementptr inbounds %wf.t.07a97e4ae2e40a7a, ptr %wf.result, i32 0, i32 4
  store i8 %v33, ptr %t120
  %t121 = getelementptr inbounds %wf.t.07a97e4ae2e40a7a, ptr %wf.result, i32 0, i32 5
  ; layout-bounded transfer from %wf.slot.6 to %t121
  %t122 = load i8, ptr %wf.slot.6, align 1
  %t123 = getelementptr inbounds i8, ptr %wf.slot.6, i64 1
  %t124 = load i8, ptr %t123, align 1
  %t125 = getelementptr inbounds i8, ptr %wf.slot.6, i64 4
  %t126 = load float, ptr %t125, align 1
  %t127 = getelementptr inbounds i8, ptr %wf.slot.6, i64 8
  %t128 = load float, ptr %t127, align 1
  %t129 = getelementptr inbounds i8, ptr %wf.slot.6, i64 12
  %t130 = load i1, ptr %t129, align 1
  store i8 %t122, ptr %t121, align 1
  %t131 = getelementptr inbounds i8, ptr %t121, i64 1
  store i8 %t124, ptr %t131, align 1
  %t132 = getelementptr inbounds i8, ptr %t121, i64 4
  store float %t126, ptr %t132, align 1
  %t133 = getelementptr inbounds i8, ptr %t121, i64 8
  store float %t128, ptr %t133, align 1
  %t134 = getelementptr inbounds i8, ptr %t121, i64 12
  store i1 %t130, ptr %t134, align 1
  ; end layout-bounded transfer from %wf.slot.6 to %t121
  %t135 = getelementptr inbounds %wf.t.07a97e4ae2e40a7a, ptr %wf.result, i32 0, i32 6
  ; layout-bounded transfer from %wf.slot.7 to %t135
  %t136 = load i8, ptr %wf.slot.7, align 1
  %t137 = getelementptr inbounds i8, ptr %wf.slot.7, i64 1
  %t138 = load i8, ptr %t137, align 1
  %t139 = getelementptr inbounds i8, ptr %wf.slot.7, i64 4
  %t140 = load float, ptr %t139, align 1
  %t141 = getelementptr inbounds i8, ptr %wf.slot.7, i64 8
  %t142 = load float, ptr %t141, align 1
  %t143 = getelementptr inbounds i8, ptr %wf.slot.7, i64 12
  %t144 = load i1, ptr %t143, align 1
  store i8 %t136, ptr %t135, align 1
  %t145 = getelementptr inbounds i8, ptr %t135, i64 1
  store i8 %t138, ptr %t145, align 1
  %t146 = getelementptr inbounds i8, ptr %t135, i64 4
  store float %t140, ptr %t146, align 1
  %t147 = getelementptr inbounds i8, ptr %t135, i64 8
  store float %t142, ptr %t147, align 1
  %t148 = getelementptr inbounds i8, ptr %t135, i64 12
  store i1 %t144, ptr %t148, align 1
  ; end layout-bounded transfer from %wf.slot.7 to %t135
  %t149 = getelementptr inbounds %wf.t.07a97e4ae2e40a7a, ptr %wf.result, i32 0, i32 7
  ; layout-bounded transfer from %wf.slot.3 to %t149
  %t150 = load i8, ptr %wf.slot.3, align 1
  %t151 = getelementptr inbounds i8, ptr %wf.slot.3, i64 1
  %t152 = load i8, ptr %t151, align 1
  %t153 = getelementptr inbounds i8, ptr %wf.slot.3, i64 4
  %t154 = load float, ptr %t153, align 1
  %t155 = getelementptr inbounds i8, ptr %wf.slot.3, i64 8
  %t156 = load float, ptr %t155, align 1
  %t157 = getelementptr inbounds i8, ptr %wf.slot.3, i64 12
  %t158 = load i1, ptr %t157, align 1
  store i8 %t150, ptr %t149, align 1
  %t159 = getelementptr inbounds i8, ptr %t149, i64 1
  store i8 %t152, ptr %t159, align 1
  %t160 = getelementptr inbounds i8, ptr %t149, i64 4
  store float %t154, ptr %t160, align 1
  %t161 = getelementptr inbounds i8, ptr %t149, i64 8
  store float %t156, ptr %t161, align 1
  %t162 = getelementptr inbounds i8, ptr %t149, i64 12
  store i1 %t158, ptr %t162, align 1
  ; end layout-bounded transfer from %wf.slot.3 to %t149
  %t163 = getelementptr inbounds %wf.t.07a97e4ae2e40a7a, ptr %wf.result, i32 0, i32 8
  store i8 %v36, ptr %t163
  %t164 = getelementptr inbounds %wf.t.07a97e4ae2e40a7a, ptr %wf.result, i32 0, i32 9
  ; layout-bounded transfer from %wf.slot.8 to %t164
  %t165 = load i32, ptr %wf.slot.8, align 1
  store i32 %t165, ptr %t164, align 1
  ; end layout-bounded transfer from %wf.slot.8 to %t164
  %t166 = getelementptr inbounds %wf.t.07a97e4ae2e40a7a, ptr %wf.result, i32 0, i32 10
  store i8 %v38, ptr %t166
  %t167 = getelementptr inbounds %wf.t.07a97e4ae2e40a7a, ptr %wf.result, i32 0, i32 11
  store i8 %v39, ptr %t167
  ret void
bb6:
  br label %bb4
}

define i8 @wf_layout.valign_edge(ptr noalias nonnull captures(none) dereferenceable(584) %v0, ptr %wf.arg.v1) #0 align 64 {
entry:
  %wf.slot.0 = alloca %wf.t.8b1203ba12af44dc, align 4
  %wf.slot.1 = alloca %wf.t.07a97e4ae2e40a7a, align 4
  %wf.slot.2 = alloca %wf.t.5ae2e280d1eb25be, align 4
  call void @wf_layout.text_of(ptr %wf.slot.1, ptr %v0, ptr %wf.arg.v1)
  %t0 = getelementptr inbounds %wf.t.07a97e4ae2e40a7a, ptr %wf.slot.1, i32 0, i32 7
  ; layout-bounded transfer from %t0 to %wf.slot.2
  %t1 = load i8, ptr %t0, align 1
  %t2 = getelementptr inbounds i8, ptr %t0, i64 1
  %t3 = load i8, ptr %t2, align 1
  %t4 = getelementptr inbounds i8, ptr %t0, i64 4
  %t5 = load float, ptr %t4, align 1
  %t6 = getelementptr inbounds i8, ptr %t0, i64 8
  %t7 = load float, ptr %t6, align 1
  %t8 = getelementptr inbounds i8, ptr %t0, i64 12
  %t9 = load i1, ptr %t8, align 1
  store i8 %t1, ptr %wf.slot.2, align 1
  %t10 = getelementptr inbounds i8, ptr %wf.slot.2, i64 1
  store i8 %t3, ptr %t10, align 1
  %t11 = getelementptr inbounds i8, ptr %wf.slot.2, i64 4
  store float %t5, ptr %t11, align 1
  %t12 = getelementptr inbounds i8, ptr %wf.slot.2, i64 8
  store float %t7, ptr %t12, align 1
  %t13 = getelementptr inbounds i8, ptr %wf.slot.2, i64 12
  store i1 %t9, ptr %t13, align 1
  ; end layout-bounded transfer from %t0 to %wf.slot.2
  %t14 = getelementptr inbounds %wf.t.5ae2e280d1eb25be, ptr %wf.slot.2, i32 0, i32 0
  %v4 = load i8, ptr %t14
  %v5 = select i1 true, i8 0, i8 0
  %v6 = icmp eq i8 %v4, %v5
  switch i1 %v6, label %invalid.tag.b0 [
    i1 1, label %bb2
    i1 0, label %bb3
  ]
invalid.tag.b0:
  unreachable
bb1:
  %t15 = getelementptr inbounds %wf.t.5ae2e280d1eb25be, ptr %wf.slot.2, i32 0, i32 1
  %v12 = load i8, ptr %t15
  %v13 = select i1 true, i8 6, i8 6
  %v14 = icmp eq i8 %v12, %v13
  switch i1 %v14, label %invalid.tag.b1 [
    i1 1, label %bb5
    i1 0, label %bb6
  ]
invalid.tag.b1:
  unreachable
bb2:
  %v11 = select i1 true, i8 0, i8 0
  ret i8 %v11
bb3:
  br label %bb1
bb4:
  %t16 = getelementptr inbounds %wf.t.5ae2e280d1eb25be, ptr %wf.slot.2, i32 0, i32 1
  %v20 = load i8, ptr %t16
  %v21 = select i1 true, i8 7, i8 7
  %v22 = icmp eq i8 %v20, %v21
  switch i1 %v22, label %invalid.tag.b4 [
    i1 1, label %bb8
    i1 0, label %bb9
  ]
invalid.tag.b4:
  unreachable
bb5:
  %v19 = select i1 true, i8 1, i8 1
  ret i8 %v19
bb6:
  br label %bb4
bb7:
  %v28 = select i1 true, i8 0, i8 0
  ret i8 %v28
bb8:
  %v27 = select i1 true, i8 2, i8 2
  ret i8 %v27
bb9:
  br label %bb7
}

declare void @wf_layout.container_of(ptr, ptr noalias nonnull captures(none) dereferenceable(584), ptr)
declare void @wf_layout.initial_text(ptr)
declare i32 @wf_layout.resolve_lp(ptr, i32)
declare i32 @wf_layout.units(float)
declare %wf.t.e582deac3cb0a0f6 @wf_layout.zero_lp()
declare void @llvm.assume(i1)
attributes #0 = { "probe-stack"="inline-asm" }
