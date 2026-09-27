//! Typed-IR tests for ordinary calls, source ownership and proof erasure.

#![allow(clippy::panic)]

use crate::lexer::{LexLimits, LexOutcome, lex};
use crate::{
    ACTIVE_KERNEL_SPEC_HASH, CanonicalLimits, CanonicalOutcome, FinalizeLimits, FinalizeOutcome,
    OverlapLowering, ParseLimits, ParseOutcome, ResolutionOutcome, SemanticOutcome, SourceBundle,
    SourceInput, SourceLimits, TerminalLimits, TerminalOutcome, audit_canonical, check_semantics,
    classify_terminals, finalize, parse, resolve,
};

use super::{
    IrAddressed, IrBlock, IrDrop, IrDropSubject, IrFunction, IrInstruction, IrIntegerOperation,
    IrNominalKind, IrOperation, IrProgram, IrSourceArgument, IrSourceCall, IrSourceMode,
    IrTerminator, IrType, IrValueId, lower_checked,
};

const SOURCE_LIMITS: SourceLimits = SourceLimits {
    max_sources: 1_024,
    max_logical_path_bytes: 128,
    max_source_bytes: 262_144,
    max_total_source_bytes: 524_288,
    max_binding_bytes: 1_048_576,
};

const LEX_LIMITS: LexLimits = LexLimits {
    max_sources: 1_024,
    max_source_bytes: 262_144,
    max_total_source_bytes: 524_288,
    max_token_bytes: 16_384,
    max_tokens: 131_072,
    max_lexemes: 262_144,
};

const PARSE_LIMITS: ParseLimits = ParseLimits {
    max_work: 8_000_000,
    max_tasks: 131_072,
    max_frames: 8_192,
    max_elements: 262_144,
};

const FINALIZE_LIMITS: FinalizeLimits = FinalizeLimits {
    max_work: 8_000_000,
    max_roots: 131_072,
    max_shape_tasks: 131_072,
    max_nodes: 131_072,
    max_child_edges: 131_072,
    max_terminals: 131_072,
    max_sources: 1_024,
};

const CANONICAL_LIMITS: CanonicalLimits = CanonicalLimits {
    max_work: 8_000_000,
    max_source_bytes: 262_144,
    max_total_source_bytes: 524_288,
    max_gaps: 131_072,
    max_path_components: 8_192,
};

/// An ordinary function selected by executable fixtures.
const PLAIN_ENTRY: &str = "fn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n";

#[test]
fn a_split_captures_an_array_payload_but_keeps_owner_and_inline_storage_addressed() {
    let source = br#"nocopy struct Inline {
  values: Array<u8, 16>;
}

fn make_inline() -> result: Inline pure {
  let values = array_filled::<u8, 16>(value: 0_u8);
  let result = Inline(values: values);
  return move result;
}

fn mapped() -> result: Box<Array<u8>> pure {
  let output = box_array_filled::<u8>(count: 16_u64, value: 0_u8);
  let inline = make_inline();
  for @fill (i in 0_u64..16_u64) {
    set output.inner[i] = 1_u8;
    set inline.values[i] = 2_u8;
  }
  return move output;
}

fn main() -> status: std::process::ExitStatus pure {
  let output = mapped();
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_ir_mode(source, OverlapLowering::On, |program| {
        let function = program
            .functions()
            .iter()
            .find(|function| function.name() == "mapped")
            .expect("mapped function");
        let (captures, chunk) = function
            .blocks()
            .iter()
            .flat_map(|block| block.instructions())
            .find_map(|instruction| match instruction {
                IrInstruction::Define {
                    operation:
                        IrOperation::LoopSplit {
                            captures, chunk, ..
                        },
                    ..
                } => Some((captures, *chunk)),
                _ => None,
            })
            .expect("the independent element writes must remain a split loop");

        let mut payload_capture = None;
        let mut inline_addresses = 0;
        for capture in captures {
            match function.value_type(*capture).expect("capture type") {
                IrType::RuntimeBoxPayload { nominal } => {
                    assert!(
                        matches!(
                            program
                                .nominal(nominal)
                                .expect("payload owner nominal")
                                .kind(),
                            IrNominalKind::Box {
                                referent: IrType::Buffer { .. },
                                ..
                            }
                        ),
                        "only a Box<Array<T>> payload may use the internal capture type"
                    );
                    assert!(payload_capture.replace((*capture, nominal)).is_none());
                }
                IrType::Address(IrAddressed::Nominal(nominal))
                    if matches!(
                        program.nominal(nominal).expect("nominal").kind(),
                        IrNominalKind::Struct { .. }
                    ) =>
                {
                    inline_addresses += 1;
                }
                _ => {}
            }
        }
        let (payload, nominal) = payload_capture.expect("one projected Box<Array<T>> capture");
        assert_eq!(
            inline_addresses, 1,
            "an inline nocopy aggregate must retain its addressed representation"
        );

        let forward = function
            .blocks()
            .iter()
            .flat_map(IrBlock::instructions)
            .filter_map(|instruction| match instruction {
                IrInstruction::Define {
                    result,
                    operation:
                        IrOperation::RuntimeBoxPayload {
                            nominal: operation_nominal,
                            owner,
                        },
                    ..
                } if *result == payload => Some((*operation_nominal, *owner)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(forward.len(), 1, "the parent must derive the payload once");
        assert_eq!(forward[0].0, nominal);
        assert_eq!(
            function.value_type(forward[0].1),
            Some(IrType::Nominal(nominal)),
            "the forward projection must read the real source-owned Box value"
        );

        let chunk = &program.functions()[chunk as usize];
        let payload_parameter = chunk
            .parameters()
            .iter()
            .find_map(|(value, ty)| {
                (*ty == IrType::RuntimeBoxPayload { nominal }).then_some(*value)
            })
            .expect("the chunk must take the projected payload in its one-word frame");
        let inverse = chunk
            .blocks()
            .iter()
            .flat_map(IrBlock::instructions)
            .filter_map(|instruction| match instruction {
                IrInstruction::Define {
                    result,
                    operation:
                        IrOperation::RuntimeBoxOwner {
                            nominal: operation_nominal,
                            payload,
                        },
                    ..
                } => Some((*result, *operation_nominal, *payload)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let [inverse] = inverse.as_slice() else {
            panic!("the chunk must reconstruct exactly one local Box value: {inverse:?}");
        };
        assert_eq!(
            (inverse.1, inverse.2),
            (nominal, payload_parameter),
            "the chunk must reconstruct exactly one local Box value from the captured payload"
        );
        assert_eq!(
            chunk.value_type(inverse.0),
            Some(IrType::Nominal(nominal)),
            "the inverse exists only to feed ordinary Box projection lowering"
        );
    });
}

fn with_ir<ResultValue>(source: &[u8], run: impl FnOnce(&IrProgram) -> ResultValue) -> ResultValue {
    with_ir_mode(source, OverlapLowering::Off, run)
}

#[test]
fn runtime_work_keeps_data_dependent_inner_extents_static() {
    let source = br#"fn count_work(upper: u64) -> result: u64 pure {
  let total = 0_u64;
  for (i in 0_u64..upper) {
    set total = total +wrap i;
  }
  return total;
}

fn write_work(input: &[u64], output: &[u64]) -> result: unit reads(input), writes(output) contract {
  requires deref(input).len <= 1024_u64;
  requires deref(output).len >= deref(input).len;
} {
  let count = deref(input).len;
  for (i in 0_u64..count) {
    let upper = deref(input)[i];
    set deref(output)[i] = count_work(upper: upper);
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_ir_mode(source, OverlapLowering::On, |program| {
        let function = program
            .functions()
            .iter()
            .find(|function| function.name() == "write_work")
            .expect("data-dependent consumer");
        let mut splits = 0;
        for instruction in function
            .blocks()
            .iter()
            .flat_map(|block| block.instructions())
        {
            if let IrInstruction::Define {
                operation: IrOperation::LoopSplit { work, .. },
                ..
            } = instruction
            {
                assert!(
                    matches!(work, Some(super::IrWorkEstimate::Constant(value)) if *value > 0),
                    "{work:?}"
                );
                splits += 1;
            }
        }
        assert_eq!(splits, 1);
    });
}

#[test]
fn runtime_helper_extents_reach_the_outer_split_estimate() {
    fn evaluate(work: &super::IrWorkEstimate, extent: u64) -> u64 {
        use super::IrWorkEstimate as Work;
        match work {
            Work::Constant(value) => *value,
            Work::Value(_) | Work::Length(_) | Work::BoxArrayLength(_) => extent,
            Work::Sum(parts) => parts.iter().fold(0_u64, |total, part| {
                total.saturating_add(evaluate(part, extent))
            }),
            Work::Product(left, right) => {
                evaluate(left, extent).saturating_mul(evaluate(right, extent))
            }
            Work::Difference(left, right) => {
                evaluate(left, extent).saturating_sub(evaluate(right, extent))
            }
            Work::Quotient(value, divisor) => evaluate(value, extent) / divisor,
        }
    }
    for (source, name) in [
        (
            include_bytes!("../../../tests/programs/compute/prefix.wf").as_slice(),
            "prefix",
        ),
        (
            include_bytes!("../../../tests/programs/compute/stencil.wf").as_slice(),
            "stencil",
        ),
        (
            include_bytes!("../../../tests/programs/compute/histogram.wf").as_slice(),
            "histogram",
        ),
    ] {
        with_ir_mode(source, OverlapLowering::On, |program| {
            let function = program
                .functions()
                .iter()
                .find(|function| function.name() == name)
                .expect("consumer function");
            let weights: Vec<_> = function
                .blocks()
                .iter()
                .flat_map(|block| block.instructions())
                .filter_map(|instruction| {
                    if let IrInstruction::Define {
                        operation:
                            IrOperation::LoopSplit {
                                work: Some(work), ..
                            },
                        ..
                    } = instruction
                    {
                        Some(work)
                    } else {
                        None
                    }
                })
                .collect();
            assert!(!weights.is_empty(), "{name}: no split");
            assert!(
                weights
                    .iter()
                    .any(|work| evaluate(work, 1024) > evaluate(work, 17).saturating_mul(10)),
                "{name}: helper extent not priced: {weights:?}"
            );
        });
    }
}

/// Each loop really needs all 32 scalars, so capture selection cannot rescue
/// any frame. The source grows linearly with depth; final IR alone would not
/// reveal repeated construction discarded by a refusing ancestor.
fn nested_wide_frame_source(depth: usize) -> String {
    use std::fmt::Write;

    let parameters = (0..32)
        .map(|index| format!("a{index}: u64"))
        .collect::<Vec<_>>()
        .join(", ");
    let mut source = format!(
        "fn folded(source: &Box<Array<u64>>, {parameters}) -> result: u64 reads(source) {{\n"
    );
    for level in 0..depth {
        let indent = "  ".repeat(level + 1);
        writeln!(source, "{indent}let total{level} = 0_u64;").expect("write fixture");
        writeln!(
            source,
            "{indent}for @level{level} (i{level} in 0_u64..2_u64) {{"
        )
        .expect("write fixture");
    }
    let indent = "  ".repeat(depth + 1);
    writeln!(source, "{indent}let bias1 = a0 +wrap a1;").expect("write fixture");
    for index in 2..32 {
        writeln!(
            source,
            "{indent}let bias{index} = bias{} +wrap a{index};",
            index - 1
        )
        .expect("write fixture");
    }
    writeln!(source, "{indent}let extent = deref(source).inner.len;").expect("write fixture");
    writeln!(source, "{indent}let combined = bias31 +wrap extent;").expect("write fixture");
    for level in (0..depth).rev() {
        let indent = "  ".repeat(level + 1);
        let contribution = if level + 1 == depth {
            "combined".to_owned()
        } else {
            format!("total{}", level + 1)
        };
        writeln!(
            source,
            "{indent}  set total{level} = total{level} +wrap {contribution};"
        )
        .expect("write fixture");
        writeln!(source, "{indent}}}").expect("write fixture");
    }
    source
        .push_str("  return total0;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n");
    source.push_str("  let input = box_array_filled::<u64>(count: 1_u64, value: 0_u64);\n");
    let arguments = (0..32)
        .map(|index| format!("a{index}: {index}_u64"))
        .collect::<Vec<_>>()
        .join(", ");
    writeln!(
        source,
        "  let observed = folded(source: &input, {arguments});"
    )
    .expect("write fixture");
    writeln!(source, "  if observed == {}_u64 {{", 497_u64 << depth).expect("write fixture");
    source.push_str("    return std::process::exit_status(code: 0_u8);\n  } else {\n    return std::process::exit_status(code: 1_u8);\n  }\n}\n");
    source
}

#[test]
fn nested_frame_refusals_lower_each_candidate_once() {
    for depth in [1, 2, 4, 6] {
        let source = nested_wide_frame_source(depth);
        with_ir_mode(source.as_bytes(), OverlapLowering::On, |program| {
            assert_eq!(
                program.loop_candidate_constructions, depth,
                "count construction before refusal, including work absent from final IR"
            );
            assert_eq!(
                program
                    .actualization_ledger()
                    .iter()
                    .filter(|row| row.contains("declined:"))
                    .count(),
                depth,
                "every genuinely wide loop must decline once"
            );
            assert!(
                program
                    .functions()
                    .iter()
                    .all(|function| function.synthesis().is_none())
            );
            let function = function(program, "folded");
            assert_eq!(function.counted_ranges.len(), depth);
            assert_eq!(
                function.readonly_reference_parameters,
                [function.parameters[0].0]
            );
            let u64_type = IrType::Integer {
                width: 64,
                signed: false,
            };
            for range in &function.counted_ranges {
                assert!(range.blocks.end <= function.blocks.len());
                assert!(range.blocks.contains(&range.continuation.index()));
                assert_eq!(function.value_type(range.lower), Some(u64_type));
                assert_eq!(function.value_type(range.upper), Some(u64_type));
            }
            crate::emit_llvm(program).expect("the reused ordinary graph must emit");
        });
    }
}

#[test]
fn a_fitting_loop_retains_its_interface_and_one_extra_field_triggers_rescue() {
    for (capture_count, retained) in [(27, 27), (28, 1)] {
        let parameters = (0..capture_count)
            .map(|index| format!("a{index}: u64"))
            .collect::<Vec<_>>()
            .join(", ");
        let arguments = (0..capture_count)
            .map(|index| format!("a{index}: {index}_u64"))
            .collect::<Vec<_>>()
            .join(", ");
        let source = format!(
            "fn folded({parameters}) -> result: u64 pure {{\n  let total = 0_u64;\n  for @items (i in 0_u64..2_u64) {{\n    set total = total +wrap a0;\n  }}\n  return total;\n}}\n\nfn main() -> status: std::process::ExitStatus pure {{\n  let total = folded({arguments});\n  return std::process::exit_status(code: 0_u8);\n}}\n"
        );
        with_ir_mode(source.as_bytes(), OverlapLowering::On, |program| {
            assert_eq!(program.loop_candidate_constructions, 1);
            let source = function(program, "folded");
            let (chunk, captures) = source
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .find_map(|instruction| match instruction {
                    IrInstruction::Define {
                        operation:
                            IrOperation::LoopSplit {
                                chunk, captures, ..
                            },
                        ..
                    } => Some((*chunk, captures)),
                    _ => None,
                })
                .expect("the fitting or rescued loop must split");
            assert_eq!(captures.len(), retained);
            assert_eq!(
                program.functions()[chunk as usize].parameters.len(),
                retained + 3
            );
            // Observe the frame actually requested by this one split through
            // the public emitter, without reaching into target-private layout.
            let module = crate::emit_llvm(program)
                .expect("the fitting or rescued graph must emit")
                .into_string();
            let frame_bytes = module
                .lines()
                .filter_map(|line| line.split_once("call ptr @wf__par_acquire_lane(i64 "))
                .map(|(_, call)| {
                    call.split_once(')')
                        .expect("closed lane-acquisition call")
                        .0
                        .parse::<u64>()
                        .expect("the emitted frame has a constant byte count")
                })
                .collect::<Vec<_>>();
            assert_eq!(frame_bytes, [if capture_count == 27 { 256 } else { 48 }]);
        });
    }
}

/// A live array capture must be measured after pruning. The u64 seed, two
/// bounds, allowance and result occupy 40 bytes; a 216-byte payload fills the
/// lane exactly. One more byte needs alignment padding and makes 264 bytes.
#[test]
fn aggregate_loop_frames_fit_the_selected_target_before_outlining() {
    use crate::backend::emitter::emit_llvm_with_layout;
    use crate::target::TargetLayout;

    for target in [
        TargetLayout::host().expect("supported test target"),
        TargetLayout::for_triple("x86_64-pc-windows-msvc").expect("supported selected target"),
    ] {
        for (length, expected_bytes) in [(1, Some(48)), (216, Some(256)), (217, None)] {
            let source = format!(
                "fn folded(values: Array<u8, {length}>) -> result: u64 pure {{\n  let total = 7_u64;\n  for (i in 0_u64..2_u64) {{\n    let copied = values;\n    let byte = copied[0_u64];\n    let word = cvt::<u8, u64>(byte);\n    set total = total +wrap word;\n  }}\n  return total;\n}}\n\nfn main() -> status: std::process::ExitStatus pure {{\n  let values = array_filled::<u8, {length}>(value: 19_u8);\n  let total = folded(values: values);\n  return std::process::exit_status(code: 0_u8);\n}}\n"
            );
            with_checked(source.as_bytes(), |checked| {
                let program =
                    super::lower_checked_with_layout(checked, OverlapLowering::On, target)
                        .expect("array capture lowering must fit or reuse its ordinary body");
                assert_eq!(program.loop_candidate_constructions, 1);
                let parent = function(&program, "folded");
                let split = parent
                    .blocks()
                    .iter()
                    .flat_map(|block| block.instructions())
                    .find_map(|instruction| match instruction {
                        IrInstruction::Define {
                            operation:
                                IrOperation::LoopSplit {
                                    splitter, captures, ..
                                },
                            ..
                        } => Some((*splitter, captures)),
                        _ => None,
                    });
                assert_eq!(split.is_some(), expected_bytes.is_some());
                if let Some((_, captures)) = split {
                    assert_eq!(captures.len(), 1);
                    assert!(
                        matches!(parent.value_type(captures[0]), Some(IrType::Array { length: n, .. }) if n == length)
                    );
                } else {
                    assert_eq!(
                        parent.counted_ranges.len(),
                        1,
                        "the ordinary loop stays at its source site"
                    );
                    assert!(
                        program
                            .functions()
                            .iter()
                            .all(|function| function.synthesis().is_none())
                    );
                    assert!(
                        program
                            .actualization_ledger()
                            .iter()
                            .any(|row| row.contains("declined:")
                                && row.contains("conservative estimate"))
                    );
                }
                let module = emit_llvm_with_layout(&program, target)
                    .expect("lowering and emission must use the same target")
                    .into_string();
                let frames = module
                    .lines()
                    .filter_map(|line| line.split_once("call ptr @wf__par_acquire_lane(i64 "))
                    .map(|(_, tail)| {
                        tail.split_once(')')
                            .expect("closed frame-size argument")
                            .0
                            .parse::<u64>()
                            .expect("constant frame size")
                    })
                    .collect::<Vec<_>>();
                assert_eq!(frames, expected_bytes.into_iter().collect::<Vec<_>>());
            });
        }
    }
}

fn with_ir_mode<ResultValue>(
    source: &[u8],
    overlap: OverlapLowering,
    run: impl FnOnce(&IrProgram) -> ResultValue,
) -> ResultValue {
    with_checked(source, |checked| {
        let ir = lower_checked(checked, overlap).expect("checked system program must lower");
        for function in ir.functions() {
            for source in &function.source_calls {
                let (target, arguments) = call_definition(function, source.result);
                let signature = ir.functions()[target as usize]
                    .source_signature
                    .as_ref()
                    .expect("a source call retains a source callee");
                assert_eq!(source.arguments.len(), arguments.len());
                assert_eq!(signature.parameters.len(), arguments.len());
                if let Some(argument) = source.returned_borrow_argument {
                    assert!(argument < arguments.len());
                    assert_ne!(signature.parameters[argument], IrSourceMode::Own);
                    assert_ne!(signature.result, IrSourceMode::Own);
                }
            }
        }
        run(&ir)
    })
}

fn with_checked<ResultValue>(
    source: &[u8],
    run: impl FnOnce(crate::semantic::CheckedProgram) -> ResultValue,
) -> ResultValue {
    let inputs = [SourceInput::new("test.wf", source)];
    let Ok(bundle) = SourceBundle::with_prelude(&inputs, SOURCE_LIMITS) else {
        panic!("lowering test bundle must be valid");
    };
    let LexOutcome::Complete(lexed) = lex(&bundle, LEX_LIMITS) else {
        panic!("lowering test source must lex");
    };
    let TerminalOutcome::Complete(classified) = classify_terminals(
        &lexed,
        ACTIVE_KERNEL_SPEC_HASH,
        TerminalLimits {
            max_tokens: LEX_LIMITS.max_tokens,
        },
    ) else {
        panic!("lowering test source must classify");
    };
    let ParseOutcome::Complete(parsed) = parse(classified, PARSE_LIMITS) else {
        panic!("lowering test source must parse");
    };
    let FinalizeOutcome::Complete(finalized) = finalize(parsed, FINALIZE_LIMITS) else {
        panic!("lowering test derivation must finalize");
    };
    let CanonicalOutcome::Complete(canonical) = audit_canonical(*finalized, CANONICAL_LIMITS)
    else {
        panic!("lowering test source must be canonical");
    };
    let ResolutionOutcome::Complete(resolved) = resolve(canonical) else {
        panic!("lowering test source must resolve");
    };
    let outcome = check_semantics(&resolved);
    let SemanticOutcome::Complete(checked) = outcome else {
        panic!("lowering test source must check: {outcome:?}");
    };
    run(*checked)
}

fn call_definition(function: &IrFunction, result: IrValueId) -> (u32, &[IrValueId]) {
    function
        .blocks()
        .iter()
        .flat_map(IrBlock::instructions)
        .find_map(|instruction| match instruction {
            IrInstruction::Define {
                result: defined,
                operation:
                    IrOperation::Call {
                        function,
                        arguments,
                    },
                ..
            } if *defined == result => Some((*function, arguments.as_slice())),
            _ => None,
        })
        .expect("source metadata must name an actual IR call")
}

/// Retired subject: physical release specialization over the store axis, which
/// the two `physical_call_inventory_*` tests drove with `Heap<'s>`, `region`
/// blocks, `arena_frame` and `Box<'s, T>`; v0.60 has one heap [STOR-8], no
/// region and no store parameter, so a `Box` has exactly one release and the
/// axis those tests measured no longer exists. Successor: this test, which
/// states the consequence -- one physical variant per source function, with
/// the calls of each variant closed inside the inventory.
#[test]
fn physical_call_inventory_gives_one_variant_per_function_under_one_heap() {
    let source = br#"fn pass<T>(value: T) -> result: T pure {
  return move value;
}

fn observe(cell: &Box<u64>, witness: &Box<u64>) -> result: unit pure {
  return unit;
}

fn relay(cell: Box<u64>) -> result: Box<u64> pure {
  return pass::<Box<u64>>(value: move cell);
}

fn main() -> status: std::process::ExitStatus pure {
  let first = box_new::<u64>(value: 1_u64);
  let second = box_new::<u64>(value: 2_u64);
  let ready = relay(cell: move first);
  observe(cell: &ready, witness: &second);
  observe(cell: &second, witness: &ready);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_checked(source, |checked| {
        let plan = super::specialize::PhysicalFunctions::build(&checked.data)
            .expect("accepted call inventory must close");
        for function in &checked.data.functions {
            let variants = plan
                .variants
                .iter()
                .filter(|variant| variant.source == function.id)
                .count();
            assert_eq!(
                variants, 1,
                "{}: one heap leaves one release environment, and every source \
                 definition is still emitted",
                function.name
            );
        }
        for variant in &plan.variants {
            assert!(
                variant
                    .calls
                    .iter()
                    .all(|(_, target)| (*target as usize) < plan.variants.len()),
                "every call names a variant of this inventory"
            );
        }
        assert!(
            plan.variants
                .windows(2)
                .all(|pair| pair[0].source.0 <= pair[1].source.0)
        );
    });
}

/// Retired subject: `physical_call_inventory_closes_captured_regions_and_recursive_edges`,
/// whose first half measured two release classes over the store axis. That
/// axis is gone with regions and store parameters [STOR-8]. Its second half is
/// not: a recursive source function's own call still has to name a variant of
/// this inventory, and under one heap that variant is the caller's own, so the
/// edge closes on itself. A `Box` release is "its content's compiler-derived
/// release followed by one compiler-derived heap free" [STOR-3] with nothing
/// left for a second environment to vary.
#[test]
fn physical_call_inventory_closes_a_recursive_edge_on_its_own_variant() {
    let source = br#"fn descend(cell: &Box<u64>, again: Bool) -> result: unit reads(cell) {
  if again {
    let stop = False();
    descend(cell: cell, again: stop);
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let held = box_new::<u64>(value: 1_u64);
  let start = True();
  descend(cell: &held, again: start);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_checked(source, |checked| {
        let plan = super::specialize::PhysicalFunctions::build(&checked.data)
            .expect("accepted recursive call inventory must close");
        let recursive = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "descend")
            .expect("recursive source declaration")
            .id;
        let variants = plan
            .variants
            .iter()
            .enumerate()
            .filter(|(_, variant)| variant.source == recursive)
            .collect::<Vec<_>>();
        let [(index, variant)] = variants.as_slice() else {
            panic!("one heap leaves one variant of a recursive function: {variants:?}");
        };
        let [(_, target)] = variant.calls.as_slice() else {
            panic!("the body's one call must be retained: {:?}", variant.calls);
        };
        assert_eq!(
            *target as usize, *index,
            "the recursive call names the caller's own variant"
        );
        for variant in &plan.variants {
            assert!(
                variant
                    .calls
                    .iter()
                    .all(|(_, target)| (*target as usize) < plan.variants.len()),
                "every call names a variant of this inventory"
            );
        }
    });
}

fn source_call<'program>(
    program: &'program IrProgram,
    caller: &str,
    callee: &str,
) -> (&'program IrSourceCall, &'program [IrValueId]) {
    let caller = function(program, caller);
    caller
        .source_calls
        .iter()
        .find_map(|source| {
            let (target, arguments) = call_definition(caller, source.result);
            (program.functions()[target as usize].name() == callee).then_some((source, arguments))
        })
        .expect("the source call must have retained use metadata")
}

fn function<'program>(program: &'program IrProgram, name: &str) -> &'program IrFunction {
    program
        .functions()
        .iter()
        .find(|function| function.name() == name)
        .unwrap_or_else(|| panic!("lowered program must contain {name}"))
}

/// The one block of a straight-line function.
fn only_block(function: &IrFunction) -> &IrBlock {
    let [block] = function.blocks() else {
        panic!("expected one block, got {}", function.blocks().len());
    };
    block
}

fn return_drops(function: &IrFunction) -> &[IrDrop] {
    let IrTerminator::Return { drops, .. } = only_block(function).terminator() else {
        panic!("expected a return terminator");
    };
    drops
}

/// Retired subject: `&uniq` against `&` over one descriptor representation,
/// which v0.60 removed with the second reference kind [REF-1]; successor:
/// this test over the three modes v0.60 does have.
#[test]
fn source_signature_modes_distinguish_the_three_parameter_modes() {
    let source = format!(
        "fn owned(value: Box<u64>) -> result: unit pure {{\n  return unit;\n}}\n\nfn referenced(value: &Box<u64>) -> result: unit pure {{\n  return unit;\n}}\n\nfn ranged(value: &[u8]) -> result: unit pure {{\n  return unit;\n}}\n\n{PLAIN_ENTRY}"
    );
    with_ir(source.as_bytes(), |program| {
        for (name, mode) in [
            ("owned", IrSourceMode::Own),
            ("referenced", IrSourceMode::Reference),
            ("ranged", IrSourceMode::Range),
        ] {
            let lowered = function(program, name);
            let signature = lowered
                .source_signature
                .as_ref()
                .expect("a source signature");
            assert_eq!(signature.parameters, [mode]);
            assert_eq!(signature.result, IrSourceMode::Own);
            assert_eq!(
                return_drops(lowered).len(),
                usize::from(mode == IrSourceMode::Own),
                "only an owned parameter carries a compiler-derived release"
            );
        }
    });
}

/// Retired subject: a borrow-mode *result*, which [REF-3] removed when it
/// forbade returning a reference; successor: this test, which states that the
/// declared result mode of every v0.60 function is `own`.
#[test]
fn source_signature_results_are_owned_because_no_reference_escapes() {
    let source = format!(
        "fn owned(value: u64) -> result: u64 pure {{\n  return value;\n}}\n\nfn read(value: &u64) -> result: u64 reads(value) {{\n  return deref(value);\n}}\n\n{PLAIN_ENTRY}"
    );
    with_ir(source.as_bytes(), |program| {
        for (name, mode) in [
            ("owned", IrSourceMode::Own),
            ("read", IrSourceMode::Reference),
        ] {
            let lowered = function(program, name);
            let signature = lowered
                .source_signature
                .as_ref()
                .expect("a source signature");
            assert_eq!(signature.parameters, [mode]);
            assert_eq!(signature.result, IrSourceMode::Own);
            assert!(!matches!(lowered.result(), IrType::Address(_)));
        }
    });
}

#[test]
fn source_signature_modes_are_not_invented_for_synthesized_functions() {
    let source = format!(
        "fn folded(lo: u64, hi: u64) -> result: u64 pure {{\n  let total = 0_u64;\n  for @points (i in lo..hi) {{\n    set total = total +wrap i;\n  }}\n  return total;\n}}\n\n{PLAIN_ENTRY}"
    );
    with_ir_mode(source.as_bytes(), OverlapLowering::On, |program| {
        let generated = program
            .functions()
            .iter()
            .filter(|function| function.synthesis().is_some())
            .collect::<Vec<_>>();
        assert!(
            !generated.is_empty(),
            "the reduction must exercise synthesized signatures: {:?}",
            program.actualization_ledger()
        );
        for function in generated {
            assert_eq!(function.source_signature, None);
        }
        let source = function(program, "folded");
        assert_eq!(
            source
                .source_signature
                .as_ref()
                .expect("a source signature")
                .parameters,
            [IrSourceMode::Own, IrSourceMode::Own]
        );
    });
}

/// The subject is the use record each source call retains: a borrow and a
/// consume of one binding are distinguished by their argument kinds.
/// Whether the two calls receive the same IR operand is the lowering's own
/// representation choice (a reference argument is the address of the owner's
/// slot, a consumed Box is the pointer loaded from it) and is pinned by no
/// rule and no design decision, so this test does not assert it (owner,
/// 2026-09-20: the specification does not govern the IR).
#[test]
fn source_call_uses_distinguish_borrow_and_consume_of_one_binding() {
    let source = format!(
        "fn inspect(value: &Box<Array<u8>>) -> result: u64 reads(value) {{\n  return deref(value).inner.len;\n}}\n\nfn consume(value: Box<Array<u8>>) -> result: u64 pure {{\n  return value.inner.len;\n}}\n\nfn run() -> result: u64 pure {{\n  let data = box_array_filled::<u8>(count: 2_u64, value: 7_u8);\n  let before = inspect(value: &data);\n  let after = consume(value: move data);\n  return after;\n}}\n\n{PLAIN_ENTRY}"
    );
    with_ir(source.as_bytes(), |program| {
        let (borrow, _borrowed_values) = source_call(program, "run", "inspect");
        let (consume, _consumed_values) = source_call(program, "run", "consume");
        assert_ne!(borrow.result, consume.result);
        assert_eq!(borrow.arguments, [IrSourceArgument::Borrow]);
        assert_eq!(
            consume.arguments,
            [IrSourceArgument::Binding { consume_root: true }]
        );
    });
}

// Retired test: `source_call_uses_keep_unique_holder_transfer_distinct_from_owning_storage`,
// whose subject was the transfer of a `&uniq` holder out of a callee; v0.60
// has one reference kind [REF-1] and returns none of them [REF-3], and its
// successor is `source_signature_results_are_owned_because_no_reference_escapes`.

#[test]
fn source_call_uses_retain_projected_root_consumption() {
    let source = format!(
        "struct Packet {{\n  first: Box<u64>;\n  second: Box<u64>;\n}}\n\nfn consume(value: Box<u64>) -> result: unit pure {{\n  return unit;\n}}\n\nfn run() -> result: unit pure {{\n  let first = box_new::<u64>(value: 3_u64);\n  let second = box_new::<u64>(value: 5_u64);\n  let packet = Packet(first: move first, second: move second);\n  return consume(value: move packet.first);\n}}\n\n{PLAIN_ENTRY}"
    );
    with_ir(source.as_bytes(), |program| {
        let (call, _) = source_call(program, "run", "consume");
        assert_eq!(
            call.arguments,
            [IrSourceArgument::Projection { consume_root: true }]
        );
        assert_eq!(call.returned_borrow_argument, None);
    });
}

/// Retired subject: the returned-borrow candidate of a call, which [REF-3]
/// removed when it forbade returning a reference; successor: this test, which
/// keeps the indexed-borrow *argument* half of the same question.
#[test]
fn source_call_uses_retain_the_actual_indexed_borrow_candidate() {
    let source = br#"struct Row {
  value: u64;
}

fn select(stamp: u64, value: &Row) -> result: u64 reads(value) {
  return deref(value).value;
}

fn main() -> status: std::process::ExitStatus pure {
  let rows = slots_new::<Row, 2>();
  let first = Row(value: 3_u64);
  place_back(window: &rows, value: first);
  let second = Row(value: 5_u64);
  place_back(window: &rows, value: second);
  let observed = select(stamp: 7_u64, value: &rows[1_u64]);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_ir(source, |program| {
        let (call, arguments) = source_call(program, "main", "select");
        assert_eq!(call.returned_borrow_argument, None);
        assert_eq!(
            call.arguments,
            [IrSourceArgument::Value, IrSourceArgument::Borrow]
        );
        assert!(
            function(program, "main")
                .blocks()
                .iter()
                .flat_map(IrBlock::instructions)
                .any(|instruction| matches!(
                    instruction,
                    IrInstruction::Define {
                        result,
                        operation: IrOperation::ProjectAddress {
                            projection: super::IrPlaceStep::RunElement { .. },
                            ..
                        },
                        ..
                    } if *result == arguments[1]
                ))
        );
    });
}

#[test]
fn counted_range_cfg_emits_with_distinct_header_update_and_exit_interfaces() {
    let source = br#"fn count() -> result: u64 pure {
  let total = 0_u64;
  for @items (i in 18446744073709551614_u64..18446744073709551615_u64) {
    set total = i;
  }
  return total;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_ir(source, |program| {
        let counted = function(program, "count");
        if let Err(error) = crate::emit_llvm(program) {
            panic!("counted IR must emit: {error:?}\n{counted:#?}");
        }
        assert!(counted.blocks().len() >= 6);
        assert!(counted.blocks().iter().any(|block| {
            matches!(
                block.terminator(),
                IrTerminator::Match {
                    enum_type: super::IrEnumType::Bool,
                    ..
                }
            )
        }));
        let hidden_updates = counted
            .blocks()
            .iter()
            .flat_map(IrBlock::instructions)
            .filter(|instruction| {
                matches!(
                    instruction,
                    IrInstruction::Define {
                        operation: IrOperation::Integer {
                            operation: IrIntegerOperation::AddWrap,
                            operand_type: IrType::Integer {
                                width: 64,
                                signed: false,
                            },
                            ..
                        },
                        ..
                    }
                )
            })
            .count();
        assert_eq!(
            hidden_updates, 1,
            "MAX-1..MAX must retain exactly the compiler-owned unit update"
        );
    });
}

#[test]
fn counted_break_and_return_edges_do_not_enter_the_hidden_update() {
    let source = br#"fn leave_by_break(stop: Bool) -> result: u64 pure {
  for @scan (i in 0_u64..2_u64) {
    if stop {
      break @scan;
    }
  }
  return 7_u64;
}

fn leave_by_return(stop: Bool) -> result: u64 pure {
  for @scan (i in 0_u64..2_u64) {
    if stop {
      return 9_u64;
    }
  }
  return 7_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_ir(source, |program| {
        let hidden_update_count = |function: &IrFunction| {
            function
                .blocks()
                .iter()
                .flat_map(IrBlock::instructions)
                .filter(|instruction| {
                    matches!(
                        instruction,
                        IrInstruction::Define {
                            operation: IrOperation::Integer {
                                operation: IrIntegerOperation::AddWrap,
                                operand_type: IrType::Integer {
                                    width: 64,
                                    signed: false,
                                },
                                ..
                            },
                            ..
                        }
                    )
                })
                .count()
        };

        let breaking = function(program, "leave_by_break");
        assert_eq!(
            hidden_update_count(breaking),
            1,
            "the normal fallthrough keeps one hidden update"
        );
        let exit_blocks = breaking
            .blocks()
            .iter()
            .enumerate()
            .filter_map(|(index, block)| {
                matches!(block.terminator(), IrTerminator::Return { .. }).then_some(index)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            exit_blocks.len(),
            1,
            "the break fixture has one final return"
        );
        let exit = exit_blocks[0];
        let jumps_to_exit = breaking
            .blocks()
            .iter()
            .filter(|block| {
                matches!(
                    block.terminator(),
                    IrTerminator::Jump { target, .. } if target.index() == exit
                )
            })
            .count();
        assert_eq!(
            jumps_to_exit, 2,
            "the false header and break edge must reach the exit directly"
        );

        let returning = function(program, "leave_by_return");
        assert_eq!(
            hidden_update_count(returning),
            1,
            "the non-returning branch keeps one hidden update"
        );
        let returns = returning
            .blocks()
            .iter()
            .filter(|block| matches!(block.terminator(), IrTerminator::Return { .. }))
            .count();
        assert_eq!(
            returns, 2,
            "the body return must remain a return edge beside the false-header exit"
        );
    });
}

#[test]
fn counted_range_carries_one_stable_binder_address_for_body_local_shared_borrows() {
    let source = br#"fn count() -> result: u64 pure {
  let total = 0_u64;
  let upper = 2_u64;
  for @items (i in 0_u64..upper) {
    let held = &i;
    let seen = deref(held);
    set total = total +wrap seen;
    set upper = 0_u64;
  }
  return total;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_ir(source, |program| {
        let counted = function(program, "count");
        if let Err(error) = crate::emit_llvm(program) {
            panic!("addressed counted binder IR must emit: {error:?}\n{counted:#?}");
        }
        let address_count = counted
            .blocks()
            .iter()
            .flat_map(IrBlock::instructions)
            .filter(|instruction| {
                matches!(
                    instruction,
                    IrInstruction::Define {
                        operation: IrOperation::AddressOf { .. },
                        ..
                    }
                )
            })
            .count();
        assert_eq!(address_count, 1, "the binder storage is allocated once");
    });
}

#[test]
fn nested_counted_breaks_keep_each_exit_interface_local_to_its_range() {
    let source = br#"fn count() -> result: u64 pure {
  let total = 0_u64;
  for @outer (i in 0_u64..4_u64) {
    for @inner (j in 0_u64..4_u64) {
      set total = total +wrap 1_u64;
      break @inner;
    }
    if i == 1_u64 {
      break @outer;
    }
  }
  return total;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_ir(source, |program| {
        let counted = function(program, "count");
        if let Err(error) = crate::emit_llvm(program) {
            panic!("nested counted IR must emit: {error:?}\n{counted:#?}");
        }
    });
}

/// Retired subject: the `dispose` statement's explicit release edge, which
/// v0.60 has no spelling for -- an affine value is released early by moving it
/// into a function that consumes it [PROV-6]; successor: this test, which
/// keeps the addressed-cleanup half, that a release names its place instead of
/// loading a second whole-owner snapshot.
#[test]
fn addressed_cleanup_keeps_places_instead_of_whole_owner_snapshots() {
    let source = format!(
        r#"struct Holder {{
  bytes: Box<u64>;
  stamp: u64;
}}

fn touch(value: &Holder) -> result: unit writes(value.stamp) {{
  set deref(value).stamp = 41_u64;
  return unit;
}}

fn release_holder(value: Holder) -> result: unit pure {{
  touch(value: &value);
  return unit;
}}

{PLAIN_ENTRY}"#
    );
    with_ir(source.as_bytes(), |program| {
        let function = function(program, "release_holder");
        let mut groups = Vec::new();
        for block in function.blocks() {
            for instruction in block.instructions() {
                assert!(
                    !matches!(
                        instruction,
                        IrInstruction::Define {
                            operation: IrOperation::Load {
                                referent: super::IrAddressed::Nominal(_),
                                ..
                            },
                            ..
                        }
                    ),
                    "cleanup must not capture a second aggregate owner"
                );
                if let IrInstruction::Drops(drops) = instruction {
                    groups.push(drops.as_slice());
                }
            }
            if let IrTerminator::Return { drops, .. } = block.terminator()
                && !drops.is_empty()
            {
                groups.push(drops.as_slice());
            }
        }
        assert_eq!(groups.len(), 1, "one scope exit, one release group");
        for drops in groups {
            let [field, owner] = drops else {
                panic!("field release then owner node, got {drops:?}");
            };
            // The cell is the field that derives release work; the owner node
            // is the struct the walk runs over [PROV-6].
            assert!(matches!(field.ty(), IrType::Nominal(_)));
            assert!(matches!(owner.ty(), IrType::Nominal(_)));
            assert_ne!(field.ty(), owner.ty());
            for drop in drops {
                let IrDropSubject::Place(address) = drop.subject() else {
                    panic!("an addressed owner keeps its cleanup place");
                };
                let Some(IrType::Address(referent)) = function.value_type(address) else {
                    panic!("cleanup place must retain its content type");
                };
                assert_eq!(referent.ty(), drop.ty());
            }
        }
    });
}

#[test]
fn an_unused_state_writing_call_reaches_ir() {
    let source = format!(
        "struct Pair {{\n  left: u64;\n}}\n\n\
         fn mutate(pair: &Pair) -> result: unit writes(pair.left) {{\n  \
         set deref(pair).left = 1_u64;\n  return unit;\n}}\n\n\
         fn wrapper(pair: &Pair) -> result: unit writes(pair.left) {{\n  \
         mutate(pair: pair);\n  return unit;\n}}\n\n\
         {PLAIN_ENTRY}"
    );
    with_ir(source.as_bytes(), |program| {
        let wrapper = function(program, "wrapper");
        assert!(wrapper.blocks().iter().any(|block| {
            block.instructions().iter().any(|instruction| {
                matches!(
                    instruction,
                    IrInstruction::Define {
                        operation: IrOperation::Call { function: 0, .. },
                        ..
                    }
                )
            })
        }));
    });
}

#[test]
fn ordinary_requires_is_not_lowered_as_a_callee_prologue() {
    let source = br#"fn bounded(value: u64) -> result: u64 pure contract {
  requires value < 8_u64;
} {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  let value = 4_u64;
  let result = bounded(value: value);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_ir(source, |program| {
        let bounded = function(program, "bounded");
        assert!(
            bounded
                .blocks()
                .iter()
                .flat_map(IrBlock::instructions)
                .next()
                .is_none(),
            "an ordinary requirement is a call-site obligation and contributes no executable callee prologue"
        );
    });
}

#[test]
fn source_proof_is_erased_before_typed_ir() {
    let source = br#"fn plain(left: u64, left_limit: u64, middle: u64, middle_limit: u64, right: u64, right_limit: u64) -> result: unit pure contract {
  requires left <= left_limit;
  requires middle <= middle_limit;
  requires right <= right_limit;
} {
  return unit;
}

fn prove_only(left: u64, left_limit: u64, middle: u64, middle_limit: u64, right: u64, right_limit: u64) -> result: unit pure contract {
  requires left <= left_limit;
  requires middle <= middle_limit;
  requires right <= right_limit;
} {
  invariant combined: left + middle + right <= left_limit + middle_limit + right_limit {
    use (left <= left_limit);
    use (middle <= middle_limit);
    use (right <= right_limit);
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_ir(source, |program| {
        let plain = function(program, "plain");
        let proved = function(program, "prove_only");
        assert_eq!(
            proved.parameters(),
            plain.parameters(),
            "the erased proof contributes no parameter or value dependency"
        );
        assert_eq!(proved.result(), plain.result());
        assert_eq!(
            proved.blocks(),
            plain.blocks(),
            "PRF-1 contributes no instruction, effect, branch, runtime check, or terminator change"
        );
        assert_eq!(proved.overlaps(), plain.overlaps());
    });
}

/// The schema and the shared constructor body must retain the same OP-9
/// pair, including empty repeated pairs and a fixed type deeper than the
/// former lowering-only limit of 64. Expected sizes come from OP-9's
/// sequence rule, independently of either implementation.
#[test]
fn stored_layout_ceilings_agree_across_lowering() {
    let mut declarations =
        "struct Giant {\n  words: Array<u64, 2305843009213693952>;\n}\n\n".to_owned();
    declarations.push_str("struct Layer0 {\n  value: u64;\n}\n\n");
    for depth in 1..=66 {
        declarations.push_str(&format!(
            "struct Layer{depth} {{\n  value: Layer{};\n}}\n\n",
            depth - 1
        ));
    }
    for (stored, size, align) in [
        ("Array<Array<u64, 0>, 4>", 0_u64, 1_u64),
        ("Slots<Array<u64, 0>, 4>", 8, 8),
        ("Ring<Array<u64, 0>, 4>", 16, 8),
        ("Array<Giant, 0>", 0, 1),
        ("Slots<Giant, 0>", 8, 8),
        ("Ring<Giant, 0>", 16, 8),
        ("Layer66", 8, 8),
    ] {
        let source = format!(
            "{declarations}fn main() -> status: std::process::ExitStatus pure {{\n  let cells = box_slots_new::<{stored}>(capacity: 0_u64);\n  free_empty(window: move cells);\n  return std::process::exit_status(code: 0_u8);\n}}\n"
        );
        with_ir(source.as_bytes(), |program| {
            let expected = super::IrLayoutCeiling {
                size: super::IrLayoutMagnitude::Finite(size),
                align,
                stride: super::IrLayoutMagnitude::Finite(size.max(1)),
            };
            let source_ceilings = program
                .functions()
                .iter()
                .flat_map(IrFunction::source_calls)
                .filter_map(|call| {
                    call.allocation()
                        .map(|allocation| allocation.layout_ceiling())
                })
                .collect::<Vec<_>>();
            let body_ceilings = program
                .functions()
                .iter()
                .flat_map(IrFunction::blocks)
                .flat_map(IrBlock::instructions)
                .filter_map(|instruction| match instruction {
                    IrInstruction::Define {
                        operation: IrOperation::WindowBlockNew { obligations, .. },
                        ..
                    } => Some(obligations.layout_ceiling),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(source_ceilings, vec![expected], "checked {stored}");
            assert_eq!(body_ceilings, vec![expected], "lowered {stored}");
        });
    }
}

/// [STOR-6]: "The accepted [OP-9] judgment retains a numeric upper bound for
/// the source length **at that allocation site**; target qualification
/// multiplies that bound by the actual target stride ... before lowering the
/// operation." The bound is a property of the site, not of the construction
/// row, so two callers with two different proved ceilings own two different
/// bounds and neither may be read from the other. [OP-9] says the same from
/// the other side: "Each runtime-capacity construction [OP-13] and `grow`
/// [OP-10] carries it over that operation's own stored type and count."
///
/// The second caller is what makes that falsifiable. With one caller a shared
/// row body carrying that caller's bound is indistinguishable from a
/// per-site bound; with two, a shared body can carry at most one of 1000 and
/// 7, and the grouping below names the function each retained bound was found
/// in, so the failure says where the bound actually landed.
#[test]
fn buffer_allocations_lower_the_source_proved_length_ceiling_into_target_obligations() {
    let source = br#"fn allocate(n: u64) -> result: unit pure contract {
  requires n <= 1000_u64;
} {
  let packed = box_array_filled::<u16>(count: n, value: 7_u16);
  let vacant = box_slots_new::<u16>(capacity: n);
  return unit;
}

fn small(n: u64) -> result: unit pure contract {
  requires n <= 7_u64;
} {
  let packed = box_array_filled::<u16>(count: n, value: 7_u16);
  let vacant = box_slots_new::<u16>(capacity: n);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  allocate(n: 4_u64);
  small(n: 3_u64);
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_ir(source, |program| {
        let mut sites: Vec<(&str, Vec<u64>)> = Vec::new();
        let mut shared_allocators = Vec::new();
        for function in program.functions() {
            // Keep inspecting the actual allocation instructions. With the
            // ordinary PRE-1 ABI they reside in one shared body per type,
            // which cannot carry both callers' different bounds.
            let shared = function
                .blocks()
                .iter()
                .flat_map(IrBlock::instructions)
                .filter_map(|instruction| {
                    let IrInstruction::Define { operation, .. } = instruction else {
                        return None;
                    };
                    match operation {
                        IrOperation::BufferFill { target_domains, .. } => Some(*target_domains),
                        IrOperation::WindowBlockNew { obligations, .. } => {
                            Some(obligations.target_domains)
                        }
                        _ => None,
                    }
                })
                .collect::<Vec<_>>();
            for domains in shared {
                assert!(!domains.has_call_site_bound());
                shared_allocators.push(function.name());
            }
            // These are lowered IR call records, attached to the result of
            // the actual Call instruction. Target qualification consumes
            // them; backend arrays tests separately pin each shape's exact
            // byte boundary and its one-byte-short rejection.
            let bounds = function
                .source_calls()
                .iter()
                .filter_map(|call| {
                    let allocation = call.allocation()?;
                    let instruction = function
                        .blocks()
                        .iter()
                        .flat_map(IrBlock::instructions)
                        .find(|instruction| {
                            matches!(instruction,
                                IrInstruction::Define { result, .. } if *result == call.result()
                            )
                        })
                        .expect("an allocation bound names an emitted IR definition");
                    let IrInstruction::Define {
                        operation:
                            IrOperation::Call {
                                function: callee,
                                arguments,
                            },
                        ..
                    } = instruction
                    else {
                        panic!("an allocation bound must belong to an ordinary Call");
                    };
                    assert!(allocation.count_argument() < arguments.len());
                    let callee = &program.functions()[*callee as usize];
                    assert!(
                        callee.name().starts_with("box_array_filled")
                            || callee.name().starts_with("box_slots_new")
                    );
                    Some(allocation.source_length_upper_bound())
                })
                .collect::<Vec<_>>();
            if !bounds.is_empty() {
                sites.push((function.name(), bounds));
            }
        }
        assert_eq!(
            sites,
            vec![("allocate", vec![1000, 1000]), ("small", vec![7, 7])],
            "each allocation site keeps its own caller's proved ceiling"
        );
        assert_eq!(shared_allocators.len(), 2, "one body per constructor type");
        for constructor in ["box_array_filled", "box_slots_new"] {
            assert_eq!(
                shared_allocators
                    .iter()
                    .filter(|name| name.starts_with(constructor))
                    .count(),
                1,
                "site bounds do not clone the ordinary prelude body"
            );
        }
    });
}

#[test]
fn an_uninhabited_function_keeps_its_abi_and_lowers_to_one_unreachable_block() {
    let source = br#"fn impossible(value: i32) -> out: i32 pure contract {
  requires value == 0_i32;
  requires value != 0_i32;
} {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_ir(source, |program| {
        let impossible = function(program, "impossible");
        assert_eq!(impossible.parameters().len(), 1);
        assert_eq!(
            impossible.result(),
            IrType::Integer {
                width: 32,
                signed: true,
            }
        );
        let [entry] = impossible.blocks() else {
            panic!("an uninhabited function must lower to exactly one block");
        };
        assert!(entry.instructions().is_empty());
        assert_eq!(entry.terminator(), &IrTerminator::Unreachable);
    });
}

#[test]
fn physical_call_inventory_omits_proof_closed_body_edges() {
    let source = br#"fn child() -> result: unit pure {
  return unit;
}

fn impossible(value: i32) -> result: unit pure contract {
  requires value == 0_i32;
  requires value != 0_i32;
} {
  child();
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_checked(source, |checked| {
        let impossible = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "impossible")
            .expect("impossible declaration");
        assert!(matches!(
            impossible.body_disposition,
            crate::semantic::CheckedBodyDisposition::Uninhabited { .. }
        ));
        assert!(impossible.body.iter().flatten().any(|statement| matches!(
            statement,
            crate::semantic::CheckedStatement::Evaluate {
                value: crate::semantic::CheckedExpression::UserCall { .. },
                ..
            }
        )));
        let plan = super::specialize::PhysicalFunctions::build(&checked.data)
            .expect("proof-closed functions retain their physical signature");
        let variant = plan
            .variants
            .iter()
            .find(|variant| variant.source == impossible.id)
            .expect("unreferenced source definition still emitted");
        assert!(
            variant.calls.is_empty(),
            "a proof-closed body has no executable calls"
        );
    });
}

#[test]
fn a_buffer_release_retains_its_owned_storage_type() {
    // STOR-3 release names ordinary owned storage; opaque drops are empty.
    // A runtime-capacity `Array<u8>` exists only as `Box` content [TYPE-9],
    // so the owner released here is the cell.
    with_ir(
        b"fn drop_buffer(values: Box<Array<u8>>) -> result: unit pure {\n  return unit;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n",
        |program| {
            let [drop] = return_drops(function(program, "drop_buffer")) else {
                panic!("the buffer owner must be released once");
            };
            let IrType::Nominal(cell) = drop.ty() else {
                panic!("the released owner is the cell, got {:?}", drop.ty());
            };
            let super::IrNominalKind::Box { referent, .. } =
                program.nominal(cell).expect("cell nominal").kind()
            else {
                panic!("the released owner is a cell");
            };
            assert!(matches!(referent, IrType::Buffer { .. }));
        },
    );
}

/// The recognized byte-walk loop for the wide-probe tests, with `{MIDDLE}`
/// and `{STEP}` varied per case.
fn byte_walk_source(middle: &str, step: &str) -> Vec<u8> {
    format!(
        "fn main() -> status: std::process::ExitStatus pure {{\n  let data = box_array_filled::<u8>(count: 64_u64, value: 97_u8);\n  let mark = 88_u8;\n  let seen = 0_u64;\n  let stop = data.inner.len;\n  let cursor = 0_u64;\n  loop @walk {{\n    let done = cursor >= stop;\n    if done {{\n      break @walk;\n    }}\n    let byte = data.inner[cursor];\n{middle}    set cursor = cursor +wrap {step};\n  }}\n  return std::process::exit_status(code: 0_u8);\n}}\n"
    )
    .into_bytes()
}

const NEUTRAL_MIDDLE: &str = "    let newline = byte == 10_u8;\n    if newline {\n      set seen = seen +wrap 1_u64;\n    }\n    let lead = byte == mark;\n    if lead {\n      set seen = seen +wrap 2_u64;\n    }\n";

fn probe_needle_counts(program: &IrProgram) -> Vec<usize> {
    program
        .functions()
        .iter()
        .flat_map(IrFunction::blocks)
        .flat_map(IrBlock::instructions)
        .filter_map(|instruction| {
            let IrInstruction::Define {
                operation: IrOperation::BufferProbeSkip { needles, .. },
                ..
            } = instruction
            else {
                return None;
            };
            Some(needles.len())
        })
        .collect()
}

#[test]
fn a_recognized_byte_walk_gains_one_wide_probe_with_its_needles() {
    with_ir(&byte_walk_source(NEUTRAL_MIDDLE, "1_u64"), |program| {
        assert_eq!(probe_needle_counts(program), vec![2]);
    });
}

#[test]
fn an_effect_on_the_quiet_path_declines_the_wide_probe() {
    let middle = format!("{NEUTRAL_MIDDLE}    set seen = seen +wrap 1_u64;\n");
    with_ir(&byte_walk_source(&middle, "1_u64"), |program| {
        assert_eq!(probe_needle_counts(program), Vec::<usize>::new());
    });
}

#[test]
fn a_non_single_step_increment_declines_the_wide_probe() {
    with_ir(&byte_walk_source(NEUTRAL_MIDDLE, "2_u64"), |program| {
        assert_eq!(probe_needle_counts(program), Vec::<usize>::new());
    });
}

#[test]
fn a_needle_declared_inside_the_loop_declines_the_wide_probe() {
    let middle = "    let inner_mark = 88_u8;\n    let lead = byte == inner_mark;\n    if lead {\n      set seen = seen +wrap 2_u64;\n    }\n";
    with_ir(&byte_walk_source(middle, "1_u64"), |program| {
        assert_eq!(probe_needle_counts(program), Vec::<usize>::new());
    });
}
