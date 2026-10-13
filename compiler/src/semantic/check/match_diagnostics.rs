//! [ERR-2] collect edit sites during the ordinary structural walk. Missing
//! arms contribute no invented edge; this scratch never reaches phase B.

use crate::{NonExhaustiveMatchSite, SemanticIssue, SemanticIssueKind, SemanticLocation};

use super::{CheckStop, CheckedType, Checker, FunctionId, NodeId, SemanticRule};

#[derive(Default)]
pub(super) struct MatchDiagnostics {
    events: Vec<(SemanticIssue, NonExhaustiveMatchSite)>,
}

impl MatchDiagnostics {
    pub(super) fn len(&self) -> usize {
        self.events.len()
    }

    pub(super) fn truncate(&mut self, len: usize) {
        self.events.truncate(len);
    }

    /// An established ERR-2 event precedes any later distinct-node stop. At
    /// its own node, an earlier rule such as GIVE-1 or LIV-1 still wins.
    /// This also preserves the established event if a later premise cannot
    /// be obtained; no compiler or capability failure becomes a guessed rule.
    pub(super) fn finish<T>(&self, result: Result<T, CheckStop>) -> Result<T, CheckStop> {
        let Some((first, _)) = self.events.first() else {
            return result;
        };
        if let Err(CheckStop::Issue(issue)) = &result
            && issue.rule.definition_rank() < first.rule.definition_rank()
            && matches!(
                (&issue.location, &first.location),
                (SemanticLocation::SourceNode(left, _), SemanticLocation::SourceNode(right, _))
                    if left == right
            )
        {
            return result;
        }
        let mut first = first.clone();
        if let SemanticIssueKind::NonExhaustiveMatch { other_sites, .. } = &mut first.kind {
            *other_sites = self.events.iter().map(|(_, site)| site.clone()).collect();
        }
        Err(CheckStop::source_issue(first))
    }
}

impl Checker<'_, '_> {
    pub(super) fn record_non_exhaustive_match(
        &mut self,
        function: FunctionId,
        node: NodeId,
        enum_type: CheckedType,
        missing_variants: Vec<String>,
    ) -> Result<(), CheckStop> {
        let path = self.types.declarations.tree.path(node)?.clone();
        if self
            .analysis
            .match_diagnostics
            .events
            .iter()
            .any(|(_, site)| {
                matches!(&site.location, SemanticLocation::SourceNode(recorded, _) if *recorded == path)
            })
        {
            return Ok(());
        }
        let location =
            SemanticLocation::SourceNode(path, self.types.declarations.tree.coordinate(node)?);
        let site = NonExhaustiveMatchSite {
            location: location.clone(),
            enum_name: self.types.checked_type_name(enum_type)?,
            missing_variants: missing_variants.clone(),
        };
        let issue = SemanticIssue {
            rule: SemanticRule::Err2,
            location,
            kind: SemanticIssueKind::NonExhaustiveMatch {
                missing_variants,
                other_sites: Vec::new(),
            },
            request: None,
        };
        let stop = self
            .types
            .attribute_to_request(function, CheckStop::source_issue(issue));
        let CheckStop::Issue(issue) = stop else {
            return Err(stop);
        };
        self.analysis.match_diagnostics.events.push((*issue, site));
        Ok(())
    }
}
