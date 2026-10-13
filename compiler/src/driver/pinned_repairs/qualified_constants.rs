//! [MOD-5] A qualified const written where a value stands. A single source
//! is the root module, so the pair carries out the own-module alternative;
//! the alias alternative needs a second module and is carried out by the
//! conformance case `mod4-pos-const-alias-in-set-and-let`.

use super::RepairPair;

pub(super) const QUALIFIED_CONSTANTS: &[RepairPair] = &[
    RepairPair {
        name: "qualified-own-const-in-a-let.wf",
        rejected: br#"const limit: u32 = 7_u32;

fn main() -> status: std::process::ExitStatus pure {
  let wanted = pkg::limit;
  if wanted != 7_u32 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "MOD-5",
        sentences: &[
            "]: UnexpectedToken\n",
            "\n  mechanical_fix: a qualified path names only a callee, type, construction or destructuring target, never a value [MOD-5]: to read the const `limit` here, add `alias limit = pkg::limit;` to this record's alias header and write `limit` in place of `pkg::limit` [MOD-4]; a const of this record's own module is written `limit` with no alias\n",
        ],
        repaired: &[br#"const limit: u32 = 7_u32;

fn main() -> status: std::process::ExitStatus pure {
  let wanted = limit;
  if wanted != 7_u32 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
];
