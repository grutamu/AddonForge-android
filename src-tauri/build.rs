fn main() {
    // Refresh the embedded window icon when only the artwork changes.
    println!("cargo:rerun-if-changed=icons/icon.png");
    tauri_build::build()
}
