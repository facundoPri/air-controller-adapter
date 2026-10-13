//! Offline BTSnoop/H4 ACL decoder. Never connects to or writes a device.
//! With no connection handle, prints only inventory. With a decimal handle,
//! emits that handle's reassembled fixed-channel ATT PDUs as JSON lines.
//! EATT dynamic channels are counted but not decoded; never infer their absence.
use serde_json::json;
use std::{collections::BTreeMap, error::Error};

const EPOCH: u64 = 0x00dcddb30f2f8000;
fn le16(b: &[u8]) -> u16 {
    u16::from_le_bytes([b[0], b[1]])
}
fn be32(b: &[u8]) -> u32 {
    u32::from_be_bytes(b[..4].try_into().unwrap())
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[derive(Default)]
struct Assembly {
    bytes: Vec<u8>,
    timestamp: u64,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    if !(2..=3).contains(&args.len()) {
        return Err("usage: hci_att capture.log [decimal_connection_handle]".into());
    }
    let selected: Option<u16> = args.get(2).map(|s| s.parse()).transpose()?;
    let data = std::fs::read(&args[1])?;
    if data.len() < 16
        || &data[..8] != b"btsnoop\0"
        || be32(&data[8..]) != 1
        || be32(&data[12..]) != 1002
    {
        return Err("Requires BTSnoop version 1, H4 datalink 1002".into());
    }
    let mut position = 16;
    let mut records = 0;
    let mut truncated = 0;
    let mut dropped_max = 0;
    let mut incomplete = 0;
    let mut pending: BTreeMap<(u16, bool), Assembly> = BTreeMap::new();
    let mut channels: BTreeMap<(u16, u16, bool), usize> = BTreeMap::new();
    let mut connections = Vec::new();
    let mut first_us = None;
    let mut last_us = None;
    while position < data.len() {
        if data.len() - position < 24 {
            return Err("Truncated BTSnoop record header".into());
        }
        let h = &data[position..position + 24];
        let original = be32(h) as usize;
        let included = be32(&h[4..]) as usize;
        let flags = be32(&h[8..]);
        dropped_max = dropped_max.max(be32(&h[12..]));
        let timestamp = u64::from_be_bytes(h[16..24].try_into()?)
            .checked_sub(EPOCH)
            .ok_or("Timestamp before Unix epoch")?;
        first_us.get_or_insert(timestamp);
        last_us = Some(timestamp);
        position += 24;
        if included > data.len() - position {
            return Err("Truncated BTSnoop record body".into());
        }
        let packet = &data[position..position + included];
        position += included;
        records += 1;
        if original != included {
            truncated += 1;
        }
        if packet.len() >= 3 && packet[0] == 4 && packet.len() == packet[2] as usize + 3 {
            let event = &packet[3..];
            if packet[1] == 0x3e
                && event.len() >= 12
                && matches!(event[0], 1 | 0x0a | 0x29)
                && event[1] == 0
            {
                let handle = le16(&event[2..]);
                connections.push(json!({"unix_us":timestamp,"handle":handle,"event":event[0],"peer_suffix":format!("{:02x}:{:02x}",event[7],event[6])}));
                for direction in [false, true] {
                    if pending.remove(&(handle, direction)).is_some() {
                        incomplete += 1;
                    }
                }
            }
            if packet[1] == 5 && event.len() >= 4 && event[0] == 0 {
                let handle = le16(&event[1..]);
                if selected == Some(handle) {
                    println!(
                        "{}",
                        json!({"unix_us":timestamp,"handle":handle,"disconnected":true,"reason":event[3]})
                    );
                }
                for direction in [false, true] {
                    if pending.remove(&(handle, direction)).is_some() {
                        incomplete += 1;
                    }
                }
            }
            continue;
        }
        if packet.len() < 5 || packet[0] != 2 {
            continue;
        }
        let field = le16(&packet[1..]);
        let handle = field & 0xfff;
        let incoming = flags & 1 != 0;
        let key = (handle, incoming);
        let boundary = (field >> 12) & 3;
        if packet.len() - 5 != le16(&packet[3..]) as usize || original != included {
            pending.remove(&key);
            incomplete += 1;
            continue;
        }
        match boundary {
            0 | 2 | 3 => {
                if pending
                    .insert(
                        key,
                        Assembly {
                            bytes: packet[5..].to_vec(),
                            timestamp,
                        },
                    )
                    .is_some()
                {
                    incomplete += 1;
                }
            }
            1 => match pending.get_mut(&key) {
                Some(a) => a.bytes.extend_from_slice(&packet[5..]),
                None => {
                    incomplete += 1;
                    continue;
                }
            },
            _ => unreachable!(),
        }
        let a = pending.get(&key).unwrap();
        if a.bytes.len() < 4 {
            continue;
        }
        let expected = le16(&a.bytes) as usize + 4;
        if a.bytes.len() < expected {
            continue;
        }
        let a = pending.remove(&key).unwrap();
        if a.bytes.len() != expected {
            incomplete += 1;
            continue;
        }
        let cid = le16(&a.bytes[2..]);
        *channels.entry((handle, cid, incoming)).or_default() += 1;
        if selected == Some(handle) && cid == 4 {
            println!(
                "{}",
                json!({"unix_us":a.timestamp,"handle":handle,"direction":if incoming {"device_to_host"} else {"host_to_device"},"cid":cid,"att":hex(&a.bytes[4..])})
            );
        }
    }
    incomplete += pending.len();
    let counts: Vec<_> = channels
        .into_iter()
        .map(|((h, c, i), n)| json!({"handle":h,"cid":c,"incoming":i,"pdus":n}))
        .collect();
    let summary = json!({"records":records,"first_unix_us":first_us,"last_unix_us":last_us,"truncated_records":truncated,"dropped_max":dropped_max,"incomplete_acl":incomplete,"connections":connections,"channels":counts,"limitation":"Only fixed-channel ATT CID4 decoded; handles can be reused between connections. No GATT semantics inferred."});
    eprintln!("{summary}");
    Ok(())
}
