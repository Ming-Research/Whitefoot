# Diagnostic repairs

The words of the repairs a rejection carries: where they live, which routes each kind of failed goal offers, the cause-specific repairs, and how each repair is pinned.

### decision repair-words-live-in-one-module
The words of every repair are the compiler's and live in one module; each alternative names one source change at the rejected construct and the instance from the payload.
- rejected: an agent proposed one sentence per rule for every position, disposition and instance; the owner rejected it, because the probed repairs that fitted the case in hand compiled in one round (data: `research/investigations/repair-wording/DESIGN.md`).

### decision repair-routes-follow-the-goal
A refuted goal's repair changes what reaches the construct; an unproved goal's repair offers the routes what it reads admits: a requirement, an invariant, a callee's `ensures`, a guard or a `let`.
- rejected: an agent proposed one repair, to establish the fact, for refuted and unproved goals alike; the owner rejected it, because every probed refuted goal given it was rejected again elsewhere or compiled to dead code, while unproved goals given a requirement compiled in one round.
- rejected: an agent proposed offering a requirement whenever the goal reads only parameters; the owner rejected it, because a parameter the body writes or consumes before the goal no longer holds its entry value there.
- rejected: an agent proposed deciding the requirement route from every write in the body; the owner rejected it, because a parameter consumed by the call that asks the goal, as `free_empty` consumes its window, would lose the one route that works for it.
- rejected: an agent proposed deciding the requirement route from every write applied before the judgment; the owner rejected it, because a write in one arm would remove the route from a sibling arm walked later, so swapping the arms would change the repair.
- rejected: an agent proposed offering a callee's `ensures` for every goal over computed values; the owner rejected it, because with no call result among the values the goal reads, the route can never be carried out.
- replaced: Treating the `len` of a range formed at the call as a value only its occurrence identifies was on main; the owner changed it, because [REF-4] gives that length the difference of its endpoints, so the repair named a missing name where a bound on that difference was missing.

### decision out-of-order-requirement-is-moved
For a subscript bound formed by a requirement, the repair moves an equivalent written requirement ahead of the one that forms the place, and adds one only when none is written.
- replaced: Suggesting a new requirement even when an equivalent one was written after the one that needs it was on main; the owner changed it, because requirements take effect in written order [FN-8] and restating one hides the ordering fault.

### decision repairs-pinned-with-applied-sources
Every repair is pinned by its rejected source and a source for each alternative carried out as written, each accepted with no judgment succeeding only in a contradictory state.
- rejected: an agent proposed pinning only the repair sentence; the owner rejected it, because the EFF-2 add-and-remove repair stayed pinned while applying it produced a row every call rejects [EFF-5].
- rejected: an agent proposed running each repaired program for an exit status; the owner rejected it, because the contradiction check catches the dead code a guard around a refuted goal compiles to without running the program.

### decision every-rejection-kind-has-a-repair-pair
The compiler lists each rejection kind it can print, and a gate test fails for a listed kind no pair reaches, an unlisted kind a pair reaches, or a printed repair line no pair pins.
- replaced: A note in the pairs' header asking each new repair to add a pair was on main; the owner changed it, because nothing failed when a kind came with no pair or a pair pinned other fields and left the repair line unchecked.

### decision callee-failed-proof-named-by-replay
A caller's failed proof names the callee, its withheld summary and its own failed obligation when a private replay of the caller after the rejection discharges the goal with that summary.
- rejected: an agent proposed naming the callee only where no replay is needed, when its result is returned or bound by a `let`; the owner rejected it, because Firn's two real cases fail elsewhere, and a discarded replay that adds one hypothesis is not the re-run [DIAG-2] forbids.

### decision lost-subscript-facts-name-the-writing-call
When a subscript bound lost its facts to calls on paths to it and putting the removed cells back on a copy discharges it, the repair names each call and offers the callee's `ensures` and its row.
- replaced: Offering only the ordinary routes, with no call named, was on main; the owner changed it, because writers then guarded such accesses; in Snowghost's layout nine guards were needed only because a call's row removed the fact (data: `research/investigations/layout-friction/DESIGN.md`).
- rejected: an agent proposed naming every earlier call whose row covers the measure, found from the source; the owner rejected it, because the bound may never have held before the call, as when the index is read from storage, so narrowing that row would change nothing.
- rejected: an agent proposed walking the function again with the calls' kills of the measure suppressed; the owner rejected it, because putting the removed cells back on a copy of the failing state asks the same question at the one failing site without a second walk.
- rejected: an agent proposed offering a narrower row for every named call; the owner rejected it, because a callee whose body writes the window holding the length covers it under every row EFF-2 admits [SET-1], so that alternative fails when carried out [DIAG-1].

### decision counted-loop-binder-bound-gets-range-forms
An unproved subscript bound formed by a counted loop's header invariant at the loop's own binder is repaired with a range invariant over the processed elements and a range requirement over the run.
- replaced: The generic request to prove the bound on every incoming instance was on main; the owner changed it, because the relation names the element at the binder, which need not exist at the base of an empty run or after the last backedge, so it cannot be carried out.
- rejected: an agent proposed offering the range forms only at the final header; the owner rejected it, because a failure at the base or at a backedge would not reach them.

### decision lost-header-relation-named-at-the-join
A header relation withheld at a join leaves an explanation-only record, and a later unproved goal names that loss when restoring the relation on a private copy discharges it without contradiction.
- replaced: Offering the ordinary routes, such as a guard, for a goal whose header relation was lost was on main; the owner changed it, because a guard leaves the loop's induction obligation unproved, while the repair must direct the failing path to keep the header relation.

### decision rewritten-value-parameter-named
When an indexed write uses a value parameter rewritten before it, the repair names the assignment and offers a fresh local for the new value, or moving the access before it where that is safe.
- replaced: Suggesting a wider row that covers every access the body makes was on main; the owner changed it, because widening the row makes callers lose their facts on the array's other elements and hides the real cause.

### decision missing-qualified-name-lists-candidates
A missing qualified name lists the accessible declarations of the named module in the role the grammar selects, sorted by spelling and at most eight.
- rejected: an agent proposed an edit-distance heuristic; the owner rejected it, because it has no ground in the specification, while a list of real names lets the writer pick one in one repair round.

### decision define-hoist-only-in-definitions
A GRAM-9 or Boolean-conjunction repair that splits a contract expression into earlier `define` bindings is offered only inside a definition initializer whose operations FN-8 admits.
- rejected: an agent proposed offering the hoist in every contract block; the owner rejected it, because definitions precede every clause [GRAM-2] and cannot name result data, routed payloads or range binders, so the hoist would fail to compile in requires and ensures clauses.

### decision reserved-spelling-repair
A name written with a reserved spelling is repaired by one sentence that says it is reserved, what takes it, derived from the grammar and operation tables, and a fresh spelling ending in `_value`.
- replaced: Giving only the expected-token list, or the reserved set's class and ordinal, was on main; the owner changed it, because neither said why `copy` or `checked` is not a name.
- rejected: an agent proposed a written description for each reserved spelling; the owner rejected it, because a second list beside the grammar table drifts silently when an amendment moves a word.
- rejected: an agent proposed offering the rename at every refusal of a fixed grammar word; the owner rejected it, because where the word is a marker, as `musttail` in `outer(musttail 0_u32)`, no name derives in its place, so the rename fails when carried out [DIAG-1].

### decision layout-rejection-lists-every-gap
A FORM-2 rejection still reports the first differing gap, and its repair lists every differing gap, the first twenty with line, nearby terminals and the bytes to write, then the count of the rest.
- rejected: an agent proposed a repair that points to the formatting request; the owner rejected it, because every host reads the library's rejection while the request is one command-line option, and a rewrite leaves the writer unaware of what changed.
- rejected: an agent proposed a line diff of the source against its rendering; the owner rejected it, because it recomputes the gaps the tree already holds with an algorithm that must settle end-of-file and blank-line ambiguities, and could disagree with the audit.
- rejected: an agent proposed carrying the whole canonical rendering in the repair; the owner rejected it, because it grows with the file and buries the rejection, the writer can miscopy a full rewrite, and a rendering cut to a bound returns to one gap per compile.
