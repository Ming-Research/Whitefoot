use std::collections::HashSet;

use super::super::catalog::{PRELUDE_DECLARATIONS, operation_id};
use super::super::scopes::ScopeBuild;
use super::super::{
    DeclarationClass, DeclarationOrigin, DeclarationRecord, LexicalUseRecord, LexicalUseRole,
    ResolutionCompilerFailure, ResolutionIssue, ResolutionIssueKind, ResolutionRule,
    ResolvedTarget, SourceOrigin,
};
use super::inventory::conflict_key;
use super::{
    AliasTarget, DeclarationIndex, DeclarationMeta, FunctionForm, PathSegment, Qualifier, UseMeta,
    is_visible,
};

/// The module program facts lookup reads: its registered modules and each
/// module's inventory scope [MOD-3, MOD-5].
pub(super) struct ModuleView<'a> {
    pub(super) modules: &'a [crate::ModuleRecord],
}

impl ModuleView<'_> {
    fn name(&self, module: crate::ModuleId) -> String {
        self.modules
            .get(module.index())
            .map_or_else(|| "pkg".to_owned(), crate::ModuleRecord::qualified_name)
    }

    fn find(&self, package: crate::Package, path: &[String]) -> Option<crate::ModuleId> {
        self.modules
            .iter()
            .position(|module| module.is_at(package, path))
            .and_then(crate::ModuleId::from_index)
    }

    /// The package a module belongs to; a source bundle's root module is the
    /// program's own [MOD-10].
    fn package(&self, module: crate::ModuleId) -> crate::Package {
        self.modules
            .get(module.index())
            .map_or(crate::Package::Program, crate::ModuleRecord::package)
    }

    /// [MOD-5] a source may name its own module and the modules its graph
    /// row lists; no other edge, transitive or ancestral, grants a name.
    fn permits(&self, from: crate::ModuleId, to: crate::ModuleId) -> bool {
        from == to
            || self
                .modules
                .get(from.index())
                .is_some_and(|module| module.depends_on(to))
    }
}

fn segment_origin(use_origin: &SourceOrigin, segment: &PathSegment) -> SourceOrigin {
    SourceOrigin {
        node: use_origin.node.clone(),
        coordinate: segment.coordinate,
        extent: use_origin.extent,
        role_ordinal: use_origin.role_ordinal,
        subtoken_ordinal: 0,
    }
}

fn written_path(qualifier: &Qualifier, last: &str) -> String {
    let mut text = qualifier.alias_root.as_ref().map_or_else(
        || if qualifier.standard { "std" } else { "pkg" }.to_owned(),
        |root| root.spelling.clone(),
    );
    for segment in &qualifier.segments {
        text.push_str("::");
        text.push_str(&segment.spelling);
    }
    if !last.is_empty() {
        text.push_str("::");
        text.push_str(last);
    }
    text
}

/// Resolves the module prefix of a qualified use [MOD-5]: a `pkg` root, which
/// names the use's own package, a `std` root, which names the standard
/// library from every other package [MOD-10], or a file-local module alias,
/// then each lowercase segment. The resolved module must be the use's own
/// module or a direct dependency of it.
#[allow(clippy::too_many_arguments)]
fn resolve_module_prefix(
    scopes: &ScopeBuild,
    declarations: &[DeclarationRecord],
    metas: &[DeclarationMeta],
    index: &DeclarationIndex,
    modules: &ModuleView<'_>,
    qualifier: &Qualifier,
    use_record: &UseMeta,
) -> Result<Result<crate::ModuleId, ResolutionIssue>, ResolutionCompilerFailure> {
    let mut path = Vec::new();
    let mut package = modules.package(use_record.module);
    if qualifier.alias_root.is_none() && qualifier.standard {
        if package == crate::Package::Standard {
            return Ok(Err(ResolutionIssue {
                rule: ResolutionRule::Mod10,
                origin: use_record.origin.clone(),
                kind: ResolutionIssueKind::LibraryNamesItself {
                    path: written_path(qualifier, ""),
                },
            }));
        }
        package = crate::Package::Standard;
    }
    if let Some(root) = &qualifier.alias_root {
        let alias = index
            .with_spelling(&root.spelling)
            .iter()
            .filter_map(|candidate| metas.get(*candidate))
            .find(|meta| {
                declarations[meta.record_index].role == super::super::DeclarationRole::Alias
                    && scopes.is_ancestor(meta.scope, use_record.scope)
            });
        let Some(AliasTarget::Module(module)) = alias.and_then(|meta| meta.alias) else {
            return Ok(Err(ResolutionIssue {
                rule: ResolutionRule::Mod5,
                origin: segment_origin(&use_record.origin, root),
                kind: ResolutionIssueKind::UnknownModule {
                    path: written_path(qualifier, ""),
                },
            }));
        };
        let record = modules
            .modules
            .get(module.index())
            .ok_or(ResolutionCompilerFailure::InvalidScopeTree)?;
        package = record.package();
        path.extend(record.path().iter().cloned());
    }
    path.extend(
        qualifier
            .segments
            .iter()
            .map(|segment| segment.spelling.clone()),
    );
    let Some(module) = modules.find(package, &path) else {
        let origin = qualifier
            .segments
            .last()
            .or(qualifier.alias_root.as_ref())
            .map_or_else(
                || use_record.origin.clone(),
                |segment| segment_origin(&use_record.origin, segment),
            );
        return Ok(Err(ResolutionIssue {
            rule: ResolutionRule::Mod5,
            origin,
            kind: ResolutionIssueKind::UnknownModule {
                path: written_path(qualifier, ""),
            },
        }));
    };
    if !modules.permits(use_record.module, module) {
        return Ok(Err(ResolutionIssue {
            rule: ResolutionRule::Mod5,
            origin: use_record.origin.clone(),
            kind: ResolutionIssueKind::MissingModuleEdge {
                from: modules.name(use_record.module),
                to: modules.name(module),
            },
        }));
    }
    Ok(Ok(module))
}

/// The target a candidate declaration contributes in one class, following an
/// alias to the identity it binds [MOD-4].
fn candidate_target(
    declarations: &[DeclarationRecord],
    metas: &[DeclarationMeta],
    meta: &DeclarationMeta,
    class: DeclarationClass,
) -> Result<ResolvedTarget, ResolutionCompilerFailure> {
    let meta = match meta.alias {
        Some(AliasTarget::Declaration(target)) => metas
            .get(target)
            .ok_or(ResolutionCompilerFailure::InvalidRoleShape)?,
        Some(AliasTarget::Module(_)) => return Err(ResolutionCompilerFailure::InvalidRoleShape),
        None => meta,
    };
    Ok(if let Some(container) = meta.container {
        ResolvedTarget::Container(container)
    } else {
        ResolvedTarget::Source {
            declaration: declarations[meta.record_index].id,
            class,
        }
    })
}

/// Whether a candidate is an interface declaration whose definition stands
/// for it in lookup [MOD-7].
const fn defers_to_definition(meta: &DeclarationMeta) -> bool {
    matches!(
        meta.function_form,
        FunctionForm::Declaration {
            definition: Some(_)
        }
    )
}

#[allow(clippy::too_many_lines)]
pub(super) fn resolve_uses_deferred(
    scopes: &ScopeBuild,
    declarations: &[DeclarationRecord],
    metas: &[DeclarationMeta],
    index: &DeclarationIndex,
    modules: &ModuleView<'_>,
    uses: &[UseMeta],
) -> Result<(Vec<LexicalUseRecord>, Option<ResolutionIssue>), ResolutionCompilerFailure> {
    let mut resolved = Vec::with_capacity(uses.len());
    // [TYPE-6] a type-owned variant member resolves only once its owner has,
    // so members wait for the second pass below.
    let mut members = Vec::new();
    for use_record in uses {
        if use_record.member_owner.is_some() {
            members.push(use_record);
            continue;
        }
        let admissible = admissible_classes(use_record.role, &use_record.spelling);
        if let Some(qualifier) = &use_record.qualifier {
            match resolve_qualified(
                scopes,
                declarations,
                metas,
                index,
                modules,
                qualifier,
                use_record,
                &admissible,
            )? {
                Ok(target) => resolved.push(LexicalUseRecord {
                    role: use_record.role,
                    spelling: use_record.spelling.clone(),
                    origin: use_record.origin.clone(),
                    target,
                }),
                Err(issue) => return Ok((resolved, Some(issue))),
            }
            continue;
        }
        let universe = universe_classes(use_record.role);
        let mut candidates = Vec::new();
        let mut invisible = Vec::new();
        let mut available = HashSet::new();
        for meta in index
            .with_spelling(&use_record.spelling)
            .iter()
            .filter_map(|candidate| metas.get(*candidate))
        {
            let declaration = &declarations[meta.record_index];
            if !scopes.is_unit_scope(meta.scope)
                && meta
                    .owner
                    .is_some_and(|owner| !use_record.owner_chain.contains(&owner))
            {
                continue;
            }
            // [MOD-3, TYPE-6] another module's inventory and another file's
            // aliases are not in scope at all, and a user variant enters no
            // unqualified inventory; none of them is a hidden candidate.
            if (scopes.is_unit_scope(meta.scope)
                && !scopes.is_ancestor(meta.scope, use_record.scope))
                || meta.type_owned
                || defers_to_definition(meta)
                || matches!(meta.alias, Some(AliasTarget::Module(_)))
            {
                continue;
            }
            for class in &meta.entries {
                if !universe.contains(class) {
                    continue;
                }
                // [MOD-3] an interface record sees its module's interface
                // declarations; an implementation-only one is out of its
                // view, as an earlier binder is out of a later one's.
                let visible = is_visible(
                    scopes,
                    meta,
                    use_record.scope,
                    use_record.origin.coordinate.source().ordinal(),
                    use_record.origin.coordinate.start().value(),
                ) && !(use_record.interface && meta.implementation_only);
                if visible {
                    available.insert(*class);
                }
                if admissible.contains(class) {
                    if visible {
                        // [TYPE-2, PRE-1] the three storage shapes and the
                        // cell `Box` are declared by the prelude like any
                        // other opaque struct, but each names one
                        // compiler-owned shape [TYPE-9] and not a source
                        // struct, so a use that selects one of those four
                        // declarations resolves to its container identity in
                        // both of its domains: the nominal-type entry an
                        // `Array<T, n>` or `Box<T>` type names, and the
                        // constructor entry [TYPE-2] exists to refuse.
                        // [MOD-4] an alias resolves to its target's identity.
                        candidates.push(candidate_target(declarations, metas, meta, *class)?);
                    } else {
                        invisible.push(declaration.diagnostic_origin(*class));
                    }
                }
            }
        }
        for prelude in PRELUDE_DECLARATIONS {
            let Some(class) = prelude.class else {
                continue;
            };
            if prelude.spelling == use_record.spelling && universe.contains(&class) {
                available.insert(class);
                if admissible.contains(&class) {
                    candidates.push(ResolvedTarget::Prelude(prelude.id));
                }
            }
        }
        // x1 [TYPE-2, PRE-1]: the three storage shapes no longer stand beside
        // the declaration tables. They are prelude opaque structs, so their
        // nominal-type and constructor entries arrive through the declaration
        // loop above like `Box`'s, and the container identity is attached
        // there. A second candidate source here would make every written
        // `Array` ambiguous against its own declaration.
        if universe.contains(&DeclarationClass::OperationFamily)
            && let Some(operation) = operation_id(&use_record.spelling)
        {
            available.insert(DeclarationClass::OperationFamily);
            if admissible.contains(&DeclarationClass::OperationFamily) {
                candidates.push(ResolvedTarget::Operation(operation));
            }
        }

        if use_record.role == LexicalUseRole::BreakLabel {
            candidates.retain(|target| match target {
                ResolvedTarget::Source { declaration, .. } => metas
                    .get(declaration.index())
                    .is_some_and(|meta| meta.owner == use_record.function_owner),
                _ => false,
            });
            if candidates.is_empty() {
                let labels: Vec<_> = index
                    .with_spelling(&use_record.spelling)
                    .iter()
                    .filter_map(|candidate| metas.get(*candidate))
                    .filter(|meta| meta.owner == use_record.function_owner)
                    .filter_map(|meta| {
                        let declaration = &declarations[meta.record_index];
                        meta.entries
                            .contains(&DeclarationClass::Label)
                            .then(|| DeclarationOrigin::Source(declaration.origin.clone()))
                    })
                    .collect();
                if !labels.is_empty() {
                    return Ok((
                        resolved,
                        Some(ResolutionIssue {
                            rule: ResolutionRule::Type6,
                            origin: use_record.origin.clone(),
                            kind: ResolutionIssueKind::NonEnclosingLabel {
                                spelling: use_record.spelling.clone(),
                                role: use_record.role,
                                origins: labels,
                            },
                        }),
                    ));
                }
            }
        }

        match candidates.as_slice() {
            [target] => resolved.push(LexicalUseRecord {
                role: use_record.role,
                spelling: use_record.spelling.clone(),
                origin: use_record.origin.clone(),
                target: *target,
            }),
            [] if !invisible.is_empty() && use_record.role != LexicalUseRole::BreakLabel => {
                invisible.sort_by(|left, right| {
                    conflict_key(left, declarations).cmp(&conflict_key(right, declarations))
                });
                return Ok((
                    resolved,
                    Some(ResolutionIssue {
                        rule: use_rule(use_record.role),
                        origin: use_record.origin.clone(),
                        kind: ResolutionIssueKind::InvisibleUse {
                            spelling: use_record.spelling.clone(),
                            role: use_record.role,
                            admissible,
                            origins: invisible,
                        },
                    }),
                ));
            }
            [] => {
                let mut available: Vec<_> = available.into_iter().collect();
                available.sort_unstable();
                return Ok((
                    resolved,
                    Some(ResolutionIssue {
                        rule: use_rule(use_record.role),
                        origin: use_record.origin.clone(),
                        kind: ResolutionIssueKind::UnresolvedUse {
                            spelling: use_record.spelling.clone(),
                            role: use_record.role,
                            admissible,
                            available,
                        },
                    }),
                ));
            }
            _ => return Err(ResolutionCompilerFailure::AmbiguousResolution),
        }
    }
    for use_record in members {
        match resolve_member(declarations, metas, &resolved, use_record)? {
            Ok(target) => resolved.push(LexicalUseRecord {
                role: use_record.role,
                spelling: use_record.spelling.clone(),
                origin: use_record.origin.clone(),
                target,
            }),
            Err(issue) => return Ok((resolved, Some(issue))),
        }
    }
    Ok((resolved, None))
}

/// Resolves a qualified use in the inventory of the module its prefix names
/// [MOD-5]: a declaration of another module must be public there.
#[allow(clippy::too_many_arguments)]
fn resolve_qualified(
    scopes: &ScopeBuild,
    declarations: &[DeclarationRecord],
    metas: &[DeclarationMeta],
    index: &DeclarationIndex,
    modules: &ModuleView<'_>,
    qualifier: &Qualifier,
    use_record: &UseMeta,
    admissible: &[DeclarationClass],
) -> Result<Result<ResolvedTarget, ResolutionIssue>, ResolutionCompilerFailure> {
    let module = match resolve_module_prefix(
        scopes,
        declarations,
        metas,
        index,
        modules,
        qualifier,
        use_record,
    )? {
        Ok(module) => module,
        Err(issue) => return Ok(Err(issue)),
    };
    let inventory = scopes
        .module_scope(module)
        .ok_or(ResolutionCompilerFailure::InvalidScopeTree)?;
    let mut candidates = Vec::new();
    for meta in index
        .with_spelling(&use_record.spelling)
        .iter()
        .filter_map(|candidate| metas.get(*candidate))
        .filter(|meta| {
            meta.scope == inventory
                && !meta.type_owned
                && !defers_to_definition(meta)
                && !(use_record.interface && meta.implementation_only)
        })
    {
        for class in &meta.entries {
            if admissible.contains(class) {
                candidates.push((meta, *class));
            }
        }
    }
    match candidates.as_slice() {
        [(meta, class)] => {
            if module != use_record.module && !meta.public {
                return Ok(Err(ResolutionIssue {
                    rule: ResolutionRule::Mod5,
                    origin: use_record.origin.clone(),
                    kind: ResolutionIssueKind::PrivateDeclaration {
                        spelling: use_record.spelling.clone(),
                        module: modules.name(module),
                    },
                }));
            }
            Ok(Ok(candidate_target(declarations, metas, meta, *class)?))
        }
        [] => Ok(Err(ResolutionIssue {
            rule: ResolutionRule::Mod5,
            origin: use_record.origin.clone(),
            kind: ResolutionIssueKind::QualifiedNameNotFound {
                spelling: use_record.spelling.clone(),
                module: modules.name(module),
                role: use_record.role,
            },
        })),
        _ => Err(ResolutionCompilerFailure::AmbiguousResolution),
    }
}

/// Resolves the member TYPEID of a type-owned variant construction among
/// the variants of its already resolved owner enum [TYPE-6].
fn resolve_member(
    declarations: &[DeclarationRecord],
    metas: &[DeclarationMeta],
    resolved: &[LexicalUseRecord],
    use_record: &UseMeta,
) -> Result<Result<ResolvedTarget, ResolutionIssue>, ResolutionCompilerFailure> {
    let owner_coordinate = use_record
        .member_owner
        .ok_or(ResolutionCompilerFailure::InvalidRoleShape)?;
    let owner = resolved
        .iter()
        .find(|usage| {
            usage.role == LexicalUseRole::VariantOwner
                && usage.origin.coordinate == owner_coordinate
        })
        .ok_or(ResolutionCompilerFailure::InvalidRoleShape)?;
    let not_found = |reason: &'static str| {
        Ok(Err(ResolutionIssue {
            rule: ResolutionRule::Type6,
            origin: use_record.origin.clone(),
            kind: ResolutionIssueKind::UnknownOwnedVariant {
                spelling: use_record.spelling.clone(),
                reason,
            },
        }))
    };
    let ResolvedTarget::Source {
        declaration: enum_declaration,
        ..
    } = owner.target
    else {
        return not_found(
            "only a source enum owns its variants; a prelude constructor is written unqualified",
        );
    };
    let Some(enum_meta) = metas.get(enum_declaration.index()) else {
        return Err(ResolutionCompilerFailure::InvalidRoleShape);
    };
    if declarations[enum_meta.record_index].role != super::super::DeclarationRole::Enum {
        return not_found("the owner of a variant construction must be an enum");
    }
    let variant = metas.iter().find(|meta| {
        meta.type_owned
            && meta.owner == enum_meta.owner
            && declarations[meta.record_index].spelling == use_record.spelling
    });
    let Some(variant) = variant else {
        return not_found("the owner enum declares no variant with this name");
    };
    Ok(Ok(ResolvedTarget::Source {
        declaration: declarations[variant.record_index].id,
        class: DeclarationClass::EnumVariant,
    }))
}

fn admissible_classes(role: LexicalUseRole, spelling: &str) -> Vec<DeclarationClass> {
    match role {
        LexicalUseRole::Type => vec![DeclarationClass::GenericType, DeclarationClass::NominalType],
        LexicalUseRole::TypeArgument => vec![
            DeclarationClass::GenericType,
            DeclarationClass::NominalType,
            DeclarationClass::Interface,
            DeclarationClass::Binding,
        ],
        LexicalUseRole::FormalGroup => vec![DeclarationClass::Interface, DeclarationClass::Binding],
        LexicalUseRole::GenericBound => {
            vec![DeclarationClass::NumericBound]
        }
        LexicalUseRole::Construct => vec![
            DeclarationClass::StructConstructor,
            DeclarationClass::EnumVariant,
        ],
        LexicalUseRole::VariantOwner => vec![DeclarationClass::NominalType],
        LexicalUseRole::EnsuresVariant => vec![DeclarationClass::EnumVariant],
        LexicalUseRole::EffectRoot | LexicalUseRole::EffectIndex => {
            vec![DeclarationClass::Value]
        }
        LexicalUseRole::BreakLabel => vec![DeclarationClass::Label],
        LexicalUseRole::Const => {
            vec![DeclarationClass::NamedConst, DeclarationClass::ConstGeneric]
        }
        LexicalUseRole::ConstValue => vec![DeclarationClass::NamedConst],
        LexicalUseRole::PlaceBase => {
            // [MSR-6] a `pbase` admits an in-scope const generic beside a
            // named const: a const generic is a monomorphization-time
            // constant and already an [ENT-2] symbolic constant term, so
            // this admission adds a spelling and no fact source.
            vec![
                DeclarationClass::NamedConst,
                DeclarationClass::ConstGeneric,
                DeclarationClass::Value,
            ]
        }
        LexicalUseRole::IdentifierCallee => {
            if operation_id(spelling).is_some() {
                vec![DeclarationClass::OperationFamily]
            } else {
                vec![
                    DeclarationClass::Function,
                    DeclarationClass::FunctionParameter,
                ]
            }
        }
        LexicalUseRole::OperationCallee => vec![DeclarationClass::OperationFamily],
        LexicalUseRole::FunctionBinding => vec![
            DeclarationClass::Function,
            DeclarationClass::FunctionParameter,
        ],
        LexicalUseRole::GenericNumericSuffix => vec![DeclarationClass::GenericType],
        // [MSR-6, INV-1] an affine atom is one bare place whose `pbase` is an
        // IDENT, and an in-scope const generic is a value in exactly that
        // position: it is a constant of [ENT-2] clause (c) rather than a
        // tracked place, so it reaches the affine domain as an immutable atom
        // nothing kills.
        LexicalUseRole::InvariantValue | LexicalUseRole::ProofValue => {
            // A named integer const is already an [ENT-2] constant term, so it
            // denotes the same value in a proof relation that it denotes
            // everywhere else; excluding it here forced the digits to be
            // rewritten inline in every invariant that names the same limit.
            vec![
                DeclarationClass::Value,
                DeclarationClass::ConstGeneric,
                DeclarationClass::NamedConst,
            ]
        }
        LexicalUseRole::InvariantFact => vec![DeclarationClass::Invariant],
    }
}

fn universe_classes(role: LexicalUseRole) -> Vec<DeclarationClass> {
    match role {
        LexicalUseRole::Type | LexicalUseRole::GenericNumericSuffix => {
            vec![DeclarationClass::GenericType, DeclarationClass::NominalType]
        }
        LexicalUseRole::TypeArgument | LexicalUseRole::FormalGroup => vec![
            DeclarationClass::GenericType,
            DeclarationClass::NominalType,
            DeclarationClass::Interface,
            DeclarationClass::Binding,
        ],
        LexicalUseRole::GenericBound => {
            vec![DeclarationClass::NumericBound]
        }
        LexicalUseRole::Construct | LexicalUseRole::EnsuresVariant => {
            vec![
                DeclarationClass::StructConstructor,
                DeclarationClass::EnumVariant,
            ]
        }
        LexicalUseRole::VariantOwner => {
            vec![DeclarationClass::GenericType, DeclarationClass::NominalType]
        }
        LexicalUseRole::EffectRoot | LexicalUseRole::EffectIndex => {
            vec![DeclarationClass::Value]
        }
        LexicalUseRole::BreakLabel => vec![DeclarationClass::Label],
        LexicalUseRole::Const
        | LexicalUseRole::ConstValue
        | LexicalUseRole::PlaceBase
        | LexicalUseRole::FunctionBinding => vec![
            DeclarationClass::Function,
            DeclarationClass::FunctionParameter,
            DeclarationClass::NamedConst,
            DeclarationClass::ConstGeneric,
            DeclarationClass::Value,
        ],
        LexicalUseRole::IdentifierCallee => vec![
            DeclarationClass::Function,
            DeclarationClass::FunctionParameter,
            DeclarationClass::NamedConst,
            DeclarationClass::ConstGeneric,
            DeclarationClass::Value,
            DeclarationClass::OperationFamily,
        ],
        LexicalUseRole::OperationCallee => vec![DeclarationClass::OperationFamily],
        LexicalUseRole::InvariantValue | LexicalUseRole::ProofValue => {
            // A named integer const is already an [ENT-2] constant term, so it
            // denotes the same value in a proof relation that it denotes
            // everywhere else; excluding it here forced the digits to be
            // rewritten inline in every invariant that names the same limit.
            vec![
                DeclarationClass::Value,
                DeclarationClass::ConstGeneric,
                DeclarationClass::NamedConst,
            ]
        }
        LexicalUseRole::InvariantFact => vec![DeclarationClass::Invariant],
    }
}

fn use_rule(role: LexicalUseRole) -> ResolutionRule {
    match role {
        LexicalUseRole::Type | LexicalUseRole::TypeArgument | LexicalUseRole::PlaceBase => {
            ResolutionRule::Type5
        }
        LexicalUseRole::GenericBound | LexicalUseRole::FormalGroup => ResolutionRule::Fn3,
        LexicalUseRole::Construct
        | LexicalUseRole::VariantOwner
        | LexicalUseRole::EnsuresVariant
        | LexicalUseRole::BreakLabel => ResolutionRule::Type6,
        LexicalUseRole::EffectRoot | LexicalUseRole::EffectIndex => ResolutionRule::Eff1,
        LexicalUseRole::Const => ResolutionRule::Const1,
        LexicalUseRole::ConstValue => ResolutionRule::Const2,
        LexicalUseRole::IdentifierCallee | LexicalUseRole::OperationCallee => ResolutionRule::Op1,
        // FN-3 selects the function named by a binding group's member;
        // FN-4 checks its compatibility after resolution.
        LexicalUseRole::FunctionBinding => ResolutionRule::Fn3,
        LexicalUseRole::GenericNumericSuffix => ResolutionRule::Form5,
        LexicalUseRole::InvariantValue => ResolutionRule::Inv1,
        LexicalUseRole::ProofValue => ResolutionRule::Prf1,
        LexicalUseRole::InvariantFact => ResolutionRule::Inv1,
    }
}

/// Resolves every alias's complete `pkg` path [MOD-4].
///
/// A lowercase alias binds a registered module, or a lowercase function or
/// constant of one; an uppercase alias binds a type, group or struct of one,
/// or a variant of a nongeneric enum of one. The target module must be the
/// alias's own module or a direct dependency of it, and a target in another
/// module must be public there; an unused alias receives the same checks.
/// Each resolved alias takes its target's classes, and each refused one
/// returns its issue beside its record index for inventory order to report.
pub(super) fn resolve_alias_targets(
    topology: &crate::syntax::FinalizedTopology,
    scopes: &ScopeBuild,
    declarations: &mut [DeclarationRecord],
    metas: &mut [DeclarationMeta],
    index: &DeclarationIndex,
    modules: &ModuleView<'_>,
    qualifiers: &[Option<Qualifier>],
) -> Result<Vec<(usize, ResolutionIssue)>, ResolutionCompilerFailure> {
    let mut issues = Vec::new();
    for record_index in 0..declarations.len() {
        if declarations[record_index].role != super::super::DeclarationRole::Alias {
            continue;
        }
        let meta = &metas[record_index];
        let qualifier = qualifiers
            .get(meta.role_index)
            .and_then(Option::as_ref)
            .ok_or(ResolutionCompilerFailure::InvalidRoleShape)?;
        let alias_module = meta
            .module
            .ok_or(ResolutionCompilerFailure::InvalidRoleShape)?;
        // [MOD-3] an alias of an interface record sees what the record sees.
        let alias_interface = !meta.implementation_only;
        let origin = declarations[record_index].origin.clone();
        let spelling = declarations[record_index].spelling.clone();
        let refuse = |reason: &'static str| ResolutionIssue {
            rule: ResolutionRule::Mod4,
            origin: origin.clone(),
            kind: ResolutionIssueKind::InvalidAliasTarget {
                spelling: spelling.clone(),
                target: written_path(qualifier, ""),
                reason,
            },
        };
        let target = alias_target(
            topology,
            scopes,
            declarations,
            metas,
            index,
            modules,
            qualifier,
            (alias_module, alias_interface),
            &spelling,
        )?;
        match target {
            Err(AliasRefusal::Target(reason)) => issues.push((record_index, refuse(reason))),
            Err(AliasRefusal::OwnLibrary) => issues.push((
                record_index,
                ResolutionIssue {
                    rule: ResolutionRule::Mod10,
                    origin: origin.clone(),
                    kind: ResolutionIssueKind::LibraryNamesItself {
                        path: written_path(qualifier, ""),
                    },
                },
            )),
            Err(AliasRefusal::Edge(target_module)) => issues.push((
                record_index,
                ResolutionIssue {
                    rule: ResolutionRule::Mod5,
                    origin: origin.clone(),
                    kind: ResolutionIssueKind::MissingModuleEdge {
                        from: modules.name(alias_module),
                        to: modules.name(target_module),
                    },
                },
            )),
            Ok(target) => {
                let entries = match target {
                    AliasTarget::Module(_) => vec![DeclarationClass::Module],
                    AliasTarget::Declaration(target) => metas[target].entries.clone(),
                };
                declarations[record_index].classes.clone_from(&entries);
                let meta = &mut metas[record_index];
                meta.entries = entries;
                meta.alias = Some(target);
            }
        }
    }
    Ok(issues)
}

/// Why an alias path binds nothing [MOD-4, MOD-5].
enum AliasRefusal {
    /// The path names nothing an alias of this case binds [MOD-4].
    Target(&'static str),
    /// The path's registered module is one the alias's module may not name
    /// [MOD-5].
    Edge(crate::ModuleId),
    /// A standard library record writes `std`, where the library names
    /// itself `pkg` [MOD-10].
    OwnLibrary,
}

/// Resolves an alias path in order: a registered module, one the alias's
/// module may name [MOD-5], then a target of the alias's case among that
/// module's declarations, which for another module are its public ones, so
/// that the verdict never depends on another module's implementation
/// records [MOD-4, MOD-8].
#[allow(clippy::too_many_arguments)]
fn alias_target(
    topology: &crate::syntax::FinalizedTopology,
    scopes: &ScopeBuild,
    declarations: &[DeclarationRecord],
    metas: &[DeclarationMeta],
    index: &DeclarationIndex,
    modules: &ModuleView<'_>,
    qualifier: &Qualifier,
    (alias_module, alias_interface): (crate::ModuleId, bool),
    spelling: &str,
) -> Result<Result<AliasTarget, AliasRefusal>, ResolutionCompilerFailure> {
    let lower = spelling
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_lowercase);
    let names: Vec<&str> = qualifier
        .segments
        .iter()
        .map(|segment| segment.spelling.as_str())
        .collect();
    let is_type = |name: &str| name.as_bytes().first().is_some_and(u8::is_ascii_uppercase);
    let first_type = names.iter().position(|name| is_type(name));
    let alias_package = modules.package(alias_module);
    if qualifier.standard && alias_package == crate::Package::Standard {
        return Ok(Err(AliasRefusal::OwnLibrary));
    }
    let package = if qualifier.standard {
        crate::Package::Standard
    } else {
        alias_package
    };
    let module_of = |path: &[&str]| {
        let path: Vec<String> = path.iter().map(|name| (*name).to_owned()).collect();
        modules.find(package, &path)
    };
    let declaration_in = |module: crate::ModuleId, name: &str, wanted: &[DeclarationClass]| {
        let inventory = scopes.module_scope(module)?;
        let found = index
            .with_spelling(name)
            .iter()
            .filter_map(|candidate| metas.get(*candidate))
            .find(|meta| {
                meta.scope == inventory
                    && if module == alias_module {
                        !(alias_interface && meta.implementation_only)
                    } else {
                        meta.public
                    }
                    && !meta.type_owned
                    && !defers_to_definition(meta)
                    && meta.entries.iter().any(|class| wanted.contains(class))
            })?;
        Some(found.record_index)
    };
    let refuse = |reason| Ok(Err(AliasRefusal::Target(reason)));
    match first_type {
        None => {
            if let Some(module) = module_of(&names) {
                if !modules.permits(alias_module, module) {
                    return Ok(Err(AliasRefusal::Edge(module)));
                }
                if !lower {
                    return refuse("a module is bound by a lowercase alias");
                }
                return Ok(Ok(AliasTarget::Module(module)));
            }
            let Some((last, prefix)) = names.split_last() else {
                return refuse("the path names no registered module");
            };
            let Some(module) = module_of(prefix) else {
                return refuse("the path names no registered module");
            };
            if !modules.permits(alias_module, module) {
                return Ok(Err(AliasRefusal::Edge(module)));
            }
            if !lower {
                return refuse("a function or constant is bound by a lowercase alias");
            }
            match declaration_in(
                module,
                last,
                &[DeclarationClass::Function, DeclarationClass::NamedConst],
            ) {
                Some(target) => Ok(Ok(AliasTarget::Declaration(target))),
                None => refuse(
                    "the module declares no function or constant with this name that the alias's module can access",
                ),
            }
        }
        Some(position) => {
            let Some(module) = module_of(&names[..position]) else {
                return refuse("the path names no registered module");
            };
            if !modules.permits(alias_module, module) {
                return Ok(Err(AliasRefusal::Edge(module)));
            }
            if lower {
                return refuse("a type, group or variant is bound by an uppercase alias");
            }
            let rest = &names[position..];
            match rest {
                [name] => match declaration_in(
                    module,
                    name,
                    &[
                        DeclarationClass::NominalType,
                        DeclarationClass::Interface,
                        DeclarationClass::Binding,
                    ],
                ) {
                    Some(target) => Ok(Ok(AliasTarget::Declaration(target))),
                    None => refuse(
                        "the module declares no type or group with this name that the alias's module can access",
                    ),
                },
                [owner, variant] if is_type(variant) => {
                    let Some(owner) =
                        declaration_in(module, owner, &[DeclarationClass::NominalType])
                    else {
                        return refuse(
                            "the module declares no enum with this name that the alias's module can access",
                        );
                    };
                    if declarations[owner].role != super::super::DeclarationRole::Enum {
                        return refuse("only an enum owns variants");
                    }
                    let owner_meta = &metas[owner];
                    let generic = owner_meta.owner.is_some_and(|node| {
                        topology.node_children(node).is_some_and(|children| {
                            children.iter().any(|child| {
                                topology.node(*child).is_some_and(|record| {
                                    record.production == crate::Production::Generics
                                })
                            })
                        })
                    });
                    if generic {
                        return refuse(
                            "a generic enum's constructor keeps its owner and arguments, so no alias abbreviates it",
                        );
                    }
                    let found = metas.iter().find(|meta| {
                        meta.type_owned
                            && meta.owner == owner_meta.owner
                            && declarations[meta.record_index].spelling == *variant
                    });
                    match found {
                        Some(found) => Ok(Ok(AliasTarget::Declaration(found.record_index))),
                        None => refuse("the enum declares no variant with this name"),
                    }
                }
                _ => refuse("an alias target ends at a module, a declaration or one variant"),
            }
        }
    }
}
