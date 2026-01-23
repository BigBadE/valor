/// Scroll integration module for connecting layout to windowing system.
///
/// This module handles:
/// - Scroll event propagation
/// - Scroll container tracking
/// - Scroll position queries
/// - Integration with sticky positioning
///
/// This provides the bridge between the layout system and the windowing/event system.
use crate::{Subpixels, positioning::sticky::ScrollState};
use rewrite_core::{Input, NodeId};

/// Input for scroll position of a scrolling container.
///
/// Key is the NodeId of the scroll container (or () for viewport scroll).
/// Value is the ScrollState with current scroll offsets.
pub struct ScrollPositionInput;

impl Input for ScrollPositionInput {
    type Key = NodeId;
    type Value = ScrollState;

    fn name() -> &'static str {
        "ScrollPositionInput"
    }

    fn default_value(_key: &Self::Key) -> Self::Value {
        ScrollState {
            block_scroll: 0,
            inline_scroll: 0,
        }
    }
}

/// Input for viewport scroll position.
///
/// This tracks the global document scroll.
pub struct ViewportScrollInput;

impl Input for ViewportScrollInput {
    type Key = ();
    type Value = ScrollState;

    fn name() -> &'static str {
        "ViewportScrollInput"
    }

    fn default_value(_key: &Self::Key) -> Self::Value {
        ScrollState {
            block_scroll: 0,
            inline_scroll: 0,
        }
    }
}

/// Scroll event from the windowing system.
#[derive(Debug, Clone, Copy)]
pub struct ScrollEvent {
    /// The container that was scrolled (None for viewport).
    pub container: Option<NodeId>,
    /// New scroll position in the block direction.
    pub block_scroll: Subpixels,
    /// New scroll position in the inline direction.
    pub inline_scroll: Subpixels,
    /// Scroll delta in the block direction.
    pub block_delta: Subpixels,
    /// Scroll delta in the inline direction.
    pub inline_delta: Subpixels,
}

impl ScrollEvent {
    /// Create a scroll event for viewport scrolling.
    pub fn viewport(block_scroll: Subpixels, inline_scroll: Subpixels) -> Self {
        Self {
            container: None,
            block_scroll,
            inline_scroll,
            block_delta: 0,
            inline_delta: 0,
        }
    }

    /// Create a scroll event with deltas.
    pub fn with_delta(
        container: Option<NodeId>,
        block_scroll: Subpixels,
        inline_scroll: Subpixels,
        block_delta: Subpixels,
        inline_delta: Subpixels,
    ) -> Self {
        Self {
            container,
            block_scroll,
            inline_scroll,
            block_delta,
            inline_delta,
        }
    }

    /// Get the scroll state from this event.
    pub fn to_scroll_state(&self) -> ScrollState {
        ScrollState {
            block_scroll: self.block_scroll,
            inline_scroll: self.inline_scroll,
        }
    }
}

/// Scroll container bounds for clamping scroll position.
#[derive(Debug, Clone, Copy)]
pub struct ScrollBounds {
    /// Maximum scroll in the block direction.
    pub max_block_scroll: Subpixels,
    /// Maximum scroll in the inline direction.
    pub max_inline_scroll: Subpixels,
}

impl ScrollBounds {
    /// Create scroll bounds from content and viewport sizes.
    pub fn from_sizes(
        content_block_size: Subpixels,
        content_inline_size: Subpixels,
        viewport_block_size: Subpixels,
        viewport_inline_size: Subpixels,
    ) -> Self {
        Self {
            max_block_scroll: (content_block_size - viewport_block_size).max(0),
            max_inline_scroll: (content_inline_size - viewport_inline_size).max(0),
        }
    }

    /// Clamp a scroll state to these bounds.
    pub fn clamp(&self, scroll: ScrollState) -> ScrollState {
        ScrollState {
            block_scroll: scroll.block_scroll.clamp(0, self.max_block_scroll),
            inline_scroll: scroll.inline_scroll.clamp(0, self.max_inline_scroll),
        }
    }

    /// Check if a scroll position is within bounds.
    pub fn contains(&self, scroll: &ScrollState) -> bool {
        scroll.block_scroll >= 0
            && scroll.block_scroll <= self.max_block_scroll
            && scroll.inline_scroll >= 0
            && scroll.inline_scroll <= self.max_inline_scroll
    }
}

/// Scroll behavior mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollBehavior {
    /// Instant scrolling (no animation).
    Auto,
    /// Smooth scrolling (animated).
    Smooth,
}

/// Scroll into view alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollAlignment {
    /// Align to start edge.
    Start,
    /// Align to center.
    Center,
    /// Align to end edge.
    End,
    /// Align to nearest edge (minimal scroll).
    Nearest,
}

/// Request to scroll an element into view.
#[derive(Debug, Clone, Copy)]
pub struct ScrollIntoViewRequest {
    /// The element to scroll into view.
    pub element: NodeId,
    /// Vertical alignment.
    pub block_alignment: ScrollAlignment,
    /// Horizontal alignment.
    pub inline_alignment: ScrollAlignment,
    /// Scroll behavior (smooth vs instant).
    pub behavior: ScrollBehavior,
}

impl ScrollIntoViewRequest {
    /// Create a basic scroll into view request (align to start, instant).
    pub fn new(element: NodeId) -> Self {
        Self {
            element,
            block_alignment: ScrollAlignment::Start,
            inline_alignment: ScrollAlignment::Nearest,
            behavior: ScrollBehavior::Auto,
        }
    }

    /// Set block alignment.
    pub fn with_block_alignment(mut self, alignment: ScrollAlignment) -> Self {
        self.block_alignment = alignment;
        self
    }

    /// Set inline alignment.
    pub fn with_inline_alignment(mut self, alignment: ScrollAlignment) -> Self {
        self.inline_alignment = alignment;
        self
    }

    /// Set scroll behavior.
    pub fn with_behavior(mut self, behavior: ScrollBehavior) -> Self {
        self.behavior = behavior;
        self
    }
}

/// Calculate the scroll position needed to bring an element into view.
///
/// This computes the scroll offset required for a scroll container to make
/// an element visible according to the specified alignment.
pub fn calculate_scroll_into_view(
    element_offset: Subpixels,
    element_size: Subpixels,
    container_scroll: Subpixels,
    container_size: Subpixels,
    alignment: ScrollAlignment,
) -> Subpixels {
    let element_start = element_offset;
    let element_end = element_offset + element_size;
    let viewport_start = container_scroll;
    let viewport_end = container_scroll + container_size;

    match alignment {
        ScrollAlignment::Start => {
            // Align element start to container start
            element_start
        }
        ScrollAlignment::Center => {
            // Align element center to container center
            element_start - (container_size - element_size) / 2
        }
        ScrollAlignment::End => {
            // Align element end to container end
            element_end - container_size
        }
        ScrollAlignment::Nearest => {
            // Minimal scroll to make element visible
            if element_start < viewport_start {
                // Element is above viewport, scroll up
                element_start
            } else if element_end > viewport_end {
                // Element is below viewport, scroll down
                element_end - container_size
            } else {
                // Element is already visible, don't scroll
                container_scroll
            }
        }
    }
}

/// Integration with sticky positioning.
///
/// This updates the sticky positioning module with current scroll state.
pub fn update_sticky_scroll_state(container: Option<NodeId>, scroll_state: ScrollState) {
    // This would update the scroll state input that sticky positioning reads from
    // The actual implementation would use the database's set_input method
}

/// Check if a scroll container can scroll in a given direction.
pub fn can_scroll(current_scroll: Subpixels, max_scroll: Subpixels, delta: Subpixels) -> bool {
    if delta > 0 {
        // Scrolling forward/down
        current_scroll < max_scroll
    } else if delta < 0 {
        // Scrolling backward/up
        current_scroll > 0
    } else {
        false
    }
}

/// Apply scroll delta with momentum/easing.
///
/// This implements smooth scrolling by applying easing to scroll deltas.
pub fn apply_scroll_momentum(
    current_scroll: Subpixels,
    target_scroll: Subpixels,
    easing_factor: f32, // 0.0 to 1.0, typically 0.1-0.3 for smooth feel
) -> Subpixels {
    let delta = target_scroll - current_scroll;
    let eased_delta = (delta as f32 * easing_factor) as Subpixels;
    current_scroll + eased_delta
}

/// Scroll state tracker for animations.
#[derive(Debug, Clone, Copy)]
pub struct ScrollAnimation {
    /// Starting scroll position.
    pub start: ScrollState,
    /// Target scroll position.
    pub target: ScrollState,
    /// Animation progress (0.0 to 1.0).
    pub progress: f32,
    /// Animation duration in milliseconds.
    pub duration_ms: f32,
    /// Easing function.
    pub easing: EasingFunction,
}

/// Easing function for scroll animations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EasingFunction {
    /// Linear easing (constant speed).
    Linear,
    /// Ease-in (slow start).
    EaseIn,
    /// Ease-out (slow end).
    EaseOut,
    /// Ease-in-out (slow start and end).
    EaseInOut,
}

impl ScrollAnimation {
    /// Create a new scroll animation.
    pub fn new(start: ScrollState, target: ScrollState, duration_ms: f32) -> Self {
        Self {
            start,
            target,
            progress: 0.0,
            duration_ms,
            easing: EasingFunction::EaseOut,
        }
    }

    /// Update animation progress (call each frame).
    pub fn update(&mut self, delta_ms: f32) {
        self.progress = (self.progress + delta_ms / self.duration_ms).min(1.0);
    }

    /// Get current scroll position based on animation progress.
    pub fn current_position(&self) -> ScrollState {
        let t = self.apply_easing(self.progress);

        ScrollState {
            block_scroll: interpolate(self.start.block_scroll, self.target.block_scroll, t),
            inline_scroll: interpolate(self.start.inline_scroll, self.target.inline_scroll, t),
        }
    }

    /// Check if animation is complete.
    pub fn is_complete(&self) -> bool {
        self.progress >= 1.0
    }

    /// Apply easing function to progress value.
    fn apply_easing(&self, t: f32) -> f32 {
        match self.easing {
            EasingFunction::Linear => t,
            EasingFunction::EaseIn => t * t,
            EasingFunction::EaseOut => t * (2.0 - t),
            EasingFunction::EaseInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    -1.0 + (4.0 - 2.0 * t) * t
                }
            }
        }
    }
}

/// Interpolate between two values.
fn interpolate(start: Subpixels, end: Subpixels, t: f32) -> Subpixels {
    start + ((end - start) as f32 * t) as Subpixels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scroll_bounds_clamp() {
        let bounds = ScrollBounds {
            max_block_scroll: 1000,
            max_inline_scroll: 500,
        };

        let scroll = ScrollState {
            block_scroll: 1500,
            inline_scroll: -100,
        };

        let clamped = bounds.clamp(scroll);
        assert_eq!(clamped.block_scroll, 1000);
        assert_eq!(clamped.inline_scroll, 0);
    }

    #[test]
    fn test_scroll_into_view_nearest() {
        // Element already visible
        let scroll = calculate_scroll_into_view(
            500, // element offset
            100, // element size
            400, // current scroll
            200, // viewport size
            ScrollAlignment::Nearest,
        );
        assert_eq!(scroll, 400); // No scroll needed

        // Element below viewport
        let scroll = calculate_scroll_into_view(
            700, // element offset
            100, // element size
            400, // current scroll
            200, // viewport size
            ScrollAlignment::Nearest,
        );
        assert_eq!(scroll, 600); // Scroll to show bottom
    }
}
