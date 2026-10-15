// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Launched by a self-update: tidy up the old exe in the background.
    if args.first().map(|a| a == "--replaced").unwrap_or(false) {
        // Tell the app it was just updated, so the "what changed" note shows once.
        std::env::set_var("ADDONFORGE_JUST_UPDATED", "1");
        if let Some(old) = args.get(1).cloned() {
            std::thread::spawn(move || addonforge_lib::finish_replace(std::path::Path::new(&old)));
        }
    }
    addonforge_lib::run()
}
