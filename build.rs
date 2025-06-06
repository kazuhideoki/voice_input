fn main() {
    // Link required macOS frameworks for Accessibility API
    if cfg!(target_os = "macos") {
        println!("cargo:rustc-link-lib=framework=ApplicationServices");
    }
}
