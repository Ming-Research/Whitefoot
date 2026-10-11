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
