#!/bin/bash
# Manually launch Chrome with strace to capture crash

TEMP_DIR=$(mktemp -d)
trap "pkill -P $$ chrome 2>/dev/null; rm -rf $TEMP_DIR" EXIT

echo "=== Launching Chrome with strace on renderer processes ==="
echo "Temp dir: $TEMP_DIR"

# Launch Chrome with OLD headless mode + ALL chromiumoxide DEFAULT_ARGS
/opt/google/chrome/chrome \
    --headless \
    --hide-scrollbars \
    --mute-audio \
    --no-sandbox \
    --disable-setuid-sandbox \
    --disable-gpu \
    --remote-debugging-port=9333 \
    --user-data-dir="$TEMP_DIR" \
    --window-size=800,600 \
    --disable-background-networking \
    --enable-features=NetworkService,NetworkServiceInProcess \
    --disable-background-timer-throttling \
    --disable-backgrounding-occluded-windows \
    --disable-breakpad \
    --disable-client-side-phishing-detection \
    --disable-component-extensions-with-background-pages \
    --disable-default-apps \
    --disable-dev-shm-usage \
    --disable-extensions \
    --disable-features=TranslateUI \
    --disable-hang-monitor \
    --disable-ipc-flooding-protection \
    --disable-popup-blocking \
    --disable-prompt-on-repost \
    --disable-renderer-backgrounding \
    --disable-sync \
    --force-color-profile=srgb \
    --metrics-recording-only \
    --no-first-run \
    --enable-automation \
    --password-store=basic \
    --use-mock-keychain \
    --enable-blink-features=IdleDetection \
    --lang=en_US \
    about:blank &

CHROME_PID=$!
echo "Chrome PID: $CHROME_PID"

# Wait for Chrome to start
sleep 3

# Check Chrome is running
if ! ps -p $CHROME_PID > /dev/null; then
    echo "Chrome failed to start!"
    exit 1
fi

echo "Chrome started, checking for renderer processes..."

# Find renderer processes
RENDERER_PIDS=$(pgrep -P $CHROME_PID)
echo "Renderer PIDs: $RENDERER_PIDS"

# Get target info
echo ""
echo "Getting CDP targets..."
curl -s http://localhost:9333/json/list | python3 -m json.tool > /tmp/targets.json 2>/dev/null || {
    echo "Failed to get targets"
    kill $CHROME_PID
    exit 1
}

WS_URL=$(python3 -c "import json; data=json.load(open('/tmp/targets.json')); print(data[0]['webSocketDebuggerUrl'])" 2>/dev/null)

if [ -z "$WS_URL" ]; then
    echo "Failed to extract WebSocket URL"
    cat /tmp/targets.json
    kill $CHROME_PID
    exit 1
fi

echo "WebSocket URL: $WS_URL"

# Now attach strace to existing renderer processes BEFORE sending content
echo ""
echo "Attaching strace to renderer processes..."

# Just attach to the first renderer we find
for PID in $RENDERER_PIDS; do
    if [ -f "/proc/$PID/cmdline" ]; then
        CMDLINE=$(cat /proc/$PID/cmdline | tr '\0' ' ')
        echo "Process $PID cmdline: ${CMDLINE:0:200}"
        if echo "$CMDLINE" | grep -q "chrome"; then
            echo "Attaching strace to process: $PID"
            strace -f -p $PID -o "/tmp/renderer_${PID}_strace.log" 2>&1 &
            STRACE_PID=$!
            echo "Strace attached (PID: $STRACE_PID)"
            sleep 1
            break
        fi
    fi
done

echo ""
echo "Sending HTML with text content..."

# Send content via CDP using Python
export WS_URL
python3 << PYTHON_EOF
import asyncio
import websockets
import json
import os

async def send_html():
    ws_url = os.environ.get('WS_URL')
    print(f"Connecting to: {ws_url}")

    try:
        async with websockets.connect(ws_url) as ws:
            # Enable Page domain
            await ws.send(json.dumps({"id":1, "method":"Page.enable"}))
            resp = await ws.recv()
            print(f"Page.enable: {resp}")

            # Get execution context
            await ws.send(json.dumps({"id":2, "method":"Runtime.enable"}))
            resp = await ws.recv()
            print(f"Runtime.enable: {resp}")

            # Use document.write() like chromiumoxide does
            html = '<!DOCTYPE html><html><body><div>Hello World</div></body></html>'
            js_code = '''
            (html) => {
                document.open();
                document.write(html);
                document.close();
            }
            '''

            await ws.send(json.dumps({
                "id":3,
                "method":"Runtime.callFunctionOn",
                "params":{
                    "functionDeclaration": js_code,
                    "arguments": [{"value": html}],
                    "objectId": None
                }
            }))
            resp = await asyncio.wait_for(ws.recv(), timeout=3.0)
            print(f"callFunctionOn: {resp}")

            print("Content sent, waiting for crash...")
            await asyncio.sleep(2)

    except Exception as e:
        print(f"Error: {e}")

asyncio.run(send_html())
PYTHON_EOF

echo ""
echo "Waiting for potential crash..."
sleep 3

# Check if renderer still exists
for PID in $RENDERER_PIDS; do
    if ! ps -p $PID > /dev/null 2>&1; then
        echo "✅ Renderer process $PID crashed!"
        echo ""
        echo "=== Strace output for crashed renderer ==="
        if [ -f "/tmp/renderer_${PID}_strace.log" ]; then
            tail -100 "/tmp/renderer_${PID}_strace.log"
            echo ""
            echo "Full log: /tmp/renderer_${PID}_strace.log"
        fi
    else
        echo "Renderer process $PID still running"
    fi
done

# Cleanup
kill $CHROME_PID 2>/dev/null

echo ""
echo "Available strace logs:"
ls -lh /tmp/renderer_*_strace.log 2>/dev/null || echo "None found"
