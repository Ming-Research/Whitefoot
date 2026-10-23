//! [SHARE-3] Wake tests (compiler/waiting-contexts/state-locks): the
//! condition a guard's watch registers for the object's own lock, derived
//! from the guard, the entry a statement that holds several lock units
//! registers for each unit its guard reads, and the write flag each release
//! passes. A guard is one comparison of two atoms, one Bool atom, or one call
//! over atoms [GRAM-5, GRAM-6].

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

fn is_not_ready(gate: &Gate) -> result: Bool reads(gate.ready) {{
  return bnot(gate^.ready);
}}

fn main() -> status: std::process::ExitStatus pure waits {{
  let no = False();
  let opened = Phase::Open();
  let start = Gate(turn: 0_u64, stop: 0_u64, level: 0_i32, ready: no, phase: opened, open: no);
  let state = shared_new::<Gate>(value: start);
  let mine = 1_u64;
  let yes = True();
  let shut = Phase::Shut();
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
        // Exclusive or keeps no atom polarity.
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
fn a_helper_returning_a_bool_field_keeps_the_inline_atom() {
    for (guard, comparison) in [
        ("g^.ready", IrWakeComparison::NotEqual),
        ("is_ready(gate: g)", IrWakeComparison::NotEqual),
        ("bnot(g^.ready)", IrWakeComparison::Equal),
        ("is_not_ready(gate: g)", IrWakeComparison::Equal),
    ] {
        only_test(guard, |program, test| {
            let test = test.expect("the returned field registers a test");
            assert_eq!(atoms(program, test), [(READY, comparison, Read::False)]);
            assert_eq!(test.program, [IrWakeCode::Atom(0)]);
        });
    }
}

/// `written` can satisfy the guard; changing `unrelated` alone cannot.
/// The helpers deliberately reuse the caller's names in different roles:
/// checked binding IDs are local to their function, so substitution must
/// resolve the caller's actual argument before entering another helper.
fn progress_guard(guard: &str) -> String {
    format!(
        r#"struct Progress {{
  written: u64;
  unrelated: u64;
}}

fn covered(progress: &Progress, target: u64) -> ready: Bool reads(progress.written) {{
  return progress^.written >= target;
}}

fn forwarded(p: u64, target: &Progress) -> ready: Bool reads(target.written) {{
  return covered(progress: target, target: p);
}}

fn scalar_covered(value: u64, limit: u64) -> ready: Bool pure {{
  return value >= limit;
}}

fn several_statements(progress: &Progress, target: u64) -> ready: Bool reads(progress.written) {{
  let observed = progress^.written >= target;
  return observed;
}}

fn recursive(value: u64) -> ready: Bool pure {{
  return recursive(value: value);
}}

fn main() -> status: std::process::ExitStatus pure waits {{
  let initial = Progress(written: 0_u64, unrelated: 0_u64);
  let state = shared_new::<Progress>(value: initial);
  let target = 7_u64;
  atomic p = &state when {guard} {{
    set p^.unrelated = 1_u64;
  }}
  return std::process::exit_status(code: 0_u8);
}}
"#
    )
}

#[test]
fn helper_inline_forwarded_and_scalar_guards_keep_the_same_progress_filter() {
    for guard in [
        "p^.written >= 7_u64",
        "covered(progress: p, target: 7_u64)",
        "forwarded(p: 7_u64, target: p)",
        "scalar_covered(value: p^.written, limit: 7_u64)",
    ] {
        with_ir(progress_guard(guard).as_bytes(), |program| {
            let tests = watch_tests(program);
            assert_eq!(tests.len(), 1, "{guard}: {tests:?}");
            let test = tests[0]
                .as_ref()
                .expect("each spelling keeps the progress filter");
            // The complete filter is written >= 7, with no atom over
            // unrelated: written below 7 refuses every unrelated write,
            // and written reaching 7 admits the wake.
            assert_eq!(
                atoms(program, test),
                [(0, IrWakeComparison::GreaterEqual, Read::Bits(7))],
                "{guard}"
            );
            assert_eq!(test.program, [IrWakeCode::Atom(0)], "{guard}");
        });
    }
}

#[test]
fn forwarded_helper_captures_the_callers_local_despite_binding_name_collisions() {
    for (guard, comparison) in [
        ("p^.written >= target", IrWakeComparison::GreaterEqual),
        (
            "covered(progress: p, target: target)",
            IrWakeComparison::GreaterEqual,
        ),
        (
            "forwarded(p: target, target: p)",
            IrWakeComparison::GreaterEqual,
        ),
    ] {
        with_ir(progress_guard(guard).as_bytes(), |program| {
            let tests = watch_tests(program);
            assert_eq!(tests.len(), 1, "{guard}: {tests:?}");
            let test = tests[0]
                .as_ref()
                .expect("a local actual argument stays captured");
            assert_eq!(
                atoms(program, test),
                [(0, comparison, Read::Bits(7))],
                "{guard}"
            );
            assert_eq!(test.program, [IrWakeCode::Atom(0)], "{guard}");
            let IrWakeOperand::Value(position) = test.atoms[0].operand else {
                panic!("the caller's target is a captured value: {test:?}");
            };
            assert_eq!(
                function(program, "main").value_type(test.values[position]),
                Some(IrType::Integer {
                    signed: false,
                    width: 64,
                }),
                "the captured actual argument is the caller's scalar, not its state reference"
            );
        });
    }
}

#[test]
fn a_helper_with_local_statements_registers_the_full_guard_callback() {
    with_ir(
        progress_guard("several_statements(progress: p, target: 7_u64)").as_bytes(),
        |program| {
            let tests = watch_tests(program);
            assert_eq!(tests.len(), 1, "{tests:?}");
            let test = tests[0].as_ref().expect("a full guard filter");
            let callback = test.callback.expect("statements use a callback");
            assert!(test.atoms.is_empty() && test.program.is_empty(), "{test:?}");
            assert_eq!(
                test.values.len(),
                1,
                "only the live state is captured: {test:?}"
            );
            assert!(matches!(
                function(program, "main").value_type(test.values[0]),
                Some(IrType::Address(_))
            ));
            let thunk = &program.functions()[callback as usize];
            let parameters = thunk.parameters();
            assert_eq!(
                parameters.len(),
                1,
                "the literal needs no capture: {thunk:?}"
            );
            assert!(matches!(parameters[0].1, IrType::Address(_)));
            let arguments = thunk
                .blocks()
                .iter()
                .flat_map(IrBlock::instructions)
                .find_map(|instruction| match instruction {
                    IrInstruction::Define {
                        operation:
                            IrOperation::Call {
                                function: callee,
                                arguments,
                                ..
                            },
                        ..
                    } if program.functions()[*callee as usize].name() == "several_statements" => {
                        Some(arguments)
                    }
                    _ => None,
                })
                .expect("the callback computes the source guard");
            assert_eq!(arguments.len(), 2, "state and literal target: {thunk:?}");
            assert_eq!(arguments[0], parameters[0].0);
            assert!(
                thunk.blocks().iter().flat_map(IrBlock::instructions).any(
                    |instruction| matches!(instruction,
                        IrInstruction::Define {
                            result,
                            operation: IrOperation::Constant(IrConstant::Integer { bits: 7, .. }),
                            ..
                        } if *result == arguments[1])
                ),
                "the callback defines its literal target directly: {thunk:?}"
            );
        },
    );
}

const MULTI_STATEMENT_HELPERS: &str = r#"struct Progress {
  written: u64;
  unrelated: u64;
  failure: Option<u64>;
  stopping: Bool;
}

fn reply_ready(progress: &Progress, sequence: u64) -> ready: Bool reads(progress.written), reads(progress.failure) {
  let covered = progress^.written >= sequence;
  if covered {
    return True();
  }
  match progress^.failure {
    None() => {
      return False();
    }
    Some(..) => {
      return True();
    }
  }
}

fn rebuilding_ready(progress: &Progress, sequence: u64) -> ready: Bool reads(progress.stopping), reads(progress.written), reads(progress.failure) {
  let stopping = progress^.stopping;
  if stopping {
    return True();
  }
  return reply_ready(progress: progress, sequence: sequence);
}

fn external_ready(progress: &Progress, sequence: &u64) -> ready: Bool reads(progress.written), reads(sequence) {
  let covered = progress^.written >= sequence^;
  return covered;
}

"#;

fn multi_statement_guard(guard: &str) -> String {
    format!(
        r#"{MULTI_STATEMENT_HELPERS}fn main() -> status: std::process::ExitStatus pure waits {{
  let clear = None<u64>();
  let no = False();
  let initial = Progress(written: 0_u64, unrelated: 0_u64, failure: clear, stopping: no);
  let state = shared_new::<Progress>(value: initial);
  let sequence = 19_u64;
  atomic p = &state when {guard} {{
    set p^.unrelated = 1_u64;
  }}
  return std::process::exit_status(code: 0_u8);
}}
"#
    )
}

#[test]
fn full_guard_callbacks_keep_matches_nested_calls_and_the_waiters_sequence() {
    for helper in ["reply_ready", "rebuilding_ready"] {
        let guard = format!("{helper}(progress: p, sequence: sequence)");
        with_ir(multi_statement_guard(&guard).as_bytes(), |program| {
            let tests = watch_tests(program);
            assert_eq!(tests.len(), 1);
            let test = tests[0].as_ref().expect("multi-statement guard is filtered");
            let thunk = &program.functions()[test.callback.expect("full guard callback") as usize];
            assert!(test.atoms.is_empty() && test.program.is_empty(), "{test:?}");
            assert_eq!(test.values.len(), 2, "state and scalar sequence: {test:?}");
            assert!(
                matches!(
                    function(program, "main").value_type(test.values[0]),
                    Some(IrType::Address(_))
                ),
                "the state must be read at wake time"
            );
            assert!(
                matches!(
                    definition(program, test.values[1]),
                    IrOperation::Constant(IrConstant::Integer { bits: 19, .. })
                ),
                "capture the caller's sequence, not a helper-local binding"
            );
            let parameters = thunk.parameters();
            assert_eq!(parameters.len(), 2, "{thunk:?}");
            assert_eq!(
                parameters[1].1,
                IrType::Integer {
                    signed: false,
                    width: 64,
                }
            );
            let call = thunk
                .blocks()
                .iter()
                .flat_map(IrBlock::instructions)
                .find_map(|instruction| match instruction {
                    IrInstruction::Define {
                        operation:
                            IrOperation::Call {
                                function: callee,
                                arguments,
                                ..
                            },
                        ..
                    } if program.functions()[*callee as usize].name() == helper => Some(arguments),
                    _ => None,
                })
                .expect("callback calls the actual source guard");
            assert_eq!(call.as_slice(), [parameters[0].0, parameters[1].0]);
            assert!(
                function(program, "reply_ready").blocks().len() > 1,
                "the helper keeps its if and match control flow"
            );
            assert!(
                function(program, "rebuilding_ready")
                    .blocks()
                    .iter()
                    .flat_map(IrBlock::instructions)
                    .any(|instruction| matches!(instruction,
                        IrInstruction::Define { operation: IrOperation::Call { function: callee, .. }, .. }
                            if program.functions()[*callee as usize].name() == "reply_ready")),
                "nested calls remain ordinary checked helper calls"
            );
        });
    }
}

#[test]
fn a_guard_capturing_an_external_reference_stays_unfiltered() {
    with_ir(
        multi_statement_guard("external_ready(progress: p, sequence: &sequence)").as_bytes(),
        |program| {
            let tests = watch_tests(program);
            assert_eq!(tests.len(), 1);
            assert!(
                tests[0].is_none(),
                "external references are not immutable captures: {tests:?}"
            );
        },
    );
}

/// Helpers whose bodies the callback audit must refuse, each beside the
/// `reply_ready` control of [`MULTI_STATEMENT_HELPERS`].
const REFUSED_HELPERS: &str = r#"struct Pair {
  a: u64;
  b: u64;
}

fn repeated_ready(progress: &Progress, sequence: u64) -> ready: Bool reads(progress.written) {
  let seen = progress^.written;
  let total = 0_u64;
  for (step in 0_u64..3_u64) {
    set total = total +wrap seen;
  }
  return total >= sequence;
}

fn scratch_ready(progress: &Progress, sequence: u64) -> ready: Bool reads(progress.written) {
  let scratch = box_slots_new::<u8>(capacity: 8_u64);
  let room = scratch.inner.len;
  let covered = progress^.written >= sequence;
  let empty = room == 0_u64;
  return band(covered, empty);
}

fn within_ready(progress: &Progress, limit: Pair) -> ready: Bool reads(progress.written) {
  let bound = limit.a;
  let covered = progress^.written >= bound;
  return covered;
}

"#;

/// Two guarded statements over one state: the first waits on `reply_ready`,
/// which the audit admits, the second on `refused`, with `locals` in scope.
/// Each statement registers one watch, so the pair shows that the refusal
/// comes from the second guard's own helper and not from the surrounding
/// program.
fn control_and_refused_guard(refused: &str, locals: &str) -> String {
    let locals = if locals.is_empty() {
        String::new()
    } else {
        format!("  {locals}\n")
    };
    format!(
        r#"{MULTI_STATEMENT_HELPERS}{REFUSED_HELPERS}fn main() -> status: std::process::ExitStatus pure waits {{
  let clear = None<u64>();
  let no = False();
  let initial = Progress(written: 0_u64, unrelated: 0_u64, failure: clear, stopping: no);
  let state = shared_new::<Progress>(value: initial);
  let sequence = 19_u64;
{locals}  atomic p = &state when reply_ready(progress: p, sequence: sequence) {{
    set p^.unrelated = 1_u64;
  }}
  atomic q = &state when {refused} {{
    set q^.unrelated = 2_u64;
  }}
  return std::process::exit_status(code: 0_u8);
}}
"#
    )
}

fn assert_refused_beside_control(refused: &str, locals: &str, why: &str) {
    with_ir(
        control_and_refused_guard(refused, locals).as_bytes(),
        |program| {
            let tests = watch_tests(program);
            assert_eq!(tests.len(), 2, "{tests:?}");
            assert!(
                tests[0]
                    .as_ref()
                    .is_some_and(|test| test.callback.is_some()),
                "the control guard registers a callback: {tests:?}"
            );
            assert!(tests[1].is_none(), "{why}: {tests:?}");
        },
    );
}

#[test]
fn a_helper_with_a_loop_registers_no_filter() {
    assert_refused_beside_control(
        "repeated_ready(progress: q, sequence: sequence)",
        "",
        "a loop has no bounded cost under the writer's hold",
    );
}

#[test]
fn a_helper_that_allocates_registers_no_filter() {
    assert_refused_beside_control(
        "scratch_ready(progress: q, sequence: sequence)",
        "",
        "a callback allocates nothing under the writer's hold",
    );
}

#[test]
fn a_guard_capturing_a_struct_local_registers_no_filter() {
    assert_refused_beside_control(
        "within_ready(progress: q, limit: pair)",
        "let pair = Pair(a: 19_u64, b: 4_u64);",
        "only scalar locals are captured at registration",
    );
}

#[test]
fn a_multi_statement_guard_reads_boxed_storage_length_from_the_written_state() {
    let source = br#"struct Logged {
  pending: Box<Slots<u8>>;
  written: u64;
}

fn drained(progress: &Logged, sequence: u64) -> ready: Bool reads(progress.pending), reads(progress.written) {
  let covered = progress^.written >= sequence;
  let empty = progress^.pending.inner.len == 0_u64;
  return band(covered, empty);
}

fn main() -> status: std::process::ExitStatus pure waits {
  let pending = box_slots_new::<u8>(capacity: 8_u64);
  let initial = Logged(pending: move pending, written: 0_u64);
  let state = shared_new::<Logged>(value: move initial);
  let sequence = 19_u64;
  atomic progress = &state when drained(progress: progress, sequence: sequence) {
    set progress^.written = sequence;
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_ir(source, |program| {
        let tests = watch_tests(program);
        assert_eq!(tests.len(), 1);
        let test = tests[0]
            .as_ref()
            .expect("boxed storage length remains a live read");
        let thunk = &program.functions()[test.callback.expect("full helper callback") as usize];
        assert_eq!(test.values.len(), 2, "capture only state and sequence: {test:?}");
        assert!(
            thunk.blocks().iter().flat_map(IrBlock::instructions).any(
                |instruction| matches!(instruction,
                    IrInstruction::Define { operation: IrOperation::Call { function: callee, .. }, .. }
                        if program.functions()[*callee as usize].name() == "drained")
            ),
            "the filter calls the source helper: {thunk:?}"
        );
    });
}

/// One object whose state holds a `Left` part and a `Right` part, so a guard
/// over both parts is a condition over one state, and the statement, whose
/// state holds no map, waits on the object's own lock alone.
fn two_parts(guard: &str) -> String {
    format!(
        r#"struct Left {{
  n: u64;
  ready: Bool;
}}

struct Right {{
  m: u64;
  ready: Bool;
}}

struct Pair {{
  left: Left;
  right: Right;
}}

fn main() -> status: std::process::ExitStatus pure waits {{
  let no = False();
  let first = Left(n: 0_u64, ready: no);
  let second = Right(m: 0_u64, ready: no);
  let pair = Pair(left: first, right: second);
  let state = shared_new::<Pair>(value: pair);
  atomic p = &state when {guard} {{
    set p^.left.n = p^.right.m;
  }}
  return std::process::exit_status(code: 0_u8);
}}
"#
    )
}

/// The part of [`two_parts`]'s state, `left` (0) or `right` (1), that holds
/// the scalar at `position` of `test`'s values.
fn part(program: &IrProgram, test: &IrWakeTest, position: usize) -> u32 {
    let IrOperation::ProjectAddress { address, .. } = definition(program, test.values[position])
    else {
        panic!("a part's scalar has a projected address: {test:?}");
    };
    field_index(program, address)
}

#[test]
fn a_full_guard_over_two_parts_of_the_state_registers_one_callback() {
    const HELPER: &str = r#"fn matching_parts(pair: &Pair) -> ready: Bool reads(pair.left), reads(pair.right) {
  let matching = pair^.left.n == pair^.right.m;
  if matching {
    return True();
  }
  return False();
}

"#;
    let source =
        two_parts("matching_parts(pair: p)").replace("fn main()", &format!("{HELPER}fn main()"));
    with_ir(source.as_bytes(), |program| {
        let tests = watch_tests(program);
        let [Some(test)] = tests.as_slice() else {
            panic!("the object's one watch has a test: {tests:?}");
        };
        assert!(
            test.callback.is_some() && test.atoms.is_empty() && test.values.len() == 1,
            "the callback reads both parts from the one live state: {test:?}"
        );
    });
}

#[test]
fn a_recursive_helper_stops_expansion_and_registers_no_filter() {
    with_ir(
        progress_guard("recursive(value: p^.written)").as_bytes(),
        |program| {
            let tests = watch_tests(program);
            assert_eq!(tests.len(), 1, "{tests:?}");
            assert!(
                tests[0].is_none(),
                "a call cycle must weaken to no filter: {tests:?}"
            );
        },
    );
}

#[test]
fn a_helper_reference_argument_preserves_its_projected_state_path() {
    for guard in [
        "p^.progress.written >= 7_u64",
        "covered(progress: &p^.progress, target: 7_u64)",
        "forwarded(p: 7_u64, target: &p^.progress)",
    ] {
        let source = progress_guard(guard)
            .replace(
                "fn main()",
                "struct Envelope {\n  unrelated: u64;\n  progress: Progress;\n}\n\nfn main()",
            )
            .replace(
                "let state = shared_new::<Progress>(value: initial);",
                "let envelope = Envelope(unrelated: 0_u64, progress: initial);\n  let state = shared_new::<Envelope>(value: envelope);",
            );
        with_ir(source.as_bytes(), |program| {
            let tests = watch_tests(program);
            assert_eq!(tests.len(), 1, "{guard}: {tests:?}");
            let test = tests[0]
                .as_ref()
                .expect("the projected argument keeps its filter");
            assert_eq!(
                atoms(program, test),
                [(0, IrWakeComparison::GreaterEqual, Read::Bits(7))]
            );
            assert_eq!(test.program, [IrWakeCode::Atom(0)]);
            let IrOperation::ProjectAddress { address, .. } =
                definition(program, test.values[test.atoms[0].field])
            else {
                panic!("the scalar field has a projected address: {test:?}");
            };
            assert_eq!(
                field_index(program, address),
                1,
                "the scalar is inside progress, not the envelope's unrelated field"
            );
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

/// A program whose one guarded statement waits on `guard` over the state `g`
/// of a `Gate` that holds a map beside its scalars, and writes the scalar
/// `n` and the map's entry under `k`: the statement holds two lock units, the
/// map's and then the object's own, and a false guard waits on the units it
/// reads with `WatchBegin`, one entry each and `WatchPark`.
fn map_gate(guard: &str) -> String {
    format!(
        r#"struct Gate {{
  index: ConcurrentHashMap<u8>;
  n: u64;
  ready: Bool;
}}

const key: Array<u8, 1> =[97_u8];

fn has_entries(gate: &Gate) -> result: Bool reads(gate) {{
  let count = map_count::<u8>(map: &gate^.index);
  return count != 0_u64;
}}

fn main() -> status: std::process::ExitStatus pure waits {{
  let no = False();
  let table = map_new::<u8>(capacity: 1_u64);
  let start = Gate(index: move table, n: 0_u64, ready: no);
  let state = shared_new::<Gate>(value: move start);
  let mine = 1_u64;
  let k = &key[0_u64..1_u64];
  atomic g = &state when {guard} {{
    set g^.n = mine;
    set g^.index[k] = Some<u8>(value: 1_u8);
  }}
  return std::process::exit_status(code: 0_u8);
}}
"#
    )
}

/// The watch operations `main` registers, in block order: `SharedWatch` for
/// a statement that holds only the object's own lock, otherwise `WatchBegin`,
/// an entry per unit the guard reads and `WatchPark`.
fn watch_steps(program: &IrProgram) -> Vec<&'static str> {
    function(program, "main")
        .blocks()
        .iter()
        .flat_map(|block| block.instructions())
        .filter_map(|instruction| match instruction {
            IrInstruction::Define { operation, .. } => match operation {
                IrOperation::SharedWatch { .. } => Some("single"),
                IrOperation::WatchBegin { .. } => Some("begin"),
                IrOperation::WatchObject { .. } => Some("object"),
                IrOperation::WatchTable { .. } => Some("table"),
                IrOperation::WatchPark { .. } => Some("park"),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// The field of `ready` in the state of [`map_gate`], after the map and `n`.
const MAP_GATE_READY: u32 = 2;

#[test]
fn each_guard_unit_of_one_state_gets_its_own_watch_entry() {
    use IrWakeComparison::NotEqual;
    // A guard that reads the object's own scalar and not the map waits on
    // the object's lock alone, with the condition on `ready`; the map's unit,
    // which the block holds, has no entry.
    with_ir(map_gate("g^.ready").as_bytes(), |program| {
        assert_eq!(watch_steps(program), ["begin", "object", "park"]);
        let tests = watch_tests(program);
        let [Some(test)] = tests.as_slice() else {
            panic!("the object's entry has a test: {tests:?}");
        };
        assert_eq!(
            atoms(program, test),
            [(MAP_GATE_READY, NotEqual, Read::False)]
        );
        assert_eq!(test.program, [IrWakeCode::Atom(0)]);
    });
    // A call over the whole state reads the map too: an entry for the map
    // first, in field order, then the object's, which has no condition to
    // check because the call's result relates to no field.
    with_ir(map_gate("has_entries(gate: g)").as_bytes(), |program| {
        assert_eq!(watch_steps(program), ["begin", "table", "object", "park"]);
        let tests = watch_tests(program);
        assert!(
            tests.len() == 1 && tests.iter().all(Option::is_none),
            "{tests:?}"
        );
    });
}

#[test]
fn helpers_over_two_parts_of_one_state_keep_conjuncts_and_disjuncts() {
    use IrWakeComparison::{Equal, NotEqual};
    const HELPERS: &str = r#"fn both(left: &Left, right: &Right) -> ready: Bool reads(left.ready), reads(right.ready) {
  return band(left^.ready, right^.ready);
}

fn either(left: &Left, right: &Right) -> ready: Bool reads(left.ready), reads(right.ready) {
  return bor(left^.ready, right^.ready);
}

fn matching(left: &Left, right: &Right) -> ready: Bool reads(left.n), reads(right.m) {
  return left^.n == right^.m;
}

"#;
    // Each part's `ready` is its second field; the two atoms read it under
    // `left` and under `right`, joined as the guard joins them.
    for (guard, joined) in [
        ("band(p^.left.ready, p^.right.ready)", IrWakeCode::And),
        ("both(left: &p^.left, right: &p^.right)", IrWakeCode::And),
        ("bor(p^.left.ready, p^.right.ready)", IrWakeCode::Or),
        ("either(left: &p^.left, right: &p^.right)", IrWakeCode::Or),
    ] {
        let source = two_parts(guard).replace("fn main()", &format!("{HELPERS}fn main()"));
        with_ir(source.as_bytes(), |program| {
            let tests = watch_tests(program);
            let [Some(test)] = tests.as_slice() else {
                panic!("the object's one watch has a test for {guard}: {tests:?}");
            };
            assert_eq!(
                atoms(program, test),
                [(1, NotEqual, Read::False), (1, NotEqual, Read::False)],
                "{guard}"
            );
            assert_eq!(
                test.program,
                [IrWakeCode::Atom(0), IrWakeCode::Atom(1), joined],
                "{guard}"
            );
            let parts = test
                .atoms
                .iter()
                .map(|atom| part(program, test, atom.field))
                .collect::<Vec<_>>();
            assert_eq!(parts, [0, 1], "{guard}");
        });
    }
    // A comparison of one part's field with the other's is one atom whose
    // scalar and operand are both fields of the state.
    for guard in [
        "p^.left.n == p^.right.m",
        "matching(left: &p^.left, right: &p^.right)",
    ] {
        let source = two_parts(guard).replace("fn main()", &format!("{HELPERS}fn main()"));
        with_ir(source.as_bytes(), |program| {
            let tests = watch_tests(program);
            let [Some(test)] = tests.as_slice() else {
                panic!("the object's one watch has a test for {guard}: {tests:?}");
            };
            assert_eq!(atoms(program, test), [(0, Equal, Read::Field(0))], "{guard}");
            assert_eq!(test.program, [IrWakeCode::Atom(0)], "{guard}");
            let IrWakeOperand::Field(operand) = test.atoms[0].operand else {
                panic!("the operand is a field: {test:?}");
            };
            assert_eq!(
                [part(program, test, test.atoms[0].field), part(program, test, operand)],
                [0, 1],
                "{guard}"
            );
        });
    }
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
    // A false guard over a state of two units releases the object's lock
    // unwritten although the block writes it; leaving the block releases it
    // as written: one written release of two. The map's unit is released by
    // its own operation and passes no flag.
    with_ir(map_gate("g^.ready").as_bytes(), |program| {
        let mut flags = unlock_flags(program);
        flags.sort_unstable();
        assert_eq!(flags, [false, true]);
    });
}

#[test]
fn a_write_through_a_called_helper_releases_written() {
    // The block writes the target only inside `advance`, whose row declares
    // the write, and reads it only inside `peek`: the first release passes
    // that it may have written, so it wakes watchers, and the second that
    // it wrote nothing.
    let source = br#"struct Gate {
  turn: u64;
}

fn advance(gate: &Gate) -> result: unit writes(gate) {
  set gate^.turn = 1_u64;
  return unit;
}

fn peek(gate: &Gate) -> result: u64 reads(gate) {
  return gate^.turn;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let start = Gate(turn: 0_u64);
  let state = shared_new::<Gate>(value: start);
  let seen = 0_u64;
  atomic g = &state {
    advance(gate: g);
  }
  atomic g = &state {
    set seen = peek(gate: g);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_ir(source, |program| {
        assert_eq!(unlock_flags(program), [true, false]);
    });
}
