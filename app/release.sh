#!/bin/bash
# Publishes release v<version> on GitHub: the universal DMG, plus the update feed Sparkle
# checks (appcast.xml, found through the latest release). Bump CFBundleShortVersionString in
# app/Info.plist first, and run it from main as pushed. The update is signed with the key
# Sparkle's generate_keys put in the Keychain.
set -euo pipefail
command -v cargo >/dev/null || export PATH="$HOME/.cargo/bin:$PATH"
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
repo="ReyNeill/fresh"
version="$(plutil -extract CFBundleShortVersionString raw -o - app/Info.plist)"
tag="v$version"

[ -z "$(git status --porcelain)" ] || { echo "Commit your changes first." >&2; exit 1; }
git fetch -q origin main
[ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] || { echo "Release from main as pushed." >&2; exit 1; }
if gh release view "$tag" --repo "$repo" >/dev/null 2>&1; then
  echo "$tag is out already; bump CFBundleShortVersionString in app/Info.plist." >&2
  exit 1
fi

dmg="$(./app/package.sh | tail -1)"
# What changed since the last release, for the update window and the release page.
previous="$(gh release view --repo "$repo" --json tagName --jq .tagName 2>/dev/null || true)"
range="${previous:+$previous..}HEAD"
changes="$(git log --format='%s' "$range")"
items="$(sed 's/&/\&amp;/g; s/</\&lt;/g; s/>/\&gt;/g; s#^#<li>#; s#$#</li>#' <<<"$changes")"
# Prints: sparkle:edSignature="…" length="…"
signature="$(app/.build/artifacts/sparkle/Sparkle/bin/sign_update "$dmg")"

cat > dist/appcast.xml <<XML
<?xml version="1.0" encoding="utf-8"?>
<rss version="2.0" xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle">
  <channel>
    <title>Fresh</title>
    <item>
      <title>Fresh $version</title>
      <pubDate>$(LC_ALL=C date -u '+%a, %d %b %Y %H:%M:%S +0000')</pubDate>
      <sparkle:version>$version</sparkle:version>
      <sparkle:shortVersionString>$version</sparkle:shortVersionString>
      <sparkle:minimumSystemVersion>15.0.0</sparkle:minimumSystemVersion>
      <description><![CDATA[<ul>$items</ul>]]></description>
      <enclosure url="https://github.com/$repo/releases/download/$tag/$(basename "$dmg")" $signature type="application/octet-stream"/>
    </item>
  </channel>
</rss>
XML

notes="$(sed 's/^/- /' <<<"$changes")

**Install:** open the DMG and drag Fresh to Applications. macOS blocks the first launch once,
because Fresh isn't notarized: click Done, then System Settings ▸ Privacy & Security ▸ Open
Anyway. After that, Fresh updates itself (Fresh ▸ Check for Updates…)."
gh release create "$tag" "$dmg" dist/appcast.xml --repo "$repo" --target "$(git rev-parse HEAD)" \
  --title "Fresh $version" --notes "$notes"
