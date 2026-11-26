//! Graph-related syntax repairs are applied to complete module programs.

use super::{CompilerLimits, module_inputs};
use crate::SourceInput;

const INTERFACE: &[u8] = b"public fn main() -> result: unit pure doc \"Runs the program.\";\n";
const BODY: &[u8] = b"fn main() -> result: unit pure {\n  return unit;\n}\n";

#[test]
fn moving_entry_from_a_function_header_to_the_graph_accepts_the_program() {
    let failure = super::compile(
        &[SourceInput::new(
            "main.wf",
            b"entry fn main() -> result: unit pure {\n  return unit;\n}\n",
        )],
        CompilerLimits::default(),
    )
    .expect_err("entry is a graph declaration, not a function modifier");
    assert!(
        failure.to_string().contains("mechanical_fix: remove `entry` before `fn`; declare the entry in the module graph file, for example `entry app = pkg::main;` [GRAM-2, MOD-1]"),
        "{failure}"
    );
    let graph = crate::form_module_graph(
        SourceInput::new("modules.wfg", b"pkg: [];\n\nentry app = pkg::main;\n"),
        CompilerLimits::default(),
    )
    .expect("the repair's entry declaration forms");
    let records = [("module.wfm", INTERFACE), ("main.wf", BODY)];
    let inputs = module_inputs(&graph, &records);
    crate::driver::check_module_entry(
        &graph,
        &inputs,
        crate::driver::ModuleEntry::Named("app"),
        CompilerLimits::default(),
    )
    .expect("the ordinary function with its graph entry is accepted");
}

#[test]
fn replacing_a_module_block_with_a_graph_row_accepts_the_program() {
    let failure = crate::form_module_graph(
        SourceInput::new("modules.wfg", b"module geometry {\n}\n"),
        CompilerLimits::default(),
    )
    .expect_err("graph modules use rows, not blocks");
    assert!(
        failure.to_string().contains("mechanical_fix: write graph declarations as `package name = \"location\";`, then module rows `module_path: [dependency_path, ...];`, then entries `entry name = module_path;`; examples are `package util = \"../util\";`, `pkg::geometry: [];`, and `entry app = pkg::geometry::main;` [GRAM-2]"),
        "{failure}"
    );
    let graph = crate::form_module_graph(
        SourceInput::new(
            "modules.wfg",
            b"pkg::geometry: [];\n\nentry app = pkg::geometry::main;\n",
        ),
        CompilerLimits::default(),
    )
    .expect("the repair's module row and entry declaration form");
    let records = [
        ("geometry/module.wfm", INTERFACE),
        ("geometry/main.wf", BODY),
    ];
    let inputs = module_inputs(&graph, &records);
    crate::driver::check_module_entry(
        &graph,
        &inputs,
        crate::driver::ModuleEntry::Named("app"),
        CompilerLimits::default(),
    )
    .expect("the program registered by the repaired row is accepted");
}
