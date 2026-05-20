#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CRATE="floresta-bitassets-wallet"
TARGETS=("$@")

if [[ ${#TARGETS[@]} -eq 0 ]]; then
  TARGETS=(
    aarch64-apple-ios
    aarch64-apple-ios-sim
    aarch64-linux-android
    armv7-linux-androideabi
    x86_64-linux-android
  )
fi

for target in "${TARGETS[@]}"; do
  rustup target add "$target"
  cargo build --manifest-path "$ROOT_DIR/Cargo.toml" -p "$CRATE" --release --target "$target"
done

echo "Built $CRATE for: ${TARGETS[*]}"
