//! [PRF-1] certificates: the written `use` premises, their weighted
//! source-order sum, and the residual the fixed rule must prove.

use super::*;

impl Vocabulary {
    /// Folds a nonlinear certificate sum to the affine inequality it equals.
    pub(super) fn folded_certificate_sum(
        &self,
        polynomial: &CertificatePolynomial,
        target: &AffineInequality,
    ) -> Result<AffineInequality, SourceProofCertificateFailure> {
        // Several bindings can hold the same product, and they are equal
        // values, so any of them folds soundly. The target's own text picks
        // among them: a monomial folded to the value the target already names
        // cancels against it, while one folded to an equal value under
        // another name does not. Failing that, the least atom is a canonical
        // choice. Neither is a search — one pass, one winner per operand pair.
        let named_by_target = target
            .terms()
            .iter()
            .map(|coefficient| coefficient.term())
            .collect::<HashSet<_>>();
        let mut products = std::collections::BTreeMap::new();
        for (product, operands) in &self.product_atoms {
            products
                .entry(*operands)
                .and_modify(|chosen: &mut AffineTermId| {
                    let better = match (
                        named_by_target.contains(chosen),
                        named_by_target.contains(product),
                    ) {
                        (false, true) => true,
                        (true, false) => false,
                        _ => *product < *chosen,
                    };
                    if better {
                        *chosen = *product;
                    }
                })
                .or_insert(*product);
        }
        let folded = polynomial
            .fold_products(&products)
            .map_err(certificate_fold_failure)?;
        let mut images = std::collections::BTreeMap::new();
        for (handle, image) in &self.handle_images {
            let mut weights = image
                .terms()
                .iter()
                .map(|coefficient| (Some(coefficient.term()), coefficient.coefficient()))
                .collect::<Vec<_>>();
            weights.push((None, image.constant_value()));
            images.insert(*handle, weights);
        }
        let folded = folded
            .unfold_handles(&images)
            .map_err(certificate_fold_failure)?;
        let mut check = AffineCheckState::new();
        match folded.into_inequality(&mut check) {
            Some(formed) => formed.map_err(certificate_fold_failure),
            None => Err(SourceProofCertificateFailure::NonlinearResidual),
        }
    }
}

impl Reasoning<'_, '_, '_> {
    pub(super) fn source_proof_premise_results(
        &mut self,
        premises: &[Option<AffineInequality>],
        named_premises: &[bool],
        published_premises: &[bool],
        values: &AffineFlowState,
        facts: &FactState,
    ) -> Vec<bool> {
        let mut closed: Option<ProofClosure> = None;
        premises
            .iter()
            .zip(named_premises)
            .zip(published_premises)
            .map(|((premise, named), published)| {
                // A bare invariant name means that exact declaration's
                // published theorem, not merely any proposition with the same
                // normalized inequality. Only a relation-form use asks AUTO
                // to prove its written source from the entering context.
                if *named {
                    return *published;
                }
                let Some(premise) = premise.as_ref() else {
                    return false;
                };
                let goal = ProofGoal::AutomaticAffine {
                    inequality: premise,
                };
                // Each relation source reads these same immutable facts. A
                // newly registered term or goal changes the closure universe,
                // so only the unchanged view is reused; no proof is memoized.
                if closed.as_ref().is_none_or(|view| {
                    !view.matches(&self.vocabulary.terms, &self.vocabulary.goals)
                }) {
                    closed = Some(ProofClosure::new(
                        facts,
                        &self.vocabulary.terms,
                        &self.vocabulary.goals,
                        &mut self.vocabulary.derivations,
                    ));
                }
                self.prove(
                    ProofContext {
                        facts,
                        affine: values,
                        closed: closed.as_ref(),
                    },
                    goal,
                )
                .disposition
                    == ProofDisposition::Proved
            })
            .collect()
    }

    /// Resolves one written multiplicity where the certificate is checked.
    ///
    /// A named multiplicity reads the value image its binding holds in the
    /// entering context, minting the atom if this is the first read of it, so
    /// the scaling step is over the same immutable value identity every other
    /// affine premise names.
    pub(super) fn certificate_multiplicity(
        &mut self,
        multiplicity: CheckedProofMultiplicity,
        state: &mut AffineFlowState,
    ) -> Option<CertificateMultiplicity> {
        match multiplicity {
            CheckedProofMultiplicity::Literal(factor) => {
                Some(CertificateMultiplicity::Literal(factor))
            }
            CheckedProofMultiplicity::Value { binding, .. } => Some(
                CertificateMultiplicity::Value(self.affine_opaque_handle(binding, state)?),
            ),
        }
    }

    /// Brings the accumulated certificate sum back to one affine inequality
    /// and checks the writer-selected residual against it.
    ///
    /// A nonlinear accumulation folds first: each degree-two monomial must be
    /// the value image of an admitted exact product, which is the only way a
    /// term-scaled premise can meet an affine target. Once folded, the residual
    /// is the same one a bare-decimal certificate reaches, proved by the same
    /// route; a monomial with no such product is a refusal, not a weaker check.
    pub(super) fn source_proof_certificate_residual(
        &mut self,
        target: &AffineInequality,
        sum: &CertificateSum,
        values: &AffineFlowState,
        facts: &FactState,
    ) -> Result<bool, SourceProofCertificateFailure> {
        let folded;
        let sum = match sum {
            CertificateSum::Empty => {
                return Err(SourceProofCertificateFailure::FormationCapacity);
            }
            CertificateSum::Affine(sum) => sum,
            CertificateSum::Nonlinear(polynomial) => {
                folded = self.vocabulary.folded_certificate_sum(polynomial, target)?;
                &folded
            }
        };
        self.source_proof_residual(target, sum, values, facts)
    }

    /// Checks the final writer-selected residual after every source proposition
    /// and its scaled sum have formed.
    ///
    /// `target - sum` may be discharged only by the existing direct L0 closure
    /// or fixed interval rule at the entering program point, applied to the
    /// written sum and then to its integer tightenings. This route never
    /// selects another affine premise, derives a multiplier, or retries a
    /// subset.
    pub(super) fn source_proof_residual(
        &mut self,
        target: &AffineInequality,
        sum: &AffineInequality,
        values: &AffineFlowState,
        facts: &FactState,
    ) -> Result<bool, SourceProofCertificateFailure> {
        let mut check = AffineCheckState::new();
        // The untightened residual forms first so an arithmetic or capacity
        // failure of the written sum keeps its exact PRF-1 diagnostic.
        match AffineInequality::residual_after(target, sum, &mut check) {
            Ok(_) => {}
            Err(AffineCheckError::ArithmeticOverflow) => {
                return Err(SourceProofCertificateFailure::ArithmeticOverflow);
            }
            Err(AffineCheckError::LimitExceeded(_)) => {
                return Err(SourceProofCertificateFailure::FormationCapacity);
            }
            Err(
                AffineCheckError::CoefficientMismatch | AffineCheckError::InvalidCertificateFactor,
            ) => return Ok(false),
        }
        let candidates = self.affine_l0_candidates(values);
        let closed = close(
            facts,
            &self.vocabulary.terms,
            &self.vocabulary.goals,
            &mut self.vocabulary.derivations,
        );
        let l0 = affine_l0_index(&candidates, &closed, &mut check);
        let mut query = AffineDirectQuery::new(&l0, values, &closed);
        Ok(self
            .affine_candidate_residual_proof(target, sum, &mut query, &mut check)
            .is_some())
    }
}

pub(super) fn source_proof_formation_failure(
    error: AffineCheckError,
) -> SourceProofCertificateFailure {
    match error {
        AffineCheckError::ArithmeticOverflow => SourceProofCertificateFailure::ArithmeticOverflow,
        AffineCheckError::LimitExceeded(_) => SourceProofCertificateFailure::FormationCapacity,
        AffineCheckError::CoefficientMismatch | AffineCheckError::InvalidCertificateFactor => {
            unreachable!("a checked affine source has inconsistent internal structure")
        }
    }
}

/// Forms the one weighted premise sum the source writer selected.
///
/// The written premises are multiplied and summed exactly in source order.
/// This phase depends only on the formed source propositions and written
/// factors. It deliberately runs before premise availability is judged.
pub(super) fn source_proof_sum(
    premises: &[(AffineInequality, CertificateMultiplicity)],
) -> Result<CertificateSum, (SourceProofCertificateFailure, u32)> {
    let actual = u32::try_from(premises.len()).unwrap_or(u32::MAX);
    if premises.len() > MAX_CERTIFICATE_PREMISES {
        let maximum =
            u32::try_from(MAX_CERTIFICATE_PREMISES).expect("certificate capacity fits u32");
        return Err((
            SourceProofCertificateFailure::UseCapacity { maximum, actual },
            maximum,
        ));
    }

    let mut first_by_premise = HashMap::new();
    for (index, (premise, multiplicity)) in premises.iter().enumerate() {
        let index = u32::try_from(index).expect("certificate capacity fits u32");
        // A term multiplicity is unsigned by [PRF-1], so only the written
        // decimal can be degenerate. A runtime zero drops its premise and
        // the sum stays sound, which is why nothing rejects it here.
        if matches!(multiplicity, CertificateMultiplicity::Literal(factor) if *factor <= 0) {
            return Err((
                SourceProofCertificateFailure::InvalidFactor { use_index: index },
                index,
            ));
        }
        if let Some(first) = first_by_premise.insert(premise.clone(), index) {
            return Err((
                SourceProofCertificateFailure::RepeatedUse {
                    first,
                    repeated: index,
                },
                index,
            ));
        }
    }

    // Build the written sum one source entry at a time. Besides preserving
    // source order, this records the exact entry whose scale or addition
    // first exceeds the proof arithmetic or affine formation domain.
    //
    // The accumulator starts affine and becomes a degree-two polynomial at
    // the first term multiplicity, if there is one; a certificate written
    // entirely with bare decimals therefore never leaves the affine arm
    // and forms exactly the inequality it always did.
    let mut sum = CertificateSum::Empty;
    for (index, (inequality, multiplicity)) in premises.iter().enumerate() {
        let index = u32::try_from(index).expect("certificate capacity fits u32");
        sum = extend_certificate_sum(sum, inequality, multiplicity)
            .map_err(|failure| (certificate_step_failure(failure, index, actual), index))?;
    }
    match sum {
        CertificateSum::Empty => Err((SourceProofCertificateFailure::FormationCapacity, 0)),
        formed => Ok(formed),
    }
}

/// Adds one written entry to the accumulated certificate sum.
pub(super) fn extend_certificate_sum(
    sum: CertificateSum,
    inequality: &AffineInequality,
    multiplicity: &CertificateMultiplicity,
) -> Result<CertificateSum, CertificateStepFailure> {
    if let CertificateMultiplicity::Literal(factor) = *multiplicity {
        match sum {
            CertificateSum::Empty => {
                let mut check = AffineCheckState::new();
                return Ok(CertificateSum::Affine(sum_explicit_scaled_inequalities(
                    &[ScaledAffinePremise { inequality, factor }],
                    &mut check,
                )?));
            }
            CertificateSum::Affine(previous) => {
                let mut check = AffineCheckState::new();
                return Ok(CertificateSum::Affine(sum_explicit_scaled_inequalities(
                    &[
                        ScaledAffinePremise {
                            inequality: &previous,
                            factor: 1,
                        },
                        ScaledAffinePremise { inequality, factor },
                    ],
                    &mut check,
                )?));
            }
            CertificateSum::Nonlinear(previous) => {
                let scaled = CertificatePolynomial::from_inequality(inequality)?.scale(factor)?;
                return Ok(CertificateSum::Nonlinear(previous.add(&scaled)?));
            }
        }
    }
    let CertificateMultiplicity::Value(value) = multiplicity else {
        unreachable!("the literal arm returned above");
    };
    let scaled = CertificatePolynomial::from_inequality(inequality)?
        .multiply(&CertificatePolynomial::from_form(value)?)?;
    let previous = match sum {
        CertificateSum::Empty => CertificatePolynomial::zero(),
        CertificateSum::Affine(previous) => CertificatePolynomial::from_inequality(&previous)?,
        CertificateSum::Nonlinear(previous) => previous,
    };
    Ok(CertificateSum::Nonlinear(previous.add(&scaled)?))
}

pub(super) fn certificate_step_failure(
    failure: CertificateStepFailure,
    index: u32,
    actual: u32,
) -> SourceProofCertificateFailure {
    match failure {
        CertificateStepFailure::Overflow => SourceProofCertificateFailure::ArithmeticOverflow,
        CertificateStepFailure::UseCapacity => SourceProofCertificateFailure::UseCapacity {
            maximum: u32::try_from(MAX_CERTIFICATE_PREMISES)
                .expect("certificate capacity fits u32"),
            actual,
        },
        CertificateStepFailure::Formation => SourceProofCertificateFailure::FormationCapacity,
        CertificateStepFailure::InvalidFactor => {
            SourceProofCertificateFailure::InvalidFactor { use_index: index }
        }
    }
}

pub(super) fn certificate_fold_failure(error: PolynomialError) -> SourceProofCertificateFailure {
    match error {
        PolynomialError::ArithmeticOverflow => SourceProofCertificateFailure::ArithmeticOverflow,
        PolynomialError::DegreeExceeded | PolynomialError::LimitExceeded => {
            SourceProofCertificateFailure::FormationCapacity
        }
    }
}
