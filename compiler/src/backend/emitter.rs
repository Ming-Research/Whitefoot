//! Conservative textual LLVM emission for the active Whitefoot specification.
//!
//! Emission consumes typed IR after optional loop shapes have been selected for
//! the same target. It preserves every retained
//! check, emits no overflow or alias promises, initializes complete aggregate
//! representations, and keeps a defensive abort edge for enum discriminants.

mod array;
mod boxes;
mod buffer;
mod cleanup;
mod contexts;
mod conversion;
mod floating;
mod floor;
mod frames;
mod frontier;
mod integer;
mod operations;
mod parallel;
pub(super) mod places;
mod reinterpret;
mod runs;
mod shared;
mod slice;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt::Write;

use super::abi::{FunctionAbi, ParameterAbi, ResultAbi};
use super::emission::{FunctionBody, Linkage, Module, Parameter, References, Signature};
pub use super::runtime::*;
use super::storage::{FunctionStoragePlan, is_stored_aggregate};
use crate::target::{
    TargetAggregateLayout, TargetFramePlan, TargetFrameSlot, TargetLayout, TargetLayoutFailure,
    TargetStorageType, parallel_lane_frame_layout, plan_target_frame, validate_program,
    validate_static_storage,
};
use crate::{
    IrAddressed, IrAllocationObligations, IrArrayRoot, IrBlock, IrBlockId, IrBooleanOperation,
    IrConstant, IrConversionMode, IrDrop, IrDropSubject, IrEnumType, IrFloatOperation, IrFunction,
    IrGlobalValue, IrInstruction, IrIntegerOperation, IrNominal, IrNominalId, IrNominalKind,
    IrOperation, IrOverlap, IrProgram, IrTargetDomainObligation, IrTerminator, IrType, IrValueId,
    IrWindowShape,
};
use cleanup::{emit_resource_drop_helpers, emit_value_cleanup, type_requires_cleanup};
pub use floor::FLOOR_STACK_BYTES;
use floor::floor_runtime_fallback;
pub use floor::{FLOOR_RUNTIME_SOURCE, FLOOR_WINDOWS_RUNTIME_SOURCE};
pub(crate) use frontier::is_recursion_budget_symbol;
use frontier::{Grain, RecursiveFrontiers, recursion_budget_symbol};
pub use parallel::module_requires_parallel_runtime;
use parallel::{
    HandedOut, LoopSplitSite, ParallelThunks, parallel_pool_query_declaration,
    parallel_pool_query_fallback, parallel_recursion_budget_declaration,
    parallel_recursion_budget_fallback, parallel_runtime_declarations, parallel_runtime_fallback,
    parallel_split_budget_declaration, parallel_split_budget_fallback, sequential_clone_set,
    sequential_clone_symbol,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackendFailure {
    TargetLayout(TargetLayoutFailure),
    InvalidIr,
    CounterOverflow,
    TextEmission,
}

impl From<std::fmt::Error> for BackendFailure {
    fn from(_: std::fmt::Error) -> Self {
        Self::TextEmission
    }
}

/// An emitted LLVM module with the definitions and dependencies its link
/// fragments need. Text-only consumers can borrow or take its rendered form.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LlvmModule {
    pub(crate) model: Module,
    text: String,
    ledger: Vec<String>,
}

impl LlvmModule {
    /// Consumes the module and returns its complete textual LLVM.
    #[must_use]
    pub fn into_string(self) -> String {
        self.text
    }

    pub(crate) fn take_actualization_ledger(&mut self) -> Vec<String> {
        std::mem::take(&mut self.ledger)
    }

    pub(crate) fn append(&mut self, module: Module) {
        self.model.append(module);
        self.text = self.model.render();
    }

    pub(crate) fn encode(&self) -> Vec<u8> {
        self.model.encode()
    }
    pub(crate) fn decode(bytes: &[u8]) -> Option<Self> {
        let model = Module::decode(bytes)?;
        Some(Self {
            text: model.render(),
            model,
            ledger: Vec::new(),
        })
    }

    /// Borrows the complete textual LLVM.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// The non-normative report of what this emission actualized that lowering
    /// could not report for it, in the order `--par-ledger` prints it.
    ///
    /// The recursion budget reads the call graph after the clone sets are
    /// known, which happens here and not in lowering, so its lines reach the
    /// ledger through this channel rather than through a second analysis. It
    /// says nothing about acceptance: a module with an empty report and a
    /// module with a full one are both programs the checker already admitted.
    #[must_use]
    pub fn actualization_ledger(&self) -> &[String] {
        &self.ledger
    }
}

impl core::ops::Deref for LlvmModule {
    type Target = str;
    fn deref(&self) -> &str {
        &self.text
    }
}
impl AsRef<[u8]> for LlvmModule {
    fn as_ref(&self) -> &[u8] {
        self.text.as_bytes()
    }
}
impl core::fmt::Display for LlvmModule {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(&self.text)
    }
}

#[cfg(test)]
pub fn emit_llvm(program: &IrProgram) -> Result<LlvmModule, BackendFailure> {
    let target = TargetLayout::host().map_err(BackendFailure::TargetLayout)?;
    emit_llvm_with_layout(program, target)
}

/// The executable builder may choose the no-pool world once at startup. The
/// set depends on ordinary calls and physical lane fit, never on an entry kind.
pub(crate) fn sequential_entry_symbol(
    program: &IrProgram,
    name: &str,
) -> Result<Option<String>, BackendFailure> {
    let clones = sequential_clone_set(program);
    let selected_is_cloned = program
        .functions()
        .iter()
        .enumerate()
        .any(|(index, function)| {
            function.name() == name
                && u32::try_from(index).is_ok_and(|index| clones.contains(&index))
        });
    if !selected_is_cloned {
        return Ok(None);
    }
    for function in program.functions() {
        for overlap in function.overlaps() {
            if ordinary_overlap_lane_frames(
                program,
                TargetLayout::host().map_err(BackendFailure::TargetLayout)?,
                function,
                overlap,
                &|_| false,
            )?
            .is_some()
            {
                return Ok(Some(sequential_clone_symbol(name)));
            }
        }
    }
    Ok(None)
}

/// Emits the same ordinary callable ABI with a selected physical target layout.
pub(crate) fn emit_llvm_with_layout(
    program: &IrProgram,
    target: TargetLayout,
) -> Result<LlvmModule, BackendFailure> {
    emit_llvm_with_window_address_facts(program, target, WindowAddressFacts::Emit)
}

/// Controls only the optional fact about a window's normalized address
/// operand. Withholding it is a test observation over the same checked IR,
/// target qualification and ordinary lowering.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum WindowAddressFacts {
    Emit,
    #[cfg(test)]
    Withhold,
}

pub(super) fn emit_llvm_with_window_address_facts(
    program: &IrProgram,
    target: TargetLayout,
    window_address_facts: WindowAddressFacts,
) -> Result<LlvmModule, BackendFailure> {
    validate_program(target, program).map_err(BackendFailure::TargetLayout)?;
    let mut intrinsics = BTreeSet::new();
    let mut thunks = ParallelThunks::default();
    let refusal_clones = if program.sequential_compute_refusal() {
        sequential_clone_set(program)
    } else {
        HashSet::new()
    };
    // Every compute-actualizing lowering asks the question, because every one
    // of them carries a recursion budget; the set is what decides which
    // components can answer it, since a family's cut lands in a clone.
    let frontier_clones = if program.recursion_budget().is_some() {
        sequential_clone_set(program)
    } else {
        HashSet::new()
    };
    let frontiers = RecursiveFrontiers::new(program, &frontier_clones);
    let mut functions = Module::default();
    for (ordinal, function) in program.functions().iter().enumerate() {
        // A member of a budgeted component keeps its ordinary symbol and its
        // ordinary signature, and that symbol obtains the initial budget and
        // enters the family. The body itself is emitted once, below.
        if frontiers.grain(ordinal).is_some() {
            functions.append(emit_recursion_budget_entry(
                program,
                function,
                &frontiers,
                &mut thunks,
            )?);
            continue;
        }
        let emitter = FunctionEmitter::new(
            program,
            target,
            function,
            ModuleState {
                intrinsics: &mut intrinsics,
                parallel: &mut thunks,
                sequential_clones: None,
                refusal_clones: &refusal_clones,
                frontiers: &frontiers,
                grain: None,
                window_address_facts,
            },
        )?;
        functions.append(emitter.emit()?);
    }
    // The budget-carrying half of each family: one variant per member, the
    // same emitter over the same IR as every other function of this module,
    // differing only in the trailing budget it tests and in naming its
    // siblings' variants.
    for (ordinal, function) in program.functions().iter().enumerate() {
        let Some(grain) = frontiers.grain(ordinal) else {
            continue;
        };
        functions.append(
            FunctionEmitter::new(
                program,
                target,
                function,
                ModuleState {
                    intrinsics: &mut intrinsics,
                    parallel: &mut thunks,
                    sequential_clones: None,
                    refusal_clones: &refusal_clones,
                    frontiers: &frontiers,
                    grain: Some(grain),
                    window_address_facts,
                },
            )?
            .emit()?,
        );
    }
    // The second world. It exists only where the first one actualizes
    // something, so a build that hands nothing out — every default build among
    // them — emits exactly the module it emitted before this path existed.
    //
    // A build launcher can select the sequential clone of its ordinary entry.
    // The clone set is closed upwards through the module's call graph and
    // does not depend on the presence or spelling of a selected entry.
    let clones = if thunks.is_used() {
        sequential_clone_set(program)
    } else if frontiers.is_used() {
        frontier_clones
    } else {
        HashSet::new()
    };
    for (ordinal, function) in program.functions().iter().enumerate() {
        if u32::try_from(ordinal).is_ok_and(|ordinal| clones.contains(&ordinal)) {
            functions.append(
                FunctionEmitter::new(
                    program,
                    target,
                    function,
                    ModuleState {
                        intrinsics: &mut intrinsics,
                        parallel: &mut thunks,
                        sequential_clones: Some(&clones),
                        refusal_clones: &refusal_clones,
                        frontiers: &frontiers,
                        grain: None,
                        window_address_facts,
                    },
                )?
                .emit()?,
            );
        }
    }
    let has_matches = program.functions().iter().any(|function| {
        function
            .blocks()
            .iter()
            .any(|block| matches!(block.terminator(), IrTerminator::Match { .. }))
    });
    let drop_helpers = emit_resource_drop_helpers(program, target)?;
    let has_heap_storage = !drop_helpers.is_empty()
        || program.functions().iter().any(IrFunction::contains_buffer)
        || cleanup::program_types(program)?.into_iter().any(|ty| {
            matches!(ty, IrType::Nominal(id) if program
                .nominal(id)
                .is_some_and(|nominal| matches!(nominal.kind(), IrNominalKind::Box { .. })))
        });
    let heap_record_type = TargetStorageType::bytes(
        u64::try_from(HEAP_RECORD.len()).map_err(|_| BackendFailure::CounterOverflow)?,
    );
    if has_heap_storage {
        validate_static_storage(target, program, &heap_record_type)
            .map_err(BackendFailure::TargetLayout)?;
    }
    let mut text = Module::default();
    text.text("; Whitefoot conservative module\n");
    text.header("source_filename = \"whitefoot\"".to_owned());
    text.header(format!("target datalayout = \"{}\"", target.data_layout()));
    text.header(format!("target triple = \"{}\"", target.triple()));
    text.text("\n");
    emit_nominal_declarations(&mut text, program)?;
    emit_global_constants(&mut text, program)?;
    // An allocation this host refuses is the heap twin of an exhausted stack,
    // and it gets the same treatment: one record naming the resource class,
    // written once, before a defined abort. The bytes carry no `rule_id`, no
    // function, and no node path because resource availability is not a
    // source-code failure.
    if has_heap_storage {
        text.global(
            ".wf_resource.heap".to_owned(),
            "unnamed_addr constant",
            llvm_storage_type(program, &heap_record_type)?,
            format!("c\"{}\"", llvm_bytes(HEAP_RECORD.as_bytes())),
            Some(1),
            References::default(),
        );
    }
    // Heap availability is outside source proof. If this module can allocate,
    // it carries one resource-record writer for allocator refusal.
    let writes_a_record = has_heap_storage;
    // A latch decides between threads, so it belongs only to a module that has
    // more than one. `thunks.is_used()` is exactly "this module hands a call
    // out to a worker lane": false for every default build, and false for a
    // `--par` build that actualizes nothing. A lone thread races no one, so
    // those modules emit the sequential resource path.
    let latched_resource_record = writes_a_record && thunks.is_used();
    if latched_resource_record {
        validate_static_storage(target, program, &TargetStorageType::integer(32))
            .map_err(BackendFailure::TargetLayout)?;
        text.append(resource_record_latch()?);
    }
    let windows = target.triple().contains("windows");
    if writes_a_record {
        if windows {
            text.declare(Signature::new(
                "wf__windows_diagnostic_write",
                "i64",
                vec![Parameter::unnamed("ptr"), Parameter::unnamed("i64")],
            ));
        } else {
            emit_posix_resource_write(&mut text, target)?;
        }
    }
    if writes_a_record || has_matches {
        let mut abort = Signature::new("abort", "void", Vec::new());
        abort.suffix = " noreturn".to_owned();
        text.declare(abort);
    }
    if has_heap_storage || cleanup::program_has_general_run(program)? {
        text.declare(Signature::new(
            "malloc",
            "ptr",
            vec![Parameter::unnamed("i64")],
        ));
        text.declare(Signature::new(
            "free",
            "void",
            vec![Parameter::unnamed("ptr")],
        ));
    }
    if latched_resource_record {
        text.append(resource_record_latch_fallback()?);
        text.append(if windows {
            windows_latched_resource_record_writer()?
        } else {
            latched_resource_record_writer()?
        });
    } else if writes_a_record {
        text.append(if windows {
            windows_sequential_resource_record_writer()?
        } else {
            sequential_resource_record_writer()?
        });
    } else if has_matches {
        text.text("\n");
    }
    if has_heap_storage {
        let mut signature = Signature::new("wf_resource_abort", "void", Vec::new());
        signature.linkage = Linkage::Private;
        signature.suffix = " noreturn".to_owned();
        let mut body = FunctionBody::default();
        body.open_block("entry".to_owned());
        body.instructions(&format!("  call void @wf_resource_record_abort(ptr @.wf_resource.heap, i64 {})\n  unreachable\n", HEAP_RECORD.len()), &["wf_resource_record_abort", ".wf_resource.heap"]);
        text.define(signature.define(body, "")?);
        text.text("\n");
    }
    text.append(drop_helpers);
    for intrinsic in intrinsics {
        let (name, result, parameters) = match intrinsic {
            IntrinsicDeclaration::Assume => (
                "llvm.assume".to_owned(),
                "void".to_owned(),
                vec!["i1".to_owned()],
            ),
            IntrinsicDeclaration::MemoryCopy => (
                "llvm.memcpy.p0.p0.i64".to_owned(),
                "void".to_owned(),
                vec![
                    "ptr".to_owned(),
                    "ptr".to_owned(),
                    "i64".to_owned(),
                    "i1 immarg".to_owned(),
                ],
            ),
            IntrinsicDeclaration::MemoryMove => (
                "llvm.memmove.p0.p0.i64".to_owned(),
                "void".to_owned(),
                vec![
                    "ptr".to_owned(),
                    "ptr".to_owned(),
                    "i64".to_owned(),
                    "i1 immarg".to_owned(),
                ],
            ),
            IntrinsicDeclaration::Overflow { name, ty } => {
                (name, format!("{{ {ty}, i1 }}"), vec![ty.clone(), ty])
            }
            IntrinsicDeclaration::UnaryWithFlag { name, ty } => {
                (name, ty.clone(), vec![ty, "i1".to_owned()])
            }
            IntrinsicDeclaration::Unary { name, ty } => (name, ty.clone(), vec![ty]),
            IntrinsicDeclaration::Binary { name, ty } => (name, ty.clone(), vec![ty.clone(), ty]),
            IntrinsicDeclaration::Ternary { name, ty } => {
                (name, ty.clone(), vec![ty.clone(), ty.clone(), ty])
            }
            IntrinsicDeclaration::UnaryCast {
                name,
                result_ty,
                argument_ty,
            } => (name, result_ty, vec![argument_ty]),
        };
        text.declare(Signature::new(
            name,
            result,
            parameters.into_iter().map(Parameter::unnamed).collect(),
        ));
    }
    // [SHARE-1] a module whose program holds no shared object names none of
    // its entries.
    if cleanup::program_uses_shared(program)? {
        text.text("\n");
        text.append(cleanup::shared_runtime_declarations());
    }
    // [WAIT-1] a module with no waiting definition names no frame symbol.
    if frames::program_has_frames(program) {
        text.text("\n");
        text.append(frames::frame_runtime_declarations());
    }
    // [PAR-4] a module that starts no context names no context thunk.
    if thunks.starts_contexts() {
        text.text("\n");
        text.append(thunks.take_context_definitions());
    }
    // Emitted only where a permitted overlap group is actually handed out, so
    // a module that overlaps nothing names no runtime symbol at all.
    if thunks.is_used() {
        text.text("\n");
        text.append(if windows {
            parallel_runtime_declarations()?
        } else {
            parallel_runtime_fallback()?
        });
        if !clones.is_empty() {
            text.append(if windows {
                parallel_pool_query_declaration()?
            } else {
                parallel_pool_query_fallback()?
            });
        }
        if thunks.queries_split_budget() {
            text.append(if windows {
                parallel_split_budget_declaration()?
            } else {
                parallel_split_budget_fallback()?
            });
        }
        if thunks.queries_recursion_budget() {
            text.append(if windows {
                parallel_recursion_budget_declaration()?
            } else {
                parallel_recursion_budget_fallback()?
            });
        }
        text.append(thunks.into_definitions());
    } else if thunks.queries_recursion_budget() {
        // A family whose every offer was declined for its frame still asks
        // for its budget, and the symbol it names must be answered.
        text.text("\n");
        text.append(if windows {
            parallel_recursion_budget_declaration()?
        } else {
            parallel_recursion_budget_fallback()?
        });
    }
    if !functions.is_empty() {
        text.text("\n");
        text.append(functions);
    }
    // Unconditional, unlike the parallel runtime's: every program can run out
    // of stack, so every module names the floor and carries its own answer for
    // a link that does not supply one.
    text.text("\n");
    text.append(floor_runtime_fallback()?);
    text.text("\n");
    text.attribute_group(0, format!("\"probe-stack\"=\"{}\"", target.stack_probe()));
    Ok(LlvmModule {
        text: text.render(),
        model: text,
        ledger: frontiers.ledger().to_vec(),
    })
}

/// The bytes an allocation refusal writes before aborting.
///
/// The heap twin of the exhausted-stack record the floor's runtime writes, and
/// fixed the same way and for the same reasons: it names the resource class
/// and nothing else. It carries no rule identifier, function, or node path
/// because an allocation the host refused is the trusted computing base
/// reaching its limit, not a source proof obligation.
const HEAP_RECORD: &str = "{\"resource\":\"heap\"}\n";

/// One call's arguments, in the emitting function's own parameters: what a
/// same-signature forward to a clone or a variant passes on.
fn ordinary_call_arguments(
    program: &IrProgram,
    function: &IrFunction,
    abi: &FunctionAbi,
) -> Result<String, BackendFailure> {
    let mut arguments = Vec::with_capacity(abi.parameters().len() + 1);
    if abi.result().uses_destination() {
        arguments.push(format!("ptr {RESULT_POINTER}"));
    }
    for ((value, _), parameter) in function.parameters().iter().zip(abi.parameters()) {
        arguments.push(incoming_parameter(program, *value, *parameter, "")?);
    }
    Ok(arguments.join(", "))
}

/// One parameter as the emitting function receives it: its head's
/// declaration with `facts` after the pointer's type, or, with no facts, the
/// operands a same-signature forward passes on unchanged.
///
/// A range reference arrives as its element pointer and count (see
/// [`super::abi`]); the body reassembles its `{ ptr, i64 }` pair at entry.
fn incoming_parameter(
    program: &IrProgram,
    value: IrValueId,
    parameter: ParameterAbi,
    facts: &str,
) -> Result<String, BackendFailure> {
    Ok(if parameter.is_indirect() {
        format!("ptr %wf.arg.v{}", value.ordinal())
    } else if parameter.is_range() {
        let (pointer, count) = incoming_range_parts(value);
        format!("ptr{facts} {pointer}, i64 {count}")
    } else {
        format!(
            "{}{facts} {}",
            llvm_type(program, parameter.ty())?,
            value_name(value)
        )
    })
}

fn incoming_parameters(
    program: &IrProgram,
    value: IrValueId,
    parameter: ParameterAbi,
    facts: &str,
    references: &mut References,
) -> Result<Vec<Parameter>, BackendFailure> {
    Ok(if parameter.is_indirect() {
        vec![Parameter::named(
            "ptr",
            format!("%wf.arg.v{}", value.ordinal()),
        )]
    } else if parameter.is_range() {
        let (pointer, count) = incoming_range_parts(value);
        vec![
            Parameter::named(format!("ptr{facts}"), pointer),
            Parameter::named("i64", count),
        ]
    } else {
        let ty = llvm_type_with_references(program, parameter.ty(), &mut references.types)?;
        vec![Parameter::named(format!("{ty}{facts}"), value_name(value))]
    })
}

/// The element pointer and count a range-reference parameter arrives as.
fn incoming_range_parts(value: IrValueId) -> (String, String) {
    (
        format!("%wf.arg.v{}.data", value.ordinal()),
        format!("%wf.arg.v{}.len", value.ordinal()),
    )
}

/// A budgeted component's ordinary entry: obtain the initial budget and enter
/// the family with it.
///
/// This is the only place a budget comes from outside the family, and it is
/// asked once per call into the component rather than once per node. The
/// runtime's answer follows the pool width, which is why it is a query and not
/// a constant: the measured best cut is not the same cut at two lanes and at
/// four. A pinned budget — the A/B control — starts from a compile-time value
/// instead, and then this entry names no runtime symbol at all.
///
/// The member keeps its ordinary symbol, signature and result ABI, so nothing
/// outside the component sees the family; what changes is that the body it
/// forwards to is emitted once, with the budget as a trailing parameter.
fn emit_recursion_budget_entry(
    program: &IrProgram,
    function: &IrFunction,
    frontiers: &RecursiveFrontiers,
    thunks: &mut ParallelThunks,
) -> Result<Module, BackendFailure> {
    let abi = FunctionAbi::build(program, function)?;
    let mut references = References::default();
    let result = if abi.result().uses_destination() {
        "void".to_owned()
    } else {
        llvm_type_with_references(program, abi.result().ty(), &mut references.types)?
    };
    let mut parameters = Vec::new();
    if abi.result().uses_destination() {
        parameters.push(Parameter::named("ptr", RESULT_POINTER));
    }
    for ((value, _), parameter) in function.parameters().iter().zip(abi.parameters()) {
        parameters.extend(incoming_parameters(
            program,
            *value,
            *parameter,
            "",
            &mut references,
        )?);
    }
    let mut signature = Signature::new(source_symbol(function.name()), result, parameters);
    signature.linkage = Linkage::Internal;
    signature.references = references;
    let mut output = FunctionBody::default();
    output.open_block("entry".to_owned());
    let mut arguments = ordinary_call_arguments(program, function, &abi)?;
    let budget = match frontiers.initial().ok_or(BackendFailure::InvalidIr)? {
        crate::RecursionBudget::Off => return Err(BackendFailure::InvalidIr),
        crate::RecursionBudget::Pinned(levels) => levels.get().to_string(),
        crate::RecursionBudget::RuntimeDerived => {
            output.instructions(
                "  %wf.budget = call i64 @wf__par_recursion_budget()\n",
                &["wf__par_recursion_budget"],
            );
            thunks.queries_recursion_budget = true;
            "%wf.budget".to_owned()
        }
    };
    if !arguments.is_empty() {
        arguments.push_str(", ");
    }
    write!(arguments, "i64 {budget}").map_err(|_| BackendFailure::TextEmission)?;
    let callee = recursion_budget_symbol(function.name());
    output.symbol(&callee);
    if abi.result().uses_destination() {
        {
            output.symbol(callee.to_string());
            write!(output, "  call void @{callee}({arguments})\n  ret void\n")
        }
        .map_err(|_| BackendFailure::TextEmission)?;
    } else {
        let result = llvm_type(program, abi.result().ty())?;
        {
            output.symbol(callee.to_string());
            write!(
                output,
                "  %wf.entry = call {result} @{callee}({arguments})\n  ret {result} %wf.entry\n"
            )
        }
        .map_err(|_| BackendFailure::TextEmission)?;
    }
    let mut module = Module::default();
    module.define(signature.define(output, "")?);
    module.text("\n");
    Ok(module)
}

/// The block a budgeted world tests its remaining levels in, and the block it
/// leaves for the sequential clone from.
const GRAIN_ENTRY_LABEL: &str = "par.grain";
const GRAIN_SPENT_LABEL: &str = "par.grain.spent";

/// The spelling of the no-capture parameter attribute this build's assembler
/// accepts, probed at build time (compiler/backend-facts). LLVM 21 renamed
/// `nocapture` to `captures(none)` and no version is pinned here.
const NO_CAPTURE_ATTRIBUTE: &str = env!("WHITEFOOT_NO_CAPTURE_ATTRIBUTE");

/// The one [PRE-1] record whose two reference arguments may name the same
/// place [OP-11], so its parameters carry every proved fact but `noalias`.
fn aliasing_admitted_row(name: &str) -> bool {
    name == "swap" || name.starts_with("swap$")
}

fn emit_global_constants(output: &mut Module, program: &IrProgram) -> Result<(), BackendFailure> {
    for constant in program.constants() {
        output.text(format!("; const {}\n", constant.name()));
        let mut references = References::default();
        let ty = llvm_type_with_references(program, constant.ty(), &mut references.types)?;
        let value =
            global_constant_value(program, constant.value(), constant.ty(), &mut references)?;
        output.global(
            format!(".wf_const.{}", constant.link_name()),
            "unnamed_addr constant",
            ty,
            value,
            None,
            references,
        );
    }
    if !program.constants().is_empty() {
        output.text("\n");
    }
    Ok(())
}

/// Renders one rodata constant value of one exact type: a scalar operand, a
/// complete array, or a complete struct aggregate with each field rendered
/// recursively [CONST-2 candidate].
fn global_constant_value(
    program: &IrProgram,
    value: &IrGlobalValue,
    ty: IrType,
    references: &mut References,
) -> Result<String, BackendFailure> {
    match (value, ty) {
        (IrGlobalValue::Scalar(value), ty) => constant_operand(*value, ty),
        (IrGlobalValue::Array(elements), IrType::Array { element, length }) => {
            if u64::try_from(elements.len()).map_err(|_| BackendFailure::CounterOverflow)? != length
            {
                return Err(BackendFailure::InvalidIr);
            }
            if elements.is_empty() {
                return Ok("zeroinitializer".to_owned());
            }
            let mut text = String::from("[");
            let element_type = program.element(element).ok_or(BackendFailure::InvalidIr)?;
            let llvm_element_type =
                llvm_type_with_references(program, element_type, &mut references.types)?;
            for (index, value) in elements.iter().enumerate() {
                if index != 0 {
                    text.push_str(", ");
                }
                write!(
                    text,
                    "{llvm_element_type} {}",
                    global_constant_value(program, value, element_type, references)?
                )
                .map_err(|_| BackendFailure::TextEmission)?;
            }
            text.push(']');
            Ok(text)
        }
        (IrGlobalValue::Struct(fields), IrType::Nominal(id)) => {
            let nominal = program.nominal(id).ok_or(BackendFailure::InvalidIr)?;
            let IrNominalKind::Struct { fields: declared } = nominal.kind() else {
                return Err(BackendFailure::InvalidIr);
            };
            if fields.len() != declared.len() {
                return Err(BackendFailure::InvalidIr);
            }
            if fields.is_empty() {
                return Ok("zeroinitializer".to_owned());
            }
            let mut text = String::from("{ ");
            for (index, (value, field)) in fields.iter().zip(declared).enumerate() {
                if index != 0 {
                    text.push_str(", ");
                }
                write!(
                    text,
                    "{} {}",
                    llvm_type_with_references(program, field.ty(), &mut references.types)?,
                    global_constant_value(program, value, field.ty(), references)?
                )
                .map_err(|_| BackendFailure::TextEmission)?;
            }
            text.push_str(" }");
            Ok(text)
        }
        _ => Err(BackendFailure::InvalidIr),
    }
}

fn emit_nominal_declarations(
    module: &mut Module,
    program: &IrProgram,
) -> Result<(), BackendFailure> {
    let mut emitted = false;
    for nominal in program.nominals() {
        // Pointer owners and the uniform opaque representation do not need a
        // named aggregate type.
        if nominal.is_tag_only_enum()
            || matches!(
                nominal.kind(),
                IrNominalKind::Box { .. } | IrNominalKind::Opaque | IrNominalKind::Shared { .. }
            )
        {
            continue;
        }
        emitted = true;
        let mut output = String::from("{ ");
        let mut references = References::default();
        match nominal.kind() {
            IrNominalKind::Struct { fields } => {
                for (index, field) in fields.iter().enumerate() {
                    if index != 0 {
                        output.push_str(", ");
                    }
                    output.push_str(&llvm_type_with_references(
                        program,
                        field.ty(),
                        &mut references.types,
                    )?);
                }
            }
            IrNominalKind::Enum { variants } => {
                output.push_str("i32");
                for variant in variants {
                    for field in variant.fields() {
                        output.push_str(", ");
                        output.push_str(&llvm_type_with_references(
                            program,
                            field.ty(),
                            &mut references.types,
                        )?);
                    }
                }
            }
            IrNominalKind::Box { .. } | IrNominalKind::Opaque | IrNominalKind::Shared { .. } => {
                return Err(BackendFailure::InvalidIr);
            }
        }
        output.push_str(" }");
        module.named_type(format!("wf.t.{}", nominal.link_name()), output, references);
    }
    if emitted {
        module.text("\n");
    }
    Ok(())
}

#[derive(Clone)]
struct Incoming {
    predecessor: IrBlockId,
    arguments: Vec<IrValueId>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum IntrinsicDeclaration {
    Assume,
    MemoryCopy,
    MemoryMove,
    Overflow {
        name: String,
        ty: String,
    },
    UnaryWithFlag {
        name: String,
        ty: String,
    },
    Unary {
        name: String,
        ty: String,
    },
    Binary {
        name: String,
        ty: String,
    },
    Ternary {
        name: String,
        ty: String,
    },
    UnaryCast {
        name: String,
        result_ty: String,
        argument_ty: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum FunctionSlot {
    /// One immutable aggregate value's planned storage, shared only after
    /// complete control-flow interference checks.
    OwnedValue(usize),
    ArrayFillIndex(IrValueId),
    Address(IrValueId),
    /// The slot a register-returned definition's public entry gives its
    /// body to construct the result in.
    Result,
    /// The slot a waiting call constructs a result that has no planned
    /// storage in, read back once the callee has transferred back [WAIT-1].
    WaitingResult(IrValueId),
    /// The slot a bound start's context constructs its result in, keyed by
    /// the start, which its await reads [WAIT-2]. It is the starting frame's
    /// own, so it outlives the context that writes it.
    ContextResult(IrValueId),
}

/// Where a body constructs its stored result: its destination parameter,
/// which for a register-returned result is its public entry's frame slot.
const RESULT_POINTER: &str = "%wf.result";

/// The internal symbol a register-returned definition's destination-form
/// body is emitted under, beside the public entry that keeps `symbol`.
///
/// No other definition can hold it. A function outside the root module is
/// spelled `path.name`, so a source function could take `<symbol>.body`
/// only as a function `body` in a child module named after the entry's
/// function, and [MOD-3] rejects a declaration that extends its module's
/// path to a registered module. An instance's `$instance$` suffix and a
/// compiler-owned `wf__` symbol hold spellings no IDENT has [FORM-3].
fn result_body_symbol(symbol: &str) -> String {
    format!("{symbol}.body")
}

struct PlannedFunctionSlot {
    logical_index: usize,
    pointer: String,
}

/// The one physical frame an ordinary generated function owns.
///
/// Planning walks the already-selected IR schedule before emission and gives
/// every actual materialization a semantic key. Target layout then turns the
/// logical slots into one explicitly padded struct. Emission can only obtain a
/// pointer by that key; it has no string-shaped `alloca` escape hatch.
struct FunctionFramePlan {
    target: TargetFramePlan,
    slots: HashMap<FunctionSlot, PlannedFunctionSlot>,
    ordered: Vec<FunctionSlot>,
}

/// Storage selected before emission, including values live across calls.
struct FunctionFrameContents<'plan> {
    storage: &'plan FunctionStoragePlan,
    result_slot: Option<usize>,
}

impl FunctionFramePlan {
    fn build(
        target: TargetLayout,
        program: &IrProgram,
        function: &IrFunction,
        contents: FunctionFrameContents<'_>,
    ) -> Result<Self, BackendFailure> {
        let FunctionFrameContents {
            storage,
            result_slot,
        } = contents;
        let mut specifications = Vec::new();
        let mut ordered = Vec::new();
        for (slot, ty) in storage.slots().iter().copied().enumerate() {
            if Some(slot) != result_slot
                && storage.destination(slot).is_none()
                && storage.field_destination(slot).is_none()
            {
                push_function_slot(
                    &mut specifications,
                    &mut ordered,
                    FunctionSlot::OwnedValue(slot),
                    TargetStorageType::source(ty),
                    None,
                )?;
            }
        }
        for block in function.blocks() {
            for instruction in block.instructions() {
                let IrInstruction::Define {
                    result, operation, ..
                } = instruction
                else {
                    continue;
                };
                match operation {
                    IrOperation::ArrayFill { .. } => {
                        push_function_slot(
                            &mut specifications,
                            &mut ordered,
                            FunctionSlot::ArrayFillIndex(*result),
                            TargetStorageType::integer(64),
                            None,
                        )?;
                    }
                    IrOperation::AddressOf { referent, .. } => {
                        let storage = TargetStorageType::source(referent.ty());
                        let key = FunctionSlot::Address(*result);
                        push_function_slot(&mut specifications, &mut ordered, key, storage, None)?;
                    }
                    IrOperation::Call {
                        function: callee, ..
                    } if storage.slot(*result).is_none()
                        && program
                            .functions()
                            .get(*callee as usize)
                            .is_some_and(IrFunction::waits) =>
                    {
                        let IrInstruction::Define { ty, .. } = instruction else {
                            continue;
                        };
                        push_function_slot(
                            &mut specifications,
                            &mut ordered,
                            FunctionSlot::WaitingResult(*result),
                            TargetStorageType::source(*ty),
                            None,
                        )?;
                    }
                    IrOperation::ContextAwait { start } => {
                        let IrInstruction::Define { ty, .. } = instruction else {
                            continue;
                        };
                        push_function_slot(
                            &mut specifications,
                            &mut ordered,
                            FunctionSlot::ContextResult(*start),
                            TargetStorageType::source(*ty),
                            None,
                        )?;
                        if storage.slot(*result).is_none() {
                            push_function_slot(
                                &mut specifications,
                                &mut ordered,
                                FunctionSlot::WaitingResult(*result),
                                TargetStorageType::source(*ty),
                                None,
                            )?;
                        }
                    }
                    _ => {}
                }
            }
        }
        let target_plan = plan_target_frame(target, program, &specifications)
            .map_err(BackendFailure::TargetLayout)?;
        let mut slots = HashMap::with_capacity(ordered.len());
        for (logical_index, key) in ordered.iter().copied().enumerate() {
            let pointer = match key {
                FunctionSlot::Address(result) => value_name(result),
                _ => format!("%wf.slot.{logical_index}"),
            };
            if slots
                .insert(
                    key,
                    PlannedFunctionSlot {
                        logical_index,
                        pointer,
                    },
                )
                .is_some()
            {
                return Err(BackendFailure::InvalidIr);
            }
        }
        Ok(Self {
            target: target_plan,
            slots,
            ordered,
        })
    }

    /// The frame of a register-returned definition's public entry: the one
    /// target-qualified slot at [`RESULT_POINTER`] its body constructs the
    /// result in.
    fn returned_value(
        target: TargetLayout,
        program: &IrProgram,
        ty: IrType,
    ) -> Result<Self, BackendFailure> {
        let mut specifications = Vec::new();
        let mut ordered = Vec::new();
        push_function_slot(
            &mut specifications,
            &mut ordered,
            FunctionSlot::Result,
            TargetStorageType::source(ty),
            None,
        )?;
        let target_plan = plan_target_frame(target, program, &specifications)
            .map_err(BackendFailure::TargetLayout)?;
        let slot = PlannedFunctionSlot {
            logical_index: 0,
            pointer: RESULT_POINTER.to_owned(),
        };
        Ok(Self {
            target: target_plan,
            slots: HashMap::from([(FunctionSlot::Result, slot)]),
            ordered,
        })
    }

    fn slot(&self, key: FunctionSlot) -> Result<String, BackendFailure> {
        self.slots
            .get(&key)
            .map(|slot| slot.pointer.clone())
            .ok_or(BackendFailure::InvalidIr)
    }

    fn render(
        &self,
        program: &IrProgram,
        references: &mut References,
    ) -> Result<String, BackendFailure> {
        if self.target.is_empty() {
            return Ok(String::new());
        }
        let fields = self
            .target
            .physical_fields()
            .iter()
            .map(|field| llvm_storage_type_with_references(program, field, &mut references.types))
            .collect::<Result<Vec<_>, _>>()?;
        let mut output = String::new();
        if let Some(alignment) = self.target.independent_slot_alignment() {
            // The complete frame was qualified before this representation
            // choice. Keep each full allocation root, including parents of
            // reused result fields; only unrelated roots gain distinct LLVM
            // allocation provenance. Storage interference is unchanged.
            for key in &self.ordered {
                let slot = self.slots.get(key).ok_or(BackendFailure::InvalidIr)?;
                let field = self
                    .target
                    .logical_field(slot.logical_index)
                    .ok_or(BackendFailure::InvalidIr)?;
                let ty = fields
                    .get(field.physical_index() as usize)
                    .ok_or(BackendFailure::InvalidIr)?;
                writeln!(
                    output,
                    "  {} = alloca {ty}, align {alignment}",
                    slot.pointer
                )
                .map_err(|_| BackendFailure::TextEmission)?;
            }
            return Ok(output);
        }
        let frame_type = format!("{{ {} }}", fields.join(", "));
        writeln!(
            output,
            "  %wf.frame = alloca {frame_type}, align {}",
            self.target.layout().align()
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        for key in &self.ordered {
            let slot = self.slots.get(key).ok_or(BackendFailure::InvalidIr)?;
            let field = self
                .target
                .logical_field(slot.logical_index)
                .ok_or(BackendFailure::InvalidIr)?;
            writeln!(
                output,
                "  {} = getelementptr inbounds {frame_type}, ptr %wf.frame, i32 0, i32 {}",
                slot.pointer,
                field.physical_index()
            )
            .map_err(|_| BackendFailure::TextEmission)?;
        }
        Ok(output)
    }
}

/// Reserves one logical frame slot under its semantic key.
///
/// `alignment` is `None` for the ordinary case, where the storage type's own
/// natural alignment is the slot's. It is `Some` only where an external
/// contract states the alignment the reservation must have, which a byte
/// block's natural alignment of one would otherwise understate.
fn push_function_slot(
    specifications: &mut Vec<TargetFrameSlot>,
    ordered: &mut Vec<FunctionSlot>,
    key: FunctionSlot,
    ty: TargetStorageType,
    alignment: Option<u64>,
) -> Result<(), BackendFailure> {
    if ordered.contains(&key) {
        return Err(BackendFailure::InvalidIr);
    }
    specifications.push(match alignment {
        Some(alignment) => TargetFrameSlot::aligned(ty, alignment),
        None => TargetFrameSlot::natural(ty),
    });
    ordered.push(key);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
struct FunctionEmitter<'program, 'state> {
    program: &'program IrProgram,
    function: &'program IrFunction,
    /// The selected target, for the extents a proved fact states in bytes
    /// (compiler/backend-facts).
    target: TargetLayout,
    window_address_facts: WindowAddressFacts,
    intrinsics: &'state mut BTreeSet<IntrinsicDeclaration>,
    incoming: Vec<Vec<Incoming>>,
    output: FunctionBody,
    /// Stack slot declarations hoisted to the top of the function's entry block.
    ///
    /// A slot is requested where it is used, but a repeated `alloca` grows the
    /// frame once per execution, so a slot inside a loop would grow the frame
    /// without bound. Declaring every slot in the entry block, which runs
    /// exactly once per call, keeps frame size a property of the function
    /// rather than of the iteration count. Stores stay at the use site.
    entry_prelude: String,
    /// The validated physical frame which supplied `entry_prelude` and every
    /// pointer returned to an operation emitter.
    frame: FunctionFramePlan,
    storage: FunctionStoragePlan,
    result_slot: Option<usize>,
    /// Per-operation snapshots for legacy value consumers. Place operations
    /// read their actual storage directly; a snapshot never becomes an alias.
    materialized: HashMap<IrValueId, String>,
    temporary: u32,
    /// The module's outlined thunks, shared by every function that hands a
    /// call out.
    parallel: &'state mut ParallelThunks,
    /// Values whose defining call is handed to a worker lane [PAR-1
    /// candidate], and the values whose definitions are the join sites that
    /// complete them.
    overlap_handed_out: HashSet<IrValueId>,
    overlap_join_sites: HashSet<IrValueId>,
    /// Selected-target layouts of the exact `{ arguments..., result }`
    /// aggregates this world may place in runtime lane storage.
    ///
    /// This map is built before any function text is emitted. Its membership
    /// is therefore also the proof that the aggregate fits the runtime's
    /// 256-byte, 16-byte-aligned slot and the target's address-index domain.
    ordinary_lane_frames: HashMap<IrValueId, TargetAggregateLayout>,
    /// Ordinary calls awaiting the group's join.
    handed_out: Vec<HandedOut>,
    /// The functions that have a sequential clone, when this emitter is
    /// rendering one.
    ///
    /// `None` is the ordinary lowering, which is every function of a default
    /// build and the overlapped half of a `--par` build. `Some` renders the
    /// clone world: no group is actualized, and a call to a function that also
    /// has a clone names the clone, so the world a call lands in is the world
    /// it was made from. The experimental refusal edge may enter a clone
    /// from ordinary code without changing its parameters or result ABI.
    sequential_clones: Option<&'state HashSet<u32>>,
    /// Existing clones callable from the opt-in refused-compute edge.
    refusal_clones: &'state HashSet<u32>,
    frontiers: &'state RecursiveFrontiers,
    /// The budgeted world this emission renders, or `None` for a function no
    /// family holds — which is every function of a default build.
    grain: Option<Grain>,
    /// The caller's remaining budget, as an operand: the value a call that
    /// stays inside this component carries. Fixed for the whole emission.
    grain_next: Option<String>,
}

/// What one function's emission shares with the rest of its module, and the
/// one choice that says which of the module's two worlds it is emitting into.
///
/// These travel together because they are all module-scope: intrinsic
/// declarations and thunks are collected across every function and rendered
/// once at the top, and the clone set is the same set for every function of the
/// module. Passing them as one named group keeps the emitter's own arguments —
/// the program and the function — the ones a reader has to
/// think about.
struct ModuleState<'state> {
    intrinsics: &'state mut BTreeSet<IntrinsicDeclaration>,
    parallel: &'state mut ParallelThunks,
    /// `None` emits the ordinary lowering; `Some` emits the sequential clone.
    sequential_clones: Option<&'state HashSet<u32>>,
    /// Existing clones callable from the opt-in refused-compute edge.
    refusal_clones: &'state HashSet<u32>,
    frontiers: &'state RecursiveFrontiers,
    grain: Option<Grain>,
    window_address_facts: WindowAddressFacts,
}

impl<'program, 'state> FunctionEmitter<'program, 'state> {
    fn new(
        program: &'program IrProgram,
        target: TargetLayout,
        function: &'program IrFunction,
        module: ModuleState<'state>,
    ) -> Result<Self, BackendFailure> {
        let ModuleState {
            intrinsics,
            parallel,
            sequential_clones,
            refusal_clones,
            frontiers,
            grain,
            window_address_facts,
        } = module;
        let mut overlaps = Vec::new();
        let mut ordinary_lane_frames = HashMap::new();
        if sequential_clones.is_none() {
            for overlap in function.overlaps() {
                // One ordinary ABI, with a budget field only for a synthesized variant.
                let Some(frames) =
                    ordinary_overlap_lane_frames(program, target, function, overlap, &|callee| {
                        grain.is_some_and(|grain| frontiers.spends(callee, grain))
                    })?
                else {
                    continue;
                };
                for (result, layout) in frames {
                    if ordinary_lane_frames.insert(result, layout).is_some() {
                        return Err(BackendFailure::InvalidIr);
                    }
                }
                overlaps.push(overlap.clone());
            }
        }
        let overlap_handed_out = overlaps
            .iter()
            .flat_map(|overlap| overlap.handed_out().iter().copied())
            .collect();
        let overlap_join_sites = overlaps
            .iter()
            .filter_map(crate::IrOverlap::join_site)
            .collect();
        let storage =
            FunctionStoragePlan::build_in_world(program, function, sequential_clones.is_some())?;
        let result_slot = places::returned_storage_slot(function, &storage);
        let frame = FunctionFramePlan::build(
            target,
            program,
            function,
            FunctionFrameContents {
                storage: &storage,
                result_slot,
            },
        )?;
        let mut output = FunctionBody::default();
        let mut entry_prelude = frame.render(program, &mut output.references)?;
        entry_prelude.push_str(&contexts::context_group_prelude(function));
        Ok(Self {
            program,
            function,
            target,
            window_address_facts,
            intrinsics,
            incoming: Vec::new(),
            output,
            entry_prelude,
            frame,
            storage,
            result_slot,
            materialized: HashMap::new(),
            temporary: 0,
            parallel,
            overlap_handed_out,
            overlap_join_sites,
            ordinary_lane_frames,
            handed_out: Vec::new(),
            sequential_clones,
            refusal_clones,
            frontiers,
            grain,
            grain_next: None,
        })
    }

    /// The facts the checked program already proved about one reference
    /// parameter, as the target attributes that name them
    /// (compiler/backend-facts).
    ///
    /// A reference is a local name for a path that is live where it is used
    /// [REF-1, REF-2], so it is `nonnull` and `dereferenceable` for the
    /// referent's own extent; [REF-3] keeps it from escaping, so nothing in
    /// the callee captures it; and `noalias` holds because every place the
    /// call writes is reached only through the parameter whose row entry
    /// names it. [EFF-5] runs at every call and rejects any program in which
    /// a place one argument's entries write is not proved disjoint from every
    /// place another argument's entries reach; entries one argument supplies
    /// may overlap each other, but they are all reached through that one
    /// parameter's pointer, which is what LLVM's `noalias` on that parameter
    /// concerns. The attribute is per parameter and no per-access alias scope
    /// is emitted, so nothing here asserts two entries of one parameter
    /// disjoint.
    ///
    /// `swap` is the stated exception: [OP-11] admits the one call whose two
    /// arguments name the same place, so its two parameters carry every fact
    /// but that one.
    ///
    /// A `&[T]` range reference [REF-4] is a reference too, and the facts go
    /// on the element pointer it arrives as. LLVM's `noalias` constrains only
    /// memory the call modifies, and [EFF-5] proved every place one argument
    /// writes disjoint from every place another argument reaches; the callee
    /// reaches caller storage only through its reference parameters, whose
    /// accesses its exact row covers [EFF-2]. Two read-only ranges may
    /// overlap, which `noalias` permits because neither is modified. The
    /// pointer addresses storage that exists while the range is valid, even
    /// for an empty range, so it is `nonnull`. Its extent is `len` elements,
    /// known only at run time and possibly zero, so it states no
    /// `dereferenceable` extent.
    fn reference_parameter_facts(
        &self,
        index: usize,
        ty: IrType,
    ) -> Result<String, BackendFailure> {
        let (mode, referent) = match ty {
            IrType::Address(referent) => (crate::IrSourceMode::Reference, Some(referent)),
            IrType::Range { .. } => (crate::IrSourceMode::Range, None),
            _ => return Ok(String::new()),
        };
        // A waiting function's ramp keeps every reference it is handed in its
        // frame and uses it after it returns, and a waiting host operation's
        // start leaves the reference with the host until its finish, so
        // neither is `nocapture`. A caller that saw one would keep its
        // referent only until the ramp or start returned, where the frame
        // needs it until the callee finishes (`frames`); the other facts
        // describe the ramp alone, so none is stated.
        if self.function.waits() {
            return Ok(String::new());
        }
        // A synthesized function has no source signature, and a fact whose
        // derivation is missing is simply not emitted.
        if self
            .function
            .source_signature()
            .and_then(|signature| signature.parameters().get(index).copied())
            != Some(mode)
        {
            return Ok(String::new());
        }
        let mut facts = String::new();
        if !aliasing_admitted_row(self.function.name()) {
            facts.push_str(" noalias");
        }
        facts.push_str(" nonnull ");
        facts.push_str(NO_CAPTURE_ATTRIBUTE);
        // The referent's own selected-target extent. A shape whose block
        // extends past its statically typed header states only the header it
        // is sure of, which is the direction `dereferenceable` needs.
        if let Some(referent) = referent
            && let Ok(layout) = crate::target::validate_static_storage(
                self.target,
                self.program,
                &crate::target::TargetStorageType::source(referent.ty()),
            )
            && layout.size() > 0
        {
            write!(facts, " dereferenceable({})", layout.size())
                .map_err(|_| BackendFailure::TextEmission)?;
        }
        Ok(facts)
    }

    fn is_overlap_join_site(&self, value: IrValueId) -> bool {
        self.overlap_join_sites.contains(&value)
    }

    /// The symbol one call names, and the budget operand it carries.
    ///
    /// In the clone world a call to a function that also has a clone names the
    /// clone, which is what keeps a clone's whole dynamic extent inside the
    /// sequential world. Inside a budgeted world a call that stays in the
    /// component names the callee's variant and spends one level; a call that
    /// leaves it enters that callee's ordinary symbol, which obtains a budget
    /// of its own. Other calls use the shared original.
    pub(super) fn callee_target(&self, ordinal: u32, name: &str) -> (String, Option<&str>) {
        match (self.sequential_clones, self.grain) {
            (Some(clones), _) if clones.contains(&ordinal) => (sequential_clone_symbol(name), None),
            (None, Some(grain)) => {
                let (symbol, spends) = self.frontiers.callee(ordinal, name, grain);
                (
                    symbol,
                    spends.then_some(self.grain_next.as_deref()).flatten(),
                )
            }
            _ => (source_symbol(name), None),
        }
    }

    pub(super) fn callee_symbol(&self, ordinal: u32, name: &str) -> String {
        self.callee_target(ordinal, name).0
    }

    /// The budget-carrying variant's entry: test the levels this activation
    /// was handed, and enter the sequential clone where none are left.
    ///
    /// One compare and one branch per activation above the cut, and nothing at
    /// all below it — the clone is the world this module already carries for a
    /// run with no pool, so no node under the cut pays a scheduler test, a
    /// null branch or a phi.
    ///
    /// Opening this first block selects it for the frame prelude: an `alloca`
    /// is promotable only in the entry block.
    ///
    /// `public` is the function's own ABI, which the clone's public symbol
    /// keeps. A register-returned clone returns its value there, and this
    /// variant's body stores it through its own destination.
    fn emit_grain_entry(&mut self, public: &FunctionAbi) -> Result<(), BackendFailure> {
        if self.grain.is_none() {
            return Ok(());
        }
        // A branch into the body needs the body to be enterable from one more
        // place. It always is: an IR entry block that were a jump target would
        // already carry phis in an LLVM entry block, which is malformed.
        if self.incoming.first().is_some_and(|edges| !edges.is_empty()) {
            return Err(BackendFailure::InvalidIr);
        }
        let arguments = ordinary_call_arguments(self.program, self.function, public)?;
        let spent = RecursiveFrontiers::exhausted(self.function.name());
        let body = block_label(IrBlockId::from_index(0).ok_or(BackendFailure::InvalidIr)?);
        self.output.open_block(GRAIN_ENTRY_LABEL.to_owned());
        {
            write!(self.output, "  %wf.budget.next = sub i64 %wf.budget, 1\n  %wf.grain = icmp sgt i64 %wf.budget, 0\n  br i1 %wf.grain, label %{body}, label %{GRAIN_SPENT_LABEL}\n").map_err(|_| BackendFailure::TextEmission)?;
            self.output.open_block(GRAIN_SPENT_LABEL.to_string());
        };
        match public.result() {
            ResultAbi::Destination(_) => {
                {
                    self.output.symbol(spent.to_string());
                    writeln!(self.output, "  call void @{spent}({arguments})\n  ret void")
                }
                .map_err(|_| BackendFailure::TextEmission)?;
            }
            ResultAbi::StoredValue(ty) => {
                let result = self.output.type_name(self.program, ty)?;
                {
                    self.output.symbol(spent.to_string());
                    writeln!(
                        self.output,
                        "  %wf.spent = call {result} @{spent}({arguments})\n  \
                     store {result} %wf.spent, ptr {RESULT_POINTER}\n  ret void"
                    )
                }
                .map_err(|_| BackendFailure::TextEmission)?;
            }
            ResultAbi::Value(ty) => {
                let result = self.output.type_name(self.program, ty)?;
                self.output.symbol(spent.to_string());
                writeln!(
                    self.output,
                    "  %wf.spent = call {result} @{spent}({arguments})\n  ret {result} %wf.spent"
                )
                .map_err(|_| BackendFailure::TextEmission)?;
            }
        }
        self.grain_next = Some("%wf.budget.next".to_owned());
        Ok(())
    }

    fn emit(mut self) -> Result<Module, BackendFailure> {
        let declaration = self.function.blocks().is_empty();
        let reachable = if declaration {
            Vec::new()
        } else {
            self.reachable_blocks()?
        };
        self.incoming = self.collect_incoming(&reachable)?;
        // A declaration names a linked definition by its public ABI. A
        // definition whose result returns in registers is emitted as its
        // destination-form body under an internal symbol, followed by the
        // public entry that returns the value.
        let public = FunctionAbi::build(self.program, self.function)?;
        // A waiting function is a resumable frame [WAIT-1]: its result is
        // always constructed through a destination and it has no public
        // entry (`frames`). A budgeted variant has no frame form.
        let waiting = self.function.waits();
        if waiting && self.grain.is_some() {
            return Err(BackendFailure::InvalidIr);
        }
        let entry =
            !waiting && !declaration && matches!(public.result(), ResultAbi::StoredValue(_));
        let abi = if waiting {
            public.waiting()
        } else if entry {
            public.body()
        } else {
            public.clone()
        };
        let symbol = match (self.sequential_clones, self.grain) {
            (Some(_), _) => sequential_clone_symbol(self.function.name()),
            (None, Some(_)) => recursion_budget_symbol(self.function.name()),
            (None, None) => source_symbol(self.function.name()),
        };
        let body_symbol = if entry {
            result_body_symbol(&symbol)
        } else {
            symbol.clone()
        };
        let (mut parameters, mut references) = self.signature_parameters(&abi)?;
        let result = if abi.result().uses_destination() {
            "void".to_owned()
        } else {
            llvm_type_with_references(self.program, abi.result().ty(), &mut references.types)?
        };
        if abi.result().uses_destination() && (declaration || !waiting) {
            parameters.insert(0, Parameter::named("ptr", RESULT_POINTER));
        }
        let mut module = Module::default();
        if waiting && declaration {
            // A waiting host function is linked as its `.start` and `.finish`.
            module.append(frames::host_declarations(&body_symbol, &parameters));
            module.text("\n");
            return Ok(module);
        }
        let mut signature = if waiting {
            frames::waiting_signature(body_symbol.clone(), parameters)
        } else {
            Signature::new(body_symbol.clone(), result, parameters)
        };
        signature.references = references;
        if entry {
            signature.linkage = Linkage::Internal;
        }
        if declaration {
            // Linked declarations retain their parameter names in whole-module
            // output; cross-fragment declarations are name-free.
            module.declare_named(signature);
            module.text("\n");
            return Ok(module);
        }
        // A range reference arrives as its element pointer and count; the
        // body reads its ordinary `{ ptr, i64 }` pair, reassembled once in
        // the entry block beside the frame's slots, where it dominates every
        // use, including a self-tail transfer's parameterized body entry.
        for ((value, _), parameter) in self.function.parameters().iter().zip(abi.parameters()) {
            if !parameter.is_range() {
                continue;
            }
            let (pointer, count) = incoming_range_parts(*value);
            let pair = self.output.type_name(self.program, parameter.ty())?;
            let name = value_name(*value);
            writeln!(
                self.entry_prelude,
                "  {name}.data = insertvalue {pair} poison, ptr {pointer}, 0\n  \
                 {name} = insertvalue {pair} {name}.data, i64 {count}, 1"
            )
            .map_err(|_| BackendFailure::TextEmission)?;
        }
        // The variant's hidden trailing budget. Legal because this definition
        // is synthesized and is named by no source call: a writer's own
        // signature is the entry's, which is emitted unchanged.
        if self.grain.is_some() {
            signature
                .parameters
                .push(Parameter::named("i64", "%wf.budget"));
        }
        self.emit_grain_entry(&public)?;
        if waiting {
            self.emit_frame_entry()?;
        }
        for (index, block) in self.function.blocks().iter().enumerate() {
            if !reachable[index] {
                continue;
            }
            self.materialized.clear();
            let block_id = IrBlockId::from_index(index).ok_or(BackendFailure::CounterOverflow)?;
            self.output.open_block(block_label(block_id));
            self.emit_block_parameters(block_id, block)?;
            if index == 0 {
                // A result can alias any consumed caller input. Snapshot
                // every other indirect input first, then initialize the one
                // entry group using the result. Scalar/address parameters
                // already arrived as SSA values before either pass.
                for writes_result in [false, true] {
                    for ((value, _), parameter) in
                        self.function.parameters().iter().zip(abi.parameters())
                    {
                        let uses_result = self.result_slot.is_some_and(|result_slot| {
                            self.storage.slot(*value).is_some_and(|slot| {
                                self.storage.allocation_root(slot) == result_slot
                            })
                        });
                        if !parameter.is_indirect() || uses_result != writes_result {
                            continue;
                        }
                        let destination = self.value_place(*value)?;
                        self.copy_storage(
                            parameter.ty(),
                            &format!("%wf.arg.v{}", value.ordinal()),
                            &destination,
                        )?;
                    }
                }
                if waiting {
                    self.emit_frame_start()?;
                }
            }
            for (instruction_index, instruction) in block.instructions().iter().enumerate() {
                self.emit_instruction(block_id, instruction_index, instruction)?;
            }
            self.emit_terminator(block_id, block.terminator())?;
            self.output.finish_ir_block(block_id)?;
        }
        if waiting {
            self.emit_frame_exit()?;
        }
        if entry {
            let public_entry = self.public_entry(&symbol, &body_symbol, &public, &abi)?;
            module.define(signature.define(self.output, &self.entry_prelude)?);
            module.text("\n");
            module.append(public_entry);
        } else {
            module.define(signature.define(self.output, &self.entry_prelude)?);
            module.text("\n");
        }
        Ok(module)
    }

    /// Every parameter of this definition's signature, with the facts the
    /// checked program proved about each (compiler/backend-facts), and
    /// without the destination pointer or a variant's budget.
    fn signature_parameters(
        &self,
        abi: &FunctionAbi,
    ) -> Result<(Vec<Parameter>, References), BackendFailure> {
        let mut parameters = Vec::with_capacity(abi.parameters().len());
        let mut references = References::default();
        for (index, ((value, _), parameter)) in self
            .function
            .parameters()
            .iter()
            .zip(abi.parameters())
            .enumerate()
        {
            let facts = self.reference_parameter_facts(index, parameter.ty())?;
            parameters.extend(incoming_parameters(
                self.program,
                *value,
                *parameter,
                &facts,
                &mut references,
            )?);
        }
        Ok((parameters, references))
    }

    /// A register-returned definition's public entry: the slot its body
    /// constructs the result in, the call, and the loaded value.
    ///
    /// The body is internal, so the entry is its one caller, and it is
    /// never marked always-inline. The host therefore simplifies the body
    /// in its destination form before it inlines the body here. An
    /// always-inline body would be merged before that simplification and
    /// would lose the loop shapes the destination form gives it.
    fn public_entry(
        &self,
        symbol: &str,
        body_symbol: &str,
        public: &FunctionAbi,
        body: &FunctionAbi,
    ) -> Result<Module, BackendFailure> {
        let ty = public.result().ty();
        let (mut parameters, mut references) = self.signature_parameters(body)?;
        let result = llvm_type_with_references(self.program, ty, &mut references.types)?;
        let frame = FunctionFramePlan::returned_value(self.target, self.program, ty)?
            .render(self.program, &mut references)?;
        let mut arguments = ordinary_call_arguments(self.program, self.function, body)?;
        if self.grain.is_some() {
            parameters.push(Parameter::named("i64", "%wf.budget"));
            arguments.push_str(", i64 %wf.budget");
        }
        let mut signature = Signature::new(symbol, result.clone(), parameters);
        signature.references = references;
        let mut output = FunctionBody::default();
        output.open_block("entry".to_owned());
        output.symbol(body_symbol);
        write!(output, "  call void @{body_symbol}({arguments})\n  %wf.returned = load {result}, ptr {RESULT_POINTER}\n  ret {result} %wf.returned\n")
            .map_err(|_| BackendFailure::TextEmission)?;
        let mut module = Module::default();
        module.define(signature.define(output, &frame)?);
        module.text("\n");
        Ok(module)
    }

    /// Source checking retains the conservative continuation of every loop
    /// [FN-1]. An executable loop with no break has no edge to that block,
    /// which may nevertheless carry lowered parameters and more dead CFG.
    /// Emit only the entry-reachable graph: a predecessor-free phi is not
    /// LLVM, and a dead cycle must not supply an incoming value to a live phi.
    fn reachable_blocks(&self) -> Result<Vec<bool>, BackendFailure> {
        let mut reachable = vec![false; self.function.blocks().len()];
        let mut pending = vec![0_usize];
        while let Some(index) = pending.pop() {
            let visited = reachable.get_mut(index).ok_or(BackendFailure::InvalidIr)?;
            if *visited {
                continue;
            }
            *visited = true;
            match self.function.blocks()[index].terminator() {
                IrTerminator::Jump { target, .. } => pending.push(target.index()),
                IrTerminator::Match { targets, .. } => {
                    pending.extend(targets.iter().map(|target| target.block().index()));
                }
                IrTerminator::Return { .. } | IrTerminator::Unreachable => {}
            }
        }
        Ok(reachable)
    }

    fn collect_incoming(&self, reachable: &[bool]) -> Result<Vec<Vec<Incoming>>, BackendFailure> {
        let mut incoming = vec![Vec::new(); self.function.blocks().len()];
        for (index, block) in self.function.blocks().iter().enumerate() {
            if !reachable[index] {
                continue;
            }
            if let IrTerminator::Jump {
                target, arguments, ..
            } = block.terminator()
            {
                let predecessor =
                    IrBlockId::from_index(index).ok_or(BackendFailure::CounterOverflow)?;
                incoming
                    .get_mut(target.index())
                    .ok_or(BackendFailure::InvalidIr)?
                    .push(Incoming {
                        predecessor,
                        arguments: arguments.clone(),
                    });
            }
        }
        Ok(incoming)
    }

    fn emit_block_parameters(
        &mut self,
        block_id: IrBlockId,
        block: &IrBlock,
    ) -> Result<(), BackendFailure> {
        if block.parameters().is_empty() {
            return Ok(());
        }
        let incoming = self
            .incoming
            .get(block_id.index())
            .ok_or(BackendFailure::InvalidIr)?;
        if (incoming.is_empty() && block_id.index() == 0)
            || incoming
                .iter()
                .any(|edge| edge.arguments.len() != block.parameters().len())
        {
            return Err(BackendFailure::InvalidIr);
        }
        for (parameter_index, (parameter, ty)) in block.parameters().iter().enumerate() {
            if self.storage.slot(*parameter).is_some() {
                continue;
            }
            if incoming.is_empty() {
                // A loop with returns but no break leaves an unreachable
                // structural exit. Its block parameters have no incoming
                // values: define them locally so its checked continuation
                // remains valid LLVM without inventing a predecessor edge.
                {
                    let emitted_type_1 = self.output.type_name(self.program, *ty)?;
                    writeln!(
                        self.output,
                        "  {} = freeze {} poison",
                        self.value_name(*parameter),
                        emitted_type_1
                    )
                }
                .map_err(|_| BackendFailure::TextEmission)?;
                continue;
            }
            {
                let emitted_type_1 = self.output.type_name(self.program, *ty)?;
                write!(
                    self.output,
                    "  {} = phi {} ",
                    self.value_name(*parameter),
                    emitted_type_1
                )
            }
            .map_err(|_| BackendFailure::TextEmission)?;
            for (edge_index, edge) in incoming.iter().enumerate() {
                let argument = *edge
                    .arguments
                    .get(parameter_index)
                    .ok_or(BackendFailure::InvalidIr)?;
                if self.value_type(argument) != Some(*ty) {
                    return Err(BackendFailure::InvalidIr);
                }
                if edge_index != 0 {
                    self.output.push_str(", ");
                }
                self.output
                    .incoming(self.value_name(argument), edge.predecessor);
            }
            self.output.push('\n');
        }
        Ok(())
    }

    /// Emit one ordinary instruction.
    fn emit_instruction(
        &mut self,
        block: IrBlockId,
        index: usize,
        instruction: &IrInstruction,
    ) -> Result<(), BackendFailure> {
        match instruction {
            IrInstruction::StoreSlice {
                slice,
                index,
                value: _,
            } => {
                self.materialize_operands([*slice, *index])?;
            }
            IrInstruction::Store { address, .. } => {
                // A stored aggregate is transferred from its backing below.
                // Loading it into SSA first lets SROA expand a large array
                // into one load/store pair per element before the copy site
                // can be recovered. Scalar and descriptor values have no
                // backing slot and continue through their ordinary operand.
                self.materialize_operands([*address])?;
            }
            IrInstruction::Drops(_) => {}
            IrInstruction::Define { .. } => {}
        }
        self.emit_instruction_body(block, index, instruction)?;
        Ok(())
    }

    fn emit_instruction_body(
        &mut self,
        _block: IrBlockId,
        _index: usize,
        instruction: &IrInstruction,
    ) -> Result<(), BackendFailure> {
        // A group's join rides the definition of its last member: the members
        // before it were handed out and their values do not exist until here.
        if let IrInstruction::Define { result, .. } = instruction
            && self.is_overlap_join_site(*result)
        {
            self.emit_definition_then_join(instruction, *result)?;
            return Ok(());
        }
        match instruction {
            IrInstruction::Define {
                result,
                ty,
                operation,
            } => self.emit_definition(*result, *ty, operation),
            IrInstruction::StoreSlice {
                slice,
                index,
                value,
            } => self.emit_slice_store(*slice, *index, *value),
            IrInstruction::Store {
                address,
                value,
                referent,
            } => self.emit_store(*address, *value, *referent),
            IrInstruction::Drops(drops) => self.emit_drops(drops),
        }
    }

    /// Emits the last member of an overlap group and then its joins.
    fn emit_definition_then_join(
        &mut self,
        instruction: &IrInstruction,
        result: IrValueId,
    ) -> Result<(), BackendFailure> {
        let IrInstruction::Define { ty, operation, .. } = instruction else {
            return Err(BackendFailure::InvalidIr);
        };
        self.emit_definition(result, *ty, operation)?;
        self.emit_overlap_joins(result)
    }

    fn emit_definition(
        &mut self,
        result: IrValueId,
        ty: IrType,
        operation: &IrOperation,
    ) -> Result<(), BackendFailure> {
        self.materialized.clear();
        if self.emit_place_definition(result, ty, operation)? {
            return Ok(());
        }
        match operation {
            // These operations transfer their payload from storage when its
            // representation is stored. Materializing that payload as an SSA
            // aggregate first lets SROA scalarize large arrays before the
            // typed storage copy can be emitted.
            IrOperation::BoxNew { .. } => {}
            IrOperation::BufferFill { length, .. } => {
                self.materialize_operands([*length])?;
            }
            IrOperation::RunInsert { run, index, .. } => {
                self.materialize_operands([*run, *index])?;
            }
            _ => self.materialize_operands(operation.operands())?,
        }
        self.emit_value_definition(result, ty, operation)?;
        if !self.overlap_handed_out.contains(&result) {
            self.save_value_result(result)?;
        }
        Ok(())
    }

    fn emit_value_definition(
        &mut self,
        result: IrValueId,
        ty: IrType,
        operation: &IrOperation,
    ) -> Result<(), BackendFailure> {
        if self.value_type(result) != Some(ty) {
            return Err(BackendFailure::InvalidIr);
        }
        match operation {
            IrOperation::Constant(constant) => self.emit_constant(result, ty, *constant),
            IrOperation::Call {
                function,
                arguments,
            } => {
                if self.overlap_handed_out.contains(&result) {
                    self.emit_handed_out_call(result, ty, *function, arguments)
                } else {
                    self.emit_call(result, ty, *function, arguments)
                }
            }
            IrOperation::ContextStart {
                function,
                arguments,
            } => self.emit_context_start(result, *function, arguments, false),
            IrOperation::ContextStartBound {
                function,
                arguments,
            } => self.emit_context_start(result, *function, arguments, true),
            IrOperation::ContextAwait { start } => self.emit_context_await(result, ty, *start),
            IrOperation::ContextJoin => self.emit_context_join(result),
            IrOperation::LoopSplit {
                splitter,
                chunk,
                seed,
                lower,
                upper,
                captures,
                weight,
                work,
            } => self.emit_loop_split(
                result,
                ty,
                &LoopSplitSite {
                    splitter: *splitter,
                    chunk: *chunk,
                    seed: *seed,
                    lower: *lower,
                    upper: *upper,
                    captures,
                    weight: *weight,
                    work: work.as_ref(),
                },
            ),
            IrOperation::Integer {
                operation,
                operand_type,
                arguments,
            } => self.emit_integer(result, ty, *operation, *operand_type, arguments),
            IrOperation::Float {
                operation,
                operand_type,
                arguments,
            } => self.emit_float(result, ty, *operation, *operand_type, arguments),
            IrOperation::NumericConversion {
                mode,
                source_type,
                destination_type,
                value,
            } => self.emit_numeric_conversion(
                result,
                ty,
                *mode,
                *source_type,
                *destination_type,
                *value,
            ),
            IrOperation::Reinterpret {
                source_type,
                destination_type,
                value,
            } => self.emit_reinterpret(result, ty, *source_type, *destination_type, *value),
            IrOperation::Boolean {
                operation,
                arguments,
            } => self.emit_boolean(result, ty, *operation, arguments),
            IrOperation::EnumEquality {
                equal,
                operand_type,
                arguments,
            } => self.emit_enum_equality(result, ty, *equal, *operand_type, *arguments),
            IrOperation::ArrayFill {
                value,
                target_domain,
            } => self.emit_array_fill(result, ty, *value, *target_domain),
            IrOperation::FullArrayConversion { value } => {
                self.emit_full_array_conversion(result, ty, *value)
            }
            IrOperation::ArrayIndex {
                root,
                offset,
                target_domain,
            } => self.emit_array_index(result, ty, *root, *offset, *target_domain),
            IrOperation::BufferFill {
                nominal,
                length,
                value,
                layout_ceiling,
                target_domains,
            } => self.emit_buffer_fill(
                result,
                ty,
                *nominal,
                *length,
                *value,
                IrAllocationObligations {
                    layout_ceiling: *layout_ceiling,
                    target_domains: *target_domains,
                },
            ),
            IrOperation::BufferMeasure { buffer } => self.emit_buffer_length(result, ty, *buffer),
            IrOperation::Window => self.emit_fixed_vector(result, ty),
            IrOperation::ContainerMeasure { measure, container } => {
                self.emit_container_measure(result, ty, *measure, *container)
            }
            IrOperation::RunIndex {
                run,
                offset,
                target_domain,
            } => self.emit_run_index(result, ty, *run, *offset, *target_domain),
            IrOperation::RunTaken { row, run } => self.emit_run_taken(result, ty, *row, *run),
            IrOperation::RunBoundary { row, run, value } => {
                self.emit_run_boundary(result, ty, *row, *run, *value)
            }
            IrOperation::RunShift { run, index, open } => {
                self.emit_run_shift(result, ty, *run, *index, *open)
            }
            IrOperation::RunInsert { run, index, value } => {
                self.emit_run_insert(result, ty, *run, *index, *value)
            }
            IrOperation::RunTransfer {
                destination,
                source,
                index,
            } => self.emit_run_transfer(result, ty, *destination, *source, *index),
            IrOperation::WindowBlockNew {
                nominal,
                capacity,
                obligations,
            } => self.emit_window_block_new(result, ty, *nominal, *capacity, *obligations),
            IrOperation::WindowGrow {
                nominal,
                cell,
                capacity,
                obligations,
            } => self.emit_window_grow(result, ty, *nominal, *cell, *capacity, *obligations),
            IrOperation::CellFree { nominal, value } => {
                self.emit_cell_free(result, ty, *nominal, *value)
            }
            IrOperation::BufferIndex {
                buffer,
                offset,
                target_domain,
            } => self.emit_buffer_index(result, ty, *buffer, *offset, *target_domain),
            IrOperation::BufferProbeSkip {
                buffer,
                index,
                limit,
                needles,
            } => self.emit_buffer_probe_skip(result, ty, *buffer, *index, *limit, needles),
            IrOperation::SliceFromBuffer { buffer } => {
                self.emit_slice_from_buffer(result, ty, *buffer)
            }
            IrOperation::SliceFromRun { run } => self.emit_slice_from_run(result, ty, *run),
            IrOperation::SliceRange { slice, start, end } => {
                self.emit_slice_range(result, ty, *slice, *start, *end)
            }
            IrOperation::SliceMeasure { slice } => self.emit_slice_length(result, ty, *slice),
            IrOperation::SliceIndex {
                slice,
                offset,
                target_domain,
            } => self.emit_slice_index(result, ty, *slice, *offset, *target_domain),
            IrOperation::SliceAddress {
                slice,
                offset,
                target_domain,
            } => self.emit_slice_address(result, ty, *slice, *offset, *target_domain),
            IrOperation::BoxNew { nominal, value } => {
                self.emit_box_new(result, ty, *nominal, *value)
            }
            IrOperation::SharedNew { nominal } => self.emit_shared_new(result, ty, *nominal),
            IrOperation::SharedState { nominal, object } => {
                self.emit_shared_state(result, ty, *nominal, *object)
            }
            IrOperation::SharedRetain { nominal, object } => {
                self.emit_shared_retain(result, ty, *nominal, *object)
            }
            IrOperation::SharedAcquire { object } => {
                self.emit_shared_wait(result, *object, "wf__shared_acquire", "acquire")
            }
            IrOperation::SharedWatch { object } => {
                self.emit_shared_wait(result, *object, "wf__shared_watch", "watch")
            }
            IrOperation::SharedUnlock { object } => self.emit_shared_unlock(result, *object),
            IrOperation::BoxTake { nominal, value } => {
                self.emit_box_take(result, ty, *nominal, *value)
            }
            IrOperation::BoxDeref { nominal, value } => {
                self.emit_box_deref(result, ty, *nominal, *value)
            }
            IrOperation::RuntimeBoxPayload { nominal, owner } => {
                self.emit_runtime_box_payload(result, ty, *nominal, *owner)
            }
            IrOperation::RuntimeBoxOwner { nominal, payload } => {
                self.emit_runtime_box_owner(result, ty, *nominal, *payload)
            }
            IrOperation::ConstructStruct { nominal, fields } => {
                self.emit_struct(result, ty, *nominal, fields)
            }
            IrOperation::ConstructEnum {
                nominal,
                variant,
                fields,
            } => self.emit_enum(result, ty, *nominal, *variant, fields),
            IrOperation::ProjectStruct {
                aggregate,
                nominal,
                field,
                consume_root,
            } => {
                self.emit_struct_projection(result, ty, *aggregate, *nominal, *field, *consume_root)
            }
            IrOperation::InsertStruct {
                aggregate,
                nominal,
                field,
                value,
            } => self.emit_struct_insertion(result, ty, *aggregate, *nominal, *field, *value),
            IrOperation::ProjectVariant {
                aggregate,
                nominal,
                variant,
                field,
            } => self.emit_variant_projection(result, ty, *aggregate, *nominal, *variant, *field),
            IrOperation::AddressOf { value, referent } => {
                self.emit_address_of(result, ty, *value, *referent)
            }
            IrOperation::ConstantAddress { constant } => {
                let global = self
                    .program
                    .constant(*constant)
                    .ok_or(BackendFailure::InvalidIr)?;
                if !matches!(ty, IrType::Address(referent) if referent.ty() == global.ty()) {
                    return Err(BackendFailure::InvalidIr);
                }
                {
                    let emitted_type_1 = self.output.type_name(self.program, global.ty())?;
                    self.output
                        .symbol(format!(".wf_const.{}", global.link_name()));
                    writeln!(
                        self.output,
                        "  {} = getelementptr inbounds {}, ptr {}, i64 0",
                        self.value_name(result),
                        emitted_type_1,
                        constant_symbol(global)
                    )
                }
                .map_err(|_| BackendFailure::TextEmission)
            }
            IrOperation::ProjectAddress {
                address,
                projection,
            } => self.emit_project_address(result, ty, *address, projection),
            IrOperation::Load { address, referent } => {
                self.emit_load(result, ty, *address, *referent)
            }
        }
    }

    fn emit_terminator(
        &mut self,
        block: IrBlockId,
        terminator: &IrTerminator,
    ) -> Result<(), BackendFailure> {
        match terminator {
            IrTerminator::Unreachable => {
                writeln!(self.output, "  unreachable").map_err(|_| BackendFailure::TextEmission)
            }
            IrTerminator::Jump {
                target,
                arguments,
                drops,
            } => {
                let target_block = self.block(*target)?;
                if target_block.parameters().len() != arguments.len() {
                    return Err(BackendFailure::InvalidIr);
                }
                for (argument, (_, ty)) in arguments.iter().zip(target_block.parameters()) {
                    if self.value_type(*argument) != Some(*ty) {
                        return Err(BackendFailure::InvalidIr);
                    }
                }
                self.emit_place_edge(*target, arguments, drops)?;
                writeln!(self.output, "  br label %{}", block_label(*target))
                    .map_err(|_| BackendFailure::TextEmission)
            }
            IrTerminator::Return { value, drops } if self.function.waits() => {
                let abi = frames::waiting_abi(self.program, self.function)?;
                if self.value_type(*value) != Some(abi.result().ty()) {
                    return Err(BackendFailure::InvalidIr);
                }
                self.store_value_at(*value, RESULT_POINTER)?;
                self.emit_drops(drops)?;
                self.emit_frame_return()
            }
            IrTerminator::Return { value, drops } => {
                // A register-returned result's body constructs it through
                // its destination; only the public entry returns the value.
                let abi = FunctionAbi::build(self.program, self.function)?.body();
                if self.value_type(*value) != Some(abi.result().ty()) {
                    return Err(BackendFailure::InvalidIr);
                }
                if abi.result().uses_destination() {
                    self.store_value_at(*value, RESULT_POINTER)?;
                    self.emit_drops(drops)?;
                    return writeln!(self.output, "  ret void")
                        .map_err(|_| BackendFailure::TextEmission);
                }
                self.emit_drops(drops)?;
                {
                    let emitted_type_0 = self.output.type_name(self.program, abi.result().ty())?;
                    writeln!(
                        self.output,
                        "  ret {} {}",
                        emitted_type_0,
                        self.value_name(*value)
                    )
                }
                .map_err(|_| BackendFailure::TextEmission)
            }
            IrTerminator::Match {
                scrutinee,
                enum_type,
                targets,
            } => {
                self.materialize_operands([*scrutinee])?;
                let (tag, tag_ty) = self.match_tag(*scrutinee, *enum_type)?;
                writeln!(
                    self.output,
                    "  switch {tag_ty} {tag}, label %{} [",
                    invalid_tag_label(block)
                )
                .map_err(|_| BackendFailure::TextEmission)?;
                let mut seen = BTreeSet::new();
                for target in targets {
                    if !seen.insert(target.tag()) {
                        return Err(BackendFailure::InvalidIr);
                    }
                    writeln!(
                        self.output,
                        "    {tag_ty} {}, label %{}",
                        target.tag(),
                        block_label(target.block())
                    )
                    .map_err(|_| BackendFailure::TextEmission)?;
                }
                {
                    let emission_argument_0 = invalid_tag_label(block);

                    writeln!(self.output, "  ]").map_err(|_| BackendFailure::TextEmission)?;
                    self.output.open_block(emission_argument_0.to_string());
                    {
                        self.output.symbol("abort");
                        write!(self.output, "  call void @abort()\n  unreachable\n")
                    }?;
                    Ok::<_, BackendFailure>(())
                }
            }
        }
    }

    fn match_tag(
        &mut self,
        scrutinee: IrValueId,
        enum_type: IrEnumType,
    ) -> Result<(String, String), BackendFailure> {
        match enum_type {
            IrEnumType::Bool => {
                if self.value_type(scrutinee) != Some(IrType::Bool) {
                    return Err(BackendFailure::InvalidIr);
                }
                Ok((self.value_name(scrutinee), "i1".to_owned()))
            }
            IrEnumType::Nominal(nominal) => {
                // Matching through a reference leaves the scrutinee live
                // [OWN-13], so the operand is the address of the enum's own
                // storage rather than a copy of it, and the tag is read from
                // that storage.
                let addressed = match self.value_type(scrutinee) {
                    Some(IrType::Nominal(actual)) if actual == nominal => false,
                    Some(IrType::Address(IrAddressed::Nominal(actual))) if actual == nominal => {
                        true
                    }
                    _ => return Err(BackendFailure::InvalidIr),
                };
                let data = self.nominal(nominal)?;
                let IrNominalKind::Enum { .. } = data.kind() else {
                    return Err(BackendFailure::InvalidIr);
                };
                let tag_only = data.is_tag_only_enum();
                let enum_llvm = self
                    .output
                    .type_name(self.program, IrType::Nominal(nominal))?;
                let tag_ty = if tag_only {
                    enum_llvm.clone()
                } else {
                    "i32".to_owned()
                };
                if !addressed {
                    if tag_only {
                        return Ok((self.value_name(scrutinee), tag_ty));
                    }
                    let temporary = self.next_temporary()?;
                    writeln!(
                        self.output,
                        "  %{temporary} = extractvalue {enum_llvm} {}, 0",
                        self.value_name(scrutinee)
                    )
                    .map_err(|_| BackendFailure::TextEmission)?;
                    return Ok((format!("%{temporary}"), tag_ty));
                }
                let address = self.value_name(scrutinee);
                let temporary = self.next_temporary()?;
                if tag_only {
                    writeln!(self.output, "  %{temporary} = load {tag_ty}, ptr {address}")
                        .map_err(|_| BackendFailure::TextEmission)?;
                } else {
                    let field =
                        self.aggregate_field_pointer(IrType::Nominal(nominal), &address, 0)?;
                    writeln!(self.output, "  %{temporary} = load i32, ptr {field}")
                        .map_err(|_| BackendFailure::TextEmission)?;
                }
                Ok((format!("%{temporary}"), tag_ty))
            }
        }
    }

    fn nominal(&self, id: IrNominalId) -> Result<&IrNominal, BackendFailure> {
        self.program.nominal(id).ok_or(BackendFailure::InvalidIr)
    }

    fn block(&self, id: IrBlockId) -> Result<&IrBlock, BackendFailure> {
        self.function
            .blocks()
            .get(id.index())
            .ok_or(BackendFailure::InvalidIr)
    }

    /// Validate the checked release and capture only content it actually
    /// reads. In particular, a no-op owner node requires no aggregate load.
    fn prepare_drop(&mut self, drop: IrDrop) -> Result<Option<String>, BackendFailure> {
        let actual = match drop.subject() {
            IrDropSubject::Value(value) => self.value_type(value),
            IrDropSubject::Place(address) => match self.value_type(address) {
                Some(IrType::Address(referent)) => Some(referent.ty()),
                _ => return Err(BackendFailure::InvalidIr),
            },
        };
        if actual != Some(drop.ty()) {
            return Err(BackendFailure::InvalidIr);
        }
        let reads_content = match drop.ty() {
            IrType::Range { .. } => false,
            IrType::Array { .. } | IrType::Window { .. } => {
                type_requires_cleanup(self.program, drop.ty())?
            }
            IrType::Buffer { .. } => true,
            IrType::Nominal(nominal) if !self.nominal(nominal)?.is_tag_only_enum() => {
                match self.nominal(nominal)?.kind() {
                    // The checker supplied separate component records. The
                    // struct node must not recursively release them again.
                    IrNominalKind::Struct { .. } => false,
                    IrNominalKind::Opaque => false,
                    IrNominalKind::Shared { .. } => true,
                    IrNominalKind::Enum { .. } | IrNominalKind::Box { .. } => {
                        type_requires_cleanup(self.program, drop.ty())?
                    }
                }
            }
            _ => return Err(BackendFailure::InvalidIr),
        };
        if !reads_content {
            return Ok(None);
        }
        match drop.subject() {
            IrDropSubject::Value(value) => self.value_operand(value).map(Some),
            IrDropSubject::Place(address) => {
                let snapshot = format!("%{}", self.next_temporary()?);
                {
                    let emitted_type_0 = self.output.type_name(self.program, drop.ty())?;
                    writeln!(
                        self.output,
                        "  {snapshot} = load {}, ptr {}",
                        emitted_type_0,
                        self.value_name(address)
                    )
                }
                .map_err(|_| BackendFailure::TextEmission)?;
                Ok(Some(snapshot))
            }
        }
    }

    fn emit_drops(&mut self, drops: &[IrDrop]) -> Result<(), BackendFailure> {
        // Capture the complete edge/group before its first effectful release,
        // just as lowering's former value snapshots did. Phi inputs are also
        // captured before this group; destination writes follow it.
        let snapshots = drops
            .iter()
            .map(|drop| self.prepare_drop(*drop))
            .collect::<Result<Vec<_>, _>>()?;
        for (drop, snapshot) in drops.iter().zip(snapshots) {
            if let Some(value) = snapshot {
                emit_value_cleanup(
                    self.program,
                    &mut self.output,
                    &mut self.temporary,
                    drop.ty(),
                    value,
                )?;
            }
            writeln!(self.output, "  ; drop {}", value_name(drop.operand()))
                .map_err(|_| BackendFailure::TextEmission)?;
        }
        Ok(())
    }

    /// Returns the address assigned by the already validated physical frame.
    /// No operation emitter can create storage of its own.
    fn entry_slot(&self, key: FunctionSlot) -> Result<String, BackendFailure> {
        self.frame.slot(key)
    }

    fn next_temporary(&mut self) -> Result<String, BackendFailure> {
        let current = self.temporary;
        self.temporary = self
            .temporary
            .checked_add(1)
            .ok_or(BackendFailure::CounterOverflow)?;
        Ok(format!("t{current}"))
    }

    fn value_type(&self, value: IrValueId) -> Option<IrType> {
        self.function.value_type(value)
    }

    fn value_name(&self, value: IrValueId) -> String {
        self.materialized
            .get(&value)
            .cloned()
            .unwrap_or_else(|| value_name(value))
    }
}

/// Fit every ordinary call's frame before selecting an overlap group.
fn ordinary_overlap_lane_frames(
    program: &IrProgram,
    target: TargetLayout,
    function: &IrFunction,
    overlap: &IrOverlap,
    carries_budget: &dyn Fn(u32) -> bool,
) -> Result<Option<Vec<(IrValueId, TargetAggregateLayout)>>, BackendFailure> {
    let mut frames = Vec::with_capacity(overlap.handed_out().len());
    for member in overlap.handed_out() {
        let Some(IrOperation::Call {
            function: ordinal, ..
        }) = definition_operation(function, *member)
        else {
            return Ok(None);
        };
        let ordinal = *ordinal;
        let callee = program
            .functions()
            .get(ordinal as usize)
            .ok_or(BackendFailure::InvalidIr)?;
        let Some(layout) = parallel_lane_frame_layout(
            target,
            program.nominals(),
            program.elements(),
            callee.parameters().iter().map(|(_, ty)| *ty),
            callee.result(),
            carries_budget(ordinal),
        )
        .map_err(BackendFailure::TargetLayout)?
        else {
            return Ok(None);
        };
        frames.push((*member, layout));
    }
    Ok(Some(frames))
}

fn definition_operation(function: &IrFunction, value: IrValueId) -> Option<&IrOperation> {
    function.blocks().iter().find_map(|block| {
        block
            .instructions()
            .iter()
            .find_map(|instruction| match instruction {
                IrInstruction::Define {
                    result, operation, ..
                } if *result == value => Some(operation),
                _ => None,
            })
    })
}

fn llvm_storage_type(
    program: &IrProgram,
    ty: &TargetStorageType,
) -> Result<String, BackendFailure> {
    llvm_storage_type_with_references(program, ty, &mut BTreeSet::new())
}

fn llvm_storage_type_with_references(
    program: &IrProgram,
    ty: &TargetStorageType,
    references: &mut BTreeSet<String>,
) -> Result<String, BackendFailure> {
    match ty {
        TargetStorageType::Source(ty) => llvm_type_with_references(program, *ty, references),
        TargetStorageType::Integer(width) if matches!(width, 1 | 8 | 16 | 32 | 64) => {
            Ok(format!("i{width}"))
        }
        TargetStorageType::Integer(_) => Err(BackendFailure::InvalidIr),
        TargetStorageType::Array { element, length } => Ok(format!(
            "[{length} x {}]",
            llvm_storage_type_with_references(program, element, references)?
        )),
    }
}

pub(crate) fn llvm_type(program: &IrProgram, ty: IrType) -> Result<String, BackendFailure> {
    llvm_type_with_references(program, ty, &mut BTreeSet::new())
}

pub(super) fn llvm_type_with_references(
    program: &IrProgram,
    ty: IrType,
    references: &mut BTreeSet<String>,
) -> Result<String, BackendFailure> {
    match ty {
        IrType::Unit => Ok("i8".to_owned()),
        IrType::Bool => Ok("i1".to_owned()),
        IrType::Integer { width: 8, .. } => Ok("i8".to_owned()),
        IrType::Integer { width: 16, .. } => Ok("i16".to_owned()),
        IrType::Integer { width: 32, .. } => Ok("i32".to_owned()),
        IrType::Integer { width: 64, .. } => Ok("i64".to_owned()),
        IrType::Integer { .. } => Err(BackendFailure::InvalidIr),
        IrType::Float { width: 32 } => Ok("float".to_owned()),
        IrType::Float { width: 64 } => Ok("double".to_owned()),
        IrType::Float { .. } => Err(BackendFailure::InvalidIr),
        IrType::Array { length: 0, .. } => Ok("[0 x i8]".to_owned()),
        IrType::Array { element, length } => Ok(format!(
            "[{length} x {}]",
            llvm_type_with_references(
                program,
                program.element(element).ok_or(BackendFailure::InvalidIr)?,
                references
            )?
        )),
        // A `&[T]` range reference is a pointer and one count [REF-4]; it is
        // a reference kind, so no storage ever holds one.
        IrType::Range { .. } => Ok("{ ptr, i64 }".to_owned()),
        // compiler/storage-representation: a runtime-capacity `Array<T>` is
        // one block `[len | elements]`, header first, exactly as a boxed
        // window block is. An `Array`'s `len` equals its `cap` [WIN-1], so
        // the one runtime number is stored once. The block is reached only
        // through the `Box` that owns it [TYPE-9], so its own type never
        // names its element count.
        IrType::Buffer { element } => Ok(format!(
            "{{ i64, [0 x {}] }}",
            llvm_type_with_references(
                program,
                program.element(element).ok_or(BackendFailure::InvalidIr)?,
                references
            )?
        )),
        // compiler/storage-representation: header first, so the inline and
        // the boxed placement of one shape share one address computation. A
        // `Slots` carries `len` alone and a `Ring` carries `len` and `head`;
        // a constant capacity is the type constant and is stored nowhere.
        IrType::Window {
            shape,
            element,
            capacity: Some(length),
        } => {
            // A zero-capacity window has no element representation. Keep
            // the same byte tail as Array<T, 0>, so LLVM does not retain T's
            // alignment beyond the header-only layout qualified by OP-9
            // and STOR-6. A positive capacity of zero-sized T is distinct.
            let element = if length == 0 {
                "i8".to_owned()
            } else {
                llvm_type_with_references(
                    program,
                    program.element(element).ok_or(BackendFailure::InvalidIr)?,
                    references,
                )?
            };
            Ok(match shape {
                IrWindowShape::Slots => format!("{{ i64, [{length} x {element}] }}"),
                IrWindowShape::Ring => format!("{{ i64, i64, [{length} x {element}] }}"),
            })
        }
        // A runtime-capacity block is reached only through the `Box` that
        // owns it [TYPE-9], so its own type never names its element count.
        IrType::Window {
            shape,
            element,
            capacity: None,
        } => {
            let element = llvm_type_with_references(
                program,
                program.element(element).ok_or(BackendFailure::InvalidIr)?,
                references,
            )?;
            Ok(match shape {
                IrWindowShape::Slots => format!("{{ i64, i64, [0 x {element}] }}"),
                IrWindowShape::Ring => format!("{{ i64, i64, i64, [0 x {element}] }}"),
            })
        }
        IrType::Address(_) | IrType::RuntimeBoxPayload { .. } => Ok("ptr".to_owned()),
        IrType::Nominal(id) => {
            let nominal = program.nominal(id).ok_or(BackendFailure::InvalidIr)?;
            if matches!(
                nominal.kind(),
                IrNominalKind::Box { .. } | IrNominalKind::Shared { .. }
            ) {
                return Ok("ptr".to_owned());
            }
            if matches!(nominal.kind(), IrNominalKind::Opaque) {
                return Ok("{ i128, i128 }".to_owned());
            }
            if nominal.is_tag_only_enum() {
                let IrNominalKind::Enum { variants } = nominal.kind() else {
                    return Err(BackendFailure::InvalidIr);
                };
                Ok(if variants.len() <= 2 { "i1" } else { "i32" }.to_owned())
            } else {
                references.insert(format!("wf.t.{}", nominal.link_name()));
                Ok(nominal_symbol(nominal))
            }
        }
    }
}

fn is_tag_only_type(program: &IrProgram, ty: IrType) -> Result<bool, BackendFailure> {
    match ty {
        IrType::Bool => Ok(true),
        IrType::Nominal(id) => program
            .nominal(id)
            .map(IrNominal::is_tag_only_enum)
            .ok_or(BackendFailure::InvalidIr),
        _ => Ok(false),
    }
}

fn constant_operand(constant: IrConstant, ty: IrType) -> Result<String, BackendFailure> {
    match (constant, ty) {
        (IrConstant::Unit, IrType::Unit) => Ok("0".to_owned()),
        (IrConstant::Bool(value), IrType::Bool) => Ok(u8::from(value).to_string()),
        (
            IrConstant::Integer {
                ty: constant_ty,
                bits,
            },
            actual_ty,
        ) if constant_ty == actual_ty => {
            let IrType::Integer { width, signed } = actual_ty else {
                return Err(BackendFailure::InvalidIr);
            };
            if !matches!(width, 8 | 16 | 32 | 64) {
                return Err(BackendFailure::InvalidIr);
            }
            let mask = if width == 64 {
                u64::MAX
            } else {
                (1_u64 << width) - 1
            };
            let bits = bits & mask;
            Ok(if signed && bits & (1_u64 << (width - 1)) != 0 {
                (i128::from(bits) - (1_i128 << width)).to_string()
            } else {
                bits.to_string()
            })
        }
        (
            IrConstant::Float {
                ty: constant_ty,
                bits,
            },
            actual_ty,
        ) if constant_ty == actual_ty => match actual_ty {
            IrType::Float { width: 32 } => {
                let bits = u32::try_from(bits).map_err(|_| BackendFailure::InvalidIr)?;
                let widened = f64::from(f32::from_bits(bits)).to_bits();
                Ok(format!("0x{widened:016X}"))
            }
            IrType::Float { width: 64 } => Ok(format!("0x{bits:016X}")),
            _ => Err(BackendFailure::InvalidIr),
        },
        _ => Err(BackendFailure::InvalidIr),
    }
}

fn variant_field_base(
    variants: &[crate::IrVariant],
    selected: u32,
) -> Result<usize, BackendFailure> {
    let mut index = 1_usize;
    for variant in variants {
        if variant.tag() == selected {
            return Ok(index);
        }
        index = index
            .checked_add(variant.fields().len())
            .ok_or(BackendFailure::CounterOverflow)?;
    }
    Err(BackendFailure::InvalidIr)
}

/// The member of the overlap group `result` joins whose join settles the
/// block's label, if `result` is a join site at all. Its `par.done` block is
/// where the block continues. Ordinary lane calls join newest first.
fn block_label(block: IrBlockId) -> String {
    if block.ordinal() == 0 {
        "entry".to_owned()
    } else {
        format!("bb{}", block.ordinal())
    }
}

fn value_name(value: IrValueId) -> String {
    format!("%v{}", value.ordinal())
}

/// A nominal's LLVM type name, by its stable link name, so an unchanged
/// function's fragment keeps its text when another type is added [MOD-8].
fn nominal_symbol(nominal: &crate::IrNominal) -> String {
    format!("%wf.t.{}", nominal.link_name())
}

/// A constant's LLVM global, by its stable link name [MOD-8].
fn constant_symbol(constant: &crate::IrGlobalConstant) -> String {
    format!("@.wf_const.{}", constant.link_name())
}

fn integer_safe_label(value: IrValueId) -> String {
    format!("integer.safe.v{}", value.ordinal())
}

fn integer_error_label(value: IrValueId) -> String {
    format!("integer.error.v{}", value.ordinal())
}

fn integer_continue_label(value: IrValueId) -> String {
    format!("integer.cont.v{}", value.ordinal())
}

fn box_new_ready_label(value: IrValueId) -> String {
    format!("box.new.ready.v{}", value.ordinal())
}

fn array_fill_head_label(value: IrValueId) -> String {
    format!("array.fill.head.v{}", value.ordinal())
}

fn array_fill_body_label(value: IrValueId) -> String {
    format!("array.fill.body.v{}", value.ordinal())
}

fn array_fill_done_label(value: IrValueId) -> String {
    format!("array.fill.done.v{}", value.ordinal())
}

fn invalid_tag_label(block: IrBlockId) -> String {
    format!("invalid.tag.b{}", block.ordinal())
}

pub(crate) fn source_symbol(name: &str) -> String {
    format!("wf_{name}")
}

/// The overlapped-world symbol a sequential clone was made from, for a caller
/// that has only the two symbols and needs to know they are one function.
///
/// It lives beside [`source_symbol`] because that is the other half of the
/// spelling: a clone is `wf__par_seq_` plus the source name where the ordinary
/// definition is `wf_` plus the same name, and a reader who has to reconstruct
/// that from two files gets it wrong.
pub(crate) fn overlapped_clone_symbol(sequential: &str) -> Option<String> {
    sequential.strip_prefix("wf__par_seq_").map(source_symbol)
}

/// Retry an interrupted POSIX diagnostic write without changing the caller's
/// cursor. The supported Darwin and Linux ABIs both number EINTR as four but
/// expose the thread-local errno cell through different accessors. This stays
/// in the emitted module, including when no floor runtime is linked.
fn emit_posix_resource_write(
    output: &mut Module,
    target: TargetLayout,
) -> Result<(), BackendFailure> {
    let errno = if target.triple().contains("apple-darwin") {
        "__error"
    } else {
        "__errno_location"
    };
    output.declare(Signature::new(
        "write",
        "i64",
        vec![
            Parameter::unnamed("i32"),
            Parameter::unnamed("ptr"),
            Parameter::unnamed("i64"),
        ],
    ));
    output.declare(Signature::new(errno, "ptr", Vec::new()));
    output.text("\n");
    let mut signature = Signature::new(
        "wf_resource_write",
        "i64",
        vec![
            Parameter::named("ptr", "%bytes"),
            Parameter::named("i64", "%length"),
        ],
    );
    signature.linkage = Linkage::Private;
    let mut body = FunctionBody::default();
    body.open_block("entry".to_owned());
    body.instructions("  br label %write\n", &[]);
    body.open_block("write".to_owned());
    body.instructions("  %written = call i64 @write(i32 2, ptr %bytes, i64 %length)\n  %failed = icmp slt i64 %written, 0\n  br i1 %failed, label %error, label %done\n", &["write"]);
    body.open_block("error".to_owned());
    body.instructions(&format!("  %errno = call ptr @{errno}()\n  %code = load i32, ptr %errno, align 4\n  %interrupted = icmp eq i32 %code, 4\n  br i1 %interrupted, label %write, label %done\n"), &[errno]);
    body.open_block("done".to_owned());
    body.instructions("  ret i64 %written\n", &[]);
    output.define(signature.define(body, "")?);
    output.text("\n");
    Ok(())
}

/// The heap-resource record writer of a module with one thread.
///
/// The thread that reaches it writes its complete record to standard error and
/// aborts the process without unwinding. There is no one to arbitrate with, so
/// there is no latch: these are the bytes every module emitted before the
/// overlapped world existed, and they are what a default build still gets.
pub(super) fn sequential_resource_record_writer() -> Result<Module, BackendFailure> {
    let mut module = Module::default();
    module.text("\n");
    let mut signature = Signature::new(
        "wf_resource_record_abort",
        "void",
        vec![
            Parameter::named("ptr", "%message"),
            Parameter::named("i64", "%length"),
        ],
    );
    signature.linkage = Linkage::Private;
    signature.suffix = " noreturn".to_owned();
    let mut body = FunctionBody::default();
    body.open_block("entry".to_owned());
    body.instructions("  br label %write.loop\n", &[]);
    body.open_block("write.loop".to_owned());
    body.instructions("  %cursor = phi ptr [ %message, %entry ], [ %next, %write.more ]\n  %remaining = phi i64 [ %length, %entry ], [ %left, %write.more ]\n  %written = call i64 @wf_resource_write(ptr %cursor, i64 %remaining)\n  %complete = icmp eq i64 %written, %remaining\n  br i1 %complete, label %abort, label %write.incomplete\n", &["wf_resource_write"]);
    body.open_block("write.incomplete".to_owned());
    body.instructions("  %progress = icmp sgt i64 %written, 0\n  br i1 %progress, label %write.more, label %abort\n", &[]);
    body.open_block("write.more".to_owned());
    body.instructions("  %next = getelementptr i8, ptr %cursor, i64 %written\n  %left = sub i64 %remaining, %written\n  br label %write.loop\n", &[]);
    body.open_block("abort".to_owned());
    body.instructions("  call void @abort()\n  unreachable\n", &["abort"]);
    module.define(signature.define(body, "")?);
    module.text("\n");
    Ok(module)
}

/// Windows twin of [`sequential_resource_record_writer`]. The private runtime
/// call writes the same bytes to the process diagnostic channel without
/// importing the POSIX file-descriptor ABI into a COFF module.
pub(super) fn windows_sequential_resource_record_writer() -> Result<Module, BackendFailure> {
    let mut module = Module::default();
    module.text("\n");
    let mut signature = Signature::new(
        "wf_resource_record_abort",
        "void",
        vec![
            Parameter::named("ptr", "%message"),
            Parameter::named("i64", "%length"),
        ],
    );
    signature.linkage = Linkage::Private;
    signature.suffix = " noreturn".to_owned();
    let mut body = FunctionBody::default();
    body.open_block("entry".to_owned());
    body.instructions("  br label %write.loop\n", &[]);
    body.open_block("write.loop".to_owned());
    body.instructions("  %cursor = phi ptr [ %message, %entry ], [ %next, %write.more ]\n  %remaining = phi i64 [ %length, %entry ], [ %left, %write.more ]\n  %written = call i64 @wf__windows_diagnostic_write(ptr %cursor, i64 %remaining)\n  %complete = icmp eq i64 %written, %remaining\n  br i1 %complete, label %abort, label %write.incomplete\n", &["wf__windows_diagnostic_write"]);
    body.open_block("write.incomplete".to_owned());
    body.instructions("  %progress = icmp sgt i64 %written, 0\n  br i1 %progress, label %write.more, label %abort\n", &[]);
    body.open_block("write.more".to_owned());
    body.instructions("  %next = getelementptr i8, ptr %cursor, i64 %written\n  %left = sub i64 %remaining, %written\n  br label %write.loop\n", &[]);
    body.open_block("abort".to_owned());
    body.instructions("  call void @abort()\n  unreachable\n", &["abort"]);
    module.define(signature.define(body, "")?);
    module.text("\n");
    Ok(module)
}

/// The module's own answer for the shared record latch, and the only state the
/// resource-record path carries.
///
/// The latch a record writer takes is the floor runtime's, because the floor's
/// signal handler writes the stack record and this module writes every other
/// one: a latch each would leave the two classes unserialized against each
/// other, and two threads dying of different resources at once could interleave
/// two records on one channel. Asking the floor for the address is what makes
/// "no execution writes a second one" a mechanism rather than an argument.
///
/// The `weak` definition here is the same standalone answer
/// [`floor::floor_runtime_fallback`] gives: an emitted module must link and run
/// without the floor's translation unit, and the real definition replaces this
/// one whenever that unit is linked, which is every ordinary build. Zero until
/// some thread writes a record, and no path outside the writer reads it, so a
/// program that writes none pays nothing for it.
pub(super) fn resource_record_latch() -> Result<Module, BackendFailure> {
    let mut module = Module::default();
    module.global(
        ".wf_resource_record.latch".to_owned(),
        "global",
        "i32".to_owned(),
        "0".to_owned(),
        Some(4),
        References::default(),
    );
    Ok(module)
}

/// The module's standalone definition of the shared latch's accessor.
pub(super) fn resource_record_latch_fallback() -> Result<Module, BackendFailure> {
    let mut module = Module::default();
    module.text("\n");
    let mut signature = Signature::new("wf__floor_record_latch", "ptr", vec![]);
    signature.linkage = Linkage::Weak;
    let mut body = FunctionBody::default();
    body.open_block("entry".to_owned());
    body.instructions(
        "  ret ptr @.wf_resource_record.latch\n",
        &[".wf_resource_record.latch"],
    );
    module.define(signature.define(body, "")?);
    Ok(module)
}

/// [`sequential_resource_record_writer`]'s work under a first-writer-wins latch,
/// emitted
/// where the module can have more than one thread inside it — that is, where
/// it writes a heap-resource record and hands a call out.
///
/// The first thread to arrive takes the shared latch and owns the record: it
/// writes its complete bytes to standard error and aborts the process without
/// unwinding. Every other thread that arrives while the latch is taken parks,
/// and the winner's abort takes it down with the process. The latch is the
/// floor runtime's, not this module's, so the parking also holds between this
/// writer and the floor's own — an execution that runs out of stack on one
/// thread while another is refused an allocation still produces exactly one
/// well-formed record rather than two interleaved ones. [PAR-1]'s
/// erroneous-execution guarantee is met by construction.
///
/// *Which* record wins may depend on the schedule, and that is the whole of
/// what a permitted overlap can change about one. The bytes are fixed by the
/// resource class that wins — no worker, thread, or dynamic stack appears in
/// them — so a run that exhausts nothing observes nothing here.
///
/// The park spins on a *volatile* load rather than an empty loop, so no
/// optimizer may delete the loop and let a losing thread fall through into a
/// second record.
pub(super) fn latched_resource_record_writer() -> Result<Module, BackendFailure> {
    let mut module = Module::default();
    module.text("\n");
    let mut signature = Signature::new(
        "wf_resource_record_abort",
        "void",
        vec![
            Parameter::named("ptr", "%message"),
            Parameter::named("i64", "%length"),
        ],
    );
    signature.linkage = Linkage::Private;
    signature.suffix = " noreturn".to_owned();
    let mut body = FunctionBody::default();
    body.open_block("entry".to_owned());
    body.instructions("  %latch = call ptr @wf__floor_record_latch()\n  %acquired = cmpxchg ptr %latch, i32 0, i32 1 seq_cst seq_cst\n  %won = extractvalue { i32, i1 } %acquired, 1\n  br i1 %won, label %write.loop, label %park\n", &["wf__floor_record_latch"]);
    body.open_block("write.loop".to_owned());
    body.instructions("  %cursor = phi ptr [ %message, %entry ], [ %next, %write.more ]\n  %remaining = phi i64 [ %length, %entry ], [ %left, %write.more ]\n  %written = call i64 @wf_resource_write(ptr %cursor, i64 %remaining)\n  %complete = icmp eq i64 %written, %remaining\n  br i1 %complete, label %abort, label %write.incomplete\n", &["wf_resource_write"]);
    body.open_block("write.incomplete".to_owned());
    body.instructions("  %progress = icmp sgt i64 %written, 0\n  br i1 %progress, label %write.more, label %abort\n", &[]);
    body.open_block("write.more".to_owned());
    body.instructions("  %next = getelementptr i8, ptr %cursor, i64 %written\n  %left = sub i64 %remaining, %written\n  br label %write.loop\n", &[]);
    body.open_block("abort".to_owned());
    body.instructions("  call void @abort()\n  unreachable\n", &["abort"]);
    body.open_block("park".to_owned());
    body.instructions(
        "  %parked = load volatile i32, ptr %latch, align 4\n  br label %park\n",
        &[],
    );
    module.define(signature.define(body, "")?);
    module.text("\n");
    Ok(module)
}

/// Windows twin of [`latched_resource_record_writer`], sharing the floor
/// runtime's first-writer latch while using the native diagnostic channel.
pub(super) fn windows_latched_resource_record_writer() -> Result<Module, BackendFailure> {
    let mut module = Module::default();
    module.text("\n");
    let mut signature = Signature::new(
        "wf_resource_record_abort",
        "void",
        vec![
            Parameter::named("ptr", "%message"),
            Parameter::named("i64", "%length"),
        ],
    );
    signature.linkage = Linkage::Private;
    signature.suffix = " noreturn".to_owned();
    let mut body = FunctionBody::default();
    body.open_block("entry".to_owned());
    body.instructions("  %latch = call ptr @wf__floor_record_latch()\n  %acquired = cmpxchg ptr %latch, i32 0, i32 1 seq_cst seq_cst\n  %won = extractvalue { i32, i1 } %acquired, 1\n  br i1 %won, label %write.loop, label %park\n", &["wf__floor_record_latch"]);
    body.open_block("write.loop".to_owned());
    body.instructions("  %cursor = phi ptr [ %message, %entry ], [ %next, %write.more ]\n  %remaining = phi i64 [ %length, %entry ], [ %left, %write.more ]\n  %written = call i64 @wf__windows_diagnostic_write(ptr %cursor, i64 %remaining)\n  %complete = icmp eq i64 %written, %remaining\n  br i1 %complete, label %abort, label %write.incomplete\n", &["wf__windows_diagnostic_write"]);
    body.open_block("write.incomplete".to_owned());
    body.instructions("  %progress = icmp sgt i64 %written, 0\n  br i1 %progress, label %write.more, label %abort\n", &[]);
    body.open_block("write.more".to_owned());
    body.instructions("  %next = getelementptr i8, ptr %cursor, i64 %written\n  %left = sub i64 %remaining, %written\n  br label %write.loop\n", &[]);
    body.open_block("abort".to_owned());
    body.instructions("  call void @abort()\n  unreachable\n", &["abort"]);
    body.open_block("park".to_owned());
    body.instructions(
        "  %parked = load volatile i32, ptr %latch, align 4\n  br label %park\n",
        &[],
    );
    module.define(signature.define(body, "")?);
    module.text("\n");
    Ok(module)
}

fn llvm_bytes(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 3);
    for byte in bytes {
        let _ = write!(encoded, "\\{byte:02X}");
    }
    encoded
}
