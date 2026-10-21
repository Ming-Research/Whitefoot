//! General enum selector admission and field-list diagnostic repairs.

use super::RepairPair;

pub(super) const ENUM_ROUTED_ENSURES: &[RepairPair] = &[
    RepairPair {
        name: "fn9-neg-enum-foreign-variant.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/fn9-neg-enum-foreign-variant.wf"
        ),
        rule: "FN-9",
        sentences: &[
            "]: InvalidPostconditionSelector\n",
            "\n  mechanical_fix: remove this ensures clause, and remove its contract block if no requires or ensures clauses remain; an unrouted clause can name only result data admitted by [CALL-4]; a routed clause selects `when b is V(f: r):` for an own enum result, where b names that result, V is its variant, f is one declared field whose type supplies admitted data, and r is fresh [FN-9]; omit `b is` only when exactly one declared result has the route's enum type [CALL-4]; use a separate clause for each field\n",
        ],
        repaired: &[br#"enum RouteOwner {
  OwnCase(index: u64);
}

fn selected(value: u64) -> result: RouteOwner pure {
  return RouteOwner::OwnCase(index: value);
}

fn main() -> status: std::process::ExitStatus pure {
  let made = selected(value: 7_u64);
  match made {
    OwnCase(index: found) => {
      let code = cvt.wrap::<u64, u8>(found);
      return std::process::exit_status(code: code);
    }
  }
}
"#],
    },
    RepairPair {
        name: "fn9-neg-enum-missing-field.wf",
        rejected: include_bytes!(
            "../../../../tests/conformance/cases/fn9-neg-enum-missing-field.wf"
        ),
        rule: "FN-9",
        sentences: &[
            "]: InvalidPostconditionFields\n",
            "\n  required_fields: [\"first\", \"second\"]\n",
        ],
        repaired: &[include_bytes!(
            "../../../../tests/conformance/cases/fn9-pos-enum-separate-field-clauses.wf"
        )],
    },
    RepairPair {
        name: "fn9-neg-none-route.wf",
        rejected: include_bytes!("../../../../tests/conformance/cases/fn9-neg-none-route.wf"),
        rule: "FN-9",
        sentences: &[
            "]: InvalidPostconditionFields\n",
            "\n  required_fields: []\n",
        ],
        repaired: &[br#"fn nothing(limit: u64) -> result: Option<u64> pure {
  return None<u64>();
}

fn main() -> status: std::process::ExitStatus pure {
  let made = nothing(limit: 7_u64);
  match made {
    Some(value: found) => {
      return std::process::exit_status(code: 1_u8);
    }
    None() => {
      return std::process::exit_status(code: 0_u8);
    }
  }
}
"#],
    },
    RepairPair {
        name: "none-postcondition-retain-requires.wf",
        rejected: br#"fn nothing(limit: u64) -> result: Option<u64> pure contract {
  requires limit <= 7_u64;
  ensures when None(value: payload): payload <= limit;
} {
  return None<u64>();
}

fn main() -> status: std::process::ExitStatus pure {
  let made = nothing(limit: 7_u64);
  match made {
    Some(value: found) => {
      return std::process::exit_status(code: 1_u8);
    }
    None() => {
      return std::process::exit_status(code: 0_u8);
    }
  }
}
"#,
        rule: "FN-9",
        sentences: &[
            "]: InvalidPostconditionFields\n",
            "\n  required_fields: []\n",
        ],
        repaired: &[br#"fn nothing(limit: u64) -> result: Option<u64> pure contract {
  requires limit <= 7_u64;
} {
  return None<u64>();
}

fn main() -> status: std::process::ExitStatus pure {
  let made = nothing(limit: 7_u64);
  match made {
    Some(value: found) => {
      return std::process::exit_status(code: 1_u8);
    }
    None() => {
      return std::process::exit_status(code: 0_u8);
    }
  }
}
"#],
    },
];
