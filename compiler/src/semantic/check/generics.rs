use std::collections::{HashMap, HashSet};

mod finiteness;
mod operands;

use crate::syntax::NodeId;
use crate::{
    BuiltinPreludeId, DeclarationClass, DeclarationId, DeclarationRole, FixedTerminal,
    LexicalUseRole, Production, ResolvedTarget, SemanticCompilerFailure, SemanticIssueKind,
    SemanticRule,
};

use super::super::goal::{CheckedRequirement, GoalDatum, GoalExpression, GoalOperation};
use super::super::model::{
    CheckedConst, CheckedElement, CheckedGenericRequirement, CheckedNominalKind, CheckedType,
    CheckedValue, IntegerType, NominalId,
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

/// Nominal-arena-independent identity for a concrete substitution discovered
/// while replaying generic source bodies. Replay intentionally runs in a
/// scratch nominal suffix; only this structural form crosses its rollback.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct StableGenericSubstitution {
    bindings: Vec<(GenericParameterKey, StableGenericArgument)>,
    regions: Vec<(DeclarationId, DeclarationId)>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum StableGenericArgument {
    Type(StableCheckedType),
    Const(CheckedConst),
    Function(super::behavior::FunctionArgument),
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum StableCheckedType {
    Scalar(CheckedType),
    SourceNominal {
        template: usize,
        substitution: StableGenericSubstitution,
    },
    Prelude(StablePreludeType),
    ResultList(Vec<(String, StableCheckedType)>),
    Boxed {
        region: Option<DeclarationId>,
        referent: Box<StableCheckedType>,
    },
    Array {
        element: StableElement,
        length: CheckedConst,
    },
    Buffer {
        element: StableElement,
    },
    Window {
        shape: super::super::model::WindowShape,
        element: StableElement,
        capacity: Option<CheckedConst>,
    },
}

/// A structural bridge across speculative nominal rollback.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct StableElement(Box<StableCheckedType>);

/// One symbolic generic requirement while its scratch nominal suffix is
/// rolled back. The checked predicate remains exact, but every scratch
/// nominal it mentions has a structural bridge that can be re-interned only
/// after the executable nominal prefix is closed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PendingGenericRequirement {
    declaration: DeclarationId,
    requirement: CheckedRequirement,
    nominal_checkpoint: usize,
    replacements: Vec<(NominalId, StableCheckedType)>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum StablePreludeType {
    Option(Box<StableCheckedType>),
    Result(Box<StableCheckedType>, Box<StableCheckedType>),
    Overflow,
    DivError,
    NarrowError,
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

impl<'unit> Checker<'unit> {
    /// Substitute captured brands without reintroducing scratch nominal IDs
    /// into the structural identity of a function argument.
    pub(super) fn substitute_stable_regions(
        &self,
        substitution: &mut StableGenericSubstitution,
        regions: &[(DeclarationId, DeclarationId)],
    ) -> Result<(), CheckStop> {
        for (_, actual) in &mut substitution.regions {
            *actual = Self::substituted_region(regions, *actual);
        }
        for (_, argument) in &mut substitution.bindings {
            match argument {
                StableGenericArgument::Type(ty) => {
                    self.substitute_stable_type_regions(ty, regions)?
                }
                StableGenericArgument::Function(value) => {
                    *value = self.substitute_function_argument_regions(*value, regions)?
                }
                StableGenericArgument::Const(_) => {}
            }
        }
        Ok(())
    }

    fn substitute_stable_type_regions(
        &self,
        ty: &mut StableCheckedType,
        regions: &[(DeclarationId, DeclarationId)],
    ) -> Result<(), CheckStop> {
        match ty {
            StableCheckedType::Scalar(_) => {}
            StableCheckedType::SourceNominal { substitution, .. } => {
                self.substitute_stable_regions(substitution, regions)?
            }
            StableCheckedType::Prelude(prelude) => match prelude {
                StablePreludeType::Option(value) => {
                    self.substitute_stable_type_regions(value, regions)?
                }
                StablePreludeType::Result(value, error) => {
                    self.substitute_stable_type_regions(value, regions)?;
                    self.substitute_stable_type_regions(error, regions)?;
                }
                StablePreludeType::Overflow
                | StablePreludeType::DivError
                | StablePreludeType::NarrowError => {}
            },
            StableCheckedType::ResultList(results) => {
                for (_, ty) in results {
                    self.substitute_stable_type_regions(ty, regions)?;
                }
            }
            StableCheckedType::Boxed { region, referent } => {
                if let Some(region) = region {
                    *region = Self::substituted_region(regions, *region);
                }
                self.substitute_stable_type_regions(referent, regions)?;
            }
            StableCheckedType::Array { element, .. }
            | StableCheckedType::Window { element, .. } => {
                self.substitute_stable_type_regions(&mut element.0, regions)?
            }
            StableCheckedType::Buffer { element } => {
                self.substitute_stable_type_regions(&mut element.0, regions)?
            }
        }
        Ok(())
    }

    pub(super) fn collect_function_templates(&mut self, items: &[NodeId]) -> Result<(), CheckStop> {
        self.collect_function_template_inventory(items)?;
        Ok(())
    }

    /// Builds only the source template inventory and exact generic-cycle
    /// judgment needed by the throwaway selector checker. Generic bodies are
    /// ordinary semantic premises and are checked later by the real H0 path.
    pub(super) fn collect_function_templates_for_postconditions(
        &mut self,
        items: &[NodeId],
    ) -> Result<(), CheckStop> {
        let nodes = items
            .iter()
            .copied()
            .filter(|node| {
                self.tree.production(*node).is_ok_and(|production| {
                    matches!(production, Production::FnDecl | Production::FnSig)
                })
            })
            .collect::<Vec<_>>();
        for node in nodes {
            match self.collect_function_template(node) {
                Ok(()) => {}
                Err(CheckStop::Issue(_) | CheckStop::Unsupported(_)) => {
                    let declaration = self.declaration_at(node, DeclarationRole::Function)?.id();
                    self.mark_postcondition_unavailable(declaration);
                }
                Err(stop) => return Err(stop),
            }
        }
        Ok(())
    }

    fn collect_function_template_inventory(&mut self, items: &[NodeId]) -> Result<(), CheckStop> {
        let nodes = items
            .iter()
            .copied()
            .filter(|node| {
                self.tree.production(*node).is_ok_and(|production| {
                    matches!(production, Production::FnDecl | Production::FnSig)
                })
            })
            .collect::<Vec<_>>();
        for node in nodes {
            self.collect_function_template(node)?;
        }
        Ok(())
    }

    fn collect_function_template(&mut self, node: NodeId) -> Result<(), CheckStop> {
        let declaration = self.declaration_at(node, DeclarationRole::Function)?;
        let template = FunctionTemplate {
            declaration: declaration.id(),
            node,
            name: declaration.spelling().to_owned(),
            generic_parameters: self.parse_generic_parameters(node)?,
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

    pub(super) fn collect_concrete_function_signatures(&mut self) -> Result<(), CheckStop> {
        self.collect_concrete_function_signatures_with(false)
    }

    /// Scratch-only counterpart used by the FN-9 selector preflight.
    ///
    /// A source-side call that has not completed FN-2 establishes no selector
    /// instance.  The throwaway checker may therefore skip that edge while it
    /// discovers every independently successful instance.  The ordinary
    /// checker keeps the strict path above, so its source diagnostics and
    /// no-`ensures` behavior are unchanged.
    pub(super) fn collect_concrete_function_signatures_for_postconditions(
        &mut self,
    ) -> Result<(), CheckStop> {
        self.collect_concrete_function_signatures_with(true)
    }

    fn collect_concrete_function_signatures_with(
        &mut self,
        tolerate_source_failure: bool,
    ) -> Result<(), CheckStop> {
        for template_index in 0..self.function_templates.len() {
            if tolerate_source_failure
                && self.postcondition_declaration_unavailable(
                    self.function_templates[template_index].declaration,
                )
            {
                continue;
            }
            if self.function_templates[template_index]
                .generic_parameters
                .is_empty()
            {
                let result = if tolerate_source_failure {
                    self.instantiate_function_signature_for_postconditions(
                        template_index,
                        GenericSubstitution::default(),
                    )
                } else {
                    self.instantiate_function_signature(
                        template_index,
                        GenericSubstitution::default(),
                    )
                };
                match result {
                    Ok(()) => {}
                    Err(
                        CheckStop::Issue(_)
                        | CheckStop::Unsupported(_)
                        | CheckStop::PostconditionPrerequisiteUnavailable,
                    ) if tolerate_source_failure => {}
                    Err(stop) => return Err(stop),
                }
            }
        }
        self.materialize_actual_groups(tolerate_source_failure)?;
        self.discover_called_function_signatures(true, tolerate_source_failure)
    }

    fn discover_called_function_signatures(
        &mut self,
        require_concrete: bool,
        tolerate_source_failure: bool,
    ) -> Result<(), CheckStop> {
        let mut cursor = 0_usize;
        let mut nominal_cursor = 0_usize;
        while cursor < self.signatures.len() || nominal_cursor < self.source_nominal_instances.len()
        {
            // A function argument is checked at every instantiation boundary,
            // including a nominal used only in a signature. Those bindings
            // can themselves name functions with further nominal instances.
            while nominal_cursor < self.source_nominal_instances.len() {
                let instance = self.source_nominal_instances[nominal_cursor].clone();
                nominal_cursor += 1;
                let Some((_, substitution)) = instance else {
                    continue;
                };
                if require_concrete && !substitution.is_concrete(&self.elements.borrow()) {
                    continue;
                }
                for (key, argument) in substitution.entries() {
                    let GenericArgument::Function(argument) = argument else {
                        continue;
                    };
                    self.ensure_formal_nominals(*key, &substitution)?;
                    self.materialize_function_argument(*argument)?;
                    // Only stable concrete declaration roots outlive the
                    // source-schema scratch inventory. Symbolic hypotheses
                    // are already reached through its signature arguments.
                    if argument.is_concrete()
                        && !self.behavior.declaration_arguments.contains(argument)
                    {
                        self.behavior.declaration_arguments.push(*argument);
                    }
                }
            }
            if cursor == self.signatures.len() {
                continue;
            }
            let signature = self.signatures[cursor].clone();
            for (key, argument) in signature.substitution.entries() {
                if let GenericArgument::Function(argument) = argument {
                    if require_concrete && !argument.is_concrete() {
                        return Err(SemanticCompilerFailure::InvalidResolution.into());
                    }
                    self.ensure_formal_nominals(*key, &signature.substitution)?;
                    self.materialize_function_argument(*argument)?;
                }
            }
            if signature.formal_parameter.is_some() {
                cursor += 1;
                continue;
            }
            for call in self
                .tree
                .descendants_with(signature.node, Production::Call)?
            {
                if self.call_is_inside_postcondition(call)? {
                    continue;
                }
                if let Some(key) = self.behavior_call_key(call)? {
                    let argument = signature
                        .substitution
                        .function_argument(key)
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                    self.ensure_formal_nominals(key, &signature.substitution)?;
                    self.materialize_function_argument(argument)?;
                    continue;
                }
                let Some((template_index, template)) = self.called_function_template(call)? else {
                    continue;
                };
                if tolerate_source_failure
                    && self.postcondition_declaration_unavailable(template.declaration)
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
                if self.operand_directed_row_index(&template)?.is_some() {
                    continue;
                }
                if tolerate_source_failure && !self.postcondition_call_arguments_have_links(call)? {
                    continue;
                }
                if tolerate_source_failure && let Some(targs) = self.tree.argument_list(call)? {
                    let checkpoint = self.nominal_checkpoint();
                    match self.ensure_nominals_in_node(targs, &signature.substitution) {
                        Ok(()) => {}
                        Err(
                            CheckStop::Issue(_)
                            | CheckStop::Unsupported(_)
                            | CheckStop::PostconditionPrerequisiteUnavailable,
                        ) => {
                            self.restore_nominal_checkpoint(checkpoint)?;
                            continue;
                        }
                        Err(stop) => return Err(stop),
                    }
                }
                let substitution = match self.call_generic_substitution(
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
                if require_concrete && !substitution.is_concrete(&self.elements.borrow()) {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                }
                let already_present = self
                    .functions_by_declaration
                    .get(&template.declaration)
                    .into_iter()
                    .flatten()
                    .any(|id| {
                        self.signatures
                            .get(id.0 as usize)
                            .is_some_and(|instance| instance.substitution == substitution)
                    });
                if !already_present {
                    self.record_instance_request(template.node, &substitution, call);
                    let result = if tolerate_source_failure {
                        self.instantiate_function_signature_for_postconditions(
                            template_index,
                            substitution,
                        )
                    } else {
                        self.instantiate_function_signature(template_index, substitution)
                    }
                    .map_err(|stop| self.attribute_to_call(call, stop));
                    match result {
                        Ok(()) => {}
                        Err(
                            CheckStop::Issue(_)
                            | CheckStop::Unsupported(_)
                            | CheckStop::PostconditionPrerequisiteUnavailable,
                        ) if tolerate_source_failure => {}
                        Err(stop) => return Err(stop),
                    }
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
        let Some(targs) = self.tree.argument_list(call)? else {
            return Ok(true);
        };
        let owner = self.tree.path(targs)?.components();
        if self.resolved.lexical_uses().iter().any(|usage| {
            let path = usage.origin().node().components();
            path.len() >= owner.len()
                && path.starts_with(owner)
                && match usage.target() {
                    ResolvedTarget::Source {
                        declaration,
                        class: DeclarationClass::NamedConst,
                    } => !self.constants.contains_key(&declaration),
                    ResolvedTarget::Source {
                        declaration,
                        class: DeclarationClass::NominalType,
                    } => {
                        self.postcondition_declaration_unavailable(declaration)
                            || !self
                                .nominal_templates_by_declaration
                                .contains_key(&declaration)
                    }
                    _ => false,
                }
        }) {
            return Ok(false);
        }
        for ty in self.tree.descendants_with(targs, Production::Type)? {
            if self.tree.names_nominal(ty)?
                && !self.resolved.lexical_uses_at(ty).any(|usage| {
                    matches!(
                        usage.role(),
                        LexicalUseRole::Type | LexicalUseRole::TypeArgument
                    )
                })
            {
                return Ok(false);
            }
        }
        for constant in self.tree.descendants_with(targs, Production::Const)? {
            let identifiers = self.tree.direct_identifiers(constant)?;
            if !identifiers.is_empty() {
                let uses = self
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
                        && !self.constants.contains_key(&declaration)
                    {
                        return Ok(false);
                    }
                }
            }
        }
        for argument in self.tree.children_with(targs, Production::Targ)? {
            if self
                .tree
                .first_child_with(argument, Production::Type)?
                .is_some()
                || self
                    .tree
                    .first_child_with(argument, Production::Const)?
                    .is_some()
                || self
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
            .tree
            .first_child_with(call, Production::Callee)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let Some(usage) = self
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
        let Some(index) = self.templates_by_declaration.get(&declaration).copied() else {
            if self.postcondition_declaration_unavailable(declaration) {
                return Ok(None);
            }
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        };
        let template = self
            .function_templates
            .get(index)
            .cloned()
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        Ok(Some((index, template)))
    }

    pub(super) fn concrete_function_for_call(
        &self,
        node: NodeId,
        declaration: DeclarationId,
        caller: &GenericSubstitution,
    ) -> Result<super::super::model::FunctionId, CheckStop> {
        let template_index = *self
            .templates_by_declaration
            .get(&declaration)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let template = self
            .function_templates
            .get(template_index)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let substitution = self.call_generic_substitution(node, template, caller)?;
        self.functions_by_declaration
            .get(&declaration)
            .into_iter()
            .flatten()
            .copied()
            .find(|id| {
                self.signatures
                    .get(id.0 as usize)
                    .is_some_and(|instance| instance.substitution == substitution)
            })
            .ok_or_else(|| SemanticCompilerFailure::InvalidResolution.into())
    }

    /// The instance one call to an operand-directed [PRE-1] row selects
    /// [OP-10, OP-11, OP-14], or `None` where the callee is any other row.
    ///
    /// The instance is keyed on the operand's own type, so it cannot exist
    /// before the body reaches the call. A substitution with no built
    /// signature is recorded and the function is retried, exactly as a
    /// derived nominal is.
    pub(super) fn operand_directed_function_for_call(
        &self,
        node: NodeId,
        declaration: DeclarationId,
        bindings: &HashMap<DeclarationId, super::LocalBinding>,
    ) -> Result<Option<super::super::model::FunctionId>, CheckStop> {
        let Some(&template_index) = self.templates_by_declaration.get(&declaration) else {
            return Ok(None);
        };
        let template = self
            .function_templates
            .get(template_index)
            .cloned()
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let Some(row_index) = self.operand_directed_row_index(&template)? else {
            return Ok(None);
        };
        let substitution =
            self.operand_directed_substitution(node, &template, row_index, bindings)?;
        if let Some(id) = self
            .functions_by_declaration
            .get(&declaration)
            .into_iter()
            .flatten()
            .copied()
            .find(|id| {
                self.signatures
                    .get(id.0 as usize)
                    .is_some_and(|instance| instance.substitution == substitution)
            })
        {
            return Ok(Some(id));
        }
        self.pending_instances
            .borrow_mut()
            .push((template_index, substitution));
        Err(CheckStop::DeferredNominal)
    }

    /// Builds one deferred operand-directed instance and admits the [FN-9]
    /// selectors of its declared `ensures`, which the ordinary pre-phase-A
    /// admission could not reach.
    pub(super) fn ensure_operand_directed_instance(
        &mut self,
        template_index: usize,
        substitution: GenericSubstitution,
    ) -> Result<(), CheckStop> {
        let template = self
            .function_templates
            .get(template_index)
            .cloned()
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        if self
            .functions_by_declaration
            .get(&template.declaration)
            .into_iter()
            .flatten()
            .copied()
            .any(|id| {
                self.signatures
                    .get(id.0 as usize)
                    .is_some_and(|instance| instance.substitution == substitution)
            })
        {
            return Ok(());
        }
        let id = super::super::model::FunctionId(
            u32::try_from(self.signatures.len())
                .map_err(|_| SemanticCompilerFailure::CounterOverflow)?,
        );
        self.instantiate_function_signature(template_index, substitution)?;
        self.admit_postcondition_selectors_for(id)
    }

    pub(super) fn instantiate_function_signature(
        &mut self,
        template_index: usize,
        substitution: GenericSubstitution,
    ) -> Result<(), CheckStop> {
        let template = self
            .function_templates
            .get(template_index)
            .cloned()
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let id = super::super::model::FunctionId(
            u32::try_from(self.signatures.len())
                .map_err(|_| SemanticCompilerFailure::CounterOverflow)?,
        );
        self.ensure_nominals_in_function(template.node, &substitution)?;
        let signature = self.build_function_signature(&template, substitution, id)?;
        self.functions_by_declaration
            .entry(template.declaration)
            .or_default()
            .push(id);
        self.signatures.push(signature);
        Ok(())
    }

    fn instantiate_function_signature_for_postconditions(
        &mut self,
        template_index: usize,
        substitution: GenericSubstitution,
    ) -> Result<(), CheckStop> {
        let template = self
            .function_templates
            .get(template_index)
            .cloned()
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let id = super::super::model::FunctionId(
            u32::try_from(self.signatures.len())
                .map_err(|_| SemanticCompilerFailure::CounterOverflow)?,
        );
        if !self.postcondition_function_header_dependencies_available(template.node)? {
            return Err(CheckStop::PostconditionPrerequisiteUnavailable);
        }
        let checkpoint = self.nominal_checkpoint();
        let prepared = self.ensure_nominals_in_function_signature(template.node, &substitution);
        let signature = match prepared
            .and_then(|()| self.build_function_signature(&template, substitution, id))
        {
            Ok(signature) => signature,
            Err(stop) => {
                self.restore_nominal_checkpoint(checkpoint)?;
                self.pending_nominals.borrow_mut().clear();
                return Err(stop);
            }
        };
        self.functions_by_declaration
            .entry(template.declaration)
            .or_default()
            .push(id);
        self.signatures.push(signature);
        Ok(())
    }

    pub(super) fn postcondition_function_header_dependencies_available(
        &self,
        function: NodeId,
    ) -> Result<bool, CheckStop> {
        let mut header_nodes = self.tree.children_with(function, Production::ParamList)?;
        let result_binding = self
            .tree
            .first_child_with(function, Production::ResultBinding)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        header_nodes.push(result_binding);
        for node in header_nodes {
            let path = self.tree.path(node)?.components();
            if self.resolved.lexical_uses().iter().any(|usage| {
                let usage_path = usage.origin().node().components();
                usage_path.len() >= path.len()
                    && usage_path.starts_with(path)
                    && match usage.target() {
                        ResolvedTarget::Source {
                            declaration,
                            class: DeclarationClass::NamedConst,
                        } => !self.constants.contains_key(&declaration),
                        ResolvedTarget::Source {
                            declaration,
                            class: DeclarationClass::NominalType,
                        } => {
                            self.postcondition_declaration_unavailable(declaration)
                                || !self
                                    .nominal_templates_by_declaration
                                    .contains_key(&declaration)
                        }
                        _ => false,
                    }
            }) {
                return Ok(false);
            }
            for ty in self.tree.descendants_with(node, Production::Type)? {
                if self.tree.names_nominal(ty)?
                    && !self.resolved.lexical_uses_at(ty).any(|usage| {
                        matches!(
                            usage.role(),
                            LexicalUseRole::Type | LexicalUseRole::TypeArgument
                        )
                    })
                {
                    return Ok(false);
                }
            }
            for constant in self.tree.descendants_with(node, Production::Const)? {
                let identifiers = self.tree.direct_identifiers(constant)?;
                if !identifiers.is_empty() {
                    let uses = self
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
        &self,
        template: &FunctionTemplate,
        substitution: GenericSubstitution,
        id: super::super::model::FunctionId,
    ) -> Result<FunctionSignature, CheckStop> {
        let _module = self.enter_module(template.declaration);
        // [GRAM-2, FORM-3] no declaration carries a region parameter in
        // v0.60: a reference is a name for a path [REF-1] and its validity is
        // the [REF-2] flow fact, not a brand on the signature.
        let region_parameters = Vec::new();
        let parameters = self.parse_parameters_with(template.node, &substitution)?;
        // [GRAM-2] the declaration writes one result or an ordered result
        // list. Every ordinal is judged by the ordinary result rules below;
        // a list additionally hands its caller one value of the compiler-owned
        // result-list nominal, which is this callable's result [CALL-4].
        let result_bindings = self
            .tree
            .children_with(template.node, Production::ResultBinding)?;
        let mut results = Vec::with_capacity(result_bindings.len());
        for binding in &result_bindings {
            let rtype = self
                .tree
                .first_child_with(*binding, Production::Rtype)?
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            // [FN-1] the binder spelling is out of callable-signature
            // equality; the resolution record carries it where a route or a
            // diagnostic names it.
            let (mode, ty) = self.parse_rtype_with(rtype, &substitution)?;
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
                    return self.issue_node(
                        SemanticRule::Ref3,
                        entry.rtype,
                        SemanticIssueKind::EscapingReference {
                            mechanical_fix: super::references::REF3_RETURN_AN_INDEX,
                        },
                    );
                }
            }
            let Some(fields) = self.result_list_fields(template.node, &substitution)? else {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            };
            let nominal = self
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
            return self.issue_node(
                SemanticRule::Ref3,
                rtype,
                SemanticIssueKind::EscapingReference {
                    mechanical_fix: super::references::REF3_RETURN_AN_INDEX,
                },
            );
        }
        let effects = self
            .tree
            .first_child_with(template.node, Production::Effects)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let mut declared_effects = self.parse_effects(effects, &parameters)?;
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
        let base = self.module_symbol_base(template.declaration, &template.name);
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
            self.stable_instance_suffix(&substitution)
                .map(|suffix| format!("{base}$instance${suffix}"))
                .filter(|candidate| {
                    !self
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

    pub(super) fn validate_generic_templates(&mut self) -> Result<(), CheckStop> {
        if !self.pending_generic_requirements.is_empty() || !self.generic_requirements.is_empty() {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        // A closed unit with no generic function declaration has no source
        // schema to validate.  Rechecking every concrete function through a
        // temporary symbolic inventory would duplicate the ordinary ENT and
        // provenance baseline without producing a schema report, and claim
        // residuality would then pay that whole-program cost once more for
        // every mask.
        if self
            .function_templates
            .iter()
            .all(|template| template.generic_parameters.is_empty())
        {
            return Ok(());
        }
        let concrete_signatures = std::mem::take(&mut self.signatures);
        let concrete_functions_by_declaration = std::mem::take(&mut self.functions_by_declaration);
        let concrete_postcondition_selectors = std::mem::take(&mut self.postcondition_selectors);
        // Bound calls checked in the scratch symbolic FunctionId inventory
        // retain exact FN-4 queries for that pass only. Preserve any earlier
        // declaration-level records, then discard the scratch suffix before
        // concrete replay assigns checked-program identities.
        let contract_query_checkpoint = self.contract_queries.borrow().len();
        let nominal_checkpoint = self.nominal_checkpoint();
        // Record only the initial source-canonical symbolic instance for each
        // generic. Transitive discovery below may instantiate another
        // symbolic shape for the same source declaration; those validate the
        // source call graph but are not a second metadata identity.
        let mut canonical_generic_signatures = Vec::new();
        for template_index in 0..self.function_templates.len() {
            let template = self
                .function_templates
                .get(template_index)
                .cloned()
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            let substitution = self.symbolic_generic_substitution(&template.generic_parameters)?;
            let signature_index = self.signatures.len();
            self.instantiate_function_signature(template_index, substitution)?;
            if !template.generic_parameters.is_empty() {
                canonical_generic_signatures.push((signature_index, template.declaration));
            }
        }
        self.materialize_actual_groups(false)?;
        self.discover_called_function_signatures(false, false)?;
        // Concrete selectors are keyed by the dense FunctionId inventory.
        // Schema validation uses a separate scratch inventory starting at
        // zero, so it must build and later discard its own selector table
        // rather than aliasing the real concrete entries by accident.
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
        self.admit_postcondition_selectors_including(&canonical_seeds)?;
        let mut phase_a = Vec::with_capacity(self.signatures.len());
        let mut index = 0_usize;
        while index < self.signatures.len() {
            // Symbolic generic validation may discover a derived box or
            // prelude nominal (for example the Result produced by a
            // `+checked` requires-local), or an operand-directed [PRE-1]
            // instance [OP-10]. Use the same deferred retry loop as concrete
            // checking; the checkpoint below discards these symbolic-only
            // instances afterwards. The dense inventory also includes
            // nongeneric callees so FN-8 requirement installation uses the
            // ordinary FunctionId-indexed path.
            phase_a.push(self.check_function_interning_nominals(index)?);
            index = index
                .checked_add(1)
                .ok_or(SemanticCompilerFailure::CounterOverflow)?;
        }
        // The retry loop above has consumed every scratch signature appended
        // during symbolic body checking. Close allocation only at that exact
        // equal-length checkpoint; restoring the concrete signature snapshot
        // below discards every scratch identity and fact together.
        self.close_allocation_metadata(&mut phase_a)?;
        for (canonical, declaration) in &canonical_generic_signatures {
            let checked = phase_a
                .get(*canonical)
                .filter(|checked| checked.function.declaration == *declaration)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            for requirement in &checked.function.requirements {
                self.pending_generic_requirements
                    .push(self.stabilize_generic_requirement(
                        *declaration,
                        requirement,
                        nominal_checkpoint,
                    )?);
            }
        }
        self.install_call_requirements(&mut phase_a)?;
        self.form_obligation_records(&mut phase_a)?;
        let callees = self.entailment_callees()?;
        self.validate_generic_body_entailment(
            &mut phase_a,
            &canonical_generic_signatures,
            &callees,
        )?;
        self.contract_queries
            .borrow_mut()
            .truncate(contract_query_checkpoint);
        self.signatures.clear();
        self.functions_by_declaration.clear();
        self.postcondition_selectors.clear();
        self.restore_nominal_checkpoint(nominal_checkpoint)?;
        self.signatures = concrete_signatures;
        self.functions_by_declaration = concrete_functions_by_declaration;
        self.postcondition_selectors = concrete_postcondition_selectors;
        let replayed_concrete = self.discover_schema_written_concrete_instances()?;
        // The replay above can append a concrete instance that is mentioned
        // only inside an uninstantiated generic body. Rebuild the selector
        // table over the final concrete inventory so those instances receive
        // the same FN-9 judgment as directly discovered instances.
        self.postcondition_selectors.clear();
        self.admit_postcondition_selectors_including(&replayed_concrete)?;
        Ok(())
    }

    /// Replays the generic source-call graph after the symbolic nominal
    /// checkpoint and retains every explicitly concrete substitution it
    /// contains. The replay carries only source template indices and freshly
    /// reconstructed substitutions, so a concrete nominal argument never
    /// leaks a scratch `NominalId` from schema validation into the executable
    /// inventory.
    fn discover_schema_written_concrete_instances(
        &mut self,
    ) -> Result<Vec<super::super::model::FunctionId>, CheckStop> {
        if !self.pending_nominals.borrow().is_empty() {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        let nominal_checkpoint = self.nominal_checkpoint();
        let discovered = (|| {
            let mut work = Vec::new();
            let mut candidates = Vec::new();
            for template_index in 0..self.function_templates.len() {
                let template = self
                    .function_templates
                    .get(template_index)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                if template.generic_parameters.is_empty() {
                    continue;
                }
                work.push((
                    template_index,
                    self.symbolic_generic_substitution(&template.generic_parameters)?,
                ));
            }
            let mut cursor = 0_usize;
            while cursor < work.len() {
                let (caller_template_index, caller_substitution) = work
                    .get(cursor)
                    .cloned()
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                let caller = self
                    .function_templates
                    .get(caller_template_index)
                    .cloned()
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                for call in self.tree.descendants_with(caller.node, Production::Call)? {
                    if self.call_is_inside_postcondition(call)? {
                        continue;
                    }
                    let Some((callee_template_index, callee)) =
                        self.called_function_template(call)?
                    else {
                        continue;
                    };
                    if callee.generic_parameters.is_empty() {
                        continue;
                    }
                    // [OP-10, OP-11, OP-14] an operand-directed row writes no
                    // type argument, so this walk over written argument lists
                    // names no instance of it. Such a row is a [PRE-1] leaf
                    // with no body and starts no instantiation cycle.
                    if self.operand_directed_row_index(&callee)?.is_some() {
                        continue;
                    }
                    if let Some(targs) = self.tree.argument_list(call)? {
                        self.ensure_nominals_in_node(targs, &caller_substitution)?;
                    }
                    let substitution =
                        self.call_generic_substitution(call, &callee, &caller_substitution)?;
                    if !work
                        .iter()
                        .any(|(candidate_template, candidate_substitution)| {
                            *candidate_template == callee_template_index
                                && candidate_substitution == &substitution
                        })
                    {
                        work.push((callee_template_index, substitution.clone()));
                    }
                    if let Some(stable) =
                        self.stabilize_concrete_substitution(&substitution, nominal_checkpoint)?
                    {
                        candidates.push((callee_template_index, callee.declaration, stable));
                    }
                }
                cursor = cursor
                    .checked_add(1)
                    .ok_or(SemanticCompilerFailure::CounterOverflow)?;
            }
            Ok::<_, CheckStop>(candidates)
        })();
        self.restore_nominal_checkpoint(nominal_checkpoint)?;
        if !self.pending_nominals.borrow().is_empty() {
            self.pending_nominals.borrow_mut().clear();
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        let mut discovered = discovered?;
        discovered.sort_by(|left, right| {
            left.1
                .cmp(&right.1)
                .then_with(|| format!("{:?}", left.2).cmp(&format!("{:?}", right.2)))
        });
        discovered.dedup_by(|left, right| left.0 == right.0 && left.2 == right.2);

        let mut replayed = Vec::new();
        for (template_index, declaration, stable) in discovered {
            let substitution = self.reify_concrete_substitution(&stable)?;
            let already_present = self
                .functions_by_declaration
                .get(&declaration)
                .into_iter()
                .flatten()
                .any(|id| {
                    self.signatures
                        .get(id.0 as usize)
                        .is_some_and(|signature| signature.substitution == substitution)
                });
            if already_present {
                continue;
            }
            let id = super::super::model::FunctionId(
                u32::try_from(self.signatures.len())
                    .map_err(|_| SemanticCompilerFailure::CounterOverflow)?,
            );
            self.instantiate_function_signature(template_index, substitution)?;
            replayed.push(id);
        }
        Ok(replayed)
    }

    fn stabilize_concrete_substitution(
        &self,
        substitution: &GenericSubstitution,
        nominal_checkpoint: usize,
    ) -> Result<Option<StableGenericSubstitution>, CheckStop> {
        let mut visiting = HashSet::new();
        let mut bindings = Vec::with_capacity(substitution.bindings.len());
        for (declaration, argument) in &substitution.bindings {
            let stable = match argument {
                GenericArgument::Type(ty) => {
                    let Some(ty) =
                        self.stabilize_concrete_type(*ty, nominal_checkpoint, &mut visiting)?
                    else {
                        return Ok(None);
                    };
                    StableGenericArgument::Type(ty)
                }
                GenericArgument::Const(value) => {
                    let Some(value) = value.value() else {
                        return Ok(None);
                    };
                    StableGenericArgument::Const(CheckedConst::Value(value))
                }
                GenericArgument::Function(value) => {
                    if !value.is_concrete() {
                        return Ok(None);
                    }
                    StableGenericArgument::Function(*value)
                }
            };
            bindings.push((*declaration, stable));
        }
        Ok(Some(StableGenericSubstitution {
            bindings,
            regions: substitution.regions.clone(),
        }))
    }

    /// Preserves concrete types discovered by transient declaration checking
    /// without letting a scratch nominal identity reach the executable table.
    pub(super) fn retain_concrete_nominals_since(
        &mut self,
        checkpoint: usize,
    ) -> Result<(), CheckStop> {
        let mut concrete = Vec::new();
        for nominal in self.nominals.iter().skip(checkpoint) {
            if let Some(ty) = self.stabilize_concrete_type(
                CheckedType::Nominal(nominal.id),
                checkpoint,
                &mut HashSet::new(),
            )? {
                concrete.push(ty);
            }
        }
        self.restore_nominal_checkpoint(checkpoint)?;
        for ty in &concrete {
            self.reify_concrete_type(ty)?;
        }
        Ok(())
    }

    fn stabilize_concrete_type(
        &self,
        ty: CheckedType,
        nominal_checkpoint: usize,
        visiting: &mut HashSet<NominalId>,
    ) -> Result<Option<StableCheckedType>, CheckStop> {
        self.stabilize_type(ty, nominal_checkpoint, visiting, false)
    }

    fn stabilize_schema_type(
        &self,
        ty: CheckedType,
        nominal_checkpoint: usize,
        visiting: &mut HashSet<NominalId>,
    ) -> Result<StableCheckedType, CheckStop> {
        self.stabilize_type(ty, nominal_checkpoint, visiting, true)?
            .ok_or_else(|| SemanticCompilerFailure::InvalidResolution.into())
    }

    fn stabilize_type(
        &self,
        ty: CheckedType,
        nominal_checkpoint: usize,
        visiting: &mut HashSet<NominalId>,
        allow_symbolic: bool,
    ) -> Result<Option<StableCheckedType>, CheckStop> {
        let stable = match ty {
            CheckedType::Unit
            | CheckedType::Bool
            | CheckedType::Integer(_)
            | CheckedType::Float(_) => StableCheckedType::Scalar(ty),
            CheckedType::Generic(_) | CheckedType::GenericInt(_) | CheckedType::GenericFloat(_) => {
                if !allow_symbolic {
                    return Ok(None);
                }
                StableCheckedType::Scalar(ty)
            }
            CheckedType::Nominal(id) => {
                if !visiting.insert(id) {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                }
                let source = self
                    .source_nominal_instances
                    .get(id.0 as usize)
                    .cloned()
                    .flatten();
                let prelude = self.prelude_types.get(id.0 as usize).cloned().flatten();
                let kind = self
                    .nominals
                    .get(id.0 as usize)
                    .map(|nominal| nominal.kind.clone())
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                let stable = if let Some((template, substitution)) = source {
                    let Some(substitution) = self.stabilize_substitution_with_visiting(
                        &substitution,
                        nominal_checkpoint,
                        visiting,
                        allow_symbolic,
                    )?
                    else {
                        visiting.remove(&id);
                        return Ok(None);
                    };
                    StableCheckedType::SourceNominal {
                        template,
                        substitution,
                    }
                } else if let Some(prelude) = prelude {
                    let Some(prelude) = self.stabilize_prelude_type(
                        prelude,
                        nominal_checkpoint,
                        visiting,
                        allow_symbolic,
                    )?
                    else {
                        visiting.remove(&id);
                        return Ok(None);
                    };
                    StableCheckedType::Prelude(prelude)
                } else if let Some((results, _)) = self
                    .result_list_nominals
                    .iter()
                    .find(|(_, candidate)| **candidate == id)
                {
                    let mut stable = Vec::with_capacity(results.len());
                    for (name, ty) in results {
                        let Some(ty) =
                            self.stabilize_type(*ty, nominal_checkpoint, visiting, allow_symbolic)?
                        else {
                            visiting.remove(&id);
                            return Ok(None);
                        };
                        stable.push((name.clone(), ty));
                    }
                    StableCheckedType::ResultList(stable)
                } else {
                    match kind {
                        CheckedNominalKind::Box {
                            referent, region, ..
                        } => {
                            let Some(referent) = self.stabilize_type(
                                referent,
                                nominal_checkpoint,
                                visiting,
                                allow_symbolic,
                            )?
                            else {
                                visiting.remove(&id);
                                return Ok(None);
                            };
                            StableCheckedType::Boxed {
                                region,
                                referent: Box::new(referent),
                            }
                        }
                        CheckedNominalKind::Opaque => {
                            return Err(SemanticCompilerFailure::InvalidResolution.into());
                        }
                        CheckedNominalKind::Struct { .. } | CheckedNominalKind::Enum { .. } => {
                            return Err(SemanticCompilerFailure::InvalidResolution.into());
                        }
                    }
                };
                visiting.remove(&id);
                stable
            }
            CheckedType::Array { element, length } => {
                let Some(element) =
                    self.stabilize_element(element, nominal_checkpoint, visiting, allow_symbolic)?
                else {
                    return Ok(None);
                };
                if !allow_symbolic && !length.is_concrete() {
                    return Ok(None);
                }
                StableCheckedType::Array { element, length }
            }
            CheckedType::Buffer { element } => {
                let Some(element) =
                    self.stabilize_element(element, nominal_checkpoint, visiting, allow_symbolic)?
                else {
                    return Ok(None);
                };
                StableCheckedType::Buffer { element }
            }
            CheckedType::Window {
                shape,
                element,
                capacity,
            } => {
                let Some(element) =
                    self.stabilize_element(element, nominal_checkpoint, visiting, allow_symbolic)?
                else {
                    return Ok(None);
                };
                if !allow_symbolic && capacity.is_some_and(|capacity| !capacity.is_concrete()) {
                    return Ok(None);
                }
                StableCheckedType::Window {
                    shape,
                    element,
                    capacity,
                }
            }
        };
        Ok(Some(stable))
    }

    /// One type's spelling by module-qualified declaration names and its
    /// arguments, the identity a [MOD-8] proof receipt key names it by;
    /// symbolic parameters keep their written position. `None` for a type
    /// with no such spelling.
    pub(super) fn stable_type_spelling(&self, ty: CheckedType) -> Option<String> {
        let stable = self
            .stabilize_type(ty, 0, &mut HashSet::new(), true)
            .ok()??;
        let mut spelled = String::new();
        self.spell_stable_type(&stable, &mut spelled).ok()?;
        Some(spelled)
    }

    /// The digest part of an instance symbol: the first eight bytes of the
    /// SHA-256 of its arguments' canonical spelling, or `None` when an
    /// argument has no concrete spelling.
    fn stable_instance_suffix(&self, substitution: &GenericSubstitution) -> Option<String> {
        use core::fmt::Write as _;
        let stable = self
            .stabilize_substitution_with_visiting(substitution, 0, &mut HashSet::new(), false)
            .ok()
            .flatten()?;
        let mut spelling = String::new();
        self.spell_stable_substitution(&stable, &mut spelling)
            .ok()?;
        let digest = crate::spec::sha256::digest(spelling.as_bytes());
        let mut suffix = String::with_capacity(16);
        for byte in &digest[..8] {
            let _ = write!(suffix, "{byte:02x}");
        }
        Some(suffix)
    }

    /// Spells a concrete substitution's arguments in binding order, each type
    /// by module-qualified declaration names and each function by its
    /// declaration and its own arguments.
    fn spell_stable_substitution(
        &self,
        substitution: &StableGenericSubstitution,
        out: &mut String,
    ) -> Result<(), CheckStop> {
        if substitution.bindings.is_empty() {
            return Ok(());
        }
        out.push('<');
        for (index, (_, argument)) in substitution.bindings.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            match argument {
                StableGenericArgument::Type(ty) => self.spell_stable_type(ty, out)?,
                StableGenericArgument::Const(value) => {
                    out.push_str(&self.checked_const_name(*value)?);
                }
                StableGenericArgument::Function(super::behavior::FunctionArgument::Source {
                    reference,
                    ..
                }) => {
                    let reference = self.function_reference(*reference)?;
                    let spelling = self
                        .resolved
                        .declaration(reference.declaration)
                        .map(|declaration| declaration.spelling().to_owned())
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                    out.push_str("fn ");
                    out.push_str(&self.module_symbol_base(reference.declaration, &spelling));
                    self.spell_stable_substitution(&reference.substitution, out)?;
                }
                StableGenericArgument::Function(super::behavior::FunctionArgument::Parameter(
                    _,
                )) => {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                }
            }
        }
        out.push('>');
        Ok(())
    }

    fn spell_stable_type(&self, ty: &StableCheckedType, out: &mut String) -> Result<(), CheckStop> {
        match ty {
            StableCheckedType::Scalar(ty) => out.push_str(&self.checked_type_name(*ty)?),
            StableCheckedType::SourceNominal {
                template,
                substitution,
            } => {
                let template = self
                    .nominal_templates
                    .get(*template)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                out.push_str(&self.module_symbol_base(template.declaration, &template.name));
                self.spell_stable_substitution(substitution, out)?;
            }
            StableCheckedType::Prelude(prelude) => match prelude {
                StablePreludeType::Option(value) => {
                    out.push_str("Option<");
                    self.spell_stable_type(value, out)?;
                    out.push('>');
                }
                StablePreludeType::Result(value, error) => {
                    out.push_str("Result<");
                    self.spell_stable_type(value, out)?;
                    out.push(',');
                    self.spell_stable_type(error, out)?;
                    out.push('>');
                }
                StablePreludeType::Overflow => out.push_str("Overflow"),
                StablePreludeType::DivError => out.push_str("DivError"),
                StablePreludeType::NarrowError => out.push_str("NarrowError"),
            },
            StableCheckedType::ResultList(results) => {
                out.push('(');
                for (index, (name, ty)) in results.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    out.push_str(name);
                    out.push(':');
                    self.spell_stable_type(ty, out)?;
                }
                out.push(')');
            }
            StableCheckedType::Boxed { referent, .. } => {
                out.push_str("Box<");
                self.spell_stable_type(referent, out)?;
                out.push('>');
            }
            StableCheckedType::Array { element, length } => {
                out.push_str("Array<");
                self.spell_stable_type(&element.0, out)?;
                out.push(',');
                out.push_str(&self.checked_const_name(*length)?);
                out.push('>');
            }
            StableCheckedType::Buffer { element } => {
                out.push_str("Array<");
                self.spell_stable_type(&element.0, out)?;
                out.push('>');
            }
            StableCheckedType::Window {
                shape,
                element,
                capacity,
            } => {
                out.push_str(shape.spelling());
                out.push('<');
                self.spell_stable_type(&element.0, out)?;
                if let Some(capacity) = capacity {
                    out.push(',');
                    out.push_str(&self.checked_const_name(*capacity)?);
                }
                out.push('>');
            }
        }
        Ok(())
    }

    pub(super) fn stabilize_substitution_with_visiting(
        &self,
        substitution: &GenericSubstitution,
        nominal_checkpoint: usize,
        visiting: &mut HashSet<NominalId>,
        allow_symbolic: bool,
    ) -> Result<Option<StableGenericSubstitution>, CheckStop> {
        let mut bindings = Vec::with_capacity(substitution.bindings.len());
        for (declaration, argument) in &substitution.bindings {
            let stable = match argument {
                GenericArgument::Type(ty) => {
                    let Some(ty) =
                        self.stabilize_type(*ty, nominal_checkpoint, visiting, allow_symbolic)?
                    else {
                        return Ok(None);
                    };
                    StableGenericArgument::Type(ty)
                }
                GenericArgument::Const(value) => {
                    if !allow_symbolic && !value.is_concrete() {
                        return Ok(None);
                    }
                    StableGenericArgument::Const(*value)
                }
                GenericArgument::Function(value) => {
                    if !allow_symbolic && !value.is_concrete() {
                        return Ok(None);
                    }
                    StableGenericArgument::Function(*value)
                }
            };
            bindings.push((*declaration, stable));
        }
        Ok(Some(StableGenericSubstitution {
            bindings,
            regions: substitution.regions.clone(),
        }))
    }

    fn stabilize_element(
        &self,
        element: CheckedElement,
        nominal_checkpoint: usize,
        visiting: &mut HashSet<NominalId>,
        allow_symbolic: bool,
    ) -> Result<Option<StableElement>, CheckStop> {
        Ok(self
            .stabilize_type(
                self.element_type(element)?,
                nominal_checkpoint,
                visiting,
                allow_symbolic,
            )?
            .map(|ty| StableElement(Box::new(ty))))
    }

    fn stabilize_prelude_type(
        &self,
        ty: PreludeType,
        nominal_checkpoint: usize,
        visiting: &mut HashSet<NominalId>,
        allow_symbolic: bool,
    ) -> Result<Option<StablePreludeType>, CheckStop> {
        Ok(match ty {
            PreludeType::Option(value) => self
                .stabilize_type(value, nominal_checkpoint, visiting, allow_symbolic)?
                .map(|value| StablePreludeType::Option(Box::new(value))),
            PreludeType::Result(ok, error) => {
                let Some(ok) =
                    self.stabilize_type(ok, nominal_checkpoint, visiting, allow_symbolic)?
                else {
                    return Ok(None);
                };
                let Some(error) =
                    self.stabilize_type(error, nominal_checkpoint, visiting, allow_symbolic)?
                else {
                    return Ok(None);
                };
                Some(StablePreludeType::Result(Box::new(ok), Box::new(error)))
            }
            PreludeType::Overflow => Some(StablePreludeType::Overflow),
            PreludeType::DivError => Some(StablePreludeType::DivError),
            PreludeType::NarrowError => Some(StablePreludeType::NarrowError),
        })
    }

    pub(super) fn reify_concrete_substitution(
        &mut self,
        substitution: &StableGenericSubstitution,
    ) -> Result<GenericSubstitution, CheckStop> {
        let mut bindings = Vec::with_capacity(substitution.bindings.len());
        for (declaration, argument) in &substitution.bindings {
            let argument = match argument {
                StableGenericArgument::Type(ty) => {
                    GenericArgument::Type(self.reify_concrete_type(ty)?)
                }
                StableGenericArgument::Const(value) => GenericArgument::Const(*value),
                StableGenericArgument::Function(value) => GenericArgument::Function(*value),
            };
            bindings.push((*declaration, argument));
        }
        GenericSubstitution::from_bindings(bindings)
            .map(|reified| reified.with_regions(substitution.regions.clone()))
            .map_err(CheckStop::Compiler)
    }

    fn reify_concrete_type(&mut self, ty: &StableCheckedType) -> Result<CheckedType, CheckStop> {
        Ok(match ty {
            StableCheckedType::Scalar(ty) => *ty,
            StableCheckedType::SourceNominal {
                template,
                substitution,
            } => {
                let substitution = self.reify_concrete_substitution(substitution)?;
                CheckedType::Nominal(self.ensure_source_nominal_instance(*template, substitution)?)
            }
            StableCheckedType::Prelude(ty) => {
                let ty = match ty {
                    StablePreludeType::Option(value) => {
                        PreludeType::Option(self.reify_concrete_type(value)?)
                    }
                    StablePreludeType::Result(ok, error) => PreludeType::Result(
                        self.reify_concrete_type(ok)?,
                        self.reify_concrete_type(error)?,
                    ),
                    StablePreludeType::Overflow => PreludeType::Overflow,
                    StablePreludeType::DivError => PreludeType::DivError,
                    StablePreludeType::NarrowError => PreludeType::NarrowError,
                };
                CheckedType::Nominal(self.intern_prelude_nominal(ty)?)
            }
            StableCheckedType::ResultList(results) => {
                let mut reified = Vec::with_capacity(results.len());
                for (name, ty) in results {
                    reified.push((name.clone(), self.reify_concrete_type(ty)?));
                }
                CheckedType::Nominal(self.intern_result_list_nominal(&reified)?)
            }
            StableCheckedType::Boxed { region, referent } => {
                let referent = self.reify_concrete_type(referent)?;
                // [TYPE-9] a `Box` carries no brand and there is one heap
                // [STOR-8], so one referent is one cell nominal.
                let _ = region;
                CheckedType::Nominal(self.intern_box_nominal(referent)?)
            }
            StableCheckedType::Array { element, length } => CheckedType::Array {
                element: self.reify_element(element)?,
                length: *length,
            },
            StableCheckedType::Buffer { element } => CheckedType::Buffer {
                element: self.reify_element(element)?,
            },
            StableCheckedType::Window {
                shape,
                element,
                capacity,
            } => CheckedType::Window {
                shape: *shape,
                element: self.reify_element(element)?,
                capacity: *capacity,
            },
        })
    }

    fn reify_element(&mut self, element: &StableElement) -> Result<CheckedElement, CheckStop> {
        let ty = self.reify_concrete_type(&element.0)?;
        self.intern_element(ty)
    }

    fn stabilize_generic_requirement(
        &self,
        declaration: DeclarationId,
        requirement: &CheckedRequirement,
        nominal_checkpoint: usize,
    ) -> Result<PendingGenericRequirement, CheckStop> {
        let mut nominals = Vec::new();
        self.collect_goal_nominals(&requirement.template.root, &mut nominals)?;
        nominals.sort_by_key(|id| id.0);
        nominals.dedup();

        let mut replacements = Vec::new();
        for nominal in nominals {
            if (nominal.0 as usize) < nominal_checkpoint {
                continue;
            }
            let stable = self.stabilize_schema_type(
                CheckedType::Nominal(nominal),
                nominal_checkpoint,
                &mut HashSet::new(),
            )?;
            replacements.push((nominal, stable));
        }
        Ok(PendingGenericRequirement {
            declaration,
            requirement: requirement.clone(),
            nominal_checkpoint,
            replacements,
        })
    }

    /// Re-interns metadata-only symbolic nominals after the executable prefix
    /// has already been measured. No scratch `NominalId` crosses the schema
    /// checkpoint, and lowering continues to see one contiguous concrete
    /// prefix.
    pub(super) fn materialize_generic_requirements(&mut self) -> Result<(), CheckStop> {
        if !self.generic_requirements.is_empty() {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        let pending = std::mem::take(&mut self.pending_generic_requirements);
        for mut pending in pending {
            let mut replacements = HashMap::new();
            for (old, stable) in &pending.replacements {
                let CheckedType::Nominal(new) = self.reify_concrete_type(stable)? else {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                };
                if replacements.insert(*old, new).is_some() {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                }
            }
            self.rewrite_goal_nominals(
                &mut pending.requirement.template.root,
                pending.nominal_checkpoint,
                &replacements,
            )?;
            self.generic_requirements.push(CheckedGenericRequirement {
                declaration: pending.declaration,
                requirement: pending.requirement,
            });
        }
        Ok(())
    }

    pub(super) fn symbolic_generic_substitution(
        &self,
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
        &self,
        parameters: &[GenericParameter],
        region_parameters: &[DeclarationId],
    ) -> Result<GenericSubstitution, CheckStop> {
        Ok(self
            .symbolic_generic_substitution(parameters)?
            .with_regions(
                region_parameters
                    .iter()
                    .map(|region| (*region, *region))
                    .collect(),
            ))
    }

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

    pub(super) fn parse_generic_parameters(
        &self,
        declaration: NodeId,
    ) -> Result<Vec<GenericParameter>, CheckStop> {
        let Some(generics) = self
            .tree
            .first_child_with(declaration, Production::Generics)?
        else {
            return Ok(Vec::new());
        };
        let mut parameters = Vec::new();
        for node in self.tree.children_with(generics, Production::Gparam)? {
            if let Some(signature) = self.tree.first_child_with(node, Production::FnSig)? {
                let declaration = self
                    .declaration_at(signature, DeclarationRole::FunctionParameter)?
                    .id();
                parameters.push(GenericParameter::Function {
                    key: GenericParameterKey::Source(declaration),
                    signature,
                });
                continue;
            }
            if let Some(application) = self.tree.group_application(node)? {
                parameters.extend(self.expand_formal_parameters(application)?);
                continue;
            }
            if self.has_fixed(node, FixedTerminal::Const)? {
                let declaration = self
                    .declaration_at(node, DeclarationRole::ConstGeneric)?
                    .id();
                let ty = self
                    .tree
                    .first_child_with(node, Production::Type)?
                    .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
                let Some(ty) = self.integer_type(ty)? else {
                    return self.issue_node(
                        SemanticRule::Const1,
                        ty,
                        SemanticIssueKind::InvalidConstValue,
                    );
                };
                parameters.push(GenericParameter::Const { declaration, ty });
                continue;
            }
            let declaration = self
                .declaration_at(node, DeclarationRole::GenericType)?
                .id();
            // [GRAM-2, PROV-6] the bound is optional and never inferred: a
            // `capability_bound` atom, a numeric marker TYPEID, or nothing,
            // and an absent bound grants the body no capability, which is
            // the linear class read at the parameter.
            let bound = match self
                .resolved
                .lexical_uses_at(node)
                .find(|usage| usage.role() == LexicalUseRole::GenericBound)
                .map(|usage| (usage.target(), usage.origin().coordinate()))
            {
                None => GenericBound::Class(
                    self.written_linearity_bound(node)?
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
                    return self.issue_at(
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

    pub(super) fn call_generic_substitution(
        &self,
        node: NodeId,
        template: &FunctionTemplate,
        caller: &GenericSubstitution,
    ) -> Result<GenericSubstitution, CheckStop> {
        let leading_regions = match self.tree.argument_list(node)? {
            Some(targs) => {
                self.user_call_region_prefix(&self.tree.children_with(targs, Production::Targ)?)?
            }
            None => 0,
        };
        // [FORM-8] user calls write caller-chosen regions before type/const
        // arguments. [DIAG-1] their generic argument list is FN-2's.
        self.generic_substitution(
            node,
            &template.generic_parameters,
            caller,
            SemanticRule::Fn2,
            leading_regions,
        )
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
        &self,
        node: NodeId,
        parameters: &[GenericParameter],
        region_parameters: &[DeclarationId],
        caller: &GenericSubstitution,
    ) -> Result<GenericSubstitution, CheckStop> {
        self.nominal_generic_substitution_with(node, parameters, region_parameters, &[], caller)
    }

    /// The same list where some region parameters are already fixed [FORM-8].
    ///
    /// A `type` position determines none of them and writes every one; a
    /// `construct`'s field operands determine the ones their declared types
    /// name, and the position writes exactly the rest, still as the leading
    /// members of the same `targs` list. The two are one function because the
    /// difference between them is exactly which formals arrive already bound.
    pub(super) fn nominal_generic_substitution_with(
        &self,
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
        let written = self.nominal_region_arguments(node, &written_parameters, caller)?;
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
        &self,
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
        &self,
        node: NodeId,
        parameters: &[GenericParameter],
        caller: &GenericSubstitution,
        argument_rule: SemanticRule,
        leading_regions: usize,
    ) -> Result<GenericSubstitution, CheckStop> {
        let written = match self.tree.argument_list(node)? {
            Some(list) => self.tree.children_with(list, Production::Targ)?,
            None if parameters.is_empty() && leading_regions == 0 => {
                return Ok(GenericSubstitution::default());
            }
            None => {
                return self.issue_node(
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
            return self.behavior_mismatch(
                argument_rule,
                node,
                "write every undetermined region before the generic arguments",
            );
        }
        let arguments = self.expand_written_arguments(&written[leading_regions..], caller)?;
        if arguments.len() != parameters.len() {
            return self.issue_node(
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
                        let Some(ty) = self.tree.first_child_with(source, Production::Type)? else {
                            return self.behavior_mismatch(
                                argument_rule,
                                source,
                                "a type argument occupies this parameter position",
                            );
                        };
                        GenericArgument::Type(self.parse_type_with(ty, caller)?)
                    }
                    GenericParameter::Const { .. } => {
                        let Some(value) = self.tree.first_child_with(source, Production::Const)?
                        else {
                            return self.behavior_mismatch(
                                argument_rule,
                                source,
                                "a const argument occupies this parameter position",
                            );
                        };
                        GenericArgument::Const(self.parse_const_expression_with(value, caller)?)
                    }
                    GenericParameter::Function { .. } => {
                        GenericArgument::Function(self.parse_function_argument(source, caller)?)
                    }
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
                        return self.issue_node(
                            SemanticRule::Fn3,
                            source,
                            SemanticIssueKind::type_mismatch(required, self.checked_type_name(ty)?),
                        );
                    }
                    if let GenericBound::Class(required) = bound {
                        let spelling = self.declaration_spelling(declaration)?;
                        self.check_linearity_bound(&spelling, required, ty, source)?;
                    }
                }
                (GenericParameter::Const { .. }, GenericArgument::Const(_))
                | (GenericParameter::Function { .. }, GenericArgument::Function(_)) => {}
                _ => {
                    return self.behavior_mismatch(
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
        self.record_behavior_binding_sites(&substitution, &binding_sites)?;
        Ok(substitution)
    }
}

impl Checker<'_> {
    fn collect_goal_nominals(
        &self,
        expression: &GoalExpression,
        output: &mut Vec<NominalId>,
    ) -> Result<(), CheckStop> {
        match expression {
            GoalExpression::Datum(datum) => match datum {
                GoalDatum::Parameter { ty, .. }
                | GoalDatum::NamedConst { ty, .. }
                | GoalDatum::Place { ty, .. } => self.collect_type_nominals(*ty, output)?,
                GoalDatum::EvaluatedValue {
                    captured_type, ty, ..
                } => {
                    self.collect_type_nominals(*captured_type, output)?;
                    self.collect_type_nominals(*ty, output)?;
                }
                GoalDatum::Literal(value) => self.collect_value_nominals(value, output)?,
            },
            GoalExpression::Operation {
                row,
                type_arguments,
                result,
                arguments,
                ..
            } => {
                self.collect_operation_nominals(*row, output)?;
                for ty in type_arguments {
                    self.collect_type_nominals(*ty, output)?;
                }
                self.collect_type_nominals(*result, output)?;
                for argument in arguments {
                    self.collect_goal_nominals(argument, output)?;
                }
            }
        };
        Ok(())
    }

    fn collect_operation_nominals(
        &self,
        operation: GoalOperation,
        output: &mut Vec<NominalId>,
    ) -> Result<(), CheckStop> {
        match operation {
            GoalOperation::Integer { operand_type, .. }
            | GoalOperation::Float { operand_type, .. }
            | GoalOperation::EnumEquality { operand_type, .. }
            | GoalOperation::BufferFits {
                element: operand_type,
                ..
            } => self.collect_type_nominals(operand_type, output)?,
            GoalOperation::BufferMeasure { element, .. }
            | GoalOperation::BufferIndex { element } => {
                self.collect_element_nominals(element, output)?;
            }
            GoalOperation::ArrayMeasure { element, .. }
            | GoalOperation::ArrayIndex { element, .. }
            | GoalOperation::RunIndex { element, .. } => {
                self.collect_element_nominals(element, output)?
            }
            GoalOperation::ContainerMeasure { element, .. } => {
                if let Some(element) = element {
                    self.collect_element_nominals(element, output)?;
                }
            }
            GoalOperation::NumericConversion { .. }
            | GoalOperation::Reinterpret { .. }
            | GoalOperation::Boolean(_) => {}
        };
        Ok(())
    }

    pub(super) fn collect_type_nominals(
        &self,
        ty: CheckedType,
        output: &mut Vec<NominalId>,
    ) -> Result<(), CheckStop> {
        match ty {
            CheckedType::Nominal(id) => output.push(id),
            CheckedType::Buffer { element } => self.collect_element_nominals(element, output)?,
            CheckedType::Array { element, .. } | CheckedType::Window { element, .. } => {
                self.collect_element_nominals(element, output)?;
            }
            CheckedType::Unit
            | CheckedType::Bool
            | CheckedType::Integer(_)
            | CheckedType::Float(_)
            | CheckedType::Generic(_)
            | CheckedType::GenericInt(_)
            | CheckedType::GenericFloat(_) => {}
        };
        Ok(())
    }

    fn collect_element_nominals(
        &self,
        element: CheckedElement,
        output: &mut Vec<NominalId>,
    ) -> Result<(), CheckStop> {
        self.collect_type_nominals(self.element_type(element)?, output)
    }

    fn collect_value_nominals(
        &self,
        value: &CheckedValue,
        output: &mut Vec<NominalId>,
    ) -> Result<(), CheckStop> {
        match value {
            CheckedValue::NumericIdentity { ty, .. } => self.collect_type_nominals(*ty, output)?,
            CheckedValue::Array { ty, elements } => {
                self.collect_type_nominals(*ty, output)?;
                for element in elements {
                    self.collect_value_nominals(element, output)?;
                }
            }
            CheckedValue::Struct { ty, fields } => {
                self.collect_type_nominals(*ty, output)?;
                for field in fields {
                    self.collect_value_nominals(field, output)?;
                }
            }
            CheckedValue::ConstGeneric { .. }
            | CheckedValue::Unit
            | CheckedValue::Bool(_)
            | CheckedValue::Integer { .. }
            | CheckedValue::Float { .. } => {}
        };
        Ok(())
    }

    fn rewrite_goal_nominals(
        &self,
        expression: &mut GoalExpression,
        checkpoint: usize,
        replacements: &HashMap<NominalId, NominalId>,
    ) -> Result<(), CheckStop> {
        match expression {
            GoalExpression::Datum(datum) => match datum {
                GoalDatum::Parameter { ty, .. }
                | GoalDatum::NamedConst { ty, .. }
                | GoalDatum::Place { ty, .. } => {
                    self.rewrite_type_nominals(ty, checkpoint, replacements)?
                }
                GoalDatum::EvaluatedValue {
                    captured_type, ty, ..
                } => {
                    self.rewrite_type_nominals(captured_type, checkpoint, replacements)?;
                    self.rewrite_type_nominals(ty, checkpoint, replacements)?;
                }
                GoalDatum::Literal(value) => {
                    self.rewrite_value_nominals(value, checkpoint, replacements)?
                }
            },
            GoalExpression::Operation {
                row,
                type_arguments,
                result,
                arguments,
                ..
            } => {
                self.rewrite_operation_nominals(row, checkpoint, replacements)?;
                for ty in type_arguments {
                    self.rewrite_type_nominals(ty, checkpoint, replacements)?;
                }
                self.rewrite_type_nominals(result, checkpoint, replacements)?;
                for argument in arguments {
                    self.rewrite_goal_nominals(argument, checkpoint, replacements)?;
                }
            }
        }
        Ok(())
    }

    fn rewrite_operation_nominals(
        &self,
        operation: &mut GoalOperation,
        checkpoint: usize,
        replacements: &HashMap<NominalId, NominalId>,
    ) -> Result<(), CheckStop> {
        match operation {
            GoalOperation::Integer { operand_type, .. }
            | GoalOperation::Float { operand_type, .. }
            | GoalOperation::EnumEquality { operand_type, .. }
            | GoalOperation::BufferFits {
                element: operand_type,
                ..
            } => self.rewrite_type_nominals(operand_type, checkpoint, replacements)?,
            GoalOperation::BufferMeasure { element, .. }
            | GoalOperation::BufferIndex { element } => {
                self.rewrite_element_nominals(element, checkpoint, replacements)?;
            }
            GoalOperation::ArrayMeasure { element, .. }
            | GoalOperation::ArrayIndex { element, .. }
            | GoalOperation::RunIndex { element, .. } => {
                self.rewrite_element_nominals(element, checkpoint, replacements)?;
            }
            GoalOperation::ContainerMeasure { element, .. } => {
                if let Some(element) = element {
                    self.rewrite_element_nominals(element, checkpoint, replacements)?;
                }
            }
            GoalOperation::NumericConversion { .. }
            | GoalOperation::Reinterpret { .. }
            | GoalOperation::Boolean(_) => {}
        }
        Ok(())
    }

    fn rewrite_type_nominals(
        &self,
        ty: &mut CheckedType,
        checkpoint: usize,
        replacements: &HashMap<NominalId, NominalId>,
    ) -> Result<(), CheckStop> {
        match ty {
            CheckedType::Nominal(id) if (id.0 as usize) >= checkpoint => {
                *id = *replacements
                    .get(id)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            }
            CheckedType::Buffer { element } => {
                self.rewrite_element_nominals(element, checkpoint, replacements)?;
            }
            CheckedType::Array { element, .. } | CheckedType::Window { element, .. } => {
                self.rewrite_element_nominals(element, checkpoint, replacements)?;
            }
            CheckedType::Unit
            | CheckedType::Bool
            | CheckedType::Integer(_)
            | CheckedType::Float(_)
            | CheckedType::Generic(_)
            | CheckedType::GenericInt(_)
            | CheckedType::GenericFloat(_)
            | CheckedType::Nominal(_) => {}
        }
        Ok(())
    }

    fn rewrite_element_nominals(
        &self,
        element: &mut CheckedElement,
        checkpoint: usize,
        replacements: &HashMap<NominalId, NominalId>,
    ) -> Result<(), CheckStop> {
        let mut ty = self.element_type(*element)?;
        self.rewrite_type_nominals(&mut ty, checkpoint, replacements)?;
        *element = self.intern_element(ty)?;
        Ok(())
    }

    fn rewrite_value_nominals(
        &self,
        value: &mut CheckedValue,
        checkpoint: usize,
        replacements: &HashMap<NominalId, NominalId>,
    ) -> Result<(), CheckStop> {
        match value {
            CheckedValue::NumericIdentity { ty, .. } => {
                self.rewrite_type_nominals(ty, checkpoint, replacements)?;
            }
            CheckedValue::Array { ty, elements } => {
                self.rewrite_type_nominals(ty, checkpoint, replacements)?;
                for element in elements {
                    self.rewrite_value_nominals(element, checkpoint, replacements)?;
                }
            }
            CheckedValue::Struct { ty, fields } => {
                self.rewrite_type_nominals(ty, checkpoint, replacements)?;
                for field in fields {
                    self.rewrite_value_nominals(field, checkpoint, replacements)?;
                }
            }
            CheckedValue::ConstGeneric { .. }
            | CheckedValue::Unit
            | CheckedValue::Bool(_)
            | CheckedValue::Integer { .. }
            | CheckedValue::Float { .. } => {}
        }
        Ok(())
    }
}
