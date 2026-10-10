#!/usr/bin/env bash
# Packages a release build of HITO as an AppImage (ADR 0017).
#
#   packaging/linux/build-appimage.sh <hito binary> <output .AppImage>
#
# Downloads the pinned appimagetool and AppImage runtime, checks their
# checksums, and builds the AppImage from an AppDir with the binary, the
# desktop entry and the icon.
set -euo pipefail

binary=$1
output=$2
here=$(cd "$(dirname "$0")" && pwd)

# Pinned so a build doesn't change when upstream publishes a new tool.
appimagetool_url=https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage
appimagetool_sha256=ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0
runtime_url=https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-x86_64
runtime_sha256=2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

curl -fsSL -o "$work/appimagetool" "$appimagetool_url"
curl -fsSL -o "$work/runtime" "$runtime_url"
sha256sum --check --strict <<SUMS
$appimagetool_sha256  $work/appimagetool
$runtime_sha256  $work/runtime
SUMS
chmod +x "$work/appimagetool"

appdir=$work/HITO.AppDir
install -Dm755 "$binary" "$appdir/usr/bin/hito"
install -Dm644 "$here/hito.desktop" "$appdir/usr/share/applications/hito.desktop"
install -Dm644 "$here/../hito.svg" "$appdir/usr/share/icons/hicolor/scalable/apps/hito.svg"
ln -s usr/share/applications/hito.desktop "$appdir/hito.desktop"
ln -s usr/share/icons/hicolor/scalable/apps/hito.svg "$appdir/hito.svg"
ln -s hito.svg "$appdir/.DirIcon"
ln -s usr/bin/hito "$appdir/AppRun"

# appimagetool is itself an AppImage. Extracting it instead of mounting it
# means the build machine needs no FUSE.
ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$work/appimagetool" \
    --no-appstream --runtime-file "$work/runtime" "$appdir" "$output"
