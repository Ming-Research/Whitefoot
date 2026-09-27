mod acceptance;
mod behavior;
mod cleanup;
mod control;
mod ensures;
pub(in crate::semantic::check) mod expressions;
pub(in crate::semantic) mod floats;
mod generics;
mod linearity;
mod nominal_instances;
mod nominals;
mod obligations;
pub(crate) mod publication;
mod receipts;
mod references;
mod repairs;
mod requires;
mod support;
mod tail_calls;
mod type_regions;
mod types;

pub(crate) use receipts::ProofReceipts;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use crate::syntax::NodeId;
use crate::{
    DeclarationId, DeclarationRole, NodePath, Production, ResolvedSyntaxUnit,
    SemanticCompilerFailure, SemanticIssue, SemanticIssueKind, SemanticLocation, SemanticOutcome,
    SemanticRule,
};

use super::entailment::{
    EntailmentCallee, EntailmentContext, PostconditionSchedule, VerifiedPostconditionSummary,
    analyze_function, analyze_function_candidate, collect_statement_calls,
    finalize_function_entailment, postcondition_schedule,
};
use super::goal::{
    CheckedCallRequirement, CheckedRequirement, ConcreteGoal, GoalDatum, GoalExpression,
    GoalOperation, GoalProjection,
};
use super::model::{
    BindingId, CheckedConst, CheckedConstant, CheckedConstantId, CheckedElement, CheckedExpression,
    CheckedFunction, CheckedGenericRequirement, CheckedMode, CheckedNominal, CheckedNominalKind,
    CheckedNumericType, CheckedParameter, CheckedProgramData, CheckedSetTarget, CheckedStatement,
    CheckedType, CheckedValue, DerivedConst, DerivedConstId, FunctionId, NominalId,
    ValueInitializerKind, evaluate_const_operation,
};
use super::permission::{PermissionSignature, analyze_permission, plan_permission_separations};
use super::permission_ledger::{LedgerSource, render_ledger};
use super::places::ResolvedPlace;
use super::postcondition::CheckedPostconditionSelector;
use super::tree::TreeView;
use super::{CheckStop, CheckedProgram};
use control::{ControlCounters, ControlScope};
use generics::{GenericParameter, GenericSubstitution, PendingGenericRequirement};
use references::ReferenceInfo;

/// The syntax tree, as the permission ledger's citations reach it.
struct PermissionLedgerSource<'view, 'unit> {
    tree: &'view TreeView<'unit>,
}

impl LedgerSource for PermissionLedgerSource<'_, '_> {
    type Error = SemanticCompilerFailure;

    fn location(&self, path: &NodePath) -> Result<(String, u64), Self::Error> {
        self.tree.source_line(path)
    }

    fn spelling(&self, path: &NodePath) -> Result<String, Self::Error> {
        self.tree.path_spelling(path)
    }
}

#[derive(Clone)]
struct ParameterSignature {
    declaration: DeclarationId,
    node_path: crate::NodePath,
    name: String,
    mode: CheckedMode,
    ty: CheckedType,
}

/// One declared result ordinal of a callable boundary [GRAM-2, FN-1].
#[derive(Clone)]
struct ResultSignature {
    mode: CheckedMode,
    ty: CheckedType,
    /// The ordinal's complete `rtype`, for a diagnostic at the declaration.
    rtype: NodeId,
}

/// Restores the module under check when one declaration's judgments end
/// [MOD-5].
pub(in crate::semantic::check) struct ModuleContext<'checker> {
    cell: &'checker Cell<Option<crate::ModuleId>>,
    previous: Option<crate::ModuleId>,
}

impl Drop for ModuleContext<'_> {
    fn drop(&mut self) {
        self.cell.set(self.previous);
    }
}

#[derive(Clone)]
struct FunctionSignature {
    id: FunctionId,
    declaration: DeclarationId,
    node: NodeId,
    name: String,
    symbol: String,
    /// Every formal region of the callable.
    region_parameters: Vec<DeclarationId>,
    parameters: Vec<ParameterSignature>,
    /// The callable result [FN-1]: the written result of a single-result
    /// declaration, and the compiler-owned result-list value of a declaration
    /// that writes an ordered result list [GRAM-2, CALL-4].
    result_mode: CheckedMode,
    result: CheckedType,
    /// Every declared result ordinal in written order. A single-result
    /// declaration has exactly one entry and it is the callable result above.
    results: Vec<ResultSignature>,
    /// The result-list nominal, for a declaration that writes two or more
    /// results. `None` is the single-result form, whose callable result is
    /// the written result itself.
    result_list: Option<NominalId>,
    effects_node: NodeId,
    declared_effects: EffectSet,
    /// A callable hypothesis used only while checking generic source spelling.
    /// Concrete calls always select a verified source function instead.
    formal_parameter: Option<generics::GenericParameterKey>,
    substitution: GenericSubstitution,
}

#[derive(Clone, Copy)]
struct PostconditionCheckContext {
    record: usize,
    result_type: CheckedType,
}

/// One fully checked concrete function awaiting program-level entailment.
///
/// Binding spellings are checker-only diagnostic data. Keeping them beside
/// the checked function lets phase A finish the complete concrete inventory
/// before phase B derives or rejects any acceptance-bearing judgment.
#[derive(Clone)]
struct CheckedFunctionInventory {
    function: CheckedFunction,
    binding_names: Vec<String>,
}

#[derive(Clone)]
struct FunctionTemplate {
    declaration: DeclarationId,
    node: NodeId,
    name: String,
    generic_parameters: Vec<GenericParameter>,
}

#[derive(Clone)]
struct NominalTemplate {
    declaration: DeclarationId,
    node: NodeId,
    name: String,
    role: DeclarationRole,
    generic_parameters: Vec<GenericParameter>,
    /// [S20, GRAM-2] the declaration's own `region_params`, in written order.
    ///
    /// They are components of the nominal's type name [TYPE-2], so an
    /// instance is keyed on them beside its type and const arguments and two
    /// instances at two regions are two types [PROV-1].
    region_parameters: Vec<DeclarationId>,
    /// [PROV-6] whether the declaration writes the `nodrop` modifier. Every
    /// instance of a marked declaration is marked.
    linear: bool,
    /// [OWN-1, GRAM-2] whether the declaration writes the `nocopy` modifier.
    /// Every instance of a marked declaration is marked.
    nocopy: bool,
    /// [FORM-8] one entry per constructor of this declaration — a struct has
    /// one, an enum one per variant in tag order — and empty for a
    /// declaration carrying no `region_params`.
    constructors: Vec<ConstructorShape>,
}

/// One `construct` occurrence's declaration data [FORM-8, TYPE-5]: the
/// template it names, the variant when its nominal is an enum, the
/// declaration's own generic and region parameters, and the constructor's
/// shape.
struct ConstructorSite {
    template: usize,
    variant: Option<u32>,
    generic_parameters: Vec<generics::GenericParameter>,
    region_parameters: Vec<DeclarationId>,
    shape: ConstructorShape,
}

/// What a `construct` of one nominal has to know *before* it forms the
/// instance [FORM-8].
///
/// A construct writes a region argument only for a region parameter no field
/// operand determines, and the operands are what determine the rest — so the
/// instance is what the judgment produces and cannot be what it consults.
/// This is read once, off the declaration's own symbolic instance, whose
/// region arguments are its region parameters, while the templates are
/// validated.
#[derive(Clone, Default)]
struct ConstructorShape {
    /// The declared field names of this constructor, in declared order.
    fields: Vec<String>,
    /// Parallel to the declaration's `region_parameters`: the field whose
    /// declared type names that region parameter, and `None` where no field
    /// of this constructor does, which is exactly the region argument the
    /// construct writes.
    determining_field: Vec<Option<usize>>,
}

/// A nominal instance a derived type named, awaiting interning.
#[derive(Clone)]
enum PendingNominal {
    /// [STOR-2] a box over this referent.
    Box(CheckedType),
    /// The compiler-owned result-list nominal of a [BLK-0] row that declares
    /// an ordered result list [CALL-4]. A row's list is fixed by its own
    /// instance and has no written form for the interning pass to find.
    ResultList(Vec<(String, CheckedType)>),
    /// A prelude instance, such as the `Result<T, E>` a checked row produces.
    Prelude(PreludeType),
    /// [S20, FN-2] one source nominal instance at a region a call determined.
    ///
    /// A callee's result names its own formal region, and the instance the
    /// caller receives is that declaration at the actual region [FORM-8]. No
    /// caller position need write that type, so the interning pass cannot
    /// have found it; the checking path records the template and the
    /// substituted instance key here and the driver interns it.
    SourceInstance {
        template: usize,
        substitution: GenericSubstitution,
    },
}

#[derive(Clone)]
struct NominalInstance {
    id: NominalId,
    substitution: GenericSubstitution,
}

#[derive(Clone, Copy)]
enum ConstructorTemplate {
    Struct { template: usize },
    Enum { template: usize, variant: u32 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LocalBinding {
    binding: BindingId,
    declaration: DeclarationId,
    mode: CheckedMode,
    ty: CheckedType,
    /// Whether the binding still owns a usable value, or names a path
    /// [OWN-1, REF-1].
    live: bool,
    loop_depth: usize,
    /// Compiler-updated counted binders are readable source bindings but are
    /// never writer-controlled storage [SET-1, OWN-11].
    compiler_updated: bool,
    /// [REF-1] what this binding names when it is a reference variable: its
    /// path set, its kind, and its [REF-2] validity. `None` is storage of its
    /// own.
    ///
    /// This replaces v0.59's `borrow`, `slice`, `slice_loans` and `suspended`
    /// fields. None of the four has a subject: a reference carries no region,
    /// no strength, no parent holder and no loan, and exclusivity suspension
    /// was the loan apparatus's way of stating what [REF-2] now states
    /// directly as a validity fact.
    reference: Option<ReferenceInfo>,
    /// [REF-2, ENT-3.S15] the borrowed-match refinement occurrences this
    /// binding witnesses. The occurrence identity is immutable even when a
    /// reference binding is rebound; only its flow-sensitive validity meets
    /// at joins.
    refinement_witnesses: Vec<RefinementWitness>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RefinementWitness {
    origin: NodePath,
    place: ResolvedPlace,
    variant: u32,
    valid: bool,
}

impl RefinementWitness {
    fn same_occurrence(&self, other: &Self) -> bool {
        self.origin == other.origin && self.place == other.place && self.variant == other.variant
    }
}

impl LocalBinding {
    /// [REF-1] the join of one binding's state over two incoming edges: the
    /// union of the reference path sets, and the meet of the validity facts.
    ///
    /// Liveness is *not* joined here. [LIV-1] makes a disagreement at a join
    /// a hard error naming the binding and the two disagreeing predecessors,
    /// so the caller compares liveness and rejects; this method runs after
    /// that comparison has agreed.
    fn join_from(&mut self, other: &Self) {
        if let (Some(left), Some(right)) = (&mut self.reference, &other.reference) {
            left.join(right);
        }
        for witness in &mut self.refinement_witnesses {
            witness.valid &= other
                .refinement_witnesses
                .iter()
                .find(|candidate| witness.same_occurrence(candidate))
                .is_some_and(|candidate| candidate.valid);
        }
        for witness in &other.refinement_witnesses {
            if !self
                .refinement_witnesses
                .iter()
                .any(|candidate| candidate.same_occurrence(witness))
            {
                let mut absent_on_left = witness.clone();
                absent_on_left.valid = false;
                self.refinement_witnesses.push(absent_on_left);
            }
        }
    }

    /// Whether two joined states agree on everything [LIV-1] compares before
    /// `join_from` meets their flow facts.
    ///
    /// A reference's path set is deliberately excluded: [REF-1] states that
    /// at a join a reference variable's target is the *union* of the incoming
    /// sets, so two different sets are the joined state rather than a
    /// disagreement. Reference validity and refinement witnesses are likewise
    /// excluded here because `join_from` meets them after this structural
    /// comparison.
    fn agrees_with(&self, other: &Self) -> bool {
        self.binding == other.binding
            && self.declaration == other.declaration
            && self.mode == other.mode
            && self.ty == other.ty
            && self.live == other.live
            && self.loop_depth == other.loop_depth
            && self.compiler_updated == other.compiler_updated
            && match (&self.reference, &other.reference) {
                (Some(left), Some(right)) => left.kind == right.kind,
                (None, None) => true,
                _ => false,
            }
    }

    /// A loop header is checked once rather than materialized by `join_from`.
    /// Its reference validity has a separate finite equation, but every
    /// non-reference refinement witness must therefore still agree exactly
    /// with the backedge as it did before loop-header widening.
    fn loop_agrees_with(&self, other: &Self) -> bool {
        self.agrees_with(other) && self.refinement_witnesses == other.refinement_witnesses
    }
}

#[derive(Clone, Copy)]
enum Constructor {
    Struct(NominalId),
    Enum { nominal: NominalId, variant: u32 },
}

struct TypedExpression {
    expression: CheckedExpression,
    mode: CheckedMode,
    /// [REF-1] the path set this expression names when it produces a
    /// reference: a `borrow_expr`, a read of a reference variable, or a value
    /// initializer every arm of which delivers one.
    ///
    /// This replaces v0.59's `borrow`, `slice` and `holder` fields. A
    /// reference has no region, no strength and no parent holder, so the
    /// three collapse into the one thing [REF-1] gives it.
    reference: Option<ReferenceInfo>,
    /// Whether this expression denotes the reference itself rather than a
    /// place reached through it.
    ///
    /// A destination of reference kind wants exactly this value; a construct
    /// that needs an owned value — a `match` scrutinee of own mode under
    /// [OWN-13], `propagate` under [ERR-3] — reads the path instead. A
    /// reference is read bare [TYPE-7], so this flag distinguishes the two
    /// readings and never selects a read-through operation.
    reference_value: bool,
    effects: EffectSet,
    accesses: Vec<PlaceAccess>,
}

#[derive(Clone)]
struct PlaceAccess {
    place: ResolvedPlace,
    /// This place is the storage selected by the enclosing place expression,
    /// rather than storage read while evaluating one of its offsets or other
    /// operands. Effects retain both; borrowed-match path recovery needs only
    /// the selected members [REF-1, OWN-13].
    selected: bool,
}

impl PlaceAccess {
    fn operand(mut self) -> Self {
        self.selected = false;
        self
    }
}

impl TypedExpression {
    fn owned(expression: CheckedExpression, effects: EffectSet) -> Self {
        Self {
            expression,
            mode: CheckedMode::Own,
            reference: None,
            reference_value: false,
            effects,
            accesses: Vec::new(),
        }
    }

    fn owned_with_access(
        expression: CheckedExpression,
        effects: EffectSet,
        place: ResolvedPlace,
    ) -> Self {
        Self {
            expression,
            mode: CheckedMode::Own,
            reference: None,
            reference_value: false,
            effects,
            accesses: vec![PlaceAccess {
                place,
                selected: true,
            }],
        }
    }
}

/// One ordinary resolved-place contribution to the enclosing effect row.
#[derive(Clone, Debug)]
struct EffectPath {
    path: super::model::CheckedStatePath,
}

impl From<super::model::CheckedStatePath> for EffectPath {
    fn from(path: super::model::CheckedStatePath) -> Self {
        Self { path }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct EffectSet {
    reads: Vec<super::model::CheckedStatePath>,
    writes: Vec<super::model::CheckedStatePath>,
    /// [EFF-3] whether the boundary this set describes allocates. It is not
    /// a row category [EFF-1, STOR-8]; it is the checked-program metadata
    /// [EFF-3]'s deduplication and reordering licence reads.
    allocates: bool,
}

impl EffectSet {
    const NONE: Self = Self {
        reads: Vec::new(),
        writes: Vec::new(),
        allocates: false,
    };
    fn union(mut self, other: Self) -> Self {
        for path in other.reads {
            self.add_read(path);
        }
        for path in other.writes {
            self.add_write(path);
        }
        self.allocates |= other.allocates;
        self
    }

    fn add_read(&mut self, path: impl Into<EffectPath>) {
        Self::add_path(&mut self.reads, path.into());
    }

    fn add_write(&mut self, path: impl Into<EffectPath>) {
        Self::add_path(&mut self.writes, path.into());
    }

    fn add_path(paths: &mut Vec<super::model::CheckedStatePath>, contribution: EffectPath) {
        if !paths.contains(&contribution.path) {
            paths.push(contribution.path);
            paths.sort_unstable();
        }
    }

    /// [EFF-3] records that this boundary allocates.
    fn add_allocation(&mut self) {
        self.allocates = true;
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum PreludeType {
    Option(CheckedType),
    Result(CheckedType, CheckedType),
    Overflow,
    DivError,
    NarrowError,
}

struct Checker<'unit> {
    resolved: &'unit ResolvedSyntaxUnit,
    /// [DIAG-1, FN-10] retain tail-condition failures until ordinary call
    /// checking, including FN-8 proofs, can establish a prior same-node rule.
    musttail_rejections: RefCell<Vec<SemanticIssue>>,
    /// [STOR-8, GRAM-2] whether this compilation unit wrote `program
    /// no_heap;`.
    ///
    /// The declaration's placement rule — at most once, and only as the first
    /// `item` of the first source record — is resolution's [GRAM-2]
    /// admission, so a `heap_decl` reaching the checker at all is the
    /// admitted one and the fact is a unit-level flag. What the checker owns
    /// is [STOR-8]'s consequence: such a unit cannot name `Box` or a
    /// runtime-capacity shape and cannot call an allocating prelude row.
    no_heap: bool,
    /// Whether an undischarged obligation rejects. Always true outside the
    /// test-only observability hooks.
    reject_entailment: bool,
    tree: TreeView<'unit>,
    nominals: Vec<CheckedNominal>,
    /// Counts the changes to `nominals`: an instance appended or completed,
    /// or a checkpoint restored. The table's layout recursion is judged again
    /// only after it changed [`Checker::reject_recursive_nominal_layouts`].
    nominal_generation: u64,
    /// The generation at which the table was last judged to hold no
    /// recursive layout.
    nominal_layouts_acyclic_at: std::cell::Cell<Option<u64>>,
    elements: RefCell<Vec<CheckedType>>,
    element_ids: RefCell<HashMap<CheckedType, CheckedElement>>,
    nominal_nodes: Vec<Option<NodeId>>,
    nominal_states: Vec<u8>,
    source_nominal_instances: Vec<Option<(usize, GenericSubstitution)>>,
    box_nominals: HashMap<CheckedType, NominalId>,
    /// The compiler-owned result-list nominal of a `fn_decl` that declares an
    /// ordered result list [GRAM-2, CALL-4], keyed by the ordered result
    /// binder spellings and types. Two declarations whose result lists agree
    /// share one nominal; the value a multi-result callable hands back is one
    /// value of it, and a destructuring binder list is its projection.
    result_list_nominals: HashMap<Vec<(String, CheckedType)>, NominalId>,
    /// Nominal instances a derived type named that were not interned yet.
    /// Written by the `&self` checking path and drained by the `&mut self`
    /// driver between attempts at one function.
    pending_nominals: RefCell<Vec<PendingNominal>>,
    /// [OP-10, OP-11, OP-14] instances of an operand-directed [PRE-1] row
    /// whose substitution a body reached but whose signature is not built yet.
    ///
    /// These rows write no type arguments at a call, so the syntax alone
    /// selects no instance and the discovery walk builds none. The `&self`
    /// body check derives the substitution from the operand's type and
    /// records it here; the `&mut self` driver builds the signature and
    /// retries the function, exactly as it does for a derived nominal.
    pending_instances: RefCell<Vec<(usize, generics::GenericSubstitution)>>,
    /// [FN-2, MOD-8] the call that first requested each concrete generic
    /// instance, keyed by its template node and substitution, in the
    /// deterministic discovery order, so a rejection raised while checking
    /// that instance names its requester.
    instance_requests: RefCell<Vec<(NodeId, generics::GenericSubstitution, NodeId)>>,
    /// [PROV-1] the region an elided store brand denotes at the position
    /// being parsed: the enclosing nominal's sole region parameter while a
    /// `struct_decl` or `enum_decl` body is being read, and `None`
    /// everywhere else, where FORM-8 requires a written store argument.
    elided_store_brand: std::cell::Cell<Option<DeclarationId>>,
    /// [FN-2, OWN-1, PROV-6] whether the body now being checked is an instance
    /// other than its generic template's own symbolic spelling authority.
    ///
    /// The template is the spelling authority: a body whose parameter lacks
    /// copy writes `move`, a `copy`-bounded body writes bare use, and the one
    /// symbolic instance decides both once. A recheck with supplied arguments,
    /// even arguments still symbolic in a caller, does not re-judge the
    /// [OWN-1]/[FORM-1] spelling, and a `move` of a template-affine value at a
    /// copy instance denotes a copy. Every other
    /// [OWN-1] judgment — consume-once, dead roots, exclusivity — is
    /// re-judged as usual, because those are properties of the concrete
    /// instance and not of the written spelling.
    template_spelling_authority: std::cell::Cell<bool>,
    /// [LIV-2] the target places of the `set` commit whose right-hand side is
    /// being checked, and whether that right-hand side has read each out.
    /// Empty everywhere else: `check_commit` installs it around exactly that
    /// one expression and removes it before any rejection leaves.
    commit_read_outs: RefCell<Vec<control::CommitReadOut>>,
    /// [EFF-5] the pairwise comparisons of the function being checked that
    /// syntax could not settle, handed to the entailment fragment with the
    /// finished body.
    call_separations: RefCell<Vec<super::model::CheckedCallSeparation>>,
    /// [REF-2] uses reached under loop-header validity variables. Every
    /// owning loop resolves its variables before the function is published;
    /// the function driver clears this scratch state on every retry.
    deferred_loop_reference_uses: RefCell<Vec<references::DeferredLoopReferenceUse>>,
    loop_reference_summaries: RefCell<HashMap<references::LoopReferenceToken, Vec<ResolvedPlace>>>,
    /// Resolved origins established by this structural function attempt.
    /// Every retry starts fresh; only its complete final walk is published.
    reference_origins: RefCell<Vec<Vec<ResolvedPlace>>>,
    /// Successful declaration-only FN-4 queries. A complete member check
    /// stages its batch before publishing here, and the whole checker remains
    /// failure-atomic with the prospective checked program [DIAG-2].
    contract_queries: RefCell<Vec<super::model::CheckedContractQuery>>,
    prelude_nominals: HashMap<PreludeType, NominalId>,
    prelude_types: Vec<Option<PreludeType>>,
    nominal_templates: Vec<NominalTemplate>,
    nominal_templates_by_declaration: HashMap<DeclarationId, usize>,
    nominals_by_declaration: HashMap<DeclarationId, Vec<NominalInstance>>,
    constructor_templates_by_declaration: HashMap<DeclarationId, ConstructorTemplate>,
    signatures: Vec<FunctionSignature>,
    function_templates: Vec<FunctionTemplate>,
    templates_by_declaration: HashMap<DeclarationId, usize>,
    functions_by_declaration: HashMap<DeclarationId, Vec<FunctionId>>,
    constants: HashMap<DeclarationId, CheckedConstantId>,
    checked_constants: Vec<CheckedConstant>,
    /// Hash-consed symbolic const operations [CONST-1 candidate]. Written by
    /// the `&self` const-expression parse while a generic template or
    /// symbolic validation instance is checked; every concrete instantiation
    /// evaluates entries away, so no id reaches lowering.
    derived_consts: RefCell<Vec<DerivedConst>>,
    /// [EFF-2] the body-syntactic contribution of each generic template's
    /// written body, recorded by its symbolic validation instance and reused
    /// by every concrete instance of the same declaration; see
    /// [`Checker::written_body_effects`].
    written_body_effect_rows: RefCell<HashMap<DeclarationId, EffectSet>>,
    pending_generic_requirements: Vec<PendingGenericRequirement>,
    generic_requirements: Vec<CheckedGenericRequirement>,
    postcondition_selectors: Vec<CheckedPostconditionSelector>,
    postcondition_unavailable_declarations: Vec<DeclarationId>,
    active_postcondition: Cell<Option<PostconditionCheckContext>>,
    /// The module of the function body under check: a readonly field is a
    /// write target only inside the module that declares it [TYPE-2].
    writing_module: Cell<Option<crate::ModuleId>>,
    /// The result datums admitted in the [FN-9] clause currently being
    /// checked: each written spelling with the result ordinal it names and
    /// the type that datum has [CALL-4]. A declaration writing one result
    /// contributes one row at ordinal zero. Set and restored beside
    /// `active_postcondition`.
    active_result_datums: RefCell<Vec<(String, u32, CheckedType)>>,
    behavior: behavior::BehaviorInventory,
    /// [MOD-8] where this check finds and keeps proof receipts; without one
    /// every function is analyzed.
    receipts: Option<&'unit dyn receipts::ProofReceipts>,
    /// Per concrete function: whether its analysis stands on a receipt
    /// rather than a fresh run [FN-9].
    reused_analyses: RefCell<Vec<bool>>,
    /// The receipt key of each function analyzed afresh, recorded once its
    /// analysis is accepted.
    receipt_keys: RefCell<Vec<(usize, Vec<u8>)>>,
}

/// Checks the currently implemented active-specification semantic family.
///
/// Unsupported language families remain explicit compiler capability results;
/// only a proved numbered-rule violation becomes [`SemanticOutcome::SourceIssue`].
#[must_use]
pub fn check_semantics(resolved: &ResolvedSyntaxUnit) -> SemanticOutcome {
    check_semantics_with(resolved, true, None)
}

/// [`check_semantics`] with proof receipts [MOD-8]: a function whose
/// analysis would read exactly what a recorded accepted analysis read takes
/// that analysis's conclusions instead of running it again, and every
/// function analyzed afresh and accepted is recorded. The judgments are the
/// same; only the work differs. The checked program keeps no entailment
/// detail for a reused function, so a caller that reads more than acceptance,
/// publication, body dispositions and allocation ceilings, such as the
/// permission table, checks without receipts.
#[must_use]
pub(crate) fn check_semantics_with_receipts(
    resolved: &ResolvedSyntaxUnit,
    receipts: &dyn receipts::ProofReceipts,
) -> SemanticOutcome {
    check_semantics_with(resolved, true, Some(receipts))
}

/// [`check_semantics`] with entailment rejection disabled, so unit tests can
/// observe every retained obligation disposition
/// of one function, not only the first rejecting one. This is a test-only
/// observability hook, never a compilation mode: acceptance behavior has
/// exactly one path.
#[cfg(test)]
#[must_use]
pub(crate) fn check_semantics_dark(resolved: &ResolvedSyntaxUnit) -> SemanticOutcome {
    check_semantics_with(resolved, false, None)
}

/// Legacy test helper selecting the one shipped semantic judgment. It remains
/// only while the arithmetic obligation tests are renamed around IntegerDomain.
#[cfg(test)]
#[must_use]
pub(crate) fn check_semantics_arithmetic_obligations(
    resolved: &ResolvedSyntaxUnit,
) -> SemanticOutcome {
    check_semantics_with(resolved, true, None)
}

/// Legacy test helper selecting the one shipped semantic judgment. It remains
/// only while the division obligation tests are renamed around IntegerDomain.
#[cfg(test)]
#[must_use]
pub(crate) fn check_semantics_division_obligations(
    resolved: &ResolvedSyntaxUnit,
) -> SemanticOutcome {
    check_semantics_with(resolved, true, None)
}

fn check_semantics_with(
    resolved: &ResolvedSyntaxUnit,
    reject_entailment: bool,
    receipts: Option<&dyn receipts::ProofReceipts>,
) -> SemanticOutcome {
    let preflight = if resolved.postconditions().is_empty() {
        Ok(())
    } else {
        Checker::new(resolved, reject_entailment, None).and_then(|mut checker| {
            let items = checker.item_declarations()?;
            checker.preflight_postcondition_selectors(&items)
        })
    };
    let result = preflight.and_then(|()| {
        Checker::new(resolved, reject_entailment, receipts).and_then(|mut checker| {
            let result = checker.check_program();
            checker.finish_musttail_checks(result)
        })
    });
    match result {
        Ok(data) => SemanticOutcome::Complete(Box::new(CheckedProgram { data })),
        Err(CheckStop::Issue(issue)) => SemanticOutcome::SourceIssue { issue: *issue },
        Err(CheckStop::Resolution(issue)) => SemanticOutcome::ResolutionIssue { issue: *issue },
        Err(CheckStop::Unsupported(unsupported)) => SemanticOutcome::Unsupported { unsupported },
        Err(CheckStop::Compiler(failure)) => SemanticOutcome::CompilerFailure { failure },
        // The deferred-box signal is repaired where it is raised, one
        // function at a time, so reaching here is an internal inconsistency
        // rather than anything the source can express.
        Err(CheckStop::DeferredNominal | CheckStop::ReferenceSummaryChanged) => {
            SemanticOutcome::CompilerFailure {
                failure: SemanticCompilerFailure::InvalidResolution,
            }
        }
        Err(CheckStop::PostconditionPrerequisiteUnavailable) => SemanticOutcome::CompilerFailure {
            failure: SemanticCompilerFailure::InvalidResolution,
        },
    }
}

impl<'unit> Checker<'unit> {
    fn mark_postcondition_unavailable(&mut self, declaration: DeclarationId) {
        if !self
            .postcondition_unavailable_declarations
            .contains(&declaration)
        {
            self.postcondition_unavailable_declarations
                .push(declaration);
        }
    }

    fn postcondition_declaration_unavailable(&self, declaration: DeclarationId) -> bool {
        self.postcondition_unavailable_declarations
            .contains(&declaration)
    }

    /// [EFF-2] the body-syntactic contribution of one checked function,
    /// judged once on the written body.
    ///
    /// A generic declaration has one written body even though the checker
    /// validates concrete instances separately. The symbolic validation is
    /// the authority for that declaration-wide syntactic contribution, so
    /// its row is recorded once and reused by every concrete instance.
    /// Instance-specific proofs may discharge static obligations, but they do
    /// do not erase a written call's declared effects. The
    /// release contribution is not syntactic and stays per instance [STOR-3].
    /// One effect row in its exact [EFF-1] canonical spelling.
    ///
    /// The rejection compared two rows and published neither, so a writer was
    /// told their row was wrong and left to derive both sides by hand. Both
    /// are in hand here, and so is the exact difference.
    ///
    /// Each entry names exactly one path, every `reads` entry precedes every
    /// `writes` entry [EFF-1], so the rendered row is one a declaration can
    /// carry as written.
    fn render_effect_row(
        &self,
        effects: &EffectSet,
        signature: &FunctionSignature,
    ) -> Result<String, CheckStop> {
        let mut entries = Vec::with_capacity(effects.reads.len() + effects.writes.len());
        for (category, paths) in [("reads", &effects.reads), ("writes", &effects.writes)] {
            for path in paths {
                entries.push(format!(
                    "{category}({})",
                    self.render_effect_path(path, signature)?
                ));
            }
        }
        // [EFF-1] the row has two categories. Allocation carries no effect
        // entry [STOR-8], so an allocating boundary still writes `pure` where
        // it reads and writes nothing [EFF-2].
        Ok(if entries.is_empty() {
            "pure".to_owned()
        } else {
            entries.join(", ")
        })
    }

    /// The row an [EFF-2] rejection suggests: the exhibited row without the
    /// entries another of its entries covers.
    ///
    /// The exhibited set records each access as the body made it, so it can
    /// hold a read and a write of one path, or a write of a whole parameter
    /// beside a write below it. A `writes` entry states every access at or
    /// below its path, so [EFF-1] refuses any entry it covers, and a `reads`
    /// entry covered by another `reads` entry adds nothing to the row. What
    /// remains is exact: every entry is an exhibited path, EFF-2 admits it in
    /// both directions, and EFF-1 admits it as written. Two entries left on one
    /// parameter either overlap at every position, which [EFF-5] does not
    /// compare, or overlap only for some position values, which the call's
    /// own proof decides.
    fn suggested_effect_row(exhibited: &EffectSet) -> EffectSet {
        let mut suggested = EffectSet::NONE;
        for path in &exhibited.writes {
            let covered = exhibited
                .writes
                .iter()
                .any(|entry| entry != path && Self::effect_path_covers(entry, path));
            if !covered {
                suggested.add_write(path.clone());
            }
        }
        for path in &exhibited.reads {
            let covered_by_write = exhibited
                .writes
                .iter()
                .any(|entry| Self::effect_path_covers(entry, path));
            let covered_by_read = exhibited
                .reads
                .iter()
                .any(|entry| entry != path && Self::effect_path_covers(entry, path));
            if !covered_by_write && !covered_by_read {
                suggested.add_read(path.clone());
            }
        }
        suggested
    }

    /// One `effect_path` in its written spelling [EFF-1]: the parameter's own
    /// name followed by the suffixes that produced it. An internal `Deref`
    /// step selects a Box's `.inner`; references add no row wrapper.
    ///
    /// The walk carries the selected type only as far as it can name a step:
    /// a field name needs its containing struct and a payload field needs its
    /// containing enum. A step the checked program cannot name renders as its
    /// ordinal, which is honest rather than invented.
    fn render_effect_path(
        &self,
        path: &super::model::CheckedStatePath,
        signature: &FunctionSignature,
    ) -> Result<String, CheckStop> {
        let parameter = signature
            .parameters
            .iter()
            .find(|parameter| parameter.declaration == path.root);
        let (mut rendered, mut ty) = match parameter {
            Some(parameter) => (parameter.name.clone(), Some(parameter.ty)),
            None => (self.declaration_spelling(path.root)?, None),
        };
        for step in &path.steps {
            match step {
                super::model::CheckedEffectStep::Field(field) => {
                    let selected = match ty {
                        Some(CheckedType::Nominal(nominal)) => match &self.nominal(nominal)?.kind {
                            CheckedNominalKind::Struct { fields } => fields
                                .get(*field as usize)
                                .map(|declared| (declared.name.clone(), declared.ty)),
                            _ => None,
                        },
                        _ => None,
                    };
                    match selected {
                        Some((name, field_type)) => {
                            rendered.push('.');
                            rendered.push_str(&name);
                            ty = Some(field_type);
                        }
                        None => {
                            rendered.push_str(&format!(".{field}"));
                            ty = None;
                        }
                    }
                }
                // The internal content step retains the referent type so
                // fields below `.inner` keep their source names [TYPE-9].
                super::model::CheckedEffectStep::Deref => {
                    rendered.push_str(".inner");
                    ty = match ty {
                        Some(CheckedType::Nominal(nominal)) => match self.nominal(nominal)?.kind {
                            CheckedNominalKind::Box { referent, .. } => Some(referent),
                            _ => None,
                        },
                        _ => None,
                    };
                }
                super::model::CheckedEffectStep::Payload { variant, field } => {
                    let selected = match ty {
                        Some(CheckedType::Nominal(nominal)) => match &self.nominal(nominal)?.kind {
                            CheckedNominalKind::Enum { variants } => {
                                variants.get(*variant as usize).and_then(|declared| {
                                    declared.fields.get(*field as usize).map(|payload| {
                                        (declared.name.clone(), payload.name.clone(), payload.ty)
                                    })
                                })
                            }
                            _ => None,
                        },
                        _ => None,
                    };
                    match selected {
                        Some((variant_name, field_name, payload_type)) => {
                            rendered.push_str(&format!(".{variant_name}.{field_name}"));
                            ty = Some(payload_type);
                        }
                        None => {
                            rendered.push_str(&format!(".{variant}.{field}"));
                            ty = None;
                        }
                    }
                }
                super::model::CheckedEffectStep::Index(index) => {
                    rendered.push_str(&format!("[{}]", self.declaration_spelling(*index)?));
                    ty = None;
                }
                super::model::CheckedEffectStep::Range { start, end } => {
                    rendered.push_str(&format!(
                        "[{}..{}]",
                        self.declaration_spelling(*start)?,
                        self.declaration_spelling(*end)?
                    ));
                    ty = None;
                }
                super::model::CheckedEffectStep::Part(part) => {
                    rendered.push('.');
                    rendered.push_str(part.spelling());
                    ty = None;
                }
                super::model::CheckedEffectStep::Measure(measure) => {
                    rendered.push('.');
                    rendered.push_str(measure.spelling());
                    ty = None;
                }
            }
        }
        Ok(rendered)
    }

    /// Whether `entry` is `access` or a proper prefix of it [EFF-1].
    ///
    /// [EFF-2] states the relation as "the body accesses storage at or below
    /// its path", and an effect path is a root formal plus a complete step
    /// list, so "below" is exactly the prefix order on those steps.
    fn effect_path_covers(
        entry: &super::model::CheckedStatePath,
        access: &super::model::CheckedStatePath,
    ) -> bool {
        entry.root == access.root && access.steps.starts_with(&entry.steps)
    }

    /// [EFF-2]'s two-way judgment over a complete row.
    ///
    /// "Rows are checked both ways against this complete exhibited set --
    /// every declared entry is exhibited in that sense, and every exhibited
    /// access lies under some declared entry." That is a covering relation
    /// and not equality of path sets: a row declaring a whole reference
    /// parameter covers the measure read `deref(p).len` below it [OP-15],
    /// while a row declaring only a field is not covered by an access to the
    /// whole.
    ///
    /// The two categories are not independent. [EFF-1] states that a
    /// `writes` entry "states every access at that path and below it", so a
    /// declared write covers an exhibited read at or below its path and an
    /// exhibited write answers for a declared read. A declared write is
    /// answered only by an exhibited write: nothing subsumes a write the body
    /// never makes.
    fn effect_row_matches(declared: &EffectSet, exhibited: &EffectSet) -> bool {
        exhibited.reads.iter().all(|access| {
            declared
                .reads
                .iter()
                .chain(&declared.writes)
                .any(|entry| Self::effect_path_covers(entry, access))
        }) && exhibited.writes.iter().all(|access| {
            declared
                .writes
                .iter()
                .any(|entry| Self::effect_path_covers(entry, access))
        }) && declared.reads.iter().all(|entry| {
            exhibited
                .reads
                .iter()
                .chain(&exhibited.writes)
                .any(|access| Self::effect_path_covers(entry, access))
        }) && declared.writes.iter().all(|entry| {
            exhibited
                .writes
                .iter()
                .any(|access| Self::effect_path_covers(entry, access))
        })
    }

    /// The declared row's two [EFF-2] failures, each named by the entry the
    /// writer adds or deletes.
    ///
    /// `missing` covers each exhibited access lying under no declared entry,
    /// and names it by the entry of the suggested row that covers it, so a
    /// read and a write of one path are one missing `writes` entry [EFF-1].
    /// `extra` names each declared entry the body never accesses at or below.
    fn effect_row_difference(
        &self,
        exhibited: &EffectSet,
        suggested: &EffectSet,
        declared: &EffectSet,
        signature: &FunctionSignature,
    ) -> Result<(Vec<String>, Vec<String>), CheckStop> {
        let mut missing = Vec::new();
        let mut extra = Vec::new();
        let uncovered_reads = exhibited.reads.iter().filter(|path| {
            !declared
                .reads
                .iter()
                .chain(&declared.writes)
                .any(|entry| Self::effect_path_covers(entry, path))
        });
        let uncovered_writes = exhibited.writes.iter().filter(|path| {
            !declared
                .writes
                .iter()
                .any(|entry| Self::effect_path_covers(entry, path))
        });
        for (write, path) in uncovered_reads
            .map(|path| (false, path))
            .chain(uncovered_writes.map(|path| (true, path)))
        {
            let covering = suggested
                .writes
                .iter()
                .find(|entry| Self::effect_path_covers(entry, path))
                .map(|entry| ("writes", entry))
                .or_else(|| {
                    (!write)
                        .then(|| {
                            suggested
                                .reads
                                .iter()
                                .find(|entry| Self::effect_path_covers(entry, path))
                                .map(|entry| ("reads", entry))
                        })
                        .flatten()
                })
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            let entry = format!(
                "{}({})",
                covering.0,
                self.render_effect_path(covering.1, signature)?
            );
            if !missing.contains(&entry) {
                missing.push(entry);
            }
        }
        for entry in &declared.reads {
            if !exhibited
                .reads
                .iter()
                .chain(&exhibited.writes)
                .any(|path| Self::effect_path_covers(entry, path))
            {
                extra.push(format!(
                    "reads({})",
                    self.render_effect_path(entry, signature)?
                ));
            }
        }
        for entry in &declared.writes {
            if !exhibited
                .writes
                .iter()
                .any(|path| Self::effect_path_covers(entry, path))
            {
                extra.push(format!(
                    "writes({})",
                    self.render_effect_path(entry, signature)?
                ));
            }
        }
        Ok((missing, extra))
    }

    /// [OWN-1, FORM-1, FN-2] whether this body is the authority on the one
    /// spelling [FORM-1] keys on a value's copy/affine class.
    ///
    /// That spelling is `move p` versus a bare `p` [OWN-1]: one spelling per
    /// meaning, selected by the class. A supplied instance of a generic
    /// template is not its authority: the template's own symbolic instance
    /// judged it under the parameter's written bound, so at a copy instance a
    /// `move` of a template-affine value denotes a copy rather than reopening
    /// a judgment the template already made [PROV-6]. Every other judgment of
    /// those rules — consume-once and dead roots — is re-judged here, because
    /// each is a property of the concrete instance and not of the spelling.
    pub(in crate::semantic::check) fn judges_class_spelling(&self) -> bool {
        !self.template_spelling_authority.get()
    }

    pub(in crate::semantic::check) fn declaration_spelling(
        &self,
        declaration: DeclarationId,
    ) -> Result<String, CheckStop> {
        self.resolved
            .declarations()
            .iter()
            .find(|record| record.id() == declaration)
            .map(|record| record.spelling().to_owned())
            .ok_or_else(|| SemanticCompilerFailure::InvalidResolution.into())
    }

    /// [OP-10] whether this type parameter is a compiler-owned window type
    /// parameter: the `W` or `X` of a [PRE-1] record, supplied by an operand
    /// and never written.
    ///
    /// The spelling alone cannot decide it, because a source declaration may
    /// name a parameter `W`; the rule's own sentence is that no source
    /// declaration can write such a parameter, so the prelude origin of the
    /// declaration that introduces it is the other half of the test.
    pub(in crate::semantic::check) fn is_window_type_parameter(
        &self,
        declaration: DeclarationId,
    ) -> Result<bool, CheckStop> {
        let Some(record) = self.resolved.declaration(declaration) else {
            return Ok(false);
        };
        if !matches!(record.spelling(), "W" | "X") {
            return Ok(false);
        }
        Ok(self
            .resolved
            .syntax()
            .classified_bundle()
            .source_bundle()
            .file(record.origin().coordinate().source())
            .is_some_and(|file| file.prelude().is_some()))
    }

    fn written_body_effects(
        &self,
        signature: &FunctionSignature,
        syntactic: EffectSet,
    ) -> EffectSet {
        if signature.substitution.is_symbolic() {
            self.written_body_effect_rows
                .borrow_mut()
                .insert(signature.declaration, syntactic.clone());
            return syntactic;
        }
        if signature.substitution.len() == 0 {
            return syntactic;
        }
        let mut written = self
            .written_body_effect_rows
            .borrow()
            .get(&signature.declaration)
            .cloned()
            .unwrap_or_else(|| syntactic.clone());
        // [EFF-3, FN-2] the cached symbolic body fixes the source-written
        // path row for every instance, but allocation has no source entry and
        // can become known only after a function argument is concrete. Keep
        // that instance fact instead of replacing it with the symbolic
        // schema's necessarily absent metadata.
        written.allocates |= syntactic.allocates;
        written
    }

    /// The symbol base of one source function [MOD-3]: its plain name in the
    /// root module, its module path joined by `.` before the name in every
    /// other module of the program, and `std.` before that path in a
    /// standard library module [MOD-10], so equal names of different modules
    /// stay distinct and no program module, whose path cannot begin with the
    /// reserved `std`, shares a library module's symbols.
    pub(in crate::semantic::check) fn module_symbol_base(
        &self,
        declaration: DeclarationId,
        name: &str,
    ) -> String {
        match self.declaration_home(declaration) {
            Some((crate::Package::Standard, path)) => format!("std.{}.{name}", path.join(".")),
            Some((_, path)) if !path.is_empty() => format!("{}.{name}", path.join(".")),
            _ => name.to_owned(),
        }
    }

    /// The package and path of the module whose records declare a
    /// declaration, as its key names them; `None` for a PRE-1 declaration
    /// [MOD-3].
    pub(in crate::semantic::check) fn declaration_home(
        &self,
        declaration: DeclarationId,
    ) -> Option<(crate::Package, &[String])> {
        match self.resolved.declaration(declaration)?.key().item() {
            crate::ItemKey::Declared {
                home: crate::ItemHome::Module { package, path, .. },
                ..
            } => Some((*package, path)),
            _ => None,
        }
    }

    /// The concrete function ids of a substitution's function-kind actuals
    /// [FN-2]; an actual not yet instantiated as a signature contributes none.
    fn function_actual_ids(
        &self,
        substitution: &generics::GenericSubstitution,
    ) -> Result<Vec<super::model::FunctionId>, CheckStop> {
        let mut actuals = Vec::new();
        for argument in substitution.function_arguments() {
            if let behavior::FunctionArgument::Source {
                reference,
                concrete: true,
            } = argument
                && let Some(id) = self.function_reference_instance(reference)?
            {
                actuals.push(id);
            }
        }
        Ok(actuals)
    }

    /// Enters the module of one declaration for the judgments written in it
    /// [MOD-5, TYPE-2]: every field access and readonly write is judged from
    /// the module that writes it. The previous module is restored when the
    /// returned guard drops, so a signature built while a body is checked
    /// does not change the body's module.
    pub(in crate::semantic::check) fn enter_module(
        &self,
        declaration: DeclarationId,
    ) -> ModuleContext<'_> {
        let module = self
            .resolved
            .declaration(declaration)
            .and_then(crate::DeclarationRecord::module);
        ModuleContext {
            cell: &self.writing_module,
            previous: self.writing_module.replace(module),
        }
    }

    /// The module whose inventory declares a source nominal; `None` for a
    /// PRE-1 or compiler-owned nominal [MOD-3].
    pub(in crate::semantic::check) fn nominal_module(
        &self,
        nominal: super::model::NominalId,
    ) -> Option<crate::ModuleId> {
        let (template, _) = self
            .source_nominal_instances
            .get(nominal.0 as usize)?
            .as_ref()?;
        let declaration = self.nominal_templates.get(*template)?.declaration;
        self.resolved
            .declaration(declaration)
            .and_then(crate::DeclarationRecord::module)
    }

    /// Whether a field of a source nominal carries `public` in its
    /// declaration: a struct field when `variant` is `None`, or a payload
    /// field of that variant [MOD-6].
    pub(in crate::semantic::check) fn field_declared_public(
        &self,
        nominal: super::model::NominalId,
        variant: Option<usize>,
        field: usize,
    ) -> Result<bool, CheckStop> {
        let Some((template, _)) = self
            .source_nominal_instances
            .get(nominal.0 as usize)
            .and_then(Option::as_ref)
        else {
            return Ok(true);
        };
        let node = self
            .nominal_templates
            .get(*template)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?
            .node;
        let field_node = match variant {
            None => self
                .tree
                .children_with(node, Production::Field)?
                .get(field)
                .copied(),
            Some(variant) => {
                let variants = self.tree.children_with(node, Production::Variant)?;
                let Some(variant) = variants.get(variant) else {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                };
                match self
                    .tree
                    .first_child_with(*variant, Production::VfieldList)?
                {
                    Some(list) => self
                        .tree
                        .children_with(list, Production::Vfield)?
                        .get(field)
                        .copied(),
                    None => None,
                }
            }
        };
        let field_node = field_node.ok_or(SemanticCompilerFailure::InvalidResolution)?;
        self.has_fixed(field_node, crate::FixedTerminal::Public)
    }

    /// [MOD-5] refuses a field selection, binding or construction written in
    /// a module other than the field's declaring module when that module
    /// does not publish the field, or when the writing module's graph row
    /// does not list the declaring module. PRE-1 and compiler-owned fields
    /// keep their ordinary availability.
    pub(in crate::semantic::check) fn reject_inaccessible_field(
        &self,
        nominal: super::model::NominalId,
        variant: Option<usize>,
        field: usize,
        name: &str,
        node: NodeId,
    ) -> Result<(), CheckStop> {
        let Some(module) = self.nominal_module(nominal) else {
            return Ok(());
        };
        let public = self.field_declared_public(nominal, variant, field)?;
        // [MOD-6] a public function's contract, effect row and formals are
        // read by every client, so they name only published fields even in
        // the declaring module.
        if !public && self.in_published_header(node)? {
            return self.issue_node(
                SemanticRule::Mod6,
                node,
                SemanticIssueKind::InaccessibleField {
                    field: name.to_owned(),
                    reason: "a public function's contract, effect row and formals name only fields its clients can access; publish the field, usually as public readonly",
                },
            );
        }
        let Some(writer) = self.writing_module.get() else {
            return Ok(());
        };
        if writer == module || (public && self.module_lists(writer, module)) {
            return Ok(());
        }
        self.issue_node(
            SemanticRule::Mod5,
            node,
            SemanticIssueKind::InaccessibleField {
                field: name.to_owned(),
                reason: if public {
                    "the field's declaring module is not in this module's graph row; list it there, or reach the field through an operation of a module the row lists"
                } else {
                    "the field is private to its declaring module; publish it in that module's interface, or use one of its operations"
                },
            },
        )
    }

    /// [MOD-5] refuses an arm label written in a module other than its
    /// enum's declaring module unless the enum is public and the writing
    /// module's graph row lists the declaring module: the label is a name
    /// written in the body. PRE-1 enums keep their ordinary availability.
    pub(in crate::semantic::check) fn reject_inaccessible_variant(
        &self,
        nominal: super::model::NominalId,
        name: &str,
        arm: NodeId,
    ) -> Result<(), CheckStop> {
        let Some(module) = self.nominal_module(nominal) else {
            return Ok(());
        };
        let Some(writer) = self.writing_module.get() else {
            return Ok(());
        };
        if writer == module {
            return Ok(());
        }
        let public = self.nominal_declared_public(nominal)?;
        if public && self.module_lists(writer, module) {
            return Ok(());
        }
        self.issue_node(
            SemanticRule::Mod5,
            arm,
            SemanticIssueKind::InaccessibleVariant {
                variant: name.to_owned(),
                reason: if public {
                    "the enum's declaring module is not in this module's graph row; list it there, or match through an operation of a module the row lists"
                } else {
                    "the enum is private to its declaring module, and so are its variants"
                },
            },
        )
    }

    /// Whether `writer`'s graph row lists `module` [MOD-1, MOD-5].
    fn module_lists(&self, writer: crate::ModuleId, module: crate::ModuleId) -> bool {
        self.resolved
            .syntax()
            .classified_bundle()
            .source_bundle()
            .module(writer)
            .is_some_and(|record| record.depends_on(module))
    }

    /// Whether a source nominal's declaration carries `public` [MOD-6].
    fn nominal_declared_public(&self, nominal: super::model::NominalId) -> Result<bool, CheckStop> {
        let Some((template, _)) = self
            .source_nominal_instances
            .get(nominal.0 as usize)
            .and_then(Option::as_ref)
        else {
            return Ok(true);
        };
        let declaration = self
            .nominal_templates
            .get(*template)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?
            .declaration;
        Ok(self
            .resolved
            .declaration(declaration)
            .is_some_and(crate::DeclarationRecord::is_public))
    }

    /// Whether a node lies in a published header: a public function's
    /// parameters, results, effect row, contract or function-kind formals,
    /// and not its body, or any formal of a public interface group, which
    /// publishes its complete formal vector [MOD-6].
    fn in_published_header(&self, node: NodeId) -> Result<bool, CheckStop> {
        let mut current = Some(node);
        while let Some(candidate) = current {
            match self.tree.production(candidate)? {
                Production::Stmt | Production::Doc => return Ok(false),
                Production::FnDecl => {
                    return Ok(self
                        .optional_declaration_at(candidate, DeclarationRole::Function)?
                        .is_some_and(crate::DeclarationRecord::is_public));
                }
                Production::InterfaceDecl => {
                    return Ok(self
                        .optional_declaration_at(candidate, DeclarationRole::Interface)?
                        .is_some_and(crate::DeclarationRecord::is_public));
                }
                _ => {}
            }
            current = self.tree.parent(candidate)?;
        }
        Ok(false)
    }

    /// [TYPE-2] whether a readonly field withholds writes from the body under
    /// check: a PRE-1 field's from every body, and a source field's from
    /// every module except the one that declares its nominal.
    pub(in crate::semantic::check) fn field_withholds_writes(
        &self,
        nominal: super::model::NominalId,
        field: &super::model::CheckedField,
    ) -> bool {
        field.readonly
            && match self.nominal_module(nominal) {
                Some(module) => self.writing_module.get() != Some(module),
                None => true,
            }
    }

    /// [GRAM-2, STOR-8] whether this unit wrote `program no_heap;`.
    ///
    /// Resolution has already refused a second `heap_decl` and one at any
    /// later item position, so a `heap_decl` present anywhere under the root
    /// is the admitted first-item declaration.
    fn declares_no_heap(tree: &TreeView<'unit>) -> Result<bool, CheckStop> {
        for item in tree.children(tree.root())? {
            if tree.production(*item)? != Production::Item {
                continue;
            }
            for child in tree.children(*item)? {
                if tree.production(*child)? == Production::HeapDecl {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    fn new(
        resolved: &'unit ResolvedSyntaxUnit,
        reject_entailment: bool,
        receipts: Option<&'unit dyn receipts::ProofReceipts>,
    ) -> Result<Self, CheckStop> {
        // A semantic unit includes the fixed PRE-1 declarations before resolution.
        // A source-only parse is useful to tools but is not a complete compiler input.
        if !resolved
            .syntax()
            .finalized
            .parsed
            .classified
            .source_bundle()
            .includes_prelude()
        {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        let tree = TreeView::new(resolved)?;
        let no_heap = Self::declares_no_heap(&tree)?;
        Ok(Self {
            resolved,
            reject_entailment,
            no_heap,
            tree,
            nominals: Vec::new(),
            nominal_generation: 0,
            nominal_layouts_acyclic_at: std::cell::Cell::new(None),
            elements: RefCell::new(Vec::new()),
            element_ids: RefCell::new(HashMap::new()),
            nominal_nodes: Vec::new(),
            nominal_states: Vec::new(),
            source_nominal_instances: Vec::new(),
            box_nominals: HashMap::new(),
            result_list_nominals: HashMap::new(),
            musttail_rejections: RefCell::new(Vec::new()),
            pending_nominals: RefCell::new(Vec::new()),
            pending_instances: RefCell::new(Vec::new()),
            instance_requests: RefCell::new(Vec::new()),
            elided_store_brand: std::cell::Cell::new(None),
            template_spelling_authority: std::cell::Cell::new(false),
            commit_read_outs: RefCell::new(Vec::new()),
            call_separations: RefCell::new(Vec::new()),
            deferred_loop_reference_uses: RefCell::new(Vec::new()),
            loop_reference_summaries: RefCell::new(HashMap::new()),
            reference_origins: RefCell::new(Vec::new()),
            contract_queries: RefCell::new(Vec::new()),
            prelude_nominals: HashMap::new(),
            prelude_types: Vec::new(),
            nominal_templates: Vec::new(),
            nominal_templates_by_declaration: HashMap::new(),
            nominals_by_declaration: HashMap::new(),
            constructor_templates_by_declaration: HashMap::new(),
            signatures: Vec::new(),
            function_templates: Vec::new(),
            templates_by_declaration: HashMap::new(),
            functions_by_declaration: HashMap::new(),
            constants: HashMap::new(),
            checked_constants: Vec::new(),
            derived_consts: RefCell::new(Vec::new()),
            written_body_effect_rows: RefCell::new(HashMap::new()),
            pending_generic_requirements: Vec::new(),
            generic_requirements: Vec::new(),
            postcondition_selectors: Vec::new(),
            postcondition_unavailable_declarations: Vec::new(),
            active_postcondition: Cell::new(None),
            writing_module: Cell::new(None),
            active_result_datums: RefCell::new(Vec::new()),
            behavior: behavior::BehaviorInventory::default(),
            receipts,
            reused_analyses: RefCell::new(Vec::new()),
            receipt_keys: RefCell::new(Vec::new()),
        })
    }

    fn check_program(&mut self) -> Result<CheckedProgramData, CheckStop> {
        self.check_musttail_positions()?;
        let items = self.item_declarations()?;
        self.collect_behavior_groups(&items)?;
        self.reject_instantiation_cycles(&items)?;
        self.declare_nominals(&items)?;
        self.collect_constants(&items)?;
        self.complete_nominals()?;
        self.collect_deferred_nominal_constants(&items)?;
        self.collect_function_signatures(&items)?;
        self.admit_postcondition_selectors()?;
        self.validate_generic_templates()?;
        if self
            .signatures
            .iter()
            .any(|signature| signature.formal_parameter.is_some())
        {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }

        // Phase A completes every reachable concrete function before any
        // acceptance-bearing entailment judgment runs. This makes forward,
        // recursive, mutually recursive, and concrete generic call summaries
        // independent of function traversal order.
        // The cursor is re-read each round because checking a body may select
        // an operand-directed [PRE-1] instance [OP-10, OP-11, OP-14] the
        // syntax could not name; those signatures are appended here and are
        // checked by this same loop before phase B reads the inventory.
        let mut function_inventory = Vec::with_capacity(self.signatures.len());
        let mut index = 0_usize;
        while index < self.signatures.len() {
            function_inventory.push(self.check_function_interning_nominals(index)?);
            index = index
                .checked_add(1)
                .ok_or(SemanticCompilerFailure::CounterOverflow)?;
        }
        // Body checking may first instantiate a nominal through the fields of
        // an ordinary constructor. FN-6 has already checked the written finite
        // dependency graph, and ensure_source_nominal_instance has completed
        // each discovered instance. Lowering reads this completed inventory;
        // no hidden entry-store instance is required to seed a representation.

        // The cursor loop above closes over every signature appended while a
        // body is checked, so the dense function and signature inventories
        // have equal length at this completion boundary.
        self.close_allocation_metadata(&mut function_inventory)?;

        self.check_behavior_bindings()?;

        // Phase B reads only the completed inventory. Kill-relevant [EFF-2]
        // projections are indexed by dense function identity [ENT-5]; later
        // program-level goal summaries extend this same complete context.
        let callees = self.entailment_callees()?;
        self.install_call_requirements(&mut function_inventory)?;
        self.form_obligation_records(&mut function_inventory)?;
        let permission_signatures = self
            .signatures
            .iter()
            .map(|signature| PermissionSignature {
                name: signature.name.clone(),
                parameter_declarations: signature
                    .parameters
                    .iter()
                    .map(|parameter| parameter.declaration)
                    .collect(),
                parameter_modes: signature
                    .parameters
                    .iter()
                    .map(|parameter| parameter.mode)
                    .collect(),
                reads: signature.declared_effects.reads.clone(),
                writes: signature.declared_effects.writes.clone(),
            })
            .collect::<Vec<_>>();
        for checked in &mut function_inventory {
            checked.function.permission_separation_queries =
                plan_permission_separations(&checked.function, &permission_signatures);
        }
        let optimistic_batch = function_inventory.iter().any(|checked| {
            !checked.function.postconditions.is_empty()
                || Self::statements_contain_value_if(
                    checked.function.body.as_deref().unwrap_or_default(),
                )
        });

        let postcondition_schedule = self.analyze_function_inventory(
            &mut function_inventory,
            &callees,
            optimistic_batch,
            None,
        )?;
        let baseline_functions = function_inventory
            .iter()
            .map(|checked| &checked.function)
            .collect::<Vec<_>>();
        if self.reject_entailment {
            let mut rejections = Vec::new();
            let mut rejected = vec![false; baseline_functions.len()];
            for (index, function) in baseline_functions.iter().enumerate() {
                // A receipt stands for an accepted analysis of exactly these
                // inputs [MOD-8].
                if self.analysis_reused(index) {
                    continue;
                }
                match self
                    .entailment_rejection(function)
                    .map_err(|stop| self.attribute_to_request(function.id, stop))
                {
                    Ok(()) => {}
                    Err(CheckStop::Issue(issue)) => {
                        rejected[index] = true;
                        let path = Self::source_issue_path(&issue)?.clone();
                        rejections.push((
                            path,
                            self.concrete_instance_rank(function)?,
                            issue.rule.definition_rank(),
                            issue,
                        ));
                    }
                    Err(stop) => return Err(stop),
                }
            }
            // Every accepted fresh analysis is kept, whether or not another
            // function's rejection fails this check [MOD-8].
            self.record_receipts(&baseline_functions, &rejected);
            rejections.sort_by(|left, right| {
                left.0
                    .components()
                    .cmp(right.0.components())
                    .then(left.1.cmp(&right.1))
                    .then(left.2.cmp(&right.2))
            });
            if let Some((_, _, _, issue)) = rejections.into_iter().next() {
                return Err(CheckStop::Issue(issue));
            }
        }
        drop(baseline_functions);
        let mut functions = function_inventory
            .into_iter()
            .map(|checked| checked.function)
            .collect::<Vec<_>>();
        if optimistic_batch {
            for (index, function) in functions.iter_mut().enumerate() {
                // A receipt's analysis retains no derivation to prune.
                if !self.analysis_reused(index) {
                    finalize_function_entailment(&mut function.entailment);
                }
            }
        }
        for function in &mut functions {
            function.body_disposition = function.entailment.body_disposition;
        }
        // Copy each accepted OP-9 site's proved numeric length ceiling onto
        // the corresponding checked allocation node. This is the sole
        // semantic-to-target handoff: lowering receives a conclusion, not the
        // proof arena, and performs no proof reconstruction.
        self.install_source_allocation_bounds(&mut functions)?;

        let executable_nominal_count = self.nominals.len();
        self.materialize_generic_requirements()?;
        let derived_consts = self.derived_consts.borrow().clone();
        for (index, derived) in derived_consts.iter().enumerate() {
            for operand in [derived.left, derived.right] {
                if matches!(operand, CheckedConst::Derived(id) if id.0 as usize >= index) {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                }
            }
        }
        // Permission is a read-only legality table over the completed checked
        // program. The affine-map rule consumes a successful OP-4 disposition
        // and exact value image retained on that program; no permission rule
        // repeats a local invariant or changes source acceptance.
        let permission = analyze_permission(&functions, &permission_signatures);
        // The ledger is rendered here because only the checker still holds the
        // syntax tree the citations name. It is pure presentation over the
        // table above and reaches no decision.
        let permission_ledger = if permission
            .functions
            .iter()
            .any(|permissions| !permissions.pairs.is_empty() || !permissions.loops.is_empty())
        {
            render_ledger(&permission, &PermissionLedgerSource { tree: &self.tree })?
        } else {
            Vec::new()
        };

        Ok(CheckedProgramData {
            nominals: self.nominals.clone(),
            nominal_confinement: self
                .nominals
                .iter()
                .map(|nominal| self.confinement_regions(CheckedType::Nominal(nominal.id)))
                .collect::<Result<_, _>>()?,
            elements: self.elements.borrow().clone(),
            executable_nominal_count,
            nominal_lowering_alias: self.nominal_lowering_aliases()?,
            nominal_physical_alias: self.nominal_physical_aliases()?,
            constants: self.checked_constants.clone(),
            nominal_spellings: (0..self.nominals.len())
                .map(|index| {
                    u32::try_from(index).ok().and_then(|index| {
                        self.stable_type_spelling(CheckedType::Nominal(NominalId(index)))
                    })
                })
                .collect(),
            constant_spellings: self
                .checked_constants
                .iter()
                .map(|constant| self.module_symbol_base(constant.declaration, &constant.name))
                .collect(),
            derived_consts,
            functions,
            contract_queries: self.contract_queries.borrow().clone(),
            postcondition_schedule,
            generic_requirements: self.generic_requirements.clone(),
            permission,
            permission_ledger,
        })
    }

    /// Every item's declaration node the checker reads. An alias binds names
    /// only [MOD-4], and an interface function declaration whose definition
    /// exists is that definition's claim, checked for correspondence and not
    /// a second function [MOD-7].
    fn item_declarations(&self) -> Result<Vec<NodeId>, CheckStop> {
        let defined = self
            .resolved
            .interface_functions()
            .iter()
            .filter(|function| function.definition().is_some())
            .filter_map(|function| self.resolved.declaration(function.declaration()))
            .map(|declaration| declaration.origin().node().clone())
            .collect::<Vec<_>>();
        let mut declarations = Vec::new();
        for item in self.tree.children(self.tree.root())? {
            if self.tree.production(*item)? != Production::Item {
                return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
            }
            let declaration = self.tree.only_child(*item)?;
            match self.tree.production(declaration)? {
                Production::AliasDecl => continue,
                Production::FnDecl if defined.contains(self.tree.path(declaration)?) => continue,
                _ => {}
            }
            declarations.push(declaration);
        }
        Ok(declarations)
    }

    fn collect_function_signatures(&mut self, items: &[NodeId]) -> Result<(), CheckStop> {
        self.collect_function_templates(items)?;
        self.validate_formal_declarations()?;
        self.collect_concrete_function_signatures()
    }

    /// Collects every const declaration with a nominal-free type. Runs before
    /// nominal completion because a nominal field's array length may name an
    /// earlier const; constants containing nominal types [CONST-2]
    /// need completed field inventories and are collected by the second pass
    /// below.
    fn collect_constants(&mut self, items: &[NodeId]) -> Result<(), CheckStop> {
        let nodes = self.constant_order(items)?;
        for node in nodes {
            if self.constant_declaration_is_deferred(node)? {
                continue;
            }
            self.collect_constant(node)?;
        }
        Ok(())
    }

    /// Collects constants containing nominal types, deferred by the first
    /// pass, in item order, after `complete_nominals` has filled the field
    /// inventories they are checked against. CONST-2's declaration-before-use
    /// rule is unaffected: a nominal-free constant cannot reference a value
    /// containing a nominal type (a cvalue reference has the exact expected
    /// type), so the two passes never reorder a legal dependency.
    fn collect_deferred_nominal_constants(&mut self, items: &[NodeId]) -> Result<(), CheckStop> {
        let nodes = self.constant_order(items)?;
        for node in nodes {
            if self.constant_declaration_is_deferred(node)? {
                self.collect_constant(node)?;
            }
        }
        Ok(())
    }

    /// Every const item in dependency order [CONST-2]: a const's value
    /// follows the values of the consts it names, whatever their item order,
    /// since a module's consts are visible throughout it [MOD-3]. A const
    /// whose value depends on itself is rejected at the first const in item
    /// order that lies on the cycle.
    fn constant_order(&self, items: &[NodeId]) -> Result<Vec<NodeId>, CheckStop> {
        let nodes = items
            .iter()
            .copied()
            .filter(|node| {
                self.tree
                    .production(*node)
                    .is_ok_and(|production| production == Production::ConstDecl)
            })
            .collect::<Vec<_>>();
        let mut declarations = HashMap::new();
        for (index, node) in nodes.iter().enumerate() {
            declarations.insert(
                self.declaration_at(*node, DeclarationRole::NamedConst)?
                    .id(),
                index,
            );
        }
        let mut dependencies = vec![Vec::new(); nodes.len()];
        for (index, node) in nodes.iter().enumerate() {
            let owner = self.tree.path(*node)?.components().to_vec();
            for usage in self.resolved.lexical_uses() {
                let path = usage.origin().node().components();
                if path.len() < owner.len() || !path.starts_with(&owner) {
                    continue;
                }
                if let crate::ResolvedTarget::Source {
                    declaration,
                    class: crate::DeclarationClass::NamedConst,
                } = usage.target()
                    && let Some(target) = declarations.get(&declaration)
                    && !dependencies[index].contains(target)
                {
                    dependencies[index].push(*target);
                }
            }
        }
        // 0: unvisited, 1: on the current path, 2: ordered.
        let mut state = vec![0_u8; nodes.len()];
        let mut order = Vec::with_capacity(nodes.len());
        for root in 0..nodes.len() {
            if state[root] != 0 {
                continue;
            }
            let mut stack = vec![(root, 0_usize)];
            state[root] = 1;
            while let Some((current, next)) = stack.last_mut() {
                let current = *current;
                if let Some(&dependency) = dependencies[current].get(*next) {
                    *next += 1;
                    match state[dependency] {
                        0 => {
                            state[dependency] = 1;
                            stack.push((dependency, 0));
                        }
                        1 => {
                            let start = stack
                                .iter()
                                .position(|(member, _)| *member == dependency)
                                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                            let members = stack[start..]
                                .iter()
                                .map(|(member, _)| *member)
                                .collect::<Vec<_>>();
                            let first = members
                                .iter()
                                .copied()
                                .min()
                                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                            let at = members
                                .iter()
                                .position(|member| *member == first)
                                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                            let cycle = members[at..]
                                .iter()
                                .chain(&members[..at])
                                .map(|member| self.identifier(nodes[*member]))
                                .collect::<Result<Vec<_>, _>>()?;
                            return self.issue_node(
                                SemanticRule::Const2,
                                nodes[first],
                                SemanticIssueKind::ConstantCycle { cycle },
                            );
                        }
                        _ => {}
                    }
                } else {
                    state[current] = 2;
                    order.push(nodes[current]);
                    stack.pop();
                }
            }
        }
        Ok(order)
    }

    fn constant_declaration_is_deferred(&self, node: NodeId) -> Result<bool, CheckStop> {
        let ty = self
            .tree
            .first_child_with(node, Production::Type)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let mut pending = vec![ty];
        while let Some(node) = pending.pop() {
            if self.tree.production(node)? == Production::Type && self.tree.names_nominal(node)? {
                return Ok(true);
            }
            pending.extend(self.tree.children(node)?.iter().copied());
        }
        Ok(false)
    }

    pub(super) fn collect_constants_for_postconditions(
        &mut self,
        items: &[NodeId],
    ) -> Result<(), CheckStop> {
        let nodes = self.constant_order(items)?;
        for node in nodes {
            let declaration = self.declaration_at(node, DeclarationRole::NamedConst)?.id();
            if !self.postcondition_constant_has_links(node)? {
                self.mark_postcondition_unavailable(declaration);
                continue;
            }
            // A nominal-typed const [CONST-2 candidate] is conservatively
            // unavailable to the FN-9 selector preflight for now; ordinary
            // checking collects it through the deferred second pass.
            if self.constant_declaration_is_deferred(node)? {
                self.mark_postcondition_unavailable(declaration);
                continue;
            }
            let ty = self
                .tree
                .first_child_with(node, Production::Type)?
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            let checkpoint = self.nominal_checkpoint();
            match self.ensure_nominal_type(ty, &GenericSubstitution::default()) {
                Ok(()) => {}
                Err(
                    CheckStop::Issue(_)
                    | CheckStop::Unsupported(_)
                    | CheckStop::PostconditionPrerequisiteUnavailable,
                ) => {
                    self.restore_nominal_checkpoint(checkpoint)?;
                    self.mark_postcondition_unavailable(declaration);
                    continue;
                }
                Err(stop) => return Err(stop),
            }
            match self.collect_constant(node) {
                Ok(()) => {}
                Err(CheckStop::Issue(_) | CheckStop::Unsupported(_)) => {
                    self.mark_postcondition_unavailable(declaration);
                }
                Err(stop) => return Err(stop),
            }
        }
        Ok(())
    }

    fn postcondition_constant_has_links(&self, node: NodeId) -> Result<bool, CheckStop> {
        let owner = self.tree.path(node)?.components();
        if self.resolved.lexical_uses().iter().any(|usage| {
            let path = usage.origin().node().components();
            path.len() >= owner.len()
                && path.starts_with(owner)
                && matches!(
                    usage.target(),
                    crate::ResolvedTarget::Source {
                        declaration,
                        class: crate::DeclarationClass::NamedConst,
                    } if !self.constants.contains_key(&declaration)
                )
        }) {
            return Ok(false);
        }
        for ty in self.tree.descendants_with(node, Production::Type)? {
            if self.tree.names_nominal(ty)?
                && !self
                    .resolved
                    .lexical_uses_at(ty)
                    .any(|usage| usage.role() == crate::LexicalUseRole::Type)
            {
                return Ok(false);
            }
        }
        let value = self
            .tree
            .first_child_with(node, Production::Cvalue)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        if self.tree.direct_token_indices(value)?.len() == 1
            && self
                .tree
                .direct_token_with(value, crate::TerminalPredicate::Identifier)?
                .is_some()
            && !self
                .resolved
                .lexical_uses_at(value)
                .any(|usage| usage.role() == crate::LexicalUseRole::ConstValue)
        {
            return Ok(false);
        }
        Ok(true)
    }

    /// Checks one const in its declaring module [MOD-5]: a construction of
    /// another module's struct in its value obeys that module's publication.
    fn collect_constant(&mut self, node: NodeId) -> Result<(), CheckStop> {
        let module = self
            .declaration_at(node, DeclarationRole::NamedConst)?
            .module();
        let previous = self.writing_module.replace(module);
        let result = self.collect_constant_in_module(node);
        self.writing_module.set(previous);
        result
    }

    fn collect_constant_in_module(&mut self, node: NodeId) -> Result<(), CheckStop> {
        let declaration = self.declaration_at(node, DeclarationRole::NamedConst)?;
        let declaration_id = declaration.id();
        let name = declaration.spelling().to_owned();
        // CONST-2 can be the first use of a concrete nominal. Prepare the
        // declared type and every written initializer argument before the
        // read-only type/value checks, just as for ordinary constructions.
        self.ensure_nominals_in_node(node, &GenericSubstitution::default())?;
        let ty_node = self
            .tree
            .first_child_with(node, Production::Type)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let ty = self.parse_const_type(ty_node)?;
        let declared_type = self.parse_type(ty_node)?;
        let value_node = self
            .tree
            .first_child_with(node, Production::Cvalue)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let value = self.parse_const_value(value_node, ty)?;
        let id = CheckedConstantId(
            u32::try_from(self.checked_constants.len())
                .map_err(|_| SemanticCompilerFailure::CounterOverflow)?,
        );
        self.checked_constants.push(CheckedConstant {
            id,
            declaration: declaration_id,
            name,
            declared_type,
            ty,
            value,
        });
        self.constants.insert(declaration_id, id);
        Ok(())
    }

    /// Checks one concrete function for the phase-A inventory, interning the
    /// nominal instances its derived types name.
    ///
    /// A derived type has no written form anywhere for the interning pass to
    /// have found — a purely local `box<T>` [STOR-2], the `Result<T, E>` a
    /// checked arithmetic row produces — and checking is `&self`, so the miss
    /// is reported as [`CheckStop::DeferredNominal`] and repaired here. Each
    /// attempt must intern at least one new nominal, which bounds the loop by
    /// the finitely many types one function can name.
    fn check_function_interning_nominals(
        &mut self,
        index: usize,
    ) -> Result<CheckedFunctionInventory, CheckStop> {
        loop {
            match self.check_function_inventory(index) {
                Err(CheckStop::DeferredNominal) => {
                    let pending = std::mem::take(&mut *self.pending_nominals.borrow_mut());
                    let instances = std::mem::take(&mut *self.pending_instances.borrow_mut());
                    let before = self.nominals.len();
                    let before_signatures = self.signatures.len();
                    for (template, substitution) in instances {
                        self.ensure_operand_directed_instance(template, substitution)?;
                    }
                    for nominal in pending {
                        match nominal {
                            PendingNominal::Box(referent) => {
                                self.intern_box_nominal(referent)?;
                            }
                            PendingNominal::ResultList(results) => {
                                self.intern_result_list_nominal(&results)?;
                            }
                            PendingNominal::Prelude(ty) => {
                                self.intern_prelude_nominal(ty)?;
                            }
                            PendingNominal::SourceInstance {
                                template,
                                substitution,
                            } => {
                                self.ensure_source_nominal_instance(template, substitution)?;
                            }
                        }
                    }
                    if self.nominals.len() == before && self.signatures.len() == before_signatures {
                        return Err(SemanticCompilerFailure::InvalidResolution.into());
                    }
                }
                outcome => {
                    return outcome.map_err(|stop| match u32::try_from(index) {
                        Ok(ordinal) => self.attribute_to_request(FunctionId(ordinal), stop),
                        Err(_) => stop,
                    });
                }
            }
        }
    }

    /// [FN-2, MOD-8] records `call` as the requester of the instance of
    /// `template` at `substitution`, unless an earlier call already is.
    pub(super) fn record_instance_request(
        &self,
        template: NodeId,
        substitution: &generics::GenericSubstitution,
        call: NodeId,
    ) {
        let mut requests = self.instance_requests.borrow_mut();
        if !requests
            .iter()
            .any(|(node, known, _)| *node == template && known == substitution)
        {
            requests.push((template, substitution.clone(), call));
        }
    }

    /// [FN-2, MOD-8] names `call` as the requester on a rejection that names
    /// none yet. The rejection keeps its location in the template.
    pub(super) fn attribute_to_call(&self, call: NodeId, stop: CheckStop) -> CheckStop {
        match stop {
            CheckStop::Issue(mut issue) if issue.request.is_none() => {
                issue.request = self.tree.coordinate(call).ok();
                CheckStop::Issue(issue)
            }
            stop => stop,
        }
    }

    /// [FN-2, MOD-8] names the call that requested `function` on a rejection
    /// raised while checking it, when `function` is a requested concrete
    /// instance of a generic template.
    pub(super) fn attribute_to_request(&self, function: FunctionId, stop: CheckStop) -> CheckStop {
        let request = self
            .signatures
            .get(function.0 as usize)
            .filter(|signature| signature.id == function)
            .and_then(|signature| {
                self.instance_requests
                    .borrow()
                    .iter()
                    .find(|(node, substitution, _)| {
                        *node == signature.node && *substitution == signature.substitution
                    })
                    .map(|(_, _, call)| *call)
            });
        match request {
            Some(call) => self.attribute_to_call(call, stop),
            None => stop,
        }
    }

    fn check_function_inventory(
        &self,
        index: usize,
    ) -> Result<CheckedFunctionInventory, CheckStop> {
        let signature = self
            .signatures
            .get(index)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        self.check_function_signature(signature)
    }

    /// [FN-2, OWN-1, PROV-6] one body, checked with the template's spelling
    /// authority recorded for the instance it is.
    ///
    /// A supplied instance of a generic template is exactly the body whose
    /// [OWN-1]/[FORM-1] spelling the template's own symbolic instance judges
    /// under the parameter's written bound, so this instance does not re-judge
    /// it. In particular, a caller may fix one argument to a copy type while
    /// forwarding another still-symbolic type, const or function parameter.
    /// Only the template's own symbolic instance and a nongeneric body judge
    /// the written spelling here.
    fn check_function_signature(
        &self,
        signature: &FunctionSignature,
    ) -> Result<CheckedFunctionInventory, CheckStop> {
        let previous = self
            .template_spelling_authority
            .replace(signature.substitution.len() > 0 && !signature.substitution.is_symbolic());
        self.loop_reference_summaries.borrow_mut().clear();
        let queries = self.contract_queries.borrow().len();
        let tail_rejections = self.musttail_rejections.borrow().len();
        let outcome = loop {
            self.call_separations.borrow_mut().clear();
            self.contract_queries.borrow_mut().truncate(queries);
            // Only the settled body may contribute FN-10 refusals. Keep the
            // position checks and earlier functions outside this attempt.
            self.musttail_rejections
                .borrow_mut()
                .truncate(tail_rejections);
            match self.check_function_signature_body(signature) {
                Err(CheckStop::ReferenceSummaryChanged) => continue,
                outcome => break outcome,
            }
        };
        if matches!(outcome, Err(CheckStop::DeferredNominal)) {
            self.musttail_rejections
                .borrow_mut()
                .truncate(tail_rejections);
        }
        self.template_spelling_authority.set(previous);
        outcome
    }

    /// [OP-10, OP-14] whether this instance still carries a compiler-owned
    /// window type parameter unsubstituted, which is exactly the symbolic
    /// instance of a window row.
    fn has_unsupplied_window_type_parameter(
        &self,
        signature: &FunctionSignature,
    ) -> Result<bool, CheckStop> {
        for (key, argument) in signature.substitution.entries() {
            let generics::GenericParameterKey::Source(declaration) = key else {
                continue;
            };
            let generics::GenericArgument::Type(ty) = argument else {
                continue;
            };
            if !self.is_window_type_parameter(*declaration)? {
                continue;
            }
            // Once an operand has supplied the shape, `deref(window).len`
            // names a measure of one of [MSR-1]'s measured types whatever
            // that shape's element type and capacity still are. [OP-14] also
            // admits a Box holding a runtime-capacity window; its concrete W
            // is not itself measured, but the prelude clause's measure place
            // instantiates as `window.inner` [TYPE-9]. Only the unsubstituted
            // parameter of the symbolic schema instance has neither form.
            let boxed_runtime_window = matches!(
                self.box_content(*ty)?,
                Some(CheckedType::Window { capacity: None, .. })
            );
            if ty.measured().is_none() && !boxed_runtime_window {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn check_function_signature_body(
        &self,
        signature: &FunctionSignature,
    ) -> Result<CheckedFunctionInventory, CheckStop> {
        // Function-local loop and binding ids restart for every inventory
        // attempt, including a DeferredNominal retry and the generic scratch
        // inventory. No deferred REF-2 dependency may cross that namespace
        // boundary.
        self.deferred_loop_reference_uses.borrow_mut().clear();
        self.reference_origins.borrow_mut().clear();
        let _module = self.enter_module(signature.declaration);
        self.check_musttail_callees(signature)?;
        self.check_entry_formers(signature)?;
        let mut bindings = HashMap::new();
        let mut parameters = Vec::with_capacity(signature.parameters.len());
        let mut next_binding = 0_u32;
        let mut next_loop = 0_u32;
        let mut binding_names = signature
            .parameters
            .iter()
            .map(|parameter| parameter.name.clone())
            .collect::<Vec<_>>();
        for parameter in &signature.parameters {
            let binding = BindingId(next_binding);
            next_binding = next_binding
                .checked_add(1)
                .ok_or(SemanticCompilerFailure::CounterOverflow)?;
            let local = self.parameter_local(parameter, binding)?;
            if let Some(reference) = &local.reference {
                self.record_reference_origins(binding, &reference.paths);
            }
            bindings.insert(parameter.declaration, local);
            parameters.push(CheckedParameter {
                name: parameter.name.clone(),
                declaration: parameter.declaration,
                node_path: parameter.node_path.clone(),
                binding,
                mode: parameter.mode,
                ty: parameter.ty,
                range_element: (parameter.mode == CheckedMode::Range)
                    .then(|| self.intern_element(parameter.ty))
                    .transpose()?,
            });
        }

        let mut counters = ControlCounters {
            next_binding: &mut next_binding,
            next_loop: &mut next_loop,
            binding_names: &mut binding_names,
        };
        let parameter_bindings = bindings.clone();
        // [OP-10] a compiler-owned window type parameter is supplied by the
        // operand and never written, so a row carrying one has no symbolic
        // instance any call can name: `deref(window).cap` names a measure of
        // the shape the operand supplies, and the unsubstituted parameter is
        // not one of [MSR-1]'s measured types. Such a row's clauses are the
        // substituted clauses of each concrete instance, which the ordinary
        // judgment below reaches once an operand fixes W. Every other generic
        // row, prelude or source, keeps its symbolic judgment.
        let unsupplied_window_row = self.has_unsupplied_window_type_parameter(signature)?;
        let (requirements, requirement_places) = if let Some(node) = self
            .tree
            .first_child_with(signature.node, Production::ContractBlock)?
            .filter(|_| !unsupplied_window_row)
        {
            let mut requires_bindings = parameter_bindings.clone();
            let checked =
                self.check_requires(signature, node, &mut requires_bindings, &mut counters)?;
            (checked.requirements, checked.places)
        } else {
            (Vec::new(), Vec::new())
        };

        let postcondition_selectors = if unsupplied_window_row {
            Vec::new()
        } else {
            self.postcondition_selectors_for_signature(signature)?
        };
        let mut postcondition_relations = Vec::with_capacity(postcondition_selectors.len());
        for selector in &postcondition_selectors {
            let mut postcondition_bindings = parameter_bindings.clone();
            postcondition_relations.push(self.check_postcondition_clause(
                signature,
                selector,
                &mut postcondition_bindings,
                &mut counters,
            )?);
        }
        self.check_published_relation_consistency(
            signature,
            &postcondition_selectors,
            &postcondition_relations,
        )?;

        bindings = parameter_bindings;
        let statements = self.tree.children_with(signature.node, Production::Stmt)?;
        let mut checked = self.check_block(
            signature,
            &statements,
            &mut bindings,
            &mut counters,
            ControlScope {
                loops: &[],
                give_context: None,
            },
        )?;
        if !self.deferred_loop_reference_uses.borrow().is_empty() {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        // A function-kind formal and a pending interface declaration
        // [MOD-8] are body-less leaves: their written boundary is what their
        // callers use, and nothing is checked below it.
        let declaration_only = self.tree.is_body_less(signature.node)?;
        if declaration_only {
            checked.can_continue = false;
            checked.effects = signature.declared_effects.clone();
        }
        if checked.can_continue {
            return Err(CheckStop::source_issue(SemanticIssue {
                rule: SemanticRule::Fn1,
                location: SemanticLocation::SourceNode(
                    self.tree.path(signature.node)?.clone(),
                    self.tree.closing_brace_coordinate(signature.node)?,
                ),
                kind: SemanticIssueKind::FunctionFallthrough,
                request: None,
            }));
        }
        let exhibited = self.written_body_effects(signature, checked.effects.clone());
        self.validate_release_graphs(&checked.statements)?;
        // [EFF-1] the row has exactly two categories, and [STOR-8] gives
        // allocation no entry in it, so [EFF-2]'s judgment is over `reads`
        // and `writes` alone. The allocation fact is [EFF-3] checked-program
        // metadata that no declaration can write, so comparing it here would
        // reject every `pure` function that calls a construction row against
        // a row it had no way to declare.
        //
        // Each category is judged by [EFF-2]'s own two-way covering relation
        // rather than by set equality.
        if !Self::effect_row_matches(&signature.declared_effects, &exhibited) {
            let suggested = Self::suggested_effect_row(&exhibited);
            let (missing, extra) = self.effect_row_difference(
                &exhibited,
                &suggested,
                &signature.declared_effects,
                signature,
            )?;
            // [EFF-2] the repair is the suggested row itself: a row EFF-1 and
            // EFF-2 admit for this body and no call refuses against itself.
            let expected_row = self.render_effect_row(&suggested, signature)?;
            return self.issue_node(
                SemanticRule::Eff2,
                signature.effects_node,
                SemanticIssueKind::EffectMismatch {
                    mechanical_fix: format!(
                        "declare the row as `{expected_row}`, which covers every access the body makes and no other"
                    ),
                    expected_row,
                    found_row: self.render_effect_row(&signature.declared_effects, signature)?,
                    missing,
                    extra,
                },
            );
        }
        let postconditions = if signature.substitution.is_concrete(&self.elements.borrow()) {
            postcondition_selectors
                .into_iter()
                .zip(postcondition_relations)
                .map(|(selector, relation)| {
                    self.build_checked_postcondition(
                        signature,
                        &parameters,
                        selector,
                        relation,
                        &checked.statements,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?
        } else {
            postcondition_selectors
                .into_iter()
                .zip(postcondition_relations)
                .map(|(selector, relation)| {
                    self.build_checked_schema_postcondition(
                        signature,
                        &parameters,
                        selector,
                        relation,
                        &checked.statements,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .flatten()
                .collect()
        };
        let function = CheckedFunction {
            formal_hypothesis: signature.formal_parameter.is_some(),
            id: signature.id,
            declaration: signature.declaration,
            module: self
                .resolved
                .declaration(signature.declaration)
                .and_then(crate::DeclarationRecord::module)
                .unwrap_or(crate::ModuleId::BUNDLE_ROOT),
            name: signature.name.clone(),
            symbol: signature.symbol.clone(),
            function_actuals: self.function_actual_ids(&signature.substitution)?,
            region_parameters: signature.region_parameters.clone(),
            parameters,
            result_mode: signature.result_mode,
            result: signature.result,
            declared_state_writes: signature.declared_effects.writes.clone(),
            // [EFF-3] the boundary's allocation fact is what the body
            // exhibits, not what the declaration wrote: no declaration can
            // write it [EFF-1, STOR-8], and for a body-less row the exhibited
            // set is the declared one, so a construction row still reports
            // its own allocation here.
            allocates: exhibited.allocates,
            requirements,
            requirement_places,
            postconditions,
            body: (!declaration_only).then_some(checked.statements),
            reference_origins: std::mem::take(&mut *self.reference_origins.borrow_mut()),
            body_disposition: super::model::CheckedBodyDisposition::Inhabited,
            call_separations: {
                let mut separations = std::mem::take(&mut *self.call_separations.borrow_mut());
                separations.sort_by_key(|separation| separation.site.components().to_vec());
                separations
            },
            permission_separation_queries: Vec::new(),
            obligations: Vec::new(),
            entailment: super::entailment::FunctionEntailment::default(),
        };
        Ok(CheckedFunctionInventory {
            function,
            binding_names,
        })
    }

    fn statements_contain_value_if(statements: &[CheckedStatement]) -> bool {
        statements.iter().any(|statement| match statement {
            CheckedStatement::ValueMatchLet { kind, arms, .. } => {
                *kind == ValueInitializerKind::ValueIf
                    || arms
                        .iter()
                        .any(|arm| Self::statements_contain_value_if(&arm.body))
            }
            CheckedStatement::Match { arms, .. } => arms
                .iter()
                .any(|arm| Self::statements_contain_value_if(&arm.body)),
            CheckedStatement::Loop { body, .. } | CheckedStatement::CountedRange { body, .. } => {
                Self::statements_contain_value_if(body)
            }
            CheckedStatement::Let { .. }
            | CheckedStatement::DestructuringLet { .. }
            | CheckedStatement::PropagateLet { .. }
            | CheckedStatement::Set { .. }
            | CheckedStatement::Evaluate { .. }
            | CheckedStatement::DropExpression { .. }
            | CheckedStatement::Proof(_)
            | CheckedStatement::Return { .. }
            | CheckedStatement::Give { .. }
            | CheckedStatement::Break { .. } => false,
        })
    }

    /// Dense kill-relevant callee inventory shared by concrete and symbolic
    /// source-schema entailment. Function identity must be a true vector
    /// index; silently skipping a malformed identity would make masks observe
    /// a different program from the baseline.
    fn entailment_callees(&self) -> Result<Vec<EntailmentCallee>, CheckStop> {
        let mut callees = Vec::with_capacity(self.signatures.len());
        for (index, signature) in self.signatures.iter().enumerate() {
            if signature.id.0 as usize != index {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            }
            callees.push(EntailmentCallee::from_signature(
                signature
                    .parameters
                    .iter()
                    .map(|parameter| (parameter.declaration, parameter.mode, parameter.ty)),
                &signature.declared_effects.writes,
            ));
        }
        Ok(callees)
    }

    fn parameter_local(
        &self,
        parameter: &ParameterSignature,
        binding: BindingId,
    ) -> Result<LocalBinding, CheckStop> {
        Ok(LocalBinding {
            binding,
            declaration: parameter.declaration,
            mode: parameter.mode,
            ty: parameter.ty,
            live: true,
            loop_depth: 0,
            compiler_updated: false,
            // [REF-1] a reference parameter arrives naming the caller's path
            // by substitution [EFF-5]; inside this body the parameter name is
            // that path, so its set anchors at itself and every resolution
            // through it terminates there.
            reference: parameter.mode.is_reference().then(|| {
                ReferenceInfo::formed(
                    if parameter.mode == CheckedMode::Range {
                        references::ReferenceKind::Range
                    } else {
                        references::ReferenceKind::Single
                    },
                    ResolvedPlace::binding(binding),
                )
            }),
            refinement_witnesses: Vec::new(),
        })
    }

    /// Checks the source-generic body while symbolic nominal and function
    /// identities are still alive.
    /// Checks every source-generic body once with symbolic arguments, even when
    /// no concrete instantiation is reachable from the executable program.
    /// The ordinary entailment engine is the only acceptance path: generic
    /// bodies do not receive a separate proof language or an assertion-based
    /// exception.
    fn validate_generic_body_entailment(
        &self,
        functions: &mut [CheckedFunctionInventory],
        canonical: &[(usize, DeclarationId)],
        callees: &[EntailmentCallee],
    ) -> Result<(), CheckStop> {
        let optimistic_batch = functions.iter().any(|checked| {
            !checked.function.postconditions.is_empty()
                || Self::statements_contain_value_if(
                    checked.function.body.as_deref().unwrap_or_default(),
                )
        });
        // Only the canonical instances are judged below, and a judged body
        // reads another function's analysis solely through the postcondition
        // summaries of its callees. The other bodies of this scratch
        // inventory — every nongeneric function among them — are analyzed
        // again by the concrete phase, so analyzing them here would repeat
        // that whole cost for a result nothing reads.
        let analyzed = Self::generic_validation_scope(functions, canonical)?;
        self.analyze_function_inventory(functions, callees, optimistic_batch, Some(&analyzed))?;
        if optimistic_batch {
            for (checked, analyzed) in functions.iter_mut().zip(&analyzed) {
                if *analyzed {
                    finalize_function_entailment(&mut checked.function.entailment);
                }
            }
        }
        if !self.reject_entailment {
            return Ok(());
        }
        for (index, declaration) in canonical {
            let checked = functions
                .get(*index)
                .filter(|checked| checked.function.declaration == *declaration)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            self.entailment_rejection(&checked.function)?;
        }
        Ok(())
    }
    /// The functions whose bodies symbolic validation must analyze: the
    /// canonical instances and everything their calls reach, by the same
    /// call-graph shape the postcondition schedule is built from.
    ///
    /// The set is closed under callees, so a strongly connected component is
    /// either wholly inside it or wholly outside, and every summary an
    /// analyzed body can consume is published by an analyzed component.
    fn generic_validation_scope(
        functions: &[CheckedFunctionInventory],
        canonical: &[(usize, DeclarationId)],
    ) -> Result<Vec<bool>, CheckStop> {
        let mut analyzed = vec![false; functions.len()];
        let mut pending = canonical
            .iter()
            .map(|(index, _)| *index)
            .collect::<Vec<_>>();
        let mut calls = Vec::new();
        while let Some(index) = pending.pop() {
            let slot = analyzed
                .get_mut(index)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            if std::mem::replace(slot, true) {
                continue;
            }
            let function = &functions[index].function;
            calls.clear();
            collect_statement_calls(
                function.id,
                function.body.as_deref().unwrap_or_default(),
                &mut calls,
            );
            pending.extend(calls.iter().map(|call| call.callee.0 as usize));
        }
        Ok(analyzed)
    }

    /// Analyzes every function of the inventory, or only those `analyzed`
    /// marks. A caller that restricts the set must close it under callees.
    fn analyze_function_inventory(
        &self,
        functions: &mut [CheckedFunctionInventory],
        callees: &[EntailmentCallee],
        optimistic_batch: bool,
        analyzed: Option<&[bool]>,
    ) -> Result<PostconditionSchedule, CheckStop> {
        let selected = |index: usize| analyzed.is_none_or(|analyzed| analyzed[index]);
        let contract_queries = self.contract_queries.borrow().clone();
        let const_parameter_types = self.const_generic_types().collect();
        // [MOD-8] the concrete inventory's analyses may stand on receipts;
        // the symbolic validation of generic templates always runs afresh.
        let receipts = self
            .receipts
            .filter(|_| analyzed.is_none() && self.reject_entailment);
        let items = receipts.map(|_| self.receipt_items()).transpose()?;
        if receipts.is_some() {
            *self.reused_analyses.borrow_mut() = vec![false; functions.len()];
        }
        // ENT is the single acceptance-bearing proof path for ordinary
        // obligations, call requirements, invariants and postconditions.
        let mut schedule =
            postcondition_schedule(functions.iter().map(|checked| &checked.function))
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        if schedule.components.is_empty() {
            for index in 0..functions.len() {
                if !selected(index) {
                    continue;
                }
                if let (Some(store), Some(items)) = (receipts, &items)
                    && let Some(entailment) = self.recorded_analysis(
                        store,
                        items,
                        functions,
                        index,
                        callees,
                        &[],
                        &const_parameter_types,
                    )
                {
                    functions[index].function.entailment = entailment;
                    continue;
                }
                let checked = &mut functions[index];
                let context = EntailmentContext {
                    declarations: self.resolved.declarations(),
                    callees,
                    constants: &self.checked_constants,
                    constant_ids: &self.constants,
                    const_parameter_types: &const_parameter_types,
                    nominals: &self.nominals,
                    elements: &self.elements.borrow(),
                    contract_queries: &contract_queries,
                    verified_postconditions: &[],
                    verified_postcondition_proofs: &[],
                    binding_names: &checked.binding_names,
                };
                checked.function.entailment = if optimistic_batch {
                    analyze_function_candidate(&checked.function, &context)
                } else {
                    analyze_function(&checked.function, &context)
                };
            }
        } else {
            for component in &mut schedule.components {
                // Callee closure keeps components whole: skipping one skips
                // both its analysis and the summaries no analyzed body reads.
                if !component
                    .functions
                    .iter()
                    .any(|function| selected(function.0 as usize))
                {
                    continue;
                }
                for function in &component.functions {
                    let function_index = function.0 as usize;
                    let verified_postconditions = functions
                        .iter()
                        .map(|checked| {
                            checked
                                .function
                                .entailment
                                .postconditions
                                .iter()
                                .filter(|proof| {
                                    proof.summary.as_ref().is_some_and(|summary| {
                                        summary.component < component.ordinal
                                    })
                                })
                                .filter_map(|proof| {
                                    checked
                                        .function
                                        .postconditions
                                        .get(proof.relation_ordinal as usize)
                                })
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>();
                    let verified_postcondition_proofs = functions
                        .iter()
                        .map(|checked| {
                            checked
                                .function
                                .entailment
                                .postconditions
                                .iter()
                                .filter(|proof| {
                                    proof.summary.as_ref().is_some_and(|summary| {
                                        summary.component < component.ordinal
                                    })
                                })
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>();
                    let checked = functions
                        .get(function_index)
                        .filter(|checked| checked.function.id == *function)
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                    let recorded = match (receipts, &items) {
                        (Some(store), Some(items)) => self.recorded_analysis(
                            store,
                            items,
                            functions,
                            function_index,
                            callees,
                            &verified_postconditions,
                            &const_parameter_types,
                        ),
                        _ => None,
                    };
                    let entailment = if let Some(entailment) = recorded {
                        entailment
                    } else {
                        let context = EntailmentContext {
                            declarations: self.resolved.declarations(),
                            callees,
                            constants: &self.checked_constants,
                            constant_ids: &self.constants,
                            const_parameter_types: &const_parameter_types,
                            nominals: &self.nominals,
                            elements: &self.elements.borrow(),
                            contract_queries: &contract_queries,
                            verified_postconditions: &verified_postconditions,
                            verified_postcondition_proofs: &verified_postcondition_proofs,
                            binding_names: &checked.binding_names,
                        };
                        analyze_function_candidate(&checked.function, &context)
                    };
                    drop(verified_postconditions);
                    drop(verified_postcondition_proofs);
                    functions[function_index].function.entailment = entailment;
                }

                let publish = component.functions.iter().all(|function| {
                    let checked = &functions[function.0 as usize].function;
                    checked
                        .entailment
                        .loop_invariants
                        .iter()
                        .all(|invariant| invariant.proof.discharged())
                        && (matches!(
                            checked.entailment.body_disposition,
                            super::model::CheckedBodyDisposition::Uninhabited { .. }
                        ) || checked.postconditions.is_empty()
                            || (checked.entailment.postconditions.len()
                                == checked.postconditions.len()
                                && checked
                                    .entailment
                                    .postconditions
                                    .iter()
                                    .all(|proof| proof.aggregate.discharged)))
                });
                if publish {
                    for function in &component.functions {
                        let checked = &mut functions[function.0 as usize].function;
                        if matches!(
                            checked.entailment.body_disposition,
                            super::model::CheckedBodyDisposition::Uninhabited { .. }
                        ) {
                            continue;
                        }
                        for proof in &mut checked.entailment.postconditions {
                            let summary = VerifiedPostconditionSummary {
                                function: *function,
                                block: proof.block.clone(),
                                relation_ordinal: proof.relation_ordinal,
                                component: component.ordinal,
                            };
                            proof.summary = Some(summary.clone());
                            component.summaries.push(summary);
                        }
                    }
                }
            }
        }
        Ok(schedule)
    }
    fn concrete_instance_rank(
        &self,
        function: &CheckedFunction,
    ) -> Result<u32, SemanticCompilerFailure> {
        let signature = self
            .signatures
            .get(function.id.0 as usize)
            .filter(|signature| signature.id == function.id)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        if signature.substitution.len() == 0 {
            Ok(0)
        } else {
            function
                .id
                .0
                .checked_add(1)
                .ok_or(SemanticCompilerFailure::CounterOverflow)
        }
    }

    fn install_call_requirements(
        &self,
        functions: &mut [CheckedFunctionInventory],
    ) -> Result<(), CheckStop> {
        let requirements = functions
            .iter()
            .map(|checked| checked.function.requirements.clone())
            .collect::<Vec<_>>();
        for checked in functions {
            if let Some(body) = &mut checked.function.body {
                self.install_statement_call_requirements(body, &requirements)?;
            }
        }
        Ok(())
    }

    /// Completes [EFF-3]'s finite allocation fact over the checked call graph.
    ///
    /// Phase A has already checked every reachable body, so each function's
    /// current bit is the directly exhibited base fact. Propagating from an
    /// allocating callee to its callers closes forward and recursive edges
    /// without replaying a body or depending on source order. Each bit changes
    /// at most once. The matching signature copy is refreshed for the retained
    /// formal-boundary metadata installed by [`Self::install_call_requirements`].
    fn close_allocation_metadata(
        &mut self,
        functions: &mut [CheckedFunctionInventory],
    ) -> Result<(), CheckStop> {
        if functions.len() != self.signatures.len() {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        let mut callers = vec![Vec::new(); functions.len()];
        let mut allocates = Vec::with_capacity(functions.len());
        let mut calls = Vec::new();
        for (index, checked) in functions.iter().enumerate() {
            if checked.function.id.0 as usize != index {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            }
            allocates.push(checked.function.allocates);
            calls.clear();
            collect_statement_calls(
                checked.function.id,
                checked.function.body.as_deref().unwrap_or_default(),
                &mut calls,
            );
            for call in &calls {
                let callee = call.callee.0 as usize;
                let Some(callee_callers) = callers.get_mut(callee) else {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                };
                callee_callers.push(index);
            }
        }

        let mut pending = allocates
            .iter()
            .enumerate()
            .filter_map(|(index, allocates)| allocates.then_some(index))
            .collect::<Vec<_>>();
        while let Some(callee) = pending.pop() {
            for caller in &callers[callee] {
                if !allocates[*caller] {
                    allocates[*caller] = true;
                    pending.push(*caller);
                }
            }
        }

        for (index, allocates) in allocates.into_iter().enumerate() {
            functions[index].function.allocates = allocates;
            let signature = self
                .signatures
                .get_mut(index)
                .filter(|signature| signature.id == functions[index].function.id)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            signature.declared_effects.allocates = allocates;
        }
        Ok(())
    }

    fn install_statement_call_requirements(
        &self,
        statements: &mut [CheckedStatement],
        requirements: &[Vec<CheckedRequirement>],
    ) -> Result<(), CheckStop> {
        for statement in statements {
            match statement {
                CheckedStatement::Let { value, .. }
                | CheckedStatement::DestructuringLet { value, .. }
                | CheckedStatement::Evaluate { value, .. }
                | CheckedStatement::DropExpression { value, .. }
                | CheckedStatement::Return { value, .. }
                | CheckedStatement::Give { value, .. } => {
                    self.install_expression_call_requirements(value, requirements)?;
                }
                CheckedStatement::PropagateLet { scrutinee, .. } => {
                    self.install_expression_call_requirements(scrutinee, requirements)?;
                }
                CheckedStatement::Set { target, value, .. } => {
                    match target {
                        CheckedSetTarget::Place(_) => {}
                        CheckedSetTarget::RangeIndex(target) => {
                            for offset in target.offsets_mut() {
                                self.install_expression_call_requirements(offset, requirements)?;
                            }
                        }
                        CheckedSetTarget::Storage(target) => {
                            for offset in target.offsets_mut() {
                                self.install_expression_call_requirements(offset, requirements)?;
                            }
                        }
                    }
                    self.install_expression_call_requirements(value, requirements)?;
                }
                CheckedStatement::Match {
                    scrutinee, arms, ..
                }
                | CheckedStatement::ValueMatchLet {
                    scrutinee, arms, ..
                } => {
                    self.install_expression_call_requirements(scrutinee, requirements)?;
                    for arm in arms {
                        self.install_statement_call_requirements(&mut arm.body, requirements)?;
                    }
                }
                CheckedStatement::Loop { body, .. } => {
                    self.install_statement_call_requirements(body, requirements)?;
                }
                CheckedStatement::CountedRange {
                    lower, upper, body, ..
                } => {
                    self.install_expression_call_requirements(lower, requirements)?;
                    self.install_expression_call_requirements(upper, requirements)?;
                    self.install_statement_call_requirements(body, requirements)?;
                }
                CheckedStatement::Proof(_) => {}
                CheckedStatement::Break { .. } => {}
            }
        }
        Ok(())
    }

    fn install_expression_call_requirements(
        &self,
        expression: &mut CheckedExpression,
        requirements: &[Vec<CheckedRequirement>],
    ) -> Result<(), CheckStop> {
        match expression {
            CheckedExpression::UserCall {
                function,
                formal_effects,
                arguments,
                goal_arguments,
                goal_regions,
                formal_contract,
                requirements: call_requirements,
                ..
            } => {
                for argument in arguments {
                    self.install_expression_call_requirements(argument, requirements)?;
                }
                let signature = self
                    .signatures
                    .get(function.0 as usize)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                if let Some(effects) = formal_effects {
                    effects.allocates = signature.declared_effects.allocates;
                }
                let boundary = match formal_contract {
                    Some(boundary) => boundary.requirements.as_slice(),
                    None => requirements
                        .get(function.0 as usize)
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?,
                };
                *call_requirements = boundary
                    .iter()
                    .map(|boundary| {
                        Ok(CheckedCallRequirement {
                            requires_clause: boundary.clause.clone(),
                            goal: ConcreteGoal::new(self.instantiate_goal_expression(
                                &boundary.template.root,
                                signature,
                                goal_regions,
                                goal_arguments,
                            )?),
                        })
                    })
                    .collect::<Result<Vec<_>, CheckStop>>()?;
            }
            CheckedExpression::IntegerOperation { arguments, .. }
            | CheckedExpression::FloatOperation { arguments, .. }
            | CheckedExpression::BooleanOperation { arguments, .. }
            | CheckedExpression::EnumEquality { arguments, .. }
            | CheckedExpression::ConstructStruct {
                fields: arguments, ..
            }
            | CheckedExpression::ConstructEnum {
                fields: arguments, ..
            } => {
                for argument in arguments {
                    self.install_expression_call_requirements(argument, requirements)?;
                }
            }
            CheckedExpression::NumericConversion { value, .. }
            | CheckedExpression::Reinterpret { value, .. }
            | CheckedExpression::BoxDeref { value, .. }
            | CheckedExpression::ProjectValue { value, .. } => {
                self.install_expression_call_requirements(value, requirements)?;
            }
            CheckedExpression::BoxTake { .. } => {}
            CheckedExpression::ReadStorage { root, .. } => {
                for offset in root.offsets_mut() {
                    self.install_expression_call_requirements(offset, requirements)?;
                }
            }
            CheckedExpression::ArrayIndex { offset, .. }
            | CheckedExpression::BufferIndex { offset, .. } => {
                self.install_expression_call_requirements(offset, requirements)?;
            }
            CheckedExpression::RangeElementMeasure { place, .. }
            | CheckedExpression::RangeIndex { place, .. }
            | CheckedExpression::BorrowRangeIndex { place, .. } => {
                for offset in place.offsets_mut() {
                    self.install_expression_call_requirements(offset, requirements)?;
                }
            }
            // [REF-4] both endpoints are ordinary operands evaluated at the
            // formation, and a storage source carries its own offsets.
            CheckedExpression::RangeOf {
                source, start, end, ..
            } => {
                if let super::model::CheckedRangeSource::Storage(root) = source {
                    for offset in root.offsets_mut() {
                        self.install_expression_call_requirements(offset, requirements)?;
                    }
                }
                self.install_expression_call_requirements(start, requirements)?;
                self.install_expression_call_requirements(end, requirements)?;
            }
            CheckedExpression::Constant(_)
            | CheckedExpression::NamedConstant { .. }
            | CheckedExpression::Binding { .. }
            | CheckedExpression::ArrayMeasure { .. }
            | CheckedExpression::BufferMeasure { .. }
            | CheckedExpression::ContainerMeasure { .. }
            | CheckedExpression::RangeMeasure { .. }
            | CheckedExpression::BorrowAddressed { .. }
            | CheckedExpression::DerefAddressed { .. }
            | CheckedExpression::Project { .. } => {}
        }
        Ok(())
    }

    fn install_source_allocation_bounds(
        &self,
        functions: &mut [CheckedFunction],
    ) -> Result<(), CheckStop> {
        for function in functions {
            let bounds = function
                .entailment
                .obligations
                .iter()
                .filter(|outcome| {
                    outcome.family == super::entailment::ObligationFamily::AllocationFit
                        && outcome.discharged
                })
                .map(|outcome| {
                    let upper = outcome
                        .allocation_length_upper_bound
                        .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
                    Ok((outcome.node_path.clone(), upper))
                })
                .collect::<Result<HashMap<_, _>, SemanticCompilerFailure>>()?;
            if let Some(body) = &mut function.body {
                Self::install_statement_allocation_bounds(body, &bounds)?;
            }
        }
        Ok(())
    }

    fn install_statement_allocation_bounds(
        statements: &mut [CheckedStatement],
        bounds: &HashMap<NodePath, u64>,
    ) -> Result<(), SemanticCompilerFailure> {
        for statement in statements {
            match statement {
                CheckedStatement::Let { value, .. }
                | CheckedStatement::DestructuringLet { value, .. }
                | CheckedStatement::Evaluate { value, .. }
                | CheckedStatement::DropExpression { value, .. }
                | CheckedStatement::Return { value, .. }
                | CheckedStatement::Give { value, .. } => {
                    Self::install_expression_allocation_bounds(value, bounds)?;
                }
                CheckedStatement::PropagateLet { scrutinee, .. } => {
                    Self::install_expression_allocation_bounds(scrutinee, bounds)?;
                }
                CheckedStatement::Set { target, value, .. } => {
                    match target {
                        CheckedSetTarget::Place(_) => {}
                        CheckedSetTarget::RangeIndex(target) => {
                            for offset in target.offsets_mut() {
                                Self::install_expression_allocation_bounds(offset, bounds)?;
                            }
                        }
                        CheckedSetTarget::Storage(target) => {
                            for offset in target.offsets_mut() {
                                Self::install_expression_allocation_bounds(offset, bounds)?;
                            }
                        }
                    }
                    Self::install_expression_allocation_bounds(value, bounds)?;
                }
                CheckedStatement::Match {
                    scrutinee, arms, ..
                }
                | CheckedStatement::ValueMatchLet {
                    scrutinee, arms, ..
                } => {
                    Self::install_expression_allocation_bounds(scrutinee, bounds)?;
                    for arm in arms {
                        Self::install_statement_allocation_bounds(&mut arm.body, bounds)?;
                    }
                }
                CheckedStatement::Loop { body, .. } => {
                    Self::install_statement_allocation_bounds(body, bounds)?;
                }
                CheckedStatement::CountedRange {
                    lower, upper, body, ..
                } => {
                    Self::install_expression_allocation_bounds(lower, bounds)?;
                    Self::install_expression_allocation_bounds(upper, bounds)?;
                    Self::install_statement_allocation_bounds(body, bounds)?;
                }
                CheckedStatement::Proof(_) => {}
                CheckedStatement::Break { .. } => {}
            }
        }
        Ok(())
    }

    fn install_expression_allocation_bounds(
        expression: &mut CheckedExpression,
        bounds: &HashMap<NodePath, u64>,
    ) -> Result<(), SemanticCompilerFailure> {
        match expression {
            CheckedExpression::UserCall {
                call,
                arguments,
                allocation,
                ..
            } => {
                if let Some(allocation) = allocation
                    && let Some(upper) = bounds.get(call).copied()
                {
                    allocation.install_source_length_upper_bound(upper);
                }
                for argument in arguments {
                    Self::install_expression_allocation_bounds(argument, bounds)?;
                }
            }
            CheckedExpression::IntegerOperation { arguments, .. }
            | CheckedExpression::FloatOperation { arguments, .. }
            | CheckedExpression::BooleanOperation { arguments, .. }
            | CheckedExpression::EnumEquality { arguments, .. }
            | CheckedExpression::ConstructStruct {
                fields: arguments, ..
            }
            | CheckedExpression::ConstructEnum {
                fields: arguments, ..
            } => {
                for argument in arguments {
                    Self::install_expression_allocation_bounds(argument, bounds)?;
                }
            }
            CheckedExpression::NumericConversion { value, .. }
            | CheckedExpression::Reinterpret { value, .. }
            | CheckedExpression::BoxDeref { value, .. }
            | CheckedExpression::ProjectValue { value, .. } => {
                Self::install_expression_allocation_bounds(value, bounds)?;
            }
            CheckedExpression::BoxTake { .. } => {}
            CheckedExpression::ReadStorage { root, .. } => {
                for offset in root.offsets_mut() {
                    Self::install_expression_allocation_bounds(offset, bounds)?;
                }
            }
            CheckedExpression::ArrayIndex { offset, .. }
            | CheckedExpression::BufferIndex { offset, .. } => {
                Self::install_expression_allocation_bounds(offset, bounds)?;
            }
            CheckedExpression::RangeElementMeasure { place, .. }
            | CheckedExpression::RangeIndex { place, .. }
            | CheckedExpression::BorrowRangeIndex { place, .. } => {
                for offset in place.offsets_mut() {
                    Self::install_expression_allocation_bounds(offset, bounds)?;
                }
            }
            CheckedExpression::RangeOf {
                source, start, end, ..
            } => {
                if let super::model::CheckedRangeSource::Storage(root) = source {
                    for offset in root.offsets_mut() {
                        Self::install_expression_allocation_bounds(offset, bounds)?;
                    }
                }
                Self::install_expression_allocation_bounds(start, bounds)?;
                Self::install_expression_allocation_bounds(end, bounds)?;
            }
            CheckedExpression::Constant(_)
            | CheckedExpression::NamedConstant { .. }
            | CheckedExpression::Binding { .. }
            | CheckedExpression::ArrayMeasure { .. }
            | CheckedExpression::BufferMeasure { .. }
            | CheckedExpression::ContainerMeasure { .. }
            | CheckedExpression::RangeMeasure { .. }
            | CheckedExpression::BorrowAddressed { .. }
            | CheckedExpression::DerefAddressed { .. }
            | CheckedExpression::Project { .. } => {}
        }
        Ok(())
    }

    fn instantiate_goal_expression(
        &self,
        expression: &GoalExpression,
        signature: &FunctionSignature,
        regions: &[DeclarationId],
        arguments: &[GoalExpression],
    ) -> Result<GoalExpression, CheckStop> {
        match expression {
            GoalExpression::Datum(GoalDatum::Parameter {
                ordinal,
                projections,
                ty,
            }) => {
                let index = usize::try_from(*ordinal)
                    .map_err(|_| SemanticCompilerFailure::CounterOverflow)?;
                let parameter = signature
                    .parameters
                    .get(index)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                let mut image = arguments
                    .get(index)
                    .cloned()
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                let parameter_type =
                    self.instantiate_goal_type(parameter.ty, signature, regions)?;
                if image.ty() != parameter_type {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                }
                let remaining = if parameter.mode == CheckedMode::Own {
                    projections.as_slice()
                } else {
                    let Some((GoalProjection::Deref, remaining)) = projections.split_first() else {
                        return Err(SemanticCompilerFailure::InvalidResolution.into());
                    };
                    remaining
                };
                let final_type = self.instantiate_goal_type(*ty, signature, regions)?;
                for projection in remaining {
                    // [MSR-1, EFF-5] a formal-valued subscript names a value
                    // parameter of the callee, and the caller substitutes its
                    // own actual for it exactly as it substitutes a row's
                    // index positions: the offset the place is identified
                    // over is the value that argument names here.
                    let projection = match projection {
                        GoalProjection::FormalSubscript { ordinal } => GoalProjection::Subscript(
                            Self::goal_argument_offset(arguments.get(*ordinal as usize))?,
                        ),
                        other => *other,
                    };
                    image = image
                        .with_projection(projection, final_type)
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                }
                if image.ty() != final_type {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                }
                Ok(image)
            }
            GoalExpression::Datum(GoalDatum::NamedConst {
                declaration,
                projections,
                ty,
            }) => Ok(GoalExpression::Datum(GoalDatum::NamedConst {
                declaration: *declaration,
                projections: projections.clone(),
                ty: self.instantiate_goal_type(*ty, signature, regions)?,
            })),
            GoalExpression::Datum(GoalDatum::Literal(value)) => Ok(GoalExpression::Datum(
                GoalDatum::Literal(self.instantiate_goal_value(value, signature, regions)?),
            )),
            GoalExpression::Datum(GoalDatum::Place { .. } | GoalDatum::EvaluatedValue { .. }) => {
                Err(SemanticCompilerFailure::InvalidResolution.into())
            }
            GoalExpression::Operation {
                row,
                type_arguments,
                const_arguments,
                result,
                arguments: operands,
            } => Ok(GoalExpression::Operation {
                row: self.instantiate_goal_operation(*row, signature, regions)?,
                type_arguments: type_arguments
                    .iter()
                    .map(|ty| self.instantiate_goal_type(*ty, signature, regions))
                    .collect::<Result<Vec<_>, _>>()?,
                const_arguments: const_arguments
                    .iter()
                    .map(|value| self.instantiate_goal_const(*value, signature))
                    .collect::<Result<Vec<_>, _>>()?,
                result: self.instantiate_goal_type(*result, signature, regions)?,
                arguments: operands
                    .iter()
                    .map(|operand| {
                        self.instantiate_goal_expression(operand, signature, regions, arguments)
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            }),
        }
    }

    /// The offset one actual supplies to a formal-valued subscript [MSR-1].
    ///
    /// [ENT-2] decides a place's identity by its canonical spelling, so an
    /// argument that names a literal, a named or generic const, or a live
    /// binding supplies exactly that, and an argument that names none of
    /// them supplies the unknown offset, which no admitted family separates
    /// -- the conservative reading in both directions [OWN-7].
    fn goal_argument_offset(
        argument: Option<&GoalExpression>,
    ) -> Result<super::places::CapturedValue, CheckStop> {
        use super::places::{CapturedTerm, CapturedValue};
        let unknown = CapturedValue::unknown();
        let Some(GoalExpression::Datum(datum)) = argument else {
            return Ok(unknown);
        };
        Ok(match datum {
            GoalDatum::Literal(CheckedValue::Integer { bits, .. }) => {
                CapturedValue::new(unknown.capture, CapturedTerm::Literal(*bits))
            }
            GoalDatum::Literal(CheckedValue::ConstGeneric { declaration, .. }) => {
                CapturedValue::new(unknown.capture, CapturedTerm::Const(*declaration))
            }
            GoalDatum::Place {
                root, projections, ..
            } if projections.is_empty() => {
                CapturedValue::new(unknown.capture, CapturedTerm::Binding(*root))
            }
            _ => return Ok(unknown),
        }
        .goal_identity())
    }

    fn instantiate_goal_operation(
        &self,
        operation: GoalOperation,
        signature: &FunctionSignature,
        regions: &[DeclarationId],
    ) -> Result<GoalOperation, CheckStop> {
        Ok(match operation {
            GoalOperation::Integer {
                operation,
                operand_type,
            } => GoalOperation::Integer {
                operation,
                operand_type: self.instantiate_goal_type(operand_type, signature, regions)?,
            },
            GoalOperation::Float {
                operation,
                operand_type,
            } => GoalOperation::Float {
                operation,
                operand_type: self.instantiate_goal_type(operand_type, signature, regions)?,
            },
            GoalOperation::NumericConversion {
                mode,
                source,
                destination,
            } => GoalOperation::NumericConversion {
                mode,
                source: self.instantiate_goal_numeric_type(source, signature, regions)?,
                destination: self.instantiate_goal_numeric_type(destination, signature, regions)?,
            },
            GoalOperation::Reinterpret {
                source,
                destination,
            } => GoalOperation::Reinterpret {
                source: self.instantiate_goal_numeric_type(source, signature, regions)?,
                destination: self.instantiate_goal_numeric_type(destination, signature, regions)?,
            },
            GoalOperation::Boolean(operation) => GoalOperation::Boolean(operation),
            GoalOperation::EnumEquality {
                equal,
                operand_type,
            } => GoalOperation::EnumEquality {
                equal,
                operand_type: self.instantiate_goal_type(operand_type, signature, regions)?,
            },
            GoalOperation::ArrayMeasure {
                measure,
                element,
                length,
            } => GoalOperation::ArrayMeasure {
                measure,
                element: self.instantiate_goal_element(element, signature, regions)?,
                length: self.instantiate_goal_const(length, signature)?,
            },
            GoalOperation::ArrayIndex { element, length } => GoalOperation::ArrayIndex {
                element: self.instantiate_goal_element(element, signature, regions)?,
                length: self.instantiate_goal_const(length, signature)?,
            },
            GoalOperation::BufferMeasure { measure, element } => GoalOperation::BufferMeasure {
                measure,
                element: self.instantiate_goal_element(element, signature, regions)?,
            },
            GoalOperation::BufferIndex { element } => GoalOperation::BufferIndex {
                element: self.instantiate_goal_element(element, signature, regions)?,
            },
            GoalOperation::BufferFits {
                element,
                maximum_length: _,
            } => {
                let element = self.instantiate_goal_type(element, signature, regions)?;
                let maximum_length = self
                    .instantiated_layout_ceiling(element)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?
                    .stride
                    .allocation_limit();
                GoalOperation::BufferFits {
                    element,
                    maximum_length,
                }
            }
            GoalOperation::ContainerMeasure {
                measure,
                measured,
                element,
                constant,
            } => GoalOperation::ContainerMeasure {
                measure,
                measured,
                element: element
                    .map(|element| self.instantiate_goal_element(element, signature, regions))
                    .transpose()?,
                constant: constant
                    .map(|constant| self.instantiate_goal_const(constant, signature))
                    .transpose()?,
            },
            GoalOperation::RunIndex {
                measured,
                element,
                constant,
            } => GoalOperation::RunIndex {
                measured,
                element: self.instantiate_goal_element(element, signature, regions)?,
                constant: constant
                    .map(|constant| self.instantiate_goal_const(constant, signature))
                    .transpose()?,
            },
        })
    }

    fn instantiate_goal_numeric_type(
        &self,
        ty: CheckedNumericType,
        signature: &FunctionSignature,
        regions: &[DeclarationId],
    ) -> Result<CheckedNumericType, CheckStop> {
        CheckedNumericType::from_type(self.instantiate_goal_type(ty.ty(), signature, regions)?)
            .ok_or_else(|| SemanticCompilerFailure::InvalidResolution.into())
    }

    fn instantiate_goal_type(
        &self,
        ty: CheckedType,
        signature: &FunctionSignature,
        regions: &[DeclarationId],
    ) -> Result<CheckedType, CheckStop> {
        Ok(match ty {
            CheckedType::Generic(declaration)
            | CheckedType::GenericInt(declaration)
            | CheckedType::GenericFloat(declaration) => signature
                .substitution
                .type_argument(declaration)
                .unwrap_or(ty),
            CheckedType::Array { element, length } => CheckedType::Array {
                element: self.instantiate_goal_element(element, signature, regions)?,
                length: self.instantiate_goal_const(length, signature)?,
            },
            CheckedType::Buffer { element } => CheckedType::Buffer {
                element: self.instantiate_goal_element(element, signature, regions)?,
            },
            CheckedType::Window {
                shape,
                element,
                capacity,
            } => CheckedType::Window {
                shape,
                element: self.instantiate_goal_element(element, signature, regions)?,
                capacity: capacity
                    .map(|capacity| self.instantiate_goal_const(capacity, signature))
                    .transpose()?,
            },
            CheckedType::Nominal(id) => self.instantiate_goal_nominal(id, signature, regions)?,
            CheckedType::Unit
            | CheckedType::Bool
            | CheckedType::Integer(_)
            | CheckedType::Float(_) => ty,
        })
    }

    /// [FN-2, S20] one nominal instance read at a caller with every formal
    /// region substituted, which is the instance the caller's own value has.
    ///
    /// The structural walk is the same one used while checking the call, so
    /// it reaches a source nominal through PRE-1 wrappers and compiler-owned
    /// store nominals as well as a source nominal's own region axis. It keeps
    /// that walk's ordinary lookup-and-defer behavior; it does not assume a
    /// result-only or nested goal type was itself a direct call argument.
    fn instantiate_goal_nominal(
        &self,
        id: NominalId,
        signature: &FunctionSignature,
        regions: &[DeclarationId],
    ) -> Result<CheckedType, CheckStop> {
        if signature.region_parameters.len() != regions.len() {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        let substitution = signature
            .region_parameters
            .iter()
            .copied()
            .zip(regions.iter().copied())
            .filter(|(formal, actual)| formal != actual)
            .collect::<Vec<_>>();
        self.substitute_type_regions(CheckedType::Nominal(id), &substitution)
    }

    /// One run element at a caller's instance [BLK-1].
    ///
    /// Complete slot types use the ordinary recursive type substitution and
    /// are re-interned only after all formal type, const and region arguments
    /// have been instantiated.
    fn instantiate_goal_element(
        &self,
        element: CheckedElement,
        signature: &FunctionSignature,
        regions: &[DeclarationId],
    ) -> Result<CheckedElement, CheckStop> {
        let ty = self.instantiate_goal_type(self.element_type(element)?, signature, regions)?;
        self.intern_element(ty)
    }

    fn instantiate_goal_const(
        &self,
        value: CheckedConst,
        signature: &FunctionSignature,
    ) -> Result<CheckedConst, CheckStop> {
        Ok(match value {
            CheckedConst::Value(_) => value,
            CheckedConst::Parameter(declaration) => signature
                .substitution
                .const_argument(declaration)
                .unwrap_or(value),
            CheckedConst::Derived(id) => {
                let derived = self.derived_const(id)?;
                let left = self.instantiate_goal_const(derived.left, signature)?;
                let right = self.instantiate_goal_const(derived.right, signature)?;
                // The owning instance body was accepted, so the same
                // evaluation already succeeded at its source node; a failure
                // here is a trusted-invariant breach, not a source verdict.
                self.combine_const(derived.operation, left, right)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?
            }
        })
    }

    /// Returns the interned symbolic const operation behind `id`.
    pub(super) fn derived_const(&self, id: DerivedConstId) -> Result<DerivedConst, CheckStop> {
        self.derived_consts
            .borrow()
            .get(id.0 as usize)
            .copied()
            .ok_or_else(|| SemanticCompilerFailure::InvalidResolution.into())
    }

    /// Combines two const operands under one const operation.
    ///
    /// Two concrete operands evaluate immediately in the u64 const-eval
    /// domain, and `None` reports the const-eval overflow policy's rejection
    /// (a result outside the domain or a zero divisor). A symbolic operand
    /// hash-conses the operation instead, so a symbolic const never fails
    /// here and always has one interned identity.
    pub(super) fn combine_const(
        &self,
        operation: super::model::ConstOperation,
        left: CheckedConst,
        right: CheckedConst,
    ) -> Option<CheckedConst> {
        if let (CheckedConst::Value(left), CheckedConst::Value(right)) = (left, right) {
            return evaluate_const_operation(operation, left, right).map(CheckedConst::Value);
        }
        let derived = DerivedConst {
            operation,
            left,
            right,
        };
        let mut table = self.derived_consts.borrow_mut();
        let index = table
            .iter()
            .position(|entry| *entry == derived)
            .unwrap_or_else(|| {
                table.push(derived);
                table.len() - 1
            });
        u32::try_from(index)
            .ok()
            .map(|index| CheckedConst::Derived(DerivedConstId(index)))
    }

    fn instantiate_goal_value(
        &self,
        value: &CheckedValue,
        signature: &FunctionSignature,
        regions: &[DeclarationId],
    ) -> Result<CheckedValue, CheckStop> {
        Ok(match value {
            CheckedValue::Unit => CheckedValue::Unit,
            CheckedValue::Bool(value) => CheckedValue::Bool(*value),
            CheckedValue::Integer { ty, bits } => CheckedValue::Integer {
                ty: *ty,
                bits: *bits,
            },
            CheckedValue::Float { ty, bits } => CheckedValue::Float {
                ty: *ty,
                bits: *bits,
            },
            // [MSR-6] a const generic is fixed at [FN-2] instantiation, so a
            // concrete instance reads a mathematical constant and only the
            // one symbolic instance keeps the declaration-anchored form.
            CheckedValue::ConstGeneric { declaration, ty } => {
                match signature.substitution.const_argument(*declaration) {
                    Some(CheckedConst::Value(value)) => CheckedValue::Integer {
                        ty: *ty,
                        bits: value,
                    },
                    _ => CheckedValue::ConstGeneric {
                        declaration: *declaration,
                        ty: *ty,
                    },
                }
            }
            CheckedValue::NumericIdentity { ty, one } => {
                match self.instantiate_goal_type(*ty, signature, regions)? {
                    CheckedType::Integer(ty) => CheckedValue::Integer {
                        ty,
                        bits: u64::from(*one),
                    },
                    CheckedType::Float(super::model::FloatType::F32) => CheckedValue::Float {
                        ty: super::model::FloatType::F32,
                        bits: if *one { 0x3f80_0000 } else { 0 },
                    },
                    CheckedType::Float(super::model::FloatType::F64) => CheckedValue::Float {
                        ty: super::model::FloatType::F64,
                        bits: if *one { 0x3ff0_0000_0000_0000 } else { 0 },
                    },
                    ty @ (CheckedType::GenericInt(_) | CheckedType::GenericFloat(_)) => {
                        CheckedValue::NumericIdentity { ty, one: *one }
                    }
                    _ => return Err(SemanticCompilerFailure::InvalidResolution.into()),
                }
            }
            CheckedValue::Array { ty, elements } => CheckedValue::Array {
                ty: self.instantiate_goal_type(*ty, signature, regions)?,
                elements: elements
                    .iter()
                    .map(|element| self.instantiate_goal_value(element, signature, regions))
                    .collect::<Result<Vec<_>, _>>()?,
            },
            CheckedValue::Struct { ty, fields } => CheckedValue::Struct {
                ty: self.instantiate_goal_type(*ty, signature, regions)?,
                fields: fields
                    .iter()
                    .map(|field| self.instantiate_goal_value(field, signature, regions))
                    .collect::<Result<Vec<_>, _>>()?,
            },
        })
    }

    fn source_issue_path(
        issue: &SemanticIssue,
    ) -> Result<&crate::NodePath, SemanticCompilerFailure> {
        match &issue.location {
            SemanticLocation::SourceNode(path, _) => Ok(path),
        }
    }
}
