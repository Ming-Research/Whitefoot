//! Emission of `Segments<T>` [TYPE-9] and its readers.
//!
//! The shape is one heap block `[len | bounds | elements]`
//! (compiler/storage-representation): `len` is the segment count, `bounds`
//! is `len + 1` element offsets beginning at zero, so segment i is the
//! elements `bounds[i] .. bounds[i + 1]`, and the elements begin at the first
//! byte offset past the bounds that the element type's alignment admits. As
//! for a runtime-capacity `Array`, the cell pointer is the block pointer, one
//! `malloc` builds it and one `free` reclaims it; its elements are copy
//! [OP-13], so the release walks nothing.
//!
//! Every operand below is the block's address.

use crate::{IrElement, IrLayoutCeiling, IrLayoutMagnitude};

use super::*;

/// The bound [OP-13]'s record states: a block is built exactly when
/// `stride_ceiling(T) * total + 8 * len <= 2^62`.
const SEGMENTS_LIMIT: u64 = 1 << 62;

/// The largest `len` whose bounds term alone stays within the limit.
const SEGMENTS_COUNT_LIMIT: u64 = SEGMENTS_LIMIT / 8;

/// The clamp of [`FunctionEmitter::emit_segments_total`]: a total at or above
/// it fails the fit whatever the stride.
const SEGMENTS_TOTAL_CLAMP: u64 = 1 << 63;

const U64: IrType = IrType::Integer {
    width: 64,
    signed: false,
};

/// The header type: `len` and the zero-length bounds tail.
const HEADER_TYPE: &str = "{ i64, [0 x i64] }";

impl<'program, 'state> FunctionEmitter<'program, 'state> {
    /// The block address one segments operand names, and its element.
    fn segments_block(&self, segments: IrValueId) -> Result<IrElement, BackendFailure> {
        match self.value_type(segments) {
            Some(IrType::Address(IrAddressed::Segments { element })) => Ok(element),
            _ => Err(BackendFailure::InvalidIr),
        }
    }

    /// The emitted element type and the constant expressions of its stride
    /// and its alignment.
    fn segments_element(
        &mut self,
        element: IrElement,
    ) -> Result<(String, String, String), BackendFailure> {
        let ty = self.output.type_name(
            self.program,
            self.program
                .element(element)
                .ok_or(BackendFailure::InvalidIr)?,
        )?;
        let stride = format!("ptrtoint (ptr getelementptr ({ty}, ptr null, i64 1) to i64)");
        let align =
            format!("ptrtoint (ptr getelementptr ({{ i1, {ty} }}, ptr null, i32 0, i32 1) to i64)");
        Ok((ty, stride, align))
    }

    /// The byte offset of the first element of a block of `count`
    /// segments: the `len` word and `count + 1` bounds, rounded up to the
    /// element's alignment.
    fn segments_header_bytes(
        &mut self,
        count: &str,
        align: &str,
    ) -> Result<String, BackendFailure> {
        let words = self.next_temporary()?;
        let bytes = self.next_temporary()?;
        let slack = self.next_temporary()?;
        let mask = self.next_temporary()?;
        let padded = self.next_temporary()?;
        let header = self.next_temporary()?;
        writeln!(
            self.output,
            "  %{words} = add nuw i64 {count}, 2\n  %{bytes} = shl nuw i64 %{words}, 3\n  %{slack} = sub i64 {align}, 1\n  %{mask} = sub i64 0, {align}\n  %{padded} = add nuw i64 %{bytes}, %{slack}\n  %{header} = and i64 %{padded}, %{mask}"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        Ok(format!("%{header}"))
    }

    /// The address of bound `index` of the block at `address`.
    fn segments_bound_pointer(
        &mut self,
        address: &str,
        index: &str,
    ) -> Result<String, BackendFailure> {
        let pointer = self.next_temporary()?;
        writeln!(
            self.output,
            "  %{pointer} = getelementptr inbounds {HEADER_TYPE}, ptr {address}, i64 0, i32 1, i64 {index}"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        Ok(format!("%{pointer}"))
    }

    /// The address of the first element of the block at `address`.
    fn segments_elements(&mut self, address: &str, align: &str) -> Result<String, BackendFailure> {
        let count = self.next_temporary()?;
        writeln!(self.output, "  %{count} = load i64, ptr {address}")
            .map_err(|_| BackendFailure::TextEmission)?;
        let header = self.segments_header_bytes(&format!("%{count}"), align)?;
        let elements = self.next_temporary()?;
        writeln!(
            self.output,
            "  %{elements} = getelementptr inbounds i8, ptr {address}, i64 {header}"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        Ok(format!("%{elements}"))
    }

    /// The pointer and count of one `{ ptr, i64 }` range descriptor.
    fn range_parts(&mut self, range: IrValueId) -> Result<(String, String), BackendFailure> {
        let pointer = self.next_temporary()?;
        let count = self.next_temporary()?;
        let name = self.value_name(range);
        writeln!(
            self.output,
            "  %{pointer} = extractvalue {{ ptr, i64 }} {name}, 0\n  %{count} = extractvalue {{ ptr, i64 }} {name}, 1"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        Ok((format!("%{pointer}"), format!("%{count}")))
    }

    /// The element type of a `&[u64]` operand, refused otherwise.
    fn lengths_operand(&self, lengths: IrValueId) -> Result<(), BackendFailure> {
        let Some(IrType::Range { element }) = self.value_type(lengths) else {
            return Err(BackendFailure::InvalidIr);
        };
        if self.program.element(element)
            != Some(IrType::Integer {
                width: 64,
                signed: false,
            })
        {
            return Err(BackendFailure::InvalidIr);
        }
        Ok(())
    }

    /// Emits a loop over the `count` lengths at `pointer` and returns the
    /// loop's exit label and the running sum at exit, in `width` bits. At
    /// each step `each(emitter, index, sum_before)` may emit more.
    fn emit_lengths_loop(
        &mut self,
        result: IrValueId,
        name: &str,
        pointer: &str,
        count: &str,
        width: u32,
        each: &mut dyn FnMut(&mut Self, &str, &str) -> Result<(), BackendFailure>,
    ) -> Result<String, BackendFailure> {
        let ordinal = result.ordinal();
        let start = format!("segments.{name}.start.v{ordinal}");
        let head = format!("segments.{name}.head.v{ordinal}");
        let body = format!("segments.{name}.body.v{ordinal}");
        let done = format!("segments.{name}.done.v{ordinal}");
        let index = self.next_temporary()?;
        let sum = self.next_temporary()?;
        let more = self.next_temporary()?;
        let next_index = self.next_temporary()?;
        let next_sum = self.next_temporary()?;
        writeln!(self.output, "  br label %{start}").map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(start.clone());
        writeln!(self.output, "  br label %{head}").map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(head.clone());
        writeln!(
            self.output,
            "  %{index} = phi i64 [ 0, %{start} ], [ %{next_index}, %{body} ]\n  %{sum} = phi i{width} [ 0, %{start} ], [ %{next_sum}, %{body} ]\n  %{more} = icmp ult i64 %{index}, {count}\n  br i1 %{more}, label %{body}, label %{done}"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(body.clone());
        each(self, &format!("%{index}"), &format!("%{sum}"))?;
        let at = self.next_temporary()?;
        let length = self.next_temporary()?;
        let widened = self.next_temporary()?;
        let extend = if width == 64 {
            format!("  %{widened} = add i64 %{length}, 0")
        } else {
            format!("  %{widened} = zext i64 %{length} to i{width}")
        };
        writeln!(
            self.output,
            "  %{at} = getelementptr inbounds i64, ptr {pointer}, i64 %{index}\n  %{length} = load i64, ptr %{at}\n{extend}\n  %{next_sum} = add nuw i{width} %{sum}, %{widened}\n  %{next_index} = add nuw i64 %{index}, 1\n  br label %{head}"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(done);
        Ok(format!("%{sum}"))
    }

    /// [OP-13] the sum of the lengths, taken in 128 bits, which no
    /// addressable run of lengths can overflow, and clamped to `2^63`, which
    /// already fails the fit.
    pub(super) fn emit_segments_total(
        &mut self,
        result: IrValueId,
        ty: IrType,
        lengths: IrValueId,
    ) -> Result<(), BackendFailure> {
        if ty
            != (IrType::Integer {
                width: 64,
                signed: false,
            })
        {
            return Err(BackendFailure::InvalidIr);
        }
        self.lengths_operand(lengths)?;
        let (pointer, count) = self.range_parts(lengths)?;
        let sum = self.emit_lengths_loop(
            result,
            "total",
            &pointer,
            &count,
            128,
            &mut |_, _, _| Ok(()),
        )?;
        let small = self.next_temporary()?;
        let narrow = self.next_temporary()?;
        writeln!(
            self.output,
            "  %{small} = icmp ult i128 {sum}, {SEGMENTS_TOTAL_CLAMP}\n  %{narrow} = trunc i128 {sum} to i64\n  {} = select i1 %{small}, i64 %{narrow}, i64 {SEGMENTS_TOTAL_CLAMP}",
            self.value_name(result)
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// [OP-13] whether `box_segments_filled` builds a block: with t the total
    /// and n the count of lengths, `stride_ceiling(T) * t + 8 * n <= 2^62`,
    /// judged with the language ceiling so that every qualified target gives
    /// the same answer.
    pub(super) fn emit_segments_fits(
        &mut self,
        result: IrValueId,
        ty: IrType,
        lengths: IrValueId,
        total: IrValueId,
        ceiling: IrLayoutCeiling,
    ) -> Result<(), BackendFailure> {
        if ty != IrType::Bool || self.value_type(total) != Some(U64) {
            return Err(BackendFailure::InvalidIr);
        }
        self.lengths_operand(lengths)?;
        let (_, count) = self.range_parts(lengths)?;
        // `floor(2^62 / c)` is the whole budget of a stride ceiling above
        // u64, which is zero, or a finite one.
        let stride = match ceiling.stride {
            IrLayoutMagnitude::Finite(stride) => stride.max(1),
            IrLayoutMagnitude::AboveU64 => u64::MAX,
        };
        let few = self.next_temporary()?;
        let clamped = self.next_temporary()?;
        let bounds = self.next_temporary()?;
        let room = self.next_temporary()?;
        let budget = self.next_temporary()?;
        let within = self.next_temporary()?;
        writeln!(
            self.output,
            "  %{few} = icmp ule i64 {count}, {SEGMENTS_COUNT_LIMIT}\n  %{clamped} = select i1 %{few}, i64 {count}, i64 {SEGMENTS_COUNT_LIMIT}\n  %{bounds} = shl nuw i64 %{clamped}, 3\n  %{room} = sub nuw i64 {SEGMENTS_LIMIT}, %{bounds}\n  %{budget} = udiv i64 %{room}, {stride}\n  %{within} = icmp ule i64 {}, %{budget}\n  {} = and i1 %{few}, %{within}",
            self.value_name(total),
            self.value_name(result)
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// [OP-13] `box_segments_filled` after [`Self::emit_segments_fits`] held:
    /// the block, its `len`, its bounds as the running sums of the lengths,
    /// and every element holding the supplied copy value.
    ///
    /// The fit bounds the total and the count, so no sum, product or offset
    /// below wraps: the block is at most `2^62 + 31` bytes.
    pub(super) fn emit_segments_fill(
        &mut self,
        result: IrValueId,
        ty: IrType,
        nominal: IrNominalId,
        lengths: IrValueId,
        total: IrValueId,
        value: IrValueId,
    ) -> Result<(), BackendFailure> {
        if ty != IrType::Nominal(nominal) || self.value_type(total) != Some(U64) {
            return Err(BackendFailure::InvalidIr);
        }
        let IrNominalKind::Box { referent, .. } = self.nominal(nominal)?.kind() else {
            return Err(BackendFailure::InvalidIr);
        };
        let IrType::Segments { element } = *referent else {
            return Err(BackendFailure::InvalidIr);
        };
        let element_ir = self
            .program
            .element(element)
            .ok_or(BackendFailure::InvalidIr)?;
        if self.value_type(value) != Some(element_ir) {
            return Err(BackendFailure::InvalidIr);
        }
        self.lengths_operand(lengths)?;
        let (_, stride, align) = self.segments_element(element)?;
        let (pointer, count) = self.range_parts(lengths)?;
        let total = self.value_name(total);
        let header = self.segments_header_bytes(&count, &align)?;
        let element_bytes = self.next_temporary()?;
        let bytes = self.next_temporary()?;
        let nonnull = self.next_temporary()?;
        let ordinal = result.ordinal();
        let oom = format!("segments.fill.oom.v{ordinal}");
        let init = format!("segments.fill.init.v{ordinal}");
        let address = self.value_name(result);
        self.output.symbol("malloc");
        writeln!(
            self.output,
            "  %{element_bytes} = mul nuw i64 {total}, {stride}\n  %{bytes} = add nuw i64 %{element_bytes}, {header}\n  {address} = call ptr @malloc(i64 %{bytes})\n  %{nonnull} = icmp ne ptr {address}, null\n  br i1 %{nonnull}, label %{init}, label %{oom}"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(oom);
        self.output.symbol("wf_resource_abort");
        writeln!(
            self.output,
            "  call void @wf_resource_abort()\n  unreachable"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(init);
        writeln!(self.output, "  store i64 {count}, ptr {address}")
            .map_err(|_| BackendFailure::TextEmission)?;
        // Bound k is the sum of the lengths before segment k.
        let block = address.clone();
        let last = self.emit_lengths_loop(
            result,
            "bounds",
            &pointer,
            &count,
            64,
            &mut |emitter, index, before| {
                let bound = emitter.segments_bound_pointer(&block, index)?;
                writeln!(emitter.output, "  store i64 {before}, ptr {bound}")
                    .map_err(|_| BackendFailure::TextEmission)
            },
        )?;
        let final_bound = self.segments_bound_pointer(&address, &count)?;
        writeln!(self.output, "  store i64 {last}, ptr {final_bound}")
            .map_err(|_| BackendFailure::TextEmission)?;
        let elements = self.next_temporary()?;
        writeln!(
            self.output,
            "  %{elements} = getelementptr inbounds i8, ptr {address}, i64 {header}"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        let start = format!("segments.fill.start.v{ordinal}");
        let head = format!("segments.fill.head.v{ordinal}");
        let body = format!("segments.fill.body.v{ordinal}");
        let done = format!("segments.fill.done.v{ordinal}");
        let index = self.next_temporary()?;
        let more = self.next_temporary()?;
        let next_index = self.next_temporary()?;
        writeln!(self.output, "  br label %{start}").map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(start.clone());
        writeln!(self.output, "  br label %{head}").map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(head.clone());
        writeln!(
            self.output,
            "  %{index} = phi i64 [ 0, %{start} ], [ %{next_index}, %{body} ]\n  %{more} = icmp ult i64 %{index}, {last}\n  br i1 %{more}, label %{body}, label %{done}"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(body.clone());
        let slot = self.segments_element_pointer(
            element_ir,
            &format!("%{elements}"),
            &format!("%{index}"),
        )?;
        self.store_value_at(value, &slot)?;
        writeln!(
            self.output,
            "  %{next_index} = add nuw i64 %{index}, 1\n  br label %{head}"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        self.output.open_block(done);
        Ok(())
    }

    /// The address of element `index` of the run beginning at `elements`.
    fn segments_element_pointer(
        &mut self,
        element: IrType,
        elements: &str,
        index: &str,
    ) -> Result<String, BackendFailure> {
        let emitted = self.output.type_name(self.program, element)?;
        let index = self.element_address_index(element, index)?.to_owned();
        let pointer = self.next_temporary()?;
        writeln!(
            self.output,
            "  %{pointer} = getelementptr inbounds {emitted}, ptr {elements}, i64 {index}"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        Ok(format!("%{pointer}"))
    }

    /// [MSR-1] a `Segments` block's one measure: the `len` word heading it.
    pub(super) fn emit_segments_measure(
        &mut self,
        result: IrValueId,
        ty: IrType,
        segments: IrValueId,
    ) -> Result<(), BackendFailure> {
        if ty
            != (IrType::Integer {
                width: 64,
                signed: false,
            })
        {
            return Err(BackendFailure::InvalidIr);
        }
        self.segments_block(segments)?;
        writeln!(
            self.output,
            "  {} = load i64, ptr {}",
            self.value_name(result),
            self.value_name(segments)
        )
        .map_err(|_| BackendFailure::TextEmission)
    }

    /// [REF-4] the range reference over segment `index`: the elements from
    /// bound `index` to bound `index + 1`. [OP-4] discharged `index < len`,
    /// so both bounds are inside the block.
    pub(super) fn emit_segment_slice(
        &mut self,
        result: IrValueId,
        ty: IrType,
        segments: IrValueId,
        index: IrValueId,
    ) -> Result<(), BackendFailure> {
        let element = self.segments_block(segments)?;
        if ty != (IrType::Range { element })
            || self.value_type(index)
                != Some(IrType::Integer {
                    width: 64,
                    signed: false,
                })
        {
            return Err(BackendFailure::InvalidIr);
        }
        let element_ir = self
            .program
            .element(element)
            .ok_or(BackendFailure::InvalidIr)?;
        let (_, _, align) = self.segments_element(element)?;
        let address = self.value_name(segments);
        let offset = self.value_name(index);
        let low_pointer = self.segments_bound_pointer(&address, &offset)?;
        let next = self.next_temporary()?;
        writeln!(self.output, "  %{next} = add nuw i64 {offset}, 1")
            .map_err(|_| BackendFailure::TextEmission)?;
        let high_pointer = self.segments_bound_pointer(&address, &format!("%{next}"))?;
        let low = self.next_temporary()?;
        let high = self.next_temporary()?;
        let length = self.next_temporary()?;
        writeln!(
            self.output,
            "  %{low} = load i64, ptr {low_pointer}\n  %{high} = load i64, ptr {high_pointer}\n  %{length} = sub nuw i64 %{high}, %{low}"
        )
        .map_err(|_| BackendFailure::TextEmission)?;
        let elements = self.segments_elements(&address, &align)?;
        let first = self.segments_element_pointer(element_ir, &elements, &format!("%{low}"))?;
        self.emit_range_descriptor(result, ty, &first, &format!("%{length}"))
    }

    /// [REF-4] the range reference over every element in segment order:
    /// the run from the first element to bound `len`.
    pub(super) fn emit_segments_all(
        &mut self,
        result: IrValueId,
        ty: IrType,
        segments: IrValueId,
    ) -> Result<(), BackendFailure> {
        let element = self.segments_block(segments)?;
        if ty != (IrType::Range { element }) {
            return Err(BackendFailure::InvalidIr);
        }
        let (_, _, align) = self.segments_element(element)?;
        let address = self.value_name(segments);
        let count = self.next_temporary()?;
        writeln!(self.output, "  %{count} = load i64, ptr {address}")
            .map_err(|_| BackendFailure::TextEmission)?;
        let last_pointer = self.segments_bound_pointer(&address, &format!("%{count}"))?;
        let total = self.next_temporary()?;
        writeln!(self.output, "  %{total} = load i64, ptr {last_pointer}")
            .map_err(|_| BackendFailure::TextEmission)?;
        let elements = self.segments_elements(&address, &align)?;
        self.emit_range_descriptor(result, ty, &elements, &format!("%{total}"))
    }

    fn emit_range_descriptor(
        &mut self,
        result: IrValueId,
        ty: IrType,
        pointer: &str,
        length: &str,
    ) -> Result<(), BackendFailure> {
        let descriptor = self.output.type_name(self.program, ty)?;
        let partial = self.next_temporary()?;
        writeln!(
            self.output,
            "  %{partial} = insertvalue {descriptor} zeroinitializer, ptr {pointer}, 0\n  {} = insertvalue {descriptor} %{partial}, i64 {length}, 1",
            self.value_name(result)
        )
        .map_err(|_| BackendFailure::TextEmission)
    }
}
