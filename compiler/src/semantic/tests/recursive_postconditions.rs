//! Same-component induction hypotheses preserve the ordinary call boundary.

use super::postconditions::assert_complete;
use super::{assert_rule_kind, with_semantics_dark};
use crate::{SemanticIssueKind, SemanticOutcome, SemanticRule};

const CALLBACK: &str = r#"interface Identity {
  fn get(value: i32) -> result: i32 pure contract {
    ensures result == value;
  };
}

fn apply<interface Identity>(value: i32) -> result: i32 pure contract {
  ensures result == value;
} {
  let selected = Identity::get(value: value);
  return selected;
}

fn actual(value: i32) -> result: i32 pure contract {
  ensures result == value;
} {
  cycle(value: value);
  return value;
}

binding Selected : Identity {
  get = actual;
}

fn cycle(value: i32) -> result: unit pure {
  let ignored = apply::<Selected>(value: value);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let ignored = apply::<Selected>(value: 1_i32);
  return std::process::exit_status(code: 0_u8);
}
"#;

#[test]
fn a_failed_actual_prevents_its_recursive_formal_component_from_publishing() {
    let source = CALLBACK.replace("  return value;", "  return 0_i32;");
    assert_rule_kind(source.as_bytes(), SemanticRule::Fn9, |kind| {
        matches!(kind, SemanticIssueKind::UndischargedPostcondition(_))
    });
    with_semantics_dark(source.as_bytes(), |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("dark analysis retains failed component evidence: {outcome:?}");
        };
        let actual = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "actual")
            .expect("actual callback");
        let component = checked
            .data
            .postcondition_schedule
            .components
            .iter()
            .find(|component| component.functions.contains(&actual.id))
            .expect("callback component");
        assert!(component.functions.len() >= 3);
        assert!(component.summaries.is_empty());
        for id in &component.functions {
            let function = &checked.data.functions[id.0 as usize];
            assert!(
                function
                    .entailment
                    .postconditions
                    .iter()
                    .all(|proof| proof.summary.is_none())
            );
        }
    });
}

#[test]
fn recursive_generic_instances_prove_their_written_relations() {
    let source = br#"fn repeat<T: Int>(value: T, depth: u64) -> result: T pure contract {
  ensures result == value;
} {
  if 0_u64 < depth {
    let next_depth = depth - 1_u64;
    let inner = repeat::<T>(value: value, depth: next_depth);
    return inner;
  }
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  let small = repeat::<i32>(value: 7_i32, depth: 2_u64);
  let wide = repeat::<u64>(value: 9_u64, depth: 2_u64);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_complete(source);
    with_semantics_dark(source, |outcome| {
        let SemanticOutcome::Complete(checked) = outcome else {
            panic!("recursive generic hypotheses must verify: {outcome:?}");
        };
        let instances = checked
            .data
            .executable_functions()
            .filter(|function| function.name == "repeat")
            .collect::<Vec<_>>();
        assert_eq!(instances.len(), 2);
        for function in instances {
            for proof in &function.entailment.postconditions {
                let summary = proof.summary.as_ref().expect("verified instance summary");
                assert_eq!(summary.function, function.id);
                let component =
                    &checked.data.postcondition_schedule.components[summary.component as usize];
                assert!(component.functions.contains(&function.id));
            }
        }
    });
}
