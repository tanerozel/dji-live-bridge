#!/usr/bin/env bash
set -euo pipefail

# Fail (non-zero) if a built .app would ship a broken sidecar signature.
# Works on any bundle: build output, a mounted DMG, or /Applications.
# Usage: verify-bundle.sh <path-to-DJI Live Bridge.app>

APP_PATH="${1:?usage: verify-bundle.sh <path-to-app-bundle>}"
MACOS_DIR="$APP_PATH/Contents/MacOS"
SIDECAR="$MACOS_DIR/mediamtx"
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

if [ "$failures" -gt 0 ]; then
  echo "Bundle verification FAILED ($failures problem(s)). Do not ship or install this build." >&2
  exit 1
fi
echo "Bundle verification passed."
