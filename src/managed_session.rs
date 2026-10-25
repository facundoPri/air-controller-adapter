//! App-managed registration. No administrator shell, staging copy, or session expiry.
use crate::service::{self, Setup, Status, Step};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionEnd {
    Canceled,
    Expired,
    Ended,
    Failed(String),
}
impl SessionEnd {
    pub fn message(&self) -> String {
        match self {
            Self::Canceled => "ACA background approval is disabled. Enable it in Login Items & Extensions, then retry.".into(),
            Self::Expired | Self::Ended => "The reader connection ended and the remote was released. Retry when ready.".into(),
            Self::Failed(error) => format!("Reader setup: {error}"),
        }
    }
}
#[derive(Default)]
pub struct Launcher {
    pub attempted: bool,
    pub status: Status,
    pub last_error: Option<String>,
    checked: Option<Instant>,
    waiting: Option<Instant>,
    task: Option<std::thread::JoinHandle<Result<(), String>>>,
    connected: bool,
}
impl Launcher {
    pub fn pending(&self) -> bool {
        self.task.is_some() || self.waiting.is_some()
    }
    pub fn enable(&mut self) -> Result<(), String> {
        if self.pending() {
            return Ok(());
        }
        verify_packaged_helper(&packaged_helper(
            &std::env::current_exe().map_err(|e| e.to_string())?,
        )?)?;
        self.attempted = true;
        self.last_error = None;
        self.task = Some(std::thread::spawn(|| {
            let mut setup = Setup::default();
            match setup.advance(service::status()) {
                Step::Connect => Ok(()),
                Step::OpenSettings => { service::open_settings(); Ok(()) },
                Step::Register => service::register(),
                Step::Wait => Err("Service status is unavailable. Check Login Items & Extensions before retrying.".into()),
            }
        }));
        Ok(())
    }
    pub fn remove(&mut self) -> Result<(), String> {
        if self.task.is_some() {
            return Err("Wait for the current setup operation.".into());
        }
        self.cancel();
        self.last_error = None;
        self.task = Some(std::thread::spawn(|| {
            if service::status() == Status::NotRegistered {
                Ok(())
            } else {
                service::unregister()
            }
        }));
        Ok(())
    }
    pub fn start(&mut self, helper: &Path, _socket: &Path) -> Result<(), String> {
        if self.pending() || self.connected {
            return Ok(());
        }
        self.attempted = true;
        verify_packaged_helper(helper)?;
        self.status = service::status();
        if self.status != Status::Enabled {
            return Err("Enable ACA's background service in Connection, then approve it in Login Items & Extensions.".into());
        }
        self.waiting = Some(Instant::now());
        Ok(())
    }
    pub fn authenticated(&mut self) {
        self.waiting = None;
        self.connected = true;
    }
    pub fn poll(&mut self) -> Option<SessionEnd> {
        if self
            .checked
            .is_none_or(|at| at.elapsed() >= Duration::from_secs(1))
        {
            self.status = service::status();
            self.checked = Some(Instant::now());
        }
        if self.task.as_ref().is_some_and(|task| task.is_finished()) {
            let result = self
                .task
                .take()
                .unwrap()
                .join()
                .unwrap_or_else(|_| Err("Service setup stopped unexpectedly".into()));
            self.status = service::status();
            self.checked = Some(Instant::now());
            if let Err(error) = result {
                self.last_error = Some(error.clone());
                return Some(SessionEnd::Failed(error));
            }
        }
        if self.connected && self.status != Status::Enabled {
            self.cancel();
            return Some(SessionEnd::Canceled);
        }
        if self
            .waiting
            .is_some_and(|at| at.elapsed() > Duration::from_secs(5))
        {
            self.waiting = None;
            return Some(SessionEnd::Failed(
                "No authenticated reader arrived. Check background approval and retry.".into(),
            ));
        }
        None
    }
    pub fn cancel(&mut self) {
        self.waiting = None;
        self.connected = false;
    }
}
// Registration cannot be cancelled while macOS owns an authorization operation.
// Its completion is reaped, and capture is separately gated by connection intent.

pub fn packaged_helper(executable: &Path) -> Result<PathBuf, String> {
    let app = executable
        .ancestors()
        .nth(3)
        .ok_or("ACA app bundle not found")?;
    Ok(app.join("Contents/Library/Helpers/ACA Reader.app/Contents/MacOS/ReadyForAirCapture"))
}
pub fn verify_packaged_helper(helper: &Path) -> Result<(), String> {
    let app = helper
        .ancestors()
        .nth(7)
        .ok_or("ACA app bundle not found")?;
    for path in helper.ancestors().take(8) {
        let metadata = std::fs::symlink_metadata(path)
            .map_err(|_| "ACA's internal reader is missing. Reinstall the complete app.")?;
        if metadata.file_type().is_symlink() {
            return Err("ACA's reader must not be a symbolic link".into());
        }
    }
    if !helper.is_file() {
        return Err("The reader is not an executable file".into());
    }
    let output = std::process::Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(app)
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("ACA's signature is invalid. Reinstall the complete signed app.".into());
    }
    Ok(())
}
