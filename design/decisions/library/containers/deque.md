# Deque

The standard library's Deque: its ring, its endpoint operations and its rebase.

### decision deque-exposes-boxed-ring-directly
The Deque library works on the ordinary boxed Ring directly, changes its ends through references, and rebases by a consuming conversion to a newly allocated backing.
- rejected: an agent proposed a wrapper struct around the Ring; the owner rejected it, because a wrapper loses the Ring's public measure contracts, while the direct shape keeps the existing measure words (data: `research/experiments/container-representation/deque-library/RESULTS.md`).