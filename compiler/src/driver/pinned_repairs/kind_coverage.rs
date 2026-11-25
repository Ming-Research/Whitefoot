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
        sentences: &[],
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
        sentences: &[],
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
        sentences: &[],
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
        sentences: &[],
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
        rejected: br#"fn main() -> status: std::process::ExitStatus pure {
  let flag = True();
  let result = if flag {
  } else {
    give 0_i32;
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "GIVE-1",
        sentences: &[],
        repaired: &[br#"fn main() -> status: std::process::ExitStatus pure {
  let flag = True();
  let result = if flag {
    give 1_i32;
  } else {
    give 0_i32;
  }
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
        sentences: &[],
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
        sentences: &[],
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
];
