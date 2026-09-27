use std::collections::{HashMap, HashSet};

use crate::syntax::NodeId;
use crate::{
    DeclarationClass, DeclarationId, FixedTerminal, LexicalUseRole, Production, ResolvedTarget,
    SemanticCompilerFailure, SemanticIssueKind, SemanticRule,
};

use super::super::super::model::{
    BindingId, CheckedMode, CheckedNominalKind, CheckedProjectedDrop, CheckedStatement,
    CheckedType, PropagationContext,
};
use super::super::{CheckStop, Checker, FunctionSignature, LocalBinding, PreludeType};
use super::{ControlScope, StatementResult};

// [DIAG-1] the return position asks TYPE-7's implicit read before the
// operand's OWN-1 spelling judgments because TYPE-7 is defined first.
const _: () = assert!(
    SemanticRule::Type7.definition_rank() < SemanticRule::Own1.definition_rank(),
    "check_return_implicit_read precedes the operand's OWN-1 judgments"
);

/// The consumed place of a destructuring consume, as its repairs name it.
pub(super) struct ConsumedPlace {
    /// Whether it names owned storage directly, so that its parts can move
    /// out [OWN-1, WIN-3].
    pub(super) owned: bool,
    /// The place as a read of it is written [TYPE-7].
    pub(super) spelling: String,
}

impl<'unit> Checker<'unit> {
    /// The result-list nominal a checked value carries, when it is one
    /// [GRAM-2, CALL-4].
    ///
    /// A destructuring binder list and a `set` target list are the two places
    /// that name a callee's result ordinals again, and both ask this one
    /// question of the value in front of them rather than of the callee's
    /// spelling or of the statement's shape.
    pub(super) fn result_list_of(
        &self,
        value: &super::super::TypedExpression,
    ) -> Option<crate::NominalId> {
        let CheckedType::Nominal(nominal) = value.expression.ty() else {
            return None;
        };
        (value.mode == CheckedMode::Own
            && self
                .result_list_nominals
                .values()
                .any(|other| *other == nominal))
        .then_some(nominal)
    }

    /// The declared result ordinals of a result-list nominal, in written
    /// order.
    pub(super) fn result_list_ordinals(
        &self,
        nominal: crate::NominalId,
    ) -> Result<Vec<CheckedType>, CheckStop> {
        match &self.nominal(nominal)?.kind {
            CheckedNominalKind::Struct { fields } => {
                Ok(fields.iter().map(|field| field.ty).collect())
            }
            _ => Err(SemanticCompilerFailure::InvalidResolution.into()),
        }
    }

    /// The [TYPE-5] rejection a binder or target list receives when its
    /// right-hand side does not produce exactly that many result ordinals.
    pub(super) fn result_list_shape_rejection<T>(
        &self,
        call: NodeId,
        written: usize,
        value: &super::super::TypedExpression,
    ) -> Result<T, CheckStop> {
        self.issue_node(
            SemanticRule::Type5,
            call,
            SemanticIssueKind::type_mismatch(
                format!("an ordered result list of {written} results"),
                self.checked_value_name(value.mode, value.expression.ty())?,
            ),
        )
    }

    /// Checks `let (a, b) = f(...);` [GRAM-4, TYPE-5, CALL-4].
    ///
    /// The call is evaluated once; binder i is an ordinary fresh `let`
    /// binding of result ordinal i, at that ordinal's declared type and mode.
    pub(super) fn check_destructuring_let(
        &self,
        function: &FunctionSignature,
        node: NodeId,
        call: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        counters: &mut super::ControlCounters<'_>,
        scope: ControlScope<'_>,
    ) -> Result<StatementResult, CheckStop> {
        let declarations = self
            .declarations_at(node, crate::DeclarationRole::Let)?
            .iter()
            .map(|declaration| (declaration.id(), declaration.spelling().to_owned()))
            .collect::<Vec<_>>();
        let value = self.check_call(function, call, bindings, scope.loops.len())?;
        let Some(nominal) = self.result_list_of(&value) else {
            return self.result_list_shape_rejection(call, declarations.len(), &value);
        };
        let ordinals = self.result_list_ordinals(nominal)?;
        if ordinals.len() != declarations.len() {
            return self.result_list_shape_rejection(call, declarations.len(), &value);
        }
        let mut binder_ids = Vec::with_capacity(ordinals.len());
        for ((declaration_id, spelling), ty) in declarations.into_iter().zip(ordinals) {
            let binding = Self::allocate_binding(counters.next_binding)?;
            counters.binding_names.push(spelling);
            if bindings
                .insert(
                    declaration_id,
                    LocalBinding {
                        binding,
                        declaration: declaration_id,
                        mode: CheckedMode::Own,
                        ty,
                        live: true,
                        loop_depth: scope.loops.len(),
                        compiler_updated: false,
                        reference: None,
                        refinement_witnesses: Vec::new(),
                    },
                )
                .is_some()
            {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            }
            binder_ids.push((
                binding,
                ty,
                u32::try_from(binder_ids.len())
                    .map_err(|_| SemanticCompilerFailure::CounterOverflow)?,
            ));
        }
        Ok(Self::continuing_statement(
            CheckedStatement::DestructuringLet {
                node_path: self.tree.path(node)?.clone(),
                bindings: binder_ids,
                // [CALL-4] a result list binds every ordinal, so there is no
                // covered ordinal and no derived release here.
                covered: Vec::new(),
                nominal,
                value: value.expression,
            },
            value.effects,
        ))
    }

    /// Checks `let N(f1: b1, ..., fk: bk) = move v;` [GRAM-4, PROV-6].
    ///
    /// The value is consumed whole and every declared field of `N` is bound
    /// in declaration order, so no residual of `v` survives the statement and
    /// nothing here derives a release of the consumed value's own storage.
    pub(super) fn check_destructuring_consume(
        &self,
        function: &FunctionSignature,
        node: NodeId,
        place: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        counters: &mut super::ControlCounters<'_>,
        scope: ControlScope<'_>,
    ) -> Result<StatementResult, CheckStop> {
        let usage = self.use_at(node, LexicalUseRole::Construct)?;
        let written = usage.spelling().to_owned();
        // S39 the one compiler-owned nominal this statement takes apart is
        // the cell, whose one field is its referent: the destructuring is
        // what takes the value out and releases the cell, and it is the
        // existing statement rather than a new operation.
        let cell = matches!(usage.target(), ResolvedTarget::Container(id)
            if crate::container_nominal(id)
                .is_some_and(|entry| entry.shape == crate::ContainerShape::Box));
        // [TYPE-2] an opaque struct has no usable constructor, and a
        // destructuring `let_stmt` whose TYPEID names one is that rule's hard
        // error at the complete statement. `Box` is the prelude's opaque
        // struct [TYPE-9, PRE-1], so a cell's content is reached through its
        // member `inner` and never by taking the cell apart.
        let opaque_repair = match usage.target() {
            ResolvedTarget::Source { declaration, .. } => {
                if self.is_opaque_struct_declaration(declaration)? {
                    Some(
                        super::super::repairs::opaque_struct_taken_apart(
                            self.opaque_struct_kind(declaration)?,
                            self.consumed_place(place, bindings)?.owned,
                        )
                        .to_owned(),
                    )
                } else {
                    None
                }
            }
            _ if cell => Some(self.cell_taken_apart_repair(node, place, bindings)?),
            _ => None,
        };
        if let Some(mechanical_fix) = opaque_repair {
            return self.issue_node(
                SemanticRule::Type2,
                node,
                SemanticIssueKind::ContainerConstruction {
                    nominal: written,
                    mechanical_fix,
                },
            );
        }
        let source_declaration = match usage.target() {
            ResolvedTarget::Source { declaration, .. } => Some(declaration),
            _ if cell => None,
            _ => return self.destructuring_shape_rejection(node, &written),
        };
        let value =
            self.check_consumed_place(function, node, place, bindings, scope.loops.len(), true)?;
        if value.mode != CheckedMode::Own {
            return self.issue_node(
                SemanticRule::Own1,
                node,
                SemanticIssueKind::BareAffineUse {
                    mechanical_fix: "destructure an own-mode value; a borrow owns nothing to take apart",
                },
            );
        }
        let CheckedType::Nominal(nominal) = value.expression.ty() else {
            return self.destructuring_shape_rejection(node, &written);
        };
        if let Some(nominal_declaration) = source_declaration
            && !self.nominal_instantiates(nominal, nominal_declaration)?
        {
            return self.destructuring_shape_rejection(node, &written);
        }
        let fields = match &self.nominal(nominal)?.kind {
            CheckedNominalKind::Struct { fields } if !cell => fields.clone(),
            CheckedNominalKind::Box {
                referent,
                region: Some(_),
                ..
            } if cell => vec![super::super::super::model::CheckedField {
                name: "value".to_owned(),
                ty: *referent,
                readonly: false,
            }],
            _ => return self.destructuring_shape_rejection(node, &written),
        };
        let binders = match self
            .tree
            .first_child_with(node, Production::FieldbindList)?
        {
            Some(list) => self.tree.children_with(list, Production::Fieldbind)?,
            None => Vec::new(),
        };
        // [GRAM-4] the rest marker is the statement's own `..` token. The
        // consumed `place` carries none: a range step spells its `..` inside
        // a `psuffix`, which is not a direct token of this statement.
        let rest = self.has_fixed(node, FixedTerminal::DotDot)?;
        if !rest && binders.len() != fields.len() {
            return self.invalid_destructuring_fields(&written, &fields, node);
        }
        let mut binder_ids = Vec::with_capacity(fields.len());
        let mut cursor = 0_usize;
        let mut covered = Vec::new();
        for written_binder in binders {
            let spelling = self
                .deferred_use_at(written_binder, crate::DeferredUseRole::MatchField)?
                .spelling()
                .to_owned();
            // [GRAM-10] every written field name is written exactly once in
            // declared order, so the next one is found at or after the field
            // the previous binder took. A missing, extra, repeated,
            // misspelled, or out-of-order name leaves no field here.
            let Some(offset) = fields[cursor..]
                .iter()
                .position(|field| field.name == spelling)
            else {
                return self.invalid_destructuring_fields(&written, &fields, written_binder);
            };
            if offset > 0 && !rest {
                return self.invalid_destructuring_fields(&written, &fields, written_binder);
            }
            covered.extend(cursor..cursor.saturating_add(offset));
            let ordinal = cursor
                .checked_add(offset)
                .ok_or(SemanticCompilerFailure::CounterOverflow)?;
            let field = fields
                .get(ordinal)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            // [MOD-5] outside the declaring module a destructuring consume
            // binds only published fields; `..` covers the rest.
            if !cell {
                self.reject_inaccessible_field(
                    nominal,
                    None,
                    ordinal,
                    &field.name,
                    written_binder,
                )?;
            }
            let declaration = self.declaration_at(written_binder, crate::DeclarationRole::Let)?;
            let binding = Self::allocate_binding(counters.next_binding)?;
            counters
                .binding_names
                .push(declaration.spelling().to_owned());
            if bindings
                .insert(
                    declaration.id(),
                    LocalBinding {
                        binding,
                        declaration: declaration.id(),
                        mode: CheckedMode::Own,
                        ty: field.ty,
                        live: true,
                        loop_depth: scope.loops.len(),
                        compiler_updated: false,
                        reference: None,
                        refinement_witnesses: Vec::new(),
                    },
                )
                .is_some()
            {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            }
            binder_ids.push((
                binding,
                field.ty,
                u32::try_from(ordinal).map_err(|_| SemanticCompilerFailure::CounterOverflow)?,
            ));
            cursor = ordinal
                .checked_add(1)
                .ok_or(SemanticCompilerFailure::CounterOverflow)?;
        }
        if cursor < fields.len() {
            if !rest {
                return self.invalid_destructuring_fields(&written, &fields, node);
            }
            covered.extend(cursor..fields.len());
        }
        let covered = self.covered_field_releases(place, &fields, &covered)?;
        Ok(Self::continuing_statement(
            CheckedStatement::DestructuringLet {
                node_path: self.tree.path(node)?.clone(),
                bindings: binder_ids,
                covered,
                nominal,
                value: value.expression,
            },
            value.effects,
        ))
    }

    /// [WIN-3, STOR-3] the compiler-derived release of every field a final
    /// `..` covers.
    ///
    /// The owner ceases to exist at the statement, so each covered field's
    /// own release action runs there. A covered field that is linear has no
    /// such action: it is a remaining linear part of a consumed owner, which
    /// is [WIN-3]'s hard error at the complete consumed `place`.
    fn covered_field_releases(
        &self,
        place: NodeId,
        fields: &[super::super::super::model::CheckedField],
        covered: &[usize],
    ) -> Result<Vec<CheckedProjectedDrop>, CheckStop> {
        let mut releases = Vec::new();
        for ordinal in covered {
            let field = fields
                .get(*ordinal)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            if self.linear_release_obligation(field.ty)?.is_some() {
                return self.issue_node(
                    SemanticRule::Win3,
                    place,
                    SemanticIssueKind::InvalidElementMove {
                        mechanical_fix:
                            "take it in the same destructuring: let N(f: a, ..) = move v;",
                    },
                );
            }
            let ordinal =
                u32::try_from(*ordinal).map_err(|_| SemanticCompilerFailure::CounterOverflow)?;
            for (path, ty) in self.drop_paths(field.ty, vec![ordinal])? {
                releases.push(CheckedProjectedDrop { fields: path, ty });
            }
        }
        Ok(releases)
    }

    /// [TYPE-2, TYPE-9] the repair of a destructuring consume that names
    /// `Box`: how the statement reaches the content instead, which turns on
    /// what the consumed cell holds and on whether the place owns it. The
    /// cell is typed by the oracle that reads declared types without judging
    /// the use, so the refusal still precedes every judgment of the consumed
    /// place.
    fn cell_taken_apart_repair(
        &self,
        node: NodeId,
        place: NodeId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<String, CheckStop> {
        use super::super::repairs::{CellContent, cell_taken_apart};
        let binder = match self
            .tree
            .first_child_with(node, Production::FieldbindList)?
        {
            Some(list) => match self
                .tree
                .children_with(list, Production::Fieldbind)?
                .first()
            {
                Some(&written) => Some(
                    self.declaration_at(written, crate::DeclarationRole::Let)?
                        .spelling()
                        .to_owned(),
                ),
                None => None,
            },
            None => None,
        };
        // A range reference selects a run, never one cell [REF-4].
        let content = match self.place_selected_kind(place, bindings) {
            Ok(Some(super::super::types::SelectedPlaceType::Value(ty))) => self.box_content(ty)?,
            Ok(_) | Err(CheckStop::Unsupported(_)) => None,
            Err(stop) => return Err(stop),
        };
        let consumed = self.consumed_place(place, bindings)?;
        // [OWN-1, WIN-3] nothing moves out of a cell a reference or an element
        // reaches, so there a content that is not read as a copy is used in
        // place. A runtime-capacity content is never a binding's value,
        // whatever its elements [TYPE-9].
        let content = match content {
            Some(CheckedType::Buffer { .. } | CheckedType::Window { capacity: None, .. }) => {
                Some(if consumed.owned {
                    CellContent::RuntimeCapacity
                } else {
                    CellContent::InPlace
                })
            }
            Some(ty) if self.is_copy_type(ty)? => Some(CellContent::Copy),
            Some(_) if consumed.owned => Some(CellContent::Owned),
            Some(_) => Some(CellContent::InPlace),
            None => None,
        };
        Ok(cell_taken_apart(
            content,
            &consumed.spelling,
            binder.as_deref(),
        ))
    }

    /// How the consumed place of a destructuring consume holds its value:
    /// `owned` when it names owned storage directly, with no reference and no
    /// element on its path, so that its parts can move out [OWN-1, WIN-3];
    /// and the spelling that reads it as a place, which steps through a
    /// reference variable written bare with `deref` [TYPE-7].
    pub(super) fn consumed_place(
        &self,
        place: NodeId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<ConsumedPlace, CheckStop> {
        let spelling = self.tree.source_spelling(place)?;
        let pbase = self
            .tree
            .first_child_with(place, Production::Pbase)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let mut element = false;
        for suffix in self.tree.children_with(place, Production::Psuffix)? {
            element |= self.subscript_offset(suffix)?.is_some();
        }
        if self.has_fixed(pbase, FixedTerminal::Deref)? {
            return Ok(ConsumedPlace {
                owned: false,
                spelling,
            });
        }
        let reference = match self.use_at(pbase, LexicalUseRole::PlaceBase)?.target() {
            ResolvedTarget::Source { declaration, .. } => bindings
                .get(&declaration)
                .is_some_and(|local| local.mode.is_reference()),
            _ => false,
        };
        if !reference {
            return Ok(ConsumedPlace {
                owned: !element,
                spelling,
            });
        }
        let root = self.tree.source_spelling(pbase)?;
        let rest = spelling.strip_prefix(root.as_str()).unwrap_or_default();
        Ok(ConsumedPlace {
            owned: false,
            spelling: format!("deref({root}){rest}"),
        })
    }

    /// [TYPE-5] the destructuring consume's operand is not a value of the
    /// nominal struct type it writes.
    fn destructuring_shape_rejection<T>(
        &self,
        node: NodeId,
        written: &str,
    ) -> Result<T, CheckStop> {
        self.issue_node(
            SemanticRule::Type5,
            node,
            SemanticIssueKind::type_mismatch(
                format!("an own value of struct type {written}"),
                "a value of another type".to_owned(),
            ),
        )
    }

    /// [GRAM-10] the destructuring consume writes every declared field of its
    /// nominal exactly once, in declared order.
    fn invalid_destructuring_fields<T>(
        &self,
        written: &str,
        fields: &[super::super::super::model::CheckedField],
        node: NodeId,
    ) -> Result<T, CheckStop> {
        self.issue_node(
            SemanticRule::Gram10,
            node,
            SemanticIssueKind::InvalidMatchFields {
                variant: written.to_owned(),
                declared_fields: fields.iter().map(|field| field.name.clone()).collect(),
            },
        )
    }

    /// Checks `return e1, ..., en;` in a declaration that writes an ordered
    /// result list [GRAM-2, GRAM-4, FN-1, CALL-4].
    ///
    /// Expression i produces result ordinal i under exactly the ordinary
    /// return judgment for that ordinal's written `rtype`, and the statement
    /// hands back the one result-list value carrying them in written order.
    /// Nothing below this point sees a second return shape.
    pub(super) fn check_result_list_return(
        &self,
        function: &FunctionSignature,
        node: NodeId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        scope: ControlScope<'_>,
    ) -> Result<StatementResult, CheckStop> {
        let nominal = function
            .result_list
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let expressions = self.tree.children_with(node, Production::Expr)?;
        let mut fields = Vec::with_capacity(expressions.len());
        let mut effects = super::super::EffectSet::NONE;
        for (expression_node, declared) in expressions.iter().zip(&function.results) {
            self.check_return_implicit_read(function, *expression_node, bindings)?;
            let value =
                self.check_expression(function, *expression_node, bindings, scope.loops.len())?;
            if value.expression.ty() != declared.ty || value.mode != CheckedMode::Own {
                return Err(CheckStop::source_issue(crate::SemanticIssue {
                    rule: SemanticRule::Fn1,
                    location: crate::SemanticLocation::SourceNode(
                        self.tree.path(node)?.clone(),
                        self.tree.coordinate(*expression_node)?,
                    ),
                    kind: SemanticIssueKind::ReturnMismatch,
                    request: None,
                }));
            }
            // [REF-3] a result ordinal is owned, so a reference written at
            // one is the escape violation itself.
            self.reject_escaping_reference(&value, *expression_node)?;
            effects = effects.union(value.effects);
            fields.push(value.expression);
        }
        let node_path = self.tree.path(node)?.clone();
        Ok(StatementResult {
            statement: CheckedStatement::Return {
                node_path: node_path.clone(),
                value: super::super::super::model::CheckedExpression::ConstructStruct {
                    carrier: node_path,
                    nominal,
                    fields,
                },
                drops: self.live_affine_drops(bindings, &HashSet::new(), node)?,
            },
            can_continue: false,
            effects,
            all_paths_deliver: true,
            direct_give: false,
            give_states: Vec::new(),
            break_states: Vec::new(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_propagate_let(
        &self,
        function: &FunctionSignature,
        let_statement: NodeId,
        propagate: NodeId,
        declaration: DeclarationId,
        binding: BindingId,
        bindings: &mut HashMap<DeclarationId, LocalBinding>,
        scope: ControlScope<'_>,
    ) -> Result<StatementResult, CheckStop> {
        let expression_node = self
            .tree
            .first_child_with(propagate, Production::Expr)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        // [TYPE-5] a `propagate_let_rhs` binder is derived from the
        // propagated Ok payload [ERR-3], so the operand carries no
        // expectation and the payload is read off its Result type below.
        let value = self.check_consuming_expression(
            function,
            expression_node,
            bindings,
            scope.loops.len(),
        )?;
        // [REF-1] a reference variable denotes the reference, and the storage
        // it names is reached only through `deref`, so a bare holder written
        // here is that missing step. A `Box` is not one of these at v0.60:
        // its content is the ordinary field `inner` [TYPE-9], so a `Box`
        // operand is [ERR-3]'s own wrong-operand rejection below.
        if value.mode != CheckedMode::Own {
            return self.issue_node(
                SemanticRule::Type7,
                expression_node,
                SemanticIssueKind::MissingDereference {
                    mechanical_fix: "write `deref(holder)`",
                },
            );
        }
        // [ERR-3] propagation is a consuming context, and [OWN-1] admits a
        // consume only for a place rooted in a live own-mode binding, which a
        // place written under a `deref` is not.
        if !self.is_copy_type(value.expression.ty())?
            && self.operand_is_written_under_deref(expression_node)?
        {
            return self.issue_node(
                SemanticRule::Own1,
                expression_node,
                SemanticIssueKind::MoveThroughReference {
                    mechanical_fix: super::super::references::OWN1_ROOTED_CONSUME,
                },
            );
        }
        let CheckedType::Nominal(result_nominal) = value.expression.ty() else {
            return self.invalid_propagation(propagate);
        };
        let Some(PreludeType::Result(ok_type, error_type)) = self.prelude_type(result_nominal)
        else {
            return self.invalid_propagation(propagate);
        };
        let CheckedType::Nominal(return_nominal) = function.result else {
            return self.invalid_propagation(propagate);
        };
        let Some(PreludeType::Result(_, return_error_type)) = self.prelude_type(return_nominal)
        else {
            return self.invalid_propagation(propagate);
        };
        if error_type != return_error_type {
            return self.invalid_propagation(propagate);
        }
        let error_drops = self.live_affine_drops(bindings, &HashSet::new(), propagate)?;
        if bindings
            .insert(
                declaration,
                LocalBinding {
                    binding,
                    declaration,
                    mode: CheckedMode::Own,
                    ty: ok_type,
                    live: true,
                    loop_depth: scope.loops.len(),
                    compiler_updated: false,
                    reference: None,
                    refinement_witnesses: Vec::new(),
                },
            )
            .is_some()
        {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        Ok(Self::continuing_statement(
            CheckedStatement::PropagateLet {
                node_path: self.tree.path(let_statement)?.clone(),
                binding,
                scrutinee: value.expression,
                result_nominal,
                return_nominal,
                ok_type,
                error_type,
                error_drops,
                context: PropagationContext {
                    function: function.name.clone(),
                    node_path: self.tree.path(propagate)?.clone(),
                },
            },
            value.effects,
        ))
    }

    /// Whether one written `expr` is a place whose `pbase` is a `deref`,
    /// which [REF-1] makes the storage a reference names rather than a place
    /// this function owns.
    fn operand_is_written_under_deref(&self, expression: NodeId) -> Result<bool, CheckStop> {
        let Some(atom) = self.tree.first_child_with(expression, Production::Atom)? else {
            return Ok(false);
        };
        let Some(place) = self.tree.first_child_with(atom, Production::Place)? else {
            return Ok(false);
        };
        let Some(pbase) = self.tree.first_child_with(place, Production::Pbase)? else {
            return Ok(false);
        };
        self.has_fixed(pbase, crate::FixedTerminal::Deref)
    }

    fn invalid_propagation<ResultValue>(&self, node: NodeId) -> Result<ResultValue, CheckStop> {
        self.issue_node(
            SemanticRule::Err3,
            node,
            SemanticIssueKind::InvalidPropagation,
        )
    }

    /// [TYPE-7]'s implicit read at return position: a live borrow-mode
    /// binding used where the written `rtype` requires its referent
    /// value is rejected citing TYPE-7, and [FN-1] forms no candidate for
    /// that use. TYPE-7's definition precedes OWN-1's spelling judgments,
    /// so this same-node rejection event is cited first
    /// [DIAG-1, `SemanticRule::definition_rank`].
    ///
    /// A `Box` is not a reference [TYPE-7], so a cell returned where its
    /// content type is declared is not an implicit read at all: it is the
    /// ordinary [FN-1] return type mismatch, whose fix names the content's
    /// own field step `move holder.inner` [TYPE-9].
    pub(super) fn check_return_implicit_read(
        &self,
        function: &FunctionSignature,
        expression_node: NodeId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<(), CheckStop> {
        if function.result_mode != CheckedMode::Own {
            return Ok(());
        }
        // An infix expression reads its operands as operands, not as the
        // returned value, so it has no implicit read at return position.
        let Some(atom) = self.tree.sole_expression_child(expression_node)? else {
            return Ok(());
        };
        if self.tree.production(atom)? != Production::Atom {
            return Ok(());
        }
        let Some(place) = self.tree.first_child_with(atom, Production::Place)? else {
            return Ok(());
        };
        let Some(pbase) = self.tree.first_child_with(place, Production::Pbase)? else {
            return Ok(());
        };
        if !self.tree.children(pbase)?.is_empty()
            || !self
                .tree
                .children_with(place, Production::Psuffix)?
                .is_empty()
        {
            return Ok(());
        }
        let usage = self.use_at(pbase, LexicalUseRole::PlaceBase)?;
        let ResolvedTarget::Source {
            declaration,
            class: DeclarationClass::Value,
        } = usage.target()
        else {
            return Ok(());
        };
        let Some(local) = bindings.get(&declaration) else {
            return Ok(());
        };
        if !local.live {
            return Ok(());
        }
        let referent = if local.mode == CheckedMode::Own {
            None
        } else {
            Some(local.ty)
        };
        if referent == Some(function.result) {
            return self.issue_node(
                SemanticRule::Type7,
                expression_node,
                SemanticIssueKind::MissingDereference {
                    mechanical_fix: "write `deref(holder)`",
                },
            );
        }
        Ok(())
    }
}
