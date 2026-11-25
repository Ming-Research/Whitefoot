# System interface

How a program reaches its host: the host modules' values and functions, the invocation's inputs, host order, resource cleanup, host bytes and error text.

### decision host-access-is-ordinary-values-and-functions
Host interactions use ordinary values and functions with no source-language exception, and a host function that may suspend its context declares `waits` like any waiting function.
- rejected: an agent proposed raw syscall numbers and integer descriptors in source; the owner rejected it, because they expose forgeable identity and unchecked host access outside the ordinary ownership boundary.
- rejected: an agent proposed ambient mutable host access absent from a function's parameters; the owner rejected it, because it hides channels of state between functions that the declared effect row cannot describe.
- rejected: an agent proposed one permanently retained Process object borrowed by every operation; the owner rejected it, because it serializes unrelated state, while an explicit shared operand stands for only the state its operations share.
- rejected: an agent proposed a literal WASI source contract; the owner rejected it, because it imports a path, buffering and asynchronous interface designed for components across languages, instead of choosing those obligations for Whitefoot's values.
- replaced: A set of compiler qualification tables, semantic operation identities, target-specific source guarantees and completion-policy classes was on main; the owner changed it, because a linked body must implement its ordinary declaration, and its origin grants no extra rule.
- rejected: an agent proposed classing a host function's suspension by where the operation is implemented; the owner rejected it, because a special category made to repair one resource interface would become a permanent second ownership and proof model.

### decision generic-host-functions-follow-generic-rules
Host functions may declare type parameters under the ordinary generic rules; the build supplies one shared definition, or specialized ones where concrete layouts are needed.
- rejected: an agent proposed that every instance of a generic host function share one implementation; the owner rejected it, because releasing a map reserve needs no traversal of the value type, while a typed encoder needs its concrete owned layout.
- rejected: an agent proposed another source function kind for generic host functions; the owner rejected it, because the ordinary generic rules already cover both a release that needs no layout and an encoder that needs a concrete one.

### decision host-order-follows-overlapping-footprints
Two host operations of one context take effect in order exactly when their footprints overlap with a write; operations with disjoint footprints have no host order.
- rejected: an agent proposed keeping sequential host order under overlap; the owner rejected it, because host order belongs to the interface through the state its operations share, and statement order would forbid overlapping any two host statements or need a hidden global order.

### decision host-resource-owners-close-explicitly
An owner that must close a native resource is declared linear and closes through an explicit consuming function, while the drop of a fieldless opaque struct is empty.
- rejected: an agent proposed release tables that infer native cleanup or resource availability from a type's origin; the owner rejected it, because ordinary must-consume checking shows cleanup obligations on every exit without a hidden finalizer.
- rejected: an agent proposed that the drop of a fieldless opaque struct implicitly close a resource or return quota; the owner rejected it, because an empty ordinary drop cannot hide a write of host state or a must-consume obligation.

### decision invocation-inputs-carry-no-storage-provider
The invocation groups its ordinary inputs in the Inputs struct and hands over no storage provider; selecting the entry needs no special source declaration or call prohibition.
- replaced: A separately branded heap passed through the invocation and allocating call chains was on main; the owner changed it, because the program has one heap that every function reaches without a parameter, so the parameter has no storage capability to convey.
- replaced: An entry `command fn main` that requests labelled command inputs and cannot be called from source was on main; the owner changed it, because ordinary parameters already express the other invocation values.
- rejected: an agent proposed an unspellable entry brand; the owner rejected it, because ordinary parameters already express the invocation values, so the entry needs no special source declaration.

### decision invocation-gives-driver-count-field
The invocation gives the runtime's driver count as the ordinary public `u64` field `drivers` of Inputs.
- rejected: an agent proposed a host function that returns the driver count; the owner rejected it, because a count fixed for the run is an execution input passed as an ordinary value, and a function that returns one with no capability parameter breaks the rule that inputs come through the entry.
- rejected: an agent proposed a field reporting the requested configuration before runtime limits and host availability apply; the owner rejected it, because a program needs the number of drivers the runtime actually starts.
- rejected: an agent proposed general interfaces to read environment variables and the CPU count, so a program computes the count itself; the owner rejected it, because each program would copy the runtime's rules and fall out of step when they change.

### decision invocation-gives-optional-process-id
The invocation gives the host's identifier of the program's process as the field `process_id: Option<u64>` of Inputs, `None` on a host with no process identifier.
- rejected: an agent proposed a nullary pure function returning the process identifier; the owner rejected it, because it would hand every function a value that depends on the execution through no parameter.
- rejected: an agent proposed reading the process identifier through the memory meter or the stop-signal capability; the owner rejected it, because those observe other things.
- rejected: an agent proposed a bare `u64` whose zero stands for a missing identifier; the owner rejected it, because a target without processes must stay a valid target with a total answer.

### decision tcp-connection-is-two-closable-halves
TcpConnection is an ordinary public struct of two direction owners that close separately.
- rejected: an agent proposed a struct with public fields and a constructor only native code may call, or a hidden release rule that the halves match; the owner rejected it, because well-typed code may pair halves of different connections, and cleanup must stay correct for that pair.

### decision host-bytes-kept-with-explicit-conversion
Arguments and paths keep the target host's bytes and convert explicitly, and an operation over a range states its exact window and result relations in ordinary contracts.
- rejected: an agent proposed paths typed as strings; the owner rejected it, because a path model of Unicode only cannot represent every host path.
- rejected: an agent proposed proof sources keyed to operation names; the owner rejected it, because an implicit window cannot supply a source proof.

### decision directory-names-are-same-bytes-everywhere
A name below a directory, given or listed, is the host's bytes on POSIX and the WTF-8 encoding of its UTF-16 code units on Windows, and `host_copy_name_bytes` copies a host string to name bytes.
- rejected: an agent proposed names in the host's native code units, UTF-16 on Windows; the owner rejected it, because a portable program must then convert a name before every comparison and print, and one that forgets fails only on Windows, as the directory walk in `tests/programs/wfgrep.wf` did.
- rejected: an agent proposed a name encoding chosen on each call, native code units or UTF-8; the owner rejected it, because UTF-8 has no good outcome for a name that is not text: failing the batch hides the other entries, and marking the record gives every record two forms.
- rejected: an agent proposed UTF-8 only, leaving out or refusing names that are not text; the owner rejected it, because a POSIX name that is not UTF-8 and a Windows name with an unpaired surrogate could then be neither listed nor opened.

### decision io-error-description-copied-into-window
`error_copy_description` copies an IoError's whole host description, in C-locale or US-English text, into a caller's window, or returns the length needed when the window is too small.
- rejected: an agent proposed allocating a string the writer sees for the description; the owner rejected it, because the caller keeps ownership of the storage when the text goes into a window it supplies.
- rejected: an agent proposed truncating the text to the window; the owner rejected it, because diagnostics need the host's whole explanation, so a short window returns the length needed and writes nothing.
- rejected: an agent proposed changing the process locale to choose the text; the owner rejected it, because the ambient locale is hidden shared state and must not select the bytes.
- rejected: an agent proposed fixed Whitefoot texts for each portable error class; the owner rejected it, because they cannot match the C library byte for byte, as Firn needs, and cannot tell apart errors of one class.
- rejected: an agent proposed an explicit locale object the caller supplies; the owner rejected it, because its creation, lifetime and failure need rules, and no program needs localized text, Redis itself using the C locale.

### decision io-error-class-description-by-variant
`error_copy_class_description` gives the host's text for the one host code the specification's table gives an IoError's variant on that host, and `DescriptionUnavailable` for a class with none.
- rejected: an agent proposed a function giving each class's host code, for the program to build an IoError from; the owner rejected it, because host codes would then enter the source, and a program could compare platform numbers.
- rejected: an agent proposed answering a class with no host code with the fixed portable text; the owner rejected it, because the caller could then not tell host text from the specification's table.