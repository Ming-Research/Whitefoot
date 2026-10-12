# Type invariants through enum payloads

Status: direction selected by the owner on 2026-10-12 (board item on type
invariants through `Result` at function boundaries, option A). This note
records the soundness argument written before implementation, the scope
limits it draws, and the results against its criterion.

## Question

[TYPE-11] makes a struct's type invariant a requirement and a postcondition
of each callable whose parameter or result is written as that struct. A
struct carried inside an enum, the ordinary shape of a fallible producer,
crosses the boundary without it. Snowghost's renderer (Snowghost-wf branch
`research/m2-fallback-inc1`, `research/investigations/m2-edit-cost/`) hit
both sides:

```text
struct BoundedSource {
  values: Box<Array<u32>>;
  invariant limit(owner): owner.values.inner.len <= 1073741824_u64;
}
fn source_failure<T: drop>() -> failed: Result<T, u32> pure { return Err<T, u32>(error: 1_u32); }
fn make_source(fail: Bool) -> made: Result<BoundedSource, u32> pure { ... }
```

- `source-result-invariant.wf`: `make_source` returns a valid source in
  `Ok`, the caller propagates it and passes it to a function taking
  `&BoundedSource`; the call's TYPE-11 requirement is unproved (FN-8),
  because nothing published the invariant through the `Result`.
- `source-error-forward.wf`: `make_source` states the bound as a written
  `ensures when Ok(value: produced): ...`; its branch `return
  source_failure::<BoundedSource>();` is unproved (FN-9), because the
  generic helper, whose every exit is `Err`, can state nothing about a
  success payload it never produces.

## Selected direction

TYPE-11's boundary rule extends to enum payloads. When an own parameter's or
an own result ordinal's declared type, after the function instance's generic
substitution [FN-2], is an enum one of whose variants V has a field f whose
type is a struct with type invariants, each of that struct's invariants is,
over the value's field f when the value holds V:

- a requirement of the function routed by (V, f) for a parameter; and
- a postcondition routed by (V, f) for a result ordinal, written as if the
  writer had stated `ensures when V(f: payload): <invariant over payload>`.

A routed postcondition a type invariant generates, which no exit of the
instance selects, holds vacuously and is not an error. Only function
boundaries change; a struct below a container element or another struct's
field stays outside TYPE-11, the remainder of the deferral the board keeps
for type invariants beyond directly written struct types.

The routes are not special to `Result` and `Option`: any enum, prelude or
source, and any variant field qualifies, failure variants included. Written
routes stay exactly as FN-9 admits them (`Ok` and `Some`).

## Soundness

The claim to keep: every value of an invariant struct S that crosses a
function boundary inside an enum payload satisfies S's invariants, and no
fact about a payload is used where its value was not checked.

### Where the invariant is proved

1. **A construction of V that reaches a return.** `return V(f: s, ...)` or
   `let r = V(f: s, ...); return move r;` is a selected exit. [ENT-5]'s
   conditional context of the constructed value holds the closed ordinary
   facts about `s` with each of `s`'s data replaced by the private payload
   root, so the relation is proved from what was known of `s` when it was
   wrapped. Constructing S itself already owes the invariant [TYPE-11], and
   the construction's field facts reach the binding of `s`, so a freshly
   built payload is provable without a written fact.
2. **A return of a callee's result.** `return g();` and `let r = g(); return
   move r;` forward a value whose context received g's own routed
   postconditions at the call [CALL-6, ENT-5]. g's instance proved them at
   its own exits by this same rule, or holds them vacuously (below).
   Nothing is assumed from g's name or body: only its verified summary
   enters, and only on the route it was verified for.
3. **Generic instances.** The rule reads the declared type after the
   instance's substitution, so `source_failure::<BoundedSource>` has the
   result type `Result<BoundedSource, u32>` and owes the `Ok`-routed
   invariant; every instance is checked separately [FN-2], so the obligation
   is discharged by the instance that publishes it. Its only exit is a
   direct `Err`, so the obligation is vacuous there, and the instance
   publishes it.
4. **A call argument.** A routed requirement is judged at each call in the
   argument's conditional context for (V, f), the same context a forwarded
   return is judged in. An argument with no evidence has an empty context
   and proves nothing. An argument that is not a bare binding, such as a
   struct's field, carries no context; its goal is taken over that
   argument's own payload place, or over its occurrence-local value where
   no place names it, so only ordinary facts already known of that payload
   place can prove it. Such a fact was formed under a refinement that the
   value held V [ENT-3.S15], and any write that could change the variant
   kills it [ENT-5].
5. **A propagated error exit.** `let x = propagate e;` returns `Err(err)` of
   the enclosing function's type. When that type's `Err` field is an
   invariant struct, the exit is selected by the `Err`-routed postcondition
   and judged in e's own `Err`-routed context, which e's producer verified.

Nothing is trusted unproved: no PRE-1 row has a parameter or result whose
type is an enum with a payload field, and a PRE-2 host function's enum types
carry only the host module's own types, none of which declares a type
invariant, and no type parameter in a payload position; so no supplied
declaration carries a generated relation that SCOPE-3 would have to supply.
A concurrent hash map's `Option<V>` entries are reached only through atomic
statements, a container position this change leaves alone.

### Where it is assumed

1. **Parameter entry.** At body entry each routed requirement is established
   in the parameter's own conditional context for (V, f) [ENT-3.S4], never
   as an ordinary fact. An own `match` arm for V that binds f, or a
   propagate's successful continuation for `Ok`, selects that context onto
   the binder [ENT-5].
2. **After a call.** The callee's routed postconditions are established in
   the result's context for (V, f) [CALL-6]; the same selections deliver
   them to a payload binder.

The conditional context is what keeps the assumption sound. A routed
condition over a variant the value does not hold is not a fact about any
place: established as an ordinary fact over the payload place, an
unsatisfiable invariant (no value of S exists) would make the entry state
contradictory and the whole instance uninhabited [FN-8], so a call passing
another variant would skip every proof in the body. Inside the context, the
contradiction stays local until V is selected, which [ENT-5] already
guarantees for success contexts (`type11-neg-enum-payload-contradictory-entry`).

### Why a vacuous routed postcondition is sound

A routed postcondition states "if the returned value holds V, the relation
holds of its field f". An exit is unselected exactly when the value it
returns definitely holds another variant: a direct construction of another
variant, or a forwarded value whose transported tag evidence is definitely
not V [ENT-5]. When every exit is unselected, no value the instance returns
holds V, so the implication holds for every returned value and the summary
is true. Publishing it lets a caller reach only conclusions about a payload
that never exists: the caller's context for (V, f) on that value becomes
the published relation, and a selection of V on that value never happens at
run time.

FN-9's rule that an inhabited instance's routed clause must select an exit
is a guard against a writer's mistake, a clause written for an exit the
writer forgot; a generated clause has no writer, and its emptiness is the
ordinary case of a failure helper. The guard stays for written clauses.

### Nested payloads and generic payloads

- **A payload of type parameter T instantiated with S**, such as
  `Result<T, E>` in a generic function's instance at `T = S`, is covered:
  the rule reads the instance's substituted type.
- **A bare parameter or result of type T instantiated with S** keeps
  today's rule: a written type parameter takes no type invariant, whatever
  its instance [TYPE-11]. Ruling A changes enum payloads only.
- **Nested payloads** such as `Option<Result<S, E>>` are out of scope: the
  outer variant's payload is an enum, not a struct with invariants, so it
  generates nothing, and the inner `Result` reached by matching the outer
  `Some` carries no transported evidence (an enum payload supplies no datum
  [CALL-4]). A struct field of type S inside a payload struct is out of
  scope the same way. Both stay with the deferral of containers and fields.
- **A reference to an enum** (`&Option<S>`) is out of scope: its declared
  type is a reference, not an enum, and [ENT-5] transports no evidence
  through a borrowed selection.
- **Range type invariants** are out of scope in this change: only affine
  type invariants generate routed clauses. Omitting both the requirement and
  its entry assumption together is sound.

### A payload moved out, mutated and re-wrapped

`match o { Some(value: p) => { set p.next = 99_u64; return Some<S>(value:
move p); } }` re-wraps a payload after a field write. The re-wrap is a
construction: its context is built from p's current facts, which the write
has already killed or replaced [ENT-5], so the relation is unproved at the
return and refused. A write through a reference-mode `match` binder into a
payload still inside its enum overlaps the enum's storage, which removes the
enum's association with its context [ENT-5]; a later return or call argument
of that value has an empty context and is refused
(`type11-neg-enum-payload-borrowed-write`).

## Consequences

- Every generic source function whose instance at an invariant struct
  returns an `Option<T>` or `Result<T, E>` holding a value it cannot show
  valid becomes unprovable at that instance. The standard library's
  containers do this: `slab_remove::<S>` returns an element read from
  storage, and storage elements carry no invariant. A program that keeps an
  invariant struct in a library container and removes it is accepted today
  and refused after this change. No program in this repository or on the
  main lines of Snowghost, Halo or Firn does so; Halo's two witnesses
  declare invariant structs but put none in a container.
- A generic wrapper `fn wrap<T>(x: T) -> Option<T>` at `T = S` is refused
  the same way, because its bare-T parameter takes no requirement. Closing
  that needs either bare type-parameter positions to take the instance's
  invariant, or container elements to carry it; both are owner decisions.

## Transport needed

[ENT-5]'s conditional context today exists only for the success variant of
`Result` and `Option`. The rule needs one per route: for an own enum value,
one context for each variant field whose type is a struct with type
invariants, beside the existing success context. A context is created by a
construction (the constructed variant's fields get their operands' facts,
every other variant's context becomes definitely-not), by a call's routed
postconditions, and by a parameter's routed requirements at entry; it follows
copies, moves, joins, kills and scope exits exactly as the success context
does; an own `match` arm for V selects (V, f) onto f's binder; and FN-9
judges a forwarded return in it. No new term, runtime value or spelling is
added.

## Criterion before implementation

The direction is supported if, with the branch compiler:

1. both Snowghost witnesses, `source-result-invariant.wf` and
   `source-error-forward.wf`, are accepted unchanged, and a version of the
   second without its written `ensures` is accepted too;
2. a payload that breaks the invariant is refused where it is wrapped and
   returned (FN-9), and an argument that breaks it is refused at the call
   (FN-8);
3. a caller of a callee whose instance can return the variant relies on the
   published invariant with no written fact, through `match` and through
   `propagate`;
4. the same struct inside a container element or another struct's field
   gains nothing, so a caller relying on it is still refused;
5. a generic failure helper instantiated at the struct is accepted with no
   selected exit, while a written routed clause with no selected exit is
   still refused;
6. every existing conformance case and compiler test keeps its verdict.

It is rejected if a sound program shape that the owner's ruling covers needs
a run-time check or a written fact to pass, or if a routed assumption can be
used where the variant is not held.

## Results against the criterion

Observed with a debug build of the branch compiler and of its base, main at
`51c5f4263`, each program checked with `--check` and the positive ones
also built and run; each program is the conformance case named. The branch
was later merged with main at `f0df1d258`; the table below was not taken
again at that base, so its Base column is main at `51c5f4263`.

| Criterion | Case | Base | Branch |
| --- | --- | --- | --- |
| 1 | `type11-pos-enum-payload-propagated-result` | FN-8 | accepted |
| 1 | `type11-pos-enum-payload-failure-helper` | FN-9 | accepted |
| 1 | `type11-pos-enum-payload-generated-forward` | FN-8 | accepted |
| 2 | `type11-neg-enum-payload-returned-construction` | accepted | FN-9 at the return |
| 2 | `type11-neg-enum-payload-argument` | OP-4 in the callee | FN-8 at the call |
| 2 | `type11-neg-enum-payload-rewrap` | accepted | FN-9 at the return |
| 3 | `type11-pos-enum-payload-option-argument` | OP-4 | runs, exit 7 |
| 3 | `type11-pos-enum-payload-error-variant` | FN-8 | runs, exit 9 |
| 3 | `type11-pos-enum-payload-source-enum` | FN-8 | runs, exit 3 |
| 3 | `type11-pos-enum-payload-interface-member` | OP-4 | runs, exit 12 |
| 4 | `type11-neg-enum-payload-struct-field` | FN-8 | FN-8 |
| 5 | `type11-pos-enum-payload-failure-helper` | FN-9 | accepted |
| 5 | `fn9-neg-no-selected-normal-exit` (existing) | FN-9 | FN-9 |
| scope | `type11-neg-enum-payload-generic-argument` | accepted | FN-8 at the call |

The generic wrapper of [Consequences](#consequences), `fn wrap_one<T:
drop>(item: T) -> result: Option<T>` called at `T = Span`, is accepted by
the base and refused by the branch with FN-9 at its return, as predicted.

Criterion 4 is shown for a struct field only; a container element was not
run.

Criterion 6 was judged by CI on the branch head `84bf61968` (base main at
`f0df1d258`): every Linux gate group (static checks, compiler unit tests,
the conformance corpus and the runtime) and the completion jobs passed, so
every existing conformance case and compiler test kept its verdict there, the
Linux conformance-corpus stage taking 173.0 s of its 175 s budget. A gate run
dispatched on the same revision to cover both platforms passed every macOS
group (the macOS corpus stage took 156.9 s of 185 s) and every Linux group
except the Linux corpus stage's time budget, which it overran at 175.6 s and
176.2 s on a re-run. Those overruns are times, not verdicts; no case changed
its result.

Cases added after these runs, with no observed verdict yet:

- `type11-neg-enum-payload-contradictory-entry`: a struct whose two
  invariants admit no value, carried in an `Option` parameter whose body
  performs an unproved subscript and is called with `None`. Entry
  establishes the invariants only in the parameter's `Some` context, so the
  subscript stays unproved (OP-4); established as an ordinary fact, the
  entry state would be contradictory and the program accepted.
- `type11-neg-enum-payload-borrowed-write`: a write through a
  reference-mode `match` binder into a payload still inside its enum,
  after which the enum is returned; the write removes the enum's
  association with its context, so the routed postcondition fails at the
  return (FN-9).
- `type11-pos-enum-payload-field-argument`: an argument that is a struct's
  field rather than a bare binding is judged over that field's own payload
  place, so a comparison already made through a reference-mode `match`
  binder proves the callee's routed requirement.
