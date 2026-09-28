//! Generic instance identity, deterministic emission and cross-record lowering.
//! These are compiler implementation observations, using shared corpus sources.
use super::{compile, compile_and_run, compile_sources};

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

#[test]
fn function_actuals_and_ordinary_calls_share_definitions_and_execute() {
    let source = br#"fn apply<fn transform(value: u64) -> result: u64 pure>(value: u64) -> result: u64 pure {
  return transform(value: value);
}

fn supplied(value: u64) -> result: u64 pure {
  return value +wrap 1_u64;
}

fn ordinary(value: u64) -> result: u64 pure {
  return value +wrap 1_u64;
}

fn recursive(value: u64) -> result: u64 pure {
  if value == 0_u64 {
    return 0_u64;
  }
  let next = value - 1_u64;
  let previous = recursive(value: next);
  return previous +wrap value;
}

fn looping(value: u64) -> result: u64 pure {
  let total = value;
  for (i in 0_u64..64_u64) {
    set total = total +wrap i;
  }
  return total;
}

fn main() -> status: std::process::ExitStatus pure {
  let bound = apply::<fn supplied>(value: 7_u64);
  let direct = supplied(value: bound);
  let control = ordinary(value: direct);
  let sum = apply::<fn recursive>(value: control);
  let total = apply::<fn looping>(value: sum);
  if total != 2071_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let module = compile(source);
    assert_eq!(
        module
            .lines()
            .filter(|line| public_definition(line, "@wf_supplied("))
            .count(),
        1,
        "ordinary and bound use share one definition"
    );
    let output = compile_and_run(&module);
    assert!(output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn function_actuals_survive_group_forwarding_and_structured_fragments() {
    let source = br#"struct Packet {
  value: u64;
}

interface Builder {
  fn make(value: u64) -> result: Packet pure;
}

fn make_packet(value: u64) -> result: Packet pure {
  return Packet(value: value);
}

binding First : Builder {
  make = make_packet;
}

binding Second : Builder {
  make = make_packet;
}

fn gather<interface Builder>(value: u64) -> result: Packet pure {
  return Builder::make(value: value);
}

fn forward<interface Builder>(value: u64) -> result: Packet pure {
  return gather::<Builder>(value: value);
}

fn main() -> status: std::process::ExitStatus pure {
  let first = forward::<First>(value: 11_u64);
  let second = forward::<Second>(value: first.value);
  if second.value != 11_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let module = crate::compile(
        &[crate::SourceInput::new("groups.wf", source)],
        crate::CompilerLimits::default(),
    )
    .expect("groups compile");
    let llvm = module.as_str();
    assert_eq!(llvm, compile(source), "emission is deterministic");
    for symbol in ["@wf_make_packet(", "@wf_make_packet.body("] {
        let definitions = llvm
            .lines()
            .filter(|line| line.starts_with("define ") && line.contains(symbol))
            .collect::<Vec<_>>();
        assert_eq!(definitions.len(), 1, "{symbol}: {definitions:?}");
    }
    for name in ["gather", "forward"] {
        let definitions = llvm
            .lines()
            .filter(|line| public_definition(line, &format!("@wf_{name}$instance$")))
            .collect::<Vec<_>>();
        assert_eq!(
            definitions.len(),
            1,
            "equivalent bindings share one {name}: {definitions:?}"
        );
    }
    let decoded = crate::LlvmModule::decode(&module.encode()).expect("structured module decodes");
    assert_eq!(decoded, module);
    for granularity in [
        crate::FragmentGranularity::Function,
        crate::FragmentGranularity::Module,
    ] {
        let fragments = crate::split_module(&decoded, granularity).expect("module splits");
        let definitions = fragments
            .iter()
            .flat_map(|fragment| fragment.lines())
            .filter(|line| {
                line.starts_with("define ")
                    && (line.contains("@wf_make_packet(") || line.contains("@wf_make_packet.body("))
            })
            .collect::<Vec<_>>();
        assert_eq!(definitions.len(), 2, "{definitions:?}");
        assert!(
            fragments.iter().any(|fragment| fragment
                .lines()
                .any(|line| line.starts_with("declare ") && line.contains("@wf_make_packet("))),
            "a caller's fragment declares its concrete callee"
        );
    }
    let output = compile_and_run(llvm);
    assert!(output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}
