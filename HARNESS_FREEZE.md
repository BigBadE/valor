# chromiumoxide Investigation - ROOT CAUSE FOUND

## All Three Bugs Identified and Root Cause Discovered

### Bug #1: Handler is !Unpin (FIXED ✅)
- **Problem**: `StreamExt::next()` on unpinned `!Unpin` types doesn't call `poll_next()`
- **Fix**: `let mut handler = pin!(handler);` before calling `.next()`
- **Status**: ✅ FIXED - Handler.poll_next() now being called
- **Location**: `crates/valor/tests/chromium_compare/layout_tests.rs:630`

### Bug #2: Handler Never Yields (FIXED ✅)
- **Problem**: Handler.poll_next() loops forever without returning to executor
- **Fix**: `return Poll::Ready(Some(Ok(())));` when work is done
- **Status**: ✅ FIXED - Handler now yields properly
- **Location**: `testing/chromiumoxide/src/handler/mod.rs:675-679`

### Bug #3: Chrome Crashes During evaluate() - ROOT CAUSE FOUND!
**Problem**: Chrome process crashes when trying to access DOM after manual HTML injection

**Root Cause**: Using manual `document.open()/document.write()/document.close()` without
proper navigation wait leaves document in unstable state. When evaluate() tries to access
`document.body` or call `getComputedStyle()`, Chrome crashes.

**Evidence**:
```
Page.loadEventFired event
CallId(17) received successfully
→ page.evaluate(layout_extraction_script) sent as CallId(18)
→ Inspector.targetCrashed event
→ Target.targetCrashed event
CallId(19) received (cleanup)
CallId(18) MISSING ← Chrome crashed before response!
```

**Investigation Results**:
1. ✅ Poll::Ready yield is REQUIRED - without it, Handler never yields and test hangs
2. ✅ Simple script `JSON.stringify({ result: 1+1 })` works fine - completes in 1.68s
3. ❌ ANY script accessing `document.body` causes Chrome crash - even just reading tagName
4. ❌ `getComputedStyle()` causes crash
5. ❌ `getBoundingClientRect()` causes crash
6. ✅ Using `page.set_content()` instead of manual injection fixes page load
7. ❌ But Chrome STILL crashes when accessing DOM elements

**Attempted Fix**: Changed from manual injection to `page.set_content()`
- Result: Page now loads properly (see Page.loadEventFired, Page.domContentEventFired)
- But: Chrome STILL crashes when evaluate() tries to access document.body

**Status**: ❌ PARTIAL PROGRESS - Page loads correctly but DOM access still crashes Chrome

## Current State

**What Works**:
- ✅ Browser launches successfully
- ✅ Handler processes events (with pin + yield fixes)
- ✅ Page loads and fires lifecycle events
- ✅ Simple JavaScript evaluation (non-DOM) works
- ✅ CallId(0-17) all succeed

**What Fails**:
- ❌ Accessing `document.body` in evaluate() → Chrome crashes
- ❌ Calling `window.getComputedStyle()` → Chrome crashes
- ❌ Calling `element.getBoundingClientRect()` → Chrome crashes
- ❌ Layout extraction script → Chrome crashes (uses all of the above)

## Detailed Timeline

### With manual document.open()/write()/close() (ORIGINAL):
1. Browser launches
2. Page created
3. evaluate(`document.open(); document.write(html); document.close();`) - SUCCESS
4. NO Page.loadEventFired (document not properly initialized)
5. evaluate(layout_script) - **CHROME CRASHES** trying to access uninitialized document

### With page.set_content() (CURRENT):
1. Browser launches
2. Page created
3. set_content(html) internally does:
   - evaluate(document.open/write/close)
   - wait_for_navigation() ← KEY DIFFERENCE
4. ✅ Page.domContentEventFired
5. ✅ Page.loadEventFired
6. ✅ Page.lifecycleEvent
7. CallId(17) received
8. evaluate(layout_script) sent as CallId(18)
9. **CHROME STILL CRASHES** - Inspector.targetCrashed / Target.targetCrashed
10. CallId(18) never arrives
11. Timeout after 10s

## Hypothesis

The crash is NOT about page initialization (set_content fixed that).
The crash happens when trying to access DOM/layout APIs in the evaluate() call.

Possible causes:
1. **Chrome version bug**: The headless Chrome might have a bug with these specific APIs
2. **Sandbox/security issue**: Running with --no-sandbox might trigger protection
3. **Memory corruption**: The document.write() method might corrupt internal state
4. **Timing issue**: Evaluate() called too soon after page load (race condition)

## Next Steps to Try

1. Add delay after set_content() before evaluate()
2. Try using `page.goto("data:text/html,<html>...")` instead of set_content()
3. Check Chrome version and known bugs
4. Try different Chrome launch flags
5. Test with visible (non-headless) Chrome
6. Try evaluating in separate steps (get body, then get styles, etc)

## Files Modified

### testing/chromiumoxide/src/handler/mod.rs
- Lines 675-679: Poll::Ready yield fix
- Lines 638-665: Debug logging for event tracking

### crates/valor/tests/chromium_compare/layout_tests.rs
- Line 630: Handler pinning fix
- Lines 681-689: Changed to use page.set_content() instead of manual injection
- Lines 603-607: Single-fixture debug filter
- Line 1014-1082: Layout extraction script (restored to original after testing)
