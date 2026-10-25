#!/bin/sh
# Build the gated single-app candidate without stopping or installing another ACA.
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT"
PROFILE="${READY_AIR_PROFILE:-dev}"
case "$PROFILE" in dev) TARGET=debug ;; release) TARGET=release ;; *) exit 2 ;; esac
DIST="${READY_AIR_DIST:-$ROOT/dist/unified}"
# Never overwrite the native-verified rollback pair.
case "$DIST/" in *'/cpu-fix/'*) echo 'Refusing to overwrite the verified rollback build' >&2; exit 2 ;; esac
IDENTITY="${READY_AIR_SIGN_IDENTITY:--}"
case "$DIST/" in *'/service-gate/'*) ;; *)
    if [ "${ACA_NATIVE_PROBE:-0}" = 1 ]; then echo 'Native diagnostic builds belong in dist/service-gate only' >&2; exit 2; fi ;;
esac
export ACA_BUILD_ID="${ACA_BUILD_ID:-$(/usr/bin/uuidgen)}"
case "$ACA_BUILD_ID" in ''|*[!a-zA-Z0-9._-]*) echo 'Invalid build identifier' >&2; exit 2 ;; esac
if [ -e "$DIST/ACA.app" ]; then
    echo 'A candidate already exists. Remove its service and move the previous candidate aside before rebuilding to this path.' >&2
    exit 1
fi
FEATURE=managed-reader
if [ "${ACA_NATIVE_PROBE:-0}" = 1 ]; then FEATURE=service-probe; fi
FEATURES="$FEATURE"
if [ "${ACA_VIRTUAL_KEYBOARD:-0}" = 1 ]; then FEATURES="$FEATURES virtual-keyboard"; fi
sh script/cargo.sh build --locked --profile "$PROFILE" --features "$FEATURES" --bin ready-for-air --bin ready-air-capture
mkdir -p "$DIST"
STAGE=$(mktemp -d "$DIST/.aca-build.XXXXXX")
trap 'rm -rf "$STAGE"' EXIT HUP INT TERM
APP="$STAGE/ACA.app"
READER="$APP/Contents/Library/Helpers/ACA Reader.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Library/LaunchDaemons" "$READER/Contents/MacOS"
cp "target/$TARGET/ready-for-air" "$APP/Contents/MacOS/ReadyForAirStudio"
cp "target/$TARGET/ready-air-capture" "$READER/Contents/MacOS/ReadyForAirCapture"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>ReadyForAirStudio</string>
<key>CFBundleIdentifier</key><string>dev.facundo.ReadyForAirStudio</string>
<key>CFBundleName</key><string>ACA</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>6</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>ACAReleaseVersion</key><string>0.1.0-preview.4</string>
<key>LSMinimumSystemVersion</key><string>13.0</string>
<key>NSPrincipalClass</key><string>NSApplication</string>
<key>NSHighResolutionCapable</key><true/>
<key>NSBluetoothAlwaysUsageDescription</key><string>Discover and connect your Motorola Ready For Air Remote.</string>
<key>NSBluetoothPeripheralUsageDescription</key><string>Discover and connect your Motorola Ready For Air Remote.</string>
<key>NSAppleEventsUsageDescription</key><string>Run the actions and AppleScripts you assigned to buttons.</string>
<key>ACAReaderBuild</key><string>$ACA_BUILD_ID</string>
</dict></plist>
PLIST
cat > "$READER/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>ReadyForAirCapture</string>
<key>CFBundleIdentifier</key><string>dev.facundo.ReadyForAirCapture</string>
<key>CFBundleName</key><string>ACA Reader</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>6</string>
<key>LSUIElement</key><true/>
</dict></plist>
PLIST
cat > "$APP/Contents/Library/LaunchDaemons/dev.facundo.ReadyForAirCapture.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>Label</key><string>dev.facundo.ReadyForAirCapture</string>
<key>AssociatedBundleIdentifiers</key><array><string>dev.facundo.ReadyForAirStudio</string></array>
<key>BundleProgram</key><string>Contents/Library/Helpers/ACA Reader.app/Contents/MacOS/ReadyForAirCapture</string>
<key>ProgramArguments</key><array><string>ReadyForAirCapture</string><string>--service</string></array>
<key>MachServices</key><dict><key>dev.facundo.ReadyForAirCapture</key><true/></dict>
<key>ProcessType</key><string>Interactive</string>
</dict></plist>
PLIST
for bundle in "$READER" "$APP"; do
    codesign --force --options runtime --timestamp=none --sign "$IDENTITY" "$bundle"
    codesign --verify --strict "$bundle"
done
codesign --verify --deep --strict "$APP"
"$APP/Contents/MacOS/ReadyForAirStudio" --service-status > "$STAGE/SERVICE.txt"
"$APP/Contents/MacOS/ReadyForAirStudio" --verify-capture
[ "$("$READER/Contents/MacOS/ReadyForAirCapture" --build-info)" = "$ACA_BUILD_ID" ]
{
    printf 'Build: %s\nSource commit: %s\n' "$ACA_BUILD_ID" "$(git rev-parse HEAD)"
    if git diff --quiet HEAD --; then
        printf 'Tracked source state: clean\n'
    else
        printf 'Tracked source state: modified\n'
    fi
    printf 'Variant: %s\nKeyboard feature: %s\n' "$FEATURE" "${ACA_VIRTUAL_KEYBOARD:-0}"
    if [ "$IDENTITY" = - ]; then printf 'Signing: ad-hoc development only; service disabled\n'; else printf 'Signing: local certificate; notarization not established\n'; fi
    cat "$STAGE/SERVICE.txt"
} > "$STAGE/BUILD.txt"
if [ -e "$DIST/ACA.app" ]; then
    echo 'A candidate already exists. Remove its service and move the previous candidate aside before rebuilding to this path.' >&2
    exit 1
fi
mv "$APP" "$DIST/ACA.app"
mv "$STAGE/BUILD.txt" "$DIST/BUILD.txt"
printf '%s\n' "$DIST/ACA.app"
