//! Foreground capture authorization. One request at a time, never a Terminal window.
use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

#[derive(Default)]
pub struct Launcher {
    child: Option<Child>,
    pub attempted: bool,
}
impl Launcher {
    pub fn pending(&self) -> bool {
        self.child.is_some()
    }
    pub fn start(&mut self, helper: &Path, socket: &Path) -> Result<(), String> {
        if self.pending() {
            return Ok(());
        }
        self.attempted = true;
        verify_packaged_helper(helper)?;
        #[cfg(unix)]
        let uid = unsafe { getuid() };
        #[cfg(not(unix))]
        let uid = 0;
        let script = authorization_script(
            helper,
            socket,
            uid,
            option_env!("READY_AIR_CAPTURE_SHA256").unwrap(),
        )?;
        let mut cmd = Command::new("/usr/bin/osascript");
        cmd.arg("-e").arg(script);
        self.spawn(&mut cmd)
    }
    fn spawn(&mut self, cmd: &mut Command) -> Result<(), String> {
        if self.pending() {
            return Ok(());
        }
        self.child = Some(
            cmd.stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| e.to_string())?,
        );
        Ok(())
    }
    pub fn poll(&mut self) -> Option<Result<(), String>> {
        let child = self.child.as_mut()?;
        match child.try_wait() {
            Ok(None) => None,
            Ok(Some(_)) => {
                let output = self.child.take().unwrap().wait_with_output();
                Some(output.map_err(|e| e.to_string()).and_then(|out| {
                    if out.status.success() {
                        Ok(())
                    } else {
                        let detail = String::from_utf8_lossy(&out.stderr);
                        if detail.contains("30-minute capture session ended") {
                            Err("The 30-minute capture session ended. Bluetooth may still be connected; use Resume reader to restore custom actions.".into())
                        } else {
                            Err(format!("The reader is not active: {}", detail.trim()))
                        }
                    }
                }))
            }
            Err(e) => {
                self.stop();
                Some(Err(e.to_string()))
            }
        }
    }
    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
impl Drop for Launcher {
    fn drop(&mut self) {
        self.stop();
    }
}
pub fn packaged_helper(executable: &Path) -> Result<PathBuf, String> {
    Ok(executable
        .ancestors()
        .nth(4)
        .ok_or("Studio app bundle not found")?
        .join("ACA Capture.app/Contents/MacOS/ReadyForAirCapture"))
}
pub fn verify_packaged_helper(helper: &Path) -> Result<(), String> {
    verify_helper(helper, option_env!("READY_AIR_CAPTURE_SHA256"))
}
fn helper_digest(path: &Path) -> Result<String, String> {
    let out = Command::new("/usr/bin/shasum")
        .args(["-a", "256"])
        .arg(path)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err("Could not verify Capture".into());
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .next()
        .unwrap_or("")
        .into())
}
fn verify_helper(helper: &Path, expected: Option<&str>) -> Result<(), String> {
    let expected = expected
        .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or("Studio has no embedded Capture identity. Build using the packaging script.")?;
    // Reject links in the executable and bundle components (not /tmp -> /private/tmp).
    for path in helper.ancestors().take(4) {
        if std::fs::symlink_metadata(path)
            .map_err(|_| "ACA Capture must be installed next to Studio")?
            .file_type()
            .is_symlink()
        {
            return Err("Capture cannot be a symbolic link".into());
        }
    }
    if !helper.is_file() {
        return Err("Capture is not a regular executable".into());
    }
    if helper_digest(helper)? != expected {
        return Err(
            "Capture does not match this version of Studio. Reinstall the complete package.".into(),
        );
    }
    Ok(())
}
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
#[cfg(unix)]
unsafe extern "C" {
    fn getuid() -> u32;
}
fn authorization_script(
    helper: &Path,
    socket: &Path,
    uid: u32,
    expected: &str,
) -> Result<String, String> {
    if uid == 0 {
        return Err("Studio must run as your user, not as administrator".into());
    }
    let helper = helper.to_str().ok_or("Invalid reader path")?;
    let socket = socket.to_str().ok_or("Invalid connection path")?;
    if expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Invalid Capture identity".into());
    }
    let command = staged_command(helper, socket, uid, expected)?;
    let literal = command.replace('\\', "\\\\").replace('"', "\\\"");
    Ok(format!(
        "with timeout of 1900 seconds\n do shell script \"{literal}\" with administrator privileges\nend timeout"
    ))
}
fn staged_command(helper: &str, socket: &str, uid: u32, expected: &str) -> Result<String, String> {
    let bundle = Path::new(helper)
        .ancestors()
        .nth(3)
        .and_then(Path::to_str)
        .ok_or("Invalid Capture bundle")?;
    // The elevated shell first copies into a private root-owned directory. Hash
    // verification AND execution target that immutable-to-the-user copy, closing
    // the replace-between-check-and-exec race on the editable source bundle.
    Ok(format!(
        r#"set -eu
umask 077
stage=$(/usr/bin/mktemp -d /private/tmp/ready-air-capture.XXXXXXXX)
trap '/bin/rm -rf "$stage"' EXIT HUP INT TERM
/bin/cp -R {bundle} "$stage/Capture.app"
for item in "$stage/Capture.app" "$stage/Capture.app/Contents" "$stage/Capture.app/Contents/MacOS" "$stage/Capture.app/Contents/MacOS/ReadyForAirCapture"; do
    test ! -L "$item" || exit 70
done
exe="$stage/Capture.app/Contents/MacOS/ReadyForAirCapture"
actual=$(/usr/bin/shasum -a 256 "$exe")
test "${{actual%% *}}" = '{expected}' || {{ echo 'Capture changed during authorization; execution rejected' >&2; exit 70; }}
/usr/bin/codesign --verify --strict "$stage/Capture.app"
/usr/bin/env SUDO_UID={uid} "$exe" --socket {socket}
"#,
        bundle = shell_quote(bundle),
        socket = shell_quote(socket)
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn substituted_helper_is_rejected_before_authorization() {
        let root = std::env::temp_dir().join(format!("rfa-helper-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let helper = root.join("Capture.app/Contents/MacOS/ReadyForAirCapture");
        std::fs::create_dir_all(helper.parent().unwrap()).unwrap();
        std::fs::write(&helper, "unexpected executable").unwrap();
        assert!(verify_helper(&helper, Some(&"0".repeat(64))).is_err());
        #[cfg(unix)]
        {
            let link = root.join("link");
            std::os::unix::fs::symlink(&helper, &link).unwrap();
            assert!(verify_helper(&link, Some(&helper_digest(&helper).unwrap())).is_err());
        }
        assert!(verify_helper(&helper, None).is_err());
        verify_helper(&helper, Some(&helper_digest(&helper).unwrap())).unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn elevated_stage_rejects_source_replaced_after_preflight_without_executing_it() {
        let root = std::env::temp_dir().join(format!("air-stage-{}", uuid::Uuid::new_v4()));
        let helper = root.join("Capture.app/Contents/MacOS/ReadyForAirCapture");
        std::fs::create_dir_all(helper.parent().unwrap()).unwrap();
        std::fs::write(&helper, "original").unwrap();
        let expected = helper_digest(&helper).unwrap();
        verify_helper(&helper, Some(&expected)).unwrap();
        std::fs::write(&helper, "replaced after preflight").unwrap();
        // Run only the staging shell as this test's ordinary user. No sudo/dialog.
        let script =
            staged_command(helper.to_str().unwrap(), "/unused.sock", 501, &expected).unwrap();
        let out = Command::new("/bin/sh")
            .arg("-c")
            .arg(script)
            .output()
            .unwrap();
        std::fs::remove_dir_all(root).unwrap();
        assert_eq!(out.status.code(), Some(70));
        assert!(String::from_utf8_lossy(&out.stderr).contains("execution rejected"));
    }
    #[test]
    fn repeated_start_keeps_one_process() {
        let mut launch = Launcher::default();
        launch.spawn(Command::new("/bin/sleep").arg("5")).unwrap();
        let id = launch.child.as_ref().unwrap().id();
        launch.spawn(Command::new("/bin/sleep").arg("5")).unwrap();
        assert_eq!(launch.child.as_ref().unwrap().id(), id);
        launch.stop();
        assert!(!launch.pending());
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn authorization_script_compiles_without_running_or_requesting_privileges() {
        use std::io::Write;
        let path = std::env::temp_dir().join(format!("air-auth-{}.scpt", uuid::Uuid::new_v4()));
        let script = authorization_script(
            Path::new("/tmp/Capture.app/Contents/MacOS/ReadyForAirCapture"),
            Path::new("/tmp/socket"),
            501,
            &"a".repeat(64),
        )
        .unwrap();
        let mut compiler = Command::new("/usr/bin/osacompile")
            .arg("-o")
            .arg(&path)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        compiler
            .stdin
            .take()
            .unwrap()
            .write_all(script.as_bytes())
            .unwrap();
        let output = compiler.wait_with_output().unwrap();
        let _ = std::fs::remove_file(path);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    #[test]
    fn paths_are_shell_and_applescript_quoted() {
        let script = authorization_script(
            Path::new("/tmp/a'b$()\"c/Contents/MacOS/ReadyForAirCapture"),
            Path::new("/tmp/socket"),
            501,
            &"a".repeat(64),
        )
        .unwrap();
        assert!(script.contains("'\\\\''"));
        assert!(script.contains("--socket '/tmp/socket'"));
        assert!(!script.contains("Terminal"));
        assert!(script.contains("/usr/bin/env SUDO_UID=501 "));
        assert!(
            authorization_script(
                Path::new("/helper"),
                Path::new("/socket"),
                0,
                &"a".repeat(64)
            )
            .is_err()
        );
    }
}
