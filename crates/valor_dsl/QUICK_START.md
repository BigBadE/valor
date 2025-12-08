# 🎯 Valor DSL - Quick Start Guide

## See It Running Now!

### Option 1: Visual Window (Full Experience) ⭐

**Opens a real window with working UI:**

```bash
cd crates/valor_dsl
cargo run --example bevy_ui --features bevy_integration --release
```

**What happens:**
1. First time: Compiles for 5-10 minutes (Bevy is large)
2. Window opens: 800x600 pixels
3. You see: Beautiful counter UI
4. You can: Click buttons to increment/decrement
5. Buttons: Light up on hover

**Screenshot description:**
```
┌─────────────────────────────────────┐
│     Purple Gradient Background      │
│                                     │
│     ┌──────────────────────┐       │
│     │   🎯 Valor Counter   │       │
│     │                      │       │
│     │         42           │       │
│     │                      │       │
│     │   [−]  [↻]  [+]     │       │
│     │                      │       │
│     │ Built with Valor DSL │       │
│     └──────────────────────┘       │
│                                     │
└─────────────────────────────────────┘
```

---

### Option 2: Terminal Test (Quick) ⚡

**No window, just shows compilation:**

```bash
cd crates/valor_dsl
cargo run --example simple_counter
```

**Output:**
```
🚀 Starting Valor Counter Example
📝 HTML length: 4378 bytes
🎨 Rendering UI...
✅ Successfully compiled HTML
📦 Generated 28 DOM updates

📋 Sample DOM Updates:
  1. InsertElement: <html>
  2. InsertElement: <head>
  3. InsertElement: <style>
  ...

✨ Completed successfully!
```

**Time:** ~30 seconds total

---

### Option 3: Run Tests ✅

**Verify everything works:**

```bash
cd crates/valor_dsl
cargo test --test simple_test
```

**Output:**
```
running 4 tests
test test_html_parsing_works ... ok
test test_attributes ... ok
test test_multiple_elements ... ok
test test_event_callback_fires ... ok

test result: ok. 4 passed
```

**Time:** ~1 minute

---

## Commands Summary

| Command | What It Does | Time | Window? |
|---------|-------------|------|---------|
| `cargo run --example bevy_ui --features bevy_integration --release` | **Opens visual UI** | 5-10min first time | ✅ Yes |
| `cargo run --example simple_counter` | Shows compilation | 30 sec | ❌ No |
| `cargo test --test simple_test` | Runs tests | 1 min | ❌ No |

## Recommended Path

1. **First:** Run `cargo run --example simple_counter` (30 seconds)
   - Quick validation that everything works

2. **Then:** Run `cargo run --example bevy_ui --features bevy_integration --release`
   - Go get coffee ☕ (compiles 5-10 min)
   - Come back to working UI!

3. **Finally:** Play with the buttons
   - Click **+** to increment
   - Click **−** to decrement
   - Click **↻** to reset
   - Hover over buttons to see effects

## Troubleshooting

### "Command not found: cargo"
- Install Rust: https://rustup.rs/

### "No such file or directory"
- Make sure you're in the right directory:
  ```bash
  cd crates/valor_dsl
  pwd  # Should show: .../valor/crates/valor_dsl
  ```

### Compilation takes forever
- That's normal for Bevy! First compile is 5-10 minutes
- Use `--release` flag for better runtime performance
- Second compile will be much faster (only changed files)

### Window doesn't open
- Check if compilation succeeded (should say "Finished")
- Try without `--release` flag
- Check terminal for error messages

## What's Next?

After seeing it work:

1. **Read the code:** `examples/bevy_ui.rs` - fully commented
2. **Modify it:** Change colors, add buttons, etc.
3. **Build your own:** Use as a template for your UI
4. **Read docs:** `README.md` and `EXAMPLES.md`

## Feature Highlights

What you just saw:

✅ **Real HTML/CSS** - No custom DSL to learn
✅ **Full Bevy Integration** - Native ECS components
✅ **Event Handling** - Rust closures for clicks
✅ **Visual Feedback** - Hover effects work
✅ **Type Safe** - Compile-time checking
✅ **Fast** - No runtime overhead

## Questions?

Check out:
- `README.md` - Full documentation
- `EXAMPLES.md` - More examples
- `examples/` - Source code for all examples

Enjoy building UIs with Valor DSL! 🚀
