#[cfg(not(target_os = "windows"))]
use anyhow::anyhow;
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use anyhow::Context;
use anyhow::Result;
use std::path::Path;

#[cfg(any(target_os = "linux", target_os = "macos"))]
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

// ---------------------------------------------------------------------------
// macOS
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
fn set_macos(path: &Path) -> Result<()> {
    // The path travels through the environment instead of osascript's argument
    // list: arguments only reach an `on run` handler reliably when a `-`
    // separator or a script file is used, and `system attribute` avoids that
    // ambiguity (and any quoting) entirely.
    let script = r#"on run
    set wallpaperPath to system attribute "DAILY_WALLPAPER_PATH"
    tell application "System Events" to set picture of every desktop to (POSIX file wallpaperPath)
end run"#;
    let mut command = Command::new("osascript");
    command
        .args(["-e", script])
        .env("DAILY_WALLPAPER_PATH", path);
    run_checked(command, "osascript").map_err(|error| {
        let text = format!("{error:#}");
        if text.contains("-1743") || text.contains("not authorized") {
            anyhow!(
                "{text} (allow Daily Wallpaper to control System Events under \
                 System Settings > Privacy & Security > Automation)"
            )
        } else {
            error
        }
    })
}

// ---------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
fn set_windows(path: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE, SPI_SETDESKWALLPAPER,
    };

    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide.push(0);
    // Calling user32 directly avoids the PowerShell + Add-Type round trip (about
    // a second per apply), never trips execution policy, and is Unicode safe.
    let updated = unsafe {
        SystemParametersInfoW(
            SPI_SETDESKWALLPAPER,
            0,
            wide.as_ptr().cast_mut().cast(),
            SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
        )
    };
    if updated == 0 {
        return Err(std::io::Error::last_os_error())
            .context("windows refused to set the desktop wallpaper");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Linux
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Desktop {
    Gnome,
    Cinnamon,
    Mate,
    Xfce,
    Kde,
    Unknown,
}

#[cfg(target_os = "linux")]
impl Desktop {
    fn label(self) -> &'static str {
        match self {
            Self::Gnome => "GNOME",
            Self::Cinnamon => "Cinnamon",
            Self::Mate => "MATE",
            Self::Xfce => "Xfce",
            Self::Kde => "KDE Plasma",
            Self::Unknown => "an unrecognized desktop",
        }
    }
}

/// `gsettings` schema, key, and whether the key expects a URI (MATE wants a
/// plain filesystem path) for the desktops that use dconf.
#[cfg(target_os = "linux")]
fn gsettings_setting(desktop: Desktop) -> Option<(&'static str, &'static str, bool)> {
    match desktop {
        Desktop::Gnome | Desktop::Unknown => {
            Some(("org.gnome.desktop.background", "picture-uri", true))
        }
        Desktop::Cinnamon => Some(("org.cinnamon.desktop.background", "picture-uri", true)),
        Desktop::Mate => Some(("org.mate.background", "picture-filename", false)),
        Desktop::Xfce | Desktop::Kde => None,
    }
}

#[cfg(target_os = "linux")]
fn detect_desktop(xdg_current_desktop: &str, desktop_session: &str) -> Desktop {
    let combined = format!("{xdg_current_desktop} {desktop_session}").to_ascii_lowercase();
    for token in combined
        .split(|c: char| c == ':' || c == ';' || c == ',' || c.is_ascii_whitespace())
        .map(|token| token.trim().trim_start_matches("x-"))
    {
        match token {
            "cinnamon" => return Desktop::Cinnamon,
            "mate" => return Desktop::Mate,
            "xfce" => return Desktop::Xfce,
            "kde" | "plasma" => return Desktop::Kde,
            "gnome" | "gnome-classic" | "gnome-flashback" | "ubuntu" | "unity" | "budgie"
            | "pantheon" | "elementary" | "pop" => return Desktop::Gnome,
            _ => {}
        }
    }
    Desktop::Unknown
}

#[cfg(target_os = "linux")]
fn set_linux(path: &Path) -> Result<()> {
    let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let path_text = path.to_string_lossy().into_owned();
    let desktop = detect_desktop(
        &std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default(),
        &std::env::var("DESKTOP_SESSION").unwrap_or_default(),
    );
    let mut failures: Vec<String> = Vec::new();

    if let Some((schema, key, wants_uri)) = gsettings_setting(desktop) {
        let value = if wants_uri {
            file_uri(&path_text)
        } else {
            path_text.clone()
        };
        match apply_gsettings(schema, key, &value) {
            Ok(()) => {
                if schema == "org.gnome.desktop.background" {
                    // Dark mode reads its own key; light mode already succeeded,
                    // so a missing dark key on older GNOME is not fatal.
                    let _ = apply_gsettings(schema, "picture-uri-dark", &value);
                }
                return Ok(());
            }
            Err(error) => failures.push(format!("{error:#}")),
        }
    }

    let desktop_attempt = match desktop {
        Desktop::Xfce => Some(apply_xfce(&path_text)),
        Desktop::Kde => Some(apply_kde(&path_text)),
        _ => None,
    };
    if let Some(result) = desktop_attempt {
        match result {
            Ok(()) => return Ok(()),
            Err(error) => failures.push(format!("{error:#}")),
        }
    }

    for (program, style) in [("feh", "--bg-fill"), ("nitrogen", "--set-zoom-fill")] {
        let mut command = Command::new(program);
        command.args([style, &path_text]);
        match run_checked(command, program) {
            Ok(()) => return Ok(()),
            Err(error) => failures.push(format!("{error:#}")),
        }
    }

    Err(anyhow!(
        "could not set the wallpaper on {} ({}). Install a backend for this desktop: \
         gsettings for GNOME, Cinnamon, or MATE; xfconf-query for Xfce; \
         plasma-apply-wallpaperimage (Plasma 5.24+) for KDE; or feh/nitrogen \
         for other X11 window managers",
        desktop.label(),
        failures.join("; ")
    ))
}

#[cfg(target_os = "linux")]
fn apply_gsettings(schema: &str, key: &str, value: &str) -> Result<()> {
    let mut command = Command::new("gsettings");
    command.args(["set", schema, key, value]);
    run_checked(command, "gsettings")
}

#[cfg(target_os = "linux")]
fn apply_xfce(path_text: &str) -> Result<()> {
    let mut listing = Command::new("xfconf-query");
    listing.args(["-c", "xfce4-desktop", "-l"]);
    let properties = xfce_backdrop_properties(&run_capture(listing, "xfconf-query")?);
    if properties.is_empty() {
        return Err(anyhow!(
            "xfconf-query found no /backdrop/.../last-image properties"
        ));
    }

    let mut failures = Vec::new();
    for property in properties {
        let mut command = Command::new("xfconf-query");
        command.args(["-c", "xfce4-desktop", "-p", &property, "-s", path_text]);
        if let Err(error) = run_checked(command, "xfconf-query") {
            failures.push(format!("{property}: {error:#}"));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(anyhow!(failures.join("; ")))
    }
}

#[cfg(target_os = "linux")]
fn xfce_backdrop_properties(listing: &str) -> Vec<String> {
    listing
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("/backdrop/") && line.ends_with("last-image"))
        .map(str::to_string)
        .collect()
}

#[cfg(target_os = "linux")]
fn apply_kde(path_text: &str) -> Result<()> {
    let mut command = Command::new("plasma-apply-wallpaperimage");
    command.arg(path_text);
    run_checked(command, "plasma-apply-wallpaperimage")
        .context("setting the Plasma wallpaper requires Plasma 5.24 or newer")
}

/// Builds an RFC 8089 `file:` URI. Slashes must stay literal while everything
/// else that is not URI-safe (spaces, `#`, `?`, non-ASCII bytes) is escaped.
/// Encoding the whole path would produce `file://%2Fhome%2F...`, which desktop
/// environments reject as an unsupported `%2F` host.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn file_uri(path: &str) -> String {
    let mut uri = String::from("file://");
    for (index, segment) in path.split('/').enumerate() {
        if index > 0 {
            uri.push('/');
        }
        uri.push_str(&urlencoding::encode(segment));
    }
    uri
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn run_checked(mut command: Command, label: &str) -> Result<()> {
    let output = command
        .output()
        .with_context(|| format!("launching {label}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = summarize(&String::from_utf8_lossy(&output.stderr));
    let detail = if stderr.is_empty() {
        summarize(&String::from_utf8_lossy(&output.stdout))
    } else {
        stderr
    };
    if detail.is_empty() {
        return Err(anyhow!("{label} exited with {}", output.status));
    }
    Err(anyhow!("{label}: {detail}"))
}

#[cfg(target_os = "linux")]
fn run_capture(mut command: Command, label: &str) -> Result<String> {
    let output = command
        .output()
        .with_context(|| format!("launching {label}"))?;
    if !output.status.success() {
        let detail = summarize(&String::from_utf8_lossy(&output.stderr));
        return Err(anyhow!(
            "{label}: {}",
            if detail.is_empty() {
                output.status.to_string()
            } else {
                detail
            }
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Keeps error reporting usable in a small status line: first meaningful line,
/// bounded length, no trailing command noise.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn summarize(text: &str) -> String {
    const LIMIT: usize = 240;
    let line = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    if line.chars().count() <= LIMIT {
        return line.to_string();
    }
    let mut truncated: String = line.chars().take(LIMIT).collect();
    truncated.push('…');
    truncated
}

#[cfg(test)]
mod tests {
    use super::file_uri;

    #[test]
    fn file_uri_keeps_slashes_and_escapes_the_rest() {
        assert_eq!(
            file_uri("/home/alice/Pictures/wallpaper.jpg"),
            "file:///home/alice/Pictures/wallpaper.jpg"
        );
        assert_eq!(
            file_uri("/home/alice/My Pictures/dawn #2.jpg"),
            "file:///home/alice/My%20Pictures/dawn%20%232.jpg"
        );
    }

    #[test]
    fn file_uri_escapes_non_ascii_paths() {
        assert_eq!(
            file_uri("/home/alice/фото/日出.jpg"),
            "file:///home/alice/%D1%84%D0%BE%D1%82%D0%BE/%E6%97%A5%E5%87%BA.jpg"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn detects_the_desktop_from_session_variables() {
        use super::{detect_desktop, Desktop};

        assert_eq!(detect_desktop("ubuntu:GNOME", "ubuntu"), Desktop::Gnome);
        assert_eq!(detect_desktop("GNOME", ""), Desktop::Gnome);
        assert_eq!(detect_desktop("pop:GNOME", "pop"), Desktop::Gnome);
        assert_eq!(detect_desktop("X-Cinnamon", "cinnamon"), Desktop::Cinnamon);
        assert_eq!(detect_desktop("MATE", "mate"), Desktop::Mate);
        assert_eq!(detect_desktop("XFCE", "xfce"), Desktop::Xfce);
        assert_eq!(detect_desktop("KDE", "plasmawayland"), Desktop::Kde);
        assert_eq!(detect_desktop("", ""), Desktop::Unknown);
        assert_eq!(detect_desktop("sway", ""), Desktop::Unknown);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn mate_reads_a_path_while_gnome_reads_a_uri() {
        use super::{gsettings_setting, Desktop};

        assert_eq!(
            gsettings_setting(Desktop::Gnome),
            Some(("org.gnome.desktop.background", "picture-uri", true))
        );
        assert_eq!(
            gsettings_setting(Desktop::Mate),
            Some(("org.mate.background", "picture-filename", false))
        );
        assert_eq!(gsettings_setting(Desktop::Xfce), None);
        assert_eq!(gsettings_setting(Desktop::Kde), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn finds_every_xfce_backdrop_monitor() {
        use super::xfce_backdrop_properties;

        let listing = "/backdrop/screen0/monitorDP-1/workspace0/last-image\n\
                       /backdrop/screen0/monitorDP-1/workspace0/image-style\n\
                       /backdrop/screen0/monitorHDMI-1/workspace0/last-image\n";
        assert_eq!(
            xfce_backdrop_properties(listing),
            vec![
                "/backdrop/screen0/monitorDP-1/workspace0/last-image",
                "/backdrop/screen0/monitorHDMI-1/workspace0/last-image",
            ]
        );
    }
}
