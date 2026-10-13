# ACA — Air Controller Adapter

Customize your **Motorola Ready For Air Remote MR-01** with a native Mac app built in Rust and GPUI. Select a button on the 3D model, choose an action, and use it: mappings save automatically.

**An independent project, not affiliated with or endorsed by Motorola.** Device names identify compatibility.

## Download

[**Download the preview**](https://github.com/facundoPri/air-controller-adapter/releases/tag/v0.1.0-preview.2) · [All releases](https://github.com/facundoPri/air-controller-adapter/releases)

Under **Assets**, download `ACA-v0.1.0-preview.2-macos-arm64.zip`. The “Source code” archives contain source files, not the application.

This is an **experimental preview for Apple Silicon Macs (M1 or later)**. It does not require Rust or Karabiner. You need an MR-01 and an account able to authorize the reader as an administrator. Development was tested on macOS 26.6.2; the bundle declares macOS 13 as its minimum, but earlier versions have not been fully validated. There are no Intel, Windows, or Linux binaries.

The download is **ad hoc signed**, without an Apple Developer ID or notarization. Installation and permissions on a fresh Mac still need validation. Reader sessions last **up to 30 minutes** and can be renewed in the app; there is no permanent background service yet.

## Install and connect

1. Extract the ZIP. Move the entire **ACA** folder into Applications (or `~/Applications`). Keep **ACA Studio.app** and **ACA Capture.app** together; do not mix applications from different releases.
2. Open **ACA Studio**. Capture is its helper reader, not a second interface you need to launch manually.
3. If macOS blocks the development-signed app, check **System Settings → Privacy & Security → Open Anyway**, following [Apple's instructions](https://support.apple.com/102445). This authorizes the specific app; you do not need to disable Gatekeeper globally. If that option is unavailable, record the exact message when reporting the issue.
4. If the remote is already connected over Bluetooth, Studio detects it automatically. For a new remote, hold **Back + OK** until the LED flashes, then choose **Discover & connect** in Connection. Select your remote if several appear. Temporarily turn off your phone's Bluetooth if it is taking the connection.
5. Open **Connection → Set up this Mac** and complete the permissions below. Use the Settings and Finder links to add the exact apps from your installation folder.
6. Approve Studio's macOS administrator dialog to start Capture. No Terminal window opens. Check the reader status, then press a button to verify that activity arrives.

### Permissions

| Permission | Application | Purpose |
| --- | --- | --- |
| Input Monitoring | ACA Studio **and** ACA Capture | Observe and capture the compatible remote |
| Accessibility | ACA Studio | Forward pointer/keyboard input and execute shortcuts |
| Bluetooth | ACA Studio | Discover and connect the remote |
| Microphone, optional | ACA Studio | Record from your Mac or headset |
| Automation, when requested | ACA Studio | Run AppleScript actions targeting another app |

Grant permissions in **System Settings → Privacy & Security**. In Input Monitoring and Accessibility, use **+** to add an app if it is missing. If macOS asks you to quit and reopen, accept and open Studio again. Capture reports its own permission at startup; Studio distinguishes observed permission, an open channel, and received packets.

Capture uses administrator privileges only to read the MR-01. Actions and scripts run as your ordinary user. No driver, login service, or extension is installed. If you already use Karabiner, turn off **Modify events only for this remote** so the two apps do not compete for it.

## Customize buttons

1. In **Buttons**, select a button on the 3D model or in the list.
2. Select the **Normal** or **Air mouse** profile to edit.
3. Choose an action: **it saves and applies automatically** to the next physical press. You do not need to identify or enable each button.

**Pause remote / Resume remote** affects all mappings. For scripts or macros, open Advanced options, edit the JSON, and choose **Save action JSON**. Saving an action does not execute it. If saving fails, the app preserves the previous configuration and displays the error.

Presets include Mission Control, app windows, desktop, apps, volume, zoom, Page Up/Down, locking, sleep, and display sleep. Shortcuts, AppleScript, shell commands, and macros are also supported. **Zoom requires keyboard shortcuts to be enabled in Accessibility → Zoom**; ACA does not change that setting for you.

### Normal and Air mouse

Received movement selects Air mouse. An arrow uses the previous profile's mapping, then returns to Normal; the remote also turns off its sensor when an arrow is pressed. After 1.5 seconds without mouse reports, ACA assumes Normal. This is an inference, not a direct reading of firmware mode.

Changing the editor profile does not activate the sensor. The physical Air mouse button has no confirmed independent press event, so **it cannot be remapped**. The default Back mapping still needs an isolated physical check; firmware variants can use **Identify again** in Advanced options.

### Voice

Assign a button to start recording on one press and stop on the next. It uses the selected Mac or headset microphone; **MR-01 audio is not confirmed**. Text insertion requires a configured local transcriber: no model is bundled and audio is not uploaded automatically. [Voice setup guide](docs/voice-setup.md).

## Troubleshooting

| Symptom | What to check |
| --- | --- |
| Remote does not appear | Mac Bluetooth, batteries, Back + OK, and whether your phone is using it. Search again in Connection. |
| Packets appear but actions do not run | Global pause state, Studio Accessibility, active reader, and the assigned Normal/Air profile. Check Activity for the reason. |
| Session ended or you canceled authorization | Use **Resume reader**. Sessions last 30 minutes; canceled authorization is not retried automatically. |
| macOS says “not permitted” although the permission is enabled | Check that you authorized the apps from this installation. An ad hoc signed update may require removing and re-adding them, then reopening Studio. |
| Capture is missing or does not match | Extract the complete ZIP again. Studio verifies its reader's hash; mixing versions is not supported. |
| Zoom does not respond | Enable keyboard shortcuts in macOS Accessibility → Zoom. |
| Shortcuts or scripts fail | Check Activity, the focused application, and any Automation permissions requested by macOS. |

**Activity** lets you inspect and export events. When [reporting an issue](https://github.com/facundoPri/air-controller-adapter/issues), include your ACA/macOS version, Mac model, button, profile and action, steps, and error message. Review logs before attaching them: they may contain configuration or device data.

## Configuration, updates, and removal

Configuration and local data live in `~/Library/Application Support/ReadyForAirLab/`. This historical folder name is preserved to keep existing configurations. `config.previous.json` holds the previous configuration; recordings and transcriptions remain local too.

To update, quit Studio and replace **both apps together**, preserving the data folder. Do not run the previous version and ACA simultaneously. Internal identifiers retain the historical Studio/Capture names for continuity; ad hoc signing may still require renewing permissions.

To uninstall, quit Studio, delete the ACA folder, and remove its permission entries in Settings. To also remove configurations, recordings, and transcriptions, delete the data folder after backing up anything you need. There is no permanent service to uninstall.

- [JSON format and examples](docs/configuration.md)
- [Button map and limitations](docs/mr01-button-map.md)
- [Engine architecture](docs/native-engine.md)
- [Build, test, and package](docs/development.md)
- [Preview release notes](docs/releases/v0.1.0-preview.2.md)

The repository includes source code; an open-source license has not yet been selected for the project's own code. Dependencies retain their licenses, and their notices are included in the download. Third-party license texts and author names are preserved as supplied upstream.
