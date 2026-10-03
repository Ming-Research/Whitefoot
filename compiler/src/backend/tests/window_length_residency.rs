//! Executable observations of length publication across append, growth,
//! read-only observers, and loop exits. Remove with a withdrawn
//! length-residency transform or merge into the window backend tests.

use crate::{
    IrConstant, IrInstruction, IrOperation, IrProgram, IrSynthesis, OverlapLowering, emit_llvm,
};

use super::{
    compile_link_and_run, emit_lowered,
    system::{with_ir, with_mutated_ir_lowering},
};

const SOURCE: &str = r#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn inspect_length(values: &Box<Slots<u64>>) -> result: u64 reads(values.inner.len) {
  let seen = values^.inner.len;
  let result = seen;
  for (index in 0_u64..1_u64) {
    set result = result +wrap index;
  }
  return result;
}

fn push_one(values: &Box<Slots<u64>>, value: u64) -> length: u64 writes(values) contract {
  requires values^.inner.len < 4_u64;
  ensures length == values^.inner.len;
  ensures values^.inner.len == entry(values)^.inner.len + 1_u64;
  ensures values^.inner.cap >= entry(values)^.inner.cap;
} {
  if values^.inner.cap <= values^.inner.len {
    grow(cell: values, capacity: 4_u64);
  }
  place_back(window: &values^.inner, value: value);
  return values^.inner.len;
}

fn append_count(values: &Box<Slots<u64>>, count: u64, seed: u64) -> length: u64 writes(values) contract {
  requires values^.inner.len + count <= 4_u64;
} {
  let initial = values^.inner.len;
  for @append (
    index in 0_u64..count,
    invariant length_lo: values^.inner.len >= initial + index,
    invariant length_hi: values^.inner.len <= initial + index
  ) {
    let value = seed +wrap index;
    let made = push_one(values: values, value: value);
  }
  return values^.inner.len;
}

fn append_observed(values: &Box<Slots<u64>>) -> result: u64 writes(values) contract {
  requires values^.inner.len == 0_u64;
} {
  for @append (
    index in 0_u64..2_u64,
    invariant length_lo: values^.inner.len >= index,
    invariant length_hi: values^.inner.len <= index
  ) {
    let value = 17_u64 +wrap index;
    let made = push_one(values: values, value: value);
    let seen = inspect_length(values: values);
    let expected = index +wrap 1_u64;
    if seen != expected {
      return 1_u64;
    }
    if made != seen {
      return 1_u64;
    }
  }
  return 0_u64;
}

fn remove_one(values: &Box<Slots<u64>>) -> removed: u64 writes(values) contract {
  requires values^.inner.len == 1_u64;
  ensures values^.inner.len == 0_u64;
  ensures values^.inner.cap == entry(values)^.inner.cap;
} {
  let taken = take_back(window: &values^.inner);
  let result = taken;
  for (index in 0_u64..1_u64) {
    set result = result +wrap index;
  }
  return result;
}

fn append_then_remove(values: &Box<Slots<u64>>) -> result: u64 writes(values) contract {
  requires values^.inner.len == 0_u64;
  requires values^.inner.cap == 4_u64;
} {
  for @append (
    index in 0_u64..2_u64,
    invariant length_lo: values^.inner.len >= 0_u64,
    invariant length_hi: values^.inner.len <= 0_u64,
    invariant capacity_lo: values^.inner.cap >= 4_u64
  ) {
    let expected = 81_u64 +wrap index;
    let made = push_one(values: values, value: expected);
    let removed = remove_one(values: values);
    if removed != expected {
      return 1_u64;
    }
    let observed = values^.inner.len;
    if observed != 0_u64 {
      return 2_u64;
    }
  }
  return 0_u64;
}

fn replace_owner(slot: &Box<Slots<u64>>) -> old: u64 writes(slot) contract {
  requires slot^.inner.len == 1_u64;
  ensures slot^.inner.len == 0_u64;
  ensures slot^.inner.cap == 4_u64;
} {
  let old = slot^.inner.len;
  let fresh = box_slots_new::<u64>(capacity: 4_u64);
  set slot^ = move fresh;
  return old;
}

fn append_then_replace(values: &Box<Slots<u64>>) -> result: u64 writes(values) contract {
  requires values^.inner.len == 0_u64;
  requires values^.inner.cap == 4_u64;
} {
  for @append (
    index in 0_u64..2_u64,
    invariant length_lo: values^.inner.len >= 0_u64,
    invariant length_hi: values^.inner.len <= 0_u64,
    invariant capacity_lo: values^.inner.cap >= 4_u64
  ) {
    let value = 91_u64 +wrap index;
    let made = push_one(values: values, value: value);
    let old = replace_owner(slot: values);
    if old != 1_u64 {
      return 1_u64;
    }
    if values^.inner.len != 0_u64 {
      return 2_u64;
    }
  }
  return 0_u64;
}

fn append_outer(outer: &Box<Box<Slots<u64>>>) -> result: u64 writes(outer) contract {
  requires outer^.inner.inner.len == 0_u64;
  requires outer^.inner.inner.cap == 4_u64;
} {
  for @append (
    index in 0_u64..2_u64,
    invariant length_lo: outer^.inner.inner.len >= index,
    invariant length_hi: outer^.inner.inner.len <= index
  ) {
    let value = 101_u64 +wrap index;
    place_back(window: &outer^.inner.inner, value: value);
  }
  return outer^.inner.inner.len;
}

fn ordinary_and_borrow() -> result: u64 pure {
  let values = box_slots_new::<u64>(capacity: 4_u64);
  let observed = append_observed(values: &values);
  if observed != 0_u64 {
    return 1_u64;
  }
  if values.inner.len != 2_u64 {
    return 2_u64;
  }
  if values.inner.cap != 4_u64 {
    return 2_u64;
  }
  if values.inner[0_u64] != 17_u64 {
    return 3_u64;
  }
  if values.inner[1_u64] != 18_u64 {
    return 3_u64;
  }
  return 0_u64;
}

fn zero_and_nonzero_start() -> result: u64 pure {
  let values = box_slots_new::<u64>(capacity: 4_u64);
  place_back(window: &values.inner, value: 41_u64);
  let none = append_count(values: &values, count: 0_u64, seed: 0_u64);
  if none != 1_u64 {
    return 1_u64;
  }
  if values.inner.len != 1_u64 {
    return 1_u64;
  }
  if values.inner[0_u64] != 41_u64 {
    return 1_u64;
  }
  let length = append_count(values: &values, count: 2_u64, seed: 42_u64);
  if length != 3_u64 {
    return 2_u64;
  }
  if values.inner.len != 3_u64 {
    return 2_u64;
  }
  if values.inner[0_u64] != 41_u64 {
    return 2_u64;
  }
  if values.inner[1_u64] != 42_u64 {
    return 2_u64;
  }
  if values.inner[2_u64] != 43_u64 {
    return 2_u64;
  }
  return 0_u64;
}

fn relocating_growth() -> result: u64 pure {
  let values = box_slots_new::<u64>(capacity: 1_u64);
  let length = append_count(values: &values, count: 2_u64, seed: 71_u64);
  if length != 2_u64 {
    return 1_u64;
  }
  if values.inner.len != 2_u64 {
    return 1_u64;
  }
  if values.inner.cap != 4_u64 {
    return 1_u64;
  }
  if values.inner[0_u64] != 71_u64 {
    return 2_u64;
  }
  if values.inner[1_u64] != 72_u64 {
    return 2_u64;
  }
  return 0_u64;
}

fn append_until_break(values: &Box<Slots<u64>>) -> result: unit writes(values) contract {
  requires values^.inner.len == 0_u64;
} {
  for @append (
    index in 0_u64..4_u64,
    invariant length_lo: values^.inner.len >= index,
    invariant length_hi: values^.inner.len <= index
  ) {
    let value = index +wrap 100_u64;
    let length = push_one(values: values, value: value);
    if index == 2_u64 {
      break @append;
    }
  }
  return unit;
}

fn early_break() -> result: u64 pure {
  let values = box_slots_new::<u64>(capacity: 4_u64);
  append_until_break(values: &values);
  if values.inner.len != 3_u64 {
    return 1_u64;
  }
  if values.inner.cap != 4_u64 {
    return 1_u64;
  }
  if values.inner[0_u64] != 100_u64 {
    return 2_u64;
  }
  if values.inner[1_u64] != 101_u64 {
    return 2_u64;
  }
  if values.inner[2_u64] != 102_u64 {
    return 2_u64;
  }
  return 0_u64;
}

fn ordinary_call_reload() -> result: u64 pure {
  let values = box_slots_new::<u64>(capacity: 4_u64);
  let observed = append_then_remove(values: &values);
  if observed != 0_u64 {
    return 1_u64;
  }
  if values.inner.len != 0_u64 {
    return 2_u64;
  }
  if values.inner.cap != 4_u64 {
    return 3_u64;
  }
  return 0_u64;
}

fn owner_replacement() -> result: u64 pure {
  let values = box_slots_new::<u64>(capacity: 4_u64);
  let observed = append_then_replace(values: &values);
  if observed != 0_u64 {
    return 1_u64;
  }
  if values.inner.len != 0_u64 {
    return 2_u64;
  }
  if values.inner.cap != 4_u64 {
    return 3_u64;
  }
  return 0_u64;
}

fn main() -> status: ExitStatus pure {
  let ordinary = ordinary_and_borrow();
  if ordinary != 0_u64 {
    return exit_status(code: 1_u8);
  }
  let starts = zero_and_nonzero_start();
  if starts != 0_u64 {
    return exit_status(code: 2_u8);
  }
  let growth = relocating_growth();
  if growth != 0_u64 {
    return exit_status(code: 3_u8);
  }
  let exit = early_break();
  if exit != 0_u64 {
    return exit_status(code: 4_u8);
  }
  let reload = ordinary_call_reload();
  if reload != 0_u64 {
    return exit_status(code: 5_u8);
  }
  let replaced = owner_replacement();
  if replaced != 0_u64 {
    return exit_status(code: 6_u8);
  }
  return exit_status(code: 0_u8);
}
"#;

#[test]
fn window_length_is_observable_across_append_growth_and_exits() {
    with_ir(SOURCE.as_bytes(), |program| {
        let helpers = program
            .functions()
            .iter()
            .filter(|function| function.synthesis() == Some(IrSynthesis::ResidentWindow))
            .count();
        let placements = program
            .functions()
            .iter()
            .flat_map(|function| function.blocks())
            .flat_map(|block| block.instructions())
            .filter(|instruction| {
                matches!(
                    instruction,
                    IrInstruction::Define {
                        operation: IrOperation::RunBoundaryResident { .. },
                        ..
                    }
                )
            })
            .count();
        assert!(
            helpers > 0 && placements > 0,
            "the append loop must exercise length residency"
        );
        for name in [
            "append_count",
            "append_observed",
            "append_until_break",
            "append_then_remove",
            "append_then_replace",
        ] {
            assert_selected_region(program, name);
        }
        // The nested-owner function is deliberately uncalled: this assertion
        // checks the conservative selection boundary, not its runtime result.
        assert_ordinary_fallback(program, "append_outer");
        let observer = program
            .functions()
            .iter()
            .find(|function| function.name() == "append_observed")
            .expect("observer loop function");
        assert!(
            observer.blocks().iter().any(|block| {
                block
                    .instructions()
                    .iter()
                    .enumerate()
                    .any(|(position, instruction)| {
                        matches!(instruction, IrInstruction::Define {
                    operation: IrOperation::Call { function, .. }, ..
                } if program.functions()[*function as usize].name() == "inspect_length")
                            && block.instructions()[..position].iter().any(|earlier| {
                                matches!(
                                    earlier,
                                    IrInstruction::Define {
                                        operation: IrOperation::RunLengthCommit { .. },
                                        ..
                                    }
                                )
                            })
                    })
            }),
            "ordinary observer call must follow a header commit"
        );
    });
    for overlap in [
        OverlapLowering::Off,
        OverlapLowering::On,
        OverlapLowering::OnWithCallGrain,
    ] {
        let module = emit_lowered(SOURCE.as_bytes(), overlap);
        let output = compile_link_and_run(&module, None, &[]);
        assert_eq!(output.status.code(), Some(0), "{overlap:?}: {output:?}");
        assert!(output.stderr.is_empty(), "{overlap:?}: {output:?}");
    }
}

fn assert_selected_region(program: &IrProgram, name: &str) {
    let function = program
        .functions()
        .iter()
        .find(|function| function.name() == name)
        .expect("named append loop function");
    let mut private_calls = 0;
    let mut commits = 0;
    for instruction in function
        .blocks()
        .iter()
        .flat_map(|block| block.instructions())
    {
        if let IrInstruction::Define { operation, .. } = instruction {
            match operation {
                IrOperation::Call { function, .. }
                    if program.functions()[*function as usize].synthesis()
                        == Some(IrSynthesis::ResidentWindow) =>
                {
                    private_calls += 1;
                }
                IrOperation::RunLengthCommit { .. } => commits += 1,
                _ => {}
            }
        }
    }
    assert!(
        private_calls > 0 && commits > 0,
        "{name} must use a private append and publish its length"
    );
}

fn assert_ordinary_fallback(program: &IrProgram, name: &str) {
    let function = program
        .functions()
        .iter()
        .find(|function| function.name() == name)
        .expect("named fallback loop function");
    assert!(
        function
            .blocks()
            .iter()
            .flat_map(|block| block.instructions())
            .all(|instruction| {
                !matches!(
                    instruction,
                    IrInstruction::Define {
                        operation: IrOperation::RunBoundaryResident { .. }
                            | IrOperation::RunLengthCommit { .. },
                        ..
                    }
                ) && !matches!(instruction, IrInstruction::Define {
                    operation: IrOperation::Call { function, .. }, ..
                } if program.functions()[*function as usize].synthesis()
                    == Some(IrSynthesis::ResidentWindow))
            }),
        "{name} must retain ordinary window operations"
    );
}

#[test]
fn missing_length_publication_changes_the_observed_result() {
    let module = with_mutated_ir_lowering(SOURCE.as_bytes(), OverlapLowering::Off, |program| {
        assert_selected_region(program, "append_observed");
        let observer = program
            .functions
            .iter_mut()
            .find(|function| function.name() == "append_observed")
            .expect("observer loop function");
        let mut removed = 0;
        for instruction in observer
            .blocks
            .iter_mut()
            .flat_map(|block| &mut block.instructions)
        {
            if let IrInstruction::Define {
                operation: operation @ IrOperation::RunLengthCommit { .. },
                ..
            } = instruction
            {
                *operation = IrOperation::Constant(IrConstant::Unit);
                removed += 1;
            }
        }
        assert!(removed > 0, "mutant must remove a real publication");
        let mut module = emit_llvm(program)
            .expect("mutated program emits")
            .into_string();
        module.push_str(
            &crate::driver::launcher::render(program, "main")
                .expect("ordinary test launcher")
                .render(),
        );
        module
    });
    let output = compile_link_and_run(&module, None, &[]);
    assert_eq!(
        output.status.code(),
        Some(1),
        "missing observer commit must change the result: {output:?}"
    );
}
