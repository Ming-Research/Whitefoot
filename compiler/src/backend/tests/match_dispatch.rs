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

fn assert_split(module: &str, base: &str) {
    let dispatch = definition(module, &format!("{base}.dispatch"));
    assert!(
        dispatch.starts_with("define internal preserve_nonecc ")
            && dispatch.contains("alwaysinline"),
        "the dispatch function is internal, inlined and register-preserving: {dispatch}"
    );
    assert!(
        dispatch.contains(&format!("ptr @{base}.dispatch.table"))
            && dispatch.contains("musttail call preserve_nonecc"),
        "the header transfers through the handler table: {dispatch}"
    );
    assert!(
        !dispatch.contains("switch "),
        "the header's match is the table transfer: {dispatch}"
    );
    for arm in 0..4 {
        let arm = definition(module, &format!("{base}.arm.{arm}"));
        assert!(arm.starts_with("define internal preserve_nonecc "), "{arm}");
    }
    assert!(
        !module.contains(&format!("@{base}.arm.4(")),
        "one function per arm"
    );
    for arm in 0..3 {
        let arm = definition(module, &format!("{base}.arm.{arm}"));
        assert!(
            arm.contains("musttail call preserve_nonecc")
                && arm.contains(&format!("@{base}.dispatch(")),
            "every looping arm ends in a guaranteed tail call of the dispatch function: {arm}"
        );
    }
    let halt = definition(module, &format!("{base}.arm.3"));
    assert!(
        !halt.contains("musttail"),
        "the halting arm returns: {halt}"
    );
    let table = module
        .lines()
        .find(|line| line.starts_with(&format!("@{base}.dispatch.table = ")))
        .expect("the handler table is emitted");
    assert!(
        table.contains("[4 x ptr]")
            && (0..4).all(|arm| table.contains(&format!("ptr @{base}.arm.{arm}"))),
        "the table has one entry per tag: {table}"
    );
}

#[test]
fn a_header_match_loop_is_split_into_one_function_per_arm() {
    let module = emit(scalar_interpreter().as_bytes());
    assert_split(&module, "wf_run");
    let enclosing = emitted_body(&module, "run");
    assert!(
        enclosing.contains("call preserve_nonecc i64 @wf_run.dispatch(")
            && !enclosing.contains("musttail"),
        "the enclosing function calls the dispatch function once and returns its result: {enclosing}"
    );
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
    assert_split(&module, base);
    let dispatch = definition(&module, &format!("{base}.dispatch"));
    if base == "wf_run.body" {
        assert!(
            dispatch.contains("(ptr %wf.result, "),
            "a destination-form body passes its destination to every part: {dispatch}"
        );
    }
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
    assert_split(&module, "wf_run");
}
