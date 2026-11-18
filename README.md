# Daily Wallpaper

A small Rust desktop app for fetching and applying a fresh wallpaper on macOS, Windows, and Linux. Version 0.2 is designed to stay responsive while network and operating-system work happens in the background.

## Features

- Bing image of the day, with title and copyright metadata.
- Random photos from Picsum Photos, requiring no API key.
- Preview before applying.
- Bounded downloads with connection and total-request timeouts.
- Atomic local cache updates in the platform application-data directory.
- Dark, compact UI with clear loading and error states.
- Native wallpaper adapters:
  - macOS: `osascript` / System Events, for every desktop and display
  - Windows: `user32` `SystemParametersInfoW` called directly, no PowerShell
  - Linux: the wallpaper backend is chosen from the running desktop:
    - GNOME (and Ubuntu, Pop, Unity, Budgie, Pantheon, Elementary): `gsettings`
    - Cinnamon: `gsettings` on `org.cinnamon.desktop.background`
    - MATE: `gsettings` on `org.mate.background`
    - Xfce: `xfconf-query`, applied to every monitor's backdrop property
    - KDE Plasma: `plasma-apply-wallpaperimage` (Plasma 5.24+)
    - Anything else on X11: `feh`, then `nitrogen`
  - Paths are passed as RFC 8089 file URIs (or plain paths where a desktop expects
    them), so spaces, `#`, and non-ASCII names work on all three platforms
  - Backend standard error is surfaced in the UI, so a failure says why

## Run

```sh
cargo run
```

## Build and install on Linux

Building `eframe` needs the usual desktop toolchain: a C compiler, `pkg-config`, and
the X11/Wayland client libraries. On Debian/Ubuntu:

```sh
sudo apt install build-essential pkg-config libx11-dev libxi-dev \
  libwayland-dev libxkbcommon-dev libgl1-mesa-dev
```

Build an optimized binary and put it on your `PATH`:

```sh
cargo build --release
install -Dm755 target/release/daily-wallpaper ~/.local/bin/daily-wallpaper
```

`~/.local/bin` is on `PATH` in a default Ubuntu session; if it is not, either add it
to your shell profile or install somewhere already on `PATH`. `cargo install --path .`
also works and installs to `~/.cargo/bin`.

Check the install and launch it:

```sh
command -v daily-wallpaper
daily-wallpaper
```

To verify without the window, run the test suite and confirm the desktop now points at
the cached image after applying a wallpaper in the app:

```sh
cargo test --all-targets
gsettings get org.gnome.desktop.background picture-uri
gio info "$(gsettings get org.gnome.desktop.background picture-uri | tr -d "'")"
```

The image is cached as `current.jpg` under `$XDG_DATA_HOME/dailywallpaper` (usually
`~/.local/share/dailywallpaper`). The app inherits `XDG_DATA_HOME` from the shell that
started it, so launching from a sandboxed terminal (for example the VS Code Flatpak)
can put the cache under that sandbox's data directory instead. To uninstall, remove
`~/.local/bin/daily-wallpaper` and that data directory.

Platform notes:

- **macOS** asks for permission the first time the app changes the wallpaper. Approve
  Daily Wallpaper under System Settings > Privacy & Security > Automation; a denied
  request is reported in the app with that hint.
- **Windows** needs nothing beyond Windows itself; the Win32 API is called directly,
  and Unicode profile paths are supported.
- **Linux** needs the wallpaper tool that matches your desktop: `gsettings` (GNOME,
  Cinnamon, MATE), `xfconf-query` (Xfce), `plasma-apply-wallpaperimage` (KDE Plasma
  5.24+), or `feh`/`nitrogen` for other X11 window managers. Bare Wayland compositors
  such as Sway or Hyprland have no wallpaper API of their own and are not supported.

Applied images are cached locally under the platform-specific application-data directory using the `directories` crate. The app does not require credentials or an API key.

## Design notes

The UI never performs network requests or wallpaper process launches on the egui event thread. Each operation runs in a short-lived background task and reports a typed result back to the UI. Provider responses are size-limited before allocation, and native command adapters pass paths as arguments or environment variables rather than interpolating them into shell commands. macOS receives the path through the environment because `osascript` only forwards arguments reliably with a `-` separator or a script file.

## CI and native builds

GitHub Actions builds and verifies the application on all three supported desktop platforms. The workflow runs formatting checks, compilation, tests, clippy, and a release build on `ubuntu-latest`, `macos-latest`, and `windows-latest`. Each run uploads the corresponding release binary as an artifact.

The Linux workflow installs the X11, Wayland, GTK, OpenGL, and `pkg-config` development packages needed to compile the native `eframe` window.
