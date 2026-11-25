# Vector

The standard library's growable vector, and how it consumes a removed suffix.

### decision vector-consumes-suffix-by-exchange
A Vector consumes a removed suffix in original order by exchanging each first-half rear element into the next suffix slot, then consuming the reversed rest from the back.
- replaced: Reversing the suffix in a separate pass before consuming it was on main; the owner changed it, because the paired comparison showed a lower cost of moving large records, though short scalar reuse cost more (data: `research/experiments/container-representation/vector-library/RESULTS.md`).
- rejected: an agent proposed walking the retained prefix to consume the suffix; the owner rejected it, because the chosen exchange moves only O(removed) elements.
- rejected: an agent proposed allocating scratch storage to consume the suffix in order; the owner rejected it, because the chosen exchange needs only constant extra element storage.