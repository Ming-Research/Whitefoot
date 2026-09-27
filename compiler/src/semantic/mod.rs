//! Target-independent semantic checking for the active Whitefoot specification.
//!
//! This stage consumes complete lexical resolution and is the sole producer of
//! the private checked-program value that may later authorize lowering. A
//! language feature not implemented yet is reported as an unsupported compiler
//! capability, never as a source-language rejection.

mod check;
mod entailment;
mod entry;
mod goal;
mod loop_permission;
mod model;
mod obligations;
pub(crate) mod permission;
mod permission_ledger;
mod places;
pub(crate) use places::PlaceRoot as CheckedPlaceRoot;
mod postcondition;
mod tree;

#[cfg(test)]
mod tests;

use crate::{NodePath, ResolutionIssue, SyntaxCoordinate};

pub use check::check_semantics;
#[cfg(test)]
pub(crate) use check::check_semantics_arithmetic_obligations;
#[cfg(test)]
pub(crate) use check::check_semantics_division_obligations;
pub(crate) use check::{ProofReceipts, check_semantics_with_receipts};
pub(crate) use entry::{EntryRejection, EntryRequest};

/// The permission table the overlap lowering reads. It is the same table the
/// ledger renders; nothing derives a second judgment from it.
pub(crate) use permission::FunctionPermissions;

/// One counted loop's [PAR-2] verdict and, where the loop is permitted and
/// eligible, the two identities actualizing it needs. Lowering reads these; it
/// never derives a verdict of its own from them.
pub(crate) use loop_permission::{LoopActualization, LoopCombine, LoopPermission};

pub(crate) use model::{
    BindingId, CheckedArrayRoot, CheckedBodyDisposition, CheckedBooleanOperation,
    CheckedBufferRoot, CheckedConst, CheckedContainerRoot, CheckedConversionMode, CheckedDrop,
    CheckedElement, CheckedEnumType, CheckedExpression, CheckedFloatOperation, CheckedFunction,
    CheckedIntegerOperation, CheckedLayoutCeiling, CheckedLayoutMagnitude, CheckedLoopId,
    CheckedMatchArm, CheckedMeasure, CheckedMode, CheckedNominalKind, CheckedNumericType,
    CheckedOwnedTakeCleanup, CheckedParameter, CheckedPlaceStep, CheckedProgramData,
    CheckedProjectedDrop, CheckedRangeElementPlace, CheckedRangeRoot, CheckedRangeSource,
    CheckedReleaseClass, CheckedSetTarget, CheckedStatement, CheckedTargetDomainObligation,
    CheckedType, CheckedValue, CheckedWritablePlace, FunctionId, FunctionMentions, MeasureCell,
    MeasuredKind, NominalId, PropagationContext, WindowShape,
};

/// Numbered rule owning one post-resolution semantic rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SemanticRule {
    /// Generic numeric identity literal eligibility.
    Form5,
    /// Numeric literal range or canonicality.
    Form7,
    /// Type-driven conditional form, and the `else` spellings it forbids.
    Gram6,
    /// Value-match delivery.
    Give1,
    /// Exact declared-order construction fields.
    Gram8,
    /// Exact declared-order match binders.
    Gram10,
    /// Exact declared-order named user-call arguments.
    Gram11,
    /// Composite-type formation and element eligibility.
    Type2,
    /// Cross-module access to a declaration's field [MOD-5].
    Mod5,
    /// A public signature naming an unpublished field [MOD-6].
    Mod6,
    /// Exact mode/type agreement.
    Type5,
    /// Constructor/variant owner agreement.
    Type6,
    /// The three storage shapes and the cell: constant-capacity placement,
    /// the runtime-capacity forms' `Box`-content-only position, and the
    /// refusal of a compiler-owned nominal's constructor `call`.
    Type9,
    /// Measures and window parts are names, not declarations: a source write
    /// to `len`, `cap`, `head`, `next`, `last`, `filled`, or `free`.
    Type10,
    /// Reaching `Box` content is explicit; a reference is read bare.
    Type7,
    /// Place assignment.
    Set1,
    /// Constant-expression formation and evaluation.
    Const1,
    /// Named-constant type and value formation.
    Const2,
    /// Copy-versus-affine use spelling, the one consuming use, and the death
    /// of the whole binding that rooted a consumed place.
    Own1,
    /// A reference is a local name for a path: the forbidden reference to a
    /// reference variable, the join of a reference variable's path set, and
    /// the static path shape a loop-carried rebinding may not extend.
    Ref1,
    /// Reference validity is a fact: a use of a reference invalidated by a
    /// write, move, or release of a proper prefix, by the end of its root
    /// binding's scope, or by the loss of a payload refinement fact.
    Ref2,
    /// References never escape: assignment into an aggregate, a return, or
    /// capture by a stored function value.
    Ref3,
    /// Range references: formation over an indexable place or another range
    /// reference, re-slicing, and the refusal of a range over a `Ring`.
    Ref4,
    /// Loop-local bindings and the per-iteration liveness agreement read at
    /// the loop head, plus the counted binder's own restrictions.
    Own11,
    /// Join-checked liveness: every predecessor of a join agrees on a
    /// binding's live-or-dead status.
    Liv1,
    /// Linearity read against the scope: the release graph, the `linear`
    /// modifier, the destructuring consume, the partial-consume refusal, and
    /// the linearity bound on a generic parameter.
    Prov6,
    /// There is no take operation and no hole: a move out of a field or of
    /// `Box` content consuming the whole owner, a remaining linear part, a
    /// move out of a window slot or array element, and an assignment over a
    /// linear owned place.
    Win3,
    /// The one heap: total allocation, and the no-heap declaration's refusal
    /// of `Box`, the runtime-capacity shapes, and the allocating rows.
    Stor8,
    /// Operation-table row selection.
    Op1,
    /// Exact integer arithmetic semantics and the constant-operand-class
    /// overflow-obligation discharge.
    Op2,
    /// Subscript base class, bounds-obligation discharge, and offset typing.
    Op4,
    /// Exact `own Bool` explicit-check condition.
    Op5,
    /// Exact conversion-pair result classification.
    Op6,
    /// The static allocation-size obligation over a stored type and a
    /// runtime count.
    Op9,
    /// The window operations: the admitted argument set of the
    /// compiler-owned window parameter, and which boundary each operation
    /// moves.
    Op10,
    /// `swap`: two owned places of one type, neither root consumed, and the
    /// refusal of a `swap` over a copy place.
    Op11,
    /// The atomic in-place update: the target class, the result condition,
    /// and the callee row's refusal to reach a prefix of the target.
    Op12,
    /// `free_empty`: the admitted shape set and the proved-empty obligation.
    Op14,
    /// Function result, reachability, or completion.
    Fn1,
    /// Explicit generic-instantiation argument presence.
    Fn2,
    /// Numeric bounds and named parameter/argument group formation.
    Fn3,
    /// Function-kind signature refinement against the instantiated formal.
    Fn4,
    /// Explicit static member selection from a formal parameter group.
    Fn5,
    /// Polymorphic recursion in a call cycle among generic functions.
    Fn6,
    /// Finite atomic function requirement goal.
    Fn8,
    /// Verified narrow normal-return relation.
    Fn9,
    /// Guaranteed direct self-tail call and activation replacement.
    Fn10,
    /// Contract vocabulary, the result ordinal, the routes, and where the
    /// relations land.
    Call4,
    /// Effect-row canonicality.
    Eff1,
    /// Exact exhibited-versus-declared effect row.
    Eff2,
    /// The call-site substitution and its pairwise comparison: two
    /// overlapping substituted effects at least one of which writes, the
    /// by-value argument's own contribution, and the outside references the
    /// call invalidates.
    Eff5,
    /// Exhaustive enum matching.
    Err2,
    /// Exact Result propagation and same-error forwarding.
    Err3,
    /// Counted endpoint admission to the closed term-or-constant vocabulary.
    Ent2,
    /// One denotation per operand position, keyed on the parameter's mode.
    Msr3,
    /// Publication: where a declared relation is instantiated, where it is
    /// established, and that a published relation set is consistent.
    Call6,
    /// Proof-only loop invariant formation.
    Inv1,
    /// Finite source-written affine proof formation and checking.
    Prf1,
}

impl SemanticRule {
    /// Returns the exact numbered rule spelling from the active kernel specification.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Form5 => "FORM-5",
            Self::Form7 => "FORM-7",
            Self::Gram6 => "GRAM-6",
            Self::Give1 => "GIVE-1",
            Self::Gram8 => "GRAM-8",
            Self::Gram10 => "GRAM-10",
            Self::Gram11 => "GRAM-11",
            Self::Type2 => "TYPE-2",
            Self::Mod5 => "MOD-5",
            Self::Mod6 => "MOD-6",
            Self::Type5 => "TYPE-5",
            Self::Type6 => "TYPE-6",
            Self::Type9 => "TYPE-9",
            Self::Type10 => "TYPE-10",
            Self::Type7 => "TYPE-7",
            Self::Set1 => "SET-1",
            Self::Const1 => "CONST-1",
            Self::Const2 => "CONST-2",
            Self::Own1 => "OWN-1",
            Self::Ref1 => "REF-1",
            Self::Ref2 => "REF-2",
            Self::Ref3 => "REF-3",
            Self::Ref4 => "REF-4",
            Self::Own11 => "OWN-11",
            Self::Liv1 => "LIV-1",
            Self::Prov6 => "PROV-6",
            Self::Win3 => "WIN-3",
            Self::Stor8 => "STOR-8",
            Self::Op1 => "OP-1",
            Self::Op2 => "OP-2",
            Self::Op4 => "OP-4",
            Self::Op5 => "OP-5",
            Self::Op6 => "OP-6",
            Self::Op9 => "OP-9",
            Self::Op10 => "OP-10",
            Self::Op11 => "OP-11",
            Self::Op12 => "OP-12",
            Self::Op14 => "OP-14",
            Self::Fn1 => "FN-1",
            Self::Fn2 => "FN-2",
            Self::Fn3 => "FN-3",
            Self::Fn4 => "FN-4",
            Self::Fn5 => "FN-5",
            Self::Fn6 => "FN-6",
            Self::Fn8 => "FN-8",
            Self::Fn9 => "FN-9",
            Self::Fn10 => "FN-10",
            Self::Call4 => "CALL-4",
            Self::Eff1 => "EFF-1",
            Self::Eff2 => "EFF-2",
            Self::Eff5 => "EFF-5",
            Self::Err2 => "ERR-2",
            Self::Err3 => "ERR-3",
            Self::Ent2 => "ENT-2",
            Self::Msr3 => "MSR-3",
            Self::Call6 => "CALL-6",
            Self::Inv1 => "INV-1",
            Self::Prf1 => "PRF-1",
        }
    }

    /// The first rule in the active specification's definition order.
    #[cfg(test)]
    pub(crate) const FIRST: Self = Self::Form5;

    /// The rule defined immediately after this one, or `None` for the last.
    ///
    /// This is the enumeration of the semantic rules, and it exists so that a
    /// checked set over them cannot silently omit one. The match is exhaustive,
    /// so a new variant does not compile until it is given a position here and
    /// a rank in [`Self::definition_rank`] — two matches that
    /// `definition_rank_matches_the_active_specification` then checks against
    /// each other, since walking this chain must yield the ranks 0, 1, 2, … in
    /// order. `SemanticRule::Gram6` was omitted from that check's
    /// hand-maintained list until 2026-08-08, which is the omission this makes
    /// impossible rather than merely unlikely.
    #[cfg(test)]
    pub(crate) const fn next_in_definition_order(self) -> Option<Self> {
        Some(match self {
            Self::Form5 => Self::Form7,
            Self::Form7 => Self::Gram6,
            Self::Gram6 => Self::Give1,
            Self::Give1 => Self::Gram8,
            Self::Gram8 => Self::Gram10,
            Self::Gram10 => Self::Gram11,
            Self::Gram11 => Self::Type2,
            Self::Type2 => Self::Type5,
            Self::Type5 => Self::Type6,
            Self::Type6 => Self::Type9,
            Self::Type9 => Self::Type10,
            Self::Type10 => Self::Type7,
            Self::Type7 => Self::Set1,
            Self::Set1 => Self::Const1,
            Self::Const1 => Self::Const2,
            Self::Const2 => Self::Own1,
            Self::Own1 => Self::Ref1,
            Self::Ref1 => Self::Ref2,
            Self::Ref2 => Self::Ref3,
            Self::Ref3 => Self::Ref4,
            Self::Ref4 => Self::Own11,
            Self::Own11 => Self::Liv1,
            Self::Liv1 => Self::Prov6,
            Self::Prov6 => Self::Win3,
            Self::Win3 => Self::Stor8,
            Self::Stor8 => Self::Op1,
            Self::Op1 => Self::Op2,
            Self::Op2 => Self::Op4,
            Self::Op4 => Self::Op5,
            Self::Op5 => Self::Op6,
            Self::Op6 => Self::Op9,
            Self::Op9 => Self::Op10,
            Self::Op10 => Self::Op11,
            Self::Op11 => Self::Op12,
            Self::Op12 => Self::Op14,
            Self::Op14 => Self::Fn1,
            Self::Fn1 => Self::Fn2,
            Self::Fn2 => Self::Fn3,
            Self::Fn3 => Self::Fn4,
            Self::Fn4 => Self::Fn5,
            Self::Fn5 => Self::Fn6,
            Self::Fn6 => Self::Fn8,
            Self::Fn8 => Self::Fn9,
            Self::Fn9 => Self::Call4,
            Self::Call4 => Self::Fn10,
            Self::Fn10 => Self::Eff1,
            Self::Eff1 => Self::Eff2,
            Self::Eff2 => Self::Eff5,
            Self::Eff5 => Self::Err2,
            Self::Err2 => Self::Err3,
            Self::Err3 => Self::Mod5,
            Self::Mod5 => Self::Mod6,
            Self::Mod6 => Self::Ent2,
            Self::Ent2 => Self::Msr3,
            Self::Msr3 => Self::Call6,
            Self::Call6 => Self::Inv1,
            Self::Inv1 => Self::Prf1,
            Self::Prf1 => return None,
        })
    }

    /// [DIAG-1] same-node citation rank: this rule's definition position in
    /// the active kernel specification. Simultaneously established
    /// post-resolution rejections whose offending premise is the same use of
    /// the same canonical node are one rejection event citing the established
    /// rule whose rank is least, so a site with a known simultaneity asks its
    /// judgments in ascending rank order. The order is machine-checked
    /// against the active specification text by
    /// `definition_rank_matches_the_active_specification`.
    #[must_use]
    pub const fn definition_rank(self) -> usize {
        match self {
            Self::Form5 => 0,
            Self::Form7 => 1,
            Self::Gram6 => 2,
            Self::Give1 => 3,
            Self::Gram8 => 4,
            Self::Gram10 => 5,
            Self::Gram11 => 6,
            Self::Type2 => 7,
            Self::Type5 => 8,
            Self::Type6 => 9,
            Self::Type9 => 10,
            Self::Type10 => 11,
            Self::Type7 => 12,
            Self::Set1 => 13,
            Self::Const1 => 14,
            Self::Const2 => 15,
            Self::Own1 => 16,
            Self::Ref1 => 17,
            Self::Ref2 => 18,
            Self::Ref3 => 19,
            Self::Ref4 => 20,
            Self::Own11 => 21,
            Self::Liv1 => 22,
            Self::Prov6 => 23,
            Self::Win3 => 24,
            Self::Stor8 => 25,
            Self::Op1 => 26,
            Self::Op2 => 27,
            Self::Op4 => 28,
            Self::Op5 => 29,
            Self::Op6 => 30,
            Self::Op9 => 31,
            Self::Op10 => 32,
            Self::Op11 => 33,
            Self::Op12 => 34,
            Self::Op14 => 35,
            Self::Fn1 => 36,
            Self::Fn2 => 37,
            Self::Fn3 => 38,
            Self::Fn4 => 39,
            Self::Fn5 => 40,
            Self::Fn6 => 41,
            Self::Fn8 => 42,
            Self::Fn9 => 43,
            Self::Call4 => 44,
            Self::Fn10 => 45,
            Self::Eff1 => 46,
            Self::Eff2 => 47,
            Self::Eff5 => 48,
            Self::Err2 => 49,
            Self::Err3 => 50,
            Self::Mod5 => 51,
            Self::Mod6 => 52,
            Self::Ent2 => 53,
            Self::Msr3 => 54,
            Self::Call6 => 55,
            Self::Inv1 => 56,
            Self::Prf1 => 57,
        }
    }
}

/// Exact checked location selected for a semantic rejection, or of a node its
/// payload names.
///
/// A payload that names another node, such as the callee requirement an
/// [FN-8] rejection failed, carries it in this form rather than as a bare
/// path, so the node reaches a reader as a source position: the checker holds
/// the tree that resolves the path, and the driver that renders the rejection
/// does not.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SemanticLocation {
    /// One source-backed production node and its rule-selected coordinate.
    SourceNode(NodePath, SyntaxCoordinate),
}

impl SemanticLocation {
    /// Returns the production node's path from the compilation-unit root.
    #[must_use]
    #[cfg(test)]
    pub const fn path(&self) -> &NodePath {
        let Self::SourceNode(path, _) = self;
        path
    }

    /// Returns the rule-selected source coordinate within that node.
    #[must_use]
    pub const fn coordinate(&self) -> SyntaxCoordinate {
        let Self::SourceNode(_, coordinate) = self;
        *coordinate
    }
}

/// One non-discharged static source obligation disposition [ENT-6].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StaticObligationDisposition {
    /// The closed state derives the canonical goal or normalization false.
    Refuted,
    /// The closed state derives neither a successful nor a refuting route.
    Unproved,
}

/// Which INV-1 induction obligation a source loop invariant failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoopInvariantProofObligation {
    /// The invariant did not follow from the loop preheader facts at the first
    /// loop header.
    Base,
    /// Some reachable normal body fallthrough did not preserve the invariant
    /// at the next loop header. A counted loop includes its hidden unit binder
    /// update in this transition.
    Backedge,
}

/// The first failed part of one erased local invariant certificate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceProofObligation {
    /// Zero-based `use` occurrence in source order.
    Premise(u32),
    /// The written weighted sum plus the fixed direct residual rule did not
    /// establish the invariant target.
    Combination,
    /// AUTO already established the target, so the entire written `use` block
    /// is forbidden redundant proof text in this specification version.
    RedundantUseBlock,
    /// Two normalized `use` relations are identical; one explicitly scaled
    /// use must express their combined contribution.
    RepeatedUse { first: u32, repeated: u32 },
    /// The source list exceeds the fixed structural use capacity.
    UseCapacity { maximum: u32, actual: u32 },
    /// A written proof-domain factor or source-order accumulated certificate
    /// exceeded the admitted i128 arithmetic.
    CertificateArithmeticOverflow,
    /// The accumulated certificate exceeded a fixed affine shape capacity.
    CertificateFormationCapacity,
    /// A nonpositive factor reached the certificate core. Canonical source
    /// checking normally rejects this before entailment.
    InvalidUseFactor { use_index: u32 },
    /// A term multiplicity left the certificate sum with a product of two
    /// values that no admitted exact multiplication in scope equals, so the
    /// sum never reduced to an affine inequality.
    NonlinearCertificateSum,
}

/// One non-discharged [FN-8] ordinary-call goal disposition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CallRequirementDisposition {
    /// The entering state derives the goal's exact negative sign.
    Refuted,
    /// The entering state derives neither exact sign.
    Unproved,
}

/// The deterministic [FN-8] ordinary-call rejection payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UndischargedCallRequirementDetail {
    /// The resolved concrete, possibly generic, callee instance.
    pub concrete_callee: String,
    /// The callee requirement occurrence's `requires_clause` node and its
    /// complete source extent.
    pub requires_clause: SemanticLocation,
    /// Stable structural rendering of the complete instantiated typed goal.
    pub instantiated_goal: String,
    /// The exact non-discharged disposition.
    pub disposition: CallRequirementDisposition,
    /// The repair [DIAG-1], selected by the disposition and by what the
    /// goal's terms are.
    pub mechanical_fix: String,
}

/// One non-discharged [FN-9] relation disposition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PostconditionProofDisposition {
    Refuted,
    Unproved,
}

/// The deterministic [FN-9] selected-return rejection payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UndischargedPostconditionDetail {
    /// The concrete, possibly generic, function instance.
    pub concrete_function: String,
    /// The unique postcondition occurrence's block node and its complete
    /// source extent.
    pub postcondition: SemanticLocation,
    /// The fixed relation occurrence ordinal (zero in this version).
    pub conjunct: u32,
    /// Exact admitted selector identity and its complete source extent.
    pub selector: SemanticLocation,
    /// The instantiated normalized relation at the selected exit.
    pub relation: String,
    /// The exact non-discharged disposition.
    pub disposition: PostconditionProofDisposition,
    /// The repair [DIAG-1], selected by the disposition.
    pub mechanical_fix: &'static str,
}

/// Structured reason for one semantic rejection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SemanticIssueKind {
    /// A literal is not the unique in-range FORM-7 spelling.
    InvalidIntegerLiteral,
    /// A float literal is not FORM-5's unique finite canonical spelling.
    InvalidFloatLiteral,
    /// A named constant value does not exactly inhabit its written type.
    InvalidConstValue,
    /// Code or an annotation of another module selects, constructs or binds
    /// a field its declaring module does not publish or its graph row does
    /// not reach, or constructs a value with a readonly field [MOD-5,
    /// TYPE-2].
    InaccessibleField {
        /// The field's spelling.
        field: String,
        /// What access the module lacks.
        reason: &'static str,
    },
    /// An arm names a variant of an enum its module cannot access [MOD-5].
    InaccessibleVariant {
        /// The variant's spelling.
        variant: String,
        /// What access the module lacks.
        reason: &'static str,
    },
    /// A named const's value depends on itself through the listed consts,
    /// in dependency order [CONST-2].
    ConstantCycle {
        /// The consts on the cycle, beginning at the rejected one.
        cycle: Vec<String>,
    },
    /// A const-expression's compile-time evaluation has no u64 result: the
    /// mathematical result lies outside the domain or the divisor is zero.
    /// This is the const-eval overflow policy's rejection [CONST-1]; it is
    /// never a runtime fallback and never enters EFF-2's effect relation.
    ConstEvalOverflow {
        /// Bare spelling of the rejected const operation.
        operation: &'static str,
    },
    /// A const-expression names a runtime arithmetic mode; const evaluation
    /// has exactly the five bare spellings under the const-eval overflow
    /// policy [CONST-1].
    ConstRuntimeArithmeticMode {
        /// Exact mechanical repair selected by CONST-1.
        mechanical_fix: &'static str,
    },
    /// Two exact modes or types disagree.
    TypeMismatch {
        /// The exact type, mode, or written form the position requires.
        expected: String,
        /// The exact type, mode, or written form found there.
        found: String,
    },
    /// [OP-10, OP-14] an operand whose shape is outside the operation's
    /// admitted set.
    UnadmittedOperandShape {
        /// The shapes the operation admits.
        expected: &'static str,
        /// The repair [DIAG-1].
        mechanical_fix: &'static str,
    },
    /// [FN-4] a supplied function whose signature, row or contract does not
    /// match the formal interface it is bound to.
    BehaviorArgumentMismatch {
        /// The part of the formal interface the supplied function misses.
        expected: String,
        /// The repair [DIAG-1].
        mechanical_fix: &'static str,
    },
    /// A constant was selected as an assignment target.
    ImmutableSetTarget,
    /// SET-1's closed writability relation did not admit the target root.
    InvalidSetTarget {
        /// Resolved target-root class.
        root_class: String,
        /// Closed set of classes required by SET-1.
        required_classes: &'static str,
    },
    /// A written reference argument reaches immutable source storage.
    ImmutableWrittenArgument {
        /// The named constant or compiler-updated counted binder.
        binding: String,
        /// Restructuring that gives the callee independently writable storage.
        mechanical_fix: &'static str,
    },
    /// [PROV-6] a value linear in this scope is live on an edge leaving it,
    /// where no compiler-derived release exists to carry it.
    LinearValueNotConsumed {
        /// The binding whose value is linear here.
        binding: String,
        /// The nominal whose `linear` declaration created the obligation.
        obligation: String,
        /// Exact restructuring required by PROV-6.
        mechanical_fix: &'static str,
    },
    /// [PROV-6] a consume of a proper sub-place of a value linear in this
    /// scope, with no commit reinitializing that sub-place.
    LinearValuePartiallyConsumed {
        /// The nominal whose `linear` declaration created the obligation.
        obligation: String,
        /// The residual the consume would abandon.
        residual: String,
        /// Exact restructuring required by PROV-6.
        mechanical_fix: &'static str,
    },
    /// [PROV-6] an instantiation whose argument's capability class does not
    /// satisfy the parameter's written bound.
    LinearityBoundMismatch {
        /// The bounded parameter's written spelling.
        parameter: String,
        /// The written bound.
        bound: &'static str,
        /// The written argument.
        argument: String,
        /// The argument's actual class.
        actual: &'static str,
    },
    /// [LIV-1] two predecessors of one join disagree about whether a binding
    /// is live there.
    LivenessJoinDisagreement {
        /// The binding whose status the predecessors disagree about.
        binding: String,
        /// The predecessor that reaches the join with the binding live.
        live_predecessor: String,
        /// The predecessor that reaches the join with the binding dead.
        dead_predecessor: String,
        /// Exact restructuring required by LIV-1.
        mechanical_fix: &'static str,
    },
    /// [WIN-3] an assignment over a place whose final selected type is
    /// linear. Assigning over an owned place releases the old value when it
    /// is affine; a linear value has no release, so the write is refused.
    ///
    /// This is the successor of v0.59's `RegionBearingCommitTarget` and
    /// `InvalidReplaceTarget`. Both stated a class demand over the target's
    /// selected type — the first that no commit reinitializes a region, the
    /// second that a `replace` target be region-free affine — and both had
    /// regions and the `replace` statement as their subject.
    LinearAssignmentTarget {
        /// Exact selected type.
        target_type: String,
        /// Exact restructuring required by WIN-3.
        mechanical_fix: &'static str,
    },
    /// [TYPE-9] a runtime-capacity storage shape was written in a position
    /// that stores it inline. Such a shape may appear only as the content of
    /// a `Box` — the type of its `inner` field.
    InlineRuntimeCapacityShape {
        /// The exact written shape.
        spelling: String,
        /// Exact restructuring required by TYPE-9.
        mechanical_fix: &'static str,
    },
    /// [OP-11] `swap` was written over a copy place. `swap` exists because no
    /// source body can write it without a hole [WIN-3]; a copy place has no
    /// hole to avoid.
    SwapOverCopyPlace {
        /// The exact place type the two arguments select.
        place_type: String,
        /// Exact restructuring required by OP-11.
        mechanical_fix: &'static str,
    },
    /// [STOR-8] a compilation unit carrying the no-heap declaration named
    /// `Box` or a runtime-capacity shape, or called an allocating prelude
    /// row.
    HeapTypeUnderNoHeap {
        /// The written spelling that names heap storage.
        spelling: String,
        /// Exact restructuring required by STOR-8.
        mechanical_fix: &'static str,
    },
    /// [WIN-3] a move out of a window slot or an array element, which has no
    /// take operation and leaves no hole.
    InvalidElementMove {
        /// Exact restructuring required by WIN-3.
        mechanical_fix: &'static str,
    },
    /// [OWN-1] a `move` of a place reached through a `deref`, which is not
    /// rooted in a live own-mode binding of this function, so it is not one
    /// of the consumes that rule admits.
    MoveThroughReference {
        /// Exact restructuring required by OWN-1.
        mechanical_fix: &'static str,
    },
    /// [TYPE-10] one of the four window-part spellings was written in a
    /// position the rule refuses: a read, a `borrow_expr` or a write of a
    /// window part of the place it follows.
    ///
    /// x1 narrows this to the parts. The measure spellings are the readonly
    /// fields [PRE-1] declares on the storage shapes, so a write of one is
    /// [`SemanticIssueKind::ReadonlyWriteTarget`], and neither set of
    /// spellings is reserved from a declaration any more: a suffix carries
    /// one of these meanings only where the type of the place it follows
    /// gives it one.
    ReservedPseudoField {
        /// The reserved spelling as it was written.
        spelling: String,
        /// Exact restructuring required by TYPE-10.
        mechanical_fix: &'static str,
    },
    /// [TYPE-2] a path that ends at or passes through a readonly field was
    /// written as a write target: a `set` target, or an argument at a
    /// reference parameter whose callee row writes that parameter.
    ReadonlyWriteTarget {
        /// The readonly field's spelling as the declaration writes it.
        spelling: String,
        /// Exact restructuring required by TYPE-2.
        mechanical_fix: &'static str,
    },
    /// `move` was written for a copy value.
    MoveOfCopy {
        /// Exact mechanical repair required by OWN-1.
        mechanical_fix: &'static str,
    },
    /// An affine value was used without its required consuming spelling.
    BareAffineUse {
        /// Exact mechanical repair required by OWN-1.
        mechanical_fix: &'static str,
    },
    /// [TYPE-2] a constructor `call` or a destructuring `let_stmt` named an
    /// opaque struct, whose constructor entry exists to be refused.
    ContainerConstruction {
        /// The nominal the construct named.
        nominal: String,
        /// The repair [DIAG-1], chosen by where the struct comes from and,
        /// for a cell taken apart, by its content.
        mechanical_fix: String,
    },
    /// A binding was used after ownership had already been consumed.
    UseAfterMove {
        /// Exact restructuring required by OWN-1.
        mechanical_fix: &'static str,
    },
    /// [REF-2] a reference was used after the event that invalidated it.
    ///
    /// The five v0.59 loan-conflict payloads this variant supersedes —
    /// `InvalidBorrowLifetime`, `BorrowConflict`, `InvalidChildReborrow`,
    /// `InvalidReborrowPosition` and `AmbiguousResultProvenance` — named a
    /// region, a loan strength, a parent holder and a provenance candidate.
    /// None of the four exists: [REF-1] gives a reference a path and nothing
    /// else, and [REF-2] makes the only reference-use rejection the use of an
    /// invalid one, carrying the invalidating event.
    InvalidReferenceUse {
        /// The reference binding, exactly as the source spells it.
        binder: String,
        /// The invalidating event [REF-2] names.
        event: &'static str,
        /// Exact restructuring required by REF-2.
        mechanical_fix: &'static str,
    },
    /// [REF-1] `&p` where `p` is a reference variable, which is not storage
    /// of its own.
    ReferenceToReferenceVariable {
        /// Exact restructuring required by REF-1.
        mechanical_fix: &'static str,
    },
    /// [REF-3] a reference was assigned into an aggregate, returned, or
    /// captured by a stored function value.
    EscapingReference {
        /// Exact restructuring required by REF-3.
        mechanical_fix: &'static str,
    },
    /// [REF-4] a range reference was formed over a `Ring`, whose wrapped
    /// window is two extents while `&[T]` has one `len`.
    RangeOverRing {
        /// Exact restructuring required by REF-4.
        mechanical_fix: &'static str,
    },
    /// [WIN-3] a move out of a window slot or an array element.
    MoveOutOfSlot {
        /// Exact restructuring required by WIN-3.
        mechanical_fix: &'static str,
    },
    /// A borrow holder was used without the required explicit dereference.
    MissingDereference {
        /// Exact mechanical repair selected by TYPE-7.
        mechanical_fix: &'static str,
    },
    /// [OWN-11] a loop body left a binding declared outside it in a
    /// different live-or-dead status than the entering edge did, so one
    /// iteration would start in a state the previous one did not leave.
    MoveOuterBindingInLoop {
        /// The outer binding whose status the backedge changed.
        binding: String,
        /// Exact restructuring required by OWN-11.
        mechanical_fix: &'static str,
    },
    /// The selected operation family has no row for the written arguments.
    InvalidOperation,
    /// A contract predicate is not exactly `own Bool`.
    InvalidPredicateCondition,
    /// A conditional was written in a form GRAM-6 does not admit for its
    /// class: a Bool-scrutinee `match`, an empty `else`, or an `else` block
    /// holding exactly one `if`.
    InvalidConditionalForm {
        /// Exact mechanical repair selected by GRAM-6.
        mechanical_fix: &'static str,
    },
    /// A subscript's bounds obligation is not derivable from the closed fact
    /// state at its node [OP-4, ENT-6].
    UndischargedBoundsObligation {
        /// The exact ENT-6 residual rendering: offset atom, ` < `, base
        /// place, `.len`.
        residual: String,
        /// The exact non-discharged disposition [MSR-4].
        disposition: StaticObligationDisposition,
        /// The repair [DIAG-1], selected by the disposition and by what the
        /// residual's terms are.
        mechanical_fix: String,
    },
    /// A release selected the empty-run graph but the current facts do not
    /// prove that the run has no initialized elements [PROV-6, ENT-6].
    UndischargedEmptyRunRelease {
        /// The exact remaining relation, `len_of(P) <= 0_u64`.
        residual: String,
        /// The exact non-discharged disposition [OP-14].
        disposition: StaticObligationDisposition,
        /// The repair [DIAG-1], selected by the disposition.
        mechanical_fix: String,
    },
    /// One proof-required exact integer operation's canonical `.defined`
    /// goal is not derivable from the closed fact state [OP-2, ENT-6].
    UndischargedIntegerDomainObligation {
        /// The exact canonical `.defined` predicate for this occurrence.
        residual: String,
        /// The exact non-discharged disposition.
        disposition: StaticObligationDisposition,
        /// The repair [DIAG-1], selected by the disposition and by what the
        /// goal's terms are.
        mechanical_fix: String,
    },
    /// One exact numeric conversion lacks its OP-6 domain proof.
    UndischargedConversionDomainObligation {
        residual: String,
        disposition: StaticObligationDisposition,
        mechanical_fix: String,
    },
    /// A runtime-sized buffer allocation lacks an OP-9 fit proof.
    UndischargedAllocationFitObligation {
        residual: String,
        disposition: StaticObligationDisposition,
        mechanical_fix: String,
    },
    /// One range-reference formation conjunct — `lo <= hi` or `hi <= x.len`
    /// — lacks a [REF-4] proof.
    UndischargedRangeFormationObligation {
        residual: String,
        disposition: StaticObligationDisposition,
        mechanical_fix: String,
    },
    /// Two compared index or range steps have no source proof of disjointness
    /// [OWN-7, EFF-5]. The residual names the exact position family.
    UndischargedCallSeparation {
        residual: String,
        mechanical_fix: &'static str,
    },
    /// Two effects of one call's substituted row lie on overlapping paths
    /// and at least one is a write, with no admitted family able to separate
    /// them [EFF-5].
    OverlappingCallEffects {
        first: String,
        second: String,
        mechanical_fix: &'static str,
    },
    /// The row of the callee of an [OP-12] atomic in-place update reaches a
    /// prefix of the updated place: it writes it, moves out of it, or frees
    /// it, any of which would make the old value's transfer and the result's
    /// commit two observable steps rather than one.
    AtomicUpdateReachesTargetPrefix {
        /// The place being updated, as the caller spells it.
        target: String,
        /// The substituted row path that reaches a prefix of it.
        effect: String,
        mechanical_fix: &'static str,
    },
    /// The complete instantiated requirement at an ordinary call is refuted
    /// or unproved in the caller's pre-transfer state [FN-8].
    UndischargedCallRequirement(Box<UndischargedCallRequirementDetail>),
    /// A counted endpoint produced `own u64` but was not itself one preceding
    /// ENT-2 term or constant.
    InvalidCountedEndpoint {
        /// The exact restructuring required by ENT-2.
        mechanical_fix: &'static str,
    },
    /// An unlabeled break has no enclosing structural loop target [GRAM-4,
    /// FN-1].
    BreakOutsideLoop {
        /// The exact source-level restructuring required by GRAM-4.
        mechanical_fix: &'static str,
    },
    /// A header or local invariant violates INV-1 name or target formation.
    InvalidInvariant {
        reason: &'static str,
        mechanical_fix: &'static str,
    },
    /// A well-formed source loop invariant failed one of INV-1's two mandatory
    /// induction judgments in the source fact context.
    UndischargedLoopInvariant {
        /// Source spelling of the invariant name.
        name: String,
        /// The failed induction obligation, selected in proof order.
        obligation: LoopInvariantProofObligation,
        /// The exact source-language relation the failed incoming edge had to
        /// establish. A counted-loop backedge renders the hidden next binder
        /// as `i + 1_u64`; no checker-private term identity is exposed.
        required_relation: String,
        /// The failed judgment's disposition [MSR-4].
        disposition: StaticObligationDisposition,
        /// The repair [DIAG-1], selected by the obligation and disposition.
        mechanical_fix: String,
    },
    /// A well-formed blockless local invariant target is not established by
    /// the specification-defined AUTO family in its entering context.
    UndischargedLocalInvariant {
        /// Source spelling of the invariant name.
        name: String,
        /// The target's disposition [MSR-4].
        disposition: StaticObligationDisposition,
        /// The repair [DIAG-1], selected by the disposition.
        mechanical_fix: String,
    },
    /// A `proof_use` relation or certificate factor violates the closed
    /// PRF-1 source form.
    InvalidSourceProof {
        reason: &'static str,
        mechanical_fix: &'static str,
    },
    /// A well-formed local invariant failed in the source fact context.
    /// Written uses are independent entering-context premises; their explicit
    /// weighted sum may establish a target weakened by the fixed direct rule.
    UndischargedSourceProof {
        name: String,
        obligation: SourceProofObligation,
        mechanical_fix: &'static str,
    },
    /// A return expression disagrees with the written function result.
    ReturnMismatch,
    /// A call-site tail-transfer guarantee failed its named condition.
    InvalidMusttail {
        /// The condition the marked call must satisfy.
        condition: &'static str,
        /// The offending argument or still-live owner, when applicable.
        subject: Option<String>,
    },
    /// A call on a cycle among generic functions instantiates its callee at
    /// something other than exactly the caller's own type parameters [FN-6].
    PolymorphicRecursion {
        /// The cycle FN-6 requires the diagnostic to name: the function
        /// spellings along the shortest cycle through this call, in call
        /// order, joined by ` -> ` and closed on the caller.
        cycle: String,
        /// Required FN-6 restructuring.
        mechanical_fix: &'static str,
    },
    /// A statement follows a structurally terminating statement.
    UnreachableStatement,
    /// The function body can reach its closing brace.
    FunctionFallthrough,
    /// A requirement entry uses a construct outside the admitted FN-8 goal subset.
    InvalidRequires,
    /// An ensures selector does not match the concrete result class FN-9 admits.
    InvalidPostconditionSelector,
    /// [CALL-4] a route omits its ordinal binder where two or more declared
    /// result ordinals could carry it.
    AmbiguousResultRoute {
        /// The repair [DIAG-1], naming the results that could carry the
        /// route.
        mechanical_fix: String,
    },
    /// A variant selector does not spell exact `Ok(value: result)`.
    InvalidPostconditionFields {
        /// Exact closed field list required by the admitted selector.
        required_fields: Vec<String>,
    },
    /// The symbolic result candidate conflicts with a live declaration.
    PostconditionCandidateNotFresh {
        /// Written candidate spelling.
        spelling: String,
        /// Ordered live declaration origins that conflict with the candidate.
        conflicts: Vec<crate::SourceOrigin>,
    },
    /// A later ensures-local declaration attempts to shadow the symbolic result.
    PostconditionLocalShadowsResult {
        /// Written candidate spelling.
        spelling: String,
        /// The admitted selector candidate's exact origin.
        selector: crate::SourceOrigin,
    },
    /// An ensures entry uses a construct outside FN-9's proof-only ANF subset.
    InvalidPostconditionClause,
    /// The alpha-expanded final condition is not one output-bearing L0 relation.
    InvalidPostconditionRelation,
    /// [MSR-3] entry is proof-only and directly names an exclusive formal.
    InvalidEntryFormer {
        /// The restructuring this occurrence needs.
        mechanical_fix: &'static str,
    },
    /// [CALL-6] the relations one contract publishes are contradictory at
    /// their establishment point, so every goal a caller submits would
    /// discharge from that contradiction alone.
    ContradictoryPublishedRelations {
        /// The two clause relations whose conjunction is unsatisfiable, as
        /// rendered from the declared templates.
        relations: Vec<String>,
        /// The restructuring this contract needs.
        mechanical_fix: &'static str,
    },
    /// A selected Result exit is not a direct canonical `Ok(value: atom)` or `Err(error: atom)`.
    InvalidPostconditionReturn,
    /// One concrete postcondition has no selected normal exit.
    NoSelectedNormalExit {
        /// The exact fixed residual required by FN-9.
        residual: &'static str,
        /// The repair [DIAG-1].
        mechanical_fix: &'static str,
    },
    /// A selected normal return's complete instantiated FN-9 relation is
    /// refuted or unproved after entry-image stability and ordinary kills.
    UndischargedPostcondition(Box<UndischargedPostconditionDetail>),
    /// Named user-call arguments differ from the parameter list.
    InvalidNamedArguments {
        /// Callee spelling at the call site.
        callee: String,
        /// Exact declared parameter names in their required order.
        declared_parameters: Vec<String>,
    },
    /// Two fields in one owner-local table have the same label.
    DuplicateFieldLabel {
        /// Repeated field label.
        label: String,
    },
    /// Construction fields differ from the constructor's declared table.
    InvalidConstructionFields {
        /// Constructor named at the failing site.
        constructor: String,
        /// Exact declared field labels in their required order.
        declared_fields: Vec<String>,
    },
    /// Match binders differ from the variant's declared field table.
    InvalidMatchFields {
        /// Variant named by the arm.
        variant: String,
        /// Exact declared field labels in their required order.
        declared_fields: Vec<String>,
    },
    /// A match arm names a variant belonging to a different enum.
    ForeignMatchVariant,
    /// A match omits one or more declared variants.
    NonExhaustiveMatch {
        /// Declared variants with no arm, in declaration order.
        missing_variants: Vec<String>,
    },
    /// A propagation operand or enclosing result has the wrong Result shape.
    InvalidPropagation,
    /// `give` is absent, misplaced, duplicated, or followed by a statement.
    InvalidGive,
    /// [GIVE-1] every arm or branch of a value initializer leaves by `return`
    /// or `break`, so its delivery set is empty and no value reaches the
    /// binding.
    EmptyDeliverySet {
        /// The binding no value reaches, as the source spells it.
        binding: String,
        /// The repair [DIAG-1]: the statement form, binding dropped.
        mechanical_fix: String,
    },
    /// The effect row is not a valid exact EFF-1 row.
    InvalidEffectRow {
        /// Which EFF-1 condition this row failed.
        reason: &'static str,
        /// Exact repair required by EFF-1 for that condition.
        mechanical_fix: &'static str,
    },
    /// A row carries an entry that another of its entries covers, such as
    /// `reads(p)` or `writes(p.x)` beside `writes(p)`, or `reads(p.x)` beside
    /// `reads(p)`, which EFF-1 never writes because the covering entry
    /// already states it. EFF-1 names no restructuring for it, so the
    /// rejection carries none.
    SubsumedEffectEntry {
        /// The redundant entry as the row writes it.
        entry: String,
        /// The first entry in written order whose path covers it: a `writes`
        /// entry, or a `reads` entry covering a `reads` entry below it.
        covering: String,
    },
    /// The written effect row differs from syntactically exhibited effects.
    EffectMismatch {
        /// The exhibited row without the entries another of its entries
        /// covers, in EFF-1 canonical spelling: EFF-2 admits it for the body,
        /// EFF-1 admits it as written, and every entry is an exhibited path.
        expected_row: String,
        /// The row the declaration writes, in the same spelling.
        found_row: String,
        /// The entries of `expected_row` that cover an exhibited access the
        /// declaration does not cover.
        missing: Vec<String>,
        /// Declared categories and paths the body does not exhibit.
        extra: Vec<String>,
        /// The repair [DIAG-1]: declare `expected_row`.
        mechanical_fix: String,
    },
    /// A generic type parameter named a source contract as its bound.
    SourceContractGenericBound,
}

/// A written-argument count and the noun it agrees with, as `1 written type
/// argument` or `2 written type arguments`.
///
/// A diagnostic a writer reads is prose, and "1 written region arguments" is
/// the kind of sentence that makes a reader doubt the rest of it.
pub(crate) fn written_count(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("{count} written {noun}")
    } else {
        format!("{count} written {noun}s")
    }
}

impl SemanticIssueKind {
    /// One [TYPE-5] disagreement, in the spellings the source uses.
    ///
    /// The rejection published neither side for four blind-writer rounds: a
    /// writer was told two types disagree and had to work out which two. Both
    /// are always in hand at the judgment, so both are published. Where the
    /// disagreement is about the written form rather than two types — a
    /// generic form written with no type arguments, a `move` where a place is
    /// required — each side states that form.
    #[must_use]
    pub(crate) fn type_mismatch(expected: impl Into<String>, found: impl Into<String>) -> Self {
        Self::TypeMismatch {
            expected: expected.into(),
            found: found.into(),
        }
    }
}

/// One deterministic post-resolution source-language rejection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticIssue {
    pub(crate) rule: SemanticRule,
    pub(crate) location: SemanticLocation,
    pub(crate) kind: SemanticIssueKind,
    /// The call that requested the concrete generic instance whose check
    /// produced this rejection, when one did. The location stays at the
    /// template's source, which owns the failure, and this names the
    /// requester [FN-2, MOD-8].
    pub(crate) request: Option<crate::SyntaxCoordinate>,
}

impl SemanticIssue {
    /// Returns the exact numbered rule established by this rejection.
    #[must_use]
    pub const fn rule_id(&self) -> &'static str {
        self.rule.id()
    }

    /// Returns the exact numbered rule established by this issue.
    #[must_use]
    #[cfg(test)]
    pub const fn rule(&self) -> SemanticRule {
        self.rule
    }

    /// Returns the exact DIAG-1 semantic location.
    ///
    /// The driver reads it to quote the offending source line, so a semantic
    /// rejection names a file and a line rather than a `SourceId` and a byte.
    #[must_use]
    pub const fn location(&self) -> &SemanticLocation {
        &self.location
    }

    /// Returns the structured rejection premise.
    #[must_use]
    #[cfg(test)]
    pub const fn kind(&self) -> &SemanticIssueKind {
        &self.kind
    }
}

/// A language family that the current compiler has not implemented yet.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnsupportedSemanticFeature {
    /// Type, const, or region polymorphism.
    Generics,
    /// Nongeneric PRE-1 enum types and constructors outside Bool.
    PreludeNominalValues,
    /// A `borrow_expr` whose root is a place form the checker does not yet
    /// resolve to a [REF-1] path. It names no rule: a reference is a local
    /// name for a path and every admitted root has one, so reaching here is
    /// a checker gap and never a source verdict [OWN-8].
    ReferenceFormation,
    /// Composite types or values outside the implemented nominal-data family.
    CompositeValues,
    /// A recursive nominal layout whose finite representation is not selected.
    RecursiveNominalLayout,
    /// An ownership-state join not yet covered by the selected finite rule.
    OwnershipJoin,
    /// Repeated match arms, whose meaning the active specification does not select.
    DuplicateMatchArm,
    /// An OP-1 family outside the implemented scalar and nominal-tag families.
    OperationFamily,
}

/// Exact source node at which an unimplemented compiler family was required.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticUnsupported {
    pub(crate) feature: UnsupportedSemanticFeature,
    pub(crate) node: SemanticLocation,
}

impl SemanticUnsupported {
    /// Returns the unimplemented semantic family.
    #[must_use]
    #[cfg(test)]
    pub const fn feature(&self) -> UnsupportedSemanticFeature {
        self.feature
    }
}

/// Trusted semantic-checker invariant failure, never a source verdict.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SemanticCompilerFailure {
    /// Canonical production topology had an impossible local shape.
    InvalidCanonicalTree,
    /// A resolved declaration or use record was missing or inconsistent.
    InvalidResolution,
    /// Exact source bytes were not representable by the required semantic form.
    InvalidSourceEncoding,
    /// A dense identity or source-coordinate calculation overflowed.
    CounterOverflow,
    /// A function's obligation records and the entailment engine's judgments
    /// disagreed: a record no judgment answered, with no answered record
    /// undischarged, or a judgment that answered no record. The function is
    /// not accepted (`design/compiler/acceptance-records.md`).
    ObligationContract,
}

/// Whole-unit semantic success and its only lowering authority.
///
/// It holds what checking concluded and no syntax: a stage that reads
/// resolution records, such as an entry's composition judgment, reads them
/// from the resolved unit the check was made over.
#[derive(Debug)]
pub struct CheckedProgram {
    pub(crate) data: CheckedProgramData,
}

impl CheckedProgram {
    /// [ENT-4] every judgment in the named functions that succeeded only
    /// because the state it was asked in is contradictory, so that a test can
    /// show a repaired program succeeds where its construct runs [DIAG-1].
    #[cfg(test)]
    pub(crate) fn contradictory_successes(&self, functions: &[String]) -> Vec<String> {
        let mut found = Vec::new();
        for function in self
            .data
            .functions
            .iter()
            .filter(|function| functions.contains(&function.name))
        {
            let summary = &function.entailment;
            if matches!(
                summary.body_disposition,
                CheckedBodyDisposition::Uninhabited { .. }
            ) {
                found.push(format!("the body of `{}`", function.name));
            }
            for obligation in &summary.obligations {
                if obligation.discharged && obligation.contradictory {
                    found.push(format!(
                        "a {:?} obligation in `{}`",
                        obligation.family, function.name
                    ));
                }
            }
            for call in &summary.call_goals {
                if call.disposition == entailment::CallGoalDisposition::Discharged
                    && call
                        .evidence
                        .contains(&entailment::CallGoalEvidence::AllDerivable)
                {
                    found.push(format!(
                        "the call requirement `{}` in `{}`",
                        call.rendered_goal, function.name
                    ));
                }
            }
        }
        found
    }

    #[cfg(test)]
    pub(crate) fn element_type(&self, element: CheckedElement) -> Option<CheckedType> {
        self.data.elements.get(element.0 as usize).copied()
    }

    /// Returns the number of checked source functions.
    #[must_use]
    #[cfg(test)]
    pub fn function_count(&self) -> usize {
        self.data
            .functions
            .iter()
            .filter(|function| function.body.is_some())
            .count()
    }

    /// Returns main's ordinary source spelling when the test declares it.
    #[must_use]
    #[cfg(test)]
    pub fn entry_function_name(&self) -> &str {
        self.data
            .functions
            .iter()
            .find(|function| function.name == "main")
            .map_or("", |function| function.name.as_str())
    }
}

/// Failure-atomic result of target-independent semantic checking.
#[derive(Debug)]
pub enum SemanticOutcome {
    /// Every applicable whole-unit judgment succeeded.
    Complete(Box<CheckedProgram>),
    /// A numbered language rule was violated.
    SourceIssue {
        /// Deterministically selected semantic issue.
        issue: SemanticIssue,
    },
    /// A delayed ensures-entry resolver issue selected only after FN-9 selector admission.
    ResolutionIssue {
        /// The original resolution issue, unchanged in rule, location, and payload.
        issue: ResolutionIssue,
    },
    /// Valid source requires a language family the compiler has not implemented.
    Unsupported {
        /// Exact unimplemented family and source node.
        unsupported: SemanticUnsupported,
    },
    /// Trusted compiler invariants failed.
    CompilerFailure {
        /// Internal failure class.
        failure: SemanticCompilerFailure,
    },
}

enum CheckStop {
    Issue(Box<SemanticIssue>),
    Resolution(Box<ResolutionIssue>),
    Unsupported(SemanticUnsupported),
    Compiler(SemanticCompilerFailure),
    /// A derived type named a nominal instance that is not interned yet.
    ///
    /// Function checking is `&self`, and every interning site reads a
    /// *written* type — a `box<T>` for [STOR-2], a `Result<T, E>` for the
    /// checked arithmetic rows. A derived type has no written form anywhere,
    /// so once the annotation is gone nothing interns it. This is the
    /// recoverable signal that closes that gap: the driver interns what is
    /// pending and checks the function again. It is private to the checker
    /// and never reaches a diagnostic.
    DeferredNominal,
    /// A finite loop-header path summary grew. Retry the ordinary typed
    /// walk; no partial checked body or obligations are published.
    ReferenceSummaryChanged,
    /// A throwaway FN-9 selector dependency whose ordinary source premise did
    /// not succeed. It must be consumed inside preflight and never becomes a
    /// source or compiler diagnostic of its own.
    PostconditionPrerequisiteUnavailable,
}

impl CheckStop {
    fn source_issue(issue: SemanticIssue) -> Self {
        Self::Issue(Box::new(issue))
    }
}

impl From<SemanticCompilerFailure> for CheckStop {
    fn from(value: SemanticCompilerFailure) -> Self {
        Self::Compiler(value)
    }
}
