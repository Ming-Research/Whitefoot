//! Source presentation for runtime waits; no acceptance or scheduling consumer.

use std::collections::HashMap;

use crate::semantic::model::{BindingId, CheckedFunction, CheckedStatement, FunctionId};
use crate::{NodePath, Production};

use super::DeclarationInventory;

impl DeclarationInventory<'_> {
    pub(super) fn wait_report_sources(
        &self,
        functions: &[CheckedFunction],
        executable: &[FunctionId],
    ) -> HashMap<NodePath, String> {
        let mut sources = HashMap::new();
        for id in executable {
            let function = &functions[id.0 as usize];
            let mut blocks = vec![function.body.as_deref().unwrap_or_default()];
            while let Some(block) = blocks.pop() {
                for (index, statement) in block.iter().enumerate() {
                    match statement {
                        CheckedStatement::Atomic {
                            node_path, body, ..
                        } => {
                            if let Some(site) = self.wait_site(node_path) {
                                sources.insert(
                                    node_path.clone(),
                                    format!(
                                        "waits for the guard of the atomic statement at {site}"
                                    ),
                                );
                            }
                            blocks.push(body);
                        }
                        CheckedStatement::Loop { body, .. }
                        | CheckedStatement::CountedRange { body, .. } => blocks.push(body),
                        CheckedStatement::Match { arms, .. }
                        | CheckedStatement::ValueMatchLet { arms, .. } => {
                            blocks.extend(arms.iter().map(|arm| arm.body.as_slice()));
                        }
                        CheckedStatement::Return { node_path, .. } => {
                            if let Some(site) = self.wait_site(node_path) {
                                sources.insert(node_path.clone(), site);
                            }
                        }
                        CheckedStatement::PropagateLet { context, .. } => {
                            if let Some(site) = self.wait_site(&context.node_path) {
                                sources.insert(context.node_path.clone(), site);
                            }
                        }
                        CheckedStatement::Let {
                            node_path, binding, ..
                        } => {
                            let Some(plan) = function
                                .waiting
                                .context_awaits
                                .iter()
                                .find(|plan| &plan.statement == node_path)
                            else {
                                continue;
                            };
                            let Some(spawn) = self.wait_site(node_path) else {
                                continue;
                            };
                            // The current lowering awaits an unplaced join at
                            // the next sibling, if any. Describe that emitted
                            // site without changing its existing schedule.
                            let next_statement = plan.before.is_none() && index + 1 < block.len();
                            let emitted_before = plan.before.or(next_statement.then_some(1));
                            let Some((site, loop_body)) =
                                self.bound_join_site(node_path, emitted_before)
                            else {
                                continue;
                            };
                            let uses_binding = plan.before.is_some_and(|offset| {
                                block
                                    .get(index + offset as usize)
                                    .is_some_and(|later| names_binding(later, *binding))
                            });
                            let kind = match plan.before {
                                Some(_) if uses_binding => "explicit use of the binding",
                                Some(_) => "implicit join before leaving this block",
                                None if next_statement => "implicit join before the next statement",
                                None if loop_body => {
                                    "implicit join at the end of this loop iteration"
                                }
                                None => "implicit join at the end of this block",
                            };
                            sources.insert(
                                node_path.clone(),
                                format!(
                                    "waits at {site} to join the context spawned at {spawn} ({kind})"
                                ),
                            );
                        }
                        _ => {}
                    }
                }
            }
            for start in &function.waiting.context_starts {
                if !sources.contains_key(start)
                    && !function
                        .waiting
                        .context_awaits
                        .iter()
                        .any(|plan| &plan.statement == start)
                    && let Some(site) = self.wait_site(start)
                {
                    sources.insert(start.clone(), format!("context spawned at {site}"));
                }
            }
        }
        sources
    }

    fn wait_site(&self, path: &NodePath) -> Option<String> {
        let node = self.tree.node_with_path(path)?;
        Some(self.lock_ledger_site(self.tree.coordinate(node).ok()?))
    }

    fn bound_join_site(&self, spawn: &NodePath, before: Option<u32>) -> Option<(String, bool)> {
        let mut statement = self.tree.node_with_path(spawn)?;
        while self.tree.production(statement).ok()? != Production::Stmt {
            statement = self.tree.parent(statement).ok()??;
        }
        let owner = self.tree.parent(statement).ok()??;
        let coordinate = if let Some(offset) = before {
            let statements = self.tree.children_with(owner, Production::Stmt).ok()?;
            let index = statements.iter().position(|node| *node == statement)?;
            self.tree
                .coordinate(*statements.get(index + offset as usize)?)
                .ok()?
        } else {
            self.tree.closing_brace_coordinate(owner).ok()?
        };
        let loop_body = matches!(
            self.tree.production(owner).ok()?,
            Production::LoopStmt | Production::ForStmt
        );
        Some((self.lock_ledger_site(coordinate), loop_body))
    }
}

/// Distinguish a written use from a leaving edge after the existing plan has
/// selected the join; this answer never places or removes a join.
fn names_binding(statement: &CheckedStatement, binding: BindingId) -> bool {
    use crate::semantic::permission::{expression_names, leaf_names};
    let block = |body: &[CheckedStatement]| body.iter().any(|item| names_binding(item, binding));
    match statement {
        CheckedStatement::Match { scrutinee, arms, .. }
        | CheckedStatement::ValueMatchLet { scrutinee, arms, .. } => {
            expression_names(scrutinee, binding) || arms.iter().any(|arm| block(&arm.body))
        }
        CheckedStatement::Loop { body, .. } => block(body),
        CheckedStatement::CountedRange {
            lower, upper, body, ..
        } => {
            expression_names(lower, binding)
                || expression_names(upper, binding)
                || block(body)
        }
        CheckedStatement::Atomic {
            targets, guard, body, ..
        } => {
            targets
                .iter()
                .flat_map(crate::semantic::CheckedTarget::expressions)
                .any(|value| expression_names(value, binding))
                || guard
                    .as_deref()
                    .is_some_and(|value| expression_names(value, binding))
                || block(body)
        }
        _ => leaf_names(statement, binding),
    }
}
