//! The interface among the computer's applications: an entry with an icon
//! that starts it, made on this machine for this user.
//!
//! - macOS: `~/Applications/anycube.app`, a bundle with an `Info.plist`,
//!   an icon set and a launcher. Made here, it carries no quarantine mark,
//!   so it opens without a warning.
//! - Linux: `~/.local/share/applications/anycube.desktop` and the icon in
//!   the usual sizes under `~/.local/share/icons/hicolor`.
//!
//! Nothing is written outside the user's own directories, and `remove`
//! takes all of it away again.

use super::{assets, window};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

fn home() -> io::Result<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))
}

/// Where the entry goes on this system, as the paths `add` writes.
pub fn places() -> io::Result<Vec<PathBuf>> {
    let home = home()?;
    if cfg!(target_os = "macos") {
        return Ok(vec![home.join("Applications/anycube.app")]);
    }
    let share = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".local/share"));
    let mut out = vec![share.join("applications/anycube.desktop")];
    for (size, _) in assets::ICONS.iter().filter(|i| i.0 <= 512) {
        out.push(share.join(format!("icons/hicolor/{0}x{0}/apps/anycube.png", size)));
    }
    Ok(out)
}

#[cfg(unix)]
fn executable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
}

#[cfg(not(unix))]
fn executable(_: &Path) -> io::Result<()> {
    Ok(())
}

/// A string inside single quotes for a shell.
fn quoted(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn add_macos(binary: &Path, bundle: &Path) -> io::Result<()> {
    let _ = fs::remove_dir_all(bundle);
    let (macos, resources) = (bundle.join("Contents/MacOS"), bundle.join("Contents/Resources"));
    fs::create_dir_all(&macos)?;
    fs::create_dir_all(&resources)?;
    fs::write(resources.join("window.js"), window::WINDOW_JS)?;

    // The icon set: the sizes macOS asks for, each also as the double-density of the size below.
    let set = resources.join("anycube.iconset");
    fs::create_dir_all(&set)?;
    let png = |size: u32| assets::ICONS.iter().find(|i| i.0 == size).map(|i| i.1).expect("an icon of every size");
    for size in [16u32, 32, 128, 256, 512] {
        fs::write(set.join(format!("icon_{0}x{0}.png", size)), png(size))?;
        fs::write(set.join(format!("icon_{0}x{0}@2x.png", size)), png(size * 2))?;
    }
    let made = Command::new("/usr/bin/iconutil").args(["-c", "icns", "-o"]).arg(resources.join("anycube.icns")).arg(&set).status();
    let _ = fs::remove_dir_all(&set);
    if !matches!(made, Ok(s) if s.success()) {
        // Without the tool the entry still works; it shows the system's plain icon.
        eprintln!("note: iconutil could not make the icon set; the entry will have a plain icon");
    }
    fs::write(resources.join("icon.png"), png(512))?;

    // The launcher: the system's script host becomes the application (the
    // window is its own), and starts the server from the installed program.
    let launcher = macos.join("anycube");
    fs::write(
        &launcher,
        format!(
            "#!/bin/sh\n# Made by `anycube app --install`. Opens the interface in a window of its own.\nhere=$(cd \"$(dirname \"$0\")/../Resources\" && pwd)\nexec /usr/bin/osascript -l JavaScript \"$here/window.js\" --serve {} \"$here/icon.png\"\n",
            quoted(&binary.to_string_lossy())
        ),
    )?;
    executable(&launcher)?;
    fs::write(
        bundle.join("Contents/Info.plist"),
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>anycube</string>
  <key>CFBundleDisplayName</key><string>anycube</string>
  <key>CFBundleIdentifier</key><string>local.anycube.app</string>
  <key>CFBundleVersion</key><string>{version}</string>
  <key>CFBundleShortVersionString</key><string>{version}</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleExecutable</key><string>anycube</string>
  <key>CFBundleIconFile</key><string>anycube</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>LSArchitecturePriority</key><array><string>arm64</string><string>x86_64</string></array>
  <key>LSApplicationCategoryType</key><string>public.app-category.puzzle-games</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSCameraUsageDescription</key><string>anycube reads the colours of a cube you hold up to the camera. The picture stays on this computer.</string>
</dict>
</plist>
"#,
            version = env!("CARGO_PKG_VERSION")
        ),
    )?;
    // The system notices a new bundle sooner when told about it.
    let _ = Command::new("/usr/bin/touch").arg(bundle).status();
    Ok(())
}

fn add_linux(binary: &Path, places: &[PathBuf]) -> io::Result<()> {
    for (place, (_, png)) in places[1..].iter().zip(assets::ICONS.iter().filter(|i| i.0 <= 512)) {
        fs::create_dir_all(place.parent().expect("a file in a directory"))?;
        fs::write(place, png)?;
    }
    let desktop = &places[0];
    fs::create_dir_all(desktop.parent().expect("a file in a directory"))?;
    // In a desktop entry a path with spaces is quoted, and a percent sign doubled.
    let exec = format!("\"{}\" app", binary.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"").replace('%', "%%"));
    fs::write(
        desktop,
        format!(
            "[Desktop Entry]\nType=Application\nName=anycube\nGenericName=Cube solver\nComment=Solve and play with Rubik's cubes of any size\nExec={}\nIcon=anycube\nTerminal=false\nCategories=Game;LogicGame;\nStartupWMClass=anycube\n",
            exec
        ),
    )?;
    // Menus that keep a cache pick the entry up at once when asked to; without the tool they do so in their own time.
    if let Some(dir) = desktop.parent() {
        let _ = Command::new("update-desktop-database").arg(dir).status();
    }
    Ok(())
}

/// Adds the entry, started by the program at `binary`. Returns what was written.
pub fn add(binary: &Path) -> io::Result<Vec<PathBuf>> {
    let places = places()?;
    if cfg!(target_os = "macos") {
        add_macos(binary, &places[0])?;
    } else if cfg!(target_os = "linux") {
        add_linux(binary, &places)?;
    } else {
        return Err(io::Error::new(io::ErrorKind::Unsupported, "an application entry is made on macOS and Linux only, for now"));
    }
    Ok(places)
}

/// Takes the entry away. Returns what was there and is gone.
pub fn remove() -> io::Result<Vec<PathBuf>> {
    let mut gone = Vec::new();
    for place in places()? {
        let removed = if place.is_dir() { fs::remove_dir_all(&place) } else { fs::remove_file(&place) };
        if removed.is_ok() {
            gone.push(place);
        }
    }
    Ok(gone)
}
