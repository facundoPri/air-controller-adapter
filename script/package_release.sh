#!/bin/sh
# Package an already-built, matching Studio/Capture pair without opening the app.
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT"
TAG="${1:-v0.1.0-preview.1}"
case "$TAG" in ''|*[!a-zA-Z0-9._-]*) echo 'Invalid release tag' >&2; exit 2 ;; esac
DIST="${READY_AIR_DIST:-$ROOT/dist/release}"
OUTPUT="$ROOT/dist/releases/$TAG"
STUDIO="$DIST/ACA Studio.app"
CAPTURE="$DIST/ACA Capture.app"
codesign --verify --strict "$STUDIO"
codesign --verify --strict "$CAPTURE"
"$STUDIO/Contents/MacOS/ReadyForAirStudio" --verify-capture
ARCH=$(lipo -archs "$STUDIO/Contents/MacOS/ReadyForAirStudio")
[ "$ARCH" = arm64 ] || { echo "Expected arm64, found $ARCH" >&2; exit 1; }
[ "$(lipo -archs "$CAPTURE/Contents/MacOS/ReadyForAirCapture")" = arm64 ]
STAGE=$(mktemp -d "${TMPDIR:-/tmp/}aca-package.XXXXXX")
trap 'rm -rf "$STAGE"' EXIT HUP INT TERM
mkdir -p "$STAGE/ACA/docs/releases" "$OUTPUT"
ditto --noextattr --norsrc "$STUDIO" "$STAGE/ACA/ACA Studio.app"
ditto --noextattr --norsrc "$CAPTURE" "$STAGE/ACA/ACA Capture.app"
cp README.md THIRD_PARTY_NOTICES.md "$STAGE/ACA/"
cp docs/configuration.md docs/config.example.json docs/voice-setup.md docs/native-engine.md docs/mr01-button-map.md docs/development.md "$STAGE/ACA/docs/"
cp "docs/releases/$TAG.md" "$STAGE/ACA/docs/releases/"
{
 printf 'Release: %s\nSource commit: %s\nArchitecture: arm64\nSigning: ' "$TAG" "$(git rev-parse HEAD)"
 codesign -dvv "$STUDIO" 2>&1 | sed -n '/^Signature=/p; /^Authority=/p'
 shasum -a 256 "$STAGE/ACA/ACA Studio.app/Contents/MacOS/ReadyForAirStudio" "$STAGE/ACA/ACA Capture.app/Contents/MacOS/ReadyForAirCapture" | sed "s|$STAGE/ACA/||"
} > "$STAGE/ACA/BUILD.txt"
ARCHIVE="ACA-$TAG-macos-arm64.zip"
ditto --noextattr --norsrc -c -k --keepParent "$STAGE/ACA" "$OUTPUT/$ARCHIVE"
# Verify the extracted artifact, not only the build directory.
ditto -x -k "$OUTPUT/$ARCHIVE" "$STAGE/unpacked"
codesign --verify --strict "$STAGE/unpacked/ACA/ACA Studio.app"
codesign --verify --strict "$STAGE/unpacked/ACA/ACA Capture.app"
"$STAGE/unpacked/ACA/ACA Studio.app/Contents/MacOS/ReadyForAirStudio" --verify-capture
(cd "$OUTPUT" && shasum -a 256 "$ARCHIVE" > SHA256SUMS.txt)
printf 'Archive: %s/%s\n' "$OUTPUT" "$ARCHIVE"
