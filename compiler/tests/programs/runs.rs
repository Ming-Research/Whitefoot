use super::support::{compile_and_run, compile_program, emitted_function};

/// [WIN-1, OP-10, MSR-1] the ring at execution: a construction row, the four
/// boundary operations over the back and the front, and the window subscript
/// at a wrapped window.
///
/// The exit code is the program's own report: it observes the queue's order,
/// the element a wrapped subscript reads, and the length the run is left with,
/// so a lowering that computed any physical slot wrongly reports a nonzero
/// code rather than passing quietly.
#[test]
fn a_run_is_a_queue_whose_window_wraps() {
    let llvm = compile_program("run_queue.wf");
    // The window subscript is the one conditional subtract [WIN-1]'s
    // coordinate system fixes, and the boundary store goes through the ring's
    // own frame slot. Re-derived for v0.60: a `Ring`'s block is header-first,
    // `{ i64 len, i64 head, [4 x i8] slots }`, so one address computation
    // serves the inline and the boxed placement alike [STOR-1].
    assert!(llvm.contains("select i1"));
    // Independent frame roots no longer need an enclosing frame-struct GEP;
    // the window's own typed address still carries the complete header.
    assert!(llvm.contains("getelementptr inbounds { i64, i64, [4 x i8] }"));

    let output = compile_and_run(&llvm);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

/// [REF-4, MSR-1, OWN-7] the range-reference half of the container design,
/// executing: a range reference formed over a window, that reference read
/// twice, an append to the window after its last use, a range reference of the
/// same window drained to empty, and a child range that reads what the parent
/// wrote.
///
/// Retired subject: the view kinds [VIEW-1, VIEW-2] and their permission
/// markers. v0.60 has no `Slice<T>`, no `&uniq` and no reborrow: [REF-4]'s
/// `&[T]` is a reference kind admitted only in parameter position, whose one
/// measure is `len`, equal to `hi - lo`.
///
/// The program reports through its own exit code, so a range that pointed at
/// the wrong slot is visible rather than silent: it reads the window's length,
/// sums the window twice through the range reference, sums it again after the
/// append, and reads the byte an element write through a reference left.
#[test]
fn a_run_is_viewable_and_a_copy_view_dies_at_its_last_use() {
    let llvm = compile_program("run_views.wf");
    // A range reference over an inline window is the address of the first
    // element of the range and the element count [REF-4, MSR-1], taken in the
    // window's own frame slot, whose block is header-first [STOR-1].
    assert!(llvm.contains("getelementptr inbounds { i64, [4 x i8] }"));

    let output = compile_and_run(&llvm);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

/// [WIN-1, OP-10, MSR-1] the bump-allocator shape at execution: one
/// constant-capacity window resident in the entry's own frame, two runs carved
/// off its back with `place_back`, and the carve its own `room` measure
/// refuses.
///
/// Retired subject: the arena and its `advance<T>(count)` take [BLK-2,
/// STOR-4]. v0.60 has no arenas and no reservation of its own; a
/// constant-capacity `Slots` is frame-resident with its slots inline
/// [STOR-1], and the bump is `place_back` with a range reference as the run it
/// hands out. The program still reads the window's own measures before and
/// after each carve, and still observes that the refused carve leaves `len`
/// exactly where it was.
#[test]
fn a_bump_extent_hands_out_runs_and_refuses_the_one_it_cannot_hold() {
    let llvm = compile_program("arena_workspace.wf");
    let main = emitted_function(&llvm, "main");
    // The window is frame-resident in the exact header-first u64 layout. The
    // range descriptor reads its length and selects its first inline slot from
    // one backing pointer, then indexes that slot with typed u64 arithmetic;
    // no allocation call is emitted [STOR-1, REF-4].
    // Both qualified frame recipes keep this complete, aligned window inline:
    // an independent root or a field of the padded frame allocation.
    assert!(
        main.contains("alloca { i64, [256 x i64] }, align 8")
            || main.contains("alloca { { i64, [256 x i64] } }, align 8")
    );
    let slots_gep = main
        .lines()
        .find(|line| {
            line.contains("getelementptr inbounds { i64, [256 x i64] }, ptr %")
                && line.ends_with("i64 0, i32 1, i64 0")
        })
        .expect("range formation must address the window's first inline u64 slot");
    fn gep_base(line: &str) -> &str {
        line.split_once("ptr ")
            .and_then(|(_, suffix)| suffix.split_once(',').map(|(base, _)| base))
            .expect("window GEP must carry one base pointer")
    }
    let slots_base = gep_base(slots_gep);
    let length_gep = main
        .lines()
        .find(|line| {
            line.contains("getelementptr inbounds { i64, [256 x i64] }, ptr %")
                && line.ends_with("i32 0, i32 0")
                && gep_base(line) == slots_base
        })
        .expect("range formation must read the length from the same inline window");
    assert_eq!(gep_base(length_gep), slots_base);
    assert!(main.contains("getelementptr inbounds i64, ptr"));
    assert!(!llvm.contains("call ptr @malloc"));

    let output = compile_and_run(&llvm);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

/// [WIN-1, MSR-3, MSR-5, OP-10] the constant-capacity half of the container
/// design's own library, executing: the two constructions, the transposing
/// removal, the two checked boundary forms, and the drain that returns a
/// wrapped window to its origin.
///
/// Every one of the six is written in the design's spelling and proves its own
/// contract, so the test is evidence for the surface as much as for the
/// lowering: `take_at` needs [OP-11]'s `swap` at an element position and the
/// arithmetic requirement `at + 2_u64 <= vector.len`; `try_place` and
/// `try_take` need
/// a parameter's measure in an `ensures` to denote its entry datum; `vacant`
/// needs a run at an unbounded element type; and `rebase` needs the rebind that
/// carries `spare`'s measures onto `built`. The program checks what it built —
/// the order the drain preserves, the element the transposition moved, and the
/// head the drain leaves at zero — so a wrong physical slot reports a nonzero
/// code rather than passing quietly.
#[test]
fn the_fixed_run_library_proves_and_runs() {
    let llvm = compile_program("fixed_run_library.wf");
    // The element-position `swap` goes through the window, so the store's
    // offset is the same conditional subtract a read uses [WIN-1].
    assert!(llvm.contains("select i1"));

    let output = compile_and_run(&llvm);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

/// [STOR-8, STOR-3, PROV-6, OP-13] the one heap at execution: the construction
/// is a real allocation and the compiler-derived release is a real free.
///
/// Retired subject: the store surface [BLK-2] and its provider parameter. The
/// heap is ambient and has no source spelling [STOR-8], so the program is the
/// whole path in one source with no `Heap` parameter and no `writes(store)`
/// row: `box_slots_new` takes a window of four slots, a counted loop fills it
/// under the invariants the writer states, and a helper adds the bytes and
/// lets the cell reach its scope exit.
///
/// The exit code is the sum, so a slot addressed wrongly reports a code rather
/// than passing quietly; the two assertions below are the allocator pair,
/// which is the half an exit code cannot see.
#[test]
fn the_general_store_hands_out_a_run_and_takes_it_back() {
    let llvm = compile_program("heap_run.wf");
    // One construction, one free, and the free is the cell's own release
    // emitted at the scope exit that owns it [PROV-6, STOR-3].
    assert_eq!(llvm.matches("call ptr @malloc").count(), 1);
    assert!(llvm.contains("call void @free"));

    let output = compile_and_run(&llvm);
    assert_eq!(output.status.code(), Some(12));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

/// [WIN-1, PROV-6, TYPE-5] the one-level lift at execution, under the two
/// nominals the design writes it with.
///
/// `BlockPool` holds the free list and `nodrop struct Lease` holds the leased
/// run, so this is the design's pool entire: eight frame-resident windows
/// carved into one window of windows, a lease taken off the back boundary and
/// returned to a free list `pool_release` *proved* had room. The `linear`
/// modifier is what makes the return unavoidable — the one path that does not
/// return the lease has to take it apart, and dropping it is refused.
///
/// The exit code is the program's own report: it reads the free list's length
/// after the carve, the room the take leaves, and the length the release
/// leaves, so an element slot laid out or addressed wrongly reports a nonzero
/// code rather than passing quietly.
#[test]
fn a_run_of_store_backed_runs_is_a_block_pool() {
    let llvm = compile_program("block_pool.wf");
    // One slot holds a whole constant-capacity block, so the outer window's
    // storage is eight of them and the element load is that aggregate
    // [WIN-1, OP-9]. The alias name alone says nothing about what a slot
    // holds, so the element type's own definition is pinned beside it: the
    // slot is a `Block`, whose one field is a `Slots<u8, 256>`, and [OP-9]
    // prices that shape as "T's pair N times and then ... that block followed
    // by one `(8,8)` word, its length", one runtime word and 256 element
    // bytes. Header first, by the pending amendment
    // compiler/storage-representation: "The constant-capacity form carries the
    // same header ahead of its elements so that the two placements share one
    // layout."
    let block = super::support::nominal_type("Block");
    assert!(llvm.contains(&format!("[8 x {block}]")));
    let element = llvm
        .lines()
        .find(|line| line.starts_with(&format!("{block} = type ")))
        .expect("the aliased element type must be declared");
    let (header, _) = element
        .split_once("[256 x i8]")
        .unwrap_or_else(|| panic!("a slot holds 256 element bytes: {element}"));
    assert!(
        header.ends_with("{ i64, "),
        "and its one length word stands ahead of them: {element}"
    );
    // Re-derived for v0.60: the blocks themselves stay frame-resident, and the
    // one heap object the program owns is the boxed run the entry hands to the
    // lease, so exactly one allocation is emitted [STOR-1, STOR-8].
    assert_eq!(llvm.matches("call ptr @malloc").count(), 1);

    let output = compile_and_run(&llvm);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

/// One successful 4,096-byte result enters a fixed run; every byte survives
/// the transfer, while the rejected producer leaves the result run empty.
/// The distinct compiler owned-child/retained-call case is not replaced here.
#[test]
fn a_wide_result_preserves_every_byte_and_leaves_refusal_empty() {
    let output = compile_and_run(&compile_program("containers/large_result.wf"));
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}
