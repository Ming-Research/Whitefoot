# Whitefoot: what's different, and why

*For people who already write Rust, C, and C++, and for people who only prompt.*

Status: **DATED, NON-AUTHORITATIVE DESIGN SYNTHESIS.** This article combines
language ideas with measurements from the retired democ compiler and proposed
mechanisms. Its historical performance, architecture, and status claims are
not a description of the current compiler or a promise of global optimality.

For current objectives and tradeoffs, read the [constitution](constitution.md).
The conformance report owns implementation status and the
[active specification](../spec/kernel-spec.md) owns language semantics.
[Research ideas](ideas.md) collect open questions and links to their evidence.
Original measurements below retain their conditions; this essay adds no
approval or workflow requirements.

---

## The premise: languages are shaped by who writes them

Every mainstream language strikes a bargain with a human writer, and the currency is information. Ergonomics buys the right to leave facts unstated: don't make me declare types, don't make me name what my function touches, don't make me say which of these pointers can overlap, let me keep the patterns I know, give me an escape hatch for when I know better. Each concession makes the language nicer to write. Each one drops a fact the author knew and the source no longer carries.

The cost compounds down the pipeline. You have shipped code on most rungs of this ladder.

**Dynamic languages (JavaScript, Python).** The language defers nearly every fact to runtime, so the engine watches the program run to learn what the author knew at their desk. Hidden classes and inline caches rediscover object shapes; bytecode specialization rediscovers types. A runtime check guards each recovered fact, an assumption break invalidates it, polymorphism degrades it one direction only, and the scheme bills warmup time and code cache. V8 and HotSpot hold some of the best engineering in our field, and much of it exists to recover, at runtime and under guard, information the language told the author not to write down.

**Managed static languages (Java).** The types are static, but the language makes nearly everything a heap reference. "This object never leaves this function" was true in the author's head, and the language kept no way to record it, so the JIT runs escape analysis to try to get it back. Oracle's own documentation states the result: conditional scalar replacement and lock elision when the analysis succeeds, and no guaranteed stack allocation. The fact was free at authoring time. Recovered, it arrives partial, late, and revocable.

**C and C++.** Compiled and lean, with the aliasing bargain inverted: the compiler assumes any two pointers may overlap until heroic analysis proves otherwise. `restrict` exists, nothing checks it, a wrong one miscompiles in silence, and so people rarely write it. Decades of alias analysis exist to re-derive non-interference facts the author usually knew. Part II shows the bill on a single page of assembly, where the same reduction runs about twenty times slower for want of one aliasing fact.

**Rust, the rung that proves the thesis.** Make ownership a checked source fact, and one mechanism yields memory safety and the optimizer's no-aliasing facts at once. Rust is the natural experiment, with C and C++ as the control, that shows a type system can carry performance facts, not only correctness facts. Rust still bargains with human writers, so it stops partway. `unsafe` reaches every writer, so every fact is defeasible. Interior mutability punches deliberate holes in exclusivity. The aliasing facts ride on function parameters but not on the data pointers loaded through them (§ 5). No signature declares an effect an optimizer can trust across a call boundary (§ 4). No writer can state, let alone check, an algebraic law (§ 6). And a large installed base forbids deleting the slow shapes (§ 1).

The rungs are ordered. Each language further down keeps more facts in the source, and each runs faster for that reason. Whitefoot sits at the limit of the ladder.

Two observations finish the argument.

First, most of what we call optimization infrastructure is archaeology: compile-time and runtime machinery that recovers, under guard and at real cost, facts that were free at authoring time. Speculative JITs, escape analysis, alias analysis, link-time optimization, and profile-guided optimization all dig for information the language design discarded.

Second, the facts that matter most resist recovery by observation. A profile can steer a cost decision, since a stale profile at worst yields a slower program. A correctness-grade fact behaves differently. "These pointers never alias," "this function has no side effects," "this operation is associative": each carries undefined behavior on violation, so the compiler uses it only when the source checks it. Information thrown away upstream has no downstream fix.

So why does every language accept the loss? The writer was human. Humans will not state every fact, will not tolerate a single spelling, want familiar patterns, and want an escape hatch. Ergonomics licenses the omission, and the optimizer pays the fee.

Change the writer, and the bargain flips. An AI writer pays verbosity in tokens, which are cheap and getting cheaper. It brings no style attachment, no installed base, no familiarity demands. It needs regularity instead, since irregularity rather than verbosity is what makes weak models fail; a specification small enough to ride in its context window; and deterministic compile-time feedback it can act on. And it must not get an escape hatch, because a stuck writer will use it.

For the first time, one design becomes rational: a proof-carrying systems
language that demands the whole truth at authoring time. State every fact,
spell each construct one way, check everything, carry the verified consequence
to the optimizer, and erase the proof itself. Nothing to recover downstream,
because nothing gets discarded upstream.

An optimizer runs only as fast as the facts it is handed. Human-first languages spend their ergonomics budget throwing those facts away, and the optimization stack labors to get them back. Change the writer and you can stop discarding them. That premise runs through the rest of this document.

The goal, in one statement. Whitefoot aims past the C baseline, not at it. It expresses what C expresses where performance lives, and it hands the backend machine-checked facts C and Rust structurally cannot: guaranteed aliasing from ownership, effect declarations the optimizer trusts across opaque boundaries, checked algebraic laws, and hazardous-operation obligations discharged by proof instead of by trust. The AI writes under a checker it cannot cheat, the human states requirements and verifies results, and entire bug classes stay unrepresentable.

The project holds itself to one honesty bar: a design decision that leaves Whitefoot equivalent to Rust has failed, because "just use Rust" was cheaper. Every difference below names what it buys over Rust.

---

# Part I: The goal

## 1. Why a new language, when LLMs already write great Rust?

The seasoned engineer objects first, so the answer goes first: *"LLMs write good Rust today. Ban `unsafe` in CI and you have safe Rust: memory-safe, fast, mature, with a decade of tooling. Why invent a language with no training data that nobody knows?"*

Concede what holds. For the ceiling, expert Rust runs high. Banning `unsafe` is a real, cheap discipline. Ask whether a top-decile engineer, given time, can write a fast safe program in Rust, and the answer is yes. Nothing below disputes it.

The answer comes in two moves.

**Move 1: the ladder.** Rust is the furthest rung a for-human language can reach, and its remaining information losses are load-bearing parts of its contract with human writers, not oversights. `unsafe` exists because experts demand final authority. Interior mutability exists because shared-mutable is sometimes the convenient shape. Five ways to write a loop exist because expressiveness is an ergonomic good. You cannot fix these by extending Rust; removing them breaks the contract that made Rust adoptable. A language whose writer is an AI signs a different contract.

**Move 2: the floor.** A ban on `unsafe` bounds what a program may do. It does not bound what a program may be.

Nothing in safe Rust prevents the `Rc<RefCell<T>>` object graph, the pointer-chasing layout, mutation scattered across twenty call sites, the obvious indexed loop that runs 1.6x slower than the restructured one (§ 3, measured), or an `assert!` that looks like a contract and optimizes like a comment (§ 3, also measured). In a large codebase, and AI-written codebases grow large fast, whatever is representable eventually gets written. Rust cannot enforce a floor under program quality, because its human contract forbids taking shapes away.

Whitefoot constrains the writable shape-space itself. One spelling per
construct, to the byte. One conditional form and closed loop headers. Overflow
behavior chosen in the operation's name, per call site. A closed, taught
catalog of program architectures. Static obligations for every partial
operation, with source invariants and finite proof steps when fixed AUTO is
insufficient. No `unsafe` or writer proof trap exists. The worst program the
checker accepts is still memory-safe, race-free, and on a fast shape. That is
the floor, and it holds only because the writer has no installed base to
appease.

The floor already shows up in measurement. In the project's shipped-library comparisons, the first correctness-green artifact a fixed mid-tier model produced, with no benchmark feedback, no performance hints, and no human edits, beat the shipped Rust `percent-encoding` crate by 1.65x and the shipped `utf8parse` crate by 1.10x on locked workloads, every bounds check still in place. The model was not clever; the shapes the language permits are the shapes that run fast. Both results carry their protocols and caveats in the repository, and they are floor evidence for these two libraries and corpora, not a universal claim.

For the ceiling, Whitefoot wins where its extra fact channels bite and ties elsewhere, and Part II shows both. The core answer to "why not Rust" is the floor, because for the floor Rust has no mechanism at all.

Two follow-up objections:

- *"No training data."* You teach the language in-context, not by pretraining. The specification stays compact by binding constraint. The current working budget, including the taught pattern cards, runs about 48k tokens, a fraction of a modern context window, with few rules, no special cases, and each rule stated once. New model, same spec, same result.
- *"How does an AI even debug it?"* It rarely has to. The design moves failure to the one feedback channel an AI handles well. Every rejection cites one rule, the exact tree location, and where possible a mechanical fix; the diagnostics are deterministic and byte-stable. The compile-check loop is the writer's inner loop, and runtime debugging is the failure mode the whole design avoids.

## 2. What safety is actually for here

Safety in Whitefoot is load-bearing infrastructure, not the mission statement, and it does two jobs worth separating.

**Job 1: safety raises the floor.** The writer is an AI. Some writers, left unconstrained, produce subtle memory bugs, and an unattended writer cannot debug a latent use-after-free three weeks later. Runtime corruption is the worst feedback channel there is. So the language removes the surface. Data races, use-after-free, dangling references, double-free, uninitialized reads, and silent overflow stay unrepresentable in accepted programs, not merely detected. No `unsafe` block reaches review, because no `unsafe` block exists. Rust polices this boundary by convention, through CI bans and code review of `unsafe`; Whitefoot has nothing to police. Alongside the single spelling, the taught catalog, and the checked contracts, this is one of the mechanisms that hold the floor: even a bad writer's accepted program is a good program.

**Job 2: the safety machinery is the aliasing machinery.** This is the part a performance engineer cares about. The ownership and exclusivity rules that make the language safe are the same facts an optimizer needs.

- An exclusive borrow (`&uniq`) means no other access path exists, which is exactly `noalias`, held universally, with no interior-mutability holes and no `unsafe` to falsify it.
- Race-freedom keeps those facts sound. A data race would let another thread observe or mutate memory in ways that falsify the compiler's reasoning after the fact. A language that guarantees race-freedom carries its aliasing facts through concurrency.

So the usual intuition inverts. "Safety costs speed, and I buy speed back with `unsafe`" stops holding, because the thing that makes the program safe is a thing that makes it fast. Sections 3 and 5 show it: the statically proved capacity contract that protects the output buffer deletes the bounds checks, and the exclusive borrow that prevents the aliasing bug unlocks guard-free vectorization.

One honest boundary. This argument says safety and speed share machinery; it
does not say every dynamic uncertainty is free. A partial operation's domain is
proved before execution. A condition that can legitimately be false is
represented by a typed ordinary outcome and real source control flow. If
neither applies, the source is rejected—never repaired with an executable proof
fallback.

The proof cost is source explicitness. Consider the relation that bounds a
running byte sum:

```whitefoot
for (
  i in 0_u64..count,
  invariant per_byte: sum <= 255_u32 * i
) {
  let w = deref(weights)[i];
  let wide = cvt::<u8, u32>(w);
  set sum = sum + wide;
}
```

The header states what must hold at every reachable iteration. AUTO subtracts
the one published affine premise `per_byte`; DIRECT then proves the residual
from the `u8` type interval of `wide`. Writing a use block there would be
redundant and invalid. When a relation needs three or more published affine
premises outside the final fixed L0-image route, or a special route, a local
invariant carries explicit `use` steps. AI absorbs that
verbosity and may search while writing it. The compiler does not search for the
invariant, choose an arbitrary proof route, ask an SMT solver, or let a timeout
choose the verdict.

---

# Part II: Where the speed comes from

**Claim boundary:** sections 3–6 explain historical fact-channel experiments.
The current compiler uses statically proved requirements and structural facts
to discharge its finite safety obligations. It checks effect rows and borrows.
D7 replaces the former FN-4 law table with checked function-kind bindings;
those bindings imply no algebraic laws or reduction transformations. The
measurements remain evidence for separately selected
optimizer consumers, not a claim that every historical consumer ships today.
Their listings preserve the exact retired source—including the old
`loop @label {` spelling—and are evidence, not current writer examples.

Each section below covers one mechanism: what the writer states, what the checker verifies, what the backend receives, and what the machine code looks like on both sides of the comparison. Every listing is an excerpt from a committed file, trimmed where marked, with full paths in the appendix. Times are medians on an Apple M4, under the exact protocol in each experiment's RESULTS file.

## 3. Everything is checked, so where does the speed come from?

Take the objection the last sentence invites. Whitefoot does not answer it by
sprinkling implicit runtime checks through the program. An index, exact integer
operation or system buffer range carries a deterministic proof obligation. If
the checker cannot discharge it, the source is rejected. Total and checked
operations return ordinary values, and expected external failures return typed
outcomes. There is no writer-visible runtime proof backstop, `unsafe`,
unchecked assumption, or freely callable trap.

The interprocedural mechanism is a static contract. A non-entry function may
carry one `contract` block with erased `define` abbreviations and independent
`requires` clauses. Every ordinary caller proves every clause in the same
pre-transfer state. The callee receives those facts at body entry without a
runtime prologue, storage slot, `llvm.assume`, or fallback edge. The sole
`command fn main` has no contract and cannot be called from source.

Worked example: a base64 encoder, the two-buffer shape every codec has, one shared input, one exclusive output, and a capacity relation between them. The committed source:

```
fn encode['r](out: &uniq 'r buffer<u8>, src: own buffer<u8>) -> written: own u64
    reads('r), writes('r) contract {
  define out_len = len<u8>(deref(out));
  define src_len = len<u8>(src);
  define out_groups = ishr.wrap<u64>(out_len, 2_u32);
  define covered_src = out_groups *wrap 3_u64;
  requires ile<u64>(src_len, covered_src);
} {
  ...
  loop @full {
    ...
    let b0 = src[i];
    ...
    set deref(out)[o] = c0;
    ...
    set i = i +wrap 3_u64;
    set o = o +wrap 4_u64;
  }
  ...
}
```

One line of contract says the output can hold `4 * ceil(len_of(src)/3)` bytes, spelled as an overflow-safe comparison. The body is the obvious indexed loop: three reads, four table lookups, four writes per iteration.

Before the proof tier (committed `b64.s`, the checked build), every read and write in the loop carries its compare-and-branch. This is what "everything is checked" costs:

```
LBB0_9:                          ; hot loop, pre-proof build
	sub	x13, x10, #1
	cmp	x13, x3               ; src bounds check
	b.hs	LBB0_26               ;   -> trap
	add	x14, x13, #1
	add	x13, x13, #2
	cmp	x14, x3               ; src bounds check
	ccmp	x13, x3, #2, lo       ; src bounds check
	b.hs	LBB0_26
	cmp	x12, x1               ; out bounds check
	b.hs	LBB0_26
	...                           ; (each of the 4 stores guarded the same way)
```

In the historical proof build (compiled from the committed `b64.ll`), the
prover connected the passed capacity fact to the loop induction `i = 3k, o =
4k` and discharged all 27 bounds sites. Under that retired experiment's
semantics the entry check compiled to one comparison, and the hot loop carried
zero check branches:

```
_encode:                         ; proof build, from committed b64.ll
	lsr	x8, x1, #2
	add	x8, x8, x8, lsl #1    ; 3 * (out_len / 4)
	cmp	x3, x8                ; the ONE retained entry check
	...
	b.ne	LBB0_11               ;   -> trap (protects even a C caller)
	...
LBB0_6:                          ; hot loop: loads, lookups, stores. No checks.
	ldurb	w13, [x11, #-2]
	ldurb	w14, [x11, #-1]
	ldrb	w15, [x11], #3
	...
	ldrb	w13, [x9, x13]        ; alphabet lookups
	ldrb	w14, [x9, x14]
	...
	sturb	w13, [x12, #-3]
	sturb	w14, [x12, #-2]
	sturb	w15, [x12, #-1]
	strb	w16, [x12], #4
	cmp	x3, #2
	b.hi	LBB0_6
```

Measured on the same 384 MB harness, same source, byte-identical output: 2.48 GB/s with the checks retained and 4.23 GB/s with the proof, a 1.71x gain within noise of a perfect-prover ceiling measurement. The retained entry trap and C-ABI probe were properties of that retired experiment, not the current language: today an undersized call is statically uncallable, while an expected small-buffer case must return a typed shortage outcome.

Now the comparison that makes this a language argument instead of a compiler trick. A controlled adversary run, all variants at full RFC semantics, all enforcing the same entry relation, same machine, isolated processes:

| variant | throughput | vs Whitefoot |
|---|---:|---:|
| Historical Whitefoot, obvious loop + checked entry predicate | 4.285 GB/s | 1.000 |
| Rust, obvious indexed loop | 2.673 GB/s | 1.60x slower |
| Rust, obvious loop + `assert!` up front | 2.677 GB/s | 1.60x slower |
| Rust, expert `chunks_exact/zip` restructure | 4.297 GB/s | tie (0.997) |
| Rust, `unsafe` indexed | 4.111 GB/s | 1.04x slower |

Three things fall out of that table.

- The folk remedy measures dead: `assert!` recovers nothing. LLVM cannot connect a top-of-function assert to the coupled `i += 3, o += 4` induction, so every interior check stays. The construct that looks like a contract optimizes like a comment. Whitefoot's static `requires` differs in that the checker proves it at each call and records which fact discharged which site.
- Expert safe Rust ties. A `chunks_exact/zip` restructure reaches the same check-free class. That takes real skill: the writer must know the idiom, the shape does not generalize to variable-size output tokens, and nothing verifies the reasoning. The obvious shape stays 1.6x behind.
- Even `unsafe` buys nothing here, landing a little behind both.

The floor claim lands in one experiment. In Whitefoot the obvious shape plus one checked line reaches the expert class; in Rust the obvious shape stays 1.6x behind and the honest-looking remedy does not work. The ceiling tie is the honest half of the same story.

## 4. Effect rows the optimizer can trust across an opaque boundary

Every Whitefoot signature declares its effects: `pure`, or a row of
`reads('r)`, `writes('r)`, `allocates(...)`, `traps`. The checker verifies the
row in both directions, so it is a checked fact rather than an aspiration. The
retired experiment lowered suitable rows to function attributes. The current
compiler emits no effect-derived attributes, and v0.17 has no termination
checker from which to derive `willreturn`.

Rust has no channel for this. An optimizer facing a Rust call inlines it or proves its purity from the body; across a crate boundary or an `extern fn` it has nothing to prove from and assumes the worst.

The committed experiment isolates that boundary. A pure mixing function compiles into its own object file, so the caller's compiler never sees the body:

```
fn mix(x: own i64) -> mixed: own i64 pure {
  let a: own i64 = imul.wrap<i64>(x, 2862933555777941757_i64);
  let b: own i64 = iadd.wrap<i64>(a, 3037000493_i64);
  ...
}

// caller: loop accumulating mix(k) with a loop-invariant argument
```

Without the declared effects (control build), the caller does what every C and Rust compiler must do, two billion real calls:

```
LBB0_1:                          ; control: the loop survives
	mov	w0, #42
	bl	_mix                  ; a real call, every iteration
	add	x19, x0, x19
	subs	x20, x20, #1
	b.ne	LBB0_1
```

With the declared row on the declaration, the compiler hoists the call and strength-reduces the accumulation, and the loop is gone:

```
_main:                           ; effects build: no loop left
	mov	w0, #42
	bl	_mix                  ; called once
	mov	w8, #49664
	movk	w8, #3051, lsl #16
	mul	x8, x0, x8            ; result * iteration_count
```

Measured: 1.47 s down to 0.00 s, a complexity-class change from O(n) to O(1), across a boundary no inliner can cross. The Rust comparisons frame it. Rust's ordinary cross-crate build (no LTO) runs the same 1.49 s; Rust with fat LTO ties us, because with the whole program visible LLVM infers the same facts.

So the delta states precisely: Whitefoot's per-file default equals Rust's most expensive build configuration, and the guarantee holds where inference cannot reach, on opaque objects, cached artifacts, foreign boundaries with declared effects, and bodies too complex for the attributor. This is a build-economics result as much as a speed result: LTO-grade cross-module optimization at ordinary `-O2` per-file cost, which also keeps the AI writer's compile-check loop fast on large projects.

The checked rows also pay off in architecture, developed in § 9: you can grep the signatures and read the effect structure straight off them, so "exactly one function in this system writes to the world state" is verifiable from signatures alone.

## 5. Ownership is the aliasing fact-base: guard-free vectorization

Two historical experiments tested one mechanism: turning a checked borrow mode
into `noalias`-grade IR facts. The current compiler checks the borrow relation
but does not emit that alias metadata.

**First, the C-facing half.** Take the classic reduction `accumulate(acc, addend, n)`, whose body folds `*addend` into `*acc`, compiled from Whitefoot borrows against plain C pointers. Naive C reloads and re-stores through both pointers every iteration, because `acc` and `addend` might alias:

```
_accumulate_naive:               ; C, no restrict
LBB0_2:
	ldr	x10, [x1]             ; reload *addend  (might have changed!)
	eor	x8, x10, x8
	mul	x8, x8, x9
	str	x8, [x0]              ; re-store *acc   (might be observed!)
	subs	x2, x2, #1
	b.ne	LBB0_2
```

Whitefoot's `&uniq acc, & addend` emits `noalias`/`readonly` on the parameters by construction, and the loop keeps everything in registers:

```
_accumulate:                     ; Whitefoot, from the borrow modes
	ldr	x8, [x1]              ; load *addend once
	ldr	x9, [x0]              ; load *acc once
LBB0_2:
	eor	x9, x8, x9            ; register-only loop body
	mul	x9, x9, x10
	subs	x2, x2, #1
	b.ne	LBB0_2
	str	x9, [x0]              ; store once at exit
```

On the additive variant of this kernel the same fact changes the complexity class: naive C keeps an O(n) loop behind a runtime alias check while Whitefoot collapses it to a single multiply-add, measured at about 22x over unannotated C. Honest label: C with a correct `restrict` and Rust's `&mut` both reach the same code here, since parameter-level no-aliasing is table stakes among the fact-keeping languages. The difference at this rung is that C's fact is an unchecked promise the programmer must remember and get right, while Whitefoot's is a checked consequence of the type.

**Second, the half Rust cannot reach.** Rust's `&mut Cols` gives LLVM `noalias` on the reference parameter, but the `Vec` data pointers loaded through it are fresh pointers the optimizer treats as possibly overlapping each other. That is exactly where real numeric kernels live: multiple columns, loaded from one struct, read and written in one loop.

The committed kernel is a struct-of-arrays update, eight `u64` columns, two written, six read, the obvious loop:

```
struct Cols {
  a: buffer<u64>;  b: buffer<u64>;  c: buffer<u64>;  d: buffer<u64>;
  e: buffer<u64>;  f: buffer<u64>;  g: buffer<u64>;  h: buffer<u64>;
}

fn kernel['r](s: &uniq 'r Cols) -> completed: own unit reads('r), writes('r), traps {
  ...
  loop @l {
    ...
    let xa: own u64 = index<u64>(deref(s).a, i);
    let xc: own u64 = index<u64>(deref(s).c, i);
    ...
    set index<u64>(deref(s).a, i) = t4;
    ...
  }
}
```

Because `buffer` values are single-owner and `&uniq` loans are exclusive with
singleton provenance, the retired experiment emitted per-column alias scopes on
the loaded pointers. The identical program in Rust (committed alongside, same
semantics, three shapes) vectorized through loop versioning: LLVM emitted a
cascade of runtime pointer-overlap guards and a speculative fast path. From the
committed Rust assembly of the obvious shape:

```
	cmp	x8, x7                ; column pair overlap test
	ccmp	x9, x24, #2, lo
	cmp	x8, x19               ; next pair
	ccmp	x10, x24, #2, lo
	cmp	x8, x20               ; next pair
	ccmp	x11, x24, #2, lo
	...                           ; 29 guards before the first vector op
```

The scoreboard for the same semantics, verified in the committed assembly:

| variant | vector ops | runtime alias guards | asm lines |
|---|---:|---:|---:|
| Whitefoot, obvious shape | 8 | **0** | **121** |
| Rust, obvious shape | 65 | 29 | 2,132 |
| Rust, destructure-and-slice | 73 | 54 | 2,360 |
| Rust, expert inner-fn-with-8-slice-params | 14 | 0 | 499 |

The timing story has three parts, and all three matter.

- **Short trips are the real speed delta.** At trip count 8, Whitefoot runs 2.0x Rust's obvious shape and 1.17x even the expert shape, which pays its extra call. Short-trip kernels called in outer loops, per-row updates, and small fixed-width blocks are common.
- **Long trips tie.** At trip counts of 32 and up, Rust's guards amortize and the times converge.
- **Code size is the durable win.** 121 lines against 2,132 for the same large-n speed, a factor of 17. Versioned-loop bloat is invisible in a microbenchmark and shows up as instruction-cache pressure in a real program. At 16 columns the Rust guards grow to 111 and the code to 2,836 lines, while the Whitefoot kernel stays at 183 lines with zero guards. The fact is static and O(1); the recovery is a runtime mechanism that scales with pointer count.

The safe-Rust escape (the committed `inner-fn` shape) is instructive. Pass all
eight columns as separate slice arguments to an inner function, because
parameter-level `noalias` is Rust's only aliasing channel. It works, at the cost
of being an idiom you must already know, applied at every such loop. In the
historical Whitefoot experiment the obvious shape was the fast shape at every
trip count; reproducing that property in the current compiler remains open.

One more consequence. `RefCell`, `Cell`, and every other interior-mutability device do not exist here. That is a performance decision: one shared-mutable hole anywhere in the type system makes every aliasing fact conditional. Because shared-xor-exclusive is absolute, the facts hold universally. An architecture pattern replaces those idioms, in § 9.

## 6. Checked algebraic laws

A compiler may not reassociate your reduction. Floating-point and saturating operations are not associative in general, so LLVM preserves your evaluation order, and your serial fold stays serial: one `add` chained to the next, bounded by dependency latency, on every language's output.

Rust experts know the workaround, a hand-written multi-accumulator loop. That workaround hides an assertion. The human claims the operation is associative, and nothing checks the claim.

The former law table made the law a checked, declared fact for source acceptance.
The retired optimizer experiment additionally consumed it for reassociation.
D7 removes that source syntax and assumes no laws of supplied behavior. The
following listing and measurement are historical evidence for a possible
future proof consumer, not an active writer form or compiler capability.
The committed historical kernel:

```
contract SatMonoid {
  fn combine(x: own u64, y: own u64) -> combined: own u64 pure;
  law associative(combine);
  law commutative(combine);
  law identity(combine, 0_u64);
}

fn satadd(x: own u64, y: own u64) -> sum: own u64 pure {
  return iadd.sat<u64>(x, y);
}

conform u64 : SatMonoid {
  combine = satadd;
}

fn reduce(b: own buffer<u64>) -> total: own u64 traps {
  doc "The obvious reduction shape: sequential fold with the user op.";
  ...
  loop @l {
    ...
    set acc = satadd(x: acc, y: x);
    ...
  }
}
```

The writer states the laws. The checker discharges them against the operation table's semantics, where unsigned saturating add is associative as table data rather than writer folklore. Only then does the optimizer use them, reassociating the obvious fold into four independent, block-interleaved accumulators seeded with the proved identity. From the committed assembly, the four parallel saturating chains:

```
LBB1_3:                          ; reassociated: 4 independent chains
	adds	x8, x8, x15
	csinv	x8, x8, xzr, lo       ; saturate
	adds	x10, x10, x16
	csinv	x10, x10, xzr, lo
	adds	x9, x9, x15
	csinv	x9, x9, xzr, lo
	adds	x11, x11, x16
	csinv	x11, x11, xzr, lo
	...
	b.lo	LBB1_3
```

Measured at n = 65,536: 0.512 ns/element for the obvious fold in either language, since LLVM refuses to reassociate what it cannot prove, and 0.156 ns/element for Whitefoot, a 3.3x gain from stating a fact, tying Rust's hand-written four-accumulator expert shape at 0.159. At short arrays the per-call guard overhead shows and the expert shape leads, 0.210 against 0.155 at n = 4,096. The small print stays in the record.

The part that earns this section its place is the failure mode. The committed Rust adversary states it in its own comment: the expert shape works only while the human's algebra is right. Swap the operation for one that is not associative, signed saturating add, where `(MAX ⊕ 1) ⊕ -1 = MAX - 1` but `MAX ⊕ (1 ⊕ -1) = MAX`, and the two languages part ways.

- **Rust compiles it in silence.** The four-accumulator loop now computes a different function than the fold it replaced. No warning exists or can exist; the language has no idea the shape encodes an algebraic claim.
- **Whitefoot rejects it at compile time.** A declared `associative` law on the signed op is refuted against the operation table with a rule-citing diagnostic, and a conformance test pins exactly this case. An undischargeable law is a hard error, so a stated-but-unchecked law never reaches the optimizer.

The transform every performance engineer does on faith is, here, a checked fact. The honest mistake is unrepresentable, and the fast shape is the obvious fold.

## 7. Keep boolean state boolean, and the vectorizer widens it

A smaller mechanism with an outsized codegen swing, and an honest framing: this one guarantees a shape C and Rust also have, and does not beat them.

Scanner kernels (word counting, token classification, run detection) carry per-byte boolean state across loop iterations. Express that state as an integer flag (`0`/`1` in a `u64`) and LLVM's vectorizer carries a full-width integer recurrence, vectorizing at width 2. Express it as a genuine 1-bit boolean combined with boolean operations, and the recurrence stays `i1`, so the same loop vectorizes at width 16.

The committed word-count kernel keeps every predicate in `Bool`:

```
    let c: own u8 = index<u8>(b, i);
    let ge9: own Bool = ige<u8>(c, 9_u8);
    let le13: own Bool = ile<u8>(c, 13_u8);
    let inrange: own Bool = band<Bool>(ge9, le13);
    let sp32: own Bool = ieq<u8>(c, 32_u8);
    let issp: own Bool = bor<Bool>(inrange, sp32);
    let notsp: own Bool = bnot<Bool>(issp);
    let starts: own Bool = band<Bool>(prevspace, notsp);
```

The committed assembly shows the result: full 16-byte vector lanes (`movi.16b`, `cmeq.16b`, `and.16b`, and peers) where the integer-flag version of the same logic capped at width 2 by interleave 4.

Measured on the whole historical kernel, the integer-recurrence form ran
1.6–1.8x behind C and Rust; the `i1` form reached parity with both. Safe Rust can
express the same wide shape, so this workload gives it no fact advantage. The
seeded Whitefoot catalog teaches Boolean dataflow for scanner state; project
validation still has to show that this reliably prevents the slower shape.

---

# Part III: Making the fast shape the only shape

The sections above are historical evidence for fact channels: things the writer
states and a future verified consumer may use. This part is the complementary
bet, to remove the shapes that waste the facts. Nobody hand-tunes the average
line under a deadline, and in a million-line codebase the average line is where
the time goes. Whitefoot aims to make recurring slow shapes unrepresentable or
unreachable; the current seeded catalog has not yet established that claim.

## 8. One spelling, to the byte; integer domains made explicit

There is one legal spelling per construct and one legal byte-level formatting, covering indentation, spacing, and blank lines. The toolchain rejects non-canonical input; it never reformats. Flat integer infix has no precedence or nesting; composition uses `let`. There are no comments; documentation lives in a structured `doc` field. `if` handles `Bool`, `match` handles enums, and the iteration forms are `loop` and one ascending half-open `for`. Arithmetic semantics are explicit at each site:

```
fn clamp_hundred(value: own u64) -> result: own u64 pure contract {
  ensures result <= 100_u64;
} {
  return imin(value, 100_u64);
}
```

The helper owns and proves that normal-result promise. Its caller consumes the
verified relation directly:

```
let wrapped = a +wrap b;
let bounded = clamp_hundred(value: a);
let exact = bounded + 1_u64;
let checked = a +checked b;
let saturated = a +sat b;
```

The wrapping, checked, and saturating forms are total value operations. The
bare exact addition has one matching static domain obligation. FN-9 proves
`clamp_hundred`'s relation at each normal return, S12 publishes it after the
call, and that machine fact discharges the addition without a caller restatement
or runtime proof path. If the body stops satisfying the promise, the callee fails FN-9;
if the promise is weakened, the ordinary caller operation fails at its own
proof obligation. A caller may not inspect the body and manufacture a hidden
postcondition, even after copying, converting, wrapping,
projecting, or storing the result. A potentially false condition still uses the
checked row or an ordinary branch and value outcome. There is no
arithmetic-specific trap mode.

Why an expert should care rather than wince:

- **Irregularity is the weak-writer failure surface.** What breaks smaller models is choice, not verbosity: alternate spellings, precedence, context-dependent elision, special cases. One spelling costs tokens, which the writer pays gladly, and buys zero ambiguity. The rule survives model scaling, since it targets error surface, not context size.
- **Canonical bytes leave nowhere to hide.** Two programs differ if and only if their bytes differ. A formatting-only diff cannot exist, and a sneaky edit cannot hide in one. Review, diff, and caching all sharpen (§ 11).
- **The semantic split kills a real divergence.** C and Rust ship two different programs from one source. Rust panics on overflow in debug and wraps in release; C invokes undefined behavior on signed overflow, and the optimizer uses that. In Whitefoot, debug and release compile the same program: `+wrap` always wraps, `+checked` always returns a value-level outcome, and bare `+` exists only after its exact domain is proved. What you test is what ships.

The verbosity price is real, and Part VI states it. The purchase is zero irregularity, and every mechanism in Part II leans on it.

## 9. A closed, taught catalog of architectures; data-oriented layout by default

*Current status: this is the seeded D6 design doctrine in `docs/patterns.md`;
it is not yet ratified as normative writer doctrine.*

At statement level, Whitefoot forces one loop form and one conditional. It applies the same policy at architecture level: the set of blessed program-scale patterns is closed and taught up front, and the catalog meets two tests. It stays complete, so every task is modelable inside it and a gap is a documentation defect to fix rather than a writer error; and it stays efficient, so each pattern names the machine property or fact channel that makes it fast.

A human language could not do this. Designers must let users carry their familiar architectures in, or the language gets rejected. An AI writer has no architectural nostalgia, so the catalog can hold exactly the shapes the fact channels light up.

Take the flagship example, the command-buffer pattern, which replaces scattered mutation. Deep code, the parser, the rules engine, whatever lives at call depth ten, is declared `pure` or `reads('world)`: it computes and returns write-intents as plain values. One shallow function holds the single exclusive borrow of the world state and applies the intents. In Rust you may adopt this discipline; in Whitefoot the checker's rules leave you no other option, because deep code cannot manufacture write access it was not handed. The `Rc<RefCell<World>>` alternative, where you hand out shared handles, mutate from anywhere, and hope the panics find the overlap, is unrepresentable.

Enforcing it buys three things back.

- The aliasing facts of § 5 stay sound, because mutation cannot scatter.
- Effect rows make the architecture readable at a glance. Grep the signatures and you find exactly one `writes('world)` in the system, and the checker verified every row (§ 4). "Where can state change?" becomes a query, not an investigation.
- Read-only deep code is the precondition for the parallel fan-out story.

Second example: struct-of-arrays is the default layout for bulk data, and § 5's `Cols` is the shape as it looks in source. Contiguous per-field columns are what the cache and the vectorizer want; per-node heap objects with headers and refcounts are what object-oriented habit wants. The AI has no such habit, so the language teaches SoA as the norm. The catalog is empirical, not ideological. Where measurement shows the paired layout wins, as it did for hash-table key/value slots, where splitting keys from values doubled the cache misses per insert, the catalog pins that instead. Patterns encode measured layout decisions.

The one line that justifies the whole part: whatever is representable eventually gets written. A language that leaves the slow architecture representable has scheduled its own performance regressions; one that removes it makes the fast shape the path of least resistance, and the only path.

## 10. Handles and copies instead of references

*Current status: append-only index-linked SoA has a seeded pattern and current
witness. Recyclable generational pools, stale-handle checks, and check-elision
schemes below are proposed or historical, not current language/compiler
capability.*

The last floor mechanism governs where long-lived data lives. The center of gravity moves off borrows. Big structures live in pools, and ordinary code holds either the value itself for small copyable data, or ownership moved in and out, or a handle—an index of plain copyable data rather than the object itself.

Node links in a tree or graph are handles into the pool, not pointers or references. Two consequences matter here.

- **Performance:** contiguous storage, no per-node allocation, no headers, no refcount traffic, and indices that survive relocation. High-performance C adopts this layout by hand in entity systems and arena-indexed ASTs; in Whitefoot it is the default, and it composes with § 5's disjoint-column facts.
- **The writer's burden:** most code holds no loans at all, so the borrow rules bite only at the few sites that point into something. The self-referential-struct wall that pushes real Rust projects through `Pin`, `unsafe`, or index-arena workarounds does not exist, because structs store values, not borrows. The problematic program is not painful to write; it is impossible to state.
- **Safety of stale handles:** a future recyclable pool would pair each slot with a generation and expose mismatch as a typed lookup outcome. A property of that lookup result must be a machine-verified interface relation where the contract language can express it; otherwise the caller branches on the typed outcome. A caller cannot manufacture an unstated property of the returned lookup value with a local invariant: the invariant itself must be proved. There is no dedicated hidden handle trap. Check-free schemes (loans that freeze reuse for a scope, affine owned handles, proof-discharged repeat checks) remain an active research track.

A Rust engineer will recognize this as "just use indices into a `Vec`," the arena
idiom Rust folklore already recommends for escaping its own borrow checker at
these exact sites. In Rust a stale unchecked index may silently select a reused
slot. The proposed Whitefoot generational form would close that hole; the
current append-only pattern avoids reuse instead and makes no generational-pool
claim.

---

# Part IV: One program, one tree, one byte-form

## 11. The canonical-form dividend

Section 8's "one spelling to the byte" has consequences past writer reliability, and they compound enough to earn their own section.

The specification pins the facts: one spelling per construct and one byte-level formatting, rejected rather than reformatted; a one-to-one, machine-enforced mapping between grammar productions and syntax-tree nodes; accepted programs elaborate to a canonical checked program that makes every derived operation explicit, including drops, releases, instantiations, and discharged static obligations; and diagnostics that are deterministic and byte-stable, each rejection citing one rule and one tree path. Proof syntax is erased before runtime lowering. Compiler stages are ordinary compiler data, and an inconsistency among them is a compiler defect rather than a reason to export or replay a second proof object.

What that buys, labeled by evidence status:

- **Reproducibility (by construction, partially exercised).** The language and
  artifact design require byte-stable results for the same source and pinned
  toolchain. The retired compiler prototypes exercised deterministic parsing
  and selected IR fixtures, but Whitefoot does not yet have a complete
  self-hosted compiler or a production-object reproducibility result.
  [Complete artifact reproducibility](ideas.md#reproducible-compiler-outputs)
  remains an open question for a real build, distribution, caching, or audit
  consumer rather than an already earned claim.
- **Semantic diff and merge (by construction).** No formatting variance exists, so every diff is a semantic diff. There is no "reformatted, 2,000 lines changed" commit, no style debate, and nowhere for an unintended edit to hide in noise. For AI-written code this is the review story: what changed is what the diff says changed.
- **Caching and build speed (projected, labeled).** Source-to-tree is a bijection, the canonical artifact is deterministic, and effect rows already decouple optimization from body visibility (§ 4, the measured half of this claim). Those are the preconditions content-addressed build caching wants, designed in on purpose. The honest status: the design removes the classical obstacles to very fast incremental compilation, and no build-speed number is measured yet.
- **The repair loop (by construction, exercised daily).** Deterministic, rule-citing, byte-stable diagnostics are an API, not prose. The writer that consumes them is a machine: same mistake, same message, same fix, every time. This is what "the AI can act on failure" means in practice.

The through-line: in most toolchains, formatting, diffing, caching, and diagnostics all run on heuristics, because the language admits many texts for one program. Collapse that to one text, one tree, one artifact, and everything downstream stops guessing.

---

# Part V: The trust story

## 12. The proposed ten sealed building blocks, and a proof lane in

*Historical direction: D17 selected a representation-invariant proof lane
while deferring the exact sealed catalog and mechanism. The retained
[storage questions](ideas.md#storage-transitions-and-representation-privileges)
do not select a concrete proof or privilege mechanism; the active specification
defines current language features.*

An expert asks the right question next: *"Some structures cannot be written in checked code in any language. A production hash table's reality, one thousand slots, thirty-seven live, liveness tracked in side-band control bytes, is not expressible in your type system. Where's the escape hatch?"*

There is no writer-accessible escape hatch. The long-term answer considered here is a
proof lane; the short list considered beside it is a deferred candidate, and
the distinction is the point.

**The short list.** The archived capability pass proposed a small, fixed catalog of ten kernels (the growable sequence, the hash table, the object pool, the arena, the single-producer/single-consumer queue, file I/O, and peers), with trusted internals written by the toolchain itself. Inexpressibility was the admission criterion: a kernel entered the candidate catalog only when ordinary users could not implement its capability at par performance from the language and the existing checked libraries. Everything user-writable would ship as taught, checked source instead. This remains a deferred architecture, not the current language's trust surface.

Contrast the proposed shape of the risk. Rust's `unsafe` is the same admission,
that some invariants live outside the type system, distributed as a keyword to
every writer, in every crate, forever, so the ecosystem's soundness is the sum
of every author's care. If Whitefoot adopts a sealed catalog, it will
concentrate that risk into explicitly trusted project artifacts it owns, audits,
and can fix once so the world recompiles. No writer-reachable syntax escalates
into the privileged layer, so there is nothing to ban, lint, or review for.

**What "trusted" would cost before shipping.** The archived catalog proposal
required each candidate kernel to pass a five-part acceptance battery
(differential testing against the reference implementation it replaces,
exhaustive small-bound model checking, sanitizer and fuzz soak, hostile
pre-ship review, and a CI-pinned performance and assembly shape), plus complete
failure semantics and teardown protocols with fault-injection evidence. This
is a historical admission proposal, not a current gate or catalog.

The archived queue dry run shows what that proposed standard meant. Its model
check covered the recorded bounded state space and caught the planted ordering
mutants: weakening an acquire/release edge produced a read of an unpublished
slot or an overwrite of an unconsumed one. The historical implementation's
steady-state hot path used zero read-modify-write atomics in the recorded
disassembly. It beat a mature Rust SPSC crate on round-trip latency on the same
machine, while that crate led on batched throughput; both numbers are in Part
VI. None of this establishes a current Whitefoot kernel or production gate.

**The proof lane.** This is the project's long-term answer to extensibility. Trust is a temporary loan, and proof buys permanent privilege.

Under D17, a future project kernel whose exact representation invariants are
machine-proved leaves any explicitly trusted boundary and becomes checked code
with only the privileges justified by that proof. The same lane is open,
long-term, to users. Proof is not intended to become a prerequisite for
ordinary performance: if a sealed catalog is later admitted, ordinary code may
compose its project-provided operations. A user implementation receives such
privileges only through deterministic machine verification of its exact
obligations, never through human review or a writer assertion. The active
language defines no such representation privilege today. The source-proof
candidate's contracts and invariants justify ordinary program safety and
optimization facts; they do not silently grant D17 representation authority.

One doctrine runs at every scale. A bounds or exact-operation obligation is
proved (§ 3) or rejected; it never turns into an implicit
operation-specific trap or writer fallback. A
cross-function result fact comes only from a machine-verified callable
boundary, not from a caller reading a callee body. A reassociation happens under separately authorized checked evidence
(§ 6) or it does not. Under the long-term D17 lane, a privileged representation
enters checked code only through verified invariants; an unproved project
kernel, if a future specification admits one, remains an explicit trusted
boundary. Nowhere in the language does "I promise" compile.

---

# Part VI: What it does not beat, and what is not yet known

Claims earn belief by naming their edges. The current honest ledger:

- **Expert safe Rust ties on the base64 kernel at full semantics** (0.997, controlled rerun, § 3). The durable delta is the floor, not the ceiling: the obvious shape runs 1.6x behind in Rust and the `assert!` recovers nothing. Where a fixed-ratio restructure like `chunks_exact` exists, an expert who knows it reaches the same class.
- **Hand-tuned SIMD stays ahead of everything shown here.** Specialist kernels (`memchr`, bytecount-class scanning) beat the naive autovectorized shapes by 2-3x. Proof elision made the base64 kernel's scalar code perfect; it did not synthesize the wide-table SIMD algorithm. Vectorized blessed patterns are future work, not a present claim.
- **The language is verbose by design.** The base64 encode kernel runs about 90 lines where C writes 15. The writer pays in tokens; humans read the contracts and the docs. If you are optimizing human writability, this is the wrong language, on purpose.
- **The frequency question is open.** The fact channels win on kernels that exercise them. How often those patterns dominate real medium-to-large codebases is not established yet, and an early survey attempt was directional at best. This is the biggest honest unknown in the performance story.
- **The sealed-kernel numbers are shape validations, not shipped-product benchmarks.** The catalog dry runs were C implementations of the specified kernel shapes against mature Rust baselines on one Apple development machine: sequence push-then-sum about 1.5x over `Vec`, table iteration about 1.4x over hashbrown, steady-state insert modestly behind, 4 of 5 workloads inside the preregistered band; the queue beats `rtrb` by about 20% on round-trip latency while `rtrb` leads by about 25% on batched-32 throughput, both far above the band's floor. None of this is whitefoot-emitted code yet, and magnitudes will be re-established on the deploy target.
- **Single-shot writability is not solved.** The current clean baseline for one research kernel is roughly a quarter of programs correct on the first attempt, with the failure modes catalogued and fixes staged. The design answer has always been the diagnostic feedback loop (§ 11), and the loop's measured effect is the next experiment, not a completed one.
- **Several announced mechanisms are deferred designs, not shipped features.** The ten-kernel catalog and D17's user representation-proof lane are historical directions, with concrete mechanisms still open in the [storage questions](ideas.md#storage-transitions-and-representation-privileges). This document describes those designs and their evidence; only the active specification defines current language capability.

---

# Part VII: If you don't write systems code at all

Everything above was for the reader who argues with compilers. This page is for the reader who prompts.

When you ask an AI for software today, you inherit a bargain you never signed: the language it writes was designed for human hands. If the AI holds memory wrong, the program corrupts in silence. If it picks a slow architecture, nothing objects, and you find out when your users do. Your defenses are testing what you can see and trusting what you cannot.

Whitefoot changes what you trust. The AI writes, and a checker, a program with no goodwill to appeal to, accepts or rejects. The failures you cannot see are not caught by the checker; they are unwritable in the first place. Memory corruption, data races, silent overflow: no sequence of tokens means those things and compiles. And the shapes that run slowly are, to the greatest extent the design can force, off the menu too, because the fast layout and the fast architecture are the only ones the language teaches and permits.

You still verify what the program should do, since no checker knows your intent. But "it quietly does something horrifying under load" comes off your list. The slow version and the corrupt version won't compile.

---

# Appendix: evidence map

Every number above, with its committed record. Protocols, machines, and caveats live in each RESULTS file; nothing here is quotable without them.

| claim | record |
|---|---|
| Historical proof tier: 27/27 bounds sites discharged, 2.48 → 4.23 GB/s (1.71x), retired entry trap retained | `research/experiments/port-study/base64/RESULTS.md` |
| Pre-proof checked loop (asm) | `research/experiments/port-study/base64/b64.s` |
| Proof-build IR (asm regenerable with `clang -O2 -S b64.ll`) | `research/experiments/port-study/base64/b64.ll` |
| Adversary table: Whitefoot 4.285 GB/s; Rust obvious 1.60x; assert 1.604x; chunks tie 0.997; unsafe 1.040 | `research/experiments/port-study/base64/RESULTS.md`, `adversary_benchmark.py`, pinned CSV + metadata |
| Effect rows: 1.47 s → 0.00 s across opaque boundary; Rust no-LTO 1.49 s; fat LTO ties | `research/experiments/effect-attrs-channel/RESULTS.md`, `main_attr.s`, `main_plain.s`, `kernel.wf` |
| Naive C vs borrow-derived noalias; ≈22x additive variant; Rust parity | `research/experiments/codegen-vs-rust-c/SUMMARY.md`, `asm/kernelB_c.s`, `asm/kernelB_xl_acc.s` |
| Vectorization: 0 guards/121 lines vs 29 guards/2,132 lines; short-trip 2.0x; long-trip tie; 16-column scaling | `research/experiments/scoped-alias-channel/RESULTS.md`, `kernel.wf`, `kernel_facts.s`, `rust_kernels.rs`, `rust_kernels.s` |
| Historical checked laws: 3.3x over obvious fold; ties expert; signed-law refuted by the retired table | `research/experiments/checked-law-channel/RESULTS.md`, `kernel.wf`, `kernel.s`, `rust_reduce.rs`; D7 retirement reasons in `research/investigations/containers-and-resources/BEHAVIOR.md` |
| Boolean dataflow: width 16 vs 2×4; 1.6-1.8x closed to parity | `research/experiments/port-study/wc-chunk-summary/RESULTS.md`, `chunk_wc.wf`, `chunk_wc.s` |
| Shipped-library floor: 1.653x [1.631, 1.667] vs `percent-encoding` 2.3.2; 1.098x [1.085, 1.145] vs `utf8parse` 0.2.2; bounds retained | `research/experiments/default-floor/RESULTS.md` |
| Kernel-shape dry runs (C mockups vs `Vec`/hashbrown; bands) | `archive/research/systems-performance-coverage/m3a-kernel-dryrun/RESULTS.md` |
| Queue: exhaustive model check, all 4 weakened-ordering mutants caught; zero-RMW hot path; latency vs throughput vs `rtrb` | `archive/research/systems-performance-coverage/m6a-spsc-dryrun/RESULTS.md` |
| Reproducibility direction and the absence of a complete object claim | [Reproducible compiler outputs](ideas.md#reproducible-compiler-outputs) |
| Source-language rules cited (one spelling; reject-not-reformat; checked effects; static contracts; exact integer domains; prove-or-reject partial operations; source invariants and finite use steps) | `spec/kernel-spec.md` |
| Pattern doctrine (command buffer, SoA pool, boolean classifier, proof at the maintained boundary) | `docs/patterns.md` |
| Founding evidence for the premise (escape analysis conditionality, JIT recovery machinery, non-interference as the central enabler, IR semantics preservation) | `archive/research/phase2-notes/verified-findings.md`, `archive/research/phase2-notes/phase2-jit-findings.jsonl`, `archive/research/debates/round1-static-vs-profile.md` |
