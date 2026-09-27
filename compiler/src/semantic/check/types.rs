use std::collections::HashSet;

use crate::syntax::NodeId;
use crate::syntax::terminal::{FixedTerminal, TerminalPredicate};
use crate::{
    BuiltinPreludeId, DeclarationClass, DeclarationRole, LexicalUseRole, Production,
    ResolvedTarget, SemanticCompilerFailure, SemanticIssueKind, SemanticRule,
    UnsupportedSemanticFeature,
};

use super::super::model::{
    CheckedConst, CheckedConstant, CheckedConstantId, CheckedEffectStep, CheckedElement,
    CheckedMeasure, CheckedMode, CheckedNominalKind, CheckedStatePath, CheckedType, CheckedValue,
    ConstOperation, FloatType, IntegerType, WindowShape, evaluate_const_operation,
};
use super::super::places::WindowPart;
use super::floats::parse_float_literal;
use super::generics::GenericSubstitution;
use super::{CheckStop, Checker, EffectSet, ParameterSignature, PreludeType};

/// [TYPE-5]'s two sides where a type spelling that takes no type arguments
/// carries a written `<...>` list.
const TYPE5_NO_TARGS_EXPECTED: &str = "this type spelled with no type arguments";
const TYPE5_NO_TARGS_FOUND: &str = "a written `<...>` type-argument list on a type that takes none";
/// The two spellings that carry `Result`'s type arguments.
///
/// This judgment is reached from a type position and from a variant
/// constructor alike, and a writer meeting it at `Ok(value: v)` sees only a
/// constructor name; naming both spellings is what makes the repair
/// mechanical there.
const RESULT_TARGS_EXPECTED: &str = "Result with both type arguments written: as a type `Result<u64, IoError>`, and as a variant constructor `Ok<u64, IoError>(value: v)`";
/// [STOR-8]'s own restructuring, shared by the type and call halves.
pub(super) const STOR8_NO_HEAP: &str =
    "use a constant-capacity shape, or withdraw the no-heap declaration";

const OPTION_TARGS_EXPECTED: &str = "Option with its type argument written: as a type `Option<u64>`, and as a variant constructor `Some<u64>(value: v)`";
/// [EFF-1]'s five row conditions, each with the repair it admits.
///
/// The rule text carries every one of these sentences; the diagnostic did not,
/// and the blind-writer trial of 2026-08-28 recorded a writer meeting the
/// repeated-category one — `writes(cwd), writes(out)` — with nothing but the
/// rule number to work from. The field-of-non-struct condition is cited at two
/// sites, so five conditions cover six rejections.
const EFF1_CATEGORY_ORDER: &str =
    "a row is written in the canonical order, every `reads` entry before every `writes` entry";
const EFF1_CATEGORY_ORDER_FIX: &str = "write every `reads` entry before the first `writes` entry, and delete each entry whose path is a `writes` entry's path or lies below it, which that `writes` already covers";
const EFF1_REPEATED_PATH: &str =
    "a row lists each path at most once per category, and this entry repeats one";
const EFF1_REPEATED_PATH_FIX: &str = "delete the repeated entry";
const EFF1_NON_PARAMETER_ROOT: &str = "every effect path is rooted at one formal value parameter of the same callable, and this root is not one";
const EFF1_NON_PARAMETER_ROOT_FIX: &str = "root the path at a parameter of this function; a local, a result binder, a region, and an unrelated declaration are never effect roots";
const EFF1_FIELD_OF_NON_STRUCT: &str = "each effect-path suffix must select a field, payload, measure, window part, or indexed position admitted by its prefix type";
const EFF1_FIELD_OF_NON_STRUCT_FIX: &str = "select a member or position admitted by the prefix type, or name the reference parameter's complete state; use .inner for Box contents";
const EFF1_UNKNOWN_FIELD: &str =
    "an effect-path suffix names a member its selected type does not declare";
const EFF1_UNKNOWN_FIELD_FIX: &str =
    "name a declared member of that type, or the reference parameter itself";

/// The kind selected at one point in an effect path.
///
/// A range reference carries its element type in [`ParameterSignature::ty`],
/// so that type alone cannot distinguish the range's own `len` from a field
/// or measure of its element. Keep the distinction through the row walk and
/// discard it exactly when an index selects one element.
#[derive(Clone, Copy)]
pub(super) enum SelectedPlaceType {
    Value(CheckedType),
    Range(CheckedType),
    /// A symbolic PRE-1 window part whose element type awaits its operand.
    /// It carries a path identity but admits no further typed projection.
    UnresolvedWindowElement,
}

impl<'unit> Checker<'unit> {
    /// [TYPE-9] a runtime-capacity shape may appear only as the content of a
    /// `Box` — the type of its `inner` field — and never inline in another
    /// value and never as a local binding.
    ///
    /// The type reader forms the type and never judges the placement, because
    /// one reader serves a parameter, a field, an element and a `Box` content
    /// position alike and only the position knows which of them it is. This
    /// is the refusal those positions make: every written `type` whose value
    /// would be stored inline calls it, and the `Box` referent position does
    /// not.
    pub(super) fn reject_inline_runtime_capacity(
        &self,
        node: NodeId,
        ty: CheckedType,
    ) -> Result<(), CheckStop> {
        // `Array<T>` is the one runtime-capacity form the checker represents
        // today; a runtime-capacity `Slots<T>` and a `Ring` in either
        // placement stop earlier as an unimplemented representation, which is
        // compiler/storage-representation's checker-shapes decision.
        if !matches!(ty, CheckedType::Buffer { .. }) || self.tree.is_prelude_node(node)? {
            return Ok(());
        }
        self.issue_node(
            SemanticRule::Type9,
            node,
            SemanticIssueKind::InlineRuntimeCapacityShape {
                spelling: self.checked_type_name(ty)?,
                mechanical_fix: "wrap it in a Box, or write the constant-capacity form",
            },
        )
    }

    pub(super) fn parse_parameters_with(
        &self,
        function: NodeId,
        substitution: &GenericSubstitution,
    ) -> Result<Vec<ParameterSignature>, CheckStop> {
        let Some(list) = self
            .tree
            .first_child_with(function, Production::ParamList)?
        else {
            return Ok(Vec::new());
        };
        let mut parameters = Vec::new();
        for node in self.tree.children_with(list, Production::Param)? {
            let declaration = self.declaration_at(node, DeclarationRole::Parameter)?;
            let mode = self.parse_parameter_mode(node)?;
            let ty_node = self
                .tree
                .first_child_with(node, Production::Type)?
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            let ty = self.parse_type_with(ty_node, substitution)?;
            // A reference parameter names a path into storage its caller
            // owns, so only the by-value position stores a shape inline.
            if mode == CheckedMode::Own {
                self.reject_inline_runtime_capacity(ty_node, ty)?;
            }
            parameters.push(ParameterSignature {
                declaration: declaration.id(),
                node_path: self.tree.path(node)?.clone(),
                name: declaration.spelling().to_owned(),
                mode,
                ty,
            });
        }
        Ok(parameters)
    }

    pub(super) fn parse_rtype_with(
        &self,
        node: NodeId,
        substitution: &GenericSubstitution,
    ) -> Result<(CheckedMode, CheckedType), CheckStop> {
        // `rtype := type` [GRAM-3]: a result is always owned, because a
        // reference never leaves the function that formed it [REF-3].
        let ty = self
            .tree
            .first_child_with(node, Production::Type)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        Ok((CheckedMode::Own, self.parse_type_with(ty, substitution)?))
    }

    pub(super) fn parse_type(&self, node: NodeId) -> Result<CheckedType, CheckStop> {
        self.parse_type_with(node, &GenericSubstitution::default())
    }

    pub(super) fn parse_type_with(
        &self,
        node: NodeId,
        substitution: &GenericSubstitution,
    ) -> Result<CheckedType, CheckStop> {
        let targs = self.tree.argument_list(node)?;
        if let Some(ty) = self.integer_type(node)? {
            if targs.is_some() {
                return self.issue_node(
                    SemanticRule::Type5,
                    node,
                    SemanticIssueKind::type_mismatch(TYPE5_NO_TARGS_EXPECTED, TYPE5_NO_TARGS_FOUND),
                );
            }
            return Ok(CheckedType::Integer(ty));
        }
        if self.has_fixed(node, FixedTerminal::Unit)? {
            if targs.is_some() {
                return self.issue_node(
                    SemanticRule::Type5,
                    node,
                    SemanticIssueKind::type_mismatch(TYPE5_NO_TARGS_EXPECTED, TYPE5_NO_TARGS_FOUND),
                );
            }
            return Ok(CheckedType::Unit);
        }
        if self.has_fixed(node, FixedTerminal::F32)? {
            return Ok(CheckedType::Float(FloatType::F32));
        }
        if self.has_fixed(node, FixedTerminal::F64)? {
            return Ok(CheckedType::Float(FloatType::F64));
        }
        if self.tree.names_nominal(node)? {
            let usage = self.use_at(node, LexicalUseRole::Type)?;
            match usage.target() {
                ResolvedTarget::Prelude(id) if id == BuiltinPreludeId::BOOL => {
                    if targs.is_some() {
                        return self.issue_node(
                            SemanticRule::Type5,
                            node,
                            SemanticIssueKind::type_mismatch(
                                TYPE5_NO_TARGS_EXPECTED,
                                TYPE5_NO_TARGS_FOUND,
                            ),
                        );
                    }
                    return Ok(CheckedType::Bool);
                }
                ResolvedTarget::Prelude(id) if id == BuiltinPreludeId::OPTION => {
                    let value = self.option_type_argument_with(node, substitution)?;
                    return self
                        .prelude_nominals
                        .get(&PreludeType::Option(value))
                        .copied()
                        .map(CheckedType::Nominal)
                        .ok_or_else(|| SemanticCompilerFailure::InvalidResolution.into());
                }
                ResolvedTarget::Prelude(id) if id == BuiltinPreludeId::RESULT => {
                    let (ok, error) = self.result_type_arguments_with(node, substitution)?;
                    return self
                        .prelude_nominals
                        .get(&PreludeType::Result(ok, error))
                        .copied()
                        .map(CheckedType::Nominal)
                        .ok_or_else(|| SemanticCompilerFailure::InvalidResolution.into());
                }
                ResolvedTarget::Prelude(id)
                    if matches!(
                        id,
                        crate::BuiltinPreludeId::OVERFLOW_TYPE
                            | crate::BuiltinPreludeId::DIV_ERROR_TYPE
                            | crate::BuiltinPreludeId::NARROW_ERROR_TYPE
                    ) =>
                {
                    if targs.is_some() {
                        return self.issue_node(
                            SemanticRule::Type5,
                            node,
                            SemanticIssueKind::type_mismatch(
                                TYPE5_NO_TARGS_EXPECTED,
                                TYPE5_NO_TARGS_FOUND,
                            ),
                        );
                    }
                    let ty = match id {
                        crate::BuiltinPreludeId::OVERFLOW_TYPE => PreludeType::Overflow,
                        crate::BuiltinPreludeId::DIV_ERROR_TYPE => PreludeType::DivError,
                        crate::BuiltinPreludeId::NARROW_ERROR_TYPE => PreludeType::NarrowError,
                        _ => return Err(SemanticCompilerFailure::InvalidResolution.into()),
                    };
                    return Ok(CheckedType::Nominal(self.prelude_nominal(ty)?));
                }
                ResolvedTarget::Prelude(_) => {
                    return self
                        .unsupported(UnsupportedSemanticFeature::PreludeNominalValues, node);
                }
                ResolvedTarget::Source {
                    declaration,
                    class: DeclarationClass::NominalType,
                } => {
                    let template_index = *self
                        .nominal_templates_by_declaration
                        .get(&declaration)
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                    let template = self
                        .nominal_templates
                        .get(template_index)
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                    let instance = self.nominal_generic_substitution(
                        node,
                        &template.generic_parameters,
                        &template.region_parameters,
                        substitution,
                    )?;
                    return self
                        .source_nominal_instance(declaration, &instance)
                        .map(CheckedType::Nominal)
                        .ok_or_else(|| SemanticCompilerFailure::InvalidResolution.into());
                }
                ResolvedTarget::Source {
                    declaration,
                    class: DeclarationClass::GenericType,
                } => {
                    if targs.is_some() {
                        return self.issue_node(
                            SemanticRule::Type5,
                            node,
                            SemanticIssueKind::type_mismatch(
                                TYPE5_NO_TARGS_EXPECTED,
                                TYPE5_NO_TARGS_FOUND,
                            ),
                        );
                    }
                    let Some(ty) = substitution.type_argument(declaration) else {
                        return self.unsupported(UnsupportedSemanticFeature::Generics, node);
                    };
                    return Ok(ty);
                }
                ResolvedTarget::Container(id) => {
                    return self.parse_container_type(node, id, substitution);
                }
                _ => {}
            }
        }
        self.unsupported(UnsupportedSemanticFeature::CompositeValues, node)
    }

    pub(super) fn result_type_arguments_with(
        &self,
        node: NodeId,
        substitution: &GenericSubstitution,
    ) -> Result<(CheckedType, CheckedType), CheckStop> {
        let Some(targs) = self.tree.argument_list(node)? else {
            return self.issue_node(
                SemanticRule::Type5,
                node,
                SemanticIssueKind::type_mismatch(
                    RESULT_TARGS_EXPECTED,
                    "Result with no written type-argument list",
                ),
            );
        };
        let arguments = self.tree.children_with(targs, Production::Targ)?;
        let [ok, error] = arguments.as_slice() else {
            return self.issue_node(
                SemanticRule::Type5,
                node,
                SemanticIssueKind::type_mismatch(
                    "Result<T, E> with exactly two type arguments",
                    "a Result type-argument list of a different length",
                ),
            );
        };
        let Some(ok) = self.tree.first_child_with(*ok, Production::Type)? else {
            return self.issue_node(
                SemanticRule::Type5,
                node,
                SemanticIssueKind::type_mismatch(
                    "a type in each Result type-argument position",
                    "a const argument in a Result type-argument position",
                ),
            );
        };
        let Some(error) = self.tree.first_child_with(*error, Production::Type)? else {
            return self.issue_node(
                SemanticRule::Type5,
                node,
                SemanticIssueKind::type_mismatch(
                    "a type in each Result type-argument position",
                    "a const argument in a Result type-argument position",
                ),
            );
        };
        let ok = self.parse_type_with(ok, substitution)?;
        let error = self.parse_type_with(error, substitution)?;
        Ok((ok, error))
    }

    /// One written `Array`, `Slots`, `Ring`, or `Box` type [TYPE-9].
    ///
    /// Each is a TYPEID with `targs` [GRAM-3], read positionally: a shape
    /// takes its element type and, in the constant-capacity form, the type
    /// constant N [CONST-1]; `Box` takes its referent and carries no brand,
    /// there being one heap [STOR-8].
    ///
    /// A runtime-capacity shape may appear only as the content of a `Box`
    /// [TYPE-9]. That is a judgment about the position the type is written
    /// at, not about reading the type, so this function forms the type and
    /// the writing position refuses it.
    fn parse_container_type(
        &self,
        node: NodeId,
        id: crate::ContainerNominalId,
        substitution: &GenericSubstitution,
    ) -> Result<CheckedType, CheckStop> {
        let shape = crate::container_nominal(id)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?
            .shape;
        let arguments = match self.tree.argument_list(node)? {
            Some(targs) => self.tree.children_with(targs, Production::Targ)?,
            None => Vec::new(),
        };
        let expected = match shape {
            crate::ContainerShape::Array => "Array<T, N> or Array<T>",
            crate::ContainerShape::Slots => "Slots<T, N> or Slots<T>",
            crate::ContainerShape::Ring => "Ring<T, N> or Ring<T>",
            crate::ContainerShape::Box => "Box<T> with one referent type",
        };
        let mismatch = |found: &str| -> Result<CheckedType, CheckStop> {
            self.issue_node(
                SemanticRule::Type5,
                node,
                SemanticIssueKind::type_mismatch(expected, found),
            )
        };
        // [STOR-8] a unit carrying the no-heap declaration cannot name `Box`
        // or a runtime-capacity shape, which is the type half of what that
        // declaration withdraws; the call half is judged at the `call`.
        if self.no_heap
            && (shape == crate::ContainerShape::Box || arguments.len() == 1)
            && !self.tree.is_prelude_node(node)?
        {
            return self.issue_node(
                SemanticRule::Stor8,
                node,
                SemanticIssueKind::HeapTypeUnderNoHeap {
                    spelling: expected.to_owned(),
                    mechanical_fix: STOR8_NO_HEAP,
                },
            );
        }
        if shape == crate::ContainerShape::Box {
            let [referent] = arguments.as_slice() else {
                return mismatch("a Box type-argument list of a different length");
            };
            let Some(referent_node) = self.tree.first_child_with(*referent, Production::Type)?
            else {
                return mismatch("a const argument in the Box referent position");
            };
            let referent = self.parse_type_with(referent_node, substitution)?;
            return self
                .box_nominals
                .get(&referent)
                .copied()
                .map(CheckedType::Nominal)
                .ok_or_else(|| SemanticCompilerFailure::InvalidResolution.into());
        }
        // [TYPE-9]'s two placements: a written type constant is the
        // constant-capacity form, and its absence is the runtime-capacity
        // form, whose capacity is a measure fixed at construction [MSR-1].
        let (element, capacity) = match arguments.as_slice() {
            [element] => (*element, None),
            [element, length] => {
                let Some(length_node) = self.tree.first_child_with(*length, Production::Const)?
                else {
                    return mismatch("a type argument in the capacity position");
                };
                (*element, Some(length_node))
            }
            _ => return mismatch("a type-argument list of a different length"),
        };
        let Some(element_node) = self.tree.first_child_with(element, Production::Type)? else {
            return mismatch("a const argument in the element position");
        };
        let element_type = self.parse_type_with(element_node, substitution)?;
        let capacity = capacity
            .map(|length| self.parse_const_expression_with(length, substitution))
            .transpose()?;
        match (shape, capacity) {
            // An `Array` has no window: every slot always holds a value and
            // `a.len == a.cap` [WIN-1].
            (crate::ContainerShape::Array, Some(length)) => Ok(CheckedType::Array {
                element: self.intern_element(element_type)?,
                length,
            }),
            (crate::ContainerShape::Array, None) => {
                self.reject_unboxed_runtime_capacity(node)?;
                Ok(CheckedType::Buffer {
                    element: self.intern_element(element_type)?,
                })
            }
            // The two window shapes in both placements [WIN-1]: the filled
            // prefix is `r.len` and a `Ring` additionally carries the window
            // origin `head`.
            (crate::ContainerShape::Slots, capacity) => {
                if capacity.is_none() {
                    self.reject_unboxed_runtime_capacity(node)?;
                }
                Ok(CheckedType::Window {
                    shape: WindowShape::Slots,
                    element: self.intern_element(element_type)?,
                    capacity,
                })
            }
            (crate::ContainerShape::Ring, capacity) => {
                if capacity.is_none() {
                    self.reject_unboxed_runtime_capacity(node)?;
                }
                Ok(CheckedType::Window {
                    shape: WindowShape::Ring,
                    element: self.intern_element(element_type)?,
                    capacity,
                })
            }
            (crate::ContainerShape::Box, _) => {
                Err(SemanticCompilerFailure::InvalidResolution.into())
            }
        }
    }

    /// [TYPE-9] a runtime-capacity `Array<T>`, `Slots<T>`, or `Ring<T>`
    /// appears only as the content of a `Box`, the type of its `inner`
    /// field, and never inline in another value and never as a local
    /// binding; every other written position is this rule's hard error at
    /// the complete `type`.
    fn reject_unboxed_runtime_capacity(&self, node: NodeId) -> Result<(), CheckStop> {
        if self.is_box_content_position(node)? {
            return Ok(());
        }
        self.issue_node(
            SemanticRule::Type9,
            node,
            SemanticIssueKind::type_mismatch(
                "the content of a Box, which is the one position a runtime-capacity shape occupies",
                "a stored, element, parameter, local, or type-argument position",
            ),
        )
    }

    /// Whether this written `type` is the one type argument of a `Box`.
    ///
    /// The judgment is over the written form and not over a substituted
    /// instance: [TYPE-9] refuses the *occurrence*, and a generic parameter
    /// bound to `Box<Slots<T>>` writes no runtime-capacity type of its own.
    fn is_box_content_position(&self, node: NodeId) -> Result<bool, CheckStop> {
        let Some(argument) = self.tree.parent(node)? else {
            return Ok(false);
        };
        if self.tree.production(argument)? != Production::Targ {
            return Ok(false);
        }
        let Some(list) = self.tree.parent(argument)? else {
            return Ok(false);
        };
        if self.tree.production(list)? != Production::Targs {
            return Ok(false);
        }
        let Some(owner) = self.tree.parent(list)? else {
            return Ok(false);
        };
        if self.tree.production(owner)? != Production::Type {
            return Ok(false);
        }
        if !self.tree.names_nominal(owner)? {
            return Ok(false);
        }
        let usage = self.use_at(owner, LexicalUseRole::Type)?;
        Ok(matches!(usage.target(), ResolvedTarget::Container(id)
            if crate::container_nominal(id)
                .is_some_and(|entry| entry.shape == crate::ContainerShape::Box)))
    }

    pub(super) fn option_type_argument_with(
        &self,
        node: NodeId,
        substitution: &GenericSubstitution,
    ) -> Result<CheckedType, CheckStop> {
        let Some(targs) = self.tree.argument_list(node)? else {
            return self.issue_node(
                SemanticRule::Type5,
                node,
                SemanticIssueKind::type_mismatch(
                    OPTION_TARGS_EXPECTED,
                    "Option with no written type-argument list",
                ),
            );
        };
        let arguments = self.tree.children_with(targs, Production::Targ)?;
        let [value] = arguments.as_slice() else {
            return self.issue_node(
                SemanticRule::Type5,
                node,
                SemanticIssueKind::type_mismatch(
                    "Option<T> with exactly one type argument",
                    "an Option type-argument list of a different length",
                ),
            );
        };
        let Some(value) = self.tree.first_child_with(*value, Production::Type)? else {
            return self.issue_node(
                SemanticRule::Type5,
                node,
                SemanticIssueKind::type_mismatch(
                    "a type in the Option type-argument position",
                    "a const argument in the Option type-argument position",
                ),
            );
        };
        let value = self.parse_type_with(value, substitution)?;
        Ok(value)
    }

    pub(super) fn integer_type(&self, node: NodeId) -> Result<Option<IntegerType>, CheckStop> {
        let fixed = [
            (FixedTerminal::I8, IntegerType::I8),
            (FixedTerminal::I16, IntegerType::I16),
            (FixedTerminal::I32, IntegerType::I32),
            (FixedTerminal::I64, IntegerType::I64),
            (FixedTerminal::U8, IntegerType::U8),
            (FixedTerminal::U16, IntegerType::U16),
            (FixedTerminal::U32, IntegerType::U32),
            (FixedTerminal::U64, IntegerType::U64),
        ];
        for (terminal, ty) in fixed {
            if self.has_fixed(node, terminal)? {
                return Ok(Some(ty));
            }
        }
        Ok(None)
    }

    /// [EFF-1] one written row: every `reads` entry before every `writes`
    /// entry, each entry naming exactly one path, each path written at most
    /// once per category, and no entry another entry covers: nothing at or
    /// below the path of a `writes` entry, and no `reads` entry below the path
    /// of another `reads` entry.
    ///
    /// `pure` is the unique spelling of the empty row. Allocation and release
    /// carry no effect entry at all [STOR-8], so the row has exactly these two
    /// categories and the canonical order is the two-element one.
    pub(super) fn parse_effects(
        &self,
        node: NodeId,
        parameters: &[ParameterSignature],
    ) -> Result<EffectSet, CheckStop> {
        if self.has_fixed(node, FixedTerminal::Pure)? {
            return Ok(EffectSet::NONE);
        }
        let effects = self.tree.children_with(node, Production::Effect)?;
        let mut previous = None;
        let mut declared = EffectSet::NONE;
        let mut written = [Vec::new(), Vec::new()];
        // Every entry in written order with its category and `effect` node,
        // for the subsumption judgment once the whole row is read.
        let mut entries = Vec::new();
        for effect in effects {
            let ordinal = if self.has_fixed(effect, FixedTerminal::Reads)? {
                0_usize
            } else if self.has_fixed(effect, FixedTerminal::Writes)? {
                1
            } else {
                return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
            };
            // [EFF-1] "A category may appear more than once in one row, and
            // the canonical order is every `reads` entry before every
            // `writes` entry"; only a descending step is the rejection.
            if previous.is_some_and(|last| last > ordinal) {
                return self.issue_node(
                    SemanticRule::Eff1,
                    node,
                    SemanticIssueKind::InvalidEffectRow {
                        reason: EFF1_CATEGORY_ORDER,
                        mechanical_fix: EFF1_CATEGORY_ORDER_FIX,
                    },
                );
            }
            previous = Some(ordinal);
            for path in self.effect_paths(effect, parameters)? {
                // [EFF-1] "A row lists each path at most once per category,
                // and a repeated entry is an EFF-1 rejection at that
                // `effect`."
                if written[ordinal].contains(&path) {
                    return self.issue_node(
                        SemanticRule::Eff1,
                        effect,
                        SemanticIssueKind::InvalidEffectRow {
                            reason: EFF1_REPEATED_PATH,
                            mechanical_fix: EFF1_REPEATED_PATH_FIX,
                        },
                    );
                }
                written[ordinal].push(path.clone());
                entries.push((path.clone(), ordinal == 1, effect));
                if ordinal == 0 {
                    declared.add_read(path);
                } else {
                    declared.add_write(path);
                }
            }
        }
        // [EFF-1] a `writes` entry states every access at or below its path
        // and a `reads` entry every observation at or below its path, so an
        // entry that another entry of the row covers is a second spelling of
        // what that entry already states [FORM-1]: a `writes` entry covers
        // every entry at or below it, and a `reads` entry every `reads` entry
        // at or below it. "At or below" is [EFF-2]'s covering relation: one
        // root and a step prefix. Two entries of one category and one path
        // were already refused above as a repeated entry, so a cover of the
        // same category reaching this pass is a proper prefix. The first such
        // entry in written order is refused at its own `effect`, naming the
        // first entry that covers it; EFF-1 names no restructuring, so none is
        // carried.
        for (index, (path, write, node)) in entries.iter().enumerate() {
            let covering = entries
                .iter()
                .enumerate()
                .find(|(other, (cover, cover_write, _))| {
                    *other != index
                        && (*cover_write || !*write)
                        && cover.root == path.root
                        && path.steps.starts_with(&cover.steps)
                });
            if let Some((_, (_, _, covering))) = covering {
                return self.issue_node(
                    SemanticRule::Eff1,
                    *node,
                    SemanticIssueKind::SubsumedEffectEntry {
                        entry: self.tree.source_spelling(*node)?,
                        covering: self.tree.source_spelling(*covering)?,
                    },
                );
            }
        }
        Ok(declared)
    }

    fn effect_paths(
        &self,
        node: NodeId,
        parameters: &[ParameterSignature],
    ) -> Result<Vec<CheckedStatePath>, CheckStop> {
        let mut paths = Vec::new();
        for path_node in self.tree.children_with(node, Production::EffectPath)? {
            paths.push(self.effect_path(node, path_node, parameters)?.0);
        }
        Ok(paths)
    }

    /// One `effect_path := epbase epsuffix*` [EFF-1], with the type its last
    /// step selects.
    ///
    /// `epbase := IDENT` names the reference parameter's selected storage;
    /// the row has no source `deref` wrapper.
    fn effect_path(
        &self,
        effect: NodeId,
        path_node: NodeId,
        parameters: &[ParameterSignature],
    ) -> Result<(CheckedStatePath, SelectedPlaceType), CheckStop> {
        let base = self
            .tree
            .first_child_with(path_node, Production::Epbase)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let (mut path, mut ty) = self.effect_root(effect, base, parameters)?;
        for suffix in self.tree.children_with(path_node, Production::Epsuffix)? {
            let (step, next) = self.effect_step(effect, path_node, suffix, ty, parameters)?;
            path.steps.push(step);
            ty = next;
        }
        Ok((path, ty))
    }

    /// The root of one row entry [EFF-1]: one reference parameter of the same
    /// callable, whose complete state a bare parameter names.
    fn effect_root(
        &self,
        effect: NodeId,
        base: NodeId,
        parameters: &[ParameterSignature],
    ) -> Result<(CheckedStatePath, SelectedPlaceType), CheckStop> {
        let usage = self
            .resolved
            .lexical_uses_at(base)
            .find(|usage| usage.role() == LexicalUseRole::EffectRoot)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let ResolvedTarget::Source {
            declaration,
            class: DeclarationClass::Value,
        } = usage.target()
        else {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        };
        // [EFF-1] a root resolving to a local, a result binder, a by-value
        // parameter, or a non-parameter declaration is a rejection: a
        // by-value parameter has no effect entry at all, the call site
        // recording the consumption of a `move` argument instead [EFF-5].
        let Some(parameter) = parameters
            .iter()
            .find(|parameter| parameter.declaration == declaration)
            .filter(|parameter| parameter.mode.is_reference())
        else {
            return self.issue_node(
                SemanticRule::Eff1,
                effect,
                SemanticIssueKind::InvalidEffectRow {
                    reason: EFF1_NON_PARAMETER_ROOT,
                    mechanical_fix: EFF1_NON_PARAMETER_ROOT_FIX,
                },
            );
        };
        Ok((
            CheckedStatePath {
                root: declaration,
                steps: Vec::new(),
            },
            if parameter.mode == CheckedMode::Range {
                SelectedPlaceType::Range(parameter.ty)
            } else {
                SelectedPlaceType::Value(parameter.ty)
            },
        ))
    }

    /// One `epsuffix := "." IDENT | "." TYPEID "." IDENT | "[" IDENT erange? "]"`
    /// [EFF-1], with the type it selects.
    ///
    /// A one-name `.IDENT` suffix is a struct field, a measure, or a window
    /// part [TYPE-10, WIN-2]. The spelling reserves nothing: the type selected
    /// by the preceding path decides whether the measure/part vocabulary
    /// applies, and every other type takes the ordinary field walk.
    fn effect_step(
        &self,
        effect: NodeId,
        path_node: NodeId,
        suffix: NodeId,
        selected: SelectedPlaceType,
        parameters: &[ParameterSignature],
    ) -> Result<(CheckedEffectStep, SelectedPlaceType), CheckStop> {
        if self.has_fixed(suffix, FixedTerminal::LeftBracket)? {
            return self.effect_index_step(suffix, selected, parameters);
        }
        let mut names = self
            .resolved
            .deferred_uses_at(suffix)
            .filter(|field| {
                matches!(
                    field.role(),
                    crate::DeferredUseRole::EffectField | crate::DeferredUseRole::PayloadVariant
                )
            })
            .collect::<Vec<_>>();
        names.sort_by_key(|field| field.origin().role_ordinal());
        match names.as_slice() {
            // `.TYPEID.IDENT`: one enum payload step [GRAM-5].
            [variant_use, field_use] => {
                let SelectedPlaceType::Value(ty) = selected else {
                    return self.invalid_effect_row(path_node, EFF1_FIELD_OF_NON_STRUCT);
                };
                let CheckedType::Nominal(nominal) = ty else {
                    return self.invalid_effect_row(path_node, EFF1_FIELD_OF_NON_STRUCT);
                };
                let CheckedNominalKind::Enum { variants } = &self.nominal(nominal)?.kind else {
                    return self.invalid_effect_row(path_node, EFF1_FIELD_OF_NON_STRUCT);
                };
                let Some((variant_ordinal, variant)) = variants
                    .iter()
                    .enumerate()
                    .find(|(_, variant)| variant.name == variant_use.spelling())
                else {
                    return self.invalid_effect_row(path_node, EFF1_UNKNOWN_FIELD);
                };
                let Some((field_ordinal, field)) = variant
                    .fields
                    .iter()
                    .enumerate()
                    .find(|(_, field)| field.name == field_use.spelling())
                else {
                    return self.invalid_effect_row(path_node, EFF1_UNKNOWN_FIELD);
                };
                self.reject_inaccessible_field(
                    nominal,
                    Some(variant_ordinal),
                    field_ordinal,
                    field_use.spelling(),
                    path_node,
                )?;
                Ok((
                    CheckedEffectStep::Payload {
                        variant: u32::try_from(variant_ordinal)
                            .map_err(|_| SemanticCompilerFailure::CounterOverflow)?,
                        field: u32::try_from(field_ordinal)
                            .map_err(|_| SemanticCompilerFailure::CounterOverflow)?,
                    },
                    SelectedPlaceType::Value(field.ty),
                ))
            }
            [field_use] => {
                let spelling = field_use.spelling();
                // [OP-10] PRE-1's W/X parameters carry a window identity
                // before an operand supplies their shape. Their declaration
                // origin distinguishes them from an ordinary source generic
                // with the same name; concrete rows are checked again below.
                let symbolic_window = match selected {
                    SelectedPlaceType::Value(CheckedType::Generic(declaration)) => {
                        self.is_window_type_parameter(declaration)?
                    }
                    _ => false,
                };
                let selected_measure = match selected {
                    SelectedPlaceType::Range(_) => {
                        (spelling == "len").then_some(CheckedMeasure::Length)
                    }
                    SelectedPlaceType::Value(ty) => measure_named(spelling).filter(|measure| {
                        (symbolic_window && *measure != CheckedMeasure::Head)
                            || ty.measured().is_some_and(|measured| {
                                !matches!(
                                    measure.cell(measured),
                                    super::super::model::MeasureCell::Absent
                                )
                            })
                    }),
                    SelectedPlaceType::UnresolvedWindowElement => None,
                };
                if let Some(measure) = selected_measure {
                    // A measure is a u64 pseudo-field of the measured place
                    // and selects no storage below itself [MSR-1, TYPE-10]. A
                    // missing row falls through: the same spelling may be an
                    // ordinary field of another type.
                    return Ok((
                        CheckedEffectStep::Measure(measure),
                        SelectedPlaceType::Value(CheckedType::Integer(IntegerType::U64)),
                    ));
                }
                let SelectedPlaceType::Value(ty) = selected else {
                    // A range has only its own `len`; its element's fields,
                    // measures and parts require an intervening index.
                    return self.invalid_effect_row(path_node, EFF1_FIELD_OF_NON_STRUCT);
                };
                if let Some(part) = window_part_named(spelling)
                    && (symbolic_window || matches!(ty, CheckedType::Window { .. }))
                {
                    // A window part names slots of the window it belongs to,
                    // so the selected type stays the element type [WIN-2].
                    let element = self.container_element_type(ty)?.unwrap_or(ty);
                    return Ok((
                        CheckedEffectStep::Part(part),
                        if symbolic_window {
                            SelectedPlaceType::UnresolvedWindowElement
                        } else {
                            SelectedPlaceType::Value(element)
                        },
                    ));
                }
                let CheckedType::Nominal(nominal) = ty else {
                    return self.invalid_effect_row(path_node, EFF1_FIELD_OF_NON_STRUCT);
                };
                // [TYPE-9] a `Box`'s content is its field `inner`, reached by
                // the ordinary field step, so a row names it as that step and
                // the selected type below it is the cell's referent. The
                // resolved place identity of that step is the dereference the
                // content already is, which is what [OWN-7] compares.
                if let CheckedNominalKind::Box { referent, .. } = self.nominal(nominal)?.kind {
                    if spelling != "inner" {
                        return self.invalid_effect_row(path_node, EFF1_UNKNOWN_FIELD);
                    }
                    return Ok((CheckedEffectStep::Deref, SelectedPlaceType::Value(referent)));
                }
                let CheckedNominalKind::Struct { fields } = &self.nominal(nominal)?.kind else {
                    return self.invalid_effect_row(path_node, EFF1_FIELD_OF_NON_STRUCT);
                };
                let Some((ordinal, field)) = fields
                    .iter()
                    .enumerate()
                    .find(|(_, field)| field.name == spelling)
                else {
                    return self.invalid_effect_row(path_node, EFF1_UNKNOWN_FIELD);
                };
                self.reject_inaccessible_field(nominal, None, ordinal, spelling, path_node)?;
                Ok((
                    CheckedEffectStep::Field(
                        u32::try_from(ordinal)
                            .map_err(|_| SemanticCompilerFailure::CounterOverflow)?,
                    ),
                    SelectedPlaceType::Value(field.ty),
                ))
            }
            _ => {
                let _ = effect;
                Err(SemanticCompilerFailure::InvalidCanonicalTree.into())
            }
        }
    }

    /// One `"[" IDENT erange? "]"` suffix [EFF-1]: a whole-index position, or
    /// a range position whose two endpoints are value parameters of the same
    /// callable. A signature never contains an index expression, so an index
    /// enters a row only through such an IDENT.
    fn effect_index_step(
        &self,
        suffix: NodeId,
        selected: SelectedPlaceType,
        parameters: &[ParameterSignature],
    ) -> Result<(CheckedEffectStep, SelectedPlaceType), CheckStop> {
        let range = self.tree.first_child_with(suffix, Production::Erange)?;
        let range_origin = range.map(|node| self.tree.path(node)).transpose()?;
        // A stable sort below orders the two nodes' uses; within one node
        // both readers yield record order.
        let mut indices = self
            .resolved
            .lexical_uses_at(suffix)
            .chain(
                range
                    .into_iter()
                    .flat_map(|node| self.resolved.lexical_uses_at(node)),
            )
            .filter(|usage| usage.role() == LexicalUseRole::EffectIndex)
            .collect::<Vec<_>>();
        indices.sort_by_key(|usage| {
            (
                range_origin
                    .as_ref()
                    .is_some_and(|range| usage.origin().node() == *range),
                usage.origin().role_ordinal(),
            )
        });
        let mut endpoints = Vec::new();
        for usage in indices {
            let ResolvedTarget::Source {
                declaration,
                class: DeclarationClass::Value,
            } = usage.target()
            else {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            };
            if !parameters
                .iter()
                .any(|parameter| parameter.declaration == declaration)
            {
                return self.invalid_effect_row(suffix, EFF1_NON_PARAMETER_ROOT);
            }
            endpoints.push(declaration);
        }
        let element = match selected {
            // An index into a range selects the range's element itself. It
            // must not project through that element again when T is Array or
            // another container.
            SelectedPlaceType::Range(element) => element,
            SelectedPlaceType::Value(ty) => self.container_element_type(ty)?.unwrap_or(ty),
            SelectedPlaceType::UnresolvedWindowElement => {
                return self.invalid_effect_row(suffix, EFF1_FIELD_OF_NON_STRUCT);
            }
        };
        match endpoints.as_slice() {
            [index] => Ok((
                CheckedEffectStep::Index(*index),
                SelectedPlaceType::Value(element),
            )),
            // A range position names a run of elements [REF-4], so every step
            // below it is relative to that run and reads the element type.
            [start, end] => Ok((
                CheckedEffectStep::Range {
                    start: *start,
                    end: *end,
                },
                SelectedPlaceType::Range(element),
            )),
            _ => Err(SemanticCompilerFailure::InvalidCanonicalTree.into()),
        }
    }

    /// The element type of a storage shape [TYPE-9], or `None` where the type
    /// is not one.
    fn container_element_type(&self, ty: CheckedType) -> Result<Option<CheckedType>, CheckStop> {
        Ok(match ty {
            CheckedType::Array { element, .. } | CheckedType::Window { element, .. } => {
                Some(self.element_type(element)?)
            }
            CheckedType::Buffer { element } => Some(self.element_type(element)?),
            _ => None,
        })
    }

    fn invalid_effect_row<T>(&self, node: NodeId, reason: &'static str) -> Result<T, CheckStop> {
        let mechanical_fix = if reason == EFF1_UNKNOWN_FIELD {
            EFF1_UNKNOWN_FIELD_FIX
        } else if reason == EFF1_NON_PARAMETER_ROOT {
            EFF1_NON_PARAMETER_ROOT_FIX
        } else {
            EFF1_FIELD_OF_NON_STRUCT_FIX
        };
        self.issue_node(
            SemanticRule::Eff1,
            node,
            SemanticIssueKind::InvalidEffectRow {
                reason,
                mechanical_fix,
            },
        )
    }

    pub(super) fn parse_const_expression_with(
        &self,
        node: NodeId,
        substitution: &GenericSubstitution,
    ) -> Result<CheckedConst, CheckStop> {
        let digits = self
            .tree
            .direct_tokens_matching(node, &[TerminalPredicate::Digits])?;
        let identifiers = self.tree.direct_identifiers(node)?;
        let mut terms = digits
            .iter()
            .copied()
            .chain(identifiers.iter().copied())
            .collect::<Vec<_>>();
        terms.sort_unstable();
        let Some(operator) = self.tree.first_child_with(node, Production::InfixOp)? else {
            let [term] = terms.as_slice() else {
                return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
            };
            return self.parse_const_term(node, *term, &identifiers, substitution);
        };
        // The candidate CONST-1 shape: exactly one operation over two terms,
        // evaluated at monomorphization. Both terms concrete evaluates now
        // under the const-eval overflow policy; a symbolic operand interns
        // one symbolic operation instead, and every concrete instantiation
        // re-enters this path with a concrete substitution.
        let operation = self.const_operation(operator)?;
        let [left, right] = terms.as_slice() else {
            return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
        };
        let left = self.parse_const_term(node, *left, &identifiers, substitution)?;
        let right = self.parse_const_term(node, *right, &identifiers, substitution)?;
        if let (CheckedConst::Value(left), CheckedConst::Value(right)) = (left, right) {
            return evaluate_const_operation(operation, left, right)
                .map(CheckedConst::Value)
                .ok_or_else(|| {
                    self.issue_value(
                        SemanticRule::Const1,
                        node,
                        SemanticIssueKind::ConstEvalOverflow {
                            operation: operation.spelling(),
                        },
                    )
                });
        }
        self.combine_const(operation, left, right)
            .ok_or_else(|| SemanticCompilerFailure::CounterOverflow.into())
    }

    /// The one const operation of a candidate-grammar `const` tail. The
    /// grammar reuses `infix_op`; the runtime arithmetic modes are rejected
    /// here, so const evaluation has exactly the five bare spellings and
    /// never overloads a runtime overflow mode [CONST-1].
    fn const_operation(&self, operator: NodeId) -> Result<ConstOperation, CheckStop> {
        let [terminal] = self.tree.direct_token_indices(operator)? else {
            return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
        };
        match self.tree.token_bytes(*terminal)? {
            b"+" => Ok(ConstOperation::Add),
            b"-" => Ok(ConstOperation::Subtract),
            b"*" => Ok(ConstOperation::Multiply),
            b"/" => Ok(ConstOperation::Divide),
            b"%" => Ok(ConstOperation::Remainder),
            b"+wrap" | b"+checked" | b"+sat" | b"-wrap" | b"-checked" | b"-sat" | b"*wrap"
            | b"*checked" | b"*sat" | b"/checked" | b"%checked" => self.issue_node(
                SemanticRule::Const1,
                operator,
                SemanticIssueKind::ConstRuntimeArithmeticMode {
                    mechanical_fix: "write the bare operator: const evaluation rejects overflow at compile time and has no runtime arithmetic modes",
                },
            ),
            _ => Err(SemanticCompilerFailure::InvalidCanonicalTree.into()),
        }
    }

    /// One `const` term: a bare decimal u64 literal, an integer-typed named
    /// const, or an in-scope const-generic parameter [CONST-1].
    fn parse_const_term(
        &self,
        node: NodeId,
        terminal: usize,
        identifiers: &[usize],
        substitution: &GenericSubstitution,
    ) -> Result<CheckedConst, CheckStop> {
        let Some(ordinal) = identifiers.iter().position(|entry| *entry == terminal) else {
            return std::str::from_utf8(self.tree.token_bytes(terminal)?)
                .ok()
                .and_then(|digits| digits.parse::<u64>().ok())
                .map(CheckedConst::Value)
                .ok_or_else(|| {
                    self.issue_value(
                        SemanticRule::Const1,
                        node,
                        SemanticIssueKind::InvalidConstValue,
                    )
                });
        };
        let uses = self.uses_at_ordered(node, LexicalUseRole::Const)?;
        let usage = uses
            .get(ordinal)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let (declaration, named) = match usage.target() {
            ResolvedTarget::Source {
                declaration,
                class: DeclarationClass::NamedConst,
            } => (declaration, true),
            ResolvedTarget::Source {
                declaration,
                class: DeclarationClass::ConstGeneric,
            } => (declaration, false),
            _ => {
                return self.issue_node(
                    SemanticRule::Const1,
                    node,
                    SemanticIssueKind::InvalidConstValue,
                );
            }
        };
        if !named {
            let Some(value) = substitution.const_argument(declaration) else {
                return self.unsupported(UnsupportedSemanticFeature::Generics, node);
            };
            return Ok(value);
        }
        let Some(constant) = self.constants.get(&declaration).copied() else {
            if self.postcondition_declaration_unavailable(declaration) {
                return Err(CheckStop::PostconditionPrerequisiteUnavailable);
            }
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        };
        let constant = self.constant(constant)?;
        let CheckedValue::Integer { ty, bits } = &constant.value else {
            return self.issue_node(
                SemanticRule::Const1,
                node,
                SemanticIssueKind::InvalidConstValue,
            );
        };
        if ty.signed() && bits & (1_u64 << (ty.width() - 1)) != 0 {
            return self.issue_node(
                SemanticRule::Const1,
                node,
                SemanticIssueKind::InvalidConstValue,
            );
        }
        Ok(CheckedConst::Value(*bits))
    }

    pub(super) fn parse_const_value(
        &self,
        node: NodeId,
        expected: CheckedType,
    ) -> Result<CheckedValue, CheckStop> {
        // The construction shape is decided first: its direct tokens include
        // the field-label IDENTs, so the single-identifier reference reader
        // below must never see it.
        if self.tree.names_nominal(node)? {
            return self.parse_const_construction(node, expected);
        }
        if let Some(literal) = self
            .tree
            .direct_token_with(node, TerminalPredicate::Literal)?
        {
            let value = self.parse_literal(node, self.tree.token_bytes(literal)?)?;
            if value.ty() == expected {
                return Ok(value);
            }
            return self.issue_node(
                SemanticRule::Const2,
                node,
                SemanticIssueKind::InvalidConstValue,
            );
        }
        if self
            .tree
            .direct_token_with(node, TerminalPredicate::Identifier)?
            .is_some()
        {
            // CONST-2 permits earlier-constant identifiers only for primitive
            // values. Arrays/runs require lists and structs require a complete
            // construction; physical constant storage cannot authorize an
            // implicit conversion between array and fixed-run source types.
            if !matches!(
                expected,
                CheckedType::Unit | CheckedType::Integer(_) | CheckedType::Float(_)
            ) {
                return self.issue_node(
                    SemanticRule::Const2,
                    node,
                    SemanticIssueKind::InvalidConstValue,
                );
            }
            let usage = self.use_at(node, LexicalUseRole::ConstValue)?;
            let ResolvedTarget::Source {
                declaration,
                class: DeclarationClass::NamedConst,
            } = usage.target()
            else {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            };
            let Some(id) = self.constants.get(&declaration).copied() else {
                if self.postcondition_declaration_unavailable(declaration) {
                    return Err(CheckStop::PostconditionPrerequisiteUnavailable);
                }
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            };
            let constant = self.constant(id)?;
            if constant.ty == expected {
                return Ok(constant.value.clone());
            }
            return self.issue_node(
                SemanticRule::Const2,
                node,
                SemanticIssueKind::InvalidConstValue,
            );
        }
        let CheckedType::Array { element, length } = expected else {
            return self.issue_node(
                SemanticRule::Const2,
                node,
                SemanticIssueKind::InvalidConstValue,
            );
        };
        if !self.has_fixed(node, FixedTerminal::LeftBracket)? {
            return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
        }
        let entries = self.tree.children_with(node, Production::Cvalue)?;
        let Some(length) = length.value() else {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        };
        if u64::try_from(entries.len()).ok() != Some(length) {
            return self.issue_node(
                SemanticRule::Const2,
                node,
                SemanticIssueKind::InvalidConstValue,
            );
        }
        let element_type = self.element_type(element)?;
        let mut elements = Vec::with_capacity(entries.len());
        for entry in entries {
            elements.push(self.parse_const_value(entry, element_type)?);
        }
        Ok(CheckedValue::Array {
            ty: expected,
            elements,
        })
    }

    /// One construction cvalue [CONST-2] totally defining a struct-typed
    /// constant. The constructor's complete instance must match the declared
    /// type, and the written fields must be the declared
    /// fields in exact declared order [GRAM-8], each field value a cvalue of
    /// the declared field type.
    fn parse_const_construction(
        &self,
        node: NodeId,
        expected: CheckedType,
    ) -> Result<CheckedValue, CheckStop> {
        let CheckedType::Nominal(id) = expected else {
            return self.issue_node(
                SemanticRule::Const2,
                node,
                SemanticIssueKind::InvalidConstValue,
            );
        };
        let declared_fields = {
            let nominal = self.nominal(id)?;
            let super::super::model::CheckedNominalKind::Struct { fields } = &nominal.kind else {
                return self.issue_node(
                    SemanticRule::Const2,
                    node,
                    SemanticIssueKind::InvalidConstValue,
                );
            };
            fields.clone()
        };
        // [MOD-5, TYPE-2] a const construction names every field, so outside
        // the struct's declaring module each must be published and none may
        // be readonly, exactly as for a runtime construction.
        for (index, field) in declared_fields.iter().enumerate() {
            self.reject_inaccessible_field(id, None, index, &field.name, node)?;
            if field.readonly && self.field_withholds_writes(id, field) {
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
        // The constructor is named as the source writes its type, with an
        // instance's type and const arguments [GRAM-3].
        let constructor_name = self.checked_type_name(expected)?;
        let (expected_template, expected_arguments) = self
            .source_nominal_instances
            .get(id.0 as usize)
            .and_then(Option::as_ref)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let usage = self.use_at(node, LexicalUseRole::Construct)?;
        let ResolvedTarget::Source { declaration, .. } = usage.target() else {
            // A core prelude constructor never names a const-eligible
            // struct.
            return self.issue_node(
                SemanticRule::Const2,
                node,
                SemanticIssueKind::InvalidConstValue,
            );
        };
        let written_template = match self.constructor_templates_by_declaration.get(&declaration) {
            Some(super::ConstructorTemplate::Struct { template }) => *template,
            _ => {
                return self.issue_node(
                    SemanticRule::Const2,
                    node,
                    SemanticIssueKind::InvalidConstValue,
                );
            }
        };
        if written_template != *expected_template {
            return self.issue_node(
                SemanticRule::Const2,
                node,
                SemanticIssueKind::InvalidConstValue,
            );
        }
        let template = self
            .nominal_templates
            .get(written_template)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let written_arguments = self.nominal_generic_substitution(
            node,
            &template.generic_parameters,
            &template.region_parameters,
            &GenericSubstitution::default(),
        )?;
        if &written_arguments != expected_arguments {
            return self.issue_node(
                SemanticRule::Const2,
                node,
                SemanticIssueKind::InvalidConstValue,
            );
        }
        let labels = self.tree.direct_identifiers(node)?;
        let values = self.tree.children_with(node, Production::Cvalue)?;
        let declared_field_names = declared_fields
            .iter()
            .map(|field| field.name.clone())
            .collect::<Vec<_>>();
        if labels.len() != declared_fields.len() || values.len() != declared_fields.len() {
            return self.issue_node(
                SemanticRule::Gram8,
                node,
                SemanticIssueKind::InvalidConstructionFields {
                    constructor: constructor_name,
                    declared_fields: declared_field_names,
                },
            );
        }
        let mut fields = Vec::with_capacity(declared_fields.len());
        for ((label, value), declared) in labels.iter().zip(&values).zip(&declared_fields) {
            if self.tree.token_bytes(*label)? != declared.name.as_bytes() {
                return self.issue_node(
                    SemanticRule::Gram8,
                    node,
                    SemanticIssueKind::InvalidConstructionFields {
                        constructor: constructor_name,
                        declared_fields: declared_field_names,
                    },
                );
            }
            fields.push(self.parse_const_value(*value, declared.ty)?);
        }
        Ok(CheckedValue::Struct {
            ty: expected,
            fields,
        })
    }

    pub(super) fn constant(&self, id: CheckedConstantId) -> Result<&CheckedConstant, CheckStop> {
        self.checked_constants
            .get(id.0 as usize)
            .ok_or_else(|| SemanticCompilerFailure::InvalidResolution.into())
    }

    pub(super) fn parse_const_type(&self, node: NodeId) -> Result<CheckedType, CheckStop> {
        self.reject_ineligible_const_storage(node)?;
        let ty = self.parse_type(node)?;
        if self.const_eligible_type(ty)? {
            Ok(ty)
        } else {
            self.issue_node(
                SemanticRule::Const2,
                node,
                SemanticIssueKind::InvalidConstValue,
            )
        }
    }

    /// [CONST-2] reject an ineligible storage shape before parsing types whose
    /// nominal instances need not exist for an ineligible const declaration.
    ///
    /// A const is pure static rodata, so `Box`, `Slots`, and `Ring` are not
    /// const-eligible, and a runtime-capacity `Array<T>` is not either, its
    /// capacity being fixed at a construction a const never performs. The
    /// walk follows only the `Array` element position: an arbitrary nominal
    /// type argument may be phantom and does not itself decide eligibility.
    fn reject_ineligible_const_storage(&self, root: NodeId) -> Result<(), CheckStop> {
        let mut current = Some(root);
        while let Some(node) = current {
            let shape = self.written_container_shape(node)?;
            let eligible_array = matches!(shape, Some(crate::ContainerShape::Array))
                && self.written_container_capacity(node)?.is_some();
            if shape.is_some() && !eligible_array {
                return self.issue_node(
                    SemanticRule::Const2,
                    node,
                    SemanticIssueKind::InvalidConstValue,
                );
            }
            current = if eligible_array {
                self.written_container_element(node)?
            } else {
                None
            };
        }
        Ok(())
    }

    /// The storage shape one written `type` node spells [TYPE-9], or `None`
    /// where the node spells any other type.
    fn written_container_shape(
        &self,
        node: NodeId,
    ) -> Result<Option<crate::ContainerShape>, CheckStop> {
        if !self.tree.names_nominal(node)? {
            return Ok(None);
        }
        let ResolvedTarget::Container(id) = self.use_at(node, LexicalUseRole::Type)?.target()
        else {
            return Ok(None);
        };
        Ok(crate::container_nominal(id).map(|nominal| nominal.shape))
    }

    /// The written element `type` node of a storage shape [TYPE-9], which is
    /// its first type argument.
    fn written_container_element(&self, node: NodeId) -> Result<Option<NodeId>, CheckStop> {
        let Some(targs) = self.tree.argument_list(node)? else {
            return Ok(None);
        };
        let arguments = self.tree.children_with(targs, Production::Targ)?;
        let Some(first) = arguments.first() else {
            return Ok(None);
        };
        Ok(self.tree.first_child_with(*first, Production::Type)?)
    }

    /// The written capacity `const` node of a constant-capacity storage shape
    /// [TYPE-9], which is its second type argument.
    fn written_container_capacity(&self, node: NodeId) -> Result<Option<NodeId>, CheckStop> {
        let Some(targs) = self.tree.argument_list(node)? else {
            return Ok(None);
        };
        let arguments = self.tree.children_with(targs, Production::Targ)?;
        let Some(second) = arguments.get(1) else {
            return Ok(None);
        };
        Ok(self.tree.first_child_with(*second, Production::Const)?)
    }

    /// Check every type reachable through CONST-2's element and field
    /// relation. Zero-length arrays still require eligible element types;
    /// a visited set closes recursive zero-extent nominal graphs. Parsing
    /// the finite cvalue separately checks every written element and field.
    fn const_eligible_type(&self, ty: CheckedType) -> Result<bool, CheckStop> {
        let mut pending = vec![ty];
        let mut visited = HashSet::new();
        while let Some(ty) = pending.pop() {
            if !visited.insert(ty) {
                continue;
            }
            match ty {
                CheckedType::Unit | CheckedType::Integer(_) | CheckedType::Float(_) => {}
                CheckedType::Array { element, .. } => {
                    pending.push(self.element_type(element)?);
                }
                CheckedType::Nominal(id) => {
                    let CheckedNominalKind::Struct { fields } = &self.nominal(id)?.kind else {
                        return Ok(false);
                    };
                    pending.extend(fields.iter().map(|field| field.ty));
                }
                CheckedType::Bool
                | CheckedType::Generic(_)
                | CheckedType::GenericInt(_)
                | CheckedType::GenericFloat(_)
                | CheckedType::Window { .. }
                | CheckedType::Buffer { .. } => return Ok(false),
            }
        }
        Ok(true)
    }

    /// Resolve one checked-program-local structural element handle.
    pub(in crate::semantic) fn element_type(
        &self,
        element: CheckedElement,
    ) -> Result<CheckedType, CheckStop> {
        self.elements
            .borrow()
            .get(element.0 as usize)
            .copied()
            .ok_or_else(|| SemanticCompilerFailure::InvalidResolution.into())
    }

    /// Hash-cons the complete slot type. Children have already been formed by
    /// the ordinary type parser, so structural edges always point backwards.
    pub(in crate::semantic) fn intern_element(
        &self,
        ty: CheckedType,
    ) -> Result<CheckedElement, CheckStop> {
        if let Some(element) = self.element_ids.borrow().get(&ty).copied() {
            return Ok(element);
        }
        let element = CheckedElement(
            u32::try_from(self.elements.borrow().len())
                .map_err(|_| SemanticCompilerFailure::CounterOverflow)?,
        );
        self.elements.borrow_mut().push(ty);
        self.element_ids.borrow_mut().insert(ty, element);
        Ok(element)
    }

    pub(super) fn parse_literal(
        &self,
        node: NodeId,
        bytes: &[u8],
    ) -> Result<CheckedValue, CheckStop> {
        if bytes == b"unit" {
            return Ok(CheckedValue::Unit);
        }
        if bytes.ends_with(b"_f32") || bytes.ends_with(b"_f64") {
            return parse_float_literal(bytes).ok_or_else(|| {
                self.issue_value(
                    SemanticRule::Form7,
                    node,
                    SemanticIssueKind::InvalidFloatLiteral,
                )
            });
        }
        parse_integer(bytes).ok_or_else(|| {
            self.issue_value(
                SemanticRule::Form7,
                node,
                SemanticIssueKind::InvalidIntegerLiteral,
            )
        })
    }
}

fn parse_integer(bytes: &[u8]) -> Option<CheckedValue> {
    let split = bytes.iter().rposition(|byte| *byte == b'_')?;
    let ty = match bytes.get(split + 1..)? {
        b"i8" => IntegerType::I8,
        b"i16" => IntegerType::I16,
        b"i32" => IntegerType::I32,
        b"i64" => IntegerType::I64,
        b"u8" => IntegerType::U8,
        b"u16" => IntegerType::U16,
        b"u32" => IntegerType::U32,
        b"u64" => IntegerType::U64,
        _ => return None,
    };
    let negative = bytes.first() == Some(&b'-');
    if negative && !ty.signed() {
        return None;
    }
    let digits = bytes.get(usize::from(negative)..split)?;
    if digits.is_empty()
        || (digits.len() > 1 && digits.first() == Some(&b'0'))
        || (negative && digits == b"0")
    {
        return None;
    }
    let magnitude = std::str::from_utf8(digits).ok()?.parse::<u128>().ok()?;
    let width = ty.width();
    let bits = if ty.signed() {
        let maximum = (1_u128 << (width - 1)) - 1;
        let minimum_magnitude = 1_u128 << (width - 1);
        if (!negative && magnitude > maximum) || (negative && magnitude > minimum_magnitude) {
            return None;
        }
        if negative {
            let modulus = 1_u128 << width;
            u64::try_from(modulus - magnitude).ok()?
        } else {
            u64::try_from(magnitude).ok()?
        }
    } else {
        let maximum = (1_u128 << width) - 1;
        if magnitude > maximum {
            return None;
        }
        u64::try_from(magnitude).ok()?
    };
    Some(CheckedValue::Integer { ty, bits })
}

/// The measure-vocabulary candidate for one `.IDENT` spelling [MSR-1,
/// TYPE-10]. A consumer still checks the type the suffix follows: these names
/// reserve nothing and remain ordinary fields of types with no matching row.
pub(super) fn measure_named(spelling: &str) -> Option<CheckedMeasure> {
    match spelling {
        "len" => Some(CheckedMeasure::Length),
        "cap" => Some(CheckedMeasure::Capacity),
        "head" => Some(CheckedMeasure::Head),
        _ => None,
    }
}

/// The window-part-vocabulary candidate for one `.IDENT` spelling [WIN-2,
/// TYPE-10]. A consumer still checks that the suffix follows a window.
pub(super) fn window_part_named(spelling: &str) -> Option<WindowPart> {
    match spelling {
        "next" => Some(WindowPart::Next),
        "last" => Some(WindowPart::Last),
        "filled" => Some(WindowPart::Filled),
        "free" => Some(WindowPart::Free),
        _ => None,
    }
}
