//! Executes repair pairs and checks the harness's contradictory-state boundary.

use super::super::{CompilationFailureKind, CompilerLimits, check, compile, with_checked_program};
use crate::SourceInput;

/// Every function a source declares, by the name its `fn` introduces.
pub(super) fn declared_functions(source: &[u8]) -> Vec<String> {
    let text = std::str::from_utf8(source).expect("a pinned source is UTF-8");
    text.split("fn ")
        .skip(1)
        .filter_map(|rest| {
            let name: String = rest
                .chars()
                .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
                .collect();
            (!name.is_empty()).then_some(name)
        })
        .collect()
}

/// The judgments in `source` that succeed only in a contradictory state, or
/// the rejection that stops it.
pub(super) fn contradictory_successes(name: &str, source: &[u8]) -> Result<Vec<String>, String> {
    let functions = declared_functions(source);
    with_checked_program(
        &[SourceInput::new(name, source)],
        None,
        CompilerLimits::default(),
        |program, _| Ok(program.contradictory_successes(&functions)),
    )
    .map_err(|failure| failure.to_string())
}

/// Each pinned repair is carried out by its programs [DIAG-1]: each
/// alternative is accepted, and its construct runs in a state that is not
/// contradictory.
pub(super) fn run_pairs() {
    for pair in super::REPAIRS
        .iter()
        .chain(super::call_separations::CALL_SEPARATIONS)
        .chain(super::captured_indices::CAPTURED_INDICES)
        .chain(super::cell_consumption::CELL_CONSUMPTION)
        .chain(super::content_moves::CONTENT_MOVES)
        .chain(super::derived_measures::DERIVED_MEASURES)
        .chain(super::effect_row_categories::EFFECT_ROW_CATEGORIES)
        .chain(super::prelude_opaque::PRELUDE_OPAQUE)
        .chain(super::proof_place_spellings::PROOF_PLACE_SPELLINGS)
        .chain(super::qualified_constants::QUALIFIED_CONSTANTS)
        .chain(super::recursive_effects::RECURSIVE_EFFECTS)
        .chain(super::rewritten_effect_indices::REWRITTEN_EFFECT_INDICES)
        .chain(super::remaining_repairs::REMAINING_REPAIRS)
        .chain(super::storage_destructuring::STORAGE_DESTRUCTURING)
        .chain(super::shared_maps::SHARED_MAPS)
        .chain(super::floats::FLOATS)
        .chain(super::interface_applications::INTERFACE_APPLICATIONS)
        .chain(super::lost_header_relations::LOST_HEADER_RELATIONS)
        .chain(super::lexical_syntax::LEXICAL_SYNTAX)
        .chain(super::qualified_names::QUALIFIED_NAMES)
        .chain(super::selector_scope::SELECTOR_SCOPE)
        .chain(super::enum_routed_ensures::ENUM_ROUTED_ENSURES)
        .chain(super::entry_formers::ENTRY_FORMERS)
        .chain(super::collisions_and_killed_facts::COLLISIONS_AND_KILLED_FACTS)
        .chain(super::unavailable_summaries::UNAVAILABLE_SUMMARIES)
        .chain(super::redundant_conditions::REDUNDANT_CONDITIONS)
        .chain(super::clause_requirements::CLAUSE_REQUIREMENTS)
        .chain(super::writer_operations::WRITER_OPERATIONS)
        .chain(super::writer_ownership::WRITER_OWNERSHIP)
        .chain(super::writer_references::WRITER_REFERENCES)
        .chain(super::postcondition_places::POSTCONDITION_PLACES)
        .chain(super::common_syntax::COMMON_SYNTAX)
    {
        let failure = compile(
            &[SourceInput::new(pair.name, pair.rejected)],
            CompilerLimits::default(),
        )
        .expect_err(pair.name);
        super::entry_formers::assert_span(pair, &failure);
        assert_eq!(
            failure.kind(),
            CompilationFailureKind::Source,
            "{}: {failure}",
            pair.name
        );
        assert_eq!(
            failure.rule_id(),
            Some(pair.rule),
            "{}: {failure}",
            pair.name
        );
        // [TYPE-9] semantic tests pin the complete call or written targ
        // span. Pin its rendered start here with the existing repair run.
        let citation = match pair.name {
            "type9-neg-runtime-slots-content-swap.wf" => {
                Some(("  ", "swap(first: &small.inner, second: &large.inner)"))
            }
            "type9-neg-generic-runtime-content-swap.wf" => Some(("exchange::<", "Slots<u64>")),
            "type9-neg-function-actual-runtime-content-swap.wf" => {
                Some(("fn swap::<", "Slots<u64>"))
            }
            _ => None,
        };
        if let Some((prefix, cited)) = citation {
            let source = std::str::from_utf8(pair.rejected).expect("source is text");
            let start = source
                .find(&format!("{prefix}{cited}"))
                .expect("refused construct")
                + prefix.len();
            let before = &source[..start];
            let line = before.bytes().filter(|byte| *byte == b'\n').count() as u64 + 1;
            let column = before
                .rsplit('\n')
                .next()
                .expect("source line")
                .chars()
                .count() as u64
                + 1;
            let location = failure.location().expect("source location");
            assert_eq!(location.path(), pair.name);
            assert_eq!((location.line(), location.column()), (line, column));
        }
        let rendered = format!("{failure}\n");
        if pair.sentences.is_empty() {
            assert!(
                !rendered.contains("\n  mechanical_fix:"),
                "{}: this position must not offer an unverified repair: {rendered}",
                pair.name,
            );
        }
        for sentence in pair.sentences {
            assert!(
                rendered.contains(sentence),
                "{}: the rejection no longer carries this repair.\nwanted: {sentence}\ngot:    {rendered}",
                pair.name,
            );
        }
        for (alternative, source) in pair.repaired.iter().enumerate() {
            match contradictory_successes(pair.name, source) {
                Ok(contradictions) => assert!(
                    contradictions.is_empty(),
                    "{}: alternative {alternative} succeeds only where its state is contradictory: {contradictions:?}",
                    pair.name
                ),
                Err(rejection) => panic!(
                    "{}: alternative {alternative} is rejected:\n{rejection}",
                    pair.name
                ),
            }
        }
    }
}

/// An allocation whose count is bounded only by its type: `u64::MAX` for a
/// `u8` element, which no supported target can allocate. This is the program
/// a writer met in an image decoder, where the count came from the image's
/// dimensions.
const UNBOUNDED_TARGET_COUNT: &[u8] = br#"alias ExitStatus = std::process::ExitStatus;
alias Inputs = std::process::Inputs;
alias args_count = std::text::args_count;
alias close_directory = std::fs::close_directory;
alias exit_status = std::process::exit_status;

fn make(count: u64) -> made: Box<Slots<u8>> pure {
  let cell = box_slots_new::<u8>(capacity: count);
  return move cell;
}

fn main(inputs: Inputs) -> status: ExitStatus pure waits {
  let Inputs(args: args, drivers: unused_driver_count, cwd: cwd_directory, stdout: out, stderr: err, handles: files, stdin: unused, clock: unused_clock, wall_clock: unused_wall_clock, stops: unused_stops, memory_meter: unused_memory_meter, host_paths: unused_host_paths, process_id: unused_process_id) = move inputs;
  let std::fs::Directory(read: cwd, write: cwd_write) = move cwd_directory;
  std::fs::close_directory_write(factory: &files, directory: move cwd_write);
  close_directory(factory: &files, directory: move cwd);
  let n = args_count(args: &args);
  let cell = make(count: n);
  let cap = cell.inner.cap;
  let code = cvt.wrap::<u64, u8>(cap);
  return exit_status(code: code);
}
"#;

/// [OP-9] the image decoder's program builds: its count carries no static
/// bound, and a size the target cannot allocate is heap exhaustion at run
/// time rather than a target-layout stop. The OP-9 repair pairs, the
/// target's count-limit stop and the tests that carried both out retired
/// with v0.87's [OP-9]; the run-time check is observed by the backend
/// exhaustion tests.
#[test]
fn an_allocation_count_bounded_only_by_its_type_builds() {
    check(
        &[SourceInput::new("unbounded.wf", UNBOUNDED_TARGET_COUNT)],
        CompilerLimits::default(),
    )
    .expect("the count carries no static bound");
    if let Err(failure) = compile(
        &[SourceInput::new("unbounded.wf", UNBOUNDED_TARGET_COUNT)],
        CompilerLimits::default(),
    ) {
        panic!("a count bounded only by its type does not build:\n{failure}");
    }
}
