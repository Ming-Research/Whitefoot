# Kernel minimality

Which storage operations the kernel owns, and which begin as source libraries built over them.

### decision the-kernel-owns-only-storage-transitions
The kernel owns the storage shapes and only the operations that move a window boundary, relocate slots, exchange a place's value or change a capacity; container policies are source libraries.
- replaced: Leaving swap, insertion and middle removal to source libraries over the storage was on main; the owner changed it, because each needs a value moved out of a slot, which the language does not admit, so each must be one kernel relocation that leaves no slot empty.
- rejected: an agent proposed a kernel container per policy, such as a growable vector, a pool, a keyed table or a string; the owner rejected it, because the storage operations give the authority memory safety needs, while growth, recycling and occupancy policy belong to the program.
- rejected: an agent proposed treating the kernel's storage transitions as ordinary source functions; the owner rejected it, because their contracts relate measures that only the storage can change, so a source body could neither establish them nor be checked against them.
- replaced: The claim that a library built on the kernel operations costs nothing extra at run time was on main; the owner changed it, because consuming a prefix window in order needs extra element transfers (data: `research/experiments/container-representation/vector-library/RESULTS.md`).

### decision the-kernel-owns-window-clear
The kernel's `clear` runs every element's derived release, sets the length to zero and keeps the storage, capacity, ring origin and pages, for any droppable element type.
- replaced: Clearing as a source library over `take_back` was on main; the owner changed it, because a library can only take the elements out one at a time or move them all to another window, so it cannot reset a window in a time that does not grow with its length.
- rejected: an agent proposed a `clear` only for copyable elements; the owner rejected it, because it would refuse structs that need no release but are not copyable, and would leave a window whose elements need release with no way to be cleared.

### decision paged-storage-is-a-kernel-shape
The kernel owns `Paged<T>` and `grow_paged`, so each element of paged storage is a one-level place for the bounds, range, overlap and parallel-permission rules.
- rejected: an agent proposed page storage as a source library; the owner rejected it, because source page storage cannot give those rules one-level element places, so every place consumer would need quotient-and-remainder separation.
