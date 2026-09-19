#!/usr/bin/env bash
set -euo pipefail

# The ONLY supported way to build DJI Live Bridge for macOS: `npm run build:mac`.
# A bare `tauri build` produces an app whose mediamtx sidecar is killed by macOS
# at launch (see scripts/sign-sidecars.sh), so the app cannot reach
# http://127.0.0.1:9997. This script builds, fixes signing, verifies the .app,
# then builds the .dmg FROM the fixed app and verifies that too.
#
# It also notarizes. macOS refuses to activate a Developer ID signed system
# extension that is not notarized ("code=8 OSSystemExtensionErrorDomain code
# signature invalid"), so without notarization the virtual camera never works.
# Credentials come from a notarytool keychain profile (default
# "dji-live-bridge"); set NOTARIZE=0 only for quick UI-only builds.
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
NOTARIZE="${NOTARIZE:-1}"
NOTARY_PROFILE="${NOTARY_PROFILE:-dji-live-bridge}"

step() { printf '\n==> %s\n' "$*"; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

# Submit a file, wait, and fail with Apple's log if it is not Accepted.
notarize() {
  local file="$1" result id status
  result="$(xcrun notarytool submit "$file" --keychain-profile "$NOTARY_PROFILE" --wait --output-format json)" \
    || die "notarytool submit failed for $file"
  id="$(node -e 'console.log(JSON.parse(process.argv[1]).id)' "$result")"
  status="$(node -e 'console.log(JSON.parse(process.argv[1]).status)' "$result")"
  echo "notarization $id: $status"
  if [ "$status" != "Accepted" ]; then
    xcrun notarytool log "$id" --keychain-profile "$NOTARY_PROFILE" >&2 || true
    die "notarization was not accepted for $file"
  fi
}

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

for tool in ffmpeg ffprobe; do
  binary="src-tauri/binaries/$tool-$TARGET"
  [ -x "$binary" ] || die "$binary missing. Build it once with: ./scripts/build-ffmpeg.sh ${TARGET%%-*}"
  chmod 0755 "$binary"
done
if [ "$TARGET" = "${HOST_ARCH}-apple-darwin" ]; then
  "$SIDECAR" --version >/dev/null || die "MediaMTX sidecar does not run: $SIDECAR"
fi

if [ "$IDENTITY" != "-" ] && ! security find-identity -v -p codesigning | grep -qF "$IDENTITY"; then
  die "signing identity not found in keychain: $IDENTITY (set APPLE_SIGNING_IDENTITY, or '-' for ad-hoc)"
fi

if [ "$NOTARIZE" = "1" ]; then
  [ "$IDENTITY" != "-" ] || die "notarization needs a Developer ID identity (or set NOTARIZE=0)"
  xcrun notarytool history --keychain-profile "$NOTARY_PROFILE" >/dev/null 2>&1 \
    || die "notarytool keychain profile '$NOTARY_PROFILE' is missing or invalid. Create it once with:
  xcrun notarytool store-credentials $NOTARY_PROFILE --key <AuthKey_XXXX.p8> --key-id <KEY_ID> --issuer <ISSUER_ID>
(or set NOTARIZE=0 for a UI-only build; the virtual camera will NOT activate)"
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

if [ "$NOTARIZE" = "1" ]; then
  step "Notarizing app (usually 1-5 minutes)"
  zip="$(mktemp -d)/$PRODUCT.zip"
  ditto -c -k --keepParent "$APP" "$zip"
  notarize "$zip"
  rm -f "$zip"
  xcrun stapler staple "$APP"
  REQUIRE_NOTARIZED=1 ./scripts/verify-bundle.sh "$APP"
fi

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
if [ "$NOTARIZE" = "1" ]; then
  step "Notarizing DMG"
  notarize "$DMG"
  xcrun stapler staple "$DMG"
fi

step "Verifying the app inside the DMG"
mount_point="$(mktemp -d)"
hdiutil attach "$DMG" -nobrowse -readonly -mountpoint "$mount_point" -quiet
REQUIRE_NOTARIZED="$NOTARIZE" ./scripts/verify-bundle.sh "$mount_point/$PRODUCT.app"

step "Done"
echo "App: $APP"
echo "DMG: $DMG"
if [ "$NOTARIZE" = "1" ]; then
  echo "Notarized and stapled: opens without warnings; the camera extension can be activated."
else
  echo "WARNING: NOT notarized (NOTARIZE=0). The virtual camera extension will be rejected by macOS."
fi
