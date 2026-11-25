# Pools and arenas

Pools and arenas as ways of using ordinary storage, and what a stale handle into them means.

### decision pools-and-arenas-are-usage
A pool is a block of slots with integer handles into it, and an arena is the same block used with an append as allocation and a truncation as reset; neither is a type of its own.
- replaced: Arena and pool types whose content the checker kept apart from ordinary values was on main; the owner changed it, because their content is ordinary owned storage, and the confinement protected an origin discipline that references no longer carry.

### decision stale-handle-is-a-logic-error
A stale handle within the block's length names the slot's current occupant and is a logic error, and a program that must detect reuse keeps a generation number beside the element.
- rejected: an agent proposed occupancy or generation tags kept by the compiler for each slot; the owner rejected it, because occupancy decided by data belongs in data, and every program that reuses a slot would pay for a tag it does not need.