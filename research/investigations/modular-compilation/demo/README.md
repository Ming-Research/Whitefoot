# Two entries over one module graph

This is a complete **source design specimen** for the module system: every
declared function has an implementation, both entries have complete bodies,
and all application dependencies are present. The compiler checks it and
builds and runs both entries, each exiting 0:

```sh
whitefootc --graph modules.wfg --check
whitefootc --graph modules.wfg --entry kernel -o kernel
whitefootc --graph modules.wfg --entry inspect -o inspect
```

Each module also checks alone against its dependencies' interfaces, and each
entry's composition checks without a build:

```sh
whitefootc --graph modules.wfg --check-module pkg::runtime
whitefootc --graph modules.wfg --check-interface pkg::runtime::queue
whitefootc --graph modules.wfg --check --entry kernel
```

With `--cache DIR`, module and entry verdicts, proof analyses and objects are
reused across invocations; the
[build-cost measurements](../../../experiments/modular-build-cost/RESULTS.md)
time this specimen's checks and builds after an edit.

The application processes two jobs through a four-slot FIFO. The `kernel`
entry keeps everything by value and requires `no_heap`. The `inspect` entry
runs the same batch and copies its report into a heap cell. Both entries check
the resulting fields and return an `ExitStatus`; neither prints output. The
kernel entry is a small allocation-free workload, not an operating-system
boot image or a target-runtime qualification.

The specimen belongs to the [modular-compilation investigation](../DESIGN.md).
Its reader is evaluating source organization, interface completeness, proof
composition and edit impact under the architect/implementer workflow. Keep the
sources and this walkthrough together; replace them when the selected syntax
or application changes, and remove them when they no longer expose a useful
design question. If this becomes a formal compiler regression, extract the
needed cases into the maintained test system. It is not a daily CI input.

## Read the project in this order

1. [modules.wfg](modules.wfg): the entire dependency architecture and both
   named entries.
2. [data/module.wfm](data/module.wfm): two public data records and one generic
   allocating function. Every field is public, so no accessor is needed.
3. [runtime/queue/module.wfm](runtime/queue/module.wfm): the complete queue
   definition with its published readonly ring and every operation's bounds.
4. [runtime/module.wfm](runtime/module.wfm): a client of that API which states
   its own precondition and accepts a function-kind argument with the queue's
   contract.
5. [kernel/start.wf](kernel/start.wf) and
   [tools/inspect/run.wf](tools/inspect/run.wf): complete uses of the same APIs,
   with different entry requirements and different local abbreviations.
6. [runtime/queue/storage.wf](runtime/queue/storage.wf),
   [runtime/queue/operations.wf](runtime/queue/operations.wf),
   [runtime/batch.wf](runtime/batch.wf),
   [runtime/report.wf](runtime/report.wf) and [data/heap.wf](data/heap.wf):
   the implementation behind those contracts.

The two entry interfaces, [kernel/module.wfm](kernel/module.wfm) and
[tools/inspect/module.wfm](tools/inspect/module.wfm), contain only their ordinary
public entry declarations. An entry is selected by the graph; it is not a special
function syntax or an implicit `main` name.

```text
demo/
  README.md
  modules.wfg
  data/
    module.wfm
    heap.wf
  runtime/
    module.wfm
    batch.wf
    report.wf
    queue/
      module.wfm
      storage.wf
      operations.wf
  kernel/
    module.wfm
    start.wf
  tools/
    inspect/
      module.wfm
      run.wf
```

## What the graph says

The directory containing the sole active `modules.wfg` is this project's
source root. No root declaration or project nickname is needed. The fixed
`pkg::` prefix means the current source package, whether it builds a library
or an executable. It does not mean the current directory of the compiler
process or the directory of the individual `.wf` file. This demo has one
source package containing five modules and two named entries.

The five rows register five exact `module.wfm` interfaces. The filename is
always the same; the containing directory supplies the module name. An arrow
below means "may use that module's public API"; it is not a runtime call or
scheduling edge.

```mermaid
graph TD
  K[pkg::kernel] --> D[pkg::data]
  K --> Q[pkg::runtime::queue]
  K --> R[pkg::runtime]
  I[pkg::tools::inspect] --> D
  I --> Q
  I --> R
  R --> D
  R --> Q
  Q --> D
```

Every listed dependency is an earlier row. That local check certifies the DAG;
the compiler does not infer a desired order from source uses. These rows give
permission, not implicit name imports. An earlier row absent from a module's
list is still unavailable to that module's source.

`runtime` is both a module and a namespace parent. Its implementation consists
of **only** `runtime/batch.wf` and `runtime/report.wf`. It does not collect
`runtime/queue/*.wf`. The separately declared child precedes its parent in
the graph, and the parent's row explicitly grants access. Parenthood grants
no private-field access. `tools` has no `module.wfm` and is only a namespace
prefix. The demo root likewise has no `module.wfm` and is not an extra module.

For example, `pkg::runtime::queue` always starts at `demo/` in these sources.
This single-package specimen does not select an external dependency-binding
format or demonstrate cross-package reuse; the standard library is the one
package beyond `pkg` a program will name
([library-modules](../../library-modules/DESIGN.md)), and other external
libraries stay deferred.

The tool reaches across directories to `runtime::queue` without moving that
module to a common ancestor. It lists both `runtime` and `runtime::queue`
because it directly uses both. Removing the latter edge is an error even
though the former still reaches the queue transitively. A future edge from
the queue back to its parent would fail the earlier-row check; retaining both
directions would be a cycle, and row reordering cannot make that legal.

The graph's entries reuse the existing `entry` and `no_heap` words. An
implementer can also run any function of its module as an unnamed entry for
its own tests without editing this file or the module's interface.

## The public boundary and the private implementation

| Module | What the caller can read in its `.wfm` | Implementation ownership |
|---|---|---|
| `pkg::data` | All `Job` and `Report` fields; `boxed_copy`'s complete generic signature and description | `heap.wf` implements the callable and reuses the interface's record declarations |
| `pkg::runtime::queue` | Public capacity, the one complete `Queue` definition with its `public readonly` ring, and the constructor, push and pop contracts | `storage.wf` owns the private ring constructor; `operations.wf` owns the public bodies; both reuse the interface's type definition |
| `pkg::runtime` | `run_two` and the complete required contract of its `take` argument | `batch.wf` calls the private `summarize` in `report.wf` through the shared module inventory |
| `pkg::kernel` | The selected `start` callable | `start.wf` constructs jobs and a stack-resident queue |
| `pkg::tools::inspect` | The selected `run` callable | `run.wf` also invokes the shared heap helper |

Top-level declarations and struct fields are private unless marked `public`,
and only `.wfm` permits that modifier. Neither aliases nor implementation files
can publish a name. `Job` and `Report` explicitly publish both their types and
each field; their complete schemas make them copyable and droppable. Callers
construct them and inspect their fields directly. The interface schemas are
reused inside their owning module; implementation files do not declare another
`Job` or `Report`. Each declaration in a `.wfm` carries a `doc` entry: in the
workflow this is how the architect tells callers and the implementer what a
function is for beyond its checked contract.

`Queue` has its one complete definition in `.wfm`, and its ring is published
as `public readonly storage: Ring<Job, capacity>`. Every module with an edge
to the queue can read the ring in code and in contracts, for example
`deref(queue).storage.len`, exactly as it reads any prelude window's length.
Only the queue module writes it: outside the module the field is never a `set`
target, never passed to a writing parameter such as `place_back`'s window, and
never supplied by construction, so `new` remains the only way to build a
queue. The ordinary component rules make this nongeneric type droppable, and
`nocopy` forbids copying. There is no second definition in `storage.wf`, no
separate capability declaration, no getter and no implicit handle. The
existing `opaque` modifier is not used as a privacy mechanism.

`new_storage` is called from a different implementation file without being
declared in `runtime/queue/module.wfm`. Likewise `summarize` is absent from
`runtime/module.wfm`. All declarations in a module share its inventory,
regardless of source-file order. `capacity`, although declared in the
interface, is reused by the ring's type and the implementation helper; no
duplicated constant can drift.

Each public callable's implementation repeats its full header and contract
without the interface-only `public` modifier; the `doc` entries are not
compared. In `runtime/queue/module.wfm` the job type is called `Job`; in
`operations.wf` it is called `Work`. Both aliases resolve directly to
`pkg::data::Job`, so the declarations must match after resolution. Likewise
`runtime/module.wfm` names the queue type through its `Queue` alias, while
`batch.wf` writes `fifo::Queue` through the module alias `fifo`; both resolve to
one nominal. Result and named-argument labels remain the same.

Every file declares its own aliases. An interface alias is not inherited by
its implementations, and `runtime` does not re-export `Queue` or `Job`.
Clients need their own direct graph edges and aliases. A canonical declaration
identity does not change when one local abbreviation changes.

## The architect and implementer view

An architect agent owns `modules.wfg` and the five `.wfm` files; implementer
agents own the `.wf` files. The queue's implementer can check the queue module
while nothing else is implemented, and the runtime's implementer can check
`batch.wf` and `report.wf` against the queue's interface before `push` or
`pop` has a body. Neither check reads the other module's `.wf`, and an edit to
`operations.wf` cannot change the runtime module's verdict. A declaration with
no definition yet is reported as pending rather than failing the rest of the
module.

An interface edit reaches its implementation and callers explicitly. If the
architect added a requirement to `pop`, `operations.wf` would fail
correspondence until its header repeats the new clause, and the impact report
would name the two entry bodies: their `run_two::<fn fifo::pop>` arguments
would no longer refine the `take` formal under FN-4. Representation choices that callers' contracts rely on, such as
the ring, are part of the interface; an implementer who needs another field
asks the architect for an interface edit.

This ownership boundary does not prescribe one LLVM module or object per
source module. Body proofs and backend partitions have their own tracked
dependencies. Moving a private helper between source files creates no
writer-managed internal link boundary, and final native linking may run in
full. The small module sizes here expose these relationships for reading and
are not a recommendation to split production work this finely.

## The proof and execution story

The compiler verifies the following argument under the visibility, readonly
and result-projection rules of specification v0.70 [MOD-5, MOD-6, TYPE-2,
CALL-4]. Deleting a clause it relies on fails the check where the missing fact
was needed: without `new`'s `ensures made.storage.len == 0_u64`, the kernel's
first `push` is refused under FN-8 because `pending.storage.len < capacity` is
unproved.

| Point in either entry | Queue length known from the public contracts | Why the next operation is permitted |
|---|---|---|
| `new` returns | 0 | `0 < capacity`, where the public constant is 4 |
| First `push` returns | 1 | There is room for the second job |
| Second `push` returns | 2 | `run_two` requires at least two jobs |
| First `take` inside `run_two` returns | At least 1 | The first call's length relation proves the second call's precondition |
| Second `take` returns | Entry length minus 2 | This proves `run_two`'s own length relation; these entries reach 0 |

`pop` satisfies the function-kind formal's exact type, effect and length
contract. Supplying `fn fifo::pop` is an ordinary compile-time argument under
the proposed module names; it does not allocate a function object or introduce
dynamic dispatch. Both entries select the same concrete
`runtime::run_two<fn runtime::queue::pop>` instance. Because `pop` does not
reach back into the entry functions, no recursive component joins the
entries with that instance, and each entry may use `run_two`'s postcondition.

Inside the queue, `push` and `pop` name the published ring's length in
requires/ensures and change it through the ordinary window operations. The
external `run_two` interface and its function-kind formal repeat the same
paths; this is ordinary access to a public field, not a special visibility
rule. `batch.wf` reads `deref(queue).storage.len` directly for the report's
`remaining` field. The `writes(queue.storage)` rows kill overlapping facts
before the verified postconditions supply the new state's facts.
`deref(entry(queue)).storage.len` is the frozen entry datum in an `ensures`,
with no runtime snapshot. `made.storage.len` describes the constructor's
returned value and uses the proposed CALL-4 result-projection admission. In
`new`, the helper's postcondition reaches `storage` at its result destination,
MSR-3's CONSTRUCT placement carries the ring's length into `built`, and the
clause is queried over `built` at the return; at the caller, CALL-4
instantiates it at the result destination, `pending`.

The FIFO bodies select jobs `(tag: 7, payload: 250)` and
`(tag: 9, payload: 10)` in that order. The private report helper computes an
explicit `+wrap` checksum: `(250 + 10) mod 256 = 4`. The intended report is:

```text
Report(first_tag: 7, second_tag: 9, digest: 4, remaining: 0)
```

Both entries should return exit code **0**. Codes 1, 2, 3 and 4 denote a
mismatched first tag, second tag, digest or remaining count respectively.
These checks test application results; they do not repair a failed source
proof. The written contracts state occupancy safety, not a formal sequence
model of FIFO contents or the exact report fields; the `pop` description states
the ordering for its implementer. Both built entries return 0.

The kernel's source composition includes `data`, the queue, `runtime` and
`kernel`. Every definition in those modules receives ordinary checking,
including the allocating generic helper's schema. Its execution closure never
selects `boxed_copy`, `Box<Report>` or their release path, so that helper does
not impose a heap requirement on this entry [STOR-8]. The separate tool module
is not part of the kernel's selected source composition.

The inspect entry selects the same three library modules and its own entry
module. It instantiates `boxed_copy<Report>`, allocates one report cell and
releases it on every return edge. Both `boxed_copy` and the entry write `pure`,
which in WF does not mean allocation-free. `no_heap` belongs to the kernel
entry in the graph, not to any of these reusable modules or functions. Adding
`no_heap` to the `inspect` entry is refused under STOR-8 at `boxed_copy` in
`data/heap.wf`, with the call path from `pkg::tools::inspect::run`.

Generated code and native/runtime supplies respect that distinction as well.
An entry's build emits only the functions its execution reaches, so the
kernel's LLVM declares no allocator and `nm kernel` lists no `malloc` or
`free`, while `inspect` references both. The compiler-owned runtime units call
no allocator; a unit test pins that.

## Notation used here

These sources follow the selected [boundary rules](../LANGUAGE.md) and the
active specification's module grammar [GRAM-2, GRAM-3, GRAM-5].

| Form | Meaning |
|---|---|
| `pkg::`, ordered graph rows and named entries | One implicit source root, exact earlier dependencies and entry requirements |
| `directory/module.wfm` | Complete declarations with their `doc` entries, one complete public representation, explicit `public`, no executable function bodies |
| File-local `alias` headers | Abbreviations with the original identities and direct-edge checks |
| `public readonly storage` | A field every module with an edge may read, in code and annotations, and only the declaring module writes or constructs |
| `reads(...)` / `writes(queue.storage)` | Exact structural effects over accessible paths, repeatable in external wrapper and formal rows |
| `deref(entry(queue)).storage.len` | Frozen mathematical entry value, independent of later mutation |
| `made.storage.len` | Result projection admitted by CALL-4, queried at the return and instantiated at the caller's result destination |

Callers still cannot write, pass to a writing parameter or construct the
queue's ring, and they cannot name a private field in any role. No `observe`,
`use view`, footprint declaration, getter in a contract, trusted axiom,
implicit type invariant, runtime snapshot or mandatory box is needed.

Both entries check and execute under these rules. The larger GrowVector
wrapper/function-kind witness from LANGUAGE.md is still to be built; the FIFO
alone is not evidence for all containers, precise effect combinations or
incremental performance.

## Edits to try while reading

The source results in the middle column can be tried with the compiler; the
incremental work in the last column is the design's prediction, not a
measured invalidation result. Each experiment starts from this specimen.

| Edit | Intended source result | Incremental work that must follow |
|---|---|---|
| Rewrite `summarize`'s body with equivalent operations and unchanged header | Public APIs and permissions remain unchanged | Recheck that helper and any affected private summary users; update code importing its implementation, then link. Do not reprove every library solely because one file changed |
| Move `summarize` to another direct `runtime/*.wf` file, preserving its resolved aliases | Same private declaration identity; still callable from `batch.wf` | Refresh file membership and locations; reuse semantic work only when the resolved declaration/context and dependencies are unchanged |
| Rename the `Work` alias in `report.wf`, updating its uses | Same canonical type, no change in other files | Refresh that file's formation/resolution; normalized semantic results may remain reusable |
| Change the published `Queue.storage` field | An interface change: `runtime/module.wfm`, `batch.wf` and every client proof over the ring may need edits | Recheck the consumers of the field, its capabilities, layout, release, ABI and optimized code; the impact report lists the affected declarations and bodies |
| Add a private field to `Queue` | Other modules' source and proofs are unaffected unless capabilities or residual release change | Recheck the queue module and layout/release/codegen consumers of `Queue` |
| Change `Report`'s public field schema | Callers may need source changes | Revalidate schema, field/type/ownership users and layout/codegen consumers; publication is a real dependency |
| Delete `pkg::kernel`'s direct queue edge while keeping its runtime edge | Reject the kernel's queue aliases/uses | Revalidate the edge/lookup consumers; transitive reachability grants no source permission |
| Change only the tool's aliases or body | Kernel source proofs are unchanged when their actual inputs are unchanged | Recheck tool consumers and its changed specialization/optimizer dependencies; no blanket graph-file or entry key should reprove all shared bodies |
| Add `no_heap` to the `inspect` entry | Reject its reachable `boxed_copy<Report>`/`Box<Report>` heap requirement at `boxed_copy` | Recheck target composition against shared heap summaries; do not reinterpret every function's ordinary proof |
| Add the same storing call to `kernel/start.wf` | Reject the kernel's reachable allocation before optimization | Recheck the changed body, concrete execution closure and target requirement; optimizer deletion is not permission |
| Change an unselected allocating helper to contain an unproved partial operation | Reject the selected module's ordinary source checking | No-heap reachability is not an exemption from source safety |
| Delete the body of `pop` from `operations.wf` | The queue module reports `pop` as pending; `runtime` and both entry modules still check against the queue's interface | Composition, lowering and both entries wait for the definition |

For additional rejection probes, try calling `fifo::new_storage` externally,
writing `pending.storage` or passing `&pending.storage` to `place_back` from
`kernel/start.wf`, constructing `Queue(storage: ...)` outside the queue module,
changing only `push`'s implementation contract, placing a function body in
`.wfm`, adding `public` to any `.wf` declaration, removing `public` from
`Job.tag` while its external uses remain, repeating the `Queue` definition in
`.wf`, rebinding `pkg`, moving an interface to the former sibling location, or
adding an edge to a later graph row. Each should fail for that specific
boundary; an unrelated earlier syntax failure is not evidence for it. The
module-form conformance cases pin the access, publication, correspondence and
graph-order boundaries among them under their rules.

A future review aid should compare the resolved public API, not only changed
lines containing `public`. Changing a published field type or a contract
without editing its modifier must still be reported. Private field changes
that alter public capabilities also matter; layout-only effects can be reported
separately. No comparison script or new review gate is implemented here.

Both entries check and run, the rejection probes reject under their rules and
the kernel artifact has no allocator dependency. Cold/incremental agreement for
these edits, and incremental-work and runtime-performance measurements, remain
to be qualified.
