//! Interfaz gráfica (GTK4).
//!
//! Estructura de la ventana:
//!
//! ```text
//! ┌ HeaderBar ───────────────────────── [⚙] ┐
//! │  ┌ Stack ─────────────────────────────┐ │
//! │  │ Vista temporizador                 │ │
//! │  │   [Trabajo][Corto][Largo]          │ │
//! │  │      ( anillo + mm:ss )            │ │
//! │  │   [ Iniciar ]  [⟳]                 │ │
//! │  │ Vista ajustes                      │ │
//! │  └────────────────────────────────────┘ │
//! └──────────────────────────────────────────┘
//! ```
//!
//! El estado (`Timer` + `Settings`) vive en un `Rc<RefCell<State>>` compartido
//! por los manejadores. Un temporizador de GTK a 250 ms refresca la vista; el
//! anillo se redibuja únicamente cuando la sesión está en marcha, por lo que el
//! consumo en reposo es prácticamente nulo.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk4 as gtk;

use gtk::gio::{self, Cancellable};
use gtk::glib;
use gtk::prelude::*;
use gtk::{
    Align, Application, ApplicationWindow, Box as GtkBox, Button, DrawingArea, FileDialog,
    FileFilter, HeaderBar, Label, Orientation, Overlay, PolicyType, ScrolledWindow, Separator,
    SpinButton, Stack, StackTransitionType, Switch, ToggleButton,
};

use crate::audio::AudioEngine;
use crate::config::{self, MAX_MINUTES, MIN_MINUTES};
use crate::notifications;
use crate::theme;
use crate::timer::{Mode, Status, Timer};

/// Intervalo de refresco de la interfaz (ms). Suficiente para que el anillo y
/// los segundos se vean fluidos con un coste de CPU mínimo.
const UI_TICK_MS: u64 = 250;

/// Estado compartido de la aplicación.
struct State {
    timer: Timer,
    settings: config::Settings,
    /// Acento actual ya convertido a componentes normalizados.
    accent: (f64, f64, f64),
    /// Pomodoros de trabajo completados (para elegir descanso corto o largo).
    completed_work: u32,
}

/// Referencias a los widgets que se actualizan dinámicamente.
struct Widgets {
    window: ApplicationWindow,
    stack: Stack,
    drawing: DrawingArea,
    time_label: Label,
    mode_label: Label,
    status_label: Label,
    primary: Button,
    reset_btn: Button,
    settings_btn: Button,
    back_btn: Button,
    mode_buttons: Vec<(Mode, ToggleButton)>,

    // --- Ajustes ---
    work_spin: SpinButton,
    short_spin: SpinButton,
    long_spin: SpinButton,
    start_sound_sw: Switch,
    finish_sound_sw: Switch,
    auto_next_sw: Switch,
    background_sw: Switch,
    select_btn: Button,
    preview_btn: Button,
    reset_tone_btn: Button,
    file_label: Label,
    error_label: Label,
    accent_btns: Vec<(String, ToggleButton)>,
}

/// Construye la interfaz. Si ya existe una ventana (p. ej. la aplicación estaba
/// en segundo plano), simplemente la vuelve a mostrar en lugar de crear otra.
pub fn build(app: &Application) {
    if let Some(existing) = app.windows().into_iter().next() {
        existing.present();
        return;
    }

    let mut settings = config::load();
    settings.sanitize();
    let accent_hex = settings.accent.clone();
    let accent = theme::parse_hex(&accent_hex).unwrap_or(theme::DEFAULT_RGB);
    let timer = Timer::new(Mode::Work, settings.duration_for(Mode::Work));

    theme::load_css(&accent_hex);

    let state = Rc::new(RefCell::new(State {
        timer,
        settings,
        accent,
        completed_work: 0,
    }));
    let audio = AudioEngine::new();

    // ----- Ventana y cabecera -----
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Pomodoro")
        .default_width(360)
        .default_height(580)
        .build();
    window.add_css_class("timer-window");

    let header = HeaderBar::new();
    header.set_show_title_buttons(true);
    let title = Label::new(Some("POMODORO"));
    title.add_css_class("app-title");
    header.set_title_widget(Some(&title));
    let settings_btn = Button::from_icon_name("emblem-system-symbolic");
    settings_btn.add_css_class("flat-icon");
    settings_btn.set_tooltip_text(Some("Ajustes"));
    header.pack_end(&settings_btn);

    let stack = Stack::new();
    stack.set_transition_type(StackTransitionType::Crossfade);
    stack.set_transition_duration(160);

    // ----- Vista del temporizador -----
    let modes_box = GtkBox::new(Orientation::Horizontal, 0);
    modes_box.add_css_class("mode-switch");
    modes_box.set_halign(Align::Center);
    let mut mode_buttons: Vec<(Mode, ToggleButton)> = Vec::new();
    for mode in Mode::ALL {
        let btn = ToggleButton::with_label(mode.short_label());
        btn.add_css_class("mode-btn");
        if let Some((_, prev)) = mode_buttons.last() {
            btn.set_group(Some(prev));
        }
        btn.set_active(mode == Mode::Work);
        modes_box.append(&btn);
        mode_buttons.push((mode, btn));
    }

    let drawing = DrawingArea::new();
    drawing.set_content_width(240);
    drawing.set_content_height(240);
    drawing.set_hexpand(true);
    drawing.set_vexpand(true);

    let overlay = Overlay::new();
    overlay.set_child(Some(&drawing));
    overlay.set_vexpand(true);

    let center = GtkBox::new(Orientation::Vertical, 2);
    center.set_valign(Align::Center);
    center.set_halign(Align::Center);
    let mode_label = Label::new(Some(&Mode::Work.label().to_uppercase()));
    mode_label.add_css_class("mode-label");
    let time_label = Label::new(Some("25:00"));
    time_label.add_css_class("time-label");
    let status_label = Label::new(Some("Listo"));
    status_label.add_css_class("status-label");
    center.append(&mode_label);
    center.append(&time_label);
    center.append(&status_label);
    overlay.add_overlay(&center);

    let primary = Button::with_label("Iniciar");
    primary.add_css_class("primary");
    primary.set_size_request(160, 44);
    let reset_btn = Button::from_icon_name("view-refresh-symbolic");
    reset_btn.add_css_class("ghost");
    reset_btn.set_size_request(44, 44);
    reset_btn.set_tooltip_text(Some("Reiniciar sesión"));
    let controls = GtkBox::new(Orientation::Horizontal, 10);
    controls.set_halign(Align::Center);
    controls.append(&primary);
    controls.append(&reset_btn);

    let timer_page = GtkBox::new(Orientation::Vertical, 16);
    timer_page.set_margin_top(18);
    timer_page.set_margin_bottom(24);
    timer_page.set_margin_start(20);
    timer_page.set_margin_end(20);
    timer_page.append(&modes_box);
    timer_page.append(&overlay);
    timer_page.append(&controls);

    // ----- Vista de ajustes -----
    let (settings_page, widgets_extra) = build_settings_page(&state);

    stack.add_named(&timer_page, Some("timer"));
    stack.add_named(&settings_page, Some("settings"));
    stack.set_visible_child_name("timer");

    let toolbar = gtk::Box::new(Orientation::Vertical, 0);
    toolbar.append(&header);
    toolbar.append(&stack);
    window.set_child(Some(&toolbar));

    // ----- Empaquetado de referencias -----
    let widgets = Rc::new(Widgets {
        window: window.clone(),
        stack: stack.clone(),
        drawing: drawing.clone(),
        time_label: time_label.clone(),
        mode_label: mode_label.clone(),
        status_label: status_label.clone(),
        primary: primary.clone(),
        reset_btn: reset_btn.clone(),
        settings_btn: settings_btn.clone(),
        back_btn: widgets_extra.back_btn,
        mode_buttons,
        work_spin: widgets_extra.work_spin,
        short_spin: widgets_extra.short_spin,
        long_spin: widgets_extra.long_spin,
        start_sound_sw: widgets_extra.start_sound_sw,
        finish_sound_sw: widgets_extra.finish_sound_sw,
        auto_next_sw: widgets_extra.auto_next_sw,
        background_sw: widgets_extra.background_sw,
        select_btn: widgets_extra.select_btn,
        preview_btn: widgets_extra.preview_btn,
        reset_tone_btn: widgets_extra.reset_tone_btn,
        file_label: widgets_extra.file_label,
        error_label: widgets_extra.error_label,
        accent_btns: widgets_extra.accent_btns,
    });

    connect_signals(&widgets, &state, &audio);
    refresh_settings(&widgets, &state);
    refresh(&widgets, &state);

    window.present();
}

/// Widgets creados en la página de ajustes, para empaquetarlos después.
struct SettingsWidgets {
    back_btn: Button,
    work_spin: SpinButton,
    short_spin: SpinButton,
    long_spin: SpinButton,
    start_sound_sw: Switch,
    finish_sound_sw: Switch,
    auto_next_sw: Switch,
    background_sw: Switch,
    select_btn: Button,
    preview_btn: Button,
    reset_tone_btn: Button,
    file_label: Label,
    error_label: Label,
    accent_btns: Vec<(String, ToggleButton)>,
}

fn build_settings_page(state: &Rc<RefCell<State>>) -> (ScrolledWindow, SettingsWidgets) {
    let settings = state.borrow().settings.clone();

    let page = GtkBox::new(Orientation::Vertical, 4);
    page.set_margin_top(14);
    page.set_margin_bottom(24);
    page.set_margin_start(20);
    page.set_margin_end(20);

    // --- Duraciones ---
    page.append(&section_title("DURACIONES"));
    let work_spin = minutes_spin(settings.work_minutes);
    let short_spin = minutes_spin(settings.short_break_minutes);
    let long_spin = minutes_spin(settings.long_break_minutes);
    page.append(&setting_row("Trabajo", Some("minutos"), &work_spin));
    page.append(&setting_row("Descanso corto", Some("minutos"), &short_spin));
    page.append(&setting_row("Descanso largo", Some("minutos"), &long_spin));

    page.append(&hairline());

    // --- Sonido ---
    page.append(&section_title("SONIDO"));
    let start_sound_sw = Switch::new();
    let finish_sound_sw = Switch::new();
    page.append(&setting_row(
        "Sonido de inicio",
        Some("Bip breve al iniciar o reanudar"),
        &start_sound_sw,
    ));
    page.append(&setting_row(
        "Sonido de finalización",
        Some("Tono al terminar cada sesión"),
        &finish_sound_sw,
    ));

    let select_btn = Button::with_label("Seleccionar…");
    select_btn.add_css_class("ghost");
    let tone_row = setting_row(
        "Tono de finalización",
        Some("WAV, OGG, MP3 o FLAC"),
        &select_btn,
    );
    page.append(&tone_row);

    let file_label = Label::new(None);
    file_label.add_css_class("file-label");
    file_label.set_xalign(0.0);
    file_label.set_halign(Align::Start);
    file_label.set_hexpand(true);
    file_label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    let preview_btn = Button::with_label("Previsualizar");
    preview_btn.add_css_class("ghost");
    let reset_tone_btn = Button::from_icon_name("edit-undo-symbolic");
    reset_tone_btn.add_css_class("ghost");
    reset_tone_btn.set_tooltip_text(Some("Restaurar tono predeterminado"));

    let file_row = GtkBox::new(Orientation::Horizontal, 8);
    file_row.set_margin_start(2);
    file_row.set_margin_bottom(4);
    file_row.append(&file_label);
    file_row.append(&preview_btn);
    file_row.append(&reset_tone_btn);
    page.append(&file_row);

    let error_label = Label::new(None);
    error_label.add_css_class("warn-label");
    error_label.set_xalign(0.0);
    error_label.set_halign(Align::Start);
    error_label.set_wrap(true);
    error_label.set_visible(false);
    page.append(&error_label);

    page.append(&hairline());

    // --- Comportamiento ---
    page.append(&section_title("COMPORTAMIENTO"));
    let auto_next_sw = Switch::new();
    page.append(&setting_row(
        "Iniciar el siguiente periodo automáticamente",
        Some("Si está desactivado, deberás iniciarlo manualmente"),
        &auto_next_sw,
    ));
    let background_sw = Switch::new();
    page.append(&setting_row(
        "Mantener en segundo plano al cerrar",
        Some("Vuelve a abrir la aplicación para recuperar la ventana"),
        &background_sw,
    ));

    page.append(&hairline());

    // --- Apariencia ---
    page.append(&section_title("APARIENCIA"));
    let accent_row = GtkBox::new(Orientation::Horizontal, 8);
    accent_row.set_valign(Align::Center);
    let mut accent_btns: Vec<(String, ToggleButton)> = Vec::new();
    let mut group_holder: Option<ToggleButton> = None;
    for (index, (hex, name)) in theme::PRESETS.iter().enumerate() {
        let btn = ToggleButton::new();
        btn.add_css_class("accent-swatch");
        btn.add_css_class(&format!("accent-{index}"));
        btn.set_tooltip_text(Some(name));
        match &group_holder {
            Some(prev) => btn.set_group(Some(prev)),
            None => group_holder = Some(btn.clone()),
        }
        if settings.accent.eq_ignore_ascii_case(hex) {
            btn.set_active(true);
        }
        accent_row.append(&btn);
        accent_btns.push((hex.to_string(), btn));
    }
    page.append(&setting_row("Color de acento", None, &accent_row));

    page.append(&hairline());

    let back_btn = Button::with_label("← Volver al temporizador");
    back_btn.add_css_class("ghost");
    back_btn.set_halign(Align::Center);
    back_btn.set_margin_top(10);
    page.append(&back_btn);

    let scrolled = ScrolledWindow::new();
    scrolled.set_policy(PolicyType::Never, PolicyType::Automatic);
    scrolled.set_child(Some(&page));

    (
        scrolled,
        SettingsWidgets {
            back_btn,
            work_spin,
            short_spin,
            long_spin,
            start_sound_sw,
            finish_sound_sw,
            auto_next_sw,
            background_sw,
            select_btn,
            preview_btn,
            reset_tone_btn,
            file_label,
            error_label,
            accent_btns,
        },
    )
}

fn section_title(text: &str) -> Label {
    let label = Label::new(Some(text));
    label.add_css_class("section-title");
    label.set_halign(Align::Start);
    label.set_xalign(0.0);
    label.set_margin_top(12);
    label.set_margin_bottom(4);
    label
}

fn hairline() -> Separator {
    let sep = Separator::new(Orientation::Horizontal);
    sep.add_css_class("hairline");
    sep.set_margin_top(10);
    sep.set_margin_bottom(2);
    sep
}

fn minutes_spin(value: u32) -> SpinButton {
    let spin = SpinButton::with_range(MIN_MINUTES as f64, MAX_MINUTES as f64, 1.0);
    spin.set_value(value as f64);
    spin.set_numeric(true);
    spin.set_width_chars(4);
    spin
}

/// Fila de ajuste: título (y subtítulo opcional) a la izquierda, control a la derecha.
fn setting_row(title: &str, subtitle: Option<&str>, control: &impl IsA<gtk::Widget>) -> GtkBox {
    let row = GtkBox::new(Orientation::Horizontal, 12);
    row.set_margin_top(6);
    row.set_margin_bottom(6);

    let texts = GtkBox::new(Orientation::Vertical, 1);
    texts.set_hexpand(true);
    texts.set_valign(Align::Center);

    let title_label = Label::new(Some(title));
    title_label.add_css_class("setting-title");
    title_label.set_halign(Align::Start);
    title_label.set_xalign(0.0);
    texts.append(&title_label);

    if let Some(subtitle) = subtitle {
        let sub = Label::new(Some(subtitle));
        sub.add_css_class("setting-sub");
        sub.set_halign(Align::Start);
        sub.set_xalign(0.0);
        sub.set_wrap(true);
        texts.append(&sub);
    }

    row.append(&texts);
    control.set_valign(Align::Center);
    row.append(control);
    row
}

/// Conecta todos los manejadores de señales.
fn connect_signals(widgets: &Rc<Widgets>, state: &Rc<RefCell<State>>, audio: &AudioEngine) {
    // Redibujo del anillo.
    {
        let state = state.clone();
        widgets.drawing.set_draw_func(move |_area, cr, width, height| {
            let st = state.borrow();
            let progress = st.timer.progress(Instant::now());
            draw_ring(cr, width as f64, height as f64, progress, st.accent);
        });
    }

    // Botón principal: iniciar / pausar / reanudar / siguiente.
    {
        let state = state.clone();
        let widgets = widgets.clone();
        let audio = audio.clone();
        let primary = widgets.primary.clone();
        primary.connect_clicked(move |_| {
            let now = Instant::now();
            let (beep, start_sound) = {
                let mut st = state.borrow_mut();
                let mut beep = false;
                match st.timer.status() {
                    Status::Running => st.timer.pause(now),
                    Status::Paused | Status::Idle => {
                        st.timer.start(now);
                        beep = true;
                    }
                    Status::Finished => {
                        let next = resolve_next(st.timer.mode(), st.completed_work);
                        let duration = st.settings.duration_for(next);
                        st.timer.switch_mode(next, duration);
                        st.timer.start(now);
                        beep = true;
                    }
                }
                (beep, st.settings.start_sound)
            };
            if beep && start_sound {
                audio.beep();
            }
            refresh(&widgets, &state);
        });
    }

    // Reiniciar sesión.
    {
        let state = state.clone();
        let widgets = widgets.clone();
        let reset_btn = widgets.reset_btn.clone();
        reset_btn.connect_clicked(move |_| {
            state.borrow_mut().timer.reset();
            refresh(&widgets, &state);
        });
    }

    // Selector de modo.
    for (mode, button) in &widgets.mode_buttons {
        let mode = *mode;
        let state = state.clone();
        let widgets = widgets.clone();
        button.connect_toggled(move |btn| {
            if !btn.is_active() {
                return;
            }
            match state.try_borrow_mut() {
                Ok(mut st) => {
                    if st.timer.mode() == mode {
                        return;
                    }
                    let duration = st.settings.duration_for(mode);
                    st.timer.switch_mode(mode, duration);
                }
                Err(_) => return,
            }
            refresh(&widgets, &state);
        });
    }

    // Abrir / cerrar ajustes.
    {
        let state = state.clone();
        let widgets = widgets.clone();
        let settings_btn = widgets.settings_btn.clone();
        settings_btn.connect_clicked(move |_| {
            refresh_settings(&widgets, &state);
            widgets.stack.set_visible_child_name("settings");
        });
    }
    {
        let widgets = widgets.clone();
        let back_btn = widgets.back_btn.clone();
        back_btn.connect_clicked(move |_| {
            widgets.stack.set_visible_child_name("timer");
        });
    }

    // Duraciones.
    connect_spin(widgets, state, &widgets.work_spin, Mode::Work);
    connect_spin(widgets, state, &widgets.short_spin, Mode::ShortBreak);
    connect_spin(widgets, state, &widgets.long_spin, Mode::LongBreak);

    // Interruptores.
    connect_switch(widgets, state, &widgets.start_sound_sw, |s, v| s.start_sound = v);
    connect_switch(widgets, state, &widgets.finish_sound_sw, |s, v| s.finish_sound = v);
    connect_switch(widgets, state, &widgets.auto_next_sw, |s, v| s.auto_start_next = v);
    connect_switch(widgets, state, &widgets.background_sw, |s, v| {
        s.run_in_background_on_close = v
    });

    // Acento.
    for (hex, button) in &widgets.accent_btns {
        let hex = hex.clone();
        let state = state.clone();
        let widgets = widgets.clone();
        button.connect_toggled(move |btn| {
            if !btn.is_active() {
                return;
            }
            let snapshot = {
                let mut st = match state.try_borrow_mut() {
                    Ok(st) => st,
                    Err(_) => return,
                };
                st.settings.accent = hex.clone();
                st.accent = theme::parse_hex(&hex).unwrap_or(theme::DEFAULT_RGB);
                st.settings.clone()
            };
            save_settings(&snapshot);
            theme::load_css(&hex);
            widgets.drawing.queue_draw();
        });
    }

    // Selección del tono de finalización (diálogo nativo).
    {
        let state = state.clone();
        let widgets = widgets.clone();
        let select_btn = widgets.select_btn.clone();
        select_btn.connect_clicked(move |_| {
            let dialog = Rc::new(
                FileDialog::builder()
                    .title("Seleccionar tono de finalización")
                    .accept_label("Seleccionar")
                    .modal(true)
                    .build(),
            );

            let filter = FileFilter::new();
            filter.set_name(Some("Archivos de audio"));
            filter.add_mime_type("audio/*");
            for pattern in ["*.wav", "*.ogg", "*.oga", "*.mp3", "*.flac", "*.m4a"] {
                filter.add_pattern(pattern);
            }
            let store = gio::ListStore::new::<FileFilter>();
            store.append(&filter);
            dialog.set_filters(Some(&store));
            dialog.set_default_filter(Some(&filter));

            let parent = widgets.window.clone();
            let state = state.clone();
            let widgets = widgets.clone();
            let dialog_ref = dialog.clone();
            dialog.open(Some(&parent), Cancellable::NONE, move |result| {
                // Mantener vivo el diálogo durante toda la operación.
                let _keep_alive = &dialog_ref;
                if let Ok(file) = result {
                    if let Some(path) = file.path() {
                        let snapshot = {
                            let mut st = state.borrow_mut();
                            st.settings.finish_sound_file = Some(path);
                            st.settings.clone()
                        };
                        save_settings(&snapshot);
                        refresh_settings(&widgets, &state);
                    }
                }
                // Si se cancela (`Err`), la configuración no se modifica.
            });
        });
    }

    // Previsualizar el tono seleccionado.
    {
        let state = state.clone();
        let audio = audio.clone();
        widgets.preview_btn.connect_clicked(move |_| {
            if let Some(path) = state.borrow().settings.finish_sound_file.clone() {
                audio.preview(path);
            }
        });
    }

    // Restaurar el tono predeterminado.
    {
        let state = state.clone();
        let widgets = widgets.clone();
        let reset_tone_btn = widgets.reset_tone_btn.clone();
        reset_tone_btn.connect_clicked(move |_| {
            let snapshot = {
                let mut st = state.borrow_mut();
                st.settings.finish_sound_file = None;
                st.settings.clone()
            };
            save_settings(&snapshot);
            refresh_settings(&widgets, &state);
        });
    }

    // Comportamiento al cerrar.
    {
        let state = state.clone();
        widgets.window.connect_close_request(move |window| {
            if state.borrow().settings.run_in_background_on_close {
                window.set_visible(false);
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
    }

    // Bucle de refresco.
    {
        let state = state.clone();
        let widgets = widgets.clone();
        let audio = audio.clone();
        glib::timeout_add_local(Duration::from_millis(UI_TICK_MS), move || {
            let now = Instant::now();
            let just_finished = state.borrow_mut().timer.tick(now);
            if just_finished {
                on_session_finished(&widgets, &state, &audio);
            }
            if let Some(message) = audio.take_error() {
                widgets.error_label.set_text(&message);
                widgets.error_label.set_visible(true);
            }
            refresh(&widgets, &state);
            glib::ControlFlow::Continue
        });
    }
}

fn connect_spin(
    widgets: &Rc<Widgets>,
    state: &Rc<RefCell<State>>,
    spin: &SpinButton,
    mode: Mode,
) {
    let state = state.clone();
    let widgets = widgets.clone();
    spin.connect_value_changed(move |spin| {
        let value = spin.value_as_int().clamp(MIN_MINUTES as i32, MAX_MINUTES as i32) as u32;
        let snapshot = {
            let mut st = match state.try_borrow_mut() {
                Ok(st) => st,
                Err(_) => return,
            };
            let current = match mode {
                Mode::Work => st.settings.work_minutes,
                Mode::ShortBreak => st.settings.short_break_minutes,
                Mode::LongBreak => st.settings.long_break_minutes,
            };
            if current == value {
                return;
            }
            match mode {
                Mode::Work => st.settings.work_minutes = value,
                Mode::ShortBreak => st.settings.short_break_minutes = value,
                Mode::LongBreak => st.settings.long_break_minutes = value,
            }
            st.settings.sanitize();

            // Aplicar de inmediato si esa sesión no está en curso.
            let status = st.timer.status();
            if st.timer.mode() == mode && matches!(status, Status::Idle | Status::Finished) {
                let duration = st.settings.duration_for(mode);
                st.timer.switch_mode(mode, duration);
            }
            st.settings.clone()
        };
        save_settings(&snapshot);
        refresh(&widgets, &state);
    });
}

fn connect_switch<F>(widgets: &Rc<Widgets>, state: &Rc<RefCell<State>>, switch: &Switch, apply: F)
where
    F: Fn(&mut config::Settings, bool) + 'static,
{
    let state = state.clone();
    let widgets = widgets.clone();
    switch.connect_active_notify(move |switch| {
        let active = switch.is_active();
        let snapshot = {
            let mut st = match state.try_borrow_mut() {
                Ok(st) => st,
                Err(_) => return,
            };
            let changed = {
                let before = st.settings.clone();
                apply(&mut st.settings, active);
                st.settings != before
            };
            if !changed {
                return;
            }
            st.settings.clone()
        };
        save_settings(&snapshot);
        refresh(&widgets, &state);
    });
}

/// Actualiza la vista del temporizador.
fn refresh(widgets: &Rc<Widgets>, state: &Rc<RefCell<State>>) {
    let now = Instant::now();
    let (seconds, status, mode, running) = {
        let st = state.borrow();
        let remaining = st.timer.remaining(now);
        // Redondeo hacia arriba para mostrar 25:00 al inicio y 00:00 al final.
        let seconds = remaining.as_secs() + u64::from(remaining.subsec_nanos() > 0);
        (seconds, st.timer.status(), st.timer.mode(), st.timer.is_running())
    };

    let time_text = format!("{:02}:{:02}", seconds / 60, seconds % 60);
    if widgets.time_label.text().as_str() != time_text {
        widgets.time_label.set_text(&time_text);
    }

    let mode_text = mode.label().to_uppercase();
    if widgets.mode_label.text().as_str() != mode_text {
        widgets.mode_label.set_text(&mode_text);
    }

    let status_text = match status {
        Status::Idle => "Listo",
        Status::Running => "En curso",
        Status::Paused => "En pausa",
        Status::Finished => "¡Sesión completada!",
    };
    if widgets.status_label.text().as_str() != status_text {
        widgets.status_label.set_text(status_text);
    }

    let primary_text = match status {
        Status::Idle => "Iniciar",
        Status::Running => "Pausar",
        Status::Paused => "Reanudar",
        Status::Finished => "Iniciar siguiente",
    };
    if widgets.primary.label().as_deref() != Some(primary_text) {
        widgets.primary.set_label(primary_text);
    }

    if widgets.window.is_visible() {
        for (candidate, button) in &widgets.mode_buttons {
            let should_be_active = *candidate == mode;
            if button.is_active() != should_be_active {
                button.set_active(should_be_active);
            }
        }
    }

    if running {
        widgets.drawing.queue_draw();
    }
}

/// Sincroniza los widgets de ajustes con el estado actual.
fn refresh_settings(widgets: &Rc<Widgets>, state: &Rc<RefCell<State>>) {
    // Se copia la configuración para no mantener prestado el estado mientras se
    // mutan widgets: mutarlos emite señales que vuelven a consultar el estado.
    let settings = state.borrow().settings.clone();

    widgets.work_spin.set_value(settings.work_minutes as f64);
    widgets
        .short_spin
        .set_value(settings.short_break_minutes as f64);
    widgets
        .long_spin
        .set_value(settings.long_break_minutes as f64);

    widgets.start_sound_sw.set_active(settings.start_sound);
    widgets.finish_sound_sw.set_active(settings.finish_sound);
    widgets.auto_next_sw.set_active(settings.auto_start_next);
    widgets
        .background_sw
        .set_active(settings.run_in_background_on_close);

    match &settings.finish_sound_file {
        Some(path) => {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string());
            widgets.file_label.set_text(&name);
            widgets.preview_btn.set_visible(true);
            widgets.reset_tone_btn.set_visible(true);
            if path.exists() {
                widgets.error_label.set_visible(false);
            } else {
                widgets.error_label.set_text(
                    "El archivo configurado ya no está disponible; se usará el tono predeterminado.",
                );
                widgets.error_label.set_visible(true);
            }
        }
        None => {
            widgets.file_label.set_text("Tono predeterminado (carillón)");
            widgets.preview_btn.set_visible(false);
            widgets.reset_tone_btn.set_visible(false);
            widgets.error_label.set_visible(false);
        }
    }
}

/// Al terminar una sesión: sonido, notificación y, opcionalmente, arranque del
/// siguiente periodo.
fn on_session_finished(
    widgets: &Rc<Widgets>,
    state: &Rc<RefCell<State>>,
    audio: &AudioEngine,
) {
    let (mode, next, next_duration, finish_sound, file, auto_start, start_sound) = {
        let mut st = state.borrow_mut();
        let mode = st.timer.mode();
        if mode == Mode::Work {
            st.completed_work += 1;
        }
        let next = resolve_next(mode, st.completed_work);
        let next_duration = st.settings.duration_for(next);
        (
            mode,
            next,
            next_duration,
            st.settings.finish_sound,
            st.settings.finish_sound_file.clone(),
            st.settings.auto_start_next,
            st.settings.start_sound,
        )
    };

    if finish_sound {
        audio.finish(file);
    }

    let auto_started = if auto_start {
        let mut st = state.borrow_mut();
        st.timer.switch_mode(next, next_duration);
        st.timer.start(Instant::now());
        true
    } else {
        false
    };

    if auto_started && start_sound {
        audio.beep();
    }

    let body = if auto_started {
        format!("Comienza: {}", next.label())
    } else {
        format!("Pulsa Iniciar para {}", next.label().to_lowercase())
    };
    notifications::send(&format!("{} completado", mode.label()), &body, true);

    if !auto_started {
        widgets.status_label.set_text("¡Sesión completada!");
    }
}

/// Tras un trabajo, descanso corto (o largo cada cuatro); tras un descanso, trabajo.
fn resolve_next(mode: Mode, completed_work: u32) -> Mode {
    match mode {
        Mode::Work => {
            if completed_work > 0 && completed_work.is_multiple_of(4) {
                Mode::LongBreak
            } else {
                Mode::ShortBreak
            }
        }
        Mode::ShortBreak | Mode::LongBreak => Mode::Work,
    }
}

/// Guarda los ajustes registrando los fallos sin interrumpir la aplicación.
fn save_settings(settings: &config::Settings) {
    if let Err(err) = config::save(settings) {
        eprintln!("pomodoro-timer: no se pudieron guardar los ajustes: {err}");
    }
}

/// Dibuja el anillo de progreso con Cairo.
fn draw_ring(
    cr: &gtk::cairo::Context,
    width: f64,
    height: f64,
    progress: f64,
    accent: (f64, f64, f64),
) {
    let size = width.min(height);
    if size <= 0.0 {
        return;
    }

    let center_x = width / 2.0;
    let center_y = height / 2.0;
    let line_width = (size * 0.05).clamp(6.0, 13.0);
    let radius = (size / 2.0 - line_width / 2.0 - 8.0).max(1.0);
    let start = -std::f64::consts::FRAC_PI_2;
    let full = std::f64::consts::TAU;

    cr.set_line_cap(gtk::cairo::LineCap::Round);

    // Pista de fondo.
    cr.set_line_width(line_width);
    cr.new_path();
    cr.arc(center_x, center_y, radius, 0.0, full);
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.07);
    let _ = cr.stroke();

    if progress > 0.0005 {
        let end = start + full * progress;

        // Halo suave (discreto, sin sombras costosas).
        cr.new_path();
        cr.arc(center_x, center_y, radius, start, end);
        cr.set_line_width(line_width * 2.0);
        cr.set_source_rgba(accent.0, accent.1, accent.2, 0.13);
        let _ = cr.stroke();

        // Arco de progreso.
        cr.new_path();
        cr.arc(center_x, center_y, radius, start, end);
        cr.set_line_width(line_width);
        cr.set_source_rgba(accent.0, accent.1, accent.2, 1.0);
        let _ = cr.stroke();
    }
}

/// Fuerza el uso del tema oscuro, coherente con la paleta de la aplicación.
pub fn prefer_dark_theme() {
    let Some(settings) = gtk::Settings::default() else {
        return;
    };
    // GTK 4.20 sustituyó `gtk-application-prefer-dark-theme` (ahora deprecado)
    // por `gtk-interface-color-scheme`. Se detecta en tiempo de ejecución para
    // funcionar igual en GTK 4.10+ sin advertir en las versiones nuevas.
    if let Some(pspec) = settings.find_property("gtk-interface-color-scheme") {
        set_enum_property(
            &settings,
            "gtk-interface-color-scheme",
            pspec.value_type(),
            gtk::ffi::GTK_INTERFACE_COLOR_SCHEME_DARK,
        );
    } else {
        settings.set_gtk_application_prefer_dark_theme(true);
    }
}

/// Asigna una propiedad de tipo enumerado a través de GObject.
///
/// Los enlaces tipados de gtk4-rs para `GtkInterfaceColorScheme` están tras una
/// feature ligada a GTK 4.20; hacerlo por FFI mantiene compatibilidad con GTK
/// anteriores sin subir el requisito de versión.
fn set_enum_property<O: glib::prelude::ObjectType>(
    object: &O,
    name: &str,
    value_type: glib::Type,
    value: i32,
) {
    use glib::gobject_ffi;
    use glib::translate::IntoGlib;

    let Ok(name) = std::ffi::CString::new(name) else {
        return;
    };

    // SAFETY: se construye un `GValue` del mismo tipo que la propiedad, se
    // rellena con `g_value_set_enum` y se libera con `g_value_unset`. El objeto
    // es válido durante toda la llamada.
    unsafe {
        let mut gvalue: gobject_ffi::GValue = std::mem::zeroed();
        gobject_ffi::g_value_init(&mut gvalue, value_type.into_glib());
        gobject_ffi::g_value_set_enum(&mut gvalue, value);
        gobject_ffi::g_object_set_property(
            object.as_ptr() as *mut gobject_ffi::GObject,
            name.as_ptr(),
            &gvalue,
        );
        gobject_ffi::g_value_unset(&mut gvalue);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_after_work_is_short_break_by_default() {
        assert_eq!(resolve_next(Mode::Work, 1), Mode::ShortBreak);
        assert_eq!(resolve_next(Mode::Work, 2), Mode::ShortBreak);
        assert_eq!(resolve_next(Mode::Work, 3), Mode::ShortBreak);
    }

    #[test]
    fn every_fourth_work_leads_to_long_break() {
        assert_eq!(resolve_next(Mode::Work, 4), Mode::LongBreak);
        assert_eq!(resolve_next(Mode::Work, 8), Mode::LongBreak);
    }

    #[test]
    fn after_any_break_comes_work() {
        assert_eq!(resolve_next(Mode::ShortBreak, 1), Mode::Work);
        assert_eq!(resolve_next(Mode::LongBreak, 4), Mode::Work);
    }
}
