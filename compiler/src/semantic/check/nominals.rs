use std::collections::HashSet;

use crate::{BuiltinPreludeId, SemanticCompilerFailure, UnsupportedSemanticFeature};

use super::super::model::{
    CheckedConst, CheckedConstructor, CheckedField, CheckedNominal, CheckedNominalKind,
    CheckedType, CheckedVariant, NominalId,
};
use super::{CheckStop, Checker, PendingNominal, PreludeType};

impl<'unit> Checker<'unit> {
    /// Appends one nominal instance, a change to the table.
    pub(super) fn push_nominal(&mut self, nominal: CheckedNominal) {
        self.nominals.push(nominal);
        self.nominal_table_changed();
    }

    /// Notes a change to the nominal table, which its layout recursion
    /// judgment must see.
    pub(super) fn nominal_table_changed(&mut self) {
        self.nominal_generation = self.nominal_generation.wrapping_add(1);
    }

    /// Rejects a nominal whose layout contains itself other than through a
    /// `Box`. The judgment reads only the table, so a table unchanged since
    /// it last found no recursion holds none now; the whole table is walked
    /// again only after an instance was appended or completed or a
    /// checkpoint restored.
    pub(super) fn reject_recursive_nominal_layouts(&self) -> Result<(), CheckStop> {
        if self.nominal_layouts_acyclic_at.get() == Some(self.nominal_generation) {
            return Ok(());
        }
        let mut colors = vec![0_u8; self.nominals.len()];
        for root in 0..self.nominals.len() {
            if colors[root] != 0 {
                continue;
            }
            colors[root] = 1;
            let mut stack = vec![(root, 0_usize, self.nominal_dependencies(root)?)];
            while let Some((current, next, dependencies)) = stack.last_mut() {
                if *next == dependencies.len() {
                    colors[*current] = 2;
                    stack.pop();
                    continue;
                }
                let dependency = dependencies[*next].0 as usize;
                *next += 1;
                match colors.get(dependency).copied() {
                    Some(0) => {
                        colors[dependency] = 1;
                        stack.push((dependency, 0, self.nominal_dependencies(dependency)?));
                    }
                    Some(1) => {
                        let node = stack
                            .iter()
                            .filter_map(|(index, _, _)| {
                                self.nominal_nodes.get(*index).copied().flatten()
                            })
                            .next()
                            .or_else(|| self.nominal_nodes.get(dependency).copied().flatten())
                            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                        return self
                            .unsupported(UnsupportedSemanticFeature::RecursiveNominalLayout, node);
                    }
                    Some(2) => {}
                    _ => return Err(SemanticCompilerFailure::InvalidResolution.into()),
                }
            }
        }
        self.nominal_layouts_acyclic_at
            .set(Some(self.nominal_generation));
        Ok(())
    }

    fn nominal_dependencies(&self, index: usize) -> Result<Vec<NominalId>, CheckStop> {
        let nominal = self
            .nominals
            .get(index)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let fields: Vec<&CheckedField> = match &nominal.kind {
            CheckedNominalKind::Struct { fields } => fields.iter().collect(),
            CheckedNominalKind::Enum { variants } => variants
                .iter()
                .flat_map(|variant| variant.fields.iter())
                .collect(),
            CheckedNominalKind::Box { .. } | CheckedNominalKind::Opaque => Vec::new(),
        };
        let mut pending: Vec<_> = fields.into_iter().map(|field| field.ty).collect();
        let mut visited = HashSet::new();
        let mut dependencies = Vec::new();
        while let Some(ty) = pending.pop() {
            if !visited.insert(ty) {
                continue;
            }
            match ty {
                CheckedType::Nominal(id) => {
                    if !matches!(self.nominal(id)?.kind, CheckedNominalKind::Box { .. }) {
                        dependencies.push(id);
                    }
                }
                CheckedType::Array { element, length } if length != CheckedConst::Value(0) => {
                    pending.push(self.element_type(element)?);
                }
                CheckedType::Window { element, .. } => {
                    pending.push(self.element_type(element)?);
                }
                // The target spells an empty array as [0 x i8], independently
                // of T. Fixed runs retain their payload's element layout even
                // at N0; descriptor-owned runs never embed their elements.
                _ => {}
            }
        }
        Ok(dependencies)
    }

    pub(super) fn nominal(&self, id: NominalId) -> Result<&CheckedNominal, CheckStop> {
        self.nominals
            .get(id.0 as usize)
            .ok_or_else(|| SemanticCompilerFailure::InvalidResolution.into())
    }

    /// [OWN-1] whether this type has the copy capability: every part it
    /// owns has it and its declaration does not remove it.
    ///
    /// A type parameter standing for itself is copy exactly when its written
    /// bound grants copy [PROV-6, FN-2], which is what admits the bare use
    /// and the duplication its body writes; a generic nominal is therefore
    /// judged per instance, from the arguments its fields carry.
    pub(super) fn is_copy_type(&self, ty: CheckedType) -> Result<bool, CheckStop> {
        let failure = std::cell::Cell::new(None);
        let answer = super::super::model::type_has_copy_capability(
            ty,
            &self.nominals,
            &self.elements.borrow(),
            &|declaration| match self.generic_parameter_class(declaration) {
                Ok(class) => Some(class == super::linearity::LinearityClass::Copy),
                Err(stop) => {
                    failure.set(Some(stop));
                    None
                }
            },
        );
        if let Some(stop) = failure.take() {
            return Err(stop);
        }
        answer.ok_or_else(|| SemanticCompilerFailure::InvalidResolution.into())
    }

    pub(super) fn prelude_type(&self, id: NominalId) -> Option<PreludeType> {
        self.prelude_types.get(id.0 as usize).copied().flatten()
    }

    /// The interned prelude instance, deferring when a derived type named one
    /// that is not interned yet.
    ///
    /// The checked arithmetic rows produce `Result<T, Overflow>` and
    /// `Result<T, DivError>` for a *derived* `T`, and after the annotation is
    /// deleted nothing writes that type, so the miss is recoverable rather
    /// than an error: the driver interns it and checks the function again.
    pub(super) fn prelude_nominal(&self, ty: PreludeType) -> Result<NominalId, CheckStop> {
        if let Some(id) = self.prelude_nominals.get(&ty) {
            return Ok(*id);
        }
        self.pending_nominals
            .borrow_mut()
            .push(PendingNominal::Prelude(ty));
        Err(CheckStop::DeferredNominal)
    }

    pub(super) fn intern_box_nominal(
        &mut self,
        referent: CheckedType,
    ) -> Result<NominalId, CheckStop> {
        if let Some(id) = self.box_nominals.get(&referent) {
            return Ok(*id);
        }
        let id = NominalId(
            u32::try_from(self.nominals.len())
                .map_err(|_| SemanticCompilerFailure::CounterOverflow)?,
        );
        let name = format!("box<{}>", self.checked_type_name(referent)?);
        self.push_nominal(CheckedNominal {
            id,
            name,
            kind: CheckedNominalKind::Box {
                referent,
                region: None,
                release: super::super::model::CheckedReleaseClass::General,
            },
            linear: false,
            // [PRE-1] `opaque nocopy struct Box<T>`.
            nocopy: true,
        });
        self.nominal_nodes.push(None);
        self.nominal_states.push(2);
        self.source_nominal_instances.push(None);
        self.prelude_types.push(None);
        if self.box_nominals.insert(referent, id).is_some() {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        Ok(id)
    }

    /// The ordered `(binder spelling, type)` rows of a `fn_decl`'s result
    /// list, or `None` when an ordinal's mode is one this compiler cannot
    /// carry in a result list yet.
    ///
    /// The mode judgment itself belongs to `build_function_signature`, which
    /// reports it at the offending `rtype`; this reader is the shared walk
    /// both the nominal pre-scan and the signature build take over the same
    /// ordinals [GRAM-2, CALL-4].
    pub(super) fn result_list_fields(
        &self,
        function: crate::syntax::NodeId,
        substitution: &super::generics::GenericSubstitution,
    ) -> Result<Option<Vec<(String, CheckedType)>>, CheckStop> {
        let mut rows = Vec::new();
        for binding in self
            .tree
            .children_with(function, crate::Production::ResultBinding)?
        {
            let [name] = self.tree.direct_identifiers(binding)?[..] else {
                return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
            };
            let name = std::str::from_utf8(self.tree.token_bytes(name)?)
                .map(str::to_owned)
                .map_err(|_| SemanticCompilerFailure::InvalidSourceEncoding)?;
            let rtype = self
                .tree
                .first_child_with(binding, crate::Production::Rtype)?
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            let (mode, ty) = self.parse_rtype_with(rtype, substitution)?;
            if mode != super::super::model::CheckedMode::Own {
                return Ok(None);
            }
            rows.push((name, ty));
        }
        Ok(Some(rows))
    }

    /// The compiler-owned nominal a `fn_decl`'s ordered result list denotes
    /// [GRAM-2, CALL-4].
    ///
    /// A declaration that writes two or more results hands its caller one
    /// value carrying them in written order, and every result ordinal is one
    /// field of it. Nothing else in the language changes: the callable
    /// boundary keeps exactly one result type, the ordinary transfer,
    /// ownership, and lowering rules apply to it, and a destructuring binder
    /// list is the projection that names the ordinals again at the caller.
    /// The name is spelled with parentheses so no source TYPEID can collide
    /// with it; nothing reads it but a diagnostic.
    /// The already-interned result-list nominal of one ordered result list,
    /// for the `&self` checking path that cannot intern one itself.
    pub(super) fn result_list_nominal(
        &self,
        results: &[(String, CheckedType)],
    ) -> Option<NominalId> {
        self.result_list_nominals.get(results).copied()
    }

    pub(super) fn intern_result_list_nominal(
        &mut self,
        results: &[(String, CheckedType)],
    ) -> Result<NominalId, CheckStop> {
        if let Some(id) = self.result_list_nominals.get(results) {
            return Ok(*id);
        }
        let id = NominalId(
            u32::try_from(self.nominals.len())
                .map_err(|_| SemanticCompilerFailure::CounterOverflow)?,
        );
        let mut rendered = Vec::with_capacity(results.len());
        let mut fields = Vec::with_capacity(results.len());
        for (name, ty) in results {
            rendered.push(format!("{name}: {}", self.checked_type_name(*ty)?));
            fields.push(CheckedField {
                name: name.clone(),
                ty: *ty,
                readonly: false,
            });
        }
        self.push_nominal(CheckedNominal {
            id,
            name: format!("({})", rendered.join(", ")),
            kind: CheckedNominalKind::Struct { fields },
            linear: false,
            nocopy: false,
        });
        self.nominal_nodes.push(None);
        self.nominal_states.push(2);
        self.source_nominal_instances.push(None);
        self.prelude_types.push(None);
        if self
            .result_list_nominals
            .insert(results.to_vec(), id)
            .is_some()
        {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        Ok(id)
    }

    pub(super) fn register_prelude_nominals(&mut self) -> Result<(), CheckStop> {
        self.intern_prelude_nominal(PreludeType::Overflow)?;
        self.intern_prelude_nominal(PreludeType::DivError)?;
        self.intern_prelude_nominal(PreludeType::NarrowError)?;
        Ok(())
    }

    pub(super) fn intern_prelude_nominal(
        &mut self,
        ty: PreludeType,
    ) -> Result<NominalId, CheckStop> {
        if let Some(id) = self.prelude_nominals.get(&ty) {
            return Ok(*id);
        }
        let id = NominalId(
            u32::try_from(self.nominals.len())
                .map_err(|_| SemanticCompilerFailure::CounterOverflow)?,
        );
        let (name, variants) = match ty {
            PreludeType::Option(value) => (
                format!("Option<{}>", self.checked_type_name(value)?),
                vec![
                    CheckedVariant {
                        name: "None".to_owned(),
                        constructor: CheckedConstructor::Prelude(BuiltinPreludeId::NONE),
                        tag: 0,
                        fields: Vec::new(),
                    },
                    CheckedVariant {
                        name: "Some".to_owned(),
                        constructor: CheckedConstructor::Prelude(BuiltinPreludeId::SOME),
                        tag: 1,
                        fields: vec![CheckedField {
                            name: "value".to_owned(),
                            ty: value,
                            readonly: false,
                        }],
                    },
                ],
            ),
            PreludeType::Result(ok, error) => (
                format!(
                    "Result<{}, {}>",
                    self.checked_type_name(ok)?,
                    self.checked_type_name(error)?
                ),
                vec![
                    CheckedVariant {
                        name: "Ok".to_owned(),
                        constructor: CheckedConstructor::Prelude(BuiltinPreludeId::OK),
                        tag: 0,
                        fields: vec![CheckedField {
                            name: "value".to_owned(),
                            ty: ok,
                            readonly: false,
                        }],
                    },
                    CheckedVariant {
                        name: "Err".to_owned(),
                        constructor: CheckedConstructor::Prelude(BuiltinPreludeId::ERR),
                        tag: 1,
                        fields: vec![CheckedField {
                            name: "error".to_owned(),
                            ty: error,
                            readonly: false,
                        }],
                    },
                ],
            ),
            PreludeType::Overflow => (
                "Overflow".to_owned(),
                vec![CheckedVariant {
                    name: "Overflow".to_owned(),
                    constructor: CheckedConstructor::Prelude(BuiltinPreludeId::OVERFLOW),
                    tag: 0,
                    fields: Vec::new(),
                }],
            ),
            PreludeType::DivError => (
                "DivError".to_owned(),
                vec![
                    CheckedVariant {
                        name: "DivideByZero".to_owned(),
                        constructor: CheckedConstructor::Prelude(BuiltinPreludeId::DIVIDE_BY_ZERO),
                        tag: 0,
                        fields: Vec::new(),
                    },
                    CheckedVariant {
                        name: "DivOverflow".to_owned(),
                        constructor: CheckedConstructor::Prelude(BuiltinPreludeId::DIV_OVERFLOW),
                        tag: 1,
                        fields: Vec::new(),
                    },
                ],
            ),
            PreludeType::NarrowError => (
                "NarrowError".to_owned(),
                vec![CheckedVariant {
                    name: "NarrowError".to_owned(),
                    constructor: CheckedConstructor::Prelude(BuiltinPreludeId::NARROW_ERROR),
                    tag: 0,
                    fields: Vec::new(),
                }],
            ),
        };
        self.push_nominal(CheckedNominal {
            id,
            name,
            kind: CheckedNominalKind::Enum { variants },
            linear: false,
            nocopy: false,
        });
        self.nominal_nodes.push(None);
        self.nominal_states.push(2);
        self.source_nominal_instances.push(None);
        self.prelude_types.push(Some(ty));
        if self.prelude_nominals.insert(ty, id).is_some() {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        }
        Ok(id)
    }
}
