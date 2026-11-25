# Storage shapes

The five storage shapes: what each one stores, how a value enters and leaves a slot, and how a window in a heap cell is reached and grown.

### decision five-storage-shapes
Storage has five shapes, `Array`, `Slots`, `Ring`, `Segments` and `Paged`; the first three have an inline constant-capacity form and a heap-only runtime form, the last two only the heap form.
- rejected: an agent proposed one growable storage type, set by whether it has a length and a head; the owner rejected it, because the metadata a shape stores is the whole difference between the shapes, and a full array would carry a length it can never change.

### decision segments-own-their-boundaries
`Segments<T>` is one contiguous run of elements whose segment boundaries `box_segments_filled` fixes; its measure is its segment count, and a subscript selects one segment.
- rejected: an agent proposed an offsets witness type with a parallel-permission clause of its own; the owner rejected it, because it adds a type whose only purpose is to carry a proof, while the storage shape already gives segments their disjointness.
- rejected: an agent proposed a proved fact that an array of offsets does not decrease; the owner rejected it, because a range fact separates only single element accesses in a counted loop, not the range reference a segment is handed to a callee as, and the shape already makes segments disjoint.
- rejected: an agent proposed a buffer per item; the owner rejected it, because its allocations ran five to eleven times slower than one contiguous buffer in the measured scatter, and slowed down with more workers (data: `research/investigations/segmented-storage/DESIGN.md`).
- replaced: `box_segments_filled` returning `None` for a total above 2^62 bytes was on main; the owner changed it, because no supported heap can supply such a size, while a smaller size the heap cannot supply already ended the program.
- rejected: an agent proposed a `None` sized by the selected target; the owner rejected it, because a program's result would then depend on the target.

### decision storage-stores-only-needed-measures
A storage stores only the measure words its shape needs, as readonly fields: an array its `len`, a prefix window `len` and `cap`, a ring window `len`, `cap` and `head`.
- replaced: A fourth word, `room`, the capacity minus the length, was on main; the owner changed it, because it gave a proof nothing the length and the capacity do not, and it needed a special rule tying every write of the length to the facts about it.

### decision each-measure-word-has-its-own-facts
An effect row entry that names one measure word kills only that word's facts, while an entry that names the whole storage overlaps all of its words.
- rejected: an agent proposed treating any write inside a storage as a write of its whole descriptor; the owner rejected it, because an operation that moves only the boundary must not cost its caller a capacity fact proved before the call.

### decision window-parts-are-effect-vocabulary
A window's four named parts, the append slot, the last filled slot, all filled slots and all free slots, exist only for effect rows and the overlap judgment.
- rejected: an agent proposed counting an append as a write of the whole storage; the owner rejected it, because a row that names only the append slot and the length lets an appending call be proved not to disturb a live reference to a filled slot.

### decision no-window-slot-is-ever-empty
A value leaves a window slot only by an operation that moves a boundary or exchanges the value; a move out of a slot is refused, and no slot carries an occupancy tag.
- rejected: an agent proposed a take that leaves a hole in the slot; the owner rejected it, because no slot may ever be observable as empty, and none carries a tag that could report it as empty.
- rejected: an agent proposed per-slot occupancy tags kept by the compiler, or a vacancy type state; the owner rejected it, because a program that needs occupancy stores it as data it can read, and every other program would pay for the tag's storage and test.

### decision boxed-window-operations-take-the-content
An operation that moves a boxed window's boundary takes `&b.inner`, while growth takes `&Box<Slots<T>>` or `&Box<Paged<T>>`, and `free_empty` consumes the cell.
- rejected: an agent proposed handing every operation on a boxed window the cell itself; the owner rejected it, because a boundary move touches only the content, and only an operation that reallocates or frees the heap object must name the owner it replaces.
- rejected: an agent proposed a built-in growth operation for `Ring`; the owner rejected it, because allocation, ordinary window transfers and consuming the emptied backing already express a rebase that keeps the order.

### decision heap-window-content-stays-in-its-cell
The content of a cell holding a runtime-capacity form, a `Segments` or a `Paged` is never moved out; the cell's release, or `free_empty` once it is proved empty, reclaims it.
- rejected: an agent proposed allowing the unbox of such a content; the owner rejected it, because such a window exists only as a cell's content, so the moved value would need a home that no placement gives, for a capacity not known until it is built.

### decision a-subscript-reaches-the-shapes
A subscript applies to the five storage shapes, to range references and to constant tables, and a range reference names part of existing storage without owning it.
- replaced: A first-class slice value beside the shapes was on main; the owner changed it, because a range names part of existing storage for a callee, and need not be another owning storage shape.

### decision paged-moves-only-a-small-first-page
`Paged<T>` is a window with one-level element indices whose growth may move only page 0, before it holds B elements, copying at most B - 1 elements; full pages never move.
- replaced: Paged growth that moved no element was on main; the owner changed it, because Snowghost's small owners paid 89,038 extra header allocations and 100,466 extra full pages, and it keeps no page reference or address across growth (data: `research/investigations/paged-storage/DESIGN.md`).
- rejected: an agent proposed stable page growth inside `Slots`; the owner rejected it, because one type would then carry two intents that callers need to choose between, and every `Slots` access would pay a directory load and lose contiguous ranges.
- rejected: an agent proposed a two-level page table written as a source library; the owner rejected it, because a flat index becomes quotient and remainder steps, and Snowghost's probe of it lost the flat-index loop permission.
- rejected: an agent proposed `Slots<Box<T>>` with growth that keeps references to the contents; the owner rejected it, because it needs one allocation per element and scattered payloads, while growth still copies every element pointer.
- rejected: an agent proposed a radix directory whose references survive growth; the owner rejected it, because every address formation would need a directory effect part and its overlap rules, beyond the need for stable elements whose references are formed again after growth.
