//! FN-6's finite written dependency graph, before any nominal or function
//! instance is materialized. Function-target flow is a finite set closure;
//! it never expands a type expression or evaluates a const argument.

use std::collections::{HashMap, VecDeque};

use super::{CheckStop, Checker, GenericParameter, GenericParameterKey};
use crate::syntax::NodeId;
use crate::{
    DeclarationClass, DeclarationId, DeclarationRole, LexicalUseRole, Production, ResolvedTarget,
    SemanticCompilerFailure, SemanticIssueKind, SemanticRule,
};

struct Template {
    node: NodeId,
    declaration: DeclarationId,
    name: String,
    parameters: Vec<(GenericParameterKey, ParameterKind)>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ParameterKind {
    Type,
    Const,
    Function,
}

#[derive(Clone)]
enum Argument {
    Forward(GenericParameterKey),
    Function {
        declaration: DeclarationId,
        application: NodeId,
    },
    Constructed,
}

struct Dependency {
    target: usize,
    node: NodeId,
    unchanged: bool,
}

impl<'unit> Checker<'unit> {
    pub(in crate::semantic::check) fn reject_instantiation_cycles(
        &self,
        items: &[NodeId],
    ) -> Result<(), CheckStop> {
        let mut templates = Vec::new();
        for node in items {
            let role = match self.tree.production(*node)? {
                Production::FnDecl | Production::FnSig => DeclarationRole::Function,
                Production::StructDecl => DeclarationRole::Struct,
                Production::EnumDecl => DeclarationRole::Enum,
                _ => continue,
            };
            let declaration = self.declaration_at(*node, role)?;
            templates.push(Template {
                node: *node,
                declaration: declaration.id(),
                name: declaration.spelling().into(),
                parameters: self.finite_parameters(*node)?,
            });
        }
        let by_declaration = templates
            .iter()
            .enumerate()
            .map(|(index, template)| (template.declaration, index))
            .collect::<HashMap<_, _>>();
        let mut edges = (0..templates.len())
            .map(|_| Vec::new())
            .collect::<Vec<Vec<Dependency>>>();
        let keys = templates
            .iter()
            .flat_map(|template| &template.parameters)
            .filter_map(|(key, kind)| (*kind == ParameterKind::Function).then_some(*key))
            .collect::<Vec<_>>();
        let key_indices = keys
            .iter()
            .enumerate()
            .map(|(index, key)| (*key, index))
            .collect::<HashMap<_, _>>();
        let mut flows = Vec::<(usize, Argument)>::new();

        // Every explicit use supplies an instantiation edge, including a
        // function mentioned only as an argument or a type in a signature.
        for usage in self.resolved.lexical_uses() {
            let ResolvedTarget::Source { declaration, class } = usage.target() else {
                continue;
            };
            if !matches!(
                class,
                DeclarationClass::Function
                    | DeclarationClass::NominalType
                    | DeclarationClass::StructConstructor
                    | DeclarationClass::EnumVariant
            ) {
                continue;
            }
            let record = self
                .resolved
                .declarations()
                .iter()
                .find(|record| record.id() == declaration)
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            let target = by_declaration
                .get(&declaration)
                .copied()
                .or_else(|| {
                    templates.iter().position(|template| {
                        self.tree.path(template.node).is_ok_and(|path| {
                            record
                                .origin()
                                .node()
                                .components()
                                .starts_with(path.components())
                        })
                    })
                })
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            let mut application = self
                .tree
                .node_with_path(usage.origin().node())
                .ok_or(SemanticCompilerFailure::InvalidResolution)?;
            if self.tree.production(application)? == Production::Callee {
                application = self
                    .tree
                    .parent(application)?
                    .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            }
            let caller = templates.iter().position(|template| {
                self.tree.path(template.node).is_ok_and(|path| {
                    usage
                        .origin()
                        .node()
                        .components()
                        .starts_with(path.components())
                })
            });
            let mut pending = vec![(target, application)];
            let mut seen = Vec::new();
            while let Some((target, application)) = pending.pop() {
                if seen.contains(&(target, application)) {
                    continue;
                }
                seen.push((target, application));
                let arguments = self.finite_arguments(application, &mut Vec::new())?;
                if let Some(caller) = caller {
                    let unchanged = arguments.len() == templates[caller].parameters.len()
                        && arguments.len() == templates[target].parameters.len()
                        && arguments.iter().zip(&templates[caller].parameters).zip(&templates[target].parameters).all(|((argument, caller), target)| {
                            caller.1 == target.1
                                && matches!(argument, Argument::Forward(key) if *key == caller.0)
                        });
                    edges[caller].push(Dependency {
                        target,
                        node: application,
                        unchanged,
                    });
                }
                for ((key, kind), argument) in templates[target].parameters.iter().zip(&arguments) {
                    if *kind == ParameterKind::Function {
                        flows.push((
                            *key_indices
                                .get(key)
                                .ok_or(SemanticCompilerFailure::InvalidResolution)?,
                            argument.clone(),
                        ));
                    }
                }
                for argument in arguments {
                    if let Argument::Function {
                        declaration,
                        application,
                    } = argument
                    {
                        pending.push((
                            *by_declaration
                                .get(&declaration)
                                .ok_or(SemanticCompilerFailure::InvalidResolution)?,
                            application,
                        ));
                    }
                }
            }
        }

        // Each bit denotes one source function, so this least fixed point
        // terminates independently of source order or machine speed.
        let mut targets = vec![vec![false; templates.len()]; keys.len()];
        loop {
            let mut changed = false;
            for (destination, argument) in &flows {
                let source = match argument {
                    Argument::Forward(key) => {
                        key_indices.get(key).map(|index| targets[*index].clone())
                    }
                    Argument::Function { declaration, .. } => {
                        let mut source = vec![false; templates.len()];
                        source[*by_declaration
                            .get(declaration)
                            .ok_or(SemanticCompilerFailure::InvalidResolution)?] = true;
                        Some(source)
                    }
                    Argument::Constructed => None,
                };
                if let Some(source) = source {
                    for (held, incoming) in targets[*destination].iter_mut().zip(source) {
                        changed |= incoming && !*held;
                        *held |= incoming;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        for (caller, template) in templates.iter().enumerate() {
            for call in self
                .tree
                .descendants_with(template.node, Production::Call)?
            {
                if let Some(key) = self.behavior_call_key(call)? {
                    let index = *key_indices
                        .get(&key)
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                    for (target, reachable) in targets[index].iter().enumerate() {
                        if *reachable {
                            // The callee's specialization is a proper finite
                            // subterm of one caller function argument. Equality
                            // with the caller's complete vector would require
                            // a self-containing bound function reference.
                            // FN-6 allows this projection only off a cycle.
                            edges[caller].push(Dependency {
                                target,
                                node: call,
                                unchanged: false,
                            });
                        }
                    }
                }
            }
            edges[caller].sort_by_key(|edge| (edge.node.index(), edge.target));
        }
        for (caller, outgoing) in edges.iter().enumerate() {
            for edge in outgoing {
                if edge.unchanged {
                    continue;
                }
                if let Some(back) = dependency_path(edge.target, caller, &edges) {
                    let mut cycle = vec![templates[caller].name.clone()];
                    cycle.extend(back.into_iter().map(|index| templates[index].name.clone()));
                    return self.issue_node(SemanticRule::Fn6, edge.node, SemanticIssueKind::PolymorphicRecursion {
                        cycle: cycle.join(" -> "),
                        mechanical_fix: "forward the complete type, const and function argument vector unchanged on the cycle, or move the changing instantiation off the cycle",
                    });
                }
            }
        }
        Ok(())
    }

    /// Finiteness uses only written kinds and identities. Validating an
    /// unrelated const parameter's integer domain here would preempt FN-9's
    /// selector-admission ordering. Ordinary template formation still checks
    /// every bound and domain before a program can be accepted.
    fn finite_parameters(
        &self,
        declaration: NodeId,
    ) -> Result<Vec<(GenericParameterKey, ParameterKind)>, CheckStop> {
        let Some(generics) = self
            .tree
            .first_child_with(declaration, Production::Generics)?
        else {
            return Ok(Vec::new());
        };
        let mut parameters = Vec::new();
        for node in self.tree.children_with(generics, Production::Gparam)? {
            if let Some(signature) = self.tree.first_child_with(node, Production::FnSig)? {
                parameters.push((
                    GenericParameterKey::Source(
                        self.declaration_at(signature, DeclarationRole::FunctionParameter)?
                            .id(),
                    ),
                    ParameterKind::Function,
                ));
            } else if let Some(application) = self.tree.group_application(node)? {
                parameters.extend(self.expand_formal_parameters(application)?.iter().map(
                    |parameter| {
                        (
                            parameter.key(),
                            match parameter {
                                GenericParameter::Type { .. } => ParameterKind::Type,
                                GenericParameter::Const { .. } => ParameterKind::Const,
                                GenericParameter::Function { .. } => ParameterKind::Function,
                            },
                        )
                    },
                ));
            } else if self.has_fixed(node, crate::FixedTerminal::Const)? {
                parameters.push((
                    GenericParameterKey::Source(
                        self.declaration_at(node, DeclarationRole::ConstGeneric)?
                            .id(),
                    ),
                    ParameterKind::Const,
                ));
            } else {
                parameters.push((
                    GenericParameterKey::Source(
                        self.declaration_at(node, DeclarationRole::GenericType)?
                            .id(),
                    ),
                    ParameterKind::Type,
                ));
            }
        }
        Ok(parameters)
    }

    fn finite_arguments(
        &self,
        application: NodeId,
        visiting: &mut Vec<DeclarationId>,
    ) -> Result<Vec<Argument>, CheckStop> {
        let mut result = Vec::new();
        if let Some(list) = self.tree.argument_list(application)? {
            for argument in self.tree.children_with(list, Production::Targ)? {
                if let Some(ty) = self.tree.first_child_with(argument, Production::Type)? {
                    if self.tree.names_nominal(ty)? {
                        match self.use_at(ty, LexicalUseRole::Type)?.target() {
                            ResolvedTarget::Source {
                                declaration,
                                class: DeclarationClass::GenericType,
                            } if self.tree.children(ty)?.is_empty() => {
                                result.push(Argument::Forward(GenericParameterKey::Source(
                                    declaration,
                                )));
                                continue;
                            }
                            ResolvedTarget::Source {
                                declaration,
                                class: DeclarationClass::Interface,
                            } => {
                                let selected = self.enclosing_group(ty, declaration)?;
                                result.extend(
                                    self.expand_formal_parameters(selected)?
                                        .iter()
                                        .map(|parameter| Argument::Forward(parameter.key())),
                                );
                                continue;
                            }
                            ResolvedTarget::Source {
                                declaration,
                                class: DeclarationClass::Binding,
                            } => {
                                if visiting.contains(&declaration) {
                                    let mut names = visiting
                                        .iter()
                                        .map(|declaration| self.declaration_spelling(*declaration))
                                        .collect::<Result<Vec<_>, _>>()?;
                                    names.push(self.declaration_spelling(declaration)?);
                                    return self.behavior_mismatch(
                                        SemanticRule::Fn3,
                                        ty,
                                        &format!(
                                            "acyclic binding groups; cycle {}",
                                            names.join(" -> ")
                                        ),
                                    );
                                }
                                visiting.push(declaration);
                                let group = self
                                    .behavior
                                    .actuals
                                    .get(&declaration)
                                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                                result.extend(self.finite_arguments(group.application, visiting)?);
                                for binding in &group.bindings {
                                    result.push(self.finite_function_argument(*binding)?);
                                }
                                visiting.pop();
                                continue;
                            }
                            _ => {}
                        }
                    }
                    result.push(Argument::Constructed);
                } else if let Some(value) =
                    self.tree.first_child_with(argument, Production::Const)?
                {
                    if self
                        .tree
                        .topology()
                        .node(value)
                        .is_some_and(|node| node.terminal_count == 1)
                        && !self.tree.direct_identifiers(value)?.is_empty()
                        && let ResolvedTarget::Source {
                            declaration,
                            class: DeclarationClass::ConstGeneric,
                        } = self.use_at(value, LexicalUseRole::Const)?.target()
                    {
                        result.push(Argument::Forward(GenericParameterKey::Source(declaration)));
                        continue;
                    }
                    result.push(Argument::Constructed);
                } else if let Some(function) = self
                    .tree
                    .first_child_with(argument, Production::FunctionArg)?
                {
                    result.push(self.finite_function_argument(function)?);
                }
                // Region arguments are alpha-bound, not instantiation keys.
            }
        }
        Ok(result)
    }

    fn finite_function_argument(&self, node: NodeId) -> Result<Argument, CheckStop> {
        let mut node = node;
        loop {
            let callee = self
                .tree
                .first_child_with(node, Production::Callee)?
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            if let Some(application) = self.tree.callee_application(callee)? {
                if self.tree.argument_list(node)?.is_some() {
                    return self.behavior_mismatch(
                        SemanticRule::Fn2,
                        node,
                        "a group member has an already instantiated signature",
                    );
                }
                let target = self
                    .use_at(application, LexicalUseRole::FormalGroup)?
                    .target();
                let name = self
                    .deferred_use_at(callee, crate::DeferredUseRole::FunctionMember)?
                    .spelling();
                if let ResolvedTarget::Source {
                    declaration,
                    class: DeclarationClass::Binding,
                } = target
                {
                    let actual = self
                        .behavior
                        .actuals
                        .get(&declaration)
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                    let formal = self
                        .behavior
                        .formals
                        .get(&actual.formal)
                        .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                    let Some(ordinal) = formal
                        .members
                        .iter()
                        .position(|(_, _, member)| member == name)
                    else {
                        return self.behavior_mismatch(
                            SemanticRule::Fn3,
                            callee,
                            "the group declares the selected member",
                        );
                    };
                    // FN-3 already refused binding-abbreviation cycles. Follow the finite
                    // alias chain without losing the original function's explicit
                    // application: it contributes both target-flow and argument
                    // construction edges to this graph.
                    node = actual.bindings[ordinal];
                    continue;
                }
                let ResolvedTarget::Source {
                    declaration,
                    class: DeclarationClass::Interface,
                } = target
                else {
                    return Ok(Argument::Constructed);
                };
                let selected = self.enclosing_group(application, declaration)?;
                let group = self
                    .behavior
                    .formals
                    .get(&declaration)
                    .ok_or(SemanticCompilerFailure::InvalidResolution)?;
                let Some((member, _, _)) =
                    group.members.iter().find(|(_, _, member)| member == name)
                else {
                    return Ok(Argument::Constructed);
                };
                return Ok(Argument::Forward(GenericParameterKey::Member {
                    application: selected,
                    member: *member,
                }));
            }
            return Ok(
                match self
                    .use_at(callee, LexicalUseRole::FunctionBinding)?
                    .target()
                {
                    ResolvedTarget::Source {
                        declaration,
                        class: DeclarationClass::Function,
                    } => Argument::Function {
                        declaration,
                        application: node,
                    },
                    ResolvedTarget::Source {
                        declaration,
                        class: DeclarationClass::FunctionParameter,
                    } if self.tree.argument_list(node)?.is_none() => {
                        Argument::Forward(GenericParameterKey::Source(declaration))
                    }
                    _ => Argument::Constructed,
                },
            );
        }
    }
}

fn dependency_path(start: usize, finish: usize, edges: &[Vec<Dependency>]) -> Option<Vec<usize>> {
    let mut previous = vec![None; edges.len()];
    let mut seen = vec![false; edges.len()];
    let mut pending = VecDeque::from([start]);
    seen[start] = true;
    while let Some(node) = pending.pop_front() {
        if node == finish {
            let mut path = vec![node];
            let mut cursor = node;
            while let Some(parent) = previous[cursor] {
                path.push(parent);
                cursor = parent;
            }
            path.reverse();
            return Some(path);
        }
        for edge in &edges[node] {
            if !seen[edge.target] {
                seen[edge.target] = true;
                previous[edge.target] = Some(node);
                pending.push_back(edge.target);
            }
        }
    }
    None
}
