//! Concrete aggregate equality in generic range clauses [RANGE-1].

use super::*;
use crate::semantic::range_facts::MAX_RANGE_ATOMS;

/// A noninteger operand survives only until its enclosing comparison decides
/// whether to expand it. Arithmetic and all other uses leave the clause empty.
pub(super) struct RangeOperand {
    term: CheckedRangeTerm,
    aggregate: Option<CheckedType>,
}

impl From<CheckedRangeTerm> for RangeOperand {
    fn from(term: CheckedRangeTerm) -> Self {
        Self {
            term,
            aggregate: None,
        }
    }
}

impl RangeOperand {
    pub(super) fn typed(term: CheckedRangeTerm, ty: CheckedType, names: &RangeNames) -> Self {
        Self {
            term,
            aggregate: (names.generic == RangeGeneric::Instance
                && !matches!(ty, CheckedType::Integer(_)))
            .then_some(ty),
        }
    }

    pub(super) fn integer(self, names: &RangeNames) -> CheckedRangeTerm {
        if self.aggregate.is_some() {
            names.unformed.set(true);
        }
        self.term
    }
}

impl Checker<'_, '_> {
    pub(super) fn range_comparison_operands(
        &mut self,
        context: FunctionContext<'_, '_>,
        node: NodeId,
        comparison: RangeComparison,
        left: RangeOperand,
        right: RangeOperand,
        names: &RangeNames,
    ) -> Result<Vec<CheckedRangeRelation>, CheckStop> {
        let path = self.types.declarations.tree.path(node)?.clone();
        if comparison == RangeComparison::Equal
            && let (Some(ty), Some(other)) = (left.aggregate, right.aggregate)
            && ty == other
            && match ty {
                CheckedType::Array { .. } | CheckedType::Bool => true,
                CheckedType::Nominal(id) => matches!(
                    self.types.nominal(id)?.kind,
                    CheckedNominalKind::Struct { .. } | CheckedNominalKind::Enum { .. }
                ),
                _ => false,
            }
            && self.types.is_copy_type(context.check_context, ty)?
        {
            // A call made while checking a generic body can substitute an
            // Array whose length (or nested element type) is still symbolic.
            // Defer the whole clause, including any already concrete fields,
            // until the concrete instance can form every projection.
            if self.range_projection_count(ty)?.is_none() {
                names.unformed.set(true);
                return Ok(Vec::new());
            }
            let mut out = Vec::new();
            self.expand_range_equality(ty, left.term, right.term, &path, &mut out)?;
            return Ok(out);
        }
        Ok(vec![CheckedRangeRelation {
            node: path,
            left: left.integer(names),
            comparison,
            right: right.integer(names),
            projected: false,
        }])
    }

    /// Count only up to the existing structural allowance. In particular an
    /// Array of empty/noninteger-only structs has no integer projections,
    /// regardless of its length, and need not enumerate its elements. A
    /// symbolic descendant returns None for the whole expansion, including
    /// beyond the cap; memoization keeps repeated field types shared.
    fn range_projection_count(&self, ty: CheckedType) -> Result<Option<usize>, CheckStop> {
        self.count_range_projections(ty, &mut HashMap::new())
    }

    fn count_range_projections(
        &self,
        ty: CheckedType,
        counts: &mut HashMap<CheckedType, Option<usize>>,
    ) -> Result<Option<usize>, CheckStop> {
        if let Some(count) = counts.get(&ty) {
            return Ok(*count);
        }
        let ceiling = MAX_RANGE_ATOMS + 1;
        let count = match ty {
            CheckedType::Integer(_) | CheckedType::Bool => 1,
            CheckedType::Array { element, length } => {
                let Some(length) = length.value() else {
                    return Ok(None);
                };
                if length == 0 {
                    return Ok(Some(0));
                }
                let Some(element) =
                    self.count_range_projections(self.types.element_type(element)?, counts)?
                else {
                    return Ok(None);
                };
                element.saturating_mul(usize::try_from(length).unwrap_or(usize::MAX))
            }
            CheckedType::Nominal(id) => match &self.types.nominal(id)?.kind {
                CheckedNominalKind::Struct { fields } => {
                    let mut count = 0_usize;
                    for field in fields {
                        let Some(field) = self.count_range_projections(field.ty, counts)? else {
                            return Ok(None);
                        };
                        count = count.saturating_add(field).min(ceiling);
                    }
                    count
                }
                CheckedNominalKind::Enum { variants } => {
                    let mut count = 1_usize;
                    for field in variants.iter().flat_map(|variant| &variant.fields) {
                        let Some(field) = self.count_range_projections(field.ty, counts)? else {
                            return Ok(None);
                        };
                        count = count.saturating_add(field).min(ceiling);
                    }
                    count
                }
                _ => 0,
            },
            CheckedType::Generic(_) | CheckedType::GenericInt(_) | CheckedType::GenericFloat(_) => {
                return Ok(None);
            }
            _ => 0,
        };
        let count = Some(count.min(ceiling));
        counts.insert(ty, count);
        Ok(count)
    }

    fn expand_range_equality(
        &self,
        ty: CheckedType,
        left: CheckedRangeTerm,
        right: CheckedRangeTerm,
        node: &crate::NodePath,
        out: &mut Vec<CheckedRangeRelation>,
    ) -> Result<(), CheckStop> {
        // Distinct projections supply distinct read atoms when an instance
        // is formed. One beyond the atom ceiling is enough to report RANGE-3
        // there, without materializing an arbitrarily large fixed Array.
        // A template alone consumes neither atoms nor fact instances.
        if out.len() > MAX_RANGE_ATOMS {
            return Ok(());
        }
        match ty {
            CheckedType::Integer(integer) => {
                out.push(CheckedRangeRelation {
                    node: node.clone(),
                    left: projected(left, None, integer),
                    comparison: RangeComparison::Equal,
                    right: projected(right, None, integer),
                    projected: true,
                });
            }
            CheckedType::Bool => {
                let tag = CheckedRangeProjection::Tag(2);
                out.push(CheckedRangeRelation {
                    node: node.clone(),
                    left: projected(left, Some(tag), IntegerType::U64),
                    comparison: RangeComparison::Equal,
                    right: projected(right, Some(tag), IntegerType::U64),
                    projected: true,
                });
            }
            CheckedType::Array { element, length } => {
                let element = self.types.element_type(element)?;
                let length = length
                    .value()
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                if length == 0 || self.range_projection_count(element)? == Some(0) {
                    return Ok(());
                }
                for index in 0..length {
                    if out.len() > MAX_RANGE_ATOMS {
                        break;
                    }
                    self.expand_range_equality(
                        element,
                        indexed(left.clone(), index),
                        indexed(right.clone(), index),
                        node,
                        out,
                    )?;
                }
            }
            CheckedType::Nominal(nominal) => match &self.types.nominal(nominal)?.kind {
                CheckedNominalKind::Struct { fields } => {
                    for (index, field) in fields.iter().enumerate() {
                        let step = CheckedRangeProjection::Field(index as u32);
                        self.expand_range_equality(
                            field.ty,
                            projected(left.clone(), Some(step), IntegerType::U64),
                            projected(right.clone(), Some(step), IntegerType::U64),
                            node,
                            out,
                        )?;
                    }
                }
                CheckedNominalKind::Enum { variants } => {
                    let count = variants.len() as u32;
                    let tag = CheckedRangeProjection::Tag(count);
                    out.push(CheckedRangeRelation {
                        node: node.clone(),
                        left: projected(left.clone(), Some(tag), IntegerType::U64),
                        comparison: RangeComparison::Equal,
                        right: projected(right.clone(), Some(tag), IntegerType::U64),
                        projected: true,
                    });
                    for variant in variants {
                        for (index, field) in variant.fields.iter().enumerate() {
                            let step = CheckedRangeProjection::Payload {
                                variant: variant.tag,
                                field: index as u32,
                                variants: count,
                            };
                            self.expand_range_equality(
                                field.ty,
                                projected(left.clone(), Some(step), IntegerType::U64),
                                projected(right.clone(), Some(step), IntegerType::U64),
                                node,
                                out,
                            )?;
                        }
                    }
                }
                _ => {}
            },
            // Noninteger leaves have no integer projections. The caller has
            // already checked OWN-1 for the complete aggregate.
            _ => {}
        }
        Ok(())
    }
}

fn projected(
    term: CheckedRangeTerm,
    step: Option<CheckedRangeProjection>,
    integer: IntegerType,
) -> CheckedRangeTerm {
    let mut term = match term {
        CheckedRangeTerm::Value(root) => CheckedRangeTerm::ValueProjection {
            root,
            indices: Vec::new(),
            projection: Vec::new(),
            element: integer,
        },
        term => term,
    };
    match &mut term {
        CheckedRangeTerm::Read {
            projection,
            element,
            guarded_from,
            ..
        } => {
            guarded_from.get_or_insert(projection.len());
            projection.extend(step);
            *element = integer;
        }
        CheckedRangeTerm::ValueProjection {
            projection,
            element,
            ..
        } => {
            projection.extend(step);
            *element = integer;
        }
        _ => unreachable!("an aggregate operand is a value or an element"),
    }
    term
}

fn indexed(term: CheckedRangeTerm, index: u64) -> CheckedRangeTerm {
    let mut term = projected(term, None, IntegerType::U64);
    match &mut term {
        CheckedRangeTerm::Read {
            indices,
            projection,
            implicit_indices,
            ..
        } => {
            let position = indices.len() as u32;
            projection.push(CheckedRangeProjection::Index(position));
            implicit_indices.push(position);
            indices.push(CheckedRangeTerm::Constant(i128::from(index)));
        }
        CheckedRangeTerm::ValueProjection {
            indices,
            projection,
            ..
        } => {
            projection.push(CheckedRangeProjection::Index(indices.len() as u32));
            indices.push(CheckedRangeTerm::Constant(i128::from(index)));
        }
        _ => unreachable!("an aggregate operand is a value or an element"),
    }
    term
}
