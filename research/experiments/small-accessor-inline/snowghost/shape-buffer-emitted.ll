; Extracted from Snowghost-wf c8f5e17 layout_oracle IR (release wf-26355924f942,
; run 38198957372); functions: wf_font.blank_item wf_font.read_item wf_font.write_item wf_font.fail wf_font.put_item wf_font.put_out wf_font.next_glyphs wf_font.next_glyph wf_font.item_at wf_font.current wf_font.out_at wf_font.make_room wf_font.shift_forward wf_font.apply_forward wf_font.sync
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
!92 = distinct !{!92}
!93 = distinct !{!93, !92}
!94 = !{!93}
!95 = distinct !{!95, !92}
!96 = !{!95}

%wf.t.5a204b97e8d207c5 = type { i32, i32 }
%wf.t.5ab0411b6d87ba03 = type { ptr, ptr, i64, i64, i64, i1, i1, i64, i64, i1, i8, i1 }
%wf.t.8b0e525ef4799b77 = type { i32, i32, i32, i8, i16, i16, i8, i32, i32, i32, i32, i32, i8 }
%wf.t.8d63ee4ca2b3d029 = type { i1, i8, i32, i1, i1, i32, i64, i64 }
%wf.t.c398fc66176a258b = type { i16, i16, i16, %wf.t.5a204b97e8d207c5 }

define void @wf_font.blank_item(ptr %wf.result) #0 align 64 {
entry:
  %v0 = select i1 true, i32 0, i32 0
  %v1 = select i1 true, i32 0, i32 0
  %v2 = select i1 true, i32 0, i32 0
  %v3 = select i1 true, i8 0, i8 0
  %v4 = select i1 true, i16 0, i16 0
  %v5 = select i1 true, i16 0, i16 0
  %v6 = select i1 true, i8 0, i8 0
  %v7 = select i1 true, i32 0, i32 0
  %v8 = select i1 true, i32 0, i32 0
  %v9 = select i1 true, i32 0, i32 0
  %v10 = select i1 true, i32 0, i32 0
  %v11 = select i1 true, i32 0, i32 0
  %v12 = select i1 true, i8 0, i8 0
  store %wf.t.8b0e525ef4799b77 zeroinitializer, ptr %wf.result
  %t0 = getelementptr inbounds %wf.t.8b0e525ef4799b77, ptr %wf.result, i32 0, i32 0
  store i32 %v0, ptr %t0
  %t1 = getelementptr inbounds %wf.t.8b0e525ef4799b77, ptr %wf.result, i32 0, i32 1
  store i32 %v1, ptr %t1
  %t2 = getelementptr inbounds %wf.t.8b0e525ef4799b77, ptr %wf.result, i32 0, i32 2
  store i32 %v2, ptr %t2
  %t3 = getelementptr inbounds %wf.t.8b0e525ef4799b77, ptr %wf.result, i32 0, i32 3
  store i8 %v3, ptr %t3
  %t4 = getelementptr inbounds %wf.t.8b0e525ef4799b77, ptr %wf.result, i32 0, i32 4
  store i16 %v4, ptr %t4
  %t5 = getelementptr inbounds %wf.t.8b0e525ef4799b77, ptr %wf.result, i32 0, i32 5
  store i16 %v5, ptr %t5
  %t6 = getelementptr inbounds %wf.t.8b0e525ef4799b77, ptr %wf.result, i32 0, i32 6
  store i8 %v6, ptr %t6
  %t7 = getelementptr inbounds %wf.t.8b0e525ef4799b77, ptr %wf.result, i32 0, i32 7
  store i32 %v7, ptr %t7
  %t8 = getelementptr inbounds %wf.t.8b0e525ef4799b77, ptr %wf.result, i32 0, i32 8
  store i32 %v8, ptr %t8
  %t9 = getelementptr inbounds %wf.t.8b0e525ef4799b77, ptr %wf.result, i32 0, i32 9
  store i32 %v9, ptr %t9
  %t10 = getelementptr inbounds %wf.t.8b0e525ef4799b77, ptr %wf.result, i32 0, i32 10
  store i32 %v10, ptr %t10
  %t11 = getelementptr inbounds %wf.t.8b0e525ef4799b77, ptr %wf.result, i32 0, i32 11
  store i32 %v11, ptr %t11
  %t12 = getelementptr inbounds %wf.t.8b0e525ef4799b77, ptr %wf.result, i32 0, i32 12
  store i8 %v12, ptr %t12
  ret void
}

define void @wf_font.read_item(ptr %wf.result, ptr noalias nonnull captures(none) dereferenceable(8) %v0, i64 %v1) #0 align 64 {
entry:
  %t0 = load ptr, ptr %v0, !tbaa !13
  %v2 = getelementptr i8, ptr %t0, i64 0
  %t2 = getelementptr inbounds { i64, i64, [0 x %wf.t.8b0e525ef4799b77] }, ptr %v2, i32 0, i32 0
  %t1 = load i64, ptr %t2, !tbaa !15
  %v3 = add i64 %t1, 0
  %v4 = icmp ult i64 %v1, %v3
  switch i1 %v4, label %invalid.tag.b0 [
    i1 1, label %bb2
    i1 0, label %bb3
  ]
invalid.tag.b0:
  unreachable
bb1:
  call void @wf_font.blank_item(ptr %wf.result)
  ret void
bb2:
  %t3 = load ptr, ptr %v0, !tbaa !13
  %v7 = getelementptr i8, ptr %t3, i64 0
  %t4.nonnegative = icmp sge i64 %v1, 0
  call void @llvm.assume(i1 %t4.nonnegative)
  %t4 = getelementptr inbounds { i64, i64, [0 x %wf.t.8b0e525ef4799b77] }, ptr %v7, i64 0, i32 2, i64 %v1
  %v8 = getelementptr i8, ptr %t4, i64 0
  ; layout-bounded transfer from %v8 to %wf.result
  %t5 = load i32, ptr %v8, align 1
  %t6 = getelementptr inbounds i8, ptr %v8, i64 4
  %t7 = load i32, ptr %t6, align 1
  %t8 = getelementptr inbounds i8, ptr %v8, i64 8
  %t9 = load i32, ptr %t8, align 1
  %t10 = getelementptr inbounds i8, ptr %v8, i64 12
  %t11 = load i8, ptr %t10, align 1
  %t12 = getelementptr inbounds i8, ptr %v8, i64 14
  %t13 = load i16, ptr %t12, align 1
  %t14 = getelementptr inbounds i8, ptr %v8, i64 16
  %t15 = load i16, ptr %t14, align 1
  %t16 = getelementptr inbounds i8, ptr %v8, i64 18
  %t17 = load i8, ptr %t16, align 1
  %t18 = getelementptr inbounds i8, ptr %v8, i64 20
  %t19 = load i32, ptr %t18, align 1
  %t20 = getelementptr inbounds i8, ptr %v8, i64 24
  %t21 = load i32, ptr %t20, align 1
  %t22 = getelementptr inbounds i8, ptr %v8, i64 28
  %t23 = load i32, ptr %t22, align 1
  %t24 = getelementptr inbounds i8, ptr %v8, i64 32
  %t25 = load i32, ptr %t24, align 1
  %t26 = getelementptr inbounds i8, ptr %v8, i64 36
  %t27 = load i32, ptr %t26, align 1
  %t28 = getelementptr inbounds i8, ptr %v8, i64 40
  %t29 = load i8, ptr %t28, align 1
  store i32 %t5, ptr %wf.result, align 1
  %t30 = getelementptr inbounds i8, ptr %wf.result, i64 4
  store i32 %t7, ptr %t30, align 1
  %t31 = getelementptr inbounds i8, ptr %wf.result, i64 8
  store i32 %t9, ptr %t31, align 1
  %t32 = getelementptr inbounds i8, ptr %wf.result, i64 12
  store i8 %t11, ptr %t32, align 1
  %t33 = getelementptr inbounds i8, ptr %wf.result, i64 14
  store i16 %t13, ptr %t33, align 1
  %t34 = getelementptr inbounds i8, ptr %wf.result, i64 16
  store i16 %t15, ptr %t34, align 1
  %t35 = getelementptr inbounds i8, ptr %wf.result, i64 18
  store i8 %t17, ptr %t35, align 1
  %t36 = getelementptr inbounds i8, ptr %wf.result, i64 20
  store i32 %t19, ptr %t36, align 1
  %t37 = getelementptr inbounds i8, ptr %wf.result, i64 24
  store i32 %t21, ptr %t37, align 1
  %t38 = getelementptr inbounds i8, ptr %wf.result, i64 28
  store i32 %t23, ptr %t38, align 1
  %t39 = getelementptr inbounds i8, ptr %wf.result, i64 32
  store i32 %t25, ptr %t39, align 1
  %t40 = getelementptr inbounds i8, ptr %wf.result, i64 36
  store i32 %t27, ptr %t40, align 1
  %t41 = getelementptr inbounds i8, ptr %wf.result, i64 40
  store i8 %t29, ptr %t41, align 1
  ; end layout-bounded transfer from %v8 to %wf.result
  ret void
bb3:
  br label %bb1
}

define i1 @wf_font.write_item(ptr noalias nonnull captures(none) dereferenceable(8) %v0, i64 %v1, ptr %wf.arg.v2) #0 align 64 {
entry:
  %wf.slot.0 = alloca %wf.t.8b0e525ef4799b77, align 4
  %t0 = load ptr, ptr %v0, !tbaa !13
  %v3 = getelementptr i8, ptr %t0, i64 0
  %t2 = getelementptr inbounds { i64, i64, [0 x %wf.t.8b0e525ef4799b77] }, ptr %v3, i32 0, i32 0
  %t1 = load i64, ptr %t2, !tbaa !15
  %v4 = add i64 %t1, 0
  %v5 = icmp ult i64 %v1, %v4
  switch i1 %v5, label %invalid.tag.b0 [
    i1 1, label %bb2
    i1 0, label %bb3
  ]
invalid.tag.b0:
  unreachable
bb1:
  %v6 = phi ptr [ %v0, %bb3 ]
  %v7 = phi i64 [ %v1, %bb3 ]
  %v9 = phi i64 [ %v4, %bb3 ]
  %v13 = icmp ne i64 %v7, %v9
  switch i1 %v13, label %invalid.tag.b1 [
    i1 1, label %bb5
    i1 0, label %bb6
  ]
invalid.tag.b1:
  unreachable
bb2:
  %t3 = load ptr, ptr %v0, !tbaa !13
  %v10 = getelementptr i8, ptr %t3, i64 0
  %t4.nonnegative = icmp sge i64 %v1, 0
  call void @llvm.assume(i1 %t4.nonnegative)
  %t4 = getelementptr inbounds { i64, i64, [0 x %wf.t.8b0e525ef4799b77] }, ptr %v10, i64 0, i32 2, i64 %v1
  %v11 = getelementptr i8, ptr %t4, i64 0
  ; layout-bounded transfer from %wf.arg.v2 to %v11
  %t5 = load i32, ptr %wf.arg.v2, align 1
  %t6 = getelementptr inbounds i8, ptr %wf.arg.v2, i64 4
  %t7 = load i32, ptr %t6, align 1
  %t8 = getelementptr inbounds i8, ptr %wf.arg.v2, i64 8
  %t9 = load i32, ptr %t8, align 1
  %t10 = getelementptr inbounds i8, ptr %wf.arg.v2, i64 12
  %t11 = load i8, ptr %t10, align 1
  %t12 = getelementptr inbounds i8, ptr %wf.arg.v2, i64 14
  %t13 = load i16, ptr %t12, align 1
  %t14 = getelementptr inbounds i8, ptr %wf.arg.v2, i64 16
  %t15 = load i16, ptr %t14, align 1
  %t16 = getelementptr inbounds i8, ptr %wf.arg.v2, i64 18
  %t17 = load i8, ptr %t16, align 1
  %t18 = getelementptr inbounds i8, ptr %wf.arg.v2, i64 20
  %t19 = load i32, ptr %t18, align 1
  %t20 = getelementptr inbounds i8, ptr %wf.arg.v2, i64 24
  %t21 = load i32, ptr %t20, align 1
  %t22 = getelementptr inbounds i8, ptr %wf.arg.v2, i64 28
  %t23 = load i32, ptr %t22, align 1
  %t24 = getelementptr inbounds i8, ptr %wf.arg.v2, i64 32
  %t25 = load i32, ptr %t24, align 1
  %t26 = getelementptr inbounds i8, ptr %wf.arg.v2, i64 36
  %t27 = load i32, ptr %t26, align 1
  %t28 = getelementptr inbounds i8, ptr %wf.arg.v2, i64 40
  %t29 = load i8, ptr %t28, align 1
  store i32 %t5, ptr %v11, align 1
  %t30 = getelementptr inbounds i8, ptr %v11, i64 4
  store i32 %t7, ptr %t30, align 1
  %t31 = getelementptr inbounds i8, ptr %v11, i64 8
  store i32 %t9, ptr %t31, align 1
  %t32 = getelementptr inbounds i8, ptr %v11, i64 12
  store i8 %t11, ptr %t32, align 1
  %t33 = getelementptr inbounds i8, ptr %v11, i64 14
  store i16 %t13, ptr %t33, align 1
  %t34 = getelementptr inbounds i8, ptr %v11, i64 16
  store i16 %t15, ptr %t34, align 1
  %t35 = getelementptr inbounds i8, ptr %v11, i64 18
  store i8 %t17, ptr %t35, align 1
  %t36 = getelementptr inbounds i8, ptr %v11, i64 20
  store i32 %t19, ptr %t36, align 1
  %t37 = getelementptr inbounds i8, ptr %v11, i64 24
  store i32 %t21, ptr %t37, align 1
  %t38 = getelementptr inbounds i8, ptr %v11, i64 28
  store i32 %t23, ptr %t38, align 1
  %t39 = getelementptr inbounds i8, ptr %v11, i64 32
  store i32 %t25, ptr %t39, align 1
  %t40 = getelementptr inbounds i8, ptr %v11, i64 36
  store i32 %t27, ptr %t40, align 1
  %t41 = getelementptr inbounds i8, ptr %v11, i64 40
  store i8 %t29, ptr %t41, align 1
  ; end layout-bounded transfer from %wf.arg.v2 to %v11
  %v12 = select i1 true, i1 1, i1 1
  ret i1 %v12
bb3:
  br label %bb1
bb4:
  %v14 = phi ptr [ %v6, %bb6 ]
  %v17 = phi i64 [ %v9, %bb6 ]
  %t42 = load ptr, ptr %v14, !tbaa !13
  %v19 = getelementptr i8, ptr %t42, i64 0
  %t44 = getelementptr inbounds { i64, i64, [0 x %wf.t.8b0e525ef4799b77] }, ptr %v19, i32 0, i32 1
  %t43 = load i64, ptr %t44, !tbaa !17
  %v20 = add i64 %t43, 0
  %v21 = icmp ult i64 %v17, %v20
  switch i1 %v21, label %invalid.tag.b4 [
    i1 1, label %bb8
    i1 0, label %bb9
  ]
invalid.tag.b4:
  unreachable
bb5:
  %v18 = select i1 true, i1 0, i1 0
  ret i1 %v18
bb6:
  br label %bb4
bb7:
  %v22 = phi ptr [ %v14, %bb9 ]
  %v25 = phi i64 [ %v17, %bb9 ]
  %v26 = phi i64 [ %v20, %bb9 ]
  %v30 = select i1 true, i64 4294967296, i64 4294967296
  %v31 = icmp uge i64 %v25, %v30
  switch i1 %v31, label %invalid.tag.b7 [
    i1 1, label %bb11
    i1 0, label %bb12
  ]
invalid.tag.b7:
  unreachable
bb8:
  %t45 = load ptr, ptr %v14, !tbaa !13
  %v27 = getelementptr i8, ptr %t45, i64 0
  %v28 = call i8 @wf_place_back$instance$e198f2302136eb24(ptr %v27, ptr %wf.arg.v2)
  %v29 = select i1 true, i1 1, i1 1
  ret i1 %v29
bb9:
  br label %bb7
bb10:
  %v32 = phi ptr [ %v22, %bb12 ]
  %v36 = phi i64 [ %v26, %bb12 ]
  %v38 = select i1 true, i64 8, i64 8
  %v39 = call i64 @llvm.umax.i64(i64 %v36, i64 %v38)
  %v40 = add nuw i64 %v39, %v39
  %v41 = select i1 true, i64 4294967296, i64 4294967296
  %v42 = icmp ule i64 %v40, %v41
  switch i1 %v42, label %invalid.tag.b10 [
    i1 1, label %bb14
    i1 0, label %bb15
  ]
invalid.tag.b10:
  unreachable
bb11:
  %v37 = select i1 true, i1 0, i1 0
  ret i1 %v37
bb12:
  br label %bb10
bb13:
  %v43 = phi ptr [ %v32, %bb14 ], [ %v32, %bb15 ]
  %t46 = load ptr, ptr %v43, !tbaa !13
  %v53 = getelementptr i8, ptr %t46, i64 0
  %v54 = call i8 @wf_place_back$instance$e198f2302136eb24(ptr %v53, ptr %wf.arg.v2)
  %v55 = select i1 true, i1 1, i1 1
  ret i1 %v55
bb14:
  %v50 = call i8 @wf_grow$instance$567132d7ca2a4441(ptr %v32, i64 %v40)
  br label %bb13
bb15:
  %v51 = select i1 true, i64 4294967296, i64 4294967296
  %v52 = call i8 @wf_grow$instance$567132d7ca2a4441(ptr %v32, i64 %v51)
  br label %bb13
}

define i8 @wf_font.fail(ptr noalias nonnull captures(none) dereferenceable(72) %v0) #0 align 64 {
entry:
  %t0 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v0, i32 0, i32 9
  %v1 = getelementptr i8, ptr %t0, i64 0
  %v2 = select i1 true, i1 1, i1 1
  store i1 %v2, ptr %v1
  %v3 = select i1 true, i8 0, i8 0
  ret i8 %v3
}

define i8 @wf_font.put_item(ptr noalias nonnull captures(none) dereferenceable(72) %v0, i64 %v1, ptr %wf.arg.v2) #0 align 64 {
entry:
  %wf.slot.0 = alloca %wf.t.8b0e525ef4799b77, align 4
  %t0 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v0, i32 0, i32 0
  %v3 = getelementptr i8, ptr %t0, i64 0
  %v4 = call i1 @wf_font.write_item(ptr %v3, i64 %v1, ptr %wf.arg.v2)
  switch i1 %v4, label %invalid.tag.b0 [
    i1 1, label %bb2
    i1 0, label %bb3
  ]
invalid.tag.b0:
  unreachable
bb1:
  %v10 = select i1 true, i8 0, i8 0
  ret i8 %v10
bb2:
  br label %bb1
bb3:
  %v9 = call i8 @wf_font.fail(ptr %v0)
  br label %bb1
}

define i8 @wf_font.put_out(ptr noalias nonnull captures(none) dereferenceable(72) %v0, i64 %v1, ptr %wf.arg.v2) #0 align 64 {
entry:
  %wf.slot.0 = alloca %wf.t.8b0e525ef4799b77, align 4
  %t0 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v0, i32 0, i32 6
  %v3 = getelementptr i8, ptr %t0, i64 0
  %v4 = load i1, ptr %v3
  switch i1 %v4, label %invalid.tag.b0 [
    i1 1, label %bb2
    i1 0, label %bb3
  ]
invalid.tag.b0:
  unreachable
bb1:
  %v5 = phi ptr [ %v0, %bb3 ]
  %v6 = phi i64 [ %v1, %bb3 ]
  %v16 = call i8 @wf_font.put_item(ptr %v5, i64 %v6, ptr %wf.arg.v2)
  %v17 = select i1 true, i8 0, i8 0
  ret i8 %v17
bb2:
  %t1 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v0, i32 0, i32 1
  %v8 = getelementptr i8, ptr %t1, i64 0
  %v9 = call i1 @wf_font.write_item(ptr %v8, i64 %v1, ptr %wf.arg.v2)
  switch i1 %v9, label %invalid.tag.b2 [
    i1 1, label %bb5
    i1 0, label %bb6
  ]
invalid.tag.b2:
  unreachable
bb3:
  br label %bb1
bb4:
  %v15 = select i1 true, i8 0, i8 0
  ret i8 %v15
bb5:
  br label %bb4
bb6:
  %v14 = call i8 @wf_font.fail(ptr %v0)
  br label %bb4
}

define i8 @wf_font.next_glyphs(ptr noalias nonnull captures(none) dereferenceable(72) %v0, i64 %v1) #0 align 64 {
entry:
  %wf.slot.0 = alloca %wf.t.8b0e525ef4799b77, align 4
  %t0 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v0, i32 0, i32 3
  %v2 = getelementptr i8, ptr %t0, i64 0
  %v3 = load i64, ptr %v2
  %t1 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v0, i32 0, i32 5
  %v4 = getelementptr i8, ptr %t1, i64 0
  %v5 = load i1, ptr %v4
  switch i1 %v5, label %invalid.tag.b0 [
    i1 1, label %bb2
    i1 0, label %bb3
  ]
invalid.tag.b0:
  unreachable
bb1:
  %v6 = phi ptr [ %v0, %bb3 ], [ %v16, %bb4 ]
  %v7 = phi i64 [ %v1, %bb3 ], [ %v17, %bb4 ]
  %v8 = phi i64 [ %v3, %bb3 ], [ %v18, %bb4 ]
  %v65 = add i64 %v8, %v7
  %t2 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v6, i32 0, i32 3
  %v66 = getelementptr i8, ptr %t2, i64 0
  store i64 %v65, ptr %v66, !tbaa !12
  %v67 = select i1 true, i8 0, i8 0
  ret i8 %v67
bb2:
  %t3 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v0, i32 0, i32 4
  %v9 = getelementptr i8, ptr %t3, i64 0
  %v10 = load i64, ptr %v9
  %v11 = icmp eq i64 %v10, %v3
  %t4 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v0, i32 0, i32 6
  %v12 = getelementptr i8, ptr %t4, i64 0
  %v13 = load i1, ptr %v12
  %v14 = xor i1 %v11, true
  %v15 = or i1 %v13, %v14
  switch i1 %v15, label %invalid.tag.b2 [
    i1 1, label %bb5
    i1 0, label %bb6
  ]
invalid.tag.b2:
  unreachable
bb3:
  br label %bb1
bb4:
  %v16 = phi ptr [ %v0, %bb6 ], [ %v36, %bb10 ]
  %v17 = phi i64 [ %v1, %bb6 ], [ %v37, %bb10 ]
  %v18 = phi i64 [ %v3, %bb6 ], [ %v38, %bb10 ]
  %v19 = phi i64 [ %v10, %bb6 ], [ %v39, %bb10 ]
  %v63 = add i64 %v19, %v17
  %t5 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v16, i32 0, i32 4
  %v64 = getelementptr i8, ptr %t5, i64 0
  store i64 %v63, ptr %v64, !tbaa !12
  br label %bb1
bb5:
  %v24 = select i1 true, i64 0, i64 0
  br label %bb7
bb6:
  br label %bb4
bb7:
  %v25 = phi ptr [ %v0, %bb5 ], [ %v50, %bb11 ]
  %v26 = phi i64 [ %v1, %bb5 ], [ %v51, %bb11 ]
  %v27 = phi i64 [ %v3, %bb5 ], [ %v52, %bb11 ]
  %v28 = phi i64 [ %v10, %bb5 ], [ %v53, %bb11 ]
  %v34 = phi i64 [ %v1, %bb5 ], [ %v59, %bb11 ]
  %v35 = phi i64 [ %v24, %bb5 ], [ %v62, %bb11 ]
  %v44 = icmp ult i64 %v35, %v34
  switch i1 %v44, label %invalid.tag.b7 [
    i1 1, label %bb8
    i1 0, label %bb9
  ]
invalid.tag.b7:
  unreachable
bb8:
  %v45 = add i64 %v27, %v35
  %v46 = add i64 %v28, %v35
  %t6 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v25, i32 0, i32 0
  %v47 = getelementptr i8, ptr %t6, i64 0
  call void @wf_font.read_item(ptr %wf.slot.0, ptr %v47, i64 %v45)
  %v49 = call i8 @wf_font.put_out(ptr %v25, i64 %v46, ptr %wf.slot.0)
  br label %bb11
bb9:
  br label %bb10
bb10:
  %v36 = phi ptr [ %v25, %bb9 ]
  %v37 = phi i64 [ %v26, %bb9 ]
  %v38 = phi i64 [ %v27, %bb9 ]
  %v39 = phi i64 [ %v28, %bb9 ]
  br label %bb4
bb11:
  %v50 = phi ptr [ %v25, %bb8 ]
  %v51 = phi i64 [ %v26, %bb8 ]
  %v52 = phi i64 [ %v27, %bb8 ]
  %v53 = phi i64 [ %v28, %bb8 ]
  %v59 = phi i64 [ %v34, %bb8 ]
  %v60 = phi i64 [ %v35, %bb8 ]
  %v61 = select i1 true, i64 1, i64 1
  %v62 = add i64 %v60, %v61
  br label %bb7
}

define i8 @wf_font.next_glyph(ptr noalias nonnull captures(none) dereferenceable(72) %v0) #0 align 64 {
entry:
  %v1 = select i1 true, i64 1, i64 1
  %v2 = call i8 @wf_font.next_glyphs(ptr %v0, i64 %v1)
  %v3 = select i1 true, i8 0, i8 0
  ret i8 %v3
}

define void @wf_font.item_at(ptr %wf.result, ptr noalias nonnull captures(none) dereferenceable(72) %v0, i64 %v1) #0 align 64 {
entry:
  %t0 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v0, i32 0, i32 0
  %v2 = getelementptr i8, ptr %t0, i64 0
  call void @wf_font.read_item(ptr %wf.result, ptr %v2, i64 %v1)
  ret void
}

define void @wf_font.current(ptr %wf.result, ptr noalias nonnull captures(none) dereferenceable(72) %v0) #0 align 64 {
entry:
  %t0 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v0, i32 0, i32 3
  %v1 = getelementptr i8, ptr %t0, i64 0
  %v2 = load i64, ptr %v1
  %t1 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v0, i32 0, i32 0
  %v3 = getelementptr i8, ptr %t1, i64 0
  call void @wf_font.read_item(ptr %wf.result, ptr %v3, i64 %v2)
  ret void
}

define void @wf_font.out_at(ptr %wf.result, ptr noalias nonnull captures(none) dereferenceable(72) %v0, i64 %v1) #0 align 64 {
entry:
  %t0 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v0, i32 0, i32 6
  %v2 = getelementptr i8, ptr %t0, i64 0
  %v3 = load i1, ptr %v2
  switch i1 %v3, label %invalid.tag.b0 [
    i1 1, label %bb2
    i1 0, label %bb3
  ]
invalid.tag.b0:
  unreachable
bb1:
  %v4 = phi ptr [ %v0, %bb3 ]
  %v5 = phi i64 [ %v1, %bb3 ]
  %t1 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v4, i32 0, i32 0
  %v8 = getelementptr i8, ptr %t1, i64 0
  call void @wf_font.read_item(ptr %wf.result, ptr %v8, i64 %v5)
  ret void
bb2:
  %t2 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v0, i32 0, i32 1
  %v6 = getelementptr i8, ptr %t2, i64 0
  call void @wf_font.read_item(ptr %wf.result, ptr %v6, i64 %v1)
  ret void
bb3:
  br label %bb1
}

define i1 @wf_font.make_room(ptr noalias nonnull captures(none) dereferenceable(72) %v0, i64 %v1, i64 %v2) #0 align 64 {
entry:
  %wf.slot.0 = alloca %wf.t.8b0e525ef4799b77, align 4
  %t0 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v0, i32 0, i32 4
  %v3 = getelementptr i8, ptr %t0, i64 0
  %v4 = load i64, ptr %v3
  %v5 = add i64 %v4, %v2
  %t1 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v0, i32 0, i32 7
  %v6 = getelementptr i8, ptr %t1, i64 0
  %v7 = load i64, ptr %v6
  %v8 = icmp ugt i64 %v5, %v7
  switch i1 %v8, label %invalid.tag.b0 [
    i1 1, label %bb2
    i1 0, label %bb3
  ]
invalid.tag.b0:
  unreachable
bb1:
  %v9 = phi ptr [ %v0, %bb3 ]
  %v10 = phi i64 [ %v1, %bb3 ]
  %v12 = phi i64 [ %v4, %bb3 ]
  %v13 = phi i64 [ %v5, %bb3 ]
  %t2 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v9, i32 0, i32 6
  %v16 = getelementptr i8, ptr %t2, i64 0
  %v17 = load i1, ptr %v16
  switch i1 %v17, label %invalid.tag.b1 [
    i1 1, label %bb5
    i1 0, label %bb6
  ]
invalid.tag.b1:
  unreachable
bb2:
  %v14 = call i8 @wf_font.fail(ptr %v0)
  %v15 = select i1 true, i1 0, i1 0
  ret i1 %v15
bb3:
  br label %bb1
bb4:
  %v18 = phi ptr [ %v9, %bb6 ]
  %v19 = phi i64 [ %v10, %bb6 ]
  %v21 = phi i64 [ %v12, %bb6 ]
  %v22 = phi i64 [ %v13, %bb6 ]
  %t3 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v18, i32 0, i32 3
  %v24 = getelementptr i8, ptr %t3, i64 0
  %v25 = load i64, ptr %v24
  %v26 = add i64 %v25, %v19
  %v27 = icmp ugt i64 %v22, %v26
  switch i1 %v27, label %invalid.tag.b4 [
    i1 1, label %bb8
    i1 0, label %bb9
  ]
invalid.tag.b4:
  unreachable
bb5:
  %v23 = select i1 true, i1 1, i1 1
  ret i1 %v23
bb6:
  br label %bb4
bb7:
  %v84 = select i1 true, i1 1, i1 1
  ret i1 %v84
bb8:
  %v35 = select i1 true, i64 0, i64 0
  br label %bb10
bb9:
  br label %bb7
bb10:
  %v36 = phi ptr [ %v18, %bb8 ], [ %v70, %bb17 ]
  %v44 = phi i64 [ %v21, %bb8 ], [ %v78, %bb17 ]
  %v45 = phi i64 [ %v35, %bb8 ], [ %v81, %bb17 ]
  %v53 = icmp ult i64 %v45, %v44
  switch i1 %v53, label %invalid.tag.b10 [
    i1 1, label %bb11
    i1 0, label %bb12
  ]
invalid.tag.b10:
  unreachable
bb11:
  %t4 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v36, i32 0, i32 0
  %v54 = getelementptr i8, ptr %t4, i64 0
  call void @wf_font.read_item(ptr %wf.slot.0, ptr %v54, i64 %v45)
  %t5 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v36, i32 0, i32 1
  %v56 = getelementptr i8, ptr %t5, i64 0
  %v57 = call i1 @wf_font.write_item(ptr %v56, i64 %v45, ptr %wf.slot.0)
  switch i1 %v57, label %invalid.tag.b11 [
    i1 1, label %bb15
    i1 0, label %bb16
  ]
invalid.tag.b11:
  unreachable
bb12:
  br label %bb13
bb13:
  %v46 = phi ptr [ %v36, %bb12 ]
  %t6 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v46, i32 0, i32 6
  %v82 = getelementptr i8, ptr %t6, i64 0
  %v83 = select i1 true, i1 1, i1 1
  store i1 %v83, ptr %v82
  br label %bb7
bb14:
  %v58 = phi ptr [ %v36, %bb15 ]
  br label %bb17
bb15:
  br label %bb14
bb16:
  %v68 = call i8 @wf_font.fail(ptr %v36)
  %v69 = select i1 true, i1 0, i1 0
  ret i1 %v69
bb17:
  %v70 = phi ptr [ %v58, %bb14 ]
  %v78 = phi i64 [ %v44, %bb14 ]
  %v79 = phi i64 [ %v45, %bb14 ]
  %v80 = select i1 true, i64 1, i64 1
  %v81 = add i64 %v79, %v80
  br label %bb10
}

define i1 @wf_font.shift_forward(ptr noalias nonnull captures(none) dereferenceable(72) %v0, i64 %v1) #0 align 64 {
entry:
  %wf.slot.0 = alloca %wf.t.8b0e525ef4799b77, align 4
  %wf.slot.1 = alloca %wf.t.8b0e525ef4799b77, align 4
  %t0 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v0, i32 0, i32 2
  %v2 = getelementptr i8, ptr %t0, i64 0
  %v3 = load i64, ptr %v2
  %t1 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v0, i32 0, i32 3
  %v4 = getelementptr i8, ptr %t1, i64 0
  %v5 = load i64, ptr %v4
  %v6 = add i64 %v3, %v1
  %t2 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v0, i32 0, i32 7
  %v7 = getelementptr i8, ptr %t2, i64 0
  %v8 = load i64, ptr %v7
  %v9 = icmp ugt i64 %v6, %v8
  switch i1 %v9, label %invalid.tag.b0 [
    i1 1, label %bb2
    i1 0, label %bb3
  ]
invalid.tag.b0:
  unreachable
bb1:
  %v10 = phi ptr [ %v0, %bb3 ]
  %v11 = phi i64 [ %v1, %bb3 ]
  %v12 = phi i64 [ %v3, %bb3 ]
  %v13 = phi i64 [ %v5, %bb3 ]
  %v14 = phi i64 [ %v6, %bb3 ]
  %v17 = sub i64 %v12, %v13
  %v18 = call i8 @wf_font.spend(ptr %v10, i64 %v17)
  %t3 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v10, i32 0, i32 9
  %v19 = getelementptr i8, ptr %t3, i64 0
  %v20 = load i1, ptr %v19
  switch i1 %v20, label %invalid.tag.b1 [
    i1 1, label %bb5
    i1 0, label %bb6
  ]
invalid.tag.b1:
  unreachable
bb2:
  %v15 = call i8 @wf_font.fail(ptr %v0)
  %v16 = select i1 true, i1 0, i1 0
  ret i1 %v16
bb3:
  br label %bb1
bb4:
  %v21 = phi ptr [ %v10, %bb6 ]
  %v22 = phi i64 [ %v11, %bb6 ]
  %v24 = phi i64 [ %v13, %bb6 ]
  %v25 = phi i64 [ %v14, %bb6 ]
  %v26 = phi i64 [ %v17, %bb6 ]
  call void @wf_font.blank_item(ptr %wf.slot.0)
  %t4 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v21, i32 0, i32 0
  %v29 = getelementptr i8, ptr %t4, i64 0
  %t5 = load ptr, ptr %v29, !tbaa !13
  %v30 = getelementptr i8, ptr %t5, i64 0
  %t7 = getelementptr inbounds { i64, i64, [0 x %wf.t.8b0e525ef4799b77] }, ptr %v30, i32 0, i32 0
  %t6 = load i64, ptr %t7, !tbaa !15
  %v31 = add i64 %t6, 0
  %v32 = icmp ugt i64 %v25, %v31
  switch i1 %v32, label %invalid.tag.b4 [
    i1 1, label %bb8
    i1 0, label %bb9
  ]
invalid.tag.b4:
  unreachable
bb5:
  %v27 = select i1 true, i1 0, i1 0
  ret i1 %v27
bb6:
  br label %bb4
bb7:
  %v33 = phi i64 [ %v42, %bb8 ], [ %v43, %bb9 ]
  %v34 = phi ptr [ %v21, %bb8 ], [ %v21, %bb9 ]
  %v35 = phi i64 [ %v22, %bb8 ], [ %v22, %bb9 ]
  %v37 = phi i64 [ %v24, %bb8 ], [ %v24, %bb9 ]
  %v38 = phi i64 [ %v25, %bb8 ], [ %v25, %bb9 ]
  %v39 = phi i64 [ %v26, %bb8 ], [ %v26, %bb9 ]
  %v41 = phi i64 [ %v31, %bb8 ], [ %v31, %bb9 ]
  %v44 = select i1 true, i64 0, i64 0
  br label %bb10
bb8:
  %v42 = sub nuw i64 %v25, %v31
  br label %bb7
bb9:
  %v43 = select i1 true, i64 0, i64 0
  br label %bb7
bb10:
  %v45 = phi ptr [ %v34, %bb7 ], [ %v69, %bb14 ]
  %v46 = phi i64 [ %v35, %bb7 ], [ %v70, %bb14 ]
  %v48 = phi i64 [ %v37, %bb7 ], [ %v72, %bb14 ]
  %v49 = phi i64 [ %v38, %bb7 ], [ %v73, %bb14 ]
  %v50 = phi i64 [ %v39, %bb7 ], [ %v74, %bb14 ]
  %v52 = phi i64 [ %v41, %bb7 ], [ %v76, %bb14 ]
  %v55 = phi i64 [ %v33, %bb7 ], [ %v79, %bb14 ]
  %v56 = phi i64 [ %v44, %bb7 ], [ %v82, %bb14 ]
  %v66 = icmp ult i64 %v56, %v55
  switch i1 %v66, label %invalid.tag.b10 [
    i1 1, label %bb11
    i1 0, label %bb12
  ]
invalid.tag.b10:
  unreachable
bb11:
  %v67 = add i64 %v52, %v56
  %v68 = call i8 @wf_font.put_item(ptr %v45, i64 %v67, ptr %wf.slot.0)
  br label %bb14
bb12:
  br label %bb13
bb13:
  %v57 = phi ptr [ %v45, %bb12 ]
  %v58 = phi i64 [ %v46, %bb12 ]
  %v60 = phi i64 [ %v48, %bb12 ]
  %v61 = phi i64 [ %v49, %bb12 ]
  %v62 = phi i64 [ %v50, %bb12 ]
  %v83 = select i1 true, i64 0, i64 0
  br label %bb15
bb14:
  %v69 = phi ptr [ %v45, %bb11 ]
  %v70 = phi i64 [ %v46, %bb11 ]
  %v72 = phi i64 [ %v48, %bb11 ]
  %v73 = phi i64 [ %v49, %bb11 ]
  %v74 = phi i64 [ %v50, %bb11 ]
  %v76 = phi i64 [ %v52, %bb11 ]
  %v79 = phi i64 [ %v55, %bb11 ]
  %v80 = phi i64 [ %v56, %bb11 ]
  %v81 = select i1 true, i64 1, i64 1
  %v82 = add i64 %v80, %v81
  br label %bb10
bb15:
  %v84 = phi ptr [ %v57, %bb13 ], [ %v114, %bb19 ]
  %v85 = phi i64 [ %v58, %bb13 ], [ %v115, %bb19 ]
  %v87 = phi i64 [ %v60, %bb13 ], [ %v117, %bb19 ]
  %v88 = phi i64 [ %v61, %bb13 ], [ %v118, %bb19 ]
  %v89 = phi i64 [ %v62, %bb13 ], [ %v119, %bb19 ]
  %v94 = phi i64 [ %v62, %bb13 ], [ %v124, %bb19 ]
  %v95 = phi i64 [ %v83, %bb13 ], [ %v127, %bb19 ]
  %v105 = icmp ult i64 %v95, %v94
  switch i1 %v105, label %invalid.tag.b15 [
    i1 1, label %bb16
    i1 0, label %bb17
  ]
invalid.tag.b15:
  unreachable
bb16:
  %v106 = sub i64 %v89, %v95
  %v107 = select i1 true, i64 1, i64 1
  %v108 = sub i64 %v106, %v107
  %v109 = add i64 %v87, %v108
  %v110 = add i64 %v109, %v85
  %t8 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v84, i32 0, i32 0
  %v111 = getelementptr i8, ptr %t8, i64 0
  call void @wf_font.read_item(ptr %wf.slot.1, ptr %v111, i64 %v109)
  %v113 = call i8 @wf_font.put_item(ptr %v84, i64 %v110, ptr %wf.slot.1)
  br label %bb19
bb17:
  br label %bb18
bb18:
  %v96 = phi ptr [ %v84, %bb17 ]
  %v97 = phi i64 [ %v85, %bb17 ]
  %v99 = phi i64 [ %v87, %bb17 ]
  %v100 = phi i64 [ %v88, %bb17 ]
  %t9 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v96, i32 0, i32 2
  %v128 = getelementptr i8, ptr %t9, i64 0
  store i64 %v100, ptr %v128, !tbaa !12
  %v129 = add i64 %v99, %v97
  %t10 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v96, i32 0, i32 3
  %v130 = getelementptr i8, ptr %t10, i64 0
  store i64 %v129, ptr %v130, !tbaa !12
  %v131 = select i1 true, i1 1, i1 1
  ret i1 %v131
bb19:
  %v114 = phi ptr [ %v84, %bb16 ]
  %v115 = phi i64 [ %v85, %bb16 ]
  %v117 = phi i64 [ %v87, %bb16 ]
  %v118 = phi i64 [ %v88, %bb16 ]
  %v119 = phi i64 [ %v89, %bb16 ]
  %v124 = phi i64 [ %v94, %bb16 ]
  %v125 = phi i64 [ %v95, %bb16 ]
  %v126 = select i1 true, i64 1, i64 1
  %v127 = add i64 %v125, %v126
  br label %bb15
}

define i8 @wf_font.apply_forward(ptr noalias nonnull captures(none) dereferenceable(296) %v0, ptr noalias nonnull captures(none) dereferenceable(680) %v1, ptr noalias nonnull captures(none) dereferenceable(32) %v2, ptr noalias nonnull captures(none) dereferenceable(72) %v3, ptr %wf.arg.v4, i64 %v5) #0 align 64 {
entry:
  %wf.slot.0 = alloca %wf.t.c398fc66176a258b, align 4
  %wf.slot.1 = alloca %wf.t.8b0e525ef4799b77, align 4
  call void @llvm.experimental.noalias.scope.decl(metadata !94)
  call void @llvm.experimental.noalias.scope.decl(metadata !96)
  %t0 = getelementptr inbounds %wf.t.8d63ee4ca2b3d029, ptr %v2, i32 0, i32 1
  %v6 = getelementptr i8, ptr %t0, i64 0
  %v7 = load i8, ptr %v6
  %t1 = getelementptr inbounds %wf.t.8d63ee4ca2b3d029, ptr %v2, i32 0, i32 2
  %v8 = getelementptr i8, ptr %t1, i64 0
  %v9 = load i32, ptr %v8
  br label %bb1
bb1:
  %v10 = phi ptr [ %v0, %entry ], [ %v157, %bb26 ]
  %v11 = phi ptr [ %v1, %entry ], [ %v158, %bb26 ]
  %v12 = phi ptr [ %v2, %entry ], [ %v159, %bb26 ]
  %v13 = phi ptr [ %v3, %entry ], [ %v160, %bb26 ]
  %v15 = phi i64 [ %v5, %entry ], [ %v162, %bb26 ]
  %v16 = phi i8 [ %v7, %entry ], [ %v163, %bb26 ]
  %v17 = phi i32 [ %v9, %entry ], [ %v164, %bb26 ]
  %t2 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v13, i32 0, i32 9
  %v26 = getelementptr i8, ptr %t2, i64 0
  %v27 = load i1, ptr %v26
  switch i1 %v27, label %invalid.tag.b1 [
    i1 1, label %bb4
    i1 0, label %bb5
  ]
invalid.tag.b1:
  unreachable
bb2:
  %v170 = select i1 true, i8 0, i8 0
  ret i8 %v170
bb3:
  %v28 = phi ptr [ %v10, %bb5 ]
  %v29 = phi ptr [ %v11, %bb5 ]
  %v30 = phi ptr [ %v12, %bb5 ]
  %v31 = phi ptr [ %v13, %bb5 ]
  %v33 = phi i64 [ %v15, %bb5 ]
  %v34 = phi i8 [ %v16, %bb5 ]
  %v35 = phi i32 [ %v17, %bb5 ]
  %t3 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v31, i32 0, i32 2
  %v36 = getelementptr i8, ptr %t3, i64 0
  %v37 = load i64, ptr %v36
  %t4 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v31, i32 0, i32 3
  %v38 = getelementptr i8, ptr %t4, i64 0
  %v39 = load i64, ptr %v38
  br label %bb6
bb4:
  br label %bb2
bb5:
  br label %bb3
bb6:
  %v40 = phi ptr [ %v28, %bb3 ], [ %v79, %bb11 ]
  %v41 = phi ptr [ %v29, %bb3 ], [ %v80, %bb11 ]
  %v42 = phi ptr [ %v30, %bb3 ], [ %v81, %bb11 ]
  %v43 = phi ptr [ %v31, %bb3 ], [ %v82, %bb11 ]
  %v45 = phi i64 [ %v33, %bb3 ], [ %v84, %bb11 ]
  %v46 = phi i8 [ %v34, %bb3 ], [ %v85, %bb11 ]
  %v47 = phi i32 [ %v35, %bb3 ], [ %v86, %bb11 ]
  %v48 = phi i64 [ %v37, %bb3 ], [ %v87, %bb11 ]
  %v49 = phi i64 [ %v39, %bb3 ], [ %v88, %bb11 ]
  %v50 = phi i64 [ %v39, %bb3 ], [ %v125, %bb11 ]
  %v62 = icmp uge i64 %v50, %v48
  switch i1 %v62, label %invalid.tag.b6 [
    i1 1, label %bb9
    i1 0, label %bb10
  ]
invalid.tag.b6:
  unreachable
bb7:
  %v51 = phi ptr [ %v40, %bb9 ], [ %v63, %bb18 ]
  %v52 = phi ptr [ %v41, %bb9 ], [ %v64, %bb18 ]
  %v53 = phi ptr [ %v42, %bb9 ], [ %v65, %bb18 ]
  %v54 = phi ptr [ %v43, %bb9 ], [ %v66, %bb18 ]
  %v56 = phi i64 [ %v45, %bb9 ], [ %v68, %bb18 ]
  %v57 = phi i8 [ %v46, %bb9 ], [ %v69, %bb18 ]
  %v58 = phi i32 [ %v47, %bb9 ], [ %v70, %bb18 ]
  %v60 = phi i64 [ %v49, %bb9 ], [ %v72, %bb18 ]
  %v61 = phi i64 [ %v50, %bb9 ], [ %v73, %bb18 ]
  %v126 = icmp ugt i64 %v61, %v60
  switch i1 %v126, label %invalid.tag.b7 [
    i1 1, label %bb21
    i1 0, label %bb22
  ]
invalid.tag.b7:
  unreachable
bb8:
  %v63 = phi ptr [ %v40, %bb10 ]
  %v64 = phi ptr [ %v41, %bb10 ]
  %v65 = phi ptr [ %v42, %bb10 ]
  %v66 = phi ptr [ %v43, %bb10 ]
  %v68 = phi i64 [ %v45, %bb10 ]
  %v69 = phi i8 [ %v46, %bb10 ]
  %v70 = phi i32 [ %v47, %bb10 ]
  %v71 = phi i64 [ %v48, %bb10 ]
  %v72 = phi i64 [ %v49, %bb10 ]
  %v73 = phi i64 [ %v50, %bb10 ]
  call void @wf_font.item_at(ptr %wf.slot.1, ptr %v66, i64 %v73)
  %t5 = getelementptr inbounds %wf.t.8b0e525ef4799b77, ptr %wf.slot.1, i32 0, i32 3
  %v75 = load i8, ptr %t5
  %v76 = and i8 %v75, %v69
  %v77 = select i1 true, i8 0, i8 0
  %v78 = icmp ne i8 %v76, %v77
  switch i1 %v78, label %invalid.tag.b8 [
    i1 1, label %bb12
    i1 0, label %bb13
  ]
invalid.tag.b8:
  unreachable
bb9:
  br label %bb7
bb10:
  br label %bb8
bb11:
  %v79 = phi ptr [ %v63, %bb13 ], [ %v94, %bb14 ]
  %v80 = phi ptr [ %v64, %bb13 ], [ %v95, %bb14 ]
  %v81 = phi ptr [ %v65, %bb13 ], [ %v96, %bb14 ]
  %v82 = phi ptr [ %v66, %bb13 ], [ %v97, %bb14 ]
  %v84 = phi i64 [ %v68, %bb13 ], [ %v99, %bb14 ]
  %v85 = phi i8 [ %v69, %bb13 ], [ %v100, %bb14 ]
  %v86 = phi i32 [ %v70, %bb13 ], [ %v101, %bb14 ]
  %v87 = phi i64 [ %v71, %bb13 ], [ %v102, %bb14 ]
  %v88 = phi i64 [ %v72, %bb13 ], [ %v103, %bb14 ]
  %v89 = phi i64 [ %v73, %bb13 ], [ %v104, %bb14 ]
  %v124 = select i1 true, i64 1, i64 1
  %v125 = add i64 %v89, %v124
  br label %bb6
bb12:
  %t6 = getelementptr inbounds %wf.t.8b0e525ef4799b77, ptr %wf.slot.1, i32 0, i32 0
  %v92 = load i32, ptr %t6
  %v93 = call i1 @wf_font.may_apply(ptr %v63, i64 %v68, i32 %v92)
  switch i1 %v93, label %invalid.tag.b12 [
    i1 1, label %bb15
    i1 0, label %bb16
  ]
invalid.tag.b12:
  unreachable
bb13:
  br label %bb11
bb14:
  %v94 = phi ptr [ %v63, %bb16 ], [ %v109, %bb17 ]
  %v95 = phi ptr [ %v64, %bb16 ], [ %v110, %bb17 ]
  %v96 = phi ptr [ %v65, %bb16 ], [ %v111, %bb17 ]
  %v97 = phi ptr [ %v66, %bb16 ], [ %v112, %bb17 ]
  %v99 = phi i64 [ %v68, %bb16 ], [ %v114, %bb17 ]
  %v100 = phi i8 [ %v69, %bb16 ], [ %v115, %bb17 ]
  %v101 = phi i32 [ %v70, %bb16 ], [ %v116, %bb17 ]
  %v102 = phi i64 [ %v71, %bb16 ], [ %v117, %bb17 ]
  %v103 = phi i64 [ %v72, %bb16 ], [ %v118, %bb17 ]
  %v104 = phi i64 [ %v73, %bb16 ], [ %v119, %bb17 ]
  br label %bb11
bb15:
  %v108 = call i1 @wf_font.glyph_allowed(ptr %v64, ptr %wf.slot.1, i32 %v70)
  switch i1 %v108, label %invalid.tag.b15 [
    i1 1, label %bb18
    i1 0, label %bb19
  ]
invalid.tag.b15:
  unreachable
bb16:
  br label %bb14
bb17:
  %v109 = phi ptr [ %v63, %bb19 ]
  %v110 = phi ptr [ %v64, %bb19 ]
  %v111 = phi ptr [ %v65, %bb19 ]
  %v112 = phi ptr [ %v66, %bb19 ]
  %v114 = phi i64 [ %v68, %bb19 ]
  %v115 = phi i8 [ %v69, %bb19 ]
  %v116 = phi i32 [ %v70, %bb19 ]
  %v117 = phi i64 [ %v71, %bb19 ]
  %v118 = phi i64 [ %v72, %bb19 ]
  %v119 = phi i64 [ %v73, %bb19 ]
  br label %bb14
bb18:
  br label %bb7
bb19:
  br label %bb17
bb20:
  %v127 = phi ptr [ %v51, %bb21 ], [ %v51, %bb22 ]
  %v128 = phi ptr [ %v52, %bb21 ], [ %v52, %bb22 ]
  %v129 = phi ptr [ %v53, %bb21 ], [ %v53, %bb22 ]
  %v130 = phi ptr [ %v54, %bb21 ], [ %v54, %bb22 ]
  %v132 = phi i64 [ %v56, %bb21 ], [ %v56, %bb22 ]
  %v133 = phi i8 [ %v57, %bb21 ], [ %v57, %bb22 ]
  %v134 = phi i32 [ %v58, %bb21 ], [ %v58, %bb22 ]
  %t7 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v130, i32 0, i32 3
  %v140 = getelementptr i8, ptr %t7, i64 0
  %v141 = load i64, ptr %v140
  %t8 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v130, i32 0, i32 2
  %v142 = getelementptr i8, ptr %t8, i64 0
  %v143 = load i64, ptr %v142
  %v144 = icmp uge i64 %v141, %v143
  switch i1 %v144, label %invalid.tag.b20 [
    i1 1, label %bb24
    i1 0, label %bb25
  ]
invalid.tag.b20:
  unreachable
bb21:
  %v138 = sub nuw i64 %v61, %v60
  %v139 = call i8 @wf_font.next_glyphs(ptr %v54, i64 %v138)
  br label %bb20
bb22:
  br label %bb20
bb23:
  %v145 = phi ptr [ %v127, %bb25 ]
  %v146 = phi ptr [ %v128, %bb25 ]
  %v147 = phi ptr [ %v129, %bb25 ]
  %v148 = phi ptr [ %v130, %bb25 ]
  %v150 = phi i64 [ %v132, %bb25 ]
  %v151 = phi i8 [ %v133, %bb25 ]
  %v152 = phi i32 [ %v134, %bb25 ]
  %v156 = call i1 @wf_font.apply_subtables(ptr %v145, ptr %v146, ptr %v147, ptr %v148, ptr %wf.arg.v4)
  switch i1 %v156, label %invalid.tag.b23 [
    i1 1, label %bb27
    i1 0, label %bb28
  ]
invalid.tag.b23:
  unreachable
bb24:
  br label %bb2
bb25:
  br label %bb23
bb26:
  %v157 = phi ptr [ %v145, %bb27 ], [ %v145, %bb28 ]
  %v158 = phi ptr [ %v146, %bb27 ], [ %v146, %bb28 ]
  %v159 = phi ptr [ %v147, %bb27 ], [ %v147, %bb28 ]
  %v160 = phi ptr [ %v148, %bb27 ], [ %v148, %bb28 ]
  %v162 = phi i64 [ %v150, %bb27 ], [ %v150, %bb28 ]
  %v163 = phi i8 [ %v151, %bb27 ], [ %v151, %bb28 ]
  %v164 = phi i32 [ %v152, %bb27 ], [ %v152, %bb28 ]
  br label %bb1
bb27:
  br label %bb26
bb28:
  %v169 = call i8 @wf_font.next_glyph(ptr %v148)
  br label %bb26
}

define i8 @wf_font.sync(ptr noalias nonnull captures(none) dereferenceable(72) %v0) #0 align 64 {
entry:
  %t0 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v0, i32 0, i32 9
  %v1 = getelementptr i8, ptr %t0, i64 0
  %v2 = load i1, ptr %v1
  switch i1 %v2, label %invalid.tag.b0 [
    i1 1, label %bb2
    i1 0, label %bb3
  ]
invalid.tag.b0:
  unreachable
bb1:
  %v3 = phi ptr [ %v0, %bb2 ], [ %v8, %bb4 ]
  %t1 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v3, i32 0, i32 5
  %v16 = getelementptr i8, ptr %t1, i64 0
  %v17 = select i1 true, i1 0, i1 0
  store i1 %v17, ptr %v16
  %t2 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v3, i32 0, i32 6
  %v18 = getelementptr i8, ptr %t2, i64 0
  %v19 = select i1 true, i1 0, i1 0
  store i1 %v19, ptr %v18
  %t3 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v3, i32 0, i32 4
  %v20 = getelementptr i8, ptr %t3, i64 0
  %v21 = select i1 true, i64 0, i64 0
  store i64 %v21, ptr %v20, !tbaa !12
  %t4 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v3, i32 0, i32 3
  %v22 = getelementptr i8, ptr %t4, i64 0
  %v23 = select i1 true, i64 0, i64 0
  store i64 %v23, ptr %v22, !tbaa !12
  %v24 = select i1 true, i8 0, i8 0
  ret i8 %v24
bb2:
  br label %bb1
bb3:
  %v4 = call i64 @wf_font.lookahead_length(ptr %v0)
  %v5 = call i8 @wf_font.next_glyphs(ptr %v0, i64 %v4)
  %t5 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v0, i32 0, i32 6
  %v6 = getelementptr i8, ptr %t5, i64 0
  %v7 = load i1, ptr %v6
  switch i1 %v7, label %invalid.tag.b3 [
    i1 1, label %bb5
    i1 0, label %bb6
  ]
invalid.tag.b3:
  unreachable
bb4:
  %v8 = phi ptr [ %v0, %bb5 ], [ %v0, %bb6 ]
  %t6 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v8, i32 0, i32 4
  %v13 = getelementptr i8, ptr %t6, i64 0
  %v14 = load i64, ptr %v13
  %t7 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v8, i32 0, i32 2
  %v15 = getelementptr i8, ptr %t7, i64 0
  store i64 %v14, ptr %v15, !tbaa !12
  br label %bb1
bb5:
  %t8 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v0, i32 0, i32 0
  %v10 = getelementptr i8, ptr %t8, i64 0
  %t9 = getelementptr inbounds %wf.t.5ab0411b6d87ba03, ptr %v0, i32 0, i32 1
  %v11 = getelementptr i8, ptr %t9, i64 0
  %v12 = call i8 @wf_swap$instance$8e800a5709e08aff(ptr %v10, ptr %v11)
  br label %bb4
bb6:
  br label %bb4
}

declare i1 @wf_font.apply_subtables(ptr noalias nonnull captures(none) dereferenceable(296), ptr noalias nonnull captures(none) dereferenceable(680), ptr noalias nonnull captures(none) dereferenceable(32), ptr noalias nonnull captures(none) dereferenceable(72), ptr)
declare i1 @wf_font.glyph_allowed(ptr noalias nonnull captures(none) dereferenceable(680), ptr, i32)
declare i64 @wf_font.lookahead_length(ptr noalias nonnull captures(none) dereferenceable(72))
declare i1 @wf_font.may_apply(ptr noalias nonnull captures(none) dereferenceable(296), i64, i32)
declare i8 @wf_font.spend(ptr noalias nonnull captures(none) dereferenceable(72), i64)
declare i8 @wf_grow$instance$567132d7ca2a4441(ptr noalias nonnull captures(none) dereferenceable(8), i64)
declare i8 @wf_place_back$instance$e198f2302136eb24(ptr noalias nonnull captures(none) dereferenceable(8), ptr)
declare i8 @wf_swap$instance$8e800a5709e08aff(ptr nonnull captures(none) dereferenceable(8), ptr nonnull captures(none) dereferenceable(8))
declare void @llvm.assume(i1)
attributes #0 = { "probe-stack"="inline-asm" }
