//! Source presentation for the non-normative lowering lock ledger.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::semantic::model::{CheckedBodyDisposition, CheckedFunction, CheckedStatement, FunctionId};
use crate::source::DescriptionPosition;
use crate::{NodePath, SourceDescription, SourceId, SyntaxCoordinate};

use super::DeclarationInventory;

/// Lazily indexed source lines, with no acceptance or lowering authority.
#[derive(Default)]
pub(super) struct SourceLines {
    starts: RefCell<HashMap<SourceId, Vec<usize>>>,
}

impl DeclarationInventory<'_> {
    pub(super) fn lock_ledger_site(&self, coordinate: SyntaxCoordinate) -> SourceDescription {
        let site = || {
            let bundle = self
                .resolved
                .syntax()
                .classified_bundle()
                .source_bundle();
            let file = bundle.file(coordinate.source())?;
            let start = usize::try_from(coordinate.start().value()).ok()?;
            if start > file.bytes().len() {
                return None;
            }
            let mut sources = self.ledger_line_starts.starts.borrow_mut();
            let line_starts = sources.entry(coordinate.source()).or_insert_with(|| {
                std::iter::once(0)
                    .chain(
                        file.bytes()
                            .iter()
                            .enumerate()
                            .filter_map(|(offset, byte)| (*byte == b'\n').then_some(offset + 1)),
                    )
                    .collect()
            });
            let line = line_starts.partition_point(|offset| *offset <= start);
            let line_start = *line_starts.get(line.checked_sub(1)?)?;
            let prefix = file.bytes().get(line_start..start)?;
            let column = std::str::from_utf8(prefix).ok()?.chars().count() + 1;
            Some(SourceDescription::position(DescriptionPosition {
                module: if bundle.is_module_program() {
                    Some(bundle.module(file.module())?.qualified_name())
                } else {
                    None
                },
                logical_path: file.logical_path().as_str().to_owned(),
                display_path: file.display_path().to_owned(),
                line,
                column,
            }))
        };
        site().unwrap_or_else(|| SourceDescription::literal("<unknown site>"))
    }

    /// Retain available written locations only. Missing report metadata must
    /// not reject a program or change the locks lowering takes.
    pub(super) fn lock_ledger_sources(
        &self,
        functions: &[CheckedFunction],
        executable_functions: &[FunctionId],
    ) -> HashMap<NodePath, SourceDescription> {
        let mut statements = Vec::new();
        for id in executable_functions {
            let Some(function) = functions.get(id.0 as usize) else {
                continue;
            };
            if matches!(function.body_disposition, CheckedBodyDisposition::Inhabited) {
                statements.extend(function.body.as_deref().unwrap_or_default());
            }
        }
        let mut sources = HashMap::new();
        while let Some(statement) = statements.pop() {
            match statement {
                CheckedStatement::Atomic {
                    node_path, body, ..
                } => {
                    statements.extend(body);
                    if sources.contains_key(node_path) {
                        continue;
                    }
                    let Some(node) = self.tree.node_with_path(node_path) else {
                        continue;
                    };
                    let mut descendants = vec![node];
                    while let Some(descendant) = descendants.pop() {
                        if let Ok(path) = self.tree.path(descendant) {
                            let site = self
                                .tree
                                .coordinate(descendant)
                                .ok()
                                .map(|coordinate| self.lock_ledger_site(coordinate))
                                .unwrap_or_else(|| SourceDescription::literal("<unknown site>"));
                            sources.insert(path.clone(), site);
                        }
                        if let Ok(children) = self.tree.children(descendant) {
                            descendants.extend(children);
                        }
                    }
                }
                CheckedStatement::Match { arms, .. }
                | CheckedStatement::ValueMatchLet { arms, .. } => {
                    for arm in arms {
                        statements.extend(&arm.body);
                    }
                }
                CheckedStatement::Loop { body, .. }
                | CheckedStatement::CountedRange { body, .. } => statements.extend(body),
                CheckedStatement::Let { .. }
                | CheckedStatement::DestructuringLet { .. }
                | CheckedStatement::PropagateLet { .. }
                | CheckedStatement::Set { .. }
                | CheckedStatement::Evaluate { .. }
                | CheckedStatement::DropExpression { .. }
                | CheckedStatement::Proof(_)
                | CheckedStatement::Return { .. }
                | CheckedStatement::Give { .. }
                | CheckedStatement::Break { .. }
                | CheckedStatement::Continue { .. } => {}
            }
        }
        sources
    }
}
