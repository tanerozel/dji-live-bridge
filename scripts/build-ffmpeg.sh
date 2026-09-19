#!/usr/bin/env bash
set -euo pipefail

# Builds the FFmpeg and ffprobe binaries that ship inside the app, so users do
# not need Homebrew or any other install step.
#
# The build is deliberately LGPL: --disable-gpl and no external libraries, so
# no GPL component (libx264, boxblur, eq, …) ends up in the bundle. H.264 comes
# from Apple's VideoToolbox, AAC and Opus from FFmpeg's own encoders.
#
# Usage: scripts/build-ffmpeg.sh [arm64|x86_64]   (default: this machine)

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="9.0.2"
# sha256 of the official tarball, pinned after verifying the FFmpeg GPG signature.
# Verified against the FFmpeg release key FCF986EA15E6E293A5644F10B4322F04D67658D8.
SHA256="8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e"
ARCH="${1:-$(uname -m)}"
TRIPLE="$([ "$ARCH" = "arm64" ] && echo aarch64 || echo "$ARCH")-apple-darwin"
WORK="$ROOT_DIR/src-tauri/target/ffmpeg-build"
OUT="$ROOT_DIR/src-tauri/binaries"
LICENSE_DIR="$ROOT_DIR/src-tauri/licenses"

step() { printf '\n==> %s\n' "$*"; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

mkdir -p "$WORK" "$OUT" "$LICENSE_DIR"
cd "$WORK"

tarball="ffmpeg-$VERSION.tar.xz"
if [ ! -f "$tarball" ]; then
  step "Downloading FFmpeg $VERSION"
  curl -fsSL -o "$tarball" "https://ffmpeg.org/releases/$tarball"
fi

actual="$(shasum -a 256 "$tarball" | cut -d' ' -f1)"
if [ "$SHA256" = "PIN_ME" ]; then
  echo "note: recording sha256 $actual — put it in SHA256 above"
elif [ "$actual" != "$SHA256" ]; then
  die "checksum mismatch for $tarball
  expected $SHA256
  actual   $actual"
fi

src="ffmpeg-$VERSION-$ARCH"
[ -d "$src" ] || { step "Unpacking"; rm -rf "ffmpeg-$VERSION"; tar xf "$tarball"; mv "ffmpeg-$VERSION" "$src"; }
cd "$src"

# Building the Intel version on an Apple Silicon Mac (or the reverse) needs the
# compiler pointed at the other architecture; configure must not try to run its
# test binaries in that case.
cross=()
if [ "$ARCH" != "$(uname -m)" ]; then
  cross=(--enable-cross-compile --target-os=darwin
         --cc="clang -arch $ARCH" --extra-cflags="-arch $ARCH" --extra-ldflags="-arch $ARCH")
  echo "cross-compiling for $ARCH on $(uname -m)"
fi

step "Configuring (LGPL, no external libraries)"
./configure \
  --prefix="$WORK/install" \
  --arch="$ARCH" \
  "${cross[@]+"${cross[@]}"}" \
  --disable-gpl --disable-nonfree --disable-version3 \
  --disable-shared --enable-static \
  --disable-doc --disable-debug --disable-ffplay --disable-sdl2 \
  --disable-libxcb --disable-xlib --disable-lzma \
  --enable-videotoolbox --enable-audiotoolbox \
  > configure.log 2>&1 || { tail -30 configure.log; die "configure failed"; }

step "Building (this takes a few minutes)"
make -j"$(sysctl -n hw.ncpu)" > build.log 2>&1 || { tail -30 build.log; die "build failed"; }

step "Installing into the sidecar folder"
for binary in ffmpeg ffprobe; do
  [ -x "$binary" ] || die "$binary was not produced"
  install -m 0755 "$binary" "$OUT/$binary-$TRIPLE"
done
cp -f COPYING.LGPLv2.1 "$LICENSE_DIR/FFmpeg-COPYING.LGPLv2.1.txt"
cp -f LICENSE.md "$LICENSE_DIR/FFmpeg-LICENSE.md"
{
  echo "FFmpeg $VERSION, built for $TRIPLE by scripts/build-ffmpeg.sh"
  echo
  echo "Configuration:"
  "$OUT/ffmpeg-$TRIPLE" -hide_banner -version | sed -n '2p'
  echo
  echo "Source: https://ffmpeg.org/releases/ffmpeg-$VERSION.tar.xz (sha256 $actual)"
  echo "This build is LGPL: no --enable-gpl, no external libraries."
} > "$LICENSE_DIR/FFmpeg-BUILD.txt"

step "Verifying the result"
file "$OUT/ffmpeg-$TRIPLE" | sed 's/.*: //'
"$OUT/ffmpeg-$TRIPLE" -hide_banner -version | sed -n '1,2p'
if "$OUT/ffmpeg-$TRIPLE" -hide_banner -version | grep -q -- "--enable-gpl"; then
  die "the build enabled GPL; it must not be shipped"
fi
# Capture once: piping into `grep -q` kills ffmpeg with SIGPIPE, and under
# `set -o pipefail` that looks like a missing feature.
encoders="$("$OUT/ffmpeg-$TRIPLE" -hide_banner -encoders 2>/dev/null)"
filters="$("$OUT/ffmpeg-$TRIPLE" -hide_banner -filters 2>/dev/null)"
missing=""
for encoder in h264_videotoolbox aac opus; do
  grep -q " $encoder " <<<"$encoders" || missing="$missing $encoder"
done
for filter in scale crop pad overlay split fps format gblur colorlevels amix afftdn acompressor alimiter adelay volume aresample; do
  grep -qE "[ >]$filter " <<<"$filters" || missing="$missing $filter"
done
[ -z "$missing" ] || die "the build is missing:$missing"

step "Done"
ls -lh "$OUT/ffmpeg-$TRIPLE" "$OUT/ffprobe-$TRIPLE" | awk '{print $5, $9}'
