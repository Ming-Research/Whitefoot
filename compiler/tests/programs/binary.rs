use super::support::{compile_and_run, compile_program};

#[test]
fn telemetry_packet_preserves_float_bits_through_network_bytes() {
    let llvm = compile_program("telemetry_packet.wf");
    assert!(llvm.contains("bitcast float"));
    assert!(llvm.contains("bitcast i32"));

    let output = compile_and_run(&llvm);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn continue_interpreter_executes_its_instruction_stream() {
    let llvm = compile_program("continue_interpreter.wf");
    let output = compile_and_run(&llvm);
    assert!(output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}
