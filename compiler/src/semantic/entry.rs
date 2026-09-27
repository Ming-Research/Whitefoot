//! A module program entry's composition judgment [MOD-8, MOD-9, STOR-8].
//!
//! Semantic checking accepts each module on its own; an entry composes only
//! when every interface declaration of its program has a definition, its name
//! selects an ordinary function of its module, public for a named entry, and a
//! `no_heap` entry's execution closure introduces no heap requirement. The
//! driver locates and renders a rejection; the judgment is made here.

use std::collections::{HashMap, HashSet};

use super::{
    CheckedFunction, CheckedNominalKind, CheckedProgram, CheckedProgramData, CheckedType,
    FunctionId, FunctionMentions, NominalId, entailment,
};

/// What one entry selects and requires [PROG-3, MOD-9, STOR-8].
#[derive(Clone, Copy, Debug)]
pub(crate) struct EntryRequest<'a> {
    pub(crate) module: crate::ModuleId,
    pub(crate) name: &'a str,
    /// A named entry, which must select a public function [MOD-9].
    pub(crate) public: bool,
    /// The entry states the no-heap requirement [STOR-8].
    pub(crate) no_heap: bool,
}

/// Why a checked module program does not compose one entry.
#[derive(Debug)]
pub(crate) enum EntryRejection<'resolved> {
    /// An interface declares this function and no implementation record of
    /// its module defines it yet [MOD-8].
    PendingDeclaration(&'resolved crate::DeclarationRecord),
    /// The entry names no ordinary nongeneric function of its module [MOD-9].
    FunctionMissing,
    /// A named entry selects a function private to its module [MOD-9].
    FunctionPrivate,
    /// The entry states `no_heap`, and its execution closure requires the
    /// heap along this call path from the entry; the last function, declared
    /// by `introducer`, introduces the requirement [STOR-8].
    HeapInClosure {
        path: Vec<FunctionId>,
        introducer: Option<&'resolved crate::DeclarationRecord>,
    },
}

impl EntryRejection<'_> {
    /// The rule the rejection applies.
    pub(crate) const fn rule(&self) -> &'static str {
        match self {
            Self::PendingDeclaration(_) => "MOD-8",
            Self::FunctionMissing | Self::FunctionPrivate => "MOD-9",
            Self::HeapInClosure { .. } => "STOR-8",
        }
    }
}

impl CheckedProgram {
    /// The ordinary function a selection names, found by its declaration's
    /// module: a PRE-1 function's checked record carries the first registered
    /// module, but its declaration belongs to no module's inventory [MOD-9,
    /// PROG-3].
    pub(crate) fn selected_function(
        &self,
        resolved: &crate::ResolvedSyntaxUnit,
        module: crate::ModuleId,
        name: &str,
    ) -> Option<&CheckedFunction> {
        self.data.functions.iter().find(|function| {
            !function.formal_hypothesis
                && function.name == name
                && resolved
                    .declaration(function.declaration)
                    .and_then(crate::DeclarationRecord::module)
                    == Some(module)
        })
    }

    /// [MOD-8, MOD-9, STOR-8] a module program's entry composes when no
    /// interface declaration is pending, it selects one ordinary function of
    /// its module, public for a named entry, and a no-heap entry's execution
    /// closure introduces no heap requirement. `resolved` is the unit this
    /// program was checked over.
    pub(crate) fn admit_entry<'resolved>(
        &self,
        resolved: &'resolved crate::ResolvedSyntaxUnit,
        request: EntryRequest<'_>,
    ) -> Result<(), EntryRejection<'resolved>> {
        // [MOD-8] composition needs every declared function's definition; a
        // pending interface declaration blocks it at the declaration, and the
        // build supplies the definition of every host module's function [PRE-2].
        let bundle = resolved.syntax().classified_bundle().source_bundle();
        if let Some(pending) = resolved
            .interface_functions()
            .iter()
            .filter(|function| function.definition().is_none())
            .filter_map(|function| resolved.declaration(function.declaration()))
            .find(|declaration| {
                !declaration
                    .module()
                    .and_then(|module| bundle.module(module))
                    .is_some_and(|module| {
                        module.package() == crate::Package::Standard
                            && crate::library::is_host_module(module.path())
                    })
            })
        {
            return Err(EntryRejection::PendingDeclaration(pending));
        }
        let Some(function) = self.selected_function(resolved, request.module, request.name) else {
            return Err(EntryRejection::FunctionMissing);
        };
        let declaration = resolved.declaration(function.declaration);
        if request.public && !declaration.is_some_and(crate::DeclarationRecord::is_public) {
            return Err(EntryRejection::FunctionPrivate);
        }
        if request.no_heap
            && let Some(path) = self.heap_introducer(function.id)
        {
            let introducer = path
                .last()
                .and_then(|id| self.data.functions.get(id.0 as usize))
                .and_then(|function| resolved.declaration(function.declaration));
            return Err(EntryRejection::HeapInClosure { path, introducer });
        }
        Ok(())
    }

    /// [STOR-8, MOD-9] the functions one run of `function` can call next:
    /// every callee its checked concrete body names, in every branch. An
    /// instance's body names the function-kind actuals it calls; erased proof
    /// annotations call nothing.
    fn closure_successors(&self, function: FunctionId) -> Vec<FunctionId> {
        let mut calls = Vec::new();
        if let Some(body) = self
            .data
            .functions
            .get(function.0 as usize)
            .and_then(|checked| checked.body.as_deref())
        {
            entailment::collect_statement_calls(function, body, &mut calls);
        }
        calls.into_iter().map(|call| call.callee).collect()
    }

    /// [MOD-9, STOR-8] an entry's execution closure, in breadth-first order
    /// from the entry: every function its run can reach through calls.
    /// Definitions outside it never run for that entry.
    fn execution_closure(&self, entry: FunctionId) -> Vec<FunctionId> {
        let mut order = vec![entry];
        let mut seen = HashSet::from([entry]);
        let mut cursor = 0;
        while let Some(function) = order.get(cursor).copied() {
            cursor += 1;
            for successor in self.closure_successors(function) {
                if seen.insert(successor) {
                    order.push(successor);
                }
            }
        }
        order
    }

    /// [STOR-8, MOD-9] the call path from an entry to the function of its
    /// execution closure that introduces a heap requirement, when one does.
    ///
    /// A function of the closure uses the heap on its own when it calls an
    /// allocating prelude row or [`holds_heap_storage`] finds heap storage in
    /// its own concrete layout; it requires the heap when it uses it on its
    /// own or reaches a function that requires it. The introducing component
    /// is the first requiring component of the closure's call graph, in a
    /// breadth-first walk from the entry, that reaches no other requiring
    /// component, and the path ends at its first member in that walk that
    /// uses the heap on its own. Uncalled definitions stay outside the
    /// closure and impose nothing on the entry.
    fn heap_introducer(&self, entry: FunctionId) -> Option<Vec<FunctionId>> {
        let functions = &self.data.functions;
        let closure = self.execution_closure(entry);
        let position = closure
            .iter()
            .enumerate()
            .map(|(index, function)| (*function, index))
            .collect::<HashMap<_, _>>();
        let successors = closure
            .iter()
            .map(|function| {
                self.closure_successors(*function)
                    .into_iter()
                    .filter_map(|successor| position.get(&successor).copied())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let allocating_leaf = |index: usize| {
            functions
                .get(closure[index].0 as usize)
                .is_some_and(|callee| callee.body.is_none() && callee.allocates)
        };
        let own = closure
            .iter()
            .zip(&successors)
            .map(|(function, successors)| {
                functions.get(function.0 as usize).is_some_and(|checked| {
                    checked.body.is_some()
                        && (holds_heap_storage(&self.data, checked)
                            || successors.iter().any(|callee| allocating_leaf(*callee)))
                })
            })
            .collect::<Vec<_>>();
        let mut requires = own.clone();
        loop {
            let mut changed = false;
            for index in (0..closure.len()).rev() {
                if !requires[index] && successors[index].iter().any(|callee| requires[*callee]) {
                    requires[index] = true;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        if !requires.first().copied().unwrap_or(false) {
            return None;
        }
        let components = entailment::strongly_connected_components(&successors);
        let mut component_of = vec![0; closure.len()];
        for (component, members) in components.iter().enumerate() {
            for member in members {
                component_of[*member] = component;
            }
        }
        let introducing = |component: usize| {
            components[component].iter().all(|member| {
                requires[*member]
                    && successors[*member]
                        .iter()
                        .all(|callee| component_of[*callee] == component || !requires[*callee])
            })
        };
        let mut parents = vec![None; closure.len()];
        let mut seen = vec![false; closure.len()];
        let mut order = vec![0];
        seen[0] = true;
        let mut cursor = 0;
        while let Some(node) = order.get(cursor).copied() {
            cursor += 1;
            for callee in &successors[node] {
                if requires[*callee] && !seen[*callee] {
                    seen[*callee] = true;
                    parents[*callee] = Some(node);
                    order.push(*callee);
                }
            }
        }
        let component = order
            .iter()
            .map(|node| component_of[*node])
            .find(|component| introducing(*component))?;
        let introducer = order
            .iter()
            .copied()
            .find(|node| component_of[*node] == component && own[*node])?;
        let mut path = vec![closure[introducer]];
        let mut current = introducer;
        while let Some(parent) = parents[current] {
            path.push(closure[parent]);
            current = parent;
        }
        path.reverse();
        Some(path)
    }
}

/// [STOR-8] whether a function's own concrete layout holds heap storage: the
/// type of one of its parameters, its result, or a value its body evaluates,
/// binds or releases holds a `Box` or a runtime-capacity shape [TYPE-9], as
/// itself or as a field, a payload field or an element. Such a value's
/// release frees heap storage, so its function needs the heap whether or not
/// it allocates.
fn holds_heap_storage(program: &CheckedProgramData, function: &CheckedFunction) -> bool {
    let mentions = FunctionMentions::collect(function);
    let mut visited = HashSet::new();
    mentions
        .types
        .iter()
        .copied()
        .chain(
            mentions
                .elements
                .iter()
                .filter_map(|element| program.elements.get(element.index()).copied()),
        )
        .any(|ty| type_holds_heap(program, ty, &mut visited))
}

fn type_holds_heap(
    program: &CheckedProgramData,
    ty: CheckedType,
    visited: &mut HashSet<NominalId>,
) -> bool {
    match ty {
        CheckedType::Buffer { .. } | CheckedType::Window { capacity: None, .. } => true,
        CheckedType::Array { element, .. } | CheckedType::Window { element, .. } => program
            .elements
            .get(element.index())
            .is_some_and(|element| type_holds_heap(program, *element, visited)),
        CheckedType::Nominal(id) => {
            if !visited.insert(id) {
                return false;
            }
            match program
                .nominals
                .get(id.0 as usize)
                .map(|nominal| &nominal.kind)
            {
                Some(CheckedNominalKind::Box { .. }) => true,
                Some(CheckedNominalKind::Struct { fields }) => fields
                    .iter()
                    .any(|field| type_holds_heap(program, field.ty, visited)),
                Some(CheckedNominalKind::Enum { variants }) => variants
                    .iter()
                    .flat_map(|variant| &variant.fields)
                    .any(|field| type_holds_heap(program, field.ty, visited)),
                Some(CheckedNominalKind::Opaque) | None => false,
            }
        }
        CheckedType::Unit
        | CheckedType::Bool
        | CheckedType::Integer(_)
        | CheckedType::Float(_)
        | CheckedType::Generic(_)
        | CheckedType::GenericInt(_)
        | CheckedType::GenericFloat(_) => false,
    }
}
