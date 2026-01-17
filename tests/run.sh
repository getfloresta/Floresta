#!/bin/bash

# Sets a temporary environment to run our tests
#
# This script should be executed after prepare.sh for running our functional test.
#
## What this script do  ?
#
# 1. Sets $PATH to include the compiled florestad and utreexod at FLORESTA_TEMP_DIR/binaries.
#
# 2. Run all needed commands for batch executing all python tests suites:
#
#       uv run tests/run_tests.py
set -e

if [[ -z "$FLORESTA_TEMP_DIR" ]]; then

    # This helps us to keep track of the actual version being tested without conflicting with any already installed binaries.
    HEAD_COMMIT_HASH=$(git rev-parse HEAD)

    # This helps us to keep track of the actual version being tested without conflicting with any already installed binaries.
    GIT_DESCRIBE=$(git describe --tags --always)

    # Since its deterministic how we make the setup, we already know where to search for the binaries to be testing.
    export FLORESTA_TEMP_DIR="/tmp/floresta-func-tests.${GIT_DESCRIBE}"

fi

rm -rf "$FLORESTA_TEMP_DIR/data"
# Detect if --preserve-data-dir is among args
# and forward args to uv
PRESERVE_DATA=false
UV_ARGS=()

for arg in "$@"; do
    if [[ "$arg" == "--preserve-data-dir" ]]; then
        PRESERVE_DATA=true
    else
        UV_ARGS+=("$arg")
    fi
done

# Clean existing logs directories BEFORE running tests (unless preserving)
if [ "$PRESERVE_DATA" = false ]; then
    echo "Cleaning up test directories before running tests..."
    rm -rf "$FLORESTA_TEMP_DIR/logs"
fi

# Run the re-freshed tests
uv run ./tests/test_runner.py "${UV_ARGS[@]}"
