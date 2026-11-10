fn main() {
    #[cfg(target_os = "macos")]
    {
        cc::Build::new()
            .file("src/macos_ext.m")
            .flag("-fobjc-arc")
            .compile("macos_ext");
        println!("cargo:rustc-link-lib=framework=Cocoa");
    }
    tauri_build::build();
}
