#!/bin/sh
# Local single-app archive. This does not notarize, install, register, or publish.
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT"
DIST="${READY_AIR_DIST:-$ROOT/dist/unified}"
APP="$DIST/ACA.app"
EXE="$APP/Contents/MacOS/ReadyForAirStudio"
READER="$APP/Contents/Library/Helpers/ACA Reader.app"
INFO=$("$EXE" --build-info)
case "$INFO" in 'managed-reader '*) ;; *) echo 'Only the normal managed-reader app can be packaged; probe and legacy artifacts are refused.' >&2; exit 2 ;; esac
BUILD=${INFO#managed-reader }
[ "$("$READER/Contents/MacOS/ReadyForAirCapture" --build-info)" = "$BUILD" ] || { echo 'Mixed reader/app builds' >&2; exit 1; }
[ "$(lipo -archs "$EXE")" = arm64 ]
[ "$(lipo -archs "$READER/Contents/MacOS/ReadyForAirCapture")" = arm64 ]
"$EXE" --verify-capture
codesign --verify --strict "$READER"
codesign --verify --deep --strict "$APP"
STAGE=$(mktemp -d "${TMPDIR:-/tmp/}aca-unified-package.XXXXXX")
trap 'rm -rf "$STAGE"' EXIT HUP INT TERM
mkdir -p "$STAGE/ACA" "$ROOT/dist/releases/local"
ditto --noextattr --norsrc "$APP" "$STAGE/ACA/ACA.app"
cp README.md RELEASE_NOTES.md THIRD_PARTY_NOTICES.md "$DIST/BUILD.txt" "$STAGE/ACA/"
mkdir -p "$STAGE/ACA/assets/screenshots"
cp assets/screenshots/aca-buttons.jpg "$STAGE/ACA/assets/screenshots/"
ARCHIVE="$ROOT/dist/releases/local/ACA-local-$BUILD-macos-arm64.zip"
[ ! -e "$ARCHIVE" ] || { echo 'Archive already exists; refusing to overwrite' >&2; exit 1; }
ditto --noextattr --norsrc -c -k --keepParent "$STAGE/ACA" "$ARCHIVE"
ditto -x -k "$ARCHIVE" "$STAGE/extracted"
EXTRACTED="$STAGE/extracted/ACA/ACA.app"
codesign --verify --strict "$EXTRACTED/Contents/Library/Helpers/ACA Reader.app"
codesign --verify --deep --strict "$EXTRACTED"
"$EXTRACTED/Contents/MacOS/ReadyForAirStudio" --verify-capture
[ "$("$EXTRACTED/Contents/MacOS/ReadyForAirStudio" --build-info)" = "$INFO" ]
shasum -a 256 "$ARCHIVE" > "$ARCHIVE.sha256"
printf 'Local artifact (native gate status: see README): %s\n' "$ARCHIVE"
