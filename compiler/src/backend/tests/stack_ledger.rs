//! The stack ledger against the machine it describes.
//!
//! The ledger's whole value is that its numbers are true of the binary. A
//! frame size is a whole-program, optimizer-chosen property — adding one
//! shallow call somewhere else in a program has been measured to double the
//! frame of a recursion it never touches — so nothing about the source says
//! what a level costs, and a ledger nobody checks is a ledger nobody should
//! believe.
//!
//! So the case below does not check that the ledger parsed its input. It takes
//! the depth the ledger says the runtime's stack holds and executes the same
//! measured assembly just inside and outside it with a runtime depth. If the report and the machine
//! ever stop agreeing — a target where the reported frame excludes the return
//! address, a frame the host reports as `dynamic`, a reserve at the top of the
//! stack that this one does not have — this fails, loudly, with both numbers.

use std::process::Command;

use super::super::{Architecture, FLOOR_STACK_BYTES, stack_ledger};
use super::exhaustion::spine_source;
use super::{compile, test_directory};

/// A recursion with a complete nonuniform fixed run live across the call.
/// It supplies a different machine-frame geometry from the narrow spine.
///
/// The pair of widths is the point: the model under test is one division, and
/// checking it at two genuinely different frame sizes says more about
/// the division than checking it twice at one size would. The run
/// is 256 slots because the host compiler spends nine seconds vectorizing
/// the fill of a seven-thousand-element one and a tenth of a second on this,
/// for the same arithmetic under test.
///
/// The boundary observer supplies depth to the exported function at runtime.
/// The recursive result selects the
/// later read from a nonuniform array, keeping the complete local array live
/// across the call. Reading only the slot just overwritten with depth lets
/// LLVM eliminate the array and solve the recursion, leaving no wide frame.
use crate::native_test_support::wide_frame_source;

/// The ledger for one already-emitted module, so a caller can ask for the
/// overlapped world as well as the sequential one.
pub(super) fn machine_report(emitted: &str, directory: &std::path::Path) -> (String, String) {
    let module = directory.join("ledger.ll");
    let assembly = directory.join("ledger.s");
    std::fs::write(&module, emitted).expect("write the ledger module");
    let status = Command::new("/usr/bin/clang")
        .arg("-x")
        .arg("ir")
        .arg(&module)
        .arg("-S")
        .arg("-o")
        .arg(&assembly)
        .arg("-fstack-usage")
        .arg("-Wno-override-module")
        .args(crate::HOST_OPTIMIZATION_ARGUMENTS)
        .status()
        .expect("run the host compiler for the ledger");
    assert!(status.success(), "the ledger compilation failed: {status}");
    let usage =
        std::fs::read_to_string(directory.join("ledger.su")).expect("read the stack-usage report");
    let text = std::fs::read_to_string(&assembly).expect("read the ledger assembly");
    (usage, text)
}

pub(super) fn ledger_lines(emitted: &str, directory: &std::path::Path) -> Vec<String> {
    let (usage, assembly) = machine_report(emitted, directory);
    stack_ledger(&usage, &assembly, FLOOR_STACK_BYTES, Architecture::HOST)
}

/// What the ledger says one level of a named recursion costs, in bytes.
pub(super) fn reported_frame_bytes(lines: &[String], name: &str) -> u64 {
    let row = cycle_row(lines, name);
    let bytes = row
        .split_whitespace()
        .zip(row.split_whitespace().skip(1))
        .find_map(|(value, unit)| (unit == "B/level").then_some(value))
        .unwrap_or_else(|| panic!("the cycle row states no frame width: {row}"));
    bytes
        .parse()
        .unwrap_or_else(|_| panic!("the frame width is not a number: {row}"))
}

fn cycle_row<'a>(lines: &'a [String], name: &str) -> &'a String {
    lines
        .iter()
        .find(|line| {
            line.starts_with("STACK cycle")
                && line
                    .split_whitespace()
                    .nth(2)
                    .is_some_and(|symbol| symbol == name)
        })
        .unwrap_or_else(|| panic!("the ledger reports no cycle for {name}: {lines:#?}"))
}

/// The levels the ledger says one named recursion's stack holds.
fn reported_levels(lines: &[String], name: &str) -> u64 {
    let row = cycle_row(lines, name);
    let levels = row
        .split_whitespace()
        .zip(row.split_whitespace().skip(1))
        .find_map(|(value, unit)| (unit == "levels").then_some(value))
        .unwrap_or_else(|| panic!("the cycle row states no level count: {row}"));
    levels
        .parse()
        .unwrap_or_else(|_| panic!("the level count is not a number: {row}"))
}

/// A row is what one activation costs, not what the function allocated.
///
/// The measured boundary checks two machine geometries on the current host. This states the same rule from a synthetic report, in a
/// millisecond, for both architectures at once — the eight bytes x86-64 leaves
/// out of every figure it reports are exactly what the ledger got wrong before
/// batch 0090, and an arm64 machine can now be the one that says so.
///
/// The cycle row is checked with the frame row because the cost is what the
/// division consumes: an activation count is the runtime's stack over what one
/// level really spends, and a ledger that carried the eight bytes into the
/// table but not into the division would over-promise depth exactly as before.
#[test]
fn a_row_is_what_one_activation_costs() {
    // `-fstack-usage` writes `locator\tbytes\tqualifier`, and clang's locator
    // is `file:line:column:name`.
    const REPORTED: u64 = 64;
    const STACK: u64 = 4_096;
    let usage = format!("probe.wf:1:1:wf_probe\t{REPORTED}\tstatic\n");
    // Column zero is a label and an indented line is an instruction, which is
    // all the graph reader needs to see a function that reaches itself.
    let assembly = "wf_probe:\n\tcall wf_probe\n\tret\n";

    for (architecture, level) in [
        (Architecture::X86_64, REPORTED + 8),
        (Architecture::Arm64, REPORTED),
    ] {
        let lines = stack_ledger(&usage, assembly, STACK, architecture);
        assert!(
            lines
                .iter()
                .any(|line| line.starts_with("STACK frame")
                    && line.contains(&format!("{level} B"))),
            "on {architecture:?} one activation costs {level} bytes: {lines:#?}"
        );
        assert_eq!(reported_frame_bytes(&lines, "wf_probe"), level);
        assert_eq!(reported_levels(&lines, "wf_probe"), STACK / level);
    }
}

/// The architecture the ledger calls the host's is the machine's own.
///
/// Read from [`std::env::consts::ARCH`] rather than from a `cfg!` of its own,
/// so this is an independent statement and not a copy of the ledger's
/// predicate: a `HOST` that named the wrong architecture would satisfy its own
/// `cfg!` and fail here.
#[test]
fn the_selected_architecture_matches_the_rust_target() {
    let expected = match std::env::consts::ARCH {
        "x86_64" => Architecture::X86_64,
        "aarch64" => Architecture::Arm64,
        other => panic!("no qualified target runs on {other}, so the ledger has no rule for it"),
    };
    assert_eq!(Architecture::HOST, expected);
}

/// Same measured machine code, runtime depth, actual entry/worker bounds.
/// The allowance is set before measuring outcomes: two pages, 4 KiB of fixed
/// caller overhead and two WF frames. It cannot hide a factor-of-two frame bug.
#[test]
fn the_reported_ceiling_is_the_measured_one() {
    for (wide, source) in [(false, spine_source(0)), (true, wide_frame_source(0, 256))] {
        let directory = test_directory();
        let emitted = compile(&source)
            .replacen(
                "define i32 @wf__main_body(",
                "define i32 @wf__unused_main_body(",
                1,
            )
            .replacen("define i32 @main(", "define i32 @wf__unused_main(", 1)
            + "\ndeclare i32 @wf__main_body(i32, ptr)\n";
        let (usage, assembly) = machine_report(&emitted, &directory);
        let frames = stack_ledger(&usage, &assembly, TEST_STACK_BYTES, Architecture::HOST);
        let frame = reported_frame_bytes(&frames, "wf_spine");
        assert!(frame > 0);
        if wide {
            assert!(
                frame >= 256 * 8,
                "the live array must remain in the machine frame: {frame}"
            );
        }
        let executable = link_measured_boundary(&directory, wide);
        for thread in ["entry", "worker"] {
            let run = |mode: &str, depth: u64| {
                Command::new(&executable)
                    .env("WF_WORKERS", "4")
                    .arg(mode)
                    .arg(depth.to_string())
                    .output()
                    .expect("run measured stack boundary")
            };
            let bounds = run(&format!("bounds-{thread}"), 0);
            assert!(bounds.status.success(), "wide={wide} {thread}: {bounds:?}");
            assert!(bounds.stderr.is_empty());
            let text = String::from_utf8(bounds.stdout).expect("ASCII bounds");
            assert!(text.starts_with(&format!("{thread}\n")), "{text}");
            let field = |name: &str| {
                text.split_whitespace()
                    .find_map(|word| word.strip_prefix(name))
                    .expect("bound field")
                    .parse::<u64>()
                    .expect("numeric bound")
            };
            let room = field("room=");
            let page = field("page=");
            assert!(field("stack=") >= TEST_STACK_BYTES);
            let allowance = 2 * page + 4096 + 2 * frame;
            assert!(
                allowance < room / 10,
                "fixture overhead must stay below ten percent"
            );
            let levels = reported_levels(
                &stack_ledger(&usage, &assembly, room, Architecture::HOST),
                "wf_spine",
            );
            let slack = allowance.div_ceil(frame);
            assert!(levels > slack);
            for (depth, exhausts) in [(levels - slack, false), (levels + slack, true)] {
                let output = run(thread, depth);
                assert_eq!(
                    output.stdout,
                    format!("{thread}\n").as_bytes(),
                    "{output:?}"
                );
                if exhausts {
                    use std::os::unix::process::ExitStatusExt;
                    assert_eq!(
                        output.status.signal(),
                        Some(6),
                        "wide={wide} {thread} depth={depth}: {output:?}"
                    );
                    super::exhaustion::assert_resource_record(&output.stderr, "stack");
                } else {
                    assert_eq!(
                        output.status.code(),
                        Some(0),
                        "wide={wide} {thread} depth={depth}: {output:?}"
                    );
                    assert!(output.stderr.is_empty(), "{output:?}");
                }
            }
        }
        std::fs::remove_dir_all(directory).expect("remove measured stack fixture");
    }
}

const TEST_STACK_BYTES: u64 = 1024 * 1024;

fn link_measured_boundary(directory: &std::path::Path, wide: bool) -> std::path::PathBuf {
    use std::sync::OnceLock;
    static FLOOR: OnceLock<Vec<u8>> = OnceLock::new();
    let floor = FLOOR.get_or_init(|| {
        let scratch = test_directory();
        let original = crate::FLOOR_RUNTIME_SOURCE;
        let definition = original
            .lines()
            .find(|line| line.starts_with("#define WF_FLOOR_STACK_BYTES "))
            .expect("the shipped floor declares its stack reservation");
        assert_eq!(original.matches(definition).count(), 1);
        // Only the reservation constant changes; setup, probing, bounds and
        // the real signal handler are the shipped implementation.
        let source = original.replacen(
            definition,
            &format!("#define WF_FLOOR_STACK_BYTES ((size_t){TEST_STACK_BYTES}u)"),
            1,
        );
        let path = scratch.join("floor.c");
        let object = scratch.join("floor.o");
        std::fs::write(&path, source).expect("write small-stack floor variant");
        let output = Command::new("/usr/bin/clang")
            .args(["-std=c11", "-pthread", "-c"])
            .arg(&path)
            .args(crate::HOST_OPTIMIZATION_ARGUMENTS)
            .arg("-o")
            .arg(&object)
            .output()
            .expect("compile small-stack floor");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let bytes = std::fs::read(object).expect("read small-stack floor object");
        std::fs::remove_dir_all(scratch).expect("remove small-stack floor construction");
        bytes
    });
    let host = directory.join("boundary.c");
    std::fs::write(&host, include_str!("stack_boundary.c")).expect("write boundary observer");
    let executable = directory.join("boundary");
    let mut command = Command::new("/usr/bin/clang");
    command
        .arg("-x")
        .arg("assembler")
        .arg(directory.join("ledger.s"))
        .arg("-x")
        .arg("c")
        .arg(&host)
        .arg("-pthread")
        .arg(format!("-DWF_TEST_WIDE={}", usize::from(wide)));
    let (_, objects) = crate::native_test_support::append_runtime_objects(
        &mut command,
        directory,
        Some("c11"),
        None,
    );
    // The shared support owns object order; index zero is its staged floor.
    std::fs::write(&objects[0], floor).expect("select the small-stack floor variant");
    let output = command
        .args(crate::HOST_OPTIMIZATION_ARGUMENTS)
        .args(crate::HOST_LINK_LIBRARIES)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("link the measured assembly");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    executable
}

/// The recursion the compiler generates is in the ledger like any other.
///
/// A compiler-derived release descends the stack exactly where the type's own
/// release graph closes on itself [PROV-6], and that recursion has no name in
/// the source, so no writer could look for it. The ledger is what makes it
/// visible: a `wf.drop` cycle row is the derived release of a recursive
/// nominal, reported beside the recursions the writer wrote.
///
/// The assertion below was the opposite until 2026-09-04, when the owner
/// deleted [PROV-6]'s release-graph cycle refusal and ruled that the walk may
/// recurse. The explicit worklist that kept the depth off the stack is gone
/// with the refusal it was written for — it allocated, and an allocation on
/// the release path is a runtime trap the writer never wrote — so the cycle
/// row is now the correct report rather than the defect.
#[test]
fn the_compilers_own_drop_glue_has_rows_and_reports_its_cycle() {
    let directory = test_directory();
    let module = compile(RECURSIVE_VALUE);
    let glue = super::exhaustion::drop_glue_calls(&module);
    let cyclic = glue
        .iter()
        .filter(|(root, _)| {
            let mut pending = vec![root.as_str()];
            let mut seen = std::collections::HashSet::new();
            while let Some(name) = pending.pop() {
                if !seen.insert(name) {
                    continue;
                }
                if let Some((_, calls)) = glue.iter().find(|(candidate, _)| candidate == name) {
                    for target in calls {
                        if target == root {
                            return true;
                        }
                        pending.push(target.as_str());
                    }
                }
            }
            false
        })
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>();
    assert!(
        !cyclic.is_empty(),
        "the fixture must derive a cyclic release graph"
    );
    let lines = ledger_lines(&module, &directory);
    assert!(
        cyclic.iter().any(|name| {
            ["STACK frame", "STACK cycle"].iter().all(|prefix| {
                lines.iter().any(|line| {
                    line.starts_with(prefix) && line.split_whitespace().nth(2) == Some(*name)
                })
            })
        }),
        "one of the actual derived cycle members must have both machine frame and cycle rows: {cyclic:?} {lines:#?}"
    );
    std::fs::remove_dir_all(&directory).expect("remove the test directory");
}

/// A recursive nominal, built and destroyed, with nothing recursive written.
///
/// [STOR-8] makes allocation total in the source, so the `Option` arms this
/// fixture used to carry around every cell have no subject left: `box_new`
/// returns the cell itself and no allocating operation carries a `Result`.
const RECURSIVE_VALUE: &[u8] = br#"enum Tree {
  Leaf();
  Branch(left: Box<Tree>, right: Box<Tree>);
}

fn boxed_leaf() -> made: Box<Tree> pure {
  let leaf = Tree::Leaf();
  let cell = box_new::<Tree>(value: move leaf);
  return move cell;
}

fn boxed_branch(left: Box<Tree>, right: Box<Tree>) -> made: Box<Tree> pure {
  let branch = Tree::Branch(left: move left, right: move right);
  let cell = box_new::<Tree>(value: move branch);
  return move cell;
}

fn main() -> status: std::process::ExitStatus pure {
  let boxed_left = boxed_leaf();
  let boxed_right = boxed_leaf();
  let root = boxed_branch(left: move boxed_left, right: move boxed_right);
  return std::process::exit_status(code: 0_u8);
}
"#;
