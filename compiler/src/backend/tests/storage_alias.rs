//! Checked access roles preserve conservative reads and storage transitions.

use super::{
    compile, compile_and_run, compile_rejection, emitted_body, emitted_function, emitted_prelude_row,
};

#[test]
fn readonly_owner_accesses_are_separate_from_another_windows_header_writes() {
    let source = br#"struct ByteOwner {
  bytes: Box<Slots<u8>>;
}

fn copy_owners(source: &ByteOwner, destination: &Box<Slots<u8>>) -> result: unit reads(source), writes(destination) contract {
  requires destination^.inner.len == 0_u64;
  requires source^.bytes.inner.len <= destination^.inner.cap;
} {
  let count = source^.bytes.inner.len;
  for (
    i in 0_u64..count,
    invariant length: destination^.inner.len == i
  ) {
    place_back(window: &destination^.inner, value: source^.bytes.inner[i]);
  }
  return unit;
}

fn read_empty_owners(first: &Box<Slots<u8>>, second: &Box<Slots<u8>>) -> result: u64 reads(first), reads(second) {
  return first^.inner.len +wrap second^.inner.len;
}

fn main() -> status: std::process::ExitStatus pure {
  let input = box_slots_new::<u8>(capacity: 16_u64);
  for (
    i in 0_u64..16_u64,
    invariant length: input.inner.len == i
  ) {
    place_back(window: &input.inner, value: 7_u8);
  }
  let owner = ByteOwner(bytes: move input);
  let destination = box_slots_new::<u8>(capacity: 16_u64);
  copy_owners(source: &owner, destination: &destination);
  if destination.inner.len != 16_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  for (i in 0_u64..destination.inner.len) {
    if destination.inner[i] != 7_u8 {
      return std::process::exit_status(code: 2_u8);
    }
  }
  let empty_bytes = box_slots_new::<u8>(capacity: 0_u64);
  let empty = ByteOwner(bytes: move empty_bytes);
  let empty_destination = box_slots_new::<u8>(capacity: 0_u64);
  copy_owners(source: &empty, destination: &empty_destination);
  if read_empty_owners(first: &empty.bytes, second: &empty_destination) != 0_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    let copy = emitted_function(&llvm, "copy_owners");
    let load = copy
        .lines()
        .find(|line| line.contains("load i64, ptr ") && line.contains(", !alias.scope !"))
        .expect("nested readonly Slots header load carries its owner scope");
    let scope = load
        .split(", !alias.scope ")
        .nth(1)
        .expect("scope attachment");
    assert!(
        copy.lines().any(|line| {
            line.contains("load i8, ptr ")
                && line.ends_with(&format!(", !alias.scope {scope}"))
        }),
        "the source byte read must be separated from header writes too: {copy}"
    );
    let write = copy
        .lines()
        .find(|line| line.contains("store i64 ") && line.contains(", !tbaa !15, !noalias !"))
        .expect("the expanded primitive writes the other Slots header");
    let write_scope = write
        .split(", !noalias ")
        .nth(1)
        .expect("write scope attachment");
    let definition = |node: &str| {
        llvm.lines()
            .find_map(|line| line.strip_prefix(&format!("{node} = ")))
            .expect("scope list definition")
    };
    assert_eq!(
        definition(scope),
        definition(write_scope),
        "the write excludes the actual source scope"
    );
    assert!(
        !copy.lines().any(|line| line.contains("call ") && line.contains("@wf_place_back")),
        "the access fact belongs to writes, not to a whole call: {copy}"
    );
    assert!(
        copy.contains(&format!(
            "call void @llvm.experimental.noalias.scope.decl(metadata {scope})"
        )),
        "each activation declares its actual load scope: {copy}"
    );
    assert!(
        !copy.lines().any(|line| {
            (line.contains("load ") || line.contains("call ")) && line.contains(", !noalias ")
        }),
        "read/read and whole-call aliasing remain conservative: {copy}"
    );
    let reads = emitted_function(&llvm, "read_empty_owners");
    assert!(
        !reads.contains("!alias.scope") && !reads.contains("!noalias"),
        "two readonly owners may share the static empty header: {reads}"
    );
    let overlapping = std::str::from_utf8(source).expect("test source").replace(
        "copy_owners(source: &empty, destination: &empty_destination)",
        "copy_owners(source: &empty, destination: &empty.bytes)",
    );
    assert_eq!(
        compile_rejection(overlapping.as_bytes()).rule_id(),
        Some("EFF-5"),
        "the scope exclusion rests on checked argument disjointness"
    );
    for (before, after) in [
        (
            "reads(source), writes(destination)",
            "reads(source.bytes), writes(destination)",
        ),
        (
            "reads(source), writes(destination)",
            "reads(source), writes(destination.inner.next), writes(destination.inner.len)",
        ),
    ] {
        let narrowed = std::str::from_utf8(source)
            .expect("test source")
            .replace(before, after);
        let narrow_llvm = compile(narrowed.as_bytes());
        let narrow_copy = emitted_function(&narrow_llvm, "copy_owners");
        assert!(
            !narrow_copy.contains("!alias.scope") && !narrow_copy.contains("!noalias"),
            "both complete-state read and write coverage are required: {narrow_copy}"
        );
    }
    // Incorrect exclusions or primitive expansion can leave the destination
    // length below 16 (code 1) or corrupt a copied byte (code 2). Zero-trip
    // calls exercise two owners of the shared empty header too.
    for module in [&llvm, &super::owned_places::retain_calls(&llvm)] {
        let output = compile_and_run(module);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
    }
}

#[test]
fn ring_head_read_can_share_an_owner_with_a_back_append() {
    let source = br#"fn observe_and_append(source: &Box<Ring<u8>>, target: &Box<Ring<u8>>) -> result: u64 reads(source.inner.head), writes(target.inner.next), writes(target.inner.len) contract {
  requires target^.inner.len < target^.inner.cap;
  ensures target^.inner.len == entry(target)^.inner.len + 1_u64;
} {
  let observed = source^.inner.head;
  place_back(window: &target^.inner, value: 7_u8);
  return observed;
}

fn main() -> status: std::process::ExitStatus pure {
  let window = box_ring_new::<u8>(capacity: 17_u64);
  place_back(window: &window.inner, value: 7_u8);
  for (
    i in 0_u64..16_u64,
    invariant length: window.inner.len == 1_u64
  ) {
    let observed = observe_and_append(source: &window, target: &window);
    if observed != i {
      return std::process::exit_status(code: 1_u8);
    }
    if window.inner.head != i {
      return std::process::exit_status(code: 2_u8);
    }
    if window.inner.len != 2_u64 {
      return std::process::exit_status(code: 3_u8);
    }
    if window.inner[0_u64] != 7_u8 {
      return std::process::exit_status(code: 4_u8);
    }
    if window.inner[1_u64] != 7_u8 {
      return std::process::exit_status(code: 5_u8);
    }
    let removed = take_front(window: &window.inner);
    if removed != 7_u8 {
      return std::process::exit_status(code: 6_u8);
    }
  }
  if window.inner.head != 16_u64 {
    return std::process::exit_status(code: 7_u8);
  }
  let final_value = take_back(window: &window.inner);
  if final_value != 7_u8 {
    return std::process::exit_status(code: 8_u8);
  }
  if window.inner.head != 16_u64 {
    return std::process::exit_status(code: 9_u8);
  }
  if window.inner.len != 0_u64 {
    return std::process::exit_status(code: 10_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    let observe = emitted_function(&llvm, "observe_and_append");
    let head_reads = observe
        .lines()
        .filter(|line| line.contains("load i64, ptr ") && line.contains("!tbaa !18"))
        .collect::<Vec<_>>();
    assert!(
        !head_reads.is_empty(),
        "the narrow head read must remain: {observe}"
    );
    assert!(
        head_reads.iter().all(|line| !line.contains("!alias.scope")),
        "a head-only row cannot scope every generated descendant access: {observe}"
    );
    assert!(
        !observe.contains("!noalias"),
        "partial write coverage cannot exclude another owner's complete scope: {observe}"
    );
    for row in ["place_back", "take_back"] {
        let body = emitted_prelude_row(&llvm, row);
        assert!(
            body.lines()
                .any(|line| line.contains("store i64 ") && line.ends_with("!tbaa !16")),
            "the back operation must still update Ring length: {body}"
        );
        assert!(
            !body
                .lines()
                .any(|line| line.contains("store i64 ") && line.contains("!tbaa !18")),
            "a back operation must not rewrite the unchanged Ring head: {body}"
        );
    }
    // The same Box reaches both formals. A stale head produces code 1 or 2;
    // corrupt boundary or element updates produce codes 3 through 10. The IR
    // assertions additionally catch metadata UB even when an unchanged-value
    // head rewrite happens not to change this native execution's result.
    for module in [&llvm, &super::owned_places::retain_calls(&llvm)] {
        let output = compile_and_run(module);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
    }
}

#[test]
fn a_filled_range_read_can_share_an_owner_with_a_back_append() {
    let source = br#"fn append_range_length(source: &Box<Slots<u64>>, target: &Box<Slots<u64>>, lo: u64, hi: u64) -> result: unit reads(source.inner[lo..hi]), writes(target.inner.next), writes(target.inner.len) contract {
  requires lo <= hi;
  requires hi <= source^.inner.len;
  requires target^.inner.len < target^.inner.cap;
  ensures target^.inner.len == entry(target)^.inner.len + 1_u64;
} {
  let run = &source^.inner[lo..hi];
  let count = run^.len;
  place_back(window: &target^.inner, value: count);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let window = box_slots_new::<u64>(capacity: 18_u64);
  place_back(window: &window.inner, value: 11_u64);
  place_back(window: &window.inner, value: 22_u64);
  for (
    i in 2_u64..18_u64,
    invariant length: window.inner.len == i
  ) {
    append_range_length(source: &window, target: &window, lo: 0_u64, hi: i);
    if window.inner[i] != i {
      return std::process::exit_status(code: 1_u8);
    }
    if window.inner[0_u64] != 11_u64 {
      return std::process::exit_status(code: 2_u8);
    }
    if window.inner[1_u64] != 22_u64 {
      return std::process::exit_status(code: 3_u8);
    }
  }
  if window.inner.len != 18_u64 {
    return std::process::exit_status(code: 4_u8);
  }
  for (i in 2_u64..18_u64) {
    if window.inner[i] != i {
      return std::process::exit_status(code: 5_u8);
    }
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    let append = emitted_function(&llvm, "append_range_length");
    let lengths = append
        .lines()
        .filter(|line| line.contains("load i64, ptr ") && line.contains("!tbaa !15"))
        .collect::<Vec<_>>();
    assert!(
        !lengths.is_empty(),
        "range formation currently loads the complete source window length: {append}"
    );
    assert!(
        lengths.iter().all(|line| !line.contains("!alias.scope")),
        "the implicit full-length load is outside reads(source.inner[lo..hi]): {append}"
    );
    assert!(
        !append.contains("!noalias"),
        "the source header can be the written target header: {append}"
    );
    // WIN-2 separates the filled prefix from next, but not the implicit
    // descriptor access from the len write. Native execution checks every
    // appended count (codes 1/5), the prefix (2/3), and final length (4).
    // The preliminary full-length load is later replaced by hi - lo: the
    // metadata absence, not a predicted native miscompile, detects the old UB.
    for module in [&llvm, &super::owned_places::retain_calls(&llvm)] {
        let output = compile_and_run(module);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
    }
}

const COPY: &[u8] = br#"fn copy_windows(source: &Box<Array<u8>>, destination: &Box<Slots<u8>>) -> result: unit reads(source), writes(destination) contract {
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
  copy_windows(source: &source, destination: &destination);
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
    let copy = emitted_function(&llvm, "copy_windows");
    assert!(
        copy.lines().any(|line| {
            line.contains("load ptr, ptr ") && line.ends_with("!tbaa !13")
        }),
        "projecting through the source Box slot has its own role: {copy}"
    );
    assert!(
        copy.lines().any(|line| {
            line.contains("load i64, ptr ")
                && line.split(", ").any(|attachment| attachment == "!tbaa !14")
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
    let source = br#"fn append_inline_row(rows: &Array<Slots<u8, 2>, 1>) -> result: unit writes(rows) contract {
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
  let initial = Pair(left: 3_u64, right: 5_u64);
  place_back(window: &values.inner, value: initial);
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
fn paged_directory_relocation_preserves_looping_field_accesses() {
    let source = br#"struct Pair {
  left: u64;
  right: u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = box_paged_new::<Pair>(capacity: 0_u64);
  for (
    i in 0_u64..1025_u64,
    invariant length: values.inner.len == i,
    invariant capacity: values.inner.cap == i
  ) {
    let next = i + 1_u64;
    grow_paged(cell: &values, capacity: next);
    let initial = Pair(left: 0_u64, right: 0_u64);
    place_back(window: &values.inner, value: initial);
    let left = i +wrap 17_u64;
    let right = i +wrap 29_u64;
    set values.inner[i].left = left;
    set values.inner[i].right = right;
    if values.inner[i].left != left {
      return std::process::exit_status(code: 1_u8);
    }
    if values.inner[i].right != right {
      return std::process::exit_status(code: 2_u8);
    }
    set values.inner[0_u64].left = i;
    set values.inner[0_u64].right = next;
    if values.inner[0_u64].left != i {
      return std::process::exit_status(code: 3_u8);
    }
    if values.inner[0_u64].right != next {
      return std::process::exit_status(code: 4_u8);
    }
  }
  for (i in 1_u64..1025_u64) {
    let expected_left = i +wrap 17_u64;
    let expected_right = i +wrap 29_u64;
    if values.inner[i].left != expected_left {
      return std::process::exit_status(code: 5_u8);
    }
    if values.inner[i].right != expected_right {
      return std::process::exit_status(code: 6_u8);
    }
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    let main = emitted_body(&llvm, "main");
    for (instruction, tag) in [
        ("load ptr, ptr ", "!tbaa !13"),
        ("load ptr, ptr ", "!tbaa !20"),
        ("store i64 ", "!tbaa !12"),
    ] {
        assert!(
            main.lines()
                .any(|line| line.contains(instruction) && line.ends_with(tag)),
            "loop access {instruction} must carry {tag}: {main}"
        );
    }
    let growth = emitted_prelude_row(&llvm, "grow_paged");
    assert!(
        growth.contains("call void @llvm.memmove."),
        "growth relocates the initialized directory: {growth}"
    );
    // Pair's two u64 fields occupy 16 bytes, so 1025 elements cross five
    // 4096-byte pages and outgrow the directory's 1/2/4-word reservations.
    // A stale owner/directory after relocation can fault, leave a new field
    // zero (codes 1/2), or write/read the wrong old page (codes 3..6).
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

fn write_payload(target: &Choice, value: u64) -> result: unit writes(target) {
  match target^ {
    Owner(value: cell) => {
      set cell^.inner = value;
      return unit;
    }
    Number(value: number) => {
      set number^ = value;
      return unit;
    }
    Signed(..) => {
      return unit;
    }
    Empty() => {
      return unit;
    }
  }
}

fn read_payload(target: &Choice) -> result: u64 reads(target) {
  match target^ {
    Owner(value: cell) => {
      return cell^.inner;
    }
    Number(value: number) => {
      return number^;
    }
    Signed(..) => {
      return 0_u64;
    }
    Empty() => {
      return 0_u64;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let target = make(value: 0_u64);
  for (round in 0_u64..64_u64) {
    let number = make(value: round);
    replace_choice(target: &target, value: move number);
    let initial_observed = read_payload(target: &target);
    if initial_observed != round {
      return std::process::exit_status(code: 1_u8);
    }
    let scalar = round +wrap 11_u64;
    write_payload(target: &target, value: scalar);
    let scalar_observed = read_payload(target: &target);
    if scalar_observed != scalar {
      return std::process::exit_status(code: 2_u8);
    }
    let boxed = round +wrap 37_u64;
    let cell = box_new::<u64>(value: boxed);
    let owner = Choice::Owner(value: move cell);
    replace_choice(target: &target, value: move owner);
    let owner_observed = read_payload(target: &target);
    if owner_observed != boxed {
      return std::process::exit_status(code: 3_u8);
    }
    let changed = round +wrap 53_u64;
    write_payload(target: &target, value: changed);
    let changed_observed = read_payload(target: &target);
    if changed_observed != changed {
      return std::process::exit_status(code: 4_u8);
    }
    let restored = round +wrap 71_u64;
    let number_again = make(value: restored);
    replace_choice(target: &target, value: move number_again);
    let restored_observed = read_payload(target: &target);
    if restored_observed != restored {
      return std::process::exit_status(code: 5_u8);
    }
    let last = round +wrap 97_u64;
    write_payload(target: &target, value: last);
    let last_observed = read_payload(target: &target);
    if last_observed != last {
      return std::process::exit_status(code: 6_u8);
    }
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    let choice = super::system::with_ir(source, |program| {
        let ty = program
            .functions()
            .iter()
            .find(|function| function.name() == "make")
            .expect("Choice constructor")
            .result();
        let crate::IrType::Nominal(id) = ty else {
            panic!("Choice is nominal");
        };
        assert!(
            crate::target::is_union_enum(program.nominals(), program.elements(), id)
                .expect("union eligibility"),
            "the scalar and owner must reuse one payload area"
        );
        program.nominal(id).expect("Choice").link_name().to_owned()
    });
    for (tag, field) in [(0, "ptr"), (1, "i64")] {
        assert!(
            llvm.contains(&format!("%wf.t.{choice}.v{tag} = type {{ i32, {field} }}")),
            "both variant views put their word after the same tag: {llvm}"
        );
    }
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
    let write = emitted_function(&llvm, "write_payload");
    assert!(
        write.lines().any(|line| {
            line.contains("store i64 ") && line.ends_with("!tbaa !12")
        }),
        "a scalar payload write participates in the sibling relation: {write}"
    );
    for name in ["write_payload", "read_payload"] {
        let body = emitted_function(&llvm, name);
        assert!(
            body.lines().any(|line| {
                line.contains("load ptr, ptr ") && line.ends_with("!tbaa !13")
            }),
            "the reused payload is also accessed as a Box owner: {body}"
        );
    }
    // If a replacement were a scalar-only or Box-only clobber, stale payload
    // bytes could become an invalid owner, or the restored Number could still
    // expose the old pointer. The six checks observe both transitions and the
    // writes on either side in each iteration, rather than only their IR.
    let output = compile_and_run(&llvm);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
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
    let owner_definitions: Vec<_> = whole
        .as_str()
        .lines()
        .filter(|line| {
            line.strip_prefix('!')
                .and_then(|line| line.split_once(" = "))
                .and_then(|(index, _)| index.parse::<usize>().ok())
                .is_some_and(|index| index >= 22)
        })
        .collect();
    assert!(!owner_definitions.is_empty(), "the fixture emits owner scopes");
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
            for definition in &owner_definitions {
                assert!(
                    fragment.contains(*definition),
                    "missing owner scope definition {definition}: {fragment}"
                );
            }
            if fragment.contains("call void @llvm.experimental.noalias.scope.decl(") {
                assert!(
                    fragment.contains("declare void @llvm.experimental.noalias.scope.decl(metadata)"),
                    "a scoped function's fragment must retain the intrinsic declaration: {fragment}"
                );
            }
        }
    }
}
