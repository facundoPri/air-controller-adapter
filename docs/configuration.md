# ACA Studio configuration

The UI and engine share `studio::Config`, versioned as `version: 1`. [config.example.json](config.example.json) is the actual default model output (`cargo run --example configuration`), with built-in MR-01 sources and original actions by default. Each button stores actions in `normal` and `air_mouse`.

Changes are saved to `~/Library/Application Support/ReadyForAirLab/config.json`. The historical directory is preserved for compatibility with existing data. Saving writes a temporary file first and preserves `config.previous.json`. Invalid JSON is not silently replaced: fix it and reload. Future schema versions, duplicate buttons, ambiguous sources, and deeply nested macros are rejected.

The MR-01 has a [built-in button map](mr01-button-map.md). Older configurations with empty sources are filled automatically, respecting signals you have already reassigned. Select a button in 3D and choose an action: it is saved and used on the next press, without identification or reconnection. Manual identification remains an advanced option for firmware variants. The physical mouse-mode button has no known independent press event and cannot be remapped; its movement packets select the Air mouse profile. The Back-to-F1 association still needs an isolated physical check.

## Actions

Each `normal` or `air_mouse` property accepts one of these objects:

```json
{"type":"default"}
{"type":"disabled"}
{"type":"shortcut","key":"c","modifiers":["command"]}
{"type":"system","command":"mission_control"}
{"type":"scroll","x":0,"y":-50}
{"type":"apple_script","script":"display notification \"Hello\" with title \"ACA\""}
{"type":"shell","command":"/usr/bin/say Hello"}
{"type":"macro","steps":[{"type":"shortcut","key":"c","modifiers":["command"]},{"type":"system","command":"mission_control"}]}
{"type":"voice_toggle"}
```

These are separate examples, not one JSON document. Shortcuts accept `command`, `control`, `option`, and `shift`; see `actions.rs` for supported keys. Scripts are user-selected code and run with your permissions. Editing, selecting, or saving them does not execute them.

System actions include `mission_control`, `app_expose`, `show_desktop`, `launchpad`, `volume_up`, `volume_down`, `mute`, `zoom_in`, `zoom_out`, `zoom_toggle`, `display_sleep`, `sleep`, and `lock_screen`. Zoom uses macOS Accessibility keyboard shortcuts; it does not enable that preference automatically. `launchpad` retains its JSON name but opens Apps.app on Tahoe and Launchpad.app on earlier versions. Show desktop and App windows use the system Mission Control launcher; arguments 1 and 2 were inspected on macOS 26.6.2 and need revalidation on newer versions.

Selecting an editor profile only changes the configuration being edited. Pointer activity is inferred from recent HID packets. Arrows mark the return to Normal; after `mouse_idle_ms` (1500 by default) without mouse packets, the app falls back to Normal. No verified command can reactivate the sensor from the Mac.

## Using mappings

When Studio opens, it detects a paired remote and enables actions automatically once permissions and capture are available. If administrator authentication is needed, Studio presents one macOS dialog; it does not open Terminal. Canceling does not automatically reopen the dialog.

1. Select a button on the 3D model and choose the Normal or Air mouse profile.
2. Choose an action. It saves and applies automatically; there is no per-button activation.
3. Press the physical button. Normal setup does not require learning. **Identify again** is an advanced option for firmware variants that use another signal.

**Pause remote / Resume remote** in the header affects all actions. For scripts/macros, use **Save action JSON** in Advanced options. Saving never executes the selected action and does not require an active reader. If writing fails or the file has changed externally, the previous configuration is preserved.

Capture sessions last up to 30 minutes and end when Studio closes. If a session expires or authorization is canceled, **Resume reader** renews it; authorization requests do not repeat automatically. There is no permanent service. Capture needs Input Monitoring; Studio needs Input Monitoring and Accessibility. Disable Modify events for the MR-01 in Karabiner to avoid competing for the device.

See [native engine and limitations](native-engine.md).

For voice, select a Mac or headset input and configure `transcription` using [voice-setup.md](voice-setup.md). One press starts recording and another stops it. Text is saved; insertion is attempted only if the same target application remains active and the mapping is still enabled.

## Drafts and execution limits

Each button/profile keeps its JSON draft while you navigate. An independent change, such as the microphone selection, does not replace edited JSON. If the base model changes, saving over it is rejected pending review; **Discard draft and reload** is explicit. Audio selection is persisted before changing the in-memory model.

A button press waits at most 500 ms before execution starts. A macro already in progress retains its 15-second budget. Pausing, editing configuration, or closing Studio cancels earlier work. When a shell finishes, its process group is terminated too; deliberately detached processes are outside the action contract. Transcription uses the same cleanup and cancellation lifecycle.
