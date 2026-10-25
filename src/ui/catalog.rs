//! Presentation groups for the core's typed action presets. The actions
//! themselves come from `Action::presets()`; this module only decides where
//! each one is shown, so the UI never duplicates commands or key strings.

use ready_for_air::studio::{Action, SystemAction};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    General,
    Shortcuts,
    Navigation,
    Zoom,
    Windows,
    Sound,
    Power,
}

impl Group {
    pub const ALL: [Self; 7] = [
        Self::General,
        Self::Shortcuts,
        Self::Navigation,
        Self::Zoom,
        Self::Windows,
        Self::Sound,
        Self::Power,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::General => "GENERAL",
            Self::Shortcuts => "SHORTCUTS",
            Self::Navigation => "NAVIGATION",
            Self::Zoom => "ZOOM",
            Self::Windows => "MACOS WINDOWS & APPS",
            Self::Sound => "SOUND",
            Self::Power => "POWER & SECURITY",
        }
    }
}

/// Group for a preset action; `None` for definitions only the configuration
/// JSON can express (scripts, macros, scrolls and other shortcuts).
pub fn group_of(action: &Action) -> Option<Group> {
    use SystemAction::*;
    Some(match action {
        #[cfg(feature = "virtual-keyboard")]
        Action::Keyboard { .. } => Group::Navigation,
        Action::Default | Action::Disabled => Group::General,
        Action::Shortcut { key, modifiers } if modifiers.is_empty() => match key.as_str() {
            "page_up" | "page_down" => Group::Navigation,
            _ => return None,
        },
        Action::Shortcut { .. } => Group::Shortcuts,
        Action::System { command } => match command {
            MissionControl | AppExpose | ShowDesktop | Launchpad | OnScreenKeyboard => {
                Group::Windows
            }
            VolumeUp | VolumeDown | Mute => Group::Sound,
            ZoomIn | ZoomOut | ZoomToggle => Group::Zoom,
            DisplaySleep | Sleep | LockScreen => Group::Power,
        },
        Action::Scroll { .. }
        | Action::AppleScript { .. }
        | Action::Shell { .. }
        | Action::Macro { .. } => {
            return None;
        }
    })
}

/// Presets in catalog order, grouped. Groups without presets are omitted.
pub fn grouped() -> Vec<(Group, Vec<Action>)> {
    let presets: Vec<Action> = Action::presets().into_iter().map(|(_, a)| a).collect();
    Group::ALL
        .into_iter()
        .map(|g| {
            (
                g,
                presets
                    .iter()
                    .filter(|a| group_of(a) == Some(g))
                    .cloned()
                    .collect::<Vec<_>>(),
            )
        })
        .filter(|(_, actions)| !actions.is_empty())
        .collect()
}

/// True when `action` is one of the presets, so its chip shows as selected.
/// Anything else is a custom definition kept from the configuration JSON.
pub fn is_preset(action: &Action) -> bool {
    Action::presets().iter().any(|(_, a)| a == action)
}

#[cfg(test)]
mod tests {
    use super::*;

    // gpui_kit's glob import shadows the std `test` attribute.
    #[::core::prelude::v1::test]
    fn every_preset_is_offered_exactly_once_in_catalog_order() {
        let shown: Vec<Action> = grouped().into_iter().flat_map(|(_, a)| a).collect();
        let presets: Vec<Action> = Action::presets().into_iter().map(|(_, a)| a).collect();
        assert_eq!(shown.len(), presets.len());
        for preset in &presets {
            assert_eq!(
                shown.iter().filter(|a| *a == preset).count(),
                1,
                "{preset:?}"
            );
        }
        let groups: Vec<Group> = grouped().into_iter().map(|(g, _)| g).collect();
        assert_eq!(groups, Group::ALL, "a required group has no presets");
    }

    #[::core::prelude::v1::test]
    fn custom_definitions_stay_custom() {
        for custom in [
            Action::Shell {
                command: "true".into(),
            },
            Action::AppleScript {
                script: "beep".into(),
            },
            Action::Macro {
                steps: vec![Action::Disabled],
            },
            Action::Scroll { x: 0, y: 3 },
            Action::Shortcut {
                key: "f".into(),
                modifiers: vec!["control".into(), "command".into()],
            },
        ] {
            assert!(!is_preset(&custom), "{custom:?}");
        }
        assert!(is_preset(&Action::Default));
    }
}
