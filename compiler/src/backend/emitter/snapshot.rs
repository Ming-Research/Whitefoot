//! Supplied snapshot definitions and immutable typed traversal metadata.
//!
//! Source calls use the ordinary callable ABI. Only these linked definitions
//! know the capsule ABI; references, held evidence and function forwarding
//! have already been checked and erased before this module runs.
use super::*;
use crate::target::{representation_fields, representation_layout, representation_variant};

const DESCRIPTOR: &str =
    "{ i64, i64, i64, i64, i64, i64, ptr, ptr, ptr, i64, i64, i64, i64, i64, i64 }";
const FIELD: &str = "{ i64, ptr }";
const VARIANT: &str = "{ i64, ptr }";

pub(super) fn required(program: &IrProgram) -> bool {
    program.functions().iter().any(|function| {
        function.blocks().is_empty() && function.name().starts_with("std.snapshot.")
    })
}

fn layout(
    target: TargetLayout,
    program: &IrProgram,
    ty: IrType,
) -> Result<TargetAggregateLayout, BackendFailure> {
    representation_layout(target, program, ty).map_err(BackendFailure::TargetLayout)
}
fn fields(
    target: TargetLayout,
    program: &IrProgram,
    types: &[IrType],
) -> Result<Vec<(IrType, u64)>, BackendFailure> {
    representation_fields(target, program, types).map_err(BackendFailure::TargetLayout)
}
fn variant(
    target: TargetLayout,
    program: &IrProgram,
    ty: IrType,
    tag: u32,
) -> Result<Vec<(IrType, u64)>, BackendFailure> {
    let IrType::Nominal(id) = ty else {
        return Err(BackendFailure::InvalidIr);
    };
    representation_variant(target, program, id, tag).map_err(BackendFailure::TargetLayout)
}
fn structure(program: &IrProgram, ty: IrType) -> Result<Vec<IrType>, BackendFailure> {
    let IrType::Nominal(id) = ty else {
        return Err(BackendFailure::InvalidIr);
    };
    let IrNominalKind::Struct { fields } =
        program.nominal(id).ok_or(BackendFailure::InvalidIr)?.kind()
    else {
        return Err(BackendFailure::InvalidIr);
    };
    Ok(fields.iter().map(|field| field.ty()).collect())
}
fn unbox(program: &IrProgram, ty: IrType) -> Result<IrType, BackendFailure> {
    let IrType::Nominal(id) = ty else {
        return Err(BackendFailure::InvalidIr);
    };
    match program.nominal(id).ok_or(BackendFailure::InvalidIr)?.kind() {
        IrNominalKind::Box {
            referent,
            release: crate::IrReleaseClass::General,
        } => Ok(*referent),
        _ => Err(BackendFailure::InvalidIr),
    }
}
fn element(program: &IrProgram, ty: IrType) -> Result<IrType, BackendFailure> {
    let (IrType::Array { element, .. }
    | IrType::Buffer { element }
    | IrType::Segments { element }
    | IrType::Window { element, .. }) = ty
    else {
        return Err(BackendFailure::InvalidIr);
    };
    program.element(element).ok_or(BackendFailure::InvalidIr)
}

struct Descriptors<'a> {
    program: &'a IrProgram,
    target: TargetLayout,
    prefix: String,
    names: HashMap<IrType, String>,
    module: Module,
}
impl Descriptors<'_> {
    fn field_array(
        &mut self,
        name: String,
        fields: &[(IrType, u64)],
    ) -> Result<String, BackendFailure> {
        if fields.is_empty() {
            return Ok("null".into());
        }
        let mut refs = References::default();
        let mut values = Vec::new();
        for (ty, offset) in fields {
            let child = self.descriptor(*ty)?;
            refs.symbols.insert(child.clone());
            values.push(format!("{FIELD} {{ i64 {offset}, ptr @{child} }}"));
        }
        self.module.global(
            name.clone(),
            "constant",
            format!("[{} x {FIELD}]", fields.len()),
            format!("[{}]", values.join(", ")),
            Some(8),
            refs,
        );
        Ok(format!("@{name}"))
    }
    fn descriptor(&mut self, ty: IrType) -> Result<String, BackendFailure> {
        if let Some(name) = self.names.get(&ty) {
            return Ok(name.clone());
        }
        let name = format!("{}.type.{}", self.prefix, self.names.len());
        // Reserve identity before descending: owned recursive graphs close
        // through Box/storage and refer back to this immutable descriptor.
        self.names.insert(ty, name.clone());
        let native = layout(self.target, self.program, ty)?;
        let (mut shape, mut tag, mut width, mut count) = (0u64, 0u64, 0u64, 0u64);
        let (mut field_ptr, mut variants_ptr, mut element_ptr) =
            ("null".to_owned(), "null".to_owned(), "null".to_owned());
        let (mut length, mut capacity, mut head, mut data, mut fixed, mut page) =
            (0u64, u64::MAX, 0u64, 0u64, 0u64, 0u64);
        match ty {
            IrType::Unit => {}
            IrType::Integer {
                width: bits,
                signed,
            } => {
                width = u64::from(bits / 8);
                tag = match bits {
                    8 => 1,
                    16 => 2,
                    32 => 3,
                    64 => 4,
                    _ => return Err(BackendFailure::InvalidIr),
                } + if signed { 0 } else { 4 };
            }
            IrType::Float { width: bits } => {
                width = u64::from(bits / 8);
                tag = if bits == 32 { 9 } else { 10 };
            }
            IrType::Bool => {
                shape = 2;
                tag = 0x11;
                width = 1;
                count = 2;
                let vn = format!("{name}.variants");
                self.module.global(
                    vn.clone(),
                    "constant",
                    format!("[2 x {VARIANT}]"),
                    format!("[{VARIANT} zeroinitializer, {VARIANT} zeroinitializer]"),
                    Some(8),
                    References::default(),
                );
                variants_ptr = format!("@{vn}");
            }
            IrType::Nominal(id) => {
                let nominal = self.program.nominal(id).ok_or(BackendFailure::InvalidIr)?;
                match nominal.kind() {
                    IrNominalKind::Struct { fields: declared } => {
                        shape = 1;
                        tag = 0x10;
                        count = declared.len() as u64;
                        let types: Vec<_> = declared.iter().map(|field| field.ty()).collect();
                        let offsets = fields(self.target, self.program, &types)?;
                        field_ptr = self.field_array(format!("{name}.fields"), &offsets)?;
                        let stable = nominal.stable_spelling().unwrap_or("");
                        if stable.starts_with("std.collections.hash_map.HashMap<") {
                            shape = 9;
                            tag = 0x30;
                            let bucket = element(self.program, unbox(self.program, types[0])?)?;
                            element_ptr = format!("@{}", self.descriptor(bucket)?);
                        } else if stable.starts_with("std.collections.ordered_map.OrderedMap<") {
                            shape = 10;
                            tag = 0x31;
                            let node = unbox(self.program, element(self.program, types[0])?)?;
                            element_ptr = format!("@{}", self.descriptor(node)?);
                        }
                    }
                    IrNominalKind::Enum { variants } => {
                        shape = 2;
                        tag = 0x11;
                        count = variants.len() as u64;
                        width = if nominal.is_tag_only_enum() && variants.len() <= 2 {
                            1
                        } else {
                            4
                        };
                        let mut entries = Vec::new();
                        let mut refs = References::default();
                        for (ordinal, member) in variants.iter().enumerate() {
                            if member.tag() as usize != ordinal {
                                return Err(BackendFailure::InvalidIr);
                            }
                            let offsets = variant(self.target, self.program, ty, member.tag())?;
                            let fp = self.field_array(format!("{name}.v{ordinal}"), &offsets)?;
                            if let Some(symbol) = fp.strip_prefix('@') {
                                refs.symbols.insert(symbol.into());
                            }
                            entries
                                .push(format!("{VARIANT} {{ i64 {}, ptr {fp} }}", offsets.len()));
                        }
                        let vn = format!("{name}.variants");
                        self.module.global(
                            vn.clone(),
                            "constant",
                            format!("[{} x {VARIANT}]", entries.len()),
                            format!("[{}]", entries.join(", ")),
                            Some(8),
                            refs,
                        );
                        variants_ptr = format!("@{vn}");
                    }
                    IrNominalKind::Box {
                        referent,
                        release: crate::IrReleaseClass::General,
                    } => {
                        shape = 8;
                        tag = 0x25;
                        element_ptr = format!("@{}", self.descriptor(*referent)?);
                    }
                    _ => return Err(BackendFailure::InvalidIr),
                }
            }
            IrType::Array { length: n, .. } => {
                shape = 3;
                tag = 0x20;
                length = u64::MAX;
                fixed = n;
                element_ptr = format!("@{}", self.descriptor(element(self.program, ty)?)?);
            }
            IrType::Buffer { .. } | IrType::Segments { .. } | IrType::Window { .. } => {
                let child = element(self.program, ty)?;
                let alignment = layout(self.target, self.program, child)?.align();
                element_ptr = format!("@{}", self.descriptor(child)?);
                match ty {
                    IrType::Buffer { .. } => {
                        shape = 3;
                        tag = 0x20;
                        data = 8;
                    }
                    IrType::Segments { .. } => {
                        shape = 6;
                        tag = 0x23;
                        data = 8;
                    }
                    IrType::Window {
                        shape: kind,
                        capacity: cap,
                        element,
                    } => {
                        shape = match kind {
                            IrWindowShape::Slots => 4,
                            IrWindowShape::Ring => 5,
                            IrWindowShape::Paged => 7,
                        };
                        tag = match kind {
                            IrWindowShape::Slots => 0x21,
                            IrWindowShape::Ring => 0x22,
                            IrWindowShape::Paged => 0x24,
                        };
                        fixed = cap.unwrap_or(0);
                        capacity = if cap.is_some() { u64::MAX } else { 8 };
                        data = if cap.is_some() { 8 } else { 16 };
                        if kind == IrWindowShape::Ring {
                            head = data;
                            data += 8;
                        }
                        if kind == IrWindowShape::Paged {
                            data = crate::target::PAGED_HEADER_BYTES;
                            page =
                                crate::target::paged_geometry(self.target, self.program, element)
                                    .map_err(BackendFailure::TargetLayout)?
                                    .0;
                        }
                    }
                    _ => return Err(BackendFailure::InvalidIr),
                }
                if !matches!(ty, IrType::Segments { .. } | IrType::Window { shape: IrWindowShape::Paged, .. }) {
                    data = data.div_ceil(alignment) * alignment;
                }
            }
            _ => return Err(BackendFailure::InvalidIr),
        }
        let mut refs = References::default();
        for pointer in [&field_ptr, &variants_ptr, &element_ptr] {
            if let Some(symbol) = pointer.strip_prefix('@') {
                refs.symbols.insert(symbol.into());
            }
        }
        self.module.global(name.clone(), "constant", DESCRIPTOR.into(), format!("{{ i64 {shape}, i64 {tag}, i64 {width}, i64 {}, i64 {}, i64 {count}, ptr {field_ptr}, ptr {variants_ptr}, ptr {element_ptr}, i64 {length}, i64 {capacity}, i64 {head}, i64 {data}, i64 {fixed}, i64 {page} }}", native.size(), native.align()), Some(8), refs);
        Ok(name)
    }
}

/// The body builder records every link dependency when writing a native call.
struct Wrapper {
    body: FunctionBody,
    declarations: Module,
    temporary: usize,
}
impl Wrapper {
    fn new() -> Self {
        let mut body = FunctionBody::default();
        body.open_block("entry".into());
        Self {
            body,
            declarations: Module::default(),
            temporary: 0,
        }
    }
    fn temp(&mut self) -> String {
        let name = format!("%s{}", self.temporary);
        self.temporary += 1;
        name
    }
    fn line(&mut self, text: impl AsRef<str>) {
        self.body.push_str(text.as_ref());
        self.body.push('\n');
    }
    fn call(&mut self, name: &str, result: &str, args: &[(&str, String)]) -> String {
        self.body.symbol(name);
        self.declarations.declare(Signature::new(
            name,
            result,
            args.iter().map(|(ty, _)| Parameter::unnamed(*ty)).collect(),
        ));
        let arguments = args
            .iter()
            .map(|(ty, arg)| format!("{ty} {arg}"))
            .collect::<Vec<_>>()
            .join(", ");
        if result == "void" {
            self.line(format!("  call void @{name}({arguments})"));
            String::new()
        } else {
            let value = self.temp();
            self.line(format!("  {value} = call {result} @{name}({arguments})"));
            value
        }
    }
    fn address(&mut self, base: &str, offset: u64) -> String {
        if offset == 0 {
            return base.into();
        }
        let ptr = self.temp();
        self.line(format!(
            "  {ptr} = getelementptr i8, ptr {base}, i64 {offset}"
        ));
        ptr
    }
    fn load(&mut self, base: &str, offset: u64, ty: &str) -> String {
        let ptr = self.address(base, offset);
        let value = self.temp();
        self.line(format!("  {value} = load {ty}, ptr {ptr}"));
        value
    }
    fn store(&mut self, base: &str, offset: u64, ty: &str, value: &str) {
        let ptr = self.address(base, offset);
        self.line(format!("  store {ty} {value}, ptr {ptr}"));
    }
    fn zero(
        &mut self,
        target: TargetLayout,
        program: &IrProgram,
        ty: IrType,
    ) -> Result<(), BackendFailure> {
        self.call(
            "llvm.memset.p0.i64",
            "void",
            &[
                ("ptr", "%result".into()),
                ("i8", "0".into()),
                ("i64", layout(target, program, ty)?.size().to_string()),
                ("i1", "false".into()),
            ],
        );
        Ok(())
    }
    fn emit(
        self,
        mut module: Module,
        name: String,
        result: &str,
        parameters: Vec<Parameter>,
    ) -> Result<Module, BackendFailure> {
        module.append(self.declarations);
        module.define(Signature::new(name, result, parameters).define(self.body, "")?);
        Ok(module)
    }
}

fn parameters(
    program: &IrProgram,
    function: &IrFunction,
    waiting: bool,
) -> Result<Vec<Parameter>, BackendFailure> {
    let abi = FunctionAbi::build(program, function)?;
    if !waiting && !abi.result().uses_destination() {
        return Err(BackendFailure::InvalidIr);
    }
    let mut result = vec![Parameter::named("ptr", "%result")];
    for (index, parameter) in abi.parameters().iter().enumerate() {
        if parameter.is_range() {
            result.push(Parameter::named("ptr", format!("%p{index}")));
            result.push(Parameter::named("i64", format!("%p{index}.len")));
        } else {
            let ty = if parameter.is_indirect() {
                "ptr".into()
            } else {
                llvm_type(program, parameter.ty())?
            };
            result.push(Parameter::named(ty, format!("%p{index}")));
        }
    }
    if waiting {
        result.push(Parameter::named("ptr", "%operation"));
    }
    Ok(result)
}

/// A capture specialization obtains V from its ordinary checked map parameter.
fn capture(
    program: &IrProgram,
    target: TargetLayout,
    function: &IrFunction,
) -> Result<Module, BackendFailure> {
    let Some((_, IrType::Address(IrAddressed::Nominal(map)))) = function.parameters().get(1) else {
        return Err(BackendFailure::InvalidIr);
    };
    let IrNominalKind::Shared {
        state: value,
        shape: IrShared::Map { entry },
    } = program
        .nominal(*map)
        .ok_or(BackendFailure::InvalidIr)?
        .kind()
    else {
        return Err(BackendFailure::InvalidIr);
    };
    let present = variant(target, program, *entry, 1)?;
    if present.len() != 1 || present[0].0 != *value {
        return Err(BackendFailure::InvalidIr);
    }
    let mut descriptors = Descriptors {
        program,
        target,
        prefix: format!("wf.snapshot.{}", function.name()),
        names: HashMap::new(),
        module: Module::default(),
    };
    let descriptor = descriptors.descriptor(*value)?;
    let mut w = Wrapper::new();
    w.body.symbol(&descriptor);
    let owner = w.load("%p0", 0, "ptr");
    let table = w.load("%p1", 0, "ptr");
    let job = w.call(
        "wf_snapshot_capture",
        "ptr",
        &[
            ("ptr", owner),
            ("ptr", table),
            ("ptr", "%p2".into()),
            ("ptr", format!("@{descriptor}")),
            ("i64", "0".into()),
            ("i32", "4".into()),
            ("i64", "0".into()),
            ("i64", present[0].1.to_string()),
        ],
    );
    let refused = w.temp();
    w.line(format!("  {refused} = icmp eq ptr {job}, null"));
    w.zero(target, program, function.result())?;
    w.line(format!("  br i1 {refused}, label %refused, label %created"));
    w.body.open_block("refused".into());
    w.store("%result", 0, "i32", "1");
    w.line("  ret void");
    w.body.open_block("created".into());
    let payload = variant(target, program, function.result(), 0)?;
    w.store("%result", payload[0].1, "ptr", &job);
    w.line("  ret void");
    w.emit(
        descriptors.module,
        source_symbol(function.name()),
        "void",
        parameters(program, function, false)?,
    )
}

fn immediate(
    program: &IrProgram,
    target: TargetLayout,
    function: &IrFunction,
    name: &str,
) -> Result<Module, BackendFailure> {
    let mut w = Wrapper::new();
    w.zero(target, program, function.result())?;
    if name == "prepare" {
        w.line("  %error = alloca i32, align 4");
        let owner = w.call(
            "wf_snapshot_prepare",
            "ptr",
            &[
                ("i64", "%p0".into()),
                ("i64", "%p1".into()),
                ("ptr", "%error".into()),
            ],
        );
        let failed = w.temp();
        w.line(format!("  {failed} = icmp eq ptr {owner}, null"));
        w.line(format!("  br i1 {failed}, label %refused, label %prepared"));
        w.body.open_block("refused".into());
        w.store("%result", 0, "i32", "1");
        let error = w.load("%error", 0, "i32");
        let ordinal = w.temp();
        w.line(format!("  {ordinal} = sub i32 {error}, 1"));
        let bit = w.temp();
        w.line(format!("  {bit} = trunc i32 {ordinal} to i1"));
        let payload = variant(target, program, function.result(), 1)?;
        w.store("%result", payload[0].1, "i1", &bit);
        w.line("  ret i32 0");
        w.body.open_block("prepared".into());
        let payload = variant(target, program, function.result(), 0)?;
        w.store("%result", payload[0].1, "ptr", &owner);
    } else {
        let owner = w.load("%p0", 0, "ptr");
        w.call("wf_snapshot_discard", "void", &[("ptr", owner)]);
    }
    w.line("  ret i32 0");
    let params = parameters(program, function, true)?;
    let mut module = w.emit(
        Module::default(),
        format!("{}.start", source_symbol(function.name())),
        "i32",
        params.clone(),
    )?;
    let mut finish = Wrapper::new();
    finish.line("  ret void");
    module = finish.emit(
        module,
        format!("{}.finish", source_symbol(function.name())),
        "void",
        params,
    )?;
    Ok(module)
}

fn read_result(
    w: &mut Wrapper,
    program: &IrProgram,
    target: TargetLayout,
    result: IrType,
) -> Result<(), BackendFailure> {
    let error = w.load("%compact", 8, "i32");
    let failed = w.temp();
    w.line(format!("  {failed} = icmp ne i32 {error}, 0"));
    w.zero(target, program, result)?;
    w.line(format!("  br i1 {failed}, label %failed, label %read"));
    w.body.open_block("failed".into());
    w.store("%result", 0, "i32", "1");
    let ordinal = w.temp();
    w.line(format!("  {ordinal} = sub i32 {error}, 1"));
    let payload = variant(target, program, result, 1)?;
    w.store("%result", payload[0].1, "i32", &ordinal);
    w.line("  br label %translated");
    w.body.open_block("read".into());
    let count = w.load("%compact", 0, "i64");
    let payload = variant(target, program, result, 0)?;
    w.store("%result", payload[0].1, "i64", &count);
    w.line("  br label %translated");
    w.body.open_block("translated".into());
    Ok(())
}

fn read(
    program: &IrProgram,
    target: TargetLayout,
    function: &IrFunction,
) -> Result<Module, BackendFailure> {
    let mut w = Wrapper::new();
    w.line("  %compact = alloca { i64, i32 }, align 8");
    let owner = w.load("%p0", 0, "ptr");
    let cancel = w.load("%p3", 0, "ptr");
    let deadline = function
        .parameters()
        .get(2)
        .ok_or(BackendFailure::InvalidIr)?
        .1;
    let payload = variant(target, program, deadline, 1)?;
    let tag = w.load("%p2", 0, "i32");
    let some = w.temp();
    w.line(format!("  {some} = icmp eq i32 {tag}, 1"));
    w.line(format!("  br i1 {some}, label %some, label %none"));
    w.body.open_block("some".into());
    let ticks = w.load("%p2", payload[0].1, "i64");
    w.line("  br label %submit");
    w.body.open_block("none".into());
    w.line("  br label %submit");
    w.body.open_block("submit".into());
    w.line(format!(
        "  %deadline = phi i64 [ {ticks}, %some ], [ 0, %none ]"
    ));
    let present = w.temp();
    w.line(format!("  {present} = zext i1 {some} to i32"));
    let state = w.call(
        "wf_snapshot_read_start",
        "i32",
        &[
            ("ptr", owner),
            ("ptr", "%p1".into()),
            ("i64", "%p1.len".into()),
            ("i64", "%deadline".into()),
            ("i32", present),
            ("ptr", cancel),
            ("ptr", "%operation".into()),
            ("ptr", "%compact".into()),
        ],
    );
    let immediate = w.temp();
    w.line(format!("  {immediate} = icmp eq i32 {state}, 0"));
    w.line(format!(
        "  br i1 {immediate}, label %translate, label %submitted"
    ));
    w.body.open_block("submitted".into());
    w.line(format!("  ret i32 {state}"));
    w.body.open_block("translate".into());
    read_result(&mut w, program, target, function.result())?;
    w.line("  ret i32 0");
    let params = parameters(program, function, true)?;
    let module = w.emit(
        Module::default(),
        format!("{}.start", source_symbol(function.name())),
        "i32",
        params.clone(),
    )?;
    let mut w = Wrapper::new();
    w.line("  %compact = alloca { i64, i32 }, align 8");
    let owner = w.load("%p0", 0, "ptr");
    w.call(
        "wf_snapshot_read_finish",
        "void",
        &[
            ("ptr", owner),
            ("ptr", "%p1".into()),
            ("ptr", "%operation".into()),
            ("ptr", "%compact".into()),
        ],
    );
    read_result(&mut w, program, target, function.result())?;
    w.line("  ret void");
    w.emit(
        module,
        format!("{}.finish", source_symbol(function.name())),
        "void",
        params,
    )
}

fn finish(
    program: &IrProgram,
    target: TargetLayout,
    function: &IrFunction,
    abort: bool,
) -> Result<Module, BackendFailure> {
    let mut w = Wrapper::new();
    let owner = w.load("%p0", 0, "ptr");
    let state = w.call(
        "wf__completion_snapshot_finish_submit",
        "i32",
        &[
            ("ptr", owner),
            ("i32", u32::from(abort).to_string()),
            ("ptr", "%operation".into()),
        ],
    );
    w.line(format!("  ret i32 {state}"));
    let params = parameters(program, function, true)?;
    let module = w.emit(
        Module::default(),
        format!("{}.start", source_symbol(function.name())),
        "i32",
        params.clone(),
    )?;
    let mut w = Wrapper::new();
    w.line("  %compact = alloca { i64, i64, i64, i64, i32, i32, i32, i32 }, align 8");
    w.line("  %transferred = alloca i64, align 8");
    w.line("  %native.error = alloca i32, align 4");
    w.call(
        "wf__completion_file_join",
        "void",
        &[
            ("ptr", "%operation".into()),
            ("ptr", "%transferred".into()),
            ("ptr", "%native.error".into()),
        ],
    );
    let owner = w.load("%p0", 0, "ptr");
    w.call(
        "wf_snapshot_take_outcome",
        "void",
        &[("ptr", owner), ("ptr", "%compact".into())],
    );
    w.zero(target, program, function.result())?;
    let result_fields = fields(target, program, &structure(program, function.result())?)?;
    if result_fields.len() != 3 {
        return Err(BackendFailure::InvalidIr);
    }
    let (child_ty, child_offset) = result_fields[0];
    let child = w.address("%result", child_offset);
    let child_tag = w.load("%compact", 32, "i32");
    let detail = w.load("%compact", 36, "i32");
    w.store(&child, 0, "i32", &child_tag);
    w.line(format!("  switch i32 {child_tag}, label %child.protocol [ i32 0, label %child.complete i32 1, label %child.export i32 2, label %child.exit i32 3, label %child.signal ]"));
    for (tag, label) in [
        (0, "complete"),
        (1, "export"),
        (2, "exit"),
        (3, "signal"),
        (4, "protocol"),
    ] {
        w.body.open_block(format!("child.{label}"));
        let payload = variant(target, program, child_ty, tag)?;
        if tag == 0 {
            let bytes = w.load("%compact", 0, "i64");
            let records = w.load("%compact", 8, "i64");
            w.store(&child, payload[0].1, "i64", &bytes);
            w.store(&child, payload[1].1, "i64", &records);
        } else if tag == 2 {
            let code = w.temp();
            w.line(format!("  {code} = trunc i32 {detail} to i8"));
            w.store(&child, payload[0].1, "i8", &code);
        } else {
            w.store(&child, payload[0].1, "i32", &detail);
        }
        w.line("  br label %collection");
    }
    w.body.open_block("collection".into());
    let (collection_ty, collection_offset) = result_fields[1];
    let collection = w.address("%result", collection_offset);
    let collection_tag = w.load("%compact", 40, "i32");
    let delivered = w.load("%compact", 16, "i64");
    w.store(&collection, 0, "i32", &collection_tag);
    w.line(format!("  switch i32 {collection_tag}, label %collection.failed [ i32 0, label %collection.collected i32 1, label %collection.discarded ]"));
    for (tag, label) in [(0, "collected"), (1, "discarded"), (2, "failed")] {
        w.body.open_block(format!("collection.{label}"));
        let payload = variant(target, program, collection_ty, tag)?;
        match tag {
            0 => w.store(&collection, payload[0].1, "i64", &delivered),
            1 => {
                let discarded = w.load("%compact", 24, "i64");
                w.store(&collection, payload[0].1, "i64", &delivered);
                w.store(&collection, payload[1].1, "i64", &discarded);
            }
            _ => {
                w.store(&collection, payload[0].1, "i32", "0");
                w.store(&collection, payload[1].1, "i64", &delivered);
            }
        }
        w.line("  br label %outcome");
    }
    w.body.open_block("outcome".into());
    let requested = w.load("%compact", 44, "i32");
    let bit = w.temp();
    w.line(format!("  {bit} = icmp ne i32 {requested}, 0"));
    w.store("%result", result_fields[2].1, "i1", &bit);
    w.line("  ret void");
    w.emit(
        module,
        format!("{}.finish", source_symbol(function.name())),
        "void",
        params,
    )
}

pub(super) fn definition(
    program: &IrProgram,
    target: TargetLayout,
    function: &IrFunction,
) -> Result<Option<Module>, BackendFailure> {
    if !function.blocks().is_empty() {
        return Ok(None);
    }
    let Some(name) = function.name().strip_prefix("std.snapshot.") else {
        return Ok(None);
    };
    let module = if name.starts_with("capture_export.typed.") {
        capture(program, target, function)?
    } else {
        match name {
            "prepare" | "discard" => immediate(program, target, function, name)?,
            "read" => read(program, target, function)?,
            "finish" | "abort" => finish(program, target, function, name == "abort")?,
            _ => return Err(BackendFailure::InvalidIr),
        }
    };
    Ok(Some(module))
}
