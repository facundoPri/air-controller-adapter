//! Read-only permission observations; no prompts or mutations during polling.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Access {
    #[default]
    Unknown,
    Denied,
    Granted,
    Verified,
}
impl Access {
    pub fn from_hid(value: u8) -> Self {
        match value {
            0 => Self::Granted,
            1 => Self::Denied,
            _ => Self::Unknown,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Unknown => "Sin comprobar",
            Self::Denied => "Denegado",
            Self::Granted => "Permiso concedido",
            Self::Verified => "Lectura comprobada",
        }
    }
}
pub fn input() -> Access {
    #[cfg(target_os = "macos")]
    {
        #[link(name = "IOKit", kind = "framework")]
        unsafe extern "C" {
            fn IOHIDCheckAccess(request: u32) -> u32;
        }
        Access::from_hid(unsafe { IOHIDCheckAccess(1) } as u8)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Access::Unknown
    }
}
pub fn optional(kind: &str) -> Access {
    #[cfg(target_os = "macos")]
    unsafe {
        use std::ffi::c_void;
        #[link(name = "AVFoundation", kind = "framework")]
        unsafe extern "C" {
            static AVMediaTypeAudio: *mut c_void;
        }
        #[link(name = "CoreBluetooth", kind = "framework")]
        unsafe extern "C" {}
        #[link(name = "objc")]
        unsafe extern "C" {
            fn objc_getClass(name: *const std::ffi::c_char) -> *mut c_void;
            fn sel_registerName(name: *const std::ffi::c_char) -> *mut c_void;
            fn objc_msgSend(receiver: *mut c_void, selector: *mut c_void) -> *mut c_void;
        }
        let value = if kind == "microphone" {
            let call: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> isize =
                std::mem::transmute(objc_msgSend as *const ());
            call(
                objc_getClass(c"AVCaptureDevice".as_ptr()),
                sel_registerName(c"authorizationStatusForMediaType:".as_ptr()),
                AVMediaTypeAudio,
            )
        } else if kind == "bluetooth" {
            let call: unsafe extern "C" fn(*mut c_void, *mut c_void) -> isize =
                std::mem::transmute(objc_msgSend as *const ());
            call(
                objc_getClass(c"CBManager".as_ptr()),
                sel_registerName(c"authorization".as_ptr()),
            )
        } else {
            return Access::Unknown;
        };
        match value {
            3 => Access::Granted,
            1 | 2 => Access::Denied,
            _ => Access::Unknown,
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = kind;
        Access::Unknown
    }
}
/// Transport handshake alone never proves the helper can read the remote.
pub fn capture_observation(reported: Access, service: bool, packets: bool) -> Access {
    if !service {
        Access::Unknown
    } else if reported == Access::Denied {
        Access::Denied
    } else if packets {
        Access::Verified
    } else {
        reported
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn handshake_does_not_grant_input_permission() {
        assert_eq!(
            capture_observation(Access::Denied, true, false),
            Access::Denied
        );
        assert_eq!(
            capture_observation(Access::Granted, true, false),
            Access::Granted
        );
        assert_eq!(
            capture_observation(Access::Granted, true, true),
            Access::Verified
        );
        assert_eq!(
            capture_observation(Access::Granted, false, true),
            Access::Unknown
        );
    }
}

pub fn open_settings(pane: &str) -> Result<String, String> {
    let url = match pane {
        "input" => "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent",
        "accessibility" => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
        }
        "bluetooth" => "x-apple.systempreferences:com.apple.preference.security?Privacy_Bluetooth",
        "radio" => "x-apple.systempreferences:com.apple.BluetoothSettings",
        "microphone" => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
        }
        "automation" => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Automation"
        }
        _ => return Err("Panel desconocido".into()),
    };
    std::process::Command::new("/usr/bin/open")
        .arg(url)
        .status()
        .map_err(|e| e.to_string())
        .and_then(|s| {
            if s.success() {
                Ok("Ajustes abiertos. Habilitá la app indicada y volvé a comprobar.".into())
            } else {
                Err("No se pudo abrir Ajustes".into())
            }
        })
}
