//! Read-through is physical placement, not a change to snapshot semantics.
//! These cases belong to the backend gate; none runs during source editing.

use super::system::with_ir;
use super::{compile_and_run, emit, emitted_function, host_optimized_module};
use crate::{IrInstruction, IrOperation, IrProgram, IrType, IrValueId};

const VALUES: &str = r#"enum Value {
  Number(n: u64);
  Function(id: u64);
  Native(id: u64);
  Empty();
}

fn classify(v: Value) -> result: u64 pure {
  match v {
    Number(n: number) => {
      return number;
    }
    Function(id: function_id) => {
      return function_id +wrap 1_u64;
    }
    Native(id: native_id) => {
      return native_id +wrap 2_u64;
    }
    Empty() => {
      return 0_u64;
    }
  }
}

fn prepare(values: &Box<Slots<Value>>, index: u64) -> result: u64 reads(values) contract {
  requires index < values^.inner.len;
} {
  let v = values^.inner[index];
  let class = classify(v: v);
  match v {
    Number(n: number) => {
      return class +wrap number;
    }
    Function(..) => {
      return class +wrap 3_u64;
    }
    Native(..) => {
      return class +wrap 4_u64;
    }
    Empty() => {
      return class +wrap 5_u64;
    }
  }
}

fn replace(values: &Box<Slots<Value>>, index: u64) -> result: unit writes(values.inner[index]) contract {
  requires index < values^.inner.len;
} {
  set values^.inner[index] = Value::Number(n: 99_u64);
  return unit;
}

fn before_write(values: &Box<Slots<Value>>, index: u64, change: Bool) -> result: u64 writes(values.inner[index]) contract {
  requires index < values^.inner.len;
} {
  let v = values^.inner[index];
  if change {
    set values^.inner[index] = Value::Number(n: 99_u64);
  }
  return classify(v: v);
}

fn before_call(values: &Box<Slots<Value>>, index: u64) -> result: u64 writes(values.inner[index]) contract {
  requires index < values^.inner.len;
} {
  let v = values^.inner[index];
  replace(values: values, index: index);
  return classify(v: v);
}

fn before_grow(values: &Box<Slots<Value>>, index: u64) -> result: u64 writes(values) contract {
  requires index < values^.inner.len;
  requires values^.inner.cap <= 8_u64;
} {
  let v = values^.inner[index];
  grow(cell: values, capacity: 8_u64);
  return classify(v: v);
}

fn before_push(values: &Box<Slots<Value>>, index: u64) -> result: u64 writes(values) contract {
  requires index < values^.inner.len;
  requires values^.inner.len < values^.inner.cap;
} {
  let v = values^.inner[index];
  let empty = Value::Empty();
  place_back(window: &values^.inner, value: empty);
  return classify(v: v);
}

fn unrelated(values: &Box<Slots<Value>>, other: &Box<Slots<Value>>, index: u64) -> result: u64 reads(values), writes(other.inner[index]) contract {
  requires index < values^.inner.len;
  requires index < other^.inner.len;
} {
  let v = values^.inner[index];
  replace(values: other, index: index);
  return classify(v: v);
}

fn unrelated_store(values: &Box<Slots<Value>>, other: &Box<Slots<Value>>, index: u64) -> result: u64 reads(values), writes(other.inner[index]) contract {
  requires index < values^.inner.len;
  requires index < other^.inner.len;
} {
  let v = values^.inner[index];
  set other^.inner[index] = Value::Empty();
  return classify(v: v);
}

struct Holder {
  cell: Box<Value>;
}

struct Wrapper {
  held: Holder;
}

fn touch(value: &Wrapper) -> result: unit reads(value) {
  let observed = value^.held.cell.inner;
  return unit;
}

fn moved_owner(flag: Bool) -> result: u64 pure {
  let initial = Value::Number(n: 7_u64);
  let cell = box_new::<Value>(value: initial);
  let holder = Holder(cell: move cell);
  let source = &holder.cell.inner;
  let v = source^;
  if flag {
    let wrapped = Wrapper(held: move holder);
    touch(value: &wrapped);
  } else {
    let wrapped = Wrapper(held: move holder);
    touch(value: &wrapped);
  }
  return classify(v: v);
}

fn after_use(values: &Box<Slots<Value>>, index: u64) -> result: u64 writes(values.inner[index]) contract {
  requires index < values^.inner.len;
} {
  let v = values^.inner[index];
  let old = classify(v: v);
  replace(values: values, index: index);
  return old;
}

fn mutate(v: Value) -> result: u64 pure {
  let address = &v;
  set address^ = Value::Number(n: 37_u64);
  return classify(v: v);
}

fn mutable_argument(values: &Box<Slots<Value>>, index: u64) -> result: u64 reads(values) contract {
  requires index < values^.inner.len;
} {
  let v = values^.inner[index];
  let changed = mutate(v: v);
  let old = classify(v: v);
  return changed +wrap old;
}

fn exposed(values: &Box<Slots<Value>>, index: u64) -> result: u64 reads(values) contract {
  requires index < values^.inner.len;
} {
  let v = values^.inner[index];
  let address = &v;
  set address^ = Value::Number(n: 37_u64);
  return classify(v: v);
}
"#;

const MAIN: &str = r#"
fn main() -> status: std::process::ExitStatus pure {
  let values = box_slots_new::<Value>(capacity: 2_u64);
  let other = box_slots_new::<Value>(capacity: 2_u64);
  if values.inner.len < values.inner.cap {
    let initial = Value::Number(n: 7_u64);
    place_back(window: &values.inner, value: initial);
  }
  if other.inner.len < other.inner.cap {
    let empty = Value::Empty();
    place_back(window: &other.inner, value: empty);
  }
  if values.inner.len > 0_u64 {
    if other.inner.len > 0_u64 {
      let observed_1 = prepare(values: &values, index: 0_u64);
      if observed_1 != 14_u64 {
        return std::process::exit_status(code: 1_u8);
      }
      let observed_2 = unrelated(values: &values, other: &other, index: 0_u64);
      if observed_2 != 7_u64 {
        return std::process::exit_status(code: 2_u8);
      }
      let direct_write = unrelated_store(values: &values, other: &other, index: 0_u64);
      if direct_write != 7_u64 {
        return std::process::exit_status(code: 13_u8);
      }
      let observed_3 = mutable_argument(values: &values, index: 0_u64);
      if observed_3 != 44_u64 {
        return std::process::exit_status(code: 3_u8);
      }
      let observed_4 = exposed(values: &values, index: 0_u64);
      if observed_4 != 37_u64 {
        return std::process::exit_status(code: 4_u8);
      }
      let observed_5 = classify(v: values.inner[0_u64]);
      if observed_5 != 7_u64 {
        return std::process::exit_status(code: 5_u8);
      }
      let no_change = False();
      let observed_6 = before_write(values: &values, index: 0_u64, change: no_change);
      if observed_6 != 7_u64 {
        return std::process::exit_status(code: 6_u8);
      }
      let change = True();
      let moved_true = moved_owner(flag: change);
      let moved_false = moved_owner(flag: no_change);
      if moved_true != 7_u64 {
        return std::process::exit_status(code: 14_u8);
      }
      if moved_false != 7_u64 {
        return std::process::exit_status(code: 15_u8);
      }
      let observed_7 = before_write(values: &values, index: 0_u64, change: change);
      if observed_7 != 7_u64 {
        return std::process::exit_status(code: 7_u8);
      }
      set values.inner[0_u64] = Value::Number(n: 7_u64);
      let observed_8 = before_call(values: &values, index: 0_u64);
      if observed_8 != 7_u64 {
        return std::process::exit_status(code: 8_u8);
      }
      set values.inner[0_u64] = Value::Number(n: 7_u64);
      if values.inner.cap <= 8_u64 {
        let observed_9 = before_grow(values: &values, index: 0_u64);
        if observed_9 != 7_u64 {
          return std::process::exit_status(code: 9_u8);
        }
      }
      if values.inner.len > 0_u64 {
        if values.inner.len < values.inner.cap {
          let observed_10 = before_push(values: &values, index: 0_u64);
          if observed_10 != 7_u64 {
            return std::process::exit_status(code: 10_u8);
          }
        }
        if values.inner.len > 0_u64 {
          let observed_11 = after_use(values: &values, index: 0_u64);
          if observed_11 != 7_u64 {
            return std::process::exit_status(code: 11_u8);
          }
        }
      }
      return std::process::exit_status(code: 0_u8);
    }
  }
  return std::process::exit_status(code: 12_u8);
}
"#;

fn snapshot(program: &IrProgram, name: &str) -> IrValueId {
    let function = program
        .functions()
        .iter()
        .find(|function| function.name() == name)
        .unwrap_or_else(|| panic!("missing fixture function {name}"));
    let (address, ty) = function
        .blocks()
        .iter()
        .flat_map(|block| block.instructions())
        .find_map(|instruction| match instruction {
            IrInstruction::Define {
                ty: ty @ IrType::Nominal(id),
                operation: IrOperation::Load { address, .. },
                ..
            } if program
                .nominal(*id)
                .is_some_and(|nominal| nominal.name() == "Value") =>
            {
                Some((*address, *ty))
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("{name}: missing snapshot Load of Value"));
    let layout = crate::target::validate_static_storage(
        crate::target::TargetLayout::host().expect("host layout"),
        program,
        &crate::target::TargetStorageType::source(ty),
    )
    .expect("snapshot layout");
    assert_eq!(
        layout.size(),
        16,
        "the witness must copy a 16-byte union enum"
    );
    address
}

fn assert_snapshot_copy(body: &str, address: IrValueId, copied: bool) {
    let source = format!(", ptr %v{}, i64 ", address.ordinal());
    assert_eq!(
        body.lines()
            .any(|line| line.contains("@llvm.memmove.") && line.contains(&source)),
        copied,
        "{body}"
    );
}

fn assert_no_copy(body: &str) {
    assert!(
        !body.contains("@llvm.memmove.") && !body.contains("@llvm.memcpy."),
        "{body}"
    );
}

fn assert_element_argument(body: &str, tag_in_caller: bool) {
    let call = body
        .lines()
        .find_map(|line| line.split_once("@wf_classify("))
        .expect("non-inlined classifier")
        .1;
    let name = call.rsplit_once('%').expect("SSA element address").1;
    let name: String = name
        .chars()
        .take_while(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '$')
        })
        .collect();
    let address = format!("%{name}");
    assert!(
        body.lines().any(|line| {
            line.trim_start()
                .starts_with(&format!("{address} = getelementptr "))
        }),
        "classifier must receive the element GEP, not a materialized slot: {body}"
    );
    if tag_in_caller {
        assert_tag_through(body, &address);
    }
}

/// Raw emission uses a zero-offset field GEP for the tag; LLVM folds it.
fn assert_tag_through(body: &str, address: &str) {
    let mut tags = vec![address.to_owned()];
    for line in body.lines() {
        if line.contains("getelementptr ")
            && line.ends_with(", i32 0, i32 0")
            && line.contains(&format!(", ptr {address},"))
        {
            tags.push(line.split_once(" = ").expect("tag GEP").0.trim().to_owned());
        }
    }
    assert!(
        body.lines().any(|line| {
            line.split_once("load i32, ptr ")
                .is_some_and(|(_, source)| {
                    let pointer = source.split([',', ' ']).next().expect("load pointer");
                    tags.iter().any(|tag| tag == pointer)
                })
        }),
        "tag must read through {address}: {body}"
    );
}

/// Keep the observation boundary without changing a dispatcher's mandatory
/// alwaysinline contract. This helper only changes test optimization policy.
fn retain_classifier(module: &str) -> String {
    module
        .lines()
        .map(|line| {
            if line.starts_with("define ") && line.contains(" @wf_classify(") {
                format!(
                    "{} noinline {{\n",
                    line.strip_suffix(" {").expect("classifier header")
                )
            } else {
                format!("{line}\n")
            }
        })
        .collect()
}

#[test]
fn read_through_snapshot_placement_preserves_old_values_and_call_boundaries() {
    let source = format!("{VALUES}{MAIN}");
    let cases = [
        ("prepare", true),
        ("before_write", false),
        ("before_call", false),
        ("before_grow", false),
        ("before_push", false),
        ("unrelated", true),
        ("unrelated_store", true),
        // The explicit reference read makes this a Load snapshot; reading
        // holder.cell.inner directly lowers as BoxDeref, outside this planner.
        ("moved_owner", false),
        ("after_use", true),
        ("mutable_argument", false),
        ("exposed", false),
    ];
    let addresses = with_ir(source.as_bytes(), |program| {
        cases
            .iter()
            .map(|(name, _)| snapshot(program, name))
            .collect::<Vec<_>>()
    });
    // Keep classify out of line: host
    // inlining must not hide a caller-side snapshot or a broken by-value ABI.
    let module = retain_classifier(&emit(source.as_bytes()));
    for ((name, eligible), address) in cases.iter().zip(addresses) {
        assert_snapshot_copy(emitted_function(&module, name), address, !eligible);
        if *name == "prepare" {
            let body = emitted_function(&module, name);
            assert!(
                body.contains(&format!("@wf_classify(ptr %v{})", address.ordinal())),
                "{body}"
            );
            assert_tag_through(body, &format!("%v{}", address.ordinal()));
        }
    }
    let optimized = host_optimized_module(&module);
    for candidate in [&module, &optimized] {
        let body = emitted_function(candidate, "prepare");
        assert_no_copy(body);
        assert_element_argument(body, true);
        assert!(
            body.contains("@wf_classify("),
            "the classifier must remain out of line: {body}"
        );
        let classify = emitted_function(candidate, "classify");
        assert_no_copy(classify);
        assert_tag_through(classify, "%wf.arg.v0");
        assert_element_argument(emitted_function(candidate, "unrelated"), false);
    }
    // Baseline d903bf3f6 fails the positive raw copy/address assertions: every
    // Load gets owned backing. Ignoring a branch/call write returns 99 rather
    // than 7; ignoring resize/push fails their explicit copy assertions even
    // if growth happens to keep the allocation. Treating any write as a
    // barrier fails unrelated; extending liveness past the last use fails
    // after_use. Exposure/mutable-argument mistakes either lose the copy or
    // corrupt the source observed again in main.
    let output = compile_and_run(&module);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(
        output.stdout.is_empty() && output.stderr.is_empty(),
        "{output:?}"
    );
}

#[test]
fn read_through_snapshot_stays_in_one_dispatch_part() {
    let source = format!(
        "{VALUES}{}",
        r#"
enum Command {
  Again();
  Read();
}

fn local(values: &Box<Slots<Value>>, command: Command) -> result: u64 reads(values) contract {
  requires values^.inner.len > 0_u64;
} {
  loop {
    match command {
      Again() => {
        set command = Command::Read();
        continue;
      }
      Read() => {
        let v = values^.inner[0_u64];
        return classify(v: v);
      }
    }
  }
}

fn crossing(values: &Box<Slots<Value>>, command: Command) -> result: u64 reads(values) contract {
  requires values^.inner.len > 0_u64;
} {
  let v = values^.inner[0_u64];
  loop {
    match command {
      Again() => {
        set command = Command::Read();
        continue;
      }
      Read() => {
        return classify(v: v);
      }
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let values = box_slots_new::<Value>(capacity: 1_u64);
  if values.inner.len < values.inner.cap {
    let initial = Value::Number(n: 7_u64);
    place_back(window: &values.inner, value: initial);
  }
  if values.inner.len > 0_u64 {
    let again = Command::Again();
    let observed_12 = local(values: &values, command: again);
    if observed_12 != 7_u64 {
      return std::process::exit_status(code: 1_u8);
    }
    let observed_13 = crossing(values: &values, command: again);
    if observed_13 != 7_u64 {
      return std::process::exit_status(code: 2_u8);
    }
    return std::process::exit_status(code: 0_u8);
  }
  return std::process::exit_status(code: 3_u8);
}
"#
    );
    let (local, crossing) = with_ir(source.as_bytes(), |program| {
        (snapshot(program, "local"), snapshot(program, "crossing"))
    });
    let module = retain_classifier(&emit(source.as_bytes()));
    // These assertions prevent an accidentally unsplit fixture from passing.
    for name in ["local", "crossing"] {
        assert!(
            module.contains(&format!("; dispatch: wf_{name}: split:")),
            "{module}"
        );
        assert!(emitted_function(&module, &format!("{name}.arm.0")).contains("musttail"));
    }
    assert_snapshot_copy(emitted_function(&module, "local.arm.1"), local, false);
    assert_snapshot_copy(emitted_function(&module, "crossing"), crossing, true);
    let optimized = host_optimized_module(&module);
    for candidate in [&module, &optimized] {
        let body = emitted_function(candidate, "local.arm.1");
        assert_no_copy(body);
        assert_element_argument(body, false);
        assert!(body.contains("@wf_classify("), "{body}");
    }
    // Allowing aliases across parts fails crossing's copy assertion;
    // excluding every split function fails local's direct-call assertion.
    let output = compile_and_run(&module);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(
        output.stdout.is_empty() && output.stderr.is_empty(),
        "{output:?}"
    );
}

#[test]
fn read_through_distinguishes_previous_and_fresh_loop_snapshots() {
    let source = format!(
        "{VALUES}{}",
        r#"
fn carried(values: &Box<Slots<Value>>, index: u64) -> result: u64 writes(values.inner[index]) contract {
  requires index < values^.inner.len;
} {
  let last = Value::Number(n: 3_u64);
  let total = 0_u64;
  for (i in 0_u64..3_u64) {
    let next = i +wrap 7_u64;
    set values^.inner[index] = Value::Number(n: next);
    let current = values^.inner[index];
    let old = classify(v: last);
    set total = total +wrap old;
    set last = current;
  }
  return total;
}

fn fresh(values: &Box<Slots<Value>>, index: u64) -> result: u64 writes(values.inner[index]) contract {
  requires index < values^.inner.len;
} {
  let total = 0_u64;
  for (i in 0_u64..3_u64) {
    let next = i +wrap 7_u64;
    set values^.inner[index] = Value::Number(n: next);
    let current = values^.inner[index];
    let observed = classify(v: current);
    set total = total +wrap observed;
  }
  return total;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = box_slots_new::<Value>(capacity: 1_u64);
  if values.inner.len < values.inner.cap {
    let initial = Value::Number(n: 7_u64);
    place_back(window: &values.inner, value: initial);
  }
  if values.inner.len > 0_u64 {
    let previous = carried(values: &values, index: 0_u64);
    if previous != 18_u64 {
      return std::process::exit_status(code: 1_u8);
    }
    let current = fresh(values: &values, index: 0_u64);
    if current != 24_u64 {
      return std::process::exit_status(code: 2_u8);
    }
    return std::process::exit_status(code: 0_u8);
  }
  return std::process::exit_status(code: 3_u8);
}
"#
    );
    let (carried, fresh) = with_ir(source.as_bytes(), |program| {
        (snapshot(program, "carried"), snapshot(program, "fresh"))
    });
    let module = retain_classifier(&emit(source.as_bytes()));
    assert_snapshot_copy(emitted_function(&module, "carried"), carried, true);
    let fresh_body = emitted_function(&module, "fresh");
    assert_snapshot_copy(fresh_body, fresh, false);
    assert!(
        fresh_body.contains(&format!("@wf_classify(ptr %v{})", fresh.ordinal())),
        "{fresh_body}"
    );
    // carried observes 3 + 7 + 8, after each source write and re-execution
    // of the same Load; reading last through the current source instead
    // observes 3 + 8 + 9.
    // The pre-loop seed makes last mixed-origin, so this source regression
    // does not isolate select_read_through's live/family guard: the earlier
    // origin checks or transfer materialization also preserve this copy.
    // fresh observes 7 + 8 + 9 with no carry. Removing source_survives's
    // Load-revisit stop propagates the next iteration's write to its read,
    // adds a snapshot copy, and fails fresh's no-copy/direct-call assertions.
    let output = compile_and_run(&module);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(
        output.stdout.is_empty() && output.stderr.is_empty(),
        "{output:?}"
    );
}

#[test]
fn snapshot_materialization_is_local_to_the_use_unless_the_source_changed() {
    let source = format!(
        "{VALUES}{}",
        r#"
fn cold_value(v: Value, values: &Box<Slots<Value>>, index: u64) -> result: u64 writes(values.inner[index]) contract {
  requires index < values^.inner.len;
} {
  set values^.inner[index] = Value::Number(n: 99_u64);
  let old = classify(v: v);
  let address = &v;
  set address^ = Value::Number(n: 37_u64);
  let changed = classify(v: v);
  let high = old *wrap 100_u64;
  return high +wrap changed;
}

fn cold_read(v: Value, values: &Box<Slots<Value>>, index: u64) -> result: u64 writes(values.inner[index]) contract {
  requires index < values^.inner.len;
} {
  set values^.inner[index] = Value::Number(n: 99_u64);
  return classify(v: v);
}

fn call_boundary(values: &Box<Slots<Value>>, index: u64) -> result: u64 writes(values.inner[index]) contract {
  requires index < values^.inner.len;
} {
  let v = values^.inner[index];
  let before = classify(v: v);
  let after = cold_read(v: v, values: values, index: index);
  return before +wrap after;
}

fn lazy(values: &Box<Slots<Value>>, index: u64, cold: Bool) -> result: u64 writes(values.inner[index]) contract {
  requires index < values^.inner.len;
} {
  let v = values^.inner[index];
  if cold {
    return cold_value(v: v, values: values, index: index);
  }
  match v {
    Number(..) => {
      return 1_u64;
    }
    Function(..) => {
      return 2_u64;
    }
    Native(..) => {
      return 3_u64;
    }
    Empty() => {
      return 4_u64;
    }
  }
}

fn changed_before_cold(values: &Box<Slots<Value>>, index: u64, cold: Bool) -> result: u64 writes(values.inner[index]) contract {
  requires index < values^.inner.len;
} {
  let v = values^.inner[index];
  if cold {
    set values^.inner[index] = Value::Number(n: 99_u64);
    return cold_value(v: v, values: values, index: index);
  }
  match v {
    Number(..) => {
      return 1_u64;
    }
    Function(..) => {
      return 2_u64;
    }
    Native(..) => {
      return 3_u64;
    }
    Empty() => {
      return 4_u64;
    }
  }
}

fn destination_result(v: Value) -> result: Value pure {
  let old = classify(v: v);
  return Value::Function(id: old);
}

fn return_snapshot(values: &Box<Slots<Value>>, index: u64) -> result: Value reads(values) contract {
  requires index < values^.inner.len;
} {
  let v = values^.inner[index];
  return v;
}

fn destination_argument(values: &Box<Slots<Value>>, index: u64) -> result: u64 reads(values) contract {
  requires index < values^.inner.len;
} {
  let v = values^.inner[index];
  let returned = destination_result(v: v);
  let produced = classify(v: returned);
  let old = classify(v: v);
  return old +wrap produced;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = box_slots_new::<Value>(capacity: 1_u64);
  if values.inner.len < values.inner.cap {
    let initial = Value::Number(n: 7_u64);
    place_back(window: &values.inner, value: initial);
  }
  if values.inner.len > 0_u64 {
    let hot = False();
    let cold = True();
    let hot_value = lazy(values: &values, index: 0_u64, cold: hot);
    if hot_value != 1_u64 {
      return std::process::exit_status(code: 1_u8);
    }
    let destination_value = destination_argument(values: &values, index: 0_u64);
    if destination_value != 15_u64 {
      return std::process::exit_status(code: 2_u8);
    }
    let returned_snapshot = return_snapshot(values: &values, index: 0_u64);
    let returned_old = classify(v: returned_snapshot);
    if returned_old != 7_u64 {
      return std::process::exit_status(code: 8_u8);
    }
    let cold_observed = lazy(values: &values, index: 0_u64, cold: cold);
    if cold_observed != 737_u64 {
      return std::process::exit_status(code: 3_u8);
    }
    let new_value = classify(v: values.inner[0_u64]);
    if new_value != 99_u64 {
      return std::process::exit_status(code: 4_u8);
    }
    set values.inner[0_u64] = Value::Number(n: 7_u64);
    let changed_hot = changed_before_cold(values: &values, index: 0_u64, cold: hot);
    if changed_hot != 1_u64 {
      return std::process::exit_status(code: 5_u8);
    }
    let changed_cold = changed_before_cold(values: &values, index: 0_u64, cold: cold);
    if changed_cold != 737_u64 {
      return std::process::exit_status(code: 6_u8);
    }
    set values.inner[0_u64] = Value::Number(n: 7_u64);
    let during_call = call_boundary(values: &values, index: 0_u64);
    if during_call != 14_u64 {
      return std::process::exit_status(code: 9_u8);
    }
    return std::process::exit_status(code: 0_u8);
  }
  return std::process::exit_status(code: 7_u8);
}
"#
    );
    let addresses = with_ir(source.as_bytes(), |program| {
        [
            "lazy",
            "changed_before_cold",
            "destination_argument",
            "return_snapshot",
            "call_boundary",
        ]
        .map(|name| snapshot(program, name))
    });
    let module = super::owned_places::retain_calls(&emit(source.as_bytes()));
    let lazy = emitted_function(&module, "lazy");
    let copy = snapshot_copy_line(lazy, addresses[0]);
    let call = lazy
        .lines()
        .position(|line| line.contains("@wf_cold_value("))
        .expect("cold call");
    assert_eq!(
        call,
        copy + 1,
        "copy immediately precedes the cold call: {lazy}"
    );
    let copied_block = llvm_block_at(lazy, copy);
    let conditional = lazy
        .lines()
        .position(|line| line.contains("switch i1 "))
        .expect("hot/cold branch");
    assert_ne!(
        copied_block,
        llvm_block_at(lazy, conditional),
        "no entry copy: {lazy}"
    );
    assert!(
        lazy.lines()
            .skip(conditional + 1)
            .take_while(|line| line.trim() != "]")
            .any(|line| line.trim() == format!("i1 1, label %{copied_block}")),
        "only the true/cold successor copies: {lazy}"
    );
    assert_eq!(
        lazy.matches("@llvm.memmove.").count(),
        1,
        "no other path copies: {lazy}"
    );
    assert_tag_through(lazy, &format!("%v{}", addresses[0].ordinal()));
    let changed = emitted_function(&module, "changed_before_cold");
    let copy = snapshot_copy_line(changed, addresses[1]);
    let conditional = changed
        .lines()
        .position(|line| line.contains("switch i1 "))
        .expect("hot/cold branch");
    assert!(
        copy < conditional,
        "a cold-path write forces the Load copy: {changed}"
    );
    assert_eq!(
        llvm_block_at(changed, copy),
        llvm_block_at(changed, conditional)
    );
    let destination = emitted_function(&module, "destination_argument");
    assert_snapshot_copy(destination, addresses[2], false);
    let call = destination
        .lines()
        .find(|line| line.contains("call void @wf_destination_result("))
        .expect("destination-result call");
    assert!(
        call.ends_with(&format!(", ptr %v{})", addresses[2].ordinal())),
        "pass the original element address: {destination}"
    );
    let returned = emitted_function(&module, "return_snapshot");
    let copy = snapshot_copy_line(returned, addresses[3]);
    let lines: Vec<_> = returned.lines().collect();
    assert!(
        lines[copy].contains("(ptr %wf.result,"),
        "copy directly to the result: {returned}"
    );
    assert_eq!(
        lines[copy + 1].trim(),
        "ret void",
        "materialize at the return: {returned}"
    );
    let boundary = emitted_function(&module, "call_boundary");
    let copy = snapshot_copy_line(boundary, addresses[4]);
    let call = boundary
        .lines()
        .position(|line| line.contains("@wf_cold_read("))
        .expect("writing call");
    assert_eq!(
        call,
        copy + 1,
        "capture before the call's source write: {boundary}"
    );
    let observation = boundary
        .lines()
        .position(|line| line.contains("@wf_classify("))
        .expect("earlier read-through observation");
    assert!(
        observation < copy,
        "keep the copy after the earlier read: {boundary}"
    );
    // This callee reads its immutable parameter in place, so the native old
    // value observation also detects omission of the caller-side capture.
    let cold_read = emitted_function(&module, "cold_read");
    assert!(
        cold_read.contains("@wf_classify(ptr %wf.arg.v0)"),
        "{cold_read}"
    );
    // The callee's entry capture stays: its general ABI admits input/result
    // aliasing at other call sites. Removing a caller copy is independent.
    let callee = emitted_function(&module, "destination_result");
    assert!(
        callee
            .lines()
            .any(|line| line.contains("@llvm.memmove.") && line.contains(", ptr %wf.arg.v0,")),
        "{callee}"
    );
    let output = compile_and_run(&module);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(
        output.stdout.is_empty() && output.stderr.is_empty(),
        "{output:?}"
    );
}

fn snapshot_copy_line(body: &str, address: IrValueId) -> usize {
    let source = format!(", ptr %v{}, i64 ", address.ordinal());
    let copies: Vec<_> = body
        .lines()
        .enumerate()
        .filter(|(_, line)| line.contains("@llvm.memmove.") && line.contains(&source))
        .map(|(line, _)| line)
        .collect();
    assert_eq!(copies.len(), 1, "one snapshot copy: {body}");
    copies[0]
}

fn llvm_block_at(body: &str, line: usize) -> &str {
    body.lines()
        .take(line + 1)
        .filter_map(|line| line.strip_suffix(':'))
        .last()
        .expect("instruction in an LLVM block")
}
