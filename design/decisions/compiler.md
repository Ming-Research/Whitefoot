# Compiler

The compiler: front end, checker, diagnostics, lowering, incremental builds, the command line.

### decision one-safe-rust-crate-with-persistent-results
The compiler is one safe-Rust crate with private interfaces, and it keeps compiler-version-private query results across runs through the same checker.
- replaced: A compiler that kept no checked work between runs, under a rule that refused every artifact format, was on main; the owner changed it, because the modular-compilation design gives retained verification and optimized objects a real consumer, with no stable artifact ABI or proof replay.
- rejected: an agent proposed a product-grade toolchain with stable crate boundaries, serialized checked artifacts and replay; the owner rejected it, because it multiplied crates and gates before a resolver or backend existed and imposed protocols that had no consumer.
- rejected: an agent proposed a separate incremental checker beside the clean checker; the owner rejected it, because cache state may change which computation is reused, never which rules decide acceptance.
- rejected: an agent proposed hardening the front end against hostile input and checking a previous stage's result again in the trusted path; the owner rejected it, because this compiler exists to iterate the language and will be replaced by a self-hosted one, so defense in depth here buys nothing.

### decision acceptance-follows-numbered-rules
Acceptance and proof discharge follow only the specification's numbered rules, through one semantic path; each names its rule and, on failure, the first missing fact.
- rejected: an agent proposed special cases that match the shape of a function, a project or the corpus in hand; the owner rejected it, because a capability made by recognizing a particular program is a special case the language does not have.
- rejected: an agent proposed letting conformance cases, staging plans or profiles select acceptance; the owner rejected it, because it would let tests and corpora define the language in place of the specification.
- replaced: Keeping superseded system-inventory states behind compile-time switches for differential tests was on main; the owner changed it, because the compiler implements one specification, the active one, so such a switch has no consumer and only adds code paths.

### decision unimplemented-source-stops-as-unsupported
Valid source that the compiler has not implemented stops as an explicit unsupported capability and is never reported as invalid source.
- rejected: an agent proposed reporting valid source the compiler has not implemented as invalid source; the owner rejected it, because a compiler gap reported as a language rejection rewrites the language from the implementation side.

### decision rejections-are-deterministic-and-cite-rules
A rejection names its numbered rule and location, one compiler rejects the same source the same way every time, and no acceptance depends on time, budget or hash order.
- rejected: an agent proposed a portable order for the first violation found after name resolution; the owner rejected it, because two correct semantic checkers may meet different first violations.
- rejected: an agent proposed checking under a timeout, fuel or work budget; the owner rejected it, because acceptance that depends on time, budget or iteration order is acceptance by accident.

### decision ownership-and-permission-proofs-share-one-ledger
The range separations [OWN-7] asks for and a counted loop's range sign goals [PAR-2] are proved in the flow ProofContext and kept as roots of its one ledger.
- rejected: an agent proposed a second range solver in the structural checker; the owner rejected it, because structural loan formation comes before the arithmetic facts it needs, and ownership and overlap must use the same captured endpoint values.
- rejected: an agent proposed a target-side overlap test; the owner rejected it, because ownership and overlap must use the same captured endpoint values.
- rejected: an agent proposed a record of retained results private to the parallel permission; the owner rejected it, because one ledger is what lets the permission read a proof the ownership check has already discharged.

### decision linked-bodies-share-the-callable-abi
A WF body and a linked body with the same ordinary signature use one callable ABI and the same permission path; a native engine stays inside the call.
- replaced: A hand-checked native qualification version that gated source acceptance was on main; the owner changed it, because ordinary declarations and link-selected implementations have no separate system domain to qualify.
- replaced: A source staged-loop permission chosen by a native operation or suspension classification was on main; the owner changed it, because ordinary call and loan boundaries must apply equally to source and linked bodies.
- rejected: an agent proposed special emission for system calls; the owner rejected it, because choosing a native implementation cannot change source acceptance, effect overlap or reference validity [REF-2].
- rejected: an agent proposed ending source references early at a native call; the owner rejected it, because choosing a native implementation cannot change source acceptance, effect overlap or reference validity [REF-2].

### decision runtime-hashing-uses-maintained-sha256
The compiler's cache digests use the maintained `sha2` crate through its safe API, with locked dependencies provided before offline checks.
- rejected: an agent proposed keeping platform-specific hash intrinsics in the compiler; the owner rejected it, because the maintained safe implementation already gave lower entry-edit compile time with the same digest bytes (data: `research/experiments/modular-build-cost/RESULTS.md`).
- rejected: an agent proposed weakening complete-key and checksum validation; the owner rejected it, because the cost was scalar hashing of large retained records, which the maintained implementation cut with the same digests (data: `research/experiments/modular-build-cost/RESULTS.md`).
