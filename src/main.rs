mod ui;
use gpui_kit::*;
use std::path::PathBuf;
gpui_kit::actions!(ready_air, [Quit]);

fn main() {
    let mut root = if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
            .join("Library/Application Support/ReadyForAirLab")
    } else if cfg!(target_os = "windows") {
        PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default()).join("ReadyForAirLab")
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share")
            })
            .join("ready-for-air")
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--data-root" => root = args.next().expect("--data-root needs a path").into(),
            "--verify-capture" => {
                let result = std::env::current_exe()
                    .map_err(|e| e.to_string())
                    .and_then(|exe| ready_for_air::session::packaged_helper(&exe))
                    .and_then(|helper| ready_for_air::session::verify_packaged_helper(&helper));
                match result {
                    Ok(()) => println!("Packaged reader verified"),
                    Err(e) => {
                        eprintln!("{e}");
                        std::process::exit(1);
                    }
                }
                return;
            }
            "--build-info" | "--service-status" | "--enable-service" | "--remove-service"
            | "--probe-reader" => {
                let result = ready_for_air::service::command(&arg);
                match result {
                    Ok(text) => println!("{text}"),
                    Err(error) => {
                        eprintln!("{error}");
                        std::process::exit(1);
                    }
                }
                return;
            }
            "--inspect" => {
                match ready_for_air::hardware::inspect_hid() {
                    Ok(lines) => {
                        for line in lines {
                            println!("{line}");
                        }
                    }
                    Err(e) => eprintln!("HID: {e}"),
                }
                return;
            }
            _ => {
                eprintln!(
                    "Usage: ready-for-air [--data-root PATH] [--inspect] [--verify-capture] [--build-info] [--service-status] [--enable-service] [--remove-service] [--probe-reader]"
                );
                std::process::exit(2);
            }
        }
    }
    match ready_for_air::native_keyboard::qa_data_root() {
        Ok(Some(isolated)) => root = isolated,
        Ok(None) => {}
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
    if cfg!(feature = "service-probe") {
        eprintln!(
            "This native diagnostic build cannot run the ACA interface. Use a normal managed-reader package."
        );
        std::process::exit(2);
    }
    #[cfg(unix)]
    {
        unsafe extern "C" {
            fn geteuid() -> u32;
        }
        if unsafe { geteuid() } == 0 {
            eprintln!(
                "ACA must run as your ordinary user. Only its internal reader may run as root."
            );
            std::process::exit(2);
        }
    }
    #[cfg(target_os = "macos")]
    if let Err(error) = ready_for_air::hardware::initialize_hid_on_main_thread() {
        eprintln!("HID initialization failed: {error}");
        std::process::exit(1);
    }
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            gpui_kit::component::Theme::change(gpui_kit::component::ThemeMode::Dark, None, cx);
            ui::install_theme(cx);
            cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
            cx.on_action(|_: &Quit, cx| cx.quit());
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            let bounds = Bounds::centered(None, size(px(1320.), px(860.)), cx);
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(1000.), px(720.))),
                    // The header is the titlebar: native traffic lights over
                    // app content, with dragging handled by the header itself.
                    titlebar: Some(TitlebarOptions {
                        title: Some(
                            if cfg!(feature = "managed-reader") {
                                "ACA"
                            } else {
                                "ACA Studio"
                            }
                            .into(),
                        ),
                        appears_transparent: true,
                        traffic_light_position: Some(ui::TRAFFIC_LIGHTS),
                    }),
                    app_owns_titlebar_drag: true,
                    ..Default::default()
                },
                cx,
                move |window, cx| {
                    window.on_window_should_close(cx, |_, cx| {
                        cx.quit();
                        true
                    });
                    cx.new(|cx| ui::StudioApp::new(root, cx))
                },
            )
            .expect("Open Studio window");
            cx.activate(true);
        });
}
