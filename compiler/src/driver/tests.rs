use super::{
    CompilationFailureKind, CompilationStage, CompilerLimits, check, compile,
    compile_with_permission_ledger,
};
use crate::{OverlapLowering, RecursionBudget, SourceInput};

/// Places each record in the module its directory names, in the
/// interface role when it is that directory's `module.wfm` [MOD-2].
fn module_inputs<'a>(
    graph: &crate::ModuleGraph,
    records: &'a [(&'a str, &'a [u8])],
) -> Vec<SourceInput<'a>> {
    records
        .iter()
        .map(|(path, bytes)| {
            let (directory, file) = path.rsplit_once('/').unwrap_or(("", path));
            let components: Vec<&str> = if directory.is_empty() {
                Vec::new()
            } else {
                directory.split('/').collect()
            };
            let module = graph
                .modules()
                .iter()
                .position(|module| module.path() == components.as_slice())
                .and_then(crate::ModuleId::from_index)
                .expect("every record lies in a registered module");
            let role = if file == "module.wfm" {
                crate::SourceRole::Interface
            } else {
                crate::SourceRole::Implementation
            };
            SourceInput::new(path, bytes).in_module(module, role)
        })
        .collect()
}

/// A fresh cache directory for one test, removed by the returned guard.
struct CacheDirectory(std::path::PathBuf);

impl CacheDirectory {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "whitefoot-driver-cache-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        Self(path)
    }

    fn open(&self) -> super::BuildCache {
        super::BuildCache::open(&self.0, [7; 32]).expect("the cache opens")
    }
}

impl Drop for CacheDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A three-module program: `pkg::base` publishes a function with a
/// requirement, `pkg::user` calls it, and `pkg::tool` is independent.
const PROGRAM_GRAPH: &[u8] =
        b"pkg::base: [];\npkg::user: [pkg::base];\npkg::tool: [];\npkg: [pkg::base, pkg::user, std::fs, std::io, std::process, std::text];\n\nentry app = pkg::main;\n";
const BASE_INTERFACE: &[u8] = b"public fn half(value: u8) -> result: u8 pure contract {\n  requires value > 1_u8;\n} doc \"Halves a value above one.\";\n";
const BASE_BODY: &[u8] = b"fn half(value: u8) -> result: u8 pure contract {\n  requires value > 1_u8;\n} {\n  let result = value / 2_u8;\n  return result;\n}\n";
const USER_INTERFACE: &[u8] = b"public fn use_half() -> result: u8 pure doc \"Halves eight.\";\n";
const USER_BODY: &[u8] = b"fn use_half() -> result: u8 pure {\n  let result = pkg::base::half(value: 8_u8);\n  return result;\n}\n";
const TOOL_INTERFACE: &[u8] =
    b"public fn spare() -> result: u8 pure doc \"Supplies a spare value.\";\n";
const TOOL_BODY: &[u8] = b"fn spare() -> result: u8 pure {\n  return 1_u8;\n}\n";
const ROOT_INTERFACE: &[u8] =
    b"public fn main() -> status: std::process::ExitStatus pure doc \"Runs the program.\";\n";
const ROOT_BODY: &[u8] = b"fn main() -> status: std::process::ExitStatus pure {\n  let code = pkg::user::use_half();\n  return std::process::exit_status(code: code);\n}\n";

/// Every module's verdict and every entry's composition verdict, with
/// and without the cache; the two must agree apart from reuse.
fn verdicts(
    graph_bytes: &[u8],
    records: &[(&str, &[u8])],
    cache: Option<&super::BuildCache>,
) -> Vec<(String, super::CheckOutcome, bool)> {
    let graph = crate::form_module_graph(
        SourceInput::new("modules.wfg", graph_bytes),
        CompilerLimits::default(),
    )
    .expect("the graph forms");
    let inputs = module_inputs(&graph, records);
    let mut verdicts = Vec::new();
    // [MOD-10] the program's own modules, as the command line lists them.
    for record in graph
        .modules()
        .iter()
        .filter(|record| record.package() == crate::Package::Program)
    {
        let verdict = super::module_verdict(
            &graph,
            &inputs,
            &record.qualified_name(),
            false,
            CompilerLimits::default(),
            cache,
        )
        .expect("a module verdict");
        verdicts.push((
            verdict.subject().to_owned(),
            verdict.outcome().clone(),
            verdict.reused(),
        ));
    }
    for entry in graph.entries() {
        let verdict = super::entry_verdict(
            &graph,
            &inputs,
            super::ModuleEntry::Named(entry.name()),
            CompilerLimits::default(),
            cache,
            &[],
        )
        .expect("a composition verdict");
        verdicts.push((
            verdict.subject().to_owned(),
            verdict.outcome().clone(),
            verdict.reused(),
        ));
    }
    verdicts
}

/// The subjects a cached run recomputed, after asserting that its
/// verdicts equal a run without any cache.
fn recomputed(
    graph_bytes: &[u8],
    records: &[(&str, &[u8])],
    cache: &super::BuildCache,
) -> Vec<String> {
    let cached = verdicts(graph_bytes, records, Some(cache));
    let cold = verdicts(graph_bytes, records, None);
    assert_eq!(
        cached
            .iter()
            .map(|(subject, outcome, _)| (subject, outcome))
            .collect::<Vec<_>>(),
        cold.iter()
            .map(|(subject, outcome, _)| (subject, outcome))
            .collect::<Vec<_>>(),
        "a cached run reports what a run without a cache reports"
    );
    assert!(cold.iter().all(|(_, _, reused)| !reused));
    cached
        .into_iter()
        .filter(|(_, _, reused)| !reused)
        .map(|(subject, _, _)| subject)
        .collect()
}

/// [MOD-8] a recorded verdict is reused exactly while every input its
/// check read is unchanged: an implementation edit recomputes its own
/// module and the compositions that contain it, an interface edit also
/// recomputes its dependents, an edit outside a composition recomputes
/// nothing in it, and every cached run reports what a cold run does,
/// including after an interface edit that breaks a client.
#[test]
fn a_recorded_verdict_is_reused_exactly_while_its_inputs_are_unchanged() {
    let directory = CacheDirectory::new("reuse");
    let cache = directory.open();
    let records = |base_interface: &'static [u8],
                   base_body: &'static [u8],
                   tool_body: &'static [u8]|
     -> Vec<(&'static str, &'static [u8])> {
        vec![
            ("base/module.wfm", base_interface),
            ("base/half.wf", base_body),
            ("user/module.wfm", USER_INTERFACE),
            ("user/use.wf", USER_BODY),
            ("tool/module.wfm", TOOL_INTERFACE),
            ("tool/spare.wf", tool_body),
            ("module.wfm", ROOT_INTERFACE),
            ("main.wf", ROOT_BODY),
        ]
    };
    let all = ["pkg::base", "pkg::user", "pkg::tool", "pkg", "app"]
        .map(str::to_owned)
        .to_vec();
    let original = records(BASE_INTERFACE, BASE_BODY, TOOL_BODY);
    assert_eq!(recomputed(PROGRAM_GRAPH, &original, &cache), all);
    assert_eq!(
        recomputed(PROGRAM_GRAPH, &original, &cache),
        Vec::<String>::new()
    );
    // A body edit that keeps the interface: only its module and the
    // compositions containing it.
    let body = b"fn half(value: u8) -> result: u8 pure contract {\n  requires value > 1_u8;\n} {\n  let result = value / 4_u8;\n  return result;\n}\n";
    assert_eq!(
        recomputed(
            PROGRAM_GRAPH,
            &records(BASE_INTERFACE, body, TOOL_BODY),
            &cache
        ),
        ["pkg::base", "app"].map(str::to_owned).to_vec()
    );
    // The tool is no part of the entry's composition.
    let tool = b"fn spare() -> result: u8 pure {\n  return 2_u8;\n}\n";
    assert_eq!(
        recomputed(
            PROGRAM_GRAPH,
            &records(BASE_INTERFACE, BASE_BODY, tool),
            &cache
        ),
        ["pkg::tool"].map(str::to_owned).to_vec()
    );
    // An interface edit that its client's call no longer satisfies: the
    // implementation fails correspondence, the client fails its call,
    // and the cached report equals the cold one. The root module reads
    // `pkg::base`'s interface only to hold its judgments, never `half`,
    // so its verdict stands [MOD-8].
    let interface = b"public fn half(value: u8) -> result: u8 pure contract {\n  requires value > 9_u8;\n} doc \"Halves a value above nine.\";\n";
    let edited = records(interface, BASE_BODY, TOOL_BODY);
    assert_eq!(
        recomputed(PROGRAM_GRAPH, &edited, &cache),
        ["pkg::base", "pkg::user", "app"]
            .map(str::to_owned)
            .to_vec()
    );
    let rules = verdicts(PROGRAM_GRAPH, &edited, Some(&cache))
        .into_iter()
        .map(|(subject, outcome, _)| {
            let rule = match outcome {
                super::CheckOutcome::Rejected { rule, .. } => rule,
                super::CheckOutcome::Accepted { .. } => None,
            };
            (subject, rule)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        rules,
        vec![
            ("pkg::base".to_owned(), Some("MOD-7".to_owned())),
            ("pkg::user".to_owned(), Some("FN-8".to_owned())),
            ("pkg::tool".to_owned(), None),
            ("pkg".to_owned(), None),
            ("app".to_owned(), Some("MOD-7".to_owned())),
        ]
    );
    // Reverting restores every earlier record without recomputing.
    assert_eq!(
        recomputed(PROGRAM_GRAPH, &original, &cache),
        Vec::<String>::new()
    );
}

/// [MOD-8] a composition acceptance stands only while every interface
/// record means what it meant: a redeclaration written into an interface
/// [TYPE-6] is rejected with the cache as without it, whether the new
/// declaration repeats the old one or precedes it with other text.
#[test]
fn an_interface_redeclaration_is_rejected_with_the_cache_as_without_it() {
    let directory = CacheDirectory::new("redeclaration");
    let cache = directory.open();
    let records = |base_interface: &'static [u8]| -> Vec<(&'static str, &'static [u8])> {
        vec![
            ("base/module.wfm", base_interface),
            ("base/half.wf", BASE_BODY),
            ("user/module.wfm", USER_INTERFACE),
            ("user/use.wf", USER_BODY),
            ("tool/module.wfm", TOOL_INTERFACE),
            ("tool/spare.wf", TOOL_BODY),
            ("module.wfm", ROOT_INTERFACE),
            ("main.wf", ROOT_BODY),
        ]
    };
    let original = records(BASE_INTERFACE);
    recomputed(PROGRAM_GRAPH, &original, &cache);
    let repeated: &'static [u8] = [BASE_INTERFACE, b"\n", BASE_INTERFACE].concat().leak();
    let preceded: &'static [u8] = [
        b"public fn half(value: u8) -> result: u8 pure doc \"Halves any value.\";\n\n".as_slice(),
        BASE_INTERFACE,
    ]
    .concat()
    .leak();
    for interface in [repeated, preceded] {
        let edited = records(interface);
        let rule =
            verdicts(PROGRAM_GRAPH, &edited, None)
                .into_iter()
                .find_map(|(subject, outcome, _)| match outcome {
                    super::CheckOutcome::Rejected { rule, .. } if subject == "app" => rule,
                    _ => None,
                });
        assert_eq!(rule.as_deref(), Some("TYPE-6"));
        recomputed(PROGRAM_GRAPH, &edited, &cache);
    }
}

/// [MOD-8] an interface record that declares nothing still enters a
/// composition's acceptance by what it writes: an alias edit that breaks the
/// record is rejected with the cache as without it.
#[test]
fn an_alias_edit_in_a_declaration_free_interface_is_rejected_with_the_cache_as_without_it() {
    let directory = CacheDirectory::new("alias-only");
    let cache = directory.open();
    let graph: &[u8] = b"pkg::base: [];\npkg::user: [pkg::base];\npkg::names: [pkg::base];\npkg: [pkg::base, pkg::user, pkg::names, std::fs, std::io, std::process, std::text];\n\nentry app = pkg::main;\n";
    let records = |names: &'static [u8]| -> Vec<(&'static str, &'static [u8])> {
        vec![
            ("base/module.wfm", BASE_INTERFACE),
            ("base/half.wf", BASE_BODY),
            ("user/module.wfm", USER_INTERFACE),
            ("user/use.wf", USER_BODY),
            ("names/module.wfm", names),
            ("module.wfm", ROOT_INTERFACE),
            ("main.wf", ROOT_BODY),
        ]
    };
    let app = |records: &[(&str, &[u8])]| {
        verdicts(graph, records, None)
            .into_iter()
            .find(|(subject, _, _)| subject == "app")
            .map(|(_, outcome, _)| outcome)
            .expect("the entry's verdict")
    };
    let original = records(b"alias half = pkg::base::half;\n");
    assert!(matches!(
        app(&original),
        super::CheckOutcome::Accepted { .. }
    ));
    recomputed(graph, &original, &cache);
    let edited = records(b"alias half = pkg::base::missing;\n");
    assert!(matches!(app(&edited), super::CheckOutcome::Rejected { .. }));
    recomputed(graph, &edited, &cache);
}

/// [MOD-8] a verdict reads of another module's interface that it holds
/// its judgments and the declarations its check reached, nothing more:
/// a reworded `doc` string or a declaration no importer reaches recomputes
/// only the edited module, while a defect in any declaration of an
/// interface an importer reads recomputes and rejects the importer, as a
/// check without a cache does.
#[test]
fn an_importer_reads_only_what_its_check_reached_of_an_interface() {
    let directory = CacheDirectory::new("reads");
    let cache = directory.open();
    let records = |base_interface: &'static [u8],
                   base_body: &'static [u8]|
     -> Vec<(&'static str, &'static [u8])> {
        vec![
            ("base/module.wfm", base_interface),
            ("base/half.wf", base_body),
            ("user/module.wfm", USER_INTERFACE),
            ("user/use.wf", USER_BODY),
            ("tool/module.wfm", TOOL_INTERFACE),
            ("tool/spare.wf", TOOL_BODY),
            ("module.wfm", ROOT_INTERFACE),
            ("main.wf", ROOT_BODY),
        ]
    };
    let _ = recomputed(PROGRAM_GRAPH, &records(BASE_INTERFACE, BASE_BODY), &cache);
    // A reworded documentation string: only the edited module.
    let reworded = b"public fn half(value: u8) -> result: u8 pure contract {\n  requires value > 1_u8;\n} doc \"Divides a value above one by two.\";\n";
    assert_eq!(
        recomputed(PROGRAM_GRAPH, &records(reworded, BASE_BODY), &cache),
        ["pkg::base"].map(str::to_owned).to_vec()
    );
    // A declaration no importer reaches, with its definition: the edited
    // module and the compositions whose implementation records changed.
    let widened = b"public fn half(value: u8) -> result: u8 pure contract {\n  requires value > 1_u8;\n} doc \"Halves a value above one.\";\n\npublic fn third(value: u8) -> result: u8 pure doc \"Divides a value by three.\";\n";
    let widened_body = b"fn half(value: u8) -> result: u8 pure contract {\n  requires value > 1_u8;\n} {\n  let result = value / 2_u8;\n  return result;\n}\n\nfn third(value: u8) -> result: u8 pure {\n  let result = value / 3_u8;\n  return result;\n}\n";
    assert_eq!(
        recomputed(PROGRAM_GRAPH, &records(widened, widened_body), &cache),
        ["pkg::base", "app"].map(str::to_owned).to_vec()
    );
    // The unreached declaration names a type no module declares: every
    // importer of the interface is rejected, cached as cold.
    let broken = b"public fn half(value: u8) -> result: u8 pure contract {\n  requires value > 1_u8;\n} doc \"Halves a value above one.\";\n\npublic fn third(value: Missing) -> result: u8 pure doc \"Divides a value by three.\";\n";
    let edited = records(broken, widened_body);
    let changed = recomputed(PROGRAM_GRAPH, &edited, &cache);
    assert_eq!(
        changed,
        ["pkg::base", "pkg::user", "pkg", "app"]
            .map(str::to_owned)
            .to_vec()
    );
    assert!(
        verdicts(PROGRAM_GRAPH, &edited, Some(&cache))
            .iter()
            .filter(|(subject, _, _)| changed.contains(subject))
            .all(|(_, outcome, _)| matches!(outcome, super::CheckOutcome::Rejected { .. })),
        "every importer of the broken interface is rejected"
    );
    // Restoring it reuses every recorded verdict.
    assert_eq!(
        recomputed(PROGRAM_GRAPH, &records(widened, widened_body), &cache),
        Vec::<String>::new()
    );
}

/// [MOD-8, FN-9] a caller's verdict reads the interface of the generic
/// function it supplies an actual to, never that function's body: when
/// the body starts calling the supplied actual, the caller's recorded
/// verdict and the root's are reused, and only the callee's module and
/// the composition, which checks the instance that now calls the actual,
/// are recomputed and accepted.
#[test]
fn a_callee_body_that_starts_calling_a_supplied_actual_leaves_its_caller_reused() {
    const GRAPH: &[u8] = b"pkg::apply: [];\npkg::caller: [pkg::apply];\npkg: [pkg::caller, std::fs, std::io, std::process, std::text];\n\nentry app = pkg::main;\n";
    const APPLY_INTERFACE: &[u8] = b"public fn run<fn step(value: u64) -> result: u64 pure>(value: u64) -> result: u64 pure doc \"Runs a value through the supplied step.\";\n";
    const RETURNS: &[u8] = b"fn run<fn step(value: u64) -> result: u64 pure>(value: u64) -> result: u64 pure {\n  return value;\n}\n";
    const CALLS: &[u8] = b"fn run<fn step(value: u64) -> result: u64 pure>(value: u64) -> result: u64 pure {\n  let stepped = step(value: value);\n  return stepped;\n}\n";
    const CALLER_INTERFACE: &[u8] =
        b"public fn go(value: u64) -> result: u64 pure doc \"Runs the caller's step.\";\n";
    const CALLER_BODY: &[u8] = b"fn twice(value: u64) -> result: u64 pure {\n  let result = value *wrap 2_u64;\n  return result;\n}\n\nfn go(value: u64) -> result: u64 pure {\n  let result = pkg::apply::run::<fn twice>(value: value);\n  return result;\n}\n";
    const MAIN_BODY: &[u8] = b"fn main() -> status: std::process::ExitStatus pure {\n  let total = pkg::caller::go(value: 3_u64);\n  let low = iand(total, 1_u64);\n  match cvt.checked::<u64, u8>(low) {\n    Ok(value: code) => {\n      return std::process::exit_status(code: code);\n    }\n    Err(error: refused) => {\n      return std::process::exit_status(code: 255_u8);\n    }\n  }\n}\n";
    let directory = CacheDirectory::new("actual");
    let cache = directory.open();
    let records = |apply_body: &'static [u8]| -> Vec<(&'static str, &'static [u8])> {
        vec![
            ("apply/module.wfm", APPLY_INTERFACE),
            ("apply/run.wf", apply_body),
            ("caller/module.wfm", CALLER_INTERFACE),
            ("caller/go.wf", CALLER_BODY),
            ("module.wfm", ROOT_INTERFACE),
            ("main.wf", MAIN_BODY),
        ]
    };
    assert_eq!(
        recomputed(GRAPH, &records(RETURNS), &cache),
        ["pkg::apply", "pkg::caller", "pkg", "app"]
            .map(str::to_owned)
            .to_vec()
    );
    assert_eq!(
        recomputed(GRAPH, &records(CALLS), &cache),
        ["pkg::apply", "app"].map(str::to_owned).to_vec()
    );
    assert!(
        verdicts(GRAPH, &records(CALLS), Some(&cache))
            .iter()
            .all(|(_, outcome, _)| matches!(outcome, super::CheckOutcome::Accepted { .. })),
        "the callee's new body and the composition that checks it are accepted"
    );
}

/// [MOD-8] a declaration a verdict reaches only through the types of
/// what it names is still read: the importer matches the variants of an
/// enum it never writes, the type of a result it calls for, so a new
/// variant recomputes and rejects it, while the root, which reaches no
/// part of that enum, keeps its verdict.
#[test]
fn a_declaration_reached_through_a_result_type_is_read() {
    const GRAPH: &[u8] = b"pkg::leaf: [];\npkg::base: [pkg::leaf];\npkg::user: [pkg::base, pkg::leaf];\npkg: [pkg::user, std::fs, std::io, std::process, std::text];\n\nentry app = pkg::main;\n";
    const TWO: &[u8] = b"public enum Shade {\n  Dark();\n  Light();\n}\n\npublic fn first() -> shade: Shade pure doc \"The first shade.\";\n";
    const THREE: &[u8] = b"public enum Shade {\n  Dark();\n  Light();\n  Dim();\n}\n\npublic fn first() -> shade: Shade pure doc \"The first shade.\";\n";
    const LEAF_BODY: &[u8] = b"fn first() -> shade: Shade pure {\n  return Shade::Dark();\n}\n";
    const BASE_INTERFACE: &[u8] =
        b"public fn pick() -> shade: pkg::leaf::Shade pure doc \"Picks a shade.\";\n";
    const BASE_BODY: &[u8] =
        b"fn pick() -> shade: pkg::leaf::Shade pure {\n  return pkg::leaf::Shade::Light();\n}\n";
    const USER_INTERFACE: &[u8] =
        b"public fn tone() -> result: u8 pure doc \"Tells the picked shade apart.\";\n";
    const USER_BODY: &[u8] = b"fn tone() -> result: u8 pure {\n  let shade = pkg::base::pick();\n  match shade {\n    Dark() => {\n      return 1_u8;\n    }\n    Light() => {\n      return 2_u8;\n    }\n  }\n}\n";
    const MAIN_BODY: &[u8] = b"fn main() -> status: std::process::ExitStatus pure {\n  let code = pkg::user::tone();\n  return std::process::exit_status(code: code);\n}\n";
    let directory = CacheDirectory::new("result-type");
    let cache = directory.open();
    let records = |leaf_interface: &'static [u8]| -> Vec<(&'static str, &'static [u8])> {
        vec![
            ("leaf/module.wfm", leaf_interface),
            ("leaf/first.wf", LEAF_BODY),
            ("base/module.wfm", BASE_INTERFACE),
            ("base/pick.wf", BASE_BODY),
            ("user/module.wfm", USER_INTERFACE),
            ("user/tone.wf", USER_BODY),
            ("module.wfm", ROOT_INTERFACE),
            ("main.wf", MAIN_BODY),
        ]
    };
    assert_eq!(
        recomputed(GRAPH, &records(TWO), &cache),
        ["pkg::leaf", "pkg::base", "pkg::user", "pkg", "app"]
            .map(str::to_owned)
            .to_vec()
    );
    assert_eq!(
        recomputed(GRAPH, &records(THREE), &cache),
        ["pkg::leaf", "pkg::base", "pkg::user", "app"]
            .map(str::to_owned)
            .to_vec()
    );
    let rejected = verdicts(GRAPH, &records(THREE), Some(&cache))
        .into_iter()
        .filter(|(_, outcome, _)| matches!(outcome, super::CheckOutcome::Rejected { .. }))
        .map(|(subject, _, _)| subject)
        .collect::<Vec<_>>();
    assert_eq!(rejected, ["pkg::user", "app"].map(str::to_owned).to_vec());
}

/// [MOD-1, MOD-5] deleting a dependency edge changes the graph facts the
/// client's check read, so its recorded verdict is not reused and the
/// recomputed one refuses the now unpermitted reference.
#[test]
fn deleting_a_graph_edge_recomputes_the_modules_that_read_it() {
    let directory = CacheDirectory::new("edge");
    let cache = directory.open();
    let records: Vec<(&str, &[u8])> = vec![
        ("base/module.wfm", BASE_INTERFACE),
        ("base/half.wf", BASE_BODY),
        ("user/module.wfm", USER_INTERFACE),
        ("user/use.wf", USER_BODY),
        ("tool/module.wfm", TOOL_INTERFACE),
        ("tool/spare.wf", TOOL_BODY),
        ("module.wfm", ROOT_INTERFACE),
        ("main.wf", ROOT_BODY),
    ];
    let _ = recomputed(PROGRAM_GRAPH, &records, &cache);
    let without_edge: &[u8] =
            b"pkg::base: [];\npkg::user: [];\npkg::tool: [];\npkg: [pkg::base, pkg::user, std::fs, std::io, std::process, std::text];\n\nentry app = pkg::main;\n";
    assert_eq!(
        recomputed(without_edge, &records, &cache),
        ["pkg::user", "pkg", "app"].map(str::to_owned).to_vec()
    );
    let user = verdicts(without_edge, &records, Some(&cache))
        .into_iter()
        .find(|(subject, _, _)| subject == "pkg::user")
        .expect("the user module's verdict");
    assert!(
        matches!(&user.1, super::CheckOutcome::Rejected { rule: Some(rule), .. } if rule == "MOD-5"),
        "{user:?}"
    );
    // A graph edit that changes no fact a check reads, such as the
    // entry list, recomputes no module verdict.
    let reworded: &[u8] =
            b"pkg::base: [];\npkg::user: [pkg::base];\npkg::tool: [];\npkg: [pkg::base, pkg::user, std::fs, std::io, std::process, std::text];\n\nentry app = pkg::main;\n\nentry spare = pkg::tool::spare;\n";
    assert_eq!(
        recomputed(reworded, &records, &cache),
        ["spare"].map(str::to_owned).to_vec()
    );
}

/// [MOD-1] a graph's meaning does not depend on row or edge order, and a
/// module outside a check's closure is no input of it: reordering both
/// recomputes nothing, and registering a module computes only its own
/// verdict and those of the modules whose names it extends.
#[test]
fn row_and_edge_order_and_unrelated_modules_leave_verdicts_reused() {
    let directory = CacheDirectory::new("order");
    let cache = directory.open();
    let mut records: Vec<(&str, &[u8])> = vec![
        ("base/module.wfm", BASE_INTERFACE),
        ("base/half.wf", BASE_BODY),
        ("user/module.wfm", USER_INTERFACE),
        ("user/use.wf", USER_BODY),
        ("tool/module.wfm", TOOL_INTERFACE),
        ("tool/spare.wf", TOOL_BODY),
        ("module.wfm", ROOT_INTERFACE),
        ("main.wf", ROOT_BODY),
    ];
    let _ = recomputed(PROGRAM_GRAPH, &records, &cache);
    let reordered: &[u8] =
            b"pkg::tool: [];\npkg::base: [];\npkg::user: [pkg::base];\npkg: [pkg::user, pkg::base, std::fs, std::io, std::process, std::text];\n\nentry app = pkg::main;\n";
    assert_eq!(
        recomputed(reordered, &records, &cache),
        Vec::<String>::new()
    );
    // A registered module one component below `pkg::tool` is a name
    // `pkg::tool`'s declarations may not take [MOD-3], so only those two
    // verdicts are computed.
    records.push(("tool/extra/module.wfm", TOOL_INTERFACE));
    records.push(("tool/extra/spare.wf", TOOL_BODY));
    let extended: &[u8] =
            b"pkg::tool: [];\npkg::base: [];\npkg::user: [pkg::base];\npkg: [pkg::user, pkg::base, std::fs, std::io, std::process, std::text];\npkg::tool::extra: [];\n\nentry app = pkg::main;\n";
    assert_eq!(
        recomputed(extended, &records, &cache),
        ["pkg::tool", "pkg::tool::extra"]
            .map(str::to_owned)
            .to_vec()
    );
}

/// [MOD-3, MOD-8] an interface sees only its module's interface
/// declarations, so a public struct whose field names a type declared in
/// an implementation record is refused in the module's own verdict, in
/// every client's verdict that reads the interface, and in the entry's
/// composition, which reports the first rejected module's verdict.
#[test]
fn a_composition_reports_its_first_rejected_module_verdict() {
    let graph: &[u8] = b"pkg::base: [];\npkg: [pkg::base, std::fs, std::io, std::process, std::text];\n\nentry app = pkg::main;\n";
    let base_interface: &[u8] = b"public struct Wrapper {\n  public value: u8;\n  secret: Hidden;\n}\n\npublic fn make() -> wrapper: Wrapper pure doc \"Makes a wrapper.\";\n";
    let base_body: &[u8] = b"struct Hidden {\n  inner: u8;\n}\n\nfn make() -> wrapper: Wrapper pure {\n  let hidden = Hidden(inner: 1_u8);\n  return Wrapper(value: 7_u8, secret: hidden);\n}\n";
    let root_body: &[u8] = b"fn main() -> status: std::process::ExitStatus pure {\n  let wrapper = pkg::base::make();\n  return std::process::exit_status(code: wrapper.value);\n}\n";
    let records: Vec<(&str, &[u8])> = vec![
        ("base/module.wfm", base_interface),
        ("base/make.wf", base_body),
        ("module.wfm", ROOT_INTERFACE),
        ("main.wf", root_body),
    ];
    let verdicts = verdicts(graph, &records, None);
    let outcome = |subject: &str| {
        verdicts
            .iter()
            .find(|(name, _, _)| name == subject)
            .map(|(_, outcome, _)| outcome.clone())
            .expect("a verdict for every subject")
    };
    let base = outcome("pkg::base");
    assert!(
        matches!(&base, super::CheckOutcome::Rejected { failure, .. } if failure.contains("base/module.wfm:3:11")),
        "{base:?}"
    );
    assert!(
        matches!(outcome("pkg"), super::CheckOutcome::Rejected { failure, .. } if failure.contains("base/module.wfm:3:11"))
    );
    assert_eq!(outcome("app"), base);
}

/// [MOD-9] an entry selects a function its module's own inventory
/// declares: a PRE-1 function is no entry even when the first registered
/// module is selected, whose row a prelude function's checked record
/// shares, and a named entry's rejection is located at its `entry_decl`.
#[test]
fn an_entry_selects_its_modules_own_function_and_is_located_when_refused() {
    let graph = crate::form_module_graph(
            SourceInput::new(
                "modules.wfg",
                b"pkg::a: [std::process];\npkg: [pkg::a, std::process];\n\nentry main = pkg::main;\n\nentry hidden = pkg::a::seven;\n",
            ),
            CompilerLimits::default(),
        )
        .expect("the graph forms");
    let records: [(&str, &[u8]); 4] = [
            (
                "a/module.wfm",
                b"public fn value() -> result: u8 pure doc \"Seven.\";\n",
            ),
            (
                "a/a.wf",
                b"fn value() -> result: u8 pure {\n  return 7_u8;\n}\n\nfn seven() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 7_u8);\n}\n",
            ),
            ("module.wfm", ROOT_INTERFACE),
            (
                "main.wf",
                b"fn main() -> status: std::process::ExitStatus pure {\n  let code = pkg::a::value();\n  return std::process::exit_status(code: code);\n}\n",
            ),
        ];
    let inputs = module_inputs(&graph, &records);
    let limits = CompilerLimits::default();
    super::check_module_entry(&graph, &inputs, super::ModuleEntry::Named("main"), limits)
        .expect("the named entry composes");
    super::check_module_entry(
        &graph,
        &inputs,
        super::ModuleEntry::Function {
            module: "pkg::a",
            function: "seven",
        },
        limits,
    )
    .expect("an unnamed entry may select a private function");
    let prelude = super::check_module_entry(
        &graph,
        &inputs,
        super::ModuleEntry::Function {
            module: "pkg::a",
            function: "exit_status",
        },
        limits,
    )
    .expect_err("a PRE-1 function is no function of pkg::a");
    assert_eq!(prelude.rule_id(), Some("MOD-9"));
    let named =
        super::check_module_entry(&graph, &inputs, super::ModuleEntry::Named("hidden"), limits)
            .expect_err("a named entry runs a public function");
    assert_eq!(named.rule_id(), Some("MOD-9"));
    let location = named.location().expect("a named entry is written");
    assert_eq!(
        (location.path(), location.line(), location.column()),
        ("modules.wfg", 6, 1)
    );
    assert!(
        named
            .to_string()
            .starts_with("modules.wfg:6:1: error[MOD-9]: EntryFunctionPrivate\n  source: entry hidden = pkg::a::seven;\n"),
        "{named}"
    );
}

/// [MOD-8] after an interface edit, the impact report lists every
/// failing definition and consumer body with its written location: the
/// changed module's definition, and both of a client's bodies, the
/// declared function and the private helper after it, whose lines are
/// those of the record as written.
#[test]
fn an_impact_report_lists_every_failing_definition_and_body() {
    let graph: &[u8] = b"pkg::base: [];\npkg::user: [pkg::base];\n";
    let records: Vec<(&str, &[u8])> = vec![
            (
                "base/module.wfm",
                b"public fn one(value: u8, extra: u8) -> result: u8 pure doc \"Now takes two values.\";\n",
            ),
            (
                "base/one.wf",
                b"fn one(value: u8) -> result: u8 pure {\n  return value;\n}\n",
            ),
            (
                "user/module.wfm",
                b"public fn first() -> result: u8 pure doc \"Calls one.\";\n",
            ),
            (
                "user/use.wf",
                b"fn first() -> result: u8 pure {\n  let value = pkg::base::one(value: 1_u8);\n  return value;\n}\n\nfn second() -> result: u8 pure {\n  let value = pkg::base::one(value: 2_u8);\n  return value;\n}\n",
            ),
        ];
    let cache_directory = CacheDirectory::new("impact");
    let cache = cache_directory.open();
    for cache in [None, Some(&cache), Some(&cache)] {
        let verdicts = verdicts(graph, &records, cache);
        let tasks = |subject: &str| match verdicts
            .iter()
            .find(|(name, _, _)| name == subject)
            .map(|(_, outcome, _)| outcome)
        {
            Some(super::CheckOutcome::Rejected {
                tasks, complete, ..
            }) => (
                tasks
                    .iter()
                    .map(|task| {
                        (
                            task.kind(),
                            task.function().map(str::to_owned),
                            task.location().map(|at| (at.path().to_owned(), at.line())),
                        )
                    })
                    .collect::<Vec<_>>(),
                *complete,
            ),
            other => panic!("{subject}: {other:?}"),
        };
        assert_eq!(
            tasks("pkg::base"),
            (
                vec![(
                    super::TaskKind::Definition,
                    Some("one".to_owned()),
                    Some(("base/one.wf".to_owned(), 1))
                )],
                true
            )
        );
        assert_eq!(
            tasks("pkg::user"),
            (
                vec![
                    (
                        super::TaskKind::Body,
                        Some("first".to_owned()),
                        Some(("user/use.wf".to_owned(), 2))
                    ),
                    (
                        super::TaskKind::Body,
                        Some("second".to_owned()),
                        Some(("user/use.wf".to_owned(), 7))
                    ),
                ],
                true
            )
        );
    }
}

/// [MOD-6, MOD-8] the interface rendering prints every resolved name as
/// its qualified identity whatever alias wrote it, leaves `doc` entries
/// out, and includes the complete definitions the public declarations
/// reach, so a dependency's representation change shows in a client
/// whose interface file did not change.
#[test]
fn an_interface_renders_resolved_identities_and_reached_definitions() {
    let graph_bytes: &[u8] = b"pkg::shape: [];\npkg::client: [pkg::shape];\n";
    let graph = crate::form_module_graph(
        SourceInput::new("modules.wfg", graph_bytes),
        CompilerLimits::default(),
    )
    .expect("the graph forms");
    let render = |shape: &'static [u8], client: &'static [u8]| {
        let records: Vec<(&str, &[u8])> =
            vec![("shape/module.wfm", shape), ("client/module.wfm", client)];
        let inputs = module_inputs(&graph, &records);
        super::render_module_interface(&graph, &inputs, "pkg::client", CompilerLimits::default())
            .expect("the interface renders")
    };
    let shape: &[u8] = b"public struct Point {\n  public x: u8;\n  hidden: u8;\n}\n";
    let client: &[u8] = b"alias Spot = pkg::shape::Point;\n\npublic fn origin() -> result: Spot pure doc \"The origin.\";\n";
    let rendered = render(shape, client);
    assert_eq!(
        rendered,
        "module pkg::client\npublic fn pkg::client::origin ( ) -> result : pkg::shape::Point pure ;\nreached\npublic struct pkg::shape::Point { public x : u8 ; hidden : u8 ; }\n"
    );
    // A documentation edit, or dropping the entry, changes nothing.
    let undocumented: &[u8] =
            b"alias Spot = pkg::shape::Point;\n\npublic fn origin() -> result: Spot pure doc \"The origin.\";\n";
    assert_eq!(render(shape, undocumented), rendered);
    // A private field of the dependency's type changes the client's
    // rendering, though the client's own file is unchanged.
    let widened: &[u8] = b"public struct Point {\n  public x: u8;\n  hidden: u16;\n}\n";
    assert_ne!(render(widened, client), rendered);
}

/// A source bundle whose `select` indexes a table through `relay`'s
/// verified bound, which `relay` in turn proves from `clamp`'s.
fn receipt_program(clamp_bound: u64, kept: u64, clamp_fallback: u64) -> String {
    format!(
        "const lookup: Array<u8, 8> =[0_u8, 1_u8, 2_u8, 3_u8, 4_u8, 5_u8, 6_u8, 7_u8];\n\n\
             fn clamp(value: u64) -> result: u64 pure contract {{\n  ensures result <= {clamp_bound}_u64;\n}} {{\n  if value <= {kept}_u64 {{\n    return value;\n  }}\n  return {clamp_fallback}_u64;\n}}\n\n\
             fn relay(value: u64) -> result: u64 pure contract {{\n  ensures result <= 7_u64;\n}} {{\n  let clamped = clamp(value: value);\n  return clamped;\n}}\n\n\
             fn select(index: u64) -> result: u8 pure {{\n  let bounded = relay(value: index);\n  return lookup[bounded];\n}}\n\n\
             fn main() -> status: std::process::ExitStatus pure {{\n  let code = select(index: 3_u64);\n  return std::process::exit_status(code: code);\n}}\n"
    )
}

/// [MOD-8, FN-9] proof receipts change which analyses a check runs and
/// never its verdict. A body edit that keeps a function's boundary
/// reanalyzes that function alone; a changed `ensures` also reanalyzes
/// the callers whose proofs read it, and no caller further out; a
/// boundary a caller's proof no longer admits is rejected as a check
/// without receipts rejects it; and reverting reuses every receipt.
#[test]
fn a_proof_receipt_is_reused_exactly_while_its_analysis_inputs_are_unchanged() {
    let directory = CacheDirectory::new("receipts");
    let cache = directory.open();
    let analyzed = |source: &str| -> (Result<(), String>, u64) {
        let inputs = [SourceInput::new("receipts.wf", source.as_bytes())];
        let fresh =
            super::check(&inputs, CompilerLimits::default()).map_err(|failure| failure.to_string());
        let (_, recorded) = cache.receipt_counts();
        let cached = super::check_with_cache(&inputs, CompilerLimits::default(), &cache)
            .map_err(|failure| failure.to_string());
        assert_eq!(cached, fresh, "receipts change no verdict");
        (cached, cache.receipt_counts().1 - recorded)
    };
    let original = receipt_program(7, 7, 7);
    let (verdict, _) = analyzed(&original);
    assert_eq!(verdict, Ok(()));
    assert_eq!(
        analyzed(&original),
        (Ok(()), 0),
        "a warm check analyzes nothing"
    );
    // `clamp`'s body changes and its boundary does not.
    assert_eq!(analyzed(&receipt_program(7, 7, 6)), (Ok(()), 1));
    // `clamp`'s boundary changes: it and `relay`, whose proof reads it,
    // are analyzed; `select` reads only `relay`'s unchanged boundary.
    assert_eq!(analyzed(&receipt_program(6, 6, 6)), (Ok(()), 2));
    // A boundary `relay`'s proof no longer admits.
    let (verdict, _) = analyzed(&receipt_program(8, 7, 7));
    assert!(
        verdict
            .as_ref()
            .is_err_and(|failure| failure.contains("[FN-9]")),
        "{verdict:?}"
    );
    assert_eq!(
        analyzed(&original),
        (Ok(()), 0),
        "reverting reuses every receipt"
    );
}

/// [MOD-9] an entry build is reused for an unchanged composition and
/// emits the same module a build without a cache emits; an edit outside
/// its composition does not rebuild it.
#[test]
fn an_entry_build_is_reused_for_an_unchanged_composition() {
    let directory = CacheDirectory::new("build");
    let cache = directory.open();
    let graph = crate::form_module_graph(
        SourceInput::new("modules.wfg", PROGRAM_GRAPH),
        CompilerLimits::default(),
    )
    .expect("the graph forms");
    let build = |tool_body: &'static [u8]| {
        let records: Vec<(&str, &[u8])> = vec![
            ("base/module.wfm", BASE_INTERFACE),
            ("base/half.wf", BASE_BODY),
            ("user/module.wfm", USER_INTERFACE),
            ("user/use.wf", USER_BODY),
            ("tool/module.wfm", TOOL_INTERFACE),
            ("tool/spare.wf", tool_body),
            ("module.wfm", ROOT_INTERFACE),
            ("main.wf", ROOT_BODY),
        ];
        let inputs = module_inputs(&graph, &records);
        let cached = super::build_module_entry(
            &graph,
            &inputs,
            super::ModuleEntry::Named("app"),
            CompilerLimits::default(),
            OverlapLowering::Off,
            Some(&cache),
        )
        .expect("the entry builds");
        let cold = super::compile_module_program(
            &graph,
            &inputs,
            super::ModuleEntry::Named("app"),
            CompilerLimits::default(),
            OverlapLowering::Off,
        )
        .expect("the entry builds");
        assert_eq!(cached.0, cold);
        cached.1
    };
    assert!(!build(TOOL_BODY));
    assert!(build(TOOL_BODY));
    assert!(build(
        b"fn spare() -> result: u8 pure {\n  return 3_u8;\n}\n"
    ));
}

/// [FN-2, MOD-8] a rejection raised while checking a concrete instance
/// stays at the template's source, in the module that owns it, and names
/// the call in another module that requested the instance. Here the
/// instance's result capacity `n * 2` leaves the u64 domain [CONST-1],
/// which the template, checked for every `n`, does not.
#[test]
fn an_instance_failure_names_the_template_and_its_requesting_call() {
    let graph = crate::form_module_graph(
        SourceInput::new(
            "modules.wfg",
            b"pkg::lib: [];\npkg: [pkg::lib, std::fs, std::io, std::process, std::text];\n\nentry app = pkg::main;\n",
        ),
        CompilerLimits::default(),
    )
    .expect("the graph forms");
    let records: [(&str, &[u8]); 4] = [
            (
                "lib/module.wfm",
                b"public fn doubled<const n: u64>(count: u64) -> result: Slots<u64, n * 2> pure doc \"Forms a run of twice the capacity.\";\n",
            ),
            (
                "lib/doubled.wf",
                b"fn doubled<const n: u64>(count: u64) -> result: Slots<u64, n * 2> pure {\n  return slots_new::<u64, n * 2>();\n}\n",
            ),
            (
                "module.wfm",
                b"public fn main() -> status: std::process::ExitStatus pure doc \"Requests one overflowing instance.\";\n",
            ),
            (
                "main.wf",
                b"fn main() -> status: std::process::ExitStatus pure {\n  let cells = pkg::lib::doubled::<9223372036854775808>(count: 7_u64);\n  return std::process::exit_status(code: 0_u8);\n}\n",
            ),
        ];
    let inputs = module_inputs(&graph, &records);
    let failure = crate::check_module_program(&graph, &inputs, CompilerLimits::default())
        .expect_err("the instance's capacity leaves the u64 domain");
    assert_eq!(failure.rule_id(), Some("CONST-1"));
    let detail = failure.to_string();
    assert!(
        detail.starts_with("lib/module.wfm:1:67: error[CONST-1]: ConstEvalOverflow\n"),
        "{detail}"
    );
    assert!(
        detail.contains("\n  requested_at: main.wf:2:15 "),
        "{detail}"
    );
}

/// [STOR-8, MOD-9] a no-heap entry's build emits its execution closure
/// alone, so neither the other entry's allocating module nor an unused
/// allocating helper of its own module leaves an allocator reference in
/// its output, while the heap-using entry of the same graph keeps one.
#[test]
fn a_no_heap_entry_build_names_no_allocator_that_its_sibling_entry_uses() {
    let graph = crate::form_module_graph(
            SourceInput::new(
                "modules.wfg",
                b"pkg: [std::process];\npkg::tools: [std::process];\n\nentry kernel = pkg::start {\n  no_heap;\n}\n\nentry tool = pkg::tools::run;\n",
            ),
            CompilerLimits::default(),
        )
        .expect("the graph forms");
    let records: [(&str, &[u8]); 4] = [
            (
                "module.wfm",
                b"public fn start() -> status: std::process::ExitStatus pure doc \"Starts the kernel.\";\n",
            ),
            (
                "start.wf",
                b"fn spare() -> result: u8 pure {\n  let cell = box_new::<u8>(value: 1_u8);\n  return 0_u8;\n}\n\nfn start() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n",
            ),
            (
                "tools/module.wfm",
                b"public fn run() -> status: std::process::ExitStatus pure doc \"Runs the tool.\";\n",
            ),
            (
                "tools/run.wf",
                b"fn run() -> status: std::process::ExitStatus pure {\n  let cell = box_new::<u8>(value: 7_u8);\n  return std::process::exit_status(code: 0_u8);\n}\n",
            ),
        ];
    let inputs = module_inputs(&graph, &records);
    let build = |entry| {
        super::compile_module_program(
            &graph,
            &inputs,
            super::ModuleEntry::Named(entry),
            CompilerLimits::default(),
            OverlapLowering::Off,
        )
        .expect("the entry builds")
    };
    let kernel = build("kernel");
    assert!(!kernel.contains("@malloc"), "{kernel}");
    assert!(!kernel.contains("@free"), "{kernel}");
    assert!(!kernel.contains("spare"), "{kernel}");
    assert!(!kernel.contains("wf_tools.run"), "{kernel}");
    let tool = build("tool");
    assert!(tool.contains("call ptr @malloc"), "{tool}");
    assert!(tool.contains("@wf_tools.run("), "{tool}");
}

#[test]
fn public_compilation_requires_a_source_record_before_adding_the_prelude() {
    for failure in [
        check(&[], CompilerLimits::default()).expect_err("no source record was supplied"),
        compile(&[], CompilerLimits::default()).expect_err("no source record was supplied"),
    ] {
        assert_eq!(failure.stage(), CompilationStage::SourceEnvelope);
        assert_eq!(failure.kind(), CompilationFailureKind::Invocation);
        assert_eq!(failure.rule_id(), None);
        // A stop that is not a source rejection carries its stage value
        // as the one `payload` field, under a kind named for it.
        assert_eq!(failure.detail(), "payload: EmptySourceSequence");
        assert!(
            failure.to_string().starts_with(
                "whitefootc: invocation failure in SourceEnvelope: EmptySourceSequence\n"
            ),
            "{failure}"
        );
    }

    let empty = check(
        &[SourceInput::new("empty.wf", b"")],
        CompilerLimits::default(),
    )
    .expect_err("a present record still needs its canonical final newline");
    assert_eq!(empty.stage(), CompilationStage::CanonicalSource);
    assert_eq!(empty.kind(), CompilationFailureKind::Source);
    assert_eq!(empty.rule_id(), Some("FORM-2"));

    check(
        &[SourceInput::new("empty.wf", b"\n")],
        CompilerLimits::default(),
    )
    .expect("a canonical source record may contain no declarations");
}

#[test]
fn source_envelope_limits_are_resource_failures_in_both_public_projections() {
    let inputs = [SourceInput::new("main.wf", b"@")];
    let mut count_limits = CompilerLimits::default();
    count_limits.source.max_sources = 0;
    let mut byte_limits = CompilerLimits::default();
    byte_limits.source.max_source_bytes = 0;

    for (limits, detail) in [
        (
            count_limits,
            format!(
                "LimitExceeded {{ limit: Sources, maximum: 0, actual: {} }}",
                inputs.len() + crate::prelude::DECLARATIONS.len()
            ),
        ),
        (
            byte_limits,
            "LimitExceeded { limit: SourceBytes, maximum: 0, actual: 1 }".to_owned(),
        ),
    ] {
        for failure in [
            check(&inputs, limits).expect_err("the source envelope exceeds its ceiling"),
            compile(&inputs, limits).expect_err("the source envelope exceeds its ceiling"),
        ] {
            assert_eq!(failure.stage(), CompilationStage::SourceEnvelope);
            assert_eq!(failure.kind(), CompilationFailureKind::Resource);
            assert_eq!(failure.rule_id(), None);
            assert_eq!(failure.detail(), format!("payload: {detail}"));
        }
    }
}

#[test]
fn source_envelope_invalid_paths_remain_invocation_failures() {
    let invalid = [SourceInput::new("/main.wf", b"@")];
    let duplicate = [
        SourceInput::new("main.wf", b"@"),
        SourceInput::new("main.wf", b"@"),
    ];
    for (inputs, detail) in [
        (invalid.as_slice(), "LogicalPath(Absolute)"),
        (
            duplicate.as_slice(),
            "DuplicateLogicalPath { path: LogicalPath(\"main.wf\"), first_position: 0, duplicate_position: 1 }",
        ),
    ] {
        for failure in [
            check(inputs, CompilerLimits::default()).expect_err("the source envelope is invalid"),
            compile(inputs, CompilerLimits::default()).expect_err("the source envelope is invalid"),
        ] {
            assert_eq!(failure.stage(), CompilationStage::SourceEnvelope);
            assert_eq!(failure.kind(), CompilationFailureKind::Invocation);
            assert_eq!(failure.rule_id(), None);
            assert_eq!(failure.detail(), format!("payload: {detail}"));
        }
    }
}

#[test]
fn source_envelope_storage_and_representation_failures_preserve_their_details() {
    use crate::{LogicalPathError, SourceBundleError, SourceLimit};

    // Exercise allocator failure classification without exhausting the host.
    for error in [
        SourceBundleError::StorageUnavailable {
            limit: SourceLimit::Sources,
            requested: 1,
        },
        SourceBundleError::ArithmeticOverflow,
        SourceBundleError::LogicalPath(LogicalPathError::LengthOverflow),
        SourceBundleError::LogicalPath(LogicalPathError::StorageUnavailable { requested: 7 }),
    ] {
        let detail = format!("{error:?}");
        let failure = super::CompilationFailure::source_envelope(error);
        assert_eq!(failure.stage(), CompilationStage::SourceEnvelope);
        assert_eq!(failure.kind(), CompilationFailureKind::Resource);
        assert_eq!(failure.rule_id(), None);
        assert_eq!(failure.detail(), format!("payload: {detail}"));
    }
}

#[test]
fn the_executable_caller_proves_the_selected_functions_contract() {
    let source = b"fn main() -> result: unit pure contract {\n  requires 0_u64 <= 1_u64;\n} {\n  return unit;\n}\n";
    let llvm = compile(
        &[SourceInput::new("entry-contract.wf", source)],
        CompilerLimits::default(),
    )
    .expect("the ordinary generated caller proves a constant requirement");
    assert!(llvm.contains("@wf_executable_caller_0("));
    assert!(llvm.contains("define i32 @main("));
    assert!(!llvm.contains("Executable caller was not admitted"));
}

#[test]
fn an_uninhabited_function_is_a_library_without_an_unproved_executable_call() {
    let source = b"fn main() -> result: unit pure contract {\n  requires 1_u64 <= 0_u64;\n} {\n  return unit;\n}\n";
    let llvm = compile(
        &[SourceInput::new("uninhabited-entry.wf", source)],
        CompilerLimits::default(),
    )
    .expect("an uninhabited ordinary declaration is accepted as a library");
    assert!(llvm.contains("@wf_main("));
    assert!(!llvm.contains("define i32 @main("));
    assert!(llvm.contains("Executable caller was not admitted"));
}

/// The permission ledger of one compiled source, in the order the driver
/// hands it to `whitefootc --par-ledger`.
///
/// The judgment is pure, so the ledger belongs to the source and not to
/// the lowering: this reads it from the default compilation, the one that
/// actualizes nothing.
fn ledger_of(name: &str, source: &[u8]) -> Vec<String> {
    let (_, ledger) = compile_with_permission_ledger(
        &[SourceInput::new(name, source)],
        CompilerLimits::default(),
        OverlapLowering::Off,
    )
    .expect("a permission-ledger fixture must compile");
    ledger
}

/// A syntax rejection prints the spellings it expected and the line it
/// stopped in.
///
/// Flat three-address form is the largest departure from every other
/// systems language, so this is the rule an unguided writer hits first.
/// They hit it as `TerminalSet(38424498140022966840644862354)` and a byte
/// offset, and ran `head -c` on their own program to find out what it
/// meant. The compiler holds the expected set and the source bytes; both
/// are printed here.
#[test]
fn a_syntax_rejection_prints_the_expected_spellings_and_the_offending_line() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  doc "Writes a nested call where the grammar admits an atom.";
  let dotted = 1_u8;
  let addressable = 2_u8;
  let skip = bor(dotted, bnot(addressable));
  return std::process::exit_status(code: skip);
}
"#;
    let failure = compile(
        &[SourceInput::from_host_path(
            "input0.wf",
            "/absolute/path/wc.wf",
            source,
        )],
        CompilerLimits::default(),
    )
    .expect_err("a nested call is not an atom");
    assert_eq!(failure.rule_id(), Some("GRAM-9"));
    let detail = failure.detail();
    // The set as spellings, in the grammar's own order.
    assert!(
        detail.contains(r#"expected: [";", "{", ")", ",", "<", ">", "["#),
        "{detail}"
    );
    // The line the writer wrote, and where in it the parser stopped.
    let rendered = failure.to_string();
    assert!(
        rendered.starts_with("/absolute/path/wc.wf:5:26: error[GRAM-9]: UnexpectedToken\n"),
        "{rendered}"
    );
    // The marker sits under `bnot(`, the forbidden call start [DIAG-1].
    assert!(
        rendered.contains(&format!(
            "\n  source:   let skip = bor(dotted, bnot(addressable));\n  marker: {}^^^^^\n",
            " ".repeat(25)
        )),
        "{rendered}"
    );
    assert!(detail.contains(r#"found: "bnot(""#), "{detail}");
}

#[test]
fn invariant_targets_and_certificate_steps_keep_distinct_rule_owners() {
    for (name, source, stage, rule) in [
            (
                "local-target-formation.wf",
                // v0.60's [INV-1] admits `==` in an invariant target and
                // refuses `!=` in either position, which is the reverse of
                // the v0.59 row this fixture carried.
                b"fn main() -> status: std::process::ExitStatus pure {\n  invariant bad: 0_u64 != 0_u64;\n  return std::process::exit_status(code: 0_u8);\n}\n"
                    .as_slice(),
                CompilationStage::Semantics,
                "INV-1",
            ),
            (
                "local-target-unproved.wf",
                b"fn main() -> status: std::process::ExitStatus pure {\n  invariant bad: 1_u64 <= 0_u64;\n  return std::process::exit_status(code: 0_u8);\n}\n",
                CompilationStage::Semantics,
                "INV-1",
            ),
            (
                "use-relation-formation.wf",
                b"fn check(value: u64, limit: u64) -> result: unit pure {\n  invariant scaled: 2_u64 * value <= 2_u64 * limit {\n    use (value == limit);\n  }\n  return unit;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n",
                CompilationStage::Semantics,
                "PRF-1",
            ),
            (
                "use-relation-name.wf",
                b"fn check(value: u64, limit: u64) -> result: unit pure {\n  invariant scaled: 2_u64 * value <= 2_u64 * limit {\n    use (value <= missing);\n  }\n  return unit;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n",
                CompilationStage::Resolution,
                "PRF-1",
            ),
            (
                "named-use-scope.wf",
                b"fn check(value: u64, limit: u64) -> result: unit pure {\n  invariant scaled: 2_u64 * value <= 2_u64 * limit {\n    use missing;\n  }\n  return unit;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n",
                CompilationStage::Resolution,
                "INV-1",
            ),
        ] {
            let failure = compile(
                &[SourceInput::new(name, source)],
                CompilerLimits::default(),
            )
            .expect_err("the focused invalid proof form must reject");
            assert_eq!(failure.stage(), stage, "{name}: {failure}");
            assert_eq!(failure.kind(), CompilationFailureKind::Source);
            assert_eq!(failure.rule_id(), Some(rule), "{name}: {failure}");
        }
}

/// A canonical-form rejection prints the bytes it wanted and the bytes it
/// found.
///
/// FORM-2 is machine-decided, so the auditor knows both at the point it
/// stops. It used to print neither, and one double space in an effect row
/// cost a writer a compile round spent bisecting a byte offset.
#[test]
fn a_canonical_rejection_prints_the_expected_bytes_beside_the_found_bytes() {
    let source = b"fn main() -> status: std::process::ExitStatus pure {\n  doc \"One double space where canonical form admits one space.\";\n  return std::process::exit_status(code:  0_u8);\n}\n";
    let failure = compile(
        &[SourceInput::from_host_path(
            "input0.wf",
            "/absolute/path/report.wf",
            source,
        )],
        CompilerLimits::default(),
    )
    .expect_err("a double space is not canonical form");
    assert_eq!(failure.rule_id(), Some("FORM-2"));
    assert_eq!(failure.detail(), "expected: \" \"\nfound: \"  \"");
    let rendered = failure.to_string();
    assert!(
        rendered.starts_with("/absolute/path/report.wf:3:"),
        "{rendered}"
    );
}

/// An ordinary reference parameter keeps its later call requirement.
///
/// Retired subject: the child reborrow inside a `region { .. }` block,
/// which ended a loan before the following ordinary statement. v0.60 has
/// no regions and no reborrows [REF-1, REF-3]; the successor kept here is
/// the second half this case always carried — a missing `open_file` range
/// proof is still reported at the call and is still repaired by writing
/// the requirement, not by adding a scope.
#[test]
fn a_reference_parameter_keeps_its_later_call_requirement() {
    let source = br#"fn walk(factory: &std::io::HandleFactory, root: &std::fs::DirectoryRead, name: &[u8]) -> result: u8 reads(root), reads(name), writes(factory) {
  match std::fs::open_file(factory: factory, root: root, name: name, start: 0_u64, end: 1_u64) {
    Ok(value: handle) => {
      std::fs::close_read(factory: factory, file: move handle);
    }
    Err(error: problem) => {
    }
  }
  let later = 0_u8;
  return 0_u8;
}
"#;
    let failure = compile(
        &[SourceInput::new("walk.wf", source)],
        CompilerLimits::default(),
    )
    .expect_err("the unchanged source still lacks the file-name range proof");
    assert_eq!(failure.rule_id(), Some("FN-8"));
    assert!(
        failure.detail().contains("1_u64 <= deref(name).len"),
        "{}",
        failure.detail()
    );
    let bounded = std::str::from_utf8(source).unwrap().replace(
        "writes(factory) {",
        "writes(factory) contract {\n  requires 1_u64 <= deref(name).len;\n} {",
    );
    compile(
        &[SourceInput::new("bounded_walk.wf", bounded.as_bytes())],
        CompilerLimits::default(),
    )
    .expect("the required range, not an extra scope helper, completes the valid program");
}

/// A post-syntax rejection names the file it is talking about and quotes
/// the line, in both stages that reject source after parsing.
///
/// The blind-writer trial's six rejections all printed `SourceId(0)` and a
/// byte offset, and the writer ran `head -c` on their own program to find
/// out what the offset meant. Semantics and resolution both already hold
/// the coordinate the rule selected; this is that coordinate resolved.
#[test]
fn a_post_syntax_rejection_names_its_file_and_quotes_its_line() {
    let host = "/absolute/path/counts.wf";

    // [OWN-1], reached in the semantic checker.
    let affine = br#"nocopy struct Counts {
  lines: u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let running = Counts(lines: 0_u64);
  let totals = running;
  return std::process::exit_status(code: 0_u8);
}
"#;
    let failure = compile(
        &[SourceInput::from_host_path("input0.wf", host, affine)],
        CompilerLimits::default(),
    )
    .expect_err("a bare affine use is rejected");
    assert_eq!(failure.rule_id(), Some("OWN-1"));
    let rendered = failure.to_string();
    assert!(
        rendered.starts_with(&format!("{host}:7:16: error[OWN-1]: BareAffineUse\n")),
        "{rendered}"
    );
    assert!(
        rendered.contains("\n  source:   let totals = running;\n"),
        "{rendered}"
    );
    assert!(!rendered.contains("input0.wf"), "{rendered}");

    // [TYPE-6], reached in the resolver.
    let collision = br#"fn main() -> status: std::process::ExitStatus pure {
  let permit = 1_u64;
  if permit == 1_u64 {
    let permit = 2_u64;
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let failure = compile(
        &[SourceInput::from_host_path("input0.wf", host, collision)],
        CompilerLimits::default(),
    )
    .expect_err("a redeclared binder is rejected");
    assert_eq!(failure.rule_id(), Some("TYPE-6"));
    let rendered = failure.to_string();
    assert!(
        rendered.starts_with(&format!(
            "{host}:4:9: error[TYPE-6]: DeclarationCollision\n"
        )),
        "{rendered}"
    );
    assert!(
        rendered.contains("\n  source:     let permit = 2_u64;\n"),
        "{rendered}"
    );
    // The earlier declaration is a position too, never a node path: its
    // name's position, quoted by the declaration it belongs to.
    assert!(
        failure
            .detail()
            .contains(&format!(r#"origin: {host}:2:7 "let permit = 1_u64;""#)),
        "{rendered}"
    );
    assert!(!rendered.contains("input0.wf"), "{rendered}");
}

/// A lexical rejection names the host path too.
///
/// It is the one stage that already printed a path of its own, from the
/// span rather than from a wrapper, and the path it printed was the
/// bundle's positional key — so the first rejection a writer can possibly
/// receive was also the one that cited a file that does not exist.
#[test]
fn a_lexical_rejection_names_the_host_path() {
    let host = "/absolute/path/pound.wf";
    let source = "fn main() -> status: std::process::ExitStatus pure {\n  let x = \u{a3};\n  return std::process::exit_status(code: 0_u8);\n}\n";
    let failure = compile(
        &[SourceInput::from_host_path(
            "input0.wf",
            host,
            source.as_bytes(),
        )],
        CompilerLimits::default(),
    )
    .expect_err("a non-source byte is rejected");
    assert_eq!(failure.rule_id(), Some("FORM-1"));
    let rendered = failure.to_string();
    assert!(
        rendered.starts_with(&format!("{host}:2:11: error[FORM-1]: UnexpectedByte\n")),
        "{rendered}"
    );
    assert!(!rendered.contains("input0.wf"), "{rendered}");
    // The scalar is invisible on many terminals, so it is also escaped.
    assert_eq!(failure.detail(), r#"found: "\u{a3}""#);
}

/// A source read from a host path the closed logical spelling cannot hold
/// is still named by that host path everywhere a reader looks.
///
/// An absolute path is how a script, a Makefile, and an agent all invoke
/// this compiler. Renaming it to a positional `input0.wf` made every
/// ledger line and every byte offset refer to a file that exists nowhere
/// on disk, so the output was not usable as emitted.
#[test]
fn a_ledger_names_the_host_path_the_source_was_read_from() {
    let source = br#"fn main() -> status: std::process::ExitStatus pure {
  let total = 0_u64;
  for @scan (index in 0_u64..4_u64) {
    set total = total +wrap index;
  }
  return std::process::exit_status(code: 0_u8);
}
"#;
    let host = "/absolute/path/counted.wf";
    let (_, ledger) = compile_with_permission_ledger(
        &[SourceInput::from_host_path("input0.wf", host, source)],
        CompilerLimits::default(),
        OverlapLowering::Off,
    )
    .expect("the fixture compiles");
    assert!(
        ledger.iter().all(|line| line.contains(host)),
        "every ledger line names the host path: {ledger:?}"
    );
    assert!(
        ledger.iter().all(|line| !line.contains("input0.wf")),
        "the bundle's own key is not reader-facing text: {ledger:?}"
    );
}

const TREE_PRELUDE: &str = "enum BoxNode {
  Leaf(w: u64);
  Branch(left: Box<BoxNode>, right: Box<BoxNode>, w: u64);
}

fn boxed_leaf(w: u64) -> result: Box<BoxNode> pure {
  let leaf = BoxNode::Leaf(w: w);
  return box_new::<BoxNode>(value: move leaf);
}

fn boxed_branch(left: Box<BoxNode>, right: Box<BoxNode>) -> result: Box<BoxNode> pure {
  let branch = BoxNode::Branch(left: move left, right: move right, w: 0_u64);
  return box_new::<BoxNode>(value: move branch);
}

";

/// The ledger states an eligible pair and the chain it composes into, then
/// says the same of a pair whose recursive closure carries an erased
/// source proof. Proof syntax changes no runtime footprint and therefore
/// produces no separate `not-actualizable` class.
#[test]
fn the_permission_ledger_reports_eligible_pairs_and_their_chains() {
    let eligible = format!(
        "{TREE_PRELUDE}fn fold(node: &Box<BoxNode>) -> result: u64 writes(node) {{
  match deref(node).inner {{
    Leaf(w: leaf_w) => {{
      return deref(leaf_w);
    }}
    Branch(left: l, right: r, w: slot) => {{
      let a = fold(node: l);
      let b = fold(node: r);
      let total = imax(a, b);
      set deref(slot) = total;
      return total;
    }}
  }}
}}

fn main() -> status: std::process::ExitStatus pure {{
  let leaf0 = boxed_leaf(w: 3_u64);
  let leaf1 = boxed_leaf(w: 4_u64);
  let branch0 = boxed_branch(left: move leaf0, right: move leaf1);
  let total = fold(node: &branch0);
  return std::process::exit_status(code: 0_u8);
}}
"
    );
    let ledger = ledger_of("fold.wf", eligible.as_bytes());
    assert_eq!(
        ledger[0],
        "PAR permitted   fold.wf:22  pair(fold, fold)  eligible"
    );
    // The chain the pair composes into, reported beside it. Pairs alone
    // cannot tell one three-member run from three separate two-member
    // ones, and those are completely different work, so the chain is
    // stated rather than left to be inferred.
    assert_eq!(
        ledger[1],
        "PAR chain       fold.wf:22  run(fold, fold)  2 members through line 23"
    );

    // The same tree fold with one checked proof in the recursive closure.
    // `scaled` makes the fact explicit, the semantic checker verifies it,
    // and lowering erases it before the permission table is consumed.
    let proved = format!(
        "{TREE_PRELUDE}fn scaled(values: Array<u64, 8>, index: u64) -> result: u64 pure {{
  let size = values.len;
  let bounded = iand(index, 7_u64);
  invariant index_in_range: bounded <= 7_u64;
  return values[bounded];
}}

fn bubble(node: &Box<BoxNode>) -> result: u64 writes(node) {{
  match deref(node).inner {{
    Leaf(w: leaf_w) => {{
      let w = deref(leaf_w);
      let values = array_filled::<u64, 8>(value: 1_u64);
      let touched = scaled(values: values, index: w);
      return w;
    }}
    Branch(left: l, right: r, w: slot) => {{
      let a = bubble(node: l);
      let b = bubble(node: r);
      let total = a +wrap b;
      set deref(slot) = total;
      return total;
    }}
  }}
}}

fn main() -> status: std::process::ExitStatus pure {{
  let leaf0 = boxed_leaf(w: 3_u64);
  let leaf1 = boxed_leaf(w: 4_u64);
  let branch0 = boxed_branch(left: move leaf0, right: move leaf1);
  let total = bubble(node: &branch0);
  if total == 7_u64 {{
  }} else {{
    return std::process::exit_status(code: 1_u8);
  }}
  return std::process::exit_status(code: 0_u8);
}}
"
    );
    let ledger = ledger_of("bubble.wf", proved.as_bytes());
    // `scaled`'s own body is reported first, in source order: its proof
    // statement joins the chain the two preceding `let`s form, and its
    // array construction and consuming call are the pair that follows.
    assert_eq!(
        ledger[0],
        "PAR chain       bubble.wf:17  run(a let statement, a let statement, \
             a proof statement)  3 members through line 19"
    );
    assert_eq!(
        ledger[1],
        "PAR permitted   bubble.wf:26  pair(a let statement, array_filled)  eligible"
    );
    assert_eq!(
        ledger[2],
        "PAR chain       bubble.wf:26  run(a let statement, array_filled)  \
             2 members through line 27"
    );
    assert_eq!(
        ledger[3],
        "PAR denied      bubble.wf:27  pair(array_filled, scaled)  condition 1: \
             the write of s1 overlaps the operand read of s2 at \
             let values = array_filled::<u64, 8>(value: 1_u64); vs values"
    );
    assert_eq!(
        ledger[4],
        "PAR denied      bubble.wf:28  pair(scaled, a return statement)  condition 2: \
             the exit edge of s2 may skip the statement written after it"
    );
    assert_eq!(
        ledger[5],
        "PAR permitted   bubble.wf:32  pair(bubble, bubble)  eligible"
    );
    assert_eq!(
        ledger[6],
        "PAR chain       bubble.wf:32  run(bubble, bubble)  2 members through line 33"
    );
    assert_eq!(
        ledger[7],
        "PAR denied      bubble.wf:33  pair(bubble, a let statement)  condition 1: \
             the write of s1 overlaps the operand read of s2 at \
             let b = bubble(node: r); vs let total = a +wrap b;"
    );
    assert!(
        !ledger.iter().any(|line| line.contains("not-actualizable")),
        "the not-actualizable verdict class no longer exists:\n{}",
        ledger.join("\n")
    );

    // Both programs end with main's own two leaf allocations, which are
    // eligible and do form a chain, and then the branch call that consumes
    // them, which is denied by condition 1. The ledger is in source order,
    // so those lines follow the recursive ones and the file is fully
    // reported.
    assert_eq!(
        ledger[8],
        "PAR permitted   bubble.wf:42  pair(boxed_leaf, boxed_leaf)  eligible"
    );
    assert_eq!(
        ledger[9],
        "PAR chain       bubble.wf:42  run(boxed_leaf, boxed_leaf)  2 members through line 43"
    );
    assert_eq!(
        ledger[10],
        "PAR denied      bubble.wf:43  pair(boxed_leaf, boxed_branch)  condition 1: \
             the write of s1 overlaps the write of s2 at \
             let leaf1 = boxed_leaf(w: 4_u64); vs move leaf1"
    );
    assert_eq!(
        ledger[11],
        "PAR denied      bubble.wf:44  pair(boxed_branch, bubble)  condition 1: \
             the write of s1 overlaps the write of s2 at \
             let branch0 = boxed_branch(left: move leaf0, right: move leaf1); vs &branch0"
    );
    assert_eq!(ledger.len(), 12);
}

/// One denial line per numbered condition, each citing that condition and
/// the source text that refused the overlap. A denial arriving under the
/// wrong condition, or with an empty citation, fails here.
#[test]
fn the_permission_ledger_names_the_condition_that_refused_each_pair() {
    // Condition 1: two reference actuals resolve to one place, so the line
    // has to name both actuals as the writer wrote them. v0.60 has no
    // permission marker, so what the pair rule sees is two writes of one
    // storage rather than two exclusive loans, and the overlap is
    // reported under condition 1 [REF-1, EFF-1, PAR-1].
    let overlapping = b"fn bump(slot: &u64) -> result: u64 writes(slot) {
  let seen = deref(slot);
  set deref(slot) = 7_u64;
  return seen;
}

fn main() -> status: std::process::ExitStatus pure {
  let cell = 1_u64;
  let lo = bump(slot: &cell);
  let hi = bump(slot: &cell);
  let total = imax(lo, hi);
  return std::process::exit_status(code: 0_u8);
}
";
    assert_eq!(
        ledger_of("bump.wf", overlapping),
        vec![
            "PAR denied      bump.wf:8  pair(a let statement, bump)  condition 1: \
                 the write of s1 overlaps the write of s2 at let cell = 1_u64; vs &cell"
                .to_owned(),
            "PAR denied      bump.wf:9  pair(bump, bump)  condition 1: \
                 the write of s1 overlaps the write of s2 at &cell vs &cell"
                .to_owned(),
            "PAR denied      bump.wf:10  pair(bump, a let statement)  condition 1: \
                 the write of s1 overlaps the operand read of s2 at \
                 let hi = bump(slot: &cell); vs let total = imax(lo, hi);"
                .to_owned(),
        ]
    );

    // Affine opaque values have empty release under PRE-1 and STOR-3.
    let capability_releases =
        b"fn release_read_file(file: std::io::OutputStream) -> result: unit pure {
  return unit;
}

fn release_pair(first: std::io::OutputStream, second: std::io::OutputStream) -> result: unit pure {
  let done_first = release_read_file(file: move first);
  let done_second = release_read_file(file: move second);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
";
    assert_eq!(
            ledger_of("row.wf", capability_releases),
            vec![
                "PAR permitted   row.wf:6  pair(release_read_file, release_read_file)  eligible".to_owned(),
                "PAR chain       row.wf:6  run(release_read_file, release_read_file)  2 members through line 7".to_owned(),
                "PAR denied      row.wf:7  pair(release_read_file, a return statement)  \
                 condition 2: the exit edge of s2 may skip the statement written after it"
                    .to_owned(),
            ]
        );

    // The `propagate` edge: a `propagate` is never a window member itself
    // [PAR-1], and the ledger now reports its Err edge under condition 2
    // — the edge condition — once for each adjacent pair it stands in
    // rather than once for the two ordinary calls it separates.
    let propagating = b"fn peek(slot: &u8) -> result: u64 reads(slot) {
  return cvt::<u8, u64>(deref(slot));
}

fn stamp(slot: &u8) -> result: u64 writes(slot) {
  set deref(slot) = 9_u8;
  return 1_u64;
}

fn probe(outcome: Result<u8, NarrowError>, a: &u8, b: &u8) -> result: Result<unit, NarrowError> reads(b), writes(a) {
  let seen = peek(slot: b);
  let narrowed = propagate outcome;
  let stamped = stamp(slot: a);
  return Ok<unit, NarrowError>(value: unit);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
";
    assert_eq!(
        ledger_of("propagate.wf", propagating),
        vec![
            "PAR denied      propagate.wf:11  pair(peek, a propagate statement)  \
                 condition 2: the Err edge of s2 may skip the statement written after it"
                .to_owned(),
            "PAR denied      propagate.wf:12  pair(a propagate statement, stamp)  \
                 condition 2: the Err edge of s1 may skip the statement written after it"
                .to_owned(),
            "PAR denied      propagate.wf:13  pair(stamp, a return statement)  \
                 condition 2: the exit edge of s2 may skip the statement written after it"
                .to_owned(),
        ]
    );
}

/// A counted loop that reduces under an exactly-associative integer
/// operation is permitted, and the line names the operation the
/// accumulator recombines under.
///
/// This is the shape the pair judgment can never reach: two iterations of
/// one statement are not a pair, so before the loop rule the compiler
/// reported the most parallel loop in a program by saying nothing about
/// it. The callee is a real `pure` function with a loop of its
/// own, so the case is about the writer's loop rather than about a body
/// small enough to be uninteresting.
#[test]
fn a_counted_loop_reducing_under_an_associative_operation_is_permitted() {
    let source = b"fn interesting(index: u64) -> result: Bool pure {
  let low = iand(index, 7_u64);
  let seen = 0_u64;
  loop @spin {
    let done = seen == 4_u64;
    if done {
      break @spin;
    }
    set seen = seen +wrap 1_u64;
  }
  return low == 3_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let hits = 0_u64;
  for @scan (i in 0_u64..4096_u64) {
    let escaped = interesting(index: i);
    if escaped {
      set hits = hits +wrap 1_u64;
    }
  }
  return std::process::exit_status(code: 0_u8);
}
";
    assert_eq!(
        ledger_of("counting.wf", source),
        vec![
            "PAR chain       counting.wf:2  run(a let statement, a let statement)  \
                 2 members through line 3"
                .to_owned(),
            "PAR loop        counting.wf:16  loop  permitted   eligible; \
                 one accumulator under +wrap"
                .to_owned(),
        ]
    );
}

/// The float denial, and the reason a loop rule can exist at all.
///
/// `fadd.strict` is not associative, so an implementation free to choose
/// the combination tree would publish different bytes at a different
/// worker count — the one failure this whole path exists to make
/// impossible. The admitted set is enumerated and contains no float, so
/// the loop is refused outright rather than permitted with a hedge, and
/// the line cites the statement, which names the operation the writer
/// wrote.
#[test]
fn a_counted_loop_reducing_under_a_float_operation_is_denied_by_condition_one() {
    let source = b"fn main() -> status: std::process::ExitStatus pure {
  let total = 0.0_f64;
  let step = 0.5_f64;
  for @sum (i in 0_u64..1024_u64) {
    set total = fadd.strict(total, step);
  }
  return std::process::exit_status(code: 0_u8);
}
";
    assert_eq!(
        ledger_of("folding.wf", source),
        vec![
            "PAR chain       folding.wf:2  run(a let statement, a let statement)  \
                 2 members through line 3"
                .to_owned(),
            "PAR loop        folding.wf:4  loop  denied      condition 1: the loop writes \
                 storage outliving the iteration that no exactly associative operation reduces, \
                 at set total = fadd.strict(total, step);"
                .to_owned()
        ]
    );

    // The identical loop over an integer accumulator is permitted, so the
    // refusal above is about the operation and not about the loop.
    let integral = b"fn main() -> status: std::process::ExitStatus pure {
  let total = 0_u64;
  let step = 5_u64;
  for @sum (i in 0_u64..1024_u64) {
    set total = total +wrap step;
  }
  return std::process::exit_status(code: 0_u8);
}
";
    assert_eq!(
        ledger_of("folding.wf", integral),
        vec![
            "PAR chain       folding.wf:2  run(a let statement, a let statement)  \
                 2 members through line 3"
                .to_owned(),
            "PAR loop        folding.wf:4  loop  permitted   eligible; \
                 one accumulator under +wrap"
                .to_owned(),
        ]
    );
}

/// A counted loop whose proved index is exactly its binder is reported as
/// an eligible map with no accumulator.
#[test]
fn a_proven_counted_binder_buffer_map_is_permitted() {
    let source = b"fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u64, 64>(value: 0_u64);
  let out = slots_from_array::<u64, 64>(values: values);
  for @fill (i in 0_u64..64_u64) {
    set out[i] = i *wrap i;
  }
  return std::process::exit_status(code: 0_u8);
}
";
    assert_eq!(
        ledger_of("mapping.wf", source),
        vec![
            "PAR denied      mapping.wf:2  pair(array_filled, slots_from_array)  \
                 condition 1: the write of s1 overlaps the operand read of s2 at \
                 let values = array_filled::<u64, 64>(value: 0_u64); vs values"
                .to_owned(),
            "PAR denied      mapping.wf:3  pair(slots_from_array, a for loop)  \
                 condition 1: s2 is a for loop"
                .to_owned(),
            "PAR loop        mapping.wf:4  loop  permitted   eligible; no accumulator".to_owned(),
        ]
    );
}

/// A counted loop whose carried state is written by a callee is refused by
/// condition 2, however associative the accumulator in view is.
///
/// The loop below folds a float total through a `&uniq` parameter, beside
/// an ordinary `+wrap` counter — and the counter is what a line reading
/// only the body's `set` statements would name, permitting a loop whose
/// real carried state is a `fadd.strict` fold one frame away. The row's
/// projection onto the actual is the fact that refuses it, so the
/// enumerated combine set governs all of the loop's carried state and not
/// only the part written in view.
#[test]
fn a_counted_loop_whose_callee_writes_carried_state_is_denied_by_condition_two() {
    let source = b"fn accum(slot: &f64, x: f64) -> result: u64 writes(slot) {
  set deref(slot) = fadd.strict(deref(slot), x);
  let bits = reinterpret::<f64, u64>(deref(slot));
  return iand(bits, 1_u64);
}

fn main() -> status: std::process::ExitStatus pure {
  let total = 0.0_f64;
  let count = 0_u64;
  for @sum (i in 0_u64..8_u64) {
    let one = accum(slot: &total, x: 0.5_f64);
    set count = count +wrap one;
  }
  return std::process::exit_status(code: 0_u8);
}
";
    assert_eq!(
        ledger_of("carrying.wf", source),
        vec![
            "PAR chain       carrying.wf:8  run(a let statement, a let statement)  \
                 2 members through line 9"
                .to_owned(),
            "PAR loop        carrying.wf:10  loop  denied      condition 2: the body \
                 writes storage that is neither introduced by the iteration nor the \
                 accumulator, at &total"
                .to_owned(),
            "PAR denied      carrying.wf:11  pair(accum, a set statement)  condition 1: \
                 the write of s1 overlaps the operand read of s2 at \
                 let one = accum(slot: &total, x: 0.5_f64); vs set count = count +wrap one;"
                .to_owned(),
        ]
    );

    // The same loop over a callee that writes nothing is permitted, so the
    // refusal above is about the projected row and not about the shape.
    let reading = b"fn weigh(x: f64) -> result: u64 pure {
  let bits = reinterpret::<f64, u64>(x);
  return iand(bits, 1_u64);
}

fn main() -> status: std::process::ExitStatus pure {
  let total = 0.0_f64;
  let count = 0_u64;
  for @sum (i in 0_u64..8_u64) {
    let one = weigh(x: total);
    set count = count +wrap one;
  }
  return std::process::exit_status(code: 0_u8);
}
";
    assert_eq!(
        ledger_of("carrying.wf", reading),
        vec![
            "PAR chain       carrying.wf:7  run(a let statement, a let statement)  \
                 2 members through line 8"
                .to_owned(),
            "PAR loop        carrying.wf:9  loop  permitted   eligible; \
                 one accumulator under +wrap"
                .to_owned(),
            "PAR denied      carrying.wf:10  pair(weigh, a set statement)  condition 1: \
                 the write of s1 overlaps the operand read of s2 at \
                 let one = weigh(x: total); vs set count = count +wrap one;"
                .to_owned(),
        ]
    );
}

/// A counted loop a `give` can leave is refused by condition 4, however
/// associative its accumulator is.
///
/// `give` is the fourth exit form, and the one that leaves the enclosing
/// value initializer as well as the loop. A combination tree over the
/// whole range has no representation for that edge at all: it folds every
/// iteration where the loop stopped at the first hit. The fixture below
/// sums 64 ones, meets a 7 at index 10 and gives there, so the loop
/// contributes 10 where a full-range fold contributes 70.
#[test]
fn a_counted_loop_a_give_can_leave_is_denied_by_condition_four() {
    let source = b"fn scan_until(src: &Slots<u64, 64>, needle: u64) -> result: u64 reads(src) {
  let count = deref(src).len;
  let acc = 0_u64;
  let always = True();
  let answer = if always {
    for @scan (i in 0_u64..count) {
      let v = deref(src)[i];
      set acc = acc +wrap v;
      let hit = v == needle;
      if hit {
        give i;
      }
    }
    give 4096_u64;
  } else {
    give 4096_u64;
  }
  return answer +wrap acc;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u64, 64>(value: 1_u64);
  let data = slots_from_array::<u64, 64>(values: values);
  set data[10_u64] = 7_u64;
  let t = scan_until(src: &data, needle: 7_u64);
  return std::process::exit_status(code: 0_u8);
}
";
    assert_eq!(
        ledger_of("giving.wf", source),
        vec![
            "PAR chain       giving.wf:2  run(a let statement, a let statement, \
                 a let statement)  3 members through line 4"
                .to_owned(),
            "PAR loop        giving.wf:6  loop  denied      condition 4: a give leaves the loop"
                .to_owned(),
            "PAR chain       giving.wf:8  run(a set statement, a let statement)  \
                 2 members through line 9"
                .to_owned(),
            "PAR denied      giving.wf:22  pair(array_filled, slots_from_array)  \
                 condition 1: the write of s1 overlaps the operand read of s2 at \
                 let values = array_filled::<u64, 64>(value: 1_u64); vs values"
                .to_owned(),
            "PAR denied      giving.wf:23  pair(slots_from_array, a set statement)  \
                 condition 1: the write of s1 overlaps the write of s2 at \
                 let data = slots_from_array::<u64, 64>(values: values); vs \
                 set data[10_u64] = 7_u64;"
                .to_owned(),
            "PAR denied      giving.wf:24  pair(a set statement, scan_until)  \
                 condition 1: the write of s1 overlaps the read of s2 at \
                 set data[10_u64] = 7_u64; vs &data"
                .to_owned(),
            "PAR denied      giving.wf:25  pair(scan_until, a return statement)  \
                 condition 2: the exit edge of s2 may skip the statement written after it"
                .to_owned(),
        ]
    );

    // The same loop with the give removed is permitted, so the refusal is
    // about the exit edge and not about the shape.
    let contained = b"fn scan_until(src: &Slots<u64, 64>, needle: u64) -> result: u64 reads(src) {
  let count = deref(src).len;
  let acc = 0_u64;
  let always = True();
  let answer = if always {
    for @scan (i in 0_u64..count) {
      let v = deref(src)[i];
      set acc = acc +wrap v;
    }
    give 4096_u64;
  } else {
    give 4096_u64;
  }
  return answer +wrap acc;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u64, 64>(value: 1_u64);
  let data = slots_from_array::<u64, 64>(values: values);
  set data[10_u64] = 7_u64;
  let t = scan_until(src: &data, needle: 7_u64);
  return std::process::exit_status(code: 0_u8);
}
";
    assert_eq!(
        ledger_of("giving.wf", contained),
        vec![
            "PAR chain       giving.wf:2  run(a let statement, a let statement, \
                 a let statement)  3 members through line 4"
                .to_owned(),
            "PAR loop        giving.wf:6  loop  permitted   eligible; \
                 one accumulator under +wrap"
                .to_owned(),
            "PAR denied      giving.wf:18  pair(array_filled, slots_from_array)  \
                 condition 1: the write of s1 overlaps the operand read of s2 at \
                 let values = array_filled::<u64, 64>(value: 1_u64); vs values"
                .to_owned(),
            "PAR denied      giving.wf:19  pair(slots_from_array, a set statement)  \
                 condition 1: the write of s1 overlaps the write of s2 at \
                 let data = slots_from_array::<u64, 64>(values: values); vs \
                 set data[10_u64] = 7_u64;"
                .to_owned(),
            "PAR denied      giving.wf:20  pair(a set statement, scan_until)  \
                 condition 1: the write of s1 overlaps the read of s2 at \
                 set data[10_u64] = 7_u64; vs &data"
                .to_owned(),
            "PAR denied      giving.wf:21  pair(scan_until, a return statement)  \
                 condition 2: the exit edge of s2 may skip the statement written after it"
                .to_owned(),
        ]
    );
}

/// The split advice outlives exactly one refusal, and names each combine
/// the way a writer spells it.
///
/// Three accumulators is the one shape this version declines while a
/// hand-written recursion returning an aggregate still reaches it, so the
/// loop line reports the refusal and a second line reports the rewrite.
/// The advice is meant to be typed, so an operation named in a spelling
/// the language does not have is advice that does not compile: the `Bool`
/// row is `band`, `bor`, `bxor` [OP-1].
#[test]
fn a_refused_multi_accumulator_loop_keeps_advice_naming_the_boolean_combines() {
    let source = b"fn main() -> status: std::process::ExitStatus pure {
  let every = True();
  let any = False();
  let parity = False();
  for @scan (i in 0_u64..64_u64) {
    let low = iand(i, 1_u64);
    let bit = low == 0_u64;
    set every = band(every, bit);
    set any = bor(any, bit);
    set parity = bxor(parity, bit);
  }
  return std::process::exit_status(code: 0_u8);
}
";
    assert_eq!(
        ledger_of("booleans.wf", source),
        vec![
            "PAR chain       booleans.wf:2  run(a let statement, a let statement, \
                 a let statement)  3 members through line 4"
                .to_owned(),
            "PAR loop        booleans.wf:5  loop  denied      condition 1: the body carries \
                 3 accumulators, and this rule recombines one"
                .to_owned(),
            "PAR hint        booleans.wf:5  loop  refused by condition 1; a recursive split \
                 over its index range would be eligible, combining under band, bor, bxor"
                .to_owned(),
            "PAR chain       booleans.wf:8  run(a set statement, a set statement, \
                 a set statement)  3 members through line 10"
                .to_owned(),
        ]
    );
}

/// Only ordinary counted-loop permission remains after C2 deletes PAR-3.
#[test]
fn a_counted_loop_reports_only_its_ordinary_permission() {
    let source = b"fn main() -> status: std::process::ExitStatus pure {
  let total = 0_u64;
  for @sum (i in 0_u64..8_u64) {
    set total = total +wrap i;
  }
  return std::process::exit_status(code: 0_u8);
}
";
    assert_eq!(
        ledger_of("counting.wf", source),
        vec![
            "PAR loop        counting.wf:3  loop  permitted   eligible; one accumulator \
                 under +wrap"
                .to_owned(),
        ]
    );
}

/// A program with no analyzed pair reports nothing, and the ledger never
/// reaches the module: the same compilation with and without it emits the
/// same bytes.
#[test]
fn the_permission_ledger_is_output_beside_an_unchanged_module() {
    let source = b"fn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n";
    let (module, ledger) = compile_with_permission_ledger(
        &[SourceInput::new("quiet.wf", source)],
        CompilerLimits::default(),
        OverlapLowering::Off,
    )
    .expect("the fixture must compile");
    assert!(ledger.is_empty(), "no analyzed pair: {ledger:?}");
    let plain = compile(
        &[SourceInput::new("quiet.wf", source)],
        CompilerLimits::default(),
    )
    .expect("the fixture must compile");
    assert_eq!(module, plain);
}

/// Actualization is compile-time opt-in, and the judgment is not: the
/// judgment lines of a program full of eligible pairs are the same with
/// the option on and off, while only the `--par` module names the runtime.
///
/// What `--par` adds to the report is what it actualized — which offers it
/// kept, and what it did with each recursive component it found — and
/// those lines say so in their own first word. A compilation that
/// actualizes nothing has none of them.
///
/// This is what makes the ledger usable on a shipped build. A developer
/// reading what the compiler decided about a program is reading a property
/// of the source, not of the compilation they happened to ask for.
/// Every cyclic component a `--par` build finds is named in the ledger,
/// with what was done to it or the member that stopped it.
///
/// Plain `--par` is the first case below, because the budget it reports is
/// the shipped default: the runtime's own answer. The three cases after it
/// are the control writing that default out, pinning a starting value
/// instead, and withholding the family altogether.
///
/// The recursion budget is an actualization choice, so a reader has to be
/// able to see which recursions got a family and which kept the ordinary
/// path — an unspecialized recursion that said nothing would be
/// indistinguishable from one the compiler never noticed. The exclusions
/// are load-bearing in the same way: a splitter is cyclic, and it is the
/// reason the three map kernels of the compute scoreboard cannot be
/// touched by this at all.
#[test]
fn the_ledger_names_every_cyclic_component_and_what_the_budget_did_with_it() {
    let recursive = format!(
        "{TREE_PRELUDE}fn fold(node: &Box<BoxNode>) -> result: u64 writes(node) {{
  match deref(node).inner {{
    Leaf(w: leaf_w) => {{
      return deref(leaf_w);
    }}
    Branch(left: l, right: r, w: slot) => {{
      let a = fold(node: l);
      let b = fold(node: r);
      let total = imax(a, b);
      set deref(slot) = total;
      return total;
    }}
  }}
}}

fn main() -> status: std::process::ExitStatus pure {{
  let leaf0 = boxed_leaf(w: 3_u64);
  let leaf1 = boxed_leaf(w: 4_u64);
  let branch0 = boxed_branch(left: move leaf0, right: move leaf1);
  let total = fold(node: &branch0);
  return std::process::exit_status(code: 0_u8);
}}
"
    );
    let budgeted = |budget| OverlapLowering::OnWithRecursionBudget {
        budget,
        maximum_scalar_leaf_operations: None,
        sequential_refusal: false,
    };
    let lines = |name: &str, source: &[u8], overlap| {
        compile_with_permission_ledger(
            &[SourceInput::new(name, source)],
            CompilerLimits::default(),
            overlap,
        )
        .expect("the fixture must compile")
        .1
        .into_iter()
        .filter(|line| line.starts_with("PAR frontier"))
        .collect::<Vec<_>>()
    };
    for (overlap, summary, component) in [
        // Plain `--par` first: what the shipped default reports. The three
        // controls after it are the same mechanism written out.
        (
            OverlapLowering::On,
            "recursion budget runtime-derived  family emitted for 1 of 1 cyclic components",
            "component(fold)  budget-carrying clone family, entered with recursion budget \
                 runtime-derived",
        ),
        (
            budgeted(RecursionBudget::RuntimeDerived),
            "recursion budget runtime-derived  family emitted for 1 of 1 cyclic components",
            "component(fold)  budget-carrying clone family, entered with recursion budget \
                 runtime-derived",
        ),
        (
            budgeted(RecursionBudget::Pinned(
                std::num::NonZeroU8::new(8).unwrap(),
            )),
            "recursion budget pinned 8  family emitted for 1 of 1 cyclic components",
            "component(fold)  budget-carrying clone family, entered with recursion budget \
                 pinned 8",
        ),
        (
            budgeted(RecursionBudget::Off),
            "recursion budget off  family emitted for 0 of 1 cyclic components",
            "component(fold)  no family: recursion budget off",
        ),
    ] {
        assert_eq!(
            lines("fold.wf", recursive.as_bytes(), overlap),
            vec![
                format!("PAR frontier    {summary}"),
                format!("PAR frontier    {component}"),
            ]
        );
    }
    // A build that actualizes no compute at all reports none of this.
    assert!(
        lines("fold.wf", recursive.as_bytes(), OverlapLowering::Off).is_empty(),
        "a default build has no actualization to report"
    );

    // A splitter calls itself to halve its range, so it is a cyclic
    // component of its own — and a synthesized one, which is why a kernel
    // that reaches the runtime through a split cannot get a family.
    let counted = b"fn interesting(index: u64) -> result: Bool pure {
  let low = iand(index, 7_u64);
  return low == 3_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let hits = 0_u64;
  for @scan (i in 0_u64..4096_u64) {
    let escaped = interesting(index: i);
    if escaped {
      set hits = hits +wrap 1_u64;
    }
  }
  return std::process::exit_status(code: 0_u8);
}
";
    let split = lines(
        "counted.wf",
        counted,
        budgeted(RecursionBudget::RuntimeDerived),
    );
    assert_eq!(
        split.len(),
        2,
        "the split fixture must have exactly one cyclic component: {split:?}"
    );
    assert!(
        split[0].contains("family emitted for 0 of 1 cyclic components"),
        "{split:?}"
    );
    assert!(
        split[1].contains("is a synthesized loop function"),
        "{split:?}"
    );
}

#[test]
fn the_permission_ledger_does_not_depend_on_whether_the_lowering_is_taken() {
    let source = format!(
        "{TREE_PRELUDE}fn fold(node: &Box<BoxNode>) -> result: u64 writes(node) {{
  match deref(node).inner {{
    Leaf(w: leaf_w) => {{
      return deref(leaf_w);
    }}
    Branch(left: l, right: r, w: slot) => {{
      let a = fold(node: l);
      let b = fold(node: r);
      let total = imax(a, b);
      set deref(slot) = total;
      return total;
    }}
  }}
}}

fn main() -> status: std::process::ExitStatus pure {{
  let leaf0 = boxed_leaf(w: 3_u64);
  let leaf1 = boxed_leaf(w: 4_u64);
  let branch0 = boxed_branch(left: move leaf0, right: move leaf1);
  let total = fold(node: &branch0);
  return std::process::exit_status(code: 0_u8);
}}
"
    );
    let inputs = [SourceInput::new("fold.wf", source.as_bytes())];
    let (default, quiet_ledger) =
        compile_with_permission_ledger(&inputs, CompilerLimits::default(), OverlapLowering::Off)
            .expect("the fixture must compile");
    let (requested, loud_ledger) =
        compile_with_permission_ledger(&inputs, CompilerLimits::default(), OverlapLowering::On)
            .expect("the fixture must compile");

    let (judgment, actualization) = loud_ledger.split_at(quiet_ledger.len());
    assert_eq!(quiet_ledger, judgment);
    assert!(
        actualization
            .iter()
            .all(|line| line.starts_with("PAR actualization") || line.starts_with("PAR frontier")),
        "only actualization lines may differ between the two lowerings: {actualization:?}"
    );
    assert!(
        actualization
            .iter()
            .any(|line| line.contains("component(fold)")),
        "the fixture's recursive fold must be named with what it got: {actualization:?}"
    );
    assert!(
        quiet_ledger.iter().any(|line| line.contains("eligible")),
        "the fixture must report an eligible pair: {quiet_ledger:?}"
    );
    assert!(
        !default.contains("wf__par_"),
        "the default module must name no runtime symbol"
    );
    assert!(
        requested.contains("wf__par_acquire_lane"),
        "the requested module must offer a lane"
    );
}

#[test]
fn driver_erases_empty_formal_and_actual_groups_before_lowering() {
    let source = b"interface Empty {\n}\n\nbinding Selected : Empty {\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n";
    let llvm = compile(
        &[SourceInput::new("value.wf", source)],
        CompilerLimits::default(),
    )
    .expect("group expansion must use the ordinary lowering path");
    assert!(llvm.contains("define i32 @main(i32 %argc, ptr %argv)"));
    assert!(!llvm.contains("Empty"));
}

#[test]
fn every_pre_semantic_rejection_publishes_the_rule_its_stage_attributed() {
    // One source per pre-semantic stage that can reject source, each
    // reaching its stage's own DIAG-1 attribution. A stop that publishes
    // no rule cannot be told from a stop that cites none, so the whole
    // frontend is checked here rather than only the semantic stage.
    for (name, source, stage, rule) in [
            (
                "comment.wf",
                b"// nope\nfn probe() -> result: unit pure {\n  return unit;\n}\n".as_slice(),
                CompilationStage::Lexing,
                "FORM-4",
            ),
            (
                "tab.wf",
                b"fn probe() -> result: unit pure {\n\treturn unit;\n}\n",
                CompilationStage::Lexing,
                "FORM-2",
            ),
            // The v0.59 row here lexed `'Bad` as a malformed REGIONID. v0.60
            // has no REGIONID at all, so `'` is an ordinary non-source byte
            // and that row's subject left the language with regions. The
            // successor is the other lexical name shape FORM-3 owns: a LABEL
            // whose `@` is not followed by the IDENT shape.
            (
                "sigil.wf",
                b"fn probe() -> result: unit pure {\n  loop @Bad {\n    break @Bad;\n  }\n  return unit;\n}\n",
                CompilationStage::Lexing,
                "FORM-3",
            ),
            (
                "dollar.wf",
                b"$\nfn probe() -> result: unit pure {\n  return unit;\n}\n",
                CompilationStage::Lexing,
                "FORM-1",
            ),
            (
                "string.wf",
                b"fn probe() -> result: unit pure {\n  let text: str = \"bad\\t\";\n  return unit;\n}\n",
                CompilationStage::Lexing,
                "FORM-5",
            ),
            (
                "numeric.wf",
                b"fn probe() -> result: unit pure {\n  let value: i32 = 1e+;\n  return unit;\n}\n",
                CompilationStage::TerminalClassification,
                "FORM-5",
            ),
            (
                "construct.wf",
                b"nope value;\n\nfn probe() -> result: unit pure {\n  return unit;\n}\n",
                CompilationStage::Parsing,
                "FORM-1",
            ),
            (
                "spacing.wf",
                b"fn  main() -> result: unit pure {\n  return unit;\n}\n",
                CompilationStage::CanonicalSource,
                "FORM-2",
            ),
            (
                // The v0.22 row wrote an undeclared region on a borrow and
                // reached [OWN-3] in the resolver. v0.60 has no region
                // spelling and no [OWN-3], so the reference's own unresolved
                // root is what this stage still owns: a reference expression
                // whose place base names nothing is a resolver rejection.
                "reference-root.wf",
                b"fn probe() -> result: unit pure {\n  let value = 0_i32;\n  let borrowed = &gone;\n  return unit;\n}\n",
                CompilationStage::Resolution,
                "TYPE-5",
            ),
        ] {
            let failure = compile(&[SourceInput::new(name, source)], CompilerLimits::default())
                .expect_err("the case must be rejected");
            assert_eq!(failure.stage(), stage, "{name}: {failure}");
            assert_eq!(
                failure.kind(),
                CompilationFailureKind::Source,
                "{name}: {failure}"
            );
            assert_eq!(failure.rule_id(), Some(rule), "{name}: {failure}");
            assert!(
                failure.to_string().contains(rule),
                "{name}: published diagnostic omitted {rule}: {failure}"
            );
        }
}

#[test]
fn unrepresentable_array_is_a_target_failure_without_a_source_rule() {
    let source = b"fn main() -> status: std::process::ExitStatus pure {\n  let values = array_filled::<u8, 18446744073709551615>(value: 0_u8);\n  return std::process::exit_status(code: 0_u8);\n}\n";
    check(
        &[SourceInput::new("value.wf", source)],
        CompilerLimits::default(),
    )
    .expect("the array is source-valid before selected-target layout");
    let failure = compile(
        &[SourceInput::new("value.wf", source)],
        CompilerLimits::default(),
    )
    .expect_err("the selected target cannot represent the array object");
    assert_eq!(failure.stage(), CompilationStage::TargetLayout);
    assert_eq!(failure.kind(), CompilationFailureKind::TargetLayout);
    assert_eq!(failure.rule_id(), None);
    assert!(failure.detail().contains("Unrepresentable"));
}

#[test]
fn a_loop_frame_outside_the_selected_address_domain_stays_a_target_failure() {
    use crate::target::{TargetLayout, TargetLayoutFailure, TargetObject};

    let source = br#"fn folded(values: Array<u8, 216>) -> result: u64 pure {
  let total = 0_u64;
  for (i in 0_u64..2_u64) {
    let copied = values;
    let byte = copied[0_u64];
    let word = cvt::<u8, u64>(byte);
    set total = total +wrap word;
  }
  return total;
}

fn main() -> status: std::process::ExitStatus pure {
  let values = array_filled::<u8, 216>(value: 17_u8);
  let total = folded(values: values);
  return std::process::exit_status(code: 0_u8);
}
"#;
    let target = TargetLayout::host()
        .expect("supported test target")
        .with_address_index_max_for_test(255);
    let failure = super::with_checked_program(
        &[SourceInput::new("frame.wf", source)],
        None,
        CompilerLimits::default(),
        |checked, _| {
            crate::lower_checked_with_layout(checked, crate::OverlapLowering::On, target)
                .map(|_| ())
                .map_err(super::CompilationFailure::lowering)
        },
    )
    .expect_err("the complete 256-byte frame exceeds the selected address domain");
    assert_eq!(failure.stage(), CompilationStage::TargetLayout);
    assert_eq!(failure.kind(), CompilationFailureKind::TargetLayout);
    assert_eq!(failure.rule_id(), None);
    assert!(failure.detail().contains("ParallelLaneFrame"));
    assert_eq!(
        crate::LoweringFailure::from(TargetLayoutFailure::Unrepresentable(
            TargetObject::ParallelLaneFrame
        )),
        crate::LoweringFailure::TargetLayout(TargetLayoutFailure::Unrepresentable(
            TargetObject::ParallelLaneFrame
        )),
    );
    assert_eq!(
        crate::LoweringFailure::from(TargetLayoutFailure::InvalidIr),
        crate::LoweringFailure::InvalidCheckedProgram,
        "malformed compiler data must not be published as a target-domain failure",
    );
}

#[test]
fn u16_buffer_whose_proved_count_exceeds_the_target_byte_domain_is_a_target_failure() {
    let source = br#"fn bounded_count(n: u64) -> result: u64 pure contract {
  ensures result <= 5000000000000000000_u64;
} {
  if n <= 5000000000000000000_u64 {
    return n;
  } else {
    return 5000000000000000000_u64;
  }
}

fn make(n: u64) -> result: Box<Array<u16>> pure {
  let bounded = bounded_count(n: n);
  return box_array_filled::<u16>(count: bounded, value: 0_u16);
}

fn main() -> status: std::process::ExitStatus pure {
  let values = make(n: 4_u64);
  return std::process::exit_status(code: 0_u8);
}
"#;
    check(
        &[SourceInput::new("value.wf", source)],
        CompilerLimits::default(),
    )
    .expect("the OP-9 proof is accepted before selected-target qualification");
    let failure = compile(
        &[SourceInput::new("value.wf", source)],
        CompilerLimits::default(),
    )
    .expect_err("the proved u16 byte ceiling exceeds the selected target domain");
    assert_eq!(failure.stage(), CompilationStage::TargetLayout);
    assert_eq!(failure.kind(), CompilationFailureKind::TargetLayout);
    assert_eq!(failure.rule_id(), None);
    assert!(failure.detail().contains("RuntimeSizedAllocation"));
}

#[test]
fn complete_frame_is_checked_after_each_slot_layout_succeeds() {
    let source = b"fn main() -> status: std::process::ExitStatus pure {\n  let left = array_filled::<u8, 4611686018427387904>(value: 0_u8);\n  let right = array_filled::<u8, 4611686018427387904>(value: 0_u8);\n  return std::process::exit_status(code: 0_u8);\n}\n";
    let failure = compile(
        &[SourceInput::new("value.wf", source)],
        CompilerLimits::default(),
    )
    .expect_err("two individually representable slots cannot form one target frame");
    assert_eq!(failure.stage(), CompilationStage::TargetLayout);
    assert_eq!(failure.kind(), CompilationFailureKind::TargetLayout);
    assert_eq!(failure.rule_id(), None);
    assert!(failure.detail().contains("StackFrame"));
}

// C2 replaces the SYS/QUAL and FN-7 entry assertions with PRE-1 ordinary
// declarations and an optional build caller; unit results are ordinary.
#[test]
fn prelude_functions_and_unit_results_use_the_normal_call_path() {
    for source in [
        b"fn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n"
            .as_slice(),
        b"fn main() -> result: unit pure {\n  return unit;\n}\n",
    ] {
        let llvm = compile(
            &[SourceInput::new("entry.wf", source)],
            CompilerLimits::default(),
        )
        .expect("an ordinary callable signature can be selected by the build caller");
        assert!(llvm.contains("define i32 @main(i32 %argc, ptr %argv)"));
    }
    // Which rule owns each of these two swapped in v0.60: [EFF-1] now
    // admits a row entry only over a reference parameter, so a row naming
    // an `own` parameter is that rule's own rejection, and the declared
    // write through a reference the body never writes is [EFF-2]'s
    // exactness. Both are still rejections of the same two sources.
    for (source, rule) in [
        (
            b"fn probe(args: std::text::Args) -> result: unit reads(args) {\n  return unit;\n}\n".as_slice(),
            "EFF-1",
        ),
        (
            b"fn probe(file: &std::fs::ReadFile) -> result: unit writes(file) {\n  return unit;\n}\n",
            "EFF-2",
        ),
    ] {
        let failure = compile(
            &[SourceInput::new("rejected.wf", source)],
            CompilerLimits::default(),
        )
        .expect_err("ordinary parameter rows retain their exactness and mode checks");
        assert_eq!(failure.stage(), CompilationStage::Semantics);
        assert_eq!(failure.kind(), CompilationFailureKind::Source);
        assert_eq!(failure.rule_id(), Some(rule));
    }
}

#[test]
fn compiler_independent_negative_cases_keep_their_semantic_rule() {
    for (name, source, rule) in [
        (
            "gram11-neg-misspelled.wf",
            include_bytes!("../../../tests/conformance/cases/gram11-neg-misspelled.wf").as_slice(),
            "GRAM-11",
        ),
        (
            "eff2-neg-declared-unexhibited.wf",
            include_bytes!("../../../tests/conformance/cases/eff2-neg-declared-unexhibited.wf")
                .as_slice(),
            "EFF-2",
        ),
        // `fn2-neg-implicit-instantiation.wf` sat here until 2026-08-08,
        // when the case was retired: A1 respelled its violation out of
        // existence, so it compiled at exit 0 and this row could never
        // hold again. Its FN-2 content lives at
        // `fn2-neg-eeq-implicit-type`, repurposed onto a user-generic
        // call. The entry goes with the case it names rather than being
        // an assertion dropped on its own.
        (
            "form7-neg-out-of-range.wf",
            include_bytes!("../../../tests/conformance/cases/form7-neg-out-of-range.wf").as_slice(),
            "FORM-7",
        ),
        (
            "type5-neg-arg-mismatch.wf",
            include_bytes!("../../../tests/conformance/cases/type5-neg-arg-mismatch.wf").as_slice(),
            "TYPE-5",
        ),
        (
            "x-struct-neg-field-order.wf",
            include_bytes!("../../../tests/conformance/cases/x-struct-neg-field-order.wf")
                .as_slice(),
            "GRAM-8",
        ),
        (
            "x-match-gram10-out-of-order-fields.wf",
            include_bytes!(
                "../../../tests/conformance/cases/x-match-gram10-out-of-order-fields.wf"
            )
            .as_slice(),
            "GRAM-10",
        ),
        (
            "err2-neg-missing-variant.wf",
            include_bytes!("../../../tests/conformance/cases/err2-neg-missing-variant.wf")
                .as_slice(),
            "ERR-2",
        ),
        (
            "x-ownmove-partial-move-kills-binding.wf",
            include_bytes!(
                "../../../tests/conformance/cases/x-ownmove-partial-move-kills-binding.wf"
            )
            .as_slice(),
            "OWN-1",
        ),
        (
            "x-ownmove-payload-binder-consumed-twice.wf",
            include_bytes!(
                "../../../tests/conformance/cases/x-ownmove-payload-binder-consumed-twice.wf"
            )
            .as_slice(),
            "OWN-1",
        ),
        (
            "x-gram-construct-repeated-field.wf",
            include_bytes!("../../../tests/conformance/cases/x-gram-construct-repeated-field.wf")
                .as_slice(),
            "GRAM-8",
        ),
        (
            "x-gram-construct-missing-field.wf",
            include_bytes!("../../../tests/conformance/cases/x-gram-construct-missing-field.wf")
                .as_slice(),
            "GRAM-8",
        ),
        (
            "x-typ-match-foreign-variant.wf",
            include_bytes!("../../../tests/conformance/cases/x-typ-match-foreign-variant.wf")
                .as_slice(),
            "TYPE-6",
        ),
        (
            "x-match-give1-wrong-type.wf",
            include_bytes!("../../../tests/conformance/cases/x-match-give1-wrong-type.wf")
                .as_slice(),
            // Moved TYPE-5 -> GIVE-1 by the 2026-08-08 M3b dispositions
            // ruling (d), source unchanged. The manifest row was updated
            // then and this second witness was not, which is exactly the
            // desync it exists to catch — so it is updated by hand against
            // the ruling, never derived from the manifest.
            "GIVE-1",
        ),
        (
            "x-integ-give-in-statement-match-rejected.wf",
            include_bytes!(
                "../../../tests/conformance/cases/x-integ-give-in-statement-match-rejected.wf"
            )
            .as_slice(),
            "GIVE-1",
        ),
    ] {
        let failure = compile(&[SourceInput::new(name, source)], CompilerLimits::default())
            .expect_err("negative conformance case must reject");
        assert_eq!(
            failure.stage(),
            CompilationStage::Semantics,
            "{name}: {failure}"
        );
        assert_eq!(
            failure.kind(),
            CompilationFailureKind::Source,
            "{name}: {failure}"
        );
        assert_eq!(failure.rule_id(), Some(rule), "{name}: {failure}");
        assert!(
            failure.to_string().contains(rule),
            "{name}: published diagnostic omitted {rule}: {failure}"
        );
    }
}

#[test]
fn retired_program_kind_spellings_reject_in_parsing_with_form1() {
    for (name, source) in [
        (
            "reject-form1-service-leading-construct.wf",
            include_bytes!(
                "../../../tests/conformance/cases/reject-form1-service-leading-construct.wf"
            )
            .as_slice(),
        ),
        (
            "reject-form1-embedded-leading-construct.wf",
            include_bytes!(
                "../../../tests/conformance/cases/reject-form1-embedded-leading-construct.wf"
            )
            .as_slice(),
        ),
        (
            "reject-form1-daemon-leading-construct.wf",
            include_bytes!(
                "../../../tests/conformance/cases/reject-form1-daemon-leading-construct.wf"
            )
            .as_slice(),
        ),
    ] {
        let failure = compile(&[SourceInput::new(name, source)], CompilerLimits::default())
            .expect_err("a retired program-kind spelling must reject");
        assert_eq!(
            failure.stage(),
            CompilationStage::Parsing,
            "{name}: {failure}"
        );
        assert_eq!(
            failure.kind(),
            CompilationFailureKind::Source,
            "{name}: {failure}"
        );
        assert_eq!(failure.rule_id(), Some("FORM-1"), "{name}: {failure}");
        assert!(
            failure.to_string().contains("FORM-1"),
            "{name}: published diagnostic omitted FORM-1: {failure}"
        );
    }
}

// -----------------------------------------------------------------------
// Batch 0100: the payloads the verification re-writer of 2026-08-28 asked
// for, each pinned by the exact rendered text a writer reads. A change to
// any of these sentences is a change to what the compiler teaches, and has
// to be made here on purpose.
// -----------------------------------------------------------------------

/// The complete record of one compilation that must fail, exactly as
/// `whitefootc` prints it: its summary line cites `error[RULE]`, and every
/// field is one `\n  label: value` line.
fn rejection(name: &str, source: &[u8]) -> String {
    compile(&[SourceInput::new(name, source)], CompilerLimits::default())
        .expect_err("this fixture exists to be rejected")
        .to_string()
}

/// [GRAM-9] names the binding form its grammar position admits.
///
/// The rule's own repair is "bind the computed value with a preceding
/// `let`", and inside a `contract_block` that repair is wrong: the block
/// has no `let_stmt` and its binding form is `define IDENT = expr;`. The
/// position is read from the open production frames, never from the text.
#[test]
fn a_forbidden_atom_names_the_binding_form_its_grammar_position_admits() {
    let body = rejection(
        "body.wf",
        br#"fn double(value: u64) -> out: u64 pure {
  return value +wrap value;
}

fn helper(value: u64) -> out: u64 pure {
  let a = double(value: double(value: value));
  return a;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
    assert!(body.contains("[GRAM-9]"), "{body}");
    assert!(
            body.contains(
                "\n  mechanical_fix: a `call` or `construct` in an atom position does not derive [GRAM-9]: bind the inner call with its own preceding `let` in this body and write that binder in the atom position — `let inner = f(x: 0_u64); let outer = g(y: inner);`"
            ),
            "{body}"
        );

    let contract = rejection(
        "contract.wf",
        br#"fn count(data: &[u8], start: u64, end: u64) -> lines: u64 reads(data) contract {
  requires imax(start, imin(start, end)) <= end;
} {
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
    assert!(contract.contains("[GRAM-9]"), "{contract}");
    assert!(
            contract.contains(
                "\n  mechanical_fix: a `call` or `construct` in an atom position does not derive [GRAM-9]: a `contract_block` has no `let`, so bind the inner call with a preceding `define` in this same block and write that binder in the atom position — `define inner = f(x: 0_u64); requires g(y: inner);`"
            ),
            "{contract}"
        );
}

/// The `define` route the contract-block repair names is accepted.
///
/// A repair the compiler refuses is worse than no repair, so the two are
/// pinned together: the rejection above and the program below differ only
/// by taking it.
#[test]
fn the_contract_block_repair_gram9_names_is_accepted() {
    compile(
        &[SourceInput::new(
            "repaired.wf",
            br#"fn count(data: &[u8], start: u64, end: u64) -> lines: u64 pure contract {
  define spare = deref(data).len;
  requires end <= spare;
} {
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        )],
        CompilerLimits::default(),
    )
    .expect("the repair GRAM-9 names must be accepted");
}

/// [EFF-1] states the condition the row failed and the row that repairs it.
///
/// Retired subject: the v0.59 defect this case showed was `writes(cwd),
/// writes(out)` — two occurrences of one category, which that version's
/// row forbade. v0.60 admits a category more than once in one row, so
/// that source is no longer a defect and the sentence it published no
/// longer exists. The successor pinned here is the condition the rule
/// does still state: a row lists each path at most once per category.
#[test]
fn an_effect_row_defect_names_its_condition_and_the_row_that_repairs_it() {
    let detail = rejection(
            "row.wf",
            br#"fn probe(cwd: &u64, out: &u64) -> status: std::process::ExitStatus writes(cwd), writes(cwd), writes(out) {
  set deref(cwd) = 1_u64;
  set deref(out) = 2_u64;
  return std::process::exit_status(code: 0_u8);
}
"#,
        );
    assert!(detail.contains("[EFF-1]"), "{detail}");
    assert!(
            detail.contains(
                "\n  reason: a row lists each path at most once per category, and this entry repeats one\n"
            ),
            "{detail}"
        );
    assert!(
        detail.contains("\n  mechanical_fix: delete the repeated entry"),
        "{detail}"
    );
}

/// [EFF-2] publishes both rows and the exact difference between them.
///
/// Four blind-writer rounds met a bare `EffectMismatch`: the writer was
/// told two rows differ and had to derive both sides by hand.
#[test]
fn an_effect_mismatch_publishes_both_rows_and_the_exact_difference() {
    let detail = rejection(
        "effects.wf",
        br#"fn count(data: &[u8]) -> lines: u64 reads(data) {
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
    assert!(detail.contains("[EFF-2]"), "{detail}");
    assert!(
            detail.contains(
                "\n  expected_row: pure\n  found_row: reads(data)\n  missing: []\n  extra: [reads(data)]\n"
            ),
            "{detail}"
        );
    assert!(
            detail.ends_with(
                "\n  mechanical_fix: declare the row as `pure`, which covers every access the body makes and no other"
            ),
            "{detail}"
        );
}

/// [TYPE-5] publishes the two sides it compared.
#[test]
fn a_type_mismatch_publishes_the_type_required_and_the_type_written() {
    let detail = rejection(
        "types.wf",
        br#"fn main() -> status: std::process::ExitStatus pure {
  let a = 1_u64;
  let b = 2_u32;
  let c = a <= b;
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
    assert!(detail.contains("[TYPE-5]"), "{detail}");
    assert!(
        detail.starts_with("types.wf:4:16: error[TYPE-5]: TypeMismatch\n"),
        "{detail}"
    );
    assert!(
        detail.ends_with("\n  expected: own u64\n  found: own u32"),
        "{detail}"
    );
}

/// A generic form written with no type arguments names both spellings that
/// carry them.
///
/// A writer meeting this at `Ok(value: v)` sees a constructor name and no
/// type anywhere, so naming the type spelling alone would not locate the
/// repair.
#[test]
fn a_generic_form_without_type_arguments_names_both_spellings() {
    let detail = rejection(
        "result.wf",
        br#"fn helper(value: u8) -> out: Result<u8, unit> pure {
  return Ok(value: value);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
    assert!(detail.contains("[TYPE-5]"), "{detail}");
    assert!(
            detail.contains(
                "\n  expected: Result with both type arguments written: as a type `Result<u64, IoError>`, and as a variant constructor `Ok<u64, IoError>(value: v)`\n  found: Result with no written type-argument list"
            ),
            "{detail}"
        );
    // And the spelling it names is accepted.
    compile(
        &[SourceInput::new(
            "result-repaired.wf",
            br#"fn helper(value: u8) -> out: Result<u8, unit> pure {
  return Ok<u8, unit>(value: value);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        )],
        CompilerLimits::default(),
    )
    .expect("the constructor spelling TYPE-5 names must be accepted");
}

/// A reference is never a result, and the grammar says so at the result
/// type.
///
/// Retired subject: [OWN-10]'s `InvalidBorrowLifetime` sentence, which
/// named a region, the binder whose storage it outlived, and the
/// `region 'r { .. }` repair. v0.60 has no regions, no region parameters
/// and no lifetimes, so that rule, that sentence and that repair all left
/// the language. The successor pinned here is [REF-3]'s own refusal,
/// which the grammar reaches first: `type` has no reference production
/// [GRAM-3], so a written reference result stops at the result type with
/// the spelling the position does admit, including a qualified type's
/// leading module alias or `pkg` [GRAM-3, MOD-3].
#[test]
fn a_reference_result_is_refused_at_the_result_type() {
    let detail = rejection(
        "reference-result.wf",
        br#"fn caller(anchor: &u64) -> out: &u64 pure {
  return anchor;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
    assert!(detail.contains("[GRAM-3]"), "{detail}");
    assert!(
            detail.contains(
                r#"expected: [IDENT, TYPEID, "pkg", "std", "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64", "unit"]"#
            ),
            "{detail}"
        );
    assert!(
            detail.contains(
                "reference-result.wf:1:33: error[GRAM-3]: UnexpectedToken\n  source: fn caller(anchor: &u64) -> out: &u64 pure {\n"
            ),
            "{detail}"
        );
}

/// [FORM-2] quotes the line its offending bytes are in.
///
/// The coordinate is the trivia gap between two terminals, and a gap that
/// carries a line break starts at the end of the line *before* the one the
/// writer must edit: the verification writer was shown the enclosing item's
/// header with a byte offset two lines further down.
#[test]
fn a_canonical_gap_quotes_the_line_its_offending_bytes_are_in() {
    let detail = rejection(
            "indent.wf",
            b"fn helper(value: u64) -> out: u64 pure {\n  let a = value +wrap 1_u64;\n    let b = a +wrap 2_u64;\n  return b;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n",
        );
    assert!(detail.contains("[FORM-2]"), "{detail}");
    assert!(
            detail.starts_with("indent.wf:3:1: error[FORM-2]: NonCanonicalTrivia\n  source:     let b = a +wrap 2_u64;\n  marker: ^^^^\n"),
            "{detail}"
        );

    // A gap that stays inside one line is unchanged: the reader is sent to
    // the first byte of the gap, which is where the wrong bytes begin.
    let inline = rejection(
            "spacing.wf",
            b"fn helper(value: u64) -> out: u64 pure {\n  let a = value +wrap 1_u64;\n  let b = a  +wrap 2_u64;\n  return b;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n",
        );
    assert!(
            inline.starts_with(&format!(
                "spacing.wf:3:12: error[FORM-2]: NonCanonicalTrivia\n  source:   let b = a  +wrap 2_u64;\n  marker: {}^^\n",
                " ".repeat(11)
            )),
            "{inline}"
        );
}

/// [MOD-10] the standard library's module table the compiler carries is
/// exactly what forming the library's own graph record gives, row for row
/// and edge for edge, through the stages every graph record passes.
#[test]
fn the_carried_library_table_is_the_library_graph() {
    let formed = super::form_graph_record(
        SourceInput::new(crate::library::GRAPH_PATH, crate::library::GRAPH.as_bytes()),
        crate::CompilerLimits::default(),
        crate::Package::Standard,
        None,
    )
    .expect("the standard library's graph record forms");
    assert_eq!(formed.modules(), crate::library::modules(0).as_slice());
    assert!(formed.entries().is_empty());
}

/// [MOD-10] inside the standard library `pkg` names the library itself, so
/// a library graph writing `std` is refused, and so is a program graph
/// writing a `std` path that names no library module.
#[test]
fn the_library_graph_writes_pkg_and_a_program_names_only_library_modules() {
    let refused = super::form_graph_record(
        SourceInput::new("std/modules.wfg", b"pkg::io: [];\npkg::fs: [std::io];\n"),
        crate::CompilerLimits::default(),
        crate::Package::Standard,
        None,
    )
    .expect_err("a library graph writing std is refused");
    assert_eq!(refused.rule_id(), Some("MOD-10"));
    let unknown = crate::form_module_graph(
        SourceInput::new("modules.wfg", b"pkg: [std::nothing];\n"),
        crate::CompilerLimits::default(),
    )
    .expect_err("a std dependency naming no library module is refused");
    assert_eq!(unknown.rule_id(), Some("MOD-10"));
    let graph = crate::form_module_graph(
        SourceInput::new("modules.wfg", b"pkg::a: [std::io];\npkg: [pkg::a];\n"),
        crate::CompilerLimits::default(),
    )
    .expect("a row may list a library module");
    let a = graph.module_named("pkg::a").expect("pkg::a is registered");
    let io = graph
        .module_named("std::io")
        .expect("the library's modules follow");
    assert!(graph.modules()[a.index()].depends_on(io));
    assert_eq!(graph.program_modules().count(), 2);
}

/// [MOD-8, MOD-10] a standard library module's verdict key is the same in
/// every program that selects it: a program module registered below a path
/// the library also uses is no child of the library module, so it enters
/// neither the library module's graph facts nor its key.
#[test]
fn a_library_module_verdict_key_ignores_the_programs_modules() {
    let material = |graph_text: &[u8]| {
        let graph = crate::form_module_graph(
            SourceInput::new("modules.wfg", graph_text),
            crate::CompilerLimits::default(),
        )
        .expect("the graph forms");
        let inputs = super::with_library_records(&graph, &[]);
        let io = graph
            .module_named("std::io")
            .expect("std::io is selectable");
        super::ModuleCheck::new(&graph, &inputs, io, false).material
    };
    let alone = material(b"pkg: [std::io];\n");
    let beside = material(b"pkg::io: [];\npkg::io::x: [];\npkg: [pkg::io, pkg::io::x, std::io];\n");
    assert_eq!(
        String::from_utf8_lossy(&alone),
        String::from_utf8_lossy(&beside)
    );
}

/// [MOD-8] a function's interface declaration and its definition are one
/// function to a proof receipt. A module verdict reads another module's
/// function through that module's interface, and the composition reads the
/// function's definition; spelled alike, every analysis the verdicts
/// recorded is reused by the entry build, which analyzes nothing afresh.
#[test]
fn an_entry_build_reuses_every_analysis_its_module_verdicts_recorded() {
    let directory = CacheDirectory::new("receipt-sharing");
    let cache = directory.open();
    let graph = crate::form_module_graph(
        SourceInput::new("modules.wfg", PROGRAM_GRAPH),
        CompilerLimits::default(),
    )
    .expect("the graph forms");
    let records: Vec<(&str, &[u8])> = vec![
        ("base/module.wfm", BASE_INTERFACE),
        ("base/half.wf", BASE_BODY),
        ("user/module.wfm", USER_INTERFACE),
        ("user/use.wf", USER_BODY),
        ("tool/module.wfm", TOOL_INTERFACE),
        ("tool/spare.wf", TOOL_BODY),
        ("module.wfm", ROOT_INTERFACE),
        ("main.wf", ROOT_BODY),
    ];
    let inputs = module_inputs(&graph, &records);
    let modules = graph.program_modules().collect::<Vec<_>>();
    super::require_module_verdicts(
        &graph,
        &super::with_library_records(&graph, &inputs),
        &modules,
        CompilerLimits::default(),
        Some(&cache),
    )
    .expect("every module is accepted");
    let (_, recorded) = cache.receipt_counts();
    super::build_module_entry(
        &graph,
        &inputs,
        super::ModuleEntry::Named("app"),
        CompilerLimits::default(),
        OverlapLowering::Off,
        Some(&cache),
    )
    .expect("the entry builds");
    let (reused, rebuilt) = cache.receipt_counts();
    assert!(reused > 0, "the entry build reads the verdicts' receipts");
    assert_eq!(
        rebuilt, recorded,
        "the entry build analyzes no function its module verdicts analyzed"
    );
}
