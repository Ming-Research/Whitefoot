# Data model

How a program holds its data: the layout of bulk data, what a type's identity carries, the heap cell, allocation, full arrays and sparse maps.

### decision bulk-data-is-taught-as-struct-of-arrays
Bulk data is taught as struct-of-arrays, one window per field, and a storage shape may also hold a struct by value as its element, for record-by-record use.
- rejected: an agent proposed forbidding array-of-structs; the owner rejected it, because record-by-record use is a real need, and owned struct elements already serve it without a copy-struct mechanism the language does not have.
- rejected: an agent proposed a separate flat heap-buffer type to hold records; the owner rejected it, because a storage shape whose element is the struct, held by value under ordinary ownership, already serves the record-by-record case.

### decision a-signature-promise-ignores-representation
A function signature promises its parameters' ownership classes and its declared effect row, and neither depends on how a parameter or a result is represented.
- rejected: an agent proposed deriving a parameter's access permission from how it is represented; the owner rejected it, because a choice of representation must not widen or narrow what a signature promises.

### decision types-carry-no-region-or-store
A type's identity has no region, store or brand part, and no type names the store its storage came from.
- replaced: Carrying region parameters, store brands and providers in types, regions as part of a nominal's identity, was on main; the owner changed it, because one heap serves every allocation and a reference never outlives the function that formed it, so a store name in a type protects nothing.
- rejected: an agent proposed letting aggregates hold reference or slice values; the owner rejected it, because a value that holds an interior pointer can no longer be moved by copying its bytes.

### decision bool-is-a-prelude-enum
`Bool` is a two-variant prelude enum, and the primitive types are `unit` and the integers and floats of 8, 16, 32 and 64 bits.
- rejected: an agent proposed a dedicated primitive boolean; the owner rejected it, because as an enum, one mechanism serves selection, equality and tag-width lowering for `Bool` and user enums alike.

### decision box-is-the-one-value-heap-cell
`Box<T>` is the heap cell for one value; it carries no measure and no brand, and it is the only home of a runtime-capacity shape, a `Segments` or a `Paged`.
- rejected: an agent proposed representing a boxed value as a window of capacity one; the owner rejected it, because such a window costs three descriptor words against a cell's one pointer, and each read would owe a proof that a container known never to be empty is not empty.

### decision box-content-is-the-field-inner
A cell's content is its ordinary field `inner`, read as `b.inner`, reached through a reference as `cell^.inner`, and unboxed only by `let n = move b.inner;`.
- replaced: Reaching the cell's content with the dereference form `deref(b)` was on main; the owner changed it, because one spelling then named both a value the program owns and storage it does not, and `set deref(b) = v;` could mean rebinding a name or writing the content.
- rejected: an agent proposed naming the content field `value`, `item` or `data`; the owner rejected it, because none of them says anything about the cell, and `value` already names the payload of `Option` and `Result`, so one word would name two steps of one path.
- rejected: an agent proposed a destructuring unbox, `let Box(inner: n) = move b;`; the owner rejected it, because the cell is an opaque struct, and an opaque struct refuses destructuring so that a linear host handle cannot be taken apart instead of closed.

### decision allocation-never-fails-in-the-source
Allocation never returns a failure and carries no `Result`; heap exhaustion stops the program, and a program or an entry may declare `no_heap` to give up the heap.
- rejected: an agent proposed an allocation that returns its payload inside an error value; the owner rejected it, because every push and every node would then carry an error arm, while virtual memory makes exhaustion a failure of the whole program, not of one site.
- rejected: an agent proposed an allocation that traps into a handler; the owner rejected it, because exhaustion is a failure of the whole program, and the domains that cannot tolerate it are the ones that give up the heap entirely.
- replaced: A static proof that each allocation's size fits was on main; the owner changed it, because a size the target cannot represent and a size the heap cannot supply both end the program from the trusted base, while proving the bound made writers clamp counts to ceilings no program needed.

### decision full-arrays-hold-owned-elements
A full array may hold owned elements and converts, by consuming, to and from a prefix window, and a helper that changes storage takes it by reference and declares its writes.
- rejected: an agent proposed forcing a completed array through a partial-window state; the owner rejected it, because completed fixed storage needs no window metadata.
- rejected: an agent proposed helpers that return the changed storage by value; the owner rejected it, because an owned mutation boundary copies the whole inline capacity.
