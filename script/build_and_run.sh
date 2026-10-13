#!/bin/sh
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
MODE="${1:-run}"
case "$MODE" in run|--build-only|--verify|--debug|--logs) ;; *) echo "Usage: $0 [--build-only|--verify|--debug|--logs]" >&2; exit 2 ;; esac
cd "$ROOT"
PROFILE="${READY_AIR_PROFILE:-dev}"
case "$PROFILE" in
 dev) TARGET_DIR=debug ;;
 release) TARGET_DIR=release ;;
 *) echo "READY_AIR_PROFILE must be dev or release" >&2; exit 2 ;;
esac
# Only this app's executable is stopped; no other GPUI prototype is affected.
if [ "$MODE" != "--build-only" ]; then pkill -x ReadyForAirStudio 2>/dev/null || true; fi
sh script/cargo.sh build --locked --profile "$PROFILE" --bin ready-air-capture
DIST="${READY_AIR_DIST:-$ROOT/dist}"
APP="$DIST/ACA Studio.app"
mkdir -p "$APP/Contents/MacOS" "$ROOT/data"
cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>ReadyForAirStudio</string>
<key>CFBundleIdentifier</key><string>dev.facundo.ReadyForAirStudio</string>
<key>CFBundleName</key><string>ACA Studio</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>1</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>LSMinimumSystemVersion</key><string>13.0</string>
<key>NSPrincipalClass</key><string>NSApplication</string>
<key>NSHighResolutionCapable</key><true/>
<key>NSBluetoothAlwaysUsageDescription</key><string>Buscar y conectar tu Motorola Ready For Air Remote.</string>
<key>NSBluetoothPeripheralUsageDescription</key><string>Buscar y conectar tu Motorola Ready For Air Remote.</string>
<key>NSAppleEventsUsageDescription</key><string>Ejecutar las acciones y AppleScripts que asignaste a los botones.</string>
<key>NSMicrophoneUsageDescription</key><string>Grabar desde el micrófono que elegís al pulsar Grabar o el botón del control que asignaste y activaste.</string>
</dict></plist>
PLIST
CAPTURE="$DIST/ACA Capture.app"
mkdir -p "$CAPTURE/Contents/MacOS"
cp "target/$TARGET_DIR/ready-air-capture" "$CAPTURE/Contents/MacOS/ReadyForAirCapture"
cat > "$CAPTURE/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>ReadyForAirCapture</string>
<key>CFBundleIdentifier</key><string>dev.facundo.ReadyForAirCapture</string>
<key>CFBundleName</key><string>ACA Capture</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>1</string>
<key>LSUIElement</key><true/>
</dict></plist>
PLIST
codesign --force --sign "${READY_AIR_SIGN_IDENTITY:--}" "$CAPTURE"
codesign --verify --strict "$CAPTURE"
CAPTURE_HASH=$(shasum -a 256 "$CAPTURE/Contents/MacOS/ReadyForAirCapture")
CAPTURE_HASH=${CAPTURE_HASH%% *}
READY_AIR_CAPTURE_SHA256="$CAPTURE_HASH" sh script/cargo.sh build --locked --profile "$PROFILE" --bin ready-for-air
cp "target/$TARGET_DIR/ready-for-air" "$APP/Contents/MacOS/ReadyForAirStudio"
codesign --force --sign "${READY_AIR_SIGN_IDENTITY:--}" "$APP"
codesign --verify --strict "$APP"
case "$MODE" in
 --build-only) echo "$APP" ;;
 --debug) lldb -- "$APP/Contents/MacOS/ReadyForAirStudio" ;;
 *)
  /usr/bin/open -n "$APP" --args
  if [ "$MODE" = "--verify" ]; then sleep 2; pgrep -x ReadyForAirStudio >/dev/null; fi
  if [ "$MODE" = "--logs" ]; then /usr/bin/log stream --info --style compact --predicate 'process == "ReadyForAirStudio"'; fi
 ;;
esac
