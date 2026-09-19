#!/usr/bin/env bash
set -euo pipefail

# Re-sign bundled sidecar binaries with EMPTY entitlements, then re-seal the app.
#
# WHY THIS EXISTS: `tauri build` signs every binary in Contents/MacOS with the
# app's entitlements.plist. That file holds restricted entitlements
# (system-extension.install, application-groups) that need a provisioning
# profile. A bare sidecar has none, so macOS (AMFI) SIGKILLs `mediamtx` at
# launch (exit 137) and the app fails with
#   "error sending request for url (http://127.0.0.1:9997/v3/paths/list)".
#
# Do not call this by hand. `npm run build:mac` runs it (and verifies the
# result). Usage: sign-sidecars.sh <path-to-DJI Live Bridge.app>

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
APP_PATH="${1:?usage: sign-sidecars.sh <path-to-app-bundle>}"
IDENTITY="${APPLE_SIGNING_IDENTITY:-Developer ID Application: Taner Ozel (GXDXLCQ92M)}"
SIDECAR_ENTITLEMENTS="$ROOT_DIR/src-tauri/entitlements-sidecar.plist"
APP_ENTITLEMENTS="$ROOT_DIR/src-tauri/entitlements.plist"
SIDECARS=(mediamtx ffmpeg ffprobe)

[ -d "$APP_PATH" ] || { echo "error: app bundle not found: $APP_PATH" >&2; exit 1; }
[ -f "$SIDECAR_ENTITLEMENTS" ] || { echo "error: missing $SIDECAR_ENTITLEMENTS" >&2; exit 1; }
[ -f "$APP_ENTITLEMENTS" ] || { echo "error: missing $APP_ENTITLEMENTS" >&2; exit 1; }

for name in "${SIDECARS[@]}"; do
  binary="$APP_PATH/Contents/MacOS/$name"
  [ -f "$binary" ] || { echo "error: sidecar not found in bundle: $binary" >&2; exit 1; }
  codesign --force --sign "$IDENTITY" \
    --options runtime --timestamp \
    --entitlements "$SIDECAR_ENTITLEMENTS" \
    "$binary"
done

# Changing a nested binary invalidates the app's seal, so the outer bundle must
# be re-signed (no --deep: it would overwrite the sidecar and the camera
# extension with the app entitlements again).
codesign --force --sign "$IDENTITY" \
  --options runtime --timestamp \
  --entitlements "$APP_ENTITLEMENTS" \
  "$APP_PATH"

echo "Sidecars re-signed with minimal entitlements: ${SIDECARS[*]}"
