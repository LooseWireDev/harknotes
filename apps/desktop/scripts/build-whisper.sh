#!/usr/bin/env bash
# Build whisper.cpp's whisper-cli as a Tauri sidecar binary.
# Output: src-tauri/binaries/whisper-cli-<target-triple>
# Tauri copies it next to the app binary in dev and bundles it as externalBin.
set -euo pipefail

WHISPER_VERSION="v1.8.3"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
DESKTOP_DIR="$(dirname "$SCRIPT_DIR")"
BUILD_DIR="$DESKTOP_DIR/.whisper-build"
BIN_DIR="$DESKTOP_DIR/src-tauri/binaries"

# Tauri externalBin naming requires the rust target triple suffix.
TRIPLE="$(rustc -vV | sed -n 's/^host: //p')"
BINARY_NAME="whisper-cli-${TRIPLE}"

mkdir -p "$BIN_DIR"

if [ -f "$BIN_DIR/$BINARY_NAME" ]; then
  echo "whisper-cli already built: $BIN_DIR/$BINARY_NAME"
  exit 0
fi

echo "Building whisper.cpp $WHISPER_VERSION for $TRIPLE..."

if [ ! -d "$BUILD_DIR" ]; then
  git clone --depth 1 --branch "$WHISPER_VERSION" https://github.com/ggml-org/whisper.cpp.git "$BUILD_DIR" 2>/dev/null \
    || git clone --depth 1 --branch "$WHISPER_VERSION" https://github.com/ggerganov/whisper.cpp.git "$BUILD_DIR"
else
  cd "$BUILD_DIR"
  git fetch --depth 1 origin tag "$WHISPER_VERSION"
  git checkout "$WHISPER_VERSION"
fi

# Static link so the sidecar is fully self-contained.
cd "$BUILD_DIR"
CMAKE_EXTRA_FLAGS="-DBUILD_SHARED_LIBS=OFF"
case "$TRIPLE" in
  *apple-darwin*)
    CMAKE_EXTRA_FLAGS="$CMAKE_EXTRA_FLAGS -DGGML_METAL=ON -DGGML_METAL_EMBED_LIBRARY=ON"
    ;;
esac
cmake -B build -DCMAKE_BUILD_TYPE=Release $CMAKE_EXTRA_FLAGS
cmake --build build --config Release -j "$(nproc 2>/dev/null || sysctl -n hw.ncpu)"

cp "$BUILD_DIR/build/bin/whisper-cli" "$BIN_DIR/$BINARY_NAME"
chmod +x "$BIN_DIR/$BINARY_NAME"

echo "Built: $BIN_DIR/$BINARY_NAME"
