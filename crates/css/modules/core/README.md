# CSS Core Module

## Overview

**Specification:** https://www.w3.org/TR/CSS22/

**Overall Status:** [Production] with targeted [Approximation]/[TODO] items

This module implements the CSS 2.2 core layout engine, covering:
- **Chapter 8**: Box model (margins, padding, borders, collapsing margins)
- **Chapter 9**: Visual formatting model (block/inline formatting contexts, positioning schemes)
- **Chapter 10**: Visual formatting model details (width/height computation, min/max constraints)

### Scope and Maturity

- **Status**: [Production] with targeted [Approximation]/[TODO] items tracked below.
- **Non-production items and caveats**:
  - [Approximation] BFC detection is implemented and used; continue to expand test coverage across edge scenarios.
  - [MVP] Inline formatting context and anonymous block synthesis are owned by the inline/text module and Display reorg; Core assumes block-level children.
  - [MVP] Positioned layout beyond relative (absolute/fixed/sticky) is owned by the `position` module; Core provides integration points.
  - [TODO] Box-sizing edge cases (see below under §10.3.3/§10.6.3) to be fully locked by fixtures.

### Key Implementation Details

**§10.3.3 Widths of non-replaced blocks** — [Production] with caveats
- [Production] Specified width + auto margins with `box-sizing` conversion: implemented auto margin resolution in the specified-width path.
- [Approximation] Percent min/max width in content-box with nested containers: conversion and resolution covered in code, expand fixtures.
- [Approximation] Over-constraint clamping (padding+border vs available width): keep asserts/fixtures to stabilize.
- Code: `10_visual_details/part_10_3_3_block_widths.rs::{solve_block_horizontal, used_border_box_width, compute_width_constraints, resolve_with_constrained_width, resolve_auto_width}`; parser flags at `css/orchestrator/src/style.rs::apply_edges_and_borders` and model fields `css/orchestrator/src/style_model.rs::ComputedStyle.{margin_left_auto,margin_right_auto}`

**§10.6.3 Heights of non-replaced blocks** — [Production] with caveats
- [Production] Percent/relative heights: root percent resolved to viewport; non-root percent resolved when parent has definite specified height (px); percent min/max constraints applied. Parsing fields in `css/orchestrator/src/style_model.rs::ComputedStyle.{height_percent,min_height_percent,max_height_percent}` with parsing in `css/orchestrator/src/style.rs::apply_dimensions`.
- [Approximation] `box-sizing` conversions for min/max height under padding/border; expand fixtures.
- Code: `10_visual_details/part_10_6_3_height_of_blocks.rs::{compute_used_height, compute_child_content_height, compute_root_heights}`

## Module Structure

### Directory Layout

```
core/
├── README.md              # This file (includes overview)
├── Cargo.toml             # Module manifest
├── src/                   # Implementation code
│   ├── 8_box_model/       # Chapter 8 implementation
│   │   ├── spec.md        # Chapter 8: Box Model spec
│   │   └── *.rs           # Implementation files
│   ├── 9_visual_formatting/ # Chapter 9 implementation
│   │   ├── spec.md        # Chapter 9: Visual Formatting Model spec
│   │   └── *.rs           # Implementation files
│   ├── 10_visual_details/ # Chapter 10 implementation
│   │   ├── spec.md        # Chapter 10: Visual Details spec
│   │   └── *.rs           # Implementation files
│   └── lib.rs             # Module entry point
└── tests/                 # Test suites
    └── fixtures/          # Test fixtures
```

## Implementation Status

### Current Progress

This module is in **[Production]** status with active implementation. The core layout algorithms are implemented and tested, with some areas marked for improvement.

**Completed:**
- ✓ CSS 2.2 box model (Chapter 8)
- ✓ Visual formatting model (Chapter 9)
- ✓ Width/height computation algorithms (Chapter 10)
- ✓ Margin collapsing
- ✓ Block formatting contexts
- ✓ Integration with orchestrator

**In Progress:**
- Expanding BFC edge case test coverage
- Refining box-sizing edge cases
- Improving percent width/height handling in nested containers

### Chapter-by-Chapter Status

Each chapter of CSS 2.2 is tracked separately. See src/N_*/spec.md files for:
- **[Production]**: Feature complete and tested
- **[MVP]**: Minimum viable implementation
- **[Approximation]**: Simplified implementation
- **[TODO]**: Planned but not implemented

## Specification Coverage

The vendored specifications in this module are embedded verbatim from the W3C with inline implementation notes. Each chapter file contains:

1. **Verbatim Spec**: Complete W3C specification text
2. **Status Markers**: Implementation status for each section
3. **Code Locations**: References to implementation files
4. **Test References**: Links to relevant test fixtures
5. **Implementation Notes**: Known limitations, approximations, and TODOs

## Testing

### Running Tests

```bash
# Run all tests for this module
cargo test --package css_core

# Run specific test
cargo test --package css_core test_name
```

### Test Organization

- **Unit tests**: In `src/` files using `#[cfg(test)]`
- **Integration tests**: In `tests/` directory
- **Fixtures**: HTML/CSS test files in `tests/fixtures/`

## Code Organization

### Key Files

- `src/style_computer.rs`: Main style computation engine
- `src/layout_engine.rs`: Layout tree builder
- `src/8_box_model/`: Box model implementation (Chapter 8)
- `src/9_visual_formatting/`: Visual formatting model (Chapter 9)
- `src/10_visual_details/`: Layout algorithms (Chapter 10)

## Integration with Valor

This module integrates with Valor's rendering pipeline through:

1. **Parsing**: CSS properties are parsed by the syntax module
2. **Cascade**: Values are cascaded through the cascade module
3. **Computation**: This module computes final values
4. **Layout**: Results are used by the layout engine
5. **Rendering**: Final values drive the renderer

## Spec Compliance

This module aims for strict W3C specification compliance. Deviations are:
- Explicitly marked as `[Approximation]` with justification
- Documented in the chapter spec files
- Tracked as TODOs for future improvement

Any behavior that differs from real browsers (Chrome, Firefox, Safari) should be considered a bug and fixed to match spec and browser consensus.

## Contributing

When working on this module:

1. **Read the spec**: Check `chapter_*/spec.md` files for the relevant section
2. **Check status**: Look for `[TODO]` or `[Approximation]` markers
3. **Update markers**: Change status to `[MVP]` or `[Production]` when implementing
4. **Add tests**: Create fixtures in `tests/fixtures/`
5. **Update notes**: Document any limitations or decisions in spec files
6. **Run tests**: Ensure `cargo test` passes
7. **Code standards**: Run `./scripts/code_standards.sh` before committing

## References

- **W3C Specification**: https
- **MDN Documentation**: https://developer.mozilla.org/en-US/docs/Web/CSS
- **Can I Use**: https://caniuse.com/
- **Web Platform Tests**: https://wpt.fyi/

## Legal Notice

The W3C specifications embedded in this module are:

```
Valor Browser Engine: https://github.com/valor-software/valor
Copyright © 2025 World Wide Web Consortium. All Rights Reserved.
This work is distributed under the W3C® Software and Document License [1]
in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even
the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
[1] https://www.w3.org/Consortium/Legal/copyright-software
```
