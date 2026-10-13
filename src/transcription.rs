//! Explicitly configured local speech-to-text command. Audio is never uploaded here.
use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

const OUTPUT_LIMIT: usize = 64 * 1024;
const TIMEOUT: Duration = Duration::from_secs(120);

pub fn transcribe(
    config: &crate::studio::TranscriptionCommand,
    audio: &Path,
) -> Result<String, String> {
    run_with_cancel(config, audio, TIMEOUT, None)
}

pub fn transcribe_cancellable(
    config: &crate::studio::TranscriptionCommand,
    audio: &Path,
    generation: &std::sync::atomic::AtomicU64,
    epoch: u64,
) -> Result<String, String> {
    run_with_cancel(config, audio, TIMEOUT, Some((generation, epoch)))
}

enum Captured {
    Complete(Vec<u8>),
    TooLarge,
    ReadError,
}

// Drain both pipes concurrently, but never retain unlimited command output.
fn capture(mut pipe: impl Read + Send + 'static) -> Receiver<Captured> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut chunk = [0; 4096];
        loop {
            match pipe.read(&mut chunk) {
                Ok(0) => {
                    let _ = tx.send(Captured::Complete(bytes));
                    return;
                }
                Ok(n) if bytes.len() + n <= OUTPUT_LIMIT => bytes.extend_from_slice(&chunk[..n]),
                Ok(_) => {
                    let _ = tx.send(Captured::TooLarge);
                    return;
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => {
                    let _ = tx.send(Captured::ReadError);
                    return;
                }
            }
        }
    });
    rx
}

fn collect(rx: &Receiver<Captured>) -> Result<Vec<u8>, String> {
    match rx.recv_timeout(Duration::from_secs(1)) {
        Ok(Captured::Complete(bytes)) => Ok(bytes),
        Ok(Captured::TooLarge) => {
            Err("La transcripción excedió el límite de salida de 64 KiB.".into())
        }
        _ => Err("No se pudo leer la salida del transcriptor local.".into()),
    }
}

#[cfg(test)]
fn run(
    config: &crate::studio::TranscriptionCommand,
    audio: &Path,
    timeout: Duration,
) -> Result<String, String> {
    run_with_cancel(config, audio, timeout, None)
}
fn run_with_cancel(
    config: &crate::studio::TranscriptionCommand,
    audio: &Path,
    timeout: Duration,
    cancellation: Option<(&std::sync::atomic::AtomicU64, u64)>,
) -> Result<String, String> {
    let check = || {
        if cancellation.is_some_and(|(g, e)| g.load(std::sync::atomic::Ordering::Acquire) != e) {
            Err("Transcripción cancelada".to_string())
        } else {
            Ok(())
        }
    };
    check()?;
    if config.program.trim().is_empty() {
        return Err("Configurá un ejecutable local de transcripción.".into());
    }
    if !config.args.iter().any(|arg| arg.contains("{audio}")) {
        return Err("Los argumentos de transcripción deben incluir {audio}.".into());
    }
    if !audio.is_file() {
        return Err("No se encontró la grabación para transcribir.".into());
    }
    let audio = audio.to_str().ok_or("La ruta del audio no es UTF-8.")?;
    let mut command = Command::new(&config.program);
    command
        .args(config.args.iter().map(|arg| arg.replace("{audio}", audio)))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let child = command.spawn().map_err(
        |_| "No se pudo iniciar el transcriptor local. Revisá el ejecutable y sus permisos.",
    )?;
    let mut running = crate::process::Running::new(child);
    let stdout = capture(
        running
            .child
            .stdout
            .take()
            .ok_or("Falta stdout del transcriptor.")?,
    );
    let stderr = capture(
        running
            .child
            .stderr
            .take()
            .ok_or("Falta stderr del transcriptor.")?,
    );
    let start = Instant::now();
    // Hold completed results while waiting for exit; overflow aborts immediately.
    let mut out = None;
    let mut err = None;
    let status = loop {
        check()?;
        for (rx, value) in [(&stdout, &mut out), (&stderr, &mut err)] {
            if value.is_none() {
                match rx.try_recv() {
                    Ok(Captured::Complete(bytes)) => *value = Some(bytes),
                    Ok(Captured::TooLarge) => {
                        return Err(
                            "La transcripción excedió el límite de salida de 64 KiB.".into()
                        );
                    }
                    Ok(Captured::ReadError) => {
                        return Err("No se pudo leer la salida del transcriptor local.".into());
                    }
                    Err(_) => {}
                }
            }
        }
        if let Some(status) = running
            .child
            .try_wait()
            .map_err(|_| "No se pudo consultar el transcriptor local.")?
        {
            break status;
        }
        if start.elapsed() >= timeout {
            return Err("El transcriptor local superó el tiempo máximo de espera.".into());
        }
        thread::sleep(Duration::from_millis(20));
    };
    // A wrapper may leave descendants holding pipe handles; terminate this group as well.
    running.stop();
    // Read pending capture results before interpreting the exit status. Closing an
    // overflowing pipe can cause SIGPIPE, racing the reader's TooLarge message.
    // The size violation remains the useful error even when the child exits first.
    let output = match out {
        Some(bytes) => bytes,
        None => collect(&stdout)?,
    };
    if err.is_none() {
        collect(&stderr)?;
    }
    if !status.success() {
        return Err(format!(
            "El transcriptor local terminó con error (código {:?}). Revisá su configuración.",
            status.code()
        ));
    }
    let output =
        String::from_utf8(output).map_err(|_| "El transcriptor devolvió texto que no es UTF-8.")?;
    let output = output.trim();
    if output.is_empty() {
        return Err("El transcriptor local no devolvió texto.".into());
    }
    Ok(output.to_owned())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SERIAL: AtomicU64 = AtomicU64::new(0);

    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new(
            script: &str,
        ) -> (
            Self,
            crate::studio::TranscriptionCommand,
            std::path::PathBuf,
        ) {
            let dir = std::env::temp_dir().join(format!(
                "ready-air-transcription-{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&dir).unwrap();
            let executable = dir.join("transcriber");
            std::fs::write(&executable, format!("#!/bin/sh\n{script}\n")).unwrap();
            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
            let audio = dir.join("audio $(touch SHOULD_NOT_EXIST); sample.wav");
            std::fs::write(&audio, b"fixture").unwrap();
            let config = crate::studio::TranscriptionCommand {
                program: executable.to_str().unwrap().into(),
                args: vec!["{audio}".into()],
                insert_text: false,
            };
            (Self(dir), config, audio)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn passes_audio_as_one_literal_argument() {
        let (_fixture, config, audio) =
            Fixture::new("test \"$#\" = 1 || exit 4\nprintf '%s' \"$1\"");
        assert_eq!(
            transcribe(&config, &audio).unwrap(),
            audio.to_str().unwrap()
        );
    }

    #[test]
    fn rejects_missing_placeholder_and_empty_output() {
        let (_fixture, mut config, audio) = Fixture::new("exit 0");
        assert!(
            transcribe(&config, &audio)
                .unwrap_err()
                .contains("no devolvió texto")
        );
        config.args.clear();
        assert!(transcribe(&config, &audio).unwrap_err().contains("{audio}"));
    }

    #[test]
    fn limits_output_and_execution_time() {
        // One bounded burst avoids thousands of shell writes competing with the
        // deadline under a parallel test run. Nonzero exit also covers the race
        // between a pipe's overflow notification and observing process exit.
        let (_fixture, config, audio) = Fixture::new(&oversized_output(false));
        let error = run(&config, &audio, Duration::from_secs(10)).unwrap_err();
        assert!(error.contains("64 KiB"), "unexpected error: {error}");
        let (_fixture, config, audio) = Fixture::new("sleep 10");
        let start = Instant::now();
        assert!(
            run(&config, &audio, Duration::from_millis(60))
                .unwrap_err()
                .contains("tiempo máximo")
        );
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn rejects_nonzero_exit_and_keeps_stderr_private() {
        let (_fixture, config, audio) = Fixture::new("printf 'sensitive diagnostic' >&2\nexit 7");
        let error = transcribe(&config, &audio).unwrap_err();
        assert!(error.contains("7"));
        assert!(!error.contains("sensitive"));
    }

    #[test]
    fn accepts_trimmed_text_and_rejects_invalid_utf8() {
        let (_fixture, config, audio) = Fixture::new("printf '  Hola, mundo.  \\n'");
        assert_eq!(transcribe(&config, &audio).unwrap(), "Hola, mundo.");
        let (_fixture, config, audio) = Fixture::new("printf '\\377'");
        assert!(transcribe(&config, &audio).unwrap_err().contains("UTF-8"));
    }

    #[test]
    fn limits_stderr_too() {
        let (_fixture, config, audio) = Fixture::new(&oversized_output(true));
        let error = run(&config, &audio, Duration::from_secs(10)).unwrap_err();
        assert!(error.contains("64 KiB"), "unexpected error: {error}");
    }

    #[test]
    fn cancellation_cleans_up_transcriber_and_descendants() {
        let (fixture, config, audio) =
            Fixture::new("(sleep 0.4; touch \"$1.marker\") & sleep 0.8; printf text");
        let generation = std::sync::Arc::new(AtomicU64::new(0));
        let cancel = generation.clone();
        let trigger = std::thread::spawn(move || {
            thread::sleep(Duration::from_millis(80));
            cancel.store(1, Ordering::Release);
        });
        let result = transcribe_cancellable(&config, &audio, &generation, 0);
        trigger.join().unwrap();
        thread::sleep(Duration::from_millis(600));
        assert!(result.is_err(), "cancelled transcription returned text");
        assert!(
            !fixture
                .0
                .join(format!(
                    "{}.marker",
                    audio.file_name().unwrap().to_str().unwrap()
                ))
                .exists()
        );
    }

    fn oversized_output(stderr: bool) -> String {
        let redirect = if stderr { " >&2" } else { "" };
        // 2^17 bytes, twice the configured cap, using only shell builtins.
        format!(
            "chunk=x\ni=0\nwhile [ \"$i\" -lt 17 ]; do chunk=\"$chunk$chunk\"; i=$((i + 1)); done\nprintf '%s' \"$chunk\"{redirect}\nexit 7"
        )
    }
}
