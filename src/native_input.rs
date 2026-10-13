//! macOS output adapter for the sole-owner MR-01 reader. Output remains in the unprivileged Studio process.
use crate::direct::Output;

pub fn trusted() -> bool {
    #[cfg(target_os = "macos")]
    {
        unsafe { mac::AXIsProcessTrusted() }
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

#[derive(Default)]
struct Clicks {
    last: Option<(u8, std::time::Instant, (f64, f64), i64)>,
    active: [i64; 8],
}
impl Clicks {
    fn event(
        &mut self,
        button: u8,
        down: bool,
        at: std::time::Instant,
        point: (f64, f64),
        interval: std::time::Duration,
    ) -> i64 {
        let Some(slot) = button
            .checked_sub(1)
            .map(usize::from)
            .filter(|i| *i < self.active.len())
        else {
            return 1;
        };
        if !down {
            return self.active[slot].max(1);
        }
        let count = self
            .last
            .filter(|(b, t, p, _)| {
                *b == button
                    && at.saturating_duration_since(*t) <= interval
                    && (p.0 - point.0).hypot(p.1 - point.1) <= 4.
            })
            .map_or(1, |(_, _, _, n)| n.saturating_add(1));
        self.last = Some((button, at, point, count));
        self.active[slot] = count;
        count
    }
}
fn modifier_flags(remote: u8, physical: u64) -> u64 {
    let mut flags = physical;
    for (mask, flag) in [
        (0x11, 1u64 << 18),
        (0x22, 1 << 17),
        (0x44, 1 << 19),
        (0x88, 1 << 20),
    ] {
        if remote & mask != 0 {
            flags |= flag;
        }
    }
    flags
}

#[derive(Default)]
pub struct Sender {
    buttons: u8,
    modifiers: u8,
    clicks: Clicks,
}
impl Sender {
    pub fn send(&mut self, output: &Output) -> Result<(), String> {
        // Unsupported originals are a per-button capability, never a transport
        // failure. The reader logs this disposition and continues routing.
        if unsupported_original(output).is_some() {
            return Ok(());
        }
        #[cfg(target_os = "macos")]
        {
            unsafe { self.send_mac(output) }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = output;
            Err("Native remapping currently requires macOS".into())
        }
    }
}
/// A default Android-only usage is intentionally inert on macOS. Keep this
/// separate from errors creating/posting a supported native event.
pub fn unsupported_original(output: &Output) -> Option<String> {
    match output {
        // MR-01 sends Mute on the Keyboard page, not only the Consumer page.
        Output::Key {
            usage: 0x7f..=0x81, ..
        } => None,
        Output::Consumer(usage) if !matches!(usage, 0xe9 | 0xea | 0xe2 | 0xcd | 0xb5 | 0xb6) => {
            Some(format!(
                "Multimedia {usage:04X}: sin función original en macOS. Asignale una función; el resto del control sigue activo."
            ))
        }
        Output::Key { usage, down: true } if keycode(*usage).is_none() => Some(format!(
            "Tecla HID {usage:02X}: sin equivalente en macOS. Asignale una función."
        )),
        Output::Key { usage, down: false } if keycode(*usage).is_none() => Some(String::new()),
        _ => None,
    }
}

// USB keyboard usages to macOS virtual key codes. The remote uses arrows/Enter;
// the broader table also handles learned keyboard functions without a layout guess.
fn keycode(usage: u8) -> Option<u16> {
    const LETTERS: [u16; 26] = [
        0, 11, 8, 2, 14, 3, 5, 4, 34, 38, 40, 37, 46, 45, 31, 35, 12, 15, 1, 17, 32, 9, 13, 7, 16,
        6,
    ];
    Some(match usage {
        4..=29 => LETTERS[(usage - 4) as usize],
        30..=39 => [18, 19, 20, 21, 23, 22, 26, 28, 25, 29][(usage - 30) as usize],
        0x28 => 36,
        0x29 => 53,
        0x2a => 51,
        0x2b => 48,
        0x2c => 49,
        0x2d => 27,
        0x2e => 24,
        0x2f => 33,
        0x30 => 30,
        0x31 => 42,
        0x33 => 41,
        0x34 => 39,
        0x35 => 50,
        0x36 => 43,
        0x37 => 47,
        0x38 => 44,
        0x39 => 57,
        0x3a..=0x45 => {
            [122, 120, 99, 118, 96, 97, 98, 100, 101, 109, 103, 111][(usage - 0x3a) as usize]
        }
        0x49 => 114,
        0x4a => 115,
        0x4b => 116,
        0x4c => 117,
        0x4d => 119,
        0x4e => 121,
        0x4f => 124,
        0x50 => 123,
        0x51 => 125,
        0x52 => 126,
        0xe0..=0xe7 => [59, 56, 58, 55, 62, 60, 61, 54][(usage - 0xe0) as usize],
        _ => return None,
    })
}

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::{c_char, c_void};
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct Point {
        pub x: f64,
        pub y: f64,
    }
    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        pub fn AXIsProcessTrusted() -> bool;
        pub fn CGEventCreate(source: *const c_void) -> *mut c_void;
        pub fn CGEventGetLocation(event: *const c_void) -> Point;
        pub fn CGEventCreateMouseEvent(
            source: *const c_void,
            kind: u32,
            point: Point,
            button: u32,
        ) -> *mut c_void;
        pub fn CGEventCreateKeyboardEvent(
            source: *const c_void,
            key: u16,
            down: bool,
        ) -> *mut c_void;
        pub fn CGEventCreateScrollWheelEvent(
            source: *const c_void,
            units: u32,
            count: u32,
            ...
        ) -> *mut c_void;
        pub fn CGEventSetIntegerValueField(event: *mut c_void, field: u32, value: i64);
        pub fn CGEventSourceFlagsState(state: i32) -> u64;
        pub fn CGEventSetFlags(event: *mut c_void, flags: u64);
        pub fn CGEventPost(location: u32, event: *mut c_void);
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        pub fn CFRelease(object: *const c_void);
    }
    #[link(name = "AppKit", kind = "framework")]
    unsafe extern "C" {}
    #[link(name = "objc")]
    unsafe extern "C" {
        pub fn objc_getClass(name: *const c_char) -> *mut c_void;
        pub fn sel_registerName(name: *const c_char) -> *mut c_void;
        pub fn objc_msgSend(receiver: *mut c_void, selector: *mut c_void) -> *mut c_void;
        pub fn objc_autoreleasePoolPush() -> *mut c_void;
        pub fn objc_autoreleasePoolPop(pool: *mut c_void);
    }
    pub unsafe fn post(event: *mut c_void) -> Result<(), String> {
        if event.is_null() {
            return Err("Could not create native input event".into());
        }
        unsafe {
            CGEventPost(1, event);
            CFRelease(event);
        } // session tap: do not feed virtual HID back into a grabber
        Ok(())
    }
    pub unsafe fn location() -> Result<Point, String> {
        unsafe {
            let e = CGEventCreate(std::ptr::null());
            if e.is_null() {
                return Err("Could not read cursor position".into());
            }
            let p = CGEventGetLocation(e);
            CFRelease(e);
            Ok(p)
        }
    }
    pub unsafe fn media(key: i64) -> Result<(), String> {
        unsafe {
            let pool = objc_autoreleasePoolPush();
            let make: unsafe extern "C" fn(
                *mut c_void,
                *mut c_void,
                usize,
                Point,
                usize,
                f64,
                isize,
                *mut c_void,
                i16,
                isize,
                isize,
            ) -> *mut c_void = std::mem::transmute(objc_msgSend as *const ());
            let get: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            let result = (|| {
                for state in [0xau32, 0xb] {
                    let e=make(objc_getClass(c"NSEvent".as_ptr()),sel_registerName(c"otherEventWithType:location:modifierFlags:timestamp:windowNumber:context:subtype:data1:data2:".as_ptr()),14,Point{x:0.,y:0.},(state<<8) as usize,0.,0,std::ptr::null_mut(),8,((key<<16)|((state as i64)<<8)) as isize,-1);
                    if e.is_null() {
                        return Err("Could not create media key".into());
                    }
                    let cg = get(e, sel_registerName(c"CGEvent".as_ptr()));
                    if cg.is_null() {
                        return Err("Could not bridge media key".into());
                    }
                    CGEventPost(1, cg);
                }
                Ok(())
            })();
            objc_autoreleasePoolPop(pool);
            result
        }
    }
}
#[cfg(target_os = "macos")]
impl Sender {
    unsafe fn send_mac(&mut self, output: &Output) -> Result<(), String> {
        use mac::*;
        unsafe {
            match *output {
                Output::Action(..) => Ok(()),
                Output::Key { usage, down } => {
                    if let Some(media_key) = keyboard_media_key(usage) {
                        return if down { media(media_key) } else { Ok(()) };
                    }
                    let key=keycode(usage).ok_or_else(||format!("Original HID key {usage:02X} is not supported; returning to observation"))?;
                    if (0xe0..=0xe7).contains(&usage) {
                        let bit = 1 << (usage - 0xe0);
                        if down {
                            self.modifiers |= bit;
                        } else {
                            self.modifiers &= !bit;
                        }
                    }
                    let e = CGEventCreateKeyboardEvent(std::ptr::null(), key, down);
                    if !e.is_null() {
                        // HID state excludes our session-tap synthetic events.
                        CGEventSetFlags(
                            e,
                            modifier_flags(self.modifiers, CGEventSourceFlagsState(1)),
                        );
                    }
                    post(e)
                }
                Output::MouseButton { button, down } => {
                    let bit = 1 << (button - 1);
                    if down {
                        self.buttons |= bit;
                    } else {
                        self.buttons &= !bit;
                    }
                    let kind = match (button, down) {
                        (1, true) => 1,
                        (1, false) => 2,
                        (2, true) => 3,
                        (2, false) => 4,
                        (_, true) => 25,
                        (_, false) => 26,
                    };
                    let point = location()?;
                    let interval: unsafe extern "C" fn(
                        *mut std::ffi::c_void,
                        *mut std::ffi::c_void,
                    ) -> f64 = std::mem::transmute(objc_msgSend as *const ());
                    let seconds = interval(
                        objc_getClass(c"NSEvent".as_ptr()),
                        sel_registerName(c"doubleClickInterval".as_ptr()),
                    );
                    let count = self.clicks.event(
                        button,
                        down,
                        std::time::Instant::now(),
                        (point.x, point.y),
                        std::time::Duration::from_secs_f64(
                            if seconds.is_finite() && seconds > 0. {
                                seconds
                            } else {
                                0.5
                            },
                        ),
                    );
                    let e =
                        CGEventCreateMouseEvent(std::ptr::null(), kind, point, (button - 1) as u32);
                    if !e.is_null() {
                        CGEventSetIntegerValueField(e, 1, count);
                    }
                    post(e)
                }
                Output::Motion { x, y, wheel } => {
                    if x != 0 || y != 0 {
                        let mut p = location()?;
                        p.x += x as f64;
                        p.y += y as f64;
                        let (kind, button) = if self.buttons & 1 != 0 {
                            (6, 0)
                        } else if self.buttons & 2 != 0 {
                            (7, 1)
                        } else if self.buttons != 0 {
                            (27, self.buttons.trailing_zeros())
                        } else {
                            (5, 0)
                        };
                        let e = CGEventCreateMouseEvent(std::ptr::null(), kind, p, button);
                        if !e.is_null() {
                            CGEventSetIntegerValueField(e, 4, x as i64);
                            CGEventSetIntegerValueField(e, 5, y as i64);
                        }
                        post(e)?;
                    }
                    if wheel != 0 {
                        post(CGEventCreateScrollWheelEvent(
                            std::ptr::null(),
                            1,
                            1,
                            wheel as i32,
                        ))?;
                    }
                    Ok(())
                }
                Output::Consumer(usage) => {
                    match usage {
                        0xe9 => media(0),
                        0xea => media(1),
                        0xe2 => media(7),
                        0xcd => media(16),
                        0xb5 => media(17),
                        0xb6 => media(18),
                        // MR-01 Android navigation/capture keys have no native macOS equivalent.
                        // Assign these a Studio action; leave Default inert, as on macOS.
                        0x6c | 0x223 | 0x224 | 0x221 | 0x40 | 0x41 => Ok(()),
                        _ => Err(format!(
                            "Original consumer HID {usage:04X} has no adapter; assign it an action"
                        )),
                    }
                }
            }
        }
    }
}

fn keyboard_media_key(usage: u8) -> Option<i64> {
    match usage {
        0x7f => Some(7), // Keyboard Mute → NX_KEYTYPE_MUTE
        0x80 => Some(0), // Keyboard Volume Up
        0x81 => Some(1), // Keyboard Volume Down
        _ => None,
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use crate::{
        direct::Router,
        studio::{Action, ButtonId, Config, InputSource, Profile, SystemAction},
    };
    #[test]
    fn clicks_preserve_down_up_sequence_and_reset_after_time_distance_or_button() {
        use std::time::{Duration, Instant};
        let now = Instant::now();
        let interval = Duration::from_millis(500);
        let mut c = Clicks::default();
        for (ms, down, count) in [
            (0, true, 1),
            (30, false, 1),
            (100, true, 2),
            (120, false, 2),
            (220, true, 3),
            (240, false, 3),
            (1000, true, 1),
        ] {
            assert_eq!(
                c.event(
                    1,
                    down,
                    now + Duration::from_millis(ms),
                    (10., 10.),
                    interval
                ),
                count
            );
        }
        assert_eq!(
            c.event(
                1,
                true,
                now + Duration::from_millis(1100),
                (100., 10.),
                interval
            ),
            1
        );
        assert_eq!(
            c.event(
                2,
                true,
                now + Duration::from_millis(1200),
                (100., 10.),
                interval
            ),
            1
        );
    }
    #[test]
    fn releasing_remote_modifier_preserves_physical_keyboard_flags() {
        assert_eq!(modifier_flags(0, 1 << 17), 1 << 17);
        assert_eq!(modifier_flags(0x08, 1 << 17), (1 << 17) | (1 << 20));
        assert_eq!(modifier_flags(0, 0), 0);
    }
    #[test]
    fn keyboard_mute_is_supported_like_consumer_mute() {
        for (usage, expected) in [(0x7f, 7), (0x80, 0), (0x81, 1)] {
            assert_eq!(keyboard_media_key(usage), Some(expected));
            for down in [true, false] {
                assert!(unsupported_original(&Output::Key { usage, down }).is_none());
            }
        }
    }
    #[test]
    fn android_default_buttons_do_not_interrupt_subsequent_mapped_actions() {
        let mut config = Config::default();
        let capture = config
            .buttons
            .iter_mut()
            .find(|b| b.button == ButtonId::Capture)
            .unwrap();
        capture.normal_sources = vec![InputSource::Consumer(0x6c)];
        capture.normal = Action::System {
            command: SystemAction::MissionControl,
        };
        let mut router = Router::default();
        let mut sender = Sender::default();
        let mut actions = 0;
        // Values from the failing live log. Only unsupported originals go to
        // Sender; mapped actions are collected, never executed on the test Mac.
        for usage in [0x6a, 0x6b, 0x8d, 0x6c] {
            for output in router.process(&[3, usage, 0], std::time::Instant::now(), &config) {
                if output == Output::Action(ButtonId::Capture, Profile::Normal) {
                    actions += 1;
                } else {
                    assert!(
                        sender.send(&output).is_ok(),
                        "{output:?} must not terminate capture"
                    );
                }
            }
            router.process(&[3, 0, 0], std::time::Instant::now(), &config);
        }
        assert_eq!(
            actions, 1,
            "Capture still dispatches after original Android buttons"
        );
    }
}
