use super::{CacheDirectory, module_inputs};
use crate::{CompilerLimits, OverlapLowering, SourceInput, driver};

/// A cached position names a qualified module rather than the graph row's
/// ordinal, so a reordered graph binds the same source without rerunning checks.
#[test]
fn reordering_graph_rows_preserves_a_waiting_entrys_source_bindings() {
    let directory = CacheDirectory::new("wait-site-graph-order");
    let cache = directory.open();
    let original =
        b"pkg::lib: [];\npkg: [pkg::lib, std::process];\npkg::peer: [];\n\nentry app = pkg::main;\n";
    let reordered =
        b"pkg::peer: [];\npkg::lib: [];\npkg: [std::process, pkg::lib];\n\nentry app = pkg::main;\n";
    let module_ids = |graph_bytes: &[u8]| {
        let graph = crate::form_module_graph(
            SourceInput::new("modules.wfg", graph_bytes),
            CompilerLimits::default(),
        )
        .expect("both dependency-first graph orders form");
        ["pkg::lib", "pkg"].map(|name| {
            graph
                .modules()
                .iter()
                .position(|module| module.qualified_name() == name)
                .and_then(crate::ModuleId::from_index)
                .expect("each graph registers the waiting library and entry")
        })
    };
    let original_ids = module_ids(original);
    let reordered_ids = module_ids(reordered);
    assert_ne!(original_ids[0], reordered_ids[0], "the waiting module moves");
    assert_ne!(original_ids[1], reordered_ids[1], "the entry module moves");
    let records: [(&str, &[u8]); 6] = [
        (
            "lib/module.wfm",
            b"public fn wait_value(cell: Shared<u64>) -> result: unit pure waits doc \"Waits on its value.\";\n",
        ),
        (
            "lib/wait.wf",
            b"fn wait_value(cell: Shared<u64>) -> result: unit pure waits {\n  atomic value = &cell when value^ != 0_u64 {\n    set value^ = 2_u64;\n  }\n  return unit;\n}\n",
        ),
        (
            "module.wfm",
            b"public fn main() -> status: std::process::ExitStatus pure waits doc \"Runs its waiting call.\";\n",
        ),
        (
            "main.wf",
            b"fn main() -> status: std::process::ExitStatus pure waits {\n  let cell = shared_new::<u64>(value: 0_u64);\n  pkg::lib::wait_value(cell: move cell);\n  return std::process::exit_status(code: 0_u8);\n}\n",
        ),
        (
            "peer/module.wfm",
            b"public fn spare() -> result: u8 pure doc \"Returns an independent value.\";\n",
        ),
        (
            "peer/spare.wf",
            b"fn spare() -> result: u8 pure {\n  return 0_u8;\n}\n",
        ),
    ];
    let build = |graph_bytes: &[u8], cached| {
        let graph = crate::form_module_graph(
            SourceInput::new("modules.wfg", graph_bytes),
            CompilerLimits::default(),
        )
        .expect("both graph row orders form");
        let inputs = module_inputs(&graph, &records);
        driver::build_module_entry(
            &graph,
            &inputs,
            driver::ModuleEntry::Named("app"),
            CompilerLimits::default(),
            OverlapLowering::Off,
            cached,
        )
        .expect("both row orders admit the waiting entry")
    };
    let (first, reused) = build(original, Some(&cache));
    assert!(!reused, "the first emission is cold");
    let counts = cache.receipt_counts();
    let (moved, reused) = build(reordered, Some(&cache));
    assert!(reused, "graph row order changes no emission input");
    assert_eq!(cache.receipt_counts(), counts, "the frontend does not rerun");
    assert_eq!(moved, first, "qualified source bindings are unchanged");
    assert_eq!(moved, build(reordered, None).0);
    let text = moved.into_string();
    let guard = "waits for the guard of the atomic statement at lib/wait.wf:2:3\0"
        .bytes()
        .map(|byte| format!("\\{byte:02X}"))
        .collect::<String>();
    assert!(text.contains(&guard), "{text}");
    assert!(text.contains("lib/wait.wf:2:3"), "the lock site is stable: {text}");
}

/// An admitted library may record why its executable caller was rejected.
/// That raw diagnostic constrains only this cached module's host paths.
#[test]
fn relocating_a_recorded_caller_rejection_rebuilds_its_nested_source_paths() {
    let directory = CacheDirectory::new("caller-rejection-paths");
    let cache = directory.open();
    let graph = crate::form_module_graph(
        SourceInput::new("modules.wfg", b"pkg: [];\n\nentry app = pkg::main;\n"),
        CompilerLimits::default(),
    )
    .expect("the graph forms");
    let records: [(&str, &[u8]); 2] = [
        (
            "module.wfm",
            b"public fn main() -> result: unit pure contract {\n  requires 1_u64 <= 0_u64;\n} doc \"Publishes an uninhabited requirement.\";\n",
        ),
        (
            "main.wf",
            b"fn main() -> result: unit pure contract {\n  requires 1_u64 <= 0_u64;\n} {\n  return unit;\n}\n",
        ),
    ];
    let build = |prefix: &str, cached| {
        let displays = records
            .iter()
            .map(|(path, _)| format!("{prefix}/{path}"))
            .collect::<Vec<_>>();
        let inputs = module_inputs(&graph, &records)
            .into_iter()
            .zip(&displays)
            .map(|(input, display)| {
                SourceInput::from_host_path(input.logical_path(), display, input.bytes())
                    .in_module(input.module(), input.role())
            })
            .collect::<Vec<_>>();
        driver::build_module_entry(
            &graph,
            &inputs,
            driver::ModuleEntry::Named("app"),
            CompilerLimits::default(),
            OverlapLowering::Off,
            cached,
        )
        .expect("the uninhabited declaration still builds as a library")
    };
    let (first, reused) = build("old", Some(&cache));
    assert!(!reused, "the first module is cold");
    assert!(first.contains("Executable caller was not admitted"));
    assert!(!first.contains("define i32 @main("));
    let (warm, reused) = build("old", Some(&cache));
    assert!(reused, "unchanged diagnostic paths reuse this module");
    assert_eq!(warm, first);
    let before = cache.receipt_counts();
    let (relocated, reused) = build("new", Some(&cache));
    let after = cache.receipt_counts();
    assert!(!reused, "the recorded diagnostic needs current source paths");
    assert!(after.0 > before.0, "rebuilding reuses admitted function proofs");
    assert_eq!(after.1, before.1, "relocation records no new admitted proof");
    assert_eq!(relocated, build("new", None).0);
    let text = relocated.into_string();
    assert!(text.contains("Executable caller was not admitted"), "{text}");
    assert!(text.contains(r#""requires_clause":{"at":{"file":"new/"#), "{text}");
    assert!(!text.contains("old/"), "{text}");
    let (_, reused) = build("new", Some(&cache));
    assert!(reused, "the rebuilt diagnostic then reuses its own reading");
}
