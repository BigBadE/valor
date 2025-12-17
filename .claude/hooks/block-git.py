#!/usr/bin/env python3
import json
import sys

input_data = json.load(sys.stdin)
command = input_data.get("tool_input", {}).get("command", "")

if "cargo test" in command.strip().lower():
    print("Run the tests with nextest, not test!", file=sys.stderr)
    sys.exit(2)

if "--test" in command.strip().lower():
    print("Run all of the tests, not just one!", file=sys.stderr)
    sys.exit(2)

if command.strip().lower().startswith("git"):
    print("DO NOT USE GIT! EVER!", file=sys.stderr)
    sys.exit(2)

sys.exit(0)
