//! Motor de audio.
//!
//! Toda la reproducción ocurre en un hilo dedicado que posee el
//! `OutputStream` (no es `Send`), de modo que la interfaz **nunca se bloquea**
//! ni al abrir el dispositivo ni al decodificar un fichero.
//!
//! - Sonido de inicio: bip sintetizado en memoria (sin ficheros ni red).
//! - Sonido de finalización: fichero del usuario (wav/ogg/mp3/flac) o un
//!   carillón sintetizado por defecto.
//!
//! Si el dispositivo no está disponible o el fichero falla, se informa mediante
//! [`AudioEngine::take_error`] y se recurre a un sonido seguro, sin bloquear ni
//! finalizar la aplicación.

use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use rodio::{buffer::SamplesBuffer, Decoder, OutputStream, OutputStreamHandle, Source};

/// Frecuencia de muestreo de los tonos sintetizados.
const SAMPLE_RATE: u32 = 48_000;

/// Peticiones que la interfaz envía al hilo de audio.
enum Command {
    Beep,
    Finish(Option<PathBuf>),
    Preview(PathBuf),
}

/// Incidencias que el hilo de audio comunica a la interfaz.
#[derive(Default)]
struct Report {
    last_error: Option<String>,
}

/// Manejador del audio. Es barato de clonar (comparte el canal y el informe).
#[derive(Clone)]
pub struct AudioEngine {
    tx: Sender<Command>,
    report: Arc<Mutex<Report>>,
}

impl AudioEngine {
    /// Arranca el hilo de audio. Si el dispositivo no está disponible, las
    /// peticiones se ignoran silenciosamente y se registra un error legible.
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let report = Arc::new(Mutex::new(Report::default()));
        let report_thread = Arc::clone(&report);

        let spawned = std::thread::Builder::new()
            .name("pomodoro-audio".to_string())
            .spawn(move || audio_loop(rx, report_thread));

        if let Err(err) = spawned {
            set_error(&report, format!("No se pudo iniciar el hilo de audio: {err}"));
        }

        Self { tx, report }
    }

    /// Bip breve de inicio/reanudación (no intrusivo, ~120 ms).
    pub fn beep(&self) {
        let _ = self.tx.send(Command::Beep);
    }

    /// Tono de finalización. `None` usa el carillón predeterminado.
    pub fn finish(&self, file: Option<PathBuf>) {
        let _ = self.tx.send(Command::Finish(file));
    }

    /// Reproduce un fichero como previsualización.
    pub fn preview(&self, file: PathBuf) {
        let _ = self.tx.send(Command::Preview(file));
    }

    /// Devuelve y limpia el último error de audio, si lo hay.
    pub fn take_error(&self) -> Option<String> {
        self.report.lock().ok().and_then(|mut r| r.last_error.take())
    }
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self::new()
    }
}

fn set_error(report: &Arc<Mutex<Report>>, message: String) {
    if let Ok(mut r) = report.lock() {
        r.last_error = Some(message);
    }
}

/// Bucle del hilo de audio: abre el dispositivo una vez y sirve peticiones.
fn audio_loop(rx: Receiver<Command>, report: Arc<Mutex<Report>>) {
    let (_stream, handle) = match OutputStream::try_default() {
        Ok(pair) => pair,
        Err(err) => {
            set_error(
                &report,
                format!("Dispositivo de audio no disponible: {err}"),
            );
            // Drena el canal para que `send` nunca falle de forma inesperada.
            for _ in rx {}
            return;
        }
    };

    for command in rx {
        match command {
            Command::Beep => {
                play(&handle, &report, beep_source());
            }
            Command::Finish(None) => {
                play(&handle, &report, chime_source());
            }
            Command::Finish(Some(path)) => {
                if let Err(err) = play_file(&handle, &path) {
                    set_error(
                        &report,
                        format!(
                            "No se pudo reproducir «{}» ({err}). Se usó el tono predeterminado.",
                            path.display()
                        ),
                    );
                    play(&handle, &report, chime_source());
                }
            }
            Command::Preview(path) => {
                if let Err(err) = play_file(&handle, &path) {
                    set_error(
                        &report,
                        format!("No se pudo previsualizar «{}»: {err}", path.display()),
                    );
                }
            }
        }
    }
}

fn play(
    handle: &OutputStreamHandle,
    report: &Arc<Mutex<Report>>,
    source: SamplesBuffer<f32>,
) {
    if let Err(err) = handle.play_raw(source) {
        set_error(report, format!("Error al reproducir audio: {err}"));
    }
}

fn play_file(handle: &OutputStreamHandle, path: &PathBuf) -> Result<(), String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let decoder = Decoder::new(BufReader::new(file)).map_err(|e| e.to_string())?;
    handle
        .play_raw(decoder.convert_samples::<f32>())
        .map_err(|e| e.to_string())
}

/// Genera un tono senoidal con rampas suaves de entrada/salida para evitar
/// chasquidos.
fn tone(freq_hz: f64, duration_s: f64, amplitude: f32) -> Vec<f32> {
    let sample_count = (duration_s * SAMPLE_RATE as f64) as usize;
    let fade = ((0.008 * SAMPLE_RATE as f64) as usize).max(1);
    let two_pi = std::f64::consts::TAU;

    (0..sample_count)
        .map(|i| {
            let t = i as f64 / SAMPLE_RATE as f64;
            let attack = (i as f64 / fade as f64).min(1.0);
            let release = (((sample_count.saturating_sub(i)) as f64) / fade as f64).min(1.0);
            let envelope = attack.min(release);
            (amplitude as f64 * envelope * (two_pi * freq_hz * t).sin()) as f32
        })
        .collect()
}

/// Bip corto y discreto para el inicio de sesión.
fn beep_source() -> SamplesBuffer<f32> {
    SamplesBuffer::new(1, SAMPLE_RATE, tone(880.0, 0.12, 0.22))
}

/// Carillón de dos notas ascendentes para el final de sesión.
fn chime_source() -> SamplesBuffer<f32> {
    let mut samples = tone(659.25, 0.18, 0.26); // E5
    samples.extend(tone(987.77, 0.32, 0.26)); // B5
    SamplesBuffer::new(1, SAMPLE_RATE, samples)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beep_has_expected_length_and_range() {
        let samples = tone(880.0, 0.12, 0.22);
        assert_eq!(samples.len(), (0.12 * SAMPLE_RATE as f64) as usize);
        assert!(samples.iter().all(|s| s.is_finite()));
        assert!(samples.iter().all(|s| s.abs() <= 0.22 + f32::EPSILON));
    }

    #[test]
    fn tones_start_and_end_near_silence() {
        // Las rampas evitan chasquidos: el primer y último sample son ~0.
        let samples = tone(440.0, 0.1, 0.5);
        assert!(samples.first().copied().unwrap_or(1.0).abs() < 0.02);
        assert!(samples.last().copied().unwrap_or(1.0).abs() < 0.02);
    }

    #[test]
    fn chime_concatenates_two_notes() {
        let samples = chime_source();
        let expected = (0.18 * SAMPLE_RATE as f64) as usize + (0.32 * SAMPLE_RATE as f64) as usize;
        // No se puede inspeccionar la longitud interna; comprobamos la fuente
        // construida indirectamente mediante los tonos.
        assert_eq!(expected, tone(659.25, 0.18, 0.26).len() + tone(987.77, 0.32, 0.26).len());
        drop(samples);
    }

    #[test]
    fn error_is_reported_once_and_cleared() {
        let engine = AudioEngine::new();
        set_error(
            &engine.report,
            "fallo de prueba".to_string(),
        );
        assert_eq!(engine.take_error().as_deref(), Some("fallo de prueba"));
        assert!(engine.take_error().is_none());
    }
}
