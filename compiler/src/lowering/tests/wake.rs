//! [SHARE-3] Wake tests (compiler/waiting-contexts/state-locks): the
//! condition a guard's watch registers for one object, derived from the
//! guard, and the write flag each release passes. A guard is one comparison
//! of two atoms, one Bool atom, or one call over atoms [GRAM-5, GRAM-6].

use super::*;
use crate::{IrConstant, IrPlaceStep, IrWakeCode, IrWakeComparison, IrWakeOperand, IrWakeTest};

/// A program whose one guarded statement waits on `guard` over the state
/// `g` of a `Gate`, with the locals `mine`, `yes` and `shut` in scope.
fn guarded(guard: &str) -> String {
    format!(
        r#"enum Phase {{
  Open();
  Shut();
}}

struct Gate {{
  turn: u64;
  stop: u64;
  level: i32;
  ready: Bool;
  phase: Phase;
  open: Bool;
}}

fn is_ready(gate: &Gate) -> result: Bool reads(gate) {{
  return gate^.ready;
}}

fn main() -> status: std::process::ExitStatus pure waits {{
  let no = False();
  let opened = Open();
  let start = Gate(turn: 0_u64, stop: 0_u64, level: 0_i32, ready: no, phase: opened, open: no);
  let state = shared_new::<Gate>(value: start);
  let mine = 1_u64;
  let yes = True();
  let shut = Shut();
  atomic g = &state when {guard} {{
    set g^.stop = mine;
    set g^.ready = yes;
    set g^.phase = shut;
  }}
  return std::process::exit_status(code: 0_u8);
}}
"#
    )
}

/// The tests every watch in `main` registers, in order.
fn watch_tests(program: &IrProgram) -> Vec<Option<IrWakeTest>> {
    function(program, "main")
        .blocks()
        .iter()
        .flat_map(|block| block.instructions())
        .filter_map(|instruction| match instruction {
            IrInstruction::Define {
                operation:
                    IrOperation::SharedWatch { test, .. } | IrOperation::WatchObject { test, .. },
                ..
            } => Some(test.clone()),
            _ => None,
        })
        .collect()
}

/// The operation defining `value` in `main`.
fn definition(program: &IrProgram, value: IrValueId) -> IrOperation {
    function(program, "main")
        .blocks()
        .iter()
        .flat_map(|block| block.instructions())
        .find_map(|instruction| match instruction {
            IrInstruction::Define {
                result, operation, ..
            } if *result == value => Some(operation.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("main defines {value:?}"))
}

/// What one atom compares, as the test reads: the field index of its
/// scalar, its comparison, and its operand as a field index, an integer
/// constant's bits, `Bool` false, or another value. An expected `Any`
/// stands for any operand that is not a field.
#[derive(Clone, Copy, Debug)]
enum Read {
    Field(u32),
    Bits(u64),
    False,
    Other,
    Any,
}

impl PartialEq for Read {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Any, Self::Field(_)) | (Self::Field(_), Self::Any) => false,
            (Self::Any, _) | (_, Self::Any) => true,
            (Self::Field(left), Self::Field(right)) => left == right,
            (Self::Bits(left), Self::Bits(right)) => left == right,
            (Self::False, Self::False) | (Self::Other, Self::Other) => true,
            _ => false,
        }
    }
}

fn field_index(program: &IrProgram, value: IrValueId) -> u32 {
    match definition(program, value) {
        IrOperation::ProjectAddress {
            projection: IrPlaceStep::Field { field, .. },
            ..
        } => field,
        other => panic!("an atom's scalar is a field address: {other:?}"),
    }
}

fn atoms(program: &IrProgram, test: &IrWakeTest) -> Vec<(u32, IrWakeComparison, Read)> {
    test.atoms
        .iter()
        .map(|atom| {
            let operand = match atom.operand {
                IrWakeOperand::Field(position) => {
                    Read::Field(field_index(program, test.values[position]))
                }
                IrWakeOperand::Value(position) => match definition(program, test.values[position])
                {
                    IrOperation::Constant(IrConstant::Integer {
                        ty: IrType::Integer { .. },
                        bits,
                    }) => Read::Bits(bits),
                    IrOperation::Constant(IrConstant::Bool(false)) => Read::False,
                    _ => Read::Other,
                },
            };
            (
                field_index(program, test.values[atom.field]),
                atom.comparison,
                operand,
            )
        })
        .collect()
}

fn only_test(guard: &str, check: impl FnOnce(&IrProgram, Option<&IrWakeTest>)) {
    with_ir(guarded(guard).as_bytes(), |program| {
        let tests = watch_tests(program);
        assert_eq!(tests.len(), 1, "one watch for `{guard}`");
        check(program, tests[0].as_ref());
    });
}

const TURN: u32 = 0;
const STOP: u32 = 1;
const LEVEL: u32 = 2;
const READY: u32 = 3;
const PHASE: u32 = 4;
const OPEN: u32 = 5;

#[test]
fn each_kept_comparison_becomes_one_atom() {
    use IrWakeComparison::{Equal, Greater, GreaterEqual, Less, NotEqual};
    for (guard, expected) in [
        // A field against a constant.
        ("g^.turn == 3_u64", (TURN, Equal, Read::Bits(3))),
        // The constant on the left: the scalar goes left, the order mirrors.
        ("3_u64 < g^.turn", (TURN, Greater, Read::Bits(3))),
        // A signed field.
        ("g^.level >= 2_i32", (LEVEL, GreaterEqual, Read::Bits(2))),
        // A local the registration captures.
        ("g^.turn != mine", (TURN, NotEqual, Read::Any)),
        // Another field of the same object.
        ("g^.turn < g^.stop", (TURN, Less, Read::Field(STOP))),
        // A bare Bool field reads as "not false".
        ("g^.ready", (READY, NotEqual, Read::False)),
        // A Bool field compared with a Bool local.
        ("g^.ready == yes", (READY, Equal, Read::Any)),
        // A tag-only enum field compared with a variant held in a local.
        ("g^.phase != shut", (PHASE, NotEqual, Read::Any)),
    ] {
        only_test(guard, |program, test| {
            let test = test.unwrap_or_else(|| panic!("`{guard}` registers a test"));
            assert_eq!(atoms(program, test), [expected], "{guard}");
            assert_eq!(test.program, [IrWakeCode::Atom(0)], "{guard}");
        });
    }
}

#[test]
fn a_captured_local_is_read_where_the_watch_registers() {
    only_test("g^.turn == mine", |program, test| {
        let test = test.expect("a comparison with a local registers a test");
        let IrWakeOperand::Value(position) = test.atoms[0].operand else {
            panic!("a local is a captured value: {test:?}");
        };
        assert!(
            !matches!(
                definition(program, test.values[position]),
                IrOperation::ProjectAddress { .. }
            ),
            "{test:?}"
        );
    });
}

#[test]
fn every_dropped_form_registers_no_test() {
    for guard in [
        // A helper call: its result has no checkable relation to the state.
        "is_ready(gate: g)",
        // Exclusive or keeps no polarity.
        "bxor(g^.ready, g^.open)",
        // A guard over locals alone reads nothing a write can change.
        "mine == 1_u64",
        "yes",
    ] {
        only_test(guard, |_, test| {
            assert!(test.is_none(), "`{guard}` registers no test: {test:?}");
        });
    }
}

#[test]
fn and_or_and_not_combine_atoms_and_dropped_parts_become_true() {
    use IrWakeComparison::{Equal, NotEqual};
    for (guard, expected, program) in [
        (
            "band(g^.ready, g^.open)",
            vec![(READY, NotEqual, Read::False), (OPEN, NotEqual, Read::False)],
            vec![IrWakeCode::Atom(0), IrWakeCode::Atom(1), IrWakeCode::And],
        ),
        (
            "bor(g^.ready, g^.open)",
            vec![(READY, NotEqual, Read::False), (OPEN, NotEqual, Read::False)],
            vec![IrWakeCode::Atom(0), IrWakeCode::Atom(1), IrWakeCode::Or],
        ),
        // A negation flips the comparison.
        (
            "bnot(g^.ready)",
            vec![(READY, Equal, Read::False)],
            vec![IrWakeCode::Atom(0)],
        ),
        // A conjunct that reads no field leaves the other.
        (
            "band(yes, g^.open)",
            vec![(OPEN, NotEqual, Read::False)],
            vec![IrWakeCode::Atom(0)],
        ),
    ] {
        only_test(guard, |ir, test| {
            let test = test.unwrap_or_else(|| panic!("`{guard}` registers a test"));
            assert_eq!(atoms(ir, test), expected, "{guard}");
            assert_eq!(test.program, program, "{guard}");
        });
    }
    // A disjunct that implies nothing makes the disjunction imply nothing.
    only_test("bor(g^.ready, yes)", |_, test| {
        assert!(test.is_none(), "{test:?}");
    });
}

/// Two targets of different types, so each is its own group and watched as
/// an object of a watch of several units.
fn two_targets(guard: &str) -> String {
    format!(
        r#"struct Left {{
  n: u64;
  ready: Bool;
}}

struct Right {{
  m: u64;
  ready: Bool;
}}

fn main() -> status: std::process::ExitStatus pure waits {{
  let no = False();
  let first = Left(n: 0_u64, ready: no);
  let second = Right(m: 0_u64, ready: no);
  let left = shared_new::<Left>(value: first);
  let right = shared_new::<Right>(value: second);
  atomic p = &left, q = &right when {guard} {{
    set p^.n = q^.m;
  }}
  return std::process::exit_status(code: 0_u8);
}}
"#
    )
}

#[test]
fn each_object_of_several_gets_the_condition_on_its_own_state() {
    use IrWakeComparison::NotEqual;
    with_ir(two_targets("band(p^.ready, q^.ready)").as_bytes(), |program| {
        let tests = watch_tests(program);
        assert_eq!(tests.len(), 2, "{tests:?}");
        for test in &tests {
            let test = test.as_ref().expect("each object has a test");
            assert_eq!(atoms(program, test), [(1, NotEqual, Read::False)]);
            assert_eq!(test.program, [IrWakeCode::Atom(0)]);
        }
        // Each test reads its own object's state, not the other's.
        let states = tests
            .iter()
            .map(|test| test.as_ref().map(|test| test.values[0]))
            .collect::<Vec<_>>();
        assert_ne!(states[0], states[1], "{tests:?}");
    });
    // A disjunction across the two objects can become true by a write to
    // either, so neither object's writes are filtered.
    with_ir(two_targets("bor(p^.ready, q^.ready)").as_bytes(), |program| {
        let tests = watch_tests(program);
        assert_eq!(tests.len(), 2, "{tests:?}");
        assert!(tests.iter().all(Option::is_none), "{tests:?}");
    });
    // A comparison of one object's field with the other's is checkable on
    // neither state alone.
    with_ir(two_targets("p^.n == q^.m").as_bytes(), |program| {
        let tests = watch_tests(program);
        assert_eq!(tests.len(), 2, "{tests:?}");
        assert!(tests.iter().all(Option::is_none), "{tests:?}");
    });
}

/// The write flag of every object release in `main`.
fn unlock_flags(program: &IrProgram) -> Vec<bool> {
    function(program, "main")
        .blocks()
        .iter()
        .flat_map(|block| block.instructions())
        .filter_map(|instruction| match instruction {
            IrInstruction::Define {
                operation: IrOperation::SharedUnlock { wrote, .. },
                ..
            } => Some(*wrote),
            _ => None,
        })
        .collect()
}

#[test]
fn a_release_passes_whether_the_statement_may_have_written() {
    let source = br#"struct Gate {
  turn: u64;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let start = Gate(turn: 0_u64);
  let state = shared_new::<Gate>(value: start);
  let seen = 0_u64;
  atomic g = &state {
    set seen = g^.turn;
  }
  atomic g = &state {
    set g^.turn = seen;
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_ir(source, |program| {
        assert_eq!(unlock_flags(program), [false, true]);
    });
    // A false guard over two objects releases both unwritten although the
    // block writes one; leaving the block releases the written one as
    // written and the read one as unwritten: one written release of four.
    with_ir(two_targets("band(p^.ready, q^.ready)").as_bytes(), |program| {
        let mut flags = unlock_flags(program);
        flags.sort_unstable();
        assert_eq!(flags, [false, false, false, true]);
    });
}
