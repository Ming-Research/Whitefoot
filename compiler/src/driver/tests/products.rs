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
    let graph = crate::form_module_graph(
        SourceInput::new("modules.wfg", graph.as_bytes()),
        CompilerLimits::default(),
    )
    .unwrap();
    let records = [
        ("lib/module.wfm", interface.as_bytes()),
        ("lib/body.wf", library.as_bytes()),
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
