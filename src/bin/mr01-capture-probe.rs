//! Bounded feasibility probe. Does not read/write reports, install services,
//! execute actions, or open any device except a uniquely identified MR-01.
use std::{collections::BTreeMap, time::Duration};

fn main() {
    if let Err(error) = run() {
        eprintln!("PROBE FAILED: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args != ["--exclusive-250ms"] {
        return Err("Usage: mr01-capture-probe --exclusive-250ms".into());
    }
    #[cfg(not(target_os = "macos"))]
    return Err("This probe requires macOS".into());
    #[cfg(target_os = "macos")]
    {
        let api = hidapi::HidApi::new().map_err(|e| e.to_string())?;
        let devices: BTreeMap<_, _> = api
            .device_list()
            .filter(|d| {
                d.product_string() == Some("ReadyForAirRemote")
                    && d.vendor_id() == 0x5f00
                    && d.product_id() == 0
            })
            .map(|d| (d.path().to_owned(), d))
            .collect();
        if devices.len() != 1 {
            return Err(format!(
                "Expected exactly one physical MR-01, found {}",
                devices.len()
            ));
        }
        let info = devices.values().next().unwrap();
        api.set_open_exclusive(false);
        let shared = info
            .open_device(&api)
            .map_err(|e| format!("shared open: {e}"))?;
        drop(shared);
        println!("SHARED OPEN OK");
        api.set_open_exclusive(true);
        let opened = info.open_device(&api);
        api.set_open_exclusive(false);
        let exclusive = opened.map_err(|e| format!("exclusive open: {e}"))?;
        // The remote alone is unavailable to macOS during this fixed interval.
        // Process termination also closes the handle. No packet payloads are read.
        std::thread::sleep(Duration::from_millis(250));
        drop(exclusive);
        println!("EXCLUSIVE OPEN OK; MR-01 released after 250 ms");
        Ok(())
    }
}
