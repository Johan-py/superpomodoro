# Makefile de conveniencia. `make install` usa PREFIX (/usr/local por defecto).

PREFIX  ?= /usr/local
BINDIR  ?= $(PREFIX)/bin
APPDIR  ?= $(PREFIX)/share/applications
ICONDIR ?= $(PREFIX)/share/icons/hicolor/scalable/apps
CARGO   ?= cargo

APP_ID := io.github.pomodoro.PomodoroTimer

.PHONY: all build release run test clippy install uninstall clean

all: release

## Compila en modo depuración
build:
	$(CARGO) build

## Compila el binario optimizado
release:
	$(CARGO) build --release

## Ejecuta la aplicación
run:
	$(CARGO) run

## Diagnóstico de audio (bip + tono; admite una ruta de fichero opcional)
audio-test:
	$(CARGO) run --release -- --audio-test

## Ejecuta las pruebas unitarias
test:
	$(CARGO) test

## Analiza el código con Clippy tratando los avisos como errores
clippy:
	$(CARGO) clippy --all-targets -- -D warnings

## Instala binario, entrada de escritorio e icono
install: release
	install -Dm755 target/release/pomodoro-timer $(DESTDIR)$(BINDIR)/pomodoro-timer
	install -Dm644 assets/$(APP_ID).desktop $(DESTDIR)$(APPDIR)/$(APP_ID).desktop
	install -Dm644 assets/$(APP_ID).svg $(DESTDIR)$(ICONDIR)/$(APP_ID).svg
	-gtk-update-icon-cache -qtf $(DESTDIR)$(PREFIX)/share/icons/hicolor 2>/dev/null || true
	-update-desktop-database $(DESTDIR)$(APPDIR) 2>/dev/null || true

## Desinstala los ficheros instalados
uninstall:
	rm -f $(DESTDIR)$(BINDIR)/pomodoro-timer
	rm -f $(DESTDIR)$(APPDIR)/$(APP_ID).desktop
	rm -f $(DESTDIR)$(ICONDIR)/$(APP_ID).svg

clean:
	$(CARGO) clean
