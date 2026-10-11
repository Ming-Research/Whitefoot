//! Checked access roles preserve conservative reads and storage transitions.

use super::{compile, compile_and_run, emitted_body, emitted_function, emitted_prelude_row};

const COPY: &[u8] = br#"fn copy(source: &Box<Array<u8>>, destination: &Box<Slots<u8>>) -> result: unit reads(source), writes(destination) contract {
  requires destination^.inner.len == 0_u64;
  requires source^.inner.len <= destination^.inner.cap;
} {
  let count = source^.inner.len;
  for (
    i in 0_u64..count,
    invariant length: destination^.inner.len == i
  ) {
    place_back(window: &destination^.inner, value: source^.inner[i]);
  }
  return unit;
}

fn read_word(value: &u64) -> result: u64 reads(value) {
  return value^;
}

fn write_word(value: &u64) -> result: unit writes(value) {
  set value^ = 11_u64;
  return unit;
}

fn nested(rows: &Array<Slots<u8, 2>, 1>) -> result: u64 reads(rows) {
  return rows^[0_u64].len;
}

fn capacity(values: &Box<Slots<u8>>) -> result: u64 reads(values) {
  return values^.inner.cap;
}

fn ring_measures(values: &Box<Ring<u8>>) -> result: u64 reads(values) {
  return values^.inner.head +wrap values^.inner.len;
}

fn main() -> status: std::process::ExitStatus pure {
  let source = box_array_filled::<u8>(count: 2_u64, value: 7_u8);
  let destination = box_slots_new::<u8>(capacity: 2_u64);
  copy(source: &source, destination: &destination);
  if destination.inner.len != 2_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if destination.inner[1_u64] != 7_u8 {
    return std::process::exit_status(code: 2_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;

#[test]
fn runtime_copy_accesses_distinguish_control_words_from_source_stores() {
    let llvm = compile(COPY);
    let copy = emitted_function(&llvm, "copy");
    assert!(
        copy.lines().any(|line| {
            line.contains("load ptr, ptr ") && line.ends_with("!tbaa !13")
        }),
        "projecting through the source Box slot has its own role: {copy}"
    );
    assert!(
        copy.lines().any(|line| {
            line.contains("load i64, ptr ") && line.ends_with("!tbaa !14")
        }),
        "runtime Array length is distinct from mutable Slots length: {copy}"
    );
    let append = emitted_prelude_row(&llvm, "place_back");
    for (instruction, tag) in [
        ("load i64, ptr ", "!tbaa !15"),
        ("store i64 ", "!tbaa !15"),
        ("store i8 ", "!tbaa !12"),
    ] {
        assert!(
            append
                .lines()
                .any(|line| line.contains(instruction) && line.ends_with(tag)),
            "{instruction} must carry {tag}: {append}"
        );
    }
    for (name, tags) in [
        ("capacity", &["!tbaa !17"][..]),
        ("ring_measures", &["!tbaa !16", "!tbaa !18"][..]),
    ] {
        let body = emitted_function(&llvm, name);
        for tag in tags {
            assert!(
                body.lines()
                    .any(|line| line.contains("load i64, ptr ") && line.ends_with(tag)),
                "runtime measure must carry {tag}: {body}"
            );
        }
    }
    for name in ["read_word", "nested"] {
        let body = emitted_function(&llvm, name);
        assert!(
            body.contains("load i64, ptr "),
            "observe a memory read: {body}"
        );
        assert!(
            !body.contains("!tbaa"),
            "a generic readonly word may be a measure; inline measures can be element bytes: {body}"
        );
    }
    let write = emitted_function(&llvm, "write_word");
    assert!(
        write.lines().any(|line| {
            line.contains("store i64 ") && line.ends_with("!tbaa !12")
        }),
        "an exact source numeric store carries the scalar role: {write}"
    );
    let output = compile_and_run(&llvm);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn inline_headers_can_be_element_storage_and_stay_untagged() {
    let source = br#"fn append(rows: &Array<Slots<u8, 2>, 1>) -> result: unit writes(rows) contract {
  requires rows^[0_u64].len < 2_u64;
} {
  place_back(window: &rows^[0_u64], value: 7_u8);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    let append = emitted_prelude_row(&llvm, "place_back");
    let controls = append
        .lines()
        .filter(|line| line.contains("load i64, ptr ") || line.contains("store i64 "))
        .collect::<Vec<_>>();
    assert!(
        controls.iter().any(|line| line.contains("store i64 ")),
        "observe an actual inline descriptor mutation: {append}"
    );
    assert!(
        controls.iter().all(|line| !line.contains("!tbaa")),
        "an inline window can be nested in an aggregate or element: {append}"
    );
    assert!(
        append.lines().any(|line| {
            line.contains("store i8 ") && line.ends_with("!tbaa !12")
        }),
        "an exact source element write still carries scalar data: {append}"
    );
}

const PAGED: &[u8] = br#"struct Pair {
  left: u64;
  right: u64;
}

fn change(values: &Box<Paged<Pair>>, index: u64) -> result: unit writes(values) contract {
  requires index < values^.inner.len;
} {
  set values^.inner[index].left = 17_u64;
  set values^.inner[index].right = 29_u64;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = box_paged_new::<Pair>(capacity: 1_u64);
  place_back(window: &values.inner, value: Pair(left: 3_u64, right: 5_u64));
  change(values: &values, index: 0_u64);
  if values.inner.len != 1_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  if values.inner[0_u64].left != 17_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if values.inner[0_u64].right != 29_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;

#[test]
fn paged_directory_reads_stay_distinct_from_field_stores() {
    let llvm = compile(PAGED);
    let change = emitted_function(&llvm, "change");
    assert_eq!(
        change
            .lines()
            .filter(|line| {
                line.contains("load ptr, ptr ") && line.ends_with("!tbaa !20")
            })
            .count(),
        2,
        "each field path rereads the directory before optimization: {change}"
    );
    assert_eq!(
        change
            .lines()
            .filter(|line| {
                line.trim().starts_with("store i64 ") && line.ends_with("!tbaa !12")
            })
            .count(),
        2,
        "both exact field stores use source scalar data: {change}"
    );
    let allocate = emitted_prelude_row(&llvm, "box_paged_new");
    assert!(
        allocate.lines().any(|line| {
            line.trim().starts_with("store ptr ") && line.ends_with("!tbaa !20")
        }),
        "directory writes agree with directory reads: {allocate}"
    );
    let append = emitted_prelude_row(&llvm, "place_back");
    assert!(
        append.lines().any(|line| {
            line.contains("load i64, ptr ") && line.ends_with("!tbaa !19")
        }),
        "Paged's length has its own role: {append}"
    );
    let output = compile_and_run(&llvm);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn constructors_and_whole_value_transfers_are_storage_role_barriers() {
    let source = br#"nocopy enum Choice {
  Owner(value: Box<u64>);
  Number(value: u64);
  Signed(value: i64);
  Empty();
}

fn make(value: u64) -> result: Choice pure {
  return Choice::Number(value: value);
}

fn replace_choice(target: &Choice, value: Choice) -> result: unit writes(target) {
  set target^ = move value;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    let make = emitted_body(&llvm, "make");
    assert!(
        make.contains("store "),
        "observe actual constructor writes: {make}"
    );
    assert!(
        !make.contains("!tbaa"),
        "variant construction changes storage role: {make}"
    );
    let replace = emitted_function(&llvm, "replace_choice");
    let (transfer, _, _) = super::layout_transfers::transfer_region(replace);
    assert!(
        !transfer.contains("!tbaa"),
        "whole-value transition clobbers all roles: {replace}"
    );
}

#[test]
fn storage_alias_definitions_survive_cached_fragment_roundtrips() {
    let inputs = [crate::SourceInput::new("storage-roles.wf", COPY)];
    let whole = crate::compile_for_emission(
        &inputs,
        crate::CompilerLimits::default(),
        crate::OverlapLowering::Off,
        None,
        true,
    )
    .expect("the fixture is compiled for partitionable layouts")
    .0;
    let cached = crate::LlvmModule::decode(&whole.encode()).expect("structured module roundtrip");
    assert_eq!(whole.as_str(), cached.as_str());
    for granularity in [
        crate::FragmentGranularity::Function,
        crate::FragmentGranularity::Module,
    ] {
        let original = crate::split_module(&whole, granularity).expect("whole module splits");
        let restored = crate::split_module(&cached, granularity).expect("cached module splits");
        assert_eq!(original, restored);
        for fragment in original {
            assert!(fragment.contains("!0 = !{!\"Whitefoot checked storage roles\"}"));
            for index in 1..=21 {
                assert!(
                    fragment.contains(&format!("!{index} = !{{")),
                    "missing definition !{index}: {fragment}"
                );
            }
        }
    }
}
