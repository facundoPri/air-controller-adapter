//! Read-only device discovery. Never writes HID reports or vendor GATT characteristics.
use btleplug::{
    api::{Central, Manager as _, Peripheral as _, ScanFilter},
    platform::Manager,
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::SyncSender,
    },
    time::{Duration, Instant},
};

pub const REMOTE_NAME: &str = "ReadyForAirRemote";
pub const VENDOR_ID: u16 = 0x5f00;
pub const PRODUCT_ID: u16 = 0;

#[derive(Debug)]
pub enum Event {
    Scoped {
        source: EventSource,
        generation: u64,
        event: Box<Event>,
    },
    HidSelected(String),
    BleSelected(String),
    BleFailed(String),
    Log(String),
    BleConnected,
    BleDone,
    HidStopped,
    CaptureService(bool),
    CapturePermission(crate::permissions::Access),
    BluetoothState(String),
    HidReady {
        exclusive: bool,
    },
    HidUnavailable {
        present: bool,
        message: String,
    },
    DirectAction {
        #[cfg(feature = "virtual-keyboard")]
        keyboard_generation: Option<u64>,
        button: crate::studio::ButtonId,
        profile: crate::studio::Profile,
        epoch: u64,
        at: Instant,
    },
    BleCandidates(Vec<(String, String)>),
    Packet {
        at: Instant,
        bytes: Vec<u8>,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventSource {
    Reader,
    Discovery,
}
#[derive(Clone)]
pub struct Events {
    tx: SyncSender<Event>,
    source: EventSource,
    generation: u64,
}
impl Events {
    pub fn new(tx: SyncSender<Event>, source: EventSource, generation: u64) -> Self {
        Self {
            tx,
            source,
            generation,
        }
    }
    pub fn try_send(&self, event: Event) -> Result<(), String> {
        self.tx
            .try_send(Event::Scoped {
                source: self.source,
                generation: self.generation,
                event: Box::new(event),
            })
            .map_err(|_| "Event queue unavailable".into())
    }
    pub fn send(&self, event: Event) -> Result<(), String> {
        self.try_send(event)
    }
}
pub fn log(tx: &Events, message: impl Into<String>) {
    let _ = tx.send(Event::Log(message.into()));
}

pub fn is_remote(name: Option<&str>, vendor: u16, product: u16) -> bool {
    name == Some(REMOTE_NAME) && vendor == VENDOR_ID && product == PRODUCT_ID
}

#[cfg(target_os = "macos")]
pub fn initialize_hid_on_main_thread() -> Result<(), String> {
    unsafe extern "C" {
        fn pthread_main_np() -> std::ffi::c_int;
    }
    if unsafe { pthread_main_np() } == 0 {
        return Err("HID initialization must run on the process main thread".into());
    }
    // hidapi keeps its native manager for the process lifetime and schedules it
    // on the first initializer's run loop.
    hidapi::HidApi::new().map(|_| ()).map_err(|e| e.to_string())
}

pub fn inspect_hid() -> Result<Vec<String>, String> {
    let api = hidapi::HidApi::new().map_err(|e| e.to_string())?;
    let mut lines = vec![];
    for info in api
        .device_list()
        .filter(|d| is_remote(d.product_string(), d.vendor_id(), d.product_id()))
    {
        lines.push(format!(
            "{} VID={:04X} PID={:04X} page={:04X} usage={:04X}",
            REMOTE_NAME,
            info.vendor_id(),
            info.product_id(),
            info.usage_page(),
            info.usage()
        ));
        let device = info.open_device(&api).map_err(|e| e.to_string())?;
        let mut descriptor = [0; 4096];
        let len = device
            .get_report_descriptor(&mut descriptor)
            .map_err(|e| e.to_string())?;
        lines.push(format!(
            "Descriptor: {}",
            crate::model::hex(&descriptor[..len])
        ));
    }
    if lines.is_empty() {
        lines.push("Remote not exposed as HID. Connect it using Bluetooth first.".into());
    }
    Ok(lines)
}

/// The path is a current handle, not a durable physical-device identity.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SelectedDevice {
    pub path: Option<String>,
    pub serial: Option<String>,
    /// BLE UUIDs cannot be safely correlated with HID paths by display name.
    pub require_confirmation: bool,
}
pub type DeviceChoice = Arc<std::sync::RwLock<SelectedDevice>>;
/// Desired native mapping, published atomically; None means shared observation only.
pub type Mapping = Arc<std::sync::RwLock<Option<(crate::studio::Config, u64)>>>;

// macOS enumerates one DeviceInfo per top-level HID collection. The MR-01
// has five collections sharing the SAME physical path; they are one remote.
pub(crate) fn physical_devices<T>(collections: impl IntoIterator<Item = (String, T)>) -> Vec<T> {
    collections
        .into_iter()
        .collect::<std::collections::BTreeMap<_, _>>()
        .into_values()
        .collect()
}

#[derive(Debug, Clone)]
struct HidIdentity {
    path: String,
    serial: Option<String>,
}

fn valid_serial(serial: Option<&str>) -> Option<String> {
    serial
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
}

fn select_hid(devices: &[HidIdentity], selected: &SelectedDevice) -> Option<usize> {
    if selected.require_confirmation {
        return None;
    }
    if let Some(serial) = &selected.serial {
        let mut matching = devices
            .iter()
            .enumerate()
            .filter(|(_, d)| d.serial.as_ref() == Some(serial));
        let first = matching.next()?.0;
        return matching.next().is_none().then_some(first);
    }
    if let Some(path) = &selected.path {
        return devices.iter().position(|d| &d.path == path);
    }
    (devices.len() == 1).then_some(0)
}

fn adopt_hid(devices: &[HidIdentity], index: usize) -> SelectedDevice {
    let device = &devices[index];
    // An explicit path choice remains usable even if the manufacturer repeats
    // serials. In that case never carry that serial across a changed path.
    let serial = device.serial.clone().filter(|serial| {
        devices
            .iter()
            .filter(|d| d.serial.as_ref() == Some(serial))
            .count()
            == 1
    });
    SelectedDevice {
        path: Some(device.path.clone()),
        serial,
        require_confirmation: false,
    }
}

pub fn listen(
    tx: Events,
    stop: Arc<AtomicBool>,
    reader_permission_request: Arc<AtomicBool>,
    root: PathBuf,
    mapping: Mapping,
    #[cfg(feature = "virtual-keyboard")] keyboard: crate::keyboard::Control,
    choice: DeviceChoice,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut last_error = String::new();
        #[cfg(target_os = "macos")]
        let mut broker = match {
            #[cfg(feature = "managed-reader")]
            {
                Ok::<_, String>(crate::capture::Broker::managed())
            }
            #[cfg(not(feature = "managed-reader"))]
            {
                crate::capture::Broker::new(&root)
            }
        } {
            Ok(b) => Some(b),
            Err(e) => {
                let _ = tx.try_send(Event::Log(format!("Capture: {e}")));
                None
            }
        };
        let mut helper_available = (false, crate::permissions::Access::Unknown);
        while !stop.load(Ordering::Relaxed) {
            #[cfg(target_os = "macos")]
            service_tick(
                &mut broker,
                &mut helper_available,
                &tx,
                &stop,
                &reader_permission_request,
            );
            let mut present = false;
            let result = (|| -> Result<(), String> {
                let api = hidapi::HidApi::new().map_err(|e| e.to_string())?;
                let devices = physical_devices(
                    api.device_list()
                        .filter(|d| is_remote(d.product_string(), d.vendor_id(), d.product_id()))
                        .map(|d| (d.path().to_string_lossy().into_owned(), d)),
                );
                present = !devices.is_empty();
                if !present {
                    return Err("Remote not connected. Wake it up or discover a new device.".into());
                }
                let identities: Vec<_> = devices
                    .iter()
                    .map(|d| HidIdentity {
                        path: d.path().to_string_lossy().into_owned(),
                        serial: valid_serial(d.serial_number()),
                    })
                    .collect();
                // Selection and adoption share the lock: an explicit choice cannot
                // be overwritten by an enumeration that began before that choice.
                let index = {
                    let mut selected = choice.write().unwrap();
                    let index = select_hid(&identities, &selected);
                    if let Some(index) = index {
                        *selected = adopt_hid(&identities, index);
                    }
                    index
                };
                let Some(index) = index else {
                    let _ = tx.try_send(Event::BleCandidates(
                        devices
                            .iter()
                            .map(|d| {
                                (
                                    format!("hid:{}", d.path().to_string_lossy()),
                                    format!(
                                        "{} · {}",
                                        REMOTE_NAME,
                                        d.serial_number().unwrap_or("HID")
                                    ),
                                )
                            })
                            .collect(),
                    ));
                    return Err("Select a remote in Connection: the previous device is unavailable or several remotes are present.".into());
                };
                let info = devices[index];
                if stop.load(Ordering::Relaxed) {
                    return Ok(());
                }
                let path = info.path().to_string_lossy().into_owned();
                tx.try_send(Event::HidSelected(path.clone()))?;
                let exclusive = mapping.read().unwrap().is_some();
                if exclusive && !crate::native_input::trusted() {
                    return Err("Enable Accessibility for Studio before enabling actions.".into());
                }
                #[cfg(target_os = "macos")]
                if exclusive {
                    let broker = broker.as_mut().ok_or("Capture service unavailable")?;
                    return capture_exclusive(
                        broker,
                        &info.path().to_string_lossy(),
                        &tx,
                        &stop,
                        &mapping,
                        #[cfg(feature = "virtual-keyboard")]
                        keyboard.clone(),
                    );
                }
                #[cfg(target_os = "macos")]
                api.set_open_exclusive(false);
                if stop.load(Ordering::Relaxed) {
                    return Ok(());
                }
                let device = info.open_device(&api).map_err(|e|
                    format!("Could not observe the MR-01: {e}. Check Input Monitoring and make sure another input remapper is not capturing this remote."))?;
                let mut descriptor = [0; 4096];
                let count = device
                    .get_report_descriptor(&mut descriptor)
                    .map_err(|e| e.to_string())?;
                std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
                std::fs::write(root.join("hid-descriptor.bin"), &descriptor[..count])
                    .map_err(|e| e.to_string())?;
                if stop.load(Ordering::Relaxed) {
                    return Ok(());
                }
                tx.try_send(Event::HidReady { exclusive })
                    .map_err(|_| "Input queue full".to_string())?;
                last_error.clear();
                let mut bytes = [0; 4096];
                let result = (|| {
                    while !stop.load(Ordering::Relaxed) {
                        #[cfg(target_os = "macos")]
                        service_tick(
                            &mut broker,
                            &mut helper_available,
                            &tx,
                            &stop,
                            &reader_permission_request,
                        );
                        let current = mapping.read().unwrap().clone();
                        let selected = choice.read().unwrap();
                        if current.is_some() != exclusive
                            || selected.require_confirmation
                            || selected.path.as_ref() != Some(&path)
                        {
                            break;
                        }
                        drop(selected);
                        let n = device
                            .read_timeout(&mut bytes, 100)
                            .map_err(|e| format!("HID reading interrupted: {e}"))?;
                        if n == 0 {
                            continue;
                        }
                        let at = Instant::now();
                        tx.try_send(Event::Packet {
                            at,
                            bytes: bytes[..n].to_vec(),
                        })
                        .map_err(|_| "Input queue full; capture stopped".to_string())?;
                    }
                    Ok(())
                })();
                result
            })();
            if let Err(message) = result {
                *mapping.write().unwrap() = None;
                if message != last_error {
                    let _ = tx.try_send(Event::HidUnavailable {
                        present,
                        message: message.clone(),
                    });
                    last_error = message;
                }
                for _ in 0..30 {
                    #[cfg(target_os = "macos")]
                    service_tick(
                        &mut broker,
                        &mut helper_available,
                        &tx,
                        &stop,
                        &reader_permission_request,
                    );
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        }
        let _ = tx.try_send(Event::HidStopped);
    })
}

#[cfg(target_os = "macos")]
fn service_tick(
    broker: &mut Option<crate::capture::Broker>,
    available: &mut (bool, crate::permissions::Access),
    tx: &Events,
    stop: &AtomicBool,
    reader_permission_request: &AtomicBool,
) {
    if stop.load(Ordering::Acquire) {
        return;
    }
    if let Some(broker) = broker {
        let result = broker.poll_status().and_then(|()| {
            if broker.available()
                && !stop.load(Ordering::Acquire)
                && reader_permission_request.swap(false, Ordering::AcqRel)
            {
                broker.request_input()?;
            }
            Ok(())
        });
        if let Err(e) = result {
            broker.disconnect();
            let _ = tx.try_send(Event::Log(format!("Capture service: {e}")));
        }
        let now = broker.available();
        if now != available.0 {
            available.0 = now;
            let _ = tx.try_send(Event::CaptureService(now));
        }
        if broker.input_access != available.1 {
            available.1 = broker.input_access;
            let _ = tx.try_send(Event::CapturePermission(broker.input_access));
        }
    }
}

#[cfg(target_os = "macos")]
fn capture_exclusive(
    broker: &mut crate::capture::Broker,
    path: &str,
    tx: &Events,
    stop: &AtomicBool,
    mapping: &Mapping,
    #[cfg(feature = "virtual-keyboard")] keyboard: crate::keyboard::Control,
) -> Result<(), String> {
    let mut router = crate::direct::Router::default();
    #[cfg(feature = "virtual-keyboard")]
    {
        router = router.with_keyboard(keyboard.clone());
    }
    let mut sender = crate::native_input::Sender::default();
    let result = (|| {
        broker.open(path)?;
        tx.try_send(Event::CapturePermission(broker.input_access))
            .map_err(|_| "Queue full")?;
        tx.try_send(Event::HidReady { exclusive: true })
            .map_err(|_| "Queue full")?;
        while !stop.load(Ordering::Relaxed) {
            let Some((config, epoch)) = mapping.read().unwrap().clone() else {
                break;
            };
            if !crate::native_input::trusted() {
                return Err("Accessibility permission was revoked".into());
            }
            #[cfg(feature = "virtual-keyboard")]
            {
                for output in router.poll_keyboard() {
                    sender.send(&output)?;
                }
                router.acknowledge_keyboard_fence();
            }
            let report = broker.read()?;
            if !broker.input_access.allowed() {
                let _ = tx.try_send(Event::CapturePermission(broker.input_access));
                return Err("Capture Input Monitoring was revoked or is unavailable.".into());
            }
            let Some(report) = report else {
                continue;
            };
            let bytes = match report {
                crate::capture::CaptureRead::Report(bytes) => bytes,
                crate::capture::CaptureRead::DeviceLost(message) => {
                    *mapping.write().unwrap() = None;
                    tx.try_send(Event::HidUnavailable {
                        present: false,
                        message: format!(
                            "Reading interrupted; reconnecting automatically: {message}"
                        ),
                    })
                    .map_err(|_| "Queue full")?;
                    break;
                }
            };
            if stop.load(Ordering::Relaxed)
                || mapping
                    .read()
                    .unwrap()
                    .as_ref()
                    .is_none_or(|(_, current)| *current != epoch)
            {
                break;
            }
            let at = Instant::now();
            for output in router.process(&bytes, at, &config) {
                if stop.load(Ordering::Relaxed) {
                    return Ok(());
                }
                #[cfg(feature = "virtual-keyboard")]
                if let crate::direct::Output::KeyboardPointer {
                    generation,
                    press,
                    down,
                } = output
                {
                    keyboard.pointer_for(
                        generation,
                        press,
                        down,
                        crate::native_keyboard::pointer_position(),
                        at,
                    );
                    continue;
                }
                #[cfg(feature = "virtual-keyboard")]
                if let crate::direct::Output::SessionAction { button, generation } = output {
                    tx.try_send(Event::DirectAction {
                        button,
                        profile: crate::studio::Profile::Keyboard,
                        epoch,
                        at,
                        keyboard_generation: Some(generation),
                    })
                    .map_err(|_| "Queue full")?;
                    continue;
                }
                if let crate::direct::Output::Action(button, profile) = output {
                    tx.try_send(Event::DirectAction {
                        #[cfg(feature = "virtual-keyboard")]
                        keyboard_generation: None,
                        button,
                        profile,
                        epoch,
                        at,
                    })
                    .map_err(|_| "Queue full")?;
                } else if let Some(message) = crate::native_input::unsupported_original(&output) {
                    if !message.is_empty() {
                        let _ = tx.try_send(Event::Log(message));
                    }
                } else {
                    sender.send(&output)?;
                }
            }
            #[cfg(feature = "virtual-keyboard")]
            router.acknowledge_keyboard_fence();
            tx.try_send(Event::Packet { at, bytes })
                .map_err(|_| "Queue full")?;
        }
        Ok(())
    })();
    #[cfg(feature = "virtual-keyboard")]
    {
        keyboard.cancel(None);
        keyboard.set_router_online(false);
    }
    // The unprivileged process releases its own synthetic holds before the
    // helper relinquishes the physical device. A broken channel exits helper.
    for output in router.release_all() {
        let _ = sender.send(&output);
    }
    // An output/permission failure releases the HID but does not throw away
    // authentication. A broken/invalid channel still fails close and disconnects.
    if let Err(e) = broker.close() {
        broker.disconnect();
        return Err(e);
    }
    result
}

async fn bounded<T>(
    duration: Duration,
    future: impl std::future::Future<Output = Result<T, String>>,
) -> Result<T, String> {
    tokio::time::timeout(duration, future).await.map_err(|_| {
        "Bluetooth timed out. Check the adapter and try searching again.".to_string()
    })?
}

/// A discovery owns its cancellation and worker. Cancellation also attempts to
/// release only its local CoreBluetooth connection; this is not physical disconnect.
pub struct Discovery {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<Result<(), String>>>,
}
impl Discovery {
    pub fn cancel(&self) {
        self.stop.store(true, Ordering::Release);
    }
    pub fn finished(&self) -> bool {
        self.thread.as_ref().is_none_or(|t| t.is_finished())
    }
    pub fn reap(&mut self) -> Result<(), String> {
        if !self.finished() {
            return Err("Discovery has not stopped yet.".into());
        }
        match self.thread.take() {
            Some(thread) => thread
                .join()
                .unwrap_or_else(|_| Err("Discovery worker stopped unexpectedly.".into())),
            None => Ok(()),
        }
    }
}
impl Drop for Discovery {
    fn drop(&mut self) {
        self.cancel();
        // All async operations and the cleanup below are time bounded.
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

async fn cancellable<T>(
    stop: &AtomicBool,
    duration: Duration,
    future: impl std::future::Future<Output = Result<T, String>>,
) -> Result<T, String> {
    use std::{future::Future, task::Poll};
    let mut future = std::pin::pin!(future);
    let mut tick = Box::pin(tokio::time::sleep(Duration::from_millis(25)));
    bounded(
        duration,
        std::future::poll_fn(|cx| {
            if stop.load(Ordering::Acquire) {
                return Poll::Ready(Err("Discovery canceled".into()));
            }
            if let Poll::Ready(value) = future.as_mut().poll(cx) {
                return Poll::Ready(value);
            }
            if tick.as_mut().poll(cx).is_ready() {
                tick.as_mut()
                    .reset(tokio::time::Instant::now() + Duration::from_millis(25));
                let _ = tick.as_mut().poll(cx);
            }
            Poll::Pending
        }),
    )
    .await
}

pub fn discover_and_connect(tx: Events, root: PathBuf, selected: Option<String>) -> Discovery {
    let stop = Arc::new(AtomicBool::new(false));
    let canceled = stop.clone();
    let thread = std::thread::spawn(move || {
        let result = tokio::runtime::Runtime::new()
            .map_err(|e| e.to_string())
            .and_then(|rt| {
                let result = rt.block_on(discovery_session(&tx, &root, selected, &canceled));
                rt.shutdown_timeout(Duration::from_secs(1));
                result
            });
        let result = match result {
            Err(error) if error == "Discovery canceled" => Ok(()),
            result => result,
        };
        if !canceled.load(Ordering::Acquire) {
            if let Err(error) = &result {
                let _ = tx.try_send(Event::BleFailed(error.clone()));
            }
            let _ = tx.try_send(Event::BleDone);
        }
        result
    });
    Discovery {
        stop,
        thread: Some(thread),
    }
}

async fn discovery_session(
    tx: &Events,
    root: &std::path::Path,
    selected: Option<String>,
    stop: &AtomicBool,
) -> Result<(), String> {
    let started = Instant::now();
    let manager = cancellable(stop, Duration::from_secs(10), async {
        Manager::new().await.map_err(|e| e.to_string())
    })
    .await?;
    let adapter = cancellable(stop, Duration::from_secs(10), async {
        manager
            .adapters()
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .next()
            .ok_or("No Bluetooth adapter".into())
    })
    .await?;
    let state = cancellable(stop, Duration::from_secs(5), async {
        adapter.adapter_state().await.map_err(|e| e.to_string())
    })
    .await?;
    tx.try_send(Event::BluetoothState(format!("{state:?}")))?;
    if state == btleplug::api::CentralState::PoweredOff {
        return Err("Bluetooth is off. Turn it on in System Settings → Bluetooth.".into());
    }
    // Keep the peripheral even while connect() is pending, so cancellation can
    // cancel that exact request. Never use adapter-wide power or unpair operations.
    let mut owned = None;
    let remaining = Duration::from_secs(60).saturating_sub(started.elapsed());
    let result = cancellable(stop, remaining, async {
        adapter.start_scan(ScanFilter::default()).await.map_err(|e| e.to_string())?;
        log(tx, "Searching directly for 15 seconds. Hold Back + OK until the LED flashes; the remote does not need to appear in Bluetooth Settings.");
        let mut found = std::collections::BTreeMap::new();
        for _ in 0..30 {
            for peripheral in adapter.peripherals().await.map_err(|e| e.to_string())? {
                let props = peripheral.properties().await.map_err(|e| e.to_string())?;
                if props.as_ref().and_then(|p| p.local_name.as_deref()) == Some(REMOTE_NAME) {
                    found.insert(format!("{:?}", peripheral.id()), peripheral);
                }
            }
            // The pinned known identifier is preferred, never another lone remote.
            if selected.as_ref().is_some_and(|id| found.contains_key(id)) { break; }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        let id = match selected {
            Some(id) => id,
            None if found.len() == 1 => found.keys().next().unwrap().clone(),
            None if found.len() > 1 => {
                tx.try_send(Event::BleCandidates(found.keys().map(|id| (id.clone(), REMOTE_NAME.into())).collect()))?;
                return Ok(false);
            }
            _ => return Err("No advertising MR-01 found. Hold Back + OK until the LED flashes, keep it nearby, then choose Search again in ACA. If another computer or phone connects first, disconnect the remote there.".into()),
        };
        let peripheral = found.remove(&id).ok_or("The selected remote is unavailable. Select a device or retry; ACA will not switch to another remote.")?;
        owned = Some(peripheral.clone());
        tx.try_send(Event::BleSelected(id.clone()))?;
        log(tx, "MR-01 advertisement found; opening its Bluetooth connection.");
        if !peripheral.is_connected().await.map_err(|e| e.to_string())? {
            bounded(Duration::from_secs(15), async { peripheral.connect().await.map_err(|e| e.to_string()) })
                .await.map_err(|e| format!("MR-01 connection/service discovery failed: {e}. Keep the remote awake and retry in ACA."))?;
        }
        bounded(Duration::from_secs(10), async { peripheral.discover_services().await.map_err(|e| e.to_string()) })
            .await.map_err(|e| format!("MR-01 service discovery failed: {e}"))?;
        let services: Vec<_> = peripheral.services().into_iter().map(|s| s.uuid.to_string()).collect();
        std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
        let evidence = serde_json::json!({"name":REMOTE_NAME,"id":id,"services":services,"audio_confirmed":false});
        std::fs::write(root.join("connection.json"), serde_json::to_vec_pretty(&evidence).unwrap()).map_err(|e| e.to_string())?;
        log(tx, "Bluetooth link open. Waiting for macOS to expose the remote as HID; input is not verified yet.");
        tx.try_send(Event::BleConnected)?;
        Ok(true)
    }).await;
    let scan_cleanup = bounded(Duration::from_secs(2), async {
        adapter.stop_scan().await.map_err(|e| e.to_string())
    })
    .await;
    let result = match scan_cleanup {
        Ok(()) => result,
        Err(error) => Err(format!("Bluetooth scan cleanup failed: {error}")),
    };
    let result = match result {
        Ok(true) => {
            let peripheral = owned.as_ref().unwrap();
            loop {
                let connected = cancellable(stop, Duration::from_secs(8), async {
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    peripheral.is_connected().await.map_err(|e| e.to_string())
                })
                .await;
                match connected {
                    Ok(true) => {}
                    Ok(false) => break Ok(()),
                    Err(error) => break Err(error),
                }
            }
        }
        Ok(false) => Ok(()),
        Err(error) => Err(error),
    };
    if let Some(peripheral) = owned {
        // Local cancellation only. macOS may retain its HID/system connection.
        bounded(Duration::from_secs(2), async {
            peripheral.disconnect().await.map_err(|e| e.to_string())
        }).await.map_err(|error| format!("Could not confirm local Bluetooth cancellation: {error}. Use Bluetooth Settings; physical disconnection is unverified."))?;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "macos")]
    #[test]
    fn idle_worker_requests_permission_only_for_live_explicit_intent() {
        use std::io::{Read, Write};
        let (broker, mut peer) = crate::capture::Broker::test_channel();
        peer.set_read_timeout(Some(Duration::from_millis(20)))
            .unwrap();
        let mut broker = Some(broker);
        let mut available = (false, crate::permissions::Access::Unknown);
        let (tx, rx) = std::sync::mpsc::sync_channel(32);
        let events = Events::new(tx, EventSource::Reader, 7);
        let stop = AtomicBool::new(false);
        let request = AtomicBool::new(false);
        let next_command = |peer: &mut std::os::unix::net::UnixStream| {
            loop {
                let mut header = [0; 3];
                match peer.read_exact(&mut header) {
                    Ok(()) => {
                        let mut payload =
                            vec![0; u16::from_le_bytes([header[1], header[2]]) as usize];
                        peer.read_exact(&mut payload).unwrap();
                        if header[0] != 2 {
                            return Some((header[0], payload));
                        }
                    }
                    Err(e)
                        if matches!(
                            e.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) =>
                    {
                        return None;
                    }
                    Err(e) => panic!("{e}"),
                }
            }
        };
        service_tick(&mut broker, &mut available, &events, &stop, &request);
        assert_eq!(next_command(&mut peer), None);
        request.store(true, Ordering::Release);
        service_tick(&mut broker, &mut available, &events, &stop, &request);
        assert!(request.load(Ordering::Acquire));
        assert_eq!(next_command(&mut peer), None);
        peer.write_all(&[14, 2, 0, 2, 2]).unwrap();
        service_tick(&mut broker, &mut available, &events, &stop, &request);
        assert_eq!(next_command(&mut peer), Some((17, vec![])));
        assert!(!request.load(Ordering::Acquire));
        peer.write_all(&[16, 1, 0, 0]).unwrap();
        service_tick(&mut broker, &mut available, &events, &stop, &request);
        assert_eq!(available, (true, crate::permissions::Access::Granted));
        assert!(rx.try_iter().any(|event| matches!(event,
            Event::Scoped { generation: 7, event, .. }
            if matches!(*event, Event::CapturePermission(crate::permissions::Access::Granted))
        )));
        service_tick(&mut broker, &mut available, &events, &stop, &request);
        assert_eq!(next_command(&mut peer), None);
        request.store(true, Ordering::Release);
        stop.store(true, Ordering::Release);
        service_tick(&mut broker, &mut available, &events, &stop, &request);
        assert_eq!(next_command(&mut peer), None);
    }
    #[test]
    fn cancel_interrupts_an_unresponsive_discovery_operation_and_joins_worker() {
        let stop = Arc::new(AtomicBool::new(false));
        let canceled = stop.clone();
        let (started, waiting) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(async {
                started.send(()).unwrap();
                assert!(
                    cancellable::<()>(&canceled, Duration::from_secs(60), std::future::pending())
                        .await
                        .unwrap_err()
                        .contains("canceled")
                );
            });
            Ok(())
        });
        waiting.recv_timeout(Duration::from_secs(2)).unwrap();
        let mut discovery = Discovery {
            stop,
            thread: Some(thread),
        };
        let before = Instant::now();
        discovery.cancel();
        while !discovery.finished() && before.elapsed() < Duration::from_secs(1) {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(discovery.finished());
        discovery.reap().unwrap();
        assert!(discovery.thread.is_none());
    }

    #[test]
    fn a_full_event_queue_cannot_block_worker_cancellation() {
        let (tx, _rx) = std::sync::mpsc::sync_channel(1);
        let events = Events::new(tx, EventSource::Discovery, 5);
        events.send(Event::BleConnected).unwrap();
        assert!(events.send(Event::BleDone).is_err());
    }

    #[test]
    fn discovery_deadline_covers_unresponsive_platform_calls() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let result = tokio::time::timeout(
                Duration::from_millis(100),
                bounded::<()>(Duration::from_millis(10), std::future::pending()),
            )
            .await;
            assert!(
                matches!(result, Ok(Err(_))),
                "platform call outlived discovery deadline"
            );
        });
    }
    #[test]
    fn same_remote_reconnects_on_new_path_without_selecting_another_device() {
        let selected = SelectedDevice {
            path: Some("old-path".into()),
            serial: Some("fixture-serial-A".into()),
            require_confirmation: false,
        };
        let devices = vec![
            HidIdentity {
                path: "other-path".into(),
                serial: Some("fixture-serial-B".into()),
            },
            HidIdentity {
                path: "new-path".into(),
                serial: Some("fixture-serial-A".into()),
            },
        ];
        assert_eq!(select_hid(&devices, &selected), Some(1));
        assert_eq!(
            select_hid(&devices[..1], &selected),
            None,
            "never adopt a different serial"
        );
        let ambiguous = vec![
            devices[1].clone(),
            HidIdentity {
                path: "duplicate-path".into(),
                serial: Some("fixture-serial-A".into()),
            },
        ];
        assert_eq!(
            select_hid(&ambiguous, &selected),
            None,
            "ambiguous identities need a choice"
        );
        let no_serial = SelectedDevice {
            path: Some("old-path".into()),
            serial: None,
            require_confirmation: false,
        };
        assert_eq!(
            select_hid(&devices, &no_serial),
            None,
            "a changed path without stable identity needs a choice"
        );
    }

    #[test]
    fn disappeared_selection_offers_remaining_remote_without_switching() {
        let devices = vec![HidIdentity {
            path: "B".into(),
            serial: None,
        }];
        let mut selected = SelectedDevice {
            path: Some("A".into()),
            ..Default::default()
        };
        assert_eq!(select_hid(&devices, &selected), None);
        selected.path = Some("B".into());
        assert_eq!(select_hid(&devices, &selected), Some(0));
        assert_eq!(select_hid(&devices, &SelectedDevice::default()), Some(0));
        selected.require_confirmation = true;
        assert_eq!(select_hid(&devices, &selected), None);
    }

    #[test]
    fn blank_or_duplicate_serials_are_not_durable_identity() {
        assert_eq!(valid_serial(Some("  ")), None);
        assert_eq!(valid_serial(None), None);
        assert_eq!(valid_serial(Some("fixture-A")), Some("fixture-A".into()));
        let devices = vec![
            HidIdentity {
                path: "A".into(),
                serial: Some("duplicate".into()),
            },
            HidIdentity {
                path: "B".into(),
                serial: Some("duplicate".into()),
            },
        ];
        let selected = adopt_hid(&devices, 1);
        assert_eq!(selected.path.as_deref(), Some("B"));
        assert_eq!(selected.serial, None);
        assert_eq!(select_hid(&devices, &selected), Some(1));
    }
    #[test]
    fn five_collections_are_one_remote_not_five_candidates() {
        let collections = (0..5).map(|i| ("DevSrvsID:4295006879".to_string(), i));
        assert_eq!(physical_devices(collections).len(), 1);
        assert_eq!(
            physical_devices([("a".into(), 0), ("a".into(), 1), ("b".into(), 2)]).len(),
            2
        );
    }
    #[test]
    fn rejects_other_keyboards_even_with_shared_ids_or_name() {
        assert!(is_remote(Some(REMOTE_NAME), VENDOR_ID, PRODUCT_ID));
        assert!(!is_remote(Some("Keyboard"), VENDOR_ID, PRODUCT_ID));
        assert!(!is_remote(Some(REMOTE_NAME), 0x05ac, PRODUCT_ID));
        assert!(!is_remote(None, VENDOR_ID, PRODUCT_ID));
    }
}
