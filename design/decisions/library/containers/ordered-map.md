# Ordered map

The standard library's ordered map: its tree shape, what its clients see of the tree, and how an insertion and a walk descend it.

### decision ordered-map-is-fanout-16-btree
OrderedMap is a B-tree of fanout 16 over boxed nodes, each node with one leading child link and each entry owning its following link of zero or one child.
- rejected: an agent proposed separate entry and child prefix windows in each node; the owner rejected it, because entry occupancy alone does not supply the premise on child capacity that a merge needs.
- rejected: an agent proposed a growing indexed pool of nodes; the owner rejected it, because node lookup by identifier and relocation of the backing would add a second change of representation to the balancing experiment.
- rejected: an agent proposed choosing a default ordered tree from allocation counts or a native AVL tree alone; the owner rejected it, because fewer B-tree allocations do not ensure fewer reserved bytes or less time, and a native AVL floor does not give the cost of a Whitefoot AVL.
- rejected: an agent proposed a single-descent insertion with a borrowed pair carrier and aggregate promotion returns; the owner rejected it, because its replacement regressions failed the registered criterion (data: `research/experiments/container-representation/ordered-library/RESULTS.md`).
- rejected: an agent proposed a single-descent insertion borrowing both the pair and the promotion slots; the owner rejected it, because its replacement regressions stayed without aggregate returns (data: `research/experiments/container-representation/ordered-library/RESULTS.md`).

### decision ordered-map-tree-is-private
OrderedMap's tree is private to its module: the root field and the node and entry types are unpublished, and only the length is published, for its contract.
- replaced: the root field and the node and entry fields were published readonly for clients to walk; the owner changed it, because the fanout-16 node is a measured baseline that may change, and every client walk written against it would break with it.

### decision ordered-map-walks-by-owned-cursor
An OrderedMap walk is an owned cursor holding a path of child indices and the map's version, made at an end or a probe, stepped either way, read by lending, ranked in the module.
- rejected: an agent proposed a walk that lends each pair from a position to one callback until it returns a stop flag; the owner rejected it, because a pair written to two places is copied out first, and two maps cannot be walked together.
- rejected: an agent proposed successor and predecessor queries by key; the owner rejected it, because every step compares keys from the root and the caller copies a key every step to keep its place.
