#!/usr/bin/env bash
# Makes the blog's phone pictures from the localized Google Play store assets.
#
# The store screenshots are 1080×1920 JPEGs of ~300 KB; a blog page shows them
# at most ~300 px wide, so they are resized to 540 px WebP here and committed
# under site/img/app/<site language>/. scripts/build-site.mjs copies that
# folder to docs/img/app/. Run this again after the store screenshots change.
#
# Needs macOS sips and cwebp (brew install webp).
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
shots="$root/mobile/android/store-assets/promo-screenshots"
graphics="$root/mobile/android/store-assets/feature-graphic"
out="$root/site/img/app"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

command -v cwebp >/dev/null || { echo "cwebp is missing: brew install webp" >&2; exit 1; }

# site language → store locale
pairs="en:en-US tr:tr-TR es:es-ES zh:zh-CN zh-Hant:zh-TW ar:ar hi:hi-IN pt:pt-BR ru:ru-RU fr:fr-FR de:de-DE ja:ja-JP ko:ko-KR id:id-ID it:it-IT"
# store file → web name
names="01-live-hero:live 02-connect-drone:connect 03-youtube:youtube 04-instagram:instagram 05-phone-mic:mic 06-multistream:multistream 07-preview:preview 08-reconnect:reconnect"

rm -rf "$out"
for pair in $pairs; do
  code="${pair%%:*}"
  store="${pair#*:}"
  mkdir -p "$out/$code"
  for name in $names; do
    source="$shots/$store/${name%%:*}.jpg"
    [ -f "$source" ] || { echo "missing $source" >&2; exit 1; }
    sips -s format png --resampleWidth 540 "$source" --out "$tmp/shot.png" >/dev/null
    cwebp -quiet -q 80 -metadata none "$tmp/shot.png" -o "$out/$code/${name#*:}.webp"
  done
  graphic="$graphics/stream-my-drone-feature-graphic-$store-1024x500.png"
  [ -f "$graphic" ] || { echo "missing $graphic" >&2; exit 1; }
  sips -s format jpeg -s formatOptions 82 "$graphic" --out "$out/$code/share.jpg" >/dev/null
done

du -sh "$out"
