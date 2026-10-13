//! [MOD-5] qualified lookup repairs select accessible inventory names in the
//! grammar-selected domain. Each listed replacement is applied literally.
use super::RepairPair;

pub(super) const QUALIFIED_NAMES: &[RepairPair] = &[
    RepairPair {
        name: "qualified-missing-callable.wf",
        rejected: br#"fn zeta() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}

fn alpha() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}

fn main() -> status: std::process::ExitStatus pure {
  return pkg::missing();
}
"#,
        rule: "MOD-5",
        sentences: &[
            "\n  mechanical_fix: Replace `missing` with an accessible name declared in `pkg` for this grammar position [MOD-5]: `alpha`, `main`, `zeta`.\n",
        ],
        repaired: &[
            br#"fn zeta() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}

fn alpha() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}

fn main() -> status: std::process::ExitStatus pure {
  return pkg::alpha();
}
"#,
            br#"fn zeta() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}

fn alpha() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}

fn main() -> status: std::process::ExitStatus pure {
  return pkg::main();
}
"#,
            br#"fn zeta() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}

fn alpha() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}

fn main() -> status: std::process::ExitStatus pure {
  return pkg::zeta();
}
"#,
        ],
    },
    RepairPair {
        name: "qualified-missing-constructor.wf",
        rejected: br#"struct Zeta {
}

struct Alpha {
}

fn main() -> status: std::process::ExitStatus pure {
  let value = pkg::Missing();
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "MOD-5",
        sentences: &[
            "\n  mechanical_fix: Replace `Missing` with an accessible name declared in `pkg` for this grammar position [MOD-5]: `Alpha`, `Zeta`.\n",
        ],
        repaired: &[
            br#"struct Zeta {
}

struct Alpha {
}

fn main() -> status: std::process::ExitStatus pure {
  let value = pkg::Alpha();
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"struct Zeta {
}

struct Alpha {
}

fn main() -> status: std::process::ExitStatus pure {
  let value = pkg::Zeta();
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        // Divergence admits every return type [FN-1], so replacing the type
        // alone tests each inventory suggestion, including opaque/nodrop
        // types, without introducing a construction or consumption repair.
        name: "qualified-missing-standard-type.wf",
        rejected: br#"fn probe() -> result: std::process::Missing pure {
  loop {
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "MOD-5",
        sentences: &[
            "\n  mechanical_fix: Replace `Missing` with an accessible name declared in `std::process` for this grammar position [MOD-5]: `ExitStatus`, `Inputs`, `MemoryMeter`, `StopKind`, `StopListener`, `StopSignals`.\n",
        ],
        repaired: &[
            br#"fn probe() -> result: std::process::ExitStatus pure {
  loop {
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn probe() -> result: std::process::Inputs pure {
  loop {
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn probe() -> result: std::process::MemoryMeter pure {
  loop {
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn probe() -> result: std::process::StopKind pure {
  loop {
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn probe() -> result: std::process::StopListener pure {
  loop {
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
            br#"fn probe() -> result: std::process::StopSignals pure {
  loop {
  }
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        ],
    },
    RepairPair {
        name: "qualified-missing-name-empty-domain.wf",
        rejected: br#"fn probe(value: pkg::Missing) -> result: unit pure {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "MOD-5",
        sentences: &[
            "\n  mechanical_fix: Declare an accessible name in `pkg` for this grammar position [MOD-5].\n",
        ],
        repaired: &[br#"struct Missing {
}

fn probe(value: pkg::Missing) -> result: unit pure {
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#],
    },
];
