#!/bin/bash
# Notarizes and staples app/build/Fresh.app, so other Macs open it without a warning.
# Needs a Developer ID Application certificate (build.sh signs with it when it's in the
# keychain) and notary credentials, stored once with:
#   xcrun notarytool store-credentials fresh-notary
set -euo pipefail
app="$(cd "$(dirname "$0")" && pwd)"
bundle="$app/build/Fresh.app"
profile="${FRESH_NOTARY_PROFILE:-fresh-notary}"

if ! codesign -dv --verbose=2 "$bundle" 2>&1 | grep -q "Authority=Developer ID Application"; then
  echo "Fresh.app isn't signed with a Developer ID Application certificate; run app/build.sh after adding one." >&2
  exit 1
fi
zip="$app/build/Fresh.zip"
ditto -c -k --keepParent "$bundle" "$zip"
xcrun notarytool submit "$zip" --keychain-profile "$profile" --wait
xcrun stapler staple "$bundle"
rm "$zip"
