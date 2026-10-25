use gpui_kit::*;
use ready_for_air::native_keyboard::{self, Target};
use std::time::Duration;

struct Probe {
    target: Option<Target>,
    status: String,
    sequence: u64,
    drag: bool,
}
impl Render for Probe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(rgb(0x20242c))
            .text_color(rgb(0xf1f4fa))
            .p_3()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .id("drag")
                    .h_8()
                    .cursor_move()
                    .child("ACA keyboard experiment · drag here")
                    .on_mouse_down(MouseButton::Left, cx.listener(|s, _, _, _| s.drag = true))
                    .on_mouse_move(cx.listener(|s, _, w, _| {
                        if s.drag {
                            s.drag = false;
                            w.start_window_move();
                        }
                    }))
                    .on_mouse_up(MouseButton::Left, cx.listener(|s, _, _, _| s.drag = false)),
            )
            .child(
                div().flex().gap_2().children(
                    ["a", "ñ", "é", "Backspace", "Tab", "Return"]
                        .into_iter()
                        .enumerate()
                        .map(|(i, label)| {
                            div()
                                .id(("key", i))
                                .px_3()
                                .py_3()
                                .rounded_md()
                                .bg(rgb(0x394455))
                                .cursor_pointer()
                                .child(label)
                                .on_click(cx.listener(move |s, _, w, cx| {
                                    let Some(target) = &s.target else {
                                        return;
                                    };
                                    s.sequence += 1;
                                    let result = match label {
                                        "Backspace" => target.control(51),
                                        "Tab" => target.control(48),
                                        "Return" => target.control(36),
                                        text => target.text(text),
                                    };
                                    if result.is_err() || matches!(label, "Tab" | "Return") {
                                        s.target = None;
                                        w.remove_window();
                                    }
                                    s.status = match result {
                                        Ok(()) => format!(
                                            "Dispatch {}. Delivery must be observed in the target.",
                                            s.sequence
                                        ),
                                        Err(e) => e,
                                    };
                                    cx.notify();
                                }))
                        }),
                ),
            )
            .child(div().text_sm().child(self.status.clone()))
    }
}
fn main() {
    if std::env::args().any(|a| a == "--target-status") {
        println!(
            "Accessibility trusted: {}",
            ready_for_air::native_input::trusted()
        );
        println!(
            "Target: {}",
            Target::capture()
                .map(|_| "verified".to_string())
                .unwrap_or_else(|e| e)
        );
        return;
    }
    gpui_kit::application().run(|cx| {
        gpui_kit::init(cx);
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        cx.spawn(async move |cx| {
            cx.background_executor().timer(Duration::from_secs(5)).await;
            let _ = cx.update(|cx| {
                let target = Target::capture();
                let status = target
                    .as_ref()
                    .map(|_| "Target captured. Clipboard is not used.".to_string())
                    .unwrap_or_else(|e| e.clone());
                eprintln!("{status}");
                let (handle, entity) = gpui_kit::open_window(
                    WindowOptions {
                        kind: WindowKind::PopUp,
                        focus: false,
                        show: true,
                        titlebar: Some(TitlebarOptions {
                            title: Some("ACA keyboard experiment".into()),
                            appears_transparent: true,
                            ..Default::default()
                        }),
                        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                            point(px(100.), px(100.)),
                            size(px(620.), px(160.)),
                        ))),
                        is_resizable: false,
                        is_minimizable: false,
                        app_owns_titlebar_drag: true,
                        ..Default::default()
                    },
                    cx,
                    |_, cx| {
                        cx.new(|_| Probe {
                            target: target.ok(),
                            status,
                            sequence: 0,
                            drag: false,
                        })
                    },
                )
                .unwrap();
                if !native_keyboard::prepare_panel("ACA keyboard experiment") {
                    eprintln!("Panel guard unavailable");
                    let _ = handle.update(cx, |_, w, _| w.remove_window());
                    return;
                }
                cx.spawn(async move |cx| {
                    loop {
                        cx.background_executor()
                            .timer(Duration::from_millis(100))
                            .await;
                        let closed = cx.update(|cx| {
                            let invalid =
                                entity.read(cx).target.as_ref().is_some_and(|t| !t.valid());
                            if invalid {
                                let _ = handle.update(cx, |_, w, _| w.remove_window());
                            }
                            invalid
                        });
                        if closed {
                            break;
                        }
                    }
                })
                .detach();
            });
        })
        .detach();
    });
}
