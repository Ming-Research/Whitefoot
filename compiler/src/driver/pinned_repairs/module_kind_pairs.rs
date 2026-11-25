//! Repair pairs for the rejections of a module program no source pair reaches
//! [MOD-1 to MOD-11, STOR-8]: its graph, its aliases and edges, its interface
//! and implementation records and its composition.
//!
//! A rejection that prints no repair is paired with an alternative the rule
//! admits, written from the rule's words, whose program is accepted. A kind
//! with no conformance case rejects a probe: the records of the accepted
//! program `mod1-pos-ordered-graph-and-entries` it replaces.

use super::module_pairs::ModulePair;

/// The graph of the bound-package programs when its dependencies are right.
const BOUND_GRAPH: &[u8] = b"package halo = \"halo\";
package json = \"json\";

pkg: [halo::vm, json, std::process];

entry main = pkg::main;
";

/// The accepted program's graph with one row listed twice.
const DUPLICATE_ROW: &[u8] = b"pkg::data: [];
pkg::data: [];
pkg::report: [pkg::data, std::process];
pkg: [pkg::data, pkg::report, std::process];

entry main = pkg::main;
";

/// The accepted program's graph with one dependency listed twice.
const DUPLICATE_DEPENDENCY: &[u8] = b"pkg::data: [];
pkg::report: [pkg::data, std::process];
pkg: [pkg::data, pkg::data, pkg::report, std::process];

entry main = pkg::main;
";

/// The accepted program's graph with a dependency no row registers.
const UNREGISTERED_DEPENDENCY: &[u8] = b"pkg::data: [];
pkg::report: [pkg::data, pkg::nowhere, std::process];
pkg: [pkg::data, pkg::report, std::process];

entry main = pkg::main;
";

/// The accepted program's graph with its entry name taken twice.
const DUPLICATE_ENTRY: &[u8] = b"pkg::data: [];
pkg::report: [pkg::data, std::process];
pkg: [pkg::data, pkg::report, std::process];

entry main = pkg::main;

entry main = pkg::report::quiet {
  no_heap;
}
";

/// The accepted program's graph with an entry in a module no row registers.
const UNREGISTERED_ENTRY: &[u8] = b"pkg::data: [];
pkg::report: [pkg::data, std::process];
pkg: [pkg::data, pkg::report, std::process];

entry main = pkg::main;

entry quiet = pkg::nowhere::quiet {
  no_heap;
}
";

pub(super) const MODULE_KIND_PAIRS: &[ModulePair] = &[
    ModulePair {
        name: "mod1-neg-self-dependency",
        case: "mod1-neg-self-dependency",
        rejected: &[],
        rule: "MOD-1",
        sentences: &[],
        repaired: &[&[(
            "modules.wfg",
            b"pkg: [std::process];

entry main = pkg::main;
",
        )]],
    },
    ModulePair {
        name: "mod1-neg-later-dependency",
        case: "mod1-neg-later-dependency",
        rejected: &[],
        rule: "MOD-1",
        sentences: &[],
        repaired: &[&[(
            "modules.wfg",
            b"pkg::data: [];
pkg: [pkg::data, std::process];

entry main = pkg::main;
",
        )]],
    },
    ModulePair {
        name: "mod1-neg-row-registers-bound-path",
        case: "mod1-neg-row-registers-bound-path",
        rejected: &[],
        rule: "MOD-1",
        sentences: &[],
        repaired: &[&[("modules.wfg", BOUND_GRAPH)]],
    },
    ModulePair {
        name: "probe-duplicate-module-row",
        case: "mod1-pos-ordered-graph-and-entries",
        rejected: &[("modules.wfg", DUPLICATE_ROW)],
        rule: "MOD-1",
        sentences: &[],
        repaired: &[&[]],
    },
    ModulePair {
        name: "probe-duplicate-dependency",
        case: "mod1-pos-ordered-graph-and-entries",
        rejected: &[("modules.wfg", DUPLICATE_DEPENDENCY)],
        rule: "MOD-1",
        sentences: &[],
        repaired: &[&[]],
    },
    ModulePair {
        name: "probe-unregistered-dependency",
        case: "mod1-pos-ordered-graph-and-entries",
        rejected: &[("modules.wfg", UNREGISTERED_DEPENDENCY)],
        rule: "MOD-1",
        sentences: &[],
        repaired: &[&[]],
    },
    ModulePair {
        name: "probe-duplicate-entry",
        case: "mod1-pos-ordered-graph-and-entries",
        rejected: &[("modules.wfg", DUPLICATE_ENTRY)],
        rule: "MOD-1",
        sentences: &[],
        repaired: &[&[]],
    },
    ModulePair {
        name: "probe-unregistered-entry-module",
        case: "mod1-pos-ordered-graph-and-entries",
        rejected: &[("modules.wfg", UNREGISTERED_ENTRY)],
        rule: "MOD-1",
        sentences: &[],
        repaired: &[&[]],
    },
    ModulePair {
        name: "mod10-neg-unknown-standard-module",
        case: "mod10-neg-unknown-standard-module",
        rejected: &[],
        rule: "MOD-10",
        sentences: &[],
        repaired: &[&[(
            "modules.wfg",
            b"pkg: [];

entry main = pkg::main;
",
        )]],
    },
    ModulePair {
        name: "mod10-neg-standard-row",
        case: "mod10-neg-standard-row",
        rejected: &[],
        rule: "MOD-10",
        sentences: &[],
        repaired: &[&[(
            "modules.wfg",
            b"pkg: [];

entry main = pkg::main;
",
        )]],
    },
    ModulePair {
        name: "mod10-neg-standard-entry",
        case: "mod10-neg-standard-entry",
        rejected: &[],
        rule: "MOD-10",
        sentences: &[],
        repaired: &[&[(
            "modules.wfg",
            b"pkg: [std::process];

entry main = pkg::main;
",
        )]],
    },
    ModulePair {
        name: "mod11-neg-binding-cycle",
        case: "mod11-neg-binding-cycle",
        rejected: &[],
        rule: "MOD-11",
        sentences: &[],
        repaired: &[&[("json/modules.wfg", b"pkg: [];\n")]],
    },
    ModulePair {
        name: "mod11-neg-package-bound-twice",
        case: "mod11-neg-package-bound-twice",
        rejected: &[],
        rule: "MOD-11",
        sentences: &[],
        repaired: &[&[("modules.wfg", BOUND_GRAPH)]],
    },
    ModulePair {
        name: "mod11-neg-name-bound-twice",
        case: "mod11-neg-name-bound-twice",
        rejected: &[],
        rule: "MOD-11",
        sentences: &[],
        repaired: &[&[("modules.wfg", BOUND_GRAPH)]],
    },
    ModulePair {
        name: "mod11-neg-location-not-relative",
        case: "mod11-neg-location-not-relative",
        rejected: &[],
        rule: "MOD-11",
        sentences: &[],
        repaired: &[&[("modules.wfg", BOUND_GRAPH)]],
    },
    ModulePair {
        name: "mod11-neg-dependency-root-unbound",
        case: "mod11-neg-dependency-root-unbound",
        rejected: &[],
        rule: "MOD-11",
        sentences: &[],
        repaired: &[&[("modules.wfg", BOUND_GRAPH)]],
    },
    ModulePair {
        name: "mod11-neg-dependency-names-no-bound-module",
        case: "mod11-neg-dependency-names-no-bound-module",
        rejected: &[],
        rule: "MOD-11",
        sentences: &[],
        repaired: &[&[("modules.wfg", BOUND_GRAPH)]],
    },
    ModulePair {
        name: "mod4-neg-alias-after-item",
        case: "mod4-neg-alias-after-item",
        rejected: &[],
        rule: "MOD-4",
        sentences: &[],
        repaired: &[&[(
            "main.wf",
            br#"alias ExitStatus = std::process::ExitStatus;
alias data = pkg::data;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  return exit_status(code: 0_u8);
}
"#,
        )]],
    },
    ModulePair {
        name: "mod4-neg-alias-case-mismatch",
        case: "mod4-neg-alias-case-mismatch",
        rejected: &[],
        rule: "MOD-4",
        sentences: &[],
        repaired: &[&[(
            "main.wf",
            br#"alias ExitStatus = std::process::ExitStatus;
alias Sign = pkg::data::Sign;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  return exit_status(code: 0_u8);
}
"#,
        )]],
    },
    ModulePair {
        name: "mod4-neg-alias-takes-bound-name",
        case: "mod4-neg-alias-takes-bound-name",
        rejected: &[],
        rule: "MOD-4",
        sentences: &[],
        repaired: &[&[(
            "main.wf",
            br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;
alias Value = json::Value;

fn number(v: Value) -> n: i32 pure {
  doc "Reads the json package's own value type.";
  return v.n;
}

fn main() -> status: ExitStatus pure {
  doc "A value made through halo is the json package's own type.";
  let v = halo::vm::box_number(n: 7_i32);
  let n = number(v: v);
  if n == 7_i32 {
    return exit_status(code: 0_u8);
  }
  return exit_status(code: 1_u8);
}
"#,
        )]],
    },
    ModulePair {
        name: "mod5-neg-missing-edge",
        case: "mod5-neg-missing-edge",
        rejected: &[],
        rule: "MOD-5",
        sentences: &[],
        repaired: &[&[(
            "modules.wfg",
            br#"pkg::data: [];
pkg::report: [pkg::data];
pkg: [pkg::data, pkg::report, std::process];

entry main = pkg::main;
"#,
        )]],
    },
    ModulePair {
        name: "mod5-neg-private-declaration",
        case: "mod5-neg-private-declaration",
        rejected: &[],
        rule: "MOD-5",
        sentences: &[],
        repaired: &[&[(
            "data/module.wfm",
            br#"public enum Sign {
  Neg();
  Zero();
  Pos();
}

public fn sign_of(value: i32) -> result: Sign pure doc "Classifies a value by its sign.";

public fn secret() -> result: u8 pure doc "Returns three.";
"#,
        )]],
    },
    ModulePair {
        name: "mod6-neg-public-in-implementation",
        case: "mod6-neg-public-in-implementation",
        rejected: &[],
        rule: "MOD-6",
        sentences: &[],
        repaired: &[&[(
            "main.wf",
            br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn helper() -> result: u8 pure {
  return 0_u8;
}

fn main() -> status: ExitStatus pure {
  let code = helper();
  return exit_status(code: code);
}
"#,
        )]],
    },
    ModulePair {
        name: "mod6-neg-private-type-in-public-signature",
        case: "mod6-neg-private-type-in-public-signature",
        rejected: &[],
        rule: "MOD-6",
        sentences: &[],
        repaired: &[&[(
            "data/module.wfm",
            br#"public struct Secret {
  value: i32;
}

public fn leak(value: Secret) -> result: i32 pure doc "Reads the secret's value.";
"#,
        )]],
    },
    ModulePair {
        name: "mod7-neg-interface-body",
        case: "mod7-neg-interface-body",
        rejected: &[],
        rule: "MOD-7",
        sentences: &[],
        repaired: &[&[
            (
                "module.wfm",
                br#"alias ExitStatus = std::process::ExitStatus;

public fn main() -> status: ExitStatus pure doc "Runs the program.";
"#,
            ),
            (
                "main.wf",
                br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  return exit_status(code: 0_u8);
}
"#,
            ),
        ]],
    },
    ModulePair {
        name: "mod7-neg-interface-declaration-without-doc",
        case: "mod7-neg-interface-declaration-without-doc",
        rejected: &[],
        rule: "MOD-7",
        sentences: &[],
        repaired: &[&[(
            "module.wfm",
            br#"alias ExitStatus = std::process::ExitStatus;

public fn main() -> status: ExitStatus pure doc "Runs the program.";
"#,
        )]],
    },
    ModulePair {
        name: "mod7-neg-definition-differs",
        case: "mod7-neg-definition-differs",
        rejected: &[],
        rule: "MOD-7",
        sentences: &[],
        repaired: &[&[(
            "data/sign.wf",
            br#"fn sign_of(value: i32) -> result: Sign pure {
  doc "Compares the value against zero.";
  if value < 0_i32 {
    return Sign::Neg();
  }
  if value == 0_i32 {
    return Sign::Zero();
  }
  return Sign::Pos();
}
"#,
        )]],
    },
    ModulePair {
        name: "mod8-neg-pending-declaration-blocks-composition",
        case: "mod8-neg-pending-declaration-blocks-composition",
        rejected: &[],
        rule: "MOD-8",
        sentences: &[],
        repaired: &[&[(
            "main.wf",
            br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  return exit_status(code: 0_u8);
}
"#,
        )]],
    },
    ModulePair {
        name: "mod9-neg-heap-declaration-in-module",
        case: "mod9-neg-heap-declaration-in-module",
        rejected: &[],
        rule: "MOD-9",
        sentences: &[],
        repaired: &[&[(
            "main.wf",
            br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  return exit_status(code: 0_u8);
}
"#,
        )]],
    },
    ModulePair {
        name: "mod9-neg-private-entry",
        case: "mod9-neg-private-entry",
        rejected: &[],
        rule: "MOD-9",
        sentences: &[],
        repaired: &[&[(
            "module.wfm",
            br#"alias ExitStatus = std::process::ExitStatus;

public fn main() -> status: ExitStatus pure doc "Runs the program.";
"#,
        )]],
    },
    ModulePair {
        name: "mod5-neg-private-field-selection",
        case: "mod5-neg-private-field-selection",
        rejected: &[],
        rule: "MOD-5",
        sentences: &[],
        repaired: &[&[(
            "main.wf",
            br#"alias ExitStatus = std::process::ExitStatus;
alias data = pkg::data;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  let pair = data::make(left: 1_u8, right: 2_u8);
  let right = data::right_of(pair: pair);
  return exit_status(code: right);
}
"#,
        )]],
    },
    ModulePair {
        name: "mod4-neg-alias-to-private-interface-function",
        case: "mod4-neg-alias-to-private-interface-function",
        rejected: &[],
        rule: "MOD-4",
        sentences: &[],
        repaired: &[&[(
            "main.wf",
            br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  let code = pkg::data::seven();
  return exit_status(code: code);
}
"#,
        )]],
    },
    ModulePair {
        name: "mod3-neg-module-slot-collision",
        case: "mod3-neg-module-slot-collision",
        rejected: &[],
        rule: "MOD-3",
        sentences: &[],
        repaired: &[&[(
            "main.wf",
            br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn helper() -> result: unit pure {
  return unit;
}

fn main() -> status: ExitStatus pure {
  return exit_status(code: 0_u8);
}
"#,
        )]],
    },
    ModulePair {
        name: "stor8-neg-entry-closure-allocates",
        case: "stor8-neg-entry-closure-allocates",
        rejected: &[],
        rule: "STOR-8",
        sentences: &[],
        repaired: &[&[(
            "modules.wfg",
            br#"pkg: [std::process];

entry small = pkg::main;
"#,
        )]],
    },
    ModulePair {
        name: "mod5-neg-qualified-const-in-let",
        case: "mod5-neg-qualified-const-in-let",
        rejected: &[],
        rule: "MOD-5",
        sentences: &[],
        repaired: &[&[(
            "main.wf",
            br#"alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;
alias marker = pkg::data::marker;

fn main() -> status: ExitStatus pure {
  let wanted = marker;
  let other = 1_u32;
  return exit_status(code: 0_u8);
}
"#,
        )]],
    },
    ModulePair {
        name: "probe-qualified-name-not-found",
        case: "mod1-pos-ordered-graph-and-entries",
        rejected: &[(
            "main.wf",
            br#"alias ExitStatus = std::process::ExitStatus;
alias data = pkg::data;
alias exit_status = std::process::exit_status;
alias report = pkg::report;

fn main() -> status: ExitStatus pure {
  let sign = data::sign_off(value: 5_i32);
  let code = report::code_of(sign: sign);
  return exit_status(code: code);
}
"#,
        )],
        rule: "MOD-5",
        sentences: &[],
        repaired: &[&[]],
    },
    ModulePair {
        name: "probe-unresolved-callee",
        case: "mod1-pos-ordered-graph-and-entries",
        rejected: &[(
            "main.wf",
            br#"alias ExitStatus = std::process::ExitStatus;
alias data = pkg::data;
alias exit_status = std::process::exit_status;
alias report = pkg::report;

fn main() -> status: ExitStatus pure {
  let sign = sign_of(value: 5_i32);
  let code = report::code_of(sign: sign);
  return exit_status(code: code);
}
"#,
        )],
        rule: "MOD-5",
        sentences: &[],
        repaired: &[&[]],
    },
    ModulePair {
        name: "probe-unknown-module-alias",
        case: "mod1-pos-ordered-graph-and-entries",
        rejected: &[(
            "main.wf",
            br#"alias ExitStatus = std::process::ExitStatus;
alias data = pkg::data;
alias exit_status = std::process::exit_status;
alias report = pkg::report;
alias nowhere = pkg::nowhere;

fn main() -> status: ExitStatus pure {
  let sign = data::sign_of(value: 5_i32);
  let code = report::code_of(sign: sign);
  return exit_status(code: code);
}
"#,
        )],
        rule: "MOD-4",
        sentences: &[],
        repaired: &[&[]],
    },
];
