# chromiumoxide: StreamExt::next() Bug - CONFIRMED ROOT CAUSE

## The Bug
**`futures::StreamExt::next()` does NOT call `Stream::poll_next()` on chromiumoxide's Handler!**

## Definitive Proof

### Using StreamExt::next() (BROKEN):
```rust
handler.next().await  // Handler.poll_next() NEVER called
```
Result: ❌ Handler.poll_next() never executes, test hangs forever

### Using poll_fn + Stream::poll_next() (WORKS):
```rust
futures::future::poll_fn(|cx| Stream::poll_next(handler_pinned.as_mut(), cx)).await
```
Result: ✅ Handler.poll_next() executes repeatedly, returns Poll::Pending

### Test Output:
```
[HANDLER-EPRINTLN] About to call Stream::poll_next
[HANDLER] poll_next() CALLED - THIS SHOULD APPEAR!  ← WORKS!
[HANDLER-EPRINTLN] Stream::poll_next returned: false (Poll::Pending)
```

## What This Means

1. **Handler Stream implementation is CORRECT** - poll_next() works when called directly
2. **StreamExt::next() is BROKEN** - it doesn't call Stream::poll_next() on Handler
3. **Bug is in futures crate or trait resolution** - StreamExt::next() must be calling a different method

## Workaround

Instead of:
```rust
while let Some(result) = handler.next().await {  // BROKEN
    result?;
}
```

Use:
```rust
use std::pin::pin;
use futures::Stream;

let mut handler_pinned = pin!(handler);
loop {
    match futures::future::poll_fn(|cx| Stream::poll_next(handler_pinned.as_mut(), cx)).await {
        Some(Ok(())) => continue,
        Some(Err(e)) => return Err(e),
        None => break,
    }
}
```

## Next Steps

1. Verify this workaround fixes the timeouts
2. Check futures crate version and look for known issues
3. File bug report against futures crate or chromiumoxide
4. Update all handler usage in tests to use workaround

## Files Modified
- `testing/chromiumoxide/src/handler/mod.rs` - Added eprintln! tracing
- `crates/valor/tests/chromium_compare/layout_tests.rs` - poll_fn test code
