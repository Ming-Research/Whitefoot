# Shared objects

Shared objects: mutable state several contexts reach through handles, the atomic statements that reach it, their guards, their progress and the handles' lifetime.

### decision shared-state-is-reached-in-atomic-statements
Mutable state several contexts reach is a shared object behind `Shared<T>` or `SharedRead<T>` handles, reached only in an `atomic` statement, which takes effect at one point.
- rejected: an agent proposed a transaction function that takes a callback (`transact`, `inspect`, `transact_when`); the owner rejected it, because the language has no closures, so every transaction needs a top-level function and an environment struct.
- rejected: an agent proposed an implicit scope that holds the object for the life of a reference formed through a handle; the owner rejected it, because whether two adjacent statements form one transaction could not then be read from the source.
- rejected: an agent proposed asynchronous messages to the object (actors, channels); the owner rejected it, because a sender that does not wait cannot know the order its messages take effect in, and a bounded channel whose send waits is a `Shared<Ring<T, n>>` with two guarded statements.
- rejected: an agent proposed shards each owned by one context and reached by messages; the owner rejected it, because every command pays two hand-offs, a command over keys of two shards needs a protocol between their owners, and the queue between contexts only moves the exclusion.
- rejected: an agent proposed transactions that read optimistically and retry; the owner rejected it, because a block runs once, moves values and writes its context's locals, so it cannot be repeated.
- rejected: an agent proposed spawns that take references to a state the starter owns, with no handle or count; the owner rejected it, because an effect row cannot state such sharing without forbidding a second spawn, and the state's identity is checked at run time either way.
- rejected: an agent proposed ordering transactions by stamps of the host completions that produced them; the owner rejected it, because it orders nothing a program can observe beyond the one point at which each statement takes effect.

### decision decision-cell-exposes-one-verdict
A one-shot `DecisionCell` exposes only an Open, Written or Killed verdict, through nonwaiting reads and competing claims, each its own shared effect point, with an absorbing winner.
- replaced: Refusing every atomic field and lock-free cell was on main; the owner changed it, because deciding a kill against an irrevocable step that holds shared keys needs the verdict seen before the statement ends, and a one-shot verdict exposes nothing of the held state.
- rejected: an agent proposed general atomic fields and general lock-free cells; the owner rejected it, because they expose interleavings of single reads and writes inside what the context meaning makes one atomic step.
- rejected: an agent proposed publishing the verdict only when the enclosing statement ends; the owner rejected it, because a kill must be decided against the irrevocable step while that step still holds the shared keys.
- rejected: an agent proposed releasing the control object early, after a statement's last lock is taken; the owner rejected it, because it needs early-release syntax and alias checks, decides only once, and makes the statement take the commit-time state before that release.
- rejected: an agent proposed short control sections in a block that take effect on their own; the owner rejected it, because they change the rule against nested statements and the order model, with the largest change to the specification and the runtime.

### decision decision-cell-is-an-owning-handle
A `DecisionCell` is an opaque owning handle that `decision_cell_share` shares, with a shared object's lifetime and ordinary value transfer into contexts.
- rejected: an agent proposed a source-addressable atomic field of a held object, or new permissions to share references to the cell; the owner rejected it, because a cell reached only through the held object would make a competing context wait for that object before it sees the verdict.

### decision guards-and-overlap-exclude-decision-cells
Guard admission and optional overlap refuse a callable whose declared boundary, with its contained types and callable parameters, can reach a `DecisionCell`.
- rejected: an agent proposed recognizing only direct prelude calls on a cell; the owner rejected it, because a helper could then hide a changing verdict from guard wake dependencies and from source-order preservation.
- rejected: an agent proposed inferring the cell's effects from implementation bodies; the owner rejected it, because module checking cannot depend on another module's body.

### decision atomic-statement-is-a-waiting-call
An atomic statement counts as a waiting call, and its guard and block contain no call to a waiting function and no atomic statement.
- replaced: Statements nested by kind, a statement on a map entry its enclosing statement held counting as no call, with an order between the kinds, was on main; the owner changed it, because one statement now holds several objects, so no statement contains another.
- replaced: A block that could run two statements on shared objects counted as a section was on main; the owner changed it, because no statement contains another, so a block holds every part it reaches until it ends and no statement falls between two of its accesses.
- rejected: an agent proposed taking some of a statement's parts later than others, in an order of kinds the language fixes, maps before objects; the owner rejected it, because it prescribes an order for one optimization, while the fields of the state a statement holds already give one.
- rejected: an agent proposed an order of store types inferred from every atomic statement of the program; the owner rejected it, because it needs every function body.
- rejected: an agent proposed lock levels declared in shared-store types; the owner rejected it, because type and object identity already order every lock, so a level the writer keeps orders nothing left open.
- rejected: an agent proposed ordering separate handles by the struct fields that contain them; the owner rejected it, because the same two handles can occur in structs in opposite orders.

### decision atomic-statement-names-its-targets
An atomic statement names each object it holds as a target, `&h`, `&h[key]` or `&h[keys]`; two handles may name one object, which the statement then holds once.
- replaced: One object per statement, with state that changes together kept in one object, was on main; the owner changed it, because unrelated state then shared a lock, and maps beside other fields needed lock units derived from the layout and grants in the header.
- rejected: an agent proposed a form that proves two handles distinct before a statement holds them together; the owner rejected it, because no static rule can decide it, since `shared_share` makes handles at run time, while merging the targets makes that case correct.
- rejected: an agent proposed a run-time failure when two targets name one object; the owner rejected it, because the language has no run-time failure.
- rejected: an agent proposed ordering every object by identity alone and taking every target when the statement begins; the owner rejected it, because a conditional logging tail would then hold its log object through a block that may never reach it.

### decision shared-handle-ends-by-release-or-close
A shared handle is `nocopy`, `shared_share` or `shared_read_share` makes another, and a handle ends by its release or by `shared_close`; the last to end gives the state to its close or releases it.
- replaced: Releasing the state with the last handle, so that only a droppable state could be shared, was on main; the owner changed it, because a linear state cannot be released implicitly, so a handle also ends in `shared_close`, which gives the state to the last handle to end.
- rejected: an agent proposed an owner context whose end releases the object; the owner rejected it, because contexts finish in an order the program does not know, so no one binding can own the state.

### decision linear-state-handle-is-linear
A handle to a linear state is itself linear and ends only in `shared_close`, a waiting call that returns the state to the last handle to end and `None` to every other.
- rejected: an agent proposed an owner handle whose waiting reclaim returns the state while the other handles stay droppable; the owner rejected it, because that reclaim waits forever while the owner itself holds another handle.
- rejected: an agent proposed a channel that hands linear values between contexts; the owner rejected it, because a channel cannot let contexts share one linear state.

### decision linear-state-has-no-own-handle
A linear state type has no part that is its own handle type, through fields, payloads, elements, `Box` content and handles' states, judged at each generic and each concrete instance.
- rejected: an agent proposed refusing every handle to a linear state inside a linear state; the owner rejected it, because that also refuses passing one object's handle to a running context through another object.

### decision turns-with-a-closable-resource-need-no-lease
Contexts that take turns with a resource that must be closed keep it in a linear shared state's one-slot window, moved out and put back by guarded atomic statements.
- rejected: an agent proposed a lexical lease block that lends the resource to a block and puts it back on every exit; the owner rejected it, because it adds only the automatic put back and a same-window contract, and that contract would refuse the file swap of a log rewrite.
- rejected: an agent proposed an explicit lease value that a return call consumes; the owner rejected it, because the resource is already a linear value another function can put back, so the extra type and protocol add only the same-window contract, which refuses the same swap.

### decision shared-state-is-in-no-effect-row
An object's state is a path of no effect row, so a function that changes it declares `waits` and reads the handle place it reaches the object through.
- rejected: an agent proposed an effect row entry that names the object; the owner rejected it, because the state belongs to no binding and no caller, and its changes take effect in the atomic order, an input of the execution like a host's effects.

### decision statement-keeps-its-object-live
An atomic statement counts as one of its object's handles until it completes, so the object stays live whatever its block does with the handle place it named.
- rejected: an agent proposed reading the target again when the block ends; the owner rejected it, because that would refuse a block that moves or replaces the handle it was reached through.
- rejected: an agent proposed that a close wait until no atomic statement on the object is executing before it returns the state; the owner rejected it, because the design tree and the compiler already count the statement as a handle, so only the specification lacked the sentence.

### decision guard-waits-for-its-condition
An optional `when` guard, a `Bool` expression that writes nothing, makes the statement take effect at a point where the guard holds, and the block may use the guard as a proved fact.
- rejected: an agent proposed a condition variable the writer signals; the owner rejected it, because blocking pops and subscriptions are waits on the shared state itself, and the block's operations need the guard's condition as their requirement.

### decision guard-reaches-no-self-changing-handle
A guard reaches no path that is or holds a self-changing host handle, which one PRE-2 table lists: `Clock`, `WallClock`, `MemoryMeter` and `HandleFactory`.
- rejected: an agent proposed accepting such a guard; the owner rejected it, because a waiting guard wakes only on a write to its targets' states, so a guard the host changes with no write may never wake, and the progress promise does not hold for it.
- rejected: an agent proposed refusing every guard call that receives a struct holding a clock; the owner rejected it, because a call whose row reads only another field of that struct reads no clock.
- rejected: an agent proposed a marker on the handles' declarations instead of a table; the owner rejected it, because the host modules' records are the specification's text, and a marker would add a declaration modifier for a handful of structs.
- rejected: an agent proposed keeping the handle factory readable in guards and waking a guard when the limit is raised; the owner rejected it, because another process can change the limit with no write that wakes a guard.

### decision begun-statement-eventually-takes-effect
A begun atomic statement with no guard, or whose guard is true from some point on, eventually takes effect while every context keeps reaching its completion or a wait.
- rejected: an agent proposed withholding weak fairness, so a statement whose guard stays true may lose the object to others forever; the owner rejected it, because a consumer and its producer must not finish only on a schedule the implementation happened to choose.
- rejected: an agent proposed strong fairness, so a statement whose guard is true only now and then takes effect; the owner rejected it, because such a guard can be missed on every schedule without the runtime seeing it, and a writer can make the guard stay true instead.
- rejected: an agent proposed starting, while a guard waits, every call around it that the overlap permission covers; the owner rejected it, because `spawn` states where a context starts and a call that is not spawned runs in order, so no start the writer cannot see is needed.

### decision statement-owns-the-whole-target-states
A statement's guard and block have exclusive access to the whole states its targets name, and an implementation may hold less, or share it, wherever outcomes stay the same.
- replaced: Naming what a statement holds by statement forms, an object, a map's state or an entry, was on main; the owner changed it, because a statement now owns its whole state, and which parts it holds is the implementation's choice, with the same meaning.
- replaced: Every atomic statement holding its object exclusively was on main; the owner changed it, because in the many-core `LRANGE` and `MSET` runs readers of one list and statements on different keys of one map took turns (data: `research/investigations/concurrent-map/DESIGN.md`).

### decision unused-target-is-refused
An atomic statement whose guard and block leave one of its targets unnamed is refused.
- replaced: A judgment of an unused state or table binding was on main; the owner changed it, because the header now names independent targets.
- rejected: an agent proposed accepting such a statement and holding what the target names anyway; the owner rejected it, because a mistyped or forgotten target would then pass silently, while every lock it takes still makes other statements wait.
- rejected: an agent proposed dropping a target that nothing uses; the owner rejected it, because what a statement holds would then differ from what its header says.

### decision shared-read-is-a-read-only-handle
`SharedRead<T>` is a retaining read-only handle that `shared_read` makes from a `Shared<T>`, shared by `shared_read_share`, with the targets, identity and lifetime of mutable handles.
- rejected: an agent proposed readonly fields alone; the owner rejected it, because they protect neither a whole replacement nor an exchange of the state.
- rejected: an agent proposed an atomic target only for cancellation; the owner rejected it, because a general read-only handle gives the needed authority through ordinary targets, with no admission rule for one host feature.
- rejected: an agent proposed that a cancellation interrupt a pending atomic statement; the owner rejected it, because that adds an outcome in which the block does not run, with its own result and edge rules, where a guard that reads the cancellation needs none.

### decision read-only-authority-follows-the-root
Read-only authority follows the resolved state root through aliases, descendants and call effects, while references keep their ordinary spelling.
- rejected: an agent proposed a new reference qualifier, or an exception by operation name; the owner rejected it, because a helper or alias must not win back write authority, and effect substitution already finds each affected path.