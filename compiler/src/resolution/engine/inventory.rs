use crate::Production;
use crate::syntax::{FinalizedExtent, FinalizedTopology};

use super::super::catalog::{PRELUDE_DECLARATIONS, reserved_name};
use super::super::scopes::ScopeBuild;
use super::super::{
    DeclarationClass, DeclarationConflict, DeclarationOrigin, DeclarationRecord, DeclarationRole,
    DeferredUseRole, DependentDeclarationRole, ReservedDeclarationRole, ResolutionCompilerFailure,
    ResolutionIssue, ResolutionIssueKind, ResolutionRule,
};
use super::{
    ClassifiedRole, DeclarationIndex, DeclarationMeta, EventKey, FunctionForm, RawRoleKind,
    SelectorRole, ancestor_with_production, declaration_domain, is_visible,
};

/// [MOD-7] pairs each interface function declaration with the one
/// definition of its spelling in its module, when the module has exactly one
/// of each. The definition takes the declaration's publication; any other
/// combination is left for the collision check to report.
pub(super) fn pair_interface_functions(
    declarations: &mut [DeclarationRecord],
    metas: &mut [DeclarationMeta],
    index: &DeclarationIndex,
) {
    for record_index in 0..metas.len() {
        if metas[record_index].function_form != (FunctionForm::Declaration { definition: None }) {
            continue;
        }
        let scope = metas[record_index].scope;
        let spelling = declarations[record_index].spelling.clone();
        let same: Vec<usize> = index
            .with_spelling(&spelling)
            .iter()
            .copied()
            .filter(|candidate| {
                *candidate != record_index
                    && metas.get(*candidate).is_some_and(|meta| {
                        meta.scope == scope && meta.function_form != FunctionForm::None
                    })
            })
            .collect();
        if let [definition] = same.as_slice()
            && metas[*definition].function_form == FunctionForm::Definition
        {
            let public = metas[record_index].public;
            metas[record_index].function_form = FunctionForm::Declaration {
                definition: Some(*definition),
            };
            metas[*definition].public = public;
            declarations[*definition].public = public;
            // [MOD-3, MOD-7] the definition stands for its interface
            // declaration in lookup, so it is visible where that is.
            metas[*definition].implementation_only = false;
        }
    }
}

/// Whether two declarations are one interface declaration and its paired
/// definition [MOD-7].
fn paired(left: &DeclarationMeta, right: &DeclarationMeta) -> bool {
    let pair = |declaration: &DeclarationMeta, definition: &DeclarationMeta| {
        declaration.function_form
            == FunctionForm::Declaration {
                definition: Some(definition.record_index),
            }
    };
    pair(left, right) || pair(right, left)
}

struct InventoryTables<'a> {
    roles: &'a [ClassifiedRole],
    declarations: &'a [DeclarationRecord],
    metas: &'a [DeclarationMeta],
    index: &'a DeclarationIndex,
    prelude_origins: &'a [super::super::PreludeDeclarationId],
}

#[allow(clippy::too_many_arguments)]
pub(super) fn check_declaration_inventory(
    topology: &FinalizedTopology,
    scopes: &ScopeBuild,
    roles: &[ClassifiedRole],
    declarations: &[DeclarationRecord],
    metas: &[DeclarationMeta],
    index: &DeclarationIndex,
    declaration_by_role: &[Option<usize>],
    prelude_origins: &[super::super::PreludeDeclarationId],
    alias_issues: &[(usize, ResolutionIssue)],
    modules: &[crate::ModuleRecord],
) -> Result<Option<ResolutionIssue>, ResolutionCompilerFailure> {
    check_inventory(
        topology,
        scopes,
        roles,
        declarations,
        metas,
        index,
        declaration_by_role,
        prelude_origins,
        alias_issues,
        modules,
        |_| true,
    )
}

#[allow(clippy::too_many_arguments)]
fn check_inventory(
    topology: &FinalizedTopology,
    scopes: &ScopeBuild,
    roles: &[ClassifiedRole],
    declarations: &[DeclarationRecord],
    metas: &[DeclarationMeta],
    index: &DeclarationIndex,
    declaration_by_role: &[Option<usize>],
    prelude_origins: &[super::super::PreludeDeclarationId],
    alias_issues: &[(usize, ResolutionIssue)],
    modules: &[crate::ModuleRecord],
    include: impl Fn(&ClassifiedRole) -> bool,
) -> Result<Option<ResolutionIssue>, ResolutionCompilerFailure> {
    if declarations.len() != metas.len()
        || metas
            .iter()
            .enumerate()
            .any(|(index, meta)| meta.record_index != index)
    {
        return Err(ResolutionCompilerFailure::InvalidRoleShape);
    }
    let tables = InventoryTables {
        roles,
        declarations,
        metas,
        index,
        prelude_origins,
    };
    for (role_index, role) in roles.iter().enumerate() {
        if !include(role) {
            continue;
        }
        // [FORM-3] reserves the dotless operation families and the five
        // operation-mode words from "No source declaration or FN-9
        // result-datum candidate in this closed list".
        //
        // x1 deletes the prelude-origin exemption that stood here. It existed
        // only for the eight measure and window-part names, which the rule no
        // longer reserves: the prelude's own fence writes `next` as a payload
        // binder of `host_copy_bytes` and as a result binding of
        // `directory_next`, so reading the old eight-name set against every
        // declaration role made the prelude reject itself before any source
        // file was read. No PRE-1 record is spelled like a dotless operation
        // family or a mode word, so the two surviving classes need no skip and
        // the prelude is now read exactly as source is.
        if let Some((reserved_role, checked_spelling)) = reserved_role(topology, role)
            && let Some((class, inventory_ordinal)) = reserved_name(checked_spelling)
        {
            return Ok(Some(ResolutionIssue {
                rule: ResolutionRule::Form3,
                origin: role.origin.clone(),
                kind: ResolutionIssueKind::ReservedName {
                    spelling: checked_spelling.to_owned(),
                    declaration_role: reserved_role,
                    class,
                    inventory_ordinal,
                },
            }));
        }
        let Some(record_index) = declaration_by_role.get(role_index).and_then(|entry| *entry)
        else {
            continue;
        };
        let meta = meta_for_record(metas, record_index)?;
        let declaration = declarations
            .get(record_index)
            .ok_or(ResolutionCompilerFailure::InvalidRoleShape)?;

        if declaration.role == DeclarationRole::MatchBinder
            && let Some(issue) = match_binder_issue(topology, scopes, role, declaration, &tables)?
        {
            return Ok(Some(issue));
        }

        // [MOD-4] an alias whose target was refused reports at the alias.
        if let Some((_, issue)) = alias_issues
            .iter()
            .find(|(alias, _)| *alias == record_index)
        {
            return Ok(Some(issue.clone()));
        }

        // [TYPE-6] a user variant belongs to its enum: it collides only with
        // an earlier variant of that same enum.
        if meta.type_owned {
            let earlier = tables
                .index
                .with_spelling(&declaration.spelling)
                .iter()
                .copied()
                .take_while(|candidate| *candidate < record_index)
                .filter_map(|candidate| tables.metas.get(candidate))
                .find(|candidate| candidate.type_owned && candidate.owner == meta.owner);
            if let Some(earlier) = earlier {
                return Ok(Some(collision(
                    declaration,
                    vec![DeclarationConflict {
                        domain: super::super::DeclarationDomain::Constructor,
                        class: DeclarationClass::EnumVariant,
                        origin: DeclarationOrigin::Source(
                            tables.declarations[earlier.record_index].origin.clone(),
                        ),
                    }],
                    ResolutionRule::Type6,
                    COLLIDES_IN_ONE_ENUM,
                )));
            }
            continue;
        }

        // [MOD-3] a module's lowercase declaration and a child module
        // registered under the same name would occupy one qualified slot.
        if let Some(module) = meta.module
            && scopes.module_scope(module) == Some(meta.scope)
            && declaration
                .spelling
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_lowercase)
            && let Some(parent) = modules.get(module.index())
        {
            let mut child = parent.path().to_vec();
            child.push(declaration.spelling.clone());
            if modules
                .iter()
                .any(|module| module.is_at(parent.package(), &child))
            {
                return Ok(Some(ResolutionIssue {
                    rule: ResolutionRule::Mod3,
                    origin: declaration.origin.clone(),
                    kind: ResolutionIssueKind::DeclarationCollision {
                        spelling: declaration.spelling.clone(),
                        conflicts: Vec::new(),
                        mechanical_fix: COLLIDES_WITH_MODULE,
                    },
                }));
            }
        }

        if let Some(issue) = collision_issue(scopes, declaration, meta, &tables)? {
            return Ok(Some(issue));
        }
    }
    Ok(None)
}

fn reserved_role<'role>(
    topology: &FinalizedTopology,
    role: &'role ClassifiedRole,
) -> Option<(ReservedDeclarationRole, &'role str)> {
    let mapped = match role.kind {
        RawRoleKind::Declaration(DeclarationRole::Function) => ReservedDeclarationRole::Function,
        RawRoleKind::Declaration(DeclarationRole::NamedConst) => {
            ReservedDeclarationRole::NamedConst
        }
        RawRoleKind::Declaration(DeclarationRole::Parameter) => ReservedDeclarationRole::Parameter,
        // A contract-block `define` binder is classified as an ordinary
        // `let` declaration for lookup, but FORM-3 names its carrier role
        // separately, so the reservation report tells the two apart.
        RawRoleKind::Declaration(DeclarationRole::Let) => {
            if topology.node(role.owner).map(|record| record.production)
                == Some(Production::ContractDefine)
            {
                ReservedDeclarationRole::ContractDefinition
            } else {
                ReservedDeclarationRole::Let
            }
        }
        RawRoleKind::Declaration(DeclarationRole::CountedBinder) => {
            ReservedDeclarationRole::ForBinder
        }
        RawRoleKind::Declaration(DeclarationRole::MatchBinder) => {
            ReservedDeclarationRole::MatchBinder
        }
        RawRoleKind::Selector(SelectorRole::PlainCandidate) => {
            ReservedDeclarationRole::PlainResultSelector
        }
        RawRoleKind::Selector(SelectorRole::VariantCandidate) => {
            ReservedDeclarationRole::VariantResultSelector
        }
        RawRoleKind::DependentDeclaration(DependentDeclarationRole::Field) => {
            ReservedDeclarationRole::Field
        }
        RawRoleKind::DependentDeclaration(DependentDeclarationRole::VariantField) => {
            ReservedDeclarationRole::VariantField
        }
        _ => return None,
    };
    Some((mapped, role.spelling.as_str()))
}

fn match_binder_issue(
    topology: &FinalizedTopology,
    scopes: &ScopeBuild,
    role: &ClassifiedRole,
    declaration: &DeclarationRecord,
    tables: &InventoryTables<'_>,
) -> Result<Option<ResolutionIssue>, ResolutionCompilerFailure> {
    let arm = ancestor_with_production(topology, role.owner, Production::Arm)
        .ok_or(ResolutionCompilerFailure::InvalidRoleShape)?;
    let paired_field = tables
        .roles
        .iter()
        .find(|candidate| {
            candidate.owner == role.owner
                && matches!(
                    candidate.kind,
                    RawRoleKind::DeferredUse(DeferredUseRole::MatchField)
                )
        })
        .ok_or(ResolutionCompilerFailure::InvalidRoleShape)?;
    let earlier_binder = tables
        .index
        .with_spelling(&declaration.spelling)
        .iter()
        .copied()
        .take_while(|candidate| *candidate < declaration.id.index())
        .filter_map(|candidate| tables.metas.get(candidate))
        .find_map(|candidate| {
            let earlier = tables.declarations.get(candidate.record_index)?;
            (earlier.role == DeclarationRole::MatchBinder
                && earlier.spelling == declaration.spelling
                && ancestor_with_production(
                    topology,
                    tables.roles[candidate.role_index].owner,
                    Production::Arm,
                ) == Some(arm))
            .then(|| earlier.origin.clone())
        });
    let arm_scope = scopes.declaration_scope(arm)?;
    let entry_scope = scopes
        .records
        .get(arm_scope.index())
        .and_then(super::super::ScopeRecord::parent)
        .ok_or(ResolutionCompilerFailure::InvalidScopeTree)?;
    let arm_record = topology
        .node(arm)
        .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
    let FinalizedExtent::Source { source, start, .. } = arm_record.extent else {
        return Err(ResolutionCompilerFailure::InvalidCanonicalTree);
    };
    let mut arm_entry_conflicts = Vec::new();
    for candidate in tables
        .index
        .with_spelling(&declaration.spelling)
        .iter()
        .filter_map(|candidate| tables.metas.get(*candidate))
    {
        let record = &tables.declarations[candidate.record_index];
        if candidate.entries.iter().any(|class| {
            matches!(
                class,
                DeclarationClass::Function
                    | DeclarationClass::NamedConst
                    | DeclarationClass::ConstGeneric
                    | DeclarationClass::Value
            )
        }) && is_visible(
            scopes,
            candidate,
            entry_scope,
            source.ordinal(),
            start.value(),
        ) {
            arm_entry_conflicts.push(record.origin.clone());
        }
    }
    arm_entry_conflicts.sort_by_key(EventKey::from_origin);
    if declaration.spelling == paired_field.spelling
        || earlier_binder.is_some()
        || !arm_entry_conflicts.is_empty()
    {
        return Ok(Some(ResolutionIssue {
            rule: ResolutionRule::Gram10,
            origin: declaration.origin.clone(),
            kind: ResolutionIssueKind::MatchBinderFreshness {
                spelling: declaration.spelling.clone(),
                paired_field: paired_field.spelling.clone(),
                earlier_binder,
                arm_entry_conflicts,
            },
        }));
    }
    Ok(None)
}

fn collision_issue(
    scopes: &ScopeBuild,
    declaration: &DeclarationRecord,
    meta: &DeclarationMeta,
    tables: &InventoryTables<'_>,
) -> Result<Option<ResolutionIssue>, ResolutionCompilerFailure> {
    let mut prelude_conflicts = Vec::new();
    for class in &meta.entries {
        let domain =
            declaration_domain(*class).ok_or(ResolutionCompilerFailure::InvalidRoleShape)?;
        for prelude in PRELUDE_DECLARATIONS {
            if prelude.spelling == declaration.spelling
                && prelude.class.and_then(declaration_domain) == Some(domain)
            {
                prelude_conflicts.push(DeclarationConflict {
                    domain,
                    class: prelude
                        .class
                        .ok_or(ResolutionCompilerFailure::InvalidRoleShape)?,
                    origin: DeclarationOrigin::Prelude(
                        tables.prelude_origins[usize::from(prelude.id.ordinal())],
                    ),
                });
            }
        }
    }
    sort_conflicts(&mut prelude_conflicts, tables.declarations);
    if !prelude_conflicts.is_empty() {
        return Ok(Some(collision(
            declaration,
            prelude_conflicts,
            declaration_collision_rule(declaration),
            COLLIDES_WITH_PRELUDE,
        )));
    }

    // x1 [TYPE-2, PRE-1]: the four storage nominals no longer stand beside the
    // declaration tables as their own rank. `Array`, `Slots`, `Ring` and `Box`
    // are the prelude's opaque structs, so a source declaration of one of
    // those spellings meets it through the ordinary prelude rank above, in the
    // same words every other PRE-1 collision uses. Nothing is narrowed: the
    // spelling still collides in both domains and neither declaration
    // resolves.

    let mut same_scope = Vec::new();
    for candidate in tables
        .index
        .with_spelling(&declaration.spelling)
        .iter()
        .copied()
        .take_while(|candidate| *candidate < meta.record_index)
        .filter_map(|candidate| tables.metas.get(candidate))
        .filter(|candidate| {
            candidate.scope == meta.scope && !candidate.type_owned && !paired(candidate, meta)
        })
    {
        collect_domain_conflicts(
            declaration,
            meta,
            &tables.declarations[candidate.record_index],
            candidate,
            &mut same_scope,
        );
    }
    sort_conflicts(&mut same_scope, tables.declarations);
    if !same_scope.is_empty() {
        return Ok(Some(collision(
            declaration,
            same_scope,
            declaration_collision_rule(declaration),
            COLLIDES_IN_ONE_SCOPE,
        )));
    }

    let mut shadows = Vec::new();
    let mut shadows_prelude = false;
    for candidate in tables
        .index
        .with_spelling(&declaration.spelling)
        .iter()
        .filter_map(|candidate| tables.metas.get(*candidate))
    {
        if candidate.record_index == meta.record_index
            || candidate.scope == meta.scope
            || candidate.type_owned
            || !scopes.is_ancestor(candidate.scope, meta.scope)
            || !is_visible(
                scopes,
                candidate,
                meta.scope,
                declaration.origin.coordinate.source().ordinal(),
                declaration.origin.coordinate.start().value(),
            )
        {
            continue;
        }
        let before = shadows.len();
        collect_domain_conflicts(
            declaration,
            meta,
            &tables.declarations[candidate.record_index],
            candidate,
            &mut shadows,
        );
        shadows_prelude |= shadows.len() != before && scopes.is_prelude_scope(candidate.scope);
    }
    sort_conflicts(&mut shadows, tables.declarations);
    Ok((!shadows.is_empty()).then(|| {
        collision(
            declaration,
            shadows,
            declaration_collision_rule(declaration),
            if shadows_prelude {
                COLLIDES_WITH_PRELUDE
            } else {
                COLLIDES_WITH_LIVE_OUTER
            },
        )
    }))
}

const fn declaration_collision_rule(declaration: &DeclarationRecord) -> ResolutionRule {
    if matches!(declaration.role(), DeclarationRole::Invariant) {
        ResolutionRule::Inv1
    } else {
        ResolutionRule::Type6
    }
}

fn meta_for_record(
    metas: &[DeclarationMeta],
    record_index: usize,
) -> Result<&DeclarationMeta, ResolutionCompilerFailure> {
    metas
        .get(record_index)
        .filter(|meta| meta.record_index == record_index)
        .ok_or(ResolutionCompilerFailure::InvalidRoleShape)
}

fn collect_domain_conflicts(
    declaration: &DeclarationRecord,
    meta: &DeclarationMeta,
    candidate: &DeclarationRecord,
    candidate_meta: &DeclarationMeta,
    conflicts: &mut Vec<DeclarationConflict>,
) {
    if candidate.spelling != declaration.spelling {
        return;
    }
    for class in &meta.entries {
        let Some(domain) = declaration_domain(*class) else {
            continue;
        };
        for candidate_class in &candidate_meta.entries {
            if declaration_domain(*candidate_class) == Some(domain) {
                conflicts.push(DeclarationConflict {
                    domain,
                    class: *candidate_class,
                    origin: candidate.diagnostic_origin(*candidate_class),
                });
            }
        }
    }
}

/// The four colliding situations [TYPE-6] selects between, each with the
/// repair it admits.
///
/// The payload locates both declarations and stops there, which leaves the
/// surprise of the fourth one unstated: a binding whose value has been
/// consumed is dead as a value while its declaration is still live, so an
/// inner declaration of the same spelling still collides with it. The
/// blind-writer trial of 2026-08-28 met that one twice and repaired it by
/// guessing.
const COLLIDES_WITH_PRELUDE: &str = "a source declaration never displaces, overrides, or shadows a PRE-1 prelude declaration of the same spelling and domain, and neither declaration resolves after the collision; rename this declaration";
const COLLIDES_IN_ONE_SCOPE: &str = "one scope declares each spelling once in a domain, so this is a redeclaration and not a shadow; rename this declaration, or delete the earlier one when nothing reads it";
const COLLIDES_IN_ONE_ENUM: &str = "one enum declares each variant name once; rename this variant";
const COLLIDES_WITH_MODULE: &str = "a registered child module already occupies this qualified name, so a lowercase declaration of its parent module cannot take it; rename the declaration or the module directory";
const COLLIDES_WITH_LIVE_OUTER: &str = "a declaration's scope ends with the block that declares it, and not where its value is consumed: a binding whose value was moved is dead as a value while its declaration stays live, so an inner declaration of the same spelling still collides with it. Rename the inner declaration, or close the block that declares the outer one before this point";

fn collision(
    declaration: &DeclarationRecord,
    conflicts: Vec<DeclarationConflict>,
    rule: ResolutionRule,
    mechanical_fix: &'static str,
) -> ResolutionIssue {
    ResolutionIssue {
        rule,
        origin: declaration.origin.clone(),
        kind: ResolutionIssueKind::DeclarationCollision {
            spelling: declaration.spelling.clone(),
            conflicts,
            mechanical_fix,
        },
    }
}

fn sort_conflicts(conflicts: &mut [DeclarationConflict], declarations: &[DeclarationRecord]) {
    conflicts.sort_by(|left, right| {
        let left_domain = left.domain.ordinal();
        let right_domain = right.domain.ordinal();
        left_domain.cmp(&right_domain).then_with(|| {
            conflict_key(&left.origin, declarations).cmp(&conflict_key(&right.origin, declarations))
        })
    });
}

pub(super) fn conflict_key(
    origin: &DeclarationOrigin,
    declarations: &[DeclarationRecord],
) -> (u8, EventKey) {
    // [DIAG-1] orders conflicts within one domain by PRE-1 declaration
    // ordinal first, then source declaration-event key.
    match origin {
        DeclarationOrigin::Prelude(id) => (
            0,
            EventKey {
                source: 0,
                start: u64::from(id.ordinal()),
                end: 0,
                path: Vec::new(),
                role: 0,
                subtoken: 0,
            },
        ),
        DeclarationOrigin::Source(origin) => {
            let _ = declarations;
            (3, EventKey::from_origin(origin))
        }
    }
}
