//! Managed reader contract. Registration, permission and HID readiness are separate facts.
pub const PROTOCOL_VERSION: u64 = 3;

#[derive(Debug, Clone)]
pub struct Handshake {
    pub build: String,
}
impl Handshake {
    pub fn current() -> Self {
        Self {
            build: option_env!("ACA_BUILD_ID").unwrap_or("unpackaged").into(),
        }
    }
    pub fn accepts(&self, protocol: u64, build: &str) -> Result<(), String> {
        if protocol != PROTOCOL_VERSION || build != self.build {
            return Err("The installed reader belongs to another ACA build. Stop ACA, remove its service, then enable this version.".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Status {
    #[default]
    Unknown,
    NotRegistered,
    Enabled,
    RequiresApproval,
    NotFound,
}
impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unknown => "Not checked",
            Self::NotRegistered => "Not enabled",
            Self::Enabled => "Enabled",
            Self::RequiresApproval => "Approval required in Login Items & Extensions",
            Self::NotFound => "No registered service found",
        }
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    Register,
    OpenSettings,
    Connect,
    Wait,
}
#[derive(Default)]
pub struct Setup {
    attempted: bool,
}
impl Setup {
    pub fn advance(&mut self, status: Status) -> Step {
        match status {
            Status::NotRegistered | Status::NotFound if !self.attempted => {
                self.attempted = true;
                Step::Register
            }
            Status::RequiresApproval => Step::OpenSettings,
            Status::Enabled => Step::Connect,
            _ => Step::Wait,
        }
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use std::{
        ffi::{CStr, CString, c_char, c_void},
        os::fd::FromRawFd,
        os::unix::net::UnixStream,
    };
    unsafe extern "C" {
        fn aca_service_status() -> i32;
        fn aca_service_register(error: *mut c_char, cap: usize) -> i32;
        fn aca_service_unregister(error: *mut c_char, cap: usize) -> i32;
        fn aca_service_settings();
        fn aca_stable_identity() -> i32;
        fn aca_channel_connect(fd: *mut i32, error: *mut c_char, cap: usize) -> *mut c_void;
        fn aca_channel_drop(channel: *mut c_void);
        fn aca_service_run(error: *mut c_char, cap: usize) -> i32;
        fn aca_session_active(uid: u32, session: u32) -> bool;
    }
    fn operation(call: unsafe extern "C" fn(*mut c_char, usize) -> i32) -> Result<(), String> {
        let mut error = [0; 2048];
        if unsafe { call(error.as_mut_ptr(), error.len()) } == 0 {
            Ok(())
        } else {
            Err(unsafe { CStr::from_ptr(error.as_ptr()) }
                .to_string_lossy()
                .into_owned())
        }
    }
    pub fn status() -> Status {
        match unsafe { aca_service_status() } {
            0 => Status::NotRegistered,
            1 => Status::Enabled,
            2 => Status::RequiresApproval,
            3 => Status::NotFound,
            _ => Status::Unknown,
        }
    }
    pub fn signed() -> bool {
        unsafe { aca_stable_identity() != 0 }
    }
    pub fn register() -> Result<(), String> {
        if !signed() {
            return Err("This build is ad-hoc signed. Use a stable Apple-signed ACA.app to enable its background service.".into());
        }
        operation(aca_service_register)
    }
    pub fn unregister() -> Result<(), String> {
        operation(aca_service_unregister)
    }
    pub fn open_settings() {
        unsafe { aca_service_settings() }
    }
    pub fn run() -> Result<(), String> {
        crate::hardware::initialize_hid_on_main_thread()?;
        operation(aca_service_run)
    }
    pub fn session_active(uid: u32, session: u32) -> bool {
        unsafe { aca_session_active(uid, session) }
    }
    pub struct Channel(*mut c_void);
    // Ownership moves with the HID worker. Native callbacks synchronize their own state.
    unsafe impl Send for Channel {}
    impl Channel {
        pub fn connect() -> Result<(Self, UnixStream), String> {
            let mut fd = -1;
            let mut error = [0; 2048];
            let channel = unsafe { aca_channel_connect(&mut fd, error.as_mut_ptr(), error.len()) };
            if channel.is_null() {
                return Err(unsafe { CStr::from_ptr(error.as_ptr()) }
                    .to_string_lossy()
                    .into_owned());
            }
            Ok((Self(channel), unsafe { UnixStream::from_raw_fd(fd) }))
        }
    }
    impl Drop for Channel {
        fn drop(&mut self) {
            unsafe { aca_channel_drop(self.0) }
        }
    }
    #[unsafe(no_mangle)]
    extern "C" fn aca_build_id() -> *const c_char {
        static BUILD: std::sync::OnceLock<CString> = std::sync::OnceLock::new();
        BUILD
            .get_or_init(|| CString::new(Handshake::current().build).unwrap())
            .as_ptr()
    }
    #[unsafe(no_mangle)]
    unsafe extern "C" fn aca_accept_handshake(protocol: u64, build: *const c_char) -> bool {
        if build.is_null() {
            return false;
        }
        unsafe extern "C" {
            fn strnlen(value: *const c_char, max: usize) -> usize;
        }
        if unsafe { strnlen(build, 65) } > 64 {
            return false;
        }
        let build = unsafe { CStr::from_ptr(build) }.to_string_lossy();
        Handshake::current().accepts(protocol, &build).is_ok()
    }
    #[unsafe(no_mangle)]
    extern "C" fn aca_reader_session(fd: i32, uid: u32, session: u32) {
        // Never unwind across the C boundary; dropping the stream releases HID on every exit.
        let result = std::panic::catch_unwind(|| {
            crate::capture::serve_authenticated(
                unsafe { UnixStream::from_raw_fd(fd) },
                uid,
                session,
            )
        });
        match result {
            Ok(Ok(())) => {}
            Ok(Err(error)) => eprintln!("ACA reader released: {error}"),
            Err(_) => eprintln!("ACA reader panic; session released"),
        }
    }
}
#[cfg(target_os = "macos")]
pub use mac::*;

#[cfg(target_os = "macos")]
pub fn command(command: &str) -> Result<String, String> {
    match command {
        "--build-info" => Ok(format!(
            "{} {}",
            if cfg!(feature = "service-probe") {
                "service-probe"
            } else if cfg!(feature = "managed-reader") {
                "managed-reader"
            } else {
                "legacy"
            },
            Handshake::current().build
        )),
        "--service-status" => Ok(format!(
            "Service: {:?}\nStable Apple signing identity: {}\nBuild: {}\nCapture: unverified",
            status(),
            signed(),
            Handshake::current().build
        )),
        "--enable-service" => {
            match status() {
                Status::Enabled => {}
                Status::RequiresApproval => {}
                _ => register()?,
            }
            Ok(format!(
                "Service: {:?}. Approve ACA in System Settings > General > Login Items & Extensions if requested. Capture remains unverified.",
                status()
            ))
        }
        "--remove-service" => {
            if status() != Status::NotRegistered {
                unregister()?;
            }
            Ok("ACA background service removed. User mappings were preserved.".into())
        }
        "--probe-reader" => probe(),
        _ => Err("Unknown service command".into()),
    }
}
#[cfg(target_os = "macos")]
fn probe() -> Result<String, String> {
    use std::time::{Duration, Instant};
    if status() != Status::Enabled {
        return Err("Enable and approve ACA's background service before testing capture.".into());
    }
    let mut broker = crate::capture::Broker::managed();
    broker.tick()?;
    if !broker.available() {
        return Err("No authenticated reader handshake".into());
    }
    println!(
        "Authenticated XPC and reader handshake: passed; Input Monitoring: {:?}",
        broker.input_access
    );
    let api = hidapi::HidApi::new().map_err(|e| e.to_string())?;
    let devices = crate::hardware::physical_devices(
        api.device_list()
            .filter(|d| {
                crate::hardware::is_remote(d.product_string(), d.vendor_id(), d.product_id())
            })
            .map(|d| (d.path().to_string_lossy().into_owned(), d)),
    );
    if devices.len() != 1 {
        return Err(format!(
            "Expected exactly one MR-01 interface, observed {}. Close other ACA versions, wake the remote and retry.",
            devices.len()
        ));
    }
    let descriptor = broker.open(&devices[0].path().to_string_lossy())?;
    println!(
        "Exclusive MR-01 open: passed; descriptor: {} bytes. Press remote buttons for 20 seconds.",
        descriptor.len()
    );
    let until = Instant::now() + Duration::from_secs(20);
    let mut reports = 0;
    while Instant::now() < until {
        match broker.read()? {
            Some(crate::capture::CaptureRead::Report(bytes)) => {
                reports += 1;
                println!("Report {reports}: {} bytes", bytes.len());
            }
            Some(crate::capture::CaptureRead::DeviceLost(error)) => return Err(error),
            None => {}
        }
    }
    broker.close()?;
    if reports == 0 {
        return Err("Exclusive open and release succeeded, but no reports arrived. Report delivery and action forwarding remain unverified.".into());
    }
    Ok(format!(
        "Received {reports} reports through the authenticated service. HID close acknowledged. This diagnostic does not execute mappings; validate forwarding separately in ACA."
    ))
}

/// Liveness watchdog shared by managed and legacy transports. A healthy managed
/// session has no wall-clock expiry; authorization remains subject to revocation.
pub struct CaptureLease {
    heartbeat: std::time::Duration,
}
impl CaptureLease {
    pub fn new(now: std::time::Duration) -> Self {
        Self { heartbeat: now }
    }
    pub fn heartbeat(&mut self, now: std::time::Duration) {
        self.heartbeat = now;
    }
    pub fn check(&self, now: std::time::Duration, active_session: bool) -> Result<(), String> {
        if !active_session {
            return Err("User session is no longer active; remote released".into());
        }
        if now.saturating_sub(self.heartbeat) > std::time::Duration::from_secs(2) {
            return Err("ACA heartbeat expired; remote released".into());
        }
        Ok(())
    }
}
