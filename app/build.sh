#!/bin/bash
# Builds app/build/Fresh.app: the Rust core as a static library, Swift bindings for it, the
# SwiftUI app and its icon, bundled and signed.
#
#   FRESH_ARCHS="arm64 x86_64"   architectures to build (default: this Mac's); both make a
#                                universal app, as package.sh does for releases
#   FRESH_BUNDLE=<path>          where to put the app, so checks never replace the one you run
#   FRESH_SIGN_IDENTITY=<name>   signing identity, instead of the most durable one available
set -euo pipefail
# rustup's default location, for shells that don't load a profile (tool prompts, IDE tasks).
command -v cargo >/dev/null || export PATH="$HOME/.cargo/bin:$PATH"
root="$(cd "$(dirname "$0")/.." && pwd)"
app="$root/app"
generated="$app/Generated"
cd "$root"

triple() { [ "$1" = arm64 ] && echo aarch64-apple-darwin || echo x86_64-apple-darwin; }
read -r -a archs <<<"${FRESH_ARCHS:-$(uname -m)}"
libs=()
swift_archs=()
for arch in "${archs[@]}"; do
  cargo build -p fresh-ffi --release --target "$(triple "$arch")"
  libs+=("target/$(triple "$arch")/release/libfresh_ffi.a")
  swift_archs+=(--arch "$arch")
done
mkdir -p target/fresh
lipo -create "${libs[@]}" -output target/fresh/libfresh_ffi.a

rm -rf "$generated" && mkdir -p "$generated/FreshCore" "$generated/include"
# Library mode reads the exported API from a built library; it must run inside the workspace.
cargo run -q -p uniffi-bindgen -- generate --library "target/$(triple "${archs[0]}")/release/libfresh_ffi.dylib" \
  --language swift --out-dir "$generated/out"
mv "$generated/out/FreshCore.swift" "$generated/FreshCore/"
mv "$generated/out/FreshCoreFFI.h" "$generated/include/"
mv "$generated/out/FreshCoreFFI.modulemap" "$generated/include/module.modulemap"
rm -rf "$generated/out"
xcodebuild -create-xcframework -library target/fresh/libfresh_ffi.a \
  -headers "$generated/include" -output "$generated/FreshCoreFFI.xcframework" >/dev/null

swift build -c release --package-path "$app" "${swift_archs[@]}"
bundle="${FRESH_BUNDLE:-$app/build/Fresh.app}"
rm -rf "$bundle" && mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
cp "$(swift build -c release --package-path "$app" "${swift_archs[@]}" --show-bin-path)/Fresh" "$bundle/Contents/MacOS/"
cp "$app/Info.plist" "$bundle/Contents/"
# The icon is drawn from square, full-bleed art (Icon.svg, else Icon.png); without either,
# macOS shows its generic icon.
for art in "$app/Icon.svg" "$app/Icon.png"; do
  if [ -f "$art" ]; then
    swift "$app/icon.swift" "$art" "$bundle/Contents/Resources/AppIcon.icns"
    break
  fi
done

# Sign with the most durable identity available. A Developer ID opens anywhere once notarized
# (see notarize.sh). An Apple Development identity isn't trusted on other Macs, which ask once
# for "Open Anyway", but it's stable, so privacy grants like Full Disk Access survive updates.
# Ad-hoc is the fallback (CI).
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
