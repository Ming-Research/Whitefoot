# Machine-checked soundness of the kernel specification

Status board item: `wf-spec-proof`. This record holds the study behind it:
which rules carry the safety claims, how other projects proved such claims
and kept the proof in step with a changing specification, the approaches
compared, the selected design and the first phase's theorem and cost.

## Question

Whitefoot's safety claims rest on the rules of
[the specification](../../../spec/kernel-spec.md), not only on the compiler.
The conformance cases check that the compiler implements the rules; nothing
checks that the rules, taken together, imply the claims. The question is how
to obtain a machine-checked proof that they do, for a core of the rules
first, in a form that stays true while the specification keeps changing.

What the proof covers is the rules. Whether the compiler implements them
stays with the conformance cases and the sanitizer runs; the trusted base
that [SCOPE-3] names (compiler, checker, linked definitions, runtime,
allocator, operating system) stays trusted.

## Why the rules need their own evidence

Rule-level defects, measured from `spec/log.md` and the status board. Each
of these let an accepted program misbehave at run time, and each was a gap in
the rules, not only in the checker, so the fix changed the specification:

| Version | Rules | What the rules admitted |
|---|---|---|
| v0.104 | ENT-3 | a Bool binding's comparison origin ended only at a `set` naming the binding, so a write through a reference left a stale fact |
| v0.129 | TYPE-7 | `&r^` of a range reference named the whole run but was typed as one element; a run of length four discharged a requirement of seven |
| v0.135 | EFF-2, EFF-5 | the call-site overlap check compared only the runtime rows of two reference arguments, while the callee's contract read other fields through them; passing one object twice made a postcondition false, and one variant ended in a segmentation fault (board item `proof-bl-contract-alias-overlap`, six witnesses) |
| v0.137 | FN-3 | `get::<300>()` was accepted for `const n: u8`; the checker reasoned about 300 while the generated code used 44, an out-of-bounds read |
| v0.149 | FN-9, ENT-2 | nothing judged a postcondition's subscripts, so `ensures r <= rows^[i].len` with `i` unbounded was a promise about an element that need not exist |
| v0.151 | PROV-6 | the list of types whose release performs an action omitted `KeySet` and `ConcurrentHashMap`, so a guaranteed tail call could leave a reference to released storage (board item `paged-bl-release-class-audit`) |
| v0.124 | TYPE-9, TYPE-10 | a `swap` of two runtime-capacity window contents exchanged the block headers alone, so a capacity-1 block carried a capacity-64 header and later writes ran past it |
| open | SHARE-2 | several objects in one atomic statement are safe only if they are distinct, a run-time fact the checker cannot see (board item `shared-object-model`; the one-object redesign replaces the rule) |

All were found by hand, by reading code or by a downstream program failing.
A proof obligation of the form "every fact holds of every state that reaches
its point" fails on each of the first six at the rule that admitted it.

The specification changes often. From 2026-09-29 to 2026-10-20 `spec/log.md`
records 75 versions (v0.81 to v0.153), on 14 of those 22 days, up to 13 on one
day. The file is 693,372 bytes with 152 rules; section 15, obligation
discharge, is 210 KB of it.

## What the claims rest on

Read for this study: sections 1 and 4 to 7 (types, ownership, storage,
operations), 8 to 14 (functions, effects, calls, sharing, parallelism) and 15
to 17 (facts and discharge), each by its own reader, with every key line
checked against the text. Line numbers are those of v0.153, archived as
[`spec/kernel-spec-v0.153.md`](../../../spec/kernel-spec-v0.153.md).

| Claim | Rules it rests on | Invariant a proof must carry |
|---|---|---|
| memory safety: no use after free, no dangling reference | REF-1 to REF-3, TYPE-8, STOR-5, STOR-7, OWN-1, LIV-1, STOR-3, PROV-6, WIN-3, SET-1, OP-10, OP-11, with EFF-5, FN-10, HOLD-1 | a reference holding its validity fact resolves to existing, initialized storage of its type; every runtime event that removes or moves storage on its path is a static invalidation event; no value holds a reference |
| memory safety: no double free | OWN-1, LIV-1, STOR-3, PROV-6, WIN-3, SET-1, OP-11, OP-12, OWN-11, OWN-13 | each heap cell has exactly one live owner path, and the static live or dead status of every binding equals its dynamic initialization at every point |
| memory safety: in bounds | OP-4, REF-4, WIN-1, OP-10, OP-13, with MSR-1, MSR-2, MSR-4, ENT-5, ENT-6 | `len <= cap`; logical offsets map injectively into the block; every fact holds |
| no uninitialized read | OWN-1, LIV-1, SET-1, WIN-1, WIN-3, OP-11 to OP-13, TYPE-2, TYPE-6 | every live binding and every slot inside a window holds a whole value, and no point observes a hole |
| no silent overflow; every partial operation proven | SCOPE-2, OP-1, OP-2, OP-4 to OP-6, TYPE-4, CONST-1, TYPE-11, with ENT-1 to ENT-6, MSR-1 to MSR-4, FN-8, FN-9, INV-1, PRF-1, RANGE-1 to RANGE-5 | every fact of the closed state at a point holds in every execution reaching it, so every discharged domain query is true when its operation runs |
| no data race | OWN-7, WIN-2, REF-1, with PAR-1, PAR-2, SHARE-1 to SHARE-3, WAIT, EFF-1, EFF-2, EFF-5 | places OWN-7 calls disjoint have disjoint runtime footprints, and every access lies inside its static footprint |

Four properties of the text shape the proof.

- The specification states no meaning for its facts. No rule says that the
  fact state at a point holds in every execution reaching that point; only
  [SCOPE-3]'s "Accepted programs have no undefined behavior" (line 17) and
  [ENT-1]'s "a wrong derivation is a compiler defect" (line 3460) imply it.
  The model must state it, over a semantics that gives each compiler-owned
  term (captures, commit values, call datums, header atoms) a ghost value
  set at its defining point; lines 3522 to 3523 rely on that reading. It is
  the theorem the proof is for.
- The static judgments depend on one another in a cycle: OWN-7 separates
  places by proved facts (lines 754 to 757), ENT-5 kills facts by OWN-7
  overlap (line 4062), and a window reference stays valid while a numeric
  bound holds (lines 863, 1249). Fact soundness, overlap soundness and
  reference validity therefore need one simultaneous induction over
  execution, not three separate proofs.
- Two checkers abstract the same execution: ENT's place-based facts that
  writes kill, and RANGE's versioned symbolic walk, which judges what ENT
  leaves undischarged and imports ENT's published targets (lines 4581 to
  4598). Both need a soundness theorem against one semantics.
- There is no termination judgment: "v0 provides no termination checker"
  (line 1662), and recursive contracts are "partial correctness, not a
  termination judgment" (line 2177). The theorem is safety and partial
  correctness.

Some rules model cleanly as inference rules: ENT-4's closure, ENT-3's local
sources, MSR-2's standing facts, the join as an intersection, INV-1's
induction, and DIRECT, AUTO and PRF-1 read as "the target follows from a
nonnegative integer combination of at most k premises and bounds". Others are
written as procedures: ENT-3's walk, ENT-5's ordering and transports,
ENT-4.OT, ENT-6's image management, MSR-4's step order and RANGE-2 and
RANGE-3. A faithful model writes the procedures as functions and proves them
against the inference-rule reading, so the ENT-1 claims (one deterministic,
terminating derivation, line 3432) become two theorems: every derivation is
sound, which safety needs, and the procedure computes the specified set,
which line 3449's claim that two implementations agree needs.

Several rules decide by an explicit list of type names: PROV-6's list of
types with a non-empty release (line 824), STOR-3's release actions (lines
905 to 916), STOR-8's no-heap lists (lines 887 and 889), TYPE-9's placement
homes. The v0.151 defect was a type missing from the first list. The model
derives each list from one descriptor per prelude type, so a missing entry
is a failed proof obligation rather than a silent default.

## Earlier decisions this design must answer

- The [specification-representation decision](../spec-representation/DOSSIER.md)
  (2026-08-16) refused "a sibling formal model (Coq/Lean/K/Sail-style, or a
  second executable spec in Rust)", because every negative precedent (KJS,
  JSCert, the WebAssembly provers, Cerberus, Standard ML) was a parallel
  formal artifact on a separate cadence that drifted and then froze, and it
  required that "anything formal must generate or gate from day one". The
  refusal stands for a model on its own cadence. The design below gates from
  day one: a covered rule cannot change without the model changing in the
  same work, and the model's checker is run against the conformance cases.
- The language tree records that the specification stands on its own with
  no ledger connecting rules to their grounds, because a record nothing
  consumes drifts. The proof's coverage list is consumed by a check on every
  change of the specification; it records which sentences the model
  formalizes, not why the rules exist.
- The specification states every normative fact once. The model is not a
  second statement of the language: the specification stays the only
  authority, and a disagreement between the model and the specification is a
  defect of one of them, settled as a specification/compiler discrepancy is.

## Proof assistants compared

Three assistants can carry a language soundness proof of this kind; each has
a large precedent. The criteria are the ones this project's situation makes
decisive: the proof is written and maintained by agents, it must run as a
check on every change of a covered rule, its model must be executable so the
conformance cases can test it, and the later concurrency phase needs a
logic for ownership and interference.

| | Lean 4 | Rocq (Coq) with Iris | Isabelle/HOL |
|---|---|---|---|
| Executable model | the model is a program: compiled to native code with no extraction step; Cedar's Lean model runs millions of differential tests against the Rust implementation [cedar-vgd] | extraction to OCaml; WasmCert-Coq's interpreter [wasmcert-two] | code generation to SML, OCaml or Scala; WasmRef-Isabelle is Wasmtime's fuzzing oracle [wasmref] |
| Arithmetic and search | `omega`; `grind`, an SMT-style tactic whose linear integer arithmetic is complete for the fragment it decides [lean-grind]; `simp`, `decide`; all produce kernel-checked terms | `lia`, `auto`; CoqHammer outside the core | Sledgehammer (external provers, proofs reconstructed), `arith`, `auto` |
| Finding a false lemma before proving it | the executable model and its own generators; Plausible | QuickChick | Nitpick and Quickcheck, the strongest of the three |
| Ownership and interference | Iris-Lean, a port of Iris; the core library is reported complete in 2026 with parts still missing [iris-lean] | Iris, mature; RustBelt [rustbelt] | no framework of Iris's reach |
| Determinism of the check | kernel check deterministic; tactic limits count heartbeats (allocations), not time | deterministic | deterministic kernel; external provers only while writing |
| Agent proficiency | the target of most machine-proving work: AlphaProof, Aristotle, Seed-Prover, the PutnamBench Lean track [alphaproof] [aristotle] [putnambench] | far less | far less |
| CI toolchain | one installer (`elan`), version pinned in `lean-toolchain`, about one release a month | opam; Iris builds take long | a large distribution, about one release a year |
| Language-definition precedent | Cedar: model, proofs and differential tests are a release condition [cedar-blog] | CompCert [compcert], RustBelt, WasmCert-Coq, the SpecTec prover backend [spectec] | seL4, WasmCert-Isabelle [watt-cpp18], WasmRef |

SMT-based verifiers (Dafny, F*) are not compared further: their verdicts can
change with solver version and resource limits, which is the instability the
project's own acceptance rule refuses for programs, and a proof that turns
red without a change cannot gate. Agda lacks arithmetic automation; K and
PLT Redex execute semantics but do not prove theorems about them.

Lean 4 is selected. The deciding grounds are the first and the sixth rows:
the model has to run on the conformance cases on every change, and Lean runs
it with no translation step; and agents write the proofs, and Lean is where
machine proving is strongest. Rocq with Iris is the stronger choice for the
concurrency phase alone; that phase is not first (see [Phase 1](#phase-1)),
and Iris-Lean is the path to reopen if Lean's interference reasoning proves
too weak there.

## Modeling approaches compared

| Approach | What is proved | Faithfulness to the rules | Size | Precedent |
|---|---|---|---|---|
| A. Faithful fragment model, executable | the actual rules for a fragment of the constructs, rule by rule, with the checker as a total function and an interpreter | high inside the fragment; the conformance cases of the fragment test it | grows with the fragment | Cedar [cedar-vgd] |
| B. Idealized core calculus | a small language that captures the idea of the rules | low: the proof is about the calculus, and the rules' details are argued informally | small | RustBelt's lambda-Rust [rustbelt], Featherweight Java |
| C. Full direct model | every rule as written | highest | the whole specification | JSCert and KJS stopped at ES5 [jscert] [kjs]; a-mir-formality is "very incomplete compared to Rust" after years [mir-formality] |
| D. One source generating text and model | the rules written once in a domain language that prints both the prose and prover definitions | exact by construction | rewrite the specification | SpecTec for WebAssembly [spectec] |
| E. Executable model with random testing, no proofs | nothing proved; the model's judgments are tested against its semantics | as A | small | Redex: mechanizing nine ICFP papers found errors in all nine [ryr] |

B is refused because every rule-level defect in [the table above](#why-the-rules-need-their-own-evidence)
was a detail: which reads the overlap check compares, which types a release
list names, whether an origin ends at a write through a reference. A
calculus that abstracts those details proves its theorem and misses them.
C is refused now because 693 KB of prose changing several times a day is not
a first target; A grows into it fragment by fragment. D needs the
specification rewritten in a new language; the representation decision
refused that for now, and WebAssembly's precondition, rules that were formal
before they were prose, is absent here. E is not refused but absorbed: A's
model is executable, so random programs can test it before and between
proofs, and the theorem is what E lacks.

A is selected: the model formalizes the actual sentences of the covered
rules for a fragment of the constructs, as a Lean checker that is a total
function and an interpreter that gets stuck exactly where the specification
gives an operation no meaning, and its theorems say that every program the
checker accepts runs without getting stuck.

## Keeping the proof in step

Three checks, all in one workflow, `proof.yml`, on every push to a pull
request that changes the specification, the model, the conformance cases or
the workflow, drafts included, and on such a push to `main`; the conformance
cases joined those paths with the third check. A pass on the revision being merged is a merge
precondition (AGENTS.md):

1. Sentence coverage. The specification is written one sentence per line, so
   a rule's text is a list of lines. A line the model formalizes carries a
   `-- spec <ID> <hash>` comment above the Lean definition that formalizes
   it, where the hash is a 64-bit FNV-1a hash of the line's bytes;
   `proof/coverage.txt` lists the covered rules, each line left `outside`
   with its reason, and the covered families, every rule of which must be
   covered or skipped with a reason. `lake exe coverage` fails when a covered
   line is neither marked nor listed, when a mark or listing names a line the
   rule no longer has, and when a covered family gains an unclassified rule,
   and prints the lines concerned. Listing an added line as outside is one
   line of the list; a line marked as modeled changes the model.
2. The proof. `lake build` checks every theorem, and the build fails when any
   declaration of the model depends on an axiom beyond Lean's standard three
   (`propext`, `Classical.choice`, `Quot.sound`), which excludes an unproved
   step (`sorry`), an evaluation the kernel does not check (`native_decide`)
   and a new `axiom`. The check enumerates the declarations of every module
   itself, and the coverage check requires the package root to import every
   module file, so no theorem has to be listed to be checked.
3. Differential conformance, once the model parses programs. The model's
   parser takes every conformance case written entirely in the fragment's
   constructs. On an `accept` or `run` case, the model must accept, and on a
   `run` case its interpreter must produce the expected exit status. On a
   `reject` case whose expected rule is covered, the model must reject. A
   mismatch fails the check: either the model is not faithful, or the case
   shows a gap in the rules, and either way the change stops until it is
   settled. Reject cases citing a rule outside the coverage (canonical form,
   diagnostics, the redundancy judgment) are not compared, so the model need
   not reproduce rejections that carry no safety meaning.

A change that edits a covered rule therefore updates the model, the proof
and the coverage list in the same work. That is the rule the project
already applies to everything derived from the specification ("conformance
cases and verdicts, the lexer/parser and generated syntax data, tests and
docs", AGENTS.md), extended to the model, and it is what the representation
decision required of anything formal: it gates from day one.

How much this costs each change, measured on the 75 versions from v0.81 to
v0.153. A sequential-core rule set (SCOPE-2, SCOPE-3, TYPE-1, TYPE-2,
TYPE-7, SET-1, OWN-1, REF-1 to REF-4, OP-1 to OP-4, FN-1 to FN-5, FN-8,
FN-9, CALL-1 to CALL-3, CALL-5, CALL-6, EFF-1, EFF-2, ENT-1 to ENT-6) was
touched by 36 of the 75 (48%); most touched a covered rule in a part outside
the fragment, such as a new prelude type, a shared object or a range
reference, and would have cost one `outside` line per changed sentence. By
reading the 36 summaries, 7 changed what the fragment means (v0.100, v0.104,
v0.109, v0.135, v0.142, v0.149, v0.153), 2 more if loops are in the
fragment (v0.101, v0.106), and 3 are uncertain (v0.120, v0.145, v0.152):
between 7 and 12 of 75, 9% to 16%, would have needed model work. Four of
the seven (v0.104, v0.135, v0.149 and, through its index identity, v0.153)
are changes where a proof obligation would have been checked at the moment
the rule changed. This classification was read from summaries, not from
the rules' diffs, and the first phase's own record replaces it with counts
from the coverage check.

Two weaker policies were considered. A release gate, the policy Cedar uses
("a new version of Cedar isn't released unless its model, proofs, and
differential tests are up to date" [cedar-blog]), lets `main` carry a stale
proof between releases; Whitefoot publishes compiler releases on demand for
downstream projects, so a defect would reach a downstream pin before the
proof saw it, and the catch-up would be the separate-cadence artifact the
representation decision refused. A periodic catch-up with no gate is that
artifact outright. A mechanization kept apart from its standard follows it
at its own pace: WasmCert-Isabelle, a version behind for a time, reached
WebAssembly 2.0 in a 2025 workshop paper [wasmcert-2].

## Phase 1

Three first targets were compared.

| First target | What it proves first | Against it |
|---|---|---|
| A. The sequential fact core: integers, fixed arrays, loops, calls with contracts | that every fact holds in every execution reaching its point, so no unproved partial operation runs | none of the seven defects lies wholly inside it; they need references or prelude types, its next phase |
| B. Ownership and local references: `Box`, moves, releases, references to locals | no use after free, no double free, no uninitialized read, with no fact discharge | the release-list and swap defects need tail calls, prelude types and windows, outside it |
| C. One-object atomic statements and data-race freedom | data-race freedom | the specification defines no location, access or race to state it against, and the shared-object rule is being redesigned (board item `shared-object-model`) |

A is selected. Section 15, the fact language and its discharge, is 210 KB of
the 693 KB and the part whose meaning the text never states; every later
phase needs that meaning, and A states and proves it on the smallest
fragment. A's second phase, references and effect rows, is where the v0.104
and v0.135 defects lived. B is the third phase; C waits for the shared-object
redesign and a memory model, and reopens Iris-Lean.

The fragment, from the section 15 reading:

- integer types and the operations of [OP-2]; `let` and whole-binding `set`;
  `if` and `else`; `loop` with `break` and `continue`, with optional
  difference-bound header invariants; local `Array<T, N>` with constant N,
  subscript reads and element `set`; non-recursive calls with own integer
  parameters, one integer result and difference-bound `requires` and
  `ensures`; `return`.
- its rules: SET-1 for value targets, OP-2, OP-4 for arrays, OP-5, TYPE-4,
  TYPE-5 and FN-1's control-flow graph; ENT-2's terms (a), (c) and (g) with
  difference bounds, normalization and implicit type bounds; ENT-3's
  comparison origin of S1, S4, S5, the `+` and `-` rows of S7, and S16;
  CALL-6's instantiation and establishment with FN-8 and FN-9; ENT-4's rules
  (1) to (3), contradiction and dispositions; ENT-5's supports, kills (a)
  and (d), pre-kill closure, call order, join and ordinary loop head; MSR-4's
  steps 1 and 3 with the subscript and integer-domain normalizations; INV-1's
  base batch and induction for difference bounds.
- left for later phases: references, proof paths and OWN-7; effect rows and
  CALL-1, CALL-3, CALL-5; ownership and release; windows and measures beyond
  `Array`; ENT-6 and PRF-1; ENT-4.OT; exclusions; counted loops; RANGE;
  generics; recursion; atomic and shared state.

The theorems, stated over the model's checker `accepts` and its interpreter
`run`, which takes a step budget and ends `done`, `stuck` (a state the
specification gives no meaning: an exact operation whose domain query is
false, a subscript out of bounds, a read of a binding with no value, a call
whose requirement is false, a return whose `ensures` is false) or
`outOfSteps`:

```lean
theorem accepted_never_stuck (p : Program) (hp : p.accepts = true)
    (args : List Int) (hreq : p.entryRequires args) (steps : Nat) :
    p.run args steps ≠ .stuck

theorem facts_hold (p : Program) (hp : p.accepts = true)
    (args : List Int) (hreq : p.entryRequires args) :
    ∀ point state, p.Reaches args point state → (p.factsAt point).Holds state
```

The first is the safety claim for the fragment; the second gives the facts
the meaning the specification leaves unstated and is the invariant the first
is proved by. Neither says anything about termination, the compiler, or
resource exhaustion, which [SCOPE-3] leaves outside the language.

Phase 1 is done when both theorems check with only the standard axioms;
every line of the listed rules is marked or listed outside with a reason;
the model accepts every accepted and run conformance case in the fragment,
runs every run case to its expected status and rejects every rejected case
whose expected rule is covered, each disagreement settled as a defect of the
model or of the specification; and `proof.yml` stays under 10 minutes on a
GitHub-hosted runner. A text search had estimated about 150 accepted or run
cases and 250 rejected cases in the fragment's constructs; the parser's exact
count is in [the second slice](#the-second-slice-syntax-tree-parser-and-interpreter).

It is built in slices, each a pull request through the same check: the
check itself with the value part of [OP-2] (the first, with this record); the
fragment's syntax, parser and interpreter, compared with the run cases; the
checker, compared with the accepted and rejected cases; the model's defects
the proofs need fixed and the points of a run the theorems are stated over
([below](#model-defects-and-the-points-of-a-run)); the proofs.

Prior criterion for the approach, written before the first slice is
measured. Each slice records its model and proof lines and the turns and CI
runs it took; once the check gates, each specification version records
whether it needed model work. The approach card is reopened if, at the end
of phase 1, `proof.yml` takes more than 10 minutes; if more than a quarter
of the versions in the month after the check starts gating need model work,
or the median such update takes more than one engineer day; or if writing
the checker requires choosing between readings of the text at more than ten
places, which would mean the rules are not yet precise enough for a faithful
model.

### The second slice: syntax tree, parser and interpreter

The second slice adds the fragment's syntax tree (`proof/WhitefootProof/Syntax.lean`),
a lexer after [GRAM-1], a parser over the fragment's productions of [GRAM-2]
to [GRAM-5], an interpreter, and `lake exe conformance`, which `proof.yml`
runs after the proofs.

Where the fragment ends is decided by the parser, not by a text search. It
follows the grammar's decisions and stops with `outside` at every alternative
the fragment leaves out (references, `match`, nominal declarations, `for`,
`move`, floats, character literals, effect rows, result lists and the rest), and
with `reject` where no production derives the source or a literal is ill formed
[FORM-7]. A parsed program is then resolved: a callee must name a function of
the program, `std::process::exit_status` or the prelude's `array_filled`, a
written type must be an integer type, `unit`, `Bool`, `Array<T, N>` of an
integer T or `std::process::ExitStatus`, and a program whose calls reach a
function from itself is outside, since the fragment's calls are non-recursive.
Exact division and remainder joined the fragment in this slice: their value
semantics is two lines of [OP-2] and nine run cases use them.

Four choices shape the tree and the machine for the proofs that follow.

| Choice | Selected | Alternatives | Why |
|---|---|---|---|
| Tree | one plain inductive per syntactic class, statements nested as `List Stmt`, names as strings, no source positions | a tree carrying source spans; a control-flow graph; de Bruijn indices | the theorems reason by structural induction over statements, as the rules state them; [TYPE-6] forbids shadowing, so a name denotes one binding in scope and indices would add renaming lemmas without removing a case; source spans serve diagnostics, which no theorem mentions, and a statement's place in a run is its path through the nesting ([Model defects and the points of a run](#model-defects-and-the-points-of-a-run)) |
| Semantics | a small-step machine: call frames, each a continuation of statement lists, block scopes and loop markers | a big-step evaluator with fuel; a step relation with no executable function | `facts_hold` quantifies over reachable points, which is the reflexive-transitive closure of `step`; a big-step evaluator would need a second, separate reachability relation; an executable `step` is what the conformance cases run |
| Outcomes | `done`, `stuck` in the five meaningless states, `illFormed`, `outOfSteps` | folding type and name failures into `stuck` | a type mismatch or an unresolved name is excluded by typing and resolution, judgments the checker decides without facts; keeping it apart lets `accepted_never_stuck` say exactly that no proof obligation is left undischarged; the fragment reaches no binding without a value, since every `let` has its value and `move` is outside it, so that state waits for a later fragment |
| Contracts | read over the mathematical integers [FN-8] at the call and at the return | erased, as the specification erases them | a false requirement or postcondition is a state the rules give no meaning; the machine must reach it as `stuck` for the theorem to exclude it |

The interpreter takes one step per statement and one per administrative move
(leaving a block, repeating a loop body, returning); an expression is flat
[GRAM-9], so evaluating it is part of its statement's step. A call of a
function of the program stands where [GRAM-5] puts an `expr`: a `let` or `set`
right-hand side, an expression statement, a `return` operand or an `if`
condition; it pushes a frame whose result goes back to that position. A run
case is given ten million steps.

The parser covers the grammar rules it implements, so a change to a production
the fragment uses changes the model in the same work: [GRAM-1] to [GRAM-6],
[FORM-5] and [FORM-7] join the covered rules, with [TYPE-4], [SET-1], [OP-4],
[OP-5], [FN-1], [FN-8], [FN-9] and [PROG-3] for the interpreter. Of the 40 newest
entries of `spec/log.md` (v0.121 to v0.160), 11 name one of [GRAM-1] to
[GRAM-6], [FORM-5] or [FORM-7]; this counts mentions in the entries' text, not
changed lines, so it bounds how many versions would have touched those rules.
A change to a production outside the fragment costs one `outside` line; only a
change to one the parser implements is model work, which the prior criterion
counts.

Results, measured by `proof.yml` on GitHub's Linux runner at the commit
before the slice's review:

| Cases | In the fragment | Outside it | Rejected by the parser |
|---|---|---|---|
| accept, 660 | 39 | 621 | 0 |
| run, 836 | 46, each to its expected exit status | 790 | 0 |
| reject, 1,564 | 84 parse | 1,446 | 34 |

So 85 accepted or run cases and 118 rejected cases are in the fragment, against
the text search's 150 and 250: the search counted recursive programs, programs
with named consts and `Array` uses that come with references. The constructs
that put the most cases outside are nominal type declarations (916),
reference parameters (750), named consts (154), generic nominal types (136)
and generic functions (123), each counted at the first construct the parser
meets. Every case the parser rejects is a reject case. Eight of the 34 cite the
rule the parser cites; the other 26 cite a more specific rule for a source no
production derives, such as [FORM-4] for a comment, [FORM-3] for a keyword in
an IDENT position or [GRAM-9] for a nested operand, where the parser names the
production that stops it, or a rule its order of judgment reaches later, such
as [OP-1] for a retired operation name the parser first rejects as positional
arguments of a call [GRAM-11]. Comparing rejections by rule is the checker
slice's work, and only for cases whose rule is covered.

The boundary is pinned: `proof/fragment-cases.txt` lists the 203 cases the
parser reads to a program or to a rejection, and the check fails when a listed
case falls outside or an unlisted one comes in. Without it a parser defect
that put 112 cases outside the fragment passed a green run. The harness also
runs two programs no run case exercises: a call of a function of the program
in an `if` condition, and a read of a name no binding declares, which is
`illFormed`.

The slice's model is 186 lines of syntax tree, 208 of lexer, 812 of parser and
528 of interpreter, with 148 lines of conformance harness and 29 added to the
integer semantics: 1,911 lines. Of them 1,299 are code, counting the lines
that are not blank, not a `--` comment and not inside a `/- … -/` comment or
doc comment; by the same count the slice had 1,252 when its review began.

The slice took five engineer turns, and one more that a network error ended,
and six `proof.yml` runs to its review: two failed to compile, and one green
run showed a parser defect, the closing `>` of `Array<u64, 4>` read as a
comparison, which put 112 cases outside the fragment wrongly. Two rounds of
review found four defects: the machine could not run a call in an `if`
condition, an unresolved name was `stuck` rather than `illFormed`, nothing
checked the fragment's boundary, and these counts. Fixing them took four more
engineer turns and four more `proof.yml` runs, one of which failed to compile
and one ran with the fragment list empty on purpose, to print the cases to
list.

### The third slice: the checker

The third slice adds the fragment's checker, `Program.accepts`, in five
modules: the judgments that need no facts (`Static.lean`: names [TYPE-6],
named arguments [GRAM-11], types [TYPE-5, OP-1], contract shapes [FN-8, FN-9],
reachability and completion [FN-1]); the L0 relation state (`Facts.lean`:
terms, difference bounds, offset disequalities, the closure of [ENT-4], kills
and joins of [ENT-5]); the affine images with `DIRECT` and `AUTO`
(`Affine.lean`, [ENT-6]); the fact sources of [ENT-3], the disposition of
[MSR-4], joins with header transport, and call obligations and publication
(`Checker.lean`); and the forward walk over statements, loops, returns and
the consistency of published relations [CALL-6] (`Walk.lean`). A program the
checker accepts makes no name or type judgment fail, so the interpreter's
`illFormed` is excluded by the checker as the first theorem needs.

The phase's rule list left [ENT-6] to a later phase, but more than twenty of
the fragment's accepted cases are provable only through it: `requires
2*x <= 8` giving `x <= 4`, a quotient's upper bound, a sum of two nonconstant
operands. The checker therefore includes the affine core: value images, join
images with delta atoms, automatic premises, `DIRECT`, `AUTO` with its two
tightenings, the strongest canonical L0 images, the bridge, the quotient bound
and the interval product. It leaves out measures, tracked fields and the power
atom of shifts, which no fragment construct forms. [ENT-4.OT] is modeled only
as far as origin expansion of established goals; its integer-class
equalities are not.

The covered rules grow by [TYPE-5], [GRAM-11], [ENT-2] to [ENT-6], the
[ENT-3] sources the fragment uses (S1, S4, S5, S7, S12, S13 and S16), [MSR-4],
[INV-1] and [CALL-6]; the lines of [OP-2], [OP-4], [OP-5], [FN-1], [FN-8],
[FN-9] and [SET-1] that the second slice listed as the checker's are now
marked where it models them. Reading each line against the model found eight
places where the model differed from the text, each fixed in the model:
[GRAM-6]'s two `else` shapes; [SET-1]'s unresolved target; [FN-9]'s
requirements that a postcondition name a result, that it have a return to be
proved at, and that a returned value be a term or constant; [ENT-5]'s join
frontier, under which a merge-only continuation passes its incoming edges on
unjoined, so that a loop's induction inputs are the edges and not their join;
the Signed Goal query's affine route, which takes orderings only [ENT-6]; and
[MSR-4]'s signed division targets, the interval product's premise tightening
and step 6's candidates, the live bindings. Two amendments of the main line landed during the
slice: the two lines of [ENT-5] on continuing kill events, one marked and one
read more strictly (below), and [SET-1]'s order, under which a target's
subscript is judged after the right-hand side, so the checker discharges it in
the state the right-hand side and a call's publication leave.

Places where the text admits more than one reading and the checker had to
choose one, the prior criterion's count:

1. `DIRECT` substitutes for each atom "the strongest closed L0/type bound for
   that atom" [ENT-6], but an atom is no L0 term. The checker reads it as the
   atom's interval tightened by the strongest canonical L0 images over that
   atom alone, `x <= c` and `-x <= c`.
2. A condition that is the literal `True()` or `False()` is a goal origin
   [ENT-3], so [OP-5] decides it by [ENT-4]'s implicit proof of the literal;
   the text's goal origin needs "a pure total operation-table row" at the
   root, and a Bool literal is a prelude constructor. The case
   `op5-neg-redundant-literal` expects the rejection.
3. [ENT-3.S7]'s matched product applies at "a later direct ordinary
   exact-multiplication binding"; the checker applies it at a `set` commit as
   well as at a `let`, as the capture itself applies at both.
4. [MSR-4]'s step 6 relates a candidate to "the goal's right-hand term r"; the
   checker forms no bridge when the right-hand side is a constant, whose term
   is Z.
5. A selected return whose returned value is no [ENT-2] term or constant has
   no evaluation of the result datum [FN-9]; the checker rejects it citing
   [FN-9] rather than leaving the relation unproved.
6. The interval product retains "a strictly tighter endpoint when subtracting
   that one premise followed by fixed interval substitution proves it"
   [MSR-4] without saying which endpoint is tried; the checker takes the bound
   interval substitution gives after subtracting the premise.
7. [ENT-6] says an immutable datum "retains its captured image" but not what
   a join keeps of it, while it states for bindings, measures and tracked
   fields that a join keeps an image only when every non-contradictory input
   holds the same one. The checker keeps a call datum's image, and a captured
   division, only when every non-contradictory input holds it, as the
   compiler does; read literally with [ENT-2]'s type bounds on every term,
   the union would let a datum minted on one branch bound a binding on the
   other. Found while fixing the model before the proofs
   ([Model defects and the points of a run](#model-defects-and-the-points-of-a-run)).

Seven places, below the prior criterion's ten.

Three simplifications are known and recorded here rather than counted: a
state is contradictory in the checker when L0 derives `t - t <= -1` or both
signs of one goal are established, so a goal whose negative sign is derivable
only through its operation-domain normalization is not found contradictory;
the `i128` formation limit and the structural ceilings of [INV-1] are not
checked, since no fragment case comes near them; and a continuing kill event
[ENT-5] that writes an index of a binding kills every fact reading that
binding, without proving two offsets distinct or apart, so the checker can
reject a loop the rule accepts, never accept one it rejects.

On the fragment's cases the checker agrees with every one it compares: all 39
`accept` and 46 `run` cases in the fragment are accepted, the run cases
reaching their expected status, and all 86 `reject` cases whose expected rule
is covered are rejected, 35 of the fragment's 119 by the parser and the rest by
the checker. Two disagreements appeared on the way, both errors of the model
and both fixed in it: a relation between two terms whose difference is one
term was read as no L0 fact, and the `\u{H}` escapes of a doc text item were
not checked [FORM-5, FORM-7]. The slice adds 2,730 lines of Lean in five
modules, 504 of them coverage marks, and adds 52 lines to the parser and 70
to the conformance harness. With it the build of the model and harness took
10.2 s and 16.5 s on two runs, against 7.7 s on the main line's run of the
same day; on the slower run every module took 1.5 to 1.8 times as long, the
unchanged lexer included. The build's provisional budget rises from 15 s to
25 s, 1.5 times that first 16.5 s run rounded up to 5 s.

The five modules hold 1,576 lines of code by the second slice's count. The
slice took seven engineer turns and eight `proof.yml` runs to its review: two
failed to compile, one showed the two disagreements above, two exceeded the
build budget on the slower host, and a re-run of one of those passed. The
review found one defect: [OP-5]'s redundancy query bounded its steps by the
goal's own size, so a condition true only through two definition links went
unproved and the checker accepted a program the rule rejects. A program
beside the cases in the conformance harness now checks that rejection; it
failed before the fix, and the fix bounds the query by the goal's complete
expansion plus the links it may follow. The fix, with main's [SET-1]
amendment merged in, took three more engineer turns and two more `proof.yml`
runs, the first red on that program alone as expected.

### Model defects and the points of a run

Before the theorems can be proved, the model needed four changes, made in one
pull request ahead of the proofs (board item `spec-proof-model-points`).

Two defects let the checker accept a program that overflows at run time.
Both were read from the source and confirmed by running the model and the
compiler on the program below; the model accepted it and its run stopped at
`y0 + 1_u8`, and the compiler rejected it citing [FN-9].

```wf
fn g(v: u8) -> r: u8 pure contract {
  ensures r - r + v != v + 1_u8;
} {
  return v;
}

fn main() -> status: std::process::ExitStatus pure {
  let a = array_filled::<u8, 1>(value: 255_u8);
  let y = a[0_u64];
  let y0 = y;
  if y < 255_u8 {
    set y = y + 1_u8;
    let w = g(v: y);
    set y = w;
  }
  let z = y0 + 1_u8;
  return std::process::exit_status(code: z);
}
```

- [FN-9] requires an operand to contain a result datum, and an operand is a
  relation term, the side after its coefficients are summed. The model
  looked for the result among the written names, so `r - r` counted. It
  now reads the names the normalized sides carry, at the shape check, at a
  return's entry-image check and where a call's publication skips a relation
  that carries the result but has no destination.
- A join took the union of the inputs' call datum images. On the then path
  the publication above gives the datum of `y` a disequality with itself
  that holds on every input, so it survives the join, the datum keeps its
  `u8` bounds, and through its image they read as `y0 <= 254` on the else
  path, where `y0` is 255. A join now keeps a call datum's image, and a
  captured division, only when every non-contradictory input holds it, as
  the compiler does; the text leaves this open (reading 7 above). For
  divisions the change accepts the same programs, since a quotient atom
  minted on one branch appears in no image after the join (deduced
  from how a join forms images).

The theorems quantify over every `Program`, not only those the parser
produces, so the checker now also judges each literal's range [FORM-7], which
only the parser judged; a literal such as `300_u8` would make the facts
contradictory and every obligation dischargeable.

The second theorem speaks of the facts at each point a run reaches, which
needs points. A point is a function's name and a statement's position, its
path through the nesting: the statement at index `i` of the block at `b` is at
`b ++ [i]`, the body is the block at `[]`, and an `if` or `loop` at `q` has
its blocks at `q ++ [0]` and, for `else`, `q ++ [1]`. The interpreter's
statement continuations and loop markers carry the position, and the walk
records, on entering each statement, the bindings in scope, the fact state
and the atoms minted so far. `Points.lean` defines the theorems' premise
(`main` exists, takes no parameter, and its requirements hold), the snapshot
at a point (a contradictory one where the walk recorded none) and
reachability (a run from `main` takes steps to a configuration whose running
frame is about to run that statement). A path was chosen over a numbering of
statements in walk order, which the interpreter would have to recompute, and
over a source span, which no theorem needs and a tree built directly lacks.

The conformance comparison checks the four changes. The program above,
beside the cases, must be rejected citing [FN-9]; the comparison failed on it
alone with the model before the fix. A tree with `300_u8`, built directly,
must be rejected citing [FORM-7]. Every case the checker accepts is run from
`main`, and each point the run reaches must have a snapshot whose bindings
have the frame's names in its order, so the interpreter and the walk agree on
every position and scope.

With the changes the comparison still agrees with every case: the 39
`accept` and 46 `run` cases in the fragment are accepted, the run cases
reach their expected status, and the 86 `reject` cases whose rule is covered
are rejected. No program was found that the union of call datum images lets
through once [FN-9] reads normalized sides, so that change shows in no case;
it brings the model to the compiler's join and lets the join's soundness be
proved input by input. The point check made the conformance run take 5.1 s
against 0.9 s on the main line's run of the same day, within its 10 s budget;
the build took 15.9 s against 16.7 s.

The slice adds 210 lines of Lean to the model and removes 80, 79 of the added
lines in the new `Points.lean`, and adds 78 lines to the conformance harness
and removes 5. It took two engineer turns and four `proof.yml` runs to its
review: the run that watched the comparison fail on the program above, one
that failed to compile for a missing import, and two green. The review found
two defects: the out-of-range literal check and the point check had not been
watched failing, and this section's place among the slices and its counts
were missing. A temporary workflow then ran the harness three times on
deliberately broken models, one per condition: the literal check removed, the
walk recording no snapshot inside a block, and each snapshot dropping its
newest binding; each run failed on the condition it targets.

### The L0 lemmas

`FactsSound.lean` gives the L0 relation state its meaning and proves each
operation keeps it. A valuation gives Z 0 and a sum the sum of its sides; a
state is satisfied when it is not contradictory and every stored bound and
disequality holds. The closure is sound because each matrix step, the
transitive one and the disequality strengthening alike, lowers an entry only
to a value the valuation already bounds, so every entry stays a true bound and
no diagonal entry turns negative; the proof needs neither the matrix's shape
nor the closure's fixed point. Queries, kills and additions follow directly.

The join needs a stronger premise than "the terms the taken input and the live
list name lie in their types". A sum term names atoms that another input can
name on their own, and the join then adds those atoms' implicit type bounds.
With `p2 : u8` live and valued 0, a taken input holding `z - (p1 + p2) <=
-1000` and another holding `p1 - p1 != 3`, the join keeps the disequality, its
closure bounds `p1` by 255, and any valuation agreeing on `p2` and on `p1 +
p2` gives `p1` a value of at least 1000. Inferred from the definitions; the
example was not run. The lemma therefore asks that the atoms of those terms lie in their
types, which a valuation read off a running frame satisfies, and builds the
new valuation atom by atom: the taken input's atoms keep their values and every
other atom takes 0, a member of every fragment type. A second form needs no
new valuation when every term already lies in its type.

The module adds 618 lines of Lean and no coverage mark. Before the model
changes above were merged in, it built in 2.3 s inside a 17.2 s build of the
model, within the 25 s budget, on the fourth `proof.yml` run: the first failed
in one lemma, the saturation step, and the third on normalizing steps that
changed nothing.

### The affine lemmas

The affine proof part, `AffineSound.lean`, gives the affine forms of [ENT-6]
their meaning under a valuation of the atoms and proves that `DIRECT`, the two
integer tightenings, the residual check and `AUTO` answer "holds" only for an
inequality that holds whenever the view is satisfied: not contradictory, every
atom in its interval, every published premise and every L0 image holding.
That the images hold, given the state's L0 relations, is left to the
discharge proof, which has the L0 satisfaction. The soundness of a
tightening does not depend on how its factor is chosen: dividing a holding
inequality by any factor of every coefficient and rounding the bound down
gives one that holds. The part adds 347 lines of Lean in one module and
changes no model definition. The build of the model and harness took 16.2 s,
10.1 s and 18.1 s on its three green runs, within the 25 s budget. The part
took five engineer turns and four `proof.yml` runs to its review, one of which
failed to compile: the toolchain's `Int.fdiv_eq_ediv` carries no sign
hypothesis.

### The typing lemmas

`Typing.lean` proves that an accepted program never stops ill formed:
`Program.never_illFormed` says that for a program the checker accepts and
whose `main` entry holds (`EntryHolds`), no number of steps of the interpreter
ends in `illFormed`. It rests on a well-typedness of interpreter states,
`Config.WellTyped`, with three lemmas: the entry state is well typed, `step`
keeps a well-typed state well typed, and a step from a well-typed state is not
`illFormed`. The statements follow the interface `spec-proof-theorems` fixes
for the theorems that use them.

A state is well typed when the checker accepts the program and every frame
agrees with a context the checker judged its continuation from. A value has a
type when its shape matches and an integer lies in its type's range, so the
range part is available to the fact proofs. A frame's bindings match the
context's scope name by name and type by type. Its continuation is accepted
statement list by statement list, under two flags the checker's flow carries:
whether the next statement can be reached normally, which only needs to be an
upper bound of the checker's flag, and for each enclosing loop whether its
exit is reached normally, which is what a counted `break` needs. The frames
below the running one take its result at their destination: the condition is
stated for every value of the result type rather than through a stored
context, since a `set` or `if` destination resumes in a state that does not
exist until the value arrives.

The module adds 1,642 lines of Lean, changes no model definition and adds no
coverage mark. The build of the model and harness took 21.9 s to 23.4 s on
its three green `proof.yml` runs, within the 25 s budget, with the module
itself 6.0 s to 6.5 s; the first run on `main` after the merge took 26.0 s,
and the budget became 30 s, 1.5 times the 16.9 s of the first run on the
merged head rounded up to 5 s. The part took seven engineer turns and five
`proof.yml` runs to its review. The first turn only studied the question and
ended on an API error; the first draft was written without a local Lean and
failed to compile in about 30 places, the second run in 9, and the two later
runs changed only warnings and this record.

### The generator lemmas

`GenGrows.lean` proves that the checker's walk only grows its generator, the
state that numbers atoms and records snapshots: the counter never falls, every
atom registered on the way has a number between the old and the new counter
and a nonempty interval, and the snapshot table only gains entries at its end,
at points below the block walked and each recorded once. A function's final
table therefore holds each point at most once. The delta atom a join
registers needs its own proof that its interval is nonempty: its ends are the
least lower and the greatest upper end of the inputs' constant intervals, and
each input's interval stays nonempty through the fold that builds it. To state
the lemmas over named functions, the loop head's state and a function's entry
state move out of the walk into `Checker.loopHead` and `Checker.entryState`,
with no change of behavior; the model's run over the conformance cases is
unchanged.

The part adds 740 lines of Lean in one module and no coverage mark. The
module alone built in 14.8 s and 15.6 s, and the build of the model and
harness took 26.9 s and 38.6 s on the first two `proof.yml` runs that
compiled it, against 16.5 s and 16.9 s on the main line's runs of the same
day. With the typing lemmas merged in from the main line it took 24.2 s,
42.2 s, 29.5 s and 41.4 s on four runs of the same content, and the build's
provisional budget rises from the main line's 30 s to 65 s, 1.5 times the
longest of them rounded up to 5 s: a budget from the first, faster run failed
on the slower host. Four earlier runs failed to compile: a tactic macro could
not resolve lemma names defined after it, then, once it could, a lemma's
proof referred to itself, and two proofs needed steps the tactic did not take.

### The discharge lemmas

`Holds.lean` gives a snapshot of the checker's facts its meaning on an
interpreter frame, the frame's bindings and its parameters' entry values: the
frame fits the binding table, and some valuation of the atoms and of the terms
makes every part of the fact state hold of it, the L0 relations, each integer
binding's image, the images of call data and returned values, the automatic
premises, the signed goals, the origin links, the stable parameters and the
captured quotients. The valuations are existential because the checker cannot
name the runtime value an atom stands for: the delta atom a join registers
stands for a different input's value on each path. One valuation serves every
part because the L0 images and the bridges tie terms to atoms, so an image is
shown to hold under the valuation that satisfies the L0 relations, the step
the affine lemmas left to this part. A contradictory state holds on no frame.
A snapshot is also fresh below the generator's counter, every identity it
names lying below it, so an atom minted later may take any value in its
interval without disturbing what already holds.

`DischargeSound.lean` reduces the disposition [MSR-4] to what a query means:
each of its forms, the signed goal, an L0 normalization whose facts hold,
every affine target holding (a target with a bridge holds once its left side
is at most the bridge's term), an affine alternative and the interval
product, implies the obligation. Each obligation class then proves what its
queries mean once, instead of each class unfolding the disposition: relation
queries for requirements, postconditions and invariants [ENT-2, ENT-6],
subscript queries [OP-4], and exact operations' domain queries [OP-2], where
the operands' intervals and the interval product give a multiplication's
range. `ObligationSound.lean` and `ValueSound.lean` state the obligations in
the form the interface of `spec-proof-theorems` fixes: an accepted right-hand
side that calls no function of the program, and a subscript target, never
stick; a proved invariant holds; a published call's arguments never stick,
lie in their types' ranges and satisfy the callee's requirements [FN-8]; a
return's check gives the postconditions of the returned value [FN-9]; and the
checker's value of each right-hand side describes its runtime value, which
the transfer lemmas use.

Reading the model for the proof found a defect: a disequality relation has no
affine normalization, yet `relQuery` gave it an empty list of affine
components, and the disposition counts a list whose every component holds as
proved, so the checker proved every `requires x != y` at a call and every
`ensures r != v` at a return. The model's checker accepted a call
`divide(n: 84_u64, d: 0_u64)` of a function that `requires d != 0_u64`, and a
return of the parameter `v` under `ensures r != v`; with the fix both are
rejected, citing [FN-8] and [FN-9], and two checker examples in the
conformance harness keep them so, watched failing on the main line's checker
by a temporary workflow. The fix gives a disequality no affine component, so
its L0 facts alone prove it, as [ENT-6] normalizes only order relations and
equalities. With the empty list put back, the proofs no longer check: a
temporary workflow watched the build fail at the disequality case of
`bridges_spec`, the lemma that a relation's affine components give the
relation. The disposition's two other lists that count as proved when every member holds,
an L0 normalization's components and the affine alternatives, are never
empty, read from the source.

The statements follow the interface with four adjustments: the lemmas that
mint atoms take the snapshot's freshness; the call-requirement and invariant
lemmas take the static judgment in the binding table's scope; the return
lemma takes the state its check reads and a checker value that describes the
returned value, since a return gives a computed integer expression a returned
value before the check, a step whose lemma belongs with the transfer lemmas
beside `set`'s, which uses the same parts; and a value-level lemma for
right-hand sides is added.

The part adds 5,015 lines of Lean in four modules, changes the model's
`bridges` and its one use, and adds no coverage mark. The build of the model
and harness took 56.5 s and 55.5 s on the first two `proof.yml` runs after the
main line was merged in, the new modules taking 1.6 to 1.8 s, 4.9 s, 20 s and
5.2 s beside the generator lemmas' 29 s, and 41.9 s on the third, on a faster
host: there the parser built in 2.3 s against 3.1 s and 3.2 s, and the new
modules in 1.4 s, 3.6 s, 14 s and 3.0 s beside 20 s. The build's provisional
budget rises from 65 s to 85 s, 1.5 times the 55.5 s of the final content's
first run, rounded up to 5 s. Writing the part to the commit handed to review
took nine engineer turns, two of them ending on network errors and one stopped
by a pause. The draft was
written without a local Lean and compiled outside the package by a temporary
workflow: seven of its nine runs failed to compile, at between 1 and 28 places
each. Moved into the package, the proofs checked on every `proof.yml` run; one
earlier run failed only on the build's budget of the time, 25.6 s against
25 s, on package content another run had passed with.

## Sealed type invariants

Board item `spec-proof-sealed-invariants`. The amendment that
`firn-req-invariant-through-fields` makes rewrites TYPE-11 and adds its parts
S1 to S8: an affine type invariant travels with a value into fields, payloads,
`Box`es and containers, a body may break it between two writes, and the body
must restore it before it hands the value on. Until the amendment merges, its
text is on that item's branch (Whitefoot#484, head `07d856f2c`). Cut as the
coverage checker cuts a rule, the nine rules are 63 lines: TYPE-11 11, S1 4,
S2 6, S3 14, S4 13, S5 4, S6 4, S7 3 and S8 4. The main line's TYPE-11 has the
older form, and the model's fragment has no structs or references.

Four review rounds of Whitefoot#484 in a row reported defects, and two were
soundness gaps in the rules themselves, each where a rule met a write through
a reference that may name several places: in the fifteenth round a whole-value
store, in the sixteenth a `swap`. A proof that every event keeps an invariant
checks each such combination: the lemma for a combination the rules get wrong
does not go through.

### Approach

| Order | What it does | Trade-off |
|---|---|---|
| A. The rules first (chosen) | the rules' sentences become a component's functions, over the values and places the model's interpreter will hold; REF-1's resolution and OWN-7's overlap, short where TYPE-11 uses them, are formalized and proved with it; the fact core and the effect rows are premises | checks the layer where the gaps were, each row against each way of writing; the end-to-end step joins later |
| B. Structs and references in the fragment, end to end | the model's syntax, parser, interpreter and checker gain structs, fields, references, effect rows and `swap`, and the fact core's simulation grows with them | the theorem is about accepted programs, but it waits for the fact core's simulation and most of the references part, which it duplicates |
| C. Wait for the references part | that part models these rules with the rest | nothing is duplicated, but the rules' implementation, and the work waiting on it, waits for that part and the fact core |

All three end with the rules formalized sentence by sentence in the executable
model and proved end to end; they differ in which part comes first. A is not
the idealized calculus that [Modeling approaches
compared](#modeling-approaches-compared) refuses: its definitions are the
specification's sentences, its run holds the values and places the interpreter
will hold, and the walk will call its functions, so each theorem below is one
step of the end-to-end induction, as the fact core's lemmas are. The
end-to-end theorem, that an accepted program hands off only valid values,
needs the fragment to have structs and references and the fact core's
simulation to be proved; it then discharges the premises below. That step is
proposed for `spec-proof-refs`.

### The component

`Sealing.lean` holds the definitions. The checker's side is the sealing state
of one body at one point, its debts and each reference variable's members,
and one function per judgment: each row of TYPE-11.S3, the hand-off of
TYPE-11.S4 with the restoration and satisfaction of TYPE-11.S5, the merge and
loop header of TYPE-11.S6 and the instance of TYPE-11.S7. `Ctx.step` applies
an event's rows: a commit or update, a `let`, a `swap`, a call, the death of a
root binding, the binding of a reference, and a kill batch alone; it rejects a
`swap` that OP-11 rejects, one whose targets have members that may overlap at
different levels. The run's side is a store of values at roots, local bindings,
by-value parameters and the referents of reference parameters, and the path
each reference variable names. A value is a tree whose children are a
struct's fields, a `Box`'s content, an `Array`'s elements or the payload of
the variant an enum holds. Every step of a place selects one value, so a
place's level is its number of steps; range, page, `Segments` and window
steps, which select runs, are outside the component.

The invariant every theorem keeps is `Ctx.Holds`: at every place of the run,
each affine invariant of the struct there holds unless a debt covers it, a
debt that names the invariant at an anchor denoting the place, its recorded
reference and projection denoting the place too, or a cover above the place
(`Ctx.Sealed`); and each reference variable's path in the run is denoted by
one of its members (`RefsOk`).

### What the proofs assume

The premises are stated in `Sem` and `Ctx.RunStep`. Each is a judgment the
component reads and does not make, and the end-to-end proof discharges each
from the rule named:

1. `Sem.distinct_sound`: two offsets the facts prove distinct differ in a run
   where the facts hold. The fact core's soundness, ENT-6.
2. `Sem.proves_sound`: an instance the facts prove holds at the place the
   spelled place denotes. The soundness of MSR-4's disposition, with its
   `invariant` and `use` certificates.
3. `Sem.Frame`: a term the kill batch does not kill keeps its value, and a
   reference variable it does not rebind or end keeps its path. ENT-5's kill
   judgment.
4. `Ctx.RhsValue`: a value that is no place read is valid at its declared
   type: a literal owns no struct, a construction owes each invariant
   (TYPE-11 line 6, judged as an FN-8 requirement), and a call's result is
   handed off at the callee's return (TYPE-11.S4). A place read gives the
   value of a place of the same declared type.
5. `Ctx.RunStep` for a call: each place apart from every place the call
   writes keeps its value and each place below no written referent that exists
   after the call existed before (EFF-2 and EFF-5); each written referent is
   valid when the call returns, which `Ctx.handoff_valid` at each of a source
   callee's exits gives and SCOPE-3's obligation gives for a supplied
   definition; the call rebinds none of the caller's reference variables.
6. `Ctx.RunStep` for a `swap`: both targets hold values of one declared type,
   and the run exchanges them (OP-11).
7. The walk: at each event the fact state holds in the run (`M.Facts f r`),
   the fact core's simulation; at body entry no debt is open and every value
   is valid, since the caller handed each argument and referent off.

Whitefoot#540 (`lang-contract-element-term`), not merged, makes an array
element a contract term; the facts can then prove instances at elements, and
premise 2 covers those unchanged (inferred from the specification text).

### Theorems

`SealingSound.lean` proves, under those premises:

- `Ctx.step_holds`: an event the rules accept keeps `Ctx.Holds`, by one lemma
  per event (`Ctx.store_holds`, `Ctx.bind_holds`, `Ctx.swap_holds`,
  `Ctx.call_holds`, `Ctx.death_holds`, `Ctx.bindRef_holds`,
  `Ctx.kill_holds`).
- `Ctx.handoff_holds` and `Ctx.handoffs_holds`: a hand-off, and a call's
  hand-offs in order, keep it; `Ctx.handoff_valid`: the value a hand-off the
  rules accept hands off is valid.
- `Ctx.join_holds`: the state at a merge agrees with the run arriving on any
  edge; `Ctx.header_entry_holds` and `Ctx.header_back_holds`: the state at a
  loop header agrees with the run entering the loop and with the run arriving
  on any backedge or `continue`.
- `Ctx.instance_true`: an instance the rules establish holds.

The events that write a place follow one argument, over three kinds of place.
A place at or below the written place holds part of the new value: for a store
or a call that value is valid, handed off, fresh or returned by the callee;
for a `swap` it is what was below the other target, and the debts exchanged
from there cover it. A place above it whose invariant the write breaks gets a
debt from the write row: the invariant held before the write, so the write
reached one of its binder data, and the write row adds a debt at every anchor
strictly above the written place whose read set the write reaches. Every other
place keeps its value, and the debt that covered it, if any, survives the
event's rows. A `let` from a place read takes each debt that may be at or
below that place, rebased onto the new binding. The axiom check accepts every
declaration of the model: 6,037 on the last run, `Sealing*.lean` among them.

### Readings

Where the text admits more than one reading, the component takes the one
below. Reading 1 is also the fix for the gap in the next section.

1. One statement's rows are judged in the state before its kill batch, with
   the members its places resolve to there (TYPE-11.S3 line 3, "in the walk's
   event order"). The kill row runs over the debts the statement found, before
   the rows that add debts, and restores with the facts before the batch, as
   ENT-5 orders. The debts the statement's own rows add are not offered for
   restoration: the batch widens each it touches (`Ctx.settle`).
2. A store's store row removes debts before the kill row widens any. A debt
   proved at or below the written place, in the state before the statement,
   lies within the stored value, which is valid; widening it first would keep
   a debt the store has paid. The write row's debts lie strictly above the
   written place, so they are never removed by the store row, before or after.
3. A `swap` target with several members moves no debt. A debt proved at or
   below one member may lie where the swap did not write, so it stays and is
   copied, exchanged, onto the other target, as a debt that may be at or below
   a target is (TYPE-11.S3 line 11).
4. The death row removes a cover rooted at the dead binding with the anchors
   rooted there (TYPE-11.S3 line 12).
5. The kill row drops the recorded reference of every debt it widens. For a
   debt whose anchor gained a `[?]` this changes nothing: such a debt forms no
   instance (TYPE-11.S5 line 3), and the store row for several members, the
   only row that reads a record, cannot remove it (TYPE-11.S3 line 9).
6. A write reaches an invariant's read set at an anchor when the written place
   may overlap the place of one of the invariant's binder data read from the
   anchor (`Ctx.reaches`, TYPE-11.S3 line 6).
7. Satisfaction at a handed place needs no `[?]` of the anchor below the
   handed place; the text implies it, since the handed place must have a
   captured offset at each step where the anchor has `[?]` (TYPE-11.S5
   line 3).

### A gap in the rules

TYPE-11.S3 does not say whether the debts a statement's write row adds are
among those its kill row offers for restoration. They are on the literal
reading, and the kill row restores with the facts before the batch, which can
still prove the instance the write breaks: a write whose place has an offset
that reads the written storage kills that offset itself. Inferred from the
rules, not run on the compiler; the case `selfIndexed` in `SealingCases.lean`
is this example, which the component rejects:

```wf
struct Node {
  next: u32;
  invariant link(node): node.next < 64_u32;
}
// An earlier read of nodes[nodes[i].next].next established link there.
// This write breaks that element, and its offset nodes[i].next reads nodes,
// so the same statement kills the offset. The facts before the batch still
// hold the instance, so the new debt is restored and no debt records the
// broken element.
set nodes[nodes[i].next].next = 1000_u32;
```

The fix, reading 1: the kill row offers for restoration only the debts the
statement found; a debt the statement's own rows add is widened to `[?]` where
the batch kills its offset, without restoration. It is the only working form.
Restoring the new debts with the facts before the batch is the gap; not
widening them leaves a debt naming another element; restoring every debt with
the facts after the batch would reject
`type11-pos-field-offset-restored-before-moved`, whose restoration needs the
facts before the cursor moves (inferred from the definitions, not run). The
change to TYPE-11.S3 belongs to the amendment's work.

### Cases

`SealingCases.lean` writes conformance cases of Whitefoot#484 as event
sequences with a small fact core, and the build checks each verdict with
`#guard`, 25 checks in all:

- `type11-neg-joined-reference-store-keeps-debt`, the fifteenth round's
  example: rejected. `type11-pos-joined-reference-store-restores`, a value
  broken through a reference and then replaced whole through the same
  reference: accepted; without the replacement: rejected.
- `type11-neg-swap-after-joined-write-restore-keeps-debt` and
  `type11-neg-swap-after-joined-write-store-keeps-debt`, the sixteenth
  round's examples: rejected. `type11-neg-reference-rebound-before-restored`:
  rejected.
- `type11-pos-joined-reference-swap-write-restored` and
  `type11-pos-joined-reference-call-write-restored`: accepted with the facts
  that restore, rejected without them.
- `type11-neg-swap-possible-alias-keeps-debt` over an `Array`: rejected, and
  the swapped-in value gets no instance; `type11-pos-swap-elements`: accepted.
- From `type11-neg-stored-field-corrupt-then-read`: a broken field and its
  copy give no instance; replacing the field restores it and not the copy.
- `type11-neg-branch-join-broken` and `type11-pos-branch-join-restored`: a
  merge offers each edge's debts for restoration in that edge's facts.
- `type11-neg-field-offset-moved-before-restored` and
  `type11-pos-field-offset-restored-before-moved` with the cursor a local:
  five checks of a debt widened when its offset moves.
- The gap's example above: rejected.

A temporary workflow put back, one per job, the rule each of three of these
cases protects, and required the cases to fail and the proofs not to check.
With the fifteenth round's joined store, a store through a reference with
several members removing the debts proved at or below every member, the checks
of `type11-neg-joined-reference-store-keeps-debt` and
`type11-neg-swap-after-joined-write-store-keeps-debt` failed and
`storeRemove_keep` did not check. With the sixteenth round's recorded swap, an
exchanged debt keeping its recorded reference, the checks of the two
`type11-neg-swap-after-joined-write-…` cases failed and `Debt.exchange_covers`
did not check. With the literal reading of the gap, the gap's case failed and
`Ctx.writeDebts_cover` and `Ctx.store_holds` did not check. No other check
failed.

### Line by line

Each line of the nine rules on Whitefoot#484's branch, numbered as the
coverage checker cuts the rule (the rule's title line is line 1), with the
FNV-1a hash the checker reads. When the amendment lands, each line with a
definition gets a `-- spec` mark above it, and each other line an `outside`
entry with the reason given; a line with both gets the mark and its
definition's docstring names what is left out. 30 lines have a definition,
and 33 are outside: 17 rule headings, 4 table heading rows and 12 sentences.

| Line | Hash | Formalized by | Outside the component, and why |
|---|---|---|---|
| TYPE-11 1 | `cbc6dbe9a8f98d67` |  | heading of the rule; its sentences are marked or listed |
| TYPE-11 2 | `f0a428ff06a687a7` |  | the declaration's form and its distinct names, which the grammar and the declaration check judge before any body; the component takes the declared invariants as given (`StructDecl`, `Inv`) |
| TYPE-11 3 | `df5862927b27db50` | `Side` and `Inv`: the admitted relation, one comparison of two sides, each a binder datum reached by field and `Box` content steps that ends at a fragment integer, displaced by a constant, or a constant alone (a named const, a typed literal or a widening conversion of either is a constant, and a widening conversion of a datum has the datum's value) | the admission judgment itself, which rejects at the declaration; measure members (measures); generic structs (generics) |
| TYPE-11 4 | `c3d84aa8041b4702` |  | the hard errors at a `type_invariant` and at a `public` field, and their repairs (the declaration check and diagnostics); the last sentence's debt is the write row of TYPE-11.S3 line 6, which the component applies to every write, having no modules |
| TYPE-11 5 | `adc70824bc97c4b4` | `Inv.HoldsOf` with `Side.eval` |  |
| TYPE-11 6 | `8fa530fd480e7407` |  | the construction's obligation, an FN-8 requirement the fact core judges; the component reads its consequence as the premise `Ctx.RhsValue` on `Rhs.fresh`: a constructed value is valid |
| TYPE-11 7 | `a41b7a68f57d6f31` |  | atomic statements and `Shared` or `SharedRead` targets (shared objects) |
| TYPE-11 8 | `0ff67ec596c9ba02` |  | range type invariants, which the range rules carry |
| TYPE-11 9 | `92055033ac128e94` |  | range type invariants, as line 8 |
| TYPE-11 10 | `a41ed3754ed1060c` |  | range type invariants, as line 8 |
| TYPE-11 11 | `4686871d58a6431c` | `Ctx.Sealed` and `Ctx.Holds`, the invariant every theorem keeps, with `Ctx.handoff` (restored before the next hand-off) and `Ctx.instance?` (an instance only where no debt may denote it); a store into a place other than a complete own local binding is `Event.store`, which hands its right-hand side off. Body entry, a call's results and written referents, and a construction are premises (the walk; `Ctx.RhsValue`; `Ctx.RunStep`) | a supplied definition's result (TYPE-11.S8); atomic entries and `shared_close` results (shared objects) |
| TYPE-11.S1 1 | `c906dbafcc33d572` |  | heading of the rule; its sentences are marked or listed |
| TYPE-11.S1 2 | `f92e52ce97faafce` |  | heading of the rule; its sentences are marked or listed |
| TYPE-11.S1 3 | `631d377f7252642a` | `Ctx.ValidVal` and `Ctx.Valid` over `Val`, whose children are a struct's fields, a `Box`'s content, the payload of the variant an enum holds and an `Array`'s elements | `Segments` and windows (the window part); handles, maps, key sets, decision cells, host handles and `Entries`, which own nothing (shared objects and maps); none is a type of the component |
| TYPE-11.S1 4 | `bba13b38c9295979` | `Ctx.invsAt` and `Ctx.invsOfPlace` (an anchor is a place whose struct has an affine invariant), `Ctx.InvHolds` (the instance in a run), `Core.proves` (the instance over a spelled place), `Inv.data` with `Ctx.reaches` (the read set) | the substitution of a function instance (generics); the descriptor storage of measure members (measures) |
| TYPE-11.S2 1 | `c909dbafcc360b49` |  | heading of the rule; its sentences are marked or listed |
| TYPE-11.S2 2 | `d0319c847eeb7262` |  | heading of the rule; its sentences are marked or listed |
| TYPE-11.S2 3 | `2660b2c95af932f1` | `St.debts`; `Debt` with `Debt.Covers` (an anchor shape with one invariant, or a cover standing for every anchor at or below its place); `Rec` (the recorded reference and projection); `Debt.sameKey`, `Debt.unite` and `addDebt` (uniting); `Ix`, `SStep` and `Shape` (each index position a captured offset or `[?]`) | the breaking events, which only the diagnostic reads; range and page positions (range references, pages) |
| TYPE-11.S2 4 | `5618b0fdcacf0224` | `Ctx.mayOverlap`, `Ctx.mayAtOrBelow`, `Debt.provedAtOrBelow` with `provedPrefix`, `Ctx.mayDenote`; a level is the number of steps, each of which selects a value (`SStep`); a sealed handed place is what `Ctx.handoff` requires | range, page, `Segments` and window steps and range or run roots, which add no level (range references, pages, the window part); cover steps inside a place, which only the generalization of a reference re-pointed in a loop forms (REF-1's loop rule, the references part) |
| TYPE-11.S2 5 | `50676a9d3b7cc413` | `Debt.rebase`: with every step selecting a value and none a cover step, p's own steps and the anchor's shortest prefix at p's level are both the anchor's first steps, as many as p has; a cover above p rebases to the cover of q; a rebased debt records nothing | an anchor that passes a cover step, as line 4 |
| TYPE-11.S2 6 | `fadae449d7141111` |  | unfoldings, which serve only the entries of maps and `Entries`, atomic statements' targets and `shared_close` results (shared objects and maps) |
| TYPE-11.S3 1 | `c90dd7afcc39ed54` |  | heading of the rule; its sentences are marked or listed |
| TYPE-11.S3 2 | `ebda1d0d657236a8` |  | heading of the rule; its sentences are marked or listed |
| TYPE-11.S3 3 | `d45c8a55493e6d99` | `Ctx.step`, each row for each member `St.resolve` and `St.resolveRec` give; the order is reading 1 |  |
| TYPE-11.S3 4 | `d0f728660df5427f` |  | the table's heading rows |
| TYPE-11.S3 5 | `5ff9cb7afb3a4ac3` |  | the table's heading rows |
| TYPE-11.S3 6 | `26ffbdde631ed078` | `Ctx.writeDebts`, `Ctx.writeRows`, `Ctx.reaches` and `writeRec`, for a commit or update (`Event.store`) and each target of a `swap` (`Event.swap`); `Ctx.reaches` is reading 6 | anchors named through a cover's current target identity, and the cover a write adds through such a description (REF-1's loop rule, as TYPE-11.S2 line 4) |
| TYPE-11.S3 7 | `772f30565627d034` | the projected writes of `Event.call` through `Ctx.writeRows`, bounded to the anchors strictly above the argument's referent; that the callee leaves the referent sealed is a premise of `Ctx.RunStep` | `map_clear` (maps); `swap` is its own event, `Event.swap` |
| TYPE-11.S3 8 | `fcbf00d2a9fe6af6` | `storeRemove` for a place resolving to one member, in `Ctx.storeRows`; a right-hand side that is a hand-off is `Rhs.read`, a call result or construction `Rhs.fresh`; the order is reading 2 |  |
| TYPE-11.S3 9 | `c84dfe7989a5017d` | `storeRemove` for a place resolving to several members, with `Debt.recordsAt` |  |
| TYPE-11.S3 10 | `c889a2ac2a29907e` | `Ctx.continueRow` in `Event.bind` with `Rhs.read`: a `let`, a `let` with `move`, a `set` of a complete own local binding, and a by-value `match` or destructuring binder as a read of the payload or field place it binds | the `Ok` binder of `propagate` (error propagation, not in the fragment) |
| TYPE-11.S3 11 | `c682fdad3739e1b6` | `Ctx.swapRow` with `Ctx.swapStay`, `Ctx.across` and `Debt.exchange`, after the write rows in the swap case of `Ctx.step`; a target with several members is reading 3 |  |
| TYPE-11.S3 12 | `dc8504bf6ab1792f` | `deathRow` in `Event.death`; a move of a part runs the continuation row (`Event.bind`) before the death; a cover counts by its place, reading 4 |  |
| TYPE-11.S3 13 | `5684a5abeb57f254` | `Ctx.killRow` with `Ctx.untouched`, `Ctx.widen` and `Ctx.widenSteps`; `Kills` and `Core.killed` give what a batch writes, ends or rebinds, `Core.tracked` which offsets read a tracked place; a counted loop's backedge is a kill batch for its binder (`Event.kill`); the debts of the statement's own rows are reading 1 and the gap below, a dropped record reading 5 | an anchor carrying a reference's current target identity, and widening to a cover (REF-1's loop rule) |
| TYPE-11.S3 14 | `2af3fb7da35e525b` | `Rhs.fresh`: `Event.bind` and `Event.store` take no debt from it, so none lies at or below the place it fills; the swap case of `Ctx.step` hands neither target off | `shared_close` results (shared objects); `release` and `map_clear` (supplied definitions, maps) |
| TYPE-11.S4 1 | `c91117afcc3c8feb` |  | heading of the rule; its sentences are marked or listed |
| TYPE-11.S4 2 | `5bdbbbc180826764` |  | heading of the rule; its sentences are marked or listed |
| TYPE-11.S4 3 | `c2c2a78f86bf6ce5` | `Ctx.handoff`: every debt that may be at or below a member of the handed place restored, or satisfied at that member, in the state before the hand-off's events; the restored ones leave | the exemption of anchors below an entry of a map or an `Entries` (maps and shared objects) |
| TYPE-11.S4 4 | `49a2318fd9edc012` |  | the table's heading rows |
| TYPE-11.S4 5 | `11ce55d3e6462cec` |  | the table's heading rows |
| TYPE-11.S4 6 | `f7e7cedc73a4aba1` | the handed places of `Event.call`, each through `Ctx.handoff` | a spawn (parallelism), whose arguments the same `Ctx.handoff` would judge; the failure's rule and site (diagnostics) |
| TYPE-11.S4 7 | `2a8f81177dbdcf51` | the handed places of `Event.call`: a reference argument's referent, which `Ctx.handoff` resolves to each member | a range reference's run (range references); the failure's site (diagnostics) |
| TYPE-11.S4 8 | `13526fd25aab12a9` | `Ctx.handoff` at the returned value's place and at each written referent; `Ctx.handoff_valid` there is the callee's side of the premise `Ctx.RunStep` states for a call | propagated error exits (error propagation, not in the fragment); the failure's site (diagnostics) |
| TYPE-11.S4 9 | `c2da17659bff10fc` |  | `give_stmt` (value blocks, not in the fragment) |
| TYPE-11.S4 10 | `e60b9f3287963147` | `Ctx.handoff` at each operand's place; the constructed value is `Rhs.fresh` | the failure's site (diagnostics) |
| TYPE-11.S4 11 | `c39bf31bfdee901f` | `Event.store` with `Rhs.read`, whose hand-off `Ctx.step` judges first | the failure's site (diagnostics) |
| TYPE-11.S4 12 | `366140955f916c30` |  | diagnostic content, which acceptance does not read |
| TYPE-11.S4 13 | `0e8b01bdc7f1cd94` | `Rhs.fresh` (a call's results carry no debt); the premise of `Ctx.RunStep` that each written referent is valid when the call returns | supplied definitions (TYPE-11.S8); `shared_close` results (shared objects) |
| TYPE-11.S5 1 | `c9141bafcc3ecc8e` |  | heading of the rule; its sentences are marked or listed |
| TYPE-11.S5 2 | `40ba9644ff978d6b` |  | heading of the rule; its sentences are marked or listed |
| TYPE-11.S5 3 | `e7582cffe4d98455` | `Ctx.restored` with `Debt.instancePlace?` (the instance over `r^` followed by p for a debt that records r and p, otherwise over the anchor's place, and none at `[?]` or for a cover); `Ctx.satisfied` with `agrees`, reading 7; `Core.proves`, the fact core's MSR-4 disposition with its certificates, is a premise (`Sem.proves_sound`) | an anchor carrying a reference's current target identity (REF-1's loop rule) |
| TYPE-11.S5 4 | `106687ed51495edf` | `Ctx.handoff` (hand-offs); the swap case of `Ctx.step` (before its events); `Ctx.killRow` (kill batches); `Ctx.absorb` with `Ctx.restoreAll`, in `Ctx.header` (backedges and `continue`) and `Ctx.join` (merges) |  |
| TYPE-11.S6 1 | `c917dbafcc4248a5` |  | heading of the rule; its sentences are marked or listed |
| TYPE-11.S6 2 | `35b30c78112e6ce2` |  | heading of the rule; its sentences are marked or listed |
| TYPE-11.S6 3 | `b6cdead0b7acdc1b` | `Ctx.join` with `Ctx.absorb`; the instances a merge establishes are `Ctx.instance?` on each incoming edge, which `Ctx.instance_true` proves true there | breaking events (diagnostics); the join of the fact states (the fact core) |
| TYPE-11.S6 4 | `23c2bb7e709598cd` | `Ctx.header`: one round of the iteration, the entering edge's debts with every backedge's and `continue`'s after restoration | the iteration to the least set, its termination and its independence of order, which belong to the walk; the component has none |
| TYPE-11.S7 1 | `c91ad7afcc4477b0` |  | heading of the rule; its sentences are marked or listed |
| TYPE-11.S7 2 | `60cd57c49f93f34c` |  | heading of the rule; its sentences are marked or listed |
| TYPE-11.S7 3 | `bda737c615296732` | `Ctx.instance?`: no debt may denote the instance at any member of the place, judged after a read, at body entry, after a call's kills at each written referent, after a `let`, a destructuring or by-value `match` binder and a commit to a binding; at a selected return, the return's hand-off (`Ctx.handoff`, `Ctx.handoff_invHolds`) | the other premises (the root live, no atomic statement's target, payload steps under a current refinement, the call's kill conditions), which the walk and the fact core judge and which make the place exist in the run; the term an instance is stated over (the fact core's term formation); the `Ok` binder of `propagate` |
| TYPE-11.S8 1 | `c8e7d7afcc191e17` |  | heading of the rule; its sentences are marked or listed |
| TYPE-11.S8 2 | `f93bf83443cc92ec` |  | heading of the rule; its sentences are marked or listed |
| TYPE-11.S8 3 | `89411175ae9c58f9` |  | supplied definitions generic in a type parameter, whose SCOPE-3 obligation is trusted; it gives a call's premise that written referents are valid on return |
| TYPE-11.S8 4 | `052ad1455e3ce8c1` |  | occurrence identities and failure sites (diagnostics); range type invariants |

### Size and cost

The component is 1,106 lines of Lean with 127 definitions, its cases 372 lines
and its proofs 2,472 lines with 157 theorems; it changes no earlier module
apart from the package root's imports, and adds no coverage mark. Before the
main line's discharge lemmas were merged in, the build of the model and
harness took 25.4 s, 32.5 s and 48.7 s on the three green `proof.yml` runs,
within the build's budget of the time, 65 s, against 39.9 s to 42.2 s on the
main line's three runs of the same day; on the last run the three modules
built in 3.4 s, 1.8 s and 5.4 s, with 5.8 s, 2.6 s and 0.9 s more for their
object code. With those lemmas merged in, the build took 61.5 s and 66.1 s on
the next two runs, the three modules 3.3 to 3.6 s, 1.5 to 2.1 s and 5.1 to
5.3 s, and the build's provisional budget rises from 85 s to 100 s, 1.5 times
66.1 s rounded up to 5 s. The study's estimate was about 1,200 lines of
definitions and 4,000 of proof. The part took two engineer turns, the first of
which also made the study, nine `proof.yml` runs and one run of the temporary
workflow to its review. Three runs failed to compile: the meaning of a cover
named a place it had not bound, three steps of the first lemmas did not
type-check, and a lemma's proof referred to itself.

## Cost

Precedents with measured sizes:

| Work | Model | Proof | Effort | Source |
|---|---|---|---|---|
| Cedar, all components (2024) | 1,673 lines of Lean | 5,714 lines of Lean | not stated | [cedar-vgd] |
| Cedar's validator soundness | 532 lines | 4,686 lines | 18 person-days for the proof | [cedar-vgd] [cedar-blog] |
| WebAssembly 1.0 in Isabelle (semantics, type system, soundness, interpreter, checker) | about 8,600 non-comment, non-whitespace lines in all | | | [ctwasm] |

Proof size grows faster than the size of what it is about: over 15,018
lemmas of seL4 and the Archive of Formal Proofs, proof size grew with the
square of the statement's size [matichuk].

The first phase's fragment (see [Phase 1](#phase-1)) carries more intricate
static rules than Cedar's validator, a type checker over expressions: facts
with a closure, invalidation by writes, joins, loop heads and the contract
boundary. The estimate for it is a model of 1,500 to 2,500 lines (parser for
the fragment, checker, interpreter, differential harness) and a proof of
6,000 to 12,000 lines. At Cedar's measured rate, 4,686 proof lines in 18
person-days, about 260 a day, that is 25 to 50 person-days of expert work.
No rate is measured for agents writing Lean against this repository's CI;
each slice records its lines and the turns and CI runs it took, and the
estimate is replaced by those measurements. The first slice, the check and
the value part of [OP-2], is 21 marked lines and 26 listed outside of OP-2's
47: a model of about 290 lines of Lean with ten theorems, beside a coverage
checker of about 250 lines and an axiom check of about 40.

Each run of `proof.yml` downloads the pinned Lean (about 20 s on GitHub's
machines), builds the model and proofs and, once the model parses programs,
runs the fragment's conformance cases. The first slice's coverage check takes
about 2.5 s and its proofs about 4 s; with the second slice the build of the
model, the coverage checker and the conformance executable takes 8.8 to 9.8 s
and the conformance run 0.7 to 0.9 s over three runs. The stages' budgets
follow the measurements as they grow.

Each later change of the specification costs, by the measurement in
[Keeping the proof in step](#keeping-the-proof-in-step), made for a larger
sequential rule set than phase 1's: about half the changes touch a covered
rule and need one coverage line per changed sentence; 9% to 16% change the
fragment's meaning and need the model and proof updated in the same work. At the measured rate of 75 versions in 22 days,
3.4 a day, that is 0.3 to 0.55 model updates a day, about one every two to
three days.

## Incidental findings

Found while reading the rules for this study. "Read" marks a fact in the
text; "inferred" marks a consequence not yet shown with a program. Finding 6
is taken up as `wf-sound-set-rhs-target`, findings 2 to 5 and 7 to 13 together
as `wf-spec-rule-gaps`, and finding 1 by the model this study designs.

1. The specification gives safety no runtime meaning to be checked against.
   [SCOPE-3] (line 17) says accepted programs have no undefined behavior but
   never says what undefined behavior is; the only mention of data races is
   [CAP-1]'s "Its data-race guarantee is subject to [SCOPE-3]; it does not
   exclude general race conditions" (line 2233), and no rule defines a
   location, an access, a happens-before order or a race. No rule defines
   parameter binding or an activation; the runtime meaning of a
   requirement ("it holds in the pre-call state of every execution") is never
   stated, the nearest being [DIAG-2]'s "every value a returning call hands
   back or leaves behind satisfies them" (line 2177). Read. The model must
   supply these; see [the semantics decision](#phase-1).
2. [FN-1] says every atomic block an edge leaves "completes its ordinary
   exit, including [TYPE-11] obligations and [SHARE-2] release" (line 1359),
   but SHARE-2 (lines 2387 to 2414) never mentions a release; the end of an
   atomic statement's exclusive access on a leaving edge is only implicit.
   Read.
3. [EFF-3] licenses deduplicating and reordering a `pure`, non-waiting call
   "which allocates nothing" (line 1660), and grounds it on "a duplicated take
   is a different program [STOR-8]". STOR-8's list of allocating rows
   (line 887) names only `Box` and storage constructors and growth, not
   `shared_new`, `frozen_new`, `key_set_new` or `cancel_source`, which are
   `pure` and create runtime objects. Read. Inferred: two equal
   `shared_new` calls may be merged into one object, which a program can
   observe, and a no-heap program may call `key_set_new` although a key set
   holds runtime storage.
4. A call through a function-kind formal is checked against the formal's
   contract and row, while the actual function runs ([FN-5], line 1423); the
   actual's own proof treated its reference parameters as separate roots for
   its possibly stronger postconditions. Inferred, no witness built: the
   v0.135 contract-read footprint covers the formal's contract only, so a
   proof must show every state the actual's proof relied on lies in the
   formal's requirements or row.
5. A guaranteed tail call transfers "without retaining the current
   activation" ([FN-10], line 1593), while a spawned statement is joined
   "when the activation that executed the spawn leaves by any edge"
   ([WAIT-3], line 2353); whether the transfer joins outstanding spawns is
   not stated. Read; no memory-safety consequence, since a context holds
   only moved values.
6. A `set` whose right-hand side changes the storage of its own target.
   [SET-1] evaluates the target first and judges its subscript's bound there
   (lines 619 and 621, and [OP-4] line 1115: "its discharge judgment is
   identical in target position"), carries the target address across the
   right-hand side (line 638), and after it rechecks only writability,
   liveness, moved prefixes and references the target is reached through
   (lines 635 to 637). [OP-12] states the missing restriction, but only for
   the atomic update ("`f`'s declared row must not write, move out of, or
   free any prefix of `p`", line 1268). Read. Measured: the compiler at
   main's revision c40acc366 accepts both programs below (this study's
   temporary workflow, run 39574795043, ubuntu-24.04). The first aborts with
   glibc's "free(): double free detected in tcache 2": as the rules predict, the commit releases
   the stale copy, in slot 1, of the box `remove_at` moved to slot 0, and the
   scope exit releases slot 0. The second exits 3: the store went to the
   block `grow` released, so slot 0 of the new block still holds 1.
   Recommended: a specification item that applies OP-12's condition to every
   `set`, with these programs as conformance cases.

   ```wf
   fn main() -> status: std::process::ExitStatus pure {
     let v = slots_new::<Box<u64>, 4>();
     let first = box_new::<u64>(value: 1_u64);
     place_back(window: &v, value: move first);
     let second = box_new::<u64>(value: 2_u64);
     place_back(window: &v, value: move second);
     set v[1_u64] = remove_at(window: &v, index: 0_u64);
     return std::process::exit_status(code: 0_u8);
   }
   ```

   ```wf
   fn refill(cell: &Box<Slots<u64>>) -> result: u64 writes(cell) contract {
     requires cell^.inner.cap <= 64_u64;
     ensures cell^.inner.len == entry(cell)^.inner.len;
   } {
     grow(cell: cell, capacity: 64_u64);
     return 7_u64;
   }

   fn main() -> status: std::process::ExitStatus pure {
     let b = box_slots_new::<u64>(capacity: 1_u64);
     let pin = box_new::<u64>(value: 9_u64);
     place_back(window: &b.inner, value: 1_u64);
     set b.inner[0_u64] = refill(cell: &b);
     let stored = b.inner[0_u64];
     if stored == 7_u64 {
       return std::process::exit_status(code: 0_u8);
     }
     return std::process::exit_status(code: 3_u8);
   }
   ```
7. Moving out through a reference parameter is refused by no stated rule:
   [TYPE-7] says a use of an affine `p^` "requires `move`" (line 612), while
   [OWN-1] admits consumption only of places "rooted in a live own-mode
   binding" (line 690). Read. Recommended: one sentence in OWN-1 naming the
   refusal and its citation.
8. A `nodrop` struct may be a const: [CONST-2] admits a non-opaque struct
   with eligible fields (line 676) and a const is never moved (line 679),
   while `nodrop` "states a logical must-consume obligation" (line 439).
   Inferred. Recommended: decide in CONST-2 whether `nodrop` types are
   eligible.
9. The loop-head state has no stated input for its "may kill" judgment.
   [ENT-5] removes "every fact having a support member that a continuing
   kill event of L may kill" (line 4181), and an indexed write is separated
   from a fact only by offsets proved distinct "in the entry state of that
   event" (line 863), which for a continuing event depends on the head being
   formed. Read; the circularity is inferred. A purely structural reading is
   safe; using pre-loop facts to separate a write whose index changes in the
   body is not. Recommended: state that the judgment is structural.
10. [CALL-1] states survival per parameter ("the call is a kill event for no
    fact supported by that actual's path", line 3726), while kill (b)
    applies to any argument's projected write (line 4062) and EFF-5 compares
    only the footprints the arguments supply (lines 1671 to 1674). Read;
    inferred that an unread, unnamed reference argument sharing storage with
    a written one is killed anyway. Recommended: phrase CALL-1 as "this
    parameter contributes no write".
11. A contradictory prelude contract would discharge every obligation in
    every caller ([ENT-4] line 3988), and [PRE-1] contracts are trusted under
    [SCOPE-3] ("a PRE-1 or PRE-2 definition is supplied under SCOPE-3",
    line 1321), with CALL-6's consistency check seeing only what
    the closure represents (line 3958). Read. Recommended: the model
    exhibits a satisfying state for each prelude contract it uses.
12. Wording a model must settle, each read: WIN-2 says "`r.len` is itself a
    write target" (line 862) and OP-15 "A measure is never a write target"
    (line 1300); WIN-3 says "frees the block" (line 874) and STOR-3 "no
    storage reclamation of its own" (line 907); TYPE-9 does not count
    `Paged` as a runtime-capacity form (lines 587 and 589) while STOR-1 and
    OP-13 do (lines 848, 1279); OP-8 carries editing history ("added in this
    revision", line 1190). Recommended: one specification item for the
    wording.
13. [OP-2] gives `iabs.wrap` a result only at `MIN(T)`: "The wrap forms
    return `wrap_T(z)` for add, subtract, multiply, and negation;
    `iabs.wrap(MIN(T))` returns `MIN(T)`" (line 1076). Its result at every
    other operand follows only from [OP-8]'s lowering ("use `llvm.abs` with
    is-int-min-poison false", line 1198) and from [ENT-3.S7]'s range row
    ("corner hull of abs(a)", line 3854). Read; found in the review of the
    first slice, whose model takes no step there. Recommended: OP-2 names
    absolute value among the wrap forms that return `wrap_T(z)`, from which
    `iabs.wrap(MIN(T))` returning `MIN(T)` follows.

## Sources

Read for this study on 2026-10-21; figures are as the sources state them.

- [alphaproof] T. Hubert et al., "Olympiad-level formal mathematical
  reasoning with reinforcement learning", Nature, 2025: Lean proofs, trained
  on 80 million statements auto-formalized into Lean.
- [aristotle] Harmonic, the Aristotle paper, arXiv:2510.01346, 2025: Lean 4
  solutions to five of the six 2025 IMO problems.
- [cedar-blog] AWS Open Source Blog, "Lean into verified software
  development", <https://aws.amazon.com/blogs/opensource/lean-into-verified-software-development/>:
  Lean chosen for its fast runtime, libraries and small trusted base; "a new
  version of Cedar isn't released unless its model, proofs, and differential
  tests are up to date"; the validator proof, 4,686 lines, took 18
  person-days.
- [cedar-vgd] C. Disselkoen et al., "How We Built Cedar: A
  Verification-Guided Approach", FSE Companion 2024, industry track,
  <https://arxiv.org/abs/2407.01688>: Table 1 (Lean model 1,673 lines, Lean
  proofs 5,714 lines, Rust 15,693 lines; validator model 532, proofs 4,686);
  proofs found 4 validator bugs, differential and property-based testing 21
  more.
- [compcert] X. Leroy, "A formally verified compiler back-end", Journal of
  Automated Reasoning, 2009, <https://arxiv.org/abs/0902.2137>.
- [ctwasm] C. Watt, J. Renner, N. Popescu, S. Cauligi, D. Stefan, "CT-Wasm:
  Type-Driven Secure Cryptography for the Web Ecosystem", POPL 2019,
  arXiv:1808.01348: inherits "~8,600 lines of non-comment, non-whitespace
  Isabelle code" from Watt's mechanization.
- [iris-lean] "Iris in Lean", arXiv:2609.24252, 2026, and the Iris workshop
  2026 slides: the core library ported, some features still missing.
- [jscert] M. Bodin et al., "A Trusted Mechanised JavaScript Specification",
  POPL 2014: ES5 in Coq with an extracted interpreter.
- [kjs] D. Park, A. Stefanescu, G. Rosu, "KJS: A Complete Formal Semantics of
  JavaScript", PLDI 2015: ES5.1, passes all 2,782 core conformance tests.
- [lean-grind] The Lean Language Reference, "The `grind` tactic",
  <https://lean-lang.org/doc/reference/latest/The--grind--tactic/>.
- [matichuk] D. Matichuk et al., "Empirical Study Towards a Leading
  Indicator for Cost of Formal Software Verification", ICSE 2015: 15,018
  lemmas, proof size quadratic in statement size.
- [mir-formality] Rust project goals, "a-mir-formality",
  <https://rust-lang.github.io/goals/2026/a-mir-formality.html>.
- [putnambench] G. Tsoukalas et al., "PutnamBench", NeurIPS 2024 datasets
  track, <https://arxiv.org/abs/2407.11214>.
- [rustbelt] R. Jung et al., "RustBelt: Securing the Foundations of the Rust
  Programming Language", POPL 2018: a core language, lambda-Rust, and its
  semantic soundness proof in Coq with Iris.
- [ryr] C. Klein et al., "Run Your Research: On the Effectiveness of
  Lightweight Mechanization", POPL 2012,
  <https://users.cs.northwestern.edu/~robby/lightweight-metatheory/>: Redex
  models of nine ICFP 2009 papers found mistakes in all nine.
- [spectec] D. Youn et al., "Bringing the WebAssembly Standard up to Speed
  with SpecTec", PLDI 2024: one source for the standard's typeset rules,
  prose, interpreter and prover definitions; the Rocq backend is the most
  developed.
- [wasmcert-2] "Updating WasmCert-Isabelle to WebAssembly 2.0", WebAssembly
  workshop at ICFP/SPLASH 2025.
- [wasmcert-two] C. Watt et al., "Two Mechanisations of WebAssembly 1.0",
  FM 2021.
- [wasmref] C. Watt, M. Trela, P. Lammich, F. Maerkl, "WasmRef-Isabelle: A
  Verified Monadic Interpreter and Industrial Fuzzing Oracle for
  WebAssembly", PLDI 2023: adopted as a fuzzing oracle for Wasmtime.
- [watt-cpp18] C. Watt, "Mechanising and Verifying the WebAssembly
  Specification", CPP 2018: a mechanized soundness proof whose work exposed
  errors in the draft specification, fixed before publication.
