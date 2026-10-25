//! Inspector for the selected button: name and current action first, then
//! the grouped action catalog. Choosing an action saves it immediately and
//! never runs it; identification and testing live in collapsed advanced
//! options.

use super::{StudioApp, catalog, theme::*};
use gpui_kit::{
    base::Disableable,
    component::{
        Icon, IconName, Sizable,
        button::{Button, ButtonVariants},
    },
    prelude::FluentBuilder,
    *,
};
use ready_for_air::{
    actions,
    studio::{Action, ButtonId, Profile},
};

impl StudioApp {
    fn assign(&mut self, action: Action) {
        let (b, p) = (self.selected, self.profile);
        let previous = action_name(self.studio.action(b, p));
        let name = action_name(&action);
        let result = self
            .studio
            .assign_action(b, p, action)
            .map(|_| {
                if self.studio.button_identified(b, p) {
                    format!("Saved · {name}")
                } else {
                    format!(
                        "Saved · {name}. This input is also used by another button; see Advanced."
                    )
                }
            })
            // Saving persists first, so a failure leaves the old mapping in place.
            .map_err(|e| {
                format!(
                    "Not saved: {}. {previous} is still assigned.",
                    e.trim_end_matches('.')
                )
            });
        self.messages.remove("test");
        self.report("action", result);
    }

    pub(super) fn render_inspector(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let b = self.selected;
        let profile = self.profile;
        let current = self.studio.action(b, profile).clone();
        let air_mouse_button = b == ButtonId::MouseMode;

        let nav = |id: &'static str,
                   icon: IconName,
                   delta: isize,
                   tip: &'static str,
                   cx: &mut Context<Self>| {
            Button::new(id)
                .icon(Icon::new(icon))
                .small()
                .ghost()
                .tooltip(tip)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.step_selection(delta);
                    cx.notify();
                }))
        };

        let title = div()
            .flex()
            .items_start()
            .gap_2()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(eyebrow(format!(
                        "{} PROFILE",
                        profile_name(profile).to_uppercase()
                    )))
                    .child(
                        div()
                            .text_size(px(26.))
                            .line_height(px(32.))
                            .font_weight(FontWeight::BOLD)
                            .child(button_name(b)),
                    )
                    .child(
                        div()
                            .text_size(px(14.))
                            .line_height(px(20.))
                            .text_color(rgb(ACCENT))
                            .child(if air_mouse_button {
                                "Switches the air mouse".to_string()
                            } else {
                                action_name(&current)
                            }),
                    ),
            )
            // Every button stays reachable even when the stage is too small to label it.
            .child(nav(
                "previous-button",
                IconName::ChevronUp,
                -1,
                "Previous button  ( ↑ )",
                cx,
            ))
            .child(nav(
                "next-button",
                IconName::ChevronDown,
                1,
                "Next button  ( ↓ )",
                cx,
            ));

        let body = if air_mouse_button {
            self.render_air_mouse_note(profile, cx).into_any_element()
        } else {
            self.render_catalog(&current, cx).into_any_element()
        };

        div()
            .id("inspector")
            .w(px(INSPECTOR_W))
            .flex_none()
            .h_full()
            .overflow_y_scroll()
            .child(
                card()
                    .gap_4()
                    .child(title)
                    .map(|d| {
                        #[cfg(feature = "virtual-keyboard")]
                        let d = {
                            let session = self.studio.keyboard_control.snapshot();
                            if profile == Profile::Keyboard && session.opener == Some(b)
                                && session.phase != ready_for_air::keyboard::Phase::Hidden
                            {
                                d.child(caption(format!(
                                    "{} opened this session and currently closes the keyboard. Its saved action ({}) is temporarily displaced. Assign that action to another button, or use Home or Capture to open the keyboard.",
                                    button_name(b), action_name(&current)
                                )))
                            } else if matches!(current, Action::Keyboard { command: ready_for_air::keyboard_model::KeyboardAction::Toggle }) {
                                d.child(caption(if profile == Profile::Keyboard {
                                    "This closes a visible keyboard. To open it, also assign Toggle keyboard in Normal or Air mouse."
                                } else {
                                    "This button will always close the keyboard it opens. Its Keyboard action is temporarily displaced. Use Home or Capture to preserve arrows and OK, or assign the displaced action to another button."
                                }))
                            } else {
                                d
                            }
                        };
                        d
                    })
                    .when_some(self.message("action").cloned(), |d, m| d.child(notice(&m)))
                    .when(b.is_arrow(), |d| {
                        d.child(caption(
                            "Arrows also turn an active air mouse off. Measured for Up; Down, Left and Right are user-reported.",
                        ))
                    })
                    .child(body)
                    .child(rule())
                    .child(self.render_advanced(&current, cx)),
            )
    }

    /// The firmware handles this button; it has no press of its own to map.
    fn render_air_mouse_note(
        &self,
        profile: Profile,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(caption(
                "This button turns the remote's air mouse on and off in firmware. It sends no event of its own, so it cannot be assigned or identified. ACA infers the mode from the mouse reports that follow.",
            ))
            .child(caption(
                "To change what the other buttons do while the air mouse is on, edit the Air mouse profile.",
            ))
            .when(profile == Profile::Normal, |d| {
                d.child(
                    Button::new("edit-r-mouse")
                        .label("Edit Air mouse profile")
                        .small()
                        .outline()
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.set_profile(Profile::AirMouse);
                            cx.notify();
                        })),
                )
            })
    }

    fn render_catalog(&self, current: &Action, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let custom = !catalog::is_preset(current);
        let groups = catalog::grouped().into_iter().map(|(group, actions)| {
            let chips = actions.into_iter().map(|action| {
                let selected = &action == current;
                let label = action_name(&action);
                Button::new(SharedString::from(format!("preset-{label}")))
                    .label(label)
                    .small()
                    .map(|b| if selected { b.primary() } else { b.outline() })
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.assign(action.clone());
                        cx.notify();
                    }))
            });
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(eyebrow(group.title()))
                .child(div().flex().flex_wrap().gap(px(6.)).children(chips))
        });
        let guidance = actions::guidance(current);
        div()
            .flex()
            .flex_col()
            .gap_4()
            .when(custom, |d| {
                d.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .p_3()
                        .rounded(px(8.))
                        .bg(rgb(ACCENT_SOFT))
                        .border_1()
                        .border_color(rgb(0x1d5a54))
                        .child(eyebrow("CUSTOM ACTION"))
                        .child(caption(format!(
                            "{} is defined in the configuration JSON. Choosing a preset below replaces it.",
                            action_name(current)
                        )))
                        .child(
                            Button::new("edit-custom")
                                .label("Edit in configuration JSON")
                                .small()
                                .outline()
                                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.open_configuration();
                                    cx.notify();
                                })),
                        ),
                )
            })
            .children(groups)
            .when(!guidance.is_empty(), |d| d.child(caption(guidance)))
            // Zoom prerequisites appear only where they apply.
            .when(actions::uses_zoom(current), |d| {
                d.child(
                    Button::new("zoom-settings")
                        .label("Open Zoom settings")
                        .small()
                        .outline()
                        .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                            cx.open_url(
                                "x-apple.systempreferences:com.apple.preference.universalaccess?Zoom",
                            );
                        })),
                )
            })
    }

    fn render_advanced(
        &self,
        current: &Action,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let b = self.selected;
        let profile = self.profile;
        let studio = &self.studio;
        let learning_this = studio.learning == Some((b, profile));
        let learning_any = studio.learning.is_some();
        // Identification needs the reader; the Air mouse button has nothing to identify.
        let can_identify = b != ButtonId::MouseMode && studio.listening;
        let open = self.advanced_open || learning_any;
        let identified = studio.button_identified(b, profile);

        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .id("advanced-toggle")
                    .flex()
                    .items_center()
                    .gap_2()
                    .cursor_pointer()
                    .text_size(px(12.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(MUTED))
                    .hover(|s| s.text_color(rgb(TEXT)))
                    .child(Icon::new(if open {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    }))
                    .child("Advanced")
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.advanced_open = !this.advanced_open;
                        cx.notify();
                    })),
            )
            .when(open, |d| {
                d.when(b != ButtonId::MouseMode, |d| {
                    d.child(fact(
                        "Detected input",
                        div().truncate().child(if identified {
                            studio.source_label(b, profile)
                        } else {
                            "Built-in MR-01 map".into()
                        }),
                    ))
                })
                .when(learning_this, |d| {
                    d.child(
                        div()
                            .text_size(px(12.5))
                            .text_color(rgb(WARN))
                            .child(format!(
                                "Press {} once on the remote… (Esc to cancel)",
                                button_name(b)
                            )),
                    )
                })
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .when(b != ButtonId::MouseMode, |d| {
                            d.child(
                                Button::new("identify")
                                    .label(if learning_any {
                                        "Cancel identification"
                                    } else if identified {
                                        "Identify again"
                                    } else {
                                        "Identify button"
                                    })
                                    .small()
                                    .outline()
                                    .disabled(!learning_any && !can_identify)
                                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                        if this.studio.learning.is_some() {
                                            this.studio.cancel_learning();
                                            this.report("action", Ok("Identification canceled".into()));
                                        } else {
                                            this.studio.learn(this.selected, this.profile);
                                            this.report(
                                                "action",
                                                Ok(format!(
                                                    "Press {} once on the remote (Esc to cancel)",
                                                    button_name(this.selected)
                                                )),
                                            );
                                        }
                                        cx.notify();
                                    })),
                            )
                        })
                        .child(
                            Button::new("test-action")
                                .label("Test action")
                                .small()
                                .outline()
                                .disabled(matches!(current, Action::Default | Action::Disabled))
                                .tooltip("Runs the assigned action once now")
                                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                    let r = this
                                        .studio
                                        .test_action(this.selected, this.profile)
                                        .map(|_| "Action requested · see Activity for the result".into());
                                    this.report("test", r);
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("view-configuration")
                                .label("View configuration JSON")
                                .small()
                                .ghost()
                                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.open_configuration();
                                    cx.notify();
                                })),
                        ),
                )
                .when(b != ButtonId::MouseMode && !can_identify && !learning_any, |d| {
                    d.child(caption("Identify again needs a connected remote. Standard MR-01 buttons work without it."))
                })
                .when_some(self.message("test").cloned(), |d, m| d.child(notice(&m)))
                .when_some(studio.last_action_result.as_ref(), |d, result| d.child(notice(result)))
            })
    }
}
