//! The interface in a window of its own, not as a tab among the tabs of a
//! browser.
//!
//! - A Chromium-family browser, if one is installed (Brave, Edge, Chromium,
//!   Chrome), started as an application window with a profile of its own
//!   under anycube's data. On macOS too: there the camera (an iPhone as a
//!   Continuity Camera, say) works in such a window and not in the system's
//!   WebKit view, which this program can only drive through `osascript`,
//!   where the page's request for the camera cannot be answered.
//! - macOS without such a browser: a Cocoa window with the system's WebKit
//!   in it, driven by the system's own `osascript` (`window.js`). Nothing
//!   to install, and no camera.
//! - Otherwise: the default browser, in a tab.
//!
//! `choice` says which; `anycube app` and the entry in Applications both
//! ask it (the entry's launcher through `anycube app --window-kind`).

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

/// The browsers that can show a page as an application window, where macOS
/// keeps them, in the order they are tried: one that lists a Continuity
/// Camera (an iPhone) among the cameras before one that does not.
///
/// Google Chrome is last because it does not list it: seen on macOS with
/// Chrome 158 and an iPhone 15 Pro, in an application window with a profile
/// of its own, `navigator.mediaDevices.enumerateDevices()` gave only the
/// "FaceTime HD Camera" (and the iPhone's microphone, but not its camera),
/// where Brave gave both cameras and `getUserMedia` worked. Edge is reported
/// to do as Brave does (not seen here); Chromium was not tried, and stands
/// before Chrome since the limit is known of Chrome.
const MAC_BROWSERS: [&str; 4] = [
    "Brave Browser.app/Contents/MacOS/Brave Browser",
    "Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
    "Chromium.app/Contents/MacOS/Chromium",
    "Google Chrome.app/Contents/MacOS/Google Chrome",
];

/// The first of `MAC_BROWSERS` that `is_file` finds, in /Applications or in
/// the user's own Applications.
fn mac_browser(home: Option<&Path>, is_file: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    let mut roots = vec![PathBuf::from("/Applications")];
    roots.extend(home.map(|h| h.join("Applications")));
    MAC_BROWSERS.iter().flat_map(|b| roots.iter().map(move |r| r.join(b))).find(|p| is_file(p))
}

/// Browsers that can show a page as an application window.
fn chromium() -> Option<PathBuf> {
    // A test (or someone with a browser elsewhere) says which.
    if let Some(p) = std::env::var_os("ANYCUBE_BROWSER").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p)).filter(|p| p.is_file());
    }
    if cfg!(target_os = "macos") {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        return mac_browser(home.as_deref(), |p| p.is_file());
    }
    let names = ["google-chrome", "google-chrome-stable", "chromium", "chromium-browser", "brave-browser", "microsoft-edge"];
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).flat_map(|dir| names.iter().map(move |n| dir.join(n))).find(|p| p.is_file())
}

/// What the interface is opened in.
#[derive(Debug, PartialEq)]
pub enum Kind {
    /// An application window of this Chromium-family browser.
    Chromium(PathBuf),
    /// The system's WebKit in a Cocoa window (macOS).
    WebKit,
    /// A tab of the default browser.
    Tab,
}

impl Kind {
    /// The word `anycube app --window-kind` prints.
    pub fn word(&self) -> &'static str {
        match self {
            Kind::Chromium(_) => "chromium",
            Kind::WebKit => "webkit",
            Kind::Tab => "tab",
        }
    }
}

fn kind_for(browser: Option<PathBuf>, macos: bool) -> Kind {
    match browser {
        Some(b) => Kind::Chromium(b),
        None if macos => Kind::WebKit,
        None => Kind::Tab,
    }
}

/// What `open` would open the interface in, here and now.
pub fn choice() -> Kind {
    kind_for(chromium(), cfg!(target_os = "macos"))
}

/// The address the page has in a window that is the application: the page
/// tells the server when such a window is closed (see `server`'s
/// `/api/window`), which a page in anyone's browser tab must not.
pub fn window_url(url: &str) -> String {
    format!("{}?window", url)
}

/// What a page's address is to a browser's permissions: "http://127.0.0.1:4321".
fn origin(url: &str) -> &str {
    let after = url.find("://").map_or(0, |i| i + 3);
    url[after..].find(['/', '?', '#']).map_or(url, |i| &url[..after + i])
}

/// The browser's arguments for an application window on `url`. A profile of
/// its own makes the browser a process of its own, apart from the user's.
pub fn chromium_args(url: &str, profile: &Path) -> Vec<String> {
    vec![
        format!("--app={}", window_url(url)),
        format!("--user-data-dir={}", profile.display()),
        "--no-first-run".into(),
        "--no-default-browser-check".into(),
        "--window-size=1280,820".into(),
    ]
}

/// The profile's preferences with the camera allowed for `origin` and for
/// nothing else: the browser's own record of "Allow", as if it had been
/// clicked, so that the page is not asked at every start (the port, and so
/// the origin, is another each time). Everything else in `was` is kept.
fn camera_allowed(was: &str, origin: &str) -> String {
    use serde_json::{json, Map, Value};
    let mut root = serde_json::from_str::<Value>(was).ok().filter(Value::is_object).unwrap_or_else(|| json!({}));
    let mut at = &mut root;
    for key in ["profile", "content_settings", "exceptions"] {
        let map = at.as_object_mut().expect("an object, by the lines below");
        let next = map.entry(key).or_insert_with(|| json!({}));
        if !next.is_object() {
            *next = Value::Object(Map::new());
        }
        at = next;
    }
    at["media_stream_camera"] = json!({ format!("{},*", origin): { "setting": 1 } });
    root.to_string()
}

/// Writes that into the profile, before the browser is started on it.
fn allow_camera(profile: &Path, url: &str) -> std::io::Result<()> {
    let dir = profile.join("Default");
    std::fs::create_dir_all(&dir)?;
    let file = dir.join("Preferences");
    let was = std::fs::read_to_string(&file).unwrap_or_default();
    std::fs::write(file, camera_allowed(&was, origin(url)))
}

fn open_chromium(browser: &Path, url: &str, data_dir: &Path) -> Option<std::process::Child> {
    let profile = data_dir.join("window-profile");
    if let Err(e) = allow_camera(&profile, url) {
        // The window still opens; the browser asks for the camera itself.
        eprintln!("note: the camera could not be allowed beforehand ({}); the window will ask", e);
    }
    Command::new(browser).args(chromium_args(url, &profile)).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().ok()
}

fn open_webkit(url: &str) -> Option<std::process::Child> {
    let dir = scratch().ok()?;
    let (script, icon) = (dir.join("window.js"), dir.join("icon.png"));
    let png = assets::ICONS.iter().find(|i| i.0 == 512).expect("the icon at 512").1;
    std::fs::write(&script, WINDOW_JS).ok()?;
    std::fs::write(&icon, png).ok()?;
    Command::new("/usr/bin/osascript").args(["-l", "JavaScript"]).arg(&script).arg(url).arg(&icon).stdin(Stdio::null()).spawn().ok()
}

pub fn open(url: &str, data_dir: &Path) -> Opened {
    let kind = choice();
    if let Kind::Chromium(browser) = &kind {
        if let Some(child) = open_chromium(browser, url, data_dir) {
            return Opened::Window(child);
        }
    }
    if cfg!(target_os = "macos") {
        if let Some(child) = open_webkit(url) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn the_browser_is_started_as_a_window_with_a_profile_of_its_own() {
        let args = chromium_args("http://127.0.0.1:4321/", Path::new("/Users/h/Library/Application Support/anycube/window-profile"));
        assert_eq!(
            args,
            [
                "--app=http://127.0.0.1:4321/?window",
                "--user-data-dir=/Users/h/Library/Application Support/anycube/window-profile",
                "--no-first-run",
                "--no-default-browser-check",
                "--window-size=1280,820",
            ]
        );
        // Nothing that lets any page have the camera without being asked.
        assert!(!args.iter().any(|a| a.contains("fake") || a.contains("disable-web-security")));
    }

    #[test]
    fn the_origin_of_an_address() {
        assert_eq!(origin("http://127.0.0.1:4321/"), "http://127.0.0.1:4321");
        assert_eq!(origin("http://127.0.0.1:4321/?window"), "http://127.0.0.1:4321");
        assert_eq!(origin("http://127.0.0.1:4321"), "http://127.0.0.1:4321");
    }

    #[test]
    fn the_camera_is_allowed_for_this_address_only_and_the_rest_is_kept() {
        // A new profile.
        let fresh: Value = serde_json::from_str(&camera_allowed("", "http://127.0.0.1:4321")).unwrap();
        assert_eq!(fresh, json!({"profile": {"content_settings": {"exceptions": {"media_stream_camera": {"http://127.0.0.1:4321,*": {"setting": 1}}}}}}));
        // One the browser has written: last time's port goes, everything else stays.
        let was = json!({
            "browser": {"app_window_placement": {"_127.0.0.1": {"left": 10}}},
            "profile": {"name": "x", "content_settings": {"exceptions": {
                "media_stream_camera": {"http://127.0.0.1:1111,*": {"setting": 1, "last_modified": "1"}},
                "media_stream_mic": {"http://127.0.0.1:1111,*": {"setting": 2}},
            }}},
        });
        let now: Value = serde_json::from_str(&camera_allowed(&was.to_string(), "http://127.0.0.1:2222")).unwrap();
        assert_eq!(now["browser"], was["browser"]);
        assert_eq!(now["profile"]["name"], "x");
        let exceptions = &now["profile"]["content_settings"]["exceptions"];
        assert_eq!(exceptions["media_stream_mic"], was["profile"]["content_settings"]["exceptions"]["media_stream_mic"]);
        assert_eq!(exceptions["media_stream_camera"], json!({"http://127.0.0.1:2222,*": {"setting": 1}}));
        // A file that is not what it should be is no reason to fail.
        for broken in ["not json", "[1]", r#"{"profile": 3}"#, r#"{"profile": {"content_settings": {"exceptions": []}}}"#] {
            let v: Value = serde_json::from_str(&camera_allowed(broken, "http://127.0.0.1:1")).unwrap();
            assert_eq!(v["profile"]["content_settings"]["exceptions"]["media_stream_camera"]["http://127.0.0.1:1,*"]["setting"], 1, "{}", broken);
        }
    }

    #[test]
    fn a_browser_before_the_webkit_window_and_chrome_after_the_others() {
        let home = Path::new("/Users/h");
        const CHROME: &str = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
        const EDGE: &str = "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge";
        const BRAVE: &str = "/Users/h/Applications/Brave Browser.app/Contents/MacOS/Brave Browser";
        const CHROMIUM: &str = "/Applications/Chromium.app/Contents/MacOS/Chromium";
        let found = |home: Option<&Path>, installed: &[&str]| mac_browser(home, |p| installed.iter().any(|i| p == Path::new(i)));
        let chrome = CHROME;
        // Those that list a Continuity Camera first: Brave, Edge, Chromium, and Chrome last.
        assert_eq!(found(Some(home), &[CHROME, CHROMIUM, EDGE, BRAVE]), Some(PathBuf::from(BRAVE)));
        assert_eq!(found(Some(home), &[CHROME, CHROMIUM, EDGE]), Some(PathBuf::from(EDGE)));
        assert_eq!(found(Some(home), &[CHROME, CHROMIUM]), Some(PathBuf::from(CHROMIUM)));
        assert_eq!(found(Some(home), &[CHROME]), Some(PathBuf::from(CHROME)));
        // The user's own Applications is looked in only when the home is known.
        assert_eq!(found(None, &[BRAVE]), None);
        assert_eq!(found(None, &[BRAVE, CHROME]), Some(PathBuf::from(CHROME)));
        assert_eq!(found(Some(home), &[]), None);
        // With a browser: its window, on any system. Without: WebKit on macOS, a tab elsewhere.
        assert_eq!(kind_for(Some(PathBuf::from(chrome)), true), Kind::Chromium(PathBuf::from(chrome)));
        assert_eq!(kind_for(Some(PathBuf::from("/usr/bin/chromium")), false).word(), "chromium");
        assert_eq!(kind_for(None, true).word(), "webkit");
        assert_eq!(kind_for(None, false).word(), "tab");
    }
}
