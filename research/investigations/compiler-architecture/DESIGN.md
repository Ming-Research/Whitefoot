# Compiler architecture: stage contracts and component boundaries

The owner asked for a rethink of the compiler's overall architecture, not only
a split of its oversized files. This investigation asks whether the structure
still fits the compiler's purpose, a research compiler that must stay correct
and easy to change while the language and the modular build keep moving, and
which structural changes buy the most for their cost. It records the
evidence, the direction it supports, the proposals in order and the
alternatives it refuses. The surviving decisions go to the design tree
through amendments; nothing here selects a design by itself.

Line numbers refer to main at `9a858a4c` unless a line says otherwise.

## Evidence

- **Measurements** over `compiler/src` (non-test code unless stated): file,
  function and `impl` sizes with string literals and comments skipped; the
  names each area imports from the others, resolved to the area that defines
  them; and churn from `git log` since 2026-07-01.
- **Five read-only reviews**, one per subsystem: front end, semantic checker,
  entailment engine, lowering and backend, driver and modular compilation.
  Every finding that carries a proposal was checked against the code, and
  cited line ranges were spot-checked; a claim that rests on reading alone
  says so.
- **Build latency** of the compiler itself, measured below.
- **Recorded decisions** in `design/compiler.md` and `design/compiler/`, so
  that the proposals separate recorded choices from drift.

## What holds

The pipeline's direction is sound and should be kept:

- Stages depend one way: lexer, syntax, resolution, checking, entailment,
  lowering, backend, driver. The one production exception is lowering's use of
  the backend's target layout (`lowering/builder.rs:3`, `builder/split.rs:69`).
- There is one semantic path. Module programs and legacy bundles form ordinary
  bundles and meet the same checker (`driver.rs:2593-2677`); proof receipts
  change only which analyses run (`semantic/check.rs:655-675`).
- The entailment engine is a total, pure function per body with data-only
  results (`semantic/entailment.rs:1167-1205`), one `prove` dispatcher and
  one derivation ledger.
- Lowering reads only the checked program and the backend reads only the IR.
  The IR is a typed SSA control-flow graph with block parameters and drops on
  edges (`lowering.rs:1037-1856`), and target qualification checks its
  obligations before any text is written (`backend/target.rs:504-533`).
- The outcome taxonomy keeps source rejections, unsupported capabilities,
  resource stops and compiler failures apart, and a rejection names its rule
  at the judgment site.
- Tests mostly observe behavior: none of the 1,014 semantic tests constructs
  a `Checker`, conformance uses only driver entry points, and the backend
  tests compile WF source through the ordinary pipeline, 22 of the 33 files in
  `backend/tests/` naming a helper that runs the built executable.

## Findings

### F1. Consumers rebuild what an earlier stage knew

The checker resolves each call argument to its places and substitutes the
callee's effect row, then discards both: `substitute_call_row`
(`semantic/check/expressions/calls/user.rs:509-624`) leaves no trace on the
checked call. Three consumers rebuild the same facts from expression shape,
each accepting a different set of argument forms: the flow's
`argument_referents` (`semantic/entailment/flow.rs:6460-6516`), permission's
`call_projection` and `argument_places` (`semantic/permission.rs:376, 1497`)
and `PlaceMap::for_function` (`semantic/places.rs:900-986`). This contradicts
two recorded decisions in `design/compiler/checker-facts.md` (places shared
"instead of reconstructing it from expression layout"; the flow discharging
questions "instead of duplicating the substituted-row judgment"), and it has
already failed open: inline range actuals once produced no ENT-5 kill and
admitted out-of-bounds reads (`docs/todo.md`, "Consumers rebuild
call-argument referents from expression shape").

The checked program is also completed by patching: call requirements are
installed after phase A, and permission queries, proof data, body disposition
and allocation bounds are filled in later (`semantic/check.rs:1592-1669,
2864-3285`). No type says at which stage a field becomes valid, and the whole
derivation ledger travels inside the checked program
(`semantic/model.rs:2552-2555`).

### F2. Rules implemented twice, with nothing checking that they agree

- **Acceptance.** The checker decides acceptance by enumerating the engine's
  outcome lists in `entailment_rejection` (`semantic/check.rs:3782-4348`,
  567 lines), which also maps obligation families to rules twice and selects
  OP-14 by the callee's spelling `free_empty` (4238). A mandatory outcome list
  added without a matching arm would be accepted silently. The obligation
  contract between the two components is written nowhere.
- **Loop-head kills.** The statement walk forms commit kills in
  `collect_target_kill` (`flow.rs:14070-14118`) and the loop summary forms
  the same events again in `push_commit_kill` (`flow.rs:15817-15860`); a
  continuing event the summary misses leaves a stale fact at the loop head.
  No assertion compares them.
- **Per-path transfer** is sequenced by hand at each entry point:
  `apply_kills`, `kill_scopes_to`, `exit_scopes_to`,
  `exit_counted_capture_scope` and `exit_counted_loops_from`
  (`flow.rs:4529-4790`) each walk the facts, Result states, separations and
  affine state in their own order, so adding a component to the path state,
  as Result transport did, means editing each one.
- **Other pairs:** INV-1 affine formation runs in the checker and again in
  the flow; call-goal images are formed on both sides; CALL-6's consistency
  check keeps its own closure (`semantic/check/publication.rs:73-131`) beside
  the ENT-4 closure the specification names; the backend recognizes OP-11's
  row by symbol spelling (`backend/emitter.rs:605-609`).

The composition cache defect found during this work belongs to the same
class: its key represented each interface record by a map of declaration
digests keyed by role and spelling, which dropped repeated declarations and
header-only alias edits, and a cached check reported an entry accepted that
an uncached check rejected under TYPE-6. It is fixed on this branch with a
regression test (`332f2dbd`).

### F3. Identity is positional and borrowed

`DeclarationId` is a dense index in bundle order
(`resolution/engine.rs:336`), so the same declaration has different ids in a
module check and in a composition. Nodes are named five ways (`NodeId`,
`NodePath`, `SyntaxCoordinate`, source-and-offset keys, `EventKey`), and the
checker joins resolution records by linear scans comparing `(role, NodePath)`
(`semantic/check/support.rs:53-200`). Every stage borrows its predecessor:
228 lines in 62 files name the three source lifetimes (`'classified` or
`<'_, '_, '_>`), `CheckedProgram` holds the whole resolved unit
(`semantic/mod.rs:1232`), and `IrProgram` holds a `_checked` field that no
code reads (`lowering.rs:1845`, set only at `builder.rs:228`).

Downstream code then invents stable names on its own. Proof-receipt keys are
derived `Debug` renderings respelled by stable identity
(`semantic/check/receipts.rs:1-22, 505-650`), and module read sets are rebuilt
from item ordinals (`driver/reads.rs`). The modular-compilation design names
the consequence: "the checked program is one whole-closure structure that
borrows its source text and indexes declarations densely, so a composition
has no module-level checked result to reuse"
([composition staging](../modular-compilation/DESIGN.md#composition-staging)).
Every check also re-forms and re-resolves the prelude and each closure
interface (`driver.rs:2593-2600`).

### F4. Two components are single mutable objects

`Checker` has 49 fields, 21 of them `Cell` or `RefCell`
(`semantic/check.rs:513-648`), and about 31,000 lines of `impl` across 32
files. Five cells are set and restored by hand around clauses and statements,
per-function scratch is cleared by hand on each attempt, and generic
validation takes tables out, truncates others and restores about ten nominal
tables, with a parallel `Stable*` type mirror to survive the rollback
(`semantic/check/generics.rs:102-168, 1213-1313`). Its calls are already
layered (types, then expressions and places, then control, then the program
driver); what holds it together is shared mutable state, not call structure.

`Analyzer` has 37 fields (`flow.rs:1397-1481`) and a 15,040-line `impl` block;
`walk_statement` alone is 1,096 lines. The ledger and the term table are
changed from every region of the file, queries included (an affine query
interns a term at `flow.rs:13478`), while outputs and walk frames each have
one writer. Moving methods into files, the plan `docs/todo.md` described
before this investigation, changes none of this, because child modules use
`use super::*` with `pub(super)` methods; and the file's section markers no
longer match its contents, so the planned cuts would fall in the wrong
places.

### F5. The checker reads raw syntax

The checker makes 522 `self.tree` calls and matches `Production::` 443 times,
probing children to learn which grammar alternative was written
(`semantic/check/control.rs:449-474`). The if/else block split is decoded
from brace offsets twice (`resolution/scopes.rs:218-260`,
`semantic/tree.rs:315-360`), and several stages build their own
terminals-by-owner index. Every grammar
amendment therefore reaches the 36,000-line checker.

### F6. Parallel structure decided mid-translation; emission unstructured

A counted-loop split is decided while its body is being translated
(`lowering/loops.rs:113-123` into `builder/split.rs:246-495`), before the body
exists. The recorded rescue mechanisms follow from that order: transferring
an oversized candidate's finished graph into the parent (`split.rs:691-817`,
which remaps each `IrFunction` field by hand), delayed ordinal reservation and
ledger rotation. Offer policy is spread over group narrowing in lowering, a
scalar-leaf post-pass, a lane-fit filter in the emitter
(`emitter.rs:1139-1158`) and the launcher, and the clone set is computed three
times.

The emitter writes LLVM text with `write!` and then patches it: entry allocas
are inserted by byte offset (`emitter.rs:1476-1479`) and the stack-probe
attribute by rewriting `define` lines (611-646). Which operations open blocks,
and so which label a phi must name, is predicted by a hand-kept list
(`definition_exit_label`, `emitter.rs:2482-2551`) separate from the code that
opens them; `backend/fragments.rs` re-parses the finished text to split it.

### F7. The driver holds rules and native construction; the API is flat

- **Source rules in the driver.** The MOD-8 pending, MOD-9 entry and STOR-8
  heap-closure rejections live in `admit_entry` (`driver.rs:2314-2389`), not
  in `semantic`.
- **Native construction in the CLI.** The runtime-unit inventory, object
  caches, LTO flags and linking live in `bin/whitefootc.rs` (57-219,
  833-1227), with a second unit inventory in `tests/support/mod.rs`, other
  link recipes in the program and conformance harnesses, and a third list in
  `compiler/Makefile`.
- **A flat API.** `lib.rs` glob re-exports seven modules, so every `pub` item
  counts as used and dead code goes unreported (`driver::check_module`,
  `driver.rs:621`, has no caller). Fifteen entry functions come in cached and
  uncached twins, and options get dropped on the way: `--graph --check`
  ignores `--cache` (`bin/whitefootc.rs:484`). `--no-overlap` now selects the
  default lowering while its help text still describes removing the
  completion runtime (`bin/whitefootc.rs:1321-1331, 1705-1731`).

### F8. Machinery with no remaining consumer

- **Region specialization.** Lowering keeps a region-specialization pass whose
  own comment says its environment "starts empty and stays empty"
  (`lowering/specialize.rs:62-71`), with `physical_types.rs` (527 lines)
  around it. Five comments cite `[S20, PROV-1]`, which the active
  specification no longer defines, and a doc comment names a test that does
  not exist (`lowering.rs:1605-1609`; the pin is
  `ordinary_lane_frame_limits_match_the_runtime_slot`).
- **Unreachable kill vocabulary.** The flow's `is_holder` is a `const fn`
  that returns `false` (`flow.rs:4060-4062`), so `EntryImageHolderConsume` is
  unreachable.
- **Hardening that a recorded decision refuses.** `design/compiler.md`
  refuses "re-verifying a previous stage's result inside the trusted path",
  but finalize checks every parsed node's children against its production
  again (`syntax/parser/finalize/shape.rs:207-390`), reporting a mismatch as a
  compiler failure.
- **Double checking.** By reading, every nongeneric body is checked
  structurally twice per compilation. Generic validation returns early only
  when no function template has generic parameters
  (`semantic/check/generics.rs:1206-1212`), and the prelude's signatures, such
  as `box_new<T>` (`prelude.rs:479`), are templates in every bundle. The cost
  has not been measured.

### F9. Changing the compiler costs minutes per edit

Measured on this four-core host with the `gate` profile the tests use,
default Cargo jobs, building `whitefootc` and the library test harness. The
edit appended one newline to `backend/emitter/integer.rs`:

| Build | Compiler binary | Test harness |
|---|---|---|
| From a warm target, as configured | 86 s | 146 s |
| After the edit, as configured | 83 s | 141 s |
| After the edit, with `CARGO_PROFILE_GATE_INCREMENTAL=true` | 3.9 s | 6.5 s |

The profile inherits `release`, which disables incremental compilation, so
every edit rebuilds the whole 215,000-line crate, once for the binary and once
for the test harness.
Enabling incremental compilation made the first build 3–4% slower.

The newline edit is the lightest possible change. Three realistic edits,
each measured on a warm incremental target and then reverted:

| Edit, with incremental compilation | Compiler binary | Test harness |
|---|---|---|
| One statement added to `emit_integer` (backend) | 9.4 s | 13.3 s |
| One statement added to `entailment_rejection` (checker) | 22.9 s | 24.6 s |
| One function appended to `entailment/flow.rs` | 22.6 s | 25.3 s |

Without incremental compilation each of these costs the full 83 s and 141 s.
The cache fix on this branch, which changed two driver files and added a type
and functions, rebuilt the test harness in about 87 s with incremental
compilation. The incrementally built compiler ran the same 157 entailment
tests (two test threads) in 5.44 s and 5.53 s against 5.06 s and 5.00 s,
about 9% slower; the whole suite was not timed.

## Direction

Give every stage boundary an explicit contract, and give every rule one
owner. Each stage hands the next an owned, complete result keyed by stable
identity, and a consumer reads that result instead of re-deriving it:

| Boundary | Contract |
|---|---|
| Syntax to resolution | Owned syntax per record (source id and byte ranges), node indexes built once, and typed accessors that normalize grammar alternatives |
| Resolution to checking | Per-node indexes of declarations and uses, stable declaration keys (module, domain, spelling) with item-relative occurrences, and the declarations each item names |
| Checking to entailment | Checked bodies with a binding table, every argument's resolved places and substituted row, and explicit obligation records (id, rule, site, rendering inputs) |
| Entailment to checking | One disposition per obligation record, published postconditions, and the ledger as its own artifact; acceptance is "every mandatory record discharged" |
| Checking to lowering | An owned checked program built in typed stages, with no reach-back into resolution |
| Lowering to backend | Ordinary IR, then parallel actualization as one IR-to-IR pass that produces a plan the emitter renders |
| Backend to output | A structured function model printed once; fragments cut from the model |

Inside the two large components, state is split along its real writers: in
the checker a growing type context, a read-only declaration inventory and a
per-attempt body checker; in the engine a vocabulary (terms, goals, ledger,
atoms), read-only inputs, outputs and walk frames, with one transfer and join
for the per-path state and one event formation shared by the walk and the
loop summary.

This is not a rewrite. Each step keeps the pipeline and its outputs, and most
steps are verified by identical verdicts and byte-identical LLVM on the whole
corpus.

## Proposals

Each proposal lists what it removes, its cost and risk, how it is validated,
and whether it changes a recorded decision. A proposal that needs a
design-tree change gets its amendment once the owner selects its direction;
the composition-staging revision, which the owner asked for, has been ruled
and applied (`design/log.md`, 2026-09-25).

### P1. Contracts with a correctness edge

1. **Publish call facts once.** Keep each argument's resolved places and the
   substituted row on the checked call, and a per-function binding table;
   the flow, permission and place map read them. This restores the two
   checker-facts decisions and removes the rebuilders' defect class. Cost:
   medium. Validation: identical verdicts, kill, permission and ledger
   results on the corpus, and a deliberately removed checker arm failing in
   one place, as the existing `docs/todo.md` item states. No tree change.
   Done on this branch in part. The flow, the permission judgments and the
   place map now read one exhaustive classification of how an expression
   names caller storage (`named_place`), in place of three shape matches, and
   dropping its range-formation arm fails five tests across kills,
   permission and loop permission. The checker's point-current paths and
   exact substituted row are not published to them: the consumers resolve
   through the function-wide origin inventory, which
   `design/compiler/checker-facts.md` records as an over-approximation, and
   permission substitutes unknown index values on purpose, so reading the
   checker's facts could narrow kills and widen permissions. That is an
   acceptance change, left to a measured comparison and an owner ruling
   (`docs/todo.md`). The corpus emits identical LLVM, diagnostics, exit codes
   and permission ledgers.
2. **Share event formation and transfer.** One kill-event formation for the
   walk and the loop summary, with a gate-profile `debug_assert` that the
   events applied on a continuing path appear in the summary, and one
   `apply`/`join` for the per-path state. Cost: small to medium. Validation:
   identical ledgers on the corpus; the assertion fails when one side drops
   an event. No tree change. Done on this branch: both sides form a `set`
   commit kill through `commit_kill`, as they already formed expression
   kills through `collect_expression_kills`. Where debug assertions are on,
   each path records the kill events applied since its innermost loop head,
   and the back edge asserts that the head's summary holds them all; it fired
   on `wfgrep` when the summary dropped the commit kills and again when it
   dropped the consume kills. The batched and per-event kill transfers apply
   their components through `kill_path_components`, the two scope transfers
   through `exit_scope_components`, and `join_flows` stays the one join. A
   nested loop's own iterations are checked by that loop's back edge. The
   corpus emits identical LLVM, diagnostics, exit codes and permission
   ledgers.
3. **Explicit obligation records.** The checker forms each mandatory
   obligation as a record; the engine returns one disposition per record;
   acceptance is one query, and the rejection builder becomes a rule-to-kind
   table. Cost: medium; many semantic tests read the engine's outcome lists,
   which can stay alongside the dispositions to spare them. Validation:
   identical verdicts, rules and locations on the corpus. Tree: a new
   decision, `design/compiler/acceptance-records.md` (owner-approved).
   Done on the follow-up branch. The checker forms the records in one walk
   over each completed function (`semantic/check/obligations.rs`), exhaustive
   over every statement, expression and place form, once call requirements
   are installed, rather than at each admission site: the walk sees only the
   final attempt's body and cannot miss a form without failing to compile,
   and it is independent of the engine's walk and reachability. The records
   carry the rule, fixed once; OP-14 comes from the checker's operand-row
   table, not the callee spelling. The engine answers each record by its
   site, family and conjunct (`answer_records`), a separation also by its
   query, and records and judgments sharing one identity pair in the order
   they were made, so the contract rests on their counts agreeing. The
   completion review found two separations at one call sharing an identity
   before the query was part of it: a valid program failed as a contract
   disagreement, and a unit test now pins it. Acceptance
   (`semantic/check/acceptance.rs`) reports the first
   undischarged answered record in the former order, and treats unanswered
   records alone, or a judgment that answers none, as a compiler failure,
   since they mean the checker and the engine disagree and are no source
   rejection. The corpus and the module graphs emit identical LLVM,
   diagnostics and exit codes. With the engine's place-subscript judgment
   disabled, 54 programs the unchanged compiler rejects fail closed instead,
   where the former acceptance, given the same engine, accepted 43 of them.

### P2. Component boundaries

1. **Engine sub-contexts, then modules.** Introduce the typed sub-contexts
   first, then move code into `ledger/`, `facts/` and `flow/{domain, events,
   goals, prover, judge, sources, postconditions, invariants, certificates,
   walk, loop_summary, render}`. What must stay together: the one `prove`
   dispatcher (MSR-4's route order is normative), the one ledger, the walker
   owning event order, and fact states with their closure. Cost: large but
   mechanical after the sub-contexts. Validation: identical ledgers and
   verdicts; the 155 entailment tests read only `FunctionEntailment`.
   Supersedes the current `docs/todo.md` plan for `flow.rs`. Tree: a new
   decision, `design/compiler/engine-components.md` (owner-approved).
   Done on the follow-up branch, in three behavior-preserving steps.
   - **Sub-contexts.** The 36 fields became `Input`, `Vocabulary` (the ledger
     and the ordinals numbering its roots included), `Output` and `Frames`.
   - **Receivers.** Each of the 386 methods then took as its receiver the
     narrowest part that it and its callees touch: 76 are on `Input`, 58 on
     `Vocabulary`, 154 on `Reasoning` (inputs with the vocabulary), 22 on
     `Judging` (with outputs), 36 stay on `Analyzer`, which owns the walk,
     and 40 became free functions.
   - **Modules.** The methods moved into the component modules listed above,
     with `sources`, `results`, `conversions` and `operation_facts` kept. The
     types and the entry points stay in `flow.rs`, now 2,366 lines; no module
     exceeds 3,200.

   The corpus and the module graphs emit identical LLVM, diagnostics and
   exit codes after each step, and the unit tests, the entailment tests'
   ledgers included, pass. Main changed the flow while the branch was open,
   and the merge reapplied the three steps to main's flow with the one-shot
   scripts that made them, which reproduce the original steps byte for byte;
   the merged compiler was checked the same way against main's.
2. **Checker components.** A `TypeContext` that can intern during body checks
   (removing `DeferredNominal`'s restarts of whole function walks), a
   read-only `DeclarationInventory` and a per-attempt `BodyChecker` owning its
   scratch; explicit context parameters instead of the scope cells. Then one
   place-elaboration step followed by separate read, measure, borrow, set and
   consume judgments. Cost: large. Validation: identical verdicts; no semantic
   test constructs a `Checker`. Tree: none, unless the generic change below
   is taken with it.
3. **Generic validation without rollback.** Grow-only interning keyed by
   structure, the executable set chosen by reachability, and validation and
   preflight as views over one inventory. This removes the table rollback,
   the `Stable*` type mirror and the preflight duplicates and, by reading,
   the second structural check of every body. Cost: large. Tree: it removes
   the ground of the refusal in `design/compiler/generic-validation-scope.md`
   (identities discarded at the checkpoint), so it needs the owner's ruling.

### P3. Identity and ownership

1. **Resolution publishes per-node indexes.** Build `NodePath` only for
   records and diagnostics, and replace the linear `(role, NodePath)` scans.
   Cost: small. Validation: identical verdicts; no test changes. Done on this
   branch: resolution indexes its declaration, dependent-declaration,
   lexical-use and deferred-use records by owner node, the checker reads them
   by node, and a path finds its node by binary search. The checker still
   builds one path per node for diagnostics. With `--check`, the median of
   nine interleaved runs on four cores fell by 20 to 35% on the
   bundled container programs (ordered map 1.04 to 0.68 s, indexed
   membership 4.19 to 2.78 s) and by 5% on `wfgrep` (2.39 to 2.28 s), with
   identical verdicts, diagnostics and LLVM.
2. **Stable declaration keys.** Resolution mints a key per declaration and an
   item-relative key per occurrence; receipts, read sets and link names use
   them instead of respelled `Debug` text and item ordinals. Cost: medium.
   Done on the follow-up branch.
   - **Keys.** An item's key is the declaration that heads it, by home (the
     module's package, path and which of its records, or the PRE-1 records),
     role and spelling; a file-local alias is keyed by the source that binds
     it. A heading declaration takes its item's key, and every other
     declaration is placed within its item by the child path from the item's
     node and its role and subtoken ordinals, so the variants of two enums
     that share a name stay apart. A node's occurrence key is its item's key
     and the path below it. A repeated key in a resolved unit is a compiler
     failure.
   - **Consumers.** Symbol prefixes, receipts and read sets read the keys.
     Receipts still render the checked function with `Debug`; what changed
     is that each program-wide identity in that text is spelled by the key
     resolution minted, not by a spelling the receipt module assembles from
     declaration records and item ordinals. A function's interface
     declaration and its definition are two keys and still one receipt
     spelling, since a receipt one check records is read by another: once
     the module verdicts of a four-module program record 25 analyses, its
     entry build records none, on main and here, and spelling the two apart
     analyzes one function afresh. A read set follows uses from item key to
     item key.
3. **Owned syntax and an owned checked program.** Tokens become a source id
   and a byte range; `CheckedProgram` stops holding the resolved unit and
   `IrProgram` drops `_checked`. Formed interfaces can then be kept between
   checks, which the owner-selected prelude and standard-library work also
   needs. Cost: large (the 228 lifetime-bearing lines). Tree: this is the
   representation step the composition staging deferred on edit-latency
   grounds alone; see the amendment below.
   Done on the follow-up branch (`IrProgram` had already dropped `_checked`
   with P4.4).
   - **Syntax.** Only four fields borrowed: a span its file, the lexed
     bundle its source bundle, the classified bundle the lexed bundle and the
     parsed bundle the classified bundle; every later stage already held its
     predecessor by value. The source bundle is now a shared handle, a span
     is its source and byte offsets read through that bundle, the lexed and
     classified bundles keep a handle on it, and the parsed bundle owns the
     classified bundle. The classified bundle no longer keeps the lexemes and
     trivia, which no stage after classification read. The three source
     lifetimes, on 239 lines in 55 files, are gone, and the driver's syntax
     step returns the canonical unit instead of lending it to a
     continuation.
   - **Checked program.** Checking borrows the resolved unit, and the checked
     program holds only what checking concluded. The driver keeps the
     resolved unit beside it and hands it to the consumers that read
     resolution records: interface rendering, pending declarations, read
     sets, entry admission and the executable caller.

   Nothing keeps a formed interface between checks yet; module build units
   are that step. Every step emits identical LLVM, diagnostics and exit
   codes on the corpus and the module graphs.
4. **A typed syntax access layer** used by resolution, the checker, the graph
   reader and the driver, with alternatives normalized once. Cost: large,
   migrated file by file. Tree: a new decision (amendment).

### P4. Lowering and backend

1. **Parallel actualization as one IR pass.** Lower the ordinary graph only,
   then outline permitted regions, prune captures, check fit with one target
   query and leave refused regions in place, producing one plan (groups, clone
   set, frontiers, refusal edges, ledger) that the emitter and launcher only
   render. It removes the graph transfer, the hand remapping of `IrFunction`
   fields and lowering's dependence on the target layout. Cost: large.
   Validation: byte-identical LLVM for `tests/programs` and the `--par`
   sources. Tree: it replaces the graph-transfer decisions in
   `design/compiler/parallel-lowering/two-worlds.md` (amendment).
2. **A structured function model for emission**, printed once: exit labels
   become recorded facts, allocas go into the entry block, and fragments are
   cut from the model instead of re-parsed text. Cost: medium to large.
   Validation: byte-identical output, which keeps the backend tests' 547
   substring checks (`.contains(`) as the regression net. Tree: a new
   decision (amendment).
3. **Remove region specialization** down to the call table, reachability and
   interning. Cost: small. No tree change. Done on this branch: the
   `$release$` symbols proved unreachable, since every function has one
   variant, so no spelling had to change, and the corpus emits identical LLVM
   before and after. The checker's own region machinery remains
   (`docs/todo.md`, "Machinery with no remaining consumer").
4. **One target module** for lowering and backend, holding the layout, the
   lane-frame bound and the runtime ABI spellings. Cost: small. No tree
   change. Done on this branch: the IR's definitions moved from `lowering.rs`
   to `ir.rs` and the target layout, with the lane-frame bound, to
   `target.rs`, so lowering names no backend item and lowering, the target
   layout and the backend read one IR module. Rust has no visibility for a
   single sibling module, so the IR fields lowering writes became
   crate-visible; readers keep the accessors. The ABI records in
   `backend/abi.rs` stay with the backend: lowering reads none of them, and
   the launcher that does runs after the backend. The corpus and the module
   graphs emit identical LLVM, diagnostics and exit codes before and after.

### P5. Driver and API

1. **Native construction in the library:** one runtime-unit inventory and one
   link recipe for the CLI and every test harness, returning a structured
   build result that carries the runner status. Cost: medium.
2. **Explicit exports and one request type.** Replace the glob re-exports with
   an explicit list, restoring dead-code detection, and collapse the fifteen
   entry functions into one request or session type, fixing the dropped
   options. Cost: medium.
3. **Entry rules in `semantic`**, next to the other acceptance rules, where
   fact-based entry checks can reuse them. Cost: small. Done on this branch:
   `semantic/entry.rs` judges MOD-8, MOD-9 and STOR-8 and the driver only
   locates and renders the rejection. The STOR-8 heap predicate came along
   from lowering, which the driver had passed into the closure walk; it and
   lowering's executable inventory now read one walk of the checked model
   (`FunctionMentions`). The module graphs, which reach all three rules, and
   the corpus report identically before and after.

### P6. The edit-build loop

Enable incremental compilation for local `gate` builds and keep CI's clean
builds non-incremental. The measured edits rebuild in 10 to 25 s instead of
224 s for binary and tests together, for a test runtime about 9% slower on
the measured subset and a first build 3–4% slower; CI builds from a fresh
checkout, where incremental compilation only adds that cost. Splitting the
crate stays unselected: `design/compiler.md` keeps one crate with private
interfaces, and incremental compilation recovers most of the latency without
new boundaries. Tree: the verification decision on the `gate` profile gains
one sentence, which the owner approved into `design/compiler/verification.md`.
Done on this branch: `compiler/Cargo.toml` makes the `gate` profile
incremental, the five workflows that build it set `CARGO_INCREMENTAL=0`, and
`README.md` says so.

## Order

1. Now, with no tree change: P1.1, P1.2, P3.1, P4.3, P4.4 and P5.3. These
   restore recorded decisions, remove defects or dead code, and are small or
   medium.
2. With amendments ruled: P6, P1.3 and P2.1, then P2.2.
3. Identity and ownership, P3.2 and P3.3, before the prelude and
   standard-library work, which needs formed interfaces that outlive one
   check. P3.4 follows as the checker is migrated.
4. P4.1 and P4.2 when the next parallel-lowering or backend experiment needs
   them, or earlier if parallel work stalls on the current structure.
5. P5.1 and P5.2 whenever a harness or entry point changes next.

Every restructuring step changes no behavior. It is validated by `make
check`, identical verdicts and diagnostics on the conformance corpus and
test programs, and, for lowering and the backend, byte-identical LLVM.

## Relation to recorded decisions

- **Composition staging.** `design/compiler/incremental-compilation.md`
  deferred persistent composition queries until edit-build measurements
  showed that rerunning the composition limits an experiment. The owner said
  that the deferral weighed build cost only, not code structure, and ruled on
  structural grounds alone (F1, F3) that the representation prerequisites do
  not wait: owned syntax, stable declaration keys and an owned checked program
  are built now. Module build units follow them, because the owner-selected
  standard library needs a library module checked once and reused by every
  program; instance units and fact-based entry checks still wait for a
  measurement or a consumer. The ruling replaced that node's first decision
  (`design/log.md`, 2026-09-25); P3.2 and P3.3 are its steps.
- **Generic validation scope.** P2.3 removes the premise of one refusal.
- **Two worlds.** P4.1 replaces the graph-transfer mechanism.
- **Kept:** one crate; one semantic path; a pure engine; textual LLVM output;
  the C runtime outside the Rust guarantee.

## Alternatives refused

- **A rewrite.** It discards the behavior evidence that the library's 1,746
  tests and the conformance corpus pin, and stops the language experiments
  for its duration; each proposal above keeps them running.
- **Splitting files by line count.** It moves methods without separating
  state, and for `flow.rs` the section markers would put the cuts in the wrong
  places (F4).
- **Splitting the crate now.** Its latency benefit is unmeasured beside
  incremental compilation, and it creates boundaries the recorded one-crate
  decision refuses without a consumer.
- **A general query framework now.** The modular design introduces query
  families one at a time as consumers appear; the representation
  prerequisites (P3) are needed first under any framework.
- **Keeping the structure and adding tests only.** Tests can catch a
  disagreement between two rebuilders after it happens; P1 removes the
  second rebuilder.

## Not measured

- The cost of the double structural check (F8).
- Whether the emitter's value-path arms that the place path shadows are
  dead; a coverage run would settle it.
- Whether splicing an oversized loop candidate drops metadata in any current
  program.
- Test runtime of the whole suite under an incrementally built compiler,
  beyond the 157-test subset (F9).
