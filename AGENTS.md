# AGENTS.md

Single Rust + GTK4 desktop app. The crate lives in `pomodoro-timer/`; run all
`cargo`/`make` commands from there. There is no git repo, lib target, CI, or
`opencode.json`. User-facing docs live in `pomodoro-timer/README.md`.

## Commands

- `cargo test` — unit tests only, inline per module (`#[cfg(test)]`); no lib
  target, so this is the only test entrypoint.
- `cargo clippy --all-targets -- -D warnings` (or `make clippy`) — expected
  **clean**; keep it that way.
- `cargo build --release` (or `make release`) — LTO + strip; cold build ~1–2 min.
- `make audio-test` / `pomodoro-timer --audio-test [file]` — runs the real audio
  path (start beep + file or default chime, prints errors) **without opening a
  window**. Use this for headless verification instead of launching the GUI.
- Do not run the GUI (`cargo run`) as a blocking step: it is a long-running app.
- Stop a running instance with `pkill -x pomodoro-timer`. **Never**
  `pkill -f pomodoro-timer` — it matches your own shell's command line and kills
  the shell.

## Non-obvious constraints

- The GTK crate is named `gtk4`; modules alias it with `use gtk4 as gtk;`.
  `gtk::…` paths only resolve where that alias exists. The `v4_10` feature is
  required for `gtk::FileDialog`; build needs GTK ≥ 4.10 (built/tested on 4.24).
- UI state is `Rc<RefCell<State>>`. Mutating widgets programmatically re-emits
  their signals, which re-enters the handlers. Pattern used everywhere: snapshot
  `state.borrow().settings.clone()` **before** touching widgets, and use
  `state.try_borrow_mut()` inside signal handlers. A plain `borrow_mut()` there
  panics ("RefCell already borrowed") and aborts, because it unwinds inside a GTK
  callback.
- `ui::prefer_dark_theme` must keep its runtime check: on GTK ≥ 4.20 it sets
  `gtk-interface-color-scheme` through raw GObject FFI (`set_enum_property`),
  otherwise the deprecated `gtk-application-prefer-dark-theme`. The typed bindings
  for that enum need gtk4 feature `v4_20`, which is deliberately not enabled.
- Audio: the dedicated thread owns the `OutputStream` (it is `!Send`) and takes
  commands over an mpsc channel — never create a stream on the GTK thread. Start
  beep and default chime are synthesized in memory (no bundled assets, offline).
- Notifications shell out to `notify-send`; there is deliberately no D-Bus crate.
- `io.github.pomodoro.PomodoroTimer` (in `main.rs`) is load-bearing: `.desktop`
  filename, icon name, notification icon, and single-instance. Relaunching
  presents the existing window via `app.windows()`, **not** `active_window()`, so
  a window hidden by "run in background" is still found.
- Settings live at `$XDG_CONFIG_HOME/pomodoro-timer/settings.toml` (→ `~/.config`
  fallback). To test without touching the user's real config, point
  `XDG_CONFIG_HOME` at a scratch dir. Load is fault-tolerant: missing/corrupt →
  defaults, and a corrupt file is renamed `*.toml.corrupt`.
- Tests must not introduce a `tempfile` dependency; `config` tests create unique
  dirs under `std::env::temp_dir()` via `temp_path()`.
- `timer.rs` is pure (no GTK, no I/O): every method takes `now: Instant` so tests
  inject time. Keep it that way. Time is monotonic, so a running session does not
  advance while the machine is suspended — intentional, noted in the README.

## Environment

- Target is Arch Linux, Wayland (Hyprland) and X11. Runtime deps: `gtk4`,
  `alsa-lib`, `libnotify`.
- Visually verifying the GUI needs a real session; capture a window with
  `grim -g "<x,y> <w>x<h>"` (geometry from `hyprctl clients -j`). On this host
  `hyprctl dispatch` is Lua-based and silently ignores classic dispatcher syntax,
  so don't rely on it to close/focus windows.
