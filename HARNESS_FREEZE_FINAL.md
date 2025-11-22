# ACTUAL ROOT CAUSE FOUND: Handler Never Polls Connection (2025-11-22)

After adding extensive tracing to chromiumoxide, discovered the REAL bug:

## The Smoking Gun

**Connection.poll_next() is called ZERO times throughout the entire test!**

The Handler.poll_next() is being called (it logs events), but it NEVER reaches the line where it should poll the Connection Stream:

```rust
// chromiumoxide/src/handler/mod.rs:624
while let Poll::Ready(Some(ev)) = Pin::new(&mut pin.conn).poll_next(cx) {
    // This line is NEVER REACHED!
}
```

## Evidence

Added tracing to track execution:
1. `tracing::error!("[CONNECTION] poll_next() called")` in conn.rs:118
2. `tracing::error!("[HANDLER] About to poll Connection...")` before Handler polls Connection
3. `tracing::error!("[HANDLER] on_response() called")` when Response messages are processed

**Results**:
- `[CONNECTION] poll_next`: **0 calls**
- `[HANDLER] About to poll`: **0 calls**
- `[HANDLER] on_response()`: **0 calls**
- No Message::Response logged, no Message::Event logged
- Handler logs "Event #1, #2, #3..." but these are just Stream yields to the spawned task, NOT CDP messages!

## Why This Happens

The Handler.poll_next() structure (simplified):

```rust
fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    loop {
        // 1. Poll from_browser channel (line 543)
        while let Poll::Ready(Some(msg)) = Pin::new(&mut pin.from_browser).poll_next(cx) {
            match msg {
                HandlerMessage::Command(cmd) => {
                    pin.submit_external_command(cmd, now)?;  // ← CAN RETURN ERROR!
                }
                ...
            }
        }

        // 2. Poll targets (line 587)
        for n in (0..pin.target_ids.len()).rev() {
            // Poll each target
        }

        // 3. THIS IS WHERE CONNECTION SHOULD BE POLLED (line 623)
        //    BUT THIS LINE IS NEVER REACHED!
        tracing::error!("[HANDLER] About to poll Connection...");  // ← NEVER LOGGED
        while let Poll::Ready(Some(ev)) = Pin::new(&mut pin.conn).poll_next(cx) {
            ...
        }

        // 4. Return based on `done` flag (line 644)
        if done {
            return Poll::Pending;
        } else {
            return Poll::Ready(Some(Ok(())));  // ← THIS is what logs "Event #N"
        }
    }
}
```

**Possible causes** why line 623 is never reached:

1. **`submit_external_command()` fails** at line 546 (the `?` operator returns early)
2. **Another early return** somewhere in the from_browser or targets loop
3. **The outer loop exits** for some reason before reaching the Connection poll

## The Full Picture

This explains EVERYTHING we observed:

1. **Why Chrome responses never arrive**:
   - Connection.poll_next() is never called
   - WebSocket messages sit in the buffer, never read
   - Handler.on_response() never executes
   - CommandFuture waits forever on the oneshot channel

2. **Why events arrive exactly at timeout**:
   - timeout fires after N seconds
   - Timeout wakes the tokio executor
   - Executor processes all pending work
   - Handler finally runs and... still doesn't poll Connection!
   - Events #30-#32 arrive within 400µs because they're Stream yields, not real messages

3. **Why PR #197 revert didn't fix it**:
   - PR #197 was about message dropping AFTER deserialization
   - But messages are never being read from the WebSocket at all!
   - The Connection Stream is never polled

## Previous Red Herrings

- ❌ Message dropping bug (PR #197): Real bug, but not the cause of timeouts
- ❌ Handler Stream not yielding (commit ac5259f): This "fix" made it worse!
- ❌ Async executor/waker issue: Symptom, not cause
- ❌ Chrome hanging: Chrome responds fine (~146ms)

## The REAL Root Cause

**Commit ac5259f's "fix" is COMPLETELY WRONG!**

It makes the Handler return `Poll::Ready(Some(Ok(())))` after polling from_browser and targets,
**BEFORE** it ever gets a chance to poll the Connection!

The Handler:
1. Polls from_browser → no messages (Poll::Pending)
2. Polls targets → no events
3. Sets `done = true`
4. **SHOULD** poll Connection here
5. **BUT INSTEAD** commit ac5259f makes it return `Poll::Ready(Some(Ok(())))`!
6. Spawned task wakes up, logs "Event #N", calls `handler.next().await`
7. Back to step 1, loop forever, NEVER polling Connection

## The Fix

**Revert commit ac5259f** - it's fundamentally broken. The Handler should NOT return after every poll_next() call. It should only return when:
- It has actual work to report (Response or Event from Connection)
- OR when there's no more work and it should yield (Poll::Pending)

The original chromiumoxide logic was correct - ac5259f broke it completely.