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

use super::assets;
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

#[cfg(target_os = "macos")]
fn add_macos(binary: &Path, bundle: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fn checked(command: &mut Command) -> io::Result<()> {
        let status = command.status()?;
        if !status.success() {
            return Err(io::Error::other(format!("{command:?} exited with {status}")));
        }
        Ok(())
    }

    let parent = bundle.parent().expect("bundle install directory");
    fs::create_dir_all(parent)?;
    let temp = Command::new("/usr/bin/mktemp")
        .arg("-d").arg(parent.join(".anycube.XXXXXXXX")).output()?;
    if !temp.status.success() {
        return Err(io::Error::other("could not create app staging directory"));
    }
    let staging = PathBuf::from(String::from_utf8(temp.stdout)
        .map_err(io::Error::other)?.trim_end_matches('\n'));
    let result = (|| {
        let staged = staging.join("anycube.app");
        let macos = staged.join("Contents/MacOS");
        let resources = staged.join("Contents/Resources");
        fs::create_dir_all(&macos)?;
        fs::create_dir_all(&resources)?;
        let launcher = macos.join("anycube");
        fs::write(&launcher, include_bytes!(concat!(env!("OUT_DIR"), "/anycube-host")))?;
        fs::set_permissions(&launcher, fs::Permissions::from_mode(0o755))?;
        let server = macos.join("anycube-server");
        fs::copy(binary, &server)?;
        checked(Command::new("/usr/bin/codesign").args(["--force", "--sign", "-", "--timestamp=none"])
            .arg(&server))?;

        fs::write(staged.join("Contents/Info.plist"),
            include_str!("../../platform/macos/Info.plist")
                .replace("@VERSION@", env!("CARGO_PKG_VERSION")))?;
        let set = staging.join("anycube.iconset");
        fs::create_dir(&set)?;
        for size in [16u32, 32, 128, 256, 512] {
            for (scale, suffix) in [(1, ""), (2, "@2x")] {
                let png = assets::ICONS.iter().find(|i| i.0 == size * scale).unwrap().1;
                fs::write(set.join(format!("icon_{size}x{size}{suffix}.png")), png)?;
            }
        }
        checked(Command::new("/usr/bin/iconutil").args(["-c", "icns", "-o"])
            .arg(resources.join("anycube.icns")).arg(&set))?;
        checked(Command::new("/usr/bin/codesign").args(["--force", "--sign", "-", "--timestamp=none"])
            .arg(&staged))?;
        checked(Command::new("/usr/bin/codesign").args(["--verify", "--deep", "--strict"])
            .arg(&staged))?;

        let backup = staging.join("previous-anycube.app");
        let had_previous = match fs::rename(bundle, &backup) {
            Ok(()) => true,
            Err(e) if e.kind() == io::ErrorKind::NotFound => false,
            Err(e) => return Err(e),
        };
        if let Err(error) = fs::rename(&staged, bundle) {
            if had_previous {
                fs::rename(&backup, bundle).map_err(|restore| io::Error::other(
                    format!("{error}; previous app remains at {} ({restore})", backup.display())
                ))?;
            }
            return Err(error);
        }
        if had_previous {
            eprintln!("Previous app saved at {}", backup.display());
        }
        Ok(())
    })();
    // Keep the previous bundle available for rollback, including on a failed restore.
    let _ = fs::remove_dir_all(staging.join("anycube.app"));
    let _ = fs::remove_dir_all(staging.join("anycube.iconset"));
    let _ = fs::remove_dir(&staging);
    result
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
    #[cfg(target_os = "macos")]
    add_macos(binary, &places[0])?;
    if cfg!(target_os = "linux") {
        add_linux(binary, &places)?;
    } else if !cfg!(target_os = "macos") {
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
