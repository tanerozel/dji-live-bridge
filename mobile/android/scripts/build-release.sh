#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
ANDROID_DIR=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)

if [ -z "${DJI_ANDROID_KEYSTORE_PATH:-}" ] ||
  [ -z "${DJI_ANDROID_KEYSTORE_PASSWORD:-}" ] ||
  [ -z "${DJI_ANDROID_KEY_ALIAS:-}" ] ||
  [ -z "${DJI_ANDROID_KEY_PASSWORD:-}" ]; then
  echo "Release signing variables are missing." >&2
  echo "Set DJI_ANDROID_KEYSTORE_PATH, DJI_ANDROID_KEYSTORE_PASSWORD," >&2
  echo "DJI_ANDROID_KEY_ALIAS and DJI_ANDROID_KEY_PASSWORD." >&2
  exit 1
fi

if [ ! -f "$DJI_ANDROID_KEYSTORE_PATH" ]; then
  echo "Release keystore not found: $DJI_ANDROID_KEYSTORE_PATH" >&2
  exit 1
fi

if [ -z "${JAVA_HOME:-}" ] || [ ! -x "$JAVA_HOME/bin/jarsigner" ]; then
  echo "JAVA_HOME must point to a JDK 17 installation with jarsigner." >&2
  exit 1
fi

if [ -n "${ANDROID_SDK_ROOT:-}" ]; then
  SDK_ROOT=$ANDROID_SDK_ROOT
elif [ -n "${ANDROID_HOME:-}" ]; then
  SDK_ROOT=$ANDROID_HOME
else
  echo "ANDROID_SDK_ROOT or ANDROID_HOME must point to the Android SDK." >&2
  exit 1
fi

APKSIGNER="$SDK_ROOT/build-tools/36.1.0/apksigner"
if [ ! -x "$APKSIGNER" ]; then
  echo "Android Build Tools 36.1 apksigner was not found at $APKSIGNER." >&2
  exit 1
fi

cd "$ANDROID_DIR"
./gradlew --no-configuration-cache clean lintRelease testDebugUnitTest assembleRelease bundleRelease

APK="$ANDROID_DIR/app/build/outputs/apk/release/app-release.apk"
BUNDLE="$ANDROID_DIR/app/build/outputs/bundle/release/app-release.aab"

if [ ! -f "$APK" ] || [ ! -f "$BUNDLE" ]; then
  echo "Expected release APK/AAB output is missing." >&2
  exit 1
fi

"$APKSIGNER" verify --verbose --print-certs "$APK"
BUNDLE_VERIFY_OUTPUT=$("$JAVA_HOME/bin/jarsigner" -verify "$BUNDLE" 2>&1)
if printf '%s\n' "$BUNDLE_VERIFY_OUTPUT" | grep -q "jar verified."; then
  echo "AAB signature: verified"
else
  printf '%s\n' "$BUNDLE_VERIFY_OUTPUT" >&2
  echo "Release AAB signature verification failed." >&2
  exit 1
fi

echo "Release artifacts:"
shasum -a 256 "$APK" "$BUNDLE"
