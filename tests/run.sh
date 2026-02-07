#!/bin/bash

# Sets a temporary environment to run our tests
#
# This script should be executed after prepare.sh for running our functional tests.
#
## What this script does:
#
# 1. Sets FLORESTA_TEMP_DIR environment variable to the default location if not already set.
#
# 2. Cleans up existing test data directories.
#
# 3. Parses command-line arguments and forwards them to the test runner.
#
# 4. Optionally cleans up log directories (unless --preserve-data-dir flag is passed).
#
# 5. Executes all Python test suites via:
#
#       uv run ./tests/test_runner.py
check_installed() {
    if ! command -v "$1" &>/dev/null; then
        echo "You must have $1 installed to run those tests!"
        exit 1
    fi
}

check_installed uv

set -e

if [[ -z "$FLORESTA_TEMP_DIR" ]]; then

    # Set default temporary directory for test files if not already defined
    export FLORESTA_TEMP_DIR="/tmp/floresta-func-tests"

fi
# Always clean existing test data directory before running tests
rm -rf "$FLORESTA_TEMP_DIR/data"
# Parse arguments: detect --preserve-data-dir flag and collect other args for test_runner.py
PRESERVE_DATA=false
UV_ARGS=()

for arg in "$@"; do
    if [[ "$arg" == "--preserve-data-dir" ]]; then
        PRESERVE_DATA=true
    else
        UV_ARGS+=("$arg")
    fi
done

# Conditionally clean log directory (skip if --preserve-data-dir was passed).
if [ "$PRESERVE_DATA" = false ]; then
    echo "Cleaning up test directories before running tests..."
    rm -rf "$FLORESTA_TEMP_DIR/logs"
fi

# Execute test runner with parsed arguments.
uv run ./tests/test_runner.py "${UV_ARGS[@]}"
