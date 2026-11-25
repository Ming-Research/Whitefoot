//! Executes repair pairs and checks the harness's contradictory-state boundary.

use std::collections::BTreeSet;
use std::sync::OnceLock;

use super::super::{CompilationFailureKind, CompilerLimits, check, compile, with_checked_program};
use super::RepairPair;
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

/// Every pinned pair, in table order.
fn all_pairs() -> impl Iterator<Item = &'static RepairPair> {
    super::REPAIRS
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
        .chain(super::kind_coverage::KIND_COVERAGE)
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
        .chain(super::reserved_names::RESERVED_NAMES)
}

/// What a pinned rejection must show [DIAG-1]: a source rejection under the
/// pinned rule whose rendering carries every pinned sentence, pins whole
/// every repair line it prints, and has sound words.
pub(super) fn check_rejection(
    name: &str,
    rule: &str,
    sentences: &[&str],
    failure: &crate::CompilationFailure,
) -> Result<(), String> {
    if failure.kind() != CompilationFailureKind::Source {
        return Err(format!("{name}: not a source rejection: {failure}"));
    }
    if failure.rule_id() != Some(rule) {
        return Err(format!("{name}: expected {rule}: {failure}"));
    }
    let rendered = format!("{failure}\n");
    if sentences.is_empty() && rendered.contains("\n  mechanical_fix:") {
        let repairs: Vec<&str> = rendered
            .lines()
            .filter(|line| line.starts_with("  mechanical_fix:"))
            .collect();
        return Err(format!(
            "{name}: this position must not offer an unverified repair; its rendering opens with `{}` and prints:\n{}",
            rendered.lines().next().unwrap_or_default(),
            repairs.join("\n")
        ));
    }
    for sentence in sentences {
        if !rendered.contains(sentence) {
            return Err(format!(
                "{name}: the rejection no longer carries this repair.\nwanted: {sentence}\ngot:    {rendered}"
            ));
        }
    }
    // Every repair line a rendering prints is one the pair carries out: a
    // pair that pins other fields only would leave the repair unchecked.
    for line in rendered
        .lines()
        .filter(|line| line.starts_with("  mechanical_fix:"))
    {
        let whole = format!("\n{line}\n");
        if !sentences.iter().any(|sentence| sentence.contains(&whole)) {
            return Err(format!(
                "{name}: this repair line is not pinned whole: {line}"
            ));
        }
    }
    let defects: Vec<String> = failure
        .diagnostics()
        .flat_map(crate::CompilationFailure::text_defects)
        .collect();
    if !defects.is_empty() {
        return Err(format!("{name}: its words are unsound: {defects:?}"));
    }
    Ok(())
}

/// Checks one pair [DIAG-1] and returns the kind its rejection prints, or why
/// the pair fails: the source is rejected under the pinned rule, the
/// rendering carries every pinned sentence and every repair line it prints is
/// pinned whole, its words are sound, and each alternative is accepted with
/// its construct running in a state that is not contradictory.
fn check_pair(pair: &RepairPair) -> Result<String, String> {
    let name = pair.name;
    let failure = match compile(
        &[SourceInput::new(pair.name, pair.rejected)],
        CompilerLimits::default(),
    ) {
        Ok(_) => return Err(format!("{name}: the pinned source is accepted")),
        Err(failure) => failure,
    };
    super::entry_formers::assert_span(pair, &failure);
    // [TYPE-9] semantic tests pin the complete call or written targ
    // span. Pin its rendered start here with the existing repair run.
    let citation = match pair.name {
        "type9-neg-runtime-slots-content-swap.wf" => {
            Some(("  ", "swap(first: &small.inner, second: &large.inner)"))
        }
        "type9-neg-generic-runtime-content-swap.wf" => Some(("exchange::<", "Slots<u64>")),
        "type9-neg-function-actual-runtime-content-swap.wf" => Some(("fn swap::<", "Slots<u64>")),
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
        if location.path() != pair.name || (location.line(), location.column()) != (line, column) {
            return Err(format!("{name}: cited at the wrong place: {failure}"));
        }
    }
    check_rejection(name, pair.rule, pair.sentences, &failure)?;
    if pair.repaired.is_empty() {
        return Err(format!("{name}: the pair has no accepted program"));
    }
    for (alternative, source) in pair.repaired.iter().enumerate() {
        match contradictory_successes(pair.name, source) {
            Ok(contradictions) if contradictions.is_empty() => {}
            Ok(contradictions) => {
                return Err(format!(
                    "{name}: alternative {alternative} succeeds only where its state is contradictory: {contradictions:?}"
                ));
            }
            Err(rejection) => {
                return Err(format!(
                    "{name}: alternative {alternative} is rejected:\n{rejection}"
                ));
            }
        }
    }
    Ok(failure.kind_name().unwrap_or_default().to_owned())
}

/// What every pinned pair showed: the kinds their rejections print and the
/// pairs that fail, with why.
pub(super) struct Run {
    pub(super) reached: BTreeSet<String>,
    pub(super) failures: Vec<String>,
}

/// Runs one pair's check, reporting a panic in it as that pair's failure so
/// that one broken pair does not hide the others.
fn guarded(name: &str, check: impl FnOnce() -> Result<String, String>) -> Result<String, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(check)).unwrap_or_else(|payload| {
        let why = payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|why| (*why).to_owned()))
            .unwrap_or_default();
        Err(format!("{name}: the check panicked: {why}"))
    })
}

/// Checks every pinned pair once for the whole test binary: each pair
/// compiles several programs, and two tests read its result.
pub(super) fn run() -> &'static Run {
    static RUN: OnceLock<Run> = OnceLock::new();
    RUN.get_or_init(|| {
        let mut run = Run {
            reached: BTreeSet::new(),
            failures: Vec::new(),
        };
        let mut results: Vec<Result<String, String>> = Vec::new();
        results.extend(all_pairs().map(|pair| guarded(pair.name, || check_pair(pair))));
        results.extend(
            super::module_pairs::MODULE_PAIRS
                .iter()
                .chain(super::module_kind_pairs::MODULE_KIND_PAIRS)
                .map(|pair| guarded(pair.name, || super::module_pairs::check_module_pair(pair))),
        );
        results.extend(super::module_pairs::LIBRARY_GRAPH_PAIRS.iter().map(|pair| {
            guarded(pair.name, || {
                super::module_pairs::check_library_graph_pair(pair)
            })
        }));
        for result in results {
            match result {
                Ok(kind) => {
                    run.reached.insert(kind);
                }
                Err(why) => run.failures.push(why),
            }
        }
        run
    })
}

/// Each pinned repair is carried out by its programs [DIAG-1]: each
/// alternative is accepted, and its construct runs in a state that is not
/// contradictory. Every failing pair is reported together.
pub(super) fn assert_pairs_carry_out_their_repairs() {
    let run = run();
    assert!(
        run.failures.is_empty(),
        "{} pinned repair pairs fail:\n{}",
        run.failures.len(),
        run.failures.join("\n\n")
    );
}

/// Every rejection kind the compiler can print has a pinned pair that
/// reaches it, and no pair reaches a kind the compiler does not list.
pub(super) fn assert_every_kind_has_a_pair() {
    let run = run();
    let listed = crate::rejection_kinds();
    let unlisted = unlisted_kinds(&listed, &run.reached);
    assert!(
        unlisted.is_empty(),
        "pairs reach kinds the compiler does not list: {unlisted:?}"
    );
    let missing = missing_kinds(&listed, &run.reached);
    assert!(
        missing.is_empty(),
        "{} rejection kinds have no repair pair: {missing:?}",
        missing.len()
    );
}

/// The kinds `listed` names that `reached` does not.
pub(super) fn missing_kinds(listed: &[&str], reached: &BTreeSet<String>) -> Vec<String> {
    listed
        .iter()
        .filter(|kind| !reached.contains(**kind))
        .map(|kind| (*kind).to_owned())
        .collect()
}

/// The kinds `reached` names that `listed` does not.
pub(super) fn unlisted_kinds(listed: &[&str], reached: &BTreeSet<String>) -> Vec<String> {
    reached
        .iter()
        .filter(|kind| !listed.contains(&kind.as_str()))
        .cloned()
        .collect()
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

/// A kind the compiler lists and no pair reaches is missing, and a kind a
/// pair reaches and the compiler does not list is unlisted.
#[test]
fn the_kind_coverage_names_what_is_missing_and_what_is_unlisted() {
    let reached: BTreeSet<String> = ["Listed".to_owned(), "Stray".to_owned()].into();
    assert_eq!(
        missing_kinds(&["Listed", "InventedKind"], &reached),
        ["InventedKind"]
    );
    assert_eq!(
        unlisted_kinds(&["Listed", "InventedKind"], &reached),
        ["Stray"]
    );
    assert!(missing_kinds(&["Listed"], &reached).is_empty());
}

/// The first pair of the table with its sentences and programs replaced.
fn type9_pair(
    sentences: &'static [&'static str],
    repaired: &'static [&'static [u8]],
) -> RepairPair {
    RepairPair {
        name: "type9-neg-runtime-slots-content-swap.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/type9-neg-runtime-slots-content-swap.wf"
        ),
        rule: "TYPE-9",
        sentences,
        repaired,
    }
}

const TYPE9_BOX_SWAP: &[u8] =
    include_bytes!("../../../../tests/conformance/cases/type9-pos-runtime-box-swap.wf");

/// A pair that pins another field of its rejection and not the repair line
/// the rejection prints leaves that repair unchecked, so it fails.
#[test]
fn a_pair_that_leaves_a_printed_repair_line_unpinned_fails() {
    let pair = type9_pair(&["]: InvalidRestrictedTypePlacement\n"], &[TYPE9_BOX_SWAP]);
    let why = check_pair(&pair).expect_err("the repair line is not pinned");
    assert!(why.contains("not pinned whole"), "{why}");
}

/// A pair with no accepted program shows no repair carried out, so it fails.
#[test]
fn a_pair_with_no_accepted_program_fails() {
    let pair = type9_pair(
        &[
            "]: InvalidRestrictedTypePlacement\n",
            "\n  mechanical_fix: swap the owning `Box` values instead of their contents\n",
        ],
        &[],
    );
    let why = check_pair(&pair).expect_err("the pair has no program");
    assert!(why.contains("no accepted program"), "{why}");
}

/// The same pair with both sentences and its program passes and names the
/// kind its rejection prints.
#[test]
fn a_complete_pair_passes_and_names_its_kind() {
    let pair = type9_pair(
        &[
            "]: InvalidRestrictedTypePlacement\n",
            "\n  mechanical_fix: swap the owning `Box` values instead of their contents\n",
        ],
        &[TYPE9_BOX_SWAP],
    );
    assert_eq!(
        check_pair(&pair),
        Ok("InvalidRestrictedTypePlacement".to_owned())
    );
}
