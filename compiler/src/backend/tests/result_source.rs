//! Call results read at their source through a definition's `.source`
//! entry (compiler/storage-placement, compiler/result-registers and
//! compiler/backend-facts, proposed). These cases belong to the backend gate.

use super::owned_places::retain_calls;
use super::{compile_and_run, emit, emitted_function};

const ITEMS: &str = r#"struct Item {
  a: u64;
  b: u64;
  c: u64;
  d: u64;
}

fn item_at(items: &Box<Slots<Item>>, index: u64) -> result: Item reads(items) contract {
  requires index < items^.inner.len;
} {
  let item = items^.inner[index];
  return item;
}

fn field_sum(items: &Box<Slots<Item>>, index: u64) -> result: u64 reads(items) {
  if index < items^.inner.len {
    let item = items^.inner[index];
    let first = item.a;
    let second = item.b;
    let third = item.c;
    let fourth = item.d;
    let low = first +wrap second;
    let high = third +wrap fourth;
    return low +wrap high;
  }
  return 0_u64;
}
"#;

const CASES: &str = r#"
fn forward(items: &Box<Slots<Item>>, index: u64) -> result: Item reads(items) contract {
  requires index < items^.inner.len;
} {
  let item = item_at(items: items, index: index);
  return item;
}

fn item_or_default(items: &Box<Slots<Item>>, index: u64) -> result: Item reads(items) {
  if index < items^.inner.len {
    let item = items^.inner[index];
    return item;
  }
  return Item(a: 5_u64, b: 6_u64, c: 7_u64, d: 8_u64);
}

fn take_and_clear(items: &Box<Slots<Item>>, index: u64) -> result: Item writes(items) contract {
  requires index < items^.inner.len;
} {
  let item = items^.inner[index];
  set items^.inner[index] = Item(a: 0_u64, b: 0_u64, c: 0_u64, d: 0_u64);
  return item;
}

fn choose(items: &Box<Slots<Item>>, index: u64, flip: Bool) -> result: Item reads(items) contract {
  requires index < items^.inner.len;
} {
  let item = item_at(items: items, index: index);
  if flip {
    return Item(a: item.d, b: item.c, c: item.b, d: item.a);
  }
  return item;
}

fn read_two(items: &Box<Slots<Item>>) -> result: u64 reads(items) contract {
  requires 1_u64 < items^.inner.len;
} {
  let item = item_at(items: items, index: 1_u64);
  let first = item.a;
  let last = item.d;
  return first +wrap last;
}

fn read_forwarded(items: &Box<Slots<Item>>) -> result: u64 reads(items) contract {
  requires 1_u64 < items^.inner.len;
} {
  let item = forward(items: items, index: 1_u64);
  let second = item.b;
  let third = item.c;
  return second +wrap third;
}

fn read_after_write(items: &Box<Slots<Item>>) -> result: u64 writes(items) contract {
  requires 1_u64 < items^.inner.len;
} {
  let item = item_at(items: items, index: 1_u64);
  set items^.inner[1_u64] = Item(a: 90_u64, b: 91_u64, c: 92_u64, d: 93_u64);
  let old = item.a;
  return old;
}

fn copy_out(items: &Box<Slots<Item>>, out: &Box<Slots<Item>>) -> result: u64 reads(items), writes(out) contract {
  requires 1_u64 < items^.inner.len;
  requires 0_u64 < out^.inner.len;
} {
  let item = item_at(items: items, index: 1_u64);
  set out^.inner[0_u64] = item;
  let second = item.b;
  return second;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = box_slots_new::<Item>(capacity: 2_u64);
  let other = box_slots_new::<Item>(capacity: 1_u64);
  if values.inner.len < values.inner.cap {
    let first = Item(a: 1_u64, b: 2_u64, c: 3_u64, d: 4_u64);
    place_back(window: &values.inner, value: first);
  }
  if values.inner.len < values.inner.cap {
    let second = Item(a: 11_u64, b: 12_u64, c: 13_u64, d: 14_u64);
    place_back(window: &values.inner, value: second);
  }
  if other.inner.len < other.inner.cap {
    let blank = Item(a: 0_u64, b: 0_u64, c: 0_u64, d: 0_u64);
    place_back(window: &other.inner, value: blank);
  }
  if 1_u64 < values.inner.len {
    if 0_u64 < other.inner.len {
      let two = read_two(items: &values);
      if two != 25_u64 {
        return std::process::exit_status(code: 1_u8);
      }
      let forwarded = read_forwarded(items: &values);
      if forwarded != 25_u64 {
        return std::process::exit_status(code: 2_u8);
      }
      let no_flip = False();
      let straight = choose(items: &values, index: 1_u64, flip: no_flip);
      let straight_first = straight.a;
      if straight_first != 11_u64 {
        return std::process::exit_status(code: 3_u8);
      }
      let flip = True();
      let flipped = choose(items: &values, index: 1_u64, flip: flip);
      let flipped_first = flipped.a;
      if flipped_first != 14_u64 {
        return std::process::exit_status(code: 4_u8);
      }
      let found = item_or_default(items: &values, index: 1_u64);
      let found_third = found.c;
      if found_third != 13_u64 {
        return std::process::exit_status(code: 5_u8);
      }
      let missing = item_or_default(items: &values, index: 7_u64);
      set other.inner[0_u64] = missing;
      let defaulted = field_sum(items: &other, index: 0_u64);
      if defaulted != 26_u64 {
        return std::process::exit_status(code: 6_u8);
      }
      let copied = copy_out(items: &values, out: &other);
      if copied != 12_u64 {
        return std::process::exit_status(code: 7_u8);
      }
      let copied_sum = field_sum(items: &other, index: 0_u64);
      if copied_sum != 50_u64 {
        return std::process::exit_status(code: 8_u8);
      }
      let old = read_after_write(items: &values);
      if old != 11_u64 {
        return std::process::exit_status(code: 9_u8);
      }
      let written = field_sum(items: &values, index: 1_u64);
      if written != 366_u64 {
        return std::process::exit_status(code: 10_u8);
      }
      if 0_u64 < values.inner.len {
        let taken = take_and_clear(items: &values, index: 0_u64);
        let taken_second = taken.b;
        if taken_second != 2_u64 {
          return std::process::exit_status(code: 11_u8);
        }
        let cleared = field_sum(items: &values, index: 0_u64);
        if cleared != 0_u64 {
          return std::process::exit_status(code: 12_u8);
        }
        return std::process::exit_status(code: 0_u8);
      }
    }
  }
  return std::process::exit_status(code: 13_u8);
}
"#;

/// The text of one emitted definition's header line.
fn header(function: &str) -> &str {
    function.lines().next().expect("a definition header")
}

/// A call names the callee's source entry, or its public symbol.
fn calls_source(body: &str, callee: &str) -> bool {
    body.contains(&format!("call ptr @wf_{callee}.source("))
}

fn calls_public(body: &str, callee: &str) -> bool {
    body.contains(&format!("call void @wf_{callee}("))
}

#[test]
fn call_results_read_at_their_source_keep_their_values() {
    let source = format!("{ITEMS}{CASES}");
    let module = emit(source.as_bytes());

    // Every use reads a field and nothing writes the source in between: the
    // caller reads through the address the source entry returns and copies
    // nothing itself.
    let two = emitted_function(&module, "read_two");
    assert!(
        calls_source(two, "item_at") && !calls_public(two, "item_at"),
        "{two}"
    );
    assert!(!two.contains("; layout-bounded transfer "), "{two}");
    // A write to a place the callee's row names, between the call and the
    // read, forces the public symbol: dropping the invalidation check reads
    // 90 instead of 11 (exit status 9).
    let written = emitted_function(&module, "read_after_write");
    assert!(
        calls_public(written, "item_at") && !calls_source(written, "item_at"),
        "{written}"
    );
    // A use that is not a read in place, a store of the whole value, forces
    // the public symbol rather than a caller-side copy.
    let copied = emitted_function(&module, "copy_out");
    assert!(
        calls_public(copied, "item_at") && !calls_source(copied, "item_at"),
        "{copied}"
    );
    // A forwarded call that received this activation's own destination
    // returns the inner entry's address; its callers read through it too.
    let forward = emitted_function(&module, "forward.source");
    assert!(
        forward.contains("call ptr @wf_item_at.source(ptr %wf.result,"),
        "{forward}"
    );
    assert!(
        forward.contains(".result.source\n") && !forward.contains("ret ptr %wf.result"),
        "{forward}"
    );
    assert!(
        calls_source(emitted_function(&module, "read_forwarded"), "forward"),
        "{module}"
    );
    // A call whose result is both returned and read on another path is not
    // forwarded, and a return is not a read in place: the caller keeps the
    // public symbol.
    let choose = emitted_function(&module, "choose");
    assert!(
        calls_public(choose, "item_at") && !calls_source(choose, "item_at"),
        "{choose}"
    );
    // The element path returns the element's address and copies nothing.
    let item_at = emitted_function(&module, "item_at.source");
    assert!(
        !item_at.contains("; layout-bounded transfer ") && !item_at.contains("ret ptr %wf.result"),
        "{item_at}"
    );
    // A source path and a built path join: each returns its own address.
    let either = emitted_function(&module, "item_or_default.source");
    assert!(either.contains("ret ptr %wf.result"), "{either}");
    assert_eq!(either.matches("  ret ptr ").count(), 2, "{either}");
    // Writing the source after reading it leaves only the built path: the
    // source entry calls the ordinary body and returns the destination.
    let cleared = emitted_function(&module, "take_and_clear.source");
    assert!(
        cleared.contains("call void @wf_take_and_clear(ptr %wf.result,")
            && cleared.contains("ret ptr %wf.result"),
        "{cleared}"
    );
    let built = emitted_function(&module, "take_and_clear");
    assert!(!built.contains(".source("), "{built}");
    // The public symbol transfers only when the address is not its own
    // destination.
    let public = emitted_function(&module, "item_at");
    let branch = public
        .find("  br i1 %wf.built, label %done, label %transfer\n")
        .expect("the public entry tests the returned address");
    assert!(
        public.contains("%wf.built = icmp eq ptr %wf.source, %wf.result"),
        "{public}"
    );
    let transfer = public.find("\ntransfer:\n").expect("a transfer block");
    let copy = public
        .find("  ; layout-bounded transfer from %wf.source to %wf.result")
        .expect("the public entry transfers from the source");
    assert!(branch < transfer && transfer < copy, "{public}");
    // The returned address is nonnull and aligned, never noalias or
    // dereferenceable; an element behind the Box owner leaves the
    // reference uncaptured.
    let no_capture = crate::toolchain::facts().no_capture_attribute;
    assert!(
        header(item_at).starts_with(&format!(
            "define internal nonnull align 8 ptr @wf_item_at.source(ptr %wf.result, ptr noalias nonnull {no_capture} "
        )),
        "{item_at}"
    );
    // Missing reads, stale reads, a skipped or partial transfer and a
    // forwarded local slot each change the program's exit status.
    for native in [module.clone(), retain_calls(&module)] {
        let output = compile_and_run(&native);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(
            output.stdout.is_empty() && output.stderr.is_empty(),
            "{output:?}"
        );
    }
}

#[test]
fn waiting_functions_never_call_or_define_source_entries() {
    let source = format!(
        "{ITEMS}{}",
        r#"
fn waiting_item(items: &Box<Slots<Item>>, index: u64) -> result: Item reads(items) waits {
  if index < items^.inner.len {
    let item = items^.inner[index];
    return item;
  }
  return Item(a: 0_u64, b: 0_u64, c: 0_u64, d: 0_u64);
}

fn waiting_reader(items: &Box<Slots<Item>>) -> result: u64 reads(items) waits {
  if 1_u64 < items^.inner.len {
    let item = item_at(items: items, index: 1_u64);
    let waited = waiting_item(items: items, index: 1_u64);
    let first = item.c;
    let second = waited.c;
    return first +wrap second;
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let values = box_slots_new::<Item>(capacity: 2_u64);
  if values.inner.len < values.inner.cap {
    let first = Item(a: 1_u64, b: 2_u64, c: 3_u64, d: 4_u64);
    place_back(window: &values.inner, value: first);
  }
  if values.inner.len < values.inner.cap {
    let second = Item(a: 11_u64, b: 12_u64, c: 13_u64, d: 14_u64);
    place_back(window: &values.inner, value: second);
  }
  let observed = waiting_reader(items: &values);
  if observed != 26_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#
    );
    let module = emit(source.as_bytes());
    // A waiting caller reads its results from its own storage: its frame
    // can outlive the source the entry would return.
    let reader = emitted_function(&module, "waiting_reader");
    assert!(
        calls_public(reader, "item_at") && !reader.contains(".source("),
        "{reader}"
    );
    // A waiting definition is a resumable frame with no source entry.
    assert!(!module.contains("@wf_waiting_item.source"), "{module}");
    let output = compile_and_run(&module);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

/// Two programs whose callee differs only in its body: one returns its
/// referent's field at the source, the other builds a new value.
fn holder_program(body: &str) -> String {
    format!(
        r#"struct Item {{
  a: u64;
  b: u64;
  c: u64;
  d: u64;
}}

struct Holder {{
  tag: u64;
  item: Item;
}}

fn item_of(holder: &Holder) -> result: Item reads(holder) {{
{body}
}}

fn read_item_of(holder: &Holder) -> result: u64 reads(holder) {{
  let item = item_of(holder: holder);
  let first = item.a;
  let last = item.d;
  return first +wrap last;
}}

fn main() -> status: std::process::ExitStatus pure {{
  let inner = Item(a: 1_u64, b: 2_u64, c: 3_u64, d: 4_u64);
  let holder = Holder(tag: 9_u64, item: inner);
  let observed = read_item_of(holder: &holder);
  if observed != 5_u64 {{
    return std::process::exit_status(code: 1_u8);
  }}
  return std::process::exit_status(code: 0_u8);
}}
"#
    )
}

#[test]
fn a_callers_fragment_depends_only_on_its_callees_signature() {
    let sourced = holder_program("  let item = holder^.item;\n  return item;");
    let built = holder_program(
        "  let item = holder^.item;\n  return Item(a: item.a, b: item.b, c: item.c, d: item.d);",
    );
    let lower = |source: &str| {
        let inputs = [crate::SourceInput::new("test.wf", source.as_bytes())];
        crate::compile_with_overlap(
            &inputs,
            crate::CompilerLimits::default(),
            crate::OverlapLowering::Off,
        )
        .expect("the fragment fixture compiles")
    };
    let modules = [lower(&sourced), lower(&built)];
    // The returned referent is captured only through the return; the built
    // body's entry captures nothing, so only the signature-derived
    // declaration can keep the caller's fragment the same.
    let whole = modules[0].as_str();
    let entry = header(emitted_function(whole, "item_of.source"));
    if crate::toolchain::facts().no_capture_attribute == "captures(none)" {
        assert!(
            entry.contains("captures(ret: address, provenance)"),
            "{entry}"
        );
    } else {
        assert!(!entry.contains("nocapture"), "{entry}");
    }
    assert!(
        entry.starts_with("define internal nonnull align 8 ptr @wf_item_of.source("),
        "{entry}"
    );
    assert!(
        header(emitted_function(modules[1].as_str(), "item_of.source"))
            .contains(crate::toolchain::facts().no_capture_attribute),
        "{}",
        modules[1].as_str()
    );
    let split = modules
        .iter()
        .map(|module| {
            crate::split_module(module, crate::FragmentGranularity::Function)
                .expect("the module splits")
        })
        .collect::<Vec<_>>();
    let defining = |fragments: &[String], symbol: &str| -> String {
        fragments
            .iter()
            .find(|fragment| {
                fragment
                    .lines()
                    .any(|line| line.starts_with("define ") && line.contains(symbol))
            })
            .unwrap_or_else(|| panic!("no fragment defines {symbol}"))
            .clone()
    };
    let callers = [
        defining(&split[0], " @wf_read_item_of("),
        defining(&split[1], " @wf_read_item_of("),
    ];
    assert_eq!(callers[0], callers[1]);
    let has_line = |text: &str, start: &str, symbol: &str| {
        text.lines()
            .any(|line| line.starts_with(start) && line.contains(symbol))
    };
    assert!(
        callers[0].contains("call ptr @wf_item_of.source(")
            && has_line(
                &callers[0],
                "declare ",
                " nonnull align 8 ptr @wf_item_of.source("
            ),
        "{}",
        callers[0]
    );
    // The callee's fragment holds both entries, whichever body it has.
    for fragments in &split {
        let callee = defining(fragments, " @wf_item_of(");
        assert!(
            has_line(
                &callee,
                "define ",
                " nonnull align 8 ptr @wf_item_of.source("
            ),
            "{callee}"
        );
    }
    // Only the callee's own fragment changed.
    assert_eq!(split[0].len(), split[1].len());
    let changed = split[0]
        .iter()
        .zip(&split[1])
        .filter(|(before, after)| before != after)
        .count();
    assert_eq!(changed, 1, "{:#?} {:#?}", split[0], split[1]);
    for module in &modules {
        let output = compile_and_run(module.as_str());
        assert_eq!(output.status.code(), Some(0), "{output:?}");
    }
}
