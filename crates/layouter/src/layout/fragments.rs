//! Fragment tree scaffold: represents the generated inline fragments and line boxes.
//!
//! This initial version is intentionally simple: it groups consecutive inline-level
//! children under a block into runs and produces a single LineBox with one Fragment
//! per inline child or text node. Wrapping and actual line breaking are not yet
//! implemented; this serves as a structural stepping stone for future work.

use js::NodeKey;

use super::geometry::LayoutRect;
use super::boxes::{LayoutBoxTree, LayoutBoxId, LayoutBoxKind};

/// The kind of fragment produced.
#[derive(Debug, Clone)]
pub enum FragmentKind {
    /// A text run fragment derived from an InlineText box.
    TextRun,
    /// An inline-level element fragment (e.g., <span>).
    InlineBox,
}

/// A single fragment within a line box.
#[derive(Debug, Clone)]
pub struct Fragment {
    /// DOM node key this fragment corresponds to (for text runs: the text node).
    pub node: NodeKey,
    /// Kind of fragment.
    pub kind: FragmentKind,
    /// Optional geometry placeholder; populated by layout later.
    pub rect: Option<LayoutRect>,
}

/// A line box consisting of inline fragments.
#[derive(Debug, Clone)]
pub struct LineBox {
    /// Fragments within the line in visual order.
    pub fragments: Vec<Fragment>,
    /// Baseline position and metrics (MVP placeholders).
    pub baseline: i32,
    pub y: i32,
    pub height: i32,
}

/// A collection of lines produced for a block's inline formatting context.
#[derive(Debug, Clone)]
pub struct FragmentTree {
    pub lines: Vec<LineBox>,
}

impl FragmentTree {
    /// Convenience: total number of fragments across all lines.
    pub fn fragment_count(&self) -> usize { self.lines.iter().map(|l| l.fragments.len()).sum() }
}

/// Group consecutive inline-level child boxes under `container` into runs.
/// Returns a vec of runs, each run is a vec of LayoutBoxId.
pub fn group_inline_runs(tree: &LayoutBoxTree, container: LayoutBoxId) -> Vec<Vec<LayoutBoxId>> {
    let mut runs: Vec<Vec<LayoutBoxId>> = Vec::new();
    let Some(container_box) = tree.get(container) else { return runs; };
    let mut current: Vec<LayoutBoxId> = Vec::new();
    for &child_id in &container_box.children {
        let Some(child) = tree.get(child_id) else { continue; };
        match &child.kind {
            LayoutBoxKind::InlineText { .. } | LayoutBoxKind::InlineElement { .. } => {
                current.push(child_id);
            }
            LayoutBoxKind::AnonymousBlock => {
                if !current.is_empty() { runs.push(std::mem::take(&mut current)); }
                // Collect inline-level descendants directly under the anonymous block as a run
                let mut anon_run: Vec<LayoutBoxId> = Vec::new();
                if let Some(anon) = tree.get(child_id) {
                    for &grand_id in &anon.children {
                        if let Some(grand) = tree.get(grand_id) {
                            if matches!(grand.kind, LayoutBoxKind::InlineText { .. } | LayoutBoxKind::InlineElement { .. }) {
                                anon_run.push(grand_id);
                            }
                        }
                    }
                }
                if !anon_run.is_empty() { runs.push(anon_run); }
            }
            _ => {
                if !current.is_empty() { runs.push(std::mem::take(&mut current)); }
            }
        }
    }
    if !current.is_empty() { runs.push(current); }
    runs
}

/// Build a naive FragmentTree for the inline content of the block associated
/// with the provided DOM `container_node`. Inline children (text or inline-level
/// elements) are converted to one fragment each and placed on a single line.
pub fn build_inline_fragments_for_node(tree: &LayoutBoxTree, container_node: NodeKey) -> FragmentTree {
    // Resolve the container box id via reverse map (first box for this node)
    let Some(&container_id) = tree.node_to_box.get(&container_node) else { return FragmentTree { lines: Vec::new() }; };
    build_inline_fragments_for_box(tree, container_id)
}

/// Build a naive FragmentTree for the inline content of a block LayoutBox.
pub fn build_inline_fragments_for_box(tree: &LayoutBoxTree, container: LayoutBoxId) -> FragmentTree {
    let mut fragments: Vec<Fragment> = Vec::new();
    let Some(container_box) = tree.get(container) else { return FragmentTree { lines: Vec::new() }; };
    // Recursively collect inline text fragments from the subtree (MVP behavior)
    fn collect_text_fragments(tree: &LayoutBoxTree, id: LayoutBoxId, out: &mut Vec<Fragment>) {
        if let Some(bx) = tree.get(id) {
            for &child_id in &bx.children {
                if let Some(child) = tree.get(child_id) {
                    match &child.kind {
                        LayoutBoxKind::InlineText { .. } => {
                            if let Some(node) = child.dom_node {
                                out.push(Fragment { node, kind: FragmentKind::TextRun, rect: None });
                            }
                        }
                        LayoutBoxKind::InlineElement { .. } => {
                            // Do not emit a fragment for the inline element itself in the scaffold;
                            // only capture descendant text runs for now.
                            collect_text_fragments(tree, child_id, out);
                        }
                        _ => {
                            collect_text_fragments(tree, child_id, out);
                        }
                    }
                }
            }
        }
    }

    collect_text_fragments(tree, container, &mut fragments);
    FragmentTree { lines: vec![LineBox { fragments, baseline: 0, y: 0, height: 0 }] }
}
