use std::collections::HashMap;

use crate::syntax::terminal::{FixedTerminal, TerminalPredicate};
use crate::syntax::{FinalizedExtent, FinalizedTopology, NodeId};
use crate::{ByteOffset, CanonicalSyntaxUnit, Production, SyntaxCoordinate};

use super::super::scopes::ScopeBuild;
use super::super::{
    DeclarationRole, DeferredUseRole, DependentDeclarationRole, LexicalUseRole,
    ResolutionCompilerFailure, SourceOrigin,
};
use super::{
    ClassifiedRole, EventKey, PathSegment, Qualifier, RawRole, RawRoleKind, SelectorRole,
    owner_chain,
};

pub(super) fn classify_roles(
    syntax: &CanonicalSyntaxUnit,
    scopes: &ScopeBuild,
) -> Result<Vec<ClassifiedRole>, ResolutionCompilerFailure> {
    let topology = &syntax.finalized.topology;
    let classified = syntax.classified_bundle();
    let mut raw = Vec::new();
    let mut complete_role_counts = vec![0_u8; topology.terminals.len()];
    let direct_terminals = direct_terminals_by_owner(topology)?;
    for index in 0..topology.nodes.len() {
        let node = NodeId::from_index(index).ok_or(ResolutionCompilerFailure::CounterOverflow)?;
        let record = topology
            .node(node)
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
        let direct = direct_terminals
            .get(index)
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
        classify_node(
            topology,
            classified,
            &direct_terminals,
            record.production,
            node,
            direct,
            &mut raw,
            &mut complete_role_counts,
        )?;
    }
    for (index, terminal) in topology.terminals.iter().enumerate() {
        let token = classified
            .tokens()
            .get(index)
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
        if terminal.owner.is_none() {
            return Err(ResolutionCompilerFailure::InvalidCanonicalTree);
        }
        let is_name = [
            TerminalPredicate::Identifier,
            TerminalPredicate::TypeIdentifier,
            TerminalPredicate::Label,
            TerminalPredicate::OperationName,
        ]
        .iter()
        .any(|predicate| token.terminals().contains(*predicate));
        if is_name && complete_role_counts.get(index) != Some(&1) {
            return Err(ResolutionCompilerFailure::UnclassifiedNameRole);
        }
    }

    let mut carrier_ordinals = HashMap::new();
    let mut carrier_rows: Vec<_> = raw
        .iter()
        .enumerate()
        .map(|(index, role)| {
            (
                role.owner.index(),
                role.source.ordinal(),
                role.carrier_start.value(),
                role.carrier_end.value(),
                role.kind.class_ordinal(),
                index,
            )
        })
        .collect();
    carrier_rows.sort_unstable();
    let mut previous = None;
    let mut ordinal = 0_u32;
    for (owner, source, start, end, _, raw_index) in carrier_rows {
        let carrier = (owner, source, start, end);
        if previous.is_some_and(|last| last != carrier) {
            ordinal = if previous.is_some_and(|last: (usize, u32, u64, u64)| last.0 == owner) {
                ordinal
                    .checked_add(1)
                    .ok_or(ResolutionCompilerFailure::CounterOverflow)?
            } else {
                0
            };
        } else if previous.is_none() {
            ordinal = 0;
        }
        carrier_ordinals.insert(raw_index, ordinal);
        previous = Some(carrier);
    }

    let mut roles = Vec::with_capacity(raw.len());
    for (index, role) in raw.into_iter().enumerate() {
        let role_ordinal = *carrier_ordinals
            .get(&index)
            .ok_or(ResolutionCompilerFailure::CounterOverflow)?;
        let scope = scopes.node_scope(role.owner)?;
        let coordinate = SyntaxCoordinate::new(role.source, role.role_start, role.role_end);
        // Presentation only: an owner without a source extent quotes the role.
        let extent = match topology.node(role.owner).map(|record| record.extent) {
            Some(FinalizedExtent::Source { source, start, end }) => {
                SyntaxCoordinate::new(source, start, end)
            }
            _ => coordinate,
        };
        roles.push(ClassifiedRole {
            kind: role.kind,
            spelling: role.spelling,
            qualifier: role.qualifier,
            member_owner: role.member_owner,
            owner: role.owner,
            origin: SourceOrigin {
                node: scopes.path(role.owner)?.clone(),
                coordinate,
                extent,
                role_ordinal,
                subtoken_ordinal: role.subtoken_ordinal,
            },
            scope,
            owner_chain: owner_chain(topology, role.owner)?,
        });
    }
    // PRE-1 declarations are supplied before writer declarations. Their
    // physical source records are appended only to preserve writer spans;
    // inventory order still makes a duplicate writer name cite the writer.
    roles.sort_by_key(|role| {
        let writer = classified
            .source_bundle()
            .file(role.origin.coordinate.source())
            .is_none_or(|file| file.prelude().is_none());
        (writer, EventKey::from_origin(&role.origin))
    });
    Ok(roles)
}

fn direct_terminals_by_owner(
    topology: &FinalizedTopology,
) -> Result<Vec<Vec<usize>>, ResolutionCompilerFailure> {
    let mut direct = vec![Vec::new(); topology.nodes.len()];
    for (terminal_index, terminal) in topology.terminals.iter().enumerate() {
        let owner = terminal
            .owner
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
        direct
            .get_mut(owner.index())
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?
            .push(terminal_index);
    }
    Ok(direct)
}

#[allow(clippy::too_many_arguments)]
fn classify_node(
    topology: &FinalizedTopology,
    classified: &crate::ClassifiedBundle,
    direct_table: &[Vec<usize>],
    production: Production,
    owner: NodeId,
    direct: &[usize],
    roles: &mut Vec<RawRole>,
    complete_counts: &mut [u8],
) -> Result<(), ResolutionCompilerFailure> {
    let names: Vec<_> = direct
        .iter()
        .copied()
        .filter(|index| name_predicate(classified, *index).is_some())
        .collect();
    match production {
        Production::FnDecl => add_single(
            classified,
            owner,
            &names,
            RawRoleKind::Declaration(DeclarationRole::Function),
            roles,
            complete_counts,
        )?,
        Production::StructDecl => add_single(
            classified,
            owner,
            &names,
            RawRoleKind::Declaration(DeclarationRole::Struct),
            roles,
            complete_counts,
        )?,
        Production::EnumDecl => add_single(
            classified,
            owner,
            &names,
            RawRoleKind::Declaration(DeclarationRole::Enum),
            roles,
            complete_counts,
        )?,
        Production::Variant => add_single(
            classified,
            owner,
            &names,
            RawRoleKind::Declaration(DeclarationRole::Variant),
            roles,
            complete_counts,
        )?,
        Production::InterfaceDecl => add_single(
            classified,
            owner,
            &names,
            RawRoleKind::Declaration(DeclarationRole::Interface),
            roles,
            complete_counts,
        )?,
        Production::ConstDecl => add_single(
            classified,
            owner,
            &names,
            RawRoleKind::Declaration(DeclarationRole::NamedConst),
            roles,
            complete_counts,
        )?,
        Production::Gparam => {
            if names.is_empty() {
                return Ok(());
            }
            let Some(first) = names.first().copied() else {
                return Err(ResolutionCompilerFailure::InvalidRoleShape);
            };
            match name_predicate(classified, first) {
                Some(TerminalPredicate::TypeIdentifier) => {
                    add_complete(
                        classified,
                        owner,
                        first,
                        RawRoleKind::Declaration(DeclarationRole::GenericType),
                        roles,
                        complete_counts,
                    )?;
                    if let Some(bound) = names.get(1).copied() {
                        add_complete(
                            classified,
                            owner,
                            bound,
                            RawRoleKind::LexicalUse(LexicalUseRole::GenericBound),
                            roles,
                            complete_counts,
                        )?;
                    }
                    if names.len() > 2 {
                        return Err(ResolutionCompilerFailure::InvalidRoleShape);
                    }
                }
                Some(TerminalPredicate::Identifier) if names.len() == 1 => {
                    add_complete(
                        classified,
                        owner,
                        first,
                        RawRoleKind::Declaration(DeclarationRole::ConstGeneric),
                        roles,
                        complete_counts,
                    )?;
                }
                _ => return Err(ResolutionCompilerFailure::InvalidRoleShape),
            }
        }
        Production::Param => add_single(
            classified,
            owner,
            &names,
            RawRoleKind::Declaration(DeclarationRole::Parameter),
            roles,
            complete_counts,
        )?,
        Production::InvariantStmt | Production::HeaderInvariant => {
            // The relation is a `compare_op` terminal between two affine
            // expressions, not a name; the only direct IDENT is the
            // invariant's own declaration.
            let [name] = names.as_slice() else {
                return Err(ResolutionCompilerFailure::InvalidRoleShape);
            };
            if name_predicate(classified, *name) != Some(TerminalPredicate::Identifier) {
                return Err(ResolutionCompilerFailure::InvalidRoleShape);
            }
            add_complete(
                classified,
                owner,
                *name,
                RawRoleKind::Declaration(DeclarationRole::Invariant),
                roles,
                complete_counts,
            )?;
        }
        // The premise of a `proof_use` is a `use_premise` node, so the two
        // productions each own at most one direct IDENT and they mean
        // different things: on the `proof_use` it is a term multiplicity,
        // `use n times X;`, which reads a runtime value in the proof domain;
        // on the `use_premise` it is the invariant the premise cites. A
        // bare-decimal multiplicity is not a name, and a relation premise has
        // no direct IDENT because its values sit inside the affine
        // expressions and take the `ProofValue` role below.
        Production::ProofUse | Production::UsePremise => {
            let role = if production == Production::ProofUse {
                LexicalUseRole::ProofValue
            } else {
                LexicalUseRole::InvariantFact
            };
            match names.as_slice() {
                [] => {}
                [name]
                    if name_predicate(classified, *name) == Some(TerminalPredicate::Identifier) =>
                {
                    add_complete(
                        classified,
                        owner,
                        *name,
                        RawRoleKind::LexicalUse(role),
                        roles,
                        complete_counts,
                    )?;
                }
                _ => return Err(ResolutionCompilerFailure::InvalidRoleShape),
            }
        }
        // [GRAM-4, MSR-5] `affine_factor` carries an `atom`, so an affine
        // relation's own IDENT is one `pbase` below it and takes its role in
        // the `pbase` arm; nothing is named directly here.
        Production::AffineFactor if !names.is_empty() => {
            return Err(ResolutionCompilerFailure::InvalidRoleShape);
        }
        // [PROV-6, GRAM-4] a destructuring consume writes the nominal it takes
        // apart where every other `let` writes its binders; the TYPEID is a use
        // of that nominal and the binders belong to the `fieldbind` list below.
        // [MOD-5] a destructuring consume may name its nominal through a
        // qualified path; its final TYPEID is the construct use.
        Production::LetStmt if child_with(topology, owner, Production::TypePath).is_some() => {
            let path = child_with(topology, owner, Production::TypePath)
                .ok_or(ResolutionCompilerFailure::InvalidRoleShape)?;
            add_path_use(
                classified,
                direct_table,
                owner,
                path,
                RawRoleKind::LexicalUse(LexicalUseRole::Construct),
                roles,
                complete_counts,
            )?;
        }
        Production::LetStmt
            if names
                .first()
                .copied()
                .and_then(|first| name_predicate(classified, first))
                == Some(TerminalPredicate::TypeIdentifier) =>
        {
            let [nominal] = names.as_slice() else {
                return Err(ResolutionCompilerFailure::InvalidRoleShape);
            };
            add_complete(
                classified,
                owner,
                *nominal,
                RawRoleKind::LexicalUse(LexicalUseRole::Construct),
                roles,
                complete_counts,
            )?;
        }
        // [GRAM-4] a `let` writes one binder or a parenthesized list of two or
        // more, and every binder of either form is one ordinary `let`
        // declaration in the statement's own scope [TYPE-6, CALL-4].
        Production::LetStmt => add_all(
            classified,
            owner,
            &names,
            RawRoleKind::Declaration(DeclarationRole::Let),
            roles,
            complete_counts,
        )?,
        Production::ContractDefine => add_single(
            classified,
            owner,
            &names,
            RawRoleKind::Declaration(DeclarationRole::Let),
            roles,
            complete_counts,
        )?,
        Production::ForBinding => add_single(
            classified,
            owner,
            &names,
            RawRoleKind::Declaration(DeclarationRole::CountedBinder),
            roles,
            complete_counts,
        )?,
        Production::LoopStmt | Production::ForStmt => match names.as_slice() {
            [] => {}
            [label] if name_predicate(classified, *label) == Some(TerminalPredicate::Label) => {
                add_complete(
                    classified,
                    owner,
                    *label,
                    RawRoleKind::Declaration(DeclarationRole::LoopLabel),
                    roles,
                    complete_counts,
                )?;
            }
            _ => return Err(ResolutionCompilerFailure::InvalidRoleShape),
        },
        Production::Field => add_single(
            classified,
            owner,
            &names,
            RawRoleKind::DependentDeclaration(DependentDeclarationRole::Field),
            roles,
            complete_counts,
        )?,
        Production::Vfield => add_single(
            classified,
            owner,
            &names,
            RawRoleKind::DependentDeclaration(DependentDeclarationRole::VariantField),
            roles,
            complete_counts,
        )?,
        Production::FnSig => add_single(
            classified,
            owner,
            &names,
            RawRoleKind::Declaration(
                if topology
                    .node(owner)
                    .and_then(|node| node.parent)
                    .and_then(|parent| topology.node(parent))
                    .is_some_and(|parent| parent.production == Production::Item)
                {
                    DeclarationRole::Function
                } else {
                    DeclarationRole::FunctionParameter
                },
            ),
            roles,
            complete_counts,
        )?,
        Production::ResultBinding => add_single(
            classified,
            owner,
            &names,
            RawRoleKind::Selector(SelectorRole::PlainCandidate),
            roles,
            complete_counts,
        )?,
        // [GRAM-2, CALL-4] a route is `when V(f: r):` or `when b is V(f: r):`,
        // where `b` names the result ordinal the route applies to. The ordinal
        // binder names a declared result and declares nothing, so it carries a
        // selector spelling exactly as a route's field name does.
        Production::ResultRoute => match names.as_slice() {
            [variant] => add_complete(
                classified,
                owner,
                *variant,
                RawRoleKind::LexicalUse(LexicalUseRole::EnsuresVariant),
                roles,
                complete_counts,
            )?,
            [ordinal, variant] => {
                if name_predicate(classified, *ordinal) != Some(TerminalPredicate::Identifier) {
                    return Err(ResolutionCompilerFailure::InvalidRoleShape);
                }
                add_complete(
                    classified,
                    owner,
                    *ordinal,
                    RawRoleKind::Selector(SelectorRole::ResultOrdinal),
                    roles,
                    complete_counts,
                )?;
                add_complete(
                    classified,
                    owner,
                    *variant,
                    RawRoleKind::LexicalUse(LexicalUseRole::EnsuresVariant),
                    roles,
                    complete_counts,
                )?;
            }
            _ => return Err(ResolutionCompilerFailure::InvalidRoleShape),
        },
        Production::Fieldbind => {
            if let [field, binder] = names.as_slice() {
                let selector_field =
                    ancestor_with_production(topology, owner, Production::ResultRoute).is_some();
                // [PROV-6, GRAM-4] a destructuring consume's binders are
                // ordinary `let` bindings of the enclosing block, exactly as
                // [CALL-4]'s binder list's are; only a `match` arm's binder is
                // arm-scoped and judged by [GRAM-10]'s freshness rule.
                //
                // Which of the two a `fieldbind` is, is decided by the
                // construct that owns its list and not by whether an arm is
                // anywhere above it: a destructuring consume written inside a
                // `match` arm has an `arm` ancestor and is still a `let`.
                let destructuring_binder = !selector_field
                    && nearest_binder_owner(topology, owner) == Some(Production::LetStmt);
                add_complete(
                    classified,
                    owner,
                    *field,
                    if selector_field {
                        RawRoleKind::Selector(SelectorRole::VariantField)
                    } else {
                        RawRoleKind::DeferredUse(DeferredUseRole::MatchField)
                    },
                    roles,
                    complete_counts,
                )?;
                add_complete(
                    classified,
                    owner,
                    *binder,
                    if selector_field {
                        RawRoleKind::Selector(SelectorRole::VariantCandidate)
                    } else if destructuring_binder {
                        RawRoleKind::Declaration(DeclarationRole::Let)
                    } else {
                        RawRoleKind::Declaration(DeclarationRole::MatchBinder)
                    },
                    roles,
                    complete_counts,
                )?;
            } else {
                return Err(ResolutionCompilerFailure::InvalidRoleShape);
            }
        }
        Production::Type if group_binder(topology, owner) && names.is_empty() => {}
        Production::Type if group_binder(topology, owner) => add_all(
            classified,
            owner,
            &names,
            RawRoleKind::Declaration(DeclarationRole::GenericType),
            roles,
            complete_counts,
        )?,
        // [GRAM-3, MOD-5] a qualified type writes its names in its child
        // `type_path`: the final TYPEID is this type's use and the root and
        // module segments before it select the module that declares it.
        Production::Type if child_with(topology, owner, Production::TypePath).is_some() => {
            let path = child_with(topology, owner, Production::TypePath)
                .ok_or(ResolutionCompilerFailure::InvalidRoleShape)?;
            add_path_use(
                classified,
                direct_table,
                owner,
                path,
                RawRoleKind::LexicalUse(
                    if parent_production(topology, owner) == Some(Production::Targ) {
                        LexicalUseRole::TypeArgument
                    } else {
                        LexicalUseRole::Type
                    },
                ),
                roles,
                complete_counts,
            )?;
        }
        // [GRAM-3] `type := <primitive> | TYPEID targs?`: a primitive writes
        // no name at all, and a nominal writes exactly one direct TYPEID,
        // every argument sitting below the child `targs`.
        Production::Type if names.is_empty() => {}
        Production::Type => add_single(
            classified,
            owner,
            &names,
            RawRoleKind::LexicalUse(
                if parent_production(topology, owner) == Some(Production::Targ) {
                    LexicalUseRole::TypeArgument
                } else {
                    LexicalUseRole::Type
                },
            ),
            roles,
            complete_counts,
        )?,
        Production::BindingDecl => add_single(
            classified,
            owner,
            &names,
            RawRoleKind::Declaration(DeclarationRole::Binding),
            roles,
            complete_counts,
        )?,
        // A `pack_use` in a callee chain is classified by that callee, which
        // alone knows whether a member follows it [GRAM-5, MOD-5].
        Production::PackUse
            if matches!(
                parent_production(topology, owner),
                Some(Production::Callee | Production::CalleePath)
            ) => {}
        Production::PackUse => add_single(
            classified,
            owner,
            &names,
            RawRoleKind::LexicalUse(LexicalUseRole::FormalGroup),
            roles,
            complete_counts,
        )?,
        // [MOD-5] a qualified group reference of a `binding_decl` or an
        // interface `gparam` carries its own use; a qualified type, callee,
        // construction or destructuring target is classified by its owner.
        Production::TypePath
            if matches!(
                parent_production(topology, owner),
                Some(Production::BindingDecl | Production::Gparam)
            ) =>
        {
            add_path_use(
                classified,
                direct_table,
                owner,
                owner,
                RawRoleKind::LexicalUse(LexicalUseRole::FormalGroup),
                roles,
                complete_counts,
            )?;
        }
        // [MOD-4] an alias declares its first name; the path after `pkg`
        // names its target, whose segments are resolved when the alias is.
        Production::AliasDecl => {
            let Some((first, rest)) = names.split_first() else {
                return Err(ResolutionCompilerFailure::InvalidRoleShape);
            };
            let segments = rest
                .iter()
                .map(|terminal| path_segment(classified, *terminal))
                .collect::<Result<Vec<_>, _>>()?;
            add_complete(
                classified,
                owner,
                *first,
                RawRoleKind::Declaration(DeclarationRole::Alias),
                roles,
                complete_counts,
            )?;
            if let Some(role) = roles.last_mut() {
                role.qualifier = Some(Qualifier {
                    alias_root: None,
                    standard: has_fixed_terminal(classified, direct, FixedTerminal::Std),
                    segments,
                });
            }
            for segment in rest {
                add_complete(
                    classified,
                    owner,
                    *segment,
                    RawRoleKind::Selector(SelectorRole::PathSegment),
                    roles,
                    complete_counts,
                )?;
            }
        }
        // [TYPE-6] an arm label resolves against the scrutinee's already
        // known enum type, so it waits for the checker.
        Production::Arm => add_single(
            classified,
            owner,
            &names,
            RawRoleKind::DeferredUse(DeferredUseRole::ArmVariant),
            roles,
            complete_counts,
        )?,
        // [EFF-1] `epbase := IDENT`: the reference parameter is the row's
        // root, with no source `deref` wrapper.
        Production::Epbase => add_single(
            classified,
            owner,
            &names,
            RawRoleKind::LexicalUse(LexicalUseRole::EffectRoot),
            roles,
            complete_counts,
        )?,
        // [EFF-1] `epsuffix := "." IDENT | "." TYPEID "." IDENT
        // | "[" IDENT erange? "]"`. The three alternatives are told apart by
        // the bracket and by the number of names, exactly as `psuffix`'s are.
        Production::Epsuffix => {
            let subscript = has_fixed_terminal(classified, direct, FixedTerminal::LeftBracket);
            classify_projection_names(
                classified,
                owner,
                &names,
                subscript,
                RawRoleKind::LexicalUse(LexicalUseRole::EffectIndex),
                RawRoleKind::DeferredUse(DeferredUseRole::EffectField),
                roles,
                complete_counts,
            )?;
        }
        // [EFF-1] `erange := ".." IDENT`: a range endpoint is one IDENT that
        // must resolve to a value parameter of the same callable.
        Production::Erange => add_single(
            classified,
            owner,
            &names,
            RawRoleKind::LexicalUse(LexicalUseRole::EffectIndex),
            roles,
            complete_counts,
        )?,
        Production::BreakStmt => match names.as_slice() {
            [] => {}
            [label] if name_predicate(classified, *label) == Some(TerminalPredicate::Label) => {
                add_complete(
                    classified,
                    owner,
                    *label,
                    RawRoleKind::LexicalUse(LexicalUseRole::BreakLabel),
                    roles,
                    complete_counts,
                )?;
            }
            _ => return Err(ResolutionCompilerFailure::InvalidRoleShape),
        },
        // Every IDENT term of a `const` expression is one Const use; the
        // candidate CONST-1 grammar admits two terms in one operation, and
        // source order is retained through the per-name role ordinal.
        Production::Const if !names.is_empty() => add_all(
            classified,
            owner,
            &names,
            if group_binder(topology, owner) {
                RawRoleKind::Declaration(DeclarationRole::ConstGeneric)
            } else {
                RawRoleKind::LexicalUse(LexicalUseRole::Const)
            },
            roles,
            complete_counts,
        )?,
        Production::Cvalue => {
            // The CONST-2 construction cvalue writes its constructor TYPEID,
            // or a type-owned variant as an owner TYPEID and a member TYPEID
            // [TYPE-6], either of them possibly through a qualified
            // `type_path` [MOD-5], plus its direct field labels; the
            // reference cvalue owns exactly one IDENT naming a const.
            let type_ids: Vec<_> = names
                .iter()
                .copied()
                .filter(|index| {
                    name_predicate(classified, *index) == Some(TerminalPredicate::TypeIdentifier)
                })
                .collect();
            let labels: Vec<_> = names
                .iter()
                .copied()
                .filter(|index| !type_ids.contains(index))
                .collect();
            let path = child_with(topology, owner, Production::TypePath);
            let constructor = match (path, type_ids.as_slice()) {
                (Some(path), []) => {
                    add_path_use(
                        classified,
                        direct_table,
                        owner,
                        path,
                        RawRoleKind::LexicalUse(LexicalUseRole::Construct),
                        roles,
                        complete_counts,
                    )?;
                    true
                }
                (Some(path), [member]) => {
                    let owner_coordinate = add_path_use(
                        classified,
                        direct_table,
                        owner,
                        path,
                        RawRoleKind::LexicalUse(LexicalUseRole::VariantOwner),
                        roles,
                        complete_counts,
                    )?;
                    add_member(
                        classified,
                        owner,
                        *member,
                        owner_coordinate,
                        roles,
                        complete_counts,
                    )?;
                    true
                }
                (None, [constructor]) => {
                    add_complete(
                        classified,
                        owner,
                        *constructor,
                        RawRoleKind::LexicalUse(LexicalUseRole::Construct),
                        roles,
                        complete_counts,
                    )?;
                    true
                }
                (None, [type_owner, member]) => {
                    add_complete(
                        classified,
                        owner,
                        *type_owner,
                        RawRoleKind::LexicalUse(LexicalUseRole::VariantOwner),
                        roles,
                        complete_counts,
                    )?;
                    let owner_coordinate = token_coordinate(classified, *type_owner)?;
                    add_member(
                        classified,
                        owner,
                        *member,
                        owner_coordinate,
                        roles,
                        complete_counts,
                    )?;
                    true
                }
                (None, []) => false,
                _ => return Err(ResolutionCompilerFailure::InvalidRoleShape),
            };
            if constructor {
                for label in labels {
                    add_complete(
                        classified,
                        owner,
                        label,
                        RawRoleKind::DeferredUse(DeferredUseRole::FieldInitializer),
                        roles,
                        complete_counts,
                    )?;
                }
            } else if !labels.is_empty() {
                add_single(
                    classified,
                    owner,
                    &labels,
                    RawRoleKind::LexicalUse(LexicalUseRole::ConstValue),
                    roles,
                    complete_counts,
                )?;
            }
        }
        Production::Pbase => {
            if !names.is_empty() {
                add_single(
                    classified,
                    owner,
                    &names,
                    RawRoleKind::LexicalUse(affine_atom_role(topology, owner)),
                    roles,
                    complete_counts,
                )?;
            }
        }
        Production::Callee => classify_callee(
            topology,
            classified,
            direct_table,
            owner,
            roles,
            complete_counts,
        )?,
        // Classified by the callee or owner that writes it.
        Production::CalleePath | Production::TypePath => {}
        Production::FnBind => {
            if let [member] = names.as_slice() {
                add_complete(
                    classified,
                    owner,
                    *member,
                    RawRoleKind::DeferredUse(DeferredUseRole::FunctionBinding),
                    roles,
                    complete_counts,
                )?;
            } else {
                return Err(ResolutionCompilerFailure::InvalidRoleShape);
            }
        }
        Production::Fieldinit => add_single(
            classified,
            owner,
            &names,
            RawRoleKind::DeferredUse(DeferredUseRole::FieldInitializer),
            roles,
            complete_counts,
        )?,
        // [GRAM-5] `psuffix := "." IDENT | "." TYPEID "." IDENT
        // | "[" atom range_tail? "]"`. The field alternative owns exactly one
        // projected-field name; the payload alternative owns the variant
        // TYPEID and that variant's field name, both deferred to the owner
        // type [DIAG-1]; the subscript alternative owns only bracket
        // punctuation, its offset and endpoint atoms classifying through the
        // `atom` productions below it.
        Production::Psuffix if !names.is_empty() => classify_projection_names(
            classified,
            owner,
            &names,
            false,
            RawRoleKind::DeferredUse(DeferredUseRole::ProjectedField),
            RawRoleKind::DeferredUse(DeferredUseRole::ProjectedField),
            roles,
            complete_counts,
        )?,
        _ => {}
    }
    if matches!(production, Production::Atom | Production::Cvalue) {
        for terminal in direct {
            add_generic_suffix(classified, owner, *terminal, roles)?;
        }
    }
    Ok(())
}

fn parent_production(topology: &FinalizedTopology, node: NodeId) -> Option<Production> {
    topology
        .node(node)?
        .parent
        .and_then(|parent| topology.node(parent))
        .map(|record| record.production)
}

/// The direct arguments of a parameter-group application declare names.
/// Nested applications remain uses and receive FN-3's flat-binder judgment.
/// An interface `gparam` writes its group either as a `pack_use` or, when the
/// group is qualified, as a `type_path` whose `targs` are the `gparam`'s own
/// children [MOD-5].
fn group_binder(topology: &FinalizedTopology, node: NodeId) -> bool {
    let parent_of = |node: NodeId| topology.node(node).and_then(|record| record.parent);
    let production_of = |node: NodeId| topology.node(node).map(|record| record.production);
    let Some(targ) =
        parent_of(node).filter(|parent| production_of(*parent) == Some(Production::Targ))
    else {
        return false;
    };
    let Some(targs) =
        parent_of(targ).filter(|parent| production_of(*parent) == Some(Production::Targs))
    else {
        return false;
    };
    let Some(application) = parent_of(targs) else {
        return false;
    };
    match production_of(application) {
        Some(Production::Gparam) => true,
        Some(Production::PackUse) => parent_of(application)
            .is_some_and(|gparam| production_of(gparam) == Some(Production::Gparam)),
        _ => false,
    }
}

/// The first direct child of `node` with this production.
fn child_with(
    topology: &FinalizedTopology,
    node: NodeId,
    production: Production,
) -> Option<NodeId> {
    topology.node_children(node)?.iter().copied().find(|child| {
        topology
            .node(*child)
            .is_some_and(|record| record.production == production)
    })
}

/// One name terminal's spelling and source coordinate.
fn path_segment(
    classified: &crate::ClassifiedBundle,
    terminal: usize,
) -> Result<PathSegment, ResolutionCompilerFailure> {
    let token = classified
        .tokens()
        .get(terminal)
        .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?
        .token();
    let bytes = classified
        .token_bytes(token)
        .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
    Ok(PathSegment {
        spelling: std::str::from_utf8(bytes)
            .map_err(|_| ResolutionCompilerFailure::InvalidNameEncoding)?
            .to_owned(),
        coordinate: token_coordinate(classified, terminal)?,
    })
}

fn token_coordinate(
    classified: &crate::ClassifiedBundle,
    terminal: usize,
) -> Result<SyntaxCoordinate, ResolutionCompilerFailure> {
    let id = classified
        .tokens()
        .get(terminal)
        .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?
        .token()
        .id();
    Ok(SyntaxCoordinate::new(id.source(), id.start(), id.end()))
}

/// Classifies one qualified path `("pkg" | IDENT) "::" (IDENT "::")* TYPEID`
/// [MOD-5]: its final TYPEID takes `kind` at `carrier` with the path's
/// qualifier, and its alias root and module segments are path selectors of
/// that same carrier. Returns the final TYPEID's coordinate.
fn add_path_use(
    classified: &crate::ClassifiedBundle,
    direct_table: &[Vec<usize>],
    carrier: NodeId,
    path: NodeId,
    kind: RawRoleKind,
    roles: &mut Vec<RawRole>,
    counts: &mut [u8],
) -> Result<SyntaxCoordinate, ResolutionCompilerFailure> {
    let direct = direct_table
        .get(path.index())
        .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
    let pkg_rooted = has_fixed_terminal(classified, direct, FixedTerminal::Pkg);
    let standard = has_fixed_terminal(classified, direct, FixedTerminal::Std);
    let names: Vec<_> = direct
        .iter()
        .copied()
        .filter(|index| name_predicate(classified, *index).is_some())
        .collect();
    let Some((last, prefix)) = names.split_last() else {
        return Err(ResolutionCompilerFailure::InvalidRoleShape);
    };
    let (alias_root, segments) = if pkg_rooted || standard {
        (None, prefix)
    } else {
        let Some((root, segments)) = prefix.split_first() else {
            return Err(ResolutionCompilerFailure::InvalidRoleShape);
        };
        (Some(path_segment(classified, *root)?), segments)
    };
    let qualifier = Qualifier {
        alias_root,
        standard,
        segments: segments
            .iter()
            .map(|terminal| path_segment(classified, *terminal))
            .collect::<Result<Vec<_>, _>>()?,
    };
    add_complete(classified, carrier, *last, kind, roles, counts)?;
    if let Some(role) = roles.last_mut() {
        role.qualifier = Some(qualifier);
    }
    for terminal in prefix {
        add_complete(
            classified,
            carrier,
            *terminal,
            RawRoleKind::Selector(SelectorRole::PathSegment),
            roles,
            counts,
        )?;
    }
    token_coordinate(classified, *last)
}

/// Adds the member TYPEID of a type-owned variant construction [TYPE-6]: a
/// construct use of `carrier` resolved among the variants of the owner whose
/// TYPEID sits at `owner`.
fn add_member(
    classified: &crate::ClassifiedBundle,
    carrier: NodeId,
    member: usize,
    owner: SyntaxCoordinate,
    roles: &mut Vec<RawRole>,
    counts: &mut [u8],
) -> Result<(), ResolutionCompilerFailure> {
    if name_predicate(classified, member) != Some(TerminalPredicate::TypeIdentifier) {
        return Err(ResolutionCompilerFailure::InvalidRoleShape);
    }
    add_complete(
        classified,
        carrier,
        member,
        RawRoleKind::LexicalUse(LexicalUseRole::Construct),
        roles,
        counts,
    )?;
    if let Some(role) = roles.last_mut() {
        role.member_owner = Some(owner);
    }
    Ok(())
}

/// Classifies one `callee` and its whole `callee_path` chain [GRAM-5, MOD-5].
///
/// The chain is `pkg`- or alias-rooted when it is qualified; its tail is the
/// innermost node, which writes either the final function IDENT or a
/// `pack_use` with an optional member. A function name is a use of the
/// callee; a bare `pack_use` is a construction of the enclosing `call`; a
/// `pack_use` with an IDENT member is a group whose member the checker
/// selects; and a `pack_use` with a TYPEID member is the owner enum of a
/// type-owned variant construction [TYPE-6].
fn classify_callee(
    topology: &FinalizedTopology,
    classified: &crate::ClassifiedBundle,
    direct_table: &[Vec<usize>],
    callee: NodeId,
    roles: &mut Vec<RawRole>,
    counts: &mut [u8],
) -> Result<(), ResolutionCompilerFailure> {
    let names_of = |node: NodeId| -> Result<Vec<usize>, ResolutionCompilerFailure> {
        Ok(direct_table
            .get(node.index())
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?
            .iter()
            .copied()
            .filter(|index| name_predicate(classified, *index).is_some())
            .collect())
    };
    let callee_direct = direct_table
        .get(callee.index())
        .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
    let callee_names = names_of(callee)?;
    let mut qualified = false;
    let mut alias_root = None;
    let mut segment_terminals = Vec::new();
    let mut tail = callee;
    let standard = has_fixed_terminal(classified, callee_direct, FixedTerminal::Std);
    if standard || has_fixed_terminal(classified, callee_direct, FixedTerminal::Pkg) {
        qualified = true;
        tail = child_with(topology, callee, Production::CalleePath)
            .ok_or(ResolutionCompilerFailure::InvalidRoleShape)?;
    } else if let Some(path) = child_with(topology, callee, Production::CalleePath) {
        let [root] = callee_names.as_slice() else {
            return Err(ResolutionCompilerFailure::InvalidRoleShape);
        };
        qualified = true;
        alias_root = Some(*root);
        tail = path;
    }
    while tail != callee {
        let Some(next) = child_with(topology, tail, Production::CalleePath) else {
            break;
        };
        let segment_names = names_of(tail)?;
        let [segment] = segment_names.as_slice() else {
            return Err(ResolutionCompilerFailure::InvalidRoleShape);
        };
        segment_terminals.push(*segment);
        tail = next;
    }
    let qualifier = if qualified {
        Some(Qualifier {
            alias_root: alias_root
                .map(|root| path_segment(classified, root))
                .transpose()?,
            standard,
            segments: segment_terminals
                .iter()
                .map(|terminal| path_segment(classified, *terminal))
                .collect::<Result<Vec<_>, _>>()?,
        })
    } else {
        None
    };
    let tail_names: Vec<usize> = if tail == callee {
        callee_names
    } else {
        names_of(tail)?
    };
    let with_qualifier = |roles: &mut Vec<RawRole>| {
        if let Some(role) = roles.last_mut() {
            role.qualifier.clone_from(&qualifier);
        }
    };
    if let Some(pack) = child_with(topology, tail, Production::PackUse) {
        let pack_names = names_of(pack)?;
        let [owner_name] = pack_names.as_slice() else {
            return Err(ResolutionCompilerFailure::InvalidRoleShape);
        };
        match tail_names.as_slice() {
            [] => {
                let call = topology
                    .node(callee)
                    .and_then(|record| record.parent)
                    .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
                add_complete(
                    classified,
                    call,
                    *owner_name,
                    RawRoleKind::LexicalUse(LexicalUseRole::Construct),
                    roles,
                    counts,
                )?;
                with_qualifier(roles);
            }
            [member]
                if name_predicate(classified, *member) == Some(TerminalPredicate::Identifier) =>
            {
                add_complete(
                    classified,
                    pack,
                    *owner_name,
                    RawRoleKind::LexicalUse(LexicalUseRole::FormalGroup),
                    roles,
                    counts,
                )?;
                with_qualifier(roles);
                add_complete(
                    classified,
                    callee,
                    *member,
                    RawRoleKind::DeferredUse(DeferredUseRole::FunctionMember),
                    roles,
                    counts,
                )?;
            }
            [member] => {
                let call = topology
                    .node(callee)
                    .and_then(|record| record.parent)
                    .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
                add_complete(
                    classified,
                    pack,
                    *owner_name,
                    RawRoleKind::LexicalUse(LexicalUseRole::VariantOwner),
                    roles,
                    counts,
                )?;
                with_qualifier(roles);
                add_member(
                    classified,
                    call,
                    *member,
                    token_coordinate(classified, *owner_name)?,
                    roles,
                    counts,
                )?;
            }
            _ => return Err(ResolutionCompilerFailure::InvalidRoleShape),
        }
    } else {
        let [name] = tail_names.as_slice() else {
            return Err(ResolutionCompilerFailure::InvalidRoleShape);
        };
        let use_role = match name_predicate(classified, *name) {
            Some(TerminalPredicate::Identifier) => {
                if matches!(
                    parent_production(topology, callee),
                    Some(Production::FunctionArg | Production::FnBind)
                ) {
                    LexicalUseRole::FunctionBinding
                } else {
                    LexicalUseRole::IdentifierCallee
                }
            }
            Some(TerminalPredicate::OperationName) if !qualified => LexicalUseRole::OperationCallee,
            _ => return Err(ResolutionCompilerFailure::InvalidRoleShape),
        };
        add_complete(
            classified,
            callee,
            *name,
            RawRoleKind::LexicalUse(use_role),
            roles,
            counts,
        )?;
        with_qualifier(roles);
    }
    for terminal in alias_root.iter().chain(&segment_terminals) {
        add_complete(
            classified,
            callee,
            *terminal,
            RawRoleKind::Selector(SelectorRole::PathSegment),
            roles,
            counts,
        )?;
    }
    Ok(())
}

/// The role one `pbase` IDENT takes [GRAM-4, GRAM-5].
///
/// A `pbase` directly below an `affine_factor` — through the factor's `atom`
/// and that atom's `place` — is one affine relation's own value atom and
/// resolves in [INV-1]'s or [PRF-1]'s narrower universe. Every other `pbase`
/// is an ordinary place base, including the operand of a measure former
/// written as an `affine_factor` `call`, whose IDENT is a place and not an
/// affine atom.
fn affine_atom_role(topology: &FinalizedTopology, pbase: NodeId) -> LexicalUseRole {
    let direct = topology
        .node(pbase)
        .and_then(|record| record.parent)
        .filter(|parent| {
            topology
                .node(*parent)
                .is_some_and(|record| record.production == Production::Place)
        })
        .and_then(|place| topology.node(place).and_then(|record| record.parent))
        .filter(|parent| {
            topology
                .node(*parent)
                .is_some_and(|record| record.production == Production::Atom)
        })
        .and_then(|atom| topology.node(atom).and_then(|record| record.parent))
        .is_some_and(|parent| {
            topology
                .node(parent)
                .is_some_and(|record| record.production == Production::AffineFactor)
        });
    if !direct {
        return LexicalUseRole::PlaceBase;
    }
    if ancestor_with_production(topology, pbase, Production::ProofUse).is_some() {
        return LexicalUseRole::ProofValue;
    }
    if ancestor_with_production(topology, pbase, Production::HeaderInvariant).is_some()
        || ancestor_with_production(topology, pbase, Production::InvariantStmt).is_some()
    {
        return LexicalUseRole::InvariantValue;
    }
    LexicalUseRole::PlaceBase
}

/// Which construct owns the `fieldbind` at `node`: the `let_stmt` of a
/// destructuring consume, or the `arm` of a `match` [GRAM-4, PROV-6].
fn nearest_binder_owner(topology: &FinalizedTopology, mut node: NodeId) -> Option<Production> {
    loop {
        let record = topology.node(node)?;
        if matches!(record.production, Production::LetStmt | Production::Arm) {
            return Some(record.production);
        }
        node = record.parent?;
    }
}

fn ancestor_with_production(
    topology: &FinalizedTopology,
    mut node: NodeId,
    production: Production,
) -> Option<NodeId> {
    loop {
        let record = topology.node(node)?;
        if record.production == production {
            return Some(node);
        }
        node = record.parent?;
    }
}

fn name_predicate(
    classified: &crate::ClassifiedBundle,
    terminal: usize,
) -> Option<TerminalPredicate> {
    let set = classified.tokens().get(terminal)?.terminals();
    [
        TerminalPredicate::Identifier,
        TerminalPredicate::TypeIdentifier,
        TerminalPredicate::Label,
        TerminalPredicate::OperationName,
    ]
    .into_iter()
    .find(|predicate| set.contains(*predicate))
}

/// Whether one node writes this exact fixed terminal directly.
fn has_fixed_terminal(
    classified: &crate::ClassifiedBundle,
    direct: &[usize],
    terminal: FixedTerminal,
) -> bool {
    direct.iter().any(|index| {
        classified.tokens().get(*index).is_some_and(|token| {
            token
                .terminals()
                .contains(TerminalPredicate::Fixed(terminal))
        })
    })
}

fn add_single(
    classified: &crate::ClassifiedBundle,
    owner: NodeId,
    terminals: &[usize],
    kind: RawRoleKind,
    roles: &mut Vec<RawRole>,
    counts: &mut [u8],
) -> Result<(), ResolutionCompilerFailure> {
    let [terminal] = terminals else {
        return Err(ResolutionCompilerFailure::InvalidRoleShape);
    };
    add_complete(classified, owner, *terminal, kind, roles, counts)
}

fn add_all(
    classified: &crate::ClassifiedBundle,
    owner: NodeId,
    terminals: &[usize],
    kind: RawRoleKind,
    roles: &mut Vec<RawRole>,
    counts: &mut [u8],
) -> Result<(), ResolutionCompilerFailure> {
    if terminals.is_empty() {
        return Err(ResolutionCompilerFailure::InvalidRoleShape);
    }
    for terminal in terminals {
        add_complete(classified, owner, *terminal, kind, roles, counts)?;
    }
    Ok(())
}

/// The names one projection step writes, for the `psuffix` of a place and the
/// `epsuffix` of an effect path alike [GRAM-5, EFF-1].
///
/// The two productions have the same three alternatives and differ only in
/// what sits in the subscript: a place writes an `atom`, which classifies
/// through its own productions and leaves no direct name here, while an effect
/// path writes a bare IDENT the caller admits as `index_kind`. The field and
/// payload alternatives are told apart by the shape kind of the token after
/// the `.`, never by grammar position, which is what keeps both spellings
/// context-free [GRAM-5].
#[allow(clippy::too_many_arguments)]
fn classify_projection_names(
    classified: &crate::ClassifiedBundle,
    owner: NodeId,
    names: &[usize],
    subscript: bool,
    index_kind: RawRoleKind,
    field_kind: RawRoleKind,
    roles: &mut Vec<RawRole>,
    counts: &mut [u8],
) -> Result<(), ResolutionCompilerFailure> {
    if subscript {
        return add_single(classified, owner, names, index_kind, roles, counts);
    }
    match names {
        [field] if name_predicate(classified, *field) == Some(TerminalPredicate::Identifier) => {
            add_complete(classified, owner, *field, field_kind, roles, counts)
        }
        [variant, field]
            if name_predicate(classified, *variant) == Some(TerminalPredicate::TypeIdentifier)
                && name_predicate(classified, *field) == Some(TerminalPredicate::Identifier) =>
        {
            add_complete(
                classified,
                owner,
                *variant,
                RawRoleKind::DeferredUse(DeferredUseRole::PayloadVariant),
                roles,
                counts,
            )?;
            add_complete(classified, owner, *field, field_kind, roles, counts)
        }
        _ => Err(ResolutionCompilerFailure::InvalidRoleShape),
    }
}

fn add_complete(
    classified: &crate::ClassifiedBundle,
    owner: NodeId,
    terminal: usize,
    kind: RawRoleKind,
    roles: &mut Vec<RawRole>,
    counts: &mut [u8],
) -> Result<(), ResolutionCompilerFailure> {
    let token = classified
        .tokens()
        .get(terminal)
        .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?
        .token();
    let id = token.id();
    let bytes = classified
        .token_bytes(token)
        .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
    let spelling = std::str::from_utf8(bytes)
        .map_err(|_| ResolutionCompilerFailure::InvalidNameEncoding)?
        .to_owned();
    let count = counts
        .get_mut(terminal)
        .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
    *count = count
        .checked_add(1)
        .ok_or(ResolutionCompilerFailure::CounterOverflow)?;
    roles.push(RawRole {
        kind,
        spelling,
        qualifier: None,
        member_owner: None,
        owner,
        source: id.source(),
        carrier_start: id.start(),
        carrier_end: id.end(),
        role_start: id.start(),
        role_end: id.end(),
        subtoken_ordinal: 0,
    });
    Ok(())
}

fn add_generic_suffix(
    classified: &crate::ClassifiedBundle,
    owner: NodeId,
    terminal: usize,
    roles: &mut Vec<RawRole>,
) -> Result<(), ResolutionCompilerFailure> {
    let classified_token = classified
        .tokens()
        .get(terminal)
        .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
    if !classified_token
        .terminals()
        .contains(TerminalPredicate::Literal)
    {
        return Ok(());
    }
    let token = classified_token.token();
    let bytes = classified
        .token_bytes(token)
        .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
    if bytes.len() < 3 || !matches!(&bytes[..2], b"0_" | b"1_") {
        return Ok(());
    }
    let suffix = std::str::from_utf8(&bytes[2..])
        .map_err(|_| ResolutionCompilerFailure::InvalidNameEncoding)?;
    if !suffix
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_uppercase)
    {
        return Ok(());
    }
    let start = token
        .id()
        .start()
        .value()
        .checked_add(2)
        .ok_or(ResolutionCompilerFailure::CounterOverflow)?;
    roles.push(RawRole {
        kind: RawRoleKind::LexicalUse(LexicalUseRole::GenericNumericSuffix),
        spelling: suffix.to_owned(),
        qualifier: None,
        member_owner: None,
        owner,
        source: token.id().source(),
        carrier_start: token.id().start(),
        carrier_end: token.id().end(),
        role_start: ByteOffset::new(start),
        role_end: token.id().end(),
        subtoken_ordinal: 1,
    });
    Ok(())
}
