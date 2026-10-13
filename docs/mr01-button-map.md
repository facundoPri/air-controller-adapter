# MR-01 built-in mapping

The device-scoped reader now supplies a button map for fresh and legacy configurations. Selecting a preset saves immediately and does not launch learning. Explicit learned mappings take precedence: missing sources are filled only if not claimed by another button in either profile. Existing actions are preserved. Reload fills the map in memory; the next save persists it through the existing conflict-checked atomic writer.

| Button | Input source | Evidence |
| --- | --- | --- |
| Home | Consumer 006A | User-isolated Home trial, `resilience-home-trial.json` |
| Apps (three lines) | Consumer 006B | User three-lines trial and current Activity; prior capture survives this usage |
| Experiences | Consumer 008D | Physical Show Desktop trial |
| Capture | Consumer 006C | Physical Mission Control trial |
| Up / Down / Left / Right | Keyboard 52 / 51 / 50 / 4F | Reports in mixed capture; standard HID arrow semantics |
| OK | Keyboard 28; Mouse 1 | Enter in mixed capture and primary mouse-button report; both routed to OK |
| Mute | Keyboard 7F; Consumer E2 | Keyboard 7F observed, HID Mute semantics; standard consumer alternative |
| Volume + / − | Consumer E9 / EA | Observed reports and HID volume semantics |
| Back | Keyboard F1 | F1 observed in mixed capture; assignment to Back is an MR-01 working inference and still needs an isolated physical check |
| Air mouse | None | Firmware switch has no independently observed press report |

The mixed capture does not independently prove every physical label. Standard usages follow [USB HID Usage Tables](https://www.usb.org/sites/default/files/hut1_21_0.pdf). Back is explicitly provisional, not presented as a measured physical correlation. Manual re-identification remains an advanced recovery option for firmware variants. No source is learned from an arbitrary next packet during ordinary action editing.

Regression: a legacy config with empty source lists + choosing Apps → Show Desktop must route `03 6B 00` immediately, with actions remaining active and no learning. The test failed before the change. A second regression covers all built-in source routes (synthetic), custom-source precedence, and the unmappable firmware switch.

Physical validation in development confirmed that two Apps presses show the desktop and restore the windows without manual learning or reconnecting. This does not certify installation on a fresh Mac.
