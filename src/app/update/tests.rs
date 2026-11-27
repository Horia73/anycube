use super::{package, Offer, State, Update};
use serde_json::{json, Value};
use std::fs;
use std::io::{self, Cursor};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{mpsc, Mutex, RwLock};
use std::time::Duration;

const CHANNEL: &str = "robots-v1";
const ARCH: &str = "arm64";
const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "anycube-updater-test-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

fn entry(tar: &mut Vec<u8>, name: &str, kind: u8, data: &[u8]) {
    let mut h = [0; 512];
    h[..name.len()].copy_from_slice(name.as_bytes());
    h[100..108].copy_from_slice(b"0000644\0");
    h[124..136].copy_from_slice(format!("{:011o}\0", data.len()).as_bytes());
    h[148..156].fill(b' ');
    h[156] = kind;
    h[257..265].copy_from_slice(b"ustar\x0000");
    let sum: u32 = h.iter().map(|&b| u32::from(b)).sum();
    h[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
    tar.extend(h);
    tar.extend(data);
    tar.resize(tar.len().next_multiple_of(512), 0);
}

fn fixture(version: &str, build: u64, arch: &str) -> Vec<u8> {
    let mut tar = Vec::new();
    for name in ["anycube.app", "anycube.app/Contents", "anycube.app/Contents/MacOS", "anycube.app/Contents/Resources"] {
        entry(&mut tar, name, b'5', b"");
    }
    for name in [package::BACKEND, package::LAUNCHER, "Contents/Info.plist"] {
        entry(&mut tar, &format!("anycube.app/{name}"), b'0', b"fixture");
    }
    let manifest = json!({"architecture": arch, "build": build, "channel": CHANNEL, "platform": "macos", "version": version});
    entry(&mut tar, &format!("anycube.app/{}", package::MANIFEST), b'0', manifest.to_string().as_bytes());
    tar
}

fn download(root: &Path, tar: &[u8]) -> (PathBuf, Offer) {
    let raw = root.join("fixture.tar");
    fs::write(&raw, tar).unwrap();
    let gzip = Command::new("gzip").arg("-c").arg(raw).output().unwrap();
    assert!(gzip.status.success());
    let path = root.join("download.tar.gz");
    fs::write(&path, &gzip.stdout).unwrap();
    let offer = release_offer(&path, gzip.stdout.len());
    (path, offer)
}

fn release_offer(path: &Path, size: usize) -> Offer {
    let sum = if cfg!(target_os = "macos") {
        Command::new("shasum").args(["-a", "256"]).arg(path).output().unwrap()
    } else {
        Command::new("sha256sum").arg(path).output().unwrap()
    };
    assert!(sum.status.success());
    let digest = String::from_utf8(sum.stdout).unwrap();
    let mut release = json!({"tag_name": "v0.2.0", "draft": false, "prerelease": false, "assets": [{
        "name": "anycube-desktop-macos-arm64-robots-v1-build-42.tar.gz",
        "state": "uploaded", "id": 101, "size": size, "digest": format!("sha256:{}", digest.split_whitespace().next().unwrap())
    }]});
    let offer = package::select(&release, ARCH, CHANNEL, 41, "0.2.0").unwrap();
    assert_eq!(offer.build, 42);
    assert!(package::select(&release, ARCH, CHANNEL, 42, "0.2.0").is_none());
    assert!(package::select(&release, ARCH, CHANNEL, 41, "0.2.1").is_none());
    assert!(package::select(&release, "x86_64", CHANNEL, 41, "0.1.3").is_none());
    assert!(package::select(&release, ARCH, "stable", 41, "0.1.3").is_none());
    release["assets"][0]["digest"] = Value::Null;
    assert!(package::select(&release, ARCH, CHANNEL, 41, "0.1.3").is_none());
    offer
}

#[test]
fn rejects_malformed_archives_and_mismatched_downloads() {
    let valid = fixture("0.2.0", 42, ARCH);
    for (name, kind) in [
        ("anycube.app/Contents/../../escape", b'0'),
        ("/tmp/escape", b'0'),
        ("anycube.app/Contents/link", b'2'),
        ("anycube.app/Contents/hard", b'1'),
        ("anycube.app/Contents/device", b'3'),
        ("anycube.app/Contents/pax", b'x'),
        ("anycube.app/Contents/MacOS/ANYCUBE", b'0'),
        ("anycube.app/Contents/Unknown/file", b'0'),
    ] {
        let temp = Scratch::new();
        let mut archive = valid.clone();
        entry(&mut archive, name, kind, b"");
        archive.extend([0; 1024]);
        assert!(package::unpack(Cursor::new(archive), &temp.0).is_err(), "{name}");
    }
    let temp = Scratch::new();
    let mut tar = fixture("0.2.0", 42, "x86_64");
    tar.extend([0; 1024]);
    let (archive, mut offer) = download(&temp.0, &tar);
    assert!(package::stage(&archive, &temp.0, &offer, ARCH, CHANNEL).unwrap_err().to_string().contains("identity"));
    offer.digest = "0".repeat(64);
    assert!(package::stage(&archive, &temp.0, &offer, ARCH, CHANNEL).unwrap_err().to_string().contains("checksum"));
    fs::write(&archive, b"truncated").unwrap();
    assert!(package::stage(&archive, &temp.0, &offer, ARCH, CHANNEL).unwrap_err().to_string().contains("size"));
}

struct Stub {
    fail_reopen: bool,
    calls: Mutex<Vec<PathBuf>>,
}

impl package::Executor for Stub {
    fn exchange(&self, installed: &Path, staged: &Path) -> io::Result<()> {
        // Model the Darwin exchange at the filesystem boundary.
        let before = installed.with_extension("previous");
        fs::rename(installed, &before)?;
        fs::rename(staged, installed)?;
        fs::rename(before, staged)
    }

    fn reopen(&self, installed: &Path) -> io::Result<()> {
        assert!(installed.join(package::BACKEND).is_file());
        assert!(!installed.join("old").exists());
        self.calls.lock().unwrap().push(installed.to_path_buf());
        if self.fail_reopen { Err(io::Error::other("stub launch failure")) } else { Ok(()) }
    }
}

fn updater() -> Update {
    Update {
        token: TOKEN.into(),
        state: Mutex::new(State::default()),
        work: RwLock::default(),
        #[cfg(target_os = "macos")]
        bundle: PathBuf::new(),
    }
}

#[test]
fn accepts_exact_offer_and_reopens_same_app_or_restores_previous_app() {
    for fail_reopen in [false, true] {
        let temp = Scratch::new();
        let mut tar = fixture("0.2.0", 42, ARCH);
        tar.extend([0; 1024]);
        let packaging = temp.0.join("packaging");
        fs::create_dir(&packaging).unwrap();
        let source = package::unpack(Cursor::new(tar), &packaging).unwrap();
        let packaged = Command::new("python3")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/package-desktop-update.py"))
            .arg(source).arg(&temp.0).output().unwrap();
        assert!(packaged.status.success(), "{}", String::from_utf8_lossy(&packaged.stderr));
        let archive = PathBuf::from(std::str::from_utf8(&packaged.stdout).unwrap().trim());
        let offer = release_offer(&archive, fs::metadata(&archive).unwrap().len() as usize);
        let installed = temp.0.join("anycube.app");
        let work = temp.0.join("work");
        fs::create_dir(&installed).unwrap();
        fs::write(installed.join("old"), b"previous app").unwrap();
        fs::create_dir(&work).unwrap();
        let settings = temp.0.join("settings.json");
        fs::write(&settings, b"saved settings and robot calibration").unwrap();
        let update = updater();
        let stub = Stub { fail_reopen, calls: Mutex::default() };
        let check = || Ok(Some(offer.clone()));
        let idle = || false;
        let never = |_: &Offer| panic!("installation was not authorized");
        let (status, result) = update.request_with("check", &json!({}), idle, check, never);
        assert_eq!(status, 200);
        assert_eq!(result["phase"], "available");
        assert_eq!(update.request_with("install", &json!({"id": "100", "url": "https://example.invalid"}), idle, check, never).0, 409);
        let accepted = json!({"id": result["offer"]["id"]});
        let (status, rejected) = update.request_with("install", &accepted, || true, check, never);
        assert_eq!(status, 409);
        assert_eq!(rejected["phase"], "error");
        assert_eq!(update.request_with("install", &accepted, idle, check, never).0, 409);
        let retry = json!({"id": rejected["offer"]["id"]});
        let (_, result) = update.request_with("install", &retry, idle, check, |offer| {
            let staged = package::stage(&archive, &work, offer, ARCH, CHANNEL).unwrap();
            let pending = work.join("activation-pending");
            assert!(update.activate_staged(&stub, &installed, &staged, &pending, || true).is_err());
            assert!(!pending.exists());
            assert!(stub.calls.lock().unwrap().is_empty());
            update.activate_staged(&stub, &installed, &staged, &pending, idle).map_err(|(message, _)| message)
        });
        assert_eq!(result["phase"], if fail_reopen { "error" } else { "restarting" });
        assert_eq!(*stub.calls.lock().unwrap(), [installed.clone()]);
        assert_eq!(installed.join("old").exists(), fail_reopen);
        assert_eq!(fs::read(settings).unwrap(), b"saved settings and robot calibration");
    }
}

#[test]
fn a_cancelled_attempt_cannot_activate_and_can_be_accepted_again() {
    let update = updater();
    let temp = Scratch::new();
    let (_, offer) = download(&temp.0, &[0; 1024]);
    let check = || Ok(Some(offer.clone()));
    let never = |_: &Offer| panic!("unexpected installation");
    let (_, result) = update.request_with("check", &json!({}), || false, check, never);
    let accepted = json!({"id": result["offer"]["id"]});
    let pending = temp.0.join("activation-pending");
    let stub = Stub { fail_reopen: false, calls: Mutex::default() };
    let (started, progress) = mpsc::channel();
    let (cancelled, signal) = mpsc::channel();
    std::thread::scope(|scope| {
        let update_ref = &update;
        let acceptance = &accepted;
        let marker = &pending;
        let root = &temp.0;
        let executor = &stub;
        let worker = scope.spawn(move || update_ref.request_with("install", acceptance, || false, check, |_| {
            started.send(()).unwrap();
            signal.recv_timeout(Duration::from_secs(5)).unwrap();
            update_ref.activate_staged(executor, &root.join("current"), &root.join("staged"), marker, || false)
                .map_err(|(message, _)| message)
        }));
        progress.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(update.request_with("install", &accepted, || false, check, never).0, 409);
        assert_eq!(update.request_with("later", &json!({}), || false, check, never).1["phase"], "cancelling");
        cancelled.send(()).unwrap();
        assert_eq!(worker.join().unwrap().1["phase"], "cancelled");
    });
    assert_eq!(update.request_with("install", &accepted, || false, check, never).0, 409);
    assert!(!pending.exists());
    assert!(stub.calls.lock().unwrap().is_empty());
    let (_, current) = update.request_with("status", &json!({}), || false, check, never);
    let retry = json!({"id": current["offer"]["id"]});
    assert_eq!(update.request_with("install", &retry, || false, check, |_| Ok(())).1["phase"], "restarting");
}

#[test]
fn requires_native_capability_exact_origin_and_json() {
    let origin = "http://127.0.0.1:8080";
    let update = updater();
    assert_eq!(update.launch_url(&format!("{origin}/")), format!("{origin}/?window#update={TOKEN}"));
    for (headers, allowed) in [
        (vec![("Origin", origin), ("X-Anycube-Desktop", TOKEN), ("Content-Type", "application/json")], true),
        (vec![("X-Anycube-Desktop", TOKEN), ("Content-Type", "application/json")], false),
        (vec![("Origin", "https://example.invalid"), ("X-Anycube-Desktop", TOKEN), ("Content-Type", "application/json")], false),
        (vec![("Origin", origin), ("Content-Type", "application/json")], false),
        (vec![("Origin", origin), ("X-Anycube-Desktop", "wrong"), ("Content-Type", "application/json")], false),
        (vec![("Origin", origin), ("X-Anycube-Desktop", TOKEN), ("Content-Type", "text/plain")], false),
        (vec![("Origin", origin), ("Origin", "null"), ("X-Anycube-Desktop", TOKEN), ("Content-Type", "application/json")], false),
    ] {
        let headers: Vec<_> = headers.into_iter().map(|(field, value)| tiny_http::Header::from_bytes(field, value).unwrap()).collect();
        assert_eq!(update.authorized(&headers, origin), allowed);
    }
}
