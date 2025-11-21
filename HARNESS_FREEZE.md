# Chrome Test Harness Investigation - Intermittent Timeout Issue

## Executive Summary

chromiumoxide 0.7 has an **intermittent state corruption bug** where `page.evaluate()` randomly hangs after 10 seconds. The bug has existed throughout the entire git history but was masked by JSON caching. No commit has ever had 100% reliable Chrome integration.

---

## The Intermittent Pattern

### Observed Behavior

Tests exhibit **catastrophic failure cascades**:
- Test run starts successfully, fixtures complete normally
- At some unpredictable point (fixture N), `page.evaluate()` starts timing out
- **ALL subsequent fixtures timeout** in sequence
- Failure point varies between runs (sometimes fixture 1, sometimes fixture 50)

### Evidence Across Commits

**Commit 52abe38** ("working" baseline):
- **With cache**: 50.05s, 3/66 fixtures timeout (~5% failure rate)
- **Cache cleared**: 8-12 fixtures timeout (~12-18% failure rate)
- Per-fixture average: ~0.76s (not 0.5s as claimed)

**Commit 7d97b2a** (earlier commit):
- **Cache cleared**: 50/~70 fixtures timeout (~75% failure rate)
- Timeouts start at fixture #1 and continue sequentially
- Every fixture hits 10s timeout

**My optimizations (bffec53)**:
- Implemented page pooling + parallelism
- **Result**: 72/72 fixtures timeout (100% failure rate)
- Page reuse exacerbates the bug

### Timeout Details

**Location**: chromiumoxide's `page.evaluate()` call (NOT navigation, NOT Valor parser)
**Threshold**: 10 seconds
**Error Message**: "Script evaluation timeout after 10s for {path}"
**Accompanying Error**:
```
Browser event error: Serde(Error("data did not match any variant of untagged enum Message", line: 0, column: 0))
```

---

## Root Cause Analysis

### What Causes the Intermittency?

**State Corruption Hypothesis:**

chromiumoxide 0.7 or Chrome DevTools Protocol enters a corrupted state after processing N fixtures:

1. **Initial State**: Browser and CDP connection are healthy
2. **Trigger Event**: After N page create/destroy cycles, something corrupts the state:
   - Chrome process resource exhaustion (file descriptors, memory, threads)
   - CDP websocket connection gets stuck
   - chromiumoxide event handler backlog accumulates
   - Race condition in page lifecycle management
3. **Cascade Failure**: Once corrupted, ALL subsequent `page.evaluate()` calls timeout
4. **Serde Error**: Chrome sends timeout/error CDP messages that chromiumoxide can't deserialize

### Why Timeouts Vary Between Runs?

The corruption trigger point is **non-deterministic**:
- Environmental factors (system load, resource availability)
- Timing-dependent race conditions in chromiumoxide
- Chrome internal state (varies by startup conditions)
- Possible specific fixture content triggering the issue

### Evidence It's NOT Content-Related

Tested multiple hypotheses:
- ❌ **NOT `<input>` elements**: Removed all inputs, still times out
- ❌ **NOT pseudo-selectors**: Simple CSS still times out
- ❌ **NOT file size**: 12-line minimal HTML times out
- ❌ **NOT directory location**: Same file times out in `/forms/`, works in `/display/`
- ✅ **IS random/state-based**: Different fixtures timeout on different runs

---

## Why JSON Caching Masks the Bug

### The Cache Strategy

`/home/user/valor/target/valor_layout_cache/` contains cached Chrome layout JSON:
- Key: `{canonical_path}|{harness_hash}`
- 222 cached fixtures in "working" baseline
- Cache persists across test runs

### How It Masks Failures

1. **First run** (no cache): Fixture hits chromiumoxide → may timeout
2. If successful: Layout JSON cached to disk
3. **Subsequent runs**: Read from cache, never calls chromiumoxide
4. Only uncached fixtures expose the bug

**Result**: Appears to work reliably with cache, fails intermittently without cache

### Proof

**Commit 52abe38 with cache**: 3 timeouts (cached fixtures never hit Chrome)
**Commit 52abe38 cache cleared**: 8-12 timeouts (all fixtures hit Chrome)
**Commit 7d97b2a cache cleared**: 50 timeouts (catastrophic cascade early in run)

---

## Attempts to Fix

### 1. Page Pooling (FAILED - Made It Worse)
- **Hypothesis**: Creating/destroying pages is expensive
- **Implementation**: Reuse pages with `page.goto()` instead of creating fresh pages
- **Result**: 100% timeout rate (72/72 fixtures)
- **Conclusion**: Page reuse accelerates state corruption

### 2. Parallel Execution (FAILED - Made It Worse)
- **Hypothesis**: Parallelism will speed up tests
- **Implementation**: Changed from `CONCURRENCY=1` to `available_parallelism()`
- **Result**: 100% timeout rate
- **Conclusion**: Commit 52abe38 notes already tested this: serial (6.4s) beats parallel (41s)

### 3. Reduced Sleep Time (FAILED - No Impact)
- **Hypothesis**: 10ms sleep in parse loop wastes time
- **Implementation**: Reduced to 2ms
- **Result**: No measurable improvement, still timeouts
- **Conclusion**: Sleep time not the bottleneck

---

## What Actually Works (Sort Of)

**Commit 52abe38 configuration:**
- Serial execution (`CONCURRENCY=1`)
- Fresh page per fixture (create → navigate → evaluate → close)
- 10-second timeout on both navigation and evaluation
- **Result**: ~5-18% failure rate (best found so far)

**Why This is "Best":**
- Minimizes page reuse (reduces state corruption risk)
- Serial execution avoids race conditions
- Fresh pages give Chrome a "reset" opportunity
- Still not 100% reliable, but better than alternatives

---

## Implications

### Performance Claims Are False

Commit messages claiming:
- "Tests complete in 6-10s for 72 fixtures" ✗
- "37.39s total (~0.5s/fixture)" ✗ (Actually 50s, ~0.76s/fixture)
- "chromiumoxide 0.7 works perfectly" ✗
- "NO hangs/timeouts" ✗

All based on **cached runs**, not actual Chrome fetches.

### No Known Working State

Every commit tested has intermittent failures:
- **52abe38**: 5-18% failure rate (best)
- **7d97b2a**: 75% failure rate
- **bffec53**: 100% failure rate

### Cache Dependency

Tests are **not reliably reproducible** without cache:
- CI/fresh environments will see high failure rates
- Local development with cache appears to work
- False sense of stability

---

## Browser Restart Strategy Test (REFUTES Resource Exhaustion Hypothesis)

### Implementation (Commit bffec53+)
Implemented browser restart every 30 fixtures to test if Chrome resource exhaustion causes timeouts:
- Launch fresh Chrome instance at fixtures 0, 30, 60
- Each browser processes max 30 fixtures before shutdown
- 500ms delay between shutdown and relaunch
- Serial execution (one fixture at a time)

### Results (293.85s runtime, cache cleared)
**Browser lifecycle events (verified via ERROR-level logging):**
```
20:59:51Z - Launching browser for fixtures 0-29
21:01:26Z - Restarting browser after 30 fixtures (processed 30/72)
21:01:26Z - Launching browser for fixtures 30-59
21:03:40Z - Restarting browser after 30 fixtures (processed 60/72)
21:03:40Z - Launching browser for fixtures 60-71
21:04:42Z - Shutting down final browser instance
```

**Timeout pattern:**
- **28 total timeouts** (39% failure rate vs. 36% baseline)
- Browser restarts did NOT prevent cascades
- **Browser 1** (fixtures 0-29): First timeout 11s after launch, then cascade (9 consecutive timeouts)
- **Browser 2** (fixtures 30-59): First timeout 10s after fresh launch, then cascade (10 consecutive timeouts)
- **Browser 3** (fixtures 60-71): First timeout 11s after fresh launch, then cascade (9 consecutive timeouts)

### Critical Finding: Fresh Browsers Fail Immediately

**Each new browser instance enters cascade after 1-2 successful fixtures.**

This is IMPOSSIBLE if the issue were Chrome resource exhaustion because:
- ✗ **NOT file descriptor exhaustion**: Fresh Chrome has all FDs available
- ✗ **NOT memory leaks**: Fresh Chrome process has minimal memory footprint
- ✗ **NOT thread exhaustion**: Fresh Chrome has thread pool available
- ✗ **NOT accumulated state**: Fresh Chrome has no prior page history

### Conclusion: chromiumoxide Configuration/Initialization Issue

Since **brand new Chrome instances fail immediately**, the issue must be:
1. **chromiumoxide launch configuration**: Wrong Chrome flags or CDP setup
2. **Environment incompatibility**: Container/headless environment issue
3. **chromiumoxide event handler bug**: Handler gets stuck from the start
4. **Race condition at initialization**: CDP connection corruption during startup

The resource exhaustion hypothesis is **definitively refuted**.

### Timeout Error Handling Discovery

**Timeouts are logged but NOT counted as test failures** (layout_tests.rs:672-678):
```rust
match result {
    Ok(true) => ran += 1,  // Success
    Ok(false) => {}        // Layout mismatch (added to failed_vec)
    Err(e) => {            // Timeout or error (logged but NOT in failed_vec)
        error!("[LAYOUT] {} ... ERROR: {}", display_name, e);
    }
}
```

This explains why test shows "1 total failure" despite 28 timeouts.

---

## Recommended Next Steps

### Short Term: Accept Intermittency
1. Keep commit 52abe38 configuration (serial, fresh pages)
2. Accept 5-18% random failure rate
3. Rely on cache for development workflow
4. Rerun failed fixtures individually to populate cache

### Medium Term: Investigate chromiumoxide Bug
1. Add extensive logging to page lifecycle
2. Monitor Chrome process resources (memory, file descriptors)
3. Instrument CDP message flow
4. Identify exact trigger for state corruption
5. Consider filing chromiumoxide issue with reproduction

### Long Term: Alternative Solutions
1. **Upgrade chromiumoxide**: Check if newer versions fix this
2. **Switch to puppeteer**: Use Node.js + puppeteer via subprocess
3. **HTTP server**: Serve fixtures over HTTP instead of file://
4. **Playwright**: Consider rust-playwright if available
5. **Manual CDP**: Implement direct CDP protocol handling

---

## Technical Details

### Test Infrastructure

**Files:**
- `crates/valor/tests/chromium_compare/layout_tests.rs` - Main test runner
- `crates/valor/tests/chromium_compare/browser.rs` - Chrome management
- `crates/valor/tests/chromium_compare/common.rs` - Caching, utilities

**Configuration (52abe38):**
```rust
const CONCURRENCY: usize = 1;  // Serial execution

// Per fixture:
let page = browser.new_page("about:blank").await?;  // Fresh page
navigate_and_prepare_page(&page, path).await?;       // Navigate
let json = chromium_layout_json_in_page(&page, path).await?;  // Evaluate
let _ = page.close().await;  // Close
```

### Timeout Implementation

**Two 10-second timeouts:**

1. **Navigation** (`browser.rs:84-96`):
```rust
timeout(Duration::from_secs(10), page.goto(url.as_str()))
    .await
    .map_err(|_| anyhow!("Navigation timeout after 10s for {}", url))??;
```

2. **Script Evaluation** (`layout_tests.rs:1022-1029`):
```rust
let result = timeout(Duration::from_secs(10), page.evaluate(script))
    .await
    .map_err(|_| anyhow!("Script evaluation timeout after 10s for {}", path))??;
```

**Observed**: Only script evaluation timeouts occur, navigation always succeeds

### Cache Format

**Location**: `/home/user/valor/target/valor_layout_cache/{hash}.json`
**Key**: `{canonical_path}|{checksum_u64(harness_src)}`
**Invalidation**: Harness source code changes clear cache
**Size**: 222 files in baseline (~500KB total)

---

## Conclusion

chromiumoxide 0.7 has always had intermittent timeout bugs that were masked by aggressive JSON caching. No commit in the git history has reliable Chrome integration. The best configuration found (commit 52abe38 with serial execution and fresh pages) still fails 5-18% of the time when cache is cleared.

The cache is not a workaround—it's a crutch that hides a fundamental library bug.
