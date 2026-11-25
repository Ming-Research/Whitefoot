# Slab

The standard library's Slab: one backing of cells, each cell's vacancy, the free list, handles and growth.

### decision slab-cells-hold-one-slot-windows
Slab uses one backing whose cells each mark vacancy with an inline one-slot window, reuses cells through an ordinary free list, and retires a cell whose generation cannot advance.
- rejected: an agent proposed a dense payload array with swap removal and a reverse index; the owner rejected it, because a removal changes another live payload's slot index, against the interface of stable slots.
- rejected: an agent proposed one separately allocated Box for each payload; the owner rejected it, because inline payloads would then pay an allocation and an indirection each, though the backing already holds their storage.
- rejected: an agent proposed a compiler-known sparse shape for the Slab; the owner rejected it, because one library's evidence does not justify a new shape, and the window keeps cleanup visible to the existing rules (data: `research/experiments/container-representation/slab-library/RESULTS.md`).

### decision slab-reserve-grows-to-ceiling
Slab grows by explicit reserve up to its type's ceiling through ordinary grow, keeping every cell, index and generation, and its free list ends at the ceiling.
- rejected: an agent proposed rebuilding the cell bookkeeping when the Slab grows; the owner rejected it, because ordinary grow already keeps the cells' order and ownership, so every handle stays valid.
- rejected: an agent proposed promising stable payload addresses across growth; the owner rejected it, because growth may move the backing, and a handle promises only its index and generation.