#!/usr/bin/env bash

set -euo pipefail

COMMIT_HASH=$(git log -1 --oneline)

echo -e "\n================================================"
echo -e " running tests for commit: $COMMIT_HASH"
echo -e "================================================\n"

cargo build --workspace --release
cargo test --workspace --release
