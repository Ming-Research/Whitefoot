use crate::semantic::check::CheckContext;
use crate::semantic::check::{DeclarationInventory, TypeContext};
use std::collections::{HashMap, HashSet};

mod finiteness;
mod operands;

use crate::syntax::NodeId;
use crate::{
    BuiltinPreludeId, DeclarationClass, DeclarationId, DeclarationRole, FixedTerminal,
    LexicalUseRole, Production, ResolvedTarget, SemanticCompilerFailure, SemanticIssueKind,
    SemanticRule,
};

use super::super::model::{
    CheckedConst, CheckedGenericRequirement, CheckedNominalKind, CheckedType, IntegerType,
    NominalId,
};
use super::{CheckStop, Checker, FunctionSignature, FunctionTemplate, PreludeType};

/// [FN-2, PROV-6] the at most one bound a type parameter carries.
///
/// A bound is a closed filter on the argument, derived from the language's
/// existing classifications, and never a user trait: `Int` and `Float` are
/// [OP-1]'s numeric rows and each implies copy, and `Class` is the class the
/// capability bound grants the body -- copy for `T: copy`, affine for
/// `T: drop`, and linear for a parameter written with no bound. It selects no
/// behavior and admits no contract member.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum GenericBound {
    Int,
    Float,
    Class(super::linearity::LinearityClass),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum GenericParameter {
    Type {
        declaration: DeclarationId,
        /// [PROV-6, FN-2] the bound this parameter's body is written for. It
        /// is never inferred, and an absent one is the linear class.
        bound: GenericBound,
    },
    /// One const `gparam`. The written integer type is retained because
    /// [MSR-6] admits the parameter as a value, whose exact type is that
    /// written type.
    Const {
        declaration: DeclarationId,
        ty: IntegerType,
    },
    /// A raw function parameter, or one hygienically expanded group member.
    Function {
        key: GenericParameterKey,
        signature: NodeId,
    },
}

/// Group members retain both written identities. Reusing one formal twice
/// never merges its function arguments or introduces lexical member names.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum GenericParameterKey {
    Source(DeclarationId),
    Member {
        application: NodeId,
        member: DeclarationId,
    },
}

impl GenericParameter {
    pub(super) const fn key(self) -> GenericParameterKey {
        match self {
            Self::Type { declaration, .. } | Self::Const { declaration, .. } => {
                GenericParameterKey::Source(declaration)
            }
            Self::Function { key, .. } => key,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum GenericArgument {
    Type(CheckedType),
    Const(CheckedConst),
    Function(super::behavior::FunctionArgument),
}

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub(super) struct GenericSubstitution {
    bindings: Vec<(GenericParameterKey, GenericArgument)>,
    /// [S20, PROV-1] the region axis of one nominal instance: each
    /// `region_params` member of the owning declaration bound to the actual
    /// region this instance names.
    ///
    /// A nominal's region parameters are components of its type name
    /// [TYPE-2], so two instances of one declaration at two regions are two
    /// types and the instance key carries them beside the type and const
    /// arguments. A function's region parameters are not in this axis: a call
    /// substitutes them positionally from its own actuals [FORM-8] and mints
    /// no second signature for a second region.
    regions: Vec<(DeclarationId, DeclarationId)>,
}

impl GenericSubstitution {
    /// Every function-kind argument this substitution supplies, in binding
    /// order.
    pub(super) fn function_arguments(
        &self,
    ) -> impl Iterator<Item = super::behavior::FunctionArgument> + '_ {
        self.bindings
            .iter()
            .filter_map(|(_, argument)| match argument {
                GenericArgument::Function(function) => Some(*function),
                _ => None,
            })
    }

    pub(super) fn from_bindings(
        bindings: Vec<(GenericParameterKey, GenericArgument)>,
    ) -> Result<Self, SemanticCompilerFailure> {
        for (index, (declaration, _)) in bindings.iter().enumerate() {
            if bindings[..index]
                .iter()
                .any(|(earlier, _)| earlier == declaration)
            {
                return Err(SemanticCompilerFailure::InvalidResolution);
            }
        }
        Ok(Self {
            bindings,
            regions: Vec::new(),
        })
    }

    /// The same substitution carrying one nominal's region axis [S20].
    pub(super) fn with_regions(mut self, regions: Vec<(DeclarationId, DeclarationId)>) -> Self {
        self.regions = regions;
        self
    }

    pub(super) fn len(&self) -> usize {
        self.bindings.len()
    }

    pub(super) fn region_arguments(&self) -> &[(DeclarationId, DeclarationId)] {
        &self.regions
    }

    pub(super) fn type_argument(&self, declaration: DeclarationId) -> Option<CheckedType> {
        self.bindings
            .iter()
            .find_map(|(candidate, argument)| {
                (*candidate == GenericParameterKey::Source(declaration)).then_some(argument)
            })
            .and_then(|argument| match argument {
                GenericArgument::Type(ty) => Some(*ty),
                GenericArgument::Const(_) | GenericArgument::Function(_) => None,
            })
    }

    pub(super) fn const_argument(&self, declaration: DeclarationId) -> Option<CheckedConst> {
        self.bindings
            .iter()
            .find_map(|(candidate, argument)| {
                (*candidate == GenericParameterKey::Source(declaration)).then_some(argument)
            })
            .and_then(|argument| match argument {
                GenericArgument::Const(value) => Some(*value),
                GenericArgument::Type(_) | GenericArgument::Function(_) => None,
            })
    }

    /// Whether this is the symbolic validation substitution of its own
    /// template: every parameter stands for itself, which is the shape
    /// [`Checker::validate_generic_templates`] builds to check a written
    /// generic body once.
    pub(super) fn is_symbolic(&self) -> bool {
        !self.bindings.is_empty()
            && self
                .bindings
                .iter()
                .all(|(declaration, argument)| match argument {
                    GenericArgument::Type(
                        CheckedType::Generic(bound)
                        | CheckedType::GenericInt(bound)
                        | CheckedType::GenericFloat(bound),
                    ) => GenericParameterKey::Source(*bound) == *declaration,
                    GenericArgument::Const(CheckedConst::Parameter(bound)) => {
                        GenericParameterKey::Source(*bound) == *declaration
                    }
                    GenericArgument::Function(super::behavior::FunctionArgument::Parameter(
                        key,
                    )) => key == declaration,
                    GenericArgument::Function(_) => false,
                    GenericArgument::Type(_) | GenericArgument::Const(_) => false,
                })
    }

    pub(super) fn is_concrete(&self, elements: &[CheckedType]) -> bool {
        self.bindings.iter().all(|(_, argument)| match argument {
            GenericArgument::Type(ty) => ty.is_concrete(elements),
            GenericArgument::Const(value) => value.is_concrete(),
            GenericArgument::Function(value) => value.is_concrete(),
        })
    }

    pub(super) fn entries(&self) -> &[(GenericParameterKey, GenericArgument)] {
        &self.bindings
    }

    pub(super) fn function_argument(
        &self,
        key: GenericParameterKey,
    ) -> Option<super::behavior::FunctionArgument> {
        self.bindings.iter().find_map(|(candidate, argument)| {
            if *candidate != key {
                return None;
            }
            match argument {
                GenericArgument::Function(value) => Some(*value),
                _ => None,
            }
        })
    }
}

/// [OP-13, OP-10] the [PRE-1] records that take from the heap [STOR-1].
///
/// [EFF-3]'s licence excepts a call that allocates from deduplication and
/// reordering, on the ground that the heap is finite and a duplicated take is
/// a different program [STOR-8]. Allocation carries no effect entry, so the
/// base case of that fact is this list and every other boundary's fact is the
/// union of the facts of the calls its body exhibits. Frame-resident
/// construction and conversion rows are not allocations [EFF-1].
pub(in crate::semantic::check) const HEAP_ALLOCATING_PRELUDE_FUNCTIONS: [&str; 5] = [
    "box_new",
    "box_array_filled",
    "box_slots_new",
    "box_ring_new",
    "grow",
];

impl<'unit> Checker<'_, 'unit> {
    /// Builds only the source template inventory and exact generic-cycle
    /// judgment needed by the selector preflight view. Generic bodies are
    /// ordinary semantic premises and are checked later by the H0 path.
    pub(super) fn collect_function_templates_for_postconditions(
        &mut self,
        check_context: &CheckContext<'_>,
        items: &[NodeId],
    ) -> Result<(), CheckStop> {
        let nodes = items
            .iter()
            .copied()
            .filter(|node| {
                self.types
                    .declarations
                    .tree
                    .production(*node)
                    .is_ok_and(|production| {
                        matches!(production, Production::FnDecl | Production::FnSig)
                    })
            })
            .collect::<Vec<_>>();
        for node in nodes {
            match self.types.collect_function_template(check_context, node) {
                Ok(()) => {}
                Err(CheckStop::Issue(_) | CheckStop::Unsupported(_)) => {
                    let declaration = self
                        .types
                        .declarations
                        .declaration_at(node, DeclarationRole::Function)?
                        .id();
                    self.analysis.mark_postcondition_unavailable(declaration);
                }
                Err(stop) => return Err(stop),
            }
        }
        Ok(())
    }

    pub(super) fn collect_concrete_function_signatures(
        &mut self,
        check_context: &CheckContext<'_>,
    ) -> Result<(), CheckStop> {
        self.collect_concrete_function_signatures_with(check_context, false)
    }

    /// Availability-limited counterpart used by the FN-9 selector preflight.
    ///
    /// A source-side call that has not completed FN-2 establishes no selector
    /// instance. The preflight view may therefore skip that edge while it
    /// discovers every independently successful instance.  The ordinary
    /// checker keeps the strict path above, so its source diagnostics and
    /// no-`ensures` behavior are unchanged.
    pub(super) fn collect_concrete_function_signatures_for_postconditions(
        &mut self,
        check_context: &CheckContext<'_>,
    ) -> Result<(), CheckStop> {
        self.collect_concrete_function_signatures_with(check_context, true)
    }

    fn collect_concrete_function_signatures_with(
        &mut self,
        check_context: &CheckContext<'_>,
        tolerate_source_failure: bool,
    ) -> Result<(), CheckStop> {
        for template_index in 0..self.types.function_templates.len() {
            if tolerate_source_failure
                && self.analysis.postcondition_declaration_unavailable(
                    self.types.function_templates[template_index].declaration,
                )
            {
                continue;
            }
            if self.types.function_templates[template_index]
                .generic_parameters
                .is_empty()
            {
                let result = if tolerate_source_failure {
                    self.instantiate_function_signature_for_postconditions(
                        check_context,
                        template_index,
                        GenericSubstitution::default(),
                    )
                } else {
                    self.instantiate_function_signature(
                        check_context,
                        template_index,
                        GenericSubstitution::default(),
                    )
                };
                match result {
                    Ok(_) => {}
                    Err(
                        CheckStop::Issue(_)
                        | CheckStop::Unsupported(_)
                        | CheckStop::PostconditionPrerequisiteUnavailable,
                    ) if tolerate_source_failure => {}
                    Err(stop) => return Err(stop),
                }
            }
        }
        self.materialize_actual_groups(check_context, tolerate_source_failure)?;
        self.discover_called_function_signatures(check_context, true, tolerate_source_failure)
    }

    fn discover_called_function_signatures(
        &mut self,
        check_context: &CheckContext<'_>,
        require_concrete: bool,
        tolerate_source_failure: bool,
    ) -> Result<(), CheckStop> {
        let mut cursor = 0_usize;
        let mut nominal_cursor = 0_usize;
        while cursor < self.types.view.functions.len()
            || nominal_cursor < self.types.view.nominals.len()
        {
            // A function argument is checked at every instantiation boundary,
            // including a nominal used only in a signature. Those bindings
            // can themselves name functions with further nominal instances.
            while nominal_cursor < self.types.view.nominals.len() {
                let nominal = self.types.view.nominals[nominal_cursor];
                let instance = self.types.source_nominal_instances[nominal.0 as usize].clone();
                nominal_cursor += 1;
                let Some((_, substitution)) = instance else {
                    continue;
                };
                if require_concrete && !substitution.is_concrete(&self.types.elements) {
                    continue;
                }
                for (key, argument) in substitution.entries() {
                    let GenericArgument::Function(argument) = argument else {
                        continue;
                    };
                    self.ensure_formal_nominals(check_context, *key, &substitution)?;
                    self.materialize_function_argument(check_context, *argument)?;
                    // Concrete declaration roots are selected by ordinary
                    // checking. Symbolic hypotheses are reached through
                    // their signature arguments in the symbolic view.
                    if argument.is_concrete()
                        && !self.types.behavior.declaration_arguments.contains(argument)
                    {
                        self.types.behavior.declaration_arguments.push(*argument);
                    }
                }
            }
            if cursor == self.types.view.functions.len() {
                continue;
            }
            let id = self.types.view.functions[cursor];
            let signature = self.types.signatures[id.0 as usize].clone();
            for (key, argument) in signature.substitution.entries() {
                if let GenericArgument::Function(argument) = argument {
                    if require_concrete && !argument.is_concrete() {
                        return Err(SemanticCompilerFailure::InvalidResolution.into());
                    }
                    self.ensure_formal_nominals(check_context, *key, &signature.substitution)?;
                    self.materialize_function_argument(check_context, *argument)?;
                }
            }
            if signature.formal_parameter.is_some() {
                cursor += 1;
                continue;
            }
            for call in self
                .types
                .declarations
                .tree
                .descendants_with(signature.node, Production::Call)?
            {
                if self.types.declarations.call_is_inside_postcondition(call)? {
                    continue;
                }
                if let Some(key) = self.types.behavior_call_key(check_context, call)? {
                    let argument = signature
                        .substitution
                        .function_argument(key)
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                    self.ensure_formal_nominals(check_context, key, &signature.substitution)?;
                    self.materialize_function_argument(check_context, argument)?;
                    continue;
                }
                let Some((template_index, template)) = self.called_function_template(call)? else {
                    continue;
                };
                if tolerate_source_failure
                    && self
                        .analysis
                        .postcondition_declaration_unavailable(template.declaration)
                {
                    continue;
                }
                if template.generic_parameters.is_empty() {
                    continue;
                }
                // [OP-10, OP-11, OP-14] a window operation, `swap` and
                // `free_empty` write no type arguments at a call: every type
                // parameter is supplied by an operand, so the written syntax
                // names no instance for this walk to build. The body check
                // reads the operand's type and defers the instance it selects.
                if self
                    .types
                    .declarations
                    .operand_directed_row_index(&template)?
                    .is_some()
                {
                    continue;
                }
                if tolerate_source_failure && !self.postcondition_call_arguments_have_links(call)? {
                    continue;
                }
                if tolerate_source_failure
                    && let Some(targs) = self.types.declarations.tree.argument_list(call)?
                {
                    let checkpoint = self.types.view.clone();
                    match self.ensure_nominals_in_node(
                        check_context,
                        targs,
                        &signature.substitution,
                    ) {
                        Ok(_) => {}
                        Err(
                            CheckStop::Issue(_)
                            | CheckStop::Unsupported(_)
                            | CheckStop::PostconditionPrerequisiteUnavailable,
                        ) => {
                            self.types.select_view(checkpoint);
                            continue;
                        }
                        Err(stop) => return Err(stop),
                    }
                }
                let substitution = match self.call_generic_substitution(
                    check_context,
                    call,
                    &template,
                    &signature.substitution,
                ) {
                    Ok(substitution) => substitution,
                    Err(
                        CheckStop::Issue(_)
                        | CheckStop::Unsupported(_)
                        | CheckStop::PostconditionPrerequisiteUnavailable,
                    ) if tolerate_source_failure => {
                        continue;
                    }
                    Err(stop) => return Err(stop),
                };
                if require_concrete && !substitution.is_concrete(&self.types.elements) {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                }
                let already_present = self
                    .types
                    .functions_by_declaration
                    .get(&template.declaration)
                    .into_iter()
                    .flatten()
                    .any(|id| {
                        self.types.view.contains_function(*id)
                            && self
                                .types
                                .signatures
                                .get(id.0 as usize)
                                .is_some_and(|instance| instance.substitution == substitution)
                    });
                if !already_present {
                    self.types
                        .record_instance_request(template.node, &substitution, call);
                    let result = if tolerate_source_failure {
                        self.instantiate_function_signature_for_postconditions(
                            check_context,
                            template_index,
                            substitution.clone(),
                        )
                    } else {
                        self.instantiate_function_signature(
                            check_context,
                            template_index,
                            substitution.clone(),
                        )
                    }
                    .map_err(|stop| self.types.declarations.attribute_to_call(call, stop));
                    match result {
                        Ok(_) => {}
                        Err(
                            CheckStop::Issue(_)
                            | CheckStop::Unsupported(_)
                            | CheckStop::PostconditionPrerequisiteUnavailable,
                        ) if tolerate_source_failure => {}
                        Err(stop) => return Err(stop),
                    }
                }
                if !require_concrete && self.types.concrete_substitution_identity(&substitution)? {
                    let id = self
                        .types
                        .function_instance(template.declaration, &substitution, None)
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                    self.analysis.schema_written_instances.push(id);
                }
            }
            cursor = cursor
                .checked_add(1)
                .ok_or(SemanticCompilerFailure::CounterOverflow)?;
        }
        Ok(())
    }

    pub(super) fn postcondition_call_arguments_have_links(
        &self,
        call: NodeId,
    ) -> Result<bool, CheckStop> {
        let Some(targs) = self.types.declarations.tree.argument_list(call)? else {
            return Ok(true);
        };
        let owner = self.types.declarations.tree.path(targs)?.components();
        if self
            .types
            .declarations
            .resolved
            .lexical_uses()
            .iter()
            .any(|usage| {
                let path = usage.origin().node().components();
                path.len() >= owner.len()
                    && path.starts_with(owner)
                    && match usage.target() {
                        ResolvedTarget::Source {
                            declaration,
                            class: DeclarationClass::NamedConst,
                        } => !self.types.constants.contains_key(&declaration),
                        ResolvedTarget::Source {
                            declaration,
                            class: DeclarationClass::NominalType,
                        } => {
                            self.analysis
                                .postcondition_declaration_unavailable(declaration)
                                || !self
                                    .types
                                    .nominal_templates_by_declaration
                                    .contains_key(&declaration)
                        }
                        _ => false,
                    }
            })
        {
            return Ok(false);
        }
        for ty in self
            .types
            .declarations
            .tree
            .descendants_with(targs, Production::Type)?
        {
            if self.types.declarations.tree.names_nominal(ty)?
                && !self
                    .types
                    .declarations
                    .resolved
                    .lexical_uses_at(ty)
                    .any(|usage| {
                        matches!(
                            usage.role(),
                            LexicalUseRole::Type | LexicalUseRole::TypeArgument
                        )
                    })
            {
                return Ok(false);
            }
        }
        for constant in self
            .types
            .declarations
            .tree
            .descendants_with(targs, Production::Const)?
        {
            let identifiers = self.types.declarations.tree.direct_identifiers(constant)?;
            if !identifiers.is_empty() {
                let uses = self
                    .types
                    .declarations
                    .resolved
                    .lexical_uses_at(constant)
                    .filter(|usage| usage.role() == LexicalUseRole::Const)
                    .collect::<Vec<_>>();
                if uses.len() != identifiers.len() {
                    return Ok(false);
                }
                for usage in uses {
                    if let ResolvedTarget::Source {
                        declaration,
                        class: DeclarationClass::NamedConst,
                    } = usage.target()
                        && !self.types.constants.contains_key(&declaration)
                    {
                        return Ok(false);
                    }
                }
            }
        }
        for argument in self
            .types
            .declarations
            .tree
            .children_with(targs, Production::Targ)?
        {
            if self
                .types
                .declarations
                .tree
                .first_child_with(argument, Production::Type)?
                .is_some()
                || self
                    .types
                    .declarations
                    .tree
                    .first_child_with(argument, Production::Const)?
                    .is_some()
                || self
                    .types
                    .declarations
                    .tree
                    .first_child_with(argument, Production::FunctionArg)?
                    .is_some()
            {
                continue;
            }
            // [GRAM-3] a `targ` is a type, a const or a function argument;
            // anything else in that position is not a written argument this
            // application can read.
            return Ok(false);
        }
        Ok(true)
    }

    pub(super) fn called_function_template(
        &self,
        call: NodeId,
    ) -> Result<Option<(usize, FunctionTemplate)>, CheckStop> {
        let callee = self
            .types
            .declarations
            .tree
            .first_child_with(call, Production::Callee)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let Some(usage) = self
            .types
            .declarations
            .resolved
            .lexical_uses_at(callee)
            .find(|usage| usage.role() == LexicalUseRole::IdentifierCallee)
        else {
            return Ok(None);
        };
        let declaration = match usage.target() {
            ResolvedTarget::Source {
                declaration,
                class: DeclarationClass::Function,
            } => declaration,
            // Function parameters are selected through behavior substitution.
            // They have no ordinary source template; FN-6 checks their edges
            // in the finite function-target graph before discovery.
            ResolvedTarget::Source {
                class: DeclarationClass::FunctionParameter,
                ..
            } => return Ok(None),
            // An OP family has no function template; recursion through one is
            // impossible, so it contributes no cycle edge.
            ResolvedTarget::Operation(_) => {
                return Ok(None);
            }
            _ => return Err(SemanticCompilerFailure::InvalidResolution.into()),
        };
        let Some(index) = self
            .types
            .templates_by_declaration
            .get(&declaration)
            .copied()
        else {
            if self
                .analysis
                .postcondition_declaration_unavailable(declaration)
            {
                return Ok(None);
            }
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        };
        let template = self
            .types
            .function_templates
            .get(index)
            .cloned()
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        Ok(Some((index, template)))
    }

    pub(super) fn concrete_function_for_call(
        &mut self,
        check_context: &CheckContext<'_>,
        node: NodeId,
        declaration: DeclarationId,
        caller: &GenericSubstitution,
    ) -> Result<super::super::model::FunctionId, CheckStop> {
        let template_index = *self
            .types
            .templates_by_declaration
            .get(&declaration)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let template = self
            .types
            .function_templates
            .get(template_index)
            .cloned()
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let substitution =
            self.call_generic_substitution(check_context, node, &template, caller)?;
        self.types
            .function_instance(declaration, &substitution, None)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        self.instantiate_function_signature(check_context, template_index, substitution)
    }

    /// The instance one call to an operand-directed [PRE-1] row selects
    /// [OP-10, OP-11, OP-14], or `None` where the callee is any other row.
    ///
    /// The instance is keyed on the operand's own type, so it cannot exist
    /// before the body reaches the call. A substitution with no built
    /// signature is materialized here, without restarting the body.
    pub(super) fn operand_directed_function_for_call(
        &mut self,
        check_context: &CheckContext<'_>,
        node: NodeId,
        declaration: DeclarationId,
        bindings: &HashMap<DeclarationId, super::LocalBinding>,
    ) -> Result<Option<super::super::model::FunctionId>, CheckStop> {
        let Some(&template_index) = self.types.templates_by_declaration.get(&declaration) else {
            return Ok(None);
        };
        let template = self
            .types
            .function_templates
            .get(template_index)
            .cloned()
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let Some(row_index) = self
            .types
            .declarations
            .operand_directed_row_index(&template)?
        else {
            return Ok(None);
        };
        let substitution = self.types.operand_directed_substitution(
            check_context,
            node,
            &template,
            row_index,
            bindings,
        )?;
        let id =
            self.ensure_operand_directed_instance(check_context, template_index, substitution)?;
        Ok(Some(id))
    }

    /// An operand-directed row's identity and selectors become available in
    /// the current view when its operand supplies the required shape.
    pub(super) fn ensure_operand_directed_instance(
        &mut self,
        check_context: &CheckContext<'_>,
        template_index: usize,
        substitution: GenericSubstitution,
    ) -> Result<super::super::model::FunctionId, CheckStop> {
        let declaration = self
            .types
            .function_templates
            .get(template_index)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?
            .declaration;
        if let Some(id) = self
            .types
            .function_instance(declaration, &substitution, None)
            && self.types.view.contains_function(id)
        {
            return Ok(id);
        }
        let id =
            self.instantiate_function_signature(check_context, template_index, substitution)?;
        self.admit_postcondition_selectors_for(id)?;
        Ok(id)
    }

    pub(super) fn instantiate_function_signature(
        &mut self,
        check_context: &CheckContext<'_>,
        template_index: usize,
        substitution: GenericSubstitution,
    ) -> Result<super::super::model::FunctionId, CheckStop> {
        let template = self
            .types
            .function_templates
            .get(template_index)
            .cloned()
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let existing = self
            .types
            .function_instance(template.declaration, &substitution, None);
        if let Some(id) = existing
            && self.types.view.contains_function(id)
        {
            return Ok(id);
        }
        // Preflight may have formed this header without walking its body.
        // Ordinary activation retains that mandatory nominal-formation walk.
        self.ensure_nominals_in_function(check_context, template.node, &substitution)?;
        if let Some(id) = existing {
            self.types.activate_function(id)?;
            return Ok(id);
        }
        let id = super::super::model::FunctionId(
            u32::try_from(self.types.signatures.len())
                .map_err(|_| SemanticCompilerFailure::CounterOverflow)?,
        );
        let signature =
            self.build_function_signature(check_context, &template, substitution, id)?;
        self.types.retain_signature(signature)
    }

    fn instantiate_function_signature_for_postconditions(
        &mut self,
        check_context: &CheckContext<'_>,
        template_index: usize,
        substitution: GenericSubstitution,
    ) -> Result<super::super::model::FunctionId, CheckStop> {
        let template = self
            .types
            .function_templates
            .get(template_index)
            .cloned()
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        if !self.postcondition_function_header_dependencies_available(template.node)? {
            return Err(CheckStop::PostconditionPrerequisiteUnavailable);
        }
        let prior = self.types.view.clone();
        let prepared =
            self.ensure_nominals_in_function_signature(check_context, template.node, &substitution);
        let signature = prepared.and_then(|()| {
            if let Some(id) =
                self.types
                    .function_instance(template.declaration, &substitution, None)
            {
                self.types.activate_function(id)?;
                return Ok(id);
            }
            let id = super::super::model::FunctionId(
                u32::try_from(self.types.signatures.len())
                    .map_err(|_| SemanticCompilerFailure::CounterOverflow)?,
            );
            let signature =
                self.build_function_signature(check_context, &template, substitution, id)?;
            self.types.retain_signature(signature)
        });
        if signature.is_err() {
            self.types.select_view(prior);
        }
        signature
    }

    pub(super) fn postcondition_function_header_dependencies_available(
        &self,
        function: NodeId,
    ) -> Result<bool, CheckStop> {
        let mut header_nodes = self
            .types
            .declarations
            .tree
            .children_with(function, Production::ParamList)?;
        let result_binding = self
            .types
            .declarations
            .tree
            .first_child_with(function, Production::ResultBinding)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        header_nodes.push(result_binding);
        for node in header_nodes {
            let path = self.types.declarations.tree.path(node)?.components();
            if self
                .types
                .declarations
                .resolved
                .lexical_uses()
                .iter()
                .any(|usage| {
                    let usage_path = usage.origin().node().components();
                    usage_path.len() >= path.len()
                        && usage_path.starts_with(path)
                        && match usage.target() {
                            ResolvedTarget::Source {
                                declaration,
                                class: DeclarationClass::NamedConst,
                            } => !self.types.constants.contains_key(&declaration),
                            ResolvedTarget::Source {
                                declaration,
                                class: DeclarationClass::NominalType,
                            } => {
                                self.analysis
                                    .postcondition_declaration_unavailable(declaration)
                                    || !self
                                        .types
                                        .nominal_templates_by_declaration
                                        .contains_key(&declaration)
                            }
                            _ => false,
                        }
                })
            {
                return Ok(false);
            }
            for ty in self
                .types
                .declarations
                .tree
                .descendants_with(node, Production::Type)?
            {
                if self.types.declarations.tree.names_nominal(ty)?
                    && !self
                        .types
                        .declarations
                        .resolved
                        .lexical_uses_at(ty)
                        .any(|usage| {
                            matches!(
                                usage.role(),
                                LexicalUseRole::Type | LexicalUseRole::TypeArgument
                            )
                        })
                {
                    return Ok(false);
                }
            }
            for constant in self
                .types
                .declarations
                .tree
                .descendants_with(node, Production::Const)?
            {
                let identifiers = self.types.declarations.tree.direct_identifiers(constant)?;
                if !identifiers.is_empty() {
                    let uses = self
                        .types
                        .declarations
                        .resolved
                        .lexical_uses_at(constant)
                        .filter(|usage| usage.role() == LexicalUseRole::Const)
                        .count();
                    if uses != identifiers.len() {
                        return Ok(false);
                    }
                }
            }
        }
        Ok(true)
    }

    pub(super) fn build_function_signature(
        &mut self,
        check_context: &CheckContext<'_>,
        template: &FunctionTemplate,
        substitution: GenericSubstitution,
        id: super::super::model::FunctionId,
    ) -> Result<FunctionSignature, CheckStop> {
        let check_context = &CheckContext {
            writing_module: self
                .types
                .declarations
                .resolved
                .declaration(template.declaration)
                .and_then(crate::DeclarationRecord::module),
            ..*check_context
        };
        // [GRAM-2, FORM-3] no declaration carries a region parameter in
        // v0.60: a reference is a name for a path [REF-1] and its validity is
        // the [REF-2] flow fact, not a brand on the signature.
        let region_parameters = Vec::new();
        let parameters = self.parse_parameters_with(check_context, template.node, &substitution)?;
        // [GRAM-2] the declaration writes one result or an ordered result
        // list. Every ordinal is judged by the ordinary result rules below;
        // a list additionally hands its caller one value of the compiler-owned
        // result-list nominal, which is this callable's result [CALL-4].
        let result_bindings = self
            .types
            .declarations
            .tree
            .children_with(template.node, Production::ResultBinding)?;
        let mut results = Vec::with_capacity(result_bindings.len());
        for binding in &result_bindings {
            let rtype = self
                .types
                .declarations
                .tree
                .first_child_with(*binding, Production::Rtype)?
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            // [FN-1] the binder spelling is out of callable-signature
            // equality; the resolution record carries it where a route or a
            // diagnostic names it.
            let (mode, ty) = self.parse_rtype_with(check_context, rtype, &substitution)?;
            results.push(super::ResultSignature { mode, ty, rtype });
        }
        let [first_result, ..] = results.as_slice() else {
            return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
        };
        // [VIEW-6] two results of the same view type at the same formal
        // region are refused at the second `result_binding`: [FN-1]'s ceiling
        // is stated over the type and the region, so each of them would carry
        // every origin the other does and a demux written with one region
        // would return views that all alias all of its inputs. The type's
        // exact identity carries the strength, the region and the element
        // [TYPE-5], so equality is the whole judgment.
        //
        // The whole list is judged before any per-ordinal capability refusal
        // below, because a source-language rejection is never replaced by a
        // compiler-capability stop.
        let single = results.len() == 1;
        let rtype = first_result.rtype;
        let (result_mode, result, result_list) = if single {
            (first_result.mode, first_result.ty, None)
        } else {
            // [REF-3] no result ordinal is a reference: FN-1 returns owned
            // values, and a `return_stmt` whose selected expression is a
            // reference is that violation at its own `expr`.
            for entry in &results {
                if entry.mode != super::super::model::CheckedMode::Own {
                    return self.types.declarations.issue_node(
                        SemanticRule::Ref3,
                        entry.rtype,
                        SemanticIssueKind::EscapingReference {
                            mechanical_fix: super::references::REF3_RETURN_AN_INDEX,
                        },
                    );
                }
            }
            let Some(fields) =
                self.result_list_fields(check_context, template.node, &substitution)?
            else {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            };
            let nominal = self
                .types
                .result_list_nominals
                .get(&fields)
                .copied()
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            (
                super::super::model::CheckedMode::Own,
                super::super::model::CheckedType::Nominal(nominal),
                Some(nominal),
            )
        };
        // [REF-3] a reference never leaves the callable that formed it, so a
        // declared result mode other than `own` is refused at the boundary.
        if result_mode != super::super::model::CheckedMode::Own {
            return self.types.declarations.issue_node(
                SemanticRule::Ref3,
                rtype,
                SemanticIssueKind::EscapingReference {
                    mechanical_fix: super::references::REF3_RETURN_AN_INDEX,
                },
            );
        }
        let effects = self
            .types
            .declarations
            .tree
            .first_child_with(template.node, Production::Effects)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let mut declared_effects = self
            .types
            .parse_effects(check_context, effects, &parameters)?;
        // [EFF-3] the allocation fact of a boundary that takes from the heap
        // by definition: the boxed [OP-13] construction functions and
        // [OP-10]'s `grow`. It is not a row category [EFF-1, STOR-8], so it is
        // set here from the declaration's own identity and unioned along the
        // call graph by the ordinary effect walk.
        declared_effects.allocates |=
            HEAP_ALLOCATING_PRELUDE_FUNCTIONS.contains(&template.name.as_str());
        // [MOD-3] functions of different modules may share a name, so a
        // module other than the root prefixes its path; a source bundle's
        // root-module symbols keep their plain names.
        let base = self
            .types
            .declarations
            .module_symbol_base(template.declaration, &template.name);
        let symbol = if template.generic_parameters.is_empty() {
            base
        } else {
            // An instance's symbol names its template and a digest of its
            // concrete arguments spelled by module-qualified declaration
            // names, so it keeps its name while unrelated instances and
            // declarations come and go and an unchanged link fragment keeps
            // its bytes. An argument with no concrete spelling, or a symbol
            // another instance already holds, falls back to the instance's
            // ordinal, which is unique.
            self.types
                .stable_instance_suffix(&substitution)
                .map(|suffix| format!("{base}$instance${suffix}"))
                .filter(|candidate| {
                    !self
                        .types
                        .signatures
                        .iter()
                        .any(|signature| signature.symbol == *candidate)
                })
                .unwrap_or_else(|| format!("{base}$instance${}", id.0))
        };
        Ok(FunctionSignature {
            id,
            declaration: template.declaration,
            node: template.node,
            name: template.name.clone(),
            symbol,
            region_parameters,
            parameters,
            result_mode,
            result,
            results,
            result_list,
            effects_node: effects,
            declared_effects,
            formal_parameter: None,
            substitution,
        })
    }

    pub(super) fn validate_generic_templates(
        &mut self,
        check_context: &CheckContext<'_>,
    ) -> Result<(), CheckStop> {
        if !self.analysis.generic_requirements.is_empty() {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        // A closed unit with no generic function declaration has no source
        // schema to validate.  Rechecking every concrete function through a
        // temporary symbolic inventory would duplicate the ordinary ENT and
        // provenance baseline without producing a schema report, and claim
        // residuality would then pay that whole-program cost once more for
        // every mask.
        if self
            .types
            .function_templates
            .iter()
            .all(|template| template.generic_parameters.is_empty())
        {
            return Ok(());
        }
        let concrete_view = self.types.view.clone();
        let concrete_allocations = concrete_view
            .functions
            .iter()
            .map(|id| {
                (
                    *id,
                    self.types.signatures[id.0 as usize]
                        .declared_effects
                        .allocates,
                )
            })
            .collect::<Vec<_>>();
        self.types.view.clear_functions();
        self.analysis.postcondition_selectors.clear();
        self.analysis.schema_written_instances.clear();
        // Record only the initial source-canonical symbolic instance for each
        // generic. Transitive discovery below may instantiate another
        // symbolic shape for the same source declaration; those validate the
        // source call graph but are not a second metadata identity.
        let mut canonical_generic_signatures = Vec::new();
        for template_index in 0..self.types.function_templates.len() {
            let template = self
                .types
                .function_templates
                .get(template_index)
                .cloned()
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            let substitution =
                Checker::symbolic_generic_substitution(&template.generic_parameters)?;
            let signature =
                self.instantiate_function_signature(check_context, template_index, substitution)?;
            let signature_index = signature.0 as usize;
            if !template.generic_parameters.is_empty() {
                canonical_generic_signatures.push((signature_index, template.declaration));
            }
        }
        self.materialize_actual_groups(check_context, false)?;
        self.discover_called_function_signatures(check_context, false, false)?;
        // Selector availability belongs to the checking view. Sharing a
        // FunctionId does not make the ordinary selector universe valid
        // for a symbolic judgment, so this view admits its own selectors.
        //
        // This pass checks every generic template's own body, and no
        // nongeneric signature reaches those bodies, so the canonical
        // symbolic instances seed the reachable-instance walk [FN-9]. Without
        // them a call a generic body makes — `slots_new` and every other
        // [PRE-1] record among them — contributed no selector here and its
        // declared `ensures` was published to no generic body.
        let canonical_seeds = canonical_generic_signatures
            .iter()
            .map(|(index, _)| {
                Ok(super::super::model::FunctionId(
                    u32::try_from(*index).map_err(|_| SemanticCompilerFailure::CounterOverflow)?,
                ))
            })
            .collect::<Result<Vec<_>, CheckStop>>()?;
        self.admit_postcondition_selectors_including(check_context, &canonical_seeds)?;
        let mut phase_a = self.check_function_view(check_context, Vec::new())?;
        self.types.close_allocation_metadata(&mut phase_a)?;
        for (canonical, declaration) in &canonical_generic_signatures {
            let checked = phase_a
                .get(*canonical)
                .filter(|checked| checked.function.declaration == *declaration)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            for requirement in &checked.function.requirements {
                self.analysis
                    .generic_requirements
                    .push(CheckedGenericRequirement {
                        declaration: *declaration,
                        requirement: requirement.clone(),
                    });
            }
        }
        self.install_call_requirements(check_context, &mut phase_a)?;
        self.types.form_obligation_records(&mut phase_a)?;
        let callees = self.types.entailment_callees()?;
        self.validate_generic_body_entailment(
            &mut phase_a,
            &canonical_generic_signatures,
            &callees,
        )?;
        self.analysis.symbolic_functions = phase_a;
        self.types.select_view(concrete_view);
        for (id, allocates) in concrete_allocations {
            self.types.signatures[id.0 as usize]
                .declared_effects
                .allocates = allocates;
        }
        let retained_concrete = self.activate_schema_written_instances(check_context)?;
        self.analysis.postcondition_selectors.clear();
        self.admit_postcondition_selectors_including(check_context, &retained_concrete)?;
        Ok(())
    }

    /// The symbolic discovery walk already found every written source call.
    /// Activate its concrete instances in the established structural order;
    /// no source-body replay or identity reconstruction is necessary.
    fn activate_schema_written_instances(
        &mut self,
        check_context: &CheckContext<'_>,
    ) -> Result<Vec<super::super::model::FunctionId>, CheckStop> {
        let mut discovered = Vec::new();
        for id in std::mem::take(&mut self.analysis.schema_written_instances) {
            let signature = self
                .types
                .signatures
                .get(id.0 as usize)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            discovered.push((
                signature.declaration,
                self.types.substitution_order_key(&signature.substitution)?,
                id,
            ));
        }
        discovered.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
        discovered.dedup_by_key(|candidate| candidate.2);
        let mut selected = Vec::new();
        for (_, _, id) in discovered {
            if self.types.view.contains_function(id) {
                continue;
            }
            let signature = self.types.signatures[id.0 as usize].clone();
            self.types.activate_substitution(&signature.substitution)?;
            let template = *self
                .types
                .templates_by_declaration
                .get(&signature.declaration)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            let activated = self.instantiate_function_signature(
                check_context,
                template,
                signature.substitution,
            )?;
            if activated != id {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            }
            selected.push(id);
        }
        Ok(selected)
    }

    /// Retain the concrete members of a transient declaration view. Identities
    /// stay interned; only membership and discovery order change.
    pub(super) fn retain_concrete_nominals_since(
        &mut self,
        _check_context: &CheckContext<'_>,
        prior: super::InventoryView,
    ) -> Result<(), CheckStop> {
        let mut concrete = Vec::new();
        for id in &self.types.view.nominals {
            if !prior.contains_nominal(*id)
                && self
                    .types
                    .concrete_type_identity(CheckedType::Nominal(*id))?
            {
                concrete.push(*id);
            }
        }
        self.types.select_view(prior);
        for id in concrete {
            self.types.activate_nominal(id)?;
        }
        Ok(())
    }

    pub(super) fn symbolic_generic_substitution(
        parameters: &[GenericParameter],
    ) -> Result<GenericSubstitution, CheckStop> {
        let bindings = parameters
            .iter()
            .copied()
            .map(|parameter| {
                let argument = match parameter {
                    GenericParameter::Type {
                        declaration,
                        bound: GenericBound::Int,
                        ..
                    } => GenericArgument::Type(CheckedType::GenericInt(declaration)),
                    GenericParameter::Type {
                        declaration,
                        bound: GenericBound::Float,
                        ..
                    } => GenericArgument::Type(CheckedType::GenericFloat(declaration)),
                    GenericParameter::Type {
                        declaration,
                        bound: GenericBound::Class(_),
                        ..
                    } => GenericArgument::Type(CheckedType::Generic(declaration)),
                    GenericParameter::Const { declaration, .. } => {
                        GenericArgument::Const(CheckedConst::Parameter(declaration))
                    }
                    GenericParameter::Function { key, .. } => {
                        GenericArgument::Function(super::behavior::FunctionArgument::Parameter(key))
                    }
                };
                (parameter.key(), argument)
            })
            .collect();
        GenericSubstitution::from_bindings(bindings).map_err(CheckStop::Compiler)
    }

    /// The symbolic instance of one nominal template: every type and const
    /// parameter stands for itself, and so does every region parameter
    /// [S20]. It is the instance a declaration is judged once at, before any
    /// concrete instance exists.
    pub(super) fn symbolic_nominal_substitution(
        parameters: &[GenericParameter],
        region_parameters: &[DeclarationId],
    ) -> Result<GenericSubstitution, CheckStop> {
        Ok(
            Checker::symbolic_generic_substitution(parameters)?.with_regions(
                region_parameters
                    .iter()
                    .map(|region| (*region, *region))
                    .collect(),
            ),
        )
    }

    pub(super) fn call_generic_substitution(
        &mut self,
        check_context: &CheckContext<'_>,
        node: NodeId,
        template: &FunctionTemplate,
        caller: &GenericSubstitution,
    ) -> Result<GenericSubstitution, CheckStop> {
        let leading_regions = match self.types.declarations.tree.argument_list(node)? {
            Some(targs) => self.types.declarations.user_call_region_prefix(
                &self
                    .types
                    .declarations
                    .tree
                    .children_with(targs, Production::Targ)?,
            )?,
            None => 0,
        };
        // [FORM-8] user calls write caller-chosen regions before type/const
        // arguments. [DIAG-1] their generic argument list is FN-2's.
        self.generic_substitution(
            check_context,
            node,
            &template.generic_parameters,
            caller,
            SemanticRule::Fn2,
            leading_regions,
        )
    }

    /// One nominal instance's complete argument list: its region arguments,
    /// then its type and const arguments [S20, TYPE-5, FORM-8].
    ///
    /// A nominal's region parameters are components of its type name, so a
    /// `type` and a `construct` alike write them, as the leading members of
    /// the same `targs` list its type and const arguments follow — the
    /// spelling the two runs and the two providers already use. They are
    /// written on exactly the ground [TYPE-5] gives a construct's type
    /// arguments: construction consults no expected nominal type, so nothing
    /// but the written arguments fixes the instance.
    pub(super) fn nominal_generic_substitution(
        &mut self,
        check_context: &CheckContext<'_>,
        node: NodeId,
        parameters: &[GenericParameter],
        region_parameters: &[DeclarationId],
        caller: &GenericSubstitution,
    ) -> Result<GenericSubstitution, CheckStop> {
        self.nominal_generic_substitution_with(
            check_context,
            node,
            parameters,
            region_parameters,
            &[],
            caller,
        )
    }

    /// The same list where some region parameters are already fixed [FORM-8].
    ///
    /// A `type` position determines none of them and writes every one; a
    /// `construct`'s field operands determine the ones their declared types
    /// name, and the position writes exactly the rest, still as the leading
    /// members of the same `targs` list. The two are one function because the
    /// difference between them is exactly which formals arrive already bound.
    pub(super) fn nominal_generic_substitution_with(
        &mut self,
        check_context: &CheckContext<'_>,
        node: NodeId,
        parameters: &[GenericParameter],
        region_parameters: &[DeclarationId],
        determined: &[(DeclarationId, DeclarationId)],
        caller: &GenericSubstitution,
    ) -> Result<GenericSubstitution, CheckStop> {
        // [TYPE-5] a generic nominal's construct writes that nominal's
        // arguments, and their absence or a wrong count is TYPE-5's own
        // violation, "at the complete `construct`".
        let written_parameters = region_parameters
            .iter()
            .copied()
            .filter(|formal| !determined.iter().any(|(bound, _)| bound == formal))
            .collect::<Vec<_>>();
        let written = Checker::nominal_region_arguments(node, &written_parameters, caller)?;
        let mut regions = Vec::with_capacity(region_parameters.len());
        for formal in region_parameters {
            let actual = determined
                .iter()
                .chain(written.iter())
                .find(|(bound, _)| bound == formal)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            regions.push(*actual);
        }
        Ok(self
            .generic_substitution(
                check_context,
                node,
                parameters,
                caller,
                SemanticRule::Type5,
                written_parameters.len(),
            )?
            .with_regions(regions))
    }

    /// [GRAM-2, FORM-3] no nominal declares a region parameter in v0.60, so
    /// a written argument list carries type, const and function arguments
    /// alone and this application binds no region.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "the caller's argument-reading path is fallible"
    )]
    fn nominal_region_arguments(
        _node: NodeId,
        region_parameters: &[DeclarationId],
        _caller: &GenericSubstitution,
    ) -> Result<Vec<(DeclarationId, DeclarationId)>, CheckStop> {
        debug_assert!(region_parameters.is_empty());
        Ok(Vec::new())
    }

    /// One argument list, read for two callee classes.
    ///
    /// [DIAG-1] selects the cited rule by the callee's class rather than by
    /// the kind of argument problem, and these two classes differ: a
    /// user-generic call cites FN-2, a generic nominal's construct cites
    /// TYPE-5. The rule therefore arrives from the caller that knows its own
    /// class, instead of being chosen here from the shape of the failure.
    pub(super) fn generic_substitution(
        &mut self,
        check_context: &CheckContext<'_>,
        node: NodeId,
        parameters: &[GenericParameter],
        caller: &GenericSubstitution,
        argument_rule: SemanticRule,
        leading_regions: usize,
    ) -> Result<GenericSubstitution, CheckStop> {
        let written = match self.types.declarations.tree.argument_list(node)? {
            Some(list) => self
                .types
                .declarations
                .tree
                .children_with(list, Production::Targ)?,
            None if parameters.is_empty() && leading_regions == 0 => {
                return Ok(GenericSubstitution::default());
            }
            None => {
                return self.types.declarations.issue_node(
                    argument_rule,
                    node,
                    SemanticIssueKind::type_mismatch(
                        crate::semantic::written_count(
                            parameters.len() + leading_regions,
                            "generic argument",
                        ),
                        "no explicit argument list",
                    ),
                );
            }
        };
        if written.len() < leading_regions {
            return self.types.declarations.behavior_mismatch(
                argument_rule,
                node,
                "write every undetermined region before the generic arguments",
            );
        }
        let arguments =
            self.expand_written_arguments(check_context, &written[leading_regions..], caller)?;
        if arguments.len() != parameters.len() {
            return self.types.declarations.issue_node(
                argument_rule,
                node,
                SemanticIssueKind::type_mismatch(
                    crate::semantic::written_count(parameters.len(), "expanded generic argument"),
                    crate::semantic::written_count(arguments.len(), "expanded generic argument"),
                ),
            );
        }
        let mut bindings = Vec::with_capacity(parameters.len());
        let mut binding_sites = Vec::new();
        for (parameter, argument) in parameters.iter().copied().zip(arguments) {
            let source = argument.source();
            let value = match argument {
                super::behavior::WrittenArgument::Expanded { value, .. } => value,
                super::behavior::WrittenArgument::Source(source) => match parameter {
                    GenericParameter::Type { .. } => {
                        let Some(ty) = self
                            .types
                            .declarations
                            .tree
                            .first_child_with(source, Production::Type)?
                        else {
                            return self.types.declarations.behavior_mismatch(
                                argument_rule,
                                source,
                                "a type argument occupies this parameter position",
                            );
                        };
                        GenericArgument::Type(self.parse_type_with(check_context, ty, caller)?)
                    }
                    GenericParameter::Const { .. } => {
                        let Some(value) = self
                            .types
                            .declarations
                            .tree
                            .first_child_with(source, Production::Const)?
                        else {
                            return self.types.declarations.behavior_mismatch(
                                argument_rule,
                                source,
                                "a const argument occupies this parameter position",
                            );
                        };
                        GenericArgument::Const(self.parse_const_expression_with(
                            check_context,
                            value,
                            caller,
                        )?)
                    }
                    GenericParameter::Function { .. } => GenericArgument::Function(
                        self.parse_function_argument(check_context, source, caller)?,
                    ),
                },
            };
            match (parameter, value) {
                (GenericParameter::Type { declaration, bound }, GenericArgument::Type(ty)) => {
                    let requirement = match bound {
                        GenericBound::Int
                            if !matches!(
                                ty,
                                CheckedType::Integer(_) | CheckedType::GenericInt(_)
                            ) =>
                        {
                            Some("an integer type, which the parameter's `Int` bound requires")
                        }
                        GenericBound::Float
                            if !matches!(
                                ty,
                                CheckedType::Float(_) | CheckedType::GenericFloat(_)
                            ) =>
                        {
                            Some("a float type, which the parameter's `Float` bound requires")
                        }
                        _ => None,
                    };
                    if let Some(required) = requirement {
                        return self.types.declarations.issue_node(
                            SemanticRule::Fn3,
                            source,
                            SemanticIssueKind::type_mismatch(
                                required,
                                self.types.checked_type_name(ty)?,
                            ),
                        );
                    }
                    if let GenericBound::Class(required) = bound {
                        let spelling = self.types.declarations.declaration_spelling(declaration)?;
                        self.types.check_linearity_bound(
                            check_context,
                            &spelling,
                            required,
                            ty,
                            source,
                        )?;
                    }
                }
                (GenericParameter::Const { .. }, GenericArgument::Const(_))
                | (GenericParameter::Function { .. }, GenericArgument::Function(_)) => {}
                _ => {
                    return self.types.declarations.behavior_mismatch(
                        argument_rule,
                        source,
                        "the expanded argument kind matches its formal parameter",
                    );
                }
            }
            if matches!(value, GenericArgument::Function(_)) {
                binding_sites.push((parameter.key(), source));
            }
            bindings.push((parameter.key(), value));
        }
        let substitution = GenericSubstitution::from_bindings(bindings)?;
        self.types
            .record_behavior_binding_sites(&substitution, &binding_sites)?;
        Ok(substitution)
    }
}

impl<'unit> TypeContext<'unit> {
    /// The written integer type of one const `gparam` [MSR-6].
    ///
    /// A const generic is declared by exactly one function or nominal
    /// template, and its `gparam` fixes its type once; scanning the templates
    /// reads that one declaration rather than re-deriving it from a use.
    pub(super) fn const_generic_type(
        &self,
        declaration: DeclarationId,
    ) -> Result<IntegerType, CheckStop> {
        self.const_generic_types()
            .find_map(|(candidate, ty)| (candidate == declaration).then_some(ty))
            .ok_or_else(|| SemanticCompilerFailure::InvalidResolution.into())
    }
    /// The declared type of each const parameter, before any caller supplies
    /// it to a differently typed const formal or uses it as a storage extent.
    /// ENT-2's symbolic constant identity remains its declaration in each use.
    pub(super) fn const_generic_types(
        &self,
    ) -> impl Iterator<Item = (DeclarationId, IntegerType)> + '_ {
        self.function_templates
            .iter()
            .flat_map(|template| template.generic_parameters.iter())
            .chain(
                self.nominal_templates
                    .iter()
                    .flat_map(|template| template.generic_parameters.iter()),
            )
            .chain(
                self.behavior
                    .formals
                    .values()
                    .flat_map(|formal| formal.parameters.iter()),
            )
            .filter_map(|parameter| match parameter {
                GenericParameter::Const { declaration, ty } => Some((*declaration, *ty)),
                _ => None,
            })
    }
    pub(super) fn collect_function_templates(
        &mut self,
        check_context: &CheckContext<'_>,
        items: &[NodeId],
    ) -> Result<(), CheckStop> {
        self.collect_function_template_inventory(check_context, items)?;
        Ok(())
    }
    fn collect_function_template_inventory(
        &mut self,
        check_context: &CheckContext<'_>,
        items: &[NodeId],
    ) -> Result<(), CheckStop> {
        let nodes = items
            .iter()
            .copied()
            .filter(|node| {
                self.declarations
                    .tree
                    .production(*node)
                    .is_ok_and(|production| {
                        matches!(production, Production::FnDecl | Production::FnSig)
                    })
            })
            .collect::<Vec<_>>();
        for node in nodes {
            self.collect_function_template(check_context, node)?;
        }
        Ok(())
    }
    fn collect_function_template(
        &mut self,
        check_context: &CheckContext<'_>,
        node: NodeId,
    ) -> Result<(), CheckStop> {
        let declaration = self
            .declarations
            .declaration_at(node, DeclarationRole::Function)?;
        if self
            .templates_by_declaration
            .contains_key(&declaration.id())
        {
            return Ok(());
        }
        let template = FunctionTemplate {
            declaration: declaration.id(),
            node,
            name: declaration.spelling().to_owned(),
            generic_parameters: self.parse_generic_parameters(check_context, node)?,
        };
        let index = self.function_templates.len();
        if self
            .templates_by_declaration
            .insert(template.declaration, index)
            .is_some()
        {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        self.function_templates.push(template);
        Ok(())
    }
    /// Whether every argument of this type has a concrete identity. Source
    /// nominal identity follows its arguments, not its potentially cyclic
    /// fields; structural element handles are expanded through the inventory.
    fn concrete_type_identity(&self, ty: CheckedType) -> Result<bool, CheckStop> {
        Ok(match ty {
            CheckedType::Generic(_) | CheckedType::GenericInt(_) | CheckedType::GenericFloat(_) => {
                false
            }
            CheckedType::Nominal(id) => {
                if let Some((_, substitution)) = self.source_nominal_instance_entry(id)? {
                    self.concrete_substitution_identity(substitution)?
                } else {
                    match &self.nominal(id)?.kind {
                        CheckedNominalKind::Struct { fields } => {
                            let mut concrete = true;
                            for field in fields {
                                concrete &= self.concrete_type_identity(field.ty)?;
                            }
                            concrete
                        }
                        CheckedNominalKind::Enum { variants } => {
                            let mut concrete = true;
                            for field in variants.iter().flat_map(|variant| &variant.fields) {
                                concrete &= self.concrete_type_identity(field.ty)?;
                            }
                            concrete
                        }
                        CheckedNominalKind::Box { referent, .. } => {
                            self.concrete_type_identity(*referent)?
                        }
                        CheckedNominalKind::Opaque => {
                            return Err(SemanticCompilerFailure::InvalidResolution.into());
                        }
                    }
                }
            }
            CheckedType::Array { element, length } => {
                length.is_concrete() && self.concrete_type_identity(self.element_type(element)?)?
            }
            CheckedType::Buffer { element } => {
                self.concrete_type_identity(self.element_type(element)?)?
            }
            CheckedType::Window {
                element, capacity, ..
            } => {
                capacity.is_none_or(CheckedConst::is_concrete)
                    && self.concrete_type_identity(self.element_type(element)?)?
            }
            _ => true,
        })
    }

    pub(super) fn concrete_substitution_identity(
        &self,
        substitution: &GenericSubstitution,
    ) -> Result<bool, CheckStop> {
        for (_, argument) in substitution.entries() {
            let concrete = match argument {
                GenericArgument::Type(ty) => self.concrete_type_identity(*ty)?,
                GenericArgument::Const(value) => value.is_concrete(),
                GenericArgument::Function(value) => value.is_concrete(),
            };
            if !concrete {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// The old schema-discovery ordering is a compatibility contract for LLVM
    /// output. Render that ordering key directly from retained identities; no
    /// mirrored type value is built, and nothing is reified from the key.
    fn substitution_order_key(
        &self,
        substitution: &GenericSubstitution,
    ) -> Result<String, CheckStop> {
        let mut out = String::new();
        self.write_substitution_identity(substitution, &mut out, true, &mut HashSet::new())?;
        Ok(out)
    }

    pub(super) fn stable_type_spelling(&self, ty: CheckedType) -> Option<String> {
        let mut out = String::new();
        self.write_type_identity(ty, &mut out, false, &mut HashSet::new())
            .ok()?;
        Some(out)
    }

    fn stable_instance_suffix(&self, substitution: &GenericSubstitution) -> Option<String> {
        use std::fmt::Write as _;
        if !self.concrete_substitution_identity(substitution).ok()? {
            return None;
        }
        let mut spelling = String::new();
        self.write_substitution_identity(substitution, &mut spelling, false, &mut HashSet::new())
            .ok()?;
        let digest = crate::spec::sha256::digest(spelling.as_bytes());
        let mut suffix = String::with_capacity(16);
        for byte in &digest[..8] {
            let _ = write!(suffix, "{byte:02x}");
        }
        Some(suffix)
    }

    fn write_substitution_identity(
        &self,
        substitution: &GenericSubstitution,
        out: &mut String,
        ordering: bool,
        visiting: &mut HashSet<NominalId>,
    ) -> Result<(), CheckStop> {
        use std::fmt::Write as _;
        if ordering {
            out.push_str("StableGenericSubstitution { bindings: [");
        } else if substitution.entries().is_empty() {
            return Ok(());
        } else {
            out.push('<');
        }
        for (index, (key, argument)) in substitution.entries().iter().enumerate() {
            if index > 0 {
                out.push_str(if ordering { ", " } else { "," });
            }
            if ordering {
                let _ = write!(out, "({key:?}, ");
            }
            match argument {
                GenericArgument::Type(ty) => {
                    if ordering {
                        out.push_str("Type(");
                    }
                    self.write_type_identity(*ty, out, ordering, visiting)?;
                    if ordering {
                        out.push(')');
                    }
                }
                GenericArgument::Const(value) => {
                    if ordering {
                        let _ = write!(out, "Const({value:?})");
                    } else {
                        out.push_str(&self.checked_const_name(*value)?);
                    }
                }
                GenericArgument::Function(value) if ordering => {
                    let _ = write!(out, "Function({value:?})");
                }
                GenericArgument::Function(super::behavior::FunctionArgument::Source {
                    reference,
                    ..
                }) => {
                    let reference = self.function_reference(*reference)?;
                    let spelling = self
                        .declarations
                        .declaration_spelling(reference.declaration)?;
                    out.push_str("fn ");
                    out.push_str(
                        &self
                            .declarations
                            .module_symbol_base(reference.declaration, &spelling),
                    );
                    self.write_substitution_identity(
                        &reference.substitution,
                        out,
                        false,
                        visiting,
                    )?;
                }
                GenericArgument::Function(super::behavior::FunctionArgument::Parameter(_)) => {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                }
            }
            if ordering {
                out.push(')');
            }
        }
        if ordering {
            let _ = write!(out, "], regions: {:?} }}", substitution.region_arguments());
        } else {
            out.push('>');
        }
        Ok(())
    }

    fn write_type_identity(
        &self,
        ty: CheckedType,
        out: &mut String,
        ordering: bool,
        visiting: &mut HashSet<NominalId>,
    ) -> Result<(), CheckStop> {
        use std::fmt::Write as _;
        match ty {
            CheckedType::Nominal(id) => {
                if !visiting.insert(id) {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                }
                if let Some((template_index, substitution)) =
                    self.source_nominal_instance_entry(id)?
                {
                    if ordering {
                        let _ = write!(
                            out,
                            "SourceNominal {{ template: {template_index}, substitution: "
                        );
                    } else {
                        let template = self
                            .nominal_templates
                            .get(template_index)
                            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                        out.push_str(
                            &self
                                .declarations
                                .module_symbol_base(template.declaration, &template.name),
                        );
                    }
                    self.write_substitution_identity(substitution, out, ordering, visiting)?;
                    if ordering {
                        out.push_str(" }");
                    }
                } else if let Some(prelude) = self.prelude_type(id) {
                    if ordering {
                        out.push_str("Prelude(");
                    }
                    match prelude {
                        PreludeType::Option(value) => {
                            out.push_str(if ordering { "Option(" } else { "Option<" });
                            self.write_type_identity(value, out, ordering, visiting)?;
                            out.push(if ordering { ')' } else { '>' });
                        }
                        PreludeType::Result(ok, error) => {
                            out.push_str(if ordering { "Result(" } else { "Result<" });
                            self.write_type_identity(ok, out, ordering, visiting)?;
                            out.push_str(if ordering { ", " } else { "," });
                            self.write_type_identity(error, out, ordering, visiting)?;
                            out.push(if ordering { ')' } else { '>' });
                        }
                        PreludeType::Overflow => out.push_str("Overflow"),
                        PreludeType::DivError => out.push_str("DivError"),
                        PreludeType::NarrowError => out.push_str("NarrowError"),
                    }
                    if ordering {
                        out.push(')');
                    }
                } else if let Some((results, _)) = self
                    .result_list_nominals
                    .iter()
                    .find(|(_, candidate)| **candidate == id)
                {
                    out.push_str(if ordering { "ResultList([" } else { "(" });
                    for (index, (name, ty)) in results.iter().enumerate() {
                        if index > 0 {
                            out.push_str(if ordering { ", " } else { "," });
                        }
                        if ordering {
                            let _ = write!(out, "({name:?}, ");
                        } else {
                            out.push_str(name);
                            out.push(':');
                        }
                        self.write_type_identity(*ty, out, ordering, visiting)?;
                        if ordering {
                            out.push(')');
                        }
                    }
                    out.push_str(if ordering { "])" } else { ")" });
                } else if let CheckedNominalKind::Box {
                    referent, region, ..
                } = self.nominal(id)?.kind
                {
                    if ordering {
                        let _ = write!(out, "Boxed {{ region: {region:?}, referent: ");
                    } else {
                        out.push_str("Box<");
                    }
                    self.write_type_identity(referent, out, ordering, visiting)?;
                    out.push_str(if ordering { " }" } else { ">" });
                } else {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                }
                visiting.remove(&id);
            }
            CheckedType::Array { element, length } => {
                out.push_str(if ordering {
                    "Array { element: StableElement("
                } else {
                    "Array<"
                });
                self.write_type_identity(self.element_type(element)?, out, ordering, visiting)?;
                if ordering {
                    let _ = write!(out, "), length: {length:?} }}");
                } else {
                    out.push(',');
                    out.push_str(&self.checked_const_name(length)?);
                    out.push('>');
                }
            }
            CheckedType::Buffer { element } => {
                out.push_str(if ordering {
                    "Buffer { element: StableElement("
                } else {
                    "Array<"
                });
                self.write_type_identity(self.element_type(element)?, out, ordering, visiting)?;
                out.push_str(if ordering { ") }" } else { ">" });
            }
            CheckedType::Window {
                shape,
                element,
                capacity,
            } => {
                if ordering {
                    let _ = write!(out, "Window {{ shape: {shape:?}, element: StableElement(");
                } else {
                    out.push_str(shape.spelling());
                    out.push('<');
                }
                self.write_type_identity(self.element_type(element)?, out, ordering, visiting)?;
                if ordering {
                    let _ = write!(out, "), capacity: {capacity:?} }}");
                } else {
                    if let Some(capacity) = capacity {
                        out.push(',');
                        out.push_str(&self.checked_const_name(capacity)?);
                    }
                    out.push('>');
                }
            }
            _ => {
                if ordering {
                    let _ = write!(out, "Scalar({ty:?})");
                } else {
                    out.push_str(&self.checked_type_name(ty)?);
                }
            }
        }
        Ok(())
    }
    pub(super) fn parse_generic_parameters(
        &self,
        check_context: &CheckContext<'_>,
        declaration: NodeId,
    ) -> Result<Vec<GenericParameter>, CheckStop> {
        let Some(generics) = self
            .declarations
            .tree
            .first_child_with(declaration, Production::Generics)?
        else {
            return Ok(Vec::new());
        };
        let mut parameters = Vec::new();
        for node in self
            .declarations
            .tree
            .children_with(generics, Production::Gparam)?
        {
            if let Some(signature) = self
                .declarations
                .tree
                .first_child_with(node, Production::FnSig)?
            {
                let declaration = self
                    .declarations
                    .declaration_at(signature, DeclarationRole::FunctionParameter)?
                    .id();
                parameters.push(GenericParameter::Function {
                    key: GenericParameterKey::Source(declaration),
                    signature,
                });
                continue;
            }
            if let Some(application) = self.declarations.tree.group_application(node)? {
                parameters.extend(self.expand_formal_parameters(check_context, application)?);
                continue;
            }
            if self
                .declarations
                .tree
                .has_fixed(node, FixedTerminal::Const)?
            {
                let declaration = self
                    .declarations
                    .declaration_at(node, DeclarationRole::ConstGeneric)?
                    .id();
                let ty = self
                    .declarations
                    .tree
                    .first_child_with(node, Production::Type)?
                    .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
                let Some(ty) = self.declarations.integer_type(ty)? else {
                    return self.declarations.issue_node(
                        SemanticRule::Const1,
                        ty,
                        SemanticIssueKind::InvalidConstValue,
                    );
                };
                parameters.push(GenericParameter::Const { declaration, ty });
                continue;
            }
            let declaration = self
                .declarations
                .declaration_at(node, DeclarationRole::GenericType)?
                .id();
            // [GRAM-2, PROV-6] the bound is optional and never inferred: a
            // `capability_bound` atom, a numeric marker TYPEID, or nothing,
            // and an absent bound grants the body no capability, which is
            // the linear class read at the parameter.
            let bound = match self
                .declarations
                .resolved
                .lexical_uses_at(node)
                .find(|usage| usage.role() == LexicalUseRole::GenericBound)
                .map(|usage| (usage.target(), usage.origin().coordinate()))
            {
                None => GenericBound::Class(
                    self.declarations
                        .written_linearity_bound(node)?
                        .unwrap_or(super::linearity::LinearityClass::Linear),
                ),
                Some((ResolvedTarget::Prelude(id), _)) if id == BuiltinPreludeId::INT => {
                    GenericBound::Int
                }
                Some((ResolvedTarget::Prelude(id), _)) if id == BuiltinPreludeId::FLOAT => {
                    GenericBound::Float
                }
                Some((
                    ResolvedTarget::Source {
                        class: DeclarationClass::NumericBound,
                        ..
                    },
                    coordinate,
                )) => {
                    return self.declarations.issue_at(
                        SemanticRule::Fn3,
                        node,
                        coordinate,
                        SemanticIssueKind::SourceContractGenericBound,
                    );
                }
                Some(_) => return Err(SemanticCompilerFailure::InvalidResolution.into()),
            };
            parameters.push(GenericParameter::Type { declaration, bound });
        }
        Ok(parameters)
    }
}

impl<'unit> DeclarationInventory<'unit> {
    pub(super) fn call_is_inside_postcondition(&self, call: NodeId) -> Result<bool, CheckStop> {
        self.node_is_inside_postcondition(call)
    }
    pub(super) fn node_is_inside_postcondition(&self, node: NodeId) -> Result<bool, CheckStop> {
        let path = self.tree.path(node)?.components();
        Ok(self.resolved.postconditions().iter().any(|record| {
            let block = record.block.components();
            path.len() > block.len() && path.starts_with(block)
        }))
    }
    /// The shared split for user-call instantiation, region binding, and
    /// syntactic generic-cycle judgments. Kernel rows keep their own table
    /// argument order and do not use this function [FORM-8, FN-2, FN-6].
    pub(super) fn user_call_region_prefix(&self, arguments: &[NodeId]) -> Result<usize, CheckStop> {
        let mut count = 0;
        for argument in arguments {
            if self
                .tree
                .first_child_with(*argument, Production::Type)?
                .is_some()
                || self
                    .tree
                    .first_child_with(*argument, Production::Const)?
                    .is_some()
                || self
                    .tree
                    .first_child_with(*argument, Production::FunctionArg)?
                    .is_some()
            {
                break;
            }
            count += 1;
        }
        Ok(count)
    }
}
