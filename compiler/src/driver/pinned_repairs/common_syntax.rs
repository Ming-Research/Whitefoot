//! Common syntax repairs, each pinned with its literal application.

use super::RepairPair;

pub(super) const COMMON_SYNTAX: &[RepairPair] = &[
    RepairPair {
        // [FN-9] return a tracked place; the widening operations are
        // preceding body lets, matching the contract definition chain.
        name: "syntax-nested-call-contract-define.wf",
        rejected: br#"fn widen(value: u8) -> result: u64 pure contract {
  define wide = cvt::<u32, u64>(cvt::<u8, u32>(value));
  ensures result == wide;
} {
  let widened_u32 = cvt::<u8, u32>(value);
  let widened_u64 = cvt::<u32, u64>(widened_u32);
  return widened_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "GRAM-9",
        sentences: &[
            "\n  mechanical_fix: if the inner operation is a non-consuming, pure, total form admitted by [FN-8], bind it with a fresh preceding `define` in this contract block and use that binder in the atom position; for example, split `define wide = cvt::<u32, u64>(cvt::<u8, u32>(value));` into `define inner = cvt::<u8, u32>(value); define wide = cvt::<u32, u64>(inner);` [GRAM-2, GRAM-9]\n",
        ],
        repaired: &[br#"fn widen(value: u8) -> result: u64 pure contract {
  define inner = cvt::<u8, u32>(value);
  define wide = cvt::<u32, u64>(inner);
  ensures result == wide;
} {
  let widened_u32 = cvt::<u8, u32>(value);
  let widened_u64 = cvt::<u32, u64>(widened_u32);
  return widened_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        // The outer call is an affine factor; its argument is atom-only, so
        // the inner `cvt ::` start reaches DIAG-1 row 2, not an earlier error.
        // Hoisting it would lose the clause-local payload [TYPE-6, FN-9].
        // Direct widening is an accepted control, not an emitted repair.
        name: "syntax-nested-call-routed-payload-no-hoist.wf",
        rejected: br#"fn wrapped(value: u8) -> result: Option<u8> pure contract {
  ensures when Some(value: payload): cvt::<u32, u64>(cvt::<u8, u32>(payload)) == cvt::<u8, u64>(value);
} {
  return Some<u8>(value: value);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "GRAM-9",
        sentences: &[],
        repaired: &[br#"fn wrapped(value: u8) -> result: Option<u8> pure contract {
  ensures when Some(value: payload): cvt::<u8, u64>(payload) == cvt::<u8, u64>(value);
} {
  return Some<u8>(value: value);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "syntax-effect-row.wf",
        rejected: br#"fn value() -> result: u32 {
  return 0_u32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "GRAM-3",
        sentences: &[
            "\n  mechanical_fix: write an effect row after the result binding: `pure` for no effects, or `reads(path)` and `writes(path)` entries describing the body's reference accesses; write `waits` after that row [GRAM-2, EFF-1]\n",
        ],
        repaired: &[br#"fn value() -> result: u32 pure {
  return 0_u32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "syntax-waits-after-row.wf",
        rejected: br#"fn value() -> result: u32 waits {
  return 0_u32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "GRAM-3",
        sentences: &[
            "\n  mechanical_fix: write an effect row after the result binding: `pure` for no effects, or `reads(path)` and `writes(path)` entries describing the body's reference accesses; write `waits` after that row [GRAM-2, EFF-1]\n",
        ],
        repaired: &[br#"fn value() -> result: u32 pure waits {
  return 0_u32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "syntax-effect-row-reads.wf",
        rejected: br#"fn value(cell: &u32) -> result: u32 {
  return cell^;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "GRAM-3",
        sentences: &[
            "\n  mechanical_fix: write an effect row after the result binding: `pure` for no effects, or `reads(path)` and `writes(path)` entries describing the body's reference accesses; write `waits` after that row [GRAM-2, EFF-1]\n",
        ],
        repaired: &[br#"fn value(cell: &u32) -> result: u32 reads(cell) {
  return cell^;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "syntax-effect-row-writes.wf",
        rejected: br#"fn value(cell: &u32) -> result: u32 {
  set cell^ = 1_u32;
  return 0_u32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "GRAM-3",
        sentences: &[
            "\n  mechanical_fix: write an effect row after the result binding: `pure` for no effects, or `reads(path)` and `writes(path)` entries describing the body's reference accesses; write `waits` after that row [GRAM-2, EFF-1]\n",
        ],
        repaired: &[br#"fn value(cell: &u32) -> result: u32 writes(cell) {
  set cell^ = 1_u32;
  return 0_u32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "syntax-infix-call-argument.wf",
        rejected: br#"fn identity(x: u32) -> result: u32 pure {
  return x;
}

fn value() -> result: u32 pure {
  return identity(x: 1_u32 +wrap 2_u32);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "GRAM-5",
        sentences: &[
            "\n  mechanical_fix: bind each operation with its own preceding `let` and pass the binding as the operand; an expression has one operation over two atoms, with no chaining or parenthesized subexpressions [GRAM-6, GRAM-9]\n",
        ],
        repaired: &[br#"fn identity(x: u32) -> result: u32 pure {
  return x;
}

fn value() -> result: u32 pure {
  let sum = 1_u32 +wrap 2_u32;
  return identity(x: sum);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "syntax-compare-call-argument.wf",
        rejected: br#"fn identity(x: Bool) -> result: Bool pure {
  return x;
}

fn value() -> result: Bool pure {
  return identity(x: 1_u32 < 2_u32);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "GRAM-5",
        sentences: &[
            "\n  mechanical_fix: bind each operation with its own preceding `let` and pass the binding as the operand; an expression has one operation over two atoms, with no chaining or parenthesized subexpressions [GRAM-6, GRAM-9]\n",
        ],
        repaired: &[br#"fn identity(x: Bool) -> result: Bool pure {
  return x;
}

fn value() -> result: Bool pure {
  let ordered = 1_u32 < 2_u32;
  return identity(x: ordered);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "syntax-chained-infix.wf",
        rejected: br#"fn value() -> result: u32 pure {
  return 0_u32 +wrap 1_u32 +wrap 2_u32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "GRAM-9",
        sentences: &[
            "\n  mechanical_fix: bind each operation with its own preceding `let` and pass the binding as the operand; an expression has one operation over two atoms, with no chaining or parenthesized subexpressions [GRAM-6, GRAM-9]\n",
        ],
        repaired: &[br#"fn value() -> result: u32 pure {
  let first = 0_u32 +wrap 1_u32;
  return first +wrap 2_u32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "syntax-parenthesized-infix.wf",
        rejected: br#"fn value() -> result: u32 pure {
  return (1_u32 +wrap 2_u32);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        // DIAG-1 stops at fn_decl's body doc? exit: after `return`, the
        // expected expression start belongs to that exit's continuation.
        rule: "GRAM-2",
        sentences: &[
            "\n  mechanical_fix: bind each operation with its own preceding `let` and pass the binding as the operand; an expression has one operation over two atoms, with no chaining or parenthesized subexpressions [GRAM-6, GRAM-9]\n",
        ],
        repaired: &[br#"fn value() -> result: u32 pure {
  let sum = 1_u32 +wrap 2_u32;
  return sum;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "syntax-array-list.wf",
        rejected: br#"fn value() -> result: u32 pure {
  let values = [1_u8, 2_u8];
  return 0_u32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "GRAM-4",
        sentences: &[
            "\n  mechanical_fix: replace the expression list with `array_filled` or `box_array_filled` for repeated copy elements, or `slots_new` followed by `place_back` for individual elements [OP-13]; for static elements, declare `const values: Array<u8, 2> =[1_u8, 2_u8];` and use `values` [CONST-2]\n",
        ],
        repaired: &[
            br#"fn value() -> result: u32 pure {
  let values = array_filled::<u8, 2>(value: 1_u8);
  return 0_u32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn value() -> result: u32 pure {
  let values = box_array_filled::<u8>(count: 2_u64, value: 1_u8);
  return 0_u32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn value() -> result: u32 pure {
  let values = slots_new::<u8, 2>();
  place_back(window: &values, value: 1_u8);
  place_back(window: &values, value: 2_u8);
  return 0_u32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"const values: Array<u8, 2> =[1_u8, 2_u8];

fn value() -> result: u32 pure {
  let first = values[0_u64];
  return 0_u32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "syntax-typed-array-list.wf",
        rejected: br#"fn value() -> result: u32 pure {
  let values = Array<u8, 2>[1_u8, 2_u8];
  return 0_u32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        // DIAG-1 stops at CONST-1's optional tail exit: `>` matches, but `[` fails its caller continuation.
        rule: "CONST-1",
        sentences: &[
            "\n  mechanical_fix: replace the expression list with `array_filled` or `box_array_filled` for repeated copy elements, or `slots_new` followed by `place_back` for individual elements [OP-13]; for static elements, declare `const values: Array<u8, 2> =[1_u8, 2_u8];` and use `values` [CONST-2]\n",
        ],
        repaired: &[
            br#"fn value() -> result: u32 pure {
  let values = array_filled::<u8, 2>(value: 1_u8);
  return 0_u32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn value() -> result: u32 pure {
  let values = box_array_filled::<u8>(count: 2_u64, value: 1_u8);
  return 0_u32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn value() -> result: u32 pure {
  let values = slots_new::<u8, 2>();
  place_back(window: &values, value: 1_u8);
  place_back(window: &values, value: 2_u8);
  return 0_u32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"const values: Array<u8, 2> =[1_u8, 2_u8];

fn value() -> result: u32 pure {
  let first = values[0_u64];
  return 0_u32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "syntax-value-match-semicolon.wf",
        rejected: br#"fn value() -> result: u32 pure {
  let option = Some<u32>(value: 1_u32);
  let selected = match option {
    Some(value: x) => {
      give x;
    }
    None() => {
      give 0_u32;
    }
  };
  return selected;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "GRAM-4",
        sentences: &[
            "\n  mechanical_fix: remove the `;` after this value match: a `let` initializer `match` ends at its closing `}` [GRAM-4, GRAM-7]\n",
        ],
        repaired: &[br#"fn value() -> result: u32 pure {
  let option = Some<u32>(value: 1_u32);
  let selected = match option {
    Some(value: x) => {
      give x;
    }
    None() => {
      give 0_u32;
    }
  }
  return selected;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "syntax-empty-loop-header.wf",
        rejected: br#"fn value() -> result: u32 pure {
  loop () {
    break;
  }
  return 0_u32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "GRAM-4",
        sentences: &[
            "\n  mechanical_fix: remove the empty parentheses and write `loop {` when the loop has no header invariant [GRAM-4]\n",
        ],
        repaired: &[br#"fn value() -> result: u32 pure {
  loop {
    break;
  }
  return 0_u32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "syntax-unnamed-loop-invariant.wf",
        rejected: br#"fn value() -> result: u32 pure {
  loop (
    invariant 0_u32 <= 0_u32
  ) {
    break;
  }
  return 0_u32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "GRAM-4",
        sentences: &[
            "\n  mechanical_fix: name this loop-header relation as `invariant name: left <= right`, with a fresh lowercase name before the colon [GRAM-4]\n",
        ],
        repaired: &[br#"fn value() -> result: u32 pure {
  loop (
    invariant bound: 0_u32 <= 0_u32
  ) {
    break;
  }
  return 0_u32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "syntax-positional-match-binder.wf",
        rejected: br#"fn value() -> result: u32 pure {
  let option = Some<u32>(value: 1_u32);
  let selected = match option {
    Some(x) => {
      give x;
    }
    None() => {
      give 0_u32;
    }
  }
  return selected;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "GRAM-4",
        sentences: &[
            "\n  mechanical_fix: write named field binders as `Variant(field: binder)` in declared field order, for example `Some(value: x)`; a final `..` covers omitted fields [GRAM-10]\n",
        ],
        repaired: &[
            br#"fn value() -> result: u32 pure {
  let option = Some<u32>(value: 1_u32);
  let selected = match option {
    Some(value: x) => {
      give x;
    }
    None() => {
      give 0_u32;
    }
  }
  return selected;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn value() -> result: u32 pure {
  let option = Some<u32>(value: 1_u32);
  let selected = match option {
    Some(..) => {
      give 0_u32;
    }
    None() => {
      give 0_u32;
    }
  }
  return selected;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "syntax-let-mut.wf",
        rejected: br#"fn value() -> result: u32 pure {
  let mut x = 0_u32;
  set x = 1_u32;
  return x;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "GRAM-4",
        sentences: &[
            "\n  mechanical_fix: remove `mut` and write `let x = value;`; `let` derives the binding's mode and type, and assignment uses `set x = value;` [GRAM-4, TYPE-5, SET-1]\n",
        ],
        repaired: &[br#"fn value() -> result: u32 pure {
  let x = 0_u32;
  set x = 1_u32;
  return x;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "syntax-let-type.wf",
        rejected: br#"fn value() -> result: u32 pure {
  let x: u32 = 0_u32;
  return x;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "GRAM-4",
        sentences: &[
            "\n  mechanical_fix: remove the type annotation and write `let x = value;`; the initializer determines the binding's mode and type [GRAM-4, TYPE-5]\n",
        ],
        repaired: &[br#"fn value() -> result: u32 pure {
  let x = 0_u32;
  return x;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "syntax-borrow-mut.wf",
        rejected: br#"fn value() -> result: u32 pure {
  let x = 0_u32;
  let p = &mut x;
  return p^;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "GRAM-5",
        sentences: &[
            "\n  mechanical_fix: remove `mut` and write `&p`; references have no permission marker, and a callee's effect row declares writes through its reference parameters [GRAM-5, REF-1, EFF-1]\n",
        ],
        repaired: &[br#"fn value() -> result: u32 pure {
  let x = 0_u32;
  let p = &x;
  return p^;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "syntax-top-level-use.wf",
        rejected: br#"use std::process;

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "GRAM-2",
        sentences: &[
            "\n  mechanical_fix: replace the import with an `alias` declaration, for example `alias process = std::process;`; in a module program, an alias names a module listed in this module's graph row [GRAM-2, MOD-4, MOD-5]\n",
        ],
        repaired: &[br#"alias process = std::process;

fn main() -> status: process::ExitStatus pure {
  return process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "syntax-boolean-and.wf",
        rejected: br#"fn value() -> result: Bool pure {
  let a = True();
  let b = False();
  return a && b;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "GRAM-5",
        sentences: &[
            "\n  mechanical_fix: write `band(a, b)` for boolean conjunction and `bor(a, b)` for boolean disjunction; both operations are eager, so bind computed operands first with `let` in a body, or with preceding `define` bindings in a contract definition initializer when [FN-8] admits those operations [OP-1, GRAM-5, GRAM-9]\n",
        ],
        repaired: &[br#"fn value() -> result: Bool pure {
  let a = True();
  let b = False();
  return band(a, b);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "syntax-chained-infix-contract.wf",
        rejected: br#"fn bounded(x: u32) -> result: u32 pure contract {
  define total = 0_u32 +wrap 1_u32 +wrap 2_u32;
  requires x <= total;
} {
  return x;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "GRAM-9",
        sentences: &[
            "\n  mechanical_fix: if the operations are admitted by [FN-8], split this definition initializer into preceding definitions with fresh binders and pass each binder as the operand; each initializer has one operation over two atoms [GRAM-2, GRAM-6, GRAM-9]\n",
        ],
        repaired: &[br#"fn bounded(x: u32) -> result: u32 pure contract {
  define subtotal = 0_u32 +wrap 1_u32;
  define total = subtotal +wrap 2_u32;
  requires x <= total;
} {
  return x;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "range-boolean-and.wf",
        rejected: br#"struct Node {
  live: Bool;
  open: Bool;
  slot: u64;
}

fn inspect(nodes: &Slots<Node, 1>) -> result: unit pure contract {
  requires nodes^.len == 1_u64;
  requires forall settled(k in 0_u64..nodes^.len) when nodes^[k].live is True && nodes^[k].open is True: nodes^[k].slot == 0_u64;
} {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "GRAM-5",
        sentences: &[
            "\n  mechanical_fix: a range clause admits no call, so `band` is not available there; join Bool tests with commas, each a variant relation testing one Bool datum, as in `when nodes^[k] is True, flags^[k] is True:`, and the same commas join conclusions [RANGE-1, GRAM-5]\n",
        ],
        repaired: &[br#"struct Node {
  live: Bool;
  open: Bool;
  slot: u64;
}

fn inspect(nodes: &Slots<Node, 1>) -> result: unit pure contract {
  requires nodes^.len == 1_u64;
  requires forall settled(k in 0_u64..nodes^.len) when nodes^[k].live is True, nodes^[k].open is True: nodes^[k].slot == 0_u64;
} {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        // [GRAM-3] a Rust array type in a type argument: the writer's
        // `Box<[u8; 256]>` is `Box<Array<u8, 256>>`.
        name: "syntax-rust-array-type-box.wf",
        rejected: br#"struct Holder {
  data: Box<[u8; 256]>;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "GRAM-3",
        sentences: &[
            "\n  mechanical_fix: there is no `[T; N]` type: a constant-capacity array is written `Array<T, N>`, so `[u8; 256]` is `Array<u8, 256>` and `Box<[u8; 256]>` is `Box<Array<u8, 256>>`; omit the capacity for a runtime-capacity array held in a `Box`, as in `Box<Array<u8>>`, and write a parameter's type `&[u8]` for a reference to a run of any length [GRAM-3, TYPE-9]\n",
        ],
        repaired: &[br#"struct Holder {
  data: Box<Array<u8, 256>>;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        // [GRAM-2] `&[u8; 4]` opens an admitted `&[T]` and then meets the
        // `;` of a Rust array type; the run reference and the array nominal
        // both fit the writer's meaning.
        name: "syntax-rust-array-type-reference.wf",
        rejected: br#"fn total(data: &[u8; 4]) -> sum: u64 pure {
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "GRAM-2",
        sentences: &[
            "\n  mechanical_fix: there is no `[T; N]` type: a constant-capacity array is written `Array<T, N>`, so `[u8; 256]` is `Array<u8, 256>` and `Box<[u8; 256]>` is `Box<Array<u8, 256>>`; omit the capacity for a runtime-capacity array held in a `Box`, as in `Box<Array<u8>>`, and write a parameter's type `&[u8]` for a reference to a run of any length [GRAM-3, TYPE-9]\n",
        ],
        repaired: &[
            br#"fn total(data: &[u8]) -> sum: u64 pure {
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn total(data: &Array<u8, 4>) -> sum: u64 pure {
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        // [GIVE-1] `give if` is refused as a lexical-looking FORM-3 fault;
        // the repair names the two forms that deliver from a conditional.
        name: "syntax-give-conditional.wf",
        rejected: br#"fn pick(flag: Bool, other: Bool) -> result: u64 pure {
  let value = if flag {
    give if other { 1_u64 } else { 2_u64 };
  } else {
    give 0_u64;
  }
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FORM-3",
        sentences: &[
            "\n  mechanical_fix: `give` takes an expression, and `if` and `match` are not expressions: a conditional value is only a `let` initializer [GRAM-4, GRAM-7, GIVE-1]; to deliver from branches, write the `if` or `match` here as a statement whose every branch ends in its own `give`, as in `if c { give a; } else { give b; }`, or bind the conditional first with `let x = if c { give a; } else { give b; }` (no `;` after its closing `}`) and write `give x;`\n",
        ],
        repaired: &[
            br#"fn pick(flag: Bool, other: Bool) -> result: u64 pure {
  let value = if flag {
    if other {
      give 1_u64;
    } else {
      give 2_u64;
    }
  } else {
    give 0_u64;
  }
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn pick(flag: Bool, other: Bool) -> result: u64 pure {
  let value = if flag {
    let inner = if other {
      give 1_u64;
    } else {
      give 2_u64;
    }
    give inner;
  } else {
    give 0_u64;
  }
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        // [GIVE-1] a conditional where `return` takes its operand has no
        // statement form to offer; the repair binds it with `let` first.
        name: "syntax-return-conditional.wf",
        rejected: br#"fn pick(flag: Bool) -> result: u64 pure {
  return if flag { 1_u64 } else { 2_u64 };
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FORM-3",
        sentences: &[
            "\n  mechanical_fix: an `if` or `match` is not an expression: a conditional value is only a `let` initializer [GRAM-4, GRAM-7, GIVE-1]; bind it first, as in `let x = if c { give a; } else { give b; }` (no `;` after its closing `}`), and write `x` where the conditional stood\n",
        ],
        repaired: &[br#"fn pick(flag: Bool) -> result: u64 pure {
  let value = if flag {
    give 1_u64;
  } else {
    give 2_u64;
  }
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        // [FORM-2] four independent layout faults: the rejection names the
        // first, and the repair lists all four, whose making is the canonical
        // source.
        name: "layout-four-differences.wf",
        rejected: br#"fn helper() -> result: u64 pure {
  return 1_u64;
}


fn main() -> status: std::process::ExitStatus pure {
  let a =  1_u64;
  let b = a +wrap
    2_u64;

  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FORM-2",
        sentences: &[
            "\n  mechanical_fix: make all 4 changes below to reach the canonical layout, then compile again; each gives the bytes FORM-2 requires between two terminals [FORM-2]: line 3 between `}` and `fn`: write \"\\n\\n\" instead of \"\\n\\n\\n\"; line 7 between `=` and `1_u64`: write \" \" instead of \"  \"; line 8 between `+wrap` and `2_u64`: write \" \" instead of \"\\n    \"; line 9 between `;` and `return`: write \"\\n  \" instead of \"\\n\\n  \"\n",
        ],
        repaired: &[br#"fn helper() -> result: u64 pure {
  return 1_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let a = 1_u64;
  let b = a +wrap 2_u64;
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
];
