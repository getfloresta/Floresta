#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CRATE="floresta-bitassets-wallet"
LIB_NAME="floresta_bitassets_wallet"
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

if command -v xcodebuild >/dev/null 2>&1; then
  ios_args=()
  for target in "${TARGETS[@]}"; do
    case "$target" in
      aarch64-apple-ios|aarch64-apple-ios-sim|x86_64-apple-ios)
        lib="$ROOT_DIR/target/$target/release/lib$LIB_NAME.a"
        if [[ -f "$lib" ]]; then
          ios_args+=("-library" "$lib" "-headers" "$ROOT_DIR/crates/$CRATE/include")
        fi
        ;;
    esac
  done

  if [[ ${#ios_args[@]} -gt 0 ]]; then
    rm -rf "$ROOT_DIR/target/$LIB_NAME.xcframework"
    xcodebuild -create-xcframework "${ios_args[@]}" -output "$ROOT_DIR/target/$LIB_NAME.xcframework" >/dev/null
    echo "Packaged target/$LIB_NAME.xcframework"
  fi
fi
