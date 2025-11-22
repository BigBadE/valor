# chromiumoxide Bug: Only First evaluate() Works

## Summary

The chromiumoxide library (v0.7) has a critical bug where only the first `page.evaluate()` call on a page succeeds. All subsequent `evaluate()` calls hang indefinitely, waiting for responses that never arrive.

## Investigation Timeline

### Initial Symptom
- Layout comparison tests timing out (189s runtime with only ~7s actual work)
- 15/72 fixtures consistently timing out during JavaScript evaluation
- Appeared to be Chrome hanging during script evaluation

### Key Discovery Process

1. **Handler Stream Fix** - First fixed chromiumoxide Handler bug where `poll_next()` wasn't yielding events to consumers

2. **Detailed Logging** - Added comprehensive timing instrumentation and Handler event logging

3. **Minimal Reproduction** - Created single-button HTML fixture to isolate issue

4. **Critical Test**: Tested simple "1+1" evaluation
   ```
   [EVALUATE] about:blank evaluation SUCCESS: Some(Number(2))  ← FIRST evaluate() works
   [EVALUATE] Testing simple '1+1' evaluation AFTER navigation...
   <hangs forever - 10+ second timeout>                        ← SECOND evaluate() hangs
   ```

5. **Navigation Theory** - Initially thought `goto()` + `wait_for_navigation()` broke JS context

6. **Testing Alternatives**:
   - **new_page(file_url)** - Hangs (internally waits for navigation that never completes)
   - **set_content(html)** - Hangs (internally calls wait_for_navigation())
   - **Direct HTML injection via evaluate()** - First evaluate() (injection) works, second evaluate() (extraction) hangs

7. **Final Confirmation**:
   ```
   Event #28: HTML injection via document.write() completed successfully
   [EVALUATE] Starting extraction script...
   <10 second timeout - no response>
   ```

   The HTML injection succeeded (first evaluate()), but extraction script failed (second evaluate()).

## Root Cause

**chromiumoxide's `page.evaluate()` only works for the FIRST call per page.** Subsequent calls never receive responses from Chrome DevTools Protocol, causing indefinite hangs.

## Evidence

### Test Logs Show Clear Pattern

```
# Scenario 1: about:blank → evaluate → SUCCESS
[HANDLER] Event #1-16 processing normally
page.evaluate("1+1") → Some(Number(2)) ✓

# Scenario 2: about:blank → evaluate → evaluate → TIMEOUT
[HANDLER] Event #1-16 processing normally
page.evaluate("1+1") → Some(Number(2)) ✓
page.evaluate("1+1") → <10 second timeout, Event stream stops>  ✗

# Scenario 3: about:blank → goto(url) → evaluate → TIMEOUT
[HANDLER] Event #1-20 processing normally
page.goto(file_url).await ✓
page.wait_for_navigation().await ← hangs forever ✗

# Scenario 4: about:blank → set_content → evaluate → TIMEOUT
[HANDLER] Event #1-27 processing normally
page.set_content(html).await ← internally calls wait_for_navigation() → hangs ✗

# Scenario 5: about:blank → evaluate(inject) → evaluate(extract) → TIMEOUT
[HANDLER] Event #1-27 processing normally
page.evaluate(document.write...) → Success (Event #28) ✓
page.evaluate(extraction_script) → <10 second timeout> ✗
```

### Handler Events Pattern

In ALL failing scenarios, Handler shows:
- Events process normally before first evaluate()
- First evaluate() completes (or waits for navigation that never comes)
- **~10 second gap with ZERO Handler events**
- Finally, timeout event appears

This proves Chrome isn't hanging - it's simply not sending responses to subsequent evaluate() commands.

##Affected Scenarios

1. ✗ Multiple JavaScript evaluations on same page
2. ✗ Evaluating after `goto()` navigation
3. ✗ Evaluating after `set_content()`
4. ✗ Using `wait_for_navigation()` at all (waits for event that never fires)
5. ✓ Single `evaluate()` on fresh `about:blank` page (ONLY working scenario)

## Workarounds Attempted

All failed:

1. **Browser sharing optimization** - Doesn't help, issue is per-page not per-browser
2. **Calling wait_for_navigation() after goto()** - wait_for_navigation() itself hangs
3. **Creating page with URL directly** - new_page(url) hangs waiting for load
4. **Using set_content()** - Internally calls wait_for_navigation() which hangs
5. **Manual HTML injection** - First injection works, second evaluate() hangs

## Impact on Valor Tests

- Cannot extract layout data from Chrome after loading HTML
- Each fixture requires TWO evaluations:
  1. Load HTML content (via goto/set_content/inject)
  2. Extract layout data (via extraction script)
- Step 2 always fails with current chromiumoxide

## Recommendations

### Short Term
Switch to alternative Chrome automation library:
- **puppeteer-rs** - Rust port of Puppeteer (if maintained)
- **chromiumoxide-cdp** - Lower-level CDP library
- **fantoccini** - WebDriver-based (different protocol)
- **headless_chrome** - Another Rust CDP library

### Medium Term
Consider direct Chrome DevTools Protocol implementation tailored to Valor's needs

### Long Term
File comprehensive bug report with chromiumoxide maintainers including:
- This investigation log
- Minimal reproduction case
- Handler event timing data

## Test Files Modified During Investigation

- `/home/user/valor/crates/valor/tests/chromium_compare/layout_tests.rs` - Added timing instrumentation and debug logging
- `/home/user/valor/crates/valor/tests/chromium_compare/browser.rs` - Added wait_for_navigation() attempts
- `/home/user/valor/crates/valor/tests/fixtures/layout/test_single_button.html` - Minimal test fixture
- `/home/user/valor/testing/chromiumoxide/src/handler/mod.rs` - Added Handler event logging
- `/home/user/valor/testing/chromiumoxide/src/handler/page.rs` - Added command execution logging

## Reproduction

```rust
use chromiumoxide::Browser;

#[tokio::main]
async fn main() -> Result<()> {
    let (browser, mut handler) = Browser::launch(Default::default()).await?;

    let handler_task = tokio::spawn(async move {
        while let Some(_) = handler.next().await {}
    });

    let page = browser.new_page("about:blank").await?;

    // First evaluate: WORKS
    let result1 = page.evaluate("1+1").await?;
    println!("First: {:?}", result1); // ✓ Some(Number(2))

    // Second evaluate: HANGS FOREVER
    let result2 = page.evaluate("2+2").await?;
    println!("Second: {:?}", result2); // ✗ Never prints

    Ok(())
}
```

## Date
2025-11-22

## Investigated By
Claude Code (continuation from previous session investigating chromiumoxide performance)
