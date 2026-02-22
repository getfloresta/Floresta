#!/bin/bash

# Run integration tests; before running tests, clean node data directories and logs.
# If the --preserve-data-dir flag is passed, do not clean the logs.
#
# This script should be executed after prepare.sh.
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

# Clean existing data directories before running the tests
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

# Clean up logsm data files and other stuffs
# if --preserve-data-dir was not passed
if [ "$PRESERVE_DATA" = false ]; then
    echo "Cleaning up test directories before running tests..."
    sh contrib/clean_data.sh
fi

# Run the tests
uv run ./tests/test_runner.py "${UV_ARGS[@]}"
