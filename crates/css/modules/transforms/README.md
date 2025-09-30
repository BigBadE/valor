# CSS Transforms Module

## Overview

//www.w3.org/TR/css-transforms-2/

**Specification:** https

**Overall Status:** [CSS Transforms Module Level 2:TODO]

## Module Structure

The W3C specification is embedded in src/spec.md with implementation notes and status markers.

### Directory Layout

```
transforms/
├── README.md              # This file
├── Cargo.toml             # Module manifest
├── src/                   # Implementation code
│   ├── spec.md            # W3C specification with implementation notes
│   └── lib.rs
└── tests/                 # Test suites
    └── fixtures/          # Test fixtures
```

## Implementation Status

### Current Progress

This module is planned but not yet implemented.

**TODO:**
- [ ] Set up basic module structure
- [ ] Vendor W3C specification chapters
- [ ] Implement core parsing logic
- [ ] Add computation/application logic
- [ ] Create comprehensive test suite
- [ ] Integrate with orchestrator

### Implementation Status Tracking

The specification is tracked in src/spec.md with inline status markers for:
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
cargo test --package css_transforms

# Run specific test
cargo test --package css_transforms test_name
```

### Test Organization

- **Unit tests**: In `src/` files using `#[cfg(test)]`
- **Integration tests**: In `tests/` directory
- **Fixtures**: HTML/CSS test files in `tests/fixtures/`

## Code Organization

### Key Files

- `src/lib.rs`: Module entry point
- `src/parser.rs`: Property parsing (if applicable)
- `src/computed.rs`: Computed value resolution (if applicable)

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
