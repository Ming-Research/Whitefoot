//! Canonical source closure and representation checks at world entrances.

use super::*;

fn callees(program: &IrProgram, function: &IrFunction) -> Result<Vec<String>, BackendFailure> {
    let mut names = Vec::new();
    for instruction in function.blocks().iter().flat_map(IrBlock::instructions) {
        if let IrInstruction::Define {
            operation:
                IrOperation::Call { function, .. }
                | IrOperation::ContextStart { function, .. }
                | IrOperation::ContextStartBound { function, .. },
            ..
        } = instruction
        {
            names.push(
                program
                    .functions()
                    .get(*function as usize)
                    .ok_or(BackendFailure::InvalidIr)?
                    .name()
                    .to_owned(),
            );
        }
    }
    Ok(names)
}

/// Route captured-ABI fallback calls into the same canonical source closure.
/// Only names bind worlds: optional synthesis appends or prunes ordinals.
pub(super) fn canonical_clone_roots(
    program: &IrProgram,
    canonical: &IrProgram,
    clones: &mut HashSet<u32>,
) -> Result<(BTreeSet<String>, BTreeSet<String>), BackendFailure> {
    let mut pending = Vec::new();
    for &ordinal in clones.iter() {
        let function = program
            .functions()
            .get(ordinal as usize)
            .ok_or(BackendFailure::InvalidIr)?;
        pending.push(function.name().to_owned());
        if function.synthesis().is_some() {
            pending.extend(callees(program, function)?);
        }
    }
    // These functions can receive values produced with the parallel world's
    // representation. Their checked signatures must agree across the ordinary
    // lowering and the extracted graph; handler identities are bound by the
    // shared dispatch plan before either world is emitted.
    let crossovers = pending
        .iter()
        .filter(|name| {
            canonical
                .functions()
                .iter()
                .any(|f| f.name() == name.as_str() && !f.blocks().is_empty())
        })
        .cloned()
        .collect();
    let mut roots = BTreeSet::new();
    while let Some(name) = pending.pop() {
        let Some(function) = canonical.functions().iter().find(|f| f.name() == name) else {
            continue;
        };
        if function.blocks().is_empty() || !roots.insert(source_symbol(&name)) {
            continue;
        }
        if let Some(ordinal) = program.functions().iter().position(|f| f.name() == name) {
            clones.insert(u32::try_from(ordinal).map_err(|_| BackendFailure::CounterOverflow)?);
        }
        pending.extend(callees(canonical, function)?);
    }
    Ok((roots, crossovers))
}

/// Opaque pointer arguments still carry typed storage. Compare every reachable
/// representation, including owner payloads, before importing a canonical root.
pub(crate) fn validate_crossovers(
    program: &IrProgram,
    canonical: &IrProgram,
    roots: &BTreeSet<String>,
) -> Result<(), BackendFailure> {
    let mut pending = Vec::new();
    for name in roots {
        let parallel = program
            .functions()
            .iter()
            .find(|f| f.name() == name)
            .ok_or(BackendFailure::InvalidIr)?;
        let sequential = canonical
            .functions()
            .iter()
            .find(|f| f.name() == name)
            .ok_or(BackendFailure::InvalidIr)?;
        if parallel.result() != sequential.result()
            || !parallel
                .parameters()
                .iter()
                .map(|(_, ty)| ty)
                .eq(sequential.parameters().iter().map(|(_, ty)| ty))
        {
            return Err(BackendFailure::InvalidIr);
        }
        pending.push(parallel.result());
        pending.extend(parallel.parameters().iter().map(|(_, ty)| *ty));
    }
    let mut visited = HashSet::new();
    while let Some(ty) = pending.pop() {
        if !visited.insert(ty) {
            continue;
        }
        match ty {
            IrType::Nominal(id) | IrType::RuntimeBoxPayload { nominal: id } => {
                let parallel = program.nominal(id).ok_or(BackendFailure::InvalidIr)?;
                let sequential = canonical.nominal(id).ok_or(BackendFailure::InvalidIr)?;
                if parallel.link_name() != sequential.link_name()
                    || parallel.kind() != sequential.kind()
                    || parallel.handler_words != sequential.handler_words
                {
                    return Err(BackendFailure::InvalidIr);
                }
                match parallel.kind() {
                    IrNominalKind::Struct { fields } => {
                        pending.extend(fields.iter().map(|f| f.ty()))
                    }
                    IrNominalKind::Enum { variants } => {
                        pending.extend(variants.iter().flat_map(|v| v.fields()).map(|f| f.ty()))
                    }
                    IrNominalKind::Box { referent, .. } => pending.push(*referent),
                    IrNominalKind::Shared { state, shape } => {
                        pending.push(*state);
                        if let IrShared::Map { entry } = shape {
                            pending.push(*entry);
                        }
                    }
                    IrNominalKind::Opaque => {}
                }
            }
            IrType::Address(referent) => pending.push(referent.ty()),
            IrType::Array { element, .. }
            | IrType::Window { element, .. }
            | IrType::Buffer { element }
            | IrType::Segments { element }
            | IrType::Range { element }
            | IrType::Run { element }
            | IrType::Entries { element } => {
                let parallel = program.element(element).ok_or(BackendFailure::InvalidIr)?;
                let sequential = canonical
                    .element(element)
                    .ok_or(BackendFailure::InvalidIr)?;
                if parallel != sequential {
                    return Err(BackendFailure::InvalidIr);
                }
                pending.push(parallel);
            }
            IrType::Unit
            | IrType::Bool
            | IrType::Integer { .. }
            | IrType::Float { .. }
            | IrType::KeySet => {}
        }
    }
    Ok(())
}
