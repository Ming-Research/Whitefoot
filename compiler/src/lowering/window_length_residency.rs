//! Keep one scalar window's logical length local to a counted append region.
//! Private acyclic helper versions use a borrowed cache; ordinary calls publish
//! and reload it. Selection is over typed places and operations, never names.

use std::collections::{HashMap, HashSet};

use crate::ir::*;
use crate::target::TargetLayout;

const U64: IrType = IrType::Integer {
    width: 64,
    signed: false,
};
const LENGTH: IrAddressed = IrAddressed::Integer {
    width: 64,
    signed: false,
};

#[derive(Clone, Debug, Eq, PartialEq)]
struct Path {
    root: IrValueId,
    steps: Vec<IrPlaceStep>,
}

impl Path {
    fn includes(&self, other: &Self) -> bool {
        self.root == other.root && other.steps.starts_with(&self.steps)
    }
}

fn scalar(ty: IrType) -> bool {
    matches!(
        ty,
        IrType::Unit | IrType::Bool | IrType::Integer { .. } | IrType::Float { .. }
    )
}

fn successors(terminator: &IrTerminator) -> Vec<IrBlockId> {
    match terminator {
        IrTerminator::Jump { target, .. } => vec![*target],
        IrTerminator::Match { targets, .. } => targets.iter().map(|target| target.block).collect(),
        IrTerminator::Return { .. } | IrTerminator::Unreachable => Vec::new(),
    }
}

fn no_drops(function: &IrFunction) -> bool {
    function.blocks.iter().all(|block| {
        block.instructions.iter().all(
            |instruction| !matches!(instruction, IrInstruction::Drops(drops) if !drops.is_empty()),
        ) && match &block.terminator {
            IrTerminator::Jump { drops, .. } | IrTerminator::Return { drops, .. } => {
                drops.is_empty()
            }
            _ => true,
        }
    })
}

/// All incoming definitions of a forwarded address must resolve to the same
/// original place. Unresolved or differing paths do not acquire an alias fact.
fn paths(function: &IrFunction) -> HashMap<IrValueId, Path> {
    let mut result = function
        .parameters
        .iter()
        .filter_map(|(id, ty)| {
            matches!(ty, IrType::Address(_)).then_some((
                *id,
                Path {
                    root: *id,
                    steps: Vec::new(),
                },
            ))
        })
        .collect::<HashMap<_, _>>();
    loop {
        let before = result.len();
        for block in &function.blocks {
            for instruction in &block.instructions {
                let IrInstruction::Define {
                    result: value,
                    operation:
                        IrOperation::ProjectAddress {
                            address,
                            projection,
                        },
                    ..
                } = instruction
                else {
                    continue;
                };
                if let Some(mut path) = result.get(address).cloned() {
                    path.steps.push(projection.clone());
                    result.entry(*value).or_insert(path);
                }
            }
        }
        for (index, block) in function.blocks.iter().enumerate() {
            let incoming = function
                .blocks
                .iter()
                .filter_map(|predecessor| match &predecessor.terminator {
                    IrTerminator::Jump {
                        target, arguments, ..
                    } if target.index() == index => Some(arguments),
                    _ => None,
                })
                .collect::<Vec<_>>();
            if incoming.is_empty() {
                continue;
            }
            for (slot, (value, ty)) in block.parameters.iter().enumerate() {
                if !matches!(ty, IrType::Address(_)) || result.contains_key(value) {
                    continue;
                }
                // Backedge arguments frequently forward this very parameter.
                // Other unresolved inputs must be proved before it is filed.
                let mut path = None;
                let mut complete = true;
                for arguments in &incoming {
                    let Some(argument) = arguments.get(slot) else {
                        complete = false;
                        break;
                    };
                    if argument == value {
                        continue;
                    }
                    let Some(next) = result.get(argument) else {
                        continue;
                    };
                    if path.as_ref().is_some_and(|previous| previous != next) {
                        complete = false;
                        break;
                    }
                    path = Some(next.clone());
                }
                if complete && let Some(path) = path {
                    result.insert(*value, path);
                }
            }
        }
        if result.len() == before {
            break;
        }
    }
    // Speculative propagation closes cyclic forwarding. Every incoming edge
    // must then agree; an unresolved or different leaf removes its dependent
    // path and is never used as authority for selection.
    loop {
        let before = result.len();
        for (index, block) in function.blocks.iter().enumerate() {
            for (slot, (value, _)) in block.parameters.iter().enumerate() {
                let Some(path) = result.get(value).cloned() else {
                    continue;
                };
                if function
                    .blocks
                    .iter()
                    .any(|predecessor| match &predecessor.terminator {
                        IrTerminator::Jump {
                            target, arguments, ..
                        } if target.index() == index => arguments
                            .get(slot)
                            .is_none_or(|argument| result.get(argument) != Some(&path)),
                        _ => false,
                    })
                {
                    result.remove(value);
                }
            }
            for instruction in &block.instructions {
                if let IrInstruction::Define {
                    result: value,
                    operation: IrOperation::ProjectAddress { address, .. },
                    ..
                } = instruction
                    && !result.contains_key(address)
                {
                    result.remove(value);
                }
            }
        }
        if result.len() == before {
            break;
        }
    }
    result
}

fn acyclic(function: &IrFunction) -> bool {
    fn visit(
        function: &IrFunction,
        block: IrBlockId,
        active: &mut HashSet<IrBlockId>,
        done: &mut HashSet<IrBlockId>,
    ) -> bool {
        if done.contains(&block) {
            return true;
        }
        if !active.insert(block) {
            return false;
        }
        let Some(body) = function.blocks.get(block.index()) else {
            return false;
        };
        if successors(&body.terminator)
            .into_iter()
            .any(|next| !visit(function, next, active, done))
        {
            return false;
        }
        active.remove(&block);
        done.insert(block);
        true
    }
    function.blocks.iter().enumerate().all(|(index, _)| {
        IrBlockId::from_index(index)
            .is_some_and(|block| visit(function, block, &mut HashSet::new(), &mut HashSet::new()))
    })
}

fn projected(program: &IrProgram, base: IrType, step: &IrPlaceStep) -> Option<IrType> {
    let IrType::Address(IrAddressed::Nominal(actual)) = base else {
        return None;
    };
    let referent = match step {
        IrPlaceStep::Field { nominal, field } if *nominal == actual => {
            let IrNominalKind::Struct { fields } = program.nominal(actual)?.kind() else {
                return None;
            };
            fields.get(*field as usize)?.ty()
        }
        IrPlaceStep::BoxReferent { nominal } if *nominal == actual => {
            let IrNominalKind::Box { referent, .. } = program.nominal(actual)?.kind() else {
                return None;
            };
            // Following an outer Box would cache an address inside an allocation
            // an ordinary call can replace. Runtime content retains its slot.
            if !matches!(referent, IrType::Window { capacity: None, .. }) {
                return None;
            }
            *referent
        }
        _ => return None,
    };
    Some(IrType::Address(IrAddressed::of(referent)?))
}

fn window(program: &IrProgram, function: &IrFunction, path: &Path) -> Option<IrAddressed> {
    let mut ty = function.value_type(path.root)?;
    for step in &path.steps {
        ty = projected(program, ty, step)?;
    }
    let IrType::Address(
        addressed @ IrAddressed::Window {
            shape: IrWindowShape::Slots,
            element,
            ..
        },
    ) = ty
    else {
        return None;
    };
    (scalar(program.element(element)?) && program.element(element)? != IrType::Unit)
        .then_some(addressed)
}

/// Translate a callee's formal-rooted path through this call's already evaluated
/// arguments. No expression, offset or owner projection is reevaluated.
fn through_call(
    callee: &IrFunction,
    arguments: &[IrValueId],
    callee_path: &Path,
    caller_paths: &HashMap<IrValueId, Path>,
) -> Option<Path> {
    let index = callee
        .parameters
        .iter()
        .position(|(parameter, _)| *parameter == callee_path.root)?;
    let mut path = caller_paths.get(arguments.get(index)?)?.clone();
    path.steps.extend(callee_path.steps.clone());
    Some(path)
}

fn appends(program: &IrProgram, ordinal: usize, active: &mut HashSet<usize>) -> Vec<Path> {
    if !active.insert(ordinal) {
        return Vec::new();
    }
    let Some(function) = program.functions.get(ordinal) else {
        active.remove(&ordinal);
        return Vec::new();
    };
    if function.waits || function.synthesis.is_some() {
        active.remove(&ordinal);
        return Vec::new();
    }
    let addresses = paths(function);
    let mut result = Vec::new();
    for block in &function.blocks {
        for instruction in &block.instructions {
            let IrInstruction::Define { operation, .. } = instruction else {
                continue;
            };
            match operation {
                IrOperation::RunBoundary {
                    row: IrBoundary::PlaceBack,
                    run,
                    ..
                } => {
                    if let Some(path) = addresses.get(run)
                        && window(program, function, path).is_some()
                    {
                        result.push(path.clone());
                    }
                }
                IrOperation::Call {
                    function: callee,
                    arguments,
                } => {
                    if let Some(callee_function) = program.functions.get(*callee as usize) {
                        for path in appends(program, *callee as usize, active) {
                            if let Some(path) =
                                through_call(callee_function, arguments, &path, &addresses)
                            {
                                result.push(path);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    active.remove(&ordinal);
    result
}

fn pure(operation: &IrOperation) -> bool {
    matches!(
        operation,
        IrOperation::Constant(_)
            | IrOperation::Integer { .. }
            | IrOperation::Float { .. }
            | IrOperation::NumericConversion { .. }
            | IrOperation::Reinterpret { .. }
            | IrOperation::Boolean { .. }
            | IrOperation::EnumEquality { .. }
    )
}

fn scalar_function(program: &IrProgram, ordinal: usize, active: &mut HashSet<usize>) -> bool {
    if !active.insert(ordinal) {
        return false;
    }
    let answer = program.functions.get(ordinal).is_some_and(|function| {
        !function.waits
            && function.overlaps.is_empty()
            && function.values.iter().all(|ty| scalar(*ty))
            && no_drops(function)
            && function.blocks.iter().all(|block| {
                block
                    .instructions
                    .iter()
                    .all(|instruction| match instruction {
                        IrInstruction::Define {
                            operation: IrOperation::Call { function, .. },
                            ..
                        } => scalar_function(program, *function as usize, active),
                        IrInstruction::Define { operation, .. } => pure(operation),
                        IrInstruction::Drops(drops) => drops.is_empty(),
                        _ => false,
                    })
            })
    });
    active.remove(&ordinal);
    answer
}

fn value(
    function: &mut IrFunction,
    ty: IrType,
    operation: IrOperation,
    output: &mut Vec<IrInstruction>,
) -> Option<IrValueId> {
    let id = IrValueId(u32::try_from(function.values.len()).ok()?);
    function.values.push(ty);
    output.push(IrInstruction::Define {
        result: id,
        ty,
        operation,
    });
    Some(id)
}

fn load(
    function: &mut IrFunction,
    cache: IrValueId,
    output: &mut Vec<IrInstruction>,
) -> Option<IrValueId> {
    value(
        function,
        U64,
        IrOperation::Load {
            address: cache,
            referent: LENGTH,
        },
        output,
    )
}

fn commit(
    function: &mut IrFunction,
    run: IrValueId,
    cache: IrValueId,
    output: &mut Vec<IrInstruction>,
) -> Option<()> {
    let length = load(function, cache, output)?;
    value(
        function,
        IrType::Unit,
        IrOperation::RunLengthCommit { run, length },
        output,
    )?;
    Some(())
}

fn reload(
    function: &mut IrFunction,
    run: IrValueId,
    cache: IrValueId,
    output: &mut Vec<IrInstruction>,
) -> Option<()> {
    let length = value(
        function,
        U64,
        IrOperation::ContainerMeasure {
            measure: IrMeasure::Length,
            container: run,
        },
        output,
    )?;
    output.push(IrInstruction::Store {
        address: cache,
        value: length,
        referent: LENGTH,
    });
    Some(())
}

struct RewriteRegion<'a> {
    addresses: &'a HashMap<IrValueId, Path>,
    tracked: &'a Path,
    run: IrValueId,
    cache: IrValueId,
    helper: bool,
}

struct Versions<'a> {
    program: &'a IrProgram,
    helpers: Vec<IrFunction>,
    active: HashSet<usize>,
    map: Vec<(usize, Path, u32)>,
}

impl Versions<'_> {
    fn callee_path(
        &self,
        ordinal: usize,
        arguments: &[IrValueId],
        addresses: &HashMap<IrValueId, Path>,
        tracked: &Path,
    ) -> Option<Path> {
        let callee = self.program.functions.get(ordinal)?;
        let mut result = None;
        for (index, argument) in arguments.iter().enumerate() {
            let Some(path) = addresses.get(argument) else {
                continue;
            };
            if path.includes(tracked) {
                if result.is_some() {
                    return None;
                }
                result = Some(Path {
                    root: callee.parameters.get(index)?.0,
                    steps: tracked.steps[path.steps.len()..].to_vec(),
                });
            } else if tracked.includes(path) {
                // An additional element/interior reference can observe the run.
                return None;
            }
        }
        result
    }

    fn version(&mut self, ordinal: usize, tracked: Path) -> Option<u32> {
        if let Some((_, _, id)) = self
            .map
            .iter()
            .find(|(source, path, _)| *source == ordinal && *path == tracked)
        {
            return Some(*id);
        }
        let source = self.program.functions.get(ordinal)?;
        if source.waits
            || source.synthesis.is_some()
            || !source.counted_ranges.is_empty()
            || !source.overlaps.is_empty()
            || !scalar(source.result)
            || !no_drops(source)
            || !acyclic(source)
            || source.values.iter().any(|ty| {
                !scalar(*ty)
                    && !matches!(
                        ty,
                        IrType::Address(IrAddressed::Nominal(_) | IrAddressed::Window { .. })
                    )
            })
            || !self.active.insert(ordinal)
        {
            return None;
        }
        let mut function = source.clone();
        let addresses = paths(source);
        let checkpoint = (self.helpers.len(), self.map.len());
        let answer = (|| {
            let addressed = window(self.program, source, &tracked)?;
            let cache = IrValueId(u32::try_from(function.values.len()).ok()?);
            function.values.push(IrType::Address(LENGTH));
            function.parameters.push((cache, IrType::Address(LENGTH)));
            function.source_signature = None;
            function.readonly_reference_parameters.clear();
            function.synthesis = Some(IrSynthesis::ResidentWindow);
            let mut prelude = Vec::new();
            let run = self.materialize(&mut function, &tracked, addressed, &mut prelude)?;
            let region = RewriteRegion {
                addresses: &addresses,
                tracked: &tracked,
                run,
                cache,
                helper: true,
            };
            let mut touched = false;
            for index in 0..function.blocks.len() {
                let instructions = std::mem::take(&mut function.blocks[index].instructions);
                let mut output = if index == 0 {
                    std::mem::take(&mut prelude)
                } else {
                    Vec::new()
                };
                for instruction in instructions {
                    touched |= self.rewrite(&mut function, instruction, &region, &mut output)?;
                }
                function.blocks[index].instructions = output;
            }
            if !touched {
                return None;
            }
            let id = u32::try_from(
                self.program
                    .functions
                    .len()
                    .checked_add(self.helpers.len())?,
            )
            .ok()?;
            function.name = format!("{}.resident-length.{id}", function.name);
            self.helpers.push(function);
            self.map.push((ordinal, tracked, id));
            Some(id)
        })();
        self.active.remove(&ordinal);
        if answer.is_none() {
            self.helpers.truncate(checkpoint.0);
            self.map.truncate(checkpoint.1);
        }
        answer
    }

    fn materialize(
        &self,
        function: &mut IrFunction,
        path: &Path,
        addressed: IrAddressed,
        output: &mut Vec<IrInstruction>,
    ) -> Option<IrValueId> {
        let mut current = path.root;
        let mut ty = function.value_type(current)?;
        for projection in &path.steps {
            ty = projected(self.program, ty, projection)?;
            current = value(
                function,
                ty,
                IrOperation::ProjectAddress {
                    address: current,
                    projection: projection.clone(),
                },
                output,
            )?;
        }
        (ty == IrType::Address(addressed)).then_some(current)
    }

    fn rewrite(
        &mut self,
        function: &mut IrFunction,
        instruction: IrInstruction,
        region: &RewriteRegion<'_>,
        output: &mut Vec<IrInstruction>,
    ) -> Option<bool> {
        let &RewriteRegion {
            addresses,
            tracked,
            run,
            cache,
            helper,
        } = region;
        let IrInstruction::Define {
            result,
            ty,
            operation,
        } = &instruction
        else {
            return match instruction {
                IrInstruction::Drops(drops) if drops.is_empty() => Some(false),
                _ => None,
            };
        };
        let same = |id: &IrValueId| addresses.get(id) == Some(tracked);
        match operation {
            IrOperation::ContainerMeasure {
                measure: IrMeasure::Length,
                container,
            } if same(container) => {
                output.push(IrInstruction::Define {
                    result: *result,
                    ty: *ty,
                    operation: IrOperation::Load {
                        address: cache,
                        referent: LENGTH,
                    },
                });
                Some(true)
            }
            IrOperation::ContainerMeasure {
                measure: IrMeasure::Capacity,
                container,
            } if same(container) => {
                output.push(instruction);
                Some(false)
            }
            IrOperation::RunBoundary {
                row: IrBoundary::PlaceBack,
                run: owner,
                value: Some(element),
            } if same(owner) => {
                let length = load(function, cache, output)?;
                let next = value(
                    function,
                    U64,
                    IrOperation::RunBoundaryResident {
                        run: *owner,
                        value: *element,
                        length,
                    },
                    output,
                )?;
                output.push(IrInstruction::Store {
                    address: cache,
                    value: next,
                    referent: LENGTH,
                });
                output.push(IrInstruction::Define {
                    result: *result,
                    ty: *ty,
                    operation: IrOperation::Constant(IrConstant::Unit),
                });
                Some(true)
            }
            IrOperation::ProjectAddress {
                address,
                projection,
            } => {
                if !matches!(
                    projection,
                    IrPlaceStep::Field { .. } | IrPlaceStep::BoxReferent { .. }
                ) || !addresses.contains_key(address)
                {
                    return None;
                }
                // Resolve every source projection, even an unused one. Outer
                // Box contents and scalar addresses make this region unmatched.
                projected(self.program, function.value_type(*address)?, projection)?;
                output.push(instruction);
                Some(false)
            }
            IrOperation::Call {
                function: callee,
                arguments,
            } => {
                let replacement = self
                    .callee_path(*callee as usize, arguments, addresses, tracked)
                    .and_then(|path| self.version(*callee as usize, path));
                if let Some(callee) = replacement {
                    let mut arguments = arguments.clone();
                    arguments.push(cache);
                    // This is now a compiler call. Reuse metadata must not
                    // pretend the hidden argument was a checked source use.
                    function.source_calls.retain(|call| call.result != *result);
                    output.push(IrInstruction::Define {
                        result: *result,
                        ty: *ty,
                        operation: IrOperation::Call {
                            function: callee,
                            arguments,
                        },
                    });
                    Some(true)
                } else if scalar_function(self.program, *callee as usize, &mut HashSet::new()) {
                    output.push(instruction);
                    Some(false)
                } else {
                    if helper && self.active.contains(&(*callee as usize)) {
                        return None;
                    }
                    commit(function, run, cache, output)?;
                    output.push(instruction);
                    reload(function, run, cache, output)?;
                    Some(false)
                }
            }
            operation if pure(operation) => {
                output.push(instruction);
                Some(false)
            }
            _ => None,
        }
    }
}

fn candidate(program: &IrProgram, ordinal: usize) -> Option<(IrFunction, Vec<IrFunction>)> {
    let source = program.functions.get(ordinal)?;
    let [range] = source.counted_ranges.as_slice() else {
        return None;
    };
    if source.waits
        || !source.overlaps.is_empty()
        || source.synthesis.is_some()
        || !no_drops(source)
        || source.values.iter().any(|ty| {
            matches!(
                ty,
                IrType::Address(
                    IrAddressed::Integer { .. }
                        | IrAddressed::Bool
                        | IrAddressed::Float { .. }
                        | IrAddressed::Unit
                )
            )
        })
    {
        return None;
    }
    let inside =
        |block: IrBlockId| range.blocks.contains(&block.index()) && block != range.continuation;
    let addresses = paths(source);
    let mut tracked = None;
    for index in range
        .blocks
        .clone()
        .filter(|index| *index != range.continuation.index())
    {
        for instruction in &source.blocks.get(index)?.instructions {
            let IrInstruction::Define { operation, .. } = instruction else {
                continue;
            };
            let options = match operation {
                IrOperation::RunBoundary {
                    row: IrBoundary::PlaceBack,
                    run,
                    ..
                } => addresses.get(run).cloned().into_iter().collect(),
                IrOperation::Call {
                    function,
                    arguments,
                } => {
                    let callee = program.functions.get(*function as usize)?;
                    appends(program, *function as usize, &mut HashSet::new())
                        .into_iter()
                        .filter_map(|path| through_call(callee, arguments, &path, &addresses))
                        .collect::<Vec<_>>()
                }
                _ => Vec::new(),
            };
            for path in options {
                window(program, source, &path)?;
                if tracked.as_ref().is_some_and(|previous| *previous != path) {
                    return None;
                }
                tracked = Some(path);
            }
        }
    }
    let tracked = tracked?;
    // The first implementation carries no local-owned lifetime across the
    // region: the stable address must be an incoming formal.
    source
        .parameters
        .iter()
        .find(|(id, _)| *id == tracked.root)?;
    let header = IrBlockId::from_index(range.blocks.start)?;
    let entries = source
        .blocks
        .iter()
        .enumerate()
        .filter_map(|(index, block)| {
            (!inside(IrBlockId::from_index(index)?)
                && successors(&block.terminator).contains(&header))
            .then_some(index)
        })
        .collect::<Vec<_>>();
    let [entry] = entries.as_slice() else {
        return None;
    };
    if !matches!(source.blocks[*entry].terminator, IrTerminator::Jump { target, .. } if target == header)
    {
        return None;
    }
    for (index, block) in source.blocks.iter().enumerate() {
        if !inside(IrBlockId::from_index(index)?)
            && successors(&block.terminator)
                .iter()
                .any(|target| inside(*target) && *target != header)
        {
            return None;
        }
    }
    let mut function = source.clone();
    let mut versions = Versions {
        program,
        helpers: Vec::new(),
        active: HashSet::new(),
        map: Vec::new(),
    };
    let mut prelude = Vec::new();
    let addressed = window(program, source, &tracked)?;
    let run = versions.materialize(&mut function, &tracked, addressed, &mut prelude)?;
    let initial = value(
        &mut function,
        U64,
        IrOperation::ContainerMeasure {
            measure: IrMeasure::Length,
            container: run,
        },
        &mut prelude,
    )?;
    let cache = value(
        &mut function,
        IrType::Address(LENGTH),
        IrOperation::AddressOf {
            value: initial,
            referent: LENGTH,
        },
        &mut prelude,
    )?;
    function.blocks[*entry].instructions.extend(prelude);
    let region = RewriteRegion {
        addresses: &addresses,
        tracked: &tracked,
        run,
        cache,
        helper: false,
    };
    let mut touched = false;
    for index in range
        .blocks
        .clone()
        .filter(|index| *index != range.continuation.index())
    {
        let instructions = std::mem::take(&mut function.blocks[index].instructions);
        let mut output = Vec::new();
        for instruction in instructions {
            touched |= versions.rewrite(&mut function, instruction, &region, &mut output)?;
        }
        let exits = successors(&function.blocks[index].terminator)
            .iter()
            .any(|target| !inside(*target))
            || matches!(
                function.blocks[index].terminator,
                IrTerminator::Return { .. }
            );
        if exits {
            if let IrTerminator::Match { targets, .. } = &function.blocks[index].terminator {
                let mut targets = targets.clone();
                for target in &mut targets {
                    if inside(target.block) {
                        continue;
                    }
                    if !function
                        .blocks
                        .get(target.block.index())?
                        .parameters
                        .is_empty()
                    {
                        return None;
                    }
                    let mut edge = Vec::new();
                    commit(&mut function, run, cache, &mut edge)?;
                    let next = IrBlockId::from_index(function.blocks.len())?;
                    function.blocks.push(IrBlock {
                        parameters: Vec::new(),
                        instructions: edge,
                        terminator: IrTerminator::Jump {
                            target: target.block,
                            arguments: Vec::new(),
                            drops: Vec::new(),
                        },
                    });
                    target.block = next;
                }
                let IrTerminator::Match {
                    targets: actual, ..
                } = &mut function.blocks[index].terminator
                else {
                    return None;
                };
                *actual = targets;
            } else {
                commit(&mut function, run, cache, &mut output)?;
            }
        }
        function.blocks[index].instructions = output;
    }
    touched.then_some((function, versions.helpers))
}

/// Qualification is optional: an additional helper signature or cache frame
/// that does not fit this target leaves the original accepted program intact.
pub(super) fn select(program: &mut IrProgram, target: TargetLayout) {
    let originals = program.functions.len();
    for ordinal in 0..originals {
        let Some((function, helpers)) = candidate(program, ordinal) else {
            continue;
        };
        let original = std::mem::replace(&mut program.functions[ordinal], function);
        let end = program.functions.len();
        program.functions.extend(helpers);
        let fits = crate::target::validate_program(target, program).is_ok()
            && std::iter::once(ordinal)
                .chain(end..program.functions.len())
                .all(|index| {
                    crate::backend::emitter::validate_resident_frame(
                        target,
                        program,
                        &program.functions[index],
                    )
                    .is_ok()
                });
        if !fits {
            program.functions.truncate(end);
            program.functions[ordinal] = original;
        }
    }
}
