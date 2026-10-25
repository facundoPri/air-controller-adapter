//! Style tokens, small styled building blocks and English labels for the
//! core's enums. One turquoise accent marks selection, focus and readiness.

use gpui_kit::{
    component::{Theme, ThemeColor},
    *,
};
use ready_for_air::studio::{Action, ButtonId, MouseMode, Profile, SystemAction};

pub const BG: u32 = 0x161616;
pub const PANEL: u32 = 0x1c1c1c;
pub const PANEL_DEEP: u32 = 0x101010;
pub const RAISED: u32 = 0x242424;
pub const BORDER: u32 = 0x2a2a2a;
pub const BORDER_STRONG: u32 = 0x3a3a3a;
pub const TEXT: u32 = 0xf5f5f5;
pub const MUTED: u32 = 0x9c9c9c;
pub const FAINT: u32 = 0x6e6e6e;
pub const ACCENT: u32 = 0x19e6d4;
const ACCENT_HOVER: u32 = 0x4cefe1;
const ACCENT_ACTIVE: u32 = 0x13c3b4;
/// Accent-tinted surface for selected or confirmed states.
pub const ACCENT_SOFT: u32 = 0x10302d;
/// Text drawn on an accent fill.
pub const ON_ACCENT: u32 = 0x062c28;
pub const DANGER: u32 = 0xff6b6b;
pub const WARN: u32 = 0xf5c451;
pub const HOVER: u32 = 0x232323;
pub const MONO: &str = "Menlo";

/// Horizontal page padding shared by the header and the content.
pub const PAGE_X: f32 = 28.;
/// Width of the navigation column; the header title aligns past it.
pub const NAV_W: f32 = 196.;
/// Inspector column; the header profile selector shares its width.
pub const INSPECTOR_W: f32 = 352.;

/// Font family for all Studio text; label measurement uses the same face.
pub const UI_FONT: &str = ".SystemUIFont";

fn hsla(c: u32) -> Hsla {
    rgb(c).into()
}

/// Accent overrides for the component library. Filled primary buttons read
/// the `button_primary*` colours; outline buttons, focus rings and links read
/// `primary`, `ring` and `link`.
fn apply_accent(t: &mut ThemeColor) {
    t.background = hsla(BG);
    t.foreground = hsla(TEXT);
    t.border = hsla(BORDER_STRONG);
    t.input = hsla(BORDER_STRONG);
    t.muted_foreground = hsla(MUTED);
    t.primary = hsla(ACCENT);
    t.primary_hover = hsla(ACCENT_HOVER);
    t.primary_active = hsla(ACCENT_ACTIVE);
    t.primary_foreground = hsla(ON_ACCENT);
    t.button_primary = hsla(ACCENT);
    t.button_primary_hover = hsla(ACCENT_HOVER);
    t.button_primary_active = hsla(ACCENT_ACTIVE);
    t.button_primary_foreground = hsla(ON_ACCENT);
    t.ring = hsla(ACCENT);
    t.caret = hsla(ACCENT);
    t.link = hsla(ACCENT);
    t.selection = Hsla {
        a: 0.3,
        ..hsla(ACCENT)
    };
}

/// Applies the accent and keeps it applied. The component registry reloads
/// its stock theme whenever the theme changes, including once right after
/// start-up, which would drop a one-time override; re-apply after each change.
/// The idempotence check stops the observer from re-triggering itself.
pub fn install(cx: &mut App) {
    fn ensure(cx: &mut App) {
        let mut wanted = Theme::global(cx).colors;
        apply_accent(&mut wanted);
        if wanted != Theme::global(cx).colors {
            Theme::update(cx, |t| apply_accent(&mut t.colors));
        }
    }
    ensure(cx);
    cx.observe_global::<Theme>(ensure).detach();
}

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
        .line_height(px(14.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(FAINT))
        .child(text.into())
}

pub fn heading(text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(16.))
        .line_height(px(22.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(TEXT))
        .child(text.into())
}

/// Coloured status dot.
pub fn dot(color: u32) -> Div {
    div().size(px(8.)).flex_none().rounded_full().bg(rgb(color))
}

/// Thin separator inside cards.
pub fn rule() -> Div {
    div().h(px(1.)).w_full().flex_none().bg(rgb(BORDER))
}

/// Inline result message (success or error).
pub fn notice(message: &(String, bool)) -> Div {
    let (text, error) = message;
    div()
        .min_w_0()
        .whitespace_normal()
        .text_size(px(12.5))
        .line_height(px(18.))
        .text_color(rgb(if *error { DANGER } else { ACCENT }))
        .child(text.clone())
}

/// Two-column fact row for compact device and permission details.
pub fn fact(label: impl Into<SharedString>, value: impl IntoElement) -> Div {
    div()
        .flex()
        .items_center()
        .gap_3()
        .min_h(px(28.))
        .text_size(px(13.))
        .child(
            div()
                .w(px(170.))
                .flex_none()
                .text_color(rgb(MUTED))
                .child(label.into()),
        )
        .child(div().flex_1().min_w_0().text_color(rgb(TEXT)).child(value))
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

/// Compact profile labels. The serialized identifier stays `air_mouse`.
pub fn profile_name(p: Profile) -> &'static str {
    match p {
        #[cfg(feature = "virtual-keyboard")]
        Profile::Keyboard => "Keyboard",
        Profile::Normal => "Normal",
        Profile::AirMouse => "Air mouse",
    }
}

/// Observed hardware mode, phrased so it never reads as the edited profile.
pub fn mode_name(m: MouseMode) -> &'static str {
    match m {
        MouseMode::Normal => "Normal mode",
        MouseMode::Active => "Air mouse on",
    }
}

fn system_name(s: SystemAction) -> &'static str {
    match s {
        SystemAction::MissionControl => "Mission Control",
        SystemAction::AppExpose => "App windows",
        SystemAction::ShowDesktop => "Show desktop",
        SystemAction::Launchpad => "Apps / Launchpad",
        SystemAction::OnScreenKeyboard => "On-screen keyboard",
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
        #[cfg(feature = "virtual-keyboard")]
        Action::Keyboard { command } => command.label().into(),
        Action::Default => "Original function".into(),
        Action::Disabled => "Disabled".into(),
        Action::Shortcut { key, modifiers } => {
            let command =
                modifiers.len() == 1 && matches!(modifiers[0].as_str(), "command" | "cmd");
            match (key.as_str(), modifiers.is_empty(), command) {
                ("page_up", true, _) => return "Page Up".into(),
                ("page_down", true, _) => return "Page Down".into(),
                ("c", _, true) => return "Copy ⌘C".into(),
                ("v", _, true) => return "Paste ⌘V".into(),
                _ => {}
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG relative luminance contrast between two sRGB colours.
    fn contrast(a: Hsla, b: Hsla) -> f32 {
        let lum = |c: Hsla| {
            let c = Rgba::from(c);
            let f = |v: f32| {
                if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b)
        };
        let (x, y) = (lum(a), lum(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    // gpui_kit's glob import shadows the std `test` attribute.
    #[::core::prelude::v1::test]
    fn selected_controls_use_the_accent_with_readable_text() {
        // Starts from the library default, as after a registry reload.
        let mut t = ThemeColor::default();
        apply_accent(&mut t);
        // Filled primary buttons (selected nav, profile, presets, setup steps).
        for fill in [
            t.button_primary,
            t.button_primary_hover,
            t.button_primary_active,
        ] {
            assert!(
                (fill.h - hsla(ACCENT).h).abs() < 0.01,
                "fill hue is not the accent"
            );
            // WCAG AA for every state, including the brief pressed shade.
            assert!(
                contrast(fill, t.button_primary_foreground) >= 4.5,
                "label contrast"
            );
        }
        // AAA where selected controls rest or are hovered.
        assert!(contrast(t.button_primary, t.button_primary_foreground) >= 7.);
        assert!(contrast(t.button_primary_hover, t.button_primary_foreground) >= 7.);
        assert_eq!(t.button_primary, hsla(ACCENT));
        // Outline primary, focus ring and links share the same colour.
        assert_eq!(t.primary, t.button_primary);
        assert_eq!(t.ring, t.button_primary);
        assert_eq!(t.link, t.button_primary);
        // Re-applying is a no-op, so the theme observer cannot loop.
        let once = t;
        apply_accent(&mut t);
        assert_eq!(t, once);
    }
}
