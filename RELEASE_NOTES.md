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
