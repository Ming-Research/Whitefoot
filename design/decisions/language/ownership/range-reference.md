# Range references

References to a contiguous run of elements, and to the runs and pages of paged storage: how they are formed, separated, passed and used.

### decision contiguous-range-reference
`&x[lo..hi]` forms a range reference of type `&[T]` over an Array, a Slots or another contiguous range after proving `lo <= hi <= x.len`; a Ring range is not admitted.
- rejected: an agent proposed admitting a Ring range from its logical bounds alone; the owner rejected it, because those bounds do not show that the physical extent is contiguous, so the pointer-and-count form would need a proved-contiguity rule or another representation.

### decision range-separated-like-any-path
A range is separated by the judgment all paths share, and from a window's append slot or free slots when proved empty or to end at or below the length; it overlaps the last filled slot.
- replaced: Comparing origin sets carried by values at call boundaries was on main; the owner changed it, because a range is a step in a path and needs no second mechanism for disjointness.
- replaced: Different roots and proved-distinct indices or ranges as a range's only separations was on main; the owner changed it, because a row such as `reads(values[start..end]), writes(values[slot])` was refused at every call, and a write through a range killed every fact beside it.
- rejected: an agent proposed classing an index, an append slot or free slots beside a range with the pairs that overlap at every position, so no call compares them; the owner rejected it, because some values separate such a pair, so a call could pass a read range that holds the written slot.
- replaced: A shared child view of an exclusive view, with the parent frozen against element writes while the child lived, was on main; the owner changed it, because with no permission marker a second range over the same storage is just a second path, and path overlap decides every access.

### decision functions-never-return-ranges
A function never returns a range reference; a helper that selects part of a storage returns indices with their bounds in `ensures`, and the caller forms the range.
- replaced: A view result with a declared origin ceiling, each result of one call in its own region, was on main; the owner changed it, because the caller already holds the path the range is formed from, so one address computation replaces the machinery that made a returned view's origin known.
- rejected: an agent proposed `Slice` and `MutSlice` as values that may be bound, moved, stored or returned; the owner rejected it, because a value that reaches storage it does not own is a reference in disguise, which the rules for owned aggregates and for references both exclude.

### decision paged-run-reference
A range over Paged is the parameter-only reference kind `&Run<T>`, with one captured length, element subscripts and re-slicing, while a selected page gives an ordinary `&[T]`.
- rejected: an agent proposed making a run that is not contiguous pass as a pointer-and-count range; the owner rejected it, because a logical run may cross allocations, while a page is contiguous.

### decision page-reference-overlaps-elements
A page reference is one captured page step in the filled window; distinct proved page indices separate their descendants, and a page overlaps every element index and range of its Paged.
- rejected: an agent proposed inferring separation from page arithmetic that depends on the target; the owner rejected it, because the fact language has no relation between a slot and its page.

### decision run-selectors-as-direct-bases
A selected segment or page is a direct base for its length read and for element subscripts, whose elements are ordinary element places.
- rejected: an agent proposed requiring a borrow before every length read or element access; the owner rejected it, because these places name exactly the place their `&` forms resolve to and add no proof or permission mechanism.
- rejected: an agent proposed limiting the selected elements to reads and writes; the owner rejected it, because a selected element names exactly an ordinary element place and adds no proof or permission mechanism.

### decision range-referent-only-as-base
The referent `r^` of a range reference is admitted only as the base of an element subscript, a range step or its `len`; any other use, `&r^` included, is refused with repairs.
- rejected: an agent proposed forming a second range reference from `&r^`; the owner rejected it, because it adds a second spelling of `let q = r;`.
- rejected: an agent proposed giving `&r^` the first element; the owner rejected it, because nothing in the spelling selects the first element.