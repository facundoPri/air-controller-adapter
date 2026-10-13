//! CONNECTION view: remote link, microphone/voice and configuration file.

use super::{StudioApp, theme::*};
use gpui_kit::{
    base::Disableable,
    component::{
        Sizable,
        button::{Button, ButtonVariants},
        input::Textarea,
    },
    prelude::FluentBuilder,
    *,
};
use ready_for_air::studio::Config;

fn state_row(
    label: &'static str,
    on: bool,
    on_text: &'static str,
    off_text: &'static str,
    on_color: u32,
) -> Div {
    div()
        .flex()
        .items_center()
        .gap_3()
        .text_size(px(13.))
        .child(dot(if on { on_color } else { FAINT }))
        .child(div().w(px(90.)).text_color(rgb(MUTED)).child(label))
        .child(
            div()
                .text_color(rgb(TEXT))
                .child(if on { on_text } else { off_text }),
        )
}

impl StudioApp {
    pub(super) fn render_connection(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let s = &self.studio;

        use ready_for_air::permissions::{self, Access};
        let capture = permissions::capture_observation(
            s.capture_access,
            s.capture_service,
            s.capture_verified,
        );
        let checks = [
            (
                "Studio · Accesibilidad",
                if ready_for_air::native_input::trusted() {
                    Access::Granted
                } else {
                    Access::Denied
                },
                "accessibility",
            ),
            (
                "Studio · Monitoreo de entrada",
                permissions::input(),
                "input",
            ),
            ("Capture · Monitoreo de entrada", capture, "input"),
            (
                "Bluetooth · permiso para buscar",
                permissions::optional("bluetooth"),
                "bluetooth",
            ),
            (
                "Micrófono · opcional, solo voz",
                permissions::optional("microphone"),
                "microphone",
            ),
        ];
        let mut setup = card().child(heading("Preparar esta Mac"))
            .child(caption("1. Encendé el control. Si ya está conectado, se detecta solo. Para emparejarlo: Atrás + OK; luego Buscar. Si hay varios, elegí uno."))
            .child(caption("2. Habilitá Studio en Accesibilidad y Monitoreo de entrada. Autorizá el lector cuando macOS lo pida; Capture comprueba su propio permiso, separado del de Studio."));
        for (index, (label, access, pane)) in checks.into_iter().enumerate() {
            setup = setup.child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(div().flex_1().child(format!("{label}: {}", access.label())))
                    .child(
                        Button::new(("permission", index))
                            .label("Abrir Ajustes")
                            .small()
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.report("connection", permissions::open_settings(pane));
                                cx.notify();
                            })),
                    ),
            );
        }
        setup = setup.child(caption(format!("Adaptador Bluetooth: {}. El estado se consulta al buscar; tener permiso no implica que esté encendido.", s.bluetooth_state)))
            .child(caption("3. Cuando diga Lectura comprobada, llegaron paquetes por Capture. Permiso concedido solo confirma TCC; Canal autenticado solo confirma la conexión entre procesos. Si cambió la firma, quitá y agregá la app exacta en Ajustes y reabrila."))
            .child(caption("Automation es opcional y depende de la acción y aplicación destino. macOS lo pide al ejecutar un AppleScript; los fallos se muestran en Activity. No se solicitan permisos de audio o Automation al iniciar."))
            .child(Button::new("bluetooth-settings").label("Ajustes de Bluetooth").small().ghost().on_click(cx.listener(|this, _, _, cx| {
                this.report("connection", permissions::open_settings("radio")); cx.notify();
            })))
            .child(caption("Si una app no aparece en Monitoreo de entrada, usá + para agregar Studio o Capture desde esta carpeta. Habilitá solo el requisito pendiente; macOS puede pedir cerrar y reabrir."))
            .child(Button::new("show-permission-apps").label("Mostrar apps para permisos").small().ghost().on_click(cx.listener(|this, _, _, cx| {
                let r = std::env::current_exe().map_err(|e|e.to_string()).and_then(|exe| {
                    let app = exe.ancestors().nth(3).ok_or("No se encontró el bundle")?;
                    let status = std::process::Command::new("/usr/bin/open").arg("-R").arg(app).status().map_err(|e|e.to_string())?;
                    if status.success() { Ok("Carpeta de apps abierta".into()) } else { Err("No se pudo abrir Finder".into()) }
                });
                this.report("connection", r); cx.notify();
            })))
            .child(Button::new("automation-settings").label("Permisos de Automation").small().ghost().on_click(cx.listener(|this, _, _, cx| {
                this.report("connection", permissions::open_settings("automation")); cx.notify();
            })))
            .child(Button::new("recheck-permissions").label("Volver a comprobar / reintentar lector").small().on_click(cx.listener(|this, _, _, cx| {
                let r = this.studio.start_capture_service(); this.report("connection",r); cx.notify();
            })));

        let remote = card()
            .flex_1()
            .min_w(px(300.))
            .child(heading("Remote"))
            .child(state_row("Control", s.connected, "Presente", if s.connecting { "Searching…" } else { "Not connected" }, OK))
            .child(state_row("HID capture", s.listening, "Listening", "Stopped", OK))
            .child(state_row("Actions service", s.capture_service, "Canal autenticado", "No iniciado", OK))
            .when(s.needs_retry(), |d| d.child(
                Button::new("start-capture-service").label("Reanudar lector").small().disabled(s.capture_starting())
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        let result = this.studio.start_capture_service();
                        this.report("connection", result);
                        cx.notify();
                    }))
            ))
            .child(caption("Studio prepara el control automáticamente al abrirse. Si macOS pide autorización, usá su diálogo. La sesión actual dura hasta 30 minutos; al terminar, Reanudar lector la renueva. No se abre Terminal ni se instala un servicio permanente."))
            .when_some(self.message("connection").cloned(), |d, m| d.child(notice(&m)))
            .child(state_row(
                "Mode",
                s.listening,
                mode_name(s.mode),
                "Normal (default)",
                ACCENT,
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .when(!s.connected, |d| d.child(
                        Button::new("connect")
                            .label(if s.connecting { "Searching…" } else { "Discover & connect" })
                            .small()
                            .primary()
                            .loading(s.connecting)
                            .disabled(s.connecting)
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.studio.connect();
                                cx.notify();
                            })),
                    ))

            )
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(rgb(TEXT))
                    .whitespace_normal()
                    .child(format!("Status: {}", s.status)),
            )
            .children(s.candidates.iter().enumerate().map(|(i,(id,name))| {
                let id=id.clone();
                Button::new(("candidate",i)).label(format!("{name} · {id}")).small()
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {this.studio.connect_candidate(id.clone());cx.notify();}))
            }))
            .child(caption(
                "Pairing: hold Back + OK until the LED blinks. If a phone grabs the remote, turn off the phone's Bluetooth first. Already paired remotes are detected automatically. Studio reconnects when they wake up. Studio needs Input Monitoring and Accessibility; ACA Capture needs Input Monitoring and administrator authentication.",
            ))
            .child(caption(format!(
                "Packets: {} · last: {}",
                s.packet_count, s.last_packet
            )));

        let selected_input = s.config.audio_input.clone();
        let input_row = |id: (&'static str, usize),
                         name: Option<String>,
                         label: String,
                         this: &Self,
                         cx: &mut Context<Self>| {
            let selected = selected_input == name;
            div()
                .id(id)
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .h(px(32.))
                .rounded(px(4.))
                .text_size(px(13.))
                .cursor_pointer()
                .map(|d| {
                    if selected {
                        d.bg(rgb(ACCENT))
                            .text_color(rgb(ACCENT_TEXT))
                            .font_weight(FontWeight::SEMIBOLD)
                    } else if this.studio.recording {
                        d.text_color(rgb(FAINT))
                    } else {
                        d.text_color(rgb(TEXT)).hover(|s| s.bg(rgb(HOVER)))
                    }
                })
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(label),
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    if this.studio.recording {
                        this.report(
                            "audio",
                            Err("Stop the recording before changing the input".into()),
                        );
                    } else {
                        let mut next = this.studio.config.clone();
                        next.audio_input = name.clone();
                        let r = this
                            .studio
                            .save_config(next)
                            .map(|_| {
                                format!(
                                    "Microphone: {}",
                                    name.as_deref().unwrap_or("System default")
                                )
                            })
                            .map_err(|e| format!("Input not changed: {e}"));
                        this.report("audio", r);
                    }
                    cx.notify();
                }))
        };
        let mut inputs = vec![input_row(
            ("input", 0),
            None,
            "System default".into(),
            self,
            cx,
        )];
        for (i, name) in self.studio.audio_inputs.clone().into_iter().enumerate() {
            inputs.push(input_row(
                ("input", i + 1),
                Some(name.clone()),
                name,
                self,
                cx,
            ));
        }
        let s = &self.studio;
        let transcriber = match &s.config.transcription {
            Some(t) => format!(
                "Local transcriber: {} {}{}",
                t.program,
                t.args.join(" "),
                if t.insert_text { " · inserts text into the focused app" } else { "" }
            ),
            None => "No local transcriber configured — recordings are saved as audio files. Add a \"transcription\" entry in the configuration JSON below.".into(),
        };

        let microphone = card()
            .flex_1()
            .min_w(px(300.))
            .child(heading("Microphone"))
            .child(div().flex().flex_col().gap_1().children(inputs))
            .when(s.audio_inputs.is_empty(), |d| {
                d.child(caption("No input devices listed. Press Refresh; macOS may ask for Microphone permission."))
            })
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Button::new("refresh-inputs")
                            .label(if s.audio_refreshing { "Loading inputs…" } else { "Refresh" })
                            .small()
                            .ghost()
                            .disabled(s.recording || s.audio_refreshing)
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.studio.refresh_audio_inputs();
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("record")
                            .label(if s.recording { "Stop recording" } else { "Record" })
                            .small()
                            .map(|b| if s.recording { b.danger() } else { b.primary() })
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.studio.toggle_recording();
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(rgb(if s.recording { DANGER } else { TEXT }))
                    .child(s.audio_status.clone()),
            )
            .child(caption(transcriber))
            .when_some(self.message("audio").cloned(), |d, m| d.child(notice(&m)));

        let editor = self.config_editor.clone();
        let configuration = card()
            .child(heading("Configuration"))
            .child(
                div()
                    .font_family(MONO)
                    .text_size(px(12.))
                    .text_color(rgb(MUTED))
                    .child(s.config_path().display().to_string()),
            )
            .child(caption(format!(
                "Air mouse idle timeout: {} ms (returns to the Normal profile after silence; this is a software default, not a hardware status query).",
                s.config.mouse_idle_ms
            )))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(Button::new("reveal-2").label("Reveal in Finder").small().on_click(
                        cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.reveal_config();
                            cx.notify();
                        }),
                    ))
                    .child(Button::new("reload-2").label("Descartar borrador y recargar").small().ghost().on_click(
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
                    .child(Button::new("apply-config").label("Validate & save JSON").small().primary().on_click(
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
            .when_some(self.message("config").cloned(), |d, m| d.child(notice(&m)))
            .child(caption(
                "Configuración completa en JSON. Se valida, guarda y aplica automáticamente.",
            ))
            .when_some(editor, |d, editor| {
                d.child(
                    div()
                        .font_family(MONO)
                        .text_size(px(12.))
                        .child(Textarea::new(&editor).h(px(320.))),
                )
            });

        div()
            .id("connection-scroll")
            .size_full()
            .overflow_y_scroll()
            .pr(px(44.))
            .pb_4()
            .flex()
            .flex_col()
            .gap_4()
            .child(setup)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_4()
                    .child(remote)
                    .child(microphone),
            )
            .child(configuration)
    }
}
