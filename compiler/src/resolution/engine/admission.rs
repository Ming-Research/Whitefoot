use crate::syntax::terminal::{FixedTerminal, TerminalPredicate};
use crate::syntax::{FinalizedExtent, FinalizedTopology, NodeId};
use crate::{Production, SourceRole, SyntaxCoordinate};

use super::super::scopes::ScopeBuild;
use super::super::{
    ContractShapeIssue, ResolutionCompilerFailure, ResolutionIssue, ResolutionIssueKind,
    ResolutionRule, SourceOrigin,
};
use super::EventKey;

pub(super) fn check_clause_blocks(
    topology: &FinalizedTopology,
    scopes: &ScopeBuild,
) -> Result<Option<ResolutionIssue>, ResolutionCompilerFailure> {
    let mut candidates = Vec::new();
    for (index, record) in topology.nodes.iter().enumerate() {
        if record.production != Production::ContractBlock {
            continue;
        }
        let id = NodeId::from_index(index).ok_or(ResolutionCompilerFailure::CounterOverflow)?;
        let has_clause = topology
            .node_children(id)
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?
            .iter()
            .any(|child| {
                topology.node(*child).is_some_and(|child| {
                    matches!(
                        child.production,
                        Production::RequiresClause | Production::EnsuresClause
                    )
                })
            });
        if !has_clause {
            candidates.push(ResolutionIssue {
                rule: ResolutionRule::Fn8,
                origin: node_origin(topology, scopes, id)?,
                kind: ResolutionIssueKind::ContractShape(ContractShapeIssue::MissingClause),
            });
        }
    }
    candidates.sort_by_key(|issue| EventKey::from_origin(&issue.origin));
    Ok(candidates.into_iter().next())
}

/// [GRAM-2] "A `heap_decl` is admitted at most once in a compilation unit and
/// only as the first `item` of the first source record [PROG-2]; a second
/// `heap_decl`, or one at any later item position, is a hard error citing
/// GRAM-2 at that `heap_decl` node."
///
/// The parser is table-driven and has no compilation-unit position rule, so
/// the grammar admits a `heap_decl` at every item position and this whole-unit
/// judgment is made here. [DIAG-1] states that [FN-8]'s contract-presence
/// judgment runs first after canonical FORM-2 and fixes nothing else about
/// this one, so it runs immediately after that and before inventory.
pub(super) fn check_heap_declaration(
    topology: &FinalizedTopology,
    scopes: &ScopeBuild,
    module_program: bool,
) -> Result<Option<ResolutionIssue>, ResolutionCompilerFailure> {
    // [MOD-9] a module program admits no `heap_decl` at any position; that
    // judgment is the module-form check's.
    if module_program {
        return Ok(None);
    }
    let items = topology
        .node_children(topology.root)
        .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
    let mut admitted: Option<SourceOrigin> = None;
    for (position, item) in items.iter().enumerate() {
        let Some(declaration) = heap_declaration(topology, *item)? else {
            continue;
        };
        let origin = node_origin(topology, scopes, declaration)?;
        // The unit's items are in [PROG-2] source-record order, so the first
        // item of the first source record is the first child of the root,
        // provided that child belongs to the first record.
        if position == 0 && admitted.is_none() && origin.coordinate.source().ordinal() == 0 {
            admitted = Some(origin);
            continue;
        }
        return Ok(Some(ResolutionIssue {
            rule: ResolutionRule::Gram2,
            origin,
            kind: ResolutionIssueKind::MisplacedHeapDeclaration {
                admitted: admitted.clone(),
            },
        }));
    }
    Ok(None)
}

/// The `heap_decl` one `item` selects, when it selects one.
fn heap_declaration(
    topology: &FinalizedTopology,
    item: NodeId,
) -> Result<Option<NodeId>, ResolutionCompilerFailure> {
    if topology
        .node(item)
        .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?
        .production
        != Production::Item
    {
        return Ok(None);
    }
    Ok(topology
        .node_children(item)
        .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?
        .iter()
        .copied()
        .find(|child| {
            topology
                .node(*child)
                .is_some_and(|record| record.production == Production::HeapDecl)
        }))
}

fn node_origin(
    topology: &FinalizedTopology,
    scopes: &ScopeBuild,
    node: NodeId,
) -> Result<SourceOrigin, ResolutionCompilerFailure> {
    let record = topology
        .node(node)
        .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
    let FinalizedExtent::Source { source, start, end } = record.extent else {
        return Err(ResolutionCompilerFailure::InvalidCanonicalTree);
    };
    Ok(SourceOrigin {
        node: scopes.path(node)?.clone(),
        coordinate: SyntaxCoordinate::new(source, start, end),
        extent: SyntaxCoordinate::new(source, start, end),
        role_ordinal: 0,
        subtoken_ordinal: 0,
    })
}

/// The structural module-form judgments, made in source order before
/// inventory [DIAG-1]:
///
/// - [MOD-4] an alias is written only in its source's initial alias header;
/// - [MOD-9] a module program states heap requirements on its entries, so
///   none of its sources writes `program no_heap`;
/// - [MOD-6] `public` is written only in a module interface, on a member
///   only of a public type, and `readonly` only on a public field; and
/// - [MOD-7] an interface declares functions without bodies, each ending in
///   its `doc` entry, and an implementation or source bundle defines them
///   with bodies.
pub(super) fn check_module_forms(
    topology: &FinalizedTopology,
    scopes: &ScopeBuild,
    classified: &crate::ClassifiedBundle,
) -> Result<Option<ResolutionIssue>, ResolutionCompilerFailure> {
    let bundle = classified.source_bundle();
    let mut direct = vec![Vec::new(); topology.nodes.len()];
    for (index, terminal) in topology.terminals.iter().enumerate() {
        let owner = terminal
            .owner
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
        direct
            .get_mut(owner.index())
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?
            .push(index);
    }
    let writes = |node: NodeId, fixed: FixedTerminal| {
        direct.get(node.index()).is_some_and(|terminals| {
            terminals.iter().any(|index| {
                classified.tokens().get(*index).is_some_and(|token| {
                    token.terminals().contains(TerminalPredicate::Fixed(fixed))
                })
            })
        })
    };
    let child = |node: NodeId, production: Production| {
        topology.node_children(node).and_then(|children| {
            children.iter().copied().find(|child| {
                topology
                    .node(*child)
                    .is_some_and(|record| record.production == production)
            })
        })
    };
    let issue = |node: NodeId, rule: ResolutionRule, kind: ResolutionIssueKind| {
        Ok::<_, ResolutionCompilerFailure>(Some(ResolutionIssue {
            rule,
            origin: node_origin(topology, scopes, node)?,
            kind,
        }))
    };
    let items = topology
        .node_children(topology.root)
        .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
    let mut header_closed = Vec::new();
    for item in items {
        let record = topology
            .node(*item)
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
        let FinalizedExtent::Source { source, .. } = record.extent else {
            return Err(ResolutionCompilerFailure::InvalidCanonicalTree);
        };
        let file = bundle
            .file(source)
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
        if file.prelude().is_some() {
            continue;
        }
        // A standard library interface record joins a source bundle's unit
        // as an interface record [MOD-10], so the role decides.
        let interface = file.role() == SourceRole::Interface;
        if let Some(alias) = child(*item, Production::AliasDecl) {
            if header_closed.contains(&source) {
                return issue(
                    alias,
                    ResolutionRule::Mod4,
                    ResolutionIssueKind::MisplacedAlias,
                );
            }
            continue;
        }
        // [MOD-4] the alias header follows the record's heap declaration.
        let heap = child(*item, Production::HeapDecl);
        if heap.is_none() {
            header_closed.push(source);
        }
        if let Some(heap) = heap
            && bundle.is_module_program()
        {
            return issue(
                heap,
                ResolutionRule::Mod9,
                ResolutionIssueKind::ModuleProgramHeapDeclaration,
            );
        }
        let public_item = writes(*item, FixedTerminal::Public);
        if public_item && !interface {
            return issue(
                *item,
                ResolutionRule::Mod6,
                ResolutionIssueKind::MisplacedPublication {
                    reason: "`public` is written only in a module interface, `module.wfm`",
                },
            );
        }
        if let Some(function) = child(*item, Production::FnDecl) {
            let body = writes(function, FixedTerminal::LeftBrace);
            let spelling = function_spelling(topology, classified, &direct, function)?;
            if interface && body {
                return issue(
                    function,
                    ResolutionRule::Mod7,
                    ResolutionIssueKind::Correspondence {
                        spelling,
                        reason: "a module interface declares a function without a body, ending in its `doc` entry; its body belongs in one of the module's `.wf` files",
                    },
                );
            }
            if interface && child(function, Production::Doc).is_none() {
                return issue(
                    function,
                    ResolutionRule::Mod7,
                    ResolutionIssueKind::Correspondence {
                        spelling,
                        reason: "a module interface declares each function with the `doc` entry that describes it to the module's clients; end the declaration in `doc \"...\";`",
                    },
                );
            }
            if !interface && !body {
                return issue(
                    function,
                    ResolutionRule::Mod7,
                    ResolutionIssueKind::Correspondence {
                        spelling,
                        reason: "a function without a body is declared only in a module interface, `module.wfm`; a definition writes its body",
                    },
                );
            }
        }
        let members = child(*item, Production::StructDecl)
            .or_else(|| child(*item, Production::EnumDecl))
            .map(|declaration| {
                let mut members = Vec::new();
                let mut pending = vec![declaration];
                while let Some(node) = pending.pop() {
                    for member in topology.node_children(node).unwrap_or(&[]) {
                        match topology.node(*member).map(|record| record.production) {
                            Some(Production::Field | Production::Vfield) => members.push(*member),
                            // A payload field sits in its variant's list.
                            Some(Production::Variant | Production::VfieldList) => {
                                pending.push(*member);
                            }
                            _ => {}
                        }
                    }
                }
                members
            })
            .unwrap_or_default();
        let mut members = members;
        members.sort_by_key(|member| member.index());
        for member in members {
            let public_member = writes(member, FixedTerminal::Public);
            if public_member && !interface {
                return issue(
                    member,
                    ResolutionRule::Mod6,
                    ResolutionIssueKind::MisplacedPublication {
                        reason: "`public` is written only in a module interface, `module.wfm`",
                    },
                );
            }
            if public_member && !public_item {
                return issue(
                    member,
                    ResolutionRule::Mod6,
                    ResolutionIssueKind::MisplacedPublication {
                        reason: "a member of a private type is reachable only inside its module, so `public` on it publishes nothing; publish the type or remove the marker",
                    },
                );
            }
            if writes(member, FixedTerminal::Readonly) && !public_member {
                return issue(
                    member,
                    ResolutionRule::Mod6,
                    ResolutionIssueKind::MisplacedPublication {
                        reason: "`readonly` withholds writes from other modules, so it is written only on a `public` field; a private field is already unreachable from them",
                    },
                );
            }
        }
    }
    Ok(None)
}

fn function_spelling(
    topology: &FinalizedTopology,
    classified: &crate::ClassifiedBundle,
    direct: &[Vec<usize>],
    function: NodeId,
) -> Result<String, ResolutionCompilerFailure> {
    let _ = topology;
    let terminal = direct
        .get(function.index())
        .and_then(|terminals| {
            terminals.iter().copied().find(|index| {
                classified
                    .tokens()
                    .get(*index)
                    .is_some_and(|token| token.terminals().contains(TerminalPredicate::Identifier))
            })
        })
        .ok_or(ResolutionCompilerFailure::InvalidRoleShape)?;
    let token = classified
        .tokens()
        .get(terminal)
        .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?
        .token();
    let bytes = classified
        .token_bytes(token)
        .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
    std::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|_| ResolutionCompilerFailure::InvalidNameEncoding)
}

/// [MOD-6] a public declaration's published vocabulary names only
/// declarations its module's clients can access: a public function's whole
/// header, a public struct's public fields' types, a public enum's public
/// payload fields' types, a public const's type and a public interface's
/// formals. A use there resolving to a private top-level declaration of the
/// same module is refused at the use; private fields, a const's initializer
/// and a binding's actuals stay private implementation.
pub(super) fn check_public_closure<'a>(
    topology: &FinalizedTopology,
    scopes: &ScopeBuild,
    classified: &crate::ClassifiedBundle,
    declarations: &[super::super::DeclarationRecord],
    uses: impl Iterator<Item = &'a super::super::LexicalUseRecord>,
) -> Result<Option<ResolutionIssue>, ResolutionCompilerFailure> {
    let bundle = classified.source_bundle();
    if !bundle
        .files()
        .iter()
        .any(|file| file.role() == SourceRole::Interface)
    {
        return Ok(None);
    }
    let writes = |node: NodeId, fixed: FixedTerminal| {
        topology
            .terminals
            .iter()
            .enumerate()
            .any(|(index, terminal)| {
                terminal.owner == Some(node)
                    && classified.tokens().get(index).is_some_and(|token| {
                        token.terminals().contains(TerminalPredicate::Fixed(fixed))
                    })
            })
    };
    let children_with = |node: NodeId, production: Production| -> Vec<NodeId> {
        topology
            .node_children(node)
            .unwrap_or(&[])
            .iter()
            .copied()
            .filter(|child| {
                topology
                    .node(*child)
                    .is_some_and(|record| record.production == production)
            })
            .collect()
    };
    let mut published = Vec::new();
    for item in topology
        .node_children(topology.root)
        .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?
    {
        if !writes(*item, FixedTerminal::Public) {
            continue;
        }
        let [declaration] = topology
            .node_children(*item)
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?
        else {
            return Err(ResolutionCompilerFailure::InvalidCanonicalTree);
        };
        let record = topology
            .node(*declaration)
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
        match record.production {
            Production::FnDecl | Production::InterfaceDecl => published.push(*declaration),
            Production::ConstDecl => {
                published.extend(children_with(*declaration, Production::Type))
            }
            Production::StructDecl => published.extend(
                children_with(*declaration, Production::Field)
                    .into_iter()
                    .filter(|field| writes(*field, FixedTerminal::Public)),
            ),
            Production::EnumDecl => {
                for variant in children_with(*declaration, Production::Variant) {
                    for list in children_with(variant, Production::VfieldList) {
                        published.extend(
                            children_with(list, Production::Vfield)
                                .into_iter()
                                .filter(|field| writes(*field, FixedTerminal::Public)),
                        );
                    }
                }
            }
            _ => {}
        }
    }
    let mut paths = Vec::with_capacity(published.len());
    for node in published {
        paths.push(scopes.path(node)?.components().to_vec());
    }
    let mut issues = Vec::new();
    for usage in uses {
        let use_path = usage.origin().node().components();
        if !paths.iter().any(|path| use_path.starts_with(path)) {
            continue;
        }
        let super::super::ResolvedTarget::Source { declaration, .. } = usage.target() else {
            continue;
        };
        let Some(record) = declarations.get(declaration.index()) else {
            return Err(ResolutionCompilerFailure::InvalidRoleShape);
        };
        let module_level = record
            .module()
            .and_then(|module| scopes.module_scope(module))
            .is_some_and(|scope| scope == record.scope());
        if module_level && !record.is_public() {
            issues.push(ResolutionIssue {
                rule: ResolutionRule::Mod6,
                origin: usage.origin().clone(),
                kind: ResolutionIssueKind::PrivateInPublicSignature {
                    spelling: usage.spelling().to_owned(),
                },
            });
        }
    }
    issues.sort_by_key(|issue| EventKey::from_origin(&issue.origin));
    Ok(issues.into_iter().next())
}
