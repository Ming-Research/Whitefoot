//! Conservative textual LLVM emission for the active Whitefoot specification.

pub(crate) mod abi;
pub(crate) mod emission;
pub(crate) mod emitter;
mod fragments;
mod runtime;
mod stack_ledger;
mod storage;

#[cfg(test)]
mod tests;

#[cfg(test)]
pub use emitter::emit_llvm;
pub use emitter::{
    BackendFailure, COMPLETION_BRIDGE_HEADER, COMPLETION_BRIDGE_SOURCE, COMPLETION_CONTRACT_HEADER,
    COMPLETION_FILE_ADAPTER_HEADER, COMPLETION_FILE_ADAPTER_SOURCE, COMPLETION_FILE_POSIX_HEADER,
    COMPLETION_FILE_POSIX_SOURCE, COMPLETION_FILE_WINDOWS_SOURCE, COMPLETION_LINUX_IO_URING_HEADER,
    COMPLETION_LINUX_IO_URING_SOURCE, COMPLETION_RUNTIME_SOURCE, COMPLETION_SOCKET_ADDRESS_HEADER,
    COMPLETION_STOP_SIGNALS_SOURCE, COMPLETION_WAIT_HOST_SOURCE, COMPLETION_WAIT_WINDOWS_SOURCE, COMPLETION_WINDOWS_IOCP_HEADER,
    COMPLETION_WINDOWS_IOCP_SOURCE, CONCURRENT_MAP_HEADER, CONCURRENT_MAP_SOURCE,
    DISPATCH_LEDGER_PREFIX, FLOOR_RUNTIME_SOURCE, FLOOR_STACK_BYTES, FLOOR_WINDOWS_RUNTIME_SOURCE,
    KEYED_TABLE_SOURCE, LlvmModule, ORDINARY_VALUES_HEADER, ORDINARY_VALUES_LLVM,
    ORDINARY_VALUES_SOURCE, SCHED_CORE_HEADER, SCHED_CORE_SOURCE, SCHED_ENTRY_HEADER,
    SCHED_ENTRY_SOURCE, SCHED_PRIM_HEADER, SCHED_PRIM_HOST_SOURCE, SCHED_PRIM_WINDOWS_SOURCE,
    WINDOWS_RUNTIME_HEADER, WINDOWS_RUNTIME_SOURCE, module_requires_parallel_runtime,
};
pub use fragments::{FragmentGranularity, SplitFailure, split_module};
pub use stack_ledger::{Architecture, stack_ledger};

/// Byte offset of a shared object's state from its runtime header; equals
/// `WF_SHARED_STATE_OFFSET` in `completion/bridge.h`.
pub(crate) const SHARED_STATE_OFFSET: u64 = 64;

/// Bytes of the record a lock of one table entry keeps in the frame; equals
/// `WF_TABLE_ENTRY_SIZE` in `completion/bridge.h`
/// (compiler/waiting-contexts/state-locks).
pub(crate) const TABLE_ENTRY_SIZE: u64 = 40;

/// Bytes of the record a hold of a table's entries keeps in the frame;
/// equals `WF_TABLE_HOLD_SIZE` in `completion/bridge.h`.
pub(crate) const TABLE_HOLD_SIZE: u64 = 304;

/// Bytes of the record a guard's watch keeps in the frame; equals
/// `WF_WATCH_SIZE` in `completion/bridge.h`.
pub(crate) const WATCH_SIZE: u64 = 152;
