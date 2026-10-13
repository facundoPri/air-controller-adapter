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
    AudioDone(Result<PathBuf, String>),
    AudioStarted(String),
    AudioInputs(Result<Vec<String>, String>),
}
pub type Events = SyncSender<Event>;
pub fn log(tx: &Events, message: impl Into<String>) {
    let _ = tx.send(Event::Log(message.into()));
}

pub fn is_remote(name: Option<&str>, vendor: u16, product: u16) -> bool {
    name == Some(REMOTE_NAME) && vendor == VENDOR_ID && product == PRODUCT_ID
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

/// Desired native mapping, published atomically; None means shared observation only.
pub type DeviceChoice = Arc<std::sync::RwLock<Option<String>>>;
pub type Mapping = Arc<std::sync::RwLock<Option<(crate::studio::Config, u64)>>>;

// macOS enumerates one DeviceInfo per top-level HID collection. The MR-01
// has five collections sharing the SAME physical path; they are one remote.
fn physical_devices<T>(collections: impl IntoIterator<Item = (String, T)>) -> Vec<T> {
    collections
        .into_iter()
        .collect::<std::collections::BTreeMap<_, _>>()
        .into_values()
        .collect()
}

fn requires_selection(paths: &[String], selected: Option<&str>) -> bool {
    !paths.is_empty()
        && match selected {
            Some(path) => !paths.iter().any(|p| p == path),
            None => paths.len() > 1,
        }
}

pub fn listen(
    tx: Events,
    stop: Arc<AtomicBool>,
    root: PathBuf,
    mapping: Mapping,
    choice: DeviceChoice,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut last_error = String::new();
        #[cfg(target_os = "macos")]
        let mut broker = match crate::capture::Broker::new(&root) {
            Ok(b) => Some(b),
            Err(e) => {
                let _ = tx.try_send(Event::Log(format!("Capture: {e}")));
                None
            }
        };
        let mut helper_available = (false, crate::permissions::Access::Unknown);
        while !stop.load(Ordering::Relaxed) {
            #[cfg(target_os = "macos")]
            service_tick(&mut broker, &mut helper_available, &tx);
            let mut present = false;
            let result = (|| -> Result<(), String> {
                let api = hidapi::HidApi::new().map_err(|e| e.to_string())?;
                let devices = physical_devices(
                    api.device_list()
                        .filter(|d| is_remote(d.product_string(), d.vendor_id(), d.product_id()))
                        .map(|d| (d.path().to_string_lossy().into_owned(), d)),
                );
                present = !devices.is_empty();
                let selected = choice.read().unwrap().clone();
                if requires_selection(
                    &devices
                        .iter()
                        .map(|d| d.path().to_string_lossy().into_owned())
                        .collect::<Vec<_>>(),
                    selected.as_deref(),
                ) {
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
                    return Err("Multiple remotes are connected. Select one in Connection.".into());
                }
                let info = devices
                    .into_iter()
                    .find(|d| {
                        selected
                            .as_ref()
                            .is_none_or(|s| d.path().to_string_lossy() == s.as_str())
                    })
                    .ok_or("Remote not connected. Wake it up or discover a new device.")?;
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
                    );
                }
                #[cfg(target_os = "macos")]
                api.set_open_exclusive(false);
                let device = info.open_device(&api).map_err(|e|
                    format!("Could not observe the MR-01: {e}. Check Input Monitoring and make sure Karabiner is not capturing this remote."))?;
                let mut descriptor = [0; 4096];
                let count = device
                    .get_report_descriptor(&mut descriptor)
                    .map_err(|e| e.to_string())?;
                std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
                std::fs::write(root.join("hid-descriptor.bin"), &descriptor[..count])
                    .map_err(|e| e.to_string())?;
                tx.try_send(Event::HidReady { exclusive })
                    .map_err(|_| "Input queue full".to_string())?;
                last_error.clear();
                let mut bytes = [0; 4096];
                let result = (|| {
                    while !stop.load(Ordering::Relaxed) {
                        #[cfg(target_os = "macos")]
                        service_tick(&mut broker, &mut helper_available, &tx);
                        let current = mapping.read().unwrap().clone();
                        if current.is_some() != exclusive {
                            break;
                        }
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
                    service_tick(&mut broker, &mut helper_available, &tx);
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
) {
    if let Some(broker) = broker {
        if let Err(e) = broker.tick() {
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
) -> Result<(), String> {
    let mut router = crate::direct::Router::default();
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
            let Some(report) = broker.read()? else {
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
            let at = Instant::now();
            for output in router.process(&bytes, at, &config) {
                if let crate::direct::Output::Action(button, profile) = output {
                    tx.try_send(Event::DirectAction {
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
            tx.try_send(Event::Packet { at, bytes })
                .map_err(|_| "Queue full")?;
        }
        Ok(())
    })();
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

pub fn discover_and_connect(tx: Events, root: PathBuf, selected: Option<String>) {
    std::thread::spawn(move || {
        let result = tokio::runtime::Runtime::new().map_err(|e|e.to_string()).and_then(|rt| rt.block_on(async {
            // Includes manager, adapter, properties and service queries, not just connect().
            let peripheral = bounded(Duration::from_secs(60), async {
                let manager = Manager::new().await.map_err(|e|e.to_string())?;
                let adapter = manager.adapters().await.map_err(|e|e.to_string())?.into_iter().next().ok_or("No Bluetooth adapter")?;
                let state = adapter.adapter_state().await.map_err(|e|e.to_string())?;
                let _ = tx.send(Event::BluetoothState(format!("{state:?}")));
                if state == btleplug::api::CentralState::PoweredOff { return Err("Bluetooth is off. Turn it on in System Settings → Bluetooth.".into()); }
                adapter.start_scan(ScanFilter::default()).await.map_err(|e|e.to_string())?;
                log(&tx,"Searching for 15 seconds. For first-time pairing, hold Back + OK until the LED flashes.");
                let result = async {
                    let mut found = std::collections::BTreeMap::new();
                    for _ in 0..30 {
                        for peripheral in adapter.peripherals().await.map_err(|e|e.to_string())? {
                            let props = peripheral.properties().await.map_err(|e|e.to_string())?;
                            if props.as_ref().and_then(|p|p.local_name.as_deref()) == Some(REMOTE_NAME) {
                                found.insert(format!("{:?}",peripheral.id()),peripheral);
                            }
                        }
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                    let id = match selected {
                        Some(id) => id,
                        None if found.len() == 1 => found.keys().next().unwrap().clone(),
                        None if found.len() > 1 => {
                            let _ = tx.send(Event::BleCandidates(found.keys().map(|id|(id.clone(),REMOTE_NAME.to_string())).collect()));
                            log(&tx,"Multiple remotes found. Select the one you want to connect.");
                            return Ok(None);
                        }
                        _ => return Err("Remote not found. Check that the LED is flashing and the remote is not connected to your phone.".into()),
                    };
                    let peripheral = found.remove(&id).ok_or("The selected remote is no longer available. Search again.")?;
                    if !peripheral.is_connected().await.map_err(|e|e.to_string())? {
                        bounded(Duration::from_secs(15), async { peripheral.connect().await.map_err(|e|e.to_string()) }).await?;
                    }
                    bounded(Duration::from_secs(10), async { peripheral.discover_services().await.map_err(|e|e.to_string()) }).await?;
                    let services: Vec<_> = peripheral.services().into_iter().map(|s|s.uuid.to_string()).collect();
                    std::fs::create_dir_all(&root).map_err(|e|e.to_string())?;
                    let evidence = serde_json::json!({"name":REMOTE_NAME,"id":id,"services":services,"audio_confirmed":false});
                    std::fs::write(root.join("connection.json"),serde_json::to_vec_pretty(&evidence).unwrap()).map_err(|e|e.to_string())?;
                    Ok::<_,String>(Some(peripheral))
                }.await;
                let _ = tokio::time::timeout(Duration::from_secs(2),adapter.stop_scan()).await;
                result.map(|p| p.map(|peripheral| (peripheral, manager, adapter)))
            }).await?;
            if let Some((peripheral, _manager, _adapter)) = peripheral {
                log(&tx,"Bluetooth connected. Waiting for the macOS HID device…");
                let _ = tx.send(Event::BleConnected);
                loop {
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    if tx.send(Event::Log(String::new())).is_err() { return Ok(()); }
                    if !bounded(Duration::from_secs(5), async { peripheral.is_connected().await.map_err(|e|e.to_string()) }).await? { break; }
                }
            }
            Ok::<(),String>(())
        }));
        if let Err(e) = result {
            log(&tx, format!("Bluetooth: {e}"));
        }
        let _ = tx.send(Event::BleDone);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn disappeared_selection_offers_remaining_remote_without_switching() {
        assert!(requires_selection(&["B".into()], Some("A")));
        assert!(!requires_selection(&["B".into()], Some("B")));
        assert!(!requires_selection(&["B".into()], None));
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
