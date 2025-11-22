# chromiumoxide Investigation - THIRD BUG FOUND

## All Three Bugs Identified

### Bug #1: Handler is !Unpin (FIXED)
- **Problem**: `StreamExt::next()` on unpinned `!Unpin` types doesn't call `poll_next()`
- **Fix**: `let mut handler = pin!(handler);` before calling `.next()`
- **Status**: ✅ FIXED - Handler.poll_next() now being called

### Bug #2: Handler Never Yields (FIXED)
- **Problem**: Handler.poll_next() loops forever without returning to executor
- **Fix**: `return Poll::Ready(Some(Ok(())));` when work is done
- **Status**: ✅ FIXED - Handler now yields properly

### Bug #3: Chrome Crashes During evaluate() (NEW - ROOT CAUSE OF TIMEOUTS!)
- **Problem**: Chrome process crashes when executing evaluation script
- **Evidence**:
  ```
  CallId(17) received
  → Inspector.targetCrashed event
  → Target.targetCrashed event
  CallId(19) received
  CallId(18) MISSING ← This was page.evaluate()!
  ```
- **Impact**: evaluate() times out because Chrome crashed before sending response
- **Status**: ❌ NEEDS FIX

## Test Results

With Bug #1 and #2 fixed:
- ✅ Handler.poll_next() is called
- ✅ Responses CallId(0) through CallId(17) received successfully
- ✅ Events processed correctly
- ❌ **Chrome crashes on CallId(18)** which is `page.evaluate()`
- ❌ evaluate() times out waiting for crashed Chrome

## Chrome Crash Details

Sequence:
1. Browser launches successfully
2. Page navigates successfully
3. CallId(0-17): Setup commands work fine
4. **CallId(18): page.evaluate() sent**
5. **Chrome crashes** - `Inspector.targetCrashed` + `Target.targetCrashed` events
6. CallId(18) response NEVER arrives
7. evaluate() times out after 10s

## Possible Causes of Chrome Crash

1. **Poll::Ready yield timing** - The yield fix might cause Chrome commands to be sent too fast
2. **Script complexity** - The layout extraction script might trigger Chrome bug
3. **Memory/resource issue** - `--no-sandbox` + `--disable-dev-shm-usage` suggest low-memory environment
4. **Chrome version incompatibility** - Script might not work with this Chrome version

## Next Steps

1. Test without Poll::Ready yield fix to see if Chrome still crashes
2. Try simpler evaluation script (e.g., `"1+1"`)
3. Check Chrome version and known crash bugs
4. Add delay between commands to prevent overwhelming Chrome

## Files Modified

- `testing/chromiumoxide/src/handler/mod.rs` - Both fixes applied + debug logging
- `crates/valor/tests/chromium_compare/layout_tests.rs` - Pin fix applied
