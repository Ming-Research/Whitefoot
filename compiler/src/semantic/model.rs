use crate::{BuiltinPreludeId, DeclarationId, NodePath};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct FunctionId(pub(crate) u32);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct BindingId(pub(crate) u32);

/// Checked-program-private identity of one exact instantiated FN-4
/// implication query. It names the retained query record, never a dense term,
/// goal, or derivation identity inside that query's isolated proof arena.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ContractQueryId(pub(crate) u32);

/// The three checked value and reference kinds [GRAM-3].
///
/// A parameter's `T`, `&T`, or `&[T]` spelling determines its kind before
/// substitution; a result always has value mode [GRAM-2, FN-1]. There is no
/// permission marker and no region on a reference [REF-1], so these three
/// kinds carry nothing: a reference is a local name for a path and the path
/// is carried beside the binding, not inside its kind.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum CheckedMode {
    Own,
    /// `&T`: a name for one path [REF-1].
    Reference,
    /// `&[T]`: a name for a range of elements, whose one measure is `len`
    /// [REF-4]. It is a reference kind and not a type [TYPE-8], so it is
    /// never a stored value, never a result, and never a generic argument.
    Range,
}

impl CheckedMode {
    /// Whether this kind names a path rather than owning storage [REF-1].
    pub(crate) const fn is_reference(self) -> bool {
        matches!(self, Self::Reference | Self::Range)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct CheckedLoopId(pub(crate) u32);

/// One proof-only mathematical integer expression. Each leaf retains
/// its exact source integer type while denoting its value in the mathematical
/// integers; this metadata does not request a runtime conversion.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct CheckedAffineExpression {
    pub(crate) node_path: NodePath,
    pub(crate) kind: CheckedAffineExpressionKind,
}

impl CheckedAffineExpression {
    /// Children before their parent, with leaves in written left-to-right
    /// order, independent of the expression's nesting depth.
    pub(crate) fn postorder(&self) -> impl Iterator<Item = &Self> {
        let mut pending = vec![(self, false)];
        std::iter::from_fn(move || {
            loop {
                let (expression, visited) = pending.pop()?;
                if !visited {
                    match &expression.kind {
                        CheckedAffineExpressionKind::Add(left, right)
                        | CheckedAffineExpressionKind::Subtract(left, right) => {
                            pending.push((expression, true));
                            pending.push((right, false));
                            pending.push((left, false));
                            continue;
                        }
                        CheckedAffineExpressionKind::MultiplyByConstant { value, .. } => {
                            pending.push((expression, true));
                            pending.push((value, false));
                            continue;
                        }
                        _ => {}
                    }
                }
                return Some(expression);
            }
        })
    }
}

impl Clone for CheckedAffineExpression {
    fn clone(&self) -> Self {
        let mut values = Vec::new();
        for expression in self.postorder() {
            let kind = match &expression.kind {
                CheckedAffineExpressionKind::Add(_, _)
                | CheckedAffineExpressionKind::Subtract(_, _) => {
                    let right = Box::new(values.pop().expect("postorder retains the right child"));
                    let left = Box::new(values.pop().expect("postorder retains the left child"));
                    if matches!(expression.kind, CheckedAffineExpressionKind::Add(_, _)) {
                        CheckedAffineExpressionKind::Add(left, right)
                    } else {
                        CheckedAffineExpressionKind::Subtract(left, right)
                    }
                }
                CheckedAffineExpressionKind::MultiplyByConstant {
                    constant,
                    constant_ty,
                    ..
                } => CheckedAffineExpressionKind::MultiplyByConstant {
                    constant: *constant,
                    constant_ty: *constant_ty,
                    value: Box::new(values.pop().expect("postorder retains the scaled child")),
                },
                leaf => leaf.clone(),
            };
            values.push(Self {
                node_path: expression.node_path.clone(),
                kind,
            });
        }
        values.pop().expect("postorder visits the root")
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CheckedAffineExpressionKind {
    Constant {
        value: i128,
        ty: IntegerType,
    },
    Local {
        binding: BindingId,
        ty: IntegerType,
    },
    /// [INV-1, MSR-6] one in-scope const generic written as an affine atom,
    /// at the one source-canonical symbolic instance.
    ///
    /// A concrete instance reads the mathematical constant [FN-2] fixed for
    /// it and forms [`Self::Constant`]; the symbolic instance keeps the
    /// declaration-anchored constant term [ENT-2] clause (c) already gives
    /// it, which nothing kills, so the affine image is one immutable atom for
    /// the whole walk.
    ConstGeneric {
        declaration: DeclarationId,
        ty: IntegerType,
        /// The writer's own spelling, so a residual renders `n` and never an
        /// internal identity.
        name: String,
    },
    /// [INV-1, MSR-1] one measure former written as an affine factor. The
    /// inner expression is exactly the measure read the [OP-1] reader row
    /// checks, so the affine domain reaches the same [ENT-2] term the
    /// automatic derivation already carries an atom for; the invariant
    /// evaluates nothing and reads no storage [INV-1].
    Measure(Box<CheckedExpression>),
    Add(Box<CheckedAffineExpression>, Box<CheckedAffineExpression>),
    Subtract(Box<CheckedAffineExpression>, Box<CheckedAffineExpression>),
    MultiplyByConstant {
        constant: i128,
        constant_ty: IntegerType,
        value: Box<CheckedAffineExpression>,
    },
}

impl Drop for CheckedAffineExpression {
    fn drop(&mut self) {
        fn take_children(
            expression: &mut CheckedAffineExpression,
            pending: &mut Vec<CheckedAffineExpression>,
        ) {
            let kind = std::mem::replace(
                &mut expression.kind,
                CheckedAffineExpressionKind::Constant {
                    value: 0,
                    ty: IntegerType::U64,
                },
            );
            match kind {
                CheckedAffineExpressionKind::Add(left, right)
                | CheckedAffineExpressionKind::Subtract(left, right) => {
                    pending.push(*left);
                    pending.push(*right);
                }
                CheckedAffineExpressionKind::MultiplyByConstant { value, .. } => {
                    pending.push(*value)
                }
                _ => {}
            }
        }

        // The source tree can exceed INV-1's formation capacity before it
        // reaches normalization, including while unwinding a source error.
        let mut pending = Vec::new();
        take_children(self, &mut pending);
        while let Some(mut expression) = pending.pop() {
            take_children(&mut expression, &mut pending);
        }
    }
}

/// One normalized source-written affine ordered relation. `left - right <=
/// bound` has `bound == 0` for non-strict order and `bound == -1` for strict
/// integer order. The checker has already admitted the expression vocabulary,
/// but this record alone grants no fact: INV-1 or PRF-1 must still prove its
/// owning judgment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedAffineRelation {
    pub(crate) node_path: NodePath,
    pub(crate) left: CheckedAffineExpression,
    pub(crate) right: CheckedAffineExpression,
    pub(crate) bound: i128,
    /// [INV-1] the written relation was `a == b`, which normalizes to the
    /// bound *pair* `a-b <= 0` and `b-a <= 0`, each proved as one batch
    /// member. The record carries `a-b <= 0`; this flag says the second
    /// member exists and is the same two sides reversed.
    pub(crate) equality: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedLoopInvariant {
    pub(crate) loop_id: CheckedLoopId,
    pub(crate) declaration: DeclarationId,
    pub(crate) name: String,
    pub(crate) relation: CheckedAffineRelation,
}

/// One source-written `use` in a local invariant certificate.
///
/// `multiplicity` is how many times the premise is added into the certificate
/// sum. The omitted source spelling is represented as the literal one. This
/// record deliberately contains no accumulating state: every use is checked
/// against the invariant statement's same entering proof context.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedProofUse {
    pub(crate) node_path: NodePath,
    pub(crate) multiplicity: CheckedProofMultiplicity,
    pub(crate) source: CheckedProofUseSource,
}

/// How many times one written `use` adds its premise into the certificate sum.
///
/// A bare decimal is a proof-domain integer known where the certificate is
/// checked. A named value is read at the entering program point instead;
/// [PRF-1] admits only an unsigned integer there, so the nonnegativity the
/// scaling step needs is a property of the written type rather than an
/// obligation the certificate must separately discharge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CheckedProofMultiplicity {
    Literal(i128),
    Value { binding: BindingId, ty: IntegerType },
}

/// The source selected by one written `use`.
///
/// A named source is the immutable theorem image published by the resolved
/// invariant declaration. A relation source is independently proved by AUTO
/// in the local invariant's entering context.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CheckedProofUseSource {
    Named(DeclarationId),
    Relation(CheckedAffineRelation),
}

/// One erased source-written local invariant. Every `use` and the target are
/// written in the `.wf` source; later analysis proves each use independently,
/// follows the written multipliers, and publishes only the checked target.
///
/// The historical type name remains internal while the parser surface moves
/// from `prove` to `invariant`; it does not grant a separate proof language or
/// runtime operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedSourceProof {
    pub(crate) node_path: NodePath,
    pub(crate) declaration: DeclarationId,
    pub(crate) name: String,
    pub(crate) target: CheckedAffineRelation,
    pub(crate) uses: Vec<CheckedProofUse>,
}

/// The checked source production that owns a value initializer. These forms
/// share GIVE-1 typing and lowering and ENT-5 relation delivery.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ValueInitializerKind {
    ValueIf,
    ValueMatch,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct NominalId(pub(crate) u32);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct CheckedConstantId(pub(crate) u32);

/// One checker-interned symbolic const operation [`DerivedConst`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct DerivedConstId(pub(crate) u32);

/// The five bare const-expression operations of the CONST-1 candidate
/// grammar. Const evaluation happens at monomorphization in the unsigned
/// 64-bit domain under the const-eval overflow policy: a result outside that
/// domain or a zero divisor is a compile-time rejection citing CONST-1, never
/// a runtime trap, so this family is disjoint from the runtime arithmetic
/// modes and excluded from EFF-2's state/allocation effect relation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ConstOperation {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
}

impl ConstOperation {
    pub(crate) const fn spelling(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Subtract => "-",
            Self::Multiply => "*",
            Self::Divide => "/",
            Self::Remainder => "%",
        }
    }
}

/// One interned symbolic const-expression node: exactly one operation over
/// two operands, mirroring the one-operation source grammar. At least one
/// operand is symbolic — a fully concrete operation is evaluated eagerly and
/// never interned — so a value of this shape exists only while a generic
/// template or symbolic validation instance is being checked, and every
/// concrete instantiation evaluates it away.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct DerivedConst {
    pub(crate) operation: ConstOperation,
    pub(crate) left: CheckedConst,
    pub(crate) right: CheckedConst,
}

/// Evaluates one const operation in the u64 const-eval domain.
///
/// `None` is the const-eval overflow policy's rejection premise: the
/// mathematical result is outside the domain, or the divisor is zero.
pub(crate) const fn evaluate_const_operation(
    operation: ConstOperation,
    left: u64,
    right: u64,
) -> Option<u64> {
    match operation {
        ConstOperation::Add => left.checked_add(right),
        ConstOperation::Subtract => left.checked_sub(right),
        ConstOperation::Multiply => left.checked_mul(right),
        ConstOperation::Divide => left.checked_div(right),
        ConstOperation::Remainder => left.checked_rem(right),
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum CheckedConst {
    Value(u64),
    Parameter(DeclarationId),
    /// One symbolic const operation, by checker-interned identity. Structural
    /// identity is id identity because interning is hash-consed.
    Derived(DerivedConstId),
}

impl CheckedConst {
    pub(crate) const fn value(self) -> Option<u64> {
        match self {
            Self::Value(value) => Some(value),
            Self::Parameter(_) | Self::Derived(_) => None,
        }
    }

    pub(crate) const fn is_concrete(self) -> bool {
        matches!(self, Self::Value(_))
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum IntegerType {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
}

impl IntegerType {
    pub(crate) const fn width(self) -> u8 {
        match self {
            Self::I8 | Self::U8 => 8,
            Self::I16 | Self::U16 => 16,
            Self::I32 | Self::U32 => 32,
            Self::I64 | Self::U64 => 64,
        }
    }

    pub(crate) const fn signed(self) -> bool {
        matches!(self, Self::I8 | Self::I16 | Self::I32 | Self::I64)
    }

    pub(crate) const fn converts_totally_to(self, destination: Self) -> bool {
        self.width() < destination.width()
            && (self.signed() == destination.signed() || (!self.signed() && destination.signed()))
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum FloatType {
    F32,
    F64,
}

impl FloatType {
    pub(crate) const fn width(self) -> u8 {
        match self {
            Self::F32 => 32,
            Self::F64 => 64,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum CheckedConversionMode {
    Exact,
    Checked,
    Defined,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum CheckedNumericType {
    Integer(IntegerType),
    Float(FloatType),
    GenericInteger(DeclarationId),
    GenericFloat(DeclarationId),
}

impl CheckedNumericType {
    /// The checked type this numeric type names.
    pub(crate) const fn checked_type(self) -> CheckedType {
        match self {
            Self::Integer(ty) => CheckedType::Integer(ty),
            Self::Float(ty) => CheckedType::Float(ty),
            Self::GenericInteger(declaration) => CheckedType::GenericInt(declaration),
            Self::GenericFloat(declaration) => CheckedType::GenericFloat(declaration),
        }
    }

    pub(crate) const fn from_type(ty: CheckedType) -> Option<Self> {
        match ty {
            CheckedType::Integer(ty) => Some(Self::Integer(ty)),
            CheckedType::Float(ty) => Some(Self::Float(ty)),
            CheckedType::GenericInt(declaration) => Some(Self::GenericInteger(declaration)),
            CheckedType::GenericFloat(declaration) => Some(Self::GenericFloat(declaration)),
            _ => None,
        }
    }

    pub(crate) const fn ty(self) -> CheckedType {
        match self {
            Self::Integer(ty) => CheckedType::Integer(ty),
            Self::Float(ty) => CheckedType::Float(ty),
            Self::GenericInteger(declaration) => CheckedType::GenericInt(declaration),
            Self::GenericFloat(declaration) => CheckedType::GenericFloat(declaration),
        }
    }

    /// [OP-6] whole-type totality over the finite domains of numeric bounds.
    /// Repeated parameters denote one type choice, so a symbolic identity is
    /// total before the independent endpoint domains are enumerated.
    pub(crate) fn converts_totally_to(self, destination: Self) -> bool {
        if self == destination {
            return true;
        }
        self.concrete_domain().iter().copied().all(|source| {
            destination
                .concrete_domain()
                .iter()
                .copied()
                .all(|destination| {
                    source == destination || source.concrete_converts_totally_to(destination)
                })
        })
    }

    pub(crate) fn concrete_domain(&self) -> &[Self] {
        const INTEGERS: [CheckedNumericType; 8] = [
            CheckedNumericType::Integer(IntegerType::I8),
            CheckedNumericType::Integer(IntegerType::I16),
            CheckedNumericType::Integer(IntegerType::I32),
            CheckedNumericType::Integer(IntegerType::I64),
            CheckedNumericType::Integer(IntegerType::U8),
            CheckedNumericType::Integer(IntegerType::U16),
            CheckedNumericType::Integer(IntegerType::U32),
            CheckedNumericType::Integer(IntegerType::U64),
        ];
        const FLOATS: [CheckedNumericType; 2] = [
            CheckedNumericType::Float(FloatType::F32),
            CheckedNumericType::Float(FloatType::F64),
        ];
        match self {
            Self::GenericInteger(_) => &INTEGERS,
            Self::GenericFloat(_) => &FLOATS,
            Self::Integer(_) | Self::Float(_) => std::slice::from_ref(self),
        }
    }

    const fn concrete_converts_totally_to(self, destination: Self) -> bool {
        match (self, destination) {
            (Self::Integer(source), Self::Integer(destination)) => {
                source.converts_totally_to(destination)
            }
            (Self::Integer(source), Self::Float(FloatType::F32)) => source.width() <= 16,
            (Self::Integer(source), Self::Float(FloatType::F64)) => source.width() <= 32,
            (Self::Float(FloatType::F32), Self::Float(FloatType::F64)) => true,
            _ => false,
        }
    }

    pub(crate) const fn reinterprets_to(self, destination: Self) -> bool {
        match (self, destination) {
            (Self::Integer(source), Self::Integer(destination)) => {
                source.width() == destination.width() && source.signed() != destination.signed()
            }
            (Self::Integer(source), Self::Float(destination))
            | (Self::Float(destination), Self::Integer(source)) => {
                source.width() == destination.width()
            }
            _ => false,
        }
    }
}

/// [STOR-1, STOR-3] which release action a compiler-owned cell performs.
///
/// The active language has one heap and therefore one represented action:
/// free the cell after releasing its content. The checked class travels into
/// the IR so lowering preserves that decision rather than rederiving it.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum CheckedReleaseClass {
    /// Release the cell to the language's one heap.
    General,
}

/// [TYPE-2, TYPE-9] the complete type of one array or run element, interned in
/// the checked program.
/// Structural children precede parents; recursive ownership graphs pass through
/// nominal identities. The handle keeps every checked type compact and Copy.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct CheckedElement(pub(crate) u32);

impl CheckedElement {
    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum CheckedType {
    Unit,
    Bool,
    Integer(IntegerType),
    Float(FloatType),
    Generic(DeclarationId),
    GenericInt(DeclarationId),
    GenericFloat(DeclarationId),
    Nominal(NominalId),
    /// One constant-capacity `Array<T, N>` [TYPE-9]: `N` slots, every one of
    /// them always holding a value. Its `len` is the type constant and is
    /// stored nowhere; the type has no `cap` measure [WIN-1, MSR-1].
    Array {
        element: CheckedElement,
        length: CheckedConst,
    },
    /// One runtime-capacity `Array<T>` [TYPE-9]. Its `len` is the allocated
    /// slot count, the one runtime number its block stores [WIN-1, MSR-1].
    Buffer {
        element: CheckedElement,
    },
    /// One `Slots<T, N>`, `Slots<T>`, `Ring<T, N>` or `Ring<T>` [TYPE-9]: a
    /// run of `cap` slots whose initialized storage is the window of `len`
    /// slots beginning at `head` modulo `cap` [WIN-1].
    Window {
        shape: WindowShape,
        element: CheckedElement,
        /// `Some` is the constant-capacity placement, whose capacity is the
        /// type constant and is stored nowhere; `None` is the
        /// runtime-capacity placement, whose capacity is a measure fixed at
        /// construction and stored with the block [TYPE-9, MSR-1].
        capacity: Option<CheckedConst>,
    },
}

/// Which of [TYPE-9]'s two window shapes a [`CheckedType::Window`] is.
///
/// The shapes are two types and never one: [WIN-1] gives `head` to a `Ring`
/// alone, [MSR-1] makes that one cell the only *bounded* cell in the whole
/// table, and compiler/storage-representation gives the two shapes two
/// emitted types so that neither carries a word no measure of it needs.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum WindowShape {
    /// `Slots`: the window begins at slot zero and there is no `head`.
    Slots,
    /// `Ring`: the window begins at `head` and wraps modulo `cap`.
    Ring,
}

impl WindowShape {
    /// The [TYPE-9] spelling, for a diagnostic that renders the type.
    pub(crate) const fn spelling(self) -> &'static str {
        match self {
            Self::Slots => "Slots",
            Self::Ring => "Ring",
        }
    }
}

impl CheckedType {
    pub(crate) fn is_concrete(self, elements: &[CheckedType]) -> bool {
        match self {
            Self::Generic(_) | Self::GenericInt(_) | Self::GenericFloat(_) => false,
            Self::Array { element, length } => {
                elements
                    .get(element.0 as usize)
                    .is_some_and(|ty| ty.is_concrete(elements))
                    && length.is_concrete()
            }
            Self::Window {
                element, capacity, ..
            } => {
                elements
                    .get(element.0 as usize)
                    .is_some_and(|ty| ty.is_concrete(elements))
                    && capacity.is_none_or(|capacity| capacity.is_concrete())
            }
            Self::Buffer { element } => elements
                .get(element.index())
                .is_some_and(|ty| ty.is_concrete(elements)),
            Self::Unit | Self::Bool | Self::Integer(_) | Self::Float(_) | Self::Nominal(_) => true,
        }
    }

    /// The [MSR-1] row this type selects, or `None` when the table gives it
    /// none.
    pub(crate) const fn measured(self) -> Option<MeasuredKind> {
        match self {
            Self::Array { .. } => Some(MeasuredKind::ConstantArray),
            Self::Buffer { .. } => Some(MeasuredKind::RuntimeArray),
            Self::Window {
                shape: WindowShape::Slots,
                capacity: Some(_),
                ..
            } => Some(MeasuredKind::ConstantSlots),
            Self::Window {
                shape: WindowShape::Slots,
                capacity: None,
                ..
            } => Some(MeasuredKind::RuntimeSlots),
            Self::Window {
                shape: WindowShape::Ring,
                capacity: Some(_),
                ..
            } => Some(MeasuredKind::ConstantRing),
            Self::Window {
                shape: WindowShape::Ring,
                capacity: None,
                ..
            } => Some(MeasuredKind::RuntimeRing),
            _ => None,
        }
    }
}

/// One of [MSR-1]'s three measures of a measured value.
///
/// The spelling and the [ENT-2] term are the same quantity read two ways, so
/// one enum keys both the [OP-1] reader row and the measure term.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum CheckedMeasure {
    Length,
    Capacity,
    Head,
}

/// One cell of [MSR-1]'s measure table.
///
/// Exact cells distinguish their value source; `Ring.head` is bounded, and
/// an undeclared measure is absent. All three classifications come from the
/// specification's table, independently of a familiar member spelling.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MeasureCell {
    /// The measure is exactly the measured value's own extent, which the
    /// `len` reader already loads.
    ExactExtent,
    /// The measure is exactly this compile-time constant.
    ExactConstant(u64),
    /// The measure is exactly the type's own written constant: an `Array`'s
    /// length or a constant-capacity window's capacity.
    ExactTypeConstant,
    /// The measure is exact and is an independent runtime quantity of the
    /// value's own descriptor: a run's `len` and a runtime-capacity window's
    /// `cap` [WIN-1].
    ExactRuntime,
    /// The measure is exact but only two-sidedly published by some writing
    /// operation. A Ring's `head` is the one measure of this class [MSR-1].
    Bounded,
    /// The type has no such measure.
    Absent,
}

impl MeasureCell {
    /// The [MSR-1] classification word this cell writes in the specification's
    /// own table: *exact*, *bounded*, or *absent*, and nothing else.
    #[cfg(test)]
    pub(crate) const fn classification(self) -> &'static str {
        match self {
            Self::ExactExtent
            | Self::ExactConstant(_)
            | Self::ExactTypeConstant
            | Self::ExactRuntime => "exact",
            Self::Bounded => "bounded",
            Self::Absent => "absent",
        }
    }
}

impl CheckedMeasure {
    /// The [MSR-1] member spelling of this measure: a measure is read as a
    /// member of the measured place, `p.len`, and the v0.59 `len_of(p)`
    /// former is not a v0.60 spelling [OP-15, TYPE-10].
    pub(crate) const fn spelling(self) -> &'static str {
        match self {
            Self::Length => "len",
            Self::Capacity => "cap",
            Self::Head => "head",
        }
    }

    /// [MSR-1]'s measure table, read row by row out of the rule's own fence.
    ///
    /// Each row determines the admitted members and their publication class;
    /// a member's spelling alone cannot establish either.
    pub(crate) const fn cell(self, measured: MeasuredKind) -> MeasureCell {
        match (measured, self) {
            // `Array<T, N>`: `len` is the type constant and every slot always
            // holds a value [WIN-1]. x1 gives the two `Array` rows no `cap`
            // cell at all: an array is its own extent and states no second
            // capacity quantity, so the row that used to answer `cap` with
            // the same number is absent rather than duplicated.
            (MeasuredKind::ConstantArray, Self::Length) => MeasureCell::ExactTypeConstant,
            // `Array<T>`: the one runtime number the block stores is its
            // allocated slot count.
            (MeasuredKind::RuntimeArray, Self::Length) => MeasureCell::ExactRuntime,
            // The four window rows: `len` is a stored runtime number, while
            // `cap` is the type constant in the constant-capacity placement
            // and the slot count taken at construction in the runtime one.
            (
                MeasuredKind::ConstantSlots
                | MeasuredKind::RuntimeSlots
                | MeasuredKind::ConstantRing
                | MeasuredKind::RuntimeRing,
                Self::Length,
            )
            | (MeasuredKind::RuntimeSlots | MeasuredKind::RuntimeRing, Self::Capacity) => {
                MeasureCell::ExactRuntime
            }
            (MeasuredKind::ConstantSlots | MeasuredKind::ConstantRing, Self::Capacity) => {
                MeasureCell::ExactTypeConstant
            }
            // `&[T]`: the range's element count, and nothing else [MSR-1].
            (MeasuredKind::Range, Self::Length) => MeasureCell::ExactRuntime,
            // The one *bounded* cell of the whole table: the two front-moving
            // operations publish a `Ring`'s window origin two-sidedly and no
            // operation re-establishes it exactly [MSR-1, OP-10].
            (MeasuredKind::ConstantRing | MeasuredKind::RuntimeRing, Self::Head) => {
                MeasureCell::Bounded
            }
            // `head` is absent on every row but the two `Ring` rows, and
            // neither an `Array` row nor a range has `cap` or `head`.
            (
                MeasuredKind::ConstantArray
                | MeasuredKind::RuntimeArray
                | MeasuredKind::ConstantSlots
                | MeasuredKind::RuntimeSlots,
                Self::Head,
            )
            | (
                MeasuredKind::ConstantArray | MeasuredKind::RuntimeArray | MeasuredKind::Range,
                Self::Capacity,
            )
            | (MeasuredKind::Range, Self::Head) => MeasureCell::Absent,
        }
    }
}

/// One measured type [MSR-1]: exactly a type the measure table gives a row.
///
/// The table's seven rows are seven identities because the two placements of
/// one shape answer `cap` differently: a constant-capacity row reads the
/// type constant and stores nothing, a runtime-capacity one reads a word its
/// block stores.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum MeasuredKind {
    /// `Array<T, N>`
    ConstantArray,
    /// `Array<T>`
    RuntimeArray,
    /// `Slots<T, N>`
    ConstantSlots,
    /// `Slots<T>`
    RuntimeSlots,
    /// `Ring<T, N>`
    ConstantRing,
    /// `Ring<T>`
    RuntimeRing,
    /// `&[T]` [REF-4], whose one measure is `len`.
    Range,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum CheckedValue {
    Unit,
    Bool(bool),
    Integer {
        ty: IntegerType,
        bits: u64,
    },
    Float {
        ty: FloatType,
        bits: u64,
    },
    /// One in-scope const generic read as a value [MSR-6]. It is a
    /// monomorphization-time constant, so a concrete [FN-2] instance folds
    /// it to `Integer` at substitution and only the one source-canonical
    /// symbolic instance retains this form, where [ENT-2] clause (c)
    /// already makes it the symbolic constant term.
    ConstGeneric {
        declaration: DeclarationId,
        ty: IntegerType,
    },
    NumericIdentity {
        ty: CheckedType,
        one: bool,
    },
    Array {
        ty: CheckedType,
        elements: Vec<CheckedValue>,
    },
    /// One struct-typed constant value [CONST-2 candidate]: the nominal
    /// instance plus its complete field values in declared order.
    Struct {
        ty: CheckedType,
        fields: Vec<CheckedValue>,
    },
}

impl CheckedValue {
    pub(crate) const fn ty(&self) -> CheckedType {
        match self {
            Self::Unit => CheckedType::Unit,
            Self::Bool(_) => CheckedType::Bool,
            Self::Integer { ty, .. } => CheckedType::Integer(*ty),
            Self::Float { ty, .. } => CheckedType::Float(*ty),
            Self::ConstGeneric { ty, .. } => CheckedType::Integer(*ty),
            Self::NumericIdentity { ty, .. } => *ty,
            Self::Array { ty, .. } => *ty,
            Self::Struct { ty, .. } => *ty,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedConstant {
    pub(crate) id: CheckedConstantId,
    /// Resolved declaration identity retained for named-const goal leaves.
    pub(crate) declaration: DeclarationId,
    pub(crate) name: String,
    /// Source value type, before a fixed-run constant's descriptor-free
    /// dense storage normalization. Borrowing cannot change this identity.
    pub(crate) declared_type: CheckedType,
    pub(crate) ty: CheckedType,
    pub(crate) value: CheckedValue,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedField {
    pub(crate) name: String,
    pub(crate) ty: CheckedType,
    /// [TYPE-2] whether the declaration wrote `readonly` before the name.
    ///
    /// A path that ends at or passes through such a field is never a write
    /// target: construction gives it its value like any other field and a
    /// whole-value assignment replaces it together with its owner, but a
    /// `set` on it and an argument naming it at a written reference parameter
    /// are refused. It states that the field is not assignable, not that its
    /// value is constant: a compiler-owned [PRE-1] operation whose row
    /// declares `writes` of it still changes it.
    pub(crate) readonly: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedVariant {
    pub(crate) name: String,
    pub(crate) constructor: CheckedConstructor,
    pub(crate) tag: u32,
    pub(crate) fields: Vec<CheckedField>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CheckedConstructor {
    Source(DeclarationId),
    Prelude(BuiltinPreludeId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CheckedNominalKind {
    Struct {
        fields: Vec<CheckedField>,
    },
    Enum {
        variants: Vec<CheckedVariant>,
    },
    /// One boxed cell. `region` is `None` for the ambient-heap `box<T>`
    /// [STOR-2] and `Some(store)` for the store-branded `Box<'s, T>` S39,
    /// whose region is a component of its type exactly as a run's is
    /// [PROV-1] and whose release class that region decides [PROV-6].
    Box {
        referent: CheckedType,
        region: Option<DeclarationId>,
        /// [PROV-6] which release action this cell's own reclamation is, read
        /// off `region` at the moment the nominal is interned. The ambient
        /// heap's `box<T>` and a general store's cell both free; a bump
        /// extent's cell is reclaimed by its region's own reset.
        release: CheckedReleaseClass,
    },
    /// An ordinary opaque nominal has no fields or constructor.
    Opaque,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedNominal {
    pub(crate) id: NominalId,
    pub(crate) name: String,
    pub(crate) kind: CheckedNominalKind,
    /// [PROV-6] whether this nominal's declaration carries the `nodrop`
    /// modifier, which removes the drop capability and copy with it [OWN-1].
    /// The field keeps the name of the class it produces: a marked nominal is
    /// linear.
    pub(crate) linear: bool,
    /// [OWN-1, GRAM-2] whether this nominal's declaration removes the copy
    /// capability alone: the written `nocopy` modifier and the cell nominals
    /// this compiler interns for declarations [PRE-1] writes `nocopy`.
    pub(crate) nocopy: bool,
}

impl CheckedNominal {
    /// Whether this is an enum whose every variant is nullary. This is a
    /// fact about the value's shape, which selects its flat-element and tag
    /// representation; its copy capability is [`type_has_copy_capability`].
    pub(crate) fn is_tag_only_enum(&self) -> bool {
        matches!(
            &self.kind,
            CheckedNominalKind::Enum { variants }
                if variants.iter().all(|variant| variant.fields.is_empty())
        )
    }
}

/// [OWN-1] whether a type has the copy capability.
///
/// Primitives have it. Every other type has it exactly when every part it
/// owns has it and its declaration does not remove it: a struct's fields and
/// an enum's variant payload fields are its parts, an `Array<T, N>` has the
/// capabilities of its element, and `Slots`, `Ring`, `Box` and the host
/// handles are declared `nocopy` or `nodrop` [PRE-1, PRE-2]. `parameter` answers for
/// a type parameter standing for itself, whose capabilities are the ones its
/// written bound grants [PROV-6]. `None` reports a nominal or element handle
/// the tables do not hold.
///
/// A runtime-capacity `Array<T>` is never a value outside its `Box` [TYPE-9],
/// so no bare read of one exists to be a copy and it answers false. An opaque
/// nominal retains no fields, so its answer is its modifier's alone.
pub(crate) fn type_has_copy_capability(
    ty: CheckedType,
    nominals: &[CheckedNominal],
    elements: &[CheckedType],
    parameter: &dyn Fn(DeclarationId) -> Option<bool>,
) -> Option<bool> {
    let mut visited = Vec::new();
    let mut pending = vec![ty];
    while let Some(current) = pending.pop() {
        match current {
            CheckedType::Unit
            | CheckedType::Bool
            | CheckedType::Integer(_)
            | CheckedType::Float(_)
            | CheckedType::GenericInt(_)
            | CheckedType::GenericFloat(_) => {}
            CheckedType::Generic(declaration) => {
                if !parameter(declaration)? {
                    return Some(false);
                }
            }
            CheckedType::Array { element, .. } => {
                pending.push(*elements.get(element.index())?);
            }
            CheckedType::Buffer { .. } | CheckedType::Window { .. } => return Some(false),
            CheckedType::Nominal(id) => {
                // A nominal met again is already being judged on this walk,
                // so it adds no part the walk has not queued.
                if visited.contains(&id) {
                    continue;
                }
                visited.push(id);
                let nominal = nominals.get(id.0 as usize)?;
                if nominal.linear || nominal.nocopy {
                    return Some(false);
                }
                match &nominal.kind {
                    CheckedNominalKind::Struct { fields } => {
                        pending.extend(fields.iter().map(|field| field.ty));
                    }
                    CheckedNominalKind::Enum { variants } => pending.extend(
                        variants
                            .iter()
                            .flat_map(|variant| variant.fields.iter().map(|field| field.ty)),
                    ),
                    CheckedNominalKind::Opaque => {}
                    CheckedNominalKind::Box { .. } => {
                        return Some(false);
                    }
                }
            }
        }
    }
    Some(true)
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum CheckedIntegerOperation {
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

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum CheckedBooleanOperation {
    And,
    Or,
    ExclusiveOr,
    Not,
}

impl CheckedBooleanOperation {
    /// The [OP-1] spelling of each Bool row, exhaustive by construction.
    pub(crate) const fn spelling(self) -> &'static str {
        match self {
            Self::And => "band",
            Self::Or => "bor",
            Self::ExclusiveOr => "bxor",
            Self::Not => "bnot",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum CheckedFloatOperation {
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

impl CheckedFloatOperation {
    /// The [OP-1] spelling of each float operation the compiler models,
    /// exhaustive by construction and locked against the specification table
    /// by `semantic::tests::operation_table`.
    pub(crate) const fn spelling(self) -> &'static str {
        match self {
            Self::AddStrict => "fadd.strict",
            Self::SubtractStrict => "fsub.strict",
            Self::MultiplyStrict => "fmul.strict",
            Self::DivideStrict => "fdiv.strict",
            Self::Equal => "feq",
            Self::Less => "flt",
            Self::LessEqual => "fle",
            Self::Greater => "fgt",
            Self::GreaterEqual => "fge",
            Self::NotEqual => "fne",
            Self::Negate => "fneg",
            Self::Absolute => "fabs",
            Self::CopySign => "fcopysign",
            Self::Minimum => "fmin",
            Self::Maximum => "fmax",
            Self::Floor => "ffloor",
            Self::Ceil => "fceil",
            Self::Truncate => "ftrunc",
            Self::RoundEven => "froundeven",
            Self::Remainder => "frem",
            Self::SquareRootStrict => "fsqrt.strict",
            Self::FusedMultiplyAddStrict => "ffma.strict",
            Self::Infinity => "finf",
            Self::Nan => "fnan",
        }
    }

    pub(crate) const fn operand_count(self) -> usize {
        match self {
            Self::Infinity | Self::Nan => 0,
            Self::Negate
            | Self::Absolute
            | Self::Floor
            | Self::Ceil
            | Self::Truncate
            | Self::RoundEven
            | Self::SquareRootStrict => 1,
            Self::FusedMultiplyAddStrict => 3,
            _ => 2,
        }
    }

    pub(crate) const fn result_type(self, operand: CheckedType) -> CheckedType {
        match self {
            Self::Equal
            | Self::Less
            | Self::LessEqual
            | Self::Greater
            | Self::GreaterEqual
            | Self::NotEqual => CheckedType::Bool,
            _ => operand,
        }
    }
}

/// The [PRE-1] error type a checked [OP-1] integer row reports.
///
/// The row's `signature` cell names it, so the choice is table data rather
/// than a property of the operation's semantics; this enum exists so that one
/// place decides it and an extraction lock can compare that decision against
/// the specification's own cell.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CheckedIntegerErrorClass {
    Overflow,
    DivError,
}

impl CheckedIntegerErrorClass {
    /// The exact PRE-1 spelling the `wf-ops` `signature` cell writes.
    ///
    /// Read only by the extraction lock: nothing in the compiler's own path
    /// needs the name, because the class already selects the prelude type.
    #[cfg(test)]
    pub(crate) const fn spelling(self) -> &'static str {
        match self {
            Self::Overflow => "Overflow",
            Self::DivError => "DivError",
        }
    }
}

impl CheckedIntegerOperation {
    /// The [OP-1] spelling of each integer operation the compiler models.
    ///
    /// Exhaustive by construction: a new variant is a compile error here,
    /// which is the point — the row it belongs to must be named before it can
    /// be checked. `semantic::tests::operation_table` locks this map against
    /// the specification's own `wf-ops` table in both directions, so every
    /// diagnostic that renders an operation in source terms renders the
    /// spelling the specification fixes.
    pub(crate) const fn spelling(self) -> &'static str {
        match self {
            Self::AddWrap => "+wrap",
            Self::SubtractWrap => "-wrap",
            Self::MultiplyWrap => "*wrap",
            Self::AddExact => "+",
            Self::SubtractExact => "-",
            Self::MultiplyExact => "*",
            Self::AddDefined => "+defined",
            Self::SubtractDefined => "-defined",
            Self::MultiplyDefined => "*defined",
            Self::AddChecked => "+checked",
            Self::SubtractChecked => "-checked",
            Self::MultiplyChecked => "*checked",
            Self::DivideExact => "/",
            Self::RemainderExact => "%",
            Self::DivideDefined => "/defined",
            Self::RemainderDefined => "%defined",
            Self::DivideChecked => "/checked",
            Self::RemainderChecked => "%checked",
            Self::AbsoluteWrap => "iabs.wrap",
            Self::AbsoluteExact => "iabs",
            Self::AbsoluteDefined => "iabs.defined",
            Self::AbsoluteChecked => "iabs.checked",
            Self::NegateWrap => "ineg.wrap",
            Self::NegateExact => "ineg",
            Self::NegateDefined => "ineg.defined",
            Self::NegateChecked => "ineg.checked",
            Self::BitAnd => "iand",
            Self::BitOr => "ior",
            Self::BitXor => "ixor",
            Self::BitNot => "inot",
            Self::ShiftLeftWrap => "ishl.wrap",
            Self::ShiftRightWrap => "ishr.wrap",
            Self::ShiftLeftExact => "ishl",
            Self::ShiftRightExact => "ishr",
            Self::ShiftLeftDefined => "ishl.defined",
            Self::ShiftRightDefined => "ishr.defined",
            Self::RotateLeft => "irotl",
            Self::RotateRight => "irotr",
            Self::PopulationCount => "ipopcount",
            Self::LeadingZeros => "iclz",
            Self::TrailingZeros => "ictz",
            Self::ByteSwap => "ibswap",
            Self::MultiplyHigh => "imulhi",
            Self::AddSaturating => "+sat",
            Self::SubtractSaturating => "-sat",
            Self::MultiplySaturating => "*sat",
            Self::Minimum => "imin",
            Self::Maximum => "imax",
            Self::Equal => "==",
            Self::NotEqual => "!=",
            Self::Less => "<",
            Self::LessEqual => "<=",
            Self::Greater => ">",
            Self::GreaterEqual => ">=",
        }
    }

    pub(crate) const fn is_exact(self) -> bool {
        matches!(
            self,
            Self::AddExact
                | Self::SubtractExact
                | Self::MultiplyExact
                | Self::DivideExact
                | Self::RemainderExact
                | Self::AbsoluteExact
                | Self::NegateExact
                | Self::ShiftLeftExact
                | Self::ShiftRightExact
        )
    }

    pub(crate) const fn defined_query(self) -> Option<Self> {
        Some(match self {
            Self::AddExact => Self::AddDefined,
            Self::SubtractExact => Self::SubtractDefined,
            Self::MultiplyExact => Self::MultiplyDefined,
            Self::DivideExact => Self::DivideDefined,
            Self::RemainderExact => Self::RemainderDefined,
            Self::AbsoluteExact => Self::AbsoluteDefined,
            Self::NegateExact => Self::NegateDefined,
            Self::ShiftLeftExact => Self::ShiftLeftDefined,
            Self::ShiftRightExact => Self::ShiftRightDefined,
            _ => return None,
        })
    }

    pub(crate) const fn is_defined_query(self) -> bool {
        matches!(
            self,
            Self::AddDefined
                | Self::SubtractDefined
                | Self::MultiplyDefined
                | Self::DivideDefined
                | Self::RemainderDefined
                | Self::AbsoluteDefined
                | Self::NegateDefined
                | Self::ShiftLeftDefined
                | Self::ShiftRightDefined
        )
    }

    pub(crate) const fn operand_count(self) -> usize {
        match self {
            Self::AbsoluteWrap
            | Self::AbsoluteExact
            | Self::AbsoluteDefined
            | Self::AbsoluteChecked
            | Self::NegateWrap
            | Self::NegateExact
            | Self::NegateDefined
            | Self::NegateChecked
            | Self::BitNot
            | Self::PopulationCount
            | Self::LeadingZeros
            | Self::TrailingZeros
            | Self::ByteSwap => 1,
            _ => 2,
        }
    }

    pub(crate) const fn accepts_operand_type(self, operand: CheckedType) -> bool {
        match (self, operand) {
            (
                Self::AbsoluteWrap
                | Self::AbsoluteExact
                | Self::AbsoluteDefined
                | Self::AbsoluteChecked
                | Self::NegateWrap
                | Self::NegateExact
                | Self::NegateDefined
                | Self::NegateChecked,
                CheckedType::Integer(operand),
            ) => operand.signed(),
            (Self::ByteSwap, CheckedType::Integer(operand)) => operand.width() >= 16,
            (
                Self::AbsoluteWrap
                | Self::AbsoluteExact
                | Self::AbsoluteDefined
                | Self::AbsoluteChecked
                | Self::NegateWrap
                | Self::NegateExact
                | Self::NegateDefined
                | Self::NegateChecked
                | Self::ByteSwap,
                CheckedType::GenericInt(_),
            ) => false,
            (_, CheckedType::Integer(_) | CheckedType::GenericInt(_)) => true,
            _ => false,
        }
    }

    pub(crate) const fn argument_type(
        self,
        operand: CheckedType,
        index: usize,
    ) -> Option<CheckedType> {
        if index >= self.operand_count() {
            return None;
        }
        if index == 1
            && matches!(
                self,
                Self::ShiftLeftWrap
                    | Self::ShiftRightWrap
                    | Self::ShiftLeftExact
                    | Self::ShiftRightExact
                    | Self::ShiftLeftDefined
                    | Self::ShiftRightDefined
                    | Self::RotateLeft
                    | Self::RotateRight
            )
        {
            Some(CheckedType::Integer(IntegerType::U32))
        } else {
            Some(operand)
        }
    }

    /// The error type of a checked row, or `None` for a row whose result is a
    /// scalar. Exactly the complement of [`Self::scalar_result_type`]'s `None`:
    /// a row either produces a scalar or produces `Result<T, E>`.
    pub(crate) const fn checked_error(self) -> Option<CheckedIntegerErrorClass> {
        match self {
            Self::AddChecked
            | Self::SubtractChecked
            | Self::MultiplyChecked
            | Self::AbsoluteChecked
            | Self::NegateChecked => Some(CheckedIntegerErrorClass::Overflow),
            Self::DivideChecked | Self::RemainderChecked => {
                Some(CheckedIntegerErrorClass::DivError)
            }
            _ => None,
        }
    }

    pub(crate) const fn scalar_result_type(self, operand: CheckedType) -> Option<CheckedType> {
        match self {
            Self::AddChecked
            | Self::SubtractChecked
            | Self::MultiplyChecked
            | Self::DivideChecked
            | Self::RemainderChecked
            | Self::AbsoluteChecked
            | Self::NegateChecked => None,
            Self::PopulationCount | Self::LeadingZeros | Self::TrailingZeros => {
                Some(CheckedType::Integer(IntegerType::U32))
            }
            Self::Equal
            | Self::NotEqual
            | Self::Less
            | Self::LessEqual
            | Self::Greater
            | Self::GreaterEqual
            | Self::AddDefined
            | Self::SubtractDefined
            | Self::MultiplyDefined
            | Self::DivideDefined
            | Self::RemainderDefined
            | Self::AbsoluteDefined
            | Self::NegateDefined
            | Self::ShiftLeftDefined
            | Self::ShiftRightDefined => Some(CheckedType::Bool),
            _ => Some(operand),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CheckedTargetDomainObligation {
    ElementAddress,
}

/// Target-independent upper bounds for one stored value's representation.
/// The backend must qualify its concrete layout against all three cells
/// before it may use the source-level `buffer_fits<T>` proof.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CheckedLayoutMagnitude {
    Finite(u64),
    AboveU64,
}

impl CheckedLayoutMagnitude {
    /// The exact largest element count admitted by OP-9 for this stride.
    /// Every stride represented by `AboveU64` is greater than U64_MAX, so
    /// only the zero-length allocation can fit its u64 byte-count domain.
    pub(crate) const fn allocation_limit(self) -> u64 {
        match self {
            Self::Finite(stride) => {
                assert!(stride >= 1, "a layout stride ceiling is always positive");
                u64::MAX / stride
            }
            Self::AboveU64 => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CheckedLayoutCeiling {
    pub(crate) size: CheckedLayoutMagnitude,
    pub(crate) align: u64,
    pub(crate) stride: CheckedLayoutMagnitude,
}

/// The static allocation-size obligation one call carries [OP-9].
///
/// [OP-13] attaches it to every runtime-capacity construction and [OP-10] to
/// `grow`, each "over that operation's own stored type and count". The
/// predicate is `n <= floor((2^64 - 1) / stride_ceiling(T))`, so the record
/// names the stored type, the language ceiling its stride fixes, and which
/// declared argument supplies the count `n`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CheckedAllocationFit {
    /// The allocating cell whose runtime-capacity content fixes the emitted
    /// block shape and header. Construction returns this cell; `grow`
    /// receives it as its first parameter.
    pub(crate) cell: CheckedType,
    /// The stored type T, after [FN-2] instantiation.
    pub(crate) element: CheckedType,
    /// [OP-9]'s language layout ceilings for that stored type.
    pub(crate) layout_ceiling: CheckedLayoutCeiling,
    /// The declared-order ordinal of the count argument.
    pub(crate) count: usize,
    /// Tightest numeric upper bound retained by this call's accepted OP-9
    /// derivation. Entailment installs it after proving the obligation;
    /// lowering must not proceed while it is absent.
    pub(crate) source_length_upper_bound: Option<u64>,
}

impl CheckedAllocationFit {
    pub(crate) fn install_source_length_upper_bound(&mut self, upper: u64) {
        self.source_length_upper_bound = Some(upper);
    }

    pub(crate) const fn source_length_upper_bound(self) -> Option<u64> {
        self.source_length_upper_bound
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CheckedArrayRoot {
    Binding {
        binding: BindingId,
        fields: Vec<u32>,
    },
    Constant(CheckedConstantId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedBufferRoot {
    pub(crate) binding: BindingId,
    /// The complete checked path from the binding to the run [REF-1]. A
    /// runtime-capacity `Array<T>` exists only as the content of a `Box`
    /// [TYPE-9], so the path of one reached from an owner carries that
    /// content step and [OWN-7] and [ENT-5] compare it like any other.
    pub(crate) path: Vec<CheckedPlaceStep>,
    pub(crate) element: CheckedElement,
    pub(crate) element_type: CheckedType,
}

impl CheckedBufferRoot {
    /// The [REF-1] resolved steps of this root.
    pub(crate) fn place_path(&self) -> Vec<super::places::PlaceStep> {
        self.path.iter().map(CheckedPlaceStep::place_step).collect()
    }
}

/// One range reference's own root [REF-4].
///
/// A range reference is the pointer-and-count pair the binding itself holds:
/// [TYPE-8] makes `&[T]` a reference kind and not a type, so no storage ever
/// holds one and no field path reaches one, and the root is that binding
/// alone. The element type travels beside it because [TYPE-7] makes the
/// referent a `deref` selects the element type, so [MSR-1]'s one `len` row
/// cannot be recovered from the selected type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedRangeRoot {
    pub(crate) binding: BindingId,
    /// The complete stored element type, interned in this checked program.
    pub(crate) element: CheckedElement,
    /// The same type carried directly for expression typing, which is
    /// context-free and cannot dereference the program-owned element table.
    pub(crate) element_type: CheckedType,
}

/// The storage one range reference is formed over [REF-4].
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CheckedRangeSource {
    /// One indexable owner place [OP-4]: a complete `Array` [TYPE-9] or a
    /// run's initialized window [WIN-1], addressed where it is stored.
    Storage(CheckedContainerRoot),
    /// Re-slicing another range reference, `&deref(part)[a..b]` [REF-4].
    Range(CheckedRangeRoot),
}

impl CheckedRangeSource {
    /// The written root and steps of the place this range is formed over
    /// [REF-1, REF-4], before reference resolution.
    ///
    /// A re-slice is formed over the run its holder names, so its steps are
    /// empty: resolving the holder supplies that run's own range step. The
    /// formed reference names each resolved place of this source extended by
    /// the formation's own range step, which is what [OWN-7] compares and
    /// what [EFF-5] substitutes into a callee's row.
    pub(crate) fn place(&self) -> (super::places::PlaceRoot, Vec<super::places::PlaceStep>) {
        match self {
            Self::Storage(root) => (root.root, root.place_path()),
            Self::Range(root) => (super::places::PlaceRoot::Binding(root.binding), Vec::new()),
        }
    }

    /// The binding the source is written at: the storage root's binding, or
    /// the holder a re-slice reads through.
    pub(crate) const fn binding(&self) -> Option<BindingId> {
        match self {
            Self::Storage(root) => root.binding(),
            Self::Range(root) => Some(root.binding),
        }
    }
}

/// One typed element place in the run a range reference names [REF-4, OP-4].
///
/// Reads, borrows, measures and `set` targets share the evaluated outer offset
/// and the typed suffix below that element. Keeping that complete path here
/// gives every storage judgment one place identity without choosing one of a
/// joined reference's possible origins [REF-1, ENT-5].
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedRangeElementPlace {
    pub(crate) root: CheckedRangeRoot,
    pub(crate) offset: CheckedExpression,
    /// The immutable image of `offset` at this occurrence [REF-1, OWN-7].
    pub(crate) captured: super::places::CapturedValue,
    /// The typed storage path below the selected element.
    pub(crate) path: Vec<CheckedPlaceStep>,
    /// The type selected by `path`, or the element type when it is empty.
    pub(crate) ty: CheckedType,
    pub(crate) obligation: NodePath,
    pub(crate) target_domain: CheckedTargetDomainObligation,
}

impl CheckedRangeElementPlace {
    /// The outer range offset followed by every nested subscript offset, in
    /// source evaluation order [SET-1, OP-4].
    pub(crate) fn offsets(&self) -> impl Iterator<Item = &CheckedExpression> {
        std::iter::once(&self.offset).chain(self.path.iter().filter_map(|step| match step {
            CheckedPlaceStep::Subscript(index) => Some(&index.offset),
            CheckedPlaceStep::Field(_) | CheckedPlaceStep::BoxReferent(_) => None,
        }))
    }

    pub(crate) fn offsets_mut(&mut self) -> impl Iterator<Item = &mut CheckedExpression> {
        std::iter::once(&mut self.offset).chain(self.path.iter_mut().filter_map(
            |step| match step {
                CheckedPlaceStep::Subscript(index) => Some(&mut index.offset),
                CheckedPlaceStep::Field(_) | CheckedPlaceStep::BoxReferent(_) => None,
            },
        ))
    }

    pub(crate) fn place_path(&self) -> Vec<super::places::PlaceStep> {
        std::iter::once(super::places::PlaceStep::Index(self.captured))
            .chain(self.path.iter().map(CheckedPlaceStep::place_step))
            .collect()
    }

    pub(crate) fn goal_projections(&self) -> Vec<super::goal::GoalProjection> {
        std::iter::once(super::goal::GoalProjection::Subscript(
            self.captured.goal_identity(),
        ))
        .chain(self.path.iter().map(CheckedPlaceStep::goal_projection))
        .collect()
    }

    pub(crate) const fn measured(&self) -> Option<MeasuredKind> {
        self.ty.measured()
    }

    /// [ENT-2] how this element place's offsets stand: the range's own
    /// subscript first, then every nested one.
    pub(crate) fn subscripted_term(&self) -> Option<SubscriptedTerm> {
        SubscriptedTerm::of_offsets(
            std::iter::once((&self.offset, self.captured)).chain(subscript_offsets(&self.path)),
        )
    }

    /// [ENT-2] clause (b): whether this element place is a term because its
    /// final step selects a readonly field of one fragment type. The range's
    /// own subscript selects the element every later step starts from.
    pub(crate) fn readonly_field_term(
        &self,
        nominals: &[CheckedNominal],
    ) -> Option<SubscriptedTerm> {
        if !selects_readonly_fragment_field(nominals, self.root.element_type, &self.path) {
            return None;
        }
        self.subscripted_term()
    }

    pub(crate) const fn element(&self) -> Option<CheckedElement> {
        match self.ty {
            CheckedType::Array { element, .. }
            | CheckedType::Window { element, .. }
            | CheckedType::Buffer { element } => Some(element),
            _ => None,
        }
    }

    pub(crate) const fn type_constant(&self) -> Option<CheckedConst> {
        match self.ty {
            CheckedType::Array { length, .. } => Some(length),
            CheckedType::Window { capacity, .. } => capacity,
            _ => None,
        }
    }
}

/// A typed storage place used by a borrow or a compiler-owned measure.
///
/// [MSR-2] makes a measure's support the resolved place of the measured value
/// itself, so the root retains every field and subscript that reaches it.
/// The type selects the addressed referent and, when measured, the table's
/// row. A `FixedVector` also carries its capacity constant here because that
/// constant is stored nowhere at run time.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedContainerRoot {
    pub(crate) root: super::places::PlaceRoot,
    /// The path below the root: field selections and subscripts, in written
    /// order [MSR-1]. `len_of(table[i])` is a term, so a measured place is
    /// not a field path.
    pub(crate) path: Vec<CheckedPlaceStep>,
    /// The type selected by the complete path.
    pub(crate) ty: CheckedType,
}

/// One step below a storage place's root [REF-1]: a field selection, one
/// `deref` of `Box` content [TYPE-7], or one [OP-4] subscript together with
/// the obligation that subscript owes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CheckedPlaceStep {
    Field(u32),
    /// One explicit dereference of `Box` content [TYPE-7]. It is an ordinary
    /// path step of the resolved place, so `deref(h).value` and
    /// `deref(h.value)` are two places [REF-1].
    BoxReferent(NominalId),
    Subscript(Box<CheckedPlaceSubscript>),
}

/// One subscript occurring inside a storage place [MSR-1, OP-4].
///
/// The offset is a logical one and its obligation is against the base's `len`
/// [WIN-1]; the storage it selects is slot `(r.head + i) mod r.cap`, which the
/// lowering computes and no rule of the overlap relation mentions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedPlaceSubscript {
    /// The base this subscript indexes. Its measure-table row is what the
    /// [OP-4] obligation `i < r.len` is stated against [MSR-1, WIN-1].
    pub(crate) base_type: CheckedType,
    /// The [TYPE-9] element type the subscript selects.
    pub(crate) element_type: CheckedType,
    pub(crate) offset: CheckedExpression,
    pub(crate) obligation: crate::NodePath,
    pub(crate) target_domain: CheckedTargetDomainObligation,
    /// [REF-1] the immutable value this index expression produced when the
    /// place was formed, which is what [OWN-7] and [ENT-5] read.
    pub(crate) captured: super::places::CapturedValue,
}

impl CheckedContainerRoot {
    pub(crate) const fn binding(&self) -> Option<BindingId> {
        match self.root {
            super::places::PlaceRoot::Binding(binding) => Some(binding),
            super::places::PlaceRoot::Constant(_) => None,
        }
    }

    /// The exact storage projection consumed by [OWN-7] and [ENT-5].
    pub(crate) fn place_path(&self) -> Vec<super::places::PlaceStep> {
        self.path.iter().map(CheckedPlaceStep::place_step).collect()
    }

    /// Offset evaluations are children of the place, including when its
    /// terminal operation only takes an address or reads a measure.
    pub(crate) fn offsets(&self) -> impl Iterator<Item = &CheckedExpression> {
        self.path.iter().filter_map(|step| match step {
            CheckedPlaceStep::Field(_) | CheckedPlaceStep::BoxReferent(_) => None,
            CheckedPlaceStep::Subscript(index) => Some(&index.offset),
        })
    }

    pub(crate) fn offsets_mut(&mut self) -> impl Iterator<Item = &mut CheckedExpression> {
        self.path.iter_mut().filter_map(|step| match step {
            CheckedPlaceStep::Field(_) | CheckedPlaceStep::BoxReferent(_) => None,
            CheckedPlaceStep::Subscript(index) => Some(&mut index.offset),
        })
    }

    /// The same path as [ENT-2] goal projections.
    pub(crate) fn goal_projections(&self) -> Vec<super::goal::GoalProjection> {
        self.path
            .iter()
            .map(|step| match step {
                CheckedPlaceStep::Field(field) => super::goal::GoalProjection::Field(*field),
                CheckedPlaceStep::BoxReferent(_) => super::goal::GoalProjection::Deref,
                CheckedPlaceStep::Subscript(subscript) => {
                    super::goal::GoalProjection::Subscript(subscript.captured.goal_identity())
                }
            })
            .collect()
    }

    /// The measure-table row this place selects [MSR-1].
    pub(crate) const fn measured(&self) -> Option<MeasuredKind> {
        self.ty.measured()
    }

    /// The element type of a storage shape.
    pub(crate) const fn element(&self) -> Option<CheckedElement> {
        match self.ty {
            CheckedType::Array { element, .. }
            | CheckedType::Window { element, .. }
            | CheckedType::Buffer { element } => Some(element),
            _ => None,
        }
    }

    /// The written capacity constant of a constant-capacity shape [TYPE-9];
    /// a runtime-capacity one stores its capacity and has none.
    pub(crate) const fn type_constant(&self) -> Option<CheckedConst> {
        match self.ty {
            CheckedType::Array { length, .. } => Some(length),
            CheckedType::Window { capacity, .. } => capacity,
            _ => None,
        }
    }

    /// [ENT-2] how this place's subscript offsets stand; a place with no
    /// subscript has none to judge.
    pub(crate) fn subscripted_term(&self) -> Option<SubscriptedTerm> {
        SubscriptedTerm::of_offsets(subscript_offsets(&self.path))
    }

    /// [ENT-2] clause (b): whether this subscripted place is a term because
    /// its final step selects a readonly field of one fragment type.
    ///
    /// Only the steps after the last subscript decide the final field; the
    /// offsets of every subscript decide whether its identity is represented.
    pub(crate) fn readonly_field_term(
        &self,
        nominals: &[CheckedNominal],
    ) -> Option<SubscriptedTerm> {
        let last = self
            .path
            .iter()
            .rposition(|step| matches!(step, CheckedPlaceStep::Subscript(_)))?;
        let CheckedPlaceStep::Subscript(index) = &self.path[last] else {
            return None;
        };
        if !selects_readonly_fragment_field(nominals, index.element_type, &self.path[last + 1..]) {
            return None;
        }
        self.subscripted_term()
    }
}

impl CheckedBufferRoot {
    /// [ENT-2] how this run's subscript offsets stand.
    pub(crate) fn subscripted_term(&self) -> Option<SubscriptedTerm> {
        SubscriptedTerm::of_offsets(subscript_offsets(&self.path))
    }
}

/// Every subscript offset of a checked storage path with its captured value,
/// in written order.
fn subscript_offsets(
    path: &[CheckedPlaceStep],
) -> impl Iterator<Item = (&CheckedExpression, super::places::CapturedValue)> {
    path.iter().filter_map(|step| match step {
        CheckedPlaceStep::Subscript(index) => Some((&index.offset, index.captured)),
        CheckedPlaceStep::Field(_) | CheckedPlaceStep::BoxReferent(_) => None,
    })
}

/// How one place's subscript offsets stand under [ENT-2]: every offset of a
/// term is itself a clause (a) or clause (c) term.
///
/// A place with an offset of any other form is no term at all and has no
/// value here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SubscriptedTerm {
    /// Every offset is a captured literal, const or binding [REF-1], so the
    /// place has the term identity the entailment fragment interns.
    Represented,
    /// An offset is a tracked place with projections, which ENT-2 admits but
    /// no captured value names. This is a compiler capability limit, reported
    /// as unsupported, not a language rejection [DIAG-1].
    Unrepresented,
}

impl SubscriptedTerm {
    /// Classifies the offsets of one place in written order.
    fn of_offsets<'offset>(
        offsets: impl IntoIterator<Item = (&'offset CheckedExpression, super::places::CapturedValue)>,
    ) -> Option<Self> {
        let mut term = Self::Represented;
        for (offset, captured) in offsets {
            if captured != super::places::CapturedValue::unknown() {
                continue;
            }
            if !is_tracked_place_read(offset) {
                return None;
            }
            term = Self::Unrepresented;
        }
        Some(term)
    }
}

/// Whether `steps`, read from a value of type `start`, end by selecting a
/// readonly field [TYPE-2] whose type is one fragment integer [ENT-2].
fn selects_readonly_fragment_field(
    nominals: &[CheckedNominal],
    start: CheckedType,
    steps: &[CheckedPlaceStep],
) -> bool {
    let struct_field = |ty: CheckedType, field: u32| {
        let CheckedType::Nominal(nominal) = ty else {
            return None;
        };
        let CheckedNominalKind::Struct { fields } = &nominals.get(nominal.0 as usize)?.kind else {
            return None;
        };
        fields.get(field as usize)
    };
    let Some((CheckedPlaceStep::Field(last), prefix)) = steps.split_last() else {
        return false;
    };
    let mut ty = start;
    for step in prefix {
        ty = match step {
            CheckedPlaceStep::Subscript(index) => index.element_type,
            CheckedPlaceStep::Field(field) => match struct_field(ty, *field) {
                Some(field) => field.ty,
                None => return false,
            },
            CheckedPlaceStep::BoxReferent(nominal) => {
                match nominals
                    .get(nominal.0 as usize)
                    .map(|nominal| &nominal.kind)
                {
                    Some(CheckedNominalKind::Box { referent, .. }) => *referent,
                    _ => return false,
                }
            }
        };
    }
    struct_field(ty, *last)
        .is_some_and(|field| field.readonly && matches!(field.ty, CheckedType::Integer(_)))
}

/// Whether one checked offset reads an [ENT-2] clause (a) tracked place:
/// a binding, possibly below field selections, `deref` wrappings and `Box`
/// content, with no subscript.
fn is_tracked_place_read(offset: &CheckedExpression) -> bool {
    match offset {
        CheckedExpression::Binding {
            consume_root: false,
            ..
        }
        | CheckedExpression::Project {
            consume_root: false,
            ..
        }
        | CheckedExpression::DerefAddressed { .. } => true,
        CheckedExpression::BoxDeref { value, .. }
        | CheckedExpression::ProjectValue { value, .. } => is_tracked_place_read(value),
        _ => false,
    }
}

impl CheckedPlaceStep {
    /// The [REF-1] path step this selection is.
    ///
    /// `deref` is a step of its own here, where v0.59 erased it: [REF-1]
    /// continues a path through `Box` content, and [OWN-7] reads the complete
    /// resolved path, so erasing the step would make `deref(h).value` and
    /// `h.value` one place.
    /// The [FN-9] clause-side projection this step is, for a goal place.
    pub(crate) fn goal_projection(&self) -> super::goal::GoalProjection {
        match self {
            Self::Field(field) => super::goal::GoalProjection::Field(*field),
            Self::BoxReferent(_) => super::goal::GoalProjection::Deref,
            Self::Subscript(index) => {
                super::goal::GoalProjection::Subscript(index.captured.goal_identity())
            }
        }
    }

    pub(crate) fn place_step(&self) -> super::places::PlaceStep {
        match self {
            Self::Field(field) => super::places::PlaceStep::Field(*field),
            Self::BoxReferent(_) => super::places::PlaceStep::Deref,
            Self::Subscript(index) => super::places::PlaceStep::Index(index.captured),
        }
    }
}

/// Source category retained only for integer-operation operands whose exact
/// written constant class affects an ENT-3 source. This is checked metadata,
/// not a second expression tree.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CheckedIntegerArgumentSource {
    TypedLiteral,
    GenericNumericIdentity,
    NamedConstant { declaration: DeclarationId },
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedIntegerArgument {
    pub(crate) node_path: NodePath,
    pub(crate) source: CheckedIntegerArgumentSource,
}

/// The conservative caller storage claim of a bound borrow-mode result:
/// the complete single provenance-candidate actual [OWN-6, ENT-5]. The
/// delivered referent may be a narrower suffix or unrelated immutable
/// constant. This record supports exclusion and kills, never value identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedResultBorrow {
    /// The sole provenance-candidate argument, in declared parameter order.
    /// Lowering retains this relation to the actual argument value rather
    /// than reconstructing an address from the resolved source path.
    pub(crate) argument: usize,
    pub(crate) root: super::places::PlaceRoot,
    pub(crate) path: Vec<super::places::PlaceStep>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CheckedExpression {
    Constant(CheckedValue),
    /// A named const read retains declaration identity for exact goal-origin
    /// equality while lowering the same immutable value as before.
    NamedConstant {
        declaration: DeclarationId,
        value: CheckedValue,
    },
    Binding {
        carrier: NodePath,
        binding: BindingId,
        ty: CheckedType,
        /// The owning checker admitted this occurrence as an affine consume.
        /// ENT keeps it beside the checked binding because holder mode is not
        /// recoverable from the value type and must not be re-read from syntax.
        consume_root: bool,
    },
    UserCall {
        function: FunctionId,
        /// A selected direct self transfer in the sole return position, either
        /// required by [FN-10] or inferred under the same conditions. This is
        /// lowering information, not a source marker; every proof judgment
        /// still sees the ordinary call.
        tail_transfer: bool,
        /// The authoritative function-formal row [FN-4, EFF-2], rebased to
        /// the selected concrete callee's parameter declarations. It is
        /// proof-only: lowering still calls `function` directly.
        formal_effects: Option<Box<CheckedEffects>>,
        /// [FN-4, FN-5] the exact instantiated formal contract and the
        /// retained implication queries authorizing execution/publication
        /// through the selected actual. The actual remains `function` for
        /// execution. Direct calls carry `None` and use that function's own
        /// contract and verified summaries.
        formal_contract: Option<Box<CheckedCallContract>>,
        /// Exact source call occurrence and declared-order argument atoms.
        call: NodePath,
        argument_nodes: Vec<NodePath>,
        arguments: Vec<CheckedExpression>,
        /// Immutable occurrence identities of the scalar actual values used
        /// to substitute indexed effect-row positions [EFF-5].
        actual_captures: Vec<super::places::CapturedValue>,
        /// Pre-transfer caller images retained for exact GoalTemplate
        /// substitution after the complete concrete function inventory exists.
        goal_arguments: Vec<super::goal::GoalExpression>,
        /// Concrete caller regions supplied for the callee's formal region
        /// parameters, in declaration order.
        goal_regions: Vec<DeclarationId>,
        /// Filled from the complete phase-A inventory before entailment runs,
        /// in callee `requires_clause` source order.
        requirements: Vec<super::goal::CheckedCallRequirement>,
        result: CheckedType,
        /// For a borrow-mode result admitted under the reborrow extension:
        /// the caller-side storage the result borrow is conservatively rooted
        /// at — the resolved place of the callee signature's single
        /// provenance-candidate actual. Entailment reads it so a write
        /// through the bound result kills exactly the facts on that storage
        /// [ENT-5]; it is `None` for every own-mode result and whenever the
        /// extension is off.
        result_borrow: Option<CheckedResultBorrow>,
        /// [OP-9, OP-13, OP-10] the static allocation-size obligation this
        /// call carries, where it is a runtime-capacity construction or
        /// `grow`. Every other call carries none.
        allocation: Option<CheckedAllocationFit>,
    },
    IntegerOperation {
        carrier: NodePath,
        operation: CheckedIntegerOperation,
        operand_type: CheckedType,
        argument_metadata: Vec<CheckedIntegerArgument>,
        arguments: Vec<CheckedExpression>,
        result: CheckedType,
    },
    FloatOperation {
        carrier: NodePath,
        operation: CheckedFloatOperation,
        operand_type: CheckedType,
        arguments: Vec<CheckedExpression>,
    },
    NumericConversion {
        carrier: NodePath,
        mode: CheckedConversionMode,
        source: CheckedNumericType,
        destination: CheckedNumericType,
        value: Box<CheckedExpression>,
        result: CheckedType,
    },
    Reinterpret {
        carrier: NodePath,
        source: CheckedNumericType,
        destination: CheckedNumericType,
        value: Box<CheckedExpression>,
    },
    BooleanOperation {
        carrier: NodePath,
        operation: CheckedBooleanOperation,
        arguments: Vec<CheckedExpression>,
    },
    EnumEquality {
        carrier: NodePath,
        equal: bool,
        operand_type: CheckedType,
        arguments: Vec<CheckedExpression>,
    },
    ArrayMeasure {
        measure: CheckedMeasure,
        root: CheckedArrayRoot,
        length: CheckedConst,
    },
    ArrayIndex {
        carrier: NodePath,
        root: CheckedArrayRoot,
        element_type: CheckedType,
        length: CheckedConst,
        offset: Box<CheckedExpression>,
        obligation: NodePath,
        target_domain: CheckedTargetDomainObligation,
    },
    /// [TYPE-9, WIN-3] move an owned path through Box content. The whole root
    /// is consumed; the selected value survives while the ordered cleanup
    /// releases its residual parts and enclosing cells.
    BoxTake {
        carrier: NodePath,
        referent: CheckedType,
        binding: BindingId,
        path: Vec<CheckedPlaceStep>,
        cleanup: Vec<CheckedOwnedTakeCleanup>,
    },
    BufferMeasure {
        measure: CheckedMeasure,
        root: CheckedBufferRoot,
    },
    /// [REF-4] `&x[lo..hi]`: one range reference over an indexable place or
    /// over another range reference, under the obligations `lo <= hi` and
    /// `hi <= x.len` submitted at `obligation`.
    ///
    /// The expression's own type is the element type, exactly as a `&[T]`
    /// parameter's is: [TYPE-8] makes the range kind a mode and not a type,
    /// so the value's kind travels in [`CheckedMode::Range`] beside it.
    RangeOf {
        carrier: NodePath,
        source: CheckedRangeSource,
        element: CheckedElement,
        element_type: CheckedType,
        start: Box<CheckedExpression>,
        end: Box<CheckedExpression>,
        obligation: NodePath,
        /// [REF-1, OWN-7] the two immutable endpoint values this formation
        /// captured.
        ///
        /// The range step a formed reference's path carries holds exactly
        /// this pair, so the separation a call submits over two such steps
        /// finds each formation's endpoints by the occurrence that evaluated
        /// them and never by the spelling written at the argument.
        captured: super::places::CapturedRange,
    },
    /// [MSR-1] the one measure a range reference has, its element count.
    RangeMeasure {
        measure: CheckedMeasure,
        root: CheckedRangeRoot,
    },
    /// [MSR-1] one measure of a typed element place selected through a range
    /// reference. Lowering addresses the element and reads the ordinary
    /// descriptor cell; the range holder remains the proof-term root so a
    /// joined reference never selects an arbitrary possible origin [ENT-5].
    RangeElementMeasure {
        carrier: NodePath,
        measure: CheckedMeasure,
        place: Box<CheckedRangeElementPlace>,
    },
    /// [OP-4] one discharged subscript read of the run a range names.
    RangeIndex {
        carrier: NodePath,
        place: Box<CheckedRangeElementPlace>,
    },
    /// [REF-1, REF-4] a reference to one discharged element place in the run
    /// a range reference names. Unlike `RangeIndex`, this preserves the
    /// selected address instead of reading the stored value.
    BorrowRangeIndex {
        carrier: NodePath,
        place: Box<CheckedRangeElementPlace>,
    },
    /// One [MSR-1] measure of a storage shape [TYPE-9], read as its [OP-1]
    /// reader row. One quantity, one name, term and reader alike.
    ContainerMeasure {
        measure: CheckedMeasure,
        root: CheckedContainerRoot,
    },
    /// One discharged source subscript read of a run [OP-4, WIN-1].
    ///
    /// The offset is a logical one and its obligation is against `len`; the
    /// storage it selects is slot `(head + i) mod cap`, which the lowering
    /// computes and no source rule mentions.
    ReadStorage {
        carrier: NodePath,
        root: CheckedContainerRoot,
    },
    BufferIndex {
        carrier: NodePath,
        root: CheckedBufferRoot,
        offset: Box<CheckedExpression>,
        obligation: NodePath,
        target_domain: CheckedTargetDomainObligation,
    },
    BoxDeref {
        carrier: NodePath,
        nominal: NominalId,
        referent: CheckedType,
        value: Box<CheckedExpression>,
    },
    /// A borrow of directly stored content, addressed by its complete typed
    /// field/subscript path [OWN-2, OWN-5, OP-4].
    BorrowAddressed {
        carrier: NodePath,
        root: CheckedContainerRoot,
    },
    /// The referent value read through such a holder [TYPE-7]. The holder
    /// itself stays a distinct expression, so lowering never has to guess
    /// whether a borrow binding is being passed on or read through.
    DerefAddressed {
        carrier: NodePath,
        binding: BindingId,
        ty: CheckedType,
    },
    ConstructStruct {
        carrier: NodePath,
        nominal: NominalId,
        fields: Vec<CheckedExpression>,
    },
    ConstructEnum {
        carrier: NodePath,
        nominal: NominalId,
        variant: u32,
        fields: Vec<CheckedExpression>,
    },
    Project {
        carrier: NodePath,
        binding: BindingId,
        fields: Vec<u32>,
        ty: CheckedType,
        consume_root: bool,
        residual_drops: Vec<CheckedProjectedDrop>,
    },
    ProjectValue {
        carrier: NodePath,
        value: Box<CheckedExpression>,
        nominal: NominalId,
        field: u32,
        ty: CheckedType,
    },
}

impl CheckedExpression {
    /// Exact PRV-1 carrier node for a positive explicit-dataflow edge.
    pub(crate) const fn carrier(&self) -> Option<&NodePath> {
        match self {
            Self::Constant(_)
            | Self::NamedConstant { .. }
            | Self::ArrayMeasure { .. }
            | Self::BufferMeasure { .. }
            | Self::ContainerMeasure { .. }
            | Self::RangeMeasure { .. } => None,
            Self::UserCall { call, .. } => Some(call),
            Self::Binding { carrier, .. }
            | Self::IntegerOperation { carrier, .. }
            | Self::FloatOperation { carrier, .. }
            | Self::NumericConversion { carrier, .. }
            | Self::Reinterpret { carrier, .. }
            | Self::BooleanOperation { carrier, .. }
            | Self::EnumEquality { carrier, .. }
            | Self::ArrayIndex { carrier, .. }
            | Self::BufferIndex { carrier, .. }
            | Self::RangeOf { carrier, .. }
            | Self::RangeElementMeasure { carrier, .. }
            | Self::RangeIndex { carrier, .. }
            | Self::BorrowRangeIndex { carrier, .. }
            | Self::ReadStorage { carrier, .. }
            | Self::BoxDeref { carrier, .. }
            | Self::BoxTake { carrier, .. }
            | Self::BorrowAddressed { carrier, .. }
            | Self::DerefAddressed { carrier, .. }
            | Self::ConstructStruct { carrier, .. }
            | Self::ConstructEnum { carrier, .. }
            | Self::Project { carrier, .. }
            | Self::ProjectValue { carrier, .. } => Some(carrier),
        }
    }

    pub(crate) const fn ty(&self) -> CheckedType {
        match self {
            Self::Constant(value) => value.ty(),
            Self::NamedConstant { value, .. } => value.ty(),
            Self::Binding { ty, .. } | Self::UserCall { result: ty, .. } => *ty,
            Self::IntegerOperation { result, .. } | Self::NumericConversion { result, .. } => {
                *result
            }
            Self::Reinterpret { destination, .. } => destination.ty(),
            Self::FloatOperation {
                operation,
                operand_type,
                ..
            } => operation.result_type(*operand_type),
            Self::BooleanOperation { .. } | Self::EnumEquality { .. } => CheckedType::Bool,
            Self::ArrayMeasure { .. } => CheckedType::Integer(IntegerType::U64),
            Self::ArrayIndex { element_type, .. } => *element_type,
            Self::BufferMeasure { .. }
            | Self::ContainerMeasure { .. }
            | Self::RangeMeasure { .. }
            | Self::RangeElementMeasure { .. } => CheckedType::Integer(IntegerType::U64),
            Self::BufferIndex { root, .. } => root.element_type,
            // [TYPE-8] `&[T]` is a reference kind, not a type: the value's
            // own type is the element type and its kind is its mode, exactly
            // as a `&[T]` parameter carries them [GRAM-2, REF-4].
            Self::RangeOf { element_type, .. } => *element_type,
            Self::RangeIndex { place, .. } | Self::BorrowRangeIndex { place, .. } => place.ty,
            Self::ReadStorage { root, .. } => root.ty,
            Self::BoxDeref { referent, .. } | Self::BoxTake { referent, .. } => *referent,
            Self::BorrowAddressed { root, .. } => root.ty,
            Self::DerefAddressed { ty, .. } => *ty,
            Self::ConstructStruct { nominal, .. } | Self::ConstructEnum { nominal, .. } => {
                CheckedType::Nominal(*nominal)
            }
            Self::Project { ty, .. } | Self::ProjectValue { ty, .. } => *ty,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CheckedEnumType {
    Bool,
    Nominal(NominalId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedMatchBinder {
    pub(crate) node_path: NodePath,
    pub(crate) binding: BindingId,
    pub(crate) field: u32,
    pub(crate) mode: CheckedMode,
    pub(crate) ty: CheckedType,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedMatchArm {
    pub(crate) tag: u32,
    pub(crate) binders: Vec<CheckedMatchBinder>,
    /// [GRAM-10, WIN-3, STOR-3] in an own-place match, the release of each
    /// payload field a final `..` covers, taken on entry to the arm: one
    /// whole-field drop, its path the field's ordinal, for every covered
    /// field whose release is non-empty. A reference match releases nothing.
    pub(crate) covered: Vec<CheckedProjectedDrop>,
    pub(crate) body: Vec<CheckedStatement>,
    pub(crate) fallthrough_drops: Vec<CheckedDrop>,
}

/// One compiler-derived release on a normal control-flow edge [STOR-3].
///
/// The record is explicit in the checked program [DIAG-2] rather than being
/// rederived from the type by every consumer. Lowering carries the record
/// into typed IR to emit the ordinary storage release [STOR-3]. Opaque
/// nominals have the empty release.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedDrop {
    pub(crate) binding: BindingId,
    pub(crate) fields: Vec<u32>,
    pub(crate) ty: CheckedType,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedProjectedDrop {
    pub(crate) fields: Vec<u32>,
    pub(crate) ty: CheckedType,
}

/// One action that remains after an owned sub-place has been taken [WIN-3].
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CheckedOwnedTakeCleanup {
    Drop {
        path: Vec<CheckedPlaceStep>,
        ty: CheckedType,
    },
    /// Release only the traversed cell. Its content has already been split
    /// between the selected value and the preceding residual actions.
    BoxShell {
        path: Vec<CheckedPlaceStep>,
        nominal: NominalId,
        referent: CheckedType,
    },
}

/// A SET-1 target whose root, path, copy type, and post-RHS writability have
/// all been established by semantic checking.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedWritablePlace {
    pub(crate) binding: BindingId,
    pub(crate) fields: Vec<u32>,
    /// The binding's source mode. Reference rebinding replaces the runtime
    /// address or range descriptor carried by the name; it does not write the
    /// storage that address names.
    pub(crate) mode: CheckedMode,
    pub(crate) ty: CheckedType,
    /// [SET-1] this commit declares the binding it writes, exactly as a `let`
    /// does: the target identifier resolved to none, so the statement is the
    /// binding's own initialization and nothing before it holds its storage.
    pub(crate) declares: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CheckedSetTarget {
    Place(CheckedWritablePlace),
    /// One element position of the run a range reference names [REF-4].
    RangeIndex(Box<CheckedRangeElementPlace>),
    /// A typed storage path including all subscripts and terminal fields.
    Storage(CheckedContainerRoot),
}

impl CheckedSetTarget {
    pub(crate) fn binding(&self) -> BindingId {
        match self {
            Self::Place(target) => target.binding,
            Self::RangeIndex(target) => target.root.binding,
            Self::Storage(target) => target
                .binding()
                .expect("checked mutation targets have local roots"),
        }
    }

    pub(crate) fn ty(&self) -> CheckedType {
        match self {
            Self::Place(target) => target.ty,
            Self::RangeIndex(target) => target.ty,
            Self::Storage(target) => target.ty,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PropagationContext {
    pub(crate) function: String,
    pub(crate) node_path: NodePath,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CheckedStatement {
    Let {
        node_path: NodePath,
        binding: BindingId,
        value: CheckedExpression,
    },
    /// [GRAM-4, CALL-4] `let (a, b) = f(...);`. One evaluation of a call whose
    /// callee declares an ordered result list, or the source consumes a
    /// struct or cell. Binders follow field order for products; a cell's
    /// sole binder receives its referent.
    DestructuringLet {
        node_path: NodePath,
        /// Each binder, its selected value type, and the field or result
        /// ordinal it receives, in written order. [GRAM-4]'s rest marker
        /// leaves the ordinals it covers unbound, so the binders are not
        /// always the leading ordinals.
        bindings: Vec<(BindingId, CheckedType, u32)>,
        /// [WIN-3, STOR-3] the compiler-derived release of every field a
        /// final `..` covers, in declaration order, rooted at the consumed
        /// value. A result list covers nothing and carries none.
        covered: Vec<CheckedProjectedDrop>,
        /// The consumed product or cell nominal [CALL-4, TYPE-6, S39].
        nominal: NominalId,
        value: CheckedExpression,
    },
    PropagateLet {
        /// Complete owning `let_stmt`, shared by Ok delivery and Err return.
        node_path: NodePath,
        binding: BindingId,
        scrutinee: CheckedExpression,
        result_nominal: NominalId,
        return_nominal: NominalId,
        ok_type: CheckedType,
        error_type: CheckedType,
        error_drops: Vec<CheckedDrop>,
        context: PropagationContext,
    },
    Set {
        node_path: NodePath,
        target: CheckedSetTarget,
        value: CheckedExpression,
        /// [SET-1, OP-12, WIN-3] the checked post-RHS disposition, for every
        /// target shape. A read-out or revived binding displaces no old value;
        /// a reference rebinding has no owned value to release either.
        displaces_live_value: bool,
    },
    /// [GRAM-4] an expression statement whose discarded result needs no
    /// release: a copy value or a borrow-mode reference.
    Evaluate {
        /// The complete `expr_stmt`, the statement's own site for the
        /// [PAR-1, PAR-2] footprint judgments, as a `let`'s is.
        node_path: NodePath,
        value: CheckedExpression,
    },
    /// The discarded result of an expression statement, with the
    /// compiler-derived release it runs [STOR-3].
    DropExpression {
        /// The complete `expr_stmt`, as for [`Self::Evaluate`].
        node_path: NodePath,
        value: CheckedExpression,
        drops: Vec<CheckedProjectedDrop>,
    },
    /// A finite source-written local invariant checked before it is published
    /// and erased before lowering. It has no runtime expression, effect,
    /// branch, or trap.
    Proof(CheckedSourceProof),
    Return {
        node_path: NodePath,
        value: CheckedExpression,
        drops: Vec<CheckedDrop>,
    },
    Match {
        scrutinee: CheckedExpression,
        enum_type: CheckedEnumType,
        arms: Vec<CheckedMatchArm>,
        continues: bool,
    },
    ValueMatchLet {
        node_path: NodePath,
        kind: ValueInitializerKind,
        binding: BindingId,
        result_type: CheckedType,
        /// [REF-1, GIVE-1] the binder's derived mode. A binder every arm of
        /// which delivers a reference *is* a reference variable, so what the
        /// join carries is the address its arms delivered and not a value of
        /// the referent type; that distinction is not recoverable from
        /// `result_type`, which is the referent's [TYPE-8].
        result_mode: CheckedMode,
        /// A range result's complete element handle. [TYPE-8] keeps the
        /// written element type in `result_type`, but the pointer-and-count
        /// lowering needs the interned element identity just as a range
        /// parameter does; no other result mode carries one.
        result_range_element: Option<CheckedElement>,
        scrutinee: CheckedExpression,
        enum_type: CheckedEnumType,
        arms: Vec<CheckedMatchArm>,
        continues: bool,
    },
    Give {
        node_path: NodePath,
        value: CheckedExpression,
        drops: Vec<CheckedDrop>,
    },
    Loop {
        id: CheckedLoopId,
        /// Formed source invariants awaiting the normal semantic proof
        /// checker. Their presence alone grants no authority.
        invariants: Vec<CheckedLoopInvariant>,
        body: Vec<CheckedStatement>,
        backedge_drops: Vec<CheckedDrop>,
    },
    CountedRange {
        id: CheckedLoopId,
        node_path: NodePath,
        binder: BindingId,
        lower: CheckedExpression,
        upper: Box<CheckedExpression>,
        /// Formed source invariants awaiting the normal semantic proof
        /// checker. Their presence alone grants no authority.
        invariants: Vec<CheckedLoopInvariant>,
        body: Vec<CheckedStatement>,
        backedge_drops: Vec<CheckedDrop>,
    },
    Break {
        target: CheckedLoopId,
        drops: Vec<CheckedDrop>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedParameter {
    pub(crate) name: String,
    pub(crate) declaration: DeclarationId,
    /// The complete source `param` node used by checked diagnostics.
    pub(crate) node_path: NodePath,
    pub(crate) binding: BindingId,
    pub(crate) mode: CheckedMode,
    pub(crate) ty: CheckedType,
    /// The complete element handle of a range parameter; absent for every
    /// ordinary owned or reference parameter.
    pub(crate) range_element: Option<CheckedElement>,
}

/// One callable-boundary state identity: the formal path one `reads(...)` or
/// `writes(...)` entry names [FN-1, EFF-1].
///
/// `root` is the reference value-parameter declaration the path is rooted at.
/// Every `effect_path` is rooted at one reference parameter of the same
/// callable; a root resolving to a local, a result binder, a by-value
/// parameter, or a non-parameter declaration is an EFF-1 rejection, and a
/// by-value parameter has no effect entry at all.
///
/// `steps` is the complete `epsuffix*` below that root, and it is complete on
/// purpose: [EFF-5] substitutes this path and [OWN-7] then compares the
/// result, so a step list that dropped what it could not represent would
/// under-approximate a write and let EFF-5 admit an overlapping pair it has
/// to refuse.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct CheckedStatePath {
    pub(crate) root: DeclarationId,
    pub(crate) steps: Vec<CheckedEffectStep>,
}

/// One `epsuffix`, or one `deref`, of a declared effect path [EFF-1].
///
/// This is the formal twin of [`super::places::PlaceStep`]: the same five
/// storage selectors plus the two name families [TYPE-10] admits in a row,
/// with the index and endpoint positions still unsubstituted. A signature
/// never contains an index expression — an index enters an effect only
/// through an IDENT that resolves to a value parameter of the same callable
/// — so an index position here is that parameter, and [EFF-5] replaces it
/// with the value its own argument supplies, evaluated once at the call.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum CheckedEffectStep {
    /// `.IDENT` selecting a struct field, by source ordinal.
    Field(u32),
    /// `deref(path)`: `Box` content [TYPE-7].
    Deref,
    /// `.TYPEID.IDENT`: one enum payload step, by variant and field ordinal.
    Payload { variant: u32, field: u32 },
    /// `[IDENT]`: one whole-index position naming a value parameter.
    Index(DeclarationId),
    /// `[IDENT..IDENT]`: one range position [REF-4], both endpoints naming
    /// value parameters.
    Range {
        start: DeclarationId,
        end: DeclarationId,
    },
    /// `.next`, `.last`, `.filled`, or `.free` [WIN-2, TYPE-10].
    Part(super::places::WindowPart),
    /// `.len`, `.cap`, or `.head` [MSR-1, OP-15, TYPE-2].
    Measure(CheckedMeasure),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedFunction {
    /// A function-kind hypothesis exists only during symbolic template
    /// checking. The concrete inventory and lowering contain none.
    pub(crate) formal_hypothesis: bool,
    pub(crate) id: FunctionId,
    pub(crate) declaration: DeclarationId,
    /// The module whose inventory declares it; the synthetic root module for
    /// a PRE-1 function [MOD-3].
    pub(crate) module: crate::ModuleId,
    pub(crate) name: String,
    pub(crate) symbol: String,
    /// The concrete function-kind actuals this instance was built with, in
    /// binding order; empty for a function without function-kind parameters.
    /// [FN-9] forms a caller's component as though an instance of another
    /// module's generic callable calls every one of them.
    pub(crate) function_actuals: Vec<FunctionId>,
    /// Formal regions in the same declaration order `UserCall::goal_regions`
    /// uses. Retained for post-acceptance physical release specialization;
    /// semantic identity remains the canonical [`FunctionId`].
    pub(crate) region_parameters: Vec<DeclarationId>,
    pub(crate) parameters: Vec<CheckedParameter>,
    pub(crate) result_mode: CheckedMode,
    pub(crate) result: CheckedType,
    /// Formal state paths named by `writes(...)`.
    pub(crate) declared_state_writes: Vec<CheckedStatePath>,
    /// Callable-boundary predicates in `requires_clause` source order.
    pub(crate) requirements: Vec<super::goal::CheckedRequirement>,
    /// [ENT-2, FN-8] the clause (b) places each requirement forms, index-
    /// aligned with `requirements`: its own, and those of every definition
    /// whose expansion it is the first requirement to reach. Each is formed
    /// at body entry in the state holding the requirements before it, where
    /// its subscripts owe [OP-4]. A hypothetical premise set forms none.
    pub(crate) requirement_places: Vec<Vec<CheckedExpression>>,
    /// Verified-relation surfaces in `ensures_clause` source order. H1
    /// constructs this metadata; the shared entailment flow proves every
    /// clause at every selected exit.
    pub(crate) postconditions: Vec<super::postcondition::CheckedPostcondition>,
    pub(crate) body: Option<Vec<CheckedStatement>>,
    /// Function-wide union of the resolved paths each reference holder names
    /// during the final structural walk, indexed by `BindingId`. Roots are
    /// owned bindings, constants or immutable incoming-reference anchors,
    /// never mutable reference-holder links to expand again. This inventory is
    /// not authority for the holder's target at any particular program point.
    pub(crate) reference_origins: Vec<Vec<super::places::ResolvedPlace>>,
    /// Whether the independently established body-entry requirements close to
    /// a contradiction. The contradiction is retained proof metadata.
    pub(crate) body_disposition: CheckedBodyDisposition,
    /// [EFF-3] whether this function allocates, which is the fact the
    /// deduplication and reordering licence reads. See [`CheckedEffects`].
    pub(crate) allocates: bool,
    /// [EFF-5] the pairs of substituted paths at this function's calls that
    /// the pairwise comparison could not separate by syntax alone, each
    /// submitted to the entailment fragment where the call is walked.
    pub(crate) call_separations: Vec<CheckedCallSeparation>,
    /// Finite optional [PAR-1] range questions planned from the complete
    /// structural footprints before entailment walks their first statements.
    pub(crate) permission_separation_queries: Vec<super::permission::PermissionSeparationQuery>,
    /// Every mandatory obligation of the completed function, which the
    /// analysis answers one by one and acceptance requires discharged.
    /// Formed once the call requirements are installed; empty before.
    pub(crate) obligations: Vec<super::obligations::ObligationRecord>,
    /// Retained [ENT] analysis summary [DIAG-2]. Semantic acceptance and
    /// diagnostics read it; lowering deliberately does not.
    #[allow(dead_code)]
    pub(crate) entailment: super::entailment::FunctionEntailment,
}

/// One [OWN-7] separation question the checker could not settle by syntax.
///
/// A mandatory call-effect pair [EFF-5] or a write's preservation of a later
/// reference use [REF-2] requires the positions to be proved distinct.
/// The checker holds the actual argument spellings and the live
/// reference state, so it owns the comparison; what it cannot do is discharge
/// the index or range goal, which is the fixed [ENT-6] families' work under
/// [MSR-4]'s disposition. This record is that handover, and the diagnostic it
/// carries cites EFF-5 or OP-11 at the call, or REF-2 at the reference use.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedCallSeparation {
    /// The call or set commit whose pre-write proof context answers the query.
    pub(crate) site: NodePath,
    /// Exchange's possible ancestry is refused by OP-11, rather than EFF-5.
    pub(crate) exchange: bool,
    /// A demanded REF-2 use; the separation is still proved at `site`, before
    /// the invalidating write, and diagnosed at this later use.
    pub(crate) reference_use: Option<CheckedReferencePreservationUse>,
    pub(crate) positions: Vec<CheckedCallSeparationPositions>,
    /// The window a position beside one of its parts reads `r.len` of
    /// [WIN-2]: the place both paths reach above the divergence.
    pub(crate) window: Option<super::places::ResolvedPlace>,
    /// The two substituted paths as the diagnostic renders them.
    pub(crate) left_spelling: String,
    pub(crate) right_spelling: String,
    /// Whether one reference argument supplies both paths [EFF-5], so that
    /// passing only one of them is no repair [DIAG-1].
    pub(crate) one_argument: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedReferencePreservationUse {
    pub(crate) site: NodePath,
    pub(crate) binder: String,
    pub(crate) event: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CheckedCallSeparationPositions {
    Indices(super::places::CapturedValue, super::places::CapturedValue),
    Ranges(super::places::CapturedRange, super::places::CapturedRange),
    /// [WIN-2] an index beside the `next` or `free` part of the window it
    /// indexes, which the pair's separation needs proved below that window's
    /// length in the call's entry state.
    Live(super::places::CapturedValue),
    /// [OWN-7] an index beside a range under containing paths that are
    /// identical step for step or differ only in index steps, which the
    /// separation needs proved before the range's start or at or after its
    /// end, or the range empty.
    IndexOutsideRange(super::places::CapturedValue, super::places::CapturedRange),
    /// [WIN-2] a range beside the window's `next` or `free`, which the
    /// separation needs proved to end at or below `r.len`, or empty.
    RangeWithinLength(super::places::CapturedRange),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum CheckedBodyDisposition {
    #[default]
    Inhabited,
    Uninhabited {
        contradiction: super::entailment::DerivationId,
    },
}

/// The one source-canonical symbolic requirement retained for a generic
/// function template.
///
/// This is acceptance metadata only. It deliberately has no [`FunctionId`]
/// and ordinary lowering must not treat it as an executable instance.
#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedGenericRequirement {
    pub(crate) declaration: DeclarationId,
    pub(crate) requirement: super::goal::CheckedRequirement,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedEffects {
    pub(crate) reads: Vec<CheckedStatePath>,
    pub(crate) writes: Vec<CheckedStatePath>,
    /// [EFF-3] whether this boundary allocates.
    ///
    /// [STOR-8] gives allocation no effect entry, so [EFF-1]'s row cannot
    /// carry it, while [EFF-3] still excepts an allocating call from
    /// deduplication and reordering: the heap a call takes from is finite and
    /// a duplicated take is a different program. The fact is therefore
    /// checked-program metadata rather than a row category, which is also
    /// what [DIAG-2] requires — lowering acts on what the checked program
    /// states and never rederives a program property of its own.
    pub(crate) allocates: bool,
}

#[derive(Debug)]
pub(crate) struct CheckedProgramData {
    pub(crate) nominals: Vec<CheckedNominal>,
    /// [BLK-4, DIAG-2] retained confinement for each complete nominal,
    /// including phantom brands. Structural types carry their region and
    /// element handles directly, so together these retain every value's set.
    #[allow(dead_code)]
    pub(crate) nominal_confinement: Vec<Vec<DeclarationId>>,
    /// Append-only structural elements, including unreachable replay history.
    /// Only handles reachable from executable types belong to lowering.
    pub(crate) elements: Vec<CheckedType>,
    // Nominal instances discovered by the ordinary function path form this
    // prefix. Later instances exist only to type-check static metadata.
    pub(crate) executable_nominal_count: usize,
    /// For each nominal, the instance it lowers as: itself, or the first
    /// instance of the same region-erased source family whose complete
    /// reclamation graph agrees [S20, PROV-1].
    ///
    /// A region is a proof-time identity. Two instances at two regions are two
    /// checked types — that is what makes a run of one store unusable at
    /// another. They share one runtime representation only when their store
    /// release classes agree at every owning position. Physical lowering may
    /// specialize the declaration-level classes in this default alias table.
    pub(crate) nominal_lowering_alias: Vec<NominalId>,
    /// Region-erased nominal families before a store-backed Box or Vector's
    /// release class is selected. Post-acceptance physical specialization
    /// combines this identity with its closed release environment; it never
    /// participates in source type equality or acceptance.
    pub(crate) nominal_physical_alias: Vec<NominalId>,
    /// Declaration-selected storage release classes for post-acceptance
    /// physical function specialization. Loan regions do not become
    /// specialization axes merely by occurring in this table.
    pub(crate) constants: Vec<CheckedConstant>,
    /// [MOD-8] each nominal's stable spelling, by module-qualified
    /// declaration names and arguments, when it has one; lowering names its
    /// link-visible type by it, so a type keeps its name in every build that
    /// has it, whatever other types that build has.
    pub(crate) nominal_spellings: Vec<Option<String>>,
    /// Each constant's module-qualified name, for the same purpose.
    pub(crate) constant_spellings: Vec<String>,
    /// Immutable structural table for every symbolic const expression named
    /// by retained schema metadata. `DerivedConstId` is meaningful only
    /// relative to this checked-program-owned table.
    #[allow(dead_code)]
    pub(crate) derived_consts: Vec<DerivedConst>,
    pub(crate) functions: Vec<CheckedFunction>,
    /// Each successful FN-4 implication, in its own declaration-only proof
    /// namespace. Lowering reads no contract query; this is retained DIAG-2
    /// evidence for the binding decision.
    #[allow(dead_code)]
    pub(crate) contract_queries: Vec<CheckedContractQuery>,
    /// Concrete ordinary-call SCCs in deterministic callee-before-caller
    /// order, with component-atomic verified FN-9 summary publication.
    #[allow(dead_code)]
    pub(crate) postcondition_schedule: super::entailment::PostconditionSchedule,
    /// One symbolic requirement per source generic that declares one. These
    /// entries survive symbolic validation without entering the concrete
    /// function inventory or executable lowering path.
    #[allow(dead_code)]
    pub(crate) generic_requirements: Vec<CheckedGenericRequirement>,
    /// Read-only [PAR-1 candidate] permission table: which sibling call pairs
    /// may be overlapped, and which of those are actualizable. Acceptance
    /// never reads it, and it is identical facts-on and facts-off. The
    /// permission ledger and the overlap lowering are its consumers.
    pub(crate) permission: super::permission::PermissionMetadata,
    /// The rendered non-normative permission ledger, one line per analyzed
    /// source site in source order, each marked with whether an ordinary
    /// compile reports it. This is developer output only: the driver hands the
    /// whole report to `whitefootc --par-ledger` and the marked subset to every
    /// compile, and no mandatory record, no normative output, and no lowering
    /// decision reads it.
    pub(crate) permission_ledger: Vec<super::permission_ledger::LedgerLine>,
}

/// One accepted function-kind contract implication. Clause paths are source
/// identities; the proof carries its own dense term, goal and DAG namespace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedContractQuery {
    /// Concrete containing function instance whose bound call uses this
    /// implication. Declaration-only FN-4 validation carries `None`; only
    /// `Some` identities are referenced by checked calls.
    pub(crate) instance: Option<FunctionId>,
    pub(crate) site: NodePath,
    pub(crate) premises: Vec<NodePath>,
    pub(crate) goal: NodePath,
    pub(crate) proof: super::entailment::FunctionEntailment,
}

/// The authoritative contract surface of one bound call [FN-5].
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedCallContract {
    pub(crate) requirements: Vec<super::goal::CheckedRequirement>,
    /// One accepted formal-requires => actual-requires implication for every
    /// actual requirement, in that declaration's source order.
    pub(crate) requirement_queries: Vec<ContractQueryId>,
    /// Formal relations callers may observe, in formal source order.
    pub(crate) postconditions: Vec<CheckedBoundPostcondition>,
}

/// One formal relation plus the exact accepted implication and actual premise
/// ordinals that authorize publishing it at a bound call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedBoundPostcondition {
    pub(crate) selector: super::postcondition::CheckedPostconditionSelector,
    pub(crate) relation: super::postcondition::RelationTemplate,
    pub(crate) query: ContractQueryId,
    pub(crate) actual_premises: Vec<u32>,
}

/// Every direct subexpression, for uniform recursion.
pub(crate) fn expression_children(expression: &CheckedExpression) -> Vec<&CheckedExpression> {
    match expression {
        CheckedExpression::Constant(_)
        | CheckedExpression::NamedConstant { .. }
        | CheckedExpression::Binding { .. }
        | CheckedExpression::ArrayMeasure { .. }
        | CheckedExpression::BufferMeasure { .. }
        | CheckedExpression::DerefAddressed { .. }
        | CheckedExpression::RangeMeasure { .. }
        | CheckedExpression::Project { .. } => Vec::new(),
        CheckedExpression::BorrowAddressed { root, .. }
        | CheckedExpression::ContainerMeasure { root, .. }
        | CheckedExpression::ReadStorage { root, .. } => root.offsets().collect(),
        CheckedExpression::UserCall { arguments, .. }
        | CheckedExpression::IntegerOperation { arguments, .. }
        | CheckedExpression::FloatOperation { arguments, .. }
        | CheckedExpression::BooleanOperation { arguments, .. }
        | CheckedExpression::EnumEquality { arguments, .. } => arguments.iter().collect(),
        CheckedExpression::NumericConversion { value, .. }
        | CheckedExpression::Reinterpret { value, .. }
        | CheckedExpression::BoxDeref { value, .. }
        | CheckedExpression::ProjectValue { value, .. } => vec![value.as_ref()],
        CheckedExpression::BoxTake { .. } => Vec::new(),
        CheckedExpression::ArrayIndex { offset, .. } => vec![offset.as_ref()],
        CheckedExpression::BufferIndex { offset, .. } => vec![offset.as_ref()],
        CheckedExpression::RangeElementMeasure { place, .. } => {
            let mut children = vec![&place.offset];
            children.extend(place.path.iter().filter_map(|step| match step {
                CheckedPlaceStep::Subscript(index) => Some(&index.offset),
                CheckedPlaceStep::Field(_) | CheckedPlaceStep::BoxReferent(_) => None,
            }));
            children
        }
        CheckedExpression::RangeIndex { place, .. }
        | CheckedExpression::BorrowRangeIndex { place, .. } => place.offsets().collect(),
        // [REF-4] both endpoints are evaluated once where the range is
        // formed, in written order, and the source place's own offsets are
        // read with them.
        CheckedExpression::RangeOf {
            source, start, end, ..
        } => match source {
            CheckedRangeSource::Storage(root) => root
                .offsets()
                .chain([start.as_ref(), end.as_ref()])
                .collect(),
            CheckedRangeSource::Range(_) => vec![start.as_ref(), end.as_ref()],
        },
        CheckedExpression::ConstructStruct { fields, .. }
        | CheckedExpression::ConstructEnum { fields, .. } => fields.iter().collect(),
    }
}

/// One call a checked body makes: its callee and the call's node path.
#[derive(Clone)]
pub(crate) struct MentionedCall {
    pub(crate) callee: FunctionId,
    pub(crate) path: NodePath,
}

/// What one checked function's own layout and body name: the type of each
/// parameter, of the result and of every value the body evaluates, binds or
/// releases, the interned elements its range places read, and every call it
/// makes. The [STOR-8] heap judgment and lowering's executable inventory read
/// the same walk.
#[derive(Default)]
pub(crate) struct FunctionMentions {
    pub(crate) types: Vec<CheckedType>,
    pub(crate) elements: Vec<CheckedElement>,
    pub(crate) calls: Vec<MentionedCall>,
}

impl FunctionMentions {
    pub(crate) fn collect(function: &CheckedFunction) -> Self {
        let mut dependencies = Self::default();
        dependencies.types.push(function.result);
        dependencies
            .types
            .extend(function.parameters.iter().map(|parameter| parameter.ty));
        if matches!(function.body_disposition, CheckedBodyDisposition::Inhabited) {
            dependencies.statements(function.body.as_deref().unwrap_or_default());
        }
        dependencies
    }

    fn statements(&mut self, statements: &[CheckedStatement]) {
        for statement in statements {
            match statement {
                CheckedStatement::Let { value, .. }
                | CheckedStatement::Evaluate { value, .. }
                | CheckedStatement::DropExpression { value, .. } => self.expression(value),
                CheckedStatement::DestructuringLet {
                    bindings,
                    nominal,
                    value,
                    ..
                } => {
                    self.types.push(CheckedType::Nominal(*nominal));
                    self.types.extend(bindings.iter().map(|(_, ty, _)| *ty));
                    self.expression(value);
                }
                CheckedStatement::PropagateLet {
                    scrutinee,
                    result_nominal,
                    return_nominal,
                    ok_type,
                    error_type,
                    error_drops,
                    ..
                } => {
                    self.types.extend([
                        CheckedType::Nominal(*result_nominal),
                        CheckedType::Nominal(*return_nominal),
                        *ok_type,
                        *error_type,
                    ]);
                    self.types.extend(error_drops.iter().map(|drop| drop.ty));
                    self.expression(scrutinee);
                }
                CheckedStatement::Set { target, value, .. } => {
                    self.target(target);
                    self.expression(value);
                }
                CheckedStatement::Proof(_) => {}
                CheckedStatement::Return { value, drops, .. }
                | CheckedStatement::Give { value, drops, .. } => {
                    self.expression(value);
                    self.types.extend(drops.iter().map(|drop| drop.ty));
                }
                CheckedStatement::Match {
                    scrutinee,
                    enum_type,
                    arms,
                    ..
                }
                | CheckedStatement::ValueMatchLet {
                    scrutinee,
                    enum_type,
                    arms,
                    ..
                } => {
                    if let CheckedStatement::ValueMatchLet {
                        result_type,
                        result_range_element,
                        ..
                    } = statement
                    {
                        self.types.push(*result_type);
                        self.elements.extend(result_range_element.iter().copied());
                    }
                    if let CheckedEnumType::Nominal(nominal) = enum_type {
                        self.types.push(CheckedType::Nominal(*nominal));
                    }
                    self.expression(scrutinee);
                    for arm in arms {
                        self.types
                            .extend(arm.binders.iter().map(|binder| binder.ty));
                        self.types.extend(arm.covered.iter().map(|drop| drop.ty));
                        self.types
                            .extend(arm.fallthrough_drops.iter().map(|drop| drop.ty));
                        self.statements(&arm.body);
                    }
                }
                CheckedStatement::Loop {
                    body,
                    backedge_drops,
                    ..
                }
                | CheckedStatement::CountedRange {
                    body,
                    backedge_drops,
                    ..
                } => {
                    if let CheckedStatement::CountedRange { lower, upper, .. } = statement {
                        self.expression(lower);
                        self.expression(upper);
                    }
                    self.types.extend(backedge_drops.iter().map(|drop| drop.ty));
                    self.statements(body);
                }
                CheckedStatement::Break { drops, .. } => {
                    self.types.extend(drops.iter().map(|drop| drop.ty));
                }
            }
        }
    }

    fn expression(&mut self, expression: &CheckedExpression) {
        self.types.push(expression.ty());
        match expression {
            CheckedExpression::UserCall {
                function,
                call,
                goal_regions,
                ..
            } => {
                debug_assert!(
                    goal_regions.is_empty(),
                    "[STOR-8] no call carries a region argument"
                );
                self.calls.push(MentionedCall {
                    callee: *function,
                    path: call.clone(),
                });
            }
            CheckedExpression::BoxDeref { nominal, .. }
            | CheckedExpression::ProjectValue { nominal, .. } => {
                self.types.push(CheckedType::Nominal(*nominal));
            }
            CheckedExpression::BoxTake { path, cleanup, .. } => {
                self.steps(path);
                for action in cleanup {
                    match action {
                        CheckedOwnedTakeCleanup::Drop { path, ty } => {
                            self.types.push(*ty);
                            self.steps(path);
                        }
                        CheckedOwnedTakeCleanup::BoxShell {
                            path,
                            nominal,
                            referent,
                        } => {
                            self.types
                                .extend([CheckedType::Nominal(*nominal), *referent]);
                            self.steps(path);
                        }
                    }
                }
            }
            CheckedExpression::Project { residual_drops, .. } => {
                self.types.extend(residual_drops.iter().map(|drop| drop.ty));
            }
            CheckedExpression::ContainerMeasure { root, .. }
            | CheckedExpression::ReadStorage { root, .. }
            | CheckedExpression::BorrowAddressed { root, .. } => self.root_types(root),
            CheckedExpression::BorrowRangeIndex { place, .. }
            | CheckedExpression::RangeIndex { place, .. } => {
                self.types.push(place.root.element_type);
                self.steps(&place.path);
            }
            CheckedExpression::RangeElementMeasure { place, .. } => {
                self.types.push(place.root.element_type);
                self.types.push(place.ty);
                self.steps(&place.path);
            }
            _ => {}
        }
        for child in expression_children(expression) {
            self.expression(child);
        }
    }

    fn target(&mut self, target: &CheckedSetTarget) {
        self.types.push(target.ty());
        match target {
            CheckedSetTarget::Place(_) => {}
            CheckedSetTarget::RangeIndex(target) => {
                self.types.push(target.root.element_type);
                for offset in target.offsets() {
                    self.expression(offset);
                }
                self.steps(&target.path);
            }
            CheckedSetTarget::Storage(root) => {
                self.root_types(root);
                for offset in root.offsets() {
                    self.expression(offset);
                }
            }
        }
    }

    fn root_types(&mut self, root: &CheckedContainerRoot) {
        self.types.push(root.ty);
        self.steps(&root.path);
    }

    fn steps(&mut self, steps: &[CheckedPlaceStep]) {
        for step in steps {
            match step {
                CheckedPlaceStep::Field(_) => {}
                CheckedPlaceStep::BoxReferent(nominal) => {
                    self.types.push(CheckedType::Nominal(*nominal));
                }
                CheckedPlaceStep::Subscript(subscript) => {
                    self.types
                        .extend([subscript.base_type, subscript.element_type]);
                    self.expression(&subscript.offset);
                }
            }
        }
    }
}
