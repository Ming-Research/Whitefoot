# Storage placement

Where the lowering places values in memory: which results reuse argument storage, how frame roots are allocated, when a by-value input is read in place, and how small values are moved.

### decision a-call-result-may-reuse-a-copied-argument-no-longer-read
A call's only consumed or copied owned argument of a type may share storage with a same-typed result, or one consuming field of a struct result, when liveness and interference checks allow.
- replaced: Reuse only for consumed inputs was on main; the owner changed it, because the callee gets a copy exactly as a consumed input, and Halo's interpreter kept a copied step result in its own slot and copied it on its hot path.
- rejected: an agent proposed promising a stable address in the language; the owner rejected it, because STOR-7 permits relocation without observable address identity.
- rejected: an agent proposed letting one consumed child reuse an undersized or misaligned parent result allocation; the owner rejected it, because the complete result still has to fit, even when the returned child is smaller than its parent.

### decision every-ordinary-frame-root-gets-its-own-alloca
Each complete positive-sized storage group whose alignment is its natural one gets its own entry alloca, mixed alignments included, under a checked bound on the total extent.
- rejected: an agent proposed separate allocas only for roots of one uniform alignment; the owner rejected it, because the mixed-alignment witness kept a dead snapshot that separate allocations let the optimizer remove (data: `research/investigations/in-place-parameters/DESIGN.md`).
- rejected: an agent proposed alignment groups; the owner rejected it, because roots in each group would still share allocation provenance, and the owner chose separate allocations to remove that coupling of unrelated roots.

### decision a-window-take-updates-its-descriptor-before-the-transfer
A compiler-owned window take captures the old element address, updates the descriptor and then moves the element, as one lowering operation.
- rejected: an agent proposed keeping an element snapshot across a separate descriptor update; the owner rejected it, because descriptor and element bytes are disjoint, and storing the descriptor before the transfer lets the aggregate copy be forwarded.

### decision a-destination-result-definition-reads-by-value-inputs-in-place
A definition reads an immutable by-value aggregate parameter through its caller's pointer when its storage is proved stable, and takes a private copy only before a possible change.
- replaced: Copying every by-value aggregate parameter at entry was on main; the owner changed it, because Halo's `push_frame` copied 80 bytes on every Lua call, and reading in place made fib 3.8% faster with no other kernel slower beyond noise.
- replaced: Excluding every definition whose result uses a destination was on main; the owner changed it, because Halo's `table_get` kept a 16-byte key copy that reading before the result write removes (data: `research/investigations/in-place-parameters/DESIGN.md`).

### decision unobserved-join-parameters-are-removed-before-storage-planning
Before storage planning, internal block parameters that nothing reads, directly or through forwarding, are removed with their incoming arguments.
- rejected: an agent proposed weakening interference between live parameters; the owner rejected it, because a continuing match carries every binding, and only an unobserved duplicate destination stands in the way of result-slot coalescing.

### decision a-by-value-snapshot-is-read-through-its-source
An eligible use of a by-value Load snapshot reads through its source address when checked facts prove the source unchanged, and other uses get a private copy just before them.
- replaced: Copying every by-value snapshot at the load was on main; the owner changed it, because the hot tag-reading path must not inherit a cold consumer's copy (data: `research/investigations/in-place-parameters/DESIGN.md`).
- rejected: an agent proposed requiring writers to borrow-match instead; the owner rejected it, because the by-value snapshot is a natural source form whose redundant transfer is a compiler gap, and changing the spelling would hide it.
- rejected: an agent proposed making indexed by-value reads borrows in the language; the owner rejected it, because a snapshot must keep the value seen at the read even when its source later changes, and a borrow means something else.

### decision a-looping-callee-may-read-an-own-parameter-in-place
An immutable indirect own parameter of an ordinary looping callee counts as a read-through consumer when the usual checks hold, except in a callee whose dispatch may be split.
- replaced: Excluding every callee with a loop was on main; the owner changed it, because Halo's `node_find` already reads its key in place, but its caller copied the key only because the consumer check excluded every loop.
- rejected: an agent proposed changing transfer widths to hide the copy; the owner rejected it, because the copy is itself unnecessary.

### decision small-values-move-by-layout-granules-from-every-source
Stored structs and enums of at most 128 bytes and 16 layout granules are moved field by field from every source, with every source granule read before any destination store.
- rejected: an agent proposed granule moves only for values the same function stored field by field; the owner rejected it, because Halo then reloaded separately stored caller fields with wide loads, concat 1.174 and string-key 1.300 (data: `research/investigations/in-place-parameters/DESIGN.md`).
- rejected: an agent proposed a Whitefoot always-inline policy bounded by IR units to offset the moves' inline cost; the owner rejected it, because it marked 44% of Halo's functions, grew its text by 25.7%, and still missed the function it targeted.