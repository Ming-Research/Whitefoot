//! Source names retained by proof goals and their repair instances.

use super::RepairPair;

pub(super) const PROOF_PLACE_SPELLINGS: &[RepairPair] = &[
    RepairPair {
        name: "proof-range-literal-repair-source-spellings.wf",
        rejected: br#"fn need(out: &[u8], src: &[u8]) -> result: unit pure contract {
  requires out^.len >= src^.len;
} {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let buffer = array_filled::<u8, 5>(value: 0_u8);
  let text = array_filled::<u8, 6>(value: 0_u8);
  need(out: &buffer[0_u64..5_u64], src: &text[0_u64..6_u64]);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FN-8",
        sentences: &[
            "\n  instantiated_goal: buffer[0_u64..5_u64].len >= text[0_u64..6_u64].len\n",
            "\n  mechanical_fix: `buffer[0_u64..5_u64].len >= text[0_u64..6_u64].len` is false for the values that reach this call, so no fact can establish it here: pass arguments that satisfy it, or change the statements or requirements that fix those values\n",
        ],
        repaired: &[
            br#"fn need(out: &[u8], src: &[u8]) -> result: unit pure contract {
  requires out^.len >= src^.len;
} {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let buffer = array_filled::<u8, 5>(value: 0_u8);
  let text = array_filled::<u8, 6>(value: 0_u8);
  need(out: &buffer[0_u64..5_u64], src: &text[0_u64..5_u64]);
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn need(out: &[u8], src: &[u8]) -> result: unit pure contract {
  requires out^.len >= src^.len;
} {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let buffer = array_filled::<u8, 6>(value: 0_u8);
  let text = array_filled::<u8, 6>(value: 0_u8);
  need(out: &buffer[0_u64..6_u64], src: &text[0_u64..6_u64]);
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
];
