use super::package::{self, Executor};
use super::{Offer, Update};
use serde_json::{json, Value};
use std::ffi::{c_char, CString};
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{self, Read};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, RwLock};
use std::time::Duration;

const REPO: &str = "Horia73/anycube";
const BUNDLE_ID: &str = "local.anycube.app";
const VERSION: &str = env!("CARGO_PKG_VERSION");
const ARCH: &str = if cfg!(target_arch = "aarch64") { "arm64" } else { std::env::consts::ARCH };

fn identity() -> Result<(&'static str, u64, &'static str), String> {
    let unpublished = || "This build has no published desktop update channel.".to_string();
    let channel = option_env!("ANYCUBE_UPDATE_CHANNEL")
        .filter(|s| !s.is_empty() && s.len() <= 40 && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'))
        .ok_or_else(unpublished)?;
    let build = option_env!("ANYCUBE_UPDATE_BUILD").and_then(|s| s.parse::<u64>().ok())
        .filter(|n| *n > 0).ok_or_else(unpublished)?;
    let team = option_env!("ANYCUBE_UPDATE_TEAM").filter(|s| s.len() == 10 && s.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()))
        .ok_or_else(unpublished)?;
    Ok((channel, build, team))
}

fn curl(seconds: &str, limit: u64) -> Command {
    let mut cmd = Command::new("/usr/bin/curl");
    // Disable .curlrc before other flags. Make the transfer independent of proxy
    // and TLS overrides from the launcher environment.
    cmd.env_clear().args([
        "-q", "--fail", "--silent", "--proto", "=https", "--proto-redir", "=https",
        "--tlsv1.2", "--connect-timeout", "8", "--max-time", seconds, "--max-filesize",
    ]).arg(limit.to_string()).stdin(Stdio::null()).stderr(Stdio::null());
    cmd
}

pub(super) fn platform_check() -> Result<Option<Offer>, String> {
    let (channel, build, _) = identity()?;
    let mut child = curl("15", 1024 * 1024)
        .args(["-H", "Accept: application/vnd.github+json", "-A", "anycube-desktop"])
        .arg(format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .stdout(Stdio::piped()).spawn().map_err(|e| e.to_string())?;
    let mut data = Vec::new();
    let read = child.stdout.take().expect("piped").take(1024 * 1024 + 1).read_to_end(&mut data);
    if read.is_err() || data.len() > 1024 * 1024 {
        let _ = child.kill();
    }
    if !child.wait().map_err(|e| e.to_string())?.success() || read.is_err() || data.len() > 1024 * 1024 {
        return Err("Could not read the official release from GitHub.".into());
    }
    let release: Value = serde_json::from_slice(&data).map_err(|_| "Invalid release information from GitHub.")?;
    if release["tag_name"].as_str().is_none() || release["assets"].as_array().is_none() {
        return Err("Incomplete release information from GitHub.".into());
    }
    Ok(package::select(&release, ARCH, channel, build, VERSION))
}

fn prepare_workspace(bundle: &Path) -> io::Result<(PathBuf, File)> {
    let root = bundle.with_file_name(".anycube-update");
    match DirBuilder::new().mode(0o700).create(&root) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    extern "C" { fn geteuid() -> u32; }
    let meta = fs::symlink_metadata(&root)?;
    if !meta.is_dir() || meta.uid() != unsafe { geteuid() } || meta.mode() & 0o077 != 0 {
        return Err(io::Error::other("Update workspace must be a private directory owned by the current user"));
    }
    let lock_path = root.join("lock");
    if fs::symlink_metadata(&lock_path).is_ok_and(|meta| !meta.is_file()) {
        return Err(io::Error::other("Invalid update lock"));
    }
    let lock = OpenOptions::new().write(true).create(true).truncate(false).mode(0o600).open(lock_path)?;
    lock.try_lock().map_err(|_| io::Error::other("Another anycube instance is updating"))?;
    Ok((root, lock))
}

fn remove_attempt(path: &Path) -> io::Result<()> {
    match fs::remove_dir_all(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

impl Update {
    pub fn desktop() -> Option<Self> {
        let executable = fs::canonicalize(std::env::current_exe().ok()?).ok()?;
        let bundle = executable.ancestors().nth(3)?;
        if bundle.file_name()? != "anycube.app" || executable != bundle.join(package::BACKEND)
            || !bundle.join(package::LAUNCHER).is_file() {
            return None;
        }
        let mut secret = [0; 32];
        File::open("/dev/urandom").ok()?.read_exact(&mut secret).ok()?;
        Some(Self {
            token: secret.iter().map(|byte| format!("{byte:02x}")).collect(),
            bundle: bundle.to_path_buf(),
            state: Mutex::default(),
            work: RwLock::default(),
        })
    }

    pub(super) fn install(&self, offer: &Offer, busy: impl Fn() -> bool) -> Result<(), String> {
        let (channel, build, team) = identity()?;
        let (root, _lock) = prepare_workspace(&self.bundle).map_err(|e| e.to_string())?;
        let attempt = root.join("attempt");
        let pending_swap = attempt.join("activation-pending");
        if pending_swap.exists() {
            return Err(format!("A previous install was interrupted. Inspect the backup at {} before retrying.", attempt.join("anycube.app").display()));
        }
        // Refuse if a second desktop session already installed over this running build.
        let manifest: Value = File::open(self.bundle.join(package::MANIFEST)).ok()
            .and_then(|file| serde_json::from_reader(file.take(4096)).ok()).unwrap_or(Value::Null);
        if manifest != json!({"version": VERSION, "build": build, "platform": "macos", "architecture": ARCH, "channel": channel}) {
            return Err("The installed app identity changed or is incomplete. Reopen anycube before updating.".into());
        }
        remove_attempt(&attempt).and_then(|()| fs::create_dir(&attempt)).map_err(|e| e.to_string())?;
        let mut preserve = false;
        let result = (|| {
            let archive = attempt.join("download.tar.gz");
            self.download(offer, &archive)?;
            {
                let mut state = self.state.lock().unwrap();
                if state.phase == "cancelling" { return Err("Cancelled".into()); }
                state.phase = "verifying";
            }
            let staged = package::stage(&archive, &attempt, offer, ARCH, channel)
                .map_err(|e| format!("Could not verify the desktop package: {e}"))?;
            verify_signature(&staged, team)?;
            if let Err((error, keep_backup)) = self.activate_staged(&Mac, &self.bundle, &staged, &pending_swap, busy) {
                preserve = keep_backup;
                return Err(error);
            }
            Ok(())
        })();
        if !preserve {
            let _ = remove_attempt(&attempt);
        }
        result
    }

    fn download(&self, offer: &Offer, archive: &Path) -> Result<(), String> {
        let url = format!("https://github.com/{REPO}/releases/download/v{}/{}", offer.version, offer.asset);
        let result = self.transfer(offer, archive, &url)?;
        let (status, redirect) = result.split_once('\n').ok_or("Invalid GitHub download response.")?;
        if status == "200" {
            return Ok(());
        }
        // Accept only the official origin and its asset CDN.
        if !matches!(status, "301" | "302" | "303" | "307" | "308")
            || !["https://release-assets.githubusercontent.com/", "https://objects.githubusercontent.com/"]
                .iter().any(|host| redirect.starts_with(host))
            || redirect.bytes().any(|byte| byte.is_ascii_control() || byte == b' ')
        {
            return Err("GitHub returned an unexpected download location.".into());
        }
        if self.transfer(offer, archive, redirect)?.split_once('\n').map(|(code, _)| code) != Some("200") {
            return Err("The official desktop asset could not be downloaded.".into());
        }
        Ok(())
    }

    fn transfer(&self, offer: &Offer, archive: &Path, url: &str) -> Result<String, String> {
        let directory = archive.parent().expect("staging directory");
        let response = directory.join("response");
        let output = File::create(&response).map_err(|e| e.to_string())?;
        let mut child = curl("300", offer.bytes).args([
            "--globoff", "--write-out", "%{http_code}\n%{redirect_url}", "--output",
        ]).current_dir(directory).arg(archive.file_name().expect("archive file"))
            .arg(url).stdout(output).spawn().map_err(|e| e.to_string())?;
        loop {
            let bytes = fs::metadata(archive).map_or(0, |meta| meta.len());
            {
                let mut state = self.state.lock().unwrap();
                state.bytes = bytes;
                if state.phase == "cancelling" || bytes > offer.bytes {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("Download cancelled or exceeded the expected size.".into());
                }
            }
            match child.try_wait() {
                Ok(Some(status)) if status.success() => {
                    let mut text = String::new();
                    File::open(&response).and_then(|file| file.take(8193).read_to_string(&mut text)).map_err(|e| e.to_string())?;
                    if text.len() > 8192 { return Err("Invalid GitHub download response.".into()); }
                    return Ok(text);
                }
                Ok(Some(_)) => return Err("Download interrupted. Try installing again.".into()),
                Ok(None) => std::thread::sleep(Duration::from_millis(150)),
                Err(e) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(e.to_string());
                }
            }
        }
    }

}

fn verify_signature(bundle: &Path, team: &str) -> Result<(), String> {
    let requirement = |id: Option<&str>| {
        let signer = format!("anchor apple generic and certificate leaf[subject.OU] = \"{team}\" and certificate leaf[field.1.2.840.113635.100.6.1.13] exists");
        id.map_or(signer.clone(), |id| format!("identifier \"{id}\" and {signer}"))
    };
    let valid_tree = Command::new("/usr/bin/codesign").env_clear().args(["--verify", "--strict", "--deep"])
        .arg(bundle).output().is_ok_and(|output| output.status.success());
    if !valid_tree { return Err("The desktop package has an invalid Apple code signature.".into()); }
    for (path, requirement) in [(bundle.to_path_buf(), requirement(Some(BUNDLE_ID))), (bundle.join(package::BACKEND), requirement(None))] {
        let valid = Command::new("/usr/bin/codesign").env_clear().args(["--verify", "--strict", "-R"])
            .arg(requirement).arg(path).output().is_ok_and(|output| output.status.success());
        if !valid { return Err("The desktop package did not pass Apple code signature verification.".into()); }
    }
    // Inspect the signed Mach-O files directly. The package contains thin binaries.
    for path in [package::BACKEND, package::LAUNCHER] {
        let mut header = [0; 8];
        File::open(bundle.join(path)).and_then(|mut file| file.read_exact(&mut header)).map_err(|e| e.to_string())?;
        let cpu = if ARCH == "arm64" { 0x0100_000c } else { 0x0100_0007 };
        if header[..4] != [0xcf, 0xfa, 0xed, 0xfe] || u32::from_le_bytes(header[4..].try_into().unwrap()) != cpu {
            return Err("The desktop package uses a different processor architecture.".into());
        }
    }
    Ok(())
}

struct Mac;

impl Executor for Mac {
    fn exchange(&self, installed: &Path, staged: &Path) -> io::Result<()> {
        extern "C" { fn renamex_np(from: *const c_char, to: *const c_char, flags: u32) -> i32; }
        let installed = CString::new(installed.as_os_str().as_bytes())?;
        let staged = CString::new(staged.as_os_str().as_bytes())?;
        const RENAME_SWAP: u32 = 0x0000_0002;
        if unsafe { renamex_np(installed.as_ptr(), staged.as_ptr(), RENAME_SWAP) } == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    fn reopen(&self, installed: &Path) -> io::Result<()> {
        let status = Command::new("/usr/bin/open").arg("-n").arg(installed).status()?;
        if status.success() { Ok(()) } else { Err(io::Error::other("Launch Services rejected the app")) }
    }
}
