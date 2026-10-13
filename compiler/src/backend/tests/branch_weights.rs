//! Static placement hints are checked on their actual LLVM successor edges.
//! These tests deliberately use C1/C2 directly, without changing process-wide
//! environment variables while the rest of the suite emits other programs.

use std::collections::BTreeMap;

use super::system::{with_ir, with_mutated_ir};
use super::{compile_and_run, emitted_body};
use crate::backend::emitter::emit_llvm_with_branch_weights;
use crate::target::TargetLayout;
use crate::{IrEnumType, IrFunction, IrMatchTarget, IrProgram, IrTerminator};

fn emit_mode(program: &IrProgram, calls: bool) -> String {
    emit_llvm_with_branch_weights(
        program,
        TargetLayout::host().expect("supported test host"),
        calls,
    )
    .expect("branch-weight fixture must emit")
    .into_string()
}

fn function<'program>(program: &'program IrProgram, name: &str) -> &'program IrFunction {
    program
        .functions()
        .iter()
        .find(|function| function.name() == name)
        .expect("fixture function exists")
}

fn block_text(body: &str, label: &str) -> String {
    let marker = format!("\n{label}:\n");
    let (_, rest) = body
        .split_once(&marker)
        .unwrap_or_else(|| panic!("missing block {label}: {body}"));
    rest.lines()
        .take_while(|line| line.starts_with(char::is_whitespace) || !line.ends_with(':'))
        .collect::<Vec<_>>()
        .join("\n")
}

fn weights(text: &str) -> Option<Vec<u32>> {
    let (_, tail) = text.split_once("!prof !{!\"branch_weights\"")?;
    let (numbers, _) = tail.split_once('}').expect("inline metadata closes");
    Some(
        numbers
            .split(',')
            .skip(1)
            .map(|number| {
                number
                    .trim()
                    .strip_prefix("i32 ")
                    .expect("branch weights are i32")
                    .parse()
                    .expect("branch weight is an integer")
            })
            .collect(),
    )
}

/// Read emitted cases and their metadata together, rather than assuming the
/// metadata's case order is the enum's tag order. The invalid default must
/// receive zero; it must not displace either real successor's weight.
fn switch_weights(
    body: &str,
    block: usize,
    targets: &[IrMatchTarget],
) -> Option<BTreeMap<u32, u32>> {
    let label = if block == 0 {
        "entry".to_owned()
    } else {
        format!("bb{block}")
    };
    let text = block_text(body, &label);
    assert!(text.contains("  switch "), "{text}");
    let values = weights(&text)?;
    assert_eq!(values.len(), targets.len() + 1, "{text}");
    assert_eq!(values[0], 0, "the unreachable default is not a hot edge");
    let cases = text
        .lines()
        .filter(|line| line.trim_start().starts_with('i') && line.contains(", label %"))
        .collect::<Vec<_>>();
    assert_eq!(cases.len(), targets.len(), "{text}");
    Some(
        cases
            .iter()
            .zip(values.into_iter().skip(1))
            .map(|(line, weight)| {
                let (value, target) = line.trim().split_once(", label %").unwrap();
                let tag: u32 = value.split_whitespace().nth(1).unwrap().parse().unwrap();
                let expected = targets.iter().find(|target| target.tag() == tag).unwrap();
                assert_eq!(target, format!("bb{}", expected.block().ordinal()));
                (tag, weight)
            })
            .collect(),
    )
}

fn bool_match(function: &IrFunction, parameter: usize) -> (usize, &[IrMatchTarget]) {
    let value = function.parameters()[parameter].0;
    function
        .blocks()
        .iter()
        .enumerate()
        .find_map(|(block, data)| match data.terminator() {
            IrTerminator::Match {
                scrutinee,
                enum_type: IrEnumType::Bool,
                targets,
            } if *scrutinee == value => Some((block, targets.as_slice())),
            _ => None,
        })
        .expect("the requested Bool parameter controls a branch")
}

fn result_match(function: &IrFunction) -> (usize, &[IrMatchTarget]) {
    function
        .blocks()
        .iter()
        .enumerate()
        .find_map(|(block, data)| match data.terminator() {
            IrTerminator::Match {
                enum_type: IrEnumType::Nominal(_),
                targets,
                ..
            } => Some((block, targets.as_slice())),
            _ => None,
        })
        .expect("the fixture matches its Result")
}

/// More than the call heuristic's sixteen-instruction leaf-size proxy.
/// Rotations and XORs also keep the helper from being an identity expression.
fn sizeable_helper() -> String {
    let mut source = String::from("fn sizeable(seed: u64) -> result: u64 pure {\n");
    let mut previous = "seed".to_owned();
    for round in 0..24 {
        source.push_str(&format!(
            "  let rotated{round} = irotl({previous}, 5_u32);\n  let value{round} = ixor(rotated{round}, seed);\n"
        ));
        previous = format!("value{round}");
    }
    source.push_str(&format!("  return {previous};\n}}\n\n"));
    source
}

#[test]
fn result_error_is_unlikely_even_when_ok_contains_a_surviving_call() {
    let source = sizeable_helper()
        + r#"fn result_edge(input: Result<u64, u64>) -> result: u64 pure {
  match input {
    Err(error: failure) => {
      return failure;
    }
    Ok(value: present) => {
      let called = sizeable(seed: present);
      let adjusted = called +wrap 1_u64;
      return adjusted;
    }
  }
}
"#;
    with_mutated_ir(source.as_bytes(), |program| {
        // Exercise both emitted case orders; tag one is Result's Err tag.
        for reverse in [false, true] {
            if reverse {
                let subject = program
                    .functions
                    .iter_mut()
                    .find(|f| f.name() == "result_edge")
                    .unwrap();
                for block in &mut subject.blocks {
                    if let IrTerminator::Match { targets, .. } = &mut block.terminator {
                        targets.reverse();
                    }
                }
            }
            let subject = function(program, "result_edge");
            let (block, targets) = result_match(subject);
            let module = emit_mode(program, true);
            let body = emitted_body(&module, "result_edge");
            assert!(body.contains("call i64 @wf_sizeable("), "{body}");
            assert_eq!(
                switch_weights(body, block, targets),
                Some(BTreeMap::from([(0, 2000), (1, 1)])),
                "S1 must keep Ok hot despite its call: {body}"
            );
        }
    });
}

#[test]
fn result_propagation_and_all_error_returns_seed_unlikely_edges() {
    let source = br#"fn propagated(input: Result<u64, u64>) -> result: Result<u64, u64> pure {
  let present = propagate input;
  return Ok<u64, u64>(value: present);
}

fn all_error(fail: Bool, error: u64) -> result: Result<u64, u64> pure {
  if fail {
    if error == 0_u64 {
      return Err<u64, u64>(error: 7_u64);
    }
    return Err<u64, u64>(error: error);
  }
  return Ok<u64, u64>(value: error);
}

fn mixed_return(fail: Bool, error: u64) -> result: Result<u64, u64> pure {
  if fail {
    if error == 0_u64 {
      return Err<u64, u64>(error: 7_u64);
    }
    return Ok<u64, u64>(value: error);
  }
  return Ok<u64, u64>(value: 9_u64);
}
"#;
    with_ir(source, |program| {
        let module = emit_mode(program, false);
        let (block, targets) = result_match(function(program, "propagated"));
        assert_eq!(
            switch_weights(emitted_body(&module, "propagated"), block, targets),
            Some(BTreeMap::from([(0, 2000), (1, 1)]))
        );
        let (block, targets) = bool_match(function(program, "all_error"), 0);
        assert_eq!(
            switch_weights(emitted_body(&module, "all_error"), block, targets),
            Some(BTreeMap::from([(0, 2000), (1, 1)])),
            "every path from the True target returns Err"
        );
        let (block, targets) = bool_match(function(program, "mixed_return"), 0);
        assert_eq!(
            switch_weights(emitted_body(&module, "mixed_return"), block, targets),
            None,
            "one possible Ok return must prevent an all-Err classification"
        );
    });
}

const SPLIT_EXITS: &[u8] = br#"enum Command {
  Work();
  Halt();
}

fn split_exit(command: Command, leave: Bool, choose: Bool) -> result: u64 pure {
  loop {
    match command {
      Work() => {
        if leave {
          return 7_u64;
        }
        if choose {
          continue;
        } else {
          continue;
        }
      }
      Halt() => {
        return 9_u64;
      }
    }
  }
}
"#;

#[test]
fn split_loop_exit_is_unlikely_but_two_continuing_edges_have_no_hint() {
    with_ir(SPLIT_EXITS, |program| {
        let module = emit_mode(program, false);
        assert!(
            module.contains(" @wf_split_exit.arm.0("),
            "the fixture must split: {module}"
        );
        let body = emitted_body(&module, "split_exit.arm.0");
        let subject = function(program, "split_exit");
        let (block, targets) = bool_match(subject, 1);
        assert_eq!(
            switch_weights(body, block, targets),
            Some(BTreeMap::from([(0, 2000), (1, 1)])),
            "True returns; False reaches the original header"
        );
        let (block, targets) = bool_match(subject, 2);
        assert_eq!(
            switch_weights(body, block, targets),
            None,
            "neither edge that returns to the header is an exit"
        );
    });
}

#[test]
fn restored_exit_weights_follow_the_bounds_guard_moved_onto_cursor_predecessors() {
    // Six carried integer words fit the ordinary x86-64 ABI too. Removing the
    // counter preserves Jnz's absolute/fall-through join, whose bounds guard
    // the split moves from an IR Match to a pair of LLVM conditional branches.
    let source = super::match_dispatch::scalar_interpreter()
        .replace("  let count = steps;\n", "")
        .replace("        let left = count -wrap 1_u64;\n", "")
        .replace("          set count = left;\n", "")
        .replace("if count != 0_u64", "if pc != 0_u64");
    with_ir(source.as_bytes(), |program| {
        let module = emit_mode(program, false);
        let body = emitted_body(&module, "run.arm.2");
        let guards = body
            .lines()
            .filter(|line| line.trim_start().starts_with("br i1 ") && line.contains(".valid"))
            .collect::<Vec<_>>();
        assert_eq!(
            guards.len(),
            2,
            "both cursor predecessors need their original guard: {body}"
        );
        for guard in guards {
            let (_, true_edge) = guard.split_once(", label %").unwrap();
            let (success, false_edge) = true_edge.split_once(", label %").unwrap();
            let (failure, _) = false_edge.split_once(',').unwrap();
            assert!(success.ends_with(".valid"), "{guard}");
            assert_eq!(weights(guard), Some(vec![2000, 1]), "{guard}");
            let valid = block_text(body, success);
            assert!(
                valid.contains("getelementptr"),
                "success forms the cursor: {valid}"
            );
            let failed = block_text(body, failure);
            let returned = failed
                .lines()
                .find_map(|line| line.trim().strip_prefix("ret i64 "))
                .expect("the failure target returns from the split loop");
            let zero = format!("{returned} = select i1 true, i64 0, i64 0");
            assert!(
                failed.lines().any(|line| line.trim() == zero),
                "the actual returned SSA value must be zero: {failed}"
            );
        }
    });
}

#[test]
fn optional_call_heuristic_weights_only_the_non_tail_call_side() {
    let source = sizeable_helper()
        + r#"fn optional_call(call_it: Bool, seed: u64) -> result: u64 pure {
  if call_it {
    let called = sizeable(seed: seed);
    let adjusted = called +wrap 1_u64;
    return adjusted;
  }
  return seed;
}
"#;
    with_ir(source.as_bytes(), |program| {
        let (block, targets) = bool_match(function(program, "optional_call"), 0);
        let c1 = emit_mode(program, false);
        assert_eq!(
            switch_weights(emitted_body(&c1, "optional_call"), block, targets),
            None
        );
        let c2 = emit_mode(program, true);
        let body = emitted_body(&c2, "optional_call");
        assert!(body.contains("call i64 @wf_sizeable("), "{body}");
        assert_eq!(
            switch_weights(body, block, targets),
            Some(BTreeMap::from([(0, 2000), (1, 1)])),
            "the conditional call must be cold only in C2: {body}"
        );
    });
}

#[test]
fn postdominating_calls_tiny_helpers_intrinsics_and_tail_calls_are_not_cold_seeds() {
    let source = sizeable_helper()
        + r#"fn tiny(seed: u64) -> result: u64 pure {
  return seed +wrap 1_u64;
}

fn shared_call(flag: Bool, seed: u64) -> result: u64 pure {
  let selected = seed;
  if flag {
    set selected = seed +wrap 1_u64;
  } else {
    set selected = seed +wrap 2_u64;
  }
  let called = sizeable(seed: selected);
  let adjusted = called +wrap 1_u64;
  return adjusted;
}

fn tiny_call(flag: Bool, seed: u64) -> result: u64 pure {
  if flag {
    let called = tiny(seed: seed);
    return called +wrap 1_u64;
  }
  return seed;
}

fn intrinsic_call(flag: Bool, seed: u64) -> result: u64 pure {
  if flag {
    return irotl(seed, 5_u32);
  }
  return seed;
}

fn tail_call(flag: Bool, seed: u64) -> result: u64 pure {
  if flag {
    return sizeable(seed: seed);
  }
  return seed;
}
"#;
    with_ir(source.as_bytes(), |program| {
        let module = emit_mode(program, true);
        for name in ["shared_call", "tiny_call", "intrinsic_call", "tail_call"] {
            let (block, targets) = bool_match(function(program, name), 0);
            let body = emitted_body(&module, name);
            assert_eq!(switch_weights(body, block, targets), None, "{name}: {body}");
        }
        assert!(emitted_body(&module, "shared_call").contains("call i64 @wf_sizeable("));
    });
}

#[test]
fn weighted_result_and_call_edges_preserve_exit_status_in_both_modes() {
    let expected = (0..24)
        .fold(3_u64, |value, _| value.rotate_left(5) ^ 3)
        .wrapping_add(1);
    let source = sizeable_helper()
        + &format!(
            r#"fn evaluate(input: Result<u64, u64>) -> result: u64 pure {{
  match input {{
    Ok(value: present) => {{
      let called = sizeable(seed: present);
      return called +wrap 1_u64;
    }}
    Err(error: failure) => {{
      return failure;
    }}
  }}
}}

fn call_or_identity(flag: Bool, seed: u64) -> result: u64 pure {{
  if flag {{
    let called = sizeable(seed: seed);
    return called +wrap 1_u64;
  }}
  return seed;
}}

fn main() -> status: std::process::ExitStatus pure {{
  let ok = Ok<u64, u64>(value: 3_u64);
  let success = evaluate(input: ok);
  if success != {expected}_u64 {{
    return std::process::exit_status(code: 1_u8);
  }}
  let err = Err<u64, u64>(error: 17_u64);
  let failure = evaluate(input: err);
  if failure != 17_u64 {{
    return std::process::exit_status(code: 2_u8);
  }}
  let called = call_or_identity(flag: True(), seed: 3_u64);
  if called != {expected}_u64 {{
    return std::process::exit_status(code: 3_u8);
  }}
  let identity = call_or_identity(flag: False(), seed: 3_u64);
  if identity != 3_u64 {{
    return std::process::exit_status(code: 4_u8);
  }}
  return std::process::exit_status(code: 0_u8);
}}
"#
        );
    with_ir(source.as_bytes(), |program| {
        for calls in [false, true] {
            let module = emit_mode(program, calls);
            assert!(emitted_body(&module, "evaluate").contains("!prof !{!\"branch_weights\""));
            let output = compile_and_run(&module);
            assert_eq!(output.status.code(), Some(0), "calls={calls}: {output:?}");
        }
    });
}
