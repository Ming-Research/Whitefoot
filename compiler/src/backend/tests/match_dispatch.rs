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

/// The calling convention the build's assembler admits and the integer
/// argument registers it has on this host, as compiler/match-dispatch-lowering
/// records them.
fn host_convention() -> (&'static str, usize) {
    let preserve_none = env!("WHITEFOOT_PRESERVE_NONE") == "1";
    match (preserve_none, cfg!(target_arch = "aarch64"), cfg!(windows)) {
        (true, true, _) => ("preserve_nonecc ", 24),
        (true, false, _) => ("preserve_nonecc ", 12),
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
        // Eight parameters: pc, acc, count, the code length and the box's
        // referent hoisted out of the header, the cell's address, the
        // handler table and the frame.
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
        header
            .split(", ")
            .filter(|parameter| parameter.contains("ptr"))
            .filter_map(|parameter| parameter.rsplit(' ').next())
            .map(|name| name.trim_end_matches(')').trim_end_matches(" {"))
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
