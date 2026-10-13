use super::{CompilerLimits, SourceInput, check, module_inputs};

#[test]
fn call_requirement_rejections_keep_alias_and_absolute_module_paths() {
    let graph = crate::form_module_graph(
        SourceInput::new(
            "modules.wfg",
            b"pkg::left: [];\npkg::right: [];\npkg: [pkg::left, pkg::right, std::fs, std::io, std::process, std::text];\n\nentry app = pkg::main;\n",
        ),
        CompilerLimits::default(),
    )
    .expect("the graph forms");
    let interface: &[u8] = br#"public fn take(counter: &u64, amount: u64) -> result: u64 reads(counter) contract {
  requires amount <= counter^;
} doc "Takes an amount no larger than the counter.";
"#;
    let body: &[u8] = br#"fn take(counter: &u64, amount: u64) -> result: u64 reads(counter) contract {
  requires amount <= counter^;
} {
  let bound = counter^;
  return amount;
}
"#;
    for (source, path) in [
        (
            br#"alias stats = pkg::left;

fn main() -> status: std::process::ExitStatus pure {
  let counter = 2_u64;
  let amount = stats::take(counter: &counter, amount: 12_u64);
  return std::process::exit_status(code: 0_u8);
}
"# as &[u8],
            "stats::take",
        ),
        (
            br#"fn main() -> status: std::process::ExitStatus pure {
  let counter = 2_u64;
  let amount = pkg::right::take(counter: &counter, amount: 12_u64);
  return std::process::exit_status(code: 0_u8);
}
"# as &[u8],
            "pkg::right::take",
        ),
    ] {
        let records = [
            ("left/module.wfm", interface),
            ("left/take.wf", body),
            ("right/module.wfm", interface),
            ("right/take.wf", body),
            ("module.wfm", super::ROOT_INTERFACE),
            ("main.wf", source),
        ];
        let inputs = module_inputs(&graph, &records);
        let failure = crate::check_module_program(&graph, &inputs, CompilerLimits::default())
            .expect_err("twelve exceeds the called function's requirement");
        assert_eq!(failure.rule_id(), Some("FN-8"));
        let detail = failure.to_string();
        assert!(
            detail.contains(&format!("\n  concrete_callee: {path}\n")),
            "{detail}"
        );
        assert!(!detail.contains("\n  concrete_callee: take\n"), "{detail}");
    }
}

#[test]
fn call_requirement_rejection_keeps_a_qualified_concrete_instance() {
    let graph = crate::form_module_graph(
        SourceInput::new(
            "modules.wfg",
            b"pkg::lib: [];\npkg: [pkg::lib, std::fs, std::io, std::process, std::text];\n\nentry app = pkg::main;\n",
        ),
        CompilerLimits::default(),
    )
    .expect("the graph forms");
    let records: [(&str, &[u8]); 4] = [
        (
            "lib/module.wfm",
            br#"public fn take<const n: u64>(amount: u64) -> result: u64 pure contract {
  requires amount < n;
} doc "Takes an amount below the bound.";
"#,
        ),
        (
            "lib/take.wf",
            br#"fn take<const n: u64>(amount: u64) -> result: u64 pure contract {
  requires amount < n;
} {
  return amount;
}
"#,
        ),
        ("module.wfm", super::ROOT_INTERFACE),
        (
            "main.wf",
            br#"alias stats = pkg::lib;

fn main() -> status: std::process::ExitStatus pure {
  let amount = stats::take::<2>(amount: 12_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        ),
    ];
    let inputs = module_inputs(&graph, &records);
    let failure = crate::check_module_program(&graph, &inputs, CompilerLimits::default())
        .expect_err("twelve exceeds the concrete instance's bound");
    assert_eq!(failure.rule_id(), Some("FN-8"));
    let detail = failure.to_string();
    assert!(
        detail.contains("\n  concrete_callee: stats::take::<2>\n"),
        "{detail}"
    );
}

#[test]
fn a_function_assignment_target_is_rejected_before_writability_checks() {
    let source = br#"fn target() -> result: u64 pure {
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  set target = 1_u64;
  return std::process::exit_status(code: 0_u8);
}
"#;
    let failure = check(
        &[SourceInput::new("function-target.wf", source)],
        CompilerLimits::default(),
    )
    .expect_err("a function declaration is no assignable storage");
    assert_eq!(failure.rule_id(), Some("SET-1"));
    let detail = failure.to_string();
    assert!(detail.contains(": UndeclaredSetTarget\n"), "{detail}");
    assert!(detail.contains("\n  spelling: target\n"), "{detail}");
    assert!(!detail.contains("\n  root_class:"), "{detail}");
}

#[test]
fn a_const_parameter_assignment_target_is_described_in_source_terms() {
    let source = br#"fn probe<const n: u64>() -> result: unit pure {
  set n = 1_u64;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let failure = check(
        &[SourceInput::new("const-parameter-target.wf", source)],
        CompilerLimits::default(),
    )
    .expect_err("a const parameter resolves as a place base but is not writable");
    assert_eq!(failure.rule_id(), Some("SET-1"));
    let detail = failure.to_string();
    assert!(detail.contains(": InvalidSetTarget\n"), "{detail}");
    assert!(detail.contains("\n  root_class: a const parameter\n"), "{detail}");
    assert!(!detail.contains("\n  root_class: ConstGeneric\n"), "{detail}");
}

#[test]
fn postcondition_clauses_keep_qualified_constant_spellings() {
    let graph = crate::form_module_graph(
        SourceInput::new(
            "modules.wfg",
            b"pkg::limits: [];\npkg: [pkg::limits, std::fs, std::io, std::process, std::text];\n\nentry app = pkg::main;\n",
        ),
        CompilerLimits::default(),
    )
    .expect("the graph forms");
    let records: [(&str, &[u8]); 3] = [
        ("limits/module.wfm", b"public const upper: u64 = 10_u64;\n"),
        ("module.wfm", super::ROOT_INTERFACE),
        (
            "main.wf",
            br#"alias bounds = pkg::limits;

fn small() -> result: u64 pure contract {
  ensures result < bounds::upper;
} {
  return 20_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ),
    ];
    let inputs = module_inputs(&graph, &records);
    let failure = crate::check_module_program(&graph, &inputs, CompilerLimits::default())
        .expect_err("the returned value exceeds the constant bound");
    assert_eq!(failure.rule_id(), Some("FN-9"));
    assert!(
        failure.to_string().contains("\n  clause: 20_u64 < bounds::upper\n"),
        "{failure}"
    );
    let json = failure.render(crate::DiagnosticFormat::Json);
    assert!(
        json.contains(r#""clause":"20_u64 < bounds::upper""#),
        "{json}"
    );
}
