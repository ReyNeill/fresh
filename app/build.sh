#!/bin/bash
# Builds app/build/Fresh.app: the Rust core as a static library, Swift bindings for it,
# then the SwiftUI app, bundled and ad-hoc signed.
set -euo pipefail
# rustup's default location, for shells that don't load a profile (tool prompts, IDE tasks).
command -v cargo >/dev/null || export PATH="$HOME/.cargo/bin:$PATH"
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

# Sign with the most durable identity available. A Developer ID can ship to other Macs
# (see notarize.sh); an Apple Development identity stays on this Mac but is stable, so privacy
# grants like Full Disk Access survive rebuilds; ad-hoc is the fallback (CI).
# FRESH_SIGN_IDENTITY overrides the choice.
identity="${FRESH_SIGN_IDENTITY:-}"
if [ -z "$identity" ]; then
  identities="$(security find-identity -v -p codesigning 2>/dev/null || true)"
  identity="$(grep -o '"Developer ID Application: [^"]*"' <<<"$identities" | head -1 | tr -d '"' || true)"
  [ -n "$identity" ] || identity="$(grep -o '"Apple Development: [^"]*"' <<<"$identities" | head -1 | tr -d '"' || true)"
fi
if [ -z "$identity" ]; then
  codesign --force --sign - "$bundle"
elif [[ "$identity" == "Developer ID Application:"* ]]; then
  codesign --force --options runtime --timestamp --sign "$identity" "$bundle"
else
  codesign --force --options runtime --timestamp=none --sign "$identity" "$bundle"
fi
echo "$bundle"
