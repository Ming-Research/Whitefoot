# Exhaustion floor

What happens when a program runs out of heap, stack or descriptors: the record it writes, stack probing and the stack ledger.

### decision exhaustion-writes-one-fixed-record
Heap and stack exhaustion write one fixed record that names only the exhausted resource class, and an unavailable compute worker or task slot is a refused offer.
- rejected: an agent proposed a language-level trap, or a record with fields a reader could take for a proof failure; the owner rejected it, because a record with no source construct, rule, function or address is what tells exhaustion apart from a language trap record.
- replaced: A compiler decision that heap exhaustion ends the process by a defined abort was on main; the owner changed it, because [STOR-8] now says in the language that allocation is total and that exhaustion ends the program from the trusted base.

### decision generated-functions-carry-stack-probes
The entry runs on a stack the runtime sizes, every generated function carries the backend's stack-probing attribute, and a fault below a stack keeps the host's own handling.
- rejected: an agent proposed comparing the stack pointer with a per-thread limit in every generated prologue; the owner rejected it, because the check spends the headroom it guards, while native probing stops frames that cross the guard without a check in each function.

### decision runtime-c-frames-keep-their-default-probing
Stack probing covers the functions the compiler generates; the runtime's C files keep their compiler's default, so upstream clang leaves a large runtime frame unprobed.
- rejected: an agent proposed claiming that the runtime's C frames are contained, from the qualification of generated code; the owner rejected it, because that qualification compiled and ran only generated functions.
- rejected: an agent proposed probing the runtime's C on macOS now, with a compiler flag; the owner rejected it, because it covers one platform only, and covering every platform together is a separate piece of work.
- rejected: an agent proposed probing the runtime's C on every platform, with a gate check, before the merge; the owner rejected it, because it needs the flags tested on both platforms, an audit of frame sizes and a new check, and is its own work.

### decision darwin-probe-method-follows-the-clang
On Darwin a probe compiles a large frame with `__chkstk_darwin`; if clang accepts it, that helper is used, and otherwise inline probing every 4096 bytes.
- rejected: an agent proposed choosing the method from the vendor string or version; the owner rejected it, because those do not show whether the clang in use can lower the helper, while compiling a large frame tests that directly.
- rejected: an agent proposed `__chkstk_darwin` for every Darwin compiler; the owner rejected it, because the AArch64 backend of upstream LLVM 22.1.8 refuses that method.
- rejected: an agent proposed inline probing on every Apple compiler now; the owner rejected it, because Apple's helper passed the comparison, while Apple's inline method has not been checked across its supported range.
- rejected: an agent proposed turning stack probing off to compile with upstream LLVM; the owner rejected it, because an unprobed frame can cross the guard region and corrupt another mapping.
- rejected: an agent proposed a probe helper written for Whitefoot; the owner rejected it, because upstream AArch64 refuses helper methods before linking, and a custom helper would add unchecked register, unwind and deployment contracts.

### decision stack-ledger-reads-the-host-compilation
The stack ledger reads frame sizes with `-fstack-usage` and the call graph from the assembly of one host compilation, plus a fixed return-address correction per architecture.
- rejected: an agent proposed a frame estimate built up during target qualification; the owner rejected it, because only the finished compilation has the numbers: a non-leaf's real cost is its ABI frame record and the spills the register allocator chose.
