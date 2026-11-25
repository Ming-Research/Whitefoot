# Ordered map

The standard library's ordered map: its tree shape and how an insertion descends it.

### decision ordered-map-is-fanout-16-btree
OrderedMap is a B-tree of fanout 16 over boxed nodes, each node with one leading child link and each entry owning its following link of zero or one child.
- rejected: an agent proposed separate entry and child prefix windows in each node; the owner rejected it, because entry occupancy alone does not supply the premise on child capacity that a merge needs.
- rejected: an agent proposed a growing indexed pool of nodes; the owner rejected it, because node lookup by identifier and relocation of the backing would add a second change of representation to the balancing experiment.
- rejected: an agent proposed choosing a default ordered tree from allocation counts or a native AVL tree alone; the owner rejected it, because fewer B-tree allocations do not ensure fewer reserved bytes or less time, and a native AVL floor does not give the cost of a Whitefoot AVL.
- rejected: an agent proposed a single-descent insertion with a borrowed pair carrier and aggregate promotion returns; the owner rejected it, because its replacement regressions failed the registered criterion (data: `research/experiments/container-representation/ordered-library/RESULTS.md`).
- rejected: an agent proposed a single-descent insertion borrowing both the pair and the promotion slots; the owner rejected it, because its replacement regressions stayed without aggregate returns (data: `research/experiments/container-representation/ordered-library/RESULTS.md`).