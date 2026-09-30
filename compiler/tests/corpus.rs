#![forbid(unsafe_code)]

mod canonical_corpus;
#[cfg(windows)]
mod native_windows;
mod support;

#[cfg_attr(windows, allow(dead_code))]
mod conformance {
    #[cfg(unix)]
    mod adapter;
    pub(super) mod corpus;
    mod json;
}

mod programs {
    #[cfg(unix)]
    mod binary;
    mod containers;
    #[cfg(unix)]
    mod contexts;
    #[cfg(unix)]
    mod hashing;
    #[cfg(unix)]
    mod heap;
    #[cfg(unix)]
    mod image;
    mod network;
    #[cfg(unix)]
    mod numerics;
    #[cfg(unix)]
    mod ordinary_io;
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
    #[cfg(unix)]
    mod wide_scan;
}
