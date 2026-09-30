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
