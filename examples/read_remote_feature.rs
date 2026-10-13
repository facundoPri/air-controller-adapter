//! Read-only probe for the observed MR-01 mouse Feature report.
//! Kept separate from the signed GUI app to preserve its existing TCC identity.
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let api = hidapi::HidApi::new()?;
    let mut devices = api.device_list().filter(|d| {
        d.product_string() == Some("ReadyForAirRemote")
            && d.vendor_id() == 0x5f00
            && d.product_id() == 0
    });
    let info = devices.next().ok_or("MR-01 not exposed as HID")?;
    let device = info.open_device(&api)?;
    let mut descriptor = [0; 4096];
    let length = device.get_report_descriptor(&mut descriptor)?;
    // Fail closed unless this is the exact descriptor inspected in this project.
    let observed = "05010906a1018501050719e029e71500250175019508810295017508810395057501050819012905910295017503910395067508150025ff0507190029ff8100c005010902a10185020901a1000509190129051500250195057501810295017503810305010930093109381580257f75089503810609011500250395017502b10295017506b103c0c0050c0901a101850375109501150126ff0319012aff038160c00612ff0a12ffa10185fc0901750895ff16000026ff00190029ff8100c00612ff0a12ffa10185fb0901750895ff16000026ff00190029ff8100950875080514190129089102c0";
    let actual: String = descriptor[..length]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if actual != observed {
        return Err("Descriptor differs from inspected MR-01; no Feature read attempted".into());
    }
    // Report ID 2 + one payload byte. No SET_REPORT, output, or GATT writes.
    let mut report = [2, 0];
    let length = device.get_feature_report(&mut report)?;
    if length != 2 || report[0] != 2 {
        return Err(format!("Unexpected Feature response: {:02x?}", &report[..length]).into());
    }
    println!(
        "Feature report 2: {:02x?}; raw low bits={} (meaning unknown)",
        report,
        report[1] & 3
    );
    Ok(())
}
