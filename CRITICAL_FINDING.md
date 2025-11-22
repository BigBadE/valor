# CRITICAL FINDING: Handler.poll_next() Never Called (2025-11-22)

## The Smoking Gun

**Handler.poll_next() is NEVER called by the tokio executor!**

### Evidence

1. Added `eprintln!("[HANDLER] poll_next() CALLED - THIS SHOULD APPEAR!");` at the start of Handler.poll_next()
2. Added multiple tracing logs in the handler task before/after `handler.next().await`
3. Test runs and shows:
   - `[HANDLER] Handler task started` ✓
   - `[HANDLER] About to call handler.next().await for the first time` ✓
   - `tokio::task::yield_now().await` completes ✓
   - `[HANDLER] After yield_now, about to call handler.next().await` ✓
   - **`eprintln!` from Handler.poll_next()**: ❌ NEVER APPEARS
   - **Any tracing from Handler.poll_next()**: ❌ NEVER APPEARS

### Code Flow

```rust
// layout_tests.rs:627
let handler_task = tokio::spawn(async move {
    log::error!("[HANDLER] Handler task started");  // ✓ LOGGED
    tokio::task::yield_now().await;  // ✓ EXECUTES
    log::error!("[HANDLER] After yield_now");  // ✓ LOGGED
    while let Some(event_result) = handler.next().await {  // ⚠️ BLOCKS FOREVER
        // Never reaches here
    }
});
```

```rust
// handler/mod.rs:535
fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    eprintln!("[HANDLER] poll_next() CALLED");  // ❌ NEVER EXECUTED
    // ...
}
```

### Runtime Configuration

The test uses a **multi-threaded tokio runtime**:
```rust
// layout_tests.rs:566
let runtime = Runtime::new()?;  // Default multi-threaded runtime
runtime.block_on(async {
    // ...
    let handler_task = tokio::spawn(async move { handler.next().await });
    // ...
});
```

### Analysis

1. **Handler task IS scheduled**: Logs before `handler.next().await` execute successfully
2. **Handler task CAN await**: `tokio::task::yield_now().await` completes
3. **Future from `handler.next()` is NOT polled**: Handler.poll_next() never called

This means the `StreamExt::next()` Future is created but never polled by the tokio executor.

### Possible Causes

1. **StreamExt::next() implementation issue**: Maybe the Future returned doesn't properly delegate to poll_next()?
2. **Pin/Unpin issue**: Maybe the Handler isn't properly implementing Stream?
3. **Tokio runtime bug**: Unlikely but possible
4. **chromiumoxide integration issue**: Maybe there's a specific way Handler must be used?

### Next Steps

1. Check chromiumoxide examples to see how they use the Handler
2. Verify StreamExt::next() is from the correct futures crate
3. Check if Handler implements Stream correctly (Pin, Unpin, etc.)
4. Try manually polling the Handler instead of using StreamExt::next()
