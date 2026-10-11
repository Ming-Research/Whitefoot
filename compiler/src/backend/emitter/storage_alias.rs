//! Exact source types and paths select storage accesses [TYPE-5, OP-4,
//! REF-1, STOR-1]; primitive reinterpretation does not cast references
//! [OP-6, OP-8]. Variant replacement invalidates payload references [REF-2].
//! Runtime descriptors are Box-only [TYPE-9], and their words are readonly
//! to source stores [TYPE-2, TYPE-10]. Constant-capacity descriptors
//! can be nested inside an element, so they receive no control-word tag.
//!
//! Numeric reads remain untagged: a readonly reference can read a measure.
//! Source constructors, reused result slots, whole-value copies and transfer
//! granules remain untagged clobbers across storage and variant transitions.

use super::*;

#[derive(Clone, Copy)]
pub(super) enum Role {
    ScalarWrite,
    BoxOwner,
    ArrayLen,
    SlotsLen,
    RingLen,
    Capacity,
    RingHead,
    PagedLen,
    PagedDirectory,
    PagedDirectoryCapacity,
}

impl Role {
    pub(super) const fn attachment(self) -> &'static str {
        match self {
            Self::ScalarWrite => ", !tbaa !12",
            Self::BoxOwner => ", !tbaa !13",
            Self::ArrayLen => ", !tbaa !14",
            Self::SlotsLen => ", !tbaa !15",
            Self::RingLen => ", !tbaa !16",
            Self::Capacity => ", !tbaa !17",
            Self::RingHead => ", !tbaa !18",
            Self::PagedLen => ", !tbaa !19",
            Self::PagedDirectory => ", !tbaa !20",
            Self::PagedDirectoryCapacity => ", !tbaa !21",
        }
    }
}

/// This is the module's only numbered metadata namespace. Keeping it in
/// the existing header also carries the definitions into cached fragments.
pub(super) fn emit_definitions(module: &mut Module) {
    module.header("!0 = !{!\"Whitefoot checked storage roles\"}".to_owned());
    module.header("!1 = !{!\"any memory\", !0, i64 0}".to_owned());
    for (index, name) in [
        "source scalar write",
        "Box owner",
        "runtime Array length",
        "runtime Slots length",
        "runtime Ring length",
        "runtime window capacity",
        "runtime Ring head",
        "Paged length",
        "Paged directory",
        "Paged directory capacity",
    ]
    .into_iter()
    .enumerate()
    {
        let class = index + 2;
        let tag = index + 12;
        module.header(format!("!{class} = !{{!\"{name}\", !1, i64 0}}"));
        module.header(format!("!{tag} = !{{!{class}, !{class}, i64 0}}"));
    }
}

pub(super) fn source_read(program: &IrProgram, ty: IrType) -> &'static str {
    match ty {
        IrType::Nominal(id)
            if program
                .nominal(id)
                .is_some_and(|nominal| matches!(nominal.kind(), IrNominalKind::Box { .. })) =>
        {
            Role::BoxOwner.attachment()
        }
        _ => "",
    }
}

pub(super) fn source_write(program: &IrProgram, ty: IrType) -> &'static str {
    match ty {
        IrType::Integer { .. } | IrType::Float { .. } => Role::ScalarWrite.attachment(),
        _ => source_read(program, ty),
    }
}

pub(super) fn runtime_word(ty: IrType, field: u32) -> &'static str {
    let role = match (ty, field) {
        (IrType::Buffer { .. }, 0) => Role::ArrayLen,
        (
            IrType::Window {
                shape,
                capacity: None,
                ..
            },
            0,
        ) => match shape {
            IrWindowShape::Slots => Role::SlotsLen,
            IrWindowShape::Ring => Role::RingLen,
            IrWindowShape::Paged => Role::PagedLen,
        },
        (IrType::Window { capacity: None, .. }, 1) => Role::Capacity,
        (
            IrType::Window {
                shape: IrWindowShape::Ring,
                capacity: None,
                ..
            },
            2,
        ) => Role::RingHead,
        (
            IrType::Window {
                shape: IrWindowShape::Paged,
                capacity: None,
                ..
            },
            2,
        ) => Role::PagedDirectoryCapacity,
        _ => return "",
    };
    role.attachment()
}

/// A scope requires a readonly formal with a bare-root declared read; an
/// exclusion requires a different formal with a bare-root declared write.
/// EFF-1 covers every physical descendant word in both sets, so EFF-5 rejects
/// sharing for each load/store pair. Narrow rows need not cover implicit
/// accesses such as a range's preliminary full-length load. Read/read sharing
/// (including the shared empty header) remains possible. Result destinations
/// and by-value inputs make no such promise.
#[derive(Default)]
pub(super) struct OwnerAliases {
    roots: HashMap<IrValueId, BTreeSet<usize>>,
    reads: BTreeMap<usize, usize>,
    writes: Option<usize>,
    writing_formals: BTreeSet<usize>,
}

fn readonly_formals(function: &IrFunction) -> Vec<usize> {
    if function.waits() || !function.overlaps().is_empty() || aliasing_admitted_row(function.name())
    {
        return Vec::new();
    }
    let Some(signature) = function.source_signature() else {
        return Vec::new();
    };
    if !function
        .parameters()
        .iter()
        .enumerate()
        .any(|(index, (value, ty))| {
            matches!(ty, IrType::Address(_))
                && signature.parameters().get(index) == Some(&crate::IrSourceMode::Reference)
                && function.whole_write_reference_parameters.contains(value)
        })
    {
        return Vec::new();
    }
    function
        .parameters()
        .iter()
        .enumerate()
        .filter_map(|(index, (value, ty))| {
            (matches!(ty, IrType::Address(_))
                && signature.parameters().get(index) == Some(&crate::IrSourceMode::Reference)
                && function.readonly_reference_parameters.contains(value)
                && function.whole_read_reference_parameters.contains(value))
            .then_some(index)
        })
        .collect()
}

/// Each readonly formal's index and its singleton scope-list metadata ID.
type FormalScopeLists = Vec<(usize, usize)>;

/// Numbered metadata is one module-wide namespace, shared with the fixed TBAA
/// definitions above. Headers accompany every fragment and cached roundtrip.
pub(super) struct OwnerScopePlan {
    definitions: Vec<(usize, FormalScopeLists, usize)>,
    functions: BTreeMap<String, (BTreeMap<usize, usize>, usize)>,
}

impl OwnerScopePlan {
    pub(super) fn new(program: &IrProgram) -> Self {
        let mut plan = Self {
            definitions: Vec::new(),
            functions: BTreeMap::new(),
        };
        let mut next = 22;
        for function in program.functions() {
            let readonly = readonly_formals(function);
            if readonly.is_empty() {
                continue;
            }
            let domain = next;
            next += 1;
            let reads: Vec<_> = readonly
                .into_iter()
                .map(|formal| {
                    let list = next + 1;
                    next += 2;
                    (formal, list)
                })
                .collect();
            let writes = next;
            next += 1;
            plan.functions.insert(
                function.name().to_owned(),
                (reads.iter().copied().collect(), writes),
            );
            plan.definitions.push((domain, reads, writes));
        }
        plan
    }

    pub(super) fn emit_definitions(&self, module: &mut Module) {
        for (domain, reads, writes) in &self.definitions {
            module.header(format!("!{domain} = distinct !{{!{domain}}}"));
            for (_, list) in reads {
                let scope = list - 1;
                module.header(format!("!{scope} = distinct !{{!{scope}, !{domain}}}"));
                module.header(format!("!{list} = !{{!{scope}}}"));
            }
            let scopes = reads
                .iter()
                .map(|(_, list)| format!("!{}", list - 1))
                .collect::<Vec<_>>()
                .join(", ");
            module.header(format!("!{writes} = !{{{scopes}}}"));
        }
    }
}

impl OwnerAliases {
    pub(super) fn build(
        plan: &OwnerScopePlan,
        function: &IrFunction,
        incoming: &[Vec<Incoming>],
    ) -> Self {
        let Some((reads, writes)) = plan.functions.get(function.name()) else {
            return Self::default();
        };
        let mut result = Self {
            reads: reads.clone(),
            writes: Some(*writes),
            writing_formals: function
                .parameters()
                .iter()
                .enumerate()
                .filter_map(|(index, (value, _))| {
                    function
                        .whole_write_reference_parameters
                        .contains(value)
                        .then_some(index)
                })
                .collect(),
            ..Self::default()
        };
        // Unknown provenance is an explicit member, so a phi cannot acquire a
        // formal's promise by silently dropping an unknown incoming edge.
        const UNKNOWN: usize = usize::MAX;
        for (index, (value, ty)) in function.parameters().iter().enumerate() {
            let root = if matches!(ty, IrType::Address(_))
                && function
                    .source_signature()
                    .and_then(|s| s.parameters().get(index))
                    == Some(&crate::IrSourceMode::Reference)
            {
                index
            } else {
                UNKNOWN
            };
            result.roots.insert(*value, BTreeSet::from([root]));
        }
        for block in function.blocks() {
            for instruction in block.instructions() {
                if let IrInstruction::Define {
                    result: value,
                    operation,
                    ..
                } = instruction
                    && !matches!(operation, IrOperation::ProjectAddress { .. })
                {
                    result.roots.insert(*value, BTreeSet::from([UNKNOWN]));
                }
            }
        }
        loop {
            let mut changed = false;
            for (block_index, block) in function.blocks().iter().enumerate() {
                for (position, (value, _)) in block.parameters().iter().enumerate() {
                    let roots: BTreeSet<_> = incoming[block_index]
                        .iter()
                        .filter_map(|edge| edge.arguments.get(position))
                        .flat_map(|argument| {
                            result.roots.get(argument).into_iter().flatten().copied()
                        })
                        .collect();
                    let before = result.roots.entry(*value).or_default().len();
                    result.roots.entry(*value).or_default().extend(roots);
                    changed |= result.roots[value].len() != before;
                }
                for instruction in block.instructions() {
                    if let IrInstruction::Define {
                        result: value,
                        operation: IrOperation::ProjectAddress { address, .. },
                        ..
                    } = instruction
                    {
                        let roots = result.roots.get(address).cloned().unwrap_or_default();
                        let before = result.roots.entry(*value).or_default().len();
                        result.roots.entry(*value).or_default().extend(roots);
                        changed |= result.roots[value].len() != before;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        result
    }

    pub(super) fn scopes(&self) -> impl Iterator<Item = usize> + '_ {
        self.reads.values().copied()
    }

    fn root(&self, value: IrValueId) -> Option<usize> {
        let roots = self.roots.get(&value)?;
        (roots.len() == 1).then(|| *roots.first().expect("one root"))
    }

    pub(super) fn read(&self, value: IrValueId) -> String {
        self.root(value)
            .and_then(|root| self.reads.get(&root))
            .map_or_else(String::new, |scope| format!(", !alias.scope !{scope}"))
    }

    pub(super) fn write(&self, value: IrValueId) -> String {
        match self.root(value) {
            Some(root) if self.writing_formals.contains(&root) => self
                .writes
                .map_or_else(String::new, |scope| format!(", !noalias !{scope}")),
            _ => String::new(),
        }
    }
}
