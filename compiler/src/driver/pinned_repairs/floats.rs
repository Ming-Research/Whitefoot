//! Canonical finite float spellings and the repair for overflow to infinity.

use super::RepairPair;

pub(super) const FLOATS: &[RepairPair] = &[
    RepairPair {
        name: "float-canonical-spelling.wf",
        rejected: br#"fn value() -> result: f64 pure {
  return 0.5e3_f64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FORM-7",
        sentences: &[
            "]: InvalidFloatLiteral\n",
            "\n  canonical_spelling: 5.0e2_f64\n",
            "\n  mechanical_fix: write the literal as `5.0e2_f64`\n",
        ],
        repaired: &[br#"fn value() -> result: f64 pure {
  return 5.0e2_f64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "float-nonfinite-value.wf",
        rejected: br#"fn value() -> result: f64 pure {
  return 1.0e999_f64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FORM-7",
        sentences: &[
            "]: InvalidFloatLiteral\n",
            "\n  mechanical_fix: replace the literal with a canonical spelling of a finite value representable in its stated type\n",
        ],
        repaired: &[br#"fn value() -> result: f64 pure {
  return 1.0_f64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
];
