# Native MR-01 engine — 2026-10-13

**Validation status:** Studio's integrated foreground capture process passed the physical Capture → Mission Control trial and user-observed cursor forwarding. Mouse input activated Air and Up returned the app to Normal in the exported logs. Pausing actions returned to shared observation; closing Studio terminated the capture helper. This remains a manually authenticated, at-most-30-minute development session, not a persistent service installer.

The Studio UI attempts to use a single reader for remapping and Air observation. Its native implementation does not call Karabiner. If installed, leave **Modify events OFF for ReadyForAirRemote**; other devices may continue using it. The obsolete Karabiner exporter and dispatch bridge were removed; `instance.rs` retains only single-instance ownership.

## Ownership and output

- Actions OFF: shared HID observation; macOS retains original input handling.
- Actions ON: only the MR-01 (exact name, vendor and product ID) is opened exclusively using hidapi's macOS API. ACA Capture holds the exclusive HID handle as root. Studio retains its configuration, router, action execution and Quartz output as the ordinary user; it requires Accessibility. Both app identities require Input Monitoring.
- A pure Rust report router emits physical press transitions once, forwards supported unassigned input, suppresses mapped originals, and sends action identifiers to the coordinator. Movement travels through the same authenticated local channel and is forwarded by Studio through Quartz. Studio requests one foreground helper session automatically when the device and permissions are ready, using the macOS administrator authentication dialog without Terminal; no login service, kernel extension or virtual driver is installed.
- Motion, clicks, keyboard Enter/arrows and common media keys have native adapters. Unsupported original keyboard/consumer usages are inert and logged per button; they never end capture. Assigning an action to that source routes it normally.
- Press/release forwarding is remembered independently of the current mapping: edits cannot strand a key whose down was forwarded. Stop, errors and orderly shutdown release forwarded holds before closing the device. Malformed reports do not generate actions. Disconnect stops runtime dispatch; reconnect restores the requested global state once prerequisites are ready. Explicit user pause is preserved. Recoverable HID read errors release the handle and retry discovery while preserving the authenticated helper and its original deadline. Permission/protocol/output failures still stop actions with a visible reason; a broken IPC channel closes the helper.
- This adapter is not a byte-for-byte virtual HID device. Keyboard auto-repeat, media hold/repeat behavior and specialized applications consuming raw HID are not claimed equivalent to the system driver. Pointer sensitivity uses the received deltas. Abrupt process termination cannot run release cleanup.

## Profiles

Normal is the default at launch, reconnect and after `mouse_idle_ms` (default 1500 ms) without a mouse report. A valid mouse report activates Air, including a stationary heartbeat. Other keys leave it active. An arrow uses the profile active immediately before that press, then selects Normal. This does not reactivate the firmware sensor. Normal after silence is an application fallback, not a claim that the hardware switched off.

## Configuration and connection

Preset changes save and update the running native mapping without reinstalling rules. New scripts still run only on a physical mapped press or explicit Test selected action; loading/editing does not execute them. The global pause/resume applies to every button. Learning suspends dispatch and automatically resumes after observed buttons are released, unless the user explicitly paused. Authorization attempts are single-flight and are not repeated automatically after cancellation or session expiry.

The reader automatically detects an already-paired HID and retries unavailable devices. Discovery is offered when no connected device is found, completes a bounded BLE scan, and requires a selection if several named remotes appear. Multiple connected HIDs also require a choice rather than silently choosing the first. macOS still mediates Bluetooth pairing/security prompts. Multi-device discovery and a fresh unpaired remote need physical validation.

## API references

- [hidapi macOS implementation](https://github.com/libusb/hidapi/blob/master/mac/hid.c)
- [Apple CGEventPost](https://developer.apple.com/documentation/coregraphics/cgevent/post(tap:))
- [Apple mouse event creation](https://developer.apple.com/documentation/coregraphics/cgevent/init(mouseeventsource:mousetype:mousecursorposition:mousebutton:))

Passing router tests does not establish real hardware delivery or permissions. The root-private staging flow still needs fresh-machine TCC validation.

The [hidapi maintainer discussion](https://github.com/signal11/hidapi/issues/266) describes the keyboard-specific root restriction, consistent with this live error. The bounded local root probe succeeded. No persistent privileged service is installed.

## Capture boundary and validation launcher

`ACA Capture.app` is a small Rust executable. Studio embeds the packaged helper hash, rejects mismatches before requesting administrator authentication, then verifies and executes a root-private copy of the signed bundle. The old Terminal launcher was removed. The current foreground validation session is limited to 30 minutes. It can be restarted with Resume reader; it does not register a LaunchDaemon. Closing Studio closes the channel and releases the physical device. Pausing actions releases it while keeping the helper available for another activation.

The socket is owner-only (`0600`). Studio accepts only a root peer; the helper checks the socket owner/type/mode and the authenticated peer UID against `SUDO_UID`. Commands are limited to open a matching MR-01 path, heartbeat and close. Paths are resolved against fresh HID enumeration and exact name/VID/PID. No shell, configuration, scripts, output reports or feature reports are accepted by the privileged process. Frames are bounded to 4096 bytes; open requests to 512 bytes. Partial frames survive read timeouts. Write deadlines, a two-second heartbeat watchdog, EOF and invalid commands release capture. Synthetic held-key cleanup belongs to Studio; a forced Studio kill cannot run that cleanup.

This foreground helper is a development validation path, not a production privileged-service installer. The installed app is still ad-hoc signed; stable signing and a supported service registration/update flow are required before claiming a finished distribution experience.

Regression coverage lives alongside the implementation in `src/`.

## Review corrections (2026-10-13)

Capture protocol v2 reports its own Input Monitoring observation at handshake and each OPEN; the channel is not labeled as verified HID input. Only actual exclusive packets mark reading verified. Studio separately observes its own HID/Accessibility permissions. No read-only observation grants a permission.

The keyboard/mouse decoder is shared by routing, learning and mode observation. Forwarded clicks track system double-click timing plus button/distance; keyboard flags combine remote modifiers with the HID-system physical state (session-posted events are not read back). Native application behavior still requires a focused hardware check.

The foreground source bundle can remain user-writable; the executable hash expected by Studio is compiled in. The elevated command creates a private temporary directory, copies the bundle, rejects symlinks in its executable path, verifies the copied executable hash and bundle signature, and runs only that copy. This closes source substitution between preflight and execution; it is not a signed/notarized privileged-service installer. A forced OS/process shutdown can leave a temporary root-owned copy until system cleanup; normal exits remove it.
