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

/// A call's formal-rooted actuals and offset names, captured by the ordinary
/// resolved-place walk even when its current callee row is empty.
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

    /// Delay only the row comparison until all bodies in the view are known.
    /// Acyclic repairs use exactly the previous row and sentence.
    pub(super) fn check_effect_rows(&mut self) -> Result<(), CheckStop> {
        for id in &self.types.view.functions {
            let signature = &self.types.signatures[id.0 as usize];
            let Some(exhibited) = self.types.effect_bodies.get(id) else {
                continue;
            };
            if Checker::effect_row_matches(&signature.declared_effects, exhibited) {
                continue;
            }
            let component =
                recursive_component(*id, &self.types.effect_bodies, &self.types.signatures);
            let (suggested, comparison, companions) = if component.is_empty() {
                (
                    Checker::suggested_effect_row(exhibited),
                    exhibited.clone(),
                    Vec::new(),
                )
            } else {
                let rows = self.recursive_repair_rows(&component)?;
                let suggested = rows
                    .get(&signature.declaration)
                    .cloned()
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                let comparison = self.repair_exhibition(*id, &rows)?;
                let mut companions = Vec::new();
                for other in &component {
                    let callee = &self.types.signatures[other.0 as usize];
                    if callee.declaration == signature.declaration {
                        continue;
                    }
                    let row = rows
                        .get(&callee.declaration)
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                    // Include every changed component boundary: changing just
                    // one side of mutual recursion need not reach a fixpoint.
                    if row.reads != callee.declared_effects.reads
                        || row.writes != callee.declared_effects.writes
                    {
                        companions.push(format!(
                            "also declare the row of `{}` as `{}`",
                            callee.name,
                            self.types.render_effect_row(row, callee)?
                        ));
                    }
                }
                (suggested, comparison, companions)
            };
            let (missing, extra) = self.types.effect_row_difference(
                &comparison,
                &suggested,
                &signature.declared_effects,
                signature,
            )?;
            let expected_row = self.types.render_effect_row(&suggested, signature)?;
            let mut mechanical_fix = format!(
                "declare the row as `{expected_row}`, which covers every access the body makes and no other"
            );
            for companion in companions {
                mechanical_fix.push_str("; ");
                mechanical_fix.push_str(&companion);
            }
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
        rows: &HashMap<DeclarationId, EffectSet>,
    ) -> Result<EffectSet, CheckStop> {
        let body = self
            .types
            .effect_bodies
            .get(&id)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let mut exhibited = EffectSet::NONE;
        for path in &body.direct_reads {
            exhibited.add_read(path.clone());
        }
        for path in &body.direct_writes {
            exhibited.add_write(path.clone());
        }
        for call in &body.calls {
            let row = rows
                .get(&self.types.signatures[call.function.0 as usize].declaration)
                .unwrap_or(&self.types.signatures[call.function.0 as usize].declared_effects);
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
    ) -> Result<HashMap<DeclarationId, EffectSet>, CheckStop> {
        let mut rows = component
            .iter()
            .map(|id| {
                (
                    self.types.signatures[id.0 as usize].declaration,
                    Checker::suggested_effect_row(&self.types.effect_bodies[id]),
                )
            })
            .collect::<HashMap<_, _>>();
        self.close_repair_rows(component, &mut rows, true)?;
        // Old callee declarations can seed writes the repaired bodies never
        // exhibit. Prune writes first: they subsume reads, so removing one can
        // expose a read that the first closure did not retain.
        self.prune_repair_rows(component, &mut rows, true)?;
        self.close_repair_rows(component, &mut rows, false)?;
        // With writes settled, removing an unexhibited read uncovers no
        // access and subsequent read projections only shrink.
        self.prune_repair_rows(component, &mut rows, false)?;
        for id in component {
            if !Checker::effect_row_matches(
                &rows[&self.types.signatures[id.0 as usize].declaration],
                &self.repair_exhibition(*id, &rows)?,
            ) {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            }
        }
        Ok(rows)
    }

    fn close_repair_rows(
        &self,
        component: &[FunctionId],
        rows: &mut HashMap<DeclarationId, EffectSet>,
        writes: bool,
    ) -> Result<(), CheckStop> {
        loop {
            let mut next = rows.clone();
            for id in component {
                let exhibited = self.repair_exhibition(*id, rows)?;
                let row = next
                    .get_mut(&self.types.signatures[id.0 as usize].declaration)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                if writes {
                    widen_category(&mut row.writes, &exhibited.writes);
                }
                widen_category(&mut row.reads, &exhibited.reads);
                *row = Checker::suggested_effect_row(row);
            }
            if *rows == next {
                return Ok(());
            }
            *rows = next;
        }
    }

    /// Removing an unexhibited entry uncovers no access in that category.
    /// Every changed round removes an entry, so no depth or work limit is
    /// needed. Write support is independent of read projections.
    fn prune_repair_rows(
        &self,
        component: &[FunctionId],
        rows: &mut HashMap<DeclarationId, EffectSet>,
        writes: bool,
    ) -> Result<(), CheckStop> {
        loop {
            let mut next = rows.clone();
            for id in component {
                let exhibited = self.repair_exhibition(*id, rows)?;
                let row = next
                    .get_mut(&self.types.signatures[id.0 as usize].declaration)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                if writes {
                    row.writes.retain(|entry| {
                        exhibited
                            .writes
                            .iter()
                            .any(|path| Checker::effect_path_covers(entry, path))
                    });
                } else {
                    row.reads.retain(|entry| {
                        exhibited
                            .reads
                            .iter()
                            .chain(&exhibited.writes)
                            .any(|path| Checker::effect_path_covers(entry, path))
                    });
                }
                *row = Checker::suggested_effect_row(row);
            }
            if *rows == next {
                return Ok(());
            }
            *rows = next;
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

/// Every new root has one finite initial suffix. An uncovered path at a root
/// already present replaces that root's entries by their common prefix. It
/// either strictly shortens a suffix or merges finitely many sibling entries;
/// later substitutions cannot lengthen it. Reads and writes widen separately,
/// then ordinary EFF-1 canonicalization removes entries covered by writes.
fn widen_category(current: &mut Vec<CheckedStatePath>, exhibited: &[CheckedStatePath]) {
    for path in exhibited {
        if current
            .iter()
            .any(|entry| Checker::effect_path_covers(entry, path))
        {
            continue;
        }
        let mut prefix = path.clone();
        for entry in current.iter().filter(|entry| entry.root == path.root) {
            let length = prefix
                .steps
                .iter()
                .zip(&entry.steps)
                .take_while(|(left, right)| left == right)
                .count();
            prefix.steps.truncate(length);
        }
        current.retain(|entry| entry.root != path.root);
        current.push(prefix);
        current.sort_unstable();
    }
}

/// A written row belongs to a source declaration, including all generic
/// instances. Renamed symbolic summaries keep canonical call identities, so
/// instance-ID cycles alone do not describe the source's recursive component.
/// Prefer a symbolic body as the representative of each declaration.
fn recursive_component(
    id: FunctionId,
    bodies: &HashMap<FunctionId, EffectSet>,
    signatures: &[FunctionSignature],
) -> Vec<FunctionId> {
    let mut representatives: HashMap<DeclarationId, FunctionId> = HashMap::new();
    for function in bodies.keys() {
        let signature = &signatures[function.0 as usize];
        let key = (!signature.substitution.is_symbolic(), *function);
        representatives
            .entry(signature.declaration)
            .and_modify(|prior| {
                let previous = &signatures[prior.0 as usize];
                if key < (!previous.substitution.is_symbolic(), *prior) {
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
    component.sort_unstable();
    component
}
