//! Block layout algorithm using constraint space propagation.
//!
//! This is the Chromium-like top-down layout approach that replaces
//! the reactive convergence model.

use super::constraint_space::{ConstraintSpace, LayoutResult};
use super::margin_strut::MarginStrut;
use css_box::{LayoutUnit, compute_box_sides};
use css_orchestrator::style_model::{ComputedStyle, Display, Float, Overflow, Position};
use js::NodeKey;
use std::collections::HashMap;

// Shared types and parameters
mod shared;
use shared::BlockLayoutParams;

// Sub-modules organized by functionality
mod absolute;
mod core;
mod flex_abspos;
mod flex_basis;
mod flex_layout;
mod float;
mod grid;
mod margin;
mod measurement;
mod sizing;
mod text;

/// Layout tree for constraint-based layout.
pub struct ConstraintLayoutTree {
    /// Computed styles per node
    pub styles: HashMap<NodeKey, ComputedStyle>,

    /// Children in DOM order per parent
    pub children: HashMap<NodeKey, Vec<NodeKey>>,

    /// Text content for text nodes
    pub text_nodes: HashMap<NodeKey, String>,

    /// Element tag names
    pub tags: HashMap<NodeKey, String>,

    /// Element attributes
    pub attrs: HashMap<NodeKey, HashMap<String, String>>,

    /// Initial containing block dimensions
    pub icb_width: LayoutUnit,
    pub icb_height: LayoutUnit,

    /// Final layout results (computed during layout)
    pub layout_results: HashMap<NodeKey, LayoutResult>,
}

impl ConstraintLayoutTree {
    /// Create a new constraint layout tree.
    pub fn new(icb_width: LayoutUnit, icb_height: LayoutUnit) -> Self {
        Self {
            styles: HashMap::new(),
            children: HashMap::new(),
            text_nodes: HashMap::new(),
            tags: HashMap::new(),
            attrs: HashMap::new(),
            icb_width,
            icb_height,
            layout_results: HashMap::new(),
        }
    }

    /// Get computed style for a node.
    pub fn style(&self, node: NodeKey) -> ComputedStyle {
        self.styles.get(&node).cloned().unwrap_or_default()
    }

    /// Check if node is a text node.
    pub(super) fn is_text_node(&self, node: NodeKey) -> bool {
        self.text_nodes.contains_key(&node)
    }

    /// Check if node establishes a BFC.
    pub fn establishes_bfc(&self, node: NodeKey) -> bool {
        let style = self.style(node);

        // Floats establish BFC
        if !matches!(style.float, Float::None) {
            return true;
        }

        // Overflow other than visible establishes BFC
        if !matches!(style.overflow, Overflow::Visible) {
            return true;
        }

        // Flex/grid containers establish BFC
        if matches!(
            style.display,
            Display::Flex | Display::InlineFlex | Display::Grid | Display::InlineGrid
        ) {
            return true;
        }

        // Absolutely positioned elements establish BFC
        if matches!(style.position, Position::Absolute | Position::Fixed) {
            return true;
        }

        false
    }

    /// Layout a block-level element.
    ///
    /// This is the main entry point for constraint-based layout.
    pub fn layout_block(
        &mut self,
        node: NodeKey,
        constraint_space: &ConstraintSpace,
    ) -> LayoutResult {
        log::error!("layout_block ENTRY: node={:?}", node);

        // Skip text nodes - they don't have boxes
        if self.is_text_node(node) {
            return LayoutResult {
                inline_size: 0.0,
                block_size: 0.0,
                bfc_offset: constraint_space.bfc_offset,
                exclusion_space: constraint_space.exclusion_space.clone(),
                end_margin_strut: MarginStrut::default(),
                baseline: None,
                needs_relayout: false,
            };
        }

        let style = self.style(node);

        // Handle display: none - these don't participate in layout
        if matches!(style.display, Display::None) {
            return LayoutResult {
                inline_size: 0.0,
                block_size: 0.0,
                bfc_offset: constraint_space.bfc_offset,
                exclusion_space: constraint_space.exclusion_space.clone(),
                end_margin_strut: MarginStrut::default(),
                baseline: None,
                needs_relayout: false,
            };
        }

        // Handle display: contents - these don't generate boxes themselves
        // Their children are lifted by normalize_children, so this should not be reached
        if matches!(style.display, Display::Contents) {
            return LayoutResult {
                inline_size: 0.0,
                block_size: 0.0,
                bfc_offset: constraint_space.bfc_offset,
                exclusion_space: constraint_space.exclusion_space.clone(),
                end_margin_strut: MarginStrut::default(),
                baseline: None,
                needs_relayout: false,
            };
        }

        let sides = compute_box_sides(&style);

        // Check if this establishes a new BFC
        let establishes_bfc = self.establishes_bfc(node);

        // Handle floats
        if !matches!(style.float, Float::None) {
            let result = self.layout_float(node, constraint_space, &style, &sides);
            self.layout_results.insert(node, result.clone());
            return result;
        }

        // Handle flex containers (both block and inline)
        log::error!(
            "Checking flex: node={:?}, display={:?}",
            node,
            style.display
        );
        if matches!(style.display, Display::Flex | Display::InlineFlex) {
            log::error!("IS FLEX!");
            let result = self.layout_flex(node, constraint_space, &style, &sides);
            log::error!(
                "INSERTING FLEX CONTAINER in layout_block: node={:?}, inline_size={}, is_measurement={}",
                node,
                result.inline_size,
                constraint_space.is_for_measurement_only
            );
            self.layout_results.insert(node, result.clone());
            return result;
        }

        // Handle grid containers (both block and inline)
        if matches!(style.display, Display::Grid | Display::InlineGrid) {
            let result = self.layout_grid_container(node, constraint_space, &style, &sides);
            self.layout_results.insert(node, result.clone());
            return result;
        }

        // Handle absolutely positioned
        if matches!(style.position, Position::Absolute | Position::Fixed) {
            let result = self.layout_absolute(node, constraint_space, &style, &sides);
            self.layout_results.insert(node, result.clone());
            return result;
        }

        // Compute inline size (width)
        let inline_size = self.compute_inline_size(node, constraint_space, &style, &sides);

        // Resolve BFC offset for this box
        let (bfc_offset, needs_two_pass) =
            Self::resolve_bfc_offset(constraint_space, &style, &sides, establishes_bfc);

        // If we need two-pass layout (margin collapsing uncertainty), mark it
        if needs_two_pass && !bfc_offset.is_resolved() {
            // First pass: estimate BFC offset for now
            let params = BlockLayoutParams {
                constraint_space,
                style: &style,
                sides: &sides,
                inline_size,
                bfc_offset,
                establishes_bfc,
            };
            let result = self.layout_block_first_pass(node, &params);
            self.layout_results.insert(node, result.clone());
            return result;
        }

        // Single-pass or second-pass layout
        let params = BlockLayoutParams {
            constraint_space,
            style: &style,
            sides: &sides,
            inline_size,
            bfc_offset,
            establishes_bfc,
        };
        let result = self.layout_block_children(node, &params);
        self.layout_results.insert(node, result.clone());
        result
    }
}

/// Run layout on the entire tree starting from root.
pub fn layout_tree(tree: &mut ConstraintLayoutTree, root: NodeKey) -> LayoutResult {
    let initial_space = ConstraintSpace::new_for_root(tree.icb_width, tree.icb_height);
    let result = tree.layout_block(root, &initial_space);

    // Store layout result for root element (html/body) so it gets proper rect
    // Note: We don't force viewport height - the root element should size to content
    // unless explicitly given height:100% or similar. This matches Chrome's behavior
    // where getBoundingClientRect() returns actual content size.
    tree.layout_results.insert(root, result.clone());

    result
}
