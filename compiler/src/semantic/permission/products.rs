//! Typed private encoding of the retained values defined by the parent.

use super::*;
use crate::semantic::products::record_struct;

record_struct!(PermissionSeparationQuery {
    first,
    second,
    left,
    right,
    formations
});

record_struct!(PermissionRangeFormation {
    carrier,
    captured,
    start,
    end
});

record_struct!(PermissionSeparationProof {
    query,
    discharged,
    derivations
});
