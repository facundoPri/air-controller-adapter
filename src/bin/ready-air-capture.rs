fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 || args[0] != "--socket" {
        eprintln!("Usage: ready-air-capture --socket <Studio capture.sock>");
        std::process::exit(2);
    }
    #[cfg(target_os = "macos")]
    if let Err(error) = ready_for_air::capture::serve(std::path::Path::new(&args[1])) {
        eprintln!("Capture stopped: {error}");
        std::process::exit(1);
    }
}
