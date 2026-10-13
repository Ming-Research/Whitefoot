//! [EFF-2] category repairs replace a declared write the body never exhibits.
//! Keep this family with the shared repair harness while these diagnostics
//! direct the writer to the corresponding read row.

use super::RepairPair;

pub(super) const EFFECT_ROW_CATEGORIES: &[RepairPair] = &[RepairPair {
    name: "eff2-neg-read-only-declared-write.wf",
    rejected: include_bytes!(
        "../../../../tests/conformance/cases/eff2-neg-read-only-declared-write.wf"
    ),
    rule: "EFF-2",
    sentences: &[
        "]: EffectMismatch\n",
        "\n  expected_row: reads(h)\n  found_row: writes(h)\n  missing: []\n  extra: [writes(h)]\n",
        "\n  mechanical_fix: declare the row as `reads(h)`, which covers every access the body makes and no other\n",
    ],
    repaired: &[br#"fn peek(h: &u64) -> result: u64 reads(h) {
  return h^;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
}];
