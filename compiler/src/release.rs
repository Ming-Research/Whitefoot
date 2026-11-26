//! Shared [STOR-3, PROV-6] release-action classification before and after lowering.

use std::collections::HashSet;
use std::hash::Hash;

/// The ownership shape of a type, translated by each typed representation.
pub(crate) enum ReleaseShape<T> {
    Empty,
    Box,
    Shared,
    KeySet,
    Paged,
    Owned(Vec<T>),
    Repeated {
        element: T,
        statically_empty: bool,
        runtime_backing: bool,
    },
    Borrowed(T),
    Unknown {
        may_release: bool,
    },
}

/// Scope release is exact about empty runs and nonowning views. Scheduling
/// asks a broader question: can a write reclaim storage reached through this
/// type? It retains runtime backing, unknown types and zero-extent elements
/// conservatively, and follows borrowed entries only for reference writes.
#[derive(Clone, Copy)]
pub(crate) enum ReleaseQuery {
    Scope,
    StorageReclaim { through_views: bool },
}

/// Walk the type graph once, without a recursion or fuel limit. Both typed
/// adapters supply representation only; this walk owns action and edge policy.
pub(crate) fn has_release<T: Copy + Eq + Hash, E>(
    ty: T,
    query: ReleaseQuery,
    mut shape: impl FnMut(T) -> Result<ReleaseShape<T>, E>,
) -> Result<bool, E> {
    let mut pending = vec![ty];
    let mut visited = HashSet::new();
    while let Some(current) = pending.pop() {
        if !visited.insert(current) {
            continue;
        }
        match shape(current)? {
            ReleaseShape::Box
            | ReleaseShape::Shared
            | ReleaseShape::KeySet
            | ReleaseShape::Paged => return Ok(true),
            ReleaseShape::Owned(parts) => pending.extend(parts),
            ReleaseShape::Repeated {
                element,
                statically_empty,
                runtime_backing,
            } => match query {
                ReleaseQuery::StorageReclaim { .. } if runtime_backing => return Ok(true),
                ReleaseQuery::StorageReclaim { .. } => pending.push(element),
                ReleaseQuery::Scope if !statically_empty => pending.push(element),
                ReleaseQuery::Scope => {}
            },
            ReleaseShape::Borrowed(element) => {
                if matches!(
                    query,
                    ReleaseQuery::StorageReclaim {
                        through_views: true
                    }
                ) {
                    pending.push(element);
                }
            }
            ReleaseShape::Unknown { may_release } => {
                if may_release || matches!(query, ReleaseQuery::StorageReclaim { .. }) {
                    return Ok(true);
                }
            }
            ReleaseShape::Empty => {}
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{
        IrElement, IrField, IrNominal, IrNominalId, IrNominalKind, IrReleaseClass, IrShared,
        IrType, IrWindowShape, type_derives_release,
    };

    #[test]
    fn ir_release_actions_follow_ownership_and_multiplicity() {
        let kinds = [
            IrNominalKind::Opaque,
            IrNominalKind::Struct {
                fields: vec![IrField { ty: IrType::Bool }],
            },
            IrNominalKind::Box {
                referent: IrType::Bool,
                release: IrReleaseClass::General,
            },
            IrNominalKind::Box {
                referent: IrType::Bool,
                release: IrReleaseClass::Frozen,
            },
            IrNominalKind::Shared {
                state: IrType::Bool,
                shape: IrShared::Object,
            },
            IrNominalKind::Shared {
                state: IrType::Bool,
                shape: IrShared::Map {
                    entry: IrType::Bool,
                },
            },
            IrNominalKind::Struct {
                fields: vec![IrField { ty: IrType::KeySet }],
            },
            // Cycles terminate without hiding another owned action.
            IrNominalKind::Struct {
                fields: vec![IrField {
                    ty: IrType::Nominal(IrNominalId(7)),
                }],
            },
            IrNominalKind::Struct {
                fields: vec![
                    IrField { ty: IrType::KeySet },
                    IrField {
                        ty: IrType::Nominal(IrNominalId(8)),
                    },
                ],
            },
        ];
        let nominals = kinds
            .into_iter()
            .enumerate()
            .map(|(index, kind)| IrNominal {
                name: format!("type{index}"),
                link_name: format!("type{index}"),
                stable: None,
                id: IrNominalId(index as u32),
                kind,
                handler_words: 0,
            })
            .collect::<Vec<_>>();
        let elements = [IrType::KeySet, IrType::Bool];
        let release = |ty| type_derives_release(&nominals, &elements, ty);
        for (index, expected) in [false, false, true, true, true, true, true, false, true]
            .into_iter()
            .enumerate()
        {
            assert_eq!(
                release(IrType::Nominal(IrNominalId(index as u32))),
                Some(expected)
            );
        }
        assert_eq!(release(IrType::KeySet), Some(true));
        for length in [0, 1] {
            assert_eq!(
                release(IrType::Array {
                    element: IrElement(0),
                    length
                }),
                Some(length != 0)
            );
            for shape in [IrWindowShape::Slots, IrWindowShape::Ring] {
                assert_eq!(
                    release(IrType::Window {
                        shape,
                        element: IrElement(0),
                        capacity: Some(length)
                    }),
                    Some(length != 0)
                );
            }
        }
        assert_eq!(
            release(IrType::Window {
                shape: IrWindowShape::Paged,
                element: IrElement(1),
                capacity: None
            }),
            Some(true)
        );
        // Runtime blocks release elements; their enclosing Box frees the backing.
        assert_eq!(
            release(IrType::Buffer {
                element: IrElement(1)
            }),
            Some(false)
        );
        assert_eq!(
            release(IrType::Buffer {
                element: IrElement(0)
            }),
            Some(true)
        );
        assert_eq!(
            release(IrType::Entries {
                element: IrElement(0)
            }),
            Some(false)
        );
        assert_eq!(release(IrType::Nominal(IrNominalId(99))), None);
        assert_eq!(
            release(IrType::Array {
                element: IrElement(99),
                length: 1
            }),
            None
        );
    }

    #[test]
    fn scheduling_keeps_its_conservative_storage_footprint() {
        let query = |root, query| {
            has_release(root, query, |node| {
                Ok::<_, ()>(match node {
                    0 => ReleaseShape::Repeated {
                        element: 1,
                        statically_empty: true,
                        runtime_backing: false,
                    },
                    1 => ReleaseShape::KeySet,
                    2 => ReleaseShape::Borrowed(1),
                    3 => ReleaseShape::Unknown { may_release: false },
                    4 => ReleaseShape::Repeated {
                        element: 5,
                        statically_empty: false,
                        runtime_backing: true,
                    },
                    _ => ReleaseShape::Empty,
                })
            })
            .unwrap()
        };
        for root in [0, 2, 3, 4] {
            assert!(!query(root, ReleaseQuery::Scope));
            assert!(query(
                root,
                ReleaseQuery::StorageReclaim {
                    through_views: true
                }
            ));
        }
        assert!(!query(
            2,
            ReleaseQuery::StorageReclaim {
                through_views: false
            }
        ));
    }
}
