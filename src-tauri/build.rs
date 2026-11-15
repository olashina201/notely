fn main() {
    println!("cargo:rerun-if-changed=src/macos_ext.m");
    println!("cargo:rerun-if-changed=src/macos_capture.m");
    #[cfg(target_os = "macos")]
    {
        cc::Build::new()
            .file("src/macos_ext.m")
            .file("src/macos_capture.m")
            .flag("-fobjc-arc")
            .compile("macos_ext");
        println!("cargo:rustc-link-lib=framework=Cocoa");
        println!("cargo:rustc-link-lib=framework=CoreGraphics");
        println!("cargo:rustc-link-lib=framework=ScreenCaptureKit");
    }
    tauri_build::build();
}
