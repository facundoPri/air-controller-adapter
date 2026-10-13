//! Style tokens, small styled building blocks and English labels for the
//! backend's enums (the core exposes Spanish labels; the Studio UI is English).

use gpui_kit::*;
use ready_for_air::studio::{Action, ButtonId, MouseMode, Profile, SystemAction};

pub const BG: u32 = 0x161616;
pub const PANEL: u32 = 0x1d1d1d;
pub const PANEL_DEEP: u32 = 0x0f0f0f;
pub const BORDER: u32 = 0x2a2a2a;
pub const CALLOUT_BORDER: u32 = 0x2c2c2c;
pub const TEXT: u32 = 0xf5f5f5;
pub const MUTED: u32 = 0x9a9a9a;
pub const FAINT: u32 = 0x5f5f5f;
pub const ACCENT: u32 = 0x19e6d4;
pub const ACCENT_TEXT: u32 = 0x0b1514;
pub const DANGER: u32 = 0xff6b6b;
pub const OK: u32 = 0x4ade80;
pub const WARN: u32 = 0xf5c451;
pub const HOVER: u32 = 0x232323;
pub const MONO: &str = "Menlo";

/// Rounded dark card used by the inspector and the Connection view.
pub fn card() -> Div {
    div()
        .min_w_0()
        .flex()
        .flex_col()
        .gap_3()
        .p_5()
        .rounded(px(12.))
        .bg(rgb(PANEL))
        .border_1()
        .border_color(rgb(BORDER))
}

pub fn caption(text: impl Into<SharedString>) -> Div {
    div()
        .min_w_0()
        .whitespace_normal()
        .text_size(px(12.5))
        .line_height(px(18.))
        .text_color(rgb(MUTED))
        .child(text.into())
}

/// Uppercase micro heading.
pub fn eyebrow(text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(11.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(MUTED))
        .child(text.into())
}

pub fn heading(text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(17.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(TEXT))
        .child(text.into())
}

/// Coloured status dot.
pub fn dot(color: u32) -> Div {
    div().size(px(8.)).flex_none().rounded_full().bg(rgb(color))
}

/// Inline result message (success or error).
pub fn notice(message: &(String, bool)) -> Div {
    let (text, error) = message;
    div()
        .min_w_0()
        .whitespace_normal()
        .text_size(px(12.5))
        .line_height(px(18.))
        .text_color(rgb(if *error { DANGER } else { OK }))
        .child(text.clone())
}

pub fn button_name(b: ButtonId) -> &'static str {
    match b {
        ButtonId::Home => "Home",
        ButtonId::Experiences => "Experiences",
        ButtonId::Apps => "Apps",
        ButtonId::Back => "Back",
        ButtonId::Up => "Up",
        ButtonId::Down => "Down",
        ButtonId::Left => "Left",
        ButtonId::Right => "Right",
        ButtonId::Ok => "OK",
        ButtonId::Capture => "Capture",
        ButtonId::MouseMode => "Air mouse",
        ButtonId::VolumeDown => "Volume −",
        ButtonId::VolumeUp => "Volume +",
        ButtonId::Mute => "Mute",
    }
}

pub fn profile_name(p: Profile) -> &'static str {
    match p {
        Profile::Normal => "Normal",
        Profile::AirMouse => "Air mouse",
    }
}

pub fn mode_name(m: MouseMode) -> &'static str {
    match m {
        MouseMode::Normal => "Normal mode",
        MouseMode::Active => "Air mouse active",
    }
}

fn system_name(s: SystemAction) -> &'static str {
    match s {
        SystemAction::MissionControl => "Mission Control",
        SystemAction::AppExpose => "App windows",
        SystemAction::ShowDesktop => "Show desktop",
        SystemAction::Launchpad => "Apps / Launchpad",
        SystemAction::VolumeUp => "Volume up",
        SystemAction::VolumeDown => "Volume down",
        SystemAction::Mute => "Mute",
        SystemAction::ZoomIn => "Zoom in",
        SystemAction::ZoomOut => "Zoom out",
        SystemAction::ZoomToggle => "Toggle zoom",
        SystemAction::DisplaySleep => "Display sleep",
        SystemAction::Sleep => "Sleep Mac",
        SystemAction::LockScreen => "Lock screen",
    }
}

pub fn action_name(a: &Action) -> String {
    match a {
        Action::Default => "Original function".into(),
        Action::Disabled => "Disabled".into(),
        Action::Shortcut { key, modifiers } => {
            if modifiers.is_empty() {
                match key.as_str() {
                    "page_up" => return "Page Up".into(),
                    "page_down" => return "Page Down".into(),
                    _ => {}
                }
            }
            let mut parts: Vec<String> = modifiers
                .iter()
                .map(|m| match m.as_str() {
                    "command" | "cmd" => "⌘".to_string(),
                    "shift" => "⇧".to_string(),
                    "option" | "alt" => "⌥".to_string(),
                    "control" | "ctrl" => "⌃".to_string(),
                    other => format!("{other}+"),
                })
                .collect();
            parts.push(key.to_uppercase());
            format!("Shortcut {}", parts.concat())
        }
        Action::System { command } => system_name(*command).into(),
        // Existing JSON keeps its pixel semantics; no silent migration to pages.
        Action::Scroll { x: 0, y } => format!("Scroll vertical {y:+} px"),
        Action::Scroll { x, y: 0 } => format!("Scroll horizontal {x:+} px"),
        Action::Scroll { x, y } => format!("Scroll {x:+}, {y:+} px"),
        Action::AppleScript { .. } => "AppleScript".into(),
        Action::Shell { .. } => "Shell command".into(),
        Action::Macro { steps } => format!("Macro · {} steps", steps.len()),
        Action::VoiceToggle => "Record voice / dictate".into(),
    }
}
