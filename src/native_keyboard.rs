use std::{
    ffi::{CString, c_void},
    marker::PhantomData,
    rc::Rc,
};

/// The AX destination stays on the UI thread and never reads the field's text.
pub struct Target {
    raw: *mut c_void,
    _ui_thread: PhantomData<Rc<()>>,
}
#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn aca_keyboard_target_capture() -> *mut c_void;
    fn aca_keyboard_target_free(target: *mut c_void);
    fn aca_keyboard_target_valid(target: *mut c_void) -> bool;
    fn aca_keyboard_target_send(
        target: *mut c_void,
        text: *const u16,
        length: usize,
        key: u16,
    ) -> bool;
    fn aca_keyboard_panel_prepare(title: *const std::ffi::c_char) -> bool;
}
impl Target {
    pub fn capture() -> Result<Self, String> {
        if !crate::native_input::trusted() {
            return Err("Accessibility permission is unavailable for this ACA executable".into());
        }
        #[cfg(target_os = "macos")]
        {
            let raw = unsafe { aca_keyboard_target_capture() };
            if !raw.is_null() {
                return Ok(Self {
                    raw,
                    _ui_thread: PhantomData,
                });
            }
        }
        Err("Focus a supported text field in another app. ACA needs Accessibility permission and an observable window and field.".into())
    }
    pub fn valid(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            unsafe { aca_keyboard_target_valid(self.raw) }
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }
    pub fn text(&self, text: &str) -> Result<(), String> {
        let units: Vec<u16> = text.encode_utf16().collect();
        if units.is_empty() || units.len() > 8 {
            return Err("Unsupported keyboard character".into());
        }
        self.send(&units, 0)
    }
    pub fn control(&self, key: u16) -> Result<(), String> {
        if ![36, 48, 51].contains(&key) {
            return Err("Unsupported keyboard control".into());
        }
        self.send(&[], key)
    }
    fn send(&self, units: &[u16], key: u16) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        if unsafe { aca_keyboard_target_send(self.raw, units.as_ptr(), units.len(), key) } {
            return Ok(());
        }
        Err("Keyboard output canceled because the destination could not be verified".into())
    }
}
impl Drop for Target {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        unsafe {
            aca_keyboard_target_free(self.raw)
        }
    }
}
pub fn prepare_panel(title: &str) -> bool {
    let Ok(title) = CString::new(title) else {
        return false;
    };
    #[cfg(target_os = "macos")]
    {
        unsafe { aca_keyboard_panel_prepare(title.as_ptr()) }
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

pub fn pointer_position() -> (f64, f64) {
    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" {
            fn aca_keyboard_pointer(x: *mut f64, y: *mut f64);
        }
        let (mut x, mut y) = (0., 0.);
        unsafe {
            aca_keyboard_pointer(&mut x, &mut y);
        }
        (x, y)
    }
    #[cfg(not(target_os = "macos"))]
    {
        (0., 0.)
    }
}

pub fn move_panel(title: &str, x: f64, y: f64) {
    let Ok(title) = CString::new(title) else {
        return;
    };
    #[cfg(target_os = "macos")]
    unsafe {
        unsafe extern "C" {
            fn aca_keyboard_panel_move(title: *const std::ffi::c_char, x: f64, y: f64);
        }
        aca_keyboard_panel_move(title.as_ptr(), x, y);
    }
}

/// QA bundles require an embedded isolated root even when LaunchServices relaunches them.
pub fn qa_data_root() -> Result<Option<std::path::PathBuf>, String> {
    #[cfg(target_os = "macos")]
    unsafe {
        unsafe extern "C" {
            fn aca_keyboard_qa_data_root() -> *const std::ffi::c_char;
        }
        let raw = aca_keyboard_qa_data_root();
        if !raw.is_null() {
            let root = std::ffi::CStr::from_ptr(raw)
                .to_str()
                .map_err(|_| "Invalid QA data root")?;
            if root.is_empty() || !root.starts_with("/private/tmp/aca-qa-") {
                return Err(
                    "QA bundle has no isolated data root. Refusing to open user configuration."
                        .into(),
                );
            }
            return Ok(Some(root.into()));
        }
    }
    Ok(None)
}
