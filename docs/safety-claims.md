# Safety claims

What Whitefoot claims about a program the compiler accepts, what each claim rests on, what is not claimed, and what the claims trust. Every line can be checked against the [specification](../spec/kernel-spec.md) and the [conformance cases](../tests/conformance/manifest.jsonl) without taking the project's word for it.

All claims hold under one condition, stated by the specification's scope rule `SCOPE-3`: the trusted base ([below](#the-trusted-base)) is correct.

## How to read this page

Each claim below has a table. Its rows are specification rules; the columns say how the rule is checked.

| Column | Meaning |
|---|---|
| Rule | A rule of the specification, by its stable ID. |
| Contribution | What the rule adds to the claim, in a clause. |
| Rejecting cases | Conformance cases whose declared verdict is a rejection citing that rule. They show that an accepted program has cleared the rule. |
| Accepting cases | Cases that cite the rule and are declared accepted, or compiled and run to a declared exit status. They show that the rule admits the safe form and that the safe form executes. |
| Other evidence | For a rule that no source verdict can tell apart (such as a permission an implementation may or may not take), the `covered_by` key of the rule's annotation in the manifest, which says which tests check it. |

The cases are a readable sample: the manifest lists every case, and each case's `rules` field names the rules it exercises. A case named here can be opened at `tests/conformance/cases/<id>.wf`, or a directory of that name for a module-form case, and its `doc` field in the manifest says what it protects.

`make safety-claims` checks this page against the specification and the manifest on every `make static`. It fails when:

- a rule named here is not defined in the specification;
- a case named here is not in the manifest, no longer cites the rule, or no longer has the verdict its column promises;
- an "other evidence" key differs from the rule's annotation in the manifest;
- a claim has no rejecting case or no accepting case;
- a quotation under [Not claimed](#not-claimed) no longer occurs verbatim in the rule it cites;
- a `make` target named under [Reproducing](#reproducing) does not exist.

The check reads declared verdicts. That the compiler reaches each declared verdict is what the gate's conformance run decides, and `make conformance-run` repeats it.

## Claims

### No memory corruption

An accepted program does not read or write outside an object's storage, use a value after its owner released or moved it, release a value twice, or keep a reference to storage that was released or changed shape.

| Rule | Contribution | Rejecting cases | Accepting cases | Other evidence |
|---|---|---|---|---|
| `OP-4` | Every subscript carries the obligation `i < len`. The checker proves it or rejects the program; no bounds check is emitted. | `op4-neg-index-undischarged`, `op4-neg-counted-past-array-end`, `op4-neg-segments-index-past-count` | `op4-pos-index-discharged` | |
| `OWN-1` | Every value has one owner. A binding whose value was moved is dead, and a later use is rejected. | `own1-neg-index-atom-after-move` | `own1-pos-consume-once` | |
| `REF-2` | A reference is valid only until a write, move, release or window change reaches its path; using an invalid one is rejected. | `ref2-neg-paged-element-after-grow` | `ref2-pos-reference-used-before-sibling-move` | |
| `REF-3` | A reference never escapes the function that formed it: it cannot be returned or stored in an aggregate. | `own4-neg-return-local-borrow` | `ref3-pos-index-result` | |
| `LIV-1` | Every join agrees on which bindings are live, so each release at scope exit is unconditional and runs once. | `liv1-neg-branches-disagree-on-liveness` | `liv1-pos-loop-moves-and-restores-an-outer-binding` | |
| `STOR-3` | Releases are derived by the compiler and run exactly once on every edge that leaves the owner's scope. | | `stor3-pos-box-drop-region`, `continue-pos-cleanup` | |
| `OP-14` | A window is freed only after the checker has proved it empty, so no live element is freed. | `op14-neg-free-empty-nonempty-window` | `op14-pos-free-empty-empty-window` | |
| `STOR-6` | The address of every element access and allocation is proved representable on the selected target after source acceptance; no runtime guard stands in for the proof. | | | `compiler-target-layout-tests` |

The cases show that the checker rejects each unsafe shape it is declared to reject. They do not enumerate unsafe programs: a program the checker accepts by mistake is a soundness defect, not a failing case. Memory safety of the generated code and of the runtime rests on the trusted base.

### No data races

In an accepted program, no two contexts, overlapped statements or overlapped loop iterations access the same storage with at least one of them writing, except through an atomic statement on a shared object. This is a guarantee about data races; [Not claimed](#not-claimed) says what it leaves out.

| Rule | Contribution | Rejecting cases | Accepting cases | Other evidence |
|---|---|---|---|---|
| `CAP-1` | There is no capability category beyond ownership, loans, path overlap and effect rows, and the only concurrency construct is the spawn. | | | `compiler-ownership-model` |
| `WAIT-3` | A spawned context's footprint is the arguments moved or copied into it; a callee with a reference parameter cannot be spawned. | `wait3-neg-reference-parameter` | `wait3-pos-spawn-let` | |
| `SHARE-2` | The state of a shared object is reached only inside an atomic statement, and one handle cannot be a whole target twice. | `share-neg-target-whole-handle-twice` | `share-pos-handle-moved-in-block` | |
| `SHARE-3` | An atomic statement takes effect at one point, with exclusive access to the states of its targets. | | `share-pos-map-two-handles-one-entry` | |
| `EFF-5` | A call's declared effect row is applied to the actual paths; two paths of one call that are not proved disjoint, one of them written, are rejected. | `own12-neg-alias-uniq-args` | `own12-pos-distinct-uniq-args` | |
| `OWN-7` | Two paths overlap when one is a prefix of the other, and two indexed positions overlap unless proved distinct. | | `own7-pos-distinct-noverlap` | |
| `PAR-1` | Adjacent statements may overlap only when each one's writes are disjoint from the other's reads and writes. | | `par1-pos-a-view-argument-is-a-footprint-on-its-origin` | `compiler-permission-judgment` |
| `PAR-2` | Loop iterations may overlap only under the stated conditions on the loop body's footprint. | | | `compiler-permission-judgment` |

Whether a permitted overlap is taken is not observable (`PAR-1`), so a source verdict cannot tell it from sequential execution. The two permission rules are checked by compiler tests that run the same emitted program at several worker counts and compare the results, which the manifest annotation of each rule describes.

### No uninitialized reads

An accepted program does not read storage that holds no value. A window's slots outside its length cannot be named, a moved-from binding cannot be read until it is assigned again, and no operation leaves a hole in a window or an array.

| Rule | Contribution | Rejecting cases | Accepting cases | Other evidence |
|---|---|---|---|---|
| `WIN-1` | Only the first `len` slots of a window are initialized, and a subscript is bounded by `len`, never by capacity. | | `win1-pos-window-is-len-slots` | |
| `OP-4` | The subscript obligation is `i < len`, so a read of a slot below capacity but not below `len` is rejected, as is a read of a window formed empty. | `win1-neg-subscript-above-len`, `op4-neg-a-run-element-read-carries-its-obligation` | `op4-pos-index-discharged` | |
| `OP-13` | A window built by a construction function starts empty; its slots become readable only as they are written. | | `op13-pos-segments-empty-run`, `type9-pos-segments-fill-and-join` | |
| `WIN-3` | There is no take operation and no hole: a move out of a window slot or an array element is rejected. | `win3-neg-move-out-of-window-slot` | `win3-pos-overwrite-releases-affine` | |
| `OWN-1` | A dead binding cannot be read. | `own1-neg-index-atom-after-move` | `own1-pos-consume-once` | |
| `LIV-1` | At every join all predecessors agree on which bindings are live, so no path reads a binding another path moved. | `liv1-neg-branches-disagree-on-liveness` | `liv1-pos-loop-moves-and-restores-an-outer-binding` | |
| `OWN-11` | A loop body may not leave an outer binding dead at its backedge, so no iteration starts with a binding the previous one consumed. | `own11-neg-move-outer-in-loop` | `own11-pos-loop-inner-region` | |
| `SET-1` | Assignment revives a dead binding, and its premises are rechecked after the right-hand side. | | `win3-pos-affine-set-target-releases-old-value` | |

### No silent overflow

An accepted program computes no integer result outside its type unless the program names a mode (`+wrap`, `+sat`, `+checked`) that defines the result. A bare operation, a conversion or a constant expression whose result could leave the range is rejected before the program is built.

| Rule | Contribution | Rejecting cases | Accepting cases | Other evidence |
|---|---|---|---|---|
| `OP-1` | Every computation names one operation and mode from the operation table, so no operation has an implicit overflow behavior. | | `x-arith-wrapping-overflow-observed`, `x-arith-iadd-checked-overflow-err-arm-runs` | |
| `OP-2` | The bare `+ - * / %`, `ineg`, `iabs`, `ishl` and `ishr` carry an integer-domain obligation that must be discharged: no overflow, a nonzero divisor, an in-range shift. | `op2-neg-exact-absolute-minimum`, `op2-neg-left-shift-amount-unbounded` | `op2-pos-early-return-divisor-guard` | |
| `OP-6` | A conversion needs a proved domain; an unproved bare conversion is rejected, and the checked form returns a `Result`. | `op6-neg-unproved-domain`, `op6-neg-inexact-constant` | `op6-pos-cvt-checked` | |
| `OP-8` | The wrapping, saturating and checked forms have edge semantics closed as table data. | | `op2-pos-sat-mode`, `op8-pos-integer-width-edges-run` | |
| `CONST-1` | Constant expressions are evaluated exactly in the unsigned 64-bit domain; one that leaves it is rejected. | `const1-neg-eval-overflow` | `const1-pos-array-size` | |

### No runtime traps

An accepted program contains no abort, exception, unwinding edge, runtime check or fallback that a failed proof could reach. Every partial operation was proved to be within its domain before lowering, and a proof the checker cannot complete rejects the program instead.

| Rule | Contribution | Rejecting cases | Accepting cases | Other evidence |
|---|---|---|---|---|
| `SCOPE-2` | A program is accepted only if every proof-required partial operation is statically discharged; failure rejects compilation and no statement can request a runtime fallback. | | `scope2-pos-accepted` | |
| `EFF-4` | Accepted source has no writer-reachable abort effect, exception, unwinding edge or hidden runtime proof fallback. | | | `compiler-source-proof-erasure` |
| `ERR-4` | Expected failures are `Result` values; an unproved function, operation-domain, bounds, layout or address obligation is a source rejection. | | `err4-pos-recoverable-value`, `err4-checked-domain-violation` | |
| `OWN-8` | The checker rejects any program it cannot prove conformant. | | | `policy` |
| `ENT-1` | The derivation system that discharges obligations is closed, deterministic and terminating, so acceptance depends on no timeout, machine speed or search budget. | | `ent1-pos-instantiation-judged-at-value` | |
| `OP-4` | Index: the bounds obligation is discharged or the program is rejected. | `op4-neg-index-undischarged` | `op4-pos-index-discharged` | |
| `OP-2` | Integer arithmetic, division and shifts: the domain obligation is discharged or the program is rejected. | `op2-neg-exact-absolute-minimum` | `op2-pos-early-return-divisor-guard` | |
| `OP-6` | Conversion: the domain obligation is discharged or the program is rejected. | `op6-neg-unproved-domain` | `op6-pos-cvt-checked` | |

## Not claimed

The specification states these limits itself. Each quotation is verbatim from the rule cited, and the check above fails if the rule changes.

| Not claimed | Specification | Rule |
|---|---|---|
| Freedom from race conditions in general | “Its data-race guarantee is subject to [SCOPE-3]; it does not exclude general race conditions.” | `CAP-1` |
| Termination | “The source spelling `pure` excludes state reads and state writes; it does not promise termination.” | `EFF-3` |
| Freedom from deadlock | “An execution in which every context that has not completed waits for an atomic statement whose guard is false or for another context, and no host operation is outstanding, takes no further step and does not complete; an implementation may stop it with a report, which is not a program outcome [SCOPE-3].” | `WAIT-2` |
| Resource availability | “The source outcome model leaves resource availability outside it: heap exhaustion, stack exhaustion, operating-system quotas, and runtime-start resources may stop execution without a Whitefoot value, status, or cleanup guarantee.” | `SCOPE-3` |
| Freedom from undefined behavior if the trusted base is wrong | “Accepted programs have no undefined behavior, conditional on the declared trusted computing base: compiler, checker, linked function definitions, runtime, allocator, and OS.” | `SCOPE-3` |

A contract states what its author wrote down. Nothing on this page says an accepted program computes what its author meant.

## The trusted base

An accepted program is safe if the following are correct. They are not verified by Whitefoot's checker.

| Part | Trusted for | Size |
|---|---|---|
| The Whitefoot compiler, in safe Rust | Parsing and checking, so that it accepts only what the specification accepts; lowering and emitting LLVM IR that means what the checked program means. | 221,514 lines in 261 files |
| The runtime, in C and LLVM IR, linked into every program | The scheduler, the completion runtime (file, socket and timer operations), the allocator, the shared-object and concurrent-map implementations, and the stack and heap limits. | 28,549 lines of C and headers in 39 files; 1,036 lines of LLVM IR in 1 file |
| The Rust crates the compiler depends on | `sha2` and what it pulls in, as listed in `compiler/Cargo.lock`. | Not counted |
| LLVM 22.1.8: clang, lld and the optimizer | Compiling the runtime's C, optimizing and generating code for the emitted IR, and linking. | 5,938,981 lines (LLVM 3,954,214; clang 1,873,115; lld 111,652) |
| The C library | The functions the runtime calls. | Not counted |
| The operating system | The kernel interfaces the runtime uses, such as threads, memory mapping, signals, `io_uring` on Linux and I/O completion ports on Windows. | Not counted |

How the counted figures are made, so that anyone can repeat them:

- The compiler count is every `.rs` file under `compiler/src` plus `compiler/build.rs`, minus test sources (a file in a `tests` directory, or named `tests.rs`, `*_test.rs` or `*_tests.rs`) and minus inline `#[cfg(test)] mod` blocks. Lines are physical lines, blank and comment lines included.
- The runtime count is the files the compiler stages for every link: the `unit(...)` entries in `compiler/src/bin/whitefootc.rs`. It counts the units of every platform. A program on one platform links the shared units and that platform's own, so its runtime is smaller than the total.
- The LLVM count is the lines of the `.cpp`, `.h`, `.c`, `.def`, `.inc` and `.td` files under `llvm/lib`, `llvm/include`, `clang/lib`, `clang/include` and `lld` in the `llvm-project-22.1.8.src.tar.xz` source release, leaving out `test` and `unittests` directories: `find llvm/lib llvm/include -type f \( -name '*.cpp' -o -name '*.h' -o -name '*.c' -o -name '*.def' -o -name '*.inc' -o -name '*.td' \) -not -path '*/test/*' -not -path '*/unittests/*' -print0 | xargs -0 cat | wc -l`, and the same for the other directories. It counts the whole of LLVM, clang and lld, more than any one program exercises. It was run once, on 2026-10-21.
- `python3 .github/check-safety-claims.py --tcb` prints these counts for the checkout, and the list of system headers the runtime includes (49 distinct headers at the time of writing, covering POSIX, Linux, macOS and Windows).
- The figures above were taken from `main` at commit `d1f2a19d2`. They are a snapshot and the check does not hold them to the checkout; run the command for the present figures.

Parts marked "Not counted" are other projects, or the machine's own, and their size is not a property of this repository: the version of the C library and the kernel is whatever the host provides, and the crates are resolved by `compiler/Cargo.lock`. The LLVM release is pinned in `.github/toolchain/llvm-assets.json`.

## Reproducing

Prerequisites are those of the README's *Try it*: Rust stable, clang, `make`, `perl` and `python3`. From a clone:

```sh
# once: fetch the locked dependencies, since the run below is offline
cargo fetch --locked --manifest-path compiler/Cargo.toml
make conformance-run
```

`make conformance-run` builds the compiler's test harness and runs every case in the manifest through the ordinary compiler, comparing each result with the verdict the manifest declares. The gate runs the same test as part of `make check`.

`make safety-claims` runs this page's check: the checker's own tests, then the page against the specification and the manifest.
