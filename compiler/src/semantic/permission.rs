//! The permission judgment [PAR-1]: whether two adjacent statements of one
//! block may be executed with their evaluations overlapped, and which runs of
//! adjacent statements may all overlap.
//!
//! P is a compiler-internal legality judgment. It refuses nothing, changes no
//! acceptance, and grants no lowering by itself; it records, per analyzed
//! block, which adjacent pairs may overlap and which runs the checked program
//! hands the backend as overlap groups.
//!
//! # The judgment
//!
//! [PAR-1] admits an overlap of two *adjacent* statements "exactly when the
//! first's write paths are disjoint from the second's read and write paths
//! and the second's write paths are disjoint from the first's, using the same
//! path-overlap and index/range-disjointness judgment as [EFF-5] and
//! [OWN-7]". Read/read overlap is admitted. There is no window of interposed
//! statements: two calls separated by a third statement are not adjacent, and
//! the run rule below is what lets all three overlap together.
//!
//! One statement's footprint has three halves:
//!
//! - **writes** — the places a call's declared `writes` row reaches through
//!   its actuals after [EFF-5] substitution, the place every by-value
//!   consumption empties ("a by-value consumption counts as a write of the
//!   argument's place"), the place a `set` names, and the binding a `let`
//!   defines ("a `let`'s defined binding is a write path"). That last clause
//!   is what makes ordinary dataflow a footprint conflict rather than a
//!   condition of its own: where the second statement reads what the first
//!   defines, the first's write path and the second's read path are one
//!   place.
//! - **reads** — the places a call's declared `reads` row reaches through the
//!   same substitution.
//! - **operand reads** — the storage the statement's own argument
//!   expressions touch on the calling thread. "Evaluating a statement's own
//!   argument expressions is part of that statement, so each statement's
//!   write paths must also be disjoint from the places the other statement's
//!   argument expressions read." Both directions are required, because which
//!   statement's operand evaluation an overlap moves is the implementation's
//!   choice of lane.
//!
//! Allocation and release contribute no path [STOR-8], so an allocating call
//! adds nothing here and never denies an adjacency on that ground.
//!
//! A footprint element whose caller place this analysis does not resolve
//! overlaps every place and denies permission, and a statement form whose
//! footprint this analysis does not compute denies for the same reason: a
//! missing element would *widen* permission, which is the one direction the
//! judgment must never fail in.
//!
//! # Runs
//!
//! "Permission composes: any run of adjacent statements that pairwise may
//! overlap may all overlap, and 'pairwise' means every ordered pair in the
//! run." A statement therefore joins the current run only after an exact
//! comparison with every earlier member. The pair identity matters: range
//! separation is proved in the flow state before that pair's first statement,
//! so an origin-free union could incorrectly reuse another pair's branch or
//! later fact. A greedy partition can lose an opportunity and can never
//! change a verdict, because permission is never an obligation.
//!
//! A run's members that are not calls carry no hand-out and leave the
//! parallel lowering's clone set untouched, as
//! `compiler/parallel-lowering/two-worlds` decides: only a handed-out call
//! has a lowering that differs between the two worlds, so a permitted
//! adjacency among ordinary statements is scheduling and alias metadata
//! rather than a new hand-out site.
//!
//! # Exit edges
//!
//! [PAR-1] v0.60 states no condition about exit edges, where v0.59 required
//! that "every normal continuation of s1 reaches s2". A statement whose
//! continuation may leave the block cannot be overlapped with the statement
//! written after it, because that statement may not execute at all and the
//! rule still promises that "bindings and every Whitefoot state place equal
//! the source-order result". This judgment therefore refuses an exit-bearing
//! statement, which is the fail-closed reading of a sentence the rule no
//! longer carries.
//!
//! # Proof statements
//!
//! Every source proof statement has already been checked against its
//! control-flow facts before permission metadata is built, and is erased
//! before lowering: it has no runtime evaluation, effect, exit edge, or
//! scheduler-visible event. A failed proof rejects the program instead of
//! creating a runtime fallback. The judgment therefore neither rechecks
//! proofs nor models a proof failure path, and a proof statement carries an
//! empty footprint.
//!
//! **Invariant.** This judgment consults typing, declared effect rows,
//! resolved places [REF-1, OWN-7], and statement-graph exit edges. It cannot
//! turn an accepted program into a rejected one or move a required check.

use super::loop_permission::LoopPermission;
use super::model::{
    BindingId, CheckedArrayRoot, CheckedEffects, CheckedExpression, CheckedFunction,
    CheckedMeasure, CheckedMode, CheckedPlaceStep, CheckedSetTarget, CheckedStatePath,
    CheckedStatement, FunctionId, expression_children,
};
use super::places::{
    CaptureId, CapturedRange, CapturedValue, PlaceMap, PlaceRoot, PlaceStep, ResolvedPlace,
    SeparationOracle, UnprovedSeparations, named_place, places_overlap, range_separation_candidate,
};
use crate::NodePath;

/// The declared effect row of one concrete function, as P reads it. This is
/// the callable boundary only: no body fact enters.
#[derive(Clone, Debug, Default)]
pub(crate) struct PermissionSignature {
    pub(crate) name: String,
    pub(crate) parameter_declarations: Vec<crate::DeclarationId>,
    pub(crate) parameter_modes: Vec<CheckedMode>,
    pub(crate) reads: Vec<CheckedStatePath>,
    pub(crate) writes: Vec<CheckedStatePath>,
}

/// One bounded [PAR-1] range question, interpreted in the flow state before
/// `first` executes and consumed only by this exact ordered statement pair.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PermissionSeparationQuery {
    pub(crate) first: NodePath,
    pub(crate) second: NodePath,
    pub(crate) left: CapturedRange,
    pub(crate) right: CapturedRange,
    /// Those of `left` and `right` that one of the pair's two calls forms as
    /// an actual. Such a range has no image in the state before `first`,
    /// because its formation runs with its call, so the flow evaluates these
    /// endpoints in that state and judges the range exactly as it judges one
    /// bound immediately before `first` [PAR-1, REF-4].
    pub(crate) formations: Vec<PermissionRangeFormation>,
}

/// A range one statement forms as an actual of its own call [REF-4]: the
/// captured pair its footprint's range step carries and the two endpoint
/// atoms that formation evaluates when its call's actuals are evaluated.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PermissionRangeFormation {
    /// The formation node, which a range image names as its source.
    pub(crate) carrier: NodePath,
    pub(crate) captured: CapturedRange,
    pub(crate) start: CheckedExpression,
    pub(crate) end: CheckedExpression,
}

/// The retained optional proof of one pair-scoped range question.
///
/// A false result is a permission denial, never a source rejection. Every
/// successful visit has its own retained root; a query reached more than once
/// is discharged only when every visit proves it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PermissionSeparationProof {
    pub(crate) query: PermissionSeparationQuery,
    pub(crate) discharged: bool,
    pub(crate) derivations: Vec<super::entailment::DerivationId>,
}

/// Which statement of an analyzed adjacency a denial cites.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PairSide {
    First,
    Second,
}

/// One exit edge of a statement that does not reach the statement's ordinary
/// successor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExitKind {
    /// A `propagate` right-hand side's `Err` edge to the function-return
    /// sink [ERR-3].
    PropagateError,
    /// A `return`, `give`, or `break` edge, which leaves the enclosing block
    /// or function without reaching the next statement.
    BlockExit,
}

/// One footprint element: a resolved caller place and the source node that
/// cites it.
///
/// There is no second shape. An arena region was v0.59's one footprint
/// element with no place of its own; [STOR-8] gives allocation and release no
/// effect entry and [PAR-1] states that they "contribute no path", so the
/// whole allocation half of the footprint is gone rather than retargeted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Access {
    pub(crate) place: ResolvedPlace,
    /// The actual or statement node the element is cited at.
    pub(crate) argument: NodePath,
}

/// One half of a statement's [PAR-1] footprint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FootprintHalf {
    Write,
    Read,
    OperandRead,
}

impl FootprintHalf {
    const fn name(self) -> &'static str {
        match self {
            Self::Write => "write",
            Self::Read => "read",
            Self::OperandRead => "operand read",
        }
    }
}

/// Which two footprint halves a conflict joins, named from the earlier and
/// the later statement, so a denial states the access it actually found.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ConflictKind {
    pub(crate) earlier: FootprintHalf,
    pub(crate) later: FootprintHalf,
}

impl ConflictKind {
    const fn new(earlier: FootprintHalf, later: FootprintHalf) -> Self {
        Self { earlier, later }
    }

    /// The two footprint halves this conflict joins, earlier first, as the
    /// ledger names them.
    pub(crate) const fn halves(self) -> (&'static str, &'static str) {
        (self.earlier.name(), self.later.name())
    }
}

/// Why P does not hold for one analyzed adjacency.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Denial {
    /// Two accesses of the two footprints overlap under [OWN-7].
    Footprint {
        kind: ConflictKind,
        left: Access,
        right: Access,
        /// The two statements the accesses belong to, earlier first.
        sides: (PairSide, PairSide),
    },
    /// Fail-closed: a footprint element whose caller place this analysis does
    /// not resolve overlaps every place [PAR-1].
    UnresolvedFootprint { side: PairSide, argument: NodePath },
    /// Fail-closed: a statement form whose footprint this analysis does not
    /// compute. Such a statement would otherwise contribute nothing and widen
    /// permission.
    UnclassifiedForm {
        side: PairSide,
        /// The form, as the ledger names it to the writer.
        form: &'static str,
    },
    /// A statement whose continuation may leave the block, so the statement
    /// written after it need not execute at all.
    SkippingExit { side: PairSide, kind: ExitKind },
}

impl Denial {
    /// The judgment clause this denial cites. The permission ledger prints it
    /// and the judgment tests assert it; acceptance never reads it.
    pub(crate) const fn condition(&self) -> u8 {
        match self {
            Self::Footprint { .. }
            | Self::UnresolvedFootprint { .. }
            | Self::UnclassifiedForm { .. } => 1,
            Self::SkippingExit { .. } => 2,
        }
    }
}

/// The judgment's outcome for one analyzed adjacency.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PermissionVerdict {
    /// P holds, so the two statements may be overlapped.
    PermittedEligible,
    Denied(Denial),
}

impl PermissionVerdict {
    pub(crate) const fn is_eligible(&self) -> bool {
        matches!(self, Self::PermittedEligible)
    }

    /// The cited clause of a denial, or `None` for a permitted verdict.
    #[allow(dead_code)]
    pub(crate) const fn denied_condition(&self) -> Option<u8> {
        match self {
            Self::Denied(denial) => Some(denial.condition()),
            Self::PermittedEligible => None,
        }
    }
}

/// One analyzed statement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PermissionSite {
    /// The statement node.
    pub(crate) statement: NodePath,
    /// The binding the statement defines, or `None` where it defines none.
    pub(crate) binding: Option<BindingId>,
    /// The named-function call this statement's right-hand side is, where it
    /// is one. Only a call-rooted member adds a hand-out; an ordinary
    /// statement is a member of the overlap group and carries none
    /// [parallel-lowering/two-worlds].
    pub(crate) call: Option<NodePath>,
    /// The callee's name for a call member, or the statement's form for any
    /// other member. The ledger prints it.
    pub(crate) callee_name: String,
}

/// One ordered pair of adjacent statements and its verdict.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PermissionPair {
    pub(crate) first: PermissionSite,
    pub(crate) second: PermissionSite,
    pub(crate) verdict: PermissionVerdict,
}

/// A maximal run of at least two adjacent statements that may all overlap.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PermissionRun {
    pub(crate) sites: Vec<PermissionSite>,
}

/// Every analyzed pair and run of one concrete function, in source order.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct FunctionPermissions {
    pub(crate) function: String,
    pub(crate) pairs: Vec<PermissionPair>,
    pub(crate) runs: Vec<PermissionRun>,
    /// The [PAR-2] verdict of every counted loop of this function, in source
    /// order.
    pub(crate) loops: Vec<LoopPermission>,
}

/// The whole-program permission table, dense by [`FunctionId`].
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct PermissionMetadata {
    pub(crate) functions: Vec<FunctionPermissions>,
}

impl PermissionMetadata {
    /// The table of one concrete function by its dense identity.
    pub(crate) fn of(&self, function: FunctionId) -> Option<&FunctionPermissions> {
        self.functions.get(function.0 as usize)
    }

    /// The table of one concrete function by its source name. Ledger and test
    /// convenience; the dense index is the identity.
    #[allow(dead_code)]
    pub(crate) fn named(&self, name: &str) -> Option<&FunctionPermissions> {
        self.functions
            .iter()
            .find(|entry| entry.function.as_str() == name)
    }
}

/// Runs P over every concrete function of one checked program.
pub(crate) fn analyze_permission(
    functions: &[CheckedFunction],
    signatures: &[PermissionSignature],
) -> PermissionMetadata {
    let program = Program { signatures };
    PermissionMetadata {
        functions: functions
            .iter()
            .map(|function| program.analyze_function(function))
            .collect(),
    }
}

/// Plans the finite proof-carrying range comparisons before entailment walks
/// the function. The planner uses the same classified footprints as the final
/// permission judgment and preserves every resolved alternative. It merely
/// identifies questions; absence of a later proof remains ordinary overlap.
pub(crate) fn plan_permission_separations(
    function: &CheckedFunction,
    signatures: &[PermissionSignature],
) -> Vec<PermissionSeparationQuery> {
    let program = Program { signatures };
    program.plan_function_separations(function)
}

pub(super) struct Program<'check> {
    signatures: &'check [PermissionSignature],
}

/// One call occurrence, reduced to what the [EFF-5] substitution reads.
pub(super) struct CallProjection<'check> {
    pub(super) formal_effects: Option<&'check CheckedEffects>,
    pub(super) call: &'check NodePath,
    pub(super) target: FunctionId,
    pub(super) arguments: &'check [CheckedExpression],
    pub(super) argument_nodes: &'check [NodePath],
}

/// The call one expression is, or `None` for every other expression form.
pub(super) fn call_projection(value: &CheckedExpression) -> Option<CallProjection<'_>> {
    match value {
        CheckedExpression::UserCall {
            function,
            call,
            argument_nodes,
            arguments,
            formal_effects,
            ..
        } => Some(CallProjection {
            formal_effects: formal_effects.as_deref(),
            call,
            target: *function,
            arguments,
            argument_nodes,
        }),
        _ => None,
    }
}

/// One statement of a block, classified once for every adjacency it takes
/// part in.
///
/// A call-rooted match uses its scrutinee call's node as its site. Other
/// statements without a node of their own remain unreported run boundaries.
struct Classified {
    site: Option<PermissionSite>,
    footprint: Result<Footprint, Refusal>,
}

/// Why one statement cannot take part in an overlap as written.
#[derive(Clone, Copy)]
enum Refusal {
    /// It carries an exit edge.
    Exit(ExitKind),
    /// Its footprint is not computed for this form.
    Form(&'static str),
}

impl<'check> Program<'check> {
    fn plan_function_separations(
        &self,
        function: &'check CheckedFunction,
    ) -> Vec<PermissionSeparationQuery> {
        let places = PlaceMap::for_function(function);
        let mut queries = Vec::new();
        let mut blocks = vec![function.body.as_deref().unwrap_or_default()];
        while let Some(block) = blocks.pop() {
            let classified = block
                .iter()
                .map(|statement| self.classify(&places, statement))
                .collect::<Vec<_>>();
            let formations = block.iter().map(call_range_formations).collect::<Vec<_>>();
            for (first_index, first) in classified.iter().enumerate() {
                let (Some(first_site), Ok(first_footprint)) = (&first.site, &first.footprint)
                else {
                    continue;
                };
                if first_footprint.unresolved.is_some() {
                    continue;
                }
                for (second_index, second) in classified.iter().enumerate().skip(first_index + 1) {
                    let (Some(second_site), Ok(second_footprint)) =
                        (&second.site, &second.footprint)
                    else {
                        // A form that carries no complete straight-line
                        // footprint ends every run through it.
                        break;
                    };
                    if second_footprint.unresolved.is_some() {
                        break;
                    }
                    let evaluable = formations_before_first(
                        &places,
                        &classified[first_index..second_index],
                        &formations[first_index],
                        &formations[second_index],
                    );
                    collect_footprint_range_queries(
                        (first_site, first_footprint),
                        (second_site, second_footprint),
                        &evaluable,
                        &mut queries,
                    );
                }
            }
            for statement in block {
                push_nested_blocks(statement, &mut blocks);
            }
        }
        queries.sort_by(|left, right| {
            left.first
                .components()
                .cmp(right.first.components())
                .then(left.second.components().cmp(right.second.components()))
                .then(left.left.cmp(&right.left))
                .then(left.right.cmp(&right.right))
        });
        queries.dedup();
        queries
    }

    fn analyze_function(&self, function: &'check CheckedFunction) -> FunctionPermissions {
        let places = PlaceMap::for_function(function);
        let mut permissions = FunctionPermissions {
            function: function.name.clone(),
            pairs: Vec::new(),
            runs: Vec::new(),
            loops: Vec::new(),
        };
        let mut blocks = vec![function.body.as_deref().unwrap_or_default()];
        while let Some(block) = blocks.pop() {
            self.analyze_block(
                &places,
                block,
                &function.entailment.permission_separations,
                &mut permissions,
            );
            for statement in block {
                push_nested_blocks(statement, &mut blocks);
            }
        }
        permissions.pairs.sort_by(|left, right| {
            left.first
                .statement
                .components()
                .cmp(right.first.statement.components())
        });
        permissions.runs.sort_by(|left, right| {
            left.sites[0]
                .statement
                .components()
                .cmp(right.sites[0].statement.components())
        });
        // The loop judgment runs last and reads the finished verdicts, so a
        // loop that already holds an eligible adjacency is never additionally
        // told to become one. Its own verdict does not read them: [PAR-2] is
        // a judgment of the loop, not of what a writer could put inside it.
        let eligible = permissions
            .pairs
            .iter()
            .filter(|pair| pair.verdict.is_eligible())
            .map(|pair| pair.first.statement.clone())
            .collect::<Vec<_>>();
        permissions.loops = super::loop_permission::judge_loops(self, &places, function, &eligible);
        permissions
    }

    /// Judges every adjacent pair of one block and collects its runs.
    ///
    /// Every statement is classified and every footprint projected a single
    /// time per block, because a block of n statements is read n-1 times as
    /// an adjacency and once more by the run walk.
    fn analyze_block(
        &self,
        places: &PlaceMap,
        block: &'check [CheckedStatement],
        proofs: &[PermissionSeparationProof],
        permissions: &mut FunctionPermissions,
    ) {
        if block.len() < 2 {
            return;
        }
        let classified = block
            .iter()
            .map(|statement| self.classify(places, statement))
            .collect::<Vec<_>>();
        for window in classified.windows(2) {
            let [first, second] = window else {
                continue;
            };
            // The ledger reports adjacencies a writer can act on. A pair of
            // two statements neither of which holds a call has no hand-out to
            // gain and would bury the lines that do, so it is judged for the
            // run walk below and not reported.
            let (Some(first_site), Some(second_site)) = (&first.site, &second.site) else {
                continue;
            };
            if first_site.call.is_none() && second_site.call.is_none() {
                continue;
            }
            permissions.pairs.push(PermissionPair {
                first: first_site.clone(),
                second: second_site.clone(),
                verdict: self.judge(first, second, proofs),
            });
        }
        self.collect_runs(&classified, proofs, permissions);
    }

    /// The verdict of one ordered adjacency.
    fn judge(
        &self,
        first: &Classified,
        second: &Classified,
        proofs: &[PermissionSeparationProof],
    ) -> PermissionVerdict {
        for (side, classified) in [(PairSide::First, first), (PairSide::Second, second)] {
            match &classified.footprint {
                Err(Refusal::Exit(kind)) => {
                    return PermissionVerdict::Denied(Denial::SkippingExit { side, kind: *kind });
                }
                Err(Refusal::Form(form)) => {
                    return PermissionVerdict::Denied(Denial::UnclassifiedForm { side, form });
                }
                Ok(footprint) => {
                    if let Some(argument) = &footprint.unresolved {
                        return PermissionVerdict::Denied(Denial::UnresolvedFootprint {
                            side,
                            argument: argument.clone(),
                        });
                    }
                }
            }
        }
        let (left, right) = match (&first.footprint, &second.footprint) {
            (Ok(left), Ok(right)) => (left, right),
            // Every refusal already returned a denial above. This arm keeps
            // the match total without a panic in a judgment that is allowed
            // to deny and never allowed to fail.
            _ => {
                return PermissionVerdict::Denied(Denial::UnclassifiedForm {
                    side: PairSide::First,
                    form: "a statement form this judgment does not compute",
                });
            }
        };
        let (Some(first_site), Some(second_site)) = (&first.site, &second.site) else {
            return PermissionVerdict::Denied(Denial::UnclassifiedForm {
                side: PairSide::First,
                form: "a statement form this judgment does not compute",
            });
        };
        let oracle = PairSeparationOracle {
            first: &first_site.statement,
            second: &second_site.statement,
            proofs,
            before_second: vec![left],
        };
        match footprint_conflict(&oracle, left, right) {
            Some(denial) => PermissionVerdict::Denied(denial),
            None => PermissionVerdict::PermittedEligible,
        }
    }

    /// Grows maximal runs by checking every source-ordered member pair.
    ///
    /// A statement joins the current run only when its pair-local oracle
    /// separates it from every earlier member. This is the literal [PAR-1]
    /// composition rule and preserves the first-statement proof context that
    /// an origin-free union would erase.
    fn collect_runs(
        &self,
        classified: &[Classified],
        proofs: &[PermissionSeparationProof],
        permissions: &mut FunctionPermissions,
    ) {
        let mut run: Vec<usize> = Vec::new();
        let mut flush = |run: &mut Vec<usize>| {
            let sites = run
                .iter()
                .filter_map(|index| classified[*index].site.clone())
                .collect::<Vec<_>>();
            // Every form that is given a footprint carries a node of its own,
            // so the filter above removes nothing; the length is read after
            // it so that a form which later loses its node cannot shorten a
            // run behind the judgment's back.
            if sites.len() >= 2 && sites.len() == run.len() {
                permissions.runs.push(PermissionRun { sites });
            }
            run.clear();
        };
        for (index, statement) in classified.iter().enumerate() {
            let Ok(footprint) = &statement.footprint else {
                // An exit-bearing or unclassified statement joins no run and
                // ends the one before it: the statements after it are not
                // reached from the same straight-line edge.
                flush(&mut run);
                continue;
            };
            if footprint.unresolved.is_some() {
                flush(&mut run);
                continue;
            }
            if run.is_empty() {
                run.push(index);
                continue;
            }
            let conflicts = run.iter().any(|earlier| {
                let first = &classified[*earlier];
                let (Some(first_site), Some(second_site), Ok(first_footprint)) =
                    (&first.site, &statement.site, &first.footprint)
                else {
                    return true;
                };
                // A run is contiguous, so the statements between this pair
                // are the run members between them, each with a footprint.
                let oracle = PairSeparationOracle {
                    first: &first_site.statement,
                    second: &second_site.statement,
                    proofs,
                    before_second: classified[*earlier..index]
                        .iter()
                        .filter_map(|member| member.footprint.as_ref().ok())
                        .collect(),
                };
                footprint_conflict(&oracle, first_footprint, footprint).is_some()
            });
            if conflicts {
                flush(&mut run);
                run.push(index);
            } else {
                run.push(index);
            }
        }
        flush(&mut run);
    }

    /// One statement, reduced to what [PAR-1] judges, or the reason it cannot
    /// be.
    ///
    /// The match is exhaustive on purpose: a statement form this analysis did
    /// not classify would contribute an empty footprint and *widen*
    /// permission. Every form is either given a footprint here or refused
    /// here.
    fn classify(&self, places: &PlaceMap, statement: &'check CheckedStatement) -> Classified {
        let (node, binding, call, label, footprint) = match statement {
            // A source proof is checked before permission and erased before
            // lowering: no runtime evaluation, effect, exit edge, or
            // scheduler-visible event, so its footprint is empty and it joins
            // a run without changing it.
            CheckedStatement::Proof(proof) => (
                Some(&proof.node_path),
                None,
                None,
                "a proof statement",
                Ok(Footprint::default()),
            ),
            CheckedStatement::Let {
                node_path,
                binding,
                value,
            } => {
                let mut footprint = self.value_footprint(places, value, node_path);
                // "a `let`'s defined binding is a write path" [PAR-1]. This
                // is what makes reading what an earlier statement defines a
                // footprint conflict rather than a rule of its own.
                footprint.writes.push(Access {
                    place: ResolvedPlace::binding(*binding),
                    argument: node_path.clone(),
                });
                let projection = call_projection(value);
                let label = projection
                    .as_ref()
                    .map_or("a let statement", |_| "a call statement");
                let call = projection.map(|projection| projection.call.clone());
                (Some(node_path), Some(*binding), call, label, Ok(footprint))
            }
            CheckedStatement::Set {
                node_path,
                target,
                value,
                ..
            } => {
                let mut footprint = self.value_footprint(places, value, node_path);
                set_target_place(places, target, node_path, &mut footprint);
                (
                    Some(node_path),
                    None,
                    None,
                    "a set statement",
                    Ok(footprint),
                )
            }
            // Exit-bearing forms.
            CheckedStatement::PropagateLet { node_path, .. } => (
                Some(node_path),
                None,
                None,
                "a propagate statement",
                Err(Refusal::Exit(ExitKind::PropagateError)),
            ),
            CheckedStatement::Return { node_path, .. } => (
                Some(node_path),
                None,
                None,
                "a return statement",
                Err(Refusal::Exit(ExitKind::BlockExit)),
            ),
            CheckedStatement::Give { node_path, .. } => (
                Some(node_path),
                None,
                None,
                "a give statement",
                Err(Refusal::Exit(ExitKind::BlockExit)),
            ),
            CheckedStatement::Break { .. } => (
                None,
                None,
                None,
                "a break statement",
                Err(Refusal::Exit(ExitKind::BlockExit)),
            ),
            // A call-rooted match can be the last actualized member: its
            // call joins before dispatch. PAR-1 still judges the complete
            // statement, so include every arm's footprint, not only the
            // condition's call. An unclassified or exiting arm fails closed.
            CheckedStatement::Match {
                scrutinee: value @ CheckedExpression::UserCall { call, .. },
                arms,
                ..
            } => {
                let mut footprint = self.value_footprint(places, value, call);
                let mut result = Ok(());
                for arm in arms {
                    for child in &arm.body {
                        match self.classify(places, child).footprint {
                            Ok(child) => footprint.absorb(&child),
                            Err(refusal) => result = Err(refusal),
                        }
                    }
                }
                (
                    Some(call),
                    None,
                    Some(call.clone()),
                    "a call-rooted match",
                    result.map(|()| footprint),
                )
            }
            CheckedStatement::Match { .. } => (
                None,
                None,
                None,
                "a match statement",
                Err(Refusal::Form("a match statement")),
            ),
            CheckedStatement::ValueMatchLet { node_path, .. } => (
                Some(node_path),
                None,
                None,
                "a value match or value if",
                Err(Refusal::Form("a value match or value if")),
            ),
            CheckedStatement::Loop { .. } => {
                (None, None, None, "a loop", Err(Refusal::Form("a loop")))
            }
            CheckedStatement::CountedRange { node_path, .. } => (
                Some(node_path),
                None,
                None,
                "a for loop",
                Err(Refusal::Form("a for loop")),
            ),
            // [CALL-4] a binder list defines more than one place in one
            // statement, and this judgment describes one definition per
            // statement.
            CheckedStatement::DestructuringLet { node_path, .. } => (
                Some(node_path),
                None,
                None,
                "a statement that binds an ordered result list",
                Err(Refusal::Form(
                    "a statement that binds an ordered result list",
                )),
            ),
            // [GRAM-4] an expression statement is one call whose result is
            // discarded. Its footprint is that call's, formed exactly as a
            // `let` right-hand side's is: the substituted row [EFF-5], its
            // operand reads, and its by-value consumptions. It defines no
            // binding, so it has no binding write path, and the release a
            // discarded affine result runs contributes no path [STOR-8].
            CheckedStatement::Evaluate { node_path, value }
            | CheckedStatement::DropExpression {
                node_path, value, ..
            } => {
                let footprint = self.value_footprint(places, value, node_path);
                let projection = call_projection(value);
                let label = projection
                    .as_ref()
                    .map_or("an expression statement", |_| "a call statement");
                let call = projection.map(|projection| projection.call.clone());
                (Some(node_path), None, call, label, Ok(footprint))
            }
        };
        let callee_name = statement_value(statement)
            .and_then(call_projection)
            .and_then(|projection| {
                self.signatures
                    .get(projection.target.0 as usize)
                    .map(|signature| signature.name.clone())
            })
            .unwrap_or_else(|| label.to_owned());
        Classified {
            site: node.cloned().map(|statement| PermissionSite {
                statement,
                binding,
                call,
                callee_name,
            }),
            footprint,
        }
    }

    /// The footprint of one statement's right-hand side: its own operand
    /// reads, the places its by-value consumptions empty, and, where the
    /// value is a call, that call's substituted row.
    fn value_footprint(
        &self,
        places: &PlaceMap,
        value: &CheckedExpression,
        node: &NodePath,
    ) -> Footprint {
        let mut footprint = Footprint::default();
        if let Some(projection) = call_projection(value) {
            self.call_footprint(places, &projection, &mut footprint);
            return footprint;
        }
        collect_consumed_places(places, value, node, &mut footprint);
        collect_operand_reads(places, value, node, &mut footprint);
        footprint
    }

    /// The written and read footprints of one call, by [EFF-5] substitution
    /// of the callee's declared row onto the actuals' resolved places.
    pub(super) fn footprint(&self, places: &PlaceMap, call: &CallProjection<'_>) -> Footprint {
        let mut footprint = Footprint::default();
        self.call_footprint(places, call, &mut footprint);
        footprint
    }

    fn call_footprint(
        &self,
        places: &PlaceMap,
        call: &CallProjection<'_>,
        footprint: &mut Footprint,
    ) {
        let Some(signature) = self.signatures.get(call.target.0 as usize) else {
            footprint.unresolved = Some(call.call.clone());
            return;
        };

        // "A by-value consumption counts as a write of the argument's place"
        // [PAR-1]. The affine discipline already forbids two consumers of one
        // place; the footprint states it rather than assuming it.
        for (index, mode) in signature.parameter_modes.iter().enumerate() {
            if !matches!(mode, CheckedMode::Own) {
                continue;
            }
            let Some(argument) = call.arguments.get(index) else {
                footprint.unresolved = Some(call.call.clone());
                return;
            };
            let node = call.argument_nodes.get(index).unwrap_or(call.call);
            if !consumes_root(argument) {
                continue;
            }
            match argument_places(places, argument) {
                Some(places) => footprint
                    .writes
                    .extend(places.into_iter().map(|place| Access {
                        place,
                        argument: node.clone(),
                    })),
                None => footprint.unresolved = Some(node.clone()),
            }
        }

        let effects = call.formal_effects;
        let reads = effects.map_or(&signature.reads, |effects| &effects.reads);
        let writes = effects.map_or(&signature.writes, |effects| &effects.writes);
        for (written, declared) in [(false, reads), (true, writes)] {
            for path in declared {
                let Some(index) = signature
                    .parameter_declarations
                    .iter()
                    .position(|declaration| *declaration == path.root)
                else {
                    footprint.unresolved = Some(call.call.clone());
                    continue;
                };
                let (Some(argument), Some(node)) =
                    (call.arguments.get(index), call.argument_nodes.get(index))
                else {
                    footprint.unresolved = Some(call.call.clone());
                    continue;
                };
                let Some(roots) = argument_places(places, argument) else {
                    footprint.unresolved = Some(node.clone());
                    continue;
                };
                for mut place in roots {
                    place.path.extend(substituted_steps(path));
                    let access = Access {
                        place,
                        argument: node.clone(),
                    };
                    if written {
                        footprint.writes.push(access);
                    } else {
                        footprint.reads.push(access);
                    }
                }
            }
        }

        // The caller-side half: what this statement's own operand evaluation
        // touches before the call.
        for (index, argument) in call.arguments.iter().enumerate() {
            let node = call.argument_nodes.get(index).unwrap_or(call.call);
            collect_operand_reads(places, argument, node, footprint);
        }
    }
}

/// The statement's right-hand side, for the one classification that needs to
/// read the callee's name back out of it.
fn statement_value(statement: &CheckedStatement) -> Option<&CheckedExpression> {
    match statement {
        CheckedStatement::Let { value, .. }
        | CheckedStatement::Evaluate { value, .. }
        | CheckedStatement::DropExpression { value, .. } => Some(value),
        CheckedStatement::Match { scrutinee, .. } => Some(scrutinee),
        _ => None,
    }
}

/// The resolved steps of one declared [EFF-1] path below its substituted root.
///
/// An index or range position of a signature names a value parameter of the
/// same callable, and [EFF-5] replaces it with the value that parameter's own
/// argument supplies. This analysis does not hold the call's argument values,
/// so such a position becomes an unknown captured value, which no admitted
/// family separates [checker-facts]. That is conservative in the direction
/// permission must fail in: an unknown index overlaps every index.
fn substituted_steps(path: &CheckedStatePath) -> Vec<PlaceStep> {
    path.steps
        .iter()
        .map(|step| match step {
            super::model::CheckedEffectStep::Field(field) => PlaceStep::Field(*field),
            super::model::CheckedEffectStep::Deref => PlaceStep::Deref,
            super::model::CheckedEffectStep::Payload { variant, field } => PlaceStep::Payload {
                variant: *variant,
                field: *field,
            },
            super::model::CheckedEffectStep::Index(_) => PlaceStep::Index(CapturedValue::unknown()),
            super::model::CheckedEffectStep::Range { .. } => PlaceStep::Range(CapturedRange {
                start: CapturedValue::unknown(),
                end: CapturedValue::unknown(),
            }),
            super::model::CheckedEffectStep::Part(part) => PlaceStep::Part(*part),
            super::model::CheckedEffectStep::Measure(measure) => PlaceStep::Measure(*measure),
        })
        .collect()
}

#[derive(Clone, Debug, Default)]
pub(super) struct Footprint {
    pub(super) writes: Vec<Access>,
    pub(super) reads: Vec<Access>,
    /// Storage this statement's own operand expressions read on the calling
    /// thread, before the call. [PAR-1] judges this half in both directions,
    /// because which statement's operand evaluation an overlap moves is the
    /// implementation's choice of lane.
    pub(super) operand_reads: Vec<Access>,
    /// Set where a footprint element's caller place is not resolved. Every
    /// such statement denies [PAR-1].
    pub(super) unresolved: Option<NodePath>,
}

impl Footprint {
    /// Adds one statement's halves to a running union [checker-facts].
    fn absorb(&mut self, other: &Self) {
        self.writes.extend(other.writes.iter().cloned());
        self.reads.extend(other.reads.iter().cloned());
        self.operand_reads
            .extend(other.operand_reads.iter().cloned());
        if let Some(unresolved) = &other.unresolved {
            self.unresolved.get_or_insert_with(|| unresolved.clone());
        }
    }

    /// Every place this footprint reads, in either half.
    fn read_halves(&self) -> impl Iterator<Item = (FootprintHalf, &Access)> {
        self.reads
            .iter()
            .map(|access| (FootprintHalf::Read, access))
            .chain(
                self.operand_reads
                    .iter()
                    .map(|access| (FootprintHalf::OperandRead, access)),
            )
    }
}

fn canonical_range_pair(
    left: CapturedRange,
    right: CapturedRange,
) -> (CapturedRange, CapturedRange) {
    if left <= right {
        (left, right)
    } else {
        (right, left)
    }
}

fn push_range_query(
    (first, second): (&PermissionSite, &PermissionSite),
    (left, right): (&ResolvedPlace, &ResolvedPlace),
    evaluable: &[&PermissionRangeFormation],
    queries: &mut Vec<PermissionSeparationQuery>,
) {
    if !places_overlap(&UnprovedSeparations, left, right) {
        return;
    }
    let Some((left, right)) = range_separation_candidate(left, right) else {
        return;
    };
    let (left, right) = canonical_range_pair(left, right);
    let formations = evaluable
        .iter()
        .filter(|formation| formation.captured == left || formation.captured == right)
        .map(|formation| (*formation).clone())
        .collect();
    queries.push(PermissionSeparationQuery {
        first: first.statement.clone(),
        second: second.statement.clone(),
        left,
        right,
        formations,
    });
}

/// Collects every proof-shaped conflict of one ordered statement pair. A
/// multi-target pair needs every such query: finding one separated access
/// never hides a later overlapping access.
fn collect_footprint_range_queries(
    (first, earlier): (&PermissionSite, &Footprint),
    (second, later): (&PermissionSite, &Footprint),
    evaluable: &[&PermissionRangeFormation],
    queries: &mut Vec<PermissionSeparationQuery>,
) {
    for write in &earlier.writes {
        for access in later
            .writes
            .iter()
            .chain(later.reads.iter())
            .chain(&later.operand_reads)
        {
            push_range_query(
                (first, second),
                (&write.place, &access.place),
                evaluable,
                queries,
            );
        }
    }
    for write in &later.writes {
        for access in earlier.reads.iter().chain(&earlier.operand_reads) {
            push_range_query(
                (first, second),
                (&write.place, &access.place),
                evaluable,
                queries,
            );
        }
    }
}

/// The ranges one statement forms as actuals of the call it evaluates on
/// entry [REF-4]: a `let`, `set` or expression statement's call, or a
/// call-rooted match's scrutinee call. A range formed inside a match arm is
/// formed after that statement's entry and is not one of them.
///
/// The flow's range images are named by the start endpoint's capture, and
/// every range endpoint carries the source occurrence that evaluated it, so
/// every formation is listed; the filter only keeps a capture that names no
/// single formation from ever reaching the flow [OWN-7].
fn call_range_formations(statement: &CheckedStatement) -> Vec<PermissionRangeFormation> {
    let value = match statement {
        CheckedStatement::Let { value, .. }
        | CheckedStatement::Set { value, .. }
        | CheckedStatement::Evaluate { value, .. }
        | CheckedStatement::DropExpression { value, .. }
        | CheckedStatement::Match {
            scrutinee: value, ..
        } => value,
        _ => return Vec::new(),
    };
    let Some(call) = call_projection(value) else {
        return Vec::new();
    };
    call.arguments
        .iter()
        .filter_map(|argument| match argument {
            CheckedExpression::RangeOf {
                carrier,
                start,
                end,
                captured,
                ..
            } if matches!(captured.start.capture, CaptureId::Source(_)) => {
                Some(PermissionRangeFormation {
                    carrier: carrier.clone(),
                    captured: *captured,
                    start: start.as_ref().clone(),
                    end: end.as_ref().clone(),
                })
            }
            _ => None,
        })
        .collect()
}

/// The pair's call formations whose endpoints hold, in the state before the
/// first statement, the values their formation reads [PAR-1].
///
/// "The paths of both statements are interpreted in the state before the
/// first statement." The first statement forms its ranges from that state.
/// The second forms its ranges only after `before_second` — the first
/// statement and every statement up to the second — has run, so its endpoint
/// values are the values of that earlier state only when none of those
/// statements writes what the endpoint expressions read. An endpoint that the
/// first statement defines, or any earlier write reaches, therefore has no
/// value there, and its formation is not evaluated before the first.
fn formations_before_first<'formation>(
    places: &PlaceMap,
    before_second: &[Classified],
    first: &'formation [PermissionRangeFormation],
    second: &'formation [PermissionRangeFormation],
) -> Vec<&'formation PermissionRangeFormation> {
    let unwritten = |formation: &PermissionRangeFormation| {
        let mut endpoints = Footprint::default();
        for endpoint in [&formation.start, &formation.end] {
            collect_operand_reads(places, endpoint, &formation.carrier, &mut endpoints);
        }
        endpoints.unresolved.is_none()
            && before_second
                .iter()
                .map(|statement| statement.footprint.as_ref().ok())
                .all(|footprint| {
                    footprint.is_some_and(|footprint| {
                        footprint.unresolved.is_none()
                            && footprint.writes.iter().all(|write| {
                                endpoints.operand_reads.iter().all(|read| {
                                    !places_overlap(&UnprovedSeparations, &write.place, &read.place)
                                })
                            })
                    })
                })
    };
    first
        .iter()
        .chain(second.iter().filter(|formation| unwritten(formation)))
        .collect()
}

struct PairSeparationOracle<'proof> {
    first: &'proof NodePath,
    second: &'proof NodePath,
    proofs: &'proof [PermissionSeparationProof],
    /// The footprints of the first statement and of every statement between
    /// the two: what runs from the state both paths are interpreted in up to
    /// the second statement's own entry [PAR-1].
    before_second: Vec<&'proof Footprint>,
}

impl SeparationOracle for PairSeparationOracle<'_> {
    /// "The paths of both statements are interpreted in the state before the
    /// first statement; the first statement's `ensures` maps the second's
    /// indices into that state, so an index that is live only after an append
    /// is not distinct from the append slot" [PAR-1, WIN-2]. This judgment
    /// performs no such mapping, so a window whose `r.len` any statement
    /// before the second writes has no length the two paths share.
    fn window_length_is_shared(&self, window: &ResolvedPlace) -> bool {
        let mut path = window.path.clone();
        path.push(PlaceStep::Measure(CheckedMeasure::Length));
        let length = ResolvedPlace {
            root: window.root,
            path,
        };
        !self
            .before_second
            .iter()
            .flat_map(|footprint| &footprint.writes)
            .any(|write| places_overlap(&UnprovedSeparations, &write.place, &length))
    }

    fn indices_distinct(&self, _left: CapturedValue, _right: CapturedValue) -> bool {
        false
    }

    fn ranges_disjoint(&self, left: CapturedRange, right: CapturedRange) -> bool {
        let (left, right) = canonical_range_pair(left, right);
        self.proofs.iter().any(|proof| {
            proof.discharged
                && proof.query.first == *self.first
                && proof.query.second == *self.second
                && proof.query.left == left
                && proof.query.right == right
        })
    }

    /// A subscript either statement forms is live in the state before the
    /// first [WIN-2]: the first's discharged [OP-4] there, and the second's
    /// discharged it against the same `r.len` wherever the guard above lets
    /// this question be asked. An index an effect row supplies is substituted
    /// as an unknown value here [EFF-5], as is an offset no captured value
    /// names, and no bound is read for either, so an unknown index is not
    /// live.
    fn index_is_live(&self, _window: &ResolvedPlace, index: CapturedValue) -> bool {
        index != CapturedValue::unknown()
    }

    /// No [PAR-1] query asks this family, so the pair overlaps, which
    /// selects the sequential lowering and is always sound.
    fn index_outside_range(&self, _index: CapturedValue, _range: CapturedRange) -> bool {
        false
    }

    /// A range either statement forms lies within the length of the state
    /// before the first, as a subscript either forms is live there [WIN-2]:
    /// its [REF-4] bound `hi <= r.len` was discharged against that length. An
    /// endpoint an effect row supplies is an unknown value here [EFF-5].
    fn range_within_length(&self, _window: &ResolvedPlace, range: CapturedRange) -> bool {
        range.start != CapturedValue::unknown() && range.end != CapturedValue::unknown()
    }
}

/// [PAR-1]'s disjointness clause over one ordered pair of footprints.
///
/// The earlier statement's writes are judged against every half of the later
/// one, and the later one's writes against every half of the earlier one.
/// Read/read overlap is admitted. The relation is [OWN-7]'s, asked of the
/// resolved paths through this exact statement pair's retained-proof oracle.
fn footprint_conflict(
    oracle: &dyn SeparationOracle,
    earlier: &Footprint,
    later: &Footprint,
) -> Option<Denial> {
    for write in &earlier.writes {
        for (half, access) in later
            .writes
            .iter()
            .map(|access| (FootprintHalf::Write, access))
            .chain(later.read_halves())
        {
            if places_overlap(oracle, &write.place, &access.place) {
                return Some(Denial::Footprint {
                    kind: ConflictKind::new(FootprintHalf::Write, half),
                    left: write.clone(),
                    right: access.clone(),
                    sides: (PairSide::First, PairSide::Second),
                });
            }
        }
    }
    for write in &later.writes {
        for (half, access) in earlier.read_halves() {
            if places_overlap(oracle, &write.place, &access.place) {
                return Some(Denial::Footprint {
                    kind: ConflictKind::new(half, FootprintHalf::Write),
                    left: access.clone(),
                    right: write.clone(),
                    sides: (PairSide::First, PairSide::Second),
                });
            }
        }
    }
    None
}

/// Records the storage a `set` names, and the operands its subscripts read.
pub(super) fn set_target_place(
    places: &PlaceMap,
    target: &CheckedSetTarget,
    node: &NodePath,
    footprint: &mut Footprint,
) {
    // A `set` whose whole target is a reference variable rebinds it and so
    // writes that variable [REF-1]; every other target through one reads it
    // to find the place it writes [PAR-1].
    let (holder, rebinding) = match target {
        CheckedSetTarget::Place(target) => (Some(target.binding), target.fields.is_empty()),
        CheckedSetTarget::RangeIndex(target) => (Some(target.root.binding), false),
        CheckedSetTarget::Storage(target) => (target.binding(), false),
    };
    if let Some(binding) = holder {
        if rebinding && let Some(place) = places.reference_holder(binding) {
            footprint.writes.push(Access {
                place,
                argument: node.clone(),
            });
        } else {
            push_reference_holder_read(places, binding, node, footprint);
        }
    }
    let resolved = match target {
        CheckedSetTarget::Place(target) => places.resolve(
            PlaceRoot::Binding(target.binding),
            &field_steps(&target.fields),
        ),
        // [REF-4] a range reference names one path, so the element a
        // subscript through it writes is that path extended by the index.
        CheckedSetTarget::RangeIndex(target) => {
            collect_operand_reads(places, &target.offset, node, footprint);
            for step in &target.path {
                if let CheckedPlaceStep::Subscript(index) = step {
                    collect_operand_reads(places, &index.offset, node, footprint);
                }
            }
            places.resolve(
                PlaceRoot::Binding(target.root.binding),
                &target.place_path(),
            )
        }
        CheckedSetTarget::Storage(target) => {
            for offset in target.offsets() {
                collect_operand_reads(places, offset, node, footprint);
            }
            places.resolve(target.root, &container_steps(target))
        }
    };
    if resolved.is_empty() {
        footprint.unresolved.get_or_insert(node.clone());
    }
    footprint
        .writes
        .extend(resolved.into_iter().map(|place| Access {
            place,
            argument: node.clone(),
        }));
}

pub(super) fn field_steps(fields: &[u32]) -> Vec<PlaceStep> {
    fields.iter().copied().map(PlaceStep::Field).collect()
}

/// The resolved steps of one checked storage path.
pub(super) fn container_steps(root: &super::model::CheckedContainerRoot) -> Vec<PlaceStep> {
    root.path.iter().map(CheckedPlaceStep::place_step).collect()
}

/// Every caller place one expression transfers away by consuming an `own`
/// value, recorded as a write of the storage it empties [PAR-1].
pub(super) fn collect_consumed_places(
    places: &PlaceMap,
    expression: &CheckedExpression,
    node: &NodePath,
    footprint: &mut Footprint,
) {
    if consumes_root(expression)
        && let Some(resolved) = argument_places(places, expression)
    {
        footprint
            .writes
            .extend(resolved.into_iter().map(|place| Access {
                place,
                argument: node.clone(),
            }));
    }
    for child in expression_children(expression) {
        collect_consumed_places(places, child, node, footprint);
    }
}

/// Whether this expression consumes the place it names [OWN-1].
fn consumes_root(expression: &CheckedExpression) -> bool {
    matches!(
        expression,
        CheckedExpression::Binding {
            consume_root: true,
            ..
        } | CheckedExpression::Project {
            consume_root: true,
            ..
        } | CheckedExpression::BoxTake { .. }
    )
}

/// Every block nested inside one statement, for the whole-body walk.
fn push_nested_blocks<'check>(
    statement: &'check CheckedStatement,
    blocks: &mut Vec<&'check [CheckedStatement]>,
) {
    match statement {
        CheckedStatement::Match { arms, .. } | CheckedStatement::ValueMatchLet { arms, .. } => {
            for arm in arms {
                blocks.push(arm.body.as_slice());
            }
        }
        CheckedStatement::Loop { body, .. } | CheckedStatement::CountedRange { body, .. } => {
            blocks.push(body.as_slice())
        }
        CheckedStatement::Let { .. }
        | CheckedStatement::DestructuringLet { .. }
        | CheckedStatement::PropagateLet { .. }
        | CheckedStatement::Set { .. }
        | CheckedStatement::Proof(_)
        | CheckedStatement::DropExpression { .. }
        | CheckedStatement::Evaluate { .. }
        | CheckedStatement::Return { .. }
        | CheckedStatement::Give { .. }
        | CheckedStatement::Break { .. } => {}
    }
}

/// Every binding one expression tree mentions, for the counted judgment's
/// accumulator count.
///
/// A range formation's source is left out: neither consumer asks about a
/// binding a range is formed over.
pub(super) fn visit_read_bindings(
    expression: &CheckedExpression,
    note: &mut impl FnMut(BindingId),
) {
    if !matches!(expression, CheckedExpression::RangeOf { .. })
        && let Some(binding) = named_binding(expression)
    {
        note(binding);
    }
    for child in expression_children(expression) {
        visit_read_bindings(child, note);
    }
}

/// The binding one expression's own place is written at, leaving its child
/// expressions to the caller.
///
/// The match is exhaustive so that a new expression form decides whether it
/// names a binding.
fn named_binding(expression: &CheckedExpression) -> Option<BindingId> {
    match expression {
        CheckedExpression::Binding { binding, .. }
        | CheckedExpression::Project { binding, .. }
        | CheckedExpression::BoxTake { binding, .. }
        | CheckedExpression::DerefAddressed { binding, .. } => Some(*binding),
        CheckedExpression::BorrowAddressed { root, .. }
        | CheckedExpression::ContainerMeasure { root, .. }
        | CheckedExpression::ReadStorage { root, .. } => root.binding(),
        CheckedExpression::BufferMeasure { root, .. }
        | CheckedExpression::BufferIndex { root, .. } => Some(root.binding),
        CheckedExpression::RangeMeasure { root, .. } => Some(root.binding),
        CheckedExpression::RangeElementMeasure { place, .. }
        | CheckedExpression::RangeIndex { place, .. }
        | CheckedExpression::BorrowRangeIndex { place, .. } => Some(place.root.binding),
        CheckedExpression::RangeOf { source, .. } => source.binding(),
        CheckedExpression::ArrayMeasure { root, .. }
        | CheckedExpression::ArrayIndex { root, .. } => match root {
            CheckedArrayRoot::Binding { binding, .. } => Some(*binding),
            CheckedArrayRoot::Constant(_) => None,
        },
        CheckedExpression::Constant(_)
        | CheckedExpression::NamedConstant { .. }
        | CheckedExpression::UserCall { .. }
        | CheckedExpression::IntegerOperation { .. }
        | CheckedExpression::FloatOperation { .. }
        | CheckedExpression::NumericConversion { .. }
        | CheckedExpression::Reinterpret { .. }
        | CheckedExpression::BooleanOperation { .. }
        | CheckedExpression::EnumEquality { .. }
        | CheckedExpression::BoxDeref { .. }
        | CheckedExpression::ConstructStruct { .. }
        | CheckedExpression::ConstructEnum { .. }
        | CheckedExpression::ProjectValue { .. } => None,
    }
}

/// [PAR-1] a `let`'s defined binding is a write path, and a statement that
/// uses a local reference variable reads that binding, which holds what the
/// reference's formation captured; the path the reference names is read or
/// written separately, as the use requires [REF-1].
fn push_reference_holder_read(
    places: &PlaceMap,
    binding: BindingId,
    node: &NodePath,
    footprint: &mut Footprint,
) {
    if let Some(place) = places.reference_holder(binding) {
        footprint.operand_reads.push(Access {
            place,
            argument: node.clone(),
        });
    }
}

/// Every caller place one operand expression reads on the calling thread,
/// with an unresolved read failing closed.
///
/// This is the storage the *caller* touches while building an actual: a value
/// read out of a binding, a field, a `deref` [TYPE-7], a subscript. Forming a
/// reference names a path and reads no content beyond its own index and
/// endpoint atoms [REF-1, REF-4], so it contributes nothing here — the
/// callee's declared row already covers whatever it reaches through that
/// reference. Naming a local reference variable, to pass it, read through
/// it or form a range from it, reads that variable's own binding as well,
/// which the `let` that formed it or a `set` that rebinds it writes [PAR-1].
///
/// The match is exhaustive on purpose. A future expression form that reads
/// caller storage must be classified here rather than silently contributing
/// nothing, because a missing operand read widens permission.
fn collect_operand_reads(
    places: &PlaceMap,
    expression: &CheckedExpression,
    node: &NodePath,
    footprint: &mut Footprint,
) {
    let read = |footprint: &mut Footprint, resolved: Vec<ResolvedPlace>| {
        if resolved.is_empty() {
            footprint.unresolved.get_or_insert(node.clone());
            return;
        }
        footprint
            .operand_reads
            .extend(resolved.into_iter().map(|place| Access {
                place,
                argument: node.clone(),
            }));
    };
    if let Some(binding) = named_binding(expression) {
        push_reference_holder_read(places, binding, node, footprint);
    }
    match expression {
        // Reads no caller storage of its own.
        CheckedExpression::Constant(_)
        | CheckedExpression::NamedConstant { .. }
        | CheckedExpression::IntegerOperation { .. }
        | CheckedExpression::FloatOperation { .. }
        | CheckedExpression::NumericConversion { .. }
        | CheckedExpression::Reinterpret { .. }
        | CheckedExpression::BooleanOperation { .. }
        | CheckedExpression::EnumEquality { .. }
        | CheckedExpression::ConstructStruct { .. }
        | CheckedExpression::ConstructEnum { .. }
        | CheckedExpression::ProjectValue { .. } => {}
        // Naming a path reads no content: a reference formation evaluates its
        // index and endpoint atoms, which are this expression's own children
        // and are walked below [REF-1, REF-4].
        CheckedExpression::BorrowAddressed { .. } => {}
        CheckedExpression::Binding { binding, .. } => {
            read(footprint, places.resolve(PlaceRoot::Binding(*binding), &[]));
        }
        CheckedExpression::Project {
            binding, fields, ..
        } => read(
            footprint,
            places.resolve(PlaceRoot::Binding(*binding), &field_steps(fields)),
        ),
        // `deref(p)` is the path `p` names [TYPE-7, REF-1], which is what
        // resolving its root through the reference summary produces.
        CheckedExpression::DerefAddressed { binding, .. } => {
            read(footprint, places.resolve(PlaceRoot::Binding(*binding), &[]));
        }
        CheckedExpression::ContainerMeasure { root, .. }
        | CheckedExpression::ReadStorage { root, .. } => {
            read(footprint, places.resolve(root.root, &container_steps(root)));
        }
        // [REF-4, MSR-2] a measure or element read through a range reference
        // reads the path the reference names; the subscript's own offset is
        // this expression's child and is walked below.
        CheckedExpression::RangeMeasure { root, .. } => {
            read(
                footprint,
                places.resolve(PlaceRoot::Binding(root.binding), &[]),
            );
        }
        CheckedExpression::RangeElementMeasure { place, .. }
        | CheckedExpression::RangeIndex { place, .. } => {
            read(
                footprint,
                places.resolve(PlaceRoot::Binding(place.root.binding), &place.place_path()),
            );
        }
        // Forming a range names a path and reads no content [REF-1, REF-4].
        CheckedExpression::RangeOf { .. } | CheckedExpression::BorrowRangeIndex { .. } => {}
        CheckedExpression::ArrayMeasure { root, .. }
        | CheckedExpression::ArrayIndex { root, .. } => match root {
            CheckedArrayRoot::Binding { binding, fields } => read(
                footprint,
                places.resolve(PlaceRoot::Binding(*binding), &field_steps(fields)),
            ),
            CheckedArrayRoot::Constant(id) => read(
                footprint,
                vec![ResolvedPlace {
                    root: PlaceRoot::Constant(*id),
                    path: Vec::new(),
                }],
            ),
        },
        // [GRAM-9] forbids a call in argument position; if one ever reaches
        // here its whole footprint is unaccounted for.
        CheckedExpression::UserCall { .. } => {
            footprint.unresolved = Some(node.clone());
        }
        CheckedExpression::BufferMeasure { .. }
        | CheckedExpression::BufferIndex { .. }
        | CheckedExpression::BoxDeref { .. } => {
            footprint.unresolved = Some(node.clone());
        }
        // [TYPE-9, WIN-3, PAR-1] an owned Box-content take has a checked
        // field/Box-only path and consumes its complete root. That root write
        // is recorded by `collect_consumed_places`; counting the selected
        // load separately would only turn a complete footprint unresolved.
        CheckedExpression::BoxTake { .. } => {}
    }
    for child in expression_children(expression) {
        collect_operand_reads(places, child, node, footprint);
    }
}

/// The caller places one actual names [REF-1].
///
/// A reference argument names a path, so its actual resolves to the path the
/// reference names rather than to any storage of its own; at a join a
/// reference names a set, and every check on it must hold for every member.
pub(super) fn argument_places(
    places: &PlaceMap,
    argument: &CheckedExpression,
) -> Option<Vec<ResolvedPlace>> {
    let resolved = named_place(argument)?.resolve(places, false);
    (!resolved.is_empty()).then_some(resolved)
}
