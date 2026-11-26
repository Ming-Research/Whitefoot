# Storage representation

How the compiler lays out storage shapes in memory: Box owners and their headers, empty and zero-sized storage, growth, Segments, Paged, range and run references, and heap alignment.

### decision a-runtime-capacity-box-is-one-pointer-to-a-header-first-block
A contiguous runtime-capacity Box owner is one pointer to header-first storage that holds only the shape's runtime measure words.
- rejected: an agent proposed a fat descriptor that owns one element allocation; the owner rejected it, because it widens owner, call and capture transport and puts owner metadata apart from the block.
- rejected: an agent proposed a pointer to a separately allocated descriptor and a second element block; the owner rejected it, because it keeps one-word transport only by adding an allocation and a release, while the header-first block meets both aims.

### decision an-empty-runtime-window-points-at-one-shared-header
A capacity-0 runtime-capacity Array, Slots, Ring or Paged Box points at one read-only shared header, which release and retake recognize.
- replaced: A header allocated for each capacity-0 owner was on main; the owner changed it, because Halo's binary-trees spent at least 25.6% of its time in malloc and free with such a header per table, while Rust's `Vec::new` allocates nothing.
- rejected: an agent proposed a null cell pointer for an empty owner; the owner rejected it, because every header read would have to test for it.

### decision constant-and-runtime-capacity-share-one-header-order
Constant- and runtime-capacity shapes use the same header order and element-address rule, leaving out compile-time measures, and Slots and Ring keep their own headers.
- rejected: an agent proposed an elements-first inline layout with an adapter, or one physical header with unused words for every shape; the owner rejected it, because one order avoids measure sources and offsets that differ by placement, within OP-9's ceilings.

### decision a-compile-time-measure-reference-points-at-a-shared-constant
A reference to a compile-time measure lowers to the address of a private `unnamed_addr` constant holding its u64 value, one per distinct value in each module.
- rejected: an agent proposed adding the word to every constant-capacity header; the owner rejected it, because the measure is readonly and fixed for the owner's lifetime, and the language observes no reference address.
- rejected: an agent proposed refusing the borrow; the owner rejected it, because the specification admits it.

### decision constant-capacity-windows-are-built-header-only
Building a value with constant-capacity window storage sets its headers and value-bearing fields and leaves unoccupied Slots and Ring elements uninitialized outside an enum's inactive variants.
- replaced: Zero-filling the whole value was on main; the owner changed it, because WIN-1 makes the unoccupied tail unobservable, and a memset of it was 21 percent of instructions in a profile of a Snowghost style computation.

### decision an-enum-construction-zero-fills-its-inactive-variants
Building an enum value zero-fills every field but the active variant's fields that hold constant-capacity window storage, so an inactive variant's fields are cleared, window fields included.
- rejected: an agent proposed leaving an inactive variant's window fields unwritten in enum values over 128 bytes; the owner rejected it, because it resolved no gain in the stable scatter's eight-worker call (data: `research/investigations/scatter-chunk-building/DESIGN.md`).
- rejected: an agent proposed leaving an inactive variant's window fields unwritten at every size; the owner rejected it, because firn's byte strings and the ordered map's links would be built differently, and nothing had measured them built that way.
- rejected: an agent proposed leaving every inactive payload unwritten; the owner rejected it, because the hash map's wide replacement and the slab's retained scalar lookup ran slower with it (data: `research/experiments/container-representation/map-library/RESULTS.md`).

### decision slots-and-ring-are-a-shape-with-optional-capacity
Slots and Ring are represented by a shape, an element type and an optional capacity, with separate Array forms, and the old region-bearing view, store, arena and extent types are gone.
- rejected: an agent proposed one checked type per placement; the owner rejected it, because shape and optional capacity already encode what layout and measure classification read, without placement flags kept in step.
- replaced: Keeping region-bearing view, store, arena and extent types that nothing could construct was on main; the owner changed it, because no source or runtime consumer owns their meaning, and keeping them left dead match arms.

### decision a-range-reference-is-an-element-pointer-and-count
A contiguous range reference lowers to a nonowning element pointer and count, read through a dedicated range place by the ordinary lowering.
- rejected: an agent proposed coercing the pointer and count into an owned descriptor, or carrying the source container's metadata beside it; the owner rejected it, because the admitted operations need neither capacity nor allocator provenance.

### decision slots-growth-retakes-its-block
Positive-capacity `grow` of a `Box<Slots<T>>` is one `wf__heap_retake` at the new size, which calls `realloc` for a block of at least 1024 bytes and takes a writable block for the shared empty header.
- replaced: Allocating a new block, copying the filled slots and freeing the old one was on main; the owner changed it, because the allocator may extend a block in place, and `realloc` made Halo's integer-table kernel 14.0% faster and sort 2.5% faster on the i9-14900K.

### decision small-blocks-are-copied-on-growth
`wf__heap_retake` grows a block smaller than 1024 bytes by allocating a fresh block, copying the old one and freeing it.
- replaced: Calling `realloc` for every size was on main; the owner changed it, because glibc's `realloc` takes the owning arena's lock, and four workers queued on it in Snowghost's text preparation (data: `research/investigations/grow-allocator-contention/DESIGN.md`).

### decision a-zero-stride-step-addresses-with-zero
Target lowering puts zero only in the element-address operand of a step whose target stride is zero, and keeps logical indices, lengths and window coordinates in their other uses.
- rejected: an agent proposed rejecting representable zero-byte storage whose logical size exceeds the signed address domain; the owner rejected it, because a zero-stride step has zero displacement for every admitted index.
- rejected: an agent proposed weakening STOR-6 to permit address operands the target cannot represent; the owner rejected it, because STOR-6 requires exact target representations for the indices used in address formation.
- rejected: an agent proposed relying on optimizer facts or dead-code elimination to erase a zero-stride address; the owner rejected it, because qualification must hold for ordinary code before optimization, and facts must not decide target success.
- rejected: an agent proposed replacing every logical index of a zero-byte element with zero; the owner rejected it, because bounds, range lengths, window coordinates and source observations keep their logical values.

### decision a-zero-capacity-payload-is-a-zero-length-byte-array
A constant-capacity zero Array, Slots or Ring represents its empty payload as a zero-length byte array, so it has no element alignment, while Slots and Ring keep their descriptor words.
- rejected: an agent proposed keeping the typed empty tail and refusing nested zero-capacity owners whose alignment then breaks the ceilings; the owner rejected it, because no element is present to need that alignment, and the zero-capacity Array form already omits it safely.
- rejected: an agent proposed treating a zero-length typed LLVM array as free of alignment; the owner rejected it, because its element still sets the LLVM ABI alignment, so header, stride and parent qualification could disagree with what is emitted.

### decision a-segments-owner-is-one-header-first-block
A `Segments<T>` Box owner is one pointer to one block holding the segment count, count plus one offsets and then the elements, whose start is computed again from the count at each access.
- rejected: an agent proposed a separately allocated bounds array; the owner rejected it, because it adds an allocation and a release, while one block keeps the owner a thin pointer freed at once.
- rejected: an agent proposed storing the element offset in a word; the owner rejected it, because it would only replace a shift, an add and a mask per access with one load, and no measurement shows the computation on a hot path.
- replaced: Refusing construction past a target-independent size predicate was on main; the owner changed it, because that refusal covered only sizes above 2^62 bytes, which no heap can supply, so the block is now sized with the checked arithmetic of the other runtime shapes.

### decision a-paged-owner-is-one-header-first-cell-with-its-directory
A Paged Box owner is one pointer to a cell holding len, cap, directory capacity and the page pointers, with pages of B elements, B the largest power of two with B times the stride ceiling up to 4096.
- rejected: an agent proposed a Paged descriptor pointing to a separately allocated directory; the owner rejected it, because it pays a dependent load per element access that a hand-written page table does not (data: `research/investigations/paged-storage/DESIGN.md`).
- rejected: an agent proposed the X1 and X2 allocation experiments, a directory floor and pages allocated lazily; the owner rejected it, because they cut page faults without saving time (data: `research/investigations/paged-storage/DESIGN.md`).
- rejected: an agent proposed a page size from the target's stride; the owner rejected it, because the page length is visible through `paged_page_len` and `p.pages.len`, so an accepted program's output would depend on the target.
- rejected: an agent proposed a radix descent; the owner rejected it, because pages of a fixed power of two give constant-time shift-and-mask indexing, and growth keeps every full page in place.

### decision paged-growth-replaces-the-cell-and-keeps-full-pages
Paged construction sizes the cell for a directory doubled to fit the pages; growth past it allocates a new cell, copies the header and page pointers and frees the old cell.
- rejected: an agent proposed copying every payload on growth, or allocating a cell for every element; the owner rejected it, because growing the cell by doubling keeps the one-word owner and the addresses of full pages.

### decision paged-page-zero-starts-small-and-grows
A positive-capacity Paged allocates page 0 at the least power of two covering its capacity, at most B, replaces and copies it when growth passes that before B, and gives every later page B elements.
- replaced: A full first page for every positive capacity and a header for every empty owner was on main; the owner changed it, because Snowghost would otherwise have to route around the extra allocations in source rather than use and improve Paged.
- rejected: an agent proposed a fixed smaller page 0 with non-uniform indexing; the owner rejected it, because choosing between its boundaries adds a compare and branch to every Paged element address.

### decision a-run-reference-is-a-directory-pointer-origin-and-length
A Run reference lowers as the directory pointer at cell + 24, its lower endpoint and its length; a page reference is an ordinary contiguous pointer and length.
- rejected: an agent proposed carrying a Box owner in the nonowning run; the owner rejected it, because element addressing needs only the page and offset, which the directory gives.
- rejected: an agent proposed keeping references across a reallocation of the cell; the owner rejected it, because growth replaces the cell that holds the directory, so it invalidates every reference first.

### decision heap-blocks-are-aligned-to-16-bytes
Every target record states 16 bytes as the alignment of heap blocks, so Box cells, Slots, Ring, Paged and the HashMap hold elements aligned up to 16 through the ordinary heap functions.
- rejected: an agent proposed aligned allocation up to 4096 bytes, with an original pointer beside each pool object and a 4096-byte shared empty header; the owner rejected it, because no stored type is aligned above 16, while every `Frozen` and `Shared` object would pay for the pointer.
- rejected: an agent proposed a 4096-byte ceiling with the original address derived from the element type's alignment; the owner rejected it, because every projection to a header would take the alignment, widening the runtime interface, for alignments no type reaches.