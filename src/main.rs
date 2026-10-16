//! Pomodoro Timer — reloj Pomodoro minimalista y ligero para Linux.
//!
//! Punto de entrada: configura la `GtkApplication`, aplica el tema y construye
//! la interfaz. La lógica del temporizador, los ajustes y el audio viven en
//! módulos independientes.

mod audio;
mod config;
mod notifications;
mod theme;
mod timer;
mod ui;

use gtk4 as gtk;

use gtk::prelude::*;
use gtk::{glib, Application};

/// Identificador de la aplicación (también usado por el escritorio).
const APP_ID: &str = "io.github.pomodoro.PomodoroTimer";

fn main() -> glib::ExitCode {
    // Diagnóstico de audio: `pomodoro-timer --audio-test [archivo]` reproduce el
    // bip de inicio y el tono de finalización (o el archivo indicado) y muestra
    // cualquier error. Útil para comprobar el sonido en Arch/Wayland.
    let args: Vec<String> = std::env::args().collect();
    if let Some(index) = args.iter().position(|a| a == "--audio-test") {
        let file = args.get(index + 1).map(std::path::PathBuf::from);
        return audio_self_test(file);
    }

    let app = Application::builder().application_id(APP_ID).build();

    // Se prefiere el tema oscuro por coherencia con la paleta de la interfaz.
    app.connect_startup(|_| {
        ui::prefer_dark_theme();
    });

    app.connect_activate(ui::build);

    app.run()
}

/// Reproduce los sonidos de la aplicación y devuelve un código de salida.
fn audio_self_test(file: Option<std::path::PathBuf>) -> glib::ExitCode {
    let engine = audio::AudioEngine::new();
    std::thread::sleep(std::time::Duration::from_millis(300));

    println!("Reproduciendo bip de inicio…");
    engine.beep();
    std::thread::sleep(std::time::Duration::from_millis(600));

    match &file {
        Some(path) => println!("Reproduciendo tono de finalización: {}", path.display()),
        None => println!("Reproduciendo carillón predeterminado…"),
    }
    engine.finish(file);
    std::thread::sleep(std::time::Duration::from_millis(1500));

    match engine.take_error() {
        Some(error) => {
            eprintln!("Error de audio: {error}");
            glib::ExitCode::FAILURE
        }
        None => {
            println!("Audio reproducido sin errores.");
            glib::ExitCode::SUCCESS
        }
    }
}
