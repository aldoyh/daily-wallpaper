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
  - macOS: `osascript` / System Events
  - Windows: PowerShell and `SystemParametersInfo`
  - Linux: GNOME `gsettings`, then `feh` or `nitrogen`

## Run

```sh
cargo run
```

On Linux, install one of `gsettings` (GNOME), `feh`, or `nitrogen` for wallpaper application.

Applied images are cached locally under the platform-specific application-data directory using the `directories` crate. The app does not require credentials or an API key.

## Design notes

The UI never performs network requests or wallpaper process launches on the egui event thread. Each operation runs in a short-lived background task and reports a typed result back to the UI. Provider responses are size-limited before allocation, and native command adapters pass paths as arguments or environment variables rather than interpolating them into shell commands.

## CI and native builds

GitHub Actions builds and verifies the application on all three supported desktop platforms. The workflow runs formatting checks, compilation, tests, clippy, and a release build on `ubuntu-latest`, `macos-latest`, and `windows-latest`. Each run uploads the corresponding release binary as an artifact.

The Linux workflow installs the X11, Wayland, GTK, OpenGL, and `pkg-config` development packages needed to compile the native `eframe` window.
