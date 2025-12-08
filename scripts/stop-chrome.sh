#!/usr/bin/env bash

# Script to stop Chrome started by start-chrome.sh

set -euo pipefail

WORKSPACE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PID_DIR="$WORKSPACE_ROOT/target/chrome_pids"
CHROME_PID_FILE="$PID_DIR/chrome.pid"

if [ ! -f "$CHROME_PID_FILE" ]; then
    echo "No Chrome PID file found, nothing to stop"
    exit 0
fi

# Read PID and temp dir
CHROME_PID=$(head -n 1 "$CHROME_PID_FILE")
USER_DATA_DIR=$(tail -n 1 "$CHROME_PID_FILE")

# Kill Chrome process
if kill -0 "$CHROME_PID" 2>/dev/null; then
    kill "$CHROME_PID" 2>/dev/null || true
    # Wait up to 5 seconds for graceful shutdown
    for i in {1..10}; do
        if ! kill -0 "$CHROME_PID" 2>/dev/null; then
            break
        fi
        sleep 0.5
    done
    # Force kill if still alive
    kill -9 "$CHROME_PID" 2>/dev/null || true
    echo "Stopped Chrome (PID: $CHROME_PID)"
else
    echo "Chrome process $CHROME_PID not running"
fi

# Wait a moment for file locks to be released
sleep 0.5

# Clean up temp directory (silently ignore any remaining locks)
if [ -d "$USER_DATA_DIR" ]; then
    rm -rf "$USER_DATA_DIR" 2>/dev/null || true
fi

# Remove PID file
rm -f "$CHROME_PID_FILE"

# Clean up directory if empty
rmdir "$PID_DIR" 2>/dev/null || true
