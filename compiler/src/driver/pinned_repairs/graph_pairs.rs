//! Repair pairs for the rejections of a module graph [MOD-1, MOD-10, MOD-11].
//!
//! A graph rejection prints no repair, so each alternative is a graph record
//! the rule admits, written from the rule's words, and its program is
//! accepted. A kind with no conformance case rejects a probe: the records of
//! the accepted program `mod1-pos-ordered-graph-and-entries` it replaces.

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

pub(super) const GRAPH_PAIRS: &[ModulePair] = &[
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
];
