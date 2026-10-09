//! Composition-wide handler-word planning, before final target layout.
//!
//! Only the ordinary split planner selects a dispatch family. The plan is
//! explicit emission input; its counts also enter every nominal layout used
//! by allocation, copies, frames, calls and constructors. Fragment builds use
//! an empty plan until cross-fragment address binding is implemented.

use std::borrow::Cow;

use super::*;

#[derive(Default)]
pub(super) struct DispatchLayoutPlan {
    families: HashMap<IrNominalId, Vec<DispatchFamily>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct DispatchFamily {
    pub(super) owner: String,
    pub(super) handlers: Vec<String>,
}

impl DispatchLayoutPlan {
    pub(super) fn families(&self, nominal: IrNominalId) -> &[DispatchFamily] {
        self.families.get(&nominal).map_or(&[], Vec::as_slice)
    }

    pub(super) fn word(&self, nominal: IrNominalId, owner: &str) -> Option<usize> {
        self.families(nominal)
            .iter()
            .position(|family| family.owner == owner)
    }

    fn apply(&self, program: &mut IrProgram) -> Result<(), BackendFailure> {
        for nominal in &mut program.nominals {
            nominal.handler_words = u32::try_from(self.families(nominal.id()).len())
                .map_err(|_| BackendFailure::CounterOverflow)?;
        }
        Ok(())
    }
}

pub(crate) struct PreparedDispatch<'program> {
    pub(crate) program: Cow<'program, IrProgram>,
    pub(super) plan: DispatchLayoutPlan,
}

/// Select all families before laying out any construction site. An enum either
/// has a word for every selected family or keeps its original representation.
/// The ordinary layouts are used only to discover eligible splits; after the
/// counts are installed both layout qualification and split planning repeat.
pub(crate) fn prepare_dispatch_layout(
    program: &IrProgram,
    target: TargetLayout,
    fragments: bool,
) -> Result<PreparedDispatch<'_>, BackendFailure> {
    validate_program(target, program).map_err(BackendFailure::TargetLayout)?;
    let empty = || PreparedDispatch {
        program: Cow::Borrowed(program),
        plan: DispatchLayoutPlan::default(),
    };
    if fragments {
        return Ok(empty());
    }
    let mut plan = census(program, target, &DispatchLayoutPlan::default())?;
    let runtime = runtime_nominals(program)?;
    for nominal in program.nominals() {
        if runtime.contains(&nominal.id()) {
            plan.families.remove(&nominal.id());
        }
    }
    if plan.families.is_empty() {
        return Ok(empty());
    }
    let mut selected = program.clone();
    loop {
        plan.apply(&mut selected)?;
        // Nominal order makes pruning deterministic even for nested enums.
        let mut removed = false;
        for nominal in program.nominals() {
            if !plan.families.contains_key(&nominal.id()) {
                continue;
            }
            let fits = match crate::target::threaded_enum_fits(target, &selected, nominal.id()) {
                Ok(fits) => fits,
                Err(TargetLayoutFailure::Unrepresentable(_)) => false,
                Err(failure) => return Err(BackendFailure::TargetLayout(failure)),
            };
            if !fits {
                plan.families.remove(&nominal.id());
                removed = true;
            }
        }
        if removed {
            continue;
        }
        // Optional storage must never turn an ordinary qualified program into
        // a target failure, including inline arrays and bounded lane frames.
        match validate_program(target, &selected) {
            Ok(()) => {}
            Err(TargetLayoutFailure::Unrepresentable(_)) => return Ok(empty()),
            Err(failure) => return Err(BackendFailure::TargetLayout(failure)),
        }
        let actual = census(&selected, target, &plan)?;
        plan.families.retain(|nominal, families| {
            let keep = actual
                .families(*nominal)
                .iter()
                .map(|family| &family.owner)
                .eq(families.iter().map(|family| &family.owner));
            if keep {
                // A selected representation may change a register-return
                // wrapper's symbol. Bind constructors to the final ABI.
                *families = actual.families(*nominal).to_vec();
            }
            removed |= !keep;
            keep
        });
        if !removed {
            return Ok(PreparedDispatch {
                program: Cow::Owned(selected),
                plan,
            });
        }
        // Each retry removes at least one enum; no layout can retain an
        // address of an arm that the final emission will not define.
    }
}

fn census(
    program: &IrProgram,
    target: TargetLayout,
    dispatch_layout: &DispatchLayoutPlan,
) -> Result<DispatchLayoutPlan, BackendFailure> {
    let mut intrinsics = BTreeSet::new();
    let mut thunks = ParallelThunks::default();
    let refusal_clones = if program.sequential_compute_refusal() {
        sequential_clone_set(program)
    } else {
        HashSet::new()
    };
    let frontier_clones = if program.recursion_budget().is_some() {
        sequential_clone_set(program)
    } else {
        HashSet::new()
    };
    let frontiers = RecursiveFrontiers::new(program, &frontier_clones);
    let mut result = DispatchLayoutPlan::default();
    for (ordinal, function) in program.functions().iter().enumerate() {
        let reachable = dispatch::reachable(function)?;
        if frontiers.grain(ordinal).is_some() || dispatch::find(function, &reachable).0.is_none() {
            continue;
        }
        let mut emitter = FunctionEmitter::new(
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
                window_address_facts: WindowAddressFacts::Emit,
                dispatch_layout,
            },
        )?;
        let (_, symbol, _, abi, _) = emitter.body_abi()?;
        let returned = if abi.result().uses_destination() {
            "void".to_owned()
        } else {
            llvm_type(program, abi.result().ty())?
        };
        if let Some(split) = emitter.plan_dispatch(
            &reachable,
            &symbol,
            abi.result().uses_destination(),
            &returned,
        )? {
            result
                .families
                .entry(split.plan.matched)
                .or_default()
                .push(DispatchFamily {
                    owner: function.name().to_owned(),
                    handlers: split.handlers_by_tag(),
                });
        }
    }
    Ok(result)
}

/// Types whose bytes cross a runtime boundary, including boundaries hidden
/// inside compiler-owned bodies. Shared storage is discovered from its IR
/// shape, not the constructor's name or whether that constructor has a body.
fn runtime_nominals(program: &IrProgram) -> Result<HashSet<IrNominalId>, BackendFailure> {
    let mut pending = Vec::new();
    for nominal in program.nominals() {
        if matches!(
            nominal.kind(),
            IrNominalKind::Shared { .. } | IrNominalKind::Opaque
        ) {
            pending.push(IrType::Nominal(nominal.id()));
        }
    }
    for function in program.functions() {
        // Linked ordinary and waiting functions include the completion bridge,
        // I/O, process and stop-signal APIs. Waiting bodies also place their
        // signature values in runtime-owned context storage.
        if function.blocks().is_empty() || function.waits() {
            pending.push(function.result());
            pending.extend(function.parameters().iter().map(|(_, ty)| *ty));
        }
        let handed_out: HashSet<_> = function
            .overlaps()
            .iter()
            .flat_map(IrOverlap::handed_out)
            .copied()
            .collect();
        for instruction in function.blocks().iter().flat_map(IrBlock::instructions) {
            let IrInstruction::Define { result, ty, operation } = instruction else {
                continue;
            };
            match operation {
                IrOperation::ContextStart {
                    function: callee, ..
                }
                | IrOperation::ContextStartBound {
                    function: callee, ..
                } => {
                    let callee = program
                        .functions()
                        .get(*callee as usize)
                        .ok_or(BackendFailure::InvalidIr)?;
                    pending.push(callee.result());
                    pending.extend(callee.parameters().iter().map(|(_, ty)| *ty));
                }
                IrOperation::LoopSplit { .. } => {
                    pending.push(*ty);
                    for operand in operation.operands() {
                        pending.push(
                            function
                                .value_type(operand)
                                .ok_or(BackendFailure::InvalidIr)?,
                        );
                    }
                }
                IrOperation::Call { .. } if handed_out.contains(result) => {
                    pending.push(*ty);
                    for operand in operation.operands() {
                        pending.push(
                            function
                                .value_type(operand)
                                .ok_or(BackendFailure::InvalidIr)?,
                        );
                    }
                }
                _ => {}
            }
        }
    }

    // Nominal containment is bidirectional: a runtime payload keeps its
    // enclosing values ordinary too. Primitive leaves do not connect otherwise
    // unrelated types (two enums containing u64 remain independent).
    let mut neighbours: HashMap<IrNominalId, HashSet<IrNominalId>> = HashMap::new();
    for nominal in program.nominals() {
        let mut children = Vec::new();
        match nominal.kind() {
            IrNominalKind::Struct { fields } => {
                children.extend(fields.iter().map(|f| f.ty()));
            }
            IrNominalKind::Enum { variants } => {
                children.extend(variants.iter().flat_map(|v| v.fields()).map(|f| f.ty()));
            }
            IrNominalKind::Box { referent, .. } => children.push(*referent),
            IrNominalKind::Shared { state, shape } => {
                children.push(*state);
                if let IrShared::Map { entry } = shape {
                    children.push(*entry);
                }
            }
            IrNominalKind::Opaque => {}
        }
        for child in contained_nominals(program, children)? {
            neighbours.entry(nominal.id()).or_default().insert(child);
            neighbours.entry(child).or_default().insert(nominal.id());
        }
    }
    let mut nominals = contained_nominals(program, pending)?;
    let mut pending: Vec<_> = nominals.iter().copied().collect();
    while let Some(id) = pending.pop() {
        for child in neighbours.get(&id).into_iter().flatten() {
            if nominals.insert(*child) {
                pending.push(*child);
            }
        }
    }
    Ok(nominals)
}

/// Resolve descriptors and references down to nominal leaves. The graph above
/// follows each nominal's payload once, in both directions, including cycles.
fn contained_nominals(
    program: &IrProgram,
    mut pending: Vec<IrType>,
) -> Result<HashSet<IrNominalId>, BackendFailure> {
    let mut visited = HashSet::new();
    let mut nominals = HashSet::new();
    while let Some(ty) = pending.pop() {
        if !visited.insert(ty) {
            continue;
        }
        match ty {
            IrType::Nominal(id) => {
                program.nominal(id).ok_or(BackendFailure::InvalidIr)?;
                nominals.insert(id);
            }
            IrType::Address(referent) => pending.push(referent.ty()),
            IrType::Array { element, .. }
            | IrType::Window { element, .. }
            | IrType::Buffer { element }
            | IrType::Segments { element }
            | IrType::Range { element }
            | IrType::Run { element }
            | IrType::Entries { element } => {
                pending.push(program.element(element).ok_or(BackendFailure::InvalidIr)?);
            }
            IrType::RuntimeBoxPayload { nominal } => pending.push(IrType::Nominal(nominal)),
            IrType::Unit
            | IrType::Bool
            | IrType::Integer { .. }
            | IrType::Float { .. }
            | IrType::KeySet => {}
        }
    }
    Ok(nominals)
}
