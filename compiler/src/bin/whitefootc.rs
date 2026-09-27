#![forbid(unsafe_code)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use whitefoot::{
    Architecture, BuildCache, COMPLETION_BRIDGE_HEADER, COMPLETION_BRIDGE_SOURCE,
    COMPLETION_CONTRACT_HEADER, COMPLETION_FILE_ADAPTER_HEADER, COMPLETION_FILE_ADAPTER_SOURCE,
    COMPLETION_FILE_POSIX_HEADER, COMPLETION_LINUX_IO_URING_HEADER, COMPLETION_RUNTIME_SOURCE,
    COMPLETION_SOCKET_ADDRESS_HEADER, COMPLETION_WINDOWS_IOCP_HEADER, CheckOutcome, CheckVerdict,
    CompilationFailure, CompilerLimits, DiagnosticFormat, FLOOR_STACK_BYTES, FragmentGranularity,
    GRAPH_FILE_NAME, HOST_OPTIMIZATION_ARGUMENTS, ModuleEntry, ORDINARY_VALUES_HEADER,
    ORDINARY_VALUES_LLVM, ORDINARY_VALUES_SOURCE, OverlapLowering, RecursionBudget,
    SCHED_CORE_HEADER, SCHED_CORE_SOURCE, SCHED_ENTRY_HEADER, SCHED_ENTRY_SOURCE,
    SCHED_PRIM_HEADER, SourceInput, WINDOWS_RUNTIME_HEADER, build_module_entry, check,
    check_module_program, check_with_cache, compile_with_cache, compile_with_overlap,
    compile_with_permission_ledger, content_digest, discover_module_sources, entry_verdict,
    form_module_graph, module_verdict, read_graph_record, render_driver_failure,
    render_module_interface, running_compiler_identity, split_module, stack_ledger,
};

// `HOST_LINK_LIBRARIES` is here rather than above because its one reader is
// this platform's `TARGET_LINK_LIBRARIES`: Windows names no library of its
// own, so an unconditional import of it is an unused import there, and that
// is a clippy error on the host that would find it last.
#[cfg(not(target_os = "windows"))]
use whitefoot::{
    COMPLETION_FILE_POSIX_SOURCE, COMPLETION_LINUX_IO_URING_SOURCE, COMPLETION_WAIT_HOST_SOURCE,
    FLOOR_RUNTIME_SOURCE, HOST_LINK_LIBRARIES, SCHED_PRIM_HOST_SOURCE,
};

#[cfg(target_os = "windows")]
use whitefoot::{
    COMPLETION_FILE_WINDOWS_SOURCE, COMPLETION_WAIT_WINDOWS_SOURCE, COMPLETION_WINDOWS_IOCP_SOURCE,
    FLOOR_WINDOWS_RUNTIME_SOURCE, SCHED_PRIM_WINDOWS_SOURCE, WINDOWS_RUNTIME_SOURCE,
};

const USAGE: &str = "usage: whitefootc [--emit-llvm] [--par] [--par-scalar-leaf-limit N|off] [--par-sequential-refusal] [--par-recursive-frontier auto|N|off] [--no-overlap] [--par-ledger] \
[--stack-ledger] [--diagnostic-format text|json] [--check] [--cache DIR [--fragments module|function] | --full-lto] [--report] [-o OUTPUT] (SOURCE... | --graph modules.wfg [--entry NAME | --function pkg::module::name | --check-module pkg::module | --check-interface pkg::module | --check-modules | --render-interface pkg::module | --compare-interface pkg::module --against OTHER/modules.wfg])";

// The compiler walks typed source and lowering trees recursively. Windows
// gives the process's primary thread a 1 MiB stack by default, which is small
// enough for an ordinary region-and-match-heavy program to exhaust while the
// same source compiles on the other hosts. Own the driver thread's stack so a
// source program's acceptance does not depend on the host executable format.
const COMPILER_DRIVER_STACK_BYTES: usize = 8 * 1024 * 1024;

fn clang_executable() -> &'static str {
    if cfg!(target_os = "windows") {
        "clang"
    } else {
        "/usr/bin/clang"
    }
}

/// One compiler-owned runtime file and the path its quoted includes expect it
/// to have below the driver's private staging root.
///
/// The sources deliberately keep the repository's `backend/` topology:
/// `completion/bridge.c` reaches the scheduler core as `../sched/core.h` and
/// the Windows host runtime as `../windows_runtime.h`, so flattening these
/// bytes into one directory would change the meaning of those includes even
/// though every required file was written.
#[derive(Clone, Copy)]
struct RuntimeUnit {
    relative_path: &'static str,
    source: &'static str,
}

const fn unit(relative_path: &'static str, source: &'static str) -> RuntimeUnit {
    RuntimeUnit {
        relative_path,
        source,
    }
}

/// One rule holds for all six lists below, and it is what makes the staged
/// tree the same shape on every platform: *every header of a group is shared*,
/// and only the group's `.c` files and its clang input list are this
/// platform's.
///
/// A header is inert -- it is never a clang input, and it reaches the compiler
/// only when a compiled unit includes it -- so staging one a link will not
/// open costs a write and nothing else. Staging one a link *does* open is not
/// optional, and the two are not distinguishable by platform: `bridge.c` is
/// one shared unit whose `#if` arms name `windows_iocp.h` and
/// `../windows_runtime.h` in text that a POSIX build never compiles but that
/// any reader, and `runtime_staging_closes_every_quoted_include`, still walks.
/// Splitting headers by platform would make that walk answer differently on
/// two hosts for one set of bytes.
///
/// The units every program links, whatever it does: the resource-exhaustion
/// floor, and on Windows the host runtime whose bootstrap the emitted module
/// names.
const FLOOR_SHARED_UNITS: &[RuntimeUnit] = &[unit("windows_runtime.h", WINDOWS_RUNTIME_HEADER)];

#[cfg(not(target_os = "windows"))]
const FLOOR_PLATFORM_UNITS: &[RuntimeUnit] = &[unit("wf_floor.c", FLOOR_RUNTIME_SOURCE)];
#[cfg(not(target_os = "windows"))]
const FLOOR_COMPILE_UNITS: &[&str] = &["wf_floor.c"];

#[cfg(target_os = "windows")]
const FLOOR_PLATFORM_UNITS: &[RuntimeUnit] = &[
    unit("wf_floor_windows.c", FLOOR_WINDOWS_RUNTIME_SOURCE),
    unit("windows_runtime.c", WINDOWS_RUNTIME_SOURCE),
];
#[cfg(target_os = "windows")]
const FLOOR_COMPILE_UNITS: &[&str] = &["wf_floor_windows.c", "windows_runtime.c"];

/// The scheduler core and the platform layer over it. One core, one entry, and
/// one leaf per platform holding the seven primitives
/// (`research/investigations/io-model/PARK-ON-MISS.md` section 7.1).
const CORE_SHARED_UNITS: &[RuntimeUnit] = &[
    unit("sched/core.h", SCHED_CORE_HEADER),
    unit("sched/prim.h", SCHED_PRIM_HEADER),
    unit("sched/entry.h", SCHED_ENTRY_HEADER),
    unit("sched/core.c", SCHED_CORE_SOURCE),
    unit("sched/entry.c", SCHED_ENTRY_SOURCE),
];

#[cfg(not(target_os = "windows"))]
const CORE_PLATFORM_UNITS: &[RuntimeUnit] = &[unit("sched/prim_host.c", SCHED_PRIM_HOST_SOURCE)];
#[cfg(not(target_os = "windows"))]
const CORE_COMPILE_UNITS: &[&str] = &["sched/core.c", "sched/prim_host.c", "sched/entry.c"];

#[cfg(target_os = "windows")]
const CORE_PLATFORM_UNITS: &[RuntimeUnit] =
    &[unit("sched/prim_windows.c", SCHED_PRIM_WINDOWS_SOURCE)];
#[cfg(target_os = "windows")]
const CORE_COMPILE_UNITS: &[&str] = &["sched/core.c", "sched/prim_windows.c", "sched/entry.c"];

/// The completion runtime: one wake epoch, one bridge, one file adapter, every
/// platform's header, and per platform the wait set, the host leaf of the
/// adapter, and the kernel completion ring.
const COMPLETION_SHARED_UNITS: &[RuntimeUnit] = &[
    unit("completion/contract.h", COMPLETION_CONTRACT_HEADER),
    unit("completion/file_adapter.h", COMPLETION_FILE_ADAPTER_HEADER),
    unit("completion/bridge.h", COMPLETION_BRIDGE_HEADER),
    unit("completion/file_posix.h", COMPLETION_FILE_POSIX_HEADER),
    unit(
        "completion/socket_address.h",
        COMPLETION_SOCKET_ADDRESS_HEADER,
    ),
    unit(
        "completion/linux_io_uring.h",
        COMPLETION_LINUX_IO_URING_HEADER,
    ),
    unit("completion/windows_iocp.h", COMPLETION_WINDOWS_IOCP_HEADER),
    unit("completion/runtime.c", COMPLETION_RUNTIME_SOURCE),
    unit("completion/file_adapter.c", COMPLETION_FILE_ADAPTER_SOURCE),
    unit("completion/bridge.c", COMPLETION_BRIDGE_SOURCE),
];

#[cfg(not(target_os = "windows"))]
const COMPLETION_PLATFORM_UNITS: &[RuntimeUnit] = &[
    unit("completion/wait_host.c", COMPLETION_WAIT_HOST_SOURCE),
    unit("completion/file_posix.c", COMPLETION_FILE_POSIX_SOURCE),
    unit(
        "completion/linux_io_uring.c",
        COMPLETION_LINUX_IO_URING_SOURCE,
    ),
];
#[cfg(not(target_os = "windows"))]
const COMPLETION_COMPILE_UNITS: &[&str] = &[
    "completion/runtime.c",
    "completion/wait_host.c",
    "completion/file_adapter.c",
    "completion/file_posix.c",
    "completion/bridge.c",
    "completion/linux_io_uring.c",
];

#[cfg(target_os = "windows")]
const COMPLETION_PLATFORM_UNITS: &[RuntimeUnit] = &[
    unit("completion/wait_windows.c", COMPLETION_WAIT_WINDOWS_SOURCE),
    unit("completion/file_windows.c", COMPLETION_FILE_WINDOWS_SOURCE),
    unit("completion/windows_iocp.c", COMPLETION_WINDOWS_IOCP_SOURCE),
];
#[cfg(target_os = "windows")]
const COMPLETION_COMPILE_UNITS: &[&str] = &[
    "completion/runtime.c",
    "completion/wait_windows.c",
    "completion/file_adapter.c",
    "completion/file_windows.c",
    "completion/bridge.c",
    "completion/windows_iocp.c",
];

/// The arguments this host's link needs beside the dialect and the inputs.
#[cfg(not(target_os = "windows"))]
const TARGET_COMPILE_ARGUMENTS: &[&str] = &["-pthread"];
#[cfg(target_os = "windows")]
const TARGET_COMPILE_ARGUMENTS: &[&str] = &[];

/// The linker a link-time-optimized link runs on this host, and the
/// argument prefix naming the directory where a ThinLTO link of fragments
/// keeps the optimized objects it produces: the host's own linker on macOS,
/// whose LTO support ships with its toolchain, and LLD elsewhere.
#[cfg(target_os = "macos")]
const LTO_LINKER: &[&str] = &[];
#[cfg(target_os = "macos")]
const THIN_LINK_CACHE: &str = "-Wl,-cache_path_lto,";
#[cfg(target_os = "windows")]
const LTO_LINKER: &[&str] = &["-fuse-ld=lld"];
#[cfg(target_os = "windows")]
const THIN_LINK_CACHE: &str = "-Wl,/lldltocache:";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const LTO_LINKER: &[&str] = &["-fuse-ld=lld"];
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const THIN_LINK_CACHE: &str = "-Wl,--thinlto-cache-dir=";

/// The libraries this host's link needs. The ordinary Windows library uses
/// Winsock and the shell's Unicode command-line conversion. The build launcher
/// defines `main`; argument conversion does not require a `wmain` entry.
#[cfg(not(target_os = "windows"))]
const TARGET_LINK_LIBRARIES: &[&str] = HOST_LINK_LIBRARIES;
#[cfg(target_os = "windows")]
const TARGET_LINK_LIBRARIES: &[&str] = &["-lws2_32", "-lshell32"];

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let format = requested_format(&arguments);
    let driver = match std::thread::Builder::new()
        .name("whitefootc-driver".to_owned())
        .stack_size(COMPILER_DRIVER_STACK_BYTES)
        .spawn(move || run(&arguments))
    {
        Ok(driver) => driver,
        Err(error) => {
            let message = format!("cannot start the compiler driver: {error}");
            eprintln!("{}", render_driver_failure("Resource", &message, format));
            std::process::exit(1);
        }
    };
    match driver.join() {
        Ok(Ok(())) => {}
        Ok(Err(stop)) => {
            eprintln!("{}", stop.render(format));
            std::process::exit(1);
        }
        // The panic hook on the driver thread has already printed the panic.
        // Preserve Rust's ordinary panic exit status without printing a
        // second, less useful panic from this joining thread.
        Err(_) => std::process::exit(101),
    }
}

/// The rendering this invocation asks for.
///
/// It is read before the other options so that an invalid option is reported
/// in the requested form too. A missing or invalid value selects text, and
/// [`Options::parse`] then reports that value as the invalid option it is.
fn requested_format(arguments: &[String]) -> DiagnosticFormat {
    let value = arguments
        .iter()
        .position(|argument| argument == "--diagnostic-format")
        .and_then(|index| arguments.get(index.saturating_add(1)));
    match value.map(String::as_str) {
        Some("json") => DiagnosticFormat::Json,
        _ => DiagnosticFormat::Text,
    }
}

/// Why one invocation stopped.
#[derive(Debug)]
enum Stop {
    /// The compilation pipeline stopped; its record carries the rule, the
    /// location and the payload.
    Compilation(CompilationFailure),
    /// The driver stopped outside the pipeline, on one sentence: an invalid
    /// invocation, an unwritable output, or a host toolchain step.
    Driver {
        category: &'static str,
        message: String,
    },
}

impl Stop {
    fn invocation(message: String) -> Self {
        Self::Driver {
            category: "Invocation",
            message,
        }
    }

    fn output(message: String) -> Self {
        Self::Driver {
            category: "Output",
            message,
        }
    }

    fn toolchain(message: String) -> Self {
        Self::Driver {
            category: "Toolchain",
            message,
        }
    }

    fn render(&self, format: DiagnosticFormat) -> String {
        match self {
            Self::Compilation(failure) => failure.render(format),
            Self::Driver { category, message } => render_driver_failure(category, message, format),
        }
    }
}

fn run(arguments: &[String]) -> Result<(), Stop> {
    let options = Options::parse(arguments).map_err(Stop::invocation)?;
    let cache = options.cache.as_deref().map(open_cache).transpose()?;
    let mut report = BuildReport::default();
    if let Some(graph) = &options.graph {
        let Some(module) = run_module_program(&options, graph, cache.as_ref(), &mut report)? else {
            return Ok(());
        };
        return finish(&options, &module, cache.as_ref(), &mut report);
    }
    let mut paths = Vec::with_capacity(options.sources.len());
    let mut bytes = Vec::with_capacity(options.sources.len());
    for (index, source) in options.sources.iter().enumerate() {
        bytes.push(std::fs::read(source).map_err(|error| {
            Stop::invocation(format!("cannot read {}: {error}", source.display()))
        })?);
        paths.push(source_names(source, index));
    }
    let inputs: Vec<_> = paths
        .iter()
        .zip(&bytes)
        .map(|((logical, display), bytes)| SourceInput::from_host_path(logical, display, bytes))
        .collect();
    let overlap = options.overlap();
    let module = if options.par_ledger {
        // The permission ledger is developer output. It goes to stdout, which
        // `Options::parse` has already kept clear of the emitted module, and
        // never to the mandatory record channel. Its judgment lines are the
        // same with or without `--par`, because the judgment is pure. Its
        // actualization lines — what the lowering did with each permission it
        // was handed — exist only where actualization was asked for, so `--par`
        // adds lines to this ledger rather than changing any of them.
        let (module, ledger) =
            compile_with_permission_ledger(&inputs, CompilerLimits::default(), overlap)
                .map_err(Stop::Compilation)?;
        for line in &ledger {
            println!("{line}");
        }
        module
    } else {
        let front_end = std::time::Instant::now();
        if options.check {
            match &cache {
                Some(cache) => check_with_cache(&inputs, CompilerLimits::default(), cache),
                None => check(&inputs, CompilerLimits::default()),
            }
            .map_err(Stop::Compilation)?;
            return Ok(());
        }
        let module = match &cache {
            Some(cache) => compile_with_cache(&inputs, CompilerLimits::default(), overlap, cache),
            None => compile_with_overlap(&inputs, CompilerLimits::default(), overlap),
        }
        .map_err(Stop::Compilation)?;
        report.front_end = front_end.elapsed();
        module
    };
    finish(&options, &module, cache.as_ref(), &mut report)
}

/// Reads a module program's graph and every registered module's records
/// below the graph's directory, then checks it or compiles its entry
/// [MOD-1, MOD-2, MOD-9]. `None` is a completed check.
fn run_module_program(
    options: &Options,
    graph_path: &Path,
    cache: Option<&BuildCache>,
    report: &mut BuildReport,
) -> Result<Option<String>, Stop> {
    let (graph, sources) = read_module_program(graph_path)?;
    let inputs = module_inputs(&sources);
    let limits = CompilerLimits::default();
    if let Some(module) = &options.render_interface {
        let rendered =
            render_module_interface(&graph, &inputs, module, limits).map_err(Stop::Compilation)?;
        let Some(against) = &options.against else {
            print!("{rendered}");
            return Ok(None);
        };
        // [MOD-6, MOD-8] the conservative revision comparison: the other
        // revision's rendering of the same module, line by line.
        let (other_graph, other_sources) = read_module_program(against)?;
        let other_inputs = module_inputs(&other_sources);
        let before = render_module_interface(&other_graph, &other_inputs, module, limits)
            .map_err(Stop::Compilation)?;
        if before == rendered {
            println!("{module}: interface unchanged");
            return Ok(None);
        }
        let old_lines = before.lines().collect::<Vec<_>>();
        let new_lines = rendered.lines().collect::<Vec<_>>();
        for line in &old_lines {
            if !new_lines.contains(line) {
                println!("- {line}");
            }
        }
        for line in &new_lines {
            if !old_lines.contains(line) {
                println!("+ {line}");
            }
        }
        return Err(Stop::Driver {
            category: "Interface",
            message: format!("{module}: interface changed"),
        });
    }
    if options.check_modules {
        let mut verdicts = Vec::new();
        // [MOD-10] the program's own modules; a composition below judges the
        // standard library modules its entry selects.
        for record in graph
            .modules()
            .iter()
            .filter(|record| record.package() == whitefoot::Package::Program)
        {
            let module = record.qualified_name();
            let (verdict, analyses) = counting_analyses(cache, || {
                module_verdict(&graph, &inputs, &module, false, limits, cache)
            });
            verdicts.push(("module", verdict.map_err(Stop::Compilation)?, analyses));
        }
        let modules = verdicts
            .iter()
            .map(|(_, verdict, _)| verdict.clone())
            .collect::<Vec<_>>();
        for entry in graph.entries() {
            let (verdict, analyses) = counting_analyses(cache, || {
                entry_verdict(
                    &graph,
                    &inputs,
                    ModuleEntry::Named(entry.name()),
                    limits,
                    cache,
                    &modules,
                )
            });
            verdicts.push(("entry", verdict.map_err(Stop::Compilation)?, analyses));
        }
        publish_verdicts(options, &verdicts)?;
        return Ok(None);
    }
    if let Some(module) = &options.check_module {
        let (verdict, analyses) = counting_analyses(cache, || {
            module_verdict(
                &graph,
                &inputs,
                module,
                options.interface_only,
                limits,
                cache,
            )
        });
        publish_verdicts(
            options,
            &[("module", verdict.map_err(Stop::Compilation)?, analyses)],
        )?;
        return Ok(None);
    }
    let entry = if let Some(name) = &options.entry {
        Some(ModuleEntry::Named(name))
    } else if let Some(written) = options.function.as_deref() {
        let (module, function) = written.rsplit_once("::").ok_or_else(|| {
            Stop::invocation(format!("--function names pkg::module::name, not {written}"))
        })?;
        Some(ModuleEntry::Function { module, function })
    } else {
        None
    };
    if options.check {
        // Without an entry, the check covers every registered module; with
        // one, it is that entry's composition check [MOD-8, MOD-9].
        match entry {
            Some(entry) => {
                let (verdict, analyses) = counting_analyses(cache, || {
                    entry_verdict(&graph, &inputs, entry, limits, cache, &[])
                });
                publish_verdicts(
                    options,
                    &[("entry", verdict.map_err(Stop::Compilation)?, analyses)],
                )?;
            }
            None => check_module_program(&graph, &inputs, limits).map_err(Stop::Compilation)?,
        }
        return Ok(None);
    }
    let entry = entry.ok_or_else(|| {
        Stop::invocation(
            "a --graph build selects --entry NAME or --function pkg::module::name".to_owned(),
        )
    })?;
    let front_end = std::time::Instant::now();
    let (module, reused) =
        build_module_entry(&graph, &inputs, entry, limits, options.overlap(), cache)
            .map_err(Stop::Compilation)?;
    report.front_end = front_end.elapsed();
    report.module_reused = Some(reused);
    Ok(Some(module))
}

/// A module program's graph and every registered module's records, read
/// from the graph's directory [MOD-1, MOD-2].
fn read_module_program(
    graph_path: &Path,
) -> Result<(whitefoot::ModuleGraph, Vec<whitefoot::ModuleSourceFile>), Stop> {
    let graph_bytes =
        read_graph_record(graph_path).map_err(|failure| Stop::invocation(failure.to_string()))?;
    let display = graph_path.display().to_string();
    let graph = form_module_graph(
        SourceInput::from_host_path(GRAPH_FILE_NAME, &display, &graph_bytes),
        CompilerLimits::default(),
    )
    .map_err(Stop::Compilation)?;
    let root = graph_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let sources = discover_module_sources(root, &graph)
        .map_err(|failure| Stop::invocation(failure.to_string()))?;
    Ok((graph, sources))
}

/// Each record placed in its module and role.
fn module_inputs(sources: &[whitefoot::ModuleSourceFile]) -> Vec<SourceInput<'_>> {
    sources
        .iter()
        .map(|source| {
            SourceInput::from_host_path(&source.logical_path, &source.display_path, &source.bytes)
                .in_module(source.module, source.role)
        })
        .collect()
}

/// Opens the build cache directory for this exact compiler.
fn open_cache(directory: &Path) -> Result<BuildCache, Stop> {
    let identity = running_compiler_identity().map_err(|error| {
        Stop::invocation(format!("cannot read this compiler's executable: {error}"))
    })?;
    BuildCache::open(directory, identity).map_err(|error| {
        Stop::output(format!(
            "cannot open the cache {}: {error}",
            directory.display()
        ))
    })
}

/// How many function analyses one check took from proof receipts and how
/// many it recorded, with a cache [MOD-8].
type Analyses = Option<(u64, u64)>;

/// The result of `check` and how many function analyses it took from proof
/// receipts and recorded, with a cache [MOD-8].
fn counting_analyses<T>(cache: Option<&BuildCache>, check: impl FnOnce() -> T) -> (T, Analyses) {
    let before = cache.map(BuildCache::receipt_counts);
    let result = check();
    let after = cache.map(BuildCache::receipt_counts);
    let analyses = before
        .zip(after)
        .map(|((reused, recorded), (now_reused, now_recorded))| {
            (now_reused - reused, now_recorded - recorded)
        });
    (result, analyses)
}

/// Prints each verdict, as one line of text or one JSON object per line, and
/// fails the invocation when any is a rejection. The text is the same
/// whether a verdict was reused or recomputed, so a cached run and a run
/// without a cache can be compared line for line; the JSON reports which,
/// and how many function analyses a recomputed one took from proof receipts.
fn publish_verdicts(
    options: &Options,
    verdicts: &[(&str, CheckVerdict, Analyses)],
) -> Result<(), Stop> {
    let mut rejected = Vec::new();
    for (kind, verdict, analyses) in verdicts {
        if options.report {
            println!("{}", verdict_json(kind, verdict, *analyses));
        }
        match verdict.outcome() {
            CheckOutcome::Accepted { pending } => {
                if !options.report {
                    if pending.is_empty() {
                        println!("{}: accepted", verdict.subject());
                    } else {
                        println!(
                            "{}: accepted, pending {}",
                            verdict.subject(),
                            pending.join(", ")
                        );
                    }
                }
            }
            CheckOutcome::Rejected {
                failure,
                tasks,
                complete,
                ..
            } => {
                if !options.report {
                    println!("{}: rejected: {failure}", verdict.subject());
                    // [MOD-8] the impact report's further tasks, one line
                    // each, after the rejection the verdict reports.
                    for task in tasks.iter().skip(1) {
                        println!(
                            "{}: also {} {}{} [{}]",
                            verdict.subject(),
                            task.kind().name(),
                            task.function()
                                .map_or_else(String::new, |name| format!("`{name}` ")),
                            task.location().map_or_else(
                                || "at no written place".to_owned(),
                                |at| format!("at {at}")
                            ),
                            task.rule().unwrap_or("no rule"),
                        );
                    }
                    if !complete {
                        println!(
                            "{}: further failures may follow the last listed one",
                            verdict.subject()
                        );
                    }
                }
                rejected.push(verdict.subject().to_owned());
            }
        }
    }
    if rejected.is_empty() {
        Ok(())
    } else {
        Err(Stop::Driver {
            category: "Verdict",
            message: format!("rejected: {}", rejected.join(", ")),
        })
    }
}

/// One verdict as a JSON object.
fn verdict_json(kind: &str, verdict: &CheckVerdict, analyses: Analyses) -> String {
    let mut fields = vec![format!("\"{kind}\":{}", json_string(verdict.subject()))];
    match verdict.outcome() {
        CheckOutcome::Accepted { pending } => {
            fields.push("\"verdict\":\"accepted\"".to_owned());
            fields.push(format!(
                "\"pending\":[{}]",
                pending
                    .iter()
                    .map(|name| json_string(name))
                    .collect::<Vec<_>>()
                    .join(",")
            ));
        }
        CheckOutcome::Rejected {
            rule,
            failure,
            tasks,
            complete,
        } => {
            fields.push("\"verdict\":\"rejected\"".to_owned());
            fields.push(format!(
                "\"rule\":{}",
                rule.as_deref()
                    .map_or_else(|| "null".to_owned(), json_string)
            ));
            fields.push(format!("\"failure\":{}", json_string(failure)));
            let optional = |text: Option<&str>| text.map_or_else(|| "null".to_owned(), json_string);
            let tasks = tasks
                .iter()
                .map(|task| {
                    let location = task.location();
                    format!(
                        "{{\"kind\":{},\"function\":{},\"rule\":{},\"file\":{},\"line\":{},\"column\":{},\"failure\":{}}}",
                        json_string(task.kind().name()),
                        optional(task.function()),
                        optional(task.rule()),
                        optional(location.map(whitefoot::SourceLocation::path)),
                        location.map_or_else(|| "null".to_owned(), |at| at.line().to_string()),
                        location.map_or_else(|| "null".to_owned(), |at| at.column().to_string()),
                        json_string(task.failure()),
                    )
                })
                .collect::<Vec<_>>();
            fields.push(format!("\"tasks\":[{}]", tasks.join(",")));
            fields.push(format!("\"complete\":{complete}"));
        }
    }
    fields.push(format!("\"reused\":{}", verdict.reused()));
    if let Some((reused, recorded)) = analyses {
        fields.push(format!(
            "\"analyses_reused\":{reused},\"analyses_recorded\":{recorded}"
        ));
    }
    format!("{{{}}}", fields.join(","))
}

/// A JSON string literal of `text`.
fn json_string(text: &str) -> String {
    use std::fmt::Write as _;
    let mut literal = String::with_capacity(text.len() + 2);
    literal.push('"');
    for character in text.chars() {
        match character {
            '"' => literal.push_str("\\\""),
            '\\' => literal.push_str("\\\\"),
            '\n' => literal.push_str("\\n"),
            '\r' => literal.push_str("\\r"),
            '\t' => literal.push_str("\\t"),
            control if u32::from(control) < 0x20 => {
                let _ = write!(literal, "\\u{:04x}", u32::from(control));
            }
            other => literal.push(other),
        }
    }
    literal.push('"');
    literal
}

/// Writes or links one emitted module as the options select.
fn finish(
    options: &Options,
    module: &str,
    cache: Option<&BuildCache>,
    report: &mut BuildReport,
) -> Result<(), Stop> {
    if options.stack_ledger {
        for line in print_stack_ledger(module).map_err(Stop::toolchain)? {
            println!("{line}");
        }
    }
    if options.emit_llvm {
        if let Some(output) = &options.output {
            std::fs::write(output, module).map_err(|error| {
                Stop::output(format!("cannot write {}: {error}", output.display()))
            })?;
        } else {
            print!("{module}");
        }
        return Ok(());
    }
    require_runner(module)?;
    compile_executable(
        module,
        options.output.as_deref().unwrap_or(Path::new("a.out")),
        cache,
        options.fragments,
        options.full_lto,
        report,
    )
    .map_err(Stop::toolchain)?;
    if options.report {
        report.analyses = cache.map(BuildCache::receipt_counts);
        println!("{}", report.json());
    }
    Ok(())
}

/// [PROG-3] the executable runs the selected function through the runner the
/// build generates for it; a checked module without one is a library, which
/// this compiler links into no executable.
fn require_runner(module: &str) -> Result<(), Stop> {
    if module.contains("\ndefine i32 @wf__main_body(") {
        return Ok(());
    }
    let refused = module
        .lines()
        .find_map(|line| line.strip_prefix("; Executable caller was not admitted: "))
        .map_or_else(String::new, |reason| {
            format!(": its generated caller was not admitted: {reason}")
        });
    Err(Stop::invocation(format!(
        "the selected function has no executable runner{refused}; this build runs a function that takes no parameter or one Inputs and returns ExitStatus or unit, and --emit-llvm writes any checked program as a library"
    )))
}

/// Compiles the module once more, to assembly, purely to read the two things
/// the ledger needs out of the host compiler: what each surviving machine
/// function's frame costs and which functions call which.
///
/// It is a second invocation rather than a flag on the link that already
/// happens, for two reasons. `-fstack-usage` writes its report beside the file
/// it compiled, so putting it on the ordinary link would drop a `.su` into
/// whatever directory the writer asked for output in, on a flag they may have
/// passed for the report alone. And the call graph has to be the post-inline
/// one that belongs to those frame numbers, which means assembly, which the
/// link does not produce. Both come out of the one compilation below, into a
/// directory this function owns and removes, and none of it runs unless the
/// ledger was asked for.
fn print_stack_ledger(llvm: &str) -> Result<Vec<String>, String> {
    let directory = std::env::temp_dir().join(format!("whitefootc-ledger-{}", std::process::id()));
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("cannot create the ledger directory: {error}"))?;
    let module = directory.join("module.ll");
    let assembly = directory.join("module.s");
    let result = (|| {
        std::fs::write(&module, llvm)
            .map_err(|error| format!("cannot write the ledger module: {error}"))?;
        let status = Command::new(clang_executable())
            .arg("-x")
            .arg("ir")
            .arg(&module)
            .arg("-S")
            .arg("-o")
            .arg(&assembly)
            .arg("-fstack-usage")
            .arg("-Wno-override-module")
            .args(HOST_OPTIMIZATION_ARGUMENTS)
            .status()
            .map_err(|error| format!("cannot start {}: {error}", clang_executable()))?;
        if !status.success() {
            return Err(format!("clang exited with {status}"));
        }
        let usage = std::fs::read_to_string(directory.join("module.su"))
            .map_err(|error| format!("cannot read the stack-usage report: {error}"))?;
        let text = std::fs::read_to_string(&assembly)
            .map_err(|error| format!("cannot read the ledger assembly: {error}"))?;
        Ok(stack_ledger(
            &usage,
            &text,
            FLOOR_STACK_BYTES,
            Architecture::HOST,
        ))
    })();
    let _ = std::fs::remove_dir_all(&directory);
    result
}

/// The runtime units this link stages and the subset clang compiles.
///
/// Every module links the complete ordinary library, including its floor,
/// scheduler and completion dependencies. Each group combines shared units
/// with this platform's leaves; source-call classifications select no units.
fn runtime_units() -> (Vec<RuntimeUnit>, Vec<&'static str>) {
    let mut staged: Vec<RuntimeUnit> = FLOOR_SHARED_UNITS.to_vec();
    staged.extend([
        unit("ordinary_values.h", ORDINARY_VALUES_HEADER),
        unit("ordinary_values.c", ORDINARY_VALUES_SOURCE),
        unit("ordinary_values.ll", ORDINARY_VALUES_LLVM),
    ]);
    staged.extend_from_slice(FLOOR_PLATFORM_UNITS);
    let mut compiled: Vec<&'static str> = FLOOR_COMPILE_UNITS.to_vec();
    compiled.extend(["ordinary_values.c", "ordinary_values.ll"]);
    {
        staged.extend_from_slice(CORE_SHARED_UNITS);
        staged.extend_from_slice(CORE_PLATFORM_UNITS);
        compiled.extend_from_slice(CORE_COMPILE_UNITS);
    }
    {
        staged.extend_from_slice(COMPLETION_SHARED_UNITS);
        staged.extend_from_slice(COMPLETION_PLATFORM_UNITS);
        compiled.extend_from_slice(COMPLETION_COMPILE_UNITS);
    }
    (staged, compiled)
}

/// Stages the compiler-owned runtime beside the emitted module and links them
/// into one executable.
///
/// The lists above supply each platform's complete ordinary library. Compute
/// joins help on the current stack; completion joins wait through their native
/// backend. Neither source-call classification nor effect rows select a
/// different set of link inputs.
///
/// Every one of those bytes travels inside this executable, so no installed
/// path, no build directory, and no environment decides which runtime a
/// program gets.
fn compile_executable(
    llvm: &str,
    output: &Path,
    cache: Option<&BuildCache>,
    fragments: Option<FragmentGranularity>,
    full_lto: bool,
    report: &mut BuildReport,
) -> Result<(), String> {
    // The ordinary prelude implementation library links its private engine.
    // Each build stages in its own directory, so two builds of one process
    // never share or remove each other's inputs.
    static BUILDS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let build = BUILDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let directory = std::env::temp_dir().join(format!("whitefootc-{}-{build}", std::process::id()));
    let result = (|| {
        let (staged, compiled) = runtime_units();
        std::fs::create_dir_all(&directory)
            .map_err(|error| format!("cannot create the runtime directory: {error}"))?;
        for unit in &staged {
            let path = directory.join(unit.relative_path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|error| {
                    format!(
                        "cannot create the runtime directory for {}: {error}",
                        unit.relative_path
                    )
                })?;
            }
            std::fs::write(&path, unit.source)
                .map_err(|error| format!("cannot write runtime {}: {error}", unit.relative_path))?;
        }
        if let Some(cache) = cache {
            return link_cached_objects(
                cache, &directory, &staged, &compiled, llvm, output, fragments, report,
            );
        }
        let linking = std::time::Instant::now();
        let mut command = Command::new(clang_executable());
        unit_arguments(&mut command, &directory);
        for relative_path in &compiled {
            command
                .arg("-x")
                .arg(unit_language(relative_path))
                .arg(directory.join(relative_path));
        }
        if full_lto {
            command.args(LTO_LINKER).arg("-flto=full");
        }
        let linked = link(&mut command, llvm, output);
        report.link = linking.elapsed();
        linked
    })();
    let _ = std::fs::remove_dir_all(&directory);
    result
}

/// The arguments every compiler-owned runtime unit compiles with.
fn unit_arguments(command: &mut Command, directory: &Path) {
    // The compiler-owned C units are written to C11 and the repository gate
    // compiles them as `-std=c11`. Naming the dialect here too is what makes
    // that gate a statement about this link: clang's default is a GNU
    // dialect, which predefines object-like macros such as `linux` that a C11
    // source may legitimately use as an identifier.
    command
        .arg("-std=c11")
        .args(TARGET_COMPILE_ARGUMENTS)
        .arg("-I")
        .arg(directory)
        .arg("-I")
        .arg(directory.join("completion"));
}

/// The clang input language of one runtime unit.
fn unit_language(relative_path: &str) -> &'static str {
    if relative_path.ends_with(".ll") {
        "ir"
    } else {
        "c"
    }
}

/// The same link over objects: each runtime unit and the emitted module
/// compile to an object, reused from the cache when a record exists for
/// exactly that input, compile arguments and host compiler, and one
/// ordinary link joins them. With `fragments`, the emitted module is split
/// into link fragments and every object is ThinLTO bitcode, so the link
/// plans cross-fragment imports over their summaries and LLVM's own cache
/// keeps the optimized objects it produces. The executable is written beside
/// `output` and renamed over it only after the link succeeds, so a failed
/// build leaves the previous executable in place.
#[allow(clippy::too_many_arguments)]
fn link_cached_objects(
    cache: &BuildCache,
    directory: &Path,
    staged: &[RuntimeUnit],
    compiled: &[&str],
    llvm: &str,
    output: &Path,
    fragments: Option<FragmentGranularity>,
    report: &mut BuildReport,
) -> Result<(), String> {
    let host = host_compiler_identity()?;
    let thin: &[&str] = if fragments.is_some() {
        &["-flto=thin"]
    } else {
        &[]
    };
    let mut runtime = Vec::new();
    for unit in staged {
        runtime.extend_from_slice(unit.relative_path.as_bytes());
        runtime.push(0);
        runtime.extend_from_slice(unit.source.as_bytes());
        runtime.push(0);
    }
    let runtime = content_digest(&runtime);
    let compiling = std::time::Instant::now();
    let mut objects = Vec::with_capacity(compiled.len() + 1);
    for (index, relative_path) in compiled.iter().enumerate() {
        let mut material = b"runtime-object 1\n".to_vec();
        material.extend_from_slice(&host);
        material.extend_from_slice(&runtime);
        material.extend_from_slice(
            format!(
                "\n{relative_path}\n{TARGET_COMPILE_ARGUMENTS:?} {HOST_OPTIMIZATION_ARGUMENTS:?} {thin:?}\n"
            )
            .as_bytes(),
        );
        let object = directory.join(format!("unit-{index}.o"));
        let reused = cached_object(cache, "runtime-objects", &material, &object, |command| {
            if unit_language(relative_path) == "ir" {
                command
                    .args(TARGET_COMPILE_ARGUMENTS)
                    .arg("-Wno-override-module");
            } else {
                unit_arguments(command, directory);
            }
            command
                .args(thin)
                .arg("-x")
                .arg(unit_language(relative_path))
                .arg(directory.join(relative_path));
            None
        })?;
        report.count_object(reused);
        objects.push(object);
    }
    let splitting = std::time::Instant::now();
    let parts = match fragments {
        Some(granularity) => {
            split_module(llvm, granularity).map_err(|failure| failure.to_string())?
        }
        None => vec![llvm.to_owned()],
    };
    report.fragments = parts.len();
    report.split = splitting.elapsed();
    for (index, text) in parts.iter().enumerate() {
        // Relative names keep the staging directory, which differs per
        // invocation, out of the compiler's inputs.
        let part = directory.join(format!("fragment-{index}.ll"));
        std::fs::write(&part, text)
            .map_err(|error| format!("cannot write {}: {error}", part.display()))?;
        let mut material = b"program-object 1\n".to_vec();
        material.extend_from_slice(&host);
        material.extend_from_slice(&content_digest(text.as_bytes()));
        material.extend_from_slice(
            format!("\n{TARGET_COMPILE_ARGUMENTS:?} {HOST_OPTIMIZATION_ARGUMENTS:?} {thin:?}\n")
                .as_bytes(),
        );
        let object = directory.join(format!("program-{index}.o"));
        let reused = cached_object(cache, "program-objects", &material, &object, |command| {
            command
                .current_dir(directory)
                .args(TARGET_COMPILE_ARGUMENTS)
                .args(thin)
                .arg("-Wno-override-module")
                .arg("-x")
                .arg("ir")
                .arg(part.file_name().unwrap_or(part.as_os_str()));
            None
        })?;
        report.count_object(reused);
        objects.push(object);
    }
    report.compile = compiling.elapsed().saturating_sub(report.split);
    let partial = output.with_file_name(format!(
        ".{}.{}.partial",
        output
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
        std::process::id()
    ));
    let linking = std::time::Instant::now();
    let mut command = Command::new(clang_executable());
    command
        .args(TARGET_COMPILE_ARGUMENTS)
        .args(HOST_OPTIMIZATION_ARGUMENTS)
        .args(&objects)
        .args(TARGET_LINK_LIBRARIES);
    if fragments.is_some() {
        let optimized = cache
            .area("thinlto")
            .map_err(|error| format!("cannot open the ThinLTO cache: {error}"))?;
        let mut cache_argument = std::ffi::OsString::from(THIN_LINK_CACHE);
        cache_argument.push(&optimized);
        command
            .args(LTO_LINKER)
            .arg("-flto=thin")
            .arg(cache_argument);
    }
    let status = command
        .arg("-o")
        .arg(&partial)
        .status()
        .map_err(|error| format!("cannot start {}: {error}", clang_executable()))?;
    report.link = linking.elapsed();
    if !status.success() {
        let _ = std::fs::remove_file(&partial);
        return Err(format!("clang exited with {status}"));
    }
    std::fs::rename(&partial, output)
        .map_err(|error| format!("cannot write {}: {error}", output.display()))
}

/// The phases and reuse of one build, reported by `--report`.
#[derive(Default)]
struct BuildReport {
    /// Whether the entry's emitted module was reused, for a module program.
    module_reused: Option<bool>,
    front_end: std::time::Duration,
    fragments: usize,
    split: std::time::Duration,
    objects_compiled: usize,
    objects_reused: usize,
    compile: std::time::Duration,
    link: std::time::Duration,
    /// Function analyses the front end took from proof receipts and
    /// recorded, with a cache [MOD-8].
    analyses: Option<(u64, u64)>,
}

impl BuildReport {
    const fn count_object(&mut self, reused: bool) {
        if reused {
            self.objects_reused += 1;
        } else {
            self.objects_compiled += 1;
        }
    }

    fn json(&self) -> String {
        let milliseconds = |duration: std::time::Duration| duration.as_secs_f64() * 1000.0;
        format!(
            "{{\"build\":{{\"module_reused\":{},\"front_end_ms\":{:.1},\"analyses_reused\":{},\"analyses_recorded\":{},\"fragments\":{},\"split_ms\":{:.1},\"objects_compiled\":{},\"objects_reused\":{},\"compile_ms\":{:.1},\"link_ms\":{:.1}}}}}",
            self.module_reused
                .map_or_else(|| "null".to_owned(), |reused| reused.to_string()),
            milliseconds(self.front_end),
            self.analyses
                .map_or_else(|| "null".to_owned(), |(reused, _)| reused.to_string()),
            self.analyses
                .map_or_else(|| "null".to_owned(), |(_, recorded)| recorded.to_string()),
            self.fragments,
            milliseconds(self.split),
            self.objects_compiled,
            self.objects_reused,
            milliseconds(self.compile),
            milliseconds(self.link),
        )
    }
}

/// Writes the object a cache records for `material` to `object`, or compiles
/// it with the arguments `configure` adds, feeding the returned text on
/// stdin, and records it.
fn cached_object<'text>(
    cache: &BuildCache,
    family: &str,
    material: &[u8],
    object: &Path,
    configure: impl FnOnce(&mut Command) -> Option<&'text str>,
) -> Result<bool, String> {
    if let Some(bytes) = cache.load(family, material) {
        return std::fs::write(object, bytes)
            .map(|()| true)
            .map_err(|error| format!("cannot write {}: {error}", object.display()));
    }
    let mut command = Command::new(clang_executable());
    let stdin = configure(&mut command);
    command
        .args(HOST_OPTIMIZATION_ARGUMENTS)
        .arg("-c")
        .arg("-o")
        .arg(object)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });
    let mut child = command
        .spawn()
        .map_err(|error| format!("cannot start {}: {error}", clang_executable()))?;
    if let Some(text) = stdin {
        child
            .stdin
            .take()
            .ok_or_else(|| "clang stdin was not available".to_owned())?
            .write_all(text.as_bytes())
            .map_err(|error| format!("cannot send LLVM to clang: {error}"))?;
    }
    let status = child
        .wait()
        .map_err(|error| format!("cannot wait for clang: {error}"))?;
    if !status.success() {
        return Err(format!("clang exited with {status}"));
    }
    let bytes = std::fs::read(object)
        .map_err(|error| format!("cannot read {}: {error}", object.display()))?;
    // A failed publication costs only a later recompilation.
    let _ = cache.store(family, material, &bytes);
    Ok(false)
}

/// The host compiler's identity: its path and its own version report.
fn host_compiler_identity() -> Result<[u8; 32], String> {
    let version = Command::new(clang_executable())
        .arg("--version")
        .output()
        .map_err(|error| format!("cannot start {}: {error}", clang_executable()))?;
    let mut identity = clang_executable().as_bytes().to_vec();
    identity.push(0);
    identity.extend_from_slice(&version.stdout);
    Ok(content_digest(&identity))
}

fn link(command: &mut Command, llvm: &str, output: &Path) -> Result<(), String> {
    let mut child = command
        .arg("-x")
        .arg("ir")
        .arg("-")
        .arg("-Wno-override-module")
        .args(HOST_OPTIMIZATION_ARGUMENTS)
        .args(TARGET_LINK_LIBRARIES)
        .arg("-o")
        .arg(output)
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|error| format!("cannot start {}: {error}", clang_executable()))?;
    child
        .stdin
        .take()
        .ok_or_else(|| "clang stdin was not available".to_owned())?
        .write_all(llvm.as_bytes())
        .map_err(|error| format!("cannot send LLVM to clang: {error}"))?;
    let status = child
        .wait()
        .map_err(|error| format!("cannot wait for clang: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("clang exited with {status}"))
    }
}

/// The two names one source argument carries: the bundle's own name, then the
/// name every reader is shown.
///
/// The display name is the argument, unchanged. That is the whole point: an
/// absolute path is how a script, a Makefile, and an agent all invoke this
/// compiler, and a diagnostic or ledger line that renames the file to
/// `input0.wf` names a file that exists nowhere on disk.
fn source_names(path: &Path, index: usize) -> (String, String) {
    (
        logical_path(path, index),
        path.to_string_lossy().into_owned(),
    )
}

/// The bundle's own name for one source.
///
/// This is program identity, not presentation: it orders the bundle and
/// detects a duplicate, and its spelling is the closed portable one, so a host
/// path that cannot be spelled there falls back to a positional name. Nothing
/// a reader sees comes from here — the diagnostic and the permission ledger
/// both name the display path, which is exactly the argument the caller typed.
fn logical_path(path: &Path, index: usize) -> String {
    let candidate = path.to_string_lossy();
    if !path.is_absolute() && portable_logical_path(&candidate) {
        candidate.into_owned()
    } else {
        format!("input{index}.wf")
    }
}

fn portable_logical_path(path: &str) -> bool {
    !path.is_empty()
        && path.split('/').all(|component| {
            !component.is_empty()
                && !matches!(component, "." | "..")
                && component
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        })
}

struct Options {
    emit_llvm: bool,
    /// Actualize the permission judgment's eligible groups on worker lanes.
    ///
    /// Compute outlining is off by default; compiler-owned completion I/O
    /// remains enabled independently. The compute path is free when requested
    /// and no pool is asked for.
    ///
    /// Outlining a call is not free — it passes its arguments through a memory
    /// frame, is reached through a function pointer, and rejoins its two edges
    /// through a phi that can foreclose a transform the sequential build gets.
    /// What that costs before any worker runs depends entirely on grain: on a
    /// heavy recursive fold it was already nothing, while `fib(38)` measured
    /// 0.0790 s default against 0.2337 s `--par` with the pool off, or 2.96x,
    /// almost all of it one foreclosed tail-recursion transform. So a `--par`
    /// build now carries the sequential lowering as well, byte for byte, and
    /// its bootstrap selects between the two once, on whether a pool was asked
    /// for. Re-measured the same way, `fib(38)` reads 0.0810 s with the pool
    /// off against 0.0811 s for the default build, and the twelve
    /// paired-layout programs read 1.00x-1.01x of their own default builds at
    /// one worker, where before they ranged 0.68x-1.02x. (Both readings are
    /// taken at `WF_WORKERS=1`. An absent setting asks for a pool sized to the
    /// machine, so it is not the pool-off spelling.)
    ///
    /// Two prices, both real. Code size, paid only by a `--par` build: 7% to
    /// 14% more machine code, which is 0.5% to 0.6% of the linked file. And a
    /// second copy shifts every address in that binary, which re-rolls this
    /// workload's known sensitivity to code placement — `par_layout.wf` reads
    /// 1.19x of its default build with the pool off, at equal instruction
    /// count and IPC 2.89 down to 2.43, and the *same* module linked with its
    /// two inputs in the other order reads 1.00x. That is the unlocated stall
    /// the batch 0075 skew investigation attributed and could not name; it is
    /// not a cost of the second copy, and it moves in both directions.
    ///
    /// Compute permission is never an obligation: without `--par`, compute
    /// outlining stays off while completion keeps its own lowering. On every
    /// maintained native target, `WF_WORKERS=0` or `1` selects the sequential
    /// compute world, and invalid settings fail before the program body.
    /// Partial worker startup keeps the available workers; complete startup
    /// refusal uses the parallel body's ordinary-call fallback. Windows
    /// compute offers require the native runtime at link time.
    par: bool,
    /// Resolved scalar-leaf offer limit: 16 under --par unless overridden.
    /// Explicit `off` keeps every otherwise eligible offer; zero still filters
    /// leaves containing no nonconstant operations.
    scalar_leaf_limit: Option<u32>,
    /// Opt-in ordinary-ABI sequential calls on refused compute offers.
    sequential_refusal: bool,
    /// Control over the recursion budget: `None` leaves the `--par` default,
    /// which asks the runtime at each component entry.
    recursive_frontier: Option<RecursionBudget>,
    /// Emit the module a compiler with no overlap lowering at all emits.
    ///
    /// This is the sequential reference build, and it exists for one reason:
    /// measurement. The default compilation actualizes compiler-owned
    /// completion I/O, so without this switch there is no way to compile one
    /// source into the program that reaches the host through ordinary direct
    /// calls and compare the two. Every I/O call becomes an ordinary call:
    /// nothing is submitted, nothing is joined, and the completion runtime
    /// does not join the link. It is not a performance option a writer picks
    /// for a shipped program — the default build is what ships — and it
    /// changes no acceptance, no claim, and no published value.
    no_overlap: bool,
    /// Print the non-normative permission ledger on stdout.
    par_ledger: bool,
    /// Print the non-normative stack ledger on stdout.
    ///
    /// What a writer cannot otherwise see: a frame size is a whole-program,
    /// optimizer-chosen property, so no reading of a function tells anyone
    /// what one activation of it costs, and no reading of a program tells
    /// anyone which of its functions can reach themselves once the inliner has
    /// finished. The flagship program in this repository used to cap its
    /// directory recursion at sixteen levels, documenting that the truncation
    /// was indistinguishable from a complete search; its frame is 1744 bytes,
    /// which the runtime's own stack holds six hundred thousand of. The bound
    /// was not careful, it was blind, and this report is what replaced it.
    stack_ledger: bool,
    /// Check the sources through complete source acceptance and stop.
    check: bool,
    /// The module program's graph file [MOD-1], instead of source records.
    graph: Option<PathBuf>,
    /// The graph entry to build [MOD-9].
    entry: Option<String>,
    /// The function to build as an unnamed entry, `pkg::module::name` [MOD-9].
    function: Option<String>,
    /// The module to check against its dependencies' interfaces [MOD-8].
    check_module: Option<String>,
    /// Check only that module's interface, before any body exists.
    interface_only: bool,
    /// Check every registered module against its dependencies' interfaces,
    /// then every named entry's composition, and report each verdict: the
    /// impact report [MOD-8].
    check_modules: bool,
    /// Report verdicts as one JSON object per line.
    report: bool,
    /// The build cache directory whose records this invocation may reuse and
    /// publish. Without it nothing is reused, which is the differential
    /// reference for a cached run.
    cache: Option<PathBuf>,
    /// Split the emitted module into ThinLTO link fragments.
    fragments: Option<FragmentGranularity>,
    /// Research-only: link the program and every runtime unit as one full
    /// link-time optimization region, the runtime-quality comparator that the
    /// modular compilation design measures fragment builds against, not a
    /// build mode for programs.
    full_lto: bool,
    /// Print this module's resolved public interface [MOD-6, MOD-8].
    render_interface: Option<String>,
    /// The other revision's graph the rendered interface is compared with.
    against: Option<PathBuf>,
    output: Option<PathBuf>,
    sources: Vec<PathBuf>,
}

impl Options {
    fn parse(arguments: &[String]) -> Result<Self, String> {
        let mut emit_llvm = false;
        let mut par = false;
        let mut scalar_leaf_limit = None;
        let mut sequential_refusal = false;
        let mut recursive_frontier = None;
        let mut no_overlap = false;
        let mut par_ledger = false;
        let mut stack_ledger = false;
        let mut check = false;
        let mut graph = None;
        let mut entry = None;
        let mut function = None;
        let mut check_module = None;
        let mut interface_only = false;
        let mut check_modules = false;
        let mut report = false;
        let mut cache = None;
        let mut fragments = None;
        let mut full_lto = false;
        let mut render_interface = None;
        let mut against = None;
        let mut diagnostic_format = false;
        let mut output = None;
        let mut sources = Vec::new();
        let mut cursor = 0;
        while cursor < arguments.len() {
            match arguments[cursor].as_str() {
                "--emit-llvm" => emit_llvm = true,
                // Read by `requested_format` before parsing; validated here
                // like every other option.
                "--diagnostic-format" => {
                    cursor += 1;
                    if !matches!(
                        arguments.get(cursor).map(String::as_str),
                        Some("text" | "json")
                    ) {
                        return Err("--diagnostic-format requires text or json".to_owned());
                    }
                    if diagnostic_format {
                        return Err("--diagnostic-format may be written only once".to_owned());
                    }
                    diagnostic_format = true;
                }
                "--par" => par = true,
                "--par-scalar-leaf-limit" => {
                    cursor += 1;
                    let invalid =
                        || "--par-scalar-leaf-limit requires a nonnegative u32 or off".to_owned();
                    let value = arguments.get(cursor).ok_or_else(invalid)?;
                    let limit = if value == "off" {
                        None
                    } else {
                        if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                            return Err(invalid());
                        }
                        Some(value.parse::<u32>().map_err(|_| invalid())?)
                    };
                    if scalar_leaf_limit.replace(limit).is_some() {
                        return Err("--par-scalar-leaf-limit may be written only once".to_owned());
                    }
                }
                "--par-recursive-frontier" => {
                    cursor += 1;
                    let written = arguments.get(cursor).map(String::as_str);
                    let budget = match written {
                        Some("off") => RecursionBudget::Off,
                        Some("auto") => RecursionBudget::RuntimeDerived,
                        _ => written
                            .filter(|value| {
                                !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit())
                            })
                            .and_then(|value| value.parse::<u8>().ok())
                            .filter(|value| *value <= 32)
                            .and_then(std::num::NonZeroU8::new)
                            .map(RecursionBudget::Pinned)
                            .ok_or_else(|| {
                                "--par-recursive-frontier requires auto, an integer in 1..32, or off"
                                    .to_owned()
                            })?,
                    };
                    if recursive_frontier.replace(budget).is_some() {
                        return Err("--par-recursive-frontier may be written only once".to_owned());
                    }
                }
                "--par-sequential-refusal" => {
                    if sequential_refusal {
                        return Err("--par-sequential-refusal may be written only once".to_owned());
                    }
                    sequential_refusal = true;
                }
                "--no-overlap" => no_overlap = true,
                "--full-lto" => full_lto = true,
                "--check" => check = true,
                "--check-modules" => check_modules = true,
                "--report" => report = true,
                "--render-interface" | "--compare-interface" => {
                    let option = arguments[cursor].clone();
                    cursor += 1;
                    let value = arguments
                        .get(cursor)
                        .ok_or_else(|| format!("{option} requires a module path"))?
                        .clone();
                    if render_interface.replace(value).is_some() {
                        return Err("--render-interface and --compare-interface select one module: write one".to_owned());
                    }
                }
                "--against" => {
                    cursor += 1;
                    let path = arguments
                        .get(cursor)
                        .ok_or_else(|| "--against requires a graph file".to_owned())?;
                    if against.replace(PathBuf::from(path)).is_some() {
                        return Err("--against may be written only once".to_owned());
                    }
                }
                "--fragments" => {
                    cursor += 1;
                    let granularity = match arguments.get(cursor).map(String::as_str) {
                        Some("module") => FragmentGranularity::Module,
                        Some("function") => FragmentGranularity::Function,
                        _ => return Err("--fragments requires module or function".to_owned()),
                    };
                    if fragments.replace(granularity).is_some() {
                        return Err("--fragments may be written only once".to_owned());
                    }
                }
                "--cache" => {
                    cursor += 1;
                    let path = arguments
                        .get(cursor)
                        .ok_or_else(|| "--cache requires a directory".to_owned())?;
                    if cache.replace(PathBuf::from(path)).is_some() {
                        return Err("--cache may be written only once".to_owned());
                    }
                }
                "--check-module" | "--check-interface" => {
                    let option = arguments[cursor].clone();
                    cursor += 1;
                    let value = arguments
                        .get(cursor)
                        .ok_or_else(|| format!("{option} requires a module path"))?
                        .clone();
                    if check_module.replace(value).is_some() {
                        return Err(
                            "--check-module and --check-interface select one module: write one"
                                .to_owned(),
                        );
                    }
                    interface_only = option == "--check-interface";
                }
                "--graph" | "--entry" | "--function" => {
                    let option = arguments[cursor].clone();
                    cursor += 1;
                    let value = arguments
                        .get(cursor)
                        .ok_or_else(|| format!("{option} requires a value"))?
                        .clone();
                    let slot = match option.as_str() {
                        "--graph" => graph.replace(PathBuf::from(&value)).is_some(),
                        "--entry" => entry.replace(value).is_some(),
                        _ => function.replace(value).is_some(),
                    };
                    if slot {
                        return Err(format!("{option} may be written only once"));
                    }
                }
                "--par-ledger" => par_ledger = true,
                "--stack-ledger" => stack_ledger = true,
                "-o" => {
                    cursor += 1;
                    let path = arguments
                        .get(cursor)
                        .ok_or_else(|| "-o requires an output path".to_owned())?;
                    if output.replace(PathBuf::from(path)).is_some() {
                        return Err("-o may be written only once".to_owned());
                    }
                }
                "-h" | "--help" => {
                    return Err(USAGE.to_owned());
                }
                argument if argument.starts_with('-') => {
                    return Err(format!("unknown option: {argument}"));
                }
                source => sources.push(PathBuf::from(source)),
            }
            cursor += 1;
        }
        if graph.is_some() == !sources.is_empty() {
            return Err(USAGE.to_owned());
        }
        if check_module.is_some() && (graph.is_none() || entry.is_some() || function.is_some()) {
            return Err("--check-module and --check-interface check one module of a --graph program and select no entry".to_owned());
        }
        if check_modules
            && (graph.is_none() || entry.is_some() || function.is_some() || check_module.is_some())
        {
            return Err("--check-modules checks every module and entry of a --graph program and selects none".to_owned());
        }
        if report && graph.is_none() && (check || par_ledger) {
            return Err("--report reports a --graph check or a build".to_owned());
        }
        if report && emit_llvm && output.is_none() {
            return Err(
                "--report cannot share stdout with --emit-llvm: name a module output with -o"
                    .to_owned(),
            );
        }
        if render_interface.is_some()
            && (graph.is_none()
                || check
                || check_modules
                || check_module.is_some()
                || entry.is_some()
                || function.is_some())
        {
            return Err("--render-interface prints one module's interface of a --graph program and checks or builds nothing else".to_owned());
        }
        if against.is_some() && render_interface.is_none() {
            return Err(
                "--against names the revision a --compare-interface compares with".to_owned(),
            );
        }
        if fragments.is_some() && (cache.is_none() || emit_llvm) {
            return Err(
                "--fragments splits a cached link: write --cache and no --emit-llvm".to_owned(),
            );
        }
        if full_lto
            && (cache.is_some()
                || emit_llvm
                || check
                || check_modules
                || check_module.is_some()
                || render_interface.is_some())
        {
            return Err("--full-lto links one optimization region over a whole build: write no --cache, --emit-llvm or check".to_owned());
        }
        if graph.is_none() && (entry.is_some() || function.is_some()) {
            return Err(
                "--entry and --function select a module program's entry: write --graph".to_owned(),
            );
        }
        if entry.is_some() && function.is_some() {
            return Err("--entry and --function each select one entry: write one".to_owned());
        }
        if graph.is_some()
            && !check
            && !check_modules
            && render_interface.is_none()
            && check_module.is_none()
            && entry.is_none()
            && function.is_none()
        {
            return Err("a module program build selects an entry: write --entry NAME, --function pkg::module::name or --check".to_owned());
        }
        if graph.is_some() && par_ledger {
            return Err("--par-ledger reports a source bundle build".to_owned());
        }
        // Two streams, one stdout. The emitted module is the payload of
        // `--emit-llvm` without `-o`, so the ledger may not be interleaved
        // into it; naming an output file separates them.
        if par_ledger && emit_llvm && output.is_none() {
            return Err(
                "--par-ledger cannot share stdout with --emit-llvm: name a module output with -o"
                    .to_owned(),
            );
        }
        if stack_ledger && emit_llvm && output.is_none() {
            return Err(
                "--stack-ledger cannot share stdout with --emit-llvm: name a module output with -o"
                    .to_owned(),
            );
        }
        // The two switches name opposite lowerings, so an invocation that
        // writes both has stated no build at all rather than a build with a
        // precedence rule to remember.
        if par && no_overlap {
            return Err("--no-overlap and --par select opposite lowerings: write one".to_owned());
        }
        if scalar_leaf_limit.is_some() && !par {
            return Err("--par-scalar-leaf-limit requires --par".to_owned());
        }
        if sequential_refusal && !par {
            return Err("--par-sequential-refusal requires --par".to_owned());
        }
        if recursive_frontier.is_some() && !par {
            return Err("--par-recursive-frontier requires --par".to_owned());
        }
        Ok(Self {
            emit_llvm,
            par,
            scalar_leaf_limit: if par {
                scalar_leaf_limit.unwrap_or(Some(16))
            } else {
                None
            },
            sequential_refusal,
            recursive_frontier,
            no_overlap,
            par_ledger,
            stack_ledger,
            check,
            graph,
            entry,
            function,
            check_module,
            interface_only,
            check_modules,
            report,
            cache,
            fragments,
            full_lto,
            render_interface,
            against,
            output,
            sources,
        })
    }

    /// The lowering this invocation compiles: ordinary calls by default,
    /// with explicit compute-overlap controls selecting actualization.
    ///
    /// The judgment is pure, so this selects an emitted lowering and nothing
    /// about what the compiler decided.
    fn overlap(&self) -> OverlapLowering {
        if self.no_overlap {
            OverlapLowering::Off
        } else if let Some(budget) = self.recursive_frontier {
            OverlapLowering::OnWithRecursionBudget {
                budget,
                maximum_scalar_leaf_operations: self.scalar_leaf_limit,
                sequential_refusal: self.sequential_refusal,
            }
        } else if self.par && self.sequential_refusal {
            OverlapLowering::OnWithSequentialRefusal {
                maximum_scalar_leaf_operations: self.scalar_leaf_limit,
            }
        } else if self.par {
            self.scalar_leaf_limit
                .map_or(OverlapLowering::On, |maximum_operations| {
                    OverlapLowering::OnWithoutSmallScalarLeaves { maximum_operations }
                })
        } else {
            OverlapLowering::Off
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::path::{Component, Path, PathBuf};

    use super::{
        DiagnosticFormat, Options, OverlapLowering, RecursionBudget, Stop, requested_format,
        require_runner, runtime_units, source_names,
    };

    /// [PROG-3] a build refuses a checked module that has no runner, naming
    /// the generated caller's refusal when there is one, instead of leaving
    /// the missing body for the linker to report.
    #[test]
    fn a_build_without_a_runner_says_so() {
        let checked = |caller: &str| {
            whitefoot::compile(
                &[whitefoot::SourceInput::new("entry.wf", caller.as_bytes())],
                whitefoot::CompilerLimits::default(),
            )
            .expect("the program checks")
        };
        require_runner(&checked(
            "fn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n",
        ))
        .expect("an ExitStatus entry has a runner");
        let integer = require_runner(&checked(
            "fn main() -> result: u64 pure {\n  return 7_u64;\n}\n",
        ))
        .expect_err("a u64 result has no runner")
        .render(DiagnosticFormat::Text);
        assert!(integer.contains("no executable runner;"), "{integer}");
        let unproved = require_runner(&checked(
            "fn main() -> result: unit pure contract {\n  requires 1_u64 <= 0_u64;\n} {\n  return unit;\n}\n",
        ))
        .expect_err("an unprovable requirement leaves no runner")
        .render(DiagnosticFormat::Text);
        assert!(
            unproved.contains("its generated caller was not admitted"),
            "{unproved}"
        );
    }

    fn parse(arguments: &[&str]) -> Result<Options, String> {
        let owned: Vec<String> = arguments.iter().map(|value| (*value).to_owned()).collect();
        Options::parse(&owned)
    }

    /// The ordinary linked library uses one platform link set independent of
    /// source call classification. Both plain arithmetic and I/O-shaped source
    /// receive its transitive headers and native implementation units.
    #[test]
    fn the_ordinary_library_links_the_same_units_for_every_source_module() {
        let (staged, compiled) = runtime_units();
        for required in [
            "ordinary_values.c",
            "ordinary_values.ll",
            "sched/core.c",
            "sched/entry.c",
            "completion/bridge.c",
        ] {
            assert!(
                compiled.contains(&required),
                "missing library implementation {required}"
            );
            assert!(staged.iter().any(|unit| unit.relative_path == required));
        }
    }

    /// The driver stages the embedded sources with the same relative topology
    /// they have under `backend/`. Every quoted compiler-owned include must
    /// therefore resolve either beside the including file or from the one `-I`
    /// root passed to clang.
    ///
    /// This is stronger than naming the paths that exposed the original
    /// flattening bug: adding a new compiler-owned quoted include without its
    /// staged target makes this test fail before CI reaches clang. It runs on
    /// whichever host builds the compiler and so covers that host's own leaf
    /// selection.
    #[test]
    fn runtime_staging_closes_every_quoted_include() {
        fn normalized(path: &Path) -> Option<PathBuf> {
            let mut result = PathBuf::new();
            for component in path.components() {
                match component {
                    Component::CurDir => {}
                    Component::ParentDir => {
                        if !result.pop() {
                            return None;
                        }
                    }
                    Component::Normal(piece) => result.push(piece),
                    Component::Prefix(_) | Component::RootDir => return None,
                }
            }
            Some(result)
        }

        let (units, compiled) = runtime_units();
        let staged: HashSet<PathBuf> = units
            .iter()
            .map(|unit| PathBuf::from(unit.relative_path))
            .collect();
        assert_eq!(
            staged.len(),
            units.len(),
            "a staged runtime path is duplicated"
        );
        assert!(staged.contains(Path::new("sched/core.c")));
        assert!(staged.contains(Path::new("completion/bridge.c")));
        assert!(
            !staged.contains(Path::new("bridge.c")),
            "completion files must not be flattened into the staging root"
        );

        for relative_path in &compiled {
            assert!(
                staged.contains(Path::new(relative_path)),
                "clang input `{relative_path}` is not staged"
            );
        }

        for unit in &units {
            let parent = Path::new(unit.relative_path)
                .parent()
                .unwrap_or_else(|| Path::new(""));
            for line in unit.source.lines() {
                let Some(include) = line
                    .trim()
                    .strip_prefix("#include \"")
                    .and_then(|rest| rest.strip_suffix('"'))
                else {
                    continue;
                };
                let beside_source = normalized(&parent.join(include));
                let from_include_root = normalized(Path::new(include));
                assert!(
                    beside_source
                        .as_ref()
                        .is_some_and(|path| staged.contains(path))
                        || from_include_root
                            .as_ref()
                            .is_some_and(|path| staged.contains(path)),
                    "{} includes `{include}`, which the staged tree cannot resolve",
                    unit.relative_path
                );
            }
        }
    }

    /// Every reader-facing name is the argument the caller typed, including an
    /// absolute one, while the bundle's own key stays inside the closed
    /// portable spelling.
    ///
    /// The two answers differ exactly when the host path cannot be spelled as
    /// a logical path, and that is the case a writer meets first: a diagnostic
    /// that renamed `/tmp/wc.wf` to `input0.wf` cited a file that does not
    /// exist.
    #[test]
    fn a_source_is_shown_by_the_path_the_caller_wrote() {
        let (logical, display) = source_names(Path::new("/tmp/wc.wf"), 0);
        assert_eq!(display, "/tmp/wc.wf");
        assert_eq!(logical, "input0.wf");

        let (logical, display) = source_names(Path::new("../out of tree.wf"), 3);
        assert_eq!(display, "../out of tree.wf");
        assert_eq!(logical, "input3.wf");

        // A portable relative path is already both names.
        let (logical, display) = source_names(Path::new("programs/wc.wf"), 0);
        assert_eq!(display, "programs/wc.wf");
        assert_eq!(logical, "programs/wc.wf");
    }

    /// The permission ledger is an opt-in developer channel: off unless the
    /// invocation asks for it, and never a second source argument.
    #[test]
    fn the_permission_ledger_is_requested_by_its_own_option() {
        let options = parse(&["value.wf"]).expect("one source is a complete invocation");
        assert!(!options.par_ledger);
        assert_eq!(options.sources.len(), 1);

        let options = parse(&["--par-ledger", "value.wf"]).expect("the option is accepted");
        assert!(options.par_ledger);
        assert!(!options.emit_llvm);
        assert_eq!(options.sources.len(), 1);

        // With a named module output the two streams no longer share stdout,
        // so requesting both is a complete invocation.
        let options = parse(&["--par-ledger", "--emit-llvm", "-o", "out.ll", "value.wf"])
            .expect("a named output separates the module from the ledger");
        assert!(options.par_ledger);
        assert!(options.emit_llvm);
    }

    /// The stack ledger is developer output on its own switch, and it is
    /// independent of `--par`: a writer reads what a program's frames cost in
    /// the build they are shipping, not in one the flag changed underneath
    /// them. Asking for it with `--par` reports both of that module's worlds
    /// instead of one.
    #[test]
    fn the_stack_ledger_is_requested_by_its_own_option() {
        let options = parse(&["value.wf"]).expect("one source is a complete invocation");
        assert!(!options.stack_ledger);

        let options = parse(&["--stack-ledger", "value.wf"]).expect("the option is accepted");
        assert!(options.stack_ledger);
        assert!(!options.par, "reading the ledger must not enable lowering");
        assert!(!options.emit_llvm);

        let options = parse(&["--par", "--stack-ledger", "value.wf"]).expect("both are accepted");
        assert!(options.stack_ledger && options.par);
    }

    /// Either ledger may not be interleaved into a module that is itself going
    /// to stdout, so that invocation is refused rather than corrupted.
    #[test]
    fn a_ledger_refuses_to_share_stdout_with_the_module() {
        let message = parse(&["--stack-ledger", "--emit-llvm", "value.wf"])
            .err()
            .expect("the two stdout streams must not be mixed");
        assert!(message.contains("--stack-ledger"), "{message}");
        assert!(message.contains("-o"), "{message}");
    }

    /// The ledger may not be interleaved into a module that is itself going
    /// to stdout, so that invocation is refused rather than corrupted.
    #[test]
    fn the_permission_ledger_refuses_to_share_stdout_with_the_module() {
        let message = parse(&["--par-ledger", "--emit-llvm", "value.wf"])
            .err()
            .expect("the two stdout streams must not be mixed");
        assert!(message.contains("--par-ledger"), "{message}");
        assert!(message.contains("-o"), "{message}");
    }

    /// Overlap lowering is off unless the invocation asks for it, and asking
    /// for the ledger is not asking for it.
    ///
    /// The two options are independent on purpose: the judgment is pure, so a
    /// developer can read what the compiler decided about a program without
    /// changing one byte of the program that gets built.
    #[test]
    fn overlap_lowering_is_off_unless_the_invocation_asks_for_it() {
        let options = parse(&["value.wf"]).expect("one source is a complete invocation");
        assert!(!options.par);

        let options = parse(&["--par-ledger", "value.wf"]).expect("the ledger option is accepted");
        assert!(!options.par, "reading the ledger must not enable lowering");

        let options = parse(&["--par", "value.wf"]).expect("the option is accepted");
        assert!(options.par);
        assert!(!options.par_ledger, "lanes are not a ledger request");
        assert_eq!(options.sources.len(), 1);
    }

    #[test]
    fn compute_leaf_default_and_explicit_overrides_select_the_expected_lowering() {
        assert_eq!(
            parse(&["value.wf"]).unwrap().overlap(),
            OverlapLowering::Off
        );
        assert_eq!(
            parse(&["--par", "value.wf"]).unwrap().overlap(),
            OverlapLowering::OnWithoutSmallScalarLeaves {
                maximum_operations: 16
            }
        );
        assert_eq!(
            parse(&["--par", "--par-scalar-leaf-limit", "off", "value.wf"])
                .unwrap()
                .overlap(),
            OverlapLowering::On
        );
        assert_eq!(
            parse(&["--par", "--par-scalar-leaf-limit", "0", "value.wf"])
                .unwrap()
                .overlap(),
            OverlapLowering::OnWithoutSmallScalarLeaves {
                maximum_operations: 0
            }
        );
        for (arguments, expected) in [
            (
                vec!["--par", "--par-sequential-refusal", "value.wf"],
                Some(16),
            ),
            (
                vec![
                    "--par",
                    "--par-sequential-refusal",
                    "--par-scalar-leaf-limit",
                    "off",
                    "value.wf",
                ],
                None,
            ),
        ] {
            assert_eq!(
                parse(&arguments).unwrap().overlap(),
                OverlapLowering::OnWithSequentialRefusal {
                    maximum_scalar_leaf_operations: expected
                }
            );
        }
        assert_eq!(
            parse(&[
                "--par",
                "--par-recursive-frontier",
                "8",
                "--par-scalar-leaf-limit",
                "off",
                "value.wf"
            ])
            .unwrap()
            .overlap(),
            OverlapLowering::OnWithRecursionBudget {
                budget: RecursionBudget::Pinned(std::num::NonZeroU8::new(8).unwrap()),
                maximum_scalar_leaf_operations: None,
                sequential_refusal: false,
            }
        );
        assert!(parse(&["--par-scalar-leaf-limit", "off", "value.wf"]).is_err());
        assert!(
            parse(&[
                "--par",
                "--par-scalar-leaf-limit",
                "off",
                "--par-scalar-leaf-limit",
                "16",
                "value.wf"
            ])
            .is_err()
        );
    }

    #[test]
    fn scalar_leaf_control_requires_an_explicit_compute_invocation() {
        let options = parse(&["--par", "--par-scalar-leaf-limit", "16", "value.wf"])
            .expect("the scalar control is available for compute experiments");
        assert_eq!(
            options.overlap(),
            OverlapLowering::OnWithoutSmallScalarLeaves {
                maximum_operations: 16
            }
        );
        for arguments in [
            vec!["--par-scalar-leaf-limit", "16", "value.wf"],
            vec!["--par", "--par-scalar-leaf-limit"],
            vec!["--par", "--par-scalar-leaf-limit", "-1", "value.wf"],
            vec!["--par", "--par-scalar-leaf-limit", "4294967296", "value.wf"],
            vec![
                "--par",
                "--par-scalar-leaf-limit",
                "1",
                "--par-scalar-leaf-limit",
                "2",
                "value.wf",
            ],
        ] {
            assert!(parse(&arguments).is_err(), "{arguments:?}");
        }
    }

    #[test]
    fn recursive_frontier_requires_compute_and_composes_with_other_controls() {
        let options = parse(&[
            "--par",
            "--par-recursive-frontier",
            "8",
            "--par-sequential-refusal",
            "--par-scalar-leaf-limit",
            "16",
            "value.wf",
        ])
        .unwrap();
        assert_eq!(
            options.overlap(),
            OverlapLowering::OnWithRecursionBudget {
                budget: RecursionBudget::Pinned(std::num::NonZeroU8::new(8).unwrap()),
                maximum_scalar_leaf_operations: Some(16),
                sequential_refusal: true,
            }
        );
        // Writing nothing is the default, and the default is the runtime's
        // answer: one mechanism with a pinned or derived starting value.
        assert_eq!(
            parse(&["--par", "value.wf"]).unwrap().overlap(),
            OverlapLowering::OnWithoutSmallScalarLeaves {
                maximum_operations: 16
            }
        );
        assert_eq!(
            parse(&["--par", "--par-recursive-frontier", "off", "value.wf"])
                .unwrap()
                .overlap(),
            OverlapLowering::OnWithRecursionBudget {
                budget: RecursionBudget::Off,
                maximum_scalar_leaf_operations: Some(16),
                sequential_refusal: false,
            }
        );
        for value in ["1", "32", "off", "auto"] {
            assert!(parse(&["--par", "--par-recursive-frontier", value, "value.wf"]).is_ok());
        }
        assert_eq!(
            parse(&["--par", "--par-recursive-frontier", "auto", "value.wf"])
                .unwrap()
                .overlap(),
            OverlapLowering::OnWithRecursionBudget {
                budget: RecursionBudget::RuntimeDerived,
                maximum_scalar_leaf_operations: Some(16),
                sequential_refusal: false,
            }
        );
        for value in [
            "0", "33", "256", "-1", "+1", "", "1.5", "Off", "none", "Auto",
        ] {
            let error = parse(&["--par", "--par-recursive-frontier", value, "value.wf"])
                .err()
                .unwrap();
            assert!(error.contains("--par-recursive-frontier"), "{error}");
        }
        for args in [
            vec!["--par-recursive-frontier", "8", "value.wf"],
            vec!["--par", "--par-recursive-frontier"],
            vec![
                "--par",
                "--par-recursive-frontier",
                "8",
                "--no-overlap",
                "value.wf",
            ],
            vec![
                "--par",
                "--par-recursive-frontier",
                "8",
                "--par-recursive-frontier",
                "8",
                "value.wf",
            ],
            vec!["--par-recursive-frontier", "off", "value.wf"],
            vec![
                "--par",
                "--par-recursive-frontier",
                "off",
                "--par-recursive-frontier",
                "8",
                "value.wf",
            ],
        ] {
            assert!(parse(&args).is_err(), "{args:?}");
        }
    }

    #[test]
    fn sequential_refusal_requires_compute_and_composes_with_leaf_control() {
        let options = parse(&[
            "--par",
            "--par-sequential-refusal",
            "--par-scalar-leaf-limit",
            "16",
            "value.wf",
        ])
        .unwrap();
        assert_eq!(
            options.overlap(),
            OverlapLowering::OnWithSequentialRefusal {
                maximum_scalar_leaf_operations: Some(16)
            }
        );
        for args in [
            vec!["--par-sequential-refusal", "value.wf"],
            vec![
                "--par",
                "--par-sequential-refusal",
                "--par-sequential-refusal",
                "value.wf",
            ],
        ] {
            assert!(parse(&args).is_err());
        }
    }

    /// The sequential reference build is its own switch, off unless asked
    /// for, and it may not be written together with the switch that selects
    /// the opposite lowering.
    #[test]
    fn the_sequential_reference_build_is_requested_by_its_own_option() {
        let options = parse(&["value.wf"]).expect("one source is a complete invocation");
        assert!(!options.no_overlap, "the default build is the shipped one");

        let options = parse(&["--no-overlap", "value.wf"]).expect("the option is accepted");
        assert!(options.no_overlap);
        assert!(!options.par, "the reference build asks for no lanes");

        let message = parse(&["--par", "--no-overlap", "value.wf"])
            .err()
            .expect("opposite lowerings may not be written together");
        assert!(message.contains("--no-overlap"), "{message}");
        assert!(message.contains("--par"), "{message}");
    }

    /// The diagnostic format is one option with two values, read before the
    /// other options so an invalid option is reported in the requested form,
    /// and validated with them so a misspelled value is refused rather than
    /// silently selecting text.
    #[test]
    fn the_diagnostic_format_is_one_option_with_two_values() {
        let owned = |arguments: &[&str]| -> Vec<String> {
            arguments.iter().map(|value| (*value).to_owned()).collect()
        };
        assert_eq!(
            requested_format(&owned(&["value.wf"])),
            DiagnosticFormat::Text
        );
        assert_eq!(
            requested_format(&owned(&["--diagnostic-format", "json", "value.wf"])),
            DiagnosticFormat::Json
        );
        assert_eq!(
            requested_format(&owned(&["--diagnostic-format", "text", "value.wf"])),
            DiagnosticFormat::Text
        );
        // An unknown option is still reported in the requested format.
        assert_eq!(
            requested_format(&owned(&["--diagnostic-format", "json", "--bogus"])),
            DiagnosticFormat::Json
        );
        assert!(parse(&["--bogus", "value.wf"]).is_err());

        for value in ["text", "json"] {
            parse(&["--diagnostic-format", value, "value.wf"]).expect("both formats are accepted");
        }
        assert!(super::USAGE.contains("--diagnostic-format text|json"));
        for arguments in [
            vec!["--diagnostic-format", "xml", "value.wf"],
            vec!["--diagnostic-format", "JSON", "value.wf"],
            vec!["value.wf", "--diagnostic-format"],
            vec![
                "--diagnostic-format",
                "json",
                "--diagnostic-format",
                "text",
                "value.wf",
            ],
        ] {
            let message = parse(&arguments).err().expect("an invalid format");
            assert!(message.contains("--diagnostic-format"), "{message}");
        }
    }

    /// A stop outside the pipeline keeps its one-sentence text form and gains
    /// the common envelope in JSON.
    #[test]
    fn a_driver_stop_renders_in_either_format() {
        let stop = Stop::invocation("cannot read missing.wf: not found".to_owned());
        assert_eq!(
            stop.render(DiagnosticFormat::Text),
            "whitefootc: cannot read missing.wf: not found"
        );
        assert_eq!(
            stop.render(DiagnosticFormat::Json),
            r#"{"category":"Invocation","stage":"Driver","detail":{"message":"cannot read missing.wf: not found"}}"#
        );
    }

    /// The usage text is one definition, so the option list a reader is shown
    /// cannot drift from the option list the parser accepts.
    #[test]
    fn the_usage_text_lists_every_accepted_option() {
        for option in [
            "--emit-llvm",
            "--par",
            "--no-overlap",
            "--par-ledger",
            "--stack-ledger",
            "-o",
        ] {
            assert!(
                super::USAGE.contains(option),
                "usage text omits {option}: {}",
                super::USAGE
            );
            parse(&[option, "out.ll", "value.wf"])
                .expect("every option the usage text lists must parse");
        }
        let message = parse(&["--par-legder", "value.wf"])
            .err()
            .expect("a misspelled option must be refused");
        assert!(message.contains("unknown option"), "{message}");
    }

    /// The module-program options: `--check-modules` and `--report` need a
    /// graph, and `--fragments` splits only a cached link.
    #[test]
    fn module_program_options_state_what_they_need() {
        let checks = parse(&["--graph", "modules.wfg", "--check-modules", "--report"])
            .expect("every module and entry is checked");
        assert!(checks.check_modules && checks.report);
        for refused in [
            &["--check-modules", "value.wf"][..],
            &[
                "--graph",
                "modules.wfg",
                "--check-modules",
                "--entry",
                "app",
            ][..],
            &["--report", "--check", "value.wf"][..],
            &[
                "--graph",
                "modules.wfg",
                "--entry",
                "app",
                "--fragments",
                "module",
            ][..],
            &[
                "--graph",
                "modules.wfg",
                "--entry",
                "app",
                "--cache",
                "c",
                "--fragments",
                "file",
            ][..],
        ] {
            assert!(parse(refused).is_err(), "{refused:?} must be refused");
        }
        let build = parse(&[
            "--graph",
            "modules.wfg",
            "--entry",
            "app",
            "--cache",
            "cache",
            "--fragments",
            "function",
        ])
        .expect("a cached fragment build");
        assert_eq!(
            build.fragments,
            Some(whitefoot::FragmentGranularity::Function)
        );
        // A source bundle's build reports its phases and its proof reuse too.
        let bundle = parse(&["--cache", "cache", "--report", "value.wf"])
            .expect("a reported source-bundle build");
        assert!(bundle.report && bundle.graph.is_none());
        // The full link-time optimization comparator links a whole build and
        // nothing else.
        assert!(
            parse(&["--graph", "modules.wfg", "--entry", "app", "--full-lto"])
                .expect("a full-LTO build")
                .full_lto
        );
        for refused in [
            &["--full-lto", "--cache", "cache", "value.wf"][..],
            &["--full-lto", "--emit-llvm", "value.wf"][..],
            &["--full-lto", "--check", "value.wf"][..],
            &["--graph", "modules.wfg", "--check-modules", "--full-lto"][..],
        ] {
            assert!(parse(refused).is_err(), "{refused:?} must be refused");
        }
        for option in [
            "--cache",
            "--fragments",
            "--full-lto",
            "--report",
            "--check-modules",
            "--render-interface",
            "--compare-interface",
            "--against",
        ] {
            assert!(super::USAGE.contains(option), "usage text omits {option}");
        }
        let compare = parse(&[
            "--graph",
            "new/modules.wfg",
            "--compare-interface",
            "pkg::data",
            "--against",
            "old/modules.wfg",
        ])
        .expect("a revision comparison");
        assert_eq!(compare.render_interface.as_deref(), Some("pkg::data"));
        assert!(compare.against.is_some());
        for refused in [
            &["--graph", "modules.wfg", "--against", "old/modules.wfg"][..],
            &[
                "--graph",
                "modules.wfg",
                "--render-interface",
                "pkg::data",
                "--check",
            ][..],
            &["--render-interface", "pkg::data", "value.wf"][..],
        ] {
            assert!(parse(refused).is_err(), "{refused:?} must be refused");
        }
    }

    /// [MOD-8] a cached build links the emitted module's native fragments
    /// with ThinLTO: the executable runs as the program says, a warm build
    /// compiles no object, and an edit to one function's body recompiles
    /// exactly one fragment, in either granularity.
    #[test]
    fn a_fragment_build_recompiles_only_the_edited_fragment() {
        use whitefoot::{CompilerLimits, FragmentGranularity, SourceInput, compile};
        let emitted = |fallback: u64| {
            let source = format!(
                "const lookup: Array<u8, 8> =[0_u8, 1_u8, 2_u8, 3_u8, 4_u8, 5_u8, 6_u8, 7_u8];\n\n\
                 fn clamp(value: u64) -> result: u64 pure contract {{\n  ensures result <= 7_u64;\n}} {{\n  if value <= 7_u64 {{\n    return value;\n  }}\n  return {fallback}_u64;\n}}\n\n\
                 fn select(index: u64) -> result: u8 pure {{\n  let bounded = clamp(value: index);\n  return lookup[bounded];\n}}\n\n\
                 fn main() -> status: std::process::ExitStatus pure {{\n  let code = select(index: 9_u64);\n  return std::process::exit_status(code: code);\n}}\n"
            );
            compile(
                &[SourceInput::new("fragments.wf", source.as_bytes())],
                CompilerLimits::default(),
            )
            .expect("the program compiles")
        };
        let root =
            std::env::temp_dir().join(format!("whitefootc-fragment-build-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create the test directory");
        let executable = root.join("program");
        for granularity in [FragmentGranularity::Function, FragmentGranularity::Module] {
            let cache =
                super::open_cache(&root.join(format!("{granularity:?}"))).expect("open the cache");
            let build = |llvm: &str| {
                let mut report = super::BuildReport::default();
                super::compile_executable(
                    llvm,
                    &executable,
                    Some(&cache),
                    Some(granularity),
                    false,
                    &mut report,
                )
                .expect("the fragments link");
                (report.objects_compiled, report.objects_reused)
            };
            let status = || {
                std::process::Command::new(&executable)
                    .status()
                    .expect("run the program")
                    .code()
            };
            let (compiled, reused) = build(&emitted(5));
            assert_eq!(reused, 0, "{granularity:?}");
            assert_eq!(status(), Some(5), "{granularity:?}");
            assert_eq!(
                build(&emitted(5)),
                (0, compiled),
                "{granularity:?}: a warm build compiles nothing"
            );
            assert_eq!(
                build(&emitted(6)),
                (1, compiled - 1),
                "{granularity:?}: an edit to one body"
            );
            assert_eq!(status(), Some(6), "{granularity:?}");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The full link-time optimization comparator links the program with
    /// every runtime unit into an executable that runs as the program says.
    #[test]
    fn a_full_lto_build_runs_the_program() {
        use whitefoot::{CompilerLimits, SourceInput, compile};
        let source = b"fn select(index: u64) -> result: u64 pure {\n  let result = index *wrap 3_u64;\n  return result;\n}\n\nfn main() -> status: std::process::ExitStatus pure {\n  let chosen = select(index: 3_u64);\n  match cvt.checked::<u64, u8>(chosen) {\n    Ok(value: code) => {\n      return std::process::exit_status(code: code);\n    }\n    Err(error: refused) => {\n      return std::process::exit_status(code: 255_u8);\n    }\n  }\n}\n";
        let llvm = compile(
            &[SourceInput::new("lto.wf", source)],
            CompilerLimits::default(),
        )
        .expect("the program compiles");
        let root = std::env::temp_dir().join(format!("whitefootc-full-lto-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create the test directory");
        let executable = root.join("program");
        super::compile_executable(
            &llvm,
            &executable,
            None,
            None,
            true,
            &mut super::BuildReport::default(),
        )
        .expect("the full-LTO link");
        let status = std::process::Command::new(&executable)
            .status()
            .expect("run the program");
        assert_eq!(status.code(), Some(9));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_report_string_is_escaped_json() {
        assert_eq!(
            super::json_string("a \"quoted\" \\ line\nnext\u{1}"),
            "\"a \\\"quoted\\\" \\\\ line\\nnext\\u0001\""
        );
    }
}
