; Lookup-shaped 48-byte record: twelve i32 leaves, source at byte offset 16.
; No transfer alias metadata: copy_granules currently emits none.
target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-unknown-linux-gnu"

; Source must stay valid and unchanged until the caller completes its reads.
define ptr @accessor(ptr %base) {
entry:
  %resolved = call ptr @resolve_source(ptr %base)
  %source = getelementptr inbounds i8, ptr %resolved, i64 16
  ret ptr %source
}

define i32 @caller_partial(ptr %base, i1 %take) {
entry:
  br i1 %take, label %rare, label %common, !prof !0
rare:
  %source = call ptr @accessor(ptr %base)
  %value = load i32, ptr %source, align 1
  ret i32 %value
common:
  ret i32 0
}

define void @caller_full(ptr %out, ptr %base, i1 %take) {
entry:
  br i1 %take, label %rare, label %common, !prof !0
rare:
  %source = call ptr @accessor(ptr %base)
  %consume.v0 = load i32, ptr %source, align 1
  %consume.p1 = getelementptr inbounds i8, ptr %source, i64 4
  %consume.v1 = load i32, ptr %consume.p1, align 1
  %consume.p2 = getelementptr inbounds i8, ptr %source, i64 8
  %consume.v2 = load i32, ptr %consume.p2, align 1
  %consume.p3 = getelementptr inbounds i8, ptr %source, i64 12
  %consume.v3 = load i32, ptr %consume.p3, align 1
  %consume.p4 = getelementptr inbounds i8, ptr %source, i64 16
  %consume.v4 = load i32, ptr %consume.p4, align 1
  %consume.p5 = getelementptr inbounds i8, ptr %source, i64 20
  %consume.v5 = load i32, ptr %consume.p5, align 1
  %consume.p6 = getelementptr inbounds i8, ptr %source, i64 24
  %consume.v6 = load i32, ptr %consume.p6, align 1
  %consume.p7 = getelementptr inbounds i8, ptr %source, i64 28
  %consume.v7 = load i32, ptr %consume.p7, align 1
  %consume.p8 = getelementptr inbounds i8, ptr %source, i64 32
  %consume.v8 = load i32, ptr %consume.p8, align 1
  %consume.p9 = getelementptr inbounds i8, ptr %source, i64 36
  %consume.v9 = load i32, ptr %consume.p9, align 1
  %consume.p10 = getelementptr inbounds i8, ptr %source, i64 40
  %consume.v10 = load i32, ptr %consume.p10, align 1
  %consume.p11 = getelementptr inbounds i8, ptr %source, i64 44
  %consume.v11 = load i32, ptr %consume.p11, align 1
  store i32 %consume.v0, ptr %out, align 1
  %consume.d1 = getelementptr inbounds i8, ptr %out, i64 4
  store i32 %consume.v1, ptr %consume.d1, align 1
  %consume.d2 = getelementptr inbounds i8, ptr %out, i64 8
  store i32 %consume.v2, ptr %consume.d2, align 1
  %consume.d3 = getelementptr inbounds i8, ptr %out, i64 12
  store i32 %consume.v3, ptr %consume.d3, align 1
  %consume.d4 = getelementptr inbounds i8, ptr %out, i64 16
  store i32 %consume.v4, ptr %consume.d4, align 1
  %consume.d5 = getelementptr inbounds i8, ptr %out, i64 20
  store i32 %consume.v5, ptr %consume.d5, align 1
  %consume.d6 = getelementptr inbounds i8, ptr %out, i64 24
  store i32 %consume.v6, ptr %consume.d6, align 1
  %consume.d7 = getelementptr inbounds i8, ptr %out, i64 28
  store i32 %consume.v7, ptr %consume.d7, align 1
  %consume.d8 = getelementptr inbounds i8, ptr %out, i64 32
  store i32 %consume.v8, ptr %consume.d8, align 1
  %consume.d9 = getelementptr inbounds i8, ptr %out, i64 36
  store i32 %consume.v9, ptr %consume.d9, align 1
  %consume.d10 = getelementptr inbounds i8, ptr %out, i64 40
  store i32 %consume.v10, ptr %consume.d10, align 1
  %consume.d11 = getelementptr inbounds i8, ptr %out, i64 44
  store i32 %consume.v11, ptr %consume.d11, align 1
  ret void
common:
  ret void
}

declare ptr @resolve_source(ptr)

!0 = !{!"branch_weights", i32 1, i32 10000}
