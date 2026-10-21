#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    cos_desktop::builder(tauri::Builder::default(), None)
        .run(tauri::generate_context!())
        .expect("run the Chief of Staff window");
}
