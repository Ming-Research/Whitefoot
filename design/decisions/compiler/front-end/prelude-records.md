# Prelude records

How the compiler reads, checks and emits the prelude's compiler-owned declarations and the linked declarations a build supplies.

### decision prelude-records-use-the-generated-grammar
Bodyless prelude function records are read with the generated signature grammar, and opaque prelude types resolve to their compiler-owned storage identities.
- rejected: an agent proposed a handwritten parallel grammar, synthetic empty bodies or a second nominal registration for prelude records; the owner rejected it, because one declaration grammar keeps one diagnostic order, while TYPE-9 owns the layout and measures a field list cannot express.

### decision prelude-operations-carry-compiler-metadata
The closed PRE-1 operations carry compiler metadata: which operand supplies each window parameter, which operations allocate and which end element-reference bounds.
- rejected: an agent proposed markers a writer could see in source; the owner rejected it, because these selections range over the closed prelude declarations, which no source declaration can imitate.
- rejected: an agent proposed inferring every property of an operation from its read and write rows; the owner rejected it, because rows say nothing about allocation or about which element-reference bounds an operation ends.

### decision prelude-operations-emit-shared-bodies
Each compiler-owned PRE-1 operation is emitted as one shared out-of-line body per materialized instance, and a missing implementation stops as unsupported.
- rejected: an agent proposed expanding the operations inline at each call after checking; the owner rejected it, because shared bodies avoid repeating generic window arithmetic at every call, and no evidence showed the gain of specializing outweighs that.
- rejected: an agent proposed treating inline expansion as a second semantic checker; the owner rejected it, because checked calls already carry the source obligations.

### decision window-edits-walk-logical-indices
Window insertion, removal, append and splitting share one logical-index walk across Slots and Ring, and releasing a proved-empty window frees only its storage.
- rejected: an agent proposed a memmove fast path for Slots only; the owner rejected it, because one coordinate walk handles wrapped rings without a second boundary algorithm, and no measured need was shown.
- replaced: An implicit release at scope end of an empty linear window was on main; the owner changed it, because an empty linear window has no element the compiler is allowed to drop.

### decision generic-linked-instances-share-or-specialize
Each instance of a generic linked declaration keeps its own checked signature; it shares one link symbol, or gets a supplied specialized body where it needs layout data.
- replaced: One link symbol shared by every instance of a generic host function was on main; the owner changed it, because an encoder must know each concrete owned layout, while ordinary calls keep their declared boundary.
- rejected: an agent proposed a symbol per instance with no supplied definition; the owner rejected it, because the host would then have to provide one for every value type.
- rejected: an agent proposed erasing the instances before semantic checking; the owner rejected it, because ordinary generics still need distinct source types and bounds.

### decision generic-linked-boundaries-admit-fixed-aggregates
A generic linked declaration admits an owned aggregate argument or result whose whole layout is fixed at its symbolic boundary.
- replaced: Refusing every generic host boundary whose machine form could depend on a type argument was on main; the owner changed it, because a fixed aggregate such as Reserve or Result<Job, Error> has the same ordinary ABI as a source function's aggregate.
- rejected: an agent proposed a checking exception for the snapshot export only; the owner rejected it, because a fixed aggregate already has the same ordinary ABI as a source function's aggregate.
