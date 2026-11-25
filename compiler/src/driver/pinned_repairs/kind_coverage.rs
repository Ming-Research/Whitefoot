//! Pairs for the rejection kinds no repair family reaches [DIAG-1]. Each
//! pair rejects the smallest conformance case that prints its kind, or a
//! probe written for it, and carries out the repair its rejection prints, or
//! is accepted once the construct the rule refuses is written as the rule
//! admits it where the rejection prints no repair.

use super::RepairPair;

/// The accepted program every lexical probe starts from.
const MAIN: &[u8] = br#"fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;

pub(super) const KIND_COVERAGE: &[RepairPair] = &[
    // [FORM-4] a comment prefix is no token.
    RepairPair {
        name: "lexical-comment-prefix.wf",
        rejected: br#"fn main() -> status: std::process::ExitStatus pure {
  // a comment
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FORM-4",
        sentences: &[],
        repaired: &[MAIN],
    },
    // [FORM-1] a byte that cannot begin a token.
    RepairPair {
        name: "lexical-unexpected-byte.wf",
        rejected: br#"fn main() -> status: std::process::ExitStatus pure {
  let x = #;
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FORM-1",
        sentences: &[],
        repaired: &[MAIN],
    },
    // [FORM-3] a label marker with no lowercase name after it.
    RepairPair {
        name: "lexical-missing-label-name.wf",
        rejected: br#"fn main() -> status: std::process::ExitStatus pure {
  let x = @;
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FORM-3",
        sentences: &[],
        repaired: &[MAIN],
    },
    // [FORM-2] a byte that is no part of a UTF-8 encoding.
    RepairPair {
        name: "lexical-invalid-utf8.wf",
        rejected: b"fn main() -> status: std::process::ExitStatus pure {\n  let x = \xff;\n  return std::process::exit_status(code: 0_u8);\n}\n",
        rule: "FORM-2",
        sentences: &[],
        repaired: &[MAIN],
    },
    // [FORM-2] a control byte outside the layout bytes.
    RepairPair {
        name: "lexical-invalid-source-byte.wf",
        rejected: b"fn main() -> status: std::process::ExitStatus pure {\n  let x = \x01;\n  return std::process::exit_status(code: 0_u8);\n}\n",
        rule: "FORM-2",
        sentences: &[],
        repaired: &[MAIN],
    },
    // [FORM-5] a STRING that reaches the end of the source open.
    RepairPair {
        name: "lexical-unterminated-text.wf",
        rejected: b"fn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n\"text",
        rule: "FORM-5",
        sentences: &[],
        repaired: &[MAIN],
    },
    // [FORM-5] a scalar a STRING may not hold raw.
    RepairPair {
        name: "lexical-invalid-text-byte.wf",
        rejected: "const text: Array<u8, 3> = \"\u{2603}\";\n\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n".as_bytes(),
        rule: "FORM-5",
        sentences: &[],
        repaired: &[br#"const text: Array<u8, 3> = "\u{2603}";

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    // [FORM-5] a backslash that begins no escape.
    RepairPair {
        name: "lexical-invalid-text-escape.wf",
        rejected: br#"const text: Array<u8, 1> = "\q";

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FORM-5",
        sentences: &[],
        repaired: &[br#"const text: Array<u8, 1> = "q";

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
];
