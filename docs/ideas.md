# Research ideas and open questions

Status: NON-AUTHORITATIVE DETAIL.

This file preserves candidate mechanisms, unresolved research questions, and
possible experiments. Follow-up work found during design or implementation,
including validation of unverified improvement opportunities, belongs in
[todo](todo.md). A selected investigation belongs in
`research/investigations/`, and settled choices in
`design/`, under the reading and workflow rules in [AGENTS.md](../AGENTS.md).
Current capabilities are what the conformance report states. An idea here
does not change the language or select implementation work.

Whitefoot keeps facts in source that other languages discard. LLVM optimization
is one consumer of those facts. The ideas below ask whether the same checked
facts can also buy portability, automated tuning, stronger testing, safer
interop, or tighter deployment controls. They also ask what those guarantees
cost to author and check. Historical experiments retain their original
conditions; they do not establish current compiler capabilities or a general
advantage over another language. The [constitution](constitution.md) owns the
objectives against which a candidate would be judged.

## Candidate directions

### Proof-derived optimizer facts

Checked bounds, effects, usable exclusive borrows, and algebraic laws may
carry information an optimizer cannot recover from ordinary emitted code.
The question is which exact fact improves a real program beyond its ordinary
lowering, and at what checking and compilation cost.

Candidate consumers include memory-effect attributes at opaque call
boundaries, alias metadata for loaded data pointers, and regrouping a
reduction whose exact operation domain supports the required laws. Each needs
its own mapping from a checked proposition to a backend consequence under the
active ABI. `pure` does not prove termination or justify `willreturn`.
Likewise, checking a law for source acceptance does not grant every
transformation that mentions that law: operand domains, ordering, effects,
and intermediate arithmetic must all survive the proposed rewrite.

The historical [bounds](../research/experiments/port-study/base64/RESULTS.md),
[effect](../research/experiments/effect-attrs-channel/RESULTS.md),
[alias](../research/experiments/scoped-alias-channel/RESULTS.md), and
[checked-law](../research/experiments/checked-law-channel/RESULTS.md)
experiments retain concrete workloads and controls from the retired compiler.
Their ABI, proof rules, and measured ratios are not current compiler results.
The effect experiment used a separate totality derivation; the alias and law
experiments include conditions where an expert baseline erased the advantage.

First experiment: identify one lost fact in a real hot path, keep the accepted
source fixed, and compare one optional consumer enabled and disabled. Inspect
the expected IR and final instruction change, use the
[differential checks](#fact-consumer-differential-checks) below, and measure
runtime and compilation against a useful expert baseline. Stop if ordinary
lowering already recovers the fact or if its consumer adds no measured value.
A termination proof is a separate candidate only when an optimization or
resource-bound consumer actually needs it.

### A portable C backend

Whitefoot could emit C from the same checked typed IR that feeds the LLVM
backend. Users would write Whitefoot, retain Whitefoot's safety rules, and use
C toolchains to reach platforms that lack a supported LLVM path.

Standard C cannot express all LLVM properties. Per-instruction alias scopes,
`dereferenceable`, precise memory effects, `willreturn`, and several arithmetic
facts have no exact portable-C spelling. A sound backend therefore needs one
recorded disposition for each fact:

1. consume the fact while generating C;
2. encode it in portable C;
3. encode it in a reviewed compiler profile such as Clang, GCC, or MSVC; or
4. reject that target backend when it cannot preserve the already-checked
   operation without undefined behavior.

The backend must generate defined C. It must avoid signed overflow on every
admitted execution, out-of-bounds pointer formation, invalid shifts,
unjustified `restrict`, padding reads, alignment violations, and assumptions
that can become undefined behavior. The portable profile would promise correct
compilation. Named compiler profiles could make separate, measured performance
claims.

First experiment: lower a small corpus that covers bounds discharge, exclusive
borrows, exact and fallible arithmetic, and effect attributes. Compile it with
two C compilers, compare values, typed failures, and cleanup with the LLVM
backend, run C sanitizers, and inspect both assembly and throughput. Stop if
the backend needs an unreviewable undefined-behavior assumption or cannot
preserve a checked operation.

### Proof-guided autotuning

An autotuner could generate several implementations of one checked operation:
different data layouts, unroll factors, SIMD widths, branch structures, or
specializations. The checker would admit variants after proving that they
preserve the source contract. A target benchmark would choose among them.

This separates semantic authority from cost selection. The benchmark runner
may choose the fastest proved variant, but benchmark noise cannot make an
unsafe variant legal.

First experiment: choose one bounded kernel with a scalar reference and two
plausible fast shapes. Freeze the input distribution and target, verify every
variant against the same differential corpus, then measure whether target
selection beats a fixed compiler choice without expanding the trusted base.
Possible variant families include record layouts and interpreter dispatch
structures. Inspect the final emitted shape: source-level duplication that
optimization folds away supplies no comparison between runtime strategies.

### Parallel grain policies and runtime profiles

The [compute-model trials](../research/investigations/compute-model/DESIGN.md#runtime-extent-trial-result)
show that exposing more parallel work can improve wall time while increasing
CPU cost, and a more literal work estimate can remove a useful split. Can one
policy serve a broad workload range, or should selection depend on workload,
input dimensions and the machine? These results do not settle that question.

Runtime profiles could record task duration, steals, idle time and input
shape for later profile-guided compilation or online scheduling adaptation.
Compare those candidates with static and captured-extent estimates, including
profiling cost, unseen inputs and wall/CPU tradeoffs. Profiles select among
already legal schedules; they cannot authorize a source operation or replace
a required proof. Offline PGO and online adaptation remain alternatives for
the dedicated study tracked in [TODO](todo.md), not selected mechanisms.

### A proof-gap performance coach

The compiler could explain each unproved static obligation and missed
optimization in terms the writer can act on. A report might say
that an index is rejected because no dominating fact proves
`offset + 16 <= len`, or that a loop cannot vectorize because two live places
may overlap. Required safety proofs and optional optimization opportunities
need distinct explanations.

An automated tool could propose a rewrite constrained to the canonical
patterns, run the checker and performance protocol, and present the source
diff, proof delta, and measurement for human approval. The tool would never add
an assumption or weaken a contract or obligation. It would change source
structure until the checker derives the needed fact.

First experiment: select ten rejected proof gaps or missed optimization
opportunities, generate one mechanical suggestion for each, and measure
suggestion validity, proof closure, code-shape change, and runtime change.
Preserve every failed suggestion as a regression for the diagnostic or rewrite
rule that produced it.

### Multiple backends as mutual oracles

Independent LLVM, C, and future WebAssembly backends could compile the same
checked program. A differential runner would compare values, typed outcomes,
external effects, and resource teardown. Each disagreement would produce the
smallest practical regression before a backend fix closes.

This approach can catch a lowering defect that source conformance misses. It
also gives the C backend value before its generated code reaches the LLVM
backend's performance.

First experiment: run the existing codegen corpus through LLVM and portable C.
Compile the C at low and high optimization levels with two compilers. Require
the same result and typed-failure class for valid inputs and boundary cases,
including failure paths.

### Safe C ABI capsules

Whitefoot could package a module as generated C plus a generated header. The
header would expose opaque validated handles, constructors, operations, and
drop functions instead of internal pointers or layouts. Boundary code would
validate lengths, tags, handle generations, ownership transitions, and error
paths before Whitefoot code receives authority.

This would let a C program consume a Whitefoot library without asking the C
caller to reproduce Whitefoot's lifetime and alias rules. Arbitrary C code can
still corrupt its own process, so stronger isolation would require a process or
sandbox boundary.

Calling an opaque foreign binary poses the reverse boundary question. It
needs an explicit contract for ownership, layout, callbacks, foreign threads,
failure, and what foreign behavior the compiler must trust. A checked wrapper
and a source-level replacement are alternatives to compare for one real
dependency. Compiler-owned system operations do not establish that a general
FFI is specified or implemented.

First experiment: export one stateful component through an opaque-handle API.
Generate misuse tests for stale handles, double drop, overlapping buffers,
short outputs, and allocation failure. Require deterministic rejection and no
partial mutation on each failing call.

### A C-to-Whitefoot assumption extractor

A migration tool could ingest a restricted C kernel and identify the
assumptions that make it work: bounds assertions, `restrict`, signed-overflow
expectations, alignment, object lifetime, and unchecked pointer arithmetic.
The tool would translate supported code into Whitefoot and turn each assumption
into an explicit checked obligation. It would reject code whose behavior
depends on an assumption Whitefoot cannot state or prove.

The extractor should favor an incomplete translation over invented semantics.
Its main artifact would be an assumption ledger that a reviewer can inspect
before approving the translated Whitefoot source.

First experiment: select a small, defined-behavior C loop with one bounds
contract and one alias contract. Mutate the contracts one at a time and require
the extractor or Whitefoot checker to reject the corresponding program.

### Effect-derived sandbox policies

Given a qualified mapping from system operations and resource authority to
host restrictions, the compiler could derive a sandbox manifest from checked
effects. A deployment tool could translate that
manifest into a WASI capability set, syscall policy, filesystem allowlist, or
network policy for a named platform.

An ordinary effect row alone does not identify every runtime path, endpoint,
or foreign call. The mapping and its limits are part of the experiment.
Platform policy generators would consume checked authority and fail closed
when the platform cannot represent a restriction.

First experiment: define a tiny abstract effect set and one sandbox target.
Generate policies for pure, read-only, and network-using fixtures. Mutation
tests should add one hidden effect at a time and require either a broader
manifest or compiler rejection.

### Storage transitions and representation privileges

A real container can expose a missing transition even when its element type
and ordinary access are expressible. Evaluate growth and backing replacement,
move-out and destruction order, initialized versus vacant slots, sparse
occupancy, stable versus recyclable identity, stale handles, and relocation
under live loans separately. These are questions to check against the active
language and the program, not a list of mechanisms presumed absent.

The [rejected owning-sequence experiment](../research/experiments/data-layout-owning-sequence/RESULTS.md)
shows why a passing prototype or a fast layout does not settle affine element
use, initialization, target layout, and complete lifecycle behavior.
First try ordinary source functions over the existing storage operations. If
a concrete representation remains impossible, state the minimal additional
privilege and the exact invariant a deterministic checker would have to
establish, including failed construction and cleanup. Compare it with the
ordinary representation under the same workload; stop if the benefit does
not justify the extra proof boundary. No general privileged proof system or
sealed container catalog is selected by this question.

### References as names, storage as windows

The [access-effects investigation](../research/investigations/access-effects/RESEARCH.md) redesigned ownership and references from the requirements up. Its result, [candidate x1](../research/investigations/access-effects/CANDIDATE-X1.md), keeps references as local names for paths that are never stored or returned, states write permission in effect rows rather than reference modes, checks calls pairwise for path overlap, uses one global heap with an affine `Box`, and gives dynamic storage a compiler-maintained window (`Array`, `Slots`, `Ring`). Two derivation rounds over the frozen rules found only wording defects; the recorded costs are listed at the end of the candidate. The next steps are a specification amendment and owner rulings into the design tree; the [bounded model](../research/experiments/access-state/RESULTS.md) of the earlier alias/state fragment is superseded and kept as an experiment record.

### Wider parallel proof domains

A useful extension starts with a real loop whose required independent work
the current proof domain cannot express. Candidate pressure includes symbolic
strides and disjoint subranges, multiple access maps, projected callee
accesses, and regrouping operations outside the admitted reduction domain.
Each requires an exact deterministic rule with counterexamples for overlap,
effects, operation domains, and changed output order. An associative
non-commutative operation or a floating-point reduction cannot inherit a rule
whose argument also requires commutativity and exact bytes.

The [parallelism study](../research/experiments/auto-parallelism-feasibility/RESULTS.md)
and [loop investigation](../research/investigations/proof-derived-parallelism/loop/DESIGN.md)
preserve the earlier witnesses and limits. A broader domain should be tested
against the same sequential source, with runtime grain, skew, locality, and
bounded task/completion storage measured separately from proof permission.
Dynamic fan-out also needs ownership and resource accounting for every live
task, result, and cancellation path. Stop if the proposed domain does not
unlock useful work or ordinary execution is cheaper.

### Narrow semantic domains and automatic niches

AI-written code should choose the narrowest honest type that contains every
legitimate value. It should not default to a broad integer for convenience, and
it should not manually reserve a sentinel value. Existing narrow integer types
already help when the domain fits them. A future language experiment could add
compiler-checked refined integers for domains such as “nonzero `u64`” or
“`u64` from zero through `2^63 - 1`.” Any syntax used to express those domains
would be a separate language decision; these examples describe semantics only.

The compiler would make the refinement invariant unforgeable. Construction and
conversion would require a static proof or a typed fallible constructor for an
expected invalid input. An asserted invariant would need a checked source
proof, with no runtime proof fallback.
Arithmetic would preserve the refined type only when its result remains in the
declared domain. The layout pass could then derive invalid bit patterns
automatically and use them as enum niches:

- `Option<nonzero u64>` could encode `None` as zero and every nonzero bit
  pattern as `Some`.
- `Option<u64 constrained below 2^63>` could use the high bit to distinguish
  `None`.
- An unconstrained `Option<u64>` still cannot fit in one `u64`; all `2^64` bit
  patterns are valid payloads, so reserving one would lose a legitimate value.

There is no universally best physical layout. A standalone value, a packed
record, and a collection with a separate presence bitmap may need different
representations. The semantic type supplies the valid-value set; the compiler
chooses and measures the context-appropriate representation without changing
that set.

The writer guidance should be: choose the narrowest type that contains every
legitimate value, but never invent a bound from a practical assumption. For
example, an offset remains `u64` unless its specification or enclosing type
actually establishes a smaller range. A compiler diagnostic could suggest a
narrower domain when a declared contract or proof supports it, but the compiler
must not silently narrow program semantics.

The first experiment should use a real bounded identifier or offset domain and
compare an ordinary `Option<u64>` with a refined representation for size, ABI,
code generation, and measured throughput. Boundary tests must show that every
valid value round-trips and every invalid construction is rejected. Stop if
maintaining the refinement costs more complexity or runtime work than the
measured layout benefit justifies.

### ML systems components

Candidate entry points include CPU request handling and batching, data
pipeline components, and numerical kernels whose users need reproducibility
or checked shape relations. A library that fits an existing Python application
offers a smaller experiment than replacing an ML stack. The
[PyO3 guide](https://pyo3.rs/main/index.html) supplies a concrete native-extension
baseline; a Whitefoot experiment would need its own checked boundary, including
buffer ownership, lifetime, errors, and crossing cost.

Possible guarantees include shape and dtype contracts, checked use of random
keys, and reproducible reductions. JAX documents
[purity and static-shape constraints](https://docs.jax.dev/en/latest/notebooks/Common_Gotchas_in_JAX.html)
and [random-key reuse](https://docs.jax.dev/en/latest/101/random.html).
These are concrete comparison problems, not evidence that Whitefoot already
solves them or that they dominate AI-written ML failures. Compare against the
baseline's available checks as well as its unchecked configuration.

First experiment: one callable CPU component with fixed valid inputs, an
independent result oracle, and a measured boundary. Separate proof and repair
cost, cold and repeated compile-run latency, runtime, and any numerical
reproducibility cost. Stop if integration or copying consumes the proposed
benefit. A tokenizer or serialization rewrite would need an advantage over an
existing safe library or format; a language change alone is not that advantage.

GPU execution and automatic differentiation are separate questions. A bounded
GPU probe could compare generated source passed to a vendor compiler with a
direct backend route. An autodiff probe would pin the LLVM/tool versions and
exercise diagnostics and compiler failures as well as derivatives. Reduced
mutation, strict arithmetic, or proof erasure does not by itself establish
cheap differentiation or competitive deterministic GPU execution.

### Embedded systems and resource evidence

Two candidate outputs are a small firmware component and machine-checkable
resource evidence for an existing systems program. A resource certificate
would need proved loop or recursion bounds, complete call and allocation
coverage, and a stated target/runtime model. Source-level boundedness alone
does not establish the final linked program's stack, heap, or timing bound.
The [fixed-resource investigation](../research/investigations/fixed-resource-execution/README.md)
compares counted control and checked ranks with target/runtime accounting;
its [source proposal](../research/investigations/fixed-resource-execution/DESIGN.md)
retains bounded recursion and separates source proof from image qualification.

First experiments can be separated: a minimal bare-metal program measures
startup, linker, runtime, and image-size costs; a bounded existing program
tests whether a stack or allocation bound can be derived and checked against
the emitted code. Measure monomorphization and runtime size before claiming
suitability for a small flash budget. A host result is not a microcontroller
result, and static bounds complement on-target measurements.

MMIO and interrupts need explicit language and target contracts. The
[LLVM volatile rules](https://llvm.org/docs/LangRef.html#volatile-memory-accesses)
do not give volatile accesses general cross-thread synchronization or ordering
against all non-volatile work. The
[RTIC model](https://rtic.rs/2/book/en/) provides a concrete comparison for
interrupt scheduling and its assumptions. It does not supply Whitefoot with
an interrupt model. Any probe must name its core, preemption and ordering
conditions, resource ownership, and treatment of external failure.

Timing analysis also has existing static baselines, including the
[aiT avionics case study](https://www.absint.com/aiT_airbus.pdf).
The question is whether Whitefoot supplies more useful analyzable inputs or
tighter justified bounds, under a specified cache and hardware model. A
certificate would also need a preservation argument through optimization and
linking. Qualification and certification require their own evidence and cost
study; neither a safety objective nor a regulation establishes adoption or a
short route to a certified toolchain.

### Constant-time preservation

A candidate information-flow discipline could prevent a secret from choosing
a branch, address, or variable-latency operation. Its useful result would be
a property of the generated program under a named leakage and target model.
Source checking alone cannot establish that optimization and instruction
selection preserve the property.

First experiment: a bounded existing cryptographic component, a fixed
functional oracle, and paired inputs differing only in the secret. Check
source flows, optimized IR, and final instructions; use timing measurements
as a probe rather than a proof of no leakage. Stop if the compiler pipeline
cannot provide an inspectable preservation contract. This shares the backend
preservation question with timing/resource evidence, while requiring a
different property and argument.

### Interoperable libraries as validation projects

Codecs, parsers, image decoders, and text tools can expose a real compiler gap
while offering an independent oracle. The dated
[artifact brainstorm](../research/notes/headline-artifact-brainstorm.md)
preserves candidate interfaces, workloads, and risks. Its rankings and
implementation estimates are historical, not a current selection.

A useful first boundary is one real operation and its failure behavior. An
ABI replacement must match ownership, lifetime, layout, and errors as well as
symbol names. Decode comparisons can demand exact output under a pinned
format and implementation contract; encoder comparisons need round trips,
compression ratio, and speed rather than assuming identical compressed bytes.
Compare against a maintained safe implementation as well as a native baseline.
Stop if the supposed drop-in requires callers to change the protected contract
or if a toy kernel no longer exercises the promised component.

The [ripgrep project frame](../research/notes/ripgrep-flagship-frame.md#the-claim)
keeps the selected full-tool comparison boundary; its
[experiment index](../research/experiments/README.md#current-flagship-experiment-evidence)
leads to the existing measurements. A component win does not establish that
full-project result.

### Compiler-guided synthesis and rejection data

A deterministic checker and canonical source could support program search,
training examples derived from diagnostics, or automated repair. Compiler
acceptance is a useful signal, but it cannot establish an unstated requirement
or prevent a search from succeeding by narrowing its own contract. Correlated
errors in a generated program and its generated explanation are another open
problem, not two independent checks.

First experiment: one small task whose interface and behavior oracle are
fixed outside the candidate writer. Compare compiler-guided search with the
same writer and resources without that assistance. Include valid inputs that
expose an over-restrictive contract, and keep oracle cases out of the search
feedback. Measure correctness, repairs, source/proof size, and runtime; stop
if gains depend on weakening the task or merely memorizing the exposed cases.
The existing [differential corpus work](../research/experiments/differential-fuzz/README.md)
illustrates executable disagreement witnesses, not evidence that this search
or a learned writer is effective.

## Open questions and experiments

### Fact-consumer differential checks

An optional optimizer consumer should change only justified code shape.
Compare accepted programs with that consumer enabled and disabled while
retaining every required source proof, and compare values, typed outcomes,
external effects, and cleanup. Vary source structure and valid inputs as well
as optimizer settings; the
[differential corpus work](../research/experiments/differential-fuzz/README.md)
provides existing executable disagreement witnesses.

Mutating or removing a premise should make the corresponding transformation
unavailable, or make the program reject when that premise was required for
safety. A disabled optional consumer must never disable a required proof.
For each proposed fact family, pair an independent behavior oracle with an
inspection of the expected IR or instruction consequence before attributing
a timing result. Agreement between two executions cannot establish that
their shared source checker or lowering is sound.

### Reproducible compiler outputs

Deterministic source checking does not by itself establish byte-identical
objects or executables across machines. A useful experiment first names the
consumer and artifact: diagnostics, checked state, LLVM IR, an object, or a
linked program. It then fixes source, specification and compiler bytes,
dependencies, target features, build flags, and linker inputs, and varies only
the environment dimension whose reproducibility is being tested.

First compare clean builds in separate checkout paths with the same tools;
extend to host or toolchain changes only when the consumer needs that promise.
Keep byte identity distinct from equivalent runtime behavior, and inspect
path, timestamp, ordering, or toolchain differences before adding metadata
normalization or caching. No complete cross-machine object reproducibility
result is claimed here. Stop at the artifact boundary the build, audit, or
backend-comparison consumer actually needs.

### AI authoring and proof costs

Does a regular, unfamiliar language help an AI author enough to justify its
extra source and proof work? How much comes from the language, the examples,
the diagnostics, and the allowed repair loop? Can independently written
components be changed and composed without disproportionate rework? The
[August writer trial](../research/experiments/blind-writer/2026-08-28/REPORT.md)
and [default-floor series](../research/experiments/default-floor/RESULTS.md)
retain concrete programs and observations under earlier compiler conditions.
They do not settle these questions for the current language or future models.

First experiment: a bounded real task with an independent oracle and fixed
requirements, comparing Whitefoot with a useful alternative such as Rust.
Record model, tools, supplied context, source/proof tokens, compile and repair
attempts, human intervention, maintenance edits, and resulting runtime. Change
one teaching or diagnostic condition at a time when attributing the result.
Compare familiar and unfamiliar spellings only in a separate controlled
language experiment; ordinary tests are not a population of real writers.

Measure costs that verbosity can hide: long proofs, spec/teaching context,
mathematically safe operations whose proofs do not fit an automatic family,
and repeated cross-module repairs. A useful mistake case is treating an eager
Boolean operation as a guard for a partial operand, documented in the
[spelling study](../research/investigations/spelling-relief/SWEEP.md).
Shorter source is not success if it changes the required result or removes a
needed proof. Reopen a teaching or surface choice when a controlled comparison
identifies the cost and an alternative preserves its required properties.

### Proof checking and compile-run latency

Erased proofs need not be cheap to check. The
[source-proof assessment](../research/investigations/proof-certificate-architecture/SOURCE-CHECKING.md)
separates the soundness obligations from unresolved checking and authoring
costs. Deterministic termination does not establish practical iteration, and
explicit finite steps do not establish linear total checking cost.

A bounded study could attribute time and memory to proof formation, automatic
derivation, explicit-certificate checking, fact transport, lowering, and native
compilation on the same programs. Compare cold runs, repeated runs, and a small
source edit; validate outputs and diagnostics while varying one mechanism.
Caching is a candidate response only if repeated work is the measured cause.
Its key would need every relevant source, specification, compiler, and target
input, and invalidation tests would have to expose stale answers. Stop before
building cache infrastructure if the cost is elsewhere. Measurements may
inform a future rule or implementation choice; elapsed time or an experiment's
budget never selects acceptance of a Whitefoot program.

## Lessons and reopening conditions

Preserve the failure that an alternative must address, together with its
conditions. A historical rejection label is not a current language rule.

| Tempting conclusion | What must be demonstrated before relying on it |
|---|---|
| Proved parallel permission guarantees a speedup. | Measure discovery, scheduling, grain, and locality costs on the same source against sequential execution. Permission establishes independence, not profitable granularity. |
| More writer annotations necessarily make checking or maintenance cheaper. | Attribute construction, checking, and repair costs. A larger vocabulary or a redundant certificate can be sound while still costing more; the source-proof assessment above keeps those questions separate. |
| Restricting source shapes guarantees a good performance floor. | Preserve a representative slow-but-accepted program, compare better valid shapes, and identify whether the cause is teaching, missing permission, or lowering. A restriction that excludes a better valid implementation needs reconsideration under the constitution. |
| A checked source property survives every backend optimization. | State and check the preserved property through the actual backend and target. Functional correctness, constant-time behavior, MMIO ordering, and timing bounds require different evidence. |
| Repeated AI statements are independent verification. | Use independently fixed requirements and behavior oracles; test contract narrowing and correlated mistakes rather than counting agreeing outputs. |
| A new language automatically supplies a compelling deployment advantage. | Compare the full component, ABI and integration cost with effective existing solutions. GPU support, foreign libraries, small-device budgets, and certification are costs to investigate, not consequences of source safety. |

## Selecting an experiment

A candidate investigation should identify its concrete consumer, useful
alternative, required properties, evidence, uncertainty, and a result that
would stop the work. For a consumer of checked facts, ask:

- Which checked proposition does the idea consume?
- Does the consumer affect correctness, performance, or both?
- Which producer and invalidators govern the proposition?
- What safe behavior remains when the target cannot represent the fact?
- Which premise-removal case demonstrates that the consumer fails closed?
- Which correctness, code-shape, and performance observations decide the
  experiment?
- Which result stops the work instead of expanding its scope?

Record a selected experiment and its discriminating criterion in its existing
research home, following [How work proceeds](../AGENTS.md#how-work-proceeds)
and the [investigation skill](skills/investigation/SKILL.md).
