use std::collections::HashSet;

use crate::syntax::NodeId;
use crate::syntax::terminal::TerminalPredicate;
use crate::{
    BuiltinPreludeId, DeclarationClass, DeclarationRole, DependentDeclarationRole, LexicalUseRole,
    Production, ResolvedTarget, SemanticCompilerFailure, SemanticIssueKind, SemanticRule,
};

use super::super::model::{
    CheckedConstructor, CheckedElement, CheckedField, CheckedNominal, CheckedNominalKind,
    CheckedNumericType, CheckedType, CheckedVariant, NominalId,
};
use super::generics::GenericSubstitution;
use super::{
    CheckStop, Checker, ConstructorTemplate, NominalInstance, NominalTemplate, PreludeType,
};

impl<'unit> Checker<'unit> {
    /// [TYPE-2] whether this `struct_decl` carries the `opaque` modifier.
    ///
    /// The modifier is a written one, and [GRAM-2] admits it on a source
    /// `struct_decl` exactly as on the prelude's own opaque structs, so the
    /// written token decides it first. The prelude-file test stays beside it
    /// because the prelude's opaque declarations are read through a record
    /// reader that fixes the modifier by its phase rather than by a token.
    fn is_opaque_declaration(&self, node: NodeId) -> Result<bool, CheckStop> {
        if self
            .tree
            .direct_token_with(node, TerminalPredicate::Fixed(crate::FixedTerminal::Opaque))?
            .is_some()
        {
            return Ok(true);
        }
        let source = self.tree.coordinate(node)?.source();
        Ok(self
            .resolved
            .syntax()
            .finalized
            .parsed
            .classified
            .source_bundle()
            .file(source)
            .is_some_and(|file| file.prelude() == Some(crate::source::PreludeSource::Opaque)))
    }

    pub(super) fn declare_nominals(&mut self, items: &[NodeId]) -> Result<(), CheckStop> {
        let nodes = items
            .iter()
            .copied()
            .filter(|node| {
                self.tree.production(*node).is_ok_and(|production| {
                    matches!(production, Production::StructDecl | Production::EnumDecl)
                })
            })
            .collect::<Vec<_>>();
        for node in nodes {
            self.declare_nominal_template(node)?;
        }
        for index in 0..self.nominal_templates.len() {
            if self.nominal_templates[index].generic_parameters.is_empty()
                && self.nominal_templates[index].region_parameters.is_empty()
            {
                self.declare_source_nominal_instance(index, GenericSubstitution::default())?;
            }
        }
        Ok(())
    }

    /// Scratch inventory for selector signatures. Invalid source templates
    /// are unavailable only to signatures that name them; unrelated nominal
    /// declarations cannot suppress an independently decidable FN-9 verdict.
    pub(super) fn declare_nominals_for_postconditions(
        &mut self,
        items: &[NodeId],
    ) -> Result<(), CheckStop> {
        let nodes = items
            .iter()
            .copied()
            .filter(|node| {
                self.tree.production(*node).is_ok_and(|production| {
                    matches!(production, Production::StructDecl | Production::EnumDecl)
                })
            })
            .collect::<Vec<_>>();
        for node in nodes {
            match self.declare_nominal_template(node) {
                Ok(()) => {}
                Err(CheckStop::Issue(_) | CheckStop::Unsupported(_)) => {
                    let role = match self.tree.production(node)? {
                        Production::StructDecl => DeclarationRole::Struct,
                        Production::EnumDecl => DeclarationRole::Enum,
                        _ => return Err(SemanticCompilerFailure::InvalidCanonicalTree.into()),
                    };
                    self.mark_postcondition_unavailable(self.declaration_at(node, role)?.id());
                }
                Err(stop) => return Err(stop),
            }
        }
        // Source instances are created and completed lazily by the exact
        // signature/type helpers. This is the dependency-local equivalent of
        // `complete_nominals`; PRE-1 instances remain ordinary shared setup.
        self.register_prelude_nominals()?;

        // A generic nominal declaration is a usable FN-2 signature premise
        // only after its ordinary symbolic template judgment succeeds. Keep
        // that judgment dependency-local: a bad template is unavailable to
        // headers and call arguments that name it, while an unrelated
        // selector remains independently decidable.
        for template_index in 0..self.nominal_templates.len() {
            let template = self
                .nominal_templates
                .get(template_index)
                .cloned()
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            if (template.generic_parameters.is_empty() && template.region_parameters.is_empty())
                || self.postcondition_declaration_unavailable(template.declaration)
            {
                continue;
            }
            let checkpoint = self.nominal_checkpoint();
            let result = (|| {
                let substitution = self.symbolic_nominal_substitution(
                    &template.generic_parameters,
                    &template.region_parameters,
                )?;
                self.ensure_source_nominal_instance(template_index, substitution)?;
                self.reject_recursive_nominal_layouts()
            })();
            self.restore_nominal_checkpoint(checkpoint)?;
            match result {
                Ok(()) => {}
                Err(
                    CheckStop::Issue(_)
                    | CheckStop::Unsupported(_)
                    | CheckStop::PostconditionPrerequisiteUnavailable,
                ) => self.mark_postcondition_unavailable(template.declaration),
                Err(stop) => return Err(stop),
            }
        }
        Ok(())
    }

    fn declare_nominal_template(&mut self, node: NodeId) -> Result<(), CheckStop> {
        let role = match self.tree.production(node)? {
            Production::StructDecl => DeclarationRole::Struct,
            Production::EnumDecl => DeclarationRole::Enum,
            _ => return Err(SemanticCompilerFailure::InvalidCanonicalTree.into()),
        };
        let declaration = self.declaration_at(node, role)?;
        let declaration_id = declaration.id();
        // Parse every source-bearing premise before publishing any table
        // entry, so a tolerant scratch failure is atomic.
        let generic_parameters = self.parse_generic_parameters(node)?;
        let variants = if role == DeclarationRole::Enum {
            self.tree.children_with(node, Production::Variant)?
        } else {
            Vec::new()
        };
        let linear = self.declaration_is_linear(node)?;
        let nocopy = self.declaration_is_nocopy(node)?;
        // [GRAM-2] no nominal declares a region parameter in v0.60.
        let region_parameters = Vec::new();
        let template = NominalTemplate {
            declaration: declaration_id,
            node,
            name: declaration.spelling().to_owned(),
            role,
            generic_parameters,
            region_parameters,
            linear,
            nocopy,
            constructors: Vec::new(),
        };
        let template_index = self.nominal_templates.len();
        if self
            .nominal_templates_by_declaration
            .insert(declaration_id, template_index)
            .is_some()
        {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        if role == DeclarationRole::Struct
            && !self.is_opaque_declaration(node)?
            && self
                .constructor_templates_by_declaration
                .insert(
                    declaration_id,
                    ConstructorTemplate::Struct {
                        template: template_index,
                    },
                )
                .is_some()
        {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        for (variant, variant_node) in variants.into_iter().enumerate() {
            let declaration = self.declaration_at(variant_node, DeclarationRole::Variant)?;
            let variant =
                u32::try_from(variant).map_err(|_| SemanticCompilerFailure::CounterOverflow)?;
            if self
                .constructor_templates_by_declaration
                .insert(
                    declaration.id(),
                    ConstructorTemplate::Enum {
                        template: template_index,
                        variant,
                    },
                )
                .is_some()
            {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            }
        }
        self.nominal_templates.push(template);
        Ok(())
    }

    pub(super) fn complete_nominals(&mut self) -> Result<(), CheckStop> {
        self.register_prelude_nominals()?;
        self.complete_pending_source_nominals()?;
        self.reject_recursive_nominal_layouts()?;
        self.validate_nominal_templates()
    }

    pub(super) fn ensure_nominals_in_node(
        &mut self,
        node: NodeId,
        substitution: &GenericSubstitution,
    ) -> Result<(), CheckStop> {
        for ty in self.nominal_type_descendants(node)? {
            self.ensure_nominal_type_head(ty, substitution)?;
        }
        for construct in self.tree.constructor_descendants(node)? {
            self.ensure_source_constructor_instance(construct, substitution)?;
        }
        self.ensure_implicit_prelude_nominals(node, substitution, false)?;
        self.reject_recursive_nominal_layouts()
    }

    /// Performs the ordinary nominal pre-scan for one function without
    /// entering its FN-9 clause. Ensures entries use private provisional
    /// links and must first pass the clause subset judgment; they are consumed
    /// only by the postcondition checker after that admission.
    pub(super) fn ensure_nominals_in_function(
        &mut self,
        function: NodeId,
        substitution: &GenericSubstitution,
    ) -> Result<(), CheckStop> {
        if self
            .tree
            .descendants_with(function, Production::EnsuresClause)?
            .is_empty()
        {
            self.ensure_nominals_in_node(function, substitution)?;
            return self.ensure_result_list_nominal(function, substitution);
        }
        // Preserve the exact ordinary category order across the retained
        // subtree: every type, then every constructor, then every implicit
        // PRE-1 instance, followed by one recursive-layout judgment.
        for ty in self.nominal_type_descendants(function)? {
            if self.node_is_inside_postcondition(ty)? {
                continue;
            }
            self.ensure_nominal_type_head(ty, substitution)?;
        }
        for construct in self.tree.constructor_descendants(function)? {
            if self.node_is_inside_postcondition(construct)? {
                continue;
            }
            self.ensure_source_constructor_instance(construct, substitution)?;
        }
        self.ensure_implicit_prelude_nominals(function, substitution, true)?;
        self.ensure_result_list_nominal(function, substitution)?;
        self.reject_recursive_nominal_layouts()
    }

    /// Interns the compiler-owned result-list nominal of a `fn_decl` that
    /// writes an ordered result list [GRAM-2, CALL-4].
    ///
    /// It is the callable's result type, so every pre-scan that precedes
    /// `build_function_signature` runs this one walk; a single-result
    /// declaration has no list and nothing is interned.
    pub(super) fn ensure_result_list_nominal(
        &mut self,
        function: NodeId,
        substitution: &GenericSubstitution,
    ) -> Result<(), CheckStop> {
        if self
            .tree
            .children_with(function, Production::ResultBinding)?
            .len()
            < 2
        {
            return Ok(());
        }
        let Some(results) = self.result_list_fields(function, substitution)? else {
            return Ok(());
        };
        self.intern_result_list_nominal(&results).map(|_| ())
    }

    /// Interns only nominal instances read by `build_function_signature`.
    /// The throwaway FN-9 preflight must not inspect requires, ensures, or the
    /// executable body before selector admission.
    pub(super) fn ensure_nominals_in_function_signature(
        &mut self,
        function: NodeId,
        substitution: &GenericSubstitution,
    ) -> Result<(), CheckStop> {
        if let Some(parameters) = self
            .tree
            .first_child_with(function, Production::ParamList)?
        {
            self.ensure_nominals_in_node(parameters, substitution)?;
        }
        // [GRAM-2] a `fn_decl` writes one result or an ordered result list;
        // every ordinal's `rtype` is a signature type, and a list also needs
        // the compiler-owned nominal that carries the ordinals [CALL-4].
        let result_bindings = self
            .tree
            .children_with(function, Production::ResultBinding)?;
        if result_bindings.is_empty() {
            return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
        }
        for binding in &result_bindings {
            let result = self
                .tree
                .first_child_with(*binding, Production::Rtype)?
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            self.ensure_nominals_in_node(result, substitution)?;
        }
        self.ensure_result_list_nominal(function, substitution)
    }

    pub(super) fn ensure_nominal_type(
        &mut self,
        node: NodeId,
        substitution: &GenericSubstitution,
    ) -> Result<(), CheckStop> {
        for nested in self.nominal_type_descendants(node)? {
            self.ensure_nominal_type_head(nested, substitution)?;
        }
        self.ensure_nominal_type_head(node, substitution)
    }

    fn nominal_type_descendants(&self, node: NodeId) -> Result<Vec<NodeId>, CheckStop> {
        let mut nested = self.tree.descendants_with(node, Production::Type)?;
        let mut uses = Vec::with_capacity(nested.len());
        for ty in nested {
            if self
                .optional_declaration_at(ty, DeclarationRole::GenericType)?
                .is_none()
            {
                uses.push(ty);
            }
        }
        nested = uses;
        nested.sort_by(|left, right| {
            let left_depth = self
                .tree
                .topology()
                .node(*left)
                .map(|record| record.tree_depth);
            let right_depth = self
                .tree
                .topology()
                .node(*right)
                .map(|record| record.tree_depth);
            right_depth
                .cmp(&left_depth)
                .then(left.index().cmp(&right.index()))
        });
        Ok(nested)
    }

    fn ensure_nominal_type_head(
        &mut self,
        node: NodeId,
        substitution: &GenericSubstitution,
    ) -> Result<(), CheckStop> {
        // [TYPE-9] `box<T>` is no longer a grammar atom: `Box<T>` is the
        // prelude's opaque struct and reaches the container branch below.
        if !self.tree.names_nominal(node)? {
            return Ok(());
        }
        let usage = self.use_at(node, LexicalUseRole::Type)?;
        // S39 a written `Box<'s, T>` is interned here, exactly as a written
        // `box<T>` is: the parse path is `&self` and cannot intern, and a
        // nominal's own field types are parsed there.
        if let ResolvedTarget::Container(id) = usage.target()
            && crate::container_nominal(id)
                .is_some_and(|entry| entry.shape == crate::ContainerShape::Box)
        {
            // [TYPE-9] `Box<T>`: one written type argument, its content.
            // The argument is reached the way every other `targs` argument
            // is -- the argument list, then its one `targ`, then that
            // argument's `type` -- because `type := TYPEID targs?` puts no
            // `type` among the head's own children.
            let Some(targs) = self.tree.argument_list(node)? else {
                return Ok(());
            };
            let arguments = self.tree.children_with(targs, Production::Targ)?;
            let [referent] = arguments.as_slice() else {
                return Ok(());
            };
            let Some(referent_node) = self.tree.first_child_with(*referent, Production::Type)?
            else {
                return Ok(());
            };
            self.ensure_nominals_in_node(referent_node, substitution)?;
            let referent = self.parse_type_with(referent_node, substitution)?;
            self.intern_box_nominal(referent)?;
            return Ok(());
        }
        match usage.target() {
            ResolvedTarget::Prelude(id) if id == BuiltinPreludeId::OPTION => {
                let value = self.option_type_argument_with(node, substitution)?;
                self.intern_prelude_nominal(PreludeType::Option(value))?;
                Ok(())
            }
            ResolvedTarget::Prelude(id) if id == BuiltinPreludeId::RESULT => {
                let (ok, error) = self.result_type_arguments_with(node, substitution)?;
                self.intern_prelude_nominal(PreludeType::Result(ok, error))?;
                Ok(())
            }
            ResolvedTarget::Source {
                declaration,
                class: DeclarationClass::NominalType,
            } => {
                if self.postcondition_declaration_unavailable(declaration) {
                    return Err(CheckStop::PostconditionPrerequisiteUnavailable);
                }
                let Some(template_index) = self
                    .nominal_templates_by_declaration
                    .get(&declaration)
                    .copied()
                else {
                    return Err(SemanticCompilerFailure::InvalidResolution.into());
                };
                let template = self
                    .nominal_templates
                    .get(template_index)
                    .cloned()
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                let instance = self.nominal_generic_substitution(
                    node,
                    &template.generic_parameters,
                    &template.region_parameters,
                    substitution,
                )?;
                self.ensure_source_nominal_instance(template_index, instance)?;
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn ensure_source_constructor_instance(
        &mut self,
        node: NodeId,
        caller: &GenericSubstitution,
    ) -> Result<(), CheckStop> {
        let usage = self.use_at(node, LexicalUseRole::Construct)?;
        // [TYPE-5] a prelude variant constructor writes its nominal's
        // arguments, so the instance it names is interned from those written
        // arguments here, before function checking reads it immutably.
        if let ResolvedTarget::Prelude(id) = usage.target() {
            match id.ordinal() {
                5 | 6 => {
                    let value = self.option_type_argument_with(node, caller)?;
                    self.intern_prelude_nominal(PreludeType::Option(value))?;
                }
                11 | 13 => {
                    let (ok, error) = self.result_type_arguments_with(node, caller)?;
                    self.intern_prelude_nominal(PreludeType::Result(ok, error))?;
                }
                _ => {}
            }
            return Ok(());
        }
        let ResolvedTarget::Source { declaration, .. } = usage.target() else {
            return Ok(());
        };
        // [TYPE-2] an opaque struct has no usable constructor and contributes
        // no constructor template, so no instance is ever named through its
        // entry; this pre-scan interns nothing for it and leaves the refusal
        // to the ordinary construct judgment, which is where the rule sites
        // its hard error.
        if self.is_opaque_struct_declaration(declaration)? {
            return Ok(());
        }
        // [FORM-8] a construct whose field operands determine a region
        // parameter does not write it, so this pre-scan cannot read the
        // instance off the written list: it is formed while the operands are
        // checked, and interned then through the deferred-nominal route.
        if self
            .constructor_shape(declaration)?
            .is_some_and(|site| site.shape.determining_field.iter().any(Option::is_some))
        {
            return Ok(());
        }
        let constructor = *self
            .constructor_templates_by_declaration
            .get(&declaration)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let template_index = match constructor {
            ConstructorTemplate::Struct { template }
            | ConstructorTemplate::Enum { template, .. } => template,
        };
        let template = self
            .nominal_templates
            .get(template_index)
            .cloned()
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let substitution = self.nominal_generic_substitution(
            node,
            &template.generic_parameters,
            &template.region_parameters,
            caller,
        )?;
        self.ensure_source_nominal_instance(template_index, substitution)?;
        Ok(())
    }

    fn ensure_implicit_prelude_nominals(
        &mut self,
        node: NodeId,
        substitution: &GenericSubstitution,
        skip_postconditions: bool,
    ) -> Result<(), CheckStop> {
        // A `propagate_let_rhs` needed its operand's `Result` instance
        // interned from the let's written annotation. [TYPE-5] deletes that
        // annotation and [ERR-3] derives the binder from the operand's own
        // Ok payload instead, so the instance is the callee's and its
        // signature already interned it.

        for call in self.tree.descendants_with(node, Production::Call)? {
            if skip_postconditions && self.node_is_inside_postcondition(call)? {
                continue;
            }
            let callee = self
                .tree
                .first_child_with(call, Production::Callee)?
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            let spelling = self.tree.direct_spelling(callee)?;
            if spelling == b"cvt.checked" {
                self.ensure_conversion_result(call, substitution)?;
                continue;
            }
            let error = if matches!(
                spelling.as_slice(),
                b"iadd.checked"
                    | b"isub.checked"
                    | b"imul.checked"
                    | b"iabs.checked"
                    | b"ineg.checked"
            ) {
                Some(PreludeType::Overflow)
            } else if matches!(spelling.as_slice(), b"idiv.checked" | b"irem.checked") {
                Some(PreludeType::DivError)
            } else {
                None
            };
            let Some(error) = error else {
                continue;
            };
            let Some(targs) = self.tree.argument_list(call)? else {
                continue;
            };
            let arguments = self.tree.children_with(targs, Production::Targ)?;
            let [argument] = arguments.as_slice() else {
                continue;
            };
            let Some(ty_node) = self.tree.first_child_with(*argument, Production::Type)? else {
                continue;
            };
            let operand = self.parse_type_with(ty_node, substitution)?;
            if !matches!(
                operand,
                CheckedType::Integer(_) | CheckedType::GenericInt(_)
            ) {
                continue;
            }
            let error = CheckedType::Nominal(self.prelude_nominal(error)?);
            self.intern_prelude_nominal(PreludeType::Result(operand, error))?;
        }
        Ok(())
    }

    fn ensure_conversion_result(
        &mut self,
        node: NodeId,
        substitution: &GenericSubstitution,
    ) -> Result<(), CheckStop> {
        let Some(targs) = self.tree.argument_list(node)? else {
            return Ok(());
        };
        let arguments = self.tree.children_with(targs, Production::Targ)?;
        let [source_argument, destination_argument] = arguments.as_slice() else {
            return Ok(());
        };
        let (Some(source_node), Some(destination_node)) = (
            self.tree
                .first_child_with(*source_argument, Production::Type)?,
            self.tree
                .first_child_with(*destination_argument, Production::Type)?,
        ) else {
            return Ok(());
        };
        let (source, destination) = (
            self.parse_type_with(source_node, substitution)?,
            self.parse_type_with(destination_node, substitution)?,
        );
        let (Some(_), Some(destination)) = (
            CheckedNumericType::from_type(source),
            CheckedNumericType::from_type(destination),
        ) else {
            return Ok(());
        };
        let error = CheckedType::Nominal(self.prelude_nominal(PreludeType::NarrowError)?);
        self.intern_prelude_nominal(PreludeType::Result(destination.ty(), error))?;
        Ok(())
    }

    fn declare_source_nominal_instance(
        &mut self,
        template_index: usize,
        substitution: GenericSubstitution,
    ) -> Result<NominalId, CheckStop> {
        let template = self
            .nominal_templates
            .get(template_index)
            .cloned()
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        if let Some(id) = self.source_nominal_instance(template.declaration, &substitution) {
            return Ok(id);
        }
        let id = NominalId(
            u32::try_from(self.nominals.len())
                .map_err(|_| SemanticCompilerFailure::CounterOverflow)?,
        );
        let name = if substitution.len() == 0 {
            template.name.clone()
        } else {
            format!("{}<instance:{}>", template.name, id.0)
        };
        self.nominal_nodes.push(Some(template.node));
        self.nominal_states.push(0);
        self.source_nominal_instances
            .push(Some((template_index, substitution.clone())));
        self.prelude_types.push(None);
        self.push_nominal(CheckedNominal {
            id,
            name,
            kind: match template.role {
                DeclarationRole::Struct if self.is_opaque_declaration(template.node)? => {
                    CheckedNominalKind::Opaque
                }
                DeclarationRole::Struct => CheckedNominalKind::Struct { fields: Vec::new() },
                DeclarationRole::Enum => CheckedNominalKind::Enum {
                    variants: Vec::new(),
                },
                _ => return Err(SemanticCompilerFailure::InvalidResolution.into()),
            },
            linear: template.linear,
            nocopy: template.nocopy,
        });
        self.nominals_by_declaration
            .entry(template.declaration)
            .or_default()
            .push(NominalInstance { id, substitution });
        Ok(id)
    }

    /// [TYPE-6] whether this nominal is an instance of that source
    /// declaration.
    pub(super) fn nominal_instantiates(
        &self,
        nominal: crate::NominalId,
        declaration: crate::DeclarationId,
    ) -> Result<bool, CheckStop> {
        let Some(template) = self.nominal_templates_by_declaration.get(&declaration) else {
            return Ok(false);
        };
        Ok(self
            .source_nominal_instances
            .get(nominal.0 as usize)
            .and_then(|entry| entry.as_ref())
            .is_some_and(|(index, _)| index == template))
    }

    pub(super) fn ensure_source_nominal_instance(
        &mut self,
        template_index: usize,
        substitution: GenericSubstitution,
    ) -> Result<NominalId, CheckStop> {
        let id = self.declare_source_nominal_instance(template_index, substitution)?;
        self.complete_source_nominal_instance(id)?;
        Ok(id)
    }

    fn complete_pending_source_nominals(&mut self) -> Result<(), CheckStop> {
        let mut index = 0_usize;
        while index < self.nominals.len() {
            if self
                .source_nominal_instances
                .get(index)
                .is_some_and(Option::is_some)
            {
                self.complete_source_nominal_instance(NominalId(
                    u32::try_from(index).map_err(|_| SemanticCompilerFailure::CounterOverflow)?,
                ))?;
            }
            index = index
                .checked_add(1)
                .ok_or(SemanticCompilerFailure::CounterOverflow)?;
        }
        Ok(())
    }

    fn validate_nominal_templates(&mut self) -> Result<(), CheckStop> {
        let checkpoint = self.nominal_checkpoint();
        for template_index in 0..self.nominal_templates.len() {
            let parameters = self.nominal_templates[template_index]
                .generic_parameters
                .clone();
            let region_parameters = self.nominal_templates[template_index]
                .region_parameters
                .clone();
            if parameters.is_empty() && region_parameters.is_empty() {
                continue;
            }
            let substitution =
                self.symbolic_nominal_substitution(&parameters, &region_parameters)?;
            let id = self.ensure_source_nominal_instance(template_index, substitution)?;
            // [FORM-8] the one place a declaration's fields can be read
            // against its own region parameters: this instance carries each
            // region parameter as its own argument, so a field type naming
            // one is visibly that parameter and not some caller's actual.
            if !region_parameters.is_empty() {
                let constructors = self.constructor_shapes(id, &region_parameters)?;
                self.nominal_templates
                    .get_mut(template_index)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?
                    .constructors = constructors;
            }
        }
        self.reject_recursive_nominal_layouts()?;
        self.restore_nominal_checkpoint(checkpoint)
    }

    /// One declaration's constructors, read off its symbolic instance
    /// [FORM-8].
    ///
    /// A field determines a region parameter exactly when its declared type
    /// names that region — the same relation [FORM-8] uses at a call, where a
    /// parameter whose type names a formal region determines it from the
    /// actual. Where two fields name one region parameter the first decides
    /// it and the rest are the ordinary [TYPE-5] equality against the formed
    /// instance, exactly as a call's second store operand is.
    fn constructor_shapes(
        &self,
        id: NominalId,
        region_parameters: &[crate::DeclarationId],
    ) -> Result<Vec<super::ConstructorShape>, CheckStop> {
        let variants: Vec<&[super::super::model::CheckedField]> = match &self.nominal(id)?.kind {
            CheckedNominalKind::Struct { fields } => vec![fields.as_slice()],
            CheckedNominalKind::Enum { variants } => variants
                .iter()
                .map(|variant| variant.fields.as_slice())
                .collect(),
            _ => return Ok(Vec::new()),
        };
        let mut constructors = Vec::with_capacity(variants.len());
        for fields in variants {
            let field_regions = fields
                .iter()
                .map(|field| self.type_region_shape(field.ty, None))
                .collect::<Result<Vec<_>, _>>()?;
            let mut determining_field = vec![None; region_parameters.len()];
            for (index, shape) in field_regions.iter().enumerate() {
                for (slot, formal) in region_parameters.iter().enumerate() {
                    if determining_field[slot].is_none() && shape.determines(*formal) {
                        determining_field[slot] = Some(index);
                    }
                }
            }
            constructors.push(super::ConstructorShape {
                fields: fields.iter().map(|field| field.name.clone()).collect(),
                determining_field,
            });
        }
        Ok(constructors)
    }

    /// The constructor shape one `construct` names, when its declaration
    /// carries `region_params` [FORM-8]; `None` for every declaration that
    /// does not, where a construct writes no region argument at all and the
    /// instance is formed from the written list alone.
    pub(super) fn constructor_shape(
        &self,
        declaration: crate::DeclarationId,
    ) -> Result<Option<super::ConstructorSite>, CheckStop> {
        let constructor = *self
            .constructor_templates_by_declaration
            .get(&declaration)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let (template_index, variant) = match constructor {
            ConstructorTemplate::Struct { template } => (template, None),
            ConstructorTemplate::Enum { template, variant } => (template, Some(variant)),
        };
        let template = self
            .nominal_templates
            .get(template_index)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let Some(shape) = template
            .constructors
            .get(variant.unwrap_or(0) as usize)
            .cloned()
        else {
            return Ok(None);
        };
        Ok(Some(super::ConstructorSite {
            template: template_index,
            variant,
            generic_parameters: template.generic_parameters.clone(),
            region_parameters: template.region_parameters.clone(),
            shape,
        }))
    }

    /// The instance one `construct` names, at the regions its own field
    /// operands determined and the ones it wrote [FORM-8, TYPE-5].
    ///
    /// No position of the writer's text need spell that instance — a
    /// construct whose every region parameter a field determines writes no
    /// region argument at all — so the interning pass cannot have found it,
    /// and a miss is the ordinary deferred-nominal report the driver repairs.
    pub(super) fn constructed_nominal(
        &self,
        node: NodeId,
        site: &super::ConstructorSite,
        determined: &[(crate::DeclarationId, crate::DeclarationId)],
        caller: &GenericSubstitution,
    ) -> Result<NominalId, CheckStop> {
        let substitution = self.nominal_generic_substitution_with(
            node,
            &site.generic_parameters,
            &site.region_parameters,
            determined,
            caller,
        )?;
        let declaration = self
            .nominal_templates
            .get(site.template)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?
            .declaration;
        if let Some(existing) = self.source_nominal_instance(declaration, &substitution) {
            return Ok(existing);
        }
        self.pending_nominals
            .borrow_mut()
            .push(super::PendingNominal::SourceInstance {
                template: site.template,
                substitution,
            });
        Err(CheckStop::DeferredNominal)
    }

    fn complete_source_nominal_instance(&mut self, id: NominalId) -> Result<(), CheckStop> {
        match self
            .nominal_states
            .get(id.0 as usize)
            .copied()
            .ok_or(SemanticCompilerFailure::InvalidResolution)?
        {
            2 => return Ok(()),
            1 => return Ok(()),
            0 => {}
            _ => return Err(SemanticCompilerFailure::InvalidResolution.into()),
        }
        self.nominal_states[id.0 as usize] = 1;
        let (template_index, substitution) = self
            .source_nominal_instances
            .get(id.0 as usize)
            .and_then(Clone::clone)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let template = self
            .nominal_templates
            .get(template_index)
            .cloned()
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        // [PROV-1] an elided stored brand belongs to this declaration's
        // sole region, not a caller's enclosing nominal. Zero/multiple
        // regions mask any outer nominal's elision context.
        let brand = match substitution.region_arguments() {
            [(_, region)] => Some(*region),
            _ => None,
        };
        let outer_brand = self.elided_store_brand.replace(brand);
        let kind = (|| {
            Ok(match template.role {
                DeclarationRole::Struct if self.is_opaque_declaration(template.node)? => {
                    CheckedNominalKind::Opaque
                }
                DeclarationRole::Struct => CheckedNominalKind::Struct {
                    fields: self.parse_struct_fields(template.node, &substitution)?,
                },
                DeclarationRole::Enum => CheckedNominalKind::Enum {
                    variants: self.parse_enum_variants(template.node, &substitution)?,
                },
                _ => return Err(CheckStop::from(SemanticCompilerFailure::InvalidResolution)),
            })
        })();
        self.elided_store_brand.set(outer_brand);
        let kind = kind?;
        self.nominals
            .get_mut(id.0 as usize)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?
            .kind = kind;
        self.nominal_table_changed();
        self.nominal_states[id.0 as usize] = 2;
        Ok(())
    }

    fn parse_struct_fields(
        &mut self,
        node: NodeId,
        substitution: &GenericSubstitution,
    ) -> Result<Vec<CheckedField>, CheckStop> {
        let nodes = self.tree.children_with(node, Production::Field)?;
        let mut seen = HashSet::with_capacity(nodes.len());
        let mut fields = Vec::with_capacity(nodes.len());
        for field in nodes {
            let declaration =
                self.dependent_declaration_at(field, DependentDeclarationRole::Field)?;
            let name = declaration.spelling().to_owned();
            if !seen.insert(name.clone()) {
                return self.issue_node(
                    SemanticRule::Type6,
                    field,
                    SemanticIssueKind::DuplicateFieldLabel { label: name },
                );
            }
            let ty = self
                .tree
                .first_child_with(field, Production::Type)?
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            self.ensure_nominal_type(ty, substitution)?;
            let parsed = self.parse_type_with(ty, substitution)?;
            self.reject_inline_runtime_capacity(ty, parsed)?;
            // [TYPE-2, GRAM-2] `field := "readonly"? IDENT ":" type ";"`: the
            // written modifier is what makes the field unassignable.
            let readonly = self
                .tree
                .direct_token_with(
                    field,
                    TerminalPredicate::Fixed(crate::FixedTerminal::Readonly),
                )?
                .is_some();
            fields.push(CheckedField {
                name,
                ty: parsed,
                readonly,
            });
        }
        Ok(fields)
    }

    fn parse_enum_variants(
        &mut self,
        node: NodeId,
        substitution: &GenericSubstitution,
    ) -> Result<Vec<CheckedVariant>, CheckStop> {
        let nodes = self.tree.children_with(node, Production::Variant)?;
        let mut variants = Vec::with_capacity(nodes.len());
        for variant_node in nodes {
            let declaration = self.declaration_at(variant_node, DeclarationRole::Variant)?;
            let declaration_id = declaration.id();
            let name = declaration.spelling().to_owned();
            let tag = u32::try_from(variants.len())
                .map_err(|_| SemanticCompilerFailure::CounterOverflow)?;
            let mut fields = Vec::new();
            let mut seen = HashSet::new();
            if let Some(list) = self
                .tree
                .first_child_with(variant_node, Production::VfieldList)?
            {
                for field in self.tree.children_with(list, Production::Vfield)? {
                    let declaration = self
                        .dependent_declaration_at(field, DependentDeclarationRole::VariantField)?;
                    let field_name = declaration.spelling().to_owned();
                    if !seen.insert(field_name.clone()) {
                        return self.issue_node(
                            SemanticRule::Type6,
                            field,
                            SemanticIssueKind::DuplicateFieldLabel { label: field_name },
                        );
                    }
                    let ty = self
                        .tree
                        .first_child_with(field, Production::Type)?
                        .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
                    self.ensure_nominal_type(ty, substitution)?;
                    let parsed = self.parse_type_with(ty, substitution)?;
                    self.reject_inline_runtime_capacity(ty, parsed)?;
                    // [GRAM-2] a `vfield` carries no modifier: `readonly` is
                    // a `field` alternative and an enum payload has none.
                    fields.push(CheckedField {
                        name: field_name,
                        ty: parsed,
                        readonly: false,
                    });
                }
            }
            variants.push(CheckedVariant {
                name,
                constructor: CheckedConstructor::Source(declaration_id),
                tag,
                fields,
            });
        }
        Ok(variants)
    }

    /// [S20] where each nominal instance's region axis leaves the program.
    ///
    /// Two instances share a representation when their region-erased source
    /// families and complete reclamation graphs agree. Region identity is
    /// proof-only, but the store's release class still selects runtime work.
    /// This table uses declaration-level classes; physical specialization
    /// interprets those classes in each accepted call's closed environment.
    pub(super) fn nominal_lowering_aliases(&self) -> Result<Vec<NominalId>, CheckStop> {
        self.nominal_aliases(true)
    }

    /// The region-erased source/type family used to select a release-class
    /// specialization after semantic acceptance. Unlike the ordinary
    /// lowering alias, this deliberately ignores a Box or Vector release
    /// class; the physical specialization key restores those classes before
    /// any IR type or cleanup action is selected.
    pub(super) fn nominal_physical_aliases(&self) -> Result<Vec<NominalId>, CheckStop> {
        self.nominal_aliases(false)
    }

    fn nominal_aliases(&self, release_sensitive: bool) -> Result<Vec<NominalId>, CheckStop> {
        let mut aliases = Vec::with_capacity(self.nominals.len());
        for index in 0..self.nominals.len() {
            let id = NominalId(
                u32::try_from(index).map_err(|_| SemanticCompilerFailure::CounterOverflow)?,
            );
            let mut alias = id;
            for earlier in 0..index {
                let candidate = NominalId(
                    u32::try_from(earlier).map_err(|_| SemanticCompilerFailure::CounterOverflow)?,
                );
                if self.nominals_share_region_erased_family(id, candidate, release_sensitive)? {
                    alias = candidate;
                    break;
                }
            }
            aliases.push(alias);
        }
        Ok(aliases)
    }

    /// The actual one formal region of a call denotes, or the region itself
    /// when this call substitutes nothing for it [FORM-8].
    pub(super) fn substituted_region(
        regions: &[(crate::DeclarationId, crate::DeclarationId)],
        region: crate::DeclarationId,
    ) -> crate::DeclarationId {
        regions
            .iter()
            .find_map(|(formal, actual)| (*formal == region).then_some(*actual))
            .unwrap_or(region)
    }

    /// One checked type with each formal region replaced by the actual a call
    /// determined for it [FN-2, FORM-8, PROV-1].
    ///
    /// A call's region arguments are fixed by its operands and by the `::`
    /// members [FORM-8] leaves it to write, and [FN-2] substitutes them into
    /// every position of the callee's signature — a result position exactly as
    /// much as a parameter position. Two calls of one declaration at two
    /// stores therefore hand back two types, which is what [PROV-1]'s
    /// invariant store region means where the value is produced rather than
    /// consumed.
    ///
    /// The walk is structural and closed over the checked type domain, so a
    /// region is reached wherever it occurs: under `Option` and `Result`, in
    /// a source nominal's own instance and in that instance's type arguments,
    /// in a run's element position, and in the compiler-owned result-list
    /// nominal a multi-result callable hands back [CALL-4]. A run's release
    /// class is a function of its region's declaration [PROV-6], so it is
    /// re-read from the substituted region rather than carried across.
    ///
    /// An instance the substitution names may exist nowhere in the caller's
    /// written text — no position of `let held = make(store: &uniq one);`
    /// spells the result's type — so a miss is the ordinary deferred-nominal
    /// report the driver repairs, exactly as a derived `box<T>` is.
    pub(super) fn substitute_type_regions(
        &self,
        ty: CheckedType,
        regions: &[(crate::DeclarationId, crate::DeclarationId)],
    ) -> Result<CheckedType, CheckStop> {
        if regions.is_empty() {
            return Ok(ty);
        }
        Ok(match ty {
            CheckedType::Unit
            | CheckedType::Bool
            | CheckedType::Integer(_)
            | CheckedType::Float(_)
            | CheckedType::Generic(_)
            | CheckedType::GenericInt(_)
            | CheckedType::GenericFloat(_) => ty,
            CheckedType::Nominal(id) => self.substitute_nominal_regions(id, regions)?,
            CheckedType::Array { element, length } => CheckedType::Array {
                element: self.substitute_element_regions(element, regions)?,
                length,
            },
            CheckedType::Buffer { element } => CheckedType::Buffer {
                element: self.substitute_element_regions(element, regions)?,
            },
            CheckedType::Window {
                shape,
                element,
                capacity,
            } => CheckedType::Window {
                shape,
                element: self.substitute_element_regions(element, regions)?,
                capacity,
            },
        })
    }

    /// One slot's content with the same substitution [BLK-1].
    fn substitute_element_regions(
        &self,
        element: CheckedElement,
        regions: &[(crate::DeclarationId, crate::DeclarationId)],
    ) -> Result<CheckedElement, CheckStop> {
        let ty = self.substitute_type_regions(self.element_type(element)?, regions)?;
        self.intern_element(ty)
    }

    /// One nominal instance with the same substitution, reported as a
    /// deferred nominal when the instance it names is not interned yet.
    fn substitute_nominal_regions(
        &self,
        id: NominalId,
        regions: &[(crate::DeclarationId, crate::DeclarationId)],
    ) -> Result<CheckedType, CheckStop> {
        if let Some(prelude) = self.prelude_type(id) {
            let substituted = match prelude {
                PreludeType::Option(value) => {
                    PreludeType::Option(self.substitute_type_regions(value, regions)?)
                }
                PreludeType::Result(ok, error) => PreludeType::Result(
                    self.substitute_type_regions(ok, regions)?,
                    self.substitute_type_regions(error, regions)?,
                ),
                PreludeType::Overflow | PreludeType::DivError | PreludeType::NarrowError => prelude,
            };
            if substituted == prelude {
                return Ok(CheckedType::Nominal(id));
            }
            return Ok(CheckedType::Nominal(self.prelude_nominal(substituted)?));
        }
        if let Some((template, instance)) = self.source_nominal_instance_entry(id)? {
            let mut changed = false;
            let mut bindings = Vec::with_capacity(instance.entries().len());
            for (declaration, argument) in instance.entries() {
                bindings.push((
                    *declaration,
                    match argument {
                        super::generics::GenericArgument::Type(ty) => {
                            let substituted = self.substitute_type_regions(*ty, regions)?;
                            changed |= substituted != *ty;
                            super::generics::GenericArgument::Type(substituted)
                        }
                        super::generics::GenericArgument::Const(value) => {
                            super::generics::GenericArgument::Const(*value)
                        }
                        super::generics::GenericArgument::Function(value) => {
                            let substituted =
                                self.substitute_function_argument_regions(*value, regions)?;
                            changed |= substituted != *value;
                            super::generics::GenericArgument::Function(substituted)
                        }
                    },
                ));
            }
            let mut axis = Vec::with_capacity(instance.region_arguments().len());
            for (formal, actual) in instance.region_arguments() {
                let substituted = Self::substituted_region(regions, *actual);
                changed |= substituted != *actual;
                axis.push((*formal, substituted));
            }
            if !changed {
                return Ok(CheckedType::Nominal(id));
            }
            let target = GenericSubstitution::from_bindings(bindings)?.with_regions(axis);
            let declaration = self
                .nominal_templates
                .get(template)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?
                .declaration;
            if let Some(existing) = self.source_nominal_instance(declaration, &target) {
                return Ok(CheckedType::Nominal(existing));
            }
            self.pending_nominals
                .borrow_mut()
                .push(super::PendingNominal::SourceInstance {
                    template,
                    substitution: target,
                });
            return Err(CheckStop::DeferredNominal);
        }
        if let Some((results, _)) = self
            .result_list_nominals
            .iter()
            .find(|(_, candidate)| **candidate == id)
        {
            let mut changed = false;
            let mut substituted = Vec::with_capacity(results.len());
            for (name, ty) in results {
                let ordinal = self.substitute_type_regions(*ty, regions)?;
                changed |= ordinal != *ty;
                substituted.push((name.clone(), ordinal));
            }
            if !changed {
                return Ok(CheckedType::Nominal(id));
            }
            let Some(existing) = self.result_list_nominal(&substituted) else {
                self.pending_nominals
                    .borrow_mut()
                    .push(super::PendingNominal::ResultList(substituted));
                return Err(CheckStop::DeferredNominal);
            };
            return Ok(CheckedType::Nominal(existing));
        }
        match &self.nominal(id)?.kind {
            CheckedNominalKind::Box { referent, .. } => {
                let substituted = self.substitute_type_regions(*referent, regions)?;
                if substituted == *referent {
                    return Ok(CheckedType::Nominal(id));
                }
                let Some(existing) = self.box_nominals.get(&substituted).copied() else {
                    self.pending_nominals
                        .borrow_mut()
                        .push(super::PendingNominal::Box(substituted));
                    return Err(CheckStop::DeferredNominal);
                };
                Ok(CheckedType::Nominal(existing))
            }
            CheckedNominalKind::Struct { .. }
            | CheckedNominalKind::Enum { .. }
            | CheckedNominalKind::Opaque => Ok(CheckedType::Nominal(id)),
        }
    }

    /// [S20] whether two nominals are two names for one representation that
    /// differ in their regions alone.
    ///
    /// The same name shape — one source declaration at two regions, one
    /// prelude shape over two region-blind-equal arguments, one result list
    /// [CALL-4] with the same ordinal names, or one `box`
    /// [STOR-2] — and the same lowered content: a region names a store for
    /// the proof, so two such nominals are two checked types and one IR
    /// nominal. The content comparison is what keeps a difference the run
    /// time *can* see out of the relation — a run or box's release class is
    /// read off its region's own declaration [PROV-6]. Representation aliasing
    /// keeps those classes distinct; the release-insensitive formation view
    /// compares only the structural family.
    ///
    /// The relation reaches beyond a source instance because a call's region
    /// substitution does: a callee returning `own Option<BlockPool<'s>>`
    /// hands its caller an `Option<BlockPool<'a>>` [FN-2], and a multi-result
    /// callee hands back its result-list nominal with every ordinal
    /// substituted, so those two classes meet at the same boundary a source
    /// instance does.
    fn nominals_share_region_erased_family(
        &self,
        left: NominalId,
        right: NominalId,
        release_sensitive: bool,
    ) -> Result<bool, CheckStop> {
        if left == right {
            return Ok(false);
        }
        // Every obligation is a pair of nodes in the finite checked type
        // graph. Visit each pair once, including recursive boxes, and finish
        // every reachable obligation. Neither depth nor traversal order can
        // turn two members of one family into unrelated representations.
        let mut pending = vec![(CheckedType::Nominal(left), CheckedType::Nominal(right))];
        let mut visited = HashSet::new();
        while let Some((left, right)) = pending.pop() {
            if left == right || !visited.insert((left, right)) {
                continue;
            }
            let same = match (left, right) {
                (CheckedType::Nominal(left), CheckedType::Nominal(right)) => self
                    .nominals_have_same_region_erased_shape(
                        left,
                        right,
                        release_sensitive,
                        &mut pending,
                    )?,
                (
                    CheckedType::Window {
                        shape: left_shape,
                        element: left,
                        capacity: left_capacity,
                    },
                    CheckedType::Window {
                        shape: right_shape,
                        element: right,
                        capacity: right_capacity,
                    },
                ) => {
                    pending.push((self.element_type(left)?, self.element_type(right)?));
                    left_shape == right_shape && left_capacity == right_capacity
                }
                (
                    CheckedType::Array {
                        element: left,
                        length: left_length,
                    },
                    CheckedType::Array {
                        element: right,
                        length: right_length,
                    },
                ) => {
                    pending.push((self.element_type(left)?, self.element_type(right)?));
                    left_length == right_length
                }
                (CheckedType::Buffer { element: left }, CheckedType::Buffer { element: right }) => {
                    pending.push((self.element_type(left)?, self.element_type(right)?));
                    true
                }
                _ => false,
            };
            if !same {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn nominals_have_same_region_erased_shape(
        &self,
        left: NominalId,
        right: NominalId,
        release_sensitive: bool,
        pending: &mut Vec<(CheckedType, CheckedType)>,
    ) -> Result<bool, CheckStop> {
        // Box content lives in the kind, rather than in fields. The ordinary
        // lowering alias retains Box's release action;
        // the physical family defers it to the closed release environment.
        match (&self.nominal(left)?.kind, &self.nominal(right)?.kind) {
            (
                CheckedNominalKind::Box {
                    referent: left,
                    release: left_release,
                    ..
                },
                CheckedNominalKind::Box {
                    referent: right,
                    release: right_release,
                    ..
                },
            ) => {
                pending.push((*left, *right));
                return Ok(!release_sensitive || left_release == right_release);
            }
            (CheckedNominalKind::Box { .. }, _) | (_, CheckedNominalKind::Box { .. }) => {
                return Ok(false);
            }
            _ => {}
        }
        if !self.nominal_names_are_region_blind_equal(left, right, pending)? {
            return Ok(false);
        }
        let (left_nominal, right_nominal) = (self.nominal(left)?, self.nominal(right)?);
        if left_nominal.linear != right_nominal.linear
            || left_nominal.nocopy != right_nominal.nocopy
        {
            return Ok(false);
        }
        match (&left_nominal.kind, &right_nominal.kind) {
            (
                CheckedNominalKind::Struct { fields: left },
                CheckedNominalKind::Struct { fields: right },
            ) => Ok(Self::queue_region_blind_fields(left, right, pending)),
            (
                CheckedNominalKind::Enum { variants: left },
                CheckedNominalKind::Enum { variants: right },
            ) => {
                if left.len() != right.len() {
                    return Ok(false);
                }
                for (left, right) in left.iter().zip(right) {
                    if left.name != right.name
                        || left.tag != right.tag
                        || !Self::queue_region_blind_fields(&left.fields, &right.fields, pending)
                    {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// Compare identity before content: identical layout never merges two
    /// source declarations, different const arguments, or phantom type
    /// arguments. A type argument may itself contain the only region axis.
    fn nominal_names_are_region_blind_equal(
        &self,
        left: NominalId,
        right: NominalId,
        pending: &mut Vec<(CheckedType, CheckedType)>,
    ) -> Result<bool, CheckStop> {
        let (left_source, right_source) = (
            self.source_nominal_instance_entry(left)?,
            self.source_nominal_instance_entry(right)?,
        );
        if let (Some((left_template, left_instance)), Some((right_template, right_instance))) =
            (left_source, right_source)
        {
            if left_template != right_template
                || left_instance.entries().len() != right_instance.entries().len()
            {
                return Ok(false);
            }
            for ((left_declaration, left_argument), (right_declaration, right_argument)) in
                left_instance.entries().iter().zip(right_instance.entries())
            {
                if left_declaration != right_declaration {
                    return Ok(false);
                }
                use super::generics::GenericArgument;
                match (left_argument, right_argument) {
                    (GenericArgument::Type(left), GenericArgument::Type(right)) => {
                        pending.push((*left, *right));
                    }
                    (GenericArgument::Const(left), GenericArgument::Const(right))
                        if left == right => {}
                    (GenericArgument::Function(left), GenericArgument::Function(right))
                        if left == right => {}
                    _ => return Ok(false),
                }
            }
            return Ok(true);
        }
        if left_source.is_some() || right_source.is_some() {
            return Ok(false);
        }
        let (left_prelude, right_prelude) = (self.prelude_type(left), self.prelude_type(right));
        if let (Some(left), Some(right)) = (left_prelude, right_prelude) {
            return Ok(match (left, right) {
                (PreludeType::Option(left), PreludeType::Option(right)) => {
                    pending.push((left, right));
                    true
                }
                (
                    PreludeType::Result(left_ok, left_error),
                    PreludeType::Result(right_ok, right_error),
                ) => {
                    pending.push((left_ok, right_ok));
                    pending.push((left_error, right_error));
                    true
                }
                (left, right) => left == right,
            });
        }
        if left_prelude.is_some() || right_prelude.is_some() {
            return Ok(false);
        }
        let (left_list, right_list) = (
            self.result_list_ordinal_names(left),
            self.result_list_ordinal_names(right),
        );
        Ok(matches!((left_list, right_list), (Some(left), Some(right)) if left == right))
    }

    fn queue_region_blind_fields(
        left: &[CheckedField],
        right: &[CheckedField],
        pending: &mut Vec<(CheckedType, CheckedType)>,
    ) -> bool {
        if left.len() != right.len() {
            return false;
        }
        for (left, right) in left.iter().zip(right) {
            if left.name != right.name {
                return false;
            }
            pending.push((left.ty, right.ty));
        }
        true
    }

    /// The ordinal names of a compiler-owned result-list nominal [CALL-4],
    /// absent for every nominal that is not one.
    fn result_list_ordinal_names(&self, id: NominalId) -> Option<Vec<String>> {
        self.result_list_nominals
            .iter()
            .find(|(_, candidate)| **candidate == id)
            .map(|(results, _)| results.iter().map(|(name, _)| name.clone()).collect())
    }

    /// The template index and instance arguments of one source nominal, when
    /// it is a source declaration's instance rather than a compiler-owned
    /// nominal [S20].
    pub(super) fn source_nominal_instance_entry(
        &self,
        id: NominalId,
    ) -> Result<Option<(usize, &GenericSubstitution)>, CheckStop> {
        Ok(self
            .source_nominal_instances
            .get(id.0 as usize)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?
            .as_ref()
            .map(|(template, substitution)| (*template, substitution)))
    }

    /// One nominal instance's region axis [S20], absent for every nominal that
    /// is not a source declaration's instance.
    pub(super) fn nominal_region_axis(
        &self,
        id: NominalId,
    ) -> Result<Option<&[(crate::DeclarationId, crate::DeclarationId)]>, CheckStop> {
        Ok(self
            .source_nominal_instance_entry(id)?
            .map(|(_, substitution)| substitution.region_arguments()))
    }

    pub(super) fn source_nominal_instance(
        &self,
        declaration: crate::DeclarationId,
        substitution: &GenericSubstitution,
    ) -> Option<NominalId> {
        self.nominals_by_declaration
            .get(&declaration)
            .into_iter()
            .flatten()
            .find(|instance| instance.substitution == *substitution)
            .map(|instance| instance.id)
    }

    /// [TYPE-2] whether this declaration is an `opaque struct`, whose
    /// constructor entry exists only to be refused.
    ///
    /// An opaque struct contributes no constructor template at all, so the
    /// refusal has to be made from the nominal template before the ordinary
    /// constructor lookup, which would otherwise report a missing entry as a
    /// resolution failure rather than as the rule's own hard error.
    pub(super) fn is_opaque_struct_declaration(
        &self,
        declaration: crate::DeclarationId,
    ) -> Result<bool, CheckStop> {
        let Some(&index) = self.nominal_templates_by_declaration.get(&declaration) else {
            return Ok(false);
        };
        let template = self
            .nominal_templates
            .get(index)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        Ok(
            template.role == DeclarationRole::Struct
                && self.is_opaque_declaration(template.node)?,
        )
    }

    /// [TYPE-2, PRE-2] where an opaque struct a construct names comes from,
    /// which selects the repair of its refusal. The standard library declares
    /// opaque structs only in its host modules, each a fieldless host handle
    /// that a host function forms [PRE-2], so its package decides; a program's
    /// own opaque struct never has a value.
    pub(super) fn opaque_struct_kind(
        &self,
        declaration: crate::DeclarationId,
    ) -> Result<super::repairs::OpaqueStruct, CheckStop> {
        if !self
            .declaration_home(declaration)
            .is_some_and(|(package, _)| package == crate::Package::Standard)
        {
            return Ok(super::repairs::OpaqueStruct::Program);
        }
        let template = self
            .nominal_templates_by_declaration
            .get(&declaration)
            .and_then(|&index| self.nominal_templates.get(index))
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        Ok(super::repairs::OpaqueStruct::HostHandle {
            linear: self.declaration_is_linear(template.node)?,
        })
    }

    pub(super) fn source_constructor(
        &self,
        node: NodeId,
        declaration: crate::DeclarationId,
        caller: &GenericSubstitution,
    ) -> Result<super::Constructor, CheckStop> {
        let constructor = *self
            .constructor_templates_by_declaration
            .get(&declaration)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let template_index = match constructor {
            ConstructorTemplate::Struct { template }
            | ConstructorTemplate::Enum { template, .. } => template,
        };
        let template = self
            .nominal_templates
            .get(template_index)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let substitution = self.nominal_generic_substitution(
            node,
            &template.generic_parameters,
            &template.region_parameters,
            caller,
        )?;
        let nominal = self
            .source_nominal_instance(template.declaration, &substitution)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        Ok(match constructor {
            ConstructorTemplate::Struct { .. } => super::Constructor::Struct(nominal),
            ConstructorTemplate::Enum { variant, .. } => {
                super::Constructor::Enum { nominal, variant }
            }
        })
    }

    pub(super) fn nominal_checkpoint(&self) -> usize {
        self.nominals.len()
    }

    pub(super) fn restore_nominal_checkpoint(
        &mut self,
        checkpoint: usize,
    ) -> Result<(), CheckStop> {
        if checkpoint > self.nominals.len() {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        // Element identities are append-only because a retained generic goal
        // may still carry a scratch structural handle until its bridge is
        // reified. Never reuse a key whose nominal identity is being retired.
        // Unreachable historical entries are not executable type roots.
        let mut retained = HashSet::new();
        for (index, ty) in self.elements.borrow().iter().copied().enumerate() {
            let mut nominals = Vec::new();
            self.collect_type_nominals(ty, &mut nominals)?;
            if nominals.iter().all(|id| (id.0 as usize) < checkpoint) {
                retained.insert(CheckedElement(
                    u32::try_from(index).map_err(|_| SemanticCompilerFailure::CounterOverflow)?,
                ));
            }
        }
        self.element_ids
            .borrow_mut()
            .retain(|_, id| retained.contains(id));
        self.nominals.truncate(checkpoint);
        self.nominal_table_changed();
        self.nominal_nodes.truncate(checkpoint);
        self.nominal_states.truncate(checkpoint);
        self.source_nominal_instances.truncate(checkpoint);
        self.prelude_types.truncate(checkpoint);
        self.nominals_by_declaration.retain(|_, instances| {
            instances.retain(|instance| (instance.id.0 as usize) < checkpoint);
            !instances.is_empty()
        });
        self.prelude_nominals
            .retain(|_, id| (id.0 as usize) < checkpoint);
        self.box_nominals
            .retain(|_, id| (id.0 as usize) < checkpoint);
        self.result_list_nominals
            .retain(|_, id| (id.0 as usize) < checkpoint);
        Ok(())
    }
}
