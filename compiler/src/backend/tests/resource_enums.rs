use super::*;

#[test]
fn source_enum_cleanup_switches_on_the_active_variant() {
    let source = br#"struct PairBuffers {
  left: Box<Slots<u8>>;
  right: Box<Slots<u8>>;
}

enum Owner {
  Empty();
  Full(value: PairBuffers);
  Last();
}

fn make_empty() -> result: Owner pure {
  return Owner::Empty();
}

fn make_last() -> result: Owner pure {
  return Owner::Last();
}

fn inspect(owner: &Owner) -> result: u64 reads(owner) {
  match owner^ {
    Empty() => {
      return 0_u64;
    }
    Full(value: pair) => {
      return pair^.left.inner.len;
    }
    Last() => {
      return 2_u64;
    }
  }
}

fn relay(owner: Owner) -> result: Owner pure {
  return move owner;
}

fn clear(owner: &Owner) -> result: unit writes(owner) {
  set owner^ = make_empty();
  return unit;
}

fn abandon(owner: Owner) -> result: unit pure {
  return unit;
}

fn consume(owner: Owner) -> result: u8 pure {
  match move owner {
    Empty() => {
      return 0_u8;
    }
    Full(value: pair) => {
      let spare = pair.left.inner.len;
      let ok = 0_u64 < spare;
      let byte = if ok {
        give pair.left.inner[0_u64];
      } else {
        give 0_u8;
      }
      return byte;
    }
    Last() => {
      return 2_u8;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let abandoned_left = box_slots_new::<u8>(capacity: 1_u64);
  place_back(window: &abandoned_left.inner, value: 7_u8);
  let abandoned_right = box_slots_new::<u8>(capacity: 1_u64);
  place_back(window: &abandoned_right.inner, value: 9_u8);
  let abandoned_pair = PairBuffers(left: move abandoned_left, right: move abandoned_right);
  let abandoned = Owner::Full(value: move abandoned_pair);
  let empty = make_empty();
  let last = make_last();
  let first_tag = inspect(owner: &empty);
  let middle_tag = inspect(owner: &abandoned);
  let last_tag = inspect(owner: &last);
  if first_tag != 0_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  if middle_tag != 1_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  if last_tag != 2_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  let last_byte = consume(owner: move last);
  if last_byte != 2_u8 {
    return std::process::exit_status(code: 3_u8);
  }
  let consumed_empty = make_empty();
  let empty_byte = consume(owner: move consumed_empty);
  if empty_byte != 0_u8 {
    return std::process::exit_status(code: 4_u8);
  }
  swap(first: &abandoned, second: &empty);
  swap(first: &empty, second: &empty);
  clear(owner: &empty);
  abandon(owner: move abandoned);
  let consumed_left = box_slots_new::<u8>(capacity: 1_u64);
  place_back(window: &consumed_left.inner, value: 11_u8);
  let consumed_right = box_slots_new::<u8>(capacity: 1_u64);
  place_back(window: &consumed_right.inner, value: 13_u8);
  let consumed_pair = PairBuffers(left: move consumed_left, right: move consumed_right);
  let consumed = Owner::Full(value: move consumed_pair);
  set empty = move consumed;
  let carried = relay(owner: move empty);
  let consumed_byte = consume(owner: move carried);
  if consumed_byte != 11_u8 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    // After the ordinary owned match, payload bindings are local; reading them
    // publishes nothing about `owner`. Check both exact-row directions: pure
    // is accepted, the wider row fails. The wider row's rejecting rule moved
    // with v0.60: [EFF-1] roots every `effect_path` at a reference parameter
    // and a by-value parameter has no effect entry at all, so `reads(owner)`
    // over `owner: Owner` is refused at the row itself rather than at
    // [EFF-2]'s both-ways comparison against the exhibited set.
    let excessive = std::str::from_utf8(source).unwrap().replace(
        "fn consume(owner: Owner) -> result: u8 pure",
        "fn consume(owner: Owner) -> result: u8 reads(owner)",
    );
    let failure = compile_rejection(excessive.as_bytes());
    assert_eq!(failure.rule_id(), Some("EFF-1"));
    // The rejected row is on the quoted source line.
    assert!(failure.to_string().contains("reads(owner)"));
    let llvm = compile(source);
    let abandon = emitted_function(&llvm, "abandon");
    let cleanup_calls: Vec<_> = abandon
        .lines()
        .filter_map(|line| line.trim().strip_prefix("call void @wf.drop."))
        .collect();
    assert_eq!(cleanup_calls.len(), 1, "abandon releases its one owner");
    let cleanup = format!("wf.drop.{}", cleanup_calls[0].split('(').next().unwrap());
    let helper_start = llvm
        .find(&format!("define private void @{cleanup}"))
        .expect("resource enum must have one drop helper");
    let helper_end = llvm[helper_start..]
        .find("\n}\n\n")
        .map(|offset| helper_start + offset + 3)
        .expect("drop helper must close");
    let helper = &llvm[helper_start..helper_end];
    assert!(helper.contains("switch i32 %tag"));
    assert!(helper.contains("i32 0, label %variant.0"));
    assert!(helper.contains("i32 1, label %variant.1"));
    assert!(helper.contains("i32 2, label %variant.2"));
    assert!(
        helper.contains("call void @abort()"),
        "cleanup is unchanged"
    );
    assert_eq!(helper.matches("call void @free").count(), 2);
    assert_eq!(abandon.matches(&format!("call void @{cleanup}")).count(), 1);
    let consume = emitted_function(&llvm, "consume");
    assert!(!consume.contains("call void @abort()"));
    assert!(!emitted_function(&llvm, "inspect").contains("call void @abort()"));
    assert!(!consume.contains(&format!("call void @{cleanup}")));
    assert_eq!(consume.matches("call void @free").count(), 2);

    // The two inline Slots owners make this result exceed the register-return
    // ceiling. A linked implementation dirties the complete destination before
    // setting only the Empty or Last tag; inactive owner bytes must never enter
    // borrowed payload reads or cleanup.
    let make_empty = emitted_function(&llvm, "make_empty");
    assert!(
        make_empty.starts_with("define void @wf_make_empty(ptr %wf.result)"),
        "Owner returns through its destination: {make_empty}"
    );
    let renamed = llvm.replacen(
        "define void @wf_make_empty(",
        "define void @wf_test_empty_body(",
        1,
    );
    assert_ne!(renamed, llvm);
    let with_last = renamed.replacen(
        "define void @wf_make_last(",
        "define void @wf_test_last_body(",
        1,
    );
    assert_ne!(with_last, renamed);
    // Eight bytes for the tag and alignment, then two 24-byte Slots owners.
    let dirty_stores = (0..7)
        .map(|word| {
            format!(
                "  %dirty.{word} = getelementptr i8, ptr %wf.result, i64 {}\n  store i64 -6510615555426900571, ptr %dirty.{word}, align 8\n",
                word * 8
            )
        })
        .collect::<String>();
    let linked_constructor = format!(
        "{with_last}\ndefine void @wf_make_empty(ptr %wf.result) {{\n{dirty_stores}  store i32 0, ptr %wf.result, align 4\n  ret void\n}}\ndefine void @wf_make_last(ptr %wf.result) {{\n{dirty_stores}  store i32 2, ptr %wf.result, align 4\n  ret void\n}}\n"
    );
    for module in [&llvm, &linked_constructor] {
        let observed = super::owned_places::retain_calls(module)
            .replace("@malloc(", "@wf_test_allocate(")
            .replace("@free(", "@wf_test_release(");
        let host = super::owned_places::allocation_observer(4, 0);
        let output = compile_link_and_run(&observed, Some(&host), &[]);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        // The first Full travels by swap, then a reference write replaces it
        // with Empty. The second Full reuses that binding, crosses a retained
        // result boundary, and is consumed through its selected payload.
        assert_eq!(output.stdout, b"A1;A2;F1;F2;A3;A4;F3;F4;", "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
    }
}

#[test]
fn tag_only_matches_execute_every_declared_variant() {
    let source = br#"enum Choice {
  First();
  Middle();
  Last();
}

fn select(value: Choice) -> result: u8 pure {
  match value {
    First() => {
      return 1_u8;
    }
    Middle() => {
      return 2_u8;
    }
    Last() => {
      return 3_u8;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let first = Choice::First();
  let first_value = select(value: first);
  if first_value != 1_u8 {
    return std::process::exit_status(code: 1_u8);
  }
  let middle = Choice::Middle();
  let middle_value = select(value: middle);
  if middle_value != 2_u8 {
    return std::process::exit_status(code: 2_u8);
  }
  let last = Choice::Last();
  let last_value = select(value: last);
  if last_value != 3_u8 {
    return std::process::exit_status(code: 3_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    let select = emitted_function(&llvm, "select");
    assert!(select.contains("switch i32"));
    assert!(!select.contains("call void @abort()"));
    let observed = super::owned_places::retain_calls(&llvm);
    let output = compile_link_and_run(&observed, None, &[]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    // Each result check must detect a wrong value from its own arm, even
    // though every mutated source still has a complete, valid tag domain.
    for tag_result in 1..=3 {
        let wrong = std::str::from_utf8(source)
            .unwrap()
            .replace(&format!("return {tag_result}_u8;"), "return 9_u8;");
        let llvm = super::owned_places::retain_calls(&compile(wrong.as_bytes()));
        let output = compile_link_and_run(&llvm, None, &[]);
        assert_eq!(output.status.code(), Some(tag_result), "{output:?}");
    }
}

/// The same transfer, error and abandonment program over an inline run.
///
/// The payload is a constant-capacity `Result<Slots<u8, n>, DecodeError>`
/// [TYPE-9], so its storage is frame-resident [STOR-1]: the enum owns no heap
/// resource, needs no drop helper, and abandoning one on any arm frees
/// nothing. That is the fact under test here, and it is checked directly
/// rather than through a helper that no longer exists;
/// `source_enum_cleanup_switches_on_the_active_variant` above keeps the drop-helper
/// coverage for a payload that does own storage. `transform` is a const
/// generic over the run's length, so the three call sites reach the backend
/// as three monomorphized instances.
#[test]
fn result_run_transfer_error_and_abandonment_execute() {
    let llvm = compile(include_bytes!(
        "../../../../tests/conformance/cases/x-result-buffer-transform-run.wf"
    ));
    assert!(
        !llvm.contains("define private void @wf.drop."),
        "a Result over an inline run owns no storage and needs no drop helper"
    );
    assert!(!llvm.contains("call ptr @malloc"));
    assert!(!llvm.contains("call void @free"));
    let transforms: Vec<_> = llvm
        .lines()
        .filter(|line| line.starts_with("define ") && line.contains("@wf_transform$instance$"))
        .collect();
    assert_eq!(
        transforms.len(),
        3,
        "one transform instance per written run length"
    );
    assert!(
        transforms.iter().all(|header| {
            header.starts_with("define void ") && header.contains("(ptr %wf.result, ptr %wf.arg.")
        }),
        "each transform takes inline input storage and a caller-owned outcome destination"
    );
    let abandon_names: Vec<_> = llvm
        .lines()
        .filter(|line| line.starts_with("define ") && line.contains("@wf_abandon$instance$"))
        .map(|line| {
            line.split("@wf_")
                .nth(1)
                .unwrap()
                .split('(')
                .next()
                .unwrap()
        })
        .collect();
    assert_eq!(abandon_names.len(), 1);
    let abandon = emitted_function(&llvm, abandon_names[0]);
    assert!(!abandon.contains("call void @wf.drop."));

    let output = compile_and_run(&llvm);
    assert!(
        output.status.success(),
        "Result run program failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn option_boxed_window_some_none_and_transfer_execute() {
    let source = br#"fn abandon(value: Option<Box<Slots<u8>>>) -> result: unit pure {
  return unit;
}

fn consume(value: Option<Box<Slots<u8>>>) -> result: u8 pure {
  match move value {
    None() => {
      return 0_u8;
    }
    Some(value: bytes) => {
      let spare = bytes.inner.len;
      let ok = 0_u64 < spare;
      let byte = if ok {
        give bytes.inner[0_u64];
      } else {
        give 0_u8;
      }
      return byte;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let abandoned_bytes = box_slots_new::<u8>(capacity: 1_u64);
  place_back(window: &abandoned_bytes.inner, value: 5_u8);
  let abandoned_some = Some<Box<Slots<u8>>>(value: move abandoned_bytes);
  abandon(value: move abandoned_some);
  let abandoned_none = None<Box<Slots<u8>>>();
  abandon(value: move abandoned_none);
  let consumed_bytes = box_slots_new::<u8>(capacity: 1_u64);
  place_back(window: &consumed_bytes.inner, value: 17_u8);
  let consumed_some = Some<Box<Slots<u8>>>(value: move consumed_bytes);
  let consumed_byte = consume(value: move consumed_some);
  if consumed_byte != 17_u8 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    let helper_start = llvm
        .find("define private void @wf.drop.")
        .expect("Option<Box<Slots<u8>>> must have a drop helper");
    let helper_end = llvm[helper_start..]
        .find("\n}\n\n")
        .map(|offset| helper_start + offset + 3)
        .expect("drop helper must close");
    let helper = &llvm[helper_start..helper_end];
    assert_eq!(helper.matches("call void @free").count(), 1);
    assert_eq!(
        emitted_function(&llvm, "abandon")
            .matches("call void @wf.drop.")
            .count(),
        1
    );
    let consume = emitted_function(&llvm, "consume");
    assert!(!consume.contains("call void @wf.drop."));
    assert_eq!(consume.matches("call void @free").count(), 1);

    let output = compile_and_run(&llvm);
    assert!(
        output.status.success(),
        "Option boxed-window program failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}
