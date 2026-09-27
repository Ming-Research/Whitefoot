//! One ordinary active-specification compilation pipeline.
//!
//! The driver keeps source failures, unsupported compiler capabilities,
//! resource failures, invariant failures, lowering failures, and backend
//! failures distinct while returning owned LLVM assembly to callers.

use core::fmt;
use std::collections::{BTreeMap, BTreeSet};

mod cache;
mod diagnostic;
mod reads;

pub(crate) mod launcher;
/// The repair pairs that pin repairs with the programs they produce.
#[cfg(test)]
mod pinned_repairs;
/// The probe corpus that pins every diagnostic sentence by its rendered text.
#[cfg(test)]
mod pinned_sentences;

use cache::Fields;
pub use cache::{BuildCache, content_digest, running_compiler_identity};
pub(crate) use diagnostic::Place;
use diagnostic::{Anchor, Head, Record};
pub use diagnostic::{DiagnosticFormat, render_driver_failure};

use crate::backend::emitter::emit_llvm_with_layout;
use crate::target::TargetLayout;
use crate::{
    ACTIVE_KERNEL_SPEC_HASH, BackendFailure, CanonicalLimits, CanonicalOutcome,
    CanonicalSyntaxUnit, CheckedProgram, EntryRejection, EntryRequest, FinalizeLimits,
    FinalizeOutcome, LexLimits, LexOutcome, LoweringFailure, ParseLimits, ParseOutcome,
    ResolutionOutcome, ResolvedSyntaxUnit, SemanticOutcome, SourceBundle, SourceInput,
    SourceLimits, TerminalLimits, TerminalOutcome, audit_canonical, check_semantics,
    classify_terminals, finalize, lex, parse, parse_graph, resolve,
};

/// Host-compiler optimization arguments for every Whitefoot executable.
///
/// One definition serves the driver executable and every test that links an
/// emitted module, so no path can silently link an unoptimized binary while
/// another links an optimized one. There is no writer-facing switch: the
/// optimization level cannot change which programs are accepted, discharge a
/// static source obligation, or insert a runtime proof fallback,
/// so no writer decision exists and the default shape is the only shape. The
/// level is provisional and may move once a measurement asks for it.
pub const HOST_OPTIMIZATION_ARGUMENTS: &[&str] = &["-O2"];

/// The host libraries a link of an emitted module names, for the same
/// one-definition reason.
///
/// A Whitefoot module reaches libm without asking for it: the backend lowers a
/// rounding to `roundevenf` and a fused multiply-add to `fma`, and the host
/// optimizer is free to turn ordinary float arithmetic into another of that
/// library's entry points. Darwin serves those from the same library as
/// `write` and needs nothing said; an ELF host keeps them in `libm` and the
/// link fails with an undefined symbol. Naming the library on both hosts is
/// one link path instead of a per-target one, and on Darwin it resolves to a
/// stub that is already linked.
///
/// Every link that builds an executable from an emitted module belongs on this
/// constant rather than on its own library list — the shipped driver's, the
/// backend's linked-executable helper, the program-corpus harness, and the
/// conformance adapter all take it from here. A link that builds only
/// compiler-owned C units, such as the exhaustion floor fixture, reaches no
/// entry point the emitter chose and does not need it. A link that should have
/// named it and did not fails loudly with an undefined symbol on an ELF host;
/// it never changes a verdict or an outcome, which is why the shipped path is
/// the one this constant has to reach.
///
/// Found by running the program corpus on an x86-64 Linux runner in batch
/// 0090: `grayscale_pixels` and `feedback_controller` compile there and did
/// not link, in the shipped driver's own link path.
pub const HOST_LINK_LIBRARIES: &[&str] = &["-lm"];

/// Explicit implementation ceilings for one compiler invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompilerLimits {
    /// Ordered source-envelope limits.
    pub source: SourceLimits,
    /// Lossless lexical limits.
    pub lexer: LexLimits,
    /// Terminal-classification limits.
    pub terminals: TerminalLimits,
    /// Predictive parser limits.
    pub parser: ParseLimits,
    /// Finalized-tree limits.
    pub finalizer: FinalizeLimits,
    /// Canonical-source audit limits.
    pub canonical: CanonicalLimits,
}

impl Default for CompilerLimits {
    fn default() -> Self {
        Self {
            source: SourceLimits {
                max_sources: 1_024,
                max_logical_path_bytes: 4_096,
                max_source_bytes: 16 * 1_024 * 1_024,
                max_total_source_bytes: 64 * 1_024 * 1_024,
                max_binding_bytes: 128 * 1_024 * 1_024,
            },
            lexer: LexLimits {
                max_sources: 1_024,
                max_source_bytes: 16 * 1_024 * 1_024,
                max_total_source_bytes: 64 * 1_024 * 1_024,
                max_token_bytes: 1_024 * 1_024,
                max_tokens: 8 * 1_024 * 1_024,
                max_lexemes: 16 * 1_024 * 1_024,
            },
            terminals: TerminalLimits {
                max_tokens: 8 * 1_024 * 1_024,
            },
            parser: ParseLimits {
                max_work: 256 * 1_024 * 1_024,
                max_tasks: 8 * 1_024 * 1_024,
                max_frames: 65_536,
                max_elements: 16 * 1_024 * 1_024,
            },
            finalizer: FinalizeLimits {
                max_work: 256 * 1_024 * 1_024,
                max_roots: 8 * 1_024 * 1_024,
                max_shape_tasks: 8 * 1_024 * 1_024,
                max_nodes: 8 * 1_024 * 1_024,
                max_child_edges: 8 * 1_024 * 1_024,
                max_terminals: 8 * 1_024 * 1_024,
                max_sources: 1_024,
            },
            canonical: CanonicalLimits {
                max_work: 256 * 1_024 * 1_024,
                max_source_bytes: 16 * 1_024 * 1_024,
                max_total_source_bytes: 64 * 1_024 * 1_024,
                max_gaps: 8 * 1_024 * 1_024,
                max_path_components: 65_536,
            },
        }
    }
}

/// Compiler stage at which one invocation stopped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompilationStage {
    /// PROG-2 source envelope.
    SourceEnvelope,
    /// Module graph formation and entry selection [MOD-1, MOD-9].
    ModuleGraph,
    /// Raw lossless lexing.
    Lexing,
    /// Context-free terminal membership.
    TerminalClassification,
    /// Strong-LL(2) grammar derivation.
    Parsing,
    /// Finalized production topology.
    Finalization,
    /// Exact FORM-2 source audit.
    CanonicalSource,
    /// Declaration and lexical-use resolution.
    Resolution,
    /// Target-independent semantic checking.
    Semantics,
    /// Checked-program to typed IR lowering, including optional loop shapes.
    Lowering,
    /// Selected-target representability and target-domain discharge.
    TargetLayout,
    /// Conservative textual LLVM emission.
    Backend,
}

/// Category of compiler stop, independent of the stage that reported it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompilationFailureKind {
    /// A numbered source-language rule was violated.
    Source,
    /// Valid source requires an unimplemented compiler capability.
    Unsupported,
    /// An explicit implementation ceiling or host storage stopped work.
    Resource,
    /// The caller supplied an invalid compilation envelope or stage identity.
    Invocation,
    /// A trusted compiler invariant failed.
    Compiler,
    /// Checked-program to IR lowering failed internally.
    Lowering,
    /// A statically materialized object is not representable on the selected target.
    TargetLayout,
    /// LLVM emission failed internally.
    Backend,
}

/// One compiler stop: its stage, its category, the numbered rule when it is a
/// source rejection, and the record a reader is shown.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompilationFailure {
    stage: CompilationStage,
    kind: CompilationFailureKind,
    rule_id: Option<&'static str>,
    /// Boxed so a successful compilation's `Result` stays small.
    record: Box<Record>,
}

/// Where a rejection is written: a record's display path and the one-based
/// line and column there, the column counting characters as a rendered
/// record's summary line does.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceLocation {
    path: String,
    line: u64,
    column: u64,
}

impl SourceLocation {
    /// Returns the record's display path.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns the one-based line.
    #[must_use]
    pub const fn line(&self) -> u64 {
        self.line
    }

    /// Returns the one-based column, in characters.
    #[must_use]
    pub const fn column(&self) -> u64 {
        self.column
    }
}

impl fmt::Display for SourceLocation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}:{}", self.path, self.line, self.column)
    }
}

impl CompilationFailure {
    fn source_envelope(failure: crate::SourceBundleError) -> Self {
        use crate::{LogicalPathError, SourceBundleError};

        let kind = match &failure {
            SourceBundleError::LimitExceeded { .. }
            | SourceBundleError::StorageUnavailable { .. }
            | SourceBundleError::ArithmeticOverflow
            | SourceBundleError::LogicalPath(
                LogicalPathError::LengthOverflow | LogicalPathError::StorageUnavailable { .. },
            ) => CompilationFailureKind::Resource,
            SourceBundleError::EmptySourceSequence
            | SourceBundleError::LogicalPath(
                LogicalPathError::Empty
                | LogicalPathError::Absolute
                | LogicalPathError::EmptyComponent
                | LogicalPathError::DotComponent
                | LogicalPathError::InvalidByte { .. },
            )
            | SourceBundleError::DuplicateLogicalPath { .. }
            | SourceBundleError::UnknownModule => CompilationFailureKind::Invocation,
        };
        Self::new(CompilationStage::SourceEnvelope, kind, failure)
    }

    /// A stop whose payload is a compiler-facing stage value with no writer
    /// repair; the record carries that value's `Debug` text.
    fn new(stage: CompilationStage, kind: CompilationFailureKind, detail: impl fmt::Debug) -> Self {
        Self {
            stage,
            kind,
            rule_id: None,
            record: Box::new(Record::opaque(&detail)),
        }
    }

    fn lowering(failure: LoweringFailure) -> Self {
        let (stage, kind) = match failure {
            LoweringFailure::TargetLayout(_) => (
                CompilationStage::TargetLayout,
                CompilationFailureKind::TargetLayout,
            ),
            // An unavailable compiler-owned body is a capability stop after
            // semantic acceptance, never a source verdict.
            LoweringFailure::UnimplementedPreludeRow(_) => (
                CompilationStage::Lowering,
                CompilationFailureKind::Unsupported,
            ),
            LoweringFailure::InvalidCheckedProgram | LoweringFailure::CounterOverflow => {
                (CompilationStage::Lowering, CompilationFailureKind::Lowering)
            }
        };
        Self::new(stage, kind, failure)
    }

    /// One source-language rejection carrying the rule its stage attributed,
    /// or, when the offending bytes belong to a compiler-supplied prelude
    /// declaration, the pipeline defect that is instead.
    ///
    /// Every stage that can reject source already selects exactly one numbered
    /// rule under DIAG-1; this constructor only publishes that selection, so a
    /// caller comparing cited rules sees the same attribution at every stage.
    /// A composition rejection [MOD-8, MOD-9, STOR-8] located at a place
    /// another record already resolved, such as a graph entry's line, or at
    /// none when nothing written names it.
    fn composition(
        rule: &'static str,
        issue: &CompositionIssue,
        bundle: &SourceBundle,
        at: Option<Place>,
    ) -> Self {
        Self {
            stage: CompilationStage::Semantics,
            kind: CompilationFailureKind::Source,
            rule_id: Some(rule),
            record: Box::new(Record::placed(issue, bundle, at)),
        }
    }

    /// The record is located at the coordinate that rule selected.
    fn at_source<Issue: diagnostic::Report + ?Sized>(
        stage: CompilationStage,
        rule: &'static str,
        issue: &Issue,
        bundle: &SourceBundle,
        coordinate: crate::SyntaxCoordinate,
        anchor: Anchor,
    ) -> Self {
        let record = Box::new(Record::located(issue, bundle, coordinate, anchor));
        if bundle
            .file(coordinate.source())
            .is_some_and(|file| file.prelude().is_some())
        {
            Self {
                stage,
                kind: CompilationFailureKind::Compiler,
                rule_id: None,
                record,
            }
        } else {
            Self {
                stage,
                kind: CompilationFailureKind::Source,
                rule_id: Some(rule),
                record,
            }
        }
    }

    /// Returns the stage that did not produce a complete result.
    #[must_use]
    pub const fn stage(&self) -> CompilationStage {
        self.stage
    }

    /// Returns the source/unsupported/resource/invocation/internal category.
    #[must_use]
    pub const fn kind(&self) -> CompilationFailureKind {
        self.kind
    }

    /// Returns the payload fields as the text rendering prints them, one
    /// `label: value` per line: the kind-specific fields of a rejection, or
    /// the `payload` of a stop that carries a compiler-facing stage value.
    #[must_use]
    pub fn detail(&self) -> String {
        self.record.detail_text()
    }

    /// Renders the record in the selected format.
    ///
    /// The text form is lean: a summary line in the `file:line:column:
    /// error[RULE]: Kind` shape -- `whitefootc: compiler failure in Stage:
    /// Kind` for a stop that is not a source rejection -- the marked source
    /// line, then one indented `label: value` line per payload field. The JSON
    /// form is one complete object on one line with the same payload field
    /// names. Both are deterministic for one compiler executable.
    #[must_use]
    pub fn render(&self, format: DiagnosticFormat) -> String {
        let category = format!("{:?}", self.kind);
        let stage = format!("{:?}", self.stage);
        // A source rejection is always in the source stages, so its rule is
        // what a writer acts on; every other stop names its stage, because
        // the stage is where the capability, limit or defect lives.
        let verdict = match (self.kind, self.rule_id) {
            (CompilationFailureKind::Source, Some(rule)) => format!("error[{rule}]"),
            (CompilationFailureKind::Unsupported, _) => {
                format!("unsupported capability in {stage}")
            }
            (CompilationFailureKind::TargetLayout, _) => {
                format!("target layout failure in {stage}")
            }
            (kind, _) => format!(
                "{} failure in {stage}",
                format!("{kind:?}").to_ascii_lowercase()
            ),
        };
        let head = Head {
            verdict,
            category: &category,
            stage: &stage,
            rule: self.rule_id,
        };
        match format {
            DiagnosticFormat::Text => self.record.text(&head),
            DiagnosticFormat::Json => self.record.json(&head),
        }
    }

    /// Returns where a source rejection is written, when it names a written
    /// place: the file, line and column its summary line prints.
    #[must_use]
    pub fn location(&self) -> Option<SourceLocation> {
        self.record.location()
    }

    /// Returns the exact numbered source rule this rejection cites.
    ///
    /// Present for every [`CompilationFailureKind::Source`] stop, at whichever
    /// stage selected it, and absent for every stop that is not a
    /// source-language rejection and therefore cites no language rule
    /// [DIAG-1].
    #[must_use]
    pub const fn rule_id(&self) -> Option<&'static str> {
        self.rule_id
    }
}

/// The default text rendering, [`CompilationFailure::render`] with
/// [`DiagnosticFormat::Text`].
impl fmt::Display for CompilationFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.render(DiagnosticFormat::Text))
    }
}

impl std::error::Error for CompilationFailure {}

/// Compiles one ordered closed source bundle to conservative textual LLVM.
///
/// Calls run sequentially by default; ordinary call outlining is opt-in through
/// [`compile_with_overlap`] and `whitefootc --par`.
pub fn compile(
    inputs: &[SourceInput<'_>],
    limits: CompilerLimits,
) -> Result<String, CompilationFailure> {
    compile_with_overlap(inputs, limits, crate::OverlapLowering::Off)
}

/// Checks one ordered closed source bundle through complete target-independent
/// semantic acceptance and monomorphization.
///
/// This is the source-verdict projection of the same front-end path [`compile`]
/// uses. It stops before lowering, selected-target layout qualification, and
/// executable-caller construction, so a later target or backend failure cannot
/// become a source rejection or erase successful source acceptance [STOR-6].
pub fn check(inputs: &[SourceInput<'_>], limits: CompilerLimits) -> Result<(), CompilationFailure> {
    with_checked_program(inputs, None, limits, |_, _| Ok(()))
}

/// [`compile`] with the [PAR-1 candidate] overlap lowering named explicitly.
///
/// [`crate::OverlapLowering::Off`] emits the module a compiler without this
/// path emits; [`crate::OverlapLowering::On`] hands every eligible group the
/// lowering can carry to a worker lane. The judgment runs either way — it is
/// pure, it changes no accepted program, and its ledger is identical — so this
/// selects an emitted lowering and nothing else.
pub fn compile_with_overlap(
    inputs: &[SourceInput<'_>],
    limits: CompilerLimits,
    overlap: crate::OverlapLowering,
) -> Result<String, CompilationFailure> {
    compile_reporting(inputs, limits, overlap).map(|reported| reported.module)
}

/// [`check`] reusing and recording proof receipts in `cache` [MOD-8]: a
/// function whose analysis would read exactly what a recorded accepted
/// analysis read is not analyzed again. The verdict is the one [`check`]
/// reaches.
///
/// # Errors
///
/// Returns the check's failure, which no cache record ever stands in for.
pub fn check_with_cache(
    inputs: &[SourceInput<'_>],
    limits: CompilerLimits,
    cache: &BuildCache,
) -> Result<(), CompilationFailure> {
    with_checked_program_using(inputs, None, limits, Some(cache), |_, _| Ok(()))
}

/// [`compile_with_overlap`] reusing and recording proof receipts in `cache`
/// [MOD-8]. An overlap lowering reads the permission table, which a receipt
/// does not retain, so it analyzes every function afresh.
///
/// # Errors
///
/// Returns the compilation's failure, which no cache record ever stands in
/// for.
pub fn compile_with_cache(
    inputs: &[SourceInput<'_>],
    limits: CompilerLimits,
    overlap: crate::OverlapLowering,
    cache: &BuildCache,
) -> Result<String, CompilationFailure> {
    compile_selected(
        inputs,
        None,
        limits,
        overlap,
        &Selection {
            module: crate::ModuleId::BUNDLE_ROOT,
            name: "main",
            no_heap: false,
            public: false,
            written: None,
        },
        receipts_for(overlap, Some(cache)),
    )
    .map(|reported| reported.module)
}

/// The proof receipts a compilation may use: none for an overlap lowering,
/// whose permission table a receipt does not retain [MOD-8].
fn receipts_for(
    overlap: crate::OverlapLowering,
    cache: Option<&BuildCache>,
) -> Option<&BuildCache> {
    cache.filter(|_| overlap == crate::OverlapLowering::Off)
}

/// [`compile_with_overlap`] plus the non-normative permission ledger for the
/// same compilation.
///
/// The ledger reports, one line per analyzed sibling-call site, whether the
/// permission judgment allows overlapping the two statements and whether a
/// permitted overlap is actualizable. It is developer output on the caller's
/// own channel: it participates in no mandatory record, changes no accepted
/// program, and selects no lowering. Permission verdicts are independent of
/// the actualization policy; additional actualization lines describe that
/// policy's choices, including omitted scalar-leaf offers. The compiler's
/// `--par-ledger` switch is its caller outside tests.
pub fn compile_with_permission_ledger(
    inputs: &[SourceInput<'_>],
    limits: CompilerLimits,
    overlap: crate::OverlapLowering,
) -> Result<(String, Vec<String>), CompilationFailure> {
    compile_reporting(inputs, limits, overlap).map(|reported| (reported.module, reported.ledger))
}

/// What one module program invocation runs [MOD-9, PROG-3].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModuleEntry<'a> {
    /// A named entry of the graph, with its requirements.
    Named(&'a str),
    /// Any ordinary function of a registered module, run as an unnamed entry
    /// with no requirement: `module` is `pkg` or `pkg::a::b`.
    Function {
        /// The module's qualified name.
        module: &'a str,
        /// The function's name.
        function: &'a str,
    },
}

/// Forms the module graph one `modules.wfg` record writes [MOD-1].
///
/// The record passes the ordinary syntax stages under the `graph_file` start
/// and then graph formation; any rejection cites its rule at the record.
pub fn form_module_graph(
    graph: SourceInput<'_>,
    limits: CompilerLimits,
) -> Result<crate::ModuleGraph, CompilationFailure> {
    form_graph_record(
        graph,
        limits,
        crate::Package::Program,
        Some(&crate::ModuleGraph::library()),
    )
}

/// Forms the graph one graph record writes for `package`, its `std`
/// dependencies naming modules of `library` [MOD-1, MOD-10].
fn form_graph_record(
    graph: SourceInput<'_>,
    limits: CompilerLimits,
    package: crate::Package,
    library: Option<&crate::ModuleGraph>,
) -> Result<crate::ModuleGraph, CompilationFailure> {
    let bundle = SourceBundle::with_limits(&[graph], limits.source)
        .map_err(CompilationFailure::source_envelope)?;
    let canonical = canonical_syntax(&bundle, limits, true)?;
    match crate::graph::form_graph(&canonical, package, library) {
        Ok(Ok(mut graph)) => {
            graph.locate_entries(|coordinate| Place::resolve(&bundle, coordinate, Anchor::Start));
            Ok(graph)
        }
        Ok(Err(issue)) => Err(CompilationFailure::at_source(
            CompilationStage::ModuleGraph,
            issue.rule_id(),
            &issue,
            &bundle,
            issue.coordinate(),
            Anchor::Start,
        )),
        Err(failure) => Err(CompilationFailure::new(
            CompilationStage::ModuleGraph,
            CompilationFailureKind::Compiler,
            failure,
        )),
    }
}

/// `inputs` followed by the records of `graph`'s standard library modules,
/// which the compiler carries [MOD-10]. A check reads a library record only
/// when its module is in the check's closure, as it reads every record. An
/// entry point that another one calls receives the records already, so a
/// record `inputs` holds is not added again.
fn with_library_records<'input>(
    graph: &crate::ModuleGraph,
    inputs: &[SourceInput<'input>],
) -> Vec<SourceInput<'input>> {
    let mut all = inputs.to_vec();
    all.extend(
        crate::library::records(graph.modules(), |_| true)
            .into_iter()
            .filter(|record| {
                !inputs
                    .iter()
                    .any(|input| input.logical_path() == record.logical_path())
            }),
    );
    all
}

/// Checks every registered module of a module program against its
/// dependencies' interfaces, in row order, without selecting an entry, and
/// returns the first rejected module's failure [MOD-8].
///
/// `inputs` are the modules' interface and implementation records, each
/// placed with [`SourceInput::in_module`].
pub fn check_module_program(
    graph: &crate::ModuleGraph,
    inputs: &[SourceInput<'_>],
    limits: CompilerLimits,
) -> Result<(), CompilationFailure> {
    let inputs = &with_library_records(graph, inputs);
    let modules = graph.program_modules().collect::<Vec<_>>();
    require_module_verdicts(graph, inputs, &modules, limits, None)
}

/// Checks one module against the interfaces it may name [MOD-8]: the
/// module's own interface and implementation records, and the interface
/// records of every module in its dependency closure. No other module's
/// implementation record is read, so the verdict holds while a dependency's
/// implementation is absent, incomplete or failing, and an edit to another
/// module's implementation cannot change it. With `interface_only`, the
/// module's own implementation records are left out as well, which checks
/// the interface an architect writes before any body exists.
pub fn check_module(
    graph: &crate::ModuleGraph,
    inputs: &[SourceInput<'_>],
    module: &str,
    interface_only: bool,
    limits: CompilerLimits,
) -> Result<(), CompilationFailure> {
    let inputs = &with_library_records(graph, inputs);
    let target = registered_module(graph, module)?;
    let selected = module_check_inputs(graph, inputs, target, interface_only);
    with_checked_program(&selected, Some(graph.modules()), limits, |_, _| Ok(()))
}

/// [MOD-6, MOD-8] renders one module's resolved public interface after
/// checking its interface records against its dependencies' interfaces, the
/// same input the interface check reads. Equal renderings of two revisions
/// mean equal public names, signatures, contracts and complete reached
/// representations, so comparing them is a conservative interface
/// comparison.
///
/// # Errors
///
/// Returns the interface check's failure, or an unregistered module.
pub fn render_module_interface(
    graph: &crate::ModuleGraph,
    inputs: &[SourceInput<'_>],
    module: &str,
    limits: CompilerLimits,
) -> Result<String, CompilationFailure> {
    let inputs = &with_library_records(graph, inputs);
    let target = registered_module(graph, module)?;
    let selected = module_check_inputs(graph, inputs, target, true);
    with_checked_program(&selected, Some(graph.modules()), limits, |_, resolved| {
        resolved.render_interface(target).map_err(|failure| {
            CompilationFailure::new(
                CompilationStage::Resolution,
                CompilationFailureKind::Compiler,
                failure,
            )
        })
    })
}

/// [MOD-8] the records one module's check reads: the module's own records,
/// or its interface alone, and the interface records of its dependency
/// closure. No other module's implementation record enters.
fn module_check_inputs<'input>(
    graph: &crate::ModuleGraph,
    inputs: &[SourceInput<'input>],
    target: crate::ModuleId,
    interface_only: bool,
) -> Vec<SourceInput<'input>> {
    let closure = graph.dependency_closure(target);
    inputs
        .iter()
        .copied()
        .filter(|input| {
            if input.module() == target {
                !interface_only || input.role() == crate::SourceRole::Interface
            } else {
                input.role() == crate::SourceRole::Interface && closure.contains(&input.module())
            }
        })
        .collect()
}

/// The module a qualified name registers, or the invocation failure naming it.
fn registered_module(
    graph: &crate::ModuleGraph,
    module: &str,
) -> Result<crate::ModuleId, CompilationFailure> {
    graph.module_named(module).ok_or_else(|| {
        CompilationFailure::new(
            CompilationStage::ModuleGraph,
            CompilationFailureKind::Invocation,
            format!("the graph registers no module `{module}`"),
        )
    })
}

/// One module's source verdict or one entry's composition verdict, as the
/// module check, the composition check and the impact report publish it
/// [MOD-8].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckVerdict {
    subject: String,
    outcome: CheckOutcome,
    reused: bool,
}

/// What one check concluded.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CheckOutcome {
    /// Every judgment the check makes holds. For a module, the named
    /// interface declarations have no definition among its implementation
    /// records yet, which blocks composition and nothing else [MOD-8]; a
    /// composition has none.
    Accepted {
        /// The pending interface function declarations, in source order.
        pending: Vec<String>,
    },
    /// The check's first source rejection, and the failing tasks an impact
    /// report lists, the first of which is that rejection [MOD-8].
    Rejected {
        /// The numbered rule the rejection cites.
        rule: Option<String>,
        /// The rejection as a fresh check renders it.
        failure: String,
        /// One task per failing definition, body or declaration.
        tasks: Vec<ImpactTask>,
        /// Whether the tasks name every failure of the check; a list that
        /// stopped at a failure it could not step past is not known to.
        complete: bool,
    },
}

/// What an impact report asks of one failure [MOD-8].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskKind {
    /// A definition that no longer repeats its interface declaration
    /// [MOD-7].
    Definition,
    /// A function body, such as one whose call a changed declaration no
    /// longer admits.
    Body,
    /// A declaration: an interface declaration, type or constant, of the
    /// module or of an interface it reads.
    Declaration,
}

impl TaskKind {
    /// The kind's name in the report.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Definition => "definition",
            Self::Body => "body",
            Self::Declaration => "declaration",
        }
    }

    fn named(name: &[u8]) -> Option<Self> {
        match name {
            b"definition" => Some(Self::Definition),
            b"body" => Some(Self::Body),
            b"declaration" => Some(Self::Declaration),
            _ => None,
        }
    }
}

/// One failing task of an impact report [MOD-8].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImpactTask {
    kind: TaskKind,
    function: Option<String>,
    rule: Option<String>,
    location: Option<SourceLocation>,
    failure: String,
}

impl ImpactTask {
    /// Returns what the task asks for.
    #[must_use]
    pub const fn kind(&self) -> TaskKind {
        self.kind
    }

    /// Returns the function whose definition or body fails, when one does.
    #[must_use]
    pub fn function(&self) -> Option<&str> {
        self.function.as_deref()
    }

    /// Returns the numbered rule the failure cites.
    #[must_use]
    pub fn rule(&self) -> Option<&str> {
        self.rule.as_deref()
    }

    /// Returns where the failure is written.
    #[must_use]
    pub const fn location(&self) -> Option<&SourceLocation> {
        self.location.as_ref()
    }

    /// Returns the failure as a check renders it.
    #[must_use]
    pub fn failure(&self) -> &str {
        &self.failure
    }
}

impl CheckVerdict {
    /// What was checked: a module's qualified name, `pkg` or `pkg::a::b`, or
    /// an entry's name.
    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }

    /// What the check concluded.
    #[must_use]
    pub const fn outcome(&self) -> &CheckOutcome {
        &self.outcome
    }

    /// Whether a verdict recorded for exactly these inputs was reused rather
    /// than recomputed.
    #[must_use]
    pub const fn reused(&self) -> bool {
        self.reused
    }
}

/// The cache family of module verdicts.
const MODULE_VERDICTS: &str = "module-verdicts";

/// [MOD-8] checks one registered module against its dependencies'
/// interfaces alone, or its interface records alone, and reports its verdict.
///
/// With a cache, a verdict recorded for exactly the same inputs is reused:
/// the module's selected records and their display names, the interface
/// records of its dependency closure, the closure's edges, the registered
/// modules one path component below a closure module, and the compiler
/// itself. An edit to another module's implementation records changes none
/// of these, so it never recomputes this verdict, and an edit to an
/// interface this module reads always does. A rejection is reused only while
/// the order in which the check reads its records and the whole set of
/// registered modules are unchanged as well, since they select which of
/// several defects it reports.
///
/// # Errors
///
/// Returns a failure that is not a source rejection: an unregistered module,
/// an exhausted limit or a compiler invariant. Such a failure is never
/// recorded.
pub fn module_verdict(
    graph: &crate::ModuleGraph,
    inputs: &[SourceInput<'_>],
    module: &str,
    interface_only: bool,
    limits: CompilerLimits,
    cache: Option<&BuildCache>,
) -> Result<CheckVerdict, CompilationFailure> {
    let inputs = &with_library_records(graph, inputs);
    let target = registered_module(graph, module)?;
    let check = ModuleCheck::new(graph, inputs, target, interface_only);
    if let Some(cache) = cache
        && let Some(pending) = validated_acceptance(graph, inputs, &check, limits, cache)?
    {
        return Ok(CheckVerdict {
            subject: module.to_owned(),
            outcome: CheckOutcome::Accepted { pending },
            reused: true,
        });
    }
    let mut judged = Vec::new();
    let verdict = recorded_verdict(
        module,
        cache,
        MODULE_VERDICTS,
        (&check.material, &check.reading),
        || match check.run(graph, limits, cache) {
            Ok(checked) => {
                judged = checked;
                let pending = judged
                    .first()
                    .map(|judged| judged.pending.clone())
                    .unwrap_or_default();
                Ok(CheckOutcome::Accepted { pending })
            }
            Err(failure) if failure.kind() == CompilationFailureKind::Source => {
                module_rejection(graph, &check, &failure, limits, cache)
            }
            Err(failure) => Err(failure),
        },
    )?;
    if let Some(cache) = cache
        && !judged.is_empty()
    {
        record_acceptances(graph, inputs, &check, &judged, limits, cache);
    }
    Ok(verdict)
}

/// [MOD-8] a rejected module's impact report: one task per failing
/// definition, body or declaration, found by checking the module again with
/// each failing function set aside. A definition whose function the
/// interface declares is removed, leaving that declaration pending as a
/// missing definition's is; a private function's header moves into the
/// interface as a pending private declaration. Callers keep using the
/// written boundary, so the next check reports the next failure. A failure
/// in no function, or a private header the interface cannot state, ends the
/// list, which is then not known to be complete.
fn module_rejection(
    graph: &crate::ModuleGraph,
    check: &ModuleCheck<'_>,
    first: &CompilationFailure,
    limits: CompilerLimits,
    cache: Option<&BuildCache>,
) -> Result<CheckOutcome, CompilationFailure> {
    let mut texts = check
        .selected
        .iter()
        .map(|input| input.bytes().to_vec())
        .collect::<Vec<_>>();
    let interface = check.selected.iter().position(|input| {
        input.module() == check.target && input.role() == crate::SourceRole::Interface
    });
    // Each current line's line in the record as written, zero for a line
    // this report added: a later failure is reported where the writer sees
    // it, and one in an added declaration is the report's, not the module's.
    let mut origins = texts
        .iter()
        .map(|text| (1..=line_count(text)).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    let mut tasks = Vec::new();
    let mut failure = first.clone();
    let complete = loop {
        let item = failing_item(&failure, &check.selected, &texts);
        let task = impact_task(&failure, &check.selected, item.as_ref());
        tasks.push(as_written(task, &check.selected, &origins));
        let Some((record, range, function)) = item.filter(|(record, _, function)| {
            function.is_some()
                && check.selected[*record].module() == check.target
                && check.selected[*record].role() == crate::SourceRole::Implementation
        }) else {
            break false;
        };
        let (Some(function), Some(interface)) = (function, interface) else {
            break false;
        };
        if !declares_function(&texts[interface], &function) {
            let Some(declaration) =
                pending_declaration(&String::from_utf8_lossy(&texts[record][range.clone()]))
            else {
                break false;
            };
            let text = &mut texts[interface];
            let mut added = line_count(declaration.as_bytes());
            if !text.is_empty() {
                text.push(b'\n');
                added += 1;
            }
            text.extend_from_slice(declaration.as_bytes());
            origins[interface].extend((0..added).map(|_| 0));
        }
        let removal = item_removal(&texts[record], range);
        let first_line =
            usize::try_from(line_count(&texts[record][..removal.start])).unwrap_or(usize::MAX);
        let lines =
            usize::try_from(line_count(&texts[record][removal.clone()])).unwrap_or(usize::MAX);
        let lines = first_line..first_line.saturating_add(lines).min(origins[record].len());
        origins[record].drain(lines);
        texts[record].drain(removal);
        let rechecked = check
            .selected
            .iter()
            .zip(&texts)
            .map(|(input, text)| input.with_bytes(text))
            .collect::<Vec<_>>();
        match with_checked_program_using(
            &rechecked,
            Some(graph.modules()),
            limits,
            cache,
            |_, _| Ok(()),
        ) {
            Ok(()) => break true,
            Err(next) if next.kind() == CompilationFailureKind::Source => {
                if written_line(next.location().as_ref(), &check.selected, &origins) == Some(0) {
                    break false;
                }
                failure = next;
            }
            Err(next) => return Err(next),
        }
    };
    Ok(CheckOutcome::Rejected {
        rule: first.rule_id().map(str::to_owned),
        failure: first.to_string(),
        tasks,
        complete,
    })
}

/// The record and top-level item a failure is written in, and the item's
/// function when it is one: in canonical form a top-level item is a run of
/// nonempty lines between empty ones [FORM-2].
fn failing_item(
    failure: &CompilationFailure,
    records: &[SourceInput<'_>],
    texts: &[Vec<u8>],
) -> Option<(usize, core::ops::Range<usize>, Option<String>)> {
    let location = failure.location()?;
    let record = records
        .iter()
        .position(|input| input.display_path() == location.path())?;
    let text = texts.get(record)?;
    let mut starts = vec![0_usize];
    starts.extend(
        text.iter()
            .enumerate()
            .filter(|(_, byte)| **byte == b'\n')
            .map(|(index, _)| index + 1),
    );
    let line = usize::try_from(location.line()).ok()?.checked_sub(1)?;
    let empty = |line: usize| {
        let (Some(&start), end) = (starts.get(line), starts.get(line + 1)) else {
            return true;
        };
        end.map_or(start >= text.len(), |end| end - start <= 1)
    };
    if empty(line) {
        return None;
    }
    let mut first = line;
    while first > 0 && !empty(first - 1) {
        first -= 1;
    }
    let mut last = line;
    while !empty(last + 1) {
        last += 1;
    }
    let start = starts[first];
    let end = starts.get(last + 1).copied().unwrap_or(text.len());
    let header = String::from_utf8_lossy(&text[start..end]);
    let header = header.lines().next().unwrap_or_default();
    let function = header
        .strip_prefix("public ")
        .unwrap_or(header)
        .strip_prefix("fn ")
        .and_then(|rest| rest.split(['(', '<']).next())
        .map(str::to_owned);
    Some((record, start..end, function))
}

/// A location's line in its record as written, from the current line's
/// origin; zero for a line an impact report added.
fn written_line(
    location: Option<&SourceLocation>,
    records: &[SourceInput<'_>],
    origins: &[Vec<u64>],
) -> Option<u64> {
    let location = location?;
    let record = records
        .iter()
        .position(|input| input.display_path() == location.path())?;
    let line = usize::try_from(location.line()).ok()?.checked_sub(1)?;
    origins.get(record)?.get(line).copied()
}

/// A task located where the writer sees it: a check of a record with earlier
/// failing items set aside reports lines of that shorter text.
fn as_written(
    mut task: ImpactTask,
    records: &[SourceInput<'_>],
    origins: &[Vec<u64>],
) -> ImpactTask {
    if let Some(line) = written_line(task.location.as_ref(), records, origins)
        && let Some(location) = &mut task.location
        && line != location.line
    {
        // The rendered record's summary line begins with the location.
        let current = format!("{location}: ");
        location.line = line;
        if let Some(rest) = task.failure.strip_prefix(&current) {
            task.failure = format!("{location}: {rest}");
        }
    }
    task
}

/// The task one failure asks for.
fn impact_task(
    failure: &CompilationFailure,
    records: &[SourceInput<'_>],
    item: Option<&(usize, core::ops::Range<usize>, Option<String>)>,
) -> ImpactTask {
    let function = item.and_then(|(_, _, function)| function.clone());
    let implementation = item
        .is_some_and(|(record, _, _)| records[*record].role() == crate::SourceRole::Implementation);
    let kind = match (implementation, &function) {
        (true, Some(_)) if failure.rule_id() == Some("MOD-7") => TaskKind::Definition,
        (true, Some(_)) => TaskKind::Body,
        _ => TaskKind::Declaration,
    };
    ImpactTask {
        kind,
        function,
        rule: failure.rule_id().map(str::to_owned),
        location: failure.location(),
        failure: failure.to_string(),
    }
}

/// Whether an interface record declares a function of this name.
fn declares_function(interface: &[u8], function: &str) -> bool {
    String::from_utf8_lossy(interface).lines().any(|line| {
        line.strip_prefix("public ")
            .unwrap_or(line)
            .strip_prefix("fn ")
            .and_then(|rest| rest.strip_prefix(function))
            .is_some_and(|rest| rest.starts_with(['(', '<']))
    })
}

/// A private definition's pending interface declaration: its header and
/// contract, which end at its body, followed by a `doc` entry, in canonical
/// form [FORM-2, MOD-7].
fn pending_declaration(definition: &str) -> Option<String> {
    const DOC: &str = "doc \"Set aside by the impact report while its body fails.\";";
    let mut lines = definition.lines();
    let first = lines.next()?;
    if first.strip_suffix(" contract {").is_some() {
        let mut declaration = format!("{first}\n");
        for line in lines {
            if line == "} {" {
                declaration.push_str(&format!("}} {DOC}\n"));
                return Some(declaration);
            }
            declaration.push_str(line);
            declaration.push('\n');
        }
        return None;
    }
    let header = first.strip_suffix(" {")?;
    Some(format!("{header} {DOC}\n"))
}

/// The number of lines a canonical text holds.
fn line_count(text: &[u8]) -> u64 {
    text.iter().filter(|byte| **byte == b'\n').count() as u64
}

/// The bytes that remove one top-level item with one of the empty lines that
/// separate it from its neighbours, keeping the record canonical: a record
/// left without items is one LF [FORM-2].
fn item_removal(text: &[u8], item: core::ops::Range<usize>) -> core::ops::Range<usize> {
    if item.end < text.len() {
        item.start..item.end + 1
    } else if item.start > 0 {
        item.start - 1..item.end
    } else {
        item.start..item.end.saturating_sub(1)
    }
}

/// One module's check against the interfaces it may name [MOD-8]: the
/// records it reads and the key material of its verdict.
struct ModuleCheck<'input> {
    target: crate::ModuleId,
    selected: Vec<SourceInput<'input>>,
    /// What the verdict depends on, whatever it concludes: every record it
    /// reads, exactly.
    material: Vec<u8>,
    /// What an acceptance depends on beyond its read set: the module's own
    /// records and the graph facts its check reads [MOD-8].
    own: Vec<u8>,
    /// What a rejection depends on beyond `material`.
    reading: Vec<u8>,
}

impl<'input> ModuleCheck<'input> {
    fn new(
        graph: &crate::ModuleGraph,
        inputs: &[SourceInput<'input>],
        target: crate::ModuleId,
        interface_only: bool,
    ) -> Self {
        let selected = module_check_inputs(graph, inputs, target, interface_only);
        let mut modules = graph.dependency_closure(target);
        modules.push(target);
        modules.sort();
        // What every record of this check shares: the check's kind, its
        // module and the graph facts it reads.
        let mut facts = if interface_only {
            b"interface-only\n".to_vec()
        } else {
            b"complete\n".to_vec()
        };
        push_module_line(&mut facts, "module", graph, target);
        push_graph_facts(&mut facts, graph, &modules);
        let mut material = b"module-verdict 2\n".to_vec();
        material.extend_from_slice(&facts);
        push_records(&mut material, graph, &selected, RecordOrder::Sorted);
        let mut own = b"module-acceptance 1\n".to_vec();
        own.extend_from_slice(&facts);
        let own_records = selected
            .iter()
            .copied()
            .filter(|input| input.module() == target)
            .collect::<Vec<_>>();
        push_records(&mut own, graph, &own_records, RecordOrder::Sorted);
        let reading = rejection_reading(graph, &selected);
        Self {
            target,
            selected,
            material,
            own,
            reading,
        }
    }

    /// The pending declarations of an accepted module and what its check
    /// read of other modules, then the same for the interface of every
    /// module of its dependency closure, whose judgments the accepted check
    /// also made; or the check's failure.
    fn run(
        &self,
        graph: &crate::ModuleGraph,
        limits: CompilerLimits,
        cache: Option<&BuildCache>,
    ) -> Result<Vec<Judged>, CompilationFailure> {
        with_checked_program_using(
            &self.selected,
            Some(graph.modules()),
            limits,
            cache,
            |_, resolved| {
                Ok(std::iter::once(self.target)
                    .chain(graph.dependency_closure(self.target))
                    .map(|module| Judged {
                        module,
                        pending: pending_declarations(resolved, module),
                        read: reads::read_declarations(resolved, module),
                    })
                    .collect())
            },
        )
    }

    /// Records an acceptance under the module's own records and graph facts,
    /// with what the check read of other modules: the digest of every
    /// closure module's interface record, and the digest of every
    /// declaration it reached [MOD-8]. Without a read set, or when a
    /// reached declaration has no digest, only the exact record stands.
    fn record_acceptance(
        &self,
        graph: &crate::ModuleGraph,
        pending: &[String],
        read: Option<&ReadSet>,
        limits: CompilerLimits,
        cache: &BuildCache,
    ) {
        let Some(read) = read else {
            return;
        };
        let mut interfaces = Vec::new();
        for module in graph.dependency_closure(self.target) {
            let Some(bytes) = interface_record(&self.selected, module) else {
                return;
            };
            interfaces.push((module_name(graph, module), content_digest(bytes)));
        }
        let mut digests = BTreeMap::new();
        let mut reads = Vec::new();
        for (module, item) in read {
            let parsed = digests.entry(*module).or_insert_with(|| {
                interface_record(&self.selected, *module)
                    .and_then(|bytes| reads::declaration_digests(bytes, limits))
            });
            let Some(digest) = parsed.as_ref().and_then(|parsed| parsed.get(item)) else {
                return;
            };
            reads.push((module_name(graph, *module), item.clone(), *digest));
        }
        let acceptance = Acceptance {
            pending: pending.to_vec(),
            interfaces,
            reads,
        };
        // A failed publication costs only a later recomputation.
        let _ = cache.store(MODULE_VERDICTS, &self.own, &acceptance.encode());
    }
}

/// What one check read of other modules: each reached declaration by its
/// module, role and spelling [MOD-8].
type ReadSet = BTreeSet<(crate::ModuleId, reads::ItemName)>;

/// One module whose records an accepted check judged: its pending
/// declarations and what its records read of other modules.
struct Judged {
    module: crate::ModuleId,
    pending: Vec<String>,
    read: Option<ReadSet>,
}

/// [MOD-8] records the acceptances one accepted check establishes: the
/// checked module's own, and the interface verdict of every module of its
/// closure, since the check judged each of those interfaces against the
/// interfaces of that module's own closure, exactly as its interface check
/// does. A later check whose dependency changed then finds the interface
/// verdicts it needs recorded instead of checking every interface again.
fn record_acceptances(
    graph: &crate::ModuleGraph,
    inputs: &[SourceInput<'_>],
    check: &ModuleCheck<'_>,
    judged: &[Judged],
    limits: CompilerLimits,
    cache: &BuildCache,
) {
    for (index, judged) in judged.iter().enumerate() {
        let owner;
        let check = if index == 0 {
            check
        } else {
            owner = ModuleCheck::new(graph, inputs, judged.module, true);
            &owner
        };
        check.record_acceptance(graph, &judged.pending, judged.read.as_ref(), limits, cache);
    }
}

/// The interface record of `module` among `inputs`.
fn interface_record<'input>(
    inputs: &[SourceInput<'input>],
    module: crate::ModuleId,
) -> Option<&'input [u8]> {
    inputs
        .iter()
        .find(|input| input.module() == module && input.role() == crate::SourceRole::Interface)
        .map(|input| input.bytes())
}

/// A registered module's qualified name.
fn module_name(graph: &crate::ModuleGraph, module: crate::ModuleId) -> String {
    graph
        .modules()
        .get(module.index())
        .map_or_else(String::new, crate::ModuleRecord::qualified_name)
}

/// A module acceptance recorded with its read set [MOD-8].
struct Acceptance {
    pending: Vec<String>,
    /// Each closure module's interface record digest, when it was read.
    interfaces: Vec<(String, [u8; 32])>,
    /// Each reached declaration: its module, role and spelling, and digest.
    reads: Vec<(String, reads::ItemName, [u8; 32])>,
}

impl Acceptance {
    fn encode(&self) -> Vec<u8> {
        let mut fields = Fields::default();
        fields.push(b"acceptance 1");
        fields.push(self.pending.len().to_string().as_bytes());
        for name in &self.pending {
            fields.push(name.as_bytes());
        }
        fields.push(self.interfaces.len().to_string().as_bytes());
        for (module, digest) in &self.interfaces {
            fields.push(module.as_bytes()).push(digest);
        }
        fields.push(self.reads.len().to_string().as_bytes());
        for (module, (role, spelling), digest) in &self.reads {
            fields
                .push(module.as_bytes())
                .push(role.as_bytes())
                .push(spelling.as_bytes())
                .push(digest);
        }
        fields.into_bytes()
    }

    fn decode(payload: &[u8]) -> Option<Self> {
        let fields = Fields::parse(payload)?;
        let mut fields = fields.into_iter();
        if fields.next()? != b"acceptance 1" {
            return None;
        }
        let count = |fields: &mut std::vec::IntoIter<&[u8]>| -> Option<usize> {
            std::str::from_utf8(fields.next()?).ok()?.parse().ok()
        };
        let text = |field: &[u8]| std::str::from_utf8(field).ok().map(str::to_owned);
        let digest = |field: &[u8]| <[u8; 32]>::try_from(field).ok();
        let mut pending = Vec::new();
        for _ in 0..count(&mut fields)? {
            pending.push(text(fields.next()?)?);
        }
        let mut interfaces = Vec::new();
        for _ in 0..count(&mut fields)? {
            interfaces.push((text(fields.next()?)?, digest(fields.next()?)?));
        }
        let mut reads = Vec::new();
        for _ in 0..count(&mut fields)? {
            let module = text(fields.next()?)?;
            let role = text(fields.next()?)?;
            let spelling = text(fields.next()?)?;
            reads.push((module, (role, spelling), digest(fields.next()?)?));
        }
        fields.next().is_none().then_some(Self {
            pending,
            interfaces,
            reads,
        })
    }
}

/// [MOD-8] the pending declarations of an acceptance a cache recorded for
/// `check`'s own records and graph facts, when it still holds: every
/// closure module's interface record still holds its own judgments, and
/// every declaration the recorded check reached still has the digest it
/// read. Only an interface record whose bytes changed is parsed again, and
/// only a module whose interface or dependency closure changed is checked
/// again, through its own interface verdict.
fn validated_acceptance(
    graph: &crate::ModuleGraph,
    inputs: &[SourceInput<'_>],
    check: &ModuleCheck<'_>,
    limits: CompilerLimits,
    cache: &BuildCache,
) -> Result<Option<Vec<String>>, CompilationFailure> {
    let Some(recorded) = cache
        .load(MODULE_VERDICTS, &check.own)
        .and_then(|payload| Acceptance::decode(&payload))
    else {
        return Ok(None);
    };
    let closure = graph.dependency_closure(check.target);
    if recorded.interfaces.len() != closure.len() {
        return Ok(None);
    }
    let mut changed = BTreeSet::new();
    for module in &closure {
        let name = module_name(graph, *module);
        let digest = interface_record(&check.selected, *module).map(content_digest);
        let unchanged = recorded
            .interfaces
            .iter()
            .any(|(recorded, recorded_digest)| {
                *recorded == name && Some(*recorded_digest) == digest
            });
        if !unchanged {
            changed.insert(*module);
        }
    }
    if changed.is_empty() {
        return Ok(Some(recorded.pending));
    }
    for module in &closure {
        let affected = changed.contains(module)
            || graph
                .dependency_closure(*module)
                .iter()
                .any(|dependency| changed.contains(dependency));
        if !affected {
            continue;
        }
        let interface = ModuleCheck::new(graph, inputs, *module, true);
        let accepted = if let Some(accepted) = cache.settled(&interface.material) {
            accepted
        } else {
            let verdict = module_verdict(
                graph,
                inputs,
                &module_name(graph, *module),
                true,
                limits,
                Some(cache),
            )?;
            let accepted = matches!(verdict.outcome, CheckOutcome::Accepted { .. });
            cache.settle(&interface.material, accepted);
            accepted
        };
        if !accepted {
            return Ok(None);
        }
    }
    let mut digests = BTreeMap::new();
    for (name, item, digest) in &recorded.reads {
        let Some(module) = graph.module_named(name) else {
            return Ok(None);
        };
        if !changed.contains(&module) {
            continue;
        }
        let parsed = digests.entry(module).or_insert_with(|| {
            interface_record(&check.selected, module)
                .and_then(|bytes| reads::declaration_digests(bytes, limits))
        });
        if parsed.as_ref().and_then(|parsed| parsed.get(item)) != Some(digest) {
            return Ok(None);
        }
    }
    Ok(Some(recorded.pending))
}

/// [MOD-8] a composition holds only while every selected module's own
/// verdict does. Each module is checked against its dependencies' interfaces
/// alone, in row order, reusing an acceptance a cache recorded for exactly
/// its inputs, and the first rejected module's failure is the composition's.
fn require_module_verdicts(
    graph: &crate::ModuleGraph,
    inputs: &[SourceInput<'_>],
    modules: &[crate::ModuleId],
    limits: CompilerLimits,
    cache: Option<&BuildCache>,
) -> Result<(), CompilationFailure> {
    for module in modules {
        let check = ModuleCheck::new(graph, inputs, *module, false);
        if let Some(cache) = cache
            && validated_acceptance(graph, inputs, &check, limits, cache)?.is_some()
        {
            continue;
        }
        let reading = content_digest(&check.reading);
        let recorded = cache
            .and_then(|cache| cache.load(MODULE_VERDICTS, &check.material))
            .and_then(|payload| decode_outcome(&payload, &reading));
        if matches!(recorded, Some(CheckOutcome::Accepted { .. })) {
            continue;
        }
        // A rejection is left to a module check, which records it with its
        // impact report; a build needs only its first failure.
        let judged = check.run(graph, limits, cache)?;
        if let Some(cache) = cache {
            record_acceptances(graph, inputs, &check, &judged, limits, cache);
            let pending = judged
                .first()
                .map(|judged| judged.pending.clone())
                .unwrap_or_default();
            // A failed publication costs only a later recomputation.
            let _ = cache.store(
                MODULE_VERDICTS,
                &check.material,
                &encode_outcome(&CheckOutcome::Accepted { pending }, &reading),
            );
        }
    }
    Ok(())
}

/// A composition check's result as a verdict records it: an acceptance, or a
/// source rejection with its one task. Any other failure returns as it is
/// and is never recorded.
fn outcome_of(
    result: Result<Vec<String>, CompilationFailure>,
    records: &[SourceInput<'_>],
) -> Result<CheckOutcome, CompilationFailure> {
    match result {
        Ok(pending) => Ok(CheckOutcome::Accepted { pending }),
        Err(failure) if failure.kind() == CompilationFailureKind::Source => {
            let texts = records
                .iter()
                .map(|input| input.bytes().to_vec())
                .collect::<Vec<_>>();
            let item = failing_item(&failure, records, &texts);
            Ok(CheckOutcome::Rejected {
                rule: failure.rule_id().map(str::to_owned),
                failure: failure.to_string(),
                tasks: vec![impact_task(&failure, records, item.as_ref())],
                complete: false,
            })
        }
        Err(failure) => Err(failure),
    }
}

/// The verdict recorded for exactly `material`, or the one `check` computes,
/// which is then recorded. A recorded rejection also needs its exact
/// `reading`.
fn recorded_verdict(
    subject: &str,
    cache: Option<&BuildCache>,
    family: &str,
    (material, reading): (&[u8], &[u8]),
    check: impl FnOnce() -> Result<CheckOutcome, CompilationFailure>,
) -> Result<CheckVerdict, CompilationFailure> {
    let reading = content_digest(reading);
    let recorded = cache
        .and_then(|cache| cache.load(family, material))
        .and_then(|payload| decode_outcome(&payload, &reading));
    if let Some(outcome) = recorded {
        return Ok(CheckVerdict {
            subject: subject.to_owned(),
            outcome,
            reused: true,
        });
    }
    let outcome = check()?;
    if let Some(cache) = cache {
        // A failed publication costs only a later recomputation.
        let _ = cache.store(family, material, &encode_outcome(&outcome, &reading));
    }
    Ok(CheckVerdict {
        subject: subject.to_owned(),
        outcome,
        reused: false,
    })
}

/// [MOD-9] the records an entry's composition reads: every record of its
/// module and of that module's dependency closure. A registered module the
/// entry's module does not depend on is no part of it.
fn composition_inputs<'input>(
    graph: &crate::ModuleGraph,
    inputs: &[SourceInput<'input>],
    module: crate::ModuleId,
) -> (Vec<crate::ModuleId>, Vec<SourceInput<'input>>) {
    let mut modules = graph.dependency_closure(module);
    modules.push(module);
    modules.sort();
    let selected = inputs
        .iter()
        .copied()
        .filter(|input| modules.contains(&input.module()))
        .collect();
    (modules, selected)
}

/// The key material of one entry's composition: the selection and its
/// requirement, the graph facts its modules read, and every record of them,
/// sorted or in the order the composition reads them.
fn composition_material(
    family: &str,
    graph: &crate::ModuleGraph,
    selection: &Selection<'_>,
    (modules, selected): (&[crate::ModuleId], &[SourceInput<'_>]),
    order: RecordOrder,
) -> Vec<u8> {
    let mut material = format!("{family} 2\n").into_bytes();
    push_module_line(&mut material, "entry-module", graph, selection.module);
    material.extend_from_slice(
        format!(
            "function {}\nno_heap {}\nnamed {}\n",
            selection.name, selection.no_heap, selection.public
        )
        .as_bytes(),
    );
    push_graph_facts(&mut material, graph, modules);
    push_records(&mut material, graph, selected, order);
    material
}

/// The cache family of entry composition verdicts.
const COMPOSITION_VERDICTS: &str = "composition-verdicts";

/// The cache family of entry builds' emitted modules.
const ENTRY_MODULES: &str = "entry-modules";

/// [MOD-8, MOD-9] checks one entry's composition, stopping before lowering,
/// and reports its verdict: every module of the entry's closure must hold its
/// own verdict, in row order, and then the composition must hold its
/// definitions, instances and the entry's own requirements. With a cache, a
/// verdict recorded for exactly the same composition is reused, and so is
/// each module's. `known` holds complete module verdicts this caller has
/// already computed, which are not computed again.
///
/// # Errors
///
/// Returns a failure that is not a source rejection: an entry naming no
/// graph entry or module, an exhausted limit or a compiler invariant.
pub fn entry_verdict(
    graph: &crate::ModuleGraph,
    inputs: &[SourceInput<'_>],
    entry: ModuleEntry<'_>,
    limits: CompilerLimits,
    cache: Option<&BuildCache>,
    known: &[CheckVerdict],
) -> Result<CheckVerdict, CompilationFailure> {
    let inputs = &with_library_records(graph, inputs);
    let subject = match entry {
        ModuleEntry::Named(name) => name.to_owned(),
        ModuleEntry::Function { module, function } => format!("{module}::{function}"),
    };
    let selection = entry_selection(graph, entry)?;
    let (modules, selected) = composition_inputs(graph, inputs, selection.module);
    let material = composition_material(
        COMPOSITION_VERDICTS,
        graph,
        &selection,
        (&modules, &selected),
        RecordOrder::Sorted,
    );
    let mut reading = rejection_reading(graph, &selected);
    // A named entry's rejection quotes where the graph writes it.
    if let Some(place) = selection.written {
        let mut fields = Fields::default();
        fields.push(&place.reading());
        reading.extend_from_slice(b"written ");
        reading.extend_from_slice(&fields.into_bytes());
        reading.push(b'\n');
    }
    // [MOD-8] an acceptance stands while every implementation record is
    // unchanged and every interface record means what it meant: an edited
    // `doc` string decides nothing a composition concludes.
    let acceptance = composition_acceptance(graph, &selection, (&modules, &selected), limits);
    if let Some(cache) = cache
        && cache.load(COMPOSITION_VERDICTS, &acceptance).is_some()
    {
        return Ok(CheckVerdict {
            subject,
            outcome: CheckOutcome::Accepted {
                pending: Vec::new(),
            },
            reused: true,
        });
    }
    let verdict = recorded_verdict(
        &subject,
        cache,
        COMPOSITION_VERDICTS,
        (&material, &reading),
        || {
            for module in &modules {
                let name = graph
                    .modules()
                    .get(module.index())
                    .map_or_else(String::new, crate::ModuleRecord::qualified_name);
                let verdict = match known.iter().find(|verdict| verdict.subject == name) {
                    Some(verdict) => verdict.clone(),
                    None => module_verdict(graph, inputs, &name, false, limits, cache)?,
                };
                if let CheckOutcome::Rejected { .. } = verdict.outcome {
                    return Ok(verdict.outcome);
                }
            }
            outcome_of(
                with_checked_program_using(
                    &selected,
                    Some(graph.modules()),
                    limits,
                    cache,
                    |checked, resolved| admit_entry(&checked, resolved, &selection),
                )
                .map(|()| Vec::new()),
                &selected,
            )
        },
    )?;
    if let Some(cache) = cache
        && matches!(verdict.outcome, CheckOutcome::Accepted { .. })
    {
        // A failed publication costs only a later recomputation.
        let _ = cache.store(COMPOSITION_VERDICTS, &acceptance, b"accepted");
    }
    Ok(verdict)
}

/// [MOD-8] the key material of an accepted composition: the selection and
/// its requirement, the graph facts its modules read, every implementation
/// record exactly, and every interface record by its text with each `doc`
/// string emptied, which only a reworded `doc` string leaves unchanged. An
/// interface record the syntax stages refuse enters exactly.
fn composition_acceptance(
    graph: &crate::ModuleGraph,
    selection: &Selection<'_>,
    (modules, selected): (&[crate::ModuleId], &[SourceInput<'_>]),
    limits: CompilerLimits,
) -> Vec<u8> {
    let mut material = b"composition-acceptance 2\n".to_vec();
    push_module_line(&mut material, "entry-module", graph, selection.module);
    material.extend_from_slice(
        format!(
            "function {}\nno_heap {}\nnamed {}\n",
            selection.name, selection.no_heap, selection.public
        )
        .as_bytes(),
    );
    push_graph_facts(&mut material, graph, modules);
    let mut records = selected
        .iter()
        .map(|input| {
            let mut fields = Fields::default();
            fields
                .push(module_name(graph, input.module()).as_bytes())
                .push(input.logical_path().as_bytes());
            let meaning = (input.role() == crate::SourceRole::Interface)
                .then(|| reads::record_meaning(input.bytes(), limits))
                .flatten();
            match meaning {
                Some(meaning) => {
                    fields.push(b"interface").push(&meaning);
                }
                None => {
                    fields.push(b"exact").push(input.bytes());
                }
            }
            fields.into_bytes()
        })
        .collect::<Vec<_>>();
    records.sort();
    for record in records {
        material.extend_from_slice(b"record ");
        material.extend_from_slice(&record);
        material.push(b'\n');
    }
    material
}

/// [MOD-8] the interface function declarations of `module` that no
/// implementation record of it defines, in source order.
fn pending_declarations(resolved: &ResolvedSyntaxUnit, module: crate::ModuleId) -> Vec<String> {
    resolved
        .interface_functions()
        .iter()
        .filter(|function| function.definition().is_none())
        .filter_map(|function| resolved.declaration(function.declaration()))
        .filter(|declaration| declaration.module() == Some(module))
        .map(|declaration| declaration.spelling().to_owned())
        .collect()
}

fn push_module_line(
    material: &mut Vec<u8>,
    label: &str,
    graph: &crate::ModuleGraph,
    module: crate::ModuleId,
) {
    let name = graph
        .modules()
        .get(module.index())
        .map_or_else(String::new, crate::ModuleRecord::qualified_name);
    material.extend_from_slice(format!("{label} {name}\n").as_bytes());
}

/// The graph facts a check over `modules` reads whatever it concludes,
/// independent of row and edge order [MOD-1]: each of those modules' direct
/// dependencies, which name permission compares with [MOD-5], and every
/// registered module one path component below one of them, whose last
/// component a top-level declaration of that module may not take [MOD-3].
/// An accepted check names only modules among these; which other modules
/// are registered changes only which rejection a check reports, and a
/// rejection's reading covers it. Entries, comments and spacing of the graph
/// file are not among them.
fn push_graph_facts(
    material: &mut Vec<u8>,
    graph: &crate::ModuleGraph,
    modules: &[crate::ModuleId],
) {
    let records = graph.modules();
    let name = |module: &crate::ModuleId| {
        records
            .get(module.index())
            .map_or_else(String::new, crate::ModuleRecord::qualified_name)
    };
    let mut edges = modules
        .iter()
        .map(|module| {
            let mut dependencies = records
                .get(module.index())
                .map_or(&[][..], crate::ModuleRecord::dependencies)
                .iter()
                .map(name)
                .collect::<Vec<_>>();
            dependencies.sort();
            (name(module), dependencies)
        })
        .collect::<Vec<_>>();
    edges.sort();
    for (module, dependencies) in &edges {
        material.extend_from_slice(format!("edges {module}:").as_bytes());
        for dependency in dependencies {
            material.push(b' ');
            material.extend_from_slice(dependency.as_bytes());
        }
        material.push(b'\n');
    }
    // A child is registered in its parent's package: a program module never
    // sits below a standard library module, nor one below a program module
    // [MOD-10].
    let mut children = records
        .iter()
        .filter(|record| {
            record.path().split_last().is_some_and(|(_, parent)| {
                modules.iter().any(|module| {
                    records
                        .get(module.index())
                        .is_some_and(|candidate| candidate.is_at(record.package(), parent))
                })
            })
        })
        .map(crate::ModuleRecord::qualified_name)
        .collect::<Vec<_>>();
    children.sort();
    material.extend_from_slice(b"children");
    for child in &children {
        material.push(b' ');
        material.extend_from_slice(child.as_bytes());
    }
    material.push(b'\n');
}

/// What a rejection depends on beyond the key material: every registered
/// module in row order, and the order in which the check reads its records,
/// which selects the first of several defects it reports [DIAG-1].
fn rejection_reading(graph: &crate::ModuleGraph, inputs: &[SourceInput<'_>]) -> Vec<u8> {
    let mut reading = b"registered".to_vec();
    for module in graph.modules() {
        reading.push(b' ');
        reading.extend_from_slice(module.qualified_name().as_bytes());
    }
    reading.push(b'\n');
    for input in inputs {
        let module = graph
            .modules()
            .get(input.module().index())
            .map_or_else(String::new, crate::ModuleRecord::qualified_name);
        let mut fields = Fields::default();
        fields
            .push(module.as_bytes())
            .push(input.logical_path().as_bytes());
        reading.extend_from_slice(&fields.into_bytes());
        reading.push(b'\n');
    }
    reading
}

/// The order in which key material lists records.
#[derive(Clone, Copy)]
enum RecordOrder {
    /// Sorted by their encoding, for a verdict, which a read order changes
    /// only through a rejection's reading.
    Sorted,
    /// The order the check reads them, for an emitted module, whose symbols
    /// and type names follow it.
    Read,
}

/// Each record a check reads: its module, role, logical and display names,
/// and bytes.
fn push_records(
    material: &mut Vec<u8>,
    graph: &crate::ModuleGraph,
    inputs: &[SourceInput<'_>],
    order: RecordOrder,
) {
    let mut records = inputs
        .iter()
        .map(|input| {
            let module = graph
                .modules()
                .get(input.module().index())
                .map_or_else(String::new, crate::ModuleRecord::qualified_name);
            let role = match input.role() {
                crate::SourceRole::Interface => "interface",
                crate::SourceRole::Implementation => "implementation",
            };
            let mut fields = Fields::default();
            fields
                .push(module.as_bytes())
                .push(role.as_bytes())
                .push(input.logical_path().as_bytes())
                .push(input.display_path().as_bytes())
                .push(input.bytes());
            fields.into_bytes()
        })
        .collect::<Vec<_>>();
    if let RecordOrder::Sorted = order {
        records.sort();
    }
    for record in records {
        material.extend_from_slice(b"record ");
        material.extend_from_slice(&record);
        material.push(b'\n');
    }
}

/// A recorded outcome; a rejection carries the digest of its reading.
fn encode_outcome(outcome: &CheckOutcome, reading: &[u8]) -> Vec<u8> {
    let mut fields = Fields::default();
    match outcome {
        CheckOutcome::Accepted { pending } => {
            fields.push(b"accepted");
            for name in pending {
                fields.push(name.as_bytes());
            }
        }
        CheckOutcome::Rejected {
            rule,
            failure,
            tasks,
            complete,
        } => {
            fields
                .push(b"rejected")
                .push(rule.as_deref().unwrap_or_default().as_bytes())
                .push(failure.as_bytes())
                .push(reading)
                .push(if *complete { b"complete" } else { b"partial" });
            for task in tasks {
                let (path, line, column) = task.location.as_ref().map_or_else(
                    || (String::new(), String::new(), String::new()),
                    |location| {
                        (
                            location.path.clone(),
                            location.line.to_string(),
                            location.column.to_string(),
                        )
                    },
                );
                fields
                    .push(task.kind.name().as_bytes())
                    .push(task.function.as_deref().unwrap_or_default().as_bytes())
                    .push(task.rule.as_deref().unwrap_or_default().as_bytes())
                    .push(path.as_bytes())
                    .push(line.as_bytes())
                    .push(column.as_bytes())
                    .push(task.failure.as_bytes());
            }
        }
    }
    fields.into_bytes()
}

/// A recorded outcome, or `None` when the payload is malformed or records a
/// rejection read in another way than `reading`.
fn decode_outcome(payload: &[u8], reading: &[u8]) -> Option<CheckOutcome> {
    let fields = Fields::parse(payload)?;
    let text = |field: &[u8]| String::from_utf8(field.to_vec()).ok();
    match fields.as_slice() {
        [b"accepted", pending @ ..] => Some(CheckOutcome::Accepted {
            pending: pending
                .iter()
                .map(|name| text(name))
                .collect::<Option<Vec<_>>>()?,
        }),
        [b"rejected", rule, failure, recorded, complete, tasks @ ..]
            if *recorded == reading && tasks.len() % 7 == 0 =>
        {
            let optional = |field: &[u8]| (!field.is_empty()).then(|| text(field)).flatten();
            let tasks = tasks
                .chunks(7)
                .map(|task| {
                    let [kind, function, rule, path, line, column, failure] = task else {
                        return None;
                    };
                    let location = if path.is_empty() {
                        None
                    } else {
                        Some(SourceLocation {
                            path: text(path)?,
                            line: text(line)?.parse().ok()?,
                            column: text(column)?.parse().ok()?,
                        })
                    };
                    Some(ImpactTask {
                        kind: TaskKind::named(kind)?,
                        function: optional(function),
                        rule: optional(rule),
                        location,
                        failure: text(failure)?,
                    })
                })
                .collect::<Option<Vec<_>>>()?;
            Some(CheckOutcome::Rejected {
                rule: optional(rule),
                failure: text(failure)?,
                tasks,
                complete: *complete == b"complete",
            })
        }
        _ => None,
    }
}

/// [MOD-8, MOD-9] checks one entry's composition, stopping before lowering:
/// every module of the entry's closure holds its own verdict, in row order,
/// and then the composition admits the entry.
pub fn check_module_entry(
    graph: &crate::ModuleGraph,
    inputs: &[SourceInput<'_>],
    entry: ModuleEntry<'_>,
    limits: CompilerLimits,
) -> Result<(), CompilationFailure> {
    let inputs = &with_library_records(graph, inputs);
    let selection = entry_selection(graph, entry)?;
    let (modules, selected) = composition_inputs(graph, inputs, selection.module);
    require_module_verdicts(graph, inputs, &modules, limits, None)?;
    with_checked_program(
        &selected,
        Some(graph.modules()),
        limits,
        |checked, resolved| admit_entry(&checked, resolved, &selection),
    )
}

/// The function a module program entry names [MOD-9].
fn entry_selection<'graph>(
    graph: &'graph crate::ModuleGraph,
    entry: ModuleEntry<'graph>,
) -> Result<Selection<'graph>, CompilationFailure> {
    let invocation = |detail: String| {
        CompilationFailure::new(
            CompilationStage::ModuleGraph,
            CompilationFailureKind::Invocation,
            detail,
        )
    };
    Ok(match entry {
        ModuleEntry::Named(name) => {
            let entry = graph
                .entry(name)
                .ok_or_else(|| invocation(format!("the graph has no entry named `{name}`")))?;
            Selection {
                module: entry.module(),
                name: entry.function(),
                no_heap: entry.no_heap(),
                public: true,
                written: entry.written(),
            }
        }
        ModuleEntry::Function { module, function } => Selection {
            module: graph
                .module_named(module)
                .ok_or_else(|| invocation(format!("the graph registers no module `{module}`")))?,
            name: function,
            no_heap: false,
            public: false,
            written: None,
        },
    })
}

/// Compiles a module program for one named or unnamed entry to textual LLVM
/// [MOD-9, PROG-3].
pub fn compile_module_program(
    graph: &crate::ModuleGraph,
    inputs: &[SourceInput<'_>],
    entry: ModuleEntry<'_>,
    limits: CompilerLimits,
    overlap: crate::OverlapLowering,
) -> Result<String, CompilationFailure> {
    build_module_entry(graph, inputs, entry, limits, overlap, None).map(|(module, _)| module)
}

/// Compiles one entry's composition to textual LLVM [MOD-9, PROG-3], reusing
/// the module a cache recorded for exactly the same composition, lowering
/// options and compiler, and reporting whether it did. Only a successful
/// build is recorded.
///
/// # Errors
///
/// Returns the build's failure, which no cache record ever stands in for.
pub fn build_module_entry(
    graph: &crate::ModuleGraph,
    inputs: &[SourceInput<'_>],
    entry: ModuleEntry<'_>,
    limits: CompilerLimits,
    overlap: crate::OverlapLowering,
    cache: Option<&BuildCache>,
) -> Result<(String, bool), CompilationFailure> {
    let inputs = &with_library_records(graph, inputs);
    let selection = entry_selection(graph, entry)?;
    let (modules, selected) = composition_inputs(graph, inputs, selection.module);
    let mut material = composition_material(
        ENTRY_MODULES,
        graph,
        &selection,
        (&modules, &selected),
        RecordOrder::Read,
    );
    material.extend_from_slice(format!("overlap {overlap:?}\n").as_bytes());
    // Only a build that composed is recorded, so a recorded module implies
    // that every module verdict of its closure held for these inputs.
    if let Some(module) = cache
        .and_then(|cache| cache.load(ENTRY_MODULES, &material))
        .and_then(|payload| String::from_utf8(payload).ok())
    {
        return Ok((module, true));
    }
    require_module_verdicts(graph, inputs, &modules, limits, cache)?;
    let module = compile_selected(
        &selected,
        Some(graph.modules()),
        limits,
        overlap,
        &selection,
        receipts_for(overlap, cache),
    )?
    .module;
    if let Some(cache) = cache {
        // A failed publication costs only a later rebuild.
        let _ = cache.store(ENTRY_MODULES, &material, module.as_bytes());
    }
    Ok((module, false))
}

/// One compilation's module and the developer-channel text it produced.
struct Reported {
    module: String,
    ledger: Vec<String>,
}

/// The one compilation path, returning the module and the developer-channel
/// permission ledger it produced. Every public entry point above is a
/// projection of this function; there is no second pipeline.
fn compile_reporting(
    inputs: &[SourceInput<'_>],
    limits: CompilerLimits,
    overlap: crate::OverlapLowering,
) -> Result<Reported, CompilationFailure> {
    compile_selected(
        inputs,
        None,
        limits,
        overlap,
        &Selection {
            module: crate::ModuleId::BUNDLE_ROOT,
            name: "main",
            no_heap: false,
            public: false,
            written: None,
        },
        None,
    )
}

/// The function one invocation runs [PROG-3]: a source bundle's `main`, or a
/// module program's named or unnamed entry [MOD-9].
struct Selection<'a> {
    module: crate::ModuleId,
    name: &'a str,
    /// The entry states the no-heap requirement [STOR-8].
    no_heap: bool,
    /// A named entry, which must select a public function [MOD-9].
    public: bool,
    /// Where a named entry is written in the graph record.
    written: Option<&'a Place>,
}

impl<'a> Selection<'a> {
    const fn request(&self) -> EntryRequest<'a> {
        EntryRequest {
            module: self.module,
            name: self.name,
            public: self.public,
            no_heap: self.no_heap,
        }
    }
}

/// A rejection an entry's composition finds in its checked closure, beyond
/// the module verdicts it requires first [MOD-8, MOD-9, STOR-8].
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CompositionIssue {
    /// An interface declares this function and no implementation record of
    /// its module defines it yet: the module checks, but no entry composes
    /// until the definition exists [MOD-8].
    PendingDeclaration { declaration: String },
    /// The entry names no ordinary nongeneric function of its module [MOD-9].
    EntryFunctionMissing { function: String },
    /// A named entry runs a public function, and this one is private to its
    /// module [MOD-9].
    EntryFunctionPrivate { function: String },
    /// The entry states `no_heap`, and its execution closure requires the
    /// heap along this call path, each function named by its module's path;
    /// the last one introduces the requirement by allocating or by holding a
    /// `Box` or runtime-capacity value [STOR-8].
    HeapInClosure { path: Vec<String> },
}

fn compile_selected(
    inputs: &[SourceInput<'_>],
    modules: Option<&[crate::ModuleRecord]>,
    limits: CompilerLimits,
    overlap: crate::OverlapLowering,
    selection: &Selection<'_>,
    receipts: Option<&BuildCache>,
) -> Result<Reported, CompilationFailure> {
    with_checked_program_using(inputs, modules, limits, receipts, |checked, resolved| {
        if modules.is_some() {
            admit_entry(&checked, resolved, selection)?;
        }
        lower_selected(
            inputs,
            (modules, limits, overlap, receipts),
            selection,
            resolved,
            checked,
        )
    })
}

/// Locates and renders an entry's composition rejection [MOD-8, MOD-9,
/// STOR-8]; the checked program makes the judgment.
fn admit_entry(
    checked: &CheckedProgram,
    resolved: &ResolvedSyntaxUnit,
    selection: &Selection<'_>,
) -> Result<(), CompilationFailure> {
    let Err(rejection) = checked.admit_entry(resolved, selection.request()) else {
        return Ok(());
    };
    let bundle = resolved.syntax().classified_bundle().source_bundle();
    let rule = rejection.rule();
    // A named entry's rejection is located at its `entry_decl` in the graph
    // record; an unnamed entry is written only in the build's selection.
    let at_entry = |issue: CompositionIssue| {
        CompilationFailure::composition(rule, &issue, bundle, selection.written.cloned())
    };
    let at_declaration = |issue: CompositionIssue, declaration: &crate::DeclarationRecord| {
        CompilationFailure::at_source(
            CompilationStage::Semantics,
            rule,
            &issue,
            bundle,
            declaration.origin().coordinate(),
            Anchor::Start,
        )
    };
    Err(match rejection {
        EntryRejection::PendingDeclaration(declaration) => at_declaration(
            CompositionIssue::PendingDeclaration {
                declaration: declaration.spelling().to_owned(),
            },
            declaration,
        ),
        EntryRejection::FunctionMissing => at_entry(CompositionIssue::EntryFunctionMissing {
            function: selection.name.to_owned(),
        }),
        EntryRejection::FunctionPrivate => at_entry(CompositionIssue::EntryFunctionPrivate {
            function: selection.name.to_owned(),
        }),
        EntryRejection::HeapInClosure { path, introducer } => {
            // The path names each function by its module's path and its
            // source name; an instance keeps its template's name, since the
            // walk passes through it with its supplied actuals.
            let names = path
                .iter()
                .filter_map(|id| checked.data.functions.get(id.0 as usize))
                .map(|function| match bundle.module(function.module) {
                    Some(module) => format!("{}::{}", module.qualified_name(), function.name),
                    None => function.name.clone(),
                })
                .collect::<Vec<_>>();
            let issue = CompositionIssue::HeapInClosure { path: names };
            match introducer {
                Some(declaration) => at_declaration(issue, declaration),
                None => CompilationFailure::composition(rule, &issue, bundle, None),
            }
        }
    })
}

/// Runs the syntax stages over one bundle, from raw lexical formation through
/// the canonical [FORM-2] audit, and returns the canonical unit, which owns
/// every stage's result. A module graph file takes the `graph_file` start and
/// every source bundle the `program` start [GRAM-2, MOD-1]; the stages are
/// otherwise one path.
fn canonical_syntax(
    bundle: &SourceBundle,
    limits: CompilerLimits,
    graph: bool,
) -> Result<CanonicalSyntaxUnit, CompilationFailure> {
    let lexed = match lex(bundle, limits.lexer) {
        LexOutcome::Complete(complete) => complete,
        LexOutcome::SourceIssue(issue) => {
            let span = issue.span();
            return Err(CompilationFailure::at_source(
                CompilationStage::Lexing,
                issue.kind().rule_id(),
                &issue,
                bundle,
                crate::SyntaxCoordinate::new(span.source(), span.start(), span.end()),
                Anchor::Start,
            ));
        }
        LexOutcome::ResourceFailure(failure) => {
            return Err(CompilationFailure::new(
                CompilationStage::Lexing,
                CompilationFailureKind::Resource,
                failure,
            ));
        }
        LexOutcome::CompilerFailure(failure) => {
            return Err(CompilationFailure::new(
                CompilationStage::Lexing,
                CompilationFailureKind::Compiler,
                failure,
            ));
        }
    };
    let classified = match classify_terminals(&lexed, ACTIVE_KERNEL_SPEC_HASH, limits.terminals) {
        TerminalOutcome::Complete(complete) => complete,
        TerminalOutcome::SourceIssue(issue) => {
            let token = issue.token();
            return Err(CompilationFailure::at_source(
                CompilationStage::TerminalClassification,
                issue.owner().id(),
                &issue,
                bundle,
                crate::SyntaxCoordinate::new(token.source(), token.start(), token.end()),
                Anchor::Start,
            ));
        }
        TerminalOutcome::ResourceFailure(failure) => {
            return Err(CompilationFailure::new(
                CompilationStage::TerminalClassification,
                CompilationFailureKind::Resource,
                failure,
            ));
        }
        TerminalOutcome::InvocationFailure(failure) => {
            return Err(CompilationFailure::new(
                CompilationStage::TerminalClassification,
                CompilationFailureKind::Invocation,
                failure,
            ));
        }
        TerminalOutcome::CompilerFailure(failure) => {
            return Err(CompilationFailure::new(
                CompilationStage::TerminalClassification,
                CompilationFailureKind::Compiler,
                failure,
            ));
        }
    };
    let parsed = match if graph {
        parse_graph(classified, limits.parser)
    } else {
        parse(classified, limits.parser)
    } {
        ParseOutcome::Complete(complete) => complete,
        ParseOutcome::SourceIssue(issue) => {
            return Err(CompilationFailure::at_source(
                CompilationStage::Parsing,
                issue.rule().id(),
                &issue,
                bundle,
                issue.coordinate(),
                Anchor::Start,
            ));
        }
        ParseOutcome::ResourceFailure(failure) => {
            return Err(CompilationFailure::new(
                CompilationStage::Parsing,
                CompilationFailureKind::Resource,
                failure,
            ));
        }
        ParseOutcome::InvocationFailure(failure) => {
            return Err(CompilationFailure::new(
                CompilationStage::Parsing,
                CompilationFailureKind::Invocation,
                failure,
            ));
        }
        ParseOutcome::CompilerFailure(failure) => {
            return Err(CompilationFailure::new(
                CompilationStage::Parsing,
                CompilationFailureKind::Compiler,
                failure,
            ));
        }
    };
    let finalized = match finalize(parsed, limits.finalizer) {
        FinalizeOutcome::Complete(complete) => *complete,
        FinalizeOutcome::ResourceFailure(failure) => {
            return Err(CompilationFailure::new(
                CompilationStage::Finalization,
                CompilationFailureKind::Resource,
                failure,
            ));
        }
        FinalizeOutcome::CompilerFailure(failure) => {
            return Err(CompilationFailure::new(
                CompilationStage::Finalization,
                CompilationFailureKind::Compiler,
                failure,
            ));
        }
    };
    let canonical = match audit_canonical(finalized, limits.canonical) {
        CanonicalOutcome::Complete(complete) => complete,
        CanonicalOutcome::SourceIssue(issue) => {
            // FORM-2's coordinate is the trivia gap between two terminals, not
            // a written construct, so the reader is anchored inside the gap
            // rather than at its first byte.
            return Err(CompilationFailure::at_source(
                CompilationStage::CanonicalSource,
                issue.rule().id(),
                &issue,
                bundle,
                issue.location().coordinate(),
                Anchor::LastLineOfGap,
            ));
        }
        CanonicalOutcome::ResourceFailure(failure) => {
            return Err(CompilationFailure::new(
                CompilationStage::CanonicalSource,
                CompilationFailureKind::Resource,
                failure,
            ));
        }
        CanonicalOutcome::CompilerFailure(failure) => {
            return Err(CompilationFailure::new(
                CompilationStage::CanonicalSource,
                CompilationFailureKind::Compiler,
                failure,
            ));
        }
    };
    Ok(canonical)
}

/// Runs the one source front end and hands its checked program, with the
/// resolved unit it was checked over, to one projection. Both `check` and
/// `compile` enter here; neither reconstructs a source verdict.
fn with_checked_program<T, F>(
    inputs: &[SourceInput<'_>],
    modules: Option<&[crate::ModuleRecord]>,
    limits: CompilerLimits,
    continuation: F,
) -> Result<T, CompilationFailure>
where
    F: FnOnce(CheckedProgram, &ResolvedSyntaxUnit) -> Result<T, CompilationFailure>,
{
    with_checked_program_using(inputs, modules, limits, None, continuation)
}

/// [`with_checked_program`] whose checker takes and keeps proof receipts in
/// `receipts` [MOD-8]. The checked program is the same one, apart from the
/// entailment detail a reused analysis does not retain, which only the
/// permission table reads.
fn with_checked_program_using<T, F>(
    inputs: &[SourceInput<'_>],
    modules: Option<&[crate::ModuleRecord]>,
    limits: CompilerLimits,
    receipts: Option<&BuildCache>,
    continuation: F,
) -> Result<T, CompilationFailure>
where
    F: FnOnce(CheckedProgram, &ResolvedSyntaxUnit) -> Result<T, CompilationFailure>,
{
    let bundle = match modules {
        Some(modules) => {
            SourceBundle::with_prelude_and_modules(inputs, modules.to_vec(), limits.source)
        }
        None => SourceBundle::with_prelude(inputs, limits.source),
    }
    .map_err(CompilationFailure::source_envelope)?;
    let canonical = canonical_syntax(&bundle, limits, false)?;
    let resolved = match resolve(canonical) {
        ResolutionOutcome::Complete(complete) => complete,
        ResolutionOutcome::SourceIssue { issue, .. } => {
            return Err(CompilationFailure::at_source(
                CompilationStage::Resolution,
                issue.rule().id(),
                &issue,
                &bundle,
                issue.origin().coordinate(),
                Anchor::Start,
            ));
        }
        ResolutionOutcome::CompilerFailure { failure, .. } => {
            return Err(CompilationFailure::new(
                CompilationStage::Resolution,
                CompilationFailureKind::Compiler,
                failure,
            ));
        }
    };
    let outcome = match receipts {
        Some(receipts) => crate::semantic::check_semantics_with_receipts(&resolved, receipts),
        None => check_semantics(&resolved),
    };
    let checked = match outcome {
        SemanticOutcome::Complete(complete) => *complete,
        SemanticOutcome::SourceIssue { issue, .. } => {
            // A semantic rejection carries the richest payload in the
            // toolchain; the coordinate the rule already selected names a
            // line of the file the caller named, so it is printed the same
            // way a syntax rejection's is. A rejection in a concrete
            // instance also names the call that requested it [FN-2, MOD-8].
            return Err(CompilationFailure::at_source(
                CompilationStage::Semantics,
                issue.rule_id(),
                &issue,
                &bundle,
                issue.location().coordinate(),
                Anchor::Start,
            ));
        }
        SemanticOutcome::ResolutionIssue { issue, .. } => {
            return Err(CompilationFailure::at_source(
                CompilationStage::Resolution,
                issue.rule().id(),
                &issue,
                &bundle,
                issue.origin().coordinate(),
                Anchor::Start,
            ));
        }
        SemanticOutcome::Unsupported { unsupported, .. } => {
            // A capability stop is never a source verdict, but the node
            // that needed the capability is still where the writer looks.
            return Err(CompilationFailure {
                stage: CompilationStage::Semantics,
                kind: CompilationFailureKind::Unsupported,
                rule_id: None,
                record: Box::new(Record::located(
                    &unsupported,
                    &bundle,
                    unsupported.node.coordinate(),
                    Anchor::Start,
                )),
            });
        }
        SemanticOutcome::CompilerFailure { failure, .. } => {
            return Err(CompilationFailure::new(
                CompilationStage::Semantics,
                CompilationFailureKind::Compiler,
                failure,
            ));
        }
    };
    continuation(checked, &resolved)
}

fn lower_selected(
    inputs: &[SourceInput<'_>],
    (modules, limits, overlap, receipts): (
        Option<&[crate::ModuleRecord]>,
        CompilerLimits,
        crate::OverlapLowering,
        Option<&BuildCache>,
    ),
    selection: &Selection<'_>,
    resolved: &ResolvedSyntaxUnit,
    checked: CheckedProgram,
) -> Result<Reported, CompilationFailure> {
    let bundle = resolved.syntax().classified_bundle().source_bundle();
    let entry = checked.selected_function(resolved, selection.module, selection.name);
    let selected = entry
        .map_or(selection.name, |function| function.symbol.as_str())
        .to_owned();
    let selected = selected.as_str();
    let launcher_contract_ready = entry.is_some_and(|main| main.requirements.is_empty());
    // This is a build caller, checked by the same pipeline as user-written
    // callers. No precondition fact is manufactured from native initialization.
    let mut caller_failure = None;
    if !launcher_contract_ready
        && let Some(function) = entry
        && let Some((name, source)) = launcher::caller_source(resolved, function)
    {
        let bundle_name = (0_u64..)
            .map(|index| format!("executable-caller-{index}.wf"))
            .find(|candidate| {
                !bundle
                    .files()
                    .iter()
                    .any(|file| file.logical_path().as_str() == candidate)
            })
            .expect("a finite bundle leaves a caller source name");
        let mut with_caller = inputs.to_vec();
        with_caller.push(
            SourceInput::new(&bundle_name, source.as_bytes())
                .in_module(selection.module, crate::SourceRole::Implementation),
        );
        let caller_selection = Selection {
            module: selection.module,
            name: &name,
            no_heap: selection.no_heap,
            public: false,
            written: selection.written,
        };
        match compile_selected(
            &with_caller,
            modules,
            limits,
            overlap,
            &caller_selection,
            receipts,
        ) {
            Ok(reported) => return Ok(reported),
            // The JSON rendering is one line by construction, which is what an
            // LLVM comment can hold.
            Err(failure) if failure.kind() == CompilationFailureKind::Source => {
                caller_failure = Some(failure.render(DiagnosticFormat::Json));
            }
            Err(failure) => return Err(failure),
        }
    }
    let permission_ledger = checked.data.permission_ledger.clone();
    let target =
        TargetLayout::host().map_err(|failure| CompilationFailure::lowering(failure.into()))?;
    // [MOD-9] a module program entry's build emits what that entry's run
    // reaches; the other entries' code is checked but is not this
    // executable's. A source bundle keeps every definition.
    let roots = modules.and(entry).map(|function| [function.id]);
    let ir = crate::lower_checked_from(
        checked,
        overlap,
        target,
        roots.as_ref().map(|roots| roots.as_slice()),
    )
    .map_err(CompilationFailure::lowering)?;
    // What this lowering did with each permission it was given, appended after
    // the judgment's own lines. The judgment reports the same verdicts with or
    // without `--par`; these lines report an actualization, which only a
    // compilation that asked for one has.
    let mut ledger: Vec<String> = permission_ledger
        .into_iter()
        .map(|line| line.text)
        .collect();
    ledger.extend_from_slice(ir.actualization_ledger());
    emit_llvm_with_layout(&ir, target)
        .and_then(|module| {
            // Emission decides which recursive components carry a runtime
            // budget after their clone families are known.
            let mut ledger = ledger;
            ledger.extend_from_slice(module.actualization_ledger());
            let launch = if launcher_contract_ready {
                launcher::render(&ir, selected)?
            } else {
                String::new()
            };
            Ok(Reported {
                module: module.into_string()
                    + &launch
                    + &caller_failure.map_or_else(String::new, |failure| {
                        format!("\n; Executable caller was not admitted: {failure}\n")
                    }),
                ledger,
            })
        })
        .map_err(|failure: BackendFailure| {
            let (stage, kind) = match failure {
                BackendFailure::TargetLayout(_) => (
                    CompilationStage::TargetLayout,
                    CompilationFailureKind::TargetLayout,
                ),
                _ => (CompilationStage::Backend, CompilationFailureKind::Backend),
            };
            CompilationFailure::new(stage, kind, failure)
        })
}

#[cfg(test)]
mod tests;
