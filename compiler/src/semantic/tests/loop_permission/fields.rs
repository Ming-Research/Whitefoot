//! PAR-2 reads of a reference's fields beside element writes of another of
//! its fields: a sibling field's read path is disjoint from the mapped root
//! [PAR-2, OWN-7], while the field itself, the whole referent and a shifted
//! element map still overlap it.

use super::*;

const SIBLING_FIELD_MAP: &str = r#"struct Common {
  scale: u64;
  table: Box<Array<u64>>;
}

struct State {
  one: Box<Array<u64>>;
  shared: Common;
}

fn fill_scalar(state: &State) -> result: unit writes(state) {
  let n = state^.one.inner.len;
  for (i in 0_u64..n) {
    let s = state^.shared.scale;
    set state^.one.inner[i] = i +wrap s;
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;

#[test]
fn scalar_field_reads_do_not_overlap_sibling_element_maps() {
    let judged = permitted(SIBLING_FIELD_MAP.as_bytes(), "fill_scalar");
    assert_eq!(
        judged.actualization,
        Some(LoopActualization::IndependentMap)
    );
    assert!(judged.indexed.is_empty());
}

#[test]
fn sibling_measure_hoisted_and_owned_field_reads_remain_permitted() {
    for (name, source) in [
        (
            "measure",
            SIBLING_FIELD_MAP.replace("state^.shared.scale", "state^.shared.table.inner.len"),
        ),
        (
            "hoisted",
            SIBLING_FIELD_MAP
                .replace("    let s = state^.shared.scale;\n", "")
                .replace("  for (i", "  let s = state^.shared.scale;\n  for (i"),
        ),
        (
            "owned",
            SIBLING_FIELD_MAP
                .replace("state: &State", "state: State")
                .replace("writes(state)", "pure")
                .replace("state^.", "state."),
        ),
    ] {
        let judged = permitted(source.as_bytes(), "fill_scalar");
        assert_eq!(
            judged.actualization,
            Some(LoopActualization::IndependentMap),
            "{name}"
        );
    }
}

#[test]
fn writing_the_read_scalar_field_still_denies_the_loop() {
    let source = SIBLING_FIELD_MAP.replace(
        "    let s = state^.shared.scale;",
        "    let s = state^.shared.scale;\n    set state^.shared.scale = i;",
    );
    assert!(matches!(
        denied(source.as_bytes(), "fill_scalar", 2),
        LoopDenial::SharedWrite { .. }
    ));
}

#[test]
fn a_whole_referent_read_still_overlaps_its_element_map() {
    let source = SIBLING_FIELD_MAP
        .replace("Box<Array<u64>>", "Array<u64, 4>")
        .replace(".inner", "")
        .replace(
            "    let s = state^.shared.scale;",
            "    let snapshot = state^;\n    let s = snapshot.shared.scale;",
        );
    assert!(matches!(
        denied(source.as_bytes(), "fill_scalar", 2),
        LoopDenial::SharedWrite { .. }
    ));
}

#[test]
fn a_shifted_read_of_the_written_field_still_denies_the_loop() {
    let source = SIBLING_FIELD_MAP.replace(
        "    let s = state^.shared.scale;\n    set state^.one.inner[i] = i +wrap s;",
        "    let next = i +wrap 1_u64;\n    if next < n {\n      let s = state^.one.inner[next];\n      set state^.one.inner[i] = i +wrap s;\n    }",
    );
    assert!(matches!(
        denied(source.as_bytes(), "fill_scalar", 2),
        LoopDenial::SharedWrite { .. }
    ));
}

#[test]
fn certified_scatter_keeps_scalar_paths_and_element_read_carriers() {
    let source = br#"struct Cell {
  value: u64;
}

struct Common {
  scale: u64;
}

struct State {
  one: Box<Array<Cell>>;
  shared: Common;
}

fn scatter(order: &[u64], pos: &[u64], state: &State) -> result: unit reads(order), writes(state) contract {
  requires pos^.len == state^.one.inner.len;
  requires forall inv(k in 0_u64..order^.len) when order^[k] < state^.one.inner.len: pos^[order^[k]] == k;
} {
  let count = order^.len;
  for (
    k in 0_u64..count,
    apart(i, j) {
    }
  ) {
    let s = state^.shared.scale;
    let e = order^[k];
    if e < state^.one.inner.len {
      let cell = &state^.one.inner[e];
      let old = cell^.value;
      set cell^.value = old +wrap s;
    }
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    with_semantics(source, |outcome| {
        let SemanticOutcome::Complete(program) = outcome else {
            panic!("the left-inverse certificate must discharge: {outcome:?}");
        };
        let function = program
            .data
            .functions
            .iter()
            .find(|f| f.name == "scatter")
            .unwrap();
        let [certificate] = function.range_facts.certified.as_slice() else {
            panic!("one retained certificate");
        };
        assert_eq!(certificate.writes.len(), 1);
        assert_eq!(certificate.reads.len(), 1);
        let judged = only_loop(&program.data.permission, "scatter");
        assert_eq!(judged.verdict, LoopVerdict::PermittedEligible, "{judged:?}");
        assert_eq!(
            judged.actualization,
            Some(LoopActualization::IndependentMap)
        );
    });
}
