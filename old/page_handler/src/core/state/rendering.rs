//! Page rendering and display management.

use super::HtmlPage;
use core::mem::replace;
use css_core::LayoutRect;
use renderer::DisplayList;

impl HtmlPage {
    /// Return whether a redraw is needed since the last call and clear the flag.
    pub const fn take_needs_redraw(&mut self) -> bool {
        replace(&mut self.render.needs_redraw, false)
    }

    /// Get the background color from the page's computed styles.
    pub const fn background_rgba(&self) -> [f32; 4] {
        [1.0, 1.0, 1.0, 1.0]
    }

    /// Get a retained snapshot of the display list.
    pub fn display_list_retained_snapshot(&mut self) -> DisplayList {
        super::accessors::display_list_retained_snapshot(
            &mut self.renderer_mirror,
            &mut self.incremental_layout,
        )
    }

    /// Set the current text selection overlay rectangle in viewport coordinates.
    #[inline]
    pub const fn selection_set(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        self.selection_overlay = Some((x0, y0, x1, y1));
    }

    /// Clear any active text selection overlay.
    #[inline]
    pub const fn selection_clear(&mut self) {
        self.selection_overlay = None;
    }

    /// Return a list of selection rectangles by intersecting inline text boxes with a selection rect.
    #[inline]
    pub fn selection_rects(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<LayoutRect> {
        super::accessors::selection_rects(&mut self.incremental_layout, x0, y0, x1, y1)
    }

    /// Compute a caret rectangle at the given point: a thin bar within the inline text box, if any.
    #[inline]
    pub fn caret_at(&mut self, x: i32, y: i32) -> Option<LayoutRect> {
        super::accessors::caret_at(&mut self.incremental_layout, x, y)
    }
}
