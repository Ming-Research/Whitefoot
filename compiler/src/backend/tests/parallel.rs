//! Parallel compiler internals: frame/clone/ABI emission, native object
//! observations and controlled runtime boundaries. Complete tree/spine/window
//! results live in programs; native construction is explicit in these helpers.
//!
//! # Kernel spec v0.60
//!
//! [PAR-1] survives the amendment and was rewritten around it: two adjacent
//! statements of one block may overlap exactly when their write paths are
//! disjoint from each other's read and write paths, judged by the same
//! path-overlap and index/range-disjointness relation as [EFF-5] and [OWN-7].
//! Loans and arenas contribute no footprint any more, and [STOR-8] gives
//! allocation and release no effect entry at all, so neither can deny an
//! overlap. The lowering cases below therefore keep their subject; their `.wf`
//! fixtures were retargeted: `region { .. }` wrappers deleted with regions
//! [OWN-3, OWN-4, OWN-10, FORM-8], `array<T, N>` spelled `Array<T, n>` and
//! built by [OP-13]'s `array_filled`, `buffer_new(count, value)` replaced by
//! [OP-13]'s `box_array_filled` over the one heap [STOR-8] whose content is
//! the field `inner` [TYPE-9], `slice_of` replaced by
//! [REF-4]'s range reference `&x[lo..hi]` at the parameter kind `&[T]`,
//! `len_of(p)` replaced by [OP-15]'s member read `p.len`, `&uniq` replaced by
//! the one reference spelling `&`, and an effect row on a by-value parameter
//! dropped entirely, because [EFF-1] roots every entry at a reference
//! parameter and a row rooted at an `own` parameter is a rejection.
//!
//! One test retired:
//!
//! - `heap_box_loop_keeps_provider_order_and_updates_borrowed_owners` retired
//!   with [PROV-1] and [BLK-0] through [BLK-4]: its whole subject was a
//!   store-provider program - a `Heap<'heap>` parameter, an `allocates(heap)`
//!   effect entry, region-parameterized `Box<'s, u64>` cells - whose
//!   observation was a per-allocation refusal schedule, refusing each of eight
//!   `heap_box` calls in turn and reading the `Err` arm the writer had to
//!   spell. [STOR-8] makes allocation total over one heap: no construction
//!   returns a `Result`, exhaustion terminates from the trusted base outside
//!   the language, and allocation carries no effect entry, so there is no
//!   refusal arm to schedule and nothing left of the ordering this case
//!   observed. The successors are [STOR-8] for the one heap and the absent
//!   effect category, [OP-9] for the allocation-size obligation now checked at
//!   the source, [OP-11] `swap(first: &a, second: &b)` for the two-place
//!   exchange the helper spelled `set (a, b) = move b, move a;` [SET-2], and
//!   [WIN-3] for the release of a displaced affine owner. Per-element release
//!   ordering of a boxed affine run is kept by `heap_programs` and
//!   `resource_enums`; the lane-side owner accounting this case shared with
//!   `owned_pair_results_survive_ordinary_join_and_forced_refusal` stays in
//!   [`run_owned_lane_cases`], which that case still drives.

use std::path::Path;
use std::process::Command;

use crate::backend::emitter::emit_llvm_with_layout;
use crate::target::{
    PARALLEL_LANE_FRAME_ALIGNMENT, TargetLayout, TargetLayoutFailure, TargetObject,
    parallel_lane_frame_layout,
};

use super::system::{with_ir, with_parallel_ir};
use super::{
    build_executable, build_linked_executable, compile_and_run, emit, emit_with_overlap,
    emitted_function, module_requires_parallel_runtime, test_directory,
};

/// A pure recursive fold over a heap tree, the smallest shape that has
/// every eligible form at once: a self-recursive sibling pair inside `fold`,
/// sibling constructor pairs inside `pair`, `quad`, and `oct`, and a run of
/// four sibling calls in `main`. Its whole result is written to standard
/// output, so a difference anywhere in the tree is a difference in the bytes.
const OVERLAPPING_FOLD: &[u8] = include_bytes!("../../../../tests/programs/parallel/tree.wf");

fn fold_module(parallel: bool) -> String {
    use std::sync::OnceLock;
    static PLAIN: OnceLock<String> = OnceLock::new();
    static PARALLEL: OnceLock<String> = OnceLock::new();
    let cell = if parallel { &PARALLEL } else { &PLAIN };
    let module = cell
        .get_or_init(|| {
            if parallel {
                emit_with_overlap(OVERLAPPING_FOLD)
            } else {
                emit(OVERLAPPING_FOLD)
            }
        })
        .clone();
    super::exhaustion::assert_stack_probes(&module);
    module
}

const LANE_FRAME_LAYOUT_FUNCTIONS: &[u8] =
    br#"fn exact_frame(values: Array<u8, 255>) -> result: u8 pure {
  return values[0_u64];
}

fn over_frame(values: Array<u8, 256>) -> result: u8 pure {
  return values[0_u64];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;

fn lane_frame_program(length: u64) -> Vec<u8> {
    format!(
        "fn first(values: Array<u8, {length}>) -> result: u8 pure {{\n  \
         return values[0_u64];\n}}\n\n\
         fn main() -> status: std::process::ExitStatus pure {{\n  \
         let left_values = array_filled::<u8, {length}>(value: 7_u8);\n  \
         let right_values = array_filled::<u8, {length}>(value: 9_u8);\n  \
         let left = first(values: left_values);\n  \
         let right = first(values: right_values);\n  \
         if left != 7_u8 {{\n    return std::process::exit_status(code: 1_u8);\n  }}\n  \
         if right != 9_u8 {{\n    return std::process::exit_status(code: 2_u8);\n  }}\n  \
         return std::process::exit_status(code: 0_u8);\n}}\n"
    )
    .into_bytes()
}

/// A pure call handed out while a second pure call, written as an `if`
/// condition, runs on this thread.
///
/// A writer may not spell a `Bool` match — [GRAM-6] demands `if` — but the
/// checker turns the `if` into one, so a call in condition position is a call
/// in scrutinee position and reaches the judgment exactly as a `let`
/// right-hand side does. That makes it an ordinary [PAR-1] candidate for
/// *compute* overlap, with no target operation anywhere in the pair — the
/// reach of a change whose motivation was completion I/O.
///
/// Both halves of the result are observable: the low byte of the handed-out
/// call's value, so a lost or unjoined hand-out shows, and a marker the
/// selected arm writes, so a condition decided wrongly shows too.
const IF_CONDITION_SIBLING: &[u8] = br#"fn mixdown(a: u64, b: u64) -> result: u64 pure {
  let spun = irotl(a, 13_u32);
  let scattered = imulhi(b, 2654435761_u64);
  let blended = ixor(spun, b);
  return ixor(blended, scattered);
}

fn odd(v: u64) -> result: Bool pure {
  let low = iand(v, 1_u64);
  return low == 1_u64;
}

fn last_byte(v: u64) -> result: u8 pure {
  let low = iand(v, 255_u64);
  match cvt.checked::<u64, u8>(low) {
    Ok(value: byte) => {
      return byte;
    }
    Err(error: problem) => {
      return 0_u8;
    }
  }
}

fn main(inputs: std::process::Inputs) -> status: std::process::ExitStatus pure {
  doc "A pure call handed out while a pure call written as an if condition runs.";
  let std::process::Inputs(args: unused_args, cwd: unused_cwd, stdout: out, stderr: unused_stderr, handles: entry_factory, stdin: unused_stdin) = move inputs;
  std::fs::close_directory(factory: &entry_factory, directory: move unused_cwd);
  let report = box_array_filled::<u8>(count: 2_u64, value: 0_u8);
  let value = mixdown(a: 11_u64, b: 22_u64);
  if odd(v: 33_u64) {
    set report.inner[1_u64] = 89_u8;
  }
  let byte = last_byte(v: value);
  set report.inner[0_u64] = byte;
  let ordinary_source_2 = &report.inner[0_u64..2_u64];
  match std::io::write_once(factory: &entry_factory, output: &out, source: ordinary_source_2, start: 0_u64, end: 2_u64) {
    Ok(value: accepted) => {
      return std::process::exit_status(code: 0_u8);
    }
    Err(error: problem) => {
      return std::process::exit_status(code: 1_u8);
    }
  }
}
"#;

/// Two sibling calls the judgment refuses: the second reads the first's
/// binding, so condition 1 denies the pair and nothing may be handed out.
const DEPENDENT_SIBLINGS: &[u8] = br#"fn twice(v: u64) -> result: u64 pure {
  return imax(v, v);
}

fn main() -> status: std::process::ExitStatus pure {
  let first = twice(v: 3_u64);
  let second = twice(v: first);
  let total = imax(first, second);
  return std::process::exit_status(code: 0_u8);
}
"#;

/// A program whose own functions are spelled like the runtime's entry points.
///
/// It overlaps, so the module carries the runtime symbols too, and both sets
/// have to coexist. `par_acquire_lane`, `par_publish`, `par_join`, and `par_release`
/// are ordinary IDENTs [FORM-3], so nothing may stop a writer from declaring
/// them.
const RUNTIME_SHAPED_NAMES: &[u8] = br#"fn par_acquire_lane(x: u64) -> result: u64 pure {
  return imax(x, x);
}

fn par_publish(x: u64) -> result: u64 pure {
  return imax(x, x);
}

fn par_join(x: u64) -> result: u64 pure {
  return imax(x, x);
}

fn par_release(x: u64) -> result: u64 pure {
  return imax(x, x);
}

fn par_thunk_0(x: u64) -> result: u64 pure {
  return imax(x, x);
}

fn main() -> status: std::process::ExitStatus pure {
  let a = par_acquire_lane(x: 1_u64);
  let b = par_publish(x: 2_u64);
  let c = par_thunk_0(x: 3_u64);
  let d = par_join(x: 4_u64);
  let e = par_release(x: 5_u64);
  let ab = imax(a, b);
  let cd = imax(c, d);
  let abcd = imax(ab, cd);
  let total = imax(abcd, e);
  return std::process::exit_status(code: 0_u8);
}
"#;

/// The backend's own symbols never collide with a source function's.
///
/// A source function is emitted as `wf_` plus its IDENT, and [FORM-3] spells
/// IDENT `[a-z][a-z0-9_]*`, so the `wf__par_` prefix the runtime uses is
/// unreachable from source. Without that reservation this program is accepted
/// by the checker and then rejected by the host toolchain with a raw
/// `invalid redefinition of function` — an accepted program failing to build,
/// which no source-level diagnostic explains.
#[test]
fn a_program_named_like_the_runtime_still_compiles_and_links() {
    let module = emit_with_overlap(RUNTIME_SHAPED_NAMES);
    assert!(
        module_requires_parallel_runtime(&module),
        "the fixture must actually hand work out:\n{module}"
    );
    assert!(
        module.contains("define i64 @wf_par_acquire_lane(i64 "),
        "the source function keeps its own symbol:\n{module}"
    );
    assert!(
        module.contains("define weak ptr @wf__par_acquire_lane(i64 %bytes) #0 {"),
        "the runtime keeps its reserved symbol:\n{module}"
    );
    let output = compile_and_run(&module);
    assert_eq!(output.status.code(), Some(0));
}

/// The selected target lays out the exact aggregate the worker thunk reads:
/// every parameter in declaration order followed by the result. A 255-byte
/// array plus a byte result reaches the 256-byte runtime boundary exactly;
/// adding one parameter byte remains a valid source function but makes this
/// optional schedule ineligible. Reducing the target address domain below the
/// exact aggregate is a target-layout failure rather than a capacity decline.
#[test]
fn selected_target_proves_the_complete_ordinary_lane_frame() {
    with_ir(LANE_FRAME_LAYOUT_FUNCTIONS, |program| {
        let host = TargetLayout::host().expect("the backend test runs on a supported host layout");
        let exact = program
            .functions()
            .iter()
            .find(|function| function.name() == "exact_frame")
            .expect("the exact-boundary function must lower");
        let over = program
            .functions()
            .iter()
            .find(|function| function.name() == "over_frame")
            .expect("the over-boundary function must lower");
        let layout = |target, function: &crate::IrFunction, carries_budget| {
            parallel_lane_frame_layout(
                target,
                program.nominals(),
                program.elements(),
                function.parameters().iter().map(|(_, ty)| *ty),
                function.result(),
                carries_budget,
            )
        };

        // KEPT AS WRITTEN for the lowering port: 255 element bytes plus the
        // one-byte result reach the slot exactly only while a constant-capacity
        // `Array<u8, 255>` [TYPE-9] is laid out as its 255 elements and nothing
        // else. If [STOR-6] gives the shape a measure word or padding, the two
        // fixture lengths (255 and 256) must be re-derived from the new layout;
        // the property this case is about is that the exact boundary fits and
        // one byte past it does not.
        let exact_layout = layout(host, exact, false)
            .expect("the exact frame is target-representable")
            .expect("the exact frame fits the lane slot");
        assert_eq!(exact_layout.size(), crate::target::LANE_FRAME_BYTES);
        assert_eq!(exact_layout.align(), 1);
        assert!(exact_layout.align() <= PARALLEL_LANE_FRAME_ALIGNMENT);
        assert_eq!(
            layout(host, over, false),
            Ok(None),
            "a target-representable frame beyond the lane capacity must decline overlap"
        );
        // A callback into a budget-carrying variant takes the budget across in
        // the frame, so a frame that already fills the slot exactly declines
        // the offer rather than overrunning it. The refusal is the existing
        // one: the group's calls run in place.
        assert_eq!(
            layout(host, exact, true),
            Ok(None),
            "a frame that exactly fills the slot cannot also carry a budget"
        );

        let short_domain =
            host.with_address_index_max_for_test(crate::target::LANE_FRAME_BYTES - 1);
        assert_eq!(
            layout(short_domain, exact, false),
            Err(TargetLayoutFailure::Unrepresentable(
                TargetObject::ParallelLaneFrame
            )),
            "the aggregate itself must fit the selected target's address domain"
        );
    });
}

/// The two constants checked by selected-target lane layout are the scheduler
/// core slot's actual byte capacity and base alignment.
///
/// The slot moved from `par_runtime.c` to `sched/core.h` when the core became
/// the runtime (design §7), and so did the two numbers; nothing else about
/// this case changes, because the lowering's question is the same one.
#[test]
fn ordinary_lane_frame_limits_match_the_runtime_slot() {
    let core = crate::SCHED_CORE_HEADER;
    let declared = core
        .lines()
        .find_map(|line| line.strip_prefix("#define WF_SCHED_FRAME_BYTES "))
        .expect("the core must state its frame capacity");
    assert_eq!(
        declared
            .trim()
            .trim_end_matches('u')
            .parse::<u64>()
            .expect("a decimal capacity"),
        crate::target::LANE_FRAME_BYTES
    );
    assert!(
        crate::SCHED_CORE_SOURCE.contains(&format!(
            "_Alignas({PARALLEL_LANE_FRAME_ALIGNMENT}) unsigned char frame[WF_PAR_FRAME_BYTES];"
        )),
        "the core slot must provide the alignment target layout relies on"
    );
}

/// A frame at the runtime boundary is handed out with the selected-target
/// size as a constant. The same valid source shape one byte wider stays on the
/// ordinary sequential call path: no thunk, lane acquisition, or address-size
/// expression is emitted for it. Both modules pass the host LLVM toolchain and
/// preserve the source result.
#[test]
fn ordinary_overlap_uses_only_target_proved_lane_frames() {
    // KEPT AS WRITTEN for the lowering port: 255 and 256 are the exact and
    // one-past-exact element counts of a constant-capacity `Array<u8, n>`
    // under the current [STOR-6] layout. Re-derive both if that layout moves.
    let exact = emit_with_overlap(&lane_frame_program(255));
    assert!(module_requires_parallel_runtime(&exact));
    assert!(exact.contains(&format!(
        "call ptr @wf__par_acquire_lane(i64 {})",
        crate::target::LANE_FRAME_BYTES
    )));
    assert!(
        !exact.contains("@wf__par_acquire_lane(i64 ptrtoint"),
        "lane size must come from selected-target layout, not emitted address arithmetic"
    );

    let over = emit_with_overlap(&lane_frame_program(256));
    assert!(
        !module_requires_parallel_runtime(&over),
        "a frame beyond the runtime slot must keep the optional overlap sequential"
    );
    assert!(!over.contains("@wf__par_thunk_"));

    let exact_output = compile_and_run(&exact);
    assert_eq!(exact_output.status.code(), Some(0));
    let over_output = compile_and_run(&over);
    assert_eq!(over_output.status.code(), Some(0));
}

/// One handed-out call emits its outlined thunk, a lane acquisition, the frame
/// stores and publication inside the granted edge, and a join whose refusal
/// edge makes the same call this thread would have made anyway.
#[test]
fn a_permitted_pair_is_outlined_offered_and_joined() {
    let module = fold_module(true);

    // The thunk is the outlined call: it loads the arguments out of the frame,
    // calls the same monomorphized function the inline edge calls, and stores
    // the result back into the frame. Its number is the module's, so the
    // assertion is on the shape rather than on which group came first.
    //
    // `fold` is recursive, so under the default recursion budget the function
    // both edges call is the component's budget-carrying variant. The
    // published callback reads the budget out of the frame the offer wrote it
    // into; the inline edge carries the same value in a register.
    assert!(
        module.contains("(ptr %frame) #0 {\nentry:\n  %p0 = getelementptr inbounds "),
        "no outlined thunk over a frame:\n{module}"
    );
    assert!(
        module.contains("%result = call i64 @wf__par_budget_fold(ptr %a0, i64 %ab)"),
        "the thunk must call the same function the inline edge calls, with the \
         budget the offer published:\n{module}"
    );
    assert!(
        function_body(&module, "@wf__par_budget_fold")
            .contains("= call i64 @wf__par_budget_fold(ptr "),
        "the inline edge must name the function the thunk calls:\n{module}"
    );
    assert!(
        module.contains("  store i64 %result, ptr %slot\n  ret void\n"),
        "the thunk must leave its result in the frame:\n{module}"
    );
    // Every runtime entry point is the module's own weak definition, so a
    // module that hands work out is still a complete program.
    for weak in [
        "define weak ptr @wf__par_acquire_lane(i64 %bytes) #0 {",
        "define weak void @wf__par_publish(ptr %frame, ptr %fn) #0 {",
        "define weak void @wf__par_join(ptr %frame) #0 {",
        "define weak void @wf__par_release(ptr %frame) #0 {",
    ] {
        assert!(module.contains(weak), "no weak `{weak}`:\n{module}");
    }

    // `fold`'s own recursive pair: a lane is acquired and the first call is
    // published to it, the second runs inline on this thread, and only then is
    // the published one joined. The ordering is what makes the overlap window
    // exactly the second call. The body is the component's budget-carrying
    // variant, because that is where a recursive member's body is emitted
    // under the default budget; both recursive calls therefore name the
    // variant and carry the level the caller has left.
    let body = function_body(&module, "@wf__par_budget_fold");
    let acquisition = body
        .find("= call ptr @wf__par_acquire_lane(i64 ")
        .expect("fold must acquire a lane for its first recursive call");
    let publish = body
        .find("call void @wf__par_publish(ptr")
        .expect("fold must publish the acquired lane its outlined call");
    let inline = body
        .find("par.offered.")
        .and_then(|start| {
            body[start..]
                .find("call i64 @wf__par_budget_fold(")
                .map(|at| start + at)
        })
        .expect("fold must run its second recursive call inline");
    let join = body
        .find("call void @wf__par_join(ptr")
        .expect("fold must join what it offered");
    assert!(
        acquisition < publish,
        "lane acquisition must precede the publish:\n{body}"
    );
    assert!(
        publish < inline,
        "the offer must precede the inline call:\n{body}"
    );
    assert!(
        inline < join,
        "the join must follow the inline call:\n{body}"
    );
    // The stores and the publish live inside the granted edge, so a refused
    // hand-out writes nothing and builds nothing.
    let offer_block = body
        .split("\npar.offer.")
        .nth(1)
        .expect("the granted edge must have its own block");
    let offer_block = offer_block
        .split_once("\npar.offered.")
        .expect("the granted edge must rejoin")
        .0;
    assert!(
        offer_block.contains("  store ") && offer_block.contains("@wf__par_publish"),
        "the frame stores and the publish must be inside the granted edge:\n{body}"
    );
    // The refused edge makes the same call the inline edge makes, so the two
    // edges are one lowering of one source call reached two ways.
    let refused = body
        .find("\npar.inline.")
        .and_then(|start| {
            body[start..]
                .find("call i64 @wf__par_budget_fold(")
                .map(|at| start + at)
        })
        .expect("the refused edge must make the call on this thread");
    assert!(
        inline < refused,
        "the refusal edge belongs to the join, not the offer:\n{body}"
    );
    // Nothing between the offer and the join reads the offered value: the
    // value is defined by the phi in the block both edges branch to.
    let read = body
        .find("\npar.done.")
        .expect("the joined value must be read in the join's own block");
    assert!(
        join < read,
        "the value must be read after the join:\n{body}"
    );
    assert!(
        body[read..].contains(" = phi i64 [ "),
        "the joined value must be the phi of the two edges:\n{body}"
    );
}

/// A call written as an `if` condition is a compute-overlap join site, and the
/// program it joins publishes the same bytes at every worker count.
///
/// This is the compute half of the same change that let a `match` scrutinee be
/// judged: an `if` checks into a `Bool` match, so the call in its condition is
/// reached by exactly the machinery a `let` right-hand side is reached by, with
/// no rule of its own. Nothing in the program performs a target operation, so
/// the group here is the ordinary [PAR-1] compute lowering — acquire, publish,
/// the condition call inline on this thread, join, phi — and it is worth
/// pinning because the batch that opened this position was about I/O and would
/// not otherwise have covered it.
///
/// `WF_WORKERS` is the knob that matters: it decides whether a lane is granted,
/// which is what separates the published edge of the join from the refused one.
/// `0` and `1` are both the opt-out — fewer than two lanes of execution is the
/// sequential world either way — and `4` starts a pool that grants, so the loop
/// runs both edges.
#[test]
fn a_call_written_as_an_if_condition_joins_a_compute_overlap_group() {
    // KEPT AS WRITTEN for the lowering port: the fixture's statement order is
    // the v0.59 one, so the run the judgment permits still leads with the
    // allocation of `report`. Under v0.60's [PAR-1] every adjacent pair is
    // judged, including a pair whose first member is a construction rather
    // than a call, so whether this group's leading member is the hand-out the
    // ordering below names is a lowering decision. If the group's first
    // published member turns out not to be `mixdown`, re-derive the ordering
    // here rather than moving the fixture's statements.
    let module = emit_with_overlap(IF_CONDITION_SIBLING);
    let body = function_body(&module, "@wf_main");
    let acquisition = body
        .find("= call ptr @wf__par_acquire_lane(i64 ")
        .expect("the first call must acquire a lane");
    let publish = body
        .find("call void @wf__par_publish(ptr")
        .expect("the acquired lane must be given the outlined call");
    let condition = body
        .find("call i1 @wf_odd(")
        .expect("the condition call must run on this thread");
    let join = body
        .find("call void @wf__par_join(ptr")
        .expect("the handed-out call must be joined");
    assert!(
        acquisition < publish && publish < condition && condition < join,
        "the condition call is the overlap window and the join follows it:\n{body}"
    );
    let done = body
        .find("\npar.done.")
        .expect("the joined value must be defined in the join's own block");
    assert!(
        join < done && body[done..].contains(" = phi i64 [ "),
        "the joined value must be the phi of the two edges:\n{body}"
    );

    let directory = test_directory();
    let executable = build_executable(&module, &directory);
    let mut runs = Vec::new();
    for workers in ["0", "1", "4"] {
        let output = Command::new(&executable)
            .env("WF_WORKERS", workers)
            .output()
            .expect("run the if-condition overlap probe");
        assert_eq!(output.status.code(), Some(0), "WF_WORKERS={workers}");
        // The handed-out value's low byte and the marker the selected arm
        // writes, in that order.
        assert_eq!(
            output.stdout, b"\x16Y",
            "WF_WORKERS={workers} published the wrong bytes"
        );
        runs.push((format!("WF_WORKERS={workers}"), output.stdout));
    }
    identical(&runs).expect("an if-condition join must not move one byte of the result");

    std::fs::remove_dir_all(&directory).expect("remove the test directory");
}

/// A group of three sibling calls whose values a loop carries: two members are
/// handed out, the third runs on this thread, and the loop header's phis name
/// the label the group's joins actually end at.
/// An independent scalar statement starts the permission run but cannot be
/// handed out; it must not hide the three-call subrun that follows it.
///
/// The loop is what makes the exit label observable. `main`'s entry block
/// reaches the header, so every carried value's phi has to name the label the
/// entry block ends at, and that label is decided before the joins are
/// written.
const THREE_MEMBER_GROUP_BEFORE_A_LOOP: &[u8] = br#"fn choose(value: u64) -> result: u64 pure {
  return imax(value, value);
}

fn main() -> status: std::process::ExitStatus pure {
  let prefix = 0_u64;
  let a = choose(value: 1_u64);
  let b = choose(value: 2_u64);
  let c = choose(value: 3_u64);
  let ab = imax(a, b);
  let acc = imax(ab, c);
  let i = 0_u64;
  loop @spin {
    let done = i >= 4_u64;
    if done {
      break @spin;
    }
    set acc = acc +wrap 1_u64;
    set i = i +wrap 1_u64;
  }
  match cvt.checked::<u64, u8>(acc) {
    Ok(value: code) => {
      return std::process::exit_status(code: code);
    }
    Err(error: problem) => {
      return std::process::exit_status(code: 255_u8);
    }
  }
}
"#;

/// A group's compute members are joined newest first, and the block continues
/// at the *first* published member's `par.done`.
///
/// This is design §4's order: the compute deque is Chase-Lev, so the owner can
/// only pop the newest end, and joining the newest hand-out first is what keeps
/// every join's target either at that end or already stolen. The order lives in
/// `compute_join_order`, and this pins what the emitter does with it — both the
/// sequence of joins and the label the two sites that predict it agree on. The
/// prediction is not cosmetic: a phi naming a block its predecessor does not
/// end at is a module `clang` rejects, so linking is part of the assertion.
///
#[test]
fn a_group_joins_its_compute_members_newest_first_and_continues_at_the_oldest() {
    // The group's hand-outs in publish order, read from the IR the emitter is
    // about to be handed, so the labels below are named rather than guessed.
    let handed_out = with_parallel_ir(THREE_MEMBER_GROUP_BEFORE_A_LOOP, |program| {
        program
            .functions()
            .iter()
            .flat_map(crate::IrFunction::overlaps)
            .map(|overlap| {
                overlap
                    .handed_out()
                    .iter()
                    .map(|member| member.ordinal())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    });
    let [group] = handed_out.as_slice() else {
        panic!("the source must lower to exactly one overlap group: {handed_out:?}");
    };
    let [first, second] = group.as_slice() else {
        panic!("the group must hand two of its three members out: {group:?}");
    };

    let module = emit_with_overlap(THREE_MEMBER_GROUP_BEFORE_A_LOOP);
    let body = emitted_function(&module, "main");
    let at = |needle: &str| {
        body.find(needle)
            .unwrap_or_else(|| panic!("missing `{needle}`:\n{body}"))
    };

    // Publish order is source order: the first member is offered a lane first.
    assert!(
        at(&format!("par.offer.v{first}:")) < at(&format!("par.offer.v{second}:")),
        "the members must be published in source order:\n{body}"
    );
    // Join order is the reverse: design §4's order, newest hand-out first.
    assert!(
        at(&format!("par.wait.v{second}:")) < at(&format!("par.wait.v{first}:")),
        "the newest hand-out must be joined first:\n{body}"
    );
    assert!(
        at(&format!("par.done.v{second}:")) < at(&format!("par.done.v{first}:")),
        "the newest hand-out's value must exist before the oldest is joined:\n{body}"
    );

    // The block therefore continues at the oldest hand-out's `par.done`, and
    // the loop header's phis are where that prediction is spent.
    let carried = body
        .lines()
        .filter(|line| line.contains(" = phi i64 [ ") && line.contains(", %par.done."))
        .collect::<Vec<_>>();
    assert!(
        !carried.is_empty(),
        "the loop must carry values out of the group's block:\n{body}"
    );
    for phi in carried {
        assert!(
            phi.contains(&format!(", %par.done.v{first} ]")),
            "the group's block ends at the first published member's join: {phi}"
        );
    }

    // Join order is not observable [PAR-1]: the program's value is the
    // source-order one whether a lane was granted or refused.
    let directory = test_directory();
    let executable = build_executable(&module, &directory);
    for workers in ["0", "1", "4"] {
        let output = Command::new(&executable)
            .env("WF_WORKERS", workers)
            .output()
            .expect("run the three-member group");
        assert_eq!(
            output.status.code(),
            Some(7),
            "WF_WORKERS={workers}: the loop must report the source-order result"
        );
    }
    std::fs::remove_dir_all(&directory).expect("remove the test directory");
}

/// Two unit helpers called as [GRAM-4] expression statements over disjoint
/// fields. A discarded result has no use, so the first call is handed out
/// exactly as a let-bound one is, and the fields both calls wrote are read only
/// after the join.
const EXPRESSION_STATEMENT_PAIR: &[u8] = br#"struct Pair {
  left: u64;
  right: u64;
}

fn fill_left(pair: &Pair, seed: u64) -> result: unit writes(pair.left) {
  let value = seed *wrap 3_u64;
  set deref(pair).left = value;
  return unit;
}

fn fill_right(pair: &Pair, seed: u64) -> result: unit writes(pair.right) {
  let value = seed *wrap 5_u64;
  set deref(pair).right = value;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let pair = Pair(left: 0_u64, right: 0_u64);
  fill_left(pair: &pair, seed: 2_u64);
  fill_right(pair: &pair, seed: 4_u64);
  let total = pair.left +wrap pair.right;
  match cvt.checked::<u64, u8>(total) {
    Ok(value: code) => {
      return std::process::exit_status(code: code);
    }
    Err(error: problem) => {
      return std::process::exit_status(code: 255_u8);
    }
  }
}
"#;

#[test]
fn an_expression_statement_pair_is_handed_out_and_joined() {
    let handed_out = with_parallel_ir(EXPRESSION_STATEMENT_PAIR, |program| {
        program
            .functions()
            .iter()
            .flat_map(crate::IrFunction::overlaps)
            .map(|overlap| overlap.handed_out().len())
            .collect::<Vec<_>>()
    });
    assert_eq!(
        handed_out,
        vec![1],
        "the two expression statements must lower to one group handing out the first"
    );

    let module = emit_with_overlap(EXPRESSION_STATEMENT_PAIR);
    let body = emitted_function(&module, "main");
    let at = |needle: &str| {
        body.find(needle)
            .unwrap_or_else(|| panic!("missing `{needle}`:\n{body}"))
    };
    assert!(
        at("call void @wf__par_publish(ptr") < at("call void @wf__par_join(ptr"),
        "the first call must be published before it is joined:\n{body}"
    );

    // 2*3 + 4*5 = 26 in every execution: the schedule is not an observation.
    let directory = test_directory();
    let executable = build_executable(&module, &directory);
    for workers in ["0", "1", "4"] {
        let output = Command::new(&executable)
            .env("WF_WORKERS", workers)
            .output()
            .expect("run the expression-statement pair");
        assert_eq!(
            output.status.code(),
            Some(26),
            "WF_WORKERS={workers}: the pair must report the source-order result"
        );
    }
    std::fs::remove_dir_all(&directory).expect("remove the test directory");
}

/// Runs one linked mixed fixture at three worker counts and demands the
/// source-order exit status from each.
///
/// [PAR-1] fixes every value to the source-order result and states that the
/// schedule is not an observation, so no worker count may move the status.
/// `WF_WORKERS=0` is the refused-lane edge, where every hand-out runs at its
/// own fallback call, and 4 is where a thief can reach one first.
fn a_mixed_fixture_reports(module: &str, expected: i32) {
    let directory = test_directory();
    let executable = build_executable(module, &directory);
    for workers in ["0", "1", "4"] {
        let output = Command::new(&executable)
            .env("WF_WORKERS", workers)
            .output()
            .expect("run the mixed group");
        assert_eq!(
            output.status.code(),
            Some(expected),
            "WF_WORKERS={workers}: the mixed group must report the source-order result"
        );
    }
    std::fs::remove_dir_all(&directory).expect("remove the test directory");
}

/// A recursion that hands one of its two calls out at every level, spelled at
/// one depth.
///
/// Every level scales the value it passes down, so no interprocedural fact
/// about the arguments collapses the sequential frame, and the two lowerings
/// of the same source are compared on the same terms. The whole result decides
/// the exit status, so neither build can drop the recursion.
const DEEP_RECURSION: &str = r#"fn leaf(v: f64) -> result: f64 pure {
  return fmul.strict(v, 0.5_f64);
}

fn spine(depth: u64, v: f64) -> result: f64 pure {
  let done = depth == 0_u64;
  if done {
    return v;
  }
  let below = depth -wrap 1_u64;
  let scaled = fmul.strict(v, 1.0009765625_f64);
  let a = spine(depth: below, v: scaled);
  let b = leaf(v: v);
  return fadd.strict(a, b);
}

fn main() -> status: std::process::ExitStatus pure {
  let total = spine(depth: DEPTH_u64, v: 1.0009765625_f64);
  let bits = reinterpret::<f64, u64>(total);
  let low = iand(bits, 1_u64);
  match cvt.checked::<u64, u8>(low) {
    Ok(value: byte) => {
      return std::process::exit_status(code: byte);
    }
    Err(error: wide) => {
      return std::process::exit_status(code: 2_u8);
    }
  }
}
"#;

/// Two machine-code ledgers protect both exact clone cost and the 48-byte
/// offer-state bound; neither assertion is a deep program execution.
#[test]
fn machine_frames_preserve_clone_cost_and_bound_offer_overhead() {
    let source = DEEP_RECURSION.replace("DEPTH", "1000").into_bytes();
    let overlapped_module = emit_with_overlap(&source);
    assert!(
        module_requires_parallel_runtime(&overlapped_module),
        "the fixture must hand work out, or this case is vacuous"
    );

    let plain_directory = test_directory();
    let overlapped_directory = test_directory();
    let plain = super::stack_ledger::ledger_lines(&emit(&source), &plain_directory);
    let overlapped = super::stack_ledger::ledger_lines(&overlapped_module, &overlapped_directory);

    let sequential = super::stack_ledger::reported_frame_bytes(&plain, "wf_spine");
    let clone = super::stack_ledger::reported_frame_bytes(&overlapped, "wf__par_seq_spine");
    assert_eq!(
        clone, sequential,
        "the --par build's sequential clone costs {clone} bytes a level where \
         the plain build's own function costs {sequential}, so handing calls \
         out is taxing activations that were never granted a lane"
    );

    let budget = super::stack_ledger::reported_frame_bytes(&overlapped, "wf__par_budget_spine");
    assert!(
        budget <= clone + 48,
        "budget frame {budget} exceeds clone {clone} by more than 48 bytes"
    );

    std::fs::remove_dir_all(&plain_directory).expect("remove the test directory");
    std::fs::remove_dir_all(&overlapped_directory).expect("remove the test directory");
}

/// The lane's frame is the lane's: asking for overlap adds no stack slot to
/// any function.
///
/// This is the whole resource bound of the hand-out. An earlier lowering put
/// the frame in the calling function's entry block, so every activation of an
/// eligible recursive function carried the slot and its argument spills
/// whether or not a lane was ever granted, and a `--par` build reached about a
/// quarter of the sequential build's recursion depth before dying on a bare
/// SIGSEGV. The comparison is against the default compilation of the same
/// source, so it measures the lowering rather than the program.
///
/// The count is taken over the *overlapped* world alone. A `--par` module
/// carries a second lowering of the eligible closure, and counting both copies
/// against one reference would compare a doubled module with a single one — a
/// failure that says nothing about whether a hand-out costs a slot. What the
/// clone costs is a separate and stronger question, answered by
/// `the_sequential_clone_is_the_sequential_lowering`: it is the sequential
/// lowering byte for byte, so its slots are the sequential build's slots.
#[test]
fn handing_a_call_out_adds_no_stack_slot() {
    let sequential = fold_module(false);
    let overlapped = fold_module(true);
    assert!(
        module_requires_parallel_runtime(&overlapped),
        "the fixture must hand work out, or this test is vacuous"
    );
    let actualized = without_clones(&overlapped);
    assert!(
        actualized.contains("@wf__par_publish"),
        "removing the clones must leave the overlapped world:\n{actualized}"
    );
    assert_eq!(
        actualized.matches("= alloca ").count(),
        sequential.matches("= alloca ").count(),
        "handing calls out must add no stack slot:\n{actualized}"
    );
}

/// The sequential clone is the sequential lowering: not similar to it, the
/// same bytes.
///
/// This is the load-bearing property of two-world compilation. The clone
/// exists so that every transform the default build gets fires on it — the one
/// that matters is LLVM's accumulator tail-recursion elimination, which the
/// hand-out's phi at `%par.done` forecloses and which was worth 2.96x on
/// `fib(38)` with the pool off. "Gets the same transforms" is not something a
/// test can ask LLVM directly; what it can ask is whether the input is the same
/// input, which is the whole property and is stronger than any list of
/// properties spelled out one at a time. A clone that drifted — a slot, a phi,
/// an operand read in a different order — would be a second lowering nobody
/// audited, and this case is what stops that.
///
/// The comparison restores the clone's own symbols, because the calls inside a
/// clone name clones: that renaming *is* the difference between the two
/// worlds, and after it there must be nothing left.
#[test]
fn the_sequential_clone_is_the_sequential_lowering() {
    let sequential = fold_module(false);
    let overlapped = fold_module(true);

    // The closure, spelled out: every function from which a handed-out call is
    // reachable. `pair`, `quad`, and `oct` hand out their sibling
    // constructors, `fold` its recursion, and `main` its four `oct` calls.
    // `leaf`, `branch`, `mix`, `low_byte`, and `spell` reach no hand-out at
    // all, so both worlds call the one copy of each and neither needs a clone.
    let mut cloned: Vec<_> = clone_symbols(&overlapped);
    cloned.sort_unstable();
    assert_eq!(
        cloned,
        [
            "@wf__par_seq_fold",
            "@wf__par_seq_main",
            "@wf__par_seq_oct",
            "@wf__par_seq_pair",
            "@wf__par_seq_quad",
        ],
        "the clone set must be the closure of the eligible calls:\n{overlapped}"
    );

    for symbol in &cloned {
        let clone = function_body(&overlapped, symbol);
        let restored = clone.replace("@wf__par_seq_", "@wf_");
        let reference = function_body(&sequential, &symbol.replace("@wf__par_seq_", "@wf_"));
        assert_eq!(
            restored, reference,
            "{symbol} is not the sequential lowering of its function"
        );
    }
}

/// Under the default policy the two worlds never call each other, and which
/// one runs is decided once. Optional refusal/frontier controls have their own
/// cases for entering sequential clones without a new runtime demand query.
///
/// Both halves matter and they fail differently. A clone that called back into
/// the overlapped world would re-enter the lowering it exists to avoid, and
/// would do so *below* the one place the choice is made, so the program would
/// pay the tax again with nothing left to catch it. And a selection made
/// anywhere but the bootstrap would be a test executed per call or per
/// activation: at best a branch in a hot loop, at worst the per-task demand
/// signal this design exists to avoid — the shared word that took the
/// fine-grain oracle cell from 0.4905 s to 0.9254 s when it was measured.
///
/// The query is a separate weak definition, so the four entry points of the
/// lane protocol are exactly the bytes they were; a link that reads them, and
/// the cases that pin them, cannot be disturbed by it.
#[test]
fn the_bootstrap_selects_one_world_once() {
    let overlapped = fold_module(true);

    // Asked once, in the one place that runs once and is inside no loop and no
    // recursion.
    assert_eq!(
        overlapped
            .matches("call i32 @wf__par_pool_active()")
            .count(),
        1,
        "the world must be selected exactly once per process:\n{overlapped}"
    );
    // The bootstrap lives in the entry body: `@main` keeps the host's
    // signature and hands the program to the exhaustion floor, which runs this
    // on a stack the compiler sized.
    let bootstrap = function_body(&overlapped, "@wf__main_body");
    assert!(
        bootstrap.contains("  %par.pool = call i32 @wf__par_pool_active()")
            && bootstrap.contains("call void @\"wf_main\"(ptr %status,")
            && bootstrap.contains("call void @\"wf__par_seq_main\"(ptr %status,"),
        "the bootstrap must branch between the two lowerings of the entry:\n{bootstrap}"
    );
    // These POSIX fallback definitions are an emitted-module property. The
    // shipped driver still links the complete ordinary runtime for every build.
    assert!(
        overlapped.contains("define weak i32 @wf__par_pool_active() #0 {\nentry:\n  ret i32 0\n}"),
        "the module must carry its own answer:\n{overlapped}"
    );
    for weak in [
        "define weak ptr @wf__par_acquire_lane(i64 %bytes) #0 {",
        "define weak void @wf__par_publish(ptr %frame, ptr %fn) #0 {",
        "define weak void @wf__par_join(ptr %frame) #0 {",
        "define weak void @wf__par_release(ptr %frame) #0 {",
    ] {
        assert!(
            overlapped.contains(weak),
            "adding the query must not disturb `{weak}`:\n{overlapped}"
        );
    }

    // No clone reaches the overlapped world. A clone may still call a function
    // that has no clone — that copy is shared because its lowering is the same
    // either way — so the forbidden targets are exactly the cloned ones.
    let cloned = clone_symbols(&overlapped);
    for symbol in &cloned {
        let body = function_body(&overlapped, symbol);
        for other in &cloned {
            let forbidden = format!(" @{}(", other.replace("@wf__par_seq_", "wf_"));
            assert!(
                !body.contains(&forbidden),
                "{symbol} calls the overlapped{forbidden}, so the clone world re-enters the \
                 lowering it exists to avoid:\n{body}"
            );
        }
    }
    // And nothing but the bootstrap and the budget family reaches the clone
    // world. The recursion budget is the one other entrance there is — a
    // member whose budget is spent enters its own clone — so each variant is
    // required to name exactly that clone and is then removed; what is left is
    // the overlapped world, and it may name no clone at all.
    // `recursive_controls_preserve_scalar_and_destination_results` reads the
    // spent-budget edge itself, at every setting of the control.
    let actualized = without_clones(&overlapped);
    let mut bootstrap_free = actualized.replace(function_body(&actualized, "@wf__main_body"), "");
    for symbol in budget_symbols(&actualized) {
        let variant = function_body(&actualized, &symbol);
        let clone = symbol.replace("@wf__par_budget_", "@wf__par_seq_");
        assert!(
            variant.contains(&format!("{clone}(")),
            "{symbol} must enter {clone} when its budget is spent:\n{variant}"
        );
        bootstrap_free = bootstrap_free.replace(variant, "");
    }
    assert!(
        !bootstrap_free.contains("@wf__par_seq_"),
        "only the bootstrap and the budget family may name a clone:\n{bootstrap_free}"
    );
}

/// A Windows module with compute offers carries unresolved lane-protocol
/// obligations instead of the sequential weak definitions used by the POSIX
/// optional-runtime path.  Consequently, omitting the scheduler core is a link
/// error and can never turn a requested Windows backend into the sequential
/// world. The shared `sched/core.c` protocol, `sched/entry.c` configuration and
/// platform primitives resolve them. Runtime resource exhaustion may still
/// refuse an offer and execute its ordinary-call fallback.
#[test]
fn windows_parallel_emission_requires_external_runtime_symbols() {
    let windows = TargetLayout::for_triple("x86_64-pc-windows-msvc")
        .expect("the supported Windows target must have a system row");
    let module = super::system::with_ir_layout(
        OVERLAPPING_FOLD,
        crate::OverlapLowering::On,
        windows,
        |program| {
            emit_llvm_with_layout(program, windows)
                .expect("the overlap fixture must emit for Windows")
                .into_string()
        },
    );

    for declaration in [
        "declare ptr @wf__par_acquire_lane(i64)",
        "declare void @wf__par_publish(ptr, ptr)",
        "declare void @wf__par_join(ptr)",
        "declare void @wf__par_release(ptr)",
        "declare i32 @wf__par_pool_active()",
    ] {
        assert!(
            module.contains(declaration),
            "Windows must leave `{declaration}` for the native runtime:\n{module}"
        );
    }
    for fallback in [
        "define weak ptr @wf__par_acquire_lane",
        "define weak void @wf__par_publish",
        "define weak void @wf__par_join",
        "define weak void @wf__par_release",
        "define weak i32 @wf__par_pool_active",
    ] {
        assert!(
            !module.contains(fallback),
            "Windows must not carry sequential fallback `{fallback}`:\n{module}"
        );
    }
    assert!(
        module_requires_parallel_runtime(&module),
        "the Windows declarations must remain a driver-visible link obligation"
    );
}

/// A pair the judgment denies emits exactly the sequential calls, with no
/// frame, no thunk, no offer, and no join anywhere in the module.
#[test]
fn a_denied_pair_emits_exactly_the_sequential_calls() {
    let module = emit_with_overlap(DEPENDENT_SIBLINGS);
    assert!(
        !module.contains("wf_par"),
        "a denied pair must name no part of the runtime:\n{module}"
    );
    let body = function_body(&module, "@wf_main");
    let calls: Vec<_> = body.match_indices("call i64 @wf_twice(").collect();
    assert_eq!(calls.len(), 2, "both calls stay ordinary calls:\n{body}");
}

/// A permitted pair whose first member is a borrowed binding is not handed
/// out, because promoting that binding reads the call's value at its
/// definition site — between the offer and the join, where the value does not
/// exist yet.
///
/// The judgment still permits the pair; this is the lowering refusing to
/// actualize a permission it cannot carry on one straight-line edge, and it
/// refuses by dropping the group rather than by moving the read.
#[test]
fn a_permitted_pair_whose_first_member_is_borrowed_is_not_handed_out() {
    let borrowed = br#"fn make() -> result: u64 pure {
  return 7_u64;
}

fn peek(v: &u64) -> result: u64 reads(v) {
  return deref(v);
}

fn main() -> status: std::process::ExitStatus pure {
  let first = make();
  let second = make();
  let seen = peek(v: &first);
  return std::process::exit_status(code: 0_u8);
}
"#;
    let module = emit_with_overlap(borrowed);
    assert!(
        !module.contains("wf_par"),
        "a borrowed first member must not be handed out:\n{module}"
    );

    // The same pair with only the *second* member borrowed is handed out: the
    // last member always runs on the calling thread, so its own value is read
    // after the join like every other.
    let trailing = br#"fn make() -> result: u64 pure {
  return 7_u64;
}

fn peek(v: &u64) -> result: u64 reads(v) {
  return deref(v);
}

fn main() -> status: std::process::ExitStatus pure {
  let first = make();
  let second = make();
  let seen = peek(v: &second);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert!(
        emit_with_overlap(trailing).contains("call void @wf__par_publish(ptr "),
        "a borrowed last member does not stop the group"
    );
}

/// The shipped default is a pool: a `--par` binary run with `WF_WORKERS`
/// absent grants lanes, and only an explicit opt-out refuses them.
///
/// This is the whole of the default-behavior change, and it needs its own case
/// because every other case here names a worker count. Before it, an unset
/// variable meant the sequential world, so a `--par` binary handed to anybody
/// who did not know about the variable was byte-for-byte a sequential program
/// and the entire path was off for every real run. The started-worker count
/// is the runtime's own counter, so "the pool started" is read rather than
/// assumed.
///
/// The opt-outs are pinned in the same case against the same executable, so a
/// change that turned the default on by making *every* setting start a pool
/// fails here rather than passing as a stronger version of the same news.
///
/// `abc` used to stand here for the unparsable settings and to be a *third
/// opt-out*: a value that is not a number was read as the sequential world.
/// Step (iv) made every startup setting of this runtime follow one rule on
/// every platform, and under it a value the setting cannot mean is a
/// configuration error rather than a value to repair -- it ends the run before
/// the program body, with one line on the diagnostic channel and nothing on
/// the output channel. So it is checked here as that, beside the two opt-outs
/// it is no longer one of.
#[test]
fn linked_runtime_observes_startup_opt_out_and_a_real_worker() {
    let module = fold_module(true);
    let directory = test_directory();

    // Startup accounting distinguishes the default worker world from the
    // pool-off world. The separate controlled task assertion checks entry.
    let counted = CountedProgram::link(&module, &directory);
    let (_, published) = counted.run(None);
    assert_eq!(published.status.code(), Some(0));
    let started = workers_started(&published);
    let report = String::from_utf8_lossy(&published.stderr);
    let requested = report
        .lines()
        .find_map(|line| line.strip_prefix("compute: threads="))
        .and_then(|tail| tail.split_whitespace().next())
        .and_then(|count| count.parse::<u64>().ok())
        .expect("configured width");
    assert_eq!(
        started > 0,
        requested > 1,
        "the default must start a pool exactly when its configured width enables one: {report}"
    );

    let (granted, parallel) = counted.run(Some("4"));
    assert_eq!(parallel.status.code(), Some(0));
    assert!(granted > 0, "the controlled task must enter a real worker");
    assert_eq!(parallel.stdout, published.stdout);

    let mut runs = vec![("WF_WORKERS absent".to_owned(), published.stdout)];
    for setting in ["0", "1"] {
        let (granted, output) = counted.run(Some(setting));
        assert_eq!(output.status.code(), Some(0), "WF_WORKERS={setting}");
        assert_eq!(
            granted, 0,
            "WF_WORKERS={setting} is an opt-out and must never grant a lane"
        );
        assert_eq!(
            workers_started(&output),
            0,
            "WF_WORKERS={setting} is an opt-out and must never start the pool"
        );
        runs.push((format!("WF_WORKERS={setting}"), output.stdout));
    }
    identical(&runs).expect("the default must not move one byte of the result");

    // A setting this runtime cannot mean, on both ends of the rule: text that
    // is not a number, and a number above the ceiling. Each ends the run
    // before the program body, so no byte of the program's own output can
    // appear, and the one line it writes names the setting and the ceiling.
    for setting in ["abc", "-1", "65"] {
        let refused = Command::new(&counted.executable)
            .env("WF_WORKERS", setting)
            .env("WF_SCHED_REPORT", "1")
            .output()
            .expect("run the invalid configuration");
        assert_eq!(
            refused.status.code(),
            Some(1),
            "WF_WORKERS={setting} is a configuration error and must not run"
        );
        assert!(
            refused.stdout.is_empty(),
            "a refused configuration must reach no program output"
        );
        // Incomplete startup cannot run observers that query the runtime.
        assert_eq!(
            String::from_utf8_lossy(&refused.stderr),
            "whitefoot scheduler: WF_WORKERS must be an integer from 0 through 64\n",
            "the refusal must name the setting and its ceiling"
        );
    }

    std::fs::remove_dir_all(&directory).expect("remove the test directory");
}

/// Overlap lowering is compile-time opt-in: the default compilation of a
/// program full of eligible sites names no part of the runtime.
///
/// This is what makes the feature free when it is not asked for. The judgment
/// still ran — the same source with the lowering switched on hands work out,
/// which is the second half of this test — so what the default drops is the
/// outlining and nothing else. The outlining is not free: it passes arguments
/// through a memory frame and is reached through a function pointer, so the
/// call cannot be inlined, and the batch audit measured that alone at about
/// 1.2x on the layout demo and 2.1x on `fib(38)` with no runtime linked and
/// `WF_WORKERS` unset.
#[test]
fn the_default_compilation_hands_nothing_out() {
    let default = fold_module(false);
    assert!(
        !default.contains("wf__par_"),
        "the default compilation must name no runtime symbol:\n{default}"
    );
    assert!(
        !default.contains("(ptr %frame) {"),
        "the default compilation must outline no thunk:\n{default}"
    );
    assert!(
        !module_requires_parallel_runtime(&default),
        "no link path may add the runtime to a default build"
    );

    // The same source asked for lanes: the sites were there all along, so the
    // assertions above are about the option and not about the program.
    let requested = fold_module(true);
    assert!(
        requested.contains("call void @wf__par_publish(ptr ")
            && requested.contains(", ptr @wf__par_thunk_"),
        "the fixture must hand work out when asked, or this test is vacuous"
    );
    assert!(module_requires_parallel_runtime(&requested));
}

/// The negative control for the repeat above: its comparison reports a
/// difference when one is present, so a green repeat is evidence about the
/// program rather than about the comparison.
#[test]
fn the_repeat_comparison_reports_an_injected_difference() {
    let same = vec![
        ("WF_WORKERS=1".to_owned(), b"abcdefgh".to_vec()),
        ("WF_WORKERS=2".to_owned(), b"abcdefgh".to_vec()),
    ];
    identical(&same).expect("equal runs must compare equal");

    for injected in [b"abcdefgi".to_vec(), b"abcdefg".to_vec(), Vec::new()] {
        let mut runs = same.clone();
        runs.push(("WF_WORKERS=4".to_owned(), injected));
        let report = identical(&runs).expect_err("a differing run must be reported");
        assert!(report.contains("WF_WORKERS=4"), "{report}");
    }
}

/// Every run produced the same bytes, or the first run that did not.
pub(super) fn identical(runs: &[(String, Vec<u8>)]) -> Result<(), String> {
    let Some((first_name, first)) = runs.first() else {
        return Err("no run to compare".to_owned());
    };
    for (name, bytes) in &runs[1..] {
        if bytes != first {
            return Err(format!(
                "{name} produced {bytes:?}, but {first_name} produced {first:?}"
            ));
        }
    }
    Ok(())
}

/// Select a real worker execution without changing the runtime's code. Only
/// call sites are wrapped; declarations and the actual queue/join stay intact.
pub(super) use crate::native_test_support::observe_worker_schedule;

pub(super) const WORKER_SCHEDULE: &str = include_str!("worker_schedule.c");

/// The observer linked beside a counted program: one destructor that reports
/// the runtime's own grant count on standard error at process exit.
///
/// `wf__par_grants` is the scheduler core's steal count, summed across the
/// core's threads on demand (`sched/entry.c`). It used to be a plain
/// `unsigned long` the parallel runtime incremented; the core keeps its
/// counters per thread so that no two threads ever write one word, so the
/// observer calls rather than reads.
pub(super) const GRANT_OBSERVER: &str = include_str!("../sched/grant_observer.c");

/// One linked build of a module against the scheduler core and the grant
/// observer, so a case that wants several runs of one module pays for the link
/// once.
///
/// The immutable runtime objects are shared. Each distinct emitted module and
/// observer is linked once and executed under the selected fresh-process
/// configurations; no previous output or runtime state is reused.
///
/// The observer reads `wf__par_grants`, which no Whitefoot construct can name;
/// it exists exactly so a pool that never grants a lane cannot pass for one
/// that does.
pub(super) struct CountedProgram {
    executable: std::path::PathBuf,
}

impl CountedProgram {
    /// Links `module` inside `directory`, which the caller removes when it is
    /// done with the fixture.
    pub(super) fn link(module: &str, directory: &Path) -> Self {
        Self {
            executable: link_counting_grants(
                &observe_worker_schedule(module),
                directory,
                &format!("{WORKER_SCHEDULE}\n{GRANT_OBSERVER}"),
            ),
        }
    }

    /// One run, with the grant count the observer reported at process exit.
    ///
    /// `workers` is `None` for the shipped default — the variable removed from
    /// the child's environment, which is how a `--par` binary is actually
    /// handed to somebody — and `Some(count)` for a run that names a count.
    pub(super) fn run(&self, workers: Option<&str>) -> (u64, std::process::Output) {
        counted_run(&self.executable, workers)
    }
}

/// Link the fresh observer/module with the ordinary immutable runtime objects.
pub(super) fn link_counting_grants(
    module: &str,
    directory: &Path,
    observation: &str,
) -> std::path::PathBuf {
    let built = build_linked_executable(module, Some(observation), &[], directory);
    let counted = directory.join("counted");
    std::fs::rename(built, &counted).expect("retain the counted executable");
    counted
}

/// One run of a linked module, with the grant count the observer reported.
fn counted_run(executable: &Path, workers: Option<&str>) -> (u64, std::process::Output) {
    let mut command = Command::new(executable);
    match workers {
        Some(count) => command.env("WF_WORKERS", count),
        None => command.env_remove("WF_WORKERS"),
    };
    // The observer prints the core's counters after the grant line when asked,
    // so a case that fails on the count has the threads' own record beside it.
    command.env("WF_SCHED_REPORT", "1");
    let output = command.output().expect("run the counted program");
    let report = String::from_utf8_lossy(&output.stderr).into_owned();
    let granted = report
        .lines()
        .find_map(|line| line.strip_prefix("grants="))
        .and_then(|count| count.trim().parse::<u64>().ok())
        .unwrap_or_else(|| panic!("the observer must report a grant count, got {report:?}"));
    (granted, output)
}

/// The number of pool threads the core started in one counted run, read from
/// the `compute:` line the observer prints after the grant line when the run
/// asks for the core's counters, which every counted run does.
fn workers_started(output: &std::process::Output) -> u64 {
    let report = String::from_utf8_lossy(&output.stderr).into_owned();
    report
        .lines()
        .find(|line| line.starts_with("compute: "))
        .and_then(|line| {
            line.split_whitespace()
                .find_map(|field| field.strip_prefix("workers_started="))
        })
        .and_then(|count| count.parse::<u64>().ok())
        .unwrap_or_else(|| panic!("the observer must report the core's counters, got {report:?}"))
}

/// Every sequential clone the module defines, by symbol.
pub(super) fn clone_symbols(module: &str) -> Vec<String> {
    module
        .lines()
        .filter(|line| line.starts_with("define "))
        .filter_map(|line| line.split_once(" @wf__par_seq_"))
        .filter_map(|(_, tail)| tail.split_once('('))
        .map(|(name, _)| format!("@wf__par_seq_{name}"))
        .collect()
}

/// The symbols of one module's budget-carrying variants, in definition order.
fn budget_symbols(module: &str) -> Vec<String> {
    module
        .lines()
        .filter(|line| line.starts_with("define "))
        .filter_map(|line| line.split_once(" @wf__par_budget_"))
        .filter_map(|(_, tail)| tail.split_once('('))
        .map(|(name, _)| format!("@wf__par_budget_{name}"))
        .collect()
}

/// The module with every sequential clone definition removed, leaving the
/// overlapped world and everything the two worlds share.
fn without_clones(module: &str) -> String {
    let mut kept = String::with_capacity(module.len());
    let mut rest = module;
    while let Some(offset) = rest.find("\ndefine ") {
        let (before, definition) = rest.split_at(offset + 1);
        kept.push_str(before);
        let end = definition
            .find("\n}\n")
            .map(|at| at + 3)
            .expect("a definition must close");
        let (definition, remainder) = definition.split_at(end);
        if !definition
            .lines()
            .next()
            .is_some_and(|header| header.contains(" @wf__par_seq_"))
        {
            kept.push_str(definition);
        }
        rest = remainder;
    }
    kept.push_str(rest);
    kept
}

/// The definition that carries `symbol`'s emitted body: its own definition,
/// or, for a result returned in registers, the internal destination-form
/// body its public entry calls (compiler/src/backend/abi.rs).
fn emitted_body_definition<'module>(module: &'module str, symbol: &str) -> &'module str {
    let body = format!("{symbol}.body");
    if module.contains(&format!("{body}(")) {
        function_body(module, &body)
    } else {
        function_body(module, symbol)
    }
}

/// The text of one emitted function definition, from its `define` line to its
/// closing brace.
pub(super) fn function_body<'module>(module: &'module str, symbol: &str) -> &'module str {
    let opening = format!("{symbol}(");
    let start = module
        .match_indices(&opening)
        .find_map(|(offset, _)| {
            let line = module[..offset]
                .rfind('\n')
                .map_or(0, |newline| newline + 1);
            module[line..offset].starts_with("define").then_some(line)
        })
        .unwrap_or_else(|| panic!("the module must define {symbol}:\n{module}"));
    let end = module[start..]
        .find("\n}\n")
        .map(|offset| start + offset + 2)
        .expect("a definition must close");
    &module[start..end]
}

/// External results live in programs; this case observes the interposed
/// statement's outlined call/join boundary without a native construction.
#[test]
fn an_interposed_builtin_retains_the_outlined_call_and_join() {
    let source = include_bytes!("../../../../tests/programs/parallel/window.wf");
    let module = emit_with_overlap(source);
    let fold = function_body(&module, "@wf__par_budget_fold");
    assert!(fold.contains("call void @wf__par_publish(ptr "));
    assert!(fold.contains("call void @wf__par_join(ptr "));
    assert!(fold.contains(", ptr @wf__par_thunk_"));
}

// Stored results need independent caller storage after lane retirement;
// scalar and descriptor-returning fixtures do not exercise that adapter.
// This two-leaf Pair returns in registers: the thunk stores the returned
// value into the lane frame, and the join copies it out before release.
// `owning_enum_windows_survive_lane_arguments_results_and_refusal` keeps a
// result too large for the registers on the destination adapter.
const OWNED_PAIR_RESULTS: &[u8] = br#"struct Pair {
  left: u64;
  right: u64;
}

fn make(seed: u64) -> result: Pair pure {
  let scaled = seed *wrap 3_u64;
  let adjacent = seed +wrap 100_u64;
  return Pair(left: scaled, right: adjacent);
}

fn main() -> status: std::process::ExitStatus pure {
  let first = make(seed: 7_u64);
  let second = make(seed: 11_u64);
  if first.left != 21_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if first.right != 107_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  if second.left != 33_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  if second.right != 111_u64 {
    return std::process::exit_status(code: 4_u8);
  }
  set first.left = 41_u64;
  if second.left != 33_u64 {
    return std::process::exit_status(code: 5_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;

#[test]
fn owned_pair_results_survive_ordinary_join_and_forced_refusal() {
    let module = emit_with_overlap(OWNED_PAIR_RESULTS);
    let main = function_body(&module, "@wf_main");
    assert!(main.contains("call void @wf__par_publish(ptr "));
    assert!(main.contains("call void @wf__par_join(ptr "));
    assert!(main.contains("\npar.inline."));
    let make = function_body(&module, "@wf_make");
    let (result, _) = make
        .strip_prefix("define ")
        .and_then(|header| header.split_once(" @wf_make(i64 %v0)"))
        .expect("make takes only its seed");
    assert!(module.contains(&format!("{result} = type {{ i64, i64 }}")));
    // The published thunk and the refused edge call the same by-value ABI;
    // both edges join as one value that enters the caller's own storage.
    // A thunk is named for the function that hands it out and numbered
    // among that function's own [MOD-8]: `main`'s first.
    let thunk = function_body(&module, "@wf__par_thunk_main.0");
    assert!(
        main.contains(", ptr @wf__par_thunk_main.0)"),
        "the call is handed out through this thunk: {main}"
    );
    assert!(thunk.contains(&format!("%result = call {result} @wf_make(i64 %a0)")));
    assert!(thunk.contains(&format!("store {result} %result, ptr %slot")));
    assert!(main.contains(&format!(" = call {result} @wf_make(i64 ")));
    assert!(main.contains(&format!(" = phi {result} [ ")));
    run_owned_lane_cases(OWNED_PAIR_RESULTS, &module, 0, 1, 0, 0, 1);
}

/// Both Empty and a partial window of Box owners cross argument and result
/// boundaries. Each granted frame keeps its dirty inactive storage until join;
/// refusal executes the same calls and must release the same four cells.
#[test]
fn owning_enum_windows_survive_lane_arguments_results_and_refusal() {
    let source = br#"enum Owner {
  Empty();
  Full(values: Slots<Box<u64>, 3>);
}

fn transform(owner: Owner, value: u64) -> result: Owner pure {
  match move owner {
    Empty() => {
      let values = slots_new::<Box<u64>, 3>();
      let cell = box_new::<u64>(value: value);
      place_back(window: &values, value: move cell);
      return Owner::Full(values: move values);
    }
    Full(values: carried_values) => {
      if carried_values.len != 1_u64 {
        return Owner::Full(values: move carried_values);
      }
      let cell = take_back(window: &carried_values);
      if cell.inner != value {
        return Owner::Full(values: move carried_values);
      }
      return Owner::Empty();
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let left_values = slots_new::<Box<u64>, 3>();
  let left_cell = box_new::<u64>(value: 17_u64);
  place_back(window: &left_values, value: move left_cell);
  let left = Owner::Full(values: move left_values);
  let empty_left = Owner::Empty();
  let right_values = slots_new::<Box<u64>, 3>();
  let right_cell = box_new::<u64>(value: 29_u64);
  place_back(window: &right_values, value: move right_cell);
  let right = Owner::Full(values: move right_values);
  let empty_right = Owner::Empty();
  let first = transform(owner: move left, value: 17_u64);
  let second = transform(owner: move empty_left, value: 41_u64);
  let third = transform(owner: move empty_right, value: 53_u64);
  let fourth = transform(owner: move right, value: 29_u64);
  match move first {
    Empty() => {
    }
    Full(values: unexpected_first) => {
      return std::process::exit_status(code: 1_u8);
    }
  }
  match move second {
    Empty() => {
      return std::process::exit_status(code: 2_u8);
    }
    Full(values: second_values) => {
      if second_values.len != 1_u64 {
        return std::process::exit_status(code: 3_u8);
      }
      let second_cell = take_back(window: &second_values);
      if second_cell.inner != 41_u64 {
        return std::process::exit_status(code: 4_u8);
      }
    }
  }
  match move fourth {
    Empty() => {
    }
    Full(values: unexpected_fourth) => {
      return std::process::exit_status(code: 5_u8);
    }
  }
  match move third {
    Empty() => {
      return std::process::exit_status(code: 6_u8);
    }
    Full(values: third_values) => {
      if third_values.len != 1_u64 {
        return std::process::exit_status(code: 7_u8);
      }
      let third_cell = take_back(window: &third_values);
      if third_cell.inner != 53_u64 {
        return std::process::exit_status(code: 8_u8);
      }
    }
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let module = super::owned_places::retain_calls(&emit_with_overlap(source));
    let main = function_body(&module, "@wf_main");
    // The first permission run also contains the preceding constructions, so
    // the four calls form two sibling pairs. Full->Empty and Empty->Full each
    // occupy a published position; the first join retires before the next pair.
    assert_eq!(main.matches("call void @wf__par_publish(ptr ").count(), 2);
    assert_eq!(main.matches("call void @wf__par_join(ptr ").count(), 2);
    assert!(
        function_body(&module, "@wf_transform").starts_with("define void @wf_transform(ptr "),
        "the same aggregate ABI serves ordinary calls and published thunks"
    );
    run_owned_lane_cases(source, &module, 0, 2, 4, 0, 1);
}

// `heap_box_loop_keeps_provider_order_and_updates_borrowed_owners` retired
// here with [PROV-1] and [BLK-0] through [BLK-4]: its observation was a
// per-allocation refusal schedule over a `Heap<'heap>` store provider, and
// [STOR-8] makes allocation total over one heap with no effect entry and no
// refusal arm to schedule. Its successors are [STOR-8], [OP-9]'s
// allocation-size obligation, [OP-11] `swap` for the exchange it spelled
// `set (a, b) = move b, move a;` [SET-2], and [WIN-3]'s disposition of a
// displaced affine owner. See this module's header for the full account.

/// The native core still performs every real grant, publication, join and
/// release. The observer can refuse acquisitions and selects the overlapped
/// entry even then, so WF_WORKERS=1 cannot silently test a sequential clone.
/// Only source allocator calls are observed; runtime allocations keep their
/// normal facilities. Each allocation remains owned until its ordinary cleanup.
fn run_owned_lane_cases(
    source: &[u8],
    module: &str,
    expected_status: i32,
    attempts: u32,
    allocations: u32,
    spares: u32,
    minimum_pending: u32,
) {
    let directory = test_directory();
    let reference = Command::new(build_executable(&emit(source), &directory))
        .current_dir(&directory)
        .env("WF_WORKERS", "1")
        .output()
        .expect("run the source without compute hand-outs");
    assert_eq!(
        reference.status.code(),
        Some(expected_status),
        "{reference:?}"
    );
    assert!(reference.stdout.is_empty() && reference.stderr.is_empty());

    // Preserve the module's genuine runtime declarations and link markers;
    // only call sites pass through the observer.
    let observed = format!(
        "{module}\ndeclare i32 @wf_test_parallel_world()\ndeclare ptr @wf_test_acquire_lane(i64)\ndeclare void @wf_test_release_lane(ptr)\ndeclare void @wf_test_publish_lane(ptr, ptr)\ndeclare void @wf_test_join_lane(ptr)\n"
    )
        .replace("call i32 @wf__par_pool_active(", "call i32 @wf_test_parallel_world(")
        .replace("call ptr @wf__par_acquire_lane(", "call ptr @wf_test_acquire_lane(")
        .replace("call void @wf__par_release(", "call void @wf_test_release_lane(")
        .replace("call void @wf__par_publish(", "call void @wf_test_publish_lane(")
        .replace("call void @wf__par_join(", "call void @wf_test_join_lane(")
        .replace("@malloc(", "@wf_test_source_allocate(")
        .replace("@free(", "@wf_test_source_release(");
    let executable = build_linked_executable(&observed, Some(OWNED_LANE_OBSERVER), &[], &directory);
    let mut outcomes = Vec::new();
    for (mode, workers) in [("1", "1"), ("0", "4"), ("2", "4")] {
        let output = Command::new(&executable)
            .current_dir(&directory)
            .env("WF_WORKERS", workers)
            .env("WF_TEST_REFUSE_LANE", mode)
            .env_remove("WF_SCHED_REPORT")
            .output()
            .expect("run the aggregate adapter with forced refusal or real lanes");
        outcomes.push((mode, output));
    }
    let joinless = observed.replace(
        "call void @wf_test_join_lane(",
        "call void @wf_test_join_removed(",
    );
    assert_ne!(joinless, observed);
    let joinless =
        format!("{joinless}\ndefine void @wf_test_join_removed(ptr %frame) {{\n  ret void\n}}\n");
    let broken = build_linked_executable(&joinless, Some(OWNED_LANE_OBSERVER), &[], &directory);
    let missing = Command::new(broken)
        .env("WF_WORKERS", "4")
        .env("WF_TEST_REFUSE_LANE", "2")
        .output()
        .expect("run one controlled missing-join path");
    assert_eq!(missing.status.code(), Some(86), "{missing:?}");
    assert!(missing.stdout.is_empty(), "{missing:?}");
    assert_eq!(missing.stderr, b"test observer: release before join\n");
    std::fs::remove_dir_all(&directory).expect("remove aggregate lane artifacts");
    // Execute every schedule before asserting. Deferral holds each acquired
    // frame until its actual join, exposing premature backing reuse without
    // relying on worker timing. The native runtime still publishes and joins it.
    for (mode, output) in &outcomes {
        assert_eq!(
            output.status.code(),
            Some(expected_status),
            "mode={mode}; all schedules: {outcomes:?}"
        );
        assert_eq!(output.stdout, reference.stdout);
        let report = String::from_utf8_lossy(&output.stderr);
        let count = |name: &str| {
            report
                .split_whitespace()
                .find_map(|field| field.strip_prefix(&format!("{name}=")))
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or_else(|| panic!("missing {name} in observer report: {report}"))
        };
        assert_eq!(count("attempts"), attempts, "{report}");
        let granted = count("granted");
        if *mode == "1" || attempts == 0 {
            assert_eq!(granted, 0, "the actual acquisition edge must refuse");
        } else {
            assert!(
                granted > 0,
                "a real lane must exercise the joined result: {report}"
            );
        }
        assert_eq!(count("released"), granted, "{report}");
        assert_eq!(count("allocations"), allocations, "{report}");
        assert_eq!(count("frees"), allocations, "{report}");
        assert_eq!(count("spares"), spares, "{report}");
        assert_eq!(count("pending"), 0, "{report}");
        if *mode == "2" {
            assert!(count("peak") >= minimum_pending, "{report}");
        }
    }
}

const OWNED_LANE_OBSERVER: &str = r#"#include <stdatomic.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>


extern void *wf__par_acquire_lane(unsigned long bytes);
extern void wf__par_release(void *frame);
extern void wf__par_publish(void *frame, void (*run)(void *));
extern void wf__par_join(void *frame);
static _Atomic unsigned attempts, granted, released, allocations, frees, spares;
static _Atomic(void *) held[10];
static size_t sizes[10];
// These fixtures publish and join only on their command thread. The workers
// run the fixture's leaf helpers; they do not access this deferred queue.
static void *pending_frames[16];
static void (*pending_runs[16])(void *);
static unsigned pending, peak;

int wf_test_parallel_world(void) { return 1; }

void *wf_test_acquire_lane(unsigned long bytes) {
    atomic_fetch_add(&attempts, 1);
    const char *refuse = getenv("WF_TEST_REFUSE_LANE");
    if (refuse != NULL && refuse[0] == '1') return NULL;
    void *frame = wf__par_acquire_lane(bytes);
    if (frame != NULL) {
        memset(frame, 0xa5, bytes);
        atomic_fetch_add(&granted, 1);
    }
    return frame;
}

void wf_test_release_lane(void *frame) {
    for (unsigned i = 0; i < 16; ++i) {
        if (pending_frames[i] == frame) {
            fputs("test observer: release before join\n", stderr);
            _Exit(86);
        }
    }
    wf__par_release(frame);
    atomic_fetch_add(&released, 1);
}

void wf_test_publish_lane(void *frame, void (*run)(void *)) {
    const char *mode = getenv("WF_TEST_REFUSE_LANE");
    if (mode == NULL || mode[0] != '2') {
        wf__par_publish(frame, run);
        return;
    }
    for (unsigned i = 0; i < 16; ++i) {
        if (pending_frames[i] == NULL) {
            pending_frames[i] = frame;
            pending_runs[i] = run;
            ++pending;
            if (pending > peak) peak = pending;
            return;
        }
    }
    abort();
}

void wf_test_join_lane(void *frame) {
    for (unsigned i = 0; i < 16; ++i) {
        if (pending_frames[i] == frame) {
            pending_frames[i] = NULL;
            --pending;
            wf__par_publish(frame, pending_runs[i]);
            break;
        }
    }
    wf__par_join(frame);
}

void *wf_test_source_allocate(size_t size) {
    unsigned id = atomic_fetch_add(&allocations, 1) + 1;
    if (id >= 10) abort();
    void *value = malloc(size);
    if (value == NULL) abort();
    memset(value, 0xa5, size);
    sizes[id] = size;
    atomic_store(&held[id], value);
    return value;
}

void wf_test_source_release(void *value) {
    if (value == NULL) abort();
    for (unsigned id = 1; id < 10; ++id) {
        void *expected = value;
        if (atomic_compare_exchange_strong(&held[id], &expected, NULL)) {
            if (sizes[id] == 3) atomic_fetch_add(&spares, 1);
            atomic_fetch_add(&frees, 1);
            free(value);
            return;
        }
    }
    abort();
}

__attribute__((destructor)) static void report(void) {
    fprintf(stderr, "attempts=%u granted=%u released=%u allocations=%u frees=%u spares=%u pending=%u peak=%u\n",
        atomic_load(&attempts), atomic_load(&granted), atomic_load(&released),
        atomic_load(&allocations), atomic_load(&frees), atomic_load(&spares), pending, peak);
}
"#;

/// Linked declarations enter the same ordinary sibling group as source bodies.
#[test]
fn a_linked_body_and_source_bodies_use_one_ordinary_call_protocol() {
    let source = br#"fn choose(value: u64) -> result: u64 pure {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  let first = choose(value: 17_u64);
  let linked = std::process::exit_status(code: 0_u8);
  let second = choose(value: 19_u64);
  let third = choose(value: 23_u64);
  let pair = first +wrap second;
  let total = pair +wrap third;
  if total != 59_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  return move linked;
}
"#;
    let module = emit_with_overlap(source);
    a_mixed_fixture_reports(&module, 0);
    assert!(module.contains("@wf_std.process.exit_status"));
    assert!(!module.contains("@wf__completion_file_"));
}

/// A published ordinary source body may call a linked I/O body before returning.
/// The join observer selects an execution on another thread, rather than
/// hoping the worker steals the single task before the caller reaches join.
#[test]
fn an_ordinary_worker_helper_can_call_the_linked_io_library() {
    let source = br#"fn write_byte(inputs: std::process::Inputs) -> result: u64 pure {
  let std::process::Inputs(args: args, cwd: cwd, stdout: out, stderr: err, handles: factory, stdin: input) = move inputs;
  std::fs::close_directory(factory: &factory, directory: move cwd);
  let bytes = box_array_filled::<u8>(count: 1_u64, value: 88_u8);
  let window = &bytes.inner[0_u64..1_u64];
  match std::io::write_once(factory: &factory, output: &out, source: window, start: 0_u64, end: 1_u64) {
    Ok(value: accepted) => {
      return accepted;
    }
    Err(error: problem) => {
      return 0_u64;
    }
  }
}

fn choose(value: u64) -> result: u64 pure {
  return value;
}

fn main(inputs: std::process::Inputs) -> status: std::process::ExitStatus pure {
  let first = write_byte(inputs: move inputs);
  let second = choose(value: 1_u64);
  if first != second {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let module = emit_with_overlap(source);
    let helper = function_body(&module, "@wf_write_byte");
    // KEPT AS WRITTEN for the lowering port: `write_once` now takes its source
    // as the parameter kind `&[T]` [REF-4, TYPE-8], so its emitted argument
    // list is whatever the range-reference ABI becomes. Only the `void` return
    // (the `Result` destination) is asserted here.
    assert!(helper.contains("call void @wf_std.io.write_once("));
    assert!(!helper.contains("@wf__completion_"));
    let main = function_body(&module, "@wf_main");
    let publishes = main
        .lines()
        .filter(|line| line.contains("call void @wf__par_publish(ptr "))
        .collect::<Vec<_>>();
    assert_eq!(publishes.len(), 1, "observe exactly one published task");
    let thunk = publishes[0]
        .rsplit_once(", ptr ")
        .and_then(|(_, operand)| operand.trim().strip_suffix(')'))
        .expect("the publication names its ordinary thunk");
    assert!(
        function_body(&module, thunk)
            .lines()
            .any(|line| line.contains("call ") && line.contains("@wf_write_byte(")),
        "the observed task must be write_byte, not its sibling"
    );
    assert_eq!(main.matches("call void @wf__par_join(").count(), 1);
    // Only this caller's one publication and join enter the observer. The
    // real acquisition, release, frame, thunk and native I/O body stay intact.
    let observed_main = main
        .replace(
            "call void @wf__par_publish(",
            "call void @wf_test_io_publish(",
        )
        .replace("call void @wf__par_join(", "call void @wf_test_io_join(");
    let observed = format!(
        "{}\ndeclare void @wf_test_io_publish(ptr, ptr)\ndeclare void @wf_test_io_join(ptr)\n",
        module.replacen(main, &observed_main, 1)
    );
    let directory = test_directory();
    let executable = build_linked_executable(&observed, Some(IO_WORKER_OBSERVER), &[], &directory);
    for workers in ["0", "4"] {
        let output = Command::new(&executable)
            .env("WF_WORKERS", workers)
            .env_remove("WF_SCHED_REPORT")
            .output()
            .expect("run the ordinary I/O task under the selected schedule");
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"X");
        let expected = if workers == "0" {
            "published=0 entered=0 completed=0 other_thread=0\n"
        } else {
            "published=1 entered=1 completed=1 other_thread=1\n"
        };
        assert_eq!(String::from_utf8_lossy(&output.stderr), expected);
    }
    std::fs::remove_dir_all(directory).expect("remove linked worker fixture");
}

/// The production publish only queues the thunk; it never calls it inline.
/// Acquisition starts the configured workers before returning the real frame.
/// With join withheld, the offering thread cannot consume that queued task,
/// so another worker enters it and releases this test-only barrier. A refused
/// acquisition skips both observer calls and fails the final positive ledger
/// instead of waiting. A failed worker startup fails before publication. The
/// workers=0 clone reaches neither observer at all.
const IO_WORKER_OBSERVER: &str = r#"#include <pthread.h>
#include <sched.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

extern void wf__par_publish(void *frame, void (*run)(void *));
extern void wf__par_join(void *frame);
extern unsigned wf__sched_pool_running(void);
extern uint64_t wf_prim_monotonic_us(void);
static void *published_frame;
static void (*original_run)(void *);
static pthread_t offering_thread;
static _Atomic unsigned published, entered, completed, other_thread;

static void run_observed(void *frame) {
    if (frame != published_frame || pthread_equal(pthread_self(), offering_thread)) abort();
    atomic_store(&other_thread, 1);
    if (atomic_fetch_add_explicit(&entered, 1, memory_order_release) != 0) abort();
    original_run(frame);
    atomic_fetch_add(&completed, 1);
}

void wf_test_io_publish(void *frame, void (*run)(void *)) {
    if (atomic_fetch_add(&published, 1) != 0) abort();
    if (wf__sched_pool_running() == 0) {
        fputs("worker publication fixture: pool startup produced no worker\n", stderr);
        abort();
    }
    published_frame = frame;
    original_run = run;
    offering_thread = pthread_self();
    wf__par_publish(frame, run_observed);
}

void wf_test_io_join(void *frame) {
    if (frame != published_frame) abort();
    unsigned long long started = wf_prim_monotonic_us();
    while (atomic_load_explicit(&entered, memory_order_acquire) == 0) {
        unsigned long long now = wf_prim_monotonic_us();
        if (!started || !now || now - started >= 5000000) {
            fputs("worker publication fixture: no real worker entered within five seconds\n", stderr);
            _Exit(86);
        }
        sched_yield();
    }
    wf__par_join(frame);
}

__attribute__((destructor)) static void report(void) {
    fprintf(stderr, "published=%u entered=%u completed=%u other_thread=%u\n",
        atomic_load(&published), atomic_load(&entered),
        atomic_load(&completed), atomic_load(&other_thread));
}
"#;

/// Omitting cheap offers must preserve the last join site and the ordinary
/// evaluation of every removed member, including members inside a mixed run.
#[test]
fn scalar_leaf_control_keeps_mixed_chain_results_and_join_boundary() {
    let source = br#"fn increment(x: u64) -> result: u64 pure {
  return x +wrap 1_u64;
}

fn counted(x: u64) -> result: u64 pure {
  let value = x;
  for (i in 0_u64..17_u64) {
    set value = value +wrap i;
  }
  return value;
}

fn mixed(x: u64) -> result: u64 pure {
  let a = increment(x: x);
  let b = counted(x: x);
  let c = increment(x: x);
  let d = counted(x: x);
  let e = increment(x: x);
  let first = a +wrap b;
  let second = c +wrap d;
  let partial = first +wrap second;
  return partial +wrap e;
}

fn main() -> status: std::process::ExitStatus pure {
  let result = mixed(x: 3_u64);
  if result == 290_u64 {
    return std::process::exit_status(code: 0_u8);
  }
  return std::process::exit_status(code: 1_u8);
}
"#;
    let all = super::emit_lowered(source, crate::OverlapLowering::On);
    let filtered = super::emit_lowered(
        source,
        crate::OverlapLowering::OnWithoutSmallScalarLeaves {
            maximum_operations: 1,
        },
    );
    assert_eq!(
        function_body(&all, "@wf_mixed")
            .matches("call void @wf__par_publish(")
            .count(),
        4
    );
    assert_eq!(
        function_body(&filtered, "@wf_mixed")
            .matches("call void @wf__par_publish(")
            .count(),
        2
    );
    assert_eq!(
        function_body(&filtered, "@wf_mixed")
            .matches("call void @wf__par_join(")
            .count(),
        2
    );
    for module in [&all, &filtered] {
        let output = compile_and_run(module);
        assert!(output.status.success(), "{output:?}");
    }
    let renamed = String::from_utf8(source.to_vec())
        .unwrap()
        .replace("increment", "renamed_leaf");
    let renamed = super::emit_lowered(
        renamed.as_bytes(),
        crate::OverlapLowering::OnWithoutSmallScalarLeaves {
            maximum_operations: 1,
        },
    );
    assert_eq!(
        function_body(&renamed, "@wf_mixed")
            .matches("call void @wf__par_publish(")
            .count(),
        2
    );
}

#[test]
fn scalar_leaf_control_drops_small_offers_without_clones() {
    let source = br#"fn twice(x: u64) -> result: u64 pure {
  return x +wrap x;
}

fn main() -> status: std::process::ExitStatus pure {
  let a = twice(x: 3_u64);
  let b = twice(x: 4_u64);
  let value = a +wrap b;
  if value == 14_u64 {
    return std::process::exit_status(code: 0_u8);
  }
  return std::process::exit_status(code: 1_u8);
}
"#;
    let filtered = super::emit_lowered(
        source,
        crate::OverlapLowering::OnWithoutSmallScalarLeaves {
            maximum_operations: 1,
        },
    );
    let sequential = super::emit_lowered(source, crate::OverlapLowering::Off);
    assert_eq!(
        filtered, sequential,
        "pruning every offer must recover the ordinary module"
    );
    let retained = super::emit_lowered(
        source,
        crate::OverlapLowering::OnWithoutSmallScalarLeaves {
            maximum_operations: 0,
        },
    );
    assert!(module_requires_parallel_runtime(&retained));
}

/// One pinned budget, as the matrix below writes it.
fn pinned(levels: u8) -> crate::RecursionBudget {
    crate::RecursionBudget::Pinned(std::num::NonZeroU8::new(levels).expect("a positive budget"))
}

/// Self/mutual budget families and rejecting a task must compute every leaf.
/// Sequential clones remove descendant attempts without changing result storage.
/// A deferred granted root also checks that its callback spends a level of the
/// budget; carrying the caller's own count across the hand-out instead would
/// exceed the independently counted tree/spine opportunities.
#[test]
fn recursive_controls_preserve_scalar_and_destination_results() {
    for mutual in [false, true] {
        for aggregate in [false, true] {
            let result = if aggregate { "Answer" } else { "u64" };
            let declaration = if aggregate {
                "struct Answer {\n  value: u64;\n}\n\n"
            } else {
                ""
            };
            let leaf = if aggregate {
                "Answer(value: value)"
            } else {
                "value"
            };
            let sum = if aggregate {
                "l.value +wrap r.value"
            } else {
                "l +wrap r"
            };
            let merged = if aggregate {
                "Answer(value: combined)"
            } else {
                "combined"
            };
            let read = if aggregate { "answer.value" } else { "answer" };
            let source = format!(
                r#"{declaration}fn fold(depth: u64, seed: &u64) -> result: {result} reads(seed) contract {{
  requires depth <= 5_u64;
}} {{
  if depth == 0_u64 {{
    let value = deref(seed);
    return {leaf};
  }}
  let below = depth - 1_u64;
  let l = fold(depth: below, seed: seed);
  let r = fold(depth: below, seed: seed);
  let combined = {sum};
  return {merged};
}}

fn main() -> status: std::process::ExitStatus pure {{
  let seed = 2_u64;
  let answer = fold(depth: 5_u64, seed: &seed);
  if {read} == 64_u64 {{
    return std::process::exit_status(code: 0_u8);
  }}
  return std::process::exit_status(code: 1_u8);
}}
"#
            );
            let source = if mutual {
                let (function, command) = source.split_once("fn main").unwrap();
                let alternate = function[function.find("fn fold(").unwrap()..].replacen(
                    "fn fold(",
                    "fn alternate(",
                    1,
                );
                format!(
                    "{}{alternate}fn main{command}",
                    function.replace("= fold(", "= alternate(")
                )
            } else {
                source
            };
            // The `None` cells are plain `--par`: the policies a build with
            // no recursion control selects, which take the default budget and
            // so get the family. The rest are the control's three forms.
            let mut implicit_modules: [Option<String>; 2] = [None, None];
            for (sequential, budget) in [
                (false, None),
                (true, None),
                (false, Some(crate::RecursionBudget::Off)),
                (false, Some(pinned(1))),
                (false, Some(pinned(3))),
                (false, Some(pinned(6))),
                (true, Some(pinned(3))),
                (false, Some(crate::RecursionBudget::RuntimeDerived)),
                (true, Some(crate::RecursionBudget::RuntimeDerived)),
            ] {
                let policy = if let Some(budget) = budget {
                    crate::OverlapLowering::OnWithRecursionBudget {
                        budget,
                        maximum_scalar_leaf_operations: Some(16),
                        sequential_refusal: sequential,
                    }
                } else if sequential {
                    crate::OverlapLowering::OnWithSequentialRefusal {
                        maximum_scalar_leaf_operations: Some(16),
                    }
                } else {
                    crate::OverlapLowering::OnWithoutSmallScalarLeaves {
                        maximum_operations: 16,
                    }
                };
                let family = budget != Some(crate::RecursionBudget::Off);
                let module = super::emit_lowered(source.as_bytes(), policy);
                if budget.is_none() {
                    implicit_modules[usize::from(sequential)] = Some(module.clone());
                } else if budget == Some(crate::RecursionBudget::RuntimeDerived) {
                    assert_eq!(
                        implicit_modules[usize::from(sequential)].as_ref().unwrap(),
                        &module
                    );
                    continue; // Equality retains the emission check; its image already ran.
                }
                // `Answer` returns in registers, so each of these definitions
                // is a public entry over an internal destination-form body
                // that holds the calls and the cut (compiler/src/backend/abi.rs).
                let entry = emitted_body_definition(&module, "@wf_fold");
                let target = if mutual { "alternate" } else { "fold" };
                assert_eq!(module.contains("@wf__par_budget_fold("), family);
                if family {
                    // The entry keeps the writer's own signature and result
                    // ABI: what it adds is the budget it enters the family
                    // with, asked of the runtime unless it was pinned.
                    let variant =
                        emitted_body_definition(&module, &format!("@wf__par_budget_{target}"));
                    assert!(entry.contains("@wf__par_budget_fold("), "{entry}");
                    assert!(!entry.contains("@wf__par_acquire_lane("), "{entry}");
                    assert_eq!(
                        entry.contains("@wf__par_recursion_budget()"),
                        !matches!(budget, Some(crate::RecursionBudget::Pinned(_)))
                    );
                    // The cut: a variant handed nothing left enters its own
                    // clone. The refusal control selects the callee's clone at
                    // a refused acquisition, and composes with it unchanged.
                    assert!(variant.contains("par.grain.spent:"), "{variant}");
                    assert!(
                        variant.contains(&format!("@wf__par_seq_{target}(")),
                        "{variant}"
                    );
                    assert_eq!(
                        emitted_body_definition(&module, "@wf__par_budget_fold")
                            .matches(&format!("@wf__par_seq_{target}("))
                            .count(),
                        usize::from(!mutual) + usize::from(sequential)
                    );
                } else {
                    assert_eq!(
                        entry.contains(&format!("@wf__par_seq_{target}(")),
                        sequential
                    );
                }

                assert!(
                    !emitted_body_definition(&module, "@wf__par_seq_fold")
                        .contains("@wf__par_acquire_lane(")
                );
                let mut observed = module
                    .replace(
                        "call ptr @wf__par_acquire_lane(",
                        "call ptr @wf_test_acquire(",
                    )
                    .replace("call void @wf__par_publish(", "call void @wf_test_publish(")
                    .replace("call void @wf__par_join(", "call void @wf_test_join(")
                    .replace("call void @wf__par_release(", "call void @wf_test_release(")
                    .replace(
                        "call i32 @wf__par_pool_active()",
                        "call i32 @wf_test_parallel_world()",
                    )
                    .replace(
                        "call i64 @wf__par_recursion_budget()",
                        "call i64 @wf_test_recursion_budget()",
                    );
                observed.push_str("\ndeclare ptr @wf_test_acquire(i64)\ndeclare void @wf_test_publish(ptr, ptr)\ndeclare void @wf_test_join(ptr)\ndeclare void @wf_test_release(ptr)\ndeclare i32 @wf_test_parallel_world()\ndeclare i64 @wf_test_recursion_budget()\n");
                let directory = test_directory();
                let executable = build_linked_executable(
                    &observed,
                    Some(SEQUENTIAL_REFUSAL_OBSERVER),
                    &[],
                    &directory,
                );
                // The budget the runtime answers, for the forms that ask.
                // Zero is the answer a scheduler-less link gets from the
                // module's own weak stub: the first node runs the clone.
                let derived_values: &[u32] = if budget.is_none() { &[5, 0] } else { &[0] };
                for &derived in derived_values {
                    for granted in [false, true] {
                        let output = Command::new(&executable)
                            .env("WF_TEST_ONE_GRANT", if granted { "1" } else { "0" })
                            .env("WF_TEST_BUDGET", derived.to_string())
                            .output()
                            .expect("run deterministic refusal schedule");
                        assert!(output.status.success(), "{output:?}");
                        // Full binary tree: 31 internal calls. With no grants
                        // only the right spine tries: 5. A granted root also
                        // runs the left subtree's right spine: 5 + 4. A
                        // budget of N cuts the tree at N levels, and nothing
                        // under the cut acquires anything at all.
                        let levels = match budget {
                            Some(crate::RecursionBudget::Pinned(levels)) => u32::from(levels.get()),
                            // No family: every node of the component offers.
                            Some(crate::RecursionBudget::Off) => 5,
                            Some(crate::RecursionBudget::RuntimeDerived) | None => derived,
                        }
                        .min(5);
                        let granted = granted && levels > 0;
                        let attempts = if levels == 0 {
                            0
                        } else if sequential {
                            if granted { levels + levels - 1 } else { levels }
                        } else {
                            (1 << levels) - 1
                        };
                        assert_eq!(
                            String::from_utf8_lossy(&output.stderr),
                            format!(
                                "attempts={attempts} granted={} released={}\n",
                                u8::from(granted),
                                u8::from(granted)
                            )
                        );
                    }
                }
                std::fs::remove_dir_all(directory).expect("remove refusal artifacts");
            }
        }
    }
}

#[test]
fn recursive_controls_keep_leaf_calls_unchanged() {
    // No descendant compute permission means no clone is needed at the call.
    let source = br#"fn leaf(x: u64) -> result: u64 pure {
  return x +wrap 1_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let a = leaf(x: 1_u64);
  let b = leaf(x: 2_u64);
  let sum = a +wrap b;
  if sum == 5_u64 {
    return std::process::exit_status(code: 0_u8);
  }
  return std::process::exit_status(code: 1_u8);
}
"#;
    let reference = emit_with_overlap(source);
    for policy in [
        crate::OverlapLowering::OnWithSequentialRefusal {
            maximum_scalar_leaf_operations: None,
        },
        crate::OverlapLowering::OnWithRecursionBudget {
            budget: crate::RecursionBudget::Pinned(std::num::NonZeroU8::new(3).unwrap()),
            maximum_scalar_leaf_operations: None,
            sequential_refusal: false,
        },
        crate::OverlapLowering::OnWithRecursionBudget {
            budget: crate::RecursionBudget::Off,
            maximum_scalar_leaf_operations: None,
            sequential_refusal: false,
        },
    ] {
        let module = super::emit_lowered(source, policy);
        assert_eq!(module, reference);
    }
    assert!(compile_and_run(&reference).status.success());
}

// C2 removes the suspension-based cycle exclusion. The ordinary linked-call
// protocol is exercised by the worker-helper regression above; recursion
// controls are derived from the same call graph for every function.

const SEQUENTIAL_REFUSAL_OBSERVER: &str = r#"#include <stdio.h>
#include <stdlib.h>
#include <stdalign.h>
static unsigned attempts, grants, releases;
static alignas(16) unsigned char frame[512];
static void (*callback)(void *);
static int joined;
static void require(int value) { if (!value) abort(); }
static void report(void) {
    require(grants == releases);
    fprintf(stderr, "attempts=%u granted=%u released=%u\n", attempts, grants, releases);
}
int wf_test_parallel_world(void) { require(atexit(report) == 0); return 1; }
/* The strong answer to the module's own weak budget stub, so one schedule can
 * be measured at a chosen cut without a scheduler in the link. */
unsigned long long wf_test_recursion_budget(void) {
    const char *budget = getenv("WF_TEST_BUDGET");
    return budget ? strtoull(budget, NULL, 10) : 0;
}
void *wf_test_acquire(unsigned long bytes) {
    require(bytes <= sizeof frame);
    ++attempts;
    const char *grant = getenv("WF_TEST_ONE_GRANT");
    if (attempts == 1 && grant && grant[0] == '1') { ++grants; return frame; }
    return NULL;
}
void wf_test_publish(void *p, void (*run)(void *)) {
    require(p == frame && !callback); callback = run;
}
void wf_test_join(void *p) {
    require(p == frame && callback && !joined); callback(p); joined = 1;
}
void wf_test_release(void *p) {
    require(p == frame && joined && !releases); ++releases;
}
"#;

/// Source permissions, clone/offer shape and actual nonowner execution for
/// both layout folds share one compilation and one observed native image.
/// Public output behavior is exercised separately by the programs collection.
#[test]
fn layout_folds_preserve_permissions_and_each_execute_a_worker() {
    let source = include_bytes!("../../../../tests/programs/par_layout.wf");
    let plain = emit(source);
    assert!(!module_requires_parallel_runtime(&plain));
    let (llvm, ledger) = whitefoot_compile_layout(source);
    let ledger = ledger.join("\n");
    assert!(
        module_requires_parallel_runtime(&llvm),
        "a module with an eligible site must ask for the runtime"
    );

    for name in ["layout", "layout_banded"] {
        let entry = function_body(&llvm, &format!("@wf_{name}"));
        assert!(
            entry.contains("= call i64 @wf__par_recursion_budget()")
                && entry.contains(&format!("call double @wf__par_budget_{name}(")),
            "wf_{name} must obtain a budget and enter its family:\n{entry}"
        );
        let symbol = format!("@wf__par_budget_{name}");
        let symbol = symbol.as_str();
        let fold = function_body(&llvm, symbol);
        assert!(
            fold.contains(&format!("@wf__par_seq_{name}(")),
            "{symbol} must enter its sequential clone with its budget spent:\n{fold}"
        );
        assert!(
            fold.contains("= call ptr @wf__par_acquire_lane(i64 "),
            "{symbol} must acquire a lane for its first child call:\n{fold}"
        );
        assert!(
            fold.contains(", ptr @wf__par_thunk_"),
            "{symbol} must publish the outlined call to the acquired lane:\n{fold}"
        );
        assert!(
            fold.contains("call void @wf__par_join(ptr"),
            "{symbol} must join what it offered:\n{fold}"
        );
    }

    let measure = function_body(&llvm, "@wf_measure_band");
    assert!(
        !measure.contains("call void @wf_trap("),
        "proved source bounds must not lower to a runtime proof-failure call:\n{measure}"
    );
    assert!(
        !measure.contains("wf__par_"),
        "a callee in no permitted pair must name no part of the runtime:\n{measure}"
    );
    assert!(
        ledger.contains("pair(layout, layout)  eligible"),
        "the table-bounded fold's child pair must be reported eligible:\n{ledger}"
    );
    assert!(
        ledger.contains("pair(layout_banded, layout_banded)  eligible"),
        "the caller-bounded fold's child pair must be reported eligible too:\n{ledger}"
    );
    assert!(
        !ledger.contains("not-actualizable"),
        "no verdict may be withheld after all source bounds are proved:\n{ledger}"
    );

    let (observed, host) = crate::native_test_support::observe_layout(&llvm);
    let directory = test_directory();
    let executable = build_linked_executable(&observed, Some(&host), &[], &directory);
    let output = Command::new(executable)
        .env("WF_WORKERS", "4")
        .output()
        .expect("run observed layout");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(output.stdout, b"420a993efa7437a1 41fa962893d45299\n");
    assert!(output.stderr.is_empty(), "{output:?}");
    std::fs::remove_dir_all(directory).expect("remove observed layout");
}

fn whitefoot_compile_layout(source: &[u8]) -> (String, Vec<String>) {
    crate::compile_with_permission_ledger(
        &[crate::SourceInput::new("par_layout.wf", source)],
        crate::CompilerLimits::default(),
        crate::OverlapLowering::On,
    )
    .expect("layout source must compile")
}
