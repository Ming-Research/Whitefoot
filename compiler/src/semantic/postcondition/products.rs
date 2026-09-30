//! Typed private encoding of the retained values defined by the parent.

use super::*;
use crate::semantic::products::{record_enum, record_struct};

record_struct!(CheckedPostconditionSelector {
    function,
    block,
    selector,
    candidate,
    ordinal,
    variant,
    field,
    result_type
});

record_struct!(PostconditionFieldIdentity {
    declaration,
    origin
});

record_struct!(RelationTemplate {
    operation,
    operands,
    normalized
});

record_struct!(RelationTerm {
    datum,
    displacement
});

record_enum!(NormalizedRelation {
    0 => Equal,
    1 => NotEqual,
    2 => UpperBound { left, right, strict },
});

record_enum!(RelationDatum {
    0 => Result { ordinal, projections, ty },
    1 => Parameter { ordinal, projections, ty, denotation },
    2 => NamedConst { declaration, projections, ty },
    3 => Literal { value, origin },
    4 => Measure(f0, f1),
});

record_enum!(ParameterDenotation {
    0 => EntryImage,
    1 => ExitState,
    2 => EntryDatum,
});

record_enum!(PostconditionConstantOrigin {
    0 => Literal,
    1 => NamedConst(f0),
    2 => GenericNumericIdentity { type_parameter, one },
    3 => ConstGeneric { declaration },
});

record_struct!(PostconditionPlace {
    root,
    projections,
    ty
});

record_enum!(PostconditionPlaceRoot {
    0 => Parameter { ordinal },
    1 => ExitParameter { ordinal },
    2 => Result { ordinal },
});

record_struct!(SelectedPostconditionReturn { statement, values });

record_enum!(PostconditionReturnDatum {
    0 => ResultPayload { ty },
    1 => Place(f0),
    2 => Literal { value, origin },
    3 => Measure(f0, f1),
    4 => Construct { fields },
});

record_struct!(PostconditionReturnPlace {
    root,
    projections,
    ty,
    range_referent,
    source
});

record_enum!(PostconditionReturnPlaceRoot {
    0 => Binding(f0),
    1 => NamedConst(f0),
});

record_struct!(CheckedPostcondition {
    selector,
    type_substitutions,
    const_substitutions,
    relation,
    selected_returns
});
