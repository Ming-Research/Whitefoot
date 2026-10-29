#![forbid(unsafe_code)]

mod canonical_corpus;
mod checker_work;
mod doc_examples;
#[cfg(windows)]
mod native_windows;
mod support;

#[cfg_attr(windows, allow(dead_code))]
mod conformance {
    #[cfg(unix)]
    mod adapter;
    pub(super) mod corpus;
    mod file_replacement;
    mod json;
}

mod programs {
    #[cfg(unix)]
    mod binary;
    mod containers;
    mod driver_count;
    #[cfg(unix)]
    mod contexts;
    mod file_replacement;
    mod stop_signals;
    #[cfg(unix)]
    mod hashing;
    #[cfg(unix)]
    mod heap;
    mod host_paths;
    #[cfg(unix)]
    mod image;
    // The expected offsets name IANA zones, which POSIX `TZ` accepts and the
    // Windows C runtime's `TZ` format does not.
    #[cfg(unix)]
    mod local_time;
    mod memory;
    // POSIX `RLIMIT_NOFILE`, set through the shell's `ulimit`.
    #[cfg(unix)]
    mod open_files;
    mod open_read_paths;
    #[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
    mod snapshot_export;
    mod network;
    #[cfg(unix)]
    mod numerics;
    #[cfg(unix)]
    mod ordinary_io;
    #[cfg(unix)]
    mod package_fixtures;
    #[cfg(unix)]
    mod parallel;
    #[cfg(unix)]
    mod raw_deflate;
    #[cfg(unix)]
    mod raw_deflate_vectors;
    #[cfg(unix)]
    mod runs;
    #[cfg(unix)]
    mod signal;
    #[cfg(unix)]
    mod stream;
    #[cfg_attr(windows, allow(dead_code))]
    pub(crate) mod support;
    #[cfg(unix)]
    mod text;
    #[cfg(windows)]
    mod windows;
    // These traversal fixtures require POSIX byte names and mode-bit denial.
    // Windows has its own production namespace fixture in the native group.
    #[cfg(unix)]
    mod traversal;
    #[cfg(unix)]
    mod wfgrep;
    mod wfgrep_hosts;
    #[cfg(unix)]
    mod wide_scan;
}
