# move-on-copy: adversarial investigation

Owner question, 2026-08-08: should [OWN-1]'s prohibition on `move p` for copy
values be removed, so a generic body can spell its parameter's use?

Four independent lenses (doctrine, the T1-T4 spelling rule, compiler/corpus
footprint, alternatives), each attacked by a skeptic instructed to kill it, then
synthesized. The synthesis re-ran every load-bearing measurement itself rather
than accepting any position's table. Removed when the question is settled.

# OWN-1 `move`-on-copy: decision report

Repository at `eca2bec`, `git status --porcelain` → empty. Compiler binary `compiler/target/debug/whitefootc` verified current (`find compiler/src -name '*.rs' -newer compiler/target/debug/whitefootc` → nothing). My probes: `<scratch-root>/synth-own1/`. I re-ran every load-bearing measurement myself rather than accepting any position's table; where I did not, I say so.

---

## 1. WHAT IS MEASURED

### M1 — The brief's premise is textually wrong. The one-clause deletion does not do what the brief says.

`sed -n '258p' spec/kernel-spec-v0.23.md`, verbatim fragment:

> "**An affine place** rooted in a live own-mode binding **is consumed** exactly once by an explicit `move p`, by use as an own-place match scrutinee under [OWN-13], or by use as the direct bare affine `Result<T, E>` place operand of `propagate` under [ERR-3]."

"Consuming use" is defined once, and only for affine places. The next sentence — "After any consuming use, the whole binding rooting `p` is dead" — is bound by that antecedent. Deleting only the copy clause therefore leaves `move p` on a copy value **legal with no defined effect**, not "killing the binding, exactly as it does for affine types."

Two independent lenses (DOCTRINE §1, FOOTPRINT's closing note) reached this from different directions. It is the single most consequential correction in the whole investigation: **the amendment on the table is a rewrite of OWN-1's consumption sentence, not a deletion of a clause.** Everything below assumes the rewrite, because that is what was actually measured.

### M2 — The pincer is real, and it is not about `return`.

One generic body, five value-use shapes, all rejected at a copy instantiation and all accepted at an affine one. Byte offsets decoded; every error points **into the generic body**, not the call site.

| probe | body shape | at `i32` | at `box<i32>` |
|---|---|---|---|
| `pick_i32.wf` | `return move value;` | exit 1 `MoveOfCopy` @198–208 | `pick_box.wf` exit 0 |
| `pick_i32_bare.wf` | `return value;` | exit 1 `BareAffineUse` @198–203 | — |
| `call_arg.wf` | `return inner<T>(v: move v);` | exit 1 `MoveOfCopy` @117–123 | not measured |
| `ctor_arg2.wf` | `return Cell<T>(slot: move v);` | exit 1 `MoveOfCopy` @100–106 | — |
| `ctor_bare.wf` | `return Cell<T>(slot: v);` | exit 1 `BareAffineUse` | — |
| `take_i32.wf` | `return move c.slot;` | exit 1 `MoveOfCopy` @102–113 | `take_box.wf` exit 0, **2 frees** |
| `take_i32_bare.wf` | `return c.slot;` | exit 1 `BareAffineUse` @102–108 | also exit 1 (symbolic pass) |

The correct statement of the hole: **an unbounded generic body cannot use its own-mode parameter as a value in any position** — return, call argument, constructor argument, or field projection — at a copy instantiation. The brief and three of the four positions understated this as a `return` problem.

The **caller is not pinched**: `drop_it<i32>(value: a)` passes OWN-1 (`zerouse_bare.wf` fails only on `EFF-2`), because the caller writes the instantiation argument explicitly and therefore knows the class. The pincer is a body-only phenomenon, caused by `nominals.rs:99` `CheckedType::Generic(_) … => false` — the symbolic pass treats an unbounded `T` as affine.

### M3 — The `Int`/`Float` bound escape does not cover Bool, tag-only enums, or shared borrows.

`spec/kernel-spec-v0.23.md:425` (FN-2), verbatim: *"A generic type parameter's written contract bound is admitted only when it resolves to the prelude `Int` or `Float` marker."*

- `bound_bool2.wf` — `fn pick<Held: Int>` instantiated at `Bool` → exit 1, **`FN-3` TypeMismatch**.
- `unbound_bool2.wf` — `fn pick<Held>` at `Bool`, body `return move value;` → exit 1, **`OWN-1 MoveOfCopy` @59–69, in the body**.
- Bare body at unbounded `T` → `BareAffineUse`.

So for `Bool`, tag-only enums, and shared borrows there is **no legal generic body and no bound workaround at all**. This is stronger than any position stated; DOCTRINE treated `Held: Int` as *the* workaround.

### M4 — Where the bound does apply, it already delivers everything, including multi-use.

- `pick_bound.wf` (`fn pick<Held: Int>`, bare body) → exit 0.
- `take_bound.wf` (`fn take<T: Int>`, bare projection) → exit 0.
- `multi_bound_bare.wf` (`fn duplicate<T: Int>` using `value` **bare twice**) → exit 0.
- That exact program ships: `tests/programs/generic_nominals.wf:23-25`.

### M5 — Measured demand for the fix, in real programs: zero.

```
grep -ohP 'fn\s+[a-z_][a-z0-9_]*<[^>(]*>' tests/programs/*.wf | sort | uniq -c
```
→ 9 generic functions, **every one** `T: Int`, `T: Float`, `T: Int, const n`, or `const n: u64`. Zero unbounded type parameters.

```
grep -ohP 'fn\s+[a-z_][a-z0-9_]*<[^>(]*>' tests/conformance/cases/*.wf | sort | uniq -c
```
→ 4 unbounded (`pick<T>` ×2, `preserve<T>`, `poly<T>`), all inside **negative** cases for FN-2/FN-6. Three instantiate at affine types (`Held`, `slice`); `fn6-neg-polymorphic-recursion.wf:4` writes bare `return x;` at `poly<i32>` and its manifest row is `"status": "pending"`.

```
find tests/programs tests/codegen research -name '*.wf' | wc -l          → 139
grep -rlP 'fn\s+[a-z_][a-z0-9_]*<[A-Z]' research --include='*.wf' | wc -l → 0
```

### M6 — Blast radius in conformance: exactly three cases, all deliberate pins of this clause.

Parallel sweep of all 403 cases against the unpatched binary, collecting `MoveOfCopy`:

```
find tests/conformance/cases -name '*.wf' | xargs -P 8 -I{} sh -c \
  'out=$("$B" "$1" 2>&1); case "$out" in *MoveOfCopy*) echo "MOC $(basename "$1")";; esac' _ {}
```
→ `own1-neg-move-of-copy`, `own1-neg-match-move-copy`, `own1-neg-index-move-copy-offset`. Nothing else. The manifest carries 14 rows with `"rule": "OWN-1"`; 5 further cases reject with `BareAffineUse` and are untouched.

I read all three. Each is a one-line negative pin of the clause under debate — `let dup = move flag;`, `match move state`, `items[move offset]`.

### M7 — A copy projection of an affine root is inside the deleted set. Measured today.

`struct Holder { node: box<i32>; tag: u64; }`

- `leak_move.wf` — `let t = move h.tag;` → exit 1, `OWN-1 MoveOfCopy` @176–186.
- `leak_bare.wf` — `let t = h.tag;` → exit 0, and `grep -o 'call void @free' leak_bare.ll | wc -l` → **1** (the box is freed by `h`'s scope-end drop).

This shape is neither a whole copy binding nor a type-parameter place. It is a third category the FOOTPRINT and ALTERNATIVES lenses did not enumerate, and it is where the change stops being about spelling.

### M8 — THE LEAK. Measured against patched compilers.

I did not build a patch. I audited and reused the trees at `<scratch-root>/skeptic/{probe,full}` and verified their contents with `diff -u` before trusting them.

**First, a correction to the record.** The tree labelled "patch A — the literal one-clause deletion" is *not* the literal deletion. `diff -u` shows it deletes the two rejection guards **and** widens both kill conditions from `if !copy {` to `if !copy || options.explicit_move {`. It supplies exactly the death semantics M1 shows the spec text does not. Every number in the FOOTPRINT report describing "patch A" describes deletion-plus-rewiring.

My probes, run against both binaries:

| probe | repo | probe/ (deletion + kill) | full/ (9 hunks, 3 files) |
|---|---|---|---|
| `pick_i32` | exit 1 `MoveOfCopy` | **exit 0** | exit 0 |
| `unbound_bool2` | exit 1 `MoveOfCopy` | exit 1 `EFF-2` only (OWN-1 passes) | same |
| `call_arg` | exit 1 `MoveOfCopy` | **exit 0** | — |
| `ctor_arg2` | exit 1 `MoveOfCopy` | **exit 0** | — |
| `leak_bare` | exit 0, **1 free** | exit 0, 1 free | exit 0, 1 free |
| `leak_move` | exit 1 `MoveOfCopy` | **exit 0, 0 frees** | exit 0, **1 free** |
| `take_box` | exit 0, 2 frees | exit 0, 2 frees | exit 0, 2 frees |
| `take_i32` | exit 1 `MoveOfCopy` | **exit 0, 0 frees** | exit 0, **1 free** |
| `multi_move` | exit 1 `UseAfterMove` | exit 1 `UseAfterMove` | **exit 1 `UseAfterMove`** |
| `multi_bare` | exit 1 `BareAffineUse` | exit 1 `BareAffineUse` | **exit 1 `BareAffineUse`** |

Read `take_box` / `take_i32` together. **One generic body**, `fn take<T>(c: own Cell<T>) -> own T { return move c.slot; }`, under the minimal rewiring: 2 frees at `box<i32>`, **0 frees at `i32`**. The `extra: box<i32>` field is never released at the copy instantiation. Mechanism, `compiler/src/semantic/check/expressions.rs:682`: `let residual_drops = if copy || fields.is_empty() { Vec::new() } else { … }` — `copy` is the class of the **projected** type (`:631`), so an `i32` field suppresses the residual cleanup of every sibling resource field while the widened kill condition at `:695` still kills the root.

The `full` tree remediates it (a `consumes = !copy || options.explicit_move` binding rewriting `access_fields`, `access_kind`, `residual_drops`, and `consume_root`), at a cost of 7 hunks in `expressions.rs` plus one each in `cleanup.rs` and `expressions/flat_storage.rs`.

### M9 — The change does not fix multi-use bodies. Under either patch.

`multi_move.wf` — `fn duplicate<T>(value: own T) -> own Pair<T> { return Pair<T>(left: move value, right: move value); }` at unbounded `T` → `OWN-1 UseAfterMove` under repo, `probe`, and `full` alike. Bare → `BareAffineUse` under all three. Mechanism: `nominals.rs:99` makes `T` affine to the symbolic pass, so the second use is a dead-binding use regardless of spelling. No amount of `move`-legalization reaches it.

### M10 — Two same-meaning spelling pairs already ship, verified by IR.

Derived from `tests/conformance/cases/err3-pos-propagation.wf` (`d`, `a` : `own Result<i64, Overflow>`, affine):

```
prop_bare  (propagate d)       exit 0
prop_move  (propagate move d)  exit 0   diff prop_bare.ll prop_move.ll  → status 0
match_move (match move a)      exit 0   diff prop_bare.ll match_move.ll → status 0
```

Byte-identical LLVM, both pairs. The spec states it deliberately, `:537` [ERR-3]: *"propagation consumes that place exactly once under [OWN-1] **without requiring a written `move`** … **An explicitly written `move p` retains its ordinary OWN-1 meaning.**"*

### M11 — Provenance: the ban is one unweighed sentence, and the project's own rule calls it provisional.

```
for f in spec/kernel-spec-v0.{0..6}.md; do grep -o 'on a copy value is a hard error' $f | wc -l; done
→ 0 0 0 0 0 0 1
```

`spec/derivation/derivation-ledger.md:172-173`, the entire recorded rationale: *"Companion FORM-1 discipline: `move` on a copy value is now a hard error (one spelling per meaning: copies are used bare)."* It rode in as a companion to the 2026-07-10 tag-only-enums-are-copy amendment, which was itself evidence-driven and measured.

[mcts_mem/whitefoot/ownership/copy-classification.alt/](https://github.com/mbbill/Whitefoot/tree/0f22b026b062c36ef2f8ce9c725ee4307e1fda64/mcts_mem/whitefoot/ownership/copy-classification.alt) contains exactly one file, `uniform-affine-enums.md` — about the classification, not the ban. **The ban has no recorded alternative and no recorded weighing.**

`docs/constitution.md:28` (R3): *"One way to say anything, and the survivor is chosen by evidence for P0+P1 among candidates, measured under W1 (weak writers). **Minimality-selected forms are PROVISIONAL.**"* And `derivation-ledger.md:169-171`, in the same amendment: *"The original affinity of Bool was minimality-selected (R3 provisional: uniform enum rule), not evidence-selected; this amendment is the evidence-driven correction."*

### M12 — The tree's spelling doctrine cuts against the *narrow* fix, not the broad one.

[mcts_mem/whitefoot/surface-form/spelling-rule.md:4](https://github.com/mbbill/Whitefoot/blob/0f22b026b062c36ef2f8ce9c725ee4307e1fda64/mcts_mem/whitefoot/surface-form/spelling-rule.md#L4), verbatim: *"The legality of a spelling depends only on its grammar class … **never on use-site context** …; **relief is all-or-nothing per class**, and a class that cannot be relieved wholly stays uniformly mandatory."*

[mcts_mem/whitefoot/surface-form/spelling-rule.alt/positional-relief.md:1-2](https://github.com/mbbill/Whitefoot/blob/0f22b026b062c36ef2f8ce9c725ee4307e1fda64/mcts_mem/whitefoot/surface-form/spelling-rule.alt/positional-relief.md#L1-L2): *"Relief was decided per position … One construct carried different legal spellings at different sites, and a class could be partially relieved."* Move: *"2026-08-07 replaced by [[spelling-rule]]."*

Item 5 of the live node — *"Every position is mandatory or forbidden; no element is optional"*, dated 2026-08-07, the newest spelling doctrine in the tree — is **falsified today** by M10.

### M13 — OWN-11's guard is untouched by the remediation, and its exemption clause stops describing reality.

`expressions.rs:653`: `if !copy && local.loop_depth < options.loop_depth`. The `full` patch's hunks land at 573/599/629/663/679/692/726 — `:653` is not among them.

`loopmove.wf` (`set total = move flag;` inside `loop @l`, `flag` declared outside): repo → exit 1 `MoveOfCopy`; `probe` → **exit 0**; `full` → **exit 0**. OWN-11 `:284` reads *"bindings declared outside `@l` may not be moved inside it **(copies exempt)**"* — an exemption for a class that would now die on use.

**Not measured:** whether a genuinely repeating loop admits the dead-binding read at iteration 2. I could not construct a two-iteration probe within the grammar (`GRAM-6`).

---

## 2. WHAT IS ARGUED (survived only as argument)

- **`move`'s recorded promise is visibility of consumption, not affinity.** `derivation-ledger.md:66`, verbatim: *"Explicit move serves W3/R4 in ordinary expression contexts; the closed OWN-13 match and v0.13 ERR-3 propagation contexts state their consumption structurally and therefore consume a direct bare affine own-rooted place **without hiding whether consumption occurs**."* No text anywhere says `move` may not mark copy consumption — but the GRAM-5 row does say *"explicit `move` makes OWN-1 **affine** consumption syntactically visible."* Both readings are in the ledger. **Argued, not settled.**
- **A `T: Copy` bound would work mechanically.** A `GenericCopy` discriminant beside `GenericInt`/`GenericFloat` at `nominals.rs:97` would make bare use and multi-use legal at every copy type. No such bound exists; **not measured**.
- **Option A′'s collateral on `geometry_vectors.wf:9-14`.** Reported by the ALTERNATIVES skeptic; I did not re-measure.
- **The index-place duplicate spelling under the full patch** (`items[0]` vs `move items[0]` byte-identical, root live in both). Reported by the FOOTPRINT skeptic; I did not reproduce it.
- **The repo unit suite is red at HEAD** (3 pre-existing failures). Reported by two skeptics; I did not run `cargo test`.

---

## 3. THE STRONGEST SURVIVING ARGUMENTS

### FOR the change

**R2 plus R3, on measured facts.** `constitution.md:27` (R2): *"A cut that harms AI codegen is a wrong cut. Simplicity is never a sufficient reason."* The pincer harms AI codegen at a general, measured shape (M2), with **no escape whatsoever** for `Bool`, tag-only enums, and shared borrows (M3). The rule causing it is one unweighed companion sentence with no recorded alternative (M11), which R3 classifies as PROVISIONAL — and the precedent for correcting exactly this kind of form is the same 2026-07-10 amendment that introduced it.

Supporting, measured: FORM-1 is not a bar. Two same-meaning spelling pairs already ship in accepted programs at own-mode affine places, one of them written into OWN-1's own companion rule ERR-3 (M10). And if anything is done, uniform relief is the doctrinally compliant shape; position-keyed relief is the form the project rejected on 2026-08-07 (M12).

### AGAINST the change

**R4, on measured facts.** `constitution.md:29`: *"Shift-left everything. Unrepresentable > check-time rejection with rule-citing diagnostics > runtime trap > (forbidden) silent corruption."*

Today, `move h.tag` — four extra characters on an integer field read — is a check-time rejection (M7). After the change it is an accepted operation that **destroys the whole struct**, and under the minimal rewiring it is an accepted operation that **leaks** the struct's resources: 1 free → 0 (M8). This is a drop-obligation change, not a spelling change, and CLAUDE.md's priority order puts it at **priority 2 (semantic correctness and required safety checks)** — above understandability, above evidence, above polish. It is remediable (the `full` tree remediates it), but the remediation is what makes the change a nine-hunk, three-file, four-rule edit rather than a clause deletion.

Supporting, measured: it does not pay for itself. Zero programs in `tests/programs`, `tests/codegen`, or `research` use an unbounded generic (M5), and the change does not reach multi-use bodies at all (M9).

---

## 4. DOES IT FIX THE GENERIC PINCER, OR RELOCATE IT?

**Both. It fixes one half cleanly, leaves a second half untouched, and converts a third into a drop-obligation problem.** Directly:

**Fixed (measured, M8).** Single-use generic bodies. `pick_i32`, `call_arg`, `ctor_arg2`, `take_i32` all go exit 1 → exit 0. This works at **every** copy type including `Bool`, where no bound workaround exists (M3). This is a genuine, complete fix for the brief's motivating shape.

**Not fixed (measured, M9).** Multi-use generic bodies. `duplicate<T>` using its parameter twice is `UseAfterMove` under repo, minimal patch, and full patch alike. The blocker is `nominals.rs:99`, not the OWN-1 clause. So "a generic that **returns** its own parameter" is fixed; "a generic that **uses** its own parameter" is not.

**Relocated (measured, M8).** At copy projections of affine roots, the pincer's rejection becomes whole-root destruction. Under the minimal rewiring that destruction is a **leak** (`take_i32` frees 0 while `take_box` frees 2 — one body, two instantiations, one of them wrong). Under the full rewiring it is correct, but it is a new destructive spelling available throughout ordinary monomorphic code: `move h.tag` now frees `h.node`.

The honest summary: the amendment converts *"no legal spelling inside generic bodies"* into *"a legal spelling inside generic bodies, plus a destructive spelling everywhere else."* Whether that trade is good is the owner's call. It is not a pure fix, and it is not a one-clause deletion (M1).

---

## 5. ALTERNATIVES, RANKED

Ranked by measured capability delivered per unit of measured cost. This is a ranking, not a recommendation.

### 1. D — Do nothing to the language; fix the diagnostic.
- **Fixes:** nothing. The pincer stands.
- **Cost:** one diagnostic variant. Today `places.rs:58` says *"write `move p` for the affine place"* and `:68` says *"use the copy place without `move`"* — a writer following either is sent to the other. A generic-aware message ("an own-mode parameter of unbounded type parameter `T` cannot be used as a value; bound `T` with `Int`/`Float`, or restructure") closes the loop with no spec change.
- **Fits:** `docs/current-plan.md:3` is `ACTIVE`; `:55-56` reads *"no spec change beyond the single ruled verdict correction."*
- **Why first:** measured demand is zero (M5), and every other option is still available later at the same cost.

### 2. C — Extend FN-2's bound vocabulary with a copy marker.
- **Fixes (argued, not measured):** single-use **and** multi-use bodies (M9's blocker is `Generic(_) => false`), at **every** copy type including the ones the `Int` bound cannot reach (M3). Strictly more powerful than the proposal.
- **Cost:** a spec amendment to FN-2 `:425`, where the vocabulary is closed verbatim to two prelude markers; a new `CheckedType` discriminant beside `GenericInt`/`GenericFloat`; substitution and bound-satisfaction rules. Touches **no** monomorphic code, **no** drop obligations, **no** conformance verdict.
- **Does not fix:** one body serving both a copy and an affine instantiation. `pick<Held>` still needs two functions.

### 3. A — Rewrite OWN-1's consumption sentence to cover copy places, and rewire drop obligations.
- **Fixes:** single-use bodies at every copy type; one body serves both classes.
- **Cost, measured:** 9 hunks in 3 compiler files (M8), plus `expressions.rs:653` still untouched (M13); 3 conformance verdicts flip (M6); OWN-1's consumption sentence rewritten, not deleted (M1); OWN-11's `(copies exempt)` and OWN-13's unqualified *"the binding dies"* both need reconciling; a new whole-struct destructor spelling in ordinary code (M8).
- **Does not fix:** multi-use (M9).
- **Residual defect (argued):** at index element places the full patch reportedly leaves `x` and `move x` byte-identical with the root live — a genuine FORM-1 duplicate spelling requiring a carve-out.

### 4. A′ — Narrow relief: legalize `move` only where the written type is a type parameter.
- **Ranked last on doctrine, not on cost.** It is `spelling-rule.alt/positional-relief.md`, replaced 2026-08-07 (M12), and it is exactly the shape `spelling-rule.md:4` forbids.
- **And it does not escape the projection problem anyway:** `move c.slot` in `take<T>` *is* a type-parameter place (M8, `take_i32`), so A′ inherits the leak/destruction question in full.
- Dominated by A on doctrine and by C on cost.

---

## 6. WHAT WOULD CHANGE THE ANSWER

- **One real program needing a single body at both a copy and an affine instantiation.** Today: zero across 139 programs and 403 conformance cases (M5). One such program in `wfgrep` or a codegen test moves **A above C**.
- **One program needing multi-use at an arbitrary copy type.** That moves **C above D immediately** and rules **A out as insufficient** (M9).
- **A three-valued class in the symbolic pass** — copy / affine / unknown at `nominals.rs:99`, rejecting only the uses that actually differ between classes. That would close the pincer with **no spelling change and no spec change** and would dominate everything on this list. Nobody built it; **not measured**.
- **Evidence that copy-projection destruction is wanted** (a program that must release a struct while reading an integer out of it). If the owner wants that capability, A's largest cost becomes a feature and the ranking inverts.
- **Confirmation of the index-place duplicate spelling under the full patch.** If real, A carries a FORM-1 defect it cannot discharge without a carve-out, dropping A below A′.
- **Any measurement showing the `full` remediation misses a drop obligation I did not probe** (arrays, buffers, nested projections, `set` targets). That would move A below D permanently, since a resource-safety regression is priority 2.

---

## 7. FOUND, THOUGH NOBODY ASKED

1. **OWN-13's text already contradicts the shipped compiler, today, independent of this proposal.** `:288` reads *"Matching a place of own mode moves it (the binding dies)"* with no class qualification, but the compiler exempts copy scrutinees (a tag-only enum can be matched twice, exit 0 — measured by the DOCTRINE skeptic). CLAUDE.md: *"A spec/compiler discrepancy stops the affected work for investigation."*
2. **The tree's newest spelling doctrine is falsified by shipped code.** `spelling-rule.md:5` (*"Every position is mandatory or forbidden; no element is optional"*, 2026-08-07) versus M10's two accepted, byte-identical pairs — one of them mandated by ERR-3 `:537`. Either the doctrine or the spec is wrong, and this is a standing inconsistency in the design memory that predates the proposal.
3. **The `Int`/`Float` bound restriction is the real constraint on generics, not the `move` clause.** Every generic function in `tests/programs` is bounded (M5), and FN-2 `:425` closes the vocabulary to two prelude markers. That is why the corpus has no unbounded generics at all — the pincer is a symptom of a narrow bound vocabulary, not the disease.
4. **The one conformance case that instantiates an unbounded generic at a copy type is unresolved.** `fn6-neg-polymorphic-recursion.wf:4` writes bare `return x;` at `poly<i32>`; its manifest row is `"status": "pending"`.
5. **A scratch artifact in circulation is mislabelled.** `<scratch-root>/skeptic/probe` is described in the FOOTPRINT report as "the literal one-clause deletion"; `diff -u` shows it also widens both kill conditions. Its conclusions stand, but its numbers measure something the brief's text does not propose.

---

## 8. WHERE THE LENSES DISAGREE

**The one genuine incompatibility is A versus A′.** ALTERNATIVES ranked the narrow, type-parameter-only relief **first**. DOCTRINE, SPELLING-RULE, and FOOTPRINT all independently landed on some version of "not the deletion as written; the coherent forms are a multi-edit consuming-`move` change or a bound extension."

**The evidence that separates them is decisive and dated:** [mcts_mem/whitefoot/surface-form/spelling-rule.alt/positional-relief.md](https://github.com/mbbill/Whitefoot/blob/0f22b026b062c36ef2f8ce9c725ee4307e1fda64/mcts_mem/whitefoot/surface-form/spelling-rule.alt/positional-relief.md), replaced by `spelling-rule` on 2026-08-07 — one day before the ALTERNATIVES report was written — describes A′ word for word. `CLAUDE.md` requires consulting *"the relevant live `mcts_mem/` node **and its rejected alternatives**"*; that folder was not opened. Independently, my `take_i32` measurement shows A′ does not even avoid the problem it was proposed to avoid. **A′ is refuted. I am not resurrecting it.**

**Positions killed by their skeptics, which I verified and am not resurrecting:**
- DOCTRINE §7(a), *"FORM-1 is violated today in the existence direction."* FORM-1 `:52` reads *"Unknown constructs are hard errors (conservative extension)"* — its answer to "this shape has no spelling" is **reject it**. The reading also trivializes the rule (OWN-11, OWN-5, and ERR-2 would all breach FORM-1). The absence of a writable generic body is a real defect, but it is an **R2** defect, not a FORM-1 defect. The "it's a wash" framing is unavailable.
- SPELLING-RULE's loan-sensitivity objection. `expressions.rs:667-671` selects `access_kind` by `copy`, not by `explicit_move`; no loan conflict arises.
- FOOTPRINT's "six-site change in two files." Its own artifacts are 9 hunks in 3 files, and `:653` is still unpatched (M13).
- The brief's premise that the death sentence "would then apply unchanged" (M1).

**A softer disagreement, which CLAUDE.md's own ordering resolves.** The three surviving lenses priced this change into three different slots: DOCTRINE at priority 3 (a readability loss valued at zero by R1/R5), SPELLING-RULE at priority 2 (a resource-safety regression), ALTERNATIVES at priority 5 (nothing needs it). These are not contradictory — they are three true statements about different costs. The project's priority order says the **priority-2 question must be answered first**, and it is answerable: the `full` tree answers it, at a measured cost of nine hunks across three files and four coupled rules. Only after that does goal discipline decide whether to do it at all — and there, the measured demand is zero.

**Where no evidence separates anything:** whether the pincer will bite in future work. "Zero programs need it today" is a measurement; "no program will need it" is a forecast, and nobody produced evidence either way.
