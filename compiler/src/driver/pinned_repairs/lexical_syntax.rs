//! Lexical spelling and inside-item spacing repairs, applied as written.

use super::RepairPair;

pub(super) const LEXICAL_SYNTAX: &[RepairPair] = &[
    RepairPair {
        name: "uppercase-constant.wf",
        rejected: br#"const MAX_SIZE: u32 = 8_u32;

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FORM-1",
        sentences: &["\n  mechanical_fix: write this name in lower case as `max_size` [FORM-3]\n"],
        repaired: &[br#"const max_size: u32 = 8_u32;

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "blank-line-in-item.wf",
        rejected: br#"fn value() -> result: u32 pure {

  return 0_u32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FORM-2",
        sentences: &["\n  mechanical_fix: remove the blank line inside this item [FORM-2]\n"],
        repaired: &[br#"fn value() -> result: u32 pure {
  return 0_u32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "character-without-suffix.wf",
        rejected: br#"fn value() -> result: unit pure {
  let character = 'a';
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FORM-5",
        sentences: &[
            "\n  mechanical_fix: write a character literal with its mandatory suffix: `u8` for ASCII or `u32` for any Unicode scalar, such as `'a'_u8` [FORM-5, FORM-7]\n",
        ],
        repaired: &[
            br#"fn value() -> result: unit pure {
  let character = 'a'_u8;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn value() -> result: unit pure {
  let character = 'a'_u32;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "unicode-character-without-suffix.wf",
        rejected: br#"fn value() -> result: u32 pure {
  return '\u{e9}';
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FORM-5",
        sentences: &[
            "\n  mechanical_fix: write a character literal with its mandatory suffix: `u8` for ASCII or `u32` for any Unicode scalar, such as `'a'_u8` [FORM-5, FORM-7]\n",
        ],
        repaired: &[br#"fn value() -> result: u32 pure {
  return '\u{e9}'_u32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "hexadecimal-integer.wf",
        rejected: br#"fn value() -> result: u32 pure {
  return 0xff_u32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FORM-5",
        sentences: &[
            "\n  mechanical_fix: write an integer literal in decimal with its mandatory type suffix, such as `255_u32` [FORM-5, FORM-7]\n",
        ],
        repaired: &[br#"fn value() -> result: u32 pure {
  return 255_u32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "remainder-wrap-mode.wf",
        rejected: br#"fn value() -> result: unit pure {
  let remainder = 8_u32 %wrap 3_u32;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "GRAM-1",
        sentences: &[
            "\n  mechanical_fix: write `%`, `%defined`, or `%checked` for remainder; `%wrap` is not an infix operator [GRAM-1, GRAM-5]\n",
        ],
        repaired: &[
            br#"fn value() -> result: unit pure {
  let remainder = 8_u32 % 3_u32;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn value() -> result: unit pure {
  let remainder = 8_u32 %defined 3_u32;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn value() -> result: unit pure {
  let remainder = 8_u32 %checked 3_u32;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "boolean-disjunction-in-contract.wf",
        rejected: br#"fn value(a: Bool, b: Bool) -> result: unit pure contract {
  define combined = bnot(a) || bnot(b);
  requires combined;
} {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FORM-1",
        sentences: &[
            "\n  mechanical_fix: write `band(a, b)` for boolean conjunction and `bor(a, b)` for boolean disjunction; both operations are eager, so bind computed operands first with `let` in a body or `define` in a contract block [OP-1, GRAM-5, GRAM-9]\n",
        ],
        repaired: &[
            br#"fn value(a: Bool, b: Bool) -> result: unit pure contract {
  define left = bnot(a);
  define right = bnot(b);
  define combined = bor(left, right);
  requires combined;
} {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "boolean-disjunction.wf",
        rejected: br#"fn value(a: Bool, b: Bool) -> result: Bool pure {
  let combined = a || b;
  return combined;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FORM-1",
        sentences: &[
            "\n  mechanical_fix: write `band(a, b)` for boolean conjunction and `bor(a, b)` for boolean disjunction; both operations are eager, so bind computed operands first with `let` in a body or `define` in a contract block [OP-1, GRAM-5, GRAM-9]\n",
        ],
        repaired: &[br#"fn value(a: Bool, b: Bool) -> result: Bool pure {
  let combined = bor(a, b);
  return combined;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "range-boolean-disjunction.wf",
        rejected: br#"struct Node {
  live: Bool;
  open: Bool;
  slot: u64;
}

fn inspect(nodes: &Slots<Node, 1>) -> result: unit pure contract {
  requires nodes^.len == 1_u64;
  requires forall held(k in 0_u64..nodes^.len) when nodes^[k].live is True || nodes^[k].open is True: nodes^[k].slot == 0_u64;
} {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FORM-1",
        sentences: &[
            "\n  mechanical_fix: a range clause admits no call and has no disjunction, so `bor` and `||` are not available there; state a disjunctive guard as one range clause per disjunct with the same conclusion, as in `when nodes^[k] is True:` and `when flags^[k] is True:`, and a disjunctive conclusion has no range spelling [RANGE-1, GRAM-5]\n",
        ],
        repaired: &[br#"struct Node {
  live: Bool;
  open: Bool;
  slot: u64;
}

fn inspect(nodes: &Slots<Node, 1>) -> result: unit pure contract {
  requires nodes^.len == 1_u64;
  requires forall held_live(k in 0_u64..nodes^.len) when nodes^[k].live is True: nodes^[k].slot == 0_u64;
  requires forall held_open(k in 0_u64..nodes^.len) when nodes^[k].open is True: nodes^[k].slot == 0_u64;
} {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
];
