//! Checked range clauses: bounded quantified facts over storage contents
//! [RANGE-1], their instantiation [RANGE-4] and a counted loop's
//! cross-iteration certificate [RANGE-5].
//!
//! This module holds the checked forms only. The resolver gives a clause its
//! name, its bound variables and its relation names; the semantic checker
//! forms the terms below from that syntax; the range judgment carries the
//! resulting facts and judges every obligation they create, after the
//! entailment flow.

use std::collections::BTreeMap;

use crate::{DeclarationId, NodePath};

use super::model::{BindingId, CheckedLoopId, CheckedMeasure, CheckedType, IntegerType};

/// What a range term's place or value starts from.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum CheckedRangeRoot {
    /// A parameter or a binding live where the clause is written.
    Binding(BindingId),
    /// One result ordinal of the function, in a postcondition, zero for a
    /// single result; for the ordinal a route `when V(value: r)` names, the
    /// payload r denotes [RANGE-1, FN-9].
    Result(u32),
}

/// One storage place a range term reads: a root and the field, `Box`
/// content and dereference steps below it. Subscripts are never part of a
/// place; a read names its subscripts separately [RANGE-1].
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct CheckedRangePlace {
    pub(crate) root: CheckedRangeRoot,
    pub(crate) path: Vec<CheckedRangeStep>,
}

/// One step below a range place's root.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum CheckedRangeStep {
    /// The referent of a reference binding, `r^` [TYPE-7].
    Referent,
    /// One struct field by source ordinal.
    Field(u32),
    /// A `Box`'s content, `b.inner` [TYPE-9].
    BoxContent,
}

/// The value shape a range read selects from.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum CheckedRangeShape {
    /// An `Array`, a `Slots`, or the run a range reference names: one
    /// subscript selects an element.
    Run,
    /// A `Segments`: a segment subscript and then an element subscript.
    Segments,
}

/// One integer term of a range relation [RANGE-1].
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum CheckedRangeTerm {
    Constant(i128),
    /// The clause's bound variable at this position.
    Bound(u32),
    /// One of a cross-iteration certificate's two iterations [RANGE-5].
    Iteration(u32),
    /// The current value of an own integer binding.
    Value(CheckedRangeRoot),
    /// One measure of a place: `p.len` or `p.cap`. The shape is the measured
    /// value's, a `Segments` counting its segments.
    Measure {
        place: CheckedRangePlace,
        measure: CheckedMeasure,
        shape: CheckedRangeShape,
    },
    /// The length of one segment of a `Segments` place, `s[d].len`.
    SegmentLength {
        place: CheckedRangePlace,
        segment: Box<CheckedRangeTerm>,
    },
    /// One integer element read, `p[i]` or `s[d][k]`.
    Read {
        place: CheckedRangePlace,
        shape: CheckedRangeShape,
        indices: Vec<CheckedRangeTerm>,
        element: IntegerType,
    },
    /// `constant + sum(coefficient * term)`.
    Sum {
        terms: Vec<(i128, CheckedRangeTerm)>,
        constant: i128,
    },
}

/// One comparison of a range relation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum RangeComparison {
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

/// One written relation of a range clause.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedRangeRelation {
    pub(crate) node: NodePath,
    pub(crate) left: CheckedRangeTerm,
    pub(crate) comparison: RangeComparison,
    pub(crate) right: CheckedRangeTerm,
}

/// One bound variable's half-open range.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedRangeBinder {
    pub(crate) start: CheckedRangeTerm,
    pub(crate) end: CheckedRangeTerm,
}

/// One checked `forall NAME(binders) when guards: conclusions` [RANGE-1].
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedRangeClause {
    pub(crate) declaration: DeclarationId,
    pub(crate) name: String,
    pub(crate) node: NodePath,
    pub(crate) binders: Vec<CheckedRangeBinder>,
    pub(crate) guards: Vec<CheckedRangeRelation>,
    pub(crate) conclusions: Vec<CheckedRangeRelation>,
}

/// One `use NAME(arguments);` step of a certificate [RANGE-4].
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedRangeUse {
    pub(crate) node: NodePath,
    pub(crate) fact: DeclarationId,
    pub(crate) arguments: Vec<CheckedRangeTerm>,
}

/// A counted loop's cross-iteration certificate [RANGE-5].
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedApart {
    pub(crate) node: NodePath,
    pub(crate) uses: Vec<CheckedRangeUse>,
}

/// The range clauses one counted loop's header carries.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct CheckedRangeLoop {
    pub(crate) invariants: Vec<CheckedRangeClause>,
    pub(crate) apart: Option<CheckedApart>,
}

/// One counted loop whose certificate holds [RANGE-5]: every element write
/// one iteration makes to storage that outlives it selects an element no
/// other iteration reads or writes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedCertifiedLoop {
    pub(crate) id: CheckedLoopId,
    /// The loop's `for_stmt`.
    pub(crate) node: NodePath,
    /// The certified element writes: each `set` statement or call.
    pub(crate) writes: Vec<NodePath>,
    /// The certified element reads of the written storage, by carrier.
    pub(crate) reads: Vec<NodePath>,
}

/// One postcondition a caller takes as a range fact after a call [RANGE-2].
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedRangePostcondition {
    pub(crate) clause: CheckedRangeClause,
    /// The route `when V(value: r)` the clause is taken through; `None` for
    /// an unrouted clause [FN-9].
    pub(crate) route: Option<CheckedRangeRoute>,
    /// Each result ordinal's type in a function that writes an ordered
    /// result list, whose one value holds them as its fields [CALL-4]; empty
    /// for a single result.
    pub(crate) results: Vec<CheckedType>,
    /// Whether the range judgment owes it at the function's exits: a range
    /// postcondition is owed [RANGE-3], while an [FN-9] relation whose sides
    /// are range terms, a clause without bound variables, is ordinary
    /// entailment's to prove and only taken here.
    pub(crate) owed: bool,
}

/// The route of a range postcondition [FN-9].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CheckedRangeRoute {
    /// The result ordinal the route names.
    pub(crate) ordinal: u32,
    /// The tag of its success variant V.
    pub(crate) tag: u32,
    /// The type of V's payload, which [`CheckedRangeRoot::Result`] of that
    /// ordinal names.
    pub(crate) payload: CheckedType,
}

/// Every range clause of one function, and the certificates the range
/// judgment found to hold.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct CheckedRangeFacts {
    pub(crate) requirements: Vec<CheckedRangeClause>,
    pub(crate) postconditions: Vec<CheckedRangePostcondition>,
    pub(crate) loops: BTreeMap<CheckedLoopId, CheckedRangeLoop>,
    pub(crate) certified: Vec<CheckedCertifiedLoop>,
}

impl CheckedRangeFacts {
    /// Whether the function states no range clause the range judgment owes.
    pub(crate) fn is_empty(&self) -> bool {
        self.requirements.is_empty()
            && self.loops.is_empty()
            && !self.postconditions.iter().any(|post| post.owed)
    }

    /// Whether `declaration` names one of these range facts.
    pub(crate) fn declares(&self, declaration: DeclarationId) -> bool {
        self.requirements
            .iter()
            .chain(self.postconditions.iter().map(|post| &post.clause))
            .chain(
                self.loops
                    .values()
                    .flat_map(|entry| entry.invariants.iter()),
            )
            .any(|clause| clause.declaration == declaration)
    }
}

impl CheckedRangeTerm {
    /// Every place this term reads, its measures included.
    pub(crate) fn collect_places(&self, out: &mut Vec<CheckedRangePlace>) {
        match self {
            Self::Constant(_) | Self::Bound(_) | Self::Iteration(_) | Self::Value(_) => {}
            Self::Measure { place, .. } => out.push(place.clone()),
            Self::SegmentLength { place, segment } => {
                out.push(place.clone());
                segment.collect_places(out);
            }
            Self::Read { place, indices, .. } => {
                out.push(place.clone());
                for index in indices {
                    index.collect_places(out);
                }
            }
            Self::Sum { terms, .. } => {
                for (_, term) in terms {
                    term.collect_places(out);
                }
            }
        }
    }

    /// Every root whose current value this term reads.
    pub(crate) fn collect_values(&self, out: &mut Vec<CheckedRangeRoot>) {
        match self {
            Self::Constant(_) | Self::Bound(_) | Self::Iteration(_) => {}
            Self::Value(binding) => out.push(*binding),
            Self::Measure { .. } => {}
            Self::SegmentLength { segment, .. } => segment.collect_values(out),
            Self::Read { indices, .. } => {
                for index in indices {
                    index.collect_values(out);
                }
            }
            Self::Sum { terms, .. } => {
                for (_, term) in terms {
                    term.collect_values(out);
                }
            }
        }
    }
}

impl CheckedRangeClause {
    /// Every relation of the clause in written order: binder endpoints are
    /// not relations, guards come before conclusions.
    pub(crate) fn relations(&self) -> impl Iterator<Item = &CheckedRangeRelation> {
        self.guards.iter().chain(self.conclusions.iter())
    }

    /// Every place the clause reads.
    pub(crate) fn places(&self) -> Vec<CheckedRangePlace> {
        let mut out = Vec::new();
        for binder in &self.binders {
            binder.start.collect_places(&mut out);
            binder.end.collect_places(&mut out);
        }
        for relation in self.relations() {
            relation.left.collect_places(&mut out);
            relation.right.collect_places(&mut out);
        }
        out.sort();
        out.dedup();
        out
    }
}
