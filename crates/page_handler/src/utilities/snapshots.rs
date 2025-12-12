//! Common snapshot types and aliases used across `page_handler`.

use js::NodeKey;

/// Layout node kind for snapshot entries
#[derive(Debug, Clone)]
pub enum LayoutNodeKind {
    /// Document root
    Document,
    /// Block-level element
    Block { tag: String },
    /// Inline text node
    InlineText { text: String },
}

/// Tuple representing one entry in a layout snapshot: (node key, kind, children).
pub type SnapshotItem = (NodeKey, LayoutNodeKind, Vec<NodeKey>);

/// Owned snapshot list.
pub type Snapshot = Vec<SnapshotItem>;

/// Borrowed view of a snapshot.
pub type SnapshotSlice<'snapshot> = &'snapshot [SnapshotItem];

/// Integer rect shorthand: x0, y0, x1, y1
pub type IRect = (i32, i32, i32, i32);
