fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && args[0] == "--build-info" {
        println!("{}", ready_for_air::service::Handshake::current().build);
        return;
    }
    if args.len() == 1 && args[0] == "--service" {
        if let Err(error) = ready_for_air::service::run() {
            eprintln!("ACA service: {error}");
            std::process::exit(1);
        }
        return;
    }
    if cfg!(feature = "managed-reader") || args.len() != 2 || args[0] != "--socket" {
        eprintln!("Usage: ready-air-capture --socket <Studio capture.sock>");
        std::process::exit(2);
    }
    #[cfg(all(target_os = "macos", not(feature = "managed-reader")))]
    if let Err(error) = ready_for_air::capture::serve(std::path::Path::new(&args[1])) {
        eprintln!("Capture stopped: {error}");
        std::process::exit(1);
    }
}
