# Ordinary host values

This records the ordinary-value design selected for C2, researched against
`20043721f68eff0c3bdda72762d8b5221473e600` (the branch's former v0.57).
The combined publication is v0.56 over main's v0.55; the former branch version
names identify historical research, not released archives. The inventory and
migration estimates remain in [INVENTORY.md](INVENTORY.md) and
[EXECUTION.md](EXECUTION.md); the selected alternatives are in
[DECISIONS.md](DECISIONS.md). The active specification defines the language;
this note records its selection grounds, not an independent acceptance rule.

## Boundary

Inside Whitefoot, a value supplied by a host is a value, and a function
implemented by native code is a function. Its provenance grants no additional
declaration class, state identity, effect, obligation source, release policy,
suspension classification, diagnostic origin, or lowering operation. The
ordinary signature, ownership rules, effect row, and written contracts are the
entire source boundary. Native definitions are supplied by the build and link.

The unconditional principle in
[the constitution](../../../docs/constitution.md#article-4-ordinary-objects-io-is-ordinary-program-state)
states the selected boundary. C2 removes the earlier departure clause rather
than making a host-specific distinction an available fallback.

## Declarations and representation

Fold the system declaration inventory into the ordinary PRE-1 inventory. The
current fifteen opaque names become ordinary nominal declarations with no
public constructor or fields. Their exact nominal identity, explicit
linearity, and ordinary type arguments are sufficient for source checking.
An opaque nominal has no implicit close, lease release, or type-specific
effect. An affine opaque value has the empty drop; a `linear` opaque value
cannot reach a scope exit unconsumed under PROV-6.

Ordinary enums keep ordinary variants, fields, matching, and derived linearity.
File, directory and TCP acquisition use ordinary `Result` success/error
branches; `AcceptedConnection` groups the connection and peer address in the
successful accept value. The [acquisition-result decision](DECISIONS.md#acquisition-results)
records why the former permit-return enums are no longer needed.
The exception for a system struct with fields but no constructor disappears.
`TcpConnection` therefore needs a deliberate ordinary API representation;
being supplied by a host cannot prohibit construction of an otherwise ordinary
struct. The alternatives are recorded in DECISIONS.md.

All twenty-nine current callable names become ordinary prelude `fn_sig`
declarations, or are retired by the selected API change. They use the same
function signature model and call checker as other functions. There is no
`SystemCall`, `SystemResource`, system declaration ordinal, inventory version,
or operation-number dispatch in checked IR. Prelude origin ordinals remain
ordinary PRE-1 origins, including for diagnostics and name collisions.

An opaque representation's size, alignment, and ordinary call ABI are build
inputs needed to lay out and link that nominal. They are not a semantic ID or
a qualification contract. A missing definition or incompatible representation
is an ordinary build/link failure, not a source-language verdict. A native
definition must obey its ordinary declaration just as every supplied
implementation must; moving that obligation to the linker does not make a
lying implementation sound.

## Effects and cleanup

Use the ordinary resolved-place basis already used by OWN-6, OWN-7 and ENT-5.
A borrow names its referent; a view names its backing range under CALL-3.
A write at a resolved place kills the facts supported there, then the
callee's ordinary ensures publish the exit facts. FN-9 verifies a Whitefoot
body's selected returns; a linked definition must satisfy its declared
contract under SCOPE-3. Both use the same contract formation and caller
instantiation rules. The actual's spelling and the origin of the callee do
not change that call-site rule.

Remove FN-1's inferred owned-result and exclusive-referent state-routing
summaries and the effects-only identity that follows an owner through calls.
A result receives only its ordinary type, ownership and written contract
facts, as CALL-2 already states for measures. Borrow-result provenance and
slice-origin ceilings remain: those protect ordinary reference lifetimes and
are not owned-state routing summaries.

For example, a helper that replaces the referent of `dst` with `src` and then
reads that referent still accesses the caller's resolved `dst` storage. It
does not acquire a second callable effect root named by the history of the
value now stored there. The D8 routing-dependent effect-cache repair is
superseded by this change, rather than being a prerequisite for it.

Keep ordinary compiler-derived memory reclamation for arrays, runs and boxes,
including the existing provider obligations. Remove the system release table,
its table-local `owner`, target contract, completion-policy classes and
`ReleaseEffectMismatch`. Opaque leaf drop contributes nothing. Reclamation
that already needs a store remains a use of that ordinary store parameter;
its surviving implementation must not call an opaque type's native finalizer.

Every host transition that must be observable has an explicit state operand.
Open/close must not silently obtain a factory or namespace from process
context, and moving a returned handle does not establish an invisible link
back to its creator. State that two operations can both change must be
represented by a shared ordinary host-state operand. Distinct opaque wrappers
alone do not prove that two host effects are independent.

Do not replace the removed ancestry analysis by a renamed host-origin table,
an opaque-type observability bit, or a purity exception for native functions.
The interface choices must survive the same move, framing and call rules as
ordinary values. DECISIONS.md selects direct exclusive factory operands
together with storage-place effects and states the corresponding
counterexample tests rather than assuming framing is harmless.

## Range contracts and result facts

Delete the SYS-8 operand class. Each signature names one actual type. Byte
operations use `&uniq MutSlice<u8>` or `&Slice<u8>`; a buffer caller constructs
the ordinary view first. CALL-3's existing first sentence already classifies
the write as a backing-range write. Its separate SYS-8 extension is redundant
once no legacy buffer is accepted at that same signature position.

The following selected PRE-1 signature uses the existing signature and
contract forms. `ReadStop` is an ordinary enum distinguishing end from a
reported failure; `ReadFile` and `HandleFactory` are ordinary opaque nominals.
The factory and file are explicit exclusive state operands.

```wf
fn read_at(factory: &uniq HandleFactory, file: &uniq ReadFile, destination: &uniq MutSlice<u8>, file_offset: own u64, start: own u64, end: own u64) -> result: own Result<u64, ReadStop> reads(factory, file, destination), writes(factory, file, destination) contract {
  requires start <= end;
  requires end <= len_of(deref(destination));
  ensures when Ok(value: next): start <= next;
  ensures when Ok(value: next): next <= end;
};
```

This is a `fn_sig` record, using the existing GRAM-2 contract punctuation;
it is not a new top-level source declaration form. The ordinary FN-8/FN-9
requirement and publication paths replace `SystemRange` and ENT-3.S10,
including their diagnostics, captured operands and result-parent bookkeeping.

Keeping `ReadBytes(next)` and `ListBytes(next, entries)` as special proof
producers would preserve the violation. FN-9 currently routes only through
`Result.Ok`. Use that existing form: a read returns `Result<u64, ReadStop>`;
enumeration returns three ordinary results: `Result<unit, ListStop>`,
`next: u64`, and `entries: u64`. Its unconditional contract is
`start <= next <= end`. The implementation returns `next == start` on an
error; that behavior is tested and adds no selected-Err proof premise.
A caller can bind all
three, match the outcome later, and retain the numeric endpoint's ordinary
multi-result facts. CALL-4 deliberately defers transporting a selected
enum-bound fact through a later match of an owned local; putting the numeric
endpoint in its own result avoids relying on that absent proof route.
Custom outcome enums remain ordinary data, but no unwritten fact follows
from matching one. Broadening FN-9 or CALL-4 is not part of this removal.

Input validation, short-transfer behavior, portable error classes, pathname
encoding, and close-error policy are ordinary library API contracts and
implementation tests. They do not own language rule IDs. Only requirements
and ensures expressible in the existing proof vocabulary contribute acceptance
facts; stronger prose is not a new proof source.

## Entry

Remove `command`, `command.x as y`, input ordinals, and the associated entry
kind and diagnostics. A build selects and invokes an ordinary function using
ordinary arguments. A prelude input struct is one possible API, not a new
program kind. Its fields receive the same ownership and borrowing treatment
as any other struct's fields.

An ordinary input struct cannot contain `Heap` under the existing provider
storage rules. A design that needs the existing store can pass it as a
separate ordinary parameter; hiding it in a privileged input struct would
recreate the exception. There is likewise no privilege to store a view in the
struct despite STOR-5. These constraints apply to entry adapters as they do to
every other caller.

## Completion and overlap

Erase `may-suspend`, `never-suspends`, result-ready, loan-released and terminal
from source interfaces, checked semantic summaries and overlap permission.
An ordinary call completes with its ordinary results and loans; there is no
earlier source-visible release of an open's pathname borrow. A wrapper has
the same boundary as any other function with that signature.

Native completion queues, readiness waits, stack switching and scheduling may
remain inside the implementation of ordinary calls. Such an implementation
must preserve the ordinary call's lifetime and effect boundary. A compiler
may not recognize `read_at` or a native-function tag to obtain extra source
overlap permission. Any retained generic scheduler path must also apply to an
ordinary Whitefoot function with the same checked interface.

Removing the language mechanism does not by itself establish performance
parity. A global exclusive factory held across a blocking accept can prevent a
connect using that same factory from running; this is ordinary exclusivity,
not a reason for a native exception. Ordinary reservation values or explicit
request/poll APIs are possible API alternatives, but their ownership,
address-stability and capacity behavior must be checked before selection.
No pinning rule, stored loan, future type, callback value or early-loan-release
annotation is introduced here.

## Validation boundary

The proposed implementation must distinguish two checks. Ordinary compiler
conformance checks signatures, linear consumption, loans, effects, range
requirements and postconditions without identifying a host operation. Native
library tests check that the linked bodies actually implement those contracts,
including refusal, short progress, encoding, cancellation and cleanup.

Neither a passing native fixture nor moving a declaration into PRE-1 certifies
the entire replacement. The execution plan must include ordinary-value
counterparts of the current routing cases, native alias and factory contention
cases, and the existing full three-mode runtime evidence. No source verdict is
changed merely because its old implementation path was deleted.

## C1 review evidence

The inventory uses the exact published baseline, not the paused D8 drafts.
Repository `make static` passed. A separate inventory check validated source
link bounds, all 779 manifest case rows and the compiler module register.
These checks validate the proposal's references and repository boundary;
they do not execute the proposed interfaces or establish performance parity.

An independent read-only completion review checked the ordinary-model
constraints, compiler dependency coverage, relevant specification clauses and
selected case migrations. Its findings were corrected: the installed-owner
effect negative is explicitly re-derived under storage-place effects; the
existing TCP constructor with two `unit` fields remains a type rejection;
BEHAVIOR's freshness/routing claims and all identified dependent spec clauses
are included. The reviewer found no remaining publication blocker in that
bounded review and no proved counterexample to ordinary-model expressibility.
Factory contention, native alias/credit/lifetime validation and lost staged
overlap remain implementation evidence to obtain, not completed proofs.
