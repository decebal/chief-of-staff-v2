#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    cos_desktop::with_menu(cos_desktop::builder(tauri::Builder::default(), None))
        .build(tauri::generate_context!())
        .expect("run the Chief of Staff window")
        .run(cos_desktop::on_run_event);
}
