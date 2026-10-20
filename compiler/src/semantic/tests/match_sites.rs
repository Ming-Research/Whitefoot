//! ERR-2 collects sites along the ordinary structural walk and publishes no
//! checked program from its incomplete match scratch.

use crate::{SemanticIssueKind, SemanticLocation, SemanticOutcome, SemanticRule};

use super::{assert_rule, with_semantics};

pub(super) fn assert_sites(source: &str, enum_name: &str, expected: &[(&str, &[&str])]) {
    with_semantics(source.as_bytes(), |outcome| {
        let SemanticOutcome::SourceIssue { issue } = outcome else {
            panic!("expected ERR-2 without a checked program, got {outcome:?}");
        };
        assert_eq!(issue.rule(), SemanticRule::Err2);
        let SemanticIssueKind::NonExhaustiveMatch {
            missing_variants,
            other_sites,
        } = issue.kind()
        else {
            panic!("expected non-exhaustive match payload: {issue:?}");
        };
        assert_eq!(other_sites.len(), expected.len());
        assert_eq!(issue.location(), &other_sites[0].location);
        assert_eq!(missing_variants, &other_sites[0].missing_variants);
        for (site, (written, missing)) in other_sites.iter().zip(expected) {
            assert_eq!(site.enum_name, enum_name);
            assert_eq!(
                site.missing_variants,
                missing.iter().map(|name| (*name).to_owned()).collect::<Vec<_>>()
            );
            let SemanticLocation::SourceNode(_, coordinate) = &site.location;
            let start = usize::try_from(coordinate.start().value()).expect("source offset");
            let end = usize::try_from(coordinate.end().value()).expect("source offset");
            assert_eq!(
                start,
                source.find(*written).expect("expected match is written")
            );
            assert_eq!(&source[start..end], *written);
        }
    });
}

const TWO_FUNCTIONS: &str = r#"enum Shape {
  Circle(radius: u64);
  Square(side: u64);
  Triangle(edge: u64);
}

fn area(s: Shape) -> result: u64 pure {
  match s {
    Circle(radius: r) => {
      return r;
    }
  }
}

fn name(s: Shape) -> result: u64 pure {
  match s {
    Circle(radius: size) => {
      return 1_u64;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;

#[test]
fn one_check_lists_both_functions_in_order_and_variants_in_declaration_order() {
    assert_sites(
        TWO_FUNCTIONS,
        "Shape",
        &[
            (
                "match s {\n    Circle(radius: r) => {\n      return r;\n    }\n  }",
                &["Square", "Triangle"],
            ),
            (
                "match s {\n    Circle(radius: size) => {\n      return 1_u64;\n    }\n  }",
                &["Square", "Triangle"],
            ),
        ],
    );
}

#[test]
fn a_loop_summary_retry_keeps_each_settled_match_site_once_in_order() {
    let source = r#"enum Shade {
  Red(value: u64);
  Blue(value: u64);
}

fn earlier(shade: Shade) -> result: u64 pure {
  match shade {
    Red(value: first) => {
      return first;
    }
  }
}

fn repeated(shade: Shade, count: u64) -> result: u64 pure {
  for (i in 0_u64..2_u64) {
    match shade {
      Red(value: current) => {
      }
    }
    set count = 0_u64;
  }
  return count;
}

fn later(shade: Shade) -> result: u64 pure {
  match shade {
    Blue(value: last) => {
      return last;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let earlier = "match shade {\n    Red(value: first) => {\n      return first;\n    }\n  }";
    let repeated = "match shade {\n      Red(value: current) => {\n      }\n    }";
    let later = "match shade {\n    Blue(value: last) => {\n      return last;\n    }\n  }";
    let recorded = super::super::check::capture_match_sites(|| {
        assert_sites(
            source,
            "Shade",
            &[
                (earlier, &["Blue"]),
                (repeated, &["Blue"]),
                (later, &["Red"]),
            ],
        );
    });
    // The backedge changes count from its incoming call value, forcing a
    // ReferenceSummaryChanged retry. Rollback must let the loop site be
    // inserted again; without it, node deduplication hides the stale event
    // in the final payload but leaves only one insertion in this trace.
    let written = recorded
        .iter()
        .map(|location| {
            let SemanticLocation::SourceNode(_, coordinate) = location;
            let start = usize::try_from(coordinate.start().value()).expect("source offset");
            let end = usize::try_from(coordinate.end().value()).expect("source offset");
            &source[start..end]
        })
        .collect::<Vec<_>>();
    assert_eq!(written, [earlier, repeated, repeated, later]);
}

#[test]
fn a_used_value_match_binding_keeps_its_derived_type_for_the_next_match() {
    let source = r#"enum Shape {
  Circle(radius: u64);
  Square(side: u64);
}

fn inspect(s: Shape) -> result: u64 pure {
  let selected = match s {
    Circle(radius: r) => {
      give Shape::Circle(radius: r);
    }
  }
  match selected {
    Square(side: edge) => {
      return edge;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_sites(
        source,
        "Shape",
        &[
            (
                "match s {\n    Circle(radius: r) => {\n      give Shape::Circle(radius: r);\n    }\n  }",
                &["Square"],
            ),
            (
                "match selected {\n    Square(side: edge) => {\n      return edge;\n    }\n  }",
                &["Circle"],
            ),
        ],
    );
}

#[test]
fn a_later_structural_error_stops_collection_without_replacing_the_first_site() {
    let source = r#"enum Shape {
  Circle();
  Square();
}

fn inspect(s: Shape) -> result: u64 pure {
  match s {
    Circle() => {
    }
  }
  match s {
    Square() => {
    }
  }
  return True();
}

fn later(s: Shape) -> result: u64 pure {
  match s {
    Circle() => {
      return 0_u64;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_sites(
        source,
        "Shape",
        &[
            ("match s {\n    Circle() => {\n    }\n  }", &["Square"]),
            ("match s {\n    Square() => {\n    }\n  }", &["Circle"]),
        ],
    );
}

#[test]
fn an_earlier_structural_error_does_not_collect_later_matches() {
    let source = r#"enum Shape {
  Circle();
  Square();
}

fn broken() -> result: u64 pure {
  return True();
}

fn later(s: Shape) -> result: u64 pure {
  match s {
    Circle() => {
      return 0_u64;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule(
        source.as_bytes(),
        SemanticRule::Fn1,
        SemanticIssueKind::ReturnMismatch,
    );
}

#[test]
fn give1_at_the_same_match_node_precedes_err2_in_rule_definition_order() {
    let source = r#"enum Shade {
  Red();
  Blue();
}

fn main() -> status: std::process::ExitStatus pure {
  let shade = Shade::Red();
  let picked = match shade {
    Red() => {
    }
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_rule(
        source.as_bytes(),
        SemanticRule::Give1,
        SemanticIssueKind::InvalidGive,
    );
}

#[test]
fn a_match_missing_one_variant_and_repeating_another_reports_the_missing_site() {
    // The repeated arm is an ERR-2 rejection at the node already recorded as
    // incomplete; both are ERR-2, so the recorded site stays the first event.
    let source = r#"enum Flag {
  A();
  B();
}

fn main() -> status: std::process::ExitStatus pure {
  let flag = Flag::A();
  match flag {
    A() => {
    }
    A() => {
    }
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_sites(
        source,
        "Flag",
        &[(
            "match flag {\n    A() => {\n    }\n    A() => {\n    }\n  }",
            &["B"],
        )],
    );
}

#[test]
fn a_later_match_repeating_a_variant_does_not_replace_the_recorded_site() {
    let source = r#"enum Flag {
  A();
  B();
}

fn first(flag: Flag) -> result: u64 pure {
  match flag {
    A() => {
      return 0_u64;
    }
  }
}

fn second(flag: Flag) -> result: u64 pure {
  match flag {
    A() => {
      return 1_u64;
    }
    A() => {
      return 2_u64;
    }
    B() => {
      return 3_u64;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_sites(
        source,
        "Flag",
        &[(
            "match flag {\n    A() => {\n      return 0_u64;\n    }\n  }",
            &["B"],
        )],
    );
}

#[test]
fn generic_instances_report_the_written_match_once() {
    let source = r#"enum Shade {
  Red();
  Blue();
}

fn inspect<const n: u64>(shade: Shade) -> result: u64 pure {
  match shade {
    Red() => {
      return n;
    }
  }
}

fn ordinary(shade: Shade) -> result: u64 pure {
  match shade {
    Blue() => {
      return 2_u64;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let shade = Shade::Red();
  let first = inspect::<1>(shade: shade);
  let second = inspect::<2>(shade: shade);
  return std::process::exit_status(code: 0_u8);
}
"#;
    assert_sites(
        source,
        "Shade",
        &[
            (
                "match shade {\n    Red() => {\n      return n;\n    }\n  }",
                &["Blue"],
            ),
            (
                "match shade {\n    Blue() => {\n      return 2_u64;\n    }\n  }",
                &["Red"],
            ),
        ],
    );
    let repaired = source
        .replace(
            "      return n;\n    }",
            "      return n;\n    }\n    Blue() => {\n      return 0_u64;\n    }",
        )
        .replace(
            "      return 2_u64;\n    }",
            "      return 2_u64;\n    }\n    Red() => {\n      return 0_u64;\n    }",
        );
    with_semantics(repaired.as_bytes(), |outcome| {
        assert!(matches!(outcome, SemanticOutcome::Complete(_)), "{outcome:?}");
    });
}
