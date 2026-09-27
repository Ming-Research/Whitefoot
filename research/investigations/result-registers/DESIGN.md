# Small results in return registers

## Question and scope

The internal callable ABI (`compiler/src/backend/abi.rs`) returns every stored
aggregate, meaning every struct, non-tag-only enum, inline array, constant
capacity window and opaque value, through a destination pointer:
`define void @wf_f(ptr %wf.result, ...)`. A call that LLVM does not inline
therefore moves even a two-word result through memory. The callee stores
each field through `%wf.result` and the caller reloads it.

On main at `efe40194a`, the hash-map lookup `find` of
`tests/programs/containers/hashmap.wf`, returning `Option<u64>`, is not
inlined into its hot caller `map_trace`, and every call stores and reloads
its result. Rust returns `Option<u32>` in registers.

This investigation selects which stored-aggregate results cross the call
boundary as first-class LLVM values and how every call route carries them.
Language acceptance, the proof system and value representation inside a body
are out of scope. compiler/storage-representation keeps its storage plans.
compiler/backend-facts keeps its parameter facts.

## Candidates

- **Destination for every aggregate**: the baseline.
- **Every aggregate by value**: return each stored aggregate as its LLVM
  first-class type.
- **Byte bound**: by value when the selected-target size is at most 16 bytes,
  the C and Rust register-return size on x86-64 and AArch64.
- **Register bound**: by value when every scalar leaf of the LLVM
  representation receives its own return register under LLVM's return
  convention on every admitted target. On x86-64 that allows three
  integer-class leaves (RAX, RDX, RCX) and two floating leaves (XMM0, XMM1).
  AArch64 allows eight of each.
- **Both bounds**: the register bound and the byte bound together.
- **`fastcc`**: keep the destination and select LLVM's fast calling
  convention.
- **Packed coercion**: coerce a result of at most two words to integer
  words, the way C and Rust lower their register returns.

## Selection criterion

This criterion was written before any probe or measurement below:

1. **Correctness.** Every admitted call route keeps its behavior: direct and
   generic calls, self-tail transfers, loop-split chunks and splitters,
   handed-out calls and their lane thunks, recursion-budget entries and
   variants, sequential clones, the build launcher and linked definitions.
   The canonical `make check` passes.
2. **No silent demotion.** When LLVM cannot return a value in registers, it
   silently passes a hidden result pointer. Each shape a candidate admits
   must return in registers on every target `TargetLayout::for_triple`
   admits, compiled with `llc -O2`. The boundary shapes are three and four
   integer leaves, two and three floating leaves, a four-leaf 16-byte
   struct, a 16-byte array of 16 bytes, a three-leaf 24-byte struct, `i1`
   leaves and a zero-leaf aggregate. A candidate that admits a demoted shape
   fails this criterion.
3. **Benefit on the surviving calls.** In the optimized x86-64 code
   (`whitefootc --emit-llvm`, then `clang -O2`), the hash-map `find` and
   `insert` calls that survive in `map_trace`, and an out-of-line
   `add_checked(a: u32, b: u32) -> Result<u32, Overflow>`, lose their
   destination store and reload. Their callee and call site then use fewer
   memory operations. Across the maintained programs, no stored-aggregate
   result that the candidate admits still reaches its caller through a
   destination.
4. **No regression.** The number of surviving destination calls does not
   increase. A find-heavy loop built from `hashmap.wf` does not slow beyond
   run-to-run variation. Both binaries are timed alternately on the same
   host, and the medians are compared.
5. **Choice among passing candidates.** Prefer the candidate that returns in
   registers every result the target's return registers can carry, because
   each such result saves its memory round trip. A candidate that changes
   the ABI of a linked definition must adapt that definition inside the one
   callable ABI of the compiler root decision. When candidates tie, prefer
   the least emitter and runtime machinery.

## Demotion probe

Criterion 2 was checked with one `llc -O2` module per admitted triple
(LLVM 18.1.3). Each callee built its result from its arguments with
`insertvalue` and returned it by value, and its assembly was read. The table
shows one of three outcomes:

- **registers**: every leaf returns in a general-purpose, SSE or AArch64
  register.
- **pointer N**: LLVM demoted the return to a hidden result pointer and made
  N stores through it, which is exactly the cost `define void
  (ptr %wf.result, ...)` already has.
- **x87**: the leaves beyond the second floating one go through the x87
  register ST0. The callee stores the value to its stack and reloads it with
  `fld`.

| Shape | LLVM type | x86-64 Linux | x86-64 Windows | x86-64 Darwin | AArch64 Linux | AArch64 Darwin |
|---|---|---|---|---|---|---|
| `Option<u64>` | `{ i32, i64 }` | registers | registers | registers | registers | registers |
| `Result<u32, Overflow>` | `{ i32, i32, i1 }` | registers | registers | registers | registers | registers |
| three integers | `{ i32, i32, i32 }` | registers | registers | registers | registers | registers |
| four integers, 16 bytes | `{ i32, i32, i32, i32 }` | pointer 4 | pointer 4 | pointer 4 | registers | registers |
| 16 bytes | `[16 x i8]` | pointer 16 | pointer 16 | pointer 16 | pointer 16 | pointer 9 |
| three words, 24 bytes | `{ i64, i64, i64 }` | registers | registers | registers | registers | registers |
| `Result<u64, Utf8Error>`, 24 bytes | `{ i32, i64, i1 }` | registers | registers | registers | registers | registers |
| `(u64, Option<u64>)`, 24 bytes | `{ i64, { i32, i64 } }` | registers | registers | registers | registers | registers |
| three bits | `{ i1, i1, i1 }` | registers | registers | registers | registers | registers |
| `Slots<u8, 2>` | `{ i64, [2 x i8] }` | registers | registers | registers | registers | registers |
| two doubles | `{ double, double }` | registers | registers | registers | registers | registers |
| three doubles | `{ double, double, double }` | x87 | x87 | x87 | registers | registers |
| three floats, 12 bytes | `{ float, float, float }` | x87 | x87 | x87 | registers | registers |
| three words and two doubles | `{ i64, i64, i64, double, double }` | registers | registers | registers | registers | registers |
| three words and three doubles | `{ i64, i64, i64, double, double, double }` | x87 | x87 | x87 | registers | registers |
| three words, one of them wide | `{ i128, i64 }` | registers | registers | registers | registers | registers |
| opaque | `{ i128, i128 }` | pointer 4 | pointer 4 | pointer 4 | registers | registers |
| empty | `{}`, `[0 x i8]`, `{ i64, [0 x i8] }` | registers | registers | registers | registers | registers |

The x87 route is more than a cost. `fld` from a 32-bit or 64-bit memory
operand converts a signaling NaN to a quiet one. A caller of
`{ double, double, double }` received the signaling NaN `7ff0000000000001` in
all three fields, and the first two came back unchanged in XMM0 and XMM1,
while the third came back as `7ff8000000000001`. `{ float, float, float }`
likewise turned `7f800001` into `7fc00001` in its third field. A float value
returned that way is not bit-exact, so a by-value return of a third floating
leaf would break correctness, not only performance.

A count of memory operands does not separate these outcomes. The x87 route
also writes and reads memory, but through the stack and not through a result
pointer. The table therefore classifies each callee by where its stores go
and by whether it loads ST0. Criterion 2 names only the hidden pointer. The
x87 route fails criterion 1, because a returned value must be bit-exact, and
it keeps the memory round trip that criterion 3 exists to remove, so it
excludes a candidate just as a hidden pointer does.

The x86-64 return convention gives each scalar leaf its own register,
without packing small leaves together: three integer-class words (RAX, RDX,
RCX, with an `i128` taking two) and two floating leaves (XMM0, XMM1). The
same budget applies on all three x86-64 triples. AArch64 has eight of each,
so every shape that fits x86-64 also fits AArch64.

- **Every aggregate by value** fails. The four-leaf, sixteen-byte and opaque
  shapes are demoted to a hidden pointer, and larger results would be too.
  The three-double shapes lose bit-exact floats through x87.
- **Byte bound** fails. `{ i32, i32, i32, i32 }` and `[16 x i8]` are 16 bytes
  and are demoted on x86-64, and the 12-byte `{ float, float, float }` goes
  through x87. It also excludes the 24-byte three-word shapes, which return in
  registers.
- **Register bound** passes by construction, and every boundary shape
  confirms it.
- **Both bounds** passes, but it sends every 24-byte three-leaf result
  through a destination although it returns in registers.
- **`fastcc`** is not a candidate for criterion 3. The destination is an IR
  parameter that no calling convention removes. Linked C bodies also share
  the ABI, so their calling convention cannot change alone.
- **Packed coercion** was not built. It needs per-target coercion rules and
  packing code at each boundary. Criterion 5 prefers the smaller machinery
  when both reach the same results, and every surviving result of at most two
  words in the maintained programs fits one register per leaf.

## Lowering

The body of a definition whose result returns in registers is its unchanged
destination-form body, emitted under an internal symbol:
`define internal void @<symbol>.body(ptr %wf.result, ...)`. The public symbol
names a small entry with the source signature. The entry allocates one
target-qualified frame slot, calls the body with that slot as `%wf.result`,
loads the slot and returns the value. The storage plan, the frame's target
qualification, construction and every return of the body are therefore those
of the destination form. The caller stores the returned value into the storage
its plan selected. Parameters, their facts and the storage plans do not
change. No source function can take a body's symbol. A function outside the
root module is spelled `path.name`, so it would have to be a function `body`
in a child module named after the entry's function, and [MOD-3] rejects a
declaration that shares a registered module's qualified name. An instance
suffix or a compiler-owned `wf__` symbol holds spellings no identifier has
[FORM-3].

The body is internal, its entry is its only caller, and it is never marked
always-inline. LLVM's inliner visits a callee before its callers, so the host
optimizes each body alone, as it optimizes the destination-form definition on
main. Only then does it inline the body into its entry, as the last call to a
local function. After `clang -O2`, no body survives as a separate definition in
any of the 80 modules of the [corpus](#corpus). The form depends on that pass
order. The always-inliner runs before the per-function optimization, so an
always-inline body would be merged into its entry unoptimized, like the merged
returns below. A host whose inliner visited the entry first would do the same.
`compiler/src/backend/tests/result_abi.rs` checks that no emitted definition is
always-inline. Only the structural results below observe the host's order.

Four value forms were compared on the same emitted modules. Each form was
produced by a scratch rewrite of the returns and compiled with `clang -O2`
18.1.3 on x86-64 Linux:

- **Merged returns**, this change's first revision: the callee constructs the
  result in its own frame slot, and every return branches to one block that
  loads the slot and returns the value.
- **Per-exit load**, the first prototype: each return loads the slot and
  returns it.
- **Per-exit leaves**: each return loads the slot's leaves and rebuilds the
  value.
- **Entry over body**, the selected form.

| Lowering | `find` instructions | `map_trace` instructions | `find` inline cost at `map_trace` | Inner ASCII loop in `records` | `records` W=1 / W=2 / W=4 | Hash-map trace |
|---|---:|---:|---|---|---|---:|
| main, destination | 144 | 285 | 595, not inlined | yes | 23.25 / 10.52 / 5.35 ms | 1.000 |
| merged returns | 142 | 1,593 | 105, inlined | no | 20.49 / 11.56 / 6.42 ms | 0.601 |
| per-exit load | 250 | 275 | 1,020, not inlined | no | 21.38 / 10.68 / 6.04 ms | 1.066 |
| per-exit leaves | 250 | 275 | 1,020, not inlined | no | 21.03 / 10.46 / 5.86 ms | 1.077 |
| entry over body | 139 | 275 | 580, not inlined | yes | 20.37 / 10.18 / 5.28 ms | 0.956 |

Instruction counts include the return, and the inline threshold is 225.
`add_checked` has eight instructions in every value form and nine on main. The
`records` times are medians of five interleaved rounds on the local Intel host,
each form at its own linked placement. On that host the
[placement controls](#hosted-compute-regression) move either arm by up to 18%,
so these times do not rank the forms. The structural columns do. The hash-map
trace column is the paired median ratio to main over 11 rounds, measured as in
[Timing](#timing). The two per-exit forms link to identical images for both
programs, so the differences between their rows are run-to-run variation.

**Why the value forms lose the loop.** LLVM's SimplifyCFG gathers the returns
of a function into one block. The merged form emits that block itself, and
SimplifyCFG builds it for the per-exit forms (its `common.ret` block). The exit
values meet in a phi there, and SimplifyCFG folds `validate_record`'s final exit
test, `remaining == 0`, into a `select` between them. In the destination form,
each exit stores its own fields instead. The late JumpThreading pass then
threads the ASCII path's latch through that test, which the path already
decides. The duplicated latch becomes an inner loop over consecutive ASCII
bytes ([Hosted compute regression](#hosted-compute-regression)). In the
per-exit forms, the phi also joins whole aggregates, which SROA does not split
after inlining. There `find` grows to 250 instructions, and its inlined
`view_entry` tag is materialized and then tested again.

**Why the merged form's trace was faster.** On main and in the entry-over-body
form, LLVM fully unrolls `find`'s eight-slot probe loop while it optimizes
`find` alone, before it considers `find` at `map_trace`. The unrolled `find`
then costs 580 there (595 on main) against the threshold of 225, and all seven
calls stay out of line. In the merged form, that loop was not unrolled at this
point. `find` cost 105, `remove` 140, and LLVM inlined all seven `find` calls
and three `remove` calls into `map_trace`, then peeled each inlined loop by
seven iterations. Both forms return in registers. What separates their traces,
0.601 and 0.956 of main's time, is that inlining. The boundary alone gives the
entry-over-body form's gain of 4% to 7% ([Timing](#timing)).
[`docs/todo.md`](../../../docs/todo.md) records the inlining as a separate
opportunity.

Every call route uses the value form it already had for scalar results:

- direct and generic calls store the returned value into the result's
  storage;
- a handed-out call's thunk stores it into the lane frame's result field;
- the refused edge calls by value, and both edges join in one phi that the
  caller stores;
- a loop split's chunk or splitter call is saved by the value-definition
  bridge;
- a recursion-budget entry, its variant and a sequential clone each have their
  own entry and body. An exhausted variant's body stores the value its
  fallback returns into its own destination;
- a self-tail transfer is a jump inside one activation
  (compiler/self-tail-lowering), so it stays inside the body and keeps the
  body's one destination;
- the build launcher's `ExitStatus` is opaque, four words, and keeps its
  destination.

A linked definition keeps the callable ABI of the compiler root decision.
`host_utf8_len`, `Result<u64, Utf8Error>` or `{ i32, i64, i1 }`, is the one
linked result that fits. Its C body became `wf__body_host_utf8_len`, and an
LLVM definition in `compiler/src/backend/ordinary_values.ll` returns the
first-class value. A test checks every linked register result against its
declaration.

## Result on the surviving calls

Criterion 3 compared the emitted code of both compilers
(`whitefootc --emit-llvm`, then `clang -O2` 18.1.3, x86-64 Linux). The
baseline was main at `efe40194a`.

`add_checked(a: u32, b: u32) -> Result<u32, Overflow>`:

```asm
; baseline                        ; candidate
xorl   %eax, %eax                 xorl   %ecx, %ecx
xorl   %ecx, %ecx                 xorl   %eax, %eax
addl   %edx, %esi                 addl   %esi, %edi
cmovbl %eax, %esi                 cmovbl %ecx, %edi
setb   %cl                        setb   %al
movl   %esi, 4(%rdi)              movl   %edi, %edx
movl   %ecx, (%rdi)               xorl   %ecx, %ecx
movb   $0, 8(%rdi)                retq
retq
```

Counting the return, that is nine instructions with three stores before the
change, and eight with none after. Rust 1.98.1 compiles `a.checked_add(b)`
returning `Option<u32>` to four instructions. The remaining difference is the
representation, not the boundary. The enum carries a separate one-bit
`Overflow` payload, and it zeroes the inactive `Ok` value.

The `find` of `owning-map.wf` stays out of line in both builds. Its `Some`
return path and one of its 14 call sites:

```asm
; baseline                        ; candidate
movq   $0, (%rdi)                 movq   (%rax), %rdx
movl   $1, (%rdi)                 movl   $1, %eax
movq   %rax, 8(%rdi)              retq
retq

leaq   1400(%rsp), %rdi           leaq   624(%rsp), %rdi
leaq   624(%rsp), %rsi            movq   1040(%rsp), %rsi
movq   1040(%rsp), %rdx           callq  wf_find@PLT
callq  wf_find@PLT                movb   $9, %cl
movb   $9, %cl                    testl  %eax, %eax
movl   1400(%rsp), %eax
testl  %eax, %eax
```

Per function:

| Function | Instructions before → after | Stores before → after |
|---|---|---|
| `hashmap.wf` `find` | 144 → 139 | 4 → 0 |
| `hashmap.wf` `insert` | 193 → 190 | 20 → 12 |
| `hashmap.wf` `remove` | 156 → 151 | 12 → 6 |
| `hashmap.wf` `view_entry` | 20 → 18 | 6 → 0 |
| `hashmap.wf` `map_trace` | 285 → 275 | 50 → 54, loads 56 → 21 |
| `owning-map.wf` `find` | 109 → 106 | 4 → 0 |
| `owning-map.wf` `exercise` | 1709 → 1681 | 416 → 416, loads 725 → 700 |

The count script counts surviving calls that pass a `%wf.result` destination,
bucketed by the size of the destination's whole alloca type after
optimization:

| Program | `<=16` bytes | `>16` bytes |
|---|---|---|
| `hashmap.wf` | 12 → 0 | 3 → 0 |
| `owning-map.wf` | 14 → 0 | 19 → 19 |

In `hashmap.wf`, `map_trace` keeps its 7 `find`, 5 `insert` and 3 `remove`
calls in both builds, and after the change each call returns in registers.
The 24-byte `remove` result `(u64, Option<u64>)` and `view_entry` result
`SlotView` are the corpus results that separate the register bound from the
both-bounds candidate. The 19 destinations left in `owning-map.wf` carry
208- and 216-byte results and the launcher's 32-byte `ExitStatus`.

## Corpus

Every single-file program under `tests/programs` (73 that compile alone) and
the seven library container bundles of `compiler/tests/programs/containers.rs`
were compiled with both compilers. The size buckets count destinations in
local slots; the other destinations are not local slots:

| Set | Destination calls `<=16` | Destination calls `>16` | Other destinations | `.text` bytes |
|---|---:|---:|---:|---:|
| 73 single-file programs | 33 → 0 | 86 → 73 | 17 → 13 | 190,589 → 188,621 |
| 7 container bundles | 0 → 0 | 3 → 3 | 0 → 0 | 225,475 → 222,259 |

No destination call of at most 16 bytes survives. Of the sized destinations
that remain, 47 are the launcher's 32-byte `ExitStatus` calls, 37 to `wf_main`
(three of them in the bundles) and 10 to `wf_exercise` where LLVM inlined
`main` into the launcher. The other 29 carry results of 32 to 2,392 bytes.
Eight of them are four-word results, which AArch64's budget would return in
registers: four `{ i32, { ptr, i64 }, i1 }` results of `parse_expression` in
`prefix_expression.wf`, and four results with four words in `option_slots.wf`
(two), `owned_link_cursors.wf` and `containers/ordered.wf`. `.text` shrank by
1.0% across the single-file programs and by 1.4% across the bundles.
`hashmap.wf` shrank by 336 bytes (8.3%). Only `boxed-helper-gap.wf`
(+16 bytes), `fixed_run_library.wf` (+256 bytes) and `wfgrep.wf` (+48 bytes)
grew.

## Timing

Criterion 4 used two scratch variants of maintained programs. The first is
`hashmap.wf` with `main` calling `map_trace(seed: 5, repetitions: 20000000)`.
The second is `owning-map.wf` with `main` looping over 2,000,000 seeds. In both,
the timed calls stay out of line in both builds. Each image was built by its
own compiler and run 11 times, alternating the order and pinned with
`taskset -c 3`. The host was a 4-vCPU x86-64 Linux container, with the
repository's host-wide verification lock held during the runs.

| Program | Baseline median | Candidate median | Paired ratio median (range) |
|---|---:|---:|---:|
| hash-map trace | 1.2937 s | 1.2200 s | 0.927 (0.891–0.999) |
| owning-map exercise | 1.4836 s | 1.4727 s | 0.992 (0.923–1.129) |

Earlier runs of the hash-map comparison under the same protocol gave paired
medians of 0.956 and 0.946 for the scratch prototype of the lowering and 0.962
for the implemented compiler.

The same variants were also scaled down to 200,000 repetitions and 20,000
seeds and run once each under `valgrind --tool=cachegrind`, which counts
executed instructions deterministically:

| Program | Instructions | Data references (reads + writes) |
|---|---|---|
| hash-map trace | 163,982,550 → 158,382,550 (−3.4%) | 59,117,850 → 43,917,850 (−25.7%) |
| owning-map exercise | 149,624,696 → 148,324,696 (−0.87%) | 67,658,952 → 66,438,952 (−1.8%) |

Both differences come from the boundary alone. `map_trace` keeps its 15 calls
and `exercise` its 14 `find` calls, and each call saves the store and reload
of its result. The hash-map trace gains 4% to 7% in wall time. The owning-map
gain shows in the counts and stays within run-to-run variation in wall time.

## Hosted compute regression

The criterion above does not name the maintained paired comparison,
`.github/workflows/compute-regression.yml`. It runs for every pushed commit.
The wall ratio is baseline over candidate, so a value below 1 means the
candidate is slower. `lower` counts the pairs in which the baseline was
faster.

**Merged returns.** The first revision's runs, for the commits up to
`0074c34a8`, all emit identical code, the later ones adding only this record.
Every run failed for one kernel, `records`, and passed the other four:

| Commit, run and host | W=1 | W=2 | W=4 |
|---|---:|---:|---:|
| `a15347c25`, [36087145857](https://github.com/mbbill/Whitefoot/actions/runs/36087145857), AMD EPYC 9V74 | 0.932 (4/5) | 0.822 (5/5) | 0.810 (5/5) |
| `d863e8e51`, [36089782067](https://github.com/mbbill/Whitefoot/actions/runs/36089782067), AMD EPYC 9V74 | 0.931 (5/5) | 0.832 (5/5) | 0.810 (5/5) |
| `f8d102ee8`, [36092739152](https://github.com/mbbill/Whitefoot/actions/runs/36092739152), AMD EPYC 7763 | 1.153 (0/5) | 0.779 (5/5) | 0.700 (5/5) |
| `968280509`, [36094040267](https://github.com/mbbill/Whitefoot/actions/runs/36094040267), AMD EPYC 9V74 | 0.928 (4/5) | 0.824 (5/5) | 0.820 (5/5) |
| `c192ca90e`, [36094502418](https://github.com/mbbill/Whitefoot/actions/runs/36094502418), AMD EPYC 7763 | 1.156 (0/5) | 0.796 (5/5) | 0.677 (5/5) |
| `080187c6f`, [36095043192](https://github.com/mbbill/Whitefoot/actions/runs/36095043192), AMD EPYC 7763 | 1.164 (0/5) | 0.781 (5/5) | 0.670 (5/5) |
| `0074c34a8`, [36095752626](https://github.com/mbbill/Whitefoot/actions/runs/36095752626), AMD EPYC 7763 | 1.163 (0/5) | 0.785 (5/5) | 0.683 (5/5) |

Each host had two cores of two threads each, and every run's `records.o`
objects are byte-identical. The identical-image control passed in every run.
The only other adverse line was a single-width `fir` W=4 suspect in the
second run (0.958), and that line passed in the other runs. No run was
repeated, and no threshold, fixture or instrument was changed. The W=1 row
follows the host class: 0.93 on the EPYC 9V74 and 1.15 to 1.16 on the EPYC
7763. W=2 and W=4 are adverse on both classes.

**What changed in the measured code.** Of the five kernels, only records'
emitted module differs, apart from an unused declaration of
`wf_host_utf8_len`. `validate_record` returns its two-leaf
`RecordCheck { valid: Bool, scalars: u64 }` in registers, and
`record_summary` stores the returned value into its slot. The `[3 x i64]`
constructor `wf_array_filled$instance$36` also returns in registers. Its only
callers are the fixture's two `main` bodies, which the timed entry
`wf_bench_records` does not reach. In the second run's artifact, `records.ll`
and `records.o` of both arms are byte-identical to local builds with clang
18.1.3. The linked images disassemble identically to the local ones, with
every symbol at the same address. On a 4-vCPU Intel
Xeon (2.80 GHz) Linux host, the unchanged `tests/performance/compare.sh` also
failed on records alone: 1.145 (0/5) at W=1, 0.860 (5/5) at W=2 and 0.816
(5/5) at W=4.

**Two copies of the loop.** The bound host adapter calls the sequential clone
`wf__par_seq_summarize_records` when no worker pool is active, which is W=1.
Otherwise it calls `wf_summarize_records`, whose leaf loop is inlined into
`wf__par_split_37`. Each function holds its own inlined copy of
`validate_record`'s loop at its own address, so W=1 and the wider rows time
different code placements.

**The loop's structure.** In the destination form, `validate_record` keeps
two exit blocks after its loop, one storing the invalid result and one the
valid result. The late JumpThreading pass threads the ASCII path's latch
through the exit test, `remaining == 0`, which is known on that path. The
duplicated latch then becomes an inner loop over consecutive ASCII bytes. In
the merged form, every return joins one block, and SimplifyCFG turns that
exit test into a `select`. Nothing is left to thread, and the loop keeps one
level.

Clang 18.1.3 does the same to a line-for-line C transcription at `-O2`.
Returning the two-field struct by value gives one loop. Writing it through a
result pointer gives the inner ASCII loop. The pointer form replaces each
`return (RecordCheck){...};` below with a store through the pointer and a
bare `return;`. A shorter hand-written validator in the completion review did
not show the inner loop in either form, so the effect depends on this loop's
shape.

```c
typedef struct { bool valid; uint64_t scalars; } RecordCheck;

RecordCheck validate_value(const uint8_t *input, uint64_t first, uint64_t end) {
    uint64_t scalars = 0;
    uint8_t remaining = 0, lower = 128, upper = 191;
    for (uint64_t i = first; i < end; ++i) {
        uint8_t byte = input[i];
        if (remaining == 0) {
            scalars += 1;
            if (byte <= 127) {
            } else {
                if (byte < 194) return (RecordCheck){false, 0};
                if (byte <= 223) remaining = 1;
                else if (byte <= 239) {
                    remaining = 2;
                    if (byte == 224) lower = 160;
                    if (byte == 237) upper = 159;
                } else if (byte <= 244) {
                    remaining = 3;
                    if (byte == 240) lower = 144;
                    if (byte == 244) upper = 143;
                } else return (RecordCheck){false, 0};
            }
        } else {
            if (byte < lower) return (RecordCheck){false, 0};
            if (byte > upper) return (RecordCheck){false, 0};
            remaining -= 1;
            lower = 128;
            upper = 191;
        }
    }
    if (remaining != 0) return (RecordCheck){false, 0};
    return (RecordCheck){true, scalars};
}
```

Cachegrind counted the W=1 kernel function over six calls:

| Input | Instructions | Conditional branches |
|---|---|---|
| hosted fixture (`shape=unicode`) | 1,883.9M → 1,861.5M (−1.2%) | 401.3M → 401.9M (+0.15%) |
| ASCII records, 131,072 of up to 255 bytes (scratch driver) | 722.3M → 1,020.8M (+41%) | 203.7M → 303.2M (+49%) |

In the hosted fixture almost every byte belongs to a four-byte sequence. On
that input the merged form executes fewer instructions and about as many
conditional branches. On ASCII records it executes about three more
instructions and one more conditional branch per byte.

**Placement.** Each scratch timing below ran five rounds of interleaved
processes, seven for the first item. Each process reports the median of five
calls after a warmup, and each figure is the median over rounds.

- Moving only the merged form's runtime objects to the baseline's addresses
  leaves records where it was. The tool was a 592-byte never-called function
  appended to the module. The merged form measured 11.54 ms at W=2 and
  6.13 ms at W=4, against the baseline's 10.08 ms and 5.31 ms.
- A never-called function placed before `wf__par_split_37` shifts both loop
  copies, and everything after them, by 0, 16, 32 or 48 bytes. No executed
  instruction changes:

  | Width | Baseline at +0 / +16 / +32 / +48 | Merged form at +0 / +16 / +32 / +48 |
  |---|---|---|
  | W=1 | 22.85 / 20.24 / 20.47 / 20.20 ms | 20.21 / 21.26 / 23.21 / 21.63 ms |
  | W=2 | 10.35 / 10.68 / 10.27 / 11.65 ms | 11.66 / 10.98 / 10.49 / 10.54 ms |
  | W=4 | 5.17 / 5.48 / 5.50 / 6.08 ms | 6.21 / 5.88 / 5.30 / 5.50 ms |

- With both arms compiled with `-falign-loops=64`, the baseline and the merged
  form measured 21.16 and 21.84 ms at W=1, 11.14 and 10.58 ms at W=2, and 5.36
  and 5.33 ms at W=4.
- ASCII records at the same four placements, W=1: baseline 11.69 / 9.46 /
  9.39 / 9.65 ms, merged form 11.98 / 9.57 / 9.71 / 9.66 ms.

**Reading.** On the Intel host, a shift of at most 48 bytes that changes no
instruction moves either arm by up to 15% at W=1 and up to 18% at W=2 and
W=4. It reverses the arms' order at W=2 and W=4. The failing widths follow
the kernel's placement and not the runtime's, and on the hosted fixture the
merged form's kernel does less work. On this evidence the hosted verdict is a
reading of the linked placement, not of added work. The structural change is
still real. The lost ASCII loop costs about 41% more kernel instructions on
ASCII records, and between 0.1% and 3.4% of W=1 time at the four placements.
The AMD hosts had no placement control, so the verdict is not attributed on
the machines that produced it. Records' placement sensitivity was already
recorded in the
[compute-runtime alignment comparison](../compute-runtime/RESULTS.md#2026-09-11--local-host-linux-x86_64-4-logical-cpus-loop-and-function-alignment-on-the-whitefoot-side-ab)
and in `docs/todo.md`'s formal compute comparison entry.

**Entry over body.** The selected form emits records' `validate_record` body
and the `[3 x i64]` constructor's body byte-identical to their definitions on
main, apart from the symbol and the internal linkage. In the optimized code,
`validate_record` and both loop copies keep main's inner ASCII loop. In the
linked images, `wf__par_split_37` is instruction-identical to main's at the
same address, and the sequential clone is instruction-identical to main's,
16 bytes earlier. On the local Intel host, the unchanged
`tests/performance/compare.sh` passed with records at 1.156 (0/5) at W=1,
0.979 (3/5) at W=2 and 1.008 (1/5) at W=4. Its three single-width suspects,
`mandelbrot` W=4, `fir` W=2 and `stencil` W=4, are in kernels whose emitted
modules differ only in the unused `wf_host_utf8_len` declaration.

The maintained hosted comparison of `9487fc24a`, the first pushed revision with
this form, ran on an AMD EPYC 7763 against main at `6facd86b8`
([36102882790](https://github.com/mbbill/Whitefoot/actions/runs/36102882790)).
It passed every kernel with no suspect. Records read 1.073 (0/5) at W=1,
1.015 (0/5) at W=2 and 0.996 (3/5) at W=4, and the identical-image control
passed. In its artifact, both loop copies and `wf_bench_records` are
instruction-identical between the arms, `wf__par_split_37` at the same
address, and records' bodies are byte-identical to main's definitions.

The run of `ac8ca1d2f`
([36104458730](https://github.com/mbbill/Whitefoot/actions/runs/36104458730))
compared byte-identical records images on an AMD EPYC 9V74. It passed with
one single-width suspect: records read 0.855 (5/5) at W=1, 1.001 (2/5) at W=2
and 1.006 (1/5) at W=4, and the identical-image control passed. W=1 times the
sequential clone, which is instruction-identical to main's and 16 bytes
earlier. The same images therefore read 1.073 at W=1 on the EPYC 7763 and
0.855 on the EPYC 9V74. As with the merged form, the W=1 row follows the host
class, and the hosted runners have no placement control to attribute it.

## Selection

The register bound is selected, with the entry-over-body lowering:

- It is the only candidate that passes criterion 2 while returning in
  registers every shape the probe keeps in registers.
- It meets criterion 3: every admitted result that survives as a call now
  returns in registers, and no destination call of at most 16 bytes remains in
  the corpus.
- It meets criterion 4. No destination call was added, the find-heavy loop is
  4% to 7% faster, and the out-of-line `find` loop is unchanged within its
  variation. The maintained paired comparison, which the criterion does not
  name, failed on `records` with the merged returns. The entry-over-body form
  keeps records' loop structure and passed that comparison in both of its
  hosted runs, one of them with a W=1 suspect on the EPYC 9V74
  ([Hosted compute regression](#hosted-compute-regression)).
- Under criterion 5 it is preferred over both bounds, because the 24-byte
  three-leaf results return in registers and the corpus has them. The cost is
  one linked LLVM definition inside the one callable ABI, and one entry beside
  each register-returned body.
- Among the lowerings, only the entry over the destination-form body keeps
  every loop structure that the destination form gives the host
  ([Lowering](#lowering)).

## Limits

- Only x86-64 Linux was timed. The AArch64 and Windows effects are inferred
  from the probe's register assignment, not measured.
- The records placement controls ran on the local Intel host only. The hosted
  AMD verdicts have no placement control of their own.
- The lowering depends on the host's pass order: the body must be optimized
  alone before it is inlined into its entry. `result_abi.rs` checks the
  emitted attributes, not the host's order. A host change is observed only by
  the structural results above, which no maintained test repeats.
- A value-returning body can lose a loop-exit threading that the destination
  form allows, as records' ASCII loop showed. The selected form keeps every
  body in its destination form, but only records and the hash map were
  examined.
- The merged form's faster hash-map trace came from LLVM inlining `find` and
  `remove`, which happens neither on main nor with the selected form. That
  opportunity is recorded in `docs/todo.md`, unmeasured on the selected form.
- The timing variants are scratch programs derived from maintained sources.
  No paired performance workload was added.
- Results that exceed the budget still pass through memory: every opaque
  value, including `ExitStatus`, and every result with four or more integer
  words. Packing small leaves into shared registers, or AArch64's larger
  budget, would carry some of them. The corpus keeps eight such calls with
  four-word results besides the launcher's, none on a measured path.
