//! Tema visual: hoja de estilos (CSS) y utilidades de color.
//!
//! La paleta es oscura y sobria con un único color de acento configurable.
//! Toda la estética es CSS puro de GTK4, sin imágenes ni animaciones costosas.

use std::cell::RefCell;

use gtk4 as gtk;

use gtk::gdk;

/// Color de acento por defecto como tupla normalizada `(r, g, b)`.
/// Equivale a [`crate::config::DEFAULT_ACCENT`] (`#e06c75`).
pub const DEFAULT_RGB: (f64, f64, f64) = (0.878, 0.424, 0.459);

/// Acentos predefinidos ofrecidos en los ajustes: `(hex, nombre)`.
pub const PRESETS: &[(&str, &str)] = &[
    ("#e06c75", "Coral"),
    ("#e5a86b", "Ámbar"),
    ("#9ece6a", "Verde"),
    ("#7dcfff", "Cian"),
    ("#bb9af7", "Violeta"),
    ("#c0caf5", "Acero"),
];

thread_local! {
    /// Proveedor de estilos único, reutilizado al cambiar de acento.
    static PROVIDER: RefCell<Option<gtk::CssProvider>> = const { RefCell::new(None) };
}

/// Convierte `#rrggbb` en `(r, g, b)` normalizado a `0.0..=1.0`.
pub fn parse_hex(value: &str) -> Option<(f64, f64, f64)> {
    let hex = value.trim().trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some((r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0))
}

/// Carga (o recarga) la hoja de estilos con el acento indicado.
pub fn load_css(accent_hex: &str) {
    let css = build_css(accent_hex);
    PROVIDER.with(|cell| {
        let mut slot = cell.borrow_mut();
        let provider = slot.get_or_insert_with(|| {
            let provider = gtk::CssProvider::new();
            if let Some(display) = gdk::Display::default() {
                gtk::style_context_add_provider_for_display(
                    &display,
                    &provider,
                    gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
                );
            }
            provider
        });
        provider.load_from_data(&css);
    });
}

/// Construye el CSS inyectando el color de acento.
pub fn build_css(accent_hex: &str) -> String {
    let accent = if crate::config::is_valid_hex_color(accent_hex) {
        accent_hex
    } else {
        crate::config::DEFAULT_ACCENT
    };

    // `@define-color` permite referenciar el acento como `@accent` en GTK4.
    format!(
        r#"
@define-color accent {accent};

window.timer-window {{
    background-color: #14161c;
}}

headerbar {{
    background-color: #14161c;
    border-bottom: 1px solid alpha(#ffffff, 0.06);
    box-shadow: none;
    min-height: 44px;
}}

.app-title {{
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 3px;
    color: alpha(#f2f4f8, 0.72);
}}

.time-label {{
    font-size: 54px;
    font-weight: 200;
    letter-spacing: 1px;
    color: #f6f7fb;
}}

.mode-label {{
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 3px;
    color: alpha(#f2f4f8, 0.5);
}}

.status-label {{
    font-size: 12px;
    color: alpha(#f2f4f8, 0.42);
}}

/* Selector segmentado de modos */
.mode-switch {{
    background-color: alpha(#ffffff, 0.05);
    border-radius: 999px;
    padding: 3px;
    border: 1px solid alpha(#ffffff, 0.05);
}}

.mode-btn {{
    background-image: none;
    background-color: transparent;
    border: none;
    box-shadow: none;
    padding: 6px 16px;
    min-height: 0;
    border-radius: 999px;
    font-size: 12px;
    font-weight: 500;
    color: alpha(#f2f4f8, 0.62);
}}

.mode-btn:hover {{
    color: #f6f7fb;
}}

.mode-btn:checked {{
    background-color: @accent;
    color: #12141a;
    font-weight: 700;
}}

/* Botones principales */
button.primary {{
    background-image: none;
    background-color: @accent;
    color: #12141a;
    border: none;
    box-shadow: none;
    border-radius: 999px;
    font-size: 14px;
    font-weight: 700;
    padding: 10px 26px;
}}

button.primary:hover {{
    background-color: shade(@accent, 1.08);
}}

button.primary:active {{
    background-color: shade(@accent, 0.92);
}}

button.ghost {{
    background-image: none;
    background-color: alpha(#ffffff, 0.06);
    color: alpha(#f2f4f8, 0.85);
    border: none;
    box-shadow: none;
    border-radius: 999px;
    min-width: 38px;
    min-height: 38px;
    padding: 0 14px;
}}

button.ghost:hover {{
    background-color: alpha(#ffffff, 0.11);
}}

button.flat-icon {{
    background-image: none;
    background-color: transparent;
    border: none;
    box-shadow: none;
    border-radius: 999px;
    min-width: 32px;
    min-height: 32px;
    color: alpha(#f2f4f8, 0.75);
}}

button.flat-icon:hover {{
    background-color: alpha(#ffffff, 0.08);
    color: #f6f7fb;
}}

/* Ajustes */
.section-title {{
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 2px;
    color: alpha(#f2f4f8, 0.38);
    margin-top: 6px;
}}

.setting-title {{
    font-size: 13px;
    color: #e9ebf2;
}}

.setting-sub {{
    font-size: 11px;
    color: alpha(#e9ebf2, 0.45);
}}

.file-label {{
    font-size: 11px;
    color: alpha(#e9ebf2, 0.55);
}}

.warn-label {{
    font-size: 11px;
    color: #e5a86b;
}}

separator.hairline {{
    background-color: alpha(#ffffff, 0.06);
    min-height: 1px;
}}

.accent-swatch {{
    min-width: 24px;
    min-height: 24px;
    padding: 0;
    border-radius: 999px;
    border: 2px solid transparent;
    box-shadow: none;
    background-image: none;
}}

.accent-swatch:checked {{
    border-color: #f6f7fb;
}}

.accent-0 {{ background-color: #e06c75; }}
.accent-1 {{ background-color: #e5a86b; }}
.accent-2 {{ background-color: #9ece6a; }}
.accent-3 {{ background-color: #7dcfff; }}
.accent-4 {{ background-color: #bb9af7; }}
.accent-5 {{ background-color: #c0caf5; }}

spinbutton {{
    border-radius: 10px;
}}

/* Los interruptores siguen el color de acento en lugar del azul por defecto. */
switch {{
    border-radius: 999px;
}}

switch:checked {{
    background-color: @accent;
}}

switch:checked > slider {{
    background-color: #ffffff;
}}
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_accepts_valid_colors() {
        assert_eq!(parse_hex("#000000"), Some((0.0, 0.0, 0.0)));
        assert_eq!(parse_hex("#ffffff"), Some((1.0, 1.0, 1.0)));
        assert!(parse_hex("#e06c75").is_some());
    }

    #[test]
    fn parse_hex_rejects_invalid_colors() {
        assert_eq!(parse_hex("nope"), None);
        assert_eq!(parse_hex("#12345"), None);
        assert_eq!(parse_hex("#zzzzzz"), None);
    }

    #[test]
    fn default_rgb_matches_default_accent_hex() {
        let (r, g, b) = parse_hex(crate::config::DEFAULT_ACCENT).unwrap();
        assert!((r - DEFAULT_RGB.0).abs() < 0.01);
        assert!((g - DEFAULT_RGB.1).abs() < 0.01);
        assert!((b - DEFAULT_RGB.2).abs() < 0.01);
    }

    #[test]
    fn css_embeds_the_accent_and_falls_back_when_invalid() {
        let css = build_css("#123456");
        assert!(css.contains("@define-color accent #123456;"));

        let fallback = build_css("not-a-color");
        assert!(fallback.contains(crate::config::DEFAULT_ACCENT));
    }

    #[test]
    fn every_preset_is_a_valid_color() {
        for (hex, _name) in PRESETS {
            assert!(crate::config::is_valid_hex_color(hex), "preset inválido: {hex}");
        }
    }
}
