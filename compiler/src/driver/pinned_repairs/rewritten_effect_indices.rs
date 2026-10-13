//! Indexed effect rows keep the value parameter supplied at the call.

use super::RepairPair;

pub(super) const REWRITTEN_EFFECT_INDICES: &[RepairPair] = &[
    RepairPair {
        name: "set-after-effect-index-write.wf",
        rejected: br#"fn put(window: &Array<u64, 4>, index: u64) -> result: u64 writes(window[index]) contract {
  requires index < 4_u64;
} {
  set index = 0_u64;
  set window^[index] = 1_u64;
  return index;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "SET-1",
        sentences: &[
            "\n  root_class: a reference whose declared row does not write this path\n",
            "\n  mechanical_fix: the row `writes(window[index])` uses `index`'s value at the call, but `set index = 0_u64;` at line 4 rewrites it before the access `window^[index]`; where this access is the row mismatch to repair, keep the row either by moving the access `window^[index]` before that write, or by leaving `index` unchanged, binding the new value in a fresh local, and using that local for the work that needs the new value\n",
        ],
        repaired: &[
            br#"fn put(window: &Array<u64, 4>, index: u64) -> result: u64 writes(window[index]) contract {
  requires index < 4_u64;
} {
  set window^[index] = 1_u64;
  set index = 0_u64;
  return index;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn put(window: &Array<u64, 4>, index: u64) -> result: u64 writes(window[index]) contract {
  requires index < 4_u64;
} {
  let newvalue = 0_u64;
  set window^[index] = 1_u64;
  return newvalue;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "call-after-effect-index-write.wf",
        rejected: br#"fn put(window: &Array<u64, 4>, index: u64) -> result: unit writes(window[index]) contract {
  requires index < 4_u64;
} {
  set window^[index] = 1_u64;
  return unit;
}

fn forward(window: &Array<u64, 4>, index: u64) -> result: u64 writes(window[index]) contract {
  requires index < 4_u64;
} {
  set index = 0_u64;
  put(window: window, index: index);
  return index;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "EFF-2",
        sentences: &[
            "\n  found_row: writes(window[index])\n",
            "\n  mechanical_fix: the row `writes(window[index])` uses `index`'s value at the call, but `set index = 0_u64;` at line 11 rewrites it before the call `put(window: window, index: index)`; where this access is the row mismatch to repair, keep the row either by moving the call `put(window: window, index: index)` before that write, or by leaving `index` unchanged, binding the new value in a fresh local, and using that local for the work that needs the new value\n",
        ],
        repaired: &[
            br#"fn put(window: &Array<u64, 4>, index: u64) -> result: unit writes(window[index]) contract {
  requires index < 4_u64;
} {
  set window^[index] = 1_u64;
  return unit;
}

fn forward(window: &Array<u64, 4>, index: u64) -> result: u64 writes(window[index]) contract {
  requires index < 4_u64;
} {
  put(window: window, index: index);
  set index = 0_u64;
  return index;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn put(window: &Array<u64, 4>, index: u64) -> result: unit writes(window[index]) contract {
  requires index < 4_u64;
} {
  set window^[index] = 1_u64;
  return unit;
}

fn forward(window: &Array<u64, 4>, index: u64) -> result: u64 writes(window[index]) contract {
  requires index < 4_u64;
} {
  let newvalue = 0_u64;
  put(window: window, index: index);
  return newvalue;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "call-through-reference-after-effect-index-write.wf",
        rejected: br#"fn put(cell: &u64) -> result: unit writes(cell) {
  set cell^ = 1_u64;
  return unit;
}

fn forward(window: &Array<u64, 4>, index: u64) -> result: u64 writes(window[index]) contract {
  requires index < 4_u64;
} {
  set index = 0_u64;
  let cell = &window^[index];
  put(cell: cell);
  return index;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "EFF-2",
        sentences: &[
            "\n  found_row: writes(window[index])\n",
            "\n  mechanical_fix: the row `writes(window[index])` uses `index`'s value at the call, but `set index = 0_u64;` at line 9 rewrites it before the call `put(cell: cell)`; where this access is the row mismatch to repair, keep the row by leaving `index` unchanged, binding the new value in a fresh local, and using that local for the work that needs the new value\n",
        ],
        repaired: &[br#"fn put(cell: &u64) -> result: unit writes(cell) {
  set cell^ = 1_u64;
  return unit;
}

fn forward(window: &Array<u64, 4>, index: u64) -> result: u64 writes(window[index]) contract {
  requires index < 4_u64;
} {
  let newvalue = 0_u64;
  let cell = &window^[index];
  put(cell: cell);
  return newvalue;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "set-from-local-after-effect-index-write.wf",
        rejected: br#"fn put(window: &Array<u64, 4>, index: u64) -> result: u64 writes(window[index]) contract {
  requires index < 4_u64;
} {
  set index = 0_u64;
  let value = 1_u64;
  set window^[index] = value;
  return index;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "SET-1",
        sentences: &[
            "\n  root_class: a reference whose declared row does not write this path\n",
            "\n  mechanical_fix: the row `writes(window[index])` uses `index`'s value at the call, but `set index = 0_u64;` at line 4 rewrites it before the access `window^[index]`; where this access is the row mismatch to repair, keep the row by leaving `index` unchanged, binding the new value in a fresh local, and using that local for the work that needs the new value\n",
        ],
        repaired: &[br#"fn put(window: &Array<u64, 4>, index: u64) -> result: u64 writes(window[index]) contract {
  requires index < 4_u64;
} {
  let newvalue = 0_u64;
  let value = 1_u64;
  set window^[index] = value;
  return newvalue;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "call-with-two-effects-after-index-write.wf",
        rejected: br#"fn put(window: &Array<u64, 4>, index: u64, other: u64) -> result: unit writes(window[index]), writes(window[other]) contract {
  requires index < 4_u64;
  requires other < 4_u64;
} {
  set window^[index] = 1_u64;
  set window^[other] = 1_u64;
  return unit;
}

fn forward(window: &Array<u64, 4>, index: u64, other: u64) -> result: u64 writes(window[index]), writes(window[other]) contract {
  requires index < 4_u64;
  requires other < 4_u64;
  requires other > 0_u64;
  requires index < other;
} {
  set index = 0_u64;
  put(window: window, index: index, other: other);
  return index;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "EFF-2",
        sentences: &[
            "\n  found_row: writes(window[index]), writes(window[other])\n",
            "\n  mechanical_fix: the row `writes(window[index]), writes(window[other])` uses `index`'s value at the call, but `set index = 0_u64;` at line 16 rewrites it before the call `put(window: window, index: index, other: other)`; where this access is the row mismatch to repair, keep the row by leaving `index` unchanged, binding the new value in a fresh local, and using that local for the work that needs the new value\n",
        ],
        repaired: &[br#"fn put(window: &Array<u64, 4>, index: u64, other: u64) -> result: unit writes(window[index]), writes(window[other]) contract {
  requires index < 4_u64;
  requires other < 4_u64;
} {
  set window^[index] = 1_u64;
  set window^[other] = 1_u64;
  return unit;
}

fn forward(window: &Array<u64, 4>, index: u64, other: u64) -> result: u64 writes(window[index]), writes(window[other]) contract {
  requires index < 4_u64;
  requires other < 4_u64;
  requires other > 0_u64;
  requires index < other;
} {
  let newvalue = 0_u64;
  put(window: window, index: index, other: other);
  return newvalue;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
];

#[test]
fn rewritten_index_repair_does_not_move_an_operation_before_its_local_declaration() {
    for pair in &REWRITTEN_EFFECT_INDICES[2..4] {
        let failure = super::compile(
            &[super::SourceInput::new(pair.name, pair.rejected)],
            super::CompilerLimits::default(),
        )
        .expect_err("the operation uses the rewritten index and a later local");
        assert_eq!(failure.rule_id(), Some(pair.rule));
        let rendered = failure.to_string();
        assert!(
            rendered.contains("keep the row by leaving `index` unchanged"),
            "{rendered}"
        );
        assert!(!rendered.contains("before that write"), "{rendered}");
    }
}

#[test]
fn rewritten_index_repair_does_not_assume_same_reference_effects_are_separate() {
    let pair = &REWRITTEN_EFFECT_INDICES[4];
    let failure = super::compile(
        &[super::SourceInput::new(pair.name, pair.rejected)],
        super::CompilerLimits::default(),
    )
    .expect_err("the call still writes through the rewritten effect index");
    assert_eq!(failure.rule_id(), Some("EFF-2"));
    let rendered = failure.to_string();
    assert!(
        rendered.contains("keep the row by leaving `index` unchanged"),
        "{rendered}"
    );
    assert!(!rendered.contains("before that write"), "{rendered}");
}

#[test]
fn rewritten_index_repair_does_not_move_past_a_later_invariant() {
    let source = String::from_utf8(REWRITTEN_EFFECT_INDICES[1].rejected.to_vec())
        .expect("the fixture is UTF-8")
        .replace(
            "  set index = 0_u64;\n  put(",
            "  set index = 0_u64;\n  invariant ready: index < 4_u64;\n  put(",
        );
    let failure = super::compile(
        &[super::SourceInput::new(
            "call-after-later-invariant.wf",
            source.as_bytes(),
        )],
        super::CompilerLimits::default(),
    )
    .expect_err("the written row still names the entry index");
    assert_eq!(failure.rule_id(), Some("EFF-2"));
    let rendered = failure.to_string();
    assert!(rendered.contains("value at the call"), "{rendered}");
    assert!(!rendered.contains("before that write"), "{rendered}");
}

#[test]
fn rewritten_index_repair_does_not_assume_an_uncertified_call_requirement() {
    let source = String::from_utf8(REWRITTEN_EFFECT_INDICES[1].rejected.to_vec())
        .expect("the fixture is UTF-8")
        .replacen("requires index < 4_u64;", "requires index <= 3_u64;", 1);
    let failure = super::compile(
        &[super::SourceInput::new(
            "call-with-uncertified-entry-requirement.wf",
            source.as_bytes(),
        )],
        super::CompilerLimits::default(),
    )
    .expect_err("the written row still names the entry index");
    assert_eq!(failure.rule_id(), Some("EFF-2"));
    let rendered = failure.to_string();
    assert!(rendered.contains("value at the call"), "{rendered}");
    assert!(!rendered.contains("before that write"), "{rendered}");
}

#[test]
fn rewritten_call_index_does_not_also_suggest_widening_the_row() {
    let pair = &REWRITTEN_EFFECT_INDICES[1];
    let failure = super::compile(
        &[super::SourceInput::new(pair.name, pair.rejected)],
        super::CompilerLimits::default(),
    )
    .expect_err("the rewritten index is outside the declared element row");
    assert_eq!(failure.rule_id(), Some("EFF-2"));
    let rendered = failure.to_string();
    assert!(rendered.contains("value at the call"), "{rendered}");
    assert!(!rendered.contains("expected_row:"), "{rendered}");
    assert!(!rendered.contains("declare the row as"), "{rendered}");
}

#[test]
fn conditional_index_write_does_not_supply_a_dominating_write_line() {
    let source = br#"fn put(window: &Array<u64, 4>, index: u64, change: Bool) -> result: unit writes(window[index]) contract {
  requires index < 4_u64;
} {
  if change {
    set index = 0_u64;
  }
  set window^[index] = 1_u64;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;
    let failure = super::compile(
        &[super::SourceInput::new("conditional-effect-index-write.wf", source)],
        super::CompilerLimits::default(),
    )
    .expect_err("one incoming path has rewritten the effect index");
    assert_eq!(failure.rule_id(), Some("SET-1"));
    let rendered = failure.to_string();
    assert!(!rendered.contains("mechanical_fix:"), "{rendered}");
}
