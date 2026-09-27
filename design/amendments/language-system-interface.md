Node: language/system-interface

Decision: Host interactions use ordinary values and functions without a source-language exception, and a host function that may suspend its context declares `waits` like any other waiting function, because a special category introduced to repair one resource API would become a permanent second ownership and proof model, instead of raw descriptors, hidden capabilities, suspension classifications derived from where an operation is implemented, or native-specific release and effect rules.

Decision: Two host operations of one context take effect in order exactly when their footprints overlap with a write, and operations with disjoint footprints have no host order even while one has not completed, because host-effect order belongs to the interface through the state its operations share and not to the writer's statement order (the owner's ruling), instead of preserving sequential host order under overlap, which forbids overlapping any two statements that reach the host or needs a hidden global order no footprint states ([design](../../research/investigations/io-model/WAITS.md#host-effects-are-ordered-through-state-not-through-statement-order)).

Decision: Owners responsible for closing a native resource are declared linear and close through explicit consuming functions, while opaque drop is empty, because ordinary must-consume checking exposes cleanup obligations on every exit without a hidden finalizer, instead of release tables that infer native cleanup or resource availability from a type's origin.

Decision: The shipped invocation groups its ordinary inputs in the Inputs struct and hands over no storage provider of any kind, entry selection imposing no special source declaration or call prohibition, because the program has one heap supplied by the trusted base that every function reaches without a parameter and whose allocation and release carry no effect entry, so an invocation parameter has no storage capability left to convey, instead of a separately branded heap parameter, command labels, an unspellable entry brand, or a mandatory source-uncallable main.

Decision: TcpConnection is an ordinary public struct of separately closable direction owners, because well-typed reconstruction may pair halves from different connections and cleanup must remain correct for that pair, instead of a public-field struct with a native-only constructor or a hidden matching-pair release invariant.

Decision: Arguments and paths preserve the target host's bytes with explicit conversion, and range-bearing operations expose their exact window and result relations through ordinary contracts, because a Unicode-only path model cannot represent every host path and an implicit window cannot supply a source proof, instead of string-typed paths or operation-name proof sources.

Rejected:
- Passing a branded heap provider through the invocation and allocating call chains: rejected because the single trusted-base heap has no source-visible store distinction or allocation effect for that parameter to convey.
- Raw syscall numbers and integer descriptors in source: rejected because they expose forgeable identity and unchecked host access outside the ordinary ownership boundary.
- Ambient mutable host access absent from a function's parameters: rejected because it hides inter-function state channels that the declared effect row cannot describe.
- Opaque drop implicitly closing or returning quota: rejected because an empty ordinary drop cannot conceal a host-state write or a must-consume obligation.
- Compiler qualification tables, semantic operation identities, target-specific source guarantees and completion-policy classes: rejected because a linked body must implement its ordinary declaration and origin alone grants no additional acceptance or cleanup rule.
- One permanently retained Process object borrowed by every operation: rejected because it unnecessarily serializes unrelated state; an explicit shared operand represents only the state its operations actually share.
- A literal WASI source contract: rejected because it imports a path, buffering and asynchronous interface designed for cross-language components rather than selecting those obligations for Whitefoot's ordinary values.
