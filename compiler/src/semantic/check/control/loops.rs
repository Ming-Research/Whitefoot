use std::collections::{HashMap, HashSet};

use crate::semantic::tree::ConditionalAlternative;
use crate::syntax::NodeId;
use crate::{
    DeclarationClass, DeclarationId, DeclarationRole, LexicalUseRole, Production, ResolvedTarget,
    SemanticCompilerFailure, SemanticIssueKind, SemanticRule, UnsupportedSemanticFeature,
};

use super::super::super::model::{
    BindingId, CheckedExpression, CheckedLoopId, CheckedLoopInvariant, CheckedMode,
    CheckedStatement, CheckedType, IntegerType, SubscriptedTerm,
};
use super::super::references::{
    InvalidationEvent, LoopReferenceToken, ReferenceValidity, RequiredReferent,
};
use super::super::{
    CheckStop, Checker, EffectSet, FunctionSignature, LocalBinding, TypedExpression,
};
use super::proofs::AffineProofOwner;
use super::{ControlCounters, ControlScope, StatementResult};

#[derive(Clone)]
pub(in crate::semantic::check) struct LoopContext {
    pub(super) id: CheckedLoopId,
    pub(super) reference_rebindings: HashSet<NodeId>,
    label_declaration: Option<DeclarationId>,
    preserved: HashSet<DeclarationId>,
}

#[derive(Default)]
struct ReferenceRebindings {
    holders: HashSet<DeclarationId>,
    sites: HashSet<NodeId>,
}

pub(in crate::semantic::check) struct BreakState {
    target: CheckedLoopId,
    bindings: HashMap<DeclarationId, LocalBinding>,
}

#[derive(Clone)]
struct LoopReferenceEquation {
    declaration: DeclarationId,
    token: LoopReferenceToken,
    entry_validity: ReferenceValidity,
    entry_dependencies: Vec<LoopReferenceToken>,
    entry_preservations: Vec<super::super::super::model::CheckedCallSeparation>,
}

#[derive(Clone)]
struct LoopReferenceResolution {
    invalid: Option<InvalidationEvent>,
    dependencies: Vec<LoopReferenceToken>,
    preservations: Vec<super::super::super::model::CheckedCallSeparation>,
}

impl<'unit> Checker<'unit> {
    fn loop_binding_agrees(
        &self,
        entry: Option<&LocalBinding>,
        backedge: Option<&LocalBinding>,
    ) -> bool {
        match (entry, backedge) {
            (Some(entry), Some(backedge)) => entry.loop_agrees_with(backedge),
            (None, None) => true,
            _ => false,
        }
    }

    /// Forms the one finite abstract header used to check every iteration.
    /// Every outer reference gets an owner-tagged validity variable. Only a
    /// holder a continuing source `set` may rebind loses capture precision
    /// and receives the current finite path summary [REF-1].
    fn enter_loop_reference_header(
        &self,
        id: CheckedLoopId,
        rebound: &HashSet<DeclarationId>,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
    ) -> Result<Vec<LoopReferenceEquation>, CheckStop> {
        let mut declarations = bindings.keys().copied().collect::<Vec<_>>();
        declarations.sort_by_key(|declaration| declaration.index());
        let mut equations = Vec::new();
        for declaration in declarations {
            let local = bindings
                .get_mut(&declaration)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            let Some(reference) = local.reference.as_mut() else {
                continue;
            };
            let token = LoopReferenceToken {
                loop_id: id,
                owner: local.binding,
            };
            equations.push(LoopReferenceEquation {
                declaration,
                token,
                entry_validity: reference.validity.clone(),
                entry_dependencies: reference.loop_dependencies().to_vec(),
                entry_preservations: reference.preservations.clone(),
            });
            if let Some(paths) =
                reference.enter_loop_header(token, rebound.contains(&declaration))?
            {
                let ty = local.ty;
                let kind = reference.kind;
                self.join_loop_reference_summary(token, ty, kind, &paths, bindings)?;
                let paths = self
                    .loop_reference_summaries
                    .borrow()
                    .get(&token)
                    .cloned()
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                bindings
                    .get_mut(&declaration)
                    .and_then(|local| local.reference.as_mut())
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?
                    .paths
                    .clone_from(&paths);
                self.record_reference_origins(token.owner, &paths);
            }
        }
        Ok(equations)
    }

    fn record_reference_rebinding_target(
        &self,
        set: NodeId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        rebound: &mut ReferenceRebindings,
    ) -> Result<(), CheckStop> {
        let [target] = self.tree.children_with(set, Production::Place)?[..] else {
            return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
        };
        if !self
            .tree
            .children_with(target, Production::Psuffix)?
            .is_empty()
        {
            return Ok(());
        }
        let Some(declaration) = self.complete_binding_target(target)? else {
            return Ok(());
        };
        if bindings
            .get(&declaration)
            .is_some_and(|binding| binding.reference.is_some())
        {
            rebound.holders.insert(declaration);
            rebound.sites.insert(set);
        }
        Ok(())
    }

    /// Finds the outer reference holders a source path reaching this loop's
    /// normal backedge may rebind. This is a structural control scan only: it
    /// allocates no binding, emits no diagnostic, and evaluates no expression.
    fn continuing_reference_rebindings(
        &self,
        statements: &[NodeId],
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<ReferenceRebindings, CheckStop> {
        let mut rebound = ReferenceRebindings::default();
        let _ =
            self.collect_continuing_reference_rebindings(statements, true, bindings, &mut rebound)?;
        Ok(rebound)
    }

    fn collect_continuing_reference_rebindings(
        &self,
        statements: &[NodeId],
        normal_reaches: bool,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        rebound: &mut ReferenceRebindings,
    ) -> Result<bool, CheckStop> {
        let mut reaches = normal_reaches;
        for wrapper in statements.iter().rev() {
            let statement = self.tree.only_child(*wrapper)?;
            reaches = self.collect_continuing_reference_rebinding_statement(
                statement, reaches, bindings, rebound,
            )?;
        }
        Ok(reaches)
    }

    fn collect_continuing_reference_rebinding_statement(
        &self,
        statement: NodeId,
        normal_reaches: bool,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        rebound: &mut ReferenceRebindings,
    ) -> Result<bool, CheckStop> {
        match self.tree.production(statement)? {
            Production::SetStmt => {
                if normal_reaches {
                    self.record_reference_rebinding_target(statement, bindings, rebound)?;
                }
                Ok(normal_reaches)
            }
            Production::ReturnStmt | Production::GiveStmt | Production::BreakStmt => Ok(false),
            Production::IfStmt => {
                let blocks = self.tree.conditional_blocks(statement)?;
                let then_reaches = self.collect_continuing_reference_rebindings(
                    &blocks.then_statements,
                    normal_reaches,
                    bindings,
                    rebound,
                )?;
                let else_reaches = match &blocks.alternative {
                    ConditionalAlternative::Absent => normal_reaches,
                    ConditionalAlternative::Block(statements) => self
                        .collect_continuing_reference_rebindings(
                            statements,
                            normal_reaches,
                            bindings,
                            rebound,
                        )?,
                    ConditionalAlternative::Chain(nested) => self
                        .collect_continuing_reference_rebinding_statement(
                            *nested,
                            normal_reaches,
                            bindings,
                            rebound,
                        )?,
                };
                Ok(then_reaches || else_reaches)
            }
            Production::MatchStmt => {
                let mut reaches = false;
                for arm in self.tree.children_with(statement, Production::Arm)? {
                    reaches |= self.collect_continuing_reference_rebindings(
                        &self.tree.children_with(arm, Production::Stmt)?,
                        normal_reaches,
                        bindings,
                        rebound,
                    )?;
                }
                Ok(reaches)
            }
            Production::LoopStmt | Production::ForStmt => {
                // FN-1 retains a conservative successor for a nested loop.
                // Any nested rebinding may consequently reach this outer
                // backedge; filtering its internal exits would require the
                // checked loop ids this side-effect-free inventory precedes.
                if normal_reaches {
                    for set in self.tree.descendants_with(statement, Production::SetStmt)? {
                        self.record_reference_rebinding_target(set, bindings, rebound)?;
                    }
                }
                Ok(normal_reaches)
            }
            _ => {
                // A value initializer can contain statement blocks below a
                // `let`. Its successful `give` continues this statement, so
                // conservatively retain each reference rebind in that
                // expression without inventing a control edge elsewhere.
                if normal_reaches {
                    for set in self.tree.descendants_with(statement, Production::SetStmt)? {
                        self.record_reference_rebinding_target(set, bindings, rebound)?;
                    }
                }
                Ok(normal_reaches)
            }
        }
    }

    /// Solves the positive, conjunction-only validity equations for one loop
    /// simultaneously. Invalidity propagates through every local dependency;
    /// a valid local SCC collapses to the finite set of still-open outer-loop
    /// dependencies it reaches. There is no iteration budget: each pass
    /// removes a local possibility or adds one finite dependency.
    fn loop_reference_resolutions(
        &self,
        equations: &[LoopReferenceEquation],
        backedge: &HashMap<DeclarationId, LocalBinding>,
        has_backedge: bool,
    ) -> Result<HashMap<LoopReferenceToken, LoopReferenceResolution>, CheckStop> {
        let local = equations
            .iter()
            .map(|equation| equation.token)
            .collect::<HashSet<_>>();
        let mut results = HashMap::new();
        let mut local_dependencies: HashMap<LoopReferenceToken, Vec<LoopReferenceToken>> =
            HashMap::new();
        for equation in equations {
            let mut invalid = match &equation.entry_validity {
                ReferenceValidity::Valid => None,
                ReferenceValidity::Invalid(event) => Some(event.clone()),
            };
            let mut dependencies = equation.entry_dependencies.clone();
            let mut preservations = equation.entry_preservations.clone();
            if has_backedge {
                let reference = backedge
                    .get(&equation.declaration)
                    .and_then(|binding| binding.reference.as_ref())
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                if invalid.is_none()
                    && let ReferenceValidity::Invalid(event) = &reference.validity
                {
                    invalid = Some(event.clone());
                }
                for preservation in &reference.preservations {
                    if !preservations.contains(preservation) {
                        preservations.push(preservation.clone());
                    }
                }
                for dependency in reference.loop_dependencies() {
                    if !dependencies.contains(dependency) {
                        dependencies.push(*dependency);
                    }
                }
            }
            let (inside, outside): (Vec<_>, Vec<_>) = dependencies
                .into_iter()
                .partition(|dependency| local.contains(dependency));
            local_dependencies.insert(equation.token, inside);
            results.insert(
                equation.token,
                LoopReferenceResolution {
                    invalid,
                    dependencies: outside,
                    preservations,
                },
            );
        }

        loop {
            let mut changed = false;
            for equation in equations {
                if results
                    .get(&equation.token)
                    .is_some_and(|result| result.invalid.is_some())
                {
                    continue;
                }
                let invalid = local_dependencies
                    .get(&equation.token)
                    .into_iter()
                    .flatten()
                    .find_map(|dependency| {
                        results
                            .get(dependency)
                            .and_then(|result| result.invalid.clone())
                    });
                if let Some(event) = invalid {
                    results
                        .get_mut(&equation.token)
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?
                        .invalid = Some(event);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        loop {
            let mut changed = false;
            for equation in equations {
                if results
                    .get(&equation.token)
                    .is_some_and(|result| result.invalid.is_some())
                {
                    continue;
                }
                let inherited = local_dependencies
                    .get(&equation.token)
                    .into_iter()
                    .flatten()
                    .flat_map(|dependency| {
                        results
                            .get(dependency)
                            .into_iter()
                            .flat_map(|result| result.dependencies.iter().copied())
                    })
                    .collect::<Vec<_>>();
                let inherited_preservations = local_dependencies
                    .get(&equation.token)
                    .into_iter()
                    .flatten()
                    .flat_map(|dependency| {
                        results
                            .get(dependency)
                            .into_iter()
                            .flat_map(|result| result.preservations.iter().cloned())
                    })
                    .collect::<Vec<_>>();
                let result = results
                    .get_mut(&equation.token)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                for preservation in inherited_preservations {
                    if !result.preservations.contains(&preservation) {
                        result.preservations.push(preservation);
                        changed = true;
                    }
                }
                for dependency in inherited {
                    if !result.dependencies.contains(&dependency) {
                        result.dependencies.push(dependency);
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        Ok(results)
    }

    fn resolve_reference_info(
        reference: &mut super::super::references::ReferenceInfo,
        resolutions: &HashMap<LoopReferenceToken, LoopReferenceResolution>,
    ) {
        let mut tokens = resolutions.keys().copied().collect::<Vec<_>>();
        tokens.sort_unstable();
        for token in tokens {
            let Some(resolution) = resolutions.get(&token) else {
                continue;
            };
            match &resolution.invalid {
                Some(event) => reference.resolve_loop_dependency(token, Err(event)),
                None => reference.resolve_loop_dependency(
                    token,
                    Ok((&resolution.dependencies, &resolution.preservations)),
                ),
            }
        }
    }

    fn resolve_loop_binding_state(
        state: &mut HashMap<DeclarationId, LocalBinding>,
        resolutions: &HashMap<LoopReferenceToken, LoopReferenceResolution>,
    ) {
        for binding in state.values_mut() {
            if let Some(reference) = &mut binding.reference {
                Self::resolve_reference_info(reference, resolutions);
            }
        }
    }

    /// Eliminates this loop's header variables from every validity use made
    /// while checking its body. A use whose equation is invalid becomes the
    /// ordinary [REF-2] rejection; a use that still depends on an enclosing
    /// loop remains deferred to that loop. The complete resolution is
    /// installed before any checked function can be published.
    fn resolve_deferred_loop_reference_uses(
        &self,
        resolutions: &HashMap<LoopReferenceToken, LoopReferenceResolution>,
    ) -> Result<(), CheckStop> {
        let pending = std::mem::take(&mut *self.deferred_loop_reference_uses.borrow_mut());
        let mut remaining = Vec::with_capacity(pending.len());
        let mut first_invalid = None;
        for mut deferred in pending {
            let mut dependencies = Vec::new();
            let mut invalid = None;
            for dependency in deferred.dependencies {
                let Some(resolution) = resolutions.get(&dependency) else {
                    if !dependencies.contains(&dependency) {
                        dependencies.push(dependency);
                    }
                    continue;
                };
                if let Some(event) = &resolution.invalid {
                    invalid.get_or_insert_with(|| event.clone());
                } else {
                    for preservation in &resolution.preservations {
                        if !deferred.preservations.contains(preservation) {
                            deferred.preservations.push(preservation.clone());
                        }
                    }
                    for inherited in &resolution.dependencies {
                        if !dependencies.contains(inherited) {
                            dependencies.push(*inherited);
                        }
                    }
                }
            }
            dependencies.sort_unstable();
            if let Some(event) = invalid {
                first_invalid.get_or_insert((deferred.node, deferred.declaration, event));
            } else if !dependencies.is_empty() {
                deferred.dependencies = dependencies;
                remaining.push(deferred);
            } else {
                self.demand_reference_preservations(
                    deferred.node,
                    deferred.declaration,
                    &deferred.preservations,
                )?;
            }
        }
        *self.deferred_loop_reference_uses.borrow_mut() = remaining;
        let Some((node, declaration, event)) = first_invalid else {
            return Ok(());
        };
        self.issue_node(
            SemanticRule::Ref2,
            node,
            SemanticIssueKind::InvalidReferenceUse {
                binder: self.declaration_spelling(declaration)?,
                event: event.phrase(),
                mechanical_fix: super::super::references::REF2_FORM_AGAIN,
            },
        )
    }

    fn form_loop_invariants(
        &self,
        nodes: Vec<NodeId>,
        loop_id: CheckedLoopId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        allowed_values: &HashSet<DeclarationId>,
        function: &FunctionSignature,
        loop_depth: usize,
    ) -> Result<Vec<CheckedLoopInvariant>, CheckStop> {
        let mut names = HashSet::new();
        let mut invariants = Vec::with_capacity(nodes.len());
        for node in nodes {
            invariants.push(self.check_loop_invariant(
                node,
                loop_id,
                bindings,
                allowed_values,
                function,
                loop_depth,
                &mut names,
            )?);
        }
        Ok(invariants)
    }

    pub(super) fn check_counted_range(
        &self,
        function: &FunctionSignature,
        node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        counters: &mut ControlCounters<'_>,
        scope: ControlScope<'_>,
    ) -> Result<StatementResult, CheckStop> {
        let binding_node = self
            .tree
            .first_child_with(node, Production::ForBinding)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let endpoints = self.tree.children_with(binding_node, Production::Atom)?;
        let [lower_node, upper_node] = endpoints.as_slice() else {
            return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
        };

        // [FN-1] endpoint evaluation is source ordered and happens before the
        // counted binder exists. Each check can therefore observe only the
        // preceding endpoint's ordinary ownership/effect consequences.
        let lower =
            self.check_counted_endpoint(function, *lower_node, bindings, scope.loops.len())?;
        let upper =
            self.check_counted_endpoint(function, *upper_node, bindings, scope.loops.len())?;
        let effects = lower.effects.union(upper.effects);

        let label = self
            .optional_declaration_at(node, DeclarationRole::LoopLabel)?
            .map(crate::DeclarationRecord::id);
        let binder_declaration =
            self.declaration_at(binding_node, DeclarationRole::CountedBinder)?;
        let binder_declaration_id = binder_declaration.id();
        let id = Self::allocate_loop(counters.next_loop)?;
        let binder = Self::allocate_binding(counters.next_binding)?;
        counters
            .binding_names
            .push(binder_declaration.spelling().to_owned());

        let base_bindings = bindings.clone();
        let base_keys = base_bindings.keys().copied().collect::<Vec<_>>();
        let preserved = base_keys.iter().copied().collect::<HashSet<_>>();
        let invariant_nodes = self.tree.children_with(node, Production::HeaderInvariant)?;
        let executable_statements = self.tree.children_with(node, Production::Stmt)?;
        let rebound =
            self.continuing_reference_rebindings(&executable_statements, &base_bindings)?;
        let mut header_bindings = base_bindings.clone();
        let reference_equations =
            self.enter_loop_reference_header(id, &rebound.holders, &mut header_bindings)?;
        if header_bindings
            .insert(
                binder_declaration_id,
                LocalBinding {
                    binding: binder,
                    declaration: binder_declaration_id,
                    mode: CheckedMode::Own,
                    ty: CheckedType::Integer(IntegerType::U64),
                    live: true,
                    loop_depth: scope.loops.len() + 1,
                    compiler_updated: true,
                    reference: None,
                    refinement_witnesses: Vec::new(),
                },
            )
            .is_some()
        {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        let mut body_bindings = header_bindings.clone();
        let header_keys = header_bindings.keys().copied().collect::<Vec<_>>();
        let header_preserved = header_keys.iter().copied().collect::<HashSet<_>>();
        let mut nested_loops = scope.loops.to_vec();
        nested_loops.push(LoopContext {
            id,
            reference_rebindings: rebound.sites,
            label_declaration: label,
            preserved: preserved.clone(),
        });

        let allowed_invariant_values = header_keys.iter().copied().collect::<HashSet<_>>();
        let invariants = self.form_loop_invariants(
            invariant_nodes,
            id,
            &body_bindings,
            &allowed_invariant_values,
            function,
            scope.loops.len(),
        )?;
        let mut checked = self.check_block(
            function,
            &executable_statements,
            &mut body_bindings,
            counters,
            ControlScope {
                loops: &nested_loops,
                give_context: scope.give_context,
            },
        )?;
        // [OWN-11, REF-2] the body is an ordinary block whose own bindings
        // begin and end with one iteration, so a reference whose path starts
        // at one of them is invalid on the backedge, before the carried-state
        // comparison, and on every edge leaving the body.
        let leaving = Self::bindings_leaving_scope(&body_bindings, &header_keys);
        Self::invalidate_references_leaving_scope(&mut body_bindings, &leaving);
        for state in &mut checked.give_states {
            Self::invalidate_references_leaving_scope(state, &leaving);
        }
        for state in &mut checked.break_states {
            state.invalidate_references_leaving_scope(&leaving);
        }
        if let Some(context) = scope.give_context
            && !checked.give_states.is_empty()
        {
            context.invalidate_reference_roots_leaving_scope(&leaving);
        }
        self.judge_backedge_liveness(node, &header_keys, &header_bindings, &body_bindings)?;
        if checked.can_continue
            && header_keys.iter().any(|key| {
                !self.loop_binding_agrees(header_bindings.get(key), body_bindings.get(key))
            })
        {
            return self.unsupported(UnsupportedSemanticFeature::OwnershipJoin, node);
        }

        let resolutions = self.loop_reference_resolutions(
            &reference_equations,
            &body_bindings,
            checked.can_continue,
        )?;
        Self::resolve_loop_binding_state(&mut body_bindings, &resolutions);
        for state in &mut checked.give_states {
            Self::resolve_loop_binding_state(state, &resolutions);
        }
        for state in &mut checked.break_states {
            Self::resolve_loop_binding_state(&mut state.bindings, &resolutions);
        }
        if let Some(context) = scope.give_context
            && let Some(reference) = context.delivered_reference.borrow_mut().as_mut()
        {
            Self::resolve_reference_info(reference, &resolutions);
        }
        self.resolve_deferred_loop_reference_uses(&resolutions)?;

        let backedge_drops = if checked.can_continue {
            self.live_affine_drops(&body_bindings, &header_preserved, node)?
        } else {
            Vec::new()
        };

        // Unlike an ordinary loop, exhaustion is an executable continuation
        // input even when no local break is written. Local breaks join it;
        // breaks targeting an enclosing loop keep escaping normally.
        let mut exhaustion_bindings = header_bindings;
        Self::resolve_loop_binding_state(&mut exhaustion_bindings, &resolutions);
        let mut continuation_states = vec![exhaustion_bindings];
        let mut continuation_labels = vec!["the loop's exhausted edge".to_owned()];
        let mut escaping_break_states = Vec::new();
        for state in checked.break_states {
            if state.target == id {
                continuation_states.push(state.bindings);
                continuation_labels.push("a `break` edge of this loop".to_owned());
            } else {
                escaping_break_states.push(state);
            }
        }
        self.join_states(
            &base_keys,
            &continuation_states,
            &continuation_labels,
            node,
            bindings,
        )?;

        Ok(StatementResult {
            statement: CheckedStatement::CountedRange {
                id,
                node_path: self.tree.path(node)?.clone(),
                binder,
                lower: lower.expression,
                upper: Box::new(upper.expression),
                invariants,
                body: checked.statements,
                backedge_drops,
            },
            can_continue: true,
            effects: effects.union(checked.effects),
            all_paths_deliver: false,
            direct_give: false,
            give_states: checked.give_states,
            break_states: escaping_break_states,
        })
    }

    /// [OWN-11] the per-iteration judgment, which is [LIV-1]'s liveness
    /// agreement read at this loop's head.
    ///
    /// The entering edge and the backedge are the loop head's two
    /// predecessors. A binding declared outside the body whose status differs
    /// between them would make one iteration start in a state the previous one
    /// did not leave, which is exactly what the prohibition on moving an outer
    /// binding into a loop body existed to prevent; a body that moves such a
    /// binding and reinitializes it before the backedge [LIV-2] agrees and is
    /// admitted.
    ///
    /// The backedge is the structural one [FN-1, LIV-1]: it is read whether or
    /// not this body's own fallthrough is executable, so a body that consumes
    /// an outer binding and then leaves by `break` or `return` is judged on
    /// the state it reached, exactly as one that falls through is. Reading
    /// reachability here instead would admit a one-iteration consume that this
    /// clause has always refused, which is a language change and not this
    /// rule's.
    fn judge_backedge_liveness(
        &self,
        node: NodeId,
        keys: &[DeclarationId],
        entering: &HashMap<DeclarationId, LocalBinding>,
        backedge: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<(), CheckStop> {
        for key in keys {
            let (Some(entering), Some(backedge)) = (entering.get(key), backedge.get(key)) else {
                continue;
            };
            if entering.live == backedge.live {
                continue;
            }
            return self.issue_node(
                SemanticRule::Own11,
                node,
                SemanticIssueKind::MoveOuterBindingInLoop {
                    binding: self.declaration_spelling(*key)?,
                    mechanical_fix: "one iteration must leave every outer binding in the status \
                                     the next one starts from: commit a value back into it before \
                                     the backedge, or declare and consume it inside the body",
                },
            );
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn check_loop_invariant(
        &self,
        node: NodeId,
        loop_id: CheckedLoopId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        allowed_values: &HashSet<DeclarationId>,
        function: &FunctionSignature,
        loop_depth: usize,
        names: &mut HashSet<String>,
    ) -> Result<CheckedLoopInvariant, CheckStop> {
        let declaration = self.declaration_at(node, DeclarationRole::Invariant)?;
        let identifiers = self.tree.direct_identifiers(node)?;
        let [name_token] = identifiers.as_slice() else {
            return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
        };
        let name = std::str::from_utf8(self.tree.token_bytes(*name_token)?)
            .map_err(|_| SemanticCompilerFailure::InvalidSourceEncoding)?
            .to_owned();
        if !names.insert(name.clone()) {
            return self.invalid_invariant(
                node,
                "a loop contains two invariants with the same name",
                "give every invariant in this loop a distinct name",
            );
        }
        let relation = self.check_ordered_affine_relation(
            node,
            bindings,
            allowed_values,
            function,
            loop_depth,
            AffineProofOwner::InvariantTarget,
        )?;

        Ok(CheckedLoopInvariant {
            loop_id,
            declaration: declaration.id(),
            name,
            relation,
        })
    }

    pub(super) fn invalid_invariant<ResultValue>(
        &self,
        node: NodeId,
        reason: &'static str,
        mechanical_fix: &'static str,
    ) -> Result<ResultValue, CheckStop> {
        self.issue_node(
            SemanticRule::Inv1,
            node,
            SemanticIssueKind::InvalidInvariant {
                reason,
                mechanical_fix,
            },
        )
    }

    fn check_counted_endpoint(
        &self,
        function: &FunctionSignature,
        node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        loop_depth: usize,
    ) -> Result<TypedExpression, CheckStop> {
        let required = CheckedType::Integer(IntegerType::U64);
        if self.direct_counted_endpoint_holder_requires_deref(node, bindings, required)? {
            return self.issue_node(
                SemanticRule::Type7,
                node,
                SemanticIssueKind::MissingDereference {
                    mechanical_fix: "write `deref(holder)`",
                },
            );
        }
        // TYPE-7 precedes the endpoint's TYPE-5 exact-value judgment. Use the
        // consuming-position atom path so a box or borrow holder reaches
        // that exclusive judgment instead of stopping first at OWN-1's bare
        // affine spelling rule.
        let endpoint = self.check_consuming_atom(function, node, bindings, loop_depth)?;
        if self.reads_implicitly_through_holder(
            endpoint.reference_value,
            endpoint.expression.ty(),
            RequiredReferent::Exact(required),
        )? {
            return self.issue_node(
                SemanticRule::Type7,
                node,
                SemanticIssueKind::MissingDereference {
                    mechanical_fix: "write `deref(holder)`",
                },
            );
        }
        if endpoint.mode != CheckedMode::Own || endpoint.expression.ty() != required {
            return self.issue_node(
                SemanticRule::Type5,
                node,
                SemanticIssueKind::type_mismatch(
                    format!("own {}", self.checked_type_name(required)?),
                    self.checked_value_name(endpoint.mode, endpoint.expression.ty())?,
                ),
            );
        }
        if !self.counted_endpoint_is_term_or_constant(node, &endpoint.expression)? {
            return self.issue_node(
                SemanticRule::Ent2,
                node,
                SemanticIssueKind::InvalidCountedEndpoint {
                    mechanical_fix: "bind the computed u64 value with one preceding ordinary let and use that term as the endpoint",
                },
            );
        }
        Ok(endpoint)
    }

    /// After TYPE-5, the only atom shapes still capable of producing `own
    /// u64` are a literal or a place. ENT-2 admits the literal, a measure
    /// [MSR-1] or other readonly field whose subscripts, if any, all carry an
    /// offset that is itself a tracked place or a constant, and a tracked
    /// place with field/deref wrappers but no subscript at any depth.
    fn counted_endpoint_is_term_or_constant(
        &self,
        node: NodeId,
        endpoint: &CheckedExpression,
    ) -> Result<bool, CheckStop> {
        let term = match endpoint {
            CheckedExpression::ArrayMeasure { .. } | CheckedExpression::RangeMeasure { .. } => {
                Some(SubscriptedTerm::Represented)
            }
            CheckedExpression::BufferMeasure { root, .. } => root.subscripted_term(),
            CheckedExpression::ContainerMeasure { root, .. } => root.subscripted_term(),
            CheckedExpression::RangeElementMeasure { place, .. } => place.subscripted_term(),
            CheckedExpression::ReadStorage { root, .. } => root.readonly_field_term(&self.nominals),
            CheckedExpression::RangeIndex { place, .. } => {
                place.readonly_field_term(&self.nominals)
            }
            _ => None,
        };
        match term {
            Some(SubscriptedTerm::Represented) => return Ok(true),
            // A tracked-place offset with projections is admitted by ENT-2
            // but names no captured value in this compiler: the place is not
            // rejected, it is not represented [DIAG-1].
            Some(SubscriptedTerm::Unrepresented) => {
                return self.unsupported(UnsupportedSemanticFeature::CompositeValues, node);
            }
            // A measure over a place with any other offset is no term.
            None if matches!(
                endpoint,
                CheckedExpression::BufferMeasure { .. }
                    | CheckedExpression::ContainerMeasure { .. }
                    | CheckedExpression::RangeElementMeasure { .. }
            ) =>
            {
                return Ok(false);
            }
            None => {}
        }
        let Some(place) = self.tree.first_child_with(node, Production::Place)? else {
            // TYPE-5 has already excluded a borrow expression, so the
            // remaining non-place atom is an integer literal constant.
            return Ok(true);
        };
        self.counted_endpoint_place_is_term(place)
    }

    fn counted_endpoint_place_is_term(&self, place: NodeId) -> Result<bool, CheckStop> {
        for suffix in self.tree.children_with(place, Production::Psuffix)? {
            if self.subscript_offset(suffix)?.is_some() {
                return Ok(false);
            }
        }
        let pbase = self
            .tree
            .first_child_with(place, Production::Pbase)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let Some(inner) = self.tree.first_child_with(pbase, Production::Place)? else {
            return Ok(true);
        };
        self.counted_endpoint_place_is_term(inner)
    }

    /// TYPE-7 is definitionally earlier than both OWN-1's holder spelling and
    /// OWN-11's outer-affine move check. Inspect a live direct holder before
    /// those generic place checks so an endpoint that plainly needs `deref`
    /// keeps the rule's exclusive attribution even inside another loop.
    fn direct_counted_endpoint_holder_requires_deref(
        &self,
        node: NodeId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        required: CheckedType,
    ) -> Result<bool, CheckStop> {
        let Some(place) = self.tree.first_child_with(node, Production::Place)? else {
            return Ok(false);
        };
        let Some(pbase) = self.tree.first_child_with(place, Production::Pbase)? else {
            return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
        };
        if !self.tree.children(pbase)?.is_empty()
            || !self
                .tree
                .children_with(place, Production::Psuffix)?
                .is_empty()
        {
            return Ok(false);
        }
        let usage = self.use_at(pbase, LexicalUseRole::PlaceBase)?;
        let ResolvedTarget::Source {
            declaration,
            class: DeclarationClass::Value,
        } = usage.target()
        else {
            return Ok(false);
        };
        let Some(local) = bindings.get(&declaration) else {
            return Ok(false);
        };
        if !local.live {
            return Ok(false);
        }
        self.reads_implicitly_through_holder(
            local.mode != CheckedMode::Own,
            local.ty,
            RequiredReferent::Exact(required),
        )
    }

    pub(super) fn check_loop(
        &self,
        function: &FunctionSignature,
        node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        counters: &mut ControlCounters<'_>,
        scope: ControlScope<'_>,
    ) -> Result<StatementResult, CheckStop> {
        let declaration = self
            .optional_declaration_at(node, DeclarationRole::LoopLabel)?
            .map(crate::DeclarationRecord::id);
        let id = Self::allocate_loop(counters.next_loop)?;
        let base_bindings = bindings.clone();
        let base_keys = base_bindings.keys().copied().collect::<Vec<_>>();
        let preserved = base_keys.iter().copied().collect::<HashSet<_>>();
        let invariant_nodes = self.tree.children_with(node, Production::HeaderInvariant)?;
        let executable_statements = self.tree.children_with(node, Production::Stmt)?;
        let rebound =
            self.continuing_reference_rebindings(&executable_statements, &base_bindings)?;
        let mut nested_loops = scope.loops.to_vec();
        nested_loops.push(LoopContext {
            id,
            reference_rebindings: rebound.sites,
            label_declaration: declaration,
            preserved: preserved.clone(),
        });
        let mut header_bindings = base_bindings.clone();
        let reference_equations =
            self.enter_loop_reference_header(id, &rebound.holders, &mut header_bindings)?;
        let mut body_bindings = header_bindings.clone();
        let allowed_invariant_values = base_keys.iter().copied().collect::<HashSet<_>>();
        let invariants = self.form_loop_invariants(
            invariant_nodes,
            id,
            &body_bindings,
            &allowed_invariant_values,
            function,
            scope.loops.len(),
        )?;
        let mut checked = self.check_block(
            function,
            &executable_statements,
            &mut body_bindings,
            counters,
            ControlScope {
                loops: &nested_loops,
                give_context: scope.give_context,
            },
        )?;
        // [OWN-11, REF-2] the body is an ordinary block whose own bindings
        // begin and end with one iteration, so a reference whose path starts
        // at one of them is invalid on the backedge, before the carried-state
        // comparison, and on every edge leaving the body.
        let leaving = Self::bindings_leaving_scope(&body_bindings, &base_keys);
        Self::invalidate_references_leaving_scope(&mut body_bindings, &leaving);
        for state in &mut checked.give_states {
            Self::invalidate_references_leaving_scope(state, &leaving);
        }
        for state in &mut checked.break_states {
            state.invalidate_references_leaving_scope(&leaving);
        }
        if let Some(context) = scope.give_context
            && !checked.give_states.is_empty()
        {
            context.invalidate_reference_roots_leaving_scope(&leaving);
        }
        self.judge_backedge_liveness(node, &base_keys, &header_bindings, &body_bindings)?;
        if checked.can_continue
            && base_keys.iter().any(|key| {
                !self.loop_binding_agrees(header_bindings.get(key), body_bindings.get(key))
            })
        {
            return self.unsupported(UnsupportedSemanticFeature::OwnershipJoin, node);
        }

        let resolutions = self.loop_reference_resolutions(
            &reference_equations,
            &body_bindings,
            checked.can_continue,
        )?;
        Self::resolve_loop_binding_state(&mut body_bindings, &resolutions);
        for state in &mut checked.give_states {
            Self::resolve_loop_binding_state(state, &resolutions);
        }
        for state in &mut checked.break_states {
            Self::resolve_loop_binding_state(&mut state.bindings, &resolutions);
        }
        if let Some(context) = scope.give_context
            && let Some(reference) = context.delivered_reference.borrow_mut().as_mut()
        {
            Self::resolve_reference_info(reference, &resolutions);
        }
        self.resolve_deferred_loop_reference_uses(&resolutions)?;

        let mut own_break_states = Vec::new();
        let mut own_break_labels: Vec<String> = Vec::new();
        let mut escaping_break_states = Vec::new();
        for state in checked.break_states {
            if state.target == id {
                own_break_states.push(state.bindings);
                own_break_labels.push("a `break` edge of this loop".to_owned());
            } else {
                escaping_break_states.push(state);
            }
        }
        // An ordinary loop with no break resolved to itself has no executable
        // continuation input. `join_states` deliberately leaves the
        // structurally retained continuation bindings unchanged for that
        // empty join; ENT-5's proof flow marks the same continuation
        // contradictory, and lowering emits an unreachable exit block.
        self.join_states(
            &base_keys,
            &own_break_states,
            &own_break_labels,
            node,
            bindings,
        )?;
        let backedge_drops = if checked.can_continue {
            self.live_affine_drops(&body_bindings, &preserved, node)?
        } else {
            Vec::new()
        };

        Ok(StatementResult {
            statement: CheckedStatement::Loop {
                id,
                invariants,
                body: checked.statements,
                backedge_drops,
            },
            // FN-1 conservatively gives every loop a normal successor; the
            // executable path reaches it only through a checked break edge.
            can_continue: true,
            effects: checked.effects,
            all_paths_deliver: false,
            direct_give: false,
            give_states: checked.give_states,
            break_states: escaping_break_states,
        })
    }

    pub(super) fn check_break(
        &self,
        node: NodeId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        scope: ControlScope<'_>,
    ) -> Result<StatementResult, CheckStop> {
        let uses = self.uses_at_ordered(node, LexicalUseRole::BreakLabel)?;
        let target = match uses.as_slice() {
            [] => scope.loops.last().ok_or_else(|| {
                self.issue_value(
                    SemanticRule::Fn1,
                    node,
                    SemanticIssueKind::BreakOutsideLoop {
                        mechanical_fix: "move `break;` inside a loop or remove it",
                    },
                )
            })?,
            [usage] => {
                let ResolvedTarget::Source {
                    declaration,
                    class: DeclarationClass::Label,
                } = usage.target()
                else {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                };
                scope
                    .loops
                    .iter()
                    .rev()
                    .find(|context| context.label_declaration == Some(declaration))
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?
            }
            _ => return Err(SemanticCompilerFailure::InvalidResolution.into()),
        };
        let all_paths_deliver = scope
            .give_context
            .is_some_and(|context| context.enclosing_loops.contains(&target.id));
        Ok(StatementResult {
            statement: CheckedStatement::Break {
                target: target.id,
                drops: self.live_affine_drops(bindings, &target.preserved, node)?,
            },
            can_continue: false,
            effects: EffectSet::NONE,
            all_paths_deliver,
            direct_give: false,
            give_states: Vec::new(),
            break_states: vec![BreakState {
                target: target.id,
                bindings: bindings.clone(),
            }],
        })
    }

    fn allocate_loop(next_loop: &mut u32) -> Result<CheckedLoopId, CheckStop> {
        let id = CheckedLoopId(*next_loop);
        *next_loop = next_loop
            .checked_add(1)
            .ok_or(SemanticCompilerFailure::CounterOverflow)?;
        Ok(id)
    }
}

impl BreakState {
    /// [REF-2] the scope of the local variables a break edge leaves ends
    /// there, so a reference whose path starts at one of them is invalid on
    /// this edge.
    pub(super) fn invalidate_references_leaving_scope(&mut self, leaving: &[BindingId]) {
        Checker::invalidate_references_leaving_scope(&mut self.bindings, leaving);
    }
}
