# Concurrent map

The C runtime's concurrent hash index behind every `ConcurrentHashMap`: its cells, moves, entry nodes, holds, waits, scans, memory and key sets.

### decision the-map-index-locks-a-cell-by-one-compare-and-swap
The runtime keeps one concurrent hash index in C, with 16-byte cells probed linearly; a writer locks its key's cell by one compare-and-swap on the key word's top bit.
- rejected: an agent proposed cache-line buckets with a ticket lock; the owner rejected it, because a branch on the matching slot mispredicted on most lookups, they used 41.9 bytes a key to growt's 32.4, and the first duel lost nine cells (data: `research/investigations/concurrent-map/DESIGN.md`).
- rejected: an agent proposed writing the index in Whitefoot source; the owner rejected it, because a lock-free index needs compare-and-swap and deferred reclamation, which Whitefoot could express only through the atomic fields and lock-free cells the shared-object design refuses.

### decision a-map-table-moves-cooperatively-when-half-full
A table moves once half its cells are used: one writer makes the next table, writers that meet the move copy blocks into it, and the mover only reads the old table.
- rejected: an agent proposed a move that marks every cell it has moved; the owner rejected it, because a mover that only reads the old table took single-thread `grow` from about 9.5 to 12.6 million a second (data: `research/investigations/concurrent-map/DESIGN.md`).
- rejected: an agent proposed that every writer crossing the threshold allocate its own next table; the owner rejected it, because at four threads each writer then cleared a fresh 128 MiB table, 1.4 to 2.3 s each (data: `research/investigations/concurrent-map/DESIGN.md`).

### decision a-moved-map-keeps-spare-cells-only-of-its-size
A moved table is freed once no user is in it, and the newest freed cells are kept for the next move only when they are the current table's size.
- replaced: Keeping the newest freed table's cells whatever their size was on main; the owner changed it, because after growth they are half the current table and serve only a move back to that size, holding 128 to 256 MiB idle at ten million keys, which a server's memory limit counts.
- rejected: an agent proposed freeing every host-mapped table's cells; the owner rejected it, because a map whose size holds steady would then move to fresh memory each time, 111 ms against 12 ms for 2^22 cells at four threads (data: `research/investigations/concurrent-map/DESIGN.md`).

### decision a-map-entry-lives-in-a-node-the-cell-points-to
A map of entries keeps each key's bytes and its `Option<V>` slot in a node from per-user chunks; the cell holds the key's hash and the node's address.
- rejected: an agent proposed storing the keys in the cells; the owner rejected it, because a cell's size would then depend on the key.

### decision an-inserting-writer-reuses-a-removed-cell
A writer that may make an absent entry present claims the first removed cell its probe passed, or else the empty cell ending the probe, and keeps it only after reading the key's run.
- rejected: an agent proposed keeping an emptied key's cell and node as an entry with an empty slot until the next move; the owner rejected it, because the mover rather than the cell's lock would then free nodes, and every distinct missed key would hold a node until the next move.

### decision map-users-are-numbered-by-thread
A table's users are the threads that run contexts, numbered by thread, and a statement keeps what it holds of a table in a record of its own frame.
- replaced: A user per driver was on main; the owner changed it, because a displaced thread can still be inside a statement when its driver's role passes to a spare thread, so two threads would act as one user.
- replaced: The one entry a driver thread kept for its running statement was on main; the owner changed it, because one statement may hold entries of several tables, or under several of its bindings, at once.
- rejected: an agent proposed a user per context; the owner rejected it, because a statement holding entries never suspends, so it ends on the thread it began on.

### decision a-statement-out-of-patience-holds-the-whole-table
A statement waiting for a held entry spins without parking; past 2^16 pauses it gives back any claim, holds the whole table, then locks its entry; whole-table holds go by ticket.
- rejected: an agent proposed a waiter mark beside an entry's cell, the unlocker handing the cell to a marked waiter; the owner rejected it, because a move gives that cell back and sends the waiter to compete in the next table, so a waiter's place does not survive the moves [WAIT-2] allows.
- rejected: an agent proposed a writer that yields the processor while its key is locked; the owner rejected it, because one-key `update` at four threads stayed at 6 to 16 million a second, against about 24 with a doubling pause count (data: `research/investigations/concurrent-map/DESIGN.md`).

### decision a-statement-holding-entries-borrows-a-handed-object
A statement holding a table's entries that takes a shared object spins for it, and borrows a hold an unlock handed a parked context, giving it back when it ends.
- rejected: an agent proposed parking inside the block; the owner rejected it, because the entry would stay held while other contexts on its driver spin for it.
- rejected: an agent proposed that a context owed the object after a take-back reserve it when it next runs; the owner rejected it, because ordinary statements could still take the object before the owed context ran, and every hand-off had to stop while any context was owed.

### decision map-cell-arrays-come-from-the-host-on-huge-pages
The index takes small blocks from the runtime's pool, and maps cell arrays of 2 MiB or more and entry chunks from the host, advising huge pages.
- rejected: an agent proposed taking the index's memory from the program's allocator; the owner rejected it, because the runtime every build links never calls the program's allocator [STOR-8].
- rejected: an agent proposed 4 KiB pages for large cell arrays; the owner rejected it, because a random probe in a large table then pays a page walk on most accesses, 105 against 127 million uniform reads a second at four threads (data: `research/investigations/concurrent-map/DESIGN.md`).

### decision a-map-cell-counts-its-readers-in-the-value-word
A map of entries counts an entry's reads under way in the top 16 bits of its cell's value word, and a writer locks the key word, then waits for that count to reach zero.
- rejected: an agent proposed marks each reader sets on its own driver's line; the owner rejected it, because every writer of every key would scan every driver's mark.

### decision a-key-set-hold-locks-cells-in-tag-order
A statement holding a set of keys' entries locks their cells in increasing order of the keys' 62-bit tags, then of their bytes, the order every such statement uses.
- rejected: an agent proposed locking each entry as the block reaches it; the owner rejected it, because two blocks reaching two keys in opposite orders form a cycle of waits, which a block that runs once cannot back out of.
- replaced: Locking in the increasing order of the keys' bytes was on main; the owner changed it, because it made a key set keep byte order for its reader and called `memcmp` at every step of the sort, where tags compare integers (data: `research/investigations/shared-state/DESIGN.md`).

### decision a-whole-hold-counts-its-own-entries
A count under a hold of the whole table counts the hold's own entries as they stand, read by their tags, and the users' counts for the rest.
- rejected: an agent proposed forbidding a count after writes under a whole hold; the owner rejected it, because that would be a language rule for a container's detail.

### decision a-map-scan-step-reads-only-the-current-table
A `map_scan` step reads only the current table, keeps the keys whose position lies in its range and whose slot is not `None`, and writes nothing of the map.
- rejected: an agent proposed a step that reads both tables or helps a move; the owner rejected it, because no move is under way when a step begins, reading the old table too would insert copied keys twice, and a reader may not help a move.

### decision a-map-scan-step-sizes-its-work-by-fullness
A `map_scan` step ends at once on a map with no `Some` entry, and otherwise passes about count keys or count times ten times the cells per live entry.
- rejected: an agent proposed a fixed budget of ten cells per requested key; the owner rejected it, because a server presizes its map, so an empty 2^19-cell map took 5,243 steps at Redis's default count of 10, where Redis ends at once (data: `research/investigations/map-enumeration/DESIGN.md`).

### decision a-map-clear-releases-values-after-the-hold
A clear exchanges the map's entries with a new empty map's under the whole hold, and the release runs each value's drop helper after giving the hold up.
- rejected: an agent proposed draining and releasing every value under the hold; the owner rejected it, because releasing them after the hold costs other statements no wait.

### decision a-map-user-protects-a-table-only-while-active
A map user keeps its table publication across statements as an address token, and reclamation protects a retired table only while that user is in a statement.
- rejected: an agent proposed clearing the publication at every outermost statement exit and republishing it on the next access; the owner rejected it, because that adds two locked XCHG operations to an ordinary x86-64 statement, while the existing activity and gate already protect.

### decision a-payload-only-statement-takes-nothing-for-an-absent-key
A single-entry statement whose writes all pass through the `Some` payload takes no cell or node for an absent key and locks a present key exclusively.
- rejected: an agent proposed deferring the node until the block first writes; the owner rejected it, because that still claims and unlocks a cell for every absent key.

### decision a-map-reserve-release-never-waits
A release of a map's reserve only tries the map's short lock, answering zero when busy; holding it, it frees the spare cells and the retired tables no user is in.
- replaced: Releasing only the spare cells, relying on the caller asking again, was on main; the owner changed it, because a finished statement's publication could pin a retired table for ever, and asking again only retried the short lock, so firn kept getting zero over its memory limit.
- rejected: an agent proposed waiting for the short lock; the owner rejected it, because firn asks again on its next command, so a wait would add a move's or a reclaim's duration to a command's latency.
- rejected: an agent proposed a release only inside a statement holding the whole map; the owner rejected it, because each release would then wait for every entry statement under way, while the release needs no such exclusion.

### decision a-bounded-scan-admits-positions-before-allocating
A bounded scan checks whole positions against its byte limit without allocating, and allocates its exact sort workspace only after admitting a prefix.
- rejected: an agent proposed copying first and testing the final heap delta; the owner rejected it, because gathering or sorting an unaffordable first position would itself exceed the promised heap bound.
- rejected: an agent proposed discounting the spare storage the thread already holds; the owner rejected it, because a thread-local spare can change the budget or grow its arena beyond the fresh simulation.

### decision a-released-small-key-set-stays-as-the-thread-spare
`key_set_bytes` sums the host-granted capacities of a key set's store and arena; a release lowers the heap reading by that sum, a small set's blocks staying as the thread's spare.
- rejected: an agent proposed counting only a key set's occupied bytes; the owner rejected it, because the retained-byte contract counts unused capacity and requires a release to lower the metered heap by the reported holding.
- replaced: A thread-local spare whose kept blocks stayed in the heap reading was on main; the owner changed it, because releasing a set small enough to be kept then lowered the reading by nothing, so `key_set_bytes` was not the decrease its release brings.
- replaced: Returning every key set to the runtime pool was on main; the owner changed it, because the pool's one lock on every driver cost firn's two-core `MSET` a quarter of its throughput, which the spare wins back within twin noise (data: `research/investigations/map-enumeration/DESIGN.md`).