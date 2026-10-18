// Windows release builds must not open a console window alongside the app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let (wayland, nvidia, already_set) = manang_lib::webkit_renderer_situation();
    if let Some((key, value)) = manang_lib::webkit_renderer_override(wayland, nvidia, already_set) {
        // SAFETY: the first statement of `main`, before Tauri, GTK or any other
        // thread exists, so nothing can read the environment concurrently.
        // WebKitGTK reads this variable once, when it initialises.
        unsafe { std::env::set_var(key, value) };
        manang_lib::note_renderer_override();
    }
    // AGENTS.md: no `unwrap`/`expect` in production paths, so startup failure is
    // reported explicitly rather than panicking inside the Tauri runtime.
    if let Err(error) = manang_lib::run() {
        eprintln!("manang: failed to start: {error}");
        std::process::exit(1);
    }
}
