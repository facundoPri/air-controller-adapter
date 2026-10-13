//! Single Studio owner per data directory. Connections never carry actions.
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
pub struct Server {
    thread: Option<std::thread::JoinHandle<()>>,
    stop: Arc<AtomicBool>,
    path: std::path::PathBuf,
}
impl Server {
    #[cfg(unix)]
    pub fn start(root: &Path) -> Result<Self, String> {
        use std::os::unix::{
            fs::{FileTypeExt, PermissionsExt},
            net::{UnixListener, UnixStream},
        };
        std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
        let path = root.join("studio.sock");
        if let Ok(meta) = std::fs::symlink_metadata(&path) {
            if !meta.file_type().is_socket() {
                return Err("studio.sock existe y no es un socket".into());
            }
            match UnixStream::connect(&path) {
                Ok(_) => return Err("Otra instancia ya controla las acciones".into()),
                Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => {}
                Err(e) => return Err(format!("No se pudo verificar el socket existente: {e}")),
            }
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        let listener = UnixListener::bind(&path).map_err(|e| e.to_string())?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let thread = std::thread::spawn(move || {
            while !stopped.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => drop(stream),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10))
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            thread: Some(thread),
            stop,
            path,
        })
    }
    #[cfg(not(unix))]
    pub fn start(_root: &Path) -> Result<Self, String> {
        Err("El puente de acciones aún requiere macOS/Unix".into())
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = std::fs::remove_file(&self.path);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn one_owner_and_releases_socket() {
        let root = std::path::PathBuf::from("/tmp").join(format!("rfa-{}", uuid::Uuid::new_v4()));
        let server = Server::start(&root).unwrap();
        assert!(Server::start(&root).is_err());
        drop(server);
        assert!(!root.join("studio.sock").exists());
        let second = Server::start(&root).unwrap();
        drop(second);
        std::fs::remove_dir(root).unwrap();
    }
}
