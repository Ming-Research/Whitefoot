//! Shared-map emission and the lock-unit ledger of an atomic statement.
//! Deliberately damaged lowering must make the amendment-S witnesses fail.
//! These checks qualify the conformance observations independently of the
//! emitted instruction sequences they damage.

use super::{compile, compile_and_run, compile_link_and_run};

/// The state of every ledger witness has a map field and a field of its own,
/// `meta`. The statement takes a unit for the map and one for the rest of the
/// state, in field order, each at its first use together with every earlier
/// unit [SHARE-3]. `m^.0`, `m^.1` name the map unit by its field index, and
/// `m^` the unit of the rest. The map's entry is used on every path, at the
/// end of the block; only the body's placement of the `meta` reads changes
/// between witnesses.
const LOCK_LEDGER_SOURCE: &str = r#"const ledger_names: Array<u8, 1> =[97_u8];

struct Meta {
  count: u64;
}

struct Ledger {
FIELDS
}

fn lock_run(state: Shared<Ledger>, kept: Bool) -> result: unit pure waits {
  let key = &ledger_names[0_u64..1_u64];
  atomic m = &stateGUARD {
BODY
    set m^.table[key] = None<u8>();
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let table = map_new::<u8>(capacity: 1_u64);
  let meta = Meta(count: 1_u64);
  let initial = CONSTRUCT;
  let state = shared_new::<Ledger>(value: move initial);
  let kept = True();
  lock_run(state: move state, kept: kept);
  return std::process::exit_status(code: 0_u8);
}
"#;

/// The map first in field order: a statement that uses only the map takes no
/// other unit, and a use of `meta` takes the map with it.
const MAP_FIRST: (&str, &str) = (
    "  table: ConcurrentHashMap<u8>;\n  meta: Meta;",
    "Ledger(table: move table, meta: meta)",
);

/// `meta` first in field order: a use of the map takes `meta` with it.
const META_FIRST: (&str, &str) = (
    "  meta: Meta;\n  table: ConcurrentHashMap<u8>;",
    "Ledger(meta: meta, table: move table)",
);

/// The witness with `body` before the map write; an empty body leaves no
/// line, since a canonical source has no blank line inside an item [FORM-2].
fn lock_ledger_source(layout: (&str, &str), guard: &str, body: &str) -> String {
    let body = if body.is_empty() {
        String::new()
    } else {
        format!("{body}\n")
    };
    LOCK_LEDGER_SOURCE
        .replace("FIELDS", layout.0)
        .replace("CONSTRUCT", layout.1)
        .replace("GUARD", guard)
        .replace("BODY\n", &body)
}

fn lock_ledger_position(source: &str, needle: &str) -> String {
    let offset = source
        .find(needle)
        .expect("the witness contains the source position");
    let before = &source[..offset];
    let line = before.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = before
        .rsplit_once('\n')
        .map_or(before.len(), |(_, tail)| tail.len())
        + 1;
    format!("test.wf:{line}:{column}")
}

fn lock_ledger_lines(module: &str) -> Vec<&str> {
    module
        .lines()
        .filter_map(|line| line.strip_prefix(crate::LOCK_LEDGER_PREFIX))
        .filter(|line| line.starts_with("LOCK lock_run  "))
        .collect()
}

/// The ledger line of the unit of the state's own part, `m^`.
fn object_lock_line(source: &str, timing: &str, first_use: &str) -> String {
    format!(
        "LOCK lock_run  {}  unit(m^): Ledger  {timing}; first use {}",
        lock_ledger_position(source, "atomic m"),
        lock_ledger_position(source, first_use),
    )
}

/// A host display path must remain within one comment, with literal
/// backslashes distinct from escapes for control characters.
#[test]
fn lock_ledger_escapes_control_characters_in_host_display_paths() {
    let source = lock_ledger_source(
        MAP_FIRST,
        "",
        "    let observed = m^.meta.count;\n    set m^.meta.count = observed;",
    );
    let inputs = [crate::SourceInput::from_host_path(
        "test.wf",
        "host\nledger_tail\r\t\u{1}\\test.wf",
        source.as_bytes(),
    )];
    let module = crate::native_test_support::timed("whitefoot-compile", || {
        crate::compile(&inputs, crate::CompilerLimits::default())
            .expect("normal compiler pipeline must emit")
            .into_string()
    });
    let lines = lock_ledger_lines(&module);
    assert_eq!(lines.len(), 2, "one line per lock unit: {lines:?}");
    let expected = object_lock_line(&source, "on every path", "let observed")
        .replace("test.wf", r"host\nledger_tail\r\t\u{1}\\test.wf");
    let comment = format!("{}{expected}", crate::LOCK_LEDGER_PREFIX);
    assert_eq!(
        module
            .lines()
            .filter(|line| *line == comment.as_str())
            .count(),
        1,
        "the escaped object path stays on one lock comment: {lines:?}"
    );
    assert!(
        !module.lines().any(|line| line.starts_with("ledger_tail")),
        "host text after a newline must not become a module line"
    );
}

/// An unconditional first read must not be described as conditional merely
/// because the emitter checks the lazy unit's taken word.
#[test]
fn lock_ledger_reports_a_metadata_read_on_every_path() {
    let source = lock_ledger_source(
        MAP_FIRST,
        "",
        "    let observed = m^.meta.count;\n    set m^.meta.count = observed;\n    if kept {\n      set m^.table[key] = Some<u8>(value: 1_u8);\n    }",
    );
    let module = compile(source.as_bytes());
    let lines = lock_ledger_lines(&module);
    assert_eq!(lines.len(), 2, "one line per lock unit: {lines:?}");
    let expected = object_lock_line(&source, "on every path", "let observed");
    assert!(
        lines.contains(&expected.as_str()),
        "an unconditional read takes the state's own unit on every path: {lines:?}"
    );
}

/// The state's own unit follows the map's in field order and is reached only
/// through the true successor of `kept`: the map's entry at the end of the
/// block does not take it. A report that promotes that use to every path must
/// fail this assertion.
#[test]
fn lock_ledger_reports_a_metadata_read_on_some_paths() {
    let source = lock_ledger_source(
        MAP_FIRST,
        "",
        "    if kept {\n      let observed = m^.meta.count;\n      set m^.meta.count = observed;\n    }",
    );
    let module = compile(source.as_bytes());
    let lines = lock_ledger_lines(&module);
    assert_eq!(lines.len(), 2, "one line per lock unit: {lines:?}");
    let expected = object_lock_line(&source, "on some paths", "let observed");
    assert!(
        lines.contains(&expected.as_str()),
        "the false successor never uses the state's own unit: {lines:?}"
    );
}

/// Units are taken in field order. The conditional read of the earlier unit,
/// `meta`, does not postpone its lock: the map's entry at the end of the
/// block takes it too, and the ledger names the map unit that does.
#[test]
fn lock_ledger_names_the_map_unit_that_takes_an_earlier_object_unit() {
    let source = lock_ledger_source(
        META_FIRST,
        "",
        "    if kept {\n      let observed = m^.meta.count;\n      set m^.meta.count = observed;\n    }",
    );
    let module = compile(source.as_bytes());
    let lines = lock_ledger_lines(&module);
    assert_eq!(lines.len(), 2, "one line per lock unit: {lines:?}");
    let conditional = lock_ledger_position(&source, "let observed");
    let entry = lock_ledger_position(&source, "set m^.table[key] = None<u8>();");
    let mut uses = [conditional, entry.clone()];
    uses.sort();
    let expected = format!(
        "LOCK lock_run  {}  unit(m^): Ledger  on every path; first use {}; taken with unit(m^.1) at {entry}",
        lock_ledger_position(&source, "atomic m"),
        uses.join(" | "),
    );
    assert!(
        lines.contains(&expected.as_str()),
        "the entry takes the earlier unit on the path that skips the read, and the report names it: {lines:?}"
    );
}

/// The guard's read requires the state's own unit before the guard, even
/// though the block itself uses it only under a condition.
#[test]
fn lock_ledger_reports_a_guard_read_before_the_guard() {
    let source = lock_ledger_source(
        MAP_FIRST,
        " when m^.meta.count != 0_u64",
        "    if kept {\n      let observed = m^.meta.count;\n      set m^.meta.count = observed;\n    }",
    );
    let module = compile(source.as_bytes());
    let lines = lock_ledger_lines(&module);
    assert_eq!(lines.len(), 2, "one line per lock unit: {lines:?}");
    let expected = object_lock_line(&source, "before the guard", "m^.meta.count !=");
    assert!(
        lines.contains(&expected.as_str()),
        "a guard read takes the unit before testing the guard: {lines:?}"
    );
}

/// A guard over a local value reads no part of the state. Its watch fallback
/// takes all units before the guard; the true guard fact makes a repeated body
/// test redundant under OP-5, so the read of `meta` runs directly.
#[test]
fn lock_ledger_reports_all_groups_before_a_guard_that_reads_no_target() {
    let source = lock_ledger_source(
        MAP_FIRST,
        " when kept",
        "    let observed = m^.meta.count;\n    set m^.meta.count = observed;",
    );
    let module = compile(source.as_bytes());
    let lines = lock_ledger_lines(&module);
    assert_eq!(lines.len(), 2, "one line per lock unit: {lines:?}");
    let expected = object_lock_line(&source, "before the guard", "atomic m");
    assert!(
        lines.contains(&expected.as_str()),
        "a guard that reads no part of the state takes every unit before testing the guard: {lines:?}"
    );
}

/// Both emitted successors acquire the state's own unit. This catches a
/// report that treats all acquisitions within source `if` statements as only
/// some paths.
#[test]
fn lock_ledger_reports_a_metadata_read_in_both_branches_on_every_path() {
    let source = lock_ledger_source(
        MAP_FIRST,
        "",
        "    if kept {\n      let observed = m^.meta.count;\n      set m^.meta.count = observed;\n    } else {\n      let other = m^.meta.count;\n      set m^.meta.count = other;\n    }",
    );
    let module = compile(source.as_bytes());
    let lines = lock_ledger_lines(&module);
    assert_eq!(lines.len(), 2, "one line per lock unit: {lines:?}");
    let object = lines
        .iter()
        .find(|line| line.contains("unit(m^): Ledger"))
        .expect("the state's own unit is reported");
    assert!(object.contains("  on every path; first use "), "{object}");
    assert!(
        object.contains(&lock_ledger_position(&source, "let observed")),
        "{object}"
    );
    assert!(
        object.contains(&lock_ledger_position(&source, "let other")),
        "{object}"
    );
}

/// A block that subscripts the map by one stable key takes that key's entry;
/// a count of the map takes the whole map. A ledger that names both map
/// units alike, or a lowering that takes the whole map for one key, fails.
#[test]
fn lock_ledger_names_an_entry_lock_and_a_whole_map_lock() {
    let map_line = |body: &str| {
        let source = lock_ledger_source(MAP_FIRST, "", body);
        let module = compile(source.as_bytes());
        lock_ledger_lines(&module)
            .into_iter()
            .find(|line| line.contains("unit(m^.0): "))
            .map(str::to_owned)
            .expect("the map unit is reported")
    };
    let entry = map_line("");
    assert!(
        entry.contains("unit(m^.0): map entry  on every path; first use "),
        "{entry}"
    );
    let whole = map_line("    set m^.meta.count = map_count::<u8>(map: &m^.table);");
    assert!(
        whole.contains("unit(m^.0): whole map  on every path; first use "),
        "{whole}"
    );
}

/// Two key bindings declared before the statement subscript the map, so no
/// one entry covers the block's reaches and the statement takes the whole
/// map, its one unit. A lowering that locks one key's entry and selects the
/// other key's entry without a lock fails.
#[test]
fn lock_ledger_takes_the_whole_map_for_two_keys() {
    let source = lock_ledger_source(
        MAP_FIRST,
        "",
        "    set m^.table[other] = Some<u8>(value: 1_u8);",
    )
    .replacen(
        "  atomic m",
        "  let other = &ledger_names[0_u64..1_u64];\n  atomic m",
        1,
    );
    let module = compile(source.as_bytes());
    let expected = format!(
        "LOCK lock_run  {}  unit(m^.0): whole map  on every path; first use {}",
        lock_ledger_position(&source, "atomic m"),
        lock_ledger_position(&source, "set m^.table[other]"),
    );
    assert_eq!(lock_ledger_lines(&module), [expected.as_str()]);
}

/// The block rebinds the key before it subscripts the map. The key the
/// statement began with need not select the entry the block names, so the
/// statement takes the whole map. A lowering that counts only storage writes
/// as writes of the key binding, so that a rebinding leaves it stable, locks
/// the entry under the old key and fails.
#[test]
fn lock_ledger_takes_the_whole_map_for_a_key_the_block_rebinds() {
    let source = lock_ledger_source(
        MAP_FIRST,
        "",
        "    set key = &ledger_names[0_u64..1_u64];",
    );
    let module = compile(source.as_bytes());
    let expected = format!(
        "LOCK lock_run  {}  unit(m^.0): whole map  on every path; first use {}",
        lock_ledger_position(&source, "atomic m"),
        lock_ledger_position(&source, "set m^.table[key] = None<u8>();"),
    );
    assert_eq!(lock_ledger_lines(&module), [expected.as_str()]);
}

/// The block writes the key's byte through the key before it subscripts the
/// map, so the bytes the statement began with need not select the entry the
/// block names, and the statement takes the whole map. A lowering that
/// counts only a rebinding, and not a write of a path the key names, locks
/// the entry under the old byte and fails.
#[test]
fn lock_ledger_takes_the_whole_map_for_a_key_the_block_writes_through() {
    let source = lock_ledger_source(MAP_FIRST, "", "    set key^[0_u64] = 98_u8;").replacen(
        "  let key = &ledger_names[0_u64..1_u64];",
        "  let bytes = array_filled::<u8, 1>(value: 97_u8);\n  let key = &bytes[0_u64..1_u64];",
        1,
    );
    let module = compile(source.as_bytes());
    let expected = format!(
        "LOCK lock_run  {}  unit(m^.0): whole map  on every path; first use {}",
        lock_ledger_position(&source, "atomic m"),
        lock_ledger_position(&source, "set m^.table[key] = None<u8>();"),
    );
    assert_eq!(lock_ledger_lines(&module), [expected.as_str()]);
}

/// A key the block rebinds selects the entry under its new bytes, `b`. A
/// statement that locks the entry under the bytes the key had when the
/// statement began writes the value under `a` instead, and the program
/// leaves status 1; a write lost altogether leaves status 2.
#[test]
fn a_key_the_block_rebinds_selects_the_entry_under_its_new_bytes() {
    let source = br#"const key_names: Array<u8, 2> =[97_u8, 98_u8];

struct Ledger {
  table: ConcurrentHashMap<u8>;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let table = map_new::<u8>(capacity: 2_u64);
  let initial = Ledger(table: move table);
  let state = shared_new::<Ledger>(value: move initial);
  let key = &key_names[0_u64..1_u64];
  atomic m = &state {
    set key = &key_names[1_u64..2_u64];
    set m^.table[key] = Some<u8>(value: 5_u8);
  }
  let first = &key_names[0_u64..1_u64];
  let under_first = None<u8>();
  let under_second = None<u8>();
  atomic m = &state {
    set under_first = m^.table[first];
    set under_second = m^.table[key];
  }
  match under_first {
    Some(value: stray) => {
      return std::process::exit_status(code: 1_u8);
    }
    None() => {
    }
  }
  match under_second {
    Some(value: stored) => {
      return std::process::exit_status(code: stored);
    }
    None() => {
    }
  }
  return std::process::exit_status(code: 2_u8);
}
"#;
    let output = compile_and_run(&compile(source));
    assert_eq!(output.status.code(), Some(5), "{output:?}");
}

/// Units are taken in field order whichever field the block uses first. With
/// the map first, the read of `meta` takes the map's entry with it, before
/// `meta`; with `meta` first, that read takes `meta` alone and the entry is
/// taken at its own use. Each ledger lists the units in field order, and a
/// lowering that takes `meta` before an earlier map, or reports the map whole
/// for its one key, fails.
#[test]
fn lock_ledger_takes_the_map_entry_and_the_other_field_in_field_order() {
    let body = "    let observed = m^.meta.count;\n    set m^.meta.count = observed;";
    let source = lock_ledger_source(MAP_FIRST, "", body);
    let module = compile(source.as_bytes());
    let site = lock_ledger_position(&source, "atomic m");
    let read = lock_ledger_position(&source, "let observed");
    let map = format!(
        "LOCK lock_run  {site}  unit(m^.0): map entry  on every path; first use {read}; taken with unit(m^) at {read}"
    );
    let object = object_lock_line(&source, "on every path", "let observed");
    assert_eq!(
        lock_ledger_lines(&module),
        [map.as_str(), object.as_str()],
        "the map first"
    );
    let source = lock_ledger_source(META_FIRST, "", body);
    let module = compile(source.as_bytes());
    let map = format!(
        "LOCK lock_run  {}  unit(m^.1): map entry  on every path; first use {}",
        lock_ledger_position(&source, "atomic m"),
        lock_ledger_position(&source, "set m^.table[key] = None<u8>();"),
    );
    let object = object_lock_line(&source, "on every path", "let observed");
    assert_eq!(
        lock_ledger_lines(&module),
        [object.as_str(), map.as_str()],
        "meta first"
    );
}

fn witness(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/conformance/cases")
            .join(format!("share-pos-map-{name}.wf")),
    )
    .expect("read the normative witness")
}

/// Compiles a witness whose statement holds a map whole, turns its write
/// selections into read selections, and requires the damaged program to leave
/// its exit code.
/// Each witness is its own test so the harness runs them in parallel.
fn assert_detects_read_selection_in_write_position(name: &str, expected: i32) {
    let llvm = compile(&witness(name));
    let mut changes = 0;
    let damaged = llvm
        .lines()
        .map(|line| {
            if line.contains("call ptr @wf__table_held_entry(") && line.ends_with("i32 1)") {
                changes += 1;
                line.replace("i32 1)", "i32 0)")
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        changes > 0,
        "{name}: the write-selection damage was applied"
    );
    let output = compile_and_run(&damaged);
    assert_ne!(output.status.code(), Some(expected), "{name}");
}

#[test]
fn whole_target_entry_detects_read_selection_in_write_position() {
    assert_detects_read_selection_in_write_position("whole-target-entry", 22);
}

#[test]
fn whole_target_alias_detects_read_selection_in_write_position() {
    assert_detects_read_selection_in_write_position("whole-target-alias", 22);
}

#[test]
fn whole_target_count_detects_read_selection_in_write_position() {
    assert_detects_read_selection_in_write_position("whole-target-count-sees-own-writes", 21);
}

#[test]
fn whole_target_callee_detects_read_selection_in_write_position() {
    assert_detects_read_selection_in_write_position("whole-target-passed-to-callee", 1);
}

#[test]
fn whole_target_guard_detects_read_selection_in_write_position() {
    assert_detects_read_selection_in_write_position("whole-target-guard", 1);
}

#[test]
fn entries_witness_detects_selection_of_one_slot_for_every_set_index() {
    let llvm = compile(&witness("whole-target-entries-over-set"));
    let mut changes = 0;
    let damaged = llvm
        .lines()
        .map(|line| {
            if line.contains("call ptr @wf__table_hold_slot(") {
                let (prefix, _) = line.rsplit_once(", i64 ").expect("slot call's index");
                changes += 1;
                format!("{prefix}, i64 0)")
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(changes > 0, "the slot-selection damage was applied");
    assert_ne!(compile_and_run(&damaged).status.code(), Some(3));
}

#[test]
fn whole_swap_witness_detects_a_missing_swap() {
    let name = "swapped";
    let llvm = compile(&witness(name));
    let mut changes = 0;
    let damaged = llvm
        .lines()
        .map(|line| {
            if line.contains("call void @wf__keyed_table_swap(") {
                changes += 1;
                "  ; deliberately omitted table swap".to_owned()
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(changes > 0, "the swap damage was applied");
    assert_ne!(compile_and_run(&damaged).status.code(), Some(22), "{name}");
}

#[test]
fn map_tag_readers_use_one_byte_without_reading_neighboring_payload() {
    let source = br#"const tag_names: Array<u8, 2> =[97_u8, 98_u8];

struct NeighborBytes {
  first: u8;
  second: u8;
  third: u8;
}

struct Stores {
  store: ConcurrentHashMap<NeighborBytes>;
  fresh: ConcurrentHashMap<NeighborBytes>;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let store_map = map_new::<NeighborBytes>(capacity: 2_u64);
  let fresh_map = map_new::<NeighborBytes>(capacity: 2_u64);
  let maps = Stores(store: move store_map, fresh: move fresh_map);
  let state = shared_new::<Stores>(value: move maps);
  let first = &tag_names[0_u64..1_u64];
  let second = &tag_names[1_u64..2_u64];
  let value = NeighborBytes(first: 129_u8, second: 193_u8, third: 251_u8);
  let initial = 0_u64;
  let remaining = 0_u64;
  let bounded_keys = key_set_new(capacity: 0_u64);
  let bounded_count = 0_u64;
  let bounded_failed = False();
  atomic t = &state {
    set t^.store[first] = Some<NeighborBytes>(value: value);
    set t^.store[second] = Some<NeighborBytes>(value: value);
    set initial = map_count::<NeighborBytes>(map: &t^.store);
    set t^.store[first] = None<NeighborBytes>();
    set remaining = map_count::<NeighborBytes>(map: &t^.store);
    let bounded = map_scan_within::<NeighborBytes>(map: &t^.store, cursor: 0_u64, count: 2_u64, limit: 0_u64, keys: &bounded_keys);
    let required = 0_u64;
    match bounded {
      Next(next: end) => {
        set bounded_failed = True();
      }
      Needs(bytes: wanted) => {
        set required = wanted;
      }
    }
    if required > 0_u64 {
      let admitted = map_scan_within::<NeighborBytes>(map: &t^.store, cursor: 0_u64, count: 2_u64, limit: required, keys: &bounded_keys);
      match admitted {
        Next(next: end) => {
          if end != 0_u64 {
            set bounded_failed = True();
          }
        }
        Needs(bytes: wanted) => {
          set bounded_failed = True();
        }
      }
    } else {
      set bounded_failed = True();
    }
    set bounded_count = bounded_keys.len;
  }
  if initial != 2_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if remaining != 1_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  if bounded_failed {
    return std::process::exit_status(code: 6_u8);
  }
  if bounded_count != 1_u64 {
    return std::process::exit_status(code: 7_u8);
  }
  atomic t = &state {
    set t^.store[second] = Some<NeighborBytes>(value: value);
  }
  let keys = key_set_new(capacity: 2_u64);
  let cursor = 0_u64;
  atomic t = &state {
    set cursor = map_scan::<NeighborBytes>(map: &t^.store, cursor: 0_u64, count: 2_u64, keys: &keys);
  }
  if keys.len != 1_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  let moved = 0_u64;
  let cleared = 1_u64;
  atomic t = &state {
    swap(first: &t^.store, second: &t^.fresh);
    set moved = map_count::<NeighborBytes>(map: &t^.fresh);
    map_clear::<NeighborBytes>(map: &t^.fresh);
    set cleared = map_count::<NeighborBytes>(map: &t^.fresh);
  }
  if moved != 1_u64 {
    return std::process::exit_status(code: 4_u8);
  }
  if cleared != 0_u64 {
    return std::process::exit_status(code: 5_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let llvm = compile(source);
    for (symbol, opcode) in [
        ("wf__keyed_table_count", "call i64"),
        ("wf__keyed_table_scan", "call i64"),
        ("wf__keyed_table_scan_within", "call i64"),
        ("wf__keyed_table_clear", "call void"),
        ("wf__keyed_table_swap", "call void"),
        ("wf__table_hold_release", "call void"),
    ] {
        let calls: Vec<_> = llvm.lines().filter(|line| {
            line.contains(&format!("{opcode} @{symbol}("))
        }).collect();
        assert!(!calls.is_empty(), "{symbol} is exercised");
        assert!(calls.iter().all(|line| line.contains(", i64 0, i32 1, i64 0")),
            "{symbol} receives tag offset 0, byte width 1 and None tag 0: {calls:?}");
    }
    let option = super::nominal_type("Option<NeighborBytes>");
    let payload = super::nominal_type("NeighborBytes");
    assert!(llvm.contains(&format!("{option} = type {{ i8, {payload} }}")), "{llvm}");
    assert!(llvm.contains(&format!("{payload} = type {{ i8, i8, i8 }}")), "{llvm}");
    assert!(llvm.contains(".tag = load i8, ptr "), "the entry unlock reads one byte: {llvm}");
    assert!(llvm.contains(".some = icmp ne i8 "), "the entry unlock compares one byte: {llvm}");
    assert!(llvm.lines().any(|line| line.contains(".present = zext i1 ") && line.ends_with(" to i32"))
        && llvm.lines().any(|line| line.contains("call void @wf__table_unlock_entry(ptr ")
            && line.contains(", i32 ")),
        "the runtime receives its unchanged i32 presence argument: {llvm}");
    // The two WF calls share one monomorphized row body. Count runtime
    // calls, and trace each one's separate needs pointer within that body.
    let mut bounded_runtime_calls = 0;
    for body in llvm.split("\n}\n") {
        for call in body.lines().filter(|line| line.contains("call i64 @wf__keyed_table_scan_within(")) {
            bounded_runtime_calls += 1;
            let (_, arguments) = call.split_once("@wf__keyed_table_scan_within(").unwrap();
            let needs = arguments.split(", ").nth(5).and_then(|arg| arg.strip_prefix("ptr "))
                .expect("the runtime's sixth argument is its separate needs pointer");
            assert!(body.contains(&format!("{needs} = alloca i32, align 4")),
                "the needs output has its unchanged i32 storage: {body}");
            let loads = body.lines().filter_map(|line| {
                line.trim().strip_suffix(&format!(" = load i32, ptr {needs}"))
            }).collect::<Vec<_>>();
            assert_eq!(loads.len(), 1, "each bounded runtime call reads its i32 needs output once: {body}");
            assert!(body.contains(&format!(" = icmp eq i32 {}, 0", loads[0])),
                "the bounded runtime call compares its separate i32 needs result: {body}");
        }
    }
    assert!(bounded_runtime_calls > 0, "the bounded scan runtime is exercised");
    // All four bytes belong to the slot. Dirtying the inactive payload of
    // None is semantically unobservable, but a stale four-byte tag reader
    // mistakes those nonzero neighbors for a present entry.
    let observer = r#"#include <stdint.h>
#include <stdio.h>
extern void *wf__table_held_entry(void *, const unsigned char *, uint64_t, uint32_t);
extern uint64_t wf__keyed_table_count(void *, uint64_t, uint32_t, uint64_t);
uint64_t wf_test_narrow_count(void *map, uint64_t offset, uint32_t width, uint64_t none) {
    static const unsigned char key[] = {97};
    unsigned char *slot = wf__table_held_entry(map, key, 1, 0);
    if (slot != NULL && slot[0] == 0) {
        slot[1] = 129;
        slot[2] = 193;
        slot[3] = 251;
        fputs("D;", stdout);
    }
    return wf__keyed_table_count(map, offset, width, none);
}
"#;
    let observed = llvm.replace("call i64 @wf__keyed_table_count(", "call i64 @wf_test_narrow_count(");
    assert_ne!(observed, llvm);
    let observed = format!("{observed}\ndeclare i64 @wf_test_narrow_count(ptr, i64, i32, i64)\n");
    let output = compile_link_and_run(&observed, Some(observer), &[]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(output.stdout, b"D;", "the inactive neighboring bytes were dirtied: {output:?}");
    // A representative wrong width must reach the intended count mismatch,
    // with no out-of-bounds read: this slot has exactly four initialized bytes.
    let damaged = observed.lines().map(|line| {
        if line.contains("call i64 @wf_test_narrow_count(") {
            line.replace(", i64 0, i32 1, i64 0)", ", i64 0, i32 4, i64 0)")
        } else {
            line.to_owned()
        }
    }).collect::<Vec<_>>().join("\n");
    assert_ne!(damaged, observed);
    let output = compile_link_and_run(&damaged, Some(observer), &[]);
    assert_eq!(output.status.code(), Some(2), "the widened read sees inactive payload bytes: {output:?}");
    assert_eq!(output.stdout, b"D;", "{output:?}");
    // Isolate the bounded scan's entry-width argument. Its runtime reads the
    // same dirty None before this hold is released; count stays correct.
    let mut scans = 0;
    let damaged = observed.lines().map(|line| {
        if line.contains("call i64 @wf__keyed_table_scan_within(") {
            scans += 1;
            let replacement = line.replace(", i64 0, i32 1, i64 0)", ", i64 0, i32 4, i64 0)");
            assert_ne!(replacement, line, "the bounded scan's byte width was damaged");
            replacement
        } else {
            line.to_owned()
        }
    }).collect::<Vec<_>>().join("\n");
    assert_eq!(scans, bounded_runtime_calls,
        "every bounded scan runtime call, shared by both WF call sites, has its entry width damaged");
    let output = compile_link_and_run(&damaged, Some(observer), &[]);
    assert_eq!(output.status.code(), Some(7),
        "the widened bounded scan includes the inactive None as a second key: {output:?}");
    assert_eq!(output.stdout, b"D;", "{output:?}");
}
