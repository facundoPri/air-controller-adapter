//! CONNECTION view: the remote's state and next step first, then compact
//! permission details, then the configuration editor and diagnostics.

use super::{StudioApp, status, theme::*};
use gpui_kit::{
    base::Disableable,
    component::{
        Icon, IconName, Sizable,
        button::{Button, ButtonVariants},
        input::Textarea,
    },
    prelude::FluentBuilder,
    *,
};
use ready_for_air::{
    lifecycle::{ConnectionCommand, ConnectionStatus, Readiness, SetupAction},
    permissions::{self, Access},
    studio::Config,
};

/// Children of the scroll container, for `scroll_to_item`.
const CONFIGURATION_ITEM: usize = 2;

/// Status text and colour for one permission or session requirement.
fn requirement(
    met: Option<bool>,
    yes: &'static str,
    no: &'static str,
    unknown: &'static str,
) -> Div {
    let (color, text) = match met {
        Some(true) => (ACCENT, yes),
        Some(false) => (WARN, no),
        None => (FAINT, unknown),
    };
    div().flex().items_center().gap_2().child(dot(color)).child(
        div()
            .text_color(rgb(if met == Some(true) { TEXT } else { MUTED }))
            .child(text),
    )
}

fn access(a: Access) -> Option<bool> {
    match a {
        Access::Unknown => None,
        Access::Denied => Some(false),
        Access::Granted | Access::Verified => Some(true),
    }
}

/// Opaque identifiers are long; show enough of the end to tell remotes apart.
fn short_id(id: &str) -> String {
    let tail: String = id
        .chars()
        .rev()
        .take(8)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if tail.len() < id.len() {
        format!("…{tail}")
    } else {
        tail
    }
}

impl StudioApp {
    fn disclosure(
        &self,
        id: &'static str,
        title: &'static str,
        open: bool,
        badge: Option<&'static str>,
        toggle: fn(&mut Self),
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        div()
            .id(id)
            .flex()
            .items_center()
            .gap_2()
            .cursor_pointer()
            .child(
                Icon::new(if open {
                    IconName::ChevronDown
                } else {
                    IconName::ChevronRight
                })
                .text_color(rgb(MUTED)),
            )
            .child(heading(title))
            .when_some(badge, |d, badge| {
                d.child(
                    div()
                        .px_2()
                        .rounded(px(4.))
                        .bg(rgb(0x3a3220))
                        .text_size(px(11.))
                        .text_color(rgb(WARN))
                        .child(badge),
                )
            })
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                toggle(this);
                cx.notify();
            }))
    }

    pub(super) fn render_connection(
        &mut self,
        status: &ConnectionStatus,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        if std::mem::take(&mut self.reveal_editor) {
            self.connection_scroll
                .scroll_to_top_of_item(CONFIGURATION_ITEM);
            if let Some(editor) = &self.config_editor {
                window.focus(&editor.read(cx).focus_handle(cx), cx);
            }
        }
        div()
            .id("connection-scroll")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.connection_scroll)
            .flex()
            .flex_col()
            .gap_4()
            .child(self.render_remote_card(status, cx))
            .child(self.render_setup_card(status, cx))
            .child(self.render_configuration_card(cx))
            .child(self.render_diagnostics_card(status, cx))
    }

    fn render_remote_card(
        &self,
        status: &ConnectionStatus,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let (headline, color) = status::headline(status);
        let primary = status::primary(status).filter(|n| n.step != status::Step::ChooseDevice);
        let secondary = status::secondary(status);
        let choosing = status.next_action == Some(SetupAction::SelectDevice);
        let reader = if !status.capture_authenticated {
            "Not running"
        } else if status.reading_verified {
            "Authenticated · reading verified"
        } else if status.capture_access == Access::Unknown {
            "Authenticated · input access not determined"
        } else if status.capture_access == Access::Denied {
            "Authenticated · input access not allowed"
        } else {
            "Authenticated · waiting for the first report"
        };
        let candidates = status
            .candidates
            .iter()
            .enumerate()
            .map(|(i, device)| {
                let id = device.id.clone();
                Button::new(("candidate", i))
                    .label(format!("{} · {}", device.name, short_id(&device.id)))
                    .small()
                    .outline()
                    .tooltip(device.id.clone())
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.run_command(ConnectionCommand::SelectDevice(id.clone()));
                        cx.notify();
                    }))
            })
            .collect::<Vec<_>>();
        card()
            .gap_4()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(dot(color))
                    .child(
                        div()
                            .text_size(px(20.))
                            .line_height(px(26.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(headline),
                    )
                    .when_some(status::observed_mode(status), |d, mode| {
                        d.child(div().text_size(px(14.)).text_color(rgb(MUTED)).child(mode))
                    })
                    .child(div().flex_1())
                    .when(status::can_disconnect(status), |d| {
                        d.child(
                            Button::new("connection-disconnect")
                                .label("Disconnect")
                                .small()
                                .outline()
                                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                    this.open_confirmation(window, cx);
                                })),
                        )
                    }),
            )
            .child(
                div()
                    .text_size(px(13.5))
                    .line_height(px(20.))
                    .text_color(rgb(TEXT))
                    .whitespace_normal()
                    .child(status.reason.clone()),
            )
            .when(primary.is_some() || secondary.is_some(), |d| {
                d.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .when_some(primary, |d, n| d.child(self.step_button("remote-primary", n, true, cx)))
                        .when_some(secondary, |d, n| d.child(self.step_button("remote-secondary", n, false, cx))),
                )
            })
            .child(
                Button::new("pair-remote")
                    .label("Pair a remote")
                    .outline()
                    .disabled(matches!(status.state, Readiness::Connecting | Readiness::Disconnecting))
                    .tooltip("Search directly for a remote in pairing mode. Stops the current ACA session; preserves mappings and macOS pairings.")
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.run_command(ConnectionCommand::DiscoverRemote);
                        cx.notify();
                    })),
            )
            .when(choosing, |d| {
                d.child(caption(if status.candidates.is_empty() {
                    "Waiting for remotes to choose from. Wake the remote you want to use."
                } else {
                    "Several remotes are available. Choose the one to use:"
                }))
                .child(div().flex().flex_wrap().gap_2().children(candidates))
            })
            .when_some(self.message("connection").cloned(), |d, m| d.child(notice(&m)))
            .when(status::stopped(status), |d| {
                d.child(caption(
                    "ACA no longer reads this remote. Its Bluetooth link may still be open; disconnect it in Bluetooth Settings if needed. ACA cannot verify that step.",
                ))
            })
            .child(rule())
            .child(fact(
                "Device",
                status
                    .device
                    .as_ref()
                    .map_or("None selected".to_string(), |d| d.name.clone()),
            ))
            .child(fact("Bluetooth link", status::transport(status.transport)))
            .child(fact("Reader", reader))
    }

    fn render_setup_card(
        &self,
        status: &ConnectionStatus,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let p = status.permissions;
        let capture = permissions::capture_observation(
            status.capture_access,
            status.capture_authenticated,
            status.reading_verified,
        );
        let settings = |id: &'static str, pane: &'static str, cx: &mut Context<Self>| {
            Button::new(id)
                .label("Open Settings")
                .small()
                .ghost()
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.report("connection", permissions::open_settings(pane));
                    cx.notify();
                }))
        };
        let needs_reader = matches!(
            status.next_action,
            Some(
                SetupAction::AuthorizeReader
                    | SetupAction::RetryReader
                    | SetupAction::EnableService
                    | SetupAction::ApproveService
            )
        );
        let row = |label: &'static str, state: Div, action: Option<AnyElement>| {
            div()
                .flex()
                .items_center()
                .gap_3()
                .min_h(px(32.))
                .text_size(px(13.))
                .child(
                    div()
                        .w(px(200.))
                        .flex_none()
                        .text_color(rgb(MUTED))
                        .child(label),
                )
                .child(div().flex_1().min_w_0().child(state))
                .when_some(action, |d, a| d.child(a))
        };
        card()
            .gap_2()
            .child(heading("Permissions & reader"))
            .when_some(status.service_error.clone(), |d, error| d.child(caption(error).text_color(rgb(DANGER))))
            .child(caption(
                if cfg!(feature = "managed-reader") { "ACA checks its own access and the reader's access separately. Choose Request reader access, then follow the entry macOS identifies in Privacy & Security. Reading is verified after input arrives." } else { "Studio and ACA Capture are separate apps in Privacy & Security; each needs its own Input Monitoring." },
            ))
            .child(row(
                if cfg!(feature = "managed-reader") { "ACA · Input Monitoring" } else { "Studio · Input Monitoring" },
                requirement(access(p.input), "Allowed", "Not allowed", "Not determined"),
                (!p.input.allowed()).then(|| settings("open-studio-input", "input", cx).into_any_element()),
            ))
            .child(row(
                if cfg!(feature = "managed-reader") { "ACA · Accessibility" } else { "Studio · Accessibility" },
                requirement(Some(p.accessibility), "Allowed", "Not allowed", ""),
                (!p.accessibility)
                    .then(|| settings("open-accessibility", "accessibility", cx).into_any_element()),
            ))
            .child(row(
                if cfg!(feature = "managed-reader") { "Reader · Input Monitoring" } else { "Capture · Input Monitoring" },
                requirement(access(capture), "Allowed", "Not allowed", "Not determined"),
                if status.next_action == Some(SetupAction::RequestReaderInput) && !status::stopped(status) {
                    Some(self.step_button(
                        "request-reader-input",
                        status::NextStep {
                            label: "Request reader access",
                            step: status::Step::Run(ConnectionCommand::RequestReaderInput),
                        },
                        true,
                        cx,
                    ).into_any_element())
                } else {
                    (capture == Access::Denied)
                        .then(|| settings("open-capture-input", "input", cx).into_any_element())
                },
            ))
            .when(cfg!(feature = "managed-reader"), |d| d.child(row(
                "Background service",
                requirement(
                    Some(status.service_status == ready_for_air::service::Status::Enabled),
                    status.service_status.label(), status.service_status.label(), "Not checked",
                ),
                Some(Button::new("open-background-settings").label("Open Login Items").small().ghost()
                    .on_click(cx.listener(|_, _: &ClickEvent, _, _| ready_for_air::service::open_settings())).into_any_element()),
            )))
            .child(row(
                if cfg!(feature = "managed-reader") { "Remote reader" } else { "Reader session" },
                requirement(
                    Some(status.capture_authenticated),
                    if cfg!(feature = "managed-reader") { "Authenticated" } else { "Authorized (up to 30 minutes per session)" },
                    if self.studio.capture_starting() {
                        "Waiting for macOS authorization…"
                    } else {
                        if cfg!(feature = "managed-reader") { "Not authenticated" } else { "Not authorized" }
                    },
                    "",
                ),
                needs_reader.then(|| {
                    self.step_button(
                        "authorize-reader",
                        status::NextStep {
                            label: status::primary(status).map_or("Set up ACA", |step| step.label),
                            step: status::Step::Run(ConnectionCommand::ResolveSetup),
                        },
                        true,
                        cx,
                    )
                    .into_any_element()
                }),
            ))
            .child(row(
                "Bluetooth discovery",
                requirement(
                    access(p.bluetooth),
                    "Allowed",
                    "Not allowed · paired remotes still work",
                    "Asked only when searching for a new remote",
                ),
                (p.bluetooth == Access::Denied)
                    .then(|| settings("open-bluetooth-privacy", "bluetooth", cx).into_any_element()),
            ))
            .child(
                div()
                    .pt_2()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(caption(if cfg!(feature = "managed-reader") { "macOS decides which app needs Input Monitoring. If its entry is missing, use + to add the exact ACA or bundled ACA Reader shown in Finder. macOS may ask you to quit and reopen the app." } else { "Missing from a list in Settings? Add it with + from the app folder." }))
                    .when(cfg!(feature = "managed-reader"), |d| d.child(
                        Button::new("show-internal-reader").label("Show reader").small().ghost()
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                let result = std::env::current_exe().map_err(|e| e.to_string())
                                    .and_then(|exe| ready_for_air::session::packaged_helper(&exe))
                                    .and_then(|helper| {
                                        ready_for_air::session::verify_packaged_helper(&helper)?;
                                        let bundle = helper.ancestors().nth(3).ok_or("Reader bundle not found")?;
                                        std::process::Command::new("/usr/bin/open").arg("-R").arg(bundle).status()
                                            .map_err(|e| e.to_string()).and_then(|s| if s.success() { Ok("ACA Reader shown in Finder. Add this app only if macOS identifies it as the entry that needs Input Monitoring.".into()) } else { Err("Could not open Finder".into()) })
                                    });
                                this.report("connection", result); cx.notify();
                            }))
                    ))
                    .child(
                        Button::new("show-permission-apps")
                            .label(if cfg!(feature = "managed-reader") { "Show ACA" } else { "Show apps" })
                            .small()
                            .ghost()
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                let r = std::env::current_exe().map_err(|e| e.to_string()).and_then(|exe| {
                                    let app = exe.ancestors().nth(3).ok_or("App bundle not found")?;
                                    let status = std::process::Command::new("/usr/bin/open")
                                        .arg("-R")
                                        .arg(app)
                                        .status()
                                        .map_err(|e| e.to_string())?;
                                    if status.success() {
                                        Ok("App folder opened".into())
                                    } else {
                                        Err("Could not open Finder".into())
                                    }
                                });
                                this.report("connection", r);
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("bluetooth-settings")
                            .label("Bluetooth Settings")
                            .small()
                            .ghost()
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.report("connection", permissions::open_settings("radio"));
                                cx.notify();
                            })),
                    ),
            )
    }

    fn render_configuration_card(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let s = &self.studio;
        let editor = self.config_editor.clone();
        let draft = editor
            .as_ref()
            .is_some_and(|e| e.read(cx).value().as_ref() != self.config_editor_key.as_str());
        let open = self.configuration_open;
        card()
            .child(self.disclosure(
                "configuration-toggle",
                "Configuration JSON",
                open,
                draft.then_some("Unsaved draft"),
                |this| this.configuration_open = !this.configuration_open,
                cx,
            ))
            .when(!open, |d| {
                d.child(caption(
                    "The full configuration, including scripts, macros and scrolls that presets do not cover.",
                ))
            })
            .when(open, |d| {
                d.when_some(s.migration_notice.clone(), |d, message| d.child(notice(&(message, false))))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .font_family(MONO)
                                    .text_size(px(12.))
                                    .text_color(rgb(MUTED))
                                    .child(s.config_path().display().to_string()),
                            )
                            .child(Button::new("reveal-config").label("Reveal in Finder").small().ghost().on_click(
                                cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.reveal_config();
                                    cx.notify();
                                }),
                            )),
                    )
                    .child(caption(format!(
                        "Validated before saving and applied immediately. The air mouse idle timeout is {} ms: after that much silence ACA returns to the Normal profile (a software default, not a hardware query).",
                        s.config.mouse_idle_ms
                    )))
                    .when_some(editor, |d, editor| {
                        d.child(
                            div()
                                .font_family(MONO)
                                .text_size(px(12.))
                                .child(Textarea::new(&editor).h(px(340.))),
                        )
                    })
                    .when_some(self.message("config").cloned(), |d, m| d.child(notice(&m)))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .justify_end()
                            .gap_2()
                            .child(Button::new("reload-config").label("Discard draft and reload").small().ghost().on_click(
                                cx.listener(|this, _: &ClickEvent, _, cx| {
                                    let r = this
                                        .studio
                                        .reload()
                                        .map(|_| "Reloaded from disk".to_string())
                                        .map_err(|e| format!("Reload failed: {e}"));
                                    if r.is_ok() {
                                        this.drafts.discard("config");
                                        this.config_editor_key.clear();
                                    }
                                    this.report("config", r);
                                    cx.notify();
                                }),
                            ))
                            .child(Button::new("apply-config").label("Validate & save").small().primary().disabled(!draft).on_click(
                                cx.listener(|this, _: &ClickEvent, _, cx| {
                                    let Some(editor) = this.config_editor.clone() else {
                                        return;
                                    };
                                    let text = editor.read(cx).value();
                                    let model = serde_json::to_string_pretty(&this.studio.config).unwrap_or_default();
                                    // Capture typing even if no render happened since the last keystroke.
                                    this.drafts.sync("config", &model, Some(("config", text.as_str())));
                                    let r = this.drafts.check_save("config", &model).and_then(|_| serde_json::from_str::<Config>(&text)
                                        .map_err(|e| format!("Invalid configuration JSON: {e}"))
                                        .and_then(|config| {
                                            config.validate().map_err(|e| format!("Invalid configuration: {e}"))?;
                                            this.studio
                                                .save_config(config)
                                                .map(|_| "Configuration saved".to_string())
                                                .map_err(|e| format!("Not saved: {e}"))
                                        }));
                                    if r.is_ok() { this.drafts.discard("config"); this.config_editor_key.clear(); }
                                    this.report("config", r);
                                    cx.notify();
                                }),
                            )),
                    )
            })
    }

    fn render_diagnostics_card(
        &self,
        status: &ConnectionStatus,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let s = &self.studio;
        let open = self.diagnostics_open;
        card()
            .child(self.disclosure(
                "diagnostics-toggle",
                "Diagnostics",
                open,
                None,
                |this| this.diagnostics_open = !this.diagnostics_open,
                cx,
            ))
            .when(open, |d| {
                d.child(fact("State", format!("{:?}", status.state)))
                    .child(fact(
                        "Device identifier",
                        div()
                            .truncate()
                            .font_family(MONO)
                            .text_size(px(12.))
                            .child(status.device.as_ref().map_or("—".to_string(), |d| d.id.clone())),
                    ))
                    .when(cfg!(feature = "managed-reader"), |d| d.child(
                        Button::new("remove-background-service").label("Remove background service").small().ghost()
                            .disabled(status.reconnect_allowed || status.state == Readiness::Disconnecting || self.studio.capture_starting())
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.run_command(ConnectionCommand::RemoveService); cx.notify();
                            }))
                    ))
                    .child(fact("Bluetooth adapter", s.bluetooth_state.clone()))
                    .child(fact("Exclusive reader", if status.exclusive_reader { "Active" } else { "Inactive" }))
                    .child(fact(
                        "Packets",
                        format!("{} · last {}", s.packet_count, s.last_packet),
                    ))
                    .when(status.state == Readiness::Failed, |d| {
                        d.child(caption("If stopping takes too long, wait for the worker to finish or quit ACA before retrying."))
                    })
                    .child(caption(
                        "Automation is requested by macOS the first time an AppleScript action targets an app; failures appear in Activity.",
                    ))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                Button::new("automation-settings")
                                    .label("Automation permissions")
                                    .small()
                                    .ghost()
                                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                        this.report("connection", permissions::open_settings("automation"));
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("retry-reader")
                                    .label("Restart reader")
                                    .small()
                                    .ghost()
                                    .disabled(!status.reconnect_allowed || self.studio.capture_starting())
                                    .tooltip(if cfg!(feature = "managed-reader") { "Releases the remote and reconnects to the approved service" } else { "Stops and restarts the reader; macOS may ask for authorization again" })
                                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                        this.run_command(ConnectionCommand::RetrySetup);
                                        cx.notify();
                                    })),
                            ),
                    )
            })
    }
}
