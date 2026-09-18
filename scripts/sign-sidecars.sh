#!/usr/bin/env bash
set -euo pipefail

# Re-sign bundled sidecar binaries with minimal entitlements so that macOS
# hardened runtime accepts them under the notarization profile. Tauri applies
# the main app entitlements.plist (which requires a matching provisioning
# profile) to every embedded binary; that trips amfid for third-party sidecars
# like mediamtx.

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
APP_PATH="${1:-$ROOT_DIR/src-tauri/target/release/bundle/macos/DJI Live Bridge.app}"
IDENTITY="${APPLE_SIGNING_IDENTITY:-Developer ID Application: Taner Ozel (GXDXLCQ92M)}"
SIDECAR_ENTITLEMENTS="$ROOT_DIR/src-tauri/entitlements-sidecar.plist"
APP_ENTITLEMENTS="$ROOT_DIR/src-tauri/entitlements.plist"

codesign --force --sign "$IDENTITY" \
  --options runtime --timestamp \
  --entitlements "$SIDECAR_ENTITLEMENTS" \
  "$APP_PATH/Contents/MacOS/mediamtx"

codesign --force --sign "$IDENTITY" \
  --options runtime --timestamp \
  --entitlements "$APP_ENTITLEMENTS" \
  "$APP_PATH"
