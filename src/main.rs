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
            "--dispatch" => {
                eprintln!("The Karabiner bridge has been removed. Use Studio's native mappings.");
                std::process::exit(2);
            }
            "--verify-capture" => {
                let result = std::env::current_exe()
                    .map_err(|e| e.to_string())
                    .and_then(|exe| ready_for_air::session::packaged_helper(&exe))
                    .and_then(|helper| ready_for_air::session::verify_packaged_helper(&helper));
                match result {
                    Ok(()) => println!("Capture matches this Studio build"),
                    Err(e) => {
                        eprintln!("{e}");
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
                match ready_for_air::audio::inputs() {
                    Ok(lines) => {
                        for line in lines {
                            println!("Audio: {line}");
                        }
                    }
                    Err(e) => eprintln!("Audio: {e}"),
                }
                return;
            }
            _ => {
                eprintln!("Uso: ready-for-air [--data-root PATH] [--inspect] [--verify-capture]");
                std::process::exit(2);
            }
        }
    }
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            gpui_kit::component::Theme::change(gpui_kit::component::ThemeMode::Dark, None, cx);
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
                    titlebar: Some(TitlebarOptions {
                        title: Some("ACA Studio".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                cx,
                move |_, cx| cx.new(|cx| ui::StudioApp::new(root, cx)),
            )
            .expect("Open Studio window");
            cx.activate(true);
        });
}
