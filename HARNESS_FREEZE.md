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

## CRITICAL FINDING
**headless_chrome navigate_to() + evaluate() is BROKEN in this environment**
- Works: about:blank (no navigation) - multiple evaluate() succeed
- Breaks: ANY navigate_to() call (file://, data:// tested)
- Even with DEFAULT LaunchOptions
- Even with fresh browser per test
- Broken in standalone minimal test project

## Must Investigate
1. Is Chrome binary itself broken?
2. Environment restriction blocking CDP after navigation?
3. Check Chrome process logs/output
4. Try different Chrome version/revision
