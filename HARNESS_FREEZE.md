# Chrome Test Harness Freeze Investigation

## Problem
Chrome layout comparison tests hang/timeout when evaluating JavaScript after navigating to file:// URLs.

## What Works
- Chrome launches successfully
- Tab/Page creation works
- Navigation to file:// URLs completes
- about:blank evaluation works

## What Fails
- JavaScript evaluation after file:// navigation (hangs or CDP connection closes)

## Attempted Solutions

### 1. Using chromiumoxide from GitHub (FAILED)
- Hypothesis: crates.io version has CDP bugs, GitHub has fixes
- Result: Did not fix the issue
- Status: REJECTED

### 2. Using headless_chrome with 2 second sleep (FAILED)
- Found in old commit fbf9c59
- Result: Still times out after sleep
- Status: REJECTED

### 3. Using wait_until_navigated() (FAILED)
- Error: "The event waited for never came"
- CDP navigation events don't fire for file:// URLs
- Status: REJECTED

## Current Investigation
- Need to find what actually worked before
- Check if environment changed
- Verify Chrome binary version
- Test if security restrictions changed

## Key Observation
navigate_and_prepare_tab() does TWO evaluations:
1. evaluate(css_reset_injection_script()) - This might work
2. Caller then does evaluate(layout_extraction_script()) - This hangs

Hypothesis: First evaluate() works, second evaluate() causes CDP connection to close

## Test Results

### 4. navigate + 2s sleep + single evaluate() (FAILED)
- Removed css_reset evaluation from navigate_and_prepare_tab
- Used exact pattern from old commit: navigate_to() + sleep(2s) + return
- Test still hangs on the single evaluate() call after sleep
- Status: REJECTED

### 5. about:blank without navigation (SUCCESS!)
- Multiple evaluate() calls work perfectly on about:blank
- No navigation to file:// URLs
- Confirms: Chrome and headless_chrome library work fine
- **Problem is SPECIFICALLY with file:// URLs**

## CONFIRMED: ANY navigation breaks evaluate()
- about:blank (no navigation): ✅ Works perfectly
- After navigate_to("file://..."): ❌ evaluate() hangs
- After navigate_to("data:..."): ❌ evaluate() hangs

**Problem: navigate_to() itself seems to break CDP connection for evaluate()**

## Critical Question
If the old code with 2s sleep also had this problem, how did tests ever pass?
Possible answers:
1. Environment changed (Chrome security policy, permissions, etc.)
2. Tests never actually passed with file:// URLs
3. Different Chrome version was used
4. Something about tab reuse is broken

### 6. Minimal test with DEFAULT LaunchOptions (FAILED)
- Tested with LaunchOptions::default() (no custom flags)
- Still hangs on evaluate() after navigate_to("file://")
- **Problem exists even with default configuration**

### 7. HTTP URL navigation (FAILED)
- Tested navigate_to("http://example.com")
- Still hangs on evaluate() after navigation
- **Problem is not specific to file:// or data: URLs**

## CRITICAL FINDING
**headless_chrome navigate_to() + evaluate() is COMPLETELY BROKEN**
- Works: about:blank (no navigation) - multiple evaluate() succeed
- Breaks: ALL navigate_to() calls tested:
  - file:// URLs → hangs
  - data: URLs → hangs
  - http:// URLs → hangs
- Even with DEFAULT LaunchOptions
- Even with fresh browser per test
- Broken in standalone minimal test project

### 8. Different Chrome version - Chrome 120 (FAILED)
- Tested with /root/.local/share/headless-chrome/linux-1217362/chrome
- Chrome 120 from October 2023
- Still hangs on evaluate() after navigate_to()
- **Problem is not Chrome version specific**

## HYPOTHESIS: Tests Never Actually Ran
If BOTH Chrome 111 and Chrome 120 fail the same way, and this is reproducible across:
- file://, data:, http:// URLs
- DEFAULT and custom LaunchOptions
- Fresh browsers and reused browsers
- Standalone minimal test projects

Then maybe **the layout tests never actually ran successfully**. Possible explanations:
1. Tests were using cached Chrome JSON (never calling navigate/evaluate)
2. chromiumoxide WAS the solution that worked (not headless_chrome)
3. There's a fundamental bug we're missing in how we call the API

## Must Check
1. Look for cached Chrome layout JSON files
2. Verify if chromiumoxide tests actually passed
3. Check if headless_chrome examples even work in this environment

### 9. wait_for_element instead of evaluate (FAILED)
- Tried tab.wait_for_element("#test") after navigate_to()
- Also hangs indefinitely
- **ANY post-navigation interaction hangs, not just evaluate()**

## FINAL CONCLUSION
headless_chrome 1.0.18 is FUNDAMENTALLY BROKEN in this environment:
- ✅ Browser launch works
- ✅ Tab creation works  
- ✅ navigate_to() completes without error
- ❌ ANY interaction after navigate_to() hangs:
  - evaluate() - hangs
  - wait_for_element() - hangs
  - wait_until_navigated() - "event never came"

Affects ALL URLs (file://, data:, http://), both Chrome versions (111, 120).

## RECOMMENDED SOLUTIONS
1. Use chromiumoxide async library (needs async context)
2. Set up local HTTP server for fixtures
3. Investigate Docker/container security restrictions
