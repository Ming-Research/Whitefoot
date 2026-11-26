//! [MSR-3] entry formers outside postconditions describe no function-entry state.

use super::RepairPair;

const CURRENT_VALUE: &[&str] = &[
    "]: InvalidEntryFormer\n",
    "\n  mechanical_fix: if this expression should read the current value, replace this `entry(...)` former with the place inside its parentheses; entry-state reads belong only in an ensures clause on a reference parameter whose declared row writes it\n",
];

pub(super) const ENTRY_FORMERS: &[RepairPair] = &[
    RepairPair {
        name: "entry-bare-type-invariant.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/msr3-neg-entry-bare-type-invariant.wf"
        ),
        rule: "MSR-3",
        sentences: CURRENT_VALUE,
        repaired: &[br#"struct Pair {
  marker: u64;
  value: u64;
  invariant stays(pair): pair.value == pair.value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "entry-ordinary-type-invariant.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/msr3-neg-entry-ordinary-type-invariant.wf"
        ),
        rule: "MSR-3",
        sentences: CURRENT_VALUE,
        repaired: &[br#"struct Pair {
  marker: u64;
  value: u64;
  invariant stays(pair): pair.value == pair.value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "entry-range-type-invariant.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/msr3-neg-entry-range-type-invariant.wf"
        ),
        rule: "MSR-3",
        sentences: CURRENT_VALUE,
        repaired: &[br#"struct Zeroes {
  marker: u64;
  cells: Array<u64, 1>;
  invariant(z): forall zero(k in 0_u64..z^.cells.len): z^.cells[k] == z^.cells[k];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "entry-in-requirement.wf",
        rejected: br#"fn width(values: &Slots<u64, 4>) -> result: u64 reads(values) contract {
  requires entry(values)^.len <= 4_u64;
} {
  return values^.len;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "MSR-3",
        sentences: CURRENT_VALUE,
        repaired: &[br#"fn width(values: &Slots<u64, 4>) -> result: u64 reads(values) contract {
  requires values^.len <= 4_u64;
} {
  return values^.len;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "entry-in-contract-definition.wf",
        rejected: br#"fn width(values: &Slots<u64, 4>) -> result: u64 reads(values) contract {
  define size = entry(values)^.len;
  requires size <= 4_u64;
} {
  return values^.len;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "MSR-3",
        sentences: CURRENT_VALUE,
        repaired: &[br#"fn width(values: &Slots<u64, 4>) -> result: u64 reads(values) contract {
  define size = values^.len;
  requires size <= 4_u64;
} {
  return values^.len;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "entry-in-body-expression.wf",
        rejected: br#"fn width(values: &Slots<u64, 4>) -> result: u64 reads(values) {
  let length = entry(values)^.len;
  return length;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "MSR-3",
        sentences: CURRENT_VALUE,
        repaired: &[br#"fn width(values: &Slots<u64, 4>) -> result: u64 reads(values) {
  let length = values^.len;
  return length;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "entry-in-body-invariant.wf",
        rejected: br#"fn width(values: &Slots<u64, 4>) -> result: u64 reads(values) {
  invariant bounded: entry(values)^.len <= 4_u64;
  return values^.len;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "MSR-3",
        sentences: CURRENT_VALUE,
        repaired: &[br#"fn width(values: &Slots<u64, 4>) -> result: u64 reads(values) {
  invariant bounded: values^.len <= 4_u64;
  return values^.len;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "entry-in-interface-requirement.wf",
        rejected: br#"interface Measure {
  fn width(values: &Slots<u64, 4>) -> result: u64 reads(values) contract {
    requires entry(values)^.len <= 4_u64;
  };
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "MSR-3",
        sentences: CURRENT_VALUE,
        repaired: &[br#"interface Measure {
  fn width(values: &Slots<u64, 4>) -> result: u64 reads(values) contract {
    requires values^.len <= 4_u64;
  };
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "entry-in-function-kind-requirement.wf",
        rejected: br#"fn apply<fn measure(values: &Slots<u64, 4>) -> result: u64 reads(values) contract {
  requires entry(values)^.len <= 4_u64;
}>(values: &Slots<u64, 4>) -> result: u64 reads(values) {
  return measure(values: values);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "MSR-3",
        sentences: CURRENT_VALUE,
        repaired: &[br#"fn apply<fn measure(values: &Slots<u64, 4>) -> result: u64 reads(values) contract {
  requires values^.len <= 4_u64;
}>(values: &Slots<u64, 4>) -> result: u64 reads(values) {
  return measure(values: values);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
];

/// Check position on the failure the shared harness already compiled.
pub(super) fn assert_span(pair: &RepairPair, failure: &crate::CompilationFailure) {
    if !ENTRY_FORMERS.iter().any(|entry| entry.name == pair.name) {
        return;
    }
    let source = std::str::from_utf8(pair.rejected).expect("entry fixture is text");
    let start = source.find("entry(").expect("entry former");
    let end = start + source[start..].find(')').expect("former closes") + 1;
    let expected = format!("\"bytes\":{{\"start\":{start},\"end\":{end}}}");
    let rendered = failure.render(crate::DiagnosticFormat::Json);
    assert!(rendered.contains(&expected), "{}: {rendered}", pair.name);
}
