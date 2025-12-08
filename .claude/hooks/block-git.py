#!/usr/bin/env python3
import json
import sys

input_data = json.load(sys.stdin)
command = input_data.get("tool_input", {}).get("command", "")

if command.strip().startswith("cargo test"):
    print("Run the tests with nextest, not test!", file=sys.stderr)
    sys.exit(2)

sys.exit(0)
