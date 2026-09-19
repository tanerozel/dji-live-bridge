#!/usr/bin/env bash
set -euo pipefail

# The ONLY supported way to build DJI Live Bridge for macOS: `npm run build:mac`.
# A bare `tauri build` produces an app whose mediamtx sidecar is killed by macOS
# at launch (see scripts/sign-sidecars.sh), so the app cannot reach
# http://127.0.0.1:9997. This script builds, fixes signing, verifies the .app,
# then builds the .dmg FROM the fixed app and verifies that too.
#
# Usage: scripts/build-macos.sh [target-triple]   (default: this machine's arch)

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

PRODUCT="DJI Live Bridge"
HOST_ARCH="$(uname -m | sed 's/arm64/aarch64/')"
TARGET="${1:-${HOST_ARCH}-apple-darwin}"
IDENTITY="${APPLE_SIGNING_IDENTITY:-Developer ID Application: Taner Ozel (GXDXLCQ92M)}"
export APPLE_SIGNING_IDENTITY="$IDENTITY"
BUNDLE_DIR="$ROOT_DIR/src-tauri/target/$TARGET/release/bundle"
APP="$BUNDLE_DIR/macos/$PRODUCT.app"
VERSION="$(node -p "require('./package.json').version")"
DMG_ARCH="${TARGET%%-*}"
DMG="$BUNDLE_DIR/dmg/${PRODUCT}_${VERSION}_${DMG_ARCH}.dmg"

step() { printf '\n==> %s\n' "$*"; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

step "Preflight"

# cargo is not always on PATH (e.g. Codex/CI shells); use the pinned toolchain.
if ! command -v cargo >/dev/null 2>&1; then
  channel="$(sed -n 's/^channel *= *"\(.*\)"/\1/p' rust-toolchain.toml)"
  toolchain_bin="$HOME/.rustup/toolchains/${channel}-${HOST_ARCH}-apple-darwin/bin"
  [ -x "$toolchain_bin/cargo" ] || die "cargo not found and pinned toolchain missing at $toolchain_bin (run: rustup toolchain install $channel)"
  export PATH="$toolchain_bin:$PATH"
fi
echo "cargo: $(command -v cargo)"

[ -d node_modules ] || die "node_modules missing (run: npm ci)"
[ -f src-tauri/embedded.provisionprofile ] || die "src-tauri/embedded.provisionprofile missing (needed for system-extension entitlement)"

SIDECAR="src-tauri/binaries/mediamtx-$TARGET"
if [ ! -x "$SIDECAR" ]; then
  echo "MediaMTX sidecar missing for $TARGET; downloading"
  ./scripts/download-mediamtx.sh "${TARGET%%-*}"
fi
chmod 0755 "$SIDECAR"
if [ "$TARGET" = "${HOST_ARCH}-apple-darwin" ]; then
  "$SIDECAR" --version >/dev/null || die "MediaMTX sidecar does not run: $SIDECAR"
fi

if [ "$IDENTITY" != "-" ] && ! security find-identity -v -p codesigning | grep -qF "$IDENTITY"; then
  die "signing identity not found in keychain: $IDENTITY (set APPLE_SIGNING_IDENTITY, or '-' for ad-hoc)"
fi

# A running instance would keep serving old code and hold ports 1935/8554/9997.
if pgrep -f "$PRODUCT.app/Contents/MacOS/dji-live-bridge" >/dev/null 2>&1; then
  die "$PRODUCT is running; quit it before building"
fi
# An orphaned sidecar from a crashed/force-quit run holds those ports too.
# Only processes started with this app's own config file are touched.
if pkill -f "DJI Live Bridge/mediamtx.yml" 2>/dev/null; then
  echo "stopped a stale MediaMTX left over from a previous run"
  sleep 1
fi

step "Building app bundle ($TARGET)"
DJI_BUILD_MAC=1 npm run tauri -- build --target "$TARGET" --bundles app

step "Fixing sidecar signing"
./scripts/sign-sidecars.sh "$APP"

step "Verifying app bundle"
./scripts/verify-bundle.sh "$APP"

step "Creating DMG from the verified app"
stage="$(mktemp -d)"
mount_point=""
cleanup() {
  [ -n "$mount_point" ] && hdiutil detach "$mount_point" -quiet 2>/dev/null || true
  rm -rf "$stage"
}
trap cleanup EXIT
ditto "$APP" "$stage/$PRODUCT.app"
ln -s /Applications "$stage/Applications"
mkdir -p "$BUNDLE_DIR/dmg"
rm -f "$DMG"
hdiutil create -volname "$PRODUCT" -srcfolder "$stage" -ov -format UDZO "$DMG" >/dev/null
codesign --force --sign "$IDENTITY" --timestamp "$DMG"

step "Verifying the app inside the DMG"
mount_point="$(mktemp -d)"
hdiutil attach "$DMG" -nobrowse -readonly -mountpoint "$mount_point" -quiet
./scripts/verify-bundle.sh "$mount_point/$PRODUCT.app"

step "Done"
echo "App: $APP"
echo "DMG: $DMG"
echo "Not notarized: first launch needs right-click > Open. Notarize for distribution."
