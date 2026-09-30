//! Typed private encoding of the retained values defined by the parent.

use super::*;
use crate::semantic::products::{record_enum, record_tuple};

record_enum!(CountedCaptureSide {
    0 => Lower,
    1 => Upper,
});

record_enum!(TermKind {
    0 => Zero,
    1 => Constant(f0),
    2 => ConstParameter(f0, f1),
    3 => ResultPayload { payload, path, measure, ty },
    4 => Place(f0, f1),
    5 => Measure(f0, f1),
    6 => CountedCapture { range_path, side },
    7 => IndexCapture { capture },
    8 => CommitValue { commit_path, ty },
    9 => CallDatum { call_path, formal, projections, measure, ty },
    10 => EntryDatum { formal, projections, measure, ty },
    11 => MeasureDatum { statement, placement, ordinal, path, measure, ty },
});

record_enum!(MeasurePlacement {
    0 => Rebind,
    1 => Construct,
    2 => Destructuring,
    3 => Element,
    4 => Payload,
});

record_tuple!(TermId, 0);

record_enum!(MeasureBound {
    0 => Constant(f0),
    1 => Equal(f0),
});
