//! Block layout algorithm using constraint space propagation.
//!
//! This is the Chromium-like top-down layout approach that replaces
//! the reactive convergence model.

use super::constraint_space::{AvailableSize, BfcOffset, ConstraintSpace, LayoutResult};
use super::exclusion_space::{ExclusionSpace, FloatSize};
use super::margin_strut::MarginStrut;
use css_box::{BoxSides, compute_box_sides};
use css_orchestrator::style_model::{
    AlignContent as StyleAlignContent, AlignItems as StyleAlignItems, BoxSizing, Clear,
    ComputedStyle, Display, FlexDirection as StyleFlexDirection, FlexWrap as StyleFlexWrap, Float,
    JustifyContent as StyleJustifyContent, Overflow, Position,
};
use js::NodeKey;
use std::collections::HashMap;

// Import flexbox module types and functions
use css_flexbox::{
    AlignContent as FlexAlignContent, AlignItems as FlexAlignItems, CrossAndBaseline, CrossContext,
    CrossPlacement, FlexChild, FlexContainerInputs, FlexDirection as FlexboxDirection,
    FlexPlacement, ItemRef, JustifyContent as FlexJustifyContent, WritingMode,
    layout_multi_line_with_cross, layout_single_line_with_cross,
};

// Import text measurement module for actual font metrics
use css_text::default_line_height_px;
use css_text::measurement::{measure_text, measure_text_wrapped};

// Import display module for normalize_children (handles display:none and display:contents)
use css_display::normalize_children;

/// Parameters for block layout operations (to avoid too many arguments).
struct BlockLayoutParams<'params> {
    constraint_space: &'params ConstraintSpace,
    style: &'params ComputedStyle,
    sides: &'params BoxSides,
    inline_size: f32,
    bfc_offset: BfcOffset,
    establishes_bfc: bool,
}

use css_box::LayoutUnit;

/// State tracked during children layout.
struct ChildrenLayoutState {
    max_block_size: f32,
    has_text_content: bool,
    last_child_end_margin_strut: MarginStrut,
    first_inflow_child_seen: bool,
    resolved_bfc_offset: BfcOffset,
}

impl ChildrenLayoutState {
    fn new(bfc_offset: BfcOffset) -> Self {
        Self {
            max_block_size: 0.0,
            has_text_content: false,
            last_child_end_margin_strut: MarginStrut::default(),
            first_inflow_child_seen: false,
            resolved_bfc_offset: bfc_offset,
        }
    }
}

/// Type alias for child style information tuple.
type ChildStyleInfo = (NodeKey, ComputedStyle, LayoutResult);

/// Type alias for flex layout result.
type FlexLayoutResult = (Vec<FlexChild>, Vec<ChildStyleInfo>, Vec<NodeKey>);

/// Parameters for computing block size from children.
struct BlockSizeParams<'exclusion> {
    style_height: Option<f32>,
    can_collapse_with_children: bool,
    establishes_bfc: bool,
    resolved_bfc_offset: BfcOffset,
    bfc_offset: BfcOffset,
    child_space_bfc_offset: Option<LayoutUnit>,
    has_text_content: bool,
    exclusion_space: &'exclusion ExclusionSpace,
    last_child_end_margin_strut: MarginStrut,
}

/// Parameters for applying flex placements.
struct FlexPlacementParams {
    content_base_inline: LayoutUnit,
    content_base_block: Option<LayoutUnit>,
    is_row: bool,
}

/// Parameters for creating flex result.
struct FlexResultParams {
    container_inline_size: f32,
    final_cross_size: f32,
    is_row: bool,
    container_style: ComputedStyle,
}

/// Parameters for running flexbox layout.
struct FlexboxLayoutParams {
    container_inputs: FlexContainerInputs,
    is_row: bool,
    container_inline_size: f32,
    container_cross_size: f32,
}

/// Parameters for computing end margin strut.
struct EndMarginStrutParams<'params> {
    sides: &'params BoxSides,
    state: &'params ChildrenLayoutState,
    incoming_space: &'params ConstraintSpace,
    can_collapse_with_children: bool,
    block_size: f32,
}

/// Parameters for creating flex abspos constraint space.
struct FlexAbsposConstraintParams<'params> {
    bfc_offset: BfcOffset,
    sides: &'params BoxSides,
    container_style: &'params ComputedStyle,
    child_style: &'params ComputedStyle,
    child_sides: &'params BoxSides,
    container_inline_size: f32,
    final_cross_size: f32,
    is_row: bool,
}

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
        if matches!(style.display, Display::Flex) {
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
            return self.layout_float(node, constraint_space, &style, &sides);
        }

        // Handle flex containers
        if matches!(style.display, Display::Flex) {
            return self.layout_flex(node, constraint_space, &style, &sides);
        }

        // Handle absolutely positioned
        if matches!(style.position, Position::Absolute | Position::Fixed) {
            return self.layout_absolute(node, constraint_space, &style, &sides);
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
            return self.layout_block_first_pass(node, &params);
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
        self.layout_block_children(node, &params)
    }

    /// Apply min/max width constraints to a border-box width.
    ///
    /// Per CSS Sizing spec, min/max constraints are applied AFTER width computation
    /// and ALWAYS in border-box space.
    fn apply_width_constraints(
        border_box_width: f32,
        style: &ComputedStyle,
        _sides: &BoxSides,
    ) -> f32 {
        let mut result = border_box_width;

        // Apply min-width constraint (border-box)
        if let Some(min_width) = style.min_width {
            result = result.max(min_width);
        }

        // Apply max-width constraint (border-box)
        if let Some(max_width) = style.max_width {
            result = result.min(max_width);
        }

        result
    }

    /// Apply min/max height constraints to a border-box height.
    ///
    /// Per CSS Sizing spec, min/max constraints are applied AFTER height computation
    /// and ALWAYS in border-box space.
    fn apply_height_constraints(
        border_box_height: f32,
        style: &ComputedStyle,
        _sides: &BoxSides,
    ) -> f32 {
        let mut result = border_box_height;

        // Apply min-height constraint (border-box)
        if let Some(min_height) = style.min_height {
            result = result.max(min_height);
        }

        // Apply max-height constraint (border-box)
        if let Some(max_height) = style.max_height {
            result = result.min(max_height);
        }

        result
    }

    /// Compute intrinsic width for form controls.
    ///
    /// Returns content-box width in pixels, or None if element doesn't have intrinsic width.
    fn compute_form_control_intrinsic_width(&self, node: NodeKey) -> Option<f32> {
        let tag = self.tags.get(&node)?;
        let tag_lower = tag.to_lowercase();

        match tag_lower.as_str() {
            "input" => {
                // Checkboxes and radios have intrinsic 13x13 size
                if let Some(attrs) = self.attrs.get(&node)
                    && let Some(input_type) = attrs.get("type")
                {
                    let type_lower = input_type.to_lowercase();
                    if type_lower == "checkbox" || type_lower == "radio" {
                        return Some(13.0);
                    }
                }
                // Other inputs don't have intrinsic width (they respect CSS width)
                None
            }
            "button" => {
                // Buttons shrink-wrap their content, no intrinsic width
                // This allows them to size based on their text content
                None
            }
            _ => None,
        }
    }

    /// Compute intrinsic height for form controls.
    ///
    /// Returns content-box height in pixels, or None if element doesn't have intrinsic height.
    fn compute_form_control_intrinsic_height(
        &self,
        node: NodeKey,
        style: &ComputedStyle,
    ) -> Option<f32> {
        let tag = self.tags.get(&node)?;
        let tag_lower = tag.to_lowercase();

        match tag_lower.as_str() {
            "input" => {
                if let Some(attrs) = self.attrs.get(&node)
                    && let Some(input_type) = attrs.get("type")
                {
                    let type_lower = input_type.to_lowercase();
                    if type_lower == "checkbox" || type_lower == "radio" {
                        // Checkboxes and radios have intrinsic 13x13 size
                        return Some(13.0);
                    }
                }
                // Text inputs: intrinsic height based on font-size + a bit of spacing
                // Chrome uses approximately: font-size * 1.2 (line-height) + small buffer
                let font_size = if style.font_size > 0.0 {
                    style.font_size
                } else {
                    14.0
                };
                // Use similar calculation to Chrome: ~1.2x font-size for line-height
                let line_height = (font_size * 1.2).round();
                Some(line_height)
            }
            "button" => {
                // Buttons: intrinsic height based on font line-height
                let font_size = if style.font_size > 0.0 {
                    style.font_size
                } else {
                    16.0
                };
                let line_height = (font_size * 1.2).round();
                Some(line_height)
            }
            "textarea" => {
                // Textareas don't have intrinsic height, they respect CSS height
                None
            }
            _ => None,
        }
    }

    /// Compute inline size (width) for a block.
    ///
    /// Returns the content-box width (not including padding/border).
    fn compute_inline_size(
        &self,
        node: NodeKey,
        constraint_space: &ConstraintSpace,
        style: &ComputedStyle,
        sides: &BoxSides,
    ) -> f32 {
        // Compute content-box width first
        let content_width = style.width.map_or_else(
            || {
                // Check for form control intrinsic width first
                if let Some(intrinsic_width) = self.compute_form_control_intrinsic_width(node) {
                    return intrinsic_width;
                }

                // Auto width: fill available space
                let available = match constraint_space.available_inline_size {
                    AvailableSize::Definite(size) => size.to_px(),
                    _ => self.icb_width.to_px(),
                };

                // For block boxes, width is available minus horizontal margins/padding/border
                let horizontal_edges = (sides.margin_left
                    + sides.padding_left
                    + sides.border_left
                    + sides.border_right
                    + sides.padding_right
                    + sides.margin_right)
                    .to_px();

                (available - horizontal_edges).max(0.0)
            },
            |width| {
                // Check box-sizing property
                match style.box_sizing {
                    BoxSizing::BorderBox => {
                        // width includes padding and border, subtract them to get content-box
                        let padding_border = (sides.padding_left
                            + sides.padding_right
                            + sides.border_left
                            + sides.border_right)
                            .to_px();
                        (width - padding_border).max(0.0)
                    }
                    BoxSizing::ContentBox => {
                        // width is already content-box
                        width
                    }
                }
            },
        );

        // Convert to border-box for constraint application
        let padding_border =
            (sides.padding_left + sides.padding_right + sides.border_left + sides.border_right)
                .to_px();
        let border_box_width = content_width + padding_border;

        // Apply min/max constraints in border-box space
        let constrained_border_box = Self::apply_width_constraints(border_box_width, style, sides);

        // Convert back to content-box
        (constrained_border_box - padding_border).max(0.0)
    }

    /// Resolve BFC offset for this box.
    ///
    /// Returns (`BfcOffset`, `needs_two_pass`).
    fn resolve_bfc_offset(
        constraint_space: &ConstraintSpace,
        style: &ComputedStyle,
        sides: &BoxSides,
        establishes_bfc: bool,
    ) -> (BfcOffset, bool) {
        // Inline offset always includes left margin (margins don't collapse in inline direction)
        let inline_offset = constraint_space.bfc_offset.inline_offset + sides.margin_left;

        // If we establish a new BFC, we don't participate in parent's margin collapsing
        // Resolve any accumulated margins and add our own margin
        if establishes_bfc {
            let block_offset = constraint_space
                .bfc_offset
                .block_offset
                .map(|offset| offset + constraint_space.margin_strut.collapse() + sides.margin_top);

            return (BfcOffset::new(inline_offset, block_offset), false);
        }

        // Check for clearance
        let clearance_floor = constraint_space
            .exclusion_space
            .clearance_offset(style.clear);

        // If clearance is needed, resolve margins and compute clearance
        if clearance_floor > LayoutUnit::zero() {
            // Clearance prevents margin collapsing, so we resolve accumulated margins
            let base_offset = constraint_space
                .bfc_offset
                .block_offset
                .unwrap_or(LayoutUnit::zero())
                + constraint_space.margin_strut.collapse();

            // Element's border-box top would naturally be at: base + margin_top
            // Clearance pushes it to at least clearance_floor
            // The final position is the max of these two
            let block_offset = (base_offset + sides.margin_top).max(clearance_floor);

            return (BfcOffset::new(inline_offset, Some(block_offset)), false);
        }

        // Check if we can collapse margins with parent
        let can_collapse_top =
            sides.padding_top == LayoutUnit::zero() && sides.border_top == LayoutUnit::zero();

        // If parent established a new BFC and there are no sibling margins to collapse with,
        // margins don't collapse - add them as spacing
        if constraint_space.is_new_formatting_context && constraint_space.margin_strut.is_empty() {
            let block_offset = constraint_space
                .bfc_offset
                .block_offset
                .map(|offset| offset + sides.margin_top);
            return (BfcOffset::new(inline_offset, block_offset), false);
        }

        if can_collapse_top {
            // We can collapse top margins (no border/padding at top)
            // Add our top margin to the strut and resolve to find our border-box position
            let mut strut = constraint_space.margin_strut;
            strut.append(sides.margin_top);
            let collapsed = strut.collapse();
            let block_offset = constraint_space
                .bfc_offset
                .block_offset
                .map(|offset| offset + collapsed);
            return (BfcOffset::new(inline_offset, block_offset), false);
        }

        // Cannot collapse top margins with parent (have padding/border)
        // But we still need to collapse our top margin with any accumulated sibling margins
        let mut strut = constraint_space.margin_strut;
        strut.append(sides.margin_top);
        let collapsed = strut.collapse();
        let block_offset = constraint_space
            .bfc_offset
            .block_offset
            .map(|offset| offset + collapsed);

        (BfcOffset::new(inline_offset, block_offset), false)
    }

    /// First-pass layout when BFC offset is uncertain.
    fn layout_block_first_pass(
        &mut self,
        node: NodeKey,
        params: &BlockLayoutParams,
    ) -> LayoutResult {
        // Estimate BFC offset (will be corrected in second pass)
        let estimated_offset = params
            .constraint_space
            .bfc_offset
            .block_offset
            .unwrap_or(LayoutUnit::zero())
            + params.sides.margin_top;

        let estimated_bfc_offset = BfcOffset::new(
            params.constraint_space.bfc_offset.inline_offset,
            Some(estimated_offset),
        );

        // Layout children with estimated offset
        let estimated_params = BlockLayoutParams {
            bfc_offset: estimated_bfc_offset,
            ..*params
        };
        let result = self.layout_block_children(node, &estimated_params);

        // Mark that we need relayout once we know actual BFC offset
        LayoutResult {
            needs_relayout: true,
            ..result
        }
    }

    /// Compute the initial margin strut for child layout.
    fn compute_initial_margin_strut(
        _constraint_space: &ConstraintSpace,
        sides: &BoxSides,
        establishes_bfc: bool,
        can_collapse_with_children: bool,
    ) -> MarginStrut {
        if establishes_bfc {
            MarginStrut::default()
        } else if can_collapse_with_children {
            // When can_collapse_with_children is true, the parent's margin should be
            // in the strut so the first child can collapse with it.
            // The parent's bfc_offset was computed INCLUDING this margin, but we
            // subtract it in compute_child_base_bfc_offset and pass it here instead.
            let mut strut = MarginStrut::default();
            strut.append(sides.margin_top);
            strut
        } else {
            // Parent has padding/border preventing collapse, start with empty strut
            MarginStrut::default()
        }
    }

    /// Compute the base BFC offset for children.
    fn compute_child_base_bfc_offset(
        bfc_offset: BfcOffset,
        sides: &BoxSides,
        establishes_bfc: bool,
        can_collapse_with_children: bool,
    ) -> BfcOffset {
        if establishes_bfc {
            BfcOffset::root()
        } else if can_collapse_with_children {
            // When parent can collapse with children, the parent's bfc_offset includes
            // the parent's collapsed margin. To allow children to re-collapse with the
            // parent's margin, we subtract it here and pass it via initial_margin_strut.
            BfcOffset::new(
                bfc_offset.inline_offset,
                bfc_offset
                    .block_offset
                    .map(|offset| offset - sides.margin_top),
            )
        } else {
            BfcOffset::new(
                bfc_offset.inline_offset + sides.padding_left + sides.border_left,
                bfc_offset
                    .block_offset
                    .map(|offset| offset + sides.padding_top + sides.border_top),
            )
        }
    }

    /// Layout a text node child.
    fn layout_text_child(
        &mut self,
        parent_child: (NodeKey, NodeKey),
        child_space: &mut ConstraintSpace,
        state: &mut ChildrenLayoutState,
        can_collapse_with_children: bool,
    ) -> bool {
        let (parent, child) = parent_child;
        let text = self.text_nodes.get(&child).map_or("", String::as_str);
        let is_whitespace_only = text.chars().all(char::is_whitespace);

        if is_whitespace_only {
            let text_result = LayoutResult {
                inline_size: 0.0,
                block_size: 0.0,
                bfc_offset: child_space.bfc_offset,
                exclusion_space: child_space.exclusion_space.clone(),
                end_margin_strut: MarginStrut::default(),
                baseline: None,
                needs_relayout: false,
            };
            self.layout_results.insert(child, text_result);
            return false;
        }

        state.has_text_content = true;

        // Text nodes don't have margins, but if parent can collapse with children,
        // we need to collapse the parent's margin in the strut and place text after it.
        let mut resolved_offset = child_space
            .bfc_offset
            .block_offset
            .unwrap_or(LayoutUnit::zero());

        if !state.first_inflow_child_seen && can_collapse_with_children {
            // This is the first child and parent can collapse with children.
            // The margin strut contains the parent's margin. Since text has no margin,
            // we collapse the strut and add it to position the text.
            let collapsed_margin = child_space.margin_strut.collapse();
            resolved_offset += collapsed_margin;

            // Update parent's resolved position to match first child
            state.resolved_bfc_offset.block_offset = Some(resolved_offset);
            state.first_inflow_child_seen = true;
        }

        let (text_width, text_height) =
            self.measure_text(child, Some(parent), child_space.available_inline_size);

        // Get the parent's line-height for vertical spacing
        let parent_style = self.styles.get(&parent).cloned().unwrap_or_default();
        let line_height = parent_style
            .line_height
            .unwrap_or_else(|| default_line_height_px(&parent_style) as f32);

        let text_result = LayoutResult {
            inline_size: text_width,
            block_size: text_height, // Text node rect uses actual font height
            bfc_offset: BfcOffset::new(child_space.bfc_offset.inline_offset, Some(resolved_offset)),
            exclusion_space: child_space.exclusion_space.clone(),
            end_margin_strut: MarginStrut::default(),
            baseline: None,
            needs_relayout: false,
        };

        self.layout_results.insert(child, text_result);
        child_space.margin_strut = MarginStrut::default();
        // Use line-height for vertical spacing, not text_height
        child_space.bfc_offset.block_offset =
            Some(resolved_offset + LayoutUnit::from_px(line_height.round()));
        true
    }

    /// Layout a block child and update state.
    fn layout_block_child_and_update_state(
        &mut self,
        child: NodeKey,
        child_space: &mut ConstraintSpace,
        state: &mut ChildrenLayoutState,
        can_collapse_with_children: bool,
    ) {
        let child_result = self.layout_block(child, child_space);
        child_space.exclusion_space = child_result.exclusion_space.clone();

        let child_style = self.style(child);
        let is_float = !matches!(child_style.float, Float::None);
        let is_out_of_flow =
            is_float || matches!(child_style.position, Position::Absolute | Position::Fixed);

        // Check if child has clearance - if so, margins don't collapse with parent
        let has_clear = !matches!(child_style.clear, Clear::None);
        let has_floats_to_clear = child_space.exclusion_space.all_floats().next().is_some();
        let has_clearance = has_clear && has_floats_to_clear;

        // Only update layout state for in-flow children (not floats or absolutely positioned)
        if !is_out_of_flow {
            if let Some(child_start) = child_result.bfc_offset.block_offset {
                // Only resolve parent offset if child doesn't have clearance
                // (clearance prevents margin collapse)
                if !has_clearance {
                    Self::resolve_parent_offset_if_needed(
                        &mut state.resolved_bfc_offset,
                        &child_result,
                        state.first_inflow_child_seen,
                        can_collapse_with_children,
                    );
                }

                state.first_inflow_child_seen = true;
                let child_border_box_end =
                    child_start + LayoutUnit::from_px(child_result.block_size.round());

                // BUG FIX: For self-collapsing elements, margins collapse THROUGH them
                // The next sibling should start at the parent's base offset, not after the self-collapsing element
                // We detect self-collapsing by: block_size==0 and end_margin_strut contains incoming margins
                let is_self_collapsing = child_result.block_size.abs() < 0.01
                    && child_result.end_margin_strut.positive_margin > LayoutUnit::zero();

                if is_self_collapsing && can_collapse_with_children {
                    // Self-collapsing element: next sibling starts where this element's incoming position was
                    // This allows the next sibling's margin to collapse with all the accumulated margins
                    // NOTE: child_space.bfc_offset.block_offset stays unchanged (not advanced)
                } else {
                    // Normal element: set next child's starting position to bottom of current child
                    let child_end = child_border_box_end;
                    // The margin strut will be carried forward for potential sibling collapse
                    child_space.bfc_offset.block_offset = Some(child_end);
                }
            }

            // Carry forward the child's end margin strut for potential sibling collapse
            // Important: This allows the next sibling to collapse margins properly
            child_space.margin_strut = child_result.end_margin_strut;
            state.last_child_end_margin_strut = child_result.end_margin_strut;
            tracing::debug!(
                "layout_block_child_and_update_state: Setting next child margin_strut={:?}",
                child_space.margin_strut
            );
        }

        state.max_block_size = state.max_block_size.max(child_result.block_size);
        self.layout_results.insert(child, child_result);
    }

    /// Create child constraint space for block layout.
    fn create_block_child_space(
        constraint_space: &ConstraintSpace,
        inline_size: f32,
        child_base_bfc_offset: BfcOffset,
        initial_margin_strut: MarginStrut,
        establishes_bfc: bool,
    ) -> ConstraintSpace {
        ConstraintSpace {
            available_inline_size: AvailableSize::Definite(LayoutUnit::from_px(inline_size)),
            available_block_size: constraint_space.available_block_size,
            bfc_offset: child_base_bfc_offset,
            exclusion_space: if establishes_bfc {
                ExclusionSpace::new()
            } else {
                constraint_space.exclusion_space.clone()
            },
            margin_strut: initial_margin_strut,
            is_new_formatting_context: establishes_bfc,
            percentage_resolution_block_size: constraint_space.percentage_resolution_block_size,
            fragmentainer_block_size: constraint_space.fragmentainer_block_size,
            fragmentainer_offset: constraint_space.fragmentainer_offset,
        }
    }

    /// Compute end margin strut for block element.
    fn compute_end_margin_strut(params: &EndMarginStrutParams<'_>) -> MarginStrut {
        let can_collapse_bottom = params.sides.padding_bottom == LayoutUnit::zero()
            && params.sides.border_bottom == LayoutUnit::zero();
        let can_collapse_top = params.sides.padding_top == LayoutUnit::zero()
            && params.sides.border_top == LayoutUnit::zero();

        // Check if this is a self-collapsing element (no content/height, no children)
        let is_self_collapsing = params.block_size.abs() < 0.01
            && !params.state.first_inflow_child_seen
            && can_collapse_top
            && can_collapse_bottom
            && params.can_collapse_with_children;

        if is_self_collapsing {
            // Self-collapsing element: margins collapse THROUGH the element
            // The next sibling should see all margins that collapsed through this element
            // BUG FIX: Include the INCOMING margin strut (which contains parent/sibling margins)
            // along with this element's own top and bottom margins
            let mut strut = params.incoming_space.margin_strut;
            strut.append(params.sides.margin_top);
            strut.append(params.sides.margin_bottom);
            return strut;
        }

        // If we have children and can collapse bottom margins
        if params.state.first_inflow_child_seen && can_collapse_bottom {
            // Collapse our bottom margin with last child's end margin strut
            let mut strut = params.state.last_child_end_margin_strut;
            strut.append(params.sides.margin_bottom);
            return strut;
        }

        // If we cannot collapse bottom (have padding/border), only our own bottom margin
        if !can_collapse_bottom {
            let mut strut = MarginStrut::default();
            strut.append(params.sides.margin_bottom);
            return strut;
        }

        // No children and can collapse bottom (but not self-collapsing due to explicit height):
        // Return just the bottom margin
        let mut strut = MarginStrut::default();
        strut.append(params.sides.margin_bottom);
        strut
    }

    /// Compute final block size from children layout.
    fn compute_block_size_from_children(
        &self,
        node: NodeKey,
        params: &BlockSizeParams<'_>,
        sides: &BoxSides,
        style: &ComputedStyle,
    ) -> f32 {
        // Compute border-box height
        let border_box_height = params.style_height.map_or_else(
            || {
                // Check for form control intrinsic height first
                if let Some(intrinsic_height) =
                    self.compute_form_control_intrinsic_height(node, style)
                {
                    // Form control has intrinsic height - use it as content-box height
                    let padding_border = sides.padding_top.to_px()
                        + sides.padding_bottom.to_px()
                        + sides.border_top.to_px()
                        + sides.border_bottom.to_px();
                    return intrinsic_height + padding_border;
                }

                // Auto height: compute from children
                // For BFC roots, children are laid out in the new BFC starting at 0
                // For non-BFC elements, use resolved/bfc offset depending on margin collapse
                let start_offset = if params.establishes_bfc {
                    LayoutUnit::zero()
                } else if params.can_collapse_with_children {
                    params
                        .resolved_bfc_offset
                        .block_offset
                        .unwrap_or(LayoutUnit::zero())
                } else {
                    params.bfc_offset.block_offset.unwrap_or(LayoutUnit::zero())
                        + sides.padding_top
                        + sides.border_top
                };

                // Consider both normal flow children and floats for the end offset
                let normal_flow_end = params.child_space_bfc_offset.unwrap_or(start_offset);
                let float_end = params.exclusion_space.last_float_bottom();
                let end_offset = normal_flow_end.max(float_end);

                let mut content_height =
                    (end_offset - start_offset).max(LayoutUnit::zero()).to_px();

                if content_height == 0.0 && params.has_text_content {
                    content_height = 18.0;
                }

                // BUG FIX: When bottom margin doesn't collapse (padding/border present),
                // the last child's margin must be included in the height calculation
                let can_collapse_bottom = sides.padding_bottom == LayoutUnit::zero()
                    && sides.border_bottom == LayoutUnit::zero();
                let non_collapsing_bottom_margin = if can_collapse_bottom {
                    0.0
                } else {
                    params.last_child_end_margin_strut.collapse().to_px()
                };

                // For both can_collapse_with_children cases, we use content_height as base:
                // - When true: padding/border is added below
                // - When false: start_offset already includes padding_top + border_top,
                //   but we still add all edges below to get correct border-box height
                content_height
                    + non_collapsing_bottom_margin
                    + sides.padding_top.to_px()
                    + sides.padding_bottom.to_px()
                    + sides.border_top.to_px()
                    + sides.border_bottom.to_px()
            },
            |height| {
                // Apply box-sizing transformation to specified height
                match style.box_sizing {
                    BoxSizing::ContentBox => {
                        // Height is content-box, add padding and border
                        let padding_border = sides.padding_top.to_px()
                            + sides.padding_bottom.to_px()
                            + sides.border_top.to_px()
                            + sides.border_bottom.to_px();
                        height + padding_border
                    }
                    BoxSizing::BorderBox => {
                        // Height already includes padding and border
                        height
                    }
                }
            },
        );

        // Apply min/max constraints in border-box space
        Self::apply_height_constraints(border_box_height.max(0.0), style, sides)
    }

    /// Compute border-box inline size.
    fn compute_border_box_inline(inline_size: f32, sides: &BoxSides) -> f32 {
        inline_size
            + sides.padding_left.to_px()
            + sides.padding_right.to_px()
            + sides.border_left.to_px()
            + sides.border_right.to_px()
    }

    /// Resolve the final BFC offset for a block box.
    fn resolve_final_bfc_offset(
        block_size: f32,
        can_collapse_with_children: bool,
        state: &ChildrenLayoutState,
        params: &BlockLayoutParams,
        _initial_margin_strut: MarginStrut,
    ) -> BfcOffset {
        // - Self-collapsing box: resolve based on parent offset + collapsed margins
        // - Box that collapses with first child: use the resolved offset (matches first child)
        // - Box with border/padding (can't collapse): use params offset
        if block_size.abs() < 0.01 && !state.first_inflow_child_seen && can_collapse_with_children {
            // Self-collapsing box: resolve position based on parent offset + all collapsed margins
            // BUG FIX: Use the incoming margin strut from params, not just the element's own margins
            // The incoming strut includes parent/sibling margins that should collapse with this element
            let parent_offset = params
                .constraint_space
                .bfc_offset
                .block_offset
                .unwrap_or(LayoutUnit::zero());
            let mut margin_strut = params.constraint_space.margin_strut;
            margin_strut.append(params.sides.margin_top);
            margin_strut.append(params.sides.margin_bottom);
            let margin_collapse = margin_strut.collapse();
            let resolved_offset = parent_offset + margin_collapse;
            BfcOffset::new(params.bfc_offset.inline_offset, Some(resolved_offset))
        } else if can_collapse_with_children && state.first_inflow_child_seen {
            // Non-self-collapsing box that can collapse with children: use resolved offset
            // (this is where the first child ended up after margin collapse)
            state.resolved_bfc_offset
        } else {
            // Box with border/padding or no children: use the box's own offset
            params.bfc_offset
        }
    }

    /// Process all children in the layout loop.
    fn process_children_layout(
        &mut self,
        node: NodeKey,
        child_space: &mut ConstraintSpace,
        state: &mut ChildrenLayoutState,
        can_collapse_with_children: bool,
    ) {
        // Normalize children to handle display:none and display:contents
        let children = normalize_children(&self.children, &self.styles, node);

        for child in children {
            if self.is_text_node(child) {
                self.layout_text_child(
                    (node, child),
                    child_space,
                    state,
                    can_collapse_with_children,
                );
            } else {
                self.layout_block_child_and_update_state(
                    child,
                    child_space,
                    state,
                    can_collapse_with_children,
                );
            }
        }
    }

    /// Layout block's children and compute final size.
    fn layout_block_children(&mut self, node: NodeKey, params: &BlockLayoutParams) -> LayoutResult {
        // Margins can collapse with children only if:
        // 1. No padding/border at top
        // 2. Parent doesn't establish a new BFC (BFC boundary prevents collapse)
        let can_collapse_with_children = params.sides.padding_top == LayoutUnit::zero()
            && params.sides.border_top == LayoutUnit::zero()
            && !params.establishes_bfc;

        let initial_margin_strut = Self::compute_initial_margin_strut(
            params.constraint_space,
            params.sides,
            params.establishes_bfc,
            can_collapse_with_children,
        );

        let child_base_bfc_offset = Self::compute_child_base_bfc_offset(
            params.bfc_offset,
            params.sides,
            params.establishes_bfc,
            can_collapse_with_children,
        );

        let mut child_space = Self::create_block_child_space(
            params.constraint_space,
            params.inline_size,
            child_base_bfc_offset,
            initial_margin_strut,
            params.establishes_bfc,
        );

        let mut state = ChildrenLayoutState::new(params.bfc_offset);

        // Process all children
        self.process_children_layout(
            node,
            &mut child_space,
            &mut state,
            can_collapse_with_children,
        );

        let block_size_params = BlockSizeParams {
            style_height: params.style.height,
            can_collapse_with_children,
            establishes_bfc: params.establishes_bfc,
            resolved_bfc_offset: state.resolved_bfc_offset,
            bfc_offset: params.bfc_offset,
            child_space_bfc_offset: child_space.bfc_offset.block_offset,
            has_text_content: state.has_text_content,
            exclusion_space: &child_space.exclusion_space,
            last_child_end_margin_strut: state.last_child_end_margin_strut,
        };
        let block_size = self.compute_block_size_from_children(
            node,
            &block_size_params,
            params.sides,
            params.style,
        );
        let border_box_inline = Self::compute_border_box_inline(params.inline_size, params.sides);

        let end_margin_strut_params = EndMarginStrutParams {
            sides: params.sides,
            state: &state,
            incoming_space: params.constraint_space,
            can_collapse_with_children,
            block_size,
        };
        let end_margin_strut = Self::compute_end_margin_strut(&end_margin_strut_params);

        let final_bfc_offset = Self::resolve_final_bfc_offset(
            block_size,
            can_collapse_with_children,
            &state,
            params,
            initial_margin_strut,
        );

        LayoutResult {
            inline_size: border_box_inline.max(0.0),
            block_size: block_size.max(0.0),
            bfc_offset: final_bfc_offset,
            exclusion_space: child_space.exclusion_space,
            end_margin_strut,
            baseline: None,
            needs_relayout: false,
        }
    }

    /// Compute inline offset for a float based on direction.
    fn compute_float_inline_offset(
        constraint_space: &ConstraintSpace,
        style: &ComputedStyle,
        sides: &BoxSides,
        border_box_inline: f32,
        container_inline_size: LayoutUnit,
    ) -> LayoutUnit {
        let base_inline_offset = constraint_space.bfc_offset.inline_offset + sides.margin_left;

        match style.float {
            Float::Right => {
                constraint_space.bfc_offset.inline_offset + container_inline_size
                    - LayoutUnit::from_px(border_box_inline.round())
                    - sides.margin_right
            }
            Float::Left | Float::None => base_inline_offset,
        }
    }

    /// Layout float's children and compute content height.
    fn layout_float_children(
        &mut self,
        node: NodeKey,
        inline_size: f32,
        constraint_space: &ConstraintSpace,
        sides: &BoxSides,
    ) -> f32 {
        let child_space = ConstraintSpace {
            available_inline_size: AvailableSize::Definite(LayoutUnit::from_px(
                inline_size.max(0.0),
            )),
            available_block_size: AvailableSize::Indefinite,
            bfc_offset: BfcOffset::root(),
            exclusion_space: ExclusionSpace::new(),
            margin_strut: MarginStrut::default(),
            is_new_formatting_context: true,
            percentage_resolution_block_size: constraint_space.percentage_resolution_block_size,
            fragmentainer_block_size: constraint_space.fragmentainer_block_size,
            fragmentainer_offset: constraint_space.fragmentainer_offset,
        };

        // Normalize children to handle display:none and display:contents
        let children = normalize_children(&self.children, &self.styles, node);
        let mut content_height = 0.0f32;

        for child in children {
            let child_result = self.layout_block(child, &child_space);
            content_height = content_height.max(
                child_result
                    .bfc_offset
                    .block_offset
                    .unwrap_or(LayoutUnit::zero())
                    .to_px()
                    + child_result.block_size,
            );
            self.layout_results.insert(child, child_result);
        }

        content_height
            + sides.padding_top.to_px()
            + sides.padding_bottom.to_px()
            + sides.border_top.to_px()
            + sides.border_bottom.to_px()
    }

    /// Layout a float element.
    fn layout_float(
        &mut self,
        node: NodeKey,
        constraint_space: &ConstraintSpace,
        style: &ComputedStyle,
        sides: &BoxSides,
    ) -> LayoutResult {
        let inline_size = style.width.unwrap_or_else(|| {
            (constraint_space
                .available_inline_size
                .resolve(LayoutUnit::from_px(400.0))
                .to_px()
                / 2.0)
                .max(100.0)
        });

        let container_inline_size = constraint_space
            .available_inline_size
            .resolve(self.icb_width);
        let border_box_inline = inline_size
            + sides.padding_left.to_px()
            + sides.padding_right.to_px()
            + sides.border_left.to_px()
            + sides.border_right.to_px();

        let inline_offset = Self::compute_float_inline_offset(
            constraint_space,
            style,
            sides,
            border_box_inline,
            container_inline_size,
        );

        let float_bfc_offset =
            BfcOffset::new(inline_offset, constraint_space.bfc_offset.block_offset);

        // Compute border-box height with box-sizing and constraints
        let block_size = style.height.map_or_else(
            || {
                // Auto height: layout children (already returns border-box)
                let border_box_height =
                    self.layout_float_children(node, inline_size, constraint_space, sides);
                Self::apply_height_constraints(border_box_height, style, sides)
            },
            |height| {
                let border_box_height = match style.box_sizing {
                    BoxSizing::ContentBox => {
                        let padding_border = sides.padding_top.to_px()
                            + sides.padding_bottom.to_px()
                            + sides.border_top.to_px()
                            + sides.border_bottom.to_px();
                        height + padding_border
                    }
                    BoxSizing::BorderBox => height,
                };
                Self::apply_height_constraints(border_box_height, style, sides)
            },
        );

        let mut updated_exclusion = constraint_space.exclusion_space.clone();
        updated_exclusion.add_float(
            node,
            float_bfc_offset,
            FloatSize {
                inline_size: LayoutUnit::from_px(
                    (border_box_inline + sides.margin_left.to_px() + sides.margin_right.to_px())
                        .round(),
                ),
                block_size: LayoutUnit::from_px(
                    (block_size + sides.margin_top.to_px() + sides.margin_bottom.to_px()).round(),
                ),
                float_type: style.float,
            },
        );

        LayoutResult {
            inline_size: border_box_inline,
            block_size,
            bfc_offset: float_bfc_offset,
            exclusion_space: updated_exclusion,
            end_margin_strut: MarginStrut::default(),
            baseline: None,
            needs_relayout: false,
        }
    }

    /// Layout absolutely positioned children and compute content height.
    fn layout_absolute_children(
        &mut self,
        node: NodeKey,
        inline_size: f32,
        constraint_space: &ConstraintSpace,
    ) -> f32 {
        let child_space = ConstraintSpace {
            available_inline_size: AvailableSize::Definite(LayoutUnit::from_px(inline_size)),
            available_block_size: AvailableSize::Indefinite,
            bfc_offset: BfcOffset::root(),
            exclusion_space: ExclusionSpace::new(),
            margin_strut: MarginStrut::default(),
            is_new_formatting_context: true,
            percentage_resolution_block_size: constraint_space.percentage_resolution_block_size,
            fragmentainer_block_size: constraint_space.fragmentainer_block_size,
            fragmentainer_offset: constraint_space.fragmentainer_offset,
        };

        // Normalize children to handle display:none and display:contents
        let children = normalize_children(&self.children, &self.styles, node);
        let mut content_height = 0.0f32;

        for child in children {
            let child_result = self.layout_block(child, &child_space);
            content_height = content_height.max(
                child_result
                    .bfc_offset
                    .block_offset
                    .unwrap_or(LayoutUnit::zero())
                    .to_px()
                    + child_result.block_size,
            );
            self.layout_results.insert(child, child_result);
        }

        content_height
    }

    /// Compute absolute positioning offset based on containing block and style.
    fn compute_abspos_offset(
        &self,
        constraint_space: &ConstraintSpace,
        style: &ComputedStyle,
        sides: &BoxSides,
    ) -> BfcOffset {
        // Determine containing block based on position type
        let (containing_block_inline, containing_block_block) =
            if matches!(style.position, Position::Fixed) {
                (LayoutUnit::zero(), LayoutUnit::zero())
            } else {
                (
                    constraint_space.bfc_offset.inline_offset,
                    constraint_space
                        .bfc_offset
                        .block_offset
                        .unwrap_or(LayoutUnit::zero()),
                )
            };

        // Apply positioning offsets (left, top, right, bottom)
        let mut inline_offset = containing_block_inline;
        let mut block_offset = containing_block_block;

        // Apply left offset if specified
        if let Some(left) = style.left {
            inline_offset += LayoutUnit::from_px(left);
        } else if let Some(left_percent) = style.left_percent {
            let cb_width = match constraint_space.available_inline_size {
                AvailableSize::Definite(width) => width,
                _ => self.icb_width,
            };
            inline_offset += cb_width * left_percent;
        }

        // Apply top offset if specified
        if let Some(top) = style.top {
            block_offset += LayoutUnit::from_px(top);
        } else if let Some(top_percent) = style.top_percent {
            let cb_height = constraint_space
                .percentage_resolution_block_size
                .unwrap_or(self.icb_height);
            block_offset += cb_height * top_percent;
        }

        // Add margins to the final position
        inline_offset += sides.margin_left;
        block_offset += sides.margin_top;

        BfcOffset::new(inline_offset, Some(block_offset))
    }

    /// Layout an absolutely positioned element.
    fn layout_absolute(
        &mut self,
        node: NodeKey,
        constraint_space: &ConstraintSpace,
        style: &ComputedStyle,
        sides: &BoxSides,
    ) -> LayoutResult {
        // Absolutely positioned elements establish BFC

        // Compute border-box width with box-sizing (match height logic below)
        let border_box_inline = style.width.map_or_else(
            || {
                // Auto width for abspos: shrink to fit (simplified to 200.0 content + padding/border)
                200.0
                    + sides.padding_left.to_px()
                    + sides.padding_right.to_px()
                    + sides.border_left.to_px()
                    + sides.border_right.to_px()
            },
            |width| match style.box_sizing {
                BoxSizing::ContentBox => {
                    let padding_border = sides.padding_left.to_px()
                        + sides.padding_right.to_px()
                        + sides.border_left.to_px()
                        + sides.border_right.to_px();
                    width + padding_border
                }
                BoxSizing::BorderBox => width,
            },
        );

        // Compute content-box width for laying out children
        let content_inline_size = match style.box_sizing {
            BoxSizing::ContentBox => style.width.unwrap_or(200.0),
            BoxSizing::BorderBox => {
                let padding_border = sides.padding_left.to_px()
                    + sides.padding_right.to_px()
                    + sides.border_left.to_px()
                    + sides.border_right.to_px();
                style.width.unwrap_or(200.0 + padding_border) - padding_border
            }
        };

        // Compute border-box height with box-sizing and constraints
        let block_size = style.height.map_or_else(
            || {
                // Auto height: layout children
                let content_height =
                    self.layout_absolute_children(node, content_inline_size, constraint_space);
                let border_box_height = content_height
                    + sides.padding_top.to_px()
                    + sides.padding_bottom.to_px()
                    + sides.border_top.to_px()
                    + sides.border_bottom.to_px();
                Self::apply_height_constraints(border_box_height, style, sides)
            },
            |height| {
                let border_box_height = match style.box_sizing {
                    BoxSizing::ContentBox => {
                        let padding_border = sides.padding_top.to_px()
                            + sides.padding_bottom.to_px()
                            + sides.border_top.to_px()
                            + sides.border_bottom.to_px();
                        height + padding_border
                    }
                    BoxSizing::BorderBox => height,
                };
                Self::apply_height_constraints(border_box_height, style, sides)
            },
        );

        let abspos_bfc_offset = self.compute_abspos_offset(constraint_space, style, sides);

        // Abspos doesn't affect normal flow, so don't add to exclusion space
        LayoutResult {
            inline_size: border_box_inline,
            block_size,
            bfc_offset: abspos_bfc_offset,
            exclusion_space: constraint_space.exclusion_space.clone(),
            end_margin_strut: MarginStrut::default(),
            baseline: None,
            needs_relayout: false,
        }
    }

    /// Resolve parent BFC offset if needed (helper to reduce nesting).
    fn resolve_parent_offset_if_needed(
        resolved_bfc_offset: &mut BfcOffset,
        child_result: &LayoutResult,
        first_inflow_child_seen: bool,
        can_collapse_with_children: bool,
    ) {
        if !first_inflow_child_seen && can_collapse_with_children {
            // Parent collapses with first child - they share the same BFC offset
            // The child's offset already includes all collapsed margins
            resolved_bfc_offset.block_offset = child_result.bfc_offset.block_offset;
        }
    }

    /// Compute flex basis for a flex item (helper to reduce nesting).
    fn compute_flex_basis(
        child_style: &ComputedStyle,
        child_sides: &BoxSides,
        child_result: &LayoutResult,
        is_row: bool,
    ) -> f32 {
        if let Some(basis) = child_style.flex_basis {
            return basis;
        }

        if is_row {
            // Row: flex basis is width (if specified), else intrinsic size
            Self::compute_flex_basis_from_width(child_style, child_sides, child_result)
        } else {
            // Column: flex basis is height (if specified), else intrinsic size
            Self::compute_flex_basis_from_height(child_style, child_sides, child_result)
        }
    }

    /// Compute flex basis from width (helper).
    fn compute_flex_basis_from_width(
        child_style: &ComputedStyle,
        child_sides: &BoxSides,
        child_result: &LayoutResult,
    ) -> f32 {
        child_style.width.map_or_else(
            || {
                // No explicit width: use intrinsic content-box size
                child_result.inline_size
                    - (child_sides.padding_left
                        + child_sides.padding_right
                        + child_sides.border_left
                        + child_sides.border_right)
                        .to_px()
            },
            |width| {
                // Explicit width: need to account for box-sizing
                match child_style.box_sizing {
                    BoxSizing::BorderBox => {
                        let padding_border = (child_sides.padding_left
                            + child_sides.padding_right
                            + child_sides.border_left
                            + child_sides.border_right)
                            .to_px();
                        (width - padding_border).max(0.0)
                    }
                    BoxSizing::ContentBox => width,
                }
            },
        )
    }

    /// Compute flex basis from height (helper).
    fn compute_flex_basis_from_height(
        child_style: &ComputedStyle,
        child_sides: &BoxSides,
        child_result: &LayoutResult,
    ) -> f32 {
        child_style.height.map_or_else(
            || {
                // No explicit height: use intrinsic content-box size
                child_result.block_size
                    - (child_sides.padding_top
                        + child_sides.padding_bottom
                        + child_sides.border_top
                        + child_sides.border_bottom)
                        .to_px()
            },
            |height| {
                // Explicit height: need to account for box-sizing
                match child_style.box_sizing {
                    BoxSizing::BorderBox => {
                        let padding_border = (child_sides.padding_top
                            + child_sides.padding_bottom
                            + child_sides.border_top
                            + child_sides.border_bottom)
                            .to_px();
                        (height - padding_border).max(0.0)
                    }
                    BoxSizing::ContentBox => height,
                }
            },
        )
    }

    /// Compute flex container's content-box cross size.
    fn compute_flex_container_cross_size(style: &ComputedStyle, sides: &BoxSides) -> f32 {
        style.height.map_or(100.0, |height| match style.box_sizing {
            BoxSizing::BorderBox => {
                let padding_border = (sides.padding_top
                    + sides.padding_bottom
                    + sides.border_top
                    + sides.border_bottom)
                    .to_px();
                (height - padding_border).max(0.0)
            }
            BoxSizing::ContentBox => height,
        })
    }

    /// Build flex items from children, filtering out abspos and display:none.
    fn build_flex_items(&mut self, children: &[NodeKey]) -> FlexLayoutResult {
        let mut flex_items = Vec::new();
        let mut child_styles = Vec::new();
        let mut abspos_children = Vec::new();

        let child_space = ConstraintSpace {
            available_inline_size: AvailableSize::Indefinite,
            available_block_size: AvailableSize::Indefinite,
            bfc_offset: BfcOffset::root(),
            exclusion_space: ExclusionSpace::new(),
            margin_strut: MarginStrut::default(),
            is_new_formatting_context: true,
            percentage_resolution_block_size: None,
            fragmentainer_block_size: None,
            fragmentainer_offset: LayoutUnit::zero(),
        };

        for child in children {
            // Skip whitespace-only text nodes (CSS Flexbox spec §4.1)
            // Text nodes in flex containers only generate anonymous flex items
            // if they contain non-whitespace characters
            if let Some(text) = self.text_nodes.get(child)
                && text.trim().is_empty()
            {
                continue;
            }

            let child_style = self.style(*child);

            if matches!(child_style.display, Display::None) {
                continue;
            }

            if matches!(child_style.position, Position::Absolute | Position::Fixed) {
                abspos_children.push(*child);
                continue;
            }

            let child_result = self.layout_block(*child, &child_space);
            let child_sides = compute_box_sides(&child_style);

            flex_items.push(FlexChild {
                handle: ItemRef(child.0),
                flex_basis: 0.0, // Will be filled later
                flex_grow: child_style.flex_grow,
                flex_shrink: child_style.flex_shrink,
                min_main: 0.0,
                max_main: 1e9,
                margin_left: child_sides.margin_left.to_px(),
                margin_right: child_sides.margin_right.to_px(),
                margin_top: child_sides.margin_top.to_px(),
                margin_bottom: child_sides.margin_bottom.to_px(),
                margin_left_auto: false,
                margin_right_auto: false,
            });

            child_styles.push((*child, child_style, child_result));
        }

        (flex_items, child_styles, abspos_children)
    }

    /// Update flex item basis values based on child styles and results.
    fn update_flex_item_basis(
        flex_items: &mut [FlexChild],
        child_styles: &[ChildStyleInfo],
        is_row: bool,
    ) {
        for (idx, (_, child_style, child_result)) in child_styles.iter().enumerate() {
            if let Some(item) = flex_items.get_mut(idx) {
                let child_sides = compute_box_sides(child_style);
                item.flex_basis =
                    Self::compute_flex_basis(child_style, &child_sides, child_result, is_row);
            }
        }
    }

    /// Check if child has explicit inline offset based on direction.
    fn has_explicit_inline_offset(child_style: &ComputedStyle, is_row: bool) -> bool {
        if is_row {
            child_style.left.is_some() || child_style.left_percent.is_some()
        } else {
            child_style.top.is_some() || child_style.top_percent.is_some()
        }
    }

    /// Check if child has explicit block offset based on direction.
    fn has_explicit_block_offset(child_style: &ComputedStyle, is_row: bool) -> bool {
        if is_row {
            child_style.top.is_some() || child_style.top_percent.is_some()
        } else {
            child_style.left.is_some() || child_style.left_percent.is_some()
        }
    }

    /// Compute static main offset for flex abspos child.
    fn compute_static_main_offset(params: &FlexAbsposConstraintParams<'_>) -> f32 {
        if Self::has_explicit_inline_offset(params.child_style, params.is_row) {
            return 0.0;
        }

        if params.is_row {
            // Row: main axis is inline, justify-content applies
            Self::compute_flex_abspos_main_offset(
                params.container_style,
                params.child_style,
                params.child_sides,
                params.container_inline_size,
                params.is_row,
            )
        } else {
            // Column: main axis is block, justify-content still applies to block axis
            Self::compute_flex_abspos_main_offset(
                params.container_style,
                params.child_style,
                params.child_sides,
                params.final_cross_size,
                params.is_row,
            )
        }
    }

    /// Compute static cross offset for flex abspos child.
    fn compute_static_cross_offset(params: &FlexAbsposConstraintParams<'_>) -> f32 {
        if Self::has_explicit_block_offset(params.child_style, params.is_row) {
            return 0.0;
        }

        if params.is_row {
            // Row: cross axis is block, align-items applies
            Self::compute_flex_abspos_cross_offset(
                params.container_style,
                params.child_style,
                params.child_sides,
                params.final_cross_size,
                params.is_row,
            )
        } else {
            // Column: cross axis is inline, align-items applies
            Self::compute_flex_abspos_cross_offset(
                params.container_style,
                params.child_style,
                params.child_sides,
                params.container_inline_size,
                params.is_row,
            )
        }
    }

    /// Create constraint space for abspos children in flex container.
    ///
    /// The containing block for absolutely positioned children of a flex container
    /// is the padding box of the flex container (content box + padding).
    fn create_flex_abspos_constraint_space(
        params: &FlexAbsposConstraintParams<'_>,
    ) -> ConstraintSpace {
        // The containing block for abspos children starts at the content box
        // (i.e., inside padding and border)
        let content_inline_offset =
            params.bfc_offset.inline_offset + params.sides.padding_left + params.sides.border_left;
        let content_block_offset = params
            .bfc_offset
            .block_offset
            .map(|y| y + params.sides.padding_top + params.sides.border_top);

        // Compute static position offsets (only if child doesn't have explicit offsets)
        let static_main_offset = Self::compute_static_main_offset(params);
        let static_cross_offset = Self::compute_static_cross_offset(params);

        let content_bfc_offset = if params.is_row {
            BfcOffset::new(
                content_inline_offset + LayoutUnit::from_px(static_main_offset),
                content_block_offset.map(|y| y + LayoutUnit::from_px(static_cross_offset)),
            )
        } else {
            BfcOffset::new(
                content_inline_offset + LayoutUnit::from_px(static_cross_offset),
                content_block_offset.map(|y| y + LayoutUnit::from_px(static_main_offset)),
            )
        };

        ConstraintSpace {
            available_inline_size: AvailableSize::Definite(LayoutUnit::from_px(
                params.container_inline_size,
            )),
            available_block_size: AvailableSize::Definite(if params.is_row {
                LayoutUnit::from_px(params.final_cross_size)
            } else {
                LayoutUnit::from_px(params.container_inline_size)
            }),
            bfc_offset: content_bfc_offset,
            exclusion_space: ExclusionSpace::new(),
            margin_strut: MarginStrut::default(),
            is_new_formatting_context: true,
            percentage_resolution_block_size: Some(if params.is_row {
                LayoutUnit::from_px(params.final_cross_size)
            } else {
                LayoutUnit::from_px(params.container_inline_size)
            }),
            fragmentainer_block_size: None,
            fragmentainer_offset: LayoutUnit::zero(),
        }
    }

    /// Compute static position offset along the main axis for abspos child in flex container.
    fn compute_flex_abspos_main_offset(
        container_style: &ComputedStyle,
        child_style: &ComputedStyle,
        child_sides: &BoxSides,
        container_main_size: f32,
        is_row: bool,
    ) -> f32 {
        use css_orchestrator::style_model::JustifyContent;

        // Get child's main size (border-box)
        let child_main_size = if is_row {
            child_style.width.map_or_else(
                || {
                    200.0
                        + child_sides.padding_left.to_px()
                        + child_sides.padding_right.to_px()
                        + child_sides.border_left.to_px()
                        + child_sides.border_right.to_px()
                },
                |width| {
                    width
                        + child_sides.padding_left.to_px()
                        + child_sides.padding_right.to_px()
                        + child_sides.border_left.to_px()
                        + child_sides.border_right.to_px()
                },
            )
        } else {
            child_style.height.unwrap_or(0.0)
                + child_sides.padding_top.to_px()
                + child_sides.padding_bottom.to_px()
                + child_sides.border_top.to_px()
                + child_sides.border_bottom.to_px()
        };

        // Apply justify-content to compute main axis offset
        match container_style.justify_content {
            JustifyContent::Center => (container_main_size - child_main_size) / 2.0,
            JustifyContent::FlexEnd => container_main_size - child_main_size,
            _ => 0.0, // FlexStart or other values default to start
        }
    }

    /// Compute static position offset along the cross axis for abspos child in flex container.
    fn compute_flex_abspos_cross_offset(
        container_style: &ComputedStyle,
        child_style: &ComputedStyle,
        child_sides: &BoxSides,
        container_cross_size: f32,
        is_row: bool,
    ) -> f32 {
        use css_orchestrator::style_model::AlignItems;

        // Get child's cross size (border-box)
        let child_cross_size = if is_row {
            // Row: cross axis is block (height)
            child_style.height.unwrap_or(0.0)
                + child_sides.padding_top.to_px()
                + child_sides.padding_bottom.to_px()
                + child_sides.border_top.to_px()
                + child_sides.border_bottom.to_px()
        } else {
            // Column: cross axis is inline (width)
            child_style.width.map_or_else(
                || {
                    200.0
                        + child_sides.padding_left.to_px()
                        + child_sides.padding_right.to_px()
                        + child_sides.border_left.to_px()
                        + child_sides.border_right.to_px()
                },
                |width| {
                    width
                        + child_sides.padding_left.to_px()
                        + child_sides.padding_right.to_px()
                        + child_sides.border_left.to_px()
                        + child_sides.border_right.to_px()
                },
            )
        };

        // Apply align-items to compute cross axis offset
        match container_style.align_items {
            AlignItems::Center => (container_cross_size - child_cross_size) / 2.0,
            AlignItems::FlexEnd => container_cross_size - child_cross_size,
            _ => 0.0, // FlexStart or other values default to start
        }
    }

    /// Apply flex placements to children and compute actual cross size.
    fn apply_flex_placements(
        &mut self,
        child_styles: &[ChildStyleInfo],
        placements: &[(FlexPlacement, CrossPlacement)],
        params: &FlexPlacementParams,
    ) -> f32 {
        let mut actual_cross_size = 0.0f32;

        for (idx, (child, child_style, _)) in child_styles.iter().enumerate() {
            if let Some((main_placement, cross_placement)) = placements.get(idx) {
                let child_sides = compute_box_sides(child_style);

                // Convert f32 coordinates to LayoutUnit to preserve sub-pixel precision
                // Margins are already in LayoutUnit from BoxSides
                let final_inline_offset = if params.is_row {
                    params.content_base_inline + LayoutUnit::from_px(main_placement.main_offset)
                } else {
                    params.content_base_inline
                        + LayoutUnit::from_px(cross_placement.cross_offset)
                        + child_sides.margin_left
                };

                let cross_with_margin =
                    LayoutUnit::from_px(cross_placement.cross_offset) + child_sides.margin_top;
                let final_block_offset = if params.is_row {
                    params.content_base_block.map(|y| y + cross_with_margin)
                } else {
                    params
                        .content_base_block
                        .map(|y| y + LayoutUnit::from_px(main_placement.main_offset))
                };

                let (final_inline_size, final_block_size) = if params.is_row {
                    (main_placement.main_size, cross_placement.cross_size)
                } else {
                    (cross_placement.cross_size, main_placement.main_size)
                };

                let final_child_result = LayoutResult {
                    inline_size: final_inline_size,
                    block_size: final_block_size,
                    bfc_offset: BfcOffset::new(final_inline_offset, final_block_offset),
                    exclusion_space: ExclusionSpace::new(),
                    end_margin_strut: MarginStrut::default(),
                    baseline: None,
                    needs_relayout: false,
                };

                self.layout_results.insert(*child, final_child_result);
                actual_cross_size = actual_cross_size
                    .max(cross_placement.cross_offset + cross_placement.cross_size);
            }
        }

        actual_cross_size
    }

    /// Create empty flex container result (when no flex items).
    fn create_empty_flex_result(
        container_inline_size: f32,
        container_cross_size: f32,
        bfc_offset: BfcOffset,
        sides: &BoxSides,
    ) -> LayoutResult {
        let border_box_inline = container_inline_size
            + sides.padding_left.to_px()
            + sides.padding_right.to_px()
            + sides.border_left.to_px()
            + sides.border_right.to_px();

        let border_box_block = container_cross_size
            + sides.padding_top.to_px()
            + sides.padding_bottom.to_px()
            + sides.border_top.to_px()
            + sides.border_bottom.to_px();

        LayoutResult {
            inline_size: border_box_inline,
            block_size: border_box_block,
            bfc_offset,
            exclusion_space: ExclusionSpace::new(),
            end_margin_strut: MarginStrut::default(),
            baseline: None,
            needs_relayout: false,
        }
    }

    /// Layout absolutely positioned children in flex container.
    fn layout_flex_abspos_children(
        &mut self,
        abspos_children: &[NodeKey],
        bfc_offset: BfcOffset,
        container_sides: &BoxSides,
        params: &FlexResultParams,
    ) {
        let container_style = params.container_style.clone();
        for abspos_child in abspos_children {
            let abspos_child_style = self.style(*abspos_child);
            let abspos_child_sides = compute_box_sides(&abspos_child_style);

            let abspos_params = FlexAbsposConstraintParams {
                bfc_offset,
                sides: container_sides,
                container_style: &container_style,
                child_style: &abspos_child_style,
                child_sides: &abspos_child_sides,
                container_inline_size: params.container_inline_size,
                final_cross_size: params.final_cross_size,
                is_row: params.is_row,
            };
            let abspos_space = Self::create_flex_abspos_constraint_space(&abspos_params);

            let abspos_result = self.layout_absolute(
                *abspos_child,
                &abspos_space,
                &abspos_child_style,
                &abspos_child_sides,
            );

            self.layout_results.insert(*abspos_child, abspos_result);
        }
    }

    /// Create final flex container result.
    fn create_flex_result(
        params: &FlexResultParams,
        bfc_offset: BfcOffset,
        sides: &BoxSides,
    ) -> LayoutResult {
        // Both row and column use the same calculation:
        // border_box_inline is always CSS width (container_inline_size + horizontal edges)
        // border_box_block is always CSS height (final_cross_size + vertical edges)
        let (border_box_inline, border_box_block) = (
            params.container_inline_size
                + sides.padding_left.to_px()
                + sides.padding_right.to_px()
                + sides.border_left.to_px()
                + sides.border_right.to_px(),
            params.final_cross_size
                + sides.padding_top.to_px()
                + sides.padding_bottom.to_px()
                + sides.border_top.to_px()
                + sides.border_bottom.to_px(),
        );

        LayoutResult {
            inline_size: border_box_inline,
            block_size: border_box_block,
            bfc_offset,
            exclusion_space: ExclusionSpace::new(),
            end_margin_strut: MarginStrut::default(),
            baseline: None,
            needs_relayout: false,
        }
    }

    /// Check if a node is a text node.
    fn is_text_node(&self, node: NodeKey) -> bool {
        self.text_nodes.contains_key(&node)
    }

    /// Measure text node dimensions using actual font metrics.
    fn measure_text(
        &self,
        child_node: NodeKey,
        parent_node: Option<NodeKey>,
        available_inline: AvailableSize,
    ) -> (f32, f32) {
        let text = self.text_nodes.get(&child_node).map_or("", String::as_str);

        if text.is_empty() {
            return (0.0, 0.0);
        }

        // Text nodes inherit font properties from their parent
        // Try to get parent's style, fallback to default
        let style = parent_node.map_or_else(
            || self.styles.get(&child_node).cloned().unwrap_or_default(),
            |parent| self.styles.get(&parent).cloned().unwrap_or_default(),
        );

        // Use actual font metrics from css_text::measurement module
        let available_width = available_inline.resolve(self.icb_width).to_px();

        // Measure without wrapping first to see if text fits
        let metrics = measure_text(text, &style);

        // Check if text needs wrapping
        if available_width.is_finite() && available_width < 1e9 {
            if metrics.width <= available_width {
                // Text fits on one line - use actual width
                (metrics.width, metrics.height)
            } else {
                // Text needs wrapping - measure wrapped height and use available width
                let (height, _line_count) = measure_text_wrapped(text, &style, available_width);
                (available_width, height)
            }
        } else {
            // Measure without wrapping
            (metrics.width, metrics.height)
        }
    }

    /// Handle empty flex container case.
    fn handle_empty_flex_container(
        &mut self,
        abspos_children: &[NodeKey],
        bfc_offset: BfcOffset,
        sides: &BoxSides,
        result_params: &FlexResultParams,
    ) -> LayoutResult {
        if !abspos_children.is_empty() {
            self.layout_flex_abspos_children(abspos_children, bfc_offset, sides, result_params);
        }

        Self::create_empty_flex_result(
            result_params.container_inline_size,
            result_params.final_cross_size,
            bfc_offset,
            sides,
        )
    }

    /// Prepare flexbox container inputs.
    fn prepare_flex_container_inputs(
        flex_direction: FlexboxDirection,
        container_inline_size: f32,
        container_cross_size: f32,
        is_row: bool,
        style: &ComputedStyle,
    ) -> FlexContainerInputs {
        FlexContainerInputs {
            direction: flex_direction,
            writing_mode: WritingMode::HorizontalTb,
            container_main_size: if is_row {
                container_inline_size
            } else {
                container_cross_size
            },
            main_gap: if is_row {
                style.column_gap
            } else {
                style.row_gap
            },
        }
    }

    /// Run flexbox layout algorithm and get placements.
    fn run_flexbox_layout(
        flex_items: &[FlexChild],
        child_styles: &[ChildStyleInfo],
        params: &FlexboxLayoutParams,
        style: &ComputedStyle,
    ) -> Vec<(FlexPlacement, CrossPlacement)> {
        type BaselineInput = Option<(f32, f32)>;

        let align_items = match style.align_items {
            StyleAlignItems::Stretch => FlexAlignItems::Stretch,
            StyleAlignItems::FlexStart => FlexAlignItems::FlexStart,
            StyleAlignItems::Center => FlexAlignItems::Center,
            StyleAlignItems::FlexEnd => FlexAlignItems::FlexEnd,
        };

        let justify_content = match style.justify_content {
            StyleJustifyContent::FlexStart => FlexJustifyContent::Start,
            StyleJustifyContent::Center => FlexJustifyContent::Center,
            StyleJustifyContent::FlexEnd => FlexJustifyContent::End,
            StyleJustifyContent::SpaceBetween => FlexJustifyContent::SpaceBetween,
            StyleJustifyContent::SpaceAround => FlexJustifyContent::SpaceAround,
            StyleJustifyContent::SpaceEvenly => FlexJustifyContent::SpaceEvenly,
        };

        let align_content = match style.align_content {
            StyleAlignContent::Stretch => FlexAlignContent::Stretch,
            StyleAlignContent::FlexStart => FlexAlignContent::Start,
            StyleAlignContent::Center => FlexAlignContent::Center,
            StyleAlignContent::FlexEnd => FlexAlignContent::End,
            StyleAlignContent::SpaceBetween => FlexAlignContent::SpaceBetween,
            StyleAlignContent::SpaceAround => FlexAlignContent::SpaceAround,
            StyleAlignContent::SpaceEvenly => FlexAlignContent::SpaceEvenly,
        };

        let cross_inputs: Vec<(f32, f32, f32)> = child_styles
            .iter()
            .map(|(_, _, result)| {
                let cross_size = if params.is_row {
                    result.block_size
                } else {
                    result.inline_size
                };
                (cross_size, 0.0, 1e9)
            })
            .collect();

        let baseline_inputs: Vec<BaselineInput> = vec![None; flex_items.len()];

        let cross_ctx = CrossContext {
            align_items,
            align_content,
            container_cross_size: if params.is_row {
                params.container_cross_size
            } else {
                params.container_inline_size
            },
            cross_gap: if params.is_row {
                style.row_gap
            } else {
                style.column_gap
            },
        };

        if matches!(style.flex_wrap, StyleFlexWrap::NoWrap) {
            layout_single_line_with_cross(
                params.container_inputs,
                justify_content,
                cross_ctx,
                flex_items,
                CrossAndBaseline {
                    cross_inputs: &cross_inputs,
                    baseline_inputs: &baseline_inputs,
                },
            )
        } else {
            layout_multi_line_with_cross(
                params.container_inputs,
                justify_content,
                cross_ctx,
                flex_items,
                CrossAndBaseline {
                    cross_inputs: &cross_inputs,
                    baseline_inputs: &baseline_inputs,
                },
            )
        }
    }

    /// Layout a flex container using the proper flexbox algorithm.
    fn layout_flex(
        &mut self,
        node: NodeKey,
        constraint_space: &ConstraintSpace,
        style: &ComputedStyle,
        sides: &BoxSides,
    ) -> LayoutResult {
        let container_inline_size = self.compute_inline_size(node, constraint_space, style, sides);
        let container_cross_size = Self::compute_flex_container_cross_size(style, sides);

        let bfc_offset = BfcOffset::new(
            constraint_space.bfc_offset.inline_offset + sides.margin_left,
            constraint_space.bfc_offset.block_offset,
        );

        let flex_direction = match style.flex_direction {
            StyleFlexDirection::Row => FlexboxDirection::Row,
            StyleFlexDirection::Column => FlexboxDirection::Column,
        };

        let is_row = matches!(flex_direction, FlexboxDirection::Row);
        let children = normalize_children(&self.children, &self.styles, node);

        let (mut flex_items, child_styles, abspos_children) = self.build_flex_items(&children);

        if flex_items.is_empty() {
            let result_params = FlexResultParams {
                container_inline_size,
                final_cross_size: container_cross_size,
                is_row,
                container_style: style.clone(),
            };
            return self.handle_empty_flex_container(
                &abspos_children,
                bfc_offset,
                sides,
                &result_params,
            );
        }

        Self::update_flex_item_basis(&mut flex_items, &child_styles, is_row);

        let container_inputs = Self::prepare_flex_container_inputs(
            flex_direction,
            container_inline_size,
            container_cross_size,
            is_row,
            style,
        );

        let flexbox_params = FlexboxLayoutParams {
            container_inputs,
            is_row,
            container_inline_size,
            container_cross_size,
        };

        let placements =
            Self::run_flexbox_layout(&flex_items, &child_styles, &flexbox_params, style);

        let placement_params = FlexPlacementParams {
            content_base_inline: bfc_offset.inline_offset + sides.padding_left + sides.border_left,
            content_base_block: bfc_offset
                .block_offset
                .map(|y| y + sides.padding_top + sides.border_top),
            is_row,
        };

        let actual_cross_size =
            self.apply_flex_placements(&child_styles, &placements, &placement_params);

        let final_cross_size = if style.height.is_some() {
            container_cross_size
        } else {
            actual_cross_size
        };

        let result_params = FlexResultParams {
            container_inline_size,
            final_cross_size,
            is_row,
            container_style: style.clone(),
        };

        self.layout_flex_abspos_children(&abspos_children, bfc_offset, sides, &result_params);

        Self::create_flex_result(&result_params, bfc_offset, sides)
    }
}

/// Run layout on the entire tree starting from root.
pub fn layout_tree(tree: &mut ConstraintLayoutTree, root: NodeKey) -> LayoutResult {
    let initial_space = ConstraintSpace::new_for_root(tree.icb_width, tree.icb_height);
    tree.layout_block(root, &initial_space)
}
