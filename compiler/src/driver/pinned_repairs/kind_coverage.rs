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
    // [TYPE-6] a `continue` outside every loop.
    RepairPair {
        name: "continue-neg-outside-loop.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/continue-neg-outside-loop.wf"
        ),
        rule: "TYPE-6",
        sentences: &[
            "\n  mechanical_fix: move `continue;` inside a loop or remove it\n",
        ],
        repaired: &[br#"fn main() -> status: std::process::ExitStatus pure {
  doc "Exercises the continue transfer.";
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    // [FN-1] a statement after a loop that never ends.
    RepairPair {
        name: "fn1-neg-statement-after-break-free-loop.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/fn1-neg-statement-after-break-free-loop.wf"
        ),
        rule: "FN-1",
        sentences: &[],
        repaired: &[br#"fn main() -> status: std::process::ExitStatus pure {
  loop {
    return std::process::exit_status(code: 0_u8);
  }
}
"#],
    },
    // [FN-1] a body that can end without returning.
    RepairPair {
        name: "fn1-neg-counted-return-fallthrough.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/fn1-neg-counted-return-fallthrough.wf"
        ),
        rule: "FN-1",
        sentences: &[],
        repaired: &[br#"fn main() -> status: std::process::ExitStatus pure {
  for (i in 0_u64..1_u64) {
    return std::process::exit_status(code: 0_u8);
  }
  return std::process::exit_status(code: 1_u8);
}
"#],
    },
    // [GRAM-12] two arms that select one value.
    RepairPair {
        name: "gram12-neg-duplicate-const-value.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/gram12-neg-duplicate-const-value.wf"
        ),
        rule: "GRAM-12",
        sentences: &[
            "\n  mechanical_fix: remove the later arm or give it a distinct integer value\n",
        ],
        repaired: &[br#"const first: u64 = 7_u64;

const alias_of_first: u64 = first;

fn selected(value: u64) -> result: unit pure {
  match value {
    first => {
      return unit;
    }
    else => {
      return unit;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    // [ERR-2] a match that leaves a variant uncovered.
    RepairPair {
        name: "reject-err2-nonexhaustive.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/reject-err2-nonexhaustive.wf"
        ),
        rule: "ERR-2",
        sentences: &[],
        repaired: &[br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

enum Sign {
  Neg();
  Zero();
  Pos();
}

fn bad(s: Sign) -> function_result: u64 pure {
  match s {
    Neg() => {
      return 1_u64;
    }
    Zero() => {
      return 2_u64;
    }
    Pos() => {
      return 3_u64;
    }
  }
}

fn main() -> status: ExitStatus pure {
  return exit_status(code: 0_u8);
}
"#],
    },
    // [TYPE-7] `^` applied to a value that is no reference.
    RepairPair {
        name: "type7-neg-deref-nonref.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/type7-neg-deref-nonref.wf"
        ),
        rule: "TYPE-7",
        sentences: &[
            "\n  mechanical_fix: a Box's content is its field inner [TYPE-9]; an owned place is named as itself\n",
        ],
        repaired: &[br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  doc "deref applied to a plain i32 value is malformed [TYPE-7]: deref takes a place whose selected kind is a reference, &T or &[T], and an owned scalar is neither; a Box is refused there too, its content being the field inner [TYPE-9].";
  let a = 42_i32;
  let v = a;
  return exit_status(code: 0_u8);
}
"#],
    },
    // [OWN-1] a value used after the match that moved it.
    RepairPair {
        name: "own1-neg-use-after-move.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/own1-neg-use-after-move.wf"
        ),
        rule: "OWN-1",
        sentences: &[
            "\n  mechanical_fix: introduce a new `let` binding before reuse\n",
        ],
        repaired: &[br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

nocopy enum Sign {
  Neg(mag: i32);
  Zero();
  Pos();
}

fn main() -> return_value: ExitStatus pure {
  doc "OWN-1/OWN-13: matching an own value moves it; a second use is use-after-move.";
  let s = Sign::Pos();
  match move s {
    Neg(mag: m) => {
    }
    Zero() => {
    }
    Pos() => {
    }
  }
  return exit_status(code: 0_u8);
}
"#],
    },
    // [CONST-1] a constant expression that leaves the unsigned 64-bit domain.
    RepairPair {
        name: "const1-neg-eval-overflow.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/const1-neg-eval-overflow.wf"
        ),
        rule: "CONST-1",
        sentences: &[],
        repaired: &[br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn doubled<const n: u64>(count: u64) -> result: Slots<u64, n * 2> pure {
  doc "Forms a run whose capacity is the forwarded const-expression.";
  return slots_new::<u64, n * 2>();
}

fn main() -> status: ExitStatus pure {
  doc "CONST-1 evaluates constants exactly in the unsigned 64-bit domain. This instantiation makes n * 2 leave that domain and is rejected at the complete const node; an accepted const expression creates no runtime operation.";
  let cells = doubled::<8>(count: 7_u64);
  return exit_status(code: 0_u8);
}
"#],
    },
    // [CONST-1] a constant argument outside its parameter's range.
    RepairPair {
        name: "const1-neg-argument-outside-parameter-range-read.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/const1-neg-argument-outside-parameter-range-read.wf"
        ),
        rule: "CONST-1",
        sentences: &[],
        repaired: &[br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn pick<const n: u8>() -> r: u8 pure contract {
  requires n > 200_u8;
} {
  doc "Within u8's range n is at least 201, so 1000 - n is at most 799 and the subscript is in bounds.";
  let block = array_filled::<u8, 800>(value: 7_u8);
  let wide = cvt::<u8, u64>(n);
  let at = 1000_u64 - wide;
  return block[at];
}

fn main() -> status: ExitStatus pure {
  doc "300 lies outside u8's closed range, so the argument is rejected at its const node; admitted, a truncated n of 44 would read block[956], past the end of the block.";
  let v = pick::<201>();
  return exit_status(code: v);
}
"#],
    },
    // [EFF-1] a read entry beside the write that covers it.
    RepairPair {
        name: "eff1-neg-read-subsumed-by-write.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/eff1-neg-read-subsumed-by-write.wf"
        ),
        rule: "EFF-1",
        sentences: &[],
        repaired: &[br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

struct Stats {
  count: u64;
}

fn record(stats: &Stats) -> result: unit writes(stats.count) {
  doc "EFF-1: writes(stats.count) states every access at or below stats.count, so reads(stats.count) beside it states one access twice.";
  let old = stats^.count;
  set stats^.count = old +wrap 1_u64;
  return unit;
}

fn main() -> status: ExitStatus pure {
  doc "The malformed declaration above is rejected before this entry calls it.";
  let stats = Stats(count: 0_u64);
  record(stats: &stats);
  return exit_status(code: 0_u8);
}
"#],
    },
    // [TYPE-6] an arm that names a variant of another enum.
    RepairPair {
        name: "x-typ-match-foreign-variant.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/x-typ-match-foreign-variant.wf"
        ),
        rule: "TYPE-6",
        sentences: &[],
        repaired: &[br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

enum Signal {
  Ready();
  Busy();
}

fn main() -> status: ExitStatus pure {
  doc "An arm naming a variant outside the scrutinee enum resolves in the constructor domain but is foreign to the matched nominal [TYPE-6].";
  let s = Signal::Ready();
  match s {
    Ready() => {
    }
    Busy() => {
    }
  }
  return exit_status(code: 0_u8);
}
"#],
    },
    // [WAIT-3] a `spawn` inside a contract clause.
    RepairPair {
        name: "wait3-neg-spawn-in-clause.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/wait3-neg-spawn-in-clause.wf"
        ),
        rule: "WAIT-3",
        sentences: &[],
        repaired: &[br#"struct Counter {
  value: u64;
}

fn id(value: u64) -> result: u64 pure {
  return value;
}

fn bump(counter: &Counter, value: u64) -> result: unit writes(counter) {
  set counter^.value = value;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let left = Counter(value: 0_u64);
  let right = Counter(value: 0_u64);
  bump(counter: &left, value: 1_u64);
  bump(counter: &right, value: 2_u64);
  let sum = left.value +wrap right.value;
  if sum == 3_u64 {
    return std::process::exit_status(code: 0_u8);
  }
  return std::process::exit_status(code: 1_u8);
}
"#],
    },
    // [GRAM-8] a construction that names its fields out of declared order.
    RepairPair {
        name: "x-struct-neg-field-order.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/x-struct-neg-field-order.wf"
        ),
        rule: "GRAM-8",
        sentences: &[],
        repaired: &[br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

struct Pair {
  a: i32;
  b: i32;
}

fn main() -> status: ExitStatus pure {
  doc "GRAM-8: constructing a struct with out-of-order field names is rejected; declared order is the one legal byte sequence.";
  let p = Pair(a: 1_i32, b: 2_i32);
  return exit_status(code: 0_u8);
}
"#],
    },
    // [GRAM-10] an arm that binds a variant's fields out of declared order.
    RepairPair {
        name: "x-match-gram10-out-of-order-fields.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/x-match-gram10-out-of-order-fields.wf"
        ),
        rule: "GRAM-10",
        sentences: &[],
        repaired: &[br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

enum Pair {
  Both(a: i32, b: i32);
}

fn main() -> status: ExitStatus pure {
  doc "GRAM-10 declared order: a two-field variant bound out of declared order (b before a) is a hard error citing GRAM-10 and Both's declared field list.";
  let p = Pair::Both(a: 1_i32, b: 2_i32);
  let v = match p {
    Both(a: x, b: y) => {
      give x;
    }
  }
  return exit_status(code: 0_u8);
}
"#],
    },
    // [TYPE-6] a struct that declares one field label twice.
    RepairPair {
        name: "probe-duplicate-field-label.wf",
        rejected: br#"struct Pair {
  x: i32;
  x: i32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-6",
        sentences: &[],
        repaired: &[br#"struct Pair {
  x: i32;
  y: i32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    // [GIVE-1] a branch of a value conditional that gives nothing.
    RepairPair {
        name: "probe-give-missing-in-branch.wf",
        rejected: br#"fn pick(flag: Bool) -> result: i32 pure {
  let chosen = if flag {
  } else {
    give 0_i32;
  }
  return chosen;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "GIVE-1",
        sentences: &[],
        repaired: &[br#"fn pick(flag: Bool) -> result: i32 pure {
  let chosen = if flag {
    give 1_i32;
  } else {
    give 0_i32;
  }
  return chosen;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    // [FN-1] a returned value of a type other than the declared result.
    RepairPair {
        name: "probe-return-type-mismatch.wf",
        rejected: br#"fn main() -> status: std::process::ExitStatus pure {
  return 0_i32;
}
"#,
        rule: "FN-1",
        sentences: &[],
        repaired: &[br#"fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    // [FN-8] a user function called in a requirement clause.
    RepairPair {
        name: "fn8-neg-requires-user-call.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/fn8-neg-requires-user-call.wf"
        ),
        rule: "FN-8",
        sentences: &[],
        repaired: &[br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn identity(x: i32) -> result: i32 pure {
  return x;
}

fn user_call(x: i32) -> result: i32 pure {
  doc "User-function calls are forbidden in a requirement clause even when the callee is pure [FN-8].";
  return identity(x: x);
}

fn main() -> status: ExitStatus pure {
  return exit_status(code: 0_u8);
}
"#],
    },
    // [GRAM-12] an integer match whose label is no integer.
    RepairPair {
        name: "gram12-neg-float-label.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/gram12-neg-float-label.wf"
        ),
        rule: "GRAM-12",
        sentences: &[
            "\n  mechanical_fix: write an integer literal or named const of type u64\n",
        ],
        repaired: &[br#"fn selected(value: u64) -> result: unit pure {
  match value {
    1_u64 => {
      return unit;
    }
    else => {
      return unit;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    // [GRAM-12] an integer match with an else arm alone.
    RepairPair {
        name: "gram12-neg-else-only.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/gram12-neg-else-only.wf"
        ),
        rule: "GRAM-12",
        sentences: &[
            "\n  mechanical_fix: add a literal or named-const value arm\n",
        ],
        repaired: &[br#"fn selected(value: u64) -> result: unit pure {
  match value {
    1_u64 => {
      return unit;
    }
    else => {
      return unit;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    // [FN-8] a contract that holds definitions and no clause.
    RepairPair {
        name: "fn8-neg-requires-no-check.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/fn8-neg-requires-no-check.wf"
        ),
        rule: "FN-8",
        sentences: &[],
        repaired: &[br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn no_check(x: i32) -> result: i32 pure contract {
  define permitted = x >= 0_i32;
  requires permitted;
} {
  doc "A define-only contract with no requires or ensures clause is malformed [FN-8].";
  return x;
}

fn main() -> status: ExitStatus pure {
  return exit_status(code: 0_u8);
}
"#],
    },
    // [CONST-2] a set whose target is a constant.
    RepairPair {
        name: "const2-neg-set.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/const2-neg-set.wf"
        ),
        rule: "CONST-2",
        sentences: &[],
        repaired: &[br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

const table: Array<u8, 4> =[10_u8, 20_u8, 30_u8, 40_u8];

fn main() -> status: ExitStatus pure {
  doc "A const item is immutable: a set through it is rejected.";
  return exit_status(code: 0_u8);
}
"#],
    },
    // [GRAM-10] a match binder that reuses a live name.
    RepairPair {
        name: "x-match-gram10-binder-not-fresh.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/x-match-gram10-binder-not-fresh.wf"
        ),
        rule: "GRAM-10",
        sentences: &[],
        repaired: &[br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  doc "GRAM-10 freshness: a match binder that reuses a live outer name (w) is not fresh and is a hard error citing GRAM-10 [TYPE-6].";
  let w = 7_i32;
  let v = match 40_i32 +checked 2_i32 {
    Ok(value: z) => {
      give z;
    }
    Err(error: e) => {
      give 0_i32;
    }
  }
  return exit_status(code: 0_u8);
}
"#],
    },
    // [OP-1] a call to a name no declaration gives.
    RepairPair {
        name: "op1-neg-eeq-retired.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/op1-neg-eeq-retired.wf"
        ),
        rule: "OP-1",
        sentences: &[],
        repaired: &[br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn compare(left: u32, right: u32) -> return_value: Bool pure {
  return left == right;
}

fn main() -> return_value: ExitStatus pure {
  return exit_status(code: 0_u8);
}
"#],
    },
    // [TYPE-6] a `continue` to the label of a loop that is not enclosing.
    RepairPair {
        name: "continue-neg-non-enclosing-label.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/continue-neg-non-enclosing-label.wf"
        ),
        rule: "TYPE-6",
        sentences: &[],
        repaired: &[br#"fn main() -> status: std::process::ExitStatus pure {
  doc "Exercises the continue transfer.";
  loop @ended {
    break;
  }
  loop @again {
    continue @again;
  }
}
"#],
    },
    // [OP-5] a requirement whose expression is no Bool.
    RepairPair {
        name: "fn8-neg-requires-non-bool-check.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/fn8-neg-requires-non-bool-check.wf"
        ),
        rule: "OP-5",
        sentences: &[],
        repaired: &[br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn non_bool(x: i32) -> result: i32 pure contract {
  requires x >= 0_i32;
} {
  doc "A requires expression must have exact type Bool [FN-8/OP-5].";
  return x;
}

fn main() -> status: ExitStatus pure {
  return exit_status(code: 0_u8);
}
"#],
    },
    // [FORM-7] an integer literal outside its type.
    RepairPair {
        name: "form7-neg-out-of-range.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/form7-neg-out-of-range.wf"
        ),
        rule: "FORM-7",
        sentences: &[],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  doc "An out-of-range integer literal is a hard error at check time [FORM-7].";
  let a = 999999999_i32;
  return exit_status(code: 0_u8);
}
"##],
    },
    // [CONST-2] a constant of a type no constant may have.
    RepairPair {
        name: "const2-neg-noneligible.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/const2-neg-noneligible.wf"
        ),
        rule: "CONST-2",
        sentences: &[],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

const bad: i32 = 0_i32;

fn main() -> status: ExitStatus pure {
  doc "CONST-2 rejects a runtime-capacity owned container as a named constant; the container is reachable only as Box content, and neither Box nor Slots is const-eligible. The original integer initializer remains.";
  return exit_status(code: 0_u8);
}
"##],
    },
    // [CONST-1] a const expression written with a runtime overflow mode.
    RepairPair {
        name: "const1-neg-runtime-arithmetic-mode.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/const1-neg-runtime-arithmetic-mode.wf"
        ),
        rule: "CONST-1",
        sentences: &[
            "\n  mechanical_fix: write the bare operator: const evaluation rejects overflow at compile time and has no runtime arithmetic modes\n",
        ],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn doubled<const n: u64>(count: u64) -> result: Slots<u64, n * 2> pure {
  doc "Forms a run whose capacity is written with a mode-suffixed operator.";
  return slots_new::<u64, n * 2>();
}

fn main() -> status: ExitStatus pure {
  doc "The const-expression tail reuses infix_op but its spelling must be one of the five bare operators. Const evaluation has no runtime overflow mode, so a mode-suffixed spelling is a hard error citing CONST-1 at the infix_op node; the grammar admits and the checker restricts.";
  let cells = doubled::<3>(count: 7_u64);
  return exit_status(code: 0_u8);
}
"##],
    },
    // [TYPE-5] an argument of another type than its parameter.
    RepairPair {
        name: "type5-neg-arg-mismatch.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/type5-neg-arg-mismatch.wf"
        ),
        rule: "TYPE-5",
        sentences: &[],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn takes_i32(x: i32) -> function_result: unit pure {
  return unit;
}

fn main() -> status: ExitStatus pure {
  doc "A unit argument where i32 is declared violates exact argument typing [TYPE-5].";
  let r = takes_i32(x: 1_i32);
  return exit_status(code: 0_u8);
}
"##],
    },
    // [SET-1] a write to a counted loop binder.
    RepairPair {
        name: "set1-neg-counted-binder-write.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/set1-neg-counted-binder-write.wf"
        ),
        rule: "SET-1",
        sentences: &[],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  doc "A counted binder is immutable compiler-updated state, never source-writable storage [SET-1].";
  for @items (i in 0_u64..1_u64) {
    let copied = i;
  }
  return exit_status(code: 0_u8);
}
"##],
    },
    // [PROV-6] a linear value that leaves its scope unconsumed.
    RepairPair {
        name: "prov6-neg-linear-value-not-consumed.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/prov6-neg-linear-value-not-consumed.wf"
        ),
        rule: "PROV-6",
        sentences: &[
            "\n  mechanical_fix: move `lease` whole into a function that consumes it, or return it; or destructure the whole struct with `let N(f: a, ...) = move lease;`, replacing `N` and `f` with its declared struct and field names and consuming every linear field\n",
        ],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

nodrop struct Lease {
  slot: u8;
}

fn main() -> status: ExitStatus pure {
  doc "PROV-6: a value linear in this scope is live on the edge leaving it, where no compiler-derived release exists to carry it. The modifier holds in every scope, so the two routes that remain are the whole move and the whole destructuring.";
  let lease = Lease(slot: 3_u8);
  let Lease(slot: s) = move lease;
  return exit_status(code: 0_u8);
}
"##],
    },
    // [PROV-6] a consume of a part of a value whose other part is linear.
    RepairPair {
        name: "prov6-neg-linear-value-partially-consumed.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/prov6-neg-linear-value-partially-consumed.wf"
        ),
        rule: "PROV-6",
        sentences: &[
            "\n  mechanical_fix: destructure the whole value with let N(f: a, ...) = move v;\n",
        ],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

struct Token {
  id: u8;
}

struct Chunk {
  page: Slots<u8, 4>;
  tickets: Slots<Token, 2>;
}

fn main() -> status: ExitStatus pure {
  doc "PROV-6: a consume of a proper sub-place of a value one of whose remaining parts is linear abandons the residual the obligation was written for. The refusal is stated over the consume, so it reaches every consuming use of that sub-place, and its restructuring is the destructuring consume of the whole value.";
  let values = array_filled::<u8, 4>(value: 0_u8);
  let run = slots_from_array::<u8, 4>(values: values);
  let tickets = slots_new::<Token, 2>();
  let chunk = Chunk(page: move run, tickets: move tickets);
  let page = move chunk.page;
  return exit_status(code: 0_u8);
}
"##],
    },
    // [PROV-6] a type argument whose linearity class is not the written bound.
    RepairPair {
        name: "prov6-neg-linearity-bound-refuses-the-instantiation.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/prov6-neg-linearity-bound-refuses-the-instantiation.wf"
        ),
        rule: "PROV-6",
        sentences: &[],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

nocopy struct Lease {
  slot: u8;
}

fn discard<T: drop>(value: T) -> count: u8 pure {
  doc "The same body written for the affine class.";
  return 2_u8;
}

fn main() -> status: ExitStatus pure {
  doc "PROV-6, S32: an instantiation whose type argument's linearity class is not the written bound is refused at the call, naming the parameter, the bound and the argument. The declaration keeps one verdict; the call is where the class is decided.";
  let lease = Lease(slot: 3_u8);
  let n = discard::<Lease>(value: move lease);
  return exit_status(code: 0_u8);
}
"##],
    },
    // [LIV-1] branches that disagree on whether a value is live.
    RepairPair {
        name: "liv1-neg-branches-disagree-on-liveness.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/liv1-neg-branches-disagree-on-liveness.wf"
        ),
        rule: "LIV-1",
        sentences: &[
            "\n  mechanical_fix: every predecessor of a join agrees on a binding's live-or-dead status: consume it on every predecessor, on none, or commit a value back into it before the predecessor that consumed it reaches the join; alternatively, if this owner is no longer needed, consume it unconditionally outside the `if`: write `release::<Slots<u8, 4>>(value: move c);` before the code that needs it released and remove the conditional consume\n",
        ],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn measure(cell: Slots<u8, 4>) -> size: u64 pure {
  doc "Consumes the window it is handed and reports its length.";
  let n = cell.len;
  return n;
}

fn main() -> status: ExitStatus pure {
  doc "LIV-1: one branch consumes the window and the other does not, so the two predecessors of the join disagree about whether it is live there.";
  let values = array_filled::<u8, 4>(value: 0_u8);
  let c = slots_from_array::<u8, 4>(values: values);
  let flag = 1_u64;
  let taken = 0_u64;
  if flag == 1_u64 {
    set taken = measure(cell: move c);
  } else {
    set taken = measure(cell: move c);
  }
  return exit_status(code: 0_u8);
}
"##],
    },
    // [WIN-3] an assignment over a linear place.
    RepairPair {
        name: "win3-neg-overwrite-linear-place.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/win3-neg-overwrite-linear-place.wf"
        ),
        rule: "WIN-3",
        sentences: &[
            "\n  mechanical_fix: take the linear value out and consume it before writing this place\n",
        ],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

nodrop struct Token {
  value: u64;
}

fn main() -> status: ExitStatus pure {
  doc "WIN-3: assigning over an owned place is a hard error at the target place when the old value is linear.";
  let first = Token(value: 1_u64);
  let second = Token(value: 2_u64);
  let Token(value: old) = move first;
  set first = move second;
  let Token(value: v) = move first;
  if v == 2_u64 {
  } else {
    return exit_status(code: 1_u8);
  }
  return exit_status(code: 0_u8);
}
"##],
    },
    // [OP-11] a swap over copy places.
    RepairPair {
        name: "op11-neg-swap-copy-place.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/op11-neg-swap-copy-place.wf"
        ),
        rule: "OP-11",
        sentences: &[
            "\n  mechanical_fix: read the two values and assign them back\n",
        ],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  doc "OP-11: a swap over a copy place is a hard error at the first borrow expression; the restructuring is to read the two values and assign them back.";
  let a = 1_u64;
  let b = 2_u64;
  let held = a;
  set a = b;
  set b = held;
  if a == 2_u64 {
  } else {
    return exit_status(code: 1_u8);
  }
  return exit_status(code: 0_u8);
}
"##],
    },
    // [STOR-8] a Box named under the no-heap declaration.
    RepairPair {
        name: "stor8-neg-no-heap-names-box.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/stor8-neg-no-heap-names-box.wf"
        ),
        rule: "STOR-8",
        sentences: &[
            "\n  mechanical_fix: use a constant-capacity shape, or withdraw the no-heap declaration\n",
        ],
        repaired: &[br##"program no_heap;

alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

struct Cell {
  content: u64;
}

fn main() -> status: ExitStatus pure {
  doc "STOR-8: a program whose compilation unit carries the no-heap declaration cannot name Box or the runtime-capacity shapes; naming such a type is a hard error at the complete type.";
  return exit_status(code: 0_u8);
}
"##],
    },
    // [WIN-3] an arm whose rest covers a linear payload.
    RepairPair {
        name: "win3-neg-arm-rest-covers-linear-payload.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/win3-neg-arm-rest-covers-linear-payload.wf"
        ),
        rule: "WIN-3",
        sentences: &[
            "\n  mechanical_fix: bind the linear payload in the arm and consume it: V(f: a, ..) => { ... }\n",
        ],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

nodrop struct Token {
  id: u8;
}

enum Held {
  Full(code: u8, token: Token);
  Empty();
}

fn spend(token: Token) -> result: u8 pure {
  let Token(id: id) = move token;
  return id;
}

fn main() -> status: ExitStatus pure {
  doc "A final `..` would cover the linear `token`, which has no release, so the arm is refused; binding it and consuming it is the route.";
  let token = Token(id: 3_u8);
  let observed = Held::Full(code: 7_u8, token: move token);
  match move observed {
    Full(code: found, token: kept) => {
      let id = spend(token: move kept);
      return exit_status(code: found);
    }
    Empty() => {
      return exit_status(code: 1_u8);
    }
  }
}
"##],
    },
    // [OWN-1] a move out through a reference.
    RepairPair {
        name: "own1-neg-return-borrowed-box.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/own1-neg-return-borrowed-box.wf"
        ),
        rule: "OWN-1",
        sentences: &[
            "\n  mechanical_fix: consume a place rooted in a live own-mode binding of this function\n",
        ],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

enum State {
  Ready();
}

fn steal(holder: Box<State>) -> return_value: Box<State> pure {
  doc "A reference names the cell's path; the cell itself is affine and is owned by the binding the reference reaches, so it cannot be moved out through that reference.";
  return move holder;
}

fn main() -> return_value: ExitStatus pure {
  return exit_status(code: 0_u8);
}
"##],
    },
    // [TYPE-10] a write to a window part.
    RepairPair {
        name: "win2-neg-window-part-write-target.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/win2-neg-window-part-write-target.wf"
        ),
        rule: "TYPE-10",
        sentences: &[
            "\n  mechanical_fix: use the operation that moves the window boundary [OP-10]\n",
        ],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  doc "WIN-2: parts are vocabulary for effect rows and the overlap judgment only, so no program writes one; a write to a part in source position is a hard error at the complete target place.";
  let r = slots_new::<u64, 4>();
  let n = r.len;
  if n == 0_u64 {
  } else {
    return exit_status(code: 1_u8);
  }
  return exit_status(code: 0_u8);
}
"##],
    },
    // [TYPE-2] a write to a readonly measure.
    RepairPair {
        name: "type2-neg-readonly-measure-write-target.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/type2-neg-readonly-measure-write-target.wf"
        ),
        rule: "TYPE-2",
        sentences: &[
            "\n  mechanical_fix: use the operation that changes it, or replace the whole value\n",
        ],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  doc "TYPE-2: len is a readonly field of the prelude's Slots declaration, and a path that ends at a readonly field is never a set target; the restructuring is to use the operation that changes it, or replace the whole value.";
  let r = slots_new::<u64, 4>();
  let n = r.len;
  if n == 0_u64 {
  } else {
    return exit_status(code: 1_u8);
  }
  return exit_status(code: 0_u8);
}
"##],
    },
    // [SHARE-1] a frozen value that holds shared state.
    RepairPair {
        name: "share1-neg-frozen-shared-part.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/share1-neg-frozen-shared-part.wf"
        ),
        rule: "SHARE-1",
        sentences: &[
            "\n  mechanical_fix: replace `MutablePart.state` with immutable owned data or a Frozen handle before freezing the value\n",
        ],
        repaired: &[br##"struct MutablePart {
  state: u8;
}

fn denied(value: MutablePart) -> result: Frozen<MutablePart> pure {
  return frozen_new::<MutablePart>(value: move value);
}
"##],
    },
    // [SHARE-1] shared state that holds a linear handle to itself.
    RepairPair {
        name: "share1-neg-linear-state-holds-its-own-handle.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/share1-neg-linear-state-holds-its-own-handle.wf"
        ),
        rule: "SHARE-1",
        sentences: &[
            "\n  mechanical_fix: keep `Left.right.Some.value[state].left.Some.value` out of the state: hold the `Shared<Left>` outside the object, or give the inner object a state type of its own\n",
        ],
        repaired: &[br##"struct Left {
  right: Option<Shared<Right>>;
}

struct Right {
  left: Option<Shared<Left>>;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"##],
    },
    // [SHARE-1] a move out of a frozen value.
    RepairPair {
        name: "share1-neg-frozen-move-inner.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/share1-neg-frozen-move-inner.wf"
        ),
        rule: "SHARE-1",
        sentences: &[
            "\n  mechanical_fix: read copy-typed parts without move, borrow `&f.inner` or its descendants, or retain a nested frozen handle with frozen_share\n",
        ],
        repaired: &[br##"fn denied() -> result: u8 pure {
  let cell = box_new::<u8>(value: 7_u8);
  let observed = frozen_new::<Box<u8>>(value: move cell);
  return observed.inner.inner;
}
"##],
    },
    // [TYPE-2] a constructor call of a storage shape.
    RepairPair {
        name: "type2-neg-storage-shape-constructor-call.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/type2-neg-storage-shape-constructor-call.wf"
        ),
        rule: "TYPE-2",
        sentences: &[
            "\n  mechanical_fix: build it with a construction function [OP-13]\n",
        ],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  doc "TYPE-2: the storage shapes are the prelude's opaque structs, and an opaque struct's constructor entry exists to be refused, so a constructor call naming Slots is a hard error at the complete call; the repair is to build it with a construction function.";
  let s = slots_new::<u64, 4>();
  let n = s.len;
  if n == 0_u64 {
  } else {
    return exit_status(code: 1_u8);
  }
  return exit_status(code: 0_u8);
}
"##],
    },
    // [REF-2] a reference used after its owner moved.
    RepairPair {
        name: "ref2-neg-use-after-move.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/ref2-neg-use-after-move.wf"
        ),
        rule: "REF-2",
        sentences: &[
            "\n  mechanical_fix: form the reference again after that event\n",
        ],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

nocopy struct Owner {
  values: Array<u64, 4>;
}

fn main() -> status: ExitStatus pure {
  doc "REF-2: a move never re-roots an existing reference, so after let w = move v the references formed from v are invalid and are not reinterpreted as references into w.";
  let values = array_filled::<u64, 4>(value: 6_u64);
  let v = Owner(values: values);
  let w = move v;
  let p = &w.values[1_u64];
  let seen = p^;
  if seen == 6_u64 {
  } else {
    return exit_status(code: 1_u8);
  }
  return exit_status(code: 0_u8);
}
"##],
    },
    // [REF-1] a payload selected where no arm establishes its variant.
    RepairPair {
        name: "ref1-neg-body-payload-without-arm.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/ref1-neg-body-payload-without-arm.wf"
        ),
        rule: "REF-1",
        sentences: &[
            "\n  mechanical_fix: match the enum again and select the payload inside its variant arm before any overlapping write\n",
        ],
        repaired: &[br##"fn unselected(cell: &Option<u64>) -> result: u64 reads(cell) {
  doc "An arbitrary Option reference supplies no current Some fact for selecting its payload.";
  match &cell^ {
    None() => {
      return 0_u64;
    }
    Some(value: inner) => {
      let direct = cell^.Some.value;
      return direct;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"##],
    },
    // [REF-1] a reference formed over a reference variable.
    RepairPair {
        name: "ref1-neg-reference-to-reference-variable.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/ref1-neg-reference-to-reference-variable.wf"
        ),
        rule: "REF-1",
        sentences: &[
            "\n  mechanical_fix: name the path the reference names\n",
        ],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  doc "REF-1: a reference variable names a path and is not storage of its own, so &p where p is a reference variable is a hard error.";
  let a = 5_u64;
  let p = &a;
  let q = &a;
  let seen = q^;
  if seen == 5_u64 {
  } else {
    return exit_status(code: 1_u8);
  }
  return exit_status(code: 0_u8);
}
"##],
    },
    // [REF-3] a returned reference.
    RepairPair {
        name: "ref3-neg-returned-reference.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/ref3-neg-returned-reference.wf"
        ),
        rule: "REF-3",
        sentences: &[
            "\n  mechanical_fix: return an index and let the caller form the reference\n",
        ],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn leak(values: &[u64]) -> result: u64 reads(values) contract {
  requires values^.len > 0_u64;
} {
  doc "REF-3: a reference may not be returned; the restructuring is to return an index and let the caller form the reference.";
  return values^[0_u64];
}

fn main() -> status: ExitStatus pure {
  doc "REF-3: the caller of a function that tried to return a reference.";
  let a = array_filled::<u64, 4>(value: 6_u64);
  let part = &a[0_u64..4_u64];
  let seen = leak(values: part);
  if seen == 6_u64 {
  } else {
    return exit_status(code: 1_u8);
  }
  return exit_status(code: 0_u8);
}
"##],
    },
    // [REF-4] a range reference over a ring.
    RepairPair {
        name: "ref4-neg-range-over-ring.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/ref4-neg-range-over-ring.wf"
        ),
        rule: "REF-4",
        sentences: &[
            "\n  mechanical_fix: a ring hands out single slots; take the elements one at a time\n",
        ],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  doc "REF-4: a range reference over a Ring is a hard error, because a wrapped window is two extents and the range-reference kind has one len.";
  let r = array_filled::<u64, 4>(value: 1_u64);
  let part = &r[0_u64..2_u64];
  let n = part^.len;
  if n == 2_u64 {
  } else {
    return exit_status(code: 1_u8);
  }
  return exit_status(code: 0_u8);
}
"##],
    },
    // [REF-4] a range reference over map entries.
    RepairPair {
        name: "share-neg-map-entries-as-a-range.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/share-neg-map-entries-as-a-range.wf"
        ),
        rule: "REF-4",
        sentences: &[
            "\n  mechanical_fix: keyed entries are entries of a table, not a run of storage; reach them one at a time as `entries^[i]`\n",
        ],
        repaired: &[br##"const names: Array<u8, 2> =[97_u8, 98_u8];

fn main() -> status: std::process::ExitStatus pure waits {
  let store = shared_map_new::<u8>(capacity: 0_u64);
  let tally = shared_new::<u8>(value: 0_u8);
  let name = &names[0_u64..1_u64];
  let keys = key_set_new(capacity: 1_u64);
  key_set_insert(keys: &keys, key: name);
  atomic entries = &store[keys], c = &tally {
    set c^ = 1_u8;
  }
  return std::process::exit_status(code: 0_u8);
}
"##],
    },
    // [WIN-3] a move out of a window slot.
    RepairPair {
        name: "win3-neg-move-out-of-window-slot.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/win3-neg-move-out-of-window-slot.wf"
        ),
        rule: "WIN-3",
        sentences: &[
            "\n  mechanical_fix: use take_back, remove_at, or swap [OP-10, OP-11]\n",
        ],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  doc "WIN-3: there is no take operation and no hole, so a move out of a window slot is a hard error; the restructuring is take_back, remove_at, or swap.";
  let r = slots_new::<Box<u64>, 2>();
  let cell = box_new::<u64>(value: 4_u64);
  place_back(window: &r, value: move cell);
  let taken = take_back(window: &r);
  let seen = taken.inner;
  if seen == 4_u64 {
  } else {
    return exit_status(code: 1_u8);
  }
  return exit_status(code: 0_u8);
}
"##],
    },
    // [OWN-11] a loop that leaves an outer binding dead.
    RepairPair {
        name: "liv1-neg-loop-leaves-an-outer-binding-dead.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/liv1-neg-loop-leaves-an-outer-binding-dead.wf"
        ),
        rule: "OWN-11",
        sentences: &[
            "\n  mechanical_fix: one iteration must leave every outer binding in the status the next one starts from: commit a value back into it before the backedge, or declare and consume it inside the body\n",
        ],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn measure(cell: Slots<u8, 4>) -> size: u64 pure {
  doc "Consumes the window it is handed and reports its length.";
  let n = cell.len;
  return n;
}

fn main() -> status: ExitStatus pure {
  doc "OWN-11: the body consumes an outer binding and does not commit a value back into it, so the backedge disagrees with the entering edge.";
  let values = array_filled::<u8, 4>(value: 0_u8);
  let c = slots_from_array::<u8, 4>(values: values);
  for (i in 0_u64..2_u64) {
    let taken = measure(cell: move c);
    set c = slots_new::<u8, 4>();
  }
  return exit_status(code: 0_u8);
}
"##],
    },
    // [GRAM-6] a match over a Bool.
    RepairPair {
        name: "gram6-neg-bool-match.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/gram6-neg-bool-match.wf"
        ),
        rule: "GRAM-6",
        sentences: &[
            "\n  mechanical_fix: spell the Bool conditional `if`\n",
        ],
        repaired: &[br##"fn selected(value: Bool) -> result: unit pure {
  if value {
    return unit;
  } else {
    return unit;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"##],
    },
    // [OP-14] freeing a window that is not empty.
    RepairPair {
        name: "op14-neg-free-empty-nonempty-window.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/op14-neg-free-empty-nonempty-window.wf"
        ),
        rule: "OP-14",
        sentences: &[
            "\n  mechanical_fix: the window still holds elements here: take every element out and consume it before `free_empty`\n",
        ],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

nodrop struct Token {
  value: u64;
}

fn main() -> status: ExitStatus pure {
  doc "OP-14: an undischarged requires window.len == 0_u64 is a hard error at the complete call; the restructuring is to empty the window and establish its zero length at this point.";
  let r = slots_new::<Token, 2>();
  free_empty(window: move r);
  return exit_status(code: 0_u8);
}
"##],
    },
    // [OP-2] an addition whose overflow is not excluded.
    RepairPair {
        name: "op2-neg-overflow-obligation-undischarged.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/op2-neg-overflow-obligation-undischarged.wf"
        ),
        rule: "OP-2",
        sentences: &[
            "\n  mechanical_fix: add `requires x +defined 1_u64;` to the `contract` of `add_one`, which each caller then establishes; or guard the operation with `if x +defined 1_u64` where skipping it is the intended behavior; or write the `+wrap`, `+checked` or `+sat` form\n",
        ],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn add_one(x: u64) -> return_value: u64 pure {
  doc "OP-2 constant-operand class: `x + 1_u64` carries the overflow obligation that the exact mathematical result belongs to u64. Nothing bounds x, so the fact state does not discharge the upper conjunct and the call is rejected citing OP-2 with residual `x <= 18446744073709551614`. Repairs include a dominating branch, requirement, invariant, checked source proof, or the explicit wrap, checked, or sat spelling.";
  let y = x +wrap 1_u64;
  return y;
}

fn main() -> return_value: ExitStatus pure {
  let raised = add_one(x: 7_u64);
  return exit_status(code: 0_u8);
}
"##],
    },
    // [OP-6] a conversion whose domain is not proved.
    RepairPair {
        name: "op6-neg-unproved-domain.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/op6-neg-unproved-domain.wf"
        ),
        rule: "OP-6",
        sentences: &[
            "\n  mechanical_fix: add `requires cvt.defined::<u32, u8>(value);` to the `contract` of `narrow`, which each caller then establishes; or guard the conversion with `if cvt.defined::<u32, u8>(value)` where skipping it is the intended behavior; or use `cvt.checked::<u32, u8>` and handle its `Err`\n",
        ],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn narrow(value: u32) -> result: u8 pure contract {
  requires cvt.defined::<u32, u8>(value);
} {
  return cvt::<u32, u8>(value);
}

fn main() -> status: ExitStatus pure {
  return exit_status(code: 0_u8);
}
"##],
    },
    // [REF-4] a range end past the source length.
    RepairPair {
        name: "view2-neg-range-end-outside-source.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/view2-neg-range-end-outside-source.wf"
        ),
        rule: "REF-4",
        sentences: &[
            "\n  mechanical_fix: `5_u64 <= values.len` is false where this range is formed: choose endpoints that satisfy it\n",
        ],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  doc "A range reference must prove its end no greater than the source length [REF-4]; a written end past the array length is unproved and the formation is refused.";
  let values = array_filled::<u64, 4>(value: 0_u64);
  let view = &values[0_u64..4_u64];
  return exit_status(code: 0_u8);
}
"##],
    },
    // [EFF-5] two substituted effects that overlap.
    RepairPair {
        name: "eff5-neg-overlapping-substituted-write.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/eff5-neg-overlapping-substituted-write.wf"
        ),
        rule: "EFF-5",
        sentences: &[
            "\n  mechanical_fix: pass places that do not overlap, or pass the shared place through one argument only\n",
        ],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

struct Cell {
  value: u64;
}

fn copy_across(source: &Cell, destination: &Cell) -> result: unit reads(source.value), writes(destination.value) {
  doc "EFF-5: the callee declares one formal-path row that the call site substitutes.";
  let v = source^.value;
  set destination^.value = v;
  return unit;
}

fn main() -> status: ExitStatus pure {
  doc "EFF-5: two substituted effects on overlapping paths where at least one is a write must be proved disjoint, and are otherwise a hard error at the complete call; with no position to separate the two paths, the repair passes places that do not overlap.";
  let a = Cell(value: 4_u64);
  let b = Cell(value: 0_u64);
  copy_across(source: &a, destination: &b);
  let seen = a.value;
  if seen == 4_u64 {
  } else {
    return exit_status(code: 1_u8);
  }
  return exit_status(code: 0_u8);
}
"##],
    },
    // [OP-12] a callee that writes a prefix of the update target.
    RepairPair {
        name: "op12-neg-callee-writes-target-prefix.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/op12-neg-callee-writes-target-prefix.wf"
        ),
        rule: "OP-12",
        sentences: &[
            "\n  mechanical_fix: declare a row that reaches no prefix of the updated place: reading anything and writing storage disjoint from it is admitted, and an update whose callee must reach the place is written as ordinary statements instead\n",
        ],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

struct Counter {
  value: u64;
}

fn recomputed(current: u64, holder: &Box<Counter>) -> result: u64 reads(holder) {
  doc "OP-12 refuses this callee at the update site: its declared row writes a proper prefix of the target place.";
  return current;
}

fn main() -> status: ExitStatus pure {
  doc "OP-12: f's declared row must not write, move out of, or free any prefix of the target place; a row that does is a hard error at the complete call.";
  let start = Counter(value: 1_u64);
  let cell = box_new::<Counter>(value: start);
  set cell.inner.value = recomputed(current: cell.inner.value, holder: &cell);
  let seen = cell.inner.value;
  if seen == 1_u64 {
  } else {
    return exit_status(code: 1_u8);
  }
  return exit_status(code: 0_u8);
}
"##],
    },
    // [FN-8] a call that fails a requirement at a constant.
    RepairPair {
        name: "fn8-neg-constant-call-requirement.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/fn8-neg-constant-call-requirement.wf"
        ),
        rule: "FN-8",
        sentences: &[
            "\n  mechanical_fix: `0_u8 == 1_u8` is false for the values that reach this call, so no fact can establish it here: pass arguments that satisfy it, or change the statements or requirements that fix those values\n",
        ],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn only_one(value: u8) -> result: unit pure contract {
  requires value == 1_u8;
} {
  return unit;
}

fn main() -> status: ExitStatus pure {
  only_one(value: 1_u8);
  return exit_status(code: 0_u8);
}
"##],
    },
    // [ENT-2] a counted endpoint that is a subscripted element.
    RepairPair {
        name: "ent2-neg-counted-subscript-endpoint.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/ent2-neg-counted-subscript-endpoint.wf"
        ),
        rule: "ENT-2",
        sentences: &[
            "\n  mechanical_fix: bind the computed u64 value with one preceding ordinary let and use that term as the endpoint\n",
        ],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn probe(bounds: Array<u64, 2>) -> result: unit pure {
  doc "A subscript is a valid atom whose own constant bounds discharge, but a subscripted place is a counted endpoint term only when its final step selects a readonly field, and a scalar element is none [ENT-2].";
  let lo = bounds[0_u64];
  let hi = bounds[1_u64];
  for @items (i in lo..hi) {
  }
  return unit;
}

fn main() -> status: ExitStatus pure {
  doc "No construction is needed to exercise the endpoint admission.";
  return exit_status(code: 0_u8);
}
"##],
    },
    // [RANGE-1] a definition that multiplies two parameters.
    RepairPair {
        name: "range1-neg-define-nonlinear-requirement-index.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/range1-neg-define-nonlinear-requirement-index.wf"
        ),
        rule: "RANGE-1",
        sentences: &[
            "\n  mechanical_fix: multiply one term by an integer literal or a named integer const\n",
        ],
        repaired: &[br##"fn need_small(source: &[u64], i: u64, j: u64) -> result: unit reads(source) contract {
  define next = i * 2_u64;
  requires next < source^.len;
  requires forall small(k in 0_u64..1_u64): source^[next] <= 7_u64;
} {
  doc "Expanding next exposes multiplication of two integer parameters, which is not a range term.";
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"##],
    },
    // [RANGE-3] a fill fact overwritten before it is used.
    RepairPair {
        name: "range2-neg-fill-overwritten.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/range2-neg-fill-overwritten.wf"
        ),
        rule: "RANGE-3",
        sentences: &[
            "\n  mechanical_fix: establish the fact before this site: a range `requires`, a range invariant of the enclosing counted loop, or a guard that excludes the uncovered elements\n",
        ],
        repaired: &[br##"fn all_zero(cells: &[u64]) -> result: u64 reads(cells) contract {
  requires forall zero(k in 0_u64..cells^.len): cells^[k] == 0_u64;
} {
  doc "Returns the first cell, which its requirement makes zero.";
  if 0_u64 < cells^.len {
    let first = cells^[0_u64];
    return first;
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  doc "A box_array_filled result holds its fill value in every element, which discharges the requirement at the call; after one element is written with another value the fact no longer holds at it.";
  let cells = box_array_filled::<u64>(count: 8_u64, value: 0_u64);
  let first = all_zero(cells: &cells.inner[0_u64..8_u64]);
  let again = all_zero(cells: &cells.inner[0_u64..8_u64]);
  if first != 0_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  if again != 0_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"##],
    },
    // [INV-1] a call as an affine factor of an invariant.
    RepairPair {
        name: "inv1-neg-an-affine-factor-is-not-a-measure-former.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/inv1-neg-an-affine-factor-is-not-a-measure-former.wf"
        ),
        rule: "INV-1",
        sentences: &[
            "\n  mechanical_fix: write P.len, P.cap or P.head over a measured place\n",
        ],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  doc "[INV-1] a `call` in an affine-factor position is refused at the call node. The admitted atoms are a measure member on an admitted measure place [OP-15], a bare IDENT place, and an integer literal, so widening the production admits one closed set of factors rather than every runtime expression form.";
  let run = slots_new::<u8, 8>();
  for (
    at in 0_u64..8_u64,
    invariant grown: run.len >= at,
    invariant spare: run.cap - run.len + at >= 8_u64
  ) {
    place_back(window: &run, value: 0_u8);
  }
  let seen = 0_u64;
  let limit = 4_u64;
  for (
    step in 0_u64..4_u64,
    invariant bounded: seen <= limit
  ) {
    let byte = run[step];
  }
  return exit_status(code: 0_u8);
}
"##],
    },
    // [INV-1] a loop invariant not proved at the base.
    RepairPair {
        name: "inv1-neg-base-unproved.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/inv1-neg-base-unproved.wf"
        ),
        rule: "INV-1",
        sentences: &[
            "\n  mechanical_fix: `limit` is false on entry to the loop: correct it, or change the values the loop starts from\n",
        ],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  for (
    i in 0_u64..1_u64,
    invariant limit: i <= 1_u64
  ) {
  }
  return exit_status(code: 0_u8);
}
"##],
    },
    // [INV-1] a local invariant that nothing proves.
    RepairPair {
        name: "inv1-neg-local-relation-unproved.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/inv1-neg-local-relation-unproved.wf"
        ),
        rule: "INV-1",
        sentences: &[
            "\n  mechanical_fix: `within_cap` is not proved from the facts that reach it: establish them before it, add `use` steps naming the facts it follows from, or weaken it\n",
        ],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn clamp_hint(x: u8, limit: u8) -> result: u8 pure contract {
  requires x <= limit;
} {
  invariant within_cap: x <= limit;
  return x;
}

fn main() -> status: ExitStatus pure {
  return exit_status(code: 0_u8);
}
"##],
    },
    // [PRF-1] a source proof with the noncanonical factor one.
    RepairPair {
        name: "prf1-neg-explicit-factor-one.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/prf1-neg-explicit-factor-one.wf"
        ),
        rule: "PRF-1",
        sentences: &[
            "\n  mechanical_fix: omit `1 *` from this use\n",
        ],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn scale_pair(first: u64, first_limit: u64, second: u64, second_limit: u64) -> result: unit pure contract {
  requires first <= first_limit;
  requires second <= second_limit;
} {
  invariant pair_bound: first + second <= first_limit + second_limit;
  invariant scaled_bound: 3_u64 * first + 3_u64 * second <= 3_u64 * first_limit + 3_u64 * second_limit {
    use 3 times pair_bound;
  }
  return unit;
}

fn main() -> status: ExitStatus pure {
  return exit_status(code: 0_u8);
}
"##],
    },
    // [PRF-1] a source proof whose premise nothing establishes.
    RepairPair {
        name: "prf1-neg-unproved-premise.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/prf1-neg-unproved-premise.wf"
        ),
        rule: "PRF-1",
        sentences: &[
            "\n  mechanical_fix: establish this use relation from facts already available before the invariant statement, or replace it with a relation AUTO can prove in that same entering context\n",
        ],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn increment(x: u8, middle: u8) -> result: u8 pure contract {
  requires x <= middle;
  requires middle <= 254_u8;
} {
  invariant upper_bound: x <= 254_u8;
  return x;
}

fn main() -> status: ExitStatus pure {
  return exit_status(code: 0_u8);
}
"##],
    },
    // [SHARE-2] a map key that is no byte window.
    RepairPair {
        name: "share-neg-map-key-not-bytes.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/share-neg-map-key-not-bytes.wf"
        ),
        rule: "SHARE-2",
        sentences: &[
            "\n  mechanical_fix: name one key as a `&[u8]` range, such as `&bytes[start..end]` or a reference variable holding one, or several keys as a place of type `KeySet` built before the statement, such as `keys` or, through a reference to one, `keys^`\n",
        ],
        repaired: &[br##"const names: Array<u8, 2> =[97_u8, 98_u8];

fn main() -> status: std::process::ExitStatus pure waits {
  let store = shared_map_new::<u8>(capacity: 0_u64);
  let name = &names[0_u64..1_u64];
  atomic slot = &store[name] {
    set slot^ = Some<u8>(value: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"##],
    },
    // [SHARE-2] a guard that reads a decision cell.
    RepairPair {
        name: "decision-cell-neg-guard-read.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/decision-cell-neg-guard-read.wf"
        ),
        rule: "SHARE-2",
        sentences: &[
            "\n  mechanical_fix: read or claim the decision cell in the atomic body; use ordinary shared state for a waiting guard dependency\n",
        ],
        repaired: &[br##"fn is_written(cell: &DecisionCell) -> result: Bool reads(cell) {
  let seen = decision_cell_read(cell: cell);
  let written = Written();
  return seen == written;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let cell = decision_cell_new();
  let target = shared_new::<u8>(value: 0_u8);
  atomic byte = &target {
    set byte^ = 1_u8;
  }
  return std::process::exit_status(code: 0_u8);
}
"##],
    },
    // [FN-10] a musttail call that is not the whole return.
    RepairPair {
        name: "fn10-neg-not-return.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/fn10-neg-not-return.wf"
        ),
        rule: "FN-10",
        sentences: &[],
        repaired: &[br##"fn walk(n: u64) -> result: u64 pure {
  return musttail walk(n: n);
}
"##],
    },
    // [HOLD-1] a held contract on a callable that waits.
    RepairPair {
        name: "hold-neg-waiting-contract.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/hold-neg-waiting-contract.wf"
        ),
        rule: "HOLD-1",
        sentences: &[
            "\n  mechanical_fix: declare one nonempty held group of distinct reference parameters on a nonwaiting callable with a pure or reads row; pass references from one atomic block or the enclosing declared held group, and prove writes disjoint from their protected regions or move those writes outside the block\n",
        ],
        repaired: &[br##"fn sample(value: &u64) -> result: u64 reads(value) contract {
  requires held(value);
} {
  return value^;
}
"##],
    },
    // [SNAP-1] a snapshot of a value that holds shared state.
    RepairPair {
        name: "snapshot-neg-shared.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/snapshot-neg-shared.wf"
        ),
        rule: "SNAP-1",
        sentences: &[
            "\n  mechanical_fix: select a value type whose complete owned graph contains only primitives, declared structs and enums, storage shapes and owned Box contents\n",
        ],
        repaired: &[br##"alias Reserve = std::snapshot::Reserve;
alias Job = std::snapshot::Job;
alias CreateError = std::snapshot::CreateError;

struct Inner {
  state: u8;
}

struct Entry {
  inner: Box<Inner>;
}

fn capture(reserve: Reserve, table: &Shared<ConcurrentHashMap<Entry>>, sequence: &Shared<u64>) -> result: Result<Job, CreateError> reads(table), reads(sequence) waits {
  atomic held_map = &table^, held_cut = &sequence^ {
    return std::snapshot::capture_export::<Entry>(reserve: move reserve, map: held_map, cut: held_cut);
  }
}
"##],
    },
    // [CALL-4] a routed ensures that names no result ordinal.
    RepairPair {
        name: "call4-neg-enum-ambiguous-ordinal.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/call4-neg-enum-ambiguous-ordinal.wf"
        ),
        rule: "CALL-4",
        sentences: &[
            "\n  mechanical_fix: more than one result can carry this route: name the one it applies to, writing `when r is` before its variant with `r` one of `left`, `right`\n",
        ],
        repaired: &[br##"enum RoutedOrdinal {
  Item(index: u64);
}

fn pair() -> (left: RoutedOrdinal, right: RoutedOrdinal) pure contract {
  ensures when left is Item(index: payload): payload == 1_u64;
} {
  let left_value = RoutedOrdinal::Item(index: 1_u64);
  let right_value = RoutedOrdinal::Item(index: 2_u64);
  return left_value, right_value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"##],
    },
    // [CONST-1] a const expression naming a constant that is no integer.
    RepairPair {
        name: "const1-neg-noninteger.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/const1-neg-noninteger.wf"
        ),
        rule: "CONST-1",
        sentences: &[],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

const name: u64 = 4_u64;

fn main() -> status: ExitStatus pure {
  doc "An IDENT resolving to a non-integer (unit-typed) const used in a const-expression position, here a run capacity, is a compile-time rejection.";
  let xs = slots_new::<i32, name>();
  return exit_status(code: 0_u8);
}
"##],
    },
    // [RANGE-5] a fact that does not separate two iterations.
    RepairPair {
        name: "range5-neg-fact-does-not-separate.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/range5-neg-fact-does-not-separate.wf"
        ),
        rule: "RANGE-5",
        sentences: &[
            "\n  mechanical_fix: add `use` steps whose instances separate the two accesses, or remove `apart` to leave the loop sequential\n",
        ],
        repaired: &[br##"fn scatter(order: &[u64], pos: &[u64], out: &[u64]) -> result: unit reads(order), writes(out) contract {
  requires pos^.len == out^.len;
  requires forall inv(k in 0_u64..order^.len) when order^[k] < out^.len: pos^[order^[k]] == k;
} {
  doc "Writes each iteration's index at the slot order names. inv bounds pos only from above, which two iterations naming one slot satisfy, so the certificate does not hold.";
  let count = order^.len;
  for (
    k in 0_u64..count,
    apart(i, j) {
    }
  ) {
    let e = order^[k];
    if e < out^.len {
      set out^[e] = k;
    }
  }
  return unit;
}

fn reversed(n: u64) -> result: u64 pure contract {
  requires n <= 1000_u64;
  requires n >= 1_u64;
} {
  doc "Builds the reversal permutation order[k] = n - 1 - k with its inverse pos, proving inv by two loop invariants, and scatters through it.";
  let order = box_array_filled::<u64>(count: n, value: 0_u64);
  let pos = box_array_filled::<u64>(count: n, value: 0_u64);
  let out = box_array_filled::<u64>(count: n, value: 0_u64);
  let last = n - 1_u64;
  for (
    k in 0_u64..n,
    invariant forall vals(q in 0_u64..k): order.inner[q] == last - q,
    invariant forall inv(q in 0_u64..k): pos.inner[order.inner[q]] == q
  ) {
    let back = last - k;
    set order.inner[k] = back;
    set pos.inner[back] = k;
  }
  scatter(order: &order.inner[0_u64..n], pos: &pos.inner[0_u64..n], out: &out.inner[0_u64..n]);
  let first = out.inner[0_u64];
  return first;
}

fn main() -> status: std::process::ExitStatus pure {
  let got = reversed(n: 5_u64);
  if got != 4_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"##],
    },
    // [FN-9] a result datum spelled like a module alias.
    RepairPair {
        name: "fn9-neg-result-beside-module-alias.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/fn9-neg-result-beside-module-alias.wf"
        ),
        rule: "FN-9",
        sentences: &[],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;
alias process = std::process;

fn capped(limit: u64) -> capped_value: u64 pure contract {
  ensures capped_value <= limit;
} {
  doc "The result datum takes the spelling of a module alias, which is in both use classes, so the candidate competes with it and is not fresh [FN-9, TYPE-6].";
  return limit;
}

fn main() -> status: ExitStatus pure {
  return exit_status(code: 0_u8);
}
"##],
    },
    // [FN-9] an ensures relation of more than two terms.
    RepairPair {
        name: "fn9-neg-affine-two-parameter-terms.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/fn9-neg-affine-two-parameter-terms.wf"
        ),
        rule: "FN-9",
        sentences: &[],
        repaired: &[br##"fn bounded(base: u64, limit: u64, spare: u64) -> after: u64 pure contract {
  ensures after <= base + limit;
} {
  doc "The extension admits one extra bare parameter term on one side, not an arbitrary sum of three quantities; formation must reject FN-9 before proving the trivial body.";
  return base;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"##],
    },
    // [CALL-6] ensures clauses that contradict each other.
    RepairPair {
        name: "call6-neg-contradictory-published-relations.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/call6-neg-contradictory-published-relations.wf"
        ),
        rule: "CALL-6",
        sentences: &[
            "\n  mechanical_fix: state one consistent relation set: a contract whose clauses cannot hold together publishes every fact at every caller\n",
        ],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn measure(taken: Slots<u8, 4>) -> measured: u64 pure contract {
  ensures measured <= taken.len;
} {
  doc "The two published relations cannot hold together, so every caller goal would discharge from the contradiction alone.";
  return 0_u64;
}

fn main() -> status: ExitStatus pure {
  doc "Two ensures clauses publish relations that cannot hold together. At a contradictory point every relation and both signs of every goal are derivable, so an inconsistent contract is not one wrong fact at one caller but every fact at every caller; the set is fixed at the declaration and refused there.";
  let run = slots_new::<u8, 4>();
  place_back(window: &run, value: 0_u8);
  place_back(window: &run, value: 0_u8);
  place_back(window: &run, value: 0_u8);
  place_back(window: &run, value: 0_u8);
  let measured = measure(taken: move run);
  return exit_status(code: 0_u8);
}
"##],
    },
    // [FN-9] a call result returned under a clause that names it.
    RepairPair {
        name: "fn9-neg-return-call-result.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/fn9-neg-return-call-result.wf"
        ),
        rule: "FN-9",
        sentences: &[],
        repaired: &[br##"fn same(value: u64) -> result: u64 pure contract {
  ensures result == value;
} {
  return value;
}

fn relay(value: u64) -> result: u64 pure contract {
  ensures result == value;
} {
  doc "A call is no computed integer expression: a returned call result evaluates to no term, so the clause naming the result rejects at this return.";
  let r = same(value: value);
  return r;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"##],
    },
    // [FN-9] a routed ensures that no return selects.
    RepairPair {
        name: "fn9-neg-no-selected-normal-exit.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/fn9-neg-no-selected-normal-exit.wf"
        ),
        rule: "FN-9",
        sentences: &[
            "\n  mechanical_fix: no `return` of this function delivers a value this clause's route selects: return such a value on some path, or delete the clause\n",
        ],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn unselected() -> result: Result<i32, Overflow> pure contract {
  ensures when Ok(value: result): result == 0_i32;
} {
  return Ok<i32, Overflow>(value: 0_i32);
}

fn main() -> status: ExitStatus pure {
  return exit_status(code: 0_u8);
}
"##],
    },
    // [GRAM-11] a call that names a parameter the callee does not declare.
    RepairPair {
        name: "gram11-neg-misspelled.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/gram11-neg-misspelled.wf"
        ),
        rule: "GRAM-11",
        sentences: &[],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn add2(x: i32, y: i32) -> return_value: i32 pure {
  return x +wrap y;
}

fn main() -> return_value: ExitStatus pure {
  doc "GRAM-11: a misspelled parameter name in a call is a hard error citing GRAM-11.";
  let r = add2(x: 40_i32, y: 2_i32);
  return exit_status(code: 0_u8);
}
"##],
    },
    // [EFF-1] an effect row whose entries are out of order.
    RepairPair {
        name: "eff1-neg-wrong-order-row.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/eff1-neg-wrong-order-row.wf"
        ),
        rule: "EFF-1",
        sentences: &[
            "\n  mechanical_fix: write every `reads` entry before the first `writes` entry, and delete each entry whose path is a `writes` entry's path or lies below it, which that `writes` already covers\n",
        ],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn f(y: &Slots<i32, 4>, x: &Slots<i32, 4>) -> result: i32 reads(x), writes(y) contract {
  requires y^.len < y^.cap;
  requires x^.len > 0_u64;
} {
  doc "Row order violates EFF-1's canonical order: every reads entry must precede every writes entry.";
  place_back(window: y, value: 1_i32);
  return x^[0_u64];
}

fn main() -> status: ExitStatus pure {
  doc "The malformed declaration above is rejected before this independent entry executes.";
  return exit_status(code: 0_u8);
}
"##],
    },
    // [FN-3] a generic bound that names a source interface.
    RepairPair {
        name: "fn3-neg-source-contract-bound.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/fn3-neg-source-contract-bound.wf"
        ),
        rule: "FN-3",
        sentences: &[],
        repaired: &[br##"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

interface SourceBound {
}

fn identity<T: Int>(value: T) -> result: T pure {
  return value;
}

fn main() -> status: ExitStatus pure {
  return exit_status(code: 0_u8);
}
"##],
    },
    // [TYPE-6] a `break` outside every loop.
    RepairPair {
        name: "probe-break-outside-loop.wf",
        rejected: br##"fn main() -> status: std::process::ExitStatus pure {
  break;
}
"##,
        rule: "TYPE-6",
        sentences: &[
            "\n  mechanical_fix: move `break;` inside a loop or remove it\n",
        ],
        repaired: &[br##"fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"##],
    },
    // [ERR-2] two arms for one variant.
    RepairPair {
        name: "probe-duplicate-match-variant.wf",
        rejected: br##"enum Flag {
  A();
  B();
}

fn main() -> status: std::process::ExitStatus pure {
  let flag = Flag::A();
  match flag {
    A() => {
    }
    A() => {
    }
    B() => {
    }
  }
  return std::process::exit_status(code: 0_u8);
}
"##,
        rule: "ERR-2",
        sentences: &[
            "\n  mechanical_fix: remove the repeated variant arm\n",
        ],
        repaired: &[br##"enum Flag {
  A();
  B();
}

fn main() -> status: std::process::ExitStatus pure {
  let flag = Flag::A();
  match flag {
    A() => {
    }
    B() => {
    }
  }
  return std::process::exit_status(code: 0_u8);
}
"##],
    },
    // [CONST-2] two constants that define each other.
    RepairPair {
        name: "probe-constant-cycle.wf",
        rejected: br##"const a: u64 = b;

const b: u64 = a;

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"##,
        rule: "CONST-2",
        sentences: &[],
        repaired: &[br##"const a: u64 = 1_u64;

const b: u64 = a;

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"##],
    },
    // [CONST-2] a reference to a constant passed where the callee writes.
    RepairPair {
        name: "probe-constant-as-written-argument.wf",
        rejected: br##"const permanent: u64 = 1_u64;

fn overwrite(target: &u64) -> result: unit writes(target) {
  set target^ = 9_u64;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  overwrite(target: &permanent);
  return std::process::exit_status(code: 0_u8);
}
"##,
        rule: "CONST-2",
        sentences: &[
            "\n  mechanical_fix: copy the value into a local binding and pass a reference to that writable copy\n",
        ],
        repaired: &[br##"const permanent: u64 = 1_u64;

fn overwrite(target: &u64) -> result: unit writes(target) {
  set target^ = 9_u64;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let copied = permanent;
  overwrite(target: &copied);
  return std::process::exit_status(code: 0_u8);
}
"##],
    },
    // [GRAM-2] a heap declaration after an item.
    RepairPair {
        name: "probe-heap-declaration-after-item.wf",
        rejected: br##"fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}

program no_heap;
"##,
        rule: "GRAM-2",
        sentences: &[],
        repaired: &[br##"program no_heap;

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"##],
    },
    // [GRAM-2] a second heap declaration.
    RepairPair {
        name: "probe-second-heap-declaration.wf",
        rejected: br##"program no_heap;

program no_heap;

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"##,
        rule: "GRAM-2",
        sentences: &[],
        repaired: &[br##"program no_heap;

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"##],
    },
    // [TYPE-6] a construction that names a variant its enum does not declare.
    RepairPair {
        name: "probe-unknown-owned-variant.wf",
        rejected: br##"enum Sign {
  Neg();
}

fn main() -> status: std::process::ExitStatus pure {
  let s = Sign::Pos();
  return std::process::exit_status(code: 0_u8);
}
"##,
        rule: "TYPE-6",
        sentences: &[],
        repaired: &[br##"enum Sign {
  Neg();
}

fn main() -> status: std::process::ExitStatus pure {
  let s = Sign::Neg();
  return std::process::exit_status(code: 0_u8);
}
"##],
    },
    // [SHARE-2] a map key read from a state the same statement holds.
    RepairPair {
        name: "share-neg-map-key-reads-a-target.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/share-neg-map-key-reads-a-target.wf"
        ),
        rule: "SHARE-2",
        sentences: &[
            "\n  mechanical_fix: read handles and keys before the statement: copy a handle held in a state out with `shared_share` in an earlier atomic statement, and compute a key that comes from a state into a local the same way; a statement reads its handles and keys when it begins, before it holds any state\n",
        ],
        repaired: &[br#"fn main() -> status: std::process::ExitStatus pure waits {
  let store = shared_map_new::<u8>(capacity: 1_u64);
  let name = array_filled::<u8, 2>(value: 97_u8);
  atomic slot = &store[&name[0_u64..1_u64]] {
    set slot^ = Some<u8>(value: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    // [ERR-2] an integer match with no final `else` arm.
    RepairPair {
        name: "err2-neg-integer-missing-else.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/err2-neg-integer-missing-else.wf"
        ),
        rule: "ERR-2",
        sentences: &[
            "\n  mechanical_fix: add one final `else => { ... }` arm to cover every other value\n",
        ],
        repaired: &[br#"fn selected(value: u8) -> result: unit pure {
  match value {
    0_u8 => {
      return unit;
    }
    else => {
      return unit;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    // [FN-9] a postcondition clause that relates a conversion the type does not make total.
    RepairPair {
        name: "fn9-neg-narrowing-conversion-relation.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/fn9-neg-narrowing-conversion-relation.wf"
        ),
        rule: "FN-9",
        sentences: &[],
        repaired: &[br#"fn low_half(value: u64) -> result: u32 pure {
  let low = cvt.wrap::<u64, u32>(value);
  return low;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    // [TYPE-7] a borrow of the run a reference parameter names, taken whole.
    RepairPair {
        name: "type7-neg-range-referent-borrow-parameter.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/type7-neg-range-referent-borrow-parameter.wf"
        ),
        rule: "TYPE-7",
        sentences: &[
            "\n  mechanical_fix: write `let q = r;` to alias the reference, `&r^[i]` to borrow one element, or `&r^[a..b]` to borrow a subrange\n",
        ],
        repaired: &[br#"fn probe(r: &[u64]) -> result: unit pure {
  let q = r;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    // [FN-9] a later definition that takes the name a routed result binder wrote.
    RepairPair {
        name: "probe-postcondition-local-shadows-result.wf",
        rejected: br#"fn some_value(limit: u64) -> result: Option<u64> pure contract {
  ensures when Some(value: found): found <= limit;
  define found = limit;
} {
  return Some<u64>(value: limit);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FN-9",
        sentences: &[],
        repaired: &[br#"fn some_value(limit: u64) -> result: Option<u64> pure contract {
  ensures when Some(value: found): found <= limit;
} {
  return Some<u64>(value: limit);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
];
