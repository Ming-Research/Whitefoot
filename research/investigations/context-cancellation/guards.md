# Cancellation observed by atomic guards

A study for the PR stacked on Whitefoot #296 (cross-context cancellation): how an atomic statement's guard observes a cancellation, as the owner's direction A requires and #296 deferred with the owner's agreement. The study was written against #296's head b3f742f6a. On 2026-10-09 the owner selected option A on board card `firn-cancel-guard-shape`: general `SharedRead<T>`, a retained `CancelState` view and waiting `cancel_fire`. The specification text and implementation still require review and CI evidence.

**1. Guards may read more than their targets, but only target state currently participates in guard wakeups.**

SHARE-2 gives a guard the ordinary `if` condition judgment: it must produce an owned `Bool`, write no path, contain no waiting call, and contain no atomic statement. It does **not** restrict every read to an atomic target.

Consequently, a guard may read:

- Ordinary in-scope values and valid reference paths, including locals, parameters, fields, and admitted subscripts.
- State reached through its target bindings: ordinary shared state, map entries, key-set selections, and entries reached through a whole-map binding.
- Results of nonwaiting calls whose instantiated footprints write nothing. Read-only helper functions are allowed; consuming an owned argument is a write and is not allowed.

The target places themselves are presently restricted to `Shared<T>` handles and the specified map selections. `CancelWatch` is a fieldless, opaque, `nodrop` host handle, so it is neither an admitted target nor a value with a readable `fired` field. Putting it inside `Shared` is also unavailable: `Shared<T>` requires `T: drop`.

These distinctions follow from [SHARE-1–3 and WAIT-2](https://github.com/Ming-Research/Whitefoot/blob/b3f742f6a795b6810f1bc3289a096001136e5013/spec/kernel-spec.md#L2227-L2295) and the [current time interface](https://github.com/Ming-Research/Whitefoot/blob/b3f742f6a795b6810f1bc3289a096001136e5013/lib/std/time/module.wfm).

The runtime does not discover arbitrary changing values read by a guard. Lowering collects the guard’s references to atomic target bindings, maps those to target groups, acquires the required groups, and evaluates the guard. On false, it registers watches while those units remain held, releases them, and parks. A writer subsequently wakes those watches. Registration before release prevents the change from falling between observation and registration; `WF_GUARD_WOKEN` handles a wake arriving before parking. See [guard lowering](https://github.com/Ming-Research/Whitefoot/blob/b3f742f6a795b6810f1bc3289a096001136e5013/compiler/src/lowering/builder/atomic.rs#L629-L714) and [guard-watch machinery](https://github.com/Ming-Research/Whitefoot/blob/b3f742f6a795b6810f1bc3289a096001136e5013/compiler/src/backend/completion/bridge.c#L2439-L2655).

Today’s cancellation allocation contains only a reference count and an atomic `fired` word. Firing notifies drivers, whose cancellation scans visit **host-wait records**. A parked atomic guard is not one of those records. Waking its driver therefore does not make the guard runnable.

The candidate forms have the following consequences.

**(a) Make `CancelWatch` an atomic target, with a query or field.**

This follows the owner’s intended composition most directly:

```wf
atomic s = &state, c = &watch when bor(s^.ready, c^.fired) {
  // Handle readiness or cancellation.
}
```

However, it needs an explicit definition of what `c` denotes. An ordinary reference to the watch’s handle storage is not a reference to its independently shared cancellation state.

A coherent version would introduce a protected cancellation-state target:

- SHARE-2 admits the new handle kind and defines its binding, identity, lifetime, aliasing, and footprint.
- The binding observes the state held by the statement, including when passed to read-only helpers.
- Its state cannot be written, exchanged, or consumed through that binding.
- Firing performs the state transition in the same atomic ordering as statements observing it.
- WAIT-2’s progress promise includes that transition making a guard persistently true.

A plain `cancel_fired(watch: &CancelWatch) -> Bool reads(watch)` does not establish any of those properties. Its row would make it eligible for a guard, but would not register the cancellation dependency. Making its meaning depend on whether that particular handle happened to be held would introduce an additional contextual call rule. A field, or a query taking a distinct held-state reference, gives a clearer boundary.

For a locked implementation, cancellation becomes a guard-watch unit. A false guard registers on that unit before releasing it; the first firing writes it and wakes its guard watchers. That same transition also performs #296’s driver notifications. **Host waits should retain find-at-fire; they need not join the cancellation unit’s guard list.**

This form can leave statements without cancellation targets unchanged. Statements using it pay for another held unit and, when parking, another watch registration. It also enlarges or supplements cancellation’s current runtime representation.

There are two substantial design problems:

1. A host-specific target admission needs justification against the existing decision that external interaction uses ordinary language abstractions.
2. A locked firing cannot remain silently callable inside an atomic block; this is discussed below.

**(b) Use a program-owned `Shared<Stop>` and write its flag.**

The owner’s example already works if `stop` is an ordinary shared object containing `fired: Bool`. It needs no new language or runtime mechanism. Writing that state wakes guards under the existing protocol.

The limitation is semantic, not mechanical: that flag and #296’s cancellation state are two independent states. The program must both update the flag and call `cancel_fire`. There is no existing rule making those two actions one transition. Ordering them explicitly still leaves an interval in which only one has changed.

This is appropriate when a program intentionally distinguishes “shutdown requested” from “end these host waits.” It does not complete direction A’s single cancellation object observed by both kinds of wait. Treating it as the implementation would conceal the missing integration.

Ordinary shared-state ordering applies to the flag. It does not publish completion of the separate host-cancellation action. Statements that omit the flag pay nothing; participating statements pay normal shared-object costs.

**(c) Add `when ... or cancelled(w)`.**

This spelling has two different possible meanings, which must not be conflated.

- **The body executes when either condition holds.** Then the block is entitled only to the disjunction, not to the original guard. This needs the same coherent observation and wakeup semantics as (a); new syntax does not solve them.
- **Cancellation ends the statement without executing its body.** Then this is a new outcome/control form. It needs a way to distinguish successful execution from cancellation, rules for initialized results and outgoing edges, and a definition of the readiness/cancellation race.

The second meaning could keep firing nonwaiting: cancellation would cancel a pending acquisition rather than mutate a state held throughout the body. But it would change the selected meaning of guarded atomic statements and require a cancellation-aware acquisition protocol. It is a different design from “the guard reads fired.”

Either form needs new grammar, effect accounting for the watch operand, dependency registration, and race semantics. It can impose no cancellation bookkeeping on statements without the clause. A deadline clause could compose naturally with the second meaning, but would also need its own outcome and race rules.

**(d) Expose the same state through a general read-only shared view — recommended.**

A possible spelling, proposed here rather than existing syntax, is:

```wf
let stop = std::time::cancel_state(watch: &watch);
atomic s = &state, c = &stop when bor(s^.ready, c^.fired) {
  // Handle readiness or cancellation.
}
```

Here `stop` has a general handle type such as `SharedRead<CancelState>`. It retains and observes the **same object** that the source fires. It is not a copied flag maintained by another context.

The new general rule would permit atomic targets through read-only shared handles. Their references retain the existing `&T` spelling and ordinary effect rows, but their resolved paths grant no write authority. This restriction must survive aliases, projections, and calls. Mutable and read-only handles of the same object must still be recognized as potentially aliasing targets and acquire that object once.

This avoids teaching atomic target admission about one particular host nominal. It also makes the field spelling possible without introducing a raw, changing query outside the atomic state model.

An ordinary `Shared<CancelState>` getter is **not** sufficient. A `readonly` field prohibits writing through that field; it does not prohibit replacing its whole owner. A copy state can be replaced by whole-value `set`. A noncopy state can be exchanged:

```wf
atomic a = &first_view, b = &second_view {
  swap(first: a, second: b);
}
```

If those ordinary mutable views expose a fired and an unfired cancellation state, this exchanges their flags and defeats permanent firing. `opaque`, private fields, and `nodrop` do not prevent that exchange. This follows from [TYPE-2](https://github.com/Ming-Research/Whitefoot/blob/b3f742f6a795b6810f1bc3289a096001136e5013/spec/kernel-spec.md#L415-L416) and [OP-11](https://github.com/Ming-Research/Whitefoot/blob/b3f742f6a795b6810f1bc3289a096001136e5013/spec/kernel-spec.md#L1204-L1211). The needed property is a **read-only state path**, including its root.

**A query without a target is a further alternative, but not an adequate small extension.** Dynamically registering dependencies whenever a query executes could wake the guard, including through helpers. It would add a separate dependency-discovery mechanism and still require a coherent snapshot protocol. An acquire load and register-then-recheck protocol prevent lost wakes; they do not establish SHARE-3’s single-point meaning.

**2. The recommendation preserves atomic-state semantics, but firing must change too.**

The existing decisions favor explicit targets, one atomic point, ordinary ownership and effect paths, and automatic waking on state changes. They reject condition variables the writer signals, optimistic retry of an executed block, and exposed interleavings of individual atomic fields. The system-interface decisions also favor ordinary abstractions over host-specific semantic classifications. These grounds favor (d), while retaining (a)’s intended semantics. See [shared-object decisions](https://github.com/Ming-Research/Whitefoot/blob/b3f742f6a795b6810f1bc3289a096001136e5013/design/language/waiting/shared-objects.md), [system-interface decisions](https://github.com/Ming-Research/Whitefoot/blob/b3f742f6a795b6810f1bc3289a096001136e5013/design/language/system-interface.md), and the [constitution’s external-interaction rule](https://github.com/Ming-Research/Whitefoot/blob/b3f742f6a795b6810f1bc3289a096001136e5013/docs/constitution.md#L68-L75).

The second required choice is the firing operation.

Currently, `cancel_fire` is nonwaiting and writes its source. Therefore, a guard cannot call it, but an atomic **block** can. With a cancellation target that holds the state, the following proposed fragment exposes the problem:

```wf
atomic c = &stop {
  let before = c^.fired;
  std::time::cancel_fire(source: &source);
  let after = c^.fired;
  // Use before and after.
}
```

If firing takes the held cancellation lock, the context waits on itself. If it modifies the state without exclusion, `before` and `after` can differ inside one atomic statement. Different source handles can conceal this identity, and two blocks can introduce opposing hidden lock acquisitions.

The straightforward resolution is:

```wf
public fn cancel_fire(source: &CancelSource) -> result: unit
  writes(source) waits;
```

The existing SHARE-2 rule then excludes it, and wrappers that call it, from atomic guards and blocks. Firing acquires only its cancellation state outside any atomic block, performs the transition, and releases it. This preserves the decision that an atomic statement’s locks are named by its targets and obey one acquisition order.

Keeping fire nonwaiting is possible only with a different, justified protocol. Caching a cancellation bit is not enough: lowering acquires some later target groups lazily, so the cached cancellation value and a subsequently read target value might never have coexisted. A sound lock-free alternative would need a larger snapshot/validation design, not merely an atomic load. The [current locking decisions](https://github.com/Ming-Research/Whitefoot/blob/b3f742f6a795b6810f1bc3289a096001136e5013/design/compiler/waiting-contexts/state-locks.md) make this distinction material.

### Decision for the owner

**Should cancellation join atomic observation through a read-only shared view?**

- **Background.** Direction A requires the guard and host waits to observe one permanent transition. Existing mutable shared handles expose too much authority; existing nonwaiting fire can introduce hidden lock cycles.
- **Option A — recommended.** Add a general read-only shared view and make firing waiting. Preserve ordinary guard evaluation, state ordering, and find-at-fire host cancellation. The costs are a new shared-handle capability, changed acceptance of nonwaiting fire callers, and contention between firing and statements holding the cancellation state.
- **Option B.** Introduce a protected cancellation-specific target. It can provide the same behavior, but needs an explicit decision for the new language primitive and its relationship to the ordinary-object principle.
- **Option C.** Define cancellation as an independent escape from a pending atomic statement. This can preserve nonwaiting fire, but requires new control/outcome semantics and revises the owner’s guard-state direction.

**Confidence 4/5** that the protection, ordering, and firing issues must be resolved; **3/5** that a general read-only shared view is the best public shape. The remaining uncertainty is the abstraction’s broader fit and measured contention, not whether a raw query alone suffices.

**Ordering should guarantee observation of the transition, not a global host-effects fence.**

For the recommended form:

- A statement observing `fired = True` is ordered after the firing transition in the shared-state order.
- Shared-state updates completed before that firing participate in the ordinary ordering; later reads cannot select an earlier state merely because cancellation used another handle.
- The cancellation state stays stable throughout the observing atomic statement.
- Firing does not mean every cancelled host call has returned, every context has joined, or later cleanup has completed.
- Observing firing does not independently order unrelated file, network, clock, or other host effects. PRE-2 explicitly leaves related handles under HOST-1’s footprint ordering. The current C `acq_rel` exchange is not a language-level promise to publish every earlier host effect.

The PRE-2 cancellation paragraph therefore needs a precise amendment distinguishing shared-state transition ordering from host-effect ordering, rather than silently strengthening all of HOST-1. See [PRE-2](https://github.com/Ming-Research/Whitefoot/blob/b3f742f6a795b6810f1bc3289a096001136e5013/spec/kernel-spec.md#L2528-L2529).

**All state-based forms can compose with deadlines through a timer context.**

A spawned context can `sleep_until` a deadline and then fire a separate cancellation source or write a shared deadline flag. The atomic guard includes that state alongside work readiness and shutdown cancellation.

There are three necessary qualifications:

- Use separate states when the body must distinguish timeout from shutdown; combining them into one source deliberately loses that distinction.
- The timer needs a disarm watch. Otherwise an operation that finishes early can still wait until the deadline at its mandatory join.
- This gives eventual execution after the timer makes the guard true, under WAIT-2. It does not give immediate execution at the physical deadline or a cancellation latency bound.

No direct clock read solves the wakeup problem. `now` writes its clock and is inadmissible in a guard; comparing a previously captured `Instant` creates no changing dependency.

The clause form could instead provide a direct timeout outcome, but that is the separate bounded-atomic-wait design already recorded in [the maintained TODO](https://github.com/Ming-Research/Whitefoot/blob/b3f742f6a795b6810f1bc3289a096001136e5013/docs/todo.md#L3482-L3494).

**3. The stacked PR needs a small semantic core, followed by explicit compiler and runtime support.**

For the recommended form, the following is the essential proposed specification text. Names are provisional.

- **SHARE-1 / PRE-1 — read-only shared handles.**

  > `SharedRead<T>` retains the same shared object as its originating handle. Sharing or releasing a read-only handle follows the object’s ordinary lifetime rules. A read-only handle grants no means of obtaining a mutable handle.

  Provide ordinary conversion from `Shared<T>` and sharing of `SharedRead<T>`, so this is a general facility.

- **SHARE-2 — targets and authority.**

  > A target may name a read-only shared handle with the corresponding shared target selection. Its binding denotes the selected state. Every resolved path formed through that binding is read-only: it admits reads and read-only calls, and refuses writes, exchanges, and consumption. The restriction follows aliases and selected descendants.

  Specify the rejection rule and location. Retain capture-at-entry, target lifetime, unused-binding checks, potential aliasing by state type, and the existing statement footprint.

- **PRE-2 — cancellation’s state view.**

  > `cancel_state` returns a read-only shared handle retaining its watch’s cancellation state. That state exposes `fired: Bool`, initially false and permanently true after firing. A view of `cancel_never` remains false. Closing any source or watch neither fires nor clears a retained state; views also retain it.

  A possible declaration is `cancel_state(watch: &CancelWatch) -> result: SharedRead<CancelState> reads(watch)`, with a public readable `fired` field on `CancelState`. Update PRE-2’s current description of fielded host opaques as “`Instant` alone.”

- **PRE-2 / SHARE-3 / WAIT-2 — transition and progress.**

  > `cancel_fire` is waiting. It performs the cancellation-state transition as an atomic state update, ordered with statements observing that state. Repeated firings leave it true. The transition supplies both guard-state observation and the existing host-wait cancellation behavior.

  State this once, with cross-references elsewhere. A guard made permanently true by firing receives the existing conditional progress guarantee.

- **Proof rules.**

  Preserve the ordinary guard judgment. `bor(ready, fired)` establishes the disjunction; it must not authorize an operation requiring `ready` without further proof. No cancellation-specific proof oracle is needed.

The compiler work includes read-only root provenance, effect-call checks, target admission, shared identity/grouping, lifetime handling, and lowering of the new handle. References remain ordinary `&T`; there is no need for a new reference qualifier.

The runtime work should reuse the current guard protocol:

1. Give cancellation a held state unit, guard-watch list, and unified lifetime across sources, watches, and views.
2. Make fire’s acquisition resumable; preserve the existing shared-object progress discipline rather than adding an unbounded spin.
3. Under the held unit, perform the first false-to-true transition. Wake guard watches on release and retain #296’s driver notification path.
4. Preserve host-wait registration-before-fired-check and acquire observation. Do not move host-wait registration under the cancellation unit’s lock.
5. Deduplicate aliases, retain captured targets through every exit, and implement `never` without accidentally treating its null runtime representation as an ordinary mutable object.
6. Keep the guard-visible state and the host fast-path fired representation one logical transition. A separate atomic mirror is an implementation possibility, not a second cancellation state.

The relevant current boundaries are [fire and driver notification](https://github.com/Ming-Research/Whitefoot/blob/b3f742f6a795b6810f1bc3289a096001136e5013/compiler/src/backend/completion/bridge.c#L1973-L2015), [shared-unit watch/release](https://github.com/Ming-Research/Whitefoot/blob/b3f742f6a795b6810f1bc3289a096001136e5013/compiler/src/backend/completion/bridge.c#L2973-L3081), and [watched host submission](https://github.com/Ming-Research/Whitefoot/blob/b3f742f6a795b6810f1bc3289a096001136e5013/compiler/src/backend/completion/bridge.c#L3969-L4007).

Statements without the new views need no additional emitted work. Nevertheless, cancellation allocations may become larger, and source firing becomes subject to state-lock contention. Those costs must be reported; “no cost without a cancellation target” must not be expanded into an unmeasured zero-cost claim for all cancellation users.

The minimum useful evidence is:

| Layer | Cases that distinguish correct behavior |
|---|---|
| Runnable conformance | Fired before statement entry; firing makes an otherwise permanently false guard proceed; ordinary readiness wins with cancellation unfired; related handles observe one permanent state; `never` stays false; closing handles preserves retained views. |
| Atomic semantics | Cancellation plus ordinary state is coherent; repeated reads within one block are stable; duplicate views acquire once; read-only helper calls work; target replacement/release and all leaving edges preserve lifetime. |
| Negative conformance | Direct field write, whole-state replacement, exchange of noncopy state through read-only views, write through aliases/helpers, consuming a read-only referent, and escaping a state reference. |
| Waiting and proofs | Fire in an atomic block is rejected; fire from a nonwaiting function is rejected; existing waiting/nested-atomic guard restrictions remain; the disjunctive guard does not prove readiness alone. |
| Runtime races | Fire before registration, between registration and park, and after park; simultaneous state-write and cancellation wakes; multiple waiters/drivers; exactly one ready-queue insertion; all links removed after waking. |
| Whole programs | One source ends both a parked host wait and a parked atomic guard; deadline-versus-work with timer disarming and prompt joins; multiple cancellation states and a later-acquired ordinary target. |

Use external watchdogs to fail lost-wake cases; do not give those waits fallback deadlines that could make broken cancellation pass. Deterministic registration/park schedules belong in runtime probes. Source conformance should assert language outcomes, not a particular scheduler timing.

The general read-only facility also needs coverage through mutable/read-only aliases of the same ordinary object, including map selections if admitted. Existing tests should be reused where they already exercise a requirement.

All execution belongs in CI. The ordinary gate and host-runtime checks must validate the final revision; contention or performance claims require a separately designed measurement. The specification, design nodes, derived declarations, conformance coverage, and approval records must change together. In particular, making fire waiting must be reported as a source-acceptance change.

**4. Direction A remains viable; its “no new machinery” explanation was incomplete.**

The investigation correctly identified the reusable wakeup mechanism. Its statement that cancellation is a shared object read as a target omitted three obligations:

- **Read authority:** ordinary shared state is writable, including at its root.
- **Atomicity:** a live atomic bit is not a held shared-state observation.
- **Firing discipline:** the current nonwaiting fire can execute while an atomic statement already holds that state.

None disproves direction A. Together they mean the stacked PR needs an owner-selected shared-observation design and an explicit decision about waiting fire. A query plus driver wakeups would leave the gap open.

The independent read-only review confirmed these issues, particularly the whole-state exchange witness and the hidden firing-lock cycle. The proposed implementation’s layout, fairness, platform behavior, and costs remain unverified until it exists and runs in CI.
**Implementation findings and evidence still required.**

The fielded-opaque representation was a prerequisite defect: nominal completion classified every opaque declaration as fieldless, so a public `CancelState.fired` could not be selected. TYPE-2 and PRE-2 already require fields to determine representation and capabilities. The general correction retains fields on every fielded opaque, keeps its constructor refused, and brings the existing `Instant` and `Option<Instant>` native layouts into agreement. This selects no new language rule. Existing clock/deadline cases and the new state-view cases must validate the change in CI.

The implementation reuses shared acquisition and guard watches. A native waiting start can request acquisition of a shared unit; its suspension retries that acquisition on wake, and its nonwaiting finish publishes the transition and releases the unit. Ordinary host completions keep their existing finish continuation. This is a private waiting ABI extension implementing the selected waiting fire, not an additional source operation or outcome.

Cancellation allocations now include the shared-unit header and guard-visible Bool beside the host atomic mirror. Firing can contend with observing statements. No contention, allocation-cost or performance measurement has run. No local build or test is authorized for this work; all execution evidence remains for CI.

Read-only authority follows resolved state paths, as SHARE-2 specifies; it does not revoke independent handles stored as data. SHARE-1 therefore states the absence of a SharedRead-to-Shared conversion, rather than claiming that ordinary read-only calls cannot copy a stored mutable capability. The owner-selected path restriction and existing effect-call judgment remain unchanged.

The authored evidence now includes both timer-versus-work outcomes with explicit disarming before the timer result is joined, and two cancellation states followed by an ordinary Shared<Instant> target. CancelState sorts before Instant in the existing qualified-type order, so the latter exercises later acquisition. The lifetime case drops the originating mutable handle before entering the statement and replaces its last program view while the statement retains its captured target. These cases are pending CI, not observed results.
