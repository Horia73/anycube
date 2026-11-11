//! The interface's files, built into the program: `anycube app` needs
//! nothing on disk beside itself.

pub struct Asset {
    /// The path it is served at, without the leading slash.
    pub path: &'static str,
    pub mime: &'static str,
    pub bytes: &'static [u8],
}

macro_rules! asset {
    ($path:literal, $mime:literal) => {
        Asset { path: $path, mime: $mime, bytes: include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/app/", $path)) }
    };
}

const JS: &str = "text/javascript; charset=utf-8";

/// Every file of `app/` that the page asks for. A test holds the list to
/// the directory, so that a new file is not forgotten here.
pub const ASSETS: &[Asset] = &[
    asset!("index.html", "text/html; charset=utf-8"),
    asset!("app.css", "text/css; charset=utf-8"),
    asset!("favicon.svg", "image/svg+xml"),
    Asset { path: "js/app.js", mime: JS, bytes: include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/app/js/app.js")) },
    Asset { path: "js/camera.js", mime: JS, bytes: include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/app/js/camera.js")) },
    Asset { path: "js/colour.js", mime: JS, bytes: include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/app/js/colour.js")) },
    Asset { path: "js/cube-model.js", mime: JS, bytes: include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/app/js/cube-model.js")) },
    Asset { path: "js/cube-view.js", mime: JS, bytes: include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/app/js/cube-view.js")) },
    Asset { path: "js/main.js", mime: JS, bytes: include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/app/js/main.js")) },
    Asset { path: "js/markup.js", mime: JS, bytes: include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/app/js/markup.js")) },
    Asset { path: "js/paint.js", mime: JS, bytes: include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/app/js/paint.js")) },
    Asset { path: "js/player.js", mime: JS, bytes: include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/app/js/player.js")) },
    Asset { path: "js/scan-fit.js", mime: JS, bytes: include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/app/js/scan-fit.js")) },
    Asset { path: "js/solver.js", mime: JS, bytes: include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/app/js/solver.js")) },
    Asset { path: "js/solver-worker.js", mime: JS, bytes: include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/app/js/solver-worker.js")) },
    Asset { path: "js/wasm-solver.js", mime: JS, bytes: include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/app/js/wasm-solver.js")) },
];

/// The icon, in the sizes an application entry wants (pixels a side).
pub const ICONS: &[(u32, &[u8])] = &[
    (16, include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/app/icon/icon-16.png"))),
    (32, include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/app/icon/icon-32.png"))),
    (64, include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/app/icon/icon-64.png"))),
    (128, include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/app/icon/icon-128.png"))),
    (256, include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/app/icon/icon-256.png"))),
    (512, include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/app/icon/icon-512.png"))),
    (1024, include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/app/icon/icon-1024.png"))),
];

pub fn find(path: &str) -> Option<&'static Asset> {
    let path = if path.is_empty() { "index.html" } else { path };
    ASSETS.iter().find(|a| a.path == path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn every_file_of_the_page_is_built_in() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("app");
        let mut on_disk = vec!["index.html".to_string(), "app.css".to_string(), "favicon.svg".to_string()];
        for entry in std::fs::read_dir(root.join("js")).unwrap() {
            on_disk.push(format!("js/{}", entry.unwrap().file_name().to_string_lossy()));
        }
        on_disk.sort();
        let mut built_in: Vec<String> = ASSETS.iter().map(|a| a.path.to_string()).collect();
        built_in.sort();
        assert_eq!(built_in, on_disk);
        assert!(find("").is_some() && find("js/app.js").is_some() && find("../Cargo.toml").is_none());
        for (size, png) in ICONS {
            assert_eq!(&png[1..4], b"PNG", "icon {}", size);
        }
    }
}
