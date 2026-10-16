//! Lógica pura del temporizador.
//!
//! No depende de GTK ni de E/S, por lo que es totalmente comprobable con
//! pruebas unitarias. El tiempo restante se calcula siempre a partir de un
//! `Instant` monotónico ([`std::time::Instant`]), nunca decrementando un
//! contador cada segundo: así los retrasos del planificador o el hecho de que
//! la ventana pierda el foco no introducen desviaciones.

use std::time::{Duration, Instant};

/// Duración mínima aceptada para una sesión (evita bucles de "0 segundos").
pub const MIN_DURATION: Duration = Duration::from_secs(1);

/// Los tres tipos de sesión del Pomodoro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Work,
    ShortBreak,
    LongBreak,
}

impl Mode {
    /// Orden de presentación en la interfaz.
    pub const ALL: [Mode; 3] = [Mode::Work, Mode::ShortBreak, Mode::LongBreak];

    pub fn label(self) -> &'static str {
        match self {
            Mode::Work => "Trabajo",
            Mode::ShortBreak => "Descanso corto",
            Mode::LongBreak => "Descanso largo",
        }
    }

    /// Etiqueta compacta para los botones de modo.
    pub fn short_label(self) -> &'static str {
        match self {
            Mode::Work => "Trabajo",
            Mode::ShortBreak => "Corto",
            Mode::LongBreak => "Largo",
        }
    }
}

/// Estado explícito del temporizador.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Listo para empezar, sin tiempo consumido.
    Idle,
    /// Contando.
    Running,
    /// Iniciado y detenido temporalmente; conserva el tiempo restante.
    Paused,
    /// La cuenta llegó a cero y todavía no se ha iniciado la siguiente sesión.
    Finished,
}

/// Máquina de estados del temporizador. Todo método que consulta o avanza el
/// tiempo recibe el "ahora" como `Instant` para poder inyectarlo en las pruebas.
#[derive(Debug, Clone)]
pub struct Timer {
    mode: Mode,
    status: Status,
    /// Duración completa de la sesión actual.
    total: Duration,
    /// Tiempo restante cuando no está corriendo (Idle/Paused/Finished).
    frozen_remaining: Duration,
    /// Instante límite mientras corre.
    deadline: Option<Instant>,
}

impl Timer {
    /// Crea un temporizador detenido para `mode` con la duración indicada.
    pub fn new(mode: Mode, total: Duration) -> Self {
        let total = total.max(MIN_DURATION);
        Self {
            mode,
            status: Status::Idle,
            total,
            frozen_remaining: total,
            deadline: None,
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn status(&self) -> Status {
        self.status
    }

    #[allow(dead_code)] // parte de la API del núcleo; usada por las pruebas
    pub fn total(&self) -> Duration {
        self.total
    }

    pub fn is_running(&self) -> bool {
        self.status == Status::Running
    }

    /// Cambia de modo con su duración y reinicia el temporizador.
    pub fn switch_mode(&mut self, mode: Mode, total: Duration) {
        self.mode = mode;
        self.total = total.max(MIN_DURATION);
        self.reset();
    }

    /// Tiempo restante según el reloj monotónico.
    pub fn remaining(&self, now: Instant) -> Duration {
        match self.deadline {
            Some(deadline) => deadline.saturating_duration_since(now),
            None => self.frozen_remaining,
        }
    }

    /// Fracción transcurrida en el rango `0.0..=1.0`, útil para el anillo.
    pub fn progress(&self, now: Instant) -> f64 {
        let total = self.total.as_secs_f64();
        if total <= 0.0 {
            return 1.0;
        }
        let remaining = self.remaining(now).as_secs_f64();
        (1.0 - remaining / total).clamp(0.0, 1.0)
    }

    /// Inicia o reanuda. Si la sesión ya terminó, la reinicia desde el principio.
    pub fn start(&mut self, now: Instant) {
        match self.status {
            Status::Running => {}
            Status::Finished => {
                self.reset();
                self.start(now);
            }
            Status::Idle | Status::Paused => {
                let remaining = self.frozen_remaining.max(MIN_DURATION);
                self.deadline = Some(now + remaining);
                self.status = Status::Running;
            }
        }
    }

    /// Pausa conservando el tiempo restante exacto.
    pub fn pause(&mut self, now: Instant) {
        if self.status == Status::Running {
            self.frozen_remaining = self.remaining(now);
            self.deadline = None;
            self.status = Status::Paused;
        }
    }

    /// Inicia, reanuda o pausa según el estado actual.
    #[allow(dead_code)] // parte de la API del núcleo; usada por las pruebas
    pub fn toggle(&mut self, now: Instant) {
        match self.status {
            Status::Running => self.pause(now),
            _ => self.start(now),
        }
    }

    /// Vuelve al estado inicial con la duración completa.
    pub fn reset(&mut self) {
        self.status = Status::Idle;
        self.deadline = None;
        self.frozen_remaining = self.total;
    }

    /// Avanza el estado hasta `now`.
    ///
    /// Devuelve `true` exactamente una vez, cuando la sesión alcanza el cero.
    /// Es idempotente: llamar de nuevo tras la finalización devuelve `false`.
    pub fn tick(&mut self, now: Instant) -> bool {
        if self.status == Status::Running {
            if let Some(deadline) = self.deadline {
                if now >= deadline {
                    self.status = Status::Finished;
                    self.deadline = None;
                    self.frozen_remaining = Duration::ZERO;
                    return true;
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: Duration = Duration::from_secs(0);
    const WORK: Duration = Duration::from_secs(25 * 60);

    fn timer() -> (Timer, Instant) {
        let base = Instant::now();
        (Timer::new(Mode::Work, WORK), base + T0)
    }

    #[test]
    fn starts_idle_with_full_duration() {
        let (t, now) = timer();
        assert_eq!(t.status(), Status::Idle);
        assert_eq!(t.remaining(now), WORK);
        assert_eq!(t.progress(now), 0.0);
    }

    #[test]
    fn start_then_remaining_decreases_monotonically() {
        let (mut t, base) = timer();
        t.start(base);
        assert!(t.is_running());
        assert_eq!(t.remaining(base), WORK);
        assert_eq!(t.remaining(base + Duration::from_secs(60)), WORK - Duration::from_secs(60));
        let expected = 60.0 / (25.0 * 60.0);
        assert!((t.progress(base + Duration::from_secs(60)) - expected).abs() < 1e-9);
    }

    #[test]
    fn pause_and_resume_do_not_introduce_drift() {
        let (mut t, base) = timer();
        t.start(base);

        // Transcurren 10 min y pausamos.
        let paused_at = base + Duration::from_secs(600);
        t.pause(paused_at);
        assert_eq!(t.status(), Status::Paused);
        assert_eq!(t.remaining(paused_at), WORK - Duration::from_secs(600));

        // El tiempo "pasa" mientras está en pausa (p. ej. 5 min); no debe consumirse.
        let much_later = paused_at + Duration::from_secs(300);
        assert_eq!(t.remaining(much_later), WORK - Duration::from_secs(600));

        // Reanudamos y comprobamos que sigue descontando desde donde quedó.
        t.start(much_later);
        assert_eq!(t.remaining(much_later), WORK - Duration::from_secs(600));
        assert_eq!(
            t.remaining(much_later + Duration::from_secs(60)),
            WORK - Duration::from_secs(660)
        );
    }

    #[test]
    fn tick_finishes_exactly_at_deadline_and_only_once() {
        let (mut t, base) = timer();
        t.start(base);
        let deadline = base + WORK;

        assert!(!t.tick(deadline - Duration::from_millis(1)));
        assert!(t.tick(deadline));
        assert_eq!(t.status(), Status::Finished);
        assert_eq!(t.remaining(deadline), Duration::ZERO);
        // Idempotente.
        assert!(!t.tick(deadline + Duration::from_secs(5)));
    }

    #[test]
    fn tick_with_large_gap_still_finishes() {
        // Simula que el proceso estuvo suspendido mucho más que la sesión.
        let (mut t, base) = timer();
        t.start(base);
        assert!(t.tick(base + Duration::from_secs(10_000)));
        assert_eq!(t.status(), Status::Finished);
    }

    #[test]
    fn reset_restores_full_duration() {
        let (mut t, base) = timer();
        t.start(base);
        let _ = t.remaining(base + Duration::from_secs(900));
        t.reset();
        assert_eq!(t.status(), Status::Idle);
        assert_eq!(t.remaining(base), WORK);
    }

    #[test]
    fn starting_a_finished_session_restarts_it() {
        let (mut t, base) = timer();
        t.start(base);
        assert!(t.tick(base + WORK));
        t.start(base + WORK);
        assert_eq!(t.status(), Status::Running);
        assert_eq!(t.remaining(base + WORK), WORK);
    }

    #[test]
    fn toggle_cycles_running_and_paused() {
        let (mut t, base) = timer();
        t.toggle(base);
        assert_eq!(t.status(), Status::Running);
        t.toggle(base + Duration::from_secs(10));
        assert_eq!(t.status(), Status::Paused);
        t.toggle(base + Duration::from_secs(10));
        assert_eq!(t.status(), Status::Running);
    }

    #[test]
    fn switch_mode_resets_and_changes_duration() {
        let (mut t, _base) = timer();
        let short = Duration::from_secs(5 * 60);
        t.switch_mode(Mode::ShortBreak, short);
        assert_eq!(t.mode(), Mode::ShortBreak);
        assert_eq!(t.total(), short);
        assert_eq!(t.status(), Status::Idle);
    }

    #[test]
    fn durations_below_one_second_are_clamped() {
        let (t, now) = (Timer::new(Mode::Work, Duration::from_millis(10)), Instant::now());
        assert_eq!(t.total(), MIN_DURATION);
        assert_eq!(t.remaining(now), MIN_DURATION);
    }

    #[test]
    fn progress_is_clamped_between_zero_and_one() {
        let (mut t, base) = timer();
        t.start(base);
        assert_eq!(t.progress(base), 0.0);
        assert_eq!(t.progress(base + WORK * 2), 1.0);
    }
}
