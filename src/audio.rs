//! Local recording from an explicitly selected system input. No remote audio protocol.
use crate::hardware::{Event, Events, log};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

pub const MAX_SECONDS: u64 = 60;

pub fn inputs() -> Result<Vec<String>, String> {
    cpal::default_host()
        .input_devices()
        .map_err(|e| e.to_string())?
        .map(|d| d.name().map_err(|e| e.to_string()))
        .collect()
}

pub fn start(tx: Events, root: PathBuf, input: Option<String>, stop: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let result = record(&tx, root, input, stop);
        let _ = tx.send(Event::AudioDone(result));
    });
}

fn record(
    tx: &Events,
    root: PathBuf,
    input: Option<String>,
    stop: Arc<AtomicBool>,
) -> Result<PathBuf, String> {
    let host = cpal::default_host();
    let device = if let Some(name) = input {
        let mut matches = host
            .input_devices()
            .map_err(|e| e.to_string())?
            .filter(|d| d.name().ok().as_deref() == Some(name.as_str()));
        let device = matches
            .next()
            .ok_or_else(|| format!("La entrada {name} no está disponible. Elegí otra entrada."))?;
        if matches.next().is_some() {
            return Err(format!(
                "Hay más de una entrada llamada {name}; elegí la predeterminada del sistema."
            ));
        }
        device
    } else {
        host.default_input_device()
            .ok_or("No hay entrada de audio predeterminada.")?
    };
    if stop.load(Ordering::Relaxed) {
        return Err("Grabación cancelada.".into());
    }
    let supported = device.default_input_config().map_err(|e| e.to_string())?;
    let name = device.name().map_err(|e| e.to_string())?;
    let config: cpal::StreamConfig = supported.clone().into();
    let samples = Arc::new(Mutex::new(Vec::<i16>::new()));
    // Bound callback storage even if the worker is delayed by the permission prompt.
    let limit = config.sample_rate.0 as usize * config.channels as usize * MAX_SECONDS as usize;
    let error = Arc::new(Mutex::new(None::<String>));
    let on_error = {
        let error = error.clone();
        move |e: cpal::StreamError| {
            *error.lock().unwrap() = Some(e.to_string());
        }
    };
    macro_rules! build {
        ($sample:ty, $convert:expr) => {{
            let samples = samples.clone();
            device
                .build_input_stream(
                    &config,
                    move |data: &[$sample], _| {
                        if let Ok(mut out) = samples.lock() {
                            let left = limit.saturating_sub(out.len());
                            out.extend(data.iter().take(left).map($convert));
                        }
                    },
                    on_error,
                    None,
                )
                .map_err(|e| e.to_string())?
        }};
    }
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => {
            build!(f32, |v: &f32| (v.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
        }
        cpal::SampleFormat::I16 => build!(i16, |v: &i16| *v),
        cpal::SampleFormat::U16 => build!(u16, |v: &u16| (*v as i32 - 32768) as i16),
        format => return Err(format!("Formato de audio aún no soportado: {format:?}")),
    };
    // A permission prompt can delay stream creation; honor Stop before playing.
    if stop.load(Ordering::Relaxed) {
        return Err("Grabación cancelada.".into());
    }
    stream.play().map_err(|e| e.to_string())?;
    let _ = tx.send(Event::AudioStarted(name.clone()));
    let started = Instant::now();
    while !stop.load(Ordering::Relaxed) && started.elapsed() < Duration::from_secs(MAX_SECONDS) {
        if error.lock().unwrap().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    if started.elapsed() >= Duration::from_secs(MAX_SECONDS) {
        log(
            tx,
            format!("Grabación detenida al alcanzar el límite de {MAX_SECONDS} segundos."),
        );
    }
    drop(stream);
    if let Some(e) = error.lock().unwrap().take() {
        return Err(e);
    }
    let samples = samples.lock().unwrap();
    if samples.is_empty() {
        return Err("No llegaron muestras. Revisá el permiso de Micrófono de la app.".into());
    }
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let path = root.join(format!("audio-{stamp}.wav"));
    let spec = hound::WavSpec {
        channels: config.channels,
        sample_rate: config.sample_rate.0,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::new(
        std::io::BufWriter::new(crate::storage::create_private(&path).map_err(|e| e.to_string())?),
        spec,
    )
    .map_err(|e| e.to_string())?;
    for sample in samples.iter() {
        writer.write_sample(*sample).map_err(|e| e.to_string())?;
    }
    writer.finalize().map_err(|e| e.to_string())?;
    crate::storage::write_private(&path.with_extension("json"), &serde_json::to_vec_pretty(&serde_json::json!({"source":name,"remote_microphone_confirmed":false,"samples":samples.len(),"sample_rate":spec.sample_rate,"channels":spec.channels})).unwrap()).map_err(|e| e.to_string())?;
    Ok(path)
}
