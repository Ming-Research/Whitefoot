//! The repairs [DIAG-1] the compiler prints, pinned with the programs they
//! produce.
//!
//! A repair is a sentence with a promise in it, so each pair pins more than
//! a probe in `driver::pinned_sentences` does: besides the rejected source
//! and the repair it carries, one program for each alternative the pair
//! carries out, which must be accepted with the repaired construct live.
//! Adding or rewording a repair means adding or updating a pair here, or in
//! `call_separations` for the [EFF-5] call-separation family; the repairs
//! still printed without one are listed in `docs/todo.md`.

use super::{CompilationFailureKind, CompilerLimits, compile};
use crate::SourceInput;

mod call_separations;

/// One repair [DIAG-1], pinned with the programs it produces: a rejected
/// source, the rule and the exact repair its rejection carries, and one
/// source for each alternative the pair carries out.
///
/// DIAG-1 asks two things of a repair's alternatives: carried out as the
/// repair directs, the rejected judgment succeeds where its construct runs,
/// in a state that is not contradictory; and nothing it writes is text a rule
/// rejects. A sentence alone can drift from both, as the printed repairs did
/// before v0.73. A pair makes the repair checkable: each repaired source must
/// be accepted, and no judgment in a function it declares may succeed only
/// because the state it is asked in is contradictory. A guard around a
/// refuted goal, for one, compiles and is caught by the second condition.
///
/// A green run shows that each pinned alternative, applied to its own probe,
/// works. It does not show that every repair works in every program.
struct RepairPair {
    /// The compiled unit's name, which also names the case under test.
    name: &'static str,
    /// The rejected source.
    rejected: &'static [u8],
    /// The numbered rule [DIAG-1] must select.
    rule: &'static str,
    /// Exact substrings of the rendered rejection: the disposition where the
    /// rule carries one, and the repair.
    sentences: &'static [&'static str],
    /// One program for each alternative the pair carries out.
    repaired: &'static [&'static [u8]],
}

const REPAIRS: &[RepairPair] = &[
    // -------------------------------------------------------------------
    // [FN-8] an ordinary call's requirement.
    // -------------------------------------------------------------------
    RepairPair {
        name: "call-requirement-refuted.wf",
        rejected: br#"fn small(x: u64) -> result: u64 pure contract {
  requires x < 10_u64;
} {
  return x;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = small(x: 20_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FN-8",
        sentences: &[
            "\n  disposition: Refuted\n",
            "\n  mechanical_fix: `20_u64 < 10_u64` is false for the values that reach this call, so no fact can establish it here: pass arguments that satisfy it, or change the statements or requirements that fix those values\n",
        ],
        repaired: &[br#"fn small(x: u64) -> result: u64 pure contract {
  requires x < 10_u64;
} {
  return x;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = small(x: 5_u64);
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "call-requirement-over-parameters.wf",
        rejected: br#"fn small(x: u64) -> result: u64 pure contract {
  requires x < 10_u64;
} {
  return x;
}

fn caller(y: u64) -> result: u64 pure {
  let r = small(x: y);
  return r;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = caller(y: 3_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FN-8",
        sentences: &[
            "\n  disposition: Unproved\n",
            "\n  mechanical_fix: add `requires y < 10_u64;` to the `contract` of `caller`, which each caller then establishes; or guard the call with `if y < 10_u64` where skipping it is the intended behavior\n",
        ],
        repaired: &[
            br#"fn small(x: u64) -> result: u64 pure contract {
  requires x < 10_u64;
} {
  return x;
}

fn caller(y: u64) -> result: u64 pure contract {
  requires y < 10_u64;
} {
  let r = small(x: y);
  return r;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = caller(y: 3_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn small(x: u64) -> result: u64 pure contract {
  requires x < 10_u64;
} {
  return x;
}

fn caller(y: u64) -> result: u64 pure {
  if y < 10_u64 {
    let r = small(x: y);
    return r;
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = caller(y: 3_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "call-requirement-over-a-computed-value.wf",
        rejected: br#"fn small(x: u64) -> result: u64 pure contract {
  requires x < 10_u64;
} {
  return x;
}

fn clamp(y: u64) -> result: u64 pure {
  if y < 10_u64 {
    return y;
  }
  return 9_u64;
}

fn caller(y: u64) -> result: u64 pure {
  let z = clamp(y: y);
  let r = small(x: z);
  return r;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = caller(y: 3_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FN-8",
        sentences: &[
            "\n  disposition: Unproved\n",
            "\n  mechanical_fix: `z < 10_u64` is not proved before this call: when facts that reach the call imply it, prove it with an `invariant` whose `use` steps name them (a loop's header `invariant` for a value the loop computes); when the callee whose result it reads can prove the bound, state it in that callee's `ensures`; or guard the call with `if z < 10_u64` where skipping it is the intended behavior\n",
        ],
        repaired: &[
            br#"fn small(x: u64) -> result: u64 pure contract {
  requires x < 10_u64;
} {
  return x;
}

fn clamp(y: u64) -> result: u64 pure contract {
  ensures result < 10_u64;
} {
  if y < 10_u64 {
    return y;
  }
  return 9_u64;
}

fn caller(y: u64) -> result: u64 pure {
  let z = clamp(y: y);
  let r = small(x: z);
  return r;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = caller(y: 3_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn small(x: u64) -> result: u64 pure contract {
  requires x < 10_u64;
} {
  return x;
}

fn clamp(y: u64) -> result: u64 pure {
  if y < 10_u64 {
    return y;
  }
  return 9_u64;
}

fn caller(y: u64) -> result: u64 pure {
  let z = clamp(y: y);
  if z < 10_u64 {
    let r = small(x: z);
    return r;
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = caller(y: 3_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        // A field of an element read through a range reference is no admitted
        // value [ENT-3], so the goal keeps the argument's occurrence-local
        // value, which DIAG-1 spells for the payload.
        name: "call-requirement-over-an-argument-evaluated-in-the-call.wf",
        rejected: br#"struct Stats {
  count: u64;
}

fn small(x: u64) -> result: u64 pure contract {
  requires x < 10_u64;
} {
  return x;
}

fn first(view: &[Stats]) -> result: u64 reads(view) contract {
  requires 1_u64 <= deref(view).len;
} {
  let r = small(x: deref(view)[0_u64].count);
  return r;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FN-8",
        sentences: &[
            "\n  instantiated_goal: argument #0 pre-transfer value < 10_u64\n",
            "\n  mechanical_fix: argument #0 is evaluated inside the call, where no fact names its value: bind it with one preceding `let`, establish the requirement over that binding, and pass the binding, borrowing it when the parameter is a reference\n",
        ],
        repaired: &[br#"struct Stats {
  count: u64;
}

fn small(x: u64) -> result: u64 pure contract {
  requires x < 10_u64;
} {
  return x;
}

fn first(view: &[Stats]) -> result: u64 reads(view) contract {
  requires 1_u64 <= deref(view).len;
} {
  let c = deref(view)[0_u64].count;
  if c < 10_u64 {
    let r = small(x: c);
    return r;
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "call-requirement-over-a-range-formed-at-the-call.wf",
        rejected: br#"fn need(v: &[u64]) -> result: u64 pure contract {
  requires 2_u64 <= deref(v).len;
} {
  return 0_u64;
}

fn caller(k: u64) -> result: u64 pure {
  let values = array_filled::<u64, 4>(value: 0_u64);
  if k <= 4_u64 {
    let r = need(v: &values[0_u64..k]);
    return r;
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = caller(k: 3_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FN-8",
        sentences: &[
            "\n  disposition: Unproved\n",
            "` reads a value no fact can name until a `let` binds it: bind that value with one preceding `let`, use the binding in the call, and establish the relation over the binding\n",
        ],
        repaired: &[br#"fn need(v: &[u64]) -> result: u64 pure contract {
  requires 2_u64 <= deref(v).len;
} {
  return 0_u64;
}

fn caller(k: u64) -> result: u64 pure {
  let values = array_filled::<u64, 4>(value: 0_u64);
  if k <= 4_u64 {
    let part = &values[0_u64..k];
    if 2_u64 <= deref(part).len {
      let r = need(v: part);
      return r;
    }
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = caller(k: 3_u64);
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    // -------------------------------------------------------------------
    // [FN-9] a normal-result relation at one selected return.
    // -------------------------------------------------------------------
    RepairPair {
        name: "postcondition-refuted.wf",
        rejected: br#"fn f() -> result: u64 pure contract {
  ensures result < 10_u64;
} {
  return 20_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = f();
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FN-9",
        sentences: &[
            "\n  disposition: Refuted\n",
            "\n  mechanical_fix: the value this `return` delivers makes the postcondition false: return a value that satisfies it, state a postcondition this return satisfies, or change the requirements that fix the returned value\n",
        ],
        repaired: &[
            br#"fn f() -> result: u64 pure contract {
  ensures result < 10_u64;
} {
  return 5_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = f();
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn f() -> result: u64 pure contract {
  ensures result <= 20_u64;
} {
  return 20_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = f();
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "postcondition-unproved.wf",
        rejected: br#"fn f(x: u64) -> result: u64 pure contract {
  ensures result < 10_u64;
} {
  return x;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = f(x: 3_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FN-9",
        sentences: &[
            "\n  disposition: Unproved\n",
            // The returned value is a parameter and no call returns anything
            // here, so no callee's `ensures` is offered.
            "\n  mechanical_fix: the postcondition is not proved where this `return` delivers its value: add a `requires` over the parameters the value is computed from, prove the bound before the return with an `invariant` whose `use` steps name the facts it follows from, or state a postcondition the body proves\n",
        ],
        repaired: &[br#"fn f(x: u64) -> result: u64 pure contract {
  requires x < 10_u64;
  ensures result < 10_u64;
} {
  return x;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = f(x: 3_u64);
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        // The returned value is a call's result, so the callee's `ensures`
        // is the route that bounds it.
        name: "postcondition-over-a-call-result.wf",
        rejected: br#"fn clamp(y: u64) -> result: u64 pure {
  if y < 10_u64 {
    return y;
  }
  return 9_u64;
}

fn f(x: u64) -> result: u64 pure contract {
  ensures result < 10_u64;
} {
  let z = clamp(y: x);
  return z;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = f(x: 3_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FN-9",
        sentences: &[
            "\n  disposition: Unproved\n",
            "\n  mechanical_fix: the postcondition is not proved where this `return` delivers its value: add a `requires` over the parameters the value is computed from, prove the bound before the return with an `invariant` whose `use` steps name the facts it follows from, state it in the `ensures` of the callee whose result the value reads when that callee can prove it, or state a postcondition the body proves\n",
        ],
        repaired: &[br#"fn clamp(y: u64) -> result: u64 pure contract {
  ensures result < 10_u64;
} {
  if y < 10_u64 {
    return y;
  }
  return 9_u64;
}

fn f(x: u64) -> result: u64 pure contract {
  ensures result < 10_u64;
} {
  let z = clamp(y: x);
  return z;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = f(x: 3_u64);
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "postcondition-with-no-selected-return.wf",
        rejected: br#"fn only_error(value: i32) -> out: Result<i32, i32> pure contract {
  ensures when Ok(value: payload): payload == value;
} {
  return Err<i32, i32>(error: value);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FN-9",
        sentences: &[
            "\n  residual: no selected normal exit\n  mechanical_fix: no `return` of this function delivers a value this clause's route selects: return such a value on some path, or delete the clause\n",
        ],
        repaired: &[
            br#"fn only_error(value: i32) -> out: Result<i32, i32> pure contract {
  ensures when Ok(value: payload): payload == value;
} {
  return Ok<i32, i32>(value: value);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn only_error(value: i32) -> out: Result<i32, i32> pure {
  return Err<i32, i32>(error: value);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    // -------------------------------------------------------------------
    // [OP-2] an exact integer operation's `.defined` domain.
    // -------------------------------------------------------------------
    RepairPair {
        name: "integer-domain-refuted.wf",
        rejected: br#"fn main() -> status: std::process::ExitStatus pure {
  let y = 255_u8 + 1_u8;
  return std::process::exit_status(code: y);
}
"#,
        rule: "OP-2",
        sentences: &[
            "\n  disposition: Refuted\n",
            "\n  mechanical_fix: the operands that reach this operation make `255_u8 +defined 1_u8` false, so the exact operation cannot execute here: change the operands or their type, or write the `+wrap`, `+checked` or `+sat` form for the result the program intends\n",
        ],
        repaired: &[
            br#"fn main() -> status: std::process::ExitStatus pure {
  let y = 254_u8 + 1_u8;
  return std::process::exit_status(code: y);
}
"#,
            br#"fn main() -> status: std::process::ExitStatus pure {
  let y = 255_u8 +wrap 1_u8;
  return std::process::exit_status(code: y);
}
"#,
        ],
    },
    RepairPair {
        name: "integer-domain-over-parameters.wf",
        rejected: br#"fn bump(x: u8) -> result: u8 pure {
  let y = x + 1_u8;
  return y;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = bump(x: 7_u8);
  return std::process::exit_status(code: r);
}
"#,
        rule: "OP-2",
        sentences: &[
            "\n  disposition: Unproved\n",
            "\n  mechanical_fix: add `requires x +defined 1_u8;` to the `contract` of `bump`, which each caller then establishes; or guard the operation with `if x +defined 1_u8` where skipping it is the intended behavior; or write the `+wrap`, `+checked` or `+sat` form\n",
        ],
        repaired: &[
            br#"fn bump(x: u8) -> result: u8 pure contract {
  requires x +defined 1_u8;
} {
  let y = x + 1_u8;
  return y;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = bump(x: 7_u8);
  return std::process::exit_status(code: r);
}
"#,
            br#"fn bump(x: u8) -> result: u8 pure {
  if x +defined 1_u8 {
    let y = x + 1_u8;
    return y;
  }
  return 0_u8;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = bump(x: 7_u8);
  return std::process::exit_status(code: r);
}
"#,
            br#"fn bump(x: u8) -> result: u8 pure {
  let y = x +wrap 1_u8;
  return y;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = bump(x: 7_u8);
  return std::process::exit_status(code: r);
}
"#,
        ],
    },
    RepairPair {
        // [ENT-5] a write kills facts on its own path only: the arm that
        // writes `deref(p)` comes first, and the goal in its sibling still
        // reads the entry value, which a requirement describes.
        name: "integer-domain-in-the-arm-after-a-sibling-write.wf",
        rejected: br#"fn bump(p: &u64, flag: Bool) -> result: u64 writes(p) {
  if flag {
    set deref(p) = 0_u64;
    return 0_u64;
  } else {
    let v = deref(p) + 1_u64;
    return v;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-2",
        sentences: &[
            "\n  disposition: Unproved\n",
            "\n  mechanical_fix: add `requires deref(p) +defined 1_u64;` to the `contract` of `bump`, which each caller then establishes; or guard the operation with `if deref(p) +defined 1_u64` where skipping it is the intended behavior, adding to the effect row any read that condition makes which the row does not yet declare; or write the `+wrap`, `+checked` or `+sat` form\n",
        ],
        repaired: &[br#"fn bump(p: &u64, flag: Bool) -> result: u64 writes(p) contract {
  requires deref(p) +defined 1_u64;
} {
  if flag {
    set deref(p) = 0_u64;
    return 0_u64;
  } else {
    let v = deref(p) + 1_u64;
    return v;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        // The same program with its arms in the other order selects the same
        // repair: which arm the walk visits first changes nothing.
        name: "integer-domain-in-the-arm-before-a-sibling-write.wf",
        rejected: br#"fn bump(p: &u64, flag: Bool) -> result: u64 writes(p) {
  if flag {
    let v = deref(p) + 1_u64;
    return v;
  } else {
    set deref(p) = 0_u64;
    return 0_u64;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-2",
        sentences: &[
            "\n  disposition: Unproved\n",
            "\n  mechanical_fix: add `requires deref(p) +defined 1_u64;` to the `contract` of `bump`, which each caller then establishes; or guard the operation with `if deref(p) +defined 1_u64` where skipping it is the intended behavior, adding to the effect row any read that condition makes which the row does not yet declare; or write the `+wrap`, `+checked` or `+sat` form\n",
        ],
        repaired: &[br#"fn bump(p: &u64, flag: Bool) -> result: u64 writes(p) contract {
  requires deref(p) +defined 1_u64;
} {
  if flag {
    let v = deref(p) + 1_u64;
    return v;
  } else {
    set deref(p) = 0_u64;
    return 0_u64;
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "integer-domain-over-a-loop-value.wf",
        rejected: br#"fn total() -> result: u64 pure {
  let sum = 0_u64;
  for (i in 0_u64..4_u64) {
    set sum = sum + i;
  }
  return sum;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = total();
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-2",
        sentences: &[
            "\n  disposition: Unproved\n",
            "\n  mechanical_fix: `sum +defined i` is not proved here: when facts that reach the operation imply it, prove it with an `invariant` whose `use` steps name them (a loop's header `invariant` for a value the loop computes); or guard the operation with `if sum +defined i` where skipping it is the intended behavior; or write the `+wrap`, `+checked` or `+sat` form\n",
        ],
        repaired: &[
            br#"fn total() -> result: u64 pure {
  let sum = 0_u64;
  for (
    i in 0_u64..4_u64,
    invariant bounded: sum <= 3_u64 * i
  ) {
    set sum = sum + i;
  }
  return sum;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = total();
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn total() -> result: u64 pure {
  let sum = 0_u64;
  for (i in 0_u64..4_u64) {
    if sum +defined i {
      set sum = sum + i;
    }
  }
  return sum;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = total();
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn total() -> result: u64 pure {
  let sum = 0_u64;
  for (i in 0_u64..4_u64) {
    set sum = sum +wrap i;
  }
  return sum;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = total();
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        // An element read is no term [ENT-2], but it is part of the goal's
        // identity, which a condition naming the same expression establishes
        // [ENT-3].
        name: "integer-domain-over-an-element.wf",
        rejected: br#"fn bump(values: &Array<u8, 2>) -> result: u8 reads(values) {
  let y = deref(values)[0_u64] + 1_u8;
  return y;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-2",
        sentences: &[
            "\n  disposition: Unproved\n",
            "\n  mechanical_fix: `deref(values)[0_u64] +defined 1_u8` is not proved here: when facts that reach the operation imply it, prove it with an `invariant` whose `use` steps name them (a loop's header `invariant` for a value the loop computes); or guard the operation with `if deref(values)[0_u64] +defined 1_u8` where skipping it is the intended behavior, adding to the effect row any read that condition makes which the row does not yet declare; or write the `+wrap`, `+checked` or `+sat` form\n",
        ],
        repaired: &[br#"fn bump(values: &Array<u8, 2>) -> result: u8 reads(values) {
  if deref(values)[0_u64] +defined 1_u8 {
    let y = deref(values)[0_u64] + 1_u8;
    return y;
  }
  return 0_u8;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    // -------------------------------------------------------------------
    // [OP-6] a bare conversion's domain.
    // -------------------------------------------------------------------
    RepairPair {
        name: "conversion-domain-refuted.wf",
        rejected: br#"fn main() -> status: std::process::ExitStatus pure {
  let b = cvt::<u32, u8>(256_u32);
  return std::process::exit_status(code: b);
}
"#,
        rule: "OP-6",
        sentences: &[
            "\n  disposition: Refuted\n",
            "\n  mechanical_fix: the value that reaches this conversion is outside `u8`: convert a value `u8` holds, choose a destination type that holds this one, or use `cvt.checked::<u32, u8>` and handle its `Err`\n",
        ],
        repaired: &[
            br#"fn main() -> status: std::process::ExitStatus pure {
  let b = cvt::<u32, u8>(255_u32);
  return std::process::exit_status(code: b);
}
"#,
            br#"fn main() -> status: std::process::ExitStatus pure {
  let b = cvt.checked::<u32, u8>(256_u32);
  match b {
    Ok(value: v) => {
      return std::process::exit_status(code: v);
    }
    Err(error: e) => {
      return std::process::exit_status(code: 1_u8);
    }
  }
}
"#,
        ],
    },
    RepairPair {
        name: "conversion-domain-over-parameters.wf",
        rejected: br#"fn narrow(x: u32) -> result: u8 pure {
  return cvt::<u32, u8>(x);
}

fn main() -> status: std::process::ExitStatus pure {
  let r = narrow(x: 7_u32);
  return std::process::exit_status(code: r);
}
"#,
        rule: "OP-6",
        sentences: &[
            "\n  disposition: Unproved\n",
            "\n  mechanical_fix: add `requires cvt.defined::<u32, u8>(x);` to the `contract` of `narrow`, which each caller then establishes; or guard the conversion with `if cvt.defined::<u32, u8>(x)` where skipping it is the intended behavior; or use `cvt.checked::<u32, u8>` and handle its `Err`\n",
        ],
        repaired: &[
            br#"fn narrow(x: u32) -> result: u8 pure contract {
  requires cvt.defined::<u32, u8>(x);
} {
  return cvt::<u32, u8>(x);
}

fn main() -> status: std::process::ExitStatus pure {
  let r = narrow(x: 7_u32);
  return std::process::exit_status(code: r);
}
"#,
            br#"fn narrow(x: u32) -> result: u8 pure {
  if cvt.defined::<u32, u8>(x) {
    return cvt::<u32, u8>(x);
  }
  return 0_u8;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = narrow(x: 7_u32);
  return std::process::exit_status(code: r);
}
"#,
            br#"fn narrow(x: u32) -> result: u8 pure {
  let b = cvt.checked::<u32, u8>(x);
  match b {
    Ok(value: v) => {
      return v;
    }
    Err(error: e) => {
      return 0_u8;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  let r = narrow(x: 7_u32);
  return std::process::exit_status(code: r);
}
"#,
        ],
    },
    RepairPair {
        name: "conversion-domain-over-a-computed-integer.wf",
        rejected: br#"fn widen(x: u32) -> result: u32 pure {
  return x;
}

fn narrow(x: u32) -> result: u8 pure {
  let v = widen(x: x);
  return cvt::<u32, u8>(v);
}

fn main() -> status: std::process::ExitStatus pure {
  let r = narrow(x: 7_u32);
  return std::process::exit_status(code: r);
}
"#,
        rule: "OP-6",
        sentences: &[
            "\n  disposition: Unproved\n",
            "\n  mechanical_fix: `cvt.defined::<u32, u8>(v)` is not proved here: when facts that reach the conversion imply it, prove it with an `invariant` whose `use` steps name them (a loop's header `invariant` for a value the loop computes); when the callee whose result it reads can prove the bound, state it in that callee's `ensures`; or guard the conversion with `if cvt.defined::<u32, u8>(v)` where skipping it is the intended behavior; or use `cvt.checked::<u32, u8>` and handle its `Err`\n",
        ],
        repaired: &[br#"fn widen(x: u32) -> result: u32 pure {
  return x;
}

fn narrow(x: u32) -> result: u8 pure {
  let v = widen(x: x);
  if cvt.defined::<u32, u8>(v) {
    return cvt::<u32, u8>(v);
  }
  return 0_u8;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = narrow(x: 7_u32);
  return std::process::exit_status(code: r);
}
"#],
    },
    RepairPair {
        name: "conversion-domain-over-a-computed-float.wf",
        rejected: br#"fn pick(a: f64) -> result: f64 pure {
  return a;
}

fn round(a: f64) -> result: i32 pure {
  let v = pick(a: a);
  return cvt::<f64, i32>(v);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-6",
        sentences: &[
            "\n  disposition: Unproved\n",
            "\n  mechanical_fix: `cvt.defined::<f64, i32>(v)` is not proved here, and no fact bounds a float operand: guard the conversion with `if cvt.defined::<f64, i32>(v)` where skipping it is the intended behavior, or use `cvt.checked::<f64, i32>` and handle its `Err`\n",
        ],
        repaired: &[br#"fn pick(a: f64) -> result: f64 pure {
  return a;
}

fn round(a: f64) -> result: i32 pure {
  let v = pick(a: a);
  if cvt.defined::<f64, i32>(v) {
    return cvt::<f64, i32>(v);
  }
  return 0_i32;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    // -------------------------------------------------------------------
    // [OP-4] a subscript's bound.
    // -------------------------------------------------------------------
    RepairPair {
        name: "bounds-refuted.wf",
        rejected: br#"fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u8, 4>(value: 0_u8);
  let v = values[5_u64];
  return std::process::exit_status(code: v);
}
"#,
        rule: "OP-4",
        sentences: &[
            "\n  disposition: Refuted\n",
            "\n  mechanical_fix: `5_u64 < values.len` is false where this access executes: index within the storage, or give the storage a length that holds this index\n",
        ],
        repaired: &[
            br#"fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u8, 4>(value: 0_u8);
  let v = values[3_u64];
  return std::process::exit_status(code: v);
}
"#,
            br#"fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u8, 6>(value: 0_u8);
  let v = values[5_u64];
  return std::process::exit_status(code: v);
}
"#,
        ],
    },
    RepairPair {
        // An offset that grows with the storage, such as its own length, is
        // out of range at every length, so no longer storage is offered: the
        // statements that fix the offset are what the repair changes.
        name: "bounds-refuted-at-an-offset-that-is-no-constant.wf",
        rejected: br#"fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u8, 4>(value: 0_u8);
  let k = values.len;
  let v = values[k];
  return std::process::exit_status(code: v);
}
"#,
        rule: "OP-4",
        sentences: &[
            "\n  disposition: Refuted\n",
            "\n  mechanical_fix: `k < values.len` is false where this access executes: index within the storage, or change the statements or requirements that fix the index\n",
        ],
        repaired: &[br#"fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u8, 4>(value: 0_u8);
  let k = values.len - 1_u64;
  let v = values[k];
  return std::process::exit_status(code: v);
}
"#],
    },
    RepairPair {
        name: "bounds-over-parameters.wf",
        rejected: br#"fn get(b: &[u8], i: u64) -> result: u8 reads(b) {
  return deref(b)[i];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-4",
        sentences: &[
            "\n  disposition: Unproved\n",
            "\n  mechanical_fix: add `requires i < deref(b).len;` to the `contract` of `get`, which each caller then establishes; or guard the access with `if i < deref(b).len` where skipping it is the intended behavior, adding to the effect row any read that condition makes which the row does not yet declare\n",
        ],
        repaired: &[
            br#"fn get(b: &[u8], i: u64) -> result: u8 reads(b) contract {
  requires i < deref(b).len;
} {
  return deref(b)[i];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn get(b: &[u8], i: u64) -> result: u8 reads(b) {
  if i < deref(b).len {
    return deref(b)[i];
  }
  return 0_u8;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "bounds-over-a-computed-offset.wf",
        rejected: br#"fn widen(x: u64) -> result: u64 pure {
  return x;
}

fn get(b: &[u8], i: u64) -> result: u8 reads(b) {
  let k = widen(x: i);
  return deref(b)[k];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-4",
        sentences: &[
            "\n  disposition: Unproved\n",
            "\n  mechanical_fix: `k < deref(b).len` is not proved here: when facts that reach the access imply it, prove it with an `invariant` whose `use` steps name them (a loop's header `invariant` for a value the loop computes); when the callee whose result it reads can prove the bound, state it in that callee's `ensures`; or guard the access with `if k < deref(b).len` where skipping it is the intended behavior, adding to the effect row any read that condition makes which the row does not yet declare\n",
        ],
        repaired: &[br#"fn widen(x: u64) -> result: u64 pure {
  return x;
}

fn get(b: &[u8], i: u64) -> result: u8 reads(b) {
  let k = widen(x: i);
  if k < deref(b).len {
    return deref(b)[k];
  }
  return 0_u8;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        // An offset that is itself an element is no term [ENT-2], and a
        // bound names terms alone, so no condition over it establishes one.
        name: "bounds-over-an-element-offset.wf",
        rejected: br#"fn pick(order: &[u64], lens: &[u8], j: u64) -> result: u8 reads(order), reads(lens) contract {
  requires j < deref(order).len;
} {
  return deref(lens)[deref(order)[j]];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-4",
        sentences: &[
            "\n  disposition: Unproved\n",
            "\n  mechanical_fix: `deref(order)[j] < deref(lens).len` reads a value no fact can name until a `let` binds it: bind that value with one preceding `let`, use the binding in the access, and establish the relation over the binding\n",
        ],
        repaired: &[br#"fn pick(order: &[u64], lens: &[u8], j: u64) -> result: u8 reads(order), reads(lens) contract {
  requires j < deref(order).len;
} {
  let k = deref(order)[j];
  if k < deref(lens).len {
    return deref(lens)[k];
  }
  return 0_u8;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        // [ENT-2, FN-8] a requirement forms its places at body entry, in the
        // state the requirements written before it build, and evaluates
        // nothing: the bound comes from an earlier requirement, and no guard
        // can skip a clause.
        name: "bounds-of-a-place-a-requirement-forms.wf",
        rejected: br#"fn pick(rows: &Slots<Slots<u8, 8>, 4>, i: u64, k: u64) -> value: u8 reads(rows) contract {
  requires k < deref(rows)[i].len;
} {
  return deref(rows)[i][k];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-4",
        sentences: &[
            "\n  residual: i < deref(rows).len\n  disposition: Unproved\n",
            "\n  mechanical_fix: add `requires i < deref(rows).len;` to the `contract` of `pick` ahead of the requirement that forms this place, which each caller then establishes\n",
        ],
        repaired: &[br#"fn pick(rows: &Slots<Slots<u8, 8>, 4>, i: u64, k: u64) -> value: u8 reads(rows) contract {
  requires i < deref(rows).len;
  requires k < deref(rows)[i].len;
} {
  return deref(rows)[i][k];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        // The earlier requirement makes `i` the length, so the place the
        // second requirement forms is refuted there: the requirements before
        // it are what fix the index.
        name: "bounds-refuted-in-a-place-a-requirement-forms.wf",
        rejected: br#"fn pick(rows: &Slots<Slots<u8, 8>, 4>, i: u64, k: u64) -> value: u8 reads(rows) contract {
  requires i == deref(rows).len;
  requires k < deref(rows)[i].len;
} {
  return deref(rows)[i][k];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-4",
        sentences: &[
            "\n  residual: i < deref(rows).len\n  disposition: Refuted\n",
            "\n  mechanical_fix: `i < deref(rows).len` is false where this place is formed: index within the storage, or change the requirements before this one that fix the index\n",
        ],
        repaired: &[br#"fn pick(rows: &Slots<Slots<u8, 8>, 4>, i: u64, k: u64) -> value: u8 reads(rows) contract {
  requires i < deref(rows).len;
  requires k < deref(rows)[i].len;
} {
  return deref(rows)[i][k];
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    // -------------------------------------------------------------------
    // [OP-9] an allocation's size.
    // -------------------------------------------------------------------
    // Each OP-9 repair names the ceiling as the language's limit for the
    // element type and asks for the count the program needs, because the
    // selected target admits less [STOR-6]; every repaired program here
    // states such a count and builds [`every_allocation_repair_builds`].
    RepairPair {
        name: "allocation-fit-refuted.wf",
        rejected: br#"fn main() -> status: std::process::ExitStatus pure {
  let block = box_slots_new::<i64>(capacity: 18446744073709551615_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-9",
        sentences: &[
            "\n  disposition: Refuted\n",
            "\n  mechanical_fix: `18446744073709551615_u64 <= 2305843009213693951_u64` is false, so this allocation cannot be formed: request the count the program needs. `2305843009213693951_u64` is the language's limit for this element type, not a bound to write: the selected target admits a smaller count, so a bound at or near that limit stops at target layout [STOR-6]\n",
        ],
        repaired: &[br#"fn main() -> status: std::process::ExitStatus pure {
  let block = box_slots_new::<i64>(capacity: 4_u64);
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "allocation-fit-over-parameters.wf",
        rejected: br#"fn make(length: u64) -> values: Box<Slots<i64>> pure {
  let block = box_slots_new::<i64>(capacity: length);
  return move block;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = make(length: 4_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-9",
        sentences: &[
            "\n  disposition: Unproved\n",
            "\n  mechanical_fix: with N the largest count the program needs, add `requires length <= N;` to the `contract` of `make`, which each caller then establishes; or guard the allocation with `if length <= N` where refusing a larger count is the intended behavior. `2305843009213693951_u64` is the language's limit for this element type, not a bound to write: the selected target admits a smaller count, so a bound at or near that limit stops at target layout [STOR-6]\n",
        ],
        repaired: &[
            br#"fn make(length: u64) -> values: Box<Slots<i64>> pure contract {
  requires length <= 1000_u64;
} {
  let block = box_slots_new::<i64>(capacity: length);
  return move block;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = make(length: 4_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn make(length: u64) -> values: Box<Slots<i64>> pure {
  if length <= 1000_u64 {
    let block = box_slots_new::<i64>(capacity: length);
    return move block;
  }
  let empty = box_slots_new::<i64>(capacity: 0_u64);
  return move empty;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = make(length: 4_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "allocation-fit-over-a-computed-count.wf",
        rejected: br#"fn widen(x: u64) -> result: u64 pure {
  return x;
}

fn make(length: u64) -> values: Box<Slots<i64>> pure {
  let n = widen(x: length);
  let block = box_slots_new::<i64>(capacity: n);
  return move block;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = make(length: 4_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-9",
        sentences: &[
            "\n  disposition: Unproved\n",
            "\n  mechanical_fix: `n` is not bounded here: with N the largest count the program needs, when facts that reach the allocation imply `n <= N`, prove it with an `invariant` whose `use` steps name them (a loop's header `invariant` for a value the loop computes); when the callee whose result it reads can prove that bound, state it in the callee's `ensures`; or guard the allocation with `if n <= N` where refusing a larger count is the intended behavior. `2305843009213693951_u64` is the language's limit for this element type, not a bound to write: the selected target admits a smaller count, so a bound at or near that limit stops at target layout [STOR-6]\n",
        ],
        repaired: &[
            br#"fn widen(x: u64) -> result: u64 pure contract {
  ensures result <= 1000_u64;
} {
  if x <= 1000_u64 {
    return x;
  }
  return 1000_u64;
}

fn make(length: u64) -> values: Box<Slots<i64>> pure {
  let n = widen(x: length);
  let block = box_slots_new::<i64>(capacity: n);
  return move block;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = make(length: 4_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn widen(x: u64) -> result: u64 pure {
  return x;
}

fn make(length: u64) -> values: Box<Slots<i64>> pure {
  let n = widen(x: length);
  if n <= 1000_u64 {
    let block = box_slots_new::<i64>(capacity: n);
    return move block;
  }
  let empty = box_slots_new::<i64>(capacity: 0_u64);
  return move empty;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = make(length: 4_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    // -------------------------------------------------------------------
    // [REF-4] one range-formation conjunct.
    // -------------------------------------------------------------------
    RepairPair {
        name: "range-formation-refuted.wf",
        rejected: br#"fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u64, 4>(value: 0_u64);
  let view = &values[0_u64..5_u64];
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "REF-4",
        sentences: &[
            "\n  disposition: Refuted\n",
            "` is false where this range is formed: choose endpoints that satisfy it\n",
        ],
        repaired: &[br#"fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u64, 4>(value: 0_u64);
  let view = &values[0_u64..4_u64];
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "range-formation-over-parameters.wf",
        rejected: br#"fn part(values: &[u64], hi: u64) -> result: u64 pure {
  let view = &deref(values)[0_u64..hi];
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "REF-4",
        sentences: &[
            "\n  disposition: Unproved\n",
            "\n  mechanical_fix: add `requires hi <= deref(values).len;` to the `contract` of `part`, which each caller then establishes; or guard the range with `if hi <= deref(values).len` where skipping it is the intended behavior, adding to the effect row any read that condition makes which the row does not yet declare\n",
        ],
        repaired: &[
            br#"fn part(values: &[u64], hi: u64) -> result: u64 pure contract {
  requires hi <= deref(values).len;
} {
  let view = &deref(values)[0_u64..hi];
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn part(values: &[u64], hi: u64) -> result: u64 reads(values.len) {
  if hi <= deref(values).len {
    let view = &deref(values)[0_u64..hi];
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "range-formation-over-a-computed-endpoint.wf",
        rejected: br#"fn widen(x: u64) -> result: u64 pure {
  return x;
}

fn part(values: &[u64], hi: u64) -> result: u64 pure {
  let h = widen(x: hi);
  let view = &deref(values)[0_u64..h];
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "REF-4",
        sentences: &[
            "\n  disposition: Unproved\n",
            "\n  mechanical_fix: `h <= deref(values).len` is not proved here: when facts that reach the range imply it, prove it with an `invariant` whose `use` steps name them (a loop's header `invariant` for a value the loop computes); when the callee whose result it reads can prove the bound, state it in that callee's `ensures`; or guard the range with `if h <= deref(values).len` where skipping it is the intended behavior, adding to the effect row any read that condition makes which the row does not yet declare\n",
        ],
        repaired: &[br#"fn widen(x: u64) -> result: u64 pure {
  return x;
}

fn part(values: &[u64], hi: u64) -> result: u64 reads(values.len) {
  let h = widen(x: hi);
  if h <= deref(values).len {
    let view = &deref(values)[0_u64..h];
  }
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    // -------------------------------------------------------------------
    // [OP-14] `free_empty`'s requirement that the window is empty.
    // -------------------------------------------------------------------
    RepairPair {
        name: "empty-run-release-refuted.wf",
        rejected: br#"nodrop struct Token {
  value: u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = slots_new::<Token, 2>();
  let t = Token(value: 1_u64);
  place_back(window: &r, value: move t);
  free_empty(window: move r);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-14",
        sentences: &[
            "\n  disposition: Refuted\n",
            "\n  mechanical_fix: the window still holds elements here: take every element out and consume it before `free_empty`\n",
        ],
        repaired: &[br#"nodrop struct Token {
  value: u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = slots_new::<Token, 2>();
  let t = Token(value: 1_u64);
  place_back(window: &r, value: move t);
  let back = take_back(window: &r);
  let Token(value: v) = move back;
  free_empty(window: move r);
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "empty-run-release-over-a-parameter.wf",
        rejected: br#"fn release(window: Box<Slots<u8>>) -> result: unit pure {
  free_empty(window: move window);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-14",
        sentences: &[
            "\n  disposition: Unproved\n",
            "\n  mechanical_fix: add `requires window.inner.len == 0_u64;` to the `contract` of `release`, which each caller then establishes, or take every element out and consume it before this call, so that its zero length is established here\n",
        ],
        repaired: &[br#"fn release(window: Box<Slots<u8>>) -> result: unit pure contract {
  requires window.inner.len == 0_u64;
} {
  free_empty(window: move window);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    // -------------------------------------------------------------------
    // [INV-1] local and loop invariants: refuted when the state derives the
    // negation of one of the target's bounds [MSR-4].
    // -------------------------------------------------------------------
    RepairPair {
        name: "local-invariant-refuted.wf",
        rejected: br#"fn main() -> status: std::process::ExitStatus pure {
  let x = 255_u8;
  invariant fits: x <= 254_u8;
  let y = x +wrap 1_u8;
  return std::process::exit_status(code: y);
}
"#,
        rule: "INV-1",
        sentences: &[
            "\n  disposition: Refuted\n",
            "\n  mechanical_fix: `fits` is false where it is stated: correct the relation, or state one that the facts reaching it imply\n",
        ],
        repaired: &[br#"fn main() -> status: std::process::ExitStatus pure {
  let x = 255_u8;
  invariant fits: x <= 255_u8;
  let y = x +wrap 1_u8;
  return std::process::exit_status(code: y);
}
"#],
    },
    RepairPair {
        name: "local-invariant-unproved.wf",
        rejected: br#"fn check(x: u8) -> result: u8 pure {
  invariant fits: x <= 254_u8;
  return x;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "INV-1",
        sentences: &[
            "\n  disposition: Unproved\n",
            "\n  mechanical_fix: `fits` is not proved from the facts that reach it: establish them before it, add `use` steps naming the facts it follows from, or weaken it\n",
        ],
        repaired: &[
            br#"fn check(x: u8) -> result: u8 pure contract {
  requires x <= 254_u8;
} {
  invariant fits: x <= 254_u8;
  return x;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn check(x: u8) -> result: u8 pure {
  invariant fits: x <= 255_u8;
  return x;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "loop-invariant-base-refuted.wf",
        rejected: br#"fn main() -> status: std::process::ExitStatus pure {
  let sum = 10_u64;
  for (
    i in 0_u64..4_u64,
    invariant low: sum <= 5_u64
  ) {
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "INV-1",
        sentences: &[
            "\n  obligation: Base\n",
            "\n  disposition: Refuted\n",
            "\n  mechanical_fix: `low` is false on entry to the loop: correct it, or change the values the loop starts from\n",
        ],
        repaired: &[
            br#"fn main() -> status: std::process::ExitStatus pure {
  let sum = 0_u64;
  for (
    i in 0_u64..4_u64,
    invariant low: sum <= 5_u64
  ) {
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn main() -> status: std::process::ExitStatus pure {
  let sum = 10_u64;
  for (
    i in 0_u64..4_u64,
    invariant low: sum <= 10_u64
  ) {
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "loop-invariant-base-unproved.wf",
        rejected: br#"fn run(start: u64) -> result: u64 pure {
  let sum = start;
  for (
    i in 0_u64..4_u64,
    invariant low: sum <= 5_u64
  ) {
  }
  return sum;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "INV-1",
        sentences: &[
            "\n  obligation: Base\n",
            "\n  disposition: Unproved\n",
            "\n  mechanical_fix: `low` is not proved on entry to the loop: establish before the loop the facts it follows from, or weaken or correct it\n",
        ],
        repaired: &[br#"fn run(start: u64) -> result: u64 pure contract {
  requires start <= 5_u64;
} {
  let sum = start;
  for (
    i in 0_u64..4_u64,
    invariant low: sum <= 5_u64
  ) {
  }
  return sum;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "loop-invariant-backedge-refuted.wf",
        rejected: br#"fn main() -> status: std::process::ExitStatus pure {
  let sum = 0_u64;
  for (
    i in 0_u64..4_u64,
    invariant fixed: sum <= 0_u64
  ) {
    set sum = sum + 1_u64;
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "INV-1",
        sentences: &[
            "\n  obligation: Backedge\n",
            "\n  disposition: Refuted\n",
            "\n  mechanical_fix: an iteration makes `fixed` false at the next loop header: correct it, or change the body so that every iteration preserves it\n",
        ],
        repaired: &[br#"fn main() -> status: std::process::ExitStatus pure {
  let sum = 0_u64;
  for (
    i in 0_u64..4_u64,
    invariant fixed: sum <= i
  ) {
    set sum = sum + 1_u64;
  }
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "loop-invariant-backedge-unproved.wf",
        rejected: br#"fn run(step: u64) -> result: u64 pure {
  let sum = 0_u64;
  for (
    i in 0_u64..4_u64,
    invariant bounded: sum <= i
  ) {
    set sum = sum +wrap step;
  }
  return sum;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "INV-1",
        sentences: &[
            "\n  obligation: Backedge\n",
            "\n  disposition: Unproved\n",
            "\n  mechanical_fix: `bounded` is not proved preserved at the next loop header: strengthen the invariant prefix, weaken or correct it, or establish in the body the facts from which every reachable fallthrough preserves it\n",
        ],
        repaired: &[br#"fn run(step: u64) -> result: u64 pure contract {
  requires step <= 1_u64;
} {
  let sum = 0_u64;
  for (
    i in 0_u64..4_u64,
    invariant bounded: sum <= i
  ) {
    set sum = sum + step;
  }
  return sum;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    // -------------------------------------------------------------------
    // Repairs that no goal selects.
    // -------------------------------------------------------------------
    RepairPair {
        name: "declared-row-is-narrower-than-the-body.wf",
        rejected: br#"fn touch(data: &[u8]) -> out: u64 pure {
  return deref(data).len;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "EFF-2",
        sentences: &[
            "\n  mechanical_fix: declare the row as `reads(data.len)`, which covers every access the body makes and no other\n",
        ],
        repaired: &[br#"fn touch(data: &[u8]) -> out: u64 reads(data.len) {
  return deref(data).len;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    // The suggested row names the body's range read beside its written
    // slot, and a call that passes the slot outside the range carries it
    // out [OWN-7, EFF-5].
    RepairPair {
        name: "declared-row-omits-a-range-read-beside-a-written-slot.wf",
        rejected: br#"fn record_run(values: &Array<u64, 4>, start: u64, end: u64, slot: u64) -> result: unit writes(values[slot]) contract {
  requires start <= end;
  requires end <= 4_u64;
  requires slot < 4_u64;
} {
  let run = &deref(values)[start..end];
  let length = deref(run).len;
  set deref(values)[slot] = length;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u64, 4>(value: 0_u64);
  record_run(values: &values, start: 0_u64, end: 2_u64, slot: 3_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "EFF-2",
        sentences: &[
            "\n  mechanical_fix: declare the row as `reads(values[start..end].len), writes(values[slot])`, which covers every access the body makes and no other\n",
        ],
        repaired: &[
            br#"fn record_run(values: &Array<u64, 4>, start: u64, end: u64, slot: u64) -> result: unit reads(values[start..end].len), writes(values[slot]) contract {
  requires start <= end;
  requires end <= 4_u64;
  requires slot < 4_u64;
} {
  let run = &deref(values)[start..end];
  let length = deref(run).len;
  set deref(values)[slot] = length;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u64, 4>(value: 0_u64);
  record_run(values: &values, start: 0_u64, end: 2_u64, slot: 3_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "row-writes-before-reads.wf",
        rejected: br#"struct Stats {
  count: u64;
  total: u64;
}

fn record(stats: &Stats) -> result: unit writes(stats.count), reads(stats.count) {
  let old = deref(stats).count;
  set deref(stats).count = old +wrap 1_u64;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let stats = Stats(count: 0_u64, total: 0_u64);
  record(stats: &stats);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "EFF-1",
        sentences: &[
            "\n  mechanical_fix: write every `reads` entry before the first `writes` entry, and delete each entry whose path is a `writes` entry's path or lies below it, which that `writes` already covers\n",
        ],
        repaired: &[br#"struct Stats {
  count: u64;
  total: u64;
}

fn record(stats: &Stats) -> result: unit writes(stats.count) {
  let old = deref(stats).count;
  set deref(stats).count = old +wrap 1_u64;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let stats = Stats(count: 0_u64, total: 0_u64);
  record(stats: &stats);
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        // [TYPE-2, PRE-2] a host handle is formed only by a host function; the program's entry receives this one.
        name: "host-handle-constructed-from-entry.wf",
        rejected: br#"alias ExitStatus = std::process::ExitStatus;
alias HandleFactory = std::io::HandleFactory;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  let factory = HandleFactory();
  return exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-2",
        sentences: &[
            "\n  mechanical_fix: a host handle is formed only by a host function [PRE-2]: replace this construction with a handle that a function of its module returns or that the program's entry receives\n",
        ],
        repaired: &[
            br#"alias ExitStatus = std::process::ExitStatus;
alias Inputs = std::process::Inputs;
alias close_directory = std::fs::close_directory;
alias exit_status = std::process::exit_status;

fn main(inputs: Inputs) -> status: ExitStatus pure {
  let Inputs(args: unused_args, cwd: unused_cwd, stdout: unused_stdout, stderr: unused_stderr, handles: factory, stdin: unused_stdin) = move inputs;
  close_directory(factory: &factory, directory: move unused_cwd);
  return exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        // [TYPE-2, PRE-2] the same repair, where a function of the handle's module returns one.
        name: "host-handle-constructed-from-function.wf",
        rejected: br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  let status = ExitStatus();
  return move status;
}
"#,
        rule: "TYPE-2",
        sentences: &[
            "\n  mechanical_fix: a host handle is formed only by a host function [PRE-2]: replace this construction with a handle that a function of its module returns or that the program's entry receives\n",
        ],
        repaired: &[br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  let status = exit_status(code: 0_u8);
  return move status;
}
"#],
    },
    RepairPair {
        // [TYPE-2, PRE-2] a host handle has no fields, so taking one apart does nothing a `nocopy` handle needs.
        name: "host-handle-taken-apart.wf",
        rejected: br#"alias ExitStatus = std::process::ExitStatus;
alias HandleFactory = std::io::HandleFactory;
alias Inputs = std::process::Inputs;
alias close_directory = std::fs::close_directory;
alias exit_status = std::process::exit_status;

fn main(inputs: Inputs) -> status: ExitStatus pure {
  let Inputs(args: unused_args, cwd: unused_cwd, stdout: unused_stdout, stderr: unused_stderr, handles: factory, stdin: unused_stdin) = move inputs;
  close_directory(factory: &factory, directory: move unused_cwd);
  let HandleFactory() = move factory;
  return exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-2",
        sentences: &[
            "\n  mechanical_fix: a host handle has no fields to take apart [PRE-2]: remove this statement\n",
        ],
        repaired: &[
            br#"alias ExitStatus = std::process::ExitStatus;
alias Inputs = std::process::Inputs;
alias close_directory = std::fs::close_directory;
alias exit_status = std::process::exit_status;

fn main(inputs: Inputs) -> status: ExitStatus pure {
  let Inputs(args: unused_args, cwd: unused_cwd, stdout: unused_stdout, stderr: unused_stderr, handles: factory, stdin: unused_stdin) = move inputs;
  close_directory(factory: &factory, directory: move unused_cwd);
  return exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        // [TYPE-2, PRE-2, PROV-6] a `nodrop` host handle leaves its scope only by moving into the function that closes it.
        name: "nodrop-host-handle-taken-apart.wf",
        rejected: br#"alias ExitStatus = std::process::ExitStatus;
alias DirectoryRead = std::fs::DirectoryRead;
alias Inputs = std::process::Inputs;
alias exit_status = std::process::exit_status;

fn main(inputs: Inputs) -> status: ExitStatus pure {
  let Inputs(args: unused_args, cwd: directory, stdout: unused_stdout, stderr: unused_stderr, handles: factory, stdin: unused_stdin) = move inputs;
  let DirectoryRead() = move directory;
  return exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-2",
        sentences: &[
            "\n  mechanical_fix: a host handle has no fields to take apart [PRE-2], and a `nodrop` one leaves its scope only by moving out [PROV-6]: replace this statement with a call to the function of its module that closes the handle, which also takes a `HandleFactory` reference\n",
        ],
        repaired: &[
            br#"alias ExitStatus = std::process::ExitStatus;
alias Inputs = std::process::Inputs;
alias close_directory = std::fs::close_directory;
alias exit_status = std::process::exit_status;

fn main(inputs: Inputs) -> status: ExitStatus pure {
  let Inputs(args: unused_args, cwd: directory, stdout: unused_stdout, stderr: unused_stderr, handles: factory, stdin: unused_stdin) = move inputs;
  close_directory(factory: &factory, directory: move directory);
  return exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        // [TYPE-2] no value of a program's own opaque struct is ever formed, so the repair is at its declaration.
        name: "program-opaque-struct-constructed.wf",
        rejected: br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

opaque struct Token {
  value: u64;
}

fn main() -> status: ExitStatus pure {
  let token = Token(value: 1_u64);
  return exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-2",
        sentences: &[
            "\n  mechanical_fix: no value of an opaque struct the program declares is ever formed [TYPE-2]: remove `opaque` from its declaration\n",
        ],
        repaired: &[
            br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

struct Token {
  value: u64;
}

fn main() -> status: ExitStatus pure {
  let token = Token(value: 1_u64);
  return exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        // [TYPE-2] the same for taking one apart.
        name: "program-opaque-struct-taken-apart.wf",
        rejected: br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

opaque nocopy struct Token {
  value: u64;
}

fn open(token: Token) -> result: u64 pure {
  let Token(value: inside) = move token;
  return inside;
}

fn main() -> status: ExitStatus pure {
  return exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-2",
        sentences: &[
            "\n  mechanical_fix: no value of an opaque struct the program declares is ever formed [TYPE-2]: remove `opaque` from its declaration\n",
        ],
        repaired: &[
            br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

nocopy struct Token {
  value: u64;
}

fn open(token: Token) -> result: u64 pure {
  let Token(value: inside) = move token;
  return inside;
}

fn main() -> status: ExitStatus pure {
  return exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        // [TYPE-2, TYPE-9, OWN-1] a cell's copy content is read through `inner`, never moved.
        name: "cell-with-copy-content-taken-apart.wf",
        rejected: br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  let cell = box_new::<u64>(value: 5_u64);
  let Box(inner: content) = move cell;
  return exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-2",
        sentences: &[
            "\n  mechanical_fix: a cell's content is its member `inner` [TYPE-9]: replace this statement with `let content = cell.inner;`\n",
        ],
        repaired: &[
            br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  let cell = box_new::<u64>(value: 5_u64);
  let content = cell.inner;
  return exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        // [TYPE-2, TYPE-9, WIN-3] a content without copy moves out through `inner`, which frees the cell.
        name: "cell-with-owned-content-taken-apart.wf",
        rejected: br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

nocopy struct Token {
  value: u64;
}

fn main() -> status: ExitStatus pure {
  let token = Token(value: 1_u64);
  let cell = box_new::<Token>(value: move token);
  let Box(inner: content) = move cell;
  let Token(value: inside) = move content;
  return exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-2",
        sentences: &[
            "\n  mechanical_fix: a cell's content is its member `inner` [TYPE-9]: replace this statement with `let content = move cell.inner;`, which frees the cell [WIN-3]\n",
        ],
        repaired: &[
            br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

nocopy struct Token {
  value: u64;
}

fn main() -> status: ExitStatus pure {
  let token = Token(value: 1_u64);
  let cell = box_new::<Token>(value: move token);
  let content = move cell.inner;
  let Token(value: inside) = move content;
  return exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        // [TYPE-2, TYPE-9, OP-14] a runtime-capacity content is used where it is, and the cell itself is what `free_empty` consumes.
        name: "cell-with-runtime-capacity-content-taken-apart.wf",
        rejected: br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  let cell = box_slots_new::<u64>(capacity: 4_u64);
  let Box(inner: window) = move cell;
  let count = window.len;
  free_empty(window: move window);
  return exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-2",
        sentences: &[
            "\n  mechanical_fix: a cell's content is its member `inner` [TYPE-9], and a runtime-capacity content never leaves it: remove this statement, and write `cell.inner` where `window` is used and `move cell` where `window` is moved [OP-14]\n",
        ],
        repaired: &[
            br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  let cell = box_slots_new::<u64>(capacity: 4_u64);
  let count = cell.inner.len;
  free_empty(window: move cell);
  return exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        // [TYPE-2, TYPE-9, OWN-1] nothing moves out of a cell a reference reaches, so a content never moved is used in place.
        name: "cell-through-a-reference-taken-apart.wf",
        rejected: br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

nocopy struct Token {
  value: u64;
}

fn peek(cell: &Box<Token>) -> result: u64 reads(cell) {
  let Box(inner: token) = move deref(cell);
  return token.value;
}

fn main() -> status: ExitStatus pure {
  return exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-2",
        sentences: &[
            "\n  mechanical_fix: a cell's content is its member `inner` [TYPE-9], and nothing moves out of a cell a reference or an element reaches [OWN-1, WIN-3]: when `token` is never moved, remove this statement and write `deref(cell).inner` where `token` is used\n",
        ],
        repaired: &[
            br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

nocopy struct Token {
  value: u64;
}

fn peek(cell: &Box<Token>) -> result: u64 reads(cell) {
  return deref(cell).inner.value;
}

fn main() -> status: ExitStatus pure {
  return exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        // [TYPE-2, TYPE-9] a statement that binds nothing only consumes the cell, which its scope releases when the content is not linear.
        name: "cell-taken-apart-binding-nothing.wf",
        rejected: br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  let cell = box_new::<u64>(value: 5_u64);
  let Box(..) = move cell;
  return exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-2",
        sentences: &[
            "\n  mechanical_fix: a cell's content is its member `inner` [TYPE-9]: remove this statement\n",
        ],
        repaired: &[
            br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  let cell = box_new::<u64>(value: 5_u64);
  return exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        // [TYPE-2, TYPE-9, WIN-3] nothing moves out of a cell an element reaches, so a content never moved is used in place.
        name: "cell-in-an-element-taken-apart.wf",
        rejected: br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

nocopy struct Token {
  value: u64;
}

fn first(values: Array<Box<Token>, 2>) -> result: u64 pure {
  let Box(inner: token) = move values[0_u64];
  return token.value;
}

fn main() -> status: ExitStatus pure {
  return exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-2",
        sentences: &[
            "\n  mechanical_fix: a cell's content is its member `inner` [TYPE-9], and nothing moves out of a cell a reference or an element reaches [OWN-1, WIN-3]: when `token` is never moved, remove this statement and write `values[0_u64].inner` where `token` is used\n",
        ],
        repaired: &[br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

nocopy struct Token {
  value: u64;
}

fn first(values: Array<Box<Token>, 2>) -> result: u64 pure {
  return values[0_u64].inner.value;
}

fn main() -> status: ExitStatus pure {
  return exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        // [TYPE-2, TYPE-7] a reference variable written bare is read through `deref`.
        name: "cell-through-a-bare-reference-taken-apart.wf",
        rejected: br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn peek(cell: &Box<u64>) -> result: u64 reads(cell) {
  let Box(inner: inside) = move cell;
  return inside;
}

fn main() -> status: ExitStatus pure {
  return exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-2",
        sentences: &[
            "\n  mechanical_fix: a cell's content is its member `inner` [TYPE-9]: replace this statement with `let inside = deref(cell).inner;`\n",
        ],
        repaired: &[br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn peek(cell: &Box<u64>) -> result: u64 reads(cell) {
  let inside = deref(cell).inner;
  return inside;
}

fn main() -> status: ExitStatus pure {
  return exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        // [TYPE-2, PRE-2, OWN-1] a `nodrop` handle a reference reaches is closed by its owner, not here.
        name: "nodrop-host-handle-through-a-reference-taken-apart.wf",
        rejected: br#"alias ExitStatus = std::process::ExitStatus;
alias DirectoryRead = std::fs::DirectoryRead;
alias exit_status = std::process::exit_status;

fn discard(directory: &DirectoryRead) -> result: unit pure {
  let DirectoryRead() = move deref(directory);
  return unit;
}

fn main() -> status: ExitStatus pure {
  return exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-2",
        sentences: &[
            "\n  mechanical_fix: a host handle has no fields to take apart [PRE-2]: remove this statement\n",
        ],
        repaired: &[br#"alias ExitStatus = std::process::ExitStatus;
alias DirectoryRead = std::fs::DirectoryRead;
alias exit_status = std::process::exit_status;

fn discard(directory: &DirectoryRead) -> result: unit pure {
  return unit;
}

fn main() -> status: ExitStatus pure {
  return exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "swap-of-a-possible-ancestor.wf",
        rejected: br#"struct Node {
  value: u64;
  next: Option<Box<Node>>;
}

fn exchange(rows: &Array<Node, 2>, i: u64, j: u64) -> result: unit writes(rows) {
  if i < 2_u64 {
    if j < 2_u64 {
      let parent = &deref(rows)[j];
      match deref(parent).next {
        Some(value: child) => {
          swap(first: &deref(rows)[i], second: &deref(child).inner);
        }
        None() => {
        }
      }
    }
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-11",
        sentences: &[
            "\n  mechanical_fix: prove before the `swap` that the two positions are distinct, or exchange places that are equal or disjoint without an ancestor relation\n",
        ],
        repaired: &[br#"struct Node {
  value: u64;
  next: Option<Box<Node>>;
}

fn exchange(rows: &Array<Node, 2>, i: u64, j: u64) -> result: unit writes(rows) {
  if i < j {
    if j < 2_u64 {
      let parent = &deref(rows)[j];
      match deref(parent).next {
        Some(value: child) => {
          swap(first: &deref(rows)[i], second: &deref(child).inner);
        }
        None() => {
        }
      }
    }
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "value-match-whose-arms-all-return.wf",
        rejected: br#"fn choose(flag: Option<i32>) -> result: i32 pure {
  let picked = match flag {
    Some(value: inner) => {
      return inner;
    }
    None() => {
      return 0_i32;
    }
  }
  return picked;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "GIVE-1",
        sentences: &[
            "]: EmptyDeliverySet\n",
            "\n  binding: picked\n  mechanical_fix: every arm leaves by `return` or `break`, so no value reaches `picked`: drop `let picked =`, write the `match` as a statement, and delete the statements after it in this block, which no path reaches\n",
        ],
        repaired: &[br#"fn choose(flag: Option<i32>) -> result: i32 pure {
  match flag {
    Some(value: inner) => {
      return inner;
    }
    None() => {
      return 0_i32;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "set-of-an-undeclared-name.wf",
        rejected: br#"fn main() -> status: std::process::ExitStatus pure {
  set total = 1_u64;
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "SET-1",
        sentences: &[
            "]: UndeclaredSetTarget\n",
            "\n  spelling: total\n  mechanical_fix: no binding `total` is in scope, so this `set` declares nothing: write it as `let total = ...;`, keeping its right-hand side, to declare the binding here, or name a binding that is in scope\n",
        ],
        repaired: &[br#"fn main() -> status: std::process::ExitStatus pure {
  let total = 1_u64;
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "front-operation-on-a-slots-window.wf",
        rejected: br#"fn main() -> status: std::process::ExitStatus pure {
  let values = slots_new::<u8, 2>();
  place_front(window: &values, value: 7_u8);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-10",
        sentences: &[
            "]: UnadmittedOperandShape\n",
            "\n  expected: a `Ring` operand, which is what this row admits\n  mechanical_fix: pass an operand of the admitted shape, or use an operation whose row admits this operand's shape\n",
        ],
        repaired: &[br#"fn main() -> status: std::process::ExitStatus pure {
  let values = slots_new::<u8, 2>();
  place_back(window: &values, value: 7_u8);
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "bound-function-exceeds-the-formal-row.wf",
        rejected: br#"interface Disposer {
  fn release(factory: &std::io::HandleFactory, file: std::fs::ReadFile) -> function_result: unit pure;
}

binding First : Disposer {
  release = release_read_file;
}

fn release_read_file(factory: &std::io::HandleFactory, file: std::fs::ReadFile) -> function_result: unit writes(factory) {
  let closed = std::fs::close_read(factory: factory, file: move file);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FN-4",
        sentences: &[
            "]: BehaviorArgumentMismatch\n",
            "\n  mechanical_fix: supply a function whose signature, row and contract meet the formal interface, or weaken the formal interface to what the supplied function declares\n",
        ],
        repaired: &[br#"interface Disposer {
  fn release(factory: &std::io::HandleFactory, file: std::fs::ReadFile) -> function_result: unit writes(factory);
}

binding First : Disposer {
  release = release_read_file;
}

fn release_read_file(factory: &std::io::HandleFactory, file: std::fs::ReadFile) -> function_result: unit writes(factory) {
  let closed = std::fs::close_read(factory: factory, file: move file);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
    RepairPair {
        name: "route-over-two-result-ordinals.wf",
        rejected: br#"fn probe(taken: Slots<u8, 8>) -> (outcome: Result<u64, Overflow>, other: Result<u64, Overflow>) pure contract {
  ensures when Ok(value: reported): reported == taken.len;
} {
  let measured = taken.len;
  return Ok<u64, Overflow>(value: measured), Ok<u64, Overflow>(value: measured);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "CALL-4",
        sentences: &[
            "\n  mechanical_fix: more than one result can carry this route: name the one it applies to, writing `when r is` before its variant with `r` one of `outcome`, `other`\n",
        ],
        repaired: &[br#"fn probe(taken: Slots<u8, 8>) -> (outcome: Result<u64, Overflow>, other: Result<u64, Overflow>) pure contract {
  ensures when outcome is Ok(value: reported): reported == taken.len;
} {
  let measured = taken.len;
  return Ok<u64, Overflow>(value: measured), Ok<u64, Overflow>(value: measured);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
];

/// Every function a source declares, by the name its `fn` introduces.
fn declared_functions(source: &[u8]) -> Vec<String> {
    let text = std::str::from_utf8(source).expect("a pinned source is UTF-8");
    text.split("fn ")
        .skip(1)
        .filter_map(|rest| {
            let name: String = rest
                .chars()
                .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
                .collect();
            (!name.is_empty()).then_some(name)
        })
        .collect()
}

/// The judgments in `source` that succeed only in a contradictory state, or
/// the rejection that stops it.
fn contradictory_successes(name: &str, source: &[u8]) -> Result<Vec<String>, String> {
    let functions = declared_functions(source);
    super::with_checked_program(
        &[SourceInput::new(name, source)],
        None,
        CompilerLimits::default(),
        |program, _| Ok(program.contradictory_successes(&functions)),
    )
    .map_err(|failure| failure.to_string())
}

/// Each pinned repair is carried out by its programs [DIAG-1]: each
/// alternative is accepted, and its construct runs in a state that is not
/// contradictory.
#[test]
fn each_pinned_repair_is_carried_out_by_its_programs() {
    for pair in REPAIRS.iter().chain(call_separations::CALL_SEPARATIONS) {
        let failure = compile(
            &[SourceInput::new(pair.name, pair.rejected)],
            CompilerLimits::default(),
        )
        .expect_err(pair.name);
        assert_eq!(
            failure.kind(),
            CompilationFailureKind::Source,
            "{}: {failure}",
            pair.name
        );
        assert_eq!(
            failure.rule_id(),
            Some(pair.rule),
            "{}: {failure}",
            pair.name
        );
        let rendered = format!("{failure}\n");
        for sentence in pair.sentences {
            assert!(
                rendered.contains(sentence),
                "{}: the rejection no longer carries this repair.\nwanted: {sentence}\ngot:    {rendered}",
                pair.name,
            );
        }
        for (alternative, source) in pair.repaired.iter().enumerate() {
            match contradictory_successes(pair.name, source) {
                Ok(contradictions) => assert!(
                    contradictions.is_empty(),
                    "{}: alternative {alternative} succeeds only where its state is contradictory: {contradictions:?}",
                    pair.name
                ),
                Err(rejection) => panic!(
                    "{}: alternative {alternative} is rejected:\n{rejection}",
                    pair.name
                ),
            }
        }
    }
}

/// [OP-9, STOR-6] an allocation's repair is carried out only when the
/// repaired program also builds: after checking, the selected target
/// qualifies the retained bound of every allocation the entry runs, which
/// [`each_pinned_repair_is_carried_out_by_its_programs`] does not reach.
#[test]
fn every_allocation_repair_builds() {
    for pair in REPAIRS.iter().filter(|pair| pair.rule == "OP-9") {
        for (alternative, source) in pair.repaired.iter().enumerate() {
            if let Err(failure) = compile(
                &[SourceInput::new(pair.name, source)],
                CompilerLimits::default(),
            ) {
                panic!(
                    "{}: alternative {alternative} does not build:\n{failure}",
                    pair.name
                );
            }
        }
    }
}

/// Why no OP-9 repair offers its ceiling as the bound to write: a program
/// that states it passes OP-9 and stops at target layout [STOR-6].
#[test]
fn an_allocation_bound_at_the_language_ceiling_stops_at_target_layout() {
    let source = br#"fn make(length: u64) -> values: Box<Slots<i64>> pure contract {
  requires length <= 2305843009213693951_u64;
} {
  let block = box_slots_new::<i64>(capacity: length);
  return move block;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = make(length: 4_u64);
  return std::process::exit_status(code: 0_u8);
}
"#;
    super::check(
        &[SourceInput::new("ceiling.wf", source)],
        CompilerLimits::default(),
    )
    .expect("the language's ceiling passes OP-9");
    let failure = compile(
        &[SourceInput::new("ceiling.wf", source)],
        CompilerLimits::default(),
    )
    .expect_err("the ceiling exceeds the selected target's allocation domain");
    assert_eq!(
        failure.kind(),
        CompilationFailureKind::TargetLayout,
        "{failure}"
    );
}

/// The pair test's second condition is live: a guard around a refuted goal
/// is accepted, and the goal inside it succeeds only because the branch
/// state is contradictory, which is what makes guarding a refuted goal no
/// repair [DIAG-1].
#[test]
fn a_guard_around_a_refuted_goal_succeeds_only_in_a_contradictory_state() {
    let guarded = br#"fn main() -> status: std::process::ExitStatus pure {
  let x = 255_u8;
  if x +defined 1_u8 {
    let y = x + 1_u8;
    return std::process::exit_status(code: y);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let contradictions = contradictory_successes("guarded-refuted-goal.wf", guarded)
        .expect("the guarded program is accepted");
    assert!(
        !contradictions.is_empty(),
        "the operation inside the dead branch must be discharged by contradiction"
    );
}
