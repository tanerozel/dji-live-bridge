#!/usr/bin/env bash
set -euo pipefail

# Fail (non-zero) if a built .app would ship a broken sidecar signature.
# Works on any bundle: build output, a mounted DMG, or /Applications.
# Usage: verify-bundle.sh <path-to-DJI Live Bridge.app>

APP_PATH="${1:?usage: verify-bundle.sh <path-to-app-bundle>}"
MACOS_DIR="$APP_PATH/Contents/MacOS"
SIDECAR="$MACOS_DIR/mediamtx"
FFMPEG="$MACOS_DIR/ffmpeg"
EXTENSION="$APP_PATH/Contents/Library/SystemExtensions/com.djilivebridge.desktop.camera.systemextension"
failures=0

fail() { echo "  FAIL: $*" >&2; failures=$((failures + 1)); }
pass() { echo "  ok:   $*"; }

echo "Verifying $APP_PATH"

[ -x "$SIDECAR" ] || { echo "  FAIL: mediamtx sidecar missing or not executable: $SIDECAR" >&2; exit 1; }

if codesign --verify --deep --strict "$APP_PATH" 2>/dev/null; then
  pass "codesign --verify --deep --strict"
else
  fail "bundle signature is invalid (re-run: npm run build:mac)"
fi

# The sidecar must NOT carry the app's restricted entitlements: without a
# provisioning profile, AMFI kills it at launch (exit 137).
sidecar_entitlements="$(codesign -d --entitlements - "$SIDECAR" 2>&1 || true)"
if grep -qE "system-extension|application-groups|com\.apple\.developer" <<<"$sidecar_entitlements"; then
  fail "mediamtx carries restricted entitlements; macOS will SIGKILL it (run: scripts/sign-sidecars.sh)"
else
  pass "mediamtx has no restricted entitlements"
fi

# The main app MUST keep them or the camera extension cannot be activated.
app_entitlements="$(codesign -d --entitlements - "$MACOS_DIR/dji-live-bridge" 2>&1 || true)"
if grep -q "system-extension.install" <<<"$app_entitlements" && grep -q "application-groups" <<<"$app_entitlements"; then
  pass "main app keeps system-extension + app-group entitlements"
else
  fail "main app lost its system-extension/app-group entitlements"
fi

if [ -d "$EXTENSION" ]; then
  codesign --verify --strict "$EXTENSION" 2>/dev/null \
    && pass "camera system extension signature" \
    || fail "camera system extension signature is invalid"
else
  fail "camera system extension is missing from the bundle"
fi

# Real launch test. AMFI rejects at exec time, so --version is enough and,
# unlike a full start, cannot collide with ports used by a running app.
set +e
version_output="$("$SIDECAR" --version 2>&1)"
status=$?
set -e
if [ "$status" -eq 0 ]; then
  pass "mediamtx launches ($version_output)"
else
  fail "mediamtx cannot launch (exit $status; 137 = killed by macOS code-signing enforcement)"
fi

# A Developer ID system extension is only activated when notarized.
# FFmpeg ships inside the app so users need no Homebrew; it must run, and it
# must not be a GPL build, which we are not allowed to redistribute this way.
if [ -x "$FFMPEG" ]; then
  if ffmpeg_version="$("$FFMPEG" -hide_banner -version 2>/dev/null | head -1)"; then
    pass "bundled $ffmpeg_version"
  else
    fail "bundled ffmpeg cannot run"
  fi
  if "$FFMPEG" -hide_banner -version 2>/dev/null | grep -q -- "--enable-gpl"; then
    fail "bundled ffmpeg is a GPL build and must not be shipped"
  else
    pass "bundled ffmpeg is an LGPL build"
  fi
  [ -f "$APP_PATH/Contents/Resources/licenses/FFmpeg-COPYING.LGPLv2.1.txt" ] \
    && pass "FFmpeg licence texts are bundled" \
    || fail "FFmpeg licence texts are missing from the bundle"
else
  fail "bundled ffmpeg is missing: $FFMPEG"
fi

if [ "${REQUIRE_NOTARIZED:-0}" = "1" ]; then
  if xcrun stapler validate "$APP_PATH" >/dev/null 2>&1; then
    pass "notarization ticket stapled"
  else
    fail "no stapled notarization ticket (the camera extension will fail with code=8)"
  fi
  if spctl -a -vv "$APP_PATH" 2>&1 | grep -q "Notarized Developer ID"; then
    pass "Gatekeeper accepts the app as notarized"
  else
    fail "Gatekeeper does not accept the app as notarized"
  fi
fi

if [ "$failures" -gt 0 ]; then
  echo "Bundle verification FAILED ($failures problem(s)). Do not ship or install this build." >&2
  exit 1
fi
echo "Bundle verification passed."
