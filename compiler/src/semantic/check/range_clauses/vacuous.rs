//! A conclusion whose reads need two variants of one element [RANGE-1].

use std::collections::{BTreeMap, BTreeSet};

use super::*;

/// The element whose variant a requirement names: the base place or value,
/// the index terms that select it, and the steps below the index terms that
/// reach the enum, all as formed, so only syntactically identical elements
/// share a key.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum ElementKey {
    Place {
        place: CheckedRangePlace,
        shape: CheckedRangeShape,
        indices: Vec<CheckedRangeTerm>,
        path: Vec<CheckedRangeProjection>,
    },
    Value {
        root: CheckedRangeRoot,
        indices: Vec<CheckedRangeTerm>,
        path: Vec<CheckedRangeProjection>,
    },
}

/// The variants one conclusion's instance needs of each element, and those
/// it excludes [RANGE-1].
#[derive(Clone, Default)]
struct VariantRequirements {
    held: BTreeMap<ElementKey, BTreeSet<u32>>,
    excluded: BTreeMap<ElementKey, BTreeSet<u32>>,
}

impl VariantRequirements {
    /// A guard's reads, and the variant a guard `e is V` or `e is not V`
    /// requires or excludes.
    fn guard(&mut self, relation: &CheckedRangeRelation) {
        self.read(&relation.left);
        self.read(&relation.right);
        let CheckedRangeTerm::Constant(tag) = relation.right else {
            return;
        };
        if relation.projected {
            return;
        }
        let Some(key) = Self::key(&relation.left, None) else {
            return;
        };
        let Ok(tag) = u32::try_from(tag) else {
            return;
        };
        match relation.comparison {
            RangeComparison::Equal => {
                self.held.entry(key).or_default().insert(tag);
            }
            RangeComparison::NotEqual => {
                self.excluded.entry(key).or_default().insert(tag);
            }
            _ => {}
        }
    }

    /// The key of the enum whose payload step is at `at`, or, without `at`,
    /// whose tag the term's last step reads.
    fn key(term: &CheckedRangeTerm, at: Option<usize>) -> Option<ElementKey> {
        let (projection, base) = match term {
            CheckedRangeTerm::Read {
                projection, shape, ..
            } => (
                projection,
                match shape {
                    CheckedRangeShape::Run => 1,
                    CheckedRangeShape::Segments => 2,
                },
            ),
            CheckedRangeTerm::ValueProjection { projection, .. } => (projection, 0),
            _ => return None,
        };
        let at = match at {
            Some(at) => at,
            None => match projection.last() {
                Some(CheckedRangeProjection::Tag(_)) => projection.len() - 1,
                _ => return None,
            },
        };
        let path = projection[..at].to_vec();
        let subscripts = path
            .iter()
            .filter(|step| matches!(step, CheckedRangeProjection::Index(_)))
            .count();
        match term {
            CheckedRangeTerm::Read {
                place,
                shape,
                indices,
                ..
            } => Some(ElementKey::Place {
                place: place.clone(),
                shape: *shape,
                indices: indices[..indices.len().min(base + subscripts)].to_vec(),
                path,
            }),
            CheckedRangeTerm::ValueProjection { root, indices, .. } => Some(ElementKey::Value {
                root: *root,
                indices: indices[..indices.len().min(subscripts)].to_vec(),
                path,
            }),
            _ => None,
        }
    }

    /// The variant each payload step the clause's text wrote in a read
    /// needs. The steps an aggregate equality introduced hold the parts of
    /// that equality apart, one relation each, and a compiler-formed value
    /// projection writes none, so neither is a requirement of the instance.
    fn read(&mut self, term: &CheckedRangeTerm) {
        match term {
            CheckedRangeTerm::Read {
                indices,
                projection,
                guarded_from,
                ..
            } => {
                for index in indices {
                    self.read(index);
                }
                let written = guarded_from.unwrap_or(projection.len());
                for (at, step) in projection[..written].iter().enumerate() {
                    if let CheckedRangeProjection::Payload { variant, .. } = step
                        && let Some(key) = Self::key(term, Some(at))
                    {
                        self.held.entry(key).or_default().insert(*variant);
                    }
                }
            }
            CheckedRangeTerm::SegmentLength { segment, .. } => self.read(segment),
            CheckedRangeTerm::Sum { terms, .. } => {
                for (_, part) in terms {
                    self.read(part);
                }
            }
            _ => {}
        }
    }

    /// Whether some element must hold two variants, or hold a variant and
    /// not hold it.
    fn conflicts(&self) -> bool {
        self.held.iter().any(|(key, held)| {
            held.len() > 1
                || self
                    .excluded
                    .get(key)
                    .is_some_and(|excluded| !excluded.is_disjoint(held))
        })
    }
}

impl Checker<'_, '_> {
    /// [RANGE-1] a conclusion whose instance needs two variants of one
    /// element, or a variant its guards exclude, is defined at no element
    /// and states nothing: a hard error at the conclusion.
    pub(super) fn reject_vacuous_conclusions(
        &self,
        guards: &[CheckedRangeRelation],
        conclusions: &[CheckedRangeRelation],
    ) -> Result<(), CheckStop> {
        let mut guarded = VariantRequirements::default();
        for guard in guards {
            guarded.guard(guard);
        }
        for conclusion in conclusions {
            let mut instance = guarded.clone();
            instance.read(&conclusion.left);
            instance.read(&conclusion.right);
            if instance.conflicts() {
                let node = self
                    .types
                    .declarations
                    .tree
                    .node_with_path(&conclusion.node)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                return self.invalid_range(
                    SemanticRule::Range1,
                    node,
                    "a conclusion needs two variants of one element, or a variant its guards exclude, so no element defines it",
                    "read one variant of an element in a conclusion with its guards, and state each other variant in a conclusion of its own",
                );
            }
        }
        Ok(())
    }
}
