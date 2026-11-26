# Is the checker itself right, and what to take as evidence

2026-11-12. I read a read-only copy of Whitefoot main (`55b6fd7a7`) and the merged PRs on GitHub. This machine cannot build the compiler or run programs,
so every claim of the form "is accepted" or "what happens at run time" comes from reading the code and the specification, and is marked "unverified", unless it says it is a measurement recorded in a PR. The process and
the raw material used are in `process.md` in the same folder.

## The conclusions first

1. **This study found two candidate holes. Both are in the range judgment.** The full programs are in the next section. In both programs,
   the checker proves that `251_u8 + v` does not overflow, while at run time `v` can be 9. Both were confirmed on the compiler on 2026-11-25 and fixed by item #211; [the range-walk references record](../range-walk-references/DESIGN.md) holds the verdicts.
2. **The inferred risk recorded in the item (a body-less built-in function publishes the generated postcondition as proved) is already closed on main.** #398
   fixed it itself in its second round; the item's text is out of date. I checked this; see section 3.
3. **I replayed the three approaches against 27 soundness defects that have already been fixed** (the table in section 5). What I saw:
   - None of these 27 was found by running a program. Of the 17 whose origin is written down, 12 were found by reading in a review or an audit, and 5 were met while implementing a feature.
   - A small verifier catches none of them. The defects are not in "whether each step of the derivation is right". They are before the derivation: which facts are still live, whether two things were treated as the same one,
     whether the obligation was raised at all. These are exactly the part a small verifier must trust.
   - The first-release Lean proof reaches none of them today (the witness programs of all 27 use things outside the model's fragment). By design it covers only "whether the specification's rules are right".
     Of the 27, 12 are holes in the specification itself, 13 are the checker not doing what the specification says, and 2 are wrong backend generation. Lean never covers the last two kinds.
   - Runtime comparison clearly catches 11 of them. Another 12 depend on how wide the generator is written. For 4 there is nothing to observe at run time at all. The referee it needs
     (a build that keeps every proved condition as a runtime check) does not exist now. The existing sanitizers do not report a program that "computed wrong but did not touch anyone else's memory".
4. **My leaning** (the owner decides; section 8 gives three options): first do "search for counterexamples by matrix". That is the method that found these two candidates this time, pushed from manual review
   to mechanical enumeration, with only `whitefootc --check` as the referee, and no programs run. Connect the differential comparison between the Lean model and the Rust checker once the fragment grows. Put the build that keeps
   runtime checks later. Do not build the small verifier.

## 2. Two candidate holes (confirmed since)

Background, from the start: the compiler has two provers. Ordinary entailment (`compiler/src/semantic/entailment/`) runs first. As soon as a function carries a range clause
(for example `requires forall small(k in 0_u64..n): xs^[k] < 4_u64;`), it "participates" in the range judgment (spec `RANGE-2`). The range judgment
(`compiler/src/semantic/range_judgment/`, about 15,000 lines) then walks the function body again. It keeps its own record of each binding's value and each piece of storage's contents, and
**proves, on behalf of ordinary entailment, the overflow obligations (OP-2) and subscript obligations (OP-4) that ordinary entailment did not prove**. It has its own logic for "which write changed which value", and that logic and the one in ordinary entailment
(the kill events of ENT-5) are two separate implementations. The already-fixed #281 and #414 both came from here: in #281 it did not forget a value changed through a reference, and proved an
obligation with the old value; in #414 it treated a reachable path as unreachable.

The two below are the same kind of error as #281. I reread both against the code myself, and the paths hold. Without a compiler, they were unverified when this study was written; [the range-walk references record](../range-walk-references/DESIGN.md) holds how the compiler judged them.

### Candidate one: after a struct binding is replaced as a whole, a reference taken earlier and the binding come apart in the walk

```wf
struct Counter {
  count: u8;
}

fn bump(marks: &[u64], x: u8) -> result: u8 pure contract {
  requires forall small(k in 0_u64..marks^.len): marks^[k] < 8_u64;
} {
  let s = Counter(count: 0_u8);
  let r = &s;
  set s = Counter(count: 1_u8);
  set r^.count = x;
  let v = s.count;
  let w = 251_u8 + v;
  return w;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
```

- **Why the specification must reject it.** `r` refers to the storage of `s`. `set s = Counter(count: 1_u8)` writes exactly `r`'s own path. By the words of `REF-2`,
  "Writing the storage at `p`'s path or below it is a content write and does not invalidate `p`.", `r` is still valid and still
  refers to `s`. So `set r^.count = x` changes `s.count` to `x`, `v` can be any number from 0 to 255, `251_u8 + v` may overflow, the `OP-2`
  obligation cannot be proved, and the program should be rejected (`RANGE-3`: "An ordinary obligation owed here that does not hold is rejected with its own rule").
  `RANGE-2` itself also says "A binding is a location once a reference to it is formed.".
- **Which line let it through.** The range walk treats "whole assignment" as "this name now refers to a new piece of storage":
  `range_judgment/walk.rs:1449-1454` first forgets the old location (`havoc_location`), then uses `bind` to bind `s` to the new location that the construction expression made
  (`walk.rs:2806`, `Origin::Constructed(self.world.new_origin())`). But at `let r = &s;`, `r` recorded the old location
  (`walk.rs:2564-2578`; an aggregate binding takes the last line, `view_of`, and gets `View::Place(old location)`). After that, `set r^.count = x` writes into the old location
  (the `Target::Location` branch at `walk.rs:1482-1484`), and `let v = s.count` reads the new location, which still reads 1. `251 + 1` does not overflow, so it is proved.
  Scalar bindings do not have this problem: the fix for #281 makes integer and Bool bindings that have had a reference taken get forgotten after any "write that cannot be placed at a specific location"
  (the `expose` at `walk.rs:2565-2575`), but it did this only for integers and Bool.
- **Why the ordinary check does not reject it first.** At `set s = …`, `compiler/src/semantic/check/control/commit.rs:274-283` invalidates only references "whose path the target is a proper
  prefix of"; the comment is exactly that sentence of REF-2. `r`'s path equals the target and is not a proper prefix (`check/references.rs:2916-2919`: for a write event,
  `include_equal` is false; only moves and releases count "equal"). What is unverified: whether some other rule stops this step (I did not find one).
- **What happens at run time.** Call `bump` with `x = 9`; `251 + 9` exceeds u8. By `design/compiler/backend-facts.md`, an exact addition is emitted to LLVM with a no-wrap
  flag, so the result is not a determined wrapped value; it is undefined.
- **How to confirm.** Anywhere with a main compiler, run `whitefootc --check c1.wf` (the repository's rule is to do it in CI: add it as a conformance case on a
  branch, expecting `reject` with rule `OP-2`, and, as with #437 at the time, see whether the conformance job reports "want Reject reached Accept"). **Accepted = confirmed.**
  Rejected citing `OP-2` = refuted, which means there is handling that neither I nor the subagent saw. Rejected citing `REF-2`, `SET-1` or some other rule = this form itself is not
  accepted, and a different shape must be tried.
- Other shapes with the same root (each worth a case once candidate one is confirmed): put `set s = …` in one arm of an `if` (at the join, `world.rs:1385-1408` also
  gives `s` a new location); an array (`let r = &a; set a = array_filled…; set r^[0_u64] = x;`); a subscript version (ending in `cells^[v]`,
  `cells: &Array<u64, 4>`, expecting rejection citing `OP-4`); an enum (a later `match` treats the arm that is actually reached as excluded, and the obligations in that arm count as proved).

### Candidate two: the two arms of an `if` each give a reference to a different struct, and the walk treats this reference as a new piece of storage

```wf
struct Counter {
  count: u8;
}

fn bump(marks: &[u64], x: u64, y: u8) -> result: u8 pure contract {
  requires forall small(k in 0_u64..marks^.len): marks^[k] < 8_u64;
} {
  let s = Counter(count: 1_u8);
  let t = Counter(count: 1_u8);
  let r = if x < 10_u64 {
    give &s;
  } else {
    give &t;
  }
  set r^.count = y;
  let v = s.count;
  let w = 251_u8 + v;
  return w;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
```

- **Why the specification must reject it.** `REF-1`: at a join, the target of a reference variable is the union of the paths each incoming edge may refer to, so a write through `r` may land on `s.count`;
  on the path where `x < 10`, `v` is `y`. `RANGE-2`: "every write the walk cannot place forget[s] every location".
- **Which line let it through.** The two arms give two different reference values. `join_values` at `world.rs:1341-1410` has no branch for "both are references but not the same",
  and falls through to the last line, which returns `Value::Unknown`. Next, `walk.rs:885` binds `r` using `result_type`, and `result_type` is the referenced type
  (`Counter`), not "reference". The comment at `semantic/model.rs:3044-3048` says this itself, "is not recoverable from `result_type`";
  the range judgment does not read `result_mode`, which is what actually tells them apart. So `bind` (`walk.rs:1323-1330`), when "the value is unknown and the type is an aggregate", makes `r` a
  brand-new piece of its own storage. After that, `set r^.count = y` writes into this storage that no one reads, and in the walk `s.count` is still 1.
- **Why the tests did not find it.** When the referenced thing is an integer, it is right (unknown + integer type binds to an unknown integer, and a write through it goes to "forget all"). The unit test
  `a_bound_written_through_a_joined_reference` (`compiler/src/semantic/tests/range_facts.rs:1893`) tests exactly the integer case. No
  test or case uses a struct or an array as the referenced thing.
- **How to confirm.** Same as candidate one: `whitefootc --check c2.wf`; accepted = confirmed; rejected citing `OP-2` = refuted.
- Other shapes with the same root: `let r = &s; if x < 10_u64 { } else { set r = &t; } let q = r; set q^.count = y;` (writing directly through `r` is right;
  the alias `q` is bound wrongly); when the referenced thing is an element of an array and the element is a struct, after a write into it the `forall` facts about that array are not forgotten either.

These two candidates were found by one "review by matrix"; the method is in sections 6 and 7. The same pass also listed 13 forms where "the walk handles it right, but no rejection case guards it",
each with a ready program, in `process.md`.

Side evidence: the Lean proof study recorded (`research/investigations/spec-proof/DESIGN.md` "Sealed type invariants") that four successive review rounds of #484
all reported defects, two of which were holes in the rules themselves, "each where a rule met a write through a reference that may name several places: in
the fifteenth round a whole-value store, in the sixteenth a `swap`". The two candidates this time are exactly these two things: whole assignment, and a reference that may refer to several
places.

## 3. The inferred risk in the item is already closed

The item says: a body-less built-in function (for example `take_back`) publishes to the caller, as proved, the compiler-generated "satisfies the type invariant when it returns Some".
This was recorded on 10-12; #398 fixed it itself in its second round before merging on 10-13 (the PR body: "Body-less declarations published generated postconditions
unproved … fixed in this branch"). Places I checked:

- Spec `TYPE-11` (`spec/kernel-spec.md:464`) now says such postconditions belong only to a `fn_sig` and to a `fn_decl` whose "definition is a Whitefoot body",
  "since no body proves a relation of a definition the build supplies".
- Code `compiler/src/semantic/check/type_invariants.rs:518`: when `supplied_definition` is true (a declaration of the prelude or a host module), it returns empty
  directly; the range form is at `range_type_invariants.rs:86`.
- The conformance case `type11-neg-generic-storage-result` is exactly this shape (`pop_last::<Span>` takes an element out of storage and returns it, and is rejected).
- The detour, binding `take_back` to a function parameter that promises the invariant, also reads as closed (`check/behavior/contracts.rs:412-447` hands the generated
  postconditions together with the written ones to the `FN-4` comparison), but I did not trace line by line into the comparison function; this sentence is inferred.

## 4. One example worked through from the start: a const generic argument is truncated (Whitefoot#390)

This is one of the two 10-12 holes the item names. I use it to walk once, from the start, through "one known hole, and whether each approach catches it"; every row of the table in section 5 was judged
this way.

**What it was like then.** Before the fix (the commit before merge commit `9bc6bb7ed`), main accepted the program below (the original text in the PR body; now the conformance
case `const1-neg-argument-outside-parameter-range-read`):

```wf
fn pick<const n: u8>() -> r: u8 pure contract {
  requires n > 200_u8;
} {
  let block = array_filled::<u8, 800>(value: 7_u8);
  let wide = cvt::<u8, u64>(n);
  let at = 1000_u64 - wide;
  return block[at];
}

fn main() -> status: ExitStatus pure {
  let v = pick::<300>();
  return exit_status(code: v);
}
```

- The checker reasons with `n = 300`: `300 > 200` holds; `at = 1000 - 300 = 700 < 800`, the subscript is proved, and no bounds check is emitted.
- The generated code truncates 300 to u8, so at run time `n = 44`, `at = 956`, and the read goes outside the 800-element array.
- The measurement recorded in the PR: a variant of this program exits on main with code 8 or 136, where it should be 7; the control (argument 250) exits 7. It did not crash, and there was no
  report of any kind; it just read other memory.

**What kind of error this is.** A hole in the specification. I checked: before the fix, the specification's `CONST-1` had no sentence saying "the argument's value must fall within the range of the parameter's type"
(searching the CONST-1 section of `git show 9bc6bb7ed^1:spec/kernel-spec.md` finds no "closed range"); the fix added this sentence to `CONST-1`.
The checker did not violate any written rule at the time. It is also a case of "two places each computing their own": the `n` the checker held was 300, the one the backend held was 44, and nothing
compared the two numbers.

**How it was found at the time.** The status board's words are "found by chance while reading code or writing new cases". It did not come out of running any check.

**Does runtime comparison catch it.** Two questions.

- Can it be generated: yes, as long as the generator, when picking numbers for const arguments, does not unwittingly pick only numbers within the parameter type's range. Biasing integer values toward boundaries (0, 1, 255, 256,
  2^16, 2^64-1) is the usual practice for this kind of generator. Inferred, not done.
- Can it be seen when run: that depends on what is used as the referee:
  - The sanitizer build now in the gate (`whitefootc --sanitize address`): not necessarily. AddressSanitizer reports only when an access falls into a "red zone" it laid out;
    here the access is 156 bytes out of bounds, and where it lands depends on what sits next to it on the stack. The measurement recorded in the PR is that it quietly read another value. Unverified. To confirm: in CI, build the compiler at
    `9bc6bb7ed^1`, compile this case with `--sanitize address` and run it; an ASan report counts as "the existing referee is enough"; an exit code other than 7 with no
    report counts as "not enough".
  - A build that keeps every proved condition as a runtime check (none exists now; `whitefootc`'s options have only `--sanitize`): certainly. On entering `pick` it recomputes
    `requires n > 200_u8`; at run time `n = 44`, and it reports on the spot. It does not look at how memory happens to be laid out; it looks at whether a sentence the checker called "proved" is really true at run time.

**Does a small verifier catch it.** No, with certainty. The certificate the engine leaves for `at < 800` is "`n > 200` (from requires), `at = 1000 - n`,
so `at <= 799`", and every step is right. The error is before the derivation starts: the fact that `n` "is 300" in this instance does not match run time, and this fact is not in the certificate.

**Does adversarial review catch it.** Yes, but it cannot be called "certain". At the time it was met by someone reading the code. A reviewer with a checklist (with an item "every place where a compile-time
integer enters a fixed-width type") would find it here.

**Does the Lean proof catch it.** Not now: generics are not in the model's fragment (`proof/coverage.txt` says `generics: outside the first
phase`). Once generics enter the fragment, yes, and it gets stuck while writing the model: the model must say "what the value of `const n: u8` is in the instance", and a value of 300 is not
a u8.

## 5. 27 fixed defects as test questions

Sources: the candidates in the status board item "publish the list of soundness defects" (the 19 PRs confirmed for inclusion plus the 6 awaiting re-review; of these, #270 reads as having no soundness defect,
so it is removed), plus the join fix in #414, the one place in the unmerged #523, plus one place that the Lean proof study found while reading the rules and measured in CI
(`research/investigations/spec-proof/DESIGN.md` "Incidental findings" item 6, called "item 6" below). Each PR's witness program, where it was fixed, and how it was found
were extracted from the PRs by three subagents; the table is in `process.md`. The classification and judgments below are mine.

Three kinds of error, one example each:

- **Holes in the specification (12)**: the checker did what the specification says, and the specification lacked a sentence. Examples: #390 in the previous section; #414 (an inner `if` has an arm with an early `return`, and the specification's definition of
  "the value after the join" makes the path through the outer `else` count as impossible; `251_u8 + v` was accepted, and in CI it ran and wrapped to 0).
- **The checker not doing what the specification says (13)**: the specification already required rejection. Example: #145, after `let wr = &rows[k]; set k = 1_u64; fill(x: wr);` it reads
  `rows[k][2_u64]`. Spec `REF-1` says a reference records the subscript value at formation, but the checker looked up facts by the name `k`.
- **Wrong backend generation (2)**: the checker judged right. Example: #275, `ignore(part: &p.inner); grow(cell: &p, …)` were put in the same group that may run in parallel,
  and `grow` may release that storage first.

A second classification by "which step of the checker the error is in"; this is the key to comparing the three approaches:

| Which step the error is in | Which ones | Example |
| --- | --- | --- |
| A fact should have been invalidated and was not | #84 #121 #145 #147 #271 #281 #282 | `let inside = x < 4_u64; let w = &inside; set w^ = True(); if inside { table^[x] }` (#282) |
| Two different things were treated as the same one | #135 #302 #369 #380 | Ranges whose endpoints are not literals shared one key `CaptureId::Unknown`, and a range sliced later replaced one sliced earlier (#135) |
| The obligation was not raised at all, or was raised on the wrong object | #118 #173 #445 #523, half of #380 | `ensures` was checked only at an explicit `return`, not at the error exit of `propagate` (#173) |
| The checker and the backend each computed their own | #390 #342 #437 | The checker, the IR and the emitter each had their own copy of "which types' release actions are non-empty", and the checker's copy missed `KeySet` (#437) |
| Other judgments (effects, aliasing, reference validity, placement) | #348 #358 #359 #373 #411, item 6 | The same object passed to two reference parameters; the call site compared only the effect rows, not what the contracts read (#373) |
| The definition of join | #414 | See above |
| Lowering | #275 #331 | See above |

**Not one error is in "some step of the derivation computed wrong"** (closure, transitivity, summation and the like).

The result of replaying each one (how each was judged is at the end of `process.md`):

| Approach | How many of the 27 it catches | Notes |
| --- | --- | --- |
| Runtime comparison, referee is the build that keeps checks | Clearly, 11: #84 #121 #145 #271 #281 #282 #342 #373 #390 #411 #414; 12 that depend on how wide the generator is; 4 with nothing to observe at run time (#358 #359 #445, and #437's witness) | Examples of "depends on how wide the generator is": #135 needs two ranges whose endpoints are not literals, with an unrelated range between them; #173 needs a function that writes a reference parameter and only afterwards does `propagate` |
| Runtime comparison, referee is only the existing sanitizers | Of the 11 above, probably only #121 #342 #411, plus some variants of #145 and #373 | #414 is a silent wrap and certainly not reported; #390 most likely not reported; none of those with "a false postcondition but no memory touched" are reported. Unverified |
| Small verifier | 0 | The closest is #414, but there the specification's rule was wrong, and a verifier checking against the same rule would pass it |
| Adversarial review | What was actually found at the time: 12 written as review or audit, 5 met while implementing a feature, 10 with no origin written | Not one was found by running. The sanitizer job entered the gate only on 10-21 |
| Lean proof (today's fragment) | 0 | The witnesses of all 27 use things outside the fragment; item 6 was found by "reading the rules in order to model them" |

There are two things to be careful about with this table. First, it is a replay after the fact: asking of a known hole "does some approach catch it" is always optimistic. Second, of the 27, only 8 were actually observed
going wrong at run time (#121, #145, #147, #342, #373, #390, #414, item 6); for the rest, only "main's `--check` accepted
it" was confirmed. So most of the column "runtime comparison catches it" is inferred.

## 6. The three approaches one by one, and the division of labour with the Lean proof

First, what already exists (all checked):

- About 3150 conformance cases, of which about 1615 must be rejected, 688 must be accepted, and 853 must compile and run to a given exit code.
- The sanitizer job (`.github/workflows/sanitizers.yml`, `design/compiler/sanitized-builds.md`): runs the corpus programs and those 853 cases once each under
  AddressSanitizer and ThreadSanitizer, and once more with `--par`. It runs only programs people wrote, which should be right anyway; there is no generator.
  The design node itself states two blind spots: UBSan covers only the C runtime, and the emitted LLVM does not go through it; ASan cannot see out-of-bounds accesses inside mappings the runtime splits itself.
- The Lean proof (`proof/`): it proves "a program the model's checker accepts cannot reach, on the model's interpreter, a state the specification gives no meaning to". The model is the specification's
  rules written sentence by sentence, not the Rust checker. The fragment is now integers, local bindings, `if`, `loop`, function calls with requires/ensures, and local
  invariants; structs, references, windows, counted loops, `match`, generics, effects and parallelism are all outside ("sealed type invariants" is a component proved separately, aimed at
  the rules of the #484 revision, and not yet connected to the fragment that can run programs). Of about 3150 cases, 208 are in the fragment.
- The only link between the Rust checker and the Lean model: on these 208 cases, both sides give the same verdict.

### Runtime comparison

- **What it is.** Two parts: one that makes programs (the generator) and one that looks at the results (the referee).
- **Which kind it can catch.** All three kinds, including wrong backend generation, which no other approach catches; provided the accepted program really runs down the path that goes wrong,
  and the referee can see it. It cannot catch: what has no runtime manifestation (only violating readonly and the like), or shapes the generator would not think to make.
- **Cost.** Mostly not in the generator but in the referee. The "build that keeps all runtime checks" the item speaks of does not exist; a new kind of lowering is needed: each proved obligation emits a runtime check
  in place. There is a ready handle: the checker already makes a record for each obligation (`ObligationRecord` in `compiler/src/semantic/obligations.rs`:
  subscripts, exact integer operations, conversions, call requires, loop invariants, local invariants, ensures, disjointness), and the contract clauses themselves
  can be executed (integer comparisons, lengths, bounded `forall`). The hard parts are `entry(...)` (needs a snapshot at entry), whether a reference is still valid (needs shadow state), and
  whether two reference arguments overlap. This is a thing of several thousand lines that will have its own errors. Its errors go in the direction of false reports (a check emitted wrong); it does not hide holes.
- **What counts as failure.** Set a measurable threshold before starting work: take the commit before the fix of each of the 11 "clearly catchable" defects, and see how many of them the generator plus referee
  rediscovers within a fixed CI time. If it does not find more than half, the generator or referee is not good enough; stop. The item's suggestion, "rediscover those two from 10-12" (#390,
  #373), is the minimum version of this threshold.
- **There is a much cheaper variant that does not run programs.** 25 of the 27 are "a program that should be rejected was accepted". If the programs made are **to be rejected by construction**
  (first establish a fact, then make it false by some path, then use it), the referee is just the verdict of `whitefootc --check`: if it is accepted, it is a hole. The two
  candidates in section 2 and the 13 programs in `process.md` are all of this kind. This variant cannot catch backend errors, and it covers only the "paths" someone has listed in the table; but it needs no
  new build and can run today. Option one in section 8 is this.

- **How to design the generator, and which known holes it can and cannot make.** Three layers; the first two need only `--check`, and only the third needs a referee that can run:
  1. Programs to be rejected by construction, multiplying three axes: the kind of fact (branch guard, comparison remembered by a Bool binding, window length, callee postcondition, range
     clause, subscript recorded at reference formation) × the path that makes it false (the rows of the table above) × the place it is used (subscript, exact operation, call requires).
     This makes the whole family "a fact should have been invalidated and was not" (#84 #121 #145 #147 #271 #281 #282); changing the axes to "reference × the event that invalidates it × types with release actions"
     makes #411 #437.
  2. Small changes to the existing 3000-plus cases: pass the same reference twice (#373), replace integers with boundary values (#390), delete a requires (#118 #380),
     wrap a piece of code in an `if` with an early exit (#414), insert a statement between the guard and the use.
  3. Randomly generate control flow plus small integer arithmetic, paired with a referee that can run or with the Lean model. Each function must carry a range clause so that it participates in the range judgment; otherwise the #414 kind
     cannot be reached.
  What it cannot make are those that need a whole scenario to trigger: #331 (spawn in a wait function plus a statement group that may run in parallel), #523 (rebinding a binding to a local value in an atomic block),
  #135 (two ranges whose endpoints are not literals, with an unrelated range between them), #342 and #369 (some rare form that itself should not be allowed). Unless someone
  lists these scenarios as axes, random generation has little chance of hitting them. These are the 12 "depends on how wide the generator is" in the table.

### Small verifier

- **What it is.** Each time the proof engine proves an obligation, it leaves a certificate of "which facts were used, and how they were added up"; another small program checks the certificate.
- **What the 2026-08-10 study itself said** (`research/investigations/proof-certificate-architecture/PACKET.md`, checked):
  a verifier that checks only algebraic steps (it calls this Envelope A): "A forged source fact or omitted kill passes", so it cannot serve as a safety boundary; the only one not ruled out
  is "a trusted extractor turns the checked program into a proof stream, and the verifier checks over that" (Envelope B), and it says "The extractor remains in the
  safety TCB". The first three steps it recommended (a complete derivation ledger, an authorization list, `EntailmentApprovedProgram`) did not land; the name cannot be found in main.
  What landed is a different, similar thing: the obligation records mentioned above (`design/compiler/acceptance-records.md`).
- **Reworked against today's specification and today's defect record.** Three points have changed:
  1. There are now two provers. The range judgment came after 08-10. Its proof is "versions, join choices, at most 256 instances", not a path on a difference-constraint graph,
     and it needs another kind of certificate.
  2. The authorization to omit checks is not only for subscripts: there are also the no-wrap flags of exact operations, call sites not checking requires, the `noalias` given by `EFF-5`, the parallel permission given by `PAR-1`/`PAR-2`,
     the `unreachable` default arm of an enum-tag `switch`, and bare float-to-integer conversion (`design/compiler/backend-facts.md`).
     Among them, aliasing, parallelism and release are not given by the proof engine at all.
  3. The table in the previous section: not one recorded defect is in the "derivation steps". The two largest families, a fact should have been invalidated and was not, and two things treated as the same one, fall exactly
     in the extractor. If the extractor reuses the engine's code that computes kill events, these errors carry over unchanged; if it is written again separately, it must redo place resolution, overlap judgment and reference
     capture in full (things at the level of `semantic/places.rs` plus `entailment/flow/events.rs`). That is not small, and it amounts to writing the hardest part
     twice.
- **Against the item's stop gates.** The first gate is "the verifier contains no complete closure computation, and no longer walks source semantics". The defect record shows the errors lie exactly in
  "walking source semantics". So: a verifier that keeps this gate cannot see these errors; a verifier that can see these errors cannot pass this gate. By the item's wording, this counts as
  the conclusion "keep a single engine".
- **Which kind it can catch.** Only errors of the derivation search itself (closure, summation, join coverage computed wrong). To date, no error of this kind is recorded.
- **Cost.** Two certificate formats, an extractor, a verifier, plus the three budgets the item requires to be frozen first: compile time, peak memory, certificate size.
- **What counts as failure.** Any one of the item's five gates not met. I add one that can be checked before starting work: replay the defect record at the time; if it catches none, do not start.
  Today's replay result is 0.

### Adversarial review

- **What it is.** Read the specification and the implementation, looking specifically for programs that are accepted yet will go wrong. To date, it is what has always found the holes (the completion reviews of the PRs, independent
  adversarial reviews, planned audits).
- **Which kind it can catch.** Any kind the reader thinks of; all three kinds have precedents. What it cannot catch cannot be stated, and that is its problem: when it is done, there is no "how far it got".
- **A method with "how far it got", tried this time: review by matrix.** Fix one kind of fact, list every path in the language that can change it, one per row; fill three cells in each row:
  which line of the checker handles this path, whether there is a rejection case of the same shape, and the conclusion. Example: the fact is "the range walk knows the value of the local integer `v`"; the paths are direct `set`,
  write through a reference, a call writing through `&v`, `swap`, whole replacement, a write later in a loop, `return`/`break`/`continue` bypassing the join, a write in an atomic block,
  a `give` join, rebinding a reference…, 25 rows in all. The filled table is the measure of coverage: 10 rows handled and with cases, 13 rows handled without cases, 2 rows not handled (section 2).
- **Cost.** This one table: one opus subagent, about 290,000 tokens, 17 minutes, plus my re-check of the two candidates. Before this I had read three pieces piecemeal myself (the numeric
  conversion backend, 630 lines; the invalidation of Bool comparison origins; the ranges of parallel loop splitting), and found nothing. Reading without a table yields very little on this codebase.
- **What counts as failure.** Three measurable things: the share of candidates confirmed in CI is too low (for example fewer than one in three), which means inferences from reading code are unreliable; two
  reviewers with no contact fill a table for the same set of "fact × path" and too many cells disagree, which means it is not repeatable; several tables in a row with no confirmed hole, which means
  this area is clean or the table was listed wrong. The first can be measured now: confirm the two candidates in section 2.

### What each covers, versus the Lean proof, and where the boundary is

My view:

- **The Lean proof covers "whether the rules are right"**: if the checker did exactly what the specification says, would accepted programs be safe. The 12 specification holes among the 27 belong to it; once the fragment extends to
  references, windows and ranges, holes of this kind will get stuck while writing the model or the proof (item 6 was found this way, and the #390 section also reasoned through it once).
- **This study covers "whether the compiler did what the rules say"**: the 13 where the checker did not, and the 2 wrong backend generations. By design Lean does not touch these; its design document says
  whether the compiler implements the rules "stays with the conformance cases and the sanitizer runs".
- **Three places on the boundary need to be made clear:**
  1. Where the specification prescribes an algorithm in words. `RANGE-2` writes the state model of the range walk into the specification ("a construction yields … new storage of its
     own", "A binding is a location once a reference to it is formed"). Candidate one sits exactly between these two sentences: is whole assignment "the name switches to another piece of
     storage" or "a write in place"? Whether it counts as a specification hole or a checker error depends on how these two sentences are read. Both sides must look at places like this.
  2. The differential comparison between the Lean model and the Rust checker. The model can read programs, judge and run (`proof/Conformance.lean`), so generated in-fragment
     programs can be given to both sides at once: if Rust accepts and the model rejects, either the model is too strict or the checker has a hole; the model's interpreter can also serve as a ready runtime referee.
     The Lean proof study cited a precedent (Cedar's Lean model running differential tests against the Rust implementation). This bridge belongs here, but each disagreement must be judged by both sides together.
     Today its scope is the fragment, and none of the 27 is in it; its value grows with the fragment.
  3. The backend takes proofs as permission (`noalias`, no-wrap flags, parallelism). Whether it is right for the specification to say "the implementation may do this" is Lean's side; whether the backend
     acts only within the permission is this side, and at present only the sanitizer job watches it.

## 7. The small samples done this time

1. **The replay of 27 fixed defects** (section 5). On paper; all three approaches went through it.
2. **One matrix of adversarial review**, aimed at the range judgment on main (sections 2 and 6). Output: two candidate holes (unverified), and 13
   rejection cases that can be added directly.
3. **Three piecemeal readings**: the numeric conversion backend checked cell by cell against the `OP-6` table (integer-to-integer conversion, the maximum-value collision in integer-to-float, NaN,
   infinity, negative zero and out-of-range in float-to-integer); whether the comparison origin of a Bool binding is invalidated after the various writes (the #282 family); `--par` loop splitting meeting a range whose upper bound is less than its lower bound.
   All match the specification; nothing found.
4. **The inferred risk in the item**: after checking, already closed (section 3).

Not done: no program was run; the generator is only designed, not implemented; no sample was made for the small verifier, only the paper replay.

## 8. For the owner to decide

**One: first confirm the two candidates in section 2.** This is not choosing an approach; these are two holes that may still be open. What to run is in that section. If confirmed, their fixes
(whether whole assignment in the walk is a write in place or a switch of storage; how a reference coming out of a join is represented) touch the text of `RANGE-2`, which is the owner's matter.

**Two: choose an approach.** Three options, which can be stacked:

- **Option one: search for counterexamples by matrix, with `--check` as the referee.** First a reviewer (an agent) lists by "one kind of fact × every path that can change it" and fills the table against the code,
  as this time; then change the two axes of the table to mechanical enumeration from the language's own lists (statement forms, expressions that can form a reference, prelude operations whose effect row carries `writes`,
  the various edges that leave a block, types with release actions), with programs assembled automatically, all expected to be rejected; each one accepted is a hole.
  Cost: one table about 300,000 tokens; the mechanical enumeration is a small generator script, which must go into CI. It cannot catch backend errors, nor shapes outside the table.
  Example: what #437 missed at the time was `KeySet`. If the axis "types with release actions" were enumerated from the prelude's declarations rather than written by hand, `KeySet`
  would be in it automatically.
- **Option two: real runtime comparison.** Build the build that keeps checks (or an interpreter that directly interprets the checked program), plus a random generator. The only approach that can catch
  backend errors and "shapes no one thought of". The cost is several thousand lines of new code and its own maintenance; I suggest setting the threshold from section 6 (how many of the 11 it rediscovers) first,
  then starting work. It can go in two steps: first just run the existing 853 runnable cases and the corpus under the build that keeps checks (no generator). This step alone answers "is any of the existing
  programs running on a false fact".
  **It meets a sentence of the specification, and the owner must decide how to read it:** `OP-4` says "A discharged subscript reads or writes with no runtime bounds check in
  every build mode", and `SCOPE-2` says "no operation receives an implicit runtime fallback". Making "keep checks" a kind of compiler
  build conflicts with these two sentences literally.
  Two ways that do not conflict: instrumentation that, like `--sanitize`, only adds reports and does not change the program's result (the decision in `design/compiler/sanitized-builds.md`,
  "changes no acceptance, lowering choice or verdict, only instrumentation", is the precedent); or make it an interpreter that directly executes the checked
  program, checking every step, never going through lowering and the backend at all. Then its results can also be compared with the compiled program, which looks at the backend along the way.
- **Option three: differential comparison between the Lean model and the Rust checker.** Smallest cost (generate in-fragment programs, have each side judge once, glue code), but today's scope
  is only the fragment. It fits connecting once the fragment includes references.
- **Small verifier: I lean toward not building it**, for the reasons in section 6: it catches none of the recorded defects, and making it catch them means it cannot pass its own first stop gate.
  This amounts to taking the item's exit "keep a single engine", and does not overturn what the owner has decided: what the owner decided on 10-12 was to move it under checker soundness as a candidate;
  what the architect recorded on 10-18 was "the owner did not choose it on the approach card, so it is not done"; there is no decision in the design tree choosing it. The consequence is that the entry in the defect list,
  "the authorization to omit checks is not independently verified and belongs to the trusted base", stays listed as it is. If runtime comparison later points to a real error in the derivation steps, look again.

My leaning: do option one now (it has already produced something, and its table is afterwards the design of option two's generator); put option two's first step (running the existing cases under the build that keeps
checks) later, and before doing it, see how many of option one's candidates are confirmed; option three follows the Lean fragment.

**Three: the defect list item.** If the candidates are confirmed, the "not fixed" section of "publish the list of soundness defects" should add these two; whether the 13 forms with no case guarding them
are added as conformance cases is decided along with it.

## 9. What is unverified, and what may be wrong

- Settled since: both candidates are accepted by main before the fix ([the range-walk references record](../range-walk-references/DESIGN.md)), so no other rule stops them first.
- The 11 "runtime comparison catches it" in the section 5 table were judged against a referee that does not yet exist and a generator that is not yet written.
- "The existing sanitizers do not report #390 and #414" is inferred; the way to confirm #390 is in section 4.
- The origins, witness programs and fix locations of the 27 defects were extracted by sonnet subagents from the PR bodies and diffs; I spot-checked #390 (the specification text before and after the fix) and
  #398, and did not check the rest one by one. #523 is not merged; its content is as on its branch.
- "Small verifier: 0" was judged against the two verifiers the 08-10 study describes; a scheme that also rewrites the extractor independently could catch part of the family "a fact should have been invalidated and was not",
  but that is no longer a "small" verifier.
