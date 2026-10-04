//! A loop whose header ends in a `match` over a nominal enum, emitted as one
//! function per arm (compiler/match-dispatch-lowering).
//!
//! The header becomes an always-inline dispatch function that computes the
//! header's values and transfers, through a handler table indexed by the
//! tag, to the function of the selected arm; every edge back to the header
//! becomes a guaranteed tail call of the dispatch function, so each arm ends
//! in its own indirect transfer. All parts take one parameter list — the
//! header's parameters, the header values the arms read, the values from
//! before the loop that it reads, and the function's frame — because a
//! guaranteed tail call needs identical prototypes. The frame stays in the
//! enclosing function's activation, which calls the dispatch function once
//! and returns its result.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt::Write;

use super::{
    BackendFailure, FunctionEmitter, FunctionFramePlan, FunctionSlot, RESULT_POINTER, block_label,
    llvm_type_with_references, value_name,
};
use crate::backend::emission::{FunctionBody, Linkage, Module, Parameter, References, Signature};
use crate::{
    IrBlock, IrBlockId, IrDrop, IrEnumType, IrFunction, IrInstruction, IrTerminator, IrType,
    IrValueId,
};

/// The calling convention of every part, with its argument registers on the
/// target: the convention without callee-saved registers where the build's
/// assembler accepts it, so the values the parts pass stay in registers
/// across the chain, and the C convention otherwise. The register counts
/// are those measured for each convention and target
/// (compiler/match-dispatch-lowering); a loop whose parts would need more is
/// not split.
fn convention(triple: &str) -> (&'static str, ArgumentRegisters) {
    let aarch64 = triple.starts_with("aarch64");
    let windows = triple.contains("windows");
    if env!("WHITEFOOT_PRESERVE_NONE") == "1" {
        let integer = if aarch64 { 24 } else { 12 };
        (
            "preserve_nonecc ",
            ArgumentRegisters::Separate { integer, float: 8 },
        )
    } else if windows {
        ("", ArgumentRegisters::Shared(4))
    } else {
        let integer = if aarch64 { 8 } else { 6 };
        ("", ArgumentRegisters::Separate { integer, float: 8 })
    }
}

/// How many arguments a convention passes in registers.
#[derive(Clone, Copy)]
enum ArgumentRegisters {
    /// Separate integer and floating-point register sequences.
    Separate { integer: usize, float: usize },
    /// One sequence of positions shared by both kinds (Windows x64).
    Shared(usize),
}

impl ArgumentRegisters {
    /// Whether parameters of these LLVM types all arrive in registers; a
    /// type other than a pointer, an integer, a float or a range's
    /// `{ ptr, i64 }` pair counts as not fitting.
    fn admit(self, types: &[String]) -> bool {
        let (mut integer, mut float) = (0_usize, 0_usize);
        for ty in types {
            match ty.as_str() {
                "ptr" => integer += 1,
                "float" | "double" => float += 1,
                "{ ptr, i64 }" => integer += 2,
                other
                    if other.strip_prefix('i').is_some_and(|width| {
                        !width.is_empty() && width.bytes().all(|byte| byte.is_ascii_digit())
                    }) =>
                {
                    integer += 1;
                }
                _ => return false,
            }
        }
        match self {
            Self::Separate {
                integer: integers,
                float: floats,
            } => integer <= integers && float <= floats,
            Self::Shared(positions) => integer + float <= positions,
        }
    }
}

/// One recognised dispatch loop of a function.
pub(super) struct DispatchLoop {
    pub(super) header: IrBlockId,
    /// The loop's blocks other than the header.
    pub(super) region: Vec<bool>,
    /// One arm per distinct match target, in the order of the header's
    /// targets, with the blocks its function contains.
    pub(super) arms: Vec<(IrBlockId, Vec<bool>)>,
    /// The handler table: for every tag, the index of its arm.
    pub(super) table: Vec<usize>,
    /// Values the header defines and the arms read.
    pub(super) header_values: Vec<IrValueId>,
    /// Values defined before the loop and read inside it.
    pub(super) invariants: Vec<IrValueId>,
}

fn successors(block: &IrBlock) -> Vec<usize> {
    match block.terminator() {
        IrTerminator::Jump { target, .. } => vec![target.index()],
        IrTerminator::Match { targets, .. } => targets
            .iter()
            .map(|target| target.block().index())
            .collect(),
        IrTerminator::Return { .. } | IrTerminator::Unreachable => Vec::new(),
    }
}

/// The blocks reachable from `starts` without entering `header`.
fn reach(function: &IrFunction, starts: &[usize], header: usize) -> Vec<bool> {
    let mut seen = vec![false; function.blocks().len()];
    let mut pending = starts.to_vec();
    while let Some(index) = pending.pop() {
        if index == header || seen[index] {
            continue;
        }
        seen[index] = true;
        pending.extend(successors(&function.blocks()[index]));
    }
    seen
}

fn defined_values(block: &IrBlock, into: &mut HashSet<IrValueId>) {
    into.extend(block.parameters().iter().map(|(value, _)| *value));
    for instruction in block.instructions() {
        if let IrInstruction::Define { result, .. } = instruction {
            into.insert(*result);
        }
    }
}

fn used_values(block: &IrBlock, into: &mut BTreeSet<IrValueId>) {
    for instruction in block.instructions() {
        into.extend(instruction.operands());
    }
    into.extend(block.terminator().operands());
}

/// The first block, in block order, that heads a dispatch loop: it ends in
/// a `match` over a nominal enum with at least two targets, none of which
/// takes parameters; the blocks reachable from its targets without passing
/// through it are entered only from it and leave only by returning or by
/// jumping back to it; and at least one of them jumps back.
pub(super) fn find(function: &IrFunction, reachable: &[bool]) -> Option<DispatchLoop> {
    let blocks = function.blocks();
    for (header, block) in blocks.iter().enumerate() {
        if header == 0 || !reachable[header] {
            continue;
        }
        let IrTerminator::Match {
            enum_type: IrEnumType::Nominal(_),
            targets,
            ..
        } = block.terminator()
        else {
            continue;
        };
        if targets.len() < 2
            || targets
                .iter()
                .any(|target| !blocks[target.block().index()].parameters().is_empty())
        {
            continue;
        }
        let starts: Vec<usize> = targets
            .iter()
            .map(|target| target.block().index())
            .collect();
        let region = reach(function, &starts, header);
        if region[0] {
            continue;
        }
        let mut closed = true;
        let mut backedge = false;
        for (index, candidate) in blocks.iter().enumerate() {
            if !reachable[index] {
                continue;
            }
            for successor in successors(candidate) {
                if region[index] {
                    // An edge back to the header must be a jump, which the
                    // emitter turns into a tail call.
                    let jump = matches!(candidate.terminator(), IrTerminator::Jump { .. });
                    closed &= region[successor] || (successor == header && jump);
                    backedge |= successor == header;
                } else if index != header {
                    closed &= !region[successor];
                }
            }
        }
        if !closed || !backedge {
            continue;
        }
        let mut arms: Vec<(IrBlockId, Vec<bool>)> = Vec::new();
        let max_tag = targets.iter().map(|target| target.tag()).max()?;
        let mut table = vec![usize::MAX; max_tag as usize + 1];
        for target in targets {
            let arm = match arms.iter().position(|(block, _)| *block == target.block()) {
                Some(arm) => arm,
                None => {
                    let blocks = reach(function, &[target.block().index()], header);
                    arms.push((target.block(), blocks));
                    arms.len() - 1
                }
            };
            table[target.tag() as usize] = arm;
        }
        if table.contains(&usize::MAX) {
            continue;
        }
        let mut loop_defined = HashSet::new();
        let mut header_defined = HashSet::new();
        defined_values(block, &mut header_defined);
        loop_defined.extend(header_defined.iter().copied());
        let mut loop_used = BTreeSet::new();
        let mut region_used = BTreeSet::new();
        used_values(block, &mut loop_used);
        for (index, member) in blocks.iter().enumerate() {
            if region[index] {
                defined_values(member, &mut loop_defined);
                used_values(member, &mut region_used);
            }
        }
        loop_used.extend(region_used.iter().copied());
        let header_parameters: HashSet<IrValueId> =
            block.parameters().iter().map(|(value, _)| *value).collect();
        let header_values = region_used
            .iter()
            .copied()
            .filter(|value| header_defined.contains(value) && !header_parameters.contains(value))
            .collect();
        let invariants = loop_used
            .iter()
            .copied()
            .filter(|value| !loop_defined.contains(value))
            .collect();
        return Some(DispatchLoop {
            header: IrBlockId::from_index(header)?,
            region,
            arms,
            table,
            header_values,
            invariants,
        });
    }
    None
}

/// Which part of a split function is being emitted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Part {
    /// The enclosing function: everything before the loop.
    Enclosing,
    /// The dispatch function: the header and its table transfer.
    Header,
    /// One arm's function.
    Arm,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Role {
    /// A header parameter.
    Carried,
    /// A value the header defines and the arms read.
    HeaderValue,
    /// A value from before the loop.
    Invariant,
}

/// The emission state of one split function.
pub(super) struct DispatchEmission {
    pub(super) plan: DispatchLoop,
    pub(super) part: Part,
    symbol: String,
    arm_symbols: Vec<String>,
    table_symbol: String,
    parameters: Vec<(IrValueId, IrType, Role)>,
    frame: bool,
    destination: bool,
    result: String,
    convention: &'static str,
}

impl FunctionFramePlan {
    /// The frame's pointers for one function of a split: the frame itself
    /// is allocated only by the enclosing function and reached by the parts
    /// through the frame pointer they receive, while a slot in `locals` is
    /// the part's own allocation. An empty frame renders nothing.
    fn render_split(
        &self,
        program: &crate::IrProgram,
        references: &mut References,
        enclosing: bool,
        locals: &HashSet<FunctionSlot>,
    ) -> Result<String, BackendFailure> {
        if self.target.is_empty() {
            return Ok(String::new());
        }
        let fields = self
            .target
            .physical_fields()
            .iter()
            .map(|field| {
                super::llvm_storage_type_with_references(program, field, &mut references.types)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let frame_type = format!("{{ {} }}", fields.join(", "));
        let align = self.target.layout().align();
        let mut text = String::new();
        if enclosing {
            writeln!(text, "  %wf.frame = alloca {frame_type}, align {align}")
                .map_err(|_| BackendFailure::TextEmission)?;
        }
        for key in &self.ordered {
            let slot = self.slots.get(key).ok_or(BackendFailure::InvalidIr)?;
            let field = self
                .target
                .logical_field(slot.logical_index)
                .ok_or(BackendFailure::InvalidIr)?;
            if locals.contains(key) {
                let ty = fields
                    .get(field.physical_index() as usize)
                    .ok_or(BackendFailure::InvalidIr)?;
                writeln!(text, "  {} = alloca {ty}, align {align}", slot.pointer)
            } else {
                writeln!(
                    text,
                    "  {} = getelementptr inbounds {frame_type}, ptr %wf.frame, i32 0, i32 {}",
                    slot.pointer,
                    field.physical_index()
                )
            }
            .map_err(|_| BackendFailure::TextEmission)?;
        }
        Ok(text)
    }
}

impl FunctionEmitter<'_, '_> {
    /// Recognises this function's dispatch loop and prepares its split, or
    /// returns `None` to emit the function whole.
    pub(super) fn plan_dispatch(
        &mut self,
        reachable: &[bool],
        body_symbol: &str,
        destination: bool,
        result: &str,
    ) -> Result<Option<DispatchEmission>, BackendFailure> {
        if self.function.waits()
            || self.grain.is_some()
            || self.sequential_clones.is_some()
            || !self.function.overlaps().is_empty()
            || self.function.synthesis().is_some()
            || !super::contexts::context_group_prelude(self.function).is_empty()
            || !super::shared::record_prelude(self.function).is_empty()
        {
            return Ok(None);
        }
        let Some(plan) = find(self.function, reachable) else {
            return Ok(None);
        };
        let header = self.block(plan.header)?;
        let mut parameters = Vec::new();
        for (value, ty) in header.parameters() {
            if self.storage.slot(*value).is_none() {
                parameters.push((*value, *ty, Role::Carried));
            }
        }
        for (values, role) in [
            (&plan.header_values, Role::HeaderValue),
            (&plan.invariants, Role::Invariant),
        ] {
            for value in values {
                // A frame address is derived again in every part.
                if self.storage.slot(*value).is_none()
                    && !self
                        .frame
                        .slots
                        .contains_key(&FunctionSlot::Address(*value))
                {
                    let ty = self.value_type(*value).ok_or(BackendFailure::InvalidIr)?;
                    parameters.push((*value, ty, role));
                }
            }
        }
        let (convention, registers) = convention(self.target.triple());
        let mut types = Vec::new();
        let mut scratch = References::default();
        if destination {
            types.push("ptr".to_owned());
        }
        for (_, ty, _) in &parameters {
            types.push(llvm_type_with_references(
                self.program,
                *ty,
                &mut scratch.types,
            )?);
        }
        if !self.frame.target.is_empty() {
            types.push("ptr".to_owned());
        }
        if !registers.admit(&types) {
            return Ok(None);
        }
        let enclosing = self.frame.render_split(
            self.program,
            &mut self.output.references,
            true,
            &HashSet::new(),
        )?;
        let frame = !enclosing.is_empty();
        self.entry_prelude = enclosing;
        self.slot_uses.borrow_mut().clear();
        let arm_symbols = (0..plan.arms.len())
            .map(|arm| format!("{body_symbol}.arm.{arm}"))
            .collect();
        Ok(Some(DispatchEmission {
            plan,
            part: Part::Enclosing,
            symbol: format!("{body_symbol}.dispatch"),
            arm_symbols,
            table_symbol: format!("{body_symbol}.dispatch.table"),
            parameters,
            frame,
            destination,
            result: result.to_owned(),
            convention,
        }))
    }

    /// The blocks the enclosing function keeps.
    pub(super) fn enclosing_blocks(&self, reachable: &[bool]) -> Vec<bool> {
        let Some(dispatch) = &self.dispatch else {
            return reachable.to_vec();
        };
        reachable
            .iter()
            .enumerate()
            .map(|(index, &live)| {
                live && !dispatch.plan.region[index] && index != dispatch.plan.header.index()
            })
            .collect()
    }

    /// The argument list of a transfer: `carried` supplies the header
    /// parameters; header values are passed by name only into an arm.
    fn dispatch_arguments(
        &mut self,
        carried: &[(IrValueId, IrValueId)],
        into_arm: bool,
    ) -> Result<String, BackendFailure> {
        let dispatch = self.dispatch.as_ref().ok_or(BackendFailure::InvalidIr)?;
        let parameters = dispatch.parameters.clone();
        let (destination, frame) = (dispatch.destination, dispatch.frame);
        let mut arguments = Vec::new();
        if destination {
            arguments.push(format!("ptr {RESULT_POINTER}"));
        }
        for (value, ty, role) in parameters {
            let ty_name = self.output.type_name(self.program, ty)?;
            let operand = match role {
                Role::Carried => {
                    let argument = carried
                        .iter()
                        .find(|(parameter, _)| *parameter == value)
                        .map(|(_, argument)| *argument)
                        .ok_or(BackendFailure::InvalidIr)?;
                    self.value_name(argument)
                }
                Role::HeaderValue if !into_arm => "poison".to_owned(),
                Role::HeaderValue | Role::Invariant => self.value_name(value),
            };
            arguments.push(format!("{ty_name} {operand}"));
        }
        if frame {
            arguments.push("ptr %wf.frame".to_owned());
        }
        Ok(arguments.join(", "))
    }

    /// Emits the transfer an edge into the dispatch header becomes: the call
    /// of the dispatch function and the return of its result from the
    /// enclosing function, or a guaranteed tail call of it from an arm.
    /// Returns whether the edge was such a transfer.
    pub(super) fn emit_dispatch_transfer(
        &mut self,
        target: IrBlockId,
        arguments: &[IrValueId],
        drops: &[IrDrop],
    ) -> Result<bool, BackendFailure> {
        let Some(dispatch) = &self.dispatch else {
            return Ok(false);
        };
        if target != dispatch.plan.header {
            return Ok(false);
        }
        let tail = dispatch.part != Part::Enclosing;
        let symbol = dispatch.symbol.clone();
        let result = dispatch.result.clone();
        let convention = dispatch.convention;
        let header = self.block(target)?;
        if header.parameters().len() != arguments.len()
            || arguments
                .iter()
                .zip(header.parameters())
                .any(|(argument, (_, ty))| self.value_type(*argument) != Some(*ty))
        {
            return Err(BackendFailure::InvalidIr);
        }
        let carried: Vec<(IrValueId, IrValueId)> = header
            .parameters()
            .iter()
            .map(|(parameter, _)| *parameter)
            .zip(arguments.iter().copied())
            .collect();
        self.emit_place_edge(target, arguments, drops)?;
        let list = self.dispatch_arguments(&carried, false)?;
        self.output.symbol(symbol.clone());
        let call = if tail { "musttail call" } else { "call" };
        let text = if result == "void" {
            format!("  {call} {convention}void @{symbol}({list})\n  ret void\n")
        } else {
            let temporary = self.next_temporary()?;
            format!(
                "  %{temporary} = {call} {convention}{result} @{symbol}({list})\n  ret {result} %{temporary}\n"
            )
        };
        self.output.push_str(&text);
        Ok(true)
    }

    /// Emits the dispatch header's terminator in the dispatch function: the
    /// handler table load and the guaranteed tail call of the selected arm.
    /// Returns whether `block` was that header.
    pub(super) fn emit_dispatch_select(
        &mut self,
        block: IrBlockId,
        scrutinee: IrValueId,
        enum_type: IrEnumType,
    ) -> Result<bool, BackendFailure> {
        let Some(dispatch) = &self.dispatch else {
            return Ok(false);
        };
        if dispatch.part != Part::Header || block != dispatch.plan.header {
            return Ok(false);
        }
        let table = dispatch.table_symbol.clone();
        let entries = dispatch.plan.table.len();
        let result = dispatch.result.clone();
        let convention = dispatch.convention;
        self.materialize_operands([scrutinee])?;
        let (tag, tag_ty) = self.match_tag(scrutinee, enum_type)?;
        let carried: Vec<(IrValueId, IrValueId)> = self
            .block(block)?
            .parameters()
            .iter()
            .map(|(parameter, _)| (*parameter, *parameter))
            .collect();
        let list = self.dispatch_arguments(&carried, true)?;
        let slot = self.next_temporary()?;
        let handler = self.next_temporary()?;
        self.output.symbol(table.clone());
        // A tag narrower than the index is zero-extended: a two-variant
        // tag-only enum's `i1` tag would otherwise index as -1.
        let index = self.next_temporary()?;
        let mut text = if tag_ty == "i64" {
            format!("  %{index} = add i64 {tag}, 0\n")
        } else {
            format!("  %{index} = zext {tag_ty} {tag} to i64\n")
        };
        write!(
            text,
            "  %{slot} = getelementptr inbounds [{entries} x ptr], ptr @{table}, i64 0, i64 %{index}\n  %{handler} = load ptr, ptr %{slot}\n"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        if result == "void" {
            text.push_str(&format!(
                "  musttail call {convention}void %{handler}({list})\n  ret void\n"
            ));
        } else {
            let returned = self.next_temporary()?;
            text.push_str(&format!(
                "  %{returned} = musttail call {convention}{result} %{handler}({list})\n  ret {result} %{returned}\n"
            ));
        }
        self.output.push_str(&text);
        Ok(true)
    }

    /// The signature every part shares; the dispatch function names the
    /// header values it recomputes apart from its own definitions of them.
    fn part_signature(&self, symbol: &str, part: Part) -> Result<Signature, BackendFailure> {
        let dispatch = self.dispatch.as_ref().ok_or(BackendFailure::InvalidIr)?;
        let mut references = References::default();
        let mut parameters = Vec::new();
        if dispatch.destination {
            parameters.push(Parameter::named("ptr", RESULT_POINTER));
        }
        for (value, ty, role) in &dispatch.parameters {
            let ty = llvm_type_with_references(self.program, *ty, &mut references.types)?;
            let name = if part == Part::Header && *role == Role::HeaderValue {
                format!("%wf.dispatch.v{}", value.ordinal())
            } else {
                value_name(*value)
            };
            parameters.push(Parameter::named(ty, name));
        }
        if dispatch.frame {
            parameters.push(Parameter::named("ptr", "%wf.frame"));
        }
        let mut signature = Signature::new(symbol, dispatch.result.clone(), parameters);
        signature.linkage = Linkage::Internal;
        signature.convention = dispatch.convention;
        signature.references = references;
        if part == Part::Header {
            signature.suffix.push_str(" alwaysinline");
        }
        Ok(signature)
    }

    /// Emits the dispatch function, every arm's function and the handler
    /// table after the enclosing function. A frame slot that the enclosing
    /// function never uses and exactly one part uses is that part's own
    /// allocation, which the host can then remove; every other slot is
    /// reached through the shared frame.
    pub(super) fn emit_dispatch_parts(
        &mut self,
        module: &mut Module,
    ) -> Result<(), BackendFailure> {
        let enclosing_uses = std::mem::take(&mut *self.slot_uses.borrow_mut());
        let dispatch = self.dispatch.as_ref().ok_or(BackendFailure::InvalidIr)?;
        let header = dispatch.plan.header;
        let symbol = dispatch.symbol.clone();
        let arm_symbols = dispatch.arm_symbols.clone();
        let arms: Vec<(IrBlockId, Vec<bool>)> = dispatch.plan.arms.clone();
        let table = dispatch.plan.table.clone();
        let table_symbol = dispatch.table_symbol.clone();
        let mut parts: Vec<(Signature, FunctionBody, HashSet<FunctionSlot>)> = Vec::new();

        self.set_part(Part::Header);
        self.output = FunctionBody::default();
        self.output.open_block("entry".to_owned());
        writeln!(self.output, "  br label %{}", block_label(header))
            .map_err(|_| BackendFailure::TextEmission)?;
        self.materialized.clear();
        self.output.open_block(block_label(header));
        let block = self.block(header)?.clone();
        for (index, instruction) in block.instructions().iter().enumerate() {
            self.emit_instruction(header, index, instruction)?;
        }
        self.emit_terminator(header, block.terminator())?;
        self.output.finish_ir_block(header)?;
        parts.push((
            self.part_signature(&symbol, Part::Header)?,
            std::mem::take(&mut self.output),
            std::mem::take(&mut *self.slot_uses.borrow_mut()),
        ));

        self.set_part(Part::Arm);
        for ((target, blocks), arm_symbol) in arms.iter().zip(&arm_symbols) {
            self.output = FunctionBody::default();
            self.incoming = self.collect_incoming(blocks)?;
            self.output.open_block("entry".to_owned());
            writeln!(self.output, "  br label %{}", block_label(*target))
                .map_err(|_| BackendFailure::TextEmission)?;
            for (index, member) in self.function.blocks().iter().enumerate() {
                if !blocks[index] {
                    continue;
                }
                self.materialized.clear();
                let block_id =
                    IrBlockId::from_index(index).ok_or(BackendFailure::CounterOverflow)?;
                self.output.open_block(block_label(block_id));
                self.emit_block_parameters(block_id, member)?;
                for (instruction_index, instruction) in member.instructions().iter().enumerate() {
                    self.emit_instruction(block_id, instruction_index, instruction)?;
                }
                self.emit_terminator(block_id, member.terminator())?;
                self.output.finish_ir_block(block_id)?;
            }
            parts.push((
                self.part_signature(arm_symbol, Part::Arm)?,
                std::mem::take(&mut self.output),
                std::mem::take(&mut *self.slot_uses.borrow_mut()),
            ));
        }

        let mut users: HashMap<FunctionSlot, usize> = HashMap::new();
        for (_, _, uses) in &parts {
            for key in uses {
                *users.entry(*key).or_default() += 1;
            }
        }
        for (signature, mut body, uses) in parts {
            let locals: HashSet<FunctionSlot> = uses
                .iter()
                .copied()
                .filter(|key| {
                    !enclosing_uses.contains(key)
                        && users.get(key) == Some(&1)
                        && !matches!(key, FunctionSlot::Address(_))
                })
                .collect();
            let prelude =
                self.frame
                    .render_split(self.program, &mut body.references, false, &locals)?;
            module.define(signature.define(body, &prelude)?);
            module.text("\n");
        }

        let mut references = References::default();
        let entries = table
            .iter()
            .map(|arm| {
                let name = &arm_symbols[*arm];
                references.symbols.insert(name.clone());
                format!("ptr @{name}")
            })
            .collect::<Vec<_>>()
            .join(", ");
        module.global(
            table_symbol,
            "unnamed_addr constant",
            format!("[{} x ptr]", table.len()),
            format!("[{entries}]"),
            None,
            references,
        );
        Ok(())
    }

    fn set_part(&mut self, part: Part) {
        if let Some(dispatch) = &mut self.dispatch {
            dispatch.part = part;
        }
    }
}
