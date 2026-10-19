#!/bin/sh
# Masaüstü uygulamasını derleyip geliştirme modunda (tauri dev) açar.
#
#   ./start-desktop.sh            # hızlı derleme (debug)
#   ./start-desktop.sh --release  # optimize derleme; AI Vision daha hızlı çalışır
#
# Dağıtılacak .app/.dmg için bu değil `npm run build:mac` kullanılır.
# Not: sanal kamera yalnızca /Applications altındaki imzalı ve notarize edilmiş
# uygulamada çalışır; geliştirme modunda kamera eklentisi etkinleşmez.

set -eu

REPO_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
cd "$REPO_DIR"

HOST_ARCH=$(uname -m | sed 's/arm64/aarch64/')
TARGET="$HOST_ARCH-apple-darwin"
DEBUG_DIR="$REPO_DIR/src-tauri/target/debug"

DEV_ARGS=""
case "${1:-}" in
    "") ;;
    --release) DEV_ARGS="--release" ;;
    *)
        echo "Kullanım: $0 [--release]" >&2
        exit 1
        ;;
esac

if [ "$(uname -s)" != "Darwin" ]; then
    echo "Hata: bu betik macOS içindir. Windows sürümü CI'da derlenir." >&2
    exit 1
fi

echo "[1/5] Rust araç zinciri kontrol ediliyor..."
if ! command -v cargo >/dev/null 2>&1; then
    CHANNEL=$(sed -n 's/^channel *= *"\(.*\)"/\1/p' rust-toolchain.toml)
    TOOLCHAIN_BIN="$HOME/.rustup/toolchains/$CHANNEL-$TARGET/bin"
    if [ ! -x "$TOOLCHAIN_BIN/cargo" ]; then
        echo "Hata: cargo bulunamadı: $TOOLCHAIN_BIN (çalıştırın: rustup toolchain install $CHANNEL)" >&2
        exit 1
    fi
    PATH="$TOOLCHAIN_BIN:$PATH"
    export PATH
fi
echo "      cargo: $(command -v cargo)"

echo "[2/5] npm paketleri kontrol ediliyor..."
if [ ! -d node_modules ]; then
    npm ci
fi

echo "[3/5] Yan uygulamalar (MediaMTX, FFmpeg) kontrol ediliyor..."
MEDIAMTX="src-tauri/binaries/mediamtx-$TARGET"
if [ ! -x "$MEDIAMTX" ]; then
    echo "      MediaMTX eksik, indiriliyor..."
    ./scripts/download-mediamtx.sh "$HOST_ARCH"
fi
for tool in ffmpeg ffprobe; do
    if [ ! -x "src-tauri/binaries/$tool-$TARGET" ]; then
        echo "Hata: src-tauri/binaries/$tool-$TARGET yok. Bir kez derleyin: ./scripts/build-ffmpeg.sh $HOST_ARCH" >&2
        exit 1
    fi
done

echo "[4/5] Çalışan bir kopya aranıyor..."
if pgrep -f "^($DEBUG_DIR/|target/debug/)dji-live-bridge" >/dev/null 2>&1 \
    || pgrep -f "^/.*/DJI Live Bridge.app/Contents/MacOS/" >/dev/null 2>&1; then
    echo "Hata: DJI Live Bridge zaten açık. Önce kapatın." >&2
    exit 1
fi
if lsof -nP -iTCP:1935 -iTCP:8554 -iTCP:9997 -sTCP:LISTEN >/dev/null 2>&1; then
    echo "      Uyarı: 1935/8554/9997 portları kullanımda. Önceki çalıştırmadan kalan"
    echo "      MediaMTX ise uygulama açılırken kendisi kapatır; değilse o programı kapatın."
fi

# tauri dev uygulamayı sert kapattığında MediaMTX ve FFmpeg sahipsiz kalabiliyor
# (1935/8554/9997 portlarını tutarlar). Çıkışta yalnızca bu deponun geliştirme
# kopyalarını kapat; sistemdeki başka FFmpeg'lere dokunma.
cleanup() {
    for name in mediamtx ffmpeg; do
        pkill -f "$DEBUG_DIR/$name" 2>/dev/null || true
    done
}
trap cleanup EXIT
trap 'exit 130' INT TERM

echo "[5/5] Derleniyor ve açılıyor (kapatmak için pencereyi kapatın veya Ctrl+C)..."
# shellcheck disable=SC2086 # DEV_ARGS boş ya da tek bir bayrak.
npm run tauri -- dev $DEV_ARGS
