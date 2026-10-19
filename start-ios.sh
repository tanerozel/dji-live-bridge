#!/bin/sh
# iOS uygulamasını (Stream My Drone) derleyip bağlı iPhone/iPad'e kurar ve açar.
#
#   ./start-ios.sh                      # USB ya da Wi-Fi ile bağlı iPhone/iPad
#   ./start-ios.sh --sim                # simülatör (varsayılan: iPhone 18 Pro)
#   ./start-ios.sh --sim "iPhone 17"    # belirli bir simülatör
#
# Birden fazla cihaz bağlıysa hangisi olacağını DJI_IOS_DEVICE ile seçin (ad ya da UDID).
# App Store'a gidecek sürüm bu betikle değil Xcode'un Archive'ı ile hazırlanır
# (mobile/ios/APP_STORE_LISTING_EN_US.md).

set -eu

REPO_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
IOS_DIR="$REPO_DIR/mobile/ios"
PROJECT="$IOS_DIR/StreamMyDrone.xcodeproj"
SCHEME="StreamMyDrone"
APP_ID="com.streammydrone.app"
DERIVED="$IOS_DIR/build"

MODE=device
SIM_NAME="iPhone 18 Pro"
case "${1:-}" in
    "") ;;
    --sim)
        MODE=sim
        [ -n "${2:-}" ] && SIM_NAME=$2
        ;;
    *)
        echo "Kullanım: $0 [--sim [simülatör adı]]" >&2
        exit 1
        ;;
esac

if [ "$(uname -s)" != "Darwin" ] || ! command -v xcodebuild >/dev/null 2>&1; then
    echo "Hata: bu betik Xcode kurulu bir Mac içindir." >&2
    exit 1
fi

echo "[1/5] Metinler kontrol ediliyor (Android strings.xml → iOS)..."
if command -v node >/dev/null 2>&1; then
    if ! node "$IOS_DIR/scripts/import-android-strings.mjs" --check >/dev/null 2>&1; then
        echo "      iOS metinleri eski, yeniden üretiliyor..."
        node "$IOS_DIR/scripts/import-android-strings.mjs"
    fi
else
    echo "      Uyarı: node yok, metin kontrolü atlandı."
fi

if [ "$MODE" = sim ]; then
    echo "[2/5] Simülatör aranıyor: $SIM_NAME"
    DEVICE_ID=$(xcrun simctl list devices available -j | /usr/bin/python3 -c '
import json, sys
name = sys.argv[1]
for runtime, devices in json.load(sys.stdin)["devices"].items():
    if "iOS" in runtime:
        for device in devices:
            if device["name"] == name:
                print(device["udid"])
                sys.exit(0)
' "$SIM_NAME")
    if [ -z "$DEVICE_ID" ]; then
        echo "Hata: \"$SIM_NAME\" adlı simülatör yok. Mevcut olanlar:" >&2
        xcrun simctl list devices available | grep -E "iPhone|iPad" >&2
        exit 1
    fi
    xcrun simctl boot "$DEVICE_ID" 2>/dev/null || true
    xcrun simctl bootstatus "$DEVICE_ID" -b >/dev/null
    PRODUCTS="$DERIVED/Build/Products/Debug-iphonesimulator"
else
    echo "[2/5] Bağlı iPhone/iPad aranıyor (biraz sürebilir)..."
    # Xcode'un kullanabildiği fiziksel cihazlar: "Ineligible" bölümünden önceki iOS satırları.
    DESTINATIONS=$(xcodebuild -showdestinations -project "$PROJECT" -scheme "$SCHEME" 2>/dev/null \
        | awk '/Ineligible destinations/ { exit } /platform:iOS,/ && !/Simulator/ && !/placeholder/' \
        | sed -n 's/.*id:\([^,]*\),.* name:\(.*\) }.*/\1	\2/p')
    if [ -n "${DJI_IOS_DEVICE:-}" ]; then
        MATCH=$(printf '%s\n' "$DESTINATIONS" | awk -F '\t' -v want="$DJI_IOS_DEVICE" '$1 == want || $2 == want')
    else
        MATCH=$DESTINATIONS
    fi
    COUNT=$(printf '%s\n' "$MATCH" | awk 'NF { count++ } END { print count + 0 }')
    if [ "$COUNT" -eq 0 ]; then
        if [ -n "${DJI_IOS_DEVICE:-}" ]; then
            echo "Hata: DJI_IOS_DEVICE=$DJI_IOS_DEVICE bağlı değil ya da Xcode kullanamıyor." >&2
        else
            echo "Hata: Bağlı iPhone/iPad bulunamadı." >&2
        fi
        echo "      Cihazı bağlayın, kilidini açın, \"Bu bilgisayara güven\"i onaylayın ve" >&2
        echo "      Ayarlar → Gizlilik ve Güvenlik → Geliştirici Modu'nu açın. Simülatör için: $0 --sim" >&2
        exit 1
    fi
    if [ "$COUNT" -gt 1 ]; then
        echo "Hata: Birden fazla cihaz bağlı. DJI_IOS_DEVICE ile birini seçin (ad ya da UDID):" >&2
        printf '%s\n' "$MATCH" | awk -F '\t' '{ print "      " $2 "  (" $1 ")" }' >&2
        exit 1
    fi
    DEVICE_ID=$(printf '%s\n' "$MATCH" | cut -f1)
    echo "      Cihaz: $(printf '%s\n' "$MATCH" | cut -f2) ($DEVICE_ID)"
    PRODUCTS="$DERIVED/Build/Products/Debug-iphoneos"
fi

echo "[3/5] Debug sürümü derleniyor (ilk derleme Rust röleyi de derler, birkaç dakika sürer)..."
LOG="$DERIVED/start-ios.log"
mkdir -p "$DERIVED"
if ! xcodebuild build \
    -project "$PROJECT" \
    -scheme "$SCHEME" \
    -configuration Debug \
    -destination "id=$DEVICE_ID" \
    -derivedDataPath "$DERIVED" \
    -allowProvisioningUpdates >"$LOG" 2>&1; then
    grep -E "error:|Signing|provisioning" "$LOG" | sort -u | head -20 >&2 || true
    echo "Hata: derleme başarısız. Tam çıktı: $LOG" >&2
    exit 1
fi
APP="$PRODUCTS/StreamMyDrone.app"
if [ ! -d "$APP" ]; then
    echo "Hata: derleme bitti ama uygulama bulunamadı: $APP" >&2
    exit 1
fi

echo "[4/5] Uygulama kuruluyor..."
if [ "$MODE" = sim ]; then
    xcrun simctl install "$DEVICE_ID" "$APP"
else
    xcrun devicectl device install app --device "$DEVICE_ID" "$APP" >/dev/null
fi

echo "[5/5] Stream My Drone açılıyor..."
if [ "$MODE" = sim ]; then
    xcrun simctl launch --terminate-running-process "$DEVICE_ID" "$APP_ID" >/dev/null
    # Xcode 27'de simülatör penceresi Device Hub'da; eski Xcode'larda Simulator.app.
    open -b com.apple.dt.Devices 2>/dev/null || open -a Simulator 2>/dev/null || true
else
    if ! xcrun devicectl device process launch --device "$DEVICE_ID" --terminate-existing "$APP_ID" >/dev/null 2>&1; then
        echo "Hata: uygulama kuruldu ama açılamadı. Cihazın kilidi açık mı? İlk kurulumda" >&2
        echo "      Ayarlar → Genel → VPN ve Cihaz Yönetimi'nden geliştiriciye güvenin." >&2
        exit 1
    fi
fi

echo "Tamamlandı. Cihaz: $DEVICE_ID"
