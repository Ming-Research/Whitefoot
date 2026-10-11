//! EFF-2 diagnostic rows closed over recursive call substitution.
//!
//! Acceptance still compares the written row with the original structural
//! effects. These finite equations are used only to construct a DIAG-1 repair.

use std::collections::{HashMap, HashSet};

use super::{CheckStop, Checker, EffectSet, FunctionId, FunctionSignature, LocalBinding};
use crate::semantic::model::{CheckedEffectStep, CheckedStatePath};
use crate::semantic::places::{CapturedValue, PlaceStep, ResolvedPlace, WindowPart};
use crate::syntax::NodeId;
use crate::{DeclarationId, SemanticCompilerFailure, SemanticIssueKind, SemanticRule};

/// Private inputs to the same checker, never enabled for accepted programs.
#[derive(Default)]
pub(super) enum RepairMode {
    #[default]
    Ordinary,
    Capture(HashMap<FunctionId, RepairBody>),
    Validate(HashMap<DeclarationId, EffectSet>),
}

impl RepairMode {
    pub(super) fn is_ordinary(&self) -> bool {
        matches!(self, Self::Ordinary)
    }

    pub(super) fn is_capture(&self) -> bool {
        matches!(self, Self::Capture(_))
    }

    pub(super) fn checks_row(&self, declaration: DeclarationId) -> bool {
        match self {
            Self::Ordinary => true,
            Self::Capture(_) => false,
            Self::Validate(rows) => rows.contains_key(&declaration),
        }
    }
}

#[derive(Clone)]
pub(super) struct RepairBody {
    pub(super) direct: EffectSet,
    pub(super) calls: Vec<EffectCall>,
}

/// A call's formal-rooted actuals and offset names, captured only during
/// a rejection replay, even when its current callee row is empty.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct EffectCall {
    function: FunctionId,
    parameters: Vec<EffectArgument>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EffectArgument {
    declaration: DeclarationId,
    bases: Vec<EffectBase>,
    offset: Option<DeclarationId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EffectBase {
    path: CheckedStatePath,
    /// A dynamic selection or descendant cover has already truncated the
    /// path; no suffix appended below it is statically nameable [EFF-2].
    complete: bool,
}

impl Checker<'_, '_> {
    /// Classify the failed declaration before starting equation capture. This
    /// source-declaration graph reads only body calls, through the ordinary
    /// callee lookup. Calls through formal boundaries keep their fixed rows
    /// and therefore are not repair-equation edges. No body is rechecked here.
    pub(super) fn requires_recursive_row_repair(
        &self,
        signature: &FunctionSignature,
    ) -> Result<bool, CheckStop> {
        let start = signature.declaration;
        let mut declarations = vec![start];
        let mut seen = HashSet::new();
        while let Some(caller) = declarations.pop() {
            if !seen.insert(caller) {
                continue;
            }
            let Some(template) = self
                .types
                .function_templates
                .iter()
                .find(|template| template.declaration == caller)
            else {
                continue;
            };
            let context = super::CheckContext {
                writing_module: self
                    .types
                    .declarations
                    .resolved
                    .declaration(caller)
                    .and_then(crate::DeclarationRecord::module),
                ..super::CheckContext::default()
            };
            let tree = &self.types.declarations.tree;
            let mut pending = tree.children_with(template.node, crate::Production::Stmt)?;
            while let Some(node) = pending.pop() {
                let production = tree.production(node)?;
                if matches!(
                    production,
                    crate::Production::InvariantStmt | crate::Production::HeaderInvariant
                ) {
                    continue;
                }
                if production == crate::Production::Call
                    && !tree.is_constructor_call(node)?
                    && self.types.behavior_call_key(&context, node)?.is_none()
                {
                    let callee = tree
                        .first_child_with(node, crate::Production::Callee)?
                        .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
                    let usage = self.types.declarations.use_at_roles(
                        &context,
                        callee,
                        &[
                            crate::LexicalUseRole::IdentifierCallee,
                            crate::LexicalUseRole::OperationCallee,
                        ],
                    )?;
                    if let crate::ResolvedTarget::Source {
                        declaration,
                        class: crate::DeclarationClass::Function,
                    } = usage.target()
                    {
                        if declaration == start {
                            return Ok(true);
                        }
                        declarations.push(declaration);
                    }
                }
                pending.extend(tree.children(node)?);
            }
        }
        Ok(false)
    }

    pub(super) fn effect_repair_call(
        &self,
        node: NodeId,
        caller: &FunctionSignature,
        callee: &FunctionSignature,
        actuals: &[Vec<ResolvedPlace>],
        captures: &[CapturedValue],
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<EffectCall, CheckStop> {
        let mut parameters = Vec::with_capacity(callee.parameters.len());
        for (index, parameter) in callee.parameters.iter().enumerate() {
            let mut bases = Vec::new();
            for place in actuals.get(index).into_iter().flatten() {
                let complete = place.path.iter().all(|step| match step {
                    PlaceStep::Descendant(_) => false,
                    PlaceStep::Index(value) | PlaceStep::Page(value) => {
                        self.captured_parameter(*value, bindings).is_some()
                    }
                    PlaceStep::Range(range) => {
                        self.captured_parameter(range.start, bindings).is_some()
                            && self.captured_parameter(range.end, bindings).is_some()
                    }
                    _ => true,
                });
                for path in self.effect_paths_for_place(node, place, bindings)? {
                    // Atomic and local roots are framed out of the enclosing
                    // row. Direct atomic accesses retain their usual checks.
                    if caller
                        .parameters
                        .iter()
                        .any(|p| p.declaration == path.path.root)
                    {
                        bases.push(EffectBase {
                            path: path.path,
                            complete,
                        });
                    }
                }
            }
            parameters.push(EffectArgument {
                declaration: parameter.declaration,
                bases,
                offset: captures
                    .get(index)
                    .and_then(|value| self.captured_parameter(*value, bindings)),
            });
        }
        Ok(EffectCall {
            function: callee.id,
            parameters,
        })
    }

    /// The ordinary pass has already established an EFF-2 rejection. Complete
    /// the replay's structural view, then solve only its recursive component.
    pub(super) fn check_effect_rows(&mut self) -> Result<(), CheckStop> {
        let RepairMode::Capture(bodies) = &self.types.effect_repairs else {
            return Ok(());
        };
        let bodies = bodies.clone();
        for id in &self.types.view.functions {
            let signature = &self.types.signatures[id.0 as usize];
            if !bodies.contains_key(id) {
                continue;
            }
            let exhibited = self.repair_exhibition(*id, &bodies, &HashMap::new())?;
            if Checker::effect_row_matches(&signature.declared_effects, &exhibited) {
                continue;
            }
            let component = recursive_component(*id, &bodies, &self.types.signatures);
            let mut suggested = Checker::suggested_effect_row(&exhibited);
            let mut comparison = exhibited.clone();
            let mut companions = Vec::new();
            let mut validated = true;
            if !component.is_empty() {
                let rows = self.recursive_repair_rows(&component, &bodies)?;
                validated = component.iter().all(|id| {
                    self.repair_exhibition(*id, &bodies, &rows)
                        .is_ok_and(|body| {
                            Checker::effect_row_matches(
                                &rows[&self.types.signatures[id.0 as usize].declaration],
                                &body,
                            )
                        })
                });
                // Reuse the ordinary semantic checker, with all component rows
                // substituted before signature/body checking. This checks every
                // EFF-5 pair, including positional goals through entailment;
                // a structural comparison alone would miss undischarged goals.
                if validated {
                    validated = super::check_semantics_attempt(
                        self.types.declarations.resolved,
                        true,
                        None,
                        RepairMode::Validate(rows.clone()),
                    )
                    .is_ok();
                }
                if validated {
                    suggested = rows[&signature.declaration].clone();
                    comparison = self.repair_exhibition(*id, &bodies, &rows)?;
                    for other in &component {
                        let callee = &self.types.signatures[other.0 as usize];
                        let row = &rows[&callee.declaration];
                        if callee.declaration != signature.declaration
                            && (row.reads != callee.declared_effects.reads
                                || row.writes != callee.declared_effects.writes)
                        {
                            companions.push(format!(
                                "also declare the row of `{}` as `{}`",
                                callee.name,
                                self.types.render_effect_row(row, callee)?
                            ));
                        }
                    }
                }
            }
            let (missing, extra) = self.types.effect_row_difference(
                &comparison,
                &suggested,
                &signature.declared_effects,
                signature,
            )?;
            // DIAG-1 permits EFF-2 to omit a repair. Neither an expected row
            // nor an instruction to insert it is published without validation.
            let (expected_row, mechanical_fix) = if validated {
                let expected = self.types.render_effect_row(&suggested, signature)?;
                let mut fix = format!(
                    "declare the row as `{expected}`, which covers every access the body makes and no other"
                );
                for companion in companions {
                    fix.push_str("; ");
                    fix.push_str(&companion);
                }
                (Some(expected), Some(fix))
            } else {
                (None, None)
            };
            let stop = self.types.declarations.issue_node::<()>(
                SemanticRule::Eff2,
                signature.effects_node,
                SemanticIssueKind::EffectMismatch {
                    mechanical_fix,
                    expected_row,
                    found_row: self
                        .types
                        .render_effect_row(&signature.declared_effects, signature)?,
                    missing,
                    extra,
                },
            );
            return stop.map_err(|stop| self.types.attribute_to_request(*id, stop));
        }
        Ok(())
    }

    fn repair_exhibition(
        &self,
        id: FunctionId,
        bodies: &HashMap<FunctionId, RepairBody>,
        rows: &HashMap<DeclarationId, EffectSet>,
    ) -> Result<EffectSet, CheckStop> {
        let body = bodies
            .get(&id)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let mut exhibited = body.direct.clone();
        for call in &body.calls {
            let signature = &self.types.signatures[call.function.0 as usize];
            let row = rows
                .get(&signature.declaration)
                .unwrap_or(&signature.declared_effects);
            for (write, paths) in [(false, &row.reads), (true, &row.writes)] {
                for path in paths {
                    for projected in call.project(path)? {
                        if write {
                            exhibited.add_write(projected);
                        } else {
                            exhibited.add_read(projected);
                        }
                    }
                }
            }
        }
        Ok(exhibited)
    }

    fn recursive_repair_rows(
        &self,
        component: &[FunctionId],
        bodies: &HashMap<FunctionId, RepairBody>,
    ) -> Result<HashMap<DeclarationId, EffectSet>, CheckStop> {
        // Parameter edges run from a callee formal to the caller formal that
        // supplies it. A growing edge participates in recursion exactly when
        // its caller root can reach its callee root again. Function SCCs alone
        // are insufficient: a parameter can merely flow out of a cycle.
        let declarations = component
            .iter()
            .map(|id| self.types.signatures[id.0 as usize].declaration)
            .collect::<HashSet<_>>();
        let mut edges: HashMap<DeclarationId, Vec<DeclarationId>> = HashMap::new();
        for id in component {
            for call in &bodies[id].calls {
                if declarations
                    .contains(&self.types.signatures[call.function.0 as usize].declaration)
                {
                    for argument in &call.parameters {
                        for base in &argument.bases {
                            edges
                                .entry(argument.declaration)
                                .or_default()
                                .push(base.path.root);
                        }
                    }
                }
            }
        }
        let mut cyclic = HashSet::new();
        for (from, targets) in &edges {
            for target in targets {
                let mut pending = vec![*target];
                let mut seen = HashSet::new();
                while let Some(root) = pending.pop() {
                    if root == *from {
                        cyclic.insert((*from, *target));
                        break;
                    }
                    if seen.insert(root) {
                        pending.extend(edges.get(&root).into_iter().flatten().copied());
                    }
                }
            }
        }
        // Start with direct accesses and calls outside the component. Written
        // component rows are not seeds: they may contain unexhibited writes.
        let seeds = component
            .iter()
            .map(|id| {
                let mut seed = bodies[id].clone();
                seed.calls.retain(|call| {
                    !declarations
                        .contains(&self.types.signatures[call.function.0 as usize].declaration)
                });
                (*id, seed)
            })
            .collect::<HashMap<_, _>>();
        let mut proposed = HashMap::new();
        for id in component {
            proposed.insert(
                self.types.signatures[id.0 as usize].declaration,
                Checker::suggested_effect_row(&self.repair_exhibition(
                    *id,
                    &seeds,
                    &HashMap::new(),
                )?),
            );
        }
        loop {
            let mut next = proposed.clone();
            for id in component {
                let mut exhibited = self.repair_exhibition(*id, bodies, &proposed)?;
                for call in &bodies[id].calls {
                    let declaration = self.types.signatures[call.function.0 as usize].declaration;
                    let Some(callee_row) = proposed.get(&declaration) else {
                        continue;
                    };
                    for argument in &call.parameters {
                        for base in &argument.bases {
                            if base.path.steps.is_empty()
                                || !cyclic.contains(&(argument.declaration, base.path.root))
                            {
                                continue;
                            }
                            // The complete resolved argument p.s covers p.s.q
                            // at every depth, including all nameable payload
                            // steps in s. Keep independent base entries; do not
                            // merge siblings at p's prefix.
                            // Zero-suffix edges and edges between cycles compose
                            // these covers by the same ordinary call projection.
                            if callee_row
                                .writes
                                .iter()
                                .any(|p| p.root == argument.declaration)
                            {
                                exhibited.add_write(base.path.clone());
                            }
                            if callee_row
                                .reads
                                .iter()
                                .any(|p| p.root == argument.declaration)
                            {
                                exhibited.add_read(base.path.clone());
                            }
                        }
                    }
                }
                let declaration = self.types.signatures[id.0 as usize].declaration;
                let prior = next
                    .remove(&declaration)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                next.insert(
                    declaration,
                    Checker::suggested_effect_row(&prior.union(exhibited)),
                );
            }
            if proposed == next {
                return Ok(proposed);
            }
            proposed = next;
        }
    }
}

impl EffectCall {
    fn project(&self, path: &CheckedStatePath) -> Result<Vec<CheckedStatePath>, CheckStop> {
        let argument = self
            .parameters
            .iter()
            .find(|p| p.declaration == path.root)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let offset = |id| {
            self.parameters
                .iter()
                .find(|p| p.declaration == id)
                .and_then(|p| p.offset)
        };
        let mut paths = Vec::new();
        for base in &argument.bases {
            let mut projected = base.path.clone();
            if base.complete {
                for step in &path.steps {
                    let mapped = match *step {
                        CheckedEffectStep::Index(id) => match offset(id) {
                            Some(id) => CheckedEffectStep::Index(id),
                            None => break,
                        },
                        CheckedEffectStep::Page(id) => match offset(id) {
                            Some(id) => CheckedEffectStep::Page(id),
                            None => {
                                projected
                                    .steps
                                    .push(CheckedEffectStep::Part(WindowPart::Filled));
                                break;
                            }
                        },
                        CheckedEffectStep::Range { start, end } => {
                            match (offset(start), offset(end)) {
                                (Some(start), Some(end)) => CheckedEffectStep::Range { start, end },
                                _ => break,
                            }
                        }
                        step => step,
                    };
                    projected.steps.push(mapped);
                }
            }
            paths.push(projected);
        }
        Ok(paths)
    }
}

/// Closure terminates without a budget: growing parameter-cycle edges have
/// one of finitely many complete argument-path covers in each access category. An
/// uncovered path can therefore traverse only a finite acyclic parameter walk
/// or zero-length cycle (whose offset substitutions range over finite formal
/// names). Base paths, argument suffixes and those walks are all finite.
/// A written row belongs to a source declaration, including all generic
/// instances. Renamed symbolic summaries keep canonical call identities, so
/// instance-ID cycles alone do not describe the source's recursive component.
/// Prefer a symbolic body as the representative of each declaration.
fn recursive_component(
    id: FunctionId,
    bodies: &HashMap<FunctionId, RepairBody>,
    signatures: &[FunctionSignature],
) -> Vec<FunctionId> {
    let mut representatives: HashMap<DeclarationId, FunctionId> = HashMap::new();
    for function in bodies.keys() {
        let signature = &signatures[function.0 as usize];
        let key = (!signature.substitution.is_symbolic(), function.0);
        representatives
            .entry(signature.declaration)
            .and_modify(|prior| {
                let previous = &signatures[prior.0 as usize];
                if key < (!previous.substitution.is_symbolic(), prior.0) {
                    *prior = *function;
                }
            })
            .or_insert(*function);
    }
    let mut forward_edges: HashMap<DeclarationId, Vec<DeclarationId>> = HashMap::new();
    let mut reverse: HashMap<DeclarationId, Vec<DeclarationId>> = HashMap::new();
    for (declaration, function) in &representatives {
        for call in &bodies[function].calls {
            let callee = signatures[call.function.0 as usize].declaration;
            forward_edges.entry(*declaration).or_default().push(callee);
            reverse.entry(callee).or_default().push(*declaration);
        }
    }
    let declaration = signatures[id.0 as usize].declaration;
    let closure = |edges: &HashMap<DeclarationId, Vec<DeclarationId>>| {
        let mut reached = HashSet::new();
        let mut pending = vec![declaration];
        while let Some(current) = pending.pop() {
            if reached.insert(current) {
                pending.extend(edges.get(&current).into_iter().flatten().copied());
            }
        }
        reached
    };
    let forward = closure(&forward_edges);
    let backward = closure(&reverse);
    let declarations = forward.intersection(&backward).copied().collect::<Vec<_>>();
    if declarations.len() == 1
        && !forward_edges
            .get(&declaration)
            .is_some_and(|callees| callees.contains(&declaration))
    {
        return Vec::new();
    }
    let mut component = declarations
        .iter()
        .filter_map(|declaration| representatives.get(declaration).copied())
        .collect::<Vec<_>>();
    component.sort_unstable_by_key(|function| function.0);
    component
}
