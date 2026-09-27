//! OP-6 domain normalization. All queries use the ordinary signed-goal and
//! numeric proof entry; constant answers inspect exact source bits.

use super::*;

fn domain_parts(
    expression: &GoalExpression,
) -> Option<(CheckedNumericType, CheckedNumericType, &GoalExpression)> {
    let GoalExpression::Operation {
        row:
            GoalOperation::NumericConversion {
                mode: CheckedConversionMode::Defined,
                source,
                destination,
            },
        arguments,
        result: CheckedType::Bool,
        ..
    } = expression
    else {
        return None;
    };
    let [operand] = arguments.as_slice() else {
        return None;
    };
    Some((*source, *destination, operand))
}

/// The initial automatic range family is sufficient in both cases. The
/// integer-to-integer interval is the exact mathematical domain, but this
/// release deliberately derives its negative only from a signed goal or an
/// exact constant answer, just like the other conversion rows.
fn domain_interval(
    source: CheckedNumericType,
    destination: CheckedNumericType,
) -> Option<(i128, i128)> {
    let CheckedNumericType::Integer(_) = source else {
        return None;
    };
    match destination {
        CheckedNumericType::Integer(integer) => Some(type_range(integer)),
        CheckedNumericType::Float(float) => {
            let precision = match float {
                FloatType::F32 => 24,
                FloatType::F64 => 53,
            };
            let limit = 1_i128 << precision;
            Some((-limit, limit))
        }
        CheckedNumericType::GenericInteger(_) | CheckedNumericType::GenericFloat(_) => None,
    }
}

fn bound_components(operand: Option<TermId>, minimum: i128, maximum: i128) -> Vec<BoundsRequest> {
    vec![
        BoundsRequest {
            left: operand,
            right: ZERO,
            bound: maximum,
            distinct: false,
        },
        BoundsRequest {
            left: operand.map(|_| ZERO),
            right: operand.unwrap_or(ZERO),
            bound: -minimum,
            distinct: false,
        },
    ]
}

impl Reasoning<'_, '_, '_> {
    pub(super) fn conversion_goal_normalization(
        &mut self,
        expression: &GoalExpression,
    ) -> Option<GoalNormalization> {
        let (source, destination, operand) = domain_parts(expression)?;
        let constant = if source.converts_totally_to(destination) {
            Some(true)
        } else {
            self.input
                .conversion_goal_constant(operand)
                .and_then(|value| constant_domain(source, destination, value))
        };
        if let Some(defined) = constant {
            return Some(GoalNormalization::conjunction(vec![Some(
                Relation::Bound {
                    left: ZERO,
                    right: ZERO,
                    bound: if defined { 0 } else { -1 },
                },
            )]));
        }
        let (minimum, maximum) = domain_interval(source, destination)?;
        let operand = self.goal_operand(operand);
        Some(GoalNormalization::sufficient_conjunction(
            bound_components(operand, minimum, maximum)
                .iter()
                .map(request_relation)
                .collect(),
        ))
    }
}

impl Input<'_, '_> {
    fn conversion_goal_constant<'a>(
        &'a self,
        operand: &'a GoalExpression,
    ) -> Option<&'a CheckedValue> {
        match operand {
            GoalExpression::Datum(GoalDatum::Literal(value)) => Some(value),
            GoalExpression::Datum(GoalDatum::NamedConst {
                declaration,
                projections,
                ty,
            }) if projections.is_empty() => {
                let value = &self.context.constant(*declaration)?.value;
                (value.ty() == *ty).then_some(value)
            }
            _ => None,
        }
    }
}

impl Judging<'_, '_, '_> {
    pub(super) fn judge_conversion_domain_obligation(
        &mut self,
        source: CheckedNumericType,
        destination: CheckedNumericType,
        operand: &CheckedExpression,
        site: &crate::NodePath,
        state: &mut ProofFlowState,
    ) {
        let canonical = GoalExpression::Operation {
            row: GoalOperation::NumericConversion {
                mode: CheckedConversionMode::Defined,
                source,
                destination,
            },
            type_arguments: vec![source.ty(), destination.ty()],
            const_arguments: Vec::new(),
            result: CheckedType::Bool,
            arguments: vec![self.reasoning().obligation_goal_operand(
                site,
                0,
                operand,
                &state.facts,
            )],
        };
        let operand_term = self
            .reasoning()
            .measure_operand(operand)
            .or_else(|| self.reasoning().read_operand(operand));
        let components = domain_interval(source, destination)
            .map_or_else(Vec::new, |(minimum, maximum)| {
                bound_components(operand_term, minimum, maximum)
            });
        let candidate_atom_start = self.vocabulary.affine_atoms.len();
        let mut prepared = state.affine.clone();
        let image = self
            .reasoning()
            .affine_pre_domain_form(operand, &mut prepared);
        let outcome = self.reasoning().prove(
            ProofContext::new(&state.facts, &prepared),
            ProofGoal::ConversionDomain {
                canonical: &canonical,
                operand: operand_term,
                image: image.as_ref(),
            },
        );
        if outcome.route == Some(ProofRoute::Affine) || outcome.route.is_none() {
            state.affine = prepared;
        } else {
            self.vocabulary.affine_atoms.truncate(candidate_atom_start);
        }
        let discharged = outcome.disposition == ProofDisposition::Proved;
        if let Some(root) = outcome.derivation {
            let ordinal =
                u32::try_from(self.output.obligations.len()).expect("obligation ordinal fits u32");
            self.vocabulary.derivations.add_root(
                DerivationRootKind::ConversionDomainObligation(ordinal),
                root,
            );
        }
        let residual = (!discharged).then(|| self.input.render_concrete_goal(&canonical));
        self.output.obligations.push(ObligationOutcome {
            node_path: site.clone(),
            family: ObligationFamily::ConversionDomain,
            conjunct: 0,
            canonical_goal: Some(canonical),
            components,
            discharged,
            refuted: outcome.disposition == ProofDisposition::Refuted,
            contradictory: outcome.route == Some(ProofRoute::Contradiction),
            residual,
            overlap_targets: None,
            derivation: outcome.derivation,
            allocation_length_upper_bound: None,
            allocation_length_upper_bound_derivation: None,
            affine_index_maps: Vec::new(),
            range_partitions: Vec::new(),
            written_before: state.written_before(discharged),
        });
    }
}

impl Reasoning<'_, '_, '_> {
    pub(super) fn prove_conversion_domain(
        &mut self,
        context: ProofContext<'_>,
        canonical: &GoalExpression,
        operand: Option<TermId>,
        image: Option<&AffineForm>,
    ) -> ProofResult {
        let finite = self.prove_signed(context, canonical, None);
        if finite.disposition != ProofDisposition::Unknown {
            return finite;
        }
        let (source, destination, _) = domain_parts(canonical).expect("conversion domain goal");
        let canonical = self.intern_goal_expression(canonical.clone());
        let derivation = domain_interval(source, destination).and_then(|interval| {
            self.conversion_bound_proof(context, canonical, operand, image, interval)
        });
        ProofResult {
            disposition: if derivation.is_some() {
                ProofDisposition::Proved
            } else {
                ProofDisposition::Unknown
            },
            route: derivation.map(|_| ProofRoute::Affine),
            derivation,
            numeric_upper_bound: None,
            product_interval: None,
        }
    }

    /// FN-8, including conversion leaves in a Boolean requirement, reads
    /// exactly the same upper/lower normalization as a bare conversion.
    pub(super) fn conversion_goal_bound_proof(
        &mut self,
        context: ProofContext<'_>,
        expression: &GoalExpression,
        goal: GoalId,
    ) -> Option<DerivationId> {
        let (source, destination, operand) = domain_parts(expression)?;
        let interval = domain_interval(source, destination)?;
        let term = self.goal_operand(operand);
        let image = self.affine_goal_value(operand, context.affine);
        self.conversion_bound_proof(context, goal, term, image.as_ref(), interval)
    }

    fn conversion_bound_proof(
        &mut self,
        context: ProofContext<'_>,
        goal: GoalId,
        operand: Option<TermId>,
        image: Option<&AffineForm>,
        (minimum, maximum): (i128, i128),
    ) -> Option<DerivationId> {
        let closed = context.close(
            &self.vocabulary.terms,
            &self.vocabulary.goals,
            &mut self.vocabulary.derivations,
        );
        let components = bound_components(operand, minimum, maximum);
        let affine = image.and_then(|value| {
            Some([
                affine_less_equal(value, &AffineForm::constant(maximum))?,
                affine_less_equal(&AffineForm::constant(minimum), value)?,
            ])
        });
        let mut parents = Vec::with_capacity(2);
        let maximum_term = self.vocabulary.terms.intern(TermKind::Constant(maximum));
        for (ordinal, request) in components.iter().enumerate() {
            if let Some(parent) = request_relation(request).and_then(|relation| {
                closed.relation_proof(&relation, &mut self.vocabulary.derivations)
            }) {
                parents.push(parent);
                continue;
            }
            let target = &affine.as_ref()?[ordinal];
            let right = if ordinal == 0 {
                Some(maximum_term)
            } else {
                operand
            };
            let proof = self.numeric_affine_proof(target, right, context)?;
            parents.push(
                self.vocabulary
                    .derivations
                    .intern(DerivationNode::AffineConsequence {
                        relation: None,
                        premises: proof.premises.into_boxed_slice(),
                        parents: proof.parents,
                    }),
            );
        }
        Some(
            self.vocabulary
                .derivations
                .intern(DerivationNode::ConversionDomain { goal, parents }),
        )
    }
}

/// Exact finite decoding after removing powers of two. An all-ones exponent
/// identifies both infinities and NaNs; OP-6 treats them alike for domain.
fn finite_float(ty: FloatType, bits: u64) -> Option<(bool, u64, i32)> {
    let (fraction_bits, exponent_bits, bias) = match ty {
        FloatType::F32 => (23, 8, 127),
        FloatType::F64 => (52, 11, 1023),
    };
    let exponent_mask = (1_u64 << exponent_bits) - 1;
    let exponent = (bits >> fraction_bits) & exponent_mask;
    if exponent == exponent_mask {
        return None;
    }
    let negative = ((bits >> (fraction_bits + exponent_bits)) & 1) != 0;
    let fraction = bits & ((1_u64 << fraction_bits) - 1);
    let mut significand = fraction
        | if exponent == 0 {
            0
        } else {
            1_u64 << fraction_bits
        };
    let mut power = if exponent == 0 {
        1 - bias
    } else {
        exponent as i32 - bias
    } - fraction_bits;
    if significand != 0 {
        let zeros = significand.trailing_zeros();
        significand >>= zeros;
        power += zeros as i32;
    }
    Some((negative, significand, power))
}

fn constant_domain(
    source: CheckedNumericType,
    destination: CheckedNumericType,
    value: &CheckedValue,
) -> Option<bool> {
    if value.ty() != source.ty() {
        return None;
    }
    if source == destination {
        return Some(true);
    }
    // FORM-5 supplies the exact mathematical values zero and one even while
    // their numeric type is symbolic. Both are representable in every
    // numeric primitive, so their domain does not depend on either width.
    if matches!(value, CheckedValue::NumericIdentity { .. }) {
        return Some(true);
    }
    if matches!(
        destination,
        CheckedNumericType::GenericInteger(_) | CheckedNumericType::GenericFloat(_)
    ) {
        // A symbolic answer must hold for every permitted destination. A
        // mixture is unknown, not false: the negative sign would otherwise
        // make a valid specialization's requirement contradictory.
        let mut uniform = None;
        for destination in destination.concrete_domain() {
            let answer = constant_domain(source, *destination, value)?;
            if uniform.is_some_and(|previous| previous != answer) {
                return None;
            }
            uniform = Some(answer);
        }
        return uniform;
    }
    match (value, destination) {
        (CheckedValue::Integer { ty, bits }, CheckedNumericType::Integer(destination)) => {
            let value = integer_value(*ty, *bits);
            let (minimum, maximum) = type_range(destination);
            Some(minimum <= value && value <= maximum)
        }
        (CheckedValue::Integer { ty, bits }, CheckedNumericType::Float(destination)) => {
            let magnitude = integer_value(*ty, *bits).unsigned_abs();
            let precision = match destination {
                FloatType::F32 => 24,
                FloatType::F64 => 53,
            };
            Some(
                magnitude == 0
                    || 128 - magnitude.leading_zeros() - magnitude.trailing_zeros() <= precision,
            )
        }
        (CheckedValue::Float { ty, bits }, CheckedNumericType::Integer(destination)) => {
            let Some((negative, significand, power)) = finite_float(*ty, *bits) else {
                return Some(false);
            };
            if significand == 0 {
                return Some(true);
            }
            if !(0..128).contains(&power) || 64 - significand.leading_zeros() + power as u32 > 127 {
                return Some(false);
            }
            let magnitude = i128::from(significand) << power;
            let value = if negative { -magnitude } else { magnitude };
            let (minimum, maximum) = type_range(destination);
            Some(minimum <= value && value <= maximum)
        }
        (CheckedValue::Float { ty, bits }, CheckedNumericType::Float(destination)) => {
            let Some((_, significand, power)) = finite_float(*ty, *bits) else {
                return Some(true);
            };
            if significand == 0 || destination == FloatType::F64 {
                return Some(true);
            }
            let significant_bits = 64 - significand.leading_zeros();
            Some(
                significant_bits <= 24
                    && power >= -149
                    && power + significant_bits as i32 - 1 <= 127,
            )
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbolic_constant_domains_require_one_uniform_truth_sign() {
        let source_parameter = crate::DeclarationId::from_index(0).unwrap();
        let destination_parameter = crate::DeclarationId::from_index(1).unwrap();
        let integer_destination = CheckedNumericType::GenericInteger(destination_parameter);
        let float_destination = CheckedNumericType::GenericFloat(destination_parameter);
        for source in [
            CheckedNumericType::GenericInteger(source_parameter),
            CheckedNumericType::GenericFloat(source_parameter),
        ] {
            for destination in [integer_destination, float_destination] {
                for one in [false, true] {
                    assert_eq!(
                        constant_domain(
                            source,
                            destination,
                            &CheckedValue::NumericIdentity {
                                ty: source.ty(),
                                one
                            }
                        ),
                        Some(true)
                    );
                }
            }
        }
        for (destination, integer, expected) in [
            (integer_destination, 127, Some(true)),
            (integer_destination, 128, None),
            (float_destination, 16_777_218, Some(true)),
            (float_destination, 16_777_217, None),
            (float_destination, u64::MAX, Some(false)),
        ] {
            assert_eq!(
                constant_domain(
                    CheckedNumericType::Integer(IntegerType::U64),
                    destination,
                    &CheckedValue::Integer {
                        ty: IntegerType::U64,
                        bits: integer
                    }
                ),
                expected
            );
        }
        assert_eq!(
            constant_domain(
                CheckedNumericType::Float(FloatType::F64),
                integer_destination,
                &CheckedValue::Float {
                    ty: FloatType::F64,
                    bits: 0x3ff8_0000_0000_0000
                }
            ),
            Some(false)
        );
    }

    #[test]
    fn exact_bit_domains_distinguish_rounding_collisions_and_subnormals() {
        for (integer, defined) in [
            (1_u64 << 24, true),
            ((1 << 24) + 1, false),
            ((1 << 24) + 2, true),
            (u64::MAX, false),
        ] {
            assert_eq!(
                constant_domain(
                    CheckedNumericType::Integer(IntegerType::U64),
                    CheckedNumericType::Float(FloatType::F32),
                    &CheckedValue::Integer {
                        ty: IntegerType::U64,
                        bits: integer
                    }
                ),
                Some(defined)
            );
        }
        for (bits, defined) in [
            (0x43e0_0000_0000_0000, false), // positive 2^63
            (0x43df_ffff_ffff_ffff, true),
            (0xc3e0_0000_0000_0000, true),  // negative 2^63
            (0x3ff8_0000_0000_0000, false), // one and a half
            (0x8000_0000_0000_0000, true),
            (0x7ff0_0000_0000_0000, false),
            (0x7ff8_0000_0000_0001, false),
        ] {
            assert_eq!(
                constant_domain(
                    CheckedNumericType::Float(FloatType::F64),
                    CheckedNumericType::Integer(IntegerType::I64),
                    &CheckedValue::Float {
                        ty: FloatType::F64,
                        bits
                    }
                ),
                Some(defined)
            );
        }
        for (bits, defined) in [
            (0x36a0_0000_0000_0000, true),  // 2^-149, smallest f32 subnormal
            (0x3690_0000_0000_0000, false), // 2^-150
            (0x47ef_ffff_e000_0000, true),  // largest finite f32
            (0x47f0_0000_0000_0000, false), // 2^128
            (0x8000_0000_0000_0000, true),
            (0x7ff0_0000_0000_0000, true),
            (0x7ff8_0000_0000_0001, true),
        ] {
            assert_eq!(
                constant_domain(
                    CheckedNumericType::Float(FloatType::F64),
                    CheckedNumericType::Float(FloatType::F32),
                    &CheckedValue::Float {
                        ty: FloatType::F64,
                        bits
                    }
                ),
                Some(defined)
            );
        }
    }
}
