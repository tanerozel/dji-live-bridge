#!/bin/sh
# Builds the Rust relay (mobile/relay-core) as a static library for the platform Xcode is
# building, and puts it where the app links it: $BUILT_PRODUCTS_DIR/libdji_relay_core.a.
# Xcode runs this as the app's first build phase; it can also be run by hand:
#   PLATFORM_NAME=iphonesimulator BUILT_PRODUCTS_DIR=/tmp/out mobile/ios/scripts/build-relay-core.sh
set -eu

here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../.." && pwd)
crate="$repo/mobile/relay-core"

case "${PLATFORM_NAME:-}" in
  iphoneos) triple=aarch64-apple-ios ;;
  iphonesimulator) triple=aarch64-apple-ios-sim ;;
  *)
    echo "error: build-relay-core.sh: unsupported platform '${PLATFORM_NAME:-}' (iphoneos or iphonesimulator)" >&2
    exit 1
    ;;
esac
out="${BUILT_PRODUCTS_DIR:?BUILT_PRODUCTS_DIR is not set}"

# The toolchain pinned in rust-toolchain.toml, found even when cargo is not on Xcode's PATH.
channel=$(sed -n 's/^channel *= *"\(.*\)"/\1/p' "$repo/rust-toolchain.toml")
host="$(uname -m | sed 's/arm64/aarch64/')-apple-darwin"
toolchain_bin="${RUSTUP_HOME:-$HOME/.rustup}/toolchains/$channel-$host/bin"
if [ -x "$toolchain_bin/cargo" ]; then
  cargo="$toolchain_bin/cargo"
  path="$toolchain_bin:/usr/bin:/bin:/usr/sbin:/sbin"
elif command -v cargo >/dev/null 2>&1; then
  cargo=$(command -v cargo)
  path="$(dirname "$cargo"):/usr/bin:/bin:/usr/sbin:/sbin"
else
  echo "error: Rust $channel is not installed. Run: rustup toolchain install $channel && rustup target add --toolchain $channel aarch64-apple-ios aarch64-apple-ios-sim" >&2
  exit 1
fi
if [ ! -d "${RUSTUP_HOME:-$HOME/.rustup}/toolchains/$channel-$host/lib/rustlib/$triple" ] && [ -x "$toolchain_bin/cargo" ]; then
  echo "error: Rust target $triple is missing. Run: rustup target add --toolchain $channel $triple" >&2
  exit 1
fi

# Xcode's environment (SDKROOT for iOS above all) would make cargo link its macOS build
# scripts against the iOS SDK. The compilers find the right SDK through xcrun instead.
env -i \
  HOME="$HOME" \
  PATH="$path" \
  TMPDIR="${TMPDIR:-/tmp}" \
  ${CARGO_HOME:+CARGO_HOME="$CARGO_HOME"} \
  ${RUSTUP_HOME:+RUSTUP_HOME="$RUSTUP_HOME"} \
  DEVELOPER_DIR="${DEVELOPER_DIR:-$(xcode-select -p)}" \
  IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-17.0}" \
  "$cargo" rustc \
  --manifest-path "$crate/Cargo.toml" \
  --locked \
  --lib \
  --crate-type staticlib \
  --release \
  --target "$triple"

mkdir -p "$out"
library="$crate/target/$triple/release/libdji_relay_core.a"
# Copied only when it changed, so an unchanged relay does not relink the app.
if ! cmp -s "$library" "$out/libdji_relay_core.a"; then
  cp "$library" "$out/libdji_relay_core.a"
fi
