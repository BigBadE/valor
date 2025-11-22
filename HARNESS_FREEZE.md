# chromiumoxide Handler Bug - Complete Investigation

## Two Bugs Required BOTH Fixes

1. **Handler is !Unpin** - Must pin before calling .next()
2. **Handler must yield** - Must return Poll::Ready when work is done

## Bug #1: Handler is !Unpin

**Problem**: `StreamExt::next()` on unpinned `!Unpin` types doesn't call `poll_next()`

**Why Handler is !Unpin:**
- Contains `Connection<CdpEventMessage>` (WebSocket - !Unpin)
- Contains `Receiver<HandlerMessage>` (async channel - !Unpin)
- No explicit `impl Unpin for Handler`

**Fix #1** (layout_tests.rs:630):
```rust
let mut handler = pin!(handler);  // Pin before calling .next()
while let Some(result) = handler.next().await { ... }
```

## Bug #2: Handler Never Yields

**Problem**: Handler.poll_next() loops forever without yielding to executor

Original code (handler/mod.rs:666-672):
```rust
if done {
    return Poll::Pending;
}
// Falls through to loop again - NEVER yields!
```

**Fix #2** (handler/mod.rs:666-674):
```rust
if done {
    return Poll::Pending;
} else {
    return Poll::Ready(Some(Ok(())));  // Yield when work done
}
```

## Test Results

**With only Fix #1 (pin)**: Handler.poll_next() called, but hangs forever (never yields)

**With only Fix #2 (yield)**: Handler.poll_next() NEVER called (no pinning)

**With BOTH fixes applied**:
- 73 total fixtures
- 30 fixtures processed in ~5 minutes
- 1 success, 32 timeouts
- **MAJOR IMPROVEMENT**: No cascade failures! Test continues after timeouts

## Current Status

Handler IS working (poll_next() called, test progresses), but Chrome responses still timing out at high rate (96% timeout rate). This suggests a THIRD issue beyond pinning and yielding.

Possible remaining issues:
- Chrome responses not being routed correctly through Handler
- evaluate() command/response matching problem
- WebSocket message processing bug in chromiumoxide

## Files Modified

- `testing/chromiumoxide/src/handler/mod.rs` - Added Poll::Ready yield fix
- `crates/valor/tests/chromium_compare/layout_tests.rs` - Added pin!(handler) fix
- Timeouts reverted to 10s (from 60s debugging values)
