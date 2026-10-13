//! Names written with a reserved lowercase spelling [FORM-3, OP-1], each
//! pinned with the rename its rejection names carried out.
//!
//! A fixed grammar word is refused by the grammar where IDENT is expected, and
//! a mode word is refused by declaration inventory; both rejections cite
//! FORM-3 and carry the same sentence form: that the spelling is reserved,
//! what takes it, and a fresh spelling to write at the declaration and every
//! use.

use super::RepairPair;

pub(super) const RESERVED_NAMES: &[RepairPair] = &[
    RepairPair {
        name: "reserved-let-grammar-word.wf",
        rejected: br#"fn main() -> status: std::process::ExitStatus pure {
  let copy = 1_u8;
  return std::process::exit_status(code: copy);
}
"#,
        rule: "FORM-3",
        sentences: &[
            "mechanical_fix: `copy` is reserved: it is a fixed word of the grammar of a type parameter's capability bound [GRAM-2], and IDENT excludes every fixed grammar word [FORM-3]; rename this name, at its declaration and at every use, to a spelling that is not reserved, such as `copy_value`\n",
        ],
        repaired: &[br#"fn main() -> status: std::process::ExitStatus pure {
  let copy_value = 1_u8;
  return std::process::exit_status(code: copy_value);
}
"#],
    },
    RepairPair {
        name: "reserved-let-mode-word.wf",
        rejected: br#"fn main() -> status: std::process::ExitStatus pure {
  let checked = 1_u8;
  return std::process::exit_status(code: checked);
}
"#,
        rule: "FORM-3",
        sentences: &[
            "mechanical_fix: `checked` is reserved: it is a mode word, the mode suffix of the operation spellings `+checked`, `-checked`, `*checked`, `/checked`, `%checked`, `ineg.checked`, `cvt.checked` and `iabs.checked`, and no declaration may use a mode word [OP-1, FORM-3]; rename this name, at its declaration and at every use, to a spelling that is not reserved, such as `checked_value`\n",
        ],
        repaired: &[br#"fn main() -> status: std::process::ExitStatus pure {
  let checked_value = 1_u8;
  return std::process::exit_status(code: checked_value);
}
"#],
    },
    RepairPair {
        // The call's argument label is a use of the parameter, so main's
        // segment is refused with the same rename.
        name: "reserved-parameter-grammar-word.wf",
        rejected: br#"fn pass(is: u8) -> result: u8 pure {
  return is;
}

fn main() -> status: std::process::ExitStatus pure {
  let value = pass(is: 1_u8);
  return std::process::exit_status(code: value);
}
"#,
        rule: "FORM-3",
        sentences: &[
            "mechanical_fix: `is` is reserved: it is a fixed word of the grammar of a routed postcondition's result route [GRAM-2] and a range variant relation [GRAM-5], and IDENT excludes every fixed grammar word [FORM-3]; rename this name, at its declaration and at every use, to a spelling that is not reserved, such as `is_value`\n",
        ],
        repaired: &[br#"fn pass(is_value: u8) -> result: u8 pure {
  return is_value;
}

fn main() -> status: std::process::ExitStatus pure {
  let value = pass(is_value: 1_u8);
  return std::process::exit_status(code: value);
}
"#],
    },
    RepairPair {
        name: "reserved-parameter-mode-word.wf",
        rejected: br#"fn pass(wrap: u8) -> result: u8 pure {
  return wrap;
}

fn main() -> status: std::process::ExitStatus pure {
  let value = pass(wrap: 1_u8);
  return std::process::exit_status(code: value);
}
"#,
        rule: "FORM-3",
        sentences: &[
            "mechanical_fix: `wrap` is reserved: it is a mode word, the mode suffix of the operation spellings `+wrap`, `-wrap`, `*wrap`, `ineg.wrap`, `cvt.wrap`, `ishl.wrap`, `ishr.wrap` and `iabs.wrap`, and no declaration may use a mode word [OP-1, FORM-3]; rename this name, at its declaration and at every use, to a spelling that is not reserved, such as `wrap_value`\n",
        ],
        repaired: &[br#"fn pass(wrap_value: u8) -> result: u8 pure {
  return wrap_value;
}

fn main() -> status: std::process::ExitStatus pure {
  let value = pass(wrap_value: 1_u8);
  return std::process::exit_status(code: value);
}
"#],
    },
    RepairPair {
        // The construction's field label is a use of the field.
        name: "reserved-field-grammar-word.wf",
        rejected: br#"struct Gauge {
  held: u8;
}

fn main() -> status: std::process::ExitStatus pure {
  let gauge = Gauge(held: 1_u8);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FORM-3",
        sentences: &[
            "mechanical_fix: `held` is reserved: it is a fixed word of the grammar of a held clause [GRAM-2], and IDENT excludes every fixed grammar word [FORM-3]; rename this name, at its declaration and at every use, to a spelling that is not reserved, such as `held_value`\n",
        ],
        repaired: &[br#"struct Gauge {
  held_value: u8;
}

fn main() -> status: std::process::ExitStatus pure {
  let gauge = Gauge(held_value: 1_u8);
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        // A mode word is reserved from fields so that `meter.sat` is never
        // both a field read and an OPNAME [FORM-3]; the program reads no
        // field, so declaration inventory is the first stage to refuse it.
        name: "reserved-field-mode-word.wf",
        rejected: br#"struct Meter {
  sat: u8;
}

fn main() -> status: std::process::ExitStatus pure {
  let meter = Meter(sat: 1_u8);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FORM-3",
        sentences: &[
            "mechanical_fix: `sat` is reserved: it is a mode word, the mode suffix of the operation spellings `+sat`, `-sat` and `*sat`, and no declaration may use a mode word [OP-1, FORM-3]; rename this name, at its declaration and at every use, to a spelling that is not reserved, such as `sat_value`\n",
        ],
        repaired: &[br#"struct Meter {
  sat_value: u8;
}

fn main() -> status: std::process::ExitStatus pure {
  let meter = Meter(sat_value: 1_u8);
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "reserved-match-binder-grammar-word.wf",
        rejected: br#"fn probe(found: Option<u8>) -> result: u8 pure {
  match found {
    Some(value: held) => {
      return held;
    }
    None() => {
      return 0_u8;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FORM-3",
        sentences: &[
            "mechanical_fix: `held` is reserved: it is a fixed word of the grammar of a held clause [GRAM-2], and IDENT excludes every fixed grammar word [FORM-3]; rename this name, at its declaration and at every use, to a spelling that is not reserved, such as `held_value`\n",
        ],
        repaired: &[br#"fn probe(found: Option<u8>) -> result: u8 pure {
  match found {
    Some(value: held_value) => {
      return held_value;
    }
    None() => {
      return 0_u8;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
];
