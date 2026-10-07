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
/// are those measured for each convention and target with the parts' own
/// transfer, a guaranteed tail call through a table-loaded address
/// (compiler/match-dispatch-lowering): on x86-64 outside Windows that
/// address takes one of the twelve registers `preserve_none` passes
/// arguments in. A loop whose parts would need more is not split.
fn convention(triple: &str) -> (&'static str, ArgumentRegisters) {
    let aarch64 = triple.starts_with("aarch64");
    let windows = triple.contains("windows");
    if env!("WHITEFOOT_PRESERVE_NONE") == "1" {
        let integer = if aarch64 {
            24
        } else if windows {
            12
        } else {
            11
        };
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
    /// The integer and floating-point argument registers parameters of these
    /// LLVM types take, or the first type that is not a pointer, an integer,
    /// a float or a range's `{ ptr, i64 }` pair.
    fn demand(types: &[String]) -> Result<(usize, usize), String> {
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
                other => return Err(other.to_owned()),
            }
        }
        Ok((integer, float))
    }

    fn fits(self, (integer, float): (usize, usize)) -> bool {
        match self {
            Self::Separate {
                integer: integers,
                float: floats,
            } => integer <= integers && float <= floats,
            Self::Shared(positions) => integer + float <= positions,
        }
    }

    fn describe(self) -> String {
        match self {
            Self::Separate { integer, float } => format!("{integer} integer and {float} floating"),
            Self::Shared(positions) => format!("{positions} shared"),
        }
    }
}

/// One recognised dispatch loop of a function.
pub(super) struct DispatchLoop {
    pub(super) header: IrBlockId,
    /// The enum the header's `match` takes apart.
    pub(super) matched: crate::IrNominalId,
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

/// Why a loop around a `match` is not a dispatch loop, or why a dispatch loop
/// is not split, for the dispatch ledger (compiler/match-dispatch-lowering).
pub(super) struct Rejection {
    /// The enum the loop's `match` takes apart.
    pub(super) matched: crate::IrNominalId,
    pub(super) reason: String,
}

/// The loops of a function whose `match` over a nominal enum with at least
/// two targets lies on a cycle through it: the first, in block order, that
/// heads a dispatch loop, and every other with the first condition it fails.
/// A `match` inside the loop of a `match` considered before it, such as one
/// in an arm, is not a loop of its own.
/// A dispatch loop's header ends in that `match`, none of whose targets
/// takes parameters; the blocks reachable from its targets without passing
/// through it are entered only from it and leave only by returning or by
/// jumping back to it.
pub(super) fn find(
    function: &IrFunction,
    reachable: &[bool],
) -> (Option<DispatchLoop>, Vec<Rejection>) {
    let blocks = function.blocks();
    let mut found = None;
    let mut rejections = Vec::new();
    let mut covered = vec![false; blocks.len()];
    for (header, block) in blocks.iter().enumerate() {
        if header == 0 || !reachable[header] {
            continue;
        }
        let IrTerminator::Match {
            enum_type: IrEnumType::Nominal(matched),
            targets,
            ..
        } = block.terminator()
        else {
            continue;
        };
        if targets.len() < 2 {
            continue;
        }
        let starts: Vec<usize> = targets
            .iter()
            .map(|target| target.block().index())
            .collect();
        let region = reach(function, &starts, header);
        let cyclic = blocks
            .iter()
            .enumerate()
            .any(|(index, member)| region[index] && successors(member).contains(&header));
        if !cyclic || covered[header] {
            continue;
        }
        for (index, member) in region.iter().enumerate() {
            covered[index] |= *member;
        }
        let reject = |reason: String| Rejection {
            matched: *matched,
            reason,
        };
        if found.is_some() {
            rejections.push(reject(
                "only the function's first dispatch loop is split".to_owned(),
            ));
            continue;
        }
        if targets
            .iter()
            .any(|target| !blocks[target.block().index()].parameters().is_empty())
        {
            rejections.push(reject("a target of its match takes parameters".to_owned()));
            continue;
        }
        if region[0] {
            rejections.push(reject("the loop contains the function's entry".to_owned()));
            continue;
        }
        let mut problem = None;
        for (index, candidate) in blocks.iter().enumerate() {
            if !reachable[index] || problem.is_some() {
                continue;
            }
            for successor in successors(candidate) {
                if region[index] && successor == header {
                    if !matches!(candidate.terminator(), IrTerminator::Jump { .. }) {
                        problem = Some("an edge back to its match is not a jump".to_owned());
                    }
                } else if region[index] && !region[successor] {
                    // The arm whose blocks reach this exit.
                    let arm = targets
                        .iter()
                        .find(|target| reach(function, &[target.block().index()], header)[index])
                        .map_or(0, |target| target.tag());
                    problem = Some(format!(
                        "the arm for tag {arm} leaves the loop other than by returning"
                    ));
                } else if !region[index] && index != header && region[successor] {
                    problem = Some(
                        "the loop is entered other than at its match, which is therefore not its header"
                            .to_owned(),
                    );
                }
                if problem.is_some() {
                    break;
                }
            }
        }
        if let Some(problem) = problem {
            rejections.push(reject(problem));
            continue;
        }
        let mut arms: Vec<(IrBlockId, Vec<bool>)> = Vec::new();
        let max_tag = targets.iter().map(|target| target.tag()).max().unwrap_or(0);
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
            rejections.push(reject("its match leaves a tag without a target".to_owned()));
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
        let Some(header) = IrBlockId::from_index(header) else {
            continue;
        };
        found = Some(DispatchLoop {
            header,
            matched: *matched,
            region,
            arms,
            table,
            header_values,
            invariants,
        });
    }
    (found, rejections)
}

/// What [`FunctionEmitter::loop_invariants`] finds the loop cannot change.
#[derive(Default)]
struct LoopInvariants {
    /// Header instructions, by index, the enclosing function computes once.
    hoisted: Vec<usize>,
    hoisted_values: HashSet<IrValueId>,
    /// Hoisted candidates for the parts, before cursor selection removes
    /// reads that only the enclosing function needs.
    passed: Vec<IrValueId>,
    /// Header parameters passed through unchanged that nothing in the loop
    /// reads once the hoisted work has left it.
    dropped: HashSet<IrValueId>,
    facts: HashMap<IrValueId, String>,
    /// For a reference whose box the loop never replaces, every projection
    /// of its referent inside the loop, mapped to the one the enclosing
    /// function computes: (the header parameter, the block and index of that
    /// projection's instruction).
    pinned: Vec<(IrValueId, usize, usize)>,
    replaced: HashMap<IrValueId, IrValueId>,
    /// For a reference whose box the loop keeps and which the loop hands to
    /// callees that cannot replace that box: the reference and its hoisted
    /// projection, which each part stores in a slot of its own.
    pins: Vec<(IrValueId, IrValueId)>,
    /// The call arguments that name such a reference, which each part
    /// replaces by its slot.
    pin_arguments: HashMap<IrValueId, IrValueId>,
    /// References whose every projection is hoisted, which the loop then
    /// only hands on to itself through joins and its own back-edge place.
    unread: HashSet<IrValueId>,
    /// Header parameters every back edge passes through unchanged.
    passed_through: HashSet<IrValueId>,
}

impl LoopInvariants {
    /// Records that the parts hand `arguments`, each naming `parameter`, to
    /// callees through a slot holding `canonical`, the box's hoisted
    /// projection; a pinned reference the loop hands to no callee needs none.
    fn pin(&mut self, parameter: IrValueId, canonical: IrValueId, arguments: &[IrValueId]) {
        if arguments.is_empty() {
            return;
        }
        self.pins.push((parameter, canonical));
        for argument in arguments {
            self.pin_arguments.insert(*argument, parameter);
        }
        if !self.passed.contains(&canonical) {
            self.passed.push(canonical);
        }
    }
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
    /// A header value the enclosing function computes once, because the
    /// loop cannot change it.
    Hoisted,
    /// The matched element's address, which every edge into the header
    /// passes for the index it gives the header (see [`Cursor`]).
    Cursor,
}

/// The header's `match` reads the element a header parameter indexes in a
/// run of slots at an address the loop cannot change, and the arms read
/// that element through its address. The parts then carry the address as
/// well as the index: an edge into the header passes the address of the
/// element its index selects, formed from the address the part received,
/// and the header uses the address it receives instead of forming it from
/// the index.
#[derive(Clone, Copy)]
struct Cursor {
    /// The header value addressing the matched element.
    place: IrValueId,
    /// The header parameter that indexes it.
    index: IrValueId,
    /// The run's address.
    run: IrValueId,
    /// The header instruction that defines `place`.
    instruction: usize,
    /// The element's type.
    element: IrType,
    /// The header projection's target-domain obligation.
    target_domain: crate::IrTargetDomainObligation,
}

/// The emission state of one split function.
pub(super) struct DispatchEmission {
    pub(super) plan: DispatchLoop,
    pub(super) part: Part,
    symbol: String,
    arm_symbols: Vec<String>,
    table_symbol: String,
    parameters: Vec<(IrValueId, IrType, Role)>,
    /// The header instructions, by index, that the enclosing function
    /// computes once before calling the dispatch function.
    hoisted: Vec<usize>,
    /// The checked reference facts of a parameter that holds the same
    /// pointer as one of the function's own parameters.
    facts: HashMap<IrValueId, String>,
    /// Box-referent projections of references whose box the loop keeps,
    /// computed once by the enclosing function (see [`LoopInvariants`]).
    pinned: Vec<(IrValueId, usize, usize)>,
    replaced: HashMap<IrValueId, IrValueId>,
    /// See [`LoopInvariants::pins`] and [`LoopInvariants::pin_arguments`].
    pins: Vec<(IrValueId, IrValueId)>,
    pin_arguments: HashMap<IrValueId, IrValueId>,
    /// Whether the parts receive the handler table's address.
    table_base: bool,
    /// The matched element's address the parts carry, if any.
    cursor: Option<Cursor>,
    /// Values the loop cannot change that the parts read from the frame,
    /// stored there by the enclosing function before the loop.
    spilled: Vec<(IrValueId, IrType)>,
    /// Header parameters no part receives. A join inside the loop may still
    /// name one only to hand it back to itself, a value nothing reads, so
    /// each part defines it as a frozen poison value.
    dropped: Vec<(IrValueId, IrType)>,
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
        let (found, rejections) = find(self.function, reachable);
        let over = |matched: crate::IrNominalId| {
            let name = self
                .program
                .nominal(matched)
                .map_or("an enum", |nominal| nominal.name.as_str());
            format!("{body_symbol}: not split: the loop over {name}")
        };
        let mut ledger: Vec<String> = rejections
            .iter()
            .map(|rejection| format!("{}: {}", over(rejection.matched), rejection.reason))
            .collect();
        let Some(plan) = found else {
            self.dispatch_ledger.extend(ledger);
            return Ok(None);
        };
        let kind = if self.function.waits() {
            Some("the function waits")
        } else if self.grain.is_some() {
            Some("it is a recursion-budget variant")
        } else if self.sequential_clones.is_some() {
            Some("it is a sequential clone")
        } else if !self.function.overlaps().is_empty() {
            Some("the function has overlap groups")
        } else if self.function.synthesis().is_some() {
            Some("the function is compiler-synthesized")
        } else if !super::contexts::context_group_prelude(self.function).is_empty()
            || !super::shared::record_prelude(self.function).is_empty()
        {
            Some("the function has a context or shared-record prelude")
        } else {
            None
        };
        if let Some(kind) = kind {
            ledger.push(format!("{}: {kind}", over(plan.matched)));
            self.dispatch_ledger.extend(ledger);
            return Ok(None);
        }
        let header = self.block(plan.header)?.clone();
        let invariant = self.loop_invariants(&plan, &header, reachable);
        let mut parameters = Vec::new();
        for (value, ty) in header.parameters() {
            if self.storage.slot(*value).is_none() && !invariant.dropped.contains(value) {
                parameters.push((*value, *ty, Role::Carried));
            }
        }
        let header_values: Vec<IrValueId> = plan
            .header_values
            .iter()
            .copied()
            .filter(|value| !invariant.hoisted_values.contains(value))
            .collect();
        for (values, role) in [
            (&invariant.passed, Role::Hoisted),
            (&header_values, Role::HeaderValue),
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
        let cursor = self.element_cursor(&header, &parameters)?;
        if let Some(cursor) = cursor {
            for (value, _, role) in &mut parameters {
                if *value == cursor.place {
                    *role = Role::Cursor;
                }
            }
        }
        let readers = self.arms_reading(&plan, &invariant, cursor);
        let mut part_reads: HashSet<IrValueId> = readers.keys().copied().collect();
        // Every part's prelude stores these values, even when that part
        // hands no pin to a callee. Keep them live without treating those
        // unconditional stores as readers for the spill order.
        part_reads.extend(invariant.pins.iter().map(|(_, canonical)| *canonical));
        parameters.retain(|(value, _, role)| {
            !matches!(role, Role::Hoisted | Role::Invariant) || part_reads.contains(value)
        });
        let (convention, registers) = convention(self.target.triple());
        let mut scratch = References::default();
        let mut typed: Vec<(IrValueId, String)> = Vec::new();
        for (value, ty, _) in &parameters {
            typed.push((
                *value,
                llvm_type_with_references(self.program, *ty, &mut scratch.types)?,
            ));
        }
        let frame_before = !self.frame.target.is_empty();
        let types_without = |spilled: &HashSet<IrValueId>| {
            let mut types = Vec::new();
            if destination {
                types.push("ptr".to_owned());
            }
            types.extend(
                typed
                    .iter()
                    .filter(|(value, _)| !spilled.contains(value))
                    .map(|(_, ty)| ty.clone()),
            );
            if frame_before || !spilled.is_empty() {
                types.push("ptr".to_owned());
            }
            types
        };
        // Past the registers, the values the loop cannot change go to the
        // frame, the one the fewest arms read first; changing carried values
        // and header values never do.
        let mut spilled: HashSet<IrValueId> = HashSet::new();
        if ArgumentRegisters::demand(&types_without(&spilled))
            .is_ok_and(|demand| !registers.fits(demand))
        {
            let mut candidates: Vec<(usize, IrValueId)> = parameters
                .iter()
                .filter(|(value, _, role)| {
                    matches!(role, Role::Invariant | Role::Hoisted)
                        || (*role == Role::Carried && invariant.passed_through.contains(value))
                })
                .map(|(value, _, _)| (readers.get(value).copied().unwrap_or(0), *value))
                .collect();
            candidates.sort();
            for (_, value) in candidates {
                spilled.insert(value);
                if ArgumentRegisters::demand(&types_without(&spilled))
                    .is_ok_and(|demand| registers.fits(demand))
                {
                    break;
                }
            }
        }
        let mut types = types_without(&spilled);
        // The handler table's address travels as a parameter where a register
        // is left for it, instead of being formed again in every arm.
        let mut with_base = types.clone();
        with_base.push("ptr".to_owned());
        let table_base = spilled.is_empty()
            && ArgumentRegisters::demand(&with_base).is_ok_and(|demand| registers.fits(demand));
        if table_base {
            types = with_base;
        }
        let triple = self.target.triple();
        let name = if convention.is_empty() {
            "C"
        } else {
            "preserve_none"
        };
        let demand = match ArgumentRegisters::demand(&types) {
            Ok(demand) => demand,
            Err(ty) => {
                ledger.push(format!(
                    "{}: a part's parameter of type {ty} has no argument register class",
                    over(plan.matched)
                ));
                self.dispatch_ledger.extend(ledger);
                return Ok(None);
            }
        };
        if !registers.fits(demand) {
            ledger.push(format!(
                "{}: its parts need {} integer and {} floating argument registers, over the {} that {name} has on {triple}",
                over(plan.matched),
                demand.0,
                demand.1,
                registers.describe()
            ));
            self.dispatch_ledger.extend(ledger);
            return Ok(None);
        }
        let spills: Vec<(IrValueId, IrType)> = parameters
            .iter()
            .filter(|(value, _, _)| spilled.contains(value))
            .map(|(value, ty, _)| (*value, *ty))
            .collect();
        if !spills.is_empty() {
            self.frame = FunctionFramePlan::build(
                self.target,
                self.program,
                self.function,
                super::FunctionFrameContents {
                    storage: &self.storage,
                    result_slot: self.result_slot,
                    spills: &spills,
                },
            )?;
            parameters.retain(|(value, _, _)| !spilled.contains(value));
        }
        let matched = self
            .program
            .nominal(plan.matched)
            .map_or("an enum", |nominal| nominal.name.as_str());
        if cursor.is_some() {
            ledger.insert(
                0,
                format!("{body_symbol}: carries the matched {matched}'s address between the parts"),
            );
        }
        if !spills.is_empty() {
            ledger.insert(
                0,
                format!(
                    "{body_symbol}: keeps {} value{} the loop cannot change in the frame",
                    spills.len(),
                    if spills.len() == 1 { "" } else { "s" }
                ),
            );
        }
        ledger.insert(
            0,
            format!(
                "{body_symbol}: split: the loop over {matched} into {} arms, taking {} integer and {} floating of the {} argument registers that {name} has on {triple}",
                plan.arms.len(),
                demand.0,
                demand.1,
                registers.describe()
            ),
        );
        self.dispatch_ledger.extend(ledger);
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
            hoisted: invariant.hoisted,
            facts: invariant.facts,
            pinned: invariant.pinned,
            replaced: invariant.replaced,
            pins: invariant.pins,
            pin_arguments: invariant.pin_arguments,
            spilled: spills,
            table_base,
            cursor,
            dropped: header
                .parameters()
                .iter()
                .filter(|(value, _)| invariant.dropped.contains(value))
                .copied()
                .collect(),
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
        cursor: Option<&str>,
    ) -> Result<String, BackendFailure> {
        let dispatch = self.dispatch.as_ref().ok_or(BackendFailure::InvalidIr)?;
        let parameters = dispatch.parameters.clone();
        let (destination, frame) = (dispatch.destination, dispatch.frame);
        let base = dispatch.table_base.then(|| {
            if dispatch.part == Part::Enclosing {
                format!("ptr @{}", dispatch.table_symbol)
            } else {
                "ptr %wf.dispatch.base".to_owned()
            }
        });
        if dispatch.table_base && dispatch.part == Part::Enclosing {
            let table = dispatch.table_symbol.clone();
            self.output.symbol(table);
        }
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
                Role::Cursor if !into_arm => cursor.ok_or(BackendFailure::InvalidIr)?.to_owned(),
                Role::HeaderValue if !into_arm => "poison".to_owned(),
                Role::HeaderValue | Role::Cursor | Role::Invariant | Role::Hoisted => {
                    self.value_name(value)
                }
            };
            arguments.push(format!("{ty_name} {operand}"));
        }
        arguments.extend(base);
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
        let cursor = dispatch.cursor;
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
        if !tail {
            // All hoisted header values, including those only the entering
            // cursor needs, computed once from this edge's arguments.
            let hoisted = self
                .dispatch
                .as_ref()
                .map(|dispatch| dispatch.hoisted.clone())
                .unwrap_or_default();
            let header = header.clone();
            let pinned = self
                .dispatch
                .as_ref()
                .map(|dispatch| dispatch.pinned.clone())
                .unwrap_or_default();
            // The header's parameters, and the joins inside the loop that are
            // the same values, are not defined in the enclosing function:
            // each one the hoisted work reads is named there as this edge's
            // argument for the header parameter it stands for.
            let mut read: Vec<(IrValueId, IrValueId)> = Vec::new();
            for index in &hoisted {
                if let Some(instruction) = header.instructions().get(*index) {
                    for operand in instruction.operands() {
                        read.push((operand, operand));
                    }
                }
            }
            for (parameter, block, index) in &pinned {
                let instruction = self
                    .function
                    .blocks()
                    .get(*block)
                    .and_then(|block| block.instructions().get(*index))
                    .ok_or(BackendFailure::InvalidIr)?;
                for operand in instruction.operands() {
                    read.push((operand, *parameter));
                }
            }
            let mut named: HashSet<IrValueId> = HashSet::new();
            for (value, stands_for) in read {
                let Some(position) = header
                    .parameters()
                    .iter()
                    .position(|(parameter, _)| *parameter == stands_for)
                else {
                    continue;
                };
                if !named.insert(value) || self.storage.slot(stands_for).is_some() {
                    continue;
                }
                let ty = self
                    .output
                    .type_name(self.program, header.parameters()[position].1)?;
                let argument = self.value_name(carried[position].1);
                writeln!(
                    self.output,
                    "  {} = select i1 true, {ty} {argument}, {ty} {argument}",
                    value_name(value)
                )
                .map_err(|_| BackendFailure::TextEmission)?;
            }
            for index in hoisted {
                let instruction = header
                    .instructions()
                    .get(index)
                    .ok_or(BackendFailure::InvalidIr)?;
                self.emit_instruction(target, index, instruction)?;
            }
            for (_, block, index) in pinned {
                let block_id =
                    IrBlockId::from_index(block).ok_or(BackendFailure::CounterOverflow)?;
                let instruction = self
                    .function
                    .blocks()
                    .get(block)
                    .and_then(|block| block.instructions().get(index))
                    .ok_or(BackendFailure::InvalidIr)?
                    .clone();
                self.emit_instruction(block_id, index, &instruction)?;
            }
        }
        if !tail {
            let spilled = self
                .dispatch
                .as_ref()
                .map(|dispatch| dispatch.spilled.clone())
                .unwrap_or_default();
            for (value, ty) in spilled {
                // A passed-through header parameter is this edge's argument.
                let source = carried
                    .iter()
                    .find(|(parameter, _)| *parameter == value)
                    .map_or(value, |(_, argument)| *argument);
                let slot = self.entry_slot(FunctionSlot::Spill(value))?;
                let ty = self.output.type_name(self.program, ty)?;
                let operand = self.value_name(source);
                writeln!(self.output, "  store {ty} {operand}, ptr {slot}")
                    .map_err(|_| BackendFailure::TextEmission)?;
            }
        }
        self.emit_place_edge(target, arguments, drops)?;
        let cursor = match cursor {
            Some(cursor) => Some(self.edge_cursor(cursor, &carried, tail)?),
            None => None,
        };
        let list = self.dispatch_arguments(&carried, false, cursor.as_deref())?;
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
        let table = if dispatch.table_base {
            "%wf.dispatch.base".to_owned()
        } else {
            format!("@{}", dispatch.table_symbol)
        };
        let table_symbol = dispatch.table_symbol.clone();
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
        let list = self.dispatch_arguments(&carried, true, None)?;
        let slot = self.next_temporary()?;
        let handler = self.next_temporary()?;
        self.output.symbol(table_symbol);
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
            "  %{slot} = getelementptr inbounds [{entries} x ptr], ptr {table}, i64 0, i64 %{index}\n  %{handler} = load ptr, ptr %{slot}\n"
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
            // A range arrives in the parts as its `{ ptr, i64 }` pair, which
            // carries no pointer attribute; only a pointer takes the facts.
            let facts = if ty == "ptr" {
                dispatch.facts.get(value).map_or("", String::as_str)
            } else {
                ""
            };
            parameters.push(Parameter::named(format!("{ty}{facts}"), name));
        }
        if dispatch.table_base {
            parameters.push(Parameter::named("ptr", "%wf.dispatch.base"));
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
        let dropped = dispatch.dropped.clone();
        let spilled = dispatch.spilled.clone();
        let pins = dispatch.pins.clone();
        let mut parts: Vec<(Signature, FunctionBody, HashSet<FunctionSlot>)> = Vec::new();

        self.set_part(Part::Header);
        self.output = FunctionBody::default();
        self.output.open_block("entry".to_owned());
        writeln!(self.output, "  br label %{}", block_label(header))
            .map_err(|_| BackendFailure::TextEmission)?;
        self.materialized.clear();
        self.output.open_block(block_label(header));
        let block = self.block(header)?.clone();
        let hoisted = self
            .dispatch
            .as_ref()
            .map(|dispatch| dispatch.hoisted.clone())
            .unwrap_or_default();
        // The header receives the matched element's address it would
        // otherwise form from the index.
        let carried_place = self
            .dispatch
            .as_ref()
            .and_then(|dispatch| dispatch.cursor)
            .map(|cursor| cursor.instruction);
        for (index, instruction) in block.instructions().iter().enumerate() {
            if !hoisted.contains(&index) && Some(index) != carried_place {
                self.emit_part_instruction(header, index, instruction)?;
            }
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
                    self.emit_part_instruction(block_id, instruction_index, instruction)?;
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
            let mut prelude =
                self.frame
                    .render_split(self.program, &mut body.references, false, &locals)?;
            for (value, ty) in &spilled {
                let slot = self.frame.slot(FunctionSlot::Spill(*value))?;
                let ty = llvm_type_with_references(self.program, *ty, &mut body.references.types)?;
                writeln!(prelude, "  {} = load {ty}, ptr {slot}", value_name(*value))
                    .map_err(|_| BackendFailure::TextEmission)?;
            }
            for (value, ty) in &dropped {
                let ty = llvm_type_with_references(self.program, *ty, &mut body.references.types)?;
                writeln!(prelude, "  {} = freeze {ty} poison", value_name(*value))
                    .map_err(|_| BackendFailure::TextEmission)?;
            }
            for (parameter, canonical) in &pins {
                let slot = pin_slot_name(*parameter);
                writeln!(prelude, "  {slot} = alloca ptr")
                    .and_then(|()| {
                        writeln!(
                            prelude,
                            "  store ptr {}, ptr {slot}",
                            value_name(*canonical)
                        )
                    })
                    .map_err(|_| BackendFailure::TextEmission)?;
            }
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

    /// What the loop cannot change. A header parameter is passed through
    /// when every edge back to the header gives it its own value; it holds
    /// a read-only reference when the one edge into the loop gives it one of
    /// the function's read-only reference parameters, whose referents the
    /// function does not write [EFF-2, EFF-5]. A header instruction that
    /// projects a box's referent or reads a container's measure through such
    /// a reference, or through a value so hoisted, is hoisted into the
    /// enclosing function. A passed-through parameter nothing else in the
    /// loop reads is dropped, and one holding the same pointer as a function
    /// parameter carries that parameter's checked facts.
    fn loop_invariants(
        &self,
        plan: &DispatchLoop,
        header: &IrBlock,
        reachable: &[bool],
    ) -> LoopInvariants {
        let mut result = LoopInvariants::default();
        let blocks = self.function.blocks();
        let parameters: Vec<IrValueId> = header
            .parameters()
            .iter()
            .map(|(value, _)| *value)
            .collect();
        let mut entries = Vec::new();
        let mut backedges = Vec::new();
        for (index, block) in blocks.iter().enumerate() {
            if let IrTerminator::Jump {
                target, arguments, ..
            } = block.terminator()
                && *target == plan.header
                && reachable[index]
            {
                if plan.region[index] {
                    backedges.push(arguments.clone());
                } else {
                    entries.push(arguments.clone());
                }
            }
        }
        let [entry] = entries.as_slice() else {
            return result;
        };
        // A block parameter inside the loop whose every incoming value is the
        // same value, as at the join of an `if` in an arm, is that value.
        let mut incoming: HashMap<IrValueId, Vec<IrValueId>> = HashMap::new();
        for (index, block) in blocks.iter().enumerate() {
            if !plan.region[index] {
                continue;
            }
            if let IrTerminator::Jump {
                target, arguments, ..
            } = block.terminator()
                && plan.region[target.index()]
            {
                for ((parameter, _), argument) in
                    blocks[target.index()].parameters().iter().zip(arguments)
                {
                    incoming.entry(*parameter).or_default().push(*argument);
                }
            }
        }
        let mut same: HashMap<IrValueId, IrValueId> = HashMap::new();
        loop {
            let mut changed = false;
            for (parameter, arguments) in &incoming {
                let roots: HashSet<IrValueId> = arguments
                    .iter()
                    .map(|argument| *same.get(argument).unwrap_or(argument))
                    .collect();
                if let [root] = roots.into_iter().collect::<Vec<_>>().as_slice()
                    && root != parameter
                    && same.get(parameter) != Some(root)
                {
                    same.insert(*parameter, *root);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        let root = |value: &IrValueId| *same.get(value).unwrap_or(value);
        let passed_through: Vec<bool> = (0..parameters.len())
            .map(|position| {
                backedges.iter().all(|arguments| {
                    arguments.get(position).map(root) == Some(parameters[position])
                })
            })
            .collect();
        result.passed_through = parameters
            .iter()
            .zip(&passed_through)
            .filter(|(_, through)| **through)
            .map(|(parameter, _)| *parameter)
            .collect();
        let function_parameters: Vec<IrValueId> = self
            .function
            .parameters()
            .iter()
            .map(|(value, _)| *value)
            .collect();
        let readonly: HashSet<IrValueId> = self
            .function
            .readonly_reference_parameters
            .iter()
            .copied()
            .collect();
        let mut rooted: HashSet<IrValueId> = HashSet::new();
        for (position, parameter) in parameters.iter().enumerate() {
            // A parameter held in a frame slot is no SSA value the enclosing
            // function could name for the hoisted work.
            if !passed_through[position] || self.storage.slot(*parameter).is_some() {
                continue;
            }
            let Some(argument) = entry.get(position) else {
                continue;
            };
            if readonly.contains(argument) {
                rooted.insert(*parameter);
            }
            if let Some(index) = function_parameters
                .iter()
                .position(|value| value == argument)
                && let Some((_, ty)) = self.function.parameters().get(index)
                && let Ok(facts) = self.reference_parameter_facts(index, *ty)
                && !facts.is_empty()
            {
                result.facts.insert(*parameter, facts);
            }
        }
        for value in &plan.invariants {
            if readonly.contains(value) {
                rooted.insert(*value);
            }
        }
        for (index, instruction) in header.instructions().iter().enumerate() {
            let IrInstruction::Define {
                result: value,
                operation,
                ..
            } = instruction
            else {
                continue;
            };
            let hoist = match operation {
                crate::IrOperation::ProjectAddress {
                    address,
                    projection: crate::IrPlaceStep::BoxReferent { .. },
                } if rooted.contains(address) => {
                    rooted.insert(*value);
                    true
                }
                crate::IrOperation::ContainerMeasure { container, .. }
                    if rooted.contains(container) =>
                {
                    true
                }
                _ => false,
            };
            if hoist {
                result.hoisted.push(index);
                result.hoisted_values.insert(*value);
            }
        }
        // What the loop still reads once the hoisted work leaves it: the
        // header's remaining instructions and terminator and every block of
        // the loop, apart from a back edge's argument for the parameter it
        // passes through.
        let mut reads: HashSet<IrValueId> = HashSet::new();
        for (index, instruction) in header.instructions().iter().enumerate() {
            if !result.hoisted.contains(&index) {
                reads.extend(instruction.operands());
            }
        }
        reads.extend(header.terminator().operands());
        for (index, block) in blocks.iter().enumerate() {
            if !plan.region[index] {
                continue;
            }
            for instruction in block.instructions() {
                reads.extend(instruction.operands());
            }
            match block.terminator() {
                IrTerminator::Jump {
                    target,
                    arguments,
                    drops,
                } if *target == plan.header => {
                    for (position, argument) in arguments.iter().enumerate() {
                        if !passed_through.get(position).copied().unwrap_or(false)
                            || root(argument) != *argument
                        {
                            reads.insert(*argument);
                        }
                    }
                    reads.extend(drops.iter().map(|drop| drop.operand()));
                }
                terminator => reads.extend(terminator.operands()),
            }
        }
        // A passed-through reference the loop uses only to project its box's
        // referent, to hand on unchanged to a join's parameter that is the
        // same value or to its own place on a back edge, or to hand to a
        // non-waiting callee whose formal writes only below the box's content,
        // keeps one box for the whole loop: nothing in the loop can replace
        // that box, and no other reference reaches it [EFF-5]. Its
        // projections are hoisted, and such a callee receives a part-local
        // slot holding the hoisted box.
        for (position, parameter) in parameters.iter().enumerate() {
            if !passed_through[position] || self.storage.slot(*parameter).is_some() {
                continue;
            }
            let mut projections: Vec<(usize, usize, IrValueId)> = Vec::new();
            let mut kept_arguments: Vec<IrValueId> = Vec::new();
            let mut pinned = true;
            let blocks_in_loop = blocks
                .iter()
                .enumerate()
                .filter(|(index, _)| plan.region[*index] || *index == plan.header.index());
            for (index, block) in blocks_in_loop {
                for (position_in_block, instruction) in block.instructions().iter().enumerate() {
                    let operands = instruction.operands();
                    if !operands.iter().any(|value| root(value) == *parameter) {
                        continue;
                    }
                    match instruction {
                        IrInstruction::Define {
                            result: value,
                            operation:
                                crate::IrOperation::ProjectAddress {
                                    address,
                                    projection: crate::IrPlaceStep::BoxReferent { .. },
                                },
                            ..
                        } if root(address) == *parameter && operands.len() == 1 => {
                            projections.push((index, position_in_block, *value));
                        }
                        IrInstruction::Define {
                            operation:
                                crate::IrOperation::Call {
                                    function,
                                    arguments,
                                },
                            ..
                        } if self.callee_keeps_box(*function, arguments, |value| {
                            root(value) == *parameter
                        }) =>
                        {
                            kept_arguments.extend(
                                arguments
                                    .iter()
                                    .copied()
                                    .filter(|argument| root(argument) == *parameter),
                            );
                        }
                        _ => pinned = false,
                    }
                }
                match block.terminator() {
                    IrTerminator::Jump {
                        target,
                        arguments,
                        drops,
                    } => {
                        let target_parameters = blocks[target.index()].parameters();
                        for (argument_position, argument) in arguments.iter().enumerate() {
                            if root(argument) != *parameter {
                                continue;
                            }
                            let onward = target_parameters
                                .get(argument_position)
                                .map(|(value, _)| *value);
                            let own_place = *target == plan.header && argument_position == position;
                            let alias = plan.region[target.index()]
                                && onward.is_some_and(|value| root(&value) == *parameter);
                            pinned &= own_place || alias;
                        }
                        pinned &= !drops.iter().any(|drop| root(&drop.operand()) == *parameter);
                    }
                    terminator => {
                        pinned &= !terminator
                            .operands()
                            .iter()
                            .any(|value| root(value) == *parameter);
                    }
                }
            }
            if !pinned || projections.is_empty() {
                continue;
            }
            // Every projection of this reference is now computed before the
            // loop, whether by the read-only rule above or here, so what is
            // left of it in the loop only passes it on to itself.
            let hoisted_already = projections
                .iter()
                .filter(|(_, _, projected)| result.hoisted_values.contains(projected))
                .count();
            if hoisted_already == projections.len() {
                result.unread.insert(*parameter);
                let canonical = projections[0].2;
                result.pin(*parameter, canonical, &kept_arguments);
                continue;
            }
            if hoisted_already > 0 {
                // The header's hoisted projection stands for the arms' ones.
                let Some(canonical) = projections
                    .iter()
                    .map(|(_, _, projected)| *projected)
                    .find(|projected| result.hoisted_values.contains(projected))
                else {
                    continue;
                };
                for (_, _, projected) in &projections {
                    if !result.hoisted_values.contains(projected) {
                        result.replaced.insert(*projected, canonical);
                    }
                }
                result.unread.insert(*parameter);
                if !result.passed.contains(&canonical) {
                    result.passed.push(canonical);
                }
                result.pin(*parameter, canonical, &kept_arguments);
                continue;
            }
            result.unread.insert(*parameter);
            let (block, index, canonical) = projections[0];
            for (_, _, projected) in &projections {
                result.replaced.insert(*projected, canonical);
            }
            result.pinned.push((*parameter, block, index));
            result.passed.push(canonical);
            result.pin(*parameter, canonical, &kept_arguments);
        }
        for (position, parameter) in parameters.iter().enumerate() {
            if passed_through[position]
                && (!reads.contains(parameter) || result.unread.contains(parameter))
            {
                result.dropped.insert(*parameter);
            }
        }
        let pinned_passed = std::mem::take(&mut result.passed);
        result.passed = result
            .hoisted
            .iter()
            .filter_map(|index| match header.instructions().get(*index) {
                Some(IrInstruction::Define { result: value, .. }) if reads.contains(value) => {
                    Some(*value)
                }
                _ => None,
            })
            .collect();
        // A pinned canonical may also be a hoisted value the loop reads.
        for value in pinned_passed {
            if !result.passed.contains(&value) {
                result.passed.push(value);
            }
        }
        result
    }

    /// Whether a call hands every argument `names` selects to a formal whose
    /// declared writes all lie below its referent's box content, so the
    /// call cannot replace that box [EFF-1, EFF-5]; a waiting callee never
    /// qualifies, its call being a transfer rather than an ordinary call.
    fn callee_keeps_box(
        &self,
        function: u32,
        arguments: &[IrValueId],
        names: impl Fn(&IrValueId) -> bool,
    ) -> bool {
        let Some(callee) = self.program.functions().get(function as usize) else {
            return false;
        };
        if callee.waits() {
            return false;
        }
        let keeping: HashSet<IrValueId> = callee
            .box_keeping_reference_parameters
            .iter()
            .copied()
            .collect();
        let mut any = false;
        for (position, argument) in arguments.iter().enumerate() {
            if !names(argument) {
                continue;
            }
            let Some((formal, _)) = callee.parameters().get(position) else {
                return false;
            };
            if !keeping.contains(formal) {
                return false;
            }
            any = true;
        }
        any
    }

    /// Emits one instruction of a part: a hoisted box-referent projection is
    /// the parameter the part receives, and another projection of the same
    /// box is a name for it.
    fn emit_part_instruction(
        &mut self,
        block: IrBlockId,
        index: usize,
        instruction: &IrInstruction,
    ) -> Result<(), BackendFailure> {
        if let IrInstruction::Define { result, .. } = instruction
            && let Some(canonical) = self
                .dispatch
                .as_ref()
                .and_then(|dispatch| dispatch.replaced.get(result).copied())
        {
            if canonical != *result {
                writeln!(
                    self.output,
                    "  {} = select i1 true, ptr {}, ptr {}",
                    value_name(*result),
                    value_name(canonical),
                    value_name(canonical)
                )
                .map_err(|_| BackendFailure::TextEmission)?;
            }
            return Ok(());
        }
        if let IrInstruction::Define {
            operation: crate::IrOperation::Call { arguments, .. },
            ..
        } = instruction
        {
            let renamed: Vec<(IrValueId, IrValueId)> = self
                .dispatch
                .as_ref()
                .map(|dispatch| {
                    arguments
                        .iter()
                        .filter_map(|argument| {
                            dispatch
                                .pin_arguments
                                .get(argument)
                                .map(|parameter| (*argument, *parameter))
                        })
                        .collect()
                })
                .unwrap_or_default();
            if !renamed.is_empty() {
                for (argument, parameter) in &renamed {
                    self.pin_names.insert(*argument, pin_slot_name(*parameter));
                }
                let emitted = self.emit_instruction(block, index, instruction);
                self.pin_names.clear();
                return emitted;
            }
        }
        self.emit_instruction(block, index, instruction)
    }

    /// The header instruction that addresses the matched element, which the
    /// arms read through that address, by a carried index into a run of
    /// slots at an address the loop cannot change (see [`Cursor`]).
    fn element_cursor(
        &self,
        header: &IrBlock,
        parameters: &[(IrValueId, IrType, Role)],
    ) -> Result<Option<Cursor>, BackendFailure> {
        let role = |value: IrValueId| {
            parameters
                .iter()
                .find(|(parameter, _, _)| *parameter == value)
                .map(|(_, _, role)| *role)
        };
        let IrTerminator::Match { scrutinee, .. } = header.terminator() else {
            return Ok(None);
        };
        // The matched element: the scrutinee's place, or the scrutinee
        // itself where it is an address.
        let matched = header
            .instructions()
            .iter()
            .find_map(|instruction| match instruction {
                IrInstruction::Define {
                    result,
                    operation: crate::IrOperation::Load { address, .. },
                    ..
                } if result == scrutinee => Some(*address),
                _ => None,
            })
            .unwrap_or(*scrutinee);
        for (instruction, defined) in header.instructions().iter().enumerate() {
            let IrInstruction::Define {
                result,
                ty: IrType::Address(referent),
                operation:
                    crate::IrOperation::ProjectAddress {
                        address,
                        projection:
                            crate::IrPlaceStep::RunElement {
                                offset,
                                target_domain,
                            },
                    },
            } = defined
            else {
                continue;
            };
            if *result == matched
                && role(*result) == Some(Role::HeaderValue)
                && role(*offset) == Some(Role::Carried)
                && matches!(role(*address), Some(Role::Hoisted | Role::Invariant))
                && self.run_slots_follow_address(*address)?
            {
                return Ok(Some(Cursor {
                    place: *result,
                    index: *offset,
                    run: *address,
                    instruction,
                    element: referent.ty(),
                    target_domain: *target_domain,
                }));
            }
        }
        Ok(None)
    }

    /// The matched element's address for the index an edge into the header
    /// gives it. Entering the loop, it is the run's element at that index.
    /// On an edge back to the header, it is the address this part received
    /// moved by the index's change, which the edge's own index bound keeps
    /// inside the run and which the host folds to one addition when the
    /// index changes by a constant.
    fn edge_cursor(
        &mut self,
        cursor: Cursor,
        carried: &[(IrValueId, IrValueId)],
        tail: bool,
    ) -> Result<String, BackendFailure> {
        let argument = carried
            .iter()
            .find(|(parameter, _)| *parameter == cursor.index)
            .map(|(_, argument)| *argument)
            .ok_or(BackendFailure::InvalidIr)?;
        if !tail {
            return self.run_element_place(
                cursor.run,
                argument,
                cursor.element,
                cursor.target_domain,
            );
        }
        if argument == cursor.index {
            return Ok(self.value_name(cursor.place));
        }
        let step = self.next_temporary()?;
        let moved = self.next_temporary()?;
        let element = self.output.type_name(self.program, cursor.element)?;
        let scaled = self
            .element_address_index(cursor.element, &format!("%{step}"))?
            .to_owned();
        writeln!(
            self.output,
            "  %{step} = sub i64 {}, {}\n  %{moved} = getelementptr {element}, ptr {}, i64 {scaled}",
            self.value_name(argument),
            self.value_name(cursor.index),
            self.value_name(cursor.place)
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        Ok(format!("%{moved}"))
    }

    /// Counts each value's reading arms after projection and call-argument
    /// replacement; a read in the emitted header counts as every arm.
    /// Includes terminator operands (and thus carried edge arguments), but
    /// excludes hoisted header work and the cursor's skipped instruction.
    /// Pin prelude stores keep their canonical values live separately: only
    /// an arm that hands the pin to a callee counts as a reader here.
    fn arms_reading(
        &self,
        plan: &DispatchLoop,
        invariant: &LoopInvariants,
        cursor: Option<Cursor>,
    ) -> HashMap<IrValueId, usize> {
        let canonical = |value: IrValueId| invariant.replaced.get(&value).copied().unwrap_or(value);
        let pins: HashMap<IrValueId, IrValueId> = invariant.pins.iter().copied().collect();
        let mut blocks_reading = vec![HashSet::new(); self.function.blocks().len()];
        for (index, block) in self.function.blocks().iter().enumerate() {
            let header = index == plan.header.index();
            if !header && !plan.region[index] {
                continue;
            }
            let reads = &mut blocks_reading[index];
            for (position, instruction) in block.instructions().iter().enumerate() {
                if header
                    && (invariant.hoisted.contains(&position)
                        || cursor.is_some_and(|cursor| cursor.instruction == position))
                {
                    continue;
                }
                if let IrInstruction::Define { result, .. } = instruction
                    && let Some(replacement) = invariant.replaced.get(result)
                {
                    // emit_part_instruction emits only an alias of the
                    // canonical projection, or nothing for its definition.
                    if replacement != result {
                        reads.insert(*replacement);
                    }
                    continue;
                }
                reads.extend(instruction.operands().into_iter().map(canonical));
                if let IrInstruction::Define {
                    operation: crate::IrOperation::Call { arguments, .. },
                    ..
                } = instruction
                {
                    reads.extend(arguments.iter().filter_map(|argument| {
                        invariant
                            .pin_arguments
                            .get(argument)
                            .and_then(|parameter| pins.get(parameter))
                            .copied()
                    }));
                }
            }
            reads.extend(block.terminator().operands().into_iter().map(canonical));
        }
        let mut readers = HashMap::new();
        for (_, blocks) in &plan.arms {
            let reads: HashSet<IrValueId> = blocks_reading
                .iter()
                .enumerate()
                .filter(|(index, _)| blocks[*index])
                .flat_map(|(_, reads)| reads.iter().copied())
                .collect();
            for value in reads {
                *readers.entry(value).or_default() += 1;
            }
        }
        for value in &blocks_reading[plan.header.index()] {
            readers.insert(*value, plan.arms.len());
        }
        readers
    }

    fn set_part(&mut self, part: Part) {
        if let Some(dispatch) = &mut self.dispatch {
            dispatch.part = part;
        }
    }
}

/// The part-local slot holding the kept box of a pinned reference, which the
/// part hands to callees in its place.
fn pin_slot_name(parameter: IrValueId) -> String {
    format!("%wf.pin.{}", parameter.ordinal())
}
