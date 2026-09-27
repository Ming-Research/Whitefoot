# A decision workflow for sustained language research

Design study and initial migration, 2026-09-07; constitutional reassessment
revised after the 2026-09-08 owner interview. The question is how Whitefoot's owner and agents
can make grounded choices, revise them when conditions change, and keep the
compiler moving without accumulating an unused process. This study does
not define standing workflow rules or language semantics. The selected trial
method lives in [decision practice](https://github.com/mbbill/Whitefoot/blob/efe40194a05b339c46b931e87bd05a40036f23cf/docs/practice.md#decision-work);
[agent instructions](../../../AGENTS.md) own branch work and merging. The
constitutional reassessment and initial index migration below implement the
first application. They do not establish the method's long-term efficacy.
The subsequent [full-rule assessment](RULE-GROUNDS.md) preserves historical
arguments and questions from that workflow; its later pre-C2 additions are
also historical, not a current rule inventory or selection.

The recommended direction is a small decision method used at actual choices
and evidence changes, with the existing task-completion review as its review
checkpoint. Its value should be tested through better decisions, recoverable
reasons, and useful compiler progress. No source below establishes that this
particular combination will work indefinitely for Whitefoot.

## Evidence and applicability

The sources were read on 2026-09-07. Established engineering methods and
project policies provide precedents; recent agent-team reports provide
experience under particular conditions. These are grounds for a trial, not a
controlled comparison proving one universally best workflow.

| Primary source | Supported practice | Application and limit here |
|---|---|---|
| Michael Nygard, [Documenting Architecture Decisions](https://www.cognitect.com/blog/2011/11/15/documenting-architecture-decisions), 2011 | Short records of significant decisions retain context, consequences, and superseded choices. Consequences can become the context for later decisions. | MCTS-Mem already supplies the record and replacement history. Use its existing structure; an additional numbered ADR collection would duplicate it. The original article reports early experience, not a long-term controlled study. |
| NASA, [Decision Analysis](https://www.nasa.gov/reference/6-8-decision-analysis/), Systems Engineering Handbook | Define criteria, compare alternatives, examine uncertainty that could change their ranking, and scale analysis effort to the decision. | Separate required properties from preferences and examine consequential uncertainty. Borrow the reasoning method, not NASA's organizational approvals or a scoring matrix for every edit. |
| [Rust RFC process](https://rust-lang.github.io/rfcs/) | Substantial changes receive design discussion; many bug fixes and documentation changes use ordinary PR review. Adoption does not imply implementation or implementation priority. | Scale investigation to semantic impact and uncertainty. Whitefoot's work branches remain available for prototypes without a preliminary RFC approval or community waiting period. |
| Nosek et al., [The preregistration revolution](https://psychologicalsciences.unimelb.edu.au/__data/assets/pdf_file/0007/2888098/The-preregistration-revolution.pdf), PNAS, 2018, DOI 10.1073/pnas.1708274114 | Distinguish generating explanations from existing observations from testing predictions with new observations. Exploration remains useful. | Before a measurement intended to select a design, state what would distinguish alternatives. Preserve changes of question or analysis as exploratory findings. This methodological argument does not require preregistering ordinary debugging. |
| OpenAI, [Harness engineering](https://openai.com/index/harness-engineering/), 2026-02-11 | A small entry map, repository-local knowledge, mechanical boundary checks, and ongoing maintenance supported one agent-built product. | Keep relevant knowledge discoverable and repair demonstrated drift. Its execution-plan hierarchy, relaxed merge gates, and automated merges do not transfer to Whitefoot's existing workflow. The report covers one product and a limited period. |
| Anthropic, [Effective context engineering](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents), 2025-09-29 | Retrieve relevant information incrementally; provide sufficient context without a monolithic instruction dump. Retrieval itself has costs. | Follow current owners, rule references, and relevant memory branches. Measure retrieval failures and effort; do not turn a current context-window size into a language-design ceiling. |
| Anthropic, [Demystifying evals for AI agents](https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents), 2026-01-09 | Inspect final outcomes as well as transcripts; combine deterministic, model, and human judgments; calibrate model graders and maintain evaluation tasks. | Verify actual artifacts and required behavior. A fast reviewer can identify local checklist violations but does not certify design soundness. Agent agreement alone is not independent technical evidence. |
| Thoughtworks, [Fitness function-driven development](https://www.thoughtworks.com/insights/articles/fitness-function-driven-development) | Executable checks can protect selected architectural properties as a system evolves. | Keep useful checks in the existing gate and add checks for observed failure modes. Tests protect their stated properties; they cannot establish that every constitutional choice is justified. |

## What the current process already provides

Whitefoot already has work-branch autonomy, an exact-revision merge boundary,
a canonical complete test entry point, document owners, bounded completion
review, and skill-managed decision history. These are useful foundations.

The [workflow memory](https://github.com/mbbill/Whitefoot/blob/0f22b026b062c36ef2f8ce9c725ee4307e1fda64/mcts_mem/whitefoot/development-workflow.md)
records two relevant failure mechanisms: a rolling plan became a changelog
because the working process did not consume it, and current guidance stayed
stale while new Facts accumulated below it. Those records are historical
rationale for avoiding duplicate state and connecting new evidence to current
instructions; they are not additional workflow authority.

The [derivation ledger](../../../spec/derivation/derivation-ledger.md) also
records form-selection questions that remain open. Finding a constitutional
ancestor, finding a convincing selection argument, and proving an execution
property are different achievements. A row's presence can be checked
mechanically; the validity and continued relevance of its reasons require
substantive examination.

The proposed improvement is to make the consumption and revision of reasons
explicit at the point they can change work. More records alone would not
address these failures.

## Working method considered

The following are actions during ordinary work, not new approval stages. The
amount of analysis depends on semantic impact, competing options, uncertainty,
and the cost of reversal. A local fix under unchanged rules normally needs its
existing specification and a distinguishing case. A choice about accepted
programs, trust boundaries, representations, or important performance claims
needs an explicit selection argument.

The 2026-09-09 refinement groups the method into four occasions: starting work,
choosing, updating settled conclusions or changed grounds, and completion.
Separate experiment and reconsideration rows duplicated the choice and update
steps; their obligations remain within those steps. The owner identified rule
volume and missing trigger occasions as reasons agents skip document upkeep.
The entry instructions therefore point to one detailed procedure rather than
another workflow document or per-task log.

The selected division gives entry instructions responsibility for triggering
actions, mechanical checks responsibility for their encoded invariants, and
the completion reviewer responsibility for local meaning and affected-document
consistency. A final reviewer cannot recover an experiment's unrecorded prior
criterion or the actual rationale behind a choice. Recording at the choice or
update occasion is therefore part of the proposed remedy, not a retrospective
completion exercise. Reduced omission and upkeep cost remain unmeasured.

No new CI mechanism is selected by this refinement. The existing gate covers
executable checks and specification/index structure; memory lint remains a
separate check. Moving a check into CI requires a reliable executable property
and a usable baseline, rather than treating a model's approval as technical
proof. A review report does not itself make an unsupported design claim true.

Useful decision information fits into ordinary prose: the problem and scope;
required properties and preferences; supporting facts and open assumptions;
alternatives actually considered; the current choice and its costs; and the
conditions and dependents relevant to reconsideration. This is a reading and
thinking aid, not a mandatory document schema. Detailed evidence stays at its
source instead of being copied into each explanation.

When a task resumes after a handoff or context reset, recover its requested
outcome, relevant changes, validation, and unresolved question from the PR,
working tree, and investigation where one exists. Temporary progress belongs
to the task, not the durable decision history. Verify the actual revision and
artifacts before trusting a previous completion summary.

### Reconsideration that does not require a whole-project sweep

Examples of useful triggers are a contradictory test or measurement, a newly
encountered program that meets an old decision's revisit condition, a changed
target or model capability relevant to the original cost argument, a changed
objective, or repeated workarounds indicating an assumption is wrong.

For each trigger, name the affected reason and its source. Follow the decisions
that explicitly depend on it, examining whether that dependency is material
before proceeding farther. Record consequential dependencies in prose and
links as decisions are made; a bare related-topic link does not establish
dependence. Existing rule IDs, named concepts, and references are the initial
index. A new graph database is not needed to test this method.

Separate three outcomes: the original ground still holds; the choice remains
useful but needs a corrected ground; or the choice should change. An old
measurement remains an old measurement even when it stops selecting today's
design. A missing rationale is unknown until recovered, not an invitation to
invent a plausible history. Improved authoring ability can change a cost
argument; it does not repair a soundness counterexample.

Old or untouched areas can still contain undiscovered stale guidance. Triggered
review does not prove global consistency. A later targeted maintenance sweep
is justified if ordinary work repeatedly misses the same class of dependency;
its scope should follow that failure rather than a default calendar ritual.

## Roles of the reasoning artifacts

These responsibilities motivated the selected allocation in the standing
document map. They do not add another owner of current project rules.

- **Constitution:** the founding purpose, explicit objectives and tradeoffs,
  and general principles with their necessary conditions. Audit each clause's
  grounds and placement. A specific mechanism cannot establish its own
  suitability merely by first becoming a constitutional clause.
- **Active specification:** the language currently being defined. Correctness
  tests follow its judgments, including while a design is being reconsidered.
  A behavior change remains an explicit specification amendment.
- **Derivation ledger:** evolve the existing artifact toward a rule-to-ground
  index. Distinguish a conditional deduction, empirical support, and a
  provisional selection. Link shared rationales rather than repeatedly
  asserting that each detail uniquely follows from the founding premise.
  Preserve historical amendments and audit the meaning of existing status
  labels before changing them. META-6 and its gate are amended together for
  current-index coverage; the existing file also retains the old evidence.
- **MCTS-Mem:** current decisions, sourced evidence, and actual rejected
  alternatives. Keep its skill-defined Items/Facts/Moves structure and
  provenance rules. Technical uncertainty and reopening conditions belong with
  the affected reason; a proposal in an investigation is not yet a live Item.
- **Investigations and experiments:** the design comparison and reproducible
  evidence, with original conditions retained. A selected research question may
  need these; a routine task does not need a new dossier.
- **Practice, agent entry, and completion checklist:** the method, its entry
  points, and the final bounded checks respectively. These consume the other
  artifacts without duplicating their changing technical content.

All language decisions should have an intelligible ground, but the depth can
vary. Several spelling details can share a provisional convention; a new
proof authority needs its own soundness argument. Neither a missing benchmark
for a harmless convention nor a completed checklist should decide whether a
substantive safety claim is established.

## Alternatives weighed for the workflow

| Candidate | Benefit | Reason for the present recommendation |
|---|---|---|
| Retain the constitutional-derivation system and add stricter bookkeeping | Familiar traceability and easy coverage checks | Does not address assumptions hidden inside constitutional clauses or the difference between a rationale and a logical consequence. |
| Require a full RFC and staged design review before significant branch work | Deliberation is visible before implementation | Adds a preliminary coordination gate and delays prototypes that may provide the needed evidence. Existing branch autonomy and one completion review fit this research compiler better. |
| Use the existing artifacts with explicit decision and reconsideration triggers | Reasons participate in work, and ordinary fixes stay inexpensive | Recommended for a trial. Its main unresolved risks are missed dependencies and reasoning that looks complete but is poorly supported. |
| Build an automatic global dependency and consistency system first | Could eventually help detect stale dependent guidance | Meaning and dependency classification remain difficult; a large schema could become another artifact to maintain before improving compiler work. Reconsider tooling after observing retrieval or propagation failures. |

The third alternative is selected for a trial, with the constitutional-chain
aspect replaced by the index described below. The [decision-ground memory](https://github.com/mbbill/Whitefoot/blob/0f22b026b062c36ef2f8ce9c725ee4307e1fda64/mcts_mem/whitefoot/development-workflow/decision-grounds.md)
records that replacement; this comparison is its design source.

## Constitutional reassessment

The founding purpose changes the intended author. It supports investigating
different tradeoffs; it does not prove which values to choose or which
mechanism meets them. The owner interview separated constitutional obligations
from mechanisms and from instructions for applying them. The following audit
covers the preceding draft's clauses; its old labels identify that draft only.
The constitution now uses complete clauses without the inherited letter classes
or per-clause usage tables. This comparison retains the placement argument.

| Clause | Ground and disposition | Boundary or open question |
|---|---|---|
| Founding purpose | Retain the language designed as a harness for AI agents as the starting premise. | It does not commit to a future OS, full software stack, or a single solution to every design question. |
| P1 | State human control over software objectives and key tradeoffs, with implementation delegated to agents and machine-checkable constraints reducing repeated human code inspection. Retain large systems as the primary scope and small embedded systems as secondary coverage. | Present model results and forecasts about future capability are conditional evidence, not permanent limits. |
| P0 | Retain runtime performance as a chosen objective; derive specific metrics and tradeoffs from projects and use cases. | No universal throughput, latency, or memory ranking and no single runtime organization is selected. |
| T1, T2 | State full safety obligations and prohibit runtime traps as a language feature. State execution-model dependencies separately, without a blanket escape condition. | A constitutional requirement is not proof of the compiler's correctness; an implementation defect does not weaken the requirement. |
| Balance, R5 | Preserve safety and practical development feasibility while allowing ease of manual source authorship, syntactic familiarity, and compilation speed to yield to runtime performance. Avoid prescribing a detailed division of implementation and review work. | Human direction is established; the necessary human judgments within the delegated workflow remain undecided. Exponential checking growth and practically unusable compilation at target-project scale are excluded; termination alone is inadequate. |
| Compatibility | Exclude migration costs of existing language designs from selection grounds before real project adoption; consider actual impact and migration capability when real projects have compatibility needs. | Internal test/example adaptation costs and syntax frequencies do not establish adoption or common usage. |
| W1 | Make guidance restrictions revisable when they exclude a better-performing implementation meeting safety and feasibility requirements. | Constraints and patterns are means to investigate, not an end that justifies sacrificing the better implementation. |
| W2, W4 | Remove standalone constitutional commitments to specific information and collaboration arrangements. | These remain design questions under effective agent development; neither their value nor their failure follows solely from removing the clauses. |
| W3 | Merge the prohibition on writer waivers and unchecked assertions into the safety requirements. | Verified contracts and exact proof admission remain specified mechanisms. Omitted or incorrect requirements remain possible. |
| R1, R2 | Keep selection arguments and comparisons in decision practice, scaled to impact, error cost, uncertainty, and reversibility. | They are actions used to choose mechanisms, not additional constitutional objectives. |
| R3 | Remove regularity as a standalone constitutional principle. | Exact canonical bytes and reject-versus-normalize remain provisional language choices with their own hypotheses and open comparisons. |
| R4 | Merge pre-acceptance machine verification and defined expected failures into safety. Keep diagnostic guidance in engineering practice. | Feedback quality is evaluated for its intended consumer rather than promoted into a particular diagnostic form. |
| T3 | Remove the separate optimization commitment from the constitution. | SCOPE-2, DIAG-2, EFF, and PAR still define the selected erasure and overlap rules; the constitutional rewrite does not amend them. |
| T4 | State the resource-bound objective under explicit budgets and applicable conditions. Remove the prescribed interface approach from constitutional status. | Resource representations, release protocols, and scheduling policies need their own safety and performance arguments. |
| D17 | Remove special constitutional status for a particular representation-privilege direction. | Its history remains evidence and the direction remains available for investigation. Existing specified mechanisms retain their own obligations. |
| R6 | Fold openness into revisable objectives and choices. | No future operating-system shape, runtime arrangement, or detailed allocation of human review tasks follows from the founding purpose. |
| R0, R7 | Keep comparison and evidence methods in practice with descriptive names, removing the inherited labels. | The completion checklist retains its own unrelated check IDs. |

### Legacy inline rationales

Twelve passages in the [preceding specification](https://github.com/mbbill/Whitefoot/blob/7f096b94bb7e1ab6519094a171adf00dc1f36616/spec/kernel-spec.md)
still cited retired constitutional labels. Their normative syntax, rejection
conditions, and execution rules remain in the specification. The references
and two inline selection arguments are removed as part of the same v0.52
amendment; this does not select replacement language mechanisms.

| Rule | Retained rule and reason requiring assessment |
|---|---|
| FORM-6 | The unit token retains its type/value roles and FORM-1 convention. The old R3 label did not establish that this spelling was uniquely necessary. |
| FORM-7 | Literal range and canonicality checks retain their stated rejection behavior; the R4 parenthetical supplied no additional judgment. |
| GRAM-11 | Named arguments retain the declaration-order check. The anti-transposition rationale is an intended benefit, not a measurement supplied by the old R4 label. |
| CONST-2 | Constant initializers still completely define their values. The retired T1 citation adds no initialization condition. |
| OWN-13 | The taken value-producing arm still moves its owned result exactly once; that local ownership requirement stands independently of the retired T1 label. |
| OP-1 | Array and buffer construction still initializes every element, and vacant-buffer construction still duplicates no source value. Removing three T1 citations changes none of these operation contracts. |
| OP-7 | Domain prefixes and their exceptions remain specified. The old W1-predictable label did not establish authoring or runtime benefit. |
| OP-8 | The operation table retains its totality edges and lowerings. Their correctness depends on those technical rules, not the old T2/W3 citation. |
| FN-7 | The prohibition on global state and static regions remains, with the existing immutable-constant exception. The preceding argument claimed mutable globals would erode parameter-derived noalias facts, hide channels from signatures, and pre-seed shared concurrency state. Those claims require assessment against concrete alternative designs and execution requirements; old P0/W3/T1 labels do not make them necessary conclusions. |
| ERR-3 | Result propagation, consumption, return behavior, and context attachment remain specified. The preceding argument appealed to earlier error discovery, avoiding context loss from manual rematching, one mechanical pattern, and preventing dropped errors. These are separate intended benefits to assess, not conclusions established by R4/W1/W3. |

The citation cleanup left all ten current-index rows unassessed with their
historical records and this scoped explanation. Later scoped assessments are
linked from their current rows. Removing the citations does not establish their
mechanisms' optimality, sufficiency, or failure. Reassess the actual reasons
when the rule or relevant evidence is materially changed.

### Surface and definition conventions

FORM-1, FORM-2, and FORM-4 remain provisional selections. The retained
[surface-form record](https://github.com/mbbill/Whitefoot/blob/0f22b026b062c36ef2f8ce9c725ee4307e1fda64/mcts_mem/whitefoot/surface-form.md) identifies
the byte-format and no-comment choices as minimality selections awaiting
comparison. Their present selection rests on the hypothesis that reducing
surface variation makes agent edit instructions and tool output more predictable.
This is a provisional reason to investigate the forms, not a constitutional
deduction, measured advantage, or fixed context limit. Neither their frequency
in the internal corpus nor the cost of rewriting that corpus supports their
selection. Canonicalization rather than rejection, and explicit
comments rather than declaration-only documentation, remain live comparison
questions. A representative writer/repair comparison or a repeated inability
to preserve useful information would reopen the respective choice. The active
rules remain unchanged while their grounds are provisional.

META-4 retains a single definition site as a provisional way to make references
and updates locatable. Its old claim that this prevents logical contradictions
is rejected: two different facts can contradict one another. This is a
provisional maintainability choice; an observed inconsistency that survives
the ownership rule or a clearer representation of shared constraints should
reopen its sufficiency. D3 and R3 in the completion checklist provide a
semantic check of affected owners, without claiming global consistency.

These are reassessed reasons to retain conventions, not experiments that
validate them or findings that other conventions fail.

### Decision index

META-6 serves the need to recover and reconsider concrete choices, with the
separation of claims expressed by decision practice. The previous
gate accepted any historical row carrying the ID; it could not distinguish
an old constitutional chain from a present selection ground. The workflow
memory also records a broader-source comparison that met an old provisional
row's stated reopening condition without updating that row. These are reasons
to connect current entries to decision and evidence triggers.

The selected form is a four-column current table in the existing ledger:
rule, basis kinds, review state, and scoped source references. Rule IDs are
already stable; shared sources avoid copied rationales. The gate reads only
this table and checks coverage and field structure. No separate ADR collection,
dependency database, per-task ledger, or version counter is needed. The exact
table shape is provisional, selected for low maintenance cost; checkable
negative cases establish its integrity behavior, not its workflow efficacy.
Reopen the choice if ordinary work repeatedly misses affected reasons, cannot
retrieve them from the index, or spends more on updating records than the
decision warrants.

The initial migration over the v0.51 container amendment provided a row for
each of its 161 active rules. Five convention
and workflow rows had the scoped grounds above; the other 156 were explicitly
`unassessed`/`revisit`, with retained evidence pointers. Those flags do not
declare the rules unsound. They reject automatic conversion of old `derived`
labels into current endorsement. The shared premise changes are the clause
audit above: all legacy chains need that distinction checked when materially
touched, including their original form-selection conditions. In particular,
proof, contract, resource, and overlap mechanisms do not become deductions
merely because a corresponding constitutional goal survives.

The obsolete provisional register is removed from the active specification's
header. It mixed selection history and an internal memory reference into the
language definition, and still called match statement-only. Its original
conditions remain in the outgoing specification archive and the retained
derivation record. META-6 changes the evidence index contract, with no change
to grammar, runtime semantics, or writer acceptance. The outgoing bytes are
archived unchanged as part of the specification amendment.

## Trial and transition

The clause audit and five assessed rows above are the first application. They
establish the migration's content and explicit limits, not that the method
improves future decisions. Continue reassessing legacy grounds as their rules
or material reasons are touched; do not require an unrelated full sweep to
fix an ordinary compiler bug.

Then try the method on a small set of real compiler decisions as they arise,
including an ordinary fix that should not acquire research paperwork. Use
these scenarios as checks on the proposal:

| Scenario | Expected useful behavior |
|---|---|
| A surface rule cites canonicality as its sole justification | Distinguish consistency with the selected convention from evidence that the convention serves the intended author. Do not silently change accepted source. |
| A broader program supplies the evidence an old provisional proof decision awaited | Revisit that decision's actual selection condition and dependent guidance; do not merely append a result beneath unchanged conclusions. |
| A previously rejected mechanism becomes affordable to implement | Recover its recorded failure, identify what changed, and check any other failure grounds before reconsidering it. |
| A local compiler defect violates an unchanged rule | Add or identify the distinguishing regression and fix the normal path, without a new decision record or approval stage. |
| A fresh agent resumes the work | Recover the current choice, evidence, uncertainty, and next useful action from relevant owners without reading the entire project history. |

The scenarios are a trial proposal, not results already obtained. Initial
replays of known failures check retrieval and reasoning behavior; they do not
establish improvement on unfamiliar design problems. Compare with the current
method using the same requested outcomes and available evidence. Record model,
context, assistance, and any reuse of known answers. Inspect outcomes and
specific reasoning errors rather than scoring compliance with one sequence
of tool calls.

Useful observations are missed relevant reasons, unjustified changes to task
requirements, stale dependent guidance, repeated owner corrections, time to
recover context, and analysis/record-maintenance effort relative to useful
implementation work. Inspect examples behind counts. A small trial supports
a local improvement claim, not a forecast of years of reliable operation.

Use the selected practice and completion checks during those tasks. Change
them in place when results justify doing so; do not invent missing historical
evidence to complete the migration. Preserve technical
records that remain informative; remove or merge this proposal when a
successor fully carries its useful comparison and trial evidence. Additional
automation earns its place by addressing observed recurring failures and
having a maintained caller. The workflow itself remains open to revision when
its cost exceeds the problems it prevents.

## Legacy memory format repair

The owner selected repair of the inherited lint baseline on 2026-09-09.
The original [container node at 46a608a1](https://github.com/mbbill/Whitefoot/blob/46a608a17bec308ec9f9ebf6d73cd66a48d8f27e/mcts_mem/whitefoot/data-model/container-representation.md)
is the immutable comparison source. The repair changes representation of the
record, not the historical design or measurements:

- The ten untagged 2026-09-06 Facts retain their full bodies and gain `sourced`:
  these entries cite historical assessments and experiment reports; the repair
  does not claim to have rerun their measurements.
- The bb8eb30f occurrence/representation/provenance pitfall becomes four atomic
  entries with the same date and revision: lost occurrence use, erased parameter
  modes, lost call-result provenance, and the resulting placement limitation.
- Four undated prose entries under Moves become dated historical rationales in
  Facts, retaining their full bodies. The first three are present in d998dd0a,
  and the external-trace entry first appears in 4f8fb9d5, both 2026-09-06.
  They did not record paired tree-node replacements; inventing counterpart nodes
  would fabricate history. Their actual selections and supersession claims remain.
- The current representation-authority Item states its technical selection
  conditions without the retired D17 label or a workflow-status phrase.

The ordinary append-only rule remains appropriate for substantive history.
Keeping malformed entries unchanged would leave lint permanently failing;
excluding the node or weakening lint would conceal future defects. A traced,
reviewed format repair preserves the record and restores a useful baseline.
The linter compares history against HEAD: before committing this repair it
reports the 15 transformed entries as an append-only violation; after commit
it uses the repaired baseline. Both results must be reported alongside the
original-revision comparison, rather than treating a new baseline as proof
that nothing changed. No lint rule or exclusion is changed.

## First bounded application

On 2026-09-09, the [constant-initialization assessment](../const-eval/INITIALIZATION.md)
used the new process on an existing rule whose constitutional citation had been
removed. Reading its construction dependency exposed an overbroad historical
transposition argument. The discriminating probe was committed before execution;
its outcome separated a guaranteed rejection from an accepted logic error. The
result updated the scoped rule grounds and memory while preserving the actual
language behavior and historical rationale.

This application exposed a practical requirement already present in the update
step: a neighboring rule's rationale may need reconsideration even when its
implementation is unchanged. It did not require a new workflow rule, a global
ledger sweep, or a claim that all of CONST-2 had been reassessed. One application
establishes neither reduced review cost nor long-term synchronization reliability.

## Documentation scope during reassessment

The source-proof reassessment exposed an existing complexity claim and a
poorly attributed cost record. In the 2026-09-09 application, that finding
prompted temporary compiler instrumentation and a program fixture before the
owner restated the documentation-only scope. Those implementation edits were
withdrawn. The surviving assessment separates supported arguments from open
compiler questions and updates their affected document owners.

This is a scope-following failure, not evidence that a new compiler mechanism
was selected. The start occasion now explicitly reads the requested outcome
and scope; discovering another problem does not expand it. A1 already checks
task fit at completion, so no second checkpoint or approval rule is added.
The final reviewer must compare the actual diff with the task constraints,
including whether a proposed technical remedy belongs in a later task.

The affected-set walk also reached MSR-4 through its shared AUTO rationale and
an inaccurate host-qualification comment describing the specification delta.
MSR-4's index now points to the full-rule assessment, which includes the scoped
correction. The comment was initially left
as an explicit finding outside the source-file scope; the owner subsequently
authorized its correction without changing executable code. Finding these
connections demonstrates
a concrete use of the dependency walk, not that every dependency is discoverable
or that review cost is reduced. A recorded finding is not a completed repair.
