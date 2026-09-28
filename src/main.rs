#[cfg(not(target_os = "macos"))]
compile_error!("Keychron Battery uses macOS AppKit and IOKit; build it on a Mac.");

mod app;
mod hid;
mod model;

fn main() {
    match std::env::args().nth(1).as_deref() {
        None => app::run(),
        Some("--diagnose") => {
            let snapshot = std::thread::spawn(|| hid::probe_snapshot("Keychron"))
                .join()
                .expect("battery reader stopped unexpectedly");
            println!("{}", serde_json::to_string_pretty(&snapshot).unwrap());
        }
        Some("--cached") => {
            let snapshot = hid::read_snapshot("Keychron");
            println!("{}", serde_json::to_string_pretty(&snapshot).unwrap());
        }
        Some("--help" | "-h") => println!(
            "Keychron Battery\n\n  keychron-battery             Open the menu bar app\n  keychron-battery --diagnose  Request a fresh battery report; print JSON\n  keychron-battery --cached    Print cached HID data without a device request\n\nUse the bundled .app for a stable macOS permission identity."
        ),
        Some(arg) => {
            eprintln!("Unknown argument: {arg}. Use --help.");
            std::process::exit(2);
        }
    }
}
