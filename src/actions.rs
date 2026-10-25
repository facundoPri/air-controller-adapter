//! Explicit, bounded platform action execution. No actions run while loading a profile.
use crate::studio::{Action, SystemAction};
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

const TIMEOUT: Duration = Duration::from_secs(15);
const MAX_STEPS: usize = 64;

pub fn execute(action: &Action) -> Result<String, String> {
    execute_with_context(
        action,
        &Execution {
            deadline: Instant::now() + TIMEOUT,
            cancellation: None,
            #[cfg(feature = "virtual-keyboard")]
            keyboard: None,
            #[cfg(feature = "virtual-keyboard")]
            keyboard_generation: std::cell::Cell::new(None),
            #[cfg(feature = "virtual-keyboard")]
            keyboard_opener: None,
        },
    )
}

pub fn execute_cancellable(
    action: &Action,
    generation: &AtomicU64,
    epoch: u64,
) -> Result<String, String> {
    execute_with_context(
        action,
        &Execution {
            deadline: Instant::now() + TIMEOUT,
            cancellation: Some((generation, epoch)),
            #[cfg(feature = "virtual-keyboard")]
            keyboard: None,
            #[cfg(feature = "virtual-keyboard")]
            keyboard_generation: std::cell::Cell::new(None),
            #[cfg(feature = "virtual-keyboard")]
            keyboard_opener: None,
        },
    )
}

#[cfg(feature = "virtual-keyboard")]
pub fn execute_with_keyboard(
    action: &Action,
    generation: &AtomicU64,
    epoch: u64,
    keyboard: &crate::keyboard::Control,
    opener: Option<crate::studio::ButtonId>,
    session: Option<u64>,
) -> Result<String, String> {
    execute_with_context(
        action,
        &Execution {
            deadline: Instant::now() + TIMEOUT,
            cancellation: Some((generation, epoch)),
            keyboard: Some(keyboard),
            keyboard_generation: std::cell::Cell::new(session),
            keyboard_opener: opener,
        },
    )
}

struct Execution<'a> {
    deadline: Instant,
    cancellation: Option<(&'a AtomicU64, u64)>,
    #[cfg(feature = "virtual-keyboard")]
    keyboard: Option<&'a crate::keyboard::Control>,
    #[cfg(feature = "virtual-keyboard")]
    keyboard_generation: std::cell::Cell<Option<u64>>,
    #[cfg(feature = "virtual-keyboard")]
    keyboard_opener: Option<crate::studio::ButtonId>,
}
impl Execution<'_> {
    fn check(&self) -> Result<(), String> {
        #[cfg(feature = "virtual-keyboard")]
        if let (Some(keyboard), Some(generation)) = (self.keyboard, self.keyboard_generation.get())
        {
            if keyboard.snapshot().generation != generation {
                return Err(
                    "Keyboard target or session changed; remaining macro steps canceled".into(),
                );
            }
        }
        if self
            .cancellation
            .is_some_and(|(generation, epoch)| generation.load(Ordering::Acquire) != epoch)
        {
            return Err(
                "Action canceled because actions were disabled or the configuration changed".into(),
            );
        }
        if Instant::now() >= self.deadline {
            return Err("The action exceeded 15 seconds and was stopped".into());
        }
        Ok(())
    }
}
fn execute_with_context(action: &Action, context: &Execution<'_>) -> Result<String, String> {
    context.check()?;
    validate(action, 0, &mut 0)?;
    if !cfg!(target_os = "macos") {
        return Err("Desktop actions currently require macOS".into());
    }
    check_zoom_requirement(action, zoom_shortcuts_enabled)?;
    execute_inner(action, context)?;
    Ok("Request completed; the effect depends on macOS and the target application".into())
}

/// Use the same executable contract when saving a mapping.
pub fn validate_definition(action: &Action) -> Result<(), String> {
    validate(action, 0, &mut 0)
}

pub fn uses_zoom(action: &Action) -> bool {
    match action {
        Action::System {
            command: SystemAction::ZoomIn | SystemAction::ZoomOut | SystemAction::ZoomToggle,
        } => true,
        Action::Macro { steps } => steps.iter().any(uses_zoom),
        _ => false,
    }
}

/// User-facing contract, including dependencies that posting an event cannot
/// validate (focused app and output device).
pub fn guidance(action: &Action) -> &'static str {
    match action {
        #[cfg(feature = "virtual-keyboard")]
        Action::Keyboard { .. } => {
            "Controls ACA's keyboard. The opener closes it across layers. Home or Capture preserves arrows and OK. A binding only in Keyboard cannot open it. Preview does not type."
        }
        Action::Default => {
            "Preserves the original input. Home, Apps, Experiences, and Capture are Android functions with no original macOS equivalent; assign them an action. Back depends on the remote's input signal."
        }
        Action::Disabled => {
            "Ignores this button in the selected profile. Does not disable the firmware's mouse mode."
        }
        Action::System { command } => match command {
            SystemAction::ZoomIn | SystemAction::ZoomOut | SystemAction::ZoomToggle => {
                "Uses macOS screen zoom. Enable System Settings → Accessibility → Zoom → Use keyboard shortcuts to zoom. Uses ⌥⌘8, ⌥⌘=, and ⌥⌘−; the zoomed area depends on the system Zoom settings."
            }
            SystemAction::MissionControl => {
                "Shows windows and desktops using macOS Mission Control."
            }
            SystemAction::AppExpose => {
                "Shows windows of the focused application. There may be no visible change if it has no available windows."
            }
            SystemAction::ShowDesktop => {
                "Toggles between showing the desktop and restoring windows using Mission Control."
            }
            SystemAction::Launchpad => {
                "Opens Apps on macOS Tahoe or Launchpad on earlier versions. Presentation depends on macOS."
            }
            SystemAction::OnScreenKeyboard => {
                "Shows the macOS Accessibility Keyboard. System Settings opens briefly, then focus returns to your app. Requires Accessibility and permission to control System Events. Use Air mouse + OK to click keys; arrow-key navigation is not provided. Close the keyboard with its close button."
            }
            SystemAction::VolumeUp | SystemAction::VolumeDown => {
                "Changes output volume by 6 points (0–100). Does not change mute state. Some HDMI outputs, digital outputs, and audio interfaces require their own controls."
            }
            SystemAction::Mute => {
                "Toggles mute for the current audio output. Some external outputs do not support macOS volume or mute controls."
            }
            SystemAction::DisplaySleep => {
                "Turns off the displays immediately. The Mac stays on; moving the mouse or pressing a key may wake the displays."
            }
            SystemAction::Sleep => {
                "Requests immediate Mac sleep. macOS may prevent or delay sleep depending on system activity."
            }
            SystemAction::LockScreen => {
                "Sends ⌃⌘Q to lock the session. Requires Accessibility and permission to control System Events."
            }
        },
        Action::Shortcut { key, modifiers }
            if modifiers.is_empty() && matches!(key.as_str(), "page_up" | "page_down") =>
        {
            "Sends Page Up / Page Down to the focused application. Scrolling depends on that application and the active field, regardless of cursor position. Requires Accessibility and permission to control System Events."
        }
        Action::Shortcut { .. } => {
            "Sends the shortcut to the focused application. Copy requires a selection; paste uses the clipboard. Requires Accessibility and permission to control System Events. Symbol keys depend on your keyboard layout."
        }
        Action::Scroll { .. } => {
            "Scrolls in pixels at the cursor location; 3 px is a very small movement. To move by a page, select Page Up / Page Down. Existing JSON values are preserved."
        }
        Action::Macro { .. } => {
            "Runs steps in order with a total limit of 15 seconds. System effects may finish later; animations are not awaited between steps."
        }
        Action::AppleScript { .. } => {
            "Runs your AppleScript with your user permissions. macOS may request permission to control each target application. Limit: 15 seconds."
        }
        Action::Shell { .. } => {
            "Runs the command with zsh without loading your shell configuration. Use absolute paths: PATH may differ from Terminal. Limit: 15 seconds."
        }
    }
}

fn check_zoom_requirement(
    action: &Action,
    read: impl FnOnce() -> Option<bool>,
) -> Result<(), String> {
    if !uses_zoom(action) {
        return Ok(());
    }
    match read() {
        Some(true) => Ok(()),
        Some(false) => Err("Zoom not sent: enable “Use keyboard shortcuts to zoom” in System Settings → Accessibility → Zoom.".into()),
        None => Err("Could not verify Zoom. Check “Use keyboard shortcuts to zoom” in System Settings → Accessibility → Zoom.".into()),
    }
}

/// Read-only preference probe. No setting is enabled on the user's behalf.
/// Missing/unreadable values are unknown, not a claim that zoom is available.
pub fn zoom_shortcuts_enabled() -> Option<bool> {
    #[cfg(target_os = "macos")]
    unsafe {
        use native::*;
        let domain = CFStringCreateWithCString(
            std::ptr::null(),
            c"com.apple.universalaccess".as_ptr(),
            0x08000100,
        );
        let key = CFStringCreateWithCString(
            std::ptr::null(),
            c"closeViewHotkeysEnabled".as_ptr(),
            0x08000100,
        );
        if domain.is_null() || key.is_null() {
            if !domain.is_null() {
                CFRelease(domain);
            }
            if !key.is_null() {
                CFRelease(key);
            }
            return None;
        }
        // This process never writes this preference; synchronize refreshes cached
        // values after the user changes Zoom while Studio stays open.
        CFPreferencesAppSynchronize(domain);
        let mut valid = 0u8;
        let enabled = CFPreferencesGetAppBooleanValue(key, domain, &mut valid);
        CFRelease(key);
        CFRelease(domain);
        (valid != 0).then_some(enabled != 0)
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

fn validate(action: &Action, depth: usize, count: &mut usize) -> Result<(), String> {
    *count += 1;
    if depth > 4 || *count > MAX_STEPS {
        return Err("Macro too large: maximum 64 steps and 4 levels".into());
    }
    match action {
        #[cfg(feature = "virtual-keyboard")]
        Action::Keyboard { command }
            if depth > 0 && *command != crate::keyboard_model::KeyboardAction::Toggle =>
        {
            return Err("Only Toggle keyboard can be used inside a macro".into());
        }
        Action::Macro { steps } => {
            for step in steps {
                validate(step, depth + 1, count)?;
            }
        }
        Action::Shell { command } | Action::AppleScript { script: command }
            if command.len() > 65536 =>
        {
            return Err("Script exceeds 64 KiB".into());
        }
        Action::Shortcut { key, modifiers } => {
            shortcut_script(key, modifiers)?;
        }
        Action::Scroll { x, y } if x.unsigned_abs() > 10000 || y.unsigned_abs() > 10000 => {
            return Err("Scroll out of range".into());
        }
        _ => {}
    }
    Ok(())
}

fn execute_inner(action: &Action, context: &Execution<'_>) -> Result<(), String> {
    context.check()?;
    match action {
        #[cfg(feature = "virtual-keyboard")]
        Action::Keyboard { command } => {
            let keyboard = context
                .keyboard
                .ok_or("Keyboard action requires the active ACA runtime")?;
            if *command != crate::keyboard_model::KeyboardAction::Toggle {
                return Err(
                    "Navigation and selection require a physical keyboard-session press".into(),
                );
            }
            let expected = context
                .keyboard_generation
                .get()
                .unwrap_or_else(|| keyboard.snapshot().generation);
            context.check()?;
            let generation =
                keyboard.toggle_for(expected, context.keyboard_opener, Instant::now())?;
            let result = keyboard.wait(
                generation,
                context
                    .deadline
                    .min(Instant::now() + Duration::from_millis(500)),
                || {
                    context
                        .cancellation
                        .is_some_and(|(g, e)| g.load(Ordering::Acquire) != e)
                },
            );
            if result.is_ok() {
                context.keyboard_generation.set(Some(generation));
            }
            result
        }
        Action::Default | Action::Disabled => Ok(()),
        Action::Shortcut { key, modifiers } => {
            apple_script(&shortcut_script(key, modifiers)?, context)
        }
        Action::AppleScript { script } => apple_script(script, context),
        Action::Shell { command } => run(
            Command::new("/bin/zsh").args(["-f", "-c", command]),
            None,
            context,
        ),
        Action::Macro { steps } => {
            for step in steps {
                execute_inner(step, context)?;
            }
            Ok(())
        }
        Action::System { command } => system(command, context),
        Action::Scroll { x, y } => scroll(*x, *y),
    }
}

fn run(command: &mut Command, input: Option<&str>, context: &Execution<'_>) -> Result<(), String> {
    context.check()?;
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let child = command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut running = crate::process::Running::new(child);
    let child = &mut running.child;
    // Drain stderr continuously, retaining only 4 KiB. A verbose script cannot
    // fill the pipe and deadlock the worker; error messages keep the real cause.
    let stderr = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let captured = stderr.clone();
    let mut stream = child.stderr.take().ok_or("Could not open stderr")?;
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut chunk = [0u8; 1024];
        while let Ok(n) = stream.read(&mut chunk) {
            if n == 0 {
                break;
            }
            let mut bytes = captured.lock().unwrap();
            let keep = n.min(4096usize.saturating_sub(bytes.len()));
            bytes.extend_from_slice(&chunk[..keep]);
        }
        let _ = done_tx.send(());
    });
    if let Some(input) = input {
        let bytes = input.as_bytes().to_vec();
        let mut stdin = child.stdin.take().ok_or("Could not open stdin")?;
        std::thread::spawn(move || {
            let _ = stdin.write_all(&bytes);
        });
    }
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            running.stop();
            return if status.success() {
                Ok(())
            } else {
                let _ = done_rx.recv_timeout(Duration::from_millis(100));
                let bytes = stderr.lock().unwrap();
                let detail: String = String::from_utf8_lossy(&bytes)
                    .chars()
                    .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
                    .collect();
                Err(format!("Action exited with {status}: {}", detail.trim()))
            };
        }
        if let Err(reason) = context.check() {
            running.stop();
            return Err(reason);
        }
        std::thread::sleep(Duration::from_millis(15));
    }
}

fn apple_script(script: &str, context: &Execution<'_>) -> Result<(), String> {
    run(
        Command::new("/usr/bin/osascript").arg("-"),
        Some(script),
        context,
    )
}

fn shortcut_script(key: &str, modifiers: &[String]) -> Result<String, String> {
    let code = key_code(key).ok_or_else(|| format!("Unsupported key: {key}"))?;
    let mut mapped = Vec::new();
    for modifier in modifiers {
        mapped.push(match modifier.as_str() {
            "command" | "cmd" | "super" => "command down",
            "control" | "ctrl" => "control down",
            "option" | "alt" => "option down",
            "shift" => "shift down",
            _ => return Err(format!("Unsupported modifier: {modifier}")),
        });
    }
    let suffix = if mapped.is_empty() {
        String::new()
    } else {
        format!(" using {{{}}}", mapped.join(", "))
    };
    Ok(format!(
        "tell application \"System Events\" to key code {code}{suffix}"
    ))
}

fn key_code(key: &str) -> Option<u16> {
    Some(match key.to_ascii_lowercase().as_str() {
        "a" => 0,
        "s" => 1,
        "d" => 2,
        "f" => 3,
        "h" => 4,
        "g" => 5,
        "z" => 6,
        "x" => 7,
        "c" => 8,
        "v" => 9,
        "b" => 11,
        "q" => 12,
        "w" => 13,
        "e" => 14,
        "r" => 15,
        "y" => 16,
        "t" => 17,
        "1" => 18,
        "2" => 19,
        "3" => 20,
        "4" => 21,
        "6" => 22,
        "5" => 23,
        "equal_sign" | "=" => 24,
        "9" => 25,
        "7" => 26,
        "hyphen" | "-" => 27,
        "8" => 28,
        "0" => 29,
        "close_bracket" | "]" => 30,
        "o" => 31,
        "u" => 32,
        "open_bracket" | "[" => 33,
        "i" => 34,
        "p" => 35,
        "return_or_enter" | "enter" | "return" => 36,
        "l" => 37,
        "j" => 38,
        "quote" | "'" => 39,
        "k" => 40,
        "semicolon" | ";" => 41,
        "backslash" | "\\" => 42,
        "comma" | "," => 43,
        "slash" | "/" => 44,
        "n" => 45,
        "m" => 46,
        "period" | "." => 47,
        "tab" => 48,
        "spacebar" | "space" => 49,
        "grave_accent_and_tilde" | "`" => 50,
        "delete_or_backspace" | "backspace" => 51,
        "escape" | "esc" => 53,
        "f1" => 122,
        "f2" => 120,
        "f3" => 99,
        "f4" => 118,
        "f5" => 96,
        "f6" => 97,
        "f7" => 98,
        "f8" => 100,
        "f9" => 101,
        "f10" => 109,
        "f11" => 103,
        "f12" => 111,
        "home" => 115,
        "end" => 119,
        "page_up" => 116,
        "page_down" => 121,
        "delete_forward" | "delete" => 117,
        "left_arrow" | "left" => 123,
        "right_arrow" | "right" => 124,
        "down_arrow" | "down" => 125,
        "up_arrow" | "up" => 126,
        _ => return None,
    })
}

fn applications_path(exists: impl Fn(&str) -> bool) -> Result<&'static str, String> {
    [
        "/System/Applications/Apps.app",
        "/System/Applications/Launchpad.app",
    ]
    .into_iter()
    .find(|path| exists(path))
    .ok_or_else(|| {
        "This macOS version provides neither Apps nor Launchpad in System Applications".into()
    })
}

fn system(action: &SystemAction, context: &Execution<'_>) -> Result<(), String> {
    let script = match action {
        SystemAction::MissionControl => {
            return run(
                Command::new("/usr/bin/open").arg("/System/Applications/Mission Control.app"),
                None,
                context,
            );
        }
        SystemAction::AppExpose | SystemAction::ShowDesktop => {
            // Use Launch Services, not the inner Mach-O (macOS rejects direct
            // launch with SIGKILL/Code Signature Invalid on Tahoe). A fresh
            // instance guarantees the requested argument is delivered.
            return run(
                Command::new("/usr/bin/open").args([
                    "-n",
                    "/System/Applications/Mission Control.app",
                    "--args",
                    if *action == SystemAction::ShowDesktop {
                        "1"
                    } else {
                        "2"
                    },
                ]),
                None,
                context,
            );
        }
        SystemAction::Launchpad => {
            return run(
                Command::new("/usr/bin/open")
                    .arg(applications_path(|p| std::path::Path::new(p).is_dir())?),
                None,
                context,
            );
        }
        SystemAction::OnScreenKeyboard => {
            return apple_script(include_str!("on_screen_keyboard.applescript"), context);
        }
        SystemAction::VolumeUp => {
            "set level to (output volume of (get volume settings)) + 6\nif level > 100 then set level to 100\nset volume output volume level"
        }
        SystemAction::VolumeDown => {
            "set level to (output volume of (get volume settings)) - 6\nif level < 0 then set level to 0\nset volume output volume level"
        }
        SystemAction::Mute => {
            "set volume output muted (not (output muted of (get volume settings)))"
        }
        SystemAction::ZoomIn => {
            "tell application \"System Events\" to key code 24 using {command down, option down}"
        }
        SystemAction::ZoomOut => {
            "tell application \"System Events\" to key code 27 using {command down, option down}"
        }
        SystemAction::ZoomToggle => {
            "tell application \"System Events\" to key code 28 using {command down, option down}"
        }
        SystemAction::DisplaySleep => {
            return run(
                Command::new("/usr/bin/pmset").arg("displaysleepnow"),
                None,
                context,
            );
        }
        SystemAction::Sleep => "tell application \"System Events\" to sleep",
        SystemAction::LockScreen => {
            "tell application \"System Events\" to key code 12 using {command down, control down}"
        }
    };
    apple_script(script, context)
}

#[cfg(target_os = "macos")]
mod native {
    use std::ffi::c_void;
    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        pub fn AXIsProcessTrusted() -> bool;
        pub fn CGEventCreateScrollWheelEvent(
            source: *const c_void,
            units: u32,
            count: u32,
            ...
        ) -> *mut c_void;
        pub fn CGEventPost(location: u32, event: *mut c_void);
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        pub fn CFRelease(object: *const c_void);
        pub fn CFStringCreateWithCString(
            allocator: *const c_void,
            string: *const std::ffi::c_char,
            encoding: u32,
        ) -> *const c_void;
        pub fn CFPreferencesAppSynchronize(application: *const c_void) -> u8;
        pub fn CFPreferencesGetAppBooleanValue(
            key: *const c_void,
            application: *const c_void,
            valid: *mut u8,
        ) -> u8;
    }
}

fn scroll(x: i32, y: i32) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    unsafe {
        if !native::AXIsProcessTrusted() {
            return Err("Accessibility permission is required to scroll".into());
        }
        let event = native::CGEventCreateScrollWheelEvent(std::ptr::null(), 0, 2, y, x);
        if event.is_null() {
            return Err("Could not create the scroll event".into());
        }
        native::CGEventPost(0, event);
        native::CFRelease(event);
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (x, y);
        Err("Scrolling requires macOS".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zoom_disabled_or_unknown_is_not_reported_as_executed() {
        let zoom = Action::System {
            command: SystemAction::ZoomToggle,
        };
        assert!(
            check_zoom_requirement(&zoom, || Some(false))
                .unwrap_err()
                .contains("Zoom not sent")
        );
        assert!(check_zoom_requirement(&zoom, || None).is_err());
        assert!(check_zoom_requirement(&zoom, || Some(true)).is_ok());
        // Preflight is done before the first step, not after earlier macro effects.
        let nested = Action::Macro {
            steps: vec![
                Action::System {
                    command: SystemAction::LockScreen,
                },
                Action::Macro { steps: vec![zoom] },
            ],
        };
        assert!(check_zoom_requirement(&nested, || Some(false)).is_err());
        assert!(
            check_zoom_requirement(&Action::Default, || panic!(
                "Unrelated actions must not read Zoom"
            ))
            .is_ok()
        );
    }
    #[test]
    fn page_shortcuts_match_macos_virtual_keys() {
        for (key, code) in [("page_up", 116), ("page_down", 121)] {
            assert_eq!(
                shortcut_script(key, &[]).unwrap(),
                format!("tell application \"System Events\" to key code {code}")
            );
        }
    }
    #[test]
    #[ignore = "read-only probe of the current Mac; no events are posted"]
    fn inspect_zoom_preference() {
        println!("Zoom keyboard shortcuts: {:?}", zoom_shortcuts_enabled());
    }
    #[test]
    fn apps_uses_tahoe_launcher_and_falls_back_for_older_macos() {
        assert_eq!(
            applications_path(|_| true).unwrap(),
            "/System/Applications/Apps.app"
        );
        assert_eq!(
            applications_path(|p| p.ends_with("Launchpad.app")).unwrap(),
            "/System/Applications/Launchpad.app"
        );
        assert!(applications_path(|_| false).is_err());
    }
    #[test]
    fn subprocess_failure_reports_cause_not_a_permission_guess() {
        let e = execute(&Action::Shell {
            command: "printf 'fixture application missing' >&2; exit 7".into(),
        })
        .unwrap_err();
        assert!(e.contains("fixture application missing"));
        assert!(!e.contains("Accessibility"));
    }
    #[test]
    fn stale_generation_is_rejected_before_any_action() {
        let generation = AtomicU64::new(2);
        assert!(
            execute_cancellable(&Action::Default, &generation, 1)
                .unwrap_err()
                .contains("canceled")
        );
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn cancelling_running_macro_stops_child_and_skips_later_steps() {
        use std::sync::Arc;
        let root =
            std::env::temp_dir().join(format!("ready-air-action-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let started = root.join("started");
        let after = root.join("after");
        let quote =
            |p: &std::path::Path| format!("'{}'", p.to_str().unwrap().replace('\'', "'\\''"));
        let action = Action::Macro {
            steps: vec![
                Action::Shell {
                    command: format!("printf started > {}; /bin/sleep 5", quote(&started)),
                },
                Action::Shell {
                    command: format!("printf after > {}", quote(&after)),
                },
            ],
        };
        let generation = Arc::new(AtomicU64::new(0));
        let cancel = generation.clone();
        let marker = started.clone();
        let canceller = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(3);
            while !marker.exists() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            cancel.fetch_add(1, Ordering::Release);
        });
        let begin = Instant::now();
        let result = execute_cancellable(&action, &generation, 0);
        canceller.join().unwrap();
        assert!(started.exists(), "test subprocess did not start");
        assert!(result.unwrap_err().contains("canceled"));
        assert!(begin.elapsed() < Duration::from_secs(4));
        assert!(!after.exists(), "a later macro step ran after cancellation");
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn completed_shell_cannot_leave_background_effects() {
        let marker = std::env::temp_dir().join(format!("air-orphan-{}", uuid::Uuid::new_v4()));
        execute(&Action::Shell {
            command: format!("(sleep 0.3; touch '{}') & exit 0", marker.display()),
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(600));
        let leaked = marker.exists();
        let _ = std::fs::remove_file(marker);
        assert!(!leaked, "descendant survived completed action");
    }
    #[test]
    fn shortcut_rejects_script_injection() {
        assert!(shortcut_script("a\" & do shell script", &[]).is_err());
        assert!(shortcut_script("a", &["command down}; do shell script".into()]).is_err());
        assert_eq!(
            shortcut_script("up", &["ctrl".into()]).unwrap(),
            "tell application \"System Events\" to key code 126 using {control down}"
        );
    }
    #[test]
    fn validates_entire_macro_before_execution() {
        let action = Action::Macro {
            steps: vec![Action::Default; 65],
        };
        assert!(validate(&action, 0, &mut 0).is_err());
        let action = Action::Macro {
            steps: vec![
                Action::Default,
                Action::Shortcut {
                    key: "unknown-key".into(),
                    modifiers: vec![],
                },
            ],
        };
        assert!(validate(&action, 0, &mut 0).is_err());
    }
}

#[cfg(all(test, feature = "virtual-keyboard"))]
mod keyboard_tests {
    use super::*;
    use crate::{
        keyboard::{Command as KeyboardRequest, Control},
        keyboard_model::KeyboardAction,
        studio::ButtonId,
    };
    #[test]
    fn unsupported_macro_keyboard_step_rejected_before_any_effect() {
        let action = Action::Macro {
            steps: vec![
                Action::Shell {
                    command: "exit 0".into(),
                },
                Action::Keyboard {
                    command: KeyboardAction::Select,
                },
            ],
        };
        assert!(validate_definition(&action).is_err());
    }
    #[test]
    fn session_cancellation_rejects_queued_action_before_execution() {
        let control = Control::default();
        let t = Instant::now();
        let session = control.toggle(None, t).unwrap();
        control.drain(t);
        assert!(control.acknowledge_open(session));
        control.cancel(None);
        let result = execute_with_keyboard(
            &Action::Default,
            &AtomicU64::new(0),
            0,
            &control,
            Some(ButtonId::Home),
            Some(session),
        );
        assert!(result.unwrap_err().contains("session changed"));
    }
    #[test]
    fn nested_macro_waits_for_toggle_ack_and_retains_physical_opener() {
        let control = Control::default();
        let worker = control.clone();
        let generation = Arc::new(AtomicU64::new(0));
        let worker_generation = generation.clone();
        let thread = std::thread::spawn(move || {
            execute_with_keyboard(
                &Action::Macro {
                    steps: vec![
                        Action::Macro {
                            steps: vec![Action::Keyboard {
                                command: KeyboardAction::Toggle,
                            }],
                        },
                        Action::Default,
                    ],
                },
                &worker_generation,
                0,
                &worker,
                Some(ButtonId::Home),
                None,
            )
        });
        let deadline = Instant::now() + Duration::from_millis(400);
        let request = loop {
            if let Some(r) = control.drain(Instant::now()).into_iter().next() {
                break r;
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        };
        assert!(matches!(request.command, KeyboardRequest::Open));
        assert_eq!(control.snapshot().opener, Some(ButtonId::Home));
        assert!(!thread.is_finished());
        assert!(control.acknowledge_open(request.generation));
        assert!(thread.join().unwrap().is_ok());
    }
    #[test]
    fn canceled_macro_cannot_open_after_late_ack() {
        let control = Control::default();
        let worker = control.clone();
        let epoch = Arc::new(AtomicU64::new(0));
        let worker_epoch = epoch.clone();
        let thread = std::thread::spawn(move || {
            execute_with_keyboard(
                &Action::Keyboard {
                    command: KeyboardAction::Toggle,
                },
                &worker_epoch,
                0,
                &worker,
                None,
                None,
            )
        });
        let deadline = Instant::now() + Duration::from_millis(400);
        let request = loop {
            if let Some(r) = control.drain(Instant::now()).into_iter().next() {
                break r;
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        };
        epoch.store(1, Ordering::Release);
        assert!(thread.join().unwrap().is_err());
        assert!(!control.acknowledge_open(request.generation));
    }
    use std::sync::Arc;
}
