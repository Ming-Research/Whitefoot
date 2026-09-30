//! Retained-product invalidation and recovery, using the ordinary uncached
//! pipeline as the independent compiler oracle. Keep these with the driver
//! boundary they exercise; remove them if retained products are retired.

use super::*;

const GRAPH: &str = "pkg::lib: [];\npkg: [pkg::lib, std::process];\n\nentry app = pkg::main;\n";

fn build(
    graph: &str,
    interface: &str,
    library: &str,
    entry: &str,
    cache: Option<&super::super::BuildCache>,
) -> Result<crate::LlvmModule, String> {
    build_with_overlap(
        graph,
        interface,
        library,
        entry,
        cache,
        OverlapLowering::Off,
    )
}

fn build_with_overlap(
    graph: &str,
    interface: &str,
    library: &str,
    entry: &str,
    cache: Option<&super::super::BuildCache>,
    overlap: OverlapLowering,
) -> Result<crate::LlvmModule, String> {
    let graph = crate::form_module_graph(
        SourceInput::new("modules.wfg", graph.as_bytes()),
        CompilerLimits::default(),
    )
    .unwrap();
    let root_interface = if entry.contains("ExitStatus pure waits") {
        std::str::from_utf8(ROOT_INTERFACE)
            .unwrap()
            .replace("pure doc", "pure waits doc")
    } else {
        std::str::from_utf8(ROOT_INTERFACE).unwrap().to_owned()
    };
    let records = [
        ("lib/module.wfm", interface.as_bytes()),
        ("lib/body.wf", library.as_bytes()),
        ("module.wfm", root_interface.as_bytes()),
        ("main.wf", entry.as_bytes()),
    ];
    let inputs = module_inputs(&graph, &records);
    match cache {
        Some(cache) => super::super::build_module_entry(
            &graph,
            &inputs,
            super::super::ModuleEntry::Named("app"),
            CompilerLimits::default(),
            overlap,
            Some(cache),
        )
        .map(|built| built.0),
        None => fresh_product_entry(&graph, &inputs, overlap),
    }
    .map_err(|failure| failure.to_string())
}

#[test]
fn entry_edits_reuse_segments_and_waiting_shared_products() {
    let cases = [
        (
            "segments",
            "",
            "fn value() -> result: u8 pure {\n  let lengths = box_array_filled::<u64>(count: 1_u64, value: 2_u64);\n  let made = box_segments_filled::<u64>(lengths: &lengths.inner[0_u64..1_u64], value: 0_u64);\n  match move made {\n    None() => {\n      return 1_u8;\n    }\n    Some(value: segments) => {\n      let length = segments.inner.len;\n      if length != 1_u64 {\n        return 3_u8;\n      }\n      let part = &segments.inner[0_u64];\n      let count = part^.len;\n      if count == 2_u64 {\n        return 0_u8;\n      }\n      return 2_u8;\n    }\n  }\n}\n",
        ),
        (
            "waiting-shared",
            " waits",
            "fn produce(cell: Shared<u8>) -> result: unit pure waits {\n  atomic state = &cell {\n    set state^ = 0_u8;\n  }\n  return unit;\n}\n\nfn value() -> result: u8 pure waits {\n  let cell = shared_new::<u8>(value: 0_u8);\n  let handle = shared_share::<u8>(shared: &cell);\n  spawn produce(cell: move handle);\n  let seen = 0_u8;\n  atomic state = &cell {\n    set seen = state^;\n  }\n  return seen;\n}\n",
        ),
    ];
    for (name, waits, library) in cases {
        let directory = CacheDirectory::new(name);
        let interface =
            format!("public fn value() -> result: u8 pure{waits} doc \"Returns a value.\";\n");
        for local in ["before", "after"] {
            let entry = format!(
                "fn main() -> status: std::process::ExitStatus pure{waits} {{\n  let {local} = pkg::lib::value();\n  return std::process::exit_status(code: {local});\n}}\n"
            );
            let cache = directory.open();
            let cached = build(GRAPH, &interface, library, &entry, Some(&cache)).unwrap();
            assert_eq!(
                cached,
                build(GRAPH, &interface, library, &entry, None).unwrap()
            );
            if local == "after" {
                let (_, checked, reused) = cache
                    .body_module_counts()
                    .into_iter()
                    .find(|(module, _, _)| module == "pkg::lib")
                    .unwrap();
                assert_eq!(
                    checked, 0,
                    "{name}: unchanged library bodies must not be walked"
                );
                assert!(reused > 0, "{name}: structural products must be imported");
                let (_, lowered, reused) = cache
                    .lowering_counts()
                    .into_iter()
                    .find(|(module, _, _)| module == "pkg::lib")
                    .unwrap();
                assert_eq!(
                    lowered, 0,
                    "{name}: unchanged library lowering must not be walked"
                );
                assert!(reused > 0, "{name}: typed lowerings must be imported");
            }
        }
    }
}

#[test]
fn products_follow_body_representation_heap_and_dependency_edits() {
    let directory = CacheDirectory::new("product-input-matrix");
    let signature = "fn make() -> result: Cell pure";
    let interface = |width| {
        format!(
            "public struct Cell {{\n  doc \"A public byte with private storage.\";\n  public value: u8;\n  hidden: Array<u64, {width}>;\n}}\n\npublic {signature} doc \"Makes a cell.\";\n"
        )
    };
    let body = |width, value, heap| {
        format!(
            "{signature} {{\n{}  let hidden = array_filled::<u64, {width}>(value: 0_u64);\n  return Cell(value: {value}_u8, hidden: hidden);\n}}\n",
            if heap {
                "  let owned = box_new::<u64>(value: 0_u64);\n"
            } else {
                ""
            },
        )
    };
    let entry = "fn main() -> status: std::process::ExitStatus pure {\n  let cell = pkg::lib::make();\n  return std::process::exit_status(code: cell.value);\n}\n";
    let initial = build(
        GRAPH,
        &interface(1),
        &body(1, 0, false),
        entry,
        Some(&directory.open()),
    )
    .unwrap();
    for (width, value, heap) in [(1, 1, false), (4, 1, false), (4, 1, true)] {
        let cached = build(
            GRAPH,
            &interface(width),
            &body(width, value, heap),
            entry,
            Some(&directory.open()),
        )
        .unwrap();
        assert_eq!(
            cached,
            build(
                GRAPH,
                &interface(width),
                &body(width, value, heap),
                entry,
                None
            )
            .unwrap()
        );
        assert_ne!(cached, initial, "each edit changes the emitted program");
    }
    let no_heap = GRAPH.replace(
        "entry app = pkg::main;",
        "entry app = pkg::main {\n  no_heap;\n}",
    );
    let cached = build(
        &no_heap,
        &interface(4),
        &body(4, 1, true),
        entry,
        Some(&directory.open()),
    );
    assert!(
        cached
            .as_ref()
            .is_err_and(|failure| failure.contains("[STOR-8]")),
        "{cached:?}"
    );
    assert_eq!(
        cached,
        build(&no_heap, &interface(4), &body(4, 1, true), entry, None)
    );

    let private_value = interface(4).replace("public value:", "value:");
    let cached = build(
        GRAPH,
        &private_value,
        &body(4, 1, false),
        entry,
        Some(&directory.open()),
    );
    assert!(
        cached
            .as_ref()
            .is_err_and(|failure| failure.contains("[MOD-5]")),
        "{cached:?}"
    );
    assert_eq!(
        cached,
        build(GRAPH, &private_value, &body(4, 1, false), entry, None)
    );

    let missing_edge = GRAPH.replace("pkg::lib, std", "std");
    let cached = build(
        &missing_edge,
        &interface(4),
        &body(4, 1, false),
        entry,
        Some(&directory.open()),
    );
    assert!(
        cached
            .as_ref()
            .is_err_and(|failure| failure.contains("[MOD-5]")),
        "{cached:?}"
    );
    assert_eq!(
        cached,
        build(
            &missing_edge,
            &interface(4),
            &body(4, 1, false),
            entry,
            None
        )
    );

    let aliased = format!(
        "alias library = pkg::lib;\n\n{}",
        entry.replace("pkg::lib::make", "library::make")
    );
    assert_eq!(
        build(
            GRAPH,
            &interface(1),
            &body(1, 0, false),
            &aliased,
            Some(&directory.open())
        )
        .unwrap(),
        initial
    );
}

#[test]
fn failed_compositions_and_damaged_products_preserve_independent_library_work() {
    let directory = CacheDirectory::new("product-recovery");
    let interface = "public fn value() -> result: u8 pure doc \"Returns a value.\";\n";
    let library = "fn value() -> result: u8 pure {\n  return 0_u8;\n}\n";
    let entry = |local: &str| {
        format!(
            "fn main() -> status: std::process::ExitStatus pure {{\n  let {local} = pkg::lib::value();\n  return std::process::exit_status(code: {local});\n}}\n"
        )
    };
    build(
        GRAPH,
        interface,
        library,
        &entry("first"),
        Some(&directory.open()),
    )
    .unwrap();
    let rejected = entry("second").replace("code: second", "code: true");
    let cached = build(
        GRAPH,
        interface,
        library,
        &rejected,
        Some(&directory.open()),
    );
    assert!(cached.is_err());
    assert_eq!(cached, build(GRAPH, interface, library, &rejected, None));
    let cache = directory.open();
    assert_eq!(
        build(GRAPH, interface, library, &entry("third"), Some(&cache)).unwrap(),
        build(GRAPH, interface, library, &entry("third"), None).unwrap()
    );
    let (_, checked, reused) = cache
        .body_module_counts()
        .into_iter()
        .find(|(name, _, _)| name == "pkg::lib")
        .unwrap();
    assert_eq!(
        checked, 0,
        "a failed entry must retain independent completed library bodies"
    );
    assert!(reused > 0);
    for family in ["module-bodies", "lowered-functions"] {
        let area = directory.0.join(family);
        let records = std::fs::read_dir(&area)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(
            !records.is_empty(),
            "the damaged family must have been populated"
        );
        for record in records {
            std::fs::write(record.path(), b"incompatible retained product").unwrap();
        }
    }
    let cache = directory.open();
    assert_eq!(
        build(GRAPH, interface, library, &entry("fourth"), Some(&cache)).unwrap(),
        build(GRAPH, interface, library, &entry("fourth"), None).unwrap()
    );
    assert!(
        cache.body_counts().0 > 0,
        "damaged bodies must be recomputed"
    );
    assert!(
        cache
            .lowering_counts()
            .iter()
            .any(|(_, lowered, _)| *lowered > 0),
        "damaged fragments must be lowered again"
    );
}

/// MOD-8 already includes an edge to an imported generic's actual even when
/// its implementation does not call it. Change the actual's own body to close
/// the component; changing only the imported body cannot create this witness.
#[test]
fn callback_actual_body_edits_keep_interface_proof_components_current() {
    let directory = CacheDirectory::new("product-callback-component");
    let signature = "fn invoke<fn step(value: u8) -> result: u8 pure>(value: u8) -> result: u8 pure contract {\n  ensures result <= 7_u8;\n}";
    let interface = format!("public {signature} doc \"Produces a bounded value.\";\n");
    let library = format!(
        "{signature} {{\n  let answer = step(value: value);\n  if answer <= 7_u8 {{\n    return answer;\n  }}\n  return 0_u8;\n}}\n"
    );
    let source = |recursive| {
        format!(
            "fn bounce(value: u8) -> result: u8 pure contract {{\n  ensures result <= 7_u8;\n}} {{\n{}}}\n\nfn main() -> status: std::process::ExitStatus pure {{\n  let code = pkg::lib::invoke::<fn bounce>(value: 3_u8);\n  return std::process::exit_status(code: code);\n}}\n",
            if recursive {
                "  let answer = pkg::lib::invoke::<fn bounce>(value: value);\n  return answer;\n"
            } else {
                "  return 0_u8;\n"
            },
        )
    };
    let original = build(
        GRAPH,
        &interface,
        &library,
        &source(false),
        Some(&directory.open()),
    )
    .unwrap();
    assert_eq!(
        original,
        build(GRAPH, &interface, &library, &source(false), None).unwrap()
    );
    let cached = build(
        GRAPH,
        &interface,
        &library,
        &source(true),
        Some(&directory.open()),
    );
    assert!(
        cached
            .as_ref()
            .is_err_and(|failure| failure.contains("[FN-9]")),
        "the callback component must withhold its own summary: {cached:?}"
    );
    assert_eq!(
        cached,
        build(GRAPH, &interface, &library, &source(true), None)
    );
    assert_eq!(
        build(
            GRAPH,
            &interface,
            &library,
            &source(false),
            Some(&directory.open())
        )
        .unwrap(),
        original
    );
}

#[test]
fn retained_callback_calls_use_the_actuals_current_heap_closure() {
    let directory = CacheDirectory::new("product-callback-heap");
    let graph = GRAPH.replace(
        "entry app = pkg::main;",
        "entry app = pkg::main {\n  no_heap;\n}",
    );
    let signature = "fn invoke<fn step() -> result: u8 pure>() -> result: u8 pure";
    let interface = format!("public {signature} doc \"Calls the supplied function.\";\n");
    let library = format!("{signature} {{\n  let result = step();\n  return result;\n}}\n");
    let source = |heap| {
        format!(
            "fn action() -> result: u8 pure {{\n{}  return 0_u8;\n}}\n\nfn main() -> status: std::process::ExitStatus pure {{\n  let code = pkg::lib::invoke::<fn action>();\n  return std::process::exit_status(code: code);\n}}\n",
            if heap {
                "  let owned = box_new::<u64>(value: 0_u64);\n"
            } else {
                ""
            },
        )
    };
    build(
        &graph,
        &interface,
        &library,
        &source(false),
        Some(&directory.open()),
    )
    .unwrap();
    let cache = directory.open();
    let cached = build(&graph, &interface, &library, &source(true), Some(&cache));
    assert!(
        cached
            .as_ref()
            .is_err_and(|failure| failure.contains("[STOR-8]")),
        "{cached:?}"
    );
    assert_eq!(
        cached,
        build(&graph, &interface, &library, &source(true), None)
    );
    let (_, checked, reused) = cache
        .body_module_counts()
        .into_iter()
        .find(|(module, _, _)| module == "pkg::lib")
        .unwrap();
    assert_eq!(checked, 0, "the unchanged callback body should import");
    assert!(
        reused > 0,
        "the heap rejection must follow a retained callback call"
    );
}

#[test]
fn private_capture_layout_changes_rejudge_retained_parallel_fragments() {
    let directory = CacheDirectory::new("product-capture-layout");
    let interface = |width| {
        format!(
            "public struct Cell {{\n  doc \"A value with private inline storage.\";\n  public value: u8;\n  hidden: Array<u64, {width}>;\n}}\n\npublic fn make() -> result: Cell pure doc \"Makes a cell.\";\n"
        )
    };
    let library = |width| {
        format!(
            "fn make() -> result: Cell pure {{\n  let hidden = array_filled::<u64, {width}>(value: 0_u64);\n  return Cell(value: 1_u8, hidden: hidden);\n}}\n"
        )
    };
    let entry = "fn read(value: pkg::lib::Cell) -> result: u8 pure {\n  return value.value;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  let cell = pkg::lib::make();\n  let sum = 0_u8;\n  for @fold (index in 0_u64..8_u64) {\n    let value = read(value: cell);\n    set sum = sum +wrap value;\n  }\n  return std::process::exit_status(code: sum);\n}\n";
    let small = build_with_overlap(
        GRAPH,
        &interface(1),
        &library(1),
        entry,
        Some(&directory.open()),
        OverlapLowering::On,
    )
    .unwrap();
    assert!(
        small
            .model
            .entities
            .iter()
            .any(|entity| entity.name.ends_with("main.0")),
        "the small inline capture must exercise actual loop synthesis"
    );
    let large = build_with_overlap(
        GRAPH,
        &interface(64),
        &library(64),
        entry,
        Some(&directory.open()),
        OverlapLowering::On,
    )
    .unwrap();
    assert!(
        !large
            .model
            .entities
            .iter()
            .any(|entity| entity.name.ends_with("main.0")),
        "the enlarged private capture must exceed the task frame and decline synthesis"
    );
    assert_eq!(
        large,
        build_with_overlap(
            GRAPH,
            &interface(64),
            &library(64),
            entry,
            None,
            OverlapLowering::On
        )
        .unwrap()
    );
}

#[test]
fn an_indirect_return_type_invariant_edit_invalidates_a_retained_caller() {
    let directory = CacheDirectory::new("product-indirect-type-invariant");
    let graph = crate::form_module_graph(
        SourceInput::new("modules.wfg", PROGRAM_GRAPH),
        CompilerLimits::default(),
    )
    .unwrap();
    let body = b"fn make() -> result: Cell pure {\n  return Cell(value: 0_u8);\n}\n";
    let user = b"fn use_half() -> result: u8 pure {\n  let cell = pkg::base::make();\n  let value = cell.value;\n  return 7_u8 - value;\n}\n";
    let interface = |bound| {
        format!(
            "public struct Cell {{\n  public readonly value: u8;\n  invariant bounded(cell): cell.value <= {bound}_u8;\n}}\n\npublic fn make() -> result: Cell pure doc \"Makes a cell.\";\n"
        )
    };
    let run = |bound, edited: bool, cache: Option<&super::super::BuildCache>| {
        let interface = interface(bound);
        let entry = if edited {
            std::str::from_utf8(ROOT_BODY)
                .unwrap()
                .replace("let code =", "let answer =")
                .replace("code: code", "code: answer")
        } else {
            std::str::from_utf8(ROOT_BODY).unwrap().to_owned()
        };
        let records = [
            ("base/module.wfm", interface.as_bytes()),
            ("base/body.wf", body.as_slice()),
            ("user/module.wfm", USER_INTERFACE),
            ("user/body.wf", user.as_slice()),
            ("tool/module.wfm", TOOL_INTERFACE),
            ("tool/body.wf", TOOL_BODY),
            ("module.wfm", ROOT_INTERFACE),
            ("main.wf", entry.as_bytes()),
        ];
        let inputs = module_inputs(&graph, &records);
        match cache {
            Some(cache) => super::super::build_module_entry(
                &graph,
                &inputs,
                super::super::ModuleEntry::Named("app"),
                CompilerLimits::default(),
                OverlapLowering::Off,
                Some(cache),
            )
            .map(|built| built.0),
            None => fresh_product_entry(&graph, &inputs, OverlapLowering::Off),
        }
        .map_err(|failure| failure.to_string())
    };
    let original = run(7, false, Some(&directory.open())).unwrap();
    assert_eq!(original, run(7, false, None).unwrap());
    let reused = directory.open();
    assert_eq!(run(7, true, Some(&reused)).unwrap(), original);
    assert!(
        reused
            .body_module_counts()
            .iter()
            .any(|(module, _, reused)| module == "pkg::user" && *reused > 0)
    );
    let changed = run(9, true, Some(&directory.open()));
    assert!(
        changed
            .as_ref()
            .is_err_and(|failure| failure.contains("[OP-2]")),
        "{changed:?}"
    );
    assert_eq!(changed, run(9, true, None));
    assert_eq!(run(7, true, Some(&directory.open())).unwrap(), original);
}
