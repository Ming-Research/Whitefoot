use super::{compile, compile_and_run, compile_rejection, emitted_function};

/// A reference delivered by a `value_if` is checked against every path its
/// incoming edges may name, and the guard on it still holds at the call that
/// consumes it.
///
/// The case was retargeted off the returned borrow it used to carry: [REF-3]
/// refuses a returned reference outright, and its own restructuring is
/// "return an index and let the caller form the reference". The successor
/// delivery form is [REF-1]'s - a `let` binder whose initializer is a
/// `value_if` every branch of which delivers a reference is itself a
/// reference variable, and at that join its target is the union of the path
/// sets the branches name, with every check holding for every member.
#[test]
fn delivered_reference_guards_check_every_named_path_across_retained_calls() {
    let source = br#"const alternative: u64 = 9_u64;

fn indexed(value: &u64) -> result: u64 reads(value) contract {
  requires deref(value) < 2_u64;
} {
  let rows = array_filled::<u64, 2>(value: 7_u64);
  let index = deref(value);
  return rows[index];
}

fn forward(value: &u64) -> result: u64 reads(value) {
  let seen = deref(value);
  let chosen = if seen == 0_u64 {
    give &alternative;
  } else {
    give value;
  }
  if deref(chosen) < 2_u64 {
    return indexed(value: chosen);
  } else {
    return 99_u64;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let zero = 0_u64;
  let one = 1_u64;
  let refused = forward(value: &zero);
  let accepted = forward(value: &one);
  if refused != 99_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if accepted != 7_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    for overlap in [super::OverlapLowering::Off, super::OverlapLowering::On] {
        let module = super::emit_lowered(source, overlap)
            .lines()
            .map(|line| {
                if let Some(header) = line
                    .strip_prefix("define ")
                    .and_then(|line| line.strip_suffix(" {"))
                {
                    format!("define {header} noinline {{\n")
                } else {
                    format!("{line}\n")
                }
            })
            .collect::<String>();
        let output = compile_and_run(&module);
        assert_eq!(output.status.code(), Some(0), "{overlap:?}: {output:?}");
        assert!(output.stdout.is_empty(), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
    }
}

const OUTPUT_CAPACITY: &[u8] =
    br#"fn copy_bytes(out: &[u8], source: Box<Slots<u8>>) -> written: u64 writes(out) contract {
  define out_length = deref(out).len;
  define source_length = source.inner.len;
  requires source_length <= out_length;
} {
  let length = source.inner.len;
  for (offset in 0_u64..length) {
    let value = source.inner[offset];
    set deref(out)[offset] = value;
  }
  return length;
}

fn main() -> status: std::process::ExitStatus pure {
  let length = 4_u64;
  let output = box_slots_new::<u8>(capacity: length);
  for @clear (
    at in 0_u64..4_u64,
    invariant grown: output.inner.len >= at,
    invariant spare: output.inner.cap + at >= output.inner.len + 4_u64
  ) {
    place_back(window: &output.inner, value: 0_u8);
  }
  let source = box_slots_new::<u8>(capacity: length);
  for @fill (
    at in 0_u64..4_u64,
    invariant grown: source.inner.len >= at,
    invariant spare: source.inner.cap + at >= source.inner.len + 4_u64
  ) {
    place_back(window: &source.inner, value: 7_u8);
  }
  let destination = &output.inner[0_u64..4_u64];
  let capacity = deref(destination).len;
  let held = source.inner.len;
  if held <= capacity {
  } else {
    return std::process::exit_status(code: 5_u8);
  }
  let written = copy_bytes(out: destination, source: move source);
  if written != length {
    return std::process::exit_status(code: 1_u8);
  }
  let trailing = output.inner[3_u64];
  if trailing != 7_u8 {
    return std::process::exit_status(code: 2_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;

#[test]
fn an_ordinary_selected_function_keeps_its_contract_without_a_wrapper_check() {
    // C2 deletes FN-7's command-entry contract refusal. This ordinary source
    // requirement is statically true; the build caller proves it normally.
    let module = compile(
        br#"fn main() -> status: std::process::ExitStatus pure contract {
  requires 0_u64 == 0_u64;
} {
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
    assert!(!emitted_function(&module, "main").contains("icmp"));
    assert!(compile_and_run(&module).status.success());
    // A false requirement does not invalidate an ordinary declaration. The
    // build caller cannot prove it, so no executable entry is supplied.
    let library = compile(
        br#"fn main() -> status: std::process::ExitStatus pure contract {
  requires 0_u64 == 1_u64;
} {
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
    assert!(emitted_function(&library, "main").contains("unreachable"));
    assert!(!library.contains("define i32 @main("));
}

#[test]
fn contradictory_requirements_emit_an_unreachable_body_without_a_trap() {
    let llvm = compile(
        br#"fn impossible(value: i32) -> out: i32 pure contract {
  requires value == 0_i32;
  requires value != 0_i32;
} {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
    let impossible = emitted_function(&llvm, "impossible");
    assert!(impossible.contains("unreachable"));
    assert!(!impossible.contains("call void @wf_trap"));
}

#[test]
fn contract_define_is_symbolic_and_not_emitted_as_runtime_work() {
    let llvm = compile(
        br#"fn identity(value: u8) -> out: u8 pure contract {
  define bits = ipopcount(value);
  requires bits == 0_u32;
  ensures out == value;
} {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  let bits = ipopcount(0_u8);
  if bits == 0_u32 {
    let zero = identity(value: 0_u8);
    return std::process::exit_status(code: zero);
  } else {
    return std::process::exit_status(code: 1_u8);
  }
}
"#,
    );
    let identity = emitted_function(&llvm, "identity");
    assert!(!identity.contains("llvm.ctpop"));
    assert!(!identity.contains("call void @wf_trap"));
}

#[test]
fn contract_define_can_hold_a_float_endpoint_conversion_without_runtime_code() {
    let llvm = compile(
        br#"fn identity(value: u8) -> out: u8 pure contract {
  define converted = cvt::<u8, f32>(value);
  requires feq(converted, 1.0_f32);
  ensures out == value;
} {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  let converted = cvt::<u8, f32>(1_u8);
  if feq(converted, 1.0_f32) {
    let one = identity(value: 1_u8);
    let code = one -wrap 1_u8;
    return std::process::exit_status(code: code);
  } else {
    return std::process::exit_status(code: 2_u8);
  }
}
"#,
    );
    let identity = emitted_function(&llvm, "identity");
    assert!(!identity.contains("uitofp"));
    assert!(!identity.contains("call void @wf_trap"));

    let output = compile_and_run(&llvm);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn ordinary_requirement_is_not_emitted_as_a_callee_prologue() {
    let llvm = compile(
        br#"fn bounded(value: i32) -> out: i32 pure contract {
  requires value >= 0_i32;
  ensures out == value;
} {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  let value = 7_i32;
  let returned = bounded(value: value);
  if returned != 7_i32 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
    let bounded = emitted_function(&llvm, "bounded");
    assert!(!bounded.contains("br i1"));
    assert!(!bounded.contains("call void @wf_trap"));
    assert!(bounded.contains("ret i32"));

    let output = compile_and_run(&llvm);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn a_requirement_must_be_discharged_at_each_ordinary_call() {
    let failure = compile_rejection(
        br#"fn positive(value: i32) -> out: i32 pure contract {
  requires value > 0_i32;
} {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  let unknown = 0_i32;
  let returned = positive(value: unknown);
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
    assert_eq!(failure.rule_id(), Some("FN-8"));
    assert!(failure.detail().contains("positive"));
}

#[test]
fn borrowed_output_capacity_contract_informs_the_body_without_a_callee_prologue() {
    let llvm = compile(OUTPUT_CAPACITY);
    let copy = emitted_function(&llvm, "copy_bytes");
    assert!(copy.contains("switch i1"));
    // The requirement and counted-range binder facts are proof inputs only.
    // They are absent after lowering, and both discharged subscripts emit no
    // additional bounds comparison or runtime proof check. The sole strict
    // comparison is the counted loop's continuation condition.
    assert_eq!(copy.matches("icmp ule i64").count(), 0);
    assert_eq!(copy.matches("icmp ult i64").count(), 1);
    assert_eq!(copy.matches("call void @wf_trap").count(), 0);
    assert!(copy.contains("load i8"));
    assert!(copy.contains("store i8"));
    // One release: the callee receives the cell by value, so the
    // `Box<Slots<u8>>` it was handed is its own affine owner there and the
    // compiler-derived free of that one heap object is on its return edge
    // [STOR-1, STOR-3, LIV-1].
    assert_eq!(copy.matches("call void @free").count(), 1);
    // Retire the blanket absence-of-assume expectation: source.inner[offset]
    // now receives the qualified nonnegative payload-index fact. The
    // source_length <= out_length requirement itself remains erased; it
    // supplies neither a callee prologue nor a separate assumption.
    assert_eq!(copy.matches("call void @llvm.assume(i1 ").count(), 1);
    assert_eq!(copy.matches(".nonnegative = icmp sge i64 ").count(), 1);

    let output = compile_and_run(&llvm);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}
