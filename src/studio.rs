//! Device-independent configuration and a bounded coordinator for the desktop app.
use crate::{
    audio,
    hardware::{self, Event},
    voice::Presses,
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
            Self::Home => "Inicio",
            Self::Experiences => "Experiencias",
            Self::Apps => "Aplicaciones",
            Self::Back => "Atrás",
            Self::Up => "Arriba",
            Self::Down => "Abajo",
            Self::Left => "Izquierda",
            Self::Right => "Derecha",
            Self::Ok => "OK",
            Self::Capture => "Captura",
            Self::MouseMode => "Air mouse",
            Self::VolumeDown => "Volumen −",
            Self::VolumeUp => "Volumen +",
            Self::Mute => "Silencio",
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
}
impl Profile {
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::AirMouse => "Air mouse",
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::AirMouse => "air_mouse",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseMode {
    Normal,
    Active,
}
impl MouseMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Active => "Air mouse activo",
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
            Self::Keyboard(n) => write!(f, "Tecla HID {n:02X}"),
            Self::Consumer(n) => write!(f, "Multimedia {n:04X}"),
            Self::Mouse(n) => write!(f, "Botón mouse {n}"),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum Action {
    Default,
    Disabled,
    Shortcut { key: String, modifiers: Vec<String> },
    System { command: SystemAction },
    Scroll { x: i32, y: i32 },
    AppleScript { script: String },
    Shell { command: String },
    Macro { steps: Vec<Action> },
    VoiceToggle,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemAction {
    MissionControl,
    AppExpose,
    ShowDesktop,
    Launchpad,
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
            Self::AppExpose => "Ventanas de la aplicación",
            Self::ShowDesktop => "Mostrar escritorio",
            Self::Launchpad => "Aplicaciones",
            Self::VolumeUp => "Subir volumen",
            Self::VolumeDown => "Bajar volumen",
            Self::Mute => "Silenciar",
            Self::ZoomIn => "Acercar zoom",
            Self::ZoomOut => "Alejar zoom",
            Self::ZoomToggle => "Activar / desactivar zoom de pantalla",
            Self::DisplaySleep => "Apagar pantalla",
            Self::Sleep => "Suspender Mac",
            Self::LockScreen => "Bloquear pantalla",
        }
    }
}
impl Action {
    pub fn label(&self) -> String {
        match self {
            Self::Default => "Función original".into(),
            Self::Disabled => "Desactivado".into(),
            Self::Shortcut { key, modifiers } => format!("{} {key}", modifiers.join(" + ")),
            Self::System { command } => command.label().into(),
            Self::Scroll { x, y } => format!("Scroll {x}, {y}"),
            Self::AppleScript { .. } => "AppleScript".into(),
            Self::Shell { .. } => "Comando personalizado".into(),
            Self::Macro { steps } => format!("Macro · {} pasos", steps.len()),
            Self::VoiceToggle => "Grabar / transcribir voz".into(),
        }
    }
    pub fn presets() -> Vec<(String, Self)> {
        let mut a = vec![Self::Default, Self::Disabled, Self::VoiceToggle];
        for command in [
            SystemAction::MissionControl,
            SystemAction::AppExpose,
            SystemAction::ShowDesktop,
            SystemAction::Launchpad,
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
        a.into_iter().map(|a| (a.label(), a)).collect()
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TranscriptionCommand {
    pub program: String,
    pub args: Vec<String>,
    pub insert_text: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ButtonMapping {
    pub button: ButtonId,
    pub normal: Action,
    pub air_mouse: Action,
    pub normal_sources: Vec<InputSource>,
    pub mouse_sources: Vec<InputSource>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    pub buttons: Vec<ButtonMapping>,
    pub audio_input: Option<String>,
    pub transcription: Option<TranscriptionCommand>,
    pub mouse_idle_ms: u64,
}
// MR-01 device map. This app's capture is restricted to VID/PID/name MR-01.
// Provenance and remaining physical checks: docs/mr01-button-map.md.
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
            version: 1,
            buttons: ButtonId::ALL
                .into_iter()
                .map(|button| ButtonMapping {
                    button,
                    normal: Action::Default,
                    air_mouse: Action::Default,
                    normal_sources: mr01_sources(button),
                    mouse_sources: vec![],
                })
                .collect(),
            audio_input: None,
            transcription: None,
            mouse_idle_ms: 1500,
        }
    }
}
impl Config {
    /// Upgrade old empty source lists without overwriting learned mappings or
    /// borrowing a source explicitly owned by another button in either profile.
    fn fill_mr01_sources(&mut self) {
        let mut claimed: BTreeSet<_> = self
            .buttons
            .iter()
            .flat_map(|b| b.normal_sources.iter().chain(&b.mouse_sources))
            .cloned()
            .collect();
        for b in &mut self.buttons {
            if b.normal_sources.is_empty() {
                b.normal_sources = mr01_sources(b.button)
                    .into_iter()
                    .filter(|s| claimed.insert(s.clone()))
                    .collect();
            }
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err(format!(
                "Versión de configuración no soportada: {}",
                self.version
            ));
        }
        if !(500..=30000).contains(&self.mouse_idle_ms) {
            return Err("mouse_idle_ms debe estar entre 500 y 30000".into());
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
                "La configuración debe contener exactamente los 14 botones, sin duplicados".into(),
            );
        }
        for profile in [Profile::Normal, Profile::AirMouse] {
            let mut seen = BTreeSet::new();
            for b in &self.buttons {
                validate_action(
                    if profile == Profile::Normal {
                        &b.normal
                    } else {
                        &b.air_mouse
                    },
                    0,
                )?;
                for s in sources(b, profile) {
                    let valid = match s {
                        InputSource::Keyboard(n) => (4..=0xe7).contains(n) || *n == 0xf1,
                        InputSource::Consumer(n) => (1..=0x3ff).contains(n),
                        InputSource::Mouse(n) => (1..=5).contains(n),
                    };
                    if !valid || !seen.insert(s.clone()) {
                        return Err(format!(
                            "Señal inválida o repetida en {}: {s}",
                            profile.label()
                        ));
                    }
                }
            }
        }
        if let Some(t) = &self.transcription {
            if t.program.trim().is_empty() || !t.args.iter().any(|a| a.contains("{audio}")) {
                return Err("Transcripción: indicar programa y argumento {audio}".into());
            }
        }
        Ok(())
    }
}
pub fn sources(b: &ButtonMapping, p: Profile) -> &[InputSource] {
    if p == Profile::AirMouse && !b.mouse_sources.is_empty() {
        &b.mouse_sources
    } else {
        &b.normal_sources
    }
}
fn validate_action(a: &Action, depth: usize) -> Result<(), String> {
    if depth == 0 {
        crate::actions::validate_definition(a)?;
    }
    if depth > 4 {
        return Err("Macro demasiado anidada (máximo 4 niveles)".into());
    }
    match a {
        Action::Macro { steps } => {
            if steps.is_empty() || steps.len() > 32 {
                return Err("Una macro admite 1–32 pasos".into());
            }
            for s in steps {
                if matches!(s, Action::VoiceToggle) {
                    return Err("La grabación debe asignarse fuera de macros".into());
                }
                validate_action(s, depth + 1)?;
            }
        }
        Action::Shortcut { key, modifiers } => {
            if key.is_empty() || key.len() > 64 || modifiers.len() > 4 {
                return Err("Atajo inválido".into());
            }
        }
        Action::Shell { command } | Action::AppleScript { script: command } => {
            if command.trim().is_empty() || command.len() > 16384 {
                return Err("El script debe contener entre 1 y 16384 bytes".into());
            }
        }
        Action::Scroll { x, y } if x.unsigned_abs() > 100 || y.unsigned_abs() > 100 => {
            return Err("Scroll fuera de rango (±100)".into());
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
    use std::io::Write;
    let parent = path.parent().ok_or("Ruta sin directorio")?;
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
        if path.exists() {
            std::fs::copy(path, path.with_extension("previous.json")).map_err(|e| e.to_string())?;
        }
        std::fs::rename(&temp, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result
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
    device_choice: hardware::DeviceChoice,
    reader_thread: Option<std::thread::JoinHandle<()>>,
    pub packet_count: u64,
    pub last_packet: String,
    pub mode: MouseMode,
    pub learning: Option<(ButtonId, Profile)>,
    pub learn_result: Option<Result<String, String>>,
    pub last_button: Option<ButtonId>,
    pub audio_inputs: Vec<String>,
    pub audio_refreshing: bool,
    pub recording: bool,
    pub audio_status: String,
    pub actions_enabled: bool,
    pub actions_requested: bool,
    pub engine_error: Option<String>,
    pub reader_error: Option<String>,
    pub last_action_result: Option<(String, bool)>,
    launcher: crate::session::Launcher,
    auto_launch: bool,
    awaiting_release: bool,
    pub status: String,
    root: PathBuf,
    ble_session: bool,
    tx: hardware::Events,
    rx: Receiver<Event>,
    stop_hid: Arc<AtomicBool>,
    stop_audio: Arc<AtomicBool>,
    presses: Presses,
    mouse_buttons: u8,
    last_mouse: Option<Instant>,
    learn_started: Option<Instant>,
    config_error: bool,
    disk_config: Option<Vec<u8>>,
    instance: Option<crate::instance::Server>,
    work_tx: Option<mpsc::SyncSender<(Action, u64, Instant)>>,
    work_thread: Option<std::thread::JoinHandle<()>>,
    work_rx: Receiver<Result<String, String>>,
    transcript_rx: Receiver<Result<(String, bool, Option<String>, u64), String>>,
    transcript_tx: mpsc::Sender<Result<(String, bool, Option<String>, u64), String>>,
    voice_for_transcription: bool,
    voice_target: Option<String>,
    recording_epoch: u64,
    recording_transcriber: Option<TranscriptionCommand>,
    transcribing: bool,
    transcript_thread: Option<std::thread::JoinHandle<()>>,
    action_generation: Arc<std::sync::atomic::AtomicU64>,
}
impl Studio {
    pub fn new(root: PathBuf) -> Self {
        Self::new_inner(root, true)
    }
    fn new_inner(root: PathBuf, start_io: bool) -> Self {
        let (tx, rx) = mpsc::sync_channel(4096);
        let (work_tx, work_req) = mpsc::sync_channel::<(Action, u64, Instant)>(16);
        let (done_tx, work_rx) = mpsc::channel();
        let action_generation = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let generation = action_generation.clone();
        let work_thread = std::thread::spawn(move || {
            while let Ok((a, epoch, at)) = work_req.recv() {
                if generation.load(Ordering::Relaxed) == epoch {
                    let result = if at.elapsed() >= Duration::from_millis(500) {
                        Err("Pulsación vencida en la cola; descartada".into())
                    } else {
                        crate::actions::execute_cancellable(&a, &generation, epoch)
                    }
                    .map(|message| format!("{}: {message}", a.label()))
                    .map_err(|error| format!("{}: {error}", a.label()));
                    let _ = done_tx.send(result);
                }
            }
        });
        let (transcript_tx, transcript_rx) = mpsc::channel();
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
            bluetooth_state: "Sin consultar; Buscar comprueba el adaptador".into(),
            candidates: vec![],
            reader_running: false,
            direct_mapping: Arc::new(std::sync::RwLock::new(None)),
            device_choice: Arc::new(std::sync::RwLock::new(None)),
            reader_thread: None,
            packet_count: 0,
            last_packet: "Sin paquetes".into(),
            mode: MouseMode::Normal,
            learning: None,
            learn_result: None,
            last_button: None,
            audio_inputs: vec![],
            audio_refreshing: false,
            recording: false,
            audio_status: "Micrófono inactivo".into(),
            actions_enabled: false,
            actions_requested: true,
            engine_error: None,
            reader_error: None,
            last_action_result: None,
            launcher: Default::default(),
            auto_launch: start_io,
            awaiting_release: false,
            status: "Conectá tu control para comenzar".into(),
            root,
            ble_session: false,
            tx,
            rx,
            stop_hid: Arc::new(AtomicBool::new(false)),
            stop_audio: Arc::new(AtomicBool::new(false)),
            presses: Presses::default(),
            mouse_buttons: 0,
            last_mouse: None,
            learn_started: None,
            config_error: false,
            disk_config: None,
            instance: None,
            work_tx: Some(work_tx),
            work_thread: Some(work_thread),
            work_rx,
            transcript_rx,
            transcript_tx,
            voice_for_transcription: false,
            voice_target: None,
            recording_epoch: 0,
            recording_transcriber: None,
            transcribing: false,
            transcript_thread: None,
            action_generation,
        };
        if let Err(e) = s.reload() {
            s.config_error = true;
            s.log("config", e);
        }
        if !start_io {
            return s;
        }
        s.refresh_audio_inputs();
        match crate::instance::Server::start(&s.root) {
            Ok(server) => s.instance = Some(server),
            Err(e) => s.log("motor", e),
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
    pub fn save_config(&mut self, mut next: Config) -> Result<(), String> {
        if self.config_error {
            return Err("El JSON existente es inválido; corregilo y recargá antes de guardar. No se sobrescribió.".into());
        }
        next.validate()?;
        next.fill_mr01_sources();
        next.validate()?;
        let disk = match std::fs::read(self.config_path()) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.to_string()),
        };
        if disk != self.disk_config {
            self.suspend_actions();
            self.engine_error =
                Some("config.json cambió fuera de la app. Recargá el archivo.".into());
            return Err("config.json cambió fuera de la app. Recargá antes de guardar para conservar esos cambios.".into());
        }
        atomic_json(&self.config_path(), &next)?;
        self.config = next;
        self.action_generation.fetch_add(1, Ordering::Relaxed);
        self.publish_mapping();
        self.disk_config =
            Some(serde_json::to_vec_pretty(&self.config).map_err(|e| e.to_string())?);
        self.log("config", "Configuración guardada");
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
        let mut config = match &disk {
            Some(bytes) => serde_json::from_slice::<Config>(bytes).map_err(|e| e.to_string())?,
            None => Config::default(),
        };
        config.validate()?;
        config.fill_mr01_sources();
        config.validate()?;
        self.config = config;
        self.disk_config = disk;
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
            return Err("Air mouse no emite una pulsación asignable. Editá las acciones del perfil Air mouse para los otros botones.".into());
        }
        let mut next = self.config.clone();
        let b = next
            .buttons
            .iter_mut()
            .find(|b| b.button == button)
            .ok_or("Botón no encontrado")?;
        if profile == Profile::Normal {
            b.normal = action
        } else {
            b.air_mouse = action
        };
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
            return Some("Terminá de identificar el botón antes de activar las acciones.".into());
        }
        if self.config_error {
            return Some("Corregí el archivo de configuración y recargalo. Tus cambios no fueron sobrescritos.".into());
        }
        if let Err(e) = self.config.validate() {
            return Some(e);
        }
        if !self.listening {
            return Some(if self.connected {
                "El control está conectado pero Studio no puede leerlo. Revisá el detalle en Conexión.".into()
            } else {
                "No se detecta el control. Despertalo o conectalo desde Conexión.".into()
            });
        }
        if !self.capture_service {
            return Some("El lector de acciones no está activo (las sesiones duran hasta 30 minutos). Iniciá el lector y luego activá las acciones. Tu configuración sigue guardada.".into());
        }
        if !trusted {
            return Some("Falta Accesibilidad para Studio en Ajustes → Privacidad y seguridad. La configuración se puede guardar igual.".into());
        }
        None
    }
    pub fn action(&self, button: ButtonId, profile: Profile) -> &Action {
        self.config
            .buttons
            .iter()
            .find(|b| b.button == button)
            .map(|b| {
                if profile == Profile::Normal {
                    &b.normal
                } else {
                    &b.air_mouse
                }
            })
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
            "Sin identificar · aprender botón".into()
        } else {
            s.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        }
    }
    pub fn connect(&mut self) {
        if self.connected || self.connecting {
            return;
        }
        if self.ble_session {
            self.log("Bluetooth", "La conexión Bluetooth ya está abierta. Usá Iniciar lectura para volver a leer el control.");
            return;
        }
        self.connecting = true;
        self.ble_session = true;
        self.candidates.clear();
        hardware::discover_and_connect(self.tx.clone(), self.root.clone(), None);
    }
    pub fn connect_candidate(&mut self, id: String) {
        if let Some(path) = id.strip_prefix("hid:") {
            if !self.listening {
                *self.device_choice.write().unwrap() = Some(path.to_string());
                self.candidates.clear();
            }
            return;
        }
        if self.connected || self.connecting {
            return;
        }
        self.connecting = true;
        self.ble_session = true;
        self.candidates.clear();
        hardware::discover_and_connect(self.tx.clone(), self.root.clone(), Some(id));
    }
    pub fn start_listening(&mut self) {
        if self.reader_running {
            return;
        }
        self.reader_running = true;
        self.stop_hid = Arc::new(AtomicBool::new(false));
        self.reader_thread = Some(hardware::listen(
            self.tx.clone(),
            self.stop_hid.clone(),
            self.root.clone(),
            self.direct_mapping.clone(),
            self.device_choice.clone(),
        ));
    }
    pub fn stop_listening(&mut self) {
        self.stop_hid.store(true, Ordering::Relaxed);
        self.suspend_actions();
        self.mode = MouseMode::Normal;
        self.last_mouse = None;
        self.listening = false;
        self.native_active = false;
        self.cancel_learning();
        self.stop_audio.store(true, Ordering::Relaxed);
    }
    pub fn learn(&mut self, button: ButtonId, profile: Profile) {
        self.learn_result = None;
        self.awaiting_release = true;
        self.suspend_actions();
        if button == ButtonId::MouseMode {
            self.log("aprender","El botón Air mouse no emitió una señal propia en las pruebas. Detectamos su actividad por reportes de mouse.");
            return;
        }
        if !self.listening {
            self.start_listening();
        }
        self.learning = Some((button, profile));
        self.learn_started = Some(Instant::now());
        self.log(
            "aprender",
            format!("Pulsá {} una vez · {}", button.label(), profile.label()),
        );
    }
    pub fn cancel_learning(&mut self) {
        self.learning = None;
        self.learn_started = None;
        self.awaiting_release = !self.presses.is_released() || self.mouse_buttons != 0;
    }
    pub fn set_actions_enabled(&mut self, enabled: bool) {
        self.actions_requested = enabled;
        self.engine_error = None;
        if !enabled {
            self.suspend_actions();
        }
    }
    fn suspend_actions(&mut self) {
        self.apply_actions(false);
    }
    fn apply_actions(&mut self, enabled: bool) {
        if !enabled {
            self.action_generation.fetch_add(1, Ordering::Relaxed);
            if self.recording && self.voice_for_transcription {
                self.stop_audio.store(true, Ordering::Relaxed);
                self.voice_for_transcription = false;
                self.audio_status = "Deteniendo dictado…".into();
            }
        }
        let error = enabled
            .then(|| self.activation_error_with_accessibility(true))
            .flatten();
        self.actions_enabled = enabled && error.is_none();
        if let Some(error) = error {
            self.log("motor", error);
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
    pub fn capture_starting(&self) -> bool {
        !self.capture_service && self.launcher.pending()
    }
    pub fn start_capture_service(&mut self) -> Result<String, String> {
        self.actions_requested = true;
        self.engine_error = None;
        if self.capture_service || self.launcher.pending() {
            return Ok("Preparando el control…".into());
        }
        let default_root = PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
            .join("Library/Application Support/ReadyForAirLab");
        self.launcher.attempted = true;
        if self.root != default_root {
            return Err(
                "El arranque automático está deshabilitado en directorios de prueba".into(),
            );
        }
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let helper = crate::session::packaged_helper(&exe)?;
        self.launcher
            .start(&helper, &self.root.join("capture.sock"))?;
        Ok("Autorizá el diálogo de macOS para usar el control. No se abre Terminal.".into())
    }
    fn reconcile_engine(&mut self, trusted: bool) {
        if !self.actions_requested
            || self.learning.is_some()
            || self.awaiting_release
            || self.engine_error.is_some()
            || self.config_error
            || !self.listening
            || !trusted
        {
            return;
        }
        if !self.capture_service {
            if self.auto_launch && !self.launcher.attempted {
                if let Err(e) = self.start_capture_service() {
                    self.engine_error = Some(e.clone());
                    self.log("motor", e);
                }
            }
        } else if !self.actions_enabled {
            self.apply_actions(true);
        }
    }
    pub fn engine_status(&self) -> String {
        if let Some(e) = &self.engine_error {
            return e.clone();
        }
        if !self.actions_requested {
            return "Control pausado".into();
        }
        if self.learning.is_some() || self.awaiting_release {
            return "Identificando botón · soltalo para continuar".into();
        }
        if !crate::native_input::trusted() {
            return "Falta Accesibilidad para Studio · abrí Conexión".into();
        }
        if let Some(error) = &self.reader_error {
            return error.clone();
        }
        if !self.listening {
            if self.connected {
                return "Control presente, lectura bloqueada · revisá Conexión".into();
            }
            return "Esperando el control · revisá Conexión si no aparece".into();
        }
        if self.native_active && self.actions_enabled {
            return "Control listo · funciones activas".into();
        }
        if self.capture_starting() {
            return "Esperando autorización de macOS…".into();
        }
        if !self.capture_service && self.launcher.attempted {
            return "Sesión terminada · usá Reanudar lector".into();
        }
        "Preparando el control…".into()
    }
    pub fn test_action(&mut self, button: ButtonId, profile: Profile) -> Result<(), String> {
        self.config.validate()?;
        match self.action(button, profile) {
            Action::Default | Action::Disabled => {
                return Err("Elegí una acción personalizada para probar".into());
            }
            _ => self.dispatch(button, profile, Instant::now()),
        }
        Ok(())
    }
    fn dispatch(&mut self, button: ButtonId, profile: Profile, at: Instant) {
        self.last_action_result = None;
        let a = self.action(button, profile).clone();
        self.log(
            "acción",
            format!("{} · {} → {}", button.label(), profile.label(), a.label()),
        );
        if a == Action::VoiceToggle {
            if !self.transcribing {
                self.voice_for_transcription = true;
            }
            self.toggle_recording();
        } else if self
            .work_tx
            .as_ref()
            .expect("worker exists while Studio is alive")
            .try_send((a, self.action_generation.load(Ordering::Relaxed), at))
            .is_err()
        {
            self.log("acción", "Cola de acciones llena; pulsación descartada");
        }
    }
    pub fn toggle_recording(&mut self) {
        if self.recording {
            self.stop_audio.store(true, Ordering::Relaxed);
            self.audio_status = "Guardando grabación…".into();
        } else {
            if self.transcribing {
                self.log("voz", "Esperá a que termine la transcripción anterior");
                return;
            }
            self.recording_epoch = self.action_generation.load(Ordering::Relaxed);
            self.recording_transcriber = self.config.transcription.clone();
            self.voice_target = crate::actions::frontmost_app().ok();
            self.recording = true;
            self.stop_audio = Arc::new(AtomicBool::new(false));
            self.audio_status = "Abriendo micrófono…".into();
            audio::start(
                self.tx.clone(),
                self.root.clone(),
                self.config.audio_input.clone(),
                self.stop_audio.clone(),
            );
        }
    }
    pub fn refresh_audio_inputs(&mut self) {
        if self.audio_refreshing {
            return;
        }
        self.audio_refreshing = true;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Event::AudioInputs(audio::inputs()));
        });
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
        if m != self.mode {
            self.mode = m;
            self.log("modo", m.label());
        }
    }
    pub fn poll(&mut self) {
        self.poll_with_trust(crate::native_input::trusted());
    }
    fn poll_with_trust(&mut self, trusted: bool) {
        if let Some(result) = self.launcher.poll() {
            if let Err(e) = result {
                self.engine_error = Some(e.clone());
                self.log("motor", e);
            }
        }
        if self.actions_enabled && !trusted {
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
                "No llegó una pulsación reconocible; no se cambió la asignación".to_string();
            self.learn_result = Some(Err(message.clone()));
            self.log("aprender", message);
        }
        if self.mode == MouseMode::Active
            && self
                .last_mouse
                .is_some_and(|t| t.elapsed() > Duration::from_millis(self.config.mouse_idle_ms))
        {
            self.change_mode(MouseMode::Normal);
        }
        for _ in 0..4096 {
            let Ok(e) = self.rx.try_recv() else { break };
            match e {
                Event::Log(m) => {
                    if m.starts_with("HID abierto") {
                        self.connected = true;
                    }
                    self.log("sistema", m)
                }
                Event::BleConnected => {
                    self.connected = true;
                    self.connecting = false;
                    self.start_listening();
                }
                Event::BleDone => {
                    self.connecting = false;
                    self.ble_session = false;
                    if !self.listening {
                        self.connected = false;
                    }
                }
                Event::CapturePermission(access) => {
                    self.capture_access = access;
                }
                Event::BluetoothState(state) => {
                    self.bluetooth_state = state;
                }
                Event::CaptureService(ready) => {
                    self.capture_service = ready;
                    if !ready {
                        self.capture_access = crate::permissions::Access::Unknown;
                        self.capture_verified = false;
                        self.suspend_actions();
                    }
                    self.log(
                        "motor",
                        if ready {
                            "Servicio de captura listo"
                        } else {
                            "Servicio de captura desconectado"
                        },
                    );
                }
                Event::HidReady { exclusive } => {
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
                    self.log(
                        "motor",
                        if exclusive {
                            "Motor nativo activo · Karabiner no necesario"
                        } else {
                            "Control conectado · observando · acciones apagadas"
                        },
                    );
                }
                Event::HidUnavailable { present, message } => {
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
                    self.log("conexión", message);
                }
                Event::BleCandidates(candidates) => {
                    self.candidates = candidates;
                }
                Event::DirectAction {
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
                        self.dispatch(button, profile, at);
                    } else {
                        self.log("routing", format!("{} · {}: pulsación descartada (control pausado, identificación o configuración anterior)", button.label(), profile.label()));
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
                    if self.native_active {
                        self.capture_verified = true;
                    }
                    self.packet(at, &bytes);
                }
                Event::AudioInputs(result) => {
                    self.audio_refreshing = false;
                    match result {
                        Ok(inputs) => self.audio_inputs = inputs,
                        Err(e) => self.log("audio", e),
                    }
                }
                Event::AudioStarted(source) => {
                    self.audio_status = format!("Grabando · {source}");
                }
                Event::AudioDone(result) => {
                    self.recording = false;
                    let transcribe = std::mem::take(&mut self.voice_for_transcription)
                        && self.recording_epoch == self.action_generation.load(Ordering::Relaxed);
                    match result {
                        Err(e) => {
                            self.audio_status = e.clone();
                            self.log("audio", e)
                        }
                        Ok(path) => {
                            self.audio_status = format!("Audio guardado: {}", path.display());
                            self.log("audio", self.audio_status.clone());
                            if transcribe {
                                if let Some(t) = self.recording_transcriber.take() {
                                    self.transcribing = true;
                                    let epoch = self.recording_epoch;
                                    let tx = self.transcript_tx.clone();
                                    let target = self.voice_target.clone();
                                    let generation = self.action_generation.clone();
                                    self.transcript_thread = Some(std::thread::spawn(move || {
                                        let result = crate::transcription::transcribe_cancellable(
                                            &t,
                                            &path,
                                            &generation,
                                            epoch,
                                        )
                                        .map(|text| (text, t.insert_text, target, epoch));
                                        let _ = tx.send(result);
                                    }));
                                    self.audio_status = "Transcribiendo localmente…".into();
                                } else {
                                    self.log("audio","Grabación guardada. Configurá un transcriptor local en config.json para obtener texto.");
                                }
                            }
                        }
                    }
                }
            }
        }
        self.reconcile_engine(trusted);
        while let Ok(r) = self.work_rx.try_recv() {
            self.last_action_result = Some(match &r {
                Ok(message) => (message.clone(), false),
                Err(error) => (error.clone(), true),
            });
            match r {
                Ok(s) => self.log("acción", s),
                Err(e) => self.log("error", e),
            }
        }
        while let Ok(r) = self.transcript_rx.try_recv() {
            self.transcribing = false;
            if let Some(worker) = self.transcript_thread.take() {
                let _ = worker.join();
            }
            match r {
                Ok((text, insert, target, epoch)) => {
                    let path = self.root.join(format!("transcript-{}.txt", stamp()));
                    if let Err(e) = crate::storage::write_private(&path, text.as_bytes()) {
                        self.log("audio", e.to_string());
                        continue;
                    }
                    self.audio_status = format!("Transcripción guardada · {}", path.display());
                    self.log("audio", self.audio_status.clone());
                    if insert
                        && epoch == self.action_generation.load(Ordering::Relaxed)
                        && self.actions_enabled
                        && target.is_some()
                        && crate::actions::frontmost_app().ok() == target
                    {
                        match crate::actions::insert_text(&text) {
                            Ok(()) => self.log("voz", "Texto insertado"),
                            Err(e) => self.log("voz", e),
                        }
                    }
                }
                Err(e) => {
                    self.audio_status = e.clone();
                    self.log("audio", e)
                }
            }
        }
    }
    fn packet(&mut self, at: Instant, bytes: &[u8]) {
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
                crate::voice::Button::Keyboard(k) => InputSource::Keyboard(k),
                crate::voice::Button::Consumer(k) => InputSource::Consumer(k),
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
                    format!("{} identificado y guardado ({source}). Elegí una función; se guarda automáticamente. Al soltar el botón, el control vuelve a funcionar automáticamente.", button.label())
                });
                self.log(
                    "aprender",
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
                    format!("solo observación · {}", self.engine_status())
                } else if *action == Action::Default {
                    "función original".into()
                } else if *action == Action::Disabled {
                    "desactivado en este perfil".into()
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
                self.log("routing", format!("{s} · {}: sin botón identificado. Seleccioná el botón en Buttons y pulsá Identificar.", observed_profile.label()));
            }
        }
    }
}
impl Drop for Studio {
    fn drop(&mut self) {
        self.stop_hid.store(true, Ordering::Relaxed);
        self.stop_audio.store(true, Ordering::Relaxed);
        self.actions_enabled = false;
        self.action_generation.fetch_add(1, Ordering::Relaxed);
        self.publish_mapping();
        self.work_tx.take();
        if let Some(worker) = self.work_thread.take() {
            let _ = worker.join();
        }
        if let Some(worker) = self.transcript_thread.take() {
            let _ = worker.join();
        }
        if let Some(thread) = self.reader_thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        old.fill_mr01_sources();
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
        assert!(error.contains("lector de acciones no está activo"));
        assert!(!error.contains("Accesibilidad"));
        studio.capture_service = true;
        assert!(
            studio
                .activation_error_with_accessibility(false)
                .unwrap()
                .contains("Falta Accesibilidad")
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
        c.version = 2;
        assert!(c.validate().is_err());
        c.version = 1;
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
        s.poll_with_trust(true);
        let epoch = s.action_generation.load(Ordering::Relaxed);
        for button in [ButtonId::Home, ButtonId::Experiences] {
            s.tx.send(Event::DirectAction {
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
