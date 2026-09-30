#!/bin/bash
# Builds a universal (Apple Silicon and Intel), signed Fresh.app and packs it into
# dist/Fresh-<version>.dmg, ready to share: drag to Applications, plus first-launch steps for
# Macs that block apps not sold through Apple.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
version="$(plutil -extract CFBundleShortVersionString raw -o - app/Info.plist)"
stage="$root/dist/Fresh-$version"
dmg="$root/dist/Fresh-$version.dmg"

rm -rf "$stage" "$dmg" && mkdir -p "$stage"
FRESH_ARCHS="arm64 x86_64" FRESH_BUNDLE="$stage/Fresh.app" ./app/build.sh
ln -s /Applications "$stage/Applications"
cp app/dmg/* "$stage/"
hdiutil create -quiet -volname "Fresh $version" -srcfolder "$stage" -ov -format UDZO "$dmg"
rm -rf "$stage"
echo "$dmg"
