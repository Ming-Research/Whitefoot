# Process exit

How a program ends its execution at once with a status, from any context, while other contexts still wait.

### decision exit-is-a-capability-and-a-waiting-call
The invocation gives a `ProcessExit` capability in Inputs, shared by `exit_share`, and the waiting `exit_now` ends the execution with its status, running no join, release or scope exit.
- rejected: an agent proposed a non-waiting `exit_now` that compute code could call; the owner rejected it, because PAR-1 could overlap it with an adjacent statement, so an earlier statement could stay undone or PAR-1 would need a case of its own.
- rejected: an agent proposed an exit taking `StopSignals`, with no new Inputs field; the owner rejected it, because the tree already refuses unrelated functions on `StopSignals`, and every holder of the stop capability could then end the program.
