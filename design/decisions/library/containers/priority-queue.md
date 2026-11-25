# Priority queue

The standard library's plain and indexed priority queues: their heap, how they are emptied, and the sift they share.

### decision priority-queue-is-growable-binary-heap
PriorityQueue is a growable binary heap over an ordinary boxed prefix, for owners of any type.
- rejected: an agent proposed the earlier fixed-capacity heap witness as the library; the owner rejected it, because only the complete comparison established growth, heapify and consumption for any owners (data: `research/experiments/container-representation/priority-library/RESULTS.md`).
- rejected: an agent proposed moving priority policy into the kernel; the owner rejected it, because an ordinary library heap already gives growth, replacement, heapify and consumption for any owners (data: `research/experiments/container-representation/priority-library/RESULTS.md`).

### decision priority-queue-drain-differs-from-free
Draining a priority queue in priority order is a separate operation from its final consumption in physical order.
- rejected: an agent proposed one final release that always consumes in priority order; the owner rejected it, because ordered consumption repairs the heap each time, while a caller that only needs every owner cleaned up can consume the window in linear time.

### decision priority-queues-share-one-sift-core
Plain and indexed priority queues share one bounded sift core, which reports positions into state apart from the heap and the comparator.
- rejected: an agent proposed a separate sift for the indexed queue; the owner rejected it, because one core keeps one set of bounds and ownership proofs, and gave identical indexed executables (data: `research/experiments/container-representation/indexed-library/RESULTS.md`).