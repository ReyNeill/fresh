#!/bin/bash
# Builds app/build/Fresh.app: the Rust core as a static library, Swift bindings for it,
# then the SwiftUI app, bundled and ad-hoc signed.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
app="$root/app"
generated="$app/Generated"
cd "$root"

cargo build -p fresh-ffi --release
rm -rf "$generated" && mkdir -p "$generated/FreshCore" "$generated/include"
# Library mode reads the exported API from the built library; it must run inside the workspace.
cargo run -q -p uniffi-bindgen -- generate --library target/release/libfresh_ffi.dylib \
  --language swift --out-dir "$generated/out"
mv "$generated/out/FreshCore.swift" "$generated/FreshCore/"
mv "$generated/out/FreshCoreFFI.h" "$generated/include/"
mv "$generated/out/FreshCoreFFI.modulemap" "$generated/include/module.modulemap"
rm -rf "$generated/out"
xcodebuild -create-xcframework -library target/release/libfresh_ffi.a \
  -headers "$generated/include" -output "$generated/FreshCoreFFI.xcframework" >/dev/null

swift build -c release --package-path "$app"
bundle="$app/build/Fresh.app"
rm -rf "$bundle" && mkdir -p "$bundle/Contents/MacOS"
cp "$(swift build -c release --package-path "$app" --show-bin-path)/Fresh" "$bundle/Contents/MacOS/"
cp "$app/Info.plist" "$bundle/Contents/"
codesign --force --sign - "$bundle"
echo "$bundle"
