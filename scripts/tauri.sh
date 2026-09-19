#!/usr/bin/env bash
set -euo pipefail

# `npm run tauri` wrapper. `tauri dev` and friends pass straight through.
# `tauri build` / `tauri bundle` are blocked unless scripts/build-macos.sh is
# driving them, because a raw build ships a mediamtx sidecar that macOS kills
# at launch ("error sending request for url (http://127.0.0.1:9997/...)").

case "${1:-}" in
  build | bundle)
    if [ -z "${DJI_BUILD_MAC:-}" ]; then
      echo "error: do not run 'tauri $1' directly; it produces a broken build." >&2
      echo "       use: npm run build:mac" >&2
      exit 1
    fi
    ;;
esac

exec "$(cd "$(dirname "$0")/.." && pwd)/node_modules/.bin/tauri" "$@"
