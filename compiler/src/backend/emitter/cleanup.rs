use std::collections::HashSet;
use std::fmt::Write;

use crate::target::TargetLayout;
use crate::{IrReleaseClass, IrVariant, IrWindowShape};

use super::{
    BackendFailure, IrNominalKind, IrProgram, IrType, llvm_type, nominal_symbol, variant_field_base,
};

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
) -> Result<String, BackendFailure> {
    let mut output = String::new();
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

        let aggregate_ty = llvm_type(program, ty)?;
        let symbol = drop_helper_symbol(nominal);
        writeln!(
            output,
            "define private void @{symbol}({aggregate_ty} %value) {{"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        emit_enum_cleanup_body(program, &mut output, variants, ty, &aggregate_ty)?;
        output.push_str("}\n\n");
    }
    for ty in cleanup_run_types(program)? {
        emit_run_drop_helper(program, target, &mut output, ty)?;
    }
    Ok(output)
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
    output: &mut String,
    ty: IrType,
) -> Result<(), BackendFailure> {
    let run_llvm = llvm_type(program, ty)?;
    let symbol = run_drop_helper_symbol(program, ty)?;
    // A runtime-capacity block is reached only through the `Box` that owns
    // it [TYPE-9], so its helper takes the block pointer; every other run is
    // a value and its helper takes that value.
    let parameter = if matches!(
        ty,
        IrType::Window { capacity: None, .. } | IrType::Buffer { .. }
    ) {
        "ptr".to_owned()
    } else {
        run_llvm.clone()
    };
    writeln!(
        output,
        "define private void @{symbol}({parameter} %value) {{\nentry:"
    )
    .map_err(|_| BackendFailure::TextEmission)?;
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
    let element_llvm = llvm_type(program, element_ty)?;
    let address_index = if crate::target::element_has_zero_stride(target, program, element_ty)
        .map_err(BackendFailure::TargetLayout)?
    {
        "0"
    } else {
        "%physical"
    };
    writeln!(
        output,
        "  br label %walk\nwalk:\n  %index = phi i64 [ 0, %entry ], [ %next, %body ]\n  %continue = icmp ult i64 %index, %length\n  br i1 %continue, label %body, label %done\nbody:\n  %raw = add i64 %origin, %index\n  %over = icmp uge i64 %raw, %capacity\n  %reduced = sub i64 %raw, %capacity\n  %physical = select i1 %over, i64 %reduced, i64 %raw\n  %element.pointer = getelementptr inbounds {element_llvm}, ptr %pointer, i64 {address_index}\n  %element = load {element_llvm}, ptr %element.pointer"
    )
    .map_err(|_| BackendFailure::TextEmission)?;
    let mut temporary = 0_u32;
    emit_value_cleanup(
        program,
        output,
        &mut temporary,
        element_ty,
        "%element".to_owned(),
    )?;
    output.push_str("  %next = add i64 %index, 1\n  br label %walk\ndone:\n  ret void\n}\n\n");
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
            IrType::Buffer { element } => {
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
    FreePointer(String),
}

pub(super) fn emit_value_cleanup(
    program: &IrProgram,
    output: &mut String,
    temporary: &mut u32,
    ty: IrType,
    operand: String,
) -> Result<(), BackendFailure> {
    emit_cleanup_jobs(
        program,
        output,
        temporary,
        vec![CleanupJob::Value { ty, operand }],
    )
}

fn emit_cleanup_jobs(
    program: &IrProgram,
    output: &mut String,
    temporary: &mut u32,
    mut jobs: Vec<CleanupJob>,
) -> Result<(), BackendFailure> {
    while let Some(job) = jobs.pop() {
        match job {
            CleanupJob::FreePointer(pointer) => {
                writeln!(output, "  call void @free(ptr {pointer})")
                    .map_err(|_| BackendFailure::TextEmission)?;
            }
            CleanupJob::Field {
                aggregate_ty,
                aggregate,
                index,
                field_ty,
            } => {
                let value = next_temporary(temporary)?;
                writeln!(
                    output,
                    "  %{value} = extractvalue {} {aggregate}, {index}",
                    llvm_type(program, aggregate_ty)?
                )
                .map_err(|_| BackendFailure::TextEmission)?;
                jobs.push(CleanupJob::Value {
                    ty: field_ty,
                    operand: format!("%{value}"),
                });
            }
            CleanupJob::Value { ty, operand } => match ty {
                // A runtime-capacity `Array<T>` exists only as `Box` content
                // [TYPE-9] and is never an owned value of its own, so the
                // cell arm below is the one route to its release, exactly as
                // it is for a runtime-capacity window. Reaching here would
                // mean a value of a type no storage can hold.
                IrType::Buffer { .. } => return Err(BackendFailure::InvalidIr),
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
                                writeln!(
                                    output,
                                    "  call void @{}({} {operand})",
                                    drop_helper_symbol(nominal),
                                    nominal_symbol(nominal)
                                )
                                .map_err(|_| BackendFailure::TextEmission)?;
                            }
                        }
                        IrNominalKind::Opaque => {}
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
                                            writeln!(
                                                output,
                                                "  call void @{symbol}(ptr {operand})"
                                            )
                                            .map_err(|_| BackendFailure::TextEmission)?;
                                        }
                                    }
                                    _ => return Err(BackendFailure::InvalidIr),
                                }
                                continue;
                            }
                            let loaded = next_temporary(temporary)?;
                            writeln!(
                                output,
                                "  %{loaded} = load {}, ptr {operand}",
                                llvm_type(program, *referent)?
                            )
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
                        let run_llvm = llvm_type(program, ty)?;
                        writeln!(output, "  call void @{symbol}({run_llvm} {operand})")
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
    output: &mut String,
    variants: &[IrVariant],
    ty: IrType,
    aggregate_ty: &str,
) -> Result<(), BackendFailure> {
    writeln!(
        output,
        "entry:\n  %tag = extractvalue {aggregate_ty} %value, 0"
    )
    .map_err(|_| BackendFailure::TextEmission)?;
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
        writeln!(output, "variant.{}:", variant.tag()).map_err(|_| BackendFailure::TextEmission)?;
        let base = variant_field_base(variants, variant.tag())?;
        let mut jobs = Vec::new();
        for (field, declaration) in variant.fields().iter().enumerate().rev() {
            if type_requires_cleanup(program, declaration.ty())? {
                jobs.push(CleanupJob::Field {
                    aggregate_ty: ty,
                    aggregate: "%value".to_owned(),
                    index: base
                        .checked_add(field)
                        .ok_or(BackendFailure::CounterOverflow)?,
                    field_ty: declaration.ty(),
                });
            }
        }
        emit_cleanup_jobs(program, output, &mut temporary, jobs)?;
        output.push_str("  br label %done\n");
    }

    output.push_str("invalid:\n  call void @abort()\n  unreachable\ndone:\n  ret void\n");
    Ok(())
}
