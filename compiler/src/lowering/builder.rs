use std::collections::{HashMap, HashSet};

use crate::target::TargetLayout;

mod buffers;
mod loops;
mod prelude;
mod probe;
mod ranges;
mod results;
mod runs;
mod scalar_grain;
mod split;
mod storage;
mod targets;
mod work;

use crate::CheckedProgram;
use crate::NodePath;
use crate::semantic::CheckedSetTarget;
use crate::semantic::{
    BindingId, CheckedArrayRoot, CheckedDrop, CheckedExpression, CheckedMatchArm, CheckedMeasure,
    CheckedMode, CheckedNominalKind, CheckedParameter, CheckedProgramData, CheckedProjectedDrop,
    CheckedStatement, CheckedValue, FunctionPermissions, MeasureCell, MeasuredKind,
};

use super::*;
use loops::LoopTarget;
use split::{Synthesis, SynthesisCell};
use storage::collect_addressed_bindings;

#[cfg(test)]
pub fn lower_checked(
    checked: CheckedProgram,
    overlap: OverlapLowering,
) -> Result<IrProgram, LoweringFailure> {
    lower_checked_with_layout(checked, overlap, TargetLayout::host()?)
}

/// Select optional target-fitting loop shapes after semantic acceptance, using
/// the same target that will qualify and emit their transported signatures.
#[cfg(test)]
pub(crate) fn lower_checked_with_layout(
    checked: CheckedProgram,
    overlap: OverlapLowering,
    target: TargetLayout,
) -> Result<IrProgram, LoweringFailure> {
    lower_checked_from(checked, overlap, target, None)
}

/// Select optional target-fitting loop shapes after semantic acceptance, using
/// the same target that will qualify and emit their transported signatures,
/// emitting only the functions `roots` reach through their calls when roots
/// are given: a module program entry's build [MOD-9].
pub(crate) fn lower_checked_from(
    checked: CheckedProgram,
    overlap: OverlapLowering,
    target: TargetLayout,
    roots: Option<&[crate::semantic::FunctionId]>,
) -> Result<IrProgram, LoweringFailure> {
    let sequential_compute_refusal = matches!(
        overlap,
        OverlapLowering::OnWithSequentialRefusal { .. }
            | OverlapLowering::OnWithRecursionBudget {
                sequential_refusal: true,
                ..
            }
    );
    // A lowering that actualizes compute at all carries a recursion budget;
    // the control form is the only one that says something other than the
    // default. A lowering that actualizes no compute carries none, so a
    // default or `--no-overlap` build names the budget nowhere.
    let recursion_budget = match overlap {
        OverlapLowering::Off => None,
        OverlapLowering::OnWithRecursionBudget { budget, .. } => Some(budget),
        OverlapLowering::On
        | OverlapLowering::OnWithSequentialRefusal { .. }
        | OverlapLowering::OnWithoutSmallScalarLeaves { .. } => Some(RecursionBudget::default()),
    };
    let scalar_leaf_limit = match overlap {
        OverlapLowering::OnWithoutSmallScalarLeaves { maximum_operations } => {
            Some(maximum_operations)
        }
        OverlapLowering::OnWithSequentialRefusal {
            maximum_scalar_leaf_operations,
        }
        | OverlapLowering::OnWithRecursionBudget {
            maximum_scalar_leaf_operations,
            ..
        } => maximum_scalar_leaf_operations,
        _ => None,
    };
    let overlap = match overlap {
        OverlapLowering::Off => OverlapLowering::Off,
        _ => OverlapLowering::On,
    };
    // Each checked nominal's IR nominal before physical merging: the lowering
    // alias the checker assigned it.
    let erasure = checked
        .data
        .nominal_lowering_alias
        .iter()
        .map(|alias| IrNominalId(alias.0))
        .collect::<Vec<_>>();
    let (base_elements, base_element_map) = physical_types::base_elements(&checked.data, &erasure)?;
    let base_types = TypeLowering {
        nominals: &erasure,
        elements: &base_element_map,
    };
    let base_nominals = lower_nominals(base_types, &checked.data)?;
    let constants = lower_constants(base_types, &checked.data)?;
    let physical = specialize::PhysicalFunctions::build_from(&checked.data, roots)?;
    let mut types = physical_types::PhysicalTypes::new(
        &checked.data,
        base_nominals,
        base_elements,
        base_element_map,
    );
    let map = types.map()?;
    let nominals = types.nominals;
    let elements = types.elements;
    // Each function's declared IR result carries its result *mode*: a borrow
    // of addressed content is an address. A call site must produce exactly
    // the callee's declared result type, so the declared results are computed
    // once and consulted at every `UserCall` [REF-1, TYPE-7].
    let types = TypeLowering {
        nominals: &map.nominals,
        elements: &map.elements,
    };
    let function_results = physical
        .variants
        .iter()
        .map(|variant| {
            let function = &checked.data.functions[variant.source.0 as usize];
            lower_borrow_mode_type(
                function.result_mode,
                lower_type(types, function.result)?,
                &nominals,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    // The [PAR-1 candidate] permission table, read exactly as the checker
    // produced it. Lowering selects which permitted groups it can actualize
    // and never widens one — and reads the table at all only when this
    // compilation asked for overlap lowering, so the default emits the same
    // module a compiler with no such lowering emits.
    let permission = match overlap {
        OverlapLowering::On
        | OverlapLowering::OnWithoutSmallScalarLeaves { .. }
        | OverlapLowering::OnWithSequentialRefusal { .. }
        | OverlapLowering::OnWithRecursionBudget { .. } => Some(&checked.data.permission),
        OverlapLowering::Off => None,
    };
    // Where a synthesized function's ordinal starts. A [PAR-2] split appends
    // its two halves after every source function, so nothing renumbers and a
    // `Call` still indexes one flat table.
    let source_functions =
        u32::try_from(physical.variants.len()).map_err(|_| LoweringFailure::CounterOverflow)?;
    let synthesis = SynthesisCell::new(Synthesis::new(source_functions));
    let symbols = physical
        .variants
        .iter()
        .map(|variant| {
            checked.data.functions[variant.source.0 as usize]
                .symbol
                .clone()
        })
        .collect::<Vec<_>>();
    let mut functions = physical
        .variants
        .iter()
        .enumerate()
        .map(|(index, variant)| {
            let function = &checked.data.functions[variant.source.0 as usize];
            let context = LoweringContext {
                target,
                erasure: types,
                physical_calls: &variant.calls,
                nominals: &nominals,
                elements: &elements,
                constants: &constants,
                function_results: &function_results,
                synthesis: &synthesis,
            };
            lower_function(
                function,
                index,
                &symbols[index],
                context,
                permission.and_then(|table| table.of(function.id)),
                overlap,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    #[cfg(test)]
    let loop_candidate_constructions = synthesis.borrow().candidate_constructions;
    let (synthesized, mut actualization) = synthesis.into_inner().finish()?;
    functions.extend(synthesized);
    split::assign_weights(&mut functions);
    if let Some(limit) = scalar_leaf_limit {
        scalar_grain::prune(&mut functions, limit, &mut actualization);
    }
    Ok(IrProgram {
        nominals,
        elements,
        constants,
        functions,
        actualization,
        sequential_compute_refusal,
        recursion_budget,
        #[cfg(test)]
        loop_candidate_constructions,
    })
}

/// What every builder of one lowering shares: the program-wide tables it reads
/// and the cell the synthesized halves of a [PAR-2] split are filed in.
///
/// These travel as one value because a chunk is built by a second builder
/// created inside the first, and a chunk containing another permitted loop
/// creates a third; passing the shared half by name keeps that recursion from
/// growing an argument list at every level.
#[derive(Clone, Copy)]
struct LoweringContext<'program> {
    target: TargetLayout,
    /// The IR nominal and element each checked nominal and element lowers to.
    erasure: TypeLowering<'program>,
    physical_calls: &'program [(NodePath, u32)],
    nominals: &'program [IrNominal],
    elements: &'program [IrType],
    constants: &'program [IrGlobalConstant],
    /// Every physical function's declared IR result, indexed by its IR ordinal.
    function_results: &'program [IrType],
    synthesis: &'program SynthesisCell,
}

fn lower_scalar_constant(value: &CheckedValue) -> Result<IrConstant, LoweringFailure> {
    match value {
        CheckedValue::Unit => Ok(IrConstant::Unit),
        CheckedValue::Bool(value) => Ok(IrConstant::Bool(*value)),
        CheckedValue::Integer { ty, bits } => Ok(IrConstant::Integer {
            ty: lower_type(
                TypeLowering::EMPTY,
                crate::semantic::CheckedType::Integer(*ty),
            )?,
            bits: *bits,
        }),
        CheckedValue::Float { ty, bits } => Ok(IrConstant::Float {
            ty: lower_type(
                TypeLowering::EMPTY,
                crate::semantic::CheckedType::Float(*ty),
            )?,
            bits: *bits,
        }),
        CheckedValue::ConstGeneric { .. }
        | CheckedValue::NumericIdentity { .. }
        | CheckedValue::Array { .. }
        | CheckedValue::Struct { .. } => Err(LoweringFailure::InvalidCheckedProgram),
    }
}

fn lower_global_value(value: &CheckedValue) -> Result<IrGlobalValue, LoweringFailure> {
    match value {
        CheckedValue::Array { elements, .. } => Ok(IrGlobalValue::Array(
            elements
                .iter()
                .map(lower_global_value)
                .collect::<Result<Vec<_>, _>>()?,
        )),
        CheckedValue::Struct { fields, .. } => Ok(IrGlobalValue::Struct(
            fields
                .iter()
                .map(lower_global_value)
                .collect::<Result<Vec<_>, _>>()?,
        )),
        scalar => Ok(IrGlobalValue::Scalar(lower_scalar_constant(scalar)?)),
    }
}

/// [MOD-8] the stable link names of a table of entities: each the first
/// sixteen hexadecimal digits of the SHA-256 of its stable spelling, the
/// order of first occurrence appended where two spellings agree, and its
/// ordinal where it has no stable spelling.
fn link_names<'spelling>(spellings: impl Iterator<Item = Option<&'spelling str>>) -> Vec<String> {
    use std::fmt::Write as _;
    let mut used = std::collections::HashMap::<String, usize>::new();
    spellings
        .enumerate()
        .map(|(index, spelling)| {
            let base = spelling.map_or_else(
                || format!("n{index}"),
                |spelling| {
                    let digest = crate::spec::sha256::digest(spelling.as_bytes());
                    let mut hex = String::with_capacity(16);
                    for byte in &digest[..8] {
                        let _ = write!(hex, "{byte:02x}");
                    }
                    hex
                },
            );
            let count = used.entry(base.clone()).or_insert(0);
            let name = if *count == 0 {
                base
            } else {
                format!("{base}.{count}")
            };
            *count += 1;
            name
        })
        .collect()
}

fn lower_constants(
    erasure: TypeLowering<'_>,
    data: &CheckedProgramData,
) -> Result<Vec<IrGlobalConstant>, LoweringFailure> {
    let names = link_names(
        (0..data.constants.len())
            .map(|index| data.constant_spellings.get(index).map(String::as_str)),
    );
    data.constants
        .iter()
        .zip(names)
        .enumerate()
        .map(|(index, (constant, link_name))| {
            if constant.id.0 as usize != index || constant.value.ty() != constant.ty {
                return Err(LoweringFailure::InvalidCheckedProgram);
            }
            Ok(IrGlobalConstant {
                id: IrConstantId(constant.id.0),
                name: constant.name.clone(),
                link_name,
                ty: lower_type(erasure, constant.ty)?,
                value: lower_global_value(&constant.value)?,
            })
        })
        .collect()
}

fn lower_nominals(
    erasure: TypeLowering<'_>,
    data: &CheckedProgramData,
) -> Result<Vec<IrNominal>, LoweringFailure> {
    let names = link_names(
        (0..data.executable_nominal_count)
            .map(|index| data.nominal_spellings.get(index).and_then(Option::as_deref)),
    );
    data.nominals
        .get(..data.executable_nominal_count)
        .ok_or(LoweringFailure::InvalidCheckedProgram)?
        .iter()
        .zip(names)
        .enumerate()
        .map(|(index, (nominal, link_name))| {
            if nominal.id.0 as usize != index {
                return Err(LoweringFailure::InvalidCheckedProgram);
            }
            let kind = match &nominal.kind {
                CheckedNominalKind::Struct { fields } => IrNominalKind::Struct {
                    fields: fields
                        .iter()
                        .map(|field| {
                            Ok(IrField {
                                ty: lower_type(erasure, field.ty)?,
                            })
                        })
                        .collect::<Result<Vec<_>, LoweringFailure>>()?,
                },
                CheckedNominalKind::Enum { variants } => IrNominalKind::Enum {
                    variants: variants
                        .iter()
                        .map(|variant| {
                            Ok(IrVariant {
                                tag: variant.tag,
                                fields: variant
                                    .fields
                                    .iter()
                                    .map(|field| {
                                        Ok(IrField {
                                            ty: lower_type(erasure, field.ty)?,
                                        })
                                    })
                                    .collect::<Result<Vec<_>, LoweringFailure>>()?,
                            })
                        })
                        .collect::<Result<Vec<_>, LoweringFailure>>()?,
                },
                CheckedNominalKind::Box {
                    referent, release, ..
                } => IrNominalKind::Box {
                    referent: lower_type(erasure, *referent)?,
                    release: lower_release_class(*release),
                },
                CheckedNominalKind::Opaque => IrNominalKind::Opaque,
            };
            Ok(IrNominal {
                name: nominal.name.clone(),
                link_name,
                stable: data.nominal_spellings.get(index).cloned().flatten(),
                id: IrNominalId(
                    u32::try_from(index).map_err(|_| LoweringFailure::CounterOverflow)?,
                ),
                kind,
            })
        })
        .collect()
}

fn lower_function<'program>(
    function: &crate::semantic::CheckedFunction,
    physical_index: usize,
    symbol: &'program str,
    context: LoweringContext<'program>,
    permissions: Option<&'program FunctionPermissions>,
    overlap: OverlapLowering,
) -> Result<IrFunction, LoweringFailure> {
    // One of the compiler-owned [PRE-1] records: declared body-less exactly
    // like a host function, but with no trusted-base object behind it, so the
    // compiler emits the body here. Only a program that called the row
    // reaches this instance, because a generic row has no instance until a
    // call selects one, and the host functions are not compiler-owned at all.
    let compiler_owned = function.body.is_none() && prelude::compiler_owned_row(&function.name);
    let uninhabited = matches!(
        function.body_disposition,
        crate::semantic::CheckedBodyDisposition::Uninhabited { .. }
    );
    // An uninhabited body must not be traversed even for storage planning.
    let addressed_bindings = if uninhabited || function.body.is_none() {
        std::collections::HashSet::new()
    } else {
        collect_addressed_bindings(function)
    };
    let result = *context
        .function_results
        .get(physical_index)
        .ok_or(LoweringFailure::InvalidCheckedProgram)?;
    // The whole permission table of this function, and only when this
    // compilation asked for actualization: with none, a permitted loop lowers
    // exactly as it did before the split existed and no group is actualized.
    let mut builder = IrBuilder::new(
        context,
        result,
        addressed_bindings,
        permissions,
        overlap,
        symbol,
    )?;
    for parameter in &function.parameters {
        let ty = lower_parameter_type(context.erasure, parameter, context.nominals)?;
        let value = builder.new_parameter(ty)?;
        if parameter.mode == CheckedMode::Reference
            && !function
                .declared_state_writes
                .iter()
                .any(|path| path.root == parameter.declaration)
        {
            builder.readonly_reference_parameters.push(value);
        }
        if builder.bindings.insert(parameter.binding, value).is_some() {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
    }
    if !uninhabited && function.body.as_deref().is_some_and(contains_tail_transfer) {
        let types = builder
            .parameters
            .iter()
            .map(|(_, ty)| *ty)
            .collect::<Vec<_>>();
        let arguments = builder.parameters.iter().map(|(value, _)| *value).collect();
        let (entry, parameters) = builder.new_block(&types)?;
        builder.terminate(IrTerminator::Jump {
            target: entry,
            arguments,
            drops: Vec::new(),
        })?;
        builder.current = Some(entry);
        builder.tail_entry = Some(entry);
        for (parameter, value) in function.parameters.iter().zip(parameters) {
            builder.bindings.insert(parameter.binding, value);
        }
    }
    // Promotion is in the body entry, so an addressed parameter receives its
    // new value on each checked self transfer, using the ordinary frame plan.
    if !compiler_owned {
        for parameter in &function.parameters {
            builder.promote_binding_if_needed(parameter.binding)?;
        }
    }
    if let Some(body) = &function.body {
        if uninhabited {
            builder.terminate(IrTerminator::Unreachable)?;
        } else {
            builder.lower_statements(body, None)?;
        }
    } else if compiler_owned {
        builder.lower_prelude_row(&function.name)?;
    } else {
        builder.blocks.clear();
        builder.current = None;
    }
    let overlaps = builder.overlaps();
    let mut lowered = builder.finish(symbol.to_owned(), overlaps, None)?;
    lowered.source_signature = Some(IrSourceSignature {
        parameters: function
            .parameters
            .iter()
            .map(|parameter| lower_source_mode(parameter.mode))
            .collect(),
        result: lower_source_mode(function.result_mode),
    });
    Ok(lowered)
}

fn contains_tail_transfer(statements: &[CheckedStatement]) -> bool {
    statements.iter().any(|statement| match statement {
        CheckedStatement::Return {
            value:
                CheckedExpression::UserCall {
                    tail_transfer: true,
                    ..
                },
            ..
        } => true,
        CheckedStatement::Match { arms, .. } | CheckedStatement::ValueMatchLet { arms, .. } => {
            arms.iter().any(|arm| contains_tail_transfer(&arm.body))
        }
        CheckedStatement::Loop { body, .. } | CheckedStatement::CountedRange { body, .. } => {
            contains_tail_transfer(body)
        }
        _ => false,
    })
}

const fn lower_source_mode(mode: CheckedMode) -> IrSourceMode {
    match mode {
        CheckedMode::Own => IrSourceMode::Own,
        CheckedMode::Reference => IrSourceMode::Reference,
        CheckedMode::Range => IrSourceMode::Range,
    }
}

fn lower_source_argument(argument: &CheckedExpression) -> IrSourceArgument {
    match argument {
        CheckedExpression::Binding { consume_root, .. } => IrSourceArgument::Binding {
            consume_root: *consume_root,
        },
        CheckedExpression::Project { consume_root, .. } => IrSourceArgument::Projection {
            consume_root: *consume_root,
        },
        CheckedExpression::BorrowAddressed { .. }
        | CheckedExpression::BorrowRangeIndex { .. }
        | CheckedExpression::RangeOf { .. } => IrSourceArgument::Borrow,
        CheckedExpression::ReadStorage { .. }
        | CheckedExpression::DerefAddressed { .. }
        | CheckedExpression::ArrayIndex { .. }
        | CheckedExpression::BufferIndex { .. }
        | CheckedExpression::BoxDeref { .. } => IrSourceArgument::PlaceRead,
        _ => IrSourceArgument::Value,
    }
}

fn lower_parameter_type(
    erasure: TypeLowering<'_>,
    parameter: &CheckedParameter,
    nominals: &[IrNominal],
) -> Result<IrType, LoweringFailure> {
    // [REF-4, TYPE-8] a `&[T]` parameter's written type is the element type
    // and its kind is its mode, so the descriptor type is formed here and
    // never from the type alone.
    if parameter.mode == CheckedMode::Range {
        return Ok(IrType::Range {
            element: lower_element(
                erasure,
                parameter
                    .range_element
                    .ok_or(LoweringFailure::InvalidCheckedProgram)?,
            )?,
        });
    }
    lower_borrow_mode_type(parameter.mode, lower_type(erasure, parameter.ty)?, nominals)
}

/// The representation a borrow-mode value carries.
///
/// A borrow addresses the owner's storage, including a Box's pointer slot.
/// Ordinary opaque values use the same address path. The buffer/view ABI
/// retains its descriptor representation for every callable body [REF-1].
fn lower_borrow_mode_type(
    mode: CheckedMode,
    ty: IrType,
    nominals: &[IrNominal],
) -> Result<IrType, LoweringFailure> {
    if mode == CheckedMode::Own || matches!(ty, IrType::Buffer { .. } | IrType::Range { .. }) {
        return Ok(ty);
    }
    let Some(referent) = IrAddressed::of(ty) else {
        return Ok(ty);
    };
    if let IrAddressed::Nominal(nominal) = referent
        && !matches!(
            nominals
                .get(nominal.index())
                .ok_or(LoweringFailure::InvalidCheckedProgram)?
                .kind,
            IrNominalKind::Struct { .. }
                | IrNominalKind::Enum { .. }
                | IrNominalKind::Box { .. }
                | IrNominalKind::Opaque
        )
    {
        return Ok(ty);
    }
    Ok(IrType::Address(referent))
}

struct BuildingBlock {
    parameters: Vec<(IrValueId, IrType)>,
    instructions: Vec<IrInstruction>,
    terminator: Option<IrTerminator>,
}

struct IrBuilder<'program> {
    target: TargetLayout,
    /// The IR nominal and element each checked nominal and element lowers to.
    erasure: TypeLowering<'program>,
    physical_calls: &'program [(NodePath, u32)],
    nominals: &'program [IrNominal],
    elements: &'program [IrType],
    constants: &'program [IrGlobalConstant],
    bindings: HashMap<BindingId, IrValueId>,
    parameters: Vec<(IrValueId, IrType)>,
    readonly_reference_parameters: Vec<IrValueId>,
    source_calls: Vec<IrSourceCall>,
    values: Vec<IrType>,
    blocks: Vec<BuildingBlock>,
    counted_ranges: Vec<crate::IrCountedRange>,
    current: Option<IrBlockId>,
    /// Parameterized body entry for a checked [FN-10] self transfer.
    tail_entry: Option<IrBlockId>,
    loops: Vec<LoopTarget>,
    result: IrType,
    addressed_bindings: std::collections::HashSet<BindingId>,
    /// Every physical function's result, indexed by its IR ordinal, so a
    /// call defines exactly the callee's declared result type — an address
    /// for a borrow of addressed content [REF-1, TYPE-7].
    function_results: &'program [IrType],
    /// For each statement holding exactly one named-function call in call
    /// position — a `let` right-hand side or a `match` scrutinee — the block
    /// the call's definition landed in and the value it defined. The
    /// permission table names its sites by call occurrence, which is the one
    /// identity every written call position has, so this is how a permitted
    /// group is found in the IR.
    call_results: HashMap<NodePath, (IrBlockId, IrValueId)>,
    /// The permission table of the source function this body belongs to: the
    /// [PAR-1] groups its statements may overlap and the [PAR-2] verdict of
    /// each of its counted loops.
    ///
    /// `None` when this compilation actualizes nothing, and inside a
    /// synthesized splitter, which has no source statement of its own — a
    /// splitter's overlap group is produced by the lowering that built it.
    /// A synthesized *chunk* carries the table, because its body is the loop's
    /// own statements and a pair inside them is permitted exactly as it was
    /// before the loop was split.
    permissions: Option<&'program FunctionPermissions>,
    /// Which subset of the pure permission judgment this compilation may
    /// actualize.
    overlap: OverlapLowering,
    /// Where a split's synthesized halves are filed, shared with every builder
    /// this one creates.
    synthesis: &'program SynthesisCell,
    /// The source function this body belongs to, for the actualization ledger.
    function_name: &'program str,
}

#[derive(Clone)]
struct GiveTarget {
    block: IrBlockId,
    result: IrType,
    carried_bindings: Vec<BindingId>,
}

impl<'program> IrBuilder<'program> {
    fn new(
        context: LoweringContext<'program>,
        result: IrType,
        addressed_bindings: std::collections::HashSet<BindingId>,
        permissions: Option<&'program FunctionPermissions>,
        overlap: OverlapLowering,
        function_name: &'program str,
    ) -> Result<Self, LoweringFailure> {
        let LoweringContext {
            target,
            erasure,
            physical_calls,
            nominals,
            elements,
            constants,
            function_results,
            synthesis,
        } = context;
        let mut builder = Self {
            target,
            erasure,
            physical_calls,
            nominals,
            elements,
            constants,
            bindings: HashMap::new(),
            parameters: Vec::new(),
            readonly_reference_parameters: Vec::new(),
            source_calls: Vec::new(),
            values: Vec::new(),
            blocks: Vec::new(),
            counted_ranges: Vec::new(),
            current: None,
            tail_entry: None,
            loops: Vec::new(),
            result,
            addressed_bindings,
            function_results,
            call_results: HashMap::new(),
            permissions,
            overlap,
            synthesis,
            function_name,
        };
        let (entry, parameters) = builder.new_block(&[])?;
        if !parameters.is_empty() {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        builder.current = Some(entry);
        Ok(builder)
    }

    /// One nominal's lowered identity: the IR nominal it lowers as.
    fn erased(&self, id: crate::NominalId) -> IrNominalId {
        self.erasure
            .nominals
            .get(id.0 as usize)
            .copied()
            .unwrap_or(IrNominalId(id.0))
    }

    /// This builder's own shared half, for the builders it creates.
    const fn context(&self) -> LoweringContext<'program> {
        LoweringContext {
            target: self.target,
            erasure: self.erasure,
            physical_calls: self.physical_calls,
            nominals: self.nominals,
            elements: self.elements,
            constants: self.constants,
            function_results: self.function_results,
            synthesis: self.synthesis,
        }
    }

    /// Declares one more parameter of the function being built.
    fn new_parameter(&mut self, ty: IrType) -> Result<IrValueId, LoweringFailure> {
        let value = self.new_value(ty)?;
        self.parameters.push((value, ty));
        Ok(value)
    }

    /// Seals the body into a function, refusing an unterminated one.
    fn finish(
        self,
        name: String,
        overlaps: Vec<IrOverlap>,
        synthesis: Option<IrSynthesis>,
    ) -> Result<IrFunction, LoweringFailure> {
        if self.current.is_some() || self.blocks.iter().any(|block| block.terminator.is_none()) {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        Ok(IrFunction {
            name,
            parameters: self.parameters,
            readonly_reference_parameters: self.readonly_reference_parameters,
            source_signature: None,
            source_calls: self.source_calls,
            result: self.result,
            values: self.values,
            blocks: self
                .blocks
                .into_iter()
                .map(|block| {
                    Ok(IrBlock {
                        parameters: block.parameters,
                        instructions: block.instructions,
                        terminator: block
                            .terminator
                            .ok_or(LoweringFailure::InvalidCheckedProgram)?,
                    })
                })
                .collect::<Result<Vec<_>, LoweringFailure>>()?,
            overlaps,
            counted_ranges: self.counted_ranges,
            synthesis,
        })
    }

    fn new_value(&mut self, ty: IrType) -> Result<IrValueId, LoweringFailure> {
        let id = IrValueId(
            u32::try_from(self.values.len()).map_err(|_| LoweringFailure::CounterOverflow)?,
        );
        self.values.push(ty);
        Ok(id)
    }

    fn new_block(
        &mut self,
        parameter_types: &[IrType],
    ) -> Result<(IrBlockId, Vec<IrValueId>), LoweringFailure> {
        let id = IrBlockId(
            u32::try_from(self.blocks.len()).map_err(|_| LoweringFailure::CounterOverflow)?,
        );
        let mut parameters = Vec::with_capacity(parameter_types.len());
        let mut values = Vec::with_capacity(parameter_types.len());
        for ty in parameter_types {
            let value = self.new_value(*ty)?;
            parameters.push((value, *ty));
            values.push(value);
        }
        self.blocks.push(BuildingBlock {
            parameters,
            instructions: Vec::new(),
            terminator: None,
        });
        Ok((id, values))
    }

    fn current_block_mut(&mut self) -> Result<&mut BuildingBlock, LoweringFailure> {
        let current = self.current.ok_or(LoweringFailure::InvalidCheckedProgram)?;
        self.blocks
            .get_mut(current.index())
            .ok_or(LoweringFailure::InvalidCheckedProgram)
    }

    fn terminate(&mut self, terminator: IrTerminator) -> Result<(), LoweringFailure> {
        let block = self.current_block_mut()?;
        if block.terminator.replace(terminator).is_some() {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        self.current = None;
        Ok(())
    }

    /// One [MSR-1] measure whose table cell is a compile-time constant.
    /// The reader still executes; it just has no load to perform.
    fn lower_fixed_measure(&mut self, value: u64) -> Result<IrValueId, LoweringFailure> {
        let ty = IrType::Integer {
            width: 64,
            signed: false,
        };
        self.define(
            ty,
            IrOperation::Constant(IrConstant::Integer { ty, bits: value }),
        )
    }

    fn define(&mut self, ty: IrType, operation: IrOperation) -> Result<IrValueId, LoweringFailure> {
        let result = self.new_value(ty)?;
        self.current_block_mut()?
            .instructions
            .push(IrInstruction::Define {
                result,
                ty,
                operation,
            });
        Ok(result)
    }

    /// The overlap groups this body can actualize, from the permitted and
    /// eligible chains the checker recorded.
    ///
    /// The judgment is the checker's; this only narrows it to what the emitted
    /// shape can carry, and every narrowing drops members rather than adding
    /// any. A group is a contiguous part of a permitted chain, kept only while
    ///
    /// - each site's `let` or expression statement lowered to exactly one
    ///   recorded call definition, so a chain member whose statement lowered
    ///   to something else (a `propagate`, or a discarded result's release,
    ///   for instance) ends the group;
    /// - every member's definition is in one block, so the handed-out call
    ///   and its join sit on one straight-line edge; and
    /// - no member but the last is an addressed binding, because promoting one
    ///   reads the call's value at the definition site — between the hand-out
    ///   and the join, where the value does not exist yet.
    ///
    /// Each contiguous part retains the chain's every-ordered-pair proof.
    /// Finally, already-proved adjacent pairs recover opportunities across a
    /// greedy run boundary. A call belongs to at most one emitted group.
    fn overlaps(&self) -> Vec<IrOverlap> {
        if self.overlap != OverlapLowering::On {
            return Vec::new();
        }
        let Some(permissions) = self.permissions else {
            return Vec::new();
        };
        let mut overlaps = Vec::new();
        let mut claimed = HashSet::new();
        let finish = |members: &mut Vec<IrValueId>,
                      claimed: &mut HashSet<IrValueId>,
                      overlaps: &mut Vec<IrOverlap>| {
            let members = std::mem::take(members);
            if members.len() >= 2 {
                claimed.extend(members.iter().copied());
                overlaps.push(IrOverlap { members });
            }
        };
        let runs = permissions
            .runs
            .iter()
            .map(|run| run.sites.iter().collect::<Vec<_>>());
        let pairs = permissions
            .pairs
            .iter()
            .filter(|pair| pair.verdict.is_eligible())
            .map(|pair| vec![&pair.first, &pair.second]);
        for sites in runs.chain(pairs) {
            let mut members = Vec::new();
            let mut home = None;
            for site in sites {
                // [PAR-1] judges every adjacent statement pair, so a run's
                // members include statements that are not calls. The hand-out
                // lowering has no form for one, so a non-call member ends the
                // group here. A later contiguous part can start another
                // group, but no group bridges this intervening statement.
                let Some(call) = &site.call else {
                    finish(&mut members, &mut claimed, &mut overlaps);
                    home = None;
                    continue;
                };
                let Some((block, value)) = self.call_results.get(call).copied() else {
                    finish(&mut members, &mut claimed, &mut overlaps);
                    home = None;
                    continue;
                };
                if claimed.contains(&value) {
                    finish(&mut members, &mut claimed, &mut overlaps);
                    home = None;
                    continue;
                }
                if home.is_some_and(|previous| previous != block) {
                    finish(&mut members, &mut claimed, &mut overlaps);
                }
                home = Some(block);
                let addressed = site
                    .binding
                    .is_some_and(|binding| self.addressed_bindings.contains(&binding));
                members.push(value);
                if addressed {
                    // This member must be the group's last, so it ends it.
                    finish(&mut members, &mut claimed, &mut overlaps);
                    home = None;
                }
            }
            finish(&mut members, &mut claimed, &mut overlaps);
        }
        overlaps
    }

    /// Records where a named-function call in call position landed, whatever
    /// written position it was in.
    ///
    /// The permission judgment reaches a call as a `let` right-hand side, as an
    /// expression statement, and as a `match` scrutinee alike, and all are
    /// named by their call occurrence, so one recording serves them all. Which
    /// of them a group can actually keep is decided later and by the IR alone:
    /// every member of a group must be defined in one block, and a scrutinee's
    /// own dispatch terminates its block, so a scrutinee call is only ever a
    /// group's last member.
    fn note_call_result(
        &mut self,
        expression: &CheckedExpression,
        value: IrValueId,
    ) -> Result<(), LoweringFailure> {
        let CheckedExpression::UserCall { call, .. } = expression else {
            return Ok(());
        };
        // The block the call's own definition landed in, which is the block
        // current after the arguments are lowered.
        let block = self.current.ok_or(LoweringFailure::InvalidCheckedProgram)?;
        self.call_results.insert(call.clone(), (block, value));
        Ok(())
    }

    fn lower_statements(
        &mut self,
        statements: &[CheckedStatement],
        give_target: Option<GiveTarget>,
    ) -> Result<(), LoweringFailure> {
        for statement in statements {
            if self.current.is_none() {
                return Err(LoweringFailure::InvalidCheckedProgram);
            }
            match statement {
                CheckedStatement::Let {
                    binding,
                    value: expression,
                    ..
                } => {
                    let value = self.expression(expression)?;
                    self.note_call_result(expression, value)?;
                    if self.bindings.insert(*binding, value).is_some() {
                        return Err(LoweringFailure::InvalidCheckedProgram);
                    }
                    self.promote_binding_if_needed(*binding)?;
                }
                CheckedStatement::PropagateLet {
                    binding,
                    scrutinee,
                    result_nominal,
                    return_nominal,
                    ok_type,
                    error_type,
                    error_drops,
                    context,
                    ..
                } => self.lower_propagate(
                    *binding,
                    scrutinee,
                    *result_nominal,
                    *return_nominal,
                    *ok_type,
                    *error_type,
                    error_drops,
                    context,
                )?,
                // [GRAM-4, CALL-4] one evaluation of the call, then one
                // projection per result ordinal in written order. The
                // callable hands back one value of its result-list nominal
                // and the ordinals are its fields, so this is the ordinary
                // struct projection every other field read uses.
                CheckedStatement::DestructuringLet {
                    bindings,
                    covered,
                    nominal,
                    value: expression,
                    ..
                } => {
                    let aggregate = self.expression(expression)?;
                    self.note_call_result(expression, aggregate)?;
                    let erased = self.erased(*nominal);
                    if self.value_type(aggregate)? != IrType::Nominal(erased) {
                        return Err(LoweringFailure::InvalidCheckedProgram);
                    }
                    // S39 a cell's one binder is its referent, loaded out
                    // while the cell's own storage is released.
                    if let Some(IrNominalKind::Box { .. }) =
                        self.nominals.get(erased.index()).map(IrNominal::kind)
                    {
                        let [(binding, ty, _)] = bindings.as_slice() else {
                            return Err(LoweringFailure::InvalidCheckedProgram);
                        };
                        let referent = lower_type(self.erasure, *ty)?;
                        let value = self.define(
                            referent,
                            IrOperation::BoxTake {
                                nominal: erased,
                                value: aggregate,
                            },
                        )?;
                        if self.bindings.insert(*binding, value).is_some() {
                            return Err(LoweringFailure::InvalidCheckedProgram);
                        }
                        self.promote_binding_if_needed(*binding)?;
                        continue;
                    }
                    // [WIN-3, STOR-3] every field a final `..` covers takes
                    // its compiler-derived release here, before the binders
                    // read the fields the statement names: the statement is
                    // the point at which the owner ceases to exist.
                    let mut lowered = Vec::with_capacity(covered.len());
                    for drop in covered {
                        lowered.push(self.lower_projected_drop(aggregate, drop)?);
                    }
                    self.append_drops(lowered)?;
                    for (binding, ty, field) in bindings {
                        let value = self.project_struct_path(aggregate, &[*field], true)?;
                        if self.value_type(value)? != lower_type(self.erasure, *ty)? {
                            return Err(LoweringFailure::InvalidCheckedProgram);
                        }
                        if self.bindings.insert(*binding, value).is_some() {
                            return Err(LoweringFailure::InvalidCheckedProgram);
                        }
                        self.promote_binding_if_needed(*binding)?;
                    }
                }
                CheckedStatement::Set {
                    target,
                    value,
                    displaces_live_value,
                    ..
                } => {
                    self.set(target, value, *displaces_live_value)?;
                }
                // A discarded result has no use, so the call may be handed out
                // exactly as a `let` binding it may.
                CheckedStatement::Evaluate {
                    value: expression, ..
                } => {
                    let value = self.expression(expression)?;
                    self.note_call_result(expression, value)?;
                }
                // The release reads the call's value at its definition, which
                // would lie between a hand-out and its join, so this call is
                // not recorded: its unavailable result ends any group through
                // it. Admitting it as a group's last member, as an addressed
                // binding is, remains a deferred opportunity (docs/todo.md).
                CheckedStatement::DropExpression {
                    value: expression,
                    drops,
                    ..
                } => {
                    let value = self.expression(expression)?;
                    let mut lowered = Vec::with_capacity(drops.len());
                    for drop in drops {
                        lowered.push(self.lower_projected_drop(value, drop)?);
                    }
                    self.append_drops(lowered)?;
                }
                // PRF-1 proof statements have already contributed their
                // checked fact to semantic flow. They have no runtime value,
                // effect, branch, or instruction.
                CheckedStatement::Proof(_) => {}
                CheckedStatement::Return { value, drops, .. } => {
                    if let CheckedExpression::UserCall {
                        tail_transfer: true,
                        arguments,
                        ..
                    } = value
                    {
                        let arguments = arguments
                            .iter()
                            .map(|argument| self.expression(argument))
                            .collect::<Result<Vec<_>, _>>()?;
                        let drops = self.lower_drops(drops)?;
                        let target = self
                            .tail_entry
                            .ok_or(LoweringFailure::InvalidCheckedProgram)?;
                        self.terminate(IrTerminator::Jump {
                            target,
                            arguments,
                            drops,
                        })?;
                        continue;
                    }
                    let value = self.expression(value)?;
                    let drops = self.lower_drops(drops)?;
                    self.terminate(IrTerminator::Return { value, drops })?;
                }
                CheckedStatement::Give { value, drops, .. } => {
                    let target = give_target
                        .as_ref()
                        .ok_or(LoweringFailure::InvalidCheckedProgram)?;
                    let value = self.expression(value)?;
                    if self.value_type(value)? != target.result {
                        return Err(LoweringFailure::InvalidCheckedProgram);
                    }
                    let mut arguments = Vec::with_capacity(1 + target.carried_bindings.len());
                    arguments.push(value);
                    arguments.extend(self.binding_values(&target.carried_bindings)?);
                    let drops = self.lower_drops(drops)?;
                    self.terminate(IrTerminator::Jump {
                        target: target.block,
                        arguments,
                        drops,
                    })?;
                }
                CheckedStatement::Loop {
                    id,
                    invariants: _,
                    body,
                    backedge_drops,
                } => self.lower_loop(*id, body, backedge_drops, give_target.clone())?,
                CheckedStatement::CountedRange {
                    id,
                    node_path,
                    binder,
                    lower,
                    upper,
                    // Source proof metadata has no runtime representation.
                    // The semantic checker must accept or reject it before
                    // lowering starts; this pattern deliberately erases it.
                    invariants: _,
                    body,
                    backedge_drops,
                } => self.lower_counted_range(
                    *id,
                    node_path,
                    *binder,
                    lower,
                    upper,
                    body,
                    backedge_drops,
                    give_target.clone(),
                )?,
                CheckedStatement::Break { target, drops } => {
                    let target = self
                        .loops
                        .iter()
                        .rev()
                        .find(|candidate| candidate.id == *target)
                        .cloned()
                        .ok_or(LoweringFailure::InvalidCheckedProgram)?;
                    let arguments = self.binding_values(&target.carried_bindings)?;
                    let drops = self.lower_drops(drops)?;
                    self.terminate(IrTerminator::Jump {
                        target: target.block,
                        arguments,
                        drops,
                    })?;
                }
                CheckedStatement::Match {
                    scrutinee,
                    enum_type,
                    arms,
                    continues,
                } => self.lower_match(
                    scrutinee,
                    *enum_type,
                    arms,
                    *continues,
                    None,
                    give_target.clone(),
                )?,
                CheckedStatement::ValueMatchLet {
                    binding,
                    result_type,
                    result_mode,
                    result_range_element,
                    scrutinee,
                    enum_type,
                    arms,
                    continues,
                    ..
                } => {
                    // [REF-1, REF-4] a binder every arm of which delivers a
                    // reference is a reference variable. An addressed join
                    // carries the selected address; a range join carries the
                    // selected pointer and count. Neither has the by-value
                    // representation of its written referent type.
                    let result = if *result_mode == CheckedMode::Range {
                        IrType::Range {
                            element: lower_element(
                                self.erasure,
                                result_range_element
                                    .ok_or(LoweringFailure::InvalidCheckedProgram)?,
                            )?,
                        }
                    } else {
                        if result_range_element.is_some() {
                            return Err(LoweringFailure::InvalidCheckedProgram);
                        }
                        lower_borrow_mode_type(
                            *result_mode,
                            lower_type(self.erasure, *result_type)?,
                            self.nominals,
                        )?
                    };
                    self.lower_match(
                        scrutinee,
                        *enum_type,
                        arms,
                        *continues,
                        Some((*binding, result)),
                        give_target.clone(),
                    )?;
                    if self.current.is_some() {
                        self.promote_binding_if_needed(*binding)?;
                    }
                }
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_match(
        &mut self,
        scrutinee: &CheckedExpression,
        enum_type: CheckedEnumType,
        arms: &[CheckedMatchArm],
        continues: bool,
        value_binding: Option<(BindingId, IrType)>,
        outer_give_target: Option<GiveTarget>,
    ) -> Result<(), LoweringFailure> {
        let scrutinee_expression = scrutinee;
        let borrowed_payloads = arms
            .iter()
            .flat_map(|arm| &arm.binders)
            .any(|binder| binder.mode != CheckedMode::Own);
        let (scrutinee, borrowed_address) = if borrowed_payloads {
            let address = self.lower_borrowed_place_address(scrutinee_expression)?;
            self.note_call_result(scrutinee_expression, address)?;
            (self.load_storage_value(address)?, Some(address))
        } else {
            let value = self.expression(scrutinee_expression)?;
            self.note_call_result(scrutinee_expression, value)?;
            (value, None)
        };
        self.lower_match_from_value(
            scrutinee,
            borrowed_address,
            enum_type,
            arms,
            continues,
            value_binding,
            outer_give_target,
        )?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn lower_match_from_value(
        &mut self,
        scrutinee: IrValueId,
        borrowed_address: Option<IrValueId>,
        enum_type: CheckedEnumType,
        arms: &[CheckedMatchArm],
        continues: bool,
        value_binding: Option<(BindingId, IrType)>,
        outer_give_target: Option<GiveTarget>,
    ) -> Result<(), LoweringFailure> {
        let base_bindings = self.bindings.clone();
        let mut carried_bindings = base_bindings.keys().copied().collect::<Vec<_>>();
        carried_bindings.sort_by_key(|binding| binding.0);
        let join = if continues {
            let mut parameter_types =
                Vec::with_capacity(carried_bindings.len() + usize::from(value_binding.is_some()));
            if let Some((_, ty)) = value_binding {
                parameter_types.push(ty);
            }
            for binding in &carried_bindings {
                let value = base_bindings
                    .get(binding)
                    .copied()
                    .ok_or(LoweringFailure::InvalidCheckedProgram)?;
                parameter_types.push(self.value_type(value)?);
            }
            let (block, parameters) = self.new_block(&parameter_types)?;
            Some((block, parameters))
        } else {
            None
        };
        let mut arm_blocks = Vec::with_capacity(arms.len());
        for _ in arms {
            arm_blocks.push(self.new_block(&[])?.0);
        }
        self.terminate(IrTerminator::Match {
            scrutinee,
            enum_type: crate::lowering::lower_enum_type(self.erasure, enum_type),
            targets: arms
                .iter()
                .zip(&arm_blocks)
                .map(|(arm, block)| IrMatchTarget {
                    tag: arm.tag,
                    block: *block,
                })
                .collect(),
        })?;
        for (arm, block) in arms.iter().zip(arm_blocks) {
            self.current = Some(block);
            self.bindings = base_bindings.clone();
            // [GRAM-10, WIN-3, STOR-3] an own-place arm's covered payloads
            // take their release on entry, before its binders read the
            // fields it names: the match is the point at which the scrutinee
            // ceases to exist.
            if !arm.covered.is_empty() {
                let CheckedEnumType::Nominal(nominal) = enum_type else {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                };
                let nominal = self.erased(nominal);
                let mut releases = Vec::with_capacity(arm.covered.len());
                for drop in &arm.covered {
                    let [field] = drop.fields.as_slice() else {
                        return Err(LoweringFailure::InvalidCheckedProgram);
                    };
                    let ty = lower_type(self.erasure, drop.ty)?;
                    let payload = self.define(
                        ty,
                        IrOperation::ProjectVariant {
                            aggregate: scrutinee,
                            nominal,
                            variant: arm.tag,
                            field: *field,
                        },
                    )?;
                    releases.push(self.lower_drop_subject(payload, &[], ty)?);
                }
                self.append_drops(releases)?;
            }
            for binder in &arm.binders {
                let CheckedEnumType::Nominal(nominal) = enum_type else {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                };
                let nominal = self.erased(nominal);
                let binder_ty = lower_type(self.erasure, binder.ty)?;
                let value = if binder.mode == CheckedMode::Own {
                    self.define(
                        binder_ty,
                        IrOperation::ProjectVariant {
                            aggregate: scrutinee,
                            nominal,
                            variant: arm.tag,
                            field: binder.field,
                        },
                    )?
                } else {
                    let address = borrowed_address.ok_or(LoweringFailure::InvalidCheckedProgram)?;
                    let referent =
                        IrAddressed::of(binder_ty).ok_or(LoweringFailure::InvalidCheckedProgram)?;
                    let address = self.define(
                        IrType::Address(referent),
                        IrOperation::ProjectAddress {
                            address,
                            projection: IrPlaceStep::EnumVariant {
                                nominal,
                                variant: arm.tag,
                                field: binder.field,
                            },
                        },
                    )?;
                    let representation =
                        lower_borrow_mode_type(binder.mode, binder_ty, self.nominals)?;
                    if representation == IrType::Address(referent) {
                        address
                    } else if representation == binder_ty {
                        self.load_storage_value(address)?
                    } else {
                        return Err(LoweringFailure::InvalidCheckedProgram);
                    }
                };
                if self.bindings.insert(binder.binding, value).is_some() {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                }
                if binder.mode == CheckedMode::Own {
                    self.promote_binding_if_needed(binder.binding)?;
                }
            }
            let arm_give_target = match value_binding {
                Some((_, ty)) => join.as_ref().map(|(block, _)| GiveTarget {
                    block: *block,
                    result: ty,
                    carried_bindings: carried_bindings.clone(),
                }),
                None => outer_give_target.clone(),
            };
            self.lower_statements(&arm.body, arm_give_target)?;
            if self.current.is_some() {
                let Some((join, _)) = &join else {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                };
                if value_binding.is_some() {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                }
                let drops = self.lower_drops(&arm.fallthrough_drops)?;
                let arguments = self.binding_values(&carried_bindings)?;
                self.terminate(IrTerminator::Jump {
                    target: *join,
                    arguments,
                    drops,
                })?;
            }
        }
        self.bindings = base_bindings;
        if let Some((join, parameters)) = join {
            self.current = Some(join);
            let carried_start = usize::from(value_binding.is_some());
            if parameters.len() != carried_start + carried_bindings.len() {
                return Err(LoweringFailure::InvalidCheckedProgram);
            }
            for (binding, value) in carried_bindings.iter().zip(&parameters[carried_start..]) {
                if self.bindings.insert(*binding, *value).is_none() {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                }
            }
            if let Some((binding, _)) = value_binding {
                let value = *parameters
                    .first()
                    .ok_or(LoweringFailure::InvalidCheckedProgram)?;
                if self.bindings.insert(binding, value).is_some() {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                }
            }
        } else {
            self.current = None;
        }
        Ok(())
    }

    fn expression(&mut self, expression: &CheckedExpression) -> Result<IrValueId, LoweringFailure> {
        match expression {
            CheckedExpression::Binding { binding, ty, .. } => {
                let value = self
                    .bindings
                    .get(binding)
                    .copied()
                    .ok_or(LoweringFailure::InvalidCheckedProgram)?;
                let expected = lower_type(self.erasure, *ty)?;
                let actual = self.value_type(value)?;
                let value = if self.addressed_bindings.contains(binding) {
                    self.load_storage_value(value)?
                } else {
                    value
                };
                // [REF-4, TYPE-8] a range reference read bare delivers its
                // own pointer-and-count descriptor; `&[T]` is a reference
                // kind and not a type, so the checked type beside it is the
                // element type the range names.
                if self.value_type(value)? != expected
                    && !matches!(
                        (actual, expected),
                        (IrType::Address(referent), _) if referent.ty() == expected
                    )
                    && !matches!(actual, IrType::Range { element } if self.element_type(element)? == expected)
                {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                }
                Ok(value)
            }
            CheckedExpression::Constant(value) => {
                let ty = lower_type(self.erasure, value.ty())?;
                let constant = lower_scalar_constant(value)?;
                self.define(ty, IrOperation::Constant(constant))
            }
            CheckedExpression::NamedConstant { value, .. } => {
                let ty = lower_type(self.erasure, value.ty())?;
                let constant = lower_scalar_constant(value)?;
                self.define(ty, IrOperation::Constant(constant))
            }
            CheckedExpression::UserCall {
                call,
                arguments,
                result_borrow,
                allocation,
                ..
            } => {
                let function = self
                    .physical_calls
                    .iter()
                    .find_map(|(site, target)| (site == call).then_some(*target))
                    .ok_or(LoweringFailure::InvalidCheckedProgram)?;
                let source_arguments = arguments.iter().map(lower_source_argument).collect();
                let source_allocation = allocation
                    .map(|allocation| {
                        let IrType::Nominal(cell) = lower_type(self.erasure, allocation.cell)?
                        else {
                            return Err(LoweringFailure::InvalidCheckedProgram);
                        };
                        Ok(IrSourceAllocation {
                            cell,
                            count_argument: allocation.count,
                            layout_ceiling: allocation.layout_ceiling.into(),
                            source_length_upper_bound: allocation
                                .source_length_upper_bound()
                                .ok_or(LoweringFailure::InvalidCheckedProgram)?,
                        })
                    })
                    .transpose()?;
                let arguments = arguments
                    .iter()
                    .map(|argument| self.expression(argument))
                    .collect::<Result<Vec<_>, _>>()?;
                // The definition takes the callee's declared IR result, which
                // carries the result mode: a borrow-returning callee delivers
                // an address, not a referent value [REF-1, TYPE-7].
                let result = *self
                    .function_results
                    .get(function as usize)
                    .ok_or(LoweringFailure::InvalidCheckedProgram)?;
                let result = self.define(
                    result,
                    IrOperation::Call {
                        function,
                        arguments,
                    },
                )?;
                self.source_calls.push(IrSourceCall {
                    result,
                    arguments: source_arguments,
                    returned_borrow_argument: result_borrow.as_ref().map(|borrow| borrow.argument),
                    allocation: source_allocation,
                });
                Ok(result)
            }
            CheckedExpression::IntegerOperation {
                operation,
                operand_type,
                arguments,
                ..
            } => {
                let arguments = arguments
                    .iter()
                    .map(|argument| self.expression(argument))
                    .collect::<Result<Vec<_>, _>>()?;
                self.define(
                    lower_type(self.erasure, expression.ty())?,
                    IrOperation::Integer {
                        operation: (*operation).into(),
                        operand_type: lower_type(self.erasure, *operand_type)?,
                        arguments,
                    },
                )
            }
            CheckedExpression::FloatOperation {
                operation,
                operand_type,
                arguments,
                ..
            } => {
                let arguments = arguments
                    .iter()
                    .map(|argument| self.expression(argument))
                    .collect::<Result<Vec<_>, _>>()?;
                self.define(
                    lower_type(self.erasure, expression.ty())?,
                    IrOperation::Float {
                        operation: (*operation).into(),
                        operand_type: lower_type(self.erasure, *operand_type)?,
                        arguments,
                    },
                )
            }
            CheckedExpression::NumericConversion {
                mode,
                source,
                destination,
                value,
                ..
            } => {
                let value = self.expression(value)?;
                self.define(
                    lower_type(self.erasure, expression.ty())?,
                    IrOperation::NumericConversion {
                        mode: (*mode).into(),
                        source_type: lower_numeric_type(*source)?,
                        destination_type: lower_numeric_type(*destination)?,
                        value,
                    },
                )
            }
            CheckedExpression::Reinterpret {
                source,
                destination,
                value,
                ..
            } => {
                let value = self.expression(value)?;
                self.define(
                    lower_numeric_type(*destination)?,
                    IrOperation::Reinterpret {
                        source_type: lower_numeric_type(*source)?,
                        destination_type: lower_numeric_type(*destination)?,
                        value,
                    },
                )
            }
            CheckedExpression::BooleanOperation {
                operation,
                arguments,
                ..
            } => {
                let arguments = arguments
                    .iter()
                    .map(|argument| self.expression(argument))
                    .collect::<Result<Vec<_>, _>>()?;
                self.define(
                    IrType::Bool,
                    IrOperation::Boolean {
                        operation: (*operation).into(),
                        arguments,
                    },
                )
            }
            CheckedExpression::EnumEquality {
                equal,
                operand_type,
                arguments,
                ..
            } => {
                let [left, right] = arguments.as_slice() else {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                };
                let left = self.expression(left)?;
                let right = self.expression(right)?;
                self.define(
                    IrType::Bool,
                    IrOperation::EnumEquality {
                        equal: *equal,
                        operand_type: lower_type(self.erasure, *operand_type)?,
                        arguments: [left, right],
                    },
                )
            }
            CheckedExpression::ArrayMeasure {
                measure,
                root,
                length,
            } => {
                if let Some(constant) = fixed_measure(*measure, MeasuredKind::ConstantArray) {
                    return self.lower_fixed_measure(constant);
                }
                let (_, ty) = self.array_root(root)?;
                let IrType::Array { length: actual, .. } = ty else {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                };
                let length = length
                    .value()
                    .ok_or(LoweringFailure::InvalidCheckedProgram)?;
                if actual != length {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                }
                self.define(
                    IrType::Integer {
                        width: 64,
                        signed: false,
                    },
                    IrOperation::Constant(IrConstant::Integer {
                        ty: IrType::Integer {
                            width: 64,
                            signed: false,
                        },
                        bits: length,
                    }),
                )
            }
            CheckedExpression::ArrayIndex {
                root,
                element_type,
                length,
                offset,
                target_domain,
                ..
            } => {
                let (root, ty) = self.array_root(root)?;
                let IrType::Array {
                    element,
                    length: actual,
                } = ty
                else {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                };
                let length = length
                    .value()
                    .ok_or(LoweringFailure::InvalidCheckedProgram)?;
                if self.element_type(element)? != lower_type(self.erasure, *element_type)?
                    || actual != length
                {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                }
                let offset = self.expression(offset)?;
                if self.value_type(offset)?
                    != (IrType::Integer {
                        width: 64,
                        signed: false,
                    })
                {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                }
                self.define(
                    self.element_type(element)?,
                    IrOperation::ArrayIndex {
                        root,
                        offset,
                        target_domain: (*target_domain).into(),
                    },
                )
            }
            CheckedExpression::BufferMeasure { measure, root } => {
                match fixed_measure(*measure, MeasuredKind::RuntimeArray) {
                    Some(constant) => self.lower_fixed_measure(constant),
                    None => self.lower_buffer_length(root),
                }
            }
            CheckedExpression::ContainerMeasure { measure, root } => {
                self.lower_container_measure(*measure, root)
            }
            CheckedExpression::RangeElementMeasure { measure, place, .. } => {
                self.lower_range_element_measure(*measure, place)
            }
            CheckedExpression::ReadStorage { root, .. } => {
                let address = self.lower_place_address(root)?;
                self.load_storage_value(address)
            }
            CheckedExpression::BufferIndex {
                root,
                offset,
                target_domain,
                ..
            } => self.lower_buffer_index(root, offset, *target_domain),
            // [REF-4] the range reference: its formation, its one measure,
            // and one discharged element read.
            CheckedExpression::RangeOf {
                source,
                element,
                start,
                end,
                ..
            } => self.lower_range_of(source, start, end, *element),
            CheckedExpression::RangeMeasure { measure, root } => {
                match fixed_measure(*measure, MeasuredKind::Range) {
                    Some(constant) => self.lower_fixed_measure(constant),
                    None => self.lower_range_measure(root),
                }
            }
            CheckedExpression::RangeIndex { place, .. } => {
                self.lower_range_index(&place.root, &place.offset, &place.path, place.target_domain)
            }
            CheckedExpression::BorrowRangeIndex { place, .. } => self.lower_range_address(
                &place.root,
                &place.offset,
                &place.path,
                place.target_domain,
            ),
            CheckedExpression::BoxDeref { nominal, value, .. } => {
                let value = self.expression(value)?;
                let nominal = self.erased(*nominal);
                let IrNominalKind::Box { referent, .. } = self
                    .nominals
                    .get(nominal.index())
                    .ok_or(LoweringFailure::InvalidCheckedProgram)?
                    .kind
                else {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                };
                self.define(referent, IrOperation::BoxDeref { nominal, value })
            }
            // [TYPE-9, WIN-3] load the selected value before releasing any
            // enclosing cell or residual field of the consumed root.
            CheckedExpression::BoxTake {
                binding,
                path,
                cleanup,
                referent,
                ..
            } => {
                let root = self
                    .bindings
                    .get(binding)
                    .copied()
                    .ok_or(LoweringFailure::InvalidCheckedProgram)?;
                if !matches!(self.value_type(root)?, IrType::Address(_)) {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                }
                let address = self.project_address_path(root, path)?;
                let selected = self.load_storage_value(address)?;
                if self.value_type(selected)? != lower_type(self.erasure, *referent)? {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                }
                for action in cleanup {
                    match action {
                        crate::semantic::CheckedOwnedTakeCleanup::Drop { path, ty } => {
                            let address = self.project_address_path(root, path)?;
                            let IrType::Address(actual) = self.value_type(address)? else {
                                return Err(LoweringFailure::InvalidCheckedProgram);
                            };
                            let ty = lower_type(self.erasure, *ty)?;
                            if actual.ty() != ty {
                                return Err(LoweringFailure::InvalidCheckedProgram);
                            }
                            self.append_drops(vec![IrDrop {
                                subject: IrDropSubject::Place(address),
                                ty,
                            }])?;
                        }
                        crate::semantic::CheckedOwnedTakeCleanup::BoxShell {
                            path,
                            nominal,
                            referent,
                        } => {
                            let address = self.project_address_path(root, path)?;
                            let owner = self.load_storage_value(address)?;
                            let nominal = self.erased(*nominal);
                            let _ = lower_type(self.erasure, *referent)?;
                            self.define(
                                IrType::Unit,
                                IrOperation::CellFree {
                                    nominal,
                                    value: owner,
                                },
                            )?;
                        }
                    }
                }
                Ok(selected)
            }
            CheckedExpression::BorrowAddressed { root, .. } => self.lower_place_address(root),
            CheckedExpression::DerefAddressed { binding, ty, .. } => {
                let value = self.binding_value(*binding)?;
                if self.value_type(value)? != lower_type(self.erasure, *ty)? {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                }
                Ok(value)
            }
            CheckedExpression::ConstructStruct {
                nominal, fields, ..
            } => {
                let fields = fields
                    .iter()
                    .map(|field| self.expression(field))
                    .collect::<Result<Vec<_>, _>>()?;
                let nominal = self.erased(*nominal);
                self.define(
                    IrType::Nominal(nominal),
                    IrOperation::ConstructStruct { nominal, fields },
                )
            }
            CheckedExpression::ConstructEnum {
                nominal,
                variant,
                fields,
                ..
            } => {
                let fields = fields
                    .iter()
                    .map(|field| self.expression(field))
                    .collect::<Result<Vec<_>, _>>()?;
                let nominal = self.erased(*nominal);
                self.define(
                    IrType::Nominal(nominal),
                    IrOperation::ConstructEnum {
                        nominal,
                        variant: *variant,
                        fields,
                    },
                )
            }
            CheckedExpression::Project {
                binding,
                fields,
                ty,
                consume_root,
                residual_drops,
                ..
            } => {
                if self.bindings.get(binding).copied().is_some_and(|storage| {
                    matches!(self.value_type(storage), Ok(IrType::Address(_)))
                }) && !consume_root
                    && residual_drops.is_empty()
                {
                    let path = fields
                        .iter()
                        .copied()
                        .map(crate::semantic::CheckedPlaceStep::Field)
                        .collect();
                    let root = crate::semantic::CheckedContainerRoot {
                        root: crate::semantic::CheckedPlaceRoot::Binding(*binding),
                        path,
                        ty: *ty,
                    };
                    let address = self.lower_place_address(&root)?;
                    return self.load_storage_value(address);
                }
                let root = self.binding_value(*binding)?;
                let mut lowered_drops = Vec::with_capacity(residual_drops.len());
                for drop in residual_drops {
                    lowered_drops.push(self.lower_projected_drop(root, drop)?);
                }
                let value = self.project_struct_path(root, fields, *consume_root)?;
                if self.value_type(value)? != lower_type(self.erasure, *ty)? {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                }
                self.append_drops(lowered_drops)?;
                Ok(value)
            }
            CheckedExpression::ProjectValue {
                value,
                nominal,
                field,
                ty,
                ..
            } => {
                let aggregate = self.expression(value)?;
                let nominal = self.erased(*nominal);
                self.define(
                    lower_type(self.erasure, *ty)?,
                    IrOperation::ProjectStruct {
                        aggregate,
                        nominal,
                        field: *field,
                        consume_root: false,
                    },
                )
            }
        }
    }

    pub(super) fn array_root(
        &mut self,
        root: &CheckedArrayRoot,
    ) -> Result<(IrArrayRoot, IrType), LoweringFailure> {
        match root {
            CheckedArrayRoot::Binding {
                binding, fields, ..
            } => {
                let storage = self
                    .bindings
                    .get(binding)
                    .copied()
                    .ok_or(LoweringFailure::InvalidCheckedProgram)?;
                let root = self.load_storage_value(storage)?;
                let value = if fields.is_empty() {
                    root
                } else {
                    self.project_struct_path(root, fields, false)?
                };
                Ok((IrArrayRoot::Value(value), self.value_type(value)?))
            }
            CheckedArrayRoot::Constant(constant) => {
                let constant = self
                    .constants
                    .get(constant.0 as usize)
                    .ok_or(LoweringFailure::InvalidCheckedProgram)?;
                Ok((
                    IrArrayRoot::Constant(IrConstantId(constant.id().0)),
                    constant.ty(),
                ))
            }
        }
    }

    /// One target root's new value, written to the storage that holds it.
    fn commit_root_storage(
        &mut self,
        binding: BindingId,
        storage: IrValueId,
        replacement: IrValueId,
    ) -> Result<(), LoweringFailure> {
        let stored = match self.value_type(storage)? {
            IrType::Address(referent) => {
                if self.value_type(replacement)? != referent.ty() {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                }
                self.store_addressed(storage, replacement, referent)?;
                storage
            }
            _ => replacement,
        };
        if self.bindings.insert(binding, stored) != Some(storage) {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        Ok(())
    }

    /// [SET-1] the one-target commit: the right-hand side is
    /// evaluated once and completely, then the one target is written by the
    /// same commit a target list uses.
    ///
    /// [WIN-3] assigning over an owned place releases the old value, so the
    /// displaced owner is read at the commit and released after the write.
    /// The write comes first so that the place holds its new owner for the
    /// whole of the old one's release walk.
    fn set(
        &mut self,
        target: &CheckedSetTarget,
        value: &CheckedExpression,
        displaces_live_value: bool,
    ) -> Result<(), LoweringFailure> {
        let target = self.prepare_target(target, displaces_live_value)?;
        let value = self.expression(value)?;
        let displaced = self.displaced_release(&target)?;
        self.write_target(&target, value)?;
        if let Some(drop) = displaced {
            self.append_drops(vec![drop])?;
        }
        Ok(())
    }

    fn project_struct_path(
        &mut self,
        mut value: IrValueId,
        fields: &[u32],
        consume_root: bool,
    ) -> Result<IrValueId, LoweringFailure> {
        if fields.is_empty() {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        for field in fields {
            let IrType::Nominal(nominal) = self.value_type(value)? else {
                return Err(LoweringFailure::InvalidCheckedProgram);
            };
            let field_ty = match &self
                .nominals
                .get(nominal.index())
                .ok_or(LoweringFailure::InvalidCheckedProgram)?
                .kind
            {
                IrNominalKind::Struct { fields } => {
                    fields
                        .get(*field as usize)
                        .ok_or(LoweringFailure::InvalidCheckedProgram)?
                        .ty
                }
                // An opaque system resource has no writer-visible field, so no
                // struct path reaches through one.
                IrNominalKind::Enum { .. } | IrNominalKind::Box { .. } | IrNominalKind::Opaque => {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                }
            };
            value = self.define(
                field_ty,
                IrOperation::ProjectStruct {
                    aggregate: value,
                    nominal,
                    field: *field,
                    consume_root,
                },
            )?;
        }
        Ok(value)
    }

    fn replace_struct_path(
        &mut self,
        aggregate: IrValueId,
        fields: &[u32],
        replacement: IrValueId,
    ) -> Result<IrValueId, LoweringFailure> {
        let Some((field, remaining)) = fields.split_first() else {
            return Err(LoweringFailure::InvalidCheckedProgram);
        };
        let IrType::Nominal(nominal) = self.value_type(aggregate)? else {
            return Err(LoweringFailure::InvalidCheckedProgram);
        };
        let field_ty = match &self
            .nominals
            .get(nominal.index())
            .ok_or(LoweringFailure::InvalidCheckedProgram)?
            .kind
        {
            IrNominalKind::Struct { fields } => {
                fields
                    .get(*field as usize)
                    .ok_or(LoweringFailure::InvalidCheckedProgram)?
                    .ty
            }
            // An opaque system resource has no writer-visible field, so no
            // struct path reaches through one.
            IrNominalKind::Enum { .. } | IrNominalKind::Box { .. } | IrNominalKind::Opaque => {
                return Err(LoweringFailure::InvalidCheckedProgram);
            }
        };
        let value = if remaining.is_empty() {
            if self.value_type(replacement)? != field_ty {
                return Err(LoweringFailure::InvalidCheckedProgram);
            }
            replacement
        } else {
            let selected = self.define(
                field_ty,
                IrOperation::ProjectStruct {
                    aggregate,
                    nominal,
                    field: *field,
                    consume_root: false,
                },
            )?;
            self.replace_struct_path(selected, remaining, replacement)?
        };
        self.define(
            IrType::Nominal(nominal),
            IrOperation::InsertStruct {
                aggregate,
                nominal,
                field: *field,
                value,
            },
        )
    }

    fn append_drops(&mut self, drops: Vec<IrDrop>) -> Result<(), LoweringFailure> {
        if !drops.is_empty() {
            self.current_block_mut()?
                .instructions
                .push(IrInstruction::Drops(drops));
        }
        Ok(())
    }

    fn lower_projected_drop(
        &mut self,
        root: IrValueId,
        drop: &CheckedProjectedDrop,
    ) -> Result<IrDrop, LoweringFailure> {
        self.lower_drop_subject(root, &drop.fields, lower_type(self.erasure, drop.ty)?)
    }

    fn lower_drop_subject(
        &mut self,
        root: IrValueId,
        fields: &[u32],
        ty: IrType,
    ) -> Result<IrDrop, LoweringFailure> {
        // [PROV-6] the empty path is the value's own release-graph node.
        // A checked release of an addressed binding needs its place, not an
        // immutable snapshot of the entire enclosing owner.
        let (subject, actual) = if matches!(self.value_type(root)?, IrType::Address(_)) {
            let path: Vec<_> = fields
                .iter()
                .copied()
                .map(crate::semantic::CheckedPlaceStep::Field)
                .collect();
            let address = self.project_address_path(root, &path)?;
            let IrType::Address(referent) = self.value_type(address)? else {
                return Err(LoweringFailure::InvalidCheckedProgram);
            };
            (IrDropSubject::Place(address), referent.ty())
        } else {
            let value = if fields.is_empty() {
                root
            } else {
                self.project_struct_path(root, fields, false)?
            };
            (IrDropSubject::Value(value), self.value_type(value)?)
        };
        if actual != ty {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        Ok(IrDrop { subject, ty })
    }

    fn value_type(&self, value: IrValueId) -> Result<IrType, LoweringFailure> {
        self.values
            .get(value.index())
            .copied()
            .ok_or(LoweringFailure::InvalidCheckedProgram)
    }

    fn element_type(&self, element: IrElement) -> Result<IrType, LoweringFailure> {
        self.elements
            .get(element.index())
            .copied()
            .ok_or(LoweringFailure::InvalidCheckedProgram)
    }

    fn binding_values(&self, bindings: &[BindingId]) -> Result<Vec<IrValueId>, LoweringFailure> {
        bindings
            .iter()
            .map(|binding| {
                self.bindings
                    .get(binding)
                    .copied()
                    .ok_or(LoweringFailure::InvalidCheckedProgram)
            })
            .collect()
    }

    fn lower_drops(&mut self, drops: &[CheckedDrop]) -> Result<Vec<IrDrop>, LoweringFailure> {
        let mut lowered = Vec::with_capacity(drops.len());
        for drop in drops {
            let root = self
                .bindings
                .get(&drop.binding)
                .copied()
                .ok_or(LoweringFailure::InvalidCheckedProgram)?;
            let ty = lower_type(self.erasure, drop.ty)?;
            // The checked program already fixed what this release performs
            // [STOR-3]; lowering preserves the record and the edge's reverse
            // declaration order rather than rederiving either.
            lowered.push(self.lower_drop_subject(root, &drop.fields, ty)?);
        }
        Ok(lowered)
    }
}

/// The compile-time value of one [MSR-1] measure cell, when the table fixes
/// it. A cell the table gives the measured value's own extent is loaded at
/// run time instead, which is the `None` case.
const fn fixed_measure(measure: CheckedMeasure, measured: MeasuredKind) -> Option<u64> {
    match measure.cell(measured) {
        MeasureCell::ExactConstant(value) => Some(value),
        MeasureCell::ExactExtent
        | MeasureCell::ExactTypeConstant
        | MeasureCell::ExactRuntime
        | MeasureCell::Bounded
        | MeasureCell::Absent => None,
    }
}
