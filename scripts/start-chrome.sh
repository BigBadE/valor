#!/usr/bin/env bash

# Nextest setup script to start Chrome for chromium comparison tests
# See: https://nexte.st/book/setup-scripts.html

set -euo pipefail

CHROME_PORT=19222
WORKSPACE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PID_DIR="$WORKSPACE_ROOT/target/chrome_pids"
CHROME_PID_FILE="$PID_DIR/chrome.pid"

# Find Chrome executable
find_chrome() {
    if command -v google-chrome &> /dev/null; then
        echo "google-chrome"
    elif command -v chromium &> /dev/null; then
        echo "chromium"
    elif command -v chromium-browser &> /dev/null; then
        echo "chromium-browser"
    elif [ -f "/c/Program Files/Google/Chrome/Application/chrome.exe" ]; then
        echo "/c/Program Files/Google/Chrome/Application/chrome.exe"
    elif [ -f "/mnt/c/Program Files/Google/Chrome/Application/chrome.exe" ]; then
        echo "/mnt/c/Program Files/Google/Chrome/Application/chrome.exe"
    else
        echo ""
    fi
}

# Clear failing test artifacts directory
FAILING_DIR="$WORKSPACE_ROOT/target/test_cache/graphics/failing"
if [ -d "$FAILING_DIR" ]; then
    rm -rf "$FAILING_DIR"
fi

# Check if Chrome is already running on the port
if curl -s "http://localhost:$CHROME_PORT/json/version" > /dev/null 2>&1; then
    echo "Chrome already running on port $CHROME_PORT"
    exit 0
fi

CHROME_BIN=$(find_chrome)
if [ -z "$CHROME_BIN" ]; then
    echo "ERROR: Chrome/Chromium not found" >&2
    exit 1
fi

# Create temp directory for user data
USER_DATA_DIR=$(mktemp -d -t chrome-valor-XXXXXX)

# Create PID directory
mkdir -p "$PID_DIR"

# Chrome args - combined settings for both layout and graphics tests
CHROME_ARGS=(
    "--remote-debugging-port=$CHROME_PORT"
    "--user-data-dir=$USER_DATA_DIR"
    "--headless=new"
    "--disable-gpu"
    "--no-sandbox"
    "--disable-dev-shm-usage"
    "--disable-extensions"
    "--disable-background-networking"
    "--disable-sync"
    "--force-device-scale-factor=1"
    "--hide-scrollbars"
    "--blink-settings=imagesEnabled=false"
    "--disable-features=OverlayScrollbar"
    "--allow-file-access-from-files"
    "--force-color-profile=sRGB"
    "--window-size=800,600"
)

# Launch Chrome in background
"$CHROME_BIN" "${CHROME_ARGS[@]}" > /dev/null 2>&1 &
CHROME_PID=$!

# Save PID and temp dir for cleanup
echo "$CHROME_PID" > "$CHROME_PID_FILE"
echo "$USER_DATA_DIR" >> "$CHROME_PID_FILE"

# Wait for Chrome to start accepting connections
MAX_WAIT=10
WAITED=0
while ! curl -s "http://localhost:$CHROME_PORT/json/version" > /dev/null 2>&1; do
    sleep 0.5
    WAITED=$((WAITED + 1))
    if [ $WAITED -ge $((MAX_WAIT * 2)) ]; then
        echo "ERROR: Chrome on port $CHROME_PORT failed to start within ${MAX_WAIT}s" >&2
        kill -9 "$CHROME_PID" 2>/dev/null || true
        rm -rf "$USER_DATA_DIR"
        exit 1
    fi
done

echo "Started Chrome on port $CHROME_PORT (PID: $CHROME_PID)"
