pub(in crate::semantic::check) mod calls;
pub(in crate::semantic::check) mod flat_storage;
mod places;

use std::collections::HashMap;

use crate::syntax::NodeId;
use crate::syntax::terminal::{FixedTerminal, TerminalPredicate};
use crate::{
    DeclarationClass, DeclarationId, DeferredUseRole, LexicalUseRole, Production, ResolvedTarget,
    SemanticCompilerFailure, SemanticIssueKind, SemanticRule, UnsupportedSemanticFeature,
};

use super::super::model::{
    CheckedConst, CheckedExpression, CheckedIntegerOperation, CheckedMode, CheckedNominalKind,
    CheckedProjectedDrop, CheckedSetTarget, CheckedType, CheckedValue, CheckedWritablePlace,
    FloatType, IntegerType,
};
use super::super::places::ResolvedPlace;
use super::{
    CheckStop, Checker, Constructor, EffectSet, FunctionSignature, LocalBinding, TypedExpression,
};

#[derive(Clone, Copy, Eq, PartialEq)]
enum AccessKind {
    Read,
    Move,
}

#[derive(Clone, Copy)]
pub(in crate::semantic::check) enum PlaceUseContext {
    Ordinary,
    Consuming,
}

#[derive(Clone, Copy)]
struct PlaceUseOptions {
    explicit_move: bool,
    context: PlaceUseContext,
    loop_depth: usize,
}

/// One exact-target identity and every storage path a place may name.
///
/// A reference with one resolved member uses that member as `identity`, so
/// distinct holders known to name the same place satisfy exact same-target
/// judgments. A true join carries one selected address at run time but no
/// statically selected member, so its identity remains rooted at the holder.
/// Structural judgments always range over `members`: effects, readonly
/// provenance, reference invalidation and overlap may not select one incoming
/// path or replace several roots by a common prefix [REF-1].
#[derive(Clone)]
pub(in crate::semantic::check) struct ResolvedPlaceSet {
    pub(in crate::semantic::check) identity: ResolvedPlace,
    pub(in crate::semantic::check) members: Vec<ResolvedPlace>,
}

impl ResolvedPlaceSet {
    pub(in crate::semantic::check) fn one(place: ResolvedPlace) -> Self {
        Self {
            identity: place.clone(),
            members: vec![place],
        }
    }

    pub(in crate::semantic::check) fn append_step(
        &mut self,
        step: super::super::places::PlaceStep,
    ) {
        self.identity.path.push(step);
        for member in &mut self.members {
            member.path.push(step);
        }
    }

    pub(in crate::semantic::check) fn extend_fields(&mut self, fields: &[u32]) {
        self.identity.extend_fields(fields);
        for member in &mut self.members {
            member.extend_fields(fields);
        }
    }
}

/// One formed and judged [SET-1] target.
///
/// v0.60 has one mutation statement: `set_stmt := "set" place "=" expr ";"`
/// [GRAM-4] writes exactly one place. The `replace` form, its [SET-2] class
/// inversion and the region-free demand it carried all went with the regions,
/// so the two-sided `MutationForm` has no subject and the target carries the
/// one resolved place it writes.
pub(in crate::semantic::check) struct MutationTarget {
    /// The source declaration the written place is rooted at: the value
    /// binding for a bare, field or subscript target, and the reference
    /// binding for a `deref` target, whose [REF-2] validity is rechecked at
    /// the commit.
    pub(in crate::semantic::check) declaration: DeclarationId,
    /// The exact-target identity and complete resolved path set this target writes
    /// [REF-1, OWN-7].
    pub(in crate::semantic::check) place: ResolvedPlaceSet,
    /// The reference binding the target is reached through, when it is
    /// `deref(p)` or a path below one [SET-1]. Its [REF-2] validity is
    /// rechecked after the right-hand side.
    pub(in crate::semantic::check) through_reference: Option<DeclarationId>,
    /// Whether the target uses the element-position judgment [MSR-2].
    pub(in crate::semantic::check) element: bool,
    pub(in crate::semantic::check) target: CheckedSetTarget,
    pub(in crate::semantic::check) effects: EffectSet,
    /// A capability this compiler does not implement at this target, carried
    /// rather than raised so that [DIAG-1]'s order holds: every source
    /// rejection of the statement is judged before the stop, and no
    /// capability limit stands in front of a rejection.
    pub(in crate::semantic::check) unsupported: Option<UnsupportedSemanticFeature>,
}

impl Checker<'_> {
    /// Re-establish writability at the commit [SET-1, LIV-1].
    ///
    /// The loan state this re-read v0.59 is gone with the loans. What [SET-1]
    /// still rechecks after the right-hand side is the one fact the
    /// right-hand side can destroy: a target reached through a reference
    /// needs that reference still valid at the commit [REF-2].
    pub(super) fn revalidate_mutation_access(
        &self,
        through_reference: Option<DeclarationId>,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        node: NodeId,
    ) -> Result<(), CheckStop> {
        let Some(declaration) = through_reference else {
            return Ok(());
        };
        let local = bindings
            .get(&declaration)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        self.check_reference_valid(local, node)
    }
}

/// [WIN-3]'s own restructuring at an assignment over a linear place.
///
/// Assigning over an owned place releases the old value when it is affine and
/// is a hard error when it is linear, because no release exists for a linear
/// value: the writer takes it out and consumes it first.
pub(in crate::semantic::check) const WIN3_LINEAR_TARGET: &str =
    "take the linear value out and consume it before writing this place";

/// The roots [SET-1] admits for a written target, as the diagnostic names
/// them.
const SET1_WRITABLE_ROOTS: &str = "a live own-mode value binding, or a path below deref of a reference whose \
     row declares that write";

impl<'unit> Checker<'unit> {
    pub(super) fn check_set_target(
        &self,
        function: &FunctionSignature,
        node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        loop_depth: usize,
    ) -> Result<MutationTarget, CheckStop> {
        self.check_mutation_target(function, node, bindings, loop_depth)
    }

    /// The source declaration a written place is rooted at, when its base is a
    /// bare name.
    ///
    /// A `deref` base is rooted in a holder rather than in the storage the
    /// place selects, so it answers `None`: the storage that place selects is
    /// the referent's, not the holder's. [SET-1] reads this to decide the one
    /// target shape it reinitializes from dead, the complete binding.
    pub(in crate::semantic::check) fn complete_binding_target(
        &self,
        place: NodeId,
    ) -> Result<Option<DeclarationId>, CheckStop> {
        let Some(pbase) = self.tree.first_child_with(place, Production::Pbase)? else {
            return Ok(None);
        };
        if self.has_fixed(pbase, FixedTerminal::Deref)? || !self.tree.children(pbase)?.is_empty() {
            return Ok(None);
        }
        let usage = self.use_at(pbase, LexicalUseRole::PlaceBase)?;
        Ok(match usage.target() {
            ResolvedTarget::Source {
                declaration,
                class: DeclarationClass::Value,
            } => Some(declaration),
            _ => None,
        })
    }

    /// One [SET-1] target: the writability relation the rule states.
    ///
    /// The target is writable exactly when it is rooted in a live own-mode
    /// value binding, when it is `deref(p)` or a path below it where `p` is a
    /// reference parameter whose declared row carries `writes` of that path,
    /// or when `p` is a local reference variable whose named path is itself
    /// writable [SET-1, EFF-1, EFF-5].
    fn check_mutation_target(
        &self,
        function: &FunctionSignature,
        node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        loop_depth: usize,
    ) -> Result<MutationTarget, CheckStop> {
        let pbase = self
            .tree
            .first_child_with(node, Production::Pbase)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        if !self.has_fixed(pbase, FixedTerminal::Deref)? && self.tree.children(pbase)?.is_empty() {
            let usage = self.use_at(pbase, LexicalUseRole::PlaceBase)?;
            if matches!(
                usage.target(),
                ResolvedTarget::Source {
                    class: DeclarationClass::NamedConst,
                    ..
                }
            ) {
                return self.issue_node(
                    SemanticRule::Const2,
                    node,
                    SemanticIssueKind::ImmutableSetTarget,
                );
            }
            if let ResolvedTarget::Source {
                declaration,
                class: DeclarationClass::Value,
            } = usage.target()
                && bindings
                    .get(&declaration)
                    .is_some_and(|local| local.compiler_updated)
            {
                return self.issue_node(
                    SemanticRule::Set1,
                    node,
                    SemanticIssueKind::InvalidSetTarget {
                        root_class: "compiler-updated counted binder".to_owned(),
                        required_classes: SET1_WRITABLE_ROOTS,
                    },
                );
            }
        }
        let suffixes = self.tree.children_with(node, Production::Psuffix)?;
        if let Some(subscript) = self.indexing_subscript(node, &suffixes, bindings)? {
            return self.check_indexed_set_target(
                function, node, &suffixes, subscript, bindings, loop_depth,
            );
        }
        if self.has_fixed(pbase, FixedTerminal::Deref)? {
            return self.check_dereferenced_set_target(function, node, bindings);
        }
        if !self.tree.children(pbase)?.is_empty() {
            return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
        }

        let usage = self.use_at(pbase, LexicalUseRole::PlaceBase)?;
        let ResolvedTarget::Source { declaration, class } = usage.target() else {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        };
        if class == DeclarationClass::NamedConst {
            return self.issue_node(
                SemanticRule::Const2,
                node,
                SemanticIssueKind::ImmutableSetTarget,
            );
        }
        if class != DeclarationClass::Value {
            return self.issue_node(
                SemanticRule::Set1,
                node,
                SemanticIssueKind::InvalidSetTarget {
                    root_class: format!("{class:?}"),
                    required_classes: SET1_WRITABLE_ROOTS,
                },
            );
        }

        let local = bindings
            .get(&declaration)
            .cloned()
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        // [REF-1] a `set` whose target is a reference variable and whose
        // right-hand side is a `borrow_expr` rebinds that name and writes no
        // storage, so [SET-1]'s value-target judgment does not apply to it.
        // That rebinding is recognized at the statement; reaching this
        // formation with a bare reference target means the right-hand side is
        // a value, which [TYPE-7] refuses with `deref(.)`.
        if local.reference.is_some() && suffixes.is_empty() {
            return self.issue_node(
                SemanticRule::Type7,
                node,
                SemanticIssueKind::MissingDereference {
                    mechanical_fix: "write `deref(.)`",
                },
            );
        }
        // [SET-1] a commit whose target is a complete binding reinitializes
        // that binding, so a dead one is the one root this formation admits;
        // every projected, dereferenced or subscripted target of a dead root
        // stays [OWN-1]'s rejection, because reinitializing one component of a
        // dead root would leave the rest uninitialized.
        if !local.live && !suffixes.is_empty() {
            return self.issue_node(
                SemanticRule::Own1,
                node,
                SemanticIssueKind::UseAfterMove {
                    mechanical_fix: "introduce a new `let` binding before reuse",
                },
            );
        }

        // [TYPE-2] a path that ends at or passes through a readonly field is
        // never a write target, and [TYPE-10] a window part is never a
        // written name at all. Both are decided against the type of the place
        // each suffix follows: x1 reserves neither vocabulary from a
        // declaration, so a source struct's own field spelled `len` or `next`
        // is an ordinary target.
        self.reject_reserved_write_members(node, &suffixes, local.ty)?;
        // [TYPE-9] a target below a `Box`'s member `inner` writes the box
        // content, which is a dereference step the field walk cannot take;
        // the explicit-place target resolver takes it for a bare IDENT base
        // exactly as it does for a written `deref` chain.
        if self.place_path_reaches_box_content(&suffixes, local.ty)? {
            return self.check_dereferenced_set_target(function, node, bindings);
        }
        let (fields, ty) = self.resolve_struct_path(&suffixes, local.ty)?;
        if local.mode != CheckedMode::Own {
            return self.issue_node(
                SemanticRule::Set1,
                node,
                SemanticIssueKind::InvalidSetTarget {
                    root_class: "a reference, which names a path rather than storage".to_owned(),
                    required_classes: SET1_WRITABLE_ROOTS,
                },
            );
        }
        let resolved = ResolvedPlace::spelled(
            crate::semantic::places::PlaceRoot::Binding(local.binding),
            false,
            fields.clone(),
        );
        self.check_mutation_target_class(node, ty)?;
        let mut effects = EffectSet::NONE;
        for path in self.effect_paths_for_place(node, &resolved, bindings)? {
            effects.add_write(path);
        }

        Ok(MutationTarget {
            declaration,
            place: ResolvedPlaceSet::one(resolved),
            through_reference: None,
            element: false,
            target: CheckedSetTarget::Place(CheckedWritablePlace {
                binding: local.binding,
                fields,
                mode: local.mode,
                ty,
                declares: false,
            }),
            effects,
            unsupported: None,
        })
    }

    /// [WIN-3] the final selected type's class judgment at a `set` target.
    ///
    /// Assigning over any owned place releases the old value when it is
    /// affine and is a hard error when it is linear: a linear value has no
    /// release, so the writer takes it out and consumes it first. v0.59's
    /// copy-only demand and its region-free companion were [SET-2]'s and went
    /// with `replace`.
    fn check_mutation_target_class(&self, node: NodeId, ty: CheckedType) -> Result<(), CheckStop> {
        if matches!(
            self.linearity_class(ty)?,
            super::linearity::LinearityClass::Linear
        ) {
            return self.issue_node(
                SemanticRule::Win3,
                node,
                SemanticIssueKind::LinearAssignmentTarget {
                    target_type: self.checked_type_name(ty)?,
                    mechanical_fix: WIN3_LINEAR_TARGET,
                },
            );
        }
        Ok(())
    }

    /// One value's exact semantic mode and type, as `own u64`, `&Counter`,
    /// or `&[u8]`, using [GRAM-3]'s mode/type notation for diagnostics.
    pub(in crate::semantic::check) fn checked_value_name(
        &self,
        mode: CheckedMode,
        ty: CheckedType,
    ) -> Result<String, CheckStop> {
        // [TYPE-8, REF-4] `&[T]` is one reference kind written around its
        // element type, not a reference to one element: what the checked
        // value carries is the element type, so rendering the mode and the
        // type apart would name `&T` where the source wrote the range.
        if mode == CheckedMode::Range {
            return Ok(format!("&[{}]", self.checked_type_name(ty)?));
        }
        let mode = self.checked_mode_name(mode)?;
        let ty = self.checked_type_name(ty)?;
        // [FORM-2] attaches `&` to what follows it, so the rendering must not
        // insert a separator the written form does not have.
        Ok(if mode.ends_with('&') {
            format!("{mode}{ty}")
        } else {
            format!("{mode} {ty}")
        })
    }

    /// One checked mode's diagnostic label [GRAM-3].
    ///
    /// `own` names value mode without being a source annotation. Both
    /// reference kinds use `&`; `checked_value_name` renders the range
    /// brackets together with its element type [REF-1, REF-4].
    pub(in crate::semantic::check) fn checked_mode_name(
        &self,
        mode: CheckedMode,
    ) -> Result<String, CheckStop> {
        Ok(match mode {
            CheckedMode::Own => "own".to_owned(),
            CheckedMode::Reference | CheckedMode::Range => "&".to_owned(),
        })
    }

    /// One region as the source spells it, or its dense identity when the
    /// declaration is not reachable.
    ///
    /// A rendering is presentation: a region a diagnostic cannot name must not
    /// turn a source rejection into a compiler failure. A region the grammar
    /// leaves unwritten has no source spelling at all: resolution mints it
    /// under a name no source token can form, and rendering that name would
    /// name a region the writer cannot write. It renders as the empty string,
    /// which is exactly how the source spells it, and every caller that
    /// splices a region into a longer form drops the separator with it.
    pub(in crate::semantic::check) fn region_spelling(&self, region: DeclarationId) -> String {
        let spelling = self
            .declaration_spelling(region)
            .unwrap_or_else(|_| format!("'region#{}", region.index()));
        if spelling.starts_with("'0_") {
            return String::new();
        }
        spelling
    }

    pub(super) fn checked_type_name(&self, ty: CheckedType) -> Result<String, CheckStop> {
        Ok(match ty {
            CheckedType::Unit => "unit".to_owned(),
            CheckedType::Bool => "Bool".to_owned(),
            CheckedType::Integer(integer) => match integer {
                IntegerType::I8 => "i8",
                IntegerType::I16 => "i16",
                IntegerType::I32 => "i32",
                IntegerType::I64 => "i64",
                IntegerType::U8 => "u8",
                IntegerType::U16 => "u16",
                IntegerType::U32 => "u32",
                IntegerType::U64 => "u64",
            }
            .to_owned(),
            CheckedType::Float(float) => match float {
                FloatType::F32 => "f32",
                FloatType::F64 => "f64",
            }
            .to_owned(),
            // [FN-2] a type parameter is written by its own name, whatever
            // its bound.
            CheckedType::Generic(declaration)
            | CheckedType::GenericInt(declaration)
            | CheckedType::GenericFloat(declaration) => self.declaration_spelling(declaration)?,
            // An instance of a generic declaration writes its type and const
            // arguments after the declared name [GRAM-3, FN-2], and a `Box`
            // its content type [TYPE-9]; the interned nominal name keys the
            // instance and is not a source spelling. [S20] a nominal's region
            // arguments are components of its type name [TYPE-2] and lead
            // that list, so the two sides of a [TYPE-5] mismatch between
            // `BlockPool<'a>` and `BlockPool<'b>` are not the same word twice.
            CheckedType::Nominal(id) => {
                if let CheckedNominalKind::Box { referent, .. } = self.nominal(id)?.kind {
                    return Ok(format!("Box<{}>", self.checked_type_name(referent)?));
                }
                let written = match self.source_nominal_instance_entry(id)? {
                    Some((template, substitution)) if substitution.len() > 0 => {
                        match self.nominal_templates.get(template) {
                            Some(template) => self
                                .generic_argument_spellings(
                                    &template.generic_parameters,
                                    substitution,
                                )?
                                .map(|arguments| (template.name.clone(), arguments)),
                            None => None,
                        }
                    }
                    _ => None,
                };
                let (name, mut arguments) = match written {
                    Some(written) => written,
                    None => (self.nominal(id)?.name.clone(), Vec::new()),
                };
                if let Some(axis) = self.nominal_region_axis(id)? {
                    let regions = axis
                        .iter()
                        .map(|(_, actual)| self.region_spelling(*actual))
                        .filter(|spelling| !spelling.is_empty());
                    arguments = regions.chain(arguments).collect();
                }
                if arguments.is_empty() {
                    name
                } else {
                    format!("{name}<{}>", arguments.join(", "))
                }
            }
            // [TYPE-9]'s own spellings: the constant-capacity placement
            // writes its capacity and the runtime-capacity one writes only
            // its element.
            CheckedType::Array { element, length } => {
                let length = self.checked_const_name(length)?;
                format!(
                    "Array<{}, {length}>",
                    self.checked_type_name(self.element_type(element)?)?
                )
            }
            CheckedType::Buffer { element } => {
                format!(
                    "Array<{}>",
                    self.checked_type_name(self.element_type(element)?)?
                )
            }
            CheckedType::Window {
                shape,
                element,
                capacity,
            } => {
                let element = self.checked_type_name(self.element_type(element)?)?;
                let shape = shape.spelling();
                match capacity {
                    Some(capacity) => {
                        let capacity = self.checked_const_name(capacity)?;
                        format!("{shape}<{element}, {capacity}>")
                    }
                    None => format!("{shape}<{element}>"),
                }
            }
        })
    }

    pub(super) fn checked_const_name(&self, value: CheckedConst) -> Result<String, CheckStop> {
        Ok(match value {
            CheckedConst::Value(value) => value.to_string(),
            CheckedConst::Parameter(declaration) => self.declaration_spelling(declaration)?,
            CheckedConst::Derived(id) => {
                let derived = self.derived_const(id)?;
                format!(
                    "{} {} {}",
                    self.checked_const_name(derived.left)?,
                    derived.operation.spelling(),
                    self.checked_const_name(derived.right)?
                )
            }
        })
    }

    /// One generic instance's written `targ` list [GRAM-3]: each parameter's
    /// argument in declaration order, a type and a const in their canonical
    /// spelling and a function argument as `fn` and the instance it names.
    /// `None` when the substitution does not bind every parameter, which the
    /// caller renders by the declared name alone.
    fn generic_argument_spellings(
        &self,
        parameters: &[super::generics::GenericParameter],
        substitution: &super::generics::GenericSubstitution,
    ) -> Result<Option<Vec<String>>, CheckStop> {
        let mut spellings = Vec::with_capacity(parameters.len());
        for parameter in parameters {
            let key = parameter.key();
            let Some((_, argument)) = substitution
                .entries()
                .iter()
                .find(|(candidate, _)| *candidate == key)
            else {
                return Ok(None);
            };
            spellings.push(match *argument {
                super::generics::GenericArgument::Type(ty) => self.checked_type_name(ty)?,
                super::generics::GenericArgument::Const(value) => self.checked_const_name(value)?,
                super::generics::GenericArgument::Function(function) => {
                    format!("fn {}", self.function_argument_spelling(function)?)
                }
            });
        }
        Ok(Some(spellings))
    }

    /// The callee one function argument names [GRAM-2 `function_arg`]: the
    /// instance it selects when one exists, and otherwise the declaration
    /// or formal parameter it names.
    fn function_argument_spelling(
        &self,
        function: super::behavior::FunctionArgument,
    ) -> Result<String, CheckStop> {
        if let Some(signature) = self
            .function_argument_instance(function)
            .ok()
            .and_then(|instance| self.signatures.get(instance.0 as usize))
        {
            return self.render_function_instance(signature);
        }
        let declaration = match function {
            super::behavior::FunctionArgument::Source { reference, .. } => {
                self.function_reference(reference)?.declaration
            }
            super::behavior::FunctionArgument::Parameter(parameter) => match parameter {
                super::generics::GenericParameterKey::Source(declaration)
                | super::generics::GenericParameterKey::Member {
                    member: declaration,
                    ..
                } => declaration,
            },
        };
        self.declaration_spelling(declaration)
    }

    /// One function instance in the spelling a call writes to select it
    /// [FN-2, GRAM-3]: the declared name, then `::` and the `targ` list of
    /// its substitution. A nongeneric function is its name alone, and so is
    /// an operand-directed [PRE-1] row, whose operand supplies its arguments
    /// and whose call writes none [OP-10]. The instance's internal symbol
    /// keys lowering and is not a source spelling.
    pub(in crate::semantic::check) fn render_function_instance(
        &self,
        signature: &FunctionSignature,
    ) -> Result<String, CheckStop> {
        let Some(template) = self
            .templates_by_declaration
            .get(&signature.declaration)
            .and_then(|index| self.function_templates.get(*index))
        else {
            return Ok(signature.name.clone());
        };
        if template.generic_parameters.is_empty()
            || signature.substitution.len() == 0
            || self.operand_directed_row_index(template)?.is_some()
        {
            return Ok(signature.name.clone());
        }
        Ok(
            match self
                .generic_argument_spellings(&template.generic_parameters, &signature.substitution)?
            {
                Some(arguments) => format!("{}::<{}>", signature.name, arguments.join(", ")),
                None => signature.name.clone(),
            },
        )
    }

    /// Resolves a run of field-selection suffixes over one starting type.
    /// Callers pass the suffix chain to walk — every suffix for a whole
    /// place, or the chain before a subscript for that subscript's base. A
    /// subscript suffix inside the walked run selects through a composite
    /// element value, which this version does not implement.
    pub(super) fn resolve_struct_path(
        &self,
        suffixes: &[NodeId],
        mut ty: CheckedType,
    ) -> Result<(Vec<u32>, CheckedType), CheckStop> {
        let mut fields = Vec::new();
        for &suffix in suffixes {
            if self.subscript_offset(suffix)?.is_some() {
                return self.unsupported(UnsupportedSemanticFeature::CompositeValues, suffix);
            }
            let name = self
                .deferred_use_at(suffix, DeferredUseRole::ProjectedField)?
                .spelling()
                .to_owned();
            // [TYPE-10] a window part is effect-row vocabulary and never a
            // place, so a part spelling following a measured place is that
            // rule's refusal rather than a struct missing a declared field.
            // x1 decides it by the type of the place the suffix follows: on
            // any other type the same spelling is an ordinary field.
            self.reject_window_part(suffix, &name, ty, false)?;
            let name = name.as_str();
            let CheckedType::Nominal(nominal_id) = ty else {
                return self.issue_node(
                    SemanticRule::Type5,
                    suffix,
                    SemanticIssueKind::type_mismatch(
                        "a source struct, whose declared field this suffix selects",
                        self.checked_type_name(ty)?,
                    ),
                );
            };
            let CheckedNominalKind::Struct {
                fields: declared_fields,
            } = &self.nominal(nominal_id)?.kind
            else {
                return self.issue_node(
                    SemanticRule::Type5,
                    suffix,
                    SemanticIssueKind::type_mismatch(
                        "a source struct, whose declared field this suffix selects",
                        self.checked_type_name(ty)?,
                    ),
                );
            };
            let Some((index, field)) = declared_fields
                .iter()
                .enumerate()
                .find(|(_, field)| field.name == name)
            else {
                return self.issue_node(
                    SemanticRule::Type5,
                    suffix,
                    SemanticIssueKind::type_mismatch(
                        format!("a declared field of {}", self.checked_type_name(ty)?),
                        format!("the field name `{name}`, which that struct does not declare"),
                    ),
                );
            };
            self.reject_inaccessible_field(nominal_id, None, index, name, suffix)?;
            fields
                .push(u32::try_from(index).map_err(|_| SemanticCompilerFailure::CounterOverflow)?);
            ty = field.ty;
        }
        Ok((fields, ty))
    }

    pub(super) fn check_expression(
        &self,
        function: &FunctionSignature,
        node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        loop_depth: usize,
    ) -> Result<TypedExpression, CheckStop> {
        self.check_expression_in_context(
            function,
            node,
            bindings,
            loop_depth,
            PlaceUseContext::Ordinary,
        )
    }

    pub(super) fn check_consuming_expression(
        &self,
        function: &FunctionSignature,
        node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        loop_depth: usize,
    ) -> Result<TypedExpression, CheckStop> {
        self.check_expression_in_context(
            function,
            node,
            bindings,
            loop_depth,
            PlaceUseContext::Consuming,
        )
    }

    fn check_expression_in_context(
        &self,
        function: &FunctionSignature,
        node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        loop_depth: usize,
        place_context: PlaceUseContext,
    ) -> Result<TypedExpression, CheckStop> {
        // [GRAM-5] `clause_expr` is the contract-clause shape: one operand,
        // or two operands around one operator token. Its operands are the
        // same three written forms an `expr` selects between, so the two
        // shapes share every judgment below and differ only in where the
        // operator and the second operand hang.
        if self.tree.production(node)? == Production::ClauseExpr {
            return self.check_clause_expression(
                function,
                node,
                bindings,
                loop_depth,
                place_context,
            );
        }
        // [GRAM-5] `expr := atom infix_tail? | call | construct`, so the only
        // shape with more than one child is the infix one.
        if let Some(tail) = self.tree.first_child_with(node, Production::InfixTail)? {
            return self.check_infix(function, node, tail, bindings, loop_depth);
        }
        let child = self.tree.only_child(node)?;
        self.check_written_operand(function, child, bindings, loop_depth, place_context)
    }

    /// [GRAM-5] one `clause_expr`: one `affine_expr`, or two around one
    /// `clause_op`. Each side is [GRAM-4]'s own affine expression, whose
    /// factors may be a `call` and which therefore admits a measure term
    /// displaced by an affine expression on either side of the operator
    /// [MSR-5].
    fn check_clause_expression(
        &self,
        function: &FunctionSignature,
        node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        loop_depth: usize,
        place_context: PlaceUseContext,
    ) -> Result<TypedExpression, CheckStop> {
        match self.tree.children(node)? {
            [side] => {
                let side = *side;
                self.check_clause_affine(function, side, None, bindings, loop_depth, place_context)
            }
            [left, operator, right] => {
                let (left, operator, right) = (*left, *operator, *right);
                let operation = self.infix_operation(self.clause_operator_node(operator)?)?;
                let left = (
                    left,
                    self.check_clause_affine(
                        function,
                        left,
                        None,
                        bindings,
                        loop_depth,
                        PlaceUseContext::Ordinary,
                    )?,
                );
                let right = (
                    right,
                    self.check_clause_affine(
                        function,
                        right,
                        None,
                        bindings,
                        loop_depth,
                        PlaceUseContext::Ordinary,
                    )?,
                );
                self.check_integer_operation_operands(node, operation, vec![left, right])
            }
            _ => Err(SemanticCompilerFailure::InvalidCanonicalTree.into()),
        }
    }

    /// The operator token's owning node inside one `clause_op` [GRAM-5]: the
    /// `compare_op` node it selected, or the `clause_op` itself when the
    /// operator is one of the five infix `defined` domain queries.
    pub(super) fn clause_operator_node(&self, operator: NodeId) -> Result<NodeId, CheckStop> {
        Ok(self
            .tree
            .first_child_with(operator, Production::CompareOp)?
            .unwrap_or(operator))
    }

    /// One `affine_expr`, `affine_term`, or `affine_factor` of a contract
    /// clause [MSR-5].
    ///
    /// `terms` bounds an `affine_expr`'s left-associative fold to its first
    /// `terms` `affine_term` children, so `a + b - c` is `(a + b) - c` with
    /// no rewriting of the source tree. Its `+`, `-`, and `*` denote the
    /// mathematical integer expression [INV-1] fixes; the [OP-1] rows named
    /// here are the exact ones, which carry no domain obligation of their own
    /// because a clause is never evaluated.
    fn check_clause_affine(
        &self,
        function: &FunctionSignature,
        node: NodeId,
        terms: Option<usize>,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        loop_depth: usize,
        place_context: PlaceUseContext,
    ) -> Result<TypedExpression, CheckStop> {
        match self.tree.production(node)? {
            Production::AffineExpr => {
                let children = self.tree.children(node)?.to_vec();
                let count = terms.unwrap_or_else(|| children.len().div_ceil(2));
                let last = count
                    .checked_mul(2)
                    .and_then(|doubled| doubled.checked_sub(2))
                    .and_then(|index| children.get(index).copied())
                    .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
                if count == 1 {
                    return self.check_clause_affine(
                        function,
                        last,
                        None,
                        bindings,
                        loop_depth,
                        place_context,
                    );
                }
                let operator = children
                    .get(
                        count
                            .checked_mul(2)
                            .and_then(|doubled| doubled.checked_sub(3))
                            .ok_or(SemanticCompilerFailure::CounterOverflow)?,
                    )
                    .copied()
                    .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
                let operation = self.affine_add_operation(operator)?;
                let left = (
                    node,
                    self.check_clause_affine(
                        function,
                        node,
                        Some(count - 1),
                        bindings,
                        loop_depth,
                        PlaceUseContext::Ordinary,
                    )?,
                );
                let right = (
                    last,
                    self.check_clause_affine(
                        function,
                        last,
                        None,
                        bindings,
                        loop_depth,
                        PlaceUseContext::Ordinary,
                    )?,
                );
                self.check_integer_operation_operands(node, operation, vec![left, right])
            }
            Production::AffineTerm => {
                let factors = self.tree.children_with(node, Production::AffineFactor)?;
                match factors.as_slice() {
                    [factor] => self.check_clause_affine(
                        function,
                        *factor,
                        None,
                        bindings,
                        loop_depth,
                        place_context,
                    ),
                    [left_node, right_node] => {
                        let left = (
                            *left_node,
                            self.check_clause_affine(
                                function,
                                *left_node,
                                None,
                                bindings,
                                loop_depth,
                                PlaceUseContext::Ordinary,
                            )?,
                        );
                        let right = (
                            *right_node,
                            self.check_clause_affine(
                                function,
                                *right_node,
                                None,
                                bindings,
                                loop_depth,
                                PlaceUseContext::Ordinary,
                            )?,
                        );
                        self.check_integer_operation_operands(
                            node,
                            CheckedIntegerOperation::MultiplyExact,
                            vec![left, right],
                        )
                    }
                    _ => Err(SemanticCompilerFailure::InvalidCanonicalTree.into()),
                }
            }
            Production::AffineFactor => {
                let child = self.tree.only_child(node)?;
                self.check_clause_affine(function, child, None, bindings, loop_depth, place_context)
            }
            _ => self.check_written_operand(function, node, bindings, loop_depth, place_context),
        }
    }

    /// The [OP-1] row one `affine_add_op` names [GRAM-4].
    fn affine_add_operation(&self, operator: NodeId) -> Result<CheckedIntegerOperation, CheckStop> {
        let [terminal] = self.tree.direct_token_indices(operator)? else {
            return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
        };
        Ok(match self.tree.token_bytes(*terminal)? {
            b"+" => CheckedIntegerOperation::AddExact,
            b"-" => CheckedIntegerOperation::SubtractExact,
            _ => return Err(SemanticCompilerFailure::InvalidCanonicalTree.into()),
        })
    }

    /// One written operand of an `expr` or a `clause_expr` [GRAM-5]: the
    /// `atom`, `call`, or `construct` the grammar selected.
    pub(in crate::semantic::check) fn check_written_operand(
        &self,
        function: &FunctionSignature,
        node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        loop_depth: usize,
        place_context: PlaceUseContext,
    ) -> Result<TypedExpression, CheckStop> {
        match self.tree.production(node)? {
            Production::Atom => {
                self.check_atom_in_context(function, node, bindings, loop_depth, place_context)
            }
            Production::Call if self.tree.is_constructor_call(node)? => {
                self.check_construct(function, node, bindings, loop_depth)
            }
            Production::Call => self.check_call(function, node, bindings, loop_depth),
            _ => Err(SemanticCompilerFailure::InvalidCanonicalTree.into()),
        }
    }

    /// [OP-1] (ii) infix resolution: the operator token selects the row.
    ///
    /// [GRAM-9] admits exactly one operation per expression, so there is no
    /// precedence to apply — the left operand is the `expr`'s own atom and
    /// the right is the tail's. The row then takes the same judgment the
    /// named spelling takes.
    fn check_infix(
        &self,
        function: &FunctionSignature,
        node: NodeId,
        tail: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        loop_depth: usize,
    ) -> Result<TypedExpression, CheckStop> {
        let left = self
            .tree
            .first_child_with(node, Production::Atom)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let operator = self.infix_operator_node(tail)?;
        let right = self
            .tree
            .first_child_with(tail, Production::Atom)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let operation = self.infix_operation(operator)?;
        self.check_integer_operation_row(
            node,
            operation,
            &[left, right],
            function,
            bindings,
            loop_depth,
        )
    }

    /// The operator child of an `infix_tail`: its `infix_op` or its
    /// `compare_op` node, whichever the tail selected [GRAM-5].
    pub(super) fn infix_operator_node(&self, tail: NodeId) -> Result<NodeId, CheckStop> {
        if let Some(operator) = self.tree.first_child_with(tail, Production::InfixOp)? {
            return Ok(operator);
        }
        self.tree
            .first_child_with(tail, Production::CompareOp)?
            .ok_or_else(|| SemanticCompilerFailure::InvalidCanonicalTree.into())
    }

    /// [OP-1] the exact operator token, and the row it spells.
    ///
    /// Bare `+ - * / %` are proof-required exact rows; `defined` names their
    /// total Bool domain queries. The remaining suffixes keep their existing
    /// value-result policies. The six `compare_op` spellings are the total
    /// integer comparison rows.
    pub(super) fn infix_operation(
        &self,
        operator: NodeId,
    ) -> Result<CheckedIntegerOperation, CheckStop> {
        let [terminal] = self.tree.direct_token_indices(operator)? else {
            return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
        };
        Ok(match self.tree.token_bytes(*terminal)? {
            b"+" => CheckedIntegerOperation::AddExact,
            b"+defined" => CheckedIntegerOperation::AddDefined,
            b"+wrap" => CheckedIntegerOperation::AddWrap,
            b"+checked" => CheckedIntegerOperation::AddChecked,
            b"+sat" => CheckedIntegerOperation::AddSaturating,
            b"-" => CheckedIntegerOperation::SubtractExact,
            b"-defined" => CheckedIntegerOperation::SubtractDefined,
            b"-wrap" => CheckedIntegerOperation::SubtractWrap,
            b"-checked" => CheckedIntegerOperation::SubtractChecked,
            b"-sat" => CheckedIntegerOperation::SubtractSaturating,
            b"*" => CheckedIntegerOperation::MultiplyExact,
            b"*defined" => CheckedIntegerOperation::MultiplyDefined,
            b"*wrap" => CheckedIntegerOperation::MultiplyWrap,
            b"*checked" => CheckedIntegerOperation::MultiplyChecked,
            b"*sat" => CheckedIntegerOperation::MultiplySaturating,
            b"/" => CheckedIntegerOperation::DivideExact,
            b"/defined" => CheckedIntegerOperation::DivideDefined,
            b"/checked" => CheckedIntegerOperation::DivideChecked,
            b"%" => CheckedIntegerOperation::RemainderExact,
            b"%defined" => CheckedIntegerOperation::RemainderDefined,
            b"%checked" => CheckedIntegerOperation::RemainderChecked,
            b"==" => CheckedIntegerOperation::Equal,
            b"!=" => CheckedIntegerOperation::NotEqual,
            b"<" => CheckedIntegerOperation::Less,
            b"<=" => CheckedIntegerOperation::LessEqual,
            b">" => CheckedIntegerOperation::Greater,
            b">=" => CheckedIntegerOperation::GreaterEqual,
            _ => return Err(SemanticCompilerFailure::InvalidCanonicalTree.into()),
        })
    }

    pub(super) fn check_atom(
        &self,
        function: &FunctionSignature,
        node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        loop_depth: usize,
    ) -> Result<TypedExpression, CheckStop> {
        self.check_atom_in_context(
            function,
            node,
            bindings,
            loop_depth,
            PlaceUseContext::Ordinary,
        )
    }

    /// Checks an atom in a position whose owning rule decides whether the
    /// selected value is admissible. This delays OWN-1's bare-affine spelling
    /// rejection long enough for an earlier TYPE-7 implicit-read judgment to
    /// take exclusive ownership of a holder used for its referent.
    pub(super) fn check_consuming_atom(
        &self,
        function: &FunctionSignature,
        node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        loop_depth: usize,
    ) -> Result<TypedExpression, CheckStop> {
        self.check_atom_in_context(
            function,
            node,
            bindings,
            loop_depth,
            PlaceUseContext::Consuming,
        )
    }

    pub(super) fn check_call_argument_atom(
        &self,
        function: &FunctionSignature,
        node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        loop_depth: usize,
    ) -> Result<TypedExpression, CheckStop> {
        self.check_atom_in_context(
            function,
            node,
            bindings,
            loop_depth,
            PlaceUseContext::Ordinary,
        )
    }

    fn check_atom_in_context(
        &self,
        function: &FunctionSignature,
        node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        loop_depth: usize,
        place_context: PlaceUseContext,
    ) -> Result<TypedExpression, CheckStop> {
        if let Some(value) = self.postcondition_result_placeholder(node)? {
            return Ok(TypedExpression::owned(
                CheckedExpression::Constant(value),
                EffectSet::NONE,
            ));
        }
        if let Some(literal) = self
            .tree
            .direct_token_with(node, TerminalPredicate::Literal)?
        {
            let bytes = self.tree.token_bytes(literal)?;
            if matches!(bytes, b"0_T" | b"1_T") {
                return self.check_generic_numeric_identity(function, node, bytes == b"1_T");
            }
            return Ok(TypedExpression::owned(
                CheckedExpression::Constant(self.parse_literal(node, bytes)?),
                EffectSet::NONE,
            ));
        }
        if let Some(place) = self.tree.first_child_with(node, Production::Place)? {
            let value = self.check_place_use(
                function,
                node,
                place,
                bindings,
                PlaceUseOptions {
                    explicit_move: self.has_fixed(node, FixedTerminal::Move)?,
                    context: place_context,
                    loop_depth,
                },
            )?;
            return Ok(value);
        }
        if let Some(borrow) = self.tree.first_child_with(node, Production::BorrowExpr)? {
            return self.check_borrow(borrow, function, bindings, loop_depth);
        }
        Err(SemanticCompilerFailure::InvalidCanonicalTree.into())
    }

    /// The `borrow_expr` that is the complete written content of `expression`,
    /// if any: the position [OWN-14] names for the returned reborrow.
    ///
    /// An infix expression is a fresh operation result rather than a written
    /// borrow, so it answers `None` like any other non-borrow shape.
    pub(super) fn complete_borrow_expression(
        &self,
        expression: NodeId,
    ) -> Result<Option<NodeId>, CheckStop> {
        let Some(child) = self.tree.sole_expression_child(expression)? else {
            return Ok(None);
        };
        if self.tree.production(child)? != Production::Atom {
            return Ok(None);
        }
        Ok(self.tree.first_child_with(child, Production::BorrowExpr)?)
    }

    fn check_generic_numeric_identity(
        &self,
        function: &FunctionSignature,
        node: NodeId,
        one: bool,
    ) -> Result<TypedExpression, CheckStop> {
        let usage = self.use_at(node, LexicalUseRole::GenericNumericSuffix)?;
        let ResolvedTarget::Source {
            declaration,
            class: DeclarationClass::GenericType,
        } = usage.target()
        else {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        };
        let ty = function
            .substitution
            .type_argument(declaration)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let value = match ty {
            CheckedType::Integer(ty) => CheckedValue::Integer {
                ty,
                bits: u64::from(one),
            },
            CheckedType::Float(FloatType::F32) => CheckedValue::Float {
                ty: FloatType::F32,
                bits: if one { 0x3f80_0000 } else { 0 },
            },
            CheckedType::Float(FloatType::F64) => CheckedValue::Float {
                ty: FloatType::F64,
                bits: if one { 0x3ff0_0000_0000_0000 } else { 0 },
            },
            CheckedType::GenericInt(_) | CheckedType::GenericFloat(_) => {
                CheckedValue::NumericIdentity { ty, one }
            }
            _ => {
                return self.issue_node(
                    SemanticRule::Form5,
                    node,
                    SemanticIssueKind::type_mismatch(
                        "an integer or float type, whose 0 and 1 this form names",
                        self.checked_type_name(ty)?,
                    ),
                );
            }
        };
        Ok(TypedExpression::owned(
            CheckedExpression::Constant(value),
            EffectSet::NONE,
        ))
    }

    fn check_place_use(
        &self,
        function: &FunctionSignature,
        use_node: NodeId,
        node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        options: PlaceUseOptions,
    ) -> Result<TypedExpression, CheckStop> {
        let pbase = self
            .tree
            .first_child_with(node, Production::Pbase)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let suffixes = self.tree.children_with(node, Production::Psuffix)?;
        if !suffixes.is_empty()
            && !self.has_fixed(pbase, FixedTerminal::Deref)?
            && self.tree.children(pbase)?.is_empty()
            && let ResolvedTarget::Source {
                declaration,
                class: DeclarationClass::NamedConst,
            } = self.use_at(pbase, LexicalUseRole::PlaceBase)?.target()
        {
            let constant = *self
                .constants
                .get(&declaration)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            return self.check_constant_storage_read(
                use_node, constant, &suffixes, bindings, function, options,
            );
        }
        // [OP-15, MSR-1] a measure is read over the place written before it,
        // and [ENT-2] clause (b) forms that place with subscripts as well as
        // field selections: `rows[0_u64].len` is the measure of the element
        // the subscript selects and never a field of it. The subscript inside
        // the place keeps its ordinary [OP-4] obligation.
        if let Some(measure) = self.trailing_measure_member(&suffixes)?
            && let Some(subscript) =
                self.indexing_subscript(node, &suffixes[..suffixes.len() - 1], bindings)?
        {
            return self.check_indexed_measure_use(
                function, use_node, node, &suffixes, subscript, measure, bindings, options,
            );
        }
        if let Some(subscript) = self.indexing_subscript(node, &suffixes, bindings)? {
            return self.check_index_use(
                function, use_node, node, &suffixes, subscript, bindings, options,
            );
        }
        // [OP-15] a measure is read as a member of the measured place, so
        // `a.len` is a place form and not a call. The written base decides
        // nothing about that read: the explicit-place walker resolves a bare
        // IDENT base exactly as it resolves a `deref` chain, so routing every
        // measure member there keeps one implementation of [MSR-1]'s rows,
        // [MSR-2]'s descriptor-only support and [EFF-2]'s attribution.
        if self.has_fixed(pbase, FixedTerminal::Deref)?
            || self.trailing_measure_member(&suffixes)?.is_some()
        {
            return self.check_dereferenced_place_use(use_node, node, pbase, bindings, options);
        }
        if !self.tree.children(pbase)?.is_empty() {
            return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
        }
        let usage = self.use_at(pbase, LexicalUseRole::PlaceBase)?;
        let ResolvedTarget::Source { declaration, class } = usage.target() else {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        };
        match class {
            DeclarationClass::Value => {
                let local = bindings
                    .get(&declaration)
                    .cloned()
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                if !local.live {
                    return self.issue_node(
                        SemanticRule::Own1,
                        use_node,
                        SemanticIssueKind::UseAfterMove {
                            mechanical_fix: "introduce a new `let` binding before reuse",
                        },
                    );
                }
                // [REF-1, TYPE-7] a bare reference variable denotes the
                // reference itself, and the storage it names is reached only
                // through `deref`, so a suffix chain written directly on one
                // is the [TYPE-7] missing-dereference rejection. The bare
                // read is a copy of a name, never a consume: a reference owns
                // no storage, so `move p` on one is [OWN-1]'s copy spelling.
                if local.mode.is_reference() {
                    if !suffixes.is_empty() {
                        return self.issue_node(
                            SemanticRule::Type7,
                            use_node,
                            SemanticIssueKind::MissingDereference {
                                mechanical_fix: "write `deref(p)`",
                            },
                        );
                    }
                    if options.explicit_move {
                        return self.issue_node(
                            SemanticRule::Own1,
                            use_node,
                            SemanticIssueKind::MoveOfCopy {
                                mechanical_fix: "use the copy place without `move`",
                            },
                        );
                    }
                    self.check_reference_valid(&local, use_node)?;
                    return Ok(TypedExpression {
                        expression: CheckedExpression::Binding {
                            carrier: self.tree.path(use_node)?.clone(),
                            binding: local.binding,
                            ty: local.ty,
                            consume_root: false,
                        },
                        mode: local.mode,
                        reference: local.reference.clone(),
                        // A bare reference variable selects the reference,
                        // not the place it names [TYPE-7, REF-1].
                        reference_value: true,
                        effects: EffectSet::NONE,
                        accesses: Vec::new(),
                    });
                }
                // [TYPE-9] a `Box`'s content is its member `inner`, reached
                // by the ordinary member step and never by `deref`. The step
                // below that member is a dereference, which the field walk
                // has no step for, so the explicit-place walker resolves the
                // whole place; it takes a bare IDENT base exactly as it takes
                // a `deref` chain, which keeps one implementation of the box
                // content step for both spellings.
                if self.place_path_reaches_box_content(&suffixes, local.ty)? {
                    return self
                        .check_dereferenced_place_use(use_node, node, pbase, bindings, options);
                }
                let (fields, ty) = self.resolve_struct_path(&suffixes, local.ty)?;
                let copy = self.is_copy_type(ty)?;
                if options.explicit_move && copy && self.judges_class_spelling() {
                    return self.issue_node(
                        SemanticRule::Own1,
                        use_node,
                        SemanticIssueKind::MoveOfCopy {
                            mechanical_fix: "use the copy place without `move`",
                        },
                    );
                }
                if !copy
                    && !options.explicit_move
                    && matches!(options.context, PlaceUseContext::Ordinary)
                {
                    return self.issue_node(
                        SemanticRule::Own1,
                        use_node,
                        SemanticIssueKind::BareAffineUse {
                            mechanical_fix: "write `move p` for the affine place",
                        },
                    );
                }
                // [SET-1] a `move` of a target place of this statement's
                // commit, or of a place reached through one, is that target's
                // read-out: the previous value leaves, the root stays live,
                // and the same statement reinitializes the target. It is not
                // [OWN-1]'s root-killing consume and derives no residual
                // cleanup of the root's unselected content.
                self.check_commit_place_live(
                    &ResolvedPlace::fields(local.binding, fields.clone()),
                    use_node,
                    false,
                )?;
                let read_out = !copy
                    && options.explicit_move
                    && self.take_commit_read_out(&ResolvedPlace::fields(
                        local.binding,
                        fields.clone(),
                    ));
                // OWN-1 makes an affine projection consume its whole root.
                // Its residual cleanup destroys every unselected resource
                // field, so the loan access is the root rather than only the
                // selected projection. A read-out consumes exactly its own
                // place, so its access is that place.
                let access_fields = if copy || read_out {
                    fields.clone()
                } else {
                    Vec::new()
                };
                let access_kind = if copy {
                    AccessKind::Read
                } else {
                    AccessKind::Move
                };
                // [PROV-6] a whole-owner consume may leave only droppable
                // residual parts; the selected field is moved, not released.
                if !copy && !read_out && !fields.is_empty() {
                    self.reject_partial_consume(local.ty, &fields, use_node)?;
                }
                let residual_drops = if copy || read_out || fields.is_empty() {
                    Vec::new()
                } else {
                    let paths = self.residual_drop_paths(local.ty, &fields)?;
                    paths
                        .into_iter()
                        .map(|(fields, ty)| CheckedProjectedDrop { fields, ty })
                        .collect()
                };
                // [SET-1] after its read-out the target is dead for the
                // remainder of the right-hand side, and the commit reinitializes
                // it. At a complete binding that is exactly this binding's own
                // liveness, so the ordinary kill stands and the commit revives
                // it; at a projection the root keeps its other content and only
                // the target place is spent, which the commit's own read-out
                // record carries.
                if !copy && (!read_out || fields.is_empty()) {
                    bindings
                        .get_mut(&declaration)
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?
                        .live = false;
                }
                // [REF-2] a consume is one of the three invalidating actions:
                // a reference whose path is this place or has this place as a
                // prefix names storage the move has carried away, and
                // [REF-2] says a move never re-roots an existing reference.
                if !copy && !read_out {
                    self.invalidate_references(
                        bindings,
                        &ResolvedPlace::fields(local.binding, fields.clone()),
                        &super::references::InvalidationEvent::PrefixMoved,
                    )?;
                }
                let access = ResolvedPlace::fields(local.binding, access_fields);
                let mut effects = EffectSet::NONE;
                // [SET-1, EFF-2] a read-out reads the target's own storage,
                // exactly as [SET-2]'s exchange does, and the commit writes it.
                //
                // [EFF-1] a loan-bearing value's effect path names the viewed
                // backing state and not the descriptor, and merely moving,
                // returning or structurally repacking that value observes
                // none of it: a read *through* the view is the subscript's own
                // attribution. Before [S27] made the shared view copy this
                // guard was invisible, because a consume exhibited no read at
                // all; the copy spelling is what would otherwise have made
                // `return value;` declare a read of storage it never touches.
                if matches!(access_kind, AccessKind::Read) || read_out {
                    for path in self.effect_paths_for_place(use_node, &access, bindings)? {
                        effects.add_read(path);
                    }
                }
                if fields.is_empty() {
                    Ok(TypedExpression::owned_with_access(
                        CheckedExpression::Binding {
                            carrier: self.tree.path(use_node)?.clone(),
                            binding: local.binding,
                            ty,
                            consume_root: !copy,
                        },
                        effects,
                        access,
                    ))
                } else {
                    Ok(TypedExpression::owned_with_access(
                        CheckedExpression::Project {
                            carrier: self.tree.path(use_node)?.clone(),
                            binding: local.binding,
                            fields,
                            ty,
                            consume_root: !copy && !read_out,
                            residual_drops,
                        },
                        effects,
                        access,
                    ))
                }
            }
            // [MSR-6] an in-scope const generic is a value wherever a named
            // const is. It is one `pbase` with no suffix and no `deref`, its
            // exact type is the `gparam`'s written integer type, and reading
            // it performs no operation and has the empty effect row.
            DeclarationClass::ConstGeneric => {
                if options.explicit_move {
                    return self.issue_node(
                        SemanticRule::Own1,
                        use_node,
                        SemanticIssueKind::MoveOfCopy {
                            mechanical_fix: "use the copy place without `move`",
                        },
                    );
                }
                if !suffixes.is_empty() {
                    return self.issue_node(
                        SemanticRule::Type5,
                        use_node,
                        SemanticIssueKind::type_mismatch(
                            "a const generic read with no suffix",
                            "a suffix chain on an integer const generic",
                        ),
                    );
                }
                let ty = self.const_generic_type(declaration)?;
                let value = match function.substitution.const_argument(declaration) {
                    Some(CheckedConst::Value(value)) => CheckedValue::Integer { ty, bits: value },
                    // [FN-2, MSR-6] a const parameter this instance's caller
                    // supplied from a const parameter of its own is that
                    // caller's parameter here. Keeping this declaration would
                    // anchor the constant to a parameter nothing outside this
                    // instance can name, and every relation published over it
                    // would be dropped at the call.
                    Some(CheckedConst::Parameter(supplied)) => CheckedValue::ConstGeneric {
                        declaration: supplied,
                        ty,
                    },
                    // The one source-canonical symbolic instance keeps the
                    // declaration-anchored constant [ENT-2] clause (c) fixes.
                    _ => CheckedValue::ConstGeneric { declaration, ty },
                };
                Ok(TypedExpression::owned(
                    CheckedExpression::Constant(value),
                    EffectSet::NONE,
                ))
            }
            DeclarationClass::NamedConst => {
                if options.explicit_move {
                    return self.issue_node(
                        SemanticRule::Own1,
                        use_node,
                        SemanticIssueKind::MoveOfCopy {
                            mechanical_fix: "use the copy place without `move`",
                        },
                    );
                }
                let constant = self
                    .constants
                    .get(&declaration)
                    .copied()
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                let constant = self.constant(constant)?;
                if matches!(
                    constant.ty,
                    CheckedType::Array { .. }
                        | CheckedType::Buffer { .. }
                        | CheckedType::Window { .. }
                ) {
                    return self.issue_node(
                        SemanticRule::Own1,
                        use_node,
                        SemanticIssueKind::BareAffineUse {
                            mechanical_fix: "read a const Array<T, n> through a subscript, or read one of its measures as `p.len`",
                        },
                    );
                }
                if matches!(constant.value, CheckedValue::Struct { .. }) {
                    return self.issue_node(
                        SemanticRule::Own1,
                        use_node,
                        SemanticIssueKind::BareAffineUse {
                            mechanical_fix: "read a const struct through its fields",
                        },
                    );
                }
                Ok(TypedExpression::owned(
                    CheckedExpression::NamedConstant {
                        declaration,
                        value: constant.value.clone(),
                    },
                    EffectSet::NONE,
                ))
            }
            _ => Err(SemanticCompilerFailure::InvalidResolution.into()),
        }
    }

    /// One [SET-1] target written `deref(p)` or a path below one.
    ///
    /// [SET-1] makes such a target writable exactly when `p` is a reference
    /// parameter whose declared row carries `writes` of that path
    /// [EFF-1, EFF-5], or a local reference variable whose named path is
    /// itself writable. `deref` of anything that is not a reference — a
    /// `Box` included, whose content is its field `inner` — is [TYPE-7]'s
    /// rejection, raised by the place resolver.
    fn check_dereferenced_set_target(
        &self,
        function: &FunctionSignature,
        node: NodeId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<MutationTarget, CheckStop> {
        let place = self.resolve_explicit_place(node, node, bindings)?;
        if place
            .resolved
            .members
            .iter()
            .any(|member| matches!(member.root, crate::semantic::places::PlaceRoot::Constant(_)))
        {
            return self.issue_node(
                SemanticRule::Const2,
                node,
                SemanticIssueKind::ImmutableSetTarget,
            );
        }
        for member in &place.resolved.members {
            self.reject_readonly_resolved_write(node, member, bindings)?;
        }
        let local = bindings
            .get(&place.declaration)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        self.check_reference_valid(local, node)?;
        let mut writable = true;
        for member in &place.resolved.members {
            writable &= self.reference_row_writes(function, member, bindings)?;
        }
        if !writable {
            return self.issue_node(
                SemanticRule::Set1,
                node,
                SemanticIssueKind::InvalidSetTarget {
                    root_class: "a reference whose declared row does not write this path"
                        .to_owned(),
                    required_classes: SET1_WRITABLE_ROOTS,
                },
            );
        }
        self.check_mutation_target_class(node, place.ty)?;
        let mut effects = EffectSet::NONE;
        for member in &place.resolved.members {
            for path in self.effect_paths_for_place(node, member, bindings)? {
                effects.add_write(path);
            }
        }
        let (binding, path) = self.explicit_container_path(&place.expression, node)?;
        Ok(MutationTarget {
            declaration: place.declaration,
            place: place.resolved,
            through_reference: Some(place.declaration),
            element: false,
            target: CheckedSetTarget::Storage(super::super::model::CheckedContainerRoot {
                root: crate::semantic::places::PlaceRoot::Binding(binding),
                path,
                ty: place.ty,
            }),
            effects,
            unsupported: None,
        })
    }

    /// [SET-1, EFF-1] whether this target is writable through the reference
    /// it is reached by.
    ///
    /// A reference *variable* names a path of this body, which the resolver
    /// has already replaced by the path itself, so the question is asked of
    /// the resolved root: a live own-mode local root is writable on its own
    /// [SET-1], and a reference-parameter root needs this callable's declared
    /// row to carry `writes` of the path, which is the same fact [EFF-5]
    /// substitutes at every call.
    fn reference_row_writes(
        &self,
        function: &FunctionSignature,
        place: &ResolvedPlace,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<bool, CheckStop> {
        let crate::semantic::places::PlaceRoot::Binding(binding) = place.root else {
            // A named const is immutable static storage [CONST-2].
            return Ok(false);
        };
        let Some(local) = bindings.values().find(|local| local.binding == binding) else {
            return Ok(false);
        };
        if !local.mode.is_reference() {
            // [SET-1] a reference does not make a counted binder writable.
            return Ok(local.live && !local.compiler_updated);
        }
        let Some(target) = self.state_path(place, bindings)? else {
            return Ok(false);
        };
        Ok(function.declared_effects.writes.iter().any(|declared| {
            declared.root == target.root
                && declared.steps.len() <= target.steps.len()
                && declared
                    .steps
                    .iter()
                    .zip(&target.steps)
                    .all(|(left, right)| left == right)
        }))
    }

    pub(super) fn check_match_expression(
        &self,
        function: &FunctionSignature,
        node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        loop_depth: usize,
    ) -> Result<TypedExpression, CheckStop> {
        self.check_consuming_expression(function, node, bindings, loop_depth)
    }

    /// [PROV-6] the operand of a `dispose` statement or of a destructuring
    /// consume: an ordinary consuming place use, judged by [OWN-1] exactly as
    /// every other consuming position is.
    pub(super) fn check_consumed_place(
        &self,
        function: &FunctionSignature,
        use_node: NodeId,
        place: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        loop_depth: usize,
        explicit_move: bool,
    ) -> Result<TypedExpression, CheckStop> {
        self.check_place_use(
            function,
            use_node,
            place,
            bindings,
            PlaceUseOptions {
                explicit_move,
                context: PlaceUseContext::Consuming,
                loop_depth,
            },
        )
    }

    /// One `construct` of a nominal carrying `region_params` [FORM-8].
    ///
    /// Every other construct forms its instance from the written argument
    /// list and then checks its operands against that instance's fields.
    /// Here the operands come first, because they are what determines the
    /// instance: a field whose declared type names one of the declaration's
    /// region parameters supplies that region from its own actual, exactly as
    /// a parameter position supplies a callee's formal region at a call, and
    /// the position writes only the region parameters no field's declared
    /// type mentions. [TYPE-5]'s ground is untouched — construction still
    /// consults no expected nominal type, and it is the operands and the
    /// written list, never a destination, that fix the instance.
    ///
    /// The instance is formed once the regions are known and every operand is
    /// then compared against *its* declared field types by the ordinary exact
    /// [TYPE-5] equality, so a second operand naming a second store is a
    /// mismatch and not a second binding [PROV-1].
    fn check_instanced_construct(
        &self,
        function: &FunctionSignature,
        node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        loop_depth: usize,
        site: &super::ConstructorSite,
        constructor_name: String,
    ) -> Result<TypedExpression, CheckStop> {
        let written_fields = match self
            .tree
            .first_child_with(node, Production::FieldinitList)?
        {
            Some(list) => self.tree.children_with(list, Production::Fieldinit)?,
            None => Vec::new(),
        };
        if written_fields.len() != site.shape.fields.len() {
            return self.issue_node(
                SemanticRule::Gram8,
                node,
                SemanticIssueKind::InvalidConstructionFields {
                    constructor: constructor_name,
                    declared_fields: site.shape.fields.clone(),
                },
            );
        }
        let mut atoms = Vec::with_capacity(written_fields.len());
        let mut operands = Vec::with_capacity(written_fields.len());
        let mut effects = EffectSet::NONE;
        for (written, declared) in written_fields.into_iter().zip(&site.shape.fields) {
            if self
                .deferred_use_at(written, DeferredUseRole::FieldInitializer)?
                .spelling()
                != *declared
            {
                return self.issue_node(
                    SemanticRule::Gram8,
                    written,
                    SemanticIssueKind::InvalidConstructionFields {
                        constructor: constructor_name,
                        declared_fields: site.shape.fields.clone(),
                    },
                );
            }
            let atom = self
                .tree
                .first_child_with(written, Production::Atom)?
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            let value = self.check_atom(function, atom, bindings, loop_depth)?;
            effects = effects.union(value.effects.clone());
            atoms.push(atom);
            operands.push(value);
        }
        let nominal = self.constructed_nominal(node, site, &[], &function.substitution)?;
        let declared_fields = match (&self.nominal(nominal)?.kind, site.variant) {
            (CheckedNominalKind::Struct { fields }, None) => fields.clone(),
            (CheckedNominalKind::Enum { variants }, Some(variant)) => variants
                .get(variant as usize)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?
                .fields
                .clone(),
            _ => return Err(SemanticCompilerFailure::InvalidResolution.into()),
        };
        let mut fields = Vec::with_capacity(operands.len());
        for ((value, atom), declared) in operands.into_iter().zip(atoms).zip(&declared_fields) {
            if value.expression.ty() != declared.ty {
                return self.issue_node(
                    SemanticRule::Type5,
                    atom,
                    SemanticIssueKind::type_mismatch(
                        self.checked_type_name(declared.ty)?,
                        self.checked_type_name(value.expression.ty())?,
                    ),
                );
            }
            if value.mode != CheckedMode::Own {
                return self.issue_node(
                    SemanticRule::Type7,
                    atom,
                    SemanticIssueKind::MissingDereference {
                        mechanical_fix: "write `deref(holder)`",
                    },
                );
            }
            fields.push(value.expression);
        }
        let carrier = self.tree.path(node)?.clone();
        let expression = match site.variant {
            None => CheckedExpression::ConstructStruct {
                carrier,
                nominal,
                fields,
            },
            Some(variant) => CheckedExpression::ConstructEnum {
                carrier,
                nominal,
                variant,
                fields,
            },
        };
        Ok(TypedExpression::owned(expression, effects))
    }

    pub(super) fn check_construct(
        &self,
        function: &FunctionSignature,
        node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        loop_depth: usize,
    ) -> Result<TypedExpression, CheckStop> {
        let usage = self.use_at(node, LexicalUseRole::Construct)?;
        let constructor_name = usage.spelling().to_owned();
        // x1 [TYPE-2]: the three storage shapes and the cell are all the
        // prelude's opaque structs, and an opaque struct's constructor entry
        // exists to be refused. "A constructor `call` and a destructuring
        // `let_stmt` naming any of the four is refused by [TYPE-2] like every
        // opaque struct's", so the four cite one rule where the shapes used
        // to cite [TYPE-9] and the cell [TYPE-2].
        if let ResolvedTarget::Container(id) = usage.target() {
            let _ =
                crate::container_nominal(id).ok_or(SemanticCompilerFailure::InvalidResolution)?;
            return self.issue_node(
                SemanticRule::Type2,
                node,
                SemanticIssueKind::ContainerConstruction {
                    nominal: constructor_name,
                    mechanical_fix: "build it with a construction function [OP-13]".to_owned(),
                },
            );
        }

        // GRAM-5 factors constructor and qualified-member prefixes through
        // one call node. Constructors still write nominal arguments directly
        // after the TYPEID, and every field remains named [TYPE-5, GRAM-8].
        if self
            .tree
            .first_child_with(node, Production::Targs)?
            .is_some()
        {
            return self.issue_node(
                SemanticRule::Type5,
                node,
                SemanticIssueKind::type_mismatch(
                    "constructor arguments immediately after its TYPEID",
                    "function-style :: arguments",
                ),
            );
        }
        if self
            .tree
            .first_child_with(node, Production::AtomList)?
            .is_some()
        {
            return self.issue_node(
                SemanticRule::Gram8,
                node,
                SemanticIssueKind::type_mismatch(
                    "named constructor fields in declaration order",
                    "positional constructor arguments",
                ),
            );
        }
        if matches!(usage.target(), ResolvedTarget::Prelude(id) if !matches!(id, crate::BuiltinPreludeId::NONE | crate::BuiltinPreludeId::SOME | crate::BuiltinPreludeId::OK | crate::BuiltinPreludeId::ERR))
            && self.tree.argument_list(node)?.is_some()
        {
            return self.issue_node(
                SemanticRule::Type5,
                node,
                SemanticIssueKind::type_mismatch(
                    "a constructor with no nominal arguments",
                    "written nominal arguments",
                ),
            );
        }
        if let ResolvedTarget::Prelude(id) = usage.target()
            && matches!(
                id,
                crate::BuiltinPreludeId::TRUE | crate::BuiltinPreludeId::FALSE
            )
        {
            let value = match id {
                crate::BuiltinPreludeId::TRUE => CheckedValue::Bool(true),
                crate::BuiltinPreludeId::FALSE => CheckedValue::Bool(false),
                _ => return Err(SemanticCompilerFailure::InvalidResolution.into()),
            };
            if self
                .tree
                .first_child_with(node, Production::FieldinitList)?
                .is_some()
            {
                return self.issue_node(
                    SemanticRule::Gram8,
                    node,
                    SemanticIssueKind::InvalidConstructionFields {
                        constructor: constructor_name,
                        declared_fields: Vec::new(),
                    },
                );
            }
            return Ok(TypedExpression::owned(
                CheckedExpression::Constant(value),
                EffectSet::NONE,
            ));
        }
        let constructor = match usage.target() {
            ResolvedTarget::Source { declaration, .. } => {
                // [TYPE-2] an opaque struct has fields and no usable
                // constructor: the entry its declaration contributes exists
                // to be refused, and the refusal is sited at the complete
                // `call`.
                if self.is_opaque_struct_declaration(declaration)? {
                    return self.issue_node(
                        SemanticRule::Type2,
                        node,
                        SemanticIssueKind::ContainerConstruction {
                            nominal: constructor_name,
                            mechanical_fix: super::repairs::opaque_struct_constructed(
                                self.opaque_struct_kind(declaration)?,
                            )
                            .to_owned(),
                        },
                    );
                }
                // [FORM-8] a nominal carrying `region_params` has its region
                // arguments determined by its field operands, so its
                // instance is formed after they are checked and not before.
                if let Some(site) = self.constructor_shape(declaration)? {
                    return self.check_instanced_construct(
                        function,
                        node,
                        bindings,
                        loop_depth,
                        &site,
                        constructor_name,
                    );
                }
                self.source_constructor(node, declaration, &function.substitution)?
            }
            ResolvedTarget::Prelude(id) => match id {
                // [TYPE-5] the prelude generic nominals are constructed
                // through these variant constructors, and they write the
                // nominal's arguments in every position, mandatorily:
                // `None()` has no operand to supply them and construction
                // never consults an expected nominal type [TYPE-6]. The
                // written arguments are read here exactly as
                // `generic_substitution` reads a source generic's, so both
                // classes cite TYPE-5 at the complete `construct`.
                crate::BuiltinPreludeId::NONE | crate::BuiltinPreludeId::SOME => {
                    let value = self.option_type_argument_with(node, &function.substitution)?;
                    Constructor::Enum {
                        nominal: self.prelude_nominal(super::PreludeType::Option(value))?,
                        variant: u32::from(id == crate::BuiltinPreludeId::SOME),
                    }
                }
                crate::BuiltinPreludeId::OK | crate::BuiltinPreludeId::ERR => {
                    let (ok, error) =
                        self.result_type_arguments_with(node, &function.substitution)?;
                    Constructor::Enum {
                        nominal: self.prelude_nominal(super::PreludeType::Result(ok, error))?,
                        variant: u32::from(id == crate::BuiltinPreludeId::ERR),
                    }
                }
                crate::BuiltinPreludeId::OVERFLOW => Constructor::Enum {
                    nominal: self.prelude_nominal(super::PreludeType::Overflow)?,
                    variant: 0,
                },
                crate::BuiltinPreludeId::DIVIDE_BY_ZERO | crate::BuiltinPreludeId::DIV_OVERFLOW => {
                    Constructor::Enum {
                        nominal: self.prelude_nominal(super::PreludeType::DivError)?,
                        variant: u32::from(id == crate::BuiltinPreludeId::DIV_OVERFLOW),
                    }
                }
                crate::BuiltinPreludeId::NARROW_ERROR => Constructor::Enum {
                    nominal: self.prelude_nominal(super::PreludeType::NarrowError)?,
                    variant: 0,
                },
                _ => {
                    return self
                        .unsupported(UnsupportedSemanticFeature::PreludeNominalValues, node);
                }
            },
            _ => return Err(SemanticCompilerFailure::InvalidResolution.into()),
        };
        let declared_fields = match constructor {
            Constructor::Struct(nominal) => match &self.nominal(nominal)?.kind {
                CheckedNominalKind::Struct { fields } => fields.clone(),
                _ => return Err(SemanticCompilerFailure::InvalidResolution.into()),
            },
            Constructor::Enum { nominal, variant } => match &self.nominal(nominal)?.kind {
                CheckedNominalKind::Enum { variants } => variants
                    .get(variant as usize)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?
                    .fields
                    .clone(),
                _ => return Err(SemanticCompilerFailure::InvalidResolution.into()),
            },
        };
        // [MOD-5, TYPE-2] a construction names every field [GRAM-8], so
        // outside the declaring module each must be published, and a
        // readonly field takes no value there.
        let (owner, variant) = match constructor {
            Constructor::Struct(nominal) => (nominal, None),
            Constructor::Enum { nominal, variant } => (nominal, Some(variant as usize)),
        };
        for (index, field) in declared_fields.iter().enumerate() {
            self.reject_inaccessible_field(owner, variant, index, &field.name, node)?;
            if field.readonly && self.field_withholds_writes(owner, field) {
                return self.issue_node(
                    SemanticRule::Mod5,
                    node,
                    SemanticIssueKind::InaccessibleField {
                        field: field.name.clone(),
                        reason: "a readonly field takes its value only from its declaring module, so a construction outside that module is refused; use one of its operations",
                    },
                );
            }
        }
        let written_fields = if let Some(list) = self
            .tree
            .first_child_with(node, Production::FieldinitList)?
        {
            self.tree.children_with(list, Production::Fieldinit)?
        } else {
            Vec::new()
        };
        let declared_field_names = declared_fields
            .iter()
            .map(|field| field.name.clone())
            .collect::<Vec<_>>();
        if written_fields.len() != declared_fields.len() {
            return self.issue_node(
                SemanticRule::Gram8,
                node,
                SemanticIssueKind::InvalidConstructionFields {
                    constructor: constructor_name,
                    declared_fields: declared_field_names,
                },
            );
        }
        let mut fields = Vec::with_capacity(written_fields.len());
        let mut effects = EffectSet::NONE;
        for (written, declared) in written_fields.into_iter().zip(&declared_fields) {
            if self
                .deferred_use_at(written, DeferredUseRole::FieldInitializer)?
                .spelling()
                != declared.name
            {
                return self.issue_node(
                    SemanticRule::Gram8,
                    written,
                    SemanticIssueKind::InvalidConstructionFields {
                        constructor: constructor_name,
                        declared_fields: declared_field_names,
                    },
                );
            }
            let atom = self
                .tree
                .first_child_with(written, Production::Atom)?
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            let value = self.check_atom(function, atom, bindings, loop_depth)?;
            if value.expression.ty() != declared.ty {
                return self.issue_node(
                    SemanticRule::Type5,
                    atom,
                    SemanticIssueKind::type_mismatch(
                        self.checked_type_name(declared.ty)?,
                        self.checked_type_name(value.expression.ty())?,
                    ),
                );
            }
            if value.mode != CheckedMode::Own {
                return self.issue_node(
                    SemanticRule::Type7,
                    atom,
                    SemanticIssueKind::MissingDereference {
                        mechanical_fix: "write `deref(holder)`",
                    },
                );
            }
            effects = effects.union(value.effects);
            fields.push(value.expression);
        }
        let expression = match constructor {
            Constructor::Struct(nominal) => CheckedExpression::ConstructStruct {
                carrier: self.tree.path(node)?.clone(),
                nominal,
                fields,
            },
            Constructor::Enum { nominal, variant } => CheckedExpression::ConstructEnum {
                carrier: self.tree.path(node)?.clone(),
                nominal,
                variant,
                fields,
            },
        };
        Ok(TypedExpression::owned(expression, effects))
    }
}
