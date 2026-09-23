#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
ANDROID_DIR=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
REPO_DIR=$(CDPATH= cd -- "$ANDROID_DIR/../.." && pwd)
OUTPUT_DIR=${1:-"$ANDROID_DIR/app/build/generated/rustJniLibs"}
TOOLCHAIN=$(sed -n 's/.*channel *= *"\([^"]*\)".*/\1/p' "$REPO_DIR/rust-toolchain.toml" | head -n 1)
CARGO_NDK_BIN="${CARGO_HOME:-$HOME/.cargo}/bin"
CARGO=""

if command -v rustup >/dev/null 2>&1; then
  CARGO=$(rustup which cargo --toolchain "$TOOLCHAIN" 2>/dev/null || true)
fi

if [ -z "$CARGO" ]; then
  HOST_TRIPLE="$(rustc -vV 2>/dev/null | sed -n 's/^host: //p')"
  RUST_BIN="${RUSTUP_HOME:-$HOME/.rustup}/toolchains/${TOOLCHAIN}-${HOST_TRIPLE}/bin"
  CARGO="$RUST_BIN/cargo"
fi

if [ ! -x "$CARGO" ]; then
  echo "Pinned Rust $TOOLCHAIN cargo executable not found" >&2
  exit 1
fi
if [ ! -x "$CARGO_NDK_BIN/cargo-ndk" ]; then
  echo "cargo-ndk not found: $CARGO_NDK_BIN/cargo-ndk" >&2
  exit 1
fi

export PATH="$(dirname "$CARGO"):$CARGO_NDK_BIN:$PATH"
export CARGO_TARGET_DIR="$ANDROID_DIR/.rust-target"

cd "$ANDROID_DIR/../relay-core"
exec "$CARGO" ndk \
  -t arm64-v8a \
  -o "$OUTPUT_DIR" \
  build \
  --locked \
  --release
