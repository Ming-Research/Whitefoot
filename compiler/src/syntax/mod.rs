//! Resource-bounded canonical syntax construction for the active Whitefoot specification.
//!
//! The implementation classifies every formed token against the complete
//! context-free terminal set, constructs one private iterative LL(2)
//! derivation, finalizes its topology and source binding, and audits exact
//! FORM-2 bytes from that same tree before publishing [`CanonicalSyntaxUnit`].
//! This crate does not perform semantic checking or create portable artifacts.

mod classifier;
/// Grammar tables derived from the active specification.
pub mod grammar;
mod outcome;
mod parser;
/// The active specification's context-free terminal partition.
pub mod terminal;

pub use classifier::classify_terminals;
pub use outcome::{
    ClassifiedBundle, ClassifiedToken, TerminalCompilerFailure, TerminalInvocationFailure,
    TerminalIssue, TerminalIssueOwner, TerminalLimit, TerminalLimits, TerminalOutcome,
    TerminalResourceFailure, TerminalStorage,
};
pub use parser::{
    BundleSourceExtent, CanonicalCompilerFailure, CanonicalIssue, CanonicalLimit, CanonicalLimits,
    CanonicalLocation, CanonicalOutcome, CanonicalResourceFailure, CanonicalStorage,
    CanonicalSyntaxUnit, ExpectedTerminals, FinalizeCompilerFailure, FinalizeLimit, FinalizeLimits,
    FinalizeOutcome, FinalizeResourceFailure, FinalizeStorage, FinalizedBundle, NodePath,
    ParseCompilerFailure, ParseInvocationFailure, ParseLimit, ParseLimits, ParseOutcome,
    ParseResourceFailure, ParseStorage, ParsedBundle, RenderOutcome, RenderedSource,
    SyntaxCoordinate, SyntaxIssue, SyntaxRule, audit_canonical, finalize, parse, parse_graph,
    render_canonical,
};

pub(crate) use parser::{FinalizedExtent, FinalizedTopology, NodeId};

#[cfg(test)]
mod tests;
