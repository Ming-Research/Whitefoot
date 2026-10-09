//! Native runtime source assets used by the build driver.
//! These files are linked implementations, with no source-checking metadata.

/// The C representation of the ordinary prelude library and launcher values.
pub const ORDINARY_VALUES_HEADER: &str = include_str!("ordinary_values.h");
/// Ordinary native function definitions linked through the regular call ABI.
pub const ORDINARY_VALUES_SOURCE: &str = include_str!("ordinary_values.c");
/// Ordinary linked view definitions using the shared Whitefoot callable ABI.
pub const ORDINARY_VALUES_LLVM: &str = include_str!("ordinary_values.ll");
/// Counted libc allocation wrappers, linked only for emitted heap references.
pub const HEAP_SOURCE: &str = include_str!("heap.c");

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
/// The invocation stop listener and host observers.
pub const COMPLETION_STOP_SIGNALS_SOURCE: &str = include_str!("completion/stop_signals.c");
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
/// Select the demand experiment's two C units. Callers use this only when
/// the emitted module defines `wf__par_demand_mode`; the default runtime
/// preprocesses out all demand state and hot-path instructions.
pub fn demand_runtime_source(path: &str, ordinary: &'static str) -> &'static str {
    match path {
        "sched/core.c" => concat!("#define WF_PAR_DEMAND 1\n", include_str!("sched/core.c")),
        "sched/entry.c" => concat!("#define WF_PAR_DEMAND 1\n", include_str!("sched/entry.c")),
        _ => ordinary,
    }
}

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

/// The runtime's concurrent map: its interface, its implementation, and the
/// unit that compiles it in as keyed tables and key sets over the completion
/// runtime [SHARE-1].
pub const CONCURRENT_MAP_HEADER: &str = include_str!("concurrent_map.h");
/// The concurrent map's implementation, which `keyed_table.c` includes.
pub const CONCURRENT_MAP_SOURCE: &str = include_str!("concurrent_map.c");
/// `keyed_table.c`, the emitted module's `wf__keyed_table_*`, `wf__table_*`,
/// `wf__key_set_*` and `wf__watch_table` ABI (`completion/bridge.h`).
pub const KEYED_TABLE_SOURCE: &str = include_str!("keyed_table.c");

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

    fn without_heap_entry_points(source: &str) -> String {
        let mut remaining = source.to_owned();
        for (signature, allocator) in [
            ("void *wf__heap_take(uint64_t bytes) {", "malloc"),
            (
                "void *wf__heap_retake(void *block, uint64_t old_bytes, uint64_t new_bytes) {",
                "realloc",
            ),
            ("void wf__heap_give(void *block, uint64_t bytes) {", "free"),
        ] {
            let start = remaining.find(signature).expect("counted heap entry point");
            let end = start + remaining[start..].find("\n}\n").expect("function end") + 3;
            let body = &remaining[start..end];
            assert_eq!(body.matches(&format!("{allocator}(")).count(), 1);
            for forbidden in [
                "malloc",
                "calloc",
                "realloc",
                "free",
                "aligned_alloc",
                "posix_memalign",
            ] {
                if forbidden != allocator {
                    assert!(!calls(body, forbidden), "{signature} calls {forbidden}");
                }
            }
            remaining.replace_range(start..end, "");
        }
        remaining
    }

    /// Every unconditional unit must remain allocator-free [STOR-8, MOD-9].
    /// Only the optional heap unit may call libc's allocator; Windows keeps
    /// its descriptor registry's separate host-heap API.
    #[test]
    fn runtime_allocator_calls_are_confined_to_counted_entry_points() {
        let units = [
            ("heap.c", HEAP_SOURCE),
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
            ("completion/stop_signals.c", COMPLETION_STOP_SIGNALS_SOURCE),
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
            ("concurrent_map.h", CONCURRENT_MAP_HEADER),
            ("concurrent_map.c", CONCURRENT_MAP_SOURCE),
            ("keyed_table.c", KEYED_TABLE_SOURCE),
            ("wf_floor.c", super::super::emitter::FLOOR_RUNTIME_SOURCE),
            (
                "wf_floor_windows.c",
                super::super::emitter::FLOOR_WINDOWS_RUNTIME_SOURCE,
            ),
        ];
        for (name, source) in units {
            let remaining;
            let source = if name == "heap.c" {
                remaining = without_heap_entry_points(source);
                remaining.as_str()
            } else {
                source
            };
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
        // A forbidden call beside either allowed body remains visible.
        let injected = format!("{HEAP_SOURCE}\nvoid bad(void) {{ free(0); }}");
        assert!(calls(&without_heap_entry_points(&injected), "free"));
        assert!(calls("  p = malloc (n);", "malloc"));
        assert!(calls("call void @free(ptr %p)", "free"));
        assert!(!calls("a lock-free queue", "free"));
    }
}
