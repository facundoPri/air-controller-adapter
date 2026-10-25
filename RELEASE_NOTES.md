# v0.1.0-preview.4 — Direct Bluetooth pairing and retry

Adds **Connection → Pair a remote** and **Search again** in ACA, including when an unpaired MR-01 is absent from macOS Bluetooth Settings. The virtual keyboard from preview.3 remains enabled.

## Changes

- Start a new discovery attempt independently of ACA's previous device selection. Existing macOS pairings and button mappings are preserved.
- Stop the previous session and wait for cleanup before searching or connecting to a selected candidate; ignore events from an older attempt.
- Retry a failed search directly in ACA, with guidance to hold Back + OK until the LED flashes.
- Show Bluetooth discovery before reader setup, and require HID exposure rather than a BLE link alone to complete the pairing checklist.

## Download and update

Download `ACA-v0.1.0-preview.4-macos-arm64.zip` for Apple Silicon. The archive includes ACA.app with its embedded reader. `SHA256SUMS.txt` and `VALIDATION.json` identify and describe the artifact.

Back up `~/Library/Application Support/ReadyForAirLab/`. In an existing single-app installation, disconnect, wait for cleanup, remove the background service in Connection → Diagnostics, wait for success, and quit before replacing the whole app. Enable the new app's service afterward. Do not mix app and reader builds. Configuration remains v3.

The app is signed with Apple Development, not Developer ID, and is not notarized. macOS may display “Apple could not verify that it is free of malware.” For that verification warning, follow the README's linked Apple Open Anyway guidance. This release does not resolve the distribution-signing warning.

## Validation and remaining limits

The previous source-bound QA run verified that Pair a remote starts a real BLE scan, an absent MR-01 exposes Search again, and retry starts another search without opening Settings. The app quit cleanly, the user's configuration was unchanged, and no service or privacy settings were changed during that run.

**Fresh physical pairing is still unverified.** The scan found no advertising MR-01. No existing pairing was removed, no new bond was demonstrated, and no physical input was received during this check. This is a prerelease of the discovery/recovery changes, not a claim that the reported hardware connection problem has been fully resolved.

Native keyboard insertion, the full remote routing/display/lifecycle matrix, fresh-machine installation, and reader permission recovery limitations from preview.3 remain. Tab still sends once and closes. Release-specific automated and package results are included in the GitHub release notes and VALIDATION.json.

---

# v0.1.0-preview.3 — Experimental ACA keyboard

This prerelease adds ACA's own virtual keyboard and packages ACA as one application with an embedded managed reader. It includes the accumulated Studio redesign and reader lifecycle improvements since preview.2. The keyboard is experimental; native acceptance remains incomplete.

## Included

- Configurable Toggle keyboard action and a dedicated Keyboard mapping profile, with directional navigation, selection and deletion.
- English/Spanish, QWERTY/alphabetical layouts, adjustable size/opacity, draggable panel, non-typing preview and a delayed test in another app.
- Destination validation and session cancellation when focus changes; Return and Tab send once and close.
- Configuration v3 migration with preserved v1/v2 backups. Legacy macOS On-screen keyboard assignments remain unchanged.
- Single ACA.app with a signed internal reader, background-service controls, updated button editor and lifecycle handling.
- Voice removal and the capture idle CPU fix from the earlier local work.

## Download and setup

Apple Silicon only. Download `ACA-v0.1.0-preview.3-macos-arm64.zip`; `SHA256SUMS.txt` contains its checksum. Follow the included README for installation, permissions, keyboard setup and rollback.

Signed with Apple Development, not Developer ID; not notarized. Gatekeeper and fresh-machine installation have not been validated. The reader and app must stay together. Do not install the embedded reader separately.

Back up `~/Library/Application Support/ReadyForAirLab/` before upgrading. Remove an existing candidate's background service through Connection → Diagnostics and wait for success before replacing it. Older builds cannot read configuration v3; rollback requires restoring the saved pre-upgrade configuration.

## Known limitations

- Native text insertion in TextEdit, browser and terminal, complete physical remote keyboard navigation/selection, and displays/Spaces/fullscreen behavior remain unverified.
- Tab sends once and closes; verified continuation to the next field is not implemented.
- Reader Input Monitoring can require manually locating the embedded helper. Retry may retain a stale permission denial; removing and re-enabling the service recovered the earlier QA instance.
- Connected CPU measurements and the full settings/lifecycle acceptance matrix remain pending.
- Shortcut recording and the full planned shortcut catalog are not included.

Earlier QA captured physical Left/Right presses and releases after a reader restart. That evidence is not a native acceptance result for this release artifact. This preview is intended for testing.
