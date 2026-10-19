#!/bin/sh

set -eu

REPO_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
ANDROID_DIR="$REPO_DIR/mobile/android"
SDK_DIR=${ANDROID_SDK_ROOT:-/Users/tanerozel/Library/Android/sdk}
JBR_DIR=${JAVA_HOME:-/Applications/Android Studio.app/Contents/jbr/Contents/Home}
ADB_BIN="$SDK_DIR/platform-tools/adb"
APK_PATH="$ANDROID_DIR/app/build/outputs/apk/debug/app-debug.apk"
APP_ID="com.streammydrone.app"
MAIN_ACTIVITY="$APP_ID/.MainActivity"

if [ ! -x "$ADB_BIN" ]; then
    echo "Hata: adb bulunamadı: $ADB_BIN" >&2
    exit 1
fi

if [ ! -x "$ANDROID_DIR/gradlew" ]; then
    echo "Hata: Gradle wrapper bulunamadı: $ANDROID_DIR/gradlew" >&2
    exit 1
fi

echo "[1/5] ADB başlatılıyor ve fiziksel telefon aranıyor..."
"$ADB_BIN" start-server >/dev/null

if [ -n "${DJI_ANDROID_DEVICE_SERIAL:-}" ]; then
    DEVICE_SERIAL=$DJI_ANDROID_DEVICE_SERIAL
    DEVICE_STATE=$("$ADB_BIN" -s "$DEVICE_SERIAL" get-state 2>/dev/null || true)
    if [ "$DEVICE_STATE" != "device" ]; then
        echo "Hata: DJI_ANDROID_DEVICE_SERIAL=$DEVICE_SERIAL bağlı ve yetkili değil." >&2
        exit 1
    fi
else
    DEVICE_SERIALS=$("$ADB_BIN" devices | awk 'NR > 1 && $2 == "device" && $1 !~ /^emulator-/ { print $1 }')
    DEVICE_COUNT=$(printf '%s\n' "$DEVICE_SERIALS" | awk 'NF { count++ } END { print count + 0 }')

    if [ "$DEVICE_COUNT" -eq 0 ]; then
        echo "Hata: Bağlı ve yetkili fiziksel Android telefon bulunamadı." >&2
        exit 1
    fi

    if [ "$DEVICE_COUNT" -gt 1 ]; then
        echo "Hata: Birden fazla fiziksel telefon bağlı. DJI_ANDROID_DEVICE_SERIAL ayarlayın:" >&2
        printf '%s\n' "$DEVICE_SERIALS" >&2
        exit 1
    fi

    DEVICE_SERIAL=$DEVICE_SERIALS
fi

echo "      Cihaz: $DEVICE_SERIAL"
echo "[2/5] Güncel debug APK derleniyor..."
(
    cd "$ANDROID_DIR"
    env JAVA_HOME="$JBR_DIR" ANDROID_SDK_ROOT="$SDK_DIR" ./gradlew assembleDebug
)

if [ ! -f "$APK_PATH" ]; then
    echo "Hata: Derleme tamamlandı ancak APK bulunamadı: $APK_PATH" >&2
    exit 1
fi

echo "[3/5] APK telefona yükleniyor..."
"$ADB_BIN" -s "$DEVICE_SERIAL" install -r "$APK_PATH"

echo "[4/5] Stream My Drone açılıyor..."
"$ADB_BIN" -s "$DEVICE_SERIAL" shell am start -W -n "$MAIN_ACTIVITY"

echo "[5/5] Uygulama işlemi doğrulanıyor..."
APP_PID=$("$ADB_BIN" -s "$DEVICE_SERIAL" shell pidof "$APP_ID" | tr -d '\r')
if [ -z "$APP_PID" ]; then
    echo "Hata: Uygulama açıldı ancak çalışan işlem doğrulanamadı." >&2
    exit 1
fi

echo "Tamamlandı. Cihaz: $DEVICE_SERIAL, PID: $APP_PID"
