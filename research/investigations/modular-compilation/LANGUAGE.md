# Module boundary rules selected for implementation

This companion to [DESIGN.md](DESIGN.md) closes its source-language choices.
It was written as an amendment over the v0.69 specification. The active
specification v0.70 now owns the grammar and the formation, name, alias,
access, publication, correspondence, module-verdict, readonly, entry and
no-heap rules below [PROG-1 to PROG-3, MOD-1 to MOD-9, TYPE-2, TYPE-6,
STOR-8, CALL-4]; where the two differ, the specification governs. Retain this
document while its remaining sections (per-module summary publication,
conservative cross-module components, effect rows over accessible paths)
have no specification text, and retire it when the specification owns the
whole boundary.

## Formation and names

There is exactly one explicitly selected `modules.wfg` for a compilation.
Its directory is the package root; the process working directory is irrelevant.
There is no graph search, nested graph activation, external package binding,
version resolution, glob import, namespace declaration, or public re-export
through an alias in this design. External packages are deliberately outside
the selected scope, not an unresolved prerequisite for implementation.

A graph consists of one or more module rows followed by zero or more named
entries:

```text
pkg::data: [];
pkg::runtime::queue: [pkg::data];
pkg::runtime: [pkg::data, pkg::runtime::queue];
entry tool = pkg::runtime::run;
entry small = pkg::runtime::run_small {
  no_heap;
}
```

Each row registers one directory and lists its exact permitted direct
dependencies. Every dependency must already have appeared as a row. Reject
duplicate rows, duplicate edges, self-edges, missing rows, and a forward edge
at its written path. Strict position descent proves acyclicity; neither an
inferred topological order nor a cycle-repair heuristic chooses the architecture.
Reordering valid rows changes no declaration identity. Multiple valid orders
are allowed: canonicality does not impose alphabetical sorting of a DAG.

`pkg` alone registers the root module. Otherwise each lowercase identifier
after `pkg::` is a directory component. Registering a descendant does not
register its ancestors. Parent, child, sibling, and cross-branch modules have
identical permission and privacy rules. Naming a descendant does not require
edges to intermediate namespace directories. A direct edge to the actual
target is required even when a transitive path already exists.

Only `directory/module.wfm` and that directory's direct regular `*.wf` files
belong to a module. Child directories are not scanned into the parent.
Reject symlinked source roots, module directories, interfaces and source files,
path escape, duplicate physical ownership, and case-folding collisions rather
than selecting behavior by host filesystem order. Directory components use
the language's lowercase identifier alphabet. Source basenames are not symbols;
sort their relative byte spellings for deterministic diagnostics. An existing
unregistered directory grants no source visibility and is not silently built.

An explicit `alias Name = pkg::path::Name;` or
`alias short = pkg::path;` is legal only in the initial alias header of a
source/interface file. The right side is a complete `pkg`-qualified path,
never another alias, an instantiated type, or a call. Aliases are file-local
abbreviations preserving the target's identity and name domain. Uppercase
aliases denote types/groups/variants; lowercase aliases denote modules or
values/callables. Context checks reject the wrong domain. Reserve `pkg`;
reject duplicate aliases and ordinary prohibited shadowing, including an alias
that collides with a module-local declaration visible in the file. An unused
alias still receives formation, access and direct-edge checks.

There is one module-local inventory across `.wfm` and direct `.wf` records.
Declarations are forward-visible; local binders keep their lexical scopes.
Detect duplicate declarations independently of file enumeration. Resolve
constant dependencies, group dependencies and finite layouts separately;
name visibility does not admit their cycles. A same-module declaration moved
between `.wf` files retains identity but must resolve the new file's aliases.
Module path components and module-level lowercase declarations cannot occupy
the same qualified slot. Type-owned member labels are separate slots.
An unrooted qualified module path begins with an explicit file-local module
alias; directory proximity is not an implicit alias. Ordinary type-owned
group/variant qualification remains a distinct name domain.

Canonical rendering extends the existing source renderer [FORM-2]: one space
after `public`, ordinary declaration indentation, and `alias name = pkg::path;`
with one alias per line, no empty line between aliases and one empty line
before the first item. A `.wfm` function declaration ends in `;` directly after
its header or its contract's closing brace, or in its `doc` entry, which stays
on that same line after one space: `} doc "Appends one job.";`. A graph row
renders as `pkg::path: [pkg::dependency, pkg::other];`, preserving written row
and edge order, with rows on consecutive lines. An entry without requirements
renders on one line as `entry name = pkg::path;`; an entry with requirements
renders its block with ordinary two-space indentation. One empty line separates
the last row from the first entry and each entry from the next. Semantic
dependency sets normalize independently of written edge order. No formatter
infers an order or changes the user's dependency architecture. Existing
no-comment/doc rules remain [FORM-4].

Source-qualified references require an edge; compiler metadata traversals do
not grant one. For example, A may return B's public type and C may consume that
inferred value through A's API with only C -> A -> B. C cannot explicitly name
`pkg::b::T`, alias it, select a B field, or call B without C -> B. The compiler
still loads B's semantic/layout descriptions and records those actual query
dependencies. Passing an inferred value does not publish B's source namespace.

## Interface ownership and correspondence

Only `.wfm` accepts `public`; unmarked declarations and data members are
private to the module. There is no `private` keyword. Aliases, parameter
labels and local binders are never export declarations.

Every public source nominal has one complete definition in `.wfm`, including
its private fields and supporting declarations. Those definitions are reused
throughout the module. Reject a second, partial, or extending `.wf` definition.
A wholly implementation-private nominal may instead be defined once in `.wf`.
The `.wfm` must form using its own declarations, prelude and allowed dependency
interfaces; it never searches `.wf` for a missing type, constant, contract,
member path or callable signature.

All `.wfm` function items, public or private, are declarations ending in `;` or
in their `doc` entry. Every one requires exactly one ordinary `.wf` definition.
Definitions repeat the entire signature and contract without `public`; ordinary
private helpers need no `.wfm` declaration. There are no executable bodies,
generated property bodies or trusted external source declarations in `.wfm`.
Constants, complete type schemas, groups and binding maps are declarative
definitions. The declaration's `doc` states what the function is for and what
its callers and implementer need beyond the checked contract; the definition's
body `doc` describes the implementation. Documentation takes no part in
correspondence.

Compare declaration and definition after resolving aliases and canonical
identities. Equality covers parameter order/names/kinds/types, result
order/names/types, generic order/kinds/bounds, normalized
effects, and ordered normalized requires/ensures. Alpha-normalize generic and
contract-local binders; retain public argument/result labels because callers
write them. Erased `define` sharing and equivalent resolved aliases may differ.
Use existing clause normalization; do not run a solver to excuse different
contracts. Reordering clauses is a change to the written boundary. A stronger
implementation requirement, weaker guarantee, extra effect, missing definition
or duplicate definition rejects. Internal invariants may prove additional
facts; those facts do not silently augment the public contract.

Correspondence is exact equality rather than FN-4-style refinement. An edited
declaration is an interface change: its implementation must acknowledge it in
the same written form, so no check can pass an interface edit by leaving the
implementation text behind. Refinement would let an implementation keep a
weaker requirement, a stronger guarantee or a narrower effect row, and callers
could use none of these because they read only the declaration; parameters,
results, generics and types cannot differ in any case. Repeating a checked
header costs a writer little. The one place where exactness asks the interface
to track the implementation is the effect row, and a row entry may cover the
accesses below its path [EFF-2]: an interface that declares `writes(queue.storage)`
admits every implementation writing parts of that storage.

Interface formation can run before implementation checking. It produces a
well-formed claim, never permission to trust that claim. Current body evidence,
generic-instance checks and current proof-component availability are required
before any composed program or executable is published.

## Visibility

One visibility rule serves executable code and annotations. Every name and
field selection in a body, a contract clause or `define`, a loop or local
invariant, a `use` premise, an effect row and a function-kind formal must be
accessible where it is written. A private declaration or field is accessible
only inside its declaring module; a public one is accessible to modules with a
direct graph edge to its declaring module. The prelude's declarations and
members keep their ordinary availability.

Public signatures close over accessible vocabulary. A public function's
parameter and result types, bounds, effect row, contract clauses and
function-kind formals, a public field's type and a public constant's type may
name only declarations and fields that the module's clients can access: public
declarations of the module and of its permitted dependency interfaces, and
public fields along every written path. This is the existing requirement that
public contracts not name private constants or nominals, applied to fields as
well. A private function's contract may name private fields, since only the
module's own code calls it.

A field that a public contract or effect row must name is therefore public,
usually `public readonly`. A queue whose operations are stated over its ring's
length publishes that ring:

```text
public nocopy struct Queue {
  public readonly storage: Ring<Job, capacity>;
}

public fn push(queue: &Queue, job: Job) -> result: unit writes(queue.storage) contract {
  requires deref(queue).storage.len < capacity;
  ensures deref(queue).storage.len == deref(entry(queue)).storage.len + 1_u64;
} doc "Appends one job at the back of the ring.";
```

Any caller may then read `deref(queue).storage.len` in executable code and in
annotations, exactly as it reads a prelude window's measure; no getter is
needed. Publication exposes the whole field, its type and every member reachable
through it, so a change to a published representation is an interface change
that callers see. That is the intended cost: the interface states what its
contracts depend on.

`readonly` restricts writers to the declaring module. Outside the module that
declares a readonly field, a path that ends at or passes through that field is
never a `set` target and never the argument at a reference parameter whose
callee row writes that parameter, and construction cannot supply its value.
Inside the declaring module it is an ordinary field. Every other TYPE-2 clause
is unchanged: a whole-value assignment still replaces the field together with
its owner, and a declared row may name it in `writes` because the row reports
what callees change. The prelude declares the storage shapes' `len`, `cap`
and `head`, so only prelude operations write them, as today. This fulfills the
existing readonly decision's stated purpose, a member every holder may read
that only the type's own operations change, which a program-wide prohibition
cannot provide for a source type whose operations update it in place.
`readonly` is admitted only on a public field, since a private field is already
unreachable from other modules. Ordinary copy reads, borrows, reads through
references and consuming extraction of a readonly field remain available to
every module that can access it.

Construction from outside the declaring module requires every field to be
public and not readonly; otherwise the module's own functions construct the
value. There are no default private values or hidden constructor writes.
Whole-value ownership and replacement remain ordinary operations and need no
field access.

An extraction names only accessible fields. A consuming field move kills the
whole owner; no holes remain. Unbound parts, including private parts covered
by `..`, must have a legal ordinary release. A private linear remainder makes
that extraction illegal; consume it through an operation in the owning module.
Moved-owner references become invalid and later use rejects. Their mere
existence is not a new prohibition on the move. Copy fields are read without
`move`. Source `opaque` retains its existing meaning and is not a privacy
synonym.

`public` on a field or payload field of a private type, and `public` in `.wf`,
reject: neither creates an access route. A public field of a public type is
reachable only through a value of that type, so effective access also requires
an accessible enclosing type.

## Types, capabilities, variants, constants and groups

Copy/drop capabilities are derived from every owning component, including
private support, under existing modifiers and generic substitution. `nocopy`
removes copy; `nodrop` removes both copy and drop. There is no handwritten
second capability declaration. Actual instance capabilities differ from what
a generic body's written bounds permit. Hidden linear ownership remains an
obligation; `nodrop` requires lawful explicit consumption, not a specially
named finalizer. No user-defined destructor or hidden runtime callback is added.

A public enum is a closed case API: all of its variants are public, and a
private enum's variants are private. Variants carry no modifier, since a
per-variant marker could only restate its enum's visibility. Payload fields
default private and can be published individually. Constructors require access
to all their payload fields; patterns bind accessible payloads and use the
existing residual-release rule for `..`, which this proposal extends to enum
arms. Match coverage still includes every variant, independently of payload
access. There is no wildcard hiding a case or unproved disposal of a private
linear payload. An API that hides its cases uses a public struct containing a
private enum and ordinary operations. This selects a closed sum rather than
adding non-exhaustive enums and opaque case matching to the module mechanism.

User enum constructors are type-owned: `Choice<T>::Some(value: x)` or
`pkg::data::Choice<T>::Some(value: x)`. A direct alias may abbreviate an
unspecialized accessible variant of a nongeneric enum; generic constructors
retain the explicit owner and its arguments. Pattern and result-route labels
remain unqualified and resolve against the known scrutinee/result type, not a
module-wide variant inventory. Compiler-prelude constructor spellings such as
`Ok<T,E>` retain their existing declarations. Type and enum member lookup
therefore need explicit owner identity in persistent keys. This changes v0.69,
where each source variant is a constructor in the program-wide inventory and an
arm or route label first resolves globally [TYPE-6]: a current unqualified
construction such as the worked example's `Neg()` becomes `Sign::Neg()`, and
arm resolution reads the already typed scrutinee. The legacy source-bundle mode
feeds the same checker and follows the same rule.

A public constant has its exact type and initializer in `.wfm`. Its value,
including private helper-constant evaluation, must be determined there.
Published types/bounds/contracts cannot require clients to name a private
constant or nominal. Private constants used only in private field types are
allowed. Const evaluation and eligibility remain unchanged except that the
acyclic dependency graph replaces source declaration order. Public constants
are compile-time values, not dynamically initialized globals. Modules introduce
no initialization order or initialization functions.

A public `interface` publishes the complete ordered function-parameter group.
Its member labels are formal parameters, not individually exported functions,
and have no visibility modifiers. A public `binding` publishes the complete
actual bundle with its checked group boundary. Private actual functions must
have complete private declarations in the same `.wfm` so the map is
self-contained; publication permits use through the bundle, not direct
source naming of those actuals. Ordinary FN-4 refinement and FN-5 authoritative
formal calls still apply; no runtime dictionary or implicit implementation is
introduced. A public bundle is an explicit API choice in the sole interface,
unlike an alias that cannot publish anything.

## Effects

Effect rows keep their structural paths [EFF-1]. Under the visibility rule, a
row in a public declaration, in a function-kind formal, or anywhere outside the
declaring module names only accessible fields; prelude measure and window-part
names remain available. An operation can declare `writes(values.storage)` while
leaving a public `values.tag` independent, and a caller can repeat that row in
a wrapper or formal. There are no named footprint declarations, mapping
expansion, extra disjointness axioms or new path syntax.

A public operation whose body writes private state declares the nearest
accessible path that covers it, which may be the whole parameter. EFF-2 already
counts a declared entry as exhibited by any access at or below its path, so the
covering row is exact in the existing sense; it only loses the independence a
finer published path would show. When callers need that independence, the
interface publishes the finer field.

Resolve each path to canonical member identities, then check declared versus
exhibited effects in both directions using EFF-2. An annotation mentioning a
field exhibits no runtime access and cannot justify a padded effect row.
Every declared effect still needs its ordinary body witness; `writes` retains
its existing coverage of reads. Aliases, prefixes, ranges and actual storage
identity decide overlap. Different spellings never imply independence.
`pure` retains its existing meaning: no state reads/writes and no promise of
termination.

## Contracts across modules

A caller uses only a callee's written declaration: its requirements are
discharged at the call and its verified postconditions are established after a
real normal return [FN-8, FN-9, CALL-6]. Because a public contract names only
accessible paths, a caller can restate any condition it needs in its own
contract, loop invariant or `use` premise. Calls remain excluded from contract
expressions; an ordinary getter keeps its runtime call and its written
normal-return postconditions, and a published readonly field makes a getter
unnecessary where executable code only needs to read state.

The existing denotations carry across modules unchanged [MSR-3]: a requirement
reads entry state, an unqualified reference-parameter path in `ensures` reads
the exit state of a written parameter, `deref(entry(parameter))` names the
frozen entry datum, and an own parameter's projections denote its entry value.
Call, entry and placement datums, including CONSTRUCT and REBIND placements
over owned descendant projections, and ENT-5 kills apply to fields of every
module alike. There is no runtime snapshot and no new datum family.

One admission is extended. CALL-4 admits a measure member of a result place
only at the bare result or through one `inner` step of a `Box` result. Admit a
measure member reached from a result place through any owned descendant
projection [MSR-3] made of struct-field selections and `Box` `inner` steps, so
a constructor can state `ensures made.storage.len == 0_u64;`. The measure is
queried at the selected return over the place that return hands back and
instantiated at the result destination, exactly as CALL-4 already does for the
bare result. Inside the callee, a call's own result reaches its binding by that
same CALL-4 instantiation, and MSR-3's CONSTRUCT and REBIND placements carry
measures from bare or moved places into the place the return hands back.
Enum-payload steps are excluded because no route selects a variant of an
unrouted result. The extension is independent of modules: a single-bundle
constructor such as `grow_vector_new` cannot state that its vector is empty
today.

There is no implicit source type invariant. Privacy alone establishes no
relation between fields. Public operations state their requirements and
guarantees; callbacks obey their ordinary boundaries even while an
implementation temporarily changes private state. A public mutable field linked
to private storage cannot rely on an unstated preserved invariant.

## Module verdicts and proof availability

A module's source verdict covers every judgment on its `.wfm` and direct `.wf`
files. It depends only on those files, the graph rows, the prelude, and the
resolved interfaces of its direct dependencies with the interfaces they close
over. It never depends on another module's `.wf`. Consequently a module can be
checked while a dependency's implementation is absent, incomplete or failing,
and an edit to one module's `.wf` changes no other module's source verdict.
This is the property the architect/implementer workflow in DESIGN.md relies on.

Recursive proof availability needs two adjustments for this.
FN-9 forms the concrete ordinary-call graph and withholds same-component
summaries. Across modules the real edges from a generic callee's instance to
the function-kind actuals it calls are known only from the callee's body. So
when a module's bodies call an instance of another module's generic callable,
that module's component formation treats the instance as calling every
function-kind actual and bundle member supplied to it, whether or not the
callee's body calls them. Components can only grow under this rule, so it
withholds more summaries and never admits a circular proof. It costs a
postcondition only where a module passes an actual that reaches back into the
calling component. An edit to the callee's body then cannot change which
summaries the caller's proofs may use.

FN-9 also publishes a component's summaries atomically, only after every member
verifies. A component formed under this rule can contain another module's
instance, whose body only composition checks, so atomic publication would hold
the calling module's own summaries, and with them its verdict, until
composition. Publication is therefore per module: the members that a module's
own check verifies publish their summaries to the module's other proofs once all
of them verify, another module's instance never delays that publication, and
composition still requires every member of the component to verify.
Same-component summaries stay unavailable
during checking, so no member's proof depends on another member's result, and
publishing one module's verified members admits no circular proof. For a
component within one module this is FN-9's existing rule.

Two judgments are about composition rather than one module's sources, and each
is reported against the module that owns the failing source:

- A concrete instance of a generic template is checked with the template's body
  and the requester's actual arguments [FN-2, FN-6, FN-8, FN-9]. A failure is
  reported at the template's definition, naming the requesting instantiation
  site.
- A target requirement such as `no_heap` is checked over the target's execution
  closure. A failure is reported at the function whose body or layout
  introduces the heap requirement, with the call path from the entry.

A composed program is accepted only when every selected module's verdict holds,
every declared function has its definition, every required instance checks, and
every target requirement holds. A module whose `.wfm` declares a function that
no `.wf` defines yet is reported with that declaration pending; its other
definitions are still checked, and pending declarations block only
composition, lowering and publication.

## Composition argument and its implementation obligations

This argument assumes the existing primitive, ownership and proof judgments;
it is a design argument, not a verification of an implementation.

First, visibility adds no proof rule: every annotation is formed, typed and
state-checked exactly as the same text inside the declaring module would be,
and naming a field establishes no fact and authorizes no runtime access.

Second, each body proves exactly its declared requirements/effects/guarantees
under the existing recursive-component restrictions, with components formed and
published as above. Exported clauses are resolved expressions with member identities, not
strings reparsed in the caller. Only normal-returning calls publish verified
postconditions. A declaration or an unverified implementation supplies no axiom.

Third, substitution preserves the referenced state/value image. Actual writes
remove overlapping facts before postconditions describe the new state; entry
datums remain frozen; checked ownership transfer carries only the measures MSR-3
places. Current proof availability and dependency equality are required when a
cached caller derivation is rebound to a changed implementation.

Finally, composition validates every selected source obligation, instance and
target requirement before publication. Native symbol resolution is
insufficient. Cold/warm differential and mutation tests exercise these rules;
agreement between two executions of the checker is not a general soundness proof.

## Qualification witnesses

The GrowVector boundary uses its existing algorithms with a published storage
field and an independent public tag:

```text
public struct GrowVector<T, const ceiling: u64> {
  public readonly storage: Box<Slots<T>>;
  public tag: u64;
}

public fn append<T, const ceiling: u64>(values: &GrowVector<T, ceiling>, value: T) -> length: u64 writes(values.storage) contract {
  requires deref(values).storage.inner.len < ceiling;
  ensures length == deref(values).storage.inner.len;
  ensures deref(values).storage.inner.len == deref(entry(values)).storage.inner.len + 1_u64;
  ensures deref(values).storage.inner.cap >= deref(entry(values)).storage.inner.cap;
};
```

An external wrapper repeats those clauses and the row. A function-kind formal
states the same boundary; actual refinement and calls use ordinary FN-4/FN-5.
A consuming `free_empty(values: GrowVector<T, ceiling>)` requires
`values.storage.inner.len <= 0_u64` over its owned entry value, as the current
library already does. A caller loop keeps a header relation between its counter
and `deref(values).storage.inner.len`, and an explicit multi-premise `use`
certificate names that length beyond the automatic family. The queue demo
supplies the constructor/result projection case and an external function-kind
consumer.

Required positive controls: module-internal `set` and written-argument uses of
a readonly field; external copy reads, borrows and consuming extraction of a
public readonly field; a public contract, invariant and effect row over public
fields; a private function contract over private fields; a module checked while
its dependency's `.wf` is missing; and an unchanged caller verdict after a
callee body edit that starts calling a function actual supplied by that caller.

Required negative controls: external `set`, written-argument and construction
through a readonly field; `readonly` on a private field; `public` on a member of
a private type; a private field named in a public contract, public effect row,
formal or external annotation, each with the same diagnostic role as the
equivalent executable selection; an unproved postcondition; a missing
member-owner edge; uninhabited projections; inactive payloads; stale references;
a CALL-4 result projection through an enum payload; an invalidated saved length
relation after `append` that survives a tag-only write; `free_empty` after a
nonempty transfer; and declared effects without a body witness. Recheck consumed
field changes, and reuse source proofs after a body edit whose written claims
and current availability remain equal. These are implementation acceptance
obligations, not executed test results.

## Target requirements

A named entry selects exactly one public ordinary function by a full `pkg`
path. It does not change FN-7/PROG-3: the build must establish a complete
ordinary call binding, including explicit generic actuals, argument types,
ownership and requirements, and ordinary result handling. That checked binding
is a target-composition input, not permission to assume requirements from a
command-line value. The graph does not contain runtime values or invent a
second argument language. The native launcher initially retains its current
supported binding shapes, using the selected identity instead of a hardcoded
name; a nongeneric wrapper is the direct source route to a different instance
or argument adaptation. Unsupported launcher binding is a compiler capability
diagnostic, not rejection of an otherwise valid source function or graph.
Duplicate entry names and missing/private/wrong-kind functions reject. A graph
may have no entries for check-only use. Check-only formation does not execute an
entry or demand that a native launcher support every valid callable signature.

The build invocation may also run any ordinary function of a registered module,
public or private, as an unnamed entry under the same FN-7/PROG-3 binding [FN-7].
Graph entries name deliverable executables and their environment requirements;
an implementer's module-local test entry needs neither a graph edit nor a public
declaration.

All graph rows are structurally checked for every invocation. A target checks
every source definition/template in its entry module's declared dependency
closure, not just called bodies. Check-only may select one module, one module
closure or all graph modules. Required concrete instances still receive
ordinary checking.

`no_heap` is a target requirement over the conservative concrete execution
closure, seeded by the entry and platform startup. Include all syntactic calls
in selected concrete bodies, called function-formal actuals, materialized
parameter/local/result/value layouts, derived release and required native/
runtime supplies. Include calls in branches regardless of constant conditions;
exclude erased proof annotations and uncalled declarations. Phantom type
arguments contribute only through their actual value/layout/release uses.
An allocation, required heap-bearing value, heap release or runtime heap need
rejects that target at the function that introduces it. Private representation
is not an exemption. Fixed-point summaries retract on edge/body deletion and are
evaluated before optimization.

An unused allocating helper in a selected source module remains fully checked
but does not impose a heap need on this target. Objects and runtime selection
must likewise exclude its allocator dependency. This deliberately replaces
STOR-8's compilation-unit spelling ban; merely moving `program no_heap` into
the graph without changing its judgment is insufficient. Ordinary module
proofs are shared between heap-enabled and no-heap targets.

## Sources and rejected alternatives

[OpenJML's visibility explanation](https://www.openjml.org/tutorial/Visibility)
separates executable access from specification visibility and describes the
representation coupling of directly exposed specification fields. WF does not
select a separate specification visibility: fields that contracts need are
published, and privacy means the same in code and annotations. This is a
comparison, not adoption of JML's solver or logical method calls.
[Why3's type-invariant rules](https://why3.org/doc/syntaxref.html#record-types)
illustrate the extra construction and call-boundary obligations of implicit
invariants; WF retains explicit operation contracts.

- Letting annotations name private fields of complete interface definitions
  while executable code could not: rejected because contracts would depend on
  fields the interface does not mark as API, and the checker would need a
  second, role-dependent lookup. Publishing the field states the dependency
  where the interface is reviewed.
- Named specification projections and effect regions in `.wfm` that clients use
  instead of field paths: deferred. They would let a representation change
  leave client source untouched, which the module goals do not require; the
  field path is already the explicit interface. Reopen for a concrete type that
  must publish one quantity without publishing the storage that holds it.
- A program-wide `readonly` for source fields: rejected because the declaring
  module could not update its own published state in place, and a type whose
  operations maintain a readable member is the purpose the readonly decision
  states.
- Refinement correspondence between declaration and definition: rejected for
  the reasons in the correspondence section.
- A per-variant `public` marker: rejected because it can only repeat its enum's
  visibility.
- Getter functions in contract expressions, logical observation functions and
  `use view` steps: not selected. Published fields name the state directly and
  keep FN-8's exclusion of callable execution in contracts.

Representation-independent model properties and mathematical functions remain
possible separate extensions. Reopen them for a concrete consumer whose
representation changes or algorithmic contract makes direct structural proofs
unsuitable; compare migration cost, totality/proof requirements and incremental
invalidation before choosing a new mechanism. No such abstraction is required
for this module implementation, and arbitrary runtime functions remain excluded
from erased proof expressions. No external reference defines WF acceptance.
