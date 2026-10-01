//! Checked range clauses: bounded quantified facts over storage contents
//! [RANGE-1], their instantiation [RANGE-4] and a counted loop's
//! cross-iteration certificate [RANGE-5].
//!
//! This module holds the checked forms only. The resolver gives a clause its
//! name, its bound variables and its relation names; the semantic checker
//! forms the terms below from that syntax; the entailment flow carries the
//! resulting facts and judges every obligation they create.

use std::collections::BTreeMap;

use crate::{DeclarationId, NodePath};

use super::model::{BindingId, CheckedLoopId, CheckedMeasure, IntegerType};

/// One storage place a range term reads: a binding and the field, `Box`
/// content and dereference steps below it. Subscripts are never part of a
/// place; a read names its subscripts separately [RANGE-1].
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct CheckedRangePlace {
    pub(crate) root: BindingId,
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
    Value(BindingId),
    /// One measure of a place: `p.len` or `p.cap`.
    Measure {
        place: CheckedRangePlace,
        measure: CheckedMeasure,
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

/// One range `ensures`, with its success route when it has one.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckedRangeEnsures {
    pub(crate) clause: CheckedRangeClause,
    pub(crate) routed: bool,
}

/// Every range clause of one function.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct CheckedRangeFacts {
    /// Parameter bindings in declaration order, for call substitution.
    pub(crate) parameters: Vec<BindingId>,
    pub(crate) requirements: Vec<CheckedRangeClause>,
    pub(crate) ensures: Vec<CheckedRangeEnsures>,
    pub(crate) loops: BTreeMap<CheckedLoopId, CheckedRangeLoop>,
}

impl CheckedRangeFacts {
    pub(crate) fn is_empty(&self) -> bool {
        self.requirements.is_empty() && self.ensures.is_empty() && self.loops.is_empty()
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

    /// Every binding whose current value this term reads.
    pub(crate) fn collect_values(&self, out: &mut Vec<BindingId>) {
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
