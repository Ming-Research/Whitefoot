//! The IR nominals and elements the checked program's types lower to.
//!
//! Nominal identity is the checked source family plus its complete reclamation
//! graph. Equal layouts alone never merge types. Pair visitation makes cyclic
//! graphs finite without a depth limit or an acceptance heuristic.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::NominalId;
use crate::semantic::{CheckedNominalKind, CheckedProgramData};

use super::*;

pub(super) struct PhysicalTypeMap {
    pub(super) nominals: Vec<IrNominalId>,
    pub(super) elements: Vec<Option<IrElement>>,
}

/// Lower only elements reached by executable types. Generic proof replay can
/// leave symbolic entries in the checked table; they have no physical type.
/// Structural children are interned before parents, so ascending handle order
/// is a topological order even when nominal references close an owning graph.
pub(super) fn base_elements(
    data: &CheckedProgramData,
    nominals: &[IrNominalId],
) -> Result<(Vec<IrType>, Vec<Option<IrElement>>), LoweringFailure> {
    let mut pending = Vec::new();
    let mut needed = BTreeSet::new();
    for function in &data.functions {
        let (types, elements) = specialize::executable_storage(function);
        pending.extend(types);
        needed.extend(elements.into_iter().map(CheckedElement::index));
        needed.extend(
            function
                .parameters
                .iter()
                .filter_map(|parameter| parameter.range_element)
                .map(CheckedElement::index),
        );
    }
    pending.extend(data.constants.iter().map(|constant| constant.ty));
    // [REF-4, TYPE-8] a range parameter or value-if result carries its element
    // kind separately from `CheckedType`: the written type is the element
    // type, so the ordinary executable-type walk cannot discover the interned
    // handle. Seed those handles explicitly before building the physical map;
    // otherwise a range reaches lowering with a `None` element even when the
    // element itself is concrete.
    for index in needed.iter().copied() {
        pending.push(
            *data
                .elements
                .get(index)
                .ok_or(LoweringFailure::InvalidCheckedProgram)?,
        );
    }
    for nominal in data.nominals.iter().take(data.executable_nominal_count) {
        match &nominal.kind {
            CheckedNominalKind::Struct { fields } => {
                pending.extend(fields.iter().map(|field| field.ty))
            }
            CheckedNominalKind::Enum { variants } => pending.extend(
                variants
                    .iter()
                    .flat_map(|variant| &variant.fields)
                    .map(|field| field.ty),
            ),
            CheckedNominalKind::Box { referent, .. } => pending.push(*referent),
            CheckedNominalKind::Opaque => {}
        }
    }
    while let Some(ty) = pending.pop() {
        if let CheckedType::Array { element, .. }
        | CheckedType::Window { element, .. }
        | CheckedType::Buffer { element } = ty
            && needed.insert(element.index())
        {
            pending.push(
                *data
                    .elements
                    .get(element.index())
                    .ok_or(LoweringFailure::InvalidCheckedProgram)?,
            );
        }
    }
    let mut map = vec![None; data.elements.len()];
    let mut elements = Vec::new();
    let mut interned = HashMap::new();
    for index in needed {
        let ty = lower_type(
            TypeLowering {
                nominals,
                elements: &map,
            },
            data.elements[index],
        )?;
        map[index] = Some(intern_element(&mut elements, &mut interned, ty)?);
    }
    Ok((elements, map))
}

fn intern_element(
    elements: &mut Vec<IrType>,
    interned: &mut HashMap<IrType, IrElement>,
    ty: IrType,
) -> Result<IrElement, LoweringFailure> {
    if let Some(id) = interned.get(&ty) {
        return Ok(*id);
    }
    let id =
        IrElement(u32::try_from(elements.len()).map_err(|_| LoweringFailure::CounterOverflow)?);
    elements.push(ty);
    interned.insert(ty, id);
    Ok(id)
}

struct Instance {
    source: NominalId,
    id: IrNominalId,
}

pub(super) struct PhysicalTypes<'a> {
    data: &'a CheckedProgramData,
    pub(super) nominals: Vec<IrNominal>,
    pub(super) elements: Vec<IrType>,
    base_elements: Vec<Option<IrElement>>,
    interned_elements: HashMap<IrType, IrElement>,
    element_instances: HashMap<CheckedElement, IrElement>,
    instances: Vec<Instance>,
}

impl<'a> PhysicalTypes<'a> {
    pub(super) fn new(
        data: &'a CheckedProgramData,
        nominals: Vec<IrNominal>,
        elements: Vec<IrType>,
        base_elements: Vec<Option<IrElement>>,
    ) -> Self {
        let instances = data
            .nominal_lowering_alias
            .iter()
            .enumerate()
            .take(data.executable_nominal_count)
            .filter(|(index, alias)| *index == alias.0 as usize)
            .map(|(_, alias)| Instance {
                source: *alias,
                id: IrNominalId(alias.0),
            })
            .collect();
        Self {
            data,
            nominals,
            interned_elements: elements
                .iter()
                .enumerate()
                .map(|(index, ty)| (*ty, IrElement(index as u32)))
                .collect(),
            elements,
            base_elements,
            element_instances: HashMap::new(),
            instances,
        }
    }

    pub(super) fn map(&mut self) -> Result<PhysicalTypeMap, LoweringFailure> {
        let nominals = (0..self.data.executable_nominal_count)
            .map(|index| {
                self.nominal(NominalId(
                    u32::try_from(index).map_err(|_| LoweringFailure::CounterOverflow)?,
                ))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut elements = vec![None; self.data.elements.len()];
        for (index, mapped) in elements.iter_mut().enumerate() {
            if self.base_elements[index].is_some() {
                *mapped = Some(self.element(CheckedElement(
                    u32::try_from(index).map_err(|_| LoweringFailure::CounterOverflow)?,
                ))?);
            }
        }
        Ok(PhysicalTypeMap { nominals, elements })
    }

    fn element(&mut self, source: CheckedElement) -> Result<IrElement, LoweringFailure> {
        if let Some(id) = self.element_instances.get(&source) {
            return Ok(*id);
        }
        let ty = *self
            .data
            .elements
            .get(source.index())
            .ok_or(LoweringFailure::InvalidCheckedProgram)?;
        let ty = self.ty(ty)?;
        let id = intern_element(&mut self.elements, &mut self.interned_elements, ty)?;
        self.element_instances.insert(source, id);
        Ok(id)
    }

    fn family(&self, source: NominalId) -> Result<NominalId, LoweringFailure> {
        self.data
            .nominal_physical_alias
            .get(source.0 as usize)
            .copied()
            .ok_or(LoweringFailure::InvalidCheckedProgram)
    }

    fn nominal(&mut self, source: NominalId) -> Result<IrNominalId, LoweringFailure> {
        let family = self.family(source)?;
        for candidate in &self.instances {
            if self.family(candidate.source)? == family
                && self.same_graph(source, candidate.source)?
            {
                return Ok(candidate.id);
            }
        }
        let id = IrNominalId(
            u32::try_from(self.nominals.len()).map_err(|_| LoweringFailure::CounterOverflow)?,
        );
        let mut nominal = self
            .nominals
            .get(source.0 as usize)
            .cloned()
            .ok_or(LoweringFailure::InvalidCheckedProgram)?;
        nominal.id = id;
        // Publish the identity before visiting children, so recursive fields
        // refer to this same instance. The placeholder is closed before use.
        self.nominals.push(nominal);
        self.instances.push(Instance { source, id });
        let kind = self
            .data
            .nominals
            .get(source.0 as usize)
            .ok_or(LoweringFailure::InvalidCheckedProgram)?
            .kind
            .clone();
        let lowered = match kind {
            CheckedNominalKind::Struct { fields } => IrNominalKind::Struct {
                fields: fields
                    .iter()
                    .map(|field| {
                        Ok(IrField {
                            ty: self.ty(field.ty)?,
                        })
                    })
                    .collect::<Result<_, LoweringFailure>>()?,
            },
            CheckedNominalKind::Enum { variants } => IrNominalKind::Enum {
                variants: variants
                    .iter()
                    .map(|variant| {
                        Ok(IrVariant {
                            tag: variant.tag,
                            fields: variant
                                .fields
                                .iter()
                                .map(|field| {
                                    Ok(IrField {
                                        ty: self.ty(field.ty)?,
                                    })
                                })
                                .collect::<Result<_, LoweringFailure>>()?,
                        })
                    })
                    .collect::<Result<_, LoweringFailure>>()?,
            },
            CheckedNominalKind::Box {
                referent, release, ..
            } => IrNominalKind::Box {
                referent: self.ty(referent)?,
                release: lower_release_class(release),
            },
            CheckedNominalKind::Opaque => self.nominals[id.index()].kind.clone(),
        };
        self.nominals[id.index()].kind = lowered;
        Ok(id)
    }

    fn ty(&mut self, ty: CheckedType) -> Result<IrType, LoweringFailure> {
        match ty {
            CheckedType::Buffer { element } => {
                return Ok(IrType::Buffer {
                    element: self.element(element)?,
                });
            }
            CheckedType::Array { element, length } => {
                return Ok(IrType::Array {
                    element: self.element(element)?,
                    length: length
                        .value()
                        .ok_or(LoweringFailure::InvalidCheckedProgram)?,
                });
            }
            CheckedType::Window {
                shape,
                element,
                capacity,
            } => {
                return Ok(IrType::Window {
                    shape: lower_window_shape(shape),
                    element: self.element(element)?,
                    capacity: capacity
                        .map(|capacity| {
                            capacity
                                .value()
                                .ok_or(LoweringFailure::InvalidCheckedProgram)
                        })
                        .transpose()?,
                });
            }
            _ => {}
        }
        let mut map = self
            .data
            .nominal_lowering_alias
            .iter()
            .map(|id| IrNominalId(id.0))
            .collect::<Vec<_>>();
        if let CheckedType::Nominal(id) = ty {
            map[id.0 as usize] = self.nominal(id)?;
        }
        lower_type(
            TypeLowering {
                nominals: &map,
                elements: &[],
            },
            ty,
        )
    }

    fn same_graph(&self, left: NominalId, right: NominalId) -> Result<bool, LoweringFailure> {
        let mut pending = vec![(CheckedType::Nominal(left), CheckedType::Nominal(right))];
        let mut visited = HashSet::new();
        while let Some((left, right)) = pending.pop() {
            if !visited.insert((left, right)) {
                continue;
            }
            match (left, right) {
                (CheckedType::Nominal(left), CheckedType::Nominal(right)) => {
                    if self.family(left)? != self.family(right)? {
                        return Ok(false);
                    }
                    let left = &self
                        .data
                        .nominals
                        .get(left.0 as usize)
                        .ok_or(LoweringFailure::InvalidCheckedProgram)?
                        .kind;
                    let right = &self
                        .data
                        .nominals
                        .get(right.0 as usize)
                        .ok_or(LoweringFailure::InvalidCheckedProgram)?
                        .kind;
                    match (left, right) {
                        (
                            CheckedNominalKind::Box {
                                referent: lt,
                                release: lc,
                                ..
                            },
                            CheckedNominalKind::Box {
                                referent: rt,
                                release: rc,
                                ..
                            },
                        ) => {
                            if lc != rc {
                                return Ok(false);
                            }
                            pending.push((*lt, *rt));
                        }
                        (
                            CheckedNominalKind::Struct { fields: left },
                            CheckedNominalKind::Struct { fields: right },
                        ) => {
                            if left.len() != right.len() {
                                return Ok(false);
                            }
                            pending.extend(
                                left.iter()
                                    .zip(right)
                                    .map(|(left, right)| (left.ty, right.ty)),
                            );
                        }
                        (
                            CheckedNominalKind::Enum { variants: left },
                            CheckedNominalKind::Enum { variants: right },
                        ) => {
                            if left.len() != right.len() {
                                return Ok(false);
                            }
                            for (left, right) in left.iter().zip(right) {
                                if left.tag != right.tag || left.fields.len() != right.fields.len()
                                {
                                    return Ok(false);
                                }
                                pending.extend(
                                    left.fields
                                        .iter()
                                        .zip(&right.fields)
                                        .map(|(left, right)| (left.ty, right.ty)),
                                );
                            }
                        }
                        (CheckedNominalKind::Opaque, CheckedNominalKind::Opaque) => {}
                        _ => return Ok(false),
                    }
                }
                (
                    CheckedType::Window {
                        shape: ls,
                        element: left,
                        capacity: ln,
                    },
                    CheckedType::Window {
                        shape: rs,
                        element: right,
                        capacity: rn,
                    },
                ) if ls == rs && ln == rn => pending.push((
                    *self
                        .data
                        .elements
                        .get(left.index())
                        .ok_or(LoweringFailure::InvalidCheckedProgram)?,
                    *self
                        .data
                        .elements
                        .get(right.index())
                        .ok_or(LoweringFailure::InvalidCheckedProgram)?,
                )),
                (
                    CheckedType::Array {
                        element: left,
                        length: ln,
                    },
                    CheckedType::Array {
                        element: right,
                        length: rn,
                    },
                ) if ln == rn => pending.push((
                    *self
                        .data
                        .elements
                        .get(left.index())
                        .ok_or(LoweringFailure::InvalidCheckedProgram)?,
                    *self
                        .data
                        .elements
                        .get(right.index())
                        .ok_or(LoweringFailure::InvalidCheckedProgram)?,
                )),
                (CheckedType::Buffer { element: left }, CheckedType::Buffer { element: right }) => {
                    pending.push((
                        *self
                            .data
                            .elements
                            .get(left.index())
                            .ok_or(LoweringFailure::InvalidCheckedProgram)?,
                        *self
                            .data
                            .elements
                            .get(right.index())
                            .ok_or(LoweringFailure::InvalidCheckedProgram)?,
                    ));
                }
                _ if left == right => {}
                _ => return Ok(false),
            }
        }
        Ok(true)
    }
}
