# CSS Color — Module Spec Checklist

Spec: https://www.w3.org/TR/css-color-4/

## Scope
- MVP subset for Speedometer diffs.
- Support parsing of <color> values for `color`, `background-color`, and `border-color` used by core/computed style.
- Accepted inputs: named colors, hex forms (#rgb/#rgba/#rrggbb/#rrggbbaa), and rgb()/rgba().

Out of scope (for now):
- Newer syntaxes color(), lab/lch/oklab/oklch, hsl()/hwb(), and color-mix().
- Relative color syntax and color profiles.
- System color keywords and forced-colors adjustments.

## Checklist
- [x] § 13 — Colors: the `color` property
  - [x] `parse_css_color()` — crates/css/modules/color/src/lib.rs
- [x] § 4 — Color value: <color> legacy syntaxes
  - [x] Named colors, hex, rgb()/rgba()
- [ ] § 4+ — Modern color spaces and functions
  - [ ] color(), lab/lch/oklab/oklch, hsl()/hwb()

## Parsing overview
- Implementation delegates to the `csscolorparser` crate to handle legacy color syntaxes robustly.
- Returns normalized 8-bit RGBA channels.

## Integration
- Upstream: tokens/values come from inline style attributes and later cascade results.
- Downstream: `css_core` consumes `parse_css_color()` to populate `ComputedStyle.color`, `ComputedStyle.background_color`, and `ComputedStyle.border_color`.

## Future work
- [ ] Add hsl()/hwb() parsing coverage.
- [ ] Support modern color functions (color(), lab/lch/oklab/oklch).
- [ ] Implement system colors and forced-colors adjustments where applicable.
