//! Notificaciones de escritorio.
//!
//! Se delega en `notify-send` (libnotify), que habla el protocolo
//! `org.freedesktop.Notifications` a través de D-Bus. Esto funciona en
//! cualquier escritorio Linux con un demonio de notificaciones y evita añadir
//! dependencias. La llamada se lanza en un hilo aparte para no bloquear la
//! interfaz y los fallos se registran sin interrumpir la aplicación.

use std::process::Command;

/// Envía una notificación de escritorio. No bloquea y nunca falla de forma
/// visible para el usuario.
pub fn send(summary: &str, body: &str, critical: bool) {
    let summary = summary.to_string();
    let body = body.to_string();

    std::thread::spawn(move || {
        let mut command = Command::new("notify-send");
        command.args(["--app-name", "Pomodoro", "--icon", "io.github.pomodoro.PomodoroTimer"]);
        command.arg(if critical {
            "--urgency=critical"
        } else {
            "--urgency=normal"
        });
        command.arg(&summary).arg(&body);

        match command.status() {
            Ok(status) if status.success() => {}
            Ok(status) => eprintln!("pomodoro-timer: notify-send terminó con {status}"),
            Err(err) => eprintln!(
                "pomodoro-timer: no se pudo usar notify-send ({err}); ¿libnotify instalado?"
            ),
        }
    });
}
