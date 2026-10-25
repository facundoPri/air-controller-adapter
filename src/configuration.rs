//! Versioned loading. Legacy bytes are backed up before any replacement is committed.
use crate::studio::{Action, ButtonId, ButtonMapping, Config, InputSource, atomic_json_checked};
use serde::Deserialize;
use std::{io::Write, path::Path};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyConfig {
    version: u32,
    buttons: Vec<LegacyMapping>,
    audio_input: Option<String>,
    transcription: Option<LegacyTranscription>,
    mouse_idle_ms: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyTranscription {
    program: String,
    args: Vec<String>,
    insert_text: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyMapping {
    button: ButtonId,
    normal: LegacyAction,
    air_mouse: LegacyAction,
    normal_sources: Vec<InputSource>,
    mouse_sources: Vec<InputSource>,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum LegacyAction {
    Removed(RemovedAction),
    Current(Action),
}
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum RemovedAction {
    VoiceToggle {},
    Macro { steps: Vec<LegacyAction> },
}
impl LegacyAction {
    // Validate the full original structure with inert placeholders before removing
    // an assignment. An invalid sibling must not be hidden by a voice step.
    fn resolve(self) -> (Action, bool) {
        match self {
            Self::Removed(RemovedAction::VoiceToggle {}) => (Action::Disabled, true),
            Self::Removed(RemovedAction::Macro { steps }) => {
                let mut removed = false;
                let steps = steps
                    .into_iter()
                    .map(|step| {
                        let (action, contains_voice) = step.resolve();
                        removed |= contains_voice;
                        action
                    })
                    .collect();
                (Action::Macro { steps }, removed)
            }
            Self::Current(action) => (action, false),
        }
    }
    fn migrate(
        self,
        button: ButtonId,
        profile: &str,
        notices: &mut Vec<String>,
    ) -> Result<Action, String> {
        let (action, removed) = self.resolve();
        #[cfg(feature = "virtual-keyboard")]
        validate_old_action(&action)?;
        crate::studio::validate_action(&action, 0)?;
        if removed {
            notices.push(format!(
                "{} / {profile} → Disabled (removed voice action or macro containing voice)",
                button.label()
            ));
            Ok(Action::Disabled)
        } else {
            Ok(action)
        }
    }
}

#[cfg(feature = "virtual-keyboard")]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VersionTwo {
    version: u32,
    buttons: Vec<VersionTwoMapping>,
    mouse_idle_ms: u64,
}
#[cfg(feature = "virtual-keyboard")]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VersionTwoMapping {
    button: ButtonId,
    normal: Action,
    air_mouse: Action,
    normal_sources: Vec<InputSource>,
    mouse_sources: Vec<InputSource>,
}

#[cfg(feature = "virtual-keyboard")]
fn validate_old_action(action: &Action) -> Result<(), String> {
    match action {
        Action::Keyboard { .. } => Err("Keyboard actions require configuration version 3".into()),
        Action::Macro { steps } => {
            for step in steps {
                validate_old_action(step)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

#[cfg(feature = "virtual-keyboard")]
fn contains_legacy_keyboard(action: &Action) -> bool {
    match action {
        Action::System {
            command: crate::studio::SystemAction::OnScreenKeyboard,
        } => true,
        Action::Macro { steps } => steps.iter().any(contains_legacy_keyboard),
        _ => false,
    }
}

pub(crate) fn load(path: &Path, bytes: &[u8]) -> Result<(Config, Option<String>), String> {
    let header: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let version = header
        .get("version")
        .and_then(|v| v.as_u64())
        .ok_or("Unsupported or missing configuration version")?;
    if version == u64::from(Config::VERSION) {
        let config: Config = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        config.validate()?;
        return Ok((config, None));
    }
    let mut notices = vec![];
    let config = match version {
        1 => {
            let legacy: LegacyConfig = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
            if legacy.version != 1 {
                return Err("Unsupported legacy configuration version".into());
            }
            if let Some(t) = &legacy.transcription {
                if t.program.trim().is_empty() || !t.args.iter().any(|a| a.contains("{audio}")) {
                    return Err("Invalid legacy transcription configuration".into());
                }
                let _ = t.insert_text;
            }
            let _ = legacy.audio_input;
            notices.push("Recording and transcription settings were removed.".into());
            Config {
                version: Config::VERSION,
                mouse_idle_ms: legacy.mouse_idle_ms,
                #[cfg(feature = "virtual-keyboard")]
                keyboard: Default::default(),
                buttons: legacy
                    .buttons
                    .into_iter()
                    .map(|b| {
                        let normal = b.normal.migrate(b.button, "Normal", &mut notices)?;
                        let air_mouse = b.air_mouse.migrate(b.button, "Air mouse", &mut notices)?;
                        #[cfg(feature = "virtual-keyboard")]
                        {
                            validate_old_action(&normal)?;
                            validate_old_action(&air_mouse)?;
                        }
                        Ok(ButtonMapping {
                            button: b.button,
                            normal,
                            air_mouse,
                            #[cfg(feature = "virtual-keyboard")]
                            keyboard: crate::studio::default_keyboard_action(b.button),
                            normal_sources: b.normal_sources,
                            mouse_sources: b.mouse_sources,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?,
            }
        }
        #[cfg(feature = "virtual-keyboard")]
        2 => {
            let old: VersionTwo = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
            if old.version != 2 {
                return Err("Unsupported configuration version".into());
            }
            Config {
                version: Config::VERSION,
                mouse_idle_ms: old.mouse_idle_ms,
                keyboard: Default::default(),
                buttons: old
                    .buttons
                    .into_iter()
                    .map(|b| {
                        validate_old_action(&b.normal)?;
                        validate_old_action(&b.air_mouse)?;
                        Ok(ButtonMapping {
                            button: b.button,
                            normal: b.normal,
                            air_mouse: b.air_mouse,
                            keyboard: crate::studio::default_keyboard_action(b.button),
                            normal_sources: b.normal_sources,
                            mouse_sources: b.mouse_sources,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?,
            }
        }
        _ => return Err(format!("Unsupported configuration version: {version}")),
    };
    config.validate()?;
    #[cfg(feature = "virtual-keyboard")]
    if config
        .buttons
        .iter()
        .any(|b| contains_legacy_keyboard(&b.normal) || contains_legacy_keyboard(&b.air_mouse))
    {
        notices.push("Legacy On-screen keyboard actions were preserved. Conversion to ACA Toggle keyboard is deferred until native focus and ordered macro gates pass.".into());
    }
    let backup = path.with_file_name(format!("config.v{version}.backup.json"));
    backup_exact(&backup, bytes)?;
    if std::fs::read(path).map_err(|e| e.to_string())? != bytes {
        return Err("config.json changed during migration. Reload before trying again.".into());
    }
    atomic_json_checked(path, &config, Some(bytes))?;
    Ok((
        config,
        Some(format!(
            "Configuration migrated to version {}. Original saved to {}. {}",
            Config::VERSION,
            backup.display(),
            notices.join(" ")
        )),
    ))
}

fn backup_exact(backup: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(backup) {
        Ok(mut file) => {
            file.write_all(bytes)
                .map_err(|e| format!("Legacy backup failed: {e}"))?;
            file.sync_all()
                .map_err(|e| format!("Legacy backup failed: {e}"))?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            if std::fs::read(backup).map_err(|e| e.to_string())? != bytes {
                return Err("Legacy backup already exists with different contents; preserve it before migrating this configuration.".into());
            }
        }
        Err(e) => return Err(format!("Legacy backup failed: {e}")),
    }
    Ok(())
}

#[cfg(all(test, feature = "virtual-keyboard"))]
mod tests {
    use super::*;
    use crate::studio::Profile;

    fn version_two() -> serde_json::Value {
        let mut value = serde_json::to_value(Config::default()).unwrap();
        value["version"] = 2.into();
        value.as_object_mut().unwrap().remove("keyboard");
        for b in value["buttons"].as_array_mut().unwrap() {
            b.as_object_mut().unwrap().remove("keyboard");
        }
        value
    }
    fn fixture(bytes: &[u8]) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("keyboard-migration-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("config.json");
        std::fs::write(&path, bytes).unwrap();
        path
    }
    #[test]
    fn v2_migration_preserves_actions_sources_and_nested_legacy_keyboard() {
        let mut original = version_two();
        original["buttons"][0]["normal"] = serde_json::json!({"type":"macro","steps":[
            {"type":"shortcut","key":"c","modifiers":["command"]},
            {"type":"macro","steps":[{"type":"system","command":"on_screen_keyboard"}]},
            {"type":"shell","command":"printf preserved"}
        ]});
        original["buttons"][0]["air_mouse"] =
            serde_json::json!({"type":"apple_script","script":"return 4"});
        original["buttons"][0]["normal_sources"] = serde_json::json!([]);
        original["buttons"][0]["mouse_sources"] = serde_json::json!([{"Consumer":200}]);
        original["mouse_idle_ms"] = 7234.into();
        let bytes = serde_json::to_vec_pretty(&original).unwrap();
        let path = fixture(&bytes);
        let (config, notice) = load(&path, &bytes).unwrap();
        assert_eq!(config.version, 3);
        assert_eq!(config.mouse_idle_ms, 7234);
        assert!(notice.unwrap().contains("deferred"));
        let mut migrated = serde_json::to_value(&config).unwrap();
        migrated["version"] = 2.into();
        migrated.as_object_mut().unwrap().remove("keyboard");
        for b in migrated["buttons"].as_array_mut().unwrap() {
            b.as_object_mut().unwrap().remove("keyboard");
        }
        assert_eq!(original, migrated);
        assert_eq!(
            std::fs::read(path.with_file_name("config.v2.backup.json")).unwrap(),
            bytes
        );
        assert!(!path.with_file_name("config.v1.backup.json").exists());
        let final_bytes = std::fs::read(&path).unwrap();
        let (_, notice) = load(&path, &final_bytes).unwrap();
        assert!(notice.is_none());
        assert_eq!(std::fs::read(&path).unwrap(), final_bytes);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
    #[test]
    fn rejection_never_rewrites_or_creates_a_backup() {
        let mut cases = vec![];
        let mut unknown = version_two();
        unknown["keyboard"] = serde_json::json!({});
        cases.push(unknown);
        let mut new_action = version_two();
        new_action["buttons"][0]["normal"] =
            serde_json::json!({"type":"macro","steps":[{"type":"keyboard","command":"toggle"}]});
        cases.push(new_action);
        let mut alias = version_two();
        alias["buttons"][0]["mouse_sources"] = serde_json::json!([{"Keyboard":82}]);
        cases.push(alias);
        let mut fallback = version_two();
        fallback["buttons"][0]["normal_sources"] = serde_json::json!([{"Consumer":200}]);
        fallback["buttons"][1]["mouse_sources"] = serde_json::json!([{"Consumer":200}]);
        cases.push(fallback);
        let mut future = version_two();
        future["version"] = 4.into();
        cases.push(future);
        let mut v1: serde_json::Value =
            serde_json::from_slice(include_bytes!("../tests/fixtures/config-v1.json")).unwrap();
        v1["buttons"][0]["normal"] = serde_json::json!({"type":"macro","steps":[{"type":"voice_toggle"},{"type":"keyboard","command":"toggle"}]});
        cases.push(v1);
        for value in cases {
            let bytes = serde_json::to_vec(&value).unwrap();
            let path = fixture(&bytes);
            assert!(load(&path, &bytes).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
            for version in [1, 2] {
                assert!(
                    !path
                        .with_file_name(format!("config.v{version}.backup.json"))
                        .exists()
                );
            }
            assert!(!path.with_file_name("config.previous.json").exists());
            std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
        }
    }
    #[test]
    fn backup_conflict_external_change_and_commit_failure_preserve_files() {
        let bytes = serde_json::to_vec(&version_two()).unwrap();
        for scenario in 0..3 {
            let path = fixture(&bytes);
            let backup = path.with_file_name("config.v2.backup.json");
            match scenario {
                0 => std::fs::write(&backup, b"different backup").unwrap(),
                1 => std::fs::write(&path, b"external edit").unwrap(),
                _ => std::fs::create_dir(path.with_file_name("config.previous.json")).unwrap(),
            }
            assert!(load(&path, &bytes).is_err());
            assert_eq!(
                std::fs::read(&path).unwrap(),
                if scenario == 1 {
                    b"external edit".as_slice()
                } else {
                    bytes.as_slice()
                }
            );
            assert_eq!(
                std::fs::read(&backup).unwrap(),
                if scenario == 0 {
                    b"different backup".as_slice()
                } else {
                    bytes.as_slice()
                }
            );
            std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
        }
    }
    #[test]
    fn v3_rejects_invalid_preferences_and_nested_keyboard_assignments_without_writes() {
        let original = serde_json::to_value(Config::default()).unwrap();
        for (pointer, bad) in [
            ("/keyboard/scale", serde_json::json!(0.6)),
            ("/keyboard/opacity", serde_json::json!(1.1)),
            ("/keyboard/position", serde_json::json!({"x":100001,"y":0})),
            ("/keyboard/layout", serde_json::json!("invented")),
            (
                "/buttons/0/keyboard",
                serde_json::json!({"type":"macro","steps":[{"type":"shell","command":""}]}),
            ),
            (
                "/buttons/0/keyboard",
                serde_json::json!({"type":"keyboard","command":"toggle","unexpected":true}),
            ),
        ] {
            let mut value = original.clone();
            *value.pointer_mut(pointer).unwrap() = bad;
            let bytes = serde_json::to_vec(&value).unwrap();
            let path = fixture(&bytes);
            assert!(load(&path, &bytes).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
            assert_eq!(
                std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
                1
            );
            std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
        }
    }
    #[test]
    fn migration_commit_rechecks_after_preparing_candidate_bytes() {
        struct ExternalEdit<'a>(&'a Path);
        impl serde::Serialize for ExternalEdit<'_> {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                std::fs::write(self.0, b"external edit during preparation").unwrap();
                serializer.serialize_str("candidate")
            }
        }
        let original = b"original";
        let path = fixture(original);
        assert!(atomic_json_checked(&path, &ExternalEdit(&path), Some(original)).is_err());
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"external edit during preparation"
        );
        assert!(!path.with_file_name("config.previous.json").exists());
        assert_eq!(
            std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
            1
        );
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
    #[test]
    fn three_layers_are_independent_and_keyboard_sources_use_normal_fallback() {
        let mut config = Config::default();
        config.keyboard = crate::keyboard_model::KeyboardPreferences {
            layout: crate::keyboard_model::KeyboardLayout::Alphabetical,
            language: crate::keyboard_model::KeyboardLanguage::Spanish,
            scale: 0.7,
            opacity: 0.4,
            position: Some(crate::keyboard_model::KeyboardPosition {
                x: -100_000.0,
                y: 100_000.0,
            }),
        };
        let home = &mut config.buttons[0];
        *home.action_mut(Profile::Keyboard) = Action::Keyboard {
            command: crate::keyboard_model::KeyboardAction::Toggle,
        };
        assert_eq!(home.action(Profile::Normal), &Action::Default);
        assert_eq!(home.action(Profile::AirMouse), &Action::Default);
        assert_eq!(
            crate::studio::sources(home, Profile::Keyboard),
            home.normal_sources
        );
        config.validate().unwrap();
        let encoded = serde_json::to_vec(&config).unwrap();
        let decoded: Config = serde_json::from_slice(&encoded).unwrap();
        decoded.validate().unwrap();
        assert_eq!(decoded.keyboard, config.keyboard);
        assert!(matches!(
            decoded.buttons[0].keyboard,
            Action::Keyboard {
                command: crate::keyboard_model::KeyboardAction::Toggle
            }
        ));
        assert_eq!(decoded.buttons[12].keyboard, Action::Default);
    }
}
