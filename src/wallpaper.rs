#[cfg(any(target_os = "macos", target_os = "windows"))]
use anyhow::Context;
use anyhow::{anyhow, Result};
use std::path::Path;
use std::process::Command;

pub fn set(path: &Path) -> Result<()> {
    if !path.is_file() {
        anyhow::bail!("wallpaper image does not exist: {}", path.display());
    }
    #[cfg(target_os = "macos")]
    return set_macos(path);
    #[cfg(target_os = "windows")]
    return set_windows(path);
    #[cfg(target_os = "linux")]
    return set_linux(path);
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    Err(anyhow!("this operating system is not supported"))
}

#[cfg(target_os = "macos")]
fn set_macos(path: &Path) -> Result<()> {
    let script = r#"on run argv
    set wallpaperPath to POSIX file (item 1 of argv)
    tell application "System Events" to set picture of every desktop to wallpaperPath
end run"#;
    let status = Command::new("osascript")
        .args(["-e", script])
        .arg(path)
        .status()
        .context("launching macOS wallpaper command")?;
    ensure_success(status, "macOS wallpaper command")
}

#[cfg(target_os = "windows")]
fn set_windows(path: &Path) -> Result<()> {
    let script = r#"$path = $env:DAILY_WALLPAPER_PATH
Add-Type -TypeDefinition @'
using System.Runtime.InteropServices;
public static class DailyWallpaper {
    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool SystemParametersInfo(int action, int parameter, string path, int flags);
}
'@
if (-not [DailyWallpaper]::SystemParametersInfo(20, 0, $path, 3)) { exit 1 }"#;
    let status = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("DAILY_WALLPAPER_PATH", path)
        .status()
        .context("launching Windows wallpaper command")?;
    ensure_success(status, "Windows wallpaper command")
}

#[cfg(target_os = "linux")]
fn set_linux(path: &Path) -> Result<()> {
    let path_text = path.to_string_lossy();
    let encoded_path = urlencoding::encode(&path_text);
    let uri = format!("file://{encoded_path}");
    if let Ok(status) = Command::new("gsettings")
        .args(["set", "org.gnome.desktop.background", "picture-uri", &uri])
        .status()
    {
        if status.success() {
            let _ = Command::new("gsettings")
                .args([
                    "set",
                    "org.gnome.desktop.background",
                    "picture-uri-dark",
                    &uri,
                ])
                .status();
            return Ok(());
        }
    }
    for command in ["feh", "nitrogen"] {
        let result = if command == "feh" {
            Command::new(command)
                .args(["--bg-fill", path.to_str().unwrap_or_default()])
                .status()
        } else {
            Command::new(command)
                .args(["--set-zoom-fill", path.to_str().unwrap_or_default()])
                .status()
        };
        if let Ok(status) = result {
            if status.success() {
                return Ok(());
            }
        }
    }
    Err(anyhow!(
        "could not set wallpaper: install GNOME gsettings, feh, or nitrogen"
    ))
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn ensure_success(status: std::process::ExitStatus, operation: &str) -> Result<()> {
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("{operation} failed with status {status}"))
    }
}
