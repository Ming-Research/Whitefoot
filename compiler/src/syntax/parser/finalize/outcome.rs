use crate::{ByteOffset, SourceId};

use super::topology::FinalizedTopology;
use crate::ClassifiedBundle;

use crate::syntax::parser::{ParsedBundle, SyntaxCoordinate, SyntaxRule};

/// Which inclusive finalization ceiling was exceeded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FinalizeLimit {
    /// Total deterministic finalizer work units.
    Work,
    /// Simultaneously live postorder roots.
    Roots,
    /// Simultaneously pending local-shape tasks.
    ShapeTasks,
    /// Finalized production-node records.
    Nodes,
    /// Production-to-production child edges.
    ChildEdges,
    /// Finalized terminal records.
    Terminals,
    /// Ordered bundle-root source extents.
    Sources,
}

/// Explicit ceilings for the one linear internal finalizer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FinalizeLimits {
    /// Maximum deterministic finalizer work units.
    pub max_work: u64,
    /// Maximum simultaneously live postorder roots.
    pub max_roots: u64,
    /// Maximum pending local-shape tasks.
    pub max_shape_tasks: u64,
    /// Maximum finalized production nodes.
    pub max_nodes: u64,
    /// Maximum production-child edges.
    pub max_child_edges: u64,
    /// Maximum terminal records.
    pub max_terminals: u64,
    /// Maximum ordered source extents.
    pub max_sources: u64,
}

/// Which finalizer allocation could not be extended.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FinalizeStorage {
    /// Live postorder roots.
    Roots,
    /// Local production-shape tasks.
    ShapeTasks,
    /// Production-node records.
    Nodes,
    /// Flat production-child edges.
    ChildEdges,
    /// Terminal metadata.
    Terminals,
    /// Bundle-root source extents.
    SourceExtents,
}

/// A finalizer resource failure, never a Whitefoot source verdict.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FinalizeResourceFailure {
    /// One caller-selected inclusive ceiling was exceeded.
    LimitExceeded {
        /// Exceeded ceiling.
        limit: FinalizeLimit,
        /// Configured inclusive maximum.
        maximum: u64,
        /// First requested value beyond the maximum.
        actual: u64,
    },
    /// One requested allocation count did not fit the host address space.
    AddressSpaceExceeded {
        /// Affected storage.
        storage: FinalizeStorage,
        /// Requested element count.
        requested: u64,
    },
    /// The allocator could not extend already-counted storage.
    StorageUnavailable {
        /// Affected storage.
        storage: FinalizeStorage,
        /// Requested element count.
        requested: u64,
    },
}

/// A trusted parser/tree invariant rejected by finalization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FinalizeCompilerFailure {
    /// The postorder arena did not reduce to exactly one program root.
    InvalidRoot,
    /// A production's direct-child count or subtree range was malformed.
    InvalidPostorder,
    /// One local production shape disagreed with active specification source EBNF.
    InvalidProductionShape,
    /// A terminal leaf did not own the next exact classified token.
    InvalidTokenCoverage,
    /// A selected terminal predicate was absent from its retained membership set.
    InvalidTerminalPredicate,
    /// A non-program node crossed sources or had a wrong source extent.
    InvalidSourceExtent,
    /// A production child acquired no unique parent or a wrong ordinal.
    InvalidParentTopology,
    /// Parser counters disagreed with the finalized inventory.
    CountDisagreement,
    /// Generated grammar data was missing or internally inconsistent.
    InvalidGrammarData,
    /// A checked finalizer counter overflowed.
    CounterOverflow,
}

/// Exact ordered coverage of one source record in the bundle-root extent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BundleSourceExtent {
    source: SourceId,
    start: ByteOffset,
    end: ByteOffset,
}

impl BundleSourceExtent {
    pub(crate) const fn new(source: SourceId, end: ByteOffset) -> Self {
        Self {
            source,
            start: ByteOffset::new(0),
            end,
        }
    }

    /// Returns the source's bundle-order identity.
    #[must_use]
    pub const fn source(self) -> SourceId {
        self.source
    }

    /// Returns the always-zero inclusive start.
    #[must_use]
    pub const fn start(self) -> ByteOffset {
        self.start
    }

    /// Returns the source byte length as exclusive end.
    #[must_use]
    pub const fn end(self) -> ByteOffset {
        self.end
    }
}

/// Opaque result of the one linear tree finalizer.
///
/// This value has checked internal topology but has not passed FORM-2 and is
/// not canonical syntax, semantic acceptance, or portable identity.
#[derive(Debug)]
pub struct FinalizedBundle {
    pub(crate) parsed: ParsedBundle,
    pub(crate) topology: FinalizedTopology,
}

impl FinalizedBundle {
    /// Returns the exact classified input retained by the finalized tree.
    #[must_use]
    pub const fn classified_bundle(&self) -> &ClassifiedBundle {
        &self.parsed.classified
    }

    /// Returns the finalized production-node count.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.topology.nodes.len()
    }

    /// Returns the finalized terminal-leaf count.
    #[must_use]
    pub fn terminal_count(&self) -> usize {
        self.topology.terminals.len()
    }

    /// Returns the exact ordered `BundleRootExtent` records.
    #[must_use]
    pub fn root_extent(&self) -> &[BundleSourceExtent] {
        &self.topology.source_extents
    }
}

/// Failure-atomic result of internal derivation-tree finalization.
#[derive(Debug)]
pub enum FinalizeOutcome {
    /// The private derivation passed the complete linear topology audit.
    Complete(Box<FinalizedBundle>),
    /// Explicit ceilings or host storage prevented completion.
    ResourceFailure(FinalizeResourceFailure),
    /// A trusted parser, tree, or grammar-data invariant failed.
    CompilerFailure(FinalizeCompilerFailure),
}

/// One finalized production-node path from the compilation-unit root.
///
/// Components are zero-based production-child ordinals. This runtime value is
/// a diagnostic location, not a portable artifact reference.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct NodePath {
    pub(crate) components: Vec<u32>,
}

impl NodePath {
    /// Returns the root-to-node child-ordinal sequence.
    #[must_use]
    pub fn components(&self) -> &[u32] {
        &self.components
    }
}

/// Exact DIAG-1 location of a canonical-source rejection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CanonicalLocation {
    /// No existing production node owns the source boundary.
    SourceBytes(SyntaxCoordinate),
    /// One existing finalized production node owns the trivia gap.
    SourceNode(NodePath, SyntaxCoordinate),
}

impl CanonicalLocation {
    /// Returns the exact source-byte extent both forms carry.
    #[must_use]
    pub const fn coordinate(&self) -> SyntaxCoordinate {
        match self {
            Self::SourceBytes(coordinate) | Self::SourceNode(_, coordinate) => *coordinate,
        }
    }
}

/// The first exact FORM-2 source-forest mismatch in stage order.
///
/// Canonical form is machine-decided, so the auditor knows the exact bytes it
/// wanted at the point it rejects. Carrying them, beside the bytes it found,
/// is what turns this rejection from a byte offset a writer has to bisect into
/// a difference they can read.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalIssue {
    pub(crate) location: CanonicalLocation,
    /// The exact trivia bytes FORM-2 requires between the two terminals.
    pub(crate) expected: String,
    /// The exact trivia bytes the source carries there instead.
    pub(crate) found: String,
}

impl CanonicalIssue {
    /// Returns the owning numbered rule, always FORM-2.
    #[must_use]
    pub const fn rule(&self) -> SyntaxRule {
        SyntaxRule::Form2
    }

    /// Returns the exact source-bound DIAG-1 location.
    #[must_use]
    pub const fn location(&self) -> &CanonicalLocation {
        &self.location
    }
}

/// Which canonical-source audit ceiling was exceeded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalLimit {
    /// Total deterministic audit work units.
    Work,
    /// Bytes in one source record.
    SourceBytes,
    /// Bytes across the complete ordered source bundle.
    TotalSourceBytes,
    /// Terminal-gap records used by the streaming comparator.
    Gaps,
    /// Components in one published node path.
    PathComponents,
}

/// Explicit ceilings for tree-driven FORM-2 auditing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CanonicalLimits {
    /// Maximum deterministic audit work units.
    pub max_work: u64,
    /// Maximum bytes in one source record.
    pub max_source_bytes: u64,
    /// Maximum bytes across the complete ordered source bundle.
    pub max_total_source_bytes: u64,
    /// Maximum gap records.
    pub max_gaps: u64,
    /// Maximum components in one diagnostic node path.
    pub max_path_components: u64,
}

/// Which canonical-source allocation could not be extended.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalStorage {
    /// Per-terminal gap-style records.
    Gaps,
    /// One diagnostic node path.
    NodePath,
    /// Rendered canonical bytes for one source.
    RenderedSource,
}

/// A canonical-source audit resource failure, never a source verdict.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalResourceFailure {
    /// One caller-selected inclusive ceiling was exceeded.
    LimitExceeded {
        /// Exceeded ceiling.
        limit: CanonicalLimit,
        /// Configured inclusive maximum.
        maximum: u64,
        /// First requested value beyond the maximum.
        actual: u64,
    },
    /// One requested allocation count did not fit the host address space.
    AddressSpaceExceeded {
        /// Affected storage.
        storage: CanonicalStorage,
        /// Requested element count.
        requested: u64,
    },
    /// The allocator could not extend already-counted storage.
    StorageUnavailable {
        /// Affected storage.
        storage: CanonicalStorage,
        /// Requested element count.
        requested: u64,
    },
}

/// A trusted finalized-tree or renderer invariant failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalCompilerFailure {
    /// Finalized node, terminal, source, or parent metadata was inconsistent.
    InvalidFinalizedTree,
    /// A terminal leaf changed after finalization or rendered inconsistently.
    TerminalBindingDisagreement,
    /// A checked audit counter overflowed.
    CounterOverflow,
}

/// Opaque active-specification canonical syntax capability.
///
/// This capability proves one finalized derivation and byte-exact per-source
/// FORM-2 rendering. It is not a semantic verdict, artifact, optimizer fact,
/// backend input, compiler executable, or release claim.
#[derive(Debug)]
pub struct CanonicalSyntaxUnit {
    pub(crate) finalized: FinalizedBundle,
}

impl CanonicalSyntaxUnit {
    /// Returns the exact classified source input retained by this capability.
    #[must_use]
    pub const fn classified_bundle(&self) -> &ClassifiedBundle {
        &self.finalized.parsed.classified
    }

    /// Returns the complete finalized production-node count.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.finalized.topology.nodes.len()
    }

    /// Returns the complete finalized terminal count.
    #[must_use]
    pub fn terminal_count(&self) -> usize {
        self.finalized.topology.terminals.len()
    }

    /// Returns the exact ordered bundle-root source extents.
    #[must_use]
    pub fn root_extent(&self) -> &[BundleSourceExtent] {
        &self.finalized.topology.source_extents
    }
}

/// One source's exact canonical bytes, as the renderer emits them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedSource {
    /// Bundle identity of the rendered source.
    pub source: SourceId,
    /// Exact FORM-2 bytes the finalized tree denotes.
    pub bytes: Vec<u8>,
}

/// Failure-atomic result of tree-driven FORM-2 rendering.
///
/// Rendering has no source verdict. The auditor asks whether given bytes are
/// canonical and can answer no; the renderer is handed a finalized tree and
/// emits the bytes that tree denotes, so it either produces them or hits a
/// ceiling or a broken invariant.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RenderOutcome {
    /// Exact canonical bytes for every source in bundle order.
    Complete(Vec<RenderedSource>),
    /// Explicit ceilings or host storage prevented complete rendering.
    ResourceFailure(CanonicalResourceFailure),
    /// A trusted finalized-tree or renderer invariant failed.
    CompilerFailure(CanonicalCompilerFailure),
}

/// Failure-atomic result of the tree-driven FORM-2 audit.
#[derive(Debug)]
pub enum CanonicalOutcome {
    /// Finalized syntax renders to every exact source byte.
    Complete(CanonicalSyntaxUnit),
    /// The first source/gap mismatch under DIAG-1 stage order.
    SourceIssue(CanonicalIssue),
    /// Explicit ceilings or host storage prevented the complete audit.
    ResourceFailure(CanonicalResourceFailure),
    /// A trusted finalized-tree or renderer invariant failed.
    CompilerFailure(CanonicalCompilerFailure),
}
