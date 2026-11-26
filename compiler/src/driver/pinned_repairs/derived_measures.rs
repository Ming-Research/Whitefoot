//! [MSR-1] a derived measure is read by value rather than borrowed.
//! The pairs pin the actual source path, its complete marked extent, and
//! acceptance of the corresponding observation. Keep this family wired to
//! the shared repair harness while these repairs are printed.

use super::RepairPair;

pub(super) const DERIVED_MEASURES: &[RepairPair] = &[
    RepairPair {
        name: "msr1-neg-range-reference-length-borrow.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/msr1-neg-range-reference-length-borrow.wf"
        ),
        rule: "MSR-1",
        sentences: &[
            "]: BorrowOfDerivedMeasure\n",
            "\n  marker:            ^^^^^^^^^^\n",
            "\n  mechanical_fix: read the measure by value with `let n = bytes^.len;`\n",
        ],
        repaired: &[br#"fn length_of(bytes: &[u8]) -> result: u64 reads(bytes.len) {
  let n = bytes^.len;
  return n;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "msr1-neg-run-reference-length-borrow.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/msr1-neg-run-reference-length-borrow.wf"
        ),
        rule: "MSR-1",
        sentences: &[
            "]: BorrowOfDerivedMeasure\n",
            "\n  marker:            ^^^^^^^^^^\n",
            "\n  mechanical_fix: read the measure by value with `let n = bytes^.len;`\n",
        ],
        repaired: &[br#"fn length_of(bytes: &Run<u8>) -> result: u64 reads(bytes.len) {
  let n = bytes^.len;
  return n;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "msr1-neg-segment-length-borrow.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/msr1-neg-segment-length-borrow.wf"
        ),
        rule: "MSR-1",
        sentences: &[
            "]: BorrowOfDerivedMeasure\n",
            "\n  marker:            ^^^^^^^^^^^^^^^^^^\n",
            "\n  mechanical_fix: read the measure by value with `let n = values^[index].len;`\n",
        ],
        repaired: &[br#"fn length_of(values: &Segments<u8>, index: u64) -> result: u64 reads(values) contract {
  requires index < values^.len;
} {
  let n = values^[index].len;
  return n;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "paged-count-by-value.wf",
        rejected: br#"fn main() -> status: std::process::ExitStatus pure {
  let pages = box_paged_new::<u8>(capacity: 4_u64);
  let n = &pages.inner.pages.len;
  let observed = n^;
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "MSR-1",
        sentences: &[
            "]: BorrowOfDerivedMeasure\n",
            "\n  marker:            ^^^^^^^^^^^^^^^^^^^^^\n",
            "\n  mechanical_fix: read the measure by value with `let n = pages.inner.pages.len;`\n",
        ],
        repaired: &[br#"fn main() -> status: std::process::ExitStatus pure {
  let pages = box_paged_new::<u8>(capacity: 4_u64);
  let n = pages.inner.pages.len;
  let observed = n;
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "msr1-neg-range-element-page-count-borrow.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/msr1-neg-range-element-page-count-borrow.wf"
        ),
        rule: "MSR-1",
        sentences: &[
            "]: BorrowOfDerivedMeasure\n",
            "\n  marker:            ^^^^^^^^^^^^^^^^^^^^^^\n",
            "\n  mechanical_fix: read the measure by value with `let n = rows^[index].pages.len;`\n",
        ],
        repaired: &[br#"fn count_of(rows: &[Paged<u8>], index: u64) -> result: u64 reads(rows) contract {
  requires index < rows^.len;
} {
  let n = rows^[index].pages.len;
  return n;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "selected-page-length-by-value.wf",
        rejected: br#"fn length_of(pages: &Paged<u8>, index: u64) -> result: u64 reads(pages) contract {
  requires index < pages^.pages.len;
} {
  let n = &pages^.pages[index].len;
  return n^;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "MSR-1",
        sentences: &[
            "]: BorrowOfDerivedMeasure\n",
            "\n  marker:            ^^^^^^^^^^^^^^^^^^^^^^^\n",
            "\n  mechanical_fix: read the measure by value with `let n = pages^.pages[index].len;`\n",
        ],
        repaired: &[br#"fn length_of(pages: &Paged<u8>, index: u64) -> result: u64 reads(pages) contract {
  requires index < pages^.pages.len;
} {
  let n = pages^.pages[index].len;
  return n;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
];

/// A range's cap and head are absent in MSR-1's table. A by-value read of
/// either would still be refused; pin their TYPE-7 classification and the
/// complete path rather than promising that invalid observation as a repair.
#[test]
fn absent_range_measures_are_refused_at_the_complete_path() {
    for measure in ["cap", "head"] {
        let source = format!(
            "fn length_of(bytes: &[Ring<u8, 4>]) -> result: u64 reads(bytes.len) {{\n  let n = &bytes^.{measure};\n  return n^;\n}}\n\nfn main() -> status: std::process::ExitStatus pure {{\n  return std::process::exit_status(code: 0_u8);\n}}\n"
        );
        let failure = super::super::check(
            &[crate::SourceInput::new(
                "absent-range-measure.wf",
                source.as_bytes(),
            )],
            super::CompilerLimits::default(),
        )
        .expect_err("a range has neither cap nor head, even if its elements do");
        assert_eq!(failure.rule_id(), Some("TYPE-7"));
        let rendered = format!("{failure}\n");
        let carets = "^".repeat(format!("bytes^.{measure}").len());
        assert!(rendered.contains(&format!("\n  marker:            {carets}\n")));
        assert!(rendered.contains("]: RangeReferentUse\n"));
        assert!(!rendered.contains("read the measure by value"));
    }
}
