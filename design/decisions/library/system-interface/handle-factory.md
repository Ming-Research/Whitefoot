# Handle factory

The budget of host handles: how acquisitions spend and return credits, how contexts share it, and the process's open-files limit.

### decision handle-factory-credits-are-explicit
Acquisition and close take the handle factory as a reference their effect row names, spending a credit on a successful acquisition and returning it on refusal or on the explicit close.
- rejected: an agent proposed exclusive factory loans with a rule against early release; the owner rejected it, because the factory's declared write and the ordinary pairwise call check already exclude competing access.
- rejected: an agent proposed a hidden global handle capacity; the owner rejected it, because the accounting stays visible when every acquisition names the factory it spends from.
- rejected: an agent proposed a reservation or one-shot permit whose drop silently returns factory credit; the owner rejected it, because opaque drop is empty and host-state writes are explicit, so a reservation would need an explicit consuming return.

### decision factory-share-draws-on-one-budget
`factory_share` returns a second factory drawing on the same budget, so a waiting call run as a context carries its own factory while the host limit stays one budget.
- rejected: an agent proposed splitting the credits into fixed partitions; the owner rejected it, because a partition refuses an acquisition while another holds unused credits, and a server's contexts would move credits out of the listener's budget for good.

### decision handle-factory-reads-and-raises-file-limit
`open_files_limit` reads and `raise_open_files_limit` raises the process's open-files limit through the factory; a successful raise adds credits, and hosts other than POSIX answer `Unsupported`.
- rejected: an agent proposed a new limit capability in Inputs; the owner rejected it, because the factory's budget comes from that limit, so a raise changes the factory's state and must be ordered with acquisitions through it.
- rejected: an agent proposed a function on `StopSignals` or `MemoryMeter`; the owner rejected it, because those observe other things.
- rejected: an agent proposed an ambient function with no parameter for the limit; the owner rejected it, because the limit is the factory's state, which a raise must order with acquisitions.
- rejected: an agent proposed a raise that silently stops at the hard limit; the owner rejected it, because a request above the hard limit goes to the host, whose refusal is the result.

### decision handle-factory-is-self-changing
`HandleFactory` is a self-changing host handle, so an atomic statement's guard reaches no factory.
- rejected: an agent proposed keeping factories readable in guards and waking guards on a raise; the owner rejected it, because a change from outside the process has no wake source, so a guard could wait forever on a true condition.

### decision descriptor-credits-cap-the-host-limit
The handle factory starts with 4096 descriptor credits on Windows, and on POSIX with the host limit capped at 2^20 less a reserve of 64; acquiring and closing update it.
- rejected: an agent proposed scanning the descriptor table; the owner rejected it, because a table of up to a million entries costs one host call per entry.
- rejected: an agent proposed atomic permit credits; the owner rejected it, because the call-site check [EFF-5] already serializes the accounting: no second path to the factory may be live at a call that writes it.
