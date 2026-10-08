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
    let foreign = foreign_nominals(program)?;
    for nominal in program.nominals() {
        if foreign.contains(&nominal.id()) {
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

fn foreign_nominals(
    program: &crate::IrProgram,
) -> Result<HashSet<crate::IrNominalId>, BackendFailure> {
    let mut pending = Vec::new();
    for function in program.functions().iter().filter(|f| f.blocks().is_empty()) {
        pending.push(function.result());
        pending.extend(function.parameters().iter().map(|(_, ty)| *ty));
    }
    let mut visited = HashSet::new();
    let mut nominals = HashSet::new();
    while let Some(ty) = pending.pop() {
        if !visited.insert(ty) {
            continue;
        }
        match ty {
            IrType::Nominal(id) => {
                nominals.insert(id);
                match program.nominal(id).ok_or(BackendFailure::InvalidIr)?.kind() {
                    crate::IrNominalKind::Struct { fields } => {
                        pending.extend(fields.iter().map(|f| f.ty()))
                    }
                    crate::IrNominalKind::Enum { variants } => {
                        pending.extend(variants.iter().flat_map(|v| v.fields()).map(|f| f.ty()))
                    }
                    crate::IrNominalKind::Box { referent, .. } => pending.push(*referent),
                    crate::IrNominalKind::Shared { state, shape } => {
                        pending.push(*state);
                        if let crate::IrShared::Map { entry } = shape {
                            pending.push(*entry);
                        }
                    }
                    crate::IrNominalKind::Opaque => {}
                }
            }
            IrType::Address(referent) => pending.push(referent.ty()),
            IrType::Array { element, .. }
            | IrType::Window { element, .. }
            | IrType::Buffer { element }
            | IrType::Segments { element }
            | IrType::Range { element }
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

