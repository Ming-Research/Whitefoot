//! The records of one checked function's mandatory obligations
//! (`design/compiler/acceptance-records.md`).
//!
//! The walk reads only what the checker admitted into the function: every
//! requirement place, statement and expression, each classified by an
//! exhaustive match, so a new obligation-bearing form cannot be admitted
//! without deciding its record here. It is independent of the engine's walk
//! and of its reachability, which is what makes a judgment the engine skips a
//! rejection instead of an acceptance.

use super::super::entailment::ObligationFamily;
use super::super::goal::CheckedCallRequirement;
use super::super::model::{
    CheckedAffineExpressionKind, CheckedAffineRelation, CheckedContainerRoot,
    CheckedConversionMode, CheckedExpression, CheckedFunction, CheckedLoopId, CheckedLoopInvariant,
    CheckedPlaceStep, CheckedProofUseSource, CheckedRangeElementPlace, CheckedRangeSource,
    CheckedSetTarget, CheckedStatement, FunctionId,
};
use super::super::obligations::{ObligationRecord, ObligationSubject};
use super::{CheckStop, CheckedFunctionInventory};
use crate::semantic::check::TypeContext;
use crate::{NodePath, SemanticCompilerFailure, SemanticRule};

/// Forms the record of every mandatory obligation `function` carries, in the
/// order its requirements, body and submitted separations admit them.
///
/// `empties_run` says whether a callee is the one row whose requirement
/// [OP-14] reports under its own rule; every other callee's requirement is
/// an [FN-8] obligation.
fn obligation_records(
    function: &CheckedFunction,
    empties_run: &dyn Fn(FunctionId) -> bool,
) -> Vec<ObligationRecord> {
    let mut records = Records {
        list: Vec::new(),
        empties_run,
        function: function.id,
        loops: Vec::new(),
        gives: 0,
        atomic: None,
    };
    // [ENT-2, FN-8] the places a requirement forms owe their subscripts'
    // obligations at body entry.
    for places in &function.requirement_places {
        for place in places {
            records.expression(place);
        }
    }
    if let Some(body) = &function.body {
        records.statements(body);
    }
    // [OWN-7] the separations the checker's comparison could not decide by
    // syntax, each submitted where its call, set or reference use stands.
    for (query, separation) in function.call_separations.iter().enumerate() {
        let query = u32::try_from(query).expect("separation queries exceed u32");
        let (rule, site, family) = match &separation.reference_use {
            Some(use_site) => (
                SemanticRule::Ref2,
                use_site.site.clone(),
                ObligationFamily::ReferencePreservation(query),
            ),
            None if separation.exchange => (
                SemanticRule::Op11,
                separation.site.clone(),
                ObligationFamily::ExchangeSeparation(query),
            ),
            None => (
                SemanticRule::Eff5,
                separation.site.clone(),
                ObligationFamily::CallSeparation(query),
            ),
        };
        records.push(
            rule,
            site,
            ObligationSubject::Source {
                family,
                conjunct: 0,
            },
        );
    }
    // [FN-9] a declared relation is proved at the exits of a body; a
    // compiler-owned [PRE-1] signature has none, its contract being a
    // declaration premise.
    if function.body.is_some() {
        for (ordinal, postcondition) in function.postconditions.iter().enumerate() {
            records.push(
                SemanticRule::Fn9,
                postcondition.selector.selector.clone(),
                ObligationSubject::Postcondition {
                    relation_ordinal: u32::try_from(ordinal)
                        .expect("postcondition relation ordinal exceeds u32"),
                },
            );
        }
    }
    records.list
}

struct Records<'rows> {
    list: Vec<ObligationRecord>,
    empties_run: &'rows dyn Fn(FunctionId) -> bool,
    /// The function whose records these are: a construction's type
    /// invariant [TYPE-11] is judged as a requirement it owes itself.
    function: FunctionId,
    /// The loops around the statement being walked, innermost last.
    loops: Vec<CheckedLoopId>,
    /// How many value initializers [GIVE-1] are around it.
    gives: usize,
    /// [TYPE-11] the atomic block around it: its invariants, and the loop
    /// and value-initializer depths where it began.
    atomic: Option<(Vec<CheckedCallRequirement>, usize, usize)>,
}

impl Records<'_> {
    fn push(&mut self, rule: SemanticRule, site: NodePath, subject: ObligationSubject) {
        self.list.push(ObligationRecord {
            rule,
            site,
            subject,
        });
    }

    fn source(
        &mut self,
        rule: SemanticRule,
        site: &NodePath,
        family: ObligationFamily,
        conjunct: u8,
    ) {
        self.push(
            rule,
            site.clone(),
            ObligationSubject::Source { family, conjunct },
        );
    }

    /// [TYPE-11] the atomic block's invariants, owed at `site`, an edge
    /// that leaves the block.
    fn atomic_exit(&mut self, site: &NodePath) {
        let Some((invariants, ..)) = &self.atomic else {
            return;
        };
        let records = invariants
            .iter()
            .map(|invariant| ObligationRecord {
                rule: SemanticRule::Type11,
                site: site.clone(),
                subject: ObligationSubject::CallRequirement {
                    callee: self.function,
                    requires_clause: invariant.requires_clause.clone(),
                    subject: invariant.subject,
                },
            })
            .collect::<Vec<_>>();
        self.list.extend(records);
    }

    fn statements(&mut self, statements: &[CheckedStatement]) {
        for statement in statements {
            self.statement(statement);
        }
    }

    fn statement(&mut self, statement: &CheckedStatement) {
        match statement {
            CheckedStatement::Let { value, .. }
            | CheckedStatement::DestructuringLet { value, .. }
            | CheckedStatement::Evaluate { value, .. }
            | CheckedStatement::DropExpression { value, .. } => self.expression(value),
            CheckedStatement::Return {
                node_path, value, ..
            } => {
                self.expression(value);
                self.atomic_exit(node_path);
            }
            CheckedStatement::Give {
                node_path, value, ..
            } => {
                self.expression(value);
                if self
                    .atomic
                    .as_ref()
                    .is_some_and(|(_, _, gives)| self.gives <= *gives)
                {
                    self.atomic_exit(node_path);
                }
            }
            CheckedStatement::PropagateLet {
                node_path,
                scrutinee,
                ..
            } => {
                self.expression(scrutinee);
                self.atomic_exit(node_path);
            }
            CheckedStatement::Set { target, value, .. } => {
                match target {
                    CheckedSetTarget::Place(_) => {}
                    CheckedSetTarget::RangeIndex(place) => self.range_element_place(place),
                    CheckedSetTarget::Storage(root) => self.container_root(root),
                }
                self.expression(value);
            }
            // [INV-1, PRF-1] a local invariant with a written `use` block is
            // a certificate; without one, AUTO proves its target.
            CheckedStatement::Proof(proof) => {
                self.affine_relation(&proof.target);
                for written_use in &proof.uses {
                    match &written_use.source {
                        CheckedProofUseSource::Relation(relation) => self.affine_relation(relation),
                        CheckedProofUseSource::Named(_) => {}
                    }
                }
                let rule = if proof.uses.is_empty() {
                    SemanticRule::Inv1
                } else {
                    SemanticRule::Prf1
                };
                self.push(
                    rule,
                    proof.node_path.clone(),
                    ObligationSubject::SourceProof,
                );
            }
            CheckedStatement::Match {
                scrutinee, arms, ..
            } => {
                self.expression(scrutinee);
                for arm in arms {
                    self.statements(&arm.body);
                }
            }
            CheckedStatement::ValueMatchLet {
                scrutinee, arms, ..
            } => {
                self.expression(scrutinee);
                self.gives += 1;
                for arm in arms {
                    self.statements(&arm.body);
                }
                self.gives -= 1;
            }
            CheckedStatement::Loop {
                id,
                invariants,
                body,
                ..
            } => {
                self.loop_invariants(invariants);
                self.loops.push(*id);
                self.statements(body);
                self.loops.pop();
            }
            CheckedStatement::CountedRange {
                id,
                lower,
                upper,
                invariants,
                body,
                ..
            } => {
                self.expression(lower);
                self.expression(upper);
                self.loop_invariants(invariants);
                self.loops.push(*id);
                self.statements(body);
                self.loops.pop();
            }
            // [TYPE-11] a break of a loop around an atomic block leaves it.
            CheckedStatement::Break {
                node_path, target, ..
            } => {
                if let Some((_, loops, _)) = &self.atomic
                    && self
                        .loops
                        .iter()
                        .rposition(|loop_id| loop_id == target)
                        .is_some_and(|position| position < *loops)
                {
                    self.atomic_exit(node_path);
                }
            }
            CheckedStatement::Atomic {
                node_path,
                target,
                key,
                guard,
                body,
                continues,
                invariants,
                ..
            } => {
                self.expression(target);
                if let Some(key) = key {
                    self.expression(key);
                }
                if let Some(guard) = guard {
                    self.expression(guard);
                }
                let enclosing =
                    self.atomic
                        .replace((invariants.clone(), self.loops.len(), self.gives));
                self.statements(body);
                if *continues {
                    self.atomic_exit(node_path);
                }
                self.atomic = enclosing;
            }
        }
    }

    fn loop_invariants(&mut self, invariants: &[CheckedLoopInvariant]) {
        for invariant in invariants {
            self.affine_relation(&invariant.relation);
            self.push(
                SemanticRule::Inv1,
                invariant.relation.node_path.clone(),
                ObligationSubject::LoopInvariant,
            );
        }
    }

    /// [INV-1, MSR-1] an invariant evaluates nothing, but a measure it names
    /// is a place whose subscripts owe [OP-4] where the invariant stands.
    fn affine_relation(&mut self, relation: &CheckedAffineRelation) {
        for side in [&relation.left, &relation.right] {
            for expression in side.postorder() {
                match &expression.kind {
                    CheckedAffineExpressionKind::Measure(measure) => self.expression(measure),
                    CheckedAffineExpressionKind::Constant { .. }
                    | CheckedAffineExpressionKind::Local { .. }
                    | CheckedAffineExpressionKind::ConstGeneric { .. }
                    | CheckedAffineExpressionKind::Add(..)
                    | CheckedAffineExpressionKind::Subtract(..)
                    | CheckedAffineExpressionKind::MultiplyByConstant { .. } => {}
                }
            }
        }
    }

    fn expression(&mut self, expression: &CheckedExpression) {
        match expression {
            CheckedExpression::UserCall {
                function,
                call,
                arguments,
                requirements,
                allocation,
                ..
            } => {
                for argument in arguments {
                    self.expression(argument);
                }
                if allocation.is_some() {
                    self.source(SemanticRule::Op9, call, ObligationFamily::AllocationFit, 0);
                }
                let rule = if (self.empties_run)(*function) {
                    SemanticRule::Op14
                } else {
                    SemanticRule::Fn8
                };
                for requirement in requirements {
                    self.push(
                        rule,
                        call.clone(),
                        ObligationSubject::CallRequirement {
                            callee: *function,
                            requires_clause: requirement.requires_clause.clone(),
                            subject: requirement.subject,
                        },
                    );
                }
            }
            CheckedExpression::IntegerOperation {
                carrier,
                operation,
                arguments,
                ..
            } => {
                for argument in arguments {
                    self.expression(argument);
                }
                if operation.is_exact() {
                    self.source(
                        SemanticRule::Op2,
                        carrier,
                        ObligationFamily::IntegerDomain,
                        0,
                    );
                }
            }
            CheckedExpression::NumericConversion {
                carrier,
                mode,
                value,
                ..
            } => {
                self.expression(value);
                if *mode == CheckedConversionMode::Exact {
                    self.source(
                        SemanticRule::Op6,
                        carrier,
                        ObligationFamily::ConversionDomain,
                        0,
                    );
                }
            }
            CheckedExpression::ArrayIndex {
                offset, obligation, ..
            } => {
                self.expression(offset);
                self.source(SemanticRule::Op4, obligation, ObligationFamily::Bounds, 0);
            }
            CheckedExpression::BufferIndex {
                root,
                offset,
                obligation,
                ..
            } => {
                self.path_subscripts(&root.path);
                self.expression(offset);
                self.source(SemanticRule::Op4, obligation, ObligationFamily::Bounds, 0);
            }
            CheckedExpression::RangeElementMeasure { place, .. }
            | CheckedExpression::RangeIndex { place, .. }
            | CheckedExpression::BorrowRangeIndex { place, .. } => self.range_element_place(place),
            // [REF-4] the formation owes `lo <= hi` and `hi <= x.len`, after
            // the source place's own subscripts.
            CheckedExpression::RangeOf {
                source,
                start,
                end,
                obligation,
                ..
            } => {
                self.expression(start);
                self.expression(end);
                match source {
                    CheckedRangeSource::Storage(root) => self.container_root(root),
                    CheckedRangeSource::Range(_) => {}
                    CheckedRangeSource::Element(place) => self.range_element_place(place),
                }
                self.source(
                    SemanticRule::Ref4,
                    obligation,
                    ObligationFamily::RangeFormation,
                    0,
                );
                self.source(
                    SemanticRule::Ref4,
                    obligation,
                    ObligationFamily::RangeFormation,
                    1,
                );
            }
            CheckedExpression::ContainerMeasure { root, .. }
            | CheckedExpression::BorrowAddressed { root, .. }
            | CheckedExpression::ReadStorage { root, .. } => self.container_root(root),
            // [OP-4] a segment owes `i < s.len` after the place's own
            // subscripts.
            CheckedExpression::BorrowSegment { root, segment, .. } => {
                self.container_root(root);
                if let crate::semantic::CheckedSegmentSelect::One(index) = segment {
                    self.expression(&index.offset);
                    self.source(
                        SemanticRule::Op4,
                        &index.obligation,
                        ObligationFamily::Bounds,
                        0,
                    );
                }
            }
            CheckedExpression::BoxTake { path, .. } => self.path_subscripts(path),
            CheckedExpression::FloatOperation { arguments, .. }
            | CheckedExpression::BooleanOperation { arguments, .. }
            | CheckedExpression::EnumEquality { arguments, .. } => {
                for argument in arguments {
                    self.expression(argument);
                }
            }
            // [TYPE-11] a construction owes its struct's type invariants
            // after its operands, as a call owes its requirements.
            CheckedExpression::ConstructStruct {
                carrier,
                fields,
                invariants,
                ..
            } => {
                for field in fields {
                    self.expression(field);
                }
                for invariant in invariants {
                    self.push(
                        SemanticRule::Type11,
                        carrier.clone(),
                        ObligationSubject::CallRequirement {
                            callee: self.function,
                            requires_clause: invariant.requires_clause.clone(),
                            subject: invariant.subject,
                        },
                    );
                }
            }
            CheckedExpression::ConstructEnum { fields, .. } => {
                for field in fields {
                    self.expression(field);
                }
            }
            CheckedExpression::Reinterpret { value, .. }
            | CheckedExpression::BoxDeref { value, .. }
            | CheckedExpression::ProjectValue { value, .. } => self.expression(value),
            CheckedExpression::Constant(_)
            | CheckedExpression::NamedConstant { .. }
            | CheckedExpression::Binding { .. }
            | CheckedExpression::ArrayMeasure { .. }
            | CheckedExpression::BufferMeasure { .. }
            | CheckedExpression::RangeMeasure { .. }
            | CheckedExpression::DerefAddressed { .. }
            | CheckedExpression::Project { .. } => {}
        }
    }

    fn container_root(&mut self, root: &CheckedContainerRoot) {
        self.path_subscripts(&root.path);
    }

    /// [OP-4, REF-4] the outer range position, then every nested subscript.
    fn range_element_place(&mut self, place: &CheckedRangeElementPlace) {
        self.expression(&place.offset);
        self.source(
            SemanticRule::Op4,
            &place.obligation,
            ObligationFamily::Bounds,
            0,
        );
        self.path_subscripts(&place.path);
    }

    /// [OP-4, MSR-1] each subscript inside a place, in written order.
    fn path_subscripts(&mut self, path: &[CheckedPlaceStep]) {
        for step in path {
            match step {
                CheckedPlaceStep::Subscript(subscript) => {
                    self.expression(&subscript.offset);
                    self.source(
                        SemanticRule::Op4,
                        &subscript.obligation,
                        ObligationFamily::Bounds,
                        0,
                    );
                }
                CheckedPlaceStep::Field(_) | CheckedPlaceStep::BoxReferent(_) => {}
            }
        }
    }
}

impl<'unit> TypeContext<'unit> {
    /// Forms the records of every function in `functions`, once its call
    /// requirements are installed and before any of them is analyzed.
    pub(super) fn form_obligation_records(
        &self,
        functions: &mut [CheckedFunctionInventory],
    ) -> Result<(), CheckStop> {
        let empties_run = self.release_rows()?;
        for checked in functions {
            if !self.view.contains_function(checked.function.id) {
                continue;
            }
            checked.function.obligations = obligation_records(&checked.function, &empties_run);
        }
        Ok(())
    }
    /// Forms the records of one function checked on its own, such as a
    /// formal's hypothetical body entry [ENT-2, FN-8].
    pub(super) fn form_function_obligation_records(
        &self,
        function: &mut CheckedFunction,
    ) -> Result<(), CheckStop> {
        let empties_run = self.release_rows()?;
        function.obligations = obligation_records(function, &empties_run);
        Ok(())
    }
    /// Whether each signature built so far is the [OP-14] row.
    fn release_rows(&self) -> Result<impl Fn(FunctionId) -> bool, CheckStop> {
        let rows = (0..self.signatures.len())
            .map(|index| {
                let index = u32::try_from(index)
                    .map_err(|_| CheckStop::from(SemanticCompilerFailure::CounterOverflow))?;
                self.empties_run(FunctionId(index))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(move |function: FunctionId| rows.get(function.0 as usize).copied().unwrap_or(false))
    }
}
