# Backend facts

The checked facts the compiler hands LLVM as attributes, flags, metadata and assumptions, and the probes of the host clang's forms.

### decision a-checked-fact-reaches-llvm-only-with-a-complete-mapping
The backend gives LLVM a checked fact as an attribute, flag, metadata or assumption only where kept evidence covers that form's whole target contract; facts never change acceptance.
- rejected: an agent proposed withholding all proved facts from LLVM; the owner rejected it, because DIAG-2 permits those optimization forms where the evidence covers their whole contract.
- rejected: an agent proposed using optimizer assertions as a new source of proof; the owner rejected it, because emitting a fact is optional and must change neither source acceptance nor target qualification.
- rejected: an agent proposed treating every eligible fact family as one implemented set; the owner rejected it, because each needs its own complete mapping from kept evidence to the target, and missing metadata must not become an acceptance failure.
- rejected: an agent proposed inferring `memory(argmem: read)` from an effect row without writes; the owner rejected it, because the row does not account for constant storage or allocation, and no complete mapping of all other memory effects exists.

### decision reference-attributes-follow-each-operation-contract
Reference parameters get `noalias` from EFF-5 disjointness except for `swap`, `dereferenceable` from the target's guaranteed referent size, and only exact arithmetic gets no-wrap flags.
- rejected: an agent proposed the same attributes for every operation; the owner rejected it, because the language layout ceiling is an upper bound, not a guaranteed extent, and wrapping operations admit operands that overflow on purpose.

### decision a-range-reference-crosses-calls-as-pointer-and-count
A contiguous range-reference parameter crosses every call as its element pointer and count, and the pointer gets the reference facts except `dereferenceable`.
- rejected: an agent proposed withholding EFF-5 disjointness from range references; the owner rejected it, because with the fact on the element pointer the host vectorizer drops its runtime overlap check (data: `research/investigations/range-reference-facts/DESIGN.md`).
- rejected: an agent proposed scoped `!alias.scope` and `!noalias` metadata on each access through a range; the owner rejected it, because it needs every access's provenance and scope declarations that survive inlining, while the parameter attribute states the fact once.
- rejected: an agent proposed an `llvm.assume` `separate_storage` bundle between range pointers; the owner rejected it, because it asserts distinct allocations, which two disjoint ranges of one array do not have.
- rejected: an agent proposed a split-parameter body behind a wrapper that takes the aggregate; the owner rejected it, because it doubles every range-taking symbol and relies on inlining for the fact, while the split alone changes no machine calling convention.

### decision a-run-reference-directory-pointer-is-readonly-not-noalias
A run-reference parameter passes its directory pointer at cell + 24, origin and count, and the pointer gets `nonnull`, `readonly` and no-capture but neither `noalias` nor `dereferenceable`.
- rejected: an agent proposed giving it the contiguous range's element-pointer facts; the owner rejected it, because disjoint runs of one `Paged` share one directory, so a promise of exclusive access would be false.

### decision clang-forms-are-probed-when-whitefootc-runs
`whitefootc` probes the clang it hands modules to, once per process, for the no-capture spelling and the result type of `llvm.coro.end`, and falls back to the older form.
- replaced: Probing the forms when the compiler was built was on main; the owner changed it, because a compiler built against one clang runs where clang accepts other forms: a macOS release built where clang took the `void` call emitted it to Apple clang 21, which refuses it.
- rejected: an agent proposed pinning LLVM for these forms; the owner rejected it, because the supported hosts span both forms.
- rejected: an agent proposed inferring support from a vendor's version number; the owner rejected it, because version strings do not tell which syntax that clang accepts.

### decision the-ring-payload-index-is-asserted-nonnegative
At the shared Slots and Ring payload projection, after target qualification, the emitter states through `llvm.assume` that the normalized physical index is nonnegative.
- rejected: an agent proposed withholding the fact until the cost to the retained reverse case is explained; the owner rejected it, because the paired Deque comparison showed a repeatable gain on the inline path (data: `research/experiments/container-representation/deque-library/RESULTS.md`).
- rejected: an agent proposed GEP `nuw` chosen by a probe when the compiler is built; the owner rejected it, because a different supported compiler may consume the module, while the nonnegative-index form gets the observed gain without that dependency.
- rejected: an agent proposed asserting the logical index nonnegative before zero-stride normalization; the owner rejected it, because zero-byte storage can have logical coordinates beyond the signed address range though its real displacement is zero.
- rejected: an agent proposed adding no-wrap flags to the Ring head arithmetic with the address fact; the owner rejected it, because wrapping coordinates have a different arithmetic domain from the final physical element address.

### decision an-enum-tag-switch-defaults-to-unreachable
The default destination of every switch on an enum tag, in a `match` or an enum's release helper, is `unreachable`.
- replaced: A call to `abort` in the default was on main; the owner changed it, because every enum value carries a declared tag, so the abort guards no reachable state, yet costs a compare and branch before every jump-table dispatch.

### decision runtime-storage-roles-get-tbaa
The emitter gives storage-role TBAA to compiler-owned window measures, Paged directory entries, Box owner accesses and explicit scalar source writes, and leaves other accesses untagged.
- rejected: an agent proposed treating results of shared-empty constructors as fresh allocations; the owner rejected it, because an empty constructor returns the shared header, so the result is not fresh.
- rejected: an agent proposed blanket disjointness between owners; the owner rejected it, because the roles promise no exclusive access between owners of the same role.

### decision readonly-formal-header-accesses-get-alias-scopes
In synchronous unsplit functions, loads through a readonly formal whose row reads its whole state get a scope, which header stores through another formal whose row writes its whole state exclude.
- rejected: an agent proposed treating any readonly formal as covering its implicit header reads and unchanged-value rewrites; the owner rejected it, because the investigation found admitted programs where one owner reaches both (data: `research/investigations/in-place-parameters/DESIGN.md`).

### decision a-window-primitive-wrapper-is-expanded-at-a-scoped-caller
A synchronous call whose body is only one window placement primitive and its return is expanded at a caller holding readonly owner scopes, so each header write keeps its real reference root.
- rejected: an agent proposed stronger `noalias` metadata on the call; the owner rejected it, because general call metadata would also exclude reads of the shared empty header that are permitted.
- rejected: an agent proposed choosing such functions by name, or marking them inline; the owner rejected it, because expanding the one primitive keeps the real reference roots on each header write.