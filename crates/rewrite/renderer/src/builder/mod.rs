//! Display list builder.
//!
//! This module builds display lists from layout information by traversing
//! the DOM tree and emitting rendering commands.

mod background;
mod border;
mod content;
mod culling;
mod effects;
mod helpers;

use rewrite_core::{Axis, Database, Edge, NodeId};

use crate::DisplayList;

/// Display list builder that converts layout tree to rendering commands.
pub struct DisplayListBuilder<'a> {
    db: &'a Database,
    viewport_width: f32,
    viewport_height: f32,
    display_list: Vec<DisplayList>,
}

impl<'a> DisplayListBuilder<'a> {
    /// Create a new display list builder.
    pub fn new(db: &'a Database, viewport_width: f32, viewport_height: f32) -> Self {
        Self {
            db,
            viewport_width,
            viewport_height,
            display_list: Vec::new(),
        }
    }

    /// Build the display list starting from a root node.
    pub fn build(mut self, root: NodeId) -> Vec<DisplayList> {
        self.traverse(root);
        self.display_list
    }

    /// Traverse the tree and emit display commands.
    fn traverse(&mut self, node: NodeId) {
        // Check if node should be rendered (visibility + culling)
        if !culling::should_render(self.db, node, self.viewport_width, self.viewport_height) {
            return;
        }

        // Get node position and size
        let x = helpers::get_offset(self.db, node, Edge::Left);
        let y = helpers::get_offset(self.db, node, Edge::Top);
        let width = helpers::get_size(self.db, node, Axis::Horizontal);
        let height = helpers::get_size(self.db, node, Axis::Vertical);

        // Push stacking context if needed
        let stacking_pushed = effects::push_stacking_context(self.db, node, &mut self.display_list);

        // Push transforms if any
        let transform_pushed = effects::push_transforms(self.db, node, &mut self.display_list);

        // Push filters if any
        let filter_pushed = effects::render_filters(self.db, node, &mut self.display_list);

        // Push opacity if not 1.0
        let opacity_pushed = effects::push_opacity(self.db, node, &mut self.display_list);

        // Push clip if overflow is not visible
        let clip_pushed =
            effects::push_clip(self.db, node, x, y, width, height, &mut self.display_list);

        // Render box shadow (behind content)
        effects::render_box_shadow(self.db, node, x, y, width, height, &mut self.display_list);

        // Render background
        background::render_background(self.db, node, x, y, width, height, &mut self.display_list);

        // Render border
        border::render_border(self.db, node, x, y, width, height, &mut self.display_list);

        // Render content (text, images, etc.)
        content::render_content(self.db, node, x, y, width, height, &mut self.display_list);

        // Render text decorations
        content::render_text_decoration(self.db, node, x, y, width, &mut self.display_list);

        // Render text shadow
        content::render_text_shadow(self.db, node, x, y, &mut self.display_list);

        // Render list marker
        content::render_list_marker(self.db, node, x, y, &mut self.display_list);

        // Traverse children
        let children = self
            .db
            .resolve_relationship(node, rewrite_core::MultiRelationship::Children);
        for child in children {
            self.traverse(child);
        }

        // Pop clip
        effects::pop_clip(clip_pushed, &mut self.display_list);

        // Pop opacity
        effects::pop_opacity(opacity_pushed, &mut self.display_list);

        // Pop filters
        effects::pop_filters(filter_pushed, &mut self.display_list);

        // Pop transforms
        effects::pop_transforms(transform_pushed, &mut self.display_list);

        // Pop stacking context
        effects::pop_stacking_context(stacking_pushed, &mut self.display_list);

        // Render outline (after everything else)
        border::render_outline(self.db, node, x, y, width, height, &mut self.display_list);

        // Render focus ring if focused
        content::render_focus_ring(self.db, node, x, y, width, height, &mut self.display_list);
    }
}

/// Helper to build a display list from a layout tree.
pub fn build_display_list(
    db: &Database,
    root: NodeId,
    viewport_width: f32,
    viewport_height: f32,
) -> Vec<DisplayList> {
    let builder = DisplayListBuilder::new(db, viewport_width, viewport_height);
    builder.build(root)
}
