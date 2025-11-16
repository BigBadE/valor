# Layout Test Infrastructure - Critical Blocker

## Status: BLOCKED
The layout comparison tests cannot run due to a fundamental incompatibility between the `headless_chrome` Rust crate and the test environment.

## Root Cause
The `headless_chrome` crate (v1.0.18) uses the Chrome DevTools Protocol (CDP) to communicate with Chrome. The `tab.evaluate()` method, which is essential for extracting layout data from Chrome, hangs indefinitely in this environment.

## Attempted Solutions

### 1. Chrome Version Compatibility (✅ Partially Successful)
- **Issue**: Initially tried Chrome 142, which is too new
- **Solution**: Manually downloaded and installed Chromium revision 1095492 (v111.0.5555.0), which is the version headless_chrome expects
- **Location**: `~/.local/share/headless-chrome/linux-1095492/chrome-linux/chrome`
- **Result**: Chrome launches successfully, but `tab.evaluate()` still hangs

### 2. Navigation Method Improvements (✅ Completed)
-  **File**: `crates/valor/tests/chromium_compare/browser.rs`
- **Changes**:
  - Removed `wait_until_navigated()` which doesn't work with file:// URLs
  - Simplified to use `std::thread::sleep(Duration::from_millis(300))`
  - Removed unused import warnings
- **Result**: Navigation completes, but next step (tab.evaluate) still hangs

### 3. headless_chrome "fetch" Feature (❌ Failed)
- **Attempted**: Enabled "fetch" feature to let headless_chrome download Chrome automatically
- **Result**: Network DNS resolution failure - ureq library couldn't resolve googleapis.com
- **Outcome**: Manually downloaded Chrome instead (see solution #1)

## The Fundamental Problem
Even with the correct Chrome version (111), `tab.evaluate()` in the `headless_chrome` crate hangs indefinitely. This occurs in `chromium_layout_json_in_tab()` at line 543 of `layout_tests.rs`:

```rust
let result = tab.evaluate(script, true)?;  // <-- Hangs here indefinitely
```

This method is essential because it:
1. Executes JavaScript in Chrome to extract layout geometry using `getBoundingClientRect()`
2. Returns the layout JSON for comparison against Valor's layout

Without this working, there's no way to get Chrome's layout data for comparison.

## Test Execution Flow (Where It Hangs)
1. ✅ Chrome launches successfully (PID 17200, Chromium 111.0.5555.0)
2. ✅ Discovers 72 layout fixtures
3. ✅ Valor computes its own layout (logs show DOM updates, layout computation)
4. ✅ Navigates Chrome tab to first fixture file://...
5. ❌ **HANGS** when calling `tab.evaluate(chromium_layout_extraction_script(), true)`

## Why This Is A Blocker
The original task is to "fix all the issues remaining with the layout fixtures" by following CSS specifications exactly. This requires:
1. Running layout comparison tests to identify which fixtures fail
2. Analyzing the differences between Valor's layout and Chrome's layout
3. Fixing Valor's layout code to match the CSS spec

**Step 1 is blocked** - we cannot identify failing fixtures because we cannot run the comparison tests.

## Potential Solutions (Not Yet Implemented)

###  Option A: Switch to Different Chrome Automation Library
- Replace `headless_chrome` with `chromiumoxide` (v0.7)
- **Pros**: More actively maintained, async/await based, may work in this environment
- **Cons**: Requires rewriting significant test infrastructure:
  - `browser.rs` - entire browser setup and tab management
  - `layout_tests.rs` - JavaScript evaluation logic
  - `graphics_tests.rs` - screenshot capture logic
  - Must convert from sync to async throughout test code
- **Effort**: Several hours of work

### Option B: Use Chrome CLI Instead of CDP
- Launch Chrome with `--dump-dom` or similar flags
- Parse output directly instead of using CDP
- **Pros**: Avoids CDP incompatibility
- **Cons**: May not provide layout geometry data (x, y, width, height), only DOM

### Option C: Manual Layout Analysis
- Extract Valor's layout output to JSON files
- Manually compare against CSS specifications
- Fix issues based on spec compliance rather than Chrome comparison
- **Pros**: Can make progress on layout fixes immediately
- **Cons**: More time-consuming, no automated regression testing

## Files Modified So Far
1. **`.cargo/config.toml`** - Changed linker from mold to lld (remains)
2. **`crates/js/js_engine_v8/Cargo.toml`** - Temporarily disabled V8 (remains)
3. **`crates/page_handler/Cargo.toml`** - Changed default to js_stub (remains)
4. **`crates/valor/Cargo.toml`** - Changed default to js_stub (remains)
5. **`crates/valor/tests/chromium_compare/browser.rs`** - Improved navigation (ready to commit)
6. **`crates/valor/tests/chromium_compare/graphics_tests.rs`** - Platform-specific event loop fixes (remains)

## Chromium Installation
Chromium 111.0.5555.0 (revision 1095492) is installed and ready to use:
- Path: `~/.local/share/headless-chrome/linux-1095492/chrome-linux/chrome`
- Status: Verified working (`chrome --version` succeeds)
- Size: ~390MB extracted

## Recommendation
**Option A** (switch to chromiumoxide) is recommended because:
1. It's actively maintained (last updated 1 year ago vs headless_chrome's older releases)
2. Uses modern async Rust which may handle CDP more reliably
3. Provides long-term solution for both layout and graphics comparison tests
4. Once implemented, tests should run reliably

**Estimated time**: 3-4 hours to rewrite test infrastructure with chromiumoxide.

## Current State
- All background test processes are still hung (can be killed with `pkill -9 chrome`)
- Chromium 111 is installed and ready
- Browser navigation code is simplified and ready
- Waiting for decision on how to proceed
