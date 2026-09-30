# Every step sequence reaches a return or a wait

## Question

Can Whitefoot require every accepted program to make progress, so that no
context runs forever without returning or waiting, and can one mechanism cover
every loop and recursion that Snowghost and Whitefoot's own programs need?
The mechanism would be the default shape of every loop and function, not an
optional proof a writer may leave out.

The witness is Snowghost's HTML tree builder before
[Snowghost PR #22](https://github.com/mbbill/Snowghost/pull/22). A `</p>` end
tag at an HTML integration point popped nothing and asked the dispatcher to
reprocess it, and the dispatcher sent it back to the same branch forever. The
checker accepted that program, and only a test harness with a timeout could
find it.

## What changes if the answer is yes

- **The constitution changes.** Its Safety section says "Logic errors,
  including unintended nontermination, may remain" (`docs/constitution.md`).
  A mandatory rule replaces that sentence for step sequences between waits.
- **An existing premise becomes a theorem.** [WAIT-2]'s progress guarantee
  holds "while every context, from every point of its execution, reaches in
  finitely many steps its completion or a wait". Today nothing establishes
  that premise, so a spinning context voids the guarantee for every context.
  A mandatory rule would discharge it for every accepted program.
- **Specification and design text change.** The specification says of the
  atomic update "Nothing is said about termination", and that "v0 provides no
  termination checker". `design/language/effects.md` says `pure` "promises
  nothing about termination".

## Where divergence lives

A server loop, an event loop and a context that waits on a guard run without
end by design. The candidate rule does not need a second opt-out effect for
them: [WAIT-2] already separates finite step sequences from waits. The
candidate obligation is that every step sequence reaches a return or a wait in
finitely many steps. A loop whose every iteration makes a waiting call meets
it without a rank, and that call can occur only in a function that declares
`waits` [WAIT-1]. Divergence is therefore visible in the signature that
already carries it, and nothing is optional.

Koka's `div` effect is the nearest precedent
([the Koka book](https://koka-lang.github.io/koka/doc/book.html), "Effect
Typing" and its recursive `fib` example). A Koka function is total unless its
type carries `div`. When the inference engine cannot prove that a recursive
function terminates, it adds `div` to the function's type, and the effect
propagates to callers. There is no written rank.

The candidate differs in four ways:

- A missing proof is a rejection, not a widened type.
- The marker is the existing `waits`, not a new effect.
- A waiting function still owes progress between its waits.
- A writer can supply a rank that no fixed check finds.

## Prior design

The deferred [fixed-resource investigation](../fixed-resource-execution/DESIGN.md#source-proposal)
drafted a `decreases` rank:

- one written scalar `u64` rank per loop or recursive function;
- for a loop, descent proved on every backedge against the current
  iteration's header rank;
- for a self call, descent proved against an erased snapshot of the rank at
  the caller's entry;
- the proof uses the existing INV-1 and PRF-1 machinery, with no inference;
- rank expressions are affine, and calls, moves and side effects are
  excluded from them;
- mutual recursion and "richer measures" were deferred "until needed", and
  "wider or lexicographic measures require a later admission change".

Its rank was optional, because it served a requested completion report. This
investigation asks whether that design, or a named extension of it, suffices
as a mandatory rule.

Two facts of the current language keep the analysis finite:

- A function-kind parameter is a compile-time parameter, never a value
  [FN-3], so every call reaches a concrete instance. The call graph after
  instantiation is static. The instantiation-cycle decision in
  `design/language/generics.md` keeps it finite: a cycle must forward its
  argument vector unchanged.
- A counted `for` has `u64` endpoints and a compiler-owned binder that source
  cannot write [SET-1], so it terminates when its body does.

The remaining obligations are therefore the `loop` statements and the
recursive components of the call graph.

## Criteria, fixed before the census

The census classifies each `loop` statement and each recursive component in
scope by the measure its termination needs. The classes are assigned before
any mechanism is judged.

| Class | Measure | Example |
|---|---|---|
| C1 cursor | `bound - cursor`: an ascending `u64` cursor with an exit at the bound | scan bytes until `pos >= len` |
| C2 countdown | a `u64` value that falls to an exit at zero | retries left |
| C3 shrink | the length of a container that only shrinks | pop until empty |
| C4 worklist | pushes and pops, bounded by a structural fact such as "each node pushed once" | explicit-stack traversal |
| C5 owned chain | descent along owned `Box` or `Option<Box>` links | owned linked list |
| C6 arena walk | index or id links (parent, next sibling) followed until none; finite only if the arena is acyclic | walk to the root |
| C7 lexicographic | several components, one falling while earlier ones hold | tokenizer reconsume, tree-builder reprocess |
| C8 fixed point | repeat until nothing changes, bounded by a monotone quantity | convergence loop |
| C9 waiting | every iteration makes a waiting call | server accept loop |
| C10 other | none of the above; the entry states why | |

Mandatory progress is **viable** for the census scope only if all four hold:

1. Every entry falls in a class a single rule covers: a scalar `u64` rank
   (C1 to C3), a waiting iteration (C9), or a named extension. An extension
   names the fact it needs, such as a lexicographic rank, structural descent
   on owned data, or an arena whose links are checked to point one way. If
   any entry needs a fact no proposed extension can state, one mechanism does
   not cover everything.
2. The pre-fix tree builder is rejected: its reprocess path has no measure
   that falls.
3. The fixed tree builder can be proved with finite written steps under the
   same rule.
4. Checking stays deterministic and polynomial. It uses no search, no
   inferred ranks and no budget, as `design/language.md` and
   `design/language/checks-and-proofs.md` require of every acceptance
   judgment.

A fifth observation measures the cost of writing proofs, but it does not
decide viability. It records how many C1 loops the current checker already
proves descending without a written `use` step, tested by inserting the
descent invariant into a sample of real loops and compiling them.

## Scope and method

- **Programs.**
  - Snowghost `renderer/` at 09d33ba;
  - Whitefoot `lib/` and `tests/programs/` at 7ad65dc7.
- **Enumeration.** `loop` statements are listed with
  `grep -rnE '^\s*loop( @[a-z_0-9]+)? *[({]'`. The recursive components come
  from a call graph over function names, with each candidate checked by hand.
- **Classification.** Each entry is read by hand against the table, and the
  raw per-entry verdicts are kept in `runs/`.

## Results

The census was run after the criteria above were committed (0d5d42ab).
Its per-entry records are in `runs/`:

- **Loops.** `runs/loops.tsv` has one row per `loop`.
- **Recursion.** `runs/recursion.tsv` has one row per recursive component.
  A component that merged same-named functions of separate programs is split
  into its real parts.
- **Tree builder.** `runs/reprocess/` holds the reprocess-edge analysis. It
  is an agent's working report with its model scripts.
- **Hang witness.** `runs/hang/` holds the hang reproduction.
- **Sample.** `runs/sample/` holds the real-loop sample.

Each census entry was classified by a mid-sized model against a fixed rubric,
and the classifications are recorded, not independently rechecked. I rechecked
by tool only the facts that decide a criterion: the hang reproduction, the
reprocess edge count, and the sample.

The census programs were written to exercise the compiler, so their class
frequencies say what kinds of loops occur, not how often real programs need
each kind. They are evidence that a form is needed. They do not weigh one
form against another.

The classification is fallible. At 09d33ba, loop 15 (`process_token`'s
reprocess loop) did not terminate, yet the loop census classed it C7
"terminating" with medium confidence after sampling some modes. Only the
recursion census, which ran the code, found the hang.

### Loops

| Class | Snowghost | Whitefoot | Total |
|---|---:|---:|---:|
| C1 cursor | 260 | 91 | 351 |
| C2 countdown | 53 | 11 | 64 |
| C3 shrink | 32 | 13 | 45 |
| C9 waiting | 1 | 24 | 25 |
| C7 lexicographic | 6 | 0 | 6 |
| C5 owned chain | 0 | 5 | 5 |
| C6 arena walk | 3 | 1 | 4 |
| C4 worklist | 2 | 0 | 2 |
| C10 other | 1 | 1 | 2 |
| **All** | **358** | **146** | **504** |

- **C1 to C3: a scalar `u64` rank (460 loops).**
  - The rubric's `needs` field splits them roughly into 294 whose descent
    follows from the loop's own guard and update, and 166 that also need a
    callee fact.
  - The split is approximate. It keys on the field's first word, so a few
    purely arithmetic entries land in the second group, and a few that rest
    on a library contract such as `take_back`'s land in the first.
  - The callee facts are typically a cursor advance (`next_char`, `read_at`)
    or a single pop (`pop_open_if_any`). Several are stated only in prose,
    not as a contract `ensures`: `next_char`, `line_end`, `run_end`,
    `find_min_codepoint_at_least`, and the strict advance of `send_once`,
    `write_once` and `read_at`.
- **C9: an iteration that waits (25 loops).**
  - Kinds: server, pipe, file and directory loops; busy and poll loops
    around an `atomic` statement; background loops that sleep.
  - Under the candidate rule they are admitted and need not terminate.
  - The write and read loops need that admission for a second reason: their
    contracts (`start <= next <= end`) let a call make no progress.
  - `poll_contexts` spins on an unguarded `atomic` statement. It is admitted
    only because [WAIT-2] counts every atomic statement as a wait.
- **The other 19 loops need more than a scalar rank:**
  - lexicographic ranks: the 6 C7 loops and `walk_rules`. One C7 loop,
    `match_complex`, orders arena positions, so it also needs the
    acyclicity fact below.
  - descent through owned links: the 5 C5 loops.
  - an acyclicity fact about index links or input data: the 4 C6 loops and
    `expand_cp`.
  - rewriting: the 2 C10 loops, a two-state flag and `hash_map_rebuild`'s
    pigeonhole argument.

### Recursion

The name-based call graph found 45 candidate components:

- one is an artifact, a loop label read as a call;
- four merged same-named functions from separate programs and split into 11
  rows.

That leaves 51 real rows.

| Class | Snowghost | Whitefoot | Total | of which mutual |
|---|---:|---:|---:|---:|
| C1 cursor | 7 | 4 | 11 | 2 |
| C2 countdown | 10 | 7 | 17 | 2 |
| C5 owned structure | 5 | 11 | 16 | 3 |
| C7 lexicographic | 2 | 1 | 3 | 2 |
| C6 arena | 2 | 0 | 2 | 2 |
| C10 other | 2 | 0 | 2 | 1 |

- **Owned structure.** Descent through owned data is common in recursion:
  layout, style profiling, the ordered map and the tree programs all recurse
  into an owned child. Ownership already makes such a tree finite and
  acyclic.
- **Mutual recursion.** 12 of the 51 rows are components of several
  functions. The fixed-resource draft left these unsupported. A mandatory
  rule needs one rank shared by every entry of the component, with descent
  on every edge inside it.
- **C10.**
  - `decompose` follows an external Unicode table whose acyclicity is a fact
    about data.
  - The static cycle between `mode_in_body` and `mode_in_template` is
    infeasible, because its two edges need different token kinds. A static
    rule sees the cycle anyway, so the code must be restructured or given a
    rank.

### The tree builder (criteria 2 and 3)

**Criterion 2 holds.**

- `runs/hang/` reproduces the witness. The tree-construction oracle driver
  was built at Snowghost 09d33ba, before the fix, and at 1120edf, the head of
  Snowghost PR #22, with `whitefootc --graph modules.wfg --entry
  html_tree_oracle` from Snowghost's pinned compiler, 290b575b.
- Each of six inputs ran under a 20-second limit. They put an end tag `br`
  or `p` at each kind of integration point: `foreignObject`, `desc`, `mi`,
  `mtext` and `annotation-xml`.
- The 09d33ba driver produced no result for any of the six. The 1120edf
  driver produced the expected tree for all six.
- On the hanging path the parser state is unchanged when the loop returns to
  its header, so no measure falls, and every sound rule rejects the program.

**Criterion 3 is not demonstrated.**

- `runs/reprocess/reprocess.md` lists the 51 edges on which the tree builder
  at 1120edf reprocesses a token. The count is confirmed by tool: 49 `True`
  literals in the mode files, and none among the 39 returns of
  `in_body_start_tag`.
- It gives a lexicographic measure:

  `(templates on the open-element stack, R[token class][mode], open-element stack depth)`

  Here `R` is a constant table of 21 modes by 13 token classes.
- `check.py` checks the measure against a hand transcription of the edges,
  not against the program, and nothing was compiled.
- The measure needs three things the checker cannot express today:
  - a lexicographic rank;
  - a rank read from a constant table: the fixed-resource draft admits only
    affine rank expressions, and no probe has tried a table read;
  - a template counter that the program would maintain beside the stack.
- No rank per mode alone works. Pairs such as in-body and after-body need
  opposite orders for different end tags.
- Per token, the analysis bounds the iterations at 6, except:
  - end-of-file: at most `6 + 4 × templates`, and `templates + 2` is
    attained;
  - `<table>`: at most `depth + 5`.
- With scalar ranks alone, the writer's recourse is a counted retry of at
  most `6 + 4 × templates + depth + 5` iterations, which covers each of these
  bounds. Its exhaustion outcome would be dead code that the checker cannot
  prove dead.

### Checking cost (criterion 4)

Each rank form is a deterministic check with no search and no inferred
measure:

- **Scalar and lexicographic descent:** INV-1 queries at each backedge or
  call inside the component.
- **Owned descent:** a syntactic check that the argument is a proper owned
  part of a parameter.
- **The waiting rule:** a check that a waiting call occurs on every path back
  to the header.

Per concrete instance, each adds work linear in the loops and calls it
checks. The checks run on every concrete instance, so they inherit the
instantiation fan-out recorded in `docs/todo.md`: acyclic instantiation can
multiply instances exponentially in source size. Nothing in this record
measures checking time.

### Authoring (observation 5)

**Probes.** Synthetic probes are in `runs/probes/`, compiled with the gate
compiler built at 7ad65dc7. Each writes a descent invariant after the update.

| Probe | Shape | Result |
|---|---|---|
| p1 | `set index = index +wrap 1_u64` after `index < need` | proved |
| p2 | `set index = index + 1_u64` after `index < need` | proved |
| p4 | callee with `ensures next > pos` | proved |
| p5 | `take_back` on a `Slots` window | proved |
| p3e | advance 1 or 3 through a value `if`, then `if` sets `pos` to `length` or `pos + advance` | descent proved; the loop's `pos <= length` header invariant is not preserved |
| p3f | p3e without that header invariant | proved |
| n1 | `set index = index +wrap 0_u64` (control) | refuted |
| p3 | Snowghost's `pos +wrap advance` then clamp (`url/host.wf`) | unproved |
| p3b, p3c | a mutable `step` clamped through a join | unproved: `1 <= step` is lost at the join |

p3 is right to fail. Near `u64::MAX` the wrapped sum is smaller than `pos`,
so that loop does not terminate.

**Real-loop sample.** `runs/sample/` samples 20 Snowghost C1 to C3 loops from
09d33ba, drawn with seed 20260930 from the 221 outside `oracle/` and
`tools/`. Each gets a snapshot at the top of its body and a descent
invariant at its end, and its module is checked with Snowghost's pinned
compiler. Two proved loops with the invariant reversed are refuted, as a
control.

| Outcome | Loops |
|---|---:|
| descent proved with no `use` step | 10 |
| rank not expressible: `buffer^.index`, a field read through a reference, "is not a measure" | 3 |
| callee advance not in a contract (`next_char`, `peek_pp`) | 3 |
| a fact lost at a join or at an inner loop's exit | 3 |
| an operation fact missing: `ishr.wrap(e, 1)` is smaller than a nonzero `e` | 1 |

Half the sample needs no written step. The per-row cause is in `results.tsv`, and
the loops and the draw are in `sample.tsv` and `insert_descent.py`. The
other half splits into four
distinct gaps:

- the rank vocabulary must admit fields read through references;
- progress facts must move from prose into `ensures`;
- joins drop facts, the shape in the
  [writer-lost facts investigation](../writer-lost-facts/DESIGN.md);
- the operation table lacks a halving fact.

## Found along the way

- **Snowghost `tools/normalization_tables/expand.wf`, `expand_cp`.** It loops
  forever on a decomposition cycle in its input, such as a code point that
  decomposes to itself, because each pop is matched by a push. The tool
  reads the Unicode data at build time. Disposition: reported to the owner
  in the handoff of this record, for Snowghost's own tracking.
- **Snowghost `html/tree_builder/insert.wf`, `move_all_children`.** It loops
  forever if its source and destination are the same node. Both callers pass
  distinct nodes. Disposition: reported to the owner, as above.
- **Snowghost `foreign.wf`.** `process_foreign_content` returns "reprocess"
  for end-of-file, which would loop if the dispatcher ever sent end-of-file
  to foreign content; today it does not. Disposition: reported to the owner,
  as above.
- **Whitefoot `hash_map_rebuild`.** Its re-insertion loop ends only by a
  pigeonhole argument in its documentation. Disposition: added to
  `docs/todo.md`.
- **Prose-only progress facts.** Many cursor and I/O contracts state
  progress only in prose, or allow no progress. For Whitefoot's `read_at`,
  `write_once` and `send_once`, a zero-length transfer is a legitimate host
  outcome, so the contract cannot promise progress and no `docs/todo.md`
  item is added. Snowghost's own prose-only facts, such as `next_char` and
  `peek_pp`, are Snowghost's to state.

## Verdict against the criteria

| Form | Loops | Recursion |
|---|---:|---:|
| scalar `u64` rank (C1 to C3) | 460 | 28 |
| a waiting call on every iteration (C9) | 25 | 0 |
| descent through owned data (C5) | 5 | 16 |
| lexicographic rank (C7 except `match_complex`, `walk_rules`) | 6 | 3 |
| an acyclicity fact about index links or data (C6, `match_complex`, `expand_cp`, `decompose`) | 6 | 3 |
| rewrite (C10) | 2 | 1 |

- **One mechanism does not cover everything.** A scalar rank covers 460 of
  504 loops and 28 of 51 recursive rows.
- **A family of rank forms, with one gap.** A single rule, "every cycle
  carries a checked descent or a wait", covers every other entry except nine
  acyclicity sites and three sites that must be restructured: a two-state
  flag, `hash_map_rebuild` and the infeasible `mode_in_body` cycle. It needs three forms beyond the scalar rank: lexicographic,
  owned-structural and waiting. It also needs shared ranks for mutual
  recursion.
- **Criterion 1 is not met for the nine sites.** They need to know that
  index links in a storage, or an input table, are acyclic.
  - No current rule can state that. A quantified storage invariant is a
    refused alternative in `design/language/checks-and-proofs.md`.
  - The criterion named "an arena whose links are checked to point one way"
    as a possible extension, but this record did not design or test one.
  - A counted walk (at most the arena's length in steps) would make each
    site terminate. That is a rewrite outside the criterion: it adds a
    runtime counter and an outcome the checker cannot prove unreachable.
- **Criterion 2 holds.**
- **Criterion 3 is not demonstrated.** A measure exists on paper, but it
  needs a lexicographic rank read from a constant table, and nothing
  compiled it.
- **Criterion 4 holds for the forms described.** The checking cost is
  unmeasured and inherits the known instantiation fan-out.

## Open design choices

1. **Arena acyclicity.** A ranked arena would carry the acyclicity proof;
   counted walks would not.
   - A ranked arena is a library or language type whose link writes prove an
     order, such as a parent's rank being below its child's. The proof
     stays static, but the type must be designed and paid for at every link
     write.
   - A counted walk keeps today's data structures but adds a runtime branch
     and an unprovable outcome to every such walk.
   - This choice decides whether one rule covers the nine sites.
2. **Written or derived ranks.** The fixed-resource draft required a written
   rank and inferred nothing.
   - A specification-fixed derivation could read a rank from the exit
     guard's form: `a < b` gives `b - a`, `x != 0` gives `x`, `w.len == 0`
     gives `w.len`. It is syntactic and involves no search, like the fixed
     families of [ENT-1]. A loop outside those forms writes its own rank.
   - The derived form makes the common loop's default shape carry no
     annotation.
   - The written form makes every loop's argument visible in its text, and
     keeps the rule free of guard-pattern matching.
3. **What counts as a wait.** The candidate rule admits a loop because it
   waits, not because it progresses.
   - A waiting call that completes at once without progress, such as a read
     that returns `next == start`, satisfies the rule forever. So does a
     spin on an unguarded `atomic`.
   - A separate divergence effect would instead mark such functions
     explicitly.
   - Excluding unguarded `atomic` statements from the waits that count would
     turn such spins into rejections.
4. **Mutual recursion and table ranks.** The rule needs a rank shared across
   a component. The tree builder also needs a rank read from a constant
   table. Neither has a design yet.

If the rule is adopted, the constitution's Safety sentence "Logic errors,
including unintended nontermination, may remain" narrows to exclude
nontermination between waits, and the [WAIT-2] premise becomes a theorem.
The constitution accepts additional proof work "within the constraints of
required safety and practical development feasibility". Whether progress
between waits is required safety is the owner's decision; the four gaps in
the sample bear on feasibility.

## Owner rulings

- **Q15.** The owner chose the checked-descent direction, on the condition
  that it be mandatory rather than an optional proof. An optional proof can
  be left out, which contradicts the principle that the default shape is the
  best shape.
- **Q16.** The owner approved each recommendation:
  - adopt the mandatory rule, under which every cycle carries a checked
    descent or a wait;
  - derive a loop's rank from its exit guard's form, and write it only
    outside those forms;
  - close the acyclicity gap with a ranked arena, not with counted walks;
  - count only waiting host calls, guarded `atomic` statements and joins as
    waits, so a spin on an unguarded `atomic` owes descent.
- **Arena answer.** Asked whether the ranked arena is static, the owner was
  told:
  - walks carry no runtime cost;
  - an arena built in index order proves its order statically;
  - a relinkable arena such as the DOM needs the cycle check that the DOM
    standard already requires, and Snowghost already performs in
    `refuse_cycle`, with the rank kept proof-only.

  The owner approved the next step on that basis: design the ranked arena,
  shared ranks for mutual recursion and constant-table ranks.

These rulings select the direction. The design-tree nodes and the
specification change land together with the rule's implementation, where
the existing decisions they replace are rewritten:
- `design/language/effects.md`: `pure` promises nothing about termination;
- `design/language/checks-and-proofs.md`: no added termination checker;
- the specification's "no termination checker" sentences.
