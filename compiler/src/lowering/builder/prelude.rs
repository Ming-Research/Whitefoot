//! The bodies of the [PRE-1] records the compiler itself owns.
//!
//! Nine construction functions [OP-13], nine window operations [OP-10],
//! `swap` [OP-11] and `free_empty` [OP-14] are declared body-less exactly as
//! a host function is [PRE-2], but no trusted-base object defines them: the compiler emits
//! their bodies. Each body is built here, at the row's own physical function
//! instance, so one monomorphized instance serves every call of that row with
//! those type arguments and the ordinary call ABI carries the operands.
//!
//! Building the body at the instance rather than expanding it at each call is
//! what keeps the window operations one implementation: the checker has
//! already discharged every bound and every window contract at the call
//! [OP-4, FN-8], so the body performs no check of its own and needs nothing
//! from the call site but its arguments.

use crate::{IrAddressed, IrBoundary, IrWindowShape};

use super::*;

/// Which compiler-owned [PRE-1] record a body-less function is.
///
/// The host functions [PRE-2] are deliberately absent: those are body-less
/// because the trusted base defines them, and calling one emits an ordinary
/// external call.
pub(super) fn compiler_owned_row(name: &str) -> bool {
    crate::lowering::COMPILER_OWNED_PRELUDE_ROWS.contains(&name)
}

const U64: IrType = IrType::Integer {
    width: 64,
    signed: false,
};

impl IrBuilder<'_> {
    /// Builds the body of one compiler-owned [PRE-1] record, or reports the
    /// row as an unimplemented capability.
    ///
    /// The parameters are already declared; this adds the instructions and
    /// the return.
    pub(super) fn lower_prelude_row(&mut self, name: &str) -> Result<(), LoweringFailure> {
        match name {
            "box_new" => self.row_box_new(),
            "array_filled" => self.row_array_filled(),
            "slots_new" | "ring_new" => self.row_window_new(),
            "slots_from_array" | "slots_into_array" => self.row_full_array_conversion(),
            "box_array_filled" => self.row_box_array_filled(),
            "box_slots_new" | "box_ring_new" => self.row_box_window_new(),
            "grow" => self.row_grow(),
            "place_back" => self.row_place(IrBoundary::PlaceBack),
            "place_front" => self.row_place(IrBoundary::PlaceFront),
            "take_back" => self.row_take(IrBoundary::TakeBack),
            "take_front" => self.row_take(IrBoundary::TakeFront),
            "insert_at" => self.row_insert_at(),
            "remove_at" => self.row_remove_at(),
            "append" => self.row_append(),
            "split_off" => self.row_split_off(),
            "swap" => self.row_swap(),
            "free_empty" => self.row_free_empty(),
            _ => Err(LoweringFailure::UnimplementedPreludeRow(
                crate::lowering::COMPILER_OWNED_PRELUDE_ROWS
                    .iter()
                    .find(|row| **row == name)
                    .copied()
                    .unwrap_or("prelude row"),
            )),
        }
    }

    /// The row's parameters, refused when the count does not match what the
    /// [PRE-1] record declares.
    fn row_parameters<const N: usize>(&self) -> Result<[IrValueId; N], LoweringFailure> {
        let mut values = [IrValueId(0); N];
        if self.parameters.len() != N {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        for (slot, (value, _)) in values.iter_mut().zip(&self.parameters) {
            *slot = *value;
        }
        Ok(values)
    }

    fn return_value(&mut self, value: IrValueId) -> Result<(), LoweringFailure> {
        if self.value_type(value)? != self.result {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        self.terminate(IrTerminator::Return {
            value,
            drops: Vec::new(),
        })
    }

    /// A row whose declared result is `own unit` returns the one unit value.
    fn return_unit(&mut self) -> Result<(), LoweringFailure> {
        if self.result != IrType::Unit {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        let unit = self.define(IrType::Unit, IrOperation::Constant(IrConstant::Unit))?;
        self.terminate(IrTerminator::Return {
            value: unit,
            drops: Vec::new(),
        })
    }

    /// The window a `&W` parameter addresses [OP-10]: its shape, element and
    /// placement.
    fn window_parameter(
        &self,
        value: IrValueId,
    ) -> Result<(IrWindowShape, IrElement, Option<u64>), LoweringFailure> {
        match self.value_type(value)? {
            IrType::Address(IrAddressed::Window {
                shape,
                element,
                capacity,
            }) => Ok((shape, element, capacity)),
            _ => Err(LoweringFailure::InvalidCheckedProgram),
        }
    }

    // ---- [OP-13] construction ------------------------------------------

    /// `box_new<T>(value: T) -> Box<T>`: one cell holding the value.
    fn row_box_new(&mut self) -> Result<(), LoweringFailure> {
        let [value] = self.row_parameters()?;
        let IrType::Nominal(nominal) = self.result else {
            return Err(LoweringFailure::InvalidCheckedProgram);
        };
        let cell = self.define(self.result, IrOperation::BoxNew { nominal, value })?;
        self.return_value(cell)
    }

    /// `array_filled<T, n>(value: T) -> Array<T, n>`: every slot
    /// holds the supplied value, which [OP-13] requires to be copy.
    fn row_array_filled(&mut self) -> Result<(), LoweringFailure> {
        let [value] = self.row_parameters()?;
        let filled = self.define(
            self.result,
            IrOperation::ArrayFill {
                value,
                target_domain: IrTargetDomainObligation::ElementAddress,
            },
        )?;
        self.return_value(filled)
    }

    /// `slots_new<T, n>()` and `ring_new<T, n>()`: the empty window over `n`
    /// raw slots, whose descriptor words are all zero [OP-13, WIN-1].
    fn row_window_new(&mut self) -> Result<(), LoweringFailure> {
        let [] = self.row_parameters()?;
        let window = self.define(self.result, IrOperation::Window)?;
        self.return_value(window)
    }

    /// `slots_from_array` and `slots_into_array`: the consuming conversion
    /// between a full fixed run and its dense array [OP-13].
    fn row_full_array_conversion(&mut self) -> Result<(), LoweringFailure> {
        let [value] = self.row_parameters()?;
        let converted = self.define(self.result, IrOperation::FullArrayConversion { value })?;
        self.return_value(converted)
    }

    /// `box_array_filled<T>(count, value) -> Box<Array<T>>`: one heap
    /// block `[len | elements]`, filled, which is the cell itself
    /// (compiler/storage-representation).
    ///
    /// [TYPE-9] stores the content "in exactly one heap object the `Box`
    /// value owns" and [STOR-3] reclaims it with "one compiler-derived heap
    /// free", so the header and the elements are one allocation and the cell
    /// pointer is the block pointer. A descriptor cell beside a separate
    /// element block would spend a second `malloc`, a second `free`, and a
    /// second word kept live through loops that read none of it.
    fn row_box_array_filled(&mut self) -> Result<(), LoweringFailure> {
        let [count, value] = self.row_parameters()?;
        let IrType::Nominal(nominal) = self.result else {
            return Err(LoweringFailure::InvalidCheckedProgram);
        };
        let IrNominalKind::Box { referent, .. } = self
            .nominals
            .get(nominal.index())
            .ok_or(LoweringFailure::InvalidCheckedProgram)?
            .kind
        else {
            return Err(LoweringFailure::InvalidCheckedProgram);
        };
        let IrType::Buffer { element } = referent else {
            return Err(LoweringFailure::InvalidCheckedProgram);
        };
        let obligations = self.runtime_obligations(self.element_type(element)?)?;
        let cell = self.define(
            self.result,
            IrOperation::BufferFill {
                nominal,
                length: count,
                value,
                layout_ceiling: obligations.layout_ceiling,
                target_domains: obligations.target_domains,
            },
        )?;
        self.return_value(cell)
    }

    /// `box_slots_new<T>(capacity)` and `box_ring_new<T>(capacity)`: one
    /// heap block `[len | cap | head? | slots]` whose window is empty
    /// (compiler/storage-representation).
    fn row_box_window_new(&mut self) -> Result<(), LoweringFailure> {
        let [capacity] = self.row_parameters()?;
        if self.value_type(capacity)? != U64 {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        let IrType::Nominal(nominal) = self.result else {
            return Err(LoweringFailure::InvalidCheckedProgram);
        };
        let IrNominalKind::Box { referent, .. } = self
            .nominals
            .get(nominal.index())
            .ok_or(LoweringFailure::InvalidCheckedProgram)?
            .kind
        else {
            return Err(LoweringFailure::InvalidCheckedProgram);
        };
        let IrType::Window {
            element,
            capacity: None,
            ..
        } = referent
        else {
            return Err(LoweringFailure::InvalidCheckedProgram);
        };
        let element_type = self.element_type(element)?;
        let obligations = self.runtime_obligations(element_type)?;
        let cell = self.define(
            self.result,
            IrOperation::WindowBlockNew {
                nominal,
                capacity,
                obligations,
            },
        )?;
        self.return_value(cell)
    }

    /// The target-domain record one runtime-capacity allocation carries
    /// [OP-9, STOR-6].
    ///
    /// [OP-9]'s own predicate is the retained bound: an accepted site proved
    /// `n <= floor((2^64 - 1) / stride_ceiling(T))`, and target
    /// qualification separately requires the actual stride to be no larger
    /// than that ceiling, so the product of this bound and the actual stride
    /// is representable.
    fn runtime_obligations(
        &self,
        element: IrType,
    ) -> Result<crate::IrAllocationObligations, LoweringFailure> {
        let ceiling = layout_ceiling(self.nominals, self.elements, element)
            .ok_or(LoweringFailure::InvalidCheckedProgram)?;
        let maximum_count = match ceiling.stride {
            crate::IrLayoutMagnitude::Finite(stride) => u64::MAX / stride.max(1),
            crate::IrLayoutMagnitude::AboveU64 => 0,
        };
        Ok(crate::IrAllocationObligations {
            layout_ceiling: ceiling,
            target_domains: IrRuntimeTargetObligations::from_language_ceiling(maximum_count),
        })
    }

    /// `grow<T>(cell: &Box<Slots<T>>, capacity)`: the cell's content is
    /// remade whole at the new capacity [OP-10].
    fn row_grow(&mut self) -> Result<(), LoweringFailure> {
        let [cell, capacity] = self.row_parameters()?;
        if self.value_type(capacity)? != U64 {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        let IrType::Address(IrAddressed::Nominal(nominal)) = self.value_type(cell)? else {
            return Err(LoweringFailure::InvalidCheckedProgram);
        };
        let IrNominalKind::Box { referent, .. } = self
            .nominals
            .get(nominal.index())
            .ok_or(LoweringFailure::InvalidCheckedProgram)?
            .kind
        else {
            return Err(LoweringFailure::InvalidCheckedProgram);
        };
        let IrType::Window {
            element,
            capacity: None,
            ..
        } = referent
        else {
            return Err(LoweringFailure::InvalidCheckedProgram);
        };
        let element_type = self.element_type(element)?;
        let obligations = self.runtime_obligations(element_type)?;
        self.define(
            IrType::Unit,
            IrOperation::WindowGrow {
                nominal,
                cell,
                capacity,
                obligations,
            },
        )?;
        self.return_unit()
    }

    // ---- [OP-10] the window operations ---------------------------------

    /// `place_back` and `place_front`: fill the boundary slot and move the
    /// boundary by one.
    fn row_place(&mut self, row: IrBoundary) -> Result<(), LoweringFailure> {
        let [window, value] = self.row_parameters()?;
        let (_, element, _) = self.window_parameter(window)?;
        if self.value_type(value)? != self.element_type(element)? {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        let placed = self.define(
            IrType::Unit,
            IrOperation::RunBoundary {
                row,
                run: window,
                value: Some(value),
            },
        )?;
        self.return_value(placed)
    }

    /// `take_back` and `take_front`: one complete [OP-10] operation captures
    /// the old boundary slot, moves the boundary and hands the element back.
    fn row_take(&mut self, row: IrBoundary) -> Result<(), LoweringFailure> {
        let [window] = self.row_parameters()?;
        let (_, element, _) = self.window_parameter(window)?;
        let element_type = self.element_type(element)?;
        if self.result != element_type {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        let taken = self.define(element_type, IrOperation::RunTaken { row, run: window })?;
        self.return_value(taken)
    }

    /// `insert_at(window, index, value)`: one shift of `window.filled` up by
    /// one from `index`, then the placement at the opened slot and the
    /// boundary move [OP-10].
    fn row_insert_at(&mut self) -> Result<(), LoweringFailure> {
        let [window, index, value] = self.row_parameters()?;
        let (_, element, _) = self.window_parameter(window)?;
        if self.value_type(index)? != U64
            || self.value_type(value)? != self.element_type(element)?
        {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        self.define(
            IrType::Unit,
            IrOperation::RunShift {
                run: window,
                index,
                open: true,
            },
        )?;
        let placed = self.define(
            IrType::Unit,
            IrOperation::RunInsert {
                run: window,
                index,
                value,
            },
        )?;
        self.return_value(placed)
    }

    /// `remove_at(window, index)`: the element at `index` is read out, then
    /// `window.filled` closes over its slot and the boundary moves down.
    fn row_remove_at(&mut self) -> Result<(), LoweringFailure> {
        let [window, index] = self.row_parameters()?;
        let (_, element, _) = self.window_parameter(window)?;
        let element_type = self.element_type(element)?;
        if self.value_type(index)? != U64 || self.result != element_type {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        let taken = self.define(
            element_type,
            IrOperation::RunIndex {
                run: window,
                offset: index,
                target_domain: IrTargetDomainObligation::ElementAddress,
            },
        )?;
        self.define(
            IrType::Unit,
            IrOperation::RunShift {
                run: window,
                index,
                open: false,
            },
        )?;
        self.return_value(taken)
    }

    /// `append(destination, source)`: every element of `source` moves to
    /// `destination.free`, and both boundaries move [OP-10].
    fn row_append(&mut self) -> Result<(), LoweringFailure> {
        let [destination, source] = self.row_parameters()?;
        let (_, destination_element, _) = self.window_parameter(destination)?;
        let (_, source_element, _) = self.window_parameter(source)?;
        if self.element_type(destination_element)? != self.element_type(source_element)? {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        let zero = self.lower_fixed_measure(0)?;
        let transferred = self.define(
            IrType::Unit,
            IrOperation::RunTransfer {
                destination,
                source,
                index: zero,
            },
        )?;
        self.return_value(transferred)
    }

    /// `split_off(source, index, destination)`: the tail of `source` from
    /// `index` moves to `destination.free`, `source.len` becomes `index`,
    /// and `destination.len` grows by the moved count [OP-10].
    fn row_split_off(&mut self) -> Result<(), LoweringFailure> {
        let [source, index, destination] = self.row_parameters()?;
        let (_, destination_element, _) = self.window_parameter(destination)?;
        let (_, source_element, _) = self.window_parameter(source)?;
        if self.value_type(index)? != U64
            || self.element_type(destination_element)? != self.element_type(source_element)?
        {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        let transferred = self.define(
            IrType::Unit,
            IrOperation::RunTransfer {
                destination,
                source,
                index,
            },
        )?;
        self.return_value(transferred)
    }

    // ---- [OP-11] and [OP-14] -------------------------------------------

    /// `swap(first: &T, second: &T)`: the two stored values are exchanged.
    ///
    /// Both values are read before either is written, so the one call
    /// [OP-11] admits whose two arguments name the same place leaves that
    /// place holding what it held.
    fn row_swap(&mut self) -> Result<(), LoweringFailure> {
        let [first, second] = self.row_parameters()?;
        let (IrType::Address(referent), IrType::Address(other)) =
            (self.value_type(first)?, self.value_type(second)?)
        else {
            return Err(LoweringFailure::InvalidCheckedProgram);
        };
        if referent != other {
            return Err(LoweringFailure::InvalidCheckedProgram);
        }
        let held_first = self.define(
            referent.ty(),
            IrOperation::Load {
                address: first,
                referent,
            },
        )?;
        let held_second = self.define(
            referent.ty(),
            IrOperation::Load {
                address: second,
                referent,
            },
        )?;
        self.store_addressed(first, held_second, referent)?;
        self.store_addressed(second, held_first, referent)?;
        self.return_unit()
    }

    /// `free_empty(window: W)`: the storage of a window proved empty is
    /// consumed [OP-14].
    ///
    /// The window holds no element, so nothing is released inside it; what
    /// remains is the block's own storage, which is a frame slot for a
    /// constant-capacity shape and the cell for a boxed runtime-capacity
    /// one.
    fn row_free_empty(&mut self) -> Result<(), LoweringFailure> {
        let [window] = self.row_parameters()?;
        match self.value_type(window)? {
            IrType::Nominal(nominal) => {
                let IrNominalKind::Box { .. } = self
                    .nominals
                    .get(nominal.index())
                    .ok_or(LoweringFailure::InvalidCheckedProgram)?
                    .kind
                else {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                };
                self.define(
                    IrType::Unit,
                    IrOperation::CellFree {
                        nominal,
                        value: window,
                    },
                )?;
            }
            // A constant-capacity window is frame-resident [STOR-1] and
            // reclaims no storage of its own.
            IrType::Window {
                capacity: Some(_), ..
            } => {}
            _ => return Err(LoweringFailure::InvalidCheckedProgram),
        }
        self.return_unit()
    }
}

/// [OP-9]'s language layout ceiling of one stored type.
///
/// The arithmetic is over unbounded mathematical integers and is the rule's
/// own table; it names no target ABI value and has the same result for one
/// source type on every qualified target.
pub(crate) fn layout_ceiling(
    nominals: &[IrNominal],
    elements: &[IrType],
    ty: IrType,
) -> Option<IrLayoutCeiling> {
    let (size, align) = ceiling_pair(nominals, elements, ty, &mut HashSet::new())?;
    Some(IrLayoutCeiling {
        size,
        align,
        stride: match size {
            crate::IrLayoutMagnitude::Finite(0) => crate::IrLayoutMagnitude::Finite(1),
            size => size,
        },
    })
}

/// The `(size_ceiling, align_ceiling)` pair [OP-9] fixes for one type.
fn ceiling_pair(
    nominals: &[IrNominal],
    elements: &[IrType],
    ty: IrType,
    visiting: &mut HashSet<IrNominalId>,
) -> Option<(crate::IrLayoutMagnitude, u64)> {
    use crate::IrLayoutMagnitude::{AboveU64, Finite};

    Some(match ty {
        IrType::Unit | IrType::Bool | IrType::Integer { width: 8, .. } => (Finite(1), 1),
        IrType::Integer { width: 16, .. } => (Finite(2), 2),
        IrType::Integer { width: 32, .. } | IrType::Float { width: 32 } => (Finite(4), 4),
        IrType::Integer { width: 64, .. } | IrType::Float { width: 64 } => (Finite(8), 8),
        IrType::Integer { .. } | IrType::Float { .. } => return None,
        // A range reference is not a stored type [TYPE-8]; a runtime-capacity
        // `Array<T>` is a pointer and a length.
        IrType::Buffer { .. } | IrType::Range { .. } => (Finite(16), 8),
        IrType::Address(_) => (Finite(8), 8),
        // This compiler-only task capture has no source layout ceiling.
        IrType::RuntimeBoxPayload { .. } => return None,
        IrType::Array { element, length } => {
            if length == 0 {
                return Some((Finite(0), 1));
            }
            let (size, align) = ceiling_pair(
                nominals,
                elements,
                *elements.get(element.index())?,
                visiting,
            )?;
            let size = match size {
                Finite(size) => size.checked_mul(length).map_or(AboveU64, Finite),
                AboveU64 => AboveU64,
            };
            (size, align)
        }
        IrType::Window {
            shape,
            element,
            capacity,
        } => {
            let words = u64::from(shape == IrWindowShape::Ring) + 1;
            match capacity {
                // A runtime-capacity `Slots<T>` is a pointer, a capacity and
                // a length; a `Ring<T>` adds a window origin.
                None => (Finite(8 * (words + 2)), 8),
                Some(0) => (Finite(8 * words), 8),
                Some(length) => {
                    let (size, align) = ceiling_pair(
                        nominals,
                        elements,
                        *elements.get(element.index())?,
                        visiting,
                    )?;
                    let slots = match size {
                        Finite(size) => size.checked_mul(length).map_or(AboveU64, Finite),
                        AboveU64 => AboveU64,
                    };
                    let align = align.max(8);
                    (
                        round_up(add_ceiling(round_up(slots, 8)?, Finite(8 * words)), align)?,
                        align,
                    )
                }
            }
        }
        IrType::Nominal(id) => {
            // The checked table rejects inline cycles. Track the actual
            // expansion path rather than imposing a layout nesting limit;
            // Box and runtime-capacity shells do not expand their content.
            if !visiting.insert(id) {
                return None;
            }
            let nominal = nominals.get(id.index())?;
            let pair = match nominal.kind() {
                // One pointer; the content lives in the heap object and
                // enters no sequence.
                IrNominalKind::Box { .. } => (Finite(8), 8),
                // Every fieldless opaque struct carries the host handles'
                // host-supplied representation.
                IrNominalKind::Opaque => (Finite(32), 16),
                IrNominalKind::Struct { fields } => {
                    sequence(nominals, elements, fields.iter().map(IrField::ty), visiting)?
                }
                IrNominalKind::Enum { variants } => {
                    if variants.iter().all(|variant| variant.fields().is_empty()) {
                        if variants.len() <= 2 {
                            (Finite(1), 1)
                        } else {
                            (Finite(4), 4)
                        }
                    } else {
                        let payload = variants
                            .iter()
                            .flat_map(|variant| variant.fields().iter().map(IrField::ty))
                            .collect::<Vec<_>>();
                        sequence(
                            nominals,
                            elements,
                            std::iter::once(IrType::Integer {
                                width: 32,
                                signed: false,
                            })
                            .chain(payload),
                            visiting,
                        )?
                    }
                }
            };
            visiting.remove(&id);
            pair
        }
    })
}

/// [OP-9]'s sequence rule over a field order.
fn sequence(
    nominals: &[IrNominal],
    elements: &[IrType],
    fields: impl Iterator<Item = IrType>,
    visiting: &mut HashSet<IrNominalId>,
) -> Option<(crate::IrLayoutMagnitude, u64)> {
    let mut offset = crate::IrLayoutMagnitude::Finite(0);
    let mut alignment = 1_u64;
    for field in fields {
        let (size, align) = ceiling_pair(nominals, elements, field, visiting)?;
        offset = add_ceiling(round_up(offset, align)?, size);
        alignment = alignment.max(align);
    }
    Some((round_up(offset, alignment)?, alignment))
}

fn add_ceiling(
    left: crate::IrLayoutMagnitude,
    right: crate::IrLayoutMagnitude,
) -> crate::IrLayoutMagnitude {
    use crate::IrLayoutMagnitude::{AboveU64, Finite};

    match (left, right) {
        (Finite(left), Finite(right)) => left.checked_add(right).map_or(AboveU64, Finite),
        _ => AboveU64,
    }
}

fn round_up(value: crate::IrLayoutMagnitude, alignment: u64) -> Option<crate::IrLayoutMagnitude> {
    use crate::IrLayoutMagnitude::{AboveU64, Finite};

    if alignment == 0 {
        return None;
    }
    Some(match value {
        Finite(value) => value
            .checked_add(alignment - 1)
            .map_or(AboveU64, |raised| Finite(raised / alignment * alignment)),
        AboveU64 => AboveU64,
    })
}
