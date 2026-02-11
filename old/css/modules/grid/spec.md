# CSS Grid Layout Module - Implementation Status

Spec: <https://www.w3.org/TR/css-grid-2/>

This document tracks the implementation status of CSS Grid Layout in the Valor browser engine.

## Overview

CSS Grid Layout is a two-dimensional layout system for the web. It lets you lay out content in rows and columns, and has many features that make building complex layouts straightforward.

## Implementation Status

### §1 Introduction

[Production] Grid layout is fully recognized and parsed.
- Code: `crates/css/orchestrator/src/style.rs` (display parsing)
- Code: `crates/css/modules/core/src/box_tree/constraint_block_layout.rs` (layout integration)

### §2 Grid Layout Box Model and Terminology

[MVP] Basic grid container and grid items are implemented.
- Code: `crates/css/modules/grid/src/types.rs` (GridItem, GridTrack)
- Grid container establishes grid formatting context
- Grid items are direct children of grid container

### §5 Grid Definition Properties

#### §5.1 grid-template-columns, grid-template-rows

[MVP] Basic track sizing with:
- Fixed lengths (px)
- Flexible tracks (fr units) - simplified distribution
- Auto sizing - basic content-based
- Percentage values

[Production] repeat() notation:
- repeat(count, pattern)
- repeat(auto-fit, pattern)
- repeat(auto-fill, pattern)

[Approximation] minmax():
- Basic minmax(min, max) parsing
- Uses minimum value for MVP track sizing
- Full intrinsic sizing algorithm TODO

Code: `crates/css/modules/core/src/box_tree/constraint_block_layout.rs` (parse_grid_template, parse_track_list)
Fixtures: `crates/css/modules/grid/tests/fixtures/layout/grid/*.html`

#### §5.2 grid-auto-rows, grid-auto-columns

[TODO] Implicit grid track sizing not yet implemented.

#### §5.3 grid-auto-flow

[MVP] Basic auto-flow directions:
- row (default)
- column
- row dense
- column dense

Code: `crates/css/orchestrator/src/style_model.rs` (GridAutoFlow enum)
Code: `crates/css/orchestrator/src/style.rs` (apply_grid_properties)

### §6 Grid Items

#### §6.1 Grid Item Display

[Production] Grid items are collected from container children.
- Code: `crates/css/modules/grid/src/types.rs` (collect_grid_items)
- Normalizes display:none and display:contents children

#### §6.4 Grid Item Sizing

[MVP] Grid items are sized to fill their grid area.
- Code: `crates/css/modules/core/src/box_tree/constraint_block_layout.rs` (layout_grid)
- Creates constraint spaces for each grid item
- Items stretch to fill cell by default

### §7 Alignment

[TODO] Grid-specific alignment properties not yet fully implemented:
- align-items / align-self
- justify-items / justify-self
- align-content / justify-content

Uses existing flexbox alignment types as MVP.
Code: `crates/css/modules/grid/src/layout.rs` (GridAlignment)

### §8 Placing Grid Items

#### §8.1 Common Patterns

[MVP] Basic auto-placement in row-major order.
- Code: `crates/css/modules/grid/src/placement.rs` (place_grid_items)

[TODO] Not yet implemented:
- Named grid lines
- Grid template areas
- Explicit grid-row/column-start/end placement
- Spanning multiple tracks
- Dense packing algorithm

#### §8.3 Line-based Placement

[TODO] Explicit line-based placement with grid-row-start/end and grid-column-start/end.

#### §8.5 Grid Item Placement Algorithm

[MVP] Simplified auto-placement:
- Items placed in row-major order
- Fills grid left-to-right, top-to-bottom
- Respects grid-auto-flow direction

Code: `crates/css/modules/grid/src/placement.rs`
Fixtures: `01_basic_grid.html`, `02_equal_columns.html`, `03_repeat_notation.html`

### §9 Absolute Positioning

[TODO] Absolutely positioned grid items not yet implemented.

### §10 Alignment and Spacing

#### §10.1 Gutters: gap, row-gap, column-gap

[Production] Gap properties are fully implemented.
- Reuses existing flexbox gap infrastructure
- Code: `crates/css/orchestrator/src/style.rs` (apply_gaps)
- Supported in grid layout algorithm

Code: `crates/css/modules/core/src/box_tree/constraint_block_layout.rs`
Fixtures: `01_basic_grid.html` (10px gap), `04_auto_fit_minmax.html` (10px gap)

### §11 Grid Sizing

[MVP] Basic grid sizing algorithm:
- Resolves fixed track sizes (px)
- Distributes flexible space to fr tracks
- Handles percentage tracks
- Basic auto-sizing for content

[Approximation] Simplified track sizing:
- Does not fully implement min-content/max-content intrinsic sizing
- Does not handle spanning items optimally
- Does not implement baseline alignment

Code: `crates/css/modules/grid/src/track_sizing.rs` (resolve_track_sizes)

### §12 Fragmenting Grid Layout

[TODO] Fragmentation (pagination, multi-column) not implemented.

## Test Fixtures

Location: `crates/css/modules/grid/tests/fixtures/layout/grid/`

1. **01_basic_grid.html** - Basic 3x2 grid with fixed track sizes
2. **02_equal_columns.html** - Grid with 1fr 1fr 1fr columns
3. **03_repeat_notation.html** - Grid using repeat(4, 150px)
4. **04_auto_fit_minmax.html** - Grid with repeat(auto-fit, minmax(200px, 1fr))
5. **05_mixed_tracks.html** - Grid with mixed px, fr, and flexible tracks
6. **06_two_column_layout.html** - Simple two-column sidebar layout
7. **07_card_grid.html** - Card-based grid layout
8. **08_text_render_matrix_grid.html** - Text rendering samples in grid

## Known Limitations (MVP)

1. **No explicit placement**: grid-row/column-start/end properties not parsed or applied
2. **Simplified fr distribution**: Flexible tracks use equal distribution instead of spec algorithm
3. **No spanning**: Items always occupy 1x1 cell
4. **No named lines or areas**: Only numeric line references (auto-generated)
5. **Basic alignment**: Items always stretch to fill cell
6. **No subgrid**: Nested grids are independent
7. **No baseline alignment**: Content alignment simplified
8. **Approximated intrinsic sizing**: min-content/max-content use estimates

## Future Enhancements

Priority features for next iteration:
1. Explicit grid placement (grid-row/column-start/end)
2. Proper fr unit distribution with leftover space
3. Grid item spanning (rowspan/colspan equivalent)
4. Named grid lines and template areas
5. Full intrinsic sizing algorithm
6. Dense packing algorithm
7. Subgrid support
8. Baseline alignment

## Integration Points

- **Style computation**: `crates/css/orchestrator/src/style.rs`
- **Layout engine**: `crates/css/modules/core/src/box_tree/constraint_block_layout.rs`
- **Display enum**: `crates/css/orchestrator/src/style_model.rs`
- **Grid module**: `crates/css/modules/grid/src/`

## Status Summary

- [Production]: 3 features (display:grid, gap properties, repeat notation)
- [MVP]: 7 features (track sizing, auto-flow, grid items, placement, alignment types)
- [Approximation]: 2 features (minmax, intrinsic sizing)
- [TODO]: 8 feature areas (explicit placement, spanning, named areas, etc.)

**Overall Status**: MVP implementation complete. Grid layout works for basic use cases with auto-placement, fixed/flexible tracks, and gap properties. Ready for testing and iteration.
