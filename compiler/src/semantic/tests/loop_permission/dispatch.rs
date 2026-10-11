//! PAR-2 reads of a borrowed match's dispatch: a `match` through a reference
//! reads its referent's tag [EFF-2], so the read enters the loop footprint and
//! needs a matching element map, both when the scrutinee is written as an
//! element and when it is reached through a cross-element alias.

use super::*;

#[test]
fn borrowed_match_dispatch_requires_the_written_elements_map() {
    let source = r#"enum Flag {
  Off();
  On();
}

fn update(flags: &Array<Flag, 4>) -> result: unit writes(flags) {
  for @items (i in 0_u64..4_u64) {
    set flags^[i] = Flag::On();
    match &flags^[0_u64] {
      Off() => {
      }
      On() => {
      }
    }
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let value_match = source.replace(
        "    match &flags^[0_u64] {\n      Off() => {\n      }\n      On() => {\n      }\n    }",
        "    let observed = match &flags^[0_u64] {\n      Off() => {\n        give 0_u64;\n      }\n      On() => {\n        give 1_u64;\n      }\n    }",
    );
    for source in [source, value_match.as_str()] {
        let table = permission_of(source.as_bytes());
        let judged = only_loop(&table, "update");
        assert!(matches!(denial(judged, 2), LoopDenial::SharedWrite { .. }));
        assert_eq!(judged.actualization, None);

        let same_element = source.replace("&flags^[0_u64]", "&flags^[i]");
        let judged = permitted(same_element.as_bytes(), "update");
        assert_eq!(judged.actualization, Some(LoopActualization::IndependentMap));
    }
}

#[test]
fn borrowed_match_dispatch_through_a_cross_element_alias_is_denied() {
    let source = br#"enum Flag {
  Off();
  On();
}

fn update(flags: &Array<Flag, 4>) -> result: unit writes(flags) {
  for @items (i in 0_u64..3_u64) {
    set flags^[i] = Flag::On();
    let next = i + 1_u64;
    let selected = &flags^[next];
    match &selected^ {
      Off() => {
      }
      On() => {
      }
    }
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let table = permission_of(source);
    let judged = only_loop(&table, "update");
    assert!(matches!(denial(judged, 2), LoopDenial::SharedWrite { .. }));
    assert_eq!(judged.actualization, None);
}
