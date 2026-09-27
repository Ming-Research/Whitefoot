//! Full-row extraction locks for the [OP-1] operation table and the [MSR-1]
//! measure table.
//!
//! The `op` column has been locked to the compiler's family inventory for
//! several versions (`resolution::catalog`), but `domain`, `signature`, and
//! `effects` were re-encoded by hand in `semantic::model` and nothing compared
//! them with the specification. The measured consequence of a one-sided mirror
//! was sixteen unreachable match arms naming operation spellings the table had
//! stopped containing; the same class covers an effect classification, a result
//! type, or an operand domain drifting from its cell in silence.
//!
//! These locks close that in both directions. Extraction keys on the
//! ```` ```wf-ops ```` fence info string rather than on a prose anchor, so
//! moving the table inside its rule cannot silently empty the extraction. The
//! variant-to-spelling maps are exhaustive matches with no wildcard arm, so a
//! new compiler operation cannot be added without naming its row, and a row
//! whose spelling matches no operation fails the coverage assertion.
//!
//! A green run establishes that every modelled row's effect classification,
//! result type, operand count, argument types, and operand domain agree with
//! the specification's own cells, and that the modelled spellings and the
//! table's spellings are the same set. It does not establish that the
//! operations compute the right values — that is the business of the ordinary
//! semantic and backend tests — and it says nothing about the rows this module
//! deliberately does not model (see `UNMODELLED_ROW_SPELLINGS`).

use std::collections::{BTreeSet, HashSet};

use crate::semantic::model::{
    CheckedFloatOperation, CheckedIntegerErrorClass, CheckedIntegerOperation, CheckedMeasure,
    CheckedType, FloatType, IntegerType, MeasureCell, MeasuredKind,
};

/// One extracted row of the `wf-ops` table, cell for cell.
#[derive(Debug, Eq, PartialEq)]
struct OpsRow {
    /// Every backticked spelling of the `op` cell, in written order.
    ops: Vec<String>,
    domain: String,
    signature: String,
    effects: String,
}

impl OpsRow {
    /// The parameter list of the `signature` cell, `(T, u32)` for
    /// `` `(T, u32) -> own T` ``. A nullary row writes `()`. Every v0.60 row
    /// writes one: the measure readers that wrote none are retired, a measure
    /// being a place form [OP-15] and not a table row.
    fn parameters(&self) -> Option<Vec<&str>> {
        let list = self.signature.strip_prefix('(')?.split_once(')')?.0;
        Some(if list.is_empty() {
            Vec::new()
        } else {
            list.split(", ").collect()
        })
    }

    /// The written result of the `signature` cell, `T` for
    /// `` `(T, T) -> own T` ``. Every row's result is `own`.
    fn result(&self) -> &str {
        self.signature
            .split_once("-> own ")
            .expect("every wf-ops signature writes one `own` result")
            .1
    }
}

/// The `wf-ops` table, extracted from the active specification by fence info
/// string.
fn ops_rows() -> Vec<OpsRow> {
    let mut fences = crate::ACTIVE_KERNEL_SPEC_TEXT.split("\n```wf-ops\n");
    let _before = fences.next().expect("the split always yields a first part");
    let body = fences
        .next()
        .expect("the active specification has one wf-ops fence")
        .split_once("\n```")
        .expect("the wf-ops fence is terminated")
        .0;
    assert!(
        fences.next().is_none(),
        "the wf-ops schema names exactly one table"
    );

    let mut lines = body.lines();
    assert_eq!(
        lines.next(),
        Some("| op | domain | signature | effects |"),
        "the first row of a wf-ops fence is its column schema"
    );
    assert_eq!(lines.next(), Some("|---|---|---|---|"));

    lines
        .map(|line| {
            let cells: Vec<&str> = line
                .strip_prefix("| ")
                .and_then(|rest| rest.strip_suffix(" |"))
                .expect("a wf-ops row is pipe-delimited")
                .split(" | ")
                .collect();
            let [ops, domain, signature, effects] = cells.as_slice() else {
                panic!("a wf-ops row has exactly four cells, found {}", cells.len());
            };
            let ops: Vec<String> = ops
                .split('`')
                .enumerate()
                .filter(|(index, _)| index % 2 == 1)
                .map(|(_, part)| part.to_owned())
                .collect();
            assert!(!ops.is_empty(), "a wf-ops op cell names at least one op");
            OpsRow {
                ops,
                domain: (*domain).to_owned(),
                // A signature cell's signature is its backticked span; any
                // surrounding prose is a gloss and not part of the cell.
                signature: signature
                    .split('`')
                    .nth(1)
                    .expect("a wf-ops signature cell is backticked")
                    .to_owned(),
                effects: (*effects).to_owned(),
            }
        })
        .collect()
}

/// Finds the one row that names `spelling`.
fn row_of<'rows>(rows: &'rows [OpsRow], spelling: &str) -> &'rows OpsRow {
    let mut found = rows
        .iter()
        .filter(|row| row.ops.iter().any(|op| op == spelling));
    let row = found
        .next()
        .unwrap_or_else(|| panic!("no wf-ops row names {spelling}"));
    assert!(
        found.next().is_none(),
        "{spelling} appears in more than one wf-ops row"
    );
    row
}

/// The [OP-1] spelling of each integer operation, from the compiler's own
/// exhaustive map.
///
/// The map moved to `CheckedIntegerOperation::spelling` when FN-8 began
/// rendering its goal in source terms: one map serves the diagnostic and this
/// lock, so a diagnostic can never print a spelling this test has not compared
/// with the specification table.
const fn integer_spelling(operation: CheckedIntegerOperation) -> &'static str {
    operation.spelling()
}

const INTEGER_OPERATIONS: [CheckedIntegerOperation; 54] = {
    use CheckedIntegerOperation as Op;
    [
        Op::AddWrap,
        Op::SubtractWrap,
        Op::MultiplyWrap,
        Op::AddExact,
        Op::SubtractExact,
        Op::MultiplyExact,
        Op::AddDefined,
        Op::SubtractDefined,
        Op::MultiplyDefined,
        Op::AddChecked,
        Op::SubtractChecked,
        Op::MultiplyChecked,
        Op::DivideExact,
        Op::RemainderExact,
        Op::DivideDefined,
        Op::RemainderDefined,
        Op::DivideChecked,
        Op::RemainderChecked,
        Op::AbsoluteWrap,
        Op::AbsoluteExact,
        Op::AbsoluteDefined,
        Op::AbsoluteChecked,
        Op::NegateWrap,
        Op::NegateExact,
        Op::NegateDefined,
        Op::NegateChecked,
        Op::BitAnd,
        Op::BitOr,
        Op::BitXor,
        Op::BitNot,
        Op::ShiftLeftWrap,
        Op::ShiftRightWrap,
        Op::ShiftLeftExact,
        Op::ShiftRightExact,
        Op::ShiftLeftDefined,
        Op::ShiftRightDefined,
        Op::RotateLeft,
        Op::RotateRight,
        Op::PopulationCount,
        Op::LeadingZeros,
        Op::TrailingZeros,
        Op::ByteSwap,
        Op::MultiplyHigh,
        Op::AddSaturating,
        Op::SubtractSaturating,
        Op::MultiplySaturating,
        Op::Minimum,
        Op::Maximum,
        Op::Equal,
        Op::NotEqual,
        Op::Less,
        Op::LessEqual,
        Op::Greater,
        Op::GreaterEqual,
    ]
};

/// The [OP-1] spelling of each float operation, from the compiler's own
/// exhaustive map.
const fn float_spelling(operation: CheckedFloatOperation) -> &'static str {
    operation.spelling()
}

const FLOAT_OPERATIONS: [CheckedFloatOperation; 24] = {
    use CheckedFloatOperation as Op;
    [
        Op::AddStrict,
        Op::SubtractStrict,
        Op::MultiplyStrict,
        Op::DivideStrict,
        Op::Equal,
        Op::Less,
        Op::LessEqual,
        Op::Greater,
        Op::GreaterEqual,
        Op::NotEqual,
        Op::Negate,
        Op::Absolute,
        Op::CopySign,
        Op::Minimum,
        Op::Maximum,
        Op::Floor,
        Op::Ceil,
        Op::Truncate,
        Op::RoundEven,
        Op::Remainder,
        Op::SquareRootStrict,
        Op::FusedMultiplyAddStrict,
        Op::Infinity,
        Op::Nan,
    ]
};

/// The four Bool rows. Their arity is decided at the call site rather than by
/// an accessor, so the lock covers their cells and arity only.
const BOOLEAN_SPELLINGS: [(&str, usize); 4] = [("band", 2), ("bor", 2), ("bxor", 2), ("bnot", 1)];

/// The table spellings no scalar-operation model covers.
///
/// Each is checked by its own rule's tests rather than by a scalar-operation
/// accessor: the conversions carry written type pairs, and the two tag
/// comparisons take one exact nominal tag-only enum rather than a numeric
/// domain. Listing them explicitly is what makes the coverage assertion below
/// two-sided — a new row is a failure unless someone decides which side it
/// belongs on.
///
/// v0.60 removed twelve of the sixteen rows this list used to carry. The
/// measure readers `len_of`, `cap_of` and `head_of` are retired:
/// a measure is a `psuffix` field selection on a measured place and is not a
/// call [OP-15, TYPE-10]. The views `slice_of` and `mut_slice_of` are retired
/// with views, a range reference being formed by `&x[lo..hi]` [REF-4]. The
/// constructions `box_new`, `arena_new`, `array_new` and `buffer_new`, and
/// the allocation-fit predicates `buffer_fits` and `buffer_vacant`, are
/// retired from the table: construction is the closed set of [PRE-1]
/// construction functions [OP-13], which are ordinary records and not
/// operation-table rows, and allocation is total so no source predicate
/// decides it [STOR-8].
const UNMODELLED_ROW_SPELLINGS: [&str; 7] = [
    "eeq",
    "ene",
    "cvt",
    "cvt.checked",
    "cvt.defined",
    "cvt.wrap",
    "reinterpret",
];

const ALL_INTEGER_TYPES: [IntegerType; 8] = [
    IntegerType::I8,
    IntegerType::I16,
    IntegerType::I32,
    IntegerType::I64,
    IntegerType::U8,
    IntegerType::U16,
    IntegerType::U32,
    IntegerType::U64,
];

/// Every table spelling is either modelled by a scalar operation or explicitly
/// excluded, and nothing is both.
#[test]
fn the_wf_ops_table_and_the_compilers_operations_name_the_same_spellings() {
    let rows = ops_rows();
    let table: Vec<String> = rows.iter().flat_map(|row| row.ops.clone()).collect();
    // Each conversion mode has its own family and uniform result shape.
    let distinct: BTreeSet<&String> = table.iter().collect();
    assert_eq!(
        table.len() - distinct.len(),
        0,
        "every wf-ops row now has a distinct family spelling"
    );

    let mut modelled: BTreeSet<String> = BTreeSet::new();
    for operation in INTEGER_OPERATIONS {
        assert!(
            modelled.insert(integer_spelling(operation).to_owned()),
            "{operation:?} repeats a spelling"
        );
    }
    for operation in FLOAT_OPERATIONS {
        assert!(
            modelled.insert(float_spelling(operation).to_owned()),
            "{operation:?} repeats a spelling"
        );
    }
    for (spelling, _) in BOOLEAN_SPELLINGS {
        assert!(modelled.insert(spelling.to_owned()));
    }
    for spelling in UNMODELLED_ROW_SPELLINGS {
        assert!(
            modelled.insert(spelling.to_owned()),
            "{spelling} is both modelled and excluded"
        );
    }

    let table: BTreeSet<String> = distinct.into_iter().cloned().collect();
    assert_eq!(
        table, modelled,
        "the wf-ops op column and the compiler's operations disagree"
    );
}

/// Every integer row is statically total or proof-required and therefore pure.
#[test]
fn the_effects_column_keeps_every_modelled_row_pure() {
    let rows = ops_rows();
    for operation in INTEGER_OPERATIONS {
        let spelling = integer_spelling(operation);
        let row = row_of(&rows, spelling);
        assert_eq!(row.effects, "pure", "{spelling}");
    }
    // [OP-3]: every float row rounds or is exact, and all are pure.
    for operation in FLOAT_OPERATIONS {
        let spelling = float_spelling(operation);
        assert_eq!(row_of(&rows, spelling).effects, "pure", "{spelling}");
    }
    for (spelling, _) in BOOLEAN_SPELLINGS {
        assert_eq!(row_of(&rows, spelling).effects, "pure", "{spelling}");
    }
}

/// Column 3 of every modelled row decides the operand count, the argument
/// types, and the result type.
#[test]
fn the_signature_column_decides_arity_argument_types_and_results() {
    let rows = ops_rows();
    let mut checked_rows = 0;
    for operation in INTEGER_OPERATIONS {
        let spelling = integer_spelling(operation);
        let row = row_of(&rows, spelling);
        let parameters = row
            .parameters()
            .unwrap_or_else(|| panic!("{spelling} writes a parameter list"));
        assert_eq!(
            parameters.len(),
            operation.operand_count(),
            "{spelling} is written {} but the compiler expects {} operands",
            row.signature,
            operation.operand_count()
        );

        // The selected type stands in for the row's `T`. i32 is an arbitrary
        // member of every integer domain the table writes, so it exercises the
        // substitution without standing in for a domain judgment, which the
        // domain test below makes separately.
        let selected = CheckedType::Integer(IntegerType::I32);
        for (index, written) in parameters.iter().enumerate() {
            let expected = match *written {
                "T" => selected,
                "u32" => CheckedType::Integer(IntegerType::U32),
                other => panic!("{spelling} writes an unmodelled parameter type {other}"),
            };
            assert_eq!(
                operation.argument_type(selected, index),
                Some(expected),
                "{spelling} argument {index} is written {written}"
            );
        }
        assert_eq!(operation.argument_type(selected, parameters.len()), None);

        match row.result() {
            "T" => assert_eq!(operation.scalar_result_type(selected), Some(selected)),
            "Bool" => assert_eq!(
                operation.scalar_result_type(selected),
                Some(CheckedType::Bool)
            ),
            "u32" => assert_eq!(
                operation.scalar_result_type(selected),
                Some(CheckedType::Integer(IntegerType::U32))
            ),
            written => {
                let error = written
                    .strip_prefix("Result<T, ")
                    .and_then(|rest| rest.strip_suffix('>'))
                    .unwrap_or_else(|| panic!("{spelling} writes an unmodelled result {written}"));
                // A `Result` row is exactly a row with no scalar result, and
                // its error type is the cell's own spelling.
                assert_eq!(
                    operation.scalar_result_type(selected),
                    None,
                    "{spelling} produces {written} yet reports a scalar result"
                );
                assert_eq!(
                    operation
                        .checked_error()
                        .map(CheckedIntegerErrorClass::spelling),
                    Some(error),
                    "{spelling} is written {written}"
                );
                checked_rows += 1;
            }
        }
        if !row.result().starts_with("Result<") {
            assert_eq!(
                operation.checked_error(),
                None,
                "{spelling} has a scalar result yet names a checked error"
            );
        }
    }
    // Seven checked integer operations: add, subtract, multiply, divide,
    // remainder, negate, absolute.
    assert_eq!(checked_rows, 7);

    for operation in FLOAT_OPERATIONS {
        let spelling = float_spelling(operation);
        let row = row_of(&rows, spelling);
        let parameters = row
            .parameters()
            .unwrap_or_else(|| panic!("{spelling} writes a parameter list"));
        assert_eq!(
            parameters.len(),
            operation.operand_count(),
            "{spelling} is written {}",
            row.signature
        );
        assert!(
            parameters.iter().all(|written| *written == "T"),
            "{spelling} writes a non-T float parameter"
        );
        let selected = CheckedType::Float(FloatType::F64);
        let expected = match row.result() {
            "T" => selected,
            "Bool" => CheckedType::Bool,
            written => panic!("{spelling} writes an unmodelled result {written}"),
        };
        assert_eq!(operation.result_type(selected), expected, "{spelling}");
    }

    for (spelling, arity) in BOOLEAN_SPELLINGS {
        let row = row_of(&rows, spelling);
        let parameters = row
            .parameters()
            .expect("a Bool row writes a parameter list");
        assert_eq!(parameters.len(), arity, "{spelling}");
        assert!(parameters.iter().all(|written| *written == "Bool"));
        assert_eq!(row.result(), "Bool", "{spelling}");
    }
}

/// Column 2 of every modelled integer row decides which operand types the
/// operation accepts.
///
/// The domain tokens are sampled at their boundaries rather than swept: `i8`
/// versus `i16` is the whole content of `width>=16`, and one signed and one
/// unsigned type is the whole content of `signed int T`. Bool stands in for
/// every non-integer operand, which every integer row must refuse.
#[test]
fn the_domain_column_decides_which_operand_types_are_accepted() {
    let rows = ops_rows();
    let mut domains: HashSet<&str> = HashSet::new();
    for operation in INTEGER_OPERATIONS {
        let spelling = integer_spelling(operation);
        let row = row_of(&rows, spelling);
        domains.insert(row.domain.as_str());
        let accepted: Vec<IntegerType> = ALL_INTEGER_TYPES
            .into_iter()
            .filter(|integer| operation.accepts_operand_type(CheckedType::Integer(*integer)))
            .collect();
        let expected: Vec<IntegerType> = match row.domain.as_str() {
            "all int T" => ALL_INTEGER_TYPES.to_vec(),
            "signed int T" => ALL_INTEGER_TYPES
                .into_iter()
                .filter(|integer| integer.signed())
                .collect(),
            "int T, width>=16" => ALL_INTEGER_TYPES
                .into_iter()
                .filter(|integer| integer.width() >= 16)
                .collect(),
            other => panic!("{spelling} writes an unmodelled domain {other}"),
        };
        assert_eq!(accepted, expected, "{spelling}'s domain is {}", row.domain);
        assert!(
            !operation.accepts_operand_type(CheckedType::Bool),
            "{spelling} accepts Bool, which no integer domain admits"
        );
    }
    // All three integer domain tokens are exercised; a table that collapsed to
    // one token would otherwise pass every assertion above.
    assert_eq!(domains.len(), 3);

    for operation in FLOAT_OPERATIONS {
        let spelling = float_spelling(operation);
        assert_eq!(row_of(&rows, spelling).domain, "f32 f64", "{spelling}");
    }
    for (spelling, _) in BOOLEAN_SPELLINGS {
        assert_eq!(row_of(&rows, spelling).domain, "Bool", "{spelling}");
    }
}

/// One extracted row of the `wf-measures` table [MSR-1].
#[derive(Debug, Eq, PartialEq)]
struct MeasureRow {
    measured: String,
    cells: Vec<String>,
}

/// [MSR-1]'s measure table, extracted from the active specification by fence
/// info string exactly as the operation table above is.
fn measure_rows() -> Vec<MeasureRow> {
    let mut fences = crate::ACTIVE_KERNEL_SPEC_TEXT.split("\n```wf-measures\n");
    let _ = fences.next();
    let body = fences
        .next()
        .expect("the active specification has one wf-measures fence")
        .split("\n```")
        .next()
        .expect("the wf-measures fence is terminated");
    assert!(
        fences.next().is_none(),
        "the wf-measures schema names exactly one table"
    );
    let mut lines = body.lines().filter(|line| line.starts_with('|'));
    let schema = lines.next().expect("the table has a column schema");
    assert_eq!(
        schema
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect::<Vec<_>>(),
        vec!["measured type", "len", "cap", "head"],
        "the first row of a wf-measures fence is its column schema"
    );
    lines
        .filter(|line| !line.trim_matches('|').starts_with('-'))
        .map(|line| {
            let mut cells = line
                .trim_matches('|')
                .split('|')
                .map(|cell| cell.trim().to_owned())
                .collect::<Vec<_>>();
            assert_eq!(cells.len(), 4, "a wf-measures row has exactly four cells");
            let measured = cells.remove(0);
            MeasureRow { measured, cells }
        })
        .collect()
}

/// The measure table the compiler reads is the one the specification writes.
///
/// The compiler's table is code [`CheckedMeasure::cell`], and [MSR-1] says the
/// table is data the rule requires to exist rather than rule text. Nothing
/// else compares the two, and a cell that drifts is exactly the class of
/// defect the operation-table lock above was added for: a measure would keep
/// proving a standing fact the specification no longer states.
#[test]
fn the_wf_measures_table_and_the_compilers_measure_table_agree() {
    let rows = measure_rows();
    // [MSR-1]'s seven rows are seven identities: the two placements of one
    // shape answer `cap` differently, so a constant-capacity row and a
    // runtime-capacity row of the same shape are two rows and two kinds
    // [TYPE-9].
    let expected = [
        (MeasuredKind::ConstantArray, "Array<T, N>"),
        (MeasuredKind::RuntimeArray, "Array<T>"),
        (MeasuredKind::ConstantSlots, "Slots<T, N>"),
        (MeasuredKind::RuntimeSlots, "Slots<T>"),
        (MeasuredKind::ConstantRing, "Ring<T, N>"),
        (MeasuredKind::RuntimeRing, "Ring<T>"),
        (MeasuredKind::Range, "&[T]"),
    ];
    assert_eq!(
        rows.iter()
            .map(|row| row.measured.as_str())
            .collect::<Vec<_>>(),
        expected.iter().map(|(_, name)| *name).collect::<Vec<_>>(),
        "the measure table gives every measured type this version has a row"
    );
    let measures = [
        CheckedMeasure::Length,
        CheckedMeasure::Capacity,
        CheckedMeasure::Head,
    ];
    // [MSR-1]: exactly one cell class is *bounded* anywhere, and it is the one
    // cell the two `Ring` rows share.
    let mut bounded = 0_usize;
    for (row, (measured, name)) in rows.iter().zip(expected) {
        for (cell, measure) in row.cells.iter().zip(measures) {
            let (written, classification) = cell
                .rsplit_once(", ")
                .unwrap_or((cell.as_str(), cell.as_str()));
            let compiled = measure.cell(measured);
            assert_eq!(
                classification,
                compiled.classification(),
                "{name}'s {} cell writes {classification} where the compiler reads {compiled:?}",
                measure.spelling()
            );
            match compiled {
                MeasureCell::ExactConstant(value) => assert_eq!(
                    written,
                    value.to_string(),
                    "{name}'s {} cell is the constant the compiler folds",
                    measure.spelling()
                ),
                MeasureCell::ExactTypeConstant => assert_eq!(
                    written,
                    "N",
                    "{name}'s {} cell is the type's own written constant",
                    measure.spelling()
                ),
                // No v0.60 row reads a measured value's own extent as a
                // separate class, so `ExactExtent` shares the runtime arm:
                // a row that reaches it still has to write one of these
                // runtime quantities.
                MeasureCell::ExactExtent | MeasureCell::ExactRuntime => assert!(
                    matches!(
                        written,
                        "allocated slots"
                            | "len"
                            | "initialized slots"
                            | "slots taken"
                            | "range elements"
                    ),
                    "{name}'s {} cell is a runtime quantity of the block, written {written}",
                    measure.spelling()
                ),
                MeasureCell::Bounded => {
                    bounded += 1;
                    assert_eq!(
                        written, "window origin",
                        "the one bounded cell class is a Ring's window origin"
                    );
                }
                MeasureCell::Absent => assert_eq!(
                    written,
                    "absent",
                    "{name}'s {} cell is absent in both",
                    measure.spelling()
                ),
            }
        }
    }
    assert_eq!(
        bounded, 2,
        "the two Ring rows share the one bounded cell and nothing else is bounded"
    );
}
