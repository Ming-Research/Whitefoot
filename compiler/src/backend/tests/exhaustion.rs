//! The resource-exhaustion floor: what an execution does when it runs out.
//!
//! Running out of stack or heap needs no source defect at all and, before this
//! floor, produced zero bytes and a bare host signal. These cases pin the
//! deliberately deferred resource-availability behavior without turning it
//! into a source proof obligation.
//!
//! Two separate obligations live here and must not be confused:
//!
//! - *containment* — a frame larger than the guard region walks its pages on
//!   the way down, so it cannot step over the guard into whatever is mapped
//!   below. This is a safety property, not a reporting one: without it an
//!   accepted program can silently overwrite a neighbouring thread's live
//!   stack. [`every_generated_definition_carries_the_stack_probe`] is its
//!   case.
//! - *reporting* — exhaustion ends the process by a defined abort that first
//!   writes one fixed record naming only the resource class. The record
//!   carries no `rule_id`, no function, and no node path.
//!
//! Compiler-generated probes, allocation edges and cleanup stay here. Pure C
//! floor setup, signal classification, stack provisioning and record-latch
//! observations live in `floor_probe.c`, under the common native test target.
//! Record bytes are a runtime implementation contract; SCOPE-3 keeps resource
//! availability outside the source outcome model.
//!
//! Allocation is total in the source language [STOR-8]; heap exhaustion ends
//! the process inside the trusted base. The tests below cover the resource
//! record and abort edges for scalar, array, Slots and Ring cells, cleanup of
//! initialized elements before their backing allocation, probe attachment to
//! every emitted definition, and native containment on a guarded stack.

use std::process::Command;

use super::{build_linked_executable, compile, test_directory};

/// The attribute group [`crate::backend::emitter`] gives every definition, and
/// the value it carries on this host.
///
/// The value is the host C compiler's own frame-probing helper, so a generated
/// frame walks its pages exactly the way the runtime's translation unit does.
#[cfg(target_os = "macos")]
const HOST_STACK_PROBE: &str = "\"probe-stack\"=\"__chkstk_darwin\"";
#[cfg(not(target_os = "macos"))]
const HOST_STACK_PROBE: &str = "\"probe-stack\"=\"inline-asm\"";

/// A program that reaches several kinds of generated definition at once: a
/// heap box and its compiler-generated drop glue, a recursive walk, a system
/// transfer, and the entry itself.
const MIXED_DEFINITIONS: &[u8] = br#"enum Chain {
  End();
  More(tail: Box<Chain>);
}

fn depth(chain: &Box<Chain>) -> result: u64 reads(chain) {
  match deref(chain).inner {
    End() => {
      return 0_u64;
    }
    More(tail: below_chain) => {
      let below = depth(chain: below_chain);
      return below +wrap 1_u64;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let end = Chain::End();
  let bottom = box_new::<Chain>(value: move end);
  let one = Chain::More(tail: move bottom);
  let boxed = box_new::<Chain>(value: move one);
  let measured = depth(chain: &boxed);
  if measured == 1_u64 {
  } else {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;

fn mixed_module(parallel: bool) -> String {
    use std::sync::OnceLock;
    static PLAIN: OnceLock<String> = OnceLock::new();
    static PARALLEL: OnceLock<String> = OnceLock::new();
    let cell = if parallel { &PARALLEL } else { &PLAIN };
    cell.get_or_init(|| {
        if parallel {
            super::emit_with_overlap(MIXED_DEFINITIONS)
        } else {
            compile(MIXED_DEFINITIONS)
        }
    })
    .clone()
}

fn heap_module() -> String {
    static MODULE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    MODULE.get_or_init(|| compile(ALL_HEAP_FORMS)).clone()
}

fn boxed_module() -> String {
    static MODULE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    MODULE
        .get_or_init(|| compile(&boxed_spine_source(4)))
        .clone()
}

fn buffer_module() -> String {
    static MODULE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    MODULE
        .get_or_init(|| compile(&buffer_chain_source(4)))
        .clone()
}

/// Every definition the module emits carries the probe attribute, and the
/// group it names is the host's.
///
/// The count equality is the point of the case. Containment is a completeness
/// property — one unprobed large frame is enough to jump the guard region into
/// a neighbouring thread's live stack — so a case that merely found *some*
/// probed definition would pass against a module that left the drop glue, a
/// clone, or a thunk unprobed. Counting both sides is what makes "every
/// generated function" checkable rather than asserted.
#[test]
fn every_generated_definition_carries_the_stack_probe() {
    assert_stack_probes(&mixed_module(false));
    assert_stack_probes(&mixed_module(true));
}

/// Called also by retained loop and pair fixtures, which actually emit chunks,
/// thunks and sequential clones in addition to the mixed fixture's drop glue.
pub(super) fn assert_stack_probes(module: &str) {
    let definitions = module
        .lines()
        .filter(|line| line.starts_with("define "))
        .count();
    assert!(
        definitions > 1,
        "the fixture must reach more than one definition:\n{module}"
    );
    let probed = module
        .lines()
        .filter(|line| line.starts_with("define ") && line.ends_with(" #0 {"))
        .count();
    assert_eq!(
        probed,
        definitions,
        "every generated definition must carry the probe group; \
             {} of {definitions} did not:\n{module}",
        definitions - probed
    );
    assert_eq!(
        module.matches("attributes #0 = { ").count(),
        1,
        "the module declares its one attribute group once:\n{module}"
    );
    assert!(
        module.contains(&format!("attributes #0 = {{ {HOST_STACK_PROBE} }}")),
        "the group must name this host's probing helper:\n{module}"
    );
}

/// Compare the small function's actual assembly with and without its probe
/// attribute. This observes target code, including inline Linux probes; a
/// whole-executable symbol-name search could not establish ordinary cost.
#[test]
fn an_ordinary_frame_has_the_same_machine_body_without_probe_instrumentation() {
    let directory = test_directory();
    let module = mixed_module(false);
    let (_, probed) = super::stack_ledger::machine_report(&module, &directory);
    let (_, ablated) =
        super::stack_ledger::machine_report(&ablate_probe(&module, "@wf_depth("), &directory);
    let body = |assembly: &str| {
        let label = if cfg!(target_os = "macos") {
            "_wf_depth:"
        } else {
            "wf_depth:"
        };
        let start = assembly
            .find(label)
            .expect("the small function survives native codegen");
        let tail = &assembly[start..];
        let end = tail
            .find(".cfi_endproc")
            .expect("the machine function ends its unwind region");
        tail[..end].to_owned()
    };
    assert_eq!(
        body(&probed),
        body(&ablated),
        "a small frame must pay no added probe instructions"
    );
    std::fs::remove_dir_all(directory).expect("remove assembly comparison");
}

/// A recursion whose depth is a parameter, in a shape the host optimizer
/// cannot turn back into a loop.
///
/// The recursive result is consumed by an addition after a second call
/// returns, so there is no tail call to eliminate and each level really takes
/// a frame. `spine`'s two callees are also an eligible overlap pair, which is
/// what lets the `--par` build carry the deep half onto a lane.
pub(super) use crate::native_test_support::spine_source;

/// The record is the resource class and nothing else.
///
/// Exhaustion is external to source proof: no operation in the program has
/// "runs out of stack" in its meaning, and the same source on the same input
/// succeeds or fails depending on the environment. The record therefore names
/// only the unavailable resource.
pub(super) fn assert_resource_record(stderr: &[u8], resource: &str) {
    let text = String::from_utf8_lossy(stderr);
    assert_eq!(
        text,
        format!("{{\"resource\":\"{resource}\"}}\n"),
        "an exhausted execution writes exactly its resource record"
    );
}

/// The signal that ended a process, or `None` if it exited normally.
fn signal_of(output: &std::process::Output) -> Option<i32> {
    std::os::unix::process::ExitStatusExt::signal(&output.status)
}

#[cfg(not(target_os = "macos"))]
const fn libc_sigsegv() -> i32 {
    11
}

const fn libc_sigabrt() -> i32 {
    6
}

/// A `--par` module that can write a heap-resource record.
///
/// Handing a call out makes concurrent allocation refusal possible, so this
/// module must use the shared first-record latch.
///
/// The count is a literal the host cannot satisfy, and with `u8`'s stride of
/// one it still discharges [OP-9]'s allocation-size obligation, so the module
/// really carries the construction whose heap exhaustion the trusted base
/// reports. Nothing in the source names that outcome: [STOR-8] hands back no
/// payload and the program holds no failure arm.
const HEAP_RECORD_LANE: &[u8] = br#"fn leafwork(v: u64) -> result: u64 pure {
  return v *wrap 3_u64;
}

fn build(n: u64) -> result: u64 pure {
  let b = box_array_filled::<u8>(count: 4000000000000000000_u64, value: 7_u8);
  let e = b.inner.len;
  return 0_u64 +wrap n;
}

fn both(n: u64) -> result: u64 pure {
  let a = build(n: n);
  let c = leafwork(v: n);
  return a +wrap c;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = both(n: 5_u64);
  let ok = r > 0_u64;
  if ok {
    return std::process::exit_status(code: 0_u8);
  }
  return std::process::exit_status(code: 1_u8);
}
"#;

#[test]
fn a_module_that_writes_a_resource_record_and_hands_a_call_out_is_latched() {
    let module = super::emit_with_overlap(HEAP_RECORD_LANE);
    assert!(
        module.contains("@wf__par_thunk_"),
        "the fixture must hand a call out, or the latch is not the question: \
         {module}"
    );
    assert!(module.contains("@.wf_resource_record.latch"));
    // KEPT AS WRITTEN for the lowering port: whether a v0.60 construction
    // still emits an inline null test that calls `@wf_resource_abort()`, or
    // whether the trusted base's own allocator terminates and the generated
    // module carries no refusal edge at all, is the lowering port's decision
    // under [STOR-8]. Re-derive this symbol and the latch symbol from it.
    assert!(
        module.contains("call void @wf_resource_abort()"),
        "the fixture must reach a resource record: {module}"
    );
    assert!(
        module.contains("%latch = call ptr @wf__floor_record_latch()"),
        "a module that writes a record on more than one thread must take the \
         shared latch: {module}"
    );
}

/// One program reaching every allocation form the emitter lowers: a filled
/// array, an empty window, a heap box, and a ring.
///
/// The counts are constants so [OP-9]'s allocation-size obligation discharges
/// statically and the fixture stays about the refusal edges rather than about
/// proving a dynamic count fits.
///
/// The arena node this image carried in v0.59 is gone with regions and arenas
/// [OWN-3, OWN-4, OWN-10, FORM-8, STOR-4]; the fourth form is now
/// `box_ring_new`, the fourth [OP-13] cell construction. Each form contributes
/// one observed measure [OP-15] so the successful run still proves it ran:
/// 7 + 4 + 4 + 3 = 18, the same exit code v0.59's image produced.
const ALL_HEAP_FORMS: &[u8] = br#"fn shapes(n: u64) -> result: u64 pure {
  let packed = box_array_filled::<u64>(count: 4_u64, value: 5_u64);
  let vacant = box_slots_new::<u32>(capacity: 4_u64);
  let cycle = box_ring_new::<u32>(capacity: 3_u64);
  let boxed = box_new::<u64>(value: 7_u64);
  let held = boxed.inner;
  let packed_len = packed.inner.len;
  let vacant_cap = vacant.inner.cap;
  let cycle_cap = cycle.inner.cap;
  let total = held +wrap packed_len;
  set total = total +wrap vacant_cap;
  set total = total +wrap cycle_cap;
  return total;
}

fn main() -> status: std::process::ExitStatus pure {
  let total = shapes(n: 4_u64);
  match cvt.checked::<u64, u8>(total) {
    Ok(value: byte) => {
      return std::process::exit_status(code: byte);
    }
    Err(error: wide) => {
      return std::process::exit_status(code: 9_u8);
    }
  }
}
"#;

/// Generated allocation calls alone are interposed; runtime startup allocation
/// remains real. One small four-form image covers success and every refusal.
///
/// The subject survives [STOR-8] unchanged: the program holds no failure arm
/// and receives no payload, and an exhausted heap ends the process from the
/// trusted base with the one record naming the resource class.
///
/// One [OP-13] cell construction is one interposed allocation, for every one
/// of the four forms. [TYPE-9] fixes it in the language: a `Box`'s one field
/// `inner` is its content, "stored in exactly one heap object the `Box` value
/// owns [STOR-1]", and [STOR-1] repeats that a `Box<T>` is "one
/// compiler-derived allocation released by one compiler-derived free at owner
/// scope exit [STOR-3]" while a runtime-capacity shape "exists only as `Box`
/// content [TYPE-9] and is heap-owned with that `Box`" -- one owner, one
/// object, not a cell beside a block. The implementation choice under that
/// rule is the pending amendment compiler/storage-representation: "A boxed
/// runtime-capacity shape is thin: `Box<Slots<T>>`, `Box<Ring<T>>` and
/// `Box<Array<T>>` are each one pointer to one block laid out `[len | cap |
/// elements]`". The four-form image is therefore four interposed allocations
/// and four frees, and the observer limit, the refusal range and both
/// identity lists below say exactly that. It is the same count
/// `a_runtime_capacity_window_crosses_functions_updates_and_frees_once` reads
/// as one `@free` per boxed `Array`.
#[test]
fn each_generated_allocation_form_reaches_its_refusal_record() {
    let directory = test_directory();
    let observed = heap_module()
        .replace("@malloc(", "@wf_test_allocate(")
        .replace("@free(", "@wf_test_release(");
    let host = format!(
        "{}\n__attribute__((constructor)) static void unbuffer(void) {{ setvbuf(stdout, NULL, _IONBF, 0); }}\n",
        super::owned_places::allocation_observer_by_process(4)
    );
    let executable = build_linked_executable(&observed, Some(&host), &[], &directory);
    for refused in 0..=4 {
        let output = Command::new(&executable)
            .env("WF_TEST_REFUSE_ALLOCATION", refused.to_string())
            .output()
            .expect("run scoped allocation refusal");
        let trace = std::str::from_utf8(&output.stdout).expect("ASCII allocation trace");
        if refused == 0 {
            assert_eq!(output.status.code(), Some(18), "{output:?}");
            assert!(output.stderr.is_empty());
            assert!(trace.starts_with("A1;A2;A3;A4;"), "{trace}");
            let mut freed = trace
                .split(';')
                .filter_map(|event| event.strip_prefix('F'))
                .collect::<Vec<_>>();
            freed.sort_unstable();
            assert_eq!(freed, ["1", "2", "3", "4"], "{trace}");
        } else {
            use std::os::unix::process::ExitStatusExt;
            assert_eq!(
                output.status.signal(),
                Some(libc_sigabrt()),
                "refused={refused}: {output:?}"
            );
            let expected = (1..refused).map(|id| format!("A{id};")).collect::<String>()
                + &format!("X{refused};");
            assert_eq!(trace, expected);
            assert_resource_record(&output.stderr, "heap");
        }
    }
    std::fs::remove_dir_all(directory).expect("remove allocation refusal image");
}

/// Script only the generated record write, leaving host startup and the floor
/// untouched. Both record writers must retry EINTR at the same byte offset,
/// keep positive partial progress, and stop on zero or a different error.
#[test]
fn heap_record_writers_retry_interruption_without_losing_partial_progress() {
    let source = std::str::from_utf8(HEAP_RECORD_LANE)
        .unwrap()
        .replace("4000000000000000000_u64", "4_u64");
    for overlap in [super::OverlapLowering::Off, super::OverlapLowering::On] {
        let directory = test_directory();
        let module = super::emit_lowered(source.as_bytes(), overlap);
        assert_eq!(
            module.contains("%latch = call ptr @wf__floor_record_latch()"),
            overlap == super::OverlapLowering::On,
            "exercise the sequential and latched record writers"
        );
        let observed = module
            .replace("@malloc(", "@wf_test_allocate(")
            .replace("@free(", "@wf_test_release(")
            .replace("@write(", "@wf_test_record_write(");
        let host = format!(
            "{}\n{}",
            super::owned_places::allocation_observer_by_process(1),
            RECORD_WRITE_OBSERVER
        );
        let executable = build_linked_executable(&observed, Some(&host), &[], &directory);
        for schedule in 0..4 {
            let output = Command::new(&executable)
                .env("WF_TEST_REFUSE_ALLOCATION", "1")
                .env("WF_TEST_WRITE_SCHEDULE", schedule.to_string())
                .current_dir(&directory)
                .output()
                .expect("run interrupted heap record writer");
            assert_eq!(signal_of(&output), Some(libc_sigabrt()), "{output:?}");
            if schedule < 2 {
                assert_resource_record(&output.stderr, "heap");
                assert_eq!(output.stdout, b"X1;W1;W2;W3;", "{output:?}");
            } else {
                assert_eq!(output.stderr, b"{\"res", "{output:?}");
                assert_eq!(output.stdout, b"X1;W1;W2;", "{output:?}");
            }
        }
        std::fs::remove_dir_all(directory).expect("remove interrupted record image");
    }
}

const RECORD_WRITE_OBSERVER: &str = r#"
#include <errno.h>
#include <unistd.h>

__attribute__((constructor)) static void unbuffer(void) {
    setvbuf(stdout, NULL, _IONBF, 0);
}

ssize_t wf_test_record_write(int fd, const void *bytes, size_t length) {
    static const char record[] = "{\"resource\":\"heap\"}\n";
    static unsigned calls;
    static size_t offset;
    const char *selected = getenv("WF_TEST_WRITE_SCHEDULE");
    if (selected == NULL || selected[0] < '0' || selected[0] > '3'
        || selected[1] != '\0') _Exit(90);
    unsigned schedule = (unsigned)(selected[0] - '0');
    unsigned step = calls++;
    if (step >= (schedule < 2 ? 3u : 2u)) _Exit(91);
    if (fd != 2 || length != sizeof(record) - 1 - offset
        || memcmp(bytes, record + offset, length) != 0) _Exit(92);
    printf("W%u;", calls);
    if ((schedule == 0 && step == 0) || (schedule == 1 && step == 1)) {
        errno = EINTR;
        return -1;
    }
    if (schedule >= 2 && step == 1) {
        errno = schedule == 2 ? EINTR : EIO;
        return schedule == 2 ? 0 : -1;
    }
    size_t count = step < 2 ? 5 : length;
    ssize_t written = write(fd, bytes, count);
    if (written != (ssize_t)count) _Exit(93);
    offset += count;
    /* A positive result remains progress even with stale EINTR. */
    errno = EINTR;
    return written;
}
"#;

/// Filled and empty windows whose proved byte ceilings fit the selected target
/// carry no runtime target-domain path. The allocator can still return null,
/// so each operation keeps its ordinary heap-resource edge, which is the
/// trusted base's termination and not a source outcome [STOR-8].
///
/// The subject is [STOR-6]'s separation: a target-qualification failure stops
/// compilation and may not become a runtime guard. That rule is unchanged.
///
/// A construction row is emitted as its own out-of-line body [PRE-1], so the
/// allocation and its refusal edge are in `wf_box_array_filled$instance$N` and
/// `wf_box_slots_new$instance$N` rather than at the call in `shapes`. The
/// emitted block names are `buffer.fill.` for a boxed `Array`'s block and
/// `window.block.` for a `Slots` or `Ring` block.
///
/// The former `*.target.` label probes were vacuous: the emitter has no
/// target-domain label, symbol or record at all, so no module could ever
/// contain one and the assertion could not fail. The falsifiable statement of
/// the same subject is the module's resource-record inventory: [STOR-6] makes
/// a failed qualification "a target-layout failure under [DIAG-1], not a
/// source-language rejection", and [DIAG-2] forbids target lowering to
/// "replace a missing proof with a runtime guard", so the only runtime
/// resource class an accepted module names is the heap [STOR-8]. A second
/// record constant, or a written record naming anything but the heap one,
/// fails an assertion below.
#[test]
fn target_qualified_buffers_keep_only_the_heap_refusal_path() {
    let module = heap_module();
    let records: Vec<&str> = module
        .lines()
        .filter(|line| line.starts_with("@.wf_resource."))
        .collect();
    assert_eq!(
        records.len(),
        1,
        "an accepted module names exactly one runtime resource class: {records:?}"
    );
    assert!(
        records[0].starts_with("@.wf_resource.heap = "),
        "and that class is the heap: {records:?}"
    );
    let written = module
        .lines()
        .filter(|line| {
            line.trim_start()
                .starts_with("call void @wf_resource_record_abort(")
        })
        .collect::<Vec<_>>();
    assert!(
        !written.is_empty(),
        "the fixture must reach a resource record:\n{module}"
    );
    for line in written {
        assert!(
            line.contains("ptr @.wf_resource.heap"),
            "every record written names the heap class: {line}"
        );
    }

    for (row, refusal_label) in [
        ("box_array_filled", "buffer.fill.oom."),
        ("box_slots_new", "window.block.oom."),
    ] {
        let body = super::emitted_prelude_row(&module, row);
        let lines: Vec<&str> = body.lines().collect();
        let refusal = lines
            .iter()
            .position(|line| line.starts_with(refusal_label))
            .expect("the allocator's null result must retain a refusal block");
        // Block ordering, restored: the allocator call is reached first and
        // the refusal block is its null-result successor, never a block the
        // row falls into before it has asked for storage.
        let allocation = lines
            .iter()
            .position(|line| line.contains("call ptr @malloc"))
            .expect("the row must reach the allocator");
        assert!(
            allocation < refusal,
            "the allocation precedes its refusal block:\n{body}"
        );
        let allocation_path = lines[allocation..refusal].join("\n");
        assert!(
            allocation_path.contains("icmp ne ptr"),
            "the refusal edge is the allocator's own null test:\n{body}"
        );
        assert_eq!(
            lines.get(refusal + 1).copied(),
            Some("  call void @wf_resource_abort()"),
            "{body}"
        );
    }
}

/// Every allocation-refusal edge reaches the resource abort, not a bare one.
///
/// The completeness matters the same way the probe attribute's does: a module
/// that routed three of its four refusal edges and left the fourth calling
/// `@abort` directly would still die silently on exactly the allocation that
/// took the fourth path, and nothing about the program would say which.
///
/// The `arena.new.oom.` edge is retired here with regions and arenas [OWN-3,
/// OWN-4, OWN-10, FORM-8, STOR-4]; the form no longer exists, so it has no
/// successor edge. The fourth form of the image above is `box_ring_new`.
///
/// The emitted label names the storage a construction takes rather than the
/// row that takes it, so the four [OP-13] cell constructions publish three
/// distinct refusal edges over four allocations: `box_new`'s scalar cell takes
/// `box.new.oom.`, the one block of `box_array_filled` takes
/// `buffer.fill.oom.`, and `box_slots_new` and `box_ring_new` share
/// `window.block.oom.`, because a window's block is header-first and one
/// address computation serves both [STOR-1, WIN-1]. Each construction takes
/// exactly one block: [TYPE-9] stores a `Box`'s content "in exactly one heap
/// object the `Box` value owns".
#[test]
fn every_allocation_refusal_edge_reaches_the_resource_abort() {
    let module = heap_module();
    let lines: Vec<&str> = module.lines().collect();
    for refusal in ["box.new.oom.", "buffer.fill.oom.", "window.block.oom."] {
        let mut found = 0;
        for (index, line) in lines.iter().enumerate() {
            if !line.starts_with(refusal) || !line.ends_with(':') {
                continue;
            }
            found += 1;
            assert_eq!(
                lines.get(index + 1).copied().unwrap_or_default(),
                "  call void @wf_resource_abort()",
                "the {line} edge must reach the resource abort, not a bare one"
            );
        }
        assert!(
            found > 0,
            "the fixture must reach a {refusal} edge:\n{module}"
        );
    }
}

/// A recursion whose every activation carries an array far larger than a guard
/// page, handed to a retained reader so the frame cannot be shrunk away. A
/// same-index local store/load can forward its scalar value and erase the
/// array entirely; it does not establish a large physical frame. The
/// controlled harness below enters only its base case; the
/// recursive edge keeps the generated function representative of an ordinary
/// source recursion without making the fault depend on a sequence of frames.
const LARGE_FRAME_SPINE: &[u8] =
    br#"fn read_pad(values: &Array<u64, 7168>, index: u64) -> result: u64 reads(values) contract {
  requires index < 7168_u64;
} {
  return deref(values)[index];
}

fn spine(depth: u64, v: u64, i: u8) -> result: u64 pure {
  let pad = array_filled::<u64, 7168>(value: v);
  let wide = cvt::<u8, u64>(i);
  set pad[wide] = depth;
  let done = depth == 0_u64;
  if done {
    return read_pad(values: &pad, index: wide);
  }
  let below = depth -wrap 1_u64;
  let a = spine(depth: below, v: v, i: i);
  let b = read_pad(values: &pad, index: wide);
  return a +wrap b;
}

fn main(inputs: std::process::Inputs) -> status: std::process::ExitStatus pure {
  let std::process::Inputs(args: args, cwd: unused_cwd, stdout: unused_stdout, stderr: unused_stderr, handles: entry_factory, stdin: unused_stdin) = move inputs;
  std::fs::close_directory(factory: &entry_factory, directory: move unused_cwd);
  let count = 0_u64;
  set count = std::text::args_count(args: &args);
  match cvt.checked::<u64, u8>(count) {
    Ok(value: idx) => {
      let depth = count *wrap 20000_u64;
      let r = spine(depth: depth, v: 3_u64, i: idx);
      let ok = r > 0_u64;
      if ok {
        return std::process::exit_status(code: 0_u8);
      }
      return std::process::exit_status(code: 1_u8);
    }
    Err(error: e) => {
      return std::process::exit_status(code: 9_u8);
    }
  }
}
"#;

/// Runs the generated large-frame function once on a stack whose surrounding
/// address space belongs to this fixture.
///
/// The stack is smaller than the array payload in one source activation. The
/// reservation beneath it is much larger than that payload and remains
/// `PROT_NONE`, so neither the floor's alternate stack nor another incidental
/// mapping can absorb the first access after an unprobed frame steps over the
/// stack. The thread attaches before making the call, exactly as a runtime lane
/// does, and its signal is classified against these known bounds.
const LARGE_FRAME_BODY: &str = r#"#define _GNU_SOURCE
#include <pthread.h>
#include <stdint.h>
#include <sys/mman.h>

extern int wf__floor_run(int argc, char **argv);
extern void wf__floor_attach_thread(void);
extern uint64_t wf_spine(uint64_t depth, uint64_t value, uint8_t index);

#define PAD_BYTES ((size_t)16 * 1024 * 1024)
#define STACK_BYTES ((size_t)32 * 1024)

static char *reservation;

static void *call_large_frame(void *opaque) {
    (void)opaque;
    wf__floor_attach_thread();
    (void)wf_spine(0, 3, 0);
    return NULL;
}

int wf__main_body(int argc, char **argv) {
    pthread_attr_t attributes;
    pthread_t thread;
    void *returned = NULL;
    (void)argc;
    (void)argv;
    reservation = mmap(NULL, PAD_BYTES + STACK_BYTES, PROT_NONE,
                       MAP_PRIVATE | MAP_ANON, -1, 0);
    if (reservation == MAP_FAILED) {
        return 2;
    }
    if (mprotect(reservation + PAD_BYTES, STACK_BYTES,
                 PROT_READ | PROT_WRITE) != 0) {
        return 3;
    }
    if (pthread_attr_init(&attributes) != 0
        || pthread_attr_setstack(&attributes, reservation + PAD_BYTES,
                                 STACK_BYTES) != 0
        || pthread_create(&thread, &attributes, call_large_frame, NULL) != 0) {
        return 4;
    }
    pthread_attr_destroy(&attributes);
    if (pthread_join(thread, &returned) != 0) {
        return 5;
    }
    return 6;
}

int main(int argc, char **argv) { return wf__floor_run(argc, argv); }
"#;

/// A frame far larger than the guard region is still contained and reported.
///
/// This is the behaviour the probe attribute buys, as distinct from the
/// attribute being present. Both binaries call the same generated function
/// once from the top of the fixture's small stack. Probed, the frame walks its
/// pages and first touches the protected memory within one stride of the low
/// bound, which the floor reports. Ablated, its first access beyond the stack
/// is farther into the fixture's protected reservation, which the floor leaves
/// to the host signal.
///
/// Owning both the stack and the memory beneath it is part of the assertion. A
/// recursive descent on a host-created stack leaves the final pre-fault stack
/// position and neighbouring mappings to target code generation and address
/// placement; either can make an unprobed run fault inside the reporting band
/// even though its large frame did not walk the guard.
#[test]
fn a_frame_larger_than_the_guard_region_is_still_reported() {
    let module = expose_large_frame_spine(&compile(LARGE_FRAME_SPINE));
    let directory = test_directory();
    let (_, probed_assembly) = super::stack_ledger::machine_report(&module, &directory);
    let executable = build_linked_executable(&module, Some(LARGE_FRAME_BODY), &[], &directory);
    let output = Command::new(&executable)
        .output()
        .expect("run the probed large frame");
    assert_eq!(
        signal_of(&output),
        Some(libc_sigabrt()),
        "a probed frame that exhausts its stack ends in the floor's abort: {:?}",
        output.status,
    );
    assert_resource_record(&output.stderr, "stack");

    let ablated = ablate_large_frame_probe(&module);
    assert_eq!(
        module.matches(" #0 {").count() - ablated.matches(" #0 {").count(),
        1,
        "the ablation must remove the group from exactly one definition"
    );
    let elsewhere = test_directory();
    let (_, assembly) = super::stack_ledger::machine_report(&ablated, &elsewhere);
    assert_native_spine_probe_was_ablated(&probed_assembly, &assembly);
    let unprobed = build_linked_executable(&ablated, Some(LARGE_FRAME_BODY), &[], &elsewhere);
    let output = Command::new(&unprobed)
        .output()
        .expect("run the unprobed large frame");
    assert!(
        output.stderr.is_empty(),
        "an unprobed frame steps over the guard region, so the fault it \
         eventually takes is not this thread running out and must not be \
         reported as one: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        signal_of(&output),
        Some(protected_page_signal()),
        "an unprobed frame keeps the host's protection-fault signal rather than \
         becoming the floor's abort: {:?}",
        output.status,
    );
    std::fs::remove_dir_all(&directory).expect("remove the test directory");
    std::fs::remove_dir_all(&elsewhere).expect("remove the second test directory");
}

/// Makes the generated function callable by the C fixture and leaves the
/// fixture to supply the program entry. The generated entry remains in the
/// module under an unused name so this changes no part of `wf_spine` itself.
fn expose_large_frame_spine(module: &str) -> String {
    let mut exposed = module
        .replacen("define internal i64 @wf_spine(", "define i64 @wf_spine(", 1)
        .replacen(
            "define i32 @wf__main_body(",
            "define i32 @wf__unused_main_body(",
            1,
        )
        .replacen("define i32 @main(", "define i32 @wf__unused_main(", 1);
    // Keep the ordinary source borrow crossing an opaque call boundary. The
    // reader's body and result stay unchanged; external visibility prevents
    // whole-program argument specialization and noinline preserves its call.
    exposed = exposed
        .lines()
        .map(|line| {
            if line.starts_with("define i64 @wf_read_pad(") {
                line.replace(" #0 {", " noinline #0 {")
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    exposed.push_str("\ndeclare i32 @wf__main_body(i32, ptr)\n");
    assert_eq!(
        module.matches("define i64 @wf_spine(").count(),
        1,
        "the fixture must expose exactly one generated spine"
    );
    assert_eq!(
        exposed.matches("define i64 @wf_read_pad(").count(),
        1,
        "the fixture retains exactly one source array reader"
    );
    assert_eq!(
        exposed.matches("define i32 @wf__unused_main_body(").count(),
        1,
        "the fixture must rename exactly one generated entry body"
    );
    assert_eq!(
        exposed.matches("define i32 @wf__unused_main(").count(),
        1,
        "the fixture must rename exactly one generated host entry"
    );
    exposed
}

#[cfg(target_os = "macos")]
const fn protected_page_signal() -> i32 {
    10
}

#[cfg(not(target_os = "macos"))]
const fn protected_page_signal() -> i32 {
    libc_sigsegv()
}

/// The same module with probing disabled on the one definition whose `define`
/// line contains `signature`.
///
/// Darwin probes large frames by target default even when `probe-stack` is
/// absent. LLVM's function-local `no-stack-arg-probe` attribute is therefore
/// the negative control: it suppresses that default only for the selected
/// definition and leaves the emitted module and every other definition's
/// production probe untouched.
fn ablate_probe(module: &str, signature: &str) -> String {
    assert!(
        !module.contains("attributes #1 ="),
        "the test-only negative-control attribute needs one fresh group"
    );
    let mut ablated = module
        .lines()
        .map(|line| {
            if line.starts_with("define ") && line.contains(signature) {
                line.strip_suffix(" #0 {")
                    .map(|head| format!("{head} #1 {{"))
                    .unwrap_or_else(|| line.to_owned())
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    ablated.push_str("\nattributes #1 = { \"no-stack-arg-probe\" }\n");
    ablated
}

/// Isolates the large-frame negative control from the probed fill helper.
///
/// At `-O2`, LLVM inlines the compiler-owned `array_filled` body into the
/// spine and propagates that callee's production `probe-stack` attribute back
/// onto the caller. Marking only this negative control's one fill definition
/// `noinline` keeps the same complete frame and fill operation while allowing
/// [`ablate_probe`] to remove the spine's own probe. The positive module and
/// every production definition remain unchanged.
fn ablate_large_frame_probe(module: &str) -> String {
    let mut fills = 0;
    let isolated = module
        .lines()
        .map(|line| {
            if line.starts_with("define void @wf_array_filled$instance$") {
                fills += 1;
                line.strip_suffix(" #0 {")
                    .map(|head| format!("{head} noinline #0 {{"))
                    .unwrap_or_else(|| line.to_owned())
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        fills, 1,
        "the large-frame control must have exactly one compiler-owned fill helper"
    );
    ablate_probe(&isolated, "@wf_spine(")
}

/// Confirms the exact positive probe and its absence from the exact negative
/// control before relying on that control's signal.
fn assert_native_spine_probe_was_ablated(probed: &str, ablated: &str) {
    let label = if cfg!(target_os = "macos") {
        "_wf_spine:"
    } else {
        "wf_spine:"
    };
    let probed = native_function_body(probed, label);
    let ablated = native_function_body(ablated, label);
    let has_probe = |body: &str| {
        if cfg!(target_os = "macos") {
            body.contains("chkstk")
        } else if cfg!(target_arch = "x86_64") {
            body.contains("subq\t$4096, %rsp") && body.contains("movq\t$0, (%rsp)")
        } else if cfg!(target_arch = "aarch64") {
            body.contains("sub\tsp, sp, #1, lsl #12") && body.contains("str\txzr, [sp]")
        } else {
            panic!("the stack floor has no qualified native-probe check on this architecture")
        }
    };
    assert!(
        has_probe(probed),
        "the positive-control spine has no recognized native stack probe:\n{probed}"
    );
    assert!(
        !has_probe(ablated),
        "the negative-control spine still contains a native stack probe:\n{ablated}"
    );
}

fn native_function_body<'assembly>(assembly: &'assembly str, label: &str) -> &'assembly str {
    assembly
        .split_once(label)
        .map(|(_, tail)| tail)
        // An unprobed leaf-like native function may need no unwind directives,
        // so `.cfi_endproc` is not a stable end marker for the very negative
        // control this helper examines. Clang emits this function-end marker
        // for both qualified architectures regardless of CFI presence.
        .and_then(|tail| tail.split_once("-- End function").map(|(body, _)| body))
        .expect("the native module must retain the selected function")
}

// ------------------------------------------- the compiler's own recursion

/// A boxed spine whose length is a runtime quantity, built by a counted loop.
///
/// The source contains no recursive function at all. The only recursion in the
/// program used to be the one the compiler generated to *destroy* the value at
/// scope exit, which is the point: a writer looking at this program can see no
/// depth to bound, cannot instrument the traversal, and cannot avoid it.
///
/// The loop read the old node out with `replace`, which is retired: [SET-1]
/// writes exactly one place and [WIN-3] gives the old value its disposition,
/// which for an affine one is its release. Here the old value must survive the
/// write, so the successor is [OP-11] `swap`: neither root is consumed, no
/// program point holds a hole, and the binding the exchange leaves behind is
/// the one the round releases. The allocation and release identities are
/// unchanged by that substitution.
fn boxed_spine_source(depth: u64) -> Vec<u8> {
    format!(
        r#"enum Tree {{
  Leaf();
  Branch(left: Box<Tree>, right: Box<Tree>);
}}

struct Holder {{
  node: Box<Tree>;
}}

fn boxed_leaf() -> result: Box<Tree> pure {{
  let leaf = Tree::Leaf();
  return box_new::<Tree>(value: move leaf);
}}

fn boxed_branch(left: Box<Tree>, right: Box<Tree>) -> result: Box<Tree> pure {{
  let branch = Tree::Branch(left: move left, right: move right);
  return box_new::<Tree>(value: move branch);
}}

fn main() -> status: std::process::ExitStatus pure {{
  let seed = boxed_leaf();
  let held = Holder(node: move seed);
  for @grow (i in 0_u64..{depth}_u64) {{
    let sibling = boxed_leaf();
    let placeholder = boxed_leaf();
    swap(first: &held.node, second: &placeholder);
    let taller = boxed_branch(left: move placeholder, right: move sibling);
    swap(first: &held.node, second: &taller);
  }}
  return std::process::exit_status(code: 0_u8);
}}
"#
    )
    .into_bytes()
}

/// A cleanup cycle that closes through a window instead of through a bare
/// cell.
///
/// `Box` supplies the indirection the target layout needs while the window
/// stays inside the cycle: `Chain` -> `Box<Slots<Chain>>` -> `Slots<Chain>` ->
/// `Chain`. Nothing about the program says "recursive"; as with the boxed
/// spine, the only recursion is the one the compiler would generate to destroy
/// the value.
///
/// The shape matters because the two indirections need different traversal
/// arms. A `Box` names one content, so one traversal step carries the whole
/// edge. A window names many elements whose reclamation order [STOR-3] fixes,
/// so it takes one step per element plus one for the block.
///
/// v0.59 modelled a vacant slot as `Option<Chain>` and moved values in and out
/// with `replace`, matching the `None()` arm each time. Both retire: [WIN-1]
/// says a slot inside the window always holds a value and no program point can
/// observe one as empty, so the vacancy is the window itself, and [OP-10]'s
/// `place_back` and `take_back` are what move the boundary. The `Option` layer
/// leaves the cycle with them, which shortens the chain by one node type but
/// not the cycle: `Slots<Chain>` still names `Chain`.
fn buffer_chain_source(depth: u64) -> Vec<u8> {
    format!(
        r#"enum Chain {{
  Nil();
  Cons(kids: Box<Slots<Chain>>);
}}

fn nest(inner: Chain) -> result: Chain pure {{
  let held = box_slots_new::<Chain>(capacity: 1_u64);
  place_back(window: &held.inner, value: move inner);
  return Chain::Cons(kids: move held);
}}

fn main() -> status: std::process::ExitStatus pure {{
  let holder = box_slots_new::<Chain>(capacity: 1_u64);
  let seed = Chain::Nil();
  place_back(window: &holder.inner, value: move seed);
  for @build (
    i in 0_u64..{depth}_u64,
    invariant width: holder.inner.cap == 1_u64,
    invariant stored: holder.inner.len == 1_u64
  ) {{
    let taken = take_back(window: &holder.inner);
    let grown = nest(inner: move taken);
    place_back(window: &holder.inner, value: move grown);
  }}
  return std::process::exit_status(code: 0_u8);
}}
"#
    )
    .into_bytes()
}

/// A value whose ownership graph is a chain rather than a cycle: deep in
/// nothing, and reached by the same emitter.
///
/// The chain is `Box<Slots<Box<u64>>>` -> `Slots<Box<u64>>` -> `Box<u64>` ->
/// `u64`, and no node type names another one above it.
const SHALLOW_OWNERSHIP: &[u8] = br#"fn main() -> status: std::process::ExitStatus pure {
  let slots = box_slots_new::<Box<u64>>(capacity: 2_u64);
  let boxed = box_new::<u64>(value: 7_u64);
  place_back(window: &slots.inner, value: move boxed);
  return std::process::exit_status(code: 0_u8);
}
"#;

/// Every compiler-derived drop in one module, with the drops each one calls.
pub(super) fn drop_glue_calls(module: &str) -> Vec<(String, Vec<String>)> {
    let mut glue: Vec<(String, Vec<String>)> = Vec::new();
    // A drop is a caller only inside its own definition. The program's own
    // functions call these helpers too, and attributing those calls to
    // whichever definition happened to be last would invent an edge.
    let mut inside = false;
    for line in module.lines() {
        if let Some(rest) = line.strip_prefix("define private void @wf.drop.") {
            let name = rest.split('(').next().expect("a definition names a symbol");
            glue.push((format!("wf.drop.{name}"), Vec::new()));
            inside = true;
            continue;
        }
        if line == "}" {
            inside = false;
            continue;
        }
        if inside
            && let Some((_, callee)) = line.split_once("call void @wf.drop.")
            && let Some((_, calls)) = glue.last_mut()
        {
            calls.push(format!(
                "wf.drop.{}",
                callee.split('(').next().unwrap_or_default()
            ));
        }
    }
    glue
}

/// The derived release of a type whose release graph has a cycle is one
/// release action per node type, entering itself where the graph closes
/// [PROV-6].
///
/// This assertion was its own opposite until 2026-09-04. The compiler ran such
/// a walk on an explicit heap worklist, and the property under test was that no
/// compiler-derived drop reached itself, because the recursion has no name in
/// the source. The owner deleted [PROV-6]'s release-graph cycle refusal that
/// day and ruled that the walk may recurse: a cycle can arise only where a heap
/// is allowed, and a heap-allowed program's resource behaviour is a runtime
/// quantity already, while the worklist bought its bounded depth with a
/// `realloc` on the release path — an allocation, and on refusal an abort, that
/// the writer never wrote. The recursion is not invisible: it is a `STACK
/// cycle` row of the stack ledger, beside the recursions the writer did write.
///
/// The check is over the emitted module rather than over a list of names, so a
/// new nominal shape whose glue closes a cycle is read here without anyone
/// remembering to extend a table.
#[test]
fn a_cyclic_release_graph_lowers_to_one_release_action_that_enters_itself() {
    // Both owning indirections a cleanup cycle can close through.
    assert_recursive_drop_glue(&boxed_module());
    assert_recursive_drop_glue(&buffer_module());
}

fn assert_recursive_drop_glue(module: &str) {
    let glue = drop_glue_calls(module);
    assert!(
        !glue
            .iter()
            .any(|(name, _)| name.starts_with("wf.drop.step.")),
        "the release walk is the type's own release actions and no traversal \
         driver: {module}"
    );
    assert!(
        !module.contains("@wf.drop.push"),
        "a release walk allocates nothing: {module}"
    );
    let mut inside = false;
    for line in module.lines() {
        if line.starts_with("define private void @wf.drop.") {
            inside = true;
        } else if line == "}" {
            inside = false;
        } else if inside {
            assert!(
                !line.contains("@wf_resource_abort")
                    && !line.contains("@realloc")
                    && !line.contains("@malloc"),
                "a release action allocates nothing and reaches no abort: \
                 {line}"
            );
        }
    }
    assert!(
        release_graph_has_cycle(&glue),
        "a recursive nominal's release graph must close: {module}"
    );
}

fn release_graph_has_cycle(glue: &[(String, Vec<String>)]) -> bool {
    let index: std::collections::HashMap<&str, usize> = glue
        .iter()
        .enumerate()
        .map(|(position, (name, _))| (name.as_str(), position))
        .collect();
    // The release graph closes, so exactly one of these definitions reaches
    // itself through the module's own call graph. Depth-first, looking for the
    // back edge rather than refusing it.
    let mut colour = vec![0_u8; glue.len()];
    let mut path: Vec<(usize, usize)> = Vec::new();
    let mut closed = false;
    for root in 0..glue.len() {
        if colour[root] != 0 {
            continue;
        }
        colour[root] = 1;
        path.push((root, 0));
        while let Some((node, cursor)) = path.last_mut() {
            let node = *node;
            let Some(callee) = glue[node].1.get(*cursor) else {
                colour[node] = 2;
                path.pop();
                continue;
            };
            *cursor += 1;
            let Some(target) = index.get(callee.as_str()).copied() else {
                continue;
            };
            if colour[target] == 1 {
                closed = true;
                continue;
            }
            if colour[target] == 0 {
                colour[target] = 1;
                path.push((target, 0));
            }
        }
    }
    closed
}

/// An acyclic ownership graph must have no direct or mutual release cycle.
#[test]
fn an_ownership_chain_keeps_its_straight_line_drop() {
    let module = compile(SHALLOW_OWNERSHIP);
    let glue = drop_glue_calls(&module);
    assert!(!glue.is_empty(), "the fixture must derive release actions");
    assert!(!release_graph_has_cycle(&glue), "{module}");
}

/// A window in a cleanup cycle whose elements each own further storage.
///
/// The four `replace` writes and their matched `None()` arms retire with
/// [SET-2] and [LIV-2]; the successor is four [OP-10] `place_back` calls onto
/// a window that starts empty [OP-13] and whose slots are never observable as
/// empty [WIN-1]. The element identities and their order are unchanged.
const WIDE_BUFFER_CYCLE: &[u8] = br#"enum Chain {
  Nil();
  Cons(kids: Box<Slots<Chain>>);
}

fn leafy() -> result: Chain pure {
  let held = box_slots_new::<Chain>(capacity: 1_u64);
  return Chain::Cons(kids: move held);
}

fn main() -> status: std::process::ExitStatus pure {
  let slots = box_slots_new::<Chain>(capacity: 4_u64);
  let child0 = leafy();
  place_back(window: &slots.inner, value: move child0);
  let child1 = leafy();
  place_back(window: &slots.inner, value: move child1);
  let child2 = leafy();
  place_back(window: &slots.inner, value: move child2);
  let child3 = leafy();
  place_back(window: &slots.inner, value: move child3);
  let root = Chain::Cons(kids: move slots);
  return std::process::exit_status(code: 0_u8);
}
"#;

/// The body of one definition, from its `define` line to its closing brace.
fn definition_body<'a>(module: &'a str, signature: &str) -> &'a str {
    let start = module
        .find(signature)
        .unwrap_or_else(|| panic!("the module defines {signature}: {module}"));
    let body = &module[start..];
    let end = body.find("\n}\n").expect("a definition closes");
    &body[..end]
}

/// [STOR-3] fixes a window's release as each element's release in ascending
/// logical index order. Its enclosing `Box` then performs the one heap free,
/// and a window inside a cycle follows the same walk as any other.
///
/// The run action walks the live elements but frees no separate backing:
/// [TYPE-9] places a runtime-capacity `Slots` only inside its `Box`, and
/// [STOR-1] gives that pair exactly one heap object. The enclosing owner action
/// therefore calls the run action and then frees the cell [STOR-3].
///
/// The order is pinned where it is chosen because nothing downstream can see
/// it: [STOR-3] gives memory reclamation the empty effect row. Walking the
/// indices downward, or freeing the block first, both emit a release that
/// looks right and reclaims in the wrong order — and freeing first would read
/// every element out of storage the release had already given back.
///
/// The case read a worklist step here until 2026-09-04. The order it pins is
/// the same one; what changed is that the elements are released by an ordinary
/// ascending loop rather than pushed in reverse onto a last-in first-out list.
#[test]
fn a_buffer_in_a_cleanup_cycle_is_walked_in_the_order_the_rule_fixes() {
    let module = buffer_module();
    let run_definitions = module
        .lines()
        .filter(|line| line.starts_with("define private void @wf.drop.run."))
        .collect::<Vec<_>>();
    let [run_definition] = run_definitions.as_slice() else {
        panic!("the module must define one runtime-window release: {run_definitions:?}");
    };
    let buffer_drop = definition_body(&module, run_definition.trim_end_matches(" #0 {"));
    assert!(
        buffer_drop.contains("%index = phi i64 [ 0, %entry ], [ %next, %body ]")
            && buffer_drop.contains("%next = add i64 %index, 1"),
        "the elements must be released from index zero upward: {buffer_drop}"
    );
    let element = buffer_drop
        .find("%element = load")
        .expect("the buffer release reads each live element");
    let body = buffer_drop
        .split("body:")
        .nth(1)
        .expect("element loop body")
        .split("done:")
        .next()
        .unwrap();
    assert!(
        body.contains("call void @wf.drop."),
        "each live element invokes its release action: {body}"
    );
    let element_release = buffer_drop
        .find("call void @wf.drop.")
        .expect("the run release calls its element release");
    assert!(
        element < element_release,
        "the element must be loaded before its release action: {buffer_drop}"
    );
    assert!(
        body.contains("br label %walk") && !body.contains("call void @free("),
        "the element loop returns to its header and frees no enclosing cell: {body}"
    );
    assert!(
        !buffer_drop.contains("call void @free("),
        "Slots has no storage reclamation separate from its Box: {buffer_drop}"
    );
    let owner_drop = module
        .split("\n\n")
        .find(|body| {
            body.starts_with("define private void @wf.drop.")
                && body.contains("call void @wf.drop.run.")
                && body.contains("call void @free(")
        })
        .expect("the enclosing owner releases the run and its one Box cell");
    let elements = owner_drop
        .find("call void @wf.drop.run.")
        .expect("the owner calls the run release");
    let cell = owner_drop
        .find("call void @free(")
        .expect("the owner frees the Box cell");
    assert!(
        elements < cell,
        "every element must be released before the Box cell that holds it: {owner_drop}"
    );
}

/// Allocation-instance identities expose omitted/double releases, field/element
/// order and backing lifetime directly. Large depth and unverified allocator
/// scribbling did not establish those properties.
///
/// The boxed-branch arithmetic is re-derived and holds under the `swap`
/// rewrite: each round still allocates sibling, placeholder and branch in that
/// order and releases the placeholder the exchange left behind, and the final
/// walk still descends left before right.
///
/// Each `box_slots_new` is one interposed allocation: [TYPE-9] makes the
/// runtime-capacity shape the content of its `Box`, and [STOR-1] stores the
/// pair in exactly one heap object. The nested and wide fixtures each create
/// one root cell and four child cells.
#[test]
fn branching_and_nested_release_walks_reclaim_each_instance_in_order() {
    let mut boxed_trace = String::from("A1;");
    for round in 0..4 {
        let first = 2 + 3 * round;
        boxed_trace.push_str(&format!(
            "A{first};A{};A{};F{};",
            first + 1,
            first + 2,
            first + 1
        ));
    }
    boxed_trace.push_str("F1;");
    for round in 0..4 {
        let first = 2 + 3 * round;
        boxed_trace.push_str(&format!("F{first};F{};", first + 2));
    }
    let nested_trace = (1..=5).map(|id| format!("A{id};")).collect::<String>()
        + &(2..=5).map(|id| format!("F{id};")).collect::<String>()
        + "F1;";
    let wide_trace = nested_trace.clone();
    for (name, module, limit, expected) in [
        ("boxed branches", boxed_module(), 13, boxed_trace),
        ("nested buffer", buffer_module(), 5, nested_trace),
        ("wide buffer", compile(WIDE_BUFFER_CYCLE), 5, wide_trace),
    ] {
        let observed = module
            .replace("@malloc(", "@wf_test_allocate(")
            .replace("@free(", "@wf_test_release(");
        let directory = test_directory();
        let observer = super::owned_places::allocation_observer(limit, 0);
        let executable = build_linked_executable(&observed, Some(&observer), &[], &directory);
        let output = Command::new(executable)
            .output()
            .expect("run observed release walk");
        assert_eq!(output.status.code(), Some(0), "{name}: {output:?}");
        assert_eq!(output.stdout, expected.as_bytes(), "{name}");
        assert!(output.stderr.is_empty(), "{name}: {output:?}");
        std::fs::remove_dir_all(directory).expect("remove release observation");
    }
}
