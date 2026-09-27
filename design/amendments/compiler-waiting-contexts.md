Node: compiler/waiting-contexts

Decision: Every context runs on the thread that runs the entry and changes hands only inside a completion join whose record is still pending, where the join parks it and runs the next ready context, so the ready queue, the parked contexts and the shared handle budget need no lock or atomic, because a compute task never waits [PAR-1, PAR-2] and so no switch falls between a compute offer and its join, and the hand-written runtime of that shape, one set of contexts per driver ring, met the bar against the native echo servers ([Experiment 1](../../research/investigations/io-model/WAITS.md#experiment-1-the-waiting-runtime-shape-against-the-native-echo-servers)), instead of running contexts on compute workers or migrating a context to whichever thread completes its record.

Decision: The floor owns each context's stack, a reservation with a guard page below it, and the switch, which moves the stack bounds the exhaustion handler classifies with before it changes stacks, because a fault on a context's guard page must end in the same stack record as the entry's, instead of stacks allocated where the bounds the handler reads do not describe them.

Decision: A context start lowers to a synthesized wrapper that makes the call and releases its result, started over a frame of its arguments on the new context's own stack, with a join before every exit of the starting activation and no path that runs the call inline, because a start is what the program means rather than an offer the runtime may refuse, and running the call inline would wait for it, instead of reusing the compute lanes, which refuse an offer by running it on the caller.

Decision: With no kernel completion ring and other contexts live, a socket receive, send or accept waits for its descriptor's readiness and is then made on the context's own thread, because a blocking call on that thread stops every context and the helper pool holds at most eight blocked peers, and the reverse-order peer test that the ring route passes stops on the helper route without it, instead of queuing peer-bound socket waits on the helper pool.

Rejected:
- One driver thread per core in this revision: rejected because several drivers need a ring each, a group count and handle budget another thread may change, and a rule placing started contexts, while no compiled measurement yet shows the single driver as the limit (`docs/todo.md`).
- A state-machine lowering of waiting functions: rejected because the language node [waiting](../language/waiting.md) refuses it, and no lowering decision reopens it.
