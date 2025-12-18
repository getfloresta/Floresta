#!/bin/bash

# Sets a temporary environment to run our tests
#
# This script should be executed after prepare.sh for running our functional test.
#
## What this script do  ?
#
# Run the all functional tests located at tests/ directory using the test runner or pytest.
set -e

USE_TEST_RUNNER=true
USE_PYTEST=true
PRESERVE_DATA=false
TEST_RUNNER_ARGS=()
for arg in "$@"; do
  case "$arg" in
  --test-runner) USE_PYTEST=false ;;
  --pytest) USE_TEST_RUNNER=false ;;
  --preserve-data-dir) PRESERVE_DATA=true ;;
  --)
    shift
    TEST_RUNNER_ARGS+=("$@")
    break
    ;;
  --*) TEST_RUNNER_ARGS+=("$arg") ;;
  *) TEST_RUNNER_ARGS+=("$arg") ;;
  esac
done

if [[ -z "$FLORESTA_TEMP_DIR" ]]; then

    # This helps us to keep track of the actual version being tested without conflicting with any already installed binaries.
    HEAD_COMMIT_HASH=$(git rev-parse HEAD)

    # This helps us to keep track of the actual version being tested without conflicting with any already installed binaries.
    GIT_DESCRIBE=$(git describe --tags --always)

    # Since its deterministic how we make the setup, we already know where to search for the binaries to be testing.
    export FLORESTA_TEMP_DIR="/tmp/floresta-func-tests.${GIT_DESCRIBE}"

fi

# Clean existing data/logs directories before running the tests
rm -rf "$FLORESTA_TEMP_DIR/data"

# Run the re-freshed tests
if [ "$USE_TEST_RUNNER" = true ]; then
  echo "FLORESTA_TEMP_DIR=$FLORESTA_TEMP_DIR uv run ./tests/test_runner.py ${TEST_RUNNER_ARGS[@]}"
  uv run ./tests/test_runner.py "${TEST_RUNNER_ARGS[@]}"
fi

if [ "$USE_PYTEST" = true ]; then
  echo "FLORESTA_TEMP_DIR=$FLORESTA_TEMP_DIR uv run pytest ${TEST_RUNNER_ARGS[@]}"
  uv run pytest "${TEST_RUNNER_ARGS[@]}"
fi

# Clean up the data dir if we succeeded and --preserve-data-dir was not passed
if [ "$PRESERVE_DATA" = false ]; then
  echo "Tests passed, cleaning up the data dir at $FLORESTA_TEMP_DIR"
  rm -rf $FLORESTA_TEMP_DIR/data $FLORESTA_TEMP_DIR/logs
fi
