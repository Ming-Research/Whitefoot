use super::{
    BASE_BODY, BASE_INTERFACE, CacheDirectory, PROGRAM_GRAPH, ROOT_BODY, ROOT_INTERFACE, TOOL_BODY,
    TOOL_INTERFACE, USER_BODY, USER_INTERFACE, write_tree,
};
use crate::{CompilerLimits, OverlapLowering, SourceInput, driver};

/// The graph, its records and the invocation's inputs for the program whose
/// graph record is spelled `graph_path`, named by that spelling.
fn spelled_program(
    graph_path: &std::path::Path,
) -> (crate::ModuleGraph, Vec<crate::ModuleSourceFile>) {
    let graph = crate::form_module_program_graph(graph_path, CompilerLimits::default())
        .unwrap_or_else(|failure| panic!("the graph forms: {failure:?}"));
    let root = graph_path.parent().expect("the graph has a directory");
    let sources = crate::discover_module_sources(root, &graph).expect("the records read");
    (graph, sources)
}

/// `path` as a relative path from the working directory.
fn relative_to_working_directory(path: &std::path::Path) -> std::path::PathBuf {
    let depth = std::env::current_dir()
        .expect("the working directory")
        .components()
        .count()
        - 1;
    let mut relative = std::path::PathBuf::new();
    (0..depth).for_each(|_| relative.push(".."));
    relative.extend(path.components().skip(1));
    relative
}

/// The record `logical` names, spelled as the invocation reads it.
fn display_of(sources: &[crate::ModuleSourceFile], logical: &str) -> String {
    sources
        .iter()
        .find(|source| source.logical_path == logical)
        .map(|source| source.display_path.clone())
        .expect("the record is read")
}

/// [MOD-8, MOD-9] the host paths an invocation spells its records by are no
/// part of a record's identity: one graph built through a relative path, an
/// absolute path and an absolute path with a `..` component reuses the entry
/// build recorded by the first, emits the same module and recomputes no
/// analysis, while an edit to a record or to the graph still prevents reuse.
#[test]
fn an_entry_build_is_reused_across_spellings_of_the_same_paths() {
    let root = std::env::temp_dir().join(format!(
        "whitefoot-driver-spellings-{}-build",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let directory = CacheDirectory::new("spellings-build");
    let cache = directory.open();
    let text = |bytes: &[u8]| String::from_utf8(bytes.to_vec()).expect("text");
    write_tree(
        &root,
        &[
            ("app/modules.wfg", &text(PROGRAM_GRAPH)),
            ("app/base/module.wfm", &text(BASE_INTERFACE)),
            ("app/base/half.wf", &text(BASE_BODY)),
            ("app/user/module.wfm", &text(USER_INTERFACE)),
            ("app/user/use.wf", &text(USER_BODY)),
            ("app/tool/module.wfm", &text(TOOL_INTERFACE)),
            ("app/tool/spare.wf", &text(TOOL_BODY)),
            ("app/module.wfm", &text(ROOT_INTERFACE)),
            ("app/main.wf", &text(ROOT_BODY)),
        ],
    );
    let absolute = root.join("app/modules.wfg");
    let spellings = [
        relative_to_working_directory(&absolute),
        absolute.clone(),
        root.join("app/../app/modules.wfg"),
    ];
    let build = |graph_path: &std::path::Path| {
        let (graph, sources) = spelled_program(graph_path);
        let inputs = sources
            .iter()
            .map(|source| {
                SourceInput::from_host_path(
                    &source.logical_path,
                    &source.display_path,
                    &source.bytes,
                )
                .in_module(source.module, source.role)
            })
            .collect::<Vec<_>>();
        driver::build_module_entry(
            &graph,
            &inputs,
            driver::ModuleEntry::Named("app"),
            CompilerLimits::default(),
            OverlapLowering::Off,
            Some(&cache),
        )
        .map(|(module, reused)| (module.encode(), reused))
    };
    let displays = spellings
        .iter()
        .map(|graph_path| display_of(&spelled_program(graph_path).1, "base/half.wf"))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(displays.len(), 3, "each spelling names the record alone");
    let (first, reused) = build(&spellings[0]).expect("the entry builds");
    assert!(!reused);
    let counts = cache.receipt_counts();
    for spelling in &spellings[1..] {
        assert_eq!(
            build(spelling).expect("the entry builds"),
            (first.clone(), true)
        );
    }
    assert_eq!(cache.receipt_counts(), counts, "no front end ran again");
    let body = "fn half(value: u8) -> result: u8 pure contract {\n  requires value > 1_u8;\n} {\n  let result = value / 4_u8;\n  return result;\n}\n";
    write_tree(&root, &[("app/base/half.wf", body)]);
    let (edited, reused) = build(&spellings[2]).expect("the edited entry builds");
    assert!(!reused, "an edited record is a different input");
    assert_ne!(edited, first);
    assert_eq!(
        build(&spellings[0]).expect("the entry builds"),
        (edited, true)
    );
    let without_edge = "pkg::base: [];\npkg::user: [];\npkg::tool: [];\npkg: [pkg::base, pkg::user, std::fs, std::io, std::process, std::text];\n\nentry app = pkg::main;\n";
    write_tree(&root, &[("app/modules.wfg", without_edge)]);
    let refused = build(&spellings[1]).expect_err("the edited graph is checked, not reused");
    assert_eq!(refused.kind(), driver::CompilationFailureKind::Source);
    let _ = std::fs::remove_dir_all(&root);
}

/// [MOD-8, DIAG-1] a rejection prints the paths the invocation names its
/// records by, never the paths of the invocation that recorded it: the same
/// rejected graph checked through a relative path, an absolute path and an
/// absolute path with a `..` component reports each spelling's own paths, as a
/// run without a cache does, from module verdicts and from the entry's
/// composition alike.
#[test]
fn a_rejection_prints_the_paths_of_the_invocation_that_reads_it() {
    let root = std::env::temp_dir().join(format!(
        "whitefoot-driver-spellings-{}-reject",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let directory = CacheDirectory::new("spellings-reject");
    let cache = directory.open();
    let text = |bytes: &[u8]| String::from_utf8(bytes.to_vec()).expect("text");
    let broken = format!(
        "{}\npublic fn third(value: Missing) -> result: u8 pure doc \"Divides a value by three.\";\n",
        text(BASE_INTERFACE)
    );
    write_tree(
        &root,
        &[
            ("app/modules.wfg", &text(PROGRAM_GRAPH)),
            ("app/base/module.wfm", &broken),
            ("app/base/half.wf", &text(BASE_BODY)),
            ("app/user/module.wfm", &text(USER_INTERFACE)),
            ("app/user/use.wf", &text(USER_BODY)),
            ("app/tool/module.wfm", &text(TOOL_INTERFACE)),
            ("app/tool/spare.wf", &text(TOOL_BODY)),
            ("app/module.wfm", &text(ROOT_INTERFACE)),
            ("app/main.wf", &text(ROOT_BODY)),
        ],
    );
    let absolute = root.join("app/modules.wfg");
    let spellings = [
        relative_to_working_directory(&absolute),
        absolute.clone(),
        root.join("app/../app/modules.wfg"),
        root.join("app/../app/modules.wfg"),
    ];
    let failures = |graph_path: &std::path::Path, cache: Option<&driver::BuildCache>| {
        let (graph, sources) = spelled_program(graph_path);
        let inputs = sources
            .iter()
            .map(|source| {
                SourceInput::from_host_path(
                    &source.logical_path,
                    &source.display_path,
                    &source.bytes,
                )
                .in_module(source.module, source.role)
            })
            .collect::<Vec<_>>();
        let limits = CompilerLimits::default();
        let module = driver::module_verdict(&graph, &inputs, "pkg::base", false, limits, cache)
            .expect("a module verdict");
        let entry = driver::entry_verdict(
            &graph,
            &inputs,
            driver::ModuleEntry::Named("app"),
            limits,
            cache,
            &[],
        )
        .expect("a composition verdict");
        (display_of(&sources, "base/module.wfm"), [module, entry])
    };
    let mut seen = Vec::new();
    for (index, spelling) in spellings.iter().enumerate() {
        let (display, verdicts) = failures(spelling, Some(&cache));
        let (_, cold) = failures(spelling, None);
        for (verdict, cold) in verdicts.iter().zip(&cold) {
            assert_eq!(verdict.outcome(), cold.outcome(), "{spelling:?}");
            let driver::CheckOutcome::Rejected { failure, tasks, .. } = verdict.outcome() else {
                panic!("the broken interface is rejected: {verdict:?}");
            };
            assert!(failure.contains(&format!("{display}:")), "{failure}");
            assert!(
                tasks.iter().all(|task| task
                    .location()
                    .is_some_and(|location| location.path() == display)),
                "{tasks:?}"
            );
            for earlier in &seen {
                assert!(!failure.contains(&format!("{earlier}:")), "{failure}");
            }
        }
        if index == 3 {
            assert!(
                verdicts.iter().all(driver::CheckVerdict::reused),
                "the same spelling reuses its rejection"
            );
        }
        if !seen.contains(&display) {
            seen.push(display);
        }
    }
    let _ = std::fs::remove_dir_all(&root);
}
