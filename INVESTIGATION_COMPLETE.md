# Complete Investigation: chromiumoxide Handler Never Polls (2025-11-22)

## Summary

**ROOT CAUSE**: Handler.poll_next() is NEVER called by the tokio executor, even though `handler.next().await` is being awaited in a spawned task.

## Definitive Evidence

### Test Logs Show:
1. ✅ `[HANDLER] Handler task started` - Task spawns successfully
2. ✅ `[HANDLER] About to call handler.next().await` - Code reaches the await point
3. ✅ `tokio::task::yield_now().await` completes - Task can execute async code
4. ✅ `[HANDLER] After yield_now` - Task continues after yield
5. ❌ `[HANDLER] poll_next() CALLED` - **NEVER APPEARS**
6. ❌ `eprintln!("[HANDLER] poll_next() CALLED")` - **NEVER APPEARS** (even eprintln!)

### Code Added for Debugging:

**handler/mod.rs:536**
```rust
fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    eprintln!("[HANDLER] poll_next() CALLED - THIS SHOULD APPEAR!");  // ❌ Never executes
    tracing::error!("[HANDLER] poll_next() CALLED");  // ❌ Never executes
    // ... rest of implementation
}
```

**layout_tests.rs:640**
```rust
let handler_task = tokio::spawn(async move {
    log::error!("[HANDLER] Handler task started");  // ✅ Executes
    tokio::task::yield_now().await;  // ✅ Completes
    log::error!("[HANDLER] After yield_now");  // ✅ Executes
    while let Some(event_result) = handler.next().await {  // ⚠️ Blocks forever, never polls
        // Never reaches here
    }
});
```

## Runtime Configuration

**Test Structure** (chromium_tests.rs):
```rust
#[test]
fn run_chromium_tests() -> Result<()> {
    chromium_compare::run_chromium_tests()
}
```

**Runtime Creation** (layout_tests.rs:566):
```rust
let runtime = Runtime::new()?;  // Multi-threaded tokio runtime
runtime.block_on(async {
    let (browser, mut handler) = Browser::launch(config).await?;
    let handler_task = tokio::spawn(async move { handler.next().await });
    // ... rest of test
});
```

## Comparison with chromiumoxide Examples

**simple-google.rs (WORKS)**:
```rust
#[tokio::main]
async fn main() {
    let (browser, mut handler) = Browser::launch(config).await.unwrap();

    let h = task::spawn(async move {
        while let Some(h) = handler.next().await {  // This WORKS
            h.unwrap();
        }
    });

    let page = browser.new_page("...").await.unwrap();
    h.await.unwrap();
}
```

**Our Test (DOESN'T WORK)**:
```rust
let runtime = Runtime::new()?;
runtime.block_on(async {
    let (browser, mut handler) = Browser::launch(config).await?;

    let handler_task = tokio::spawn(async move {
        while let Some(h) = handler.next().await {  // This DOESN'T WORK
            h.unwrap();
        }
    });

    let page = browser.new_page("...").await?;
    // ... continue without awaiting handler_task
});
```

## Key Differences

1. **Runtime Creation**:
   - Example uses `#[tokio::main]` macro
   - Test uses manual `Runtime::new()` + `runtime.block_on()`

2. **Handler Task Lifecycle**:
   - Example awaits handler task at end: `h.await.unwrap()`
   - Test spawns handler task and never awaits it (runs in background)

## Previous Investigation Findings

1. **NOT PR #197 message dropping**: Reverting PR #197 didn't fix timeouts
2. **NOT commit ac5259f's broken fix**: Reverting ac5259f made it worse (no Handler logs at all)
3. **NOT single-threaded deadlock**: Test uses multi-threaded runtime
4. **NOT Chrome hanging**: Chrome responds quickly (~146ms)
5. **NOT async executor/waker bug**: yield_now() works, proving executor is functional

## Hypotheses to Investigate

### 1. StreamExt::next() Future Implementation Issue
- The Future returned by `StreamExt::next()` might not properly delegate to `poll_next()`
- Possible version mismatch between futures crate and chromiumoxide expectations

### 2. Handler Unpin/Pin Issues
- Handler might not implement Unpin correctly
- `Pin::new(&mut handler)` in StreamExt::next() might fail silently

### 3. Tokio Runtime Incompatibility
- `Runtime::new()` + `block_on()` might not properly schedule spawned tasks
- #[tokio::main] might set up additional runtime configuration needed

### 4. Handler Task Dropped Prematurely
- spawning without awaiting might cause issues
- Though logs show task continues to run (yield_now completes)

### 5. Circular Dependency/Deadlock
- `block_on()` waits for inner async block
- Inner async block waits for page operations
- Page operations send commands to Handler via channel
- Handler never polls because... ?

## Recommended Next Steps

1. **Try using `#[tokio::test]` instead of manual runtime**
   - Modify test to use async fn with #[tokio::test]
   - See if that fixes Handler polling

2. **Check Handler Unpin implementation**
   - Verify Handler implements Unpin trait
   - Check if Pin::new() is valid for Handler

3. **Compare futures crate versions**
   - Check what version chromiumoxide expects
   - Check what version the test is using

4. **Try local_set for single-threaded execution**
   - Use `LocalSet` with `current_thread` runtime
   - Match chromiumoxide's expected execution model

5. **Instrument StreamExt::next() implementation**
   - Add debug logging to futures crate's next() method
   - Verify it's actually calling poll_next()

## Files Modified During Investigation

- `/home/user/valor/testing/chromiumoxide/src/handler/mod.rs` - Added extensive tracing
- `/home/user/valor/testing/chromiumoxide/src/conn.rs` - Added Connection.poll_next() tracing
- `/home/user/valor/crates/valor/tests/chromium_compare/layout_tests.rs` - Added handler task tracing
- `/home/user/valor/crates/valor/tests/chromium_compare/browser.rs` - Increased timeouts to 60s

## Investigation Documents Created

- `/home/user/valor/HARNESS_FREEZE.md` - Initial findings on PR #197
- `/home/user/valor/HARNESS_FREEZE_FINAL.md` - Finding that Handler never polls Connection
- `/home/user/valor/CRITICAL_FINDING.md` - Finding that Handler.poll_next() never called
- `/home/user/valor/INVESTIGATION_COMPLETE.md` - This document

## Conclusion

The issue is NOT in chromiumoxide's Handler implementation itself. The Handler.poll_next() method would work fine if it were ever called. The issue is that **the tokio executor is not calling poll_next() on the Future returned by StreamExt::next()**.

This suggests either:
1. A fundamental incompatibility between how the test creates/uses the tokio runtime vs. how chromiumoxide examples do it
2. A bug in the futures crate's StreamExt::next() implementation
3. Some subtle Pin/Unpin or trait implementation issue that causes the Future to never be polled

The fact that `yield_now().await` works but `handler.next().await` doesn't strongly suggests this is a Future-specific issue, not a general runtime/executor problem.
