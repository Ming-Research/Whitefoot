; Ordinary linked definitions with the exact Whitefoot FunctionAbi.
; Every host function's link name is its standard library identity,
; wf_std.<module>.<name> [MOD-10], which no C identifier can spell, so each
; is defined here over its C body wf__body_<name>. A range reference crosses
; the call as its element pointer and count, as every WF call passes it; the
; C bodies of those functions take a pointer to one view, since C aggregate
; parameter coercions differ between target ABIs. A narrow integer is passed
; zero-extended, as a C callee may assume on every supported target.
; A result whose scalar leaves fit the return registers crosses as its LLVM
; first-class value, one register per leaf; its C body writes the ordinary
; representation through a pointer, since C packs small struct returns
; differently on each target.
; This file is library implementation, with no compiler operation dispatch.

declare void @wf__body_host_copy_bytes(ptr, ptr, ptr, i64, i64)

define void @wf_std.text.host_copy_bytes(ptr %result, ptr %value, ptr %destination.data, i64 %destination.len, i64 %start, i64 %end) {
entry:
  %view = alloca { ptr, i64 }, align 8
  store ptr %destination.data, ptr %view, align 8
  %view.len = getelementptr inbounds { ptr, i64 }, ptr %view, i32 0, i32 1
  store i64 %destination.len, ptr %view.len, align 8
  call void @wf__body_host_copy_bytes(ptr %result, ptr %value, ptr %view, i64 %start, i64 %end)
  ret void
}

declare void @wf__body_host_copy_utf8(ptr, ptr, ptr, i64, i64)

define void @wf_std.text.host_copy_utf8(ptr %result, ptr %value, ptr %destination.data, i64 %destination.len, i64 %start, i64 %end) {
entry:
  %view = alloca { ptr, i64 }, align 8
  store ptr %destination.data, ptr %view, align 8
  %view.len = getelementptr inbounds { ptr, i64 }, ptr %view, i32 0, i32 1
  store i64 %destination.len, ptr %view.len, align 8
  call void @wf__body_host_copy_utf8(ptr %result, ptr %value, ptr %view, i64 %start, i64 %end)
  ret void
}

declare void @wf__body_read_at(ptr, ptr, ptr, ptr, i64, i64, i64)

define void @wf_std.fs.read_at(ptr %result, ptr %factory, ptr %file, ptr %destination.data, i64 %destination.len, i64 %offset, i64 %start, i64 %end) {
entry:
  %view = alloca { ptr, i64 }, align 8
  store ptr %destination.data, ptr %view, align 8
  %view.len = getelementptr inbounds { ptr, i64 }, ptr %view, i32 0, i32 1
  store i64 %destination.len, ptr %view.len, align 8
  call void @wf__body_read_at(ptr %result, ptr %factory, ptr %file, ptr %view, i64 %offset, i64 %start, i64 %end)
  ret void
}

declare void @wf__body_write_once(ptr, ptr, ptr, ptr, i64, i64)

define void @wf_std.io.write_once(ptr %result, ptr %factory, ptr %output, ptr %source.data, i64 %source.len, i64 %start, i64 %end) {
entry:
  %view = alloca { ptr, i64 }, align 8
  store ptr %source.data, ptr %view, align 8
  %view.len = getelementptr inbounds { ptr, i64 }, ptr %view, i32 0, i32 1
  store i64 %source.len, ptr %view.len, align 8
  call void @wf__body_write_once(ptr %result, ptr %factory, ptr %output, ptr %view, i64 %start, i64 %end)
  ret void
}

declare void @wf__body_open_directory(ptr, ptr, ptr, ptr, i64, i64)

define void @wf_std.fs.open_directory(ptr %result, ptr %factory, ptr %root, ptr %name.data, i64 %name.len, i64 %start, i64 %end) {
entry:
  %view = alloca { ptr, i64 }, align 8
  store ptr %name.data, ptr %view, align 8
  %view.len = getelementptr inbounds { ptr, i64 }, ptr %view, i32 0, i32 1
  store i64 %name.len, ptr %view.len, align 8
  call void @wf__body_open_directory(ptr %result, ptr %factory, ptr %root, ptr %view, i64 %start, i64 %end)
  ret void
}

declare void @wf__body_directory_next(ptr, ptr, ptr, i64, i64)

define void @wf_std.fs.directory_next(ptr %result, ptr %source, ptr %destination.data, i64 %destination.len, i64 %start, i64 %end) {
entry:
  %view = alloca { ptr, i64 }, align 8
  store ptr %destination.data, ptr %view, align 8
  %view.len = getelementptr inbounds { ptr, i64 }, ptr %view, i32 0, i32 1
  store i64 %destination.len, ptr %view.len, align 8
  call void @wf__body_directory_next(ptr %result, ptr %source, ptr %view, i64 %start, i64 %end)
  ret void
}

declare void @wf__body_open_file(ptr, ptr, ptr, ptr, i64, i64)

define void @wf_std.fs.open_file(ptr %result, ptr %factory, ptr %root, ptr %name.data, i64 %name.len, i64 %start, i64 %end) {
entry:
  %view = alloca { ptr, i64 }, align 8
  store ptr %name.data, ptr %view, align 8
  %view.len = getelementptr inbounds { ptr, i64 }, ptr %view, i32 0, i32 1
  store i64 %name.len, ptr %view.len, align 8
  call void @wf__body_open_file(ptr %result, ptr %factory, ptr %root, ptr %view, i64 %start, i64 %end)
  ret void
}

declare void @wf__body_read_next(ptr, ptr, ptr, ptr, i64, i64)

define void @wf_std.io.read_next(ptr %result, ptr %factory, ptr %input, ptr %destination.data, i64 %destination.len, i64 %start, i64 %end) {
entry:
  %view = alloca { ptr, i64 }, align 8
  store ptr %destination.data, ptr %view, align 8
  %view.len = getelementptr inbounds { ptr, i64 }, ptr %view, i32 0, i32 1
  store i64 %destination.len, ptr %view.len, align 8
  call void @wf__body_read_next(ptr %result, ptr %factory, ptr %input, ptr %view, i64 %start, i64 %end)
  ret void
}

declare void @wf__body_receive_next(ptr, ptr, ptr, i64, i64)

define void @wf_std.net.receive_next(ptr %result, ptr %receive, ptr %destination.data, i64 %destination.len, i64 %start, i64 %end) {
entry:
  %view = alloca { ptr, i64 }, align 8
  store ptr %destination.data, ptr %view, align 8
  %view.len = getelementptr inbounds { ptr, i64 }, ptr %view, i32 0, i32 1
  store i64 %destination.len, ptr %view.len, align 8
  call void @wf__body_receive_next(ptr %result, ptr %receive, ptr %view, i64 %start, i64 %end)
  ret void
}

declare void @wf__body_send_once(ptr, ptr, ptr, i64, i64)

define void @wf_std.net.send_once(ptr %result, ptr %send, ptr %source.data, i64 %source.len, i64 %start, i64 %end) {
entry:
  %view = alloca { ptr, i64 }, align 8
  store ptr %source.data, ptr %view, align 8
  %view.len = getelementptr inbounds { ptr, i64 }, ptr %view, i32 0, i32 1
  store i64 %source.len, ptr %view.len, align 8
  call void @wf__body_send_once(ptr %result, ptr %send, ptr %view, i64 %start, i64 %end)
  ret void
}

declare i64 @wf__body_args_count(ptr)

define i64 @wf_std.text.args_count(ptr %args) {
entry:
  %count = tail call i64 @wf__body_args_count(ptr %args)
  ret i64 %count
}

declare void @wf__body_arg_get(ptr, ptr, i64)

define void @wf_std.text.arg_get(ptr %result, ptr %args, i64 %position) {
entry:
  tail call void @wf__body_arg_get(ptr %result, ptr %args, i64 %position)
  ret void
}

declare i64 @wf__body_host_bytes_len(ptr)

define i64 @wf_std.text.host_bytes_len(ptr %value) {
entry:
  %length = tail call i64 @wf__body_host_bytes_len(ptr %value)
  ret i64 %length
}

declare void @wf__body_host_utf8_len(ptr, ptr)

; `Result<u64, Utf8Error>` is `{ i32, i64, i1 }`: the tag, the `Ok` value and
; the one-bit `Utf8Invalid` tag, which the C body stores as a byte.
define { i32, i64, i1 } @wf_std.text.host_utf8_len(ptr %value) {
entry:
  %result = alloca { i32, i64, i8 }, align 8
  call void @wf__body_host_utf8_len(ptr %result, ptr %value)
  %tag = load i32, ptr %result, align 8
  %ok.field = getelementptr inbounds { i32, i64, i8 }, ptr %result, i32 0, i32 1
  %ok = load i64, ptr %ok.field, align 8
  %error.field = getelementptr inbounds { i32, i64, i8 }, ptr %result, i32 0, i32 2
  %error.byte = load i8, ptr %error.field, align 8
  %error = trunc i8 %error.byte to i1
  %with.tag = insertvalue { i32, i64, i1 } poison, i32 %tag, 0
  %with.ok = insertvalue { i32, i64, i1 } %with.tag, i64 %ok, 1
  %returned = insertvalue { i32, i64, i1 } %with.ok, i1 %error, 2
  ret { i32, i64, i1 } %returned
}

declare void @wf__body_relative_path(ptr, ptr)

define void @wf_std.fs.relative_path(ptr %result, ptr %value) {
entry:
  tail call void @wf__body_relative_path(ptr %result, ptr %value)
  ret void
}

declare void @wf__body_open_read(ptr, ptr, ptr, ptr)

define void @wf_std.fs.open_read(ptr %result, ptr %factory, ptr %root, ptr %path) {
entry:
  tail call void @wf__body_open_read(ptr %result, ptr %factory, ptr %root, ptr %path)
  ret void
}

declare void @wf__body_open_directory_source(ptr, ptr, ptr)

define void @wf_std.fs.open_directory_source(ptr %result, ptr %factory, ptr %directory) {
entry:
  tail call void @wf__body_open_directory_source(ptr %result, ptr %factory, ptr %directory)
  ret void
}

declare void @wf__body_close_read(ptr, ptr, ptr)

define void @wf_std.fs.close_read(ptr %result, ptr %factory, ptr %file) {
entry:
  tail call void @wf__body_close_read(ptr %result, ptr %factory, ptr %file)
  ret void
}

declare void @wf__body_close_directory(ptr, ptr, ptr)

define void @wf_std.fs.close_directory(ptr %result, ptr %factory, ptr %directory) {
entry:
  tail call void @wf__body_close_directory(ptr %result, ptr %factory, ptr %directory)
  ret void
}

declare void @wf__body_close_directory_source(ptr, ptr, ptr)

define void @wf_std.fs.close_directory_source(ptr %result, ptr %factory, ptr %source) {
entry:
  tail call void @wf__body_close_directory_source(ptr %result, ptr %factory, ptr %source)
  ret void
}

declare void @wf__body_socket_address_v4(ptr, i8 zeroext, i8 zeroext, i8 zeroext, i8 zeroext, i16 zeroext)

define void @wf_std.net.socket_address_v4(ptr %result, i8 %a, i8 %b, i8 %c, i8 %d, i16 %port) {
entry:
  tail call void @wf__body_socket_address_v4(ptr %result, i8 zeroext %a, i8 zeroext %b, i8 zeroext %c, i8 zeroext %d, i16 zeroext %port)
  ret void
}

declare void @wf__body_socket_address_v6(ptr, i16 zeroext, i16 zeroext, i16 zeroext, i16 zeroext, i16 zeroext, i16 zeroext, i16 zeroext, i16 zeroext, i16 zeroext)

define void @wf_std.net.socket_address_v6(ptr %result, i16 %a, i16 %b, i16 %c, i16 %d, i16 %e, i16 %f, i16 %g, i16 %h, i16 %port) {
entry:
  tail call void @wf__body_socket_address_v6(ptr %result, i16 zeroext %a, i16 zeroext %b, i16 zeroext %c, i16 zeroext %d, i16 zeroext %e, i16 zeroext %f, i16 zeroext %g, i16 zeroext %h, i16 zeroext %port)
  ret void
}

declare void @wf__body_tcp_listen(ptr, ptr, ptr)

define void @wf_std.net.tcp_listen(ptr %result, ptr %factory, ptr %address) {
entry:
  tail call void @wf__body_tcp_listen(ptr %result, ptr %factory, ptr %address)
  ret void
}

declare void @wf__body_tcp_accept(ptr, ptr, ptr)

define void @wf_std.net.tcp_accept(ptr %result, ptr %factory, ptr %listener) {
entry:
  tail call void @wf__body_tcp_accept(ptr %result, ptr %factory, ptr %listener)
  ret void
}

declare void @wf__body_tcp_connect(ptr, ptr, ptr)

define void @wf_std.net.tcp_connect(ptr %result, ptr %factory, ptr %address) {
entry:
  tail call void @wf__body_tcp_connect(ptr %result, ptr %factory, ptr %address)
  ret void
}

declare void @wf__body_close_listener(ptr, ptr, ptr)

define void @wf_std.net.close_listener(ptr %result, ptr %factory, ptr %listener) {
entry:
  tail call void @wf__body_close_listener(ptr %result, ptr %factory, ptr %listener)
  ret void
}

declare void @wf__body_close_receive(ptr, ptr, ptr)

define void @wf_std.net.close_receive(ptr %result, ptr %factory, ptr %receive) {
entry:
  tail call void @wf__body_close_receive(ptr %result, ptr %factory, ptr %receive)
  ret void
}

declare void @wf__body_close_send(ptr, ptr, ptr)

define void @wf_std.net.close_send(ptr %result, ptr %factory, ptr %send) {
entry:
  tail call void @wf__body_close_send(ptr %result, ptr %factory, ptr %send)
  ret void
}

declare void @wf__body_exit_status(ptr, i8 zeroext)

define void @wf_std.process.exit_status(ptr %result, i8 %code) {
entry:
  tail call void @wf__body_exit_status(ptr %result, i8 zeroext %code)
  ret void
}
