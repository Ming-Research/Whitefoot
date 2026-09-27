//! [OP-10, OP-11, OP-14] the [PRE-1] rows whose type parameters an operand
//! supplies.
//!
//! [OP-10] states that a compiler-owned window type parameter "is supplied by
//! the operand and never written", and that "a window operation, `swap`
//! [OP-11], and `free_empty` [OP-14] therefore write no type arguments at a
//! call: every type parameter of those rows is supplied by an operand". The
//! construction functions [OP-13] are explicitly excluded and keep [FN-2]'s
//! written argument list.
//!
//! Each of the eleven rows here fixes every type parameter from one designated
//! value parameter's selected type. The window rows take the shape from the
//! `&W` operand and the element type from that same shape, because [OP-10]
//! says the parameter's "element type is that shape's own element type"; a
//! `value` argument never selects the instance and is checked against the
//! element type the shape already fixed.

use std::collections::HashMap;

use crate::syntax::NodeId;
use crate::{
    DeclarationClass, DeclarationId, FixedTerminal, LexicalUseRole, Production, ResolvedTarget,
    SemanticCompilerFailure, SemanticIssueKind, SemanticRule, UnsupportedSemanticFeature,
};

use super::super::super::model::{CheckedMode, CheckedNominalKind, CheckedType, WindowShape};
use super::super::types::SelectedPlaceType;
use super::super::{CheckStop, Checker, FunctionTemplate, LocalBinding};
use super::{GenericArgument, GenericSubstitution};

/// Which part of one operand's selected type a type parameter takes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::semantic::check) enum OperandProjection {
    /// The operand's own selected type: the referent of a `&W` parameter or
    /// the value of an `own W` one.
    Whole,
    /// The element type of the shape the operand selects, reached through a
    /// `Box` where the operand is one [TYPE-9].
    Element,
}

/// Which operand types the parameter, and which shapes that operand admits.
#[derive(Clone, Copy, Debug)]
struct OperandParameter {
    /// The value parameter ordinal this type parameter reads, in the row's
    /// declared parameter order.
    ordinal: usize,
    projection: OperandProjection,
}

/// The shapes one row's operand admits [OP-10, OP-14].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AdmittedShapes {
    /// `Slots<T, n>`, `Slots<T>`, `Ring<T, n>` and `Ring<T>` [OP-10].
    Window,
    /// `Ring<T, n>` and `Ring<T>` alone: `place_front` and `take_front`
    /// "admit `Ring<T, n>` and `Ring<T>` alone" [OP-10].
    Ring,
    /// `Box<Slots<T>>` alone: "`grow` is defined on `Box<Slots<T>>` alone"
    /// [OP-10].
    BoxedRuntimeSlots,
    /// The window shapes plus runtime-capacity `Box<Slots<T>>` and
    /// `Box<Ring<T>>` [OP-14].
    WindowOrBoxedWindow,
    /// Any owned type: `swap` "exchanges the values at two owned places of one
    /// type" [OP-11].
    AnyValue,
}

/// One [PRE-1] row whose type parameters an operand supplies.
#[derive(Clone, Copy, Debug)]
struct OperandRow {
    name: &'static str,
    /// The rule a refused operand cites, and the rule the row belongs to.
    rule: SemanticRule,
    admitted: AdmittedShapes,
    /// One entry per declared type parameter, in declared order.
    parameters: &'static [OperandParameter],
}

const WINDOW_AND_ELEMENT: &[OperandParameter] = &[
    OperandParameter {
        ordinal: 0,
        projection: OperandProjection::Whole,
    },
    OperandParameter {
        ordinal: 0,
        projection: OperandProjection::Element,
    },
];

/// The eleven rows [OP-10], [OP-11] and [OP-14] name. Every other [PRE-1]
/// row, the nine construction functions [OP-13] included, writes its
/// arguments explicitly under [FN-2].
const OPERAND_ROWS: &[OperandRow] = &[
    OperandRow {
        name: "place_back",
        rule: SemanticRule::Op10,
        admitted: AdmittedShapes::Window,
        parameters: WINDOW_AND_ELEMENT,
    },
    OperandRow {
        name: "take_back",
        rule: SemanticRule::Op10,
        admitted: AdmittedShapes::Window,
        parameters: WINDOW_AND_ELEMENT,
    },
    OperandRow {
        name: "insert_at",
        rule: SemanticRule::Op10,
        admitted: AdmittedShapes::Window,
        parameters: WINDOW_AND_ELEMENT,
    },
    OperandRow {
        name: "remove_at",
        rule: SemanticRule::Op10,
        admitted: AdmittedShapes::Window,
        parameters: WINDOW_AND_ELEMENT,
    },
    // `append(destination: &W, source: &X)`: two shapes, one per operand.
    OperandRow {
        name: "append",
        rule: SemanticRule::Op10,
        admitted: AdmittedShapes::Window,
        parameters: &[
            OperandParameter {
                ordinal: 0,
                projection: OperandProjection::Whole,
            },
            OperandParameter {
                ordinal: 1,
                projection: OperandProjection::Whole,
            },
        ],
    },
    // `split_off(source: &W, index: u64, destination: &X)`.
    OperandRow {
        name: "split_off",
        rule: SemanticRule::Op10,
        admitted: AdmittedShapes::Window,
        parameters: &[
            OperandParameter {
                ordinal: 0,
                projection: OperandProjection::Whole,
            },
            OperandParameter {
                ordinal: 2,
                projection: OperandProjection::Whole,
            },
        ],
    },
    // `grow<T>(cell: &Box<Slots<T>>, capacity: u64)`: the one parameter is
    // the boxed window's element type.
    OperandRow {
        name: "grow",
        rule: SemanticRule::Op10,
        admitted: AdmittedShapes::BoxedRuntimeSlots,
        parameters: &[OperandParameter {
            ordinal: 0,
            projection: OperandProjection::Element,
        }],
    },
    OperandRow {
        name: "place_front",
        rule: SemanticRule::Op10,
        admitted: AdmittedShapes::Ring,
        parameters: WINDOW_AND_ELEMENT,
    },
    OperandRow {
        name: "take_front",
        rule: SemanticRule::Op10,
        admitted: AdmittedShapes::Ring,
        parameters: WINDOW_AND_ELEMENT,
    },
    OperandRow {
        name: "swap",
        rule: SemanticRule::Op11,
        admitted: AdmittedShapes::AnyValue,
        parameters: &[OperandParameter {
            ordinal: 0,
            projection: OperandProjection::Whole,
        }],
    },
    OperandRow {
        name: "free_empty",
        rule: SemanticRule::Op14,
        admitted: AdmittedShapes::WindowOrBoxedWindow,
        parameters: &[OperandParameter {
            ordinal: 0,
            projection: OperandProjection::Whole,
        }],
    },
];

impl<'unit> Checker<'unit> {
    /// Whether this template is one of the eleven rows whose type parameters
    /// an operand supplies [OP-10, OP-11, OP-14].
    ///
    /// The spelling decides it because [TYPE-6] gives the whole closed unit
    /// one declaration domain: a source `fn_decl` of one of these names
    /// collides with the [PRE-1] record and never reaches this question. The
    /// prelude-origin test is kept beside it so a future relaxation of that
    /// collision cannot silently capture a source declaration.
    pub(in crate::semantic::check) fn operand_directed_row_index(
        &self,
        template: &FunctionTemplate,
    ) -> Result<Option<usize>, CheckStop> {
        let Some(index) = OPERAND_ROWS
            .iter()
            .position(|row| row.name == template.name)
        else {
            return Ok(None);
        };
        if !self.tree.is_prelude_node(template.node)? {
            return Ok(None);
        }
        Ok(Some(index))
    }

    /// Whether this instance is of the one row whose undischarged requirement
    /// is reported under [OP-14] "at the complete `call`", where every other
    /// callee's is an [FN-8] report.
    pub(in crate::semantic::check) fn empties_run(
        &self,
        function: super::super::super::model::FunctionId,
    ) -> Result<bool, CheckStop> {
        let signature = self
            .signatures
            .get(function.0 as usize)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let Some(&template_index) = self.templates_by_declaration.get(&signature.declaration)
        else {
            return Ok(false);
        };
        let template = self
            .function_templates
            .get(template_index)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        Ok(self
            .operand_directed_row_index(template)?
            .and_then(|index| OPERAND_ROWS.get(index))
            .is_some_and(|row| row.rule == SemanticRule::Op14))
    }

    /// The substitution one call to an operand-directed row selects [OP-10].
    ///
    /// The operand is read as a written place, which is what every admitted
    /// spelling of these arguments is: a `borrow_expr` over a place, a bare
    /// reference variable naming a path [REF-1], or `move p` for the one row
    /// that consumes its operand [OP-14]. Nothing else can carry a window, so
    /// an argument of any other form is an operand outside the row's admitted
    /// set.
    pub(in crate::semantic::check) fn operand_directed_substitution(
        &self,
        call: NodeId,
        template: &FunctionTemplate,
        row_index: usize,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<GenericSubstitution, CheckStop> {
        let row = OPERAND_ROWS
            .get(row_index)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        // [OP-10] no argument list is written at these calls at all; one that
        // is written is the writer asking for an instance the operand already
        // fixes.
        if self.tree.argument_list(call)?.is_some() {
            return self.issue_node(
                row.rule,
                call,
                SemanticIssueKind::type_mismatch(
                    "no written generic argument, the operand supplying every type parameter",
                    "an explicit argument list",
                ),
            );
        }
        if template.generic_parameters.len() != row.parameters.len() {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        let mut bindings_out = Vec::with_capacity(row.parameters.len());
        for (parameter, source) in template.generic_parameters.iter().zip(row.parameters) {
            let operand = self.operand_selected_type(call, source.ordinal, bindings)?;
            let Some(operand) = operand else {
                return self.refuse_operand(row, call);
            };
            if !self.operand_shape_admitted(row.admitted, operand)? {
                return self.refuse_operand(row, call);
            }
            let Some(value) = self.project_operand_type(source.projection, operand)? else {
                return self.refuse_operand(row, call);
            };
            bindings_out.push((parameter.key(), GenericArgument::Type(value)));
        }
        Ok(GenericSubstitution::from_bindings(bindings_out)?)
    }

    fn refuse_operand<T>(&self, row: &OperandRow, call: NodeId) -> Result<T, CheckStop> {
        self.issue_node(
            row.rule,
            call,
            SemanticIssueKind::UnadmittedOperandShape {
                expected: match row.admitted {
                    AdmittedShapes::Window => "a `Slots` or `Ring` operand [OP-10]",
                    AdmittedShapes::Ring => "a `Ring` operand, which is what this row admits",
                    AdmittedShapes::BoxedRuntimeSlots => "a `Box<Slots<T>>` operand",
                    AdmittedShapes::WindowOrBoxedWindow => {
                        "a `Slots` or `Ring`, or a `Box` holding its runtime-capacity form [OP-14]"
                    }
                    AdmittedShapes::AnyValue => "an owned place of one type [OP-11]",
                },
                mechanical_fix: "pass an operand of the admitted shape, or use an operation whose row admits this operand's shape",
            },
        )
    }

    fn operand_shape_admitted(
        &self,
        admitted: AdmittedShapes,
        operand: CheckedType,
    ) -> Result<bool, CheckStop> {
        Ok(match admitted {
            AdmittedShapes::AnyValue => true,
            AdmittedShapes::Window => matches!(operand, CheckedType::Window { .. }),
            AdmittedShapes::Ring => matches!(
                operand,
                CheckedType::Window {
                    shape: WindowShape::Ring,
                    ..
                }
            ),
            AdmittedShapes::BoxedRuntimeSlots => matches!(
                self.box_content(operand)?,
                Some(CheckedType::Window {
                    shape: WindowShape::Slots,
                    capacity: None,
                    ..
                })
            ),
            AdmittedShapes::WindowOrBoxedWindow => {
                matches!(operand, CheckedType::Window { .. })
                    || matches!(
                        self.box_content(operand)?,
                        Some(CheckedType::Window { capacity: None, .. })
                    )
            }
        })
    }

    /// The content of a [TYPE-9] `Box`, for an operand that is one.
    pub(in crate::semantic::check) fn box_content(
        &self,
        operand: CheckedType,
    ) -> Result<Option<CheckedType>, CheckStop> {
        let CheckedType::Nominal(nominal) = operand else {
            return Ok(None);
        };
        Ok(match self.nominal(nominal)?.kind {
            CheckedNominalKind::Box { referent, .. } => Some(referent),
            _ => None,
        })
    }

    fn project_operand_type(
        &self,
        projection: OperandProjection,
        operand: CheckedType,
    ) -> Result<Option<CheckedType>, CheckStop> {
        match projection {
            OperandProjection::Whole => Ok(Some(operand)),
            OperandProjection::Element => {
                let shape = match self.box_content(operand)? {
                    Some(content) => content,
                    None => operand,
                };
                match shape {
                    CheckedType::Window { element, .. } | CheckedType::Array { element, .. } => {
                        Ok(Some(self.element_type(element)?))
                    }
                    _ => Ok(None),
                }
            }
        }
    }

    /// The selected type of the argument at `ordinal`, where that argument is
    /// a written place [GRAM-5].
    ///
    /// `None` is an argument whose form carries no place, which is exactly an
    /// operand outside the row's admitted set; the caller renders that
    /// refusal so the diagnostic names the operation rather than the walk.
    fn operand_selected_type(
        &self,
        call: NodeId,
        ordinal: usize,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<Option<CheckedType>, CheckStop> {
        let Some(list) = self
            .tree
            .first_child_with(call, Production::FieldinitList)?
        else {
            return Ok(None);
        };
        let fields = self.tree.children_with(list, Production::Fieldinit)?;
        let Some(field) = fields.get(ordinal).copied() else {
            return Ok(None);
        };
        let Some(atom) = self.tree.first_child_with(field, Production::Atom)? else {
            return Ok(None);
        };
        let place = match self.tree.first_child_with(atom, Production::Place)? {
            Some(place) => place,
            None => {
                let Some(borrow) = self.tree.first_child_with(atom, Production::BorrowExpr)? else {
                    // `atom := literal | "move" place | place | borrow_expr`
                    // [GRAM-5], so what is left here is a literal, which
                    // carries no window at all.
                    return Ok(None);
                };
                let Some(place) = self.tree.first_child_with(borrow, Production::Place)? else {
                    return Ok(None);
                };
                place
            }
        };
        self.place_selected_type(place, bindings)
    }

    /// The type one written `place` selects, read without checking the use.
    ///
    /// This is a type oracle and not a second place judgment: it reads the
    /// declared types the bindings already carry and the type each written
    /// step selects, and reports `None` wherever the written form selects no
    /// type it can name. Every ownership, liveness, validity and effect
    /// judgment on the same operand is made once, by the ordinary argument
    /// check against the instance this oracle selects.
    pub(in crate::semantic::check) fn place_selected_type(
        &self,
        place: NodeId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<Option<CheckedType>, CheckStop> {
        Ok(self
            .place_selected_kind(place, bindings)?
            .and_then(|selected| match selected {
                SelectedPlaceType::Value(ty) | SelectedPlaceType::Range(ty) => Some(ty),
                SelectedPlaceType::UnresolvedWindowElement => None,
            }))
    }

    /// The value or range kind selected by one written `place`.
    ///
    /// A range binding stores its element in `LocalBinding::ty`, so retaining
    /// the kind prevents an index from projecting through a composite element
    /// a second time.
    pub(in crate::semantic::check) fn place_selected_kind(
        &self,
        place: NodeId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<Option<SelectedPlaceType>, CheckStop> {
        let pbase = self
            .tree
            .first_child_with(place, Production::Pbase)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        // [TYPE-7] `deref` names the referent of a reference, whose selected
        // type is the type the reference binding already carries [REF-1].
        let mut ty = if self.has_fixed(pbase, FixedTerminal::Deref)? {
            let inner = self
                .tree
                .first_child_with(pbase, Production::Place)?
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            match self.place_selected_kind(inner, bindings)? {
                Some(selected) => selected,
                None => return Ok(None),
            }
        } else {
            // `entry(IDENT)` is a contract-only `pbase` [GRAM-5, MSR-3] and
            // names no operand in a body.
            if !self.tree.children(pbase)?.is_empty() {
                return self.unsupported(UnsupportedSemanticFeature::CompositeValues, pbase);
            }
            let usage = self.use_at(pbase, LexicalUseRole::PlaceBase)?;
            match usage.target() {
                ResolvedTarget::Source {
                    declaration,
                    class: DeclarationClass::Value,
                } => match bindings.get(&declaration) {
                    Some(local) if local.mode == CheckedMode::Range => {
                        SelectedPlaceType::Range(local.ty)
                    }
                    Some(local) => SelectedPlaceType::Value(local.ty),
                    None => return Ok(None),
                },
                ResolvedTarget::Source {
                    declaration,
                    class: DeclarationClass::NamedConst,
                } => match self.constants.get(&declaration) {
                    Some(constant) => SelectedPlaceType::Value(self.constant(*constant)?.ty),
                    None => return Ok(None),
                },
                _ => return Ok(None),
            }
        };
        for suffix in self.tree.children_with(place, Production::Psuffix)? {
            if self.subscript_offset(suffix)?.is_some() {
                // [REF-4] a range step selects a `&[T]`, which is a reference
                // kind and not a type [TYPE-8]; no row here admits one.
                if self
                    .tree
                    .first_child_with(suffix, Production::RangeTail)?
                    .is_some()
                {
                    return Ok(None);
                }
                // [OP-4] a subscript selects the base shape's element type.
                match ty {
                    SelectedPlaceType::Range(element) => {
                        ty = SelectedPlaceType::Value(element);
                    }
                    SelectedPlaceType::Value(CheckedType::Array { element, .. })
                    | SelectedPlaceType::Value(CheckedType::Window { element, .. }) => {
                        ty = SelectedPlaceType::Value(self.element_type(element)?);
                    }
                    SelectedPlaceType::Value(CheckedType::Buffer { element }) => {
                        ty = SelectedPlaceType::Value(self.element_type(element)?);
                    }
                    // [REF-4] a `&[T]` binding already carries the element
                    // type the dereference selects, so its subscript selects
                    // that same type.
                    _ => {}
                }
                continue;
            }
            // [GRAM-5] an enum payload step is `"." TYPEID "." IDENT`; the
            // explicit-place walk has no step for one either, so a window
            // reached through a payload is a capability limit here and never
            // a source verdict.
            if self
                .tree
                .direct_token_with(suffix, crate::TerminalPredicate::TypeIdentifier)?
                .is_some()
            {
                return self.unsupported(UnsupportedSemanticFeature::CompositeValues, suffix);
            }
            let name = self
                .deferred_use_at(suffix, crate::DeferredUseRole::ProjectedField)?
                .spelling()
                .to_owned();
            // [OP-15, MSR-1, TYPE-10] a real measure is an `own u64` member
            // and ends the written path; no row here takes one as its shape
            // operand. The spelling reserves nothing, so a type with no such
            // measure continues through its ordinary field below.
            if matches!(ty, SelectedPlaceType::Range(_)) && name == "len" {
                return Ok(None);
            }
            let SelectedPlaceType::Value(value_ty) = ty else {
                return Ok(None);
            };
            if super::super::types::measure_named(&name).is_some_and(|measure| {
                value_ty.measured().is_some_and(|measured| {
                    !matches!(
                        measure.cell(measured),
                        super::super::super::model::MeasureCell::Absent
                    )
                })
            }) {
                return Ok(None);
            }
            // [WIN-2, TYPE-10] parts likewise select only a window. A source
            // struct may declare a field with the same spelling.
            if super::super::types::window_part_named(&name).is_some()
                && matches!(value_ty, CheckedType::Window { .. })
            {
                return Ok(None);
            }
            // [TYPE-9] a `Box`'s content is its field `inner`.
            if let Some(referent) = self.box_content(value_ty)? {
                if name != "inner" {
                    return Ok(None);
                }
                ty = SelectedPlaceType::Value(referent);
                continue;
            }
            let CheckedType::Nominal(nominal) = value_ty else {
                return Ok(None);
            };
            let CheckedNominalKind::Struct { fields } = &self.nominal(nominal)?.kind else {
                return Ok(None);
            };
            let Some(field) = fields.iter().find(|field| field.name == name) else {
                return Ok(None);
            };
            ty = SelectedPlaceType::Value(field.ty);
        }
        Ok(Some(ty))
    }
}
