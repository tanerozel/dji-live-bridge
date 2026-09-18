#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
SOURCE_DIR="$ROOT_DIR/native/camera-extension"
BUNDLE_DIR="$ROOT_DIR/src-tauri/camera-extension/com.djilivebridge.desktop.camera.systemextension"
EXECUTABLE_DIR="$BUNDLE_DIR/Contents/MacOS"
EXECUTABLE="$EXECUTABLE_DIR/DJILiveBridgeCamera"
SDK_PATH="$(xcrun --sdk macosx --show-sdk-path)"
ARCH="${TAURI_ENV_ARCH:-$(uname -m)}"
TARGET="${ARCH}-apple-macos13.0"
MODULE_CACHE="$ROOT_DIR/src-tauri/target/swift-module-cache"

mkdir -p "$EXECUTABLE_DIR" "$MODULE_CACHE"
cp "$SOURCE_DIR/Info.plist" "$BUNDLE_DIR/Contents/Info.plist"

xcrun swiftc \
  -O \
  -parse-as-library \
  -target "$TARGET" \
  -sdk "$SDK_PATH" \
  -module-cache-path "$MODULE_CACHE" \
  -framework Foundation \
  -framework CoreMedia \
  -framework CoreMediaIO \
  -framework CoreVideo \
  "$SOURCE_DIR/Sources/main.swift" \
  "$SOURCE_DIR/Sources/CameraExtensionProvider.swift" \
  -o "$EXECUTABLE"

chmod 0755 "$EXECUTABLE"

# Embed camera extension provisioning profile if available
CAMERA_PROFILE_PATH="${CAMERA_EXTENSION_PROFILE:-}"
if [ -z "$CAMERA_PROFILE_PATH" ]; then
  # Search installed profiles for the camera extension bundle ID
  for f in "$HOME/Library/MobileDevice/Provisioning Profiles/"*.provisionprofile; do
    if [ -f "$f" ] && security cms -D -i "$f" 2>/dev/null | grep -q "com.djilivebridge.desktop.camera"; then
      CAMERA_PROFILE_PATH="$f"
      break
    fi
  done
fi
if [ -n "$CAMERA_PROFILE_PATH" ] && [ -f "$CAMERA_PROFILE_PATH" ]; then
  cp "$CAMERA_PROFILE_PATH" "$BUNDLE_DIR/Contents/embedded.provisionprofile"
  echo "Camera extension provisioning profile embedded"
fi

codesign --force --sign "${APPLE_SIGNING_IDENTITY:-Developer ID Application: Taner Ozel (GXDXLCQ92M)}" \
  --options runtime \
  --entitlements "$SOURCE_DIR/CameraExtension.entitlements" \
  "$BUNDLE_DIR"
