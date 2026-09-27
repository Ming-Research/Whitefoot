//! Typed private encoding of the retained values defined by the parent.

use super::*;
use crate::semantic::products::{record_enum, record_struct, record_tuple};

record_tuple!(AffineTermId, 0);

record_struct!(AffineCoefficient { term, coefficient });

record_struct!(AffineForm { terms, constant });

record_struct!(AffineInequality { terms, upper });

record_enum!(AffineCheckLimit {
    0 => ExpressionNodes,
    1 => InputTerms,
    2 => ResultTerms,
    3 => CertificatePremises,
});

record_enum!(AffineCheckError {
    0 => ArithmeticOverflow,
    1 => LimitExceeded(f0),
    2 => CoefficientMismatch,
    3 => InvalidCertificateFactor,
});
