# Range facts for ordinary obligations

Status: investigation, awaiting the owner's choice of route. Baseline:
branch `claude/checker-completion` at 375eec894 (specification v0.102), whose
range rules match main's v0.101. No implementation or measurement
accompanies this draft.

## Question

A range fact states a property of every element of a run, such as
`forall all(k in 0_u64..rows^.len): rows^[k].len <= 4_u64`. Today only the
range judgment consumes one: [RANGE-2] says no ordinary obligation consumes a
range fact, and [RANGE-4] admits a written instance only in an `apart`
certificate. A bounds, overflow or invariant obligation at a read of one
element therefore cannot use the fact the writer stated about every element,
and the writer adds a run-time test the program does not need.

The owner selected making range facts usable per element in ordinary proofs
now (board card "range facts now or in phase five", option A, 2026-10-08),
after approving on 2026-10-07 the same capability for validated interpreter
code (Q139 A: a local invariant may `use` a range fact, with read provenance;
Q145 A2: a read instantiates the range facts that cover it). This
investigation chooses how the two judgments share the work.

## Witnesses

Each is a minimal program refused today only by the boundary in question.
Formal cases will be standalone copies under `tests/conformance/`.

1. Input-guaranteed rows. A requirement bounds every row's length; the
   counted loop subscripts a five-entry table by the current row's length.
   ```text
   fn f(rows: &Slots<Slots<u8, 8>>, table: &Array<u64, 5>) -> r: u64 reads(rows, table) contract {
     requires forall all(k in 0_u64..rows^.len): rows^[k].len <= 4_u64;
   } { ... for (i in 0_u64..rows^.len) { let t = table^[rows^[i].len]; ... } ... }
   ```
   OP-4 owes `rows^[i].len < 5`. `rows^[k].len` is also refused as a range
   term today ([RANGE-1], "a range term selects below an element"), so this
   witness needs the projection extension of
   [range-field-terms](../range-field-terms/DESIGN.md) as well.
2. Stored positions. Snowghost's layout reads a position from one window and
   subscripts another with it
   ([layout friction](../layout-friction/DESIGN.md#a-second-cause-positions-stored-in-another-window),
   probes `stored-position.wf` and `stored-position-use.wf`): with
   `forall bounded(j in 0_u64..slots^.len): slots^[j] < blocks^.len`,
   `blocks^[slots^[k]]` fails OP-4 with residual `at < blocks^.len`, and the
   written instance `invariant within: at < blocks^.len { use bounded(k); }`
   is refused by RANGE-4.
3. Validated code. `range-use.wf` of the match-dispatch plan (PR #262):
   `let t = targets^[i];` under `forall valid(j in 0_u64..4_u64): targets^[j] < 4_u64`,
   then `invariant bounded: t < 4_u64 { use valid(i); }`.
4. The per-row header invariant (board item on the current-element header
   relation): `for (i in 0_u64..n, invariant fits: rows^[i].len <= 4_u64)` is
   refused at the final header, where `rows^[n]` does not exist. That refusal
   is correct and stays; with witness 1 accepted, the writer states the
   requirement and needs no per-row invariant. Its backedge would owe
   `rows^[i + 1].len <= 4_u64`, which only a range fact can supply, so
   checking it only at headers that enter the body would not make it
   provable either.

## How the two judgments are built

Ordinary entailment runs first over every function
(`compiler/src/semantic/check.rs`, `analyze_function_inventory`), and the
range judgment runs once afterwards (`range_judgment::judge_program`). The
range walk reads no result of entailment: it re-reads the function's affine
requirements, each local invariant's target and each counted loop's affine
invariants from the checked tree as path conditions, trusting entailment to
have proved them (`range_judgment/walk.rs`, the `Proof` arm).

Ordinary entailment has no term for an element read. [ENT-2] admits a
subscripted place as a term only when its last step is a readonly field or
a measure; a plain element read is a fresh unknown in the affine layer and an
opaque goal datum otherwise (`entailment/flow/sources.rs`, `read_operand`).
Its facts die by [ENT-5] overlap kills, so a write to any element of a run
kills every fact over that run.

The range walk is the opposite: element reads are atoms of a storage
version, a write makes a new version defined by the old, two reads of one
version at equal indices are one value, and [RANGE-3] instantiates each
active fact at the indices the problem's reads select.

The compiler design tree refused putting range facts into the entailment
flow because "every element write would kill them through the ordinary
overlap events and the prover's families relate no element reads"
([range judgment](../../../design/compiler/range-judgment.md)), and the
language tree refused ordinary consumption because ordinary entailment would
then rest on a judgment that runs after it
([range facts](../../../design/language/checks-and-proofs/range-facts.md)).
The owner's rulings reopen the second decision; its reopening condition, a
writer trial showing guards costing more than facts, is replaced by the
owner's ground that a run-time test standing in for a fact the writer can
state is a workaround. The route chosen below has to answer both stated
objections.

## Routes

### A. The range judgment discharges what ordinary entailment leaves open (recommended)

In a function that takes part in the range judgment, an ordinary obligation
whose goal is an integer comparison, and which ordinary entailment does not
prove, is owed at the same point to the range judgment instead of rejecting.
Ordinary entailment continues past it as though it held: an invariant
target so deferred is published as usual. The range walk proves it as a
clause without bound variables under [RANGE-3], with the walk's state at that
point: its path conditions, its active facts, every written instance there,
and every earlier local invariant target as a path condition, whether
ordinary entailment or the walk proved it. If the range judgment does not
prove it, the program is rejected with the original obligation's rule and
diagnostic.

- RANGE-4 admits `use NAME(terms)` in a local invariant's proof. Its
  instance joins the problem of that invariant's deferred target.
- Q145 A2 (a read instantiates covering facts) is [RANGE-3] step 1 as it
  stands: instances form at the reads the deferred goal contains, after
  definitions such as `t` = `targets^[i]` are expanded.
- Soundness: each deferred obligation is proved at its own point from facts
  holding there. A local invariant target is assumed after its point by
  both judgments and proved at it; a header invariant is assumed at the
  header and proved at entry and at each backedge, the same induction both
  judgments already use. No obligation is assumed at the point that owes it,
  so there is no cycle.
- Objections answered: no range fact enters the entailment flow, so the
  compiler tree's rejected alternative stays rejected; ordinary entailment
  does not rest on a judgment that runs after it, because what it defers is
  checked later and nothing it has proved changes.
- Cost: an accepted program today defers nothing, so its checking time is
  unchanged. A function that takes part already pays the range walk; a
  deferred goal adds one problem at its point.
- Limit: the range walk evaluates only literals, consts, bindings, measures,
  element reads, exact `+`, `-`, `*` by a constant and exact `cvt`
  ([RANGE-2]); every other expression is an unknown, and the walk does not
  hold ordinary entailment's automatic operation facts, ordinary callee
  postconditions or L0 closure. A goal needing both such a fact and a range
  fact is proved by first stating the ordinary part as a local invariant,
  whose target the walk then holds. This is an explicit proof step the
  language already has, not a run-time test.

### B. Ordinary entailment gains element-read terms and range facts as a fact source

[ENT-2] admits an integer element read `p[i]` as a term with proof-path
identity; [ENT-3] adds a source that activates range requirements at entry,
range invariants at a loop header and callee range postconditions after a
call; a goal instantiates an active fact at the element reads it contains
when the instance's ranges, guards and read existence are entailed. [ENT-5]
kills a range fact by overlap with its storage.

- Combines range instances with every ordinary fact in one proof.
- Needs a second model of what the range walk already models: activation,
  routed postconditions in `match` arms, and copy provenance. The range
  judgment keeps its own place model already
  (`docs/todo.md`, the range judgment's world against `places.rs`).
- Precision: any element write kills the whole fact, which is exactly the
  compiler tree's rejection ground. A loop that writes some rows and reads
  others loses the fact at its first write.
- Cost: every function with range facts pays the new source at every goal,
  accepted programs included.

### C. Keep the boundary

Leave the refusal and require run-time tests. Refused by the owner's ruling.

## Criterion before implementation

Route A is supported if, with an experiment compiler implementing it:

- witnesses 1, 2 and 3 are accepted with no run-time test and no written
  instance beyond the one in witness 3;
- their negative twins are refused with the original ordinary rule: a fact
  whose bound is one too weak, a write that breaks the fact before the read,
  a read outside the fact's range, and a deferred invariant target that is
  false;
- every case accepted before keeps its verdict, and the checking time of the
  conformance suite and of the natural-form v2h does not change beyond
  runner variance (measured in CI, same-source before and after).

Route A is rejected if a witness needs an ordinary-only fact that cannot be
stated as a local invariant, or if a negative twin is accepted.

## Specification changes route A needs

- [RANGE-2]: replace "no ordinary obligation consumes a range fact" with the
  deferral above, and add deferred obligations to the sites the walk proves.
- [RANGE-3]: add the deferred ordinary obligation as a fifth owed site, a
  clause without bound variables.
- [RANGE-4]: admit `use NAME(terms)` in an `invariant_stmt`, where NAME is a
  range requirement of the function or a range invariant of an enclosing
  loop.
- [ENT-1] and the rules that state ordinary obligations: an obligation left
  open in a function that takes part is owed to the range judgment rather
  than rejected.

The design tree changes with them: the language node's "No ordinary
obligation consumes a range fact" decision is replaced, its first two
rejected alternatives are retired with the reason that changed, and the
compiler node records the deferral.
