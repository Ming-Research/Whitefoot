//! Native build reuse for unit, program and conformance test binaries.
//!
//! Every case keeps its own emitted module, executable and fixture directory.
//! Only immutable library objects built by the same compiler with the same
//! options are reused, in memory for this test process. Scripted backend tests
//! with translation-unit defines retain their separate build path.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use whitefoot::{
    COMPLETION_BRIDGE_HEADER, COMPLETION_BRIDGE_SOURCE, COMPLETION_CONTRACT_HEADER,
    COMPLETION_FILE_ADAPTER_HEADER, COMPLETION_FILE_ADAPTER_SOURCE, COMPLETION_FILE_POSIX_HEADER,
    COMPLETION_LINUX_IO_URING_HEADER, COMPLETION_RUNTIME_SOURCE, COMPLETION_SOCKET_ADDRESS_HEADER,
    COMPLETION_WINDOWS_IOCP_HEADER, HOST_OPTIMIZATION_ARGUMENTS, ORDINARY_VALUES_HEADER,
    ORDINARY_VALUES_LLVM, ORDINARY_VALUES_SOURCE, SCHED_CORE_HEADER, SCHED_CORE_SOURCE,
    SCHED_ENTRY_HEADER, SCHED_ENTRY_SOURCE, SCHED_PRIM_HEADER, WINDOWS_RUNTIME_HEADER,
};

#[cfg(unix)]
use whitefoot::{
    COMPLETION_FILE_POSIX_SOURCE as FILE_SOURCE, COMPLETION_LINUX_IO_URING_SOURCE as NATIVE_SOURCE,
    COMPLETION_WAIT_HOST_SOURCE as WAIT_SOURCE, FLOOR_RUNTIME_SOURCE as FLOOR_SOURCE,
    SCHED_PRIM_HOST_SOURCE as PRIM_SOURCE,
};
#[cfg(windows)]
use whitefoot::{
    COMPLETION_FILE_WINDOWS_SOURCE as FILE_SOURCE, COMPLETION_WAIT_WINDOWS_SOURCE as WAIT_SOURCE,
    COMPLETION_WINDOWS_IOCP_SOURCE as NATIVE_SOURCE, FLOOR_WINDOWS_RUNTIME_SOURCE as FLOOR_SOURCE,
    SCHED_PRIM_WINDOWS_SOURCE as PRIM_SOURCE, WINDOWS_RUNTIME_SOURCE,
};

#[cfg(unix)]
pub(crate) const CLANG: &str = "/usr/bin/clang";
#[cfg(windows)]
pub(crate) const CLANG: &str = "clang";
#[cfg(unix)]
pub(crate) const COMPILE_ARGUMENTS: &[&str] = &["-pthread"];
#[cfg(windows)]
pub(crate) const COMPILE_ARGUMENTS: &[&str] = &[];
#[cfg(unix)]
#[allow(dead_code)]
pub(crate) const LINK_LIBRARIES: &[&str] = whitefoot::HOST_LINK_LIBRARIES;
#[cfg(windows)]
pub(crate) const LINK_LIBRARIES: &[&str] = &["-lws2_32", "-lshell32"];

static DEFAULT_OBJECTS: OnceLock<Vec<Vec<u8>>> = OnceLock::new();
static C11_OBJECTS: OnceLock<Vec<Vec<u8>>> = OnceLock::new();
static NEXT_BUILD: AtomicU64 = AtomicU64::new(0);

/// Records optional test-helper wall times separately from Cargo construction.
///
/// `WHITEFOOT_TEST_TIMINGS` names an append-only TSV file. Rows contain process,
/// test thread, phase and elapsed seconds. Parent phases include their children;
/// parallel rows must not be summed as suite wall time. Logging never selects a
/// compiler verdict or changes the callback's result.
#[allow(dead_code)]
pub(crate) fn timed<T>(phase: &str, run: impl FnOnce() -> T) -> T {
    static OUTPUT: OnceLock<Option<Mutex<std::fs::File>>> = OnceLock::new();
    let output = OUTPUT.get_or_init(|| {
        std::env::var_os("WHITEFOOT_TEST_TIMINGS").and_then(
            |path| match std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
            {
                Ok(file) => Some(Mutex::new(file)),
                Err(error) => {
                    eprintln!("test timing output unavailable: {error}");
                    None
                }
            },
        )
    });
    let Some(output) = output else {
        return run();
    };
    let start = Instant::now();
    let result = run();
    let thread = std::thread::current();
    let row = format!(
        "{}\t{}\t{phase}\t{:.6}\n",
        std::process::id(),
        thread.name().unwrap_or("unnamed"),
        start.elapsed().as_secs_f64(),
    );
    if let Ok(mut file) = output.lock()
        && let Err(error) = file.write_all(row.as_bytes())
    {
        eprintln!("test timing output failed: {error}");
    }
    result
}

const SOURCES: &[(&str, &str)] = &[
    ("wf_floor.c", FLOOR_SOURCE),
    ("ordinary_values.h", ORDINARY_VALUES_HEADER),
    ("ordinary_values.c", ORDINARY_VALUES_SOURCE),
    ("ordinary_values.ll", ORDINARY_VALUES_LLVM),
    ("sched/core.h", SCHED_CORE_HEADER),
    ("sched/prim.h", SCHED_PRIM_HEADER),
    ("sched/entry.h", SCHED_ENTRY_HEADER),
    ("sched/core.c", SCHED_CORE_SOURCE),
    ("sched/prim_host.c", PRIM_SOURCE),
    ("sched/entry.c", SCHED_ENTRY_SOURCE),
    ("completion/contract.h", COMPLETION_CONTRACT_HEADER),
    ("completion/file_adapter.h", COMPLETION_FILE_ADAPTER_HEADER),
    ("completion/bridge.h", COMPLETION_BRIDGE_HEADER),
    ("completion/file_posix.h", COMPLETION_FILE_POSIX_HEADER),
    (
        "completion/socket_address.h",
        COMPLETION_SOCKET_ADDRESS_HEADER,
    ),
    (
        "completion/linux_io_uring.h",
        COMPLETION_LINUX_IO_URING_HEADER,
    ),
    ("completion/windows_iocp.h", COMPLETION_WINDOWS_IOCP_HEADER),
    ("windows_runtime.h", WINDOWS_RUNTIME_HEADER),
    #[cfg(windows)]
    ("windows_runtime.c", WINDOWS_RUNTIME_SOURCE),
    ("completion/completion_runtime.c", COMPLETION_RUNTIME_SOURCE),
    ("completion/wait_host.c", WAIT_SOURCE),
    ("completion/file_adapter.c", COMPLETION_FILE_ADAPTER_SOURCE),
    ("completion/file_posix.c", FILE_SOURCE),
    ("completion/completion_bridge.c", COMPLETION_BRIDGE_SOURCE),
    ("completion/linux_io_uring.c", NATIVE_SOURCE),
];

// Preserve the original link order, including all library bodies. These are
// separate object inputs, not archive members selected by symbol reachability.
const UNITS: &[&str] = &[
    "wf_floor.c",
    "sched/core.c",
    "sched/prim_host.c",
    "sched/entry.c",
    "completion/completion_runtime.c",
    "completion/wait_host.c",
    "completion/file_adapter.c",
    "completion/file_posix.c",
    "completion/completion_bridge.c",
    "completion/linux_io_uring.c",
    "ordinary_values.c",
    "ordinary_values.ll",
    #[cfg(windows)]
    "windows_runtime.c",
];

fn stage_sources(directory: &Path) -> Vec<PathBuf> {
    for name in ["completion", "sched"] {
        std::fs::create_dir_all(directory.join(name)).expect("stage native source directory");
    }
    SOURCES
        .iter()
        .map(|(name, source)| {
            let path = directory.join(name);
            std::fs::write(&path, source).expect("write native source unit");
            path
        })
        .collect()
}

fn compile_objects(c_standard: Option<&str>) -> Vec<Vec<u8>> {
    let sequence = NEXT_BUILD.fetch_add(1, Ordering::Relaxed);
    let directory = std::env::temp_dir().join(format!(
        "whitefoot-native-objects-{}-{sequence}",
        std::process::id()
    ));
    std::fs::create_dir(&directory).expect("create unique native object build directory");
    let _sources = stage_sources(&directory);
    let objects = UNITS
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let object = directory.join(format!("unit-{index}.o"));
            let mut command = Command::new(CLANG);
            if let Some(standard) = c_standard {
                command.arg(format!("-std={standard}"));
            }
            command
                .args(COMPILE_ARGUMENTS)
                .args(HOST_OPTIMIZATION_ARGUMENTS)
                .arg("-I")
                .arg(directory.join("completion"))
                .arg("-I")
                .arg(&directory)
                .arg("-x")
                .arg(if name.ends_with(".ll") { "ir" } else { "c" })
                .arg(directory.join(name))
                .arg("-c")
                .arg("-o")
                .arg(&object);
            let result = run_command(&mut command);
            assert!(
                result.status.success(),
                "clang rejected native unit {name}:\n{}",
                String::from_utf8_lossy(&result.stderr)
            );
            std::fs::read(object).expect("read native library object")
        })
        .collect();
    std::fs::remove_dir_all(directory).expect("remove native object build directory");
    objects
}

/// Appends all ordinary library objects and, optionally, one fresh observer.
///
/// `None` retains the host C dialect; `Some("c11")` retains the grant observer's
/// explicit dialect. The fixed source bytes, compiler path and options belong
/// to this process, so neither entry can outlive a rebuilt integration binary.
/// Source files are still staged for the caller's existing cleanup policy.
/// The caller must remove the returned object paths before executing a case;
/// directory-walking programs must not observe new build artifacts.
pub(crate) fn append_runtime_objects(
    command: &mut Command,
    directory: &Path,
    c_standard: Option<&str>,
    observer: Option<&Path>,
) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let sources = stage_sources(directory);
    let objects = match c_standard {
        None => DEFAULT_OBJECTS.get_or_init(|| compile_objects(None)),
        Some("c11") => C11_OBJECTS.get_or_init(|| compile_objects(Some("c11"))),
        Some(other) => panic!("unrecorded native test dialect: {other}"),
    };
    // These includes also reach a fresh observer, exactly as in the original
    // whole-source link. The observer is inserted after the floor object.
    command
        .arg("-I")
        .arg(directory.join("completion"))
        .arg("-I")
        .arg(directory);
    let paths = objects
        .iter()
        .enumerate()
        .map(|(index, bytes)| {
            let path = directory.join(format!("wf-native-unit-{index}.o"));
            std::fs::write(&path, bytes).expect("materialize native library object");
            command.arg("-x").arg("none").arg(&path);
            if index == 0
                && let Some(observer) = observer
            {
                command.arg("-x").arg("c").arg(observer);
            }
            path
        })
        .collect();
    (sources, paths)
}

// Each harness uses a different subset of the shared native observations.
#[allow(dead_code)]
mod native;
#[allow(unused_imports)]
pub(crate) use native::{observe_layout, observe_worker_schedule, spine_source, wide_frame_source};

#[allow(dead_code)]
mod process;
#[allow(unused_imports)]
pub(crate) use process::{PROGRAM_DEADLINE, ProgramChild, output_within, run_command};
