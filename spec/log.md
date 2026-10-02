# Specification change log

Newest first. One entry per owner-approved change of the active
specification, `kernel-spec.md`: a `## <date> <title>` heading, `Rules:`
naming every rule added, changed or retired, `Owner-approved:` identifying the
owner's approval in the owner's words, and a concise `Summary:` of the change
and its selection ground. The form is the design tree log's, with `Rules:` in
place of `Nodes:`. The entry is written only after the owner approves, and
`make design-ready` requires a new approved entry whenever the active
specification changes; it cannot tell whether `Rules:` names every changed
rule. Earlier versions are the released archives beside this
file; git holds the rest of the history.

## 2026-10-02 v0.87: an allocation's size computed at run time

Rules: changed SCOPE-3, STOR-6, STOR-8, OP-9, OP-10, OP-13, ERR-4, DIAG-1, DIAG-2, ENT-1, ENT-2, MSR-4

Owner-approved: 2026-10-02, in the session, written in Chinese: after the handoff of PR #206's cards, Q33 B ("apart from 25, which I think needs research, I agree to all the others"); after the handoff of PR #209, which showed every rule change with its before and after behavior, the changes as shown ("for Q25 option A I need to see what the code actually looks like; the others agreed")

Summary: OP-9 computes a runtime-capacity construction's and `grow`'s byte size at run time with the checked arithmetic STOR-6 fixes, and a size the selected target cannot allocate is heap exhaustion [STOR-8]; no count carries a static obligation and every `u64` count is admitted, while the layout ceilings stay. STOR-6 holds a runtime-capacity shape's padded descriptor to the runtime-allocation maximum and states the run-time size check before any allocator call; STOR-8, OP-10 and OP-13 compute sizes as OP-9 states; SCOPE-3 drops the allocation-ceiling proof; the allocation-size family leaves ERR-4's classification, the DIAG-1 selection list, DIAG-2's retention list and lowering sentence, the ENT-1 schema and fragment lists, ENT-2's goal universe and MSR-4. Selected by the owner's ruling on direction 3D of the layout-friction investigation. The branch amended v0.86 before main released it; main's v0.86 is archived, and the same rule changes stand over it as v0.87.

## 2026-10-02 v0.86: callables and values in separate use classes

Rules: changed TYPE-6, DIAG-1

Owner-approved: 2026-10-02, in the session, written in Chinese: after the handoff of PR #206's cards, Q32 B ("apart from 25, which I think needs research, I agree to all the others"); after the handoff of PR #207, which showed every rule change with its before and after behavior, Q35 A and the changes as shown ("for Q25 option A I need to see what the code actually looks like; the others agreed")

Summary: TYPE-6 gives the lexical IDENT domain two use classes, callables (top-level `fn_decl`s, raw function-kind `gparam`s and PRE-1 functions) and values (its other entries), a module alias in both and any other alias in its target's class, and every other domain one class; two declarations compete when they have one spelling and one domain and either share a use class or both enter one module's inventory, and redeclaration, shadowing and GRAM-10 binder freshness hold only among competing declarations. DIAG-1's FN-9 result-candidate freshness tests compare only with live declarations a candidate competes with, and the GRAM-10 payload names the competing arm-entry origins. Selected by the owner's rulings on direction 2B of the layout-friction investigation and on the module-inventory clause.

## 2026-10-02 v0.85: shared maps and keyed atomic statements

Rules: changed PRE-1, REF-1, SET-1, SHARE-1, SHARE-2, SHARE-3, STOR-3, WAIT-2

Owner-approved: 2026-10-02, in the session, written in Chinese, after the handoffs of PR #202, which showed every rule change with its before and after behavior: cards #1 to #11 as recommended, #6, #7 and #9 after they were explained again ("the others agreed as recommended", then "9 agreed. The others agreed too"), the revised card #9 ("#9 approved") and the wording added to it after review ("confirmed"). The branch's amendment was written over v0.83 as v0.84; when main released its own v0.84, main's text was archived and the same rule changes were applied over it as v0.85, which the last handoff showed against main's v0.84.

Summary: SHARE-1 adds the shared map, a `SharedMap<V>` handle to state `SharedMapState<V>` holding an `Option<V>` entry for each byte-string key, with `shared_map_new`, `shared_map_share` and `shared_map_count`, its state and entries belonging to no binding and no context. SHARE-2 states an atomic statement's target and binding as one table of four forms, an object, a whole map, `m[k]` and `s^[k]` under a held map state; every atomic statement counts as a waiting call for PAR-1 and PAR-2, `s^[k]` as no call for WAIT-1; a map's or an entry's block admits object statements without a guard and a whole-map block its own `s^[k]`, and only an object statement outside every atomic block has a guard. SHARE-3 gives exclusive access to what a statement holds, a map's state including its entries, with one order for the statements on one key together with the map's whole-map statements. SET-1, REF-1, STOR-3 and WAIT-2 name a map's state and entries where they named an object's state. PRE-1 declares the opaque structs `SharedMap` and `SharedMapState` and the shared-object and shared-map functions in the declaration preorder beside main's range postconditions. Selection ground: the owner's rulings above; [the investigation](../research/investigations/concurrent-map/DESIGN.md#stage-b-the-language-surface-stated-before-building).

## 2026-10-02 v0.84 amended: an order-free range derivation

Rules: changed RANGE-3

Owner-approved: 2026-10-02, in the session, written in Chinese, after the handoff of PR #204: Q22 and Q23 as recommended ("22-24 approved")

Summary: Before v0.84 is released, RANGE-3's theory solves a set's equalities over the integers, their integer solutions written one to one over free integer parameters, and is contradictory when the equalities have no integer solution, a disequality's sides agree at every solution, or the inequalities written over the parameters and tightened have no rational solution; solving only equalities with a unit coefficient let the verdict depend on the order of solving. Its decision splits a branch on any open item, a definition without a case, a disequality the solutions do not fix, or two reads of one version neither at one index tuple nor held apart at a differing position, into all of the item's cases, so the verdict is the same in every split order; the fixed order left a pair kept apart at two positions without the orientation a one-position pair got, and never settled a pair at strided indices. Selection ground: deduction from the witnesses in `research/investigations/unique-keys/POINTWISE.md`, "An order-free derivation", and the cases `range3-neg-strided-reads`, `range3-pos-reads-apart-at-two-positions` and `range3-neg-reads-apart-in-one-arm`.

## 2026-10-02 v0.84: range facts, range postconditions and the apart certificate

Rules: added RANGE-1, RANGE-2, RANGE-3, RANGE-4, RANGE-5; changed FORM-2, GRAM-2, GRAM-4, GRAM-5, FN-9, INV-1, PAR-2, PRE-1

Owner-approved: 2026-10-01 and 2026-10-02, in the session, written in Chinese: before the work, Q1, Q2 and Q3 as recommended and Q4 with a change, both steps done and the second needing no further confirmation ("Q4, two steps, but do not confirm the second step"; "agree to the rest"); after the first handoff of PR #203, Q5 to Q15 as recommended ("Q5 to Q15 all approved as recommended"); then Q16 B, which reopened Q11 and Q12 for range postconditions ("Q16 choose B, hand off again when done"); and after the second handoff, which showed each card, the other tree edits and every rule change against v0.83 with its before and after behavior, Q17 to Q21 as recommended, with the proof-cost work in a pull request of its own after this one ("Q17 to Q21 all approved as recommended; the proof-cost optimization goes after 203, in a separate PR").

Summary: RANGE-1 forms a range clause, `forall NAME(x in a..b, ...) when guards: conclusions` with one or two bound variables, admitted as a function's requirement or postcondition and a loop's header invariant (GRAM-2, GRAM-4, GRAM-5, INV-1); a postcondition names result ordinals, routed through FN-9's admission, integer parameters at entry and reference parameters' storage at the exit, never a consumed parameter, and a generic clause states nothing at a non-integer instance. RANGE-2 defines the forward walk after ordinary entailment: storage versions, the variant each location is known to hold, the three fact sources (requirements, loop range invariants, and a callee's postconditions and range-term FN-9 relations after a call), exact integer evaluation, copies as new storage, joins, loop headers that forget what any iteration can write and the state after a loop as the join of its exits. RANGE-3 owes a fact at calls, loop entries, backedges and the exits that select a postcondition, requires an inhabited instance to select each postcondition, and proves each by a fixed derivation bounded only by the problem's size; RANGE-4 admits written instances in a certificate; RANGE-5 defines the `apart(i, j)` certificate on a counted loop, whose failure is an error, and PAR-2 admits the writes a holding certificate placed as certified elements; FORM-2 renders the certificate. FN-9 hands a range postcondition to RANGE-1 and RANGE-3, and PRE-1 states the fill constructors' contents as range postconditions. Selection ground: the owner's rulings above; [the derivation and measurements](../research/investigations/unique-keys/POINTWISE.md).

## 2026-09-30 v0.83: a deadline completes the wait, and each clock and sync fact is stated once

Rules: changed PRE-2

Owner-approved: 2026-09-30, the owner approved decision cards 6 and 9 of the completion review's findings as recommended ("all agreed", written in Chinese).

Summary: PRE-2 stated only when `DeadlinePassed` may appear, which a call that never returned also satisfied; it now states that an outcome the host has not produced before the clock reaches the deadline is produced then as `DeadlinePassed` with nothing transferred, so the call completes as [WAIT-2] completes every waiting call whose outcome has been produced, and that `DeadlinePassed` arises in no other way. The ordering of two `now` reads through one clock, the completion of `sleep_until` and the promise of `sync_file` were each stated in PRE-2's prose and again in the record's doc string; the prose keeps what the doc strings do not say, and its sentence on `sync_file`, which lacked a verb, now states only what lies outside this specification. Selection ground: a deadline that bounds nothing is no bound, and each normative fact is stated once.

## 2026-09-30 v0.83: clocks, deadlines and append-only files

Rules: changed PRE-2, TYPE-2

Owner-approved: 2026-09-30, in the session, written in Chinese: the rulings Q23 B, Q24 A, Q25 D and Q26 A, then Q27 to Q30 as recommended ("Q27 agreed. Q28 agreed Q29 agreed Q30 agreed"), and the handoff that showed each rule change against v0.82 with its before and after behavior ("all approved").

Summary: PRE-2 adds a sixth host module, `std::time`, with a monotonic `Clock`, a calendar `WallClock`, an `Instant` that only host functions form, `now`, total `Instant` arithmetic, `sleep_until` and `unix_nanoseconds`; it gives `read_next`, `write_once`, `tcp_accept`, `tcp_connect`, `receive_next` and `send_once` a last parameter `deadline: Option<Instant>`, whose passing is `IoError::DeadlinePassed()` only once the clock has reached it and only when nothing was transferred; it splits `Inputs.cwd` into a `Directory` of a read half and a write half, adds `clock` and `wall_clock` to `Inputs`, and adds appending, syncing and closing a file below the write half, with `sync_file` promising only the hand-off to the host's durability mechanism. TYPE-2 lets a host function form every opaque struct a host module declares, not only its handles, and gives an opaque struct a host module declares with fields, `Instant` alone, the representation and capabilities its fields give it. Selection ground: the owner's rulings above; [the design](../research/investigations/io-model/TIME-AND-FILES.md).

## 2026-09-30 v0.82: shared objects, type invariants and spawn

Rules: added SHARE-1, SHARE-2, SHARE-3, TYPE-11, WAIT-3; retired PAR-4; changed CALL-6, CAP-1, DIAG-1, ENT-2, ENT-3, FN-9, FORM-2, GIVE-1, GRAM-2, GRAM-4, GRAM-5, OP-1, PRE-1, REF-1, SET-1, STOR-3, TYPE-6, WAIT-1, WAIT-2

Owner-approved: 2026-09-29 and 2026-09-30, in the session, all written in Chinese: the shared-object statement form ("this is much cleaner, go with it") and its revisions ("all decisions approved, the spec revisions approved too"); the type invariant at the module boundary ("card 7 approved, build it at the module boundary") with an explicit binder ("agreed on the explicit binder, it is indeed better"); cards 1 to 3 of the TYPE-11 review ("cards 1, 2 and 3 all approved, do as recommended"); Q9 to Q15 on the concurrency model, each as option A; cards 1 to 6 of the spawn handoff with its rule-by-rule revisions ("#1 approved ... 6 approved. Then approve all the others too"); and on 2026-09-30 the final handoff ("all approved"), which showed every rule change against v0.81 and the three made after that ruling: the retitle to v0.82 over main's v0.81, the removal of a WAIT-3 sentence that restated WAIT-1's definition, and PRE-1's sentence merged with main's `Segments`.

Summary: A `Shared<T>` handle names one object whose state is reached only in `atomic s = &h (when guard)? { ... }`, which takes effect at one point where its guard holds and cannot wait inside (SHARE-1 to SHARE-3, with GRAM-4, PRE-1, REF-1, SET-1, STOR-3, TYPE-6, GIVE-1, ENT-2, ENT-3 and OP-1 admitting the statement and its binder). A struct may declare difference-bound invariants over its own fields, owed at construction and every hand-off and assumed at every function's entry (TYPE-11, with GRAM-2, GRAM-4 and FORM-2), and an unrouted postcondition is owed at every propagated error exit (FN-9, CALL-6, DIAG-1). An execution consists of contexts: the entry runs in the root context and each `spawn` of a waiting call starts one that runs concurrently with its starter, taking only value parameters and joined at its activation's exit or, for a let, where its block first names the binding or leaves; a call that is not spawned runs in order; while every context keeps reaching waits, a ready context proceeds and a statement whose guard stays true takes effect, and a program that can take no step may be stopped with a report (WAIT-1 to WAIT-3, GRAM-5, CAP-1). `mustpar` and PAR-4 are retired. Selection ground: the owner's rulings above; the waiting rules make nested acquisition, holding an object across a wait and lock-order deadlock unrepresentable; single ownership and non-escaping references make a struct's value unobservable between two field writes, so only hand-offs owe its invariant; and concurrent I/O has programs with no sequential schedule, so concurrency is a choice the writer makes visibly. Evidence: [shared objects](../research/investigations/io-model/SHARED.md) and [the concurrency model](../research/investigations/io-model/CONCURRENCY-MODEL.md).

## 2026-09-29 v0.81: element-subtree loop accesses and Segments

Rules: PAR-2, TYPE-2, TYPE-8, TYPE-9, STOR-1, STOR-6, STOR-8, OP-4, OP-9, OP-13, REF-4, MSR-1, PRE-1, and TYPE-9's release-graph paragraph

Owner-approved: 2026-09-29, the owner approved PR #186's handoff, which showed every rule change with its before and after behavior and decision cards Q1 to Q4, writing in Chinese "all agreed", after selecting element-subtree option C and the segmented-storage design earlier in the session.

Summary: PAR-2's element family admits every access at or below one proved affine subscript of an Array, a Slots, a range's run or a Segments. TYPE-9 adds Segments<T>, a Box-only run of segments; OP-4 admits its subscript only as REF-4's &s[i], REF-4 forms &s[i] and &s.all, MSR-1 gives it len, and PRE-1 declares it and box_segments_filled. OP-13 and STOR-8 state that the construction returns None exactly when stride_ceiling(T) * total + 8 * count exceeds 2^62, and STOR-6 qualifies the target for the largest admitted block. The remaining rules add Segments to their shape lists. Grounds: research/investigations/segmented-storage/DESIGN.md.
