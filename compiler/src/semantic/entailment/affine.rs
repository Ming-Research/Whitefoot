//! Deterministic affine arithmetic for the normal semantic checker.
//!
//! This module normalizes checked affine expressions, carries exact affine
//! value forms, constructs canonical inequalities, and applies the fixed
//! fixed residual and interval rules used by semantic checking. It
//! performs no heuristic search and every `i128` operation is checked. Work is
//! counted for measurement, but a cumulative compiler budget never changes
//! whether a source proposition is accepted.

/// Dense identity of one term in a semantic-checker-owned affine vocabulary.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct AffineTermId(u32);

impl AffineTermId {
    pub(crate) const fn from_index(index: u32) -> Self {
        Self(index)
    }

    pub(crate) const fn index(self) -> u32 {
        self.0
    }
}

/// Minimal resolved expression accepted by the affine normalizer.
///
/// Source typing, name resolution, and invariant-scope checks happen before
/// this value is built. Consequently the arithmetic core needs no binding
/// table or program-point authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AffineExpression {
    Constant(i128),
    Term(AffineTermId),
    Add(Box<AffineExpression>, Box<AffineExpression>),
    Subtract(Box<AffineExpression>, Box<AffineExpression>),
    MultiplyByConstant {
        constant: i128,
        value: Box<AffineExpression>,
    },
}

impl Drop for AffineExpression {
    fn drop(&mut self) {
        fn take_children(expression: &mut AffineExpression, pending: &mut Vec<AffineExpression>) {
            match expression {
                AffineExpression::Add(left, right) | AffineExpression::Subtract(left, right) => {
                    pending.push(std::mem::replace(
                        left.as_mut(),
                        AffineExpression::Constant(0),
                    ));
                    pending.push(std::mem::replace(
                        right.as_mut(),
                        AffineExpression::Constant(0),
                    ));
                }
                AffineExpression::MultiplyByConstant { value, .. } => {
                    pending.push(std::mem::replace(
                        value.as_mut(),
                        AffineExpression::Constant(0),
                    ));
                }
                AffineExpression::Constant(_) | AffineExpression::Term(_) => {}
            }
        }

        // Formation may reject an oversized tree; releasing that tree must
        // not recurse through the very shape whose capacity was exceeded.
        let mut pending = Vec::new();
        take_children(self, &mut pending);
        while let Some(mut expression) = pending.pop() {
            take_children(&mut expression, &mut pending);
        }
    }
}

/// One nonzero coefficient in a canonical affine left-hand side.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct AffineCoefficient {
    term: AffineTermId,
    coefficient: i128,
}

/// One exact mathematical value carried by the normal semantic flow.
///
/// `constant + sum(coefficient * term)` is canonical: term identities are
/// strictly ordered and zero coefficients are absent.  A form records what a
/// source value equals at the current program point; it is not a proposition
/// and grants no proof authority by itself.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct AffineForm {
    terms: Box<[AffineCoefficient]>,
    constant: i128,
}

impl AffineForm {
    pub(crate) fn constant(value: i128) -> Self {
        Self {
            terms: Box::new([]),
            constant: value,
        }
    }

    pub(crate) fn term(term: AffineTermId) -> Self {
        Self {
            terms: Box::new([AffineCoefficient {
                term,
                coefficient: 1,
            }]),
            constant: 0,
        }
    }

    pub(crate) fn terms(&self) -> &[AffineCoefficient] {
        &self.terms
    }

    pub(crate) const fn constant_value(&self) -> i128 {
        self.constant
    }

    /// Returns the canonical nonconstant coefficient vector with a zero
    /// constant. Structural joins use this only after every incoming form has
    /// the same `terms()` slice; the differing constants are represented by
    /// one separately bounded delta atom.
    pub(crate) fn nonconstant_part(&self) -> Self {
        Self {
            terms: self.terms.clone(),
            constant: 0,
        }
    }

    pub(crate) fn unit_term(&self) -> Option<AffineTermId> {
        let [coefficient] = self.terms.as_ref() else {
            return None;
        };
        (self.constant == 0 && coefficient.coefficient == 1).then_some(coefficient.term)
    }

    pub(crate) fn add(
        &self,
        right: &Self,
        check: &mut AffineCheckState,
    ) -> Result<Self, AffineCheckError> {
        Ok(Self {
            terms: merge_scaled(self.terms(), right.terms(), 1, check)?.into_boxed_slice(),
            constant: checked_add(self.constant, right.constant)?,
        })
    }

    pub(crate) fn subtract(
        &self,
        right: &Self,
        check: &mut AffineCheckState,
    ) -> Result<Self, AffineCheckError> {
        Ok(Self {
            terms: merge_scaled(self.terms(), right.terms(), -1, check)?.into_boxed_slice(),
            constant: checked_sub(self.constant, right.constant)?,
        })
    }

    pub(crate) fn scale(
        &self,
        factor: i128,
        check: &mut AffineCheckState,
    ) -> Result<Self, AffineCheckError> {
        if factor == 0 {
            check.charge(1)?;
            return Ok(Self::constant(0));
        }
        Ok(Self {
            terms: merge_scaled(&[], self.terms(), factor, check)?.into_boxed_slice(),
            constant: checked_mul(self.constant, factor)?,
        })
    }
}

impl AffineCoefficient {
    pub(crate) const fn term(self) -> AffineTermId {
        self.term
    }

    pub(crate) const fn coefficient(self) -> i128 {
        self.coefficient
    }
}

/// Canonical `sum(coefficient * term) <= upper`.
///
/// Terms are strictly ordered by identity and zero coefficients are removed.
/// Private fields ensure all values pass the same checked canonicalization.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct AffineInequality {
    terms: Box<[AffineCoefficient]>,
    upper: i128,
}

impl AffineInequality {
    pub(crate) fn from_terms(
        terms: &[(AffineTermId, i128)],
        upper: i128,
        check: &mut AffineCheckState,
    ) -> Result<Self, AffineCheckError> {
        if terms.len() > check.limits.max_input_terms {
            return Err(AffineCheckError::LimitExceeded(
                AffineCheckLimit::InputTerms,
            ));
        }
        let mut canonical = Vec::new();
        for &(term, coefficient) in terms {
            check.charge(1)?;
            insert_coefficient(&mut canonical, term, coefficient, check)?;
        }
        Ok(Self {
            terms: canonical.into_boxed_slice(),
            upper,
        })
    }

    pub(crate) fn terms(&self) -> &[AffineCoefficient] {
        &self.terms
    }

    pub(crate) const fn upper(&self) -> i128 {
        self.upper
    }

    /// Forms the proposition `left <= right` from two exact current values.
    pub(crate) fn from_forms(
        left: &AffineForm,
        right: &AffineForm,
        check: &mut AffineCheckState,
    ) -> Result<Self, AffineCheckError> {
        let difference = left.subtract(right, check)?;
        Ok(Self {
            terms: difference.terms,
            upper: checked_neg(difference.constant)?,
        })
    }

    /// Forms the proposition `left - right <= bound` from two exact current
    /// values and one already-established ordinary difference bound.
    pub(crate) fn from_bounded_forms(
        left: &AffineForm,
        right: &AffineForm,
        bound: i128,
        check: &mut AffineCheckState,
    ) -> Result<Self, AffineCheckError> {
        let mut inequality = Self::from_forms(left, right, check)?;
        inequality.upper = checked_add(inequality.upper, bound)?;
        Ok(inequality)
    }

    /// The exact complement `sum(coefficient * term) >= upper + 1`, in
    /// canonical form. Every atom denotes a mathematical integer, so this is
    /// the negation an [INV-1] target's refutation derives [MSR-4].
    pub(crate) fn negated(&self, check: &mut AffineCheckState) -> Result<Self, AffineCheckError> {
        Ok(Self {
            terms: merge_scaled(&[], self.terms(), -1, check)?.into_boxed_slice(),
            upper: checked_sub(checked_neg(self.upper)?, 1)?,
        })
    }

    /// Removes one already-established premise with coefficient one.
    ///
    /// If interval facts prove the returned inequality, adding `premise`
    /// proves `target`. The semantic checker deliberately performs no
    /// coefficient search: if this fixed coefficient-one rule does not close
    /// the target, this route has not proved it.
    pub(crate) fn residual_after(
        target: &Self,
        premise: &Self,
        check: &mut AffineCheckState,
    ) -> Result<Self, AffineCheckError> {
        Self::residual_after_scaled(target, premise, 1, check)
    }

    /// Removes one already-established premise multiplied by the written
    /// mathematical factor selected by the deterministic residual rule.
    pub(crate) fn residual_after_scaled(
        target: &Self,
        premise: &Self,
        factor: i128,
        check: &mut AffineCheckState,
    ) -> Result<Self, AffineCheckError> {
        Ok(Self {
            terms: merge_scaled(target.terms(), premise.terms(), checked_neg(factor)?, check)?
                .into_boxed_slice(),
            upper: checked_sub(target.upper, checked_mul(premise.upper, factor)?)?,
        })
    }
}

/// Divides one accumulated inequality by a positive integer factor that
/// divides every coefficient exactly, keeping the mathematical floor of its
/// bound.
///
/// Every affine atom denotes a mathematical integer, so `factor * v <= upper`
/// with a positive `factor` also proves `v <= floor(upper / factor)`. The
/// floor is taken toward negative infinity, which is the exact strongest
/// integer consequence; `i128` truncation toward zero would weaken it for a
/// negative bound. `None` means this factor does not divide the coefficient
/// vector and therefore tightens nothing.
pub(crate) fn integer_tightening(
    inequality: &AffineInequality,
    factor: i128,
    check: &mut AffineCheckState,
) -> Result<Option<AffineInequality>, AffineCheckError> {
    if factor <= 1 || inequality.terms().is_empty() {
        return Ok(None);
    }
    let mut divided = Vec::with_capacity(inequality.terms().len());
    for coefficient in inequality.terms() {
        check.charge(1)?;
        if coefficient.coefficient() % factor != 0 {
            return Ok(None);
        }
        divided.push((coefficient.term(), coefficient.coefficient() / factor));
    }
    let upper = inequality
        .upper()
        .checked_div_euclid(factor)
        .ok_or(AffineCheckError::ArithmeticOverflow)?;
    Ok(Some(AffineInequality::from_terms(&divided, upper, check)?))
}

/// The complete fixed list of integer tightenings of one accumulated candidate
/// against one target, in this order: the positive multiple relating the two
/// coefficient vectors, then the greatest common divisor of the candidate's
/// own coefficients.
///
/// Both factors are functions of the two coefficient vectors alone. Nothing
/// here searches for a factor, selects a premise, or forms a candidate outside
/// the caller's fixed family; a factor that tightens nothing is absent.
///
/// Each factor is derived and divided out on its own. A factor whose
/// derivation or division leaves the checked `i128` range, or exceeds an
/// ordinary checker capacity, is an unrepresentable candidate: it is skipped
/// and cannot suppress the other factor, which may still be representable.
/// This function therefore reports no failure of its own, and the fixed order
/// of the surviving candidates is independent of which were skipped.
pub(crate) fn integer_tightenings(
    candidate: &AffineInequality,
    target: &AffineInequality,
    check: &mut AffineCheckState,
) -> Vec<AffineInequality> {
    let multiple = target_multiple_factor(candidate, target, check)
        .ok()
        .flatten();
    let divisor = coefficient_gcd(candidate, check)
        .ok()
        .flatten()
        .filter(|factor| Some(*factor) != multiple);

    let mut tightenings = Vec::new();
    for factor in [multiple, divisor].into_iter().flatten() {
        if let Ok(Some(tightened)) = integer_tightening(candidate, factor, check) {
            tightenings.push(tightened);
        }
    }
    tightenings
}

/// Reads the one positive integer `k` for which the candidate's coefficient
/// vector is exactly `k` times the target's.
///
/// The factor is read from the first coefficient pair and then verified
/// against every remaining pair, so it is decided rather than searched for.
fn target_multiple_factor(
    candidate: &AffineInequality,
    target: &AffineInequality,
    check: &mut AffineCheckState,
) -> Result<Option<i128>, AffineCheckError> {
    if candidate.terms().len() != target.terms().len() {
        return Ok(None);
    }
    let (Some(first_candidate), Some(first_target)) =
        (candidate.terms().first(), target.terms().first())
    else {
        return Ok(None);
    };
    check.charge(1)?;
    let factor = first_candidate
        .coefficient()
        .checked_div(first_target.coefficient())
        .ok_or(AffineCheckError::ArithmeticOverflow)?;
    if factor <= 0 || first_candidate.coefficient() % first_target.coefficient() != 0 {
        return Ok(None);
    }
    for (candidate_term, target_term) in candidate.terms().iter().zip(target.terms()) {
        check.charge(1)?;
        if candidate_term.term() != target_term.term()
            || candidate_term.coefficient() != checked_mul(target_term.coefficient(), factor)?
        {
            return Ok(None);
        }
    }
    Ok(Some(factor))
}

/// The greatest common divisor of one inequality's coefficient magnitudes.
///
/// `None` means there is no such positive `i128` value: the inequality has no
/// term, or its divisor is the magnitude of `i128::MIN` and is unrepresentable.
fn coefficient_gcd(
    inequality: &AffineInequality,
    check: &mut AffineCheckState,
) -> Result<Option<i128>, AffineCheckError> {
    let mut divisor = 0_u128;
    for coefficient in inequality.terms() {
        check.charge(1)?;
        let mut left = divisor;
        let mut right = coefficient.coefficient().unsigned_abs();
        while right != 0 {
            check.charge(1)?;
            let remainder = left % right;
            left = right;
            right = remainder;
        }
        divisor = left;
    }
    if divisor == 0 {
        return Ok(None);
    }
    Ok(i128::try_from(divisor).ok())
}

/// Sums one author-selected affine certificate in exactly the supplied order.
///
/// The caller is responsible for proving every written premise independently.
/// This core only forms the canonical mathematical sum; it neither selects an
/// additional premise nor searches for coefficients.
pub(crate) fn sum_explicit_inequalities(
    premises: &[AffineInequality],
    check: &mut AffineCheckState,
) -> Result<AffineInequality, AffineCheckError> {
    if premises.len() > check.limits.max_certificate_premises {
        return Err(AffineCheckError::LimitExceeded(
            AffineCheckLimit::CertificatePremises,
        ));
    }

    let mut terms = Vec::new();
    let mut upper = 0_i128;
    for premise in premises {
        terms = merge_scaled(&terms, premise.terms(), 1, check)?;
        upper = checked_add(upper, premise.upper())?;
    }
    Ok(AffineInequality {
        terms: terms.into_boxed_slice(),
        upper,
    })
}

/// One premise explicitly selected by a source certificate, paired with the
/// positive mathematical multiplier written on that `use`.
///
/// The multiplier belongs to the proof domain, not to any machine-integer
/// type. Source checking admits only values in `1..=i128::MAX`; this arithmetic
/// core repeats that check so an invalid internal value cannot be mistaken for
/// a valid certificate step.
#[derive(Clone, Copy)]
pub(crate) struct ScaledAffinePremise<'premise> {
    pub(crate) inequality: &'premise AffineInequality,
    pub(crate) factor: i128,
}

/// Sums one author-selected affine certificate in exactly source order.
///
/// Every multiplier is written in the source. This function never derives a
/// multiplier, retries a different order, selects another premise, or
/// publishes an intermediate sum. Its work is therefore linear in the
/// written `use` list, subject only to the fixed affine formation capacities.
pub(crate) fn sum_explicit_scaled_inequalities(
    premises: &[ScaledAffinePremise<'_>],
    check: &mut AffineCheckState,
) -> Result<AffineInequality, AffineCheckError> {
    if premises.len() > check.limits.max_certificate_premises {
        return Err(AffineCheckError::LimitExceeded(
            AffineCheckLimit::CertificatePremises,
        ));
    }

    let mut terms = Vec::new();
    let mut upper = 0_i128;
    for premise in premises {
        if premise.factor <= 0 {
            return Err(AffineCheckError::InvalidCertificateFactor);
        }
        terms = merge_scaled(&terms, premise.inequality.terms(), premise.factor, check)?;
        upper = checked_add(
            upper,
            checked_mul(premise.inequality.upper(), premise.factor)?,
        )?;
    }
    Ok(AffineInequality {
        terms: terms.into_boxed_slice(),
        upper,
    })
}

/// Uses one independently known inclusive interval per affine term.  This is
/// the fixed interval rule: positive coefficients select the upper endpoint,
/// negative coefficients select the lower endpoint.  `false` means only that
/// this rule did not prove the proposition; it never means the proposition is
/// false.
pub(crate) fn interval_proves(
    inequality: &AffineInequality,
    interval: impl FnMut(AffineTermId) -> Option<(i128, i128)>,
    check: &mut AffineCheckState,
) -> Result<bool, AffineCheckError> {
    let Some(maximum) = interval_maximum(inequality.terms(), interval, check)? else {
        return Ok(false);
    };
    check.charge(1)?;
    Ok(maximum <= inequality.upper())
}

/// Computes the greatest value of one affine term sum from independently
/// known inclusive intervals. This is the numeric half of
/// [`interval_proves`], exposed so a finite domain rule can derive an exact
/// operand endpoint before checking a fixed set of nonlinear endpoint
/// combinations. It performs no optimization search: each coefficient is
/// visited once and selects exactly one endpoint by its sign.
pub(crate) fn interval_maximum(
    coefficients: &[AffineCoefficient],
    mut interval: impl FnMut(AffineTermId) -> Option<(i128, i128)>,
    check: &mut AffineCheckState,
) -> Result<Option<i128>, AffineCheckError> {
    let mut maximum = 0_i128;
    for coefficient in coefficients {
        check.charge(1)?;
        let Some((minimum, upper)) = interval(coefficient.term()) else {
            return Ok(None);
        };
        let endpoint = if coefficient.coefficient() > 0 {
            upper
        } else {
            minimum
        };
        maximum = checked_add(maximum, checked_mul(coefficient.coefficient(), endpoint)?)?;
    }
    Ok(Some(maximum))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AffineCheckLimits {
    max_expression_nodes: usize,
    max_input_terms: usize,
    max_terms: usize,
    max_certificate_premises: usize,
}

/// Compiler-owned deterministic ceilings. They are ordinary checker
/// capacities, not values selected by source text or an external caller.
const AFFINE_CHECK_LIMITS: AffineCheckLimits = AffineCheckLimits {
    max_expression_nodes: 4_096,
    max_input_terms: 4_096,
    max_terms: 4_096,
    max_certificate_premises: 4_096,
};

/// Maximum number of source-written `use` entries in one local certificate.
/// This is a structural language capacity, not a time or work budget.
pub(crate) const MAX_CERTIFICATE_PREMISES: usize = AFFINE_CHECK_LIMITS.max_certificate_premises;

/// One finite capacity of an affine semantic-check operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AffineCheckLimit {
    ExpressionNodes,
    InputTerms,
    ResultTerms,
    CertificatePremises,
}

/// Deterministic failure from the affine arithmetic checker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AffineCheckError {
    ArithmeticOverflow,
    LimitExceeded(AffineCheckLimit),
    CoefficientMismatch,
    InvalidCertificateFactor,
}

/// Structural limits and measured work for one affine checking unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AffineCheckState {
    limits: AffineCheckLimits,
    used: u64,
}

impl AffineCheckState {
    pub(crate) const fn new() -> Self {
        Self {
            limits: AFFINE_CHECK_LIMITS,
            used: 0,
        }
    }

    pub(crate) const fn used(&self) -> u64 {
        self.used
    }

    #[cfg(test)]
    const fn with_limits(limits: AffineCheckLimits) -> Self {
        Self { limits, used: 0 }
    }

    pub(crate) fn charge(&mut self, amount: u64) -> Result<(), AffineCheckError> {
        self.used = self.used.saturating_add(amount);
        Ok(())
    }
}

impl Default for AffineCheckState {
    fn default() -> Self {
        Self::new()
    }
}

/// Normalizes one explicit `left <= right` expression into
/// `left - right <= 0`, moving constants to the right-hand side.
pub(crate) fn normalize_less_equal(
    left: &AffineExpression,
    right: &AffineExpression,
    check: &mut AffineCheckState,
) -> Result<AffineInequality, AffineCheckError> {
    normalize_bounded_less_equal(left, right, 0, check)
}

/// Normalizes one explicit `left - right <= bound` expression after both
/// source expressions have been formed in the written direction.
///
/// Ordered-root checking uses this after applying the root's fixed direction:
/// `a >= b` reaches this function as `b - a <= 0`, while `a > b`
/// reaches it as `b - a <= -1`. This prevents a discarded forward direction
/// from deciding formation success at an `i128` boundary.
pub(crate) fn normalize_bounded_less_equal(
    left: &AffineExpression,
    right: &AffineExpression,
    bound: i128,
    check: &mut AffineCheckState,
) -> Result<AffineInequality, AffineCheckError> {
    let mut scheduled_nodes = 0_usize;
    // Form every child before applying its parent operation. In particular,
    // an outer multiplication by zero cannot erase an overflowing inner
    // coefficient formation: the inner source expression must be valid on
    // its own before the zero is applied.
    let left = normalize_expression(left, check, &mut scheduled_nodes)?;
    let right = normalize_expression(right, check, &mut scheduled_nodes)?;
    AffineInequality::from_bounded_forms(&left, &right, bound, check)
}

#[derive(Clone, Copy)]
enum NormalizeExpression<'expression> {
    Visit(&'expression AffineExpression),
    Add,
    Subtract,
    MultiplyByConstant(i128),
}

fn normalize_expression(
    expression: &AffineExpression,
    check: &mut AffineCheckState,
    scheduled_nodes: &mut usize,
) -> Result<AffineForm, AffineCheckError> {
    let mut pending = vec![NormalizeExpression::Visit(expression)];
    let mut values = Vec::new();
    while let Some(next) = pending.pop() {
        match next {
            NormalizeExpression::Visit(expression) => {
                if *scheduled_nodes >= check.limits.max_expression_nodes {
                    return Err(AffineCheckError::LimitExceeded(
                        AffineCheckLimit::ExpressionNodes,
                    ));
                }
                *scheduled_nodes += 1;
                check.charge(1)?;
                match expression {
                    AffineExpression::Constant(value) => {
                        values.push(AffineForm::constant(*value));
                    }
                    AffineExpression::Term(term) => values.push(AffineForm::term(*term)),
                    AffineExpression::Add(left, right) => {
                        pending.push(NormalizeExpression::Add);
                        pending.push(NormalizeExpression::Visit(right));
                        pending.push(NormalizeExpression::Visit(left));
                    }
                    AffineExpression::Subtract(left, right) => {
                        pending.push(NormalizeExpression::Subtract);
                        pending.push(NormalizeExpression::Visit(right));
                        pending.push(NormalizeExpression::Visit(left));
                    }
                    AffineExpression::MultiplyByConstant { constant, value } => {
                        pending.push(NormalizeExpression::MultiplyByConstant(*constant));
                        pending.push(NormalizeExpression::Visit(value));
                    }
                }
            }
            NormalizeExpression::Add => {
                let right = values.pop().ok_or(AffineCheckError::CoefficientMismatch)?;
                let left = values.pop().ok_or(AffineCheckError::CoefficientMismatch)?;
                values.push(left.add(&right, check)?);
            }
            NormalizeExpression::Subtract => {
                let right = values.pop().ok_or(AffineCheckError::CoefficientMismatch)?;
                let left = values.pop().ok_or(AffineCheckError::CoefficientMismatch)?;
                values.push(left.subtract(&right, check)?);
            }
            NormalizeExpression::MultiplyByConstant(constant) => {
                let value = values.pop().ok_or(AffineCheckError::CoefficientMismatch)?;
                values.push(value.scale(constant, check)?);
            }
        }
    }
    let result = values.pop().ok_or(AffineCheckError::CoefficientMismatch)?;
    if values.is_empty() {
        Ok(result)
    } else {
        Err(AffineCheckError::CoefficientMismatch)
    }
}

fn insert_coefficient(
    terms: &mut Vec<AffineCoefficient>,
    term: AffineTermId,
    coefficient: i128,
    check: &mut AffineCheckState,
) -> Result<(), AffineCheckError> {
    if coefficient == 0 {
        return Ok(());
    }

    let mut lower = 0_usize;
    let mut upper = terms.len();
    while lower < upper {
        check.charge(1)?;
        let middle = lower + (upper - lower) / 2;
        if terms[middle].term < term {
            lower = middle + 1;
        } else {
            upper = middle;
        }
    }

    if let Some(existing) = terms.get(lower)
        && existing.term == term
    {
        let combined = checked_add(existing.coefficient, coefficient)?;
        if combined == 0 {
            check.charge((terms.len() - lower) as u64)?;
            terms.remove(lower);
        } else {
            check.charge(1)?;
            terms[lower].coefficient = combined;
        }
        return Ok(());
    }

    if terms.len() >= check.limits.max_terms {
        return Err(AffineCheckError::LimitExceeded(
            AffineCheckLimit::ResultTerms,
        ));
    }
    check.charge((terms.len() - lower + 1) as u64)?;
    terms.insert(lower, AffineCoefficient { term, coefficient });
    Ok(())
}

fn merge_scaled(
    current: &[AffineCoefficient],
    parent: &[AffineCoefficient],
    factor: i128,
    check: &mut AffineCheckState,
) -> Result<Vec<AffineCoefficient>, AffineCheckError> {
    let mut merged = Vec::with_capacity(
        current
            .len()
            .checked_add(parent.len())
            .ok_or(AffineCheckError::LimitExceeded(
                AffineCheckLimit::ResultTerms,
            ))?
            .min(check.limits.max_terms),
    );
    let mut current_index = 0_usize;
    let mut parent_index = 0_usize;
    while current_index < current.len() || parent_index < parent.len() {
        check.charge(1)?;
        let coefficient = match (current.get(current_index), parent.get(parent_index)) {
            (Some(current_term), Some(parent_term)) if current_term.term < parent_term.term => {
                current_index += 1;
                *current_term
            }
            (Some(current_term), Some(parent_term)) if parent_term.term < current_term.term => {
                parent_index += 1;
                AffineCoefficient {
                    term: parent_term.term,
                    coefficient: checked_mul(parent_term.coefficient, factor)?,
                }
            }
            (Some(current_term), Some(parent_term)) => {
                current_index += 1;
                parent_index += 1;
                AffineCoefficient {
                    term: current_term.term,
                    coefficient: checked_add(
                        current_term.coefficient,
                        checked_mul(parent_term.coefficient, factor)?,
                    )?,
                }
            }
            (Some(current_term), None) => {
                current_index += 1;
                *current_term
            }
            (None, Some(parent_term)) => {
                parent_index += 1;
                AffineCoefficient {
                    term: parent_term.term,
                    coefficient: checked_mul(parent_term.coefficient, factor)?,
                }
            }
            (None, None) => break,
        };
        if coefficient.coefficient != 0 {
            if merged.len() >= check.limits.max_terms {
                return Err(AffineCheckError::LimitExceeded(
                    AffineCheckLimit::ResultTerms,
                ));
            }
            merged.push(coefficient);
        }
    }
    Ok(merged)
}

fn checked_add(left: i128, right: i128) -> Result<i128, AffineCheckError> {
    left.checked_add(right)
        .ok_or(AffineCheckError::ArithmeticOverflow)
}

fn checked_sub(left: i128, right: i128) -> Result<i128, AffineCheckError> {
    left.checked_sub(right)
        .ok_or(AffineCheckError::ArithmeticOverflow)
}

fn checked_mul(left: i128, right: i128) -> Result<i128, AffineCheckError> {
    left.checked_mul(right)
        .ok_or(AffineCheckError::ArithmeticOverflow)
}

fn checked_neg(value: i128) -> Result<i128, AffineCheckError> {
    value
        .checked_neg()
        .ok_or(AffineCheckError::ArithmeticOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn term(index: u32) -> AffineExpression {
        AffineExpression::Term(AffineTermId::from_index(index))
    }

    fn constant(value: i128) -> AffineExpression {
        AffineExpression::Constant(value)
    }

    fn add(left: AffineExpression, right: AffineExpression) -> AffineExpression {
        AffineExpression::Add(Box::new(left), Box::new(right))
    }

    fn subtract(left: AffineExpression, right: AffineExpression) -> AffineExpression {
        AffineExpression::Subtract(Box::new(left), Box::new(right))
    }

    fn multiply(constant: i128, value: AffineExpression) -> AffineExpression {
        AffineExpression::MultiplyByConstant {
            constant,
            value: Box::new(value),
        }
    }

    fn coefficients(inequality: &AffineInequality) -> Vec<(u32, i128)> {
        inequality
            .terms()
            .iter()
            .map(|coefficient| (coefficient.term().index(), coefficient.coefficient()))
            .collect()
    }

    fn inequality(terms: &[(u32, i128)], upper: i128) -> AffineInequality {
        let terms = terms
            .iter()
            .map(|&(term, coefficient)| (AffineTermId::from_index(term), coefficient))
            .collect::<Vec<_>>();
        AffineInequality::from_terms(&terms, upper, &mut AffineCheckState::new())
            .expect("test inequality")
    }

    #[test]
    fn normalization_moves_constants_orders_terms_and_removes_cancellation() {
        // sum + 5 <= 3*i + 9 becomes sum - 3*i <= 4.
        let left = add(term(7), constant(5));
        let right = add(multiply(3, term(2)), constant(9));
        let mut check = AffineCheckState::new();
        let normalized = normalize_less_equal(&left, &right, &mut check).expect("normalization");
        assert_eq!(coefficients(&normalized), vec![(2, -3), (7, 1)]);
        assert_eq!(normalized.upper(), 4);
        assert!(check.used() > 0);

        let cancelled = subtract(term(4), term(4));
        let normalized =
            normalize_less_equal(&cancelled, &constant(0), &mut AffineCheckState::new())
                .expect("cancellation");
        assert!(normalized.terms().is_empty());
    }

    #[test]
    fn canonical_inequalities_merge_and_remove_zero_coefficients() {
        let canonical = AffineInequality::from_terms(
            &[
                (AffineTermId::from_index(7), 0),
                (AffineTermId::from_index(4), 3),
                (AffineTermId::from_index(2), 1),
                (AffineTermId::from_index(4), -1),
                (AffineTermId::from_index(2), -1),
                (AffineTermId::from_index(1), 5),
            ],
            9,
            &mut AffineCheckState::new(),
        )
        .expect("canonical inequality");
        assert_eq!(coefficients(&canonical), vec![(1, 5), (4, 2)]);
        assert_eq!(canonical.upper(), 9);
    }

    #[test]
    fn exact_value_forms_follow_assignment_arithmetic_without_creating_a_fact() {
        let mut check = AffineCheckState::new();
        let sum = AffineForm::term(AffineTermId::from_index(0));
        let wide = AffineForm::term(AffineTermId::from_index(2));
        let updated = sum.add(&wide, &mut check).expect("sum + wide form");
        assert_eq!(
            updated
                .terms()
                .iter()
                .map(|term| (term.term().index(), term.coefficient()))
                .collect::<Vec<_>>(),
            vec![(0, 1), (2, 1)]
        );
        assert_eq!(updated.constant_value(), 0);

        let restored = updated
            .subtract(&wide, &mut check)
            .expect("subtract copied value");
        assert_eq!(restored, sum);
        assert_eq!(
            AffineForm::constant(7)
                .scale(-3, &mut check)
                .expect("constant scale")
                .constant_value(),
            -21
        );
    }

    #[test]
    fn one_header_invariant_leaves_the_weigh_step_as_a_type_interval() {
        // Header: sum_k <= 255*i_k.
        let header = inequality(&[(0, 1), (1, -255)], 0);
        // After the body and hidden update:
        // sum_k + wide <= 255*(i_k + 1).
        let next = inequality(&[(0, 1), (1, -255), (2, 1)], 255);
        let mut check = AffineCheckState::new();
        let residual = AffineInequality::residual_after(&next, &header, &mut check)
            .expect("fixed coefficient-one residual");
        assert_eq!(coefficients(&residual), vec![(2, 1)]);
        assert_eq!(residual.upper(), 255);
        assert!(
            interval_proves(
                &residual,
                |term| (term.index() == 2).then_some((0, 255)),
                &mut check,
            )
            .expect("u8 interval rule")
        );
    }

    #[test]
    fn explicit_sum_certificate_composes_the_weigh_limit_exactly() {
        // sum - 255*count <= 0
        let per_byte = inequality(&[(0, 1), (1, -255)], 0);
        // 255*count <= 255000
        let count_limit = inequality(&[(1, 255)], 255_000);
        // sum <= 255000
        let target = inequality(&[(0, 1)], 255_000);

        assert_eq!(
            sum_explicit_inequalities(&[per_byte, count_limit], &mut AffineCheckState::new(),),
            Ok(target)
        );
    }

    #[test]
    fn explicit_scaled_sum_uses_only_the_written_positive_factors() {
        // i <= count, scaled by the written factor 255.
        let count_limit = inequality(&[(0, 1), (1, -1)], 0);
        // wide <= 255, used with the omitted factor 1.
        let byte_limit = inequality(&[(2, 1)], 255);
        let uses = [
            ScaledAffinePremise {
                inequality: &count_limit,
                factor: 255,
            },
            ScaledAffinePremise {
                inequality: &byte_limit,
                factor: 1,
            },
        ];

        assert_eq!(
            sum_explicit_scaled_inequalities(&uses, &mut AffineCheckState::new()),
            Ok(inequality(&[(0, 255), (1, -255), (2, 1)], 255))
        );
    }

    #[test]
    fn explicit_scaled_sum_rejects_nonpositive_internal_factors() {
        let premise = inequality(&[(0, 1)], 7);
        for factor in [0, -1] {
            assert_eq!(
                sum_explicit_scaled_inequalities(
                    &[ScaledAffinePremise {
                        inequality: &premise,
                        factor,
                    }],
                    &mut AffineCheckState::new(),
                ),
                Err(AffineCheckError::InvalidCertificateFactor)
            );
        }
    }

    #[test]
    fn explicit_scaled_sum_reports_checked_multiplier_overflow() {
        let premise = inequality(&[(0, 1)], i128::MAX);
        assert_eq!(
            sum_explicit_scaled_inequalities(
                &[ScaledAffinePremise {
                    inequality: &premise,
                    factor: 2,
                }],
                &mut AffineCheckState::new(),
            ),
            Err(AffineCheckError::ArithmeticOverflow)
        );
    }

    #[test]
    fn explicit_sum_uses_the_supplied_order_without_changing_the_mathematical_sum() {
        let per_byte = inequality(&[(0, 1), (1, -255)], 0);
        let count_limit = inequality(&[(1, 255)], 255_000);
        let target = inequality(&[(0, 1)], 255_000);

        assert_eq!(
            sum_explicit_inequalities(
                &[per_byte.clone(), count_limit.clone()],
                &mut AffineCheckState::new(),
            ),
            Ok(target.clone())
        );
        assert_eq!(
            sum_explicit_inequalities(&[count_limit, per_byte], &mut AffineCheckState::new(),),
            Ok(target)
        );

        // These two lists have the same mathematical sum. The safe source
        // order succeeds, while the other reaches an i128 overflow at its
        // second written premise. The checker does not reassociate the list to
        // avoid that intermediate result.
        let maximum = inequality(&[], i128::MAX);
        let plus_one = inequality(&[], 1);
        let minus_one = inequality(&[], -1);
        assert_eq!(
            sum_explicit_inequalities(
                &[maximum.clone(), minus_one.clone(), plus_one.clone()],
                &mut AffineCheckState::new(),
            ),
            Ok(maximum.clone())
        );
        assert_eq!(
            sum_explicit_inequalities(
                &[maximum.clone(), plus_one, minus_one],
                &mut AffineCheckState::new(),
            ),
            Err(AffineCheckError::ArithmeticOverflow)
        );
    }

    #[test]
    fn explicit_sum_exposes_a_missing_premise_to_the_residual_checker() {
        let per_byte = inequality(&[(0, 1), (1, -255)], 0);
        let target = inequality(&[(0, 1)], 255_000);
        let formed = sum_explicit_inequalities(&[per_byte], &mut AffineCheckState::new())
            .expect("the written premise has a canonical sum");
        assert_ne!(formed, target);
        let residual =
            AffineInequality::residual_after(&target, &formed, &mut AffineCheckState::new())
                .expect("the missing component remains explicit");
        assert_eq!(coefficients(&residual), vec![(1, 255)]);
        assert_eq!(residual.upper(), 255_000);
    }

    #[test]
    fn integer_tightening_divides_the_vector_and_floors_the_bound_toward_negative_infinity() {
        let mut check = AffineCheckState::new();
        // 2*(mid - hi) <= -1 proves mid - hi <= -1. Truncation toward zero
        // would weaken that bound to 0 and lose the midpoint relation.
        let midpoint = integer_tightening(&inequality(&[(0, 2), (1, -2)], -1), 2, &mut check)
            .expect("the tightening is representable")
            .expect("two divides the doubled midpoint relation");
        assert_eq!(coefficients(&midpoint), vec![(0, 1), (1, -1)]);
        assert_eq!(midpoint.upper(), -1);

        let positive = integer_tightening(&inequality(&[(0, 3)], 5), 3, &mut check)
            .expect("the tightening is representable")
            .expect("three divides the single coefficient");
        assert_eq!(positive.upper(), 1);

        let negative = integer_tightening(&inequality(&[(0, 3)], -4), 3, &mut check)
            .expect("the tightening is representable")
            .expect("three divides the single coefficient");
        assert_eq!(negative.upper(), -2);

        assert_eq!(
            integer_tightening(&inequality(&[(0, 2)], 5), 1, &mut check),
            Ok(None),
            "a factor of one reproduces the candidate and adds nothing"
        );
        assert_eq!(
            integer_tightening(&inequality(&[], 5), 2, &mut check),
            Ok(None),
            "a constant candidate has no coefficient vector to divide"
        );
    }

    #[test]
    fn integer_tightenings_offer_the_target_multiple_before_the_coefficient_divisor() {
        let candidate = inequality(&[(0, 4), (1, -4)], 7);
        let target = inequality(&[(0, 2), (1, -2)], 3);
        let tightenings = integer_tightenings(&candidate, &target, &mut AffineCheckState::new());
        assert_eq!(tightenings.len(), 2);
        assert_eq!(coefficients(&tightenings[0]), vec![(0, 2), (1, -2)]);
        assert_eq!(tightenings[0].upper(), 3);
        assert_eq!(coefficients(&tightenings[1]), vec![(0, 1), (1, -1)]);
        assert_eq!(tightenings[1].upper(), 1);
    }

    #[test]
    fn the_floored_bound_is_the_exact_boundary_of_a_tightened_target() {
        // 3*t <= 5 proves t <= 1 over the integers and proves no smaller
        // bound, because t = 1 satisfies the candidate.
        let candidate = inequality(&[(0, 3)], 5);
        let mut check = AffineCheckState::new();
        for (target, closes) in [
            (inequality(&[(0, 1)], 1), true),
            (inequality(&[(0, 1)], 0), false),
        ] {
            let tightenings = integer_tightenings(&candidate, &target, &mut check);
            let residual = AffineInequality::residual_after(&target, &tightenings[0], &mut check)
                .expect("the tightened residual is representable");
            assert!(residual.terms().is_empty());
            assert_eq!(residual.upper() >= 0, closes);
        }
    }

    #[test]
    fn a_candidate_no_positive_factor_divides_adds_no_tightening() {
        let candidate = inequality(&[(0, 2), (1, 3)], 7);
        assert_eq!(
            integer_tightening(&candidate, 2, &mut AffineCheckState::new()),
            Ok(None),
            "two does not divide every coefficient"
        );
        assert_eq!(
            integer_tightenings(
                &candidate,
                &inequality(&[(0, 1), (1, 1)], 0),
                &mut AffineCheckState::new(),
            ),
            Vec::new(),
            "the target is no multiple of this candidate and its divisor is one"
        );
        assert_eq!(
            integer_tightenings(
                &inequality(&[(0, i128::MIN)], 0),
                &inequality(&[(0, 1)], 0),
                &mut AffineCheckState::new(),
            ),
            Vec::new(),
            "an unrepresentable divisor grants no authority"
        );
    }

    #[test]
    fn an_unrepresentable_target_multiple_does_not_suppress_the_divisor_tightening() {
        // The first coefficient pair reads the multiple 6/3 = 2, and verifying
        // that multiple against the second pair doubles a coefficient that is
        // already past i128::MAX / 2. The target-multiple candidate is
        // therefore unrepresentable, while the coefficient divisor 6 divides
        // the same candidate exactly and stays inside i128.
        let wide = i128::MAX / 8;
        let candidate = inequality(&[(0, 6), (1, 6 * wide)], 13);
        let target = inequality(&[(0, 3), (1, 6 * wide)], 5);
        assert_eq!(
            target_multiple_factor(&candidate, &target, &mut AffineCheckState::new()),
            Err(AffineCheckError::ArithmeticOverflow),
            "the multiple cannot be verified in i128"
        );

        let tightenings = integer_tightenings(&candidate, &target, &mut AffineCheckState::new());
        assert_eq!(
            tightenings.len(),
            1,
            "the skipped multiple leaves the representable divisor candidate"
        );
        assert_eq!(coefficients(&tightenings[0]), vec![(0, 1), (1, wide)]);
        assert_eq!(tightenings[0].upper(), 2);
    }

    #[test]
    fn two_unrepresentable_tightening_factors_are_an_ordinary_skip() {
        // Reading the multiple divides i128::MIN by -1, and the divisor of the
        // same candidate is the magnitude of i128::MIN. Neither factor exists
        // in i128, so both candidates are absent and the caller sees an
        // ordinary empty list rather than a checker failure.
        let candidate = inequality(&[(0, i128::MIN), (1, i128::MIN)], 0);
        let target = inequality(&[(0, -1), (1, -1)], 0);
        let mut check = AffineCheckState::new();
        assert_eq!(
            target_multiple_factor(&candidate, &target, &mut check),
            Err(AffineCheckError::ArithmeticOverflow)
        );
        assert_eq!(coefficient_gcd(&candidate, &mut check), Ok(None));
        assert_eq!(
            integer_tightenings(&candidate, &target, &mut AffineCheckState::new()),
            Vec::new()
        );
    }

    #[test]
    fn a_tightening_over_a_checker_capacity_is_skipped_rather_than_reported() {
        // The factor 2 is representable and divides the candidate exactly, but
        // the divided inequality cannot be built inside a one-term input
        // capacity. A capacity failure inside one candidate is the same skip
        // as an arithmetic one.
        let candidate = inequality(&[(0, 2), (1, 2)], 4);
        let target = inequality(&[(0, 1), (1, 1)], 1);
        let mut check = AffineCheckState::with_limits(AffineCheckLimits {
            max_input_terms: 1,
            ..AFFINE_CHECK_LIMITS
        });
        assert_eq!(
            integer_tightening(&candidate, 2, &mut check),
            Err(AffineCheckError::LimitExceeded(
                AffineCheckLimit::InputTerms
            ))
        );
        assert_eq!(
            integer_tightenings(&candidate, &target, &mut check),
            Vec::new(),
            "the one shared factor is skipped without reporting a failure"
        );
    }

    #[test]
    fn oversized_expression_cleanup_handles_every_recursive_form() {
        for shape in 0..3 {
            let mut expression = term(0);
            for _ in 0..20_000 {
                expression = match shape {
                    0 => AffineExpression::Add(Box::new(expression), Box::new(constant(0))),
                    1 => AffineExpression::Subtract(Box::new(constant(0)), Box::new(expression)),
                    _ => multiply(1, expression),
                };
            }
            assert_eq!(
                normalize_less_equal(&expression, &constant(0), &mut AffineCheckState::new()),
                Err(AffineCheckError::LimitExceeded(
                    AffineCheckLimit::ExpressionNodes
                )),
            );
            drop(expression);
        }
    }

    #[test]
    fn explicit_sum_has_a_fixed_premise_capacity() {
        let premises = [inequality(&[(0, 1)], 0), inequality(&[(1, 1)], 0)];
        let mut check = AffineCheckState::with_limits(AffineCheckLimits {
            max_certificate_premises: 1,
            ..AFFINE_CHECK_LIMITS
        });
        assert_eq!(
            sum_explicit_inequalities(&premises, &mut check),
            Err(AffineCheckError::LimitExceeded(
                AffineCheckLimit::CertificatePremises
            ))
        );
    }

    #[test]
    fn interval_rule_uses_the_endpoint_selected_by_each_coefficient_sign() {
        let target = inequality(&[(0, 3), (1, -2)], 32);
        let mut check = AffineCheckState::new();
        assert!(
            interval_proves(
                &target,
                |term| match term.index() {
                    0 => Some((0, 10)),
                    1 => Some((-1, 8)),
                    _ => None,
                },
                &mut check,
            )
            .expect("fixed interval rule")
        );
        // 3*10 - 2*(-2) = 34, so the same rule cannot prove <= 32.
        assert_eq!(
            interval_proves(
                &target,
                |term| match term.index() {
                    0 => Some((0, 10)),
                    1 => Some((-2, 8)),
                    _ => None,
                },
                &mut check,
            ),
            Ok(false)
        );
    }

    #[test]
    fn every_i128_operation_is_checked() {
        let nested_scale = multiply(i128::MAX, multiply(2, term(1)));
        assert_eq!(
            normalize_less_equal(&nested_scale, &constant(0), &mut AffineCheckState::new(),),
            Err(AffineCheckError::ArithmeticOverflow)
        );

        let coefficient_add = add(multiply(i128::MAX, term(1)), term(1));
        assert_eq!(
            normalize_less_equal(&coefficient_add, &constant(0), &mut AffineCheckState::new(),),
            Err(AffineCheckError::ArithmeticOverflow)
        );

        assert_eq!(
            normalize_less_equal(
                &constant(i128::MIN),
                &constant(0),
                &mut AffineCheckState::new(),
            ),
            Err(AffineCheckError::ArithmeticOverflow)
        );

        let negated_multiplier = multiply(i128::MIN, subtract(term(1), term(2)));
        assert_eq!(
            normalize_less_equal(
                &negated_multiplier,
                &constant(0),
                &mut AffineCheckState::new(),
            ),
            Err(AffineCheckError::ArithmeticOverflow)
        );
    }

    #[test]
    fn outer_zero_does_not_hide_an_invalid_inner_formation() {
        let overflowing_inner = multiply(i128::MAX, multiply(2, term(1)));
        let zero_times_inner = multiply(0, overflowing_inner);
        assert_eq!(
            normalize_less_equal(
                &zero_times_inner,
                &constant(0),
                &mut AffineCheckState::new(),
            ),
            Err(AffineCheckError::ArithmeticOverflow)
        );
    }

    #[test]
    fn structural_limits_reject_shape_while_cumulative_work_only_measures() {
        let limits = AffineCheckLimits {
            max_expression_nodes: 2,
            max_input_terms: 8,
            max_terms: 8,
            max_certificate_premises: 8,
        };
        let mut check = AffineCheckState::with_limits(limits);
        assert_eq!(
            normalize_less_equal(&add(term(0), term(1)), &constant(0), &mut check),
            Err(AffineCheckError::LimitExceeded(
                AffineCheckLimit::ExpressionNodes
            ))
        );

        let mut check = AffineCheckState::new();
        check
            .charge(u64::MAX)
            .expect("work measurement cannot reject");
        check
            .charge(1)
            .expect("saturated measurement cannot reject");
        let inequality =
            AffineInequality::from_terms(&[(AffineTermId::from_index(0), 1)], 0, &mut check)
                .expect("cumulative work never changes affine acceptance");
        assert_eq!(inequality.upper(), 0);
        assert_eq!(check.used(), u64::MAX);
    }
}
