//! compiler/match-dispatch-lowering: a loop whose header ends in a `match`
//! is emitted as one function per arm, chained by guaranteed tail calls.

use super::{compile_and_run, emit, emitted_body};

/// A four-instruction interpreter written as a `loop` whose body is one
/// `match`, each looping arm ending in `continue`:
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

fn run(code: &Box<Slots<Op>>, start: u64, seed: u64, steps: u64) -> r: RESULT reads(code) contract {
  requires start < code^.inner.len;
} {
  let n = code^.inner.len;
  let pc = start;
  let acc = seed;
  let count = steps;
  loop (
    invariant code_bound: pc < n
  ) {
    match code^.inner[pc] {
      Add(k: kv) => {
        let next = pc + 1_u64;
        let sum = acc +wrap kv^;
        if next < n {
          set pc = next;
          set acc = sum;
          continue;
        }
        return FAILED;
      }
      Dec() => {
        let next = pc + 1_u64;
        let left = count -wrap 1_u64;
        if next < n {
          set pc = next;
          set count = left;
          continue;
        }
        return FAILED;
      }
      Jnz(t: tv) => {
        let next = pc + 1_u64;
        if count != 0_u64 {
          set next = tv^;
        }
        if next < n {
          set pc = next;
          continue;
        }
        return FAILED;
      }
      Halt() => {
        let done = DONE;
        return done;
      }
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
    let r = run(code: &code, start: 0_u64, seed: 0_u64, steps: 1000_u64);
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
    let word = module.contains(&format!("{base}: dispatches through the handler word"));
    if word {
        assert!(dispatch.contains("load ptr, ptr "), "{dispatch}");
        assert!(!dispatch.contains("x ptr], ptr "), "{dispatch}");
        assert!(!module.contains(&format!("@{base}.dispatch.table")), "{module}");
    } else {
        // Keep the original table checks for every unthreaded split.
        let through_parameter = dispatch.contains("x ptr], ptr %wf.dispatch.base");
        assert!(
            through_parameter || dispatch.contains(&format!("x ptr], ptr @{base}.dispatch.table")),
            "the header transfers through the handler table: {dispatch}"
        );
        assert!(!through_parameter || module.contains(&format!("ptr @{base}.dispatch.table")));
        let table = module.lines()
            .find(|line| line.starts_with(&format!("@{base}.dispatch.table = ")))
            .expect("the handler table is emitted");
        assert!(table.contains(&format!("[{arms} x ptr]"))
            && (0..arms).all(|arm| table.contains(&format!("ptr @{base}.arm.{arm}"))), "{table}");
    }
    assert!(dispatch.contains(&format!("musttail call {convention}")), "{dispatch}");
    assert!(!dispatch.contains("switch "), "{dispatch}");
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
        // Seven parameters: pc, acc, count, the hoisted code length and
        // box referent (the run), the cell's address and the frame. Jnz's
        // joined next index is not a known step,
        // so its edge needs the run to form the next cell's address.
        assert!(
            verdict.starts_with(
                "split: the loop over Op into 4 arms, taking 7 integer and 0 floating"
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
fn split_dispatch_keeps_one_struct_and_passes_its_pointer_to_every_part() {
    let module = emit(
        br#"enum Step {
  Again(left: u64);
  Done();
}

fn run(first: Step) -> result: u64 pure {
  let step = first;
  loop @steps {
    match step {
      Again(left: n) => {
        let left = n;
        if left == 0_u64 {
          return 0_u64;
        }
        let next = left -wrap 1_u64;
        set step = Step::Again(left: next);
        continue;
      }
      Done() => {
        break @steps;
      }
    }
  }
  return 1_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let first = Step::Again(left: 2_u64);
  let result = run(first: first);
  if result == 0_u64 {
    return std::process::exit_status(code: 0_u8);
  }
  return std::process::exit_status(code: 1_u8);
}
"#,
    );
    assert_split(&module, "wf_run", 2);
    let enclosing = emitted_body(&module, "run");
    assert_eq!(enclosing.matches("%wf.frame = alloca {").count(), 1);
    assert_eq!(enclosing.matches(" = alloca ").count(), 1, "{enclosing}");
    assert!(enclosing.contains(", ptr %wf.frame, i32 0, i32 "));
    let call = enclosing
        .lines()
        .find(|line| line.contains("@wf_run.dispatch("))
        .expect("the enclosing function calls dispatch");
    assert!(call.contains("ptr %wf.frame"), "{call}");
    for symbol in ["wf_run.dispatch", "wf_run.arm.0", "wf_run.arm.1"] {
        let part = definition(&module, symbol);
        let signature = part.lines().next().expect("a part signature");
        assert_eq!(signature.matches("ptr %wf.frame").count(), 1, "{part}");
        assert!(!part.contains("%wf.frame = alloca"), "{part}");
    }
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

fn run(code: &Box<Slots<Op>>, start: u64, seed: u64, steps: u64) -> r: u64 reads(code) contract {
  requires start < code^.inner.len;
} {
  let n = code^.inner.len;
  let pc = start;
  let acc = seed;
  let count = steps;
  loop (
    invariant code_bound: pc < n
  ) {
    match code^.inner[pc] {
      Add(k: kv) => {
        let next = pc + 1_u64;
        let sum = acc +wrap kv^;
        if next < n {
          set pc = next;
          set acc = sum;
          continue;
        }
        return 0_u64;
      }
      Jump(t: tv) => {
        let target = tv^;
        if target < n {
          set pc = target;
          continue;
        }
        return 0_u64;
      }
      Rep(k: kv) => {
        if count != 0_u64 {
          let sum = acc +wrap kv^;
          let left = count -wrap 1_u64;
          set acc = sum;
          set count = left;
          continue;
        }
        let next = pc + 1_u64;
        if next < n {
          set pc = next;
          continue;
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
    let r = run(code: &code, start: 1_u64, seed: 0_u64, steps: 3_u64);
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
    let module = emit(CURSOR_INTERPRETER.as_bytes());
    if verdict(&module, "wf_run").starts_with("split") {
        assert_handler_load(&module, "wf_run", 16, 8);
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
            r#"      Jump(t: tv) => {
        let target = tv^;
        if target < n {
          set pc = target;
          continue;
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

fn run(steps: u64, first: Parity) -> r: u64 pure {
  let n = steps;
  let acc = 0_u64;
  let step = first;
  loop {
    match step {
      Even() => {
        if n == 0_u64 {
          return acc;
        }
        let left = n -wrap 1_u64;
        let sum = acc +wrap 2_u64;
        let next = Parity::Odd();
        set n = left;
        set acc = sum;
        set step = next;
        continue;
      }
      Odd() => {
        if n == 0_u64 {
          return acc;
        }
        let left = n -wrap 1_u64;
        let sum = acc +wrap 3_u64;
        let next = Parity::Even();
        set n = left;
        set acc = sum;
        set step = next;
        continue;
      }
    }
  }
}

fn main() -> status: ExitStatus pure {
  let first = Parity::Even();
  let r = run(steps: 10_u64, first: first);
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
/// only the counter `p0` changes and the rest stay as they entered. Starting
/// from p0 = 7 and the others at 1, it returns the sum of p1 to p29 once p0
/// reaches zero: 232 when they change, 29 when they stay.
fn thirty_values(changing: bool) -> String {
    let names: Vec<String> = (0..30).map(|index| format!("p{index}")).collect();
    let parameters = names
        .iter()
        .map(|name| format!("s{}: u64", &name[1..]))
        .collect::<Vec<_>>()
        .join(", ");
    let bindings = names
        .iter()
        .map(|name| format!("  let {name} = s{};\n", &name[1..]))
        .collect::<String>();
    let steps = if changing {
        names[1..]
            .iter()
            .map(|name| format!("        let n{name} = {name} +wrap 1_u64;\n"))
            .collect::<String>()
    } else {
        String::new()
    };
    let sum = names[1..]
        .iter()
        .enumerate()
        .map(|(index, name)| {
            if index == 0 {
                format!("          let t1 = {name};\n")
            } else {
                format!("          let t{} = t{index} +wrap {name};\n", index + 1)
            }
        })
        .collect::<String>();
    let updates = names
        .iter()
        .filter_map(|name| {
            if name == "p0" {
                Some("        set p0 = left;\n".to_owned())
            } else if changing {
                Some(format!("        set {name} = n{name};\n"))
            } else {
                None
            }
        })
        .collect::<String>();
    let initial = names
        .iter()
        .map(|name| {
            if name == "p0" {
                "s0: 7_u64".to_owned()
            } else {
                format!("s{}: 1_u64", &name[1..])
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let expected = if changing { 232 } else { 29 };
    format!(
        r#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

enum Parity {{
  Even();
  Odd();
}}

fn run({parameters}, first: Parity) -> r: u64 pure {{
{bindings}  let step = first;
  loop {{
    match step {{
      Even() => {{
        if p0 == 0_u64 {{
{sum}          return t29;
        }}
        let left = p0 -wrap 1_u64;
{steps}        let next = Parity::Odd();
{updates}        set step = next;
        continue;
      }}
      Odd() => {{
        if p0 == 0_u64 {{
{sum}          return t29;
        }}
        let left = p0 -wrap 1_u64;
{steps}        let next = Parity::Even();
{updates}        set step = next;
        continue;
      }}
    }}
  }}
}}

fn main() -> status: ExitStatus pure {{
  let first = Parity::Even();
  let r = run({initial}, first: first);
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
    // Twenty-nine of the thirty values stay unchanged and are read only when
    // the loop ends, so they can wait in the frame and the loop still splits.
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

fn NAME(code: &Box<Slots<Op>>, regs: &Box<Slots<u64>>, start: u64) -> r: u64 reads(code), writes(regs) contract {
  requires start < code^.inner.len;
  requires 2_u64 <= regs^.inner.len;
} {
  let n = code^.inner.len;
  let pc = start;
  loop (
    invariant code_bound: pc < code^.inner.len,
    invariant regs_bound: 2_u64 <= regs^.inner.len
  ) {
    match code^.inner[pc] {
      Add() => {
        let a = regs^.inner[0_u64];
        let b = a +wrap 3_u64;
        set regs^.inner[0_u64] = b;
        let next = pc + 1_u64;
        if next < n {
          set pc = next;
          continue;
        }
        return 0_u64;
      }
      Dec() => {
        DECREMENT
        let next = pc + 1_u64;
        if next < n {
          set pc = next;
          continue;
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
          set pc = next;
          continue;
        }
        return 0_u64;
      }
      Halt() => {
        let a = regs^.inner[0_u64];
        return a;
      }
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
      let r = NAME(code: &code, regs: &regs, start: 0_u64);
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
        "let c = regs^.inner[1_u64];\n        let d = c -wrap 1_u64;\n        set regs^.inner[1_u64] = d;",
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
        .map(|name| format!("        set a = a +wrap {name}^.inner.len;\n"))
        .collect::<String>();
    let zero_reads = |arm: &str, sum: &str| {
        names
            .iter()
            .map(|name| {
                format!(
                    "        let {name}_{arm} = {name}^.inner.len -wrap 1_u64;\n        set {sum} = {sum} +wrap {name}_{arm};\n"
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
        .replace("start: u64) ->", &format!("start: u64, {parameters}) ->"))
        .replace("reads(code), writes(regs)", &format!("reads(code), {effects}, writes(regs)"))
        .replace("regs: &regs, start: 0_u64)", &format!("regs: &regs, start: 0_u64, {initial})"))
        .replace("        return a;", &format!("{reads}        return a;"))
        .replace(
            "        let b = a +wrap 3_u64;\n",
            &format!("        let b = a +wrap 3_u64;\n{}", zero_reads("add", "b")),
        )
        .replace(
            "        let c = regs^.inner[1_u64];\n        let next = pc + 1_u64;\n",
            &format!(
                "        let c = regs^.inner[1_u64];\n{}        let next = pc + 1_u64;\n",
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
            "let seen = peek(code: code);\n        let c = regs^.inner[1_u64];\n        let d = c -wrap 1_u64;\n        set regs^.inner[1_u64] = d;",
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

fn run(code: &Box<Slots<Op>>, regs: &Box<Slots<u64>>, start: u64, seed: u64, steps: u64, e0: u64, e1: u64, e2: u64, e3: u64, e4: u64, e5: u64, e6: u64, e7: u64, e8: u64, e9: u64, e10: u64, e11: u64, e12: u64, e13: u64, e14: u64, e15: u64, e16: u64, e17: u64, e18: u64, e19: u64, e20: u64, e21: u64, e22: u64, e23: u64) -> r: Outcome reads(code), writes(regs) contract {
  requires start < code^.inner.len;
  requires 1_u64 <= regs^.inner.len;
} {
  let n = code^.inner.len;
  let pc = start;
  let acc = seed;
  let count = steps;
  loop (
    invariant code_bound: pc < n,
    invariant regs_bound: 1_u64 <= regs^.inner.len
  ) {
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
          set pc = next;
          set acc = a11;
          continue;
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
          set pc = next;
          set acc = d11;
          set count = left;
          continue;
        }
        return Outcome::Failed();
      }
      Jnz(t: tv) => {
        let next = pc + 1_u64;
        if count != 0_u64 {
          set next = tv^;
        }
        if next < n {
          set pc = next;
          continue;
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
      let r = run(code: &code, regs: &regs, start: 0_u64, seed: 0_u64, steps: 1000_u64, e0: 1_u64, e1: 2_u64, e2: 3_u64, e3: 4_u64, e4: 5_u64, e5: 6_u64, e6: 7_u64, e7: 8_u64, e8: 9_u64, e9: 10_u64, e10: 11_u64, e11: 12_u64, e12: 13_u64, e13: 14_u64, e14: 15_u64, e15: 16_u64, e16: 17_u64, e17: 18_u64, e18: 19_u64, e19: 20_u64, e20: 21_u64, e21: 22_u64, e22: 23_u64, e23: 24_u64);
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
            "let c = regs^.inner[1_u64];\n        let d = c -wrap 1_u64;\n        set regs^.inner[1_u64] = d;",
        )
        .replace("  let n = code^.inner.len;\n  let pc = start;", "  let pc = start;")
        .replace(
            "        let next = pc + 1_u64;",
            "        let n = code^.inner.len;\n        let next = pc + 1_u64;",
        );
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

/// The explicit next-iteration edge must receive the same split as an
/// implicit backedge. Runtime semantics live in the program test.
#[test]
fn continue_edges_to_a_header_match_split_into_handlers() {
    let source = include_bytes!("../../../../tests/programs/continue_interpreter.wf");
    let module = emit(source);
    let (convention, _) = host_convention();
    if !convention.is_empty() {
        assert!(verdict(&module, "wf_run").starts_with("split:"), "{module}");
        assert_interpreter_split(&module, "wf_run");
    }
}

/// A 12-byte union inside a 28-byte product ceiling: one word makes 20,
/// two words make 28, three cannot fit. Tags deliberately differ from arm
/// order. Copying Add, then replacing that copy with Sub must compute 18.
const HANDLER_CELLS: &[u8] = br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

enum Cell {
  Halt(value: u32, extra: u8);
  Add(value: u32, extra: u8);
  Sub(value: u32, extra: u8);
}

fn run(code: &Box<Slots<Cell>>) -> result: u32 reads(code) contract {
  requires code^.inner.len > 0_u64;
} {
  let n = code^.inner.len;
  let pc = 0_u64;
  let acc = 0_u32;
  loop (
    invariant bound: pc < n
  ) {
    match code^.inner[pc] {
      Add(value: v, extra: e) => {
        let extra = cvt::<u8, u32>(e^);
        let amount = v^ +wrap extra;
        set acc = acc +wrap amount;
        let next = pc + 1_u64;
        if next < n {
          set pc = next;
          continue;
        }
        return 0_u32;
      }
      Sub(value: v, extra: e) => {
        let extra = cvt::<u8, u32>(e^);
        let amount = v^ +wrap extra;
        set acc = acc -wrap amount;
        let next = pc + 1_u64;
        if next < n {
          set pc = next;
          continue;
        }
        return 0_u32;
      }
      Halt(value: v, extra: e) => {
        let extra = cvt::<u8, u32>(e^);
        let amount = v^ +wrap extra;
        return acc +wrap amount;
      }
    }
  }
}

fn make_add() -> result: Cell pure {
  return Cell::Add(value: 7_u32, extra: 2_u8);
}

fn copy_cell(cell: &Cell) -> result: Cell reads(cell) {
  return cell^;
}

fn tag(cell: &Cell) -> result: u32 reads(cell) {
  match cell^ {
    Add(value: v, extra: e) => {
      return 1_u32;
    }
    Sub(value: v, extra: e) => {
      return 2_u32;
    }
    Halt(value: v, extra: e) => {
      return 3_u32;
    }
  }
}

fn push(code: &Box<Slots<Cell>>, cell: Cell) -> result: unit writes(code) {
  if code^.inner.len < code^.inner.cap {
    place_back(window: &code^.inner, value: cell);
  }
  return unit;
}

fn main() -> status: ExitStatus pure {
  let code = box_slots_new::<Cell>(capacity: 3_u64);
  let first = make_add();
  let copied = copy_cell(cell: &first);
  push(code: &code, cell: copied);
  push(code: &code, cell: first);
  let third = Cell::Halt(value: 10_u32, extra: 3_u8);
  push(code: &code, cell: third);
  if code.inner.len == 3_u64 {
    let replacement = Cell::Sub(value: 3_u32, extra: 1_u8);
    set code.inner[1_u64] = replacement;
    let kind = tag(cell: &code.inner[1_u64]);
    let result = run(code: &code);
    if result == 18_u32 {
      if kind == 2_u32 {
        return exit_status(code: 0_u8);
      }
    }
    return exit_status(code: 1_u8);
  }
  return exit_status(code: 2_u8);
}
"#;

/// Follow the loaded target from the cursor parameter through the word GEP
/// to the indirect call. A tag/table implementation cannot satisfy this.
fn assert_handler_load(module: &str, base: &str, offset: u64, align: u64) {
    let dispatch = definition(module, &format!("{base}.dispatch"));
    let (slot, place) = dispatch.lines().find_map(|line| {
        let (slot, gep) = line.trim().split_once(" = getelementptr inbounds i8, ptr ")?;
        let place = gep.strip_suffix(&format!(", i64 {offset}"))?;
        Some((slot, place))
    }).expect("the handler word is addressed in the received element");
    let handler = dispatch.lines().find_map(|line| {
        line.trim().strip_suffix(&format!(" = load ptr, ptr {slot}, align {align}"))
    }).expect("the target is loaded with the word's alignment");
    assert!(dispatch.lines().next().unwrap().contains(&format!("ptr {place}")), "{dispatch}");
    assert!(dispatch.lines().any(|line| line.contains("musttail call ")
        && line.contains(&format!(" {handler}("))), "{dispatch}");
    assert!(!dispatch.contains("load i32"), "{dispatch}");
    assert!(!dispatch.contains("x ptr]"), "{dispatch}");
    assert!(!module.contains(&format!("@{base}.dispatch.table")), "{module}");
    assert!(module.contains(&format!("{base}: dispatches through the handler word")), "{module}");
}

#[test]
fn handler_words_preserve_copies_replacements_tags_and_four_byte_alignment() {
    let (ty, mut probes) = super::system::with_ir(HANDLER_CELLS, |program| {
        let target = crate::target::TargetLayout::host().expect("host target");
        let prepared = crate::backend::emitter::prepare_dispatch_layout(program, target, false)
            .expect("composition plan");
        let selected = &prepared.program;
        let cell = selected.nominals().iter().find(|n| n.name() == "Cell").unwrap();
        let layout = crate::target::union_enum_layout(target, selected, cell.id()).unwrap();
        assert_eq!((layout.size(), layout.handler_offset(), layout.handler_alignment()),
            (20, Some(12), Some(4)));
        (format!("wf.t.{}", cell.link_name()), super::payload_enums::enum_layout_probes(selected))
    });
    let module = emit(HANDLER_CELLS);
    assert_handler_load(&module, "wf_run", 12, 4);
    let made = emitted_body(&module, "make_add");
    assert!(made.contains("store ptr @wf_run.arm.0,") && made.contains(", align 4"), "{made}");
    let copied = emitted_body(&module, "copy_cell");
    assert!(copied.contains("call void @llvm.memmove.")
        && copied.contains(&format!("getelementptr (%{ty}, ptr null, i32 1)")), "{copied}");
    let main = emitted_body(&module, "main");
    assert!(main.contains("store ptr @wf_run.arm.1,") && main.contains("store ptr @wf_run.arm.2,"), "{main}");
    let tag = emitted_body(&module, "tag");
    assert!(tag.contains("load i32") && tag.contains("switch i32"), "{tag}");
    assert!(!tag.contains("load ptr"), "an ordinary match reads only the tag: {tag}");
    assert!(module.contains(&format!(
        "%{ty} = type {{ i32, [8 x i8], [8 x i8], [0 x i8], [0 x %{ty}.v0] }}"
    )), "{module}");
    // Fixed layout constants plus LLVM DataLayout and the program's result
    // distinguish a coherent wrong layout from the approved representation.
    probes.insert(ty.clone(), (20, 4));
    for tag in 0..3 {
        probes.insert(format!("{ty}.v{tag}"), (12, 4));
    }
    super::payload_enums::assert_llvm_layouts(&module, probes);
}

#[test]
fn each_dispatch_family_gets_a_word_only_when_all_families_fit() {
    super::system::with_ir(HANDLER_CELLS, |program| {
        let target = crate::target::TargetLayout::host().unwrap();
        let owner = program.functions().iter().position(|f| f.name() == "run").unwrap();
        let cell = program.nominals().iter().find(|n| n.name() == "Cell").unwrap().id();
        let mut two = program.clone();
        let mut other = two.functions[owner].clone();
        other.name = "other_run".to_owned();
        two.functions.push(other.clone());
        let prepared = crate::backend::emitter::prepare_dispatch_layout(&two, target, false).unwrap();
        let layout = crate::target::union_enum_layout(target, &prepared.program, cell).unwrap();
        assert_eq!((layout.size(), layout.handler_offset()), (28, Some(12)));
        let module = crate::backend::emitter::emit_prepared_llvm(&prepared, target).unwrap();
        assert_handler_load(&module, "wf_run", 12, 4);
        assert_handler_load(&module, "wf_other_run", 20, 4);
        let made = emitted_body(&module, "make_add");
        assert!(made.contains("store ptr @wf_run.arm.0,")
            && made.contains("store ptr @wf_other_run.arm.0,"), "{made}");
        let mut three = two;
        other.name = "third_run".to_owned();
        three.functions.push(other);
        let prepared = crate::backend::emitter::prepare_dispatch_layout(&three, target, false).unwrap();
        assert_eq!(prepared.program.nominal(cell).unwrap().handler_words, 0);
        let module = crate::backend::emitter::emit_prepared_llvm(&prepared, target).unwrap();
        for base in ["wf_run", "wf_other_run", "wf_third_run"] {
            assert_split(&module, base, 3);
            assert!(module.contains(&format!("@{base}.dispatch.table = ")), "{module}");
        }
        assert!(!module.contains("store ptr @wf_run.arm."), "{module}");
    });
}

#[test]
fn unsplit_and_native_owners_leave_enums_unthreaded() {
    super::system::with_ir(HANDLER_CELLS, |program| {
        let target = crate::target::TargetLayout::host().unwrap();
        let owner = program.functions().iter().position(|f| f.name() == "run").unwrap();
        let cell = program.nominals().iter().find(|n| n.name() == "Cell").unwrap().id();
        let mut unsplit = program.clone();
        unsplit.functions[owner].synthesis = Some(crate::IrSynthesis::Chunk);
        let prepared = crate::backend::emitter::prepare_dispatch_layout(&unsplit, target, false).unwrap();
        assert_eq!(prepared.program.nominal(cell).unwrap().handler_words, 0);
        let module = crate::backend::emitter::emit_prepared_llvm(&prepared, target).unwrap();
        assert!(verdict(&module, "wf_run").contains("compiler-synthesized"), "{module}");
        assert!(!module.contains("store ptr @wf_run.arm."), "{module}");
        let mut foreign = program.clone();
        let mut native = foreign.functions[owner].clone();
        native.name = "native_run".to_owned();
        native.blocks.clear();
        foreign.functions.push(native);
        let prepared = crate::backend::emitter::prepare_dispatch_layout(&foreign, target, false).unwrap();
        assert_eq!(prepared.program.nominal(cell).unwrap().handler_words, 0);
    });
}

#[test]
fn handler_words_use_body_symbols_and_fragment_mode_keeps_tables() {
    let source = enum_interpreter();
    let inputs = [crate::SourceInput::new("handler-bodies.wf", source.as_bytes())];
    let compile = |fragments| crate::compile_for_emission(&inputs,
        crate::CompilerLimits::default(), crate::OverlapLowering::Off, None, fragments).unwrap().0;
    let whole = compile(false);
    let fragments = compile(true);
    // The destination wrapper is for Outcome; Op's selected representation
    // must not make its constructor guess the public wrapper's arm symbols.
    if verdict(&whole, "wf_run.body").starts_with("split") {
        assert!(whole.contains("store ptr @wf_run.body.arm."), "{whole}");
        assert!(!whole.contains("store ptr @wf_run.arm."), "{whole}");
        for module in [&whole, &crate::LlvmModule::decode(&whole.encode()).unwrap()] {
            assert!(crate::split_module(module, crate::FragmentGranularity::Function).is_err());
        }
    } else {
        assert!(!whole.contains("dispatches through the handler word"));
    }
    assert!(!fragments.contains("dispatches through the handler word"));
    assert!(!fragments.contains("store ptr @wf_run.body.arm."));
    if verdict(&fragments, "wf_run.body").starts_with("split") {
        assert_split(&fragments, "wf_run.body", 4);
    }
    for granularity in [crate::FragmentGranularity::Function, crate::FragmentGranularity::Module] {
        crate::split_module(&fragments, granularity).expect("ordinary layouts remain splittable");
    }
}

#[test]
fn nested_enum_words_use_the_language_ceiling_not_the_smaller_child_layout() {
    let source = br#"enum Inner {
  A(value: u64);
  B(value: u64);
  C(value: u64);
}

enum Outer {
  Left(inner: Inner);
  Right(inner: Inner);
}

fn walk(op: &Outer, count: u64) -> result: u64 reads(op) {
  let left = count;
  loop {
    match op^ {
      Left(inner: v) => {
        if left > 0_u64 {
          set left = left -wrap 1_u64;
          continue;
        }
        return 1_u64;
      }
      Right(inner: v) => {
        return 2_u64;
      }
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    super::system::with_ir(source, |program| {
        let target = crate::target::TargetLayout::host().unwrap();
        let outer = program.nominals().iter().find(|n| n.name() == "Outer").unwrap().id();
        let owner = program.functions().iter().find(|f| f.name() == "walk").unwrap();
        let mut three = program.clone();
        for name in ["second_walk", "third_walk"] {
            let mut other = owner.clone();
            other.name = name.to_owned();
            three.functions.push(other);
        }
        let prepared = crate::backend::emitter::prepare_dispatch_layout(&three, target, false).unwrap();
        // Inner: selected 16, ceiling 32. Outer: selected union 24,
        // selected-child product 40, language ceiling 72. Three words give
        // 48: legal under OP-9, refused by the prototype's 40-byte bound.
        let layout = crate::target::union_enum_layout(target, &prepared.program, outer).unwrap();
        assert_eq!((layout.size(), layout.handler_offset()), (48, Some(24)));
        assert_eq!(prepared.program.nominal(outer).unwrap().handler_words, 3);
        assert_eq!(prepared.program.nominal_ceilings[outer.index()].size,
            crate::IrLayoutMagnitude::Finite(72));
        let module = crate::backend::emitter::emit_prepared_llvm(&prepared, target).unwrap();
        for base in ["wf_walk", "wf_second_walk", "wf_third_walk"] {
            assert!(module.contains(&format!("{base}: dispatches through the handler word")), "{module}");
            assert!(!module.contains(&format!("@{base}.dispatch.table")), "{module}");
        }
    });
}

/// The result nominal has room for a handler word (union 16 + word 8,
/// product ceiling 24). Both primitive outcomes reach the split helper.
const CHECKED_HANDLER_RESULT: &str = r#"fn handler_result_make(input: INPUT_TYPE) -> result: Result<OUTPUT_TYPE, ERROR_TYPE> pure {
  return CHECKED_EXPRESSION;
}

fn handler_result_run(value: &Result<OUTPUT_TYPE, ERROR_TYPE>, count: u64) -> result: OUTPUT_TYPE reads(value) {
  let remaining = count;
  loop {
    match value^ {
      Ok(value: payload) => {
        if remaining > 0_u64 {
          set remaining = remaining -wrap 1_u64;
          continue;
        }
        return payload^;
      }
      Err(error: problem) => {
        break;
      }
    }
  }
  return 99_OUTPUT_TYPE;
}

fn main() -> status: std::process::ExitStatus pure {
  let success = handler_result_make(input: SUCCESS_INPUT);
  let failure = handler_result_make(input: FAILURE_INPUT);
  let good = handler_result_run(value: &success, count: 2_u64);
  let bad = handler_result_run(value: &failure, count: 2_u64);
  if good == SUCCESS_OUTPUT {
    if bad == 99_OUTPUT_TYPE {
      return std::process::exit_status(code: 0_u8);
    }
  }
  return std::process::exit_status(code: 1_u8);
}
"#;

fn assert_checked_handler_result(
    input: &str,
    output: &str,
    error: &str,
    expression: &str,
    success: &str,
    failure: &str,
    expected: &str,
) {
    let source = CHECKED_HANDLER_RESULT
        .replace("INPUT_TYPE", input)
        .replace("OUTPUT_TYPE", output)
        .replace("ERROR_TYPE", error)
        .replace("CHECKED_EXPRESSION", expression)
        .replace("SUCCESS_INPUT", success)
        .replace("FAILURE_INPUT", failure)
        .replace("SUCCESS_OUTPUT", expected);
    let module = emit(source.as_bytes());
    assert_split(&module, "wf_handler_result_run", 2);
    assert!(
        module.contains("wf_handler_result_run: dispatches through the handler word"),
        "{module}"
    );
    let producer = emitted_body(&module, "handler_result_make");
    for arm in 0..2 {
        assert!(
            producer.contains(&format!("store ptr @wf_handler_result_run.arm.{arm},")),
            "{producer}"
        );
    }
    let output = compile_and_run(&module);
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn checked_integer_results_initialize_handler_words() {
    // Before F1 this fails during emission with InvalidIr, before linking.
    assert_checked_handler_result(
        "u64",
        "u64",
        "Overflow",
        "input +checked 1_u64",
        "6_u64",
        "18446744073709551615_u64",
        "7_u64",
    );
}

#[test]
fn checked_conversion_results_initialize_handler_words() {
    // i64 -> u64 has both outcomes while retaining room for the word.
    // Before F1 the checked-conversion producer fails with InvalidIr.
    assert_checked_handler_result(
        "i64",
        "u64",
        "NarrowError",
        "cvt.checked::<i64, u64>(input)",
        "7_i64",
        "-1_i64",
        "7_u64",
    );
}

#[test]
fn checked_absolute_results_initialize_handler_words() {
    // A different intrinsic producer, formerly another insertvalue path.
    assert_checked_handler_result(
        "i64",
        "i64",
        "Overflow",
        "iabs.checked(input)",
        "-7_i64",
        "-9223372036854775808_i64",
        "7_i64",
    );
}

#[test]
fn checked_division_results_initialize_handler_words() {
    // The successful division remains guarded; both branches construct the
    // same planned layout. Before F1 emission refuses its memory-only result.
    assert_checked_handler_result(
        "i64",
        "i64",
        "DivError",
        "14_i64 /checked input",
        "2_i64",
        "0_i64",
        "7_i64",
    );
}

const RUNTIME_HANDLER_RESULT: &[u8] = br#"const handler_map_key: Array<u8, 1> =[97_u8];

enum HandlerMapInner {
  First(value: u64);
  Second(value: u64);
  Third(value: u64);
}

enum HandlerMapOuter {
  Left(inner: HandlerMapInner);
  Right(inner: HandlerMapInner);
}

enum HandlerIndependent {
  First(value: u64);
  Second(value: u64);
  Third(value: u64);
}

fn handler_map_run(value: &Option<HandlerMapInner>, count: u64) -> result: u64 reads(value) {
  let remaining = count;
  loop {
    match value^ {
      Some(value: payload) => {
        if remaining > 0_u64 {
          set remaining = remaining -wrap 1_u64;
          continue;
        }
        match payload^ {
          First(value: number) => {
            return number^;
          }
          Second(value: number) => {
            return number^;
          }
          Third(value: number) => {
            return number^;
          }
        }
      }
      None() => {
        break;
      }
    }
  }
  return 11_u64;
}

fn handler_outer_run(value: &HandlerMapOuter, count: u64) -> result: u64 reads(value) {
  let remaining = count;
  loop {
    match value^ {
      Left(inner: payload) => {
        if remaining > 0_u64 {
          set remaining = remaining -wrap 1_u64;
          continue;
        }
        return 13_u64;
      }
      Right(inner: payload) => {
        break;
      }
    }
  }
  return 17_u64;
}

fn handler_independent_run(value: &HandlerIndependent, count: u64) -> result: u64 reads(value) {
  let remaining = count;
  loop {
    match value^ {
      First(value: number) => {
        if remaining > 0_u64 {
          set remaining = remaining -wrap 1_u64;
          continue;
        }
        return number^;
      }
      Second(value: number) => {
        return number^;
      }
      Third(value: number) => {
        break;
      }
    }
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let store = shared_map_new::<HandlerMapInner>(capacity: 2_u64);
  let absent = 0_u64;
  let fresh = 0_u64;
  let present = 0_u64;
  let key = &handler_map_key[0_u64..1_u64];
  atomic slot = &store[key] {
    set absent = handler_map_run(value: slot, count: 2_u64);
  }
  atomic slot = &store[key] {
    set fresh = handler_map_run(value: slot, count: 2_u64);
    let content = HandlerMapInner::Second(value: 7_u64);
    set slot^ = Some<HandlerMapInner>(value: content);
  }
  atomic slot = &store[key] {
    set present = handler_map_run(value: slot, count: 2_u64);
  }
  let unrelated = HandlerIndependent::First(value: 19_u64);
  let control = handler_independent_run(value: &unrelated, count: 2_u64);
  let content = HandlerMapInner::Third(value: 23_u64);
  let outer = HandlerMapOuter::Left(inner: content);
  let enclosed = handler_outer_run(value: &outer, count: 2_u64);
  if absent == 11_u64 {
    if fresh == 11_u64 {
      if present == 7_u64 {
        if control == 19_u64 {
          if enclosed == 13_u64 {
            return std::process::exit_status(code: 0_u8);
          }
        }
      }
    }
  }
  return std::process::exit_status(code: 1_u8);
}
"#;

#[test]
fn runtime_map_values_keep_ordinary_layout_without_disabling_unrelated_handlers() {
    // Without F2, Option<HandlerMapInner> receives a word: the assertion fails,
    // and absent/fresh entries would load a null handler from runtime zeros.
    let module = emit(RUNTIME_HANDLER_RESULT);
    for base in ["wf_handler_map_run", "wf_handler_outer_run"] {
        assert_split(&module, base, 2);
        assert!(
            module.contains(&format!("@{base}.dispatch.table = ")),
            "{module}"
        );
        assert!(
            !module.contains(&format!("{base}: dispatches through the handler word")),
            "{module}"
        );
    }
    // A blanket shutdown of handler words in a runtime-using program fails
    // this control. Common primitive leaves must not connect the nominals.
    assert_split(&module, "wf_handler_independent_run", 3);
    assert!(
        module.contains("wf_handler_independent_run: dispatches through the handler word"),
        "{module}"
    );
    let output = compile_and_run(&module);
    assert!(output.status.success(), "{output:?}");
}

/// The same provisional-result selection in a dispatch arm must write into
/// the enclosing call's result destination, not into a part-local temporary.
#[test]
fn selection_in_a_split_arm_passes_the_result_destination_to_its_producer() {
    let source = format!(
        "{}{}",
        super::payload_enums::SELECT_STEP,
        r#"
enum Command {
  Again();
  Select();
}

fn dispatch(command: Command, n: u64) -> result: Step pure {
  loop {
    match command {
      Again() => {
        set command = Command::Select();
        continue;
      }
      Select() => {
        let step = prepare(n: n);
        let final_step = step;
        match step {
          Error() => {
            set final_step = unwind(n: n);
          }
          Jump(..) => {
          }
          Done(..) => {
          }
          Stop() => {
          }
          Budget() => {
          }
        }
        return final_step;
      }
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  for (n in 0_u64..5_u64) {
    let select = Command::Select();
    let direct = dispatch(command: select, n: n);
    let first = valid_step(step: direct, n: n);
    if bnot(first) {
      return std::process::exit_status(code: 1_u8);
    }
    let again = Command::Again();
    let repeated = dispatch(command: again, n: n);
    let second = valid_step(step: repeated, n: n);
    if bnot(second) {
      return std::process::exit_status(code: 2_u8);
    }
  }
  return std::process::exit_status(code: 0_u8);
}
"#
    );
    let module = super::payload_enums::retain_step_producers(&emit(source.as_bytes()));
    assert_split(&module, "wf_dispatch", 2);
    super::payload_enums::assert_step_destination(definition(&module, "wf_dispatch.arm.1"));
    let optimized = super::host_optimized_module(&module);
    super::payload_enums::assert_step_destination(definition(&optimized, "wf_dispatch.arm.1"));
    // The base retains both Step join parameters within this arm, so its raw
    // producer destination and copy assertions fail even if LLVM hides a copy.
    let output = compile_and_run(&module);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(
        output.stdout.is_empty() && output.stderr.is_empty(),
        "{output:?}"
    );
}

#[test]
fn bounded_scan_results_initialize_handler_words() {
    // ScanStep may acquire a handler word just like a source payload enum.
    // Constructing only an SSA { tag, next, bytes } leaves that word absent.
    let source = br#"fn scan_step_run(value: &ScanStep, count: u64) -> result: u64 reads(value) {
  let remaining = count;
  loop {
    match value^ {
      Next(next: cursor) => {
        if remaining > 0_u64 {
          set remaining = remaining -wrap 1_u64;
          continue;
        }
        return cursor^;
      }
      Needs(bytes: required) => {
        return 99_u64;
      }
    }
  }
}

fn main() -> status: std::process::ExitStatus pure waits {
  let store = shared_map_new::<u8>(capacity: 1_u64);
  let keys = key_set_new(capacity: 0_u64);
  let key = array_filled::<u8, 1>(value: 255_u8);
  let code = 1_u8;
  atomic map = &store {
    let name = &key[0_u64..1_u64];
    set map^[name] = Some<u8>(value: 1_u8);
    let needs = map_scan_within::<u8>(map: map, cursor: 0_u64, count: 1_u64, limit: 0_u64, keys: &keys);
    let refused = scan_step_run(value: &needs, count: 2_u64);
    let required = 0_u64;
    match needs {
      Next(next: cursor) => {
      }
      Needs(bytes: bytes) => {
        set required = bytes;
      }
    }
    if refused == 99_u64 {
      if required > 0_u64 {
        let next = map_scan_within::<u8>(map: map, cursor: 0_u64, count: 1_u64, limit: required, keys: &keys);
        let cursor = scan_step_run(value: &next, count: 2_u64);
        if cursor == 0_u64 {
          if keys.len == 1_u64 {
            set code = 0_u8;
          }
        }
      }
    }
  }
  return std::process::exit_status(code: code);
}
"#;
    let module = emit(source);
    assert_split(&module, "wf_scan_step_run", 2);
    assert!(
        module.contains("wf_scan_step_run: dispatches through the handler word"),
        "{module}"
    );
    for arm in 0..2 {
        assert!(
            module.contains(&format!("store ptr @wf_scan_step_run.arm.{arm},")),
            "{module}"
        );
    }
    let output = compile_and_run(&module);
    assert!(output.status.success(), "{output:?}");
}
