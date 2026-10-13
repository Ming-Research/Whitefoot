//! Internal call entries: forms of a definition that only calls the
//! compiler itself emits can name (compiler/recursive-environment).
//!
//! Every definition keeps its public symbol in the one callable ABI that
//! linked C bodies share, and every caller the compiler cannot see, C or
//! another fragment's code built without this plan, names that symbol. An
//! internal entry is described by its form, [`EntryForm`], and is named by
//! [`internal_entry_symbol`]; a call site chooses it in one place,
//! [`FunctionEmitter::internal_call`], by the relation between caller and
//! callee, and otherwise names the public symbol.
//!
//! The one form here is the shared environment of a recursive group. A group
//! is a strongly connected component of the call graph among the functions
//! that take part: definitions with a checked source signature that do not
//! wait, are not synthesized, have no overlap groups, belong to no recursion
//! budget family and contain no dispatch loop a split could take. Linked
//! declarations have no body to give an internal form, a waiting function is
//! entered by a frame transfer [WAIT-1], and a dispatch part's arguments are
//! the registers its tail calls carry, so none of them takes part.
//!
//! A slot of the environment is one read-only borrowed parameter (a `&T`
//! reference or a `&[T]` range whose declared effects never write through
//! it) that every member has and that every call inside the group passes on
//! unchanged: at each such call the argument is the caller's own parameter
//! of the same slot, directly or through block parameters that only ever
//! receive it. A recursive edge that passes a different borrow, such as a
//! narrowed range, keeps that parameter out of the record, and so does a
//! member that lacks it. A parameter written through keeps its own `noalias`
//! argument, which a pointer loaded from the record cannot carry and which
//! is what separates its writes from every other access, so it stays out.
//!
//! Inside the group a member's body is defined under its internal symbol and
//! receives one pointer to the record in place of its slot parameters,
//! loading each slot once at entry. Its public symbol becomes the group's
//! entry from outside: it stores its slot parameters into a record in its
//! own frame and calls the internal form, which the host can inline into
//! each outside caller. A group uses the record only when it pays: the slots
//! must replace at least two argument words with the one pointer, and some
//! member must pass more argument words than the selected target's integer
//! argument registers, since below that budget every argument already
//! travels in a register and loading from the record could only add work.

use super::*;
use crate::cycles::components;

/// The form of an internal entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum EntryForm {
    /// The member of a recursive group that receives the group's shared
    /// environment record in place of its slot parameters.
    Environment,
}

impl EntryForm {
    const fn tag(self) -> &'static str {
        match self {
            Self::Environment => "env",
        }
    }
}

/// The symbol of the internal entry of the function `name` in `form`.
///
/// It is compiler-owned: a `wf__` symbol holds a spelling no source symbol
/// has [FORM-3], and each form's tag gives it a prefix no other
/// compiler-owned symbol begins with.
pub(super) fn internal_entry_symbol(form: EntryForm, name: &str) -> String {
    format!("wf__{}_{name}", form.tag())
}

/// The operand an internal body receives its environment record through.
pub(super) const ENVIRONMENT: &str = "%wf.env";

/// The internal entry one call names in place of its callee's public symbol.
pub(super) struct InternalCall {
    pub(super) symbol: String,
    /// The calling convention keyword with its trailing space, or empty for
    /// the C convention.
    pub(super) convention: &'static str,
    /// For each of the callee's parameters, whether the caller passes it
    /// inside the environment record rather than as itself.
    pub(super) packed: Vec<bool>,
}

/// How one borrowed parameter lies in the record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SlotKind {
    Reference,
    Range,
    Run,
}

impl SlotKind {
    const fn of(ty: IrType) -> Option<Self> {
        match ty {
            IrType::Address(_) => Some(Self::Reference),
            IrType::Range { .. } => Some(Self::Range),
            IrType::Run { .. } => Some(Self::Run),
            _ => None,
        }
    }

    /// The record's fields for one slot: exactly the words the parameter
    /// crosses an ordinary call as (see [`super::super::abi`]).
    const fn fields(self) -> &'static [&'static str] {
        match self {
            Self::Reference => &["ptr"],
            Self::Range => &["ptr", "i64"],
            Self::Run => &["ptr", "i64", "i64"],
        }
    }

    const fn mode(self) -> crate::IrSourceMode {
        match self {
            Self::Reference => crate::IrSourceMode::Reference,
            Self::Range | Self::Run => crate::IrSourceMode::Range,
        }
    }

    /// The operands the parameter `value` arrives as in an ordinary
    /// definition, one per field, which the body reads by these names.
    fn operands(self, value: IrValueId) -> Vec<String> {
        let (pointer, count) = incoming_range_parts(value);
        match self {
            Self::Reference => vec![value_name(value)],
            Self::Range => vec![pointer, count],
            Self::Run => vec![pointer, format!("%wf.arg.v{}.lo", value.ordinal()), count],
        }
    }
}

/// One member's place in its group.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Member {
    group: usize,
    /// For each parameter, its slot in the group's record, or `None` for a
    /// parameter passed as itself.
    parameter_slots: Vec<Option<usize>>,
}

/// The module's recursive groups that pass a shared environment record.
#[derive(Debug, Default)]
pub(super) struct EnvironmentPlan {
    /// Each group's slots in record order.
    groups: Vec<Vec<SlotKind>>,
    members: HashMap<u32, Member>,
}

impl EnvironmentPlan {
    pub(super) fn new(
        program: &IrProgram,
        target: TargetLayout,
        frontiers: &RecursiveFrontiers,
    ) -> Result<Self, BackendFailure> {
        let functions = program.functions();
        let mut taking_part = Vec::with_capacity(functions.len());
        for (ordinal, function) in functions.iter().enumerate() {
            taking_part.push(takes_part(function, ordinal, frontiers)?);
        }
        let mut edges = vec![Vec::new(); functions.len()];
        for (caller, function) in functions.iter().enumerate() {
            if !taking_part[caller] {
                continue;
            }
            for (callee, _) in calls(function) {
                let callee = callee as usize;
                if taking_part.get(callee) == Some(&true) && !edges[caller].contains(&callee) {
                    edges[caller].push(callee);
                }
            }
        }
        let mut plan = Self::default();
        for mut component in components(&edges) {
            component.sort_unstable();
            let recursive = component.len() > 1
                || component
                    .first()
                    .is_some_and(|only| edges[*only].contains(only));
            if recursive {
                plan.plan_group(program, target, &component)?;
            }
        }
        Ok(plan)
    }

    /// The place of the `ordinal`th function in a group, if it has one.
    pub(super) fn member(&self, ordinal: u32) -> Option<&Member> {
        self.members.get(&ordinal)
    }

    fn slots(&self, group: usize) -> Result<&[SlotKind], BackendFailure> {
        self.groups
            .get(group)
            .map(Vec::as_slice)
            .ok_or(BackendFailure::InvalidIr)
    }

    /// The record's LLVM type and the first field of each slot.
    fn record(&self, group: usize) -> Result<(String, Vec<usize>, usize), BackendFailure> {
        let mut fields = Vec::new();
        let mut starts = Vec::new();
        for kind in self.slots(group)? {
            starts.push(fields.len());
            fields.extend_from_slice(kind.fields());
        }
        Ok((format!("{{ {} }}", fields.join(", ")), starts, fields.len()))
    }

    fn plan_group(
        &mut self,
        program: &IrProgram,
        target: TargetLayout,
        members: &[usize],
    ) -> Result<(), BackendFailure> {
        let functions = program.functions();
        let position: HashMap<usize, usize> = members
            .iter()
            .enumerate()
            .map(|(position, ordinal)| (*ordinal, position))
            .collect();
        // Every candidate parameter, keyed by (member position, index).
        let mut keys = Vec::new();
        let mut key_of = HashMap::new();
        for (member, &ordinal) in members.iter().enumerate() {
            let function = &functions[ordinal];
            for (index, (value, ty)) in function.parameters().iter().enumerate() {
                if is_candidate(function, index, *value, *ty) {
                    key_of.insert((member, index), keys.len());
                    keys.push((member, index));
                }
            }
        }
        // Each call inside the group joins the callee's candidate to the
        // caller's candidate it passes unchanged, or rules the callee's out.
        let mut parent: Vec<usize> = (0..keys.len()).collect();
        let mut excluded = vec![false; keys.len()];
        for (member, &ordinal) in members.iter().enumerate() {
            let caller = &functions[ordinal];
            let origins = formal_origins(caller);
            for (callee, arguments) in calls(caller) {
                let Some(&callee_member) = position.get(&(callee as usize)) else {
                    continue;
                };
                let count = functions[callee as usize].parameters().len();
                for index in 0..count {
                    let Some(&key) = key_of.get(&(callee_member, index)) else {
                        continue;
                    };
                    let passed = arguments
                        .get(index)
                        .and_then(|argument| origins.get(argument))
                        .and_then(|formal| key_of.get(&(member, *formal)));
                    match passed {
                        Some(&source) if arguments.len() == count => {
                            union(&mut parent, key, source);
                        }
                        _ => excluded[key] = true,
                    }
                }
            }
        }
        let mut classes: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for key in 0..keys.len() {
            let root = find(&mut parent, key);
            classes.entry(root).or_default().push(key);
        }
        // A slot is a class holding exactly one parameter of every member,
        // all of one type, none ruled out. Record order follows the first
        // member's parameter order.
        let mut slots: Vec<(usize, SlotKind, Vec<(usize, usize)>)> = Vec::new();
        for class in classes.values() {
            if class.len() != members.len() || class.iter().any(|key| excluded[*key]) {
                continue;
            }
            let mut seen = vec![false; members.len()];
            let mut ty = None;
            let mut sound = true;
            let mut first = None;
            for &key in class {
                let (member, index) = keys[key];
                if std::mem::replace(&mut seen[member], true) {
                    sound = false;
                }
                let parameter = functions[members[member]].parameters()[index].1;
                if *ty.get_or_insert(parameter) != parameter {
                    sound = false;
                }
                if member == 0 {
                    first = Some(index);
                }
            }
            let (Some(first), Some(kind)) = (first, ty.and_then(SlotKind::of)) else {
                continue;
            };
            if sound {
                slots.push((first, kind, class.iter().map(|key| keys[*key]).collect()));
            }
        }
        slots.sort_by_key(|(first, ..)| *first);
        // Profitability: the record replaces at least two words with one
        // pointer, and some member passes more words than the registers hold.
        let packed: usize = slots.iter().map(|(_, kind, _)| kind.fields().len()).sum();
        if packed < 2 {
            return Ok(());
        }
        let budget = target.integer_argument_registers();
        let mut exceeds = false;
        for &ordinal in members {
            exceeds |= argument_words(program, target, &functions[ordinal])? > budget;
        }
        if !exceeds {
            return Ok(());
        }
        let group = self.groups.len();
        self.groups
            .push(slots.iter().map(|(_, kind, _)| *kind).collect());
        for (member, &ordinal) in members.iter().enumerate() {
            let mut parameter_slots = vec![None; functions[ordinal].parameters().len()];
            for (slot, (_, _, keys)) in slots.iter().enumerate() {
                for &(owner, index) in keys {
                    if owner == member {
                        parameter_slots[index] = Some(slot);
                    }
                }
            }
            self.members.insert(
                u32::try_from(ordinal).map_err(|_| BackendFailure::CounterOverflow)?,
                Member {
                    group,
                    parameter_slots,
                },
            );
        }
        Ok(())
    }
}

/// Whether the function may belong to a group (see the module comment).
fn takes_part(
    function: &IrFunction,
    ordinal: usize,
    frontiers: &RecursiveFrontiers,
) -> Result<bool, BackendFailure> {
    if function.blocks().is_empty()
        || function.waits()
        || function.synthesis().is_some()
        || function.source_signature().is_none()
        || !function.overlaps().is_empty()
        || frontiers.grain(ordinal).is_some()
    {
        return Ok(false);
    }
    let reachable = dispatch::reachable(function)?;
    Ok(dispatch::find(function, &reachable).0.is_none())
}

/// Whether the `index`th parameter can be a slot: a read-only `&T` or
/// `&[T]` borrow by its checked source mode.
fn is_candidate(function: &IrFunction, index: usize, value: IrValueId, ty: IrType) -> bool {
    let Some(kind) = SlotKind::of(ty) else {
        return false;
    };
    function
        .source_signature()
        .and_then(|signature| signature.parameters().get(index).copied())
        == Some(kind.mode())
        && function.readonly_reference_parameters.contains(&value)
}

/// Every ordinary call the function makes, with its arguments.
fn calls(function: &IrFunction) -> impl Iterator<Item = (u32, &[IrValueId])> {
    function
        .blocks()
        .iter()
        .flat_map(IrBlock::instructions)
        .filter_map(|instruction| match instruction {
            IrInstruction::Define {
                operation:
                    IrOperation::Call {
                        function,
                        arguments,
                    },
                ..
            } => Some((*function, arguments.as_slice())),
            _ => None,
        })
}

/// The values that are one of the function's own parameters unchanged, and
/// that parameter's index: the parameters themselves, and each block
/// parameter every jump into whose block passes the same one of them.
///
/// A block parameter starts with no known incoming value and settles at a
/// parameter index or at none; a loop's parameter that receives a formal on
/// entry and itself on the back edge settles at that formal.
fn formal_origins(function: &IrFunction) -> HashMap<IrValueId, usize> {
    let formals: HashMap<IrValueId, usize> = function
        .parameters()
        .iter()
        .enumerate()
        .map(|(index, (value, _))| (*value, index))
        .collect();
    let block_parameters: HashSet<IrValueId> = function
        .blocks()
        .iter()
        .flat_map(|block| block.parameters().iter().map(|(value, _)| *value))
        .collect();
    let mut state: HashMap<IrValueId, Option<usize>> = HashMap::new();
    loop {
        let mut changed = false;
        for block in function.blocks() {
            let IrTerminator::Jump {
                target, arguments, ..
            } = block.terminator()
            else {
                continue;
            };
            let Some(target) = function.blocks().get(target.index()) else {
                continue;
            };
            for ((parameter, _), argument) in target.parameters().iter().zip(arguments) {
                let incoming = if let Some(formal) = formals.get(argument) {
                    Some(*formal)
                } else if block_parameters.contains(argument) {
                    match state.get(argument) {
                        Some(known) => *known,
                        None => continue,
                    }
                } else {
                    None
                };
                let next = match state.get(parameter) {
                    None => incoming,
                    Some(current) if *current == incoming => incoming,
                    Some(_) => None,
                };
                if state.insert(*parameter, next) != Some(next) {
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    let mut origins = formals;
    for (value, formal) in state {
        if let Some(formal) = formal {
            origins.insert(value, formal);
        }
    }
    origins
}

/// The integer-class words an ordinary call to the function passes: the
/// destination pointer, one pointer per indirect parameter, a range's two
/// words and a run's three, nothing for a floating or unit value, which
/// travel elsewhere or not at all, and the eight-byte words of any other
/// value's storage.
fn argument_words(
    program: &IrProgram,
    target: TargetLayout,
    function: &IrFunction,
) -> Result<u64, BackendFailure> {
    let abi = FunctionAbi::build(program, function)?;
    let mut words = u64::from(abi.result().uses_destination());
    for parameter in abi.parameters() {
        words += if parameter.is_indirect() {
            1
        } else {
            match parameter.ty() {
                IrType::Unit | IrType::Float { .. } => 0,
                IrType::Address(_) => 1,
                IrType::Range { .. } => 2,
                IrType::Run { .. } => 3,
                ty => crate::target::validate_static_storage(
                    target,
                    program,
                    &crate::target::TargetStorageType::source(ty),
                )
                .map_or(1, |layout| layout.size().div_ceil(8).max(1)),
            }
        };
    }
    Ok(words)
}

fn find(parent: &mut [usize], key: usize) -> usize {
    let mut root = key;
    while parent[root] != root {
        root = parent[root];
    }
    let mut node = key;
    while parent[node] != root {
        let next = parent[node];
        parent[node] = root;
        node = next;
    }
    root
}

fn union(parent: &mut [usize], left: usize, right: usize) {
    let left = find(parent, left);
    let right = find(parent, right);
    if left != right {
        parent[left.max(right)] = left.min(right);
    }
}

impl FunctionEmitter<'_, '_> {
    /// The internal entry a call to the `function`th function names where it
    /// would otherwise name `symbol`, or `None` for the public symbol.
    ///
    /// This is the one place a call site chooses an internal entry. A call
    /// from a member of a recursive group to a member of the same group, in
    /// the ordinary world, names the callee's environment form and passes
    /// the caller's own record.
    pub(super) fn internal_call(&self, function: u32, symbol: &str) -> Option<InternalCall> {
        let own = self.environment.as_ref()?;
        let callee = self.environments.member(function)?;
        let target = self.program.functions().get(function as usize)?;
        if callee.group != own.group || symbol != source_symbol(target.name()) {
            return None;
        }
        Some(InternalCall {
            symbol: internal_entry_symbol(EntryForm::Environment, target.name()),
            convention: "",
            packed: callee.parameter_slots.iter().map(Option::is_some).collect(),
        })
    }

    fn own_environment(&self) -> Result<&Member, BackendFailure> {
        self.environment.as_ref().ok_or(BackendFailure::InvalidIr)
    }

    /// The record pointer's declaration: the record lives in the outside
    /// caller's frame for the whole call, nothing writes it after the entry
    /// filled it, and no member keeps the pointer past its return.
    fn environment_parameter(&self) -> Result<Parameter, BackendFailure> {
        let (_, _, fields) = self.environments.record(self.own_environment()?.group)?;
        Ok(Parameter::named(
            format!(
                "ptr noalias readonly nonnull {} align 8 dereferenceable({})",
                crate::toolchain::facts().no_capture_attribute,
                fields * 8
            ),
            ENVIRONMENT,
        ))
    }

    /// The parameters of this member's internal form: the record pointer,
    /// then every parameter outside the record with its facts, without the
    /// destination pointer.
    pub(super) fn internal_parameters(
        &self,
        abi: &FunctionAbi,
    ) -> Result<(Vec<Parameter>, References), BackendFailure> {
        let own = self.own_environment()?;
        let mut parameters = vec![self.environment_parameter()?];
        let mut references = References::default();
        for (index, (((value, _), parameter), slot)) in self
            .function
            .parameters()
            .iter()
            .zip(abi.parameters())
            .zip(&own.parameter_slots)
            .enumerate()
        {
            if slot.is_some() {
                continue;
            }
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

    /// The arguments a forward to this member's internal form passes, in the
    /// forwarding function's own parameters and record.
    pub(super) fn internal_call_arguments(
        &self,
        abi: &FunctionAbi,
    ) -> Result<String, BackendFailure> {
        let own = self.own_environment()?;
        let mut arguments = Vec::new();
        if abi.result().uses_destination() {
            arguments.push(format!("ptr {RESULT_POINTER}"));
        }
        arguments.push(format!("ptr {ENVIRONMENT}"));
        for (((value, _), parameter), slot) in self
            .function
            .parameters()
            .iter()
            .zip(abi.parameters())
            .zip(&own.parameter_slots)
        {
            if slot.is_none() {
                arguments.push(incoming_parameter(self.program, *value, *parameter, "")?);
            }
        }
        Ok(arguments.join(", "))
    }

    /// The fields of each slot parameter of this member, with the record's
    /// type: `(parameter value, kind, first field)`.
    fn slot_parameters(
        &self,
    ) -> Result<(String, Vec<(IrValueId, SlotKind, usize)>), BackendFailure> {
        let own = self.own_environment()?;
        let (record, starts, _) = self.environments.record(own.group)?;
        let kinds = self.environments.slots(own.group)?;
        let mut found = Vec::new();
        for ((value, _), slot) in self.function.parameters().iter().zip(&own.parameter_slots) {
            if let Some(slot) = slot {
                let kind = *kinds.get(*slot).ok_or(BackendFailure::InvalidIr)?;
                let start = *starts.get(*slot).ok_or(BackendFailure::InvalidIr)?;
                found.push((*value, kind, start));
            }
        }
        Ok((record, found))
    }

    /// The entry-block loads that give an internal body its slot parameters
    /// under the names an ordinary body receives them by, or nothing for a
    /// function outside every group.
    ///
    /// The loaded pointer carries what the parameter's attributes would
    /// have said and a load can say: it is nonnull, and a reference is
    /// dereferenceable for its referent's extent. `noalias` has no load
    /// form, which is why only read-only borrows enter the record.
    pub(super) fn environment_loads(&self) -> Result<String, BackendFailure> {
        if self.environment.is_none() {
            return Ok(String::new());
        }
        let (record, slots) = self.slot_parameters()?;
        let mut text = String::new();
        for (value, kind, start) in slots {
            for (field, (operand, ty)) in kind.operands(value).iter().zip(kind.fields()).enumerate()
            {
                let address = format!("%wf.env.v{}.{field}", value.ordinal());
                writeln!(
                    text,
                    "  {address} = getelementptr inbounds {record}, ptr {ENVIRONMENT}, i32 0, i32 {}",
                    start + field
                )?;
                let mut facts = String::new();
                if field == 0 {
                    facts.push_str(", !nonnull !{}");
                    if let Some(IrType::Address(referent)) = self.value_type(value)
                        && let Ok(layout) = crate::target::validate_static_storage(
                            self.target,
                            self.program,
                            &crate::target::TargetStorageType::source(referent.ty()),
                        )
                        && layout.size() > 0
                    {
                        write!(facts, ", !dereferenceable !{{i64 {}}}", layout.size())?;
                    }
                }
                writeln!(
                    text,
                    "  {operand} = load {ty}, ptr {address}, align 8, !noundef !{{}}{facts}"
                )?;
            }
        }
        Ok(text)
    }

    /// This member's public symbol as the group's entry from outside: the
    /// ordinary signature `public` with its facts, which stores the slot
    /// parameters into a record in its own frame and calls the internal
    /// form.
    pub(super) fn environment_entry(
        &self,
        symbol: &str,
        public: &FunctionAbi,
    ) -> Result<Module, BackendFailure> {
        let (mut parameters, mut references) = self.signature_parameters(public)?;
        let destination = public.result().uses_destination();
        let result = if destination {
            "void".to_owned()
        } else {
            llvm_type_with_references(self.program, public.result().ty(), &mut references.types)?
        };
        if destination {
            parameters.insert(0, Parameter::named("ptr", RESULT_POINTER));
        }
        let (record, slots) = self.slot_parameters()?;
        let internal = internal_entry_symbol(EntryForm::Environment, self.function.name());
        let arguments = self.internal_call_arguments(public)?;
        let mut output = FunctionBody::default();
        output.open_block("entry".to_owned());
        writeln!(output, "  {ENVIRONMENT} = alloca {record}, align 8")?;
        for (value, kind, start) in slots {
            for (field, (operand, ty)) in kind.operands(value).iter().zip(kind.fields()).enumerate()
            {
                let address = format!("%wf.env.v{}.{field}", value.ordinal());
                writeln!(
                    output,
                    "  {address} = getelementptr inbounds {record}, ptr {ENVIRONMENT}, i32 0, i32 {}\n  \
                     store {ty} {operand}, ptr {address}, align 8",
                    start + field
                )?;
            }
        }
        output.symbol(internal.clone());
        if destination {
            write!(output, "  call void @{internal}({arguments})\n  ret void\n")?;
        } else {
            write!(
                output,
                "  %wf.entry = call {result} @{internal}({arguments})\n  ret {result} %wf.entry\n"
            )?;
        }
        let mut signature = Signature::new(symbol, result, parameters);
        signature.references = references;
        let mut module = Module::default();
        module.define(signature.define(output, "")?);
        module.text("\n");
        Ok(module)
    }
}
