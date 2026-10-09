#!/usr/bin/env bash
set -euo pipefail

# ort's 1.28 prebuilts no longer include Intel macOS. Build Microsoft's pinned
# source with CoreML and use its combined static archive, without shipping a
# separate runtime dylib. Requires full Xcode and uv; cached under target/.
ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
TARGET="${1:-x86_64-apple-darwin}"
[ "$TARGET" = "x86_64-apple-darwin" ] || { echo "unsupported target: $TARGET" >&2; exit 1; }
VERSION="1.28.0"
COMMIT="da9b5e364c465de65c49d91e696cd6485270757f"
CACHE="$ROOT_DIR/src-tauri/target/onnxruntime/$VERSION"
SOURCE="$CACHE/source"
TOOLS="$CACHE/tools"
BUILD="$CACHE/$TARGET/build"
LIB_DIR="$CACHE/$TARGET/lib"
LIB="$LIB_DIR/libonnxruntime.a"

if [ -f "$LIB" ] && [ -f "$LIB.sha256" ] && (cd "$LIB_DIR" && shasum -a 256 -c libonnxruntime.a.sha256); then
  [ "$(lipo -archs "$LIB")" = "x86_64" ] || { echo "wrong ONNX Runtime architecture" >&2; exit 1; }
  echo "Using cached ONNX Runtime $VERSION ($TARGET): $LIB"
  exit 0
fi

command -v uv >/dev/null || { echo "uv is required to prepare ONNX Runtime build tools" >&2; exit 1; }
xcodebuild -version >/dev/null
mkdir -p "$CACHE"
if [ ! -d "$SOURCE/.git" ]; then
  git clone --depth 1 --branch "v$VERSION" https://github.com/microsoft/onnxruntime.git "$SOURCE"
fi
[ "$(git -C "$SOURCE" rev-parse HEAD)" = "$COMMIT" ] || { echo "unexpected ONNX Runtime source commit" >&2; exit 1; }
[ -z "$(git -C "$SOURCE" status --porcelain --untracked-files=no)" ] || { echo "ONNX Runtime source has local modifications" >&2; exit 1; }

if [ ! -x "$TOOLS/bin/python" ]; then
  uv venv --python 3.12 "$TOOLS"
fi
uv pip install --python "$TOOLS/bin/python" cmake==3.31.6 packaging==25.0 numpy==2.2.6
export PATH="$TOOLS/bin:$PATH"
"$TOOLS/bin/python" "$SOURCE/tools/ci_build/build.py" \
  --build_dir "$BUILD" --config Release --update --build --parallel "${DJI_ORT_BUILD_JOBS:-6}" \
  --skip_tests --skip_submodule_sync --compile_no_warning_as_error \
  --use_xcode --build_apple_framework --macos MacOSX --apple_sysroot macosx \
  --apple_deploy_target 13.0 --osx_arch x86_64 --use_coreml \
  --cmake_extra_defines onnxruntime_BUILD_UNIT_TESTS=OFF FETCHCONTENT_TRY_FIND_PACKAGE_MODE=NEVER

ARCHIVE="$BUILD/Release/Release-macosx/static_framework/onnxruntime.framework/onnxruntime"
[ "$(lipo -archs "$ARCHIVE")" = "x86_64" ] || { echo "wrong ONNX Runtime architecture" >&2; exit 1; }
file "$ARCHIVE" | grep -q 'archive' || { echo "ONNX Runtime must be a static archive" >&2; exit 1; }
mkdir -p "$LIB_DIR"
cp "$ARCHIVE" "$LIB"
cp "$SOURCE/LICENSE" "$LIB_DIR/LICENSE"
(cd "$LIB_DIR" && shasum -a 256 libonnxruntime.a > libonnxruntime.a.sha256)
echo "Built ONNX Runtime $VERSION from $COMMIT: $LIB"
