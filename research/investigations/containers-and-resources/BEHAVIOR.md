# Behavior parameterization: selected D7 design

The owner selected D6b option A, with `formal` and `actual` as the two group
keywords, a formal-authoritative effect row, structurally matched contracts,
and originally fresh non-copy owned function-formal results. C2 removes that
fresh-result restriction together with owned-history summaries; ordinary
ownership applies to source and linked actuals alike. The
[specification](../../../spec/kernel-spec.md) defines that interface; the
executable evidence and its cost limits are linked below. D6's protocol/conform design,
D6b's routing ceiling, and a compile-cost admission prerequisite are not selected.
The selection serves the constitution's
[delegation/performance objectives](../../../docs/constitution.md): ordinary
explicit free-function interfaces, reusable parameter/argument groups, and no
runtime dispatch or unchecked behavior laws.

The executable D7 evidence is in the family
[behavior measurements](../../experiments/container-representation/families/RESULTS.md#static-behavior-parameterization-d7).
The owning exchange matches its non-generic expansion control but is slower
than D5's scalar-copy exchange; the map has no run transfer, with remaining
retained value/result ABI costs. These limits do not become language rules.

## D7 selection ground

The selected core is a function-kind parameter with an ordinary callable
signature; `formal` and `actual` only abbreviate explicit vectors. This fits
the language's explicit trust boundaries and native-performance objective:
binding is checked before use and each instantiated call has one direct
target. The explicit environment remains an ordinary value or loan. Flat
groups avoid a second module evaluator, type-owned lookup, inference and
overlap rules. Int and Float remain built-in numeric bounds; no behavior law
is inferred from an actual. Ownership and bounds are the container's proof
obligations even under hostile equality or comparison.

The formal row remains authoritative because an author needs one visible
boundary; actual paths must fit it by category and prefix. Exact signatures,
including corresponding region bounds, and structural source-contract
matching avoid an added implication calculus. Captured store brands are
substituted before per-call member regions; a formal has no header regions.
Fresh non-copy function-formal results avoid an invented routing ceiling.
Ordinary direct functions keep their existing owner-return routes.

The owner-selected `actual SeedKey : Key<...>` spelling has a spaced
declaration separator, now stated explicitly in FORM-2; ordinary field,
parameter and bound colons retain left attachment. OP-1's closed reserved-name
inventory is unchanged. A raw function parameter does not shadow a dotless
operation in ordinary call position; its explicit function-argument role and
group-qualified members use the separate FN-5 resolution forms.

FN-6 checks the complete type/const/function dependency vector before any
instance discovery. Unchanged cycles reuse an existing key; acyclic expansion
is finite. This conservatively refuses finite permutations and says nothing
about practical cost of large acyclic expansions. Compile cost remains an
open question, not a fuel limit or a new admission prerequisite. The families
runner compares the selected witnesses with D5 and the retained C controls;
measurements and completion status belong to the evidence below.

## Evidence from seven families

**Ada/SPARK.** Formal subprograms accept named actuals; formal packages reuse
whole parameter groups. Profile matching is not WF proof checking. SPARK requires a generic's copied `Global`/`Depends` specifications to
work for every instance; it explicitly suggests restricting actual subprograms
to a fixed global set. SPARK violations can still appear at instance analysis.
Ada rejects recursive generic instantiation, including indirect nesting. Growth: Ada 83 formals gained formal packages in Ada 95, then aspects; import grouping, not the package/overload system. [1]

**ML.** Signatures name reusable type/value interfaces; structures supply them;
functors accept structures. Ordinary function types supply neither WF effects
nor proved pre/postconditions. SML97 elaborates module syntax rather than
requiring recursively generated monomorphic machine instances; functions remain
runtime values, so direct calls are not a language guarantee. SML97 revised sharing/opaque matching; OCaml grew higher-order, recursive
and first-class modules. Retain signature reuse, not module evaluation. [2]

**Zig.** `comptime` parameters accept functions/types; namespace structs group
operations without a separately declared interface. Bodies are checked for
selected arguments; assertions/reflection replace reusable formal contracts,
with no declared WF effect row. Unrestricted compile-time evaluation is bounded
by a configurable backwards-branch quota, not structural termination. The 0.4.0 comptime core coexists in 0.15.2 with reflection, inline expansion
and executable diagnostics; it lacks WF's determinate, fuel-free proof boundary. [3]

**Austral.** Linear universes coexist with Haskell-style typeclasses: signatures
are grouped, instances supply implementations, and compilation monomorphizes
method calls. Its documented function signatures provide no WF effect or
pre/postcondition calculus; linearity alone supplies neither. The reviewed spec
and compiler account do not establish a fuel-free finite-instantiation rule.
Longitudinal growth evidence is insufficient, but instance uniqueness,
overlap/orphan rules and resolution are already costs WF can avoid. [4]

**C++.** Concepts reuse argument constraints; policy types or constant function
arguments supply behaviors. Templates specialize direct calls, but concepts
constrain well-formed operations, not exact effects or machine-proved contracts;
`noexcept` is only about exceptions. Recursive instantiation/constant evaluation
has implementation limits, not WF's termination discipline. Historical layering
from templates/specialization through SFINAE, concepts and constexpr illustrates
how inference, overload choice and programmable formation accumulate rules.
C++26 contracts do not turn concepts into WF proofs. [5]

**Odin.** Explicit polymorphic type/constant parameters accept concrete procedures;
procedure types repeat signatures and procedure groups name overload sets, not
reusable behavioral interfaces. `contextless` removes an implicit context pointer,
not side effects; `where` checks are not WF pre/post proofs. Its overview does
not specify a finite-instantiation acceptance criterion. Starting as a Pascal
clone in 2016, it now combines explicit/implicit polymorphism, specialization,
`where` and procedure groups while still rejecting methods. Function-centric
syntax alone does not prevent generic-system growth. [6]

**Go.** Constraint interfaces reuse method/type-set requirements, satisfied by
types rather than separately supplied free functions; function arguments are
runtime values. There are no WF effect/proof contracts. The compiler detects
expanding type-flow cycles, a useful structural finiteness example; Go 1.18's
implementation uses shape sharing and dictionaries, not guaranteed direct calls.
Growth from Go 1.18 type sets to Go 1.20's `comparable` satisfaction exception
shows the interaction costs of retrofitting existing interfaces. [7]

## Three options on identical code

The three proposed-source appendices compared identical enum-slot owning-Box
`find`/`put` and owning-element priority `push`/`pop`, changing only parameter
expansion, qualification and naming. Their dated source is retained in
[the D6b comparison revision](https://github.com/mbbill/Whitefoot/tree/cd3e946817af5eaea9a80efcefd828e39b94d015/research/investigations/containers-and-resources).
The selected A appendix is now the executable
[owning map](../../experiments/container-representation/families/owning-behavior.wf)
and [priority queue](../../experiments/container-representation/families/priority-behavior.wf)
in the families runner. The map extends the selected source to the complete
D2 growth/refusal contract; D5 and C controls remain independent comparators.
The three superseded proposed-source files are removed; their alternatives
and selection grounds remain in this table.

| Constraint / cost | A: formal + actual | B: nominal + actual | C: function-kind only |
| --- | --- | --- | --- |
| Free functions; no Self or dispatch | Yes | Parameter projection only | Yes |
| One semantic core; direct call | Function substitution | Same | Same |
| New top-level kinds / binder forms | 2 / 2 | 1 / 2 | 0 / 1 |
| New productions / keywords | 4 / 2 | 3 / 1 | 1 / 0 |
| Fixed row; explicit instantiation | Yes, boundary below | Same | Only with same boundary |
| Reused formal / actual lists | Both named | Both named | Both repeatedly written |
| Arbitrary functions / types | Ordinary signatures; limits below | Same, plus nominal dependency | Same; no reusable interface |
| Container identity includes behavior | `Map<SeedKey>` | `Map<SeedKey>` | `Map<u64, Seed, fn seed_hash, fn seed_equal>` |
| No-container sort | `sort<Order<T,E>>` | Dummy struct or depend on Queue | Repeat comparison signature |
| Laws / termination / region safety | None assumed; rules below | Same | Same; no arbitrary CTFE |
| Likely growth pressure | Group nesting, defaults | Fake types, header extraction, type projection | Signature aliases, inferred requirements |

**A** uses the owner's general mechanism spelling `formal Key<K: linear,E: linear>`,
`actual SeedKey : Key<u64,Seed>`, `fn find<Key<K,E>>`, `Key::hash`, and
`find::<SeedKey>`. `Key<K,E>` forwards the whole group; the full
`Key<K1,E1>::hash` disambiguates distinct written applications before
substitution, even if their final types coincide. Two identically written
applications need separately named raw function formals: no pack aliases are
proposed. A group is an abbreviation,
not a type; equal expanded vectors give identical container types. Declaration
pack positions bind fresh ordered type/const names; forwarding positions name
existing bindings. Generated function-formal identities are hygienic: `Key::hash`
cannot capture `put`'s local `hash`.

The maintained complete sparse-map witness linked above deliberately narrows
that mechanism to `K: affine`. Its duplicate-replacement path explicitly
disposes the displaced key, which is not a valid operation at the symbolic
`K: linear` instance. The earlier linear spelling therefore demonstrates the
behavior-parameter mechanism's bound shape, not a claim that this particular
full sparse state machine can consume every must-consume key on replacement.

**B** puts those function formals on `struct Map`; `fn find<Map<K,E>>`
imports its generic header, and `Map::hash` selects a formal, never a method.
`Map<'s,Map<K,E>>` explicitly forwards that header into a nominal instance;
header projection excludes the storage fields and nominal region list.
This saves `formal` but makes unrelated algorithms depend on a storage
abstraction or invent a fieldless struct. Types gain a second meaning.

**C** writes `fn hash(...) ...` inside every generic parameter list and passes
`fn seed_hash` at every application. It retains FN-2's per-instance checking.
A truly Zig-like version without formal rows/contracts violates the fixed-row
constraint; this restricted version retains them, but loses declaration-time
group validation and signature reuse.

## Shared semantic boundary and exact deltas

**Fixed does not mean inferred later.** At a formal call, use the formal's
written row and contracts, even after binding a narrower actual. Match modes,
types, ordered results and alpha-renamed regions exactly; require each actual
effect path to be covered by a formal path in the same category (prefix coverage,
allocation brands equal). Match ordered requires/ensures structurally after
binder/region normalization and definition substitution; no implication solver. A group checks all bindings atomically at declaration.
Generic actual functions must have all type/const/function arguments supplied;
region-polymorphic ordinary signatures remain available.

This **changes EFF-2's abstraction boundary**: exact means the complete syntactic
row using declared formal interfaces, not the narrower selected bodies.
For example, formal `reads(env,key)` remains so when its actual reads only
`env.salt,key`; no dummy reads or runtime adapter are emitted. Check generic
row attribution at the symbolic template, retaining formal state paths even
when a later copy instantiation would frame them out. Instance rechecks preserve that public row, rejecting additional effects;
ordinary call projection uses the resolved actual storage. Under C2, an owning
input moved into a local does not leave a value-history effect root; mutation
through an exclusive parameter still projects onto the caller's place.
If “exact” instead requires re-derived concrete-body equality, the baseline's
subset binding cannot meet it; require equality instead, losing narrower
implementations.

**The D7 freshness restriction is historical.** D7 initially required every
non-copy owned formal result to carry fresh state and refused an owned identity
actual. C2 [selected decision 6](../ordinary-host-values/DECISIONS.md#6-owned-effects-and-function-formals)
removes that restriction together with owned-result and exclusive-referent
routing. An ownership-returning actual now uses the same ordinary signature,
formal-row coverage and structural contract checks as every other function.
Shared and view results keep ordinary signature-derived loan provenance.
Neither an owned input nor an opaque result receives a hidden history root.
The earlier finite-routing-ceiling proposal remains rejected; C2 introduces
no replacement summary mechanism.

Keep FN-2/FN-9 instance checks for ownership/proofs and recursive proof-summary
withholding; use only formal proof contracts, with no FN-1 target summary. FN-8 proves
requires, FN-9 verifies actual ensures, CALL-6 kills overlapping fixed-row support
before publication. `&uniq` bare/`entry(p)` measures denote exit/entry, including
fields and multiple results. Shared/view outputs keep FN-1 loan-origin bounds;
FORM-8 alpha-matches quantified formal loan regions, never extends loans.
A formal declaration has no header region parameters. Each member declares
its own regions as an ordinary function signature does; every call instantiates
those loan regions under FORM-8 and the OWN rules. The owner selected this
scope over a group-wide region argument, which would add a different
quantification and forwarding mechanism without a requirement from the two
witnesses. Actual declarations keep their region parameters solely for store
brands inside the formal's type arguments under FN-2.
Captured brands remain exact: `actual G['s] : Key<Box<'s,Resource>,Seed> ...`.

All options expose FN-2's existing arbitrary-type limit: receivers must name
every store brand hidden in K. To support arbitrarily many such brands, propose
retaining K's complete opaque branded identity and STOR-5 confinement without
requiring redundant receiver binders. This is the selected FN-2 delta; its executable branded-key witness is part of D7 validation. Hidden loans/providers remain excluded;
ordinary directly written loan/provider parameters, owned results and multiple
results are available on function formals. No higher-rank type-polymorphic
function actuals, implicit environment capture or behavior construction code.

**Finiteness:** resolve finite written bindings/forwarding edges before expansion.
Extend FN-6 to function and nominal-instantiation dependency cycles, the
complete type/const/function argument vector, and indirect formal-call edges: each edge within a recursive component must forward that
vector unchanged, modulo region alpha-renaming. Reject constructed/permuted
arguments on a cycle, including nesting a specialized function actual.
This also rejects `Grow<T>` containing `box<Grow<box<T>>>`; TYPE-2 alone
does not ensure finite nominal expansion. Acyclic source/header expansion is
finite; cycles create no new instance keys. No CTFE or fuel. This proves
termination only: acyclic fan-out can still be exponential under existing FN-2.
Acyclic compile cost is recorded as an open engineering question; the owner
explicitly did not make a new cost-admission mechanism part of D7. Timing cannot
prove an asymptotic bound, and no fuel or timeout selects source acceptance.

**Open correspondence finding after the main merge.** The new
[language design root](../../../design/language.md) rules out compilation work
exponential in written source size. D7's unchanged cycle restriction proves
finiteness, not that stronger bound: a finite acyclic graph can duplicate
specializations at each level. The owner deferred that cost question in D7.
The combined v0.56 work has not reconciled these decisions and does not claim
the stronger bound is implemented. This remains an owner decision; no timing
gate, instance fuel or silent restriction resolves it.

**Grammar inventory** (new production counts, not net counts; reused names count
as modifications): new binder forms are function-kind (all) and pack (A/B).
All options add `function_arg`, modify `gparam`, `targ`
and `fn_sig`; `fn_sig` gains fn_decl's result list/contract and loses its trailing
semicolon. A adds `formal_decl`, `actual_decl`, `pack_use`; B adds only the latter
two. A/B also modify `item`, `fn_bind`, `callee`; type syntax already parses
pack applications, with a kind check before expansion. C needs no such changes.
The exact new right-hand sides are:

```text
function_arg := "fn" callee ("::" targs)?
pack_use := TYPEID targs?
formal_decl := "formal" TYPEID generics? "{" doc? (fn_sig ";")* "}"
actual_decl := "actual" TYPEID region_params? ":" pack_use "{" doc? fn_bind* "}"
```

`gparam` adds `fn_sig` (A/B also `pack_use`); `targ` adds `function_arg`.
A/B's `callee` adds `pack_use "::" IDENT`; `fn_bind` becomes
`IDENT "=" callee ("::" targs)? ";"`. Only `formal`/`actual` are new keywords.
Remove `contract_decl`, `conform_decl`, `law`, `law_arg`; C also removes unused
`fn_bind`. Keep `contract` for proof blocks; Int/Float become built-in bounds.
Rules: GRAM-1 permits abbreviation expansion with retained source nodes;
GRAM-2/3/5 and TYPE-5/6 cover syntax/kinds/resolution;
FN-1 retains ordinary signatures and borrow provenance; FN-2–6 cover binding/checking/termination
and retire source laws;
EFF-1/2 fix formal attribution; FN-8/9, CALL-6, MSR-3 and FORM-8 reuse the above
contract/region judgments; PRE-1/section 15 retain numeric bounds.
Strong-LL(2) selector generation remains a required implementation check.

**Selected: A**, with authoritative formal rows; C2 subsequently removes the
initial fresh-result restriction at function-formal bindings. It removes synchronized signatures without
type-owned functions; compile cost remains an open engineering question;
B saves one declaration kind at a lasting conceptual cost, C saves syntax by
moving that cost to every library layer. Initially keep groups flat, with no
inheritance, defaults, inference, overload choice or group-producing functions.
The executable D7 witnesses now cover expansion, retained-helper timings,
adversarial equality, branded keys and two-state mutation. The generic queue
matches its ordinary-function expansion control; its owning exchange still
costs more than D5's scalar-copy exchange. The map retains value/result ABI
costs despite zero run transfer. Broad formal rows can also lose parallel
independence. The linked measurements bound these claims rather than asserting
universal native parity.

## Acyclic instantiation: measured open boundary

A bounded experiment on 2026-09-13 distinguishes distinct-instance growth
from repeated calls to one instance. Before measurement its criterion was:
all three acyclic families must be accepted under unchanged FN-2/FN-6; the
family with two different type arguments should emit `2^(n+1)-1` generic
functions, while repeated-identical and single-call controls should emit
`n+1`. Timing locates cost, not an acceptance bound. This is a synthetic
structural witness, not an estimate of application frequency.

The complete depth-two distinct witness is:

```wf
struct Left<T: affine> {
}

struct Right<T: affine> {
}

fn step0<T: affine>() -> result: own unit pure {
  return unit;
}

fn step1<T: affine>() -> result: own unit pure {
  step0::<Left<T>>();
  step0::<Right<T>>();
  return unit;
}

fn step2<T: affine>() -> result: own unit pure {
  step1::<Left<T>>();
  step1::<Right<T>>();
  return unit;
}

fn main() -> status: own ExitStatus pure {
  step2::<u64>();
  return exit_status(code: 0_u8);
}
```

For depth `n`, repeat the step declaration up to `stepN`, always calling the
previous level, and call that final level from main. The same-type control
replaces `Right<T>` with `Left<T>` in each call; the single-call control omits
the second call. The two phantom structs have no fields, so runtime payload
size does not grow. No behavior group, allocation or nontrivial proof is
needed. Compile with `whitefootc --no-overlap --emit-llvm` and count emitted
`wf_step` definitions, excluding ordinary/prelude entries and the launcher.

The pinned compiler was gate-profile `6ba8cf52`, source-equivalent to
`ea69e8eb` in the compiler paths, binary SHA-256
`b602395f5fba8d970dc85ecf75a259435fa17f776d35b87b66d864d9762adc0d`.
The host was arm64 macOS 26.6.2, Rust 1.98.1. Depths 0,2,4,6,8,9,10 each had
one warmup and three interleaved recorded samples per family. All 84
compilations succeeded. [All 92 observations](generic-cost-samples.jsonl)
retain those 84 runs, seven separately instrumented stage runs and the later
paired stock run; keep
this artifact with this interpretation until superseded by matched evidence.

| Depth | Distinct source bytes | Distinct instances | Same/single instances | Distinct median user CPU ms |
| ---: | ---: | ---: | ---: | ---: |
| 6 | 893 | 127 | 7 | 162.6 |
| 8 | 1117 | 511 | 9 | 863.1 |
| 9 | 1229 | 1023 | 10 | 3879.8 |
| 10 | 1343 | 2047 | 11 | 20062.8 |

The initial timing cohort was host-contended. A later paired run of the same
unchanged binary took 6.475 seconds wall / 6.308 seconds user CPU at depth 10;
this is not an optimization. Counts are exact: growth is exponential in the
number of written declarations, or superpolynomial in source bytes including
growing identifier lengths. Distinct depth 10 IR was 335056 bytes, versus
6616/6216 in the same/single controls. This does not prove that a sound
compiler must enumerate this particular family of equivalent unit bodies.

A temporary source archive of the same revision added only `Instant` reads
and function-count prints around `compile_selected`'s prefix,
`check_semantics`, `lower_checked`, and `emit_llvm`/launcher result. It changed
no acceptance call, instance or algorithm; its output was byte-identical to
the stock module for all seven stage runs. Its binary SHA-256 was
`06f6099084d956493d4270015c59e3aec30674a0da52ccde49f806ed85615549`.
The temporary instrumentation does not enter the compiler. The two depth 10
runs located 3.410/3.278 seconds in semantics, 3.088/3.026 in lowering and
0.0055/0.0055 in LLVM text generation. Stage samples locate expense, not
robust asymptotic exponents; RSS and native stack profiling were unavailable.

`semantic/check/generics.rs` deduplicates full substitution keys; the controls
confirm that repeated identical calls do not multiply instances. Its linear
key searches and staged symbolic/concrete replay can still repeat work.
`lowering/builder.rs` builds a full physical type map per function variant;
`lowering/physical_types.rs` visits executable nominals and scans matching
instances. These are plausible spec-neutral optimization targets, but their
individual costs and a repair's speedup have not been measured. Optimizing
them cannot by itself prove a polynomial bound on distinct-instance count.

Three owner options remain: retain the written-source complexity objective
and design a provable structural restriction or explicit instance inventory;
retain current accepted programs and research shared symbolic checking/code
sharing, whose general bound is unproved; or explicitly replace the objective
with cost relative to a finite expanded graph, retaining the proof algorithm's
existing constraints. The first can restrict useful composition, the second
needs a new argument, and the third permits compact sources with very large
expansions. No option is selected here. The recommendation is a bounded
comparison of the first two against real container compositions while keeping
the current principle; whether PR #30 may merge with this identified open
item is a separate owner scope ruling. No fuel, cutoff or new rejection has
been introduced, and no duplicate-cache improvement is presented as closing
the conflict.

## Sources

Primary sources checked 2026-09-11; no cross-language timings.

[1] Ada [instantiation rule](https://ada-rapporteur-group.github.io/ARM/Ada_2012/RM-12-3.html), Ada 95 AARM [12.6](https://www.adaic.org/resources/add_content/standards/95aarm/AARM_HTML/AA-12-6.html), [12.7](https://www.adaic.org/resources/add_content/standards/95aarm/AARM_HTML/AA-12-7.html); AdaCore [SPARK RM 27.0w, §12](https://docs.adacore.com/spark2014-docs/html/lrm/generic-units.html).
[2] Milner et al., [SML97 Definition, §§3/5/7 and appendix G](https://smlfamily.github.io/sml97-defn.pdf); [OCaml 5.3 generative functors/extensions](https://ocaml.org/manual/5.3/generativefunctors.html).
[3] Zig official language references [0.4.0](https://ziglang.org/documentation/0.4.0/#comptime), [0.15.2 comptime and quota](https://ziglang.org/documentation/0.15.2/#comptime).
[4] [Austral specification](https://austral-lang.org/spec/spec.html), [author's compiler design, 2023](https://borretti.me/article/design-austral-compiler).
[5] C++ draft [template parameters](https://eel.is/c++draft/temp.param), [constraints](https://eel.is/c++draft/temp.constr), [implementation limits](https://eel.is/c++draft/implimits), [contracts](https://eel.is/c++draft/dcl.contract).
[6] Odin official [overview](https://odin-lang.org/docs/overview/), [history/FAQ](https://odin-lang.org/docs/faq/).
[7] Go [specification](https://go.dev/ref/spec), [instantiation-cycle checker](https://go.dev/src/go/types/mono.go), [Go 1.18 dictionary design](https://go.googlesource.com/proposal/+/master/design/generics-implementation-dictionaries-go1.18.md), [Go 1.20 comparable change](https://go.dev/blog/comparable).

## D7 source migration and retirement

These dispositions record the original D7 amendment on the pre-merge branch.
C2's later [case register](../ordinary-host-values/CASES.md) supersedes their
owned-history and native-release assumptions. An old expected verdict is not
changed to accommodate a compiler failure.
The five retired law sources are preserved, with only `command fn main`
normalized to ordinary `fn main`, in the compiler's
`retired_closed_law_table_has_no_remaining_acceptance_path` grammar regression;
their former law-acceptance obligations were removed by D7 and remain absent
from the combined v0.56 publication.

| Previous source ID | Current source ID or disposition | Rule and reason |
| --- | --- | --- |
| `fn3-neg-source-contract-bound` | `fn3-neg-source-contract-bound` | Only Int and Float occupy the numeric-bound TYPEID position. A named formal group occupies its own generic-list position and is refused as a numeric bound under FN-3. |
| `fn4-neg-bad-lawname` | Retired | FN-4 removed law/law_arg; source remains in the compiler grammar-retirement regression with the entry-syntax normalization stated above. |
| `fn3-pos-empty-marker-conformance` | `fn3-pos-empty-marker-conformance` | A zero-member formal admits a matching empty named actual; expansion creates no runtime function or group object. |
| `fn3-pos-normalized-region-effects` | `fn3-pos-normalized-region-effects` | Formal and actual loan regions alpha-match by parameter position and effect paths normalize by parameter ordinal; reversed read-set order preserves the boundary. |
| `fn3-neg-generic-contract` | `fn3-pos-generic-formal` | D7 admits flat type/const parameters on a formal header; this replaces the retired source-contract-template refusal. |
| `fn3-neg-out-of-order-binding` | `fn3-neg-out-of-order-binding` | A complete set of actual bindings in a different order from the formal member declarations rejects under FN-3. |
| `fn3-neg-two-conformances` | `fn3-pos-two-explicit-actuals` | D7 removes implicit type/conformance uniqueness. Two differently named actual groups may bind the same formal application; selection is explicit. |
| `reject-syseff-pure-member-binds-release` | `fn4-neg-pure-member-binds-release` | An actual's ordinary derived release writes its incoming owner. D7 FN-4 refuses that effect at a pure formal boundary; resources use ordinary objects and effects. |
| `fn4-pos-law-discharged` | Retired | FN-4 removed law/law_arg; source remains in the compiler grammar-retirement regression with the entry-syntax normalization stated above. |
| `fn4-neg-law-undischarged` | Retired | FN-4 removed law/law_arg; source remains in the compiler grammar-retirement regression with the entry-syntax normalization stated above. |
| `fn3-neg-missing-binding` | `fn3-neg-missing-binding` | An actual must bind every formal member exactly once in source order. FN-3 refuses the complete incomplete actual declaration before publishing a group. |
| `fn3-neg-signature-effect-mismatch` | `fn4-neg-formal-row-coverage` | D7 FN-4 requires every actual effect path to be covered in the same category by the formal row. An allocating actual exceeds a reads-only formal. |
| `fn3-neg-requires-member` | `fn4-neg-requires-mismatch` | D7 FN-4 requires ordered structural contract equality. A requires-bearing actual cannot bind a formal with no requirement. |
| `fn1-pos-result-provenance-zero-candidate` | `fn1-pos-result-provenance-zero-candidate` | A formal member with a borrowed result and no candidate input obeys FN-1's zero-candidate rule; only named const storage can supply it. |
| `fn4-pos-law-in-contract` | Retired | FN-4 removed law/law_arg; source remains in the compiler grammar-retirement regression with the entry-syntax normalization stated above. |
| `fn3-neg-contract-arguments` | `fn3-neg-contract-arguments` | An actual applies a zero-header-parameter formal to one type argument; FN-3 refuses the arity mismatch. |
| `fn3-pos-contract-conform` | `fn3-pos-contract-conform` | A formal declares function-kind signatures and a named actual supplies the complete ordered function argument group (D7 FN-3); no implementation is attached to a type. |
| `fn3-neg-extra-binding` | `fn3-neg-extra-binding` | An actual has an extra binding beyond its formal's complete member table; FN-3 refuses the extra member. |
| `fn1-neg-contract-borrowed-slice-result` | `fn1-neg-contract-borrowed-slice-result` | The ordinary borrow-mode direct-slice result refusal applies to a formal member's signature formation before any actual can bind it. |
| `fn4-neg-law-refuted-signedness` | Retired | FN-4 removed law/law_arg; source remains in the compiler grammar-retirement regression with the entry-syntax normalization stated above. |
| `checked-law-channel/kernel.wf`, `kernel_lib.wf` | Historical experiment, retired from active language claims | D7 removes law declarations and any implicit law authority. Retained measurements describe their historical toolchain only; neither file is a current check target. |

The unchanged `x-eff-pure-combined-with-allocation` source still rejects. Its
diagnostic owner changes from EFF-1 to GRAM-2: a raw function-kind `gparam`
makes a comma a valid FOLLOW token after the `effects` alternative `pure`.
At a top-level function that comma instead fails the enclosing `fn_decl`
continuation. DIAG-1 assigns the failure to that production. The unit probe
`live_effect_categories_keep_eff1_canonical_order_and_multiplicity` migrates
the same citation; duplicate and reversed nonempty rows still reject EFF-1.
Neither source bytes nor accepted-program status change.

The separate S37 case `gram2-neg-a-type-parameter-writes-no-bound` becomes
`fn3-neg-bare-type-parameter-has-no-formal-group`. Its executable source still
writes `fn pass<T>(...)`; only its explanatory docs change. With `pack_use`
admitted in `gparam`, bare `T` now parses as a formal application and TYPE-6
performs group lookup in the shared nominal-type domain. There is no such declaration, so the
rejection is FN-3 rather than the former GRAM-2. This does not admit an
unbounded type parameter or infer a bound. The diagnostic migration follows
the selected grammar and namespace, not a relaxation of the expected result.

## D7 implementation review evidence

The independent completion reviewer rebuilt an isolated CLI and checked the
replacement FN-6 graph before every instance-discovery entrance: ordinary
checking, FN-9 selector preflight, and missing-main diagnostic salvage. The last
path initially bypassed the guard; its repair now rejects the growing nominal
with and without a main or selector, and is retained as
`missing_entry_diagnostic_salvage_checks_instantiation_before_discovery`.

The owner authorized deletion of twelve old helpers after both a repository-wide
call search and the independent Rust build reported no active callers:
`reject_generic_call_cycles`, `first_polymorphic_recursion`,
`call_instantiates_caller_parameters`, `targ_names_type_parameter`,
`type_parameters`, `render_call_cycle`, `generic_call_edges`,
`generic_cycle_analysis`, `generic_cycle_components`,
`call_repeats_caller_generic_arguments`, `targ_names_const_parameter`, and
`graph_reaches`. They only called one another. The live guard is
[`generics/finiteness.rs`](../../../compiler/src/semantic/check/generics/finiteness.rs);
`const_generic_type` and both postcondition-membership helpers remain in
`generics.rs`. This deletion removes no active termination judgment.

The subsequent review closed four grammar/instantiation boundary defects.
Per-call member regions must have equal PROV-6 bounds, and an actual's captured
regions must satisfy that declaration's own bound at each application.
Region argument readers now test for a REGIONID directly; a function-kind
argument in that position is an ordinary wrong-kind source diagnostic, never
an internal resolution failure. Raw binding diagnostics retain their written
argument provenance separately from instance identity, including nominal-only
uses and shared instances.

FN-6 also checks resolved member-call edges as complete-vector projections.
The selected target's specialization is a proper finite subterm of one caller
function argument; it cannot equal the entire caller vector without an infinite
self-containing argument. Such calls are allowed off cycles and refused on
written recursive components. The finite `first<second<stop>>` counterexample
is deliberately refused by this selected structural rule, while its acyclic
variant is admitted. This is a rule-compliance repair, not evidence that that
particular program would instantiate forever.

## Ordinary inlining hints for supplied functions

The current compiler candidate gives ordinary LLVM `inlinehint` to a concrete
function definition supplied as a function-kind actual by an emitted instance.
This extends the [Vector callback investigation](../../experiments/container-representation/vector-library/RESULTS.md#ordinary-behavior-hints-combined-k-gains-without-useful-cell-regression)
into the ordinary compiler path; the raw-artifact experiment motivates a
candidate, not a selected policy for every family. FN-2 and FN-5 supply a
general category with concrete direct targets, but imply neither small bodies,
frequent calls, termination nor a performance benefit from inlining.
In that artifact the wide callback's observed cost remains 450 while the
ordinary-hint threshold changes from 375 to 487; original-source F is unchanged.
The combined forward-consumption/hint arm gains useful cells but regresses the
three wide empty-suffix controls. F+hint alone has identical native images to
F; the callback threshold observation belongs to the combined K+hint artifact.
The subsequent [actual-compiler factor comparison](../../experiments/container-representation/vector-library/RESULTS.md#actual-compiler-factor-isolation-after-ownership-integration)
finds no native effect from the hint with or without terminal consumption at
its source and toolchain pin. The timed and accounting objects are
byte-identical within each hint pair, and neither image retains an accept
callback. That comparison establishes no current Vector benefit for the hint;
the traversal gains cannot supply its separate selection ground.

The physical call inventory reads the checked instance's `function_actuals`
identities after group expansion, imported resolution and forwarding. Only
instances in the build's emitted inventory contribute. Repeated bindings name
one physical definition; if ordinary calls also reach it, they see that same
hint. Merely checking an unrelated instance does not affect a selected entry,
and the hint does not make an otherwise uncalled definition reachable. Large,
recursive and mixed-use actuals remain subject to the host inliner's ordinary
cost and legality decisions. Bodyless linked source declarations receive no
new hint; declarations synthesized into fragments from hinted definitions
retain the definition's attribute.

The provenance travels on the existing typed function into structured LLVM
headers. One physical function identity can emit ordinary, destination-form,
budgeted and sequential-world symbols; every such definition retains that
identity's hint. A register-returned actual's entry and destination-form body
both receive it. None is `alwaysinline`: the candidate preserves the existing
body/entry structure and pipeline and requests no early mandatory inlining.
Header checks alone do not establish LLVM's actual pass order or resulting
body optimization; the production native audit must still inspect them.
Source acceptance, bodies,
call ABI, linkage, arithmetic and alias facts are unchanged. Structured
fragment declarations retain the attribute, and serialized module headers and
fragment bytes change with it. Entry-cache input already includes the selected
composition's records and entry; a changed binding context must rebuild that
module, while the fragment's changed bytes select a different native object.

The discriminating checks are raw actuals, named groups, imported forwarding,
repeated concrete bindings, mixed ordinary use, nonactual controls, selected
entry scope, and cache reuse across a binding-only context change.

All four `function_actual_hints` tests and the existing linked-native behavior-actual
control passed. They cover the ordinary and `.body` headers, recursion-budget
entry and sequential worlds, bodyless linked-declaration exclusion, mixed and
forwarded bindings, physical deduplication, structured fragments and cache
selection. These results establish the tested metadata and linked-call behavior;
they do not establish LLVM optimization order or performance.

The complete Vector comparison has now run. The earlier
[five-family construction](../../experiments/container-representation/ECOSYSTEM.md#actual-compiler-construction-and-native-admission)
also passed the other families' functional checks and found their native
objects and linked sections unchanged at that pin. These observations do not
establish a general benefit or a cost ceiling for large or recursive actuals.
The pending [compiler amendment](../../../design/amendments/function-actual-inlining.md)
still records the provisional preference. Independent review found that its
historical callback ground does not establish a benefit in the current Vector
compiler; whether to withdraw it or retain it for a different measured consumer
awaits the owner's ruling. The provisional implementation remains on the work
branch pending that ruling.
A global threshold, forced inlining,
name-based selection and changed export visibility are separate alternatives,
not part of this candidate.
