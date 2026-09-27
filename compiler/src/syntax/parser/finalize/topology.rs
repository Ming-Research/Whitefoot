use crate::syntax::grammar::Production;
use crate::{ByteOffset, SourceId};

use super::outcome::BundleSourceExtent;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct NodeId(u32);

impl NodeId {
    pub(crate) fn from_index(index: usize) -> Option<Self> {
        u32::try_from(index).ok().map(Self)
    }

    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FinalizedExtent {
    Source {
        source: SourceId,
        start: ByteOffset,
        end: ByteOffset,
    },
    BundleRoot,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct NodeRecord {
    pub(crate) production: Production,
    pub(crate) parent: Option<NodeId>,
    pub(crate) child_ordinal: u32,
    pub(crate) child_start: u32,
    pub(crate) child_count: u32,
    pub(crate) tree_depth: u32,
    pub(crate) format_depth: u32,
    pub(crate) top_item: Option<NodeId>,
    pub(crate) first_terminal: u64,
    pub(crate) terminal_count: u64,
    pub(crate) extent: FinalizedExtent,
    pub(crate) body_open: Option<u64>,
    pub(crate) body_close: Option<u64>,
    /// The second brace pair, which only `if_stmt` and `value_if` own.
    ///
    /// Every other block-bearing production owns exactly one pair, because a
    /// nested block always belongs to a nested production node. An `if` with a
    /// braced `else` owns two; an `else if` chain owns one plus an `else`
    /// terminal whose block belongs to the nested `if_stmt` node.
    pub(crate) else_open: Option<u64>,
    pub(crate) else_close: Option<u64>,
    /// Whether this node directly owns an `else` terminal.
    pub(crate) has_else: bool,
}

impl NodeRecord {
    pub(crate) fn last_terminal(self) -> Option<u64> {
        self.first_terminal
            .checked_add(self.terminal_count.checked_sub(1)?)
    }

    /// Returns the brace pairs this node owns, in source order.
    /// The indentation depth of one terminal this node owns directly: the
    /// node's own depth, one deeper when the terminal lies inside one of the
    /// node's braced blocks. An entry's requirement block is the one block
    /// whose content is its owner's own terminals [GRAM-2, FORM-2].
    pub(crate) fn terminal_depth(self, terminal: u64) -> u32 {
        let inside = self
            .body_ranges()
            .iter()
            .flatten()
            .any(|(open, close)| terminal > *open && terminal < *close);
        self.format_depth.saturating_add(u32::from(inside))
    }

    pub(crate) fn body_ranges(self) -> [Option<(u64, u64)>; 2] {
        [
            self.body_open.zip(self.body_close),
            self.else_open.zip(self.else_close),
        ]
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct TerminalRecord {
    pub(crate) element_index: usize,
    pub(crate) source: SourceId,
    pub(crate) local_ordinal: u64,
    pub(crate) owner: Option<NodeId>,
}

#[derive(Debug)]
pub(crate) struct FinalizedTopology {
    pub(crate) nodes: Vec<NodeRecord>,
    pub(crate) children: Vec<NodeId>,
    pub(crate) terminals: Vec<TerminalRecord>,
    pub(crate) source_extents: Vec<BundleSourceExtent>,
    pub(crate) root: NodeId,
}

impl FinalizedTopology {
    pub(crate) fn node(&self, id: NodeId) -> Option<&NodeRecord> {
        self.nodes.get(id.index())
    }

    pub(crate) fn node_children(&self, id: NodeId) -> Option<&[NodeId]> {
        let node = self.node(id)?;
        let start = usize::try_from(node.child_start).ok()?;
        let count = usize::try_from(node.child_count).ok()?;
        self.children.get(start..start.checked_add(count)?)
    }
}
