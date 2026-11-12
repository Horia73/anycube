//! The interface in a window of its own, not as a tab among the tabs of a
//! browser.
//!
//! - macOS: a Cocoa window with the system's WebKit in it, driven by the
//!   system's own `osascript` (`window.js`): nothing to install.
//! - Linux, and macOS if that fails: a Chromium-family browser, if one is
//!   installed, started as an application window with a profile of its own.
//! - Otherwise: the default browser, in a tab.

use super::assets;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The script that makes the window on macOS.
pub const WINDOW_JS: &str = include_str!("window.js");

/// How the interface was opened.
pub enum Opened {
    /// A window of its own; the program should end when this process does.
    Window(std::process::Child),
    /// A tab of the default browser; the server runs until it is stopped.
    Tab,
    /// Nothing could be opened; the address has to be opened by hand.
    Nothing,
}

/// The directory for this run's small files (the window script, the icon).
fn scratch_dir() -> PathBuf {
    std::env::temp_dir().join(format!("anycube-{}", std::process::id()))
}

fn scratch() -> std::io::Result<PathBuf> {
    let dir = scratch_dir();
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Removes this run's small files, once the window that used them is closed.
pub fn tidy() {
    let _ = std::fs::remove_dir_all(scratch_dir());
}

/// Browsers that can show a page as an application window.
fn chromium() -> Option<PathBuf> {
    let mac = [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
        "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
    ];
    if let Some(p) = mac.iter().map(Path::new).find(|p| p.is_file()) {
        return Some(p.to_path_buf());
    }
    let names = ["google-chrome", "google-chrome-stable", "chromium", "chromium-browser", "brave-browser", "microsoft-edge"];
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).flat_map(|dir| names.iter().map(move |n| dir.join(n))).find(|p| p.is_file())
}

pub fn open(url: &str, data_dir: &Path) -> Opened {
    if cfg!(target_os = "macos") {
        if let Ok(dir) = scratch() {
            let (script, icon) = (dir.join("window.js"), dir.join("icon.png"));
            let png = assets::ICONS.iter().find(|i| i.0 == 512).expect("the icon at 512").1;
            if std::fs::write(&script, WINDOW_JS).is_ok() && std::fs::write(&icon, png).is_ok() {
                let child = Command::new("/usr/bin/osascript")
                    .args(["-l", "JavaScript"])
                    .arg(&script)
                    .arg(url)
                    .arg(&icon)
                    .stdin(Stdio::null())
                    .spawn();
                if let Ok(child) = child {
                    return Opened::Window(child);
                }
            }
        }
    }
    if let Some(browser) = chromium() {
        // A profile of its own makes the browser a process of its own, which ends with the window.
        let profile = data_dir.join("window-profile");
        let child = Command::new(browser)
            .arg(format!("--app={}", url))
            .arg(format!("--user-data-dir={}", profile.display()))
            .args(["--no-first-run", "--no-default-browser-check", "--window-size=1280,820"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        if let Ok(child) = child {
            return Opened::Window(child);
        }
    }
    open_tab(url)
}

pub fn open_tab(url: &str) -> Opened {
    if open_default_browser(url) {
        Opened::Tab
    } else {
        Opened::Nothing
    }
}

#[cfg(not(windows))]
fn open_default_browser(url: &str) -> bool {
    let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    Command::new(opener)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(windows)]
fn open_default_browser(url: &str) -> bool {
    use std::ffi::c_void;
    use std::ptr::{null, null_mut};

    #[link(name = "shell32")]
    extern "system" {
        fn ShellExecuteW(
            window: *mut c_void,
            operation: *const u16,
            file: *const u16,
            parameters: *const u16,
            directory: *const u16,
            show: i32,
        ) -> isize;
    }

    let url: Vec<u16> = url.encode_utf16().chain([0]).collect();
    // The server supplies a loopback URL. Its wide string is NUL-terminated,
    // and ShellExecuteW uses the default handler for its scheme.
    const SW_SHOWNORMAL: i32 = 1;
    (unsafe { ShellExecuteW(null_mut(), null(), url.as_ptr(), null(), null(), SW_SHOWNORMAL) }) > 32
}
