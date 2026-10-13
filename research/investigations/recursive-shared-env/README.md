# A recursive group's shared environment record

This record was written before any measurement of the proposal. The design
it measures is the proposed node compiler/recursive-environment; the
implementation is `compiler/src/backend/emitter/call_entries.rs`.

## Question

Snowghost's selector matcher is a chain of mutually recursive functions:
step calls compound, compound calls test, and test's negation arm calls step
again. Each passes the borrowed `rules`, `classes`, `tags` and the range
`table` on unchanged, together with values that vary per call. With the
table, step takes 9 argument words and test 8, against the 6 integer
argument registers of x86-64 System V. The witness
[`witness-argument-words.wf`](https://github.com/Ming-Research/Snowghost-wf/blob/637d568/research/investigations/style/witness-argument-words.wf),
built by `research/investigations/style/scan-param.sh witness` at the same
Snowghost-wf commit, holds two copies of the chain, one with the table and
one without, and checks that both count the same matches. Under callgrind on
CI with the base compiler, step costs 59.34 instructions per call with the
table and 55.05 without, test 24.55 and 20.37, and main's loop 24 and 21.
The disassembly attributes the difference to about 5.3 stack-passed
arguments, 2 instructions preparing arguments, and about 6.6 spills and
reloads around inner calls and from register pressure.

The question: does passing the borrows every recursive call forwards
unchanged in one record, built once where the chain is entered and reached
through one pointer inside it, remove the per-call cost of the extra words,
without making the chain that fits its registers slower?

## Comparison

- Workload: the witness above, unchanged.
- Sides: the base compiler (Whitefoot main at `9bc6bb7ed`) and this branch,
  each building the witness with the same host clang and options.
- Machine: the same CI host for both sides, interleaved, with a second build
  of the base as a twin control for noise.
- Measure: callgrind's self instructions per call of `step_with`,
  `step_without`, `test_with` and `test_without`, and of main's two loops per
  call of step.
- Reported: per side, the with-table minus without-table gap for step and
  for test, and the without-table cost against the base's.

The profitability rule applies on x86-64 to both copies: the without-table
step passes 7 words, more than 6, so it also takes the record, with three
slots. On AArch64, with eight registers, only the with-table chain does.

## Result that decides

- Accepted when, on this branch, the with-table and without-table copies
  differ by at most 1 instruction per call for step and for test, and each
  without-table function costs no more per call than on the base.
- Rejected when a gap of 3 or more instructions per call remains for step
  or for test, or when a without-table function costs 2 or more
  instructions per call more than on the base.
- Anything between is inconclusive and is attributed (stack traffic, record
  loads, spills) before a decision.

## Risks the measurement must see

- A member whose branches each read a different borrowed input pays one
  load per slot it reads. The loads are emitted once in the entry block and
  rely on the host sinking them into the branch that uses them; an argument
  past the registers was already a load from the stack, so the record should
  cost no more there, but a member whose words all fit loses nothing to the
  stack and only gains loads. The profitability rule keeps such a group out
  only when every member fits.
- A pointer loaded from the record carries `nonnull` and `dereferenceable`
  but cannot carry `noalias`, which the parameter had. Only read-only
  borrows enter the record, so every write still goes through a `noalias`
  argument, but a load through a slot pointer is no longer known to survive
  a store to memory the function owns.

## Later downstream check

Snowghost's renderer has a function whose 36 arms each read a different
borrowed input. Its scan entry with the table against without it is today
0.9047 in Snowghost's timing ratio; after this change is adopted downstream
the target is at least 0.98. This check runs in Snowghost-wf with a released
compiler and is not part of this record's decision.

## First measurement

CI run 38334593158 measured the first implementation, which had no
`noinline` cut, against the base.

- The with-table and without-table copies cost the same per call: the gap
  is 0, inside the 1-instruction acceptance bound.
- The without-table chain costs 14.17 more instructions per top-level call
  than on the base, past the 2-instruction rejection bound. By the
  criterion above, this implementation is rejected.
- Attribution, from callgrind's annotation of the same run:
  - The record itself is a net gain of 1.22 per call. Slot loads add 1.97,
    and fewer argument moves save 2.19 and one fewer entry copy 1.00.
  - The host now inlines the internal step, with compound inside it, into
    the negation arm of the internal test. Test becomes non-leaf, so every
    call to it pushes and pops six callee-saved registers, adjusts the
    stack and keeps the record pointer in a callee-saved register: 16.56
    per call. Shared-epilogue jumps add 1.09.
  - The base keeps step a call, and test stays frameless.
  - No alias reloads were found, and no slot loads on paths that do not
    use them: the host already sinks the loads into the arms.

The follow-up keeps the cycle-closing calls of each component out of line
(compiler/recursive-environment). It is measured against the same
criterion, on the same witness, against the same base.

## Second measurement

CI run 38336250748 measured the version with the `noinline` cut on cycle-closing calls.

- The with-table and without-table copies again cost the same per call:
  the gap is 0.
- Step costs 50.78 instructions per call. On the base it cost 55.05
  without the table and 59.34 with it.
- The whole chain costs 76.81 per top-level call, against 77.96 for the
  base without the table.
- The without-table test costs 23.23 per call, against 20.37 on the base:
  2.86 more, past the 2-instruction rejection bound.
- Attribution, verified in the disassembly:
  - 2.00 is two register copies at the entry of every call of the internal
    test. Its form was `(env, simple, id, context)`, while its rarely taken
    call of step wants `(env, first, last, id, context)`, so the allocator
    moves id and context into step's positions for the whole function.
  - 0.88 is the slot loads in the Class and Tag arms, which the record
    inherently costs.

The follow-up gives the parameters that calls inside the group forward
unchanged the same position in every member's internal form
(compiler/recursive-environment). It is measured against the same
criterion.

## Status

The first two versions were measured; each crossed a rejection bound. The
aligned positions of forwarded parameters are implemented and not yet
measured.
