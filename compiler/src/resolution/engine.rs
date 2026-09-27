use std::collections::HashMap;

use crate::syntax::terminal::{FixedTerminal, TerminalPredicate};
use crate::syntax::{FinalizedExtent, FinalizedTopology, NodeId};
use crate::{ByteOffset, CanonicalSyntaxUnit, Production, SourceId};

use super::catalog::PRELUDE_DECLARATIONS;
use super::scopes::ScopeBuild;
use super::{
    DeclarationClass, DeclarationDomain, DeclarationId, DeclarationRecord, DeclarationRole,
    DeferredUseRecord, DeferredUseRole, DependentDeclarationRecord, DependentDeclarationRole,
    LexicalUseRecord, LexicalUseRole, PostconditionCandidateRecord, PostconditionFieldRecord,
    PostconditionResolutionRecord, PostconditionSelectorClass, PostconditionSelectorUseRecord,
    PreludeDeclarationRecord, ResolutionCompilerFailure, ResolutionIssue, ResolutionIssueKind,
    ResolutionOutcome, ResolvedSyntaxUnit, ResolvedTarget, ScopeId, SourceOrigin,
};
use super::{DeclarationKey, ItemHome, ItemKey};

mod admission;
mod correspondence;
mod inventory;
mod lookup;
mod prelude;
mod render;
mod roles;

pub(super) use render::render_interface;

use admission::{
    check_clause_blocks, check_heap_declaration, check_module_forms, check_public_closure,
};
use inventory::{check_declaration_inventory, pair_interface_functions};
use lookup::{ModuleView, resolve_alias_targets, resolve_uses_deferred};
use roles::classify_roles;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct EventKey {
    source: u32,
    start: u64,
    end: u64,
    path: Vec<u32>,
    role: u32,
    subtoken: u32,
}

impl EventKey {
    fn from_origin(origin: &SourceOrigin) -> Self {
        let coordinate = origin.coordinate;
        Self {
            source: coordinate.source().ordinal(),
            start: coordinate.start().value(),
            end: coordinate.end().value(),
            path: origin.node.components().to_vec(),
            role: origin.role_ordinal,
            subtoken: origin.subtoken_ordinal,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum RawRoleKind {
    Declaration(DeclarationRole),
    DependentDeclaration(DependentDeclarationRole),
    Selector(SelectorRole),
    LexicalUse(LexicalUseRole),
    DeferredUse(DeferredUseRole),
}

impl RawRoleKind {
    const fn class_ordinal(self) -> u8 {
        match self {
            Self::Declaration(_) | Self::DependentDeclaration(_) => 0,
            Self::Selector(_) => 1,
            Self::LexicalUse(_) => 2,
            Self::DeferredUse(_) => 3,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum SelectorRole {
    PlainCandidate,
    VariantField,
    VariantCandidate,
    /// The optional ordinal binder of a route, `when b is V(f: r):`
    /// [GRAM-2, CALL-4]. It names a declared result and declares nothing.
    ResultOrdinal,
    /// The alias root or a module segment of a qualified path, or a segment
    /// of an alias target [MOD-4, MOD-5]. The use or alias that carries the
    /// path resolves it as a whole.
    PathSegment,
}

/// One written name of a qualified path [MOD-4, MOD-5].
#[derive(Clone, Debug)]
struct PathSegment {
    spelling: String,
    coordinate: crate::SyntaxCoordinate,
}

/// The module prefix of a qualified use, or the complete target path of an
/// alias [MOD-4, MOD-5].
#[derive(Clone, Debug)]
struct Qualifier {
    /// `None` when the path begins with `pkg` or `std`; otherwise the
    /// file-local module alias that roots it.
    alias_root: Option<PathSegment>,
    /// Whether the path begins with `std`, the standard library's qualifier
    /// [MOD-10], rather than `pkg`.
    standard: bool,
    /// The names written after the root, before the use's own final name.
    /// For an alias they are the complete target after `pkg`.
    segments: Vec<PathSegment>,
}

struct RawRole {
    kind: RawRoleKind,
    spelling: String,
    /// The module prefix of a qualified use, or an alias's target path.
    qualifier: Option<Qualifier>,
    /// For the member TYPEID of a type-owned variant construction, the
    /// coordinate of its owner's TYPEID [TYPE-6].
    member_owner: Option<crate::SyntaxCoordinate>,
    owner: NodeId,
    source: SourceId,
    carrier_start: ByteOffset,
    carrier_end: ByteOffset,
    role_start: ByteOffset,
    role_end: ByteOffset,
    subtoken_ordinal: u32,
}

struct ClassifiedRole {
    kind: RawRoleKind,
    spelling: String,
    qualifier: Option<Qualifier>,
    member_owner: Option<crate::SyntaxCoordinate>,
    owner: NodeId,
    origin: SourceOrigin,
    scope: ScopeId,
    owner_chain: Vec<NodeId>,
}

#[derive(Clone, Copy)]
enum Visibility {
    Always,
    After { source: u32, byte: u64 },
}

struct DeclarationMeta {
    role_index: usize,
    record_index: usize,
    scope: ScopeId,
    owner: Option<NodeId>,
    visibility: Visibility,
    entries: Vec<DeclarationClass>,
    /// Which compiler-owned container this declaration is, when it is one of
    /// [PRE-1]'s four opaque storage records [TYPE-2, TYPE-9]. A use that
    /// selects it resolves to that identity rather than to the declaration,
    /// because a written `Array`, `Slots`, `Ring` or `Box` names one
    /// compiler-owned shape and not a source struct: only the identity
    /// carries the element storage, the omitted-capacity form and the measure
    /// rows no struct body can state.
    container: Option<crate::ContainerNominalId>,
    /// The module whose inventory holds it; `None` for a PRE-1 record.
    module: Option<crate::ModuleId>,
    /// Whether an interface publishes it [MOD-6]. A definition paired with
    /// its interface declaration takes the declaration's publication.
    public: bool,
    /// A user enum variant, which belongs to its enum and enters no
    /// unqualified constructor inventory [TYPE-6].
    type_owned: bool,
    /// What an alias declaration binds, once its target is resolved [MOD-4].
    alias: Option<AliasTarget>,
    /// Which part of an interface/implementation pairing a function
    /// declaration is [MOD-7].
    function_form: FunctionForm,
    /// Declared by an implementation record of a module program and standing
    /// for no interface declaration, so visible only in implementation
    /// records [MOD-3].
    implementation_only: bool,
}

/// What one alias binds [MOD-4].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AliasTarget {
    Module(crate::ModuleId),
    /// A declaration, by record index.
    Declaration(usize),
}

/// How a top-level function declaration takes part in module correspondence
/// [MOD-7].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FunctionForm {
    /// Not a module-level function.
    None,
    /// A body-less interface declaration, with its definition's record index
    /// once paired.
    Declaration { definition: Option<usize> },
    /// A definition with a body.
    Definition,
}

struct DeclarationIndex {
    by_spelling: HashMap<String, Vec<usize>>,
}

impl DeclarationIndex {
    fn build(declarations: &[DeclarationRecord]) -> Self {
        let mut by_spelling = HashMap::new();
        for declaration in declarations {
            by_spelling
                .entry(declaration.spelling.clone())
                .or_insert_with(Vec::new)
                .push(declaration.id.index());
        }
        Self { by_spelling }
    }

    fn with_spelling(&self, spelling: &str) -> &[usize] {
        self.by_spelling.get(spelling).map_or(&[], Vec::as_slice)
    }
}

#[derive(Clone)]
struct UseMeta {
    role: LexicalUseRole,
    spelling: String,
    qualifier: Option<Qualifier>,
    member_owner: Option<crate::SyntaxCoordinate>,
    /// The module of the source that writes the use.
    module: crate::ModuleId,
    /// Written in a module's interface record, which sees only its module's
    /// interface declarations [MOD-3].
    interface: bool,
    owner: NodeId,
    origin: SourceOrigin,
    scope: ScopeId,
    owner_chain: Vec<NodeId>,
    function_owner: Option<NodeId>,
}

struct Tables {
    scopes: Vec<super::ScopeRecord>,
    prelude: Vec<PreludeDeclarationRecord>,
    declarations: Vec<DeclarationRecord>,
    dependent_declarations: Vec<DependentDeclarationRecord>,
    lexical_uses: Vec<LexicalUseRecord>,
    deferred_uses: Vec<DeferredUseRecord>,
    postconditions: Vec<PostconditionResolutionRecord>,
    interface_functions: Vec<super::InterfaceFunction>,
    by_node: super::NodeRecords,
    items: Vec<Option<ItemKey>>,
}

enum BuildStop {
    Issue(Box<ResolutionIssue>),
    Compiler(ResolutionCompilerFailure),
}

impl From<ResolutionCompilerFailure> for BuildStop {
    fn from(value: ResolutionCompilerFailure) -> Self {
        Self::Compiler(value)
    }
}

/// Resolves every active-specification declaration and lexical use in canonical syntax.
#[must_use]
pub fn resolve(syntax: CanonicalSyntaxUnit) -> ResolutionOutcome {
    match build_tables(&syntax) {
        Ok(tables) => ResolutionOutcome::Complete(ResolvedSyntaxUnit {
            syntax,
            scopes: tables.scopes,
            prelude: tables.prelude,
            declarations: tables.declarations,
            dependent_declarations: tables.dependent_declarations,
            lexical_uses: tables.lexical_uses,
            deferred_uses: tables.deferred_uses,
            postconditions: tables.postconditions,
            interface_functions: tables.interface_functions,
            by_node: tables.by_node,
            items: tables.items,
        }),
        Err(BuildStop::Issue(issue)) => ResolutionOutcome::SourceIssue {
            syntax,
            issue: *issue,
        },
        Err(BuildStop::Compiler(failure)) => ResolutionOutcome::CompilerFailure { syntax, failure },
    }
}

fn build_tables(syntax: &CanonicalSyntaxUnit) -> Result<Tables, BuildStop> {
    let topology = &syntax.finalized.topology;
    let scopes = ScopeBuild::build(topology, syntax.finalized.parsed.classified.source_bundle())?;
    // [DIAG-1] fixes this order: complete unit-wide FN-8 admission precedes
    // declaration inventory, and only complete inventory permits lexical
    // resolution.
    if let Some(issue) = check_clause_blocks(topology, &scopes)? {
        return Err(BuildStop::Issue(Box::new(issue)));
    }
    // [GRAM-2]'s whole-unit heap-declaration position rule. The parser is
    // table-driven and has no unit-level position judgment, so this is the
    // first stage that can make it.
    if let Some(issue) = check_heap_declaration(
        topology,
        &scopes,
        syntax
            .classified_bundle()
            .source_bundle()
            .is_module_program(),
    )? {
        return Err(BuildStop::Issue(Box::new(issue)));
    }
    if let Some(issue) = check_module_forms(topology, &scopes, syntax.classified_bundle())? {
        return Err(BuildStop::Issue(Box::new(issue)));
    }
    let roles = classify_roles(syntax, &scopes)?;
    let prelude = prelude::PreludeInventory::build(syntax, &roles)?;
    // [SET-1] "A `set` whose target name resolves to nothing declares nothing
    // and is a hard error citing SET-1 at that `place`." The v0.59 promotion
    // loop that turned such a target into a `let` declaration is gone with
    // [LIV-2], and with it resolution's only re-entrant path; the shape
    // predicate survives only to re-attribute the ordinary unresolved-name
    // rejection to the rule and location SET-1 states.
    let bare_set_targets = bare_set_target_roles(topology, &roles)?;
    let classified = syntax.classified_bundle();
    let bundle = classified.source_bundle();
    let items = item_keys(topology, &roles, bundle)?;
    {
        let mut declarations = Vec::new();
        let mut dependent_declarations = Vec::new();
        let mut deferred_uses = Vec::new();
        let mut declaration_metas = Vec::new();
        let mut declaration_by_role = vec![None; roles.len()];
        let mut uses = Vec::new();
        let mut route_uses = Vec::new();
        let mut postcondition_entry_uses = Vec::new();

        for (role_index, role) in roles.iter().enumerate() {
            match role.kind {
                RawRoleKind::Declaration(declaration_role) => {
                    let id = DeclarationId::from_index(declarations.len())
                        .ok_or(ResolutionCompilerFailure::CounterOverflow)?;
                    // A named interface's members have stable function-parameter
                    // identities, but FN-3 introduces no unqualified lexical
                    // names. Only a raw function binder enters that domain.
                    // Member distinctness is the interface table's FN-3 judgment.
                    let grouped_member = declaration_role == DeclarationRole::FunctionParameter
                        && role
                            .owner_chain
                            .get(1)
                            .and_then(|owner| topology.node(*owner))
                            .is_some_and(|record| record.production == Production::InterfaceDecl);
                    // [TYPE-2] an opaque struct's constructor entry "exists to
                    // be refused", so it is an ordinary entry of the
                    // constructor TYPEID domain like any other struct's. The
                    // refusal is the checker's hard error at the complete
                    // `call`, which is a judgment over a resolved declaration
                    // and needs the entry to reach.
                    let entries = if grouped_member {
                        Vec::new()
                    } else {
                        declaration_classes(declaration_role)
                    };
                    // [TYPE-2, PRE-1] the prelude's four storage records keep
                    // the compiler-owned identities every later stage reads a
                    // written `Array`, `Slots`, `Ring` or `Box` through, even
                    // though each declaration is now an ordinary prelude
                    // record [TYPE-9].
                    let container = if declaration_role == DeclarationRole::Struct
                        && syntax
                            .finalized
                            .parsed
                            .classified
                            .source_bundle()
                            .file(role.origin.coordinate.source())
                            .is_some_and(|file| {
                                file.prelude() == Some(crate::source::PreludeSource::Opaque)
                            }) {
                        crate::container_nominal_id(&role.spelling)
                    } else {
                        None
                    };
                    let file = bundle.file(role.origin.coordinate.source());
                    let prelude_source = file.is_some_and(|file| file.prelude().is_some());
                    let module = file
                        .filter(|file| file.prelude().is_none())
                        .map(crate::SourceFile::module);
                    // [MOD-3] a module-level declaration of a writer source
                    // joins its module's one inventory and is visible
                    // throughout the module, independently of file and item
                    // order; PRE-1 records stay in the supplied environment.
                    let top_level = is_top_level_role(declaration_role);
                    let scope = match (top_level, prelude_source, module) {
                        (true, false, Some(module)) => scopes
                            .module_scope(module)
                            .ok_or(ResolutionCompilerFailure::InvalidScopeTree)?,
                        _ => declaration_scope(role, declaration_role, &scopes)?,
                    };
                    let visibility = if (top_level || declaration_role == DeclarationRole::Alias)
                        && (prelude_source || module.is_some())
                    {
                        Visibility::Always
                    } else {
                        declaration_visibility(topology, role, declaration_role)?
                    };
                    // A variant is published with its enum; every other
                    // top-level declaration by its own item's marker.
                    let publishing_node = if declaration_role == DeclarationRole::Variant {
                        role.owner_chain.first().copied()
                    } else {
                        Some(role.owner)
                    };
                    let public = top_level
                        && !prelude_source
                        && publishing_node
                            .is_some_and(|node| declares_public(topology, classified, node));
                    let function_form =
                        if declaration_role == DeclarationRole::Function && !prelude_source {
                            let node = role
                                .owner_chain
                                .first()
                                .copied()
                                .ok_or(ResolutionCompilerFailure::InvalidRoleShape)?;
                            if has_body(topology, classified, node) {
                                FunctionForm::Definition
                            } else {
                                FunctionForm::Declaration { definition: None }
                            }
                        } else {
                            FunctionForm::None
                        };
                    let record_index = declarations.len();
                    declarations.push(DeclarationRecord {
                        id,
                        role: declaration_role,
                        spelling: role.spelling.clone(),
                        origin: role.origin.clone(),
                        scope,
                        classes: entries.clone(),
                        diagnostic_origins: prelude.roles[role_index].clone(),
                        module,
                        public,
                        key: declaration_key(&role.origin, role_index, &items)?,
                    });
                    declaration_by_role[role_index] = Some(record_index);
                    declaration_metas.push(DeclarationMeta {
                        role_index,
                        record_index,
                        scope,
                        owner: if declaration_role == DeclarationRole::FunctionParameter {
                            // The callable binder belongs to the receiving
                            // generic declaration; its own parameters and
                            // regions remain owned by the nested FnSig.
                            Some(
                                *role
                                    .owner_chain
                                    .get(1)
                                    .ok_or(ResolutionCompilerFailure::InvalidRoleShape)?,
                            )
                        } else {
                            role.owner_chain.first().copied()
                        },
                        visibility,
                        entries,
                        container,
                        module,
                        public,
                        type_owned: declaration_role == DeclarationRole::Variant && !prelude_source,
                        alias: None,
                        function_form,
                        implementation_only: bundle.is_module_program()
                            && !prelude_source
                            && file.is_some_and(|file| {
                                file.role() == crate::SourceRole::Implementation
                            }),
                    });
                }
                RawRoleKind::DependentDeclaration(dependent_role) => {
                    dependent_declarations.push(DependentDeclarationRecord {
                        role: dependent_role,
                        spelling: role.spelling.clone(),
                        origin: role.origin.clone(),
                    });
                }
                RawRoleKind::LexicalUse(use_role) => {
                    let use_meta = UseMeta {
                        role: use_role,
                        spelling: role.spelling.clone(),
                        qualifier: role.qualifier.clone(),
                        member_owner: role.member_owner,
                        module: bundle
                            .file(role.origin.coordinate.source())
                            .map_or(crate::ModuleId::BUNDLE_ROOT, crate::SourceFile::module),
                        interface: bundle.file(role.origin.coordinate.source()).is_some_and(
                            |file| {
                                file.prelude().is_none()
                                    && file.role() == crate::SourceRole::Interface
                            },
                        ),
                        owner: role.owner,
                        origin: role.origin.clone(),
                        scope: role.scope,
                        owner_chain: role.owner_chain.clone(),
                        function_owner: function_owner(topology, role.owner),
                    };
                    if use_role == LexicalUseRole::EnsuresVariant {
                        route_uses.push(use_meta);
                    } else if ancestor_with_production(
                        topology,
                        role.owner,
                        Production::EnsuresClause,
                    )
                    .is_some()
                    {
                        postcondition_entry_uses.push(use_meta);
                    } else {
                        uses.push(use_meta);
                    }
                }
                RawRoleKind::DeferredUse(deferred_role) => deferred_uses.push(DeferredUseRecord {
                    role: deferred_role,
                    spelling: role.spelling.clone(),
                    origin: role.origin.clone(),
                }),
                RawRoleKind::Selector(_) => {}
            }
        }

        let declaration_index = DeclarationIndex::build(&declarations);
        let modules = ModuleView {
            modules: bundle.modules(),
        };
        // [MOD-7] an interface declaration and its definition are one
        // function; [MOD-4] every alias target resolves before inventory, so
        // an alias takes its target's collision domains.
        pair_interface_functions(
            &mut declarations,
            &mut declaration_metas,
            &declaration_index,
        );
        let qualifiers: Vec<_> = roles.iter().map(|role| role.qualifier.clone()).collect();
        let alias_issues = resolve_alias_targets(
            topology,
            &scopes,
            &mut declarations,
            &mut declaration_metas,
            &declaration_index,
            &modules,
            &qualifiers,
        )?;

        if let Some(issue) = check_declaration_inventory(
            topology,
            &scopes,
            &roles,
            &declarations,
            &declaration_metas,
            &declaration_index,
            &declaration_by_role,
            &prelude.builtins,
            &alias_issues,
            bundle.modules(),
        )? {
            return Err(BuildStop::Issue(Box::new(issue)));
        }
        let (mut lexical_uses, unresolved) = resolve_uses_deferred(
            &scopes,
            &declarations,
            &declaration_metas,
            &declaration_index,
            &modules,
            &uses,
        )?;
        if let Some(issue) = unresolved {
            return Err(BuildStop::Issue(Box::new(set_target_attribution(
                topology,
                &scopes,
                &roles,
                &bare_set_targets,
                issue,
            )?)));
        }
        if let Some(issue) = resolve_route_variants(
            topology,
            &roles,
            &scopes,
            &declarations,
            &declaration_metas,
            &declaration_index,
            &modules,
            &route_uses,
            &mut lexical_uses,
        )? {
            return Err(BuildStop::Issue(Box::new(issue)));
        }
        let postconditions = build_postcondition_records(
            topology,
            &scopes,
            &roles,
            &declarations,
            &declaration_metas,
            &declaration_index,
            &modules,
            &postcondition_entry_uses,
            &lexical_uses,
        )?;
        // [MOD-6] public signatures close over accessible vocabulary.
        if let Some(issue) = check_public_closure(
            topology,
            &scopes,
            classified,
            &declarations,
            lexical_uses.iter().chain(
                postconditions
                    .iter()
                    .flat_map(|record| record.provisional_uses.iter()),
            ),
        )? {
            return Err(BuildStop::Issue(Box::new(issue)));
        }
        // [MOD-7] every paired definition repeats its interface declaration.
        let pairs: Vec<(usize, usize)> = declaration_metas
            .iter()
            .filter_map(|meta| match meta.function_form {
                FunctionForm::Declaration {
                    definition: Some(definition),
                } => Some((meta.record_index, definition)),
                _ => None,
            })
            .collect();
        let function_nodes = declaration_metas
            .iter()
            .filter(|meta| meta.function_form != FunctionForm::None)
            .filter_map(|meta| Some((declarations[meta.record_index].id, meta.owner?)))
            .collect();
        if let Some(issue) = correspondence::check_correspondence(
            topology,
            classified,
            &declarations,
            lexical_uses.iter().chain(
                postconditions
                    .iter()
                    .flat_map(|record| record.provisional_uses.iter()),
            ),
            &pairs,
            &function_nodes,
        )? {
            return Err(BuildStop::Issue(Box::new(issue)));
        }
        let interface_functions = declaration_metas
            .iter()
            .filter_map(|meta| match meta.function_form {
                FunctionForm::Declaration { definition } => Some(super::InterfaceFunction {
                    declaration: declarations[meta.record_index].id,
                    definition: definition.map(|definition| declarations[definition].id),
                }),
                _ => None,
            })
            .collect();
        // Inventory has rejected every duplicate declaration of one scope, so
        // each key now names exactly one declaration.
        let mut keys = std::collections::HashSet::with_capacity(declarations.len());
        if !declarations.iter().all(|record| keys.insert(&record.key)) {
            return Err(ResolutionCompilerFailure::DuplicateDeclarationKey.into());
        }
        let nodes_by_path = scopes.nodes_by_path();
        let by_node = super::NodeRecords::build(
            topology.nodes.len(),
            |path| nodes_by_path.get(path).copied(),
            &declarations,
            &dependent_declarations,
            &lexical_uses,
            &deferred_uses,
        );
        Ok(Tables {
            scopes: scopes.records,
            prelude: prelude.records,
            declarations,
            dependent_declarations,
            lexical_uses,
            deferred_uses,
            postconditions,
            interface_functions,
            by_node,
            items: items
                .into_iter()
                .map(|item| item.map(|(key, _)| key))
                .collect(),
        })
    }
}

/// Resolves each result route's leading variant label against its route's
/// result type [TYPE-6, CALL-4]: the label names a prelude variant written
/// unqualified, or a variant of the source enum a declared result ordinal
/// has. A route with an ordinal binder looks only at that ordinal's type.
#[allow(clippy::too_many_arguments)]
fn resolve_route_variants(
    topology: &FinalizedTopology,
    roles: &[ClassifiedRole],
    scopes: &ScopeBuild,
    declarations: &[DeclarationRecord],
    metas: &[DeclarationMeta],
    index: &DeclarationIndex,
    modules: &ModuleView<'_>,
    routes: &[UseMeta],
    lexical_uses: &mut Vec<LexicalUseRecord>,
) -> Result<Option<ResolutionIssue>, ResolutionCompilerFailure> {
    for route in routes {
        let (mut resolved, issue) = resolve_uses_deferred(
            scopes,
            declarations,
            metas,
            index,
            modules,
            std::slice::from_ref(route),
        )?;
        if issue.is_none() {
            lexical_uses.append(&mut resolved);
            continue;
        }
        let function = function_owner(topology, route.owner)
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
        let ordinal = selector_spelling(roles, route.owner, |selector| {
            matches!(selector, SelectorRole::ResultOrdinal)
        });
        let mut found = None;
        for binding in topology
            .node_children(function)
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?
            .iter()
            .copied()
            .filter(|child| {
                topology
                    .node(*child)
                    .is_some_and(|record| record.production == Production::ResultBinding)
            })
        {
            if let Some(ordinal) = &ordinal
                && selector_spelling(roles, binding, |selector| {
                    matches!(selector, SelectorRole::PlainCandidate)
                })
                .as_deref()
                    != Some(ordinal.as_str())
            {
                continue;
            }
            // `result_binding := IDENT ":" rtype` and `rtype := type`.
            let child_with = |node: NodeId, production: Production| {
                topology.node_children(node).and_then(|children| {
                    children.iter().copied().find(|child| {
                        topology
                            .node(*child)
                            .is_some_and(|record| record.production == production)
                    })
                })
            };
            let Some(ty) = child_with(binding, Production::Rtype)
                .and_then(|rtype| child_with(rtype, Production::Type))
            else {
                continue;
            };
            let path = scopes.path(ty)?;
            let Some(ResolvedTarget::Source { declaration, .. }) = lexical_uses
                .iter()
                .find(|usage| usage.role() == LexicalUseRole::Type && usage.origin().node() == path)
                .map(LexicalUseRecord::target)
            else {
                continue;
            };
            let Some(enum_meta) = metas.get(declaration.index()) else {
                continue;
            };
            if let Some(variant) = metas.iter().find(|meta| {
                meta.type_owned
                    && meta.owner == enum_meta.owner
                    && declarations[meta.record_index].spelling == route.spelling
            }) {
                found = Some(declarations[variant.record_index].id);
                break;
            }
        }
        match found {
            Some(declaration) => lexical_uses.push(LexicalUseRecord {
                role: route.role,
                spelling: route.spelling.clone(),
                origin: route.origin.clone(),
                target: ResolvedTarget::Source {
                    declaration,
                    class: DeclarationClass::EnumVariant,
                },
            }),
            None => return Ok(issue),
        }
    }
    Ok(None)
}

/// The spelling of the one selector role of `kind` a node carries: a
/// route's written ordinal binder, `when b is V(...)`, or a result binding's
/// binder.
fn selector_spelling(
    roles: &[ClassifiedRole],
    node: NodeId,
    kind: fn(SelectorRole) -> bool,
) -> Option<String> {
    roles.iter().find_map(|role| match role.kind {
        RawRoleKind::Selector(selector) if role.owner == node && kind(selector) => {
            Some(role.spelling.clone())
        }
        _ => None,
    })
}

/// [SET-1, GRAM-4, GRAM-5] every `pbase` role that is a bare `set` target: the
/// base of a target `place` of a `set_stmt`, written with no `psuffix` and no
/// `deref`.
///
/// Each such role index maps to its `place` node, which is the location
/// [SET-1] states for the rejection when the target name resolves to nothing.
fn bare_set_target_roles(
    topology: &FinalizedTopology,
    roles: &[ClassifiedRole],
) -> Result<HashMap<usize, NodeId>, ResolutionCompilerFailure> {
    let mut candidates = HashMap::new();
    for (index, role) in roles.iter().enumerate() {
        if !matches!(
            role.kind,
            RawRoleKind::LexicalUse(super::LexicalUseRole::PlaceBase)
        ) {
            continue;
        }
        let pbase = role.owner;
        let record = topology
            .node(pbase)
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
        if record.production != Production::Pbase || record.terminal_count != 1 {
            continue;
        }
        // `pbase := IDENT | "deref" "(" place ")"`: a `deref` base owns a
        // nested `place` child, and this role's spelling would then be that
        // inner base's, so a base with any child is never a bare target.
        if topology
            .node_children(pbase)
            .is_some_and(|children| !children.is_empty())
        {
            continue;
        }
        let Some(place) = record.parent else {
            continue;
        };
        let place_record = topology
            .node(place)
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
        if place_record.production != Production::Place {
            continue;
        }
        // `place := pbase psuffix*`: a projected or subscripted target selects
        // one component of a value that must already exist, so it declares
        // nothing.
        if topology.node_children(place).is_some_and(|children| {
            children.iter().any(|child| {
                topology
                    .node(*child)
                    .is_some_and(|record| record.production == Production::Psuffix)
            })
        }) {
            continue;
        }
        let Some(statement) = place_record.parent else {
            continue;
        };
        if topology
            .node(statement)
            .is_some_and(|record| record.production == Production::SetStmt)
        {
            candidates.insert(index, place);
        }
    }
    Ok(candidates)
}

/// [SET-1] re-attributes an unresolved bare `set` target.
///
/// [DIAG-1]'s role table sends an unresolved `pbase` IDENT to [TYPE-5] at that
/// IDENT. [SET-1] states its own rule and its own location for the one case it
/// owns — a `set` target name that resolves to nothing — and a rule's own
/// stated location wins, so the issue is rebuilt at the complete `place`.
/// Every other unresolved use passes through unchanged.
fn set_target_attribution(
    topology: &FinalizedTopology,
    scopes: &ScopeBuild,
    roles: &[ClassifiedRole],
    bare_set_targets: &HashMap<usize, NodeId>,
    issue: ResolutionIssue,
) -> Result<ResolutionIssue, ResolutionCompilerFailure> {
    let spelling = match &issue.kind {
        ResolutionIssueKind::UnresolvedUse { spelling, .. } => spelling.clone(),
        _ => return Ok(issue),
    };
    let Some(place) = bare_set_targets.iter().find_map(|(index, place)| {
        roles
            .get(*index)
            .is_some_and(|role| role.origin == issue.origin)
            .then_some(*place)
    }) else {
        return Ok(issue);
    };
    let record = topology
        .node(place)
        .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
    let FinalizedExtent::Source { source, start, end } = record.extent else {
        return Err(ResolutionCompilerFailure::InvalidCanonicalTree);
    };
    Ok(ResolutionIssue {
        rule: super::ResolutionRule::Set1,
        origin: SourceOrigin {
            node: scopes.path(place)?.clone(),
            coordinate: crate::SyntaxCoordinate::new(source, start, end),
            extent: crate::SyntaxCoordinate::new(source, start, end),
            role_ordinal: 0,
            subtoken_ordinal: 0,
        },
        kind: ResolutionIssueKind::UndeclaredSetTarget {
            mechanical_fix: format!(
                "no binding `{spelling}` is in scope, so this `set` declares nothing: write it as `let {spelling} = ...;`, keeping its right-hand side, to declare the binding here, or name a binding that is in scope"
            ),
            spelling,
        },
    })
}

#[allow(clippy::too_many_arguments)]
fn build_postcondition_records(
    topology: &FinalizedTopology,
    scopes: &ScopeBuild,
    roles: &[ClassifiedRole],
    declarations: &[DeclarationRecord],
    declaration_metas: &[DeclarationMeta],
    declaration_index: &DeclarationIndex,
    modules: &ModuleView<'_>,
    entry_uses: &[UseMeta],
    lexical_uses: &[LexicalUseRecord],
) -> Result<Vec<PostconditionResolutionRecord>, BuildStop> {
    let mut blocks = Vec::new();
    for (index, record) in topology.nodes.iter().enumerate() {
        if record.production == Production::EnsuresClause {
            blocks
                .push(NodeId::from_index(index).ok_or(ResolutionCompilerFailure::CounterOverflow)?);
        }
    }
    blocks.sort_by_key(|block| {
        let record = &topology.nodes[block.index()];
        let (source, start, end) = match record.extent {
            FinalizedExtent::Source { source, start, end } => {
                (source.ordinal(), start.value(), end.value())
            }
            FinalizedExtent::BundleRoot => (u32::MAX, u64::MAX, u64::MAX),
        };
        let path = scopes
            .path(*block)
            .map_or_else(|_| Vec::new(), |path| path.components().to_vec());
        (source, start, end, path)
    });

    let mut out = Vec::with_capacity(blocks.len());
    for block in blocks {
        let function = function_owner(topology, block)
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
        // [GRAM-2] the declaration writes one result or an ordered result
        // list; every ordinal's binder is a candidate a clause may name
        // [CALL-4].
        let result_bindings = topology
            .node_children(function)
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?
            .iter()
            .copied()
            .filter(|child| {
                topology
                    .node(*child)
                    .is_some_and(|record| record.production == Production::ResultBinding)
            })
            .collect::<Vec<_>>();
        let result_binding = result_bindings
            .first()
            .copied()
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
        let mut result_binders = Vec::with_capacity(result_bindings.len());
        for binding in &result_bindings {
            let [candidate] = roles
                .iter()
                .filter(|role| role.owner == *binding)
                .collect::<Vec<_>>()[..]
            else {
                return Err(ResolutionCompilerFailure::InvalidRoleShape.into());
            };
            if !matches!(
                candidate.kind,
                RawRoleKind::Selector(SelectorRole::PlainCandidate)
            ) {
                return Err(ResolutionCompilerFailure::InvalidRoleShape.into());
            }
            result_binders.push(build_postcondition_candidate(
                topology,
                scopes,
                candidate,
                None,
                block,
                declarations,
                declaration_metas,
                declaration_index,
                roles,
            )?);
        }
        let route = topology
            .node_children(block)
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?
            .iter()
            .copied()
            .find(|child| {
                topology
                    .node(*child)
                    .is_some_and(|record| record.production == Production::ResultRoute)
            });
        let selector = route.unwrap_or(result_binding);
        let selector_path = scopes.path(selector)?;
        let selector_roles: Vec<_> = roles.iter().filter(|role| role.owner == selector).collect();
        // An unrouted clause's selector node is the first result binder, whose
        // candidate role is one of `result_binders` above; a routed clause's
        // selector node is the route, which carries its variant and its
        // optional ordinal binder [CALL-4].
        let (class, route_ordinal, variant_target) = if route.is_none() {
            (PostconditionSelectorClass::Plain, None, None)
        } else {
            let (route_ordinal, variant) = match selector_roles.as_slice() {
                [variant] => (None, *variant),
                [ordinal, variant] => {
                    if !matches!(
                        ordinal.kind,
                        RawRoleKind::Selector(SelectorRole::ResultOrdinal)
                    ) {
                        return Err(ResolutionCompilerFailure::InvalidRoleShape.into());
                    }
                    (Some(ordinal.spelling.clone()), *variant)
                }
                _ => return Err(ResolutionCompilerFailure::InvalidRoleShape.into()),
            };
            if !matches!(
                variant.kind,
                RawRoleKind::LexicalUse(LexicalUseRole::EnsuresVariant)
            ) {
                return Err(ResolutionCompilerFailure::InvalidRoleShape.into());
            }
            let target = lexical_uses
                .iter()
                .find(|usage| {
                    usage.role() == LexicalUseRole::EnsuresVariant
                        && usage.origin().node() == selector_path
                })
                .map(LexicalUseRecord::target)
                .ok_or(ResolutionCompilerFailure::InvalidRoleShape)?;
            (
                PostconditionSelectorClass::Variant,
                route_ordinal,
                Some(target),
            )
        };

        let mut field_roles: Vec<_> = roles
            .iter()
            .filter(|role| {
                matches!(
                    role.kind,
                    RawRoleKind::Selector(SelectorRole::VariantField)
                        | RawRoleKind::Selector(SelectorRole::VariantCandidate)
                ) && ancestor_with_production(topology, role.owner, Production::EnsuresClause)
                    == Some(block)
            })
            .collect();
        field_roles.sort_by_key(|role| EventKey::from_origin(&role.origin));
        let mut fields = Vec::new();
        for pair in field_roles.chunks(2) {
            let [field, candidate] = pair else {
                return Err(ResolutionCompilerFailure::InvalidRoleShape.into());
            };
            if field.owner != candidate.owner
                || !matches!(
                    field.kind,
                    RawRoleKind::Selector(SelectorRole::VariantField)
                )
                || !matches!(
                    candidate.kind,
                    RawRoleKind::Selector(SelectorRole::VariantCandidate)
                )
            {
                return Err(ResolutionCompilerFailure::InvalidRoleShape.into());
            }
            fields.push(PostconditionFieldRecord {
                spelling: field.spelling.clone(),
                origin: field.origin.clone(),
                candidate: build_postcondition_candidate(
                    topology,
                    scopes,
                    candidate,
                    Some(field.spelling.clone()),
                    block,
                    declarations,
                    declaration_metas,
                    declaration_index,
                    roles,
                )?,
            });
        }
        if class == PostconditionSelectorClass::Plain && !fields.is_empty() {
            return Err(ResolutionCompilerFailure::InvalidRoleShape.into());
        }

        let candidate_spellings: Vec<&str> = result_binders
            .iter()
            .map(|candidate| candidate.spelling.as_str())
            .chain(fields.iter().map(|field| field.candidate.spelling.as_str()))
            .collect();
        let mut ordinary_entry_uses = Vec::new();
        let mut selector_uses = Vec::new();
        for use_record in entry_uses.iter().filter(|use_record| {
            ancestor_with_production(topology, use_record.owner, Production::EnsuresClause)
                == Some(block)
        }) {
            if use_record.role == LexicalUseRole::PlaceBase
                && candidate_spellings.contains(&use_record.spelling.as_str())
            {
                selector_uses.push(PostconditionSelectorUseRecord {
                    spelling: use_record.spelling.clone(),
                    origin: use_record.origin.clone(),
                });
            } else {
                ordinary_entry_uses.push(use_record.clone());
            }
        }
        let entry_inventory_issue = None;
        let (provisional_uses, entry_resolution_issue) = if ordinary_entry_uses.is_empty() {
            (Vec::new(), None)
        } else {
            let (resolved, issue) = resolve_uses_deferred(
                scopes,
                declarations,
                declaration_metas,
                declaration_index,
                modules,
                &ordinary_entry_uses,
            )?;
            if issue.is_some() {
                (Vec::new(), issue)
            } else {
                (resolved, None)
            }
        };
        out.push(PostconditionResolutionRecord {
            function: scopes.path(function)?.clone(),
            block: scopes.path(block)?.clone(),
            selector: selector_path.clone(),
            class,
            result_binders,
            route_ordinal,
            fields,
            variant_target,
            provisional_uses,
            selector_uses,
            entry_inventory_issue,
            entry_resolution_issue,
        });
    }
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn build_postcondition_candidate(
    topology: &FinalizedTopology,
    scopes: &ScopeBuild,
    role: &ClassifiedRole,
    paired_field: Option<String>,
    block: NodeId,
    declarations: &[DeclarationRecord],
    metas: &[DeclarationMeta],
    index: &DeclarationIndex,
    roles: &[ClassifiedRole],
) -> Result<PostconditionCandidateRecord, ResolutionCompilerFailure> {
    let mut live_conflicts = Vec::new();
    for meta in index
        .with_spelling(&role.spelling)
        .iter()
        .filter_map(|candidate| metas.get(*candidate))
    {
        if !scopes.is_unit_scope(meta.scope)
            && meta
                .owner
                .is_some_and(|owner| !role.owner_chain.contains(&owner))
        {
            continue;
        }
        if !meta.entries.iter().any(|class| {
            matches!(
                class,
                DeclarationClass::Function
                    | DeclarationClass::NamedConst
                    | DeclarationClass::ConstGeneric
                    | DeclarationClass::Value
            )
        }) || !is_visible(
            scopes,
            meta,
            role.scope,
            role.origin.coordinate.source().ordinal(),
            role.origin.coordinate.start().value(),
        ) {
            continue;
        }
        live_conflicts.push(declarations[meta.record_index].origin.clone());
    }
    live_conflicts.sort_by_key(EventKey::from_origin);
    let later_local_collision = metas
        .iter()
        .filter_map(|meta| {
            let declaration = declarations.get(meta.record_index)?;
            let classified = roles.get(meta.role_index)?;
            Some((declaration, classified.owner))
        })
        .filter(|(declaration, _)| {
            declaration.role == DeclarationRole::Let
                && declaration.spelling == role.spelling
                && EventKey::from_origin(&declaration.origin) > EventKey::from_origin(&role.origin)
        })
        .find(|(_, owner)| {
            ancestor_with_production(topology, *owner, Production::ContractBlock)
                == ancestor_with_production(topology, block, Production::ContractBlock)
        })
        .map(|(declaration, _)| declaration.origin.clone());
    Ok(PostconditionCandidateRecord {
        spelling: role.spelling.clone(),
        origin: role.origin.clone(),
        paired_field,
        live_conflicts,
        later_local_collision,
    })
}

fn owner_chain(
    topology: &FinalizedTopology,
    mut node: NodeId,
) -> Result<Vec<NodeId>, ResolutionCompilerFailure> {
    let mut owners = Vec::new();
    loop {
        let record = topology
            .node(node)
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
        if matches!(
            record.production,
            Production::FnSig
                | Production::FnDecl
                | Production::StructDecl
                | Production::EnumDecl
                | Production::InterfaceDecl
                | Production::BindingDecl
        ) {
            owners.push(node);
        }
        let Some(parent) = record.parent else {
            break;
        };
        node = parent;
    }
    Ok(owners)
}

fn function_owner(topology: &FinalizedTopology, mut node: NodeId) -> Option<NodeId> {
    loop {
        let record = topology.node(node)?;
        if matches!(record.production, Production::FnDecl | Production::FnSig) {
            return Some(node);
        }
        node = record.parent?;
    }
}

/// Whether a declaration role is one module-level item declaration, or a
/// variant of one [MOD-3].
const fn is_top_level_role(role: DeclarationRole) -> bool {
    matches!(
        role,
        DeclarationRole::Function
            | DeclarationRole::Struct
            | DeclarationRole::Enum
            | DeclarationRole::Variant
            | DeclarationRole::Interface
            | DeclarationRole::Binding
            | DeclarationRole::NamedConst
    )
}

/// The key of every item of the unit, by its ordinal among the root's
/// children, with the index of the role that heads it: the item's function,
/// struct, enum, interface, binding or constant declaration by home, role and
/// spelling, or the alias it binds. `program no_heap` heads none.
fn item_keys(
    topology: &FinalizedTopology,
    roles: &[ClassifiedRole],
    bundle: &crate::SourceBundle,
) -> Result<Vec<Option<(ItemKey, usize)>>, ResolutionCompilerFailure> {
    let count = topology
        .node_children(topology.root)
        .ok_or(ResolutionCompilerFailure::InvalidRoleShape)?
        .len();
    let mut items = vec![None; count];
    for (index, role) in roles.iter().enumerate() {
        let RawRoleKind::Declaration(declaration_role) = role.kind else {
            continue;
        };
        let heads = matches!(
            declaration_role,
            DeclarationRole::Function
                | DeclarationRole::Struct
                | DeclarationRole::Enum
                | DeclarationRole::Interface
                | DeclarationRole::Binding
                | DeclarationRole::NamedConst
                | DeclarationRole::Alias
        );
        let Some(&ordinal) = role.origin.node.components().first() else {
            continue;
        };
        let slot = items
            .get_mut(ordinal as usize)
            .ok_or(ResolutionCompilerFailure::InvalidRoleShape)?;
        if !heads || slot.is_some() {
            continue;
        }
        let file = bundle
            .file(role.origin.coordinate.source())
            .ok_or(ResolutionCompilerFailure::InvalidRoleShape)?;
        let key = if declaration_role == DeclarationRole::Alias {
            ItemKey::Alias {
                source: file.logical_path().as_str().to_owned(),
                spelling: role.spelling.clone(),
            }
        } else {
            ItemKey::Declared {
                home: ItemHome::of(file, bundle)
                    .ok_or(ResolutionCompilerFailure::InvalidRoleShape)?,
                role: declaration_role,
                spelling: role.spelling.clone(),
            }
        };
        *slot = Some((key, index));
    }
    Ok(items)
}

/// The key of the declaration role `index` forms: its item's key when it
/// heads the item, and otherwise its place within the item.
fn declaration_key(
    origin: &SourceOrigin,
    index: usize,
    items: &[Option<(ItemKey, usize)>],
) -> Result<DeclarationKey, ResolutionCompilerFailure> {
    let (first, rest) = origin
        .node
        .components()
        .split_first()
        .ok_or(ResolutionCompilerFailure::InvalidRoleShape)?;
    let (item, head) = items
        .get(*first as usize)
        .and_then(Option::as_ref)
        .ok_or(ResolutionCompilerFailure::InvalidRoleShape)?;
    Ok(if *head == index {
        DeclarationKey::Item(item.clone())
    } else {
        DeclarationKey::Local {
            item: item.clone(),
            path: rest.to_vec(),
            ordinal: (origin.role_ordinal, origin.subtoken_ordinal),
        }
    })
}

/// Whether the item that owns this declaration node writes `public` [MOD-6].
fn declares_public(
    topology: &FinalizedTopology,
    classified: &crate::ClassifiedBundle,
    declaration: NodeId,
) -> bool {
    topology
        .node(declaration)
        .and_then(|record| record.parent)
        .filter(|item| {
            topology
                .node(*item)
                .is_some_and(|record| record.production == Production::Item)
        })
        .is_some_and(|item| writes_fixed(topology, classified, item, FixedTerminal::Public))
}

/// Whether a `fn_decl` writes a body rather than ending in `;` or its `doc`
/// entry [GRAM-2, MOD-7].
fn has_body(
    topology: &FinalizedTopology,
    classified: &crate::ClassifiedBundle,
    declaration: NodeId,
) -> bool {
    writes_fixed(topology, classified, declaration, FixedTerminal::LeftBrace)
}

/// Whether one node writes this fixed terminal directly.
fn writes_fixed(
    topology: &FinalizedTopology,
    classified: &crate::ClassifiedBundle,
    node: NodeId,
    terminal: FixedTerminal,
) -> bool {
    topology
        .terminals
        .iter()
        .enumerate()
        .any(|(index, record)| {
            record.owner == Some(node)
                && classified.tokens().get(index).is_some_and(|token| {
                    token
                        .terminals()
                        .contains(TerminalPredicate::Fixed(terminal))
                })
        })
}

fn declaration_classes(role: DeclarationRole) -> Vec<DeclarationClass> {
    match role {
        // An alias takes its target's classes once the target resolves.
        DeclarationRole::Alias => Vec::new(),
        DeclarationRole::Function => vec![DeclarationClass::Function],
        DeclarationRole::FunctionParameter => vec![DeclarationClass::FunctionParameter],
        DeclarationRole::Struct => vec![
            DeclarationClass::NominalType,
            DeclarationClass::StructConstructor,
        ],
        DeclarationRole::Enum => vec![DeclarationClass::NominalType],
        DeclarationRole::Variant => vec![DeclarationClass::EnumVariant],
        DeclarationRole::Interface => vec![DeclarationClass::Interface],
        DeclarationRole::Binding => vec![DeclarationClass::Binding],
        DeclarationRole::NamedConst => vec![DeclarationClass::NamedConst],
        DeclarationRole::GenericType => vec![DeclarationClass::GenericType],
        DeclarationRole::ConstGeneric => vec![DeclarationClass::ConstGeneric],
        DeclarationRole::Parameter
        | DeclarationRole::Let
        | DeclarationRole::MatchBinder
        | DeclarationRole::CountedBinder => {
            vec![DeclarationClass::Value]
        }
        DeclarationRole::LoopLabel => vec![DeclarationClass::Label],
        DeclarationRole::Invariant => vec![DeclarationClass::Invariant],
    }
}

fn declaration_scope(
    role: &ClassifiedRole,
    declaration_role: DeclarationRole,
    scopes: &ScopeBuild,
) -> Result<ScopeId, ResolutionCompilerFailure> {
    match declaration_role {
        DeclarationRole::Variant => scopes.node_scope(
            *role
                .owner_chain
                .first()
                .ok_or(ResolutionCompilerFailure::InvalidRoleShape)?,
        ),
        DeclarationRole::LoopLabel => scopes.declaration_scope(role.owner),
        _ => Ok(role.scope),
    }
}

fn declaration_visibility(
    topology: &FinalizedTopology,
    role: &ClassifiedRole,
    declaration_role: DeclarationRole,
) -> Result<Visibility, ResolutionCompilerFailure> {
    if declaration_role == DeclarationRole::Function {
        return Ok(Visibility::Always);
    }
    let coordinate = role.origin.coordinate;
    let byte = match declaration_role {
        DeclarationRole::NamedConst
        | DeclarationRole::ConstGeneric
        | DeclarationRole::FunctionParameter
        | DeclarationRole::Parameter
        | DeclarationRole::CountedBinder
        | DeclarationRole::Invariant => node_end(topology, role.owner)?.value(),
        // [PROV-6, GRAM-4] a destructuring consume's binder is owned by its
        // `fieldbind`, and the whole statement is where it becomes visible,
        // exactly as an ordinary `let` binder's own statement is. A `set`
        // target declares nothing any more [SET-1], so `let_stmt` is the only
        // statement a `Let` declaration can be owned by.
        DeclarationRole::Let => {
            let owner = ancestor_with_production(topology, role.owner, Production::LetStmt)
                .unwrap_or(role.owner);
            node_end(topology, owner)?.value()
        }
        DeclarationRole::MatchBinder => {
            let list = ancestor_with_production(topology, role.owner, Production::FieldbindList)
                .ok_or(ResolutionCompilerFailure::InvalidRoleShape)?;
            node_end(topology, list)?.value()
        }
        _ => coordinate.end().value(),
    };
    Ok(Visibility::After {
        source: coordinate.source().ordinal(),
        byte,
    })
}

fn node_end(
    topology: &FinalizedTopology,
    node: NodeId,
) -> Result<ByteOffset, ResolutionCompilerFailure> {
    let record = topology
        .node(node)
        .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
    let FinalizedExtent::Source { end, .. } = record.extent else {
        return Err(ResolutionCompilerFailure::InvalidCanonicalTree);
    };
    Ok(end)
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

fn is_visible(
    scopes: &ScopeBuild,
    declaration: &DeclarationMeta,
    use_scope: ScopeId,
    use_source: u32,
    use_byte: u64,
) -> bool {
    if !scopes.is_ancestor(declaration.scope, use_scope) {
        return false;
    }
    match declaration.visibility {
        Visibility::Always => true,
        Visibility::After { source, byte } => {
            use_source > source || (use_source == source && use_byte >= byte)
        }
    }
}

fn declaration_domain(class: DeclarationClass) -> Option<DeclarationDomain> {
    match class {
        DeclarationClass::Function
        | DeclarationClass::FunctionParameter
        | DeclarationClass::NamedConst
        | DeclarationClass::ConstGeneric
        | DeclarationClass::Value => Some(DeclarationDomain::LexicalIdentifier),
        DeclarationClass::GenericType
        | DeclarationClass::NominalType
        | DeclarationClass::Interface
        | DeclarationClass::Binding => Some(DeclarationDomain::NominalType),
        DeclarationClass::StructConstructor | DeclarationClass::EnumVariant => {
            Some(DeclarationDomain::Constructor)
        }
        DeclarationClass::NumericBound => Some(DeclarationDomain::NumericBound),
        // [MOD-4] a module alias occupies its lowercase spelling beside the
        // module's functions and constants.
        DeclarationClass::Module => Some(DeclarationDomain::LexicalIdentifier),
        DeclarationClass::Label => Some(DeclarationDomain::Label),
        DeclarationClass::Invariant => Some(DeclarationDomain::Invariant),
        DeclarationClass::OperationFamily => None,
    }
}
