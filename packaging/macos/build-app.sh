#!/usr/bin/env bash
# Packages a release build of HITO as a macOS app bundle, zipped (ADR 0017).
#
#   packaging/macos/build-app.sh <hito binary> <version> <output .zip>
#
# The bundle is signed ad hoc, not with a Developer ID, so Gatekeeper asks
# the user to allow it the first time.
set -euo pipefail

binary=$1
version=$2
output=$3
here=$(cd "$(dirname "$0")" && pwd)

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

app=$work/HITO.app
mkdir -p "$app/Contents/MacOS"
cp "$binary" "$app/Contents/MacOS/hito"
sed "s/VERSION/$version/g" "$here/Info.plist" > "$app/Contents/Info.plist"
codesign --force --sign - "$app"
ditto -c -k --keepParent "$app" "$output"
