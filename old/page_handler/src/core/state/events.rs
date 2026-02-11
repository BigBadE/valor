//! Input event dispatch and handling.

use super::HtmlPage;
use crate::input::events::KeyMods;
use crate::input::focus as focus_mod;
use js::NodeKey;

impl HtmlPage {
    /// Return the currently focused node, if any.
    #[inline]
    pub const fn focused_node(&self) -> Option<NodeKey> {
        self.focused_node
    }

    /// Set the focused node explicitly.
    #[inline]
    pub const fn focus_set(&mut self, node: Option<NodeKey>) {
        self.focused_node = node;
    }

    /// Move focus to the next focusable element using a basic tabindex order, then natural order fallback.
    #[inline]
    pub fn focus_next(&mut self) -> Option<NodeKey> {
        let snapshot = self.incremental_layout.snapshot();
        let attrs = self.incremental_layout.attrs_map();
        let next = focus_mod::next(&snapshot, attrs, self.focused_node);
        self.focused_node = next;
        next
    }

    /// Move focus to the previous focusable element.
    #[inline]
    pub fn focus_prev(&mut self) -> Option<NodeKey> {
        let snapshot = self.incremental_layout.snapshot();
        let attrs = self.incremental_layout.attrs_map();
        let prev = focus_mod::prev(&snapshot, attrs, self.focused_node);
        self.focused_node = prev;
        prev
    }

    /// Dispatch event methods (stubs for now)
    pub const fn dispatch_pointer_move(&mut self, _x: f64, _y: f64) {}
    pub const fn dispatch_pointer_down(&mut self, _x: f64, _y: f64, _button: u32) {}
    pub const fn dispatch_pointer_up(&mut self, _x: f64, _y: f64, _button: u32) {}
    pub const fn dispatch_key_down(&mut self, _key: &str, _code: &str, _mods: KeyMods) {}
    pub const fn dispatch_key_up(&mut self, _key: &str, _code: &str, _mods: KeyMods) {}
}
