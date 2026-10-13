//! Narrow privileged-capture transport. No configuration, scripts or output reports
//! cross this boundary: only a selected MR-01 path, lifecycle commands and input.
#[cfg(target_os = "macos")]
mod mac {
    use std::{
        io::{self, Read, Write},
        os::unix::{
            fs::{FileTypeExt, MetadataExt, PermissionsExt},
            io::AsRawFd,
            net::{UnixListener, UnixStream},
        },
        path::{Path, PathBuf},
        time::{Duration, Instant},
    };
    const OPEN: u8 = 1;
    const PING: u8 = 2;
    const STOP: u8 = 3;
    const READY: u8 = 10;
    const REPORT: u8 = 11;
    const CLOSED: u8 = 12;
    const ERROR: u8 = 13;
    const HELLO: u8 = 14;
    const ACCESS: u8 = 16;
    const DEVICE_LOST: u8 = 15;

    #[derive(Debug, PartialEq)]
    pub enum CaptureRead {
        Report(Vec<u8>),
        DeviceLost(String),
    }
    const MAX: usize = 4096;
    unsafe extern "C" {
        fn geteuid() -> u32;
        fn getpeereid(fd: i32, uid: *mut u32, gid: *mut u32) -> i32;
    }
    fn peer_uid(s: &UnixStream) -> Result<u32, String> {
        let (mut uid, mut gid) = (0, 0);
        if unsafe { getpeereid(s.as_raw_fd(), &mut uid, &mut gid) } != 0 {
            return Err(io::Error::last_os_error().to_string());
        }
        Ok(uid)
    }
    struct Wire {
        stream: UnixStream,
        pending: Vec<u8>,
    }
    impl Wire {
        fn new(stream: UnixStream) -> Result<Self, String> {
            stream
                .set_read_timeout(Some(Duration::from_millis(50)))
                .map_err(|e| e.to_string())?;
            stream
                .set_write_timeout(Some(Duration::from_millis(200)))
                .map_err(|e| e.to_string())?;
            Ok(Self {
                stream,
                pending: vec![],
            })
        }
        fn send(&mut self, kind: u8, data: &[u8]) -> Result<(), String> {
            if data.len() > MAX {
                return Err("Capture frame exceeds limit".into());
            }
            let mut frame = vec![kind];
            frame.extend_from_slice(&(data.len() as u16).to_le_bytes());
            frame.extend_from_slice(data);
            self.stream.write_all(&frame).map_err(|e| e.to_string())
        }
        fn recv(&mut self) -> Result<Option<(u8, Vec<u8>)>, String> {
            loop {
                if self.pending.len() >= 3 {
                    let len = u16::from_le_bytes([self.pending[1], self.pending[2]]) as usize;
                    if len > MAX {
                        return Err("Capture frame exceeds limit".into());
                    }
                    if self.pending.len() >= len + 3 {
                        let kind = self.pending[0];
                        let data = self.pending[3..3 + len].to_vec();
                        self.pending.drain(..3 + len);
                        return Ok(Some((kind, data)));
                    }
                }
                let mut buf = [0; MAX + 3];
                match self.stream.read(&mut buf) {
                    Ok(0) => return Err("Capture service disconnected".into()),
                    Ok(n) => self.pending.extend_from_slice(&buf[..n]),
                    Err(e)
                        if matches!(
                            e.kind(),
                            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                        ) =>
                    {
                        return Ok(None);
                    }
                    Err(e) => return Err(e.to_string()),
                }
            }
        }
    }

    pub struct Broker {
        listener: UnixListener,
        path: PathBuf,
        wire: Option<Wire>,
        greeted: bool,
        accepted_at: Instant,
        ping_at: Instant,
        pub input_access: crate::permissions::Access,
    }
    impl Broker {
        pub fn new(root: &Path) -> Result<Self, String> {
            let path = root.join("capture.sock");
            if let Ok(meta) = std::fs::symlink_metadata(&path) {
                if !meta.file_type().is_socket() || meta.uid() != unsafe { geteuid() } {
                    return Err("Unsafe capture socket; refusing to replace".into());
                }
                match UnixStream::connect(&path) {
                    Ok(_) => return Err("Another capture broker is running".into()),
                    Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => {}
                    Err(e) => return Err(e.to_string()),
                }
                std::fs::remove_file(&path).map_err(|e| e.to_string())?;
            }
            let listener = UnixListener::bind(&path).map_err(|e| e.to_string())?;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| e.to_string())?;
            listener.set_nonblocking(true).map_err(|e| e.to_string())?;
            Ok(Self {
                listener,
                path,
                wire: None,
                greeted: false,
                accepted_at: Instant::now(),
                ping_at: Instant::now(),
                input_access: crate::permissions::Access::Unknown,
            })
        }
        pub fn available(&self) -> bool {
            self.greeted && self.wire.is_some()
        }
        pub fn disconnect(&mut self) {
            self.wire = None;
            self.greeted = false;
            self.input_access = crate::permissions::Access::Unknown;
        }
        pub fn tick(&mut self) -> Result<(), String> {
            if self.wire.is_none() {
                match self.listener.accept() {
                    Ok((stream, _)) => {
                        if peer_uid(&stream)? != 0 {
                            return Err("Capture service must run as root".into());
                        }
                        self.wire = Some(Wire::new(stream)?);
                        self.greeted = false;
                        self.input_access = crate::permissions::Access::Unknown;
                        self.accepted_at = Instant::now();
                        self.ping_at = Instant::now() - Duration::from_secs(1);
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(()),
                    Err(e) => return Err(e.to_string()),
                }
            }
            let wire = self.wire.as_mut().unwrap();
            if !self.greeted {
                match wire.recv()? {
                    Some((HELLO, version))
                        if version.len() == 2 && version[0] == 2 && version[1] <= 2 =>
                    {
                        self.input_access = crate::permissions::Access::from_hid(version[1]);
                        self.greeted = true;
                    }
                    Some(_) => return Err("Unsupported capture protocol".into()),
                    None if self.accepted_at.elapsed() < Duration::from_secs(2) => return Ok(()),
                    None => return Err("Capture handshake timed out".into()),
                }
            }
            if self.ping_at.elapsed() >= Duration::from_millis(400) {
                wire.send(PING, &[])?;
                self.ping_at = Instant::now();
            }
            Ok(())
        }
        pub fn open(&mut self, path: &str) -> Result<Vec<u8>, String> {
            if !self.available() {
                return Err("Start ACA Capture before enabling actions.".into());
            }
            if path.is_empty() || path.len() > 512 {
                return Err("Invalid HID path".into());
            }
            self.wire.as_mut().unwrap().send(OPEN, path.as_bytes())?;
            let start = Instant::now();
            while start.elapsed() < Duration::from_secs(3) {
                self.tick()?;
                match self.wire.as_mut().unwrap().recv()? {
                    Some((ACCESS, value)) if value.len() == 1 && value[0] <= 2 => {
                        self.input_access = crate::permissions::Access::from_hid(value[0]);
                    }
                    Some((READY, data)) => return Ok(data),
                    Some((ERROR, data)) => return Err(String::from_utf8_lossy(&data).into_owned()),
                    Some(_) => return Err("Unexpected capture response".into()),
                    None => {}
                }
            }
            Err("Capture open timed out".into())
        }
        pub fn read(&mut self) -> Result<Option<CaptureRead>, String> {
            self.tick()?;
            match self.wire.as_mut().ok_or("Capture disconnected")?.recv()? {
                Some((REPORT, data)) => Ok(Some(CaptureRead::Report(data))),
                Some((DEVICE_LOST, data)) => Ok(Some(CaptureRead::DeviceLost(
                    String::from_utf8_lossy(&data).into_owned(),
                ))),
                Some((ERROR, data)) => Err(String::from_utf8_lossy(&data).into_owned()),
                Some(_) => Err("Unexpected capture message".into()),
                None => Ok(None),
            }
        }
        pub fn close(&mut self) -> Result<(), String> {
            self.wire
                .as_mut()
                .ok_or("Capture disconnected")?
                .send(STOP, &[])?;
            let start = Instant::now();
            while start.elapsed() < Duration::from_secs(2) {
                self.tick()?;
                match self.wire.as_mut().unwrap().recv()? {
                    Some((CLOSED, data)) if data.is_empty() => return Ok(()),
                    Some((REPORT, _)) | Some((DEVICE_LOST, _)) | None => {}
                    _ => return Err("Capture close failed".into()),
                }
            }
            Err("Capture close timed out".into())
        }
    }
    impl Drop for Broker {
        fn drop(&mut self) {
            self.disconnect();
            let _ = std::fs::remove_file(&self.path);
        }
    }

    fn valid_command(kind: u8, data: &[u8], active: bool) -> bool {
        match kind {
            OPEN => {
                !active
                    && !data.is_empty()
                    && data.len() <= 512
                    && std::str::from_utf8(data).is_ok()
            }
            PING | STOP => data.is_empty(),
            _ => false,
        }
    }
    /// Foreground, finite session. Caller authenticates with sudo; no installation.
    pub fn serve(path: &Path) -> Result<(), String> {
        if unsafe { geteuid() } != 0 {
            return Err("Capture requires administrator authentication".into());
        }
        let uid = std::env::var("SUDO_UID")
            .ok()
            .and_then(|x| x.parse::<u32>().ok())
            .filter(|uid| *uid != 0)
            .ok_or("Launch with sudo from your own user session")?;
        let meta = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        if !meta.file_type().is_socket() || meta.uid() != uid || meta.mode() & 0o777 != 0o600 {
            return Err("Capture socket must belong exclusively to the requesting user".into());
        }
        let stream = UnixStream::connect(path).map_err(|e| e.to_string())?;
        if peer_uid(&stream)? != uid {
            return Err("Unexpected Studio peer identity".into());
        }
        let mut wire = Wire::new(stream)?;
        wire.stream
            .set_read_timeout(Some(Duration::from_millis(1)))
            .map_err(|e| e.to_string())?;
        let api = hidapi::HidApi::new().map_err(|e| e.to_string())?;
        let mut device: Option<hidapi::HidDevice> = None;
        fn access_byte() -> u8 {
            match crate::permissions::input() {
                crate::permissions::Access::Granted => 0,
                crate::permissions::Access::Denied => 1,
                _ => 2,
            }
        }
        wire.send(HELLO, &[2, access_byte()])?;
        let started = Instant::now();
        let mut last_ping = Instant::now();
        let mut bytes = [0; MAX];
        while started.elapsed() < Duration::from_secs(30 * 60) {
            if last_ping.elapsed() > Duration::from_secs(2) {
                return Err("Studio heartbeat expired; remote released".into());
            }
            if let Some((kind, data)) = wire.recv()? {
                if !valid_command(kind, &data, device.is_some()) {
                    return Err("Invalid capture command".into());
                }
                match kind {
                    PING => last_ping = Instant::now(),
                    STOP => {
                        device = None;
                        wire.send(CLOSED, &[])?;
                    }
                    OPEN => {
                        wire.send(ACCESS, &[access_byte()])?;
                        // Refresh each request: device registry paths change on reconnection.
                        let mut api = hidapi::HidApi::new().map_err(|e| e.to_string())?;
                        api.refresh_devices().map_err(|e| e.to_string())?;
                        let info = api.device_list().find(|d| {
                            crate::hardware::is_remote(
                                d.product_string(),
                                d.vendor_id(),
                                d.product_id(),
                            ) && d.path().to_bytes() == data.as_slice()
                        });
                        let result = (|| {
                            let info = info.ok_or("Selected path is not an MR-01")?;
                            api.set_open_exclusive(true);
                            let opened = info.open_device(&api);
                            api.set_open_exclusive(false);
                            let dev = opened.map_err(|e| e.to_string())?;
                            let n = dev
                                .get_report_descriptor(&mut bytes)
                                .map_err(|e| e.to_string())?;
                            wire.send(READY, &bytes[..n])?;
                            device = Some(dev);
                            Ok::<_, String>(())
                        })();
                        if let Err(e) = result {
                            wire.send(ERROR, e.as_bytes())?;
                        }
                    }
                    _ => unreachable!(),
                }
            }
            if let Some(dev) = &device {
                match dev.read_timeout(&mut bytes, 5) {
                    Ok(n) if n > 0 => wire.send(REPORT, &bytes[..n])?,
                    Ok(_) => {}
                    Err(e) => {
                        // Device sleep/disconnection releases just this HID handle.
                        // Keep the authenticated channel and its original deadline.
                        device = None;
                        wire.send(DEVICE_LOST, e.to_string().as_bytes())?;
                    }
                }
            } else {
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        drop(device);
        drop(api);
        Err("30-minute capture session ended; remote released".into())
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn helper_permission_is_reported_separately_and_refreshed_on_open() {
            let path = std::env::temp_dir().join(format!("mr01-{}.sock", uuid::Uuid::new_v4()));
            let listener = UnixListener::bind(&path).unwrap();
            let (client, server) = UnixStream::pair().unwrap();
            let mut broker = Broker {
                listener,
                path,
                wire: Some(Wire::new(client).unwrap()),
                greeted: false,
                accepted_at: Instant::now(),
                ping_at: Instant::now(),
                input_access: crate::permissions::Access::Unknown,
            };
            let worker = std::thread::spawn(move || {
                let mut wire = Wire::new(server).unwrap();
                wire.send(HELLO, &[2, 1]).unwrap();
                loop {
                    if let Some((kind, _)) = wire.recv().unwrap() {
                        if kind == OPEN {
                            break;
                        }
                    }
                }
                wire.send(ACCESS, &[0]).unwrap();
                wire.send(READY, b"fixture descriptor").unwrap();
                std::thread::sleep(Duration::from_millis(100));
            });
            broker.tick().unwrap();
            assert!(broker.available());
            assert_eq!(broker.input_access, crate::permissions::Access::Denied);
            broker.open("MR01-path").unwrap();
            assert_eq!(broker.input_access, crate::permissions::Access::Granted);
            worker.join().unwrap();
        }
        #[test]
        fn lifecycle_drains_reports_before_close_and_can_reopen() {
            let path = std::env::temp_dir().join(format!("mr01-{}.sock", uuid::Uuid::new_v4()));
            let listener = UnixListener::bind(&path).unwrap();
            let (client, server) = UnixStream::pair().unwrap();
            let mut broker = Broker {
                listener,
                path,
                wire: Some(Wire::new(client).unwrap()),
                greeted: true,
                accepted_at: Instant::now(),
                ping_at: Instant::now(),
                input_access: crate::permissions::Access::Unknown,
            };
            let worker = std::thread::spawn(move || {
                let mut wire = Wire::new(server).unwrap();
                for _ in 0..2 {
                    loop {
                        if let Some((kind, data)) = wire.recv().unwrap() {
                            if kind == PING {
                                continue;
                            }
                            assert_eq!((kind, data.as_slice()), (OPEN, b"MR01-path".as_slice()));
                            break;
                        }
                    }
                    wire.send(READY, b"descriptor").unwrap();
                    wire.send(REPORT, &[3, 0x6c, 0]).unwrap();
                    wire.send(REPORT, &[3, 0, 0]).unwrap();
                    loop {
                        if let Some((kind, _)) = wire.recv().unwrap() {
                            if kind == PING {
                                continue;
                            }
                            assert_eq!(kind, STOP);
                            break;
                        }
                    }
                    wire.send(CLOSED, &[]).unwrap();
                }
            });
            for _ in 0..2 {
                assert_eq!(broker.open("MR01-path").unwrap(), b"descriptor");
                assert_eq!(
                    broker.read().unwrap(),
                    Some(CaptureRead::Report(vec![3, 0x6c, 0]))
                );
                broker.close().unwrap(); // release report still queued; must drain it
            }
            worker.join().unwrap();
            assert!(broker.read().is_err());
            broker.disconnect();
            assert!(!broker.available());
        }
        #[test]
        fn device_read_failure_keeps_authenticated_session_for_reopen() {
            let path = std::env::temp_dir().join(format!("mr01-{}.sock", uuid::Uuid::new_v4()));
            let listener = UnixListener::bind(&path).unwrap();
            let (client, server) = UnixStream::pair().unwrap();
            let mut broker = Broker {
                listener,
                path,
                wire: Some(Wire::new(client).unwrap()),
                greeted: true,
                accepted_at: Instant::now(),
                ping_at: Instant::now(),
                input_access: crate::permissions::Access::Unknown,
            };
            let worker = std::thread::spawn(move || {
                let mut wire = Wire::new(server).unwrap();
                let expect = |wire: &mut Wire, expected| loop {
                    if let Some((kind, _)) = wire.recv().unwrap() {
                        if kind != PING {
                            assert_eq!(kind, expected);
                            break;
                        }
                    }
                };
                expect(&mut wire, OPEN);
                wire.send(READY, b"descriptor").unwrap();
                wire.send(15, b"remote slept").unwrap();
                expect(&mut wire, STOP);
                wire.send(CLOSED, &[]).unwrap();
                expect(&mut wire, OPEN);
                wire.send(READY, b"descriptor").unwrap();
                wire.send(REPORT, &[3, 0x6c, 0]).unwrap();
                expect(&mut wire, STOP);
                wire.send(CLOSED, &[]).unwrap();
            });
            broker.open("MR01-path").unwrap();
            assert_eq!(
                broker.read().unwrap(),
                Some(CaptureRead::DeviceLost("remote slept".into()))
            );
            broker.close().unwrap();
            assert!(broker.available());
            broker.open("MR01-path").unwrap();
            assert!(broker.read().unwrap().is_some());
            broker.close().unwrap();
            worker.join().unwrap();
        }
        #[test]
        fn broker_refuses_an_unprivileged_peer() {
            if unsafe { geteuid() } == 0 {
                return;
            }
            let root = std::env::temp_dir().join(format!("mr01-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&root).unwrap();
            let mut broker = Broker::new(&root).unwrap();
            let _client = UnixStream::connect(root.join("capture.sock")).unwrap();
            assert!(broker.tick().unwrap_err().contains("root"));
            assert!(!broker.available());
            drop(broker);
            std::fs::remove_dir(root).unwrap();
        }
        #[test]
        fn command_boundary_rejects_payloads_and_unknown_commands() {
            assert!(!valid_command(99, b"anything", false));
            assert!(!valid_command(PING, b"script", false));
            assert!(!valid_command(OPEN, &[0xff], false));
            assert!(!valid_command(OPEN, &[b'x'; 513], false));
            assert!(!valid_command(OPEN, b"path", true));
            assert!(valid_command(OPEN, b"path", false));
            assert!(valid_command(STOP, b"", true));
        }
        #[test]
        fn partial_frame_survives_timeout_and_eof_is_an_error() {
            let (a, mut b) = UnixStream::pair().unwrap();
            let mut wire = Wire::new(a).unwrap();
            b.write_all(&[REPORT, 3]).unwrap();
            assert!(wire.recv().unwrap().is_none());
            b.write_all(&[0, 3, 0x6c, 0]).unwrap();
            assert_eq!(wire.recv().unwrap(), Some((REPORT, vec![3, 0x6c, 0])));
            drop(b);
            assert!(wire.recv().is_err());
        }
        #[test]
        fn rejects_oversized_frame_before_allocating_payload() {
            let (a, mut b) = UnixStream::pair().unwrap();
            let mut wire = Wire::new(a).unwrap();
            b.write_all(&[REPORT, 255, 255]).unwrap();
            assert!(wire.recv().is_err());
        }
    }
}
#[cfg(target_os = "macos")]
pub use mac::{Broker, CaptureRead, serve};
