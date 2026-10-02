use crate::backend::emission::{FunctionBody, Linkage, Module, Parameter, Signature};
use std::collections::HashSet;
use std::fmt::Write;

use crate::target::TargetLayout;
use crate::{IrReleaseClass, IrShared, IrVariant, IrWindowShape};

use super::union_enums::{is_memory_only, variant_field_gep};
use super::{BackendFailure, IrNominalId, IrNominalKind, IrProgram, IrType};

/// One release action per node type of the release graph [PROV-6].
///
/// A type whose release graph has a cycle enters its own release action where
/// the graph closes, so the walk's depth is the value's rather than the
/// type's. That is the owner's ruling of 2026-09-04, which deleted the cycle
/// refusal this emitter used to work around: a cycle can arise only where a
/// heap is allowed, and a heap-allowed program's resource behaviour is a
/// runtime quantity already. The explicit worklist that ran such a walk off
/// the machine stack is gone with it, and so is the one release-path caller of
/// `wf_resource_abort` — the worklist allocated, and an allocation on the
/// release path is a runtime trap the writer never wrote.
pub(super) fn emit_resource_drop_helpers(
    program: &IrProgram,
    target: TargetLayout,
) -> Result<Module, BackendFailure> {
    let mut module = Module::default();
    for ty in program_types(program)? {
        let IrType::Nominal(id) = ty else {
            continue;
        };
        let nominal = program.nominal(id).ok_or(BackendFailure::InvalidIr)?;
        let IrNominalKind::Enum { variants } = nominal.kind() else {
            continue;
        };
        if !type_requires_cleanup(program, ty)? {
            continue;
        }

        let mut output = FunctionBody::default();
        // A memory-only enum's helper takes its address
        // (compiler/payload-enum-layout); every other takes the value.
        let by_address = is_memory_only(program, ty)?;
        let parameter = if by_address {
            "ptr".to_owned()
        } else {
            output.type_name(program, ty)?
        };
        let symbol = drop_helper_symbol(nominal);
        let mut signature = Signature::new(
            symbol,
            "void",
            vec![Parameter::named(parameter.clone(), "%value")],
        );
        signature.linkage = Linkage::Private;
        signature.references = output.references.clone();
        emit_enum_cleanup_body(program, &mut output, id, variants, by_address, &parameter)?;
        module.define(signature.define(output, "")?);
        module.text("\n");
    }
    for ty in program_types(program)? {
        let IrType::Nominal(id) = ty else {
            continue;
        };
        let nominal = program.nominal(id).ok_or(BackendFailure::InvalidIr)?;
        match nominal.kind() {
            IrNominalKind::Shared {
                state,
                shape: IrShared::Object,
            } => emit_shared_drop_helper(program, &mut module, nominal, *state)?,
            IrNominalKind::Shared {
                shape: IrShared::Map { entry },
                ..
            } => emit_shared_map_drop_helper(program, &mut module, nominal, *entry)?,
            _ => {}
        }
    }
    for ty in cleanup_run_types(program)? {
        emit_run_drop_helper(program, target, &mut module, ty)?;
    }
    Ok(module)
}

/// [SHARE-1] one map handle's release: the runtime counts the handle out, and
/// with the last one hands out every entry still holding a value, each of
/// which is released in place as its `Option<V>`, and then frees the map.
fn emit_shared_map_drop_helper(
    program: &IrProgram,
    module: &mut Module,
    nominal: &crate::IrNominal,
    entry: IrType,
) -> Result<(), BackendFailure> {
    let mut output = FunctionBody::default();
    let symbol = drop_helper_symbol(nominal);
    let mut signature = Signature::new(symbol, "void", vec![Parameter::named("ptr", "%value")]);
    signature.linkage = Linkage::Private;
    output.open_block("entry".to_owned());
    output.instructions(
        "  %last = call i32 @wf__shared_map_release(ptr %value)\n  %is.last = icmp ne i32 %last, 0\n  br i1 %is.last, label %drain, label %done\n",
        &["wf__shared_map_release"],
    );
    output.open_block("drain".to_owned());
    output.instructions(
        "  %slot = call ptr @wf__shared_map_drain(ptr %value)\n  %drained = icmp eq ptr %slot, null\n  br i1 %drained, label %free, label %entry.release\n",
        &["wf__shared_map_drain"],
    );
    output.open_block("entry.release".to_owned());
    if type_requires_cleanup(program, entry)? {
        let mut temporary = 0_u32;
        emit_cleanup_jobs(
            program,
            &mut output,
            &mut temporary,
            vec![CleanupJob::Place {
                address: "%slot".to_owned(),
                ty: entry,
            }],
        )?;
    }
    output.push_str("  br label %drain\n");
    output.open_block("free".to_owned());
    output.instructions(
        "  call void @wf__shared_map_free(ptr %value)\n  br label %done\n",
        &["wf__shared_map_free"],
    );
    output.open_block("done".to_owned());
    output.push_str("  ret void\n");
    signature.references = output.references.clone();
    module.define(signature.define(output, "")?);
    module.text("\n");
    Ok(())
}

/// [SHARE-1] one handle's release: the runtime counts the handle out, and the
/// release of the last one drops the state behind the object's header and
/// returns the object to the runtime.
fn emit_shared_drop_helper(
    program: &IrProgram,
    module: &mut Module,
    nominal: &crate::IrNominal,
    state: IrType,
) -> Result<(), BackendFailure> {
    let mut output = FunctionBody::default();
    let symbol = drop_helper_symbol(nominal);
    let mut signature = Signature::new(symbol, "void", vec![Parameter::named("ptr", "%value")]);
    signature.linkage = Linkage::Private;
    output.open_block("entry".to_owned());
    output.instructions(
        "  %last = call i32 @wf__shared_release(ptr %value)\n  %is.last = icmp ne i32 %last, 0\n  br i1 %is.last, label %state, label %done\n",
        &["wf__shared_release"],
    );
    output.open_block("state".to_owned());
    if type_requires_cleanup(program, state)? {
        writeln!(
            output,
            "  %state.address = getelementptr inbounds i8, ptr %value, i64 {}",
            crate::backend::SHARED_STATE_OFFSET
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        // The state is released in place: a memory-only state is never
        // loaded (compiler/payload-enum-layout), and any other is loaded by
        // the place job itself.
        let mut temporary = 0_u32;
        emit_cleanup_jobs(
            program,
            &mut output,
            &mut temporary,
            vec![CleanupJob::Place {
                ty: state,
                address: "%state.address".to_owned(),
            }],
        )?;
    }
    output.instructions(
        "  call void @wf__shared_free(ptr %value)\n  br label %done\n",
        &["wf__shared_free"],
    );
    output.open_block("done".to_owned());
    output.push_str("  ret void\n");
    signature.references = output.references.clone();
    module.define(signature.define(output, "")?);
    module.text("\n");
    Ok(())
}

/// Whether any type of this program is a shared-object handle [SHARE-1], and
/// so names the runtime's shared-object entries.
pub(super) fn program_uses_shared(program: &IrProgram) -> Result<bool, BackendFailure> {
    Ok(program_types(program)?.into_iter().any(|ty| {
        matches!(ty, IrType::Nominal(id) if program
            .nominal(id)
            .is_some_and(|nominal| matches!(nominal.kind(), IrNominalKind::Shared { .. })))
    }))
}

/// The runtime's shared-object entries (`completion/bridge.h`).
pub(super) fn shared_runtime_declarations() -> Module {
    let mut module = Module::default();
    let declarations: [(&str, &str, &[&str]); 20] = [
        ("wf__shared_new", "ptr", &["i64"]),
        ("wf__shared_share", "void", &["ptr"]),
        ("wf__shared_release", "i32", &["ptr"]),
        ("wf__shared_free", "void", &["ptr"]),
        ("wf__shared_acquire", "i32", &["ptr", "i32", "ptr"]),
        ("wf__shared_unlock", "void", &["ptr", "i32"]),
        ("wf__shared_take", "void", &["ptr", "i32"]),
        ("wf__shared_watch", "i32", &["ptr", "i32", "ptr"]),
        ("wf__shared_map_new", "ptr", &["i64", "i64", "i64"]),
        ("wf__shared_map_share", "void", &["ptr"]),
        ("wf__shared_map_release", "i32", &["ptr"]),
        ("wf__shared_map_drain", "ptr", &["ptr"]),
        ("wf__shared_map_free", "void", &["ptr"]),
        ("wf__shared_map_hold", "void", &["ptr"]),
        ("wf__shared_map_unhold", "void", &["ptr"]),
        ("wf__shared_map_lock", "ptr", &["ptr", "ptr", "i64", "i32"]),
        ("wf__shared_map_unlock", "void", &["ptr", "i32", "i32"]),
        ("wf__shared_map_read", "ptr", &["ptr", "ptr", "i64"]),
        ("wf__shared_map_unread", "void", &["ptr"]),
        ("wf__shared_map_count", "i64", &["ptr"]),
    ];
    for (name, result, parameters) in declarations {
        module.declare(Signature::new(
            name,
            result,
            parameters
                .iter()
                .map(|ty| Parameter::unnamed(*ty))
                .collect(),
        ));
    }
    module
}

/// [PROV-6, WIN-1] one run's release: its window is visited, in ascending
/// logical order, and only then is its own backing released.
///
/// The walk is over the window and not over the capacity, because a slot
/// outside the window is raw [WIN-1] and reading it would be an uninitialized
/// read. The physical slot of logical offset `i` is `(head + i) mod cap`,
/// which is the one conditional subtract a subscript already emits.
///
/// This helper visits elements only. A Box owner releases its allocation after
/// the walk; an inline array or window has no separate backing action.
fn emit_run_drop_helper(
    program: &IrProgram,
    target: TargetLayout,
    module: &mut Module,
    ty: IrType,
) -> Result<(), BackendFailure> {
    let mut output = FunctionBody::default();
    let run_llvm = output.type_name(program, ty)?;
    let symbol = run_drop_helper_symbol(program, ty)?;
    // A runtime-capacity block is reached only through the `Box` that owns
    // it [TYPE-9], so its helper takes the block pointer. A memory-only run
    // (compiler/payload-enum-layout) is released from its address too; every
    // other run is a value and its helper takes that value.
    let by_address = is_memory_only(program, ty)?;
    let parameter = if by_address
        || matches!(
            ty,
            IrType::Window { capacity: None, .. } | IrType::Buffer { .. }
        ) {
        "ptr".to_owned()
    } else {
        run_llvm.clone()
    };
    let mut signature = Signature::new(
        symbol,
        "void",
        vec![Parameter::named(parameter.clone(), "%value")],
    );
    signature.linkage = Linkage::Private;
    if parameter != "ptr" {
        signature.references = output.references.clone();
    }
    output.open_block("entry".to_owned());
    let element = match ty {
        IrType::Buffer { element } => {
            writeln!(
                output,
                "  %pointer = getelementptr inbounds {run_llvm}, ptr %value, i64 0, i32 1, i64 0\n  %length = load i64, ptr %value\n  %capacity = add i64 %length, 0\n  %origin = add i64 0, 0"
            )
            .map_err(|_| BackendFailure::TextEmission)?;
            element
        }
        // A full array has no window descriptor. Every logical element is
        // live; the shared walk performs no access for an empty array.
        IrType::Array { element, length } if by_address => {
            writeln!(
                output,
                "  %pointer = getelementptr inbounds {run_llvm}, ptr %value, i64 0, i64 0\n  %capacity = add i64 {length}, 0\n  %length = add i64 {length}, 0\n  %origin = add i64 0, 0"
            )
            .map_err(|_| BackendFailure::TextEmission)?;
            element
        }
        IrType::Array { element, length } => {
            writeln!(
                output,
                "  %storage = alloca {run_llvm}\n  store {run_llvm} %value, ptr %storage\n  %pointer = getelementptr inbounds {run_llvm}, ptr %storage, i64 0, i64 0\n  %capacity = add i64 {length}, 0\n  %length = add i64 {length}, 0\n  %origin = add i64 0, 0"
            )
            .map_err(|_| BackendFailure::TextEmission)?;
            element
        }
        // A frame-resident run's slots are inside its own value, so the walk
        // needs an address for it; its capacity is the type constant.
        // compiler/storage-representation: the header is first and the
        // slots follow it in the same block, so one address computation
        // serves the inline window; a `Slots` window begins at slot zero.
        IrType::Window {
            shape,
            element,
            capacity: Some(length),
        } if by_address => {
            let slots = if shape == IrWindowShape::Ring { 2 } else { 1 };
            let origin = if shape == IrWindowShape::Ring {
                format!(
                    "  %origin.pointer = getelementptr inbounds {run_llvm}, ptr %value, i64 0, i32 1\n  %origin = load i64, ptr %origin.pointer"
                )
            } else {
                "  %origin = add i64 0, 0".to_owned()
            };
            writeln!(
                output,
                "  %pointer = getelementptr inbounds {run_llvm}, ptr %value, i64 0, i32 {slots}, i64 0\n  %capacity = add i64 {length}, 0\n  %length = load i64, ptr %value\n{origin}"
            )
            .map_err(|_| BackendFailure::TextEmission)?;
            element
        }
        IrType::Window {
            shape,
            element,
            capacity: Some(length),
        } => {
            let slots = if shape == IrWindowShape::Ring { 2 } else { 1 };
            let origin = if shape == IrWindowShape::Ring {
                format!("extractvalue {run_llvm} %value, 1")
            } else {
                "add i64 0, 0".to_owned()
            };
            writeln!(
                output,
                "  %storage = alloca {run_llvm}\n  store {run_llvm} %value, ptr %storage\n  %pointer = getelementptr inbounds {run_llvm}, ptr %storage, i64 0, i32 {slots}, i64 0\n  %capacity = add i64 {length}, 0\n  %length = extractvalue {run_llvm} %value, 0\n  %origin = {origin}"
            )
            .map_err(|_| BackendFailure::TextEmission)?;
            element
        }
        // A runtime-capacity block is `[len | cap | head? | slots]` in one
        // allocation (compiler/storage-representation): every measure the
        // walk needs is a header word of the block the parameter points at,
        // and the slots follow that header.
        IrType::Window {
            shape,
            element,
            capacity: None,
        } => {
            let slots = if shape == IrWindowShape::Ring { 3 } else { 2 };
            let origin = if shape == IrWindowShape::Ring {
                format!(
                    "  %origin.pointer = getelementptr inbounds {run_llvm}, ptr %value, i64 0, i32 2\n  %origin = load i64, ptr %origin.pointer"
                )
            } else {
                "  %origin = add i64 0, 0".to_owned()
            };
            writeln!(
                output,
                "  %pointer = getelementptr inbounds {run_llvm}, ptr %value, i64 0, i32 {slots}, i64 0\n  %length = load i64, ptr %value\n  %capacity.pointer = getelementptr inbounds {run_llvm}, ptr %value, i64 0, i32 1\n  %capacity = load i64, ptr %capacity.pointer\n{origin}"
            )
            .map_err(|_| BackendFailure::TextEmission)?;
            element
        }
        _ => return Err(BackendFailure::InvalidIr),
    };
    let element_ty = program.element(element).ok_or(BackendFailure::InvalidIr)?;
    let element_llvm = output.type_name(program, element_ty)?;
    let address_index = if crate::target::element_has_zero_stride(target, program, element_ty)
        .map_err(BackendFailure::TargetLayout)?
    {
        "0"
    } else {
        "%physical"
    };
    {
        writeln!(output, "  br label %walk").map_err(|_| BackendFailure::TextEmission)?;
        output.open_block("walk".to_string());
        write!(output, "  %index = phi i64 [ 0, %entry ], [ %next, %body ]\n  %continue = icmp ult i64 %index, %length\n  br i1 %continue, label %body, label %done\n").map_err(|_| BackendFailure::TextEmission)?;
        output.open_block("body".to_string());
        write!(output, "  %raw = add i64 %origin, %index\n  %over = icmp uge i64 %raw, %capacity\n  %reduced = sub i64 %raw, %capacity\n  %physical = select i1 %over, i64 %reduced, i64 %raw\n  %element.pointer = getelementptr inbounds {element_llvm}, ptr %pointer, i64 {address_index}\n").map_err(|_| BackendFailure::TextEmission)?;
    };
    let mut temporary = 0_u32;
    // A memory-only element is released where it lies.
    let element = if is_memory_only(program, element_ty)? {
        CleanupOperand::Address("%element.pointer".to_owned())
    } else {
        writeln!(
            output,
            "  %element = load {element_llvm}, ptr %element.pointer"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        CleanupOperand::Value("%element".to_owned())
    };
    emit_cleanup(program, &mut output, &mut temporary, element_ty, element)?;
    output.push_str("  %next = add i64 %index, 1\n  br label %walk\n");
    output.open_block("done".to_owned());
    output.push_str("  ret void\n");
    module.define(signature.define(output, "")?);
    module.text("\n");
    Ok(())
}

/// Every full array or run whose live elements derive release work.
/// The complete type graph includes arbitrary nested arrays, runs, and cycles;
/// its deterministic inventory fixes helper identities without a depth cap.
fn cleanup_run_types(program: &IrProgram) -> Result<Vec<IrType>, BackendFailure> {
    let mut needed = Vec::new();
    for ty in program_types(program)? {
        let (IrType::Array { element, .. }
        | IrType::Window { element, .. }
        | IrType::Buffer { element }) = ty
        else {
            continue;
        };
        let element = program.element(element).ok_or(BackendFailure::InvalidIr)?;
        if type_requires_cleanup(program, element)? {
            needed.push(ty);
        }
    }
    Ok(needed)
}

/// One run type's release helper, named by the digest of the type's stable
/// spelling, so the helper and every call of it keep their text when other
/// types come and go [MOD-8].
fn run_drop_helper_symbol(program: &IrProgram, ty: IrType) -> Result<String, BackendFailure> {
    use core::fmt::Write as _;
    let digest = crate::spec::sha256::digest(stable_type_spelling(program, ty)?.as_bytes());
    let mut symbol = "wf.drop.run.".to_owned();
    for byte in &digest[..8] {
        let _ = write!(symbol, "{byte:02x}");
    }
    Ok(symbol)
}

/// One type's spelling by its structure and the stable link names of the
/// nominals it holds.
fn stable_type_spelling(program: &IrProgram, ty: IrType) -> Result<String, BackendFailure> {
    let element = |element| {
        program
            .element(element)
            .ok_or(BackendFailure::InvalidIr)
            .and_then(|ty| stable_type_spelling(program, ty))
    };
    Ok(match ty {
        IrType::Unit => "unit".to_owned(),
        IrType::Bool => "bool".to_owned(),
        IrType::Integer { width, signed } => {
            format!("{}{width}", if signed { "i" } else { "u" })
        }
        IrType::Float { width } => format!("f{width}"),
        IrType::Nominal(id) => program
            .nominal(id)
            .ok_or(BackendFailure::InvalidIr)?
            .link_name()
            .to_owned(),
        IrType::Address(referent) => {
            format!("address<{}>", stable_type_spelling(program, referent.ty())?)
        }
        IrType::Array {
            element: held,
            length,
        } => format!("array<{};{length}>", element(held)?),
        IrType::Buffer { element: held } => format!("buffer<{}>", element(held)?),
        IrType::Segments { element: held } => format!("segments<{}>", element(held)?),
        IrType::Range { element: held } => format!("range<{}>", element(held)?),
        IrType::RuntimeBoxPayload { nominal } => format!(
            "payload<{}>",
            program
                .nominal(nominal)
                .ok_or(BackendFailure::InvalidIr)?
                .link_name()
        ),
        IrType::Window {
            shape,
            element: held,
            capacity,
        } => format!("{shape:?}<{};{capacity:?}>", element(held)?),
    })
}

/// The helper one run type's release walk is emitted as, when its window holds
/// values that derive a release action.
fn run_drop_helper(program: &IrProgram, ty: IrType) -> Result<Option<String>, BackendFailure> {
    if cleanup_run_types(program)?.contains(&ty) {
        run_drop_helper_symbol(program, ty).map(Some)
    } else {
        Ok(None)
    }
}

/// Every type reachable from the program's functions and constants,
/// including arbitrary run nesting, in deterministic discovery order: nominal
/// types first in declaration order, then the rest. Ownership cycles through
/// descriptors or nominal references visit each exact type once. A nominal no
/// emitted function or constant reaches gets no helper, so a module program
/// entry's build names nothing its execution closure does not use [MOD-9].
pub(super) fn program_types(program: &IrProgram) -> Result<Vec<IrType>, BackendFailure> {
    let reached = reachable_types(program, Vec::new())?
        .into_iter()
        .collect::<HashSet<_>>();
    let seeds = program
        .nominals()
        .iter()
        .map(|nominal| IrType::Nominal(nominal.id()))
        .filter(|ty| reached.contains(ty))
        .collect();
    reachable_types(program, seeds)
}

/// The types reachable from `seeds` and then from the program's constants
/// and functions, in discovery order.
fn reachable_types(program: &IrProgram, seeds: Vec<IrType>) -> Result<Vec<IrType>, BackendFailure> {
    let mut pending = seeds;
    for constant in program.constants() {
        pending.push(constant.ty());
    }
    for function in program.functions() {
        pending.extend(function.value_types().iter().copied());
        pending.extend(function.parameters().iter().map(|(_, ty)| *ty));
        pending.push(function.result());
    }
    let mut types = Vec::new();
    let mut visited = HashSet::new();
    let mut cursor = 0;
    while let Some(ty) = pending.get(cursor).copied() {
        cursor += 1;
        if !visited.insert(ty) {
            continue;
        }
        types.push(ty);
        match ty {
            IrType::Array { element, .. } | IrType::Window { element, .. } => {
                pending.push(program.element(element).ok_or(BackendFailure::InvalidIr)?);
            }
            IrType::Buffer { element } | IrType::Segments { element } => {
                pending.push(program.element(element).ok_or(BackendFailure::InvalidIr)?)
            }
            IrType::Range { element } => {
                pending.push(program.element(element).ok_or(BackendFailure::InvalidIr)?);
            }
            IrType::RuntimeBoxPayload { .. } => {}
            IrType::Address(referent) => pending.push(referent.ty()),
            IrType::Nominal(id) => {
                let nominal = program.nominal(id).ok_or(BackendFailure::InvalidIr)?;
                match nominal.kind() {
                    IrNominalKind::Struct { fields } => {
                        pending.extend(fields.iter().map(|field| field.ty()));
                    }
                    IrNominalKind::Enum { variants } => {
                        pending.extend(
                            variants
                                .iter()
                                .flat_map(|variant| variant.fields())
                                .map(|field| field.ty()),
                        );
                    }
                    IrNominalKind::Box { referent, .. } => pending.push(*referent),
                    IrNominalKind::Shared { state, shape } => {
                        pending.push(*state);
                        if let IrShared::Map { entry } | IrShared::State { entry } = shape {
                            pending.push(*entry);
                        }
                    }
                    IrNominalKind::Opaque => {}
                }
            }
            IrType::Unit | IrType::Bool | IrType::Integer { .. } | IrType::Float { .. } => {}
        }
    }
    Ok(types)
}

/// Whether any type of this program is a run taken from a general store
/// [PROV-1]. Such a run's backing release is a free, so the module declares
/// the two allocator symbols even where nothing else allocates.
pub(super) fn program_has_general_run(program: &IrProgram) -> Result<bool, BackendFailure> {
    Ok(program_types(program)?.into_iter().any(|ty| {
        matches!(
            ty,
            IrType::Buffer { .. } | IrType::Window { capacity: None, .. }
        )
    }))
}

pub(super) fn type_requires_cleanup(
    program: &IrProgram,
    ty: IrType,
) -> Result<bool, BackendFailure> {
    // Whether a value of this type derives release work [STOR-3, PROV-6].
    crate::ir::type_derives_release(program.nominals(), program.elements(), ty)
        .ok_or(BackendFailure::InvalidIr)
}

/// One enum's release helper, named by the enum's stable link name [MOD-8].
pub(super) fn drop_helper_symbol(nominal: &crate::IrNominal) -> String {
    format!("wf.drop.t.{}", nominal.link_name())
}

/// What a release reads: a first-class value, or the address of a value
/// that stays in its storage. A memory-only value
/// (compiler/payload-enum-layout) is always released from its address.
pub(super) enum CleanupOperand {
    Value(String),
    Address(String),
}

enum CleanupJob {
    Value {
        ty: IrType,
        operand: String,
    },
    Field {
        aggregate_ty: IrType,
        aggregate: String,
        index: usize,
        field_ty: IrType,
    },
    /// The value of type `ty` at `address`.
    Place {
        ty: IrType,
        address: String,
    },
    /// Field `index` of the struct of type `aggregate_ty` at `address`.
    StructFieldPlace {
        aggregate_ty: IrType,
        address: String,
        index: usize,
        field_ty: IrType,
    },
    /// Payload field `field` of variant `variant` of the enum at `address`.
    VariantFieldPlace {
        nominal: IrNominalId,
        address: String,
        variant: u32,
        field: u32,
        field_ty: IrType,
    },
    FreePointer(String),
}

pub(super) fn emit_cleanup(
    program: &IrProgram,
    output: &mut FunctionBody,
    temporary: &mut u32,
    ty: IrType,
    operand: CleanupOperand,
) -> Result<(), BackendFailure> {
    let job = match operand {
        CleanupOperand::Value(operand) => CleanupJob::Value { ty, operand },
        CleanupOperand::Address(address) => CleanupJob::Place { ty, address },
    };
    emit_cleanup_jobs(program, output, temporary, vec![job])
}

fn emit_cleanup_jobs(
    program: &IrProgram,
    output: &mut FunctionBody,
    temporary: &mut u32,
    mut jobs: Vec<CleanupJob>,
) -> Result<(), BackendFailure> {
    while let Some(job) = jobs.pop() {
        match job {
            CleanupJob::FreePointer(pointer) => {
                output.symbol("free");
                {
                    output.symbol("free");
                    writeln!(output, "  call void @free(ptr {pointer})")
                }
                .map_err(|_| BackendFailure::TextEmission)?;
            }
            CleanupJob::Field {
                aggregate_ty,
                aggregate,
                index,
                field_ty,
            } => {
                let value = next_temporary(temporary)?;
                {
                    let emitted_type_0 = output.type_name(program, aggregate_ty)?;
                    writeln!(
                        output,
                        "  %{value} = extractvalue {} {aggregate}, {index}",
                        emitted_type_0
                    )
                }
                .map_err(|_| BackendFailure::TextEmission)?;
                jobs.push(CleanupJob::Value {
                    ty: field_ty,
                    operand: format!("%{value}"),
                });
            }
            CleanupJob::StructFieldPlace {
                aggregate_ty,
                address,
                index,
                field_ty,
            } => {
                let pointer = next_temporary(temporary)?;
                let emitted = output.type_name(program, aggregate_ty)?;
                writeln!(
                    output,
                    "  %{pointer} = getelementptr inbounds {emitted}, ptr {address}, i32 0, i32 {index}"
                )
                .map_err(|_| BackendFailure::TextEmission)?;
                jobs.push(CleanupJob::Place {
                    ty: field_ty,
                    address: format!("%{pointer}"),
                });
            }
            CleanupJob::VariantFieldPlace {
                nominal,
                address,
                variant,
                field,
                field_ty,
            } => {
                let pointer = next_temporary(temporary)?;
                let (emitted, indices) = variant_field_gep(
                    program,
                    &mut output.references.types,
                    nominal,
                    variant,
                    field,
                )?;
                writeln!(
                    output,
                    "  %{pointer} = getelementptr inbounds {emitted}, ptr {address}, {indices}"
                )
                .map_err(|_| BackendFailure::TextEmission)?;
                jobs.push(CleanupJob::Place {
                    ty: field_ty,
                    address: format!("%{pointer}"),
                });
            }
            CleanupJob::Place { ty, address } => {
                if !type_requires_cleanup(program, ty)? {
                    continue;
                }
                if !is_memory_only(program, ty)? {
                    let loaded = next_temporary(temporary)?;
                    let emitted = output.type_name(program, ty)?;
                    writeln!(output, "  %{loaded} = load {emitted}, ptr {address}")
                        .map_err(|_| BackendFailure::TextEmission)?;
                    jobs.push(CleanupJob::Value {
                        ty,
                        operand: format!("%{loaded}"),
                    });
                    continue;
                }
                match ty {
                    IrType::Nominal(id) => {
                        let nominal = program.nominal(id).ok_or(BackendFailure::InvalidIr)?;
                        match nominal.kind() {
                            // Jobs are popped: enqueue in reverse to preserve
                            // PROV-6's declaration-order traversal.
                            IrNominalKind::Struct { fields } => {
                                for (index, field) in fields.iter().enumerate().rev() {
                                    if type_requires_cleanup(program, field.ty())? {
                                        jobs.push(CleanupJob::StructFieldPlace {
                                            aggregate_ty: ty,
                                            address: address.clone(),
                                            index,
                                            field_ty: field.ty(),
                                        });
                                    }
                                }
                            }
                            IrNominalKind::Enum { .. } => {
                                let symbol = drop_helper_symbol(nominal);
                                output.symbol(symbol.clone());
                                writeln!(output, "  call void @{symbol}(ptr {address})")
                                    .map_err(|_| BackendFailure::TextEmission)?;
                            }
                            // A pointer owner, a shared handle and the opaque
                            // representation hold no union enum inline.
                            IrNominalKind::Box { .. }
                            | IrNominalKind::Shared { .. }
                            | IrNominalKind::Opaque => {
                                return Err(BackendFailure::InvalidIr);
                            }
                        }
                    }
                    IrType::Array { .. }
                    | IrType::Window {
                        capacity: Some(_), ..
                    } => {
                        let symbol =
                            run_drop_helper(program, ty)?.ok_or(BackendFailure::InvalidIr)?;
                        output.symbol(symbol.clone());
                        writeln!(output, "  call void @{symbol}(ptr {address})")
                            .map_err(|_| BackendFailure::TextEmission)?;
                    }
                    _ => return Err(BackendFailure::InvalidIr),
                }
            }
            // A memory-only value never reaches a release as a first-class
            // value (compiler/payload-enum-layout).
            CleanupJob::Value { ty, .. } if is_memory_only(program, ty)? => {
                return Err(BackendFailure::InvalidIr);
            }
            CleanupJob::Value { ty, operand } => match ty {
                // A runtime-capacity `Array<T>` exists only as `Box` content
                // [TYPE-9] and is never an owned value of its own, so the
                // cell arm below is the one route to its release, exactly as
                // it is for a runtime-capacity window. Reaching here would
                // mean a value of a type no storage can hold.
                IrType::Buffer { .. } | IrType::Segments { .. } => {
                    return Err(BackendFailure::InvalidIr);
                }
                IrType::Nominal(id) => {
                    let nominal = program.nominal(id).ok_or(BackendFailure::InvalidIr)?;
                    match nominal.kind() {
                        IrNominalKind::Struct { fields } => {
                            // Jobs are popped: enqueue in reverse to preserve
                            // PROV-6's declaration-order traversal.
                            for (index, field) in fields.iter().enumerate().rev() {
                                if type_requires_cleanup(program, field.ty())? {
                                    jobs.push(CleanupJob::Field {
                                        aggregate_ty: ty,
                                        aggregate: operand.clone(),
                                        index,
                                        field_ty: field.ty(),
                                    });
                                }
                            }
                        }
                        IrNominalKind::Enum { .. } => {
                            // The one release action of this node type
                            // [PROV-6]. Where the release graph closes on
                            // itself this is the recursive edge, and the
                            // depth is the value's own.
                            if type_requires_cleanup(program, ty)? {
                                output.symbol(drop_helper_symbol(nominal));
                                {
                                    let emitted_type_1 = output.type_name(program, ty)?;
                                    {
                                        output.symbol(drop_helper_symbol(nominal));
                                        writeln!(
                                            output,
                                            "  call void @{}({} {operand})",
                                            drop_helper_symbol(nominal),
                                            emitted_type_1
                                        )
                                    }
                                }
                                .map_err(|_| BackendFailure::TextEmission)?;
                            }
                        }
                        IrNominalKind::Opaque => {}
                        // [SHARE-1] release one handle; its helper releases
                        // the state with the last one.
                        IrNominalKind::Shared { .. } => {
                            let symbol = drop_helper_symbol(nominal);
                            output.symbol(symbol.clone());
                            writeln!(output, "  call void @{symbol}(ptr {operand})")
                                .map_err(|_| BackendFailure::TextEmission)?;
                        }
                        // [PROV-6, STOR-3] release the referent first, then
                        // free the cell back to the one heap.
                        IrNominalKind::Box { referent, release } => {
                            // A boxed runtime-capacity shape is thin: the
                            // cell pointer is the block, whose header and
                            // elements are the same allocation
                            // (compiler/storage-representation), which is
                            // what [TYPE-9]'s "exactly one heap object" and
                            // [STOR-3]'s "one compiler-derived heap free"
                            // say. Loading the block would read past its
                            // declared zero-length element array, so its
                            // walk takes the pointer and the cell's own free
                            // is the block's.
                            // A `Segments` block holds copy elements
                            // [OP-13], so its release is the free alone.
                            if matches!(referent, IrType::Segments { .. }) {
                                if *release == IrReleaseClass::General {
                                    jobs.push(CleanupJob::FreePointer(operand.clone()));
                                }
                                continue;
                            }
                            if matches!(
                                referent,
                                IrType::Window { capacity: None, .. } | IrType::Buffer { .. }
                            ) {
                                if *release == IrReleaseClass::General {
                                    jobs.push(CleanupJob::FreePointer(operand.clone()));
                                }
                                match referent {
                                    IrType::Window { element, .. } | IrType::Buffer { element } => {
                                        let element = program
                                            .element(*element)
                                            .ok_or(BackendFailure::InvalidIr)?;
                                        if type_requires_cleanup(program, element)? {
                                            let symbol = run_drop_helper(program, *referent)?
                                                .ok_or(BackendFailure::InvalidIr)?;
                                            output.symbol(&symbol);
                                            {
                                                output.symbol(symbol.to_string());
                                                writeln!(
                                                    output,
                                                    "  call void @{symbol}(ptr {operand})"
                                                )
                                            }
                                            .map_err(|_| BackendFailure::TextEmission)?;
                                        }
                                    }
                                    _ => return Err(BackendFailure::InvalidIr),
                                }
                                continue;
                            }
                            // A memory-only referent is released in the
                            // cell before the cell itself.
                            if is_memory_only(program, *referent)? {
                                if *release == IrReleaseClass::General {
                                    jobs.push(CleanupJob::FreePointer(operand.clone()));
                                }
                                jobs.push(CleanupJob::Place {
                                    ty: *referent,
                                    address: operand,
                                });
                                continue;
                            }
                            let loaded = next_temporary(temporary)?;
                            {
                                let emitted_type_0 = output.type_name(program, *referent)?;
                                writeln!(
                                    output,
                                    "  %{loaded} = load {}, ptr {operand}",
                                    emitted_type_0
                                )
                            }
                            .map_err(|_| BackendFailure::TextEmission)?;
                            if *release == IrReleaseClass::General {
                                jobs.push(CleanupJob::FreePointer(operand));
                            }
                            jobs.push(CleanupJob::Value {
                                ty: *referent,
                                operand: format!("%{loaded}"),
                            });
                        }
                    }
                }
                // A runtime-capacity window exists only as `Box` content
                // [TYPE-9] and is never an owned value of its own, so the
                // cell arm above is the one route to its release. Reaching
                // here would mean a value of a type no storage can hold.
                IrType::Window { capacity: None, .. } => return Err(BackendFailure::InvalidIr),
                IrType::Array { element, .. }
                | IrType::Window {
                    element,
                    capacity: Some(_),
                    ..
                } => {
                    let element = program.element(element).ok_or(BackendFailure::InvalidIr)?;
                    if type_requires_cleanup(program, element)? {
                        let symbol =
                            run_drop_helper(program, ty)?.ok_or(BackendFailure::InvalidIr)?;
                        let run_llvm = output.type_name(program, ty)?;
                        output.symbol(&symbol);
                        {
                            output.symbol(symbol.to_string());
                            writeln!(output, "  call void @{symbol}({run_llvm} {operand})")
                        }
                        .map_err(|_| BackendFailure::TextEmission)?;
                    }
                }
                IrType::Unit
                | IrType::Bool
                | IrType::Integer { .. }
                | IrType::Float { .. }
                | IrType::Range { .. }
                | IrType::RuntimeBoxPayload { .. }
                | IrType::Address(_) => {}
            },
        }
    }
    Ok(())
}

fn next_temporary(counter: &mut u32) -> Result<String, BackendFailure> {
    let current = *counter;
    *counter = counter
        .checked_add(1)
        .ok_or(BackendFailure::CounterOverflow)?;
    Ok(format!("drop.{current}"))
}

/// The body of one enum's drop: the tag switch and each variant's field
/// cleanup, from the entry label through the closing `ret`.
fn emit_enum_cleanup_body(
    program: &IrProgram,
    output: &mut FunctionBody,
    nominal: IrNominalId,
    variants: &[IrVariant],
    by_address: bool,
    aggregate_ty: &str,
) -> Result<(), BackendFailure> {
    let ty = IrType::Nominal(nominal);
    {
        output.open_block("entry".to_string());
        // The tag is at offset 0 in every enum layout.
        if by_address {
            writeln!(output, "  %tag = load i32, ptr %value")
        } else {
            writeln!(output, "  %tag = extractvalue {aggregate_ty} %value, 0")
        }
        .map_err(|_| BackendFailure::TextEmission)?;
    };
    writeln!(output, "  switch i32 %tag, label %invalid [")
        .map_err(|_| BackendFailure::TextEmission)?;
    for variant in variants {
        writeln!(
            output,
            "    i32 {}, label %variant.{}",
            variant.tag(),
            variant.tag()
        )
        .map_err(|_| BackendFailure::TextEmission)?;
    }
    output.push_str("  ]\n");

    let mut temporary = 0_u32;
    for variant in variants {
        output.open_block(format!("variant.{}", variant.tag()));
        let base = super::variant_field_base(variants, variant.tag())?;
        let mut jobs = Vec::new();
        for (field, declaration) in variant.fields().iter().enumerate().rev() {
            if !type_requires_cleanup(program, declaration.ty())? {
                continue;
            }
            jobs.push(if by_address {
                CleanupJob::VariantFieldPlace {
                    nominal,
                    address: "%value".to_owned(),
                    variant: variant.tag(),
                    field: u32::try_from(field).map_err(|_| BackendFailure::CounterOverflow)?,
                    field_ty: declaration.ty(),
                }
            } else {
                CleanupJob::Field {
                    aggregate_ty: ty,
                    aggregate: "%value".to_owned(),
                    index: base
                        .checked_add(field)
                        .ok_or(BackendFailure::CounterOverflow)?,
                    field_ty: declaration.ty(),
                }
            });
        }
        emit_cleanup_jobs(program, output, &mut temporary, jobs)?;
        output.push_str("  br label %done\n");
    }

    output.symbol("abort");
    output.open_block("invalid".to_owned());
    output.instructions("  call void @abort()\n  unreachable\n", &["abort"]);
    output.open_block("done".to_owned());
    output.push_str("  ret void\n");
    Ok(())
}
