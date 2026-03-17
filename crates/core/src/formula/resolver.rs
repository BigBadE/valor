//! Formula resolution with memoization.
//!
//! The resolver evaluates formulas to concrete pixel values, caching
//! results keyed by `(NodeId, formula_ptr)`. No separate dependency
//! tracking — the formula tree itself encodes dependencies. When a
//! value may be stale, evict the relevant cache entries and
//! re-resolve the formula.

use crate::{
    Aggregation, Formula, FormulaList, LineAggregateParams, LineItemAggregateParams, MeasureAxis,
    MeasureMode, MultiRelationship, NodeId, Operation, PrevLinesAggregateParams, QueryFn,
    SingleRelationship, Subpixel, TextMeasurement,
};
use lightningcss::properties::{Property, PropertyId};
use std::collections::HashMap;
use std::ptr::from_ref;

/// Per-node cache: maps formula pointer → resolved value.
type NodeCache = HashMap<usize, Subpixel>;

/// Trait for the interface the resolver needs from a styler.
///
/// This covers both the resolver's needs (property resolution, tree
/// navigation) and the query functions' needs (CSS property inspection,
/// intrinsic node detection).
pub trait StylerAccess {
    /// Query a CSS property for the current node, converted to pixels.
    fn get_property(&self, prop_id: &PropertyId<'static>) -> Option<Subpixel>;

    /// Query the raw CSS property for the current node (for display-mode dispatch).
    fn get_css_property(&self, prop_id: &PropertyId<'static>) -> Option<Property<'static>>;

    /// Get a boxed styler for a related node.
    fn related(&self, rel: SingleRelationship) -> Box<dyn StylerAccess>;

    /// Get boxed stylers for all nodes in a multi-relationship.
    fn related_iter(&self, rel: MultiRelationship) -> Vec<Box<dyn StylerAccess>>;

    /// Get the node ID this styler is scoped to.
    fn node_id(&self) -> NodeId;

    /// Get the viewport width in pixels.
    fn viewport_width(&self) -> u32;

    /// Get the viewport height in pixels.
    fn viewport_height(&self) -> u32;

    /// Get a styler for the root element (the `<html>` element).
    fn root(&self) -> Box<dyn StylerAccess>;

    /// Whether this node is an intrinsic node (text node, replaced element).
    /// Intrinsic nodes don't own their display type — they inherit layout
    /// behavior from their parent.
    fn is_intrinsic(&self) -> bool;

    /// Get the text content of this node, if it is a text node.
    fn text_content(&self) -> Option<String>;

    /// Measure text with the node's font properties.
    ///
    /// Returns full text metrics (width, height, ascent, descent).
    /// If `max_width` is `Some`, the text is wrapped at that width.
    /// If `None`, text is measured as a single unwrapped line.
    fn measure_text(&self, text: &str, max_width: Option<f32>) -> Option<TextMeasurement>;
}

/// Key for cached line assignments: (parent_node, item_main_size_fn_ptr, available_main_ptr, gap_ptr).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct LineCacheKey {
    parent: NodeId,
    main_size_fn: usize,
    available_ptr: usize,
    gap_ptr: usize,
}

/// Cached line assignment for a parent's children.
/// Each inner `Vec<usize>` contains the child indices on that line.
struct LineAssignment {
    lines: Vec<Vec<usize>>,
}

/// Context for formula resolution with memoization.
///
/// Caches resolved values keyed by `(NodeId, formula_ptr)`. The styler
/// is passed to each `resolve` call, keeping the context free of
/// lifetime parameters and allowing it to persist across resolution
/// passes.
pub struct ResolveContext {
    /// Cached resolved values: node → (formula_ptr → pixels).
    cache: HashMap<NodeId, NodeCache>,

    /// Cached line assignments: computed once per (parent, line-breaking params),
    /// reused by both `LineAggregate` and `LineItemAggregate`.
    line_cache: HashMap<LineCacheKey, LineAssignment>,

    /// Viewport width in pixels.
    pub viewport_width: u32,
    /// Viewport height in pixels.
    pub viewport_height: u32,

    /// Debug: recursion depth counter.
    depth: u32,
}

impl ResolveContext {
    /// Create a new resolve context.
    pub fn new(viewport_width: u32, viewport_height: u32) -> Self {
        Self {
            cache: HashMap::new(),
            line_cache: HashMap::new(),
            viewport_width,
            viewport_height,
            depth: 0,
        }
    }

    /// Invalidate cached values for specific formulas on a node.
    ///
    /// Given a list of formulas, removes only those formulas' cached values
    /// for the specified node. Used for selective invalidation when only
    /// certain CSS properties change.
    pub fn invalidate_formulas(&mut self, node: NodeId, formulas: &[&'static Formula]) {
        if let Some(node_cache) = self.cache.get_mut(&node) {
            for formula in formulas {
                let formula_ptr = from_ref::<Formula>(*formula) as usize;
                node_cache.remove(&formula_ptr);
            }
        }
        // Also invalidate line cache if any formula affects line breaking
        // (conservative: invalidate all line caches for this node)
        self.line_cache.retain(|key, _| key.parent != node);
    }

    /// Invalidate all cached values for a single node.
    ///
    /// Removes every cached formula result for this node and any line
    /// cache entries where this node is the parent. Other nodes' caches
    /// are untouched.
    pub fn invalidate_node(&mut self, node: NodeId) {
        self.cache.remove(&node);
        self.line_cache.retain(|key, _| key.parent != node);
    }

    /// Look up a previously resolved value from the cache.
    /// Returns `None` if the formula was never resolved for this node.
    pub fn get_cached(&self, formula: &'static Formula, node: NodeId) -> Option<Subpixel> {
        let formula_ptr = from_ref::<Formula>(formula) as usize;
        self.cache
            .get(&node)
            .and_then(|nc| nc.get(&formula_ptr))
            .copied()
    }

    /// Clear all caches. Call before starting a fresh resolution pass.
    pub fn clear_cache(&mut self) {
        self.cache.clear();
        self.line_cache.clear();
    }

    /// Resolve a formula for a node, using cache if available.
    pub fn resolve(
        &mut self,
        formula: &'static Formula,
        node: NodeId,
        styler: &dyn StylerAccess,
    ) -> Option<Subpixel> {
        let formula_ptr = from_ref::<Formula>(formula) as usize;

        if let Some(&cached) = self.cache.get(&node).and_then(|nc| nc.get(&formula_ptr)) {
            return Some(cached);
        }

        self.depth += 1;
        if self.depth > 200 {
            self.depth -= 1;
            return None;
        }
        let value = self.resolve_inner(formula, node, styler);
        self.depth -= 1;

        if let Some(val) = value {
            self.cache.entry(node).or_default().insert(formula_ptr, val);
            Some(val)
        } else {
            None
        }
    }

    /// Resolve a formula in the context of a different node.
    fn resolve_for_node(
        &mut self,
        formula: &'static Formula,
        target: &dyn StylerAccess,
    ) -> Option<Subpixel> {
        let target_node = target.node_id();
        self.resolve(formula, target_node, target)
    }

    /// Resolve a `Related` formula — navigate to a related node, query it
    /// for a formula, and resolve that formula in the target's context.
    fn resolve_related(
        &mut self,
        rel: SingleRelationship,
        query_fn: QueryFn,
        styler: &dyn StylerAccess,
    ) -> Option<Subpixel> {
        let target = styler.related(rel);
        let result_formula = query_fn(target.as_ref())?;
        self.resolve_for_node(result_formula, target.as_ref())
    }

    /// Internal resolve function.
    fn resolve_inner(
        &mut self,
        formula: &'static Formula,
        node: NodeId,
        styler: &dyn StylerAccess,
    ) -> Option<Subpixel> {
        match formula {
            Formula::Constant(value) => Some(*value),
            Formula::ViewportWidth => Some(Subpixel::from_px(self.viewport_width as i32)),
            Formula::ViewportHeight => Some(Subpixel::from_px(self.viewport_height as i32)),
            Formula::BinOp(oper, lhs, rhs) => {
                let lhs_val = self.resolve(lhs, node, styler)?;
                let rhs_val = self.resolve(rhs, node, styler)?;
                Some(match oper {
                    Operation::Add => lhs_val + rhs_val,
                    Operation::Sub => lhs_val - rhs_val,
                    Operation::Mul => lhs_val * rhs_val,
                    Operation::Div => {
                        if rhs_val == Subpixel::ZERO {
                            Subpixel::ZERO
                        } else {
                            lhs_val / rhs_val
                        }
                    }
                    Operation::Max => {
                        if lhs_val > rhs_val {
                            lhs_val
                        } else {
                            rhs_val
                        }
                    }
                    Operation::Min => {
                        if lhs_val < rhs_val {
                            lhs_val
                        } else {
                            rhs_val
                        }
                    }
                })
            }
            Formula::CssValue(prop_id) => styler.get_property(prop_id),
            Formula::CssValueOrDefault(prop_id, default) => {
                Some(styler.get_property(prop_id).unwrap_or(*default))
            }
            Formula::Related(rel, query_fn) => self.resolve_related(*rel, *query_fn, styler),
            Formula::Aggregate(agg, list) => self.resolve_aggregate(*agg, list, styler),
            Formula::InlineMeasure(axis, mode) => self.resolve_inline_measure(*axis, *mode, styler),
            Formula::LineAggregate(params) => self.resolve_line_aggregate(params, node, styler),
            Formula::LineItemAggregate(params) => {
                self.resolve_line_item_aggregate(params, node, styler)
            }
            Formula::PrevLinesAggregate(params) => {
                self.resolve_prev_lines_aggregate(params, node, styler)
            }
            Formula::Imperative(func) => {
                let results = func(node, styler, &mut |f, n, s| self.resolve(f, n, s))?;
                let formula_ptr = from_ref::<Formula>(formula) as usize;
                let mut my_value = None;
                for &(n, val) in &results {
                    self.cache.entry(n).or_default().insert(formula_ptr, val);
                    if n == node {
                        my_value = Some(val);
                    }
                }
                my_value
            }
        }
    }

    /// Compute the available inline width from the containing block.
    ///
    /// Walks up from the node to find the nearest block-level ancestor
    /// (the containing block per CSS 2.2 §10.1), then computes its
    /// content width (explicit width minus padding and border).
    /// Falls back to viewport width if no block ancestor is found.
    fn containing_block_width(&self, styler: &dyn StylerAccess) -> f32 {
        let mut current = styler.related(SingleRelationship::Parent);
        loop {
            // Check if this ancestor has a Display property.
            let display = current.get_css_property(&PropertyId::Display);
            let is_block = match &display {
                Some(Property::Display(display)) => {
                    use lightningcss::properties::display::{
                        Display, DisplayInside, DisplayOutside,
                    };
                    !matches!(
                        display,
                        Display::Pair(pair)
                            if matches!(pair.outside, DisplayOutside::Inline)
                                && matches!(pair.inside, DisplayInside::Flow)
                    )
                }
                // No Display property or non-Display property — treat as block.
                _ => true,
            };

            if is_block {
                let raw_width = current
                    .get_property(&PropertyId::Width)
                    .unwrap_or_else(|| Subpixel::from_px(self.viewport_width as i32));
                let padding_left = current
                    .get_property(&PropertyId::PaddingLeft)
                    .unwrap_or(Subpixel::ZERO);
                let padding_right = current
                    .get_property(&PropertyId::PaddingRight)
                    .unwrap_or(Subpixel::ZERO);
                let border_left = current
                    .get_property(&PropertyId::BorderLeftWidth)
                    .unwrap_or(Subpixel::ZERO);
                let border_right = current
                    .get_property(&PropertyId::BorderRightWidth)
                    .unwrap_or(Subpixel::ZERO);
                return (raw_width - padding_left - padding_right - border_left - border_right)
                    .max(Subpixel::ZERO)
                    .to_f32();
            }

            let parent = current.related(SingleRelationship::Parent);
            if parent.node_id() == current.node_id() {
                // Reached the root without finding a block ancestor.
                return self.viewport_width as f32;
            }
            current = parent;
        }
    }

    /// Resolve `InlineMeasure` for a single node.
    ///
    /// Handles all combinations of `MeasureAxis` × `MeasureMode`:
    /// - `FitAvailable`: wrap text to containing block width
    /// - `MinContent`: measure at max_width=0 (narrowest without overflow)
    /// - `MaxContent`: measure with no wrapping (single line)
    /// - `Baseline`: return ascent (distance from top to first baseline)
    fn resolve_inline_measure(
        &mut self,
        axis: MeasureAxis,
        mode: MeasureMode,
        styler: &dyn StylerAccess,
    ) -> Option<Subpixel> {
        // Text node: measure directly.
        if let Some(text) = styler.text_content() {
            if text.trim().is_empty() {
                return None;
            }

            // For MinContent width, we need the longest word's width.
            // measure_text with max_width=0 would break at every glyph,
            // so we split on whitespace and measure each word unwrapped.
            if mode == MeasureMode::MinContent && axis == MeasureAxis::Width {
                let mut max_word_width: f32 = 0.0;
                for word in text.split_whitespace() {
                    if let Some(wm) = styler.measure_text(word, None) {
                        if wm.width > max_word_width {
                            max_word_width = wm.width;
                        }
                    }
                }
                return Some(Subpixel::from_f32(max_word_width));
            }

            let max_width = match mode {
                MeasureMode::FitAvailable => Some(self.containing_block_width(styler)),
                MeasureMode::MinContent => Some(0.0), // height at narrowest
                MeasureMode::MaxContent | MeasureMode::Baseline => None,
            };

            let m = styler.measure_text(&text, max_width)?;

            return Some(Subpixel::from_f32(match (axis, mode) {
                (MeasureAxis::Width, _) => m.width,
                (MeasureAxis::Height, MeasureMode::Baseline) => m.ascent,
                (MeasureAxis::Height, _) => m.height,
            }));
        }

        // Inline element (e.g. <span>): recurse into children.
        let children = styler.related_iter(MultiRelationship::Children);
        if children.is_empty() {
            return None;
        }

        match axis {
            MeasureAxis::Width => {
                // Sum children's widths (inline elements flow horizontally).
                let mut total = Subpixel::ZERO;
                for child in &children {
                    if let Some(val) = self.resolve_inline_measure(axis, mode, child.as_ref()) {
                        total = total + val;
                    }
                }
                Some(total)
            }
            MeasureAxis::Height => {
                // Max of children's heights (tallest child determines line height).
                let mut max_val = Subpixel::ZERO;
                for child in &children {
                    if let Some(val) = self.resolve_inline_measure(axis, mode, child.as_ref())
                        && val > max_val
                    {
                        max_val = val;
                    }
                }
                Some(max_val)
            }
        }
    }

    /// Resolve an aggregate formula.
    fn resolve_aggregate(
        &mut self,
        agg: Aggregation,
        list: &'static FormulaList,
        styler: &dyn StylerAccess,
    ) -> Option<Subpixel> {
        let FormulaList::Related(rel, query_fn) = list;
        let targets = styler.related_iter(*rel);

        // Standard aggregation: resolve each formula and aggregate.
        let mut values = Vec::new();
        for target in &targets {
            if let Some(formula) = query_fn(target.as_ref()) {
                if let Some(val) = self.resolve_for_node(formula, target.as_ref()) {
                    values.push(val);
                }
            }
        }

        Some(aggregate_values(agg, &values))
    }

    // ========================================================================
    // Line-breaking aggregate resolution
    // ========================================================================

    /// Compute or retrieve cached line assignments for a parent's children.
    ///
    /// Groups children into lines by walking them in order and accumulating
    /// their main-axis sizes. When the accumulated size (plus gaps) exceeds
    /// `available_main`, a new line starts.
    fn compute_line_assignments(
        &mut self,
        parent_node: NodeId,
        parent_styler: &dyn StylerAccess,
        children: &[Box<dyn StylerAccess>],
        item_main_size: QueryFn,
        available_main: &'static Formula,
        gap: &'static Formula,
    ) -> Vec<Vec<usize>> {
        let line_key = LineCacheKey {
            parent: parent_node,
            main_size_fn: item_main_size as usize,
            available_ptr: from_ref::<Formula>(available_main) as usize,
            gap_ptr: from_ref::<Formula>(gap) as usize,
        };

        if let Some(cached) = self.line_cache.get(&line_key) {
            return cached.lines.clone();
        }

        let available = self
            .resolve(available_main, parent_node, parent_styler)
            .unwrap_or(Subpixel::from_px(self.viewport_width as i32))
            .to_f32();
        let gap_val = self
            .resolve(gap, parent_node, parent_styler)
            .unwrap_or(Subpixel::ZERO)
            .to_f32();

        let mut lines: Vec<Vec<usize>> = vec![Vec::new()];
        let mut line_used: f32 = 0.0;

        // The children array is in reverse DOM order (last-appended first,
        // from the prepend-to-head linked list). Line-breaking must process
        // items in DOM order, so iterate in reverse. We still store the
        // original indices so callers can look up `children[idx]`.
        let dom_order: Vec<usize> = (0..children.len()).rev().collect();

        for &idx in &dom_order {
            let child = &children[idx];
            let child_formula = item_main_size(child.as_ref());

            // If the main-size query returns None, this item forces a line
            // break (e.g., a block item inside an inline formatting context).
            // Flush the current line, add this item as its own line, and
            // start a fresh line for subsequent items.
            let Some(formula) = child_formula else {
                if !lines.last().unwrap().is_empty() {
                    lines.push(Vec::new());
                }
                lines.last_mut().unwrap().push(idx);
                lines.push(Vec::new());
                line_used = 0.0;
                continue;
            };

            let child_size = self
                .resolve_for_node(formula, child.as_ref())
                .unwrap_or(Subpixel::ZERO)
                .to_f32();

            let current_line = lines.last().unwrap();
            let needed = if current_line.is_empty() {
                child_size
            } else {
                child_size + gap_val
            };

            if !current_line.is_empty() && line_used + needed > available {
                // Break to new line.
                lines.push(Vec::new());
                line_used = child_size;
            } else {
                line_used += needed;
            }

            lines.last_mut().unwrap().push(idx);
        }

        // Remove trailing empty line (from block-item flush).
        if lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        if lines.is_empty() {
            lines.push(Vec::new());
        }

        self.line_cache.insert(
            line_key,
            LineAssignment {
                lines: lines.clone(),
            },
        );

        lines
    }

    /// Resolve a `LineAggregate` formula.
    ///
    /// Groups children into lines, aggregates values within each line
    /// using `within_line_agg`, then aggregates across lines using `line_agg`.
    fn resolve_line_aggregate(
        &mut self,
        params: &LineAggregateParams,
        node: NodeId,
        styler: &dyn StylerAccess,
    ) -> Option<Subpixel> {
        let children = styler.related_iter(MultiRelationship::Children);
        if children.is_empty() {
            return Some(Subpixel::ZERO);
        }

        let lines = self.compute_line_assignments(
            node,
            styler,
            &children,
            params.item_main_size,
            params.available_main,
            params.gap,
        );

        let line_gap_val = self
            .resolve(params.line_gap, node, styler)
            .unwrap_or(Subpixel::ZERO);

        let mut line_values: Vec<Subpixel> = Vec::new();

        for line_indices in &lines {
            let mut item_values: Vec<Subpixel> = Vec::new();

            for &child_idx in line_indices {
                let child = children[child_idx].as_ref();
                if let Some(formula) = (params.item_value)(child) {
                    if let Some(val) = self.resolve_for_node(formula, child) {
                        item_values.push(val);
                    }
                }
            }

            let line_val = aggregate_values(params.within_line_agg, &item_values);
            line_values.push(line_val);
        }

        // Add line gaps between lines.
        let total_line_gap = if lines.len() > 1 {
            line_gap_val * Subpixel::raw(lines.len() as i32 - 1)
        } else {
            Subpixel::ZERO
        };

        let result = aggregate_values(params.line_agg, &line_values);
        Some(result + total_line_gap)
    }

    /// Resolve a `LineItemAggregate` formula.
    ///
    /// Computes line assignments for the parent's children, determines
    /// which line the current item is on, then aggregates only over the
    /// siblings on that same line according to the specified relationship.
    fn resolve_line_item_aggregate(
        &mut self,
        params: &LineItemAggregateParams,
        node: NodeId,
        styler: &dyn StylerAccess,
    ) -> Option<Subpixel> {
        // Get the parent to compute line assignments.
        let parent = styler.related(SingleRelationship::Parent);
        let parent_node = parent.node_id();
        let all_children = parent.related_iter(MultiRelationship::Children);

        if all_children.is_empty() {
            return Some(Subpixel::ZERO);
        }

        let lines = self.compute_line_assignments(
            parent_node,
            parent.as_ref(),
            &all_children,
            params.item_main_size,
            params.available_main,
            params.gap,
        );

        // Find which line the current node is on and its position within.
        let mut my_line_idx = 0;
        let mut my_pos_in_line = 0;
        'outer: for (line_idx, line) in lines.iter().enumerate() {
            for (pos, &child_idx) in line.iter().enumerate() {
                if all_children[child_idx].node_id() == node {
                    my_line_idx = line_idx;
                    my_pos_in_line = pos;
                    break 'outer;
                }
            }
        }

        let my_line = &lines[my_line_idx];

        // Determine which children to aggregate based on the relationship.
        let target_indices: Vec<usize> = match params.relationship {
            MultiRelationship::PrevSiblings | MultiRelationship::OrderedPrevSiblings => {
                my_line[..my_pos_in_line].to_vec()
            }
            MultiRelationship::NextSiblings => {
                if my_pos_in_line + 1 < my_line.len() {
                    my_line[my_pos_in_line + 1..].to_vec()
                } else {
                    Vec::new()
                }
            }
            MultiRelationship::Children
            | MultiRelationship::OrderedChildren
            | MultiRelationship::Siblings => {
                // All items on the same line (including self for Children,
                // excluding self for Siblings).
                if matches!(params.relationship, MultiRelationship::Siblings) {
                    my_line
                        .iter()
                        .copied()
                        .filter(|&idx| all_children[idx].node_id() != node)
                        .collect()
                } else {
                    my_line.to_vec()
                }
            }
        };

        let mut values: Vec<Subpixel> = Vec::new();
        for child_idx in &target_indices {
            let child = all_children[*child_idx].as_ref();
            if let Some(formula) = (params.query)(child) {
                if let Some(val) = self.resolve_for_node(formula, child) {
                    values.push(val);
                }
            }
        }

        Some(aggregate_values(params.agg, &values))
    }

    /// Resolve a `PrevLinesAggregate` formula.
    ///
    /// Computes line assignments for the parent's children, finds which
    /// line the current item is on, then aggregates values from all
    /// *previous* lines (lines before the current item's line).
    ///
    /// Within each previous line, item values are aggregated using
    /// `within_line_agg` (e.g., Max for cross sizes). Across previous
    /// lines, the per-line values are aggregated using `line_agg`
    /// (e.g., Sum to get total cross offset). Line gaps are added
    /// between previous lines.
    fn resolve_prev_lines_aggregate(
        &mut self,
        params: &PrevLinesAggregateParams,
        node: NodeId,
        styler: &dyn StylerAccess,
    ) -> Option<Subpixel> {
        let parent = styler.related(SingleRelationship::Parent);
        let parent_node = parent.node_id();
        let all_children = parent.related_iter(MultiRelationship::Children);

        if all_children.is_empty() {
            return Some(Subpixel::ZERO);
        }

        let lines = self.compute_line_assignments(
            parent_node,
            parent.as_ref(),
            &all_children,
            params.item_main_size,
            params.available_main,
            params.gap,
        );

        // Find which line the current node is on.
        let mut my_line_idx = 0;
        'outer: for (line_idx, line) in lines.iter().enumerate() {
            for &child_idx in line {
                if all_children[child_idx].node_id() == node {
                    my_line_idx = line_idx;
                    break 'outer;
                }
            }
        }

        // If on line 0, there are no previous lines.
        if my_line_idx == 0 {
            return Some(Subpixel::ZERO);
        }

        // Aggregate values from lines 0..my_line_idx.
        let mut line_values: Vec<Subpixel> = Vec::new();
        for line_indices in &lines[..my_line_idx] {
            let mut item_values: Vec<Subpixel> = Vec::new();
            for &child_idx in line_indices {
                let child = all_children[child_idx].as_ref();
                if let Some(formula) = (params.item_value)(child)
                    && let Some(val) = self.resolve_for_node(formula, child)
                {
                    item_values.push(val);
                }
            }
            let line_val = aggregate_values(params.within_line_agg, &item_values);
            line_values.push(line_val);
        }

        let line_gap_val = self
            .resolve(params.line_gap, parent_node, parent.as_ref())
            .unwrap_or(Subpixel::ZERO);

        // Add line gaps between previous lines.
        let total_line_gap = if my_line_idx > 0 {
            line_gap_val * Subpixel::raw(my_line_idx as i32)
        } else {
            Subpixel::ZERO
        };

        let result = aggregate_values(params.line_agg, &line_values);
        Some(result + total_line_gap)
    }
}

/// Aggregate a slice of values using the given aggregation mode.
fn aggregate_values(agg: Aggregation, values: &[Subpixel]) -> Subpixel {
    match agg {
        Aggregation::Sum => values.iter().copied().sum(),
        Aggregation::Max => values.iter().copied().max().unwrap_or(Subpixel::ZERO),
        Aggregation::Min => values.iter().copied().min().unwrap_or(Subpixel::ZERO),
        Aggregation::Average => {
            if values.is_empty() {
                Subpixel::ZERO
            } else {
                let sum: Subpixel = values.iter().copied().sum();
                sum / Subpixel::raw(values.len() as i32)
            }
        }
        Aggregation::Count => Subpixel::raw(values.len() as i32),
    }
}
