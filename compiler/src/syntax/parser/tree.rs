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
    pub(crate) in_contract: bool,
    /// Whether an enclosing `contract_define` admits a preceding definition.
    pub(crate) in_contract_define: bool,
    /// Whether this frame or any frame enclosing it is a `range_clause`.
    ///
    /// [RANGE-1] forms no call, so the repair for `&&` and `||` differs there.
    pub(crate) in_range: bool,
}
