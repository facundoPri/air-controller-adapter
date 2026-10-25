//! ACTIVITY view: packet counters and the bounded engine log (newest at bottom).

use super::{StudioApp, status, theme::*};
use gpui_kit::{
    component::{
        Sizable,
        button::{Button, ButtonVariants},
    },
    prelude::FluentBuilder,
    *,
};
use ready_for_air::lifecycle::ConnectionStatus;

/// Rendering every bounded entry each frame is wasteful; export keeps them all.
const VISIBLE_LOGS: usize = 400;

/// English chip label and colour for the core's log categories.
fn category(category: &str) -> (String, u32) {
    let (label, color) = match category {
        "packet" => ("PACKET", 0x6b8afd),
        "mode" => ("MODE", ACCENT),
        "learn" => ("LEARN", WARN),
        "engine" => ("ENGINE", 0xc084fc),
        "config" => ("CONFIG", 0x9ad1ff),
        "system" => ("SYSTEM", MUTED),
        "routing" => ("BUTTON", WARN),
        "action" => ("ACTION", ACCENT),
        "error" => ("ERROR", DANGER),
        other => return (other.to_uppercase(), MUTED),
    };
    (label.into(), color)
}

impl StudioApp {
    pub(super) fn render_activity(
        &mut self,
        status: &ConnectionStatus,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let s = &self.studio;
        let total = s.logs.len();
        let skip = total.saturating_sub(VISIBLE_LOGS);
        let rows = s.logs.iter().enumerate().skip(skip).map(|(i, l)| {
            let (chip, chip_color) = category(&l.category);
            div()
                .id(("log", i))
                .flex()
                .items_start()
                .gap_3()
                .py(px(5.))
                .px_3()
                .border_b_1()
                .border_color(rgb(0x202020))
                .child(
                    div()
                        .flex_none()
                        .w(px(96.))
                        .font_family(MONO)
                        .text_size(px(11.5))
                        .text_color(rgb(FAINT))
                        .child(l.time.clone()),
                )
                .child(
                    div().flex_none().w(px(78.)).child(
                        div()
                            .px_1()
                            .rounded(px(3.))
                            .border_1()
                            .border_color(rgb(BORDER_STRONG))
                            .text_size(px(10.5))
                            .text_color(rgb(chip_color))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(chip),
                    ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .whitespace_normal()
                        .text_size(px(12.5))
                        .when(l.category == "packet", |d| d.font_family(MONO))
                        .child(l.message.clone()),
                )
        });

        let (headline, color) = status::headline(status);
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                card()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap_6()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(eyebrow("PACKETS"))
                            .child(
                                div()
                                    .text_size(px(24.))
                                    .font_weight(FontWeight::BOLD)
                                    .child(s.packet_count.to_string()),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(220.))
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(eyebrow("LAST REPORT"))
                            .child(
                                div()
                                    .font_family(MONO)
                                    .text_size(px(13.))
                                    .child(s.last_packet.clone()),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .text_size(px(12.5))
                                    .text_color(rgb(MUTED))
                                    .child(dot(color))
                                    .child(headline)
                                    .when_some(status::observed_mode(status), |d, mode| {
                                        d.child("·").child(mode)
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_2()
                            .child(Button::new("clear-logs").label("Clear").small().ghost().on_click(
                                cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.studio.clear_logs();
                                    this.report("logs", Ok("Log cleared".into()));
                                    cx.notify();
                                }),
                            ))
                            .child(Button::new("export-logs").label("Export").small().outline().on_click(
                                cx.listener(|this, _: &ClickEvent, _, cx| {
                                    let r = this
                                        .studio
                                        .export_logs()
                                        .map(|p| format!("Exported to {}", p.display()))
                                        .map_err(|e| format!("Export failed: {e}"));
                                    this.report("logs", r);
                                    cx.notify();
                                }),
                            )),
                    ),
            )
            .when_some(self.message("logs").cloned(), |d, m| d.child(notice(&m)))
            .child(caption(format!(
                "Keyboard and vendor reports are all logged; mouse movement is sampled (1 in 20) while the packet counter counts every report.{}",
                if skip > 0 {
                    format!(" Showing the latest {VISIBLE_LOGS} of {total} entries; Export saves all.")
                } else {
                    String::new()
                }
            )))
            .child(
                div()
                    .id("log-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.log_scroll)
                    .rounded(px(12.))
                    .bg(rgb(PANEL_DEEP))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .mb_2()
                    .when(total == 0, |d| {
                        d.child(div().p_4().child(caption(
                            "No activity yet. Press a button on the connected remote.",
                        )))
                    })
                    .children(rows),
            )
    }
}
