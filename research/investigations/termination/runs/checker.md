# Snowghost under the implemented loop rule

This run measures what the implemented [TERM-1] check does to Snowghost's
loops as they stand, before any migration. It is evidence of the rule's effect
on a real program, not a ground for choosing a form: each failure class below
witnesses a gap or an authoring cost, and `docs/todo.md` states each gap as a
minimal witness.

## Method

- Compiler: this branch at `e40dad21` with `checker-report.patch` applied.
  The patch reports every loop's form on standard error when
  `WF_TERM1_CENSUS` is set, and in that mode lets a loop with no form, or a
  rank that does not fall, pass, so checking continues to the next loop. It
  changes nothing else. The patch lives here until the investigation closes
  or the compiler gains a per-loop progress report.
- Program: Snowghost `renderer/` at `09d33ba`, copied, with
  `clock: unused_clock, wall_clock: unused_wall_clock` added to each
  `let Inputs(...)` destructure. Main's v0.83 added those fields; the edit
  is unrelated to TERM-1.
- Each module of `renderer/modules.wfg` was checked with
  `--graph modules.wfg --check-module <module>`.
  - 20 of the 43 modules were checked to the end: every library module and
    two oracle modules.
  - The other 23 (oracles, table tools and prototypes) stopped at a TYPE-5,
    GRAM-11 or ERR-2 rejection from main's library changes, so their loops
    are partial. Their derived-rank loops before the stop were formed but
    never reached the proof walk.
- `checker-loops.tsv` has one row per loop. It gives the form, whether the
  module was checked to the end, and the descent relation of a derived rank.
- The 20 complete modules have 229 loops. This matches a `grep` of the
  `loop` keyword in the same directories, for example 63 in
  `html/tree_builder` and 19 in `css/syntax`.

## Results for the 20 complete modules

| Outcome | Loops |
|---|---|
| derived rank, descent proved | 142 |
| derived rank, descent not proved | 30 |
| no form applies | 57 |

In Snowghost no loop waits, none descends structurally, and none writes a
rank yet. The structural form's evidence remains the owned-walk programs of
the Whitefoot corpus ([migration](migration.md#owned-structure)).

The 57 loops with no form, by cause, read from each loop's first statements:

| Cause | Loops | Remedy |
|---|---|---|
| The exit test reads a length or field through a reference, via an accessor call or `buffer^.index` | 21 | 14 tree-builder stack loops: a written rank over the measure (probe B). 7 font loops over the scalar field `buffer^.index`: none, the field-atom gap |
| Tokenizer character loops that exit when `next_char` returns the end-of-file character | 16 | a written `decreases length - position` and a `next_char` postcondition; loops that change phase without consuming also need a lexicographic rank |
| The exit test is not among the leading statements | 9 | a written rank |
| The exit condition is not a comparison: a `Bool` flag, `band`, or a `cvt` operand | 4 | reorder the tests, or the flag-test gap |
| The exit operand is a `+wrap` sum | 2 | exact `+` |
| An equality exit | 1 | `>=` |
| A DOM link walk, the tree builder's reprocess loop and its token loop | 3 | ranked arena and table ranks ([ARENA.md](../ARENA.md)) |
| The exit operand is a call result | 1 | a written rank |

The 30 loops whose rank does not fall mostly advance a position through a
helper's result, such as `peek_pp`'s `next`, whose contract does not say the
result exceeds the argument.

## Probes

Each probe edits the Snowghost copy and rechecks one module with the same
compiler.

- **A. Helper advance (`pkg::css::syntax`).** Before the probe, 6 ranks fell
  and 13 did not. The probe made three edits:
  - it added `ensures next > pos;` to `decode_utf8` and `peek_pp`, and
    `ensures next >= pos;` to `consume_escape`;
  - it changed their `+wrap` steps to exact `+`;
  - it added `invariant ahead: pos < cur` to `consume_escape`'s hex loop.

  After it, 10 fell, 9 did not, and the module was accepted.
  - With `+wrap` the postcondition `next > pos` was unproved at
    `return value, next`, although `pos < length` held.
  - With exact `+` it was proved. The derivation gap is exactly this: a
    difference bound `pos < length` does not make `pos +wrap 1` exact.
  - The remaining 9 advance through other helpers that the probe did not
    edit.
- **B. Stack measure through a reference (`pkg::html::tree_builder`,
  `stack.wf` `pop_until_node`).** The probe made three edits:
  - it wrote `decreases state^.open_elements.inner.len` in the header;
  - it gave `pop_open` the postcondition
    `state^.open_elements.inner.len + 1_u64 == entry(state)^.open_elements.inner.len`;
  - it narrowed `count_p_removed`'s row from `writes(state)` to
    `writes(state.open_p_count)`.

  The rank then fell, and the module was accepted.
  - Without the narrowed row, `pop_open`'s postcondition was unproved: the
    callee's whole-`state` write killed the length.
  - A measure through a reference is a rank atom. A scalar field through a
    reference is not.
- **C. `url/ipv6.wf` `@quad`.** Changing its two `pos +wrap 1_u64` steps to
  `+` did not make the rank fall. When no octet has been seen, the loop
  advances `pos` only inside the nested `@octet_digits` loop. That loop
  carries no invariant relating its exit `pos` to its entry value, so the
  outer loop needs one written. The failure is an authoring cost, not a
  derivation gap.

## Reading

- The rule accepts 62% (142 of 229) of Snowghost's complete-module loops
  unchanged.
- Most of the rest need writer work the language already has:
  - a written rank;
  - a helper postcondition stating the advance;
  - a narrower effect row;
  - `>=` for `==`.
- Four gaps in the checker or the language remain:
  - a scalar field through a reference cannot be a rank atom (7 loops);
  - a `+wrap` step is not exact under a difference bound (probe A, and 2
    loops directly);
  - a `Bool` exit test ends the leading statements (2 loops begin with
    one);
  - link walks and the reprocess loop need the ranked arena and table
    ranks (3 loops).
