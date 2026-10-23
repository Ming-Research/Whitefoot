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
