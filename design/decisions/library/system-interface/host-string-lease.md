# Host string lease

How the host strings and relative paths taken from the invocation are held.

### decision host-strings-are-plain-opaque-values
HostString and RelativePath are ordinary opaque values with native representations the invocation backs, and no borrow or finalizer the writer sees.
- rejected: an agent proposed leases that carry a region, or an owned buffer with a reference count; the owner rejected it, because values taken from arguments must stay storable and lossless, while the supplied implementation keeps their backing valid through their uses.