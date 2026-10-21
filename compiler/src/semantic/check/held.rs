//! Ordinary structural callable-held contracts [HOLD-1].
use std::collections::{HashMap, HashSet};

use crate::{DeclarationId, DeferredUseRole, Production, SemanticCompilerFailure, SemanticIssueKind, SemanticRule};
use crate::syntax::NodeId;
use super::{CheckStop, Checker, EffectSet, FunctionSignature, LocalBinding, ParameterSignature};
use super::super::model::{CheckedCallSeparation, CheckedHeldProtectionUse, CheckedMode, CheckedNominalKind, CheckedReleaseClass, CheckedShared, CheckedType};
use super::generics::{GenericArgument, GenericSubstitution};
use super::super::places::{CapturedRange, CapturedTerm, CapturedValue, PlaceRoot, PlaceStep, ResolvedPlace, SeparationOracle, places_overlap};

/// HOLD-1 compares captures from every state in the block, not two places
/// known live against one length. Keep OWN-7's structural/value separations,
/// but supply no state-relative WIN-2 separation.
struct BlockSeparations;

impl SeparationOracle for BlockSeparations {
    fn indices_distinct(&self, _left: CapturedValue, _right: CapturedValue) -> bool { false }
    fn ranges_disjoint(&self, _left: CapturedRange, _right: CapturedRange) -> bool { false }
    fn index_outside_range(&self, _index: CapturedValue, _range: CapturedRange) -> bool { false }
    fn index_is_live(&self, _window: &ResolvedPlace, _index: CapturedValue) -> bool { false }
    fn range_within_length(&self, _window: &ResolvedPlace, _range: CapturedRange) -> bool { false }
    fn window_length_is_shared(&self, _window: &ResolvedPlace) -> bool { false }
}

pub(super) const REPAIR: &str = "declare one nonempty held group of distinct reference parameters on a nonwaiting callable with a pure or reads row; pass references from one atomic block or the enclosing declared held group, and prove writes disjoint from their protected regions or move those writes outside the block";

impl FunctionSignature {
    pub(super) fn checked_held_parameters(&self) -> Result<Vec<u32>, CheckStop> {
        self.held_parameters.iter()
            .map(|ordinal| u32::try_from(*ordinal).map_err(|_| SemanticCompilerFailure::CounterOverflow.into()))
            .collect()
    }
}

impl Checker<'_, '_> {
    /// A linked generic boundary can own an aggregate whose representation
    /// is fixed independently of its symbolic parameters. Pointer slots
    /// remain fixed; inline generic payloads do not.
    pub(super) fn fixed_host_abi_type(&self, mode: CheckedMode, ty: CheckedType) -> Result<bool, CheckStop> {
        if super::generics::generic_host_abi_type(mode, ty) { return Ok(true); }
        if mode != CheckedMode::Own { return Ok(false); }
        self.fixed_host_layout(ty, &mut HashSet::new())
    }

    fn fixed_host_layout(&self, ty: CheckedType, seen: &mut HashSet<CheckedType>) -> Result<bool, CheckStop> {
        if !seen.insert(ty) { return Ok(true); }
        Ok(match ty {
            CheckedType::Generic(_) | CheckedType::GenericInt(_) | CheckedType::GenericFloat(_) => false,
            CheckedType::Nominal(id) => match &self.types.nominal(id)?.kind {
                CheckedNominalKind::Struct { fields } => {
                    for field in fields { if !self.fixed_host_layout(field.ty, seen)? { return Ok(false); } }
                    true
                }
                CheckedNominalKind::Enum { variants } => {
                    for variant in variants { for field in &variant.fields { if !self.fixed_host_layout(field.ty, seen)? { return Ok(false); } } }
                    true
                }
                CheckedNominalKind::Box { .. } | CheckedNominalKind::Shared { .. } | CheckedNominalKind::Opaque => true,
            },
            CheckedType::Array { element, length } => length.is_concrete() && self.fixed_host_layout(self.types.element_type(element)?, seen)?,
            CheckedType::Window { element, capacity: Some(capacity), .. } => capacity.is_concrete() && self.fixed_host_layout(self.types.element_type(element)?, seen)?,
            // Runtime storage owns descriptors, independently of slot layout.
            CheckedType::Buffer { .. } | CheckedType::Window { capacity: None, .. } | CheckedType::Segments { .. } | CheckedType::Entries { .. } | CheckedType::KeySet => true,
            CheckedType::Unit | CheckedType::Bool | CheckedType::Integer(_) | CheckedType::Float(_) => true,
        })
    }

    pub(super) fn qualify_snapshot_selection(&self, node: NodeId, declaration: DeclarationId, substitution: &GenericSubstitution) -> Result<(), CheckStop> {
        if !self.is_snapshot_export(declaration)? { return Ok(()); }
        let ty = substitution.entries().iter().find_map(|(_, argument)| match argument { GenericArgument::Type(ty) => Some(*ty), _ => None }).ok_or(SemanticCompilerFailure::InvalidResolution)?;
        self.snapshot_graph(node, ty, "value".to_owned(), &mut HashSet::new())
    }

    pub(super) fn is_snapshot_export(&self, declaration: DeclarationId) -> Result<bool, CheckStop> {
        // SNAP-1 qualifies the resolved PRE-2 item, never a caller's spelling
        // or an enclosing item's name. MOD-4 aliases retain this same key.
        let record = self.types.declarations.resolved.declaration(declaration)
            .ok_or(SemanticCompilerFailure::InvalidResolution)?;
        Ok(matches!(record.key(), crate::DeclarationKey::Item(crate::ItemKey::Declared {
            home: crate::ItemHome::Module {
                package: crate::PackageKey::Standard,
                path,
                record: crate::SourceRole::Interface,
            },
            role: crate::DeclarationRole::Function,
            spelling,
        }) if path.len() == 1 && path[0] == "snapshot" && spelling == "capture_export"))
    }

    fn snapshot_graph(&self, node: NodeId, ty: CheckedType, path: String, seen: &mut HashSet<CheckedType>) -> Result<(), CheckStop> {
        if !seen.insert(ty) { return Ok(()); }
        match ty {
            CheckedType::Unit | CheckedType::Bool | CheckedType::Integer(_) | CheckedType::Float(_) => Ok(()),
            // A written generic body retains the graph obligation; every
            // concrete selection runs this same walk on its substitution.
            CheckedType::Generic(_) | CheckedType::GenericInt(_) | CheckedType::GenericFloat(_) => Ok(()),
            CheckedType::Array { element, .. } | CheckedType::Buffer { element } | CheckedType::Window { element, .. } | CheckedType::Segments { element } => self.snapshot_graph(node, self.types.element_type(element)?, format!("{path}.element"), seen),
            CheckedType::Nominal(id) => {
                let nominal = self.types.nominal(id)?;
                match &nominal.kind {
                    CheckedNominalKind::Struct { fields } => {
                        for field in fields { self.snapshot_graph(node, field.ty, format!("{path}.{}", field.name), seen)?; }
                        Ok(())
                    }
                    CheckedNominalKind::Enum { variants } => {
                        for variant in variants { for field in &variant.fields { self.snapshot_graph(node, field.ty, format!("{path}.{}.{}", variant.name, field.name), seen)?; } }
                        Ok(())
                    }
                    CheckedNominalKind::Box { referent, release: CheckedReleaseClass::General, .. } => self.snapshot_graph(node, *referent, format!("{path}.inner"), seen),
                    _ => self.snapshot_resource(node, format!("{path}: {}", nominal.name)),
                }
            }
            CheckedType::KeySet | CheckedType::Entries { .. } => self.snapshot_resource(node, format!("{path}: keyed shared storage")),
        }
    }

    fn snapshot_resource(&self, node: NodeId, owned_path: String) -> Result<(), CheckStop> {
        self.types.declarations.issue_node(SemanticRule::Snap1, node, SemanticIssueKind::SnapshotResource { owned_path, mechanical_fix: "select a value type whose complete owned graph contains only primitives, declared structs and enums, storage shapes and owned Box contents" })
    }
    pub(super) fn held_failure<T>(&self, node: NodeId, reason: String) -> Result<T, CheckStop> {
        self.types.declarations.issue_node(SemanticRule::Hold1, node, SemanticIssueKind::InvalidHeldContract { reason, mechanical_fix: REPAIR })
    }

    pub(super) fn form_held_parameters(&self, node: NodeId, parameters: &[ParameterSignature], waits: bool, effects: &EffectSet) -> Result<Vec<usize>, CheckStop> {
        let Some(block) = self.types.declarations.tree.first_child_with(node, Production::ContractBlock)? else { return Ok(Vec::new()); };
        let mut group = Vec::new();
        let mut found = false;
        for clause in self.types.declarations.tree.children_with(block, Production::RequiresClause)? {
            let Some(held) = self.types.declarations.tree.first_child_with(clause, Production::HeldClause)? else { continue; };
            if found { return self.held_failure(clause, "the callable declares more than one held group".to_owned()); }
            found = true;
            if waits || !effects.writes.is_empty() { return self.held_failure(clause, "the callable waits or its row writes reference state".to_owned()); }
            for usage in self.types.declarations.resolved.deferred_uses_at(held).filter(|usage| usage.role() == DeferredUseRole::HeldParameter) {
                let Some(ordinal) = parameters.iter().position(|parameter| parameter.name == usage.spelling()) else {
                    return self.held_failure(clause, format!("{} does not name a parameter of this callable", usage.spelling()));
                };
                if !parameters[ordinal].mode.is_reference() || group.contains(&ordinal) {
                    return self.held_failure(clause, format!("{} is not a distinct reference parameter", usage.spelling()));
                }
                group.push(ordinal);
            }
            if group.is_empty() { return self.held_failure(clause, "the held group is empty".to_owned()); }
        }
        group.sort_unstable();
        Ok(group)
    }

    /// Origins are already flattened by REF-1, including aliases and joins.
    /// A local copy creates a new root and therefore cannot acquire this fact.
    pub(super) fn check_held_call(&mut self, node: NodeId, caller: &FunctionSignature, callee: &FunctionSignature, actuals: &[Vec<ResolvedPlace>], bindings: &HashMap<DeclarationId, LocalBinding>) -> Result<(), CheckStop> {
        if callee.held_parameters.is_empty() { return Ok(()); }
        if self.body.held_guard { return self.held_failure(node, "the call occurs in an atomic guard rather than its committed block".to_owned()); }
        let symbolic = caller.held_parameters.iter().filter_map(|ordinal| bindings.get(&caller.parameters[*ordinal].declaration).map(|local| local.binding)).collect::<HashSet<_>>();
        let mut common = None;
        let mut protected = Vec::new();
        for ordinal in &callee.held_parameters {
            let paths = actuals.get(*ordinal).ok_or(SemanticCompilerFailure::InvalidResolution)?;
            if paths.is_empty() { return self.held_failure(node, format!("{} has no held reference origin", callee.parameters[*ordinal].name)); }
            for path in paths {
                let origin = match path.root {
                    PlaceRoot::Binding(binding) if self.body.held_atomic_roots.contains(&binding) => Some(false),
                    PlaceRoot::Binding(binding) if symbolic.contains(&binding) => Some(true),
                    _ => None,
                };
                let Some(origin) = origin else {
                    let root = match path.root {
                        PlaceRoot::Binding(binding) => bindings.values().find(|local| local.binding == binding).ok_or(SemanticCompilerFailure::InvalidResolution)?.declaration,
                        PlaceRoot::Constant(constant) => self.types.checked_constants.get(constant.0 as usize).ok_or(SemanticCompilerFailure::InvalidResolution)?.declaration,
                    };
                    return self.held_failure(node, format!("{} has an alternative rooted at {}, outside the caller's held origins", callee.parameters[*ordinal].name, self.types.declarations.declaration_spelling(root)?));
                };
                if !self.within_held_cover(path, bindings)? {
                    return self.held_failure(node, format!("{} follows a handle beyond the protected owned region", callee.parameters[*ordinal].name));
                }
                if common.is_some_and(|previous| previous != origin) { return self.held_failure(node, "the argument origins belong to different held statements".to_owned()); }
                common = Some(origin);
                protected.push(path.clone());
            }
        }
        let block = self.types.declarations.tree.first_child_with(callee.node, Production::ContractBlock)?.ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let mut requirement = None;
        for clause in self.types.declarations.tree.children_with(block, Production::RequiresClause)? {
            if self.types.declarations.tree.first_child_with(clause, Production::HeldClause)?.is_some() {
                requirement = Some(clause);
                break;
            }
        }
        let requirement = requirement.ok_or(SemanticCompilerFailure::InvalidResolution)?;
        self.body.checked_held_calls.push(super::super::model::CheckedHeldCall {
            site: self.types.declarations.tree.path(node)?.clone(),
            requirement: self.types.declarations.tree.path(requirement)?.clone(),
            atomic: if common == Some(false) { self.body.held_atomic_node.clone() } else { None },
            protected: protected.clone(),
        });
        if common == Some(false) { self.body.held_calls.push((node, protected)); }
        Ok(())
    }

    fn within_held_cover(&self, path: &ResolvedPlace, bindings: &HashMap<DeclarationId, LocalBinding>) -> Result<bool, CheckStop> {
        let PlaceRoot::Binding(root) = path.root else { return Ok(false); };
        let local = bindings.values().find(|local| local.binding == root).ok_or(SemanticCompilerFailure::InvalidResolution)?;
        let mut ty = local.ty;
        let mut range = local.mode.is_range();
        for step in &path.path {
            if matches!(step, PlaceStep::Index(_) | PlaceStep::Page(_)) && range {
                range = false;
                continue;
            }
            if matches!(step, PlaceStep::Range(_)) {
                if !range {
                    ty = match ty {
                        CheckedType::Array { element, .. } | CheckedType::Buffer { element } | CheckedType::Window { element, .. } | CheckedType::Segments { element } => self.types.element_type(element)?,
                        _ => return Err(SemanticCompilerFailure::InvalidResolution.into()),
                    };
                }
                range = true;
                continue;
            }
            ty = match (*step, ty) {
                (PlaceStep::Descendant(target), _) => {
                    if target.frozen_content { return Ok(false); }
                    range = target.range;
                    target.ty
                }
                (PlaceStep::Field(field), CheckedType::Nominal(id)) => match &self.types.nominal(id)?.kind {
                    CheckedNominalKind::Struct { fields } => fields.get(field as usize).ok_or(SemanticCompilerFailure::InvalidResolution)?.ty,
                    _ => return Err(SemanticCompilerFailure::InvalidResolution.into()),
                },
                (PlaceStep::Payload { variant, field }, CheckedType::Nominal(id)) => match &self.types.nominal(id)?.kind {
                    CheckedNominalKind::Enum { variants } => variants.get(variant as usize).and_then(|variant| variant.fields.get(field as usize)).ok_or(SemanticCompilerFailure::InvalidResolution)?.ty,
                    _ => return Err(SemanticCompilerFailure::InvalidResolution.into()),
                },
                (PlaceStep::Deref, CheckedType::Nominal(id)) => match &self.types.nominal(id)?.kind {
                    CheckedNominalKind::Box { referent, release: CheckedReleaseClass::General, .. } => *referent,
                    CheckedNominalKind::Box { release: CheckedReleaseClass::Frozen, .. } | CheckedNominalKind::Shared { .. } => return Ok(false),
                    _ => return Err(SemanticCompilerFailure::InvalidResolution.into()),
                },
                (PlaceStep::Index(_) | PlaceStep::Page(_), CheckedType::Segments { element }) => {
                    range = true;
                    self.types.element_type(element)?
                }
                (PlaceStep::Index(_) | PlaceStep::Page(_), CheckedType::Array { element, .. } | CheckedType::Buffer { element } | CheckedType::Window { element, .. } | CheckedType::Entries { element }) => {
                    range = matches!(step, PlaceStep::Page(_));
                    self.types.element_type(element)?
                }
                (PlaceStep::Index(_), CheckedType::Nominal(id)) => match &self.types.nominal(id)?.kind {
                    CheckedNominalKind::Shared { shape: CheckedShared::Map { entry }, .. } => *entry,
                    _ => return Err(SemanticCompilerFailure::InvalidResolution.into()),
                },
                (PlaceStep::Part(_), ty) => ty,
                (PlaceStep::Measure(_), _) => return Ok(true),
                _ => return Err(SemanticCompilerFailure::InvalidResolution.into()),
            };
        }
        Ok(true)
    }

    /// Compare the complete conservative footprint before SHARE-2 frames it
    /// out. OWN-7 still owns field, payload, window and prefix separation.
    pub(super) fn record_held_write(&mut self, node: NodeId, place: &ResolvedPlace) {
        if self.body.atomic_depth > 0 {
            self.body.held_writes.push((node, place.clone()));
        }
    }

    /// A source occurrence inside a loop denotes many evaluations. Likewise,
    /// a mutable offset spelling cannot identify its value at two block states.
    /// Widen those captures before either structural separation or a proof
    /// request; unchanged ordinary bindings and value-determined offsets retain
    /// the existing ENT-6 admission.
    fn block_place(place: &ResolvedPlace, writes: &[(NodeId, ResolvedPlace)], bindings: &HashMap<DeclarationId, LocalBinding>) -> ResolvedPlace {
        let stable = |value: CapturedValue| match value.term {
            CapturedTerm::Literal(_) | CapturedTerm::Const(_) => value,
            CapturedTerm::Binding(binding) => {
                let local = bindings.values().find(|local| local.binding == binding);
                if local.is_some_and(|local| local.loop_depth == 0 && !local.compiler_updated)
                    && !writes.iter().any(|(_, write)| places_overlap(&BlockSeparations, &ResolvedPlace::binding(binding), write))
                { value } else { CapturedValue::unknown() }
            }
            CapturedTerm::Superseded(_)
            | CapturedTerm::Place(_)
            | CapturedTerm::SupersededPlace(_)
            | CapturedTerm::Opaque => CapturedValue::unknown(),
        };
        let mut result = place.clone();
        // A Descendant identity denotes a current target within one state,
        // not the same node at every loop iteration. Across the complete
        // block only its containing owned cover is stable: suffix fields
        // below two generations cannot establish sibling separation.
        if let Some(depth) = result.path.iter().position(|step| matches!(step, PlaceStep::Descendant(_))) {
            result.path.truncate(depth);
        }
        for step in &mut result.path {
            *step = match *step {
                PlaceStep::Index(value) => PlaceStep::Index(stable(value)),
                PlaceStep::Page(value) => PlaceStep::Page(stable(value)),
                PlaceStep::Range(range) => PlaceStep::Range(CapturedRange { start: stable(range.start), end: stable(range.end) }),
                other => other,
            };
        }
        result
    }

    /// Descriptor mutation can retarget a projection without writing its old
    /// logical index (a Ring head shift is one example). Element/descriptor
    /// separation remains correct for one state; it cannot establish stability
    /// of an address selected through that descriptor across this block.
    fn writes_selection_support(region: &ResolvedPlace, write: &ResolvedPlace) -> bool {
        for (depth, step) in region.path.iter().enumerate() {
            if !matches!(step, PlaceStep::Index(_) | PlaceStep::Page(_) | PlaceStep::Range(_) | PlaceStep::Part(_)) { continue; }
            for measure in [super::super::model::CheckedMeasure::Length, super::super::model::CheckedMeasure::Capacity, super::super::model::CheckedMeasure::Head] {
                let mut support = region.clone();
                support.path.truncate(depth);
                support.path.push(PlaceStep::Measure(measure));
                if places_overlap(&BlockSeparations, &support, write) { return true; }
            }
        }
        false
    }

    pub(super) fn check_held_protection(&mut self, writes: &[(NodeId, ResolvedPlace)], bindings: &HashMap<DeclarationId, LocalBinding>, calls: &[(NodeId, Vec<ResolvedPlace>)]) -> Result<(), CheckStop> {
        for (node, protected) in calls {
            let site = self.types.declarations.tree.path(*node)?.clone();
            let requirement = self.body.checked_held_calls.iter().find(|call| call.site == site).map(|call| call.requirement.clone()).ok_or(SemanticCompilerFailure::InvalidResolution)?;
            for (write_node, write) in writes {
                for region in protected {
                    let compared_region = Self::block_place(region, writes, bindings);
                    let compared_write = Self::block_place(write, writes, bindings);
                    let selection_changed = Self::writes_selection_support(&compared_region, &compared_write);
                    if !selection_changed && !places_overlap(&BlockSeparations, &compared_region, &compared_write) { continue; }
                    if let Some((positions, None)) = (!selection_changed).then(|| Self::separable_by_position(&compared_region, &compared_write)).flatten() {
                        let proof_site = if self.types.declarations.tree.coordinate(*write_node)?.start().value() > self.types.declarations.tree.coordinate(*node)?.start().value() { *write_node } else { *node };
                        self.body.call_separations.push(CheckedCallSeparation {
                            site: self.types.declarations.tree.path(proof_site)?.clone(),
                            occasion: super::super::model::CheckedSeparationOccasion::Call,
                            reference_use: None,
                            held_use: Some(CheckedHeldProtectionUse { site: site.clone(), requirement: requirement.clone() }),
                            positions,
                            contract_indices: Vec::new(),
                            window: None,
                            left_spelling: self.types.render_resolved_place(region, bindings)?,
                            right_spelling: self.types.render_resolved_place(write, bindings)?,
                            one_argument: false,
                        });
                        continue;
                    }
                    let requirement_node = self.types.declarations.tree.node_with_path(&requirement).ok_or(SemanticCompilerFailure::InvalidResolution)?;
                    return self.held_failure(*node, format!("{} protects {} which the complete atomic block writes through {}", self.types.declarations.tree.source_spelling(requirement_node)?, self.types.render_resolved_place(region, bindings)?, self.types.render_resolved_place(write, bindings)?));
                }
            }
        }
        Ok(())
    }
}
