//! Checked FN-9 declaration surface shared with the later callee-proof pass.
//!
//! This module deliberately contains only source-derived, concrete-instance
//! metadata.  The symbolic result is an identity in a relation template, not
//! a declaration, [`super::model::BindingId`], storage location, or executable
//! expression.  Task 0060 consumes these values to prove selected exits; H1
//! stops explicitly before that proof.

use crate::{BuiltinPreludeId, DeclarationId, NodePath, SourceOrigin};

use super::goal::GoalProjection;
use super::model::{
    BindingId, CheckedIntegerOperation, CheckedMeasure, CheckedType, CheckedValue, FunctionId,
};

/// One admitted concrete selector and its resolver-owned source identities.
#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedPostconditionSelector {
    pub(crate) function: FunctionId,
    pub(crate) block: NodePath,
    pub(crate) selector: NodePath,
    pub(crate) candidate: SourceOrigin,
    /// The declared result ordinal this clause's result datum names
    /// [CALL-4]. A declaration that writes one result has ordinal zero, the
    /// route writes no ordinal binder, and every clause names it.
    pub(crate) ordinal: u32,
    pub(crate) variant: Option<BuiltinPreludeId>,
    pub(crate) field: Option<PostconditionFieldIdentity>,
    pub(crate) result_type: CheckedType,
}

/// Exact PRE-1 field identity and written origin of `Ok(value: result)`.
#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PostconditionFieldIdentity {
    pub(crate) declaration: BuiltinPreludeId,
    pub(crate) origin: SourceOrigin,
}

/// One alpha-expanded, output-bearing L0 relation occurrence.
#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RelationTemplate {
    pub(crate) operation: CheckedIntegerOperation,
    pub(crate) operands: [RelationTerm; 2],
    pub(crate) normalized: NormalizedRelation,
}

/// One operand of a declared relation: one datum displaced by a written
/// constant [FN-9].
///
/// A clause side is an `affine_expr` [MSR-5], and the part of it that is not
/// the datum reduces to one mathematical integer. That is exactly the shape
/// [ENT-4]'s closure represents and the shape every declared relation of the
/// kernel declaration domain writes [BLK-0], so `len_of(rest) + 1_u64 ==
/// len_of(vector)` is one relation occurrence and not a second fact class.
#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RelationTerm {
    pub(crate) datum: RelationDatum,
    pub(crate) displacement: i128,
}

impl RelationTerm {
    pub(crate) const fn undisplaced(datum: RelationDatum) -> Self {
        Self {
            datum,
            displacement: 0,
        }
    }

    pub(crate) const fn ty(&self) -> CheckedType {
        self.datum.ty()
    }

    pub(crate) const fn contains_result(&self) -> bool {
        self.datum.contains_result()
    }
}

/// Direction-normalized relation shape. Equality denotes its ordinary pair
/// of non-strict bounds; disequality remains its one L0 disequality.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NormalizedRelation {
    Equal,
    NotEqual,
    /// `operands[left] <(=) operands[right]` after reversing `>`/`>=`.
    UpperBound {
        left: u8,
        right: u8,
        strict: bool,
    },
}

/// One closed FN-9 RelationTemplate datum after clause-local expansion.
#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RelationDatum {
    Result {
        /// The declared result ordinal this datum names [CALL-4]. A
        /// declaration writing one result has ordinal zero.
        ordinal: u32,
        /// The owned descendant projection [MSR-3] from the ordinal's value
        /// to the fragment-integer place this datum names: empty for a
        /// fragment-integer result, and struct-field and `Box` content steps
        /// for an integer field of an aggregate result [CALL-4].
        projections: Vec<GoalProjection>,
        ty: CheckedType,
    },
    Parameter {
        ordinal: u32,
        projections: Vec<GoalProjection>,
        ty: CheckedType,
        /// Which of [MSR-3]'s table rows gives this operand its meaning.
        denotation: ParameterDenotation,
    },
    NamedConst {
        declaration: DeclarationId,
        projections: Vec<GoalProjection>,
        ty: CheckedType,
    },
    Literal {
        value: CheckedValue,
        origin: PostconditionConstantOrigin,
    },
    Measure(CheckedMeasure, PostconditionPlace),
}

impl RelationDatum {
    pub(crate) const fn ty(&self) -> CheckedType {
        match self {
            Self::Result { ty, .. } | Self::Parameter { ty, .. } | Self::NamedConst { ty, .. } => {
                *ty
            }
            Self::Literal { value, .. } => value.ty(),
            Self::Measure(..) => CheckedType::Integer(super::model::IntegerType::U64),
        }
    }

    /// Whether this datum names a declared result ordinal [CALL-4], as the
    /// value itself or as a measure over that ordinal's place.
    pub(crate) const fn contains_result(&self) -> bool {
        match self {
            Self::Result { .. } => true,
            Self::Measure(_, place) => {
                matches!(place.root, PostconditionPlaceRoot::Result { .. })
            }
            Self::Parameter { .. } | Self::NamedConst { .. } | Self::Literal { .. } => false,
        }
    }

    /// Whether this datum is the exit state of a reference parameter whose
    /// row writes it [MSR-3], a measure or a fragment-integer place alike.
    pub(crate) const fn is_exit_state(&self) -> bool {
        match self {
            Self::Measure(_, place) => {
                matches!(place.root, PostconditionPlaceRoot::ExitParameter { .. })
            }
            Self::Parameter { denotation, .. } => {
                matches!(denotation, ParameterDenotation::ExitState)
            }
            Self::Result { .. } | Self::NamedConst { .. } | Self::Literal { .. } => false,
        }
    }
}

/// What one non-measure parameter datum of an `ensures` clause denotes
/// [MSR-3, FN-9].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ParameterDenotation {
    /// The function-entry image, read over the live place while [FN-9]'s
    /// entry-image stability holds. Every datum outside the two rows below.
    EntryImage,
    /// A bare place of a reference parameter whose row writes it: the
    /// selected return's exit state, instantiated at the caller's resolved
    /// exit place.
    ExitState,
    /// The same place under `entry(parameter)`: the immutable entry datum,
    /// instantiated at the caller as that call's call datum.
    EntryDatum,
}

/// Source category of a constant-valued atom.  Concrete substitution never
/// erases whether mathematical one came from a literal, named constant, or a
/// generic source; task 0060's closed S7 admission depends on that distinction.
#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PostconditionConstantOrigin {
    Literal,
    NamedConst(DeclarationId),
    GenericNumericIdentity {
        type_parameter: DeclarationId,
        one: bool,
    },
    /// One in-scope const generic named as a clause operand [MSR-6]. Its
    /// value lives in the datum itself, concrete or symbolic; this origin
    /// retains only the declaration the spelling resolved to.
    ConstGeneric {
        declaration: DeclarationId,
    },
}

/// A parameter or named-const place accepted below `len`.
#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PostconditionPlace {
    pub(crate) root: PostconditionPlaceRoot,
    pub(crate) projections: Vec<GoalProjection>,
    pub(crate) ty: CheckedType,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PostconditionPlaceRoot {
    Parameter {
        ordinal: u32,
    },
    /// The resolved exclusive referent at the selected return or call exit.
    ExitParameter {
        ordinal: u32,
    },
    /// One declared result ordinal [CALL-4]: a measure over an admitted
    /// result place is an operand with no per-family admission, exactly as a
    /// measure over an admitted formal place is.
    Result {
        ordinal: u32,
    },
}

/// One selected explicit normal return, already classified by H1 so H2 need
/// not re-recognize syntax or infer a Result route.
#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SelectedPostconditionReturn {
    pub(crate) statement: NodePath,
    /// The datum each declared result ordinal evaluates to at this return, in
    /// written order [CALL-4]. An ordinal the return leaves outside the
    /// [ENT-2] term fragment has no datum; a clause naming it is not selected
    /// at this return. A declaration writing one result has one entry.
    pub(crate) values: Vec<Option<PostconditionReturnDatum>>,
}

/// Exact ENT-2-shaped datum evaluated at a selected return.
#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PostconditionReturnDatum {
    /// Success payload of a forwarded Result, read only in that value's
    /// isolated conditional proof context.
    ResultPayload {
        ty: CheckedType,
    },
    Place(PostconditionReturnPlace),
    Literal {
        value: CheckedValue,
        origin: PostconditionConstantOrigin,
    },
    Measure(CheckedMeasure, PostconditionReturnPlace),
    /// A returned struct construction [FN-9]: the value of each field
    /// operand, in declaration order, each an atom [GRAM-9]. A projected
    /// result datum selects one field and continues below its operand.
    Construct {
        fields: Vec<Option<PostconditionReturnDatum>>,
    },
}

/// Complete checked place identity for a selected result term or `len_of(P)`.
/// Field and dereference projections remain ordered and the root retains its
/// binding/constant class, so the proof pass never re-walks source syntax.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub(crate) struct PostconditionReturnPlace {
    pub(crate) root: PostconditionReturnPlaceRoot,
    pub(crate) projections: Vec<GoalProjection>,
    pub(crate) ty: CheckedType,
    /// [TYPE-8, REF-4] the place is a range reference whose measured kind
    /// cannot be recovered from its element `ty`.
    pub(crate) range_referent: bool,
    pub(crate) source: NodePath,
}

impl PartialEq for PostconditionReturnPlace {
    fn eq(&self, other: &Self) -> bool {
        self.root == other.root
            && self.projections == other.projections
            && self.ty == other.ty
            && self.range_referent == other.range_referent
    }
}

impl Eq for PostconditionReturnPlace {}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PostconditionReturnPlaceRoot {
    Binding(BindingId),
    NamedConst(DeclarationId),
}

/// Complete checked H1 handoff for one concrete FN-9 occurrence.
#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedPostcondition {
    pub(crate) selector: CheckedPostconditionSelector,
    /// Concrete declaration-to-argument identity in written generic order.
    /// These entries are source-free template identity; occurrence paths live
    /// on the surrounding selector and selected returns instead.
    pub(crate) type_substitutions: Vec<(DeclarationId, CheckedType)>,
    pub(crate) const_substitutions: Vec<(DeclarationId, u64)>,
    pub(crate) relation: RelationTemplate,
    pub(crate) selected_returns: Vec<SelectedPostconditionReturn>,
}

mod products;
