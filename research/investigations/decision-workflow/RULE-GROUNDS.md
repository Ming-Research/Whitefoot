# Historical language-rule assessments

This record preserves assessments from the retired rule-index workflow and
the container branch before the ordinary-host-values amendment. Its rule
inventory and recommendations are historical, including owner-routing,
release, function-result freshness, command-entry, suspension, qualification
and gated-family mechanisms. The selected
[ordinary-host-values design](../ordinary-host-values/DESIGN.md) supersedes
those recommendations; they do not describe the current language. The
[active specification](../../../spec/kernel-spec.md) defines acceptance, and
the [design trees](../../../design/) carry current decisions and their grounds.
There is no replacement rule index to keep in sync here.

The arguments below record the assessments made during those revisions, not
recovered claims about an original author's intent. Their `current` and
`revisit` labels refer to those assessments, not to a present selection or
work queue. Alternatives are historical comparisons unless explicitly
identified otherwise. No corpus count, familiarity claim, or migration cost
establishes that a retained historical restriction should be restored.

## Reading the judgments

The common premises are the chosen safety requirements, practical development
at large-system scale, and runtime performance within those constraints.
Human control of objectives does not require a fixed division of labor between
models. Reducing repeated human inspection motivates machine-enforced
interfaces; it does not make any particular grammar or proof calculus necessary.

`deduction` is used only for the conditional consequence identified in a row.
It does not certify the complete mechanism, implementation, or soundness proof.
`provisional` marks a mechanism whose comparative merits remain uncertain.
No new comparative experiment or formal soundness proof was performed for this
assessment. Existing experiments retain their recorded scope. In particular,
conformance tests can check implementation agreement with a chosen rule, but
cannot establish that the rule is the best design.

`current` means the selection argument has been assessed and has no specific
unresolved objection identified here. `revisit` means the argument has been
assessed but a concrete question remains open; it does not mean the row was
skipped. The reopening triggers below belong to that historical assessment;
current work follows [decision practice](https://github.com/mbbill/Whitefoot/blob/efe40194a05b339c46b931e87bd05a40036f23cf/docs/practice.md#decision-work).

## Scope and surface

The earlier [surface assessment](DESIGN.md#surface-and-definition-conventions)
and [surface decision memory](https://github.com/mbbill/Whitefoot/blob/0f22b026b062c36ef2f8ce9c725ee4307e1fda64/mcts_mem/whitefoot/surface-form.md)
record the canonical-form hypothesis and alternatives. Predictable editing is
a reason to try these forms, not an established improvement in agent success.
Reopen a convention when representative agent work exposes avoidable failures,
proof friction, or excluded safe implementations; compare task outcomes and
compiler costs, not merely token counts.

| Rules | Grounds and limits |
|---|---|
| SCOPE-1 | Excluding writer-controlled unsafe bypasses follows from the chosen safety requirement. A separate trusted implementation family is one provisional way to implement system primitives; it must not become a route for writers to waive proofs. The distinction between that family and repository work needs clarification with GATE-1 and LEDGER-1. |
| SCOPE-2 | Given safety before acceptance, every required domain obligation must have checked evidence. Treating runtime inputs as symbolic values preserves this requirement without forbidding useful input. Canonical syntax and this exact evidence calculus remain selected mechanisms. |
| SCOPE-3 | A guarantee must state its implementation and environmental assumptions. The listed TCB is a provisional implementation boundary, not a permission to weaken source safety. External exhaustion is explicitly outside the current outcome model; this does not yet satisfy the constitutional goal for uses with resource budgets. Revisit when defining those budgets or adding a target/runtime. |
| FORM-1, FORM-2, FORM-4 | One spelling per construct, exact formatting, and declaration documentation instead of comments are authoring conventions. They may simplify generation and review, but rejection of harmless formatting or comments is not a safety consequence. The existing scoped assessment remains applicable. |
| FORM-3 | Disjoint lexical classes and reserved mode suffixes give a stable lexical interpretation and prevent operation names from colliding with field access. Sigils, casing, and the size of the reservation set are provisional; a different unambiguous lexer could serve the same purpose. |
| FORM-5 | Typed literals and defined rounding fix the value being compiled. Shortest-decimal selection, decimal-only integers, ASCII documentation, and enum Bool spelling are provisional canonicalization policies. Unique value interpretation does not require the shortest spelling or exclude hexadecimal notation. |
| FORM-6 | Unit needs a defined type and value. Reusing one token in disjoint grammar positions is a compact, unambiguous convention, not a consequence of the harness purpose. |
| FORM-7 | Rejecting out-of-range literal values prevents implicit overflow under the chosen exact numeric semantics. Leading-zero and noncanonical-float rejection serve FORM-1, not safety independently. |
| FORM-8 | Preserving distinct region identities and identifying returned origins are required by the selected ownership model. Omitting structurally determined region arguments can reduce redundant writing without losing those identities. Its placement-specific rules are provisional and expose the overbroad wording of META-2; region inference outside this closed relation is not being admitted. |
| LEX-1 | Naming the invariant rather than one lowering consequence can prevent misleading source expectations. `uniq` versus `mut`, the divergence census, and the ban on backend vocabulary are provisional terminology policies. A familiar word is neither automatically wrong nor a safety guarantee. |
| GRAM-1 | Deterministic, unambiguous parsing supports reproducible diagnostics. Maximal lexical formation, strong LL(2), and a production-to-node mapping are selected implementation constraints; other deterministic grammars could work. Their usefulness must be judged against actual language needs. |
| GRAM-2 | A closed declaration grammar makes callable, type, region, contract, and effect boundaries inspectable. Exact declaration ordering, mandatory result binders, and the currently restricted contract grammar are provisional. The grammar does not establish that the available abstractions suffice for target projects. |
| GRAM-3 | Explicit mode/type productions distinguish ownership from representation. Their exact token and parameter forms are conventions; their semantic obligations belong to TYPE, OWN, and PROV. |
| GRAM-4 | Explicit statements, local proofs, mutation, and multi-result binding give each transfer a syntactic home. The inventory and placements are provisional; a different structured grammar can preserve the same transfer and proof judgments. |
| GRAM-5 | A closed operand/place grammar makes evaluation and proof identities tractable. Its flat executable forms and broader erased relation forms serve different judgments. Their exact boundaries are provisional and must not be described as all expressions having identical context-independent meaning. |
| GRAM-6 | Explicit if/else supports ordinary input-dependent behavior. Keeping it distinct from erased invariant syntax prevents a proof from silently adding execution. Exact statement/value forms and chaining are provisional. |
| GRAM-7 | A value initializer gives branch-dependent values an explicit delivery point without mutation of an outer binding. Restricting it to a let initializer is provisional; other typed expression forms could provide the same value-flow guarantee. |
| GIVE-1 | Every continuing path must deliver a value of the agreed type and mode to avoid an uninitialized result. Structural delivery checking is a conservative choice, not a termination proof. Bounded relation delivery exists for value-if but has no equivalent for value-match; this is an open compositionality limitation. Revisit with a concrete relation lost at a match join. |
| GRAM-8 | Complete, correctly identified fields support initialized construction. Mandatory labels and declaration order are provisional safeguards against omission or misidentification, not protection against swapping two same-typed values. The [constant probe](../const-eval/INITIALIZATION.md) demonstrates precisely that limitation. |
| GRAM-9 | Flat computation exposes intermediate values and evaluation order, which can simplify local checking. This does not establish faster generated code or fewer agent errors. Nested expressions with explicit evaluation order are a viable comparison if flatness obstructs useful programs. |
| GRAM-10 | Exhaustive named payload binding identifies the fields being consumed or borrowed. Field order and mandatory distinct binder spelling are conventions; they do not prove the bound value is used correctly. Routed postconditions have a separate, narrower FN-9 admission. |
| GRAM-11 | Named user/system arguments expose interface roles; positional primitive operands follow the closed operation table. Both are provisional surface choices. Labels check names, coverage, and order, not the semantic suitability of same-typed actual values. |

## Types, ownership, and storage

The active TYPE through STOR rules are the semantic reading set. The
[ownership record](https://github.com/mbbill/Whitefoot/blob/0f22b026b062c36ef2f8ce9c725ee4307e1fda64/mcts_mem/whitefoot/ownership.md) preserves earlier
alternatives and their experimental conditions; its historical claims do not
cover every subsequent container, view, or linearity amendment. Reopen the
provisional boundaries below when a representative program requires a safe
ownership/storage form that they cannot express, or when checking/lowering
costs prevent practical iteration. A model check of an earlier calculus is
not a soundness proof of the combined current rules.

| Rules | Grounds and limits |
|---|---|
| TYPE-1 | Explicit primitive domains support defined arithmetic and layout. The exact widths and inventory are provisional; safety does not uniquely select these ten numeric types or exclude wider integers. |
| TYPE-2 | Product/sum data, indirection, and bounded collections support systems programs. Their modes, regions, and storage must remain coherent. The exact nominal/container/provider inventory and restrictions on nesting are provisional, not the only safe representation choices. The [full-array choice](../containers-and-resources/FOUNDATION.md#selected-full-array-experiment) selects the complete owning element domain without widening copy-fill or const construction. |
| TYPE-3 | Finite writable type names make declared interfaces expressible. This follows conditionally from the selected explicit-interface model; the exact grammar and permitted type universe remain choices. |
| TYPE-4 | Defined conversions must not silently violate the promised value semantics. Prohibiting all implicit conversions is a stronger provisional policy: implicit value-preserving widening could also be safe. Explicit conversion makes loss/refusal visible, while OP-6 owns the numeric partition. Scoping `cvt` to numeric value conversion keeps that partition unchanged beside the explicit [full-array conversions](../containers-and-resources/FOUNDATION.md#selected-full-array-experiment). |
| TYPE-5 | Agreement with a selected RHS and exact interface types makes checking local and avoids use-site guessing. Deriving body annotations while retaining selected boundary/type arguments is provisional. Its uniformity and agent benefit need comparison, not an assumption that verbosity is free. |
| TYPE-6 | Name resolution needs a determinate declaration identity. Role-separated domains, declaration visibility points, forward function visibility, and no live-binding shadowing are provisional mechanisms; deterministic resolution does not require that exact collision policy. |
| TYPE-7 | Explicit dereference distinguishes a holder from its referent and makes reads/transfers visible. It is a selected authoring policy; implicit dereference can be sound in another typed design. Cell extraction remains a separate consuming operation. |
| SET-1 | Resolving targets once and revalidating after RHS effects prevents stale loan/liveness premises from authorizing a commit. Copy overwrite and LIV-2 affine read-out/reinitialization have different ownership obligations. Target-first order and the multi-target surface are provisional; alternative evaluation orders would need equally explicit semantics. |
| SET-2 | Atomic exchange with an explicit old owner preserves initialized storage and ownership without a visible hole. Region-free eligibility and its let-only spelling are provisional restrictions. The borrowed-place exception requires exclusivity and an intact replacement; it is not a general move-through-borrow permission. The [ordinary owner-transfer controls](../containers-and-resources/FOUNDATION.md#owner-identity-across-replacement-and-calls) distinguish the transferred values from the storage and loans that remain in place. |
| CONST-1 | Type-level sizes require a defined constant evaluation domain. The closed single-operation grammar, exact u64 rejection, and ordered symbolic identity are provisional complexity choices. Revisit with a needed composite size expression; external source generation is not evidence that this domain is sufficient. |
| CONST-2 | Read-only static values must be fully initialized and safe to share without mutable global state. The [focused assessment](../const-eval/INITIALIZATION.md) supports that consequence and distinguishes mandatory written coverage from safe defaults. This broader assessment also covers the eligible primitive/array/FixedVector/source-struct inventory, closed initializer grammar, no-drop storage, and excluded enum/heap/view forms: those are provisional scope choices, not consequences of complete initialization. FixedVector constants use full length and zero head; that representation convention is not a general runtime-container result. |
| OWN-1 | Unique affine ownership and controlled copying support memory safety without writer-managed aliases. Explicit move, whole-root death on partial consumption, the copy set, and the LIV-2 revival exception are provisional mechanisms. Whole-root death is not independently required by safety; a more precise partial-state calculus could be compared. |
| OWN-2 | Shared/unique permissions establish the selected aliasing invariant. Their three-mode vocabulary, spelling, and FORM-8 region elision are provisional representations. |
| OWN-3 | A borrow needs a valid lifetime relation. Lexical regions and incomparable caller regions provide a conservative finite relation; flow-sensitive or richer explicit relations are alternatives. The selected relation can reject valid programs and needs workload evidence. |
| OWN-4 | Under lexical loan lifetimes, holder scope and outlives checks prevent a borrow from escaping its authority. Keeping a bound loan to region end is conservative; shorter proven endpoints are an alternative. Call temporaries have their separate OWN-6 endpoints. |
| OWN-5 | Exclusivity must apply to ultimate storage through aliases, not just distinct holder spellings. Conflict checks, parent suspension, and finite view origins implement that requirement. The closed reborrow/origin forms are provisional; their combination needs evidence beyond the original ownership experiment. |
| OWN-6 | Resolving holders to origins prevents an alias from bypassing conflict checks. [The local-region experiment](../containers-and-resources/FOUNDATION.md#temporary-child-regions-and-statement-endpoints) separates formation/type ceilings from statement and nonescaping control-header endpoints: retaining the surrounding block does not retain the temporary loan, while returned borrows and views keep their independent claims. The unchanged lexical lifetime and suspension premises support that deduction; executable controls support the observed forms, not global soundness or optimality. Wider bound/result-carrying forms need their own argument. |
| OWN-7 | A sound overlap relation is required by the chosen permissions. Distinct fields and unequal literal indices identify disjoint storage; treating other index pairs conservatively is a choice. Symbolic disjointness proofs are a possible extension, not authority available today. |
| OWN-8 | Required safety cannot be waived when proof fails. Reject-and-explain implements that requirement. It does not justify arbitrary incompleteness or prove that a suggested restructuring is affordable. |
| OWN-9 | This rule explicitly states a non-normative optimizer consequence of proven unique access. The invariant may support noalias information; a particular backend encoding and runtime benefit remain to demonstrate. It is not an additional acceptance premise. |
| OWN-10 | Referent storage must outlive the borrow. The root classifications close that obligation for current storage families; new families need the same argument without pretending the present taxonomy is universal. |
| OWN-11 | Repeated execution must preserve admissible ownership at a backedge. Per-iteration lexical regions and structural status agreement are provisional mechanisms, not a ban on all loop-local borrowing or all mutation of outer values. |
| OWN-12 | Call substitution and loan/effect projection must preserve the caller's permissions. Exact signature-based substitution is the selected modular mechanism; richer result summaries could preserve safety with different expressivity. |
| OWN-13 | Moving an owned scrutinee and borrowing a borrowed scrutinee have different ownership effects. Derived payload modes follow that selected distinction; omitting written binder modes is a convention, not a safety theorem. Deferred parent/child forms must not be inferred from ordinary match syntax. |
| OWN-14 | Return-only reborrowing avoids a later local point using parent and child together, subject to the returned lifetime checks. Its restricted parent kinds and placement are provisional; it does not admit stored, give, or arbitrary bound children. |
| LIV-1 | The selected unconditional release model requires compatible live ownership on joining paths. Rejecting mismatched states is conservative; path-dependent cleanup could be sound with a different explicitly checked representation. |
| LIV-2 | A single commit can read out and reinstall affine targets without an observable vacancy. Target disjointness and exact read-out correspondence protect that argument. Complete-binding revival and the closed indexed matching relation are selected mechanisms, not arbitrary revival of dead projected storage. The [entry-dead initialization exception](../containers-and-resources/FOUNDATION.md#owner-identity-across-replacement-and-calls) avoids attributing a write to a value already moved elsewhere; same-statement read-out keeps its write. This amends the former universal commit-write sentence and is a provisional choice. |
| PROV-1 | Store identity must survive transfer if reclamation depends on the originating store. Region branding supplies a static identity without runtime tags. One store per brand and its elision forms are provisional; new stores must preserve the same identity obligation. |
| PROV-6 | An owner cannot be silently abandoned when its scope lacks the capability needed to reclaim it. Scope-relative linearity, explicit disposal/destructuring, and release-graph traversal implement this requirement. Bounds and recursive graph treatment are provisional mechanisms needing combined provider/nested-release evidence. The [full-array choice](../containers-and-resources/FOUNDATION.md#selected-full-array-experiment) extends that element closure and preserves its conservative zero-extent linearity; it does not add an empty-owner discharge. |
| BLK-0 | Trusted container operations need complete signatures, domain conditions, relations, and effects. One compiler-owned generic declaration domain is a provisional organization that keeps these records inspectable; it is not a writer-defined unsafe extension point. The [full-array conversions](../containers-and-resources/FOUNDATION.md#selected-full-array-experiment) use its ordinary operand-supplied type/const criterion and add no inference family. |
| BLK-1 | Initialized indexed storage and a defined window support efficient collections. FixedVector/Vector, modulo windows, and the exact element classes are provisional representations. The [full-array choice](../containers-and-resources/FOUNDATION.md#selected-full-array-experiment) additionally transfers a complete initialized set by consuming its source run. Existing successful programs or one representation-size comparison do not establish superiority across target workloads. |
| BLK-2 | Formation must prove size/alignment/domain conditions and preserve ownership on refusal. Provider operands and Option-shaped refusal are selected mechanisms; exact acquisition rows and per-activation arena limits remain choices to assess with resource-budget workloads. |
| BLK-3 | Boundary mutation preserves the initialized set and exposes two-state measures through the [selected exclusive interface](../containers-and-resources/FOUNDATION.md#exclusive-two-state-run-contracts). D1/D2 motivate removing owned run round trips, while full-array conversions retain genuine owned input/output. This evidence does not prove complete container coverage. |
| BLK-4 | The [exclusive two-state interface](../containers-and-resources/FOUNDATION.md#exclusive-two-state-run-contracts) removes the recursive run-referent refusal. Exact projected writes kill old state and verified contracts restore only promised state. The same storage-overlap kill handles whole replacement through an opaque type parameter, removing the entire exclusive-parameter refusal. Confinement and loan-bearing position judgments remain; richer generic provenance is a separate question. |
| VIEW-1 | A view must retain origin and access strength independently of descriptor ownership. Copy shared views and affine mutable views provide that information in the selected type model. Prohibiting stored views is a provisional expressivity cut. A compiler's flat-element implementation limit does not narrow VIEW-2's admitted operand domain; the [full-array experiment](../containers-and-resources/FOUNDATION.md#selected-full-array-experiment) keeps general view implementation as an explicit capability gap. |
| VIEW-2 | A produced view's loan must outlive the argument temporary that creates it. Origin transfer implements that requirement; the operation/operand inventory and unavailable child/range forms remain provisional. The [full-array choice](../containers-and-resources/FOUNDATION.md#selected-full-array-experiment) preserves array viewing and removes the obsolete premise that array retirement must precede a spelling migration; no new loan permission follows. |
| VIEW-4 | A commit must not leave a live view loan referring to a displaced owner. The same-statement consumption condition is one selected way to make displacement safe; it does not waive other commit checks. |
| VIEW-6 | Returned views require a signature-determined origin ceiling and valid access permissions. Restrictions on duplicate view results and child reborrows are conservative mechanisms; richer disjoint result summaries are a possible comparison. |
| STOR-1 | Storage, ownership, and reclamation must agree. Type-directed classes, compiler-owned providers, and no writer finalizers are provisional ways to make that agreement checkable; alternative explicit storage policies could also be safe. The [dense full-array representation](../containers-and-resources/FOUNDATION.md#selected-full-array-experiment) removes mandatory window metadata without promising zero-copy conversion or allocation adoption. |
| STOR-2 | Explicit construction and dereference expose allocation/access operations. The retained ambient allocation spelling needs reconciliation with OP-1 and EFF-1; this rule cannot establish general-store capability accounting by itself. |
| STOR-3 | Under the chosen ownership/release contract, every ordinary exit must dispose of remaining resources exactly as specified. Compiler-derived release, reverse order, and fixed resource-type policies are provisional mechanisms. The [full-array choice](../containers-and-resources/FOUNDATION.md#selected-full-array-experiment) visits exactly the N initialized elements in index order and adds no backing release. Resource exhaustion remains outside this cleanup guarantee under SCOPE-3. |
| STOR-4 | Region-owned arena storage cannot outlive its region, including through returned owners or borrows. The chosen arena type/confinement mechanism enforces that condition; richer safe transfer forms would need a different lifetime contract. |
| STOR-5 | Stored loans/providers require provenance and reclamation information that the current stored-value model does not carry. Recursive exclusion, with the store-branded exception, is a conservative boundary. Per-leaf provenance could permit more safe programs; current exclusion is not a proof that such designs are impossible or slower. |
| STOR-6 | Source-level index/size proof alone does not guarantee target address and layout representability. Target qualification before emission closes that obligation without narrowing values or adding hidden guards. The exact qualification organization is provisional; target failure remains distinct from invalid source. |

## Numeric operations

The operation table is the direct source for the following assessment. Its
recorded lowering choices are not newly verified against every backend here.
Reopen a row when a target cannot implement its specified edge semantics or a
real numeric workload needs a safe operation that the table excludes.

| Rules | Grounds and limits |
|---|---|
| OP-1 | Explicit operation identities expose numeric policy and prevent user overloads from changing the meaning of an operator. A closed table and reserved names are provisional mechanisms. The table's allocator rows and conflicting heap-effect prose need reconciliation with the provider-based model before relying on heap-unreachability claims. |
| OP-2 | Given exact integer semantics, proving a partial operation's domain before emission excludes its invalid cases. Wrap, checked, and saturating operations are explicit total policies, not silent exact overflow. Fixed-width bit shifts intentionally discard bits; this must remain distinguishable from exact arithmetic. The mode inventory, operand-derived typing, and closed widths are provisional. |
| OP-3 | Defined rounding and contraction behavior provide a stable numeric contract. Strict-only floating arithmetic is a provisional performance/precision choice; it must be revisited if a safe explicit relaxed mode is needed by a target workload. |
| OP-4 | Given the collection model, a logical index below length plus layout/address guarantees permits access to a live element without a runtime bounds check. The available proof families and exact offset type remain choices; bounds proof alone does not justify narrowing an address. |
| OP-5 | Requiring an actual Bool prevents implicit truthiness and fixes which expression is a condition. It is a conservative typing convention, not a proof of logical intent. Executed conditions and erased predicates retain different roles. |
| OP-6 | Exact conversion either preserves the value or reports a defined failure. The total-pair partition follows from the selected source/destination value sets; choosing exact conversion as the only conversion operation is provisional. Explicit rounding conversions could be safe additions. |
| OP-7 | Prefixes and mode suffixes make operation policies identifiable. Their exact partition, including symbolic integer exceptions, is conventional, not a unique deduction from regularity. |
| OP-8 | Every exposed primitive needs defined edge behavior consistent with its lowering, including zero counts, minimum signed values, NaNs, and signed zero. The listed intrinsics and instruction choices are provisional backend realizations. The sentence deriving floating reproducibility from FORM-1 conflates source spelling with value semantics; revisit that rationale without changing the numeric contract. |
| OP-9 | A count/stride bound plus target qualification justifies nonoverflowing allocation-size arithmetic. The conservative target-independent ceiling is provisional and may exclude allocations a particular target can represent. It proves representability, not available memory. Reopen on a measured ceiling-related rejection, not by adding a hidden runtime fallback. |

## Functions and contracts

The [contract record](https://github.com/mbbill/Whitefoot/blob/0f22b026b062c36ef2f8ce9c725ee4307e1fda64/mcts_mem/whitefoot/contracts.md) preserves the
choice of checked metadata before callable contract-member behavior. That
historical scope choice does not establish sufficiency for kernels, compilers,
or browsers. The following assessment covers the full rule boundaries,
including the facets outside the earlier source-proof review.

| Rules | Grounds and limits |
|---|---|
| FN-1 | A caller needs checked modes, types, effects, provenance, requirements, and promised result relations without trusting the callee's text. Finite interface summaries serve that purpose. The [normal-exit owner summary](../containers-and-resources/FOUNDATION.md#owner-identity-across-replacement-and-calls) uses ordinary direct/helper controls to extend result-only routing with exclusive referent outputs, instantiated from one entry image. The finite representation remains provisional, as do exact result ordinals, conservative slice ceilings, unique borrow-result suppliers, structural return checking, and once-captured counted iteration. Structural reachability deliberately differs from proof of actual reachability; a nonreturning call does not settle the internal origin encoding or erase structural effects. Revisit when a safe returned view or control-flow pattern cannot be expressed. |
| FN-2 | Concrete substitution and rechecking preserve the selected generic semantics. [D7 behavior selection](../containers-and-resources/BEHAVIOR.md#d7-selection-ground) admits explicit function arguments and retains opaque store brands without redundant receiver binders; loans and providers remain excluded. Monomorphization removes runtime generic dispatch. Finite instances alone do not meet the constitutional compilation-cost requirement; acyclic expansion size and practical checking cost remain open. |
| FN-3 | [D7 behavior selection](../containers-and-resources/BEHAVIOR.md#d7-selection-ground) makes formal/actual declarations flat, hygienic abbreviations of complete explicit parameter/argument vectors. Ordered declaration checking prevents missing or mismatched members from becoming interface authority. Group syntax is a selected convenience, not a deduction that types must own implementations. |
| FN-4 | [D7 behavior selection](../containers-and-resources/BEHAVIOR.md#d7-selection-ground) checks exact callable signatures and region domains, formal-prefix effect coverage, structurally equal source contracts and fresh non-copy function-formal results. These boundaries prevent an actual from silently adding obligations or returning a retained input owner. Structural equality and the fresh-result restriction are conservative choices; behavior laws grant no authority. |
| FN-5 | [D7 behavior selection](../containers-and-resources/BEHAVIOR.md#d7-selection-ground) uses explicit environments and monomorphized member calls, preserving the formal interface without runtime dictionaries, values or dispatch. The map and queue witnesses discriminate reusable behavior from type-owned lookup or closed enum dispatch. Indirect calls remain outside the selected design. |
| FN-6 | [D7 behavior selection](../containers-and-resources/BEHAVIOR.md#d7-selection-ground) extends cycle checking to functions, nominal instantiations and function arguments. Every recursive edge must preserve the complete type/const/function vector; this prevents unbounded instance discovery without fuel. Finite permutation cycles are also refused. It does not bound acyclic specialization cost or runtime recursion depth. |
| FN-7 | Explicit labelled entry inputs expose authority and avoid ambient system access. One command entry, its result type, row order, and exclusion of source calls to main are provisional executable-profile choices. A kernel or embedded entry model will need its own argument. The heap-unreachable claim needs reconciliation with the retained allocation rows in OP-1/STOR-2. |
| FN-8 | Caller proof followed by callee assumption is justified only when every ordinary call discharges the requirement; written assertions alone grant nothing. Erasure avoids executable prologues. Exact tree identity, the restricted pure-total clause language, independent clauses, and legal uninhabited bodies are provisional. Recursive definition expansion must preserve sharing or otherwise demonstrate feasible cost; finiteness does not bound expanded size by written size. |
| FN-9 | Proving every selected normal return before caller publication prevents self-certified postconditions. Earlier-SCC-only publication remains. The [exclusive contract amendment](../containers-and-resources/FOUNDATION.md#exclusive-two-state-run-contracts) adds exit-state measures and checks equality in both directions; unused ordinary results need not be relation datums. Other selector and recursion limits remain open. |

## Proof state and transport

The [source-proof assessment](../proof-certificate-architecture/SOURCE-CHECKING.md)
records the earlier focused argument and source-cost observations. This section
adds the previously unassessed vocabulary, normalization, joins, transports,
and generic policy. It does not convert a finite calculus into a proof of
practical cost or a complete proof system.

| Rules | Grounds and limits |
|---|---|
| ENT-1 | Specification-fixed checked evidence implements the chosen acceptance guarantee without solver-dependent verdicts. No-SMT is a provisional architectural choice: checked certificates from another search mechanism are a comparison, not a current feature. Checking unreachable and symbolic generic bodies avoids an unchecked textual path, but its conservative vocabulary can reject concretely safe instantiations. Practical full-function cost remains open. |
| ENT-2 | Explicit value identities prevent facts about one value from proving a claim about another. A finite term/goal universe makes closure definable. Exact expression identity, restricted tracked places, occurrence-local captures, and separate mutable support are provisional representations. Structural expansion can duplicate a shared expression exponentially if implemented literally; a size/sharing argument is still needed. |
| ENT-3 | Facts from executed edges, proved requirements, validated operations, and verified summaries have conditional semantic grounds. Only selected edges justify their predicates; merely computing a Bool does not. The closed source list, one-level comparison origin, complete goal-origin expansion, exact arithmetic-image rows, and publication placements are provisional completeness boundaries. Each added source needs its own soundness argument. Origin expansion shares ENT-2's size concern. |
| ENT-4 | Difference-bound transitivity, integer strictness, Boolean introduction, and contradiction have ordinary logical justifications under truthful premises. A finite closed universe and exact lookup are selected calculus boundaries. The statement that least closure is finite needs the contradictory-state convention: a negative cycle permits indefinitely tighter numeric bounds unless represented as the all-derivable state. No generic claim of cheap closure follows. |
| ENT-5 | Mutation must invalidate propositions about overwritten storage; facts about immutable past values can remain true. Closing before kills preserves consequences whose own support survives, and joins must retain only conclusions valid on every reaching path. Full pre-kill closure, interval joins, conservative loop kills, and restricted value-if delivery are provisional algorithms. Their repeated cost and missing value-match transport remain concrete reopening questions. |
| ENT-6 | Exact value images, sound interval substitution, nonnegative sums, and integer tightening justify the individual affine steps under their premises. The zero/one/two-premise families, i128 limits, delta-atom joins, L0 bridge index, and first-witness order are provisional choices. With P premises the pair family alone has P(P+1)/2 candidates per query, before normalization and closure costs; practical scaling remains open. Product publication is justified by its proved operand intervals, not a general nonlinear theorem facility. |
| MSR-1 | Length/capacity/window measures expose the facts needed for safe container operations. For positive capacity and length no greater than capacity, the logical-to-physical map is injective on the initialized range; at zero capacity that range is empty and no address is formed. The four-measure inventory, ring coordinates, and bounded head publication are provisional. Unsupported measure-place offsets are a capability boundary, not evidence of invalid source. |
| MSR-2 | Descriptor mutation can change a measure; element mutation does not change the outer descriptor but can change a measured element's descriptor. This justifies storage-granular support and kills. Standing invariants remain valid only for live, well-formed values. The precise table of exact versus bounded facts is selected representation data, not a new inference from a spelling such as “element.” |
| MSR-3 | Immutable entry/call/placement images relate states without resurrecting dead storage. The [exclusive contract amendment](../containers-and-resources/FOUNDATION.md#exclusive-two-state-run-contracts) gives bare exclusive measures exit meaning and entry(name) entry meaning. The same-time increment negative distinguishes these meanings. Other placement and payload transport limits remain provisional. |
| MSR-4 | One disposition shared by consumers prevents accidental differences in proof authority. Each numeric normalization must imply the governed operation's domain. Fixed interval products and affine/L0 bridges are provisional completeness choices; they do not publish arbitrary nonlinear relations. The repeated AUTO calls make total cost larger than one premise scan. Revisit jointly with ENT-6 and PRF-1. |
| MSR-5 | A common erased relation surface can state measures directly without runtime arithmetic or a separate definition per measure. Sharing grammar does not imply equal admission in requires, ensures, and invariant positions. The operand sets and affine restrictions remain provisional. |
| MSR-6 | Treating integer const generics as immutable symbolic constants preserves their identity during schema checking and substitutes their values at concrete instantiation. Their exact positions and literal-only multiplication distinction are provisional expressivity choices. |
| CALL-1 | Under the ban on shared-reference mutation and the exact write-effect check, a shared borrow supplies no write kill on its referent. This consequence must be reassessed if interior mutation or new write classifications are introduced. |
| CALL-2 | Consuming an affine actual invalidates facts tied to its old place; a copy actual does not consume it. A new result needs verified relations, not an assumed identity with the input. Contract-only result transport is a provisional modularity boundary; immutable call images carry only explicitly related past values. |
| CALL-3 | An operation confined to element storage cannot overwrite the outer descriptor, but can overwrite descriptors belonging to elements. This is a conditional storage argument, not permission inferred from an argument's syntax. The declared extent and view model must justify each application. The [full-array amendment](../containers-and-resources/FOUNDATION.md#selected-full-array-experiment) removes the obsolete flat-only description while retaining that exact inner-descriptor kill judgment; a general view implementation must enforce it. |
| CALL-4 | Result ordinals and unambiguous routes identify the value a relation describes. Ordinal naming, Ok-only routes, bare result measures, and selected delivery sites are provisional. The deferred route and projection extensions are explicit open expressivity questions, not extra accepted forms. |
| CALL-5 | Declared interfaces select transport and exact projected writes kill overlapping support, independently of actual spelling. The [exclusive contract amendment](../containers-and-resources/FOUNDATION.md#exclusive-two-state-run-contracts) replaces the by-value-only run strategy with verified two-state relations. A whole-referent replacement remains the discriminating negative; body-derived framing remains unavailable. |
| CALL-6 | Instantiation, transfer, kills and establishment refer to the correct state. The [exclusive contract amendment](../containers-and-resources/FOUNDATION.md#exclusive-two-state-run-contracts) uses call datums for entry terms, resolved places for exit terms and one normal-continuation publication for result-free clauses. Consistency checking supplements return proofs; it does not replace them. Existing result and route restrictions remain. |
| INV-1 | Base and arbitrary-backedge preservation justify loop induction; a local target requires proof at its own program point. Exhaustion substitution additionally needs the captured bound and exact update, and cannot be exported across an unproved break. Affine inequalities only, named local placement, no header proof blocks, i128/4096 formation ceilings, and canonical image intersection are provisional. These restrictions and their practical costs remain open for target workloads. |
| PRF-1 | Independently admitted premises, nonnegative scaling, checked product folding, and a proved residual justify a target without trusting writer assertions. Declaration identity prevents a later value from impersonating the cited one. Local-only blocks, duplicate refusal, 4096 uses, exact fold carriers, and rejection when AUTO already proves the target are provisional. Redundancy rejection adds checking and editing work without strengthening the proof; changing it remains a separate language decision. Total checking is not established linear in the use count. |

The cost questions above have a concrete trigger: before claiming feasibility
for target-scale software or enlarging the corresponding families, account for
written source size, instantiated size, retained DAG size, coefficient width,
live terms, and query count separately. Measure representative and adversarial
scaling. A fixed ceiling on one expression or one block does not bound a whole
program's work; an implementation resource failure must not be reported as a
source rejection. This assessment does not select a budget, timeout, solver,
or new acceptance rule.

## Effects, execution, and system interfaces

These arguments use the active EFF through SYS tables and the
[system-interface rationale](https://github.com/mbbill/Whitefoot/blob/0f22b026b062c36ef2f8ce9c725ee4307e1fda64/mcts_mem/whitefoot/system-interface.md).
Earlier capability-category and trap-era explanations are not current grounds.
The exact system inventory is a provisional executable profile, not evidence
that kernels, browsers, or embedded resource budgets are already covered.
Reopen a profile when a required host operation, observation, or failure cannot
be represented without weakening its contract. Qualification evidence must be
specific to the target and implementation being used.

The constitution's [ordinary-object requirement](../../../docs/constitution.md#external-interaction-through-ordinary-objects)
fixes uniform source treatment of I/O and other resources. It does not select
one object decomposition, result-origin analysis, or release implementation.
This is a chosen architectural constraint, not a theorem uniquely implied by
performance or safety alone.
A diagnosis must distinguish an API that omits a required relation from a
general language or compiler limitation by checking the corresponding ordinary
object case. A resource-specific example alone does not justify a special
ownership or effect rule. A current compiler limitation does not demonstrate
that the ordinary model cannot express the required behavior. Native adapters
may implement host-specific operations, but must honor their complete
source-level contracts without hidden I/O-specific permissions or dependencies.

| Rules | Grounds and limits |
|---|---|
| EFF-1 | Formal-rooted state paths expose a callee's accesses and allocation authority for composition. Exact category equality and canonical order are provisional; safe upper-bound rows are an alternative. Ambient box/buffer allocation explicitly contributes no written row, yet OP-1 still prints `allocates(heap)` without a provider parameter. These two descriptions need reconciliation. |
| EFF-2 | Body accesses, call projection, normal-exit state routing, and derived releases must account for the effects the selected model promises. The [ordinary replacement evidence](../containers-and-resources/FOUNDATION.md#owner-identity-across-replacement-and-calls) requires later effects to follow the installed owner while call effects use entry identities; nested-content representation remains open. Both-ways equality and framing fresh local state out of the callable boundary are provisional mechanisms. A framed-out effect is not thereby unobservable or freely eliminable. |
| EFF-3 | Equal-argument pure, never-suspending calls have the rule's selected reordering/deduplication permission; unused-call elimination additionally needs termination evidence and is unavailable in v0. Purity alone proves no termination or algebraic law. Its statement that pure excludes allocation needs reconciliation with EFF-1's ambient allocation treatment; a complete transformation argument must retain ownership, observations, and control behavior. |
| EFF-4 | No writer abort/trap/proof fallback follows from the chosen safety policy. Prohibiting all exception/unwinding mechanisms is a stronger selected execution design; a different defined recoverable-error mechanism could be safe. External resource/TCB failure is not a source effect or a proof bypass. |
| EFF-5 | Explicit ordinary state parameters prevent hidden mutable system access and let ownership/effects govern interaction. Uniform source treatment is a constitutional requirement; the exact state-object decomposition and host mapping remain provisional choices. Native aliases do not change language-place identity, and parallel/reordering permissions still need their complete observation argument. |
| ERR-1 | Expected failures need defined behavior. Result/Option with explicit dispatch is a provisional encoding; safety alone does not rule out every other total error representation. |
| ERR-2 | Every reachable variant needs defined handling. Enumerating all arms without a wildcard makes additions visible to writers, but is a stronger provisional policy than exhaustiveness alone. Bool's if-only and empty-else conventions are surface choices, not consequences of error safety. |
| ERR-3 | Propagation must transfer payload ownership and perform the ordinary error exit/cleanup exactly once. Let-only syntax, exact error-type equality, and automatic context metadata are selected conveniences; propagation grants no new proof or borrow escape. |
| ERR-4 | Separating recoverable outcomes, invalid source obligations, external failure, and unavailable optimization permission prevents one from disguising another. The selected table classifications remain provisional at the resource boundary. Failure to prove optional overlap retains sequential execution, not source rejection. |
| PROG-1 | A closed unit supplies the definitions needed by current whole-program analyses. It is a provisional compilation model, not a necessity of safety. Whole-unit processing and specialization need a practical scaling argument; the resource-closed profile must not be confused with unrestricted command programs. |
| PROG-2 | Ordered logical source records give deterministic unit identity and diagnostics independent of incidental filesystem traversal. The record format and ordering are provisional tooling choices. |
| PROG-3 | Startup, one entry invocation, normal status, and pre-entry failure require defined boundaries. The command lifecycle is a provisional hosted execution profile; runtime-start refusal is not a runtime language trap or an accepted fallback for missing proof. |
| DIAG-1 | Distinguishing source rejection, unsupported capability, and invocation failure makes repair feedback truthful. Rule/node attribution and deterministic selection support reproducible agent repair. The exact diagnostic inventory and mandatory restructuring text are provisional; their effect on repair success remains unmeasured. |
| DIAG-2 | Lowering must consume the successfully checked program, not treat a diagnostic record as independent authority. Retained goals, provenance, and derivation parents make decisions inspectable. The concrete artifact schema is provisional, and internal consistency checks do not prove the checker sound. |
| CAP-1 | Reusing ownership and overlap avoids competing permission models. This is provisional architectural reuse, conditional on the operations and concurrency model actually covered; it does not prove arbitrary external race freedom. |
| PAR-1 | Permitted overlap must preserve loans, effects, dependencies, exits, and observations of the accepted sequential program. Using existing place/effect identities is a provisional mechanism. No permission is required merely to accept the sequential source, and target scheduling still needs its own protocol proof. |
| PAR-2 | Counted-loop overlap needs independence across iterations. The closed affine footprint and exit conditions provide a conservative candidate proof; they are not a general parallel-loop calculus. Revisit on a useful rejected parallel workload or measured overhead. |
| PAR-3 | Staged overlap requires compatible footprints and preserved producer/consumer order. The selected staging and resource restrictions are provisional; safety of the sequential source alone does not license speculation or resource replication. |
| GATE-1 | A trusted implementation boundary must not let ordinary source inject unchecked operations or facts. “Privileged” editing of contracts/signatures is ambiguous between that boundary and authoring workflow. Current repository work branches need no approval; this paragraph must not create a competing permission process. Revisit the language/toolchain boundary wording with SCOPE-1 and LEDGER-1. |
| LEDGER-1 | Foreign/unsafe implementation obligations need explicit identity and scope. A human-approved gated family is a selected trust-management mechanism, not a machine proof and not today's repository approval ledger. The retained wording needs clarification without making writer unsafe available. |
| GATE-2 | Compiler-owned system operations have specification-fixed contracts and qualified target implementations; they are not writer-authored gated FFI constructs. Keeping that distinction prevents a system call from silently admitting arbitrary foreign calls or imposing the separate foreign-code condition on every kernel program. The domain organization is provisional; adding an operation requires a specification amendment, not merely a target implementation or human approval. |
| PRE-1 | Prelude declarations give Bool, outcomes, numeric markers, and system errors a stable nominal identity. Their exact inventory and field shapes are provisional; Bool-as-enum does not independently prove the selected if/match distinction. |
| SYS-1 | A compiler-owned system declaration domain gives operations a checked lookup/interface home. The single-domain and naming policies are provisional; they must retain explicit state inputs and avoid ambient mutable authority. |
| SYS-2 | Complete signatures, effects, results, and failure contracts are needed for callable system rows. The particular operation inventory and parameter forms are provisional. Table completeness is a consistency obligation, not proof that every target can implement each row. |
| SYS-3 | Unconditional system-name admission keeps declaration lookup independent of entry validity. That is a provisional frontend organization; it supplies names, not permission to perform operations without the required inputs. |
| SYS-4 | Applying the same ordinary state/ownership judgments to system values avoids a second capability classifier. This follows within the chosen unified model; a separate capability type system could be another design. |
| SYS-5 | Every resource type needs a defined exactly-once release policy consistent with its effects and milestones. The selected compiler-derived release policies are provisional, particularly where late close/writeback errors cannot be returned. They do not guarantee successful persistence. |
| SYS-6 | Per-operation outcomes avoid claiming variants an operation cannot produce. Separate result shapes are provisional; a broader typed error sum could be safe but may obscure relevant handling. |
| SYS-7 | Portable errors need defined mapping and preserved required information. A closed IoError inventory and its field widths are provisional profile choices. New host failures must be mapped honestly or fail qualification, not silently coerced to success. |
| SYS-8 | Range obligations must establish a valid logical half-open extent before host access. One-attempt semantics, empty-range behavior, and progress results are selected contracts that prevent hidden retries from changing work/latency. A caller remains responsible for its intended retry policy. |
| SYS-9 | Direct host access to a checked range can avoid copies when encoding, lifetime, and target representation all permit it. The inline path and supported families are provisional. No universal zero-copy performance result follows from the interface alone. |
| SYS-10 | Finite host-handle acquisition needs authority backed by actual capacity. One-shot credit-backed permits account for it explicitly; their protocol is provisional and target-qualified, not a proof that all host resources are bounded. |
| SYS-11 | Positioned reads separate the selected offset from shared cursor mutation. This is a provisional API design; concurrent external file changes remain part of the host contract rather than stable in-memory facts. |
| SYS-12 | Output operations need exclusive source-state transitions and defined progress/failure. The selected output/release contract may lose late writeback diagnostics; it must not be presented as a durability guarantee. |
| SYS-13 | An opaque validated exit status prevents unrepresentable host status values. Its constructor and range policy are provisional target-interface choices. |
| SYS-14 | Directory enumeration needs defined entry identity, names, progress, and failure. The record layout and operation split are provisional; qualification must implement the promised semantics without pretending arbitrary path traversal is confined. |
| SYS-15 | Input-stream position advances as explicit owned/unique state. The current read interface and release behavior are provisional; seek, framing, and additional stream kinds require separate contracts. |
| SYS-16 | Address construction must yield valid opaque addresses or defined errors. Numeric constructors and their exact spelling are provisional; they do not constitute an implemented name-resolution policy. |
| SYS-17 | Listener/accept operations must preserve permit accounting, loans, and result ownership. The selected accept interface is provisional, with concurrency and completion behavior requiring target-specific qualification. |
| SYS-18 | Connection halves and their shared release obligation must have a coherent ownership lifecycle. The paired struct and shutdown/release order are provisional mechanisms, not a general proof of application-level network correctness. |
| HOST-1 | Host names must not silently normalize, truncate, or replace code units. Lossless target-indexed representations implement that objective; the two-family closure is provisional target coverage. |
| HOST-2 | UTF-8 conversion must report inability to represent the input under the selected lossless contract. Its explicit fallible form is provisional; it must not be confused with lossy display conversion. |
| HOST-3 | An inline lease requires backing storage that outlives its use and a matching host representation. A distinct type is a selected way to expose those conditions; absent backing cannot be repaired by a lifetime assertion. |
| PATH-1 | Relative-path construction needs explicit parsing and failure semantics without hidden text normalization. The selected opaque path and constructor inventory are provisional. |
| PATH-2 | Directory-relative lookup does not imply confinement through parent components, links, or mounts. Stating that limitation prevents an invented security guarantee. A confined API would require a separate contract and qualified implementation. |
| QUAL-1 | Target implementations must preserve the same semantic operation identity and contract. Fail-closed qualification protects that boundary; profile IDs, tables, and adapter organization are provisional mechanisms. |
| QUAL-2 | Properties not established by source checking must hold before the target executes the governed operation. Qualification/startup refusal is a selected boundary, not a source-language rejection or hidden fallback. Its evidence is target-specific. |
| QUAL-3 | Static selection can preserve a direct hot path after qualification. The mechanism and its performance benefit remain provisional; emitted-code inspection and representative measurements are needed for a runtime claim. “Approved implementation” denotes technical qualification, not an additional repository approval step. |

The allocation inconsistency is narrow but significant: EFF-1 distinguishes
ambient box/buffer storage from the provider-backed general store. Therefore
FN-7's `heap-unreachable` means the latter, not “this program cannot allocate
anything.” OP-1's retained `allocates(heap)` rows and EFF-3's allocation-free pure
wording still need a consistent account. This assessment records the conflict;
it does not choose which allocation model to change.

## Specification maintenance

| Rules | Grounds and limits |
|---|---|
| EX-1 | A checked worked example makes the rules inspectable and catches drift between normative bytes and parsing. Its particular algorithm is illustrative, not evidence of language coverage, common syntax, or performance. |
| META-1 | Unique rule IDs and resolvable references support unambiguous navigation. Machine checks can establish these structural properties, not semantic regularity or soundness. Exact gate organization is provisional. |
| META-2 | Predictable rule selection is a useful goal. The absolute wording that meaning never depends on context is incompatible with ordinary scoping and too broad for FORM-8, MSR-3, and proof-versus-runtime arithmetic. Revisit its formulation as a precise convention; it cannot override those explicit rules. |
| META-3 | Positive tables can make cases easier to inspect, but prohibiting all exceptions is an editorial choice. Current rules explicitly contain exceptions and position-dependent admissions. Revisit the blanket statement rather than treating a table rewrite as proof that the conceptual complexity disappeared. |
| META-4 | One owning statement per normative fact reduces contradictory copies. This is the existing provisional documentation convention, not a guarantee against drift; examples and explanations must remain subordinate to their owner. |
| META-5 | Recording a change and its grounds makes revision assessable. The old evidence-selected/minimality-selected binary omits conditional deductions and provisional tradeoffs now distinguished by META-6. The ban on version commentary also conflicts with retained active historical narration. Revisit that editorial contract; no new approval ledger is implied. |
| META-6 | One scoped grounds pointer per rule makes missing and contested reasons visible. The gate checks coverage and vocabulary, while review checks meaning. The earlier workflow assessment remains provisional until actual retrieval and maintenance experience supports it. |

The current scope is assessment and documentation correction. The open wording
and language-design questions above must not be closed by quietly changing the
compiler, test expectations, or a normative admission. The index distinguishes
those questions from work that has not been assessed at all.
