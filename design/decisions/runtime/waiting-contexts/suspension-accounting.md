# Suspension accounting

How the context runtime counts native context invocations, so the stuck check sees a quiet program without every switch writing a shared counter.

### decision invocations-counted-per-executor
Each native context invocation moves an odd/even sequence on its physical executor's own cache line, and the stuck check reads all 128 slots before and after its driver scan.
- replaced: A process-wide active-invocation count and change counter, written on every native return, was on main; the owner changed it, because every cooperative switch on every driver then wrote the same cache lines, a cost that grows with the number of drivers.
- rejected: an agent proposed one invocation slot per logical driver; the owner rejected it, because a replacement thread can enter that driver while its displaced physical executor is still computing.
- rejected: an agent proposed one collection, or an active Boolean alone; the owner rejected it, because an invocation can begin and end between two observations without changing the final inactive value.

### decision cold-transitions-share-one-mutex
Quiescence scans and the cold idle, wake and transfer transitions run under one native mutex, and a driver stays logically idle through empty maintenance polls.
- rejected: an agent proposed a double collect with no serialized cold transitions and no persistent idle state; the owner rejected it, because failed probes and empty polls can keep invalidating every scan of a program that has no possible next step.

### decision fairness-suspension-skips-the-wake-gate
A suspension made only for fairness goes to its driver's ready queue after native unwind without the external-wake gate, which registered waits still use.
- rejected: an agent proposed using the wake-race handshake for it too; the owner rejected it, because a fairness boundary registers no external waker, so no other publisher competes, and the executing invocation stays visible until the queue publication.