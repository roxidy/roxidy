fn main() {
    // Only run tauri build when the tauri feature is enabled
    #[cfg(feature = "tauri")]
    tauri_build::build();
}
