Node: language/waiting

Decision: Waiting is a function kind the writer declares with `waits` after the effect row, a waiting call is admitted only in the body of another waiting function, and the entry may wait, because compute code that never waits is what lets a helping join run any task beneath it, and a declared kind shows the writer and the runtime every place a context can pause, instead of deriving where waiting happens from the call graph or absorbing any wait in the scheduler ([design](../../research/investigations/io-model/WAITS.md#waiting-is-a-function-kind-the-writer-declares)).

Decision: A waiting call is an ordinary call, and a context pauses only inside a host operation whose result it needs, by switching its own stack to a driver that resumes another ready context, because the written function is then the function that runs and every proof checked on it still describes it, and the hand-written runtime of that shape reached 0.94 to 0.98 of the best native echo server on the development host ([Experiment 1](../../research/investigations/io-model/WAITS.md#experiment-1-the-waiting-runtime-shape-against-the-native-echo-servers)), instead of compiling a waiting function into a resumable state machine.

Rejected:
- A call-site `wait` keyword: rejected because the callee's signature already states which calls wait, so the keyword would repeat it at every call.
- A bridge that lets a function that does not wait block on a waiting one: rejected because that bridge is the wait inside compute that strands a join beneath it.
- Deriving waiting from the call graph (the continuation branch): rejected because a callee's body would change its callers' calling convention without any spelling the writer sees.
- Letting every call wait and repairing it in the scheduler (park-on-miss): rejected because a task that waits under a helping join strands every join beneath it, which the language can exclude instead.
- A resumable state machine per waiting function (the Rust `async` model): rejected because every local living across a wait moves into a compiler-built record whose proofs would have to be carried over from the written function, and the stackful shape measured no slower.
- A host thread per context: rejected because its stack and scheduling cost grow with the number of contexts, which is the design the echo references beat.
