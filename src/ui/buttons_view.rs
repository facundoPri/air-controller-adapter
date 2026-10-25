//! BUTTONS view: the 3D remote beside the inspector, a one-line setup strip
//! while the remote is not ready, and a landing page when it is not
//! connected. Mappings stay editable offline.

use super::{StudioApp, status, theme::*};
use gpui_kit::{
    component::{
        Icon, IconName, Sizable,
        button::{Button, ButtonVariants},
    },
    prelude::FluentBuilder,
    *,
};
use ready_for_air::lifecycle::{ConnectionCommand, ConnectionStatus, Readiness};

impl StudioApp {
    pub(super) fn render_buttons(
        &mut self,
        status: &ConnectionStatus,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        if status::landing(status) && !self.offline_editing {
            return self.render_landing(status, cx).into_any_element();
        }
        let strip = self.render_setup_strip(status, cx);
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_3()
            .when_some(strip, |d, strip| d.child(strip))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .gap(px(24.))
                    .child(self.render_stage(window, cx))
                    .child(self.render_inspector(cx)),
            )
            .into_any_element()
    }

    /// The next step while the remote is not ready; nothing when it is.
    fn render_setup_strip(
        &self,
        status: &ConnectionStatus,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        if status.state == Readiness::Ready {
            return None;
        }
        let offline = status::landing(status);
        let (text, secondary) = if offline {
            (
                "Editing offline. Changes are saved now and apply when the remote connects."
                    .to_string(),
                None,
            )
        } else {
            (status.reason.clone(), status::secondary(status))
        };
        // Connect already sits in the header.
        let primary = status::primary(status)
            .filter(|n| n.step != status::Step::Run(ConnectionCommand::Connect));
        let pending = primary.is_none()
            && matches!(
                status.state,
                Readiness::Connecting | Readiness::Preparing | Readiness::Disconnecting
            );
        let error = self.message("connection").filter(|(_, e)| *e).cloned();
        Some(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap_3()
                .min_h(px(44.))
                .px_4()
                .py_2()
                .rounded(px(10.))
                .bg(rgb(PANEL))
                .border_1()
                .border_color(rgb(BORDER))
                .child(
                    Icon::new(if offline {
                        IconName::CircleX
                    } else if pending {
                        IconName::LoaderCircle
                    } else {
                        IconName::Info
                    })
                    .text_color(rgb(if pending { WARN } else { MUTED })),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_size(px(13.))
                                .line_height(px(18.))
                                .text_color(rgb(TEXT))
                                .child(text),
                        )
                        .when_some(error, |d, m| d.child(notice(&m))),
                )
                .when_some(secondary, |d, n| {
                    d.child(self.step_button("strip-secondary", n, false, cx))
                })
                .when_some(primary, |d, n| {
                    d.child(self.step_button("strip-primary", n, true, cx))
                })
                .child(
                    Button::new("strip-details")
                        .label("Details")
                        .small()
                        .ghost()
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.show(super::View::Connection);
                            cx.notify();
                        })),
                ),
        )
    }

    fn render_landing(
        &self,
        status: &ConnectionStatus,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let stopped = status::stopped(status);
        let steps = status::checklist(status)
            .into_iter()
            .enumerate()
            .map(|(i, r)| {
                div()
                    .flex()
                    .items_start()
                    .gap_3()
                    .child(
                        div()
                            .size(px(22.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .border_1()
                            .border_color(rgb(if r.met { ACCENT } else { BORDER_STRONG }))
                            .bg(rgb(if r.met { ACCENT_SOFT } else { PANEL_DEEP }))
                            .text_size(px(11.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(if r.met { ACCENT } else { MUTED }))
                            .map(|d| {
                                if r.met {
                                    d.child(Icon::new(IconName::Check).xsmall())
                                } else {
                                    d.child((i + 1).to_string())
                                }
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(
                                div()
                                    .text_size(px(13.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(TEXT))
                                    .child(r.title),
                            )
                            .child(caption(r.detail)),
                    )
            });
        let primary = status::primary(status);
        let secondary = status::secondary(status);
        div()
            .id("landing")
            .size_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .items_center()
            .child(div().flex_1().min_h(px(16.)))
            .child(
                card()
                    .w(px(560.))
                    .max_w_full()
                    .flex_none()
                    .gap_5()
                    .p(px(28.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(eyebrow(if stopped { "ACA STOPPED" } else { "NOT CONNECTED" }))
                            .child(
                                div()
                                    .text_size(px(24.))
                                    .line_height(px(30.))
                                    .font_weight(FontWeight::BOLD)
                                    .child(if stopped {
                                        "ACA is not using your MR-01"
                                    } else {
                                        "Connect your MR-01"
                                    }),
                            )
                            .child(caption(status.reason.clone())),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_2()
                            .when_some(primary, |d, n| d.child(self.step_button("landing-primary", n, true, cx)))
                            .when_some(secondary, |d, n| {
                                d.child(self.step_button("landing-secondary", n, false, cx))
                            })
                            .child(
                                Button::new("edit-offline")
                                    .label("Edit mappings offline")
                                    .small()
                                    .ghost()
                                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                        this.offline_editing = true;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .when_some(self.message("connection").cloned(), |d, m| d.child(notice(&m)))
                    .child(rule())
                    .child(div().flex().flex_col().gap_4().children(steps))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(caption("Permission details, device choices and the configuration file are in Connection."))
                            .child(
                                Button::new("landing-connection")
                                    .label("Open Connection")
                                    .small()
                                    .ghost()
                                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                        this.show(super::View::Connection);
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .child(div().flex_1().min_h(px(16.)))
    }
}
