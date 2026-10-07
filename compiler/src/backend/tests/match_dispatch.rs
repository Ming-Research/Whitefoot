//! compiler/match-dispatch-lowering: a loop whose header ends in a `match`
//! is emitted as one function per arm, chained by guaranteed tail calls.

use super::{compile_and_run, emit, emitted_body};

/// A four-instruction interpreter written as self-tail transfers [FN-10]:
/// `Add 3; Dec; Jnz 0; Halt` with a count of 1000 returns 3000. `RESULT`
/// is the result type and `DONE(x)` constructs it from the accumulator.
const INTERPRETER: &str = r#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

enum Op {
  Add(k: u64);
  Dec();
  Jnz(t: u64);
  Halt();
}

enum Outcome {
  Done(value: u64);
  Failed();
}

fn run(code: &Box<Slots<Op>>, pc: u64, acc: u64, count: u64) -> r: RESULT reads(code) contract {
  requires pc < code^.inner.len;
} {
  let n = code^.inner.len;
  match code^.inner[pc] {
    Add(k: kv) => {
      let next = pc + 1_u64;
      let sum = acc +wrap kv^;
      if next < n {
        return musttail run(code: code, pc: next, acc: sum, count: count);
      }
      return FAILED;
    }
    Dec() => {
      let next = pc + 1_u64;
      let left = count -wrap 1_u64;
      if next < n {
        return musttail run(code: code, pc: next, acc: acc, count: left);
      }
      return FAILED;
    }
    Jnz(t: tv) => {
      let next = pc + 1_u64;
      if count != 0_u64 {
        set next = tv^;
      }
      if next < n {
        return musttail run(code: code, pc: next, acc: acc, count: count);
      }
      return FAILED;
    }
    Halt() => {
      let done = DONE;
      return done;
    }
  }
}

fn push(code: &Box<Slots<Op>>, op: Op) -> ok: Bool writes(code) {
  if code^.inner.len < code^.inner.cap {
    place_back(window: &code^.inner, value: op);
    return True();
  }
  return False();
}

fn main() -> status: ExitStatus pure {
  let code = box_slots_new::<Op>(capacity: 4_u64);
  let c0 = Op::Add(k: 3_u64);
  let p0 = push(code: &code, op: c0);
  let c1 = Op::Dec();
  let p1 = push(code: &code, op: c1);
  let c2 = Op::Jnz(t: 0_u64);
  let p2 = push(code: &code, op: c2);
  let c3 = Op::Halt();
  let p3 = push(code: &code, op: c3);
  if code.inner.len > 0_u64 {
    let r = run(code: &code, pc: 0_u64, acc: 0_u64, count: 1000_u64);
    CHECK
  }
  return exit_status(code: 2_u8);
}
"#;

fn scalar_interpreter() -> String {
    INTERPRETER
        .replace("RESULT", "u64")
        .replace("FAILED", "0_u64")
        .replace("DONE", "acc")
        .replace(
            "CHECK",
            "if r == 3000_u64 {\n      return exit_status(code: 0_u8);\n    }\n    return exit_status(code: 1_u8);",
        )
}

fn enum_interpreter() -> String {
    INTERPRETER
        .replace("RESULT", "Outcome")
        .replace("FAILED", "Outcome::Failed()")
        .replace("DONE", "Outcome::Done(value: acc)")
        .replace(
            "CHECK",
            "match r {\n      Done(value: v) => {\n        if v == 3000_u64 {\n          return exit_status(code: 0_u8);\n        }\n        return exit_status(code: 1_u8);\n      }\n      Failed() => {\n        return exit_status(code: 3_u8);\n      }\n    }",
        )
}

fn definition<'module>(module: &'module str, symbol: &str) -> &'module str {
    let needle = format!(" @{symbol}(");
    let start = module
        .match_indices("define ")
        .map(|(at, _)| at)
        .find(|&at| {
            module[at..]
                .lines()
                .next()
                .is_some_and(|line| line.contains(&needle))
        })
        .unwrap_or_else(|| panic!("missing definition of {symbol}: {module}"));
    let end = module[start..]
        .find("\n}\n")
        .map(|offset| start + offset + 2)
        .expect("definition closes");
    &module[start..end]
}

/// The calling convention the host's assembler admits at run time and its
/// integer argument registers, as compiler/match-dispatch-lowering records
/// them.
fn host_convention() -> (&'static str, usize) {
    let preserve_none = crate::toolchain::facts().preserve_none;
    match (preserve_none, cfg!(target_arch = "aarch64"), cfg!(windows)) {
        (true, true, _) => ("preserve_nonecc ", 24),
        (true, false, true) => ("preserve_nonecc ", 12),
        (true, false, false) => ("preserve_nonecc ", 11),
        (false, _, true) => ("", 4),
        (false, true, false) => ("", 8),
        (false, false, false) => ("", 6),
    }
}

fn assert_split(module: &str, base: &str, arms: usize) {
    let (convention, _) = host_convention();
    let dispatch = definition(module, &format!("{base}.dispatch"));
    assert!(
        dispatch.starts_with(&format!("define internal {convention}"))
            && dispatch.contains("alwaysinline"),
        "the dispatch function is internal, inlined and of the parts' convention: {dispatch}"
    );
    // The table is reached through the parameter the enclosing function
    // passes where a register was left for it, and directly otherwise.
    let through_parameter = dispatch.contains("x ptr], ptr %wf.dispatch.base");
    assert!(
        (through_parameter || dispatch.contains(&format!("x ptr], ptr @{base}.dispatch.table")))
            && dispatch.contains(&format!("musttail call {convention}")),
        "the header transfers through the handler table: {dispatch}"
    );
    assert!(
        !through_parameter || module.contains(&format!("ptr @{base}.dispatch.table")),
        "the enclosing function passes the table's address: {module}"
    );
    assert!(
        !dispatch.contains("switch "),
        "the header's match is the table transfer: {dispatch}"
    );
    for arm in 0..arms {
        let arm = definition(module, &format!("{base}.arm.{arm}"));
        assert!(
            arm.starts_with(&format!("define internal {convention}")),
            "{arm}"
        );
        assert!(
            !arm.contains("%wf.frame = alloca"),
            "only the enclosing function allocates the frame: {arm}"
        );
    }
    assert!(
        !module.contains(&format!("@{base}.arm.{arms}(")),
        "one function per arm"
    );
    let table = module
        .lines()
        .find(|line| line.starts_with(&format!("@{base}.dispatch.table = ")))
        .expect("the handler table is emitted");
    assert!(
        table.contains(&format!("[{arms} x ptr]"))
            && (0..arms).all(|arm| table.contains(&format!("ptr @{base}.arm.{arm}"))),
        "the table has one entry per tag: {table}"
    );
}

/// The ledger's verdict on one function's loop.
fn verdict<'module>(module: &'module str, symbol: &str) -> &'module str {
    module
        .lines()
        .find_map(|line| {
            line.strip_prefix(crate::DISPATCH_LEDGER_PREFIX)
                .and_then(|line| line.strip_prefix(&format!("{symbol}: ")))
                .filter(|line| line.starts_with("split") || line.starts_with("not split"))
        })
        .unwrap_or_else(|| panic!("the ledger has a verdict for {symbol}: {module}"))
}

/// The four-instruction interpreter's split: its three looping arms tail-call
/// the dispatch function, the halting arm returns, and the scrutinee's copy,
/// which only the dispatch function reads, is that function's own
/// allocation.
fn assert_interpreter_split(module: &str, base: &str) {
    let (convention, _) = host_convention();
    assert_split(module, base, 4);
    for arm in 0..3 {
        let arm = definition(module, &format!("{base}.arm.{arm}"));
        assert!(
            arm.contains(&format!("musttail call {convention}"))
                && arm.contains(&format!("@{base}.dispatch(")),
            "every looping arm ends in a guaranteed tail call of the dispatch function: {arm}"
        );
    }
    let halt = definition(module, &format!("{base}.arm.3"));
    assert!(
        !halt.contains("musttail"),
        "the halting arm returns: {halt}"
    );
    let dispatch = definition(module, &format!("{base}.dispatch"));
    assert!(
        dispatch.contains(" = alloca "),
        "a slot only the dispatch function uses is its own: {dispatch}"
    );
}

#[test]
fn a_header_match_loop_is_split_into_one_function_per_arm() {
    let module = emit(scalar_interpreter().as_bytes());
    let (convention, _) = host_convention();
    let verdict = verdict(&module, "wf_run");
    if !convention.is_empty() {
        // Eight parameters, in order: pc, acc, count, the hoisted code
        // length and box referent (the run), the cell's address, the handler
        // table and the frame. Jnz's joined next index is not a known step,
        // so its edge needs the run to form the next cell's address.
        assert!(
            verdict.starts_with(
                "split: the loop over Op into 4 arms, taking 8 integer and 0 floating"
            ),
            "{verdict}"
        );
    }
    if verdict.starts_with("split") {
        assert_interpreter_split(&module, "wf_run");
        // `code` is read-only and passed through unchanged, so its box's
        // referent and length are computed once before the loop and no part
        // loads a pointer out of a pointer it receives.
        let dispatch = definition(&module, "wf_run.dispatch");
        let header = dispatch.lines().next().expect("a definition header");
        for parameter in header
            .split(", ")
            .filter(|parameter| parameter.contains("ptr"))
        {
            let name = parameter
                .rsplit(' ')
                .next()
                .expect("a named parameter")
                .trim_end_matches(')');
            assert!(
                !dispatch.contains(&format!("load ptr, ptr {name}\n")),
                "the dispatch function reloads {name}: {dispatch}"
            );
        }
        let enclosing = emitted_body(&module, "run");
        assert!(
            enclosing.contains(&format!("call {convention}i64 @wf_run.dispatch("))
                && !enclosing.contains("musttail"),
            "the enclosing function calls the dispatch function once and returns its result: {enclosing}"
        );
    } else {
        assert!(!module.contains("@wf_run.dispatch"), "{module}");
    }
    let output = compile_and_run(&module);
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn a_result_returned_through_its_destination_threads_the_destination_through_every_part() {
    let module = emit(enum_interpreter().as_bytes());
    let base = if module.contains(" @wf_run.body(") {
        "wf_run.body"
    } else {
        "wf_run"
    };
    let (convention, _) = host_convention();
    let verdict = verdict(&module, base);
    assert!(
        convention.is_empty() || verdict.starts_with("split"),
        "{verdict}"
    );
    if verdict.starts_with("split") {
        assert_interpreter_split(&module, base);
        let dispatch = definition(&module, &format!("{base}.dispatch"));
        assert!(
            base != "wf_run.body" || dispatch.contains("(ptr %wf.result, "),
            "a destination-form body passes its destination to every part: {dispatch}"
        );
    } else {
        assert!(!module.contains(&format!("@{base}.dispatch")), "{module}");
    }
    let output = compile_and_run(&module);
    assert!(output.status.success(), "{output:?}");
}

/// An interpreter whose path tells a carried element address from a wrong
/// one: it enters at `pc` 1, past an `Add 1000` no correct path reaches,
/// jumps forward to 3, repeats `Rep 5` three times in place, then jumps back
/// to 2 and halts with 15. `Halt` adds 100 times its `pc`, so an address
/// that reaches it while `pc` names another operation also changes the
/// result, 215.
const CURSOR_INTERPRETER: &str = r#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

enum Op {
  Add(k: u64);
  Jump(t: u64);
  Rep(k: u64);
  Halt();
}

fn run(code: &Box<Slots<Op>>, pc: u64, acc: u64, count: u64) -> r: u64 reads(code) contract {
  requires pc < code^.inner.len;
} {
  let n = code^.inner.len;
  match code^.inner[pc] {
    Add(k: kv) => {
      let next = pc + 1_u64;
      let sum = acc +wrap kv^;
      if next < n {
        return musttail run(code: code, pc: next, acc: sum, count: count);
      }
      return 0_u64;
    }
    Jump(t: tv) => {
      let target = tv^;
      if target < n {
        return musttail run(code: code, pc: target, acc: acc, count: count);
      }
      return 0_u64;
    }
    Rep(k: kv) => {
      if count != 0_u64 {
        let sum = acc +wrap kv^;
        let left = count -wrap 1_u64;
        return musttail run(code: code, pc: pc, acc: sum, count: left);
      }
      let next = pc + 1_u64;
      if next < n {
        return musttail run(code: code, pc: next, acc: acc, count: count);
      }
      return 0_u64;
    }
    Halt() => {
      let at = pc *wrap 100_u64;
      let r = acc +wrap at;
      return r;
    }
  }
}

fn push(code: &Box<Slots<Op>>, op: Op) -> ok: Bool writes(code) {
  if code^.inner.len < code^.inner.cap {
    place_back(window: &code^.inner, value: op);
    return True();
  }
  return False();
}

fn main() -> status: ExitStatus pure {
  let code = box_slots_new::<Op>(capacity: 5_u64);
  let c0 = Op::Add(k: 1000_u64);
  let p0 = push(code: &code, op: c0);
  let c1 = Op::Jump(t: 3_u64);
  let p1 = push(code: &code, op: c1);
  let c2 = Op::Halt();
  let p2 = push(code: &code, op: c2);
  let c3 = Op::Rep(k: 5_u64);
  let p3 = push(code: &code, op: c3);
  let c4 = Op::Jump(t: 2_u64);
  let p4 = push(code: &code, op: c4);
  if code.inner.len > 1_u64 {
    let r = run(code: &code, pc: 1_u64, acc: 0_u64, count: 3_u64);
    if r == 215_u64 {
      return exit_status(code: 0_u8);
    }
    return exit_status(code: 1_u8);
  }
  return exit_status(code: 2_u8);
}
"#;

#[test]
fn the_matched_element_s_address_travels_between_the_parts() {
    // The header matches `code^.inner[pc]` and the arms read the operation
    // through its address, so the parts carry that address: the dispatch
    // function no longer forms it from `pc`. A known step moves the received
    // address; a jump forms its address from the run. The result checks the
    // entering address, forward and backward jumps, and an in-place edge.
    // Reorder the tags independently of the match arms and construct in
    // several functions, including enum results returned through a destination.
    // The same execution still checks the cursor and the result 215.
    let source = CURSOR_INTERPRETER
        .replace(
            "  Add(k: u64);\n  Jump(t: u64);\n  Rep(k: u64);\n  Halt();",
            "  Jump(t: u64);\n  Add(k: u64);\n  Halt();\n  Rep(k: u64);",
        )
        .replace(
            "let c0 = Op::Add(k: 1000_u64);",
            "let c0 = make_add(k: 1000_u64);",
        )
        .replace(
            "let c3 = Op::Rep(k: 5_u64);",
            "let c3 = make_rep(k: 5_u64);",
        )
        .replace(
            "fn main()",
            r#"fn make_add(k: u64) -> r: Op pure {
  return Op::Add(k: k);
}

fn make_rep(k: u64) -> r: Op pure {
  return Op::Rep(k: k);
}

fn main()"#,
        );
    let module = emit(source.as_bytes());
    if verdict(&module, "wf_run").starts_with("split") {
        assert_element_handlers(&module, "wf_run");
        // Before the prototype none of these stores exists. Checking each
        // tag beside its handler store also catches using tag as arm ordinal.
        for (function, tag, arm, sites) in [
            ("make_add", 1, 0, 1),
            ("make_rep", 3, 2, 1),
            ("main", 0, 1, 2),
            ("main", 2, 3, 1),
        ] {
            let body = emitted_body(&module, function);
            let lines: Vec<_> = body.lines().collect();
            let stores = lines
                .windows(3)
                .filter(|lines| {
                    lines[0]
                        .trim_start()
                        .starts_with(&format!("store i32 {tag}, ptr "))
                        && lines[1].ends_with(", i32 0, i32 2")
                        && lines[2]
                            .trim_start()
                            .starts_with(&format!("store ptr @wf_run.arm.{arm}, ptr "))
                })
                .count();
            assert_eq!(
                stores, sites,
                "every construction stores its mapped handler in {function}: {body}"
            );
        }
        assert_eq!(
            module.matches("store ptr @wf_run.arm.").count(),
            5,
            "all five construction sites initialize the hidden word: {module}"
        );
        assert!(
            module.contains(&format!(
                "{}wf_run: carries the matched Op's address between the parts",
                crate::DISPATCH_LEDGER_PREFIX
            )),
            "the ledger reports the carried address: {module}"
        );
        let dispatch = definition(&module, "wf_run.dispatch");
        assert!(
            !dispatch
                .lines()
                .any(|line| line.contains("= getelementptr inbounds {")
                    && !line.contains("%wf.frame")),
            "the dispatch function receives the element's address instead of forming it: {dispatch}"
        );
        let add = definition(&module, "wf_run.arm.0");
        let step = add
            .lines()
            .find(|line| line.contains(" = getelementptr %") && line.ends_with(", i64 1"))
            .expect("Add moves the received address by one element");
        let (_, operands) = step.split_once(", ptr ").expect("the cursor GEP's pointer");
        let (place, _) = operands.split_once(',').expect("the cursor GEP's index");
        assert!(
            !add.contains("sub i64")
                && add
                    .lines()
                    .next()
                    .expect("the part's signature")
                    .contains(&format!("ptr {place}")),
            "Add moves a received address without subtracting indices: {add}"
        );
        let jump = definition(&module, "wf_run.arm.1");
        let (_, jump_body) = jump
            .split_once("  br label ")
            .expect("the part's prelude ends");
        let run = cursor_run(&module);
        assert!(
            !jump.contains("sub i64")
                && jump_body.lines().any(|line| {
                    line.contains(" = getelementptr inbounds {")
                        && line.contains(&format!(", ptr {run},"))
                }),
            "Jump forms the target address from the run after the prelude: {jump}"
        );
        let rep = definition(&module, "wf_run.arm.2");
        assert!(
            rep.lines().any(|line| {
                line.contains(" = getelementptr %")
                    && line.ends_with(&format!(", ptr {place}, i64 1"))
            }) && rep.lines().any(|line| {
                line.contains("musttail call ") && line.contains(&format!(", ptr {place},"))
            }),
            "Rep advances by one element on exit and passes the received address unchanged in place: {rep}"
        );
    }
    let output = compile_and_run(&module);
    assert!(output.status.success(), "{output:?}");
}

/// Every mechanism assertion here fails on the old table-based dispatch:
/// it has neither the ledger line, the pointer field nor its element load.
fn assert_element_handlers(module: &str, base: &str) {
    let dispatch = definition(module, &format!("{base}.dispatch"));
    let word = dispatch
        .lines()
        .find(|line| {
            line.contains(" = getelementptr inbounds %wf.t.") && line.ends_with(", i32 0, i32 2")
        })
        .expect("dispatch addresses the element's hidden pointer field");
    let (slot, gep) = word
        .trim()
        .split_once(" = getelementptr inbounds ")
        .unwrap();
    let (ty, operands) = gep.split_once(", ptr ").unwrap();
    let (place, _) = operands.split_once(',').unwrap();
    let handler = dispatch
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_suffix(&format!(" = load ptr, ptr {slot}"))
        })
        .expect("the dispatch target is loaded from the hidden word");
    let (convention, _) = host_convention();
    assert!(
        module.contains(&format!(
            "{}{}: dispatches through the handler word in each Op",
            crate::DISPATCH_LEDGER_PREFIX,
            base
        )) && dispatch
            .lines()
            .next()
            .unwrap()
            .contains(&format!("ptr {place}"))
            && dispatch.contains(&format!("musttail call {convention}i64 {handler}("))
            && !dispatch.contains("load i32")
            && !dispatch.contains("x ptr]")
            && !module.contains(&format!("@{base}.dispatch.table"))
            && !dispatch.contains("%wf.dispatch.base"),
        "dispatch loads and calls the element's handler without a tag, table or table parameter: {dispatch}"
    );
    assert!(
        module.contains(&format!(
            "{ty} = type {{ i32, [12 x i8], ptr, [0 x i8], [0 x {ty}.v0] }}"
        )) && module
            .lines()
            .any(|line| line.contains("call void @llvm.memmove.")
                && line.contains(&format!("getelementptr ({ty}, ptr null, i32 1)"))),
        "the 16-byte variant gains an aligned pointer and memmove copies its complete type: {module}"
    );
}

#[test]
fn handler_words_follow_destination_body_names_and_fragment_selection() {
    // A third payload makes Op memory-only while Outcome still returns in
    // registers. The loop's handlers therefore belong to wf_run.body.
    let source = enum_interpreter()
        .replace("  Dec();\n", "  Dec(k: u64);\n")
        .replace("    Dec() =>", "    Dec(k: unused) =>")
        .replace("Op::Dec()", "Op::Dec(k: 0_u64)");
    let inputs = [crate::SourceInput::new("threaded.wf", source.as_bytes())];
    let whole = crate::compile_for_emission(
        &inputs,
        crate::CompilerLimits::default(),
        crate::OverlapLowering::Off,
        None,
        false,
    )
    .expect("whole-program emission")
    .0;
    let fragments = crate::compile_for_emission(
        &inputs,
        crate::CompilerLimits::default(),
        crate::OverlapLowering::Off,
        None,
        true,
    )
    .expect("fragment-compatible emission")
    .0;
    let split = verdict(&whole, "wf_run.body").starts_with("split");
    let stores = whole.matches("store ptr @wf_run.body.arm.").count();
    // Both modes had the same layout before the prototype. The positive
    // store/layout difference makes these controls fail on that old output.
    assert!(
        whole.contains("[12 x i8], ptr, [0 x i8]")
            && (if split {
                stores == 4
            } else {
                stores == 0 && whole.contains("store ptr null")
            })
            && !fragments.contains("[12 x i8], ptr, [0 x i8]")
            && !fragments.contains("store ptr @wf_run.body.arm."),
        "constructors name the emitted .body arms only in whole-module emission: {whole}"
    );
    let pieces = crate::split_module(&fragments, crate::FragmentGranularity::Function)
        .expect("fragment dependencies resolve without cross-module handler words");
    assert!(
        whole.contains("[12 x i8], ptr, [0 x i8]")
            && pieces
                .iter()
                .all(|piece| !piece.contains("store ptr @wf_run.body.arm.")),
        "fragment output never stores private handler addresses"
    );
    // The restriction survives a retained-module round trip; before this
    // prototype both calls succeeded instead of enforcing fragment selection.
    for module in [
        &whole,
        &crate::LlvmModule::decode(&whole.encode()).expect("retained module"),
    ] {
        assert!(
            crate::split_module(module, crate::FragmentGranularity::Function)
                .is_err_and(|failure| failure.to_string().contains("fragments=true")),
            "a whole-program layout cannot be split after emission"
        );
    }
}

#[test]
fn an_unsplit_owner_initializes_null_and_a_second_owner_disables_threading() {
    let source = format!(
        "{CURSOR_INTERPRETER}{}",
        r#"
enum Small {
  A(a: u8);
  B(b: u8);
  C(c: u8);
}

fn small(op: &Small, count: u64) -> r: u64 reads(op) {
  match op^ {
    A(a: unused) => {
      if count != 0_u64 {
        let next = count -wrap 1_u64;
        return musttail small(op: op, count: next);
      }
      return 0_u64;
    }
    B(b: unused) => {
      return 1_u64;
    }
    C(c: unused) => {
      return 2_u64;
    }
  }
}
"#
    );
    super::system::with_ir(source.as_bytes(), |program| {
        let target = crate::target::TargetLayout::host().expect("test target");
        let id = program
            .nominals()
            .iter()
            .find(|n| n.name() == "Op")
            .unwrap()
            .id();
        let layout = |program: &crate::IrProgram| {
            crate::target::union_enum_layout(target, program, id).expect("Op layout")
        };
        let selected = crate::backend::emitter::prepare_dispatch_layout(program, target, false)
            .expect("whole-program selection");
        let threaded = layout(&selected);
        assert!(
            threaded.size() == 24
                && threaded.handler_offset() == Some(16)
                && layout(program).size() == 16,
            "the hidden word follows the largest variant; previously Op stayed 16 bytes"
        );
        let small = selected
            .nominals()
            .iter()
            .find(|n| n.name() == "Small")
            .unwrap();
        let small_layout = crate::target::union_enum_layout(target, &selected, small.id())
            .expect("Small is also a memory-only union");
        assert!(
            threaded.handler_offset() == Some(16)
                && small_layout.handler_offset().is_none()
                && small_layout.size() == 8,
            "the pointer cannot enlarge Small beyond its 8-byte, 4-aligned OP-9 ceiling"
        );

        let owner = program
            .functions()
            .iter()
            .position(|f| f.name() == "run")
            .unwrap();
        let mut unsplit = program.clone();
        // Synthesis is one of the ordinary planner's explicit unsplit cases.
        unsplit.functions[owner].synthesis = Some(crate::IrSynthesis::Chunk);
        let module = crate::backend::emitter::emit_llvm_with_layout(&unsplit, target)
            .expect("whole-function emission");
        let ty = format!("%wf.t.{}", program.nominal(id).unwrap().link_name());
        let lines: Vec<_> = module.lines().collect();
        let nulls = lines
            .windows(2)
            .filter(|pair| {
                pair[0].contains(&format!("getelementptr inbounds {ty},"))
                    && pair[0].ends_with(", i32 0, i32 2")
                    && pair[1].trim_start().starts_with("store ptr null, ptr ")
            })
            .count();
        assert!(
            nulls == 5
                && !module.contains("@wf_run.arm.")
                && verdict(&module, "wf_run").contains("compiler-synthesized"),
            "all constructors initialize an unused null word without undefined arms: {module}"
        );

        let mut two = program.clone();
        let mut other = two.functions[owner].clone();
        other.name = "other_run".to_owned();
        two.functions.push(other);
        let two = crate::backend::emitter::prepare_dispatch_layout(&two, target, false)
            .expect("two-loop program");
        assert!(
            threaded.handler_offset() == Some(16) && layout(&two).handler_offset().is_none(),
            "a second recognized loop anywhere in the program restores ordinary layout"
        );

        let mut foreign = program.clone();
        let mut native = foreign.functions[owner].clone();
        native.name = "native_run".to_owned();
        native.blocks.clear();
        foreign.functions.push(native);
        let foreign = crate::backend::emitter::prepare_dispatch_layout(&foreign, target, false)
            .expect("native boundary program");
        assert!(
            threaded.handler_offset() == Some(16) && layout(&foreign).handler_offset().is_none(),
            "an enum reachable through a native signature keeps its native layout"
        );
    });
}

#[test]
fn the_cursor_s_run_address_is_passed_when_a_jump_needs_it() {
    // Jump's target is read from the operation, so its edge forms an address
    // from the run. The cursor execution test above covers this program.
    let module = emit(CURSOR_INTERPRETER.as_bytes());
    let (convention, _) = host_convention();
    let verdict = verdict(&module, "wf_run");
    assert!(
        convention.is_empty() || verdict.starts_with("split"),
        "{verdict}"
    );
    if verdict.starts_with("split") {
        let run = cursor_run(&module);
        let mut symbols = vec!["wf_run.dispatch".to_owned()];
        symbols.extend((0..4).map(|arm| format!("wf_run.arm.{arm}")));
        for symbol in symbols {
            let part = definition(&module, &symbol);
            // A smaller C convention may spill the run; preserve_none has
            // room for this fixture's common parameter list.
            assert!(
                part.lines()
                    .next()
                    .expect("the part's signature")
                    .split(|c: char| c.is_whitespace() || c == ',' || c == ')')
                    .any(|word| word == run)
                    || (convention.is_empty() && part.contains(&format!("  {run} = load ptr,"))),
                "each part receives the run for Jump, in a parameter or a C-convention spill: {part}"
            );
        }
    }
}

/// The final element GEP forms the entering cursor. The length may use
/// another projection, so identify the run by this GEP's operand.
fn cursor_run(module: &str) -> &str {
    let enclosing = definition(module, "wf_run");
    let run = enclosing
        .lines()
        .rev()
        .find(|line| {
            line.contains(" = getelementptr inbounds ")
                && line.contains(", i32 ")
                && line
                    .rsplit(", ")
                    .next()
                    .is_some_and(|index| index.starts_with("i64 "))
        })
        .and_then(|line| line.split_once(", ptr "))
        .and_then(|(_, operands)| operands.split_once(','))
        .map(|(value, _)| value)
        .expect("the enclosing function forms the entering cursor from the run");
    assert!(
        enclosing.contains(&format!("\n  {run} = ")),
        "the enclosing function still computes the run the entering cursor needs: {enclosing}"
    );
    run
}

#[test]
fn known_steps_leave_the_cursor_s_run_in_the_enclosing_function() {
    // Enter at 1, skip Add 1000, add 7 and 3, repeat Rep 5 three times,
    // then halt at 4: 7 + 3 + 3 * 5 + 4 * 100 = 425. Every edge is pc or
    // pc + 1, so no part needs the run, and advancing edges use a literal
    // step rather than subtracting indices. Only this distinct program gets
    // an additional execution; the Jump program still runs once above.
    let source = CURSOR_INTERPRETER
        .replace("  Jump(t: u64);\n", "")
        .replace(
            r#"    Jump(t: tv) => {
      let target = tv^;
      if target < n {
        return musttail run(code: code, pc: target, acc: acc, count: count);
      }
      return 0_u64;
    }
"#,
            "",
        )
        .replace("Op::Jump(t: 3_u64)", "Op::Add(k: 7_u64)")
        .replace("let c2 = Op::Halt();", "let c2 = Op::Add(k: 3_u64);")
        .replace("Op::Jump(t: 2_u64)", "Op::Halt()")
        .replace("r == 215_u64", "r == 425_u64");
    let module = emit(source.as_bytes());
    let (convention, _) = host_convention();
    let split = verdict(&module, "wf_run").starts_with("split");
    let mut known_steps_without_run = convention.is_empty();
    if split {
        let run = cursor_run(&module);
        let parts = [
            "wf_run.dispatch",
            "wf_run.arm.0",
            "wf_run.arm.1",
            "wf_run.arm.2",
        ];
        let run_absent = parts.iter().all(|symbol| {
            !definition(&module, symbol)
                .split(|c: char| c.is_whitespace() || c == ',' || c == ')')
                .any(|word| word == run)
        });
        let add = definition(&module, "wf_run.arm.0");
        known_steps_without_run = run_absent
            && !add.contains("sub i64")
            && add
                .lines()
                .any(|line| line.contains(" = getelementptr %") && line.ends_with(", i64 1"));
    }
    let output = compile_and_run(&module);
    assert!(
        known_steps_without_run && output.status.success(),
        "known steps need no run parameter and execute to 425: {module}\n{output:?}"
    );
}

#[test]
fn a_two_variant_tag_indexes_the_handler_table_unsigned() {
    // A tag-only enum of two variants has a one-bit tag, which must index
    // the table as 0 or 1, never as -1. Ten alternating steps add 25.
    let source = r#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

enum Parity {
  Even();
  Odd();
}

fn run(n: u64, acc: u64, step: Parity) -> r: u64 pure {
  match step {
    Even() => {
      if n == 0_u64 {
        return acc;
      }
      let left = n -wrap 1_u64;
      let sum = acc +wrap 2_u64;
      let next = Parity::Odd();
      return musttail run(n: left, acc: sum, step: next);
    }
    Odd() => {
      if n == 0_u64 {
        return acc;
      }
      let left = n -wrap 1_u64;
      let sum = acc +wrap 3_u64;
      let next = Parity::Even();
      return musttail run(n: left, acc: sum, step: next);
    }
  }
}

fn main() -> status: ExitStatus pure {
  let first = Parity::Even();
  let r = run(n: 10_u64, acc: 0_u64, step: first);
  if r == 25_u64 {
    return exit_status(code: 0_u8);
  }
  return exit_status(code: 1_u8);
}
"#;
    let module = emit(source.as_bytes());
    assert_split(&module, "wf_run", 2);
    let dispatch = definition(&module, "wf_run.dispatch");
    assert!(
        dispatch.contains("zext i1 "),
        "the one-bit tag is zero-extended before indexing: {dispatch}"
    );
    let output = compile_and_run(&module);
    assert!(output.status.success(), "{output:?}");
}

/// A two-arm loop over thirty `u64` values. With `changing`, every arm adds
/// one to each of them, so all thirty change on every dispatch; otherwise
/// only the counter `p0` changes and the rest pass through. Starting from
/// p0 = 7 and the others at 1, it returns p29: 8 when they change, 1 when
/// they pass through.
fn thirty_values(changing: bool) -> String {
    let names: Vec<String> = (0..30).map(|index| format!("p{index}")).collect();
    let parameters = names
        .iter()
        .map(|name| format!("{name}: u64"))
        .collect::<Vec<_>>()
        .join(", ");
    let steps = if changing {
        names[1..]
            .iter()
            .map(|name| format!("      let n{name} = {name} +wrap 1_u64;\n"))
            .collect::<String>()
    } else {
        String::new()
    };
    let forwarded = names
        .iter()
        .map(|name| {
            if name == "p0" {
                "p0: left".to_owned()
            } else if changing {
                format!("{name}: n{name}")
            } else {
                format!("{name}: {name}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let initial = names
        .iter()
        .map(|name| {
            if name == "p0" {
                "p0: 7_u64".to_owned()
            } else {
                format!("{name}: 1_u64")
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let expected = if changing { 8 } else { 1 };
    format!(
        r#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

enum Parity {{
  Even();
  Odd();
}}

fn run({parameters}, step: Parity) -> r: u64 pure {{
  match step {{
    Even() => {{
      if p0 == 0_u64 {{
        return p29;
      }}
      let left = p0 -wrap 1_u64;
{steps}      let next = Parity::Odd();
      return musttail run({forwarded}, step: next);
    }}
    Odd() => {{
      if p0 == 0_u64 {{
        return p29;
      }}
      let left = p0 -wrap 1_u64;
{steps}      let next = Parity::Even();
      return musttail run({forwarded}, step: next);
    }}
  }}
}}

fn main() -> status: ExitStatus pure {{
  let first = Parity::Even();
  let r = run({initial}, step: first);
  if r == {expected}_u64 {{
    return exit_status(code: 0_u8);
  }}
  return exit_status(code: 1_u8);
}}
"#
    )
}

#[test]
fn a_loop_whose_changing_values_exceed_the_argument_registers_is_emitted_whole() {
    // Thirty values that change on every dispatch exceed every convention's
    // integer argument registers and none can go to the frame.
    let module = emit(thirty_values(true).as_bytes());
    assert!(!module.contains("@wf_run.dispatch"), "{module}");
    let output = compile_and_run(&module);
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn values_the_loop_cannot_change_go_to_the_frame_past_the_registers() {
    // Twenty-nine of the thirty values pass through unchanged, so they can
    // wait in the frame and the loop still splits.
    let module = emit(thirty_values(false).as_bytes());
    assert_split(&module, "wf_run", 2);
    assert!(
        module.contains(&format!("{}wf_run: keeps ", crate::DISPATCH_LEDGER_PREFIX)),
        "the ledger reports the values kept in the frame: {module}"
    );
    let enclosing = emitted_body(&module, "run");
    assert!(
        enclosing.contains("store i64 "),
        "the enclosing function stores them before the loop: {enclosing}"
    );
    let output = compile_and_run(&module);
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn a_loop_whose_match_is_not_its_header_is_emitted_whole() {
    // The header is the bounds test, so the match block is entered from
    // inside the loop and the loop is not a dispatch loop.
    let source = scalar_interpreter().replace(
        "fn run(code: &Box<Slots<Op>>",
        r#"fn count(code: &Box<Slots<Op>>) -> r: u64 reads(code) {
  let pc = 0_u64;
  let total = 0_u64;
  loop @scan {
    if pc >= code^.inner.len {
      break @scan;
    }
    match code^.inner[pc] {
      Add(k: kv) => {
        set total = total +wrap kv^;
      }
      Dec() => {
      }
      Jnz(t: tv) => {
      }
      Halt() => {
      }
    }
    set pc = pc + 1_u64;
  }
  return total;
}

fn run(code: &Box<Slots<Op>>"#,
    );
    let module = emit(source.as_bytes());
    assert!(!module.contains("@wf_count.dispatch"), "{module}");
    assert!(
        module.contains(&format!(
            "{}wf_count: not split: the loop over Op: the loop is entered other than at its match, which is therefore not its header\n",
            crate::DISPATCH_LEDGER_PREFIX
        )),
        "the ledger names the condition the loop failed: {module}"
    );
    // The interpreter in the same module still splits where its parameters
    // fit, so the absence above is the recogniser's verdict.
    if verdict(&module, "wf_run").starts_with("split") {
        assert_interpreter_split(&module, "wf_run");
    }
}

/// An interpreter over a register file it writes through `regs`. In `kept`
/// every arm only reads and writes elements, so the loop keeps `regs`'s box;
/// in `touched` one arm hands `regs` itself to a function that writes it,
/// so the box may change. Both return 3000.
const REGISTER_FILE: &str = r#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

enum Op {
  Add();
  Dec();
  Jnz(t: u64);
  Halt();
}

fn touch(regs: &Box<Slots<u64>>) -> r: u64 writes(regs) contract {
  ensures regs^.inner.len == entry(regs)^.inner.len;
} {
  if regs^.inner.len > 1_u64 {
    let c = regs^.inner[1_u64];
    let d = c -wrap 1_u64;
    set regs^.inner[1_u64] = d;
  }
  return 0_u64;
}

fn NAME(code: &Box<Slots<Op>>, regs: &Box<Slots<u64>>, pc: u64) -> r: u64 reads(code), writes(regs) contract {
  requires pc < code^.inner.len;
  requires 2_u64 <= regs^.inner.len;
} {
  let n = code^.inner.len;
  match code^.inner[pc] {
    Add() => {
      let a = regs^.inner[0_u64];
      let b = a +wrap 3_u64;
      set regs^.inner[0_u64] = b;
      let next = pc + 1_u64;
      if next < n {
        return musttail NAME(code: code, regs: regs, pc: next);
      }
      return 0_u64;
    }
    Dec() => {
      DECREMENT
      let next = pc + 1_u64;
      if next < n {
        return musttail NAME(code: code, regs: regs, pc: next);
      }
      return 0_u64;
    }
    Jnz(t: tv) => {
      let c = regs^.inner[1_u64];
      let next = pc + 1_u64;
      if c != 0_u64 {
        set next = tv^;
      }
      if next < n {
        return musttail NAME(code: code, regs: regs, pc: next);
      }
      return 0_u64;
    }
    Halt() => {
      let a = regs^.inner[0_u64];
      return a;
    }
  }
}

fn push(code: &Box<Slots<Op>>, op: Op) -> ok: Bool writes(code) {
  if code^.inner.len < code^.inner.cap {
    place_back(window: &code^.inner, value: op);
    return True();
  }
  return False();
}

fn main() -> status: ExitStatus pure {
  let code = box_slots_new::<Op>(capacity: 4_u64);
  let c0 = Op::Add();
  let p0 = push(code: &code, op: c0);
  let c1 = Op::Dec();
  let p1 = push(code: &code, op: c1);
  let c2 = Op::Jnz(t: 0_u64);
  let p2 = push(code: &code, op: c2);
  let c3 = Op::Halt();
  let p3 = push(code: &code, op: c3);
  let regs = box_slots_new::<u64>(capacity: 2_u64);
  if regs.inner.len < regs.inner.cap {
    place_back(window: &regs.inner, value: 0_u64);
  }
  if regs.inner.len < regs.inner.cap {
    place_back(window: &regs.inner, value: 1000_u64);
  }
  if code.inner.len > 0_u64 {
    if regs.inner.len >= 2_u64 {
      let r = NAME(code: &code, regs: &regs, pc: 0_u64);
      if r == 3000_u64 {
        return exit_status(code: 0_u8);
      }
      return exit_status(code: 1_u8);
    }
  }
  return exit_status(code: 2_u8);
}
"#;

/// Whether any part of a split loop loads a pointer out of a pointer
/// parameter it receives, as reloading a box's referent does.
fn some_part_reloads_a_box(module: &str, base: &str, arms: usize) -> bool {
    let mut symbols = vec![format!("{base}.dispatch")];
    symbols.extend((0..arms).map(|arm| format!("{base}.arm.{arm}")));
    symbols.iter().any(|symbol| {
        let part = definition(module, symbol);
        let header = part.lines().next().expect("a definition header");
        // A reference reaches a part as a parameter or, past the registers,
        // as a value its prelude loads from the shared frame.
        let parameters = header
            .split(", ")
            .filter(|parameter| parameter.contains("ptr"))
            .filter_map(|parameter| parameter.rsplit(' ').next())
            .map(|name| name.trim_end_matches(')').trim_end_matches(" {"));
        let spilled = part.lines().filter_map(|line| {
            line.trim()
                .split_once(" = load ptr, ptr %wf.slot.")
                .map(|(name, _)| name)
        });
        parameters
            .chain(spilled)
            .any(|name| part.contains(&format!("load ptr, ptr {name}\n")))
    })
}

#[test]
fn a_reference_whose_box_the_loop_keeps_is_projected_once() {
    let source = REGISTER_FILE.replace("NAME", "kept").replace(
        "DECREMENT",
        "let c = regs^.inner[1_u64];\n      let d = c -wrap 1_u64;\n      set regs^.inner[1_u64] = d;",
    );
    let module = emit(source.as_bytes());
    assert_split(&module, "wf_kept", 4);
    assert!(
        !some_part_reloads_a_box(&module, "wf_kept", 4),
        "no part reloads a box the loop keeps: {module}"
    );
    let output = compile_and_run(&module);
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn a_reference_handed_to_a_writer_is_reloaded_in_the_loop() {
    let source = REGISTER_FILE
        .replace("NAME", "touched")
        .replace("DECREMENT", "let ignored = touch(regs: regs);");
    let module = emit(source.as_bytes());
    assert_split(&module, "wf_touched", 4);
    assert!(
        some_part_reloads_a_box(&module, "wf_touched", 4),
        "a part reloads the box a writer may replace: {module}"
    );
    let dispatch = definition(&module, "wf_touched.dispatch");
    assert!(
        dispatch
            .lines()
            .next()
            .is_some_and(|header| header.contains("ptr noalias nonnull")),
        "the reference the parts receive keeps its checked facts: {dispatch}"
    );
    let output = compile_and_run(&module);
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn a_reference_handed_to_a_content_writer_keeps_its_box() {
    // `touch_content` writes only what the box holds, so it cannot replace
    // the box `regs` reaches: each part hands it a slot holding the box the
    // enclosing function projected, and no part loads the box again.
    let source = REGISTER_FILE
        .replace("NAME", "content")
        .replace("DECREMENT", "let ignored = touch_content(regs: regs);")
        .replace(
            "fn touch(regs: &Box<Slots<u64>>) -> r: u64 writes(regs) contract {",
            "fn touch_content(regs: &Box<Slots<u64>>) -> r: u64 writes(regs.inner) contract {",
        );
    let module = emit(source.as_bytes());
    assert_split(&module, "wf_content", 4);
    assert!(
        !some_part_reloads_a_box(&module, "wf_content", 4),
        "no part reloads a box a content writer cannot replace: {module}"
    );
    let handed = (0..4).any(|arm| {
        let part = definition(&module, &format!("wf_content.arm.{arm}"));
        part.contains("= alloca ptr")
            && part
                .lines()
                .any(|line| line.contains("@wf_touch_content(ptr %wf.pin."))
    });
    assert!(handed, "a part hands the callee its pin slot: {module}");
    let output = compile_and_run(&module);
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn rarely_read_values_spill_before_a_box_read_through_projections_and_a_pin() {
    // As in thirty_values, thirty invariant values exceed every host's
    // argument registers. Here they are box projections that Add, Jnz and
    // Halt read (Add and Jnz add zero with them); regs is read by the same
    // three arms through their own projections and by Dec only through its
    // call's pin. Its first projection has the lower ID, so without the pin
    // read regs ties with them at three arms and spills first; counting the
    // pin gives it four, and the frame keeps the rare projections.
    let names: Vec<String> = (0..30).map(|index| format!("rare{index}")).collect();
    let parameters = names
        .iter()
        .map(|name| format!("{name}: &Box<Slots<u64>>"))
        .collect::<Vec<_>>()
        .join(", ");
    let forwarded = names
        .iter()
        .map(|name| format!("{name}: {name}"))
        .collect::<Vec<_>>()
        .join(", ");
    let initial = names
        .iter()
        .map(|name| format!("{name}: &rare"))
        .collect::<Vec<_>>()
        .join(", ");
    let effects = names
        .iter()
        .map(|name| format!("reads({name})"))
        .collect::<Vec<_>>()
        .join(", ");
    let reads = names
        .iter()
        .map(|name| format!("      set a = a +wrap {name}^.inner.len;\n"))
        .collect::<String>();
    let zero_reads = |arm: &str, sum: &str| {
        names
            .iter()
            .map(|name| {
                format!(
                    "      let {name}_{arm} = {name}^.inner.len -wrap 1_u64;\n      set {sum} = {sum} +wrap {name}_{arm};\n"
                )
            })
            .collect::<String>()
    };
    let source = REGISTER_FILE
        .replace("NAME", "pressure")
        .replace("DECREMENT", "let ignored = touch_content(regs: regs);")
        .replace(
            "fn touch(regs: &Box<Slots<u64>>) -> r: u64 writes(regs) contract {",
            "fn touch_content(regs: &Box<Slots<u64>>) -> r: u64 writes(regs.inner) contract {",
        )
        .replace("pc: u64) ->", &format!("pc: u64, {parameters}) ->"))
        .replace("reads(code), writes(regs)", &format!("reads(code), {effects}, writes(regs)"))
        .replace("regs: regs, pc: next)", &format!("regs: regs, pc: next, {forwarded})"))
        .replace("regs: &regs, pc: 0_u64)", &format!("regs: &regs, pc: 0_u64, {initial})"))
        .replace("      return a;", &format!("{reads}      return a;"))
        .replace(
            "      let b = a +wrap 3_u64;\n",
            &format!("      let b = a +wrap 3_u64;\n{}", zero_reads("add", "b")),
        )
        .replace(
            "      let c = regs^.inner[1_u64];\n      let next = pc + 1_u64;\n",
            &format!(
                "      let c = regs^.inner[1_u64];\n{}      let next = pc + 1_u64;\n",
                zero_reads("jnz", "c")
            ),
        )
        .replace("r == 3000_u64", "r == 3030_u64")
        .replace(
            "  if code.inner.len > 0_u64 {",
            "  let rare = box_slots_new::<u64>(capacity: 1_u64);\n  if rare.inner.len < rare.inner.cap {\n    place_back(window: &rare.inner, value: 0_u64);\n  }\n  if code.inner.len > 0_u64 {",
        );
    let module = emit(source.as_bytes());
    assert_split(&module, "wf_pressure", 4);
    let dispatch = definition(&module, "wf_pressure.dispatch");
    let pinned = dispatch
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("store ptr ")
                .and_then(|line| line.split_once(", ptr %wf.pin."))
                .map(|(value, _)| value)
        })
        .expect("each part's pin holds the regs projection");
    let enclosing = definition(&module, "wf_pressure");
    let spills: Vec<&str> = enclosing
        .lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("store ptr ")
                .and_then(|line| line.split_once(", ptr %wf.slot."))
                .map(|(value, _)| value)
        })
        .collect();
    let (_, registers) = host_convention();
    assert!(
        spills.len() >= names.len() - registers,
        "the rarely read projections exceed the registers and wait in the frame: {enclosing}"
    );
    assert!(
        !spills.contains(&pinned),
        "the box read by all four arms keeps a register ahead of the rare projections: {enclosing}"
    );
    let output = compile_and_run(&module);
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn a_read_only_reference_handed_to_a_reader_keeps_its_box() {
    // `code` is read-only, so the header's projection of its box is hoisted
    // by the read-only rule; an arm that also hands `code` to a reader keeps
    // it pinned, and that arm hands the reader a pin slot.
    let source = REGISTER_FILE
        .replace("NAME", "reader")
        .replace(
            "DECREMENT",
            "let seen = peek(code: code);\n      let c = regs^.inner[1_u64];\n      let d = c -wrap 1_u64;\n      set regs^.inner[1_u64] = d;",
        )
        .replace(
            "fn touch(regs: &Box<Slots<u64>>) -> r: u64 writes(regs) contract {",
            "fn peek(code: &Box<Slots<Op>>) -> r: u64 reads(code) {\n  return code^.inner.len;\n}\n\nfn touch(regs: &Box<Slots<u64>>) -> r: u64 writes(regs) contract {",
        );
    let module = emit(source.as_bytes());
    assert_split(&module, "wf_reader", 4);
    assert!(
        !some_part_reloads_a_box(&module, "wf_reader", 4),
        "no part reloads a box a reader cannot replace: {module}"
    );
    let handed = (0..4).any(|arm| {
        definition(&module, &format!("wf_reader.arm.{arm}"))
            .lines()
            .any(|line| line.contains("@wf_peek(ptr %wf.pin."))
    });
    assert!(handed, "the arm hands the reader its pin slot: {module}");
    let output = compile_and_run(&module);
    assert!(output.status.success(), "{output:?}");
}

/// An interpreter whose `Dec` arm replaces the box `regs` holds, returning
/// its result through memory and carrying twenty-four values it never
/// changes: `regs` cannot be kept, past the registers the unchanged values
/// wait in the frame, and the result is 404000.
const REPLACING: &str = r#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

enum Op {
  Add(k: u64);
  Dec();
  Jnz(t: u64);
  Halt();
}

enum Outcome {
  Done(value: u64);
  Failed();
}

fn run(code: &Box<Slots<Op>>, regs: &Box<Slots<u64>>, pc: u64, acc: u64, count: u64, e0: u64, e1: u64, e2: u64, e3: u64, e4: u64, e5: u64, e6: u64, e7: u64, e8: u64, e9: u64, e10: u64, e11: u64, e12: u64, e13: u64, e14: u64, e15: u64, e16: u64, e17: u64, e18: u64, e19: u64, e20: u64, e21: u64, e22: u64, e23: u64) -> r: Outcome reads(code), writes(regs) contract {
  requires pc < code^.inner.len;
  requires 1_u64 <= regs^.inner.len;
} {
  let n = code^.inner.len;
  match code^.inner[pc] {
    Add(k: kv) => {
      let next = pc + 1_u64;
      let t1 = acc +wrap kv^;
      let a0 = t1 +wrap e0;
      let a1 = a0 +wrap e2;
      let a2 = a1 +wrap e4;
      let a3 = a2 +wrap e6;
      let a4 = a3 +wrap e8;
      let a5 = a4 +wrap e10;
      let a6 = a5 +wrap e12;
      let a7 = a6 +wrap e14;
      let a8 = a7 +wrap e16;
      let a9 = a8 +wrap e18;
      let a10 = a9 +wrap e20;
      let a11 = a10 +wrap e22;
      let cur = regs^.inner[0_u64];
      let upd = cur +wrap 1_u64;
      set regs^.inner[0_u64] = upd;
      if next < n {
        return musttail run(code: code, regs: regs, pc: next, acc: a11, count: count, e0: e0, e1: e1, e2: e2, e3: e3, e4: e4, e5: e5, e6: e6, e7: e7, e8: e8, e9: e9, e10: e10, e11: e11, e12: e12, e13: e13, e14: e14, e15: e15, e16: e16, e17: e17, e18: e18, e19: e19, e20: e20, e21: e21, e22: e22, e23: e23);
      }
      return Outcome::Failed();
    }
    Dec() => {
      let next = pc + 1_u64;
      let left = count -wrap 1_u64;
      let old0 = regs^.inner[0_u64];
      let bumped = old0 +wrap 100_u64;
      let fresh = box_slots_new::<u64>(capacity: 1_u64);
      place_back(window: &fresh.inner, value: bumped);
      set regs^ = move fresh;
      let d0 = acc +wrap e1;
      let d1 = d0 +wrap e3;
      let d2 = d1 +wrap e5;
      let d3 = d2 +wrap e7;
      let d4 = d3 +wrap e9;
      let d5 = d4 +wrap e11;
      let d6 = d5 +wrap e13;
      let d7 = d6 +wrap e15;
      let d8 = d7 +wrap e17;
      let d9 = d8 +wrap e19;
      let d10 = d9 +wrap e21;
      let d11 = d10 +wrap e23;
      if next < n {
        return musttail run(code: code, regs: regs, pc: next, acc: d11, count: left, e0: e0, e1: e1, e2: e2, e3: e3, e4: e4, e5: e5, e6: e6, e7: e7, e8: e8, e9: e9, e10: e10, e11: e11, e12: e12, e13: e13, e14: e14, e15: e15, e16: e16, e17: e17, e18: e18, e19: e19, e20: e20, e21: e21, e22: e22, e23: e23);
      }
      return Outcome::Failed();
    }
    Jnz(t: tv) => {
      let next = pc + 1_u64;
      if count != 0_u64 {
        set next = tv^;
      }
      if next < n {
        return musttail run(code: code, regs: regs, pc: next, acc: acc, count: count, e0: e0, e1: e1, e2: e2, e3: e3, e4: e4, e5: e5, e6: e6, e7: e7, e8: e8, e9: e9, e10: e10, e11: e11, e12: e12, e13: e13, e14: e14, e15: e15, e16: e16, e17: e17, e18: e18, e19: e19, e20: e20, e21: e21, e22: e22, e23: e23);
      }
      return Outcome::Failed();
    }
    Halt() => {
      let r0 = regs^.inner[0_u64];
      let tot = acc +wrap r0;
      let done = Outcome::Done(value: tot);
      return done;
    }
  }
}

fn push(code: &Box<Slots<Op>>, op: Op) -> ok: Bool writes(code) {
  if code^.inner.len < code^.inner.cap {
    place_back(window: &code^.inner, value: op);
    return True();
  }
  return False();
}

fn main() -> status: ExitStatus pure {
  let code = box_slots_new::<Op>(capacity: 4_u64);
  let c0 = Op::Add(k: 3_u64);
  let p0 = push(code: &code, op: c0);
  let c1 = Op::Dec();
  let p1 = push(code: &code, op: c1);
  let c2 = Op::Jnz(t: 0_u64);
  let p2 = push(code: &code, op: c2);
  let c3 = Op::Halt();
  let p3 = push(code: &code, op: c3);
  let regs = box_slots_new::<u64>(capacity: 1_u64);
  if regs.inner.len < regs.inner.cap {
    place_back(window: &regs.inner, value: 0_u64);
  }
  if code.inner.len > 0_u64 {
    if regs.inner.len >= 1_u64 {
      let r = run(code: &code, regs: &regs, pc: 0_u64, acc: 0_u64, count: 1000_u64, e0: 1_u64, e1: 2_u64, e2: 3_u64, e3: 4_u64, e4: 5_u64, e5: 6_u64, e6: 7_u64, e7: 8_u64, e8: 9_u64, e9: 10_u64, e10: 11_u64, e11: 12_u64, e12: 13_u64, e13: 14_u64, e14: 15_u64, e15: 16_u64, e16: 17_u64, e17: 18_u64, e18: 19_u64, e19: 20_u64, e20: 21_u64, e21: 22_u64, e22: 23_u64, e23: 24_u64);
      match r {
        Done(value: v) => {
          if v == 404000_u64 {
            return exit_status(code: 0_u8);
          }
          return exit_status(code: 1_u8);
        }
        Failed() => {
          return exit_status(code: 3_u8);
        }
      }
    }
  }
  return exit_status(code: 2_u8);
}
"#;

#[test]
fn a_box_replaced_in_the_loop_is_reloaded_while_unchanged_values_wait_in_the_frame() {
    let module = emit(REPLACING.as_bytes());
    let base = if module.contains(" @wf_run.body(") {
        "wf_run.body"
    } else {
        "wf_run"
    };
    let (convention, _) = host_convention();
    let verdict = verdict(&module, base);
    assert!(
        convention.is_empty() || verdict.starts_with("split"),
        "{verdict}"
    );
    if verdict.starts_with("split") {
        assert_split(&module, base, 4);
        assert!(
            module.contains(&format!("{}{base}: keeps ", crate::DISPATCH_LEDGER_PREFIX)),
            "the unchanged values wait in the frame: {module}"
        );
        assert!(
            some_part_reloads_a_box(&module, base, 4),
            "a part reloads the box an arm replaces: {module}"
        );
    }
    let output = compile_and_run(&module);
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn a_hoisted_projection_an_arm_repeats_is_passed_once() {
    // The header projects `code`'s box for the match and every arm projects
    // it again to read the length: the arms name the hoisted projection,
    // which each part receives once.
    let source = REGISTER_FILE
        .replace("NAME", "kept")
        .replace(
            "DECREMENT",
            "let c = regs^.inner[1_u64];\n      let d = c -wrap 1_u64;\n      set regs^.inner[1_u64] = d;",
        )
        .replace("  let n = code^.inner.len;\n  match", "  match")
        .replace("      let next = pc + 1_u64;", "      let n = code^.inner.len;\n      let next = pc + 1_u64;");
    let module = emit(source.as_bytes());
    assert_split(&module, "wf_kept", 4);
    let dispatch = definition(&module, "wf_kept.dispatch");
    let header = dispatch.lines().next().expect("a definition header");
    let mut names: Vec<&str> = header
        .split(", ")
        .filter_map(|parameter| parameter.rsplit(' ').next())
        .collect();
    let count = names.len();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), count, "no part parameter repeats: {header}");
    assert!(
        !some_part_reloads_a_box(&module, "wf_kept", 4),
        "no arm reloads the box the header projects: {module}"
    );
    let output = compile_and_run(&module);
    assert!(output.status.success(), "{output:?}");
}
