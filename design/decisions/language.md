# Language

The language the specification defines as a whole: safety, the cost of checking, one behavior, what the backend may be told, and the form of the specification itself.

### decision accepted-source-has-no-unchecked-escape
Every fact a source needs is machine-checked before acceptance and erased before lowering; a writer has no unsafe block, trusted theorem or runtime trap [SCOPE-2, EFF-4].
- rejected: an agent proposed a Rust-style `unsafe` policed by convention; the owner rejected it, because any escape a writer can reach becomes a failure edge in the generated code that no one can audit.
- rejected: an agent proposed admitting panics as a language feature; the owner rejected it, because any escape a writer can reach becomes a failure edge in the generated code that no one can audit.

### decision checking-work-never-grows-exponentially
Compiling and proof checking never need work that grows exponentially with the size of the program or of its written proofs.
- rejected: an agent proposed admitting exponential families and keeping them practical with timeouts or work budgets; the owner rejected it, because the target projects must stay practical to iterate on, and a checker that is sure to end is not usable for that alone.

### decision an-accepted-program-has-one-behavior
An accepted program has one observable behavior fixed by the language; no build mode, flag, optimization or backend changes what it does.
- rejected: an agent proposed build modes or switches that trade behavior for speed, such as debug and release builds that differ on integer overflow; the owner rejected it, because then the program that was checked is not the program that runs.

### decision proved-facts-may-reach-the-backend
Every fact the checker has discharged may be given to the backend as attributes, flags, metadata or assumptions; a supplied fact adds no runtime branch and changes no verdict [DIAG-2].
- replaced: A supply confined to disjointness and independence facts as attributes and metadata, never an assumption, was on main; the owner changed it, because correctness comes first and performance is pursued on top of it, and a proved fact is correct to state in any form the backend reads.

### decision the-specification-has-no-ground-ledger
The specification stands on its own, and the design tree, not an index or ledger in the specification, holds the reasons for its rules.
- replaced: A rule-to-ground ledger that the specification required [META-6] was on main; the owner changed it, because the specification and the design tree are the two records, and a third record that nothing reads only drifts.

### decision language-choices-are-made-on-merits
Whitefoot's language choices are made on their merits, never on migration cost, on corpus counts, on the effort a change takes or on how a model writes today.
- rejected: an agent proposed design driven by compatibility with existing designs; the owner rejected it, because before real projects adopt the language there are no users to migrate.
- rejected: an agent proposed design driven by how often the current tests and programs show something; the owner rejected it, because that corpus was written to exercise the compiler, not to represent real programs.
- rejected: an agent proposed design driven by how easily writers write today, as a writer trial measures it; the owner rejected it, because the language trades human writing convenience for performance and checked correctness on purpose.

### decision recoverable-failures-are-values
A recoverable failure is an ordinary `Result` or `Option` value, dispatched by `match` and forwarded by `propagate`; there is no exception, unwinding or panic [EFF-4].
- rejected: an agent proposed exceptions; the owner rejected it, because a second control mechanism would repeat what the values already say and add non-local control flow that the proofs would have to model.
- rejected: an agent proposed algebraic effect handlers that resume; the owner rejected it, because effect rows describe state and not control, and a second control mechanism would add non-local control flow that the proofs would have to model.

### decision the-worked-example-is-normative
The specification's worked example is normative, byte-exact canonical form.
- rejected: an agent proposed an informative illustration beside the rules; the owner rejected it, because everything that appears in the specification is part of the specification.

### decision each-specification-fact-is-stated-once
The specification states each normative fact once, and every other mention cites it by rule id.
- rejected: an agent proposed local restatements and except-when carve-outs so that each rule reads on its own; the owner rejected it, because a restated fact or an exception clause is a second statement of one thing, and it can drift from the first.

### decision the-specification-fixes-what-a-repair-achieves
The specification fixes a rejection's rule, place and payload, which rejections carry a repair and what each repair must achieve, and leaves a repair's words to the compiler [DIAG-1].
- replaced: A repair sentence for each rule in the specification was on main; the owner changed it, because a fixed sentence cannot name the case or pick a proof route, and 22 of 42 reachable prescriptions differed from the printed repairs (data: `research/investigations/repair-wording/DESIGN.md`).
- rejected: an agent proposed prescribing each repair's content while leaving its words free; the owner rejected it, because content varies by position and case: the bare-affine repair has six printed forms where the specification has two (data: `research/investigations/repair-wording/DESIGN.md`).
- rejected: an agent proposed requiring only that a repaired construct is not rejected again by the same rule; the owner rejected it, because that catches one of the seventeen failed literal applications in the probes (data: `research/investigations/repair-wording/DESIGN.md`).
- rejected: an agent proposed requiring every repair alternative to succeed unconditionally; the owner rejected it, because no proof route for a goal over computed values can promise that, and without those routes only a guard is left, which changes what the program does.