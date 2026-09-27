//! Source allocation reuse and observed output batching in wfgrep.
//!
//! C2 retires QUAL-1..3's per-operation compiler-wrapper cost table. The
//! prelude bodies are linked implementations with one ordinary callable ABI.
//! These two independent program properties remain executable: each source
//! buffer allocates and initializes once, and a reusable output batch issues
//! one write per full batch. CASES.md records every retired assertion.

use super::deterministic_target::{HostScript, run_emitted_on_deterministic_host};
use super::{host_optimized_module, optimized_main};
use std::sync::OnceLock;

const WFGREP: &[u8] = include_bytes!("../../../../tests/programs/wfgrep.wf");

fn emitted() -> &'static str {
    static MODULE: OnceLock<String> = OnceLock::new();
    MODULE.get_or_init(|| super::compile(WFGREP))
}

fn program_functions() -> &'static [&'static str] {
    static FUNCTIONS: OnceLock<Vec<&'static str>> = OnceLock::new();
    FUNCTIONS.get_or_init(|| {
        let module = optimized();
        let mut functions = vec![entry()];
        for line in std::str::from_utf8(WFGREP)
            .expect("WF source is UTF-8")
            .lines()
        {
            let Some(signature) = line.strip_prefix("fn ") else {
                continue;
            };
            let name = signature.split(['(', '[', '<']).next().expect("fn name");
            if name == "main" {
                continue;
            }
            let needle = format!(" @wf_{name}(");
            if module
                .match_indices(&needle)
                .any(|(at, _)| definition_start(module, at).is_some())
            {
                functions.push(source_function(module, &format!("wf_{name}")));
            }
        }
        functions
    })
}

fn optimized() -> &'static str {
    static MODULE: OnceLock<String> = OnceLock::new();
    MODULE.get_or_init(|| host_optimized_module(emitted()))
}

fn entry() -> &'static str {
    optimized_main(optimized())
}

fn definition_start(module: &str, at: usize) -> Option<usize> {
    let line = module[..at].rfind('\n').map_or(0, |newline| newline + 1);
    module[line..at].starts_with("define").then_some(line)
}

fn call_argument<'line>(line: &'line str, callee: &str, wanted: usize) -> Option<&'line str> {
    let open = line.find(&format!("@{callee}("))? + callee.len() + 2;
    comma_item(&line[open..], wanted)
}

fn comma_item(rest: &str, wanted: usize) -> Option<&str> {
    let mut depth = 0_usize;
    let mut ordinal = 0_usize;
    let mut start = 0_usize;
    for (offset, character) in rest.char_indices() {
        match character {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' if depth == 0 => {
                return (ordinal == wanted).then(|| rest[start..offset].trim());
            }
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                if ordinal == wanted {
                    return Some(rest[start..offset].trim());
                }
                ordinal += 1;
                start = offset + 1;
            }
            _ => {}
        }
    }
    (ordinal == wanted).then(|| rest[start..].trim())
}

fn getelementptr_base(definition: &str) -> Option<&str> {
    let operands = definition.strip_prefix("getelementptr ")?;
    comma_item(operands, 1)?.split_whitespace().next_back()
}

fn is_aggregate_destination<'module>(function: &'module str, mut pointer: &'module str) -> bool {
    let mut seen = Vec::new();
    loop {
        let first_parameter = function.lines().next().and_then(|header| {
            let (callee, _) = header.split_once('@')?.1.split_once('(')?;
            call_argument(header, callee, 0)?
                .split_whitespace()
                .next_back()
        });
        if pointer == "%wf.result" && first_parameter == Some(pointer) {
            return true;
        }
        if seen.contains(&pointer) {
            return false;
        }
        seen.push(pointer);
        let prefix = format!("  {pointer} = ");
        let Some(definition) = function.lines().find_map(|line| line.strip_prefix(&prefix)) else {
            return false;
        };
        if definition.starts_with("alloca ") {
            return true;
        }
        if !definition.starts_with("getelementptr ") {
            return false;
        }
        let Some(base) = getelementptr_base(definition) else {
            return false;
        };
        pointer = base;
    }
}

/// Recognize a single initialization of a freshly allocated payload, including
/// a malloc whose null check puts the fill in a successor block. Following a
/// unique predecessor chain back to the allocation excludes a refill loop:
/// every execution of this fill must first execute that allocation again.
/// Unknown provenance or a control-flow join fails this narrow test oracle.
fn fresh_allocation_for_fill<'module>(
    function: &'module str,
    mut pointer: &'module str,
    fill: &str,
) -> Option<&'module str> {
    let lines: Vec<_> = function.lines().collect();
    let mut seen = Vec::new();
    let allocation = loop {
        if seen.contains(&pointer) {
            return None;
        }
        seen.push(pointer);
        let prefix = format!("  {pointer} = ");
        let (index, definition) = lines.iter().enumerate().find_map(|(index, line)| {
            line.strip_prefix(&prefix)
                .map(|definition| (index, definition))
        })?;
        if call_target(lines[index]) == Some("malloc") {
            break index;
        }
        pointer = getelementptr_base(definition)?;
    };
    let fill = lines.iter().position(|line| *line == fill)?;
    let mut names = vec!["<entry>"];
    let mut line_blocks = Vec::with_capacity(lines.len());
    for line in &lines {
        if !line.starts_with(char::is_whitespace)
            && let Some((label, _)) = line.split_once(':')
        {
            names.push(label);
        }
        line_blocks.push(names.len() - 1);
    }
    let mut predecessors = vec![std::collections::BTreeSet::new(); names.len()];
    for (line, block) in lines.iter().zip(&line_blocks) {
        for successor in line.split("label %").skip(1) {
            let name = successor.split([',', ' ', ']', '\r']).next()?;
            let successor = names.iter().position(|candidate| *candidate == name)?;
            predecessors[successor].insert(*block);
        }
    }
    let allocation_block = line_blocks[allocation];
    let mut block = line_blocks[fill];
    let mut visited = std::collections::BTreeSet::new();
    while block != allocation_block {
        if !visited.insert(block) || predecessors[block].len() != 1 {
            return None;
        }
        block = *predecessors[block].first()?;
    }
    (line_blocks[fill] != allocation_block || allocation < fill).then_some(pointer)
}

fn bulk_initializations_use_fresh_storage(function: &str) -> Result<(), String> {
    let mut initialized_allocations = std::collections::BTreeSet::new();
    for line in function.lines() {
        let Some(callee) = call_target(line).filter(|name| name.contains("memset")) else {
            continue;
        };
        let pointer = call_argument(line, callee, 0)
            .and_then(|argument| argument.split_whitespace().next_back())
            .expect("a memset has a destination");
        if is_aggregate_destination(function, pointer) {
            continue;
        }
        if let Some(allocation) = fresh_allocation_for_fill(function, pointer, line)
            && initialized_allocations.insert(allocation)
        {
            continue;
        }
        return Err(format!(
            "bulk initialization must not refill heap run backing: {line}\n{}",
            aggregate_destination_trace(function, pointer)
        ));
    }
    Ok(())
}

#[test]
fn heap_initialization_oracle_refuses_repeated_and_reused_fills() {
    let fresh = r#"define void @fresh() {
entry:
  %run = tail call dereferenceable_or_null(4112) ptr @malloc(i64 4112)
  %is_null = icmp eq ptr %run, null
  br i1 %is_null, label %failed, label %initialize
failed:
  ret void
initialize:
  %payload = getelementptr i8, ptr %run, i64 16
  call void @llvm.memset.p0.i64(ptr %payload, i8 0, i64 4096, i1 false)
  ret void
}"#;
    assert!(bulk_initializations_use_fresh_storage(fresh).is_ok());

    let twice = fresh.replace(
        "  ret void\n}",
        "  call void @llvm.memset.p0.i64(ptr %payload, i8 0, i64 4096, i1 false)\n  ret void\n}",
    );
    assert!(bulk_initializations_use_fresh_storage(&twice).is_err());
    let looped = fresh.replace("  ret void\n}", "  br label %initialize\n}");
    assert!(bulk_initializations_use_fresh_storage(&looped).is_err());
    let reused = r#"define void @reuse(ptr %run) {
  %payload = getelementptr i8, ptr %run, i64 16
  call void @llvm.memset.p0.i64(ptr %payload, i8 0, i64 4096, i1 false)
  ret void
}"#;
    assert!(bulk_initializations_use_fresh_storage(reused).is_err());
    let calloc = fresh.replace("@malloc(i64 4112)", "@calloc(i64 1, i64 4112)");
    assert!(bulk_initializations_use_fresh_storage(&calloc).is_err());
}

/// A compact pointer-provenance trace for optimizer-version failures in the
/// aggregate-initialization oracle. It stops at the first instruction the
/// classifier does not follow, so CI reports the missing spelling without
/// dumping a whole optimized module.
fn aggregate_destination_trace<'module>(
    function: &'module str,
    mut pointer: &'module str,
) -> String {
    let header = function
        .lines()
        .next()
        .unwrap_or("<missing function header>");
    let mut trace = vec![
        format!("function: {header}"),
        format!("destination: {pointer}"),
    ];
    let mut seen = Vec::new();
    loop {
        if seen.contains(&pointer) {
            trace.push(format!("cycle at {pointer}"));
            break;
        }
        seen.push(pointer);
        let prefix = format!("  {pointer} = ");
        let Some(definition) = function.lines().find_map(|line| line.strip_prefix(&prefix)) else {
            trace.push(format!("{pointer}: no local definition"));
            break;
        };
        trace.push(format!("{pointer} = {definition}"));
        if !definition.starts_with("getelementptr ") {
            break;
        }
        let Some(base) = getelementptr_base(definition) else {
            trace.push("getelementptr: no top-level base operand".to_owned());
            break;
        };
        pointer = base;
    }
    trace.join("\n")
}

#[test]
fn aggregate_destination_provenance_ignores_commas_inside_gep_types() {
    let stack = r#"define void @stack() {
  %wf.frame = alloca { i64, ptr, ptr }, align 8
  %wf.slot = getelementptr inbounds { i64, ptr, ptr }, ptr %wf.frame, i64 0, i32 1
  %wf.inner = getelementptr inbounds { i64, ptr, ptr }, ptr %wf.slot, i64 0, i32 2
  ret void
}"#;
    assert!(is_aggregate_destination(stack, "%wf.slot"));
    assert!(is_aggregate_destination(stack, "%wf.inner"));

    let heap = r#"define void @heap() {
  %wf.cell = call ptr @malloc(i64 24)
  %wf.slot = getelementptr inbounds { i64, ptr, ptr }, ptr %wf.cell, i64 0, i32 1
  %wf.inner = getelementptr inbounds { i64, ptr, ptr }, ptr %wf.slot, i64 0, i32 2
  ret void
}"#;
    assert!(!is_aggregate_destination(heap, "%wf.slot"));
    assert!(!is_aggregate_destination(heap, "%wf.inner"));
    assert!(aggregate_destination_trace(stack, "%wf.inner").contains("%wf.frame = alloca "));
}

fn source_function<'module>(module: &'module str, symbol: &str) -> &'module str {
    let needle = format!(" @{symbol}(");
    let start = module
        .match_indices(&needle)
        .find_map(|(at, _)| definition_start(module, at))
        .unwrap_or_else(|| panic!("the module must define {symbol}"));
    let end = module[start..]
        .find("\n}\n")
        .map(|offset| start + offset + 2)
        .unwrap_or_else(|| panic!("the definition of {symbol} must close"));
    &module[start..end]
}

fn call_target(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if !(trimmed.starts_with("call ")
        || trimmed.starts_with("tail call ")
        || trimmed.contains(" = call ")
        || trimmed.contains(" = tail call "))
    {
        return None;
    }
    let at = trimmed.find('@')? + 1;
    let rest = &trimmed[at..];
    let end = rest.find(|character: char| {
        !(character.is_ascii_alphanumeric() || character == '_' || character == '.')
    })?;
    rest[end..].starts_with('(').then(|| &rest[..end])
}

/// Follows a pointer through its local `getelementptr` definitions to the
/// value it was derived from.
fn pointer_root<'module>(function: &'module str, mut pointer: &'module str) -> &'module str {
    let mut seen = Vec::new();
    while !seen.contains(&pointer) {
        seen.push(pointer);
        let prefix = format!("  {pointer} = ");
        let Some(base) = function
            .lines()
            .find_map(|line| line.strip_prefix(&prefix))
            .and_then(getelementptr_base)
        else {
            break;
        };
        pointer = base;
    }
    pointer
}

/// Recognize an allocation that remakes an existing run instead of taking a
/// new one. `grow` [OP-10] allocates the larger block, copies the old window
/// into it, and frees the old block, so the allocation is the destination of a
/// bulk copy whose source block the same function frees. A take may also be a
/// copy destination, as `walk`'s child path is for the prefix it copies in,
/// but its source is never a block the function releases.
fn remakes_a_run(function: &str, allocation: &str) -> bool {
    let Some(register) = allocation.trim_start().split(" = ").next() else {
        return false;
    };
    let freed: Vec<_> = function
        .lines()
        .filter(|line| call_target(line) == Some("free"))
        .filter_map(|line| call_argument(line, "free", 0))
        .filter_map(|argument| argument.split_whitespace().next_back())
        .collect();
    function.lines().any(|line| {
        let Some(callee) = call_target(line)
            .filter(|name| name.starts_with("llvm.memmove") || name.starts_with("llvm.memcpy"))
        else {
            return false;
        };
        let operand = |ordinal| {
            call_argument(line, callee, ordinal)
                .and_then(|argument| argument.split_whitespace().next_back())
                .map(|pointer| pointer_root(function, pointer))
        };
        operand(0) == Some(register) && operand(1).is_some_and(|source| freed.contains(&source))
    })
}

#[test]
fn the_reused_buffers_are_initialized_once_at_allocation() {
    // `wfgrep` asks for exactly eleven runs, and gets exactly eleven store
    // takes. The numbers here are payload capacities; each runtime `Slots`
    // allocation also contains its 16-byte `len`/`cap` descriptor. Derived
    // from source, function by function: `main` takes the pattern (4096), the
    // root name (4096), the root path (4096), and the diagnostic report
    // (4352); `walk` takes its enumeration batch (8192), its record store
    // (4096 bytes, empty), its offset store (512 u64 slots, 4096 bytes,
    // empty), one child path and its own report, both sized from the display
    // prefix it was handed and so the two takes without a constant extent;
    // `search_file` takes the read input (4096) and the publication batch
    // (8192).
    //
    // This is where the recursive search moved the cost: `walk`'s five and
    // `search_file`'s two are per call, so a walk of D directories holding F
    // files allocates 5D + 2F + 4 times, where the argv-list version allocated
    // four times for the whole run. That is a measured property of the new
    // shape, recorded here rather than hidden — it is one of the numbers the
    // flagship re-attribution has to explain. Growth is extra and is not a
    // take: `widen_window` remakes the read input for a line longer than it,
    // and `push_byte` and `push_word` remake a store for a directory whose
    // names pass 4096 bytes or whose entries pass 512. Each remake allocates
    // the larger block, copies the old run into it and frees the old block.
    //
    // The source still owns initialization. With exclusive run rows LLVM can
    // fold malloc plus the zero-fill loop into calloc. Count either optimized
    // spelling, including both calloc factors, while retaining the exact
    // source-site and per-size counts, and one allocation per retained helper.
    let mut expanded = 0;
    let mut out_of_line = 0;
    let mut prefix_sized = 0;
    let mut remakes = 0;
    let mut helper_takes = 0;
    let mut helper_defined = false;
    let mut helper_retained = false;
    let mut sizes = std::collections::BTreeMap::new();
    for function in program_functions() {
        let signature = function.lines().next().unwrap_or_default();
        let helper = signature.contains(" @wf_zeroed_bytes(");
        helper_defined |= helper;
        for line in function.lines() {
            if let Some(callee @ ("malloc" | "calloc")) = call_target(line) {
                if helper {
                    helper_takes += 1;
                    continue;
                }
                let factor = |ordinal| {
                    call_argument(line, callee, ordinal)
                        .and_then(|argument| argument.split_whitespace().next_back())
                        .and_then(|value| value.parse::<u64>().ok())
                };
                let extent = if callee == "calloc" {
                    factor(0).zip(factor(1)).map(|(count, size)| count * size)
                } else {
                    factor(0)
                };
                match extent {
                    Some(bytes) => {
                        expanded += 1;
                        *sizes.entry(bytes).or_insert(0) += 1;
                    }
                    None if remakes_a_run(function, line) => remakes += 1,
                    None => {
                        expanded += 1;
                        prefix_sized += 1;
                    }
                }
            }
            if call_target(line) == Some("wf_zeroed_bytes") {
                out_of_line += 1;
                helper_retained = true;
                // Capacity is the helper's final source argument. Optimizers
                // can remove its unused provider argument but do not change
                // that value. The observed allocation also contains the
                // runtime Slots descriptor before its payload.
                let count = (0..)
                    .map_while(|ordinal| call_argument(line, "wf_zeroed_bytes", ordinal))
                    .last()
                    .and_then(|argument| argument.split_whitespace().next_back())
                    .and_then(|value| value.parse::<u64>().ok());
                match count {
                    Some(count) => *sizes.entry(16 + count).or_insert(0) += 1,
                    None => prefix_sized += 1,
                }
            }
        }
    }
    assert_eq!(
        helper_takes,
        usize::from(helper_defined),
        "the retained initialization helper has exactly one allocation"
    );
    // Ordinary public definitions survive even when all of their calls have
    // been inlined. Validate the definition and remaining calls independently.
    assert!(!helper_retained || helper_defined);
    assert_eq!(
        expanded + out_of_line,
        11,
        "eleven source runs, eleven store takes"
    );
    assert_eq!(
        prefix_sized, 2,
        "only walk's child path and its report take a run sized from the prefix"
    );
    assert_eq!(
        sizes,
        std::collections::BTreeMap::from([(4112, 6), (8208, 2), (4368, 1)]),
        "all nine constant source takes retain their exact descriptor-plus-payload byte extents"
    );
    assert!(
        remakes >= 3,
        "widen_window, push_byte and push_word each remake a run by copying it: {remakes}"
    );
    // Nothing reallocates in place, and nothing re-initializes. That the fill
    // runs once per take is a source fact under this surface rather than an
    // allocator guarantee: the fill loop is inside `zeroed_bytes`, between the
    // take and the hand-back, so a caller cannot reach a filled run without
    // having taken it and cannot re-reach the fill without taking another. A
    // remake fills only the slots it added, and only once, after its copy.
    // LLVM may retain that first fill as a memset after malloc and
    // its null check, instead of a calloc. The provenance/control-flow oracle
    // permits one such fill per allocation and refuses repeated fills, fills
    // reached again without allocation, and fills through incoming pointers.
    // Aggregate or frame initialization may also become a memset, without
    // refilling a run's heap payload. The no-refill oracle must not depend on
    // which aggregate stores this optimizer combines.
    // Keep the no-refill claim on pointer provenance, rather than forbidding
    // the unrelated aggregate initialization instruction by name.
    for forbidden in ["@realloc(", "@reallocf(", "bzero"] {
        assert!(
            !optimized().contains(forbidden),
            "the reused buffers must not reach {forbidden}"
        );
    }
    for definition in optimized().split("\ndefine ").skip(1) {
        let function = definition
            .split("\n}")
            .next()
            .expect("a definition has a body");
        bulk_initializations_use_fresh_storage(function).unwrap();
    }
    // Allocation begins in each function's prologue: the first allocation a
    // function makes precedes every host transfer it reaches, so no buffer is
    // first created in the drain, the match loop, or the flush.
    //
    // `search_file` remains a retained helper, reached after `main` opens the
    // fallback file or `walk` opens a child. Its two allocations are per file,
    // not per read. The per-buffer count above carries "one allocation per
    // source buffer"; this ordering check additionally requires allocation to
    // begin before that function's first emitted transfer. A remake of the
    // read input necessarily follows a read, since only a read shows a line
    // longer than the window, so the check constrains the first allocation.
    for function in program_functions() {
        let Some(first_allocation) = ["@malloc(", "@calloc(", "@wf_zeroed_bytes("]
            .iter()
            .filter_map(|site| function.find(site))
            .min()
        else {
            continue;
        };
        for transfer in [
            "@wf_std.fs.open_read(",
            "@wf_std.fs.read_at(",
            "@wf_std.io.write_once(",
            "@wf_std.fs.directory_next(",
        ] {
            let Some(first) = function.find(transfer) else {
                continue;
            };
            assert!(
                first_allocation < first,
                "allocation must begin before the first {transfer}"
            );
        }
    }
}

#[test]
fn the_output_batch_costs_one_host_write_per_full_batch() {
    // Three thousand matching lines of two bytes each, published through an
    // eight-thousand-one-hundred-and-ninety-two-byte batch. The scripted host
    // holds one regular file, so the search's root `open_directory` reports
    // `ENOTDIR` and the run reaches the file through the path route — which is
    // exactly the shape this row needs: one file, one read cursor, one batch.
    const MATCHES: u64 = 3_000;
    let mut fixture = Vec::new();
    for _ in 0..MATCHES {
        fixture.extend_from_slice(b"x\n");
    }

    let run = run_emitted_on_deterministic_host(
        emitted(),
        &HostScript::new().file(&fixture),
        &[b"x", b"lines.txt"],
    );
    assert_eq!(
        run.output.status.code(),
        Some(0),
        "every line matches; trace was {:?}",
        run.trace()
    );

    // The host echoes each accepted payload into its own trace, and this
    // program's payload is lines, so a trace entry is not reliably a trace
    // *line*. Counting occurrences of each marker instead is exact here
    // because no marker can occur inside a payload of `x` and newline.
    let trace = run.trace();
    let published: Vec<u64> = trace
        .match_indices("wf_test write fd=1 count=")
        .map(|(at, marker)| {
            trace[at + marker.len()..]
                .split_whitespace()
                .next()
                .expect("a write trace carries its requested count")
                .parse()
                .expect("a requested count is a number")
        })
        .collect();

    // A handful of host writes for three thousand matches: the batch fills and
    // is published whole, then the remainder is published on the way out. A
    // syscall-per-match implementation would show three thousand here, which
    // is exactly what §12.2 rejects.
    //
    // The exact split is no longer a fixed vector, because a published record
    // now carries the file's path and the line's ordinal, so record lengths
    // differ by the ordinal's digit count and a fixed vector would be a
    // transcription of this fixture rather than a property. What the row
    // asserts instead is the property itself: every batch but the last is
    // published within one record of full, the sum is exactly the output the
    // matches produced with no byte published twice, and the call count is two
    // orders of magnitude below the match count.
    const BATCH: u64 = 8_192;
    let expected: u64 = (1..=MATCHES)
        .map(|ordinal| {
            "lines.txt:".len() as u64 + ordinal.to_string().len() as u64 + ":x\n".len() as u64
        })
        .sum();
    assert!(published.len() >= 2, "trace was {trace:?}");
    assert!(
        (published.len() as u64) * 100 < MATCHES,
        "one write per full batch, not per match: {published:?}"
    );
    // The program flushes when the next record would not fit under its own
    // conservative reservation — the path length plus twenty-four bytes for
    // the ordinal and the two separators, plus the line's own span. Here that
    // is nine plus twenty-four plus two, so a full batch lands within
    // thirty-five bytes of the cap and never above it.
    const RESERVE: u64 = 40;
    for count in &published[..published.len() - 1] {
        assert!(
            *count > BATCH - RESERVE && *count <= BATCH,
            "a flushed batch must be within one record of full: {published:?}"
        );
    }
    // The batch is not refilled or re-initialized after a flush: each
    // publication starts from an empty batch and carries only what followed the
    // previous one, so the requested counts sum to exactly the output the
    // matches produced and no byte is published twice.
    assert_eq!(
        published.iter().sum::<u64>(),
        expected,
        "trace was {trace:?}"
    );

    // Both linear handles close explicitly. Affine output owners have empty drop.
    assert_eq!(
        trace.matches("wf_test close ").count(),
        2,
        "trace was {trace:?}"
    );
    assert_eq!(
        trace
            .lines()
            .filter(|line| line.starts_with("wf_test close fd=")
                && !line.starts_with("wf_test close fd=42 ")
                && line.ends_with("outcome=ok"))
            .count(),
        1
    );
    assert_eq!(trace.matches("wf_test close fd=42 outcome=ok").count(), 1);
    assert!(!trace.contains("wf_test close fd=1 "));
    assert!(!trace.contains("wf_test close fd=2 "));
    // Three reads for a two-chunk file: two that delivered bytes and one that
    // observed the end. One host attempt per source `read_at`, never a retry
    // and never a second attempt to confirm the end. Nothing was ever
    // published to standard error: the successful path reports nothing.
    // And the whole run is the §12.2 per-byte-call rejection, measured rather
    // than argued: six thousand bytes in and roughly fifty thousand out cost a
    // dozen host calls altogether. Not one host call per byte, per line, per
    // match, or per field.
    assert!(
        trace.matches("wf_test ").count() < 20,
        "the whole run is a dozen host calls: {trace:?}"
    );
    assert_eq!(
        trace.matches("wf_test pread fd=42 ").count(),
        3,
        "trace was {trace:?}"
    );
    // The refused directory open of the one regular file, then the path-route
    // open that reached it.
    assert_eq!(
        trace
            .lines()
            .filter(|line| line.starts_with("wf_test openat root=") && line.ends_with("-> notdir"))
            .count(),
        1,
        "trace was {trace:?}"
    );
    assert_eq!(
        trace
            .lines()
            .filter(|line| line.starts_with("wf_test openat root=") && line.ends_with("fd=42"))
            .count(),
        1,
        "trace was {trace:?}"
    );
    assert!(!trace.contains("wf_test write fd=2 "));
}
