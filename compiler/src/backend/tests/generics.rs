//! Generic instance identity, deterministic emission and cross-record lowering.
//! These are compiler implementation observations, using shared corpus sources.
use super::{compile, compile_and_run, compile_sources};

/// A host module's generic Whitefoot definition uses ordinary instance and
/// waiting-frame lowering. Reuse the source-checking conformance witness;
/// these assertions observe compiler structure, not runtime accounting.
#[test]
fn the_scoped_runner_monomorphizes_its_whitefoot_body() {
    super::system::with_ir(
        include_bytes!("../../../../tests/conformance/cases/scoped-meter-pos-runner.wf"),
        |program| {
            let runners = program
                .functions()
                .iter()
                .filter(|function| {
                    function
                        .name()
                        .starts_with("std.process.scope_run$instance$")
                })
                .collect::<Vec<_>>();
            assert_eq!(runners.len(), 1, "one concrete KeepOwner runner");
            assert!(runners[0].waits());
            assert!(
                !runners[0].blocks().is_empty(),
                "the runner has a checked body"
            );
            for name in ["scope_enter", "scope_leave"] {
                let primitive = program
                    .functions()
                    .iter()
                    .find(|function| function.name() == format!("std.process.{name}"))
                    .expect("the private primitive is an ordinary declaration");
                assert!(primitive.blocks().is_empty());
                assert!(!primitive.waits());
            }
            let llvm = crate::emit_llvm(program)
                .expect("the concrete runner emits")
                .into_string();
            assert!(llvm.contains("call i1 @wf_std.process.scope_enter("));
            assert!(llvm.contains("call i8 @wf_std.process.scope_leave("));
            assert!(
                llvm.contains("call void @wf_keep("),
                "direct supplied member call"
            );
            assert!(
                llvm.lines()
                    .any(|line| { public_definition(line, "@wf_std.process.scope_run$instance$") }),
                "the waiting runner is emitted, not a linked start/finish pair"
            );
        },
    );
}

fn compile_program(name: &str) -> String {
    match name {
        "generic_instances.wf" => compile(include_bytes!(
            "../../../../tests/programs/generic_instances.wf"
        )),
        "generic_nominals.wf" => compile(include_bytes!(
            "../../../../tests/programs/generic_nominals.wf"
        )),
        _ => unreachable!("named generic fixture"),
    }
}

/// Whether `line` defines a public symbol with `symbol` in its name. A
/// register-returned instance is emitted as its public entry and an internal
/// destination-form body (compiler/src/backend/abi.rs); it counts once, by
/// its entry.
fn public_definition(line: &str, symbol: &str) -> bool {
    line.starts_with("define ") && line.contains(symbol) && !line.contains(".body(")
}

const GENERIC_LIBRARY: &[u8] = br#"struct Pair<T: Int> {
  value: T;
}

fn bundle_pair<T: Int>(value: T) -> pair: Pair<T> pure {
  return Pair<T>(value: value);
}
"#;
const GENERIC_CONSUMER: &[u8] = br#"fn forward<T: Int>(value: T) -> pair: Pair<T> pure {
  return bundle_pair::<T>(value: value);
}

fn main() -> status: std::process::ExitStatus pure {
  let small = forward::<u8>(value: 13_u8);
  let wide = forward::<i64>(value: -17_i64);
  let small_value = small.value;
  let wide_value = wide.value;
  if small_value == 13_u8 {
  } else {
    return std::process::exit_status(code: 1_u8);
  }
  if wide_value == -17_i64 {
  } else {
    return std::process::exit_status(code: 2_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;

#[test]
fn concrete_type_and_const_instances_have_distinct_symbols_and_execute() {
    let llvm = compile_program("generic_instances.wf");
    assert_eq!(
        llvm,
        compile_program("generic_instances.wf"),
        "instance emission is deterministic"
    );
    for name in ["maximum", "forward", "preserve"] {
        let symbol = format!("@wf_{name}$instance$");
        let definitions = llvm
            .lines()
            .filter(|line| public_definition(line, &symbol))
            .collect::<Vec<_>>();
        assert_eq!(definitions.len(), 2, "{name} definitions: {definitions:?}");
        assert_ne!(definitions[0], definitions[1]);
    }
    assert_eq!(
        llvm.lines()
            .filter(|line| public_definition(line, "@wf_filled_array$instance$"))
            .count(),
        2
    );
    assert_eq!(
        llvm.lines()
            .filter(|line| public_definition(line, "@wf_filled_buffer$instance$"))
            .count(),
        1
    );

    let output = compile_and_run(&llvm);
    assert!(output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn concrete_generic_struct_enum_and_const_nominal_instances_execute() {
    let llvm = compile_program("generic_nominals.wf");
    assert_eq!(
        llvm,
        compile_program("generic_nominals.wf"),
        "instance emission is deterministic"
    );
    for name in ["duplicate", "present", "checked_sum"] {
        let symbol = format!("@wf_{name}$instance$");
        assert_eq!(
            llvm.lines()
                .filter(|line| public_definition(line, &symbol))
                .count(),
            2,
            "{name} must have one definition per concrete type"
        );
    }

    let output = compile_and_run(&llvm);
    assert!(output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn generic_instances_forward_across_ordered_source_records() {
    let llvm = compile_sources(&[
        ("library/generics.wf", GENERIC_LIBRARY),
        ("application/main.wf", GENERIC_CONSUMER),
    ]);
    for name in ["bundle_pair", "forward"] {
        assert_eq!(
            llvm.lines()
                .filter(|line| public_definition(line, &format!("@wf_{name}$instance$")))
                .count(),
            2
        );
    }
    let output = compile_and_run(&llvm);
    assert!(output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}
