#!/bin/sh
set -eu

VERSION="1.21.0"
ARCH="${1:-$(uname -m)}"
case "$ARCH" in
  arm64|aarch64)
    ARCHIVE_ARCH="arm64"
    TARGET="aarch64-apple-darwin"
    ;;
  x86_64|amd64)
    ARCHIVE_ARCH="amd64"
    TARGET="x86_64-apple-darwin"
    ;;
  *)
    echo "Unsupported macOS architecture: $ARCH" >&2
    exit 2
    ;;
esac

ROOT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
DEST="$ROOT_DIR/src-tauri/binaries/mediamtx-$TARGET"
TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT INT TERM

BASE_URL="https://github.com/bluenviron/mediamtx/releases/download/v${VERSION}"
ARCHIVE="mediamtx_v${VERSION}_darwin_${ARCHIVE_ARCH}.tar.gz"
curl --fail --location --silent --show-error "$BASE_URL/$ARCHIVE" --output "$TMP_DIR/$ARCHIVE"
curl --fail --location --silent --show-error "$BASE_URL/checksums.sha256" --output "$TMP_DIR/checksums.sha256"

EXPECTED=$(awk -v archive="$ARCHIVE" '{ sub(/^\*/, "", $2); if ($2 == archive) print $1 }' "$TMP_DIR/checksums.sha256")
if [ -z "$EXPECTED" ]; then
  echo "Checksum entry not found for $ARCHIVE" >&2
  exit 3
fi
ACTUAL=$(shasum -a 256 "$TMP_DIR/$ARCHIVE" | awk '{ print $1 }')
if [ "$EXPECTED" != "$ACTUAL" ]; then
  echo "Checksum mismatch for $ARCHIVE" >&2
  exit 4
fi

tar -xzf "$TMP_DIR/$ARCHIVE" -C "$TMP_DIR" mediamtx
install -m 0755 "$TMP_DIR/mediamtx" "$DEST"
echo "Installed MediaMTX v$VERSION for $TARGET ($ACTUAL)"
