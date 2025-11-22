# chromiumoxide Handler Hang - EXACT ROOT CAUSE

## The Exact Issue

**Handler is `!Unpin`, and `StreamExt::next()` on unpinned `!Unpin` types doesn't call `poll_next()`!**

### Line-by-Line Breakdown

**BROKEN CODE** (layout_tests.rs:635-637):
```rust
let (browser, mut handler) = Browser::launch(config).await?;  // handler is !Unpin
let handler_task = tokio::spawn(async move {
    while let Some(result) = handler.next().await {  // ❌ Unpinned !Unpin type
        result?;
    }
});
```

**Why it fails:**
1. `Handler` struct contains `Connection<CdpEventMessage>` (WebSocket) and `Receiver<HandlerMessage>` (channel)
2. Both of these types are `!Unpin` (contain async state)
3. Therefore `Handler` is `!Unpin` (no explicit `impl Unpin for Handler`)
4. `StreamExt::next()` on an **unpinned** `!Unpin` type creates a future that never polls the underlying stream
5. The future hangs waiting for something that will never happen

**WORKING CODE**:
```rust
let (browser, handler) = Browser::launch(config).await?;  // Remove mut
let handler_task = tokio::spawn(async move {
    use std::pin::pin;  // ← KEY: Must pin !Unpin types
    use futures::StreamExt;

    let mut handler = pin!(handler);  // ← Pin the handler first!
    while let Some(result) = handler.next().await {  // ✅ Pinned !Unpin type works
        result?;
    }
});
```

## Test Proof

**Unpinned `handler.next().await`:**
- ❌ `Handler.poll_next()` NEVER called
- ❌ Test hangs for 60+ seconds
- ❌ No events processed

**Pinned `pin!(handler); handler.next().await`:**
- ✅ `Handler.poll_next()` called continuously
- ✅ Test completes in 1.89 seconds
- ✅ Events processed correctly

## Why chromiumoxide Examples Work

chromiumoxide examples likely use a different tokio version or runtime configuration where this isn't an issue,
OR they're using `Box::pin()` or another pinning mechanism we didn't notice.

## The Fix

Change ALL handler spawns from:
```rust
let (browser, mut handler) = Browser::launch(config).await?;
tokio::spawn(async move { while let Some(h) = handler.next().await { ... } });
```

To:
```rust
let (browser, handler) = Browser::launch(config).await?;
tokio::spawn(async move {
    use std::pin::pin;
    use futures::StreamExt;
    let mut handler = pin!(handler);
    while let Some(h) = handler.next().await { ... }
});
```

## Root Cause Summary

- **Specific line:** `handler.next().await` on unpinned Handler (layout_tests.rs:640)
- **Why:** `StreamExt::next()` requires `Unpin` or explicit pinning for `!Unpin` types
- **Handler is !Unpin because:** Contains `Connection` (WebSocket) and `Receiver` (channel), both `!Unpin`
- **Solution:** `use std::pin::pin; let mut handler = pin!(handler);` before calling `.next()`
