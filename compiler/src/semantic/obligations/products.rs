//! Typed private encoding of the retained values defined by the parent.

use super::*;
use crate::semantic::products::{record_enum, record_struct};

record_struct!(ObligationRecord {
    rule,
    site,
    subject
});

record_enum!(ObligationSubject {
    0 => Source { family, conjunct },
    1 => CallRequirement { callee, requires_clause },
    2 => LoopInvariant,
    3 => SourceProof,
    4 => Postcondition { relation_ordinal },
});

record_enum!(RecordAnswer {
    0 => Obligation(f0),
    1 => CallGoal(f0),
    2 => LoopInvariant(f0),
    3 => SourceProof(f0),
    4 => Postcondition(f0),
    5 => Uninhabited,
});
