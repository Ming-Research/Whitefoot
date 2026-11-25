# Frozen handles

How a `Frozen<T>` handle is lowered: its runtime block, its handle count and how the checker and lowering model it.

### decision a-frozen-handle-is-a-counted-pool-block-without-a-lock
A `Frozen<T>` handle points at its value in one runtime pool block whose header holds an atomic handle count and no lock, and the checker and lowering treat it as a cell with its own release class.
- rejected: an agent proposed reusing the shared object's block and its lock for frozen values; the owner rejected it, because no write ever reaches a frozen value, so the lock would only make readers wait for each other.
- rejected: an agent proposed a separate kind of type for frozen handles; the owner rejected it, because each of its cases would repeat the cell's read path, while a read through `inner` is exactly a cell's content read.