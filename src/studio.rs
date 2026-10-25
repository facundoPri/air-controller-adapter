//! Device-independent configuration and a bounded coordinator for the desktop app.
use crate::{
    hardware::{self, Event},
    lifecycle::{
        ConnectionCommand, ConnectionStatus, DeviceIdentity, DisconnectCapability, Readiness,
        SetupAction, TransportEvidence,
    },
    presses::Presses,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ButtonId {
    Home,
    Experiences,
    Apps,
    Back,
    Up,
    Down,
    Left,
    Right,
    Ok,
    Capture,
    MouseMode,
    VolumeDown,
    VolumeUp,
    Mute,
}
impl ButtonId {
    pub const ALL: [Self; 14] = [
        Self::Home,
        Self::Experiences,
        Self::Apps,
        Self::Back,
        Self::Up,
        Self::Down,
        Self::Left,
        Self::Right,
        Self::Ok,
        Self::Capture,
        Self::MouseMode,
        Self::VolumeDown,
        Self::VolumeUp,
        Self::Mute,
    ];
    pub fn id(self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Experiences => "experiences",
            Self::Apps => "apps",
            Self::Back => "back",
            Self::Up => "up",
            Self::Down => "down",
            Self::Left => "left",
            Self::Right => "right",
            Self::Ok => "ok",
            Self::Capture => "capture",
            Self::MouseMode => "mouse_mode",
            Self::VolumeDown => "volume_down",
            Self::VolumeUp => "volume_up",
            Self::Mute => "mute",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Home => "Home",
            Self::Experiences => "Experiences",
            Self::Apps => "Apps",
            Self::Back => "Back",
            Self::Up => "Up",
            Self::Down => "Down",
            Self::Left => "Left",
            Self::Right => "Right",
            Self::Ok => "OK",
            Self::Capture => "Capture",
            Self::MouseMode => "Air mouse",
            Self::VolumeDown => "Volume −",
            Self::VolumeUp => "Volume +",
            Self::Mute => "Mute",
        }
    }
    pub fn is_arrow(self) -> bool {
        matches!(self, Self::Up | Self::Down | Self::Left | Self::Right)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Profile {
    Normal,
    AirMouse,
    #[cfg(feature = "virtual-keyboard")]
    Keyboard,
}
impl Profile {
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::AirMouse => "Air mouse",
            #[cfg(feature = "virtual-keyboard")]
            Self::Keyboard => "Keyboard",
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::AirMouse => "air_mouse",
            #[cfg(feature = "virtual-keyboard")]
            Self::Keyboard => "keyboard",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseMode {
    Normal,
    Active,
}
impl MouseMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Active => "Air mouse active",
        }
    }
}

/// Mouse reports confirm activity; keyboard arrows mark the firmware's exit.
/// Other keys and empty/malformed packets never manufacture a mode transition.
fn mode_evidence(bytes: &[u8]) -> Option<MouseMode> {
    match crate::report::decode(bytes) {
        Some(crate::report::Report::Mouse { .. }) => Some(MouseMode::Active),
        Some(crate::report::Report::Keyboard { keys, .. })
            if keys.iter().any(|k| (0x4f..=0x52).contains(k)) =>
        {
            Some(MouseMode::Normal)
        }
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum InputSource {
    Keyboard(u8),
    Consumer(u16),
    Mouse(u8),
}
impl std::fmt::Display for InputSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Keyboard(n) => write!(f, "HID key {n:02X}"),
            Self::Consumer(n) => write!(f, "Consumer {n:04X}"),
            Self::Mouse(n) => write!(f, "Mouse button {n}"),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum Action {
    Default,
    Disabled,
    Shortcut {
        key: String,
        modifiers: Vec<String>,
    },
    System {
        command: SystemAction,
    },
    #[cfg(feature = "virtual-keyboard")]
    Keyboard {
        command: crate::keyboard_model::KeyboardAction,
    },
    Scroll {
        x: i32,
        y: i32,
    },
    AppleScript {
        script: String,
    },
    Shell {
        command: String,
    },
    Macro {
        steps: Vec<Action>,
    },
}
// Empty struct variants reject unexpected fields; serde's internally tagged
// unit variants otherwise accept and silently discard them.
impl<'de> Deserialize<'de> for Action {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "snake_case", tag = "type", deny_unknown_fields)]
        enum Definition {
            Default {},
            Disabled {},
            Shortcut {
                key: String,
                modifiers: Vec<String>,
            },
            System {
                command: SystemAction,
            },
            #[cfg(feature = "virtual-keyboard")]
            Keyboard {
                command: crate::keyboard_model::KeyboardAction,
            },
            Scroll {
                x: i32,
                y: i32,
            },
            AppleScript {
                script: String,
            },
            Shell {
                command: String,
            },
            Macro {
                steps: Vec<Action>,
            },
        }
        Ok(match Definition::deserialize(deserializer)? {
            Definition::Default {} => Self::Default,
            Definition::Disabled {} => Self::Disabled,
            Definition::Shortcut { key, modifiers } => Self::Shortcut { key, modifiers },
            Definition::System { command } => Self::System { command },
            #[cfg(feature = "virtual-keyboard")]
            Definition::Keyboard { command } => Self::Keyboard { command },
            Definition::Scroll { x, y } => Self::Scroll { x, y },
            Definition::AppleScript { script } => Self::AppleScript { script },
            Definition::Shell { command } => Self::Shell { command },
            Definition::Macro { steps } => Self::Macro { steps },
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemAction {
    MissionControl,
    AppExpose,
    ShowDesktop,
    Launchpad,
    OnScreenKeyboard,
    VolumeUp,
    VolumeDown,
    Mute,
    ZoomIn,
    ZoomOut,
    ZoomToggle,
    DisplaySleep,
    Sleep,
    LockScreen,
}
impl SystemAction {
    pub fn label(self) -> &'static str {
        match self {
            Self::MissionControl => "Mission Control",
            Self::AppExpose => "App windows",
            Self::ShowDesktop => "Show desktop",
            Self::Launchpad => "Apps",
            Self::OnScreenKeyboard => "On-screen keyboard",
            Self::VolumeUp => "Volume up",
            Self::VolumeDown => "Volume down",
            Self::Mute => "Mute",
            Self::ZoomIn => "Zoom in",
            Self::ZoomOut => "Zoom out",
            Self::ZoomToggle => "Toggle screen zoom",
            Self::DisplaySleep => "Display sleep",
            Self::Sleep => "Sleep Mac",
            Self::LockScreen => "Lock screen",
        }
    }
}
impl Action {
    pub fn label(&self) -> String {
        match self {
            Self::Default => "Original function".into(),
            Self::Disabled => "Disabled".into(),
            Self::Shortcut { key, modifiers } => format!("{} {key}", modifiers.join(" + ")),
            Self::System { command } => command.label().into(),
            #[cfg(feature = "virtual-keyboard")]
            Self::Keyboard { command } => command.label().into(),
            Self::Scroll { x, y } => format!("Scroll {x}, {y}"),
            Self::AppleScript { .. } => "AppleScript".into(),
            Self::Shell { .. } => "Custom command".into(),
            Self::Macro { steps } => format!("Macro · {} steps", steps.len()),
        }
    }
    pub fn presets() -> Vec<(String, Self)> {
        let mut a = vec![Self::Default, Self::Disabled];
        for command in [
            SystemAction::MissionControl,
            SystemAction::AppExpose,
            SystemAction::ShowDesktop,
            SystemAction::Launchpad,
            SystemAction::OnScreenKeyboard,
            SystemAction::VolumeUp,
            SystemAction::VolumeDown,
            SystemAction::Mute,
            SystemAction::ZoomIn,
            SystemAction::ZoomOut,
            SystemAction::ZoomToggle,
            SystemAction::DisplaySleep,
            SystemAction::Sleep,
            SystemAction::LockScreen,
        ] {
            a.push(Self::System { command });
        }
        a.extend([
            Self::Shortcut {
                key: "page_up".into(),
                modifiers: vec![],
            },
            Self::Shortcut {
                key: "page_down".into(),
                modifiers: vec![],
            },
            Self::Shortcut {
                key: "c".into(),
                modifiers: vec!["command".into()],
            },
            Self::Shortcut {
                key: "v".into(),
                modifiers: vec!["command".into()],
            },
        ]);
        #[cfg(feature = "virtual-keyboard")]
        for command in [
            crate::keyboard_model::KeyboardAction::Toggle,
            crate::keyboard_model::KeyboardAction::Up,
            crate::keyboard_model::KeyboardAction::Down,
            crate::keyboard_model::KeyboardAction::Left,
            crate::keyboard_model::KeyboardAction::Right,
            crate::keyboard_model::KeyboardAction::Select,
            crate::keyboard_model::KeyboardAction::Delete,
        ] {
            a.push(Self::Keyboard { command });
        }
        a.into_iter().map(|a| (a.label(), a)).collect()
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ButtonMapping {
    pub button: ButtonId,
    pub normal: Action,
    pub air_mouse: Action,
    #[cfg(feature = "virtual-keyboard")]
    pub keyboard: Action,
    pub normal_sources: Vec<InputSource>,
    pub mouse_sources: Vec<InputSource>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    pub buttons: Vec<ButtonMapping>,
    pub mouse_idle_ms: u64,
    #[cfg(feature = "virtual-keyboard")]
    pub keyboard: crate::keyboard_model::KeyboardPreferences,
}
// MR-01 device map. This app's capture is restricted to VID/PID/name MR-01.
// Back's Keyboard F1 association remains provisional pending an isolated physical check.
fn mr01_sources(button: ButtonId) -> Vec<InputSource> {
    use InputSource::{Consumer as C, Keyboard as K, Mouse as M};
    match button {
        ButtonId::Home => vec![C(0x6a)],
        ButtonId::Experiences => vec![C(0x8d)],
        ButtonId::Apps => vec![C(0x6b)],
        ButtonId::Capture => vec![C(0x6c)],
        ButtonId::Back => vec![K(0xf1)],
        ButtonId::Up => vec![K(0x52)],
        ButtonId::Down => vec![K(0x51)],
        ButtonId::Left => vec![K(0x50)],
        ButtonId::Right => vec![K(0x4f)],
        ButtonId::Ok => vec![K(0x28), M(1)],
        ButtonId::Mute => vec![K(0x7f), C(0xe2)],
        ButtonId::VolumeUp => vec![C(0xe9)],
        ButtonId::VolumeDown => vec![C(0xea)],
        ButtonId::MouseMode => vec![], // Firmware switch, no independent report.
    }
}
impl Default for Config {
    fn default() -> Self {
        Self {
            version: Self::VERSION,
            buttons: ButtonId::ALL
                .into_iter()
                .map(|button| ButtonMapping {
                    button,
                    normal: Action::Default,
                    air_mouse: Action::Default,
                    #[cfg(feature = "virtual-keyboard")]
                    keyboard: default_keyboard_action(button),
                    normal_sources: mr01_sources(button),
                    mouse_sources: vec![],
                })
                .collect(),
            mouse_idle_ms: 1500,
            #[cfg(feature = "virtual-keyboard")]
            keyboard: Default::default(),
        }
    }
}
#[cfg(feature = "virtual-keyboard")]
pub(crate) fn default_keyboard_action(button: ButtonId) -> Action {
    use crate::keyboard_model::KeyboardAction;
    let command = match button {
        ButtonId::Up => KeyboardAction::Up,
        ButtonId::Down => KeyboardAction::Down,
        ButtonId::Left => KeyboardAction::Left,
        ButtonId::Right => KeyboardAction::Right,
        ButtonId::Ok => KeyboardAction::Select,
        ButtonId::Back => KeyboardAction::Delete,
        ButtonId::VolumeUp | ButtonId::VolumeDown | ButtonId::Mute => return Action::Default,
        _ => return Action::Disabled,
    };
    Action::Keyboard { command }
}
impl ButtonMapping {
    pub fn action(&self, profile: Profile) -> &Action {
        match profile {
            Profile::Normal => &self.normal,
            Profile::AirMouse => &self.air_mouse,
            #[cfg(feature = "virtual-keyboard")]
            Profile::Keyboard => &self.keyboard,
        }
    }
    pub fn action_mut(&mut self, profile: Profile) -> &mut Action {
        match profile {
            Profile::Normal => &mut self.normal,
            Profile::AirMouse => &mut self.air_mouse,
            #[cfg(feature = "virtual-keyboard")]
            Profile::Keyboard => &mut self.keyboard,
        }
    }
}
impl Config {
    pub const VERSION: u32 = if cfg!(feature = "virtual-keyboard") {
        3
    } else {
        2
    };
    /// Explicit assignment may identify its button using the built-in map.
    /// Never change other buttons or borrow a source owned in either profile.
    fn fill_mr01_sources(&mut self, button: ButtonId) {
        let mut claimed: BTreeSet<_> = self
            .buttons
            .iter()
            .flat_map(|b| b.normal_sources.iter().chain(&b.mouse_sources))
            .cloned()
            .collect();
        for b in &mut self.buttons {
            if b.button == button && b.normal_sources.is_empty() {
                b.normal_sources = mr01_sources(b.button)
                    .into_iter()
                    .filter(|s| claimed.insert(s.clone()))
                    .collect();
            }
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version != Self::VERSION {
            return Err(format!(
                "Unsupported configuration version: {}",
                self.version
            ));
        }
        #[cfg(feature = "virtual-keyboard")]
        self.keyboard.validate()?;
        if !(500..=30000).contains(&self.mouse_idle_ms) {
            return Err("mouse_idle_ms must be between 500 and 30000".into());
        }
        if self.buttons.len() != 14
            || self
                .buttons
                .iter()
                .map(|b| b.button)
                .collect::<BTreeSet<_>>()
                .len()
                != 14
        {
            return Err(
                "Configuration must contain exactly all 14 buttons, without duplicates".into(),
            );
        }
        for profile in [Profile::Normal, Profile::AirMouse] {
            let mut seen = std::collections::BTreeMap::new();
            for b in &self.buttons {
                validate_action(b.action(profile), 0)?;
                for s in sources(b, profile) {
                    let valid = match s {
                        InputSource::Keyboard(n) => (4..=0xe7).contains(n) || *n == 0xf1,
                        InputSource::Consumer(n) => (1..=0x3ff).contains(n),
                        InputSource::Mouse(n) => (1..=5).contains(n),
                    };
                    if !valid {
                        return Err(format!("Invalid input in {}: {s}", profile.label()));
                    }
                    if let Some(owner) = seen.insert(s, b.button) {
                        return Err(format!(
                            "Duplicate input {s} in {} for {} and {}. Remove or relearn the conflicting source alias.",
                            profile.label(),
                            owner.label(),
                            b.button.label()
                        ));
                    }
                }
            }
        }
        #[cfg(feature = "virtual-keyboard")]
        {
            let mut ownership = std::collections::BTreeMap::new();
            for b in &self.buttons {
                validate_action(&b.keyboard, 0)?;
                for source in sources(b, Profile::Normal)
                    .iter()
                    .chain(sources(b, Profile::AirMouse))
                {
                    if let Some(owner) = ownership.insert(source, b.button) {
                        if owner != b.button {
                            return Err(format!(
                                "Input {source} identifies both {} and {} across Normal / Air mouse. Remove or relearn the conflicting source alias.",
                                owner.label(),
                                b.button.label()
                            ));
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
/// Hardware identity has only Normal and Air mouse maps. Resolve a physical
/// profile before selecting an action layer. Keyboard uses the Normal fallback.
pub fn sources(b: &ButtonMapping, p: Profile) -> &[InputSource] {
    if p == Profile::AirMouse && !b.mouse_sources.is_empty() {
        &b.mouse_sources
    } else {
        &b.normal_sources
    }
}
pub(crate) fn validate_action(a: &Action, depth: usize) -> Result<(), String> {
    if depth == 0 {
        crate::actions::validate_definition(a)?;
    }
    if depth > 4 {
        return Err("Macro nesting is too deep (maximum 4 levels)".into());
    }
    match a {
        Action::Macro { steps } => {
            if steps.is_empty() || steps.len() > 32 {
                return Err("A macro supports 1–32 steps".into());
            }
            for s in steps {
                validate_action(s, depth + 1)?;
            }
        }
        Action::Shortcut { key, modifiers } => {
            if key.is_empty() || key.len() > 64 || modifiers.len() > 4 {
                return Err("Invalid shortcut".into());
            }
        }
        Action::Shell { command } | Action::AppleScript { script: command } => {
            if command.trim().is_empty() || command.len() > 16384 {
                return Err("Script must contain between 1 and 16384 bytes".into());
            }
        }
        Action::Scroll { x, y } if x.unsigned_abs() > 100 || y.unsigned_abs() > 100 => {
            return Err("Scroll out of range (±100)".into());
        }
        _ => {}
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    pub time: String,
    pub category: String,
    pub message: String,
}
fn stamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}
pub(crate) fn atomic_json(path: &std::path::Path, value: &impl Serialize) -> Result<(), String> {
    atomic_json_checked(path, value, None)
}
pub(crate) fn atomic_json_checked(
    path: &std::path::Path,
    value: &impl Serialize,
    expected: Option<&[u8]>,
) -> Result<(), String> {
    use std::io::Write;
    let unchanged = || -> Result<(), String> {
        if let Some(original) = expected {
            if std::fs::read(path).map_err(|e| e.to_string())? != original {
                return Err(
                    "config.json changed during migration. Reload before trying again.".into(),
                );
            }
        }
        Ok(())
    };
    let parent = path.parent().ok_or("Path has no parent directory")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    let temp = parent.join(format!(".ready-air-{}-{}.tmp", std::process::id(), stamp()));
    let result = (|| {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&temp).map_err(|e| e.to_string())?;
        f.write_all(&bytes).map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())?;
        unchanged()?;
        if path.exists() {
            std::fs::copy(path, path.with_extension("previous.json")).map_err(|e| e.to_string())?;
        }
        unchanged()?;
        std::fs::rename(&temp, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result
}

struct QueuedAction {
    action: Action,
    epoch: u64,
    at: Instant,
    #[cfg(feature = "virtual-keyboard")]
    button: ButtonId,
    #[cfg(feature = "virtual-keyboard")]
    keyboard_generation: Option<u64>,
}
pub struct Studio {
    pub config: Config,
    pub logs: Vec<LogEntry>,
    pub connected: bool,
    pub connecting: bool,
    pub listening: bool,
    pub native_active: bool,
    pub capture_service: bool,
    pub capture_access: crate::permissions::Access,
    pub capture_verified: bool,
    pub bluetooth_state: String,
    pub candidates: Vec<(String, String)>,
    reader_running: bool,
    direct_mapping: hardware::Mapping,
    #[cfg(feature = "virtual-keyboard")]
    pub keyboard_control: crate::keyboard::Control,
    device_choice: hardware::DeviceChoice,
    reader_thread: Option<std::thread::JoinHandle<()>>,
    pub packet_count: u64,
    pub last_packet: String,
    pub mode: MouseMode,
    pub learning: Option<(ButtonId, Profile)>,
    pub learn_result: Option<Result<String, String>>,
    pub last_button: Option<ButtonId>,
    pub migration_notice: Option<String>,
    pub actions_enabled: bool,
    pub actions_requested: bool,
    pub engine_error: Option<String>,
    pub reader_error: Option<String>,
    pub last_action_result: Option<(String, bool)>,
    launcher: crate::session::Launcher,
    session_end: Option<crate::session::SessionEnd>,
    auto_launch: bool,
    permission_snapshot: crate::permissions::Snapshot,
    awaiting_release: bool,
    pub status: String,
    root: PathBuf,
    ble_session: bool,
    connection_wanted: bool,
    io_enabled: bool,
    stopping: Option<Instant>,
    cleanup_error: Option<String>,
    stop_failure: Option<String>,
    discovery_error: Option<String>,
    startup_error: Option<String>,
    pending_connect: bool,
    discover_requested: bool,
    discovery: Option<hardware::Discovery>,
    discovery_generation: u64,
    reader_generation: u64,
    known_ble: Option<String>,
    transport: TransportEvidence,
    mode_observed: bool,
    work_busy: Arc<AtomicBool>,
    tx: mpsc::SyncSender<Event>,
    rx: Receiver<Event>,
    stop_hid: Arc<AtomicBool>,
    reader_permission_request: Arc<AtomicBool>,
    presses: Presses,
    mouse_buttons: u8,
    last_mouse: Option<Instant>,
    /// Relative HID movement for the current UI poll, independent of log sampling.
    view_motion: (i32, i32),
    learn_started: Option<Instant>,
    config_error: bool,
    disk_config: Option<Vec<u8>>,
    instance: Option<crate::instance::Server>,
    work_tx: Option<mpsc::SyncSender<QueuedAction>>,
    work_thread: Option<std::thread::JoinHandle<()>>,
    work_rx: Receiver<(u64, Result<String, String>)>,
    action_generation: Arc<std::sync::atomic::AtomicU64>,
}
impl Studio {
    pub fn new(root: PathBuf) -> Self {
        Self::new_inner(root, true)
    }
    fn new_inner(root: PathBuf, start_io: bool) -> Self {
        let (tx, rx) = mpsc::sync_channel(4096);
        let (work_tx, work_req) = mpsc::sync_channel::<QueuedAction>(16);
        let (done_tx, work_rx) = mpsc::channel();
        let action_generation = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let work_busy = Arc::new(AtomicBool::new(false));
        let busy = work_busy.clone();
        let generation = action_generation.clone();
        #[cfg(feature = "virtual-keyboard")]
        let keyboard_control = crate::keyboard::Control::default();
        #[cfg(feature = "virtual-keyboard")]
        let worker_keyboard = keyboard_control.clone();
        let work_thread = std::thread::spawn(move || {
            while let Ok(work) = work_req.recv() {
                let QueuedAction {
                    action: a,
                    epoch,
                    at,
                    ..
                } = work;
                if generation.load(Ordering::Relaxed) == epoch {
                    busy.store(true, Ordering::Release);
                    let result = if at.elapsed() >= Duration::from_millis(500) {
                        Err("Queued button press expired; discarded".into())
                    } else {
                        #[cfg(feature = "virtual-keyboard")]
                        {
                            crate::actions::execute_with_keyboard(
                                &a,
                                &generation,
                                epoch,
                                &worker_keyboard,
                                Some(work.button),
                                work.keyboard_generation,
                            )
                        }
                        #[cfg(not(feature = "virtual-keyboard"))]
                        {
                            crate::actions::execute_cancellable(&a, &generation, epoch)
                        }
                    }
                    .map(|message| format!("{}: {message}", a.label()))
                    .map_err(|error| format!("{}: {error}", a.label()));
                    busy.store(false, Ordering::Release);
                    let _ = done_tx.send((epoch, result));
                }
            }
        });
        let mut s = Self {
            config: Config::default(),
            logs: vec![],
            connected: false,
            connecting: false,
            listening: false,
            native_active: false,
            capture_service: false,
            capture_access: Default::default(),
            capture_verified: false,
            bluetooth_state: "Not checked; Discover checks the adapter".into(),
            candidates: vec![],
            reader_running: false,
            direct_mapping: Arc::new(std::sync::RwLock::new(None)),
            #[cfg(feature = "virtual-keyboard")]
            keyboard_control,
            device_choice: Arc::new(std::sync::RwLock::new(hardware::SelectedDevice::default())),
            reader_thread: None,
            packet_count: 0,
            last_packet: "No packets".into(),
            mode: MouseMode::Normal,
            learning: None,
            learn_result: None,
            last_button: None,
            migration_notice: None,
            actions_enabled: false,
            actions_requested: true,
            engine_error: None,
            reader_error: None,
            last_action_result: None,
            launcher: Default::default(),
            session_end: None,
            auto_launch: start_io,
            permission_snapshot: Default::default(),
            awaiting_release: false,
            status: "Connect your remote to get started".into(),
            root,
            ble_session: false,
            connection_wanted: true,
            io_enabled: start_io,
            stopping: None,
            cleanup_error: None,
            stop_failure: None,
            discovery_error: None,
            startup_error: None,
            pending_connect: false,
            discover_requested: false,
            discovery: None,
            discovery_generation: 0,
            reader_generation: 0,
            known_ble: None,
            transport: TransportEvidence::Unknown,
            mode_observed: false,
            work_busy,
            tx,
            rx,
            stop_hid: Arc::new(AtomicBool::new(false)),
            reader_permission_request: Arc::new(AtomicBool::new(false)),
            presses: Presses::default(),
            mouse_buttons: 0,
            last_mouse: None,
            view_motion: (0, 0),
            learn_started: None,
            config_error: false,
            disk_config: None,
            instance: None,
            work_tx: Some(work_tx),
            work_thread: Some(work_thread),
            work_rx,
            action_generation,
        };
        if let Err(e) = s.reload() {
            s.config_error = true;
            s.log("config", e);
        }
        if !start_io {
            return s;
        }
        match crate::instance::Server::start(&s.root) {
            Ok(server) => s.instance = Some(server),
            Err(e) => {
                s.io_enabled = false;
                s.startup_error = Some(e.clone());
                s.log("engine", e);
            }
        }
        if s.instance.is_some() {
            s.start_listening();
        }
        s
    }
    pub fn config_path(&self) -> PathBuf {
        self.root.join("config.json")
    }
    fn log(&mut self, category: &str, message: impl Into<String>) {
        let message = message.into();
        if message.is_empty() {
            return;
        }
        if category != "packet" {
            self.status = message.clone();
        }
        self.logs.push(LogEntry {
            time: stamp(),
            category: category.into(),
            message,
        });
        if self.logs.len() > 1500 {
            self.logs.drain(..self.logs.len() - 1500);
        }
    }
    pub fn save_config(&mut self, next: Config) -> Result<(), String> {
        self.persist_config(next)?;
        #[cfg(feature = "virtual-keyboard")]
        self.keyboard_control.cancel(None);
        self.action_generation.fetch_add(1, Ordering::Relaxed);
        self.publish_mapping();
        Ok(())
    }
    #[cfg(feature = "virtual-keyboard")]
    pub fn save_keyboard_position(
        &mut self,
        position: crate::keyboard_model::KeyboardPosition,
    ) -> Result<(), String> {
        let mut next = self.config.clone();
        next.keyboard.position = Some(position);
        self.persist_config(next)
    }
    fn persist_config(&mut self, next: Config) -> Result<(), String> {
        if self.config_error {
            return Err("The existing JSON is invalid; fix it and reload before saving. It has not been overwritten.".into());
        }
        next.validate()?;
        let disk = match std::fs::read(self.config_path()) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.to_string()),
        };
        if disk != self.disk_config {
            self.suspend_actions();
            self.engine_error =
                Some("config.json changed outside the app. Reload the file.".into());
            return Err("config.json changed outside the app. Reload before saving to preserve those changes.".into());
        }
        atomic_json_checked(&self.config_path(), &next, self.disk_config.as_deref())?;
        self.config = next;
        self.disk_config =
            Some(serde_json::to_vec_pretty(&self.config).map_err(|e| e.to_string())?);
        self.log("config", "Configuration saved");
        Ok(())
    }
    pub fn reload(&mut self) -> Result<(), String> {
        self.suspend_actions();
        self.config_error = true;
        let disk = match std::fs::read(self.config_path()) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.to_string()),
        };
        let (config, notice) = match &disk {
            Some(bytes) => crate::configuration::load(&self.config_path(), bytes)?,
            None => (Config::default(), None),
        };
        self.config = config;
        self.disk_config = if notice.is_some() {
            Some(serde_json::to_vec_pretty(&self.config).map_err(|e| e.to_string())?)
        } else {
            disk
        };
        if let Some(notice) = notice {
            self.log("config", &notice);
            self.migration_notice = Some(notice);
        }
        self.config_error = false;
        self.engine_error = None;
        Ok(())
    }
    /// Persist first: a failed save must never change the running mapping.
    pub fn save_action(
        &mut self,
        button: ButtonId,
        profile: Profile,
        action: Action,
    ) -> Result<(), String> {
        if button == ButtonId::MouseMode {
            return Err("Air mouse does not emit an assignable button press. Edit the Air mouse profile actions for the other buttons.".into());
        }
        let mut next = self.config.clone();
        let b = next
            .buttons
            .iter_mut()
            .find(|b| b.button == button)
            .ok_or("Button not found")?;
        *b.action_mut(profile) = action;
        next.fill_mr01_sources(button);
        self.save_config(next)
    }
    /// UI assignment saves immediately using the built-in device map. Manual
    /// learning is an explicit advanced override, never an assignment side effect.
    pub fn assign_action(
        &mut self,
        button: ButtonId,
        profile: Profile,
        action: Action,
    ) -> Result<(), String> {
        self.save_action(button, profile, action)?;
        self.learn_result = None;
        self.last_action_result = None;
        Ok(())
    }
    pub fn button_identified(&self, button: ButtonId, profile: Profile) -> bool {
        self.config
            .buttons
            .iter()
            .find(|b| b.button == button)
            .is_some_and(|b| !sources(b, profile).is_empty())
    }
    fn activation_error_with_accessibility(&self, trusted: bool) -> Option<String> {
        if self.learning.is_some() {
            return Some("Finish identifying the button before enabling actions.".into());
        }
        if self.config_error {
            return Some(
                "Fix the configuration file and reload it. Your changes have not been overwritten."
                    .into(),
            );
        }
        if let Err(e) = self.config.validate() {
            return Some(e);
        }
        if !self.listening {
            return Some(if self.connected {
                "The remote is connected but ACA cannot read it. Check the details in Connection."
                    .into()
            } else {
                "Remote not detected. Wake it up or connect it from Connection.".into()
            });
        }
        if !self.capture_service {
            return Some("The action reader is not active (sessions last up to 30 minutes). Start the reader and then enable actions. Your configuration is still saved.".into());
        }
        if !trusted {
            return Some("ACA needs Accessibility in System Settings → Privacy & Security. You can still save your configuration.".into());
        }
        None
    }
    pub fn action(&self, button: ButtonId, profile: Profile) -> &Action {
        self.config
            .buttons
            .iter()
            .find(|b| b.button == button)
            .map(|b| b.action(profile))
            .unwrap_or(&Action::Default)
    }
    pub fn source_label(&self, button: ButtonId, profile: Profile) -> String {
        let s = self
            .config
            .buttons
            .iter()
            .find(|b| b.button == button)
            .map(|b| sources(b, profile))
            .unwrap_or(&[]);
        if s.is_empty() {
            "Not identified · learn button".into()
        } else {
            s.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        }
    }
    /// Resume local operation. Try the known HID first; discover only when it is absent.
    pub fn connect(&mut self) {
        self.connection_wanted = true;
        self.actions_requested = true;
        self.engine_error = None;
        self.cleanup_error = None;
        self.stop_failure = None;
        self.discovery_error = None;
        self.discover_requested = true;
        if self.stopping.is_some() {
            self.pending_connect = true;
            return;
        }
        if self.listening {
            return;
        }
        if self.reader_thread.is_some() && self.stop_hid.load(Ordering::Relaxed) {
            self.stopping = Some(Instant::now());
            self.pending_connect = true;
            return;
        }
        self.launcher.attempted = false;
        self.session_end = None;
        self.start_listening();
    }
    fn begin_discovery(&mut self) {
        if !self.io_enabled
            || !self.connection_wanted
            || self.stopping.is_some()
            || self.discovery.is_some()
        {
            return;
        }
        if self.permission_snapshot.bluetooth == crate::permissions::Access::Denied {
            self.reader_error = Some(
                "Allow Bluetooth discovery for ACA, or connect the remote in Bluetooth Settings."
                    .into(),
            );
            return;
        }
        self.discovery_generation = self.discovery_generation.wrapping_add(1);
        self.discovery_error = None;
        self.connecting = true;
        self.ble_session = true;
        self.discover_requested = false;
        self.candidates.clear();
        self.discovery = Some(hardware::discover_and_connect(
            hardware::Events::new(
                self.tx.clone(),
                hardware::EventSource::Discovery,
                self.discovery_generation,
            ),
            self.root.clone(),
            self.known_ble.clone(),
        ));
    }
    pub fn connect_candidate(&mut self, id: String) {
        // Accept only identities offered by the current attempt, never arbitrary paths.
        if !self
            .candidates
            .iter()
            .any(|(candidate, _)| candidate == &id)
        {
            return;
        }
        if let Some(path) = id.strip_prefix("hid:") {
            *self.device_choice.write().unwrap() = hardware::SelectedDevice {
                path: Some(path.into()),
                ..Default::default()
            };
            self.candidates.clear();
            self.reader_error = None;
            self.connect();
        } else {
            self.suspend_actions();
            self.listening = false;
            self.native_active = false;
            self.capture_verified = false;
            self.mode_observed = false;
            self.view_motion = (0, 0);
            *self.device_choice.write().unwrap() = hardware::SelectedDevice {
                require_confirmation: true,
                ..Default::default()
            };
            self.known_ble = Some(id);
            self.candidates.clear();
            self.connect();
            self.begin_discovery();
        }
    }
    pub fn disconnect(&mut self) {
        self.connection_wanted = false;
        self.actions_requested = false;
        self.pending_connect = false;
        self.discover_requested = false;
        self.stop_runtime();
        self.poll_cleanup();
    }
    fn stop_runtime(&mut self) {
        self.stopping = Some(Instant::now());
        self.cleanup_error = None;
        self.stop_failure = None;
        self.reader_generation = self.reader_generation.wrapping_add(1);
        self.discovery_generation = self.discovery_generation.wrapping_add(1);
        if let Some(discovery) = &self.discovery {
            discovery.cancel();
        }
        self.stop_listening();
        self.launcher.cancel();
        self.connecting = false;
        self.candidates.clear();
        self.last_button = None;
        self.last_action_result = None;
        self.mode_observed = false;
        self.transport = TransportEvidence::UnverifiedAfterStop;
    }
    fn poll_cleanup(&mut self) {
        if self.discovery.as_ref().is_some_and(|d| d.finished()) {
            if let Some(mut discovery) = self.discovery.take() {
                if let Err(error) = discovery.reap() {
                    if self.stopping.is_some() {
                        self.stop_failure = Some(error);
                    } else {
                        self.discovery_error = Some(error);
                    }
                }
            }
            self.ble_session = false;
            self.connecting = false;
        }
        if self.reader_thread.as_ref().is_some_and(|t| t.is_finished()) {
            let _ = self.reader_thread.take().unwrap().join();
            self.reader_running = false;
            if self.stopping.is_none() {
                self.reader_generation = self.reader_generation.wrapping_add(1);
                self.transport = TransportEvidence::UnverifiedAfterStop;
                self.stop_listening();
                self.reader_error =
                    Some("The reader stopped. Retry setup to start a new reader.".into());
                self.engine_error = self.reader_error.clone();
            }
        }
        if let Some(started) = self.stopping {
            if self.reader_thread.is_none()
                && self.discovery.is_none()
                && !self.work_busy.load(Ordering::Acquire)
            {
                self.stopping = None;
                self.cleanup_error = None;
                self.reader_running = false;
                self.capture_service = false;
                self.capture_access = crate::permissions::Access::Unknown;
                self.capture_verified = false;
                if self.pending_connect && self.connection_wanted && self.stop_failure.is_none() {
                    self.pending_connect = false;
                    self.launcher.attempted = false;
                    self.session_end = None;
                    self.start_listening();
                }
            } else if started.elapsed() > Duration::from_secs(8) {
                self.cleanup_error = Some("ACA is still waiting for a worker to stop. No new reader or actions will start; wait or quit ACA before retrying.".into());
            }
        }
    }
    pub fn retry_setup(&mut self) -> Result<String, String> {
        if !self.connection_wanted {
            return Err("Choose Connect before restarting ACA.".into());
        }
        self.actions_requested = true;
        self.engine_error = None;
        self.reader_error = None;
        self.pending_connect = true;
        self.stop_runtime();
        self.poll_cleanup();
        Ok("Restarting the reader.".into())
    }
    pub fn connection_command(&mut self, command: ConnectionCommand) -> Result<String, String> {
        match command {
            ConnectionCommand::Connect => {
                self.connect();
                Ok("Preparing the connection.".into())
            }
            ConnectionCommand::Disconnect => {
                self.disconnect();
                Ok("Stopping ACA. Finish disconnecting this remote in Bluetooth Settings; physical disconnection is not verified.".into())
            }
            ConnectionCommand::SelectDevice(id) => {
                if !self
                    .candidates
                    .iter()
                    .any(|(candidate, _)| candidate == &id)
                {
                    return Err("That device choice is no longer available.".into());
                }
                self.connect_candidate(id);
                Ok("Selected remote; preparing the reader.".into())
            }
            ConnectionCommand::EnableService => {
                #[cfg(feature = "managed-reader")]
                {
                    self.session_end = None;
                    self.launcher.enable()?;
                    Ok(
                        "Complete ACA setup in Login Items & Extensions, then choose Retry reader."
                            .into(),
                    )
                }
                #[cfg(not(feature = "managed-reader"))]
                {
                    Err("Use the single-app ACA build for background setup.".into())
                }
            }
            ConnectionCommand::RemoveService => {
                if self.connection_wanted
                    || self.stopping.is_some()
                    || self.reader_running
                    || self.work_busy.load(Ordering::Acquire)
                {
                    return Err("Stop ACA and wait for its reader and actions to finish before removing the service.".into());
                }
                #[cfg(feature = "managed-reader")]
                {
                    self.launcher.remove()?;
                    Ok("Removing ACA's background service. Your mappings are preserved.".into())
                }
                #[cfg(not(feature = "managed-reader"))]
                {
                    Err("This build does not own a background service.".into())
                }
            }
            ConnectionCommand::RetrySetup => self.retry_setup(),
            ConnectionCommand::RequestReaderInput => {
                if !self.connection_wanted
                    || self.stopping.is_some()
                    || self.stop_hid.load(Ordering::Acquire)
                    || !self.capture_service
                {
                    return Err("Connect and wait for the reader to authenticate before requesting Input Monitoring.".into());
                }
                self.reader_permission_request
                    .store(true, Ordering::Release);
                Ok("Requesting Input Monitoring for the ACA reader.".into())
            }
            ConnectionCommand::ResolveSetup => match self.connection_status().next_action {
                Some(SetupAction::EnableService) => {
                    self.connection_command(ConnectionCommand::EnableService)
                }
                Some(SetupAction::ApproveService) => {
                    crate::service::open_settings();
                    Ok("Approve ACA in Login Items & Extensions, then retry the reader.".into())
                }
                Some(SetupAction::StudioInput | SetupAction::CaptureInput) => {
                    crate::permissions::open_settings("input")
                }
                Some(SetupAction::RequestReaderInput) => {
                    self.connection_command(ConnectionCommand::RequestReaderInput)
                }
                Some(SetupAction::Accessibility) => {
                    crate::permissions::open_settings("accessibility")
                }
                Some(SetupAction::BluetoothPermission) => {
                    crate::permissions::open_settings("bluetooth")
                }
                Some(SetupAction::BluetoothSettings) => crate::permissions::open_settings("radio"),
                Some(SetupAction::AuthorizeReader | SetupAction::RetryReader) => self.retry_setup(),
                Some(SetupAction::Connect) => {
                    self.connect();
                    Ok("Preparing the connection.".into())
                }
                Some(SetupAction::SelectDevice) => {
                    Err("Select one of the available remotes.".into())
                }
                Some(SetupAction::RepairConfiguration) => {
                    Err("Fix config.json and reload it in Connection.".into())
                }
                None => Ok(self.connection_status().reason),
            },
        }
    }
    pub fn start_listening(&mut self) {
        if !self.io_enabled
            || !self.connection_wanted
            || self.stopping.is_some()
            || self.reader_running
            || self.reader_thread.is_some()
        {
            return;
        }
        self.reader_generation = self.reader_generation.wrapping_add(1);
        self.reader_running = true;
        self.stop_hid = Arc::new(AtomicBool::new(false));
        self.reader_permission_request = Arc::new(AtomicBool::new(false));
        self.reader_thread = Some(hardware::listen(
            hardware::Events::new(
                self.tx.clone(),
                hardware::EventSource::Reader,
                self.reader_generation,
            ),
            self.stop_hid.clone(),
            self.reader_permission_request.clone(),
            self.root.clone(),
            self.direct_mapping.clone(),
            #[cfg(feature = "virtual-keyboard")]
            self.keyboard_control.clone(),
            self.device_choice.clone(),
        ));
    }
    pub fn stop_listening(&mut self) {
        self.stop_hid.store(true, Ordering::Relaxed);
        self.reader_permission_request
            .store(false, Ordering::Release);
        self.reader_permission_request = Arc::new(AtomicBool::new(false));
        self.suspend_actions();
        self.mode = MouseMode::Normal;
        self.last_mouse = None;
        self.view_motion = (0, 0);
        self.mode_observed = false;
        self.listening = false;
        self.native_active = false;
        self.learning = None;
        self.learn_started = None;
        self.presses = Presses::default();
        self.mouse_buttons = 0;
        self.awaiting_release = false;
    }
    pub fn learn(&mut self, button: ButtonId, profile: Profile) {
        #[cfg(feature = "virtual-keyboard")]
        if profile == Profile::Keyboard {
            self.learn_result = Some(Err(
                "Identify hardware in Normal or Air mouse. Keyboard uses those same source maps."
                    .into(),
            ));
            return;
        }
        if !self.connection_wanted || self.stopping.is_some() {
            return;
        }
        self.learn_result = None;
        self.awaiting_release = true;
        self.suspend_actions();
        if button == ButtonId::MouseMode {
            self.log("learn","The Air mouse button did not emit its own input during testing. Activity is detected from mouse reports.");
            return;
        }
        if !self.listening {
            self.start_listening();
        }
        self.learning = Some((button, profile));
        self.learn_started = Some(Instant::now());
        self.log(
            "learn",
            format!("Press {} once · {}", button.label(), profile.label()),
        );
    }
    pub fn cancel_learning(&mut self) {
        self.learning = None;
        self.learn_started = None;
        self.awaiting_release = !self.presses.is_released() || self.mouse_buttons != 0;
    }
    pub fn set_actions_enabled(&mut self, enabled: bool) {
        self.actions_requested = enabled && self.connection_wanted;
        self.engine_error = None;
        if !enabled {
            self.suspend_actions();
        }
    }
    fn suspend_actions(&mut self) {
        #[cfg(feature = "virtual-keyboard")]
        self.keyboard_control.cancel(None);
        self.apply_actions(false);
    }
    fn apply_actions(&mut self, enabled: bool) {
        if !enabled {
            self.action_generation.fetch_add(1, Ordering::Relaxed);
        }
        let error = enabled
            .then(|| self.activation_error_with_accessibility(true))
            .flatten();
        self.actions_enabled =
            enabled && self.connection_wanted && self.stopping.is_none() && error.is_none();
        if let Some(error) = error {
            self.log("engine", error);
        }
        self.publish_mapping();
    }
    fn publish_mapping(&self) {
        *self.direct_mapping.write().unwrap() = self.actions_enabled.then(|| {
            (
                self.config.clone(),
                self.action_generation.load(Ordering::Relaxed),
            )
        });
    }
    pub fn needs_retry(&self) -> bool {
        self.engine_error.is_some()
            || self.reader_error.is_some()
            || (self.actions_requested
                && self.listening
                && !self.capture_service
                && self.launcher.attempted
                && !self.launcher.pending())
    }
    fn session_ended(&mut self, end: crate::session::SessionEnd) {
        self.reader_generation = self.reader_generation.wrapping_add(1);
        self.stop_listening();
        self.launcher.attempted = true;
        self.capture_service = false;
        self.capture_access = crate::permissions::Access::Unknown;
        self.capture_verified = false;
        self.native_active = false;
        self.mode_observed = false;
        self.mode = MouseMode::Normal;
        self.view_motion = (0, 0);
        self.suspend_actions();
        self.log("engine", end.message());
        self.session_end = Some(end);
    }
    pub fn capture_starting(&self) -> bool {
        !self.capture_service && self.launcher.pending()
    }
    pub fn start_capture_service(&mut self) -> Result<String, String> {
        if !self.io_enabled {
            return Err("Runtime I/O is disabled for this session.".into());
        }
        if !self.connection_wanted || self.stopping.is_some() {
            return Err("Choose Connect after ACA has stopped.".into());
        }
        if !self.listening
            || !self.permission_snapshot.input.allowed()
            || !self.permission_snapshot.accessibility
        {
            return Err(
                "Connect the remote and grant ACA Input Monitoring and Accessibility first.".into(),
            );
        }
        self.actions_requested = true;
        self.engine_error = None;
        if self.capture_service || self.launcher.pending() {
            return Ok("Preparing the remote…".into());
        }
        let default_root = PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
            .join("Library/Application Support/ReadyForAirLab");
        self.launcher.attempted = true;
        if self.root != default_root {
            return Err("Automatic startup is disabled in test directories".into());
        }
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let helper = crate::session::packaged_helper(&exe)?;
        self.launcher
            .start(&helper, &self.root.join("capture.sock"))?;
        self.session_end = None;
        Ok(if cfg!(feature = "managed-reader") {
            "Connecting to ACA’s approved background service.".into()
        } else {
            "Approve the macOS dialog to use the remote. No Terminal window is opened.".into()
        })
    }
    fn reconcile_engine(&mut self, trusted: bool) {
        if !self.connection_wanted
            || self.stopping.is_some()
            || !self.actions_requested
            || self.learning.is_some()
            || self.awaiting_release
            || self.engine_error.is_some()
            || self.config_error
            || self.session_end.is_some()
            || self.device_choice.read().unwrap().require_confirmation
            || !self.listening
            || !trusted
            || !self.permission_snapshot.input.allowed()
            || (self.capture_service && !self.capture_access.allowed())
        {
            return;
        }
        if !self.capture_service {
            if self.auto_launch
                && !self.launcher.attempted
                && (!cfg!(feature = "managed-reader")
                    || self.connection_service_status() == crate::service::Status::Enabled)
            {
                if let Err(e) = self.start_capture_service() {
                    self.engine_error = Some(e.clone());
                    self.log("engine", e);
                }
            }
        } else if !self.actions_enabled {
            self.apply_actions(true);
        }
    }
    fn connection_service_status(&self) -> crate::service::Status {
        #[cfg(feature = "managed-reader")]
        {
            self.launcher.status
        }
        #[cfg(not(feature = "managed-reader"))]
        {
            crate::service::Status::Unknown
        }
    }
    pub fn connection_status(&self) -> ConnectionStatus {
        let (state, reason, next_action) = if let Some(error) = &self.cleanup_error {
            (Readiness::Failed, error.clone(), None)
        } else if let Some(error) = &self.stop_failure {
            (
                Readiness::Failed,
                error.clone(),
                Some(SetupAction::BluetoothSettings),
            )
        } else if self.stopping.is_some() {
            (
                Readiness::Disconnecting,
                "Stopping ACA readers and actions…".into(),
                None,
            )
        } else if !self.connection_wanted {
            (Readiness::NeedsSetup, "ACA is stopped and will not reconnect. Disconnect this remote in Bluetooth Settings; its physical connection is unverified.".into(), Some(SetupAction::BluetoothSettings))
        } else if self.discover_requested
            && self.permission_snapshot.bluetooth == crate::permissions::Access::Denied
            && !self.listening
        {
            (
                Readiness::NeedsSetup,
                "Allow Bluetooth discovery, or connect the paired remote in Bluetooth Settings."
                    .into(),
                Some(SetupAction::BluetoothPermission),
            )
        } else if let Some(error) = &self.startup_error {
            (Readiness::Failed, error.clone(), None)
        } else if let Some(error) = &self.discovery_error {
            (
                Readiness::Failed,
                error.clone(),
                Some(SetupAction::BluetoothSettings),
            )
        } else if self.config_error {
            (
                Readiness::NeedsSetup,
                "Fix the configuration file and reload it.".into(),
                Some(SetupAction::RepairConfiguration),
            )
        } else if !self.candidates.is_empty()
            || self.device_choice.read().unwrap().require_confirmation
        {
            (
                Readiness::NeedsSetup,
                "Select the remote to use.".into(),
                Some(SetupAction::SelectDevice),
            )
        } else if !self.permission_snapshot.input.allowed() {
            (
                Readiness::NeedsSetup,
                "Enable Input Monitoring for ACA.".into(),
                Some(SetupAction::StudioInput),
            )
        } else if !self.permission_snapshot.accessibility {
            (
                Readiness::NeedsSetup,
                "Enable Accessibility for ACA.".into(),
                Some(SetupAction::Accessibility),
            )
        } else if cfg!(feature = "managed-reader") && self.session_end.is_some() {
            (
                Readiness::NeedsSetup,
                self.session_end.as_ref().unwrap().message(),
                Some(SetupAction::EnableService),
            )
        } else if cfg!(feature = "managed-reader")
            && !self.capture_service
            && matches!(
                self.connection_service_status(),
                crate::service::Status::NotRegistered | crate::service::Status::NotFound
            )
            && !self.capture_starting()
        {
            (Readiness::NeedsSetup, "Enable ACA's background service. macOS may ask you to approve it once in Login Items & Extensions.".into(), Some(SetupAction::EnableService))
        } else if cfg!(feature = "managed-reader")
            && self.connection_service_status() == crate::service::Status::RequiresApproval
        {
            (
                Readiness::NeedsSetup,
                "Allow ACA in Login Items & Extensions, then retry the reader.".into(),
                Some(SetupAction::ApproveService),
            )
        } else if self.capture_service && !self.capture_access.allowed() {
            (
                Readiness::NeedsSetup,
                if self.capture_access == crate::permissions::Access::Unknown {
                    "Request Input Monitoring for the ACA reader. macOS has not determined its permission yet.".into()
                } else if cfg!(feature = "managed-reader") {
                    "Allow the ACA reader in Input Monitoring. macOS determines which entry requires permission.".into()
                } else {
                    "Enable Input Monitoring for ACA Capture.".into()
                },
                Some(
                    if self.capture_access == crate::permissions::Access::Unknown {
                        SetupAction::RequestReaderInput
                    } else {
                        SetupAction::CaptureInput
                    },
                ),
            )
        } else if let Some(end) = &self.session_end {
            (
                Readiness::NeedsSetup,
                end.message(),
                Some(SetupAction::AuthorizeReader),
            )
        } else if let Some(error) = &self.engine_error {
            (
                Readiness::NeedsSetup,
                error.clone(),
                Some(SetupAction::RetryReader),
            )
        } else if self.connecting {
            (
                Readiness::Connecting,
                "Connecting to the selected remote…".into(),
                None,
            )
        } else if !self.connected {
            (
                Readiness::NotConnected,
                "Remote not detected. Wake it up or connect it.".into(),
                Some(SetupAction::Connect),
            )
        } else if let Some(error) = &self.reader_error {
            (
                Readiness::NeedsSetup,
                error.clone(),
                Some(SetupAction::RetryReader),
            )
        } else if self.capture_starting() {
            (
                Readiness::Preparing,
                "Waiting for macOS authorization…".into(),
                None,
            )
        } else if !self.capture_service && self.launcher.attempted {
            (
                Readiness::NeedsSetup,
                if cfg!(feature = "managed-reader") {
                    "Choose Retry reader to connect to ACA's approved service and verify input."
                        .into()
                } else {
                    "Reader session ended or authorization was canceled. Retry when ready.".into()
                },
                Some(if cfg!(feature = "managed-reader") {
                    SetupAction::RetryReader
                } else {
                    SetupAction::AuthorizeReader
                }),
            )
        } else if self.learning.is_some() || self.awaiting_release {
            (
                Readiness::Preparing,
                "Identifying button · release it to continue.".into(),
                None,
            )
        } else if self.native_active && self.actions_enabled && self.capture_verified {
            (
                Readiness::Ready,
                "Remote ready · actions active.".into(),
                None,
            )
        } else {
            (
                Readiness::Preparing,
                if self.native_active {
                    "Press a remote button to verify input."
                } else {
                    "Preparing the remote reader…"
                }
                .into(),
                None,
            )
        };
        ConnectionStatus {
            state,
            reason,
            next_action,
            device: self
                .device_choice
                .read()
                .unwrap()
                .path
                .as_ref()
                .map(|path| DeviceIdentity {
                    id: format!("hid:{path}"),
                    name: hardware::REMOTE_NAME.into(),
                })
                .or_else(|| {
                    self.known_ble.as_ref().map(|id| DeviceIdentity {
                        id: id.clone(),
                        name: hardware::REMOTE_NAME.into(),
                    })
                }),
            candidates: self
                .candidates
                .iter()
                .map(|(id, name)| DeviceIdentity {
                    id: id.clone(),
                    name: name.clone(),
                })
                .collect(),
            observed_mode: (self.listening && self.mode_observed).then_some(self.mode),
            transport: self.transport,
            disconnect: DisconnectCapability::SystemSettingsHandoff,
            reconnect_allowed: self.connection_wanted,
            permissions: self.permission_snapshot,
            service_error: {
                #[cfg(feature = "managed-reader")]
                {
                    self.launcher.last_error.clone()
                }
                #[cfg(not(feature = "managed-reader"))]
                {
                    None
                }
            },
            service_status: {
                #[cfg(feature = "managed-reader")]
                {
                    self.launcher.status
                }
                #[cfg(not(feature = "managed-reader"))]
                {
                    crate::service::Status::Unknown
                }
            },
            capture_access: self.capture_access,
            capture_authenticated: self.capture_service,
            exclusive_reader: self.native_active,
            reading_verified: self.capture_verified,
        }
    }
    pub fn engine_status(&self) -> String {
        self.connection_status().reason
    }
    pub fn test_action(&mut self, button: ButtonId, profile: Profile) -> Result<(), String> {
        if !self.connection_wanted || self.stopping.is_some() {
            return Err("Connect ACA before testing actions.".into());
        }
        self.config.validate()?;
        match self.action(button, profile) {
            Action::Default | Action::Disabled => {
                return Err("Select a custom action to test".into());
            }
            _ => self.dispatch(button, profile, Instant::now()),
        }
        Ok(())
    }
    fn dispatch(&mut self, button: ButtonId, profile: Profile, at: Instant) {
        self.dispatch_session(button, profile, at, None);
    }
    fn dispatch_session(
        &mut self,
        button: ButtonId,
        profile: Profile,
        at: Instant,
        _keyboard_generation: Option<u64>,
    ) {
        self.last_action_result = None;
        let a = self.action(button, profile).clone();
        #[cfg(feature = "virtual-keyboard")]
        if matches!(a, Action::Keyboard { .. }) {
            self.last_action_result = Some(("Use Keyboard settings for a non-typing preview, or assign an opener and focus another app.".into(), true));
            return;
        }
        self.log(
            "action",
            format!("{} · {} → {}", button.label(), profile.label(), a.label()),
        );
        if self
            .work_tx
            .as_ref()
            .expect("worker exists while Studio is alive")
            .try_send(QueuedAction {
                action: a,
                epoch: self.action_generation.load(Ordering::Relaxed),
                at,
                #[cfg(feature = "virtual-keyboard")]
                button,
                #[cfg(feature = "virtual-keyboard")]
                keyboard_generation: _keyboard_generation,
            })
            .is_err()
        {
            self.log("action", "Action queue full; button press discarded");
        }
    }
    pub fn clear_logs(&mut self) {
        self.logs.clear();
    }
    pub fn export_logs(&mut self) -> Result<PathBuf, String> {
        let p = self.root.join(format!("logs-{}.json", stamp()));
        atomic_json(&p, &self.logs)?;
        Ok(p)
    }
    fn change_mode(&mut self, m: MouseMode) {
        if m == MouseMode::Normal {
            self.view_motion = (0, 0);
        }
        if m != self.mode {
            self.mode = m;
            self.log("mode", m.label());
        }
    }
    pub fn poll(&mut self) {
        self.poll_with_access(crate::permissions::Snapshot::observe());
    }
    /// Consume fresh movement once; this never changes cursor or action routing.
    pub fn take_view_motion(&mut self) -> (i32, i32) {
        std::mem::take(&mut self.view_motion)
    }
    #[cfg(test)]
    fn poll_with_trust(&mut self, trusted: bool) {
        self.poll_with_access(crate::permissions::Snapshot {
            input: crate::permissions::Access::Granted,
            accessibility: trusted,
            bluetooth: crate::permissions::Access::Granted,
        });
    }
    fn poll_with_access(&mut self, access: crate::permissions::Snapshot) {
        self.poll_cleanup();
        self.permission_snapshot = access;
        let trusted = access.accessibility;
        self.view_motion = (0, 0);
        if let Some(end) = if self.io_enabled {
            self.launcher.poll()
        } else {
            None
        } {
            if self.connection_wanted && self.stopping.is_none() {
                self.session_ended(end);
            }
        }
        if self.actions_enabled
            && (!trusted || !access.input.allowed() || !self.capture_access.allowed())
        {
            self.suspend_actions();
        }

        if self.actions_enabled && self.config.validate().is_err() {
            self.suspend_actions();
        }
        self.publish_mapping();
        if self
            .learn_started
            .is_some_and(|s| s.elapsed() > Duration::from_secs(15))
        {
            self.cancel_learning();
            let message =
                "No recognizable button press received; mapping was not changed".to_string();
            self.learn_result = Some(Err(message.clone()));
            self.log("learn", message);
        }
        if self.mode == MouseMode::Active
            && self
                .last_mouse
                .is_some_and(|t| t.elapsed() > Duration::from_millis(self.config.mouse_idle_ms))
        {
            self.change_mode(MouseMode::Normal);
            self.mode_observed = false;
        }
        for _ in 0..4096 {
            let Ok(event) = self.rx.try_recv() else { break };
            let e = match event {
                Event::Scoped {
                    source,
                    generation,
                    event,
                } => {
                    let current = match source {
                        hardware::EventSource::Reader => self.reader_generation,
                        hardware::EventSource::Discovery => self.discovery_generation,
                    };
                    if generation != current {
                        continue;
                    }
                    *event
                }
                event => event,
            };
            if !self.connection_wanted || self.stopping.is_some() {
                continue;
            }
            match e {
                Event::Scoped { .. } => continue,
                Event::HidSelected(path) => {
                    let mut selected = self.device_choice.write().unwrap();
                    if !selected.require_confirmation
                        && selected.path.as_ref().is_none_or(|p| p == &path)
                    {
                        selected.path = Some(path);
                    }
                }
                Event::BleSelected(id) => {
                    self.known_ble = Some(id);
                }
                Event::BleFailed(error) => {
                    self.discovery_error = Some(error.clone());
                    self.log("Bluetooth", error);
                }
                Event::Log(m) => self.log("system", m),
                Event::BleConnected => {
                    self.transport = TransportEvidence::LocalBluetoothConnected;
                    self.reader_error = None;
                    self.connected = true;
                    self.connecting = false;
                    self.start_listening();
                }
                Event::BleDone => {
                    self.connecting = false;
                    self.ble_session = false;
                    if !self.listening {
                        self.connected = false;
                        self.transport = TransportEvidence::Unknown;
                    }
                }
                Event::CapturePermission(access) => {
                    self.capture_access = access;
                    if !access.allowed() {
                        self.suspend_actions();
                        self.native_active = false;
                        self.capture_verified = false;
                    }
                }
                Event::BluetoothState(state) => {
                    self.bluetooth_state = state;
                }
                Event::CaptureService(ready) => {
                    if ready && self.session_end.is_some() {
                        continue;
                    }
                    self.capture_service = ready;
                    if ready {
                        #[cfg(feature = "managed-reader")]
                        self.launcher.authenticated();
                        self.launcher.attempted = true;
                    }
                    if !ready {
                        self.native_active = false;
                        self.capture_access = crate::permissions::Access::Unknown;
                        self.capture_verified = false;
                        self.suspend_actions();
                    }
                    self.log(
                        "engine",
                        if ready {
                            "Capture service ready"
                        } else {
                            "Capture service disconnected"
                        },
                    );
                }
                Event::HidReady { exclusive } => {
                    if self.device_choice.read().unwrap().require_confirmation {
                        continue;
                    }
                    self.transport = TransportEvidence::HidPresent;
                    self.mode_observed = false;
                    self.reader_error = None;
                    self.awaiting_release = false;
                    self.connected = true;
                    self.candidates.clear();
                    self.listening = true;
                    self.native_active = exclusive;
                    self.capture_verified = false;
                    self.presses = Presses::default();
                    self.mouse_buttons = 0;
                    self.last_mouse = None;
                    self.mode = MouseMode::Normal;
                    self.view_motion = (0, 0);
                    self.log(
                        "engine",
                        if exclusive {
                            "Native engine active"
                        } else {
                            "Remote connected · observing · actions off"
                        },
                    );
                }
                Event::HidUnavailable { present, message } => {
                    self.transport = if present {
                        TransportEvidence::HidPresent
                    } else {
                        TransportEvidence::NotDetected
                    };
                    self.mode_observed = false;
                    self.reader_error = Some(message.clone());
                    if self.actions_enabled && present {
                        self.engine_error = Some(message.clone());
                    }
                    self.connected = present;
                    self.listening = false;
                    self.native_active = false;
                    self.capture_verified = false;
                    self.suspend_actions();
                    self.mode = MouseMode::Normal;
                    self.last_mouse = None;
                    self.view_motion = (0, 0);
                    self.log("connection", message);
                    if !present && self.discover_requested {
                        self.begin_discovery();
                    }
                }
                Event::BleCandidates(candidates) => {
                    self.candidates = candidates;
                }
                Event::DirectAction {
                    #[cfg(feature = "virtual-keyboard")]
                    keyboard_generation,
                    button,
                    profile,
                    epoch,
                    at,
                } => {
                    if self.actions_enabled
                        && self.learning.is_none()
                        && epoch == self.action_generation.load(Ordering::Relaxed)
                        && at.elapsed() < Duration::from_millis(500)
                    {
                        #[cfg(feature = "virtual-keyboard")]
                        let session = keyboard_generation;
                        #[cfg(not(feature = "virtual-keyboard"))]
                        let session = None;
                        self.dispatch_session(button, profile, at, session);
                    } else {
                        self.log("routing", format!("{} · {}: button press discarded (remote paused, learning, or outdated configuration)", button.label(), profile.label()));
                    }
                }
                Event::HidStopped => {
                    self.reader_running = false;
                    self.listening = false;
                    self.stop_listening();
                    self.presses = Presses::default();
                    self.mouse_buttons = 0;
                }
                Event::Packet { at, bytes } => {
                    if self.device_choice.read().unwrap().require_confirmation {
                        continue;
                    }
                    if self.native_active {
                        self.capture_verified = true;
                    }
                    self.packet(at, &bytes);
                }
            }
        }
        if self.discover_requested
            && (self.transport == TransportEvidence::NotDetected
                || self.device_choice.read().unwrap().require_confirmation)
            && self.candidates.is_empty()
        {
            self.begin_discovery();
        }
        self.reconcile_engine(trusted);
        while let Ok((epoch, r)) = self.work_rx.try_recv() {
            if epoch != self.action_generation.load(Ordering::Relaxed) {
                continue;
            }
            self.last_action_result = Some(match &r {
                Ok(message) => (message.clone(), false),
                Err(error) => (error.clone(), true),
            });
            match r {
                Ok(s) => self.log("action", s),
                Err(e) => self.log("error", e),
            }
        }
    }

    fn packet(&mut self, at: Instant, bytes: &[u8]) {
        if mode_evidence(bytes).is_some() {
            self.mode_observed = true;
        }
        self.packet_count += 1;
        self.last_packet = crate::model::hex(bytes);
        self.connected = true;
        // Bound stored traffic and avoid formatting every mouse packet into a growing log.
        if bytes.first() != Some(&2) || self.packet_count % 20 == 0 {
            self.log("packet", self.last_packet.clone());
        }
        let mut pressed: Vec<InputSource> = self
            .presses
            .update(bytes)
            .into_iter()
            .map(|b| match b {
                crate::presses::Button::Keyboard(k) => InputSource::Keyboard(k),
                crate::presses::Button::Consumer(k) => InputSource::Consumer(k),
            })
            .collect();
        if let Some(crate::report::Report::Mouse { buttons, .. }) = crate::report::decode(bytes) {
            for b in 0..5 {
                if buttons & (1 << b) != 0 && self.mouse_buttons & (1 << b) == 0 {
                    pressed.push(InputSource::Mouse(b + 1));
                }
            }
            self.mouse_buttons = buttons;
        }
        if self.learning.is_none() && self.presses.is_released() && self.mouse_buttons == 0 {
            self.awaiting_release = false;
        }
        if at.elapsed() > Duration::from_millis(500) {
            return;
        }
        if let Some(crate::report::Report::Mouse { x, y, .. }) = crate::report::decode(bytes) {
            self.view_motion.0 = self.view_motion.0.saturating_add(i32::from(x));
            self.view_motion.1 = self.view_motion.1.saturating_add(i32::from(y));
        }
        let before = if self.mode == MouseMode::Active {
            Profile::AirMouse
        } else {
            Profile::Normal
        };
        if let Some(mode) = mode_evidence(bytes) {
            self.last_mouse = (mode == MouseMode::Active).then_some(at);
            self.change_mode(mode);
        }
        if let Some((button, profile)) = self.learning {
            if pressed.len() == 1 && self.learn_started.is_some_and(|t| at >= t) {
                let source = pressed[0].clone();
                let mut next = self.config.clone();
                let b = next
                    .buttons
                    .iter_mut()
                    .find(|b| b.button == button)
                    .unwrap();
                if profile == Profile::Normal {
                    b.normal_sources = vec![source.clone()]
                } else {
                    b.mouse_sources = vec![source.clone()]
                };
                self.cancel_learning();
                let result = self.save_config(next).map(|_| {
                    self.last_button = Some(button);
                    format!("{} identified and saved ({source}). Select an action; it saves automatically. Release the button to resume the remote automatically.", button.label())
                });
                self.log(
                    "learn",
                    match &result {
                        Ok(m) | Err(m) => m.clone(),
                    },
                );
                self.learn_result = Some(result);
            }
            return;
        }
        let observed_profile = if bytes.first() == Some(&2) {
            Profile::AirMouse
        } else {
            before
        };
        for s in pressed {
            if let Some(b) = self
                .config
                .buttons
                .iter()
                .find(|b| sources(b, observed_profile).contains(&s))
            {
                self.last_button = Some(b.button);
                let button = b.button;
                let action = if observed_profile == Profile::Normal {
                    &b.normal
                } else {
                    &b.air_mouse
                };
                let disposition = if !self.actions_enabled || !self.native_active {
                    format!("observation only · {}", self.engine_status())
                } else if *action == Action::Default {
                    "original function".into()
                } else if *action == Action::Disabled {
                    "disabled in this profile".into()
                } else {
                    continue; // DirectAction records the actual dispatch separately.
                };
                self.log(
                    "routing",
                    format!(
                        "{s} → {} · {}: {disposition}",
                        button.label(),
                        observed_profile.label()
                    ),
                );
            } else {
                self.log("routing", format!("{s} · {}: no identified button. Select the button in Buttons and click Identify button.", observed_profile.label()));
            }
        }
    }
}
impl Drop for Studio {
    fn drop(&mut self) {
        self.connection_wanted = false;
        self.stop_runtime();
        self.work_tx.take();
        if let Some(worker) = self.work_thread.take() {
            let _ = worker.join();
        }
        // Discovery cancellation is bounded, including scan/local-link cleanup.
        self.discovery.take();
        // HID reads and broker waits are bounded. A platform call stalled inside
        // the OS cannot be force-killed safely; its stop flag prevents late output.
        let deadline = Instant::now() + Duration::from_secs(8);
        while self
            .reader_thread
            .as_ref()
            .is_some_and(|t| !t.is_finished())
            && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        if self.reader_thread.as_ref().is_some_and(|t| t.is_finished()) {
            let _ = self.reader_thread.take().unwrap().join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_explicit_reader_permission_action_arms_current_generation() {
        let mut s = Studio::new_inner(
            std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()),
            false,
        );
        assert!(!s.reader_permission_request.load(Ordering::Acquire));
        s.connect();
        assert!(!s.reader_permission_request.load(Ordering::Acquire));
        assert!(
            s.connection_command(ConnectionCommand::RequestReaderInput)
                .is_err()
        );
        assert!(!s.reader_permission_request.load(Ordering::Acquire));
        s.capture_service = true;
        s.connection_command(ConnectionCommand::RequestReaderInput)
            .unwrap();
        let old_request = s.reader_permission_request.clone();
        assert!(old_request.load(Ordering::Acquire));
        s.retry_setup().unwrap();
        assert!(!old_request.load(Ordering::Acquire));
        assert!(!s.reader_permission_request.load(Ordering::Acquire));
        assert!(!Arc::ptr_eq(&old_request, &s.reader_permission_request));
        old_request.store(true, Ordering::Release);
        s.connect();
        assert!(!s.reader_permission_request.load(Ordering::Acquire));
        s.disconnect();
        assert!(!s.reader_permission_request.load(Ordering::Acquire));
        assert!(
            s.connection_command(ConnectionCommand::RequestReaderInput)
                .is_err()
        );
    }
    #[test]
    fn unknown_reader_permission_requests_once_and_grant_unblocks_actions() {
        let mut s = Studio::new_inner(
            std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()),
            false,
        );
        s.connected = true;
        s.listening = true;
        s.capture_service = true;
        s.poll_with_trust(true);
        assert_eq!(
            s.connection_status().next_action,
            Some(SetupAction::RequestReaderInput)
        );
        assert!(!s.actions_enabled);
        assert!(s.direct_mapping.read().unwrap().is_none());
        s.connection_command(ConnectionCommand::ResolveSetup)
            .unwrap();
        assert!(s.reader_permission_request.load(Ordering::Acquire));
        s.tx.send(Event::CapturePermission(
            crate::permissions::Access::Granted,
        ))
        .unwrap();
        s.poll_with_trust(true);
        assert!(s.actions_enabled);
        assert!(s.direct_mapping.read().unwrap().is_some());
        s.tx.send(Event::CapturePermission(crate::permissions::Access::Denied))
            .unwrap();
        s.poll_with_trust(true);
        assert!(!s.actions_enabled);
        assert_eq!(
            s.connection_status().next_action,
            Some(SetupAction::CaptureInput)
        );
    }
    #[test]
    fn explicit_disconnect_ignores_late_events_until_connect() {
        use crate::lifecycle::{Readiness, SetupAction, TransportEvidence};
        let mut s = Studio::new_inner(
            std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()),
            false,
        );
        s.listening = true;
        s.connected = true;
        s.capture_service = true;
        s.capture_access = crate::permissions::Access::Granted;
        s.native_active = true;
        s.poll_with_trust(true);
        let old_epoch = s.action_generation.load(Ordering::Relaxed);
        s.disconnect();
        s.tx.send(Event::BleConnected).unwrap();
        s.tx.send(Event::HidReady { exclusive: true }).unwrap();
        s.tx.send(Event::Packet {
            at: Instant::now(),
            bytes: vec![2, 0, 20, 10, 0],
        })
        .unwrap();
        s.poll_with_trust(true);
        let status = s.connection_status();
        assert_eq!(status.state, Readiness::NeedsSetup);
        assert_eq!(status.next_action, Some(SetupAction::BluetoothSettings));
        assert_eq!(status.transport, TransportEvidence::UnverifiedAfterStop);
        assert!(!status.reconnect_allowed);
        assert!(!s.actions_enabled);
        assert!(!s.listening);
        assert_eq!(s.take_view_motion(), (0, 0));
        assert!(s.action_generation.load(Ordering::Relaxed) > old_epoch);
        assert!(s.test_action(ButtonId::Home, Profile::Normal).is_err());
        s.connect();
        assert!(s.connection_status().reconnect_allowed);
    }

    #[test]
    fn capture_permission_revocation_stops_actions_in_the_same_poll() {
        let mut s = Studio::new_inner(
            std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()),
            false,
        );
        s.connected = true;
        s.listening = true;
        s.capture_service = true;
        s.capture_access = crate::permissions::Access::Granted;
        s.poll_with_trust(true);
        assert!(s.actions_enabled);
        s.tx.send(Event::CapturePermission(crate::permissions::Access::Denied))
            .unwrap();
        s.poll_with_trust(true);
        assert!(!s.actions_enabled);
        assert!(s.direct_mapping.read().unwrap().is_none());
        assert_eq!(
            s.connection_status().next_action,
            Some(SetupAction::CaptureInput)
        );
    }

    #[test]
    fn canceled_or_expired_authorization_requires_explicit_retry() {
        use crate::session::SessionEnd;
        for end in [SessionEnd::Canceled, SessionEnd::Expired, SessionEnd::Ended] {
            let mut s = Studio::new_inner(
                std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()),
                false,
            );
            s.connected = true;
            s.listening = true;
            s.capture_service = true;
            s.capture_access = crate::permissions::Access::Granted;
            s.native_active = true;
            s.poll_with_trust(true);
            s.session_ended(end.clone());
            for _ in 0..3 {
                s.poll_with_trust(true);
            }
            assert_eq!(s.connection_status().state, Readiness::NeedsSetup);
            assert_eq!(
                s.connection_status().next_action,
                Some(if cfg!(feature = "managed-reader") {
                    SetupAction::EnableService
                } else {
                    SetupAction::AuthorizeReader
                })
            );
            assert!(!s.actions_enabled);
            assert!(s.launcher.attempted);
            assert!(!s.launcher.pending());
            assert_eq!(s.session_end, Some(end));
            s.retry_setup().unwrap();
            assert!(!s.launcher.attempted);
            assert!(s.session_end.is_none());
        }
    }

    #[test]
    fn discovery_failure_remains_actionable_after_worker_finishes() {
        let mut s = Studio::new_inner(
            std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()),
            false,
        );
        s.tx.send(Event::BleFailed("Bluetooth adapter timed out".into()))
            .unwrap();
        s.tx.send(Event::BleDone).unwrap();
        s.poll_with_trust(true);
        assert_eq!(s.connection_status().state, Readiness::Failed);
        assert!(s.connection_status().reason.contains("timed out"));
        assert_eq!(
            s.connection_status().next_action,
            Some(SetupAction::BluetoothSettings)
        );
    }

    #[test]
    fn previous_attempt_events_cannot_change_the_reconnected_session() {
        let mut s = Studio::new_inner(
            std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()),
            false,
        );
        let reader = hardware::Events::new(
            s.tx.clone(),
            hardware::EventSource::Reader,
            s.reader_generation,
        );
        let discovery = hardware::Events::new(
            s.tx.clone(),
            hardware::EventSource::Discovery,
            s.discovery_generation,
        );
        s.disconnect();
        s.connect();
        s.connecting = true;
        reader.send(Event::HidSelected("old-path".into())).unwrap();
        reader.send(Event::HidReady { exclusive: true }).unwrap();
        reader
            .send(Event::Packet {
                at: Instant::now(),
                bytes: vec![2, 0, 20, 10, 0],
            })
            .unwrap();
        discovery.send(Event::BleConnected).unwrap();
        discovery.send(Event::BleDone).unwrap();
        discovery
            .send(Event::BleCandidates(vec![("stale".into(), "old".into())]))
            .unwrap();
        s.poll_with_trust(true);
        assert!(s.connecting);
        assert!(!s.listening);
        assert!(!s.actions_enabled);
        assert!(s.connection_status().device.is_none());
        assert!(s.candidates.is_empty());
        assert_eq!(s.packet_count, 0);
        assert_eq!(s.take_view_motion(), (0, 0));
    }

    #[test]
    fn device_selection_rejects_stale_choices_and_keeps_selected_identity() {
        let mut s = Studio::new_inner(
            std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()),
            false,
        );
        s.tx.send(Event::BleCandidates(vec![
            ("hid:A".into(), "Remote A".into()),
            ("hid:B".into(), "Remote B".into()),
        ]))
        .unwrap();
        s.poll_with_trust(true);
        assert_eq!(
            s.connection_status().next_action,
            Some(SetupAction::SelectDevice)
        );
        assert!(
            s.connection_command(ConnectionCommand::SelectDevice("hid:C".into()))
                .is_err()
        );
        s.connection_command(ConnectionCommand::SelectDevice("hid:B".into()))
            .unwrap();
        assert_eq!(s.connection_status().device.unwrap().id, "hid:B");
        s.disconnect();
        s.connect();
        assert_eq!(s.connection_status().device.unwrap().id, "hid:B");
        assert!(
            s.connection_command(ConnectionCommand::SelectDevice("hid:A".into()))
                .is_err()
        );
    }

    #[test]
    fn disconnect_cancels_running_macro_and_queued_work_before_cleanup_completes() {
        let root = std::env::temp_dir().join(format!("phase-b-cancel-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let started = root.join("started");
        let after = root.join("after");
        let queued = root.join("queued");
        let mut s = Studio::new_inner(root.clone(), false);
        s.assign_action(
            ButtonId::Home,
            Profile::Normal,
            Action::Macro {
                steps: vec![
                    Action::Shell {
                        command: format!("printf started > '{}'; /bin/sleep 5", started.display()),
                    },
                    Action::Shell {
                        command: format!("touch '{}'", after.display()),
                    },
                ],
            },
        )
        .unwrap();
        s.assign_action(
            ButtonId::Apps,
            Profile::Normal,
            Action::Shell {
                command: format!("touch '{}'", queued.display()),
            },
        )
        .unwrap();
        s.test_action(ButtonId::Home, Profile::Normal).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !started.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(started.exists());
        s.test_action(ButtonId::Apps, Profile::Normal).unwrap();
        s.disconnect();
        while s.connection_status().state == Readiness::Disconnecting && Instant::now() < deadline {
            s.poll_with_trust(true);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(s.connection_status().state, Readiness::NeedsSetup);
        assert!(!s.work_busy.load(Ordering::Acquire));
        assert!(!after.exists());
        assert!(!queued.exists());
        assert!(
            s.last_action_result.is_none(),
            "obsolete action result leaked into the new state"
        );
        drop(s);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bluetooth_choice_requires_hid_identity_confirmation_before_actions() {
        let mut s = Studio::new_inner(
            std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()),
            false,
        );
        s.capture_service = true;
        s.capture_access = crate::permissions::Access::Granted;
        s.listening = true;
        s.native_active = true;
        s.actions_enabled = true;
        s.candidates = vec![("fixture-ble".into(), hardware::REMOTE_NAME.into())];
        s.connect_candidate("fixture-ble".into());
        s.tx.send(Event::HidReady { exclusive: true }).unwrap();
        s.poll_with_trust(true);
        assert!(
            !s.actions_enabled,
            "a BLE identifier does not prove which HID device to control"
        );
        assert_ne!(s.connection_status().state, Readiness::Ready);
        assert!(!s.connection_status().exclusive_reader);
        s.candidates = vec![("hid:chosen-path".into(), hardware::REMOTE_NAME.into())];
        s.connect_candidate("hid:chosen-path".into());
        assert!(!s.device_choice.read().unwrap().require_confirmation);
        assert_eq!(s.connection_status().device.unwrap().id, "hid:chosen-path");
    }

    #[test]
    fn sleep_and_wake_preserve_device_intent_and_clear_mode_without_reauthorization() {
        let mut s = Studio::new_inner(
            std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()),
            false,
        );
        s.tx.send(Event::HidSelected("selected-path".into()))
            .unwrap();
        s.tx.send(Event::CaptureService(true)).unwrap();
        s.tx.send(Event::CapturePermission(
            crate::permissions::Access::Granted,
        ))
        .unwrap();
        s.tx.send(Event::HidReady { exclusive: true }).unwrap();
        s.tx.send(Event::Packet {
            at: Instant::now(),
            bytes: vec![2, 0, 9, 4, 0],
        })
        .unwrap();
        s.poll_with_trust(true);
        assert_eq!(s.connection_status().state, Readiness::Ready);
        assert_eq!(s.connection_status().observed_mode, Some(MouseMode::Active));
        s.tx.send(Event::HidUnavailable {
            present: false,
            message: "fixture sleep".into(),
        })
        .unwrap();
        s.poll_with_trust(true);
        assert_eq!(s.connection_status().state, Readiness::NotConnected);
        assert!(s.connection_status().reconnect_allowed);
        assert_eq!(s.connection_status().observed_mode, None);
        assert_eq!(s.take_view_motion(), (0, 0));
        assert!(
            s.capture_service,
            "sleep should retain the authenticated helper"
        );
        assert!(s.launcher.attempted);
        s.tx.send(Event::HidReady { exclusive: false }).unwrap();
        s.poll_with_trust(true);
        assert_eq!(s.connection_status().state, Readiness::Preparing);
        s.tx.send(Event::HidReady { exclusive: true }).unwrap();
        s.poll_with_trust(true);
        assert_eq!(s.connection_status().state, Readiness::Preparing);
        s.tx.send(Event::Packet {
            at: Instant::now(),
            bytes: vec![3, 0, 0],
        })
        .unwrap();
        s.poll_with_trust(true);
        assert_eq!(s.connection_status().state, Readiness::Ready);
        assert_eq!(
            s.connection_status().device.unwrap().id,
            "hid:selected-path"
        );
        assert!(!s.launcher.pending());
    }

    #[test]
    fn slow_cleanup_never_claims_completion_or_starts_an_overlapping_reader() {
        let mut s = Studio::new_inner(
            std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()),
            false,
        );
        let finish = Arc::new(AtomicBool::new(false));
        let worker_finish = finish.clone();
        s.reader_thread = Some(std::thread::spawn(move || {
            while !worker_finish.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(5));
            }
        }));
        s.reader_running = true;
        s.disconnect();
        let initial = s.connection_status().state;
        s.stopping = Some(Instant::now() - Duration::from_secs(9));
        s.poll_with_trust(true);
        let timed_out = s.connection_status().state;
        s.connect();
        s.poll_with_trust(true);
        let still_owned = s.reader_thread.is_some();
        let has_pending_connect = s.pending_connect;
        finish.store(true, Ordering::Release);
        let deadline = Instant::now() + Duration::from_secs(1);
        while s.reader_thread.is_some() && Instant::now() < deadline {
            s.poll_with_trust(true);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(initial, Readiness::Disconnecting);
        assert_eq!(timed_out, Readiness::Failed);
        assert!(still_owned && has_pending_connect);
        assert!(s.reader_thread.is_none());
        assert!(s.stopping.is_none());
        assert!(s.connection_status().reconnect_allowed);
    }

    #[test]
    fn readiness_requires_reader_and_each_permission_not_just_bluetooth() {
        use crate::lifecycle::{Readiness, SetupAction};
        use crate::permissions::Access;
        let mut s = Studio::new_inner(
            std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()),
            false,
        );
        s.connected = true;
        s.poll_with_access(crate::permissions::Snapshot {
            input: Access::Denied,
            accessibility: true,
            bluetooth: Access::Granted,
        });
        assert_eq!(s.connection_status().state, Readiness::NeedsSetup);
        assert_eq!(
            s.connection_status().next_action,
            Some(SetupAction::StudioInput)
        );
        s.listening = true;
        s.capture_service = true;
        s.capture_access = crate::permissions::Access::Granted;
        s.capture_access = Access::Denied;
        s.poll_with_trust(true);
        assert!(!s.actions_enabled);
        assert_eq!(
            s.connection_status().next_action,
            Some(SetupAction::CaptureInput)
        );
        s.capture_access = Access::Granted;
        s.poll_with_trust(true);
        assert_eq!(s.connection_status().state, Readiness::Preparing);
        s.native_active = true;
        assert_eq!(s.connection_status().state, Readiness::Preparing);
        s.tx.send(Event::Packet {
            at: Instant::now(),
            bytes: vec![3, 0, 0],
        })
        .unwrap();
        s.poll_with_trust(true);
        assert_eq!(s.connection_status().state, Readiness::Ready);
        s.poll_with_trust(false);
        assert!(!s.actions_enabled);
        assert_eq!(
            s.connection_status().next_action,
            Some(SetupAction::Accessibility)
        );
    }

    #[test]
    fn legacy_config_is_backed_up_and_preserves_non_voice_settings() {
        let root = std::env::temp_dir().join(format!("migration-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let original = include_bytes!("../tests/fixtures/config-v1.json");
        std::fs::write(root.join("config.json"), original).unwrap();
        let mut studio = Studio::new_inner(root.clone(), false);
        studio.reload().unwrap();
        assert_eq!(studio.config.version, Config::VERSION);
        assert_eq!(
            studio.action(ButtonId::Home, Profile::Normal),
            &Action::Disabled
        );
        assert_eq!(
            std::fs::read(root.join("config.v1.backup.json")).unwrap(),
            original
        );
        let mut expected: serde_json::Value = serde_json::from_slice(original).unwrap();
        expected["version"] = Config::VERSION.into();
        #[cfg(feature = "virtual-keyboard")]
        {
            expected["keyboard"] = serde_json::to_value(&studio.config.keyboard).unwrap();
            for b in expected["buttons"].as_array_mut().unwrap() {
                let id: ButtonId = serde_json::from_value(b["button"].clone()).unwrap();
                b["keyboard"] = serde_json::to_value(default_keyboard_action(id)).unwrap();
            }
        }
        expected["buttons"][0]["normal"] = serde_json::json!({"type":"disabled"});
        expected.as_object_mut().unwrap().remove("audio_input");
        expected.as_object_mut().unwrap().remove("transcription");
        assert_eq!(serde_json::to_value(&studio.config).unwrap(), expected);
        assert_eq!(
            serde_json::to_value(
                serde_json::from_slice::<Config>(&std::fs::read(root.join("config.json")).unwrap())
                    .unwrap()
            )
            .unwrap(),
            expected
        );
        assert!(
            studio
                .logs
                .iter()
                .any(|entry| entry.message.contains("Home") && entry.message.contains("Disabled"))
        );
        studio.save_config(studio.config.clone()).unwrap();
        assert_eq!(
            serde_json::to_value(&studio.config).unwrap(),
            expected,
            "Saving a migrated configuration must preserve empty and custom source lists"
        );
        assert_eq!(
            std::fs::read(root.join("config.v1.backup.json")).unwrap(),
            original
        );
        drop(studio);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_voice_inside_nested_macro_disables_the_entire_assignment() {
        let root = std::env::temp_dir().join(format!("migration-macro-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let mut legacy: serde_json::Value =
            serde_json::from_slice(include_bytes!("../tests/fixtures/config-v1.json")).unwrap();
        legacy["buttons"][1]["air_mouse"] = serde_json::json!({"type":"macro","steps":[
            {"type":"shell","command":"printf never-run"},
            {"type":"macro","steps":[{"type":"voice_toggle"}]}
        ]});
        let bytes = serde_json::to_vec(&legacy).unwrap();
        std::fs::write(root.join("config.json"), &bytes).unwrap();
        let studio = Studio::new_inner(root.clone(), false);
        assert_eq!(
            studio.action(ButtonId::Experiences, Profile::AirMouse),
            &Action::Disabled
        );
        assert!(
            studio
                .migration_notice
                .as_ref()
                .unwrap()
                .contains("Experiences / Air mouse")
        );
        assert_eq!(
            std::fs::read(root.join("config.v1.backup.json")).unwrap(),
            bytes
        );
        drop(studio);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn malformed_and_future_configurations_are_never_migrated_or_overwritten() {
        let original: serde_json::Value =
            serde_json::from_slice(include_bytes!("../tests/fixtures/config-v1.json")).unwrap();
        let mut cases = vec![b"{invalid JSON".to_vec()];
        for (pointer, bad) in [
            ("/version", serde_json::json!(3)),
            ("/version", serde_json::json!(null)),
            ("/mouse_idle_ms", serde_json::json!(10)),
            ("/audio_input", serde_json::json!(12)),
            ("/transcription/program", serde_json::json!("")),
            ("/transcription/args", serde_json::json!(["no placeholder"])),
            (
                "/buttons/0/normal",
                serde_json::json!({"type":"voice_toggle","unexpected":true}),
            ),
            ("/buttons/0/normal", serde_json::json!({"type":"unknown"})),
            (
                "/buttons/0/normal",
                serde_json::json!({"type":"macro","steps":[{"type":"voice_toggle"},{"type":"shortcut","key":"unknown-key","modifiers":[]}]}),
            ),
            (
                "/buttons/0/normal",
                serde_json::json!({"type":"macro","steps":[{"type":"voice_toggle"},{"type":"macro","steps":[]}]}),
            ),
            (
                "/buttons/0/normal_sources",
                serde_json::json!([{"Mouse":9}]),
            ),
            ("/buttons/1/button", serde_json::json!("home")),
        ] {
            let mut value = original.clone();
            *value.pointer_mut(pointer).unwrap() = bad;
            cases.push(serde_json::to_vec(&value).unwrap());
        }
        let mut unknown = original.clone();
        unknown["unexpected"] = true.into();
        cases.push(serde_json::to_vec(&unknown).unwrap());
        let mut current = serde_json::to_value(Config::default()).unwrap();
        current["buttons"][0]["normal"] = serde_json::json!({"type":"voice_toggle"});
        cases.push(serde_json::to_vec(&current).unwrap());
        current["buttons"][0]["normal"] = serde_json::json!({"type":"default","unexpected":true});
        cases.push(serde_json::to_vec(&current).unwrap());
        let duplicate = String::from_utf8(serde_json::to_vec(&original).unwrap())
            .unwrap()
            .replacen("{", "{\"version\":1,", 1);
        cases.push(duplicate.into_bytes());
        for (index, bytes) in cases.into_iter().enumerate() {
            let root =
                std::env::temp_dir().join(format!("migration-reject-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(root.join("config.json"), &bytes).unwrap();
            let mut studio = Studio::new_inner(root.clone(), false);
            assert!(studio.reload().is_err(), "case {index} was accepted");
            assert!(
                studio.save_config(Config::default()).is_err(),
                "case {index} allowed overwrite"
            );
            assert_eq!(std::fs::read(root.join("config.json")).unwrap(), bytes);
            assert!(!root.join("config.v1.backup.json").exists());
            assert!(!root.join("config.previous.json").exists());
            drop(studio);
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn migration_requires_a_safe_backup_and_retains_it_across_later_saves() {
        let original = include_bytes!("../tests/fixtures/config-v1.json");
        for obstruction_is_directory in [false, true] {
            let root =
                std::env::temp_dir().join(format!("migration-backup-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(root.join("config.json"), original).unwrap();
            let backup = root.join("config.v1.backup.json");
            if obstruction_is_directory {
                std::fs::create_dir(&backup).unwrap();
            } else {
                std::fs::write(&backup, b"previous legacy file").unwrap();
            }
            let mut studio = Studio::new_inner(root.clone(), false);
            assert!(studio.reload().is_err());
            assert!(studio.save_config(Config::default()).is_err());
            assert_eq!(std::fs::read(root.join("config.json")).unwrap(), original);
            if obstruction_is_directory {
                std::fs::remove_dir(&backup).unwrap();
            } else {
                assert_eq!(std::fs::read(&backup).unwrap(), b"previous legacy file");
                std::fs::remove_file(&backup).unwrap();
            }
            // An interrupted migration may have already written the exact backup.
            std::fs::write(&backup, original).unwrap();
            studio.reload().unwrap();
            assert_eq!(studio.config.version, Config::VERSION);
            studio.save_config(studio.config.clone()).unwrap();
            assert_eq!(std::fs::read(&backup).unwrap(), original);
            drop(studio);
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn failed_migration_commit_keeps_original_and_completed_legacy_backup() {
        let root = std::env::temp_dir().join(format!("migration-commit-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let original = include_bytes!("../tests/fixtures/config-v1.json");
        std::fs::write(root.join("config.json"), original).unwrap();
        std::fs::write(root.join("audio-user.wav"), b"user recording").unwrap();
        // Block the ordinary previous-file backup, after the dedicated legacy
        // backup has succeeded but before config.json can be replaced.
        std::fs::create_dir(root.join("config.previous.json")).unwrap();
        let mut studio = Studio::new_inner(root.clone(), false);
        assert!(studio.reload().is_err());
        assert!(studio.save_config(Config::default()).is_err());
        assert_eq!(std::fs::read(root.join("config.json")).unwrap(), original);
        assert_eq!(
            std::fs::read(root.join("config.v1.backup.json")).unwrap(),
            original
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(root.join("config.v1.backup.json"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        std::fs::remove_dir(root.join("config.previous.json")).unwrap();
        studio.reload().unwrap();
        assert_eq!(studio.config.version, Config::VERSION);
        assert_eq!(
            std::fs::read(root.join("audio-user.wav")).unwrap(),
            b"user recording"
        );
        drop(studio);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn view_motion_uses_fresh_remote_xy_once_without_sampling() {
        let mut studio = Studio::new_inner(
            std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()),
            false,
        );
        for (at, bytes) in [
            (Instant::now(), vec![2, 0, 12, 250, 0]),
            (Instant::now(), vec![2, 0, 3, 2, 0]),
            (Instant::now(), vec![2, 0, 0, 0, 4]), // Wheel is not rotation.
            (Instant::now(), vec![2, 0, 99]),      // Invalid report.
            (
                Instant::now() - Duration::from_secs(1),
                vec![2, 0, 99, 99, 0],
            ),
        ] {
            studio.tx.send(Event::Packet { at, bytes }).unwrap();
        }
        studio.poll_with_trust(false);
        assert_eq!(studio.take_view_motion(), (15, -4));
        assert_eq!(studio.take_view_motion(), (0, 0));
    }

    #[test]
    fn view_motion_is_discarded_on_exit_and_reader_restart() {
        for boundary in [
            Event::Packet {
                at: Instant::now(),
                bytes: vec![1, 0, 0, 0x52, 0, 0, 0, 0, 0],
            },
            Event::HidUnavailable {
                present: false,
                message: "Disconnected".into(),
            },
            Event::HidReady { exclusive: false },
            Event::HidStopped,
        ] {
            let mut studio = Studio::new_inner(
                std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()),
                false,
            );
            studio
                .tx
                .send(Event::Packet {
                    at: Instant::now(),
                    bytes: vec![2, 0, 20, 10, 0],
                })
                .unwrap();
            studio.tx.send(boundary).unwrap();
            studio.poll_with_trust(false);
            assert_eq!(studio.take_view_motion(), (0, 0));
        }
    }

    #[test]
    fn builtin_map_routes_each_button_and_keeps_custom_sources() {
        let mut c = Config::default();
        c.validate().unwrap();
        for b in &mut c.buttons {
            b.normal = Action::System {
                command: SystemAction::MissionControl,
            };
            b.air_mouse = b.normal.clone();
        }
        for b in &c.buttons {
            for source in &b.normal_sources {
                let (packet, profile) = match source {
                    InputSource::Consumer(k) => {
                        (vec![3, *k as u8, (*k >> 8) as u8], Profile::Normal)
                    }
                    InputSource::Keyboard(k) => (vec![1, 0, 0, *k, 0, 0, 0, 0, 0], Profile::Normal),
                    InputSource::Mouse(k) => (vec![2, 1 << (k - 1), 0, 0, 0], Profile::AirMouse),
                };
                let result = crate::direct::Router::default().process(&packet, Instant::now(), &c);
                assert!(
                    result.contains(&crate::direct::Output::Action(b.button, profile)),
                    "{:?}",
                    b.button
                );
            }
        }
        let mut old = Config::default();
        for b in &mut old.buttons {
            b.normal_sources.clear();
            b.mouse_sources.clear();
        }
        old.buttons[0].normal_sources = vec![InputSource::Consumer(0x6b)];
        old.buttons[0].normal = Action::Disabled;
        for button in ButtonId::ALL {
            old.fill_mr01_sources(button);
        }
        old.validate().unwrap();
        assert_eq!(
            old.buttons[0].normal_sources,
            vec![InputSource::Consumer(0x6b)]
        );
        assert_eq!(old.buttons[0].normal, Action::Disabled);
        assert!(
            old.buttons[2].normal_sources.is_empty(),
            "Never steal a learned source"
        );
        assert!(
            old.buttons[10].normal_sources.is_empty(),
            "Never fabricate a mode-button event"
        );
    }
    #[test]
    fn mr01_apps_assignment_routes_without_learning_or_reconnecting() {
        let root = std::env::temp_dir().join(format!("mr01-builtin-{}", uuid::Uuid::new_v4()));
        let mut legacy = Config::default();
        for b in &mut legacy.buttons {
            b.normal_sources.clear();
            b.mouse_sources.clear();
        }
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("config.json"),
            serde_json::to_vec(&legacy).unwrap(),
        )
        .unwrap();
        let mut s = Studio::new_inner(root.clone(), false);
        s.listening = true;
        s.capture_service = true;
        s.capture_access = crate::permissions::Access::Granted;
        s.poll_with_trust(true);
        s.assign_action(
            ButtonId::Apps,
            Profile::Normal,
            Action::System {
                command: SystemAction::ShowDesktop,
            },
        )
        .unwrap();
        assert!(
            s.learning.is_none(),
            "Selecting an MR-01 function must not ask for learning"
        );
        assert!(s.actions_enabled);
        let mut router = crate::direct::Router::default();
        assert_eq!(
            router.process(&[3, 0x6b, 0], Instant::now(), &s.config),
            vec![crate::direct::Output::Action(
                ButtonId::Apps,
                Profile::Normal
            )]
        );
        drop(s);
        let restored = Studio::new_inner(root.clone(), false);
        assert!(restored.button_identified(ButtonId::Apps, Profile::Normal));
        drop(restored);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn on_screen_keyboard_preset_saves_and_reloads_without_execution() {
        let action: Action =
            serde_json::from_str(r#"{"type":"system","command":"on_screen_keyboard"}"#).unwrap();
        assert!(
            Action::presets()
                .iter()
                .any(|(_, preset)| preset == &action)
        );
        crate::actions::validate_definition(&action).unwrap();
        let root = std::env::temp_dir().join(format!("aca-keyboard-{}", uuid::Uuid::new_v4()));
        let mut studio = Studio::new_inner(root.clone(), false);
        studio
            .save_action(ButtonId::Capture, Profile::Normal, action.clone())
            .unwrap();
        let restored = Studio::new_inner(root.clone(), false);
        assert_eq!(restored.action(ButtonId::Capture, Profile::Normal), &action);
        assert_eq!(
            restored.action(ButtonId::Capture, Profile::AirMouse),
            studio.action(ButtonId::Capture, Profile::AirMouse)
        );
        assert!(!studio.actions_enabled);
        drop(restored);
        drop(studio);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    #[cfg(feature = "virtual-keyboard")]
    fn keyboard_position_save_keeps_macro_and_session_generations() {
        let root = std::env::temp_dir().join(format!("aca-position-{}", uuid::Uuid::new_v4()));
        let mut studio = Studio::new_inner(root.clone(), false);
        studio.save_config(studio.config.clone()).unwrap();
        let control = studio.keyboard_control.clone();
        let generation = control
            .toggle(Some(ButtonId::Home), Instant::now())
            .unwrap();
        assert!(control.acknowledge_open(generation));
        let generation = control
            .toggle(Some(ButtonId::Home), Instant::now())
            .unwrap();
        control.acknowledge_close(generation);
        let epoch = studio.action_generation.load(Ordering::Relaxed);
        let position = crate::keyboard_model::KeyboardPosition { x: 120., y: 240. };
        studio.save_keyboard_position(position).unwrap();
        assert_eq!(studio.action_generation.load(Ordering::Relaxed), epoch);
        assert_eq!(control.snapshot().generation, generation);
        let restored = Studio::new_inner(root.clone(), false);
        assert_eq!(restored.config.keyboard.position, Some(position));
        drop(restored);
        let mut next = studio.config.clone();
        next.keyboard.scale = 1.1;
        studio.save_config(next).unwrap();
        assert_ne!(studio.action_generation.load(Ordering::Relaxed), epoch);
        assert_ne!(control.snapshot().generation, generation);
        drop(studio);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn new_button_action_saves_offline_and_failed_edit_keeps_previous_action() {
        let root = std::env::temp_dir().join(format!("mr01-save-{}", uuid::Uuid::new_v4()));
        let mut studio = Studio::new_inner(root.clone(), false);
        studio
            .save_action(ButtonId::Home, Profile::Normal, Action::Disabled)
            .unwrap();
        assert!(!studio.actions_enabled);
        assert!(studio.button_identified(ButtonId::Home, Profile::Normal));
        let mut restored = Studio::new_inner(root.clone(), false);
        assert_eq!(
            restored.action(ButtonId::Home, Profile::Normal),
            &Action::Disabled
        );
        std::fs::write(studio.config_path(), b"external edit").unwrap();
        assert!(
            restored
                .save_action(ButtonId::Home, Profile::Normal, Action::Default)
                .is_err()
        );
        assert_eq!(
            restored.action(ButtonId::Home, Profile::Normal),
            &Action::Disabled
        );
        assert_eq!(
            std::fs::read(studio.config_path()).unwrap(),
            b"external edit"
        );
        let _ = std::fs::remove_dir_all(root);
    }
    #[test]
    fn failed_learning_save_preserves_previous_source_and_reports_error() {
        let root =
            std::env::temp_dir().join(format!("mr01-learn-conflict-{}", uuid::Uuid::new_v4()));
        let mut studio = Studio::new_inner(root.clone(), false);
        studio.save_config(studio.config.clone()).unwrap();
        studio.listening = true;
        studio.learn(ButtonId::Home, Profile::Normal);
        std::fs::write(studio.config_path(), b"external edit").unwrap();
        studio.packet(Instant::now(), &[3, 0x23, 2]);
        assert!(studio.button_identified(ButtonId::Home, Profile::Normal));
        assert!(matches!(studio.learn_result, Some(Err(_))));
        assert!(studio.learning.is_none());
        let _ = std::fs::remove_dir_all(root);
    }
    #[test]
    fn startup_and_reconnect_enable_actions_but_manual_pause_persists() {
        let root = std::env::temp_dir().join(format!("mr01-auto-{}", uuid::Uuid::new_v4()));
        let mut s = Studio::new_inner(root, false);
        assert!(s.actions_requested);
        s.tx.send(Event::CaptureService(true)).unwrap();
        s.tx.send(Event::CapturePermission(
            crate::permissions::Access::Granted,
        ))
        .unwrap();
        s.tx.send(Event::HidReady { exclusive: false }).unwrap();
        s.poll_with_trust(true);
        assert!(s.actions_enabled);
        s.set_actions_enabled(false);
        s.tx.send(Event::HidReady { exclusive: false }).unwrap();
        s.poll_with_trust(true);
        assert!(!s.actions_enabled);
        s.set_actions_enabled(true);
        s.poll_with_trust(false);
        assert!(!s.actions_enabled, "No activation without Accessibility");
        s.poll_with_trust(true);
        assert!(s.actions_enabled);
        s.tx.send(Event::HidUnavailable {
            present: false,
            message: "Disconnected".into(),
        })
        .unwrap();
        s.poll_with_trust(true);
        assert!(!s.actions_enabled);
        s.tx.send(Event::HidReady { exclusive: false }).unwrap();
        s.poll_with_trust(true);
        assert!(s.actions_enabled);
    }
    #[test]
    fn capture_error_stops_automatic_retry_until_user_requests_it() {
        let mut s = Studio::new_inner(
            std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()),
            false,
        );
        s.capture_service = true;
        s.capture_access = crate::permissions::Access::Granted;
        s.listening = true;
        s.poll_with_trust(true);
        s.tx.send(Event::HidUnavailable {
            present: true,
            message: "Exclusive denied".into(),
        })
        .unwrap();
        s.tx.send(Event::HidReady { exclusive: false }).unwrap();
        s.poll_with_trust(true);
        assert!(!s.actions_enabled);
        assert!(s.engine_error.is_some());
        s.poll_with_trust(true);
        assert!(!s.actions_enabled);
        s.set_actions_enabled(true);
        s.poll_with_trust(true);
        assert!(s.actions_enabled);
    }
    #[test]
    fn identifying_does_not_require_manual_reactivation() {
        let root = std::env::temp_dir().join(format!("mr01-resume-{}", uuid::Uuid::new_v4()));
        let mut studio = Studio::new_inner(root.clone(), false);
        studio.listening = true;
        studio.capture_service = true;
        studio.capture_access = crate::permissions::Access::Granted;
        studio.actions_enabled = true;
        studio.learn(ButtonId::Home, Profile::Normal);
        studio.packet(Instant::now(), &[3, 0x23, 2]);
        studio.poll_with_trust(true);
        assert!(
            !studio.actions_enabled,
            "Learning press must not execute its action while held"
        );
        studio.packet(Instant::now(), &[3, 0, 0]);
        studio.poll_with_trust(true);
        assert!(
            studio.actions_enabled,
            "Identifying a button must resume the global engine"
        );
        let _ = std::fs::remove_dir_all(root);
    }
    #[test]
    fn activation_reports_only_the_missing_requirement() {
        let root = std::env::temp_dir().join(format!("mr01-status-{}", uuid::Uuid::new_v4()));
        let mut studio = Studio::new_inner(root, false);
        studio.listening = true;
        let error = studio.activation_error_with_accessibility(true).unwrap();
        assert!(error.contains("action reader is not active"));
        assert!(!error.contains("Accessibility"));
        studio.capture_service = true;
        studio.capture_access = crate::permissions::Access::Granted;
        assert!(
            studio
                .activation_error_with_accessibility(false)
                .unwrap()
                .contains("Accessibility")
        );
        assert!(studio.activation_error_with_accessibility(true).is_none());
    }
    #[test]
    fn learning_a_new_button_persists_without_capture_service() {
        let root = std::env::temp_dir().join(format!("mr01-learn-{}", uuid::Uuid::new_v4()));
        let mut studio = Studio::new_inner(root.clone(), false);
        studio.listening = true;
        studio.learn(ButtonId::Home, Profile::Normal);
        studio.packet(Instant::now(), &[3, 0x23, 2]);
        let saved = std::fs::read(studio.config_path());
        let _ = std::fs::remove_dir_all(root);
        let saved: Config =
            serde_json::from_slice(&saved.expect("Learning must save automatically")).unwrap();
        assert_eq!(
            saved
                .buttons
                .iter()
                .find(|b| b.button == ButtonId::Home)
                .unwrap()
                .normal_sources,
            vec![InputSource::Consumer(0x223)]
        );
        assert!(!studio.actions_enabled);
    }
    #[test]
    fn observed_mode_does_not_toggle_on_other_buttons_or_silence() {
        assert_eq!(mode_evidence(&[2, 0, 0, 0, 0]), Some(MouseMode::Active));
        assert_eq!(
            mode_evidence(&[1, 0, 0, 0x52, 0, 0, 0, 0, 0]),
            Some(MouseMode::Normal)
        );
        assert_eq!(mode_evidence(&[1, 0, 0, 0x28, 0, 0, 0, 0, 0]), None);
        assert_eq!(mode_evidence(&[3, 0xe9, 0]), None);
        assert_eq!(mode_evidence(&[2, 0]), None);
        assert_eq!(mode_evidence(&[2, 0xff, 0, 0, 0]), None);
        assert_eq!(mode_evidence(&[]), None);
    }
    #[test]
    fn config_conflicts_include_mouse_fallback() {
        let mut c = Config::default();
        assert!(c.validate().is_ok());
        c.buttons
            .iter_mut()
            .find(|b| b.button == ButtonId::Capture)
            .unwrap()
            .mouse_sources = vec![InputSource::Keyboard(0x52)];
        assert!(c.validate().is_err());
    }
    #[test]
    fn future_version_and_missing_button_rejected() {
        let mut c = Config::default();
        c.version = Config::VERSION + 1;
        assert!(c.validate().is_err());
        c.version = Config::VERSION;
        c.buttons.pop();
        assert!(c.validate().is_err());
    }
    #[test]
    fn oversized_recursive_macro_rejected() {
        let mut a = Action::Default;
        for _ in 0..6 {
            a = Action::Macro { steps: vec![a] };
        }
        assert!(validate_action(&a, 0).is_err());
    }
    #[test]
    fn assignments_reject_actions_that_executor_cannot_run() {
        for action in [
            Action::Shortcut {
                key: "not_a_key".into(),
                modifiers: vec![],
            },
            Action::Shortcut {
                key: "a".into(),
                modifiers: vec!["not_a_modifier".into()],
            },
            Action::Macro {
                steps: vec![
                    Action::Macro {
                        steps: vec![Action::Default; 32]
                    };
                    3
                ],
            },
        ] {
            let mut config = Config::default();
            config.buttons[0].normal = action;
            assert!(
                config.validate().is_err(),
                "Unexecutable assignment was accepted"
            );
        }
    }
    #[test]
    fn page_presets_are_assignable_and_keep_custom_pixel_scroll() {
        let presets = Action::presets();
        for key in ["page_up", "page_down"] {
            assert!(
                presets.iter().any(|(_, a)| *a
                    == Action::Shortcut {
                        key: key.into(),
                        modifiers: vec![]
                    }),
                "Missing {key}"
            );
        }
        assert!(
            !presets
                .iter()
                .any(|(_, a)| matches!(a, Action::Scroll { .. })),
            "Tiny pixel scroll should not be an ordinary preset"
        );
        for action in presets
            .into_iter()
            .map(|(_, a)| a)
            .chain([Action::Scroll { x: 3, y: -3 }])
        {
            let mut config = Config::default();
            config.buttons[0].normal = action;
            assert!(config.validate().is_ok());
        }
    }
    #[test]
    fn configuration_roundtrips() {
        let c = Config::default();
        let bytes = serde_json::to_vec(&c).unwrap();
        let decoded: Config = serde_json::from_slice(&bytes).unwrap();
        assert!(decoded.validate().is_ok());
        assert_eq!(
            decoded.buttons[4].normal_sources,
            vec![InputSource::Keyboard(0x52)]
        );
    }
}

// Append to src/studio.rs in an ISOLATED COPY ONLY. No hardware/UI APIs used.
#[cfg(test)]
mod review_diagnostics {
    use super::*;

    #[test]
    fn review_queued_press_must_expire_before_execution() {
        let root = std::env::temp_dir().join(format!("rfa-review-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let marker = root.join("late-action");
        let mut s = Studio::new_inner(root.clone(), false);
        s.config.buttons[0].normal = Action::Shell {
            command: "/bin/sleep 1".into(),
        };
        s.config.buttons[1].normal = Action::Shell {
            command: format!("printf ran > '{}'", marker.display()),
        };
        s.listening = true;
        s.capture_service = true;
        s.capture_access = crate::permissions::Access::Granted;
        s.poll_with_trust(true);
        let epoch = s.action_generation.load(Ordering::Relaxed);
        for button in [ButtonId::Home, ButtonId::Experiences] {
            s.tx.send(Event::DirectAction {
                #[cfg(feature = "virtual-keyboard")]
                keyboard_generation: None,
                button,
                profile: Profile::Normal,
                epoch,
                at: Instant::now(),
            })
            .unwrap();
        }
        s.poll_with_trust(true);
        s.work_rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .1
            .unwrap();
        let _ = s.work_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let ran = marker.exists();
        drop(s);
        std::fs::remove_dir_all(root).unwrap();
        assert!(
            !ran,
            "Press queued >1 second still executed, despite the 500 ms freshness gate"
        );
    }

    #[test]
    fn closing_studio_waits_for_running_action_cleanup() {
        let root = std::env::temp_dir().join(format!("air-close-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let marker = root.join("pid");
        let mut studio = Studio::new_inner(root.clone(), false);
        studio.config.buttons[0].normal = Action::Shell {
            command: format!("echo $$ > '{}'; sleep 10", marker.display()),
        };
        studio.test_action(ButtonId::Home, Profile::Normal).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        let pid: i32 = loop {
            if let Some(pid) = std::fs::read_to_string(&marker)
                .ok()
                .and_then(|s| s.trim().parse().ok())
            {
                break pid;
            }
            assert!(Instant::now() < deadline, "action did not publish its PID");
            std::thread::sleep(Duration::from_millis(5));
        };
        drop(studio);
        unsafe extern "C" {
            fn kill(pid: i32, signal: i32) -> i32;
        }
        let alive = unsafe { kill(pid, 0) } == 0;
        std::fs::remove_dir_all(root).unwrap();
        assert!(!alive, "Studio exited before its action process was reaped");
    }

    #[test]
    fn learning_rejects_malformed_mouse_packets_like_router() {
        let root = std::env::temp_dir().join(format!("air-invalid-{}", uuid::Uuid::new_v4()));
        let mut s = Studio::new_inner(root.clone(), false);
        s.listening = true;
        s.learn(ButtonId::Home, Profile::Normal);
        s.packet(Instant::now(), &[2, 0x81, 0, 0, 0]);
        assert_eq!(s.learning, Some((ButtonId::Home, Profile::Normal)));
        assert!(!s.config_path().exists());
        drop(s);
        let _ = std::fs::remove_dir_all(root);
    }
    #[test]
    fn review_reconnect_must_clear_old_learning_release_gate() {
        let root = std::env::temp_dir().join(format!("rfa-review-{}", uuid::Uuid::new_v4()));
        let mut s = Studio::new_inner(root.clone(), false);
        s.listening = true;
        s.capture_service = true;
        s.capture_access = crate::permissions::Access::Granted;
        s.poll_with_trust(true);
        s.learn(ButtonId::Home, Profile::Normal);
        s.packet(Instant::now(), &[3, 0x23, 2]);
        assert!(s.learning.is_none());
        assert!(s.awaiting_release);
        s.tx.send(Event::HidUnavailable {
            present: true,
            message: "fixture reconnect after read error".into(),
        })
        .unwrap();
        s.tx.send(Event::HidReady { exclusive: false }).unwrap();
        s.poll_with_trust(true);
        assert!(s.presses.is_released());
        let active = s.actions_enabled;
        drop(s);
        std::fs::remove_dir_all(root).unwrap();
        assert!(
            active,
            "Reconnected with fresh released state, but old awaiting_release still pauses actions"
        );
    }
}
