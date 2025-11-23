#!/bin/bash
# Run Chrome with strace to capture the failing syscall

set -e

# Create temp directory for Chrome user data
TEMP_DIR=$(mktemp -d)
trap "rm -rf $TEMP_DIR" EXIT

echo "Starting Chrome with strace..."
echo "User data dir: $TEMP_DIR"
echo ""

# Run Chrome in background with strace
# Focus on syscalls that might fail: open, mmap, clone, futex, etc.
strace -f -o /tmp/chrome_strace.log \
    -e trace=open,openat,mmap,mprotect,clone,futex,memfd_create,mremap,ioctl,prctl,seccomp \
    /opt/google/chrome/chrome \
    --headless \
    --no-sandbox \
    --disable-gpu \
    --disable-dev-shm-usage \
    --remote-debugging-port=9222 \
    --user-data-dir="$TEMP_DIR" \
    --window-size=800,600 \
    about:blank &

CHROME_PID=$!
echo "Chrome PID: $CHROME_PID"

# Wait for Chrome to start
sleep 2

# Check if Chrome is still running
if ! kill -0 $CHROME_PID 2>/dev/null; then
    echo "Chrome already crashed during startup!"
    exit 1
fi

echo "Chrome started successfully, sending HTML with text..."

# Use CDP to set content with text
curl -s http://localhost:9222/json > /tmp/targets.json
TARGET_URL=$(cat /tmp/targets.json | grep -o '"webSocketDebuggerUrl":"[^"]*"' | head -1 | cut -d'"' -f4)

if [ -z "$TARGET_URL" ]; then
    echo "Failed to get WebSocket URL"
    kill $CHROME_PID 2>/dev/null || true
    exit 1
fi

echo "WebSocket URL: $TARGET_URL"
echo ""

# Send set_content via CDP using websocat if available, otherwise use Python
if command -v websocat &> /dev/null; then
    echo '{"id":1,"method":"Page.enable"}' | websocat -n1 "$TARGET_URL" &
    sleep 0.5
    echo '{"id":2,"method":"Page.setContent","params":{"html":"<!DOCTYPE html><html><body><div>Hello</div></body></html>"}}' | websocat -n1 "$TARGET_URL"
else
    # Use Python WebSocket client
    python3 - <<PYTHON_EOF
import asyncio
import websockets
import json

async def send_content():
    async with websockets.connect('$TARGET_URL') as ws:
        await ws.send(json.dumps({"id":1,"method":"Page.enable"}))
        await ws.recv()

        await ws.send(json.dumps({
            "id":2,
            "method":"Page.setContent",
            "params":{"html":"<!DOCTYPE html><html><body><div>Hello</div></body></html>"}
        }))
        response = await ws.recv()
        print(f"Response: {response}")

asyncio.run(send_content())
PYTHON_EOF
fi

echo ""
echo "Waiting for Chrome to crash..."
sleep 3

# Check if Chrome crashed
if ! kill -0 $CHROME_PID 2>/dev/null; then
    echo "✅ Chrome crashed as expected!"
    echo ""
    echo "Analyzing strace output for crash pattern..."
    echo ""

    # Find the renderer process that crashed
    echo "=== Last 50 lines of strace before crash ==="
    tail -50 /tmp/chrome_strace.log

    echo ""
    echo "=== Looking for SIGSEGV or failures ==="
    grep -E "SIGSEGV|segfault|killed|SIGKILL|\+\+\+ killed" /tmp/chrome_strace.log | tail -20 || echo "No crash signals found in strace"
else
    echo "Chrome still running, killing..."
    kill $CHROME_PID 2>/dev/null || true
fi

echo ""
echo "Full strace log saved to: /tmp/chrome_strace.log"
