//! The typed control-flow IR that lowering builds from the checked program
//! and the target layout and backend read.
//!
//! It records exact value types, nominal construction/projection, direct calls,
//! erased source proofs, and explicit control-flow edges. The value and
//! control-flow representation is target-neutral.
//!
//! Lowering, in its own module, builds and rewrites these records, so the
//! fields it writes are crate-visible; readers use the accessors.

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct IrValueId(pub(crate) u32);

impl IrValueId {
    #[must_use]
    pub const fn ordinal(self) -> u32 {
        self.0
    }

    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct IrBlockId(pub(crate) u32);

impl IrBlockId {
    pub(crate) fn from_index(index: usize) -> Option<Self> {
        u32::try_from(index).ok().map(Self)
    }

    #[must_use]
    pub const fn ordinal(self) -> u32 {
        self.0
    }

    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct IrNominalId(pub(crate) u32);

impl IrNominalId {
    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct IrConstantId(pub(crate) u32);

impl IrConstantId {
    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

/// The complete type of an array or run element, interned in its program's type table.
/// Structural nesting uses handles; only nominal edges can close a type graph.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct IrElement(pub(crate) u32);

impl IrElement {
    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

/// The content of an [`IrType::Address`]. A typed place may hold inline
/// content, a descriptor, or a handle. Source borrows of descriptors and
/// handles still use their value ABI; a place containing one is distinct
/// from the storage or resource that descriptor or handle denotes.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum IrAddressed {
    Unit,
    Bool,
    Integer {
        width: u8,
        signed: bool,
    },
    Float {
        width: u8,
    },
    Nominal(IrNominalId),
    Buffer {
        element: IrElement,
    },
    /// Dense inline array storage reached through a checked borrow or target.
    Array {
        element: IrElement,
        length: u64,
    },
    /// One inline window [TYPE-9]. A constant-capacity `Slots` or `Ring` is
    /// inline storage in its owner exactly as a struct is, so a reference to
    /// one is the address of that storage rather than a copy of the window.
    Window {
        shape: IrWindowShape,
        element: IrElement,
        capacity: Option<u64>,
    },
}

impl IrAddressed {
    pub const fn ty(self) -> IrType {
        match self {
            Self::Unit => IrType::Unit,
            Self::Bool => IrType::Bool,
            Self::Integer { width, signed } => IrType::Integer { width, signed },
            Self::Float { width } => IrType::Float { width },
            Self::Nominal(id) => IrType::Nominal(id),
            Self::Buffer { element } => IrType::Buffer { element },
            Self::Array { element, length } => IrType::Array { element, length },
            Self::Window {
                shape,
                element,
                capacity,
            } => IrType::Window {
                shape,
                element,
                capacity,
            },
        }
    }

    pub(crate) const fn of(ty: IrType) -> Option<Self> {
        Some(match ty {
            IrType::Unit => Self::Unit,
            IrType::Bool => Self::Bool,
            IrType::Integer { width, signed } => Self::Integer { width, signed },
            IrType::Float { width } => Self::Float { width },
            IrType::Nominal(id) => Self::Nominal(id),
            IrType::Buffer { element } => Self::Buffer { element },
            IrType::Range { .. } | IrType::RuntimeBoxPayload { .. } => return None,
            IrType::Array { element, length } => Self::Array { element, length },
            IrType::Window {
                shape,
                element,
                capacity,
            } => Self::Window {
                shape,
                element,
                capacity,
            },
            IrType::Address(_) => return None,
        })
    }
}

/// [PROV-6, STOR-3] which release action a store-backed run's own reclamation
/// is, carried into the IR because the region that decided it is erased there.
///
/// The checker fixes this from the store region's declaration alone
/// [`crate::semantic::CheckedReleaseClass`]; nothing after that point
/// rediscovers it, and no lowering may infer one action from a type shape.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum IrReleaseClass {
    /// A free to the general store the run was taken from.
    General,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum IrType {
    Unit,
    Bool,
    Integer {
        width: u8,
        signed: bool,
    },
    Float {
        width: u8,
    },
    Nominal(IrNominalId),
    Address(IrAddressed),
    Array {
        element: IrElement,
        length: u64,
    },
    /// One runtime-capacity `Array<T>` block [TYPE-9].
    ///
    /// The block is `[len | elements]` in one allocation, exactly as a
    /// runtime-capacity `Slots` block is `[len | cap | slots]`
    /// (compiler/storage-representation): an `Array`'s `len` equals its `cap`
    /// [WIN-1], so the one runtime number is stored once. [TYPE-9] admits the
    /// shape only as `Box` content, so the block is reached only by pointer —
    /// the `Box` value is that pointer — and no value of this type is ever
    /// copied, passed, or stored.
    Buffer {
        element: IrElement,
    },
    /// One `&[T]` range reference [REF-4]: a pointer to the first element of
    /// the range and the element count, which is its one measure [MSR-1].
    /// It is a reference kind and not a type [TYPE-8], so no storage ever
    /// holds one and nothing is ever released through one.
    Range {
        element: IrElement,
    },
    /// One compiler-synthesized task capture of the first element address of
    /// a runtime-capacity `Array` block. The pointer may be one-past for an
    /// empty array, so it is not an [`IrType::Address`] and cannot be loaded
    /// or stored through directly. Its nominal identifies the ordinary Box
    /// owner that [`IrOperation::RuntimeBoxOwner`] may reconstruct.
    RuntimeBoxPayload {
        nominal: IrNominalId,
    },
    /// One `Slots<T, N>`, `Slots<T>`, `Ring<T, N>` or `Ring<T>` [TYPE-9].
    ///
    /// The layout is header-first so that the inline and the boxed placement
    /// of one shape share one address computation
    /// (compiler/storage-representation): a `Slots` is `{ i64 len, slots }`
    /// and a `Ring` is `{ i64 len, i64 head, slots }`, with a
    /// runtime-capacity block carrying `cap` after `len` and a
    /// constant-capacity one storing no capacity at all. The two shapes are
    /// two emitted types so that neither carries a word no measure of it
    /// needs.
    Window {
        shape: IrWindowShape,
        element: IrElement,
        /// `Some` is the constant-capacity inline placement; `None` is the
        /// runtime-capacity block, which [TYPE-9] admits only as `Box`
        /// content and which is therefore only ever reached by pointer.
        capacity: Option<u64>,
    },
}

/// Which of [TYPE-9]'s two window shapes an [`IrType::Window`] is.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum IrWindowShape {
    /// The window begins at slot zero and the block stores no `head`.
    Slots,
    /// The window begins at `head` and wraps modulo `cap` [WIN-1].
    Ring,
}

/// Whether a value of this type derives any release work at all [STOR-3].
///
/// This is the single reading of "does dropping this value do something": the
/// target stage asks it to decide whether a drop emits a cleanup. A `None`
/// answer is a malformed nominal
/// reference, which each caller reports in its own vocabulary; no caller may
/// read it as "no release", because unknown must never be silently inert.
pub(crate) fn type_derives_release(
    nominals: &[IrNominal],
    elements: &[IrType],
    ty: IrType,
) -> Option<bool> {
    let nominal_kind = |id: IrNominalId| nominals.get(id.index()).map(IrNominal::kind);
    let mut pending = vec![ty];
    let mut visited = std::collections::HashSet::new();
    while let Some(current) = pending.pop() {
        if !visited.insert(current) {
            continue;
        }
        match current {
            // A runtime-capacity block is one heap object the owner frees
            // [TYPE-9, STOR-3], so it always derives a release.
            IrType::Buffer { .. }
            | IrType::Window {
                capacity: None, ..
            } => return Some(true),
            // A constant-capacity window reclaims nothing of its own; it
            // still needs a walk when its slots hold values that derive one,
            // and [PROV-6] visits those before the block is released.
            IrType::Array { element, .. }
            | IrType::Window {
                element,
                capacity: Some(_),
                ..
            } => {
                pending.push(*elements.get(element.index())?);
            }
            // S39 a cell needs a release exactly when its own storage or
            // its referent does: a bump extent's cell whose referent derives
            // nothing needs no walk at all.
            IrType::Nominal(id)
                if matches!(
                    nominal_kind(id),
                    Some(IrNominalKind::Box {
                        release: IrReleaseClass::General,
                        ..
                    })
                ) =>
            {
                return Some(true);
            }
            IrType::Nominal(id)
                if matches!(nominal_kind(id), Some(IrNominalKind::Box { .. })) =>
            {
                let Some(IrNominalKind::Box { referent, .. }) = nominal_kind(id) else {
                    return None;
                };
                pending.push(*referent);
            }
            IrType::Nominal(id) => match nominal_kind(id)? {
                IrNominalKind::Struct { fields } => {
                    pending.extend(fields.iter().map(IrField::ty));
                }
                IrNominalKind::Enum { variants } => {
                    pending.extend(
                        variants
                            .iter()
                            .flat_map(IrVariant::fields)
                            .map(IrField::ty),
                    );
                }
                IrNominalKind::Box { .. } => {
                    return Some(true);
                }
                // Ordinary opaque values have empty release [PRE-1].
                IrNominalKind::Opaque => {}
            },
            IrType::Unit
            | IrType::Bool
            | IrType::Integer { .. }
            | IrType::Float { .. }
            // [REF-4, TYPE-8] a range reference is a name for elements it
            // does not own, so nothing of it is ever released.
            | IrType::Range { .. }
            // A synthesized payload capture borrows the source Box allocation
            // and never carries ownership or cleanup authority.
            | IrType::RuntimeBoxPayload { .. }
            | IrType::Address(_) => {}
        }
    }
    Some(false)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IrField {
    pub(crate) ty: IrType,
}

impl IrField {
    pub const fn ty(&self) -> IrType {
        self.ty
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IrVariant {
    pub(crate) tag: u32,
    pub(crate) fields: Vec<IrField>,
}

impl IrVariant {
    pub const fn tag(&self) -> u32 {
        self.tag
    }

    pub fn fields(&self) -> &[IrField] {
        &self.fields
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IrNominalKind {
    Struct {
        fields: Vec<IrField>,
    },
    Enum {
        variants: Vec<IrVariant>,
    },
    Box {
        referent: IrType,
        /// [PROV-6, S39] which release action this cell's own reclamation is.
        /// The ambient-heap `box<T>` [STOR-1] and a `Box<'s, T>` at a general
        /// store both free their cell; a `Box<'s, T>` at a bump extent is
        /// reclaimed by its region's own reset and has no action of its own.
        release: IrReleaseClass,
    },
    /// An ordinary opaque nominal supplied by PRE-1.
    Opaque,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IrNominal {
    pub(crate) name: String,
    /// The stable part of the type's link-visible name [MOD-8].
    pub(crate) link_name: String,
    /// The type's module-qualified spelling, when it has one.
    pub(crate) stable: Option<String>,
    pub(crate) id: IrNominalId,
    pub(crate) kind: IrNominalKind,
}

impl IrNominal {
    /// The ordinary declaration name, which tests read.
    #[cfg(test)]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The stable part of this type's link-visible name: the digest of its
    /// module-qualified spelling, so an unchanged fragment keeps its text
    /// when another type is added or removed [MOD-8].
    pub fn link_name(&self) -> &str {
        &self.link_name
    }

    /// The type's module-qualified spelling, `std.process.Inputs` for a
    /// standard library type [MOD-10], which names it apart from any
    /// program type of the same name.
    pub fn stable_spelling(&self) -> Option<&str> {
        self.stable.as_deref()
    }

    pub const fn id(&self) -> IrNominalId {
        self.id
    }

    pub const fn kind(&self) -> &IrNominalKind {
        &self.kind
    }

    pub fn is_tag_only_enum(&self) -> bool {
        matches!(
            &self.kind,
            IrNominalKind::Enum { variants }
                if variants.iter().all(|variant| variant.fields.is_empty())
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IrEnumType {
    Bool,
    Nominal(IrNominalId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IrIntegerOperation {
    AddWrap,
    SubtractWrap,
    MultiplyWrap,
    AddExact,
    SubtractExact,
    MultiplyExact,
    AddDefined,
    SubtractDefined,
    MultiplyDefined,
    AddChecked,
    SubtractChecked,
    MultiplyChecked,
    DivideChecked,
    RemainderChecked,
    DivideExact,
    RemainderExact,
    DivideDefined,
    RemainderDefined,
    AbsoluteWrap,
    AbsoluteExact,
    AbsoluteDefined,
    AbsoluteChecked,
    NegateWrap,
    NegateExact,
    NegateDefined,
    NegateChecked,
    BitAnd,
    BitOr,
    BitXor,
    BitNot,
    ShiftLeftWrap,
    ShiftRightWrap,
    ShiftLeftExact,
    ShiftRightExact,
    ShiftLeftDefined,
    ShiftRightDefined,
    RotateLeft,
    RotateRight,
    PopulationCount,
    LeadingZeros,
    TrailingZeros,
    ByteSwap,
    MultiplyHigh,
    AddSaturating,
    SubtractSaturating,
    MultiplySaturating,
    Minimum,
    Maximum,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IrBooleanOperation {
    And,
    Or,
    ExclusiveOr,
    Not,
}

/// The source-selected conversion contract, retained independently of its
/// concrete endpoints and result representation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IrConversionMode {
    Exact,
    Checked,
    Defined,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IrFloatOperation {
    AddStrict,
    SubtractStrict,
    MultiplyStrict,
    DivideStrict,
    Equal,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    NotEqual,
    Negate,
    Absolute,
    CopySign,
    Minimum,
    Maximum,
    Floor,
    Ceil,
    Truncate,
    RoundEven,
    Remainder,
    SquareRootStrict,
    FusedMultiplyAddStrict,
    Infinity,
    Nan,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IrConstant {
    Unit,
    Bool(bool),
    Integer { ty: IrType, bits: u64 },
    Float { ty: IrType, bits: u64 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IrGlobalValue {
    Scalar(IrConstant),
    Array(Vec<IrGlobalValue>),
    /// One struct-typed rodata constant [CONST-2 candidate]: complete field
    /// values in declared order.
    Struct(Vec<IrGlobalValue>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IrGlobalConstant {
    pub(crate) id: IrConstantId,
    pub(crate) name: String,
    /// The stable part of the constant's link-visible name [MOD-8].
    pub(crate) link_name: String,
    pub(crate) ty: IrType,
    pub(crate) value: IrGlobalValue,
}

impl IrGlobalConstant {
    pub const fn id(&self) -> IrConstantId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// The stable part of this constant's link-visible name [MOD-8].
    pub fn link_name(&self) -> &str {
        &self.link_name
    }

    pub const fn ty(&self) -> IrType {
        self.ty
    }

    pub const fn value(&self) -> &IrGlobalValue {
        &self.value
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IrArrayRoot {
    Value(IrValueId),
    Constant(IrConstantId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IrTargetDomainObligation {
    RuntimeSizedAllocation,
    ElementAddress,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IrRuntimeTargetObligations {
    allocation: IrTargetDomainObligation,
    element_address: IrTargetDomainObligation,
    source_length_upper_bound: u64,
    /// Whether `source_length_upper_bound` is the numeric bound the
    /// allocation site's own [OP-9] discharge established, or [OP-9]'s
    /// ceiling standing in for a bound the checked program does not retain.
    ///
    /// A compiler-owned [PRE-1] construction row is one body per instance,
    /// reached from every call of that row, so no single call's proved bound
    /// belongs to it (compiler/prelude-records). Each caller carries and is
    /// qualified with its own bound in [`IrSourceCall`]; this flag keeps the
    /// shared body's representation record from reusing the language maximum
    /// as though it were one caller's target-domain bound.
    call_site_bound: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IrLayoutMagnitude {
    Finite(u64),
    AboveU64,
}

impl IrLayoutMagnitude {
    pub(crate) const fn permits(self, actual: u64) -> bool {
        match self {
            Self::Finite(ceiling) => actual <= ceiling,
            Self::AboveU64 => true,
        }
    }
}

/// The complete [OP-9] and [STOR-6] record one runtime-capacity allocation
/// carries: the language layout ceiling its stored type must stay under, and
/// the target-domain obligations with the retained source bound.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IrAllocationObligations {
    pub layout_ceiling: IrLayoutCeiling,
    pub target_domains: IrRuntimeTargetObligations,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IrLayoutCeiling {
    pub size: IrLayoutMagnitude,
    pub align: u64,
    pub stride: IrLayoutMagnitude,
}

impl IrRuntimeTargetObligations {
    pub(crate) const fn is_complete(self) -> bool {
        matches!(
            (self.allocation, self.element_address),
            (
                IrTargetDomainObligation::RuntimeSizedAllocation,
                IrTargetDomainObligation::ElementAddress
            )
        )
    }

    pub(crate) const fn source_length_upper_bound(self) -> u64 {
        self.source_length_upper_bound
    }

    /// Whether the retained bound came from the allocation site's own
    /// [OP-9] discharge.
    pub(crate) const fn has_call_site_bound(self) -> bool {
        self.call_site_bound
    }

    /// The record one compiler-owned [PRE-1] allocation carries [OP-9].
    ///
    /// A construction row's body is built once per monomorphized instance.
    /// This record retains [OP-9]'s language maximum for the body-level
    /// representation check; each accepted caller separately retains its
    /// tighter proved bound in [`IrSourceAllocation`] for target byte-domain
    /// qualification. The language maximum is deliberately not used as a
    /// selected-target allocation bound here.
    pub(crate) const fn from_language_ceiling(source_length_upper_bound: u64) -> Self {
        Self {
            allocation: IrTargetDomainObligation::RuntimeSizedAllocation,
            element_address: IrTargetDomainObligation::ElementAddress,
            source_length_upper_bound,
            call_site_bound: false,
        }
    }
}

/// The [MSR-1] measure one reader row loads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IrMeasure {
    Length,
    Capacity,
    Head,
}

/// Which of [BLK-3]'s four boundary operations one run operation is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IrBoundary {
    PlaceBack,
    PlaceFront,
    TakeBack,
    TakeFront,
}

impl IrBoundary {
    /// Whether this row moves the front boundary, which is the one that can
    /// leave `head` nonzero [MSR-1].
    #[must_use]
    pub const fn front(self) -> bool {
        matches!(self, Self::PlaceFront | Self::TakeFront)
    }

    /// Whether this row places a value rather than removing one.
    #[must_use]
    pub const fn places(self) -> bool {
        matches!(self, Self::PlaceBack | Self::PlaceFront)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IrPlaceStep {
    /// A field of directly stored nominal content.
    Field { nominal: IrNominalId, field: u32 },
    /// The allocation payload reached through a stored Box owner slot.
    BoxReferent { nominal: IrNominalId },
    /// One payload field of an enum in directly addressed storage.
    EnumVariant {
        nominal: IrNominalId,
        variant: u32,
        field: u32,
    },
    /// An initialized run element selected by its checked logical offset.
    RunElement {
        offset: IrValueId,
        target_domain: IrTargetDomainObligation,
    },
    /// An initialized legacy array element, without run descriptor words.
    ArrayElement {
        offset: IrValueId,
        target_domain: IrTargetDomainObligation,
    },
    /// One runtime-capacity Array element after its descriptor word.
    BufferElement {
        offset: IrValueId,
        target_domain: IrTargetDomainObligation,
    },
}

/// A total, nonnegative estimate evaluated only for a parallel split budget.
/// Its leaves are existing scalar captures or descriptor lengths, never
/// element reads or user calls. Arithmetic saturates rather than overflowing.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum IrWorkEstimate {
    Constant(u64),
    Value(IrValueId),
    Length(IrValueId),
    /// The runtime Array length behind an original, read-only Box reference
    /// formal, or its exact capture in a synthesized chunk. The marker on
    /// `IrFunction` supplies the no-write fact; the observation must also
    /// originate in an exhibited typed Array-length read. An address or a
    /// captured Box owner alone does not authorize the observation.
    BoxArrayLength(IrValueId),
    Sum(Vec<Self>),
    Product(Box<Self>, Box<Self>),
    Difference(Box<Self>, Box<Self>),
    /// The divisor is a positive compiler constant.
    Quotient(Box<Self>, u64),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IrOperation {
    Constant(IrConstant),
    Call {
        function: u32,
        arguments: Vec<IrValueId>,
    },
    Integer {
        operation: IrIntegerOperation,
        operand_type: IrType,
        arguments: Vec<IrValueId>,
    },
    Float {
        operation: IrFloatOperation,
        operand_type: IrType,
        arguments: Vec<IrValueId>,
    },
    NumericConversion {
        mode: IrConversionMode,
        source_type: IrType,
        destination_type: IrType,
        value: IrValueId,
    },
    Reinterpret {
        source_type: IrType,
        destination_type: IrType,
        value: IrValueId,
    },
    Boolean {
        operation: IrBooleanOperation,
        arguments: Vec<IrValueId>,
    },
    EnumEquality {
        equal: bool,
        operand_type: IrType,
        arguments: [IrValueId; 2],
    },
    ArrayFill {
        value: IrValueId,
        target_domain: IrTargetDomainObligation,
    },
    /// BLK-3's consuming conversion between a full fixed run and its dense
    /// array. Source and result have identical element type and extent.
    FullArrayConversion {
        value: IrValueId,
    },
    /// One discharged source subscript read [OP-4]: the checker has already
    /// derived the bounds obligation, so no runtime branch is emitted in any
    /// build mode; the offset is consumed directly.
    ArrayIndex {
        root: IrArrayRoot,
        offset: IrValueId,
        target_domain: IrTargetDomainObligation,
    },
    /// [OP-13] `box_array_filled<T>(count, value)`: one runtime-capacity
    /// `Array<T>` block and the cell that owns it.
    ///
    /// The block is `[len | elements]` in one allocation, so the defined
    /// value is the cell's own `Box` pointer and the block pointer at once
    /// (compiler/storage-representation). One `malloc` builds it, one `free`
    /// reclaims it, and every element address is one `inbounds`
    /// `getelementptr` into it.
    BufferFill {
        nominal: IrNominalId,
        length: IrValueId,
        value: IrValueId,
        layout_ceiling: IrLayoutCeiling,
        target_domains: IrRuntimeTargetObligations,
    },
    /// [MSR-1] the one measure a runtime-capacity `Array<T>` has, read from
    /// the `len` word at the head of its block. `buffer` is the block's
    /// address.
    BufferMeasure {
        buffer: IrValueId,
    },
    /// [BLK-2] `fixed_vector`: one frame-resident run of the defined type's own
    /// capacity, whose window is empty. Every slot is raw and the two
    /// descriptor words are zero.
    /// [OP-13] `slots_new` and `ring_new`: the empty window over `N` raw
    /// slots. The value is the zero aggregate, so `len` and, on a `Ring`,
    /// `head` both start at zero, which is exactly what the two records'
    /// `ensures` publish.
    Window,
    /// [MSR-1] one measure of a window, read as its [OP-15] member reader
    /// loads it. A cell the measure table fixes as a constant never reaches
    /// here.
    ContainerMeasure {
        measure: IrMeasure,
        container: IrValueId,
    },
    /// One discharged source subscript read of a run [OP-4, WIN-1]: the
    /// offset is a logical one and the storage read is slot
    /// `(head + i) mod cap`. See [`Self::ArrayIndex`] for the discharge.
    RunIndex {
        run: IrValueId,
        offset: IrValueId,
        target_domain: IrTargetDomainObligation,
    },
    /// [OP-10] place one element at a window boundary and move that boundary.
    RunBoundary {
        row: IrBoundary,
        run: IrValueId,
        /// The placed element. Takes use the complete `RunTaken` operation.
        value: Option<IrValueId>,
    },
    /// [OP-10] take one element and move the window boundary. The physical
    /// element address is captured before changing the descriptor; no source
    /// observation occurs between that change and reading the captured slot.
    RunTaken {
        row: IrBoundary,
        run: IrValueId,
    },
    /// [OP-10] the one shift `insert_at` and `remove_at` each perform over
    /// `window.filled`, followed by the boundary move that shift makes room
    /// for or closes.
    ///
    /// `open` shifts every element at `index ..` up by one slot and raises
    /// `len`; its complement shifts every element above `index` down by one
    /// and lowers `len`. On a `Slots` the window is contiguous and the shift
    /// is one memmove; on a `Ring` the window may wrap and the shift walks
    /// the logical indices in the order that never overwrites an unread
    /// slot [WIN-1].
    RunShift {
        run: IrValueId,
        index: IrValueId,
        open: bool,
    },
    /// [OP-10] `insert_at`'s placement into the slot [`Self::RunShift`] just
    /// opened. The boundary has already moved, so the slot is inside the
    /// window and the store is an ordinary element write.
    RunInsert {
        run: IrValueId,
        index: IrValueId,
        value: IrValueId,
    },
    /// [OP-10] the run of elements `append` and `split_off` each move between
    /// two windows: `source[index ..]` moves to `destination.free`,
    /// `source.len` becomes `index`, and `destination.len` grows by the
    /// moved count. `append` is this row at index zero.
    RunTransfer {
        destination: IrValueId,
        source: IrValueId,
        index: IrValueId,
    },
    /// [OP-13] one runtime-capacity window block and the cell that owns it:
    /// `[len | cap | head? | slots]` with an empty window
    /// (compiler/storage-representation). The value is the cell pointer.
    WindowBlockNew {
        nominal: IrNominalId,
        capacity: IrValueId,
        obligations: IrAllocationObligations,
    },
    /// [OP-10] `grow`: the cell's content is remade whole at the new
    /// capacity. One allocation, one copy of the header and the filled
    /// slots, one free, and the cell's pointer slot takes the new block
    /// (compiler/storage-representation).
    WindowGrow {
        nominal: IrNominalId,
        cell: IrValueId,
        capacity: IrValueId,
        obligations: IrAllocationObligations,
    },
    /// Release only one cell's own storage. [OP-14] uses this after proving a
    /// boxed window empty; an owned-path take uses it after the checked cleanup
    /// plan has accounted for the cell's split content [WIN-3, STOR-3].
    CellFree {
        nominal: IrNominalId,
        value: IrValueId,
    },
    /// One discharged source subscript read [OP-4]; see [`Self::ArrayIndex`].
    /// `buffer` is the block's address, and the element address is one
    /// `inbounds` step into it.
    BufferIndex {
        buffer: IrValueId,
        offset: IrValueId,
        target_domain: IrTargetDomainObligation,
    },
    /// One semantics-preserving wide-probe step over a `u8` buffer.
    ///
    /// Computes how many upcoming iterations of a recognized byte-walk loop
    /// are provably no-ops: the count of leading bytes at `index ..` that
    /// match no needle, but only when `index + 16 <= min(limit, length)`
    /// bounds both the walk's exit guard and every skipped read; otherwise 0.
    /// Every byte at which anything observable can happen — a needle hit or
    /// the exit bound — therefore reaches the unchanged scalar body. The probe
    /// itself reads only bytes its internal guard proves in bounds.
    BufferProbeSkip {
        buffer: IrValueId,
        index: IrValueId,
        limit: IrValueId,
        needles: Vec<IrValueId>,
    },
    SliceFromBuffer {
        buffer: IrValueId,
    },
    /// [REF-4] one range reference over typed owner storage: a run's
    /// initialized window [WIN-1] or a complete array with its type's length
    /// and zero head.
    ///
    /// The window is `len` slots beginning at `head`, and the row's own
    /// requirement `vector.head <= vector.cap` was discharged before
    /// this operation exists, so the window is one contiguous range and the
    /// descriptor is the slot at `head` together with `len`.
    SliceFromRun {
        run: IrValueId,
    },
    /// A statically proved relative subrange of an existing descriptor.
    SliceRange {
        slice: IrValueId,
        start: IrValueId,
        end: IrValueId,
    },
    SliceMeasure {
        slice: IrValueId,
    },
    /// One discharged source subscript read [OP-4]; see [`Self::ArrayIndex`].
    SliceIndex {
        slice: IrValueId,
        offset: IrValueId,
        target_domain: IrTargetDomainObligation,
    },
    /// The address of one discharged range element, retained for a source
    /// reference instead of loaded as an owned value.
    SliceAddress {
        slice: IrValueId,
        offset: IrValueId,
        target_domain: IrTargetDomainObligation,
    },
    BoxNew {
        nominal: IrNominalId,
        value: IrValueId,
    },
    /// S39 the destructuring consume of a cell: its referent is loaded out
    /// and its own storage is released, which is a free on a general store
    /// and nothing on a bump extent.
    BoxTake {
        nominal: IrNominalId,
        value: IrValueId,
    },
    BoxDeref {
        nominal: IrNominalId,
        value: IrValueId,
    },
    /// The first-element pointer used only by a synthesized split capture of
    /// a `Box<Array<T>>`. The source Box value remains the allocation-base
    /// pointer and retains sole cleanup authority.
    RuntimeBoxPayload {
        nominal: IrNominalId,
        owner: IrValueId,
    },
    /// Recovers the ordinary allocation-base Box pointer from a synthesized
    /// payload capture before rebuilding borrowed local owner storage in a
    /// chunk. The inverse uses the same target-layout field offset as the
    /// forward projection.
    RuntimeBoxOwner {
        nominal: IrNominalId,
        payload: IrValueId,
    },
    ConstructStruct {
        nominal: IrNominalId,
        fields: Vec<IrValueId>,
    },
    ConstructEnum {
        nominal: IrNominalId,
        variant: u32,
        fields: Vec<IrValueId>,
    },
    ProjectStruct {
        aggregate: IrValueId,
        nominal: IrNominalId,
        field: u32,
        consume_root: bool,
    },
    InsertStruct {
        aggregate: IrValueId,
        nominal: IrNominalId,
        field: u32,
        value: IrValueId,
    },
    ProjectVariant {
        aggregate: IrValueId,
        nominal: IrNominalId,
        variant: u32,
        field: u32,
    },
    AddressOf {
        value: IrValueId,
        referent: IrAddressed,
    },
    /// Address of immutable program-lifetime storage. It never owns a frame
    /// slot or participates in local destination reuse.
    ConstantAddress {
        constant: IrConstantId,
    },
    /// A typed child place of an already stable owner or borrow. This keeps
    /// the same backing and lifetime; it does not read or copy its content.
    ProjectAddress {
        address: IrValueId,
        projection: IrPlaceStep,
    },
    Load {
        address: IrValueId,
        referent: IrAddressed,
    },
    /// One permitted counted loop [PAR-2 candidate], actualized as a recursive
    /// split of its index range.
    ///
    /// The whole loop is one instruction here because it has exactly two
    /// renderings and the choice between them is the world, not the source. The
    /// overlapped world asks the runtime what a split of this span may afford
    /// and calls `splitter`; the sequential world calls `chunk`, which *is* the
    /// loop, so that world runs the code the loop always had. A reduction uses
    /// the first argument and result for its accumulator. An independent map
    /// uses `Unit` in both positions as a synchronization token; its observable
    /// result is the disjoint stores completed before the call returns. The
    /// site therefore has nothing to recombine in either form.
    ///
    /// `splitter` and `chunk` are ordinary synthesized [`IrFunction`]s: the
    /// splitter's two recursive calls are one ordinary overlap group, so the
    /// hand-out, the thunk, the deque, and the join are the ones every other
    /// permitted pair uses.
    LoopSplit {
        splitter: u32,
        chunk: u32,
        /// A reduction's accumulator on entry, or the `Unit` synchronization
        /// token of an independent map. A reduction seed folds into the
        /// leftmost chunk, which keeps its leaf order the source's own.
        seed: IrValueId,
        lower: IrValueId,
        upper: IrValueId,
        /// The values the body reads from the enclosing scope, in the order the
        /// two synthesized functions declare them.
        captures: Vec<IrValueId>,
        /// The static cost estimate of one iteration, which the runtime
        /// allowance multiplies by the span. A cost over the emitted IR, never
        /// a name, a signature, or a source shape.
        weight: u64,
        /// An available runtime extent estimate; absence keeps `weight`.
        work: Option<IrWorkEstimate>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IrInstruction {
    Define {
        result: IrValueId,
        ty: IrType,
        operation: IrOperation,
    },
    /// One element-position store through an exclusive view [SET-1,
    /// VIEW-1]. The descriptor is unchanged; the storage written is the
    /// origin's, reached through the view's own data pointer.
    StoreSlice {
        slice: IrValueId,
        index: IrValueId,
        value: IrValueId,
    },
    Store {
        address: IrValueId,
        value: IrValueId,
        referent: IrAddressed,
    },
    /// Capture every cleanup subject before running this checked release
    /// sequence. A release must not change a later subject's saved value.
    Drops(Vec<IrDrop>),
}

/// A release consumes either an already captured value or the initialized
/// content at an existing typed place. Naming a cleanup place does not read
/// its entire content into another owned aggregate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IrDropSubject {
    Value(IrValueId),
    Place(IrValueId),
}

/// One compiler-derived release, explicit on the normal control-flow edge
/// that carries it [STOR-3].
///
/// Every drop and every release is represented before lowering. The IR places
/// these records on `Jump` and `Return` terminators and as `Drops` instructions
/// in straight-line position. Their order inside one edge is the checked
/// program's reverse declaration order. Their position relative to calls
/// preserves the ordinary scope-exit release sequence [STOR-3].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IrDrop {
    pub(crate) subject: IrDropSubject,
    pub(crate) ty: IrType,
}

impl IrDrop {
    pub const fn subject(self) -> IrDropSubject {
        self.subject
    }

    pub const fn operand(self) -> IrValueId {
        match self.subject {
            IrDropSubject::Value(value) | IrDropSubject::Place(value) => value,
        }
    }

    pub const fn ty(self) -> IrType {
        self.ty
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IrMatchTarget {
    pub(crate) tag: u32,
    pub(crate) block: IrBlockId,
}

impl IrMatchTarget {
    pub const fn tag(self) -> u32 {
        self.tag
    }

    pub const fn block(self) -> IrBlockId {
        self.block
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IrTerminator {
    /// A checker-proved uninhabited function body. The function keeps its
    /// ordinary ABI but has no source-derived executable path.
    Unreachable,
    Jump {
        target: IrBlockId,
        arguments: Vec<IrValueId>,
        drops: Vec<IrDrop>,
    },
    Match {
        scrutinee: IrValueId,
        enum_type: IrEnumType,
        targets: Vec<IrMatchTarget>,
    },
    Return {
        value: IrValueId,
        drops: Vec<IrDrop>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IrBlock {
    pub(crate) parameters: Vec<(IrValueId, IrType)>,
    pub(crate) instructions: Vec<IrInstruction>,
    pub(crate) terminator: IrTerminator,
}

impl IrBlock {
    pub fn parameters(&self) -> &[(IrValueId, IrType)] {
        &self.parameters
    }

    pub fn instructions(&self) -> &[IrInstruction] {
        &self.instructions
    }

    pub const fn terminator(&self) -> &IrTerminator {
        &self.terminator
    }
}

/// Where one call into an ordinary cyclic call-graph component starts
/// counting the levels that may still hand work out.
///
/// Every member of such a component gets one synthesized variant carrying this
/// count as a hidden trailing parameter; inside the family a call spends one
/// level, and a call made with nothing left enters the component's existing
/// same-ABI sequential clone, the world with no scheduler test in it. The
/// count therefore selects how much of an admitted program is actualized in
/// parallel. It is read by the emitter and by no acceptance path: it is not a
/// timeout, a fuel bound, a proof-work budget or an early failure, it cannot
/// reject a program, and compiling the same source with any of these three
/// forms accepts exactly the same programs and computes exactly the same
/// values.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RecursionBudget {
    /// Emit no budget family: every node of every recursive component offers.
    ///
    /// The control, and what every `--par` build did before the family
    /// existed. On the compute scoreboard's quadrature kernel this is 35 to 40
    /// percent behind the default at every measured width.
    Off,
    /// Ask the runtime once, at the component's ordinary entry. The answer
    /// follows the pool width, which a compile-time constant cannot — and the
    /// measured best fixed value is not the same value at two lanes and at
    /// four, which is why the default is a query rather than a number.
    #[default]
    RuntimeDerived,
    /// Start from this compile-time value instead of asking. The control the
    /// measured depth sweep uses.
    Pinned(std::num::NonZeroU8),
}

/// One group of pure sibling calls whose evaluations may be overlapped
/// [PAR-1 candidate].
///
/// The members are the values those calls define, in source order, all in one
/// block of one function. The compute scheduler may hand out every member but
/// the last, runs that source-last member on the calling lane, and joins the
/// handed-out calls before any value use or block exit.
///
/// The group is a permission the target stage may take, never an obligation:
/// a target that hands nothing out emits exactly the sequential code, because
/// the handed-out call and the default inline fallback call the same
/// monomorphized function on the same arguments. Optional refusal/frontier
/// controls may choose a same-ABI sequential clone that declines descendant
/// offers; the original operations, arguments and join boundary remain.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IrOverlap {
    pub(crate) members: Vec<IrValueId>,
}

impl IrOverlap {
    /// The value whose definition is the group's join site: the last member,
    /// which runs on the calling thread.
    pub fn join_site(&self) -> Option<IrValueId> {
        self.members.last().copied()
    }

    /// The members that may be handed to a worker lane, in source order.
    pub fn handed_out(&self) -> &[IrValueId] {
        self.members
            .split_last()
            .map_or(&[][..], |(_, earlier)| earlier)
    }
}

/// Why a function exists, for the one consumer that has to tell the two worlds
/// apart: a source function is emitted into both, while the two halves of a
/// [`IrOperation::LoopSplit`] each belong to exactly one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IrSynthesis {
    /// The recursive range splitter. It exists only in the overlapped world —
    /// the sequential world calls the chunk directly — so cloning it would emit
    /// a second, unreachable caller of the chunk's clone and cost that clone
    /// the single-call-site inlining the sequential world depends on.
    Splitter,
    /// The loop over a subrange, seeded by its first parameter. Both worlds
    /// reach it, so it is cloned like a source function and each world's copy
    /// has exactly one caller.
    Chunk,
}

/// A checked source signature's kind [GRAM-3, REF-1, REF-4], independent of
/// its lowered value type.
///
/// A reference is a local name for a path, with no permission marker and no
/// region [REF-1], so this record names only which of the three kinds the
/// signature wrote. It carries no origin and no validity interval, and cannot
/// by itself authorize aliasing an input and a result destination.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IrSourceMode {
    /// The source signature passes an owned value.
    Own,
    /// `&T`: the signature names one path the caller already holds [REF-1].
    Reference,
    /// `&[T]`: the signature names a range of elements [REF-4].
    Range,
}

/// Checked source roles retained independently of representation and erased regions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IrSourceSignature {
    pub(crate) parameters: Vec<IrSourceMode>,
    pub(crate) result: IrSourceMode,
}

impl IrSourceSignature {
    pub(crate) fn parameters(&self) -> &[IrSourceMode] {
        &self.parameters
    }

    pub(crate) const fn result(&self) -> IrSourceMode {
        self.result
    }
}

/// One source argument's checked use, distinct from its formal passing mode.
///
/// Consuming a unique holder transfers that holder, not ownership of its
/// referent. Combine this record with the callee's source signature; neither
/// the representation type nor a consume flag alone supplies that distinction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IrSourceArgument {
    /// A direct binding occurrence; the flag is the owning checker's verdict.
    Binding {
        /// Whether this occurrence consumes the source binding.
        consume_root: bool,
    },
    /// A checked field projection, including its enclosing-root consumption.
    Projection {
        /// Whether this projection consumes its enclosing source binding.
        consume_root: bool,
    },
    /// A borrow or reborrow formed over existing storage.
    Borrow,
    /// Content selected through a place or dereference. This does not claim
    /// that the enclosing owner is consumed or that affine content is copyable.
    PlaceRead,
    /// A literal or another computed value, with no binding-transfer claim.
    Value,
}

/// The accepted source-level allocation judgment attached to one ordinary
/// call of a compiler-owned construction or growth row [OP-9, STOR-6].
///
/// The row body remains one out-of-line monomorphized function. Target
/// qualification reads this per-call record to qualify the exact proved
/// count bound against the selected target's element stride and block header.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct IrSourceAllocation {
    pub(crate) cell: IrNominalId,
    pub(crate) count_argument: usize,
    pub(crate) layout_ceiling: IrLayoutCeiling,
    pub(crate) source_length_upper_bound: u64,
}

impl IrSourceAllocation {
    pub(crate) const fn cell(self) -> IrNominalId {
        self.cell
    }

    pub(crate) const fn count_argument(self) -> usize {
        self.count_argument
    }

    pub(crate) const fn layout_ceiling(self) -> IrLayoutCeiling {
        self.layout_ceiling
    }

    pub(crate) const fn source_length_upper_bound(self) -> u64 {
        self.source_length_upper_bound
    }
}

/// Source-call use and direct borrow-result relations tied to one IR call.
///
/// The actual arguments and their typed address/projection operations remain
/// on the call. A result's origin is the complete candidate argument; it need
/// not be the exact subplace selected inside the callee. No source offset is
/// reevaluated to produce this metadata, and it adds no executable read.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IrSourceCall {
    pub(crate) result: IrValueId,
    pub(crate) arguments: Vec<IrSourceArgument>,
    /// A direct borrow result's checked candidate. Absence says nothing about
    /// loans carried inside owned view results or other aggregates.
    pub(crate) returned_borrow_argument: Option<usize>,
    /// The call's accepted allocation bound, only for the runtime-capacity
    /// construction and growth rows that carry OP-9.
    pub(crate) allocation: Option<IrSourceAllocation>,
}

impl IrSourceCall {
    pub(crate) const fn result(&self) -> IrValueId {
        self.result
    }

    pub(crate) fn arguments(&self) -> &[IrSourceArgument] {
        &self.arguments
    }

    pub(crate) const fn allocation(&self) -> Option<IrSourceAllocation> {
        self.allocation
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct IrCountedRange {
    /// Blocks built for this counted loop, including its structural exits.
    pub(crate) blocks: std::ops::Range<usize>,
    /// The continuation is allocated with the loop but executes after it.
    pub(crate) continuation: IrBlockId,
    /// The endpoint values captured before the first header.
    pub(crate) lower: IrValueId,
    pub(crate) upper: IrValueId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IrFunction {
    pub(crate) name: String,
    pub(crate) parameters: Vec<(IrValueId, IrType)>,
    /// Original reference formals whose roots have no declared write, copied
    /// from checked effects. Only their exact direct chunk captures inherit
    /// the fact: changed reference values and reconstructed owned captures do
    /// not. This marker alone is not a lifetime certificate: a BoxArrayLength
    /// observation must originate in a checked typed read. EFF-2 retains that
    /// read even in a zero-trip body, and EFF-5 separates it from reference
    /// writes and by-value consumption throughout the call. Scheduling uses
    /// those checked facts without inferring new lifetimes.
    pub(crate) readonly_reference_parameters: Vec<IrValueId>,
    /// Checked source modes, or `None` for a compiler-synthesized function.
    /// Internal transfer contracts must not be invented from representation.
    pub(crate) source_signature: Option<IrSourceSignature>,
    /// Only calls lowered from checked source; synthesized calls do not
    /// acquire invented source use or provenance records.
    pub(crate) source_calls: Vec<IrSourceCall>,
    pub(crate) result: IrType,
    pub(crate) values: Vec<IrType>,
    pub(crate) blocks: Vec<IrBlock>,
    /// Counted extents retained only for the scheduler's work estimate.
    pub(crate) counted_ranges: Vec<IrCountedRange>,
    pub(crate) overlaps: Vec<IrOverlap>,
    pub(crate) synthesis: Option<IrSynthesis>,
}

impl IrFunction {
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Why this function exists, or `None` for a source function.
    pub const fn synthesis(&self) -> Option<IrSynthesis> {
        self.synthesis
    }

    pub const fn result(&self) -> IrType {
        self.result
    }

    pub fn parameters(&self) -> &[(IrValueId, IrType)] {
        &self.parameters
    }

    pub fn blocks(&self) -> &[IrBlock] {
        &self.blocks
    }

    pub(crate) const fn source_signature(&self) -> Option<&IrSourceSignature> {
        self.source_signature.as_ref()
    }

    pub(crate) fn source_calls(&self) -> &[IrSourceCall] {
        &self.source_calls
    }

    /// The permission-derived overlap groups of this function's body, in
    /// source order and pairwise disjoint in their members.
    pub fn overlaps(&self) -> &[IrOverlap] {
        &self.overlaps
    }

    pub(crate) fn contains_buffer(&self) -> bool {
        self.values
            .iter()
            .any(|ty| matches!(ty, IrType::Buffer { .. }))
    }

    /// Every defined value's type, for whole-program type enumeration.
    pub(crate) fn value_types(&self) -> &[IrType] {
        &self.values
    }

    pub(crate) fn value_type(&self, value: IrValueId) -> Option<IrType> {
        self.values.get(value.index()).copied()
    }
}

#[derive(Debug)]
pub struct IrProgram {
    pub(crate) nominals: Vec<IrNominal>,
    pub(crate) elements: Vec<IrType>,
    pub(crate) constants: Vec<IrGlobalConstant>,
    pub(crate) functions: Vec<IrFunction>,
    pub(crate) actualization: Vec<String>,
    pub(crate) sequential_compute_refusal: bool,
    pub(crate) recursion_budget: Option<RecursionBudget>,
    /// Construction work is not recoverable from final IR after a refusal.
    #[cfg(test)]
    pub(crate) loop_candidate_constructions: usize,
}

impl IrProgram {
    /// How a compute-actualizing lowering fixes the recursion budget, or
    /// `None` where this lowering actualizes no compute at all. Selects
    /// emitted machine code; never an acceptance bound.
    pub(crate) const fn recursion_budget(&self) -> Option<RecursionBudget> {
        self.recursion_budget
    }

    /// Opt-in machine-code selection after a refused compute acquisition.
    pub(crate) const fn sequential_compute_refusal(&self) -> bool {
        self.sequential_compute_refusal
    }

    pub fn nominals(&self) -> &[IrNominal] {
        &self.nominals
    }

    pub fn elements(&self) -> &[IrType] {
        &self.elements
    }

    pub fn element(&self, element: IrElement) -> Option<IrType> {
        self.elements.get(element.index()).copied()
    }

    pub fn nominal(&self, id: IrNominalId) -> Option<&IrNominal> {
        self.nominals.get(id.index())
    }

    pub fn constants(&self) -> &[IrGlobalConstant] {
        &self.constants
    }

    pub fn constant(&self, id: IrConstantId) -> Option<&IrGlobalConstant> {
        self.constants.get(id.index())
    }

    pub fn functions(&self) -> &[IrFunction] {
        &self.functions
    }

    /// The non-normative ledger lines this lowering added: one per permitted
    /// counted loop it either actualized or declined to.
    ///
    /// The judgment's own ledger states what [PAR-2] permits and is the same
    /// with or without `--par`. These state what *this* lowering did with a
    /// permission, which is a different fact and exists only where actualization
    /// was asked for. Both are developer output on the caller's channel; neither
    /// participates in acceptance or in any mandatory [DIAG-2] record.
    pub fn actualization_ledger(&self) -> &[String] {
        &self.actualization
    }
}
