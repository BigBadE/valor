//! Formula resolution with memoization.
//!
//! The resolver evaluates formulas to concrete pixel values, caching
//! results keyed by `(NodeId, formula_ptr)`. No separate dependency
//! tracking — the formula tree itself encodes dependencies. When a
//! value may be stale, evict the relevant cache entries and
//! re-resolve the formula.

use crate::{
    Aggregation, Formula, FormulaList, LineAggregateParams, LineItemAggregateParams, MeasureAxis,
    MeasureMode, MultiRelationship, NodeId, Operation, PrevLinesAggregateParams,
    PropertyResolver, QueryFn, SingleRelationship, Subpixel,
};
use lightningcss::properties::{Property, PropertyId};
use lightningcss::vendor_prefix::VendorPrefix;
use std::collections::HashMap;
use std::ptr::from_ref;

/// Per-node cache: maps formula pointer → resolved value.
type NodeCache = HashMap<usize, Subpixel>;

/// Font-size formula resolved explicitly by InlineMeasure.
/// Public so the renderer can use the same pointer for invalidation.
pub static FONT_SIZE_FORMULA: Formula = Formula::CssValue(PropertyId::FontSize);

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

/// Key for cached prefix-sum computations: (parent_node, aggregate_formula_ptr).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct PrefixKey {
    parent: NodeId,
    formula_ptr: usize,
}

/// Cached prefix values for all children of a parent.
/// Each child's value is the aggregate of all previous siblings.
struct PrefixValues {
    /// Map from child NodeId to the prefix aggregate up to (not including) that child.
    values: HashMap<NodeId, Subpixel>,
}

/// Context for formula resolution with memoization.
///
/// Caches resolved values keyed by `(NodeId, formula_ptr)`. The property
/// resolver is passed to each `resolve` call, keeping the context free of
/// lifetime parameters and allowing it to persist across resolution passes.
pub struct ResolveContext {
    /// Cached resolved values: node → (formula_ptr → pixels).
    cache: HashMap<NodeId, NodeCache>,

    /// Cached line assignments: computed once per (parent, line-breaking params),
    /// reused by both `LineAggregate` and `LineItemAggregate`.
    line_cache: HashMap<LineCacheKey, LineAssignment>,

    /// Cached prefix-sum/count/max computations for aggregates over PrevSiblings.
    /// Computed once per (parent, aggregate_formula), reused for all children.
    prefix_cache: HashMap<PrefixKey, PrefixValues>,

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
            prefix_cache: HashMap::new(),
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
        // Invalidate line cache and prefix cache for this node as parent,
        // since children's values may have changed.
        self.line_cache.retain(|key, _| key.parent != node);
        self.prefix_cache.retain(|key, _| key.parent != node);
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

    /// Invalidate aggregate caches (prefix sums, line assignments) for a parent.
    pub fn invalidate_parent_aggregates(&mut self, parent: NodeId) {
        self.prefix_cache.retain(|key, _| key.parent != parent);
        self.line_cache.retain(|key, _| key.parent != parent);
    }

    /// Clear all caches. Call before starting a fresh resolution pass.
    pub fn clear_cache(&mut self) {
        self.cache.clear();
        self.line_cache.clear();
        self.prefix_cache.clear();
    }

    /// Resolve a formula for a node. No caching.
    pub fn resolve(
        &mut self,
        formula: &'static Formula,
        node: NodeId,
        ctx: &dyn PropertyResolver,
    ) -> Option<Subpixel> {
        self.depth += 1;
        if self.depth > 200 {
            self.depth -= 1;
            return None;
        }
        let value = self.resolve_inner(formula, node, ctx);
        self.depth -= 1;
        value
    }

    /// Navigate to a related node from `node` using a `SingleRelationship`.
    fn navigate_single(
        &self,
        node: NodeId,
        rel: SingleRelationship,
        ctx: &dyn PropertyResolver,
    ) -> NodeId {
        match rel {
            SingleRelationship::Self_ => node,
            SingleRelationship::Parent => ctx.parent(node).unwrap_or(NodeId(0)),
            SingleRelationship::PrevSibling => {
                // Find the closest previous element sibling that participates
                // in layout (must be a DOM element, not text/comment/display:none).
                ctx.prev_siblings(node)
                    .into_iter()
                    .find(|&id| {
                        ctx.is_element(id)
                            && !matches!(
                                ctx.get_css_property(id, &PropertyId::Display),
                                Some(Property::Display(
                                    lightningcss::properties::display::Display::Keyword(
                                        lightningcss::properties::display::DisplayKeyword::None,
                                    ),
                                ))
                            )
                    })
                    .unwrap_or(node)
            }
            SingleRelationship::BlockContainer => {
                // Walk up ancestors to find the nearest block container
                // (an ancestor whose display is not inline).
                let mut current = node;
                while let Some(parent_id) = ctx.parent(current) {
                    let display = ctx.get_css_property(parent_id, &PropertyId::Display);
                    let is_inline = matches!(
                        display,
                        Some(Property::Display(
                            lightningcss::properties::display::Display::Pair(pair)
                        )) if matches!(
                            pair.outside,
                            lightningcss::properties::display::DisplayOutside::Inline
                        ) && matches!(
                            pair.inside,
                            lightningcss::properties::display::DisplayInside::Flow
                        )
                    );
                    if !is_inline {
                        return parent_id;
                    }
                    current = parent_id;
                }
                // Fallback: root node.
                NodeId::ROOT
            }
        }
    }

    /// Navigate to multiple related nodes from `node` using a `MultiRelationship`.
    fn navigate_multi(
        &self,
        node: NodeId,
        rel: MultiRelationship,
        ctx: &dyn PropertyResolver,
    ) -> Vec<NodeId> {
        /// Get CSS `order` property value for a node (defaults to 0).
        fn get_order(node: NodeId, ctx: &dyn PropertyResolver) -> i32 {
            match ctx.get_css_property(node, &PropertyId::Order(VendorPrefix::None)) {
                Some(Property::Order(val, _)) => val,
                _ => 0,
            }
        }

        match rel {
            MultiRelationship::Children => ctx.children(node),
            MultiRelationship::PrevSiblings => ctx.prev_siblings(node),
            MultiRelationship::NextSiblings => ctx.next_siblings(node),
            MultiRelationship::Siblings => {
                let mut all = ctx.prev_siblings(node);
                all.extend(ctx.next_siblings(node));
                all
            }
            MultiRelationship::OrderedChildren => {
                // children() returns reverse DOM order; reverse to DOM order
                // before sorting so items with equal `order` keep DOM order.
                let mut children = ctx.children(node);
                children.reverse();
                children.sort_by_key(|&id| get_order(id, ctx));
                children
            }
            MultiRelationship::OrderedPrevSiblings => {
                // Get parent, then all siblings sorted by order.
                // "Previous" = all siblings before this node in sorted order.
                let parent = ctx.parent(node).unwrap_or(node);
                let mut siblings = ctx.children(parent);
                siblings.reverse(); // reverse DOM order → DOM order
                siblings.sort_by_key(|&id| get_order(id, ctx));
                let pos = siblings.iter().position(|&id| id == node).unwrap_or(0);
                siblings[..pos].to_vec()
            }
        }
    }

    /// Resolve a `Related` formula — navigate to a related node, query it
    /// for a formula, and resolve that formula in the target's context.
    fn resolve_related(
        &mut self,
        node: NodeId,
        rel: SingleRelationship,
        query_fn: QueryFn,
        ctx: &dyn PropertyResolver,
    ) -> Option<Subpixel> {
        let target = self.navigate_single(node, rel, ctx);
        let result_formula = query_fn(target, ctx)?;
        self.resolve(result_formula, target, ctx)
    }

    /// Internal resolve function.
    fn resolve_inner(
        &mut self,
        formula: &'static Formula,
        node: NodeId,
        ctx: &dyn PropertyResolver,
    ) -> Option<Subpixel> {
        match formula {
            Formula::Constant(value) => Some(*value),
            Formula::ViewportWidth => Some(Subpixel::from_px(self.viewport_width as i32)),
            Formula::ViewportHeight => Some(Subpixel::from_px(self.viewport_height as i32)),
            Formula::BinOp(oper, lhs, rhs) => {
                let lhs_val = self.resolve(lhs, node, ctx)?;
                let rhs_val = self.resolve(rhs, node, ctx)?;
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
            Formula::CssValue(prop_id) => ctx.get_property(node, prop_id),
            Formula::CssValueOrDefault(prop_id, default) => {
                Some(ctx.get_property(node, prop_id).unwrap_or(*default))
            }
            Formula::Related(rel, query_fn) => {
                self.resolve_related(node, *rel, *query_fn, ctx)
            }
            Formula::Aggregate(agg, list) => self.resolve_aggregate(*agg, list, node, ctx),
            Formula::InlineMeasure(axis, mode) => {
                self.resolve_inline_measure(*axis, *mode, node, ctx)
            }
            Formula::LineAggregate(params) => self.resolve_line_aggregate(params, node, ctx),
            Formula::LineItemAggregate(params) => {
                self.resolve_line_item_aggregate(params, node, ctx)
            }
            Formula::PrevLinesAggregate(params) => {
                self.resolve_prev_lines_aggregate(params, node, ctx)
            }
            Formula::Imperative(func) => {
                let results = func(node, ctx, &mut |f, n| self.resolve(f, n, ctx))?;
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
    fn containing_block_width(&self, node: NodeId, ctx: &dyn PropertyResolver) -> f32 {
        let mut current = ctx.parent(node).unwrap_or(NodeId(0));
        loop {
            // Check if this ancestor has a Display property.
            let display = ctx.get_css_property(current, &PropertyId::Display);
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
                let raw_width = ctx
                    .get_property(current, &PropertyId::Width)
                    .unwrap_or_else(|| Subpixel::from_px(self.viewport_width as i32));
                let padding_left = ctx
                    .get_property(current, &PropertyId::PaddingLeft)
                    .unwrap_or(Subpixel::ZERO);
                let padding_right = ctx
                    .get_property(current, &PropertyId::PaddingRight)
                    .unwrap_or(Subpixel::ZERO);
                let border_left = ctx
                    .get_property(current, &PropertyId::BorderLeftWidth)
                    .unwrap_or(Subpixel::ZERO);
                let border_right = ctx
                    .get_property(current, &PropertyId::BorderRightWidth)
                    .unwrap_or(Subpixel::ZERO);
                return (raw_width - padding_left - padding_right - border_left - border_right)
                    .max(Subpixel::ZERO)
                    .to_f32();
            }

            let parent = ctx.parent(current);
            match parent {
                Some(p) if p != current => current = p,
                _ => return self.viewport_width as f32,
            }
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
        node: NodeId,
        ctx: &dyn PropertyResolver,
    ) -> Option<Subpixel> {
        // Text node: measure directly.
        if let Some(text) = ctx.text_content(node) {
            if text.trim().is_empty() {
                return None;
            }

            // Resolve font-size through the formula cache — explicit dependency.
            let font_size = self
                .resolve(&FONT_SIZE_FORMULA, node, ctx)
                .unwrap_or(Subpixel::from_px(16))
                .to_f32();

            // For MinContent width, we need the longest word's width.
            if mode == MeasureMode::MinContent && axis == MeasureAxis::Width {
                let mut max_word_width: f32 = 0.0;
                for word in text.split_whitespace() {
                    if let Some(wm) = ctx.measure_text(node, word, font_size, None) {
                        if wm.width > max_word_width {
                            max_word_width = wm.width;
                        }
                    }
                }
                return Some(Subpixel::from_f32(max_word_width));
            }

            let max_width = match mode {
                MeasureMode::FitAvailable => Some(self.containing_block_width(node, ctx)),
                MeasureMode::MinContent => Some(0.0),
                MeasureMode::MaxContent | MeasureMode::Baseline => None,
            };

            let m = ctx.measure_text(node, &text, font_size, max_width)?;

            return Some(Subpixel::from_f32(match (axis, mode) {
                (MeasureAxis::Width, _) => m.width,
                (MeasureAxis::Height, MeasureMode::Baseline) => m.ascent,
                (MeasureAxis::Height, _) => m.height,
            }));
        }

        // Inline element (e.g. <span>): recurse into children.
        let children = ctx.children(node);
        if children.is_empty() {
            return None;
        }

        match axis {
            MeasureAxis::Width => {
                // Sum children's widths (inline elements flow horizontally).
                let mut total = Subpixel::ZERO;
                for &child in &children {
                    if let Some(val) = self.resolve_inline_measure(axis, mode, child, ctx) {
                        total = total + val;
                    }
                }
                Some(total)
            }
            MeasureAxis::Height => {
                // Max of children's heights (tallest child determines line height).
                let mut max_val = Subpixel::ZERO;
                for &child in &children {
                    if let Some(val) = self.resolve_inline_measure(axis, mode, child, ctx)
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
    ///
    /// For `Sum` and `Count` aggregates over `PrevSiblings` / `OrderedPrevSiblings`,
    /// uses prefix caching: compute prefix sums for ALL children of the parent
    /// in one pass, then look up the result for any child in O(1).
    fn resolve_aggregate(
        &mut self,
        agg: Aggregation,
        list: &'static FormulaList,
        node: NodeId,
        ctx: &dyn PropertyResolver,
    ) -> Option<Subpixel> {
        let FormulaList::Related(rel, query_fn) = list;

        // Check if this is a prefix-cacheable pattern: Sum or Count over PrevSiblings.
        let is_prev_sibling_agg = matches!(
            rel,
            MultiRelationship::PrevSiblings | MultiRelationship::OrderedPrevSiblings
        );
        let is_prefix_cacheable =
            is_prev_sibling_agg && matches!(agg, Aggregation::Sum | Aggregation::Count);

        if is_prefix_cacheable {
            // Use the aggregate formula's list pointer as part of the cache key.
            // This uniquely identifies this specific aggregate (same query_fn + relationship).
            let formula_ptr = from_ref::<FormulaList>(list) as usize;

            if let Some(parent) = ctx.parent(node) {
                let prefix_key = PrefixKey {
                    parent,
                    formula_ptr,
                };

                // Check if prefix values are already computed for this parent.
                if let Some(prefix) = self.prefix_cache.get(&prefix_key) {
                    if let Some(&val) = prefix.values.get(&node) {
                        return Some(val);
                    }
                }

                // Compute prefix values for all children of the parent.
                let all_children = if matches!(rel, MultiRelationship::OrderedPrevSiblings) {
                    self.navigate_multi(parent, MultiRelationship::OrderedChildren, ctx)
                } else {
                    // PrevSiblings: need children in DOM order.
                    let mut c = ctx.children(parent);
                    c.reverse();
                    c
                };

                let mut prefix_values = HashMap::new();
                let mut running = Subpixel::ZERO;

                for &child in &all_children {
                    // Store the prefix value BEFORE this child's contribution.
                    prefix_values.insert(child, running);

                    // Compute this child's contribution.
                    if let Some(formula) = query_fn(child, ctx) {
                        if let Some(val) = self.resolve(formula, child, ctx) {
                            match agg {
                                Aggregation::Sum => running = running + val,
                                Aggregation::Count => {
                                    running = running + Subpixel::raw(1);
                                }
                                _ => unreachable!(),
                            }
                        }
                    }
                }

                self.prefix_cache.insert(prefix_key, PrefixValues {
                    values: prefix_values,
                });

                // Return the value for the requested node.
                return self
                    .prefix_cache
                    .get(&prefix_key)
                    .and_then(|pv| pv.values.get(&node).copied());
            }
        }

        // Non-prefix path: standard aggregation.
        let targets = self.navigate_multi(node, *rel, ctx);
        let mut values = Vec::new();
        for &target in &targets {
            if let Some(formula) = query_fn(target, ctx) {
                if let Some(val) = self.resolve(formula, target, ctx) {
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
        children: &[NodeId],
        item_main_size: QueryFn,
        available_main: &'static Formula,
        gap: &'static Formula,
        ctx: &dyn PropertyResolver,
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
            .resolve(available_main, parent_node, ctx)
            .unwrap_or(Subpixel::from_px(self.viewport_width as i32))
            .to_f32();
        let gap_val = self
            .resolve(gap, parent_node, ctx)
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
            let child = children[idx];
            let child_formula = item_main_size(child, ctx);

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
                .resolve(formula, child, ctx)
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
        ctx: &dyn PropertyResolver,
    ) -> Option<Subpixel> {
        let children = ctx.children(node);
        if children.is_empty() {
            return Some(Subpixel::ZERO);
        }

        let lines = self.compute_line_assignments(
            node,
            &children,
            params.item_main_size,
            params.available_main,
            params.gap,
            ctx,
        );

        let line_gap_val = self
            .resolve(params.line_gap, node, ctx)
            .unwrap_or(Subpixel::ZERO);

        let mut line_values: Vec<Subpixel> = Vec::new();

        for line_indices in &lines {
            let mut item_values: Vec<Subpixel> = Vec::new();

            for &child_idx in line_indices {
                let child = children[child_idx];
                if let Some(formula) = (params.item_value)(child, ctx) {
                    if let Some(val) = self.resolve(formula, child, ctx) {
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
        ctx: &dyn PropertyResolver,
    ) -> Option<Subpixel> {
        // Get the parent to compute line assignments.
        let parent_node = ctx.parent(node).unwrap_or(NodeId(0));
        let all_children = ctx.children(parent_node);

        if all_children.is_empty() {
            return Some(Subpixel::ZERO);
        }

        let lines = self.compute_line_assignments(
            parent_node,
            &all_children,
            params.item_main_size,
            params.available_main,
            params.gap,
            ctx,
        );

        // Find which line the current node is on and its position within.
        let mut my_line_idx = 0;
        let mut my_pos_in_line = 0;
        'outer: for (line_idx, line) in lines.iter().enumerate() {
            for (pos, &child_idx) in line.iter().enumerate() {
                if all_children[child_idx] == node {
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
                        .filter(|&idx| all_children[idx] != node)
                        .collect()
                } else {
                    my_line.to_vec()
                }
            }
        };

        let mut values: Vec<Subpixel> = Vec::new();
        for &child_idx in &target_indices {
            let child = all_children[child_idx];
            if let Some(formula) = (params.query)(child, ctx) {
                if let Some(val) = self.resolve(formula, child, ctx) {
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
        ctx: &dyn PropertyResolver,
    ) -> Option<Subpixel> {
        let parent_node = ctx.parent(node).unwrap_or(NodeId(0));
        let all_children = ctx.children(parent_node);

        if all_children.is_empty() {
            return Some(Subpixel::ZERO);
        }

        let lines = self.compute_line_assignments(
            parent_node,
            &all_children,
            params.item_main_size,
            params.available_main,
            params.gap,
            ctx,
        );

        // Find which line the current node is on.
        let mut my_line_idx = 0;
        'outer: for (line_idx, line) in lines.iter().enumerate() {
            for &child_idx in line {
                if all_children[child_idx] == node {
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
                let child = all_children[child_idx];
                if let Some(formula) = (params.item_value)(child, ctx)
                    && let Some(val) = self.resolve(formula, child, ctx)
                {
                    item_values.push(val);
                }
            }
            let line_val = aggregate_values(params.within_line_agg, &item_values);
            line_values.push(line_val);
        }

        let line_gap_val = self
            .resolve(params.line_gap, parent_node, ctx)
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
