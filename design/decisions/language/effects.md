# Effects

Effect rows: what a function declares it reads and writes, how exact that declaration is, and what a call's writes do to its caller's facts.

### decision effect-rows-are-exact
An effect row is exact in both directions: an effect the body shows but the row does not declare, and one the row declares but the body does not show, are both errors.
- rejected: an agent proposed rows as upper bounds that a padded declaration may overstate; the owner rejected it, because a row a caller cannot read exactly is not a boundary, and entries the body does not show are a place to smuggle effects.

### decision rows-only-on-reference-parameters
An effect row is declared only on reference parameters and holds only `reads` and `writes`; a by-value argument's consumption or copy is recorded at the call from its own place.
- rejected: an agent proposed rows over owned parameters; the owner rejected it, because a row exists to tell a caller which of its storage a callee may reach, and the fate of an owned argument is already visible where it is written.
- rejected: an agent proposed separate external, blocks and traps effect categories; the owner rejected it, because system resources are ordinary state under ownership and scheduling belongs to lowering, so the categories described mechanisms rather than state.
- replaced: Effect roots recovered by following owned values through locals, results or replacements was on main; the owner changed it, because reference paths with explicit contracts give the whole call boundary, while following ancestry makes it depend on implementation bodies.
- rejected: an agent proposed a permission marker on a reference parameter, such as an exclusive or mutable mode, to state whether a callee writes through it; the owner rejected it, because the effect row already states that path by path, and two statements of one fact can disagree.

### decision effect-path-from-bare-parameter
An effect path starts at the bare name of a reference parameter and goes through fields, measures, window parts, payloads and index or range positions given as arguments, with no dereference step.
- replaced: A dereference step inside an effect path was on main; the owner changed it, because the root of every effect path is a reference parameter by rule, so the step told a reader nothing, and one meaning may have only one spelling.
- rejected: an agent proposed a path grammar of struct fields only; the owner rejected it, because it cannot name heap content or the one element a caller selected.
- rejected: an agent proposed signatures that carry index expressions; the owner rejected it, because a checker would have to evaluate them again in the callee's state, while an index evaluated once at the call is a fixed object the overlap judgment can compare.

### decision no-covered-row-entries
A row lists no entry that another of its entries covers, and a covered entry is an EFF-1 rejection at that entry, naming the entry that covers it.
- rejected: an agent proposed refusing only entries at or below a `writes` entry; the owner rejected it, because `reads(stats), reads(stats.count)` and `reads(stats)` would then be two admitted rows for one body, where FORM-1 asks for one spelling.
- rejected: an agent proposed refusing only the same-path pair `reads(p), writes(p)`; the owner rejected it, because `reads(p.x)` and `writes(p.x)` beside `writes(p)` are just as redundant, and once a call no longer compares one argument's always-overlapping entries nothing else refuses them.
- rejected: an agent proposed placing the rejection at the covering entry or at the whole row; the owner rejected it, because the covered entry is the one the writer deletes, so its own position with the entry that covers it locates the repair in one step.

### decision proofs-add-no-effects
Contracts and invariants are erased proof syntax and add no effect to a row.
- rejected: an agent proposed letting proof statements contribute effects; the owner rejected it, because a proof-only statement that changed a row would be observable.

### decision pure-promises-no-termination
`pure` says that a function has no state effects and promises nothing about termination.
- rejected: an agent proposed `pure` as a promise of totality; the owner rejected it, because the language has no termination checker, and a promise the checker cannot verify would be a trusted theorem.

### decision pure-licence-excludes-waiting-calls
A `pure` call's licence to be merged with or reordered past an equal call does not cover a waiting call.
- rejected: an agent proposed redefining `pure` to cover only state the caller can reach; the owner rejected it, because that needs a judgment of what a caller can reach, which no row expresses.
- rejected: an agent proposed excluding waiting calls only in the backend; the owner rejected it, because the specification would then contradict itself.

### decision allocation-has-no-effect-entry
Allocation and release carry no effect entry, no provider parameter and no effect category of their own.
- replaced: Provider parameters named in an `allocates` category, beside branded stores and an ambient heap, was on main; the owner changed it, because there is one heap from the trusted base and its addresses are not observable, so an allocation tells a caller nothing about storage it can reach.

### decision call-writes-kill-overlapping-facts
A callee's `writes`, with actual paths substituted, kill exactly the caller facts whose storage overlaps them and apply the validity rule to every live reference, before its `ensures` adds facts.
- rejected: an agent proposed killing by the syntax of an actual argument; the owner rejected it, because keeping a fact across a write to its storage is unsound.
- rejected: an agent proposed killing every fact about a written argument; the owner rejected it, because killing facts the write cannot reach destroys modular proofs.

### decision computed-index-writes-kill-length-facts
A write at a computed index is attributed to the window that holds it, so a row entry covering a window kills its callers' length facts even when the callee changes no length.
- rejected: an agent proposed attributing it to the window's filled slots when the row writes no measure; the owner rejected it, because it would keep only three of nine killed length facts, which a range argument keeps already (data: `research/investigations/layout-friction/DESIGN.md`).
- rejected: an agent proposed requiring every row to name the narrowest paths the body touches; the owner rejected it, because rows would grow and change with every body edit, while the kills it prevents are few and a writer can narrow a row where one matters.
- rejected: an agent proposed inferring the rows of module-private functions from their bodies; the owner rejected it, because a row is a boundary read without the body, and the measured kills do not pay for losing it (data: `research/investigations/layout-friction/DESIGN.md`).

### decision rows-name-accessible-paths
Rows in public declarations, function-kind formals and code outside the declaring module name only accessible paths; a public operation writing private state declares the nearest accessible path.
- rejected: an agent proposed rows that name private fields; the owner rejected it, because one visibility rule serves every role, and an interface that needs finer independence publishes the finer field.
- rejected: an agent proposed named footprint regions; the owner rejected it, because the published field states the precise path directly, and a named region would only keep client rows unchanged when a published representation changes, which the module goals do not require.

### decision proof-mentions-grant-no-access
Mentioning a field in a proof or effect annotation grants no runtime access and creates no scheduling edge.
- rejected: an agent proposed using proof mentions to justify a padded row; the owner rejected it, because proof support decides which facts stay valid, while body operations decide effects.