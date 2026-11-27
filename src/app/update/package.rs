use super::Offer;
use serde_json::Value;
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const LIMIT: u64 = 192 * 1024 * 1024;
pub const BACKEND: &str = "Contents/MacOS/anycube-server";
pub const LAUNCHER: &str = "Contents/MacOS/anycube";
pub const MANIFEST: &str = "Contents/Resources/desktop-update.json";

fn version(value: &str) -> Option<[u64; 3]> {
    let mut parts = value.split('.');
    let result = [parts.next()?.parse().ok()?, parts.next()?.parse().ok()?, parts.next()?.parse().ok()?];
    (parts.next().is_none() && value == format!("{}.{}.{}", result[0], result[1], result[2])).then_some(result)
}

pub fn select(release: &Value, architecture: &str, channel: &str, build: u64, current: &str) -> Option<Offer> {
    if release["draft"] != false || release["prerelease"] != false {
        return None;
    }
    let tag = release["tag_name"].as_str()?.strip_prefix('v')?;
    if version(tag)? < version(current)? {
        return None;
    }
    let prefix = format!("anycube-desktop-macos-{architecture}-{channel}-build-");
    release["assets"].as_array()?.iter().filter_map(|asset| {
        let name = asset["name"].as_str()?;
        let next: u64 = name.strip_prefix(&prefix)?.strip_suffix(".tar.gz")?.parse().ok()?;
        let bytes = asset["size"].as_u64().filter(|size| (1..=LIMIT).contains(size))?;
        let digest = asset["digest"].as_str()?.strip_prefix("sha256:")?;
        let id = asset["id"].as_u64()?.to_string();
        if next <= build || name != format!("{prefix}{next}.tar.gz") || asset["state"] != "uploaded"
            || digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        Some(Offer {
            id, version: tag.into(), notes: release["body"].as_str().unwrap_or("").chars().take(6000).collect(),
            asset: name.into(), digest: digest.to_ascii_lowercase(), bytes, build: next,
        })
    }).max_by_key(|offer| offer.build)
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn octal(bytes: &[u8]) -> io::Result<u64> {
    let text = std::str::from_utf8(bytes).map_err(|_| invalid("Invalid archive number"))?;
    u64::from_str_radix(text.trim_matches([' ', '\0']), 8).map_err(|_| invalid("Invalid archive number"))
}

fn field(bytes: &[u8]) -> io::Result<&str> {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    if bytes[end..].iter().any(|&b| b != 0) {
        return Err(invalid("Invalid archive name"));
    }
    std::str::from_utf8(&bytes[..end]).map_err(|_| invalid("Invalid archive name"))
}

fn archive_path(header: &[u8; 512], directory: bool) -> io::Result<String> {
    let prefix = field(&header[345..500])?;
    let name = field(&header[..100])?;
    let name = if directory { name.strip_suffix('/').unwrap_or(name) } else { name };
    let path = if prefix.is_empty() { name.into() } else { format!("{prefix}/{name}") };
    if !(path == "anycube.app" && directory || path.starts_with("anycube.app/Contents/") || path == "anycube.app/Contents" && directory)
        || path.split('/').any(|part| {
            part.is_empty() || matches!(part, "." | "..") || part.ends_with(['.', ' '])
                || !part.bytes().all(|b| b.is_ascii_alphanumeric() || b"._- ".contains(&b))
        }) {
        return Err(invalid("Unexpected path in desktop archive"));
    }
    Ok(path)
}

// Only POSIX ustar regular files and directories. Requiring explicit parents keeps
// case collisions, order, and permissions predictable on case-insensitive volumes.
pub fn unpack(mut input: impl Read, root: &Path) -> io::Result<PathBuf> {
    let mut seen = HashMap::new();
    let mut remaining = LIMIT;
    loop {
        let mut header = [0; 512];
        input.read_exact(&mut header)?;
        remaining = remaining.checked_sub(512).ok_or_else(|| invalid("Desktop archive is too large"))?;
        if header == [0; 512] {
            input.read_exact(&mut header)?;
            if header != [0; 512] {
                return Err(invalid("Invalid desktop archive ending"));
            }
            let mut tail = Vec::new();
            input.take(10_241).read_to_end(&mut tail)?;
            if tail.len() > 10_240 || tail.iter().any(|&b| b != 0) {
                return Err(invalid("Unexpected data after desktop archive"));
            }
            break;
        }
        let sum: u64 = header.iter().enumerate().map(|(i, &b)| u64::from(if (148..156).contains(&i) { b' ' } else { b })).sum();
        if &header[257..265] != b"ustar\x0000" || octal(&header[148..156])? != sum || seen.len() >= 2048 {
            return Err(invalid("Invalid desktop archive header"));
        }
        let directory = match header[156] {
            b'5' => true,
            b'0' | 0 => false,
            _ => return Err(invalid("Desktop archive contains an unsupported entry")),
        };
        let name = archive_path(&header, directory)?;
        if let Some((parent, _)) = name.rsplit_once('/') {
            if !seen.get(&parent.to_ascii_lowercase()).is_some_and(|entry| entry == &(parent.to_string(), true)) {
                return Err(invalid("Desktop archive is missing a parent directory"));
            }
        }
        if seen.insert(name.to_ascii_lowercase(), (name.clone(), directory)).is_some() {
            return Err(invalid("Duplicate path in desktop archive"));
        }
        let path = root.join(&name);
        let size = octal(&header[124..136])?;
        if directory {
            if size != 0 { return Err(invalid("Invalid archive directory")); }
            fs::create_dir(&path)?;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
        } else {
            let padded = size.checked_add(511).ok_or_else(|| invalid("Desktop archive is too large"))? / 512 * 512;
            remaining = remaining.checked_sub(padded).ok_or_else(|| invalid("Desktop archive is too large"))?;
            let relative = name.strip_prefix("anycube.app/").unwrap_or("");
            let mode = if matches!(relative, BACKEND | LAUNCHER) { 0o755 } else { 0o644 };
            let mut file = OpenOptions::new().write(true).create_new(true).mode(mode).open(path)?;
            if io::copy(&mut input.by_ref().take(size), &mut file)? != size {
                return Err(invalid("Truncated desktop archive file"));
            }
            input.read_exact(&mut [0; 511][..(padded - size) as usize])?;
            file.set_permissions(fs::Permissions::from_mode(mode))?;
            file.sync_all()?;
        }
    }
    let bundle = root.join("anycube.app");
    for name in [BACKEND, LAUNCHER, MANIFEST, "Contents/Info.plist"] {
        if !bundle.join(name).is_file() {
            return Err(invalid("Desktop archive is missing required bundle files"));
        }
    }
    Ok(bundle)
}

pub fn stage(archive: &Path, root: &Path, offer: &Offer, architecture: &str, channel: &str) -> io::Result<PathBuf> {
    if fs::metadata(archive)?.len() != offer.bytes || offer.bytes > LIMIT {
        return Err(invalid("Download size differs from the official release"));
    }
    let mut hash = if cfg!(target_os = "macos") {
        let mut cmd = Command::new("/usr/bin/shasum");
        cmd.args(["-a", "256"]);
        cmd
    } else {
        Command::new("/usr/bin/sha256sum")
    };
    let output = hash.env_clear().stdin(File::open(archive)?).output()?;
    if !output.status.success() || std::str::from_utf8(&output.stdout).ok().and_then(|text| text.split_whitespace().next()) != Some(&offer.digest) {
        return Err(invalid("Download checksum differs from the official release"));
    }
    let mut gzip = Command::new("/usr/bin/gzip").env_clear().arg("-dc")
        .stdin(File::open(archive)?).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()?;
    let result = unpack(gzip.stdout.take().expect("piped"), root);
    if result.is_err() { let _ = gzip.kill(); }
    let status = gzip.wait()?;
    let bundle = result?;
    if !status.success() { return Err(invalid("Invalid compressed desktop archive")); }
    let manifest_file = File::open(bundle.join(MANIFEST))?;
    if manifest_file.metadata()?.len() > 4096 {
        return Err(invalid("Desktop bundle manifest is too large"));
    }
    let manifest: Value = serde_json::from_reader(manifest_file)?;
    if manifest != serde_json::json!({
        "version": offer.version, "build": offer.build, "platform": "macos", "architecture": architecture, "channel": channel
    }) {
        return Err(invalid("Bundle identity differs from the official release"));
    }
    Ok(bundle)
}

pub trait Executor {
    fn exchange(&self, installed: &Path, staged: &Path) -> io::Result<()>;
    fn reopen(&self, installed: &Path) -> io::Result<()>;
}

// The old bundle remains in staging until Launch Services accepts the reopen.
pub fn activate(executor: &impl Executor, installed: &Path, staged: &Path) -> Result<(), (String, bool)> {
    executor.exchange(installed, staged)
        .map_err(|e| (format!("Could not replace anycube.app: {e}"), false))?;
    if let Err(error) = executor.reopen(installed) {
        return match executor.exchange(installed, staged) {
            Ok(()) => Err((format!("Could not reopen anycube.app. The previous app was restored: {error}"), false)),
            Err(rollback) => Err((format!("Could not reopen anycube.app: {error}. Could not restore the previous app: {rollback}. The backup is at {}.", staged.display()), true)),
        };
    }
    Ok(())
}
