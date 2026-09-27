# Library modules: the prelude core and the standard library

This investigation decides how the prelude and a standard library become
ordinary modules, the work the owner selected after the modular compilation
PR ([todo item](../../../docs/todo.md)). It owns the design, its measurements and
its rejected alternatives; the decisions that survive go to the design tree,
and the rules they need go to the specification. The owner approved D1 to D5
and D7, now [`language/standard-library`](../../../design/language/standard-library.md),
[`language/name-resolution`](../../../design/language/name-resolution.md) and
[`language/system-interface/declaration-home`](../../../design/language/system-interface/declaration-home.md).
Specification v0.72 states D1 to D4 and the compiler implements all six.
Measurements of the implemented split follow E1.

## Question

Four questions, fixed by the todo item:

1. Which prelude parts stay compiler-owned, and which become source modules?
2. How does a program name library modules? The name-resolution decision
   defers external dependency-name binding, and the modular design keeps the
   compiler-owned prelude out of an ordinary source package called `std`.
3. Do library modules join every check and closure, or only those that name
   them?
4. How are library verdicts, proof receipts and objects reused across
   programs?

## Current state

Facts at `9a858a4c` (main after the modular compilation merge).

**The prelude is 69 hand-written records plus a built-in catalog.**
`compiler/src/prelude.rs` holds 69 records: 18 opaque structs (the storage
shapes `Array`, `Slots`, `Ring`, the cell `Box`, and 14 host handles), two item
files (the structs `TcpConnection`, `AcceptedConnection`, `Inputs` and the
enums `ArgError` through `ListStop`), and 49 function signatures (29 host
functions, 9 construction functions, 9 window operations, `swap`,
`free_empty`). `Bool`, `Option`, `Result`, `Overflow`, `DivError`,
`NarrowError`, `Int` and `Float` are a separate 24-record built-in catalog
(`compiler/src/resolution/catalog.rs`), the only part a test compares with
specification section 14. The 69 records match that section's text, but no
test holds them to it.

**Every check re-parses and re-checks all of it.** `with_prelude_and_modules`
(`compiler/src/source.rs`) appends the 69 records to every module check,
composition and build; they belong to no module, sit in the unit's root scope
and are visible everywhere. The cache keys them only through the compiler's
own executable hash (`compiler/src/driver/cache.rs`).

**The host part is a library in all but name.** No rule outside section 14
names `IoError`, `HandleFactory`, `OutputStream` or any host function; only
the worked example of section 16 calls `exit_status` and returns
`ExitStatus`. Language rules do name `Bool` (conditions), `Option` and
`Result` (ERR-1, `propagate`), `Overflow`, `DivError` and `NarrowError` (the
checked operation table), the storage shapes and `Box` (TYPE-9, STOR-8) and
their construction and window operations (OP-10, OP-13). Beyond that example,
`ExitStatus` and `Inputs` are named only by the runner
(`compiler/src/driver/launcher.rs` matches both by spelling) and by PROG-3's
permission to use `Inputs`.

**The specification forbids external packages today.** PROG-1: "There is no
source include, external package, glob import, source-path search, dynamic
loading or reflection." MOD-8 makes a body-less interface declaration pending,
so no source form of a declaration whose definition the build supplies exists
outside the prelude.

**Libraries exist only as source-bundle text.** `lib/containers/` holds six
container libraries (1,871 lines: grow vector, deque, slab, hash map,
priority queue, ordered map), each a bundle with prefixed global names that a
caller concatenates before its own source. Apart from their own callers in
`tests/programs/containers/`, no program uses them. The I/O programs in
`tests/programs/` instead repeat the same helpers: an 88-line `IoError`
classifier in seven programs, a `write_once` retry loop in six, a `send_once`
retry loop in three, port parsing in four, and zero-filled byte windows
written inline 32 times. A module program cannot reach `lib/` at all: MOD-2
reads only records under its package root.

## Measurement E1: what the host rows cost every check

**Criterion, recorded before the run** (2026-09-25T07:25Z). Same source,
two compilers: (A) the gate build at `9a858a4c`; (B) the same tree with the
45 host records (14 handles, both item files, 29 host functions) removed from
`compiler/src/prelude.rs`, keeping the 24 storage, construction and window
records and the built-in catalog. Primary measure: callgrind instructions;
secondary: median wall time. If B executes at least 30% fewer instructions
than A on a small module check, the host rows are a material fixed cost of
every check; below 10%, they are not.

**Workloads.** A one-function module (`pkg::a`, `public fn one() -> result:
u64 pure`); and the 32-module, 16-function chain that
[`modular-build-cost/run.sh`](../../experiments/modular-build-cost/run.sh)
generates, with the root entry returning `u64` so that B, which has no
`ExitStatus`, reads the same source. Machine: this container, 4 cores of an
Intel Xeon at 2.10 GHz; 15 wall runs for the small module, 11 for the chain
checks, 5 for `--check-modules`.

| Check | A instructions | B instructions | Fewer | A median | B median |
|---|---|---|---|---|---|
| One-function module, `--check-module` | 171,944,274 | 65,613,478 | 61.8% | 21.4 ms | 10.7 ms |
| Chain module `pkg::m16`, `--check-module` | 517,411,939 | 302,111,721 | 41.6% | 54.7 ms | 33.9 ms |
| Chain entry, `--entry app --check`, no cache | 21,572,999,804 | 13,780,549,497 | 36.1% | 1761.0 ms | 1113.9 ms |
| Chain, `--check-modules` | | | | 1852.3 ms | 1206.2 ms |

Process start alone is about 3 ms. Control: a `u32` literal edited into
`m16/body.wf` is rejected by both compilers with the same TYPE-5 diagnostic at
the same column, so B still checks what A checks.

**Result.** The criterion is met: the host rows are a material fixed cost,
paid by every module check and every composition whether or not the module
uses a single host declaration. The chain composition without a cache is 647
ms slower in A: it checks every closure module's verdict and then the
closure, each with the full prelude.

**What the cost is.** The saving grows with the module: 106 million
instructions on the one-function module, 215 million on the 16-function one.
Callgrind attributes it mostly to the checker's nominal passes; parsing is
18.6 million of the 215 million:
`reject_recursive_nominal_layouts` (57.8 million in A, 2.4 million in B),
`nominal_dependencies` (47.8 to 0.8), `ensure_nominals_in_node` (70.2 to
11.7), `instantiate_function_signature` (71.2 to 15.8) and
`validate_generic_templates` (88.7 to 36.8), with 28 million instructions of
`CheckedType` hash-set insertion that vanish in B. The 28-variant `IoError`
and the `Result<_, IoError>` signatures of the host functions feed these
passes, whose work grows with the prelude's nominals times the module's
functions. Part of the cost is therefore an implementation inefficiency that
a fix to those passes would reduce for every check; recorded in
[`docs/todo.md`](../../../docs/todo.md). Removing the rows from checks that do not
name them removes the cost entirely for those checks, and the design below
does both.

**Limits.** B is a probe, not the design: a module that names a standard
library module will process that module's interface, as it processes any
dependency's. The numbers bound what a check that names no host declaration
saves; they do not measure a check that names some.

## Measurements of the implemented split

**E1 again.** The criterion was recorded at 2026-09-25T09:13Z, before either
compiler was built. Compilers: (A) the gate build at `6b66e523`, main before
this branch, with the host rows in the prelude; (B) the gate build of this
branch. W1 to W4 are E1's workloads, byte for byte. W5 is W1 with the
function returning `ExitStatus`, the common program shape: A names the
prelude's `ExitStatus`, B lists `std::process` in the row and names
`std::process::ExitStatus`. W6 is W1 returning `Result<u64, IoError>`,
through `std::io` in B. Predictions: P1, B saves within 5 points of the
probe's 61.8, 41.6 and 36.1 percent on W1 to W3; P2, B executes fewer
instructions than A on W5, and a saving under 10 percent would mean naming
`ExitStatus` brings back most of the host rows, because `std::process`
depends on `std::io`, `std::text` and `std::fs` through `Inputs`' fields;
P3, B executes fewer instructions than A on W6.

| Check | A instructions | B instructions | Fewer | A median | B median |
|---|---|---|---|---|---|
| W1 one-function module | 171,966,487 | 66,939,445 | 61.1% | 19.4 ms | 8.0 ms |
| W2 chain module `pkg::m16` | 517,717,399 | 304,402,511 | 41.2% | 52.4 ms | 33.1 ms |
| W3 chain entry, `--check`, no cache | 21,568,717,726 | 13,863,693,093 | 35.7% | 1776.9 ms | 1226.2 ms |
| W4 chain, `--check-modules` | | | | 1832.9 ms | 1208.9 ms |
| W5 W1 returning `ExitStatus` | 172,352,056 | 149,015,795 | 13.5% | 19.3 ms | 17.7 ms |
| W6 W1 returning `Result<u64, IoError>` | 173,853,395 | 91,058,464 | 47.6% | 19.2 ms | 11.6 ms |

P1 holds: each saving is within 0.7 points of the probe's, and B executes
1.3 million instructions more than the probe on W1, so the standard library
machinery costs a check that names no library module almost nothing. P3
holds. P2 holds by its criterion, 13.5 percent being above 10, but the
common program shape keeps most of the host cost: W5 executes 82 million
instructions more than W1, 78 percent of the 105 million the split saves.
Callgrind puts 37 million of them in the semantic check, mostly the nominal
passes of the [todo item](../../../docs/todo.md) (`ensure_nominals_in_node`,
`reject_recursive_nominal_layouts`, `nominal_dependencies` about 12, 12 and
10 million), 25 million in parsing and finalizing the four interface records
`std::process` selects, and 15 million in resolution tables. Two changes
would each reduce it: a module layout in which `ExitStatus` and
`exit_status` depend on no other module, which is a PRE-2 change for the
owner, and the nominal passes' fix, which helps every check.

**E2: a second program reuses the first one's standard library verdicts.**
The criterion was recorded before the first run: after a first program's
entry check fills an empty cache, a second, different program's entry check
reports every standard library verdict of its closure as reused and executes
fewer instructions than against an empty cache. P1 names `std::process`; P2
has a module `pkg::a` naming `std::io` and a root naming `pkg::a` and
`std::process`, so both closures hold `std::process`, `std::io`, `std::text`
and `std::fs`. Against an empty cache P2 writes verdict and acceptance
records for all six of its modules; after P1, it writes records only for
`pkg` and `pkg::a`, its own modules. Its check executes 1,191,455,739
instructions instead of 1,740,012,428 (31.5 percent fewer), in 106.3 instead
of 155.5 ms (median of 11). The criterion is met. Making it hold took one
fix: a module's graph facts listed a program module registered below a path
a library module also has (`pkg::io::x` below `std::io`) as the library
module's child, so the library module's key depended on the program; a
driver test now holds the key equal across two such programs.

**Front-end times.** wfgrep, a source bundle that names every host module but
`std::net`, checks in 15,220,474,215 instructions under A (main's source)
and 15,243,647,766 under B (this branch's source, the same program with
`std` paths): 0.15 percent more, and 2082.7 against 1954.3 ms (median of 7).
A program that names almost every host module keeps the host cost, as D3
predicts. The chain's times are W3 and W4 above.

## The checker's per-function costs after the split

The owner kept `std::process`'s layout and chose to fix the checker's nominal
passes instead (the second W5 ruling). **Criterion, recorded before any
checker change** (2026-09-25T21:51Z): with H1 the instructions a
one-function module's check spends when its row lists `std::process`
(the same records with and without that row entry) and H16 the same for the
16-function chain module `pkg::m16`, the fix must bring H16 to at most 1.1
times H1, so the host interfaces cost a larger module no more than a small
one, with every test unchanged and no more than 2 percent added to the checks
that read no host interface. Falsifier: H16 above 1.5 times H1 after the
nominal passes keep their results means the growth lies elsewhere.

| | Before | Layout memo | And signature index |
|---|---|---|---|
| H1 | 81,471,646 | 71,759,983 | 71,455,609 |
| H16 | 179,497,960 | 136,483,999 | 121,475,951 |
| H16 / H1 | 2.20 | 1.90 | 1.70 |
| One-function module, no row entry | 66,972,653 | 66,740,416 | 66,767,209 |
| Chain module, no row entry | 304,588,417 | 302,558,318 | 277,305,330 |

**The layout memo.** Every pre-scan of a function, signature or template
ended by walking every nominal of the table for a layout that contains itself
(`reject_recursive_nominal_layouts`), host enums included, so the walk ran
once per function over a table the host interfaces make large. The judgment
reads only the table, so the checker now counts the table's changes (an
instance appended or completed, a checkpoint restored) and walks it again only
after one. **The signature index.** Postcondition selector admission compared
every postcondition record with every signature, recomputing the signature's
path each time; it now indexes the eligible signatures by path once per pass,
which admits the same signatures in the same order.

**Result.** The criterion is not met: H16 is 1.70 times H1, above the
falsifier's 1.5, so the rest of the growth lies outside the nominal passes.
Callgrind puts the remaining 50 million instructions of H16 over H1 in
resolution's table building and public-closure check (about 25 million),
the entailment schedule of the function inventory (about 11 million),
instantiation-cycle rejection (7 million) and concrete signature collection
(6 million); [`docs/todo.md`](../../../docs/todo.md) records them. Against
main, the common program shape now saves 19.4 percent of its check (W5:
138,856,704 instructions against A's 172,352,056), a module that names no
library module 61.2 percent (W1: 66,763,119), the 16-function module 46.4
percent (W2: 277,310,479) and the uncached chain entry 41.5 percent (W3:
12,611,569,297), all from the same sources as E1.

## Design

### D1. The core stays compiler-owned; host declarations become `std` modules

The **core prelude** keeps exactly the declarations a language rule names:
`Bool`, `Option`, `Result`, `Overflow`, `DivError`, `NarrowError`, the bounds
`Int` and `Float`, the storage shapes `Array`, `Slots`, `Ring`, the cell `Box`,
the nine construction functions, the nine window operations, `swap` and
`free_empty`. It stays visible in every module without a name, because
conditions, `propagate`, the checked operations, TYPE-9 layouts and the
OP-10/OP-13 rows are the language's own meaning; a program cannot opt out of
them. The compiler keeps their closed operation identities as it does now
(`design/compiler/prelude-records`).

Every **host declaration** moves into a module interface of the standard
library: the 14 host handles, `Inputs`, `TcpConnection`, `AcceptedConnection`,
the eight host error enums and the 29 host functions, `ExitStatus` and
`exit_status` among them. Ground: no language rule names them, and the
constitution already requires that no declaration, type, ownership, effect,
proof, diagnostic or semantic rule distinguish a value or function by whether
its implementation crosses the host boundary. An ordinary public declaration
of an ordinary module interface is the most direct form of that requirement,
where a compiler-injected declaration visible in every module is not.

Initial layout, ordered by dependency:

| Module | Declarations |
|---|---|
| `std::io` | `HandleFactory`, `OutputStream`, `InputStream`, `IoError`, `ReadStop`, `write_once`, `read_next` |
| `std::text` | `Args`, `HostString`, `ArgError`, `CopyError`, `Utf8Error`, `Utf8CopyError`, `args_count`, `arg_get`, `host_bytes_len`, `host_copy_bytes`, `host_utf8_len`, `host_copy_utf8` |
| `std::fs` | `RelativePath`, `PathError`, `DirectoryRead`, `ReadFile`, `DirectorySource`, `ListStop`, `relative_path` and the file and directory functions |
| `std::net` | `SocketAddress`, `TcpListener`, `TcpReceive`, `TcpSend`, `TcpConnection`, `AcceptedConnection` and the socket functions |
| `std::process` | `Inputs`, `ExitStatus`, `exit_status` |

`ExitStatus` and `exit_status` were the one contested placement: 1,159 of the
1,307 conformance sources name `exit_status` and no other host declaration,
so moving them rewrites those sources (an alias or a qualified path each),
and the worked example's `main` changes with them. Keeping them in the core would
spare that rewrite but put a host handle among the language's own
declarations, with no rule naming it; the runner, which interprets a returned
value outside the source call (PROG-3), can recognize `std::process::ExitStatus`
by identity as easily as by spelling. The owner ruled that they move, and that
rewriting tests is no cost.

Rejected:

- Keeping the whole prelude implicit and adding a separate library beside it:
  the host rows would keep costing every check (E1), and host declarations
  would stay a second, compiler-injected declaration path.
- Moving the storage shapes, `Box` or their operations into `std`: their
  layout (TYPE-9), measures and allocation facts are language meaning the
  checker must know in every module; a module form would add a declaration
  path without removing any compiler knowledge.

### D2. A fixed `std` qualifier names the toolchain's standard library

The standard library is one source package shipped with the toolchain, with
its own graph registering its modules, and the fixed qualifier `std` names it,
as the fixed `pkg` names the program's own package. A module program's graph
row lists the standard library modules its module may name, beside its own
package's modules (`pkg::report: [std::io, pkg::data];`); MOD-5's single
access rule applies unchanged, so a record names `std::io` only when its row
lists it. A source bundle has no row, so it may name every standard library
module; its check and closure include only the modules it actually names,
with their dependencies. Inside the standard library's own records, `pkg`
names the standard library itself, which is the existing meaning of `pkg`:
the source's owning package.

Rejected:

- General external-package binding now (`package name = location;` in the
  graph): it answers third-party libraries too, but needs identity, version
  and renaming rules the owner has not selected, and the standard library,
  shipped with the compiler, needs none of them. It stays deferred, and a
  fixed `std` does not constrain it: a later binding form can treat `std` as
  one pre-bound name.
- An implicit standard prelude, every `std` declaration visible without a
  name in every module: it contradicts the single dependency authority and the
  refusal of glob imports, and E1 shows what injecting unused declarations
  into every check costs.

### D3. Library modules join only the checks and closures that name them

A module check reads the core prelude, its own records and the interfaces of
the modules its row lists, standard library modules included; a composition
selects the closure its entry reaches through rows. A module that names no
`std` module pays nothing for the standard library (E1 bounds the saving),
and a no-heap entry never selects a container module it does not reach. This
is D2's row rule applied to the closure; it needs no rule of its own beyond
MOD-8's existing "the interfaces of the modules it may name".

### D4. The build supplies the host definitions; the specification keeps their boundaries

A host declaration is a public interface declaration without a body, like any
pending declaration. Its definition is supplied by the build from the runtime
units, as the prelude's host rows are supplied today and as PROG-1 already
permits ("Build and link supply definitions for ordinary declarations").
Composition's requirement that every declared function have its definition
(MOD-8) is met by a Whitefoot body or by a definition the build supplies; which
standard library declarations the toolchain's runtime supplies is compiler
metadata keyed by their identities, like the compiler-owned PRE-1 rows, and
never a source marker.

Because those definitions are trusted, not checked, the specification keeps
stating their boundaries: section 14 becomes the core prelude plus the exact
interface text of the host modules of D1. The standard library's Whitefoot
modules, such as the containers of D5, are checked code and are not
specification text; their interfaces are library documentation.

Rejected:

- A source modifier (`extern`, `native`, `host`) on declarations the build
  supplies: it distinguishes declarations by implementation origin, which the
  constitution and `language/system-interface/declaration-home` refuse.
- Moving the host interfaces out of the specification with the rest of the
  library: the safety guarantees depend on the supplied definitions honoring
  their boundaries (SCOPE-3), and the constitution requires the specification
  to state the external conditions its guarantees depend on.

### D5. The container libraries become the first Whitefoot-bodied `std` modules

The owner approved it after asking whether the containers belong in `std`,
and it is implemented: each library is a module directory under
`lib/std/collections/`, its types and API signatures in `module.wfm`, its
bodies unchanged in an implementation record, and its callers name it through
alias headers, with every allocation ledger unchanged. The decisive
ground is reachability: MOD-2 reads no record outside a program's package
root and binding other packages stays deferred, so without `std` a module
program can use a container only by copying its source.

`lib/containers/` moves into the standard library as modules
(`std::collections::vector`, `deque`, `slab`, `hash_map`, `priority_queue`,
`ordered_map`), each with a `module.wfm` publishing its operations and the
fields its callers read; their existing callers in `tests/programs/containers/`
and their allocation ledgers become the regression suite of the move. Helpers
the corpus repeats (a `write_once` loop, an `IoError` classifier, byte-window
filling, decimal parsing and formatting) are library candidates with measured
duplication, but each needs its own interface decision; they follow once the
package works.

An earlier container investigation recorded the owner's lean toward having no
standard library at all
([containers-and-resources](../containers-and-resources/DESIGN.md)). The owner's
selection of this work supersedes that lean; the libraries it produced are the
first content.

### D6. Verdicts, receipts and objects are reused by identity, across programs

A standard library module's verdict depends only on its own records, the
standard library graph, the core prelude and its dependencies' interfaces
(MOD-8), never on the program that names it, so its cache key is independent
of the consumer: two programs sharing a `--cache` directory check each
standard library module once. Proof receipts are keyed by canonical inputs
already. Stable LLVM names derive from module-qualified spellings, so a
non-generic standard library function has one name in every program and its
fragment object is reused wherever its text is equal; a generic instance is
reused wherever its concrete arguments are equal. A toolchain-supplied warm
cache is possible later and needs no new key.

### D7. The standard library's source lives in `lib/std/`, embedded in the compiler

The owner approved it after asking where `std` lives. The source is one
package in the repository's `lib/std/` directory, with its own `modules.wfg`
and one directory per module (`lib/std/io/module.wfm`, and so on), the
containers under `lib/std/collections/`. The compiler carries those records'
bytes from its own build, as it carries the runtime units today
(`compiler/src/backend/runtime.rs`), so the library always matches the
compiler that checks against it, needs no installed location or search
(PROG-1 refuses source-path search), and enters every cache key through the
compiler's identity and its records' bytes. The owner already selected root
`lib/` as the home of reusable Whitefoot source. The cost: editing the library
means rebuilding the compiler, about 80 seconds on this container.

Rejected:

- Locating the library on disk beside the executable, or through an
  environment variable or a command-line root: each lets a program be checked
  against a library other than the one its compiler ships, and no current
  experiment needs to substitute the library.

## Specification changes

Specification v0.72 made them as one amendment, with MOD-10 stating the
standard library package, its `std` qualifier and which of its modules a
program selects:

- PRE-1: the core list of D1; the host records leave it, and its diagnostic
  preorder shrinks.
- Section 14: the host modules' interface text (D4).
- PROG-1: the standard library is the one package outside the program that a
  program may name; no other external package.
- PROG-2: a source bundle may name every standard library module.
- MOD-1, MOD-5: a row may list standard library modules; `std` is a module
  prefix beside `pkg` and aliases.
- MOD-4: an alias may target a standard library path, and a record's alias
  header follows its heap declaration.
- MOD-8: a definition the build supplies meets composition's definition
  requirement.
- TYPE-2: a value of an opaque struct is formed only by a definition the build
  supplies, the construction rows or a host function.
- PROG-3 and the worked example (section 16): `std::process::ExitStatus` and
  `std::process::Inputs`.

## Implementation plan

1. The standard library package: its root in the toolchain, the `std`
   qualifier in paths, rows and aliases, module identities that include the
   package, standard library records in module checks and closures, and
   consumer-independent cache keys.
2. The host modules: the host records leave `compiler/src/prelude.rs` for
   `std` interface files; supplied-definition metadata replaces the prelude
   origin test; the runtime's LLVM unit defines the module-qualified link
   name every module function has (`wf_std.io.write_once`, D6) as a
   forwarding function over the C body `wf__body_write_once`, since no C
   identifier can spell it (a C assembler label spelling it broke Apple's
   linker under full LTO); the runner recognizes `Inputs` and `ExitStatus`
   by identity.
3. The corpus: conformance cases and test programs name the host modules
   through aliases or qualified paths, with every verdict and runtime result
   unchanged.
4. The containers (done): `lib/containers/` becomes `std::collections`, with its
   callers and allocation ledgers.
5. Measurements: E1 again with the real split; E2, a second program reusing
   the first one's standard library verdicts through a shared cache; the
   chain and wfgrep front-end times before and after.

## Open points

- Whether the standard library is versioned apart from the compiler. It is
  not, for now: its records are inputs to the compiler identity's cache scope
  like any other records, and it ships with the compiler.
- The standard library module granularity of D1: the owner adopted it with
  D1. W5 above shows its cost for the common program shape, since
  `ExitStatus` shares `std::process` with `Inputs` and so depends on
  `std::io`, `std::text` and `std::fs`; shown that cost, the owner kept the
  layout and chose to fix the checker's nominal passes, which help every
  check, instead of moving `Inputs` into its own module.
- The nominal passes' growth with prelude size is recorded separately; this
  design does not wait for it.
