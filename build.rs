fn main() {
    println!("cargo:rerun-if-changed=native/service.m");
    println!("cargo:rerun-if-changed=native/keyboard.m");
    println!("cargo:rerun-if-env-changed=ACA_BUILD_ID");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        cc::Build::new()
            .file("native/service.m")
            .file("native/keyboard.m")
            .flag("-fobjc-arc")
            .flag("-fblocks")
            .flag("-mmacosx-version-min=13.0")
            .warnings(true)
            .compile("aca_service");
        for framework in [
            "Foundation",
            "AppKit",
            "ApplicationServices",
            "ServiceManagement",
            "Security",
            "SystemConfiguration",
        ] {
            println!("cargo:rustc-link-lib=framework={framework}");
        }
    }
}
