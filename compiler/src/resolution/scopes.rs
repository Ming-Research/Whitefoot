use crate::syntax::{FinalizedTopology, NodeId};
use crate::{NodePath, Production, SourceBundle};

use super::{ResolutionCompilerFailure, ScopeId, ScopeKind, ScopeRecord};

pub(crate) struct ScopeBuild {
    pub(crate) records: Vec<ScopeRecord>,
    node_scopes: Vec<Option<ScopeId>>,
    declaration_scopes: Vec<Option<ScopeId>>,
    paths: Vec<Option<NodePath>>,
    /// One declaration-inventory scope per registered module [MOD-3].
    module_scopes: Vec<ScopeId>,
    /// Each writer source's file scope, holding its alias header [MOD-4];
    /// `None` for a PRE-1 record.
    file_scopes: Vec<Option<ScopeId>>,
    prelude_present: bool,
}

impl ScopeBuild {
    pub(crate) fn build(
        topology: &FinalizedTopology,
        sources: &SourceBundle,
    ) -> Result<Self, ResolutionCompilerFailure> {
        let mut build = Self {
            records: Vec::new(),
            node_scopes: vec![None; topology.nodes.len()],
            declaration_scopes: vec![None; topology.nodes.len()],
            paths: vec![None; topology.nodes.len()],
            module_scopes: Vec::new(),
            file_scopes: Vec::new(),
            prelude_present: sources.includes_prelude(),
        };
        let root_path = NodePath {
            components: Vec::new(),
        };
        // PRE-1 is the fixed outer environment that writer declarations
        // extend. Its signature-local names cannot see later writer names.
        let supplied = build.push_scope(None, ScopeKind::CompilationUnit, root_path.clone())?;
        // [MOD-3] each registered module owns one declaration inventory over
        // the PRE-1 environment; a source bundle has its one synthetic root
        // module [MOD-9]. [MOD-4] each writer source opens a file scope over
        // its module for its alias header, so an alias is visible in that
        // source alone while module declarations are visible in all of them.
        for _ in sources.modules() {
            let module = build.push_scope(Some(supplied), ScopeKind::Module, root_path.clone())?;
            build.module_scopes.push(module);
        }
        for (_, file) in sources.iter() {
            if file.prelude().is_some() {
                build.file_scopes.push(None);
                continue;
            }
            let module = *build
                .module_scopes
                .get(file.module().index())
                .ok_or(ResolutionCompilerFailure::InvalidScopeTree)?;
            let scope = build.push_scope(Some(module), ScopeKind::File, root_path.clone())?;
            build.file_scopes.push(Some(scope));
        }
        let mut tasks = vec![(topology.root, supplied, root_path)];
        while let Some((node_id, current_scope, path)) = tasks.pop() {
            if build
                .node_scopes
                .get(node_id.index())
                .and_then(|scope| *scope)
                .is_some()
            {
                return Err(ResolutionCompilerFailure::InvalidScopeTree);
            }
            let node = topology
                .node(node_id)
                .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
            build.node_scopes[node_id.index()] = Some(current_scope);
            build.paths[node_id.index()] = Some(path.clone());
            let children = topology
                .node_children(node_id)
                .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;

            let mut child_scopes = vec![current_scope; children.len()];
            match node.production {
                Production::Program => {
                    for (index, child) in children.iter().enumerate() {
                        let record = topology
                            .node(*child)
                            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
                        let crate::FinalizedExtent::Source { source, .. } = record.extent else {
                            return Err(ResolutionCompilerFailure::InvalidCanonicalTree);
                        };
                        child_scopes[index] = match build.file_scopes.get(source.ordinal() as usize)
                        {
                            Some(Some(file)) => *file,
                            Some(None) => supplied,
                            None => return Err(ResolutionCompilerFailure::InvalidScopeTree),
                        };
                    }
                }
                Production::StructDecl
                | Production::EnumDecl
                | Production::InterfaceDecl
                | Production::BindingDecl => {
                    // [TYPE-6] a nominal's generic parameters are its own: a
                    // generic TYPEID "may not redeclare another parameter in
                    // the same generic list or shadow a live nominal type or
                    // enclosing generic", so two nominals may each write `T`.
                    // An interface group always opens the scope because FN-3 gives
                    // it member signatures even when it writes no `generics`.
                    if node.production == Production::InterfaceDecl
                        || children.iter().any(|child| {
                            topology
                                .node(*child)
                                .is_some_and(|record| record.production == Production::Generics)
                        })
                    {
                        let generic = build.push_scope(
                            Some(current_scope),
                            ScopeKind::DeclarationGenerics,
                            path.clone(),
                        )?;
                        child_scopes.fill(generic);
                    }
                }
                Production::FnDecl => {
                    let generic = if children.iter().any(|child| {
                        topology
                            .node(*child)
                            .is_some_and(|record| record.production == Production::Generics)
                    }) {
                        build.push_scope(
                            Some(current_scope),
                            ScopeKind::DeclarationGenerics,
                            path.clone(),
                        )?
                    } else {
                        current_scope
                    };
                    let signature = build.push_scope(
                        Some(generic),
                        ScopeKind::FunctionSignature,
                        path.clone(),
                    )?;
                    let body =
                        build.push_scope(Some(signature), ScopeKind::FunctionBody, path.clone())?;
                    for (index, child) in children.iter().enumerate() {
                        let production = topology
                            .node(*child)
                            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?
                            .production;
                        child_scopes[index] = match production {
                            Production::Generics => generic,
                            Production::ContractBlock => build.push_scope(
                                Some(signature),
                                ScopeKind::ContractBlock,
                                path.clone(),
                            )?,
                            Production::Doc | Production::Stmt => body,
                            _ => signature,
                        };
                    }
                }
                Production::FnSig => {
                    let signature = build.push_scope(
                        Some(current_scope),
                        ScopeKind::ContractSignature,
                        path.clone(),
                    )?;
                    child_scopes.fill(signature);
                    for (index, child) in children.iter().enumerate() {
                        if topology
                            .node(*child)
                            .is_some_and(|record| record.production == Production::ContractBlock)
                        {
                            child_scopes[index] = build.push_scope(
                                Some(signature),
                                ScopeKind::ContractBlock,
                                path.clone(),
                            )?;
                        }
                    }
                }
                Production::LoopStmt => {
                    let label = build.push_scope(
                        Some(current_scope),
                        ScopeKind::LoopLabel,
                        path.clone(),
                    )?;
                    let body =
                        build.push_scope(Some(label), ScopeKind::NestedBody, path.clone())?;
                    build.declaration_scopes[node_id.index()] = Some(label);
                    assign_nested_body_scopes(topology, children, &mut child_scopes, label, body)?;
                }
                Production::ForStmt => {
                    let range = build.push_scope(
                        Some(current_scope),
                        ScopeKind::CountedRange,
                        path.clone(),
                    )?;
                    let body =
                        build.push_scope(Some(range), ScopeKind::NestedBody, path.clone())?;
                    // The optional direct LABEL and mandatory IDENT declaration
                    // map to `range`, while both endpoint atoms deliberately
                    // stay in the enclosing scope. Only statements enter the
                    // body scope, making either declaration body-only without a
                    // second grammar production or a multi-scope owner table.
                    build.declaration_scopes[node_id.index()] = Some(range);
                    assign_counted_range_scopes(
                        topology,
                        children,
                        &mut child_scopes,
                        range,
                        body,
                    )?;
                }
                Production::Arm => {
                    let arm =
                        build.push_scope(Some(current_scope), ScopeKind::Arm, path.clone())?;
                    let body = build.push_scope(Some(arm), ScopeKind::NestedBody, path.clone())?;
                    build.declaration_scopes[node_id.index()] = Some(arm);
                    assign_nested_body_scopes(topology, children, &mut child_scopes, arm, body)?;
                }
                // A conditional's two blocks are two lexical blocks, and they
                // are the one construct whose blocks are not nested
                // productions: [GRAM-4] hangs both `stmt*` sequences off the
                // same `if_stmt` or `value_if` node, so their statements can
                // only be told apart by the brace pair each falls inside.
                // Without this, a `let` in either block would declare into the
                // enclosing block and collide with a sibling branch's binder or
                // with a later binder of the same spelling in the enclosing
                // block, both of which [TYPE-6] admits as disjoint scopes.
                Production::IfStmt | Production::ValueIf => {
                    let [Some(then_range), else_range] = node.body_ranges() else {
                        return Err(ResolutionCompilerFailure::InvalidCanonicalTree);
                    };
                    let then_body = build.push_scope(
                        Some(current_scope),
                        ScopeKind::NestedBody,
                        path.clone(),
                    )?;
                    // Absent for the else-free `if` and for an `else if` chain,
                    // whose alternative is the nested conditional node rather
                    // than a block this node owns.
                    let else_body = match else_range {
                        Some(_) => Some(build.push_scope(
                            Some(current_scope),
                            ScopeKind::NestedBody,
                            path.clone(),
                        )?),
                        None => None,
                    };
                    for (index, child) in children.iter().enumerate() {
                        let child_record = topology
                            .node(*child)
                            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
                        // The condition `expr` and a chained conditional both
                        // read the enclosing scope, so only statements move.
                        if child_record.production != Production::Stmt {
                            continue;
                        }
                        child_scopes[index] = if within(
                            child_record.first_terminal,
                            child_record.last_terminal(),
                            then_range,
                        ) {
                            then_body
                        } else {
                            else_body.ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?
                        };
                    }
                }
                _ => {}
            }

            for (index, child) in children.iter().enumerate().rev() {
                let child_record = topology
                    .node(*child)
                    .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?;
                let mut child_path = path.clone();
                child_path.components.push(child_record.child_ordinal);
                tasks.push((*child, child_scopes[index], child_path));
            }
        }
        if build.node_scopes.iter().any(Option::is_none) || build.paths.iter().any(Option::is_none)
        {
            return Err(ResolutionCompilerFailure::InvalidScopeTree);
        }
        Ok(build)
    }

    fn push_scope(
        &mut self,
        parent: Option<ScopeId>,
        kind: ScopeKind,
        owner: NodePath,
    ) -> Result<ScopeId, ResolutionCompilerFailure> {
        let id = ScopeId::from_index(self.records.len())
            .ok_or(ResolutionCompilerFailure::CounterOverflow)?;
        self.records.push(ScopeRecord {
            id,
            parent,
            kind,
            owner,
        });
        Ok(id)
    }

    pub(crate) fn node_scope(&self, node: NodeId) -> Result<ScopeId, ResolutionCompilerFailure> {
        self.node_scopes
            .get(node.index())
            .and_then(|scope| *scope)
            .ok_or(ResolutionCompilerFailure::InvalidScopeTree)
    }

    /// Whether a scope holds top-level declarations: the PRE-1 environment,
    /// a module inventory, or a file's alias header.
    pub(crate) fn is_unit_scope(&self, scope: ScopeId) -> bool {
        self.records.get(scope.index()).is_some_and(|record| {
            matches!(
                record.kind,
                ScopeKind::CompilationUnit | ScopeKind::Module | ScopeKind::File
            )
        })
    }

    /// The ordinary outer environment supplied by PRE-1, when the bundle
    /// carries the prelude records.
    pub(crate) fn is_prelude_scope(&self, scope: ScopeId) -> bool {
        self.prelude_present
            && self.records.get(scope.index()).is_some_and(|record| {
                record.kind == ScopeKind::CompilationUnit && record.parent().is_none()
            })
    }

    /// The declaration-inventory scope of one module [MOD-3].
    pub(crate) fn module_scope(&self, module: crate::ModuleId) -> Option<ScopeId> {
        self.module_scopes.get(module.index()).copied()
    }

    pub(crate) fn declaration_scope(
        &self,
        node: NodeId,
    ) -> Result<ScopeId, ResolutionCompilerFailure> {
        self.declaration_scopes
            .get(node.index())
            .and_then(|scope| *scope)
            .ok_or(ResolutionCompilerFailure::InvalidScopeTree)
    }

    /// Each node's index by its path, the inverse of [`Self::path`].
    pub(crate) fn nodes_by_path(&self) -> std::collections::HashMap<&NodePath, usize> {
        self.paths
            .iter()
            .enumerate()
            .filter_map(|(node, path)| path.as_ref().map(|path| (path, node)))
            .collect()
    }

    pub(crate) fn path(&self, node: NodeId) -> Result<&NodePath, ResolutionCompilerFailure> {
        self.paths
            .get(node.index())
            .and_then(Option::as_ref)
            .ok_or(ResolutionCompilerFailure::InvalidScopeTree)
    }

    pub(crate) fn is_ancestor(&self, ancestor: ScopeId, mut scope: ScopeId) -> bool {
        loop {
            if scope == ancestor {
                return true;
            }
            let Some(parent) = self
                .records
                .get(scope.index())
                .and_then(ScopeRecord::parent)
            else {
                return false;
            };
            scope = parent;
        }
    }
}

/// Whether a node's complete terminal run lies strictly inside a brace pair.
fn within(first_terminal: u64, last_terminal: Option<u64>, (open, close): (u64, u64)) -> bool {
    first_terminal > open && last_terminal.is_some_and(|last| last < close)
}

fn assign_nested_body_scopes(
    topology: &FinalizedTopology,
    children: &[NodeId],
    child_scopes: &mut [ScopeId],
    introduced: ScopeId,
    body: ScopeId,
) -> Result<(), ResolutionCompilerFailure> {
    for (index, child) in children.iter().enumerate() {
        let production = topology
            .node(*child)
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?
            .production;
        child_scopes[index] = match production {
            Production::Stmt => body,
            Production::HeaderInvariant => introduced,
            _ => introduced,
        };
    }
    Ok(())
}

fn assign_counted_range_scopes(
    topology: &FinalizedTopology,
    children: &[NodeId],
    child_scopes: &mut [ScopeId],
    range: ScopeId,
    body: ScopeId,
) -> Result<(), ResolutionCompilerFailure> {
    let mut binding_count = 0_u8;
    for (index, child) in children.iter().enumerate() {
        let production = topology
            .node(*child)
            .ok_or(ResolutionCompilerFailure::InvalidCanonicalTree)?
            .production;
        child_scopes[index] = match production {
            Production::ForBinding => {
                binding_count = binding_count
                    .checked_add(1)
                    .ok_or(ResolutionCompilerFailure::CounterOverflow)?;
                range
            }
            Production::HeaderInvariant => range,
            Production::Stmt => body,
            _ => return Err(ResolutionCompilerFailure::InvalidCanonicalTree),
        };
    }
    if binding_count != 1 {
        return Err(ResolutionCompilerFailure::InvalidCanonicalTree);
    }
    Ok(())
}
