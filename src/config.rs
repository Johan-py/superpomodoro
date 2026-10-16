//! Ajustes de la aplicación y su persistencia.
//!
//! Se guardan en `$XDG_CONFIG_HOME/pomodoro-timer/settings.toml`
//! (o `~/.config/pomodoro-timer/settings.toml`), respetando la
//! [XDG Base Directory Specification](https://specifications.freedesktop.org/basedir-spec/latest/).
//! Nunca se escribe en rutas del sistema que requieran privilegios.
//!
//! La carga es tolerante a fallos: un fichero ausente, corrupto o con valores
//! fuera de rango **no impide el arranque**; se aplican valores por defecto.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::timer::Mode;

/// Límites de los valores configurables (en minutos).
pub const MIN_MINUTES: u32 = 1;
pub const MAX_MINUTES: u32 = 240;
/// Color de acento por defecto (rojo coral, sobrio y legible).
pub const DEFAULT_ACCENT: &str = "#e06c75";

/// Preferencias persistidas del usuario.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub work_minutes: u32,
    pub short_break_minutes: u32,
    pub long_break_minutes: u32,
    /// Reproducir un bip breve al iniciar o reanudar.
    pub start_sound: bool,
    /// Reproducir un tono al finalizar.
    pub finish_sound: bool,
    /// Ruta del tono personalizado de finalización (`None` = predeterminado).
    pub finish_sound_file: Option<PathBuf>,
    /// Iniciar automáticamente el periodo siguiente (desactivado por defecto).
    pub auto_start_next: bool,
    /// Al cerrar la ventana, ocultarla y seguir en segundo plano.
    pub run_in_background_on_close: bool,
    /// Color de acento en formato `#rrggbb`.
    pub accent: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            work_minutes: 25,
            short_break_minutes: 5,
            long_break_minutes: 15,
            start_sound: true,
            finish_sound: true,
            finish_sound_file: None,
            auto_start_next: false,
            run_in_background_on_close: false,
            accent: DEFAULT_ACCENT.to_string(),
        }
    }
}

impl Settings {
    /// Corrige valores fuera de rango o inválidos in situ.
    pub fn sanitize(&mut self) {
        self.work_minutes = self.work_minutes.clamp(MIN_MINUTES, MAX_MINUTES);
        self.short_break_minutes = self.short_break_minutes.clamp(MIN_MINUTES, MAX_MINUTES);
        self.long_break_minutes = self.long_break_minutes.clamp(MIN_MINUTES, MAX_MINUTES);

        // Una ruta vacía equivale a "predeterminado".
        if let Some(path) = &self.finish_sound_file {
            if path.as_os_str().is_empty() {
                self.finish_sound_file = None;
            }
        }

        if !is_valid_hex_color(&self.accent) {
            self.accent = DEFAULT_ACCENT.to_string();
        }
    }

    /// Duración efectiva para un modo.
    pub fn duration_for(&self, mode: Mode) -> std::time::Duration {
        let minutes = match mode {
            Mode::Work => self.work_minutes,
            Mode::ShortBreak => self.short_break_minutes,
            Mode::LongBreak => self.long_break_minutes,
        };
        std::time::Duration::from_secs(minutes.clamp(MIN_MINUTES, MAX_MINUTES) as u64 * 60)
    }
}

/// `true` si `value` es un color `#rrggbb` válido.
pub fn is_valid_hex_color(value: &str) -> bool {
    let hex = value.strip_prefix('#').unwrap_or(value);
    hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit())
}

/// Directorio base de configuración según XDG.
fn config_dir() -> PathBuf {
    if let Some(dir) = non_empty_env("XDG_CONFIG_HOME") {
        return PathBuf::from(dir);
    }
    let home = non_empty_env("HOME").unwrap_or_else(|| ".".to_string());
    PathBuf::from(home).join(".config")
}

/// Ruta del fichero de ajustes.
pub fn config_path() -> PathBuf {
    config_dir().join("pomodoro-timer").join("settings.toml")
}

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

/// Carga los ajustes desde la ubicación estándar (nunca falla).
pub fn load() -> Settings {
    load_from(&config_path())
}

/// Carga los ajustes desde `path`. Tolerante a ausencia o corrupción.
pub fn load_from(path: &Path) -> Settings {
    match fs::read_to_string(path) {
        Ok(raw) => match toml::from_str::<Settings>(&raw) {
            Ok(mut settings) => {
                settings.sanitize();
                settings
            }
            Err(err) => {
                // No se pierde la configuración del usuario: se conserva aparte.
                eprintln!("pomodoro-timer: ajustes corruptos ({err}); usando valores por defecto");
                let backup = path.with_extension("toml.corrupt");
                let _ = fs::rename(path, backup);
                Settings::default()
            }
        },
        Err(_) => Settings::default(),
    }
}

/// Guarda los ajustes en la ubicación estándar.
pub fn save(settings: &Settings) -> io::Result<()> {
    save_to(settings, &config_path())
}

/// Guarda los ajustes en `path` de forma atómica (fichero temporal + rename).
pub fn save_to(settings: &Settings, path: &Path) -> io::Result<()> {
    let mut settings = settings.clone();
    settings.sanitize();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let serialized = toml::to_string_pretty(&settings)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, serialized)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Directorio temporal único por prueba, sin dependencias externas.
    fn temp_path(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "pomodoro-timer-test-{}-{}-{}",
            std::process::id(),
            tag,
            nanos
        ));
        fs::create_dir_all(&dir).unwrap();
        dir.join("settings.toml")
    }

    #[test]
    fn defaults_match_the_spec() {
        let s = Settings::default();
        assert_eq!(s.work_minutes, 25);
        assert_eq!(s.short_break_minutes, 5);
        assert_eq!(s.long_break_minutes, 15);
        assert!(s.start_sound);
        assert!(s.finish_sound);
        assert!(!s.auto_start_next);
        assert!(!s.run_in_background_on_close);
        assert_eq!(s.finish_sound_file, None);
    }

    #[test]
    fn save_then_load_round_trips() {
        let path = temp_path("roundtrip");
        let original = Settings {
            work_minutes: 50,
            short_break_minutes: 8,
            long_break_minutes: 30,
            start_sound: false,
            auto_start_next: true,
            accent: "#7dcfff".to_string(),
            finish_sound_file: Some(PathBuf::from("/home/user/tono.ogg")),
            ..Settings::default()
        };

        save_to(&original, &path).unwrap();
        let loaded = load_from(&path);
        assert_eq!(loaded, original);

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn missing_file_yields_defaults() {
        let path = temp_path("missing");
        // No se crea el fichero.
        let loaded = load_from(&path);
        assert_eq!(loaded, Settings::default());
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn corrupt_file_yields_defaults_without_panicking() {
        let path = temp_path("corrupt");
        fs::write(&path, "esto no es TOML = = = [").unwrap();
        let loaded = load_from(&path);
        assert_eq!(loaded, Settings::default());
        // El original se conserva para no perder datos.
        assert!(path.with_extension("toml.corrupt").exists());
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn out_of_range_values_are_clamped_on_load() {
        let path = temp_path("clamp");
        let raw = "work_minutes = 0\nshort_break_minutes = 9999\nlong_break_minutes = 15\naccent = \"no-es-color\"\n";
        fs::write(&path, raw).unwrap();
        let loaded = load_from(&path);
        assert_eq!(loaded.work_minutes, MIN_MINUTES);
        assert_eq!(loaded.short_break_minutes, MAX_MINUTES);
        assert_eq!(loaded.accent, DEFAULT_ACCENT);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn partial_file_uses_defaults_for_missing_fields() {
        let path = temp_path("partial");
        fs::write(&path, "work_minutes = 40\n").unwrap();
        let loaded = load_from(&path);
        assert_eq!(loaded.work_minutes, 40);
        assert_eq!(loaded.short_break_minutes, 5); // valor por defecto
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn empty_finish_sound_file_becomes_none() {
        let mut s = Settings {
            finish_sound_file: Some(PathBuf::from("")),
            ..Settings::default()
        };
        s.sanitize();
        assert_eq!(s.finish_sound_file, None);
    }

    #[test]
    fn duration_for_maps_each_mode() {
        let s = Settings::default();
        assert_eq!(s.duration_for(Mode::Work).as_secs(), 25 * 60);
        assert_eq!(s.duration_for(Mode::ShortBreak).as_secs(), 5 * 60);
        assert_eq!(s.duration_for(Mode::LongBreak).as_secs(), 15 * 60);
    }

    #[test]
    fn hex_color_validation() {
        assert!(is_valid_hex_color("#e06c75"));
        assert!(is_valid_hex_color("e06c75"));
        assert!(!is_valid_hex_color("#e06c7"));
        assert!(!is_valid_hex_color("#gggggg"));
        assert!(!is_valid_hex_color(""));
    }

    #[test]
    fn accent_is_used_to_build_accent_css() {
        // Garantiza que el acento elegido es un hex válido que podemos inyectar en CSS.
        let s = Settings::default();
        assert!(is_valid_hex_color(&s.accent));
    }
}
