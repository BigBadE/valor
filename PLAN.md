# Formula System Evolution Plan

## Problem Statement

Three flex layout features cannot be expressed in the current formula system:

1. **Min/max constraint clamping with redistribution (§9.7)** — The spec requires iterative freeze-and-redistribute: compute tentative sizes, clamp items that violate min/max, freeze them, redistribute remaining free space among unfrozen items, repeat until no violations remain. This is inherently iterative and cannot be expressed declaratively.

2. **Automatic minimum size (§4.5)** — `min-width: auto` on flex items computes to a content-based minimum (the min-content size), not zero. This requires intrinsic sizing infrastructure.

3. **Baseline alignment** — `align-items: baseline` requires querying each item's first-line text baseline, computing a per-line max baseline, and offsetting items accordingly.

---

## Existing Inline Measurement Infrastructure

The text measurement layer already supports everything we need:

| Call | What it gives | CSS concept |
|------|--------------|-------------|
| `measure_text(text, Some(available))` | Width/height wrapped to available space | **Fit-available** size (what `InlineWidth`/`InlineHeight` use today) |
| `measure_text(text, Some(0.0))` | Width of longest word, fully-wrapped height | **Min-content** size |
| `measure_text(text, None)` | Single-line width/height | **Max-content** size |

Additionally, both `TextMetrics` and `WrappedTextMetrics` already carry `ascent` and `descent` from cosmic-text — the `StylerAccess::measure_text` interface just discards them, returning only `(width, height)`.

**The gap is not in measurement — it's in the formula/resolver interface.** The current `InlineWidth`/`InlineHeight` formula variants hardcode the fit-available mode. We need to parameterize them.

---

## Design: Two Formula Changes

### Change 1: Parameterized Inline Measurement

Replace the separate `InlineWidth` / `InlineHeight` variants with a single parameterized variant:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeasureMode {
    /// Wrap to containing block width (current InlineWidth/InlineHeight behavior).
    FitAvailable,
    /// No wrapping constraint — width of longest word.
    MinContent,
    /// No wrapping — single-line measurement.
    MaxContent,
    /// First-line baseline (ascent from top of content area).
    Baseline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeasureAxis {
    Width,
    Height,
}

pub enum Formula {
    // ... existing variants ...

    /// Inline content measurement with configurable mode and axis.
    /// Replaces InlineWidth, InlineHeight, and covers min-content,
    /// max-content, and baseline queries.
    InlineMeasure(MeasureAxis, MeasureMode),

    // InlineWidth and InlineHeight kept as aliases during transition,
    // or removed entirely and replaced with InlineMeasure.
}
```

Macros:

```rust
inline_width!()          // InlineMeasure(Width, FitAvailable)  — backwards compat
inline_height!()         // InlineMeasure(Height, FitAvailable) — backwards compat
min_content_width!()     // InlineMeasure(Width, MinContent)
min_content_height!()    // InlineMeasure(Height, MinContent)
max_content_width!()     // InlineMeasure(Width, MaxContent)
inline_baseline!()       // InlineMeasure(Width, Baseline) — axis ignored, returns ascent
```

Resolver — one function handles all modes:

```rust
fn resolve_inline_measure(&mut self, axis: MeasureAxis, mode: MeasureMode, styler: &dyn StylerAccess) -> Option<Subpixel> {
    let max_width = match mode {
        MeasureMode::FitAvailable => Some(self.containing_block_width(styler)),
        MeasureMode::MinContent => Some(0.0),
        MeasureMode::MaxContent => None,
        MeasureMode::Baseline => Some(self.containing_block_width(styler)),
    };

    if let Some(text) = styler.text_content() {
        if text.trim().is_empty() { return None; }
        // Expand measure_text to return (width, height, ascent, descent)
        let metrics = styler.measure_text_full(&text, max_width)?;
        return Some(match (axis, mode) {
            (_, MeasureMode::Baseline) => Subpixel::from_f32(metrics.ascent),
            (MeasureAxis::Width, _) => Subpixel::from_f32(metrics.width),
            (MeasureAxis::Height, _) => Subpixel::from_f32(metrics.height),
        });
    }

    // Non-text nodes: recurse into children
    match mode {
        MeasureMode::Baseline => {
            // First child's baseline + own padding-top + border-top
            let children = styler.related_iter(MultiRelationship::Children);
            for child in &children {
                if let Some(b) = self.resolve_inline_measure(MeasureAxis::Width, MeasureMode::Baseline, child.as_ref()) {
                    let pt = styler.get_property(&PropertyId::PaddingTop).unwrap_or(Subpixel::ZERO);
                    let bt = styler.get_property(&PropertyId::BorderTopWidth).unwrap_or(Subpixel::ZERO);
                    return Some(b + pt + bt);
                }
            }
            None
        }
        MeasureMode::MinContent => {
            // Block: max of children's min-content widths
            // (display-aware dispatch handled by query functions, not here)
            let children = styler.related_iter(MultiRelationship::Children);
            let mut result = Subpixel::ZERO;
            for child in &children {
                if let Some(v) = self.resolve_inline_measure(axis, mode, child.as_ref()) {
                    result = result.max(v);
                }
            }
            Some(result)
        }
        _ => {
            // FitAvailable / MaxContent — existing inline element logic
            let children = styler.related_iter(MultiRelationship::Children);
            if children.is_empty() { return None; }
            let mut total = Subpixel::ZERO;
            for child in &children {
                if let Some(v) = self.resolve_inline_measure(axis, mode, child.as_ref()) {
                    total = total + v;
                }
            }
            Some(total)
        }
    }
}
```

**StylerAccess change:** Expand `measure_text` to return full metrics:

```rust
pub struct TextMeasurement {
    pub width: f32,
    pub height: f32,
    pub ascent: f32,
    pub descent: f32,
}

pub trait StylerAccess {
    // Replace measure_text(&self, text, max_width) -> Option<(f32, f32)>
    // with:
    fn measure_text(&self, text: &str, max_width: Option<f32>) -> Option<TextMeasurement>;
}
```

The implementation in `styler_context.rs` already has access to `WrappedTextMetrics.ascent`/`.descent` — just stop discarding them.

### Change 2: `Formula::Imperative` for Iterative Algorithms

```rust
/// Batch-returning imperative resolver function.
///
/// Computes values for multiple nodes at once (e.g., all flex items
/// on a line). Returns (NodeId, value) pairs that are all cached
/// under this formula's pointer. Subsequent calls for sibling nodes
/// hit the cache directly.
pub type ImperativeFn = fn(
    node: NodeId,
    styler: &dyn StylerAccess,
    resolve: &mut dyn FnMut(&'static Formula, NodeId, &dyn StylerAccess) -> Option<Subpixel>,
) -> Option<Vec<(NodeId, Subpixel)>>;

pub enum Formula {
    // ... existing variants ...

    /// Imperative resolution with batch caching.
    ///
    /// The function performs arbitrary computation (iteration,
    /// conditionals on computed values) and returns results for
    /// multiple nodes. All returned values are inserted into the
    /// cache. The resolve callback allows reading any other formula
    /// value through the same cache context.
    Imperative(ImperativeFn),
}
```

Resolver:

```rust
Formula::Imperative(func) => {
    let results = func(node, styler, &mut |f, n, s| self.resolve(f, n, s))?;
    let formula_ptr = from_ref::<Formula>(formula) as usize;
    let mut my_value = None;
    for &(n, val) in &results {
        self.cache.insert(CacheKey { node: n, formula_ptr }, val);
        if n == node {
            my_value = Some(val);
        }
    }
    my_value
}
```

Macro:

```rust
#[macro_export]
macro_rules! imperative {
    ($fn:expr) => {{
        static F: $crate::Formula = $crate::Formula::Imperative($fn);
        &F
    }};
}
```

**Cache behavior:** When the first flex item on a line resolves, the imperative function runs the full §9.7 algorithm for ALL items on that line, returning `Vec<(NodeId, Subpixel)>`. All results are cached. When subsequent items resolve, they find their value already cached → O(1). Total cost: O(n) for the first item, O(1) for the rest.

---

## Implementation Phases

### Phase 1: `Formula::Imperative` + Full §9.7

**Goal:** Flex item main-axis sizing implements the complete spec algorithm with iterative freeze-and-redistribute. Explicit `min-width`/`max-width`/`min-height`/`max-height` are enforced.

**Changes:**

| File | Change |
|------|--------|
| `crates/core/src/formula/mod.rs` | Add `Imperative` variant, `ImperativeFn` type |
| `crates/core/src/formula/macros.rs` | Add `imperative!` macro |
| `crates/core/src/formula/resolver.rs` | Handle `Imperative` in `resolve_inner` with batch caching |
| `crates/layout/src/queries/flex.rs` | Replace `flex_item_main_formula` and `flex_item_main_formula_wrap` with imperative resolvers implementing §9.7 |
| `crates/layout/src/queries/size.rs` | Wire imperative flex sizing through `flex_item_size` |

**The imperative resolver implements:**

1. Collect all sibling items: basis, flex-grow, flex-shrink, explicit min/max CSS values
2. Compute container main size and total gaps
3. Determine initial free space
4. Loop (max n iterations where n = item count):
   - Compute each unfrozen item's target = basis + proportional share of free space
   - Check ALL items for min/max violations
   - If any violation: freeze violated items at their clamped size, recompute free space, continue
   - If no violations: done
5. Return all items' final sizes as `Vec<(NodeId, Subpixel)>`

For wrapping containers, the imperative function computes line assignments first (same greedy algorithm as `compute_line_assignments`), then runs the §9.7 loop independently per line.

**Testing:**
- `flex_min_max.html` — existing fixture passes
- New: `flex_clamp_grow.html`, `flex_clamp_shrink.html`, `flex_clamp_multi.html` (multi-item cascading freeze)

### Phase 2: Parameterized `InlineMeasure` + Automatic Minimum Size (§4.5)

**Goal:** `min-width: auto` on flex items computes to the content-based minimum size per §4.5. Min-content/max-content sizing primitives available for all layout modes.

**Changes:**

| File | Change |
|------|--------|
| `crates/core/src/formula/mod.rs` | Add `InlineMeasure(MeasureAxis, MeasureMode)` variant, `MeasureMode` enum. Remove or alias `InlineWidth`/`InlineHeight` |
| `crates/core/src/formula/macros.rs` | Add `min_content_width!()`, `min_content_height!()`, `max_content_width!()`, `inline_baseline!()`. Keep `inline_width!()`/`inline_height!()` as aliases |
| `crates/core/src/formula/resolver.rs` | Replace `resolve_inline_width`/`resolve_inline_height` with unified `resolve_inline_measure`. Add `TextMeasurement` struct to `StylerAccess` |
| `crates/css/src/styler_context.rs` | Return full `TextMeasurement` (width, height, ascent, descent) instead of `(f32, f32)` |
| `crates/layout/src/queries/size.rs` | Add `min_content_size_query` dispatching by display type |
| `crates/layout/src/queries/flex.rs` | Add `flex_auto_min_size` implementing §4.5; wire into Phase 1's imperative resolver as the `min` floor per item |

**§4.5 logic inside the imperative resolver:**

For each item, when collecting its `min` value:
1. If `min-width` (or `min-height` for column) has an explicit length → use that
2. Else if the item is a scroll container (`overflow != visible`) → use 0
3. Else (auto): content-based minimum = `min(min_content_size, specified_size_or_infinity)`, clamped by max constraint

The min-content size is resolved via the `min_content_width!()`/`min_content_height!()` formula through the `resolve` callback, which calls `resolve_inline_measure` with `MeasureMode::MinContent`.

**Testing:**
- `flex_auto_min_text.html` — item with long text, won't shrink below longest word
- `flex_auto_min_overflow.html` — `overflow: hidden` item shrinks to 0
- `flex_auto_min_explicit.html` — `min-width: 0` overrides auto minimum

### Phase 3: Baseline Alignment

**Goal:** `align-items: baseline` and `align-self: baseline` work correctly in flex containers.

**Changes:**

| File | Change |
|------|--------|
| `crates/layout/src/queries/flex.rs` | Add `CrossAlign::Baseline` variant. Map `BaselinePosition` to it instead of `FlexStart`. Build cross-offset formulas using `inline_baseline!()` + `line_item_aggregate!(Max)` for max baseline on line |
| (no formula changes) | `InlineMeasure(_, Baseline)` already added in Phase 2 |

**Cross-offset formula for baseline-aligned items:**

```
my_offset = max_baseline_on_line - my_baseline
```

Where:
- `my_baseline = inline_baseline!()` — ascent from top of content area (via `InlineMeasure(Width, Baseline)`)
- `max_baseline_on_line = line_item_aggregate!(Max, OrderedChildren, baseline_if_aligned_query, ...)` — max baseline among baseline-participating items on the same line
- `baseline_if_aligned_query` returns `inline_baseline!()` only for items with `align-self: baseline`, returns `None` for others (excluding them from the max)

For items without text content and no children, the baseline falls back to the item's cross size (margin-box bottom edge), per spec.

**Testing:**
- `flex_align_baseline.html` — items with different font sizes
- `flex_align_baseline_nested.html` — baseline from nested child element
- `flex_align_baseline_no_text.html` — empty item, falls back to margin edge

---

## Phase Dependencies

```
Phase 1 (Imperative + §9.7)  ──→  Phase 2 (InlineMeasure + §4.5)  ──→  Phase 3 (Baseline)
       independent                  depends on Phase 1                    depends on Phase 2
                                    (auto min feeds into                   (uses InlineMeasure
                                     imperative resolver)                   Baseline mode)
```

Phase 3 is lightweight once Phase 2 lands — it's primarily query-function wiring in `flex.rs`, no new formula variants.

---

## Design Alternatives Considered

### Alternative A: Separate formula variants for each measurement mode

Add `MinContentWidth`, `MinContentHeight`, `MaxContentWidth`, `InlineBaseline` as distinct `Formula` variants.

**Rejected:** These all do the same thing (call `measure_text` with different parameters). Parameterizing `InlineMeasure` with `MeasureMode` is cleaner, more extensible, and keeps the `Formula` enum small.

### Alternative B: Simple post-hoc clamping (`min!(max_size, max!(min_size, result))`)

Wrap the existing declarative flex formula in `min!/max!` for clamping, skip redistribution.

**Rejected:** This gives the right size for the clamped item but the **wrong size for unconstrained siblings** — they don't receive the redistributed free space. Example: items A(basis=400, min=250) and B(basis=400) in a 600px container. Proportional shrink gives A=200, B=200. Clamping A to 250 gives (250, 200) but the correct answer is (250, 350). Only the iterative algorithm produces correct results for all items.

### Alternative C: Full imperative layout (abandon formulas for flex)

**Rejected:** Loses fine-grained caching, incremental updates, and structural change detection. Creates two parallel layout systems.

### Alternative D: Multi-pass resolution

Run the resolver multiple times, feeding previous results forward.

**Rejected:** Formula system uses pointer-identity caching. Multi-pass requires either separate caches per pass or selective eviction — both add complexity. The imperative variant is more targeted.
