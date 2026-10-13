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
            // A waiting function's body is its coroutine's `.resume` half
            // once the host has split it; its own symbol keeps only the
            // ramp that makes the frame. The entry waits too, so its body is
            // `wf_main.resume` rather than inside the launcher.
            for symbol in [format!("wf_{name}"), format!("wf_{name}.resume")] {
                if name == "main" && symbol == "wf_main" {
                    continue;
                }
                let needle = format!(" @{symbol}(");
                if module
                    .match_indices(&needle)
                    .any(|(at, _)| definition_start(module, at).is_some())
                {
                    functions.push(source_function(module, &symbol));
                }
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

/// Whether any index of a `getelementptr` is a register rather than a
/// constant, so that the offset it computes is a run-time value.
fn getelementptr_has_runtime_index(definition: &str) -> bool {
    let Some(operands) = definition.strip_prefix("getelementptr ") else {
        return false;
    };
    (2..)
        .map_while(|ordinal| comma_item(operands, ordinal))
        .any(|index| {
            index
                .split_whitespace()
                .next_back()
                .is_some_and(|value| value.starts_with('%'))
        })
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
        // A waiting function's resumed half runs in its frame, the coroutine
        // form of its allocas, and reloads its destination parameter from
        // that frame under the name the coroutine split gives the spill.
        if pointer == "%wf.coro.handle" && first_parameter == Some(pointer) {
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
        if let Some(address) = definition
            .strip_prefix("load ptr, ptr ")
            .and_then(|rest| rest.split(',').next())
            .map(str::trim)
            .filter(|address| address.starts_with("%wf.result.reload.addr"))
        {
            let address_prefix = format!("  {address} = ");
            return function
                .lines()
                .find_map(|line| line.strip_prefix(&address_prefix))
                .and_then(getelementptr_base)
                .is_some_and(|base| base == "%wf.coro.handle" && first_parameter == Some(base));
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
/// a counted take whose null check puts the fill in a successor block. Following a
/// unique predecessor chain back to the allocation excludes a refill loop:
/// every execution of this fill must first execute that allocation again.
/// A `wf__heap_retake` root additionally requires the fill's destination to add a
/// run-time index to the block, because the slots it preserves are a prefix of
/// run-time length. This structural test does not prove that an arbitrary
/// runtime index is past that prefix; the remaining provenance limitation is
/// recorded in status board item lm-bl-grow-oracle. Unknown provenance or a
/// control-flow join fails this narrow test oracle.
fn fresh_allocation_for_fill<'module>(
    function: &'module str,
    mut pointer: &'module str,
    fill: &str,
) -> Option<&'module str> {
    let lines: Vec<_> = function.lines().collect();
    let mut seen = Vec::new();
    let mut runtime_offset = false;
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
        match call_target(lines[index]) {
            Some("wf__heap_take") => break index,
            // A `grow` [OP-10] reallocates the cell and keeps the header and the
            // filled slots, so its result is fresh only past that preserved
            // prefix. The prefix length is the block's run-time length, which no
            // constant offset can be shown to reach, so only a destination that
            // adds a run-time index to the block qualifies.
            Some("wf__heap_retake") if runtime_offset => break index,
            Some("wf__heap_retake") => return None,
            _ => {}
        }
        runtime_offset |= getelementptr_has_runtime_index(definition);
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
  %run = tail call dereferenceable_or_null(4112) ptr @wf__heap_take(i64 4112)
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
    let calloc = fresh.replace("@wf__heap_take(i64 4112)", "@calloc(i64 1, i64 4112)");
    assert!(bulk_initializations_use_fresh_storage(&calloc).is_err());
    // A `grow` remake keeps the old prefix and fills only the slots it added,
    // at a run-time offset past that prefix, once, after its realloc.
    let remade = r#"define void @remade(ptr %old, i64 %length, i64 %added_bytes) {
entry:
  %run = call ptr @wf__heap_retake(ptr %old, i64 48, i64 4112)
  %is_null = icmp eq ptr %run, null
  br i1 %is_null, label %failed, label %initialize
failed:
  ret void
initialize:
  %payload = getelementptr i8, ptr %run, i64 16
  %added = getelementptr i8, ptr %payload, i64 %length
  call void @llvm.memset.p0.i64(ptr %added, i8 0, i64 %added_bytes, i1 false)
  ret void
}"#;
    assert!(bulk_initializations_use_fresh_storage(remade).is_ok());
    let remade_twice = remade.replace(
        "  ret void\n}",
        "  call void @llvm.memset.p0.i64(ptr %added, i8 0, i64 %added_bytes, i1 false)\n  ret void\n}",
    );
    assert!(bulk_initializations_use_fresh_storage(&remade_twice).is_err());
    let refilled_prefix = remade.replace("ptr %added, i8 0", "ptr %payload, i8 0");
    assert!(bulk_initializations_use_fresh_storage(&refilled_prefix).is_err());
    let refilled_block = remade.replace("ptr %added, i8 0", "ptr %run, i8 0");
    assert!(bulk_initializations_use_fresh_storage(&refilled_block).is_err());
    let constant_offset = remade.replace("i64 %length\n", "i64 8\n");
    assert_ne!(constant_offset, remade);
    assert!(bulk_initializations_use_fresh_storage(&constant_offset).is_err());
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
  %wf.cell = call ptr @wf__heap_take(i64 24)
  %wf.slot = getelementptr inbounds { i64, ptr, ptr }, ptr %wf.cell, i64 0, i32 1
  %wf.inner = getelementptr inbounds { i64, ptr, ptr }, ptr %wf.slot, i64 0, i32 2
  ret void
}"#;
    assert!(!is_aggregate_destination(heap, "%wf.slot"));
    assert!(!is_aggregate_destination(heap, "%wf.inner"));
    // A resumed waiting function reaches its destination through the frame
    // spill the coroutine split makes; a pointer loaded from any other frame
    // field is not the destination.
    let resumed = r#"define internal fastcc void @f.resume(ptr %wf.coro.handle) {
  %wf.result.reload.addr = getelementptr inbounds %f.Frame, ptr %wf.coro.handle, i32 0, i32 2
  %wf.result.reload = load ptr, ptr %wf.result.reload.addr, align 8
  %other.addr = getelementptr inbounds %f.Frame, ptr %wf.coro.handle, i32 0, i32 3
  %other = load ptr, ptr %other.addr, align 8
  ret void
}"#;
    assert!(is_aggregate_destination(resumed, "%wf.result.reload"));
    assert!(!is_aggregate_destination(resumed, "%other"));
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

/// Keep the LLVM token, including quotes, so argument and definition lookup
/// use exactly the same spelling. Generic instance symbols contain `$`.
fn function_token(line: &str) -> Option<&str> {
    let rest = &line[line.find('@')? + 1..];
    let end = if let Some(quoted) = rest.strip_prefix('"') {
        // LLVM escapes inside quoted names are hex bytes, not literal quotes.
        quoted.find('"')? + 2
    } else {
        rest.find(|character: char| {
            !(character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | '$' | '-'))
        })?
    };
    (end > 0 && rest[end..].starts_with('(')).then(|| &rest[..end])
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
    function_token(trimmed)
}

fn value_definition<'a>(function: &'a str, value: &str) -> Option<&'a str> {
    function.lines().find_map(|line| {
        let (result, definition) = line.trim().split_once(" = ")?;
        (result == value).then_some(definition)
    })
}

/// Evaluate only the allocation's integer extent, not arbitrary LLVM. Unknown
/// arithmetic fails closed rather than acquiring the expected source extent.
fn allocation_integer(
    function: &str,
    value: &str,
    formal: &str,
    actual: u64,
    visiting: &mut std::collections::BTreeSet<String>,
) -> Option<u64> {
    if value == formal {
        return Some(actual);
    }
    if let Ok(number) = value.parse() {
        return Some(number);
    }
    if !visiting.insert(value.to_owned()) {
        return None;
    }
    let definition = value_definition(function, value)?;
    let (operation, left, right) =
        if let Some(rest) = definition.strip_prefix("extractvalue { i64, i1 } ") {
            let pair = comma_item(rest, 0)?;
            if comma_item(rest, 1)? != "0" {
                return None;
            }
            let producer = value_definition(function, pair)?;
            if call_target(producer)? != "llvm.uadd.with.overflow.i64" {
                return None;
            }
            (
                "add",
                call_argument(producer, "llvm.uadd.with.overflow.i64", 0)?,
                call_argument(producer, "llvm.uadd.with.overflow.i64", 1)?,
            )
        } else {
            let (operation, rest) = definition.split_once(' ')?;
            if !matches!(operation, "add" | "mul" | "shl") {
                return None;
            }
            let (flags, rest) = rest.split_once("i64 ")?;
            if !flags
                .split_whitespace()
                .all(|flag| matches!(flag, "nuw" | "nsw"))
            {
                return None;
            }
            (operation, comma_item(rest, 0)?, comma_item(rest, 1)?)
        };
    let left = allocation_integer(
        function,
        left.split_whitespace().next_back()?,
        formal,
        actual,
        visiting,
    )?;
    let right = allocation_integer(
        function,
        right.split_whitespace().next_back()?,
        formal,
        actual,
        visiting,
    )?;
    let answer = match operation {
        "add" => left.checked_add(right),
        "mul" => left.checked_mul(right),
        "shl" => left.checked_mul(1_u64.checked_shl(u32::try_from(right).ok()?)?),
        _ => None,
    };
    visiting.remove(value);
    answer
}

/// A retained empty-window constructor is credited per call, never once per
/// definition. Validate its body independently of its symbol or source type:
/// one take, the actual extent, two header writes, and the fresh return.
/// This deliberately narrow oracle rejects unfamiliar bodies for inspection.
fn retained_constructor_bytes(function: &str, call: &str) -> Result<u64, String> {
    let fail = || format!("unrecognized retained allocation at {call}\n{function}");
    let callee = call_target(call).ok_or_else(fail)?;
    let header = function.lines().next().ok_or_else(fail)?;
    if function_token(header) != Some(callee) || !header.starts_with("define ptr ") {
        return Err(fail());
    }
    let parameter = call_argument(header, callee, 0).ok_or_else(fail)?;
    if !parameter.starts_with("i64 ") || call_argument(header, callee, 1).is_some() {
        return Err(fail());
    }
    let formal = parameter.split_whitespace().next_back().ok_or_else(fail)?;
    let actual = call_argument(call, callee, 0)
        .and_then(|argument| {
            argument
                .strip_prefix("i64 ")?
                .split_whitespace()
                .next_back()?
                .parse::<u64>()
                .ok()
        })
        .filter(|actual| *actual > 0)
        .ok_or_else(fail)?;
    if call_argument(call, callee, 1).is_some() {
        return Err(fail());
    }
    let takes: Vec<_> = function
        .lines()
        .filter(|line| call_target(line) == Some("wf__heap_take"))
        .collect();
    let [take] = takes.as_slice() else {
        return Err(fail());
    };
    let (allocation, _) = take.trim().split_once(" = ").ok_or_else(fail)?;
    let integer = |value| {
        allocation_integer(
            function,
            value,
            formal,
            actual,
            &mut std::collections::BTreeSet::new(),
        )
    };
    let bytes = call_argument(take, "wf__heap_take", 0)
        .and_then(|argument| argument.split_whitespace().next_back())
        .and_then(integer)
        .filter(|bytes| (16..=i64::MAX as u64).contains(bytes))
        .ok_or_else(fail)?;
    bulk_initializations_use_fresh_storage(function)?;

    // Acyclic blocks and intersection of predecessor facts establish that
    // every successful path allocates and writes both header words once.
    // Only the exact zero-capacity edge is pruned using the actual argument;
    // overflow and allocation-failure branches may end in unreachable.
    let mut names = vec!["<entry>"];
    let mut blocks = vec![Vec::new()];
    for line in function.lines().skip(1) {
        let text = line.trim();
        if text.is_empty() || text.starts_with(';') || text == "}" {
            continue;
        }
        if !line.starts_with(char::is_whitespace)
            && let Some((label, _)) = text.split_once(':')
        {
            if blocks.last().is_some_and(Vec::is_empty) {
                *names.last_mut().ok_or_else(fail)? = label;
            } else {
                names.push(label);
                blocks.push(Vec::new());
            }
        } else {
            blocks.last_mut().ok_or_else(fail)?.push(text);
        }
    }
    let mut successors = vec![Vec::new(); blocks.len()];
    let mut predecessors = vec![Vec::new(); blocks.len()];
    for (index, block) in blocks.iter().enumerate() {
        let end = *block.last().ok_or_else(fail)?;
        let mut labels: Vec<_> = end
            .split("label %")
            .skip(1)
            .map(|part| part.split([',', ' ']).next().ok_or_else(fail))
            .collect::<Result<_, _>>()?;
        if end.starts_with("br i1 ")
            && let Some(predicate) = end
                .strip_prefix("br i1 ")
                .and_then(|rest| rest.split(',').next())
            && let Some(condition) = value_definition(function, predicate)
            && condition == format!("icmp eq i64 {formal}, 0")
        {
            if labels.len() != 2 {
                return Err(fail());
            }
            labels.remove(0);
        }
        if !end.starts_with("br ") && !end.starts_with("ret ptr ") && end != "unreachable" {
            return Err(fail());
        }
        for label in labels {
            let target = names
                .iter()
                .position(|name| *name == label)
                .ok_or_else(fail)?;
            successors[index].push(target);
            predecessors[target].push(index);
        }
    }
    let mut pending: std::collections::VecDeque<_> = predecessors
        .iter()
        .enumerate()
        .filter_map(|(index, incoming)| incoming.is_empty().then_some(index))
        .collect();
    let mut remaining: Vec<_> = predecessors.iter().map(Vec::len).collect();
    let mut facts = vec![None; blocks.len()];
    facts[0] = Some(0_u8);
    let mut visited = 0;
    let mut returns = 0;
    let mut stores = [0, 0];
    while let Some(index) = pending.pop_front() {
        visited += 1;
        if let Some(mut state) = facts[index] {
            for line in &blocks[index] {
                if call_target(line) == Some("wf__heap_take") {
                    state |= 1;
                } else if let Some(callee) = call_target(line)
                    && !matches!(callee, "llvm.uadd.with.overflow.i64" | "wf_resource_abort")
                {
                    return Err(fail());
                }
                if let Some(store) = line.strip_prefix("store i64 ") {
                    let value = comma_item(store, 0).and_then(integer).ok_or_else(fail)?;
                    let address = comma_item(store, 1)
                        .and_then(|operand| operand.strip_prefix("ptr "))
                        .ok_or_else(fail)?;
                    let word = if address == allocation && value == 0 {
                        0
                    } else if value == actual
                        && let Some(projection) = value_definition(function, address)
                        && projection.starts_with("getelementptr ")
                        && getelementptr_base(projection) == Some(allocation)
                        && comma_item(projection, 0)
                            .and_then(|part| part.split_whitespace().next_back())
                            == Some("i8")
                        && comma_item(
                            projection.strip_prefix("getelementptr ").ok_or_else(fail)?,
                            2,
                        ) == Some("i64 8")
                    {
                        1
                    } else {
                        return Err(fail());
                    };
                    if state & 1 == 0 {
                        return Err(fail());
                    }
                    stores[word] += 1;
                    state |= 2 << word;
                } else if line.starts_with("store ") {
                    return Err(fail());
                }
                if let Some(returned) = line.strip_prefix("ret ptr ") {
                    if state != 7 {
                        return Err(fail());
                    }
                    if returned != allocation {
                        let phi = value_definition(function, returned)
                            .and_then(|definition| definition.strip_prefix("phi ptr "))
                            .ok_or_else(fail)?;
                        for &from in &predecessors[index] {
                            if facts[from].is_none() {
                                continue;
                            }
                            let incoming =
                                (0..).map_while(|at| comma_item(phi, at)).find_map(|part| {
                                    let pair = part.trim().strip_prefix('[')?.strip_suffix(']')?;
                                    (comma_item(pair, 1)?.trim().strip_prefix('%')? == names[from])
                                        .then(|| comma_item(pair, 0))
                                        .flatten()
                                });
                            if incoming != Some(allocation) {
                                return Err(fail());
                            }
                        }
                    }
                    returns += 1;
                }
            }
            facts[index] = Some(state);
            for &target in &successors[index] {
                facts[target] = Some(facts[target].map_or(state, |previous| previous & state));
            }
        }
        for &target in &successors[index] {
            remaining[target] -= 1;
            if remaining[target] == 0 {
                pending.push_back(target);
            }
        }
    }
    if visited != blocks.len() || returns == 0 || stores != [1, 1] {
        return Err(fail());
    }
    Ok(bytes)
}

#[test]
fn retained_allocation_oracle_follows_quoted_calls_and_measured_extent() {
    let body = r#"define ptr @"window$words"(i64 %capacity) {
entry:
  %empty = icmp eq i64 %capacity, 0
  br i1 %empty, label %ready, label %size
size:
  %payload = shl i64 %capacity, 3
  %sum = call { i64, i1 } @llvm.uadd.with.overflow.i64(i64 %payload, i64 16)
  %bytes = extractvalue { i64, i1 } %sum, 0
  %overflow = extractvalue { i64, i1 } %sum, 1
  br i1 %overflow, label %oom, label %allocate
allocate:
  %fresh = call ptr @wf__heap_take(i64 %bytes)
  %null = icmp eq ptr %fresh, null
  br i1 %null, label %oom, label %initialize
initialize:
  store i64 0, ptr %fresh, align 8
  %cap = getelementptr inbounds i8, ptr %fresh, i64 8
  store i64 %capacity, ptr %cap, align 8
  br label %ready
oom:
  call void @wf_resource_abort()
  unreachable
ready:
  %result = phi ptr [ %fresh, %initialize ], [ @wf__empty_window, %entry ]
  ret ptr %result
}
"#;
    let call = "  %run = tail call ptr @\"window$words\"(i64 512)";
    assert_eq!(call_target(call), Some("\"window$words\""));
    assert_eq!(call_argument(call, "\"window$words\"", 0), Some("i64 512"));
    assert_eq!(source_function(body, "\"window$words\""), body.trim_end());
    assert_eq!(retained_constructor_bytes(body, call), Ok(4112));
    // Each source call contributes its own extent, not one take per helper
    // definition. Wrong arithmetic must change the observed size map.
    let calls = [call.to_owned(), call.replace("512", "1024")];
    let mut sizes = std::collections::BTreeMap::new();
    for site in &calls {
        *sizes
            .entry(retained_constructor_bytes(body, site).unwrap())
            .or_insert(0) += 1;
    }
    assert_eq!(
        sizes,
        std::collections::BTreeMap::from([(4112, 1), (8208, 1)])
    );
    assert_eq!(
        retained_constructor_bytes(&body.replace("%capacity, 3", "%capacity, 2"), call),
        Ok(2064)
    );
    assert_eq!(
        retained_constructor_bytes(&body.replace("i64 16)", "i64 24)"), call),
        Ok(4120)
    );
    assert_eq!(
        retained_constructor_bytes(
            &body.replace("shl i64 %capacity, 3", "mul i64 %capacity, 8"),
            call
        ),
        Ok(4112)
    );
    let bytes = body.replace("shl i64 %capacity, 3", "add i64 %capacity, 0");
    assert_eq!(
        retained_constructor_bytes(&bytes, &call.replace("512", "4096")),
        Ok(4112)
    );

    for (fault, bad) in [
        ("missing take", body.replace("%fresh = call ptr @wf__heap_take(i64 %bytes)", "%fresh = inttoptr i64 1 to ptr")),
        ("two takes", body.replace("%null =", "%extra = call ptr @wf__heap_take(i64 %bytes)\n  %null =")),
        ("cyclic take", body.replace("label %oom, label %initialize", "label %allocate, label %initialize")),
        ("unproved bypass", body.replace("icmp eq i64 %capacity, 0", "icmp eq i64 %capacity, 512")),
        ("reused return", body.replace("[ %fresh, %initialize ]", "[ @wf__empty_window, %initialize ]")),
        ("wrong header", body.replace("store i64 %capacity", "store i64 17")),
        ("missing header word", body.replace("  store i64 %capacity, ptr %cap, align 8\n", "")),
        ("repeated header write", body.replace("  store i64 %capacity, ptr %cap, align 8", "  store i64 %capacity, ptr %cap, align 8\n  store i64 %capacity, ptr %cap, align 8")),
        ("wrong header address", body.replace("getelementptr inbounds i8", "getelementptr inbounds i64")),
        ("unsupported extent", body.replace("shl i64 %capacity, 3", "xor i64 %capacity, 3")),
        ("payload initialization", body.replace("br label %ready", "call void @llvm.memset.p0.i64(ptr %fresh, i8 0, i64 16, i1 false)\n  br label %ready")),
    ] {
        assert_ne!(bad, body, "{fault} mutation must change its witness");
        assert!(retained_constructor_bytes(&bad, call).is_err(), "{fault}");
    }
    assert!(retained_constructor_bytes(body, &call.replace("512", "0")).is_err());
    assert!(retained_constructor_bytes(body, &call.replace("512", "%unknown")).is_err());
    assert_eq!(
        call_target("  call ptr @window$bytes(i64 4096)"),
        Some("window$bytes")
    );
    assert_eq!(call_target("  call ptr @\"unfinished(i64 1)"), None);
}

/// A counted retake remakes the existing cell; a fresh take or a null input
/// cannot stand in for growth. The copy-source/release oracle belonged to
/// the former allocate-copy-free lowering.
fn remakes_a_run(allocation: &str) -> bool {
    call_target(allocation) == Some("wf__heap_retake")
        && call_argument(allocation, "wf__heap_retake", 0)
            .and_then(|argument| argument.split_whitespace().next_back())
            .is_some_and(|old| old != "null")
}

#[test]
fn run_remake_oracle_requires_a_retake_of_an_existing_block() {
    let remake = "  %fresh = call ptr @wf__heap_retake(ptr %old, i64 48, i64 %bytes)";
    assert!(remakes_a_run(remake));
    assert!(!remakes_a_run(&remake.replace("ptr %old", "ptr null")));
    assert!(!remakes_a_run(
        "  %fresh = call ptr @wf__heap_take(i64 %bytes)"
    ));
    assert!(!remakes_a_run(
        "  %fresh = call ptr @realloc(ptr %old, i64 %bytes)"
    ));
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
    // names pass 4096 bytes or whose entries pass 512. Each remake is one
    // `realloc` of the old block to the larger size [OP-10].
    //
    // The source still owns initialization. With exclusive run rows LLVM can
    // fold an allocator plus a zero-fill loop into calloc. The counted
    // wrapper retains its accounting side effect. Count either optimized
    // spelling, including both calloc factors, while retaining the exact
    // source-site and per-size counts, and one allocation per retained helper.
    // Branch weights can leave an allocation primitive out of line. Its body
    // is not a source allocation site: credit its validated take at each call.
    let retained_helpers: std::collections::BTreeMap<_, _> = optimized()
        .lines()
        .filter(|line| line.starts_with("define "))
        .filter_map(function_token)
        .map(|symbol| (symbol, source_function(optimized(), symbol)))
        .filter(|(_, body)| !program_functions().contains(body))
        .filter(|(_, body)| {
            body.lines()
                .any(|line| matches!(call_target(line), Some("wf__heap_take" | "calloc")))
        })
        .collect();
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
            if call_target(line) == Some("wf__heap_retake") {
                // `grow` [OP-10] remakes a run with one `realloc`, which keeps
                // the filled slots; a remake is never a take.
                remakes += 1;
                assert!(
                    remakes_a_run(line),
                    "a retake remakes an existing run and never takes a new one: {line}"
                );
            }
            if let Some(callee @ ("wf__heap_take" | "calloc")) = call_target(line) {
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
            if let Some(body) = call_target(line).and_then(|callee| retained_helpers.get(callee)) {
                let bytes = retained_constructor_bytes(body, line)
                    .unwrap_or_else(|reason| panic!("{reason}"));
                out_of_line += 1;
                *sizes.entry(bytes).or_insert(0) += 1;
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
        "widen_window, push_byte and push_word each remake a run by reallocating it: {remakes}"
    );
    // Only `grow` reallocates, and nothing re-initializes. That the fill
    // runs once per take is a source fact under this surface rather than an
    // allocator guarantee: the fill loop is inside `zeroed_bytes`, between the
    // take and the hand-back, so a caller cannot reach a filled run without
    // having taken it and cannot re-reach the fill without taking another. A
    // remake fills only the slots it added, at a run-time offset past the
    // prefix its realloc preserved, and only once.
    // LLVM may retain that first fill as a memset after the counted allocation
    // and its null check. The provenance/control-flow oracle
    // permits one such fill per allocation and refuses repeated fills, fills
    // reached again without allocation, and fills through incoming pointers.
    // Aggregate or frame initialization may also become a memset, without
    // refilling a run's heap payload. The no-refill oracle must not depend on
    // which aggregate stores this optimizer combines.
    // Keep the no-refill claim on pointer provenance, rather than forbidding
    // the unrelated aggregate initialization instruction by name.
    // `grow` uses the counted retake, so bare reallocation would bypass
    // accounting; other reallocating or clearing spellings stay forbidden.
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
        let Some(first_allocation) = ["@wf__heap_take(", "@calloc(", "@wf_zeroed_bytes("]
            .iter()
            .filter_map(|site| function.find(site))
            .chain(
                retained_helpers
                    .keys()
                    .filter_map(|callee| function.find(&format!("@{callee}("))),
            )
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

    // Every linear handle closes explicitly: the file and both halves of the
    // working directory [PRE-2]. Affine output owners have empty drop.
    assert_eq!(
        trace.matches("wf_test close ").count(),
        3,
        "trace was {trace:?}"
    );
    assert_eq!(
        trace
            .lines()
            .filter(|line| line.starts_with("wf_test close fd=")
                && !line.starts_with("wf_test close fd=42 ")
                && line.ends_with("outcome=ok"))
            .count(),
        2
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
