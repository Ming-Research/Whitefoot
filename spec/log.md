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
