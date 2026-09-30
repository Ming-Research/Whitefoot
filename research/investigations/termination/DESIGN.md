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

Koka's `div` effect is the nearest precedent: functions are total unless
their type carries `div`, which is inferred and propagates to callers. The
candidate differs in three ways. Whitefoot would never infer divergence from
a missing proof, so a loop without evidence is a rejection, not a widened
type. The marker is the existing `waits`, which a caller already sees, and
not a new effect. And a waiting function still owes progress between its
waits.

## Prior design

The deferred [fixed-resource investigation](../fixed-resource-execution/DESIGN.md#source-proposal)
drafted a `decreases` rank:

- one written scalar `u64` rank per loop or recursive function;
- descent proved on every backedge, or at every self call, against an erased
  entry snapshot;
- the proof uses the existing INV-1 and PRF-1 machinery, with no inference;
- mutual recursion, lexicographic ranks and structural measures were deferred
  "until needed".

Its rank was optional, because it served a requested completion report. This
investigation asks whether that design, or a named extension of it, suffices
as a mandatory rule.

Two facts of the current language keep the analysis finite:

- A function-kind parameter is a compile-time parameter, never a value
  [FN-3], so every call reaches a concrete instance. The call graph after
  instantiation is static, and D7's instantiation rule (`design/language/generics.md`)
  already keeps it finite.
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
Its per-entry records are in `runs/`.
- **Loops.** `runs/loops.tsv` has one row per `loop`.
- **Recursion.** `runs/recursion.tsv` has one row per recursive component. A component that merged same-named functions of separate programs is split into its real parts.
- **Classification.** Each entry was read by a mid-sized model working against a fixed rubric. I spot-checked the entries that decide a criterion; the rest are recorded, not rechecked.

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

- **A scalar `u64` rank covers 460 loops (91 percent).** These are the C1 to C3 loops, 345 of the 358 in Snowghost.
  - 294 of them need only facts local to the loop: its exit guard and its update.
  - 166 also need a callee fact, typically that a callee advances a cursor (`next_char`, `read_at`) or pops exactly one element (`pop_open_if_any`). Several such facts are stated only in prose: `next_char`, `line_end`, `run_end`, `find_min_codepoint_at_least`, and the strict advance of `send_once`, `write_once` and `read_at`. None is a contract `ensures` today.
- **The waiting rule covers 25 loops.** These are server, pipe, file and directory loops that make a waiting call every iteration. Some would also have a scalar rank. The write and read loops whose contracts allow a call to make no progress (`start <= next <= end`) terminate only under this rule.
- **The other 19 loops need more than a scalar rank:**
  - lexicographic ranks: the 6 C7 loops and `walk_rules`, although one C7 loop, `match_complex`, orders arena positions and so also needs the acyclicity fact below;
  - descent through owned links: the 5 C5 loops;
  - an acyclicity fact that no current rule can state: the 4 C6 loops and `expand_cp`;
  - rewriting: the 2 C10 loops.

### Recursion

The name-based call graph found 45 candidate components.
- One is an artifact: a loop label read as a call.
- Four merged same-named functions from separate programs and split into 11 rows.

That leaves 51 real rows.

| Class | Snowghost | Whitefoot | Total |
|---|---:|---:|---:|
| C1 cursor | 7 | 4 | 11 |
| C2 countdown | 10 | 7 | 17 |
| C5 owned structure | 5 | 11 | 16 |
| C7 lexicographic | 2 | 1 | 3 |
| C6 arena | 2 | 0 | 2 |
| C10 other | 2 | 0 | 2 |

Structural descent through owned data matters far more for recursion than for loops.
- Layout, style profiling, the ordered map and the tree programs all recurse into an owned child.
- Ownership already makes such a tree finite and acyclic, so the descent needs no numeric measure.
- The two C10 rows:
  - `decompose` follows an external Unicode table whose acyclicity is a fact about data.
  - A static cycle between `mode_in_body` and `mode_in_template` is infeasible, because its two edges need different token kinds.

### The tree builder (criteria 2 and 3)

- **Criterion 2 holds.** Before the fix, the breakout path for an end tag at an integration point returned to the loop header with the parser state unchanged. The recursion census rediscovered this independently. It built Snowghost main at 09d33ba, which lacks the fix, and six inputs timed out, including `<svg><desc></br>` and `<math><mtext></br>`. The fixed driver parses both correctly. No measure falls on an edge that leaves the state unchanged, so every sound rule rejects the pre-fix program.
- **Criterion 3 holds, but not with a scalar rank.** `runs/reprocess/reprocess.md` lists all 51 edges on which the fixed tree builder reprocesses a token, and checks a measure against them. The count is confirmed: 49 `True` literals in the mode files, and none among the 39 returns of `in_body_start_tag`. The measure is lexicographic:

  `(templates on the open-element stack, R[token class][mode], open-element stack depth)`

  `R` is a fixed table of 21 modes by 13 token classes.
  - No rank per mode alone works, because pairs such as in-body and after-body need opposite orders for different end tags.
  - The template count and the stack depth are needed because `<table>` and end-of-file reprocess through reset edges.
  - One token reprocesses at most 6 times, except end-of-file (open templates + 2) and `<table>` (stack depth + 5).

  The edge table is a hand transcription, and `check.py` checks the measure against that transcription, not against the compiled program.

So the fixed tree builder is provable, but only with a lexicographic rank, a rank read from a constant table, and a template counter the program maintains. With scalar ranks alone, the writer's recourse is a counted retry. Its bound is `7 + templates + depth`, and exhausting it is an explicit outcome that the analysis shows is never reached.

### Checking cost and authoring (criteria 4 and 5)

Each rank form stays deterministic and polynomial:
- scalar and lexicographic descent are checked by INV-1 queries at each backedge or call;
- owned descent is a syntactic check that the argument is a proper owned part of a parameter;
- the waiting rule is a check that a waiting call occurs on every path back to the header.

None of them searches or infers a measure.

Probes against the current checker at 7ad65dc7 are in `runs/probes/`. A descent invariant `before < index` or `after < before` was written after the update.

| Probe | Shape | Result |
|---|---|---|
| p1 | `set index = index +wrap 1_u64` after `index < need` | proved |
| p2 | `set index = index + 1_u64` after `index < need` | proved |
| p4 | callee with `ensures next > pos` | proved |
| p5 | `take_back` on a `Slots` window | proved |
| p3e | advance 1 or 3 through a value `if`, clamped by an `if` that sets `pos` | proved |
| n1 | `set index = index +wrap 0_u64` (control) | refuted |
| p3 | Snowghost's `pos +wrap advance` then clamp (`url/host.wf`) | unproved |
| p3b, p3c | a mutable `step` clamped through a join | unproved: `1 <= step` is lost at the join |

- **p3 is right to fail.** Near `u64::MAX` the wrapped sum is smaller than `pos`, so that loop does not terminate. A mandatory rule rejects that idiom, and the writer must use a non-wrapping advance.
- **p3b and p3c** repeat the join loss recorded in the [writer-lost facts investigation](../writer-lost-facts/DESIGN.md).
- The local C1 to C3 shapes needed no `use` step.

## Found along the way

- **Snowghost `tools/normalization_tables/expand.wf`, `expand_cp`.** It loops forever on a decomposition cycle in its input, such as a code point that decomposes to itself, because each pop is matched by a push. The tool reads the Unicode data at build time.
- **Snowghost `html/tree_builder/insert.wf`, `move_all_children`.** It loops forever if its source and destination are the same node. Both callers pass distinct nodes.
- **Whitefoot `lib/std/collections/hash_map`, `hash_map_rebuild`.** Its re-insertion loop ends only by a pigeonhole argument in its documentation: the probe visits every bucket, and fewer buckets are occupied than exist. No contract carries that argument.
- **Snowghost `foreign.wf`.** `process_foreign_content` returns "reprocess" for end-of-file, which would loop if the dispatcher ever sent end-of-file to foreign content; today it does not. The template-mode pop on end-of-file does nothing on an empty stack, so its termination rests on the template count.
- **Contracts.** Many cursor and I/O contracts state progress only in prose, or allow no progress (`start <= next <= end`). A mandatory rule turns each into a missing `ensures`.

## Verdict against the criteria

Mandatory progress is viable for the census scope, with three measure forms
beyond the scalar rank and one rewrite:

| Form | Loops | Recursion |
|---|---:|---:|
| scalar `u64` rank (C1 to C3) | 460 | 28 |
| a waiting call on every iteration (C9) | 25 | 0 |
| descent through owned data (C5) | 5 | 16 |
| lexicographic rank (C7 except `match_complex`, `walk_rules`) | 6 | 3 |
| counted walk with an explicit outcome (C6, `match_complex`, `expand_cp`, `decompose`) | 6 | 3 |
| rewrite (C10) | 2 | 1 |

- **Criterion 1 holds with these forms, except for arena acyclicity.** No rule, current or proposed, can state that index links in a storage are acyclic: a quantified storage invariant is a refused alternative in `design/language/checks-and-proofs.md`. The nine arena and external-data sites therefore need a counted walk. They include `match_complex`, whose lexicographic measure is over arena positions. The walk takes at most the arena's length in steps, with an outcome for exhaustion. A later "ranked arena" type, whose link writes prove a rank order, could replace the counted walk if its cost is justified.
- **Criteria 2 and 3 hold.** The fixed tree builder needs the lexicographic form and a constant rank table, as shown above.
- **Criterion 4 holds.** Every form is a fixed deterministic check.

## Open design choices

1. **Written or derived ranks.** The fixed-resource draft required a written rank everywhere and inferred nothing. For a mandatory rule that means a `decreases` clause on each of the 504 loops. A specification-fixed derivation could instead read a rank from the exit guard's form: `a < b` gives `b - a`, `x != 0` gives `x`, `w.len == 0` gives `w.len`. It is syntactic, with no search, the same kind of fixed family [ENT-1] already uses. The derived rank is proved like a written one, and a loop outside those forms writes its own. The 294 local C1 to C3 loops would then need no annotation.
2. **Waits as the only divergence.** Under the candidate rule, a function that does not wait always returns. A `waits` function returns, or waits again, in finitely many steps. There is no separate divergence effect.
3. **Constitution.** The sentence "Logic errors, including unintended nontermination, may remain" would narrow to exclude nontermination between waits, and the [WAIT-2] premise would become a theorem.
