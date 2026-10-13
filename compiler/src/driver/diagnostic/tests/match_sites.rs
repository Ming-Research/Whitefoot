use super::{DiagnosticFormat, assert_no_internal_identity, stop};

#[test]
fn match_sites_render_as_positions_in_text_and_complete_records_in_json() {
    let source = r#"enum Shape {
  Circle(radius: u64);
  Square(side: u64);
  Triangle(edge: u64);
}

fn area(s: Shape) -> result: u64 pure {
  match s {
    Circle(radius: radius) => {
      return radius;
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
    let failure = stop("sites.wf", source.as_bytes());
    let text = failure.to_string();
    assert!(
        text.starts_with("sites.wf:8:3: error[ERR-2]: NonExhaustiveMatch\n"),
        "{text}"
    );
    assert!(
        text.contains("  missing_variants: [Square, Triangle]\n"),
        "{text}"
    );
    assert!(
        text.ends_with(
            "  other_sites: [sites.wf:8:3 missing [Square, Triangle], sites.wf:16:3 missing [Square, Triangle]]"
        ),
        "{text}"
    );
    assert_no_internal_identity(&text);

    let sites = [
        (
            8,
            "match s {\n    Circle(radius: radius) => {\n      return radius;\n    }\n  }",
        ),
        (
            16,
            "match s {\n    Circle(radius: size) => {\n      return 1_u64;\n    }\n  }",
        ),
    ]
    .into_iter()
    .map(|(line, written)| {
        let start = source.find(written).expect("match site is written");
        let end = start + written.len();
        format!(
            r#"{{"location":{{"at":{{"file":"sites.wf","line":{line},"column":3}},"bytes":{{"start":{start},"end":{end}}}}},"enum_name":"Shape","missing_variants":["Square","Triangle"]}}"#
        )
    })
    .collect::<Vec<_>>()
    .join(",");
    let json = failure.render(DiagnosticFormat::Json);
    assert!(json.contains(&format!(r#""other_sites":[{sites}]"#)), "{json}");
    assert!(!json.contains('\n'));
    assert_no_internal_identity(&json);
}
