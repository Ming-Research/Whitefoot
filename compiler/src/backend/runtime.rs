//! Native runtime source assets used by the build driver.
//! These files are linked implementations, with no source-checking metadata.

/// The C representation of the ordinary prelude library and launcher values.
pub const ORDINARY_VALUES_HEADER: &str = include_str!("ordinary_values.h");
/// Ordinary native function definitions linked through the regular call ABI.
pub const ORDINARY_VALUES_SOURCE: &str = include_str!("ordinary_values.c");
/// Ordinary linked view definitions using the shared Whitefoot callable ABI.
pub const ORDINARY_VALUES_LLVM: &str = include_str!("ordinary_values.ll");

/// The finite completion core contract embedded in the compiler.
pub const COMPLETION_CONTRACT_HEADER: &str = include_str!("completion/contract.h");
/// The typed file-adapter contract embedded in the compiler.
pub const COMPLETION_FILE_ADAPTER_HEADER: &str = include_str!("completion/file_adapter.h");
/// The compiler-owned file-completion bridge contract embedded in the compiler.
pub const COMPLETION_BRIDGE_HEADER: &str = include_str!("completion/bridge.h");
/// The target-guarded Linux io_uring adapter contract embedded in the compiler.
pub const COMPLETION_LINUX_IO_URING_HEADER: &str = include_str!("completion/linux_io_uring.h");
/// The Windows IOCP ring's contract embedded in the compiler.
pub const COMPLETION_WINDOWS_IOCP_HEADER: &str = include_str!("completion/windows_iocp.h");
/// The typed file-adapter's POSIX leaf contract embedded in the compiler.
pub const COMPLETION_FILE_POSIX_HEADER: &str = include_str!("completion/file_posix.h");
/// The address vocabulary every socket engine shares, on every platform.
pub const COMPLETION_SOCKET_ADDRESS_HEADER: &str = include_str!("completion/socket_address.h");
/// The finite completion core implementation embedded in the compiler.
pub const COMPLETION_RUNTIME_SOURCE: &str = include_str!("completion/runtime.c");
/// The host's one wait set, which `runtime.c` and the file adapter sleep on.
pub const COMPLETION_WAIT_HOST_SOURCE: &str = include_str!("completion/wait_host.c");
/// Windows's one wait set, the twin of the above.
pub const COMPLETION_WAIT_WINDOWS_SOURCE: &str = include_str!("completion/wait_windows.c");
/// The typed file-adapter implementation embedded in the compiler.
pub const COMPLETION_FILE_ADAPTER_SOURCE: &str = include_str!("completion/file_adapter.c");
/// The file adapter's POSIX host leaf: one host call per request kind.
pub const COMPLETION_FILE_POSIX_SOURCE: &str = include_str!("completion/file_posix.c");
/// The file adapter's Windows host leaf, the twin of the above.
pub const COMPLETION_FILE_WINDOWS_SOURCE: &str = include_str!("completion/file_windows.c");
/// The compiler-owned file-completion bridge embedded in the compiler.
pub const COMPLETION_BRIDGE_SOURCE: &str = include_str!("completion/bridge.c");
/// The target-guarded Linux io_uring adapter embedded in the compiler.
pub const COMPLETION_LINUX_IO_URING_SOURCE: &str = include_str!("completion/linux_io_uring.c");
/// The target-guarded Windows IOCP ring embedded in the compiler.
pub const COMPLETION_WINDOWS_IOCP_SOURCE: &str = include_str!("completion/windows_iocp.c");

/// The scheduler core's contract embedded in the compiler.
///
/// The ordinary linked library and worker thunks share the native runtime's
/// scheduler entry points; the current-stack core carries no managed stacks.
pub const SCHED_CORE_HEADER: &str = include_str!("sched/core.h");
/// The scheduler core embedded in the compiler.
pub const SCHED_CORE_SOURCE: &str = include_str!("sched/core.c");
/// The seven primitives the core reaches shared state through.
pub const SCHED_PRIM_HEADER: &str = include_str!("sched/prim.h");
/// The host's implementation of those primitives.
pub const SCHED_PRIM_HOST_SOURCE: &str = include_str!("sched/prim_host.c");
/// Windows's implementation of the same set, the twin of the above.
pub const SCHED_PRIM_WINDOWS_SOURCE: &str = include_str!("sched/prim_windows.c");
/// The platform layer over the core: its one instance, the startup policy and
/// the emitted module's `wf__par_*` ABI (design §7's platform layer).
pub const SCHED_ENTRY_HEADER: &str = include_str!("sched/entry.h");
/// That layer's implementation, which replaces `par_runtime.c`.
pub const SCHED_ENTRY_SOURCE: &str = include_str!("sched/entry.c");

/// Windows host primitives used by ordinary linked function definitions.
pub const WINDOWS_RUNTIME_HEADER: &str = include_str!("windows_runtime.h");
/// Windows implementations of those private host primitives.
pub const WINDOWS_RUNTIME_SOURCE: &str = include_str!("windows_runtime.c");

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether `source` calls or declares `name`: the identifier, not part of
    /// a longer one, followed by an opening parenthesis. Comments that use
    /// the word in prose are not calls.
    fn calls(source: &str, name: &str) -> bool {
        source.match_indices(name).any(|(start, _)| {
            let before = source[..start].chars().next_back();
            !before.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
                && source[start + name.len()..].trim_start().starts_with('(')
        })
    }

    /// [STOR-8] the compiler-owned native supplies every build links never
    /// call the program's allocator, so an executable requires one exactly
    /// when its own emitted code does, and a no-heap entry's build requires
    /// none [MOD-9]. The Windows runtime keeps its host descriptor registry
    /// in the process heap, a host resource under [SCOPE-3] that this pins to
    /// that one table.
    #[test]
    fn no_runtime_unit_calls_the_allocator() {
        let units = [
            ("ordinary_values.h", ORDINARY_VALUES_HEADER),
            ("ordinary_values.c", ORDINARY_VALUES_SOURCE),
            ("ordinary_values.ll", ORDINARY_VALUES_LLVM),
            ("completion/contract.h", COMPLETION_CONTRACT_HEADER),
            ("completion/file_adapter.h", COMPLETION_FILE_ADAPTER_HEADER),
            ("completion/bridge.h", COMPLETION_BRIDGE_HEADER),
            (
                "completion/linux_io_uring.h",
                COMPLETION_LINUX_IO_URING_HEADER,
            ),
            ("completion/windows_iocp.h", COMPLETION_WINDOWS_IOCP_HEADER),
            ("completion/file_posix.h", COMPLETION_FILE_POSIX_HEADER),
            (
                "completion/socket_address.h",
                COMPLETION_SOCKET_ADDRESS_HEADER,
            ),
            ("completion/runtime.c", COMPLETION_RUNTIME_SOURCE),
            ("completion/wait_host.c", COMPLETION_WAIT_HOST_SOURCE),
            ("completion/wait_windows.c", COMPLETION_WAIT_WINDOWS_SOURCE),
            ("completion/file_adapter.c", COMPLETION_FILE_ADAPTER_SOURCE),
            ("completion/file_posix.c", COMPLETION_FILE_POSIX_SOURCE),
            ("completion/file_windows.c", COMPLETION_FILE_WINDOWS_SOURCE),
            ("completion/bridge.c", COMPLETION_BRIDGE_SOURCE),
            (
                "completion/linux_io_uring.c",
                COMPLETION_LINUX_IO_URING_SOURCE,
            ),
            ("completion/windows_iocp.c", COMPLETION_WINDOWS_IOCP_SOURCE),
            ("sched/core.h", SCHED_CORE_HEADER),
            ("sched/core.c", SCHED_CORE_SOURCE),
            ("sched/prim.h", SCHED_PRIM_HEADER),
            ("sched/prim_host.c", SCHED_PRIM_HOST_SOURCE),
            ("sched/prim_windows.c", SCHED_PRIM_WINDOWS_SOURCE),
            ("sched/entry.h", SCHED_ENTRY_HEADER),
            ("sched/entry.c", SCHED_ENTRY_SOURCE),
            ("windows_runtime.h", WINDOWS_RUNTIME_HEADER),
            ("windows_runtime.c", WINDOWS_RUNTIME_SOURCE),
            ("wf_floor.c", super::super::emitter::FLOOR_RUNTIME_SOURCE),
            (
                "wf_floor_windows.c",
                super::super::emitter::FLOOR_WINDOWS_RUNTIME_SOURCE,
            ),
        ];
        for (name, source) in units {
            for allocator in [
                "malloc",
                "calloc",
                "realloc",
                "free",
                "aligned_alloc",
                "posix_memalign",
            ] {
                assert!(!calls(source, allocator), "{name} calls {allocator}");
            }
            let host_heap = ["HeapAlloc", "HeapReAlloc", "HeapFree"]
                .into_iter()
                .filter(|host| calls(source, host))
                .count();
            if name == "windows_runtime.c" {
                assert_eq!(
                    source.matches("HeapAlloc(").count() + source.matches("HeapReAlloc(").count(),
                    2,
                    "the descriptor registry is the one growing host-heap table"
                );
            } else {
                assert_eq!(host_heap, 0, "{name} uses the host heap");
            }
        }
        assert!(calls("  p = malloc (n);", "malloc"));
        assert!(calls("call void @free(ptr %p)", "free"));
        assert!(!calls("a lock-free queue", "free"));
    }
}
