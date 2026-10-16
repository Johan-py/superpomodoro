# Mantenedor: Pomodoro Timer contributors
pkgname=pomodoro-timer
pkgver=0.1.0
pkgrel=1
pkgdesc="Reloj Pomodoro minimalista y ligero para Linux (GTK4)"
arch=('x86_64' 'aarch64')
url="https://example.invalid/pomodoro-timer"
license=('MIT')
depends=('gtk4' 'alsa-lib' 'libnotify')
makedepends=('cargo' 'pkgconf')
provides=('pomodoro-timer')
conflicts=('pomodoro-timer')
options=('!lto')
# Sustituir por la URL del tarball/Git del proyecto, o empaquetar el directorio
# local creando antes el tarball:
#   tar -czf pomodoro-timer-$pkgver.tar.gz --transform "s,^,pomodoro-timer-$pkgver/," \
#       pomodoro-timer/{Cargo.toml,Cargo.lock,src,assets,LICENSE}
source=("$pkgname-$pkgver.tar.gz")
sha256sums=('SKIP')

build() {
  cd "$pkgname-$pkgver"
  # `--frozen` usa Cargo.lock tal cual (garantiza reproducibilidad).
  # Si empaquetas sin red, vendoriza antes:  cargo vendor vendor/
  # y añade en .cargo/config.toml:
  #   [source.crates-io]
  #   replace-with = "vendored-sources"
  #   [source.vendored-sources]
  #   directory = "vendor"
  cargo build --release --frozen
}

package() {
  cd "$pkgname-$pkgver"
  install -Dm755 target/release/pomodoro-timer "$pkgdir/usr/bin/pomodoro-timer"
  install -Dm644 assets/io.github.pomodoro.PomodoroTimer.desktop \
    "$pkgdir/usr/share/applications/io.github.pomodoro.PomodoroTimer.desktop"
  install -Dm644 assets/io.github.pomodoro.PomodoroTimer.svg \
    "$pkgdir/usr/share/icons/hicolor/scalable/apps/io.github.pomodoro.PomodoroTimer.svg"
  install -Dm644 LICENSE "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
}
