#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
ANDROID_DIR=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
REPO_DIR=$(CDPATH= cd -- "$ANDROID_DIR/../.." && pwd)
OUTPUT_DIR=${1:-"$ANDROID_DIR/app/build/generated/rustJniLibs"}
TOOLCHAIN=$(sed -n 's/.*channel *= *"\([^"]*\)".*/\1/p' "$REPO_DIR/rust-toolchain.toml" | head -n 1)
RUST_BIN="${RUSTUP_HOME:-$HOME/.rustup}/toolchains/${TOOLCHAIN}-aarch64-apple-darwin/bin"
CARGO_NDK_BIN="${CARGO_HOME:-$HOME/.cargo}/bin"

if [ ! -x "$RUST_BIN/cargo" ]; then
  echo "Pinned Rust toolchain not found: $RUST_BIN/cargo" >&2
  exit 1
fi
if [ ! -x "$CARGO_NDK_BIN/cargo-ndk" ]; then
  echo "cargo-ndk not found: $CARGO_NDK_BIN/cargo-ndk" >&2
  exit 1
fi

export PATH="$RUST_BIN:$CARGO_NDK_BIN:$PATH"
export CARGO_TARGET_DIR="$ANDROID_DIR/.rust-target"

cd "$ANDROID_DIR/../relay-core"
exec "$RUST_BIN/cargo" ndk \
  -t arm64-v8a \
  -o "$OUTPUT_DIR" \
  build \
  --locked \
  --release
