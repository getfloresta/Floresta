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
check_installed() {
    if ! command -v "$1" &>/dev/null; then
        echo "You must have $1 installed to run those tests!"
        exit 1
    fi
}

check_installed uv

set -e

if [[ -z "$FLORESTA_TEMP_DIR" ]]; then

    # Since its deterministic how we make the setup, we already know where to search for the binaries to be testing.
    export FLORESTA_TEMP_DIR="/tmp/floresta-func-tests"

fi

# Clean existing data/logs directories before running the tests
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

# Run the re-freshed tests
uv run ./tests/test_runner.py "${UV_ARGS[@]}"

# Clean up the data dir if we succeeded and --preserve-data-dir was not passed
if [ $? -eq 0 ] && [ "$PRESERVE_DATA" = false ];
then
    echo "Tests passed, cleaning up the data dir at $FLORESTA_TEMP_DIR"
    rm -rf $FLORESTA_TEMP_DIR/data $FLORESTA_TEMP_DIR/logs
fi
