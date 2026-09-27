//! The internal function ABI, shared by definitions and every call route.
//!
//! This module describes value representation: which parameters and results
//! travel as values and which travel as a pointer to their storage. A
//! declaration and a definition use the same ABI.
//!
//! A `&[T]` range reference [REF-4] is the one value that crosses a call
//! boundary as two arguments: its element pointer and its count. Inside a
//! body it stays the `{ ptr, i64 }` pair compiler/storage-representation
//! selects, and every admitted target already passes that aggregate's two
//! words as two independent arguments, so the split changes no machine
//! calling convention. It exists so that the pointer can carry the facts
//! below, which an LLVM aggregate parameter cannot.
//!
//! A stored aggregate result crosses the boundary as its LLVM first-class
//! value when every scalar leaf of that value gets its own return register
//! on every admitted target (see [`fits_return_registers`]). Any larger
//! stored aggregate is constructed through the caller's destination
//! pointer. The two forms differ only at the boundary. The body that
//! constructs a register-returned result is the destination-form body,
//! emitted under an internal symbol ([`FunctionAbi::body`]). The
//! definition's public symbol is a small entry that gives that body a frame
//! slot, calls it, and returns the loaded value. The host optimizer
//! simplifies the body on its own before it inlines the body into the
//! entry, so the body's loops and exits keep the shape the destination form
//! gives them. If the returns instead joined one block, or each loaded and
//! returned the slot, the host would fold their exit tests into selects
//! before that point. The caller stores the returned value into the storage
//! its plan selected, and SROA removes that copy. A bound in bytes would not
//! give the register guarantee. On x86-64 LLVM silently passes a hidden
//! result pointer for four 32-bit fields and returns the third of three
//! 32-bit floats through the x87 stack, while it returns three 64-bit words
//! in registers.
//!
//! Representation is not the whole signature. A parameter's *source mode*
//! does select the aliasing facts its emitted signature carries
//! (compiler/backend-facts): a reference parameter's pointer, and a range
//! reference's element pointer, is `noalias`, `nonnull` and non-capturing,
//! and a reference's is also `dereferenceable`, because [REF-1] through
//! [REF-4] and [EFF-5] already proved each of those, and `swap` [OP-11] is
//! the one row whose two arguments may name the same place. The emitter reads
//! the source signature, not this representation table, to decide that.

use crate::{IrFunction, IrNominalKind, IrProgram, IrType};

use super::{BackendFailure, storage::is_stored_aggregate};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ParameterAbi {
    Value(IrType),
    ContentPointer(IrType),
}

impl ParameterAbi {
    pub(crate) const fn ty(self) -> IrType {
        match self {
            Self::Value(ty) | Self::ContentPointer(ty) => ty,
        }
    }

    pub(crate) const fn is_indirect(self) -> bool {
        matches!(self, Self::ContentPointer(_))
    }

    /// Whether this parameter crosses the call boundary as a range
    /// reference's element pointer and count rather than as one value.
    pub(crate) const fn is_range(self) -> bool {
        matches!(self, Self::Value(IrType::Range { .. }))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ResultAbi {
    /// A scalar or descriptor, returned as its own SSA value.
    Value(IrType),
    /// A stored aggregate small enough for the return registers. The
    /// definition's body still constructs it through a destination pointer,
    /// under an internal symbol, and the public entry returns the value
    /// ([`FunctionAbi::body`]). A caller stores the returned value into the
    /// storage its plan selected.
    StoredValue(IrType),
    /// A larger stored aggregate. The callee constructs it through the
    /// caller's destination pointer, `ptr %wf.result`, and returns `void`.
    Destination(IrType),
}

impl ResultAbi {
    pub(crate) const fn ty(self) -> IrType {
        match self {
            Self::Value(ty) | Self::StoredValue(ty) | Self::Destination(ty) => ty,
        }
    }

    pub(crate) const fn uses_destination(self) -> bool {
        matches!(self, Self::Destination(_))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FunctionAbi {
    parameters: Vec<ParameterAbi>,
    result: ResultAbi,
}

impl FunctionAbi {
    pub(crate) fn build(
        program: &IrProgram,
        function: &IrFunction,
    ) -> Result<Self, BackendFailure> {
        let parameters = function
            .parameters()
            .iter()
            .map(|(_, ty)| {
                Ok(if is_stored_aggregate(program, *ty)? {
                    ParameterAbi::ContentPointer(*ty)
                } else {
                    ParameterAbi::Value(*ty)
                })
            })
            .collect::<Result<Vec<_>, BackendFailure>>()?;
        let ty = function.result();
        let result = if !is_stored_aggregate(program, ty)? {
            ResultAbi::Value(ty)
        } else if fits_return_registers(program, ty)? {
            ResultAbi::StoredValue(ty)
        } else {
            ResultAbi::Destination(ty)
        };
        Ok(Self { parameters, result })
    }

    pub(crate) fn parameters(&self) -> &[ParameterAbi] {
        &self.parameters
    }

    pub(crate) const fn result(&self) -> ResultAbi {
        self.result
    }

    /// The ABI a definition's body is emitted under. A register-returned
    /// result is constructed through a destination pointer inside the body,
    /// as a larger result is, and only the definition's public entry
    /// returns it as a value. Every other result keeps its ABI.
    pub(crate) fn body(&self) -> Self {
        let result = match self.result {
            ResultAbi::StoredValue(ty) => ResultAbi::Destination(ty),
            result => result,
        };
        Self {
            parameters: self.parameters.clone(),
            result,
        }
    }
}

/// The integer-class words one returned first-class value can occupy on
/// every admitted target: LLVM's x86-64 return convention assigns RAX, RDX
/// and RCX, one per scalar leaf, and AArch64 assigns X0 to X7.
const RETURN_INTEGER_WORDS: u64 = 3;

/// The floating leaves one returned value can occupy on every admitted
/// target: XMM0 and XMM1 on x86-64, and D0 to D7 on AArch64. A third
/// floating leaf on x86-64 returns in the x87 register ST0 through a stack
/// store and `fld`, which quiets a signaling NaN, so this bound also keeps
/// returned floats bit-exact.
const RETURN_FLOATING_LEAVES: u64 = 2;

/// Whether every scalar leaf of `ty`'s LLVM representation gets its own
/// return register on every admitted target.
///
/// LLVM returns a first-class aggregate by giving each scalar leaf its own
/// return register, without packing small leaves together. A value with more
/// integer leaves than the target has registers is silently returned through
/// a hidden pointer, which is the destination ABI with an extra copy. A third
/// floating leaf on x86-64 goes through the x87 stack instead (see
/// [`RETURN_FLOATING_LEAVES`]). The count
/// therefore follows the leaves of [`super::emitter::llvm_type`], not bytes:
/// `Result<u32, Overflow>`, `{ i32, i32, i1 }`, uses three registers, and
/// the 32-byte opaque representation `{ i128, i128 }` needs four words and
/// keeps its destination. The admitted targets share the smaller x86-64
/// budget, so the ABI and the linked definitions are the same on every
/// target.
pub(crate) fn fits_return_registers(
    program: &IrProgram,
    ty: IrType,
) -> Result<bool, BackendFailure> {
    let mut leaves = ReturnLeaves::default();
    leaves.add(program, ty, 1)?;
    Ok(leaves.integer_words <= RETURN_INTEGER_WORDS && leaves.floating <= RETURN_FLOATING_LEAVES)
}

/// The scalar leaves of one LLVM representation, counted in the return
/// registers they occupy. Counts saturate, so a long array only ever
/// exceeds the budget.
#[derive(Default)]
struct ReturnLeaves {
    integer_words: u64,
    floating: u64,
}

impl ReturnLeaves {
    /// Adds `copies` repetitions of `ty`'s leaves, mirroring the
    /// representation `llvm_type` emits for it.
    fn add(&mut self, program: &IrProgram, ty: IrType, copies: u64) -> Result<(), BackendFailure> {
        match ty {
            // `i8`, `i1`, the integer widths, a pointer, and a Box owner's
            // pointer each occupy one integer-class register.
            IrType::Unit
            | IrType::Bool
            | IrType::Integer {
                width: 8 | 16 | 32 | 64,
                ..
            }
            | IrType::Address(_)
            | IrType::RuntimeBoxPayload { .. } => self.integer(copies, 1),
            IrType::Integer { .. } => return Err(BackendFailure::InvalidIr),
            IrType::Float { width: 32 | 64 } => {
                self.floating = self.floating.saturating_add(copies);
            }
            IrType::Float { .. } => return Err(BackendFailure::InvalidIr),
            // `[0 x i8]` has no leaf; a longer array repeats its element.
            IrType::Array { length: 0, .. } => {}
            IrType::Array { element, length } => self.add(
                program,
                program.element(element).ok_or(BackendFailure::InvalidIr)?,
                copies.saturating_mul(length),
            )?,
            // `{ ptr, i64 }`, and the runtime-capacity blocks' headers, whose
            // zero-length element tails have no leaf.
            IrType::Range { .. } => self.integer(copies, 2),
            IrType::Buffer { .. } => self.integer(copies, 1),
            IrType::Window {
                shape,
                element,
                capacity,
            } => {
                let header = match (shape, capacity) {
                    (crate::IrWindowShape::Slots, Some(_)) => 1,
                    (crate::IrWindowShape::Slots, None) | (crate::IrWindowShape::Ring, Some(_)) => {
                        2
                    }
                    (crate::IrWindowShape::Ring, None) => 3,
                };
                self.integer(copies, header);
                if let Some(length @ 1..) = capacity {
                    self.add(
                        program,
                        program.element(element).ok_or(BackendFailure::InvalidIr)?,
                        copies.saturating_mul(length),
                    )?;
                }
            }
            IrType::Nominal(id) => {
                let nominal = program.nominal(id).ok_or(BackendFailure::InvalidIr)?;
                match nominal.kind() {
                    IrNominalKind::Box { .. } => self.integer(copies, 1),
                    // `{ i128, i128 }`: each `i128` takes two words.
                    IrNominalKind::Opaque => self.integer(copies, 4),
                    // `i1` or `i32`.
                    IrNominalKind::Enum { .. } if nominal.is_tag_only_enum() => {
                        self.integer(copies, 1);
                    }
                    // The `i32` tag, then every variant's fields in order.
                    IrNominalKind::Enum { variants } => {
                        self.integer(copies, 1);
                        for field in variants.iter().flat_map(|variant| variant.fields()) {
                            self.add(program, field.ty(), copies)?;
                        }
                    }
                    IrNominalKind::Struct { fields } => {
                        for field in fields {
                            self.add(program, field.ty(), copies)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn integer(&mut self, copies: u64, words: u64) {
        self.integer_words = self
            .integer_words
            .saturating_add(copies.saturating_mul(words));
    }
}
