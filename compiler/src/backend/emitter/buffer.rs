//! Emission of the runtime-capacity `Array<T>` [TYPE-9] and its readers.
//!
//! The shape is one heap block `[len | elements]`
//! (compiler/storage-representation): [TYPE-9] stores a `Box`'s content "in
//! exactly one heap object the `Box` value owns" and [STOR-3] reclaims it
//! with "one compiler-derived heap free", so the cell pointer *is* the block
//! pointer, one `malloc` builds it, one `free` reclaims it, and every element
//! address is one `inbounds` step past the header. An `Array`'s `len` equals
//! its `cap` [WIN-1], so the one runtime number is stored once, exactly as a
//! boxed `Slots` stores `len` and `cap` and a boxed `Ring` stores `head`
//! beside them.
//!
//! Runtime-content references retain the selected Box owner slot; each access
//! resolves its current block so an earlier alias follows content exchange.

use crate::IrElement;

use super::*;

/// The aggregate field index of the `len` word, which the header-first layout
/// puts first.
const LENGTH_FIELD: usize = 0;

/// The aggregate field index of the element array.
const ELEMENTS_FIELD: u32 = 1;

impl<'program, 'state> FunctionEmitter<'program, 'state> {
    /// The block type one `Box<Array<T>>` cell nominal owns.
    fn buffer_block_type(&self, nominal: IrNominalId) -> Result<IrType, BackendFailure> {
        let IrNominalKind::Box { referent, .. } = self.nominal(nominal)?.kind() else {
            return Err(BackendFailure::InvalidIr);
        };
        if !matches!(referent, IrType::Buffer { .. }) {
            return Err(BackendFailure::InvalidIr);
        }
        Ok(*referent)
    }

    /// The block type one buffer reference names.
    ///
    /// [TYPE-9] admits a runtime-capacity shape only as `Box` content, so the
    /// operand addresses the selected Box slot and is never a value of the
    /// runtime content itself.
    pub(super) fn buffer_block(
        &self,
        buffer: IrValueId,
    ) -> Result<(IrType, IrElement), BackendFailure> {
        match self.value_type(buffer) {
            Some(IrType::Address(IrAddressed::Buffer { element })) => {
                Ok((IrType::Buffer { element }, element))
            }
            _ => Err(BackendFailure::InvalidIr),
        }
    }

    /// The byte offset of the first element: the block's header.
    fn buffer_header_size(&mut self, block: IrType) -> Result<String, BackendFailure> {
        Ok(format!(
            "ptrtoint (ptr getelementptr ({}, ptr null, i64 0, i32 {ELEMENTS_FIELD}) to i64)",
            self.output.type_name(self.program, block)?
        ))
    }

    fn buffer_element_stride(&mut self, element: IrElement) -> Result<String, BackendFailure> {
        Ok(format!(
            "ptrtoint (ptr getelementptr ({}, ptr null, i64 1) to i64)",
            self.output.type_name(
                self.program,
                self.program
                    .element(element)
                    .ok_or(BackendFailure::InvalidIr)?
            )?
        ))
    }

    /// One element's address inside the block, which is the one `inbounds`
    /// step [OP-4]'s discharged subscript needs.
    pub(super) fn buffer_element_pointer(
        &mut self,
        block: IrType,
        address: &str,
        offset: &str,
    ) -> Result<String, BackendFailure> {
        let IrType::Buffer { element } = block else {
            return Err(BackendFailure::InvalidIr);
        };
        let element = self
            .program
            .element(element)
            .ok_or(BackendFailure::InvalidIr)?;
        let offset = self.element_address_index(element, offset)?;
        let pointer = self.next_temporary()?;
        {
let emitted_type_0 = self.output.type_name(self.program, block)?;
writeln!(self.output, "  %{pointer} = getelementptr inbounds {}, ptr {address}, i64 0, i32 {ELEMENTS_FIELD}, i64 {offset}", emitted_type_0)
}
        .map_err(|_| BackendFailure::TextEmission)?;
        Ok(format!("%{pointer}"))
    }

    /// Projects the first-element address carried by a synthesized task
    /// capture. This changes only the compiler's internal capture ABI; the
    /// source Box value remains the allocation-base pointer.
    pub(super) fn emit_runtime_box_payload(
        &mut self,
        result: IrValueId,
        ty: IrType,
        nominal: IrNominalId,
        owner: IrValueId,
    ) -> Result<(), BackendFailure> {
        if ty != (IrType::RuntimeBoxPayload { nominal })
            || self.value_type(owner) != Some(IrType::Nominal(nominal))
        {
            return Err(BackendFailure::InvalidIr);
        }
        let block = self.buffer_block_type(nominal)?;
        {
            let emitted_type_1 = self.output.type_name(self.program, block)?;
            writeln!(
                self.output,
                "  {} = getelementptr inbounds {}, ptr {}, i64 0, i32 {ELEMENTS_FIELD}, i64 0",
                self.value_name(result),
                emitted_type_1,
                self.value_name(owner)
            )
        }
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// Reverses [`Self::emit_runtime_box_payload`] before ordinary chunk
    /// lowering rebuilds its borrowed local Box slot.
    pub(super) fn emit_runtime_box_owner(
        &mut self,
        result: IrValueId,
        ty: IrType,
        nominal: IrNominalId,
        payload: IrValueId,
    ) -> Result<(), BackendFailure> {
        if ty != IrType::Nominal(nominal)
            || self.value_type(payload) != Some(IrType::RuntimeBoxPayload { nominal })
        {
            return Err(BackendFailure::InvalidIr);
        }
        let block = self.buffer_block_type(nominal)?;
        let negative_header = self.next_temporary()?;
        let header_size = self.buffer_header_size(block)?;
        writeln!(
            self.output,
            "  %{negative_header} = sub i64 0, {}\n  {} = getelementptr inbounds i8, ptr {}, i64 %{negative_header}",
            header_size,
            self.value_name(result),
            self.value_name(payload),
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// [OP-13] `box_array_filled`: one block `[len | elements]`, every
    /// element holding the supplied copy value, and the cell that owns it,
    /// which is that same pointer.
    pub(super) fn emit_buffer_fill(
        &mut self,
        result: IrValueId,
        ty: IrType,
        nominal: IrNominalId,
        length: IrValueId,
        value: IrValueId,
        obligations: IrAllocationObligations,
    ) -> Result<(), BackendFailure> {
        if !obligations.target_domains.is_complete() || ty != IrType::Nominal(nominal) {
            return Err(BackendFailure::InvalidIr);
        }
        let block = self.buffer_block_type(nominal)?;
        let IrType::Buffer { element } = block else {
            return Err(BackendFailure::InvalidIr);
        };
        let u64_type = IrType::Integer {
            width: 64,
            signed: false,
        };
        if self.value_type(length) != Some(u64_type)
            || self.value_type(value)
                != Some(
                    self.program
                        .element(element)
                        .ok_or(BackendFailure::InvalidIr)?,
                )
        {
            return Err(BackendFailure::InvalidIr);
        }
        self.emit_buffer_block(result, block, element, length, Some(value))
    }

    /// One allocation of `header + count * stride` bytes, the `len` word, and
    /// the element loop.
    ///
    /// The count's product with the stride is representable: [OP-9]'s
    /// accepted site proved `n <= floor((2^64 - 1) / stride_ceiling(T))` and
    /// target qualification proved the actual stride no larger than that
    /// ceiling, so neither the product nor the header's addition wraps.
    fn emit_buffer_block(
        &mut self,
        result: IrValueId,
        block: IrType,
        element: IrElement,
        length: IrValueId,
        stored: Option<IrValueId>,
    ) -> Result<(), BackendFailure> {
        let element_type = self.output.type_name(
            self.program,
            self.program
                .element(element)
                .ok_or(BackendFailure::InvalidIr)?,
        )?;
        let stride = self.buffer_element_stride(element)?;
        let header = self.buffer_header_size(block)?;
        let element_bytes = self.next_temporary()?;
        let bytes = self.next_temporary()?;
        let nonnull = self.next_temporary()?;
        let allocate = buffer_fill_allocate_label(result);
        let oom = buffer_fill_oom_label(result);
        let init = buffer_fill_init_label(result);
        let head = buffer_fill_head_label(result);
        let body = buffer_fill_body_label(result);
        let done = buffer_fill_done_label(result);
        let count = self.value_name(length);
        let address = self.value_name(result);
        {
            write!(self.output, "  %{element_bytes} = mul nuw i64 {count}, {stride}\n  %{bytes} = add nuw i64 %{element_bytes}, {header}\n  br label %{allocate}\n").map_err(|_| BackendFailure::TextEmission)?;
            self.output.open_block(allocate.to_string());
            {
                self.output.symbol("malloc");
                write!(
                    self.output,
                    "  {address} = call ptr @malloc(i64 %{bytes})\n  %{nonnull} = icmp ne ptr {address}, null\n  br i1 %{nonnull}, label %{init}, label %{oom}\n"
                )
            }?;
            self.output.open_block(oom.to_string());
            {
                self.output.symbol("wf_resource_abort");
                write!(
                    self.output,
                    "  call void @wf_resource_abort()\n  unreachable\n"
                )
            }?;
            self.output.open_block(init.to_string());
        };
        let length_address = self.aggregate_field_pointer(block, &address, LENGTH_FIELD)?;
        let index = self.next_temporary()?;
        let in_range = self.next_temporary()?;
        let next_index = self.next_temporary()?;
        {
            write!(
                self.output,
                "  store i64 {count}, ptr {length_address}\n  br label %{head}\n"
            )
            .map_err(|_| BackendFailure::TextEmission)?;
            self.output.open_block(head.to_string());
            write!(self.output, "  %{index} = phi i64 [ 0, %{init} ], [ %{next_index}, %{body} ]\n  %{in_range} = icmp ult i64 %{index}, {count}\n  br i1 %{in_range}, label %{body}, label %{done}\n").map_err(|_| BackendFailure::TextEmission)?;
            self.output.open_block(body.to_string());
        };
        let offset = format!("%{index}");
        let element_pointer = self.buffer_element_pointer(block, &address, &offset)?;
        if let Some(value) = stored {
            self.store_value_at(value, &element_pointer)?;
        } else {
            writeln!(
                self.output,
                "  store {element_type} zeroinitializer, ptr {element_pointer}"
            )
            .map_err(|_| BackendFailure::TextEmission)?;
        }
        {
            write!(
                self.output,
                "  %{next_index} = add i64 %{index}, 1\n  br label %{head}\n"
            )
            .map_err(|_| BackendFailure::TextEmission)?;
            self.output.open_block(done.to_string());
            Ok::<_, BackendFailure>(())
        }
    }

    /// [MSR-1] the one measure of a runtime-capacity `Array<T>`: the `len`
    /// word at the head of its block.
    pub(super) fn emit_buffer_length(
        &mut self,
        result: IrValueId,
        ty: IrType,
        buffer: IrValueId,
    ) -> Result<(), BackendFailure> {
        if ty
            != (IrType::Integer {
                width: 64,
                signed: false,
            })
        {
            return Err(BackendFailure::InvalidIr);
        }
        let (block, _) = self.buffer_block(buffer)?;
        let address = self.addressed_storage_pointer(buffer)?;
        let length_address = self.aggregate_field_pointer(block, &address, LENGTH_FIELD)?;
        writeln!(
            self.output,
            "  {} = load i64, ptr {length_address}",
            self.value_name(result),
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// Emits a discharged source subscript read [OP-4]: the checker derived
    /// the bounds obligation, so no compare, branch, or trap is emitted in
    /// any build mode, and the element address is one `inbounds` step into
    /// the block.
    pub(super) fn emit_buffer_index(
        &mut self,
        result: IrValueId,
        ty: IrType,
        buffer: IrValueId,
        offset: IrValueId,
        target_domain: IrTargetDomainObligation,
    ) -> Result<(), BackendFailure> {
        if target_domain != IrTargetDomainObligation::ElementAddress {
            return Err(BackendFailure::InvalidIr);
        }
        let (block, element) = self.buffer_block(buffer)?;
        if self
            .program
            .element(element)
            .ok_or(BackendFailure::InvalidIr)?
            != ty
            || self.value_type(offset)
                != Some(IrType::Integer {
                    width: 64,
                    signed: false,
                })
        {
            return Err(BackendFailure::InvalidIr);
        }
        let address = self.addressed_storage_pointer(buffer)?;
        let index = self.value_name(offset);
        let element_pointer = self.buffer_element_pointer(block, &address, &index)?;
        self.load_place_result(result, ty, &element_pointer)
    }

    /// Emits the proof-preserving wide probe: how many upcoming byte-walk
    /// iterations are provably no-ops.
    ///
    /// The window guard `index + 16 <= min(limit, length)` is internal, so
    /// the probe reads only bytes it proves in bounds and below the walk's
    /// exit bound. Every observable byte — a needle hit or the exit bound —
    /// stays with the unchanged scalar body. The lane-to-bit `bitcast` places
    /// lane 0 at the least
    /// significant bit on every supported (little-endian) target, so
    /// `cttz` yields the count of leading clean bytes.
    ///
    /// The walked run is either a boxed runtime-capacity `Array<u8>`, whose
    /// length is the `len` word of its block, or an inline `Array<u8, N>`,
    /// whose length is the type constant and is stored nowhere [TYPE-9,
    /// WIN-1]. Both are one contiguous extent starting at their first
    /// element, which is the whole of what this probe needs.
    pub(super) fn emit_buffer_probe_skip(
        &mut self,
        result: IrValueId,
        ty: IrType,
        buffer: IrValueId,
        index: IrValueId,
        limit: IrValueId,
        needles: &[IrValueId],
    ) -> Result<(), BackendFailure> {
        let u64_type = IrType::Integer {
            width: 64,
            signed: false,
        };
        let u8_type = IrType::Integer {
            width: 8,
            signed: false,
        };
        if ty != u64_type
            || self.value_type(index) != Some(u64_type)
            || self.value_type(limit) != Some(u64_type)
            || needles.is_empty()
            || needles.len() > 4
            || needles
                .iter()
                .any(|needle| self.value_type(*needle) != Some(u8_type))
        {
            return Err(BackendFailure::InvalidIr);
        }
        self.intrinsics.insert(IntrinsicDeclaration::UnaryWithFlag {
            name: "llvm.cttz.i16".to_owned(),
            ty: "i16".to_owned(),
        });
        // The length is read before the guard so both forms reach the same
        // window computation; a constant-capacity run's length is its type
        // constant and reads nothing.
        let (length, first_element) = match self.value_type(buffer) {
            Some(IrType::Address(IrAddressed::Buffer { element }))
                if self
                    .program
                    .element(element)
                    .ok_or(BackendFailure::InvalidIr)?
                    == u8_type =>
            {
                let block = IrType::Buffer { element };
                let address = self.addressed_storage_pointer(buffer)?;
                let length_address = self.aggregate_field_pointer(block, &address, LENGTH_FIELD)?;
                let length = self.next_temporary()?;
                writeln!(self.output, "  %{length} = load i64, ptr {length_address}")
                    .map_err(|_| BackendFailure::TextEmission)?;
                let zero = "0".to_owned();
                let first = self.buffer_element_pointer(block, &address, &zero)?;
                (format!("%{length}"), first)
            }
            Some(IrType::Address(IrAddressed::Array {
                element,
                length: count,
            })) if self.program.element(element) == Some(u8_type) => {
                (count.to_string(), self.value_name(buffer))
            }
            Some(IrType::Array {
                element,
                length: count,
            }) if self.program.element(element) == Some(u8_type) => {
                (count.to_string(), self.value_place(buffer)?)
            }
            _ => return Err(BackendFailure::InvalidIr),
        };
        let tighter = self.next_temporary()?;
        let window = self.next_temporary()?;
        let room = self.next_temporary()?;
        let edge = self.next_temporary()?;
        let fits = self.next_temporary()?;
        let address = self.next_temporary()?;
        let vector = self.next_temporary()?;
        {
            let emission_argument_0 = self.value_name(limit);
            let emission_argument_1 = self.value_name(limit);
            let emission_argument_2 = buffer_probe_room_label(result);
            let emission_argument_3 = buffer_probe_zero_label(result);
            let emission_argument_4 = buffer_probe_room_label(result);
            let emission_argument_5 = self.value_name(index);
            let emission_argument_6 = buffer_probe_load_label(result);
            let emission_argument_7 = buffer_probe_zero_label(result);
            let emission_argument_8 = buffer_probe_load_label(result);
            let emission_argument_9 = self.value_name(index);

            write!(self.output, "  %{tighter} = icmp ult i64 {emission_argument_0}, {length}\n  %{window} = select i1 %{tighter}, i64 {emission_argument_1}, i64 {length}\n  %{room} = icmp uge i64 %{window}, 16\n  br i1 %{room}, label %{emission_argument_2}, label %{emission_argument_3}\n").map_err(|_| BackendFailure::TextEmission)?;
            self.output.open_block(emission_argument_4.to_string());
            write!(self.output, "  %{edge} = sub i64 %{window}, 16\n  %{fits} = icmp ule i64 {emission_argument_5}, %{edge}\n  br i1 %{fits}, label %{emission_argument_6}, label %{emission_argument_7}\n").map_err(|_| BackendFailure::TextEmission)?;
            self.output.open_block(emission_argument_8.to_string());
            write!(self.output, "  %{address} = getelementptr inbounds i8, ptr {first_element}, i64 {emission_argument_9}\n  %{vector} = load <16 x i8>, ptr %{address}, align 1\n").map_err(|_| BackendFailure::TextEmission)?;
        };
        let mut accumulated: Option<String> = None;
        for needle in needles {
            let inserted = self.next_temporary()?;
            let splat = self.next_temporary()?;
            let equal = self.next_temporary()?;
            writeln!(
                self.output,
                "  %{inserted} = insertelement <16 x i8> poison, i8 {}, i64 0\n  %{splat} = shufflevector <16 x i8> %{inserted}, <16 x i8> poison, <16 x i32> zeroinitializer\n  %{equal} = icmp eq <16 x i8> %{vector}, %{splat}",
                self.value_name(*needle),
            )
            .map_err(|_| BackendFailure::TextEmission)?;
            accumulated = Some(match accumulated {
                None => equal,
                Some(previous) => {
                    let combined = self.next_temporary()?;
                    writeln!(
                        self.output,
                        "  %{combined} = or <16 x i1> %{previous}, %{equal}"
                    )
                    .map_err(|_| BackendFailure::TextEmission)?;
                    combined
                }
            });
        }
        let mask_lanes = accumulated.ok_or(BackendFailure::InvalidIr)?;
        let mask = self.next_temporary()?;
        let none = self.next_temporary()?;
        let zeros = self.next_temporary()?;
        let extended = self.next_temporary()?;
        {
            let emission_argument_0 = buffer_probe_clean_label(result);
            let emission_argument_1 = buffer_probe_found_label(result);
            let emission_argument_2 = buffer_probe_clean_label(result);
            let emission_argument_3 = buffer_probe_join_label(result);
            let emission_argument_4 = buffer_probe_found_label(result);
            let emission_argument_5 = buffer_probe_join_label(result);
            let emission_argument_6 = buffer_probe_zero_label(result);
            let emission_argument_7 = buffer_probe_join_label(result);
            let emission_argument_8 = buffer_probe_join_label(result);
            let emission_argument_9 = self.value_name(result);
            let emission_argument_10 = buffer_probe_clean_label(result);
            let emission_argument_11 = buffer_probe_found_label(result);
            let emission_argument_12 = buffer_probe_zero_label(result);

            write!(self.output, "  %{mask} = bitcast <16 x i1> %{mask_lanes} to i16\n  %{none} = icmp eq i16 %{mask}, 0\n  br i1 %{none}, label %{emission_argument_0}, label %{emission_argument_1}\n").map_err(|_| BackendFailure::TextEmission)?;
            self.output.open_block(emission_argument_2.to_string());
            writeln!(self.output, "  br label %{emission_argument_3}")
                .map_err(|_| BackendFailure::TextEmission)?;
            self.output.open_block(emission_argument_4.to_string());
            {
                self.output.symbol("llvm.cttz.i16");
                write!(
                    self.output,
                    "  %{zeros} = call i16 @llvm.cttz.i16(i16 %{mask}, i1 true)\n  %{extended} = zext i16 %{zeros} to i64\n  br label %{emission_argument_5}\n"
                )
            }?;
            self.output.open_block(emission_argument_6.to_string());
            writeln!(self.output, "  br label %{emission_argument_7}")
                .map_err(|_| BackendFailure::TextEmission)?;
            self.output.open_block(emission_argument_8.to_string());
            writeln!(self.output, "  {emission_argument_9} = phi i64 [ 16, %{emission_argument_10} ], [ %{extended}, %{emission_argument_11} ], [ 0, %{emission_argument_12} ]").map_err(|_| BackendFailure::TextEmission)?;
            Ok::<_, BackendFailure>(())
        }
    }
}

pub(super) fn buffer_fill_allocate_label(value: IrValueId) -> String {
    format!("buffer.fill.allocate.v{}", value.ordinal())
}

pub(super) fn buffer_fill_oom_label(value: IrValueId) -> String {
    format!("buffer.fill.oom.v{}", value.ordinal())
}

pub(super) fn buffer_fill_init_label(value: IrValueId) -> String {
    format!("buffer.fill.init.v{}", value.ordinal())
}

pub(super) fn buffer_fill_head_label(value: IrValueId) -> String {
    format!("buffer.fill.head.v{}", value.ordinal())
}

pub(super) fn buffer_fill_body_label(value: IrValueId) -> String {
    format!("buffer.fill.body.v{}", value.ordinal())
}

pub(super) fn buffer_fill_done_label(value: IrValueId) -> String {
    format!("buffer.fill.done.v{}", value.ordinal())
}

pub(super) fn buffer_probe_room_label(value: IrValueId) -> String {
    format!("buffer.probe.room.v{}", value.ordinal())
}

pub(super) fn buffer_probe_load_label(value: IrValueId) -> String {
    format!("buffer.probe.load.v{}", value.ordinal())
}

pub(super) fn buffer_probe_clean_label(value: IrValueId) -> String {
    format!("buffer.probe.clean.v{}", value.ordinal())
}

pub(super) fn buffer_probe_found_label(value: IrValueId) -> String {
    format!("buffer.probe.found.v{}", value.ordinal())
}

pub(super) fn buffer_probe_zero_label(value: IrValueId) -> String {
    format!("buffer.probe.zero.v{}", value.ordinal())
}

pub(super) fn buffer_probe_join_label(value: IrValueId) -> String {
    format!("buffer.probe.join.v{}", value.ordinal())
}
