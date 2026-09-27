mod diagnostic;
mod engine;
mod finalize;
mod outcome;
mod tree;

pub use engine::{parse, parse_graph};
pub use finalize::{
    BundleSourceExtent, CanonicalCompilerFailure, CanonicalIssue, CanonicalLimit, CanonicalLimits,
    CanonicalLocation, CanonicalOutcome, CanonicalResourceFailure, CanonicalStorage,
    CanonicalSyntaxUnit, FinalizeCompilerFailure, FinalizeLimit, FinalizeLimits, FinalizeOutcome,
    FinalizeResourceFailure, FinalizeStorage, FinalizedBundle, NodePath, RenderOutcome,
    RenderedSource, audit_canonical, finalize, render_canonical,
};
pub use outcome::{
    ExpectedTerminals, ParseCompilerFailure, ParseInvocationFailure, ParseLimit, ParseLimits,
    ParseOutcome, ParseResourceFailure, ParseStorage, ParsedBundle, SyntaxCoordinate, SyntaxIssue,
    SyntaxRule,
};

pub(crate) use diagnostic::{
    DecisionSelection, DiagnosticResult, DiagnosticSite, ProbeContext, diagnose_decision,
    direct_mismatch, select_arm,
};
pub(crate) use finalize::topology::{FinalizedExtent, FinalizedTopology, NodeId};
pub(crate) use outcome::{ExpectedBuilder, Work};
pub(crate) use tree::{DerivationElement, DerivationTree, Frame};

#[cfg(test)]
mod tests;
