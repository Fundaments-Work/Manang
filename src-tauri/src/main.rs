// Windows release builds must not open a console window alongside the app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // AGENTS.md: no `unwrap`/`expect` in production paths, so startup failure is
    // reported explicitly rather than panicking inside the Tauri runtime.
    if let Err(error) = manang_lib::run() {
        eprintln!("manang: failed to start: {error}");
        std::process::exit(1);
    }
}
