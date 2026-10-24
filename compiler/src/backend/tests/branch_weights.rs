//! Static placement hints are checked on their actual LLVM successor edges.
//! These tests deliberately use C1/C2 directly, without changing process-wide
//! environment variables while the rest of the suite emits other programs.

use std::collections::{BTreeMap, HashSet};

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

/// An if's empty false arm can forward to a separate continuation holding
/// the return. Follow the emitted edges; a cycle, conditional or transfer
/// cannot stand in for this fixture's unconditional zero-return path.
fn assert_returns_zero(body: &str, start: &str) {
    let mut label = start.to_owned();
    let mut seen = HashSet::new();
    let mut path = String::new();
    let returned = loop {
        assert!(seen.insert(label.clone()), "failure path cycles at {label}");
        let block = block_text(body, &label);
        path.push_str(&block);
        path.push('\n');
        if let Some(value) = block
            .lines()
            .find_map(|line| line.trim().strip_prefix("ret i64 "))
        {
            break value.to_owned();
        }
        label = block
            .lines()
            .find_map(|line| line.trim().strip_prefix("br label %"))
            .unwrap_or_else(|| panic!("failure must return or forward unconditionally: {block}"))
            .to_owned();
    };
    let zero = format!("{returned} = select i1 true, i64 0, i64 0");
    assert!(
        path.lines().any(|line| line.trim() == zero),
        "the failure path must return its zero SSA value: {path}"
    );
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
    // Loops and continuing matches forward bindings into fresh block
    // parameters. These fixtures never assign their Bool inputs; trace the
    // forwarding edges instead of assuming the formal SSA name survives.
    let mut aliases = HashSet::from([value]);
    loop {
        let before = aliases.len();
        for block in function.blocks() {
            if let IrTerminator::Jump {
                target, arguments, ..
            } = block.terminator()
            {
                for ((parameter, _), argument) in function.blocks()[target.index()]
                    .parameters()
                    .iter()
                    .zip(arguments)
                {
                    if aliases.contains(argument) {
                        aliases.insert(*parameter);
                    }
                }
            }
        }
        if aliases.len() == before {
            break;
        }
    }
    for block in function.blocks() {
        if let IrTerminator::Jump {
            target, arguments, ..
        } = block.terminator()
        {
            for ((parameter, _), argument) in function.blocks()[target.index()]
                .parameters()
                .iter()
                .zip(arguments)
            {
                assert!(
                    !aliases.contains(parameter) || aliases.contains(argument),
                    "a fixture changed the Bool input instead of forwarding it"
                );
            }
        }
    }
    function
        .blocks()
        .iter()
        .enumerate()
        .find_map(|(block, data)| match data.terminator() {
            IrTerminator::Match {
                scrutinee,
                enum_type: IrEnumType::Bool,
                targets,
            } if aliases.contains(scrutinee) => Some((block, targets.as_slice())),
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
            assert_returns_zero(body, failure);
        }
    });
}

#[test]
fn a_call_free_return_is_no_evidence_against_the_call_side() {
    // The shape of a guard: return early on a degenerate input, otherwise
    // do the work that calls. Either side may be the common one, so neither
    // mode predicts, and the unconditional call keeps the host's ordinary
    // inlining budget.
    let source = sizeable_helper()
        + r#"fn guarded_call(call_it: Bool, seed: u64) -> result: u64 pure {
  if call_it {
    let called = sizeable(seed: seed);
    let adjusted = called +wrap 1_u64;
    return adjusted;
  }
  return seed;
}
"#;
    with_ir(source.as_bytes(), |program| {
        let (block, targets) = bool_match(function(program, "guarded_call"), 0);
        for calls in [false, true] {
            let module = emit_mode(program, calls);
            let body = emitted_body(&module, "guarded_call");
            assert!(body.contains("call i64 @wf_sizeable("), "{body}");
            assert_eq!(
                switch_weights(body, block, targets),
                None,
                "calls={calls}: a return without a call must not make the call cold: {body}"
            );
        }
    });
}

#[test]
fn ordinary_loop_suppresses_call_hint_but_split_continuation_receives_it() {
    let source = sizeable_helper()
        + r#"enum Command {
  Work();
  Halt();
}

fn ordinary_call_loop(call_it: Bool, seed: u64) -> result: u64 pure {
  loop {
    if call_it {
      let called = sizeable(seed: seed);
      set seed = called;
      continue;
    }
    return seed;
  }
}

fn split_call_loop(command: Command, call_it: Bool, seed: u64) -> result: u64 pure {
  loop {
    match command {
      Work() => {
        if call_it {
          let called = sizeable(seed: seed);
          set seed = called;
          continue;
        } else {
          continue;
        }
      }
      Halt() => {
        return seed;
      }
    }
  }
}
"#;
    with_ir(source.as_bytes(), |program| {
        let module = emit_mode(program, true);
        assert!(
            !module.contains(" @wf_ordinary_call_loop.arm."),
            "the ordinary Bool loop must remain unsplit: {module}"
        );
        let ordinary = emitted_body(&module, "ordinary_call_loop");
        assert!(ordinary.contains("call i64 @wf_sizeable("), "{ordinary}");
        let (block, targets) = bool_match(function(program, "ordinary_call_loop"), 0);
        assert_eq!(
            switch_weights(ordinary, block, targets),
            None,
            "a surviving ordinary backedge must suppress S3: {ordinary}"
        );
        assert!(
            module.contains(" @wf_split_call_loop.arm.0("),
            "the nominal dispatch loop must actually split: {module}"
        );
        let split = emitted_body(&module, "split_call_loop.arm.0");
        assert!(split.contains("call i64 @wf_sizeable("), "{split}");
        let (block, targets) = bool_match(function(program, "split_call_loop"), 1);
        assert_eq!(
            switch_weights(split, block, targets),
            Some(BTreeMap::from([(0, 2000), (1, 1)])),
            "both edges continue, but only True contains a surviving call: {split}"
        );
        let c1 = emit_mode(program, false);
        assert_eq!(
            switch_weights(emitted_body(&c1, "split_call_loop.arm.0"), block, targets),
            None,
            "the split continuation's hint must come from S3, not S2"
        );
    });
}

const COMMAND: &str = "enum Command {\n  Work();\n  Halt();\n}\n\n";

/// A nominal dispatch loop that splits: its Work arm calls `callee` on one
/// side of `flag` and continues on the other, so both edges reach the
/// restored header and only the call distinguishes them.
fn split_caller(name: &str, callee: &str) -> String {
    format!(
        "fn {name}(command: Command, flag: Bool, seed: u64) -> result: u64 pure {{
  loop {{
    match command {{
      Work() => {{
        if flag {{
          let called = {callee}(seed: seed);
          set seed = called;
          continue;
        }} else {{
          continue;
        }}
      }}
      Halt() => {{
        return seed;
      }}
    }}
  }}
}}
"
    )
}

/// The Work arm's body in `module`, which must have split the loop.
fn work_arm<'module>(module: &'module str, name: &str) -> &'module str {
    assert!(
        module.contains(&format!(" @wf_{name}.arm.0(")),
        "the nominal dispatch loop {name} must actually split: {module}"
    );
    emitted_body(module, &format!("{name}.arm.0"))
}

#[test]
fn small_callee_cutoff_is_exactly_sixteen_ir_instructions() {
    let mut source = String::from(COMMAND);
    for count in [16, 17] {
        if count == 17 {
            source.push('\n');
        }
        source.push_str(&format!(
            "fn leaf{count}(seed: u64) -> result: u64 pure {{\n"
        ));
        let mut previous = "seed".to_owned();
        for instruction in 0..count {
            // Two existing SSA operands produce exactly one IR instruction;
            // no literal materialization can shift the tested boundary.
            source.push_str(&format!(
                "  let value{instruction} = ixor({previous}, seed);\n"
            ));
            previous = format!("value{instruction}");
        }
        source.push_str(&format!("  return {previous};\n}}\n\n"));
        source.push_str(&split_caller(
            &format!("split_leaf{count}"),
            &format!("leaf{count}"),
        ));
    }
    with_ir(source.as_bytes(), |program| {
        let module = emit_mode(program, true);
        for count in [16, 17] {
            let leaf = function(program, &format!("leaf{count}"));
            assert_eq!(
                leaf.blocks()
                    .iter()
                    .map(|block| block.instructions().len())
                    .sum::<usize>(),
                count,
                "the fixture must test the exact IR boundary"
            );
            let name = format!("split_leaf{count}");
            let (block, targets) = bool_match(function(program, &name), 1);
            let body = work_arm(&module, &name);
            assert!(
                body.contains(&format!("call i64 @wf_leaf{count}(")),
                "{body}"
            );
            let expected = (count == 17).then(|| BTreeMap::from([(0, 2000), (1, 1)]));
            assert_eq!(
                switch_weights(body, block, targets),
                expected,
                "{name}: {body}"
            );
        }
    });
}

#[test]
fn small_wrapper_inherits_its_inner_surviving_call() {
    let source = String::from(COMMAND)
        + &sizeable_helper()
        + r#"fn wrapper(seed: u64) -> result: u64 pure {
  let called = sizeable(seed: seed);
  return called +wrap seed;
}

"# + &split_caller("split_wrapper", "wrapper");
    with_ir(source.as_bytes(), |program| {
        let wrapper = function(program, "wrapper");
        assert!(
            wrapper
                .blocks()
                .iter()
                .map(|block| block.instructions().len())
                .sum::<usize>()
                <= 16,
            "the wrapper must qualify for the small-body estimate"
        );
        let module = emit_mode(program, true);
        let wrapper_body = emitted_body(&module, "wrapper");
        assert!(
            wrapper_body.contains("call i64 @wf_sizeable("),
            "{wrapper_body}"
        );
        let body = work_arm(&module, "split_wrapper");
        assert!(body.contains("call i64 @wf_wrapper("), "{body}");
        let (block, targets) = bool_match(function(program, "split_wrapper"), 1);
        assert_eq!(
            switch_weights(body, block, targets),
            Some(BTreeMap::from([(0, 2000), (1, 1)])),
            "inlining the small wrapper still leaves its large inner call: {body}"
        );
    });
}

#[test]
fn distinct_surviving_calls_on_both_sides_receive_no_hint() {
    // True calls and continues; False calls before another branch. Neither
    // side has a call-free path to the header.
    let source = String::from(COMMAND)
        + &sizeable_helper()
        + &sizeable_helper().replace("sizeable", "other_sizeable")
        + r#"fn distinct_calls(command: Command, flag: Bool, seed: u64) -> result: u64 pure {
  loop {
    match command {
      Work() => {
        if flag {
          let called = sizeable(seed: seed);
          set seed = called;
          continue;
        } else {
          let called = other_sizeable(seed: seed);
          if called == 0_u64 {
            return seed;
          }
          set seed = called;
          continue;
        }
      }
      Halt() => {
        return seed;
      }
    }
  }
}
"#;
    with_ir(source.as_bytes(), |program| {
        let module = emit_mode(program, true);
        let body = work_arm(&module, "distinct_calls");
        assert!(body.contains("call i64 @wf_sizeable("), "{body}");
        assert!(body.contains("call i64 @wf_other_sizeable("), "{body}");
        let (block, targets) = bool_match(function(program, "distinct_calls"), 1);
        assert_eq!(
            switch_weights(body, block, targets),
            None,
            "neither distinct call has an opposite call-free path: {body}"
        );
    });
}

#[test]
fn postdominating_calls_tiny_helpers_intrinsics_and_tail_calls_are_not_cold_seeds() {
    let source = String::from(COMMAND)
        + &sizeable_helper()
        + r#"fn tiny(seed: u64) -> result: u64 pure {
  return seed +wrap 1_u64;
}

fn shared_call(command: Command, flag: Bool, seed: u64) -> result: u64 pure {
  loop {
    match command {
      Work() => {
        let selected = seed;
        if flag {
          set selected = seed +wrap 1_u64;
        } else {
          set selected = seed +wrap 2_u64;
        }
        let called = sizeable(seed: selected);
        set seed = called;
        continue;
      }
      Halt() => {
        return seed;
      }
    }
  }
}

"# + &split_caller("tiny_call", "tiny")
        + r#"
fn intrinsic_call(command: Command, flag: Bool, seed: u64) -> result: u64 pure {
  loop {
    match command {
      Work() => {
        if flag {
          set seed = irotl(seed, 5_u32);
          continue;
        } else {
          continue;
        }
      }
      Halt() => {
        return seed;
      }
    }
  }
}

fn tail_call(command: Command, flag: Bool, leave: Bool, seed: u64) -> result: u64 pure {
  loop {
    match command {
      Work() => {
        if flag {
          if leave {
            return sizeable(seed: seed);
          }
          continue;
        } else {
          continue;
        }
      }
      Halt() => {
        return seed;
      }
    }
  }
}
"#;
    with_ir(source.as_bytes(), |program| {
        let module = emit_mode(program, true);
        for name in ["shared_call", "tiny_call", "intrinsic_call", "tail_call"] {
            let (block, targets) = bool_match(function(program, name), 1);
            let body = work_arm(&module, name);
            assert_eq!(switch_weights(body, block, targets), None, "{name}: {body}");
        }
        assert!(work_arm(&module, "shared_call").contains("call i64 @wf_sizeable("));
        assert!(work_arm(&module, "tail_call").contains("call i64 @wf_sizeable("));
    });
}

#[test]
fn weighted_result_and_call_edges_preserve_exit_status_in_both_modes() {
    // counted_call_loop is a split dispatch loop that ends after `rounds`
    // passes, so both the predicted call edge and its continuation run.
    let sized = (0..24).fold(3_u64, |value, _| value.rotate_left(5) ^ 3);
    let expected = sized.wrapping_add(1);
    let source = String::from(COMMAND)
        + &sizeable_helper()
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

fn counted_call_loop(command: Command, flag: Bool, seed: u64, rounds: u64) -> result: u64 pure {{
  loop {{
    match command {{
      Work() => {{
        if rounds == 0_u64 {{
          return seed;
        }}
        set rounds = rounds -wrap 1_u64;
        if flag {{
          let called = sizeable(seed: seed);
          set seed = called;
          continue;
        }} else {{
          continue;
        }}
      }}
      Halt() => {{
        return seed;
      }}
    }}
  }}
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
  let enabled = True();
  let called = call_or_identity(flag: enabled, seed: 3_u64);
  if called != {expected}_u64 {{
    return std::process::exit_status(code: 3_u8);
  }}
  let disabled = False();
  let identity = call_or_identity(flag: disabled, seed: 3_u64);
  if identity != 3_u64 {{
    return std::process::exit_status(code: 4_u8);
  }}
  let first = Command::Work();
  let loop_on = True();
  let looped = counted_call_loop(command: first, flag: loop_on, seed: 3_u64, rounds: 1_u64);
  if looped != {sized}_u64 {{
    return std::process::exit_status(code: 5_u8);
  }}
  let second = Command::Work();
  let loop_off = False();
  let skipped = counted_call_loop(command: second, flag: loop_off, seed: 3_u64, rounds: 1_u64);
  if skipped != 3_u64 {{
    return std::process::exit_status(code: 6_u8);
  }}
  return std::process::exit_status(code: 0_u8);
}}
"#
        );
    with_ir(source.as_bytes(), |program| {
        for calls in [false, true] {
            let mut module = emit_mode(program, calls);
            // Lowered callable IR needs the same launcher the driver adds
            // before the floor runtime can enter the source main function.
            module.push_str(
                &crate::driver::launcher::render(program, "main")
                    .expect("the runtime fixture has an ordinary executable entry")
                    .render(),
            );
            assert!(module.contains("define i32 @wf__main_body("));
            assert!(emitted_body(&module, "evaluate").contains("!prof !{!\"branch_weights\""));
            if calls {
                assert!(
                    work_arm(&module, "counted_call_loop").contains("!prof !{!\"branch_weights\""),
                    "the executed split loop must carry predictions: {module}"
                );
            }
            let output = compile_and_run(&module);
            assert_eq!(output.status.code(), Some(0), "calls={calls}: {output:?}");
        }
    });
}
