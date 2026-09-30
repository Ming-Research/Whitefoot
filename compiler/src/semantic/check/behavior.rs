//! Function-kind parameters and hygienic named argument groups [FN-2..FN-6].
//! These identities exist only during checking and monomorphization.

mod contracts;

use crate::semantic::check::CheckContext;
use crate::semantic::check::{DeclarationInventory, TypeContext};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use crate::syntax::NodeId;
use crate::{
    DeclarationClass, DeclarationId, DeclarationRole, LexicalUseRole, Production, ResolvedTarget,
    SemanticCompilerFailure, SemanticIssueKind, SemanticRule,
};

use super::super::model::CheckedStatePath;
use super::super::model::FunctionId;
use super::generics::{
    GenericArgument, GenericParameter, GenericParameterKey, GenericSubstitution,
};
use super::{CheckStop, Checker, FunctionSignature, FunctionTemplate};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct FunctionReferenceId(u32);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum FunctionArgument {
    Parameter(GenericParameterKey),
    Source {
        reference: FunctionReferenceId,
        concrete: bool,
    },
}

impl FunctionArgument {
    pub(super) fn is_concrete(self) -> bool {
        matches!(self, Self::Source { concrete: true, .. })
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct FunctionReference {
    pub(super) declaration: DeclarationId,
    pub(super) substitution: GenericSubstitution,
}

#[derive(Clone)]
pub(super) struct FormalGroup {
    pub(super) parameters: Vec<GenericParameter>,
    pub(super) members: Vec<(DeclarationId, NodeId, String)>,
}

#[derive(Clone)]
pub(super) struct ActualGroup {
    pub(super) node: NodeId,
    pub(super) formal: DeclarationId,
    pub(super) application: NodeId,
    pub(super) regions: Vec<DeclarationId>,
    pub(super) bindings: Vec<NodeId>,
}

struct BindingSite {
    substitution: GenericSubstitution,
    key: GenericParameterKey,
    source: NodeId,
}

#[derive(Default)]
pub(super) struct BehaviorInventory {
    pub(super) formals: HashMap<DeclarationId, FormalGroup>,
    pub(super) actuals: HashMap<DeclarationId, ActualGroup>,
    references: RefCell<Vec<FunctionReference>>,
    binding_sites: RefCell<Vec<BindingSite>>,
    pub(super) declaration_arguments: Vec<FunctionArgument>,
}

#[derive(Clone, Copy)]
pub(super) enum WrittenArgument {
    Source(NodeId),
    Expanded {
        source: NodeId,
        value: GenericArgument,
    },
}

impl WrittenArgument {
    pub(super) fn source(self) -> NodeId {
        match self {
            Self::Source(node) | Self::Expanded { source: node, .. } => node,
        }
    }
}

impl<'unit> Checker<'_, 'unit> {
    pub(super) fn formal_signature(
        &mut self,
        check_context: &CheckContext<'_>,
        key: GenericParameterKey,
        context: &GenericSubstitution,
        id: FunctionId,
    ) -> Result<FunctionSignature, CheckStop> {
        let template = self.types.declarations.formal_template(key)?;
        let substitution = self
            .types
            .formal_substitution(check_context, key, context)?;
        let mut signature =
            self.build_function_signature(check_context, &template, substitution, id)?;
        signature.formal_parameter = Some(key);
        Ok(signature)
    }

    pub(super) fn ensure_formal_nominals(
        &mut self,
        check_context: &CheckContext<'_>,
        key: GenericParameterKey,
        context: &GenericSubstitution,
    ) -> Result<(), CheckStop> {
        let template = self.types.declarations.formal_template(key)?;
        let substitution = self
            .types
            .formal_substitution(check_context, key, context)?;
        self.ensure_nominals_in_function(check_context, template.node, &substitution)
    }

    pub(super) fn symbolic_behavior_signature(
        &mut self,
        check_context: &CheckContext<'_>,
        declaration: DeclarationId,
    ) -> Result<FunctionSignature, CheckStop> {
        let key = GenericParameterKey::Source(declaration);
        let context = self.types.symbolic_formal_context(check_context, key)?;
        self.ensure_formal_nominals(check_context, key, &context)?;
        self.formal_signature(check_context, key, &context, FunctionId(u32::MAX))
    }

    pub(super) fn validate_formal_declarations(
        &mut self,
        check_context: &CheckContext<'_>,
    ) -> Result<(), CheckStop> {
        let checkpoint = self.types.view.clone();
        let members = self
            .types
            .declarations
            .resolved
            .declarations()
            .iter()
            .filter(|declaration| declaration.role() == DeclarationRole::FunctionParameter)
            .map(|declaration| declaration.id())
            .collect::<Vec<_>>();
        for member in members {
            // Formation is required even without a binding group or a member call.
            // The transient signature supplies no executable function or
            // contract theorem to the concrete inventory.
            let signature = self.symbolic_behavior_signature(check_context, member)?;
            self.check_formal_contract_formation(check_context, &signature)?;
        }
        // A formal may name both concrete types and types containing its
        // owner's symbolic parameters. Only the concrete types belong to
        // the executable inventory after the transient signatures expire.
        self.retain_concrete_nominals_since(check_context, checkpoint)
    }

    pub(super) fn materialize_actual_groups(
        &mut self,
        check_context: &CheckContext<'_>,
        tolerate_source_failure: bool,
    ) -> Result<(), CheckStop> {
        let mut groups = self
            .types
            .behavior
            .actuals
            .values()
            .cloned()
            .collect::<Vec<_>>();
        groups.sort_by_key(|group| group.node.index());
        for group in groups {
            let result = (|| {
                let context = Checker::actual_declaration_context(&group)?;
                self.ensure_nominals_in_node(check_context, group.application, &context)?;
                let formal = self
                    .types
                    .behavior
                    .formals
                    .get(&group.formal)
                    .cloned()
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                let substitution = self.generic_substitution(
                    check_context,
                    group.application,
                    &formal.parameters,
                    &context,
                    SemanticRule::Fn3,
                    0,
                )?;
                for ((_, node, _), binding) in formal.members.iter().zip(&group.bindings) {
                    self.ensure_nominals_in_function(check_context, *node, &substitution)?;
                    self.ensure_nominals_in_node(check_context, *binding, &context)?;
                    let argument =
                        self.parse_function_binding(check_context, *binding, &context)?;
                    self.materialize_function_argument(check_context, argument)?;
                    if !self
                        .types
                        .behavior
                        .declaration_arguments
                        .contains(&argument)
                    {
                        self.types.behavior.declaration_arguments.push(argument);
                    }
                }
                Ok(())
            })();
            match result {
                Err(
                    CheckStop::Issue(_)
                    | CheckStop::Unsupported(_)
                    | CheckStop::PostconditionPrerequisiteUnavailable,
                ) if tolerate_source_failure => {}
                result => result?,
            }
        }
        Ok(())
    }

    fn actual_declaration_context(group: &ActualGroup) -> Result<GenericSubstitution, CheckStop> {
        Ok(GenericSubstitution::default().with_regions(
            group
                .regions
                .iter()
                .map(|region| (*region, *region))
                .collect(),
        ))
    }

    pub(super) fn materialize_function_argument(
        &mut self,
        check_context: &CheckContext<'_>,
        argument: FunctionArgument,
    ) -> Result<FunctionId, CheckStop> {
        match argument {
            FunctionArgument::Source { reference, .. } => {
                let value = self.types.function_reference(reference)?;
                self.types.activate_substitution(&value.substitution)?;
                let template = *self
                    .types
                    .templates_by_declaration
                    .get(&value.declaration)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                self.instantiate_function_signature(check_context, template, value.substitution)
            }
            FunctionArgument::Parameter(key) => {
                let context = self.types.symbolic_formal_context(check_context, key)?;
                let template = self.types.declarations.formal_template(key)?;
                let substitution = self
                    .types
                    .formal_substitution(check_context, key, &context)?;
                if let Some(id) =
                    self.types
                        .function_instance(template.declaration, &substitution, Some(key))
                {
                    self.types.activate_function(id)?;
                    return Ok(id);
                }
                self.ensure_nominals_in_function(check_context, template.node, &substitution)?;
                let id = FunctionId(
                    u32::try_from(self.types.signatures.len())
                        .map_err(|_| SemanticCompilerFailure::CounterOverflow)?,
                );
                let signature = self.formal_signature(check_context, key, &context, id)?;
                self.types.retain_signature(signature)
            }
        }
    }

    pub(super) fn parse_function_argument(
        &mut self,
        check_context: &CheckContext<'_>,
        argument: NodeId,
        caller: &GenericSubstitution,
    ) -> Result<FunctionArgument, CheckStop> {
        let Some(reference) = self
            .types
            .declarations
            .tree
            .first_child_with(argument, Production::FunctionArg)?
        else {
            return self.types.declarations.behavior_mismatch(
                SemanticRule::Fn2,
                argument,
                "a function argument writes `fn` and an explicit source function",
            );
        };
        self.parse_function_binding(check_context, reference, caller)
    }

    fn parse_function_binding(
        &mut self,
        check_context: &CheckContext<'_>,
        node: NodeId,
        caller: &GenericSubstitution,
    ) -> Result<FunctionArgument, CheckStop> {
        let callee = self
            .types
            .declarations
            .tree
            .first_child_with(node, Production::Callee)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        if let Some(application) = self.types.declarations.tree.callee_application(callee)? {
            if self.types.declarations.tree.argument_list(node)?.is_some() {
                return self.types.declarations.behavior_mismatch(
                    SemanticRule::Fn2,
                    node,
                    "a group member has an already instantiated signature",
                );
            }
            let member = self
                .types
                .declarations
                .deferred_use_at(callee, crate::DeferredUseRole::FunctionMember)?
                .spelling();
            return self.group_member_argument(check_context, application, member, caller);
        }
        let usage = self.types.declarations.use_at(
            check_context,
            callee,
            LexicalUseRole::FunctionBinding,
        )?;
        match usage.target() {
            ResolvedTarget::Source {
                declaration,
                class: DeclarationClass::FunctionParameter,
            } => {
                if self.types.declarations.tree.argument_list(node)?.is_some() {
                    return self.types.declarations.behavior_mismatch(
                        SemanticRule::Fn2,
                        node,
                        "a function parameter has an already instantiated signature",
                    );
                }
                caller
                    .function_argument(GenericParameterKey::Source(declaration))
                    .ok_or_else(|| SemanticCompilerFailure::InvalidResolution.into())
            }
            ResolvedTarget::Source {
                declaration,
                class: DeclarationClass::Function,
            } => {
                let written = self
                    .types
                    .declarations
                    .resolved
                    .declarations()
                    .iter()
                    .find(|candidate| candidate.id() == declaration)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                let source = self
                    .types
                    .declarations
                    .tree
                    .node_with_path(written.origin().node())
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                let parameters = self.types.parse_generic_parameters(check_context, source)?;
                let substitution = self.generic_substitution(
                    check_context,
                    node,
                    &parameters,
                    caller,
                    SemanticRule::Fn2,
                    0,
                )?;
                self.types
                    .intern_function_reference(declaration, &substitution)
            }
            _ => self.types.declarations.behavior_mismatch(
                SemanticRule::Fn4,
                node,
                "a behavior argument names a source function or function parameter",
            ),
        }
    }

    pub(super) fn group_member_argument(
        &mut self,
        check_context: &CheckContext<'_>,
        application: NodeId,
        member: &str,
        caller: &GenericSubstitution,
    ) -> Result<FunctionArgument, CheckStop> {
        let usage = self.types.declarations.use_at(
            check_context,
            application,
            LexicalUseRole::FormalGroup,
        )?;
        if let ResolvedTarget::Source {
            declaration,
            class: DeclarationClass::Binding,
        } = usage.target()
        {
            let actual = self
                .types
                .behavior
                .actuals
                .get(&declaration)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            let formal = self
                .types
                .behavior
                .formals
                .get(&actual.formal)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            let Some(index) = formal
                .members
                .iter()
                .position(|(_, _, name)| name == member)
            else {
                return self.types.declarations.behavior_mismatch(
                    SemanticRule::Fn3,
                    application,
                    "the binding group's interface declares the selected member",
                );
            };
            let parameter_count = formal.parameters.len();
            let values =
                self.expand_actual_arguments(check_context, application, declaration, caller)?;
            return match values.get(parameter_count + index) {
                Some(GenericArgument::Function(argument)) => Ok(*argument),
                _ => Err(SemanticCompilerFailure::InvalidResolution.into()),
            };
        }
        let formal = self
            .types
            .declarations
            .application_formal(check_context, application)?;
        let selected = self
            .types
            .enclosing_group(check_context, application, formal)?;
        let group = self
            .types
            .behavior
            .formals
            .get(&formal)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let Some((declaration, _, _)) = group.members.iter().find(|(_, _, name)| name == member)
        else {
            return self.types.declarations.behavior_mismatch(
                SemanticRule::Fn3,
                application,
                "the interface declares the selected member",
            );
        };
        caller
            .function_argument(GenericParameterKey::Member {
                application: selected,
                member: *declaration,
            })
            .ok_or_else(|| SemanticCompilerFailure::InvalidResolution.into())
    }

    pub(super) fn expand_written_arguments(
        &mut self,
        check_context: &CheckContext<'_>,
        arguments: &[NodeId],
        caller: &GenericSubstitution,
    ) -> Result<Vec<WrittenArgument>, CheckStop> {
        let mut expanded = Vec::new();
        for argument in arguments {
            let Some(ty) = self
                .types
                .declarations
                .tree
                .first_child_with(*argument, Production::Type)?
            else {
                expanded.push(WrittenArgument::Source(*argument));
                continue;
            };
            if !self.types.declarations.tree.names_nominal(ty)? {
                expanded.push(WrittenArgument::Source(*argument));
                continue;
            };
            let usage = self
                .types
                .declarations
                .use_at(check_context, ty, LexicalUseRole::Type)?;
            let mut member_sources = match usage.target() {
                ResolvedTarget::Source {
                    declaration,
                    class: DeclarationClass::Binding,
                } => self
                    .types
                    .behavior
                    .actuals
                    .get(&declaration)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?
                    .bindings
                    .clone()
                    .into_iter(),
                _ => Vec::new().into_iter(),
            };
            let values = match usage.target() {
                ResolvedTarget::Source {
                    declaration,
                    class: DeclarationClass::Binding,
                } => self.expand_actual_arguments(check_context, ty, declaration, caller)?,
                ResolvedTarget::Source {
                    declaration,
                    class: DeclarationClass::Interface,
                } => {
                    let application = self.types.enclosing_group(check_context, ty, declaration)?;
                    let mut values = Vec::new();
                    for parameter in self
                        .types
                        .expand_formal_parameters(check_context, application)?
                    {
                        let value = match parameter {
                            GenericParameter::Type { declaration, .. } => GenericArgument::Type(
                                caller
                                    .type_argument(declaration)
                                    .ok_or(SemanticCompilerFailure::InvalidResolution)?,
                            ),
                            GenericParameter::Const { declaration, .. } => GenericArgument::Const(
                                caller
                                    .const_argument(declaration)
                                    .ok_or(SemanticCompilerFailure::InvalidResolution)?,
                            ),
                            GenericParameter::Function { key, .. } => GenericArgument::Function(
                                caller
                                    .function_argument(key)
                                    .ok_or(SemanticCompilerFailure::InvalidResolution)?,
                            ),
                        };
                        values.push(value);
                    }
                    values
                }
                _ => {
                    expanded.push(WrittenArgument::Source(*argument));
                    continue;
                }
            };
            expanded.extend(values.into_iter().map(|value| {
                let source = if matches!(value, GenericArgument::Function(_)) {
                    member_sources.next().unwrap_or(*argument)
                } else {
                    *argument
                };
                WrittenArgument::Expanded { source, value }
            }));
        }
        Ok(expanded)
    }

    fn expand_actual_arguments(
        &mut self,
        check_context: &CheckContext<'_>,
        use_node: NodeId,
        declaration: DeclarationId,
        caller: &GenericSubstitution,
    ) -> Result<Vec<GenericArgument>, CheckStop> {
        let group = self
            .types
            .behavior
            .actuals
            .get(&declaration)
            .cloned()
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let written = match self.types.declarations.tree.argument_list(use_node)? {
            Some(list) => self
                .types
                .declarations
                .tree
                .children_with(list, Production::Targ)?,
            None => Vec::new(),
        };
        if !written.is_empty() {
            return self.types.declarations.behavior_mismatch(
                SemanticRule::Fn2,
                use_node,
                "a binding group's written application carries type, const and function arguments only",
            );
        }
        let _ = caller;
        let context = GenericSubstitution::default();
        let formal = self
            .types
            .behavior
            .formals
            .get(&group.formal)
            .cloned()
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let header = self.generic_substitution(
            check_context,
            group.application,
            &formal.parameters,
            &context,
            SemanticRule::Fn3,
            0,
        )?;
        let mut values = header
            .entries()
            .iter()
            .map(|(_, value)| *value)
            .collect::<Vec<_>>();
        for binding in &group.bindings {
            values.push(GenericArgument::Function(self.parse_function_binding(
                check_context,
                *binding,
                &context,
            )?));
        }
        Ok(values)
    }

    /// Rebase the public interface onto the implementation's declaration
    /// identities. The selected function remains the direct-call target.
    pub(super) fn behavior_call_signature(
        &mut self,
        check_context: &CheckContext<'_>,
        node: NodeId,
        instance: Option<super::super::model::FunctionId>,
        formal: &FunctionSignature,
        actual: &FunctionSignature,
    ) -> Result<
        (
            FunctionSignature,
            super::super::model::CheckedEffects,
            super::super::model::CheckedCallContract,
        ),
        CheckStop,
    > {
        let bound_actual = actual.clone();
        if formal.parameters.len() != actual.parameters.len()
            || formal.results.len() != actual.results.len()
        {
            return self.types.declarations.behavior_mismatch(
                SemanticRule::Fn4,
                node,
                "matching parameter and result counts",
            );
        }
        // [FN-4] parameter and result counts, modes and exact types must
        // agree in order; binder spellings are not signature identity.
        for (left, right) in formal.parameters.iter().zip(&bound_actual.parameters) {
            if left.mode != right.mode || left.ty != right.ty {
                return self.types.declarations.behavior_mismatch(
                    SemanticRule::Fn4,
                    node,
                    "matching parameter modes and types",
                );
            }
        }
        for (left, right) in formal.results.iter().zip(&bound_actual.results) {
            if left.mode != right.mode || left.ty != right.ty {
                return self.types.declarations.behavior_mismatch(
                    SemanticRule::Fn4,
                    node,
                    "matching result modes and types",
                );
            }
        }
        // [FN-4] the actual's row must be a SUBSET of the formal's after
        // parameter-ordinal and path normalization, which is the refinement
        // direction: a supplied function may read and write less than the
        // interface promises and may never exceed it.
        let rebase = |paths: &[CheckedStatePath]| -> Result<Vec<CheckedStatePath>, CheckStop> {
            paths
                .iter()
                .map(|path| {
                    let ordinal = formal
                        .parameters
                        .iter()
                        .position(|parameter| parameter.declaration == path.root)
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                    Ok(CheckedStatePath {
                        root: actual.parameters[ordinal].declaration,
                        steps: path.steps.clone(),
                    })
                })
                .collect()
        };
        let mut boundary = formal.declared_effects.clone();
        boundary.reads = rebase(&boundary.reads)?;
        boundary.writes = rebase(&boundary.writes)?;
        // [STOR-8] allocation has no source effect entry and therefore is
        // not an FN-4 refinement dimension. It remains compiler metadata:
        // retained analyses index the selected actual, so preserve that
        // actual's allocation fact rather than inferring purity from the
        // interface's path row.
        boundary.allocates = actual.declared_effects.allocates;
        // [EFF-1] writes subsumes reads at or below the same path. FN-4
        // checks this one-way coverage; the actual's own EFF-2 check still
        // requires every write it declares to be exhibited by its body.
        let reads_covered = actual.declared_effects.reads.iter().all(|path| {
            boundary
                .reads
                .iter()
                .chain(&boundary.writes)
                .any(|prefix| Checker::effect_path_covers(prefix, path))
        });
        let writes_covered = actual.declared_effects.writes.iter().all(|path| {
            boundary
                .writes
                .iter()
                .any(|prefix| Checker::effect_path_covers(prefix, path))
        });
        if !reads_covered || !writes_covered {
            return self.types.declarations.behavior_mismatch(
                SemanticRule::Fn4,
                node,
                "the formal row covers actual reads with reads or writes and actual writes with writes",
            );
        }
        // [FN-4, WAIT-1] a waiting actual may stand only where the formal
        // waits: a body that calls a formal which does not wait may itself
        // not wait, and the actual's calls would then wait outside a waiting
        // function. A formal that waits admits an actual that does not.
        if bound_actual.waits && !formal.waits {
            return self.types.declarations.behavior_mismatch(
                SemanticRule::Fn4,
                node,
                "a formal that waits, because the supplied function waits",
            );
        }
        let contract =
            self.check_behavior_contracts(check_context, node, instance, formal, &bound_actual)?;
        // [FN-5] the immediate call judgment stays wholly in the formal
        // parameter namespace, including its row roots. Keep a second copy of
        // the same row rebased onto the actual parameter declarations only
        // for retained analyses that index the executable callee by its
        // FunctionId. Mixing either namespace makes a valid row look
        // unresolved and can omit its overlap checks.
        let mut effective = formal.clone();
        effective.id = actual.id;
        effective.name = actual.name.clone();
        effective.symbol = actual.symbol.clone();
        effective.declared_effects.allocates = actual.declared_effects.allocates;
        Ok((
            effective,
            super::super::model::CheckedEffects {
                reads: boundary.reads,
                writes: boundary.writes,
                allocates: boundary.allocates,
            },
            contract,
        ))
    }

    pub(super) fn check_behavior_bindings(
        &mut self,
        check_context: &CheckContext<'_>,
    ) -> Result<(), CheckStop> {
        let mut groups = self
            .types
            .behavior
            .actuals
            .values()
            .cloned()
            .collect::<Vec<_>>();
        groups.sort_by_key(|group| group.node.index());
        for group in groups {
            let context = Checker::actual_declaration_context(&group)?;
            let formal = self
                .types
                .behavior
                .formals
                .get(&group.formal)
                .cloned()
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            let substitution = self.generic_substitution(
                check_context,
                group.application,
                &formal.parameters,
                &context,
                SemanticRule::Fn3,
                0,
            )?;
            for ((declaration, _, _), binding) in formal.members.iter().zip(&group.bindings) {
                let argument = self.parse_function_binding(check_context, *binding, &context)?;
                let target = self.types.function_argument_instance(argument)?;
                let actual = self
                    .types
                    .signatures
                    .get(target.0 as usize)
                    .cloned()
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                let signature = self.formal_signature(
                    check_context,
                    GenericParameterKey::Source(*declaration),
                    &substitution,
                    target,
                )?;
                let _ = self.behavior_call_signature(
                    check_context,
                    *binding,
                    None,
                    &signature,
                    &actual,
                )?;
            }
        }
        let mut contexts = self
            .types
            .view
            .functions
            .iter()
            .map(|id| &self.types.signatures[id.0 as usize])
            .map(|signature| (signature.node, signature.substitution.clone()))
            .collect::<Vec<_>>();
        for (template, substitution) in self
            .types
            .view
            .nominals
            .iter()
            .filter_map(|id| self.types.source_nominal_instances[id.0 as usize].as_ref())
        {
            if substitution.is_concrete(&self.types.elements) {
                contexts.push((
                    self.types.nominal_templates[*template].node,
                    substitution.clone(),
                ));
            }
        }
        for (node, substitution) in contexts {
            for (key, argument) in substitution.entries() {
                let GenericArgument::Function(argument) = argument else {
                    continue;
                };
                let target = self.types.function_argument_instance(*argument)?;
                let actual = self
                    .types
                    .signatures
                    .get(target.0 as usize)
                    .cloned()
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                let formal = self.formal_signature(check_context, *key, &substitution, target)?;
                let source = self
                    .types
                    .behavior_binding_site(node, *key, &substitution)?;
                let _ =
                    self.behavior_call_signature(check_context, source, None, &formal, &actual)?;
            }
        }
        Ok(())
    }
}

impl<'unit> TypeContext<'unit> {
    /// Diagnostic provenance is not part of function or nominal instance
    /// identity. The retained substitution names the binding directly; an
    /// independently attached region vector is not an argument of a
    /// function-kind formal and does not select its binding site.
    pub(super) fn record_behavior_binding_sites(
        &self,
        substitution: &GenericSubstitution,
        sources: &[(GenericParameterKey, NodeId)],
    ) -> Result<(), CheckStop> {
        if sources.is_empty() {
            return Ok(());
        }
        let arguments = substitution.clone().with_regions(Vec::new());
        let mut sites = self.behavior.binding_sites.borrow_mut();
        for (key, source) in sources {
            if let Some(site) = sites
                .iter_mut()
                .find(|site| site.key == *key && site.substitution == arguments)
            {
                if source.index() < site.source.index() {
                    site.source = *source;
                }
            } else {
                sites.push(BindingSite {
                    substitution: arguments.clone(),
                    key: *key,
                    source: *source,
                });
            }
        }
        Ok(())
    }
    pub(super) fn behavior_binding_site(
        &self,
        fallback: NodeId,
        key: GenericParameterKey,
        substitution: &GenericSubstitution,
    ) -> Result<NodeId, CheckStop> {
        let arguments = substitution.clone().with_regions(Vec::new());
        Ok(self
            .behavior
            .binding_sites
            .borrow()
            .iter()
            .find(|site| site.key == key && site.substitution == arguments)
            .map_or(fallback, |site| site.source))
    }
    pub(super) fn function_argument_instance(
        &self,
        argument: FunctionArgument,
    ) -> Result<FunctionId, CheckStop> {
        match argument {
            FunctionArgument::Source { reference, .. } => self
                .function_reference_instance(reference)?
                .ok_or_else(|| SemanticCompilerFailure::InvalidResolution.into()),
            FunctionArgument::Parameter(key) => self
                .signatures
                .iter()
                .find(|signature| signature.formal_parameter == Some(key))
                .map(|signature| signature.id)
                .ok_or_else(|| SemanticCompilerFailure::InvalidResolution.into()),
        }
    }
    pub(super) fn function_reference_instance(
        &self,
        id: FunctionReferenceId,
    ) -> Result<Option<FunctionId>, CheckStop> {
        let value = self.function_reference(id)?;
        for id in self
            .functions_by_declaration
            .get(&value.declaration)
            .into_iter()
            .flatten()
        {
            let signature = self
                .signatures
                .get(id.0 as usize)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            if signature.substitution == value.substitution {
                return Ok(Some(*id));
            }
        }
        Ok(None)
    }
    pub(super) fn intern_function_reference(
        &self,
        declaration: DeclarationId,
        substitution: &GenericSubstitution,
    ) -> Result<FunctionArgument, CheckStop> {
        let concrete = substitution.is_concrete(&self.elements);
        let substitution = substitution.clone();
        let value = FunctionReference {
            declaration,
            substitution,
        };
        self.intern_function_reference_value(value, concrete)
    }
    fn intern_function_reference_value(
        &self,
        value: FunctionReference,
        concrete: bool,
    ) -> Result<FunctionArgument, CheckStop> {
        let mut references = self.behavior.references.borrow_mut();
        let index = references
            .iter()
            .position(|candidate| *candidate == value)
            .unwrap_or_else(|| {
                let index = references.len();
                references.push(value);
                index
            });
        let reference = FunctionReferenceId(
            u32::try_from(index).map_err(|_| SemanticCompilerFailure::CounterOverflow)?,
        );
        Ok(FunctionArgument::Source {
            reference,
            concrete,
        })
    }
    pub(super) fn function_reference(
        &self,
        id: FunctionReferenceId,
    ) -> Result<FunctionReference, CheckStop> {
        self.behavior
            .references
            .borrow()
            .get(id.0 as usize)
            .cloned()
            .ok_or_else(|| SemanticCompilerFailure::InvalidResolution.into())
    }
    fn formal_substitution(
        &self,
        check_context: &CheckContext<'_>,
        key: GenericParameterKey,
        context: &GenericSubstitution,
    ) -> Result<GenericSubstitution, CheckStop> {
        let GenericParameterKey::Member { application, .. } = key else {
            return Ok(context.clone());
        };
        let formal = self
            .declarations
            .application_formal(check_context, application)?;
        let group = self
            .behavior
            .formals
            .get(&formal)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let parameters = self.expand_formal_parameters(check_context, application)?;
        let mut values = Vec::new();
        for (formal, written) in group.parameters.iter().zip(&parameters) {
            let value = match written {
                GenericParameter::Type { declaration, .. } => GenericArgument::Type(
                    context
                        .type_argument(*declaration)
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?,
                ),
                GenericParameter::Const { declaration, .. } => GenericArgument::Const(
                    context
                        .const_argument(*declaration)
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?,
                ),
                GenericParameter::Function { .. } => {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                }
            };
            values.push((formal.key(), value));
        }
        Ok(GenericSubstitution::from_bindings(values)?
            .with_regions(context.region_arguments().to_vec()))
    }
    fn symbolic_formal_context(
        &self,
        check_context: &CheckContext<'_>,
        key: GenericParameterKey,
    ) -> Result<GenericSubstitution, CheckStop> {
        let mut owner = match key {
            GenericParameterKey::Member { application, .. } => application,
            GenericParameterKey::Source(_) => self.declarations.formal_source(key)?.1,
        };
        loop {
            if matches!(
                self.declarations.tree.production(owner)?,
                Production::FnDecl
                    | Production::StructDecl
                    | Production::EnumDecl
                    | Production::InterfaceDecl
            ) {
                break;
            }
            owner = self
                .declarations
                .tree
                .parent(owner)?
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        }
        Checker::symbolic_generic_substitution(
            &self.parse_generic_parameters(check_context, owner)?,
        )
    }
    pub(super) fn behavior_call_key(
        &self,
        check_context: &CheckContext<'_>,
        call: NodeId,
    ) -> Result<Option<GenericParameterKey>, CheckStop> {
        let callee = self
            .declarations
            .tree
            .first_child_with(call, Production::Callee)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        if let Some(application) = self.declarations.tree.callee_application(callee)? {
            if self.declarations.tree.is_constructor_call(call)? {
                return Ok(None);
            }
            let formal = self
                .declarations
                .application_formal(check_context, application)?;
            let selected = self.enclosing_group(check_context, application, formal)?;
            let member = self
                .declarations
                .deferred_use_at(callee, crate::DeferredUseRole::FunctionMember)?
                .spelling();
            let group = self
                .behavior
                .formals
                .get(&formal)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            let Some((declaration, _, _)) =
                group.members.iter().find(|(_, _, name)| name == member)
            else {
                return self.declarations.behavior_mismatch(
                    SemanticRule::Fn3,
                    callee,
                    "the group declares the selected member",
                );
            };
            return Ok(Some(GenericParameterKey::Member {
                application: selected,
                member: *declaration,
            }));
        }
        Ok(self
            .declarations
            .resolved
            .lexical_uses_at(callee)
            .find_map(|usage| {
                if usage.role() != LexicalUseRole::IdentifierCallee {
                    return None;
                }
                match usage.target() {
                    ResolvedTarget::Source {
                        declaration,
                        class: DeclarationClass::FunctionParameter,
                    } => Some(GenericParameterKey::Source(declaration)),
                    _ => None,
                }
            }))
    }
    pub(super) fn collect_behavior_groups(
        &mut self,
        check_context: &CheckContext<'_>,
        items: &[NodeId],
    ) -> Result<(), CheckStop> {
        for phase in [Production::InterfaceDecl, Production::BindingDecl] {
            for node in items.iter().copied() {
                if self.declarations.tree.production(node)? != phase {
                    continue;
                }
                match self.declarations.tree.production(node)? {
                    Production::InterfaceDecl => {
                        let declaration = self
                            .declarations
                            .declaration_at(node, DeclarationRole::Interface)?
                            .id();
                        if let Some(generics) = self
                            .declarations
                            .tree
                            .first_child_with(node, Production::Generics)?
                        {
                            for parameter in self
                                .declarations
                                .tree
                                .children_with(generics, Production::Gparam)?
                            {
                                if self
                                    .declarations
                                    .tree
                                    .group_application(parameter)?
                                    .is_some()
                                    || self
                                        .declarations
                                        .tree
                                        .first_child_with(parameter, Production::FnSig)?
                                        .is_some()
                                {
                                    return self.declarations.behavior_mismatch(SemanticRule::Fn3, parameter, "an interface header contains only flat type and const parameters");
                                }
                            }
                        }
                        let parameters = self.parse_generic_parameters(check_context, node)?;
                        if parameters
                            .iter()
                            .any(|parameter| matches!(parameter, GenericParameter::Function { .. }))
                        {
                            return self.declarations.behavior_mismatch(
                                SemanticRule::Fn3,
                                node,
                                "an interface header contains only flat type and const parameters",
                            );
                        }
                        let mut members = Vec::new();
                        let mut names = HashSet::new();
                        for signature in self
                            .declarations
                            .tree
                            .children_with(node, Production::FnSig)?
                        {
                            let member = self
                                .declarations
                                .declaration_at(signature, DeclarationRole::FunctionParameter)?;
                            if !names.insert(member.spelling().to_owned()) {
                                return self.declarations.behavior_mismatch(
                                    SemanticRule::Fn3,
                                    signature,
                                    "each interface member name occurs once",
                                );
                            }
                            members.push((member.id(), signature, member.spelling().to_owned()));
                        }
                        self.behavior.formals.insert(
                            declaration,
                            FormalGroup {
                                parameters,
                                members,
                            },
                        );
                    }
                    Production::BindingDecl => {
                        let declaration = self
                            .declarations
                            .declaration_at(node, DeclarationRole::Binding)?
                            .id();
                        let application = self
                            .declarations
                            .tree
                            .group_application(node)?
                            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
                        let usage = self.declarations.use_at(
                            check_context,
                            application,
                            LexicalUseRole::FormalGroup,
                        )?;
                        let ResolvedTarget::Source {
                            declaration: formal,
                            class: DeclarationClass::Interface,
                        } = usage.target()
                        else {
                            return self.declarations.behavior_mismatch(
                                SemanticRule::Fn3,
                                application,
                                "a binding group names an interface declaration",
                            );
                        };
                        let group = self
                            .behavior
                            .formals
                            .get(&formal)
                            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                        let bindings = self
                            .declarations
                            .tree
                            .children_with(node, Production::FnBind)?;
                        if bindings.len() != group.members.len() {
                            return self.declarations.behavior_mismatch(SemanticRule::Fn3, node, "a binding group binds every interface member exactly once in declared order");
                        }
                        for (binding, (_, _, name)) in bindings.iter().zip(&group.members) {
                            if self
                                .declarations
                                .deferred_use_at(*binding, crate::DeferredUseRole::FunctionBinding)?
                                .spelling()
                                != name
                            {
                                return self.declarations.behavior_mismatch(
                                    SemanticRule::Fn3,
                                    *binding,
                                    "binding member names follow the interface's declared order",
                                );
                            }
                        }
                        // [FORM-3, GRAM-2] no declaration carries a region
                        // parameter in v0.60, so a binding group captures
                        // none.
                        let regions = Vec::new();
                        self.behavior.actuals.insert(
                            declaration,
                            ActualGroup {
                                node,
                                formal,
                                application,
                                regions,
                                bindings,
                            },
                        );
                    }
                    _ => {}
                }
            }
        }
        self.reject_actual_group_cycles()
    }
    /// Abbreviations must close before instance discovery. A reference in a
    /// member binding's type/function arguments is just as much an expansion
    /// edge as a reference in the binding group's header.
    fn reject_actual_group_cycles(&self) -> Result<(), CheckStop> {
        let mut groups = self.behavior.actuals.iter().collect::<Vec<_>>();
        groups.sort_by_key(|(_, group)| group.node.index());
        let mut edges = vec![Vec::new(); groups.len()];
        for (source, (_, group)) in groups.iter().enumerate() {
            let prefix = self.declarations.tree.path(group.node)?.components();
            for usage in self.declarations.resolved.lexical_uses() {
                let ResolvedTarget::Source {
                    declaration,
                    class: DeclarationClass::Binding,
                } = usage.target()
                else {
                    continue;
                };
                if usage.origin().node().components().starts_with(prefix)
                    && let Some(target) = groups
                        .iter()
                        .position(|(candidate, _)| **candidate == declaration)
                    && !edges[source].contains(&target)
                {
                    edges[source].push(target);
                }
            }
        }
        for start in 0..groups.len() {
            let mut pending = std::collections::VecDeque::from([(start, vec![start])]);
            let mut visited = vec![false; groups.len()];
            while let Some((source, path)) = pending.pop_front() {
                if visited[source] {
                    continue;
                }
                visited[source] = true;
                for target in &edges[source] {
                    let mut path = path.clone();
                    path.push(*target);
                    if *target == start {
                        let names = path
                            .iter()
                            .map(|index| {
                                self.declarations
                                    .declaration_at(groups[*index].1.node, DeclarationRole::Binding)
                                    .map(|declaration| declaration.spelling().to_owned())
                            })
                            .collect::<Result<Vec<_>, _>>()?;
                        return self.declarations.behavior_mismatch(
                            SemanticRule::Fn3,
                            groups[source].1.node,
                            &format!("acyclic binding expansion; cycle: {}", names.join(" -> ")),
                        );
                    }
                    pending.push_back((*target, path));
                }
            }
        }
        Ok(())
    }
    pub(super) fn expand_formal_parameters(
        &self,
        check_context: &CheckContext<'_>,
        application: NodeId,
    ) -> Result<Vec<GenericParameter>, CheckStop> {
        let usage =
            self.declarations
                .use_at(check_context, application, LexicalUseRole::FormalGroup)?;
        let ResolvedTarget::Source {
            declaration,
            class: DeclarationClass::Interface,
        } = usage.target()
        else {
            return self.declarations.behavior_mismatch(
                SemanticRule::Fn3,
                application,
                "a parameter group names an interface declaration",
            );
        };
        let group = self
            .behavior
            .formals
            .get(&declaration)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let arguments = match self.declarations.tree.argument_list(application)? {
            Some(list) => self
                .declarations
                .tree
                .children_with(list, Production::Targ)?,
            None => Vec::new(),
        };
        if arguments.len() != group.parameters.len() {
            return self.declarations.behavior_mismatch(
                SemanticRule::Fn3,
                application,
                "a group application writes one fresh binder per interface header parameter",
            );
        }
        let mut parameters = Vec::new();
        for (argument, parameter) in arguments.into_iter().zip(&group.parameters) {
            let expanded = match parameter {
                GenericParameter::Type { bound, .. } => {
                    let ty = self
                        .declarations
                        .tree
                        .first_child_with(argument, Production::Type)?;
                    let declaration = match ty {
                        Some(ty) if self.declarations.tree.children(ty)?.is_empty() => self
                            .declarations
                            .optional_declaration_at(ty, DeclarationRole::GenericType)?,
                        _ => None,
                    };
                    let Some(declaration) = declaration else {
                        return self.declarations.behavior_mismatch(
                            SemanticRule::Fn3,
                            argument,
                            "a type group parameter is one fresh TYPEID binder",
                        );
                    };
                    GenericParameter::Type {
                        declaration: declaration.id(),
                        bound: *bound,
                    }
                }
                GenericParameter::Const { ty, .. } => {
                    let value = self
                        .declarations
                        .tree
                        .first_child_with(argument, Production::Const)?;
                    let declaration = match value {
                        Some(value) if self.declarations.tree.is_single_token(value) => self
                            .declarations
                            .optional_declaration_at(value, DeclarationRole::ConstGeneric)?,
                        _ => None,
                    };
                    let Some(declaration) = declaration else {
                        return self.declarations.behavior_mismatch(
                            SemanticRule::Fn3,
                            argument,
                            "a const group parameter is one fresh IDENT binder",
                        );
                    };
                    GenericParameter::Const {
                        declaration: declaration.id(),
                        ty: *ty,
                    }
                }
                GenericParameter::Function { .. } => {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                }
            };
            parameters.push(expanded);
        }
        for (member, signature, _) in &group.members {
            parameters.push(GenericParameter::Function {
                key: GenericParameterKey::Member {
                    application,
                    member: *member,
                },
                signature: *signature,
            });
        }
        Ok(parameters)
    }
    /// Select by the written application, never by equal substituted types.
    pub(super) fn enclosing_group(
        &self,
        check_context: &CheckContext<'_>,
        node: NodeId,
        formal: DeclarationId,
    ) -> Result<NodeId, CheckStop> {
        if self.declarations.tree.production(node)? == Production::Type
            && self.declarations.tree.argument_list(node)?.is_none()
            && self
                .behavior
                .formals
                .get(&formal)
                .is_some_and(|group| !group.parameters.is_empty())
        {
            return self.declarations.behavior_mismatch(
                SemanticRule::Fn2,
                node,
                "a forwarded group writes its complete type and const application",
            );
        }
        let mut owner = node;
        loop {
            if matches!(
                self.declarations.tree.production(owner)?,
                Production::FnDecl | Production::StructDecl | Production::EnumDecl
            ) {
                break;
            }
            let Some(parent) = self.declarations.tree.parent(owner)? else {
                return self.declarations.behavior_mismatch(
                    SemanticRule::Fn3,
                    node,
                    "the interface application is in scope",
                );
            };
            owner = parent;
        }
        let mut candidates = Vec::new();
        if let Some(generics) = self
            .declarations
            .tree
            .first_child_with(owner, Production::Generics)?
        {
            for parameter in self
                .declarations
                .tree
                .children_with(generics, Production::Gparam)?
            {
                if let Some(application) = self.declarations.tree.group_application(parameter)?
                    && self
                        .declarations
                        .application_formal(check_context, application)?
                        == formal
                {
                    candidates.push(application);
                }
            }
        }
        if let Some(arguments) = self.declarations.tree.argument_list(node)? {
            let written = self
                .declarations
                .tree
                .children_with(arguments, Production::Targ)?;
            let mut keys = Vec::new();
            for argument in written {
                if let Some(ty) = self
                    .declarations
                    .tree
                    .first_child_with(argument, Production::Type)?
                {
                    if self.declarations.tree.argument_list(ty)?.is_some() {
                        return self.declarations.behavior_mismatch(
                            SemanticRule::Fn3,
                            node,
                            "the full application names the declared group binders",
                        );
                    }
                    let usage =
                        self.declarations
                            .use_at(check_context, ty, LexicalUseRole::Type)?;
                    match usage.target() {
                        ResolvedTarget::Source {
                            declaration,
                            class: DeclarationClass::GenericType,
                        } => keys.push(GenericParameterKey::Source(declaration)),
                        _ => {
                            return self.declarations.behavior_mismatch(
                                SemanticRule::Fn3,
                                node,
                                "the full application names the declared group binders",
                            );
                        }
                    }
                } else if let Some(value) = self
                    .declarations
                    .tree
                    .first_child_with(argument, Production::Const)?
                {
                    if !self.declarations.tree.is_single_token(value) {
                        return self.declarations.behavior_mismatch(
                            SemanticRule::Fn3,
                            node,
                            "the full application names the declared group binders",
                        );
                    }
                    let usage =
                        self.declarations
                            .use_at(check_context, value, LexicalUseRole::Const)?;
                    match usage.target() {
                        ResolvedTarget::Source {
                            declaration,
                            class: DeclarationClass::ConstGeneric,
                        } => keys.push(GenericParameterKey::Source(declaration)),
                        _ => {
                            return self.declarations.behavior_mismatch(
                                SemanticRule::Fn3,
                                node,
                                "the full application names the declared group binders",
                            );
                        }
                    }
                } else {
                    return self.declarations.behavior_mismatch(
                        SemanticRule::Fn3,
                        node,
                        "the full application names the declared group binders",
                    );
                }
            }
            let mut selected = Vec::new();
            for candidate in candidates {
                let parameters = self.expand_formal_parameters(check_context, candidate)?;
                let candidate_keys = parameters
                    .iter()
                    .filter(|parameter| !matches!(parameter, GenericParameter::Function { .. }))
                    .map(|parameter| parameter.key())
                    .collect::<Vec<_>>();
                if candidate_keys == keys {
                    selected.push(candidate);
                }
            }
            candidates = selected;
        }
        let [selected] = candidates.as_slice() else {
            return self.declarations.behavior_mismatch(
                SemanticRule::Fn5,
                node,
                "select one in-scope group; when an interface occurs twice, write its full application",
            );
        };
        Ok(*selected)
    }
    /// Written provenance for the expanded type axis [FORM-8]. Forwarded
    /// type parameters remain opaque even after concrete substitution; a
    /// group abbreviation does not make their hidden regions inferable.
    pub(super) fn behavior_type_sources(
        &self,
        check_context: &CheckContext<'_>,
        ty: NodeId,
    ) -> Result<Vec<NodeId>, CheckStop> {
        self.behavior_type_sources_inner(check_context, ty, &mut Vec::new())
    }
    fn behavior_type_sources_inner(
        &self,
        check_context: &CheckContext<'_>,
        ty: NodeId,
        visiting: &mut Vec<DeclarationId>,
    ) -> Result<Vec<NodeId>, CheckStop> {
        if !self.declarations.tree.names_nominal(ty)? {
            return Ok(vec![ty]);
        }
        let application = match self
            .declarations
            .use_at(check_context, ty, LexicalUseRole::Type)?
            .target()
        {
            ResolvedTarget::Source {
                declaration,
                class: DeclarationClass::Binding,
            } => {
                if visiting.contains(&declaration) {
                    return self.declarations.behavior_mismatch(
                        SemanticRule::Fn3,
                        ty,
                        "binding group expansion is acyclic",
                    );
                }
                visiting.push(declaration);
                self.behavior
                    .actuals
                    .get(&declaration)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?
                    .application
            }
            ResolvedTarget::Source {
                class: DeclarationClass::Interface,
                ..
            } => ty,
            _ => return Ok(vec![ty]),
        };
        let mut sources = Vec::new();
        if let Some(targs) = self.declarations.tree.argument_list(application)? {
            for argument in self
                .declarations
                .tree
                .children_with(targs, Production::Targ)?
            {
                if let Some(source) = self
                    .declarations
                    .tree
                    .first_child_with(argument, Production::Type)?
                {
                    let mut branch = visiting.clone();
                    sources.extend(self.behavior_type_sources_inner(
                        check_context,
                        source,
                        &mut branch,
                    )?);
                }
            }
        }
        Ok(sources)
    }
}

impl<'unit> DeclarationInventory<'unit> {
    fn formal_source(
        &self,
        key: GenericParameterKey,
    ) -> Result<(DeclarationId, NodeId), CheckStop> {
        let declaration = match key {
            GenericParameterKey::Source(declaration)
            | GenericParameterKey::Member {
                member: declaration,
                ..
            } => declaration,
        };
        let record = self
            .resolved
            .declarations()
            .iter()
            .find(|candidate| {
                candidate.id() == declaration
                    && candidate.role() == DeclarationRole::FunctionParameter
            })
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let node = self
            .tree
            .node_with_path(record.origin().node())
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        Ok((declaration, node))
    }
    fn formal_template(&self, key: GenericParameterKey) -> Result<FunctionTemplate, CheckStop> {
        let (declaration, node) = self.formal_source(key)?;
        Ok(FunctionTemplate {
            declaration,
            node,
            name: self.declaration_spelling(declaration)?,
            generic_parameters: Vec::new(),
        })
    }
    pub(super) fn behavior_mismatch<T>(
        &self,
        rule: SemanticRule,
        node: NodeId,
        requirement: &str,
    ) -> Result<T, CheckStop> {
        // [FN-4] a binding mismatch carries its repair [DIAG-1]; FN-2's
        // argument-kind refusals keep the plain two-sided payload.
        let kind = if rule == SemanticRule::Fn4 {
            SemanticIssueKind::BehaviorArgumentMismatch {
                expected: requirement.to_owned(),
                mechanical_fix: "supply a function whose signature, row and contract meet the formal interface, or weaken the formal interface to what the supplied function declares",
            }
        } else {
            SemanticIssueKind::type_mismatch(requirement, "a nonmatching behavior argument")
        };
        self.issue_node(rule, node, kind)
    }
    fn application_formal(
        &self,
        check_context: &CheckContext<'_>,
        node: NodeId,
    ) -> Result<DeclarationId, CheckStop> {
        let usage = if matches!(
            self.tree.production(node)?,
            Production::PackUse | Production::TypePath
        ) {
            self.use_at(check_context, node, LexicalUseRole::FormalGroup)?
        } else {
            self.use_at(check_context, node, LexicalUseRole::TypeArgument)?
        };
        match usage.target() {
            ResolvedTarget::Source {
                declaration,
                class: DeclarationClass::Interface,
            } => Ok(declaration),
            _ => self.behavior_mismatch(
                SemanticRule::Fn3,
                node,
                "a forwarded group names an interface declaration",
            ),
        }
    }
}
