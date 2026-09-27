use crate::lexer::Token;
use crate::syntax::grammar::Production;
use crate::syntax::terminal::TerminalPredicate;
use crate::{ByteOffset, SourceId};

/// One private typed postorder element.
#[derive(Debug)]
pub(crate) enum DerivationElement {
    Terminal {
        token: Token,
        predicate: TerminalPredicate,
    },
    Production {
        production: Production,
        child_count: u32,
        subtree_elements: u64,
        extent: DerivationExtent,
    },
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum DerivationExtent {
    Source {
        source: SourceId,
        start: ByteOffset,
        end: ByteOffset,
    },
    BundleRoot,
}

#[derive(Debug)]
pub(crate) struct DerivationTree {
    pub(crate) elements: Vec<DerivationElement>,
    pub(crate) terminal_count: u64,
    pub(crate) production_count: u64,
}

#[derive(Debug)]
pub(crate) struct Frame {
    pub(crate) production: Production,
    pub(crate) element_start: usize,
    pub(crate) child_count: u32,
    pub(crate) extent: Option<(SourceId, ByteOffset, ByteOffset)>,
    pub(crate) atom_only: bool,
    /// Whether this frame or any frame enclosing it is a `contract_block`.
    ///
    /// The flag is carried rather than recomputed because the enclosing frames
    /// are the only record of the grammar position once the parser has
    /// descended into an `expr`, and one diagnostic repair — GRAM-9's — differs
    /// between a body and a contract block.
    pub(crate) in_contract: bool,
}
