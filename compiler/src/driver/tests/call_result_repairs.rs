//! A callee contract cannot repair a bound over storage replaced since its
//! result arrived. These checks keep that repair selection separate from the
//! source verdict and carry out the remaining guard on the conformance case.

use super::{CompilationFailureKind, CompilerLimits, SourceInput, compile, module_inputs};

const CALLEE_ROUTE: &str =
    "when the callee whose result it reads can prove the bound, state it in that callee's `ensures`";

#[test]
fn a_write_stops_call_result_provenance_through_a_loop_bound() {
    let graph = crate::form_module_graph(
        SourceInput::new(
            "modules.wfg",
            include_bytes!("../../../../tests/conformance/cases/ent5-neg-readonly-field-callee-writes-base/modules.wfg"),
        ),
        CompilerLimits::default(),
    )
    .expect("the conformance graph forms");
    let main = include_str!("../../../../tests/conformance/cases/ent5-neg-readonly-field-callee-writes-base/main.wf");
    let records: &[(&str, &[u8])] = &[
        ("main.wf", main.as_bytes()),
        (
            "module.wfm",
            include_bytes!("../../../../tests/conformance/cases/ent5-neg-readonly-field-callee-writes-base/module.wfm"),
        ),
        (
            "kinds/kind.wf",
            include_bytes!("../../../../tests/conformance/cases/ent5-neg-readonly-field-callee-writes-base/kinds/kind.wf"),
        ),
        (
            "kinds/module.wfm",
            include_bytes!("../../../../tests/conformance/cases/ent5-neg-readonly-field-callee-writes-base/kinds/module.wfm"),
        ),
    ];
    let inputs = module_inputs(&graph, records);
    let failure = crate::check_module_program(&graph, &inputs, CompilerLimits::default())
        .expect_err("the widened element no longer bounds the cell index");
    assert_eq!(failure.kind(), CompilationFailureKind::Source);
    assert_eq!(failure.rule_id(), Some("OP-4"));
    let rendered = failure.to_string();
    assert!(rendered.contains("disposition: Unproved"), "{rendered}");
    assert!(!rendered.contains(CALLEE_ROUTE), "{rendered}");
    assert!(
        rendered.contains("or guard the access with `if c < cells.len` where skipping it is the intended behavior"),
        "{rendered}"
    );

    let unguarded = "      let cell = cells[c];\n      let widened = cvt::<u8, u64>(cell);\n      set total = total +wrap widened;";
    let guarded = "      if c < cells.len {\n        let cell = cells[c];\n        let widened = cvt::<u8, u64>(cell);\n        set total = total +wrap widened;\n      }";
    assert_eq!(main.matches(unguarded).count(), 1);
    let repaired = main.replace(unguarded, guarded);
    let repaired_inputs: Vec<_> = inputs
        .iter()
        .map(|input| {
            if input.logical_path() == "main.wf" {
                input.with_bytes(repaired.as_bytes())
            } else {
                *input
            }
        })
        .collect();
    crate::check_module_program(&graph, &repaired_inputs, CompilerLimits::default())
        .expect("the guard repairs the rejected access");
    let all = super::super::with_library_records(&graph, &repaired_inputs);
    let root = graph.module_named("pkg").expect("root module");
    let selected = super::super::module_check_inputs(&graph, &all, root, false);
    let contradictions = super::super::with_checked_program(
        &selected,
        Some(graph.modules()),
        CompilerLimits::default(),
        |program, _| Ok(program.contradictory_successes(&["main".to_owned()])),
    )
    .expect("the guarded module checks");
    assert!(contradictions.is_empty(), "{contradictions:?}");
}

const PREFIX: &str = "fn clamp(y: u64) -> result: u64 pure {\n  if y < 10_u64 {\n    return y;\n  }\n  return 9_u64;\n}\n\nfn small(x: u64) -> result: u64 pure contract {\n  requires x < 10_u64;\n} {\n  return x;\n}\n\n";

fn repair_for_body(body: &str) -> String {
    let source = format!(
        "{PREFIX}fn caller(y: u64, choice: u8) -> result: u64 pure {{\n  let z = clamp(y: y);\n{body}}}\n\nfn main() -> status: std::process::ExitStatus pure {{\n  return std::process::exit_status(code: 0_u8);\n}}\n"
    );
    let failure = compile(
        &[SourceInput::new("call-result.wf", source.as_bytes())],
        CompilerLimits::default(),
    )
    .expect_err("small still requires the unproved bound");
    assert_eq!(failure.rule_id(), Some("FN-8"), "{failure}");
    failure.to_string()
}

#[test]
fn a_write_in_either_sibling_arm_preserves_the_other_arms_callee_route() {
    for body in [
        "  if choice == 0_u8 {\n    set z = y;\n    return z;\n  } else {\n    let r = small(x: z);\n    return r;\n  }\n",
        "  if choice == 0_u8 {\n    let r = small(x: z);\n    return r;\n  } else {\n    set z = y;\n    return z;\n  }\n",
    ] {
        let rendered = repair_for_body(body);
        assert!(rendered.contains(CALLEE_ROUTE), "{rendered}");
    }
}

#[test]
fn a_write_on_an_incoming_arm_removes_the_callee_route_after_the_join() {
    let rendered = repair_for_body(
        "  if choice == 0_u8 {\n    set z = y;\n  }\n  let r = small(x: z);\n  return r;\n",
    );
    assert!(!rendered.contains(CALLEE_ROUTE), "{rendered}");
    assert!(
        rendered.contains("or guard the call with `if z < 10_u64`"),
        "{rendered}"
    );
}

#[test]
fn a_write_to_another_root_preserves_a_copied_call_result_route() {
    let rendered = repair_for_body(
        "  let saved = z;\n  set y = 15_u64;\n  let r = small(x: saved);\n  return r;\n",
    );
    assert!(rendered.contains(CALLEE_ROUTE), "{rendered}");
}
